//! ARM64 stage-1 **boot 页表** + 开启 MMU（plan Step 4，命门）。
//!
//! 2b-1 高半翻转后（`内存管理设计.md` §5.1）：内核主体在高半执行；本模块的
//! `build_boot_pt`/`enable_mmu` 留 `.text.boot` 低半（VMA==LMA，boot asm 在开 MMU
//! 前后 `bl` 调用），4 张表留 `.bss.boot` 低半（VMA==PA：早期代码 ADRP 只有 ±4GB，
//! 引用不了高半静态量；也使 TTBR 写入的地址算式恒为 PA，无需 virt_to_phys）。
//!
//! 映射（低半 PA 与高半 `KERNEL_VA_BASE+pa` 两个别名同此规则）：
//!  - RAM `[RAM_BASE, RAM_TOP)`（含内核镜像 + 空闲 RAM）：**Normal WB**；
//!  - UART `0x0900_0000`、GIC `0x0800_0000` 各所在 2MiB 块：**Device-nGnRnE**。
//!  - TTBR0 = BOOT_L1 身份表（**临时**，2b-2 退役——届时低半访问必然 fault 是正向断言）；
//!    TTBR1 = KERNEL_L1 高半内核表（canonical，正式）。
//!
//! **最高风险点（任一漏=静默挂死无输出）**：开 MMU **前**设备 MMIO 必须先映进来——
//! 否则开启后下一次 `uart_putc` 写 VA 0x0900_0000 会同步外部中止、静默挂死。
//! cache 从 boot 起空（`boot.rs` 已 `sctlr_el1=0`，C=I=0），故建表后无需刷 cache，
//! 只需 `dsb ish` 保证表写入对页表遍历可见，再置 `sctlr_el1 = M|C|I`、`isb`。

use crate::paging::*;
use crate::{GICD_BASE, KERNEL_VA_BASE, RAM_BASE, RAM_TOP, UART0_BASE};
use core::arch::asm;

const TWO_MB: usize = 1 << 21;
const L2_INDEX_MASK: usize = PTES_PER_TABLE - 1; // 9 位

/// L1 表（512 项，4KiB）。
#[repr(C, align(4096))]
struct L1Table([u64; PTES_PER_TABLE]);
/// L2 表（512 项，4KiB）。
#[repr(C, align(4096))]
struct L2Table([u64; PTES_PER_TABLE]);

// boot.asm 先清 .bss.boot（__boot_bss_*）；静态零初始化 → NOLOAD 低半段。
// 2b-2 低半退役后，这些表经 phys_to_virt(pa==va) 访问，规则不变。
#[link_section = ".bss.boot"]
static mut BOOT_L1: L1Table = L1Table([0; PTES_PER_TABLE]);
/// L1[0]：低区（设备 MMIO，`[0, 1GiB)`）。
#[link_section = ".bss.boot"]
static mut BOOT_L2_LOW: L2Table = L2Table([0; PTES_PER_TABLE]);
/// L1[1]：RAM 区（`[0x4000_0000, 0x8000_0000)`，实际映 `[RAM_BASE, RAM_TOP)`）。
#[link_section = ".bss.boot"]
static mut BOOT_L2_RAM: L2Table = L2Table([0; PTES_PER_TABLE]);

/// TTBR1 内核表（2b-0 引入）。条目与 BOOT_L1 同构——指向**同一对 L2 表**：
/// 描述符输出的是 PA，与从哪张 L1 走查无关；`KERNEL_VA_BASE` bits[38:0]=0，
/// 故 KVA[38:30] == PA[38:30]，高半 L1 索引镜像身份 L1 索引。2b-2 低半退役后
/// 这张表成为内核唯一页表根。
#[link_section = ".bss.boot"]
static mut KERNEL_L1: L1Table = L1Table([0; PTES_PER_TABLE]);

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
/// 2b-1 起由 boot asm 在开 MMU 前后直接 `bl` 调用（顺序固定：先本函数后
/// [`enable_mmu`]）；不再经 HAL `init()`——rust_main 进来时 MMU 已开、在高半。
///
/// # Safety
/// 须在 EL1、MMU 关、启动单线程早期调用一次（由 boot asm 保证）；
/// 本函数与所调 paging 辅助全部落 `.text.boot` / 表落 `.bss.boot`（低半）。
#[no_mangle]
#[link_section = ".text.boot"]
pub unsafe extern "C" fn build_boot_pt() {
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

    // 2b-0：TTBR1 内核表——高半 KVA 与低半 PA 走同一对 L2（见 KERNEL_L1 文档）。
    let k_l1 = &raw mut KERNEL_L1;
    (*k_l1).0[0] = table_descriptor(l2_low as usize);
    (*k_l1).0[1] = table_descriptor(l2_ram as usize);

    // 表写入对页表遍历可见。
    dsb_ish();
}

#[no_mangle]
#[link_section = ".text.boot"]
unsafe extern "C" fn enable_mmu() {
    write_mair_el1(MAIR_EL1);
    write_tcr_el1(TCR_EL1);
    // TTBR0 = 身份表（临时，2b-2 退役）；TTBR1 = 高半内核表（2b-0 起）。
    // 表在 .bss.boot 低半（VMA==PA），地址即 PA——2b-2 后新增表才需 virt_to_phys。
    write_ttbr0_el1((&raw const BOOT_L1) as u64);
    write_ttbr1_el1((&raw const KERNEL_L1) as u64);
    isb();
    // TTBR / 表写入全部落定后再开 MMU。
    dsb_ish();
    // 切换点：M|C|I 一齐开。本函数在 .text.boot（VA==PA，身份 TTBR0 已映），
    // 开启后下一条取指无缝；跳高半由 boot asm 随后完成（换 __stack_top 后 br）。
    write_sctlr_el1(SCTLR_ENABLE);
    isb();
}

/// 高半别名自校验：同一物理位置经低半身份（TTBR0）与高半直映（TTBR1）两个
/// 窗口读，证两套翻译一致且生效（2b-0 引入；2b-1 起主体高半执行，双别名 =
/// 低半 PA × 高半 VMA）。
///
/// RAM 探针 = 静态魔数双别名读回比对；设备探针 = GICD_PIDR2（只读外设 ID 寄存器，
/// 读无副作用）双别名读值一致。任一不符 → 翻译有错（TTBRx/TCR/表内容），
/// 必须在推进 2b-2 前暴露。2b-2 低半退役时本探针改为只走高半 + 断言低半 fault。
pub(crate) fn kva_alias_probe() -> bool {
    static PROBE_MAGIC: u64 = 0x0B05_2B0B_5A5C_0FF0;
    // 主体符号已链高半：本体 VMA 在高半，PA = VMA - KERNEL_VA_BASE（镜像布局）。
    let va = (&raw const PROBE_MAGIC) as usize;
    let pa = va - KERNEL_VA_BASE;
    // SAFETY: 两别名指向同一物理页（身份 TTBR0 / 高半 TTBR1 直映），读无副作用。
    let ram_ok = unsafe { (va as *const u64).read_volatile() } == PROBE_MAGIC
        && unsafe { (pa as *const u64).read_volatile() } == PROBE_MAGIC;
    const GICD_PIDR2: usize = GICD_BASE + 0x0FE8;
    // SAFETY: 只读外设 ID 寄存器；两别名走同一 L2 Device 块（同一 BOOT_L2_LOW）。
    let v_pa = unsafe { crate::mmio::read(GICD_PIDR2) };
    let v_kva = unsafe { crate::mmio::read(KERNEL_VA_BASE + GICD_PIDR2) };
    ram_ok && v_pa == v_kva
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
