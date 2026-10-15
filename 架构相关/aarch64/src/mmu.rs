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

/// 内核空表（2b-2 低半退役）：TTBR0 的「无 AS」落点——全零 → 任何低半访问必然
/// fault（身份表退役的正向断言即由此而来）。放普通 `.bss`（高半 VMA，boot 期已清零；
/// 仅供高半代码经直映访问）；TTBR0 写入时按 `VMA - KERNEL_VA_BASE` 折算 PA。
static NULL_L1: L1Table = L1Table([0; PTES_PER_TABLE]);

/// 2b-2 回退开关（§5.1「保留身份表分支」）：`false` 时 [`retire_low_half`] 不动
/// TTBR0（低半身份继续可用），供问题定位时快速回退。
const RETIRE_IDENTITY: bool = true;

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

/// 高半自校验（2b-0 引入；2b-2 起**只走高半**——低半身份已退役，PA 别名侧探针
/// 移交 ktest 的期望故障正向断言）：
/// - RAM 探针 = 静态魔数经高半直映读回；
/// - 设备探针 = GICD_PIDR2（只读外设 ID 寄存器）经高半 Device 块读。
/// 任一翻译坏了都不是读到错值，而是**同步异常挂死**——能读回即证 TTBR1 生效。
pub(crate) fn kva_alias_probe() -> bool {
    static PROBE_MAGIC: u64 = 0x0B05_2B0B_5A5C_0FF0;
    // 主体符号已链高半：本体 VMA 在高半直映区。
    let va = (&raw const PROBE_MAGIC) as usize;
    // SAFETY: 只读自身静态量（Normal 直映），读无副作用。
    let ram_ok = unsafe { (va as *const u64).read_volatile() } == PROBE_MAGIC;
    const GICD_PIDR2: usize = GICD_BASE + 0x0FE8;
    // SAFETY: 只读外设 ID 寄存器（高半 Device 块；翻译坏则异常挂死，即失败信号）。
    let _dev = unsafe { crate::mmio::read(KERNEL_VA_BASE + GICD_PIDR2) };
    ram_ok
}

// ---- 2b-2：ASID + 每任务 TTBR0 激活 + 低半退役（内存管理设计 §5.1/§5.2）----

/// 探测 ASID 位宽：`ID_AA64MMFR0_EL1.ASIDBits`（bits[7:4]）——0 → 8 位、2 → 16 位；
/// 其余为架构保留值，保守取 8。QEMU `virt`/cortex-a72 实测值由 ktest 打进关卡记录。
pub(crate) fn asid_bits() -> u32 {
    let v: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {0}, id_aa64mmfr0_el1", out(reg) v, options(nostack, preserves_flags)) };
    match (v >> 4) & 0xf {
        2 => 16,
        _ => 8, // 0 → 8 位；保留值保守取 8
    }
}

/// ASID 在 TTBR0 / `tlbi aside1` 操作数里的移位量：16 位 → [63:48]、8 位 → [63:56]
/// （TCR.A1=0：硬件 ASID 取自 TTBR0，与 TCR.AS=0 的 8 位取位一致）。
#[inline(always)]
fn asid_shift() -> u32 {
    if asid_bits() == 16 { 48 } else { 56 }
}

/// 按 ASID 失效（`tlbi aside1is`；IS 口径——单核下与非 IS 等价，多核语义先立对）。
/// Vmar 的 unmap/protect 改叶后调用（2b-3 起独立出口，激活路径复用）。
pub(crate) unsafe fn tlbi_aside1(asid: u16) {
    let shift = asid_shift();
    asm!("tlbi aside1is, {0}", in(reg) (asid as u64) << shift, options(nostack, preserves_flags));
    dsb_ish();
    isb();
}

/// 激活任务地址空间：TTBR0 ← `ttbr0_pa` + ASID（`ttbr0_pa == 0` = 无 AS → 内核
/// 空表，低半必然 fault）；CONTEXTIDR.PROCID = asid（可观测性——硬件 ASID 恒取
/// TTBR0，A1=0）；`tlbi aside1is` 防上一轮同号映射经 TLB 残留（§5.1 坑「TLB」）。
///
/// 只做寄存器操作，**不负责页表内容屏障**——填表方须在调用前 `dsb ish`
/// （见内核 `aspace` 模块）。
pub(crate) unsafe fn activate_ttbr0(ttbr0_pa: usize, asid: u16) {
    let table = if ttbr0_pa == 0 {
        (&raw const NULL_L1) as usize - KERNEL_VA_BASE
    } else {
        ttbr0_pa
    };
    let shift = asid_shift();
    let ttbr = (asid as u64) << shift | table as u64;
    write_ttbr0_el1(ttbr);
    isb();
    // CONTEXTIDR_EL1：PROCID = asid（调试/trace 关联用；A1=0 下非功能位）。
    asm!(
        "msr contextidr_el1, {0}",
        in(reg) (asid as u64) << 13,
        options(nostack, preserves_flags)
    );
    // 同号旧映射失效。
    tlbi_aside1(asid);
}

/// 低半身份表退役（2b-2）：TTBR0 ← 内核空表（ASID 0）+ **全量 TLB 失效**。
/// 此后低半任何访问必然 fault——ktest 以期望故障正向断言之。
/// 用 `vmalle1is`：身份时代的低半 TLB 项是 global（boot 块描述符 nG=0），按号
/// 失效够不着 global 项；全量清（高半内核项随后经 TTBR1 重走查，无害）。
/// 回退开关：[`RETIRE_IDENTITY`]。
pub(crate) unsafe fn retire_low_half() {
    if !RETIRE_IDENTITY {
        return;
    }
    activate_ttbr0(0, 0);
    flush_all_asid();
}

/// 全量失效（ASID 分配器回卷 / 低半退役用）。注意：正确的「全量」指令是
/// `vmalle1is`（含 global 项；`aside1` 家族只按号失效、够不着 global）。
/// 设计文档 §5.2 原文写的 `tlbi aside1all` 并非真实助记符，此处即更正。
pub(crate) unsafe fn flush_all_asid() {
    asm!("tlbi vmalle1is", options(nostack, preserves_flags));
    dsb_ish();
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
