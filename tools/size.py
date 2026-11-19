#!/usr/bin/env python3
"""BerryOS 内核体积基线报告 —— `just size` / CI 同款入口。

12 号报告 §10.1：BerryOS 自身 ELF 体积至今未测（红队缺口 1），
一切体积讨论的基线从 day-one 建起。零依赖解析 ELF64 头：

  - 文件总量（ROM/镜像体积）
  - PT_LOAD 汇总：filesz = 镜像内实占，ram = memsz 静态内存**小计**
    （memsz - filesz ≈ .bss，即不占镜像但占 RAM 的部分；
    不含链接脚本裸 `.` 递增保留的 boot/主栈——它们不进任何段）
  - ram_total = __kernel_end - RAM_BASE：**真实静态内存占用** = memsz 小计
    + 保留栈（boot 32KiB + 主栈 256KiB），体积基线口径以此为准；
    symtab 缺失/找不到符号时优雅降级为提示行（strip 产物），不阻断
  - alloc 段明细（.text / .rodata / .data / .bss / …）

只报数、不设阈值——基线先有数据，回归门禁等基线稳定后再定
（Tock 192KB 教训：Rust 体积劣化是无声发生的，得先能看见）。

`--assert-flip <elf>`：2b-1 高半翻转程序头断言（smoke 固化，内存管理设计 §5.1
「VMA≠LMA 调试地狱」的防漂移哨兵）：
  - e_entry 在低半（boot 段内）；
  - 每个 PT_LOAD 满足 vaddr==paddr（低半 boot：.text.boot/.bss.boot/.boot_stack）
    或 vaddr-paddr==0xFFFFFF8000000000（高半主体 = KERNEL_VA_BASE 镜像）；
  - 全部 paddr 落在 QEMU virt RAM [0x40000000, 0x48000000)（-m 128M）；
  - K-3：.text.boot 全部 ADRP 目标必低半（bit63==0）——K-2 同族防复发哨兵。
    K-2 断链形态（低半 .text.boot 引高半 .rodata.cst16，~512GiB 外）在链接期
    就死了（build-release 守卫捕获）；本断言守的是**链得起来**、目标却落高半的
    形态（如负向偏移 mod 2^64 回卷到 bit63=1 区——指令完全合法、链接器照收，
    唯 bit63 断言能拦）。debug 面经 smoke.sh 既有 --assert-flip 调用零改动生效，
    release 面在 just build-release recipe 尾同款执法。非 AArch64 ELF 直接 FAIL
    （K-3 处置收紧：两条管线皆 AArch64 专用，混入 = 调用方传错文件——
    fail-closed，跳过会让合成混合体打出整体 OK）。

`--selftest`：ADRP 解码器向量自检（挂 just test-host，与 mkapp/mkfat/mkimage
自检同位——脚本自检不进 cargo 计数）：真机编码向量（两面 .text.boot 实取、
与 llvm-objdump 对拍 1:1）+ 高半失败样本（必须红）+ 非 ADRP 对照 + encode
负样本（K-2 断链形态）+ 红/绿分支 + 节空/NOBITS、节数据截断、size%4!=0、
零 ADRP 四拒面。
"""
import os
import struct
import sys

EI_NIDENT = 16

KERNEL_VA_BASE = 0xFFFFFF8000000000
# QEMU virt RAM 顶：基址 0x40000000，实跑配置 -m 128M（justfile 的 MACH/run-el2
# 与 tools/smoke.sh 一致）→ 顶 = 0x40000000 + 128MiB = 0x48000000。
RAM_BASE, RAM_TOP = 0x40000000, 0x48000000

EM_AARCH64 = 0xB7  # e_machine@18：AArch64——ADRP 断言的解码前置（防 x86 ELF 混入）
MASK64 = 0xFFFFFFFFFFFFFFFF


def u(buf, off, size):
    return int.from_bytes(buf[off:off + size], "little")


def require_elf64_le(elf, path):
    """入口共用校验：魔数 + class（64 位）+ 小端——防 32 位/大端伪造产物
    混过任一入口（main 与 assert_flip 都要先过这一关）。"""
    if elf[:4] != b"\x7fELF":
        sys.exit(f"size.py: 不是 ELF 文件：{path}")
    if elf[4] != 2 or elf[5] != 1:
        sys.exit(f"size.py: 只支持 ELF64 小端：{path}")


