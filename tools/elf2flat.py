#!/usr/bin/env python3
"""ELF64 → flat binary（G-eco 编译侧，docs/G-eco编译侧设计草案.md D2）。

解析 PT_LOAD 段，从约定基址起铺平镜像：段内取 p_filesz 字节、间隙补零，
尾部补零到 max(p_vaddr+p_memsz) 向上取整页——.bss 覆盖由文件侧兑现
（VMO 页零初始化语义不依赖，UserProcess::build 零特判）。

用法：elf2flat.py <in.elf> <out.bin> [--page 4096] [--max N] [--base 0x1000]

失败一律非零退出（坏魔数/非 AArch64/ET_DYN/基址漂移/越 --max）——
缺产物或坏产物 = 下游 include_bytes/断言炸响，不静默。

自检：elf2flat.py --selftest——内存合成最小 ELF 双段正样本（gap 零填 +
bss 尾零 + 页取整 + 确定性逐字节钉板）+ 八拒面（坏魔数/ELF32/非
AArch64/ET_DYN/基址漂移/段重叠/超 --max/无 PT_LOAD）。挂 just test-host
前置（elf2pe --selftest 同位纪律：本件是 build-user 产链 6 处消费的
门禁，门禁自身坏 = 平像静默劣化，a-3 U4）。
"""
import argparse
import struct
import sys

EM_AARCH64 = 183
PT_LOAD = 1


def build_flat(data: bytes, base: int, page: int, max_: int):
    """纯转换核：ELF 字节 → (平像, base, file_len, mem_len, total)。

    拒面一律 SystemExit（与 CLI 语义同源）；main 与 --selftest 共用本函数。
    """
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
    if lo != base:
        sys.exit(f"elf2flat: 链接基址 {lo:#x} ≠ 约定 {base:#x}（link-user.ld 漂移？）")
    hi_file = max(v + f for v, f, _, _ in segs)
    hi_mem = max(v + m for v, _, m, _ in segs)
    # 按相对长度取整（base 页对齐时与绝对取整同值，但语义以镜像长度为准）。
    total = (hi_mem - lo + page - 1) // page * page
    if max_ and total > max_:
        sys.exit(f"elf2flat: 镜像 {total}B 超 --max {max_}（单页墙——多页放宽随 G-eco-b/D3）")

    img = bytearray(total)
    for v, f, _m, o in segs:
        img[v - lo : v - lo + f] = data[o : o + f]
    return img, lo, hi_file - lo, hi_mem - lo, total


def synth_elf(segs=None, e_type=2, e_machine=EM_AARCH64,
              ident=b"\x7fELF\x02\x01\x01\x00" + b"\x00" * 8) -> bytes:
    """合成最小 ELF64（elf2pe synth_elf 同款纪律）：ehdr + 逐段 PT_LOAD
    phdr + 段文件字节顺次 appended（字节模式 (k*31 + 7 + 段号) % 256——
    自检侧同式重导做逐字节对拍，勿手抄常量）。"""
    if segs is None:
        segs = []
    phoff = 64
    data_off = phoff + 56 * len(segs)
    ehdr = bytearray(64)
    ehdr[0:16] = ident
    struct.pack_into("<HHIQQQIHHHHHH", ehdr, 16,
                     e_type, e_machine, 1, 0, phoff, 0, 0,
                     64, 56, len(segs), 0, 0, 0)
    phdrs = bytearray()
    blob = bytearray()
    for i, (vaddr, filesz, memsz) in enumerate(segs):
        phdrs += struct.pack("<IIQQQQQQ", PT_LOAD, 0x6,
                             data_off + len(blob), vaddr, vaddr, filesz, memsz, 0x1000)
        blob += bytes((k * 31 + 7 + i) % 256 for k in range(filesz))
    return bytes(ehdr) + bytes(phdrs) + bytes(blob)


def selftest() -> int:
    # 双段正样本：段1 [0x1000,0x1020) 恰 file；段2 vaddr=0x2000 跨页 +
    # memsz>filesz（bss 尾零面）——total = ceil(0x1030/0x1000) = 2 页。
    seg1 = bytes((k * 31 + 7) % 256 for k in range(0x20))
    seg2 = bytes((k * 31 + 8) % 256 for k in range(0x10))
    elf = synth_elf([(0x1000, 0x20, 0x20), (0x2000, 0x10, 0x30)])
    img, lo, flen, mlen, total = build_flat(elf, 0x1000, 4096, 0)
    assert (lo, flen, mlen, total) == (0x1000, 0x1010, 0x1030, 0x2000), \
        (hex(lo), hex(flen), hex(mlen), hex(total))
    assert len(img) == 0x2000
    assert img[0x0:0x20] == seg1, "段1 文件字节未落位"
    assert img[0x20:0x1000] == b"\x00" * (0x1000 - 0x20), "段间 gap 未零填"
    assert img[0x1000:0x1010] == seg2, "段2 文件字节未落位"
    assert img[0x1010:] == b"\x00" * (0x2000 - 0x1010), "bss 尾/页取整零面破"
    assert build_flat(elf, 0x1000, 4096, 0)[0] == img, "转换不确定"

    def expect_reject(elf_bytes, base=0x1000, page=4096, max_=0, why=""):
        try:
            build_flat(elf_bytes, base, page, max_)
        except SystemExit:
            return
        print(f"elf2flat: selftest FAIL: {why} 未被拒绝")
        raise SystemExit(1)

    expect_reject(synth_elf(ident=b"\x00ELF\x02\x01\x01\x00" + b"\x00" * 8), why="坏魔数")
    expect_reject(synth_elf(ident=b"\x7fELF\x01\x01\x01\x00" + b"\x00" * 8), why="ELF32")
    expect_reject(synth_elf(e_machine=62), why="非 AArch64")
    expect_reject(synth_elf(e_type=3), why="ET_DYN")
    expect_reject(synth_elf([(0x2000, 0x10, 0x10)]), why="基址漂移")
    expect_reject(synth_elf([(0x1000, 0x10, 0x2000), (0x2000, 0x10, 0x10)]), why="PT_LOAD 重叠")
    expect_reject(elf, max_=4096, why="超 --max")
    expect_reject(synth_elf([]), why="无 PT_LOAD")
    print(f"elf2flat: selftest ok ({total:#x}B 双段钉板 + 8 拒面)")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if args[:1] == ["--selftest"]:
        return selftest()
    ap = argparse.ArgumentParser(description="ELF64 AArch64 → 页对齐 flat binary")
    ap.add_argument("elf")
    ap.add_argument("out")
    ap.add_argument("--page", type=lambda x: int(x, 0), default=4096)
    ap.add_argument("--max", type=lambda x: int(x, 0), default=0, help="镜像长度硬帽（单页档执法点）")
    ap.add_argument("--base", type=lambda x: int(x, 0), default=0x1000, help="约定链接基址")
    a = ap.parse_args(args)

    with open(a.elf, "rb") as f:
        data = f.read()

    img, lo, file_len, mem_len, total = build_flat(data, a.base, a.page, a.max)

    with open(a.out, "wb") as f:
        f.write(img)
    print(f"elf2flat: {a.out} base={lo:#x} file={file_len:#x} mem={mem_len:#x} total={total:#x}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
