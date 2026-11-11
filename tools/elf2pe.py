#!/usr/bin/env python3
"""elf2pe——静态 ELF（AArch64）→ PE32+ EFI 应用转换器（V-0a 刀 1，配对
消费端 = QEMU AAVMF 固件 DXE 加载器；elf2flat 同纪律：确定性、无时刻输入）。

为什么存在（路线修正记录，详 史 → docs/阶段沿革.md V-0a 条目）：VM 草案
原定 `staticlib + rust-lld -flavor link /subsystem:efi_application`——
stable 工具链实测死路：lld 的 COFF 驱动只收 COFF 目标，rustc staticlib 产
ELF rcgu.o（"unknown file type"）；bitcode 路同样死（LTO codegen 跟随
bitcode triple aarch64-unknown-none 落 ELF）。真决策 = 免 nightly + builtin
target + 仓库自带工具链——保留；链接形态改为「ELF 全链接 + 本工具转换」。

管线：cargo（staticlib）→ rust-lld -flavor ld 静态全链接（--emit-relocs
保留重定位元数据 + 自带 berryboot.ld 钉单 .img 节 @ vaddr 0x1000、
--image-base 0）→ elf2pe.py 出 PE32+。

布局不变量（转换器的前提，链接脚本执法）：
  - 单一 alloc 节 .img @ 0x1000（RWE 合一，W^X 拆分留 V-1+）；
  - ImageBase=0 ⇒ RVA == ELF vaddr——所有 PC 相对立即数（ADRP/ADD/BL）
    原样有效，无需改写；
  - 基址重定位只收 R_AARCH64_ABS64（257）→ PE DIR64（type 10）；
    PC 相对/页相对族（ADR_PREL*/ADD_ABS_LO12/LDST*_LO12/JUMP26/CALL26/
    PREL*）在均匀基址平移下不变——忽略；白名单外类型一律 loud FAIL
    （对本仓纪律：静默错比崩更糟）。

用法：elf2pe.py <in.elf> <out.efi>
  转换后恒回读自断言（machine=ARM64 / subsystem=EFI app / entry 落
  .img 内 / .reloc 块尺寸闭合），并打一行机器可读摘要。挂 build-boot。
自检：elf2pe.py --selftest——内存合成最小 ELF（含 ABS64/CALL26/未知型
RELA）走全链转换，字节级钉板断言 + 负样本（未知重定位型须拒）。挂
just test-host 前置（与 mkapp --selftest 同位）。

确定性：TimeDateStamp=0、无 debug 目录、无 PDB 名——归档字节只随内容变。
"""
import struct
import sys

# —— PE 常量。
DOS_E_LFANEW_OFF = 0x3C
PE_SIG = b"PE\0\0"
MACHINE_ARM64 = 0xAA64
SUBSYSTEM_EFI_APPLICATION = 10
OPT_MAGIC_PE32PLUS = 0x20B
NUM_DATA_DIRS = 16
SIZEOF_OPT_HEADER = 240  # 112 字段面 + 16×8 数据目录
SECTION_ALIGNMENT = 0x1000
FILE_ALIGNMENT = 0x200
SCN_CODE_EXEC_READ_WRITE = 0xE0000020   # CNT_CODE|EXECUTE|READ|WRITE
SCN_DATA_DISCARDABLE_READ = 0x42000040  # CNT_INITIALIZED_DATA|DISCARDABLE|READ
IMAGE_FILE_EXECUTABLE_IMAGE = 0x0002
RELOC_DIR64 = 10
RELOC_ABSOLUTE = 0  # 官方 no-op 填充型（空 .reloc 兜底块用）
IDX_BASE_RELOC = 5

# —— ELF 常量（本工具只吃 ELF64 / LE / AArch64）。
EM_AARCH64 = 183
SHT_PROGBITS = 1
SHT_SYMTAB = 2
SHT_STRTAB = 3
SHT_RELA = 4
SHF_ALLOC = 0x2
R_AARCH64_ABS64 = 257

