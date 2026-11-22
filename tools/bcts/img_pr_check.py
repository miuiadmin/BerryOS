#!/usr/bin/env python3
# BCTS 版本声称位对账（img_pr_check 模块面——bcts-run.sh PYEOF⑤段机械抽出，
# bctsrun Rust 件四刀的 parity oracle 基座；abi_level 同刀同款先例）。
# 抽取源 = tools/bcts-run.sh img_pr_check() heredoc 体逐字（`python3 - "$1"
# "$2" "$3" <<'PYEOF5'` 调用位），+2 缩进归位外零变更：bcts-run.sh 改经
# `python3 "$SCRIPT_DIR/bcts/img_pr_check.py" "$1" "$2" "$3"` 同参调用，
# 文案/退码逐字节不变。模块化动机同前——Rust 件 parity 对拍需要稳定驱动面。
#
# 出口协议（stdout 对账 ok 行；拒 = stderr「bcts: FAIL: …」+ 退 1）。
import re, sys
manifest, want, anchor = sys.argv[1], int(sys.argv[2]), sys.argv[3]


def die(msg):
    print(f"bcts: FAIL: {msg}", file=sys.stderr)
    sys.exit(1)


try:
    with open(manifest, encoding="ascii") as f:
        text = f.read()
except OSError as e:
    die(f"读 {manifest} 失败（mkimage 侧产物应与镜像同目录）：{e}")
prs = re.findall(r"(?m)^item .* platform_revision=(\d{10})$", text)
if not prs:
    die(f"{manifest} 无 platform_revision 条目行（manifest v1 产物面断裂——镜像非 mkimage 产链？）")
vals = sorted({int(p) for p in prs})
if len(vals) > 1:
    die(f"{manifest} platform_revision 条目混杂 {vals}（同盘同版断裂——mkimage 逐条目同源取值）")
got = vals[0]
if got != want:
    die(f"镜像 manifest platform_revision={got} != 回放清单 pr={want}（对账锚：{anchor}）"
        "——版本声称位对账断裂，不等 loud 拒（认证语义：镜像声称版本必须与回放冻结件同版）")
print(f"bcts: 版本声称位对账 ok：manifest platform_revision={got} == {anchor}")
