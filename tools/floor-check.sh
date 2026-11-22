#!/usr/bin/env bash
# floor-check.sh —— 工具版本地板门（工程纪律刀阶段 2；规则/16 §2）。
# 地板声明（单源纪律——地板常量与消费者理由同文件）：
#   rustc   单源 = rust-toolchain.toml channel（本文件不自持第二份；rustup
#           在仓内自动按该文件选版，rustc --version 必须 >= channel）。
#   qemu    >= 10.0 —— 非安全第二 PL011 0x09040000 是 QEMU 10.0 新增（8.2
#           下第二个 -serial 只建 chardev 不建设备 → RX 相 uart1_init
#           external abort；ci.yml 原注释迁此成执法）。
#   node    >= 18  —— check-docs.mjs 头注契约（Node >= 18）。
#   python3 >= 3.9 —— docs_guard.py 运行期 list[str] 泛型注解面。
#   just    在场即可 —— 无已知最低版需求（地板 = presence，不虚设数字）。
# --verify 语义（impossible-os 同款「证不出的地板即失败」）：缺工具、版本
# 解析不出、低于地板，一律 FAIL。CI 首步与本地同脚本（环境漂移从排查题
# 变一行红）。
# --selftest：PATH 桩垫片三腿——低版必红 / 达标必过 / 畸形版本必红。
set -u
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$SCRIPT_DIR/.."
TOOLCHAIN_FILE=${FLOOR_TOOLCHAIN_FILE:-rust-toolchain.toml}

FLOOR_QEMU="10.0"
FLOOR_NODE="18"
FLOOR_PY="3.9"

fails=0

# ver_le A B：A <= B（点分数值比较；bash 3.2 安全，段缺席按 0）
ver_le() {
  local a b i x y
  local IFS=.
  a=($1); b=($2)
  unset IFS
  for i in 0 1 2 3; do
    x=$((10#${a[$i]:-0})); y=$((10#${b[$i]:-0}))
    [ "$x" -lt "$y" ] && return 0
    [ "$x" -gt "$y" ] && return 1
  done
  return 0
}

# need_ver <工具> <实得版本串> <地板>：空串/非数值形态即解析失败红
need_ver() {
  local tool=$1 got=$2 floor=$3
  if [ -z "$got" ] || ! printf '%s' "$got" | grep -Eq '^[0-9]+(\.[0-9]+)*$'; then
    echo "floor-check FAIL: $tool 版本解析不出（实得 '${got:-空}'）——证不出的地板即失败" >&2
    fails=$((fails + 1)); return
  fi
  if ver_le "$floor" "$got"; then
    echo "floor-check: ok  $tool ${got}（地板 ${floor}）"
  else
    echo "floor-check FAIL: $tool $got 低于地板 $floor" >&2
    fails=$((fails + 1))
  fi
}

run_floors() {
  local channel
  if [ ! -f "$TOOLCHAIN_FILE" ]; then
    echo "floor-check FAIL: $TOOLCHAIN_FILE 缺席——rustc 地板单源不可解析" >&2
    fails=$((fails + 1))
    channel=""
  else
    channel=$(awk -F'"' '/^[[:space:]]*channel[[:space:]]*=/{print $2; exit}' "$TOOLCHAIN_FILE")
  fi
  need_ver rustc "$(rustc --version 2>/dev/null | awk '{print $2}')" "${channel:-0}"
  need_ver qemu-system-aarch64 "$(qemu-system-aarch64 --version 2>/dev/null | awk '{print $4}' | head -1)" "$FLOOR_QEMU"
  need_ver node "$(node --version 2>/dev/null | sed 's/^v//')" "$FLOOR_NODE"
  need_ver python3 "$(python3 --version 2>/dev/null | awk '{print $2}')" "$FLOOR_PY"
  if command -v just >/dev/null 2>&1; then
    echo "floor-check: ok  just $(just --version 2>/dev/null | head -1)（地板 = presence）"
  else
    echo "floor-check FAIL: just 缺席" >&2
    fails=$((fails + 1))
  fi
}

case "${1:-}" in
  --selftest)
    T=$(mktemp -d) || exit 2
    trap 'rm -rf "$T"' EXIT
    mkdir -p "$T/bin"
    printf 'channel = "9.9.9"\n' > "$T/rust-toolchain.toml"
    mk_stub() { printf '#!/bin/sh\necho "%s"\n' "$2" > "$T/bin/$1"; chmod +x "$T/bin/$1"; }
    # 腿 1：全低于地板 → 必红
    mk_stub rustc 'rustc 1.96.0 (xxxx 2026-01-01)'
    mk_stub qemu-system-aarch64 'QEMU emulator version 8.2.0'
    mk_stub node 'v16.20.0'
    mk_stub python3 'Python 3.8.2'
    mk_stub just 'just 1.0.0'
    PATH="$T/bin:$PATH" FLOOR_TOOLCHAIN_FILE="$T/rust-toolchain.toml" bash "$0" >/dev/null 2>&1 \
      && { echo "floor-check 自检 FAIL: 低版腿未红" >&2; exit 1; }
    # 腿 2：全达标（rustc 恰等于 channel）→ 必过
    mk_stub rustc 'rustc 9.9.9 (xxxx 2026-01-01)'
    mk_stub qemu-system-aarch64 'QEMU emulator version 11.0.3'
    mk_stub node 'v18.19.0'
    mk_stub python3 'Python 3.9.6'
    PATH="$T/bin:$PATH" FLOOR_TOOLCHAIN_FILE="$T/rust-toolchain.toml" bash "$0" >/dev/null 2>&1 \
      || { echo "floor-check 自检 FAIL: 达标腿未过" >&2; exit 1; }
    # 腿 3：版本串畸形（解析不出）→ 必红
    mk_stub qemu-system-aarch64 'weird output'
    PATH="$T/bin:$PATH" FLOOR_TOOLCHAIN_FILE="$T/rust-toolchain.toml" bash "$0" >/dev/null 2>&1 \
      && { echo "floor-check 自检 FAIL: 畸形版本腿未红" >&2; exit 1; }
    echo "floor-check 自检：PASS（低版 / 达标 / 畸形三腿全触发）"
    ;;
  --verify|"")
    run_floors
    if [ "$fails" -gt 0 ]; then
      echo "floor-check: FAIL（${fails} 项不过）" >&2
      exit 1
    fi
    echo "floor-check: PASS（rustc 地板单源 rust-toolchain.toml；qemu>=${FLOOR_QEMU} node>=${FLOOR_NODE} python>=${FLOOR_PY} just presence）"
    ;;
  *) echo "usage: $0 [--verify|--selftest]" >&2; exit 2 ;;
esac