# 均匀基址平移下不变的类型（PC 相对/页相对/低 12 位）——可忽略。
R_PCREL_INVARIANT = {
    260,  # PREL64
    261,  # PREL32
    262,  # PREL16
    274,  # ADR_PREL_LO21
    275,  # ADR_PREL_PG_HI21
    276,  # ADR_PREL_PG_HI21_NC
    277,  # ADD_ABS_LO12_NC
    278,  # LDST8_ABS_LO12_NC
    279,  # TSTBR14
    280,  # CONDBR19
    282,  # JUMP26
    283,  # CALL26
    284,  # LDST16_ABS_LO12_NC
    285,  # LDST32_ABS_LO12_NC
    286,  # LDST64_ABS_LO12_NC
    288,  # LDST128_ABS_LO12_NC
}


def fail(msg: str) -> None:
    sys.stderr.write(f"elf2pe: FAIL: {msg}\n")
    sys.exit(1)


# ---------------------------------------------------------------- ELF 解析。

class Elf:
    def __init__(self, data: bytes):
        if len(data) < 64 or data[:4] != b"\x7fELF":
            fail("非 ELF（魔数不匹配）")
        (self.ei_class, self.ei_data) = (data[4], data[5])
        if self.ei_class != 2 or self.ei_data != 1:
            fail("只吃 ELF64 小端")
        (self.e_type, self.e_machine, _, self.e_entry, self.e_phoff,
         self.e_shoff, _, self.e_ehsize, _, _, self.e_shentsize,
         self.e_shnum, self.e_shstrndx) = struct.unpack_from(
            "<HHIQQQIHHHHHH", data, 16)
        if self.e_machine != EM_AARCH64:
            fail(f"machine=0x{self.e_machine:X} 非 AArch64（183）")
        if self.e_shoff == 0:
            fail("无节头表——--emit-relocs 的输入必有节头")
        self.data = data
        self.sections = []  # dict(name/off/addr/size/type/flags/info/align/entsize)
        for i in range(self.e_shnum):
            self.sections.append(self._read_sh(data, self.e_shoff + i * self.e_shentsize))
        self._patch_names()

    def _read_sh(self, data: bytes, off: int) -> dict:
        (sh_name, sh_type, sh_flags, sh_addr, sh_offset, sh_size, sh_link,
         sh_info, _, sh_entsize) = struct.unpack_from("<IIQQQQIIQQ", data, off)
        return {"name_off": sh_name, "name": "", "type": sh_type,
                "flags": sh_flags, "addr": sh_addr, "offset": sh_offset,
                "size": sh_size, "link": sh_link, "info": sh_info,
                "entsize": sh_entsize}

    def _patch_names(self) -> None:
        if self.e_shstrndx == 0 or self.e_shstrndx >= len(self.sections):
            fail("缺 .shstrtab")
        sh = self.sections[self.e_shstrndx]
        strtab = self.data[sh["offset"]:sh["offset"] + sh["size"]]
        for s in self.sections:
            end = strtab.find(b"\0", s["name_off"])
            s["name"] = strtab[s["name_off"]:end].decode("ascii", "replace")

    def section_by_name(self, name: str):
        for s in self.sections:
            if s["name"] == name:
                return s
        return None

    def contents(self, s: dict) -> bytes:
        return self.data[s["offset"]:s["offset"] + s["size"]]


def collect_relocs(elf: Elf) -> list:
    """ABS64 槽位列表（已排序）。基址重定位只关心「8 字节槽在哪」=
    r_offset——lld --emit-relocs 产**最终镜像绝对 VMA**（非节相对）；条目
    的 sym/addend 描述槽**内容**（指向谁），链接期已解析，与本转换无关。
    V-0b 刀 2 实踩：旧版误用 sym 值当槽位（lld 对静态 reloc 惯用节符号
    val=节基址 0x1000）⇒ 五条 DIR64 全打 RVA 0x1000——固件加载时
    +delta×5 砸穿首函数头两条指令（undefined instruction）。"""
    out = []
    for s in elf.sections:
        if s["type"] != SHT_RELA:
            continue
        target = elf.sections[s["info"]]
        if not (target["flags"] & SHF_ALLOC):
            continue  # （.eh_frame 等 DISCARD 目标的 RELA——target 非_alloc）
        blob = elf.contents(s)
        for off in range(0, len(blob) - 23, 24):
            (r_offset, r_info, _) = struct.unpack_from("<QQq", blob, off)
            rtype = r_info & 0xFFFFFFFF
            if rtype == 0:
                continue
            if rtype == R_AARCH64_ABS64:
                out.append(r_offset)
            elif rtype in R_PCREL_INVARIANT:
                continue
            else:
                fail(f"未归类重定位型 {rtype}@0x{r_offset:X}（节 {target['name']}）——"
                     "白名单外（本仓纪律：静默错比崩更糟，先扩白名单论证再放行）")
    return sorted(out)


