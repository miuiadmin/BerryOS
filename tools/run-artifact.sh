#!/usr/bin/env bash
# run-artifact.sh —— 证物信封（自动化开发二期 P1；深研 survey 推绿链节）。
# 为什么：一条门命令的结论若只活在终端滚动缓冲里，跑完即蒸发——无人会话
# 「自称绿」无法对账，事后只剩人嘴。信封把「开始 / 判决」落成盘上状态机：
# begin 建 running，verdict 落判决后才翻转成 complete；中途被杀 = 永远
# running（或无标记的缺件）= 可见的未完成态，绝不假完成。
# fail-direction 总律：check 的一切歧义都倒向非 0——宁可报「未完成 / 缺件」，
# 不可报「通过」。信封不证明门绿，只证明门「跑完且判了什么」。
# 子命令：
#   begin <label>                     建信封：<根>/<label>/ + running 标记 + 起 ts
#   verdict <label> <rc> [摘要行...]  写判决文件（rc+摘要+完成 ts）→ 删 running
#                                     → 加 complete。顺序立法见 cmd_verdict 注。
#   check <label>                     0=complete 且 rc=0；1=complete 且 rc!=0；
#                                     2=running（未完成）；3=缺件（无信封 / 无
#                                     标记 / 判决被撕或不可解析）
#   rotate                            只保最新 40 个 label 目录（按 mtime 新→旧）；
#                                     artifacts.log 是文件，永不随轮转消失
# 账本 sink：<根>/artifacts.log，append-only，begin/verdict 各恰一行、不可
# 屏蔽（无关闭开关；追加失败即 loud 红）——信封文件可被 rotate 走、被重跑
# 覆写，账本行是最后的事实源。
# env：ARTIFACTS_DIR 覆写根（默认 <仓>/out/artifacts——out/ 已 gitignore，
# 信封不污染树净门；自检与上级包裹器用它隔离）。
# 用法例（完整一行式见 tools/gate-wrap.sh）：
#   bash tools/run-artifact.sh begin smoke-bare-1234
#   bash tools/run-artifact.sh verdict smoke-bare-1234 0 "markers 12/12"
#   bash tools/run-artifact.sh check smoke-bare-1234
# 退出码：0 成功；1 内部失败（mkdir/写盘/追加账本）；2 用法错（含非法 label、
# verdict 无信封）；check 另有 3=缺件。check 的 1（门红）对本工具不是「工具
# 失败」——那是信封如实转述判决。
set -u
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
ARTIFACTS_DIR=${ARTIFACTS_DIR:-$ROOT/out/artifacts}
KEEP=40
# label 字符集律：首字符字母数字，余下 [A-Za-z0-9._-]——禁斜杠禁空白，防
# 路径逃逸（label 直接拼进目录路径），也让轮转的 ls 解析可安全分词。
# bash 3.2（macOS）正则须走变量——引号字面量在 [[ ]] 里不作正则用。
LABEL_RE='^[A-Za-z0-9][A-Za-z0-9._-]*$'

now_iso() { date -u +%Y-%m-%dT%H:%M:%SZ; }

label_ok() { [[ "$1" =~ $LABEL_RE ]]; }

# log_line <文案>：账本恰一行。不可屏蔽 = 无开关 + 失败 loud 红（return 1）。
log_line() {
  mkdir -p "$ARTIFACTS_DIR" || return 1
  printf '%s %s\n' "$(now_iso)" "$1" >> "$ARTIFACTS_DIR/artifacts.log" || {
    echo "run-artifact: artifacts.log 追加失败（$ARTIFACTS_DIR/artifacts.log）——账本是最后事实源，不许静默" >&2
    return 1
  }
}

cmd_begin() {
  local label=$1 dir
  label_ok "$label" || { echo "run-artifact: 非法 label「${label}」（只许 [A-Za-z0-9][A-Za-z0-9._-]*——防路径逃逸）" >&2; return 2; }
  dir=$ARTIFACTS_DIR/$label
  mkdir -p "$dir" || { echo "run-artifact: 建信封失败（${dir}）" >&2; return 1; }
  # 重开同一 label = 有意重跑：清 complete/verdict（上一轮的账已在 artifacts.log
  # 永不丢失），running 以新 ts 覆写——check 立即回到「未完成」侧，旧判决不再
  # 被误读为本轮结论。
  rm -f "$dir/complete" "$dir/verdict"
  printf 'started=%s\n' "$(now_iso)" > "$dir/running" || { echo "run-artifact: 写 running 失败（${dir}）" >&2; return 1; }
  log_line "begin $label" || return 1
  echo "run-artifact: begin ${label}（${dir}）"
}

