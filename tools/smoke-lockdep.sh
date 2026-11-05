#!/usr/bin/env bash
# G3-3 lockdep 面独立冒烟（f4 复验 F10 闭合件——2026-11-05）。
#
# 缺口：selftest marker `LOCKDEP selftest ok` 此前无自动化门禁载体——
# smoke.sh/smoke-smp.sh/markers.sh 全树无 LOCKDEP 执法面；selftest FAIL
# 非停机（与 VIOLATION 的 fail-loud 停机不同），无 KEEP_LOG 的跑里 FAIL
# 行随串口档清失 = 零执法。本脚本 = feature 面最小执法载体：
#
#   快启 -smp 2 → 等 marker（帽 60s）→ 三断言：
#     ① `LOCKDEP selftest ok` 恰一次
#     ② `LOCKDEP selftest FAIL` 零次
#     ③ `LOCKDEP VIOLATION` 零次
#     ④ `DPG ok` ≥1 且行号晚于 selftest ok（C-3，G4-1 三镜：selftest 在
#       boot 窗先于 dpg::init——断言窗延到 dpg 闭环毕，执法「feature boot
#       含 dpg 锁序」不再靠借位）
#
# 用法：bash tools/smoke-lockdep.sh <内核 ELF>（须 feature 面——脚本先验
# marker 串在镜像内，default 面二进制直拒 loud，不烧 60s 帽）。镜像出法：
#   cargo build --features lockdep && bash tools/smoke-lockdep.sh "$(bash tools/paths.sh kernel)"
# （无 just 配方：justfile 为兄弟会话 WIP 占用时 script 直调同效——
# smoke-smp 先例，内核路径参数化单一事实源。）
#
# 不并入主 smoke 族（ARM64 四跑零扰动纪律——主族 default 面单核；feature
# 面独立通道）。FAIL 恒留档；SMOKE_LOCKDEP_KEEP_LOG=1 成功亦留。
set -u

KERNEL="${1:-}"
if [ -z "$KERNEL" ] || [ ! -f "$KERNEL" ]; then
  echo "smoke-lockdep: 用法：$0 <内核 ELF>（feature 面——cargo build --features lockdep 出）" >&2
  exit 2
fi

# 镜像面自证：default 面二进制 marker 串编译消除——前置直拒（陈旧产物/
# 面错配零歧义，G-eco-b「陈旧产物静默上线」同族防线）。
if ! strings "$KERNEL" | grep -q 'LOCKDEP selftest ok'; then
  echo "smoke-lockdep: 镜像非 lockdep feature 面（marker 串零命中）——先 cargo build --features lockdep" >&2
  exit 2
fi

PATH="$HOME/.cargo/bin:$PATH"
QEMU="qemu-system-aarch64"
MACH="-machine virt -cpu cortex-a72 -m 128M -nographic"

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-lockdep.XXXXXX")
keep_log() {
  if [ -n "${SMOKE_LOCKDEP_KEEP_LOG:-}" ] || [ "$1" = "keep" ]; then
    echo "smoke-lockdep: 串口日志保留：$LOG" >&2
  else
    rm -f "$LOG"
  fi
}

# 快启 -smp 2（selftest 在 boot 窗 G3-1a assert 后；-smp 2 = 快启直验同形态，
# 亦是最强面——per-CPU 链栈双核各走 selftest 点）。
"$QEMU" $MACH -smp 2 -kernel "$KERNEL" >"$LOG" 2>&1 &
QPID=$!

# 有界等待：marker 三态任一即止（ok / FAIL / VIOLATION），帽 60s（本机实测
# boot→marker ≈2s；帽对宿主争用保守 ×30）。C-3：ok 态续等到 `DPG ok`
#（dpg::init 在 selftest 后——锁序执法面须盖到 dpg 闭环毕）；FAIL/VIOLATION/
# panicked = 终态坏，早断交断言面自红。
deadline=$((SECONDS + 60))
while [ $SECONDS -lt $deadline ]; do
  if grep -qE 'LOCKDEP selftest FAIL|LOCKDEP VIOLATION|panicked' "$LOG" 2>/dev/null; then
    break
  fi
  if grep -q 'LOCKDEP selftest ok' "$LOG" 2>/dev/null && grep -q 'DPG ok' "$LOG" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$QPID" 2>/dev/null; then
    break
  fi
  sleep 0.2
done
kill "$QPID" 2>/dev/null
wait "$QPID" 2>/dev/null

OK=$(grep -c 'LOCKDEP selftest ok' "$LOG" 2>/dev/null || true)
FAIL=$(grep -c 'LOCKDEP selftest FAIL' "$LOG" 2>/dev/null || true)
VIO=$(grep -c 'LOCKDEP VIOLATION' "$LOG" 2>/dev/null || true)

rc=0
if [ "$OK" -ne 1 ]; then
  echo "smoke-lockdep: FAIL——selftest ok 计数 $OK（期望恰 1；0 = 未达 boot 窗或超时）" >&2
  rc=1
fi
if [ "$FAIL" -ne 0 ]; then
  echo "smoke-lockdep: FAIL——selftest FAIL 计数 $FAIL（六阳性三负对照有臂翻车）" >&2
  rc=1
fi
if [ "$VIO" -ne 0 ]; then
  echo "smoke-lockdep: FAIL——VIOLATION 计数 $VIO（真锁序违约——检测器命中）" >&2
  rc=1
fi
# ④（C-3）：DPG ok ≥1 且行号晚于 selftest ok 行（执法窗盖到 dpg 闭环）。
DPG=$(grep -c 'DPG ok' "$LOG" 2>/dev/null || true)
SF_LINE=$(grep -n 'LOCKDEP selftest ok' "$LOG" 2>/dev/null | head -1 | cut -d: -f1)
DPG_LINE=$(grep -n 'DPG ok' "$LOG" 2>/dev/null | head -1 | cut -d: -f1)
if [ "$DPG" -lt 1 ]; then
  echo "smoke-lockdep: FAIL——DPG ok 计数 $DPG（C-3：断言窗已延至 dpg 闭环后，0 = dpg 链未毕/超时）" >&2
  rc=1
elif [ -z "$SF_LINE" ] || [ -z "$DPG_LINE" ] || [ "$DPG_LINE" -le "$SF_LINE" ]; then
  echo "smoke-lockdep: FAIL——DPG ok 行号($DPG_LINE) 不晚于 selftest ok 行号($SF_LINE)（C-3 行序断言）" >&2
  rc=1
fi

if [ "$rc" -eq 0 ]; then
  echo "smoke-lockdep: PASS（selftest ok 恰 1 + FAIL 0 + VIOLATION 0 + DPG ok 行序对，-smp 2 快启）"
  keep_log drop
else
  keep_log keep
fi
exit "$rc"