# ---------------------------------------------------------------- PE 生成。

def align_up(v: int, a: int) -> int:
    return (v + a - 1) // a * a


def build_reloc_blocks(vaddrs: list) -> bytes:
    """DIR64 重定位块（按 4KiB 页分组，块尺寸含头恒 4B 对齐）。空表产
    一个合法空块（仅块头 8B）——.reloc 目录恒有可走查的落点。"""
    pages = {}
    for v in vaddrs:
        pages.setdefault(v >> 12, []).append(v)
    if not pages:
        pages[0] = []
    out = bytearray()
    for page in sorted(pages):
        ents = [RELOC_DIR64 << 12 | (v & 0xFFF) for v in sorted(pages[page])]
        while (8 + 2 * len(ents)) % 4 != 0:
            ents.append(RELOC_ABSOLUTE << 12)  # 块内 4B 对齐填充（no-op 型）
        out += struct.pack("<II", page << 12, 8 + 2 * len(ents))
        out += b"".join(struct.pack("<H", e) for e in ents)
    return bytes(out)


def elf_section_payload(elf: Elf):
    """单 alloc 节形状执法：恰一个 SHF_ALLOC 的 PROGBITS/NOBITS 同体节
    .img（berryboot.ld 钉的形状）——多 alloc 节 = 链接脚本漂移，loud FAIL。"""
    img = elf.section_by_name(".img")
    if img is None:
        fail("缺 .img 节（链接脚本必须把全部内容并进单一 .img @0x1000）")
    if not (img["flags"] & SHF_ALLOC):
        fail(".img 节非 SHF_ALLOC")
    for s in elf.sections:
        if s is img or s["type"] == 0 or s["type"] == SHT_RELA:
            continue
        if s["flags"] & SHF_ALLOC:
            fail(f"多 alloc 节 {s['name']}——单 .img 不变量破坏")
    payload = elf.contents(img)
    return img, payload


