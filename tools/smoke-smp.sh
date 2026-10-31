#!/usr/bin/env bash
# BerryOS S-1 副核 bring-up 冒烟（`SMP设计草案.md` §4 S-1 行；smoke-x86 同款
# 独立形态——不并入主 smoke、不进 CI：主 smoke 五跑是单核回归基线，零扰动）。
#
# **-smp 只进本线**：全仓唯一带 `-smp 2` 的 QEMU 入口（justfile run-smp /
# smoke-smp 本脚本）；主 run/run-el2/run-rootfs/smoke 一律单核（S-1 单核零
# 漂移硬线）。默认机型 + `-kernel` ELF 直加载（conduit 分型：HVC 只在默认
# 机型被 QEMU 拦截为 PSCI——virtualization=on 下 HVC 从 EL1 陷阱到 EL2 且
# 不再拦截，VBAR_EL2=0 → PC=0 静默挂死，内核侧 BOOT_EL 门先行跳过）。
#
# 断言三面（状态谓词，非竞速）：
#   1. `SMP ok ncores=2` 恰一次（grep -cF 全串 = 1）——副核 PSCI CPU_ON →
#      §3.2 十项 → hb1 → 主核 dump → release → hb2 握手全闭环的机器断言
#      （两心跳都是内存状态断言，非时序竞速——4-5a 纪律）；反向断言
#      `SMP: FAIL` / `SMP: skip` 不出现（错机型/错旗时 loud 快败）。
#   2. 裸相 27 标记全部出现（与主 smoke 裸相同面——副核 park 后主核全链
#      零扰动：ktest/BENCH/G1.5/稳态矩阵照跑）。
#   3. switches 金丝雀 ≥3 条严格递增（主核调度器在双核地形下活着——
#      副核稳态 wfi 真睡、零干扰主核 tick/轮转）。
#
# 时限：双核 MTTCG 吞吐实测 ≈ 单核 0.61x（S-0 B 相）——全部帽 ≥2x 单核
# （boot 120→240s、金丝雀 180→400s；常态 ~2min 内到齐，帽只兜结构性挂死）。
#
# QEMU 生命周期同 smoke.sh 纪律：后台拉起 + 按记录 PID kill（多会话共享本机，
# 禁 pkill）；kill 后 sleep 3（SIGTERM 后 QEMU 仍存活数秒——P6 A-6 实证）。
set -u

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."
source "$SCRIPT_DIR/paths.sh"   # 非交互 PATH 修补（同 smoke.sh）

# 内核路径 = 必传首参（just smoke-smp 经 KERNEL 注入——单一事实源，勿在本
# 文件硬编码第二份；smoke-x86 对抗审查实抓过的漂移面）。
KERNEL=${1:?用法：just smoke-smp（或显式传参：bash tools/smoke-smp.sh <内核 ELF>）}
QEMU=${QEMU:-qemu-system-aarch64}
DEADLINE_BOOT=240    # 秒：boot 标记 + SMP 握手到齐（单核 120s ×2）
DEADLINE_CANARY=400  # 秒：等 ≥3 条 switches 金丝雀（单核 180s ×2；双核
                     # MTTCG 0.61x 吞吐 + 金丝雀行周期变长）

