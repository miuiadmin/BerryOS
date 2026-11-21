#!/bin/bash
# control-plane-gate.sh —— 控制面条件测试门（自动化开发二期 P2 治理四件；规则/16 §5）。
#
# 为什么：FROZEN 集（e-guard.py）只拦「无人会话写宪法」这一半；attended 会话改了
# 治理工具后没人逼它跑自检——2026-11-22 深研差距表 #7（改控制面必跑其测试）。
# 本门补另一半：staged 命中 tools/control-plane.txt 任一前缀 → 打印命中面并逐件
# 跑 --selftest（.py 走 python3、.sh 走 bash；.sh 内容无 --selftest 的跳过并列名；
# 目录/配置前缀走清单行「=> 命令」的前缀级测试，同前缀只跑一次）。无命中直通。
#
# fail-direction（哪边失败是安全边）：
#   自检红=红；清单文件缺席=红（删清单本身就是动了控制面，default-conservative）；
#   git 不可用=红（本门只在提交路径跑，宁可误拦不可漏拦）；
#   .sh 无 --selftest=跳过列名不红（存量补齐是欠账不是事故——列名让人看见）。
#
# 退出码：0=全绿或无命中；1=红；2=用法错。
# 环境变量（--selftest 与测试专用，正常提交路径不设；设了就不真动 git index）：
#   CP_STAGED_OVERRIDE    用换行分隔清单换掉 git diff --cached --name-only
#   CP_MANIFEST_OVERRIDE  换掉清单文件路径
#   CP_ROOT_OVERRIDE      换掉仓根（解析 staged 相对路径）
set -u

SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
ROOT="${CP_ROOT_OVERRIDE:-$(cd "$(dirname "$0")/.." && pwd)}"
MANIFEST="${CP_MANIFEST_OVERRIDE:-$ROOT/tools/control-plane.txt}"

# 清单两列（前缀、前缀级测试命令）落在临时文件，行号天然对齐——bash 3.2 无关联
# 数组，也不靠 paste/子 shell（变量会在子 shell 丢）。
TMPDIR_CP="$(mktemp -d "${TMPDIR:-/tmp}/cpgate.XXXXXX")" || exit 2
PREFIXES_F="$TMPDIR_CP/prefixes"; TESTS_F="$TMPDIR_CP/tests"
trap 'rm -rf "$TMPDIR_CP"' EXIT

load_manifest() {
  if [ ! -f "$MANIFEST" ]; then
    echo "control-plane-gate: 红——清单缺席 $MANIFEST（删清单=动控制面，default-conservative）" >&2
    return 1
  fi
  : > "$PREFIXES_F"; : > "$TESTS_F"
  while IFS= read -r line; do
    case "$line" in ''|\#*) continue;; esac
    set -- $line
    printf '%s\n' "$1" >> "$PREFIXES_F"
    tc=""
    case "$line" in *"=>"*) tc="${line#*=>}";; esac
    printf '%s\n' "$tc" >> "$TESTS_F"
  done < "$MANIFEST"
  return 0
}

# match_prefix <path> —— stdout 打印命中的最长前缀；无命中打印空。
match_prefix() {
  _mp_path="$1"; _mp_best=""
  while IFS= read -r _mp_p; do
    case "$_mp_path" in
      "$_mp_p"|"_mp_p"*)
        if [ "${#_mp_p}" -gt "${#_mp_best}" ]; then _mp_best="$_mp_p"; fi
        ;;
    esac
  done < "$PREFIXES_F"
  printf '%s' "$_mp_best"
}

# prefix_test <prefix> —— stdout 打印该前缀行的「=> 命令」（无则空）。双文件行号对齐读。
prefix_test() {
  _pt_want="$1"
  while IFS= read -r _pt_p && IFS= read -r _pt_t <&5; do
    if [ "$_pt_p" = "$_pt_want" ]; then printf '%s' "$_pt_t"; return 0; fi
  done < "$PREFIXES_F" 5< "$TESTS_F"
  return 0
}