def convert(data: bytes) -> bytes:
    elf = Elf(data)
    img, payload = elf_section_payload(elf)
    if img["addr"] != SECTION_ALIGNMENT:
        fail(f".img vaddr=0x{img['addr']:X} ≠ 0x1000（RVA==vaddr 前提破坏）")
    if not (img["addr"] <= elf.e_entry < img["addr"] + img["size"]):
        fail(f"entry=0x{elf.e_entry:X} 落 .img 外")
    relocs = collect_relocs(elf)
    reloc_data = build_reloc_blocks(relocs)

    img_raw_size = align_up(len(payload), FILE_ALIGNMENT)
    img_va = img["addr"]
    reloc_va = align_up(img_va + max(img["size"], img_raw_size), SECTION_ALIGNMENT)
    reloc_raw = align_up(len(reloc_data), FILE_ALIGNMENT)
    # 布局不变量：.img/.reloc 的文件偏移 == 各自 RVA（文件 = 头页 + 内存
    # 镜像）——verify 的 RVA 走查即文件走查，无 RVA↔offset 换算面。
    assert SECTION_ALIGNMENT == img_va
    reloc_ptr = reloc_va
    size_of_image = align_up(reloc_va + len(reloc_data), SECTION_ALIGNMENT)

    # —— 头部页（0x0..0x1000）：DOS + PE + COFF + Optional + 节表，余零。
    hdr = bytearray(SECTION_ALIGNMENT)
    struct.pack_into("<H", hdr, 0, 0x5A4D)                    # e_magic 'MZ'
    struct.pack_into("<I", hdr, DOS_E_LFANEW_OFF, 0x80)      # e_lfanew
    pe_off = 0x80
    hdr[pe_off:pe_off + 4] = PE_SIG
    coff_off = pe_off + 4
    struct.pack_into("<HHIIIHH", hdr, coff_off,
                     MACHINE_ARM64,            # Machine
                     2,                        # NumberOfSections（.img/.reloc）
                     0,                        # TimeDateStamp（确定性：恒 0）
                     0, 0,                     # 符号表（无）
                     SIZEOF_OPT_HEADER,
                     IMAGE_FILE_EXECUTABLE_IMAGE)
    opt_off = coff_off + 20
    o = bytearray(SIZEOF_OPT_HEADER)
    struct.pack_into("<HBBIIIIIQIIHHHHHHIIIIHHQQQQII", o, 0,
                     OPT_MAGIC_PE32PLUS, 0, 0,           # Magic / LinkerVersion
                     img_raw_size, reloc_raw, 0,          # Code/InitData/Uninit
                     elf.e_entry,                          # AddressOfEntryPoint
                     img_va,                               # BaseOfCode
                     0,                                    # ImageBase=0 ⇒ RVA==vaddr
                     SECTION_ALIGNMENT, FILE_ALIGNMENT,
                     0, 0,                                 # OS 版本
                     0, 0,                                 # Image 版本
                     1, 0,                                 # Subsystem 版本（≥1.0）
                     0,                                    # Win32VersionValue
                     size_of_image,
                     SECTION_ALIGNMENT,                    # SizeOfHeaders
                     0,                                    # CheckSum
                     SUBSYSTEM_EFI_APPLICATION,
                     0,                                    # DllCharacteristics
                     0, 0, 0, 0,                           # Stack/Heap（DXE 不用）
                     0, NUM_DATA_DIRS)
    struct.pack_into("<II", o, 112 + 8 * IDX_BASE_RELOC, reloc_va, len(reloc_data))
    hdr[opt_off:opt_off + SIZEOF_OPT_HEADER] = o

    def sect(name: bytes, vsize: int, va: int, rawsz: int, ptr: int, chars: int):
        e = bytearray(40)
        e[0:8] = name
        struct.pack_into("<IIIIIIHHI", e, 8, vsize, va, rawsz, ptr,
                         0, 0, 0, 0, chars)
        return e

    sh_off = opt_off + SIZEOF_OPT_HEADER
    hdr[sh_off:sh_off + 40] = sect(b".img\0\0\0\0", len(payload), img_va,
                                   img_raw_size, SECTION_ALIGNMENT,
                                   SCN_CODE_EXEC_READ_WRITE)
    hdr[sh_off + 40:sh_off + 80] = sect(b".reloc\0\0", len(reloc_data),
                                        reloc_va, reloc_raw, reloc_ptr,
                                        SCN_DATA_DISCARDABLE_READ)

    out = bytearray(hdr)
    out += payload + b"\0" * (img_raw_size - len(payload))
    out += b"\0" * (reloc_ptr - len(out))
    out += reloc_data + b"\0" * (reloc_raw - len(reloc_data))
    assert len(out) == reloc_ptr + reloc_raw, "文件长度不变量破坏"
    return bytes(out)


# ---------------------------------------------------------------- 回读断言。

def verify(pe: bytes, reloc_count_in: int) -> str:
    if pe[:2] != b"MZ":
        fail("回读：无 MZ")
    pe_off = struct.unpack_from("<I", pe, DOS_E_LFANEW_OFF)[0]
    if pe[pe_off:pe_off + 4] != PE_SIG:
        fail("回读：无 PE 签名")
    machine, nsec = struct.unpack_from("<HH", pe, pe_off + 4)
    if machine != MACHINE_ARM64:
        fail(f"回读：machine=0x{machine:X} 非 ARM64")
    opt = pe_off + 24
    magic, = struct.unpack_from("<H", pe, opt)
    if magic != OPT_MAGIC_PE32PLUS:
        fail("回读：非 PE32+")
    entry, = struct.unpack_from("<I", pe, opt + 16)
    subsys, = struct.unpack_from("<H", pe, opt + 68)
    if subsys != SUBSYSTEM_EFI_APPLICATION:
        fail(f"回读：subsystem={subsys} 非 EFI application(10)")
    soh, = struct.unpack_from("<I", pe, opt + 60)
    if soh != SECTION_ALIGNMENT:
        fail(f"回读：SizeOfHeaders={soh}")
    rva, rsz = struct.unpack_from("<II", pe, opt + 112 + 8 * IDX_BASE_RELOC)
    sh = opt + struct.unpack_from("<H", pe, pe_off + 20)[0]
    names = [pe[sh + 40 * i:sh + 40 * i + 8].rstrip(b"\0") for i in range(nsec)]
    if names[0] != b".img" or b".reloc" not in names:
        fail(f"回读：节名 {names}")
    img_vs, img_va = struct.unpack_from("<II", pe, sh + 8)
    if not (img_va <= entry < img_va + img_vs):
        fail(f"回读：entry=0x{entry:X} 不在 .img [0x{img_va:X},+0x{img_vs:X})")
    if rsz == 0:
        fail("回读：.reloc 目录空")
    n_blocks = 0
    p = rva
    while p < rva + rsz:
        _, bsz = struct.unpack_from("<II", pe, p)
        n_blocks += 1
        p += bsz
    if p != rva + rsz:
        fail("回读：重定位块尺寸不闭合")
    return (f"elf2pe: ok entry=0x{entry:X} img=0x{img_vs:X}B "
            f"relocs={reloc_count_in} blocks={n_blocks}")


