/* BerryOS aarch64 链接脚本（QEMU virt，RAM @ 0x40000000，128MiB）。
   2b-1 高半翻转（内存管理设计 §5.1）：
   - 低半（VMA == LMA）：.text.boot（EL 检测/降级、清 bss、建表、开 MMU、跳高半）
     + 32KiB 早期栈 + .bss.boot（4 张 boot 页表——早期代码 ADRP 只能 ±4GB，
       引用不了高半静态量，故表留低半）。
   - 高半主体：VMA = KERNEL_VA_BASE + LMA（镜像布局）——.data/.bss **零拷贝**
     （QEMU 按 p_paddr 落内存，高半直映同一物理页）；每段显式 AT()，不依赖
     LMA 计数器语义。程序头断言见 tools/size.py --assert-flip（smoke 固化）。 */
ENTRY(_start)

KERNEL_VA_BASE = 0xFFFFFF8000000000;

SECTIONS {
    /* ---- 低半（VMA == LMA）：MMU 开之前的执行/数据 ---- */
    . = 0x40000000;
    . = ALIGN(4096);
    .text.boot : { KEEP(*(.text.boot)) }
    __boot_text_end = .;

    /* 早期栈 32KiB（纯保留，无段内容）。 */
    . = ALIGN(16);
    . += 0x8000;
    __boot_stack_top = .;

    . = ALIGN(4096);
    __boot_bss_start = .;
    .bss.boot (NOLOAD) : { KEEP(*(.bss.boot)) }
    . = ALIGN(4096);
    __boot_bss_end = .;

    /* 主体 LMA 起点（低半，页对齐；主体各段 VMA - KERNEL_VA_BASE == LMA）。 */
    . = ALIGN(4096);
    __body_lma = .;

    /* ---- 高半主体：每段 4KiB 对齐（W^X 权限映射预留）+ 显式 AT 镜像 ---- */
    . = __body_lma + KERNEL_VA_BASE;
    __text_start = .;
    .text : AT(__text_start - KERNEL_VA_BASE) { *(.text .text.*) }
    . = ALIGN(4096);
    __text_end = .;

    __rodata_start = .;
    .rodata : AT(__rodata_start - KERNEL_VA_BASE) { *(.rodata .rodata.*) }
    . = ALIGN(4096);
    __rodata_end = .;

    __data_start = .;
    .data : AT(__data_start - KERNEL_VA_BASE) { *(.data .data.*) }
    . = ALIGN(4096);
    __data_end = .;

    . = ALIGN(16);
    __bss_start = .;
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) }
    . = ALIGN(16);
    __bss_end = .;

    /* 256 KiB 主栈（向下生长；高半 VMA，跳高半后才装 SP）。 */
    . = ALIGN(16);
    . += 0x40000;
    __stack_top = .;

    /* 帧分配器下界（高半 VA）。free RAM = [__kernel_end_pa, RAM_TOP)。 */
    . = ALIGN(4096);
    __kernel_end = .;

    /* PA 侧镜像符号：boot 期（MMU 未开）清主体 .bss 用（asm ldr = 绝对值装载，
       无 ADRP 范围问题）。 */
    __bss_start_pa = __bss_start - KERNEL_VA_BASE;
    __bss_end_pa = __bss_end - KERNEL_VA_BASE;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.eh_frame_hdr)
        *(.ARM.exidx*)
        *(.ARM.extab*)
    }
}
