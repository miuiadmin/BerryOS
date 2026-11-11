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