# ---------------------------------------------------------------- 自检。

def synth_elf() -> bytes:
    """内存合成最小 ELF64/LE/AArch64：单 .img @0x1000（4 指令 + 8B 数据）
    + .rela.img（ABS64 + CALL26）+ .symtab/.strtab/.shstrtab——转换器全链
    的字节级基准（真实产物另有 berryboot 集成面，两层互补）。"""
    code = bytes.fromhex(
        "90000010"  # 0x1000: adr x16, +4        （占位 PC 相对）
        "91000230"  # 0x1004: add x16, x16, #0
        "58000141"  # 0x1008: ldr x1, [x10]      （占位）
        "d65f03c0"  # 0x100C: ret
    ) + struct.pack("<Q", 0x1008)  # 0x1010: 8B 数据槽（ABS64 目标 A）
    code += struct.pack("<Q", 0x1000)  # 0x1018: 8B 数据槽（ABS64 目标 B）
    shstrtab = b"\0.img\0.rela.img\0.symtab\0.strtab\0.shstrtab\0"
    strtab = b"\0"
    symtab = struct.pack("<IBBHQQ", 0, 0, 0, 0, 0, 0)  # sym[0] 空
    symtab += struct.pack("<IBBHQQ", 0, (3 << 4), 0, 1, 0x1000, 0)
    # sym[1] = STT_SECTION（.img，val=节基址）——lld --emit-relocs 静态
    # reloc 惯用形态：r_off=槽位绝对 VMA + rsym=节符号 + addend=符号内偏移
    rela = struct.pack("<QQq", 0x1010, R_AARCH64_ABS64, 0x1008)   # 槽 A，rsym=0
    rela += struct.pack("<QQq", 0x0, 283, 0)                      # CALL26（忽略型）
    rela += struct.pack("<QQq", 0x1018, (1 << 32) | R_AARCH64_ABS64, 0x1000)
    # 槽 B，rsym=1（节符号）——旧 bug 形态：sym 值 0x1000 被当槽位错打 0x1000

    ehsize, phentsize, shentsize = 64, 56, 64
    blobs = [code, rela, symtab, strtab, shstrtab]
    sec_offs = {}
    off = ehsize
    for b in blobs:
        sec_offs[len(sec_offs)] = off
        off += len(b)
    out = bytearray(ehsize)
    for b in blobs:
        out += b
    shoff = len(out)
    out += b"\0" * (6 * shentsize)
    struct.pack_into("<4sBBBBB7xHHIQQQIHHHHHH", out, 0,
                     b"\x7fELF", 2, 1, 1, 0, 0,
                     2, EM_AARCH64, 1,       # ET_EXEC / EV_CURRENT
                     0x1000,                 # e_entry = .img 头 = 0x1000
                     0, shoff, 0,            # phoff=0 / shoff / flags
                     ehsize, phentsize, 0, shentsize, 6, 5)
    sections = [
        # (name_off, type, flags, addr, data_off_idx, size, link, info, entsize)
        (0, 0, 0, 0, None, 0, 0, 0, 0),                                # [0] NULL
        (shstrtab.index(b".img"), SHT_PROGBITS, SHF_ALLOC | 0x4 | 0x1,
         0x1000, 0, len(code), 0, 0, 0),                               # [1] .img
        (shstrtab.index(b".rela.img"), SHT_RELA, 0, 0, 1, len(rela),
         3, 1, 24),                                                    # [2] .rela.img
        (shstrtab.index(b".symtab"), SHT_SYMTAB, 0, 0, 2, len(symtab),
         4, 1, 24),                                                    # [3] .symtab
        (shstrtab.index(b".strtab"), SHT_STRTAB, 0, 0, 3, len(strtab),
         0, 0, 0),                                                     # [4] .strtab
        (shstrtab.index(b".shstrtab"), SHT_STRTAB, 0, 0, 4, len(shstrtab),
         0, 0, 0),                                                     # [5] .shstrtab
    ]
    for i, (no, ty, fl, addr, doi, size, link, info, ent) in enumerate(sections):
        doff = sec_offs[doi] if doi is not None else 0
        struct.pack_into("<IIQQQQIIQQ", out, shoff + i * shentsize,
                         no, ty, fl, addr, doff, size, link, info, 0, ent)
    return bytes(out)


