#!/usr/bin/env python3
"""BerryOS 内核体积基线报告 —— `just size` / CI 同款入口。

12 号报告 §10.1：BerryOS 自身 ELF 体积至今未测（红队缺口 1），
一切体积讨论的基线从 day-one 建起。零依赖解析 ELF64 头：

  - 文件总量（ROM/镜像体积）
  - PT_LOAD 汇总：filesz = 镜像内实占，memsz = 运行时静态内存
    （memsz - filesz ≈ .bss，即不占镜像但占 RAM 的部分）
  - alloc 段明细（.text / .rodata / .data / .bss / …）

只报数、不设阈值——基线先有数据，回归门禁等基线稳定后再定
（Tock 192KB 教训：Rust 体积劣化是无声发生的，得先能看见）。

`--assert-flip <elf>`：2b-1 高半翻转程序头断言（smoke 固化，内存管理设计 §5.1
「VMA≠LMA 调试地狱」的防漂移哨兵）：
  - e_entry 在低半（boot 段内）；
  - 每个 PT_LOAD 满足 vaddr==paddr（低半 boot：.text.boot/.bss.boot/.boot_stack）
    或 vaddr-paddr==0xFFFFFF8000000000（高半主体 = KERNEL_VA_BASE 镜像）；
  - 全部 paddr 落在 QEMU virt RAM [0x40000000, 0x80000000)。
"""
import os
import struct
import sys

EI_NIDENT = 16

KERNEL_VA_BASE = 0xFFFFFF8000000000
RAM_BASE, RAM_TOP = 0x40000000, 0x80000000


def u(buf, off, size):
    return int.from_bytes(buf[off:off + size], "little")


def main(path):
    with open(path, "rb") as f:
        elf = f.read()

    if elf[:4] != b"\x7fELF":
        sys.exit(f"size.py: 不是 ELF 文件：{path}")
    if elf[4] != 2 or elf[5] != 1:
        sys.exit(f"size.py: 只支持 ELF64 小端：{path}")

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
          f"  # ram-rom={ram - rom}B 为 .bss 类零填充")
    for vaddr, filesz, memsz, flags in loads:
        print(f"berryos-size:   LOAD vaddr=0x{vaddr:012x} filesz={filesz}B memsz={memsz}B flags=0x{flags:x}")
    for name, file_sz, mem_sz in sections:
        if file_sz or mem_sz:
            print(f"berryos-size:   {name:12s} file={file_sz:8d}B mem={mem_sz:8d}B")


def assert_flip(path):
    """2b-1 高半翻转程序头断言（见模块文档）。失败非零退出（smoke FAIL）。"""
    with open(path, "rb") as f:
        elf = f.read()
    if elf[:4] != b"\x7fELF":
        sys.exit(f"size.py: 不是 ELF 文件：{path}")
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

    if problems:
        for p in problems:
            print(f"berryos-size: assert-flip FAIL: {p}")
        sys.exit(1)
    print(f"berryos-size: assert-flip OK（boot 低半 + 主体高半镜像，entry=0x{e_entry:x}）")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--assert-flip":
        assert_flip(sys.argv[2])
    elif len(sys.argv) == 2:
        main(sys.argv[1])
    else:
        sys.exit(f"用法：{sys.argv[0]} <kernel-elf> | {sys.argv[0]} --assert-flip <kernel-elf>")
