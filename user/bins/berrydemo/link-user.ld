/* G-eco 用户程序平像链接脚本（docs/G-eco编译侧设计草案.md D1/D2）：
   基址 0x1000 = 内核 user.rs 的 USER_CODE_VA（UserProcess::build 恒映在
   0x1000，链接基址 == 加载基址，绝对寻址/adrp 全部成立）。.text.entry
   首位保证 _start 落镜像第一指令（spawn_user 的 entry 即 0x1000）。
   无 PIE/动态段（v1 固定基址，ASLR 留 v2）。
   栈另由 UserProcess::build 映在 [0x3000,0x4000)——镜像不得越过 0x3000：
   单页档由 elf2flat.py --max 4096 执法（多页放宽随 G-eco-b/D3）。 */
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
