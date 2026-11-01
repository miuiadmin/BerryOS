/* berrydump 链接脚本（V-5a-0）——berryboot.ld 原样复刻（elf2pe.py 的形状
   契约单一事实源不变：单一 alloc 输出节 .img @ vaddr 0x1000 +
   --image-base 0 ⇒ PE 侧 RVA == ELF vaddr；.rela* 不 DISCARD——
   --emit-relocs 输出重定位是 elf2pe.py 转 PE .reloc 的输入）。 */

ENTRY(efi_main)

SECTIONS
{
    . = 0x1000;
    .img : {
        KEEP(*(.text .text.*))
        *(.rodata .rodata.*)
        *(.data .data.*)
        *(.bss .bss.*)
        *(COMMON)
    }
    /DISCARD/ : {
        *(.eh_frame*) *(.eh_frame_hdr)
        *(.comment)
        *(.debug*)
        *(.note*)
        *(.gnu*)
        *(.ARM.attributes)
        /* .rela* 不 DISCARD：同 berryboot.ld 注。 */
    }
}
