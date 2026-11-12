#!/usr/bin/env python3
"""cpio_filter——从 mkcpio newc 归档确定性移除验证 marker。

用法：cpio_filter.py <in.cpio> <out.cpio> --remove <name> [--selftest]

产品桌面镜像与 GUI8 验证镜像共享同一套 rootfs 条目；验证镜像携带
`probe.on`，svcmgr 只在该条目在场时执行串行 app boot-probe 批。本工具只
移除指定条目并经 `mkcpio.build_archive` 单编码器重发射，不做第二套 newc
编码逻辑。重复 marker 全移除；目标缺席则输出逐字节相等，显式防静默拼错。
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from mkcpio import build_archive


HEADER_SIZE = 110
MAGIC = b"070701"
TRAILER = b"TRAILER!!!"


class FilterError(RuntimeError):
    """输入不是本工具支持的 mkcpio newc 形态。"""


def _hex_field(header: bytes, index: int) -> int:
    start = 6 + index * 8
    raw = header[start : start + 8]
    if len(raw) != 8:
        raise FilterError("truncated cpio header")
    try:
        return int(raw, 16)
    except ValueError as error:
        raise FilterError("bad cpio hex field") from error


def parse_entries(blob: bytes) -> list[tuple[bytes, bytes]]:
    """解析 mkcpio 生成的定值 newc 流（普通文件 + 一个 trailer）。"""
    items: list[tuple[bytes, bytes]] = []
    pos = 0
    trailer_seen = False
    while pos < len(blob):
        if trailer_seen:
            raise FilterError("bytes after cpio trailer")
        header = blob[pos : pos + HEADER_SIZE]
        if len(header) != HEADER_SIZE or header[:6] != MAGIC:
            raise FilterError("bad cpio header")
        size = _hex_field(header, 6)
        namesize = _hex_field(header, 11)
        name_start = pos + HEADER_SIZE
        name_end = name_start + namesize
        if namesize == 0 or name_end > len(blob) or blob[name_end - 1] != 0:
            raise FilterError("bad cpio name")
        name = bytes(blob[name_start : name_end - 1])
        data_start = (name_end + 3) & ~3
        data_end = data_start + size
        if data_end > len(blob):
            raise FilterError("truncated cpio data")
        if name == TRAILER:
            if size != 0:
                raise FilterError("nonempty cpio trailer")
            trailer_seen = True
            break
        items.append((name, bytes(blob[data_start:data_end])))
        pos = (data_end + 3) & ~3
    if not trailer_seen:
        raise FilterError("missing cpio trailer")
    return items


def filter_archive(blob: bytes, remove: bytes) -> tuple[bytes, int]:
    """移除全部 `remove` 条目；重编码保持其余条目顺序与 payload 不变。"""
    items = parse_entries(blob)
    kept = [(name, data) for name, data in items if name != remove]
    return build_archive(kept), len(items) - len(kept)


def selftest() -> int:
    original = build_archive(
        [(b"probe.on", b"verify\n"), (b"bin/hello", b"abc"), (b"z", b"")]
    )
    filtered, removed = filter_archive(original, b"probe.on")
    if removed != 1:
        raise FilterError("selftest remove count")
    if dict(parse_entries(filtered)) != {b"bin/hello": b"abc", b"z": b""}:
        raise FilterError("selftest entries")
    unchanged, removed = filter_archive(filtered, b"probe.on")
    if removed != 0 or unchanged != filtered:
        raise FilterError("selftest absent marker")
    print("cpio-filter: selftest ok")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("input", nargs="?", type=Path)
    ap.add_argument("output", nargs="?", type=Path)
    ap.add_argument("--remove")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if args.input is None or args.output is None or args.remove is None:
        ap.error("input, output, and --remove are required without --selftest")
    blob = args.input.read_bytes()
    filtered, removed = filter_archive(blob, args.remove.encode())
    args.output.write_bytes(filtered)
    print(
        f"cpio-filter: {args.input} -> {args.output} "
        f"removed={removed} bytes={len(filtered)}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except FilterError as error:
        print(f"cpio-filter: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
