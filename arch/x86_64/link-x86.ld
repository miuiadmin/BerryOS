/* P7-x0 x86_64 最小集引导链接脚本（ADR #41 范围盒子）。
 *
 * 布局：链接基址 0x100000（1MiB，低 RAM 常规内核位；VMA==PMA 恒等——
 * PVH 入口期分页关闭，物理=线性）。首 1GiB 由 boot32 弹床以 512 × 2MiB
 * 大页恒等映射（qemu64 默认 CPU 不广播 1GiB 页，2MiB 是可移植面）。
 *
 * 两条纪律：
 *  - .note 绝不 DISCARD（用户态 link-user.ld 的 `*(.note*)` 弃单在这里是
 *    大忌）：PVH 引导协议靠 ELF note `Xen`/type 18（XEN_ELFNOTE_PHYS32_ENTRY）
 *    向 QEMU 主侧声明 32 位入口，丢 note = QEMU 按 bzImage 面拒收 ELF64。
 *  - .note 单列 PT_NOTE 段（QEMU 扫 program header 面不扫 section 表）。
 */
OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

PHDRS {
    text   PT_LOAD  FLAGS(5); /* R-X */
    rodata PT_LOAD  FLAGS(4); /* R-- */
    note   PT_NOTE  FLAGS(4); /* R--：PVH Xen note */
    data   PT_LOAD  FLAGS(6); /* RW- */
}

SECTIONS {
    . = 0x100000;

    /* boot32 弹床置首（PVH 32 位入口；顺序非协议要求，集中可读性）。
     * .usrprog（P7-x4 CPL3 用户自证程序）显式并入——拒绝孤儿段漂移：运行期
     * 拷到 0x200000 执行，但字节源必须在镜像内且随 PT_LOAD 装载到位。 */
    .text : {
        KEEP(*(.text.boot))
        *(.text .text.*)
        *(.usrprog)
    } :text

    . = ALIGN(0x1000);
    .rodata : ALIGN(4) {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    } :rodata

    .note : ALIGN(4) {
        *(.note .note.*)
    } :note

    . = ALIGN(0x1000);
    .data : ALIGN(16) {
        *(.data .data.*)
        *(.sdata .sdata.*)
    } :data

    .bss (NOLOAD) : ALIGN(16) {
        __bss_start = .;
        *(.sbss .sbss.*)
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(16);
        __bss_end = .;
    } :data

    __kernel_end = .;

    /DISCARD/ : {
        *(.comment)
        *(.eh_frame*)
        /* 无 .note*——见文件头纪律 */
    }
}