def find_symbol(elf, e_shoff, e_shentsize, e_shnum, name):
    """在第一个 SHT_SYMTAB 里按名找符号，返回 st_value；无 symtab 或
    找不到返回 None（调用方负责降级，别崩）。"""
    sh = lambda i: elf[e_shoff + i * e_shentsize: e_shoff + (i + 1) * e_shentsize]
    for i in range(e_shnum):
        s = sh(i)
        if u(s, 4, 4) != 2:  # SHT_SYMTAB
            continue
        sym_off, sym_size = u(s, 24, 8), u(s, 32, 8)
        entsize = u(s, 56, 8) or 24  # Elf64_Sym
        strtab_off = u(sh(u(s, 40, 4)), 24, 8)  # sh_link → 关联字符串表
        for j in range(sym_size // entsize):
            sym = elf[sym_off + j * entsize: sym_off + (j + 1) * entsize]
            st_name = u(sym, 0, 4)
            end = elf.index(b"\0", strtab_off + st_name)
            if elf[strtab_off + st_name:end].decode() == name:
                return u(sym, 8, 8)  # st_value
    return None


def decode_adrp_target(word, pc):
    """单条 4B 指令字解码 ADRP：是 ADRP 返回 64 位目标页地址，否则 None。

    A64 编码（ARMv8 C4.5.2）：bit31=1、bits[28:24]=10000（判别
    (word & 0x9F000000) == 0x90000000），immlo=bits[30:29]（2b）、
    immhi=bits[23:5]（19b）拼 21 位**有符号**立即数，
    target = (pc & ~0xFFF) + (imm21 << 12)（mod 2^64——负向偏移回卷出 0
    即落 bit63=1 高位区，这正是断言要抓的形态）。注意编码只依赖 pc 的
    **页基**（低 12 位不进 word）——同页内指令挪位不改 word。"""
    if (word & 0x9F000000) != 0x90000000:
        return None
    imm21 = (((word >> 5) & 0x7FFFF) << 2) | ((word >> 29) & 0x3)
    if imm21 & (1 << 20):  # 21 位符号扩展
        imm21 -= 1 << 21
    return ((pc & ~0xFFF) + (imm21 << 12)) & MASK64


def encode_adrp(pc, target, rd=0):
    """decode 的对偶（selftest 造红样本用，兼作编码算式的可执行文档）。

    目标页差超出 ±4GB（imm21 有符号 21 位窗 = ±2^20 页）抛 ValueError——
    K-2 断链正是这个形态：低半 .text.boot 引高半 .rodata.cst16
    （0xffffff80_40040000，~512GiB 外），链接器无从重定位、断在链接期。"""
    delta = (target - (pc & ~0xFFF)) & MASK64
    if delta >= 1 << 63:
        delta -= 1 << 64  # mod 2^64 回卷差先折带符号（负向偏移在此显形）
    page = delta >> 12
    if not -(1 << 20) <= page < (1 << 20):
        raise ValueError(f"目标超出 ADRP ±4GB 可编码窗：pc=0x{pc:x} "
                         f"target=0x{target:x}（页差 {page}——K-2 断链形态）")
    imm21 = page & 0x1FFFFF
    return (0x90000000 | ((imm21 & 0x3) << 29) | ((imm21 >> 2) << 5)
            | (rd & 0x1F))


def find_section(elf, name):
    """按名取节 → (sh_addr, sh_type, sh_size, data)；找不到返回 None。

    比 main() 内联走查多读 sh_addr@16——ADRP 断言要算节内绝对 VA。data 对
    SHT_NOBITS（.bss 类）给 b""（.text.boot 是 PROGBITS，NOBITS 即问题）。"""
    e_shoff = u(elf, 40, 8)
    e_shentsize, e_shnum, e_shstrndx = u(elf, 58, 2), u(elf, 60, 2), u(elf, 62, 2)
    sh = lambda i: elf[e_shoff + i * e_shentsize: e_shoff + (i + 1) * e_shentsize]
    strtab_off = u(sh(e_shstrndx), 24, 8)
    for i in range(e_shnum):
        s = sh(i)
        sh_name = u(s, 0, 4)
        end = elf.index(b"\0", strtab_off + sh_name)
        if elf[strtab_off + sh_name:end].decode() != name:
            continue
        sh_type, sh_addr = u(s, 4, 4), u(s, 16, 8)
        sh_size = u(s, 32, 8)
        data = b"" if sh_type == 8 else elf[u(s, 24, 8):u(s, 24, 8) + sh_size]
        return sh_addr, sh_type, sh_size, data
    return None


def scan_boot_section(section, sh_addr, sh_type, sh_size, data):
    """节级四拒面 + ADRP 扫描 → (adrps, problems)。adrps = [(pc, word, target)]。

    四拒面：节空/NOBITS（断言面失守）、节数据截断（PROGBITS len(data)!=sh_size
    ——对抗审查 K-3 处置：find_section 的切片对物理截断静默变短，被截尾部的
    高半 ADRP 逃扫 = fail-open，必须 loud）、size%4!=0（指令流完整性最低门）、
    ADRP 数=0（K-2 后两面各恰 5 条，归零 = 断言面失守）。抽出来让 ELF 级
    断言与 selftest 红/绿分支共用同一条扫描路径（勿长两套）。"""
    problems = []
    if sh_type == 8 or sh_size == 0 or not data:
        problems.append(f"节 {section} 为空/NOBITS（sh_type={sh_type} "
                        f"sh_size={sh_size}）——ADRP 断言面失守")
        return [], problems
    if sh_type != 8 and len(data) != sh_size:
        problems.append(f"节 {section} 数据截断（len(data)={len(data)} != "
                        f"sh_size={sh_size}）——尾部逃扫，ADRP 断言面失守")
        return [], problems
    if len(data) % 4 != 0:
        problems.append(f"节 {section} size={len(data)} 非 4 字节倍数——指令流已烂")
        return [], problems
    adrps = []
    for off in range(0, len(data), 4):
        word = u(data, off, 4)
        pc = sh_addr + off
        target = decode_adrp_target(word, pc)
        if target is None:
            continue
        adrps.append((pc, word, target))
        if target >> 63:
            problems.append(f"ADRP pc=0x{pc:x} word=0x{word:08x} "
                            f"target=0x{target:016x} bit63=1（高半）——"
                            f"低半 boot 代码引高半数据（K-2 同族）")
    if not adrps:
        problems.append(f"节 {section} 内 ADRP 数 = 0——断言面失守（两面基线各 5 条）")
    return adrps, problems


def assert_boot_adrp_low(elf, path):
    """K-3 断言主体：.text.boot 全部 ADRP 目标必低半。返回 problems 列表
    （空 = 过），由 assert_flip 并入统一 FAIL 出口（免费获得非零退出协议）。
    非 AArch64 ELF 直接 FAIL（K-3 处置收紧：两条管线皆 AArch64 专用，混入
    即调用方传错文件——fail-closed，跳过会让合成混合体打出整体 OK）。"""
    if u(elf, 18, 2) != EM_AARCH64:
        return [f"{path}: e_machine={u(elf, 18, 2):#x} != "
                f"EM_AARCH64({EM_AARCH64:#x})——非 AArch64 ELF，ADRP 断言面"
                f"失守（fail-closed 不跳过）"]
    sec = find_section(elf, ".text.boot")
    if sec is None:
        return ["节 .text.boot 不存在——ADRP 断言面失守"]
    adrps, problems = scan_boot_section(".text.boot", *sec)
    if adrps and not problems:
        targets = "/".join(f"0x{t:x}" for t in sorted({t for _, _, t in adrps}))
        print(f"berryos-size: assert-flip: .text.boot ADRP {len(adrps)} 条"
              f"全低半（bit63=0，目标 {targets}）")
    return problems


def main(path):
    with open(path, "rb") as f:
        elf = f.read()

    require_elf64_le(elf, path)

    # ELF64 头字段偏移（e_ident[16] 之后：e_type/e_machine/e_version 各 4B，
    # e_entry/e_phoff/e_shoff 各 8B，再 e_flags 4B + 6 个 u16）
    e_phoff = u(elf, 32, 8)
    e_shoff = u(elf, 40, 8)
    e_phentsize, e_phnum = u(elf, 54, 2), u(elf, 56, 2)
    e_shentsize, e_shnum, e_shstrndx = u(elf, 58, 2), u(elf, 60, 2), u(elf, 62, 2)

    # —— 段表（PT_LOAD）：镜像 vs 运行时内存 ——
    rom = ram = 0
    loads = []
    for i in range(e_phnum):
        ph = elf[e_phoff + i * e_phentsize: e_phoff + (i + 1) * e_phentsize]
        p_type, p_flags = u(ph, 0, 4), u(ph, 4, 4)
        p_offset, p_vaddr = u(ph, 8, 8), u(ph, 16, 8)
        p_filesz, p_memsz = u(ph, 32, 8), u(ph, 40, 8)
        if p_type == 1:  # PT_LOAD
            rom += p_filesz
            ram += p_memsz
            loads.append((p_vaddr, p_filesz, p_memsz, p_flags))

    # —— 节表（alloc 节明细）——
    sh = lambda i: elf[e_shoff + i * e_shentsize: e_shoff + (i + 1) * e_shentsize]
    strtab_off = u(sh(e_shstrndx), 24, 8)
    sections = []
    for i in range(e_shnum):
        s = sh(i)
        sh_name, sh_type, sh_flags = u(s, 0, 4), u(s, 4, 4), u(s, 8, 8)
        sh_offset, sh_size = u(s, 24, 8), u(s, 32, 8)
        if not sh_flags & 0x2:  # SHF_ALLOC
            continue
        end = elf.index(b"\0", strtab_off + sh_name)
        name = elf[strtab_off + sh_name:end].decode()
        file_sz = 0 if sh_type == 8 else sh_size  # SHT_NOBITS = .bss 类，只在内存
        sections.append((name, file_sz, sh_size))

    kib = lambda b: f"{b / 1024:.1f}KiB"
    print(f"berryos-size: {path}")
    print(f"berryos-size: file={len(elf)}B ({kib(len(elf))})")
    print(f"berryos-size: PT_LOAD rom={rom}B ({kib(rom)}) ram={ram}B ({kib(ram)})"
          f"  # ram-rom={ram - rom}B 为 .bss 类零填充；ram 为 memsz 小计"
          f"（不含链接脚本裸 . 保留的 boot/主栈）")
    # —— 真实静态内存：__kernel_end 是链接脚本镜像末端符号（linker-aarch64.ld，
    #    含裸 `.` 保留的 boot 栈 0x8000 + 主栈 0x40000，PT_LOAD 看不见这部分）。
    #    高半符号先减 KERNEL_VA_BASE 折算 PA 再减 RAM_BASE。——
    kend = find_symbol(elf, e_shoff, e_shentsize, e_shnum, "__kernel_end")
    if kend is not None:
        kend_pa = kend - KERNEL_VA_BASE if kend >= KERNEL_VA_BASE else kend
        print(f"berryos-size: ram_total={kend_pa - RAM_BASE}B "
              f"({kib(kend_pa - RAM_BASE)})"
              f"  # __kernel_end-RAM_BASE 真实静态内存"
              f"（memsz 小计 + boot 栈 32KiB + 主栈 256KiB）")
    else:
        print("berryos-size: ram_total 未知：无 SHT_SYMTAB 或找不到 __kernel_end"
              "（strip 过的产物？）——降级只报 memsz 小计")
    for vaddr, filesz, memsz, flags in loads:
        print(f"berryos-size:   LOAD vaddr=0x{vaddr:012x} filesz={filesz}B memsz={memsz}B flags=0x{flags:x}")
    for name, file_sz, mem_sz in sections:
        if file_sz or mem_sz:
            print(f"berryos-size:   {name:12s} file={file_sz:8d}B mem={mem_sz:8d}B")


def assert_flip(path):
    """2b-1 高半翻转程序头断言（见模块文档）。失败非零退出（smoke FAIL）。"""
    with open(path, "rb") as f:
        elf = f.read()
    require_elf64_le(elf, path)
    e_entry = u(elf, 24, 8)
    e_phoff, e_phentsize, e_phnum = u(elf, 32, 8), u(elf, 54, 2), u(elf, 56, 2)

    problems = []
    if e_entry >= 1 << 39:
        problems.append(f"e_entry=0x{e_entry:016x} 不在低半（boot 段丢了？）")

    boot_any = body_any = False
    for i in range(e_phnum):
        ph = elf[e_phoff + i * e_phentsize: e_phoff + (i + 1) * e_phentsize]
        if u(ph, 0, 4) != 1:  # PT_LOAD
            continue
        p_vaddr, p_paddr = u(ph, 16, 8), u(ph, 24, 8)
        diff = (p_vaddr - p_paddr) & 0xFFFFFFFFFFFFFFFF
        if diff == 0:
            boot_any = True
        elif diff == KERNEL_VA_BASE:
            body_any = True
        else:
            problems.append(f"PT_LOAD vaddr=0x{p_vaddr:016x} paddr=0x{p_paddr:016x} "
                            f"v-p=0x{diff:016x} 既非 0 也非 KERNEL_VA_BASE（镜像漂移）")
        if not (RAM_BASE <= p_paddr < RAM_TOP):
            problems.append(f"PT_LOAD paddr=0x{p_paddr:016x} 不在 QEMU RAM 内")
    if not boot_any:
        problems.append("没有 vaddr==paddr 的 PT_LOAD（.text.boot 不在低半？）")
    if not body_any:
        problems.append("没有 vaddr-paddr==KERNEL_VA_BASE 的 PT_LOAD（主体不在高半？）")

    # K-3：.text.boot 全部 ADRP 目标必低半（K-2 同族防复发哨兵，见模块文档）。
    problems += assert_boot_adrp_low(elf, path)

    if problems:
        for p in problems:
            print(f"berryos-size: assert-flip FAIL: {p}")
        sys.exit(1)
    print(f"berryos-size: assert-flip OK（boot 低半 + 主体高半镜像，entry=0x{e_entry:x}）")


# ---------- K-3 selftest：ADRP 解码器向量自检（--selftest） ----------
#
# 向量族（/tmp 原型期与 llvm-objdump 对拍 1:1 全等；真机条目随布局演进重取）：
#   A. 真机编码 5 条——两面 .text.boot 实取（K-3 L1 volatile 化后两面指令流
#      逐条相同）；word 只依赖 pc 页基，同页挪位不变；
#   B. 高半失败样本 2 条——decode 纯函数级钉「算得对」。B2（imm21=-2^20 的
#      -4GiB mod 2^64 回卷，word=0x9080000a）是**可链接**形态：±4GB 窗内合法
#      指令、链接器照收，唯 bit63 断言能拦——① 号断言的独立价值；B1 的高半
#      段目标从低半 pc 结构性不可编码（encode 亲测抛 ValueError = K-2 断链
#      形态，败在链接期），故 B1 只能以高半 pc 形态入纯函数向量。
SELFTEST_VECTORS = [
    (0xD000004A, 0x400000F8, 0x4000A000, "build_boot_pt: adrp x10, BOOT_L2_RAM"),
    (0xF000004C, 0x40000104, 0x4000B000, "build_boot_pt: adrp x12, BOOT_L1"),
    (0x9000006A, 0x40000154, 0x4000C000, "build_boot_pt: adrp x10, KERNEL_L1"),
    (0xF0000048, 0x40000184, 0x4000B000, "enable_mmu: adrp x8, BOOT_L1 → TTBR0"),
    (0x90000068, 0x40000190, 0x4000C000, "enable_mmu: adrp x8, KERNEL_L1 → TTBR1"),
    (0xF0FFFFE0, 0xFFFFFF8040001000, 0xFFFFFF8040000000,
     "高半失败样本 B1：高半 pc、目标同段（imm21=-1）"),
    (0x9080000A, 0x40000150, 0xFFFFFFFF40000000,
     "高半失败样本 B2：低半 pc、-4GiB 回卷 bit63=1（imm21=-2^20，可链接形态）"),
]

# 非 ADRP 对照（decode 必须返回 None）：adr 与字面池 .word 是 4B 扫描器的
# 理论误报面——今日节内零命中（对拍 llvm-objdump 数量立穿帮），向量钉死。
SELFTEST_NON_ADRP = [
    (0x1004786B, "adr x11——同族 PC 相对寻址但非 ADRP"),
    (0x4000007C, ".word 0x4000007c（boot asm 字面池）"),
    (0xFFFFFF80, ".word 0xffffff80（字面池高半地址低字）"),
    (0x00000000, "全零字"),
]

# encode 负样本：K-2 形态——低半 pc 引高半 cst16 池，页差 -8388608 页
# （≈-512GiB）超出 ±2^20 页窗，encode 必须抛 ValueError（断链在链接期）。
SELFTEST_ENCODE_OUT_OF_WINDOW = [
    (0x400000F8, 0xFFFFFF8040040000,
     "K-2 形态：低半 .text.boot 引高半 cst16 池——±4GB 窗外结构性不可编码"),
]


def selftest():
    """--selftest：纯函数向量 + 红/绿分支 + 四拒面。任一失败逐条打 FAIL 行并
    非零退出，全过打单行 ok（形态对照 mkfat.py selftest_puts）。"""
    fails = []
    # ① decode 向量（含 B 族高半样本——decode 是纯函数，只钉「算得对」）
    for word, pc, want, note in SELFTEST_VECTORS:
        got = decode_adrp_target(word, pc)
        if got != want:
            fails.append(f"decode(word=0x{word:08x}, pc=0x{pc:x}) = "
                         f"{hex(got) if got is not None else None} "
                         f"!= 0x{want:x} [{note}]")
    # ② 非 ADRP 对照（decode 必须返回 None）
    for word, note in SELFTEST_NON_ADRP:
        if decode_adrp_target(word, 0x40000000) is not None:
            fails.append(f"decode(word=0x{word:08x}) 误判 ADRP [{note}]")
    # ③ encode/decode 对偶 roundtrip（rd 位保真——word 逐位复现）
    for word, pc, want, note in SELFTEST_VECTORS:
        if encode_adrp(pc, want, rd=word & 0x1F) != word:
            fails.append(f"encode(pc=0x{pc:x}, target=0x{want:x}) "
                         f"!= 0x{word:08x} [{note}]")
    # ④ encode 负样本（K-2 断链形态必须抛 ValueError）
    for pc, target, note in SELFTEST_ENCODE_OUT_OF_WINDOW:
        try:
            encode_adrp(pc, target)
            fails.append(f"encode(pc=0x{pc:x}, target=0x{target:x}) "
                         f"未抛 ValueError [{note}]")
        except ValueError:
            pass
    # ⑤ 红分支：绿 ADRP + B2 高半字 + 非 ADRP 填充的合成节 → 恰 1 条高半 problem
    red = ((0xD000004A).to_bytes(4, "little")
           + (0x9080000A).to_bytes(4, "little")
           + (0x1004786B).to_bytes(4, "little"))
    adrps, problems = scan_boot_section(".text.boot", 0x40000000, 1, len(red), red)
    if len(adrps) != 2 or len(problems) != 1 \
            or "pc=0x40000004" not in problems[0] \
            or "0xffffffff40000000" not in problems[0]:
        fails.append(f"红分支形状不合：adrps={adrps!r} problems={problems!r}")
    # ⑥ 绿分支：单条真机 ADRP 的节 → 零 problem（bit63 断言只对高半红）
    green = (0xD000004A).to_bytes(4, "little")
    _, gproblems = scan_boot_section(".text.boot", 0x40000000, 1, 4, green)
    if gproblems:
        fails.append(f"绿分支误报：{gproblems!r}")
    # ⑦ 四拒面：节空 / NOBITS / 数据截断 / size%4!=0 / 零 ADRP——断言面失守
    #    必须各自 loud（截断样本钉 K-3 处置新增面：PROGBITS len!=sh_size 逃扫）
    _, p_empty = scan_boot_section(".text.boot", 0x40000000, 1, 0, b"")
    if not p_empty or "为空" not in p_empty[0]:
        fails.append(f"拒面「节空」未报：{p_empty!r}")
    _, p_nobits = scan_boot_section(".text.boot", 0x40000000, 8, 0x1000, b"")
    if not p_nobits or "NOBITS" not in p_nobits[0]:
        fails.append(f"拒面「NOBITS」未报：{p_nobits!r}")
    _, p_trunc = scan_boot_section(".text.boot", 0x40000000, 1, 8, b"\x00" * 4)
    if not p_trunc or "截断" not in p_trunc[0]:
        fails.append(f"拒面「数据截断」未报：{p_trunc!r}")
    _, p_odd = scan_boot_section(".text.boot", 0x40000000, 1, 5, b"\x00" * 5)
    if not p_odd or "4 字节倍数" not in p_odd[0]:
        fails.append(f"拒面「size%4!=0」未报：{p_odd!r}")
    _, p_zero = scan_boot_section(".text.boot", 0x40000000, 1, 4, b"\x00" * 4)
    if not p_zero or "ADRP 数 = 0" not in p_zero[0]:
        fails.append(f"拒面「零 ADRP」未报：{p_zero!r}")

    if fails:
        for f in fails:
            print(f"berryos-size: selftest FAIL: {f}")
        sys.exit(1)
    print(f"berryos-size: selftest ok: decode/non-ADRP/roundtrip/负样本 "
          f"{len(SELFTEST_VECTORS)}+{len(SELFTEST_NON_ADRP)}"
          f"+{len(SELFTEST_ENCODE_OUT_OF_WINDOW)} 向量 + 红/绿/四拒面全过")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--assert-flip":
        assert_flip(sys.argv[2])
    elif len(sys.argv) == 2 and sys.argv[1] == "--selftest":
        selftest()
    elif len(sys.argv) == 2:
        main(sys.argv[1])
    else:
        sys.exit(f"用法：{sys.argv[0]} <kernel-elf> | {sys.argv[0]} "
                 f"--assert-flip <kernel-elf> | {sys.argv[0]} --selftest")
