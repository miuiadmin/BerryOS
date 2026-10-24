#!/usr/bin/env python3
"""ELF64 → flat binary（G-eco 编译侧，docs/G-eco编译侧设计草案.md D2）。

解析 PT_LOAD 段，从约定基址起铺平镜像：段内取 p_filesz 字节、间隙补零，
尾部补零到 max(p_vaddr+p_memsz) 向上取整页——.bss 覆盖由文件侧兑现
（VMO 页零初始化语义不依赖，UserProcess::build 零特判）。

用法：elf2flat.py <in.elf> <out.bin> [--page 4096] [--max N] [--base 0x1000]

失败一律非零退出（坏魔数/非 AArch64/ET_DYN/基址漂移/越 --max）——
缺产物或坏产物 = 下游 include_bytes/断言炸响，不静默。
"""
import argparse
import struct
import sys

EM_AARCH64 = 183
PT_LOAD = 1


def main() -> None:
    ap = argparse.ArgumentParser(description="ELF64 AArch64 → 页对齐 flat binary")
    ap.add_argument("elf")
    ap.add_argument("out")
    ap.add_argument("--page", type=lambda x: int(x, 0), default=4096)
    ap.add_argument("--max", type=lambda x: int(x, 0), default=0, help="镜像长度硬帽（单页档执法点）")
    ap.add_argument("--base", type=lambda x: int(x, 0), default=0x1000, help="约定链接基址")
    a = ap.parse_args()

    with open(a.elf, "rb") as f:
        data = f.read()

    if data[:4] != b"\x7fELF":
        sys.exit("elf2flat: 坏魔数")
    if data[4] != 2 or data[5] != 1:
        sys.exit("elf2flat: 须 ELF64 小端")
    e_type, e_machine = struct.unpack_from("<HH", data, 16)
    if e_machine != EM_AARCH64:
        sys.exit(f"elf2flat: 须 AArch64 (e_machine=183)，得 {e_machine}")
    if e_type != 2:
        sys.exit(f"elf2flat: 须 ET_EXEC(2)，得 {e_type}（v1 固定基址无 PIE，见草案 D1）")
    e_phoff = struct.unpack_from("<Q", data, 32)[0]
    e_phentsize = struct.unpack_from("<H", data, 54)[0]
    e_phnum = struct.unpack_from("<H", data, 56)[0]

    segs = []  # (p_vaddr, p_filesz, p_memsz, p_offset)
    for i in range(e_phnum):
        off = e_phoff + i * e_phentsize
        p_type = struct.unpack_from("<I", data, off)[0]
        p_offset, p_vaddr, _p_paddr, p_filesz, p_memsz = struct.unpack_from("<QQQQQ", data, off + 8)
        if p_type == PT_LOAD and p_memsz > 0:
            segs.append((p_vaddr, p_filesz, p_memsz, p_offset))
    if not segs:
        sys.exit("elf2flat: 无 PT_LOAD")

    segs.sort()
    for (v1, _f1, m1, _o1), (v2, _f2, _m2, _o2) in zip(segs, segs[1:]):
        if v1 + m1 > v2:
            sys.exit(f"elf2flat: PT_LOAD 重叠 @{v1:#x}..{v2:#x}（链接器异常）")

    lo = segs[0][0]
    if lo != a.base:
        sys.exit(f"elf2flat: 链接基址 {lo:#x} ≠ 约定 {a.base:#x}（link-user.ld 漂移？）")
    hi_file = max(v + f for v, f, _, _ in segs)
    hi_mem = max(v + m for v, _, m, _ in segs)
    # 按相对长度取整（base 页对齐时与绝对取整同值，但语义以镜像长度为准）。
    total = (hi_mem - lo + a.page - 1) // a.page * a.page
    if a.max and total > a.max:
        sys.exit(f"elf2flat: 镜像 {total}B 超 --max {a.max}（单页墙——多页放宽随 G-eco-b/D3）")

    img = bytearray(total)
    for v, f, _m, o in segs:
        img[v - lo : v - lo + f] = data[o : o + f]

    with open(a.out, "wb") as f:
        f.write(img)
    print(f"elf2flat: {a.out} base={lo:#x} file={hi_file - lo:#x} mem={hi_mem - lo:#x} total={total:#x}")


if __name__ == "__main__":
    main()
