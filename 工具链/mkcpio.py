#!/usr/bin/env python3
"""mkcpio——cpio newc 打包器（G-eco-c rootfs 产出端，配对消费端
`用户态/berry-loader` 的 cpio_walk；格式对读 GNU cpio(1) `newc`）。

用法：mkcpio.py <out.cpio> name=path [name=path ...]
  - name 归档内路径（如 bin/hello），path 本地文件（ELF 本体，非平像）；
  - 条目按 name 排序（确定性输出：同输入同字节——AI 测试友好化）；
  - 字段全定值（mtime=0/uid=gid=0/mode 0755/nlink 1/ino 递增）：归档字节
    只随内容变，不随打包时刻变；
  - TRAILER!!! 自终止 + 尾垫 4（消费端 walk 到 trailer 即停）。
"""
import sys


def hex8(v: int) -> bytes:
    return f"{v:08X}".encode()


def entry(name: bytes, data: bytes, ino: int) -> bytes:
    namesize = len(name) + 1  # 含 NUL
    # magic + ino mode uid gid nlink mtime filesize devmaj devmin
    #        rdevmaj rdevmin namesize check（13 × hex8）
    out = b"070701" + b"".join(
        hex8(v)
        for v in (ino, 0o100755, 0, 0, 1, 0, len(data), 0, 0, 0, 0, namesize, 0)
    )
    out += name + b"\0"
    out += b"\0" * (-len(out) % 4)  # 头+名垫到 4
    out += data
    out += b"\0" * (-len(data) % 4)  # 数据垫到 4
    return out


def main() -> int:
    if len(sys.argv) < 3:
        sys.stderr.write(__doc__)
        return 2
    out_path, items = sys.argv[1], sys.argv[2:]
    blob = bytearray()
    for i, item in enumerate(sorted(items)):
        name, _, path = item.partition("=")
        if not name or not _ or "/" in name and name.startswith("/"):
            sys.stderr.write(f"mkcpio: 坏条目 {item!r}\n")
            return 2
        with open(path, "rb") as f:
            blob += entry(name.encode(), f.read(), i + 1)
    blob += entry(b"TRAILER!!!", b"", 0)
    blob += b"\0" * (-len(blob) % 4)
    with open(out_path, "wb") as f:
        f.write(blob)
    print(f"mkcpio: {out_path} {len(blob)}B x{len(items)} 条目")
    return 0


if __name__ == "__main__":
    sys.exit(main())
