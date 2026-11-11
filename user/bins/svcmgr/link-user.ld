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
    /* X-3c-3：pager 任务入口/栈钉位（内核 boot 直拉第二裸任务共享本映像
       ——kernel/src/main.rs X-3c-1 同值双改：入口 0x1E000 / 栈顶 0x20000；
       dpgp `dpgi_entry` 同款 idiom）。栈区 [入口段尾, 0x20000) NOLOAD
       （elf2flat memsz 尾零垫覆盖——.bss 同机制，UserProcess::build 整
       映像 RW 映射即含栈页）。text+bss 长过 0x1E000 = 位置计数器回退 →
       链接失败（fail loud 二道几何钉：128KiB --max 帽之前先炸）。段名
       pager_entry 不匹配 .text.* 通配 = 不会被首段吞并。 */
    . = 0x1E000;
    .pager_entry : {
        KEEP(*(pager_entry))
    }
    . = ALIGN(16);
    .pager_stack (NOLOAD) : {
        . = 0x20000;
    }
    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) }
}
