/* berry-verify 链接脚本（hello link-user.ld 同款结构——rootfs ELF 消费
   形，非 elf2flat 平像）：基址 0x1000 = USER_CODE_VA；.text.entry 首位
   保证 _start 落镜像第一指令（spawn_rootfs_elf 的 ELF 入口即此）。
   映幅由 berry-spawngeo ELF_MAP_MAX（16MiB）+ 自身 ≤10KB 红线执法。 */
ENTRY(_start)

SECTIONS {
    . = 0x1000;
    .text : {
        KEEP(*(.text.entry))
        *(.text .text.*)
    }
    . = ALIGN(16);
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }
    . = ALIGN(16);
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }
    . = ALIGN(16);
    .bss (NOLOAD) : {
        __bss_start = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        . = ALIGN(8);
        __bss_end = .;
    }
    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) }
}
