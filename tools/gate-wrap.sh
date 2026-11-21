#!/usr/bin/env bash
# gate-wrap.sh —— 门命令一行式包裹：证物信封 + 内容寻址回执（自动化开发二期 P1）。
# 为什么：推绿链要把「这条门命令真跑过、跑动中输入没被偷换、结论是什么」三件
# 事一次落齐。手动串 run-artifact 与 e-receipts 必有一环被忘——判例：e-guard
# 四门里「门禁套件绿」最常被人肉跳过。一行式让正确路径成为最短路径。
# 三条用法例（速记：label 在前、可选回执段、-- 后是原命令——原命令照旧抄）：
#   # smoke（不带回执段：纯信封——smoke 输入面是整棵树，files 清单列不全）：
#   bash tools/gate-wrap.sh smoke-bare-$$ -- just smoke-bare
#   # bench-icount（回执段：判据脚本 + 基线账两文件即漂移面）：
#   bash tools/gate-wrap.sh bench-icount-$$ bench-icount \
#     --files tools/bench.py tools/bench_baseline_icount.txt -- \
#     python3 tools/bench.py --check
#   # facts（回执段：对拍器 + 制品）：
#   bash tools/gate-wrap.sh facts-$$ facts \
#     --files tools/facts.py tools/facts.txt -- python3 tools/facts.py
# 语义：begin 信封 → e-receipts wrap（可选；输入漂移=rc4 回执作废）→ verdict
# 落判决 → 命令 rc 原样透传（含 4）。
# fail-direction：begin 失败就不跑命令直接红（exit 2）——没有证物的「绿」比
# 没有绿更糟；verdict 写不进则 exit 2 让信封停在 running（可见的未完成态），
# 绝不放行一个「跑过但无判决」的 0。
# env 透传：ARTIFACTS_DIR（信封根，见 run-artifact.sh）、E_RECEIPTS_STORE
# （回执存储，见 e-receipts.py——自检隔离用）。
# 退出码：命令 rc 原样；2 = 包裹器自身失败（用法/信封）；信封的 check 语义
# 另见 run-artifact.sh（0/1/2/3）。
set -u
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

usage() {
  echo "usage: gate-wrap.sh <label> [<kind> --files <f...>] -- <命令...>" >&2
  exit 2
}

