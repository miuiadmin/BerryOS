/* BerryOS aarch64 链接脚本（QEMU virt，RAM @ 0x40000000）。 */
ENTRY(_start)

MEMORY {
    RAM (rwx) : ORIGIN = 0x40000000, LENGTH = 128M
}

SECTIONS {
    . = 0x40000000;

    .text : {
        KEEP(*(.text.boot))   /* _start 必须物理在最前 */
        *(.text .text.*)
    } > RAM

    .rodata : { *(.rodata .rodata.*) } > RAM
    .data : { *(.data .data.*) } > RAM

    . = ALIGN(16);
    __bss_start = .;
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) } > RAM
    . = ALIGN(16);
    __bss_end = .;

    /* 256 KiB 栈。 */
    . = ALIGN(16);
    . = . + 0x40000;
    __stack_top = .;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.eh_frame_hdr)
        *(.ARM.exidx*)
        *(.ARM.extab*)
    }
}
