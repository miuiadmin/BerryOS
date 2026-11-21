#!/usr/bin/env bash
# image-fresh.sh —— 镜像新鲜度门（工程纪律刀阶段 2；规则/16 §2 判据单源
# 与新鲜度）。out/manifest.txt 是 `just image` 的侧产物账本（mkimage v1
# 冻结格式，每次造盘整账重写），其中 /KERNEL.ELF 与 /INITRD.BIN 条目的
# sha256 必须等于现场重算值——对不上 = 改了二进制还在跑旧镜像（gui8
# 预烘镜像陷阱的机器根治：此前靠 GUI8_REBUILD=1 人肉防，规则/16 立法
# 「陈旧镜像得出的验证结论作废」）。
# 用法：
#   bash tools/image-fresh.sh            # 对拍：不符 exit 1（附 just image 提示）
#   bash tools/image-fresh.sh --ensure   # 陈旧/缺件即 just image 重建后再对拍
#   bash tools/image-fresh.sh --selftest # 合成夹具：正例过 / 篡改红 / 缺件判 / 源变更红
# 缺席语义：镜像或 manifest 缺席 = 「未烘」不算陈旧（check 模式放行附注，
# --ensure 模式触发重建）——只有「在场但哈希对不上」才是陈旧红。
set -u
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$SCRIPT_DIR/.."

sha256_of() { # 便携哈希：Linux sha256sum / macOS shasum
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'
  else echo "image-fresh: 无 sha256 工具（sha256sum/shasum 均缺席）" >&2; return 3; fi
}

manifest_sha() { # $1=manifest $2=条目（如 /KERNEL.ELF）→ 哈希或空
  # 经 ENVIRON 传条目名（macOS awk 的 -v 不接受含 '=' 的值——path=/KERNEL.ELF）
  EPATH="$2" awk '$1=="item" && $2==("path=" ENVIRON["EPATH"]) {for (i=3; i<=NF; i++) if ($i ~ /^sha256=/) {sub(/^sha256=/, "", $i); print $i; exit}}' "$1"
}

# check_pair <manifest> <entry> <file> <label>：0 新鲜 / 1 陈旧 / 2 缺件
check_pair() {
  local manifest=$1 ent=$2 file=$3 label=$4 want have
  [ -f "$manifest" ] || { echo "image-fresh: manifest 缺席（$manifest）" >&2; return 2; }
  want=$(manifest_sha "$manifest" "$ent")
  [ -n "$want" ] || { echo "image-fresh: manifest 无条目 $ent" >&2; return 2; }
  [ -f "$file" ] || { echo "image-fresh: $label 源文件缺席（$file）" >&2; return 2; }
  have=$(sha256_of "$file") || return 2
  if [ "$want" = "$have" ]; then return 0; fi
  echo "image-fresh: $label 陈旧——manifest $ent 与现场不符" >&2
  echo "  manifest: $want" >&2
  echo "  现场    : $have" >&2
  return 1
}

# run_check：0 全新鲜 / 1 有陈旧 / 2 缺件（无陈旧）
run_check() {
  local stale=0 missing=0 r kernel
  kernel=$(bash "$SCRIPT_DIR/paths.sh" kernel) || return 2
  r=0; check_pair out/manifest.txt /KERNEL.ELF "$kernel" "内核 ELF" || r=$?
  [ "$r" -eq 1 ] && stale=1; [ "$r" -eq 2 ] && missing=1
  r=0; check_pair out/manifest.txt /INITRD.BIN rootfs.cpio "rootfs.cpio" || r=$?
  [ "$r" -eq 1 ] && stale=1; [ "$r" -eq 2 ] && missing=1
  [ "$stale" -eq 1 ] && return 1
  [ "$missing" -eq 1 ] && return 2
  return 0
}

case "${1:-check}" in
  --selftest)
    T=$(mktemp -d) || exit 2
    trap 'rm -rf "$T"' EXIT
    echo aaaa > "$T/k.elf"
    H=$(sha256_of "$T/k.elf") || exit 2
    printf 'berryos-manifest v1\nitem path=/KERNEL.ELF   sha256=%s size=0004\n' "$H" > "$T/m1"
    check_pair "$T/m1" /KERNEL.ELF "$T/k.elf" 夹具 || { echo "自检 FAIL: 正例未过" >&2; exit 1; }
    printf 'item path=/KERNEL.ELF   sha256=deadbeef\n' > "$T/m2"
    check_pair "$T/m2" /KERNEL.ELF "$T/k.elf" 夹具 2>/dev/null && { echo "自检 FAIL: 篡改哈希未红" >&2; exit 1; }
    check_pair "$T/m3" /KERNEL.ELF "$T/k.elf" 夹具 2>/dev/null && { echo "自检 FAIL: 缺 manifest 未判缺件" >&2; exit 1; }
    echo cccc >> "$T/k.elf"
    check_pair "$T/m1" /KERNEL.ELF "$T/k.elf" 夹具 2>/dev/null && { echo "自检 FAIL: 源变更未红" >&2; exit 1; }
    echo "image-fresh 自检：PASS（正例 / 篡改哈希 / 缺 manifest / 源变更四腿全触发）"
    ;;
  --ensure)
    run_check
    rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "image-fresh: 陈旧或缺件——触发重建（just image）"
      just image || { echo "image-fresh: just image 失败" >&2; exit 1; }
    fi
    run_check || { echo "image-fresh: 重建后仍不符——manifest 与产物链断裂，查 mkimage 产链" >&2; exit 1; }
    echo "image-fresh: PASS（KERNEL.ELF / INITRD.BIN 与 manifest 一致）"
    ;;
  check|"")
    run_check
    rc=$?
    case "$rc" in
      0) echo "image-fresh: PASS（KERNEL.ELF / INITRD.BIN 与 manifest 一致）" ;;
      1) echo "image-fresh: FAIL——镜像陈旧，先 just image（规则/16 §2：陈旧镜像得出的验证结论作废）" >&2; exit 1 ;;
      2) echo "image-fresh: 缺件（镜像或输入未烘全）——check 模式放行；--ensure 模式将触发重建" ;;
    esac
    ;;
  *) echo "usage: $0 [check|--ensure|--selftest]" >&2; exit 2 ;;
esac
