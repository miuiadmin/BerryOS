/* G4-2 dpgp 平像链接脚本（thrchild link-user.ld 同款结构）：基址 0x1000 =
   USER_CODE_VA（链接基址 == 加载基址，绝对寻址全部成立）。.text.entry
   首位保证 _start 落镜像第一指令（spawn entry 即 0x1000）。
   dpgp 无 .bss 面需求（无内嵌栈/哨兵——主线程直读即退），但保留同款
   段序（NOLOAD .bss 平像补零语义与 elf2flat --base 断言面一致）。
   布局（svcmgr dpg_selftest 装载侧镜像，两文件各持 const 改须双改）：
   像窗 [0x1000,0x6000) eager（image + 零页，主 sp=0x6000 窗顶）；demand
   VMO 一页 @ 0x6000（窗界外，读它必走缺页路径）。elf2flat --max 8192
   = 2 页墙（本像 1 页 + 头寸）。
   G4-4 X-2：第二入口 dpgi_entry 段钉 **0x2000**（`dpgi_start` ——I-abort
   腿专用；svcmgr `DPGI_ENTRY` 同值双改）——.text 膨胀过 0x2000 = 段
   VMA 叠 = lld 链接期炸响（fail loud）；像随 dpgi 页占满 2 页墙
   （[0x1000,0x3000) ≤ 8192）。段名 dpgi_entry 不匹配 .text.* 通配 =
   不会被首段吞并。 */
ENTRY(_start)

SECTIONS {
    . = 0x1000;
    .text : {
        KEEP(*(.text.entry))
        *(.text .text.*)
    }
    . = 0x2000;
    .dpgi : {
        KEEP(*(dpgi_entry))
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