run_main() {
  [ $# -ge 1 ] || usage
  LABEL=$1
  shift
  KIND=""
  FILES=()
  if [ "${1:-}" != "--" ]; then
    # 回执段：<kind> --files f... 直到 --（缺任一件即用法红——半套回执=假包裹）。
    [ $# -ge 2 ] || usage
    KIND=$1
    shift
    [ "${1:-}" = "--files" ] || usage
    shift
    [ $# -ge 1 ] && [ "$1" != "--" ] || usage
    while [ $# -gt 0 ] && [ "$1" != "--" ]; do
      FILES+=("$1")
      shift
    done
  fi
  [ $# -ge 1 ] && [ "$1" = "--" ] || usage
  shift
  [ $# -ge 1 ] || usage
  CMD=("$@")

  # ① 信封先立——立不起来就拒跑：跑而无据 = 事后对不了账的「自称绿」。
  bash "$SCRIPT_DIR/run-artifact.sh" begin "$LABEL" || {
    echo "gate-wrap: 信封建立失败——不跑命令（无据之绿=假绿）" >&2
    exit 2
  }

  # ② 跑活：有回执段走 e-receipts wrap（输入漂移面由 --files 定义），否则直跑。
  rc=0
  if [ -n "$KIND" ]; then
    python3 "$SCRIPT_DIR/e-receipts.py" wrap "$KIND" --files "${FILES[@]}" -- "${CMD[@]}" || rc=$?
  else
    "${CMD[@]}" || rc=$?
  fi

  # ③ 判决落信封（rc 原样入档，含漂移 4）；verdict 失败 → exit 2，信封停在
  # running=可见未完成，不透传可能被误解的 0。
  if ! bash "$SCRIPT_DIR/run-artifact.sh" verdict "$LABEL" "$rc" "cmd=${CMD[0]}" "receipt=${KIND:-none}"; then
    echo "gate-wrap: 判决落信封失败——信封保持 running（未完成态），不放行" >&2
    exit 2
  fi
  exit "$rc"
}

selftest() {
  local T fails=0 r
  T=$(mktemp -d) || { echo "gate-wrap 自检 FAIL: mktemp 失败" >&2; return 1; }
  trap 'rm -rf "$T"' RETURN INT TERM
  # 双隔离：信封根与回执存储都指临时面——零真仓依赖（真 out/artifacts 与
  # .claude/state 均不碰；后者是并行会话共享面）。
  local ARTIFACTS_DIR=$T/art E_RECEIPTS_STORE=$T/store.json
  export ARTIFACTS_DIR E_RECEIPTS_STORE
  ME=$SCRIPT_DIR/gate-wrap.sh
  RA=$SCRIPT_DIR/run-artifact.sh

  ck() {
    if [ "$2" = "$3" ]; then
      echo "gate-wrap 自检: 腿绿 $1"
    else
      echo "gate-wrap 自检: FAIL $1（期望 $2 实得 $3）" >&2
      fails=$((fails + 1))
    fi
  }

  # 腿1/2（任务钉板腿）：true/false 透传 + 信封落盘
  bash "$ME" gw-true -- true >/dev/null 2>&1; r=$?
  ck "true 透传 rc=0" 0 "$r"
  bash "$RA" check gw-true >/dev/null 2>&1; r=$?
  ck "true 信封 complete rc=0" 0 "$r"
  bash "$ME" gw-false -- false >/dev/null 2>&1; r=$?
  ck "false 透传 rc=1" 1 "$r"
  bash "$RA" check gw-false >/dev/null 2>&1; r=$?
  ck "false 信封 complete rc=1（门红如实入档）" 1 "$r"
  # 腿3：回执段绿链——回执在账 + 信封 rc=0。--files 用绝对夹具路径（e-receipts
  # 的输入键以仓根解析相对路径；绝对路径走工作树字节面，合成夹具零真仓写）。
  local f1=$T/in1.txt f2=$T/in2.txt
  printf 甲 > "$f1"; printf 乙 > "$f2"
  bash "$ME" gw-ok kind-ok --files "$f1" -- true >/dev/null 2>&1; r=$?
  ck "回执段包裹 rc=0" 0 "$r"
  bash "$RA" check gw-ok >/dev/null 2>&1; r=$?
  ck "回执段信封 rc=0" 0 "$r"
  command grep -q '"kind-ok"' "$T/store.json"; r=$?
  ck "回执落账（kind-ok 在 store）" 0 "$r"
  # 腿4（红腿）：命令跑动中改输入 → e-receipts 漂移 rc4 → 透传 4 + 信封 rc=4
  # （check=1 门红侧）+ 回执不记账
  bash "$ME" gw-drift kind-drift --files "$f2" -- bash -c "printf 丙 > '$f2'" >/dev/null 2>&1; r=$?
  ck "漂移透传 rc=4" 4 "$r"
  bash "$RA" check gw-drift >/dev/null 2>&1; r=$?
  ck "漂移信封 complete rc!=0（check=1）" 1 "$r"
  ! command grep -q '"kind-drift"' "$T/store.json"; r=$?
  ck "漂移不写回执" 0 "$r"
  # 腿5：用法红——缺 -- 段拒跑且不留信封（半包裹比不包裹更危险）
  bash "$ME" gw-usage 2>/dev/null; r=$?
  ck "缺命令段用法红 rc=2" 2 "$r"
  [ ! -d "$T/art/gw-usage" ]; r=$?
  ck "用法红不留信封" 0 "$r"

  echo "gate-wrap 自检：${fails} 腿红——$([ "$fails" -eq 0 ] && echo PASS || echo FAIL)"
  [ "$fails" -eq 0 ]
}

case "${1:-}" in
  --selftest) selftest ;;
  *) run_main "$@" ;;
esac
