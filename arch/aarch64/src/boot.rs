//! 内核启动入口 `_start`：检测异常级别，从 EL2/EL3 降到 EL1，设早期低半栈、
//! 清 `.bss.boot` 与主体 `.bss`（PA 侧），建表开 MMU（`build_boot_pt`/`enable_mmu`），
//! 换高半主栈后 `br rust_main`——2b-1 起内核主体在高半执行（`内存管理设计.md` §5.1）。
//!
//! 经链接脚本 `ENTRY(_start)` + `KEEP(.text.boot)` 根化；
//! HAL↔kernel 入口 `rust_main` 用 C-ABI 约定（ADR #18），符号名与 asm 精确匹配。
//! 串口标记：走 EL2 路径打 `'2'`、到达 EL1 打 `'1'`（EL3 罕见，桩打 `'3'`）。
use core::arch::global_asm;

global_asm!(
    ".section .text.boot",
    ".globl _start",
    ".type _start,%function",
    "_start:",
    "    mov x19, x0",              // 存 QEMU 传入 x0（DTB PA——G-eco 入机侧）：
    "    mrs x0, CurrentEL",        // EL 检测要占 x0；x19 为 AAPCS callee-saved，
    "    lsr x0, x0, #2",           // x0 = 异常级别 (1/2/3)
    "    cmp x0, #2",
    "    b.eq .Lfrom_el2",
    "    cmp x0, #3",
    "    b.eq .Lfrom_el3",
    "    b .Lat_el1",

    // ---- 从 EL2 降到 EL1 ----
    ".Lfrom_el2:",
    "    mrs x0, sctlr_el2",
    "    bic x0, x0, #1",              // 清 M 位：关 EL2 MMU
    "    msr sctlr_el2, x0",
    "    isb",
    "    tlbi alle2",
    "    dsb sy",
    "    isb",
    "    movz x0, #0x8000, lsl #16",   // HCR_EL2.RW=1 → EL1 为 AArch64
    "    msr hcr_el2, x0",
    "    msr sctlr_el1, xzr",          // EL1 MMU 干净状态
    "    isb",
    "    ldr x1, =0x09000000",         // 串口标记 '2'：走了 EL2 路径
    "    mov x0, #0x32",
    "    str w0, [x1]",
    "    ldr x0, =.Lat_el1",
    "    msr elr_el2, x0",
    "    mov x0, #0x3c5",              // EL1h + DAIF 屏蔽
    "    msr spsr_el2, x0",
    "    eret",

    // ---- EL3（QEMU virt 默认不出现；桩，Phase 1+ 补正确 EL3→EL2→EL1）----
    ".Lfrom_el3:",
    "    ldr x1, =0x09000000",
    "    mov x0, #0x33",               // 串口标记 '3'
    "    str w0, [x1]",
    "    b .Lfrom_el2",

    // ---- EL1 入口（直达 或 eret 后）----
    ".Lat_el1:",
    "    ldr x1, =0x09000000",         // 串口标记 '1'：到达 EL1
    "    mov x0, #0x31",
    "    str w0, [x1]",
    "    movz x0, #0x30, lsl #16",     // CPACR_EL1.FPEN=0b11：显式开 EL1 FP/SIMD（ADR #34）
    "    msr cpacr_el1, x0",           // 消 reset 依赖：QEMU 8.2 reset=FPEN 0（架构值），
    "    isb",                         // 编译器 SIMD memset 一执行即 EC=0x07 陷阱（CI 实测）；
    "    ldr x0, =__boot_stack_top",   // QEMU 11 reset 宽容。用户态 FP 陷+保存归 Phase 2b。
    "    mov sp, x0",                  // 早期低半栈（VMA==LMA）：EL 检测到此全程用它
    "    ldr x0, =__boot_bss_start",   // 清 .bss.boot（4 张页表，低半 VMA==PA）
    "    ldr x1, =__boot_bss_end",
    ".Lbss_boot:",
    "    cmp x0, x1",
    "    b.ge .Lbss_boot_done",
    "    str xzr, [x0], #8",
    "    b .Lbss_boot",
    ".Lbss_boot_done:",
    "    ldr x0, =__bss_start_pa",     // 清主体 .bss：其 VMA 在高半，MMU 未开 → 走 PA 侧
    "    ldr x1, =__bss_end_pa",       // （镜像布局：PA = VMA - KERNEL_VA_BASE = LMA）
    ".Lbss:",
    "    cmp x0, x1",
    "    b.ge .Lbss_done",
    "    str xzr, [x0], #8",
    "    b .Lbss",
    ".Lbss_done:",
    "    bl build_boot_pt",            // 建表（mmu.rs，#[link_section .text.boot]；
    "    bl enable_mmu",               //  表在 .bss.boot 低半 → 函数内低半寻址即可）
    "    ldr x1, =__stack_top",     // 换高半主栈 + 跳高半入口（此后不再回低半执行；
    "    mov sp, x1",               //  本段 VA==PA 走身份 TTBR0，开 MMU 无缝衔接）
    "    ldr x1, =rust_main",       // C-ABI 首参 = 入口 x0（DTB/handoff PA，
    "    mov x0, x19",              // V-1a 起内核按 magic 分流）；x19 跨两个
    "    br x1",                    // extern "C" bl（boot_pt/mmu）存活至此
    ".Lhang:",                         // 绝不返回
    "    wfe",
    "    b .Lhang",
    ".size _start, . - _start",
);
