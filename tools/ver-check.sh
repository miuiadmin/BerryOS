#!/usr/bin/env bash
# ver-check —— 版本一致性验收（版本管理标准 §3 G-ver-3/4；入口 `just
#   ver-check`，发版前手跑——脚本门已轻挂 CI fail-fast 位（QEMU 类门不挂）、不进 build 依赖链，标准 §3
#   「新门挂载」节）。
#
# G-ver-3（tag 一致）：每个 `v*` tag 指向提交的 Cargo workspace version
#   必须等于 tag 名（去 v 前缀）。banner 版本编译期同源
#   env!("CARGO_PKG_VERSION")（kernel/src/main.rs，G-ver-2 的同源面），
#   故 Cargo version 即 banner 版本——对拍即验标准原文「tag 指向的提交，
#   其 banner 版本必须等于 tag 名」。
# G-ver-4（发版净树）：**事后不可验**——git 不留存「打 tag 时点工作树是否
#   clean」的状态（tag 只是 commit 的只读引用，「打标时树净」是打标动作的
#   时态属性，无法从对象库重建）。运行时替代：boot banner 的构建标识本身
#   带 -dirty 位（kernel/build.rs `git_ident()` 运行时查 git 注入）——smoke
#   日志/boot 串口人眼可辨，含 -dirty 的产物禁止打版本标（§3 G-ver-4 原文）。
#   本脚本只做静态降级面：tag 指向提交的 kernel/build.rs 须存在 git_ident
#   函数（保证彼时构建已具 dirty 位机制，-dirty 真的可辨）。
# 豁免（PASS 注记，不红）：无 git 元数据 / 零 `v*` tag——无可验对象。
# 只读脚本：git 仅 `tag -l`/`show`/`rev-parse` 只读面，零 git 写。
set -u

cd "$(dirname "$0")/.."

# ---------- --selftest（合成 git 沙箱，零真仓零写 tag；改对拍引擎/豁免面/CLI 形态必跑） ----------
# 沙箱模式 = 脚本拷贝进各合成仓 tools/ 再跑（fwcfg-check 同款）——脚本头
# 自 cd "$(dirname $0)/.." 到合成仓根，对拍面完全隔离于真仓。
if [ "${1:-}" = "--selftest" ]; then
  SELF=$(mktemp -d "${TMPDIR:-/tmp}/berryos-vercheck-selftest.XXXXXX")
  cleanup() { rm -rf "$SELF"; }
  trap cleanup EXIT
  S="$0"

  # mkrepo <dir> —— 合成仓母版：Cargo.toml [workspace.package] version 1.2.3
  # + kernel/build.rs 含 git_ident（G-ver-4 静态降级面绿件）+ 脚本拷贝入席。
  mkrepo() {
    mkdir -p "$1/kernel" "$1/tools"
    printf '[workspace.package]\nversion = "1.2.3"\n' > "$1/Cargo.toml"
    printf 'fn git_ident() {}\n' > "$1/kernel/build.rs"
    cp "$S" "$1/tools/ver-check.sh"
    ( cd "$1" && git init -q . && git config user.email t@t \
      && git config user.name t && git add -A && git commit -qm init ) >/dev/null 2>&1
  }

  legs=0; fails=0
  leg() {  # leg <名> <期望rc> <锚面:stdout|stderr|-> <锚> <cmd…>
    name="$1"; want="$2"; face="$3"; pat="$4"; shift 4
    rc=0
    ( "$@" ) >"$SELF/out.log" 2>&1 || rc=$?
    legs=$((legs+1)); why=""
    [ "$rc" = "$want" ] || why="rc 期望 ${want} 实得 ${rc}"
    if [ -z "$why" ] && [ "$face" != "-" ] && ! grep -qF "$pat" "$SELF/out.log"; then
      why="输出缺锚「${pat}」"
    fi
    if [ -n "$why" ]; then
      fails=$((fails+1)); echo "ver-check selftest 腿红：${name}（${why}）" >&2
      tail -4 "$SELF/out.log" >&2
    else
      echo "ver-check selftest 腿绿：${name}"
    fi
  }

  # 腿 1：G-ver-3 对拍绿——tag v1.2.3 ↔ version 1.2.3 + git_ident 在席。
  mkrepo "$SELF/green"; ( cd "$SELF/green" && git tag v1.2.3 ) >/dev/null 2>&1
  leg "G-ver-3 对拍绿" 0 stdout "v1.2.3 ok" sh "$SELF/green/tools/ver-check.sh"
  # 腿 2-3：版本不匹配红（v9.9.9 ↔ 1.2.3）+ 点名差值。
  mkrepo "$SELF/mismatch"; ( cd "$SELF/mismatch" && git tag v9.9.9 ) >/dev/null 2>&1
  leg "版本不匹配红" 1 stderr "G-ver-3" sh "$SELF/mismatch/tools/ver-check.sh"
  leg "不匹配点名（9.9.9 ≠ 1.2.3）" 1 stderr "9.9.9" sh "$SELF/mismatch/tools/ver-check.sh"
  # 腿 4：tag 指向提交无 [workspace.package] version。
  mkrepo "$SELF/nover"
  printf '# 无 version 行\n[workspace.package]\nedition = "2021"\n' > "$SELF/nover/Cargo.toml"
  ( cd "$SELF/nover" && git add -A && git commit -qm nover && git tag v1.2.3 ) >/dev/null 2>&1
  leg "无 version 红" 1 stderr "workspace.package" sh "$SELF/nover/tools/ver-check.sh"
  # 腿 5：git_ident 缺席（G-ver-4 静态降级面红）。
  mkrepo "$SELF/noident"
  printf '// 无 git_ident\nfn other() {}\n' > "$SELF/noident/kernel/build.rs"
  ( cd "$SELF/noident" && git add -A && git commit -qm noident && git tag v1.2.3 ) >/dev/null 2>&1
  leg "git_ident 缺席红" 1 stderr "G-ver-4" sh "$SELF/noident/tools/ver-check.sh"
  # 腿 6：段作用域防串号——version 行在非 workspace.package 段（如
  # [workspace.dependencies]）不得被拾取当版本对拍（awk 段卫兵的对面纪律）。
  mkrepo "$SELF/wrongseg"
  printf '[workspace.package]\nversion = "1.2.3"\n\n[workspace.dependencies]\nversion = "9.9.9"\n' > "$SELF/wrongseg/Cargo.toml"
  ( cd "$SELF/wrongseg" && git add -A && git commit -qm wrongseg && git tag v1.2.3 ) >/dev/null 2>&1
  leg "段作用域防串号绿" 0 stdout "v1.2.3 ok" sh "$SELF/wrongseg/tools/ver-check.sh"
  # 腿 7：零 v* tag 豁免（PASS 非红）。
  mkrepo "$SELF/notag"
  leg "零 tag 豁免" 0 stdout "零 v* tag" sh "$SELF/notag/tools/ver-check.sh"
  # 腿 8：非 git 树豁免（脚本拷入裸目录——自 cd 后 rev-parse 失败走豁免路径）。
  mkdir -p "$SELF/nogit/tools"
  cp "$S" "$SELF/nogit/tools/ver-check.sh"
  leg "非 git 豁免" 0 stdout "无 git 元数据" sh "$SELF/nogit/tools/ver-check.sh"

  echo "ver-check --selftest: ${legs} 腿，${fails} 红"
  [ "$fails" -eq 0 ] || exit 1
  exit 0
