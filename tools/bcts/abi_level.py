#!/usr/bin/env python3
# BCTS abi level 读取（abi_level 模块面——bcts-run.sh PYEOF④段机械抽出，
# bctsrun Rust 件四刀的 parity oracle 基座；t1_check/t2_check/reconcile_t2
# 同款先例）。抽取源 = tools/bcts-run.sh abi_level() heredoc 体逐字
# （`python3 - "$SCRIPT_DIR/abi.toml" <<'PYEOF4'` 调用位），零缩进归位
# （段体本就顶格）：bcts-run.sh 改经
# `python3 "$SCRIPT_DIR/bcts/abi_level.py" "$SCRIPT_DIR/abi.toml"` 同参调用，
# 文案/退码逐字节不变。模块化动机同前——Rust 件 parity 对拍需要稳定驱动面。
#
# 出口协议（stdout = level 十进制整数；拒 = stderr + 退 1）。
import sys, tomllib

try:
    with open(sys.argv[1], "rb") as f:
        lv = tomllib.load(f).get("platform", {}).get("level")
except Exception as e:
    print(f"bcts: FAIL: 读 {sys.argv[1]} [platform].level 失败：{e}", file=sys.stderr)
    sys.exit(1)
if not isinstance(lv, int) or isinstance(lv, bool):
    print(f"bcts: FAIL: {sys.argv[1]} [platform].level 非整数：{lv!r}", file=sys.stderr)
    sys.exit(1)
print(lv)
