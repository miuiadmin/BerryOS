#!/usr/bin/env python3
"""mkcpio——cpio newc 打包器（G-eco-c rootfs 产出端，配对消费端
`user/berry-loader` 的 cpio_walk；格式对读 GNU cpio(1) `newc`）。

用法：mkcpio.py <out.cpio> name=path [name=path ...]
  - name 归档内路径（如 bin/hello），path 本地文件（ELF 本体，非平像）；
  - 条目按整条 `name=path` 字符串排序（确定性输出：同输入同字节——AI
    测试友好化。app-0 §4 钉板：这是 CLI main() 的行为；`build_archive`
    纯编码器**不排序**——排序归调用方，勿混）；
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


def build_archive(items: list[tuple[bytes, bytes]]) -> bytes:
    """纯编码器（app-0 §4 单编码器纪律）：不排序、按传入序编码、ino =
    传入序号 + 1——**排序归调用方**。TRAILER + 尾垫 4 在内。

    mkapp（.bap 打包）`from mkcpio import build_archive` 复用——禁止任何
    第二份 newc 编码/trailer 逻辑（双编码器 = 漂移面）。CLI main() 的
    排序语义（整条 `name=path` 字符串）与其调用本函数的序保持不变。
    """
    blob = bytearray()
    for i, (name, data) in enumerate(items):
        blob += entry(name, data, i + 1)
    blob += entry(b"TRAILER!!!", b"", 0)
    blob += b"\0" * (-len(blob) % 4)
    return bytes(blob)


def main() -> int:
    if len(sys.argv) < 3:
        sys.stderr.write(__doc__)
        return 2
    out_path, items = sys.argv[1], sys.argv[2:]
    parsed = []
    for item in sorted(items):
        name, _, path = item.partition("=")
        if not name or not _ or "/" in name and name.startswith("/"):
            sys.stderr.write(f"mkcpio: 坏条目 {item!r}\n")
            return 2
        with open(path, "rb") as f:
            parsed.append((name.encode(), f.read()))
    blob = build_archive(parsed)
    with open(out_path, "wb") as f:
        f.write(blob)
    print(f"mkcpio: {out_path} {len(blob)}B x{len(items)} 条目")
    return 0


if __name__ == "__main__":
    sys.exit(main())