fi

fail() { echo "ver-check: FAIL——$1" >&2; exit 1; }

if ! command -v git >/dev/null 2>&1 \
   || ! git rev-parse --git-dir >/dev/null 2>&1; then
  echo "ver-check: PASS（豁免：无 git 元数据——无可验 tag；nogit 产物不得对外发布，标准 §6）"
  exit 0
fi

tags=$(git tag -l 'v*')
if [ -z "$tags" ]; then
  echo "ver-check: PASS（零 v* tag——首个版本标打出前无可验对象）"
  exit 0
fi

n=0
for tag in $tags; do
  n=$((n + 1))
  # G-ver-3：tag 名 ↔ 该提交 [workspace.package] version 对拍（段作用域
  # awk：只在 [workspace.package] 段内收首个 version 行，防他段串号）。
  ver=$(git show "$tag:Cargo.toml" 2>/dev/null | awk -F'"' '
    /^\[/ { in_ws = ($0 ~ /^\[workspace\.package\]$/) ? 1 : 0 }
    in_ws && /^version[[:space:]]*=/ { print $2; exit }
  ')
  [ -n "$ver" ] \
    || fail "G-ver-3：$tag 指向提交的 Cargo.toml 无 [workspace.package] version（无法对拍；版本管理标准 §3）"
  want=${tag#v}
  [ "$ver" = "$want" ] \
    || fail "G-ver-3：${tag} 指向提交的版本 ${ver} ≠ tag 名 ${want}（版本管理标准 §3：banner 版本必须等于 tag 名——同源 env!(\"CARGO_PKG_VERSION\")）"
  # G-ver-4 静态降级面：构建标识/dirty 位机制在席（事后树净性不可验，见头注）。
  git show "$tag:kernel/build.rs" 2>/dev/null | grep -q 'fn git_ident' \
    || fail "G-ver-4（静态降级面）：$tag 指向提交的 kernel/build.rs 无 git_ident 函数（dirty 位机制缺席——banner 无从携带 -dirty，树净性失去人眼可辨的运行时替代）"
  echo "ver-check: $tag ok（Cargo version $ver = tag 名；git_ident 机制在席）"
done

echo "ver-check: PASS（$n 个 v* tag 全过 G-ver-3 对拍 + G-ver-4 静态降级面）"