command -v "$QEMU" >/dev/null 2>&1 || { echo "smoke-smp: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f "$KERNEL" ] || { echo "smoke-smp: FAIL: 内核未构建（先 just build）：$KERNEL" >&2; exit 1; }

# QEMU 严格串行（多会话共享本机）：起跑前等字段空（占位 → 15s 重试，至多
# 5 轮后 loud 退——不抢跑、不 pkill 别人的 run）。
FIELD_WAITED=0
while pgrep -f qemu-system >/dev/null 2>&1; do
  FIELD_WAITED=$((FIELD_WAITED + 1))
  [ "$FIELD_WAITED" -gt 5 ] && { echo "smoke-smp: FAIL: QEMU 字段被占（pgrep 非空），放弃" >&2; exit 1; }
  echo "smoke-smp: QEMU 字段被占，等待 15s（第 ${FIELD_WAITED} 轮）…" >&2
  sleep 15
done

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-smp.XXXXXX")
QEMU_PID=""

cleanup() {
  if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null
    wait "$QEMU_PID" 2>/dev/null
  fi
  # SIGTERM 后 QEMU 仍存活数秒并继续刷串口（P6 A-6 实证）——断言前日志须已
  # 落定，kill+wait 后再 sleep 3 收尾。
  if [ -n "$QEMU_PID" ]; then sleep 3; fi
  if [ -n "${SMOKE_KEEP_LOG:-}" ]; then
    echo "smoke-smp: 串口日志保留：$LOG" >&2
  else
    rm -f "$LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke-smp: FAIL: $1" >&2
  echo "---- 串口日志尾部（$LOG）----" >&2
  tail -40 "$LOG" >&2
  echo "--------------------------------" >&2
  exit 1
}

# 标记断言：DEMO ok 特判（AIDEMO ok 子串混叠——smoke.sh 同款，P5 对抗审查
# 实抓）；其余裸 grep -qF。
assert_marker() {
  if [ "$1" = "DEMO ok" ]; then
    grep -qE '(^|[^I])DEMO ok' "$LOG"
  else
    grep -qF "$1" "$LOG"
  fi
}

MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "FP ok" "ASPACE ok" "VMO ok" "HANDLE ok" "POLICY ok" "ktest done" "BENCHXH ok" "BENCH ok n=6" "BENCHWAKE n=" "G1.5 ok" "BLOCK ok" "WAIT ok" "CHAIN ok" "MANY ok" "PI ok" "INTENT ok" "FAST ok" "SVC ok" "AIDEMO ok" "DEMO ok")

echo "smoke-smp: QEMU 启动（$KERNEL，-smp 2）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# —— 阶段 1：SMP 握手 + boot 标记（全到齐才算过）——
SMP_MARKER="SMP ok ncores=2"
t0=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（崩溃？）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  grep -q "SMP: FAIL" "$LOG" && fail "SMP 握手自报 FAIL：$(grep 'SMP: FAIL' "$LOG" | tail -1)"
  grep -q "SMP: skip" "$LOG" && fail "SMP 被跳过（错机型/错旗？）：$(grep 'SMP: skip' "$LOG" | tail -1)"
  missing=""
  grep -qF "$SMP_MARKER" "$LOG" || missing="$SMP_MARKER"
  for m in "${MARKERS[@]}"; do
    assert_marker "$m" || missing="${missing:+$missing }$m"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE_BOOT" ] && fail "boot/SMP 标记 ${DEADLINE_BOOT}s 未到齐，缺：$missing"
  sleep 1
done

# SMP 标记恰一次（重复 = 副核路径重入；缺失已由上面覆盖——这里 n≠1 即 FAIL）。
n=$(grep -cF "$SMP_MARKER" "$LOG" || true)
[ "$n" -eq 1 ] || fail "SMP 标记非恰一次（n=$n）：$SMP_MARKER"

# 副核报告行在席（bring-up 证据面：mpidr/ctx 回显/per-core 寄存器——超时
# dump 的诊断源）；哨兵 's' 紧邻其前（UART 单写者纪律的可观测面）。
grep -q "SMP: sec up" "$LOG" || fail "缺副核报告行 SMP: sec up"

echo "smoke-smp: SMP 握手 + boot 标记 ${#MARKERS[@]}/${#MARKERS[@]} 到齐"

# —— 阶段 2：switches 金丝雀（≥3 条且严格递增；smoke.sh 同款 awk 体）——
t1=$(date +%s)
while :; do
  n=$(grep -c 'switches=' "$LOG" || true)
  [ "${n:-0}" -ge 3 ] && break
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在金丝雀阶段退出"
  [ $(( $(date +%s) - t1 )) -gt "$DEADLINE_CANARY" ] \
    && fail "金丝雀 ${DEADLINE_CANARY}s 内不足 3 条（现有 ${n:-0}）"
  sleep 2
done

grep -o 'switches=[0-9]*' "$LOG" | cut -d= -f2 | awk '
  { if (NR>1 && $1<=prev) bad=1; prev=$1; n=NR }
  END { exit (bad || n<3) ? 1 : 0 }
' || fail "switches 序列非严格递增：$(grep -o 'switches=[0-9]*' "$LOG" | tr '\n' ' ')"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""
echo "smoke-smp: PASS（SMP ok ncores=2 恰一次 + 裸相 ${#MARKERS[@]} 标记 + 金丝雀严格递增）"
