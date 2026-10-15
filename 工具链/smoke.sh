#!/usr/bin/env bash
# BerryOS 无人值守冒烟测试 —— CI 与本地同一入口（just smoke）。
#
# 做三件事：
#   1. 后台拉起 QEMU virt（-kernel ELF 直加载），串口重定向到临时日志；
#   2. 断言 boot 标记全部出现（BerryOS boot / GIC ok / … / ktest done）；
#   3. 断言 switches=N 金丝雀 ≥3 条且严格递增（IRQ 抢占分时活着、计数不冻）。
#
# 注意：QEMU TCG 下 CNTV tick 实测 ≈ SLICE_HZ/2（见 CLAUDE.md 陷阱节），
# 切换率 ≈5/s，100 次切换 ≈20s，故金丝雀截止 180s 足够宽。
# 调试：SMOKE_KEEP_LOG=1 just smoke 保留串口日志不删。
set -u

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."

# 非交互 shell 常缺 ~/.cargo/bin（与 justfile 开头同病），补上再找 cargo。
if ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

# 与 justfile 同法解析 target 目录（本机 ~/.cargo/config.toml 可能把 target-dir
# 指到全局 ~/.cargo-target，CI 上则是仓库 ./target——cargo metadata 两边都对）。
TARGET_DIR=$(cargo metadata --no-deps --format-version=1 2>/dev/null \
  | grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4)
KERNEL="$TARGET_DIR/aarch64-unknown-none/debug/bos-kernel"

QEMU=${QEMU:-qemu-system-aarch64}
command -v "$QEMU" >/dev/null 2>&1 || { echo "smoke: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f "$KERNEL" ] || { echo "smoke: FAIL: 内核未构建（先 just build）：$KERNEL" >&2; exit 1; }

# 2b-1 高半翻转程序头断言（内存管理设计 §5.1：VMA≠LMA 防漂移哨兵——
# boot 段 vaddr==paddr、主体 vaddr-paddr==KERNEL_VA_BASE，任一漂移即 FAIL）。
python3 "$(dirname "$0")/size.py" --assert-flip "$KERNEL" \
  || { echo "smoke: FAIL: 高半程序头断言失败" >&2; exit 1; }

DEADLINE_BOOT=120    # 秒：boot 全部标记到齐
DEADLINE_CANARY=180  # 秒：等 ≥3 条 switches 金丝雀
MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "ktest done")

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke.XXXXXX")
QEMU_PID=""

cleanup() {
  if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null
    wait "$QEMU_PID" 2>/dev/null
  fi
  if [ -n "${SMOKE_KEEP_LOG:-}" ]; then
    echo "smoke: 串口日志保留：$LOG" >&2
  else
    rm -f "$LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke: FAIL: $1" >&2
  # 失败必带现场：串口日志尾部直接进 stderr（CI 里即是诊断依据）。
  echo "---- 串口日志尾部（$LOG）----" >&2
  tail -40 "$LOG" >&2
  echo "--------------------------------" >&2
  exit 1
}

echo "smoke: QEMU 启动（$KERNEL）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# —— 阶段 1：boot 标记（全部出现才算过）——
t0=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（崩溃？）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${MARKERS[@]}"; do
    grep -qF "$m" "$LOG" || missing="$m"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE_BOOT" ] && fail "boot 标记 ${DEADLINE_BOOT}s 未到齐，缺：$missing"
  sleep 1
done
echo "smoke: boot 标记 ${#MARKERS[@]}/${#MARKERS[@]} 到齐"

# —— 阶段 2：switches 金丝雀（≥3 条且严格递增）——
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

last=$(grep -o 'switches=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
echo "smoke: PASS（金丝雀 ${n} 条严格递增，最新 switches=$last）"
