/* T-1 thrchild 平像链接脚本（hello link-user.ld 同款结构）：基址 0x1000 =
   USER_CODE_VA（链接基址 == 加载基址，绝对寻址全部成立）。.text.entry
   首位保证 _start 落镜像第一指令（spawn entry 即 0x1000）。
   t2 线程栈 STACK2 2048B 内嵌 .bss（#[repr(align(16))] 满足内核
   thread_start 的 sp%16==0 门）——与代码共页，平像恰 1 页（8192 栈版
   会把平像推 3 页 → svcmgr 内嵌总量撞破 64KiB USER_CODE_MAX 帽，故
   收紧）；主栈在像外由 svcmgr 加宽映射另给（THR_MAP_LEN [0x1000,0x7000)，
   主 sp=0x7000；elf2flat --max 16384 = 4 页墙——镜像最大 4 页时主栈区
   仍余 2 页）。 */
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
