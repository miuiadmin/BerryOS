#!/bin/sh
# fwcfg 键表对拍：tools/fwcfg-keys.toml（单源键表）vs 仓内实际注入/消费的
# opt/berry/<name> 键名集合。零漂移 = 两侧键名集合相等；漂移 = 键表漏登记
# 或死键新增。用法：bash tools/fwcfg-check.sh（退出 0=对拍过，1=有漂移）。
set -e
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# 代码侧：全仓提取 opt/berry/<name>（键名字符集含下划线/大写——正则宽
# 于现行全小写键名，防未来键名形态让对拍假阴性）；rootfsN 数字卷归并
# rootfs0..9。二轮审计改排除式全仓扫（此前白名单列举式漏 bootloader/
# drivers 等未来注入面，且吞 rg 报错静默缩面）——只排文档/归档/外部
# 镜像/构建产物，代码面全收。
rg -o 'opt/berry/[A-Za-z0-9_-]+' --no-filename --hidden \
  -g '!docs/**' -g '!.agents/**' -g '!.git/**' -g '!reference/**' \
  -g '!research/**' -g '!out/**' -g '!*target*/**' -g '!*.img' -g '!*.cpio' \
  -g '!*.md' -g '!WAVE2-GALLERY/**' -g '!.berry-review/**' \
  . 2>/dev/null \
  | sed -E -e 's|opt/berry/||' -e 's/^rootfs[0-9]+$/rootfs0..9/' | sort -u \
  > "$tmp/code.txt" || true

# 键表侧：[[keys]] name 字段。
sed -n 's/^name = "\(.*\)"$/\1/p' tools/fwcfg-keys.toml | sort -u > "$tmp/toml.txt"

status=0
if comm -23 "$tmp/code.txt" "$tmp/toml.txt" | grep -q .; then
  echo "fwcfg-check: 键表漏登记（代码在用、键表没有）:" >&2
  comm -23 "$tmp/code.txt" "$tmp/toml.txt" | sed 's/^/  /' >&2
  status=1
fi
if comm -13 "$tmp/code.txt" "$tmp/toml.txt" | grep -q .; then
  echo "fwcfg-check: 键表多余（键表在册、代码无踪——死键须注记手拼专递或删）:" >&2
  comm -13 "$tmp/code.txt" "$tmp/toml.txt" | sed 's/^/  /' >&2
  status=1
fi
[ $status -eq 0 ] && echo "fwcfg-check: 对拍过（$(wc -l < "$tmp/toml.txt" | tr -d ' ') 键两侧一致）"
exit $status
