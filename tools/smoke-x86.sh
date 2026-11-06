#!/usr/bin/env bash
# BerryOS x86_64 最小集冒烟（P7-x0 起，ADR #41；P7-x4 五原语收官）。
#
# 独立关卡——不并入主 smoke.sh：主 smoke 的四跑是 ARM64 回归基线，x86 面以
# 独立入口并行存在（加法零扰）。P7-x4 收官起挂进 CI 面（ci.yml：cargo 直调
# 构建 + 本脚本，与 ARM64 smoke 同 job 串行）。
#
# 断言七标记（状态谓词，非竞速）：
#   1. "BerryOS boot (x86_64)" —— 32 位入口 → 长模式 → Rust main 全通；
#   2. "PVH ok" —— hvm_start_info magic/version 校验过（引导协议握手内容
#      活证，非仅串口能出字）；
#   3. "TIMER ok" —— PIT 100Hz → 8259 IRQ0 → IDT 0x20 → sti 全链真中断
#      收满 100 拍（拍数只能来自 handler 递增——定时器链内容活证）；
#   4. "EXC ok" —— ud2 触发 #UD(6)：dump 落串口后 rip+2 恢复续行 + rax 金丝
#      雀往返不变（异常门/桩帧/全量保存恢复三环的机器断言——恢复判定
#      EXC_RECOVERED 是状态谓词；EXC dump vec=6 行先于标记、形态可区分）；
#   5. "CTX ok" —— A/B 两内核任务在独立 .bss 栈上经 context_switch 严格
#      乒乓 128 轮：每轮双方互验对方计数，计数值只能来自真实交替切换而非
#      两个独立自增（callee-saved/rsp 保存恢复 + ret 弹入口的机器断言）；
#   6. "SYS ok" —— SYSCALL/SYSRET 全链：CPL3 用户程序 128 轮 syscall(1)
#      对拍回显 3i+7（错配即 ud2 → CPL3 #UD → EXC panic loud 快败），末轮
#      syscall(2) DONE 哨兵经 sys_kern_return 内核恢复路径归位 main——
#      SYS_COUNT/SYS_USER_OK 只能来自真实 CPL3↔CPL0 往返（MSR/TSS/用户段
#      U/S 授权任一环断则标记缺失）；
#   7. "X86 ok" —— 刀验收标记。
# 任何 FAIL 行 / "EXC panic" 出现即 FAIL（比标记缺失更快定位——意外向量走
# #GP(13) dump 后 halt，真异常 loud 死不给假绿）。
#
# QEMU 生命周期同 smoke.sh 纪律：后台拉起 + 按记录 PID kill（多会话共享本机，
# 禁 pkill）；-no-reboot 让早期三重故障直接退出（否则 SeaBIOS 重启循环刷日志）。
set -u

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."
source "$SCRIPT_DIR/paths.sh"   # 非交互 PATH 修补（同 smoke.sh）

# 内核路径 = 必传首参（just smoke-x86 经 X86_KERNEL 注入——单一事实源，
# 勿在本文件硬编码第二份；对抗审查实抓过的漂移面）。
KERNEL=${1:?用法：just smoke-x86（或显式传参：bash tools/smoke-x86.sh <内核 ELF>）}
QEMU=${QEMU:-qemu-system-x86_64}
DEADLINE=60

command -v "$QEMU" >/dev/null 2>&1 || { echo "smoke-x86: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f "$KERNEL" ] || { echo "smoke-x86: FAIL: 内核未构建（先 just build-x86）：$KERNEL" >&2; exit 1; }

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-x86.XXXXXX")
QEMU_PID=""

cleanup() {
  if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null
    wait "$QEMU_PID" 2>/dev/null
  fi
  if [ -n "${SMOKE_KEEP_LOG:-}" ]; then
    echo "smoke-x86: 串口日志保留：$LOG" >&2
  else
    rm -f "$LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke-x86: FAIL: $1" >&2
  echo "---- 串口日志尾部（$LOG）----" >&2
  tail -30 "$LOG" >&2
  exit 1
}

echo "smoke-x86: QEMU 启动（$KERNEL）"
"$QEMU" -m 128M -nographic -no-reboot -kernel "$KERNEL" \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

MARKERS=("BerryOS boot (x86_64)" "PVH ok" "TIMER ok" "EXC ok" "CTX ok" "SYS ok" "X86 ok")
t0=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（引导崩/拒收？）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  grep -q "X86 FAIL" "$LOG" && fail "内核自报 FAIL：$(grep 'X86 FAIL' "$LOG" | tail -1)"
  grep -q "EXC panic" "$LOG" && fail "异常面 panic：$(grep 'EXC panic' "$LOG" | tail -1)"
  missing=""
  for m in "${MARKERS[@]}"; do
    grep -qF "$m" "$LOG" || missing="${missing:+$missing }$m"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE" ] && fail "${DEADLINE}s 标记未到齐，缺：$missing"
  sleep 1
done

# 标记恰一次（重复 = 引导脚本重入，PVH 不该有第二跳）
for m in "${MARKERS[@]}"; do
  n=$(grep -cF "$m" "$LOG" || true)
  [ "$n" -eq 1 ] || fail "标记非恰一次（n=$n）：$m"
done

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""
echo "smoke-x86: PASS（7 标记恰一次到齐）"
