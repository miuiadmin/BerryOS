/* berryboot 链接脚本（V-0a 刀 1）——elf2pe.py 的形状契约：
** 单一 alloc 输出节 .img @ vaddr 0x1000**（全部内容并进一节）+
** --image-base 0** ⇒ PE 侧 RVA == ELF vaddr，一切 PC 相对立即数原样有效。
   elf2pe.py 对此执法：多 alloc 节 / .img≠0x1000 即 loud FAIL。
   .bss 并进文件面（零填充，VirtualSize==SizeOfRawData——加载器零清面
   留 v2 再用 NOLOAD 缩文件）。
   W^X 拆分（text RX / data RW 两节）留 V-1+——v0 骨架单 RWX 节。 */

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
        /* .rela* 不 DISCARD：--emit-relocs 的输出重定位是 elf2pe.py 的
           输入（ABS64 → PE DIR64——丢了 = 静态指针槽无 .reloc 可搬）。 */
    }
}
