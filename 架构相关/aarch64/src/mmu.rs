//! ARM64 stage-1 **boot 页表** + 开启 MMU（plan Step 4，命门）。
//!
//! 身份映射（plan 裁决 #3）：`TTBR0_EL1 == TTBR1_EL1 == &BOOT_L1`，`VA == PA`。
//! 39-bit VA（T0SZ=T1SZ=25）、4KiB granule、3 级（L1→L2→L3）；boot 表用 L2 2MiB 块。
//!
//! 映射：
//!  - RAM `[RAM_BASE, RAM_TOP)`（含内核镜像 + 空闲 RAM）：**Normal WB**，identity；
//!  - UART `0x0900_0000`、GIC `0x0800_0000` 各所在 2MiB 块：**Device-nGnRnE**，identity。
//!
//! **最高风险点（任一漏=静默挂死无输出）**：开 MMU **前**设备 MMIO 必须先映进来——
//! 否则开启后下一次 `uart_putc` 写 VA 0x0900_0000 会同步外部中止、静默挂死。
//! cache 从 boot 起空（`boot.rs` 已 `sctlr_el1=0`，C=I=0），故建表后无需刷 cache，
//! 只需 `dsb ish` 保证表写入对页表遍历可见，再置 `sctlr_el1 = M|C|I`、`isb`。

use crate::paging::*;
use crate::{GICD_BASE, RAM_BASE, RAM_TOP, UART0_BASE};
use core::arch::asm;

const TWO_MB: usize = 1 << 21;
const L2_INDEX_MASK: usize = PTES_PER_TABLE - 1; // 9 位

/// L1 表（512 项，4KiB）。
#[repr(C, align(4096))]
struct L1Table([u64; PTES_PER_TABLE]);
/// L2 表（512 项，4KiB）。
#[repr(C, align(4096))]
struct L2Table([u64; PTES_PER_TABLE]);

// boot.asm 已清 .bss；静态零初始化 → 落 BSS（启动期 boot.asm 再清一次，幂等）。
static mut BOOT_L1: L1Table = L1Table([0; PTES_PER_TABLE]);
/// L1[0]：低区（设备 MMIO，`[0, 1GiB)`）。
static mut BOOT_L2_LOW: L2Table = L2Table([0; PTES_PER_TABLE]);
/// L1[1]：RAM 区（`[0x4000_0000, 0x8000_0000)`，实际映 `[RAM_BASE, RAM_TOP)`）。
static mut BOOT_L2_RAM: L2Table = L2Table([0; PTES_PER_TABLE]);

/// TCR_EL1：4KiB granule、39-bit VA（T0SZ=T1SZ=25）、inner shareable、WB 页表走查、IPS=0。
///
/// 逐字段拼（cortex-a72 / ARMv8.0）。TG1 编码与 TG0 不同（4KiB: TG0=00、TG1=10）。
const TCR_EL1: u64 = (25 << 0) // T0SZ
    | (0b01 << 8)  // IRGN0（页表走查 inner WB）
    | (0b01 << 10) // ORGN0
    | (0b11 << 12) // SH0 = inner shareable
    | (0b00 << 14) // TG0 = 4KiB
    | (25 << 16)   // T1SZ
    | (0b01 << 24) // IRGN1
    | (0b01 << 26) // ORGN1
    | (0b11 << 28) // SH1
    | (0b10 << 30) // TG1 = 4KiB
    | (0b000 << 32); // IPS = 32-bit PA（UART/GIC/RAM 全 < 4GiB）

/// SCTLR_EL1 使能值：`M | C | I`（开 MMU + 数据/统一 cache + 指令 cache）。A=0（不查对齐）。
const SCTLR_ENABLE: u64 = (1 << 0) | (1 << 2) | (1 << 12);

/// 建表 + 编程系统寄存器 + 开 MMU。
///
/// # Safety
/// 须在 EL1、MMU 关、启动单线程早期调用一次（由 HAL `init()` 保证）。
pub unsafe fn init() {
    build_boot_pt();
    enable_mmu();
}

unsafe fn build_boot_pt() {
    // `&raw mut`：取静态裸指针，避开 `static_mut_refs` 借用告警（G0 单访问点）。
    let l1 = &raw mut BOOT_L1;
    let l2_low = &raw mut BOOT_L2_LOW;
    let l2_ram = &raw mut BOOT_L2_RAM;

    // L1[0] → 低区 L2；L1[1] → RAM L2。
    (*l1).0[0] = table_descriptor(l2_low as usize);
    (*l1).0[1] = table_descriptor(l2_ram as usize);

    // 低区 L2：映 UART、GIC 所在的 2MiB 块为 Device（identity）。
    let dev_blocks = [
        UART0_BASE & !(TWO_MB - 1),
        GICD_BASE & !(TWO_MB - 1),
    ];
    for pa in dev_blocks {
        let idx = (pa >> 21) & L2_INDEX_MASK;
        (*l2_low).0[idx] = block_descriptor(pa, ATTR_DEVICE, SH_NONE);
    }

    // RAM L2：[RAM_BASE, RAM_TOP) 逐 2MiB Normal 块（identity）。
    // BOOT_L2_RAM 由 L1[1] 指向，覆盖 [0x4000_0000, 0x8000_0000)；
    // pa>>21 & 0x1FF 对 [RAM_BASE, RAM_TOP) 恰落在 [0, 64)。
    let mut pa = RAM_BASE;
    while pa < RAM_TOP {
        let idx = (pa >> 21) & L2_INDEX_MASK;
        (*l2_ram).0[idx] = block_descriptor(pa, ATTR_NORMAL, SH_INNER);
        pa += TWO_MB;
    }

    // 表写入对页表遍历可见。
    dsb_ish();
}

unsafe fn enable_mmu() {
    write_mair_el1(MAIR_EL1);
    write_tcr_el1(TCR_EL1);
    // 身份映射：TTBR0 == TTBR1 == 同一表（单表无切分）。
    let l1_addr = (&raw const BOOT_L1) as u64;
    write_ttbr0_el1(l1_addr);
    write_ttbr1_el1(l1_addr);
    isb();
    // TTBR / 表写入全部落定后再开 MMU。
    dsb_ish();
    // 切换点：M|C|I 一齐开。身份映射 → 下一条取指的 VA==PA 已映，无缝衔接。
    write_sctlr_el1(SCTLR_ENABLE);
    isb();
}

// ---- 系统寄存器读写（unsafe 收敛地，裸 asm）----

#[inline(always)]
unsafe fn write_mair_el1(v: u64) {
    asm!("msr mair_el1, {0}", in(reg) v, options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn write_tcr_el1(v: u64) {
    asm!("msr tcr_el1, {0}", in(reg) v, options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn write_ttbr0_el1(v: u64) {
    asm!("msr ttbr0_el1, {0}", in(reg) v, options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn write_ttbr1_el1(v: u64) {
    asm!("msr ttbr1_el1, {0}", in(reg) v, options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn write_sctlr_el1(v: u64) {
    asm!("msr sctlr_el1, {0}", in(reg) v, options(nostack));
}
#[inline(always)]
unsafe fn isb() {
    asm!("isb", options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn dsb_ish() {
    asm!("dsb ish", options(nostack, preserves_flags));
}
