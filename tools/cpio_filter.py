#!/usr/bin/env python3
"""cpio_filter——mkcpio newc 归档的确定性 marker 移除/追加。

用法：
cpio_filter.py <in.cpio> <out.cpio> --remove <name> [--add <name>=<file>]...
cpio_filter.py --selftest

产品桌面镜像与 GUI8 验证镜像共享同一套 rootfs 条目；验证镜像携带
`probe.on`，svcmgr 只在该条目在场时执行串行 app boot-probe 批。GPU2 诊断
镜像在同一产物上追加 `gpuhdi.on`。本工具按 mkcpio X-3c 的 4096B 条目垫料解析，并只经 `mkcpio.build_archive`
单编码器重发射，不做第二套 newc 编码逻辑。重复 remove marker 全移除；
追加名如已存在则 loud 拒绝，防静默改写。
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from mkcpio import CPIO_PAD, build_archive


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
        header_end = name_end - pos
        data_start = pos + (header_end + CPIO_PAD - 1) // CPIO_PAD * CPIO_PAD
        data_end = data_start + size
        if data_end > len(blob):
            raise FilterError("truncated cpio data")
        if name == TRAILER:
            if size != 0:
                raise FilterError("nonempty cpio trailer")
            trailer_seen = True
            break
        items.append((name, bytes(blob[data_start:data_end])))
        pos = (data_end + CPIO_PAD - 1) // CPIO_PAD * CPIO_PAD
    if not trailer_seen:
        raise FilterError("missing cpio trailer")
    return items


def filter_archive(blob: bytes, remove: bytes) -> tuple[bytes, int]:
    """移除全部 `remove` 条目；重编码保持其余条目顺序与 payload 不变。"""
    items = parse_entries(blob)
    kept = [(name, data) for name, data in items if name != remove]
    return build_archive(kept), len(items) - len(kept)


def add_entries(
    blob: bytes, additions: list[tuple[bytes, Path]]
) -> tuple[bytes, int]:
    """按声明顺序追加普通文件条目；同名已存在 / 文件缺失 loud 拒绝。"""
    items = parse_entries(blob)
    names = {name for name, _ in items}
    appended: list[tuple[bytes, bytes]] = []
    for name, path in additions:
        if not name or b"/" in name or b"\0" in name:
            raise FilterError(f"bad add name: {name!r}")
        if name in names:
            raise FilterError(f"duplicate add name: {name!r}")
        data = path.read_bytes()
        appended.append((name, data))
        names.add(name)
    return build_archive(items + appended), len(appended)


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
    # selftest 内建临时文件，避免依赖仓外路径；CLI 真实文件面由 GPU2 镜像门覆盖。
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        marker = Path(tmp) / "gpuhdi.on"
        marker.write_bytes(b"gpu2\n")
        added, n = add_entries(filtered, [(b"gpuhdi.on", marker)])
        if n != 1 or dict(parse_entries(added)).get(b"gpuhdi.on") != b"gpu2\n":
            raise FilterError("selftest add entry")
        try:
            add_entries(added, [(b"gpuhdi.on", marker)])
        except FilterError:
            pass
        else:
            raise FilterError("selftest duplicate add must fail")
    print("cpio-filter: selftest ok")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("input", nargs="?", type=Path)
    ap.add_argument("output", nargs="?", type=Path)
    ap.add_argument("--remove")
    ap.add_argument("--add", action="append", default=[], metavar="NAME=FILE")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if args.input is None or args.output is None:
        ap.error("input and output are required without --selftest")
    if args.remove is None and not args.add:
        ap.error("at least one --remove or --add is required")
    blob = args.input.read_bytes()
    removed = 0
    if args.remove is not None:
        blob, removed = filter_archive(blob, args.remove.encode())
    additions: list[tuple[bytes, Path]] = []
    for spec in args.add:
        try:
            name, path = spec.split("=", 1)
        except ValueError as error:
            raise FilterError(f"bad --add spec: {spec!r}") from error
        additions.append((name.encode(), Path(path)))
    added = 0
    if additions:
        blob, added = add_entries(blob, additions)
    args.output.write_bytes(blob)
    print(
        f"cpio-filter: {args.input} -> {args.output} "
        f"removed={removed} added={added} bytes={len(blob)}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except FilterError as error:
        print(f"cpio-filter: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
