#!/usr/bin/env python3
"""bash 3.2 多字节吞名守卫（CI/本地 smoke 宿主面，2026-11）。

根因（macOS 系统 bash 3.2.57 实测定界）：`$NAME` 后紧跟任一非 ASCII
字节（全角括号/逗号/CJK 正文/∈→ 等），bash 3.2 把多字节首字节吞进
变量名一起做 `set -u` 查表——`unbound variable` 直炸（未开 set -u 时
则是静默吃值：`${!-ref}` 空串替换，双重危害）。CI（ubuntu bash 5）
与 brew bash 5 均无此症——这是本地复现 CI 挂账（W1-Q6）的宿主断点。

修复口径 = 花括号包裹（`${NAME}`）：bash 3/5 与 dash 字节级同义，
不需要把中文相名日志降级成 ASCII。本工具把该口径钉成门禁，防回归。

用法:
  python3 tools/shvarcheck.py            # 扫描 + 报告（默认）
  python3 tools/shvarcheck.py --check    # 有命中即 FAIL（CI/just test-host 前置）
  python3 tools/shvarcheck.py --selftest # 门禁自身钉板（a-3 U4 纪律）

口径:
  - 扫描面 = tools/*.sh（含 markers.sh/paths.sh 等门禁/冒烟宿主件）；
  - 命中 = `$NAME`（NAME ∈ [A-Za-z_][A-Za-z0-9_]*) 紧邻其后一个码点
    > U+007F（多字节字符被吞进变量名的必要条件）；
  - `${NAME}` 已包裹、`$1`/`$?`/`$!` 单字符特殊参量后接多字节不吞
    （实测钉板）、heredoc 引号体（<<'PY'）内不展开——只扫 shell 展开面。
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCAN_GLOB = "tools/*.sh"

# $NAME 后紧邻非 ASCII 码点（bash 3.2 吞名必要条件）
RE_BARE_VAR = re.compile(r"\$([A-Za-z_][A-Za-z0-9_]*)")


def scan_text(text: str) -> list[tuple[int, str, str]]:
    """返回 [(行号, 变量名, 行摘录)]——命中的裸变量 + 多字节邻接。"""
    hits: list[tuple[int, str, str]] = []
    for lineno, line in enumerate(text.splitlines(), 1):
        for m in RE_BARE_VAR.finditer(line):
            end = m.end()
            if end < len(line) and ord(line[end]) > 0x7F:
                hits.append((lineno, m.group(1), line.strip()[:78]))
    return hits


def scan_repo() -> list[tuple[str, list[tuple[int, str, str]]]]:
    bad: list[tuple[str, list[tuple[int, str, str]]]] = []
    for f in sorted(REPO.glob(SCAN_GLOB)):
        hits = scan_text(f.read_text(encoding="utf-8"))
        if hits:
            bad.append((f.name, hits))
    return bad


def selftest() -> int:
    """门禁自身钉板：吞名面 6 形 + 免疫面 7 形（含特殊参量/包裹态）。"""
    cases_bad = [
        ('echo "（$KERNEL）"', "KERNEL"),
        ('echo "（$K，-smp 2）"', "K"),
        ('echo "ovfl $OV∈[1,9]"', "OV"),
        ('echo "$v_first→尾"', "v_first"),
        ('echo "$summary；帽"', "summary"),
        ('echo "x$f。" >&2', "f"),
    ]
    for src, want in cases_bad:
        hits = scan_text(src)
        assert len(hits) == 1 and hits[0][1] == want, f"吞名面漏报: {src!r} -> {hits}"
    cases_ok = [
        'echo "（${KERNEL}）"',          # 花括号包裹（修复口径）
        'echo "（$1）"',                 # 特殊参量：bash 3.2 不吞（实测）
        'echo "（$?）"',
        'echo "（$!）"',
        'echo "（$KERNEL done"',         # 变量后是 ASCII 空格
        'echo "（${p%%:*} 尾）"',        # 参数展开自带边界
    ]
    for src in cases_ok:
        hits = scan_text(src)
        assert not hits, f"免疫面误报: {src!r} -> {hits}"
    # 吞名面补全角句号贴变量形（'尾部：$missing。'——句号直接贴变量）
    hits = scan_text('echo "尾部：$missing。"')
    assert len(hits) == 1 and hits[0][1] == "missing", f"句号贴面漏报: {hits}"
    print("shvarcheck: selftest ok（吞名 7 形钉板 + 免疫 6 形 + 修复口径花括号面）")
    return 0


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    check = "--check" in argv
    bad = scan_repo()
    if not bad:
        print("shvarcheck: ok（tools/*.sh 裸变量零多字节邻接——bash 3.2 宿主面免疫）")
        return 0
    for name, hits in bad:
        for lineno, var, excerpt in hits:
            print(f"shvarcheck: FAIL {name}:{lineno}：${var} 后紧邻多字节字符"
                  f"（bash 3.2 吞名 -> unbound variable/静默吃值）——改写为 ${{{var}}}")
            print(f"  -> {excerpt}")
    if check:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