run_gate() {
  # staged 清单：override 优先（自检腿不真动 git index）；否则真读暂存区。
  if [ "${CP_STAGED_OVERRIDE+set}" = "set" ]; then
    _rg_staged="$CP_STAGED_OVERRIDE"
  else
    _rg_staged="$(git -C "$ROOT" diff --cached --name-only 2>/dev/null)"
    if [ $? -ne 0 ]; then
      echo "control-plane-gate: 红——git diff --cached 不可用（提交路径上基础设施无答=拦，不归责但不放行）" >&2
      return 1
    fi
  fi
  [ -z "$_rg_staged" ] && return 0

  _rg_ran_prefix_tests=" "
  _rg_hits=0; _rg_red=0; _rg_skipped=""
  while IFS= read -r _rg_path; do
    [ -z "$_rg_path" ] && continue
    _rg_p="$(match_prefix "$_rg_path")"
    [ -z "$_rg_p" ] && continue
    _rg_hits=$((_rg_hits + 1))
    _rg_tc="$(prefix_test "$_rg_p")"
    if [ -n "$_rg_tc" ]; then
      # 前缀级测试同前缀只跑一次：.githooks/ 五个钩子文件不必跑五遍门禁套件。
      case "$_rg_ran_prefix_tests" in
        *" $_rg_p "*) ;;
        *)
          _rg_ran_prefix_tests="$_rg_ran_prefix_tests$_rg_p "
          echo "control-plane-gate: 命中前缀 $_rg_p（$_rg_path）→ 前缀级测试：$_rg_tc"
          if ! (cd "$ROOT" && eval "$_rg_tc"); then
            echo "control-plane-gate: 红——前缀级测试失败：$_rg_tc" >&2
            _rg_red=1
          fi
          ;;
      esac
      continue
    fi
    case "$_rg_path" in
      *.py)
        echo "control-plane-gate: 命中 $_rg_path → python3 --selftest"
        if ! python3 "$ROOT/$_rg_path" --selftest; then
          echo "control-plane-gate: 红——$_rg_path --selftest 失败" >&2
          _rg_red=1
        fi
        ;;
      *.sh)
        if grep -q -- '--selftest' "$ROOT/$_rg_path" 2>/dev/null; then
          echo "control-plane-gate: 命中 $_rg_path → bash --selftest"
          if ! bash "$ROOT/$_rg_path" --selftest; then
            echo "control-plane-gate: 红——$_rg_path --selftest 失败" >&2
            _rg_red=1
          fi
        else
          # 跳过不红：fail-direction 见头注——存量 .sh 无自检是欠账，列名提请补齐。
          echo "control-plane-gate: 跳过（无 --selftest 面）：$_rg_path ——请补自检腿"
          _rg_skipped="$_rg_skipped $_rg_path"
        fi
        ;;
      *)
        # 配置/登记面（settings.json、清单自身、宪法 skill 散文）：无逐件自检，
        # 命中即打印（内容归快照门与人工评审管）。
        echo "control-plane-gate: 命中（登记/配置面，无逐件自检）：$_rg_path（前缀 $_rg_p）"
        ;;
    esac
  done <<EOF
$_rg_staged
EOF

  if [ "$_rg_hits" -eq 0 ]; then
    echo "control-plane-gate: 无控制面命中，直通"
    return 0
  fi
  [ -n "$_rg_skipped" ] && echo "control-plane-gate: 跳过清单：$_rg_skipped"
  return $_rg_red
}

selftest() {
  _st_fails=0
  _st_check() {
    if [ "$2" -eq 0 ]; then echo "control-plane-gate 自检: 腿绿 $1"; else echo "control-plane-gate 自检: FAIL $1（腿红）"; _st_fails=$((_st_fails + 1)); fi
  }
  # 合成树：占位件全在临时目录，零真仓依赖、零 git index 接触。
  _st_td="$(mktemp -d "${TMPDIR:-/tmp}/cpgate-st.XXXXXX")" || return 2
  mkdir -p "$_st_td/tools" "$_st_td/dirp"
  printf '#!/bin/bash\ncase "$1" in --selftest) exit 0;; esac\nexit 0\n' > "$_st_td/tools/yes.sh"
  printf 'import sys\nsys.exit(1)\n' > "$_st_td/tools/no.py"
  printf '#!/bin/bash\necho "plain helper, no selftest"\n' > "$_st_td/tools/plain.sh"
  printf 'x\n' > "$_st_td/dirp/x.txt"
  cat > "$_st_td/cp.txt" <<'EOM'
# 合成清单（自检腿）
tools/yes.sh
tools/no.py
tools/plain.sh
dirp/ => true
EOM
  _st_run() { # _st_run <期望 rc> <腿名> <staged 清单>
    _sr_want="$1"; _sr_name="$2"; _sr_staged="$3"; _sr_got=1
    CP_ROOT_OVERRIDE="$_st_td" CP_MANIFEST_OVERRIDE="$_st_td/cp.txt" CP_STAGED_OVERRIDE="$_sr_staged" \
      bash "$SELF" > "$_st_td/out.txt" 2>&1 && _sr_got=0
    if [ "$_sr_got" -eq "$_sr_want" ]; then _st_check "$_sr_name" 0; else _st_check "$_sr_name" 1; fi
  }
  _st_run 0 "命中跑 true 占位件（.sh 带 selftest 绿）" "tools/yes.sh"
  _st_run 0 "无命中直通" "README.md"
  _st_run 1 "红件拒（.py selftest exit 1）" "tools/no.py"
  CP_ROOT_OVERRIDE="$_st_td" CP_MANIFEST_OVERRIDE="$_st_td/cp.txt" CP_STAGED_OVERRIDE="tools/plain.sh" \
    bash "$SELF" > "$_st_td/out.txt" 2>&1
  _st_got=$?; _sr_want=0
  if [ "$_st_got" -eq "$_sr_want" ] && grep -q "跳过" "$_st_td/out.txt"; then _st_check ".sh 无 selftest 跳过且列名（rc 0）" 0; else _st_check ".sh 无 selftest 跳过且列名（rc 0）" 1; fi
  _st_run 0 "前缀级测试腿（=> true）" "dirp/x.txt"
  CP_ROOT_OVERRIDE="$_st_td" CP_MANIFEST_OVERRIDE="$_st_td/absent.txt" CP_STAGED_OVERRIDE="tools/yes.sh" \
    bash "$SELF" > /dev/null 2>&1
  if [ $? -eq 1 ]; then _st_check "清单缺席红（default-conservative）" 0; else _st_check "清单缺席红（default-conservative）" 1; fi
  rm -rf "$_st_td"
  if [ "$_st_fails" -eq 0 ]; then echo "control-plane-gate 自检：PASS"; return 0; fi
  echo "control-plane-gate 自检：FAIL（$_st_fails 腿红）" >&2
  return 1
}

if [ "${1:-}" = "--selftest" ]; then
  selftest
  exit $?
fi

if ! load_manifest; then
  exit 1
fi
run_gate
exit $?