def selftest() -> int:
    elf = synth_elf()
    # 合成源内 3 条 RELA：2 条 ABS64（槽 A rsym=0 / 槽 B rsym=节符号——
    # 双路径都进 .reloc 且槽位恒取 r_offset）+ 1 条 CALL26（忽略）。
    pe = convert(elf)
    line = verify(pe, 2)
    assert struct.unpack_from("<H", pe, 0) == (0x5A4D,)
    pe_off = 0x80
    machine, nsec = struct.unpack_from("<HH", pe, pe_off + 4)
    assert machine == MACHINE_ARM64 and nsec == 2
    opt = pe_off + 24
    assert struct.unpack_from("<H", pe, opt)[0] == OPT_MAGIC_PE32PLUS
    assert struct.unpack_from("<I", pe, opt + 16)[0] == 0x1000  # entry
    assert struct.unpack_from("<H", pe, opt + 68)[0] == SUBSYSTEM_EFI_APPLICATION
    assert struct.unpack_from("<Q", pe, opt + 24)[0] == 0        # ImageBase=0
    # .img 数据字节逐位进 PE（RVA==vaddr ⇒ file@0x1000 即镜像内容）。
    img_off = SECTION_ALIGNMENT
    assert pe[img_off:img_off + 20] == Elf(elf).contents(
        Elf(elf).section_by_name(".img"))[:20]
    # .reloc 块钉板：页 0x1000、块 12B（2 DIR64 条目）——槽 A 0x1010（rsym=0）
    # + 槽 B 0x1018（rsym=节符号——旧 bug 曾把这条错打到 0x1000，本钉板回归执法）。
    rva, rsz = struct.unpack_from("<II", pe, opt + 112 + 8 * IDX_BASE_RELOC)
    assert (rva, rsz) == (0x2000, 12), (hex(rva), rsz)
    assert struct.unpack_from("<IIHH", pe, rva) == (
        0x1000, 12, RELOC_DIR64 << 12 | 0x010, RELOC_DIR64 << 12 | 0x018)
    # 确定性：两次转换逐字节同。
    assert convert(elf) == pe
    # 负样本：白名单外重定位型必须拒（SystemExit 非 0 退出）。
    bad = bytearray(elf)
    # 把 .rela.img 第二条（CALL26 @ rela+24）改成未知型 0xDEAD。
    rela_off = sec_offs_of(Elf(elf), ".rela.img")
    struct.pack_into("<Q", bad, rela_off + 24 + 8, 0xDEAD)
    try:
        convert(bytes(bad))
    except SystemExit:
        pass
    else:
        print("elf2pe: selftest FAIL: 未知重定位型未被拒绝")
        return 1
    print(f"elf2pe: selftest ok ({line})")
    return 0


def sec_offs_of(elf: Elf, name: str) -> int:
    return elf.section_by_name(name)["offset"]


def main() -> int:
    args = sys.argv[1:]
    if args[:1] == ["--selftest"]:
        return selftest()
    if len(args) < 2 or args[0].startswith("-"):
        sys.stderr.write(__doc__)
        return 2
    with open(args[0], "rb") as f:
        data = f.read()
    relocs = collect_relocs(Elf(data))
    pe = convert(data)
    line = verify(pe, len(relocs))
    with open(args[1], "wb") as f:
        f.write(pe)
    print(f"{line} -> {args[1]} {len(pe)}B")
    return 0


if __name__ == "__main__":
    sys.exit(main())
