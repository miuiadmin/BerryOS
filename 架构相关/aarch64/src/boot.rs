//! 内核启动入口 `_start`：检测异常级别，从 EL2/EL3 降到 EL1，设栈、清 `.bss`，调用 `rust_main`。
//!
//! 经链接脚本 `ENTRY(_start)` + `KEEP(.text.boot)` 根化；
//! HAL↔kernel 入口 `rust_main` 用 C-ABI 约定（ADR #18），符号名与 `bl rust_main` 精确匹配。
//! 串口标记：走 EL2 路径打 `'2'`、到达 EL1 打 `'1'`（EL3 罕见，桩打 `'3'`）。
use core::arch::global_asm;

global_asm!(
    ".section .text.boot",
    ".globl _start",
    ".type _start,%function",
    "_start:",
    "    mrs x0, CurrentEL",
    "    lsr x0, x0, #2",              // x0 = 异常级别 (1/2/3)
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
    "    ldr x0, =__stack_top",        // 设栈
    "    mov sp, x0",
    "    ldr x0, =__bss_start",        // 清 .bss
    "    ldr x1, =__bss_end",
    ".Lbss:",
    "    cmp x0, x1",
    "    b.ge .Lbss_done",
    "    str xzr, [x0], #8",
    "    b .Lbss",
    ".Lbss_done:",
    "    bl rust_main",
    ".Lhang:",                         // 绝不返回
    "    wfe",
    "    b .Lhang",
    ".size _start, . - _start",
);
