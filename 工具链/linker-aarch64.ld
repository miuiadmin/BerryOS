/* BerryOS aarch64 链接脚本（QEMU virt，RAM @ 0x40000000，128MiB）。 */
ENTRY(_start)

MEMORY {
    RAM (rwx) : ORIGIN = 0x40000000, LENGTH = 128M
}

/* Phase 1 Step 3：每段 4KiB 对齐，供 Step 4 开 MMU 后按段做 W^X 权限映射；
   并导出段起止 + __kernel_end（帧分配器下界，= 镜像含栈之后的页对齐地址）。 */
SECTIONS {
    . = 0x40000000;

    . = ALIGN(4096);
    __text_start = .;
    .text : {
        KEEP(*(.text.boot))   /* _start 必须物理在最前 */
        *(.text .text.*)
    } > RAM
    . = ALIGN(4096);
    __text_end = .;

    . = ALIGN(4096);
    __rodata_start = .;
    .rodata : { *(.rodata .rodata.*) } > RAM
    . = ALIGN(4096);
    __rodata_end = .;

    . = ALIGN(4096);
    __data_start = .;
    .data : { *(.data .data.*) } > RAM
    . = ALIGN(4096);
    __data_end = .;

    . = ALIGN(16);
    __bss_start = .;
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) } > RAM
    . = ALIGN(16);
    __bss_end = .;

    /* 256 KiB 栈（向下生长）。 */
    . = ALIGN(16);
    . = . + 0x40000;
    __stack_top = .;

    /* 帧分配器下界：内核镜像（含栈）之上的页对齐地址。
       free RAM = [__kernel_end, RAM_TOP)。 */
    . = ALIGN(4096);
    __kernel_end = .;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.eh_frame_hdr)
        *(.ARM.exidx*)
        *(.ARM.extab*)
    }
}