cmd_verdict() {
  local label=$1 rc=$2 dir
  shift 2
  label_ok "$label" || { echo "run-artifact: 非法 label「${label}」" >&2; return 2; }
  [[ "$rc" =~ ^-?[0-9]+$ ]] || { echo "run-artifact: verdict 的 rc 须为整数，得「${rc}」" >&2; return 2; }
  dir=$ARTIFACTS_DIR/$label
  [ -d "$dir" ] || { echo "run-artifact: 无信封 ${label}（先 begin 再 verdict——没有开始的判决是悬空证物）" >&2; return 2; }
  # 顺序立法：① 判决文件先落盘（rc+完成 ts+摘要）→ ② 删 running → ③ 加
  # complete。三步不可换序：①后被杀 → running 仍在 = check 2（未完成，真话）；
  # ②后被杀 → 无 running 无 complete = check 3（缺件，真话）；任何时序都不
  # 出现「complete 而无判决」或「running 却已判绿」的假态。
  { printf 'rc=%s\n' "$rc"
    printf 'finished=%s\n' "$(now_iso)"
    # 摘要可零行：末句必须无条件成功——判例（本件自检抓出）：`[ $# -gt 0 ] &&
    # printf` 作组尾时零摘要让整组假红，verdict 报失败、complete 不落、信封
    # 卡 running。`if` 形态条件假时返回 0，组 status 只由写盘成败决定。
    if [ $# -gt 0 ]; then printf '%s\n' "$@"; fi
  } > "$dir/verdict" || { echo "run-artifact: 写判决失败（$dir/verdict）——信封保持 running（未完成态）" >&2; return 1; }
  rm -f "$dir/running"
  : > "$dir/complete" || { echo "run-artifact: 写 complete 失败（$dir/complete）" >&2; return 1; }
  log_line "verdict $label rc=$rc" || return 1
  echo "run-artifact: verdict $label rc=$rc"
}

cmd_check() {
  local label=$1 dir rc
  label_ok "$label" || { echo "run-artifact: 非法 label「${label}」" >&2; return 2; }
  dir=$ARTIFACTS_DIR/$label
  if [ ! -d "$dir" ]; then
    echo "run-artifact: check $label 缺件（无信封——未 begin 或已被 rotate）" >&2
    return 3
  fi
  # running 先判：即使 complete 因撕裂态并存，也按「未完成」报——宁可未完成
  # 不可假完成（重跑刚 begin 完旧 complete 未清的窗口同样落这里）。
  if [ -f "$dir/running" ]; then
    echo "run-artifact: check $label running（未完成——begin 后未 verdict，或 verdict 中途被杀）" >&2
    return 2
  fi
  if [ ! -f "$dir/complete" ]; then
    echo "run-artifact: check $label 缺件（无 running 无 complete——verdict 中途被杀的撕裂态）" >&2
    return 3
  fi
  if [ ! -f "$dir/verdict" ]; then
    echo "run-artifact: check $label 缺件（complete 而无判决文件——信封被撕，不可当通过）" >&2
    return 3
  fi
  rc=$(sed -n 's/^rc=//p' "$dir/verdict" | head -n 1)
  [[ "$rc" =~ ^-?[0-9]+$ ]] || { echo "run-artifact: check $label 缺件（判决 rc 行不可解析：得「${rc}」）" >&2; return 3; }
  if [ "$rc" -eq 0 ]; then
    echo "run-artifact: check $label complete rc=0"
    return 0
  fi
  echo "run-artifact: check $label complete rc=${rc}（信封如实转述：门红）" >&2
  return 1
}

cmd_rotate() {
  [ -d "$ARTIFACTS_DIR" ] || { echo "run-artifact: rotate 无事可做（根缺席）"; return 0; }
  local n=0 e
  cd "$ARTIFACTS_DIR" || { echo "run-artifact: rotate 进不去 $ARTIFACTS_DIR" >&2; return 1; }
  # ls -1t = mtime 新→旧。while-read 免分词免 glob（外来怪名不炸）；-d 跳过
  # artifacts.log 等文件——账本与任何文件都不在轮转面，只轮转目录。
  # 超出 KEEP 的最旧目录整体删除：证物有保鲜期，无界增长会被真仓 out/ 挤爆
  # （判例：/tmp 整洗扬掉 target dir——长寿命目录必须自带回收）。
  while IFS= read -r e; do
    [ -n "$e" ] || continue
    [ -d "./$e" ] || continue
    n=$((n + 1))
    if [ "$n" -gt "$KEEP" ]; then
      rm -rf "./$e" || { echo "run-artifact: rotate 删不动 ./$e" >&2; return 1; }
    fi
  done < <(LC_ALL=C ls -1t)
  echo "run-artifact: rotate 完成（保最新 $KEEP 个 label 目录，账本不动）"
}

selftest() {
  local T fails=0 r want got n
  T=$(mktemp -d) || { echo "run-artifact 自检 FAIL: mktemp 失败" >&2; return 1; }
  trap 'rm -rf "$T"' RETURN INT TERM
  # 隔离面：ARTIFACTS_DIR 指临时根——零真仓依赖（真 out/artifacts 不碰）。
  ARTIFACTS_DIR=$T/art
  export ARTIFACTS_DIR
  ME=$SCRIPT_DIR/run-artifact.sh

  ck() { # ck <腿名> <期望> <实得>
    if [ "$2" = "$3" ]; then
      echo "run-artifact 自检: 腿绿 $1"
    else
      echo "run-artifact 自检: FAIL $1（期望 $2 实得 $3）" >&2
      fails=$((fails + 1))
    fi
  }
  q() { bash "$ME" "$@" >/dev/null 2>&1; }  # 静默跑子命令，只收退出码

  # 主链：begin→running→verdict0→complete0→verdict1→complete1（同信封改判）
  q begin alpha; r=$?; ck "begin 退出 0" 0 "$r"
  q check alpha; r=$?; ck "begin 后 check=running(2)" 2 "$r"
  q verdict alpha 0 "markers 12/12"; r=$?; ck "verdict 0 退出 0" 0 "$r"
  q check alpha; r=$?; ck "verdict 0 后 check=0" 0 "$r"
  q verdict alpha 1 "门红"; r=$?; ck "verdict 1 退出 0" 0 "$r"
  q check alpha; r=$?; ck "verdict 1 后 check=1" 1 "$r"
  # 零摘要腿：摘要行可选——不带摘要的 verdict 不许假红（判例：`&&` 作组尾
  # 曾让零摘要报「写判决失败」卡 running）
  q begin nosum; q verdict nosum 0; r=$?; ck "零摘要 verdict 退出 0" 0 "$r"
  q check nosum; r=$?; ck "零摘要信封 complete rc=0" 0 "$r"
  # 缺件腿：无信封
  q check ghost; r=$?; ck "无信封 check=3" 3 "$r"
  # 撕裂腿：complete 在而判决被撕——不可当通过
  q begin beta; q verdict beta 0 "腿"
  rm "$T/art/beta/verdict"
  q check beta; r=$?; ck "判决被撕 check=3" 3 "$r"
  # 重跑腿：完成态再 begin → 立即回「未完成」，旧判决不复读
  q begin alpha; r=$?; ck "重开 begin 退出 0" 0 "$r"
  q check alpha; r=$?; ck "重开后 check=running(2)" 2 "$r"
  # 账本腿：begin/verdict 各恰一行、内容在账（此刻 8 op：alpha×4 + beta×2 + nosum×2）
  n=$(wc -l < "$T/art/artifacts.log" | tr -d ' ')
  ck "账本行数=8（begin/verdict 各一行不可屏蔽）" 8 "$n"
  command grep -q " begin alpha$" "$T/art/artifacts.log"; r=$?; ck "账本含 begin alpha" 0 "$r"
  command grep -q " verdict alpha rc=1$" "$T/art/artifacts.log"; r=$?; ck "账本含 verdict rc" 0 "$r"
  # 轮转帽腿：48 个信封（alpha/beta/nosum 最旧 + g01..g45 递新）→ rotate 后恰
  # 40，最旧 8 个（alpha beta nosum g01..g05）被回收，g06 起幸存——保鲜不误伤新证物
  local i
  for i in $(seq 1 45); do
    q begin "g$(printf %02d "$i")"
    touch -t "2026010100$(printf %02d "$i").00" "$T/art/g$(printf %02d "$i")"
  done
  touch -t 202512312359.00 "$T/art/alpha" "$T/art/beta" "$T/art/nosum"
  q rotate; r=$?; ck "rotate 退出 0" 0 "$r"
  n=$(find "$T/art" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')
  ck "轮转后目录数=40" 40 "$n"
  [ ! -d "$T/art/alpha" ]; r=$?; ck "最旧 alpha 已回收" 0 "$r"
  [ ! -d "$T/art/g05" ]; r=$?; ck "第 41 旧 g05 已回收" 0 "$r"
  [ -d "$T/art/g06" ]; r=$?; ck "第 40 新 g06 幸存" 0 "$r"
  [ -f "$T/art/artifacts.log" ]; r=$?; ck "账本不随轮转消失" 0 "$r"

  echo "run-artifact 自检：${fails} 腿红——$([ "$fails" -eq 0 ] && echo PASS || echo FAIL)"
  [ "$fails" -eq 0 ]
}

case "${1:-}" in
  begin) shift; [ $# -ge 1 ] || { echo "usage: run-artifact.sh begin <label>" >&2; exit 2; }; cmd_begin "$1" ;;
  verdict)
    shift; [ $# -ge 2 ] || { echo "usage: run-artifact.sh verdict <label> <rc> [摘要行...]" >&2; exit 2; }
    cmd_verdict "$@"
    ;;
  check) shift; [ $# -ge 1 ] || { echo "usage: run-artifact.sh check <label>" >&2; exit 2; }; cmd_check "$1" ;;
  rotate) cmd_rotate ;;
  --selftest) selftest ;;
  *) echo "usage: run-artifact.sh {begin <label>|verdict <label> <rc> [摘要...]|check <label>|rotate|--selftest}" >&2; exit 2 ;;
esac
