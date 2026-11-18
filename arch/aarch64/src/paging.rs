//! ARM64 stage-1 页表项定义与描述符构造（4KiB granule、39-bit VA；F0-2 硬件 W^X 与 PXN/UXN）。
//!
//! 描述符格式见 ARM DDI 0487 D8.2 / D8.3（VMSAv8-64 stage-1）：
//! - `AP[2:1]`（bits[7:6]）：`00`=EL1 RW、`01`=EL1/EL0 RW、`10`=EL1 RO、`11`=EL1/EL0 RO；
//! - `PXN`（bit 53）：特权执行禁止（一切用户叶与内核非 `.text` 页必置 `1`）；
//! - `UXN`（bit 54）：非特权执行禁止（一切内核页与用户非代码页必置 `1`）。

/// 每张表项数（4KiB 表 / 8 字节）。
pub const PTES_PER_TABLE: usize = 512;

// ---- 描述符位（stage-1）----
const VALID: u64 = 1 << 0;
const TABLE_OR_PAGE: u64 = 1 << 1;
const AF: u64 = 1 << 10;

/// MAIR attr 字段移位 + 取值。
const ATTRIDX_SHIFT: u64 = 2;
/// attr 索引 0 → MAIR attr0 = 0x00 = Device-nGnRnE。
pub const ATTR_DEVICE: u64 = 0 << ATTRIDX_SHIFT;
/// attr 索引 1 → MAIR attr1 = 0xFF = Normal WB RA+WA。
pub const ATTR_NORMAL: u64 = 1 << ATTRIDX_SHIFT;

/// 可共享性（stage-1 SH 字段，bits[9:8]）。
pub const SH_NONE: u64 = 0;
pub const SH_INNER: u64 = 0b11 << 8;

/// AP bits[7:6]=00：仅 EL1 读写（EL0 无访问权）。
pub const AP_EL1_RW: u64 = 0b00 << 6;
/// AP bits[7:6]=01：EL1/EL0 均读写。
pub const AP_USER_RW: u64 = 0b01 << 6;
/// AP bits[7:6]=10：仅 EL1 只读（EL0 无访问权）。
pub const AP_RO: u64 = 0b10 << 6;
/// AP bits[7:6]=11：EL1/EL0 均只读（`UserRo` / `UserRx`）。
pub const AP_USER_RO: u64 = 0b11 << 6;

/// Privileged Execute-Never（bit 53）：禁止 EL1 取指执行（SMEP + 内核直映数据防执行）。
pub const PXN: u64 = 1 << 53;
/// Unprivileged Execute-Never（bit 54）：禁止 EL0 取指执行。
pub const UXN: u64 = 1 << 54;

/// MAIR_EL1 取值：attr0=Device-nGnRnE、attr1=Normal WB RA+WA、attr2=Normal 非缓存。
pub const MAIR_EL1: u64 = 0x00 | (0xFF << 8) | (0x44 << 16);

/// 构造 L1/L2 **表描述符**（指向下一级表）。bits[1:0]=0b11。
#[inline(always)]
pub const fn table_descriptor(next_table_addr: usize) -> u64 {
    (next_table_addr as u64) | VALID | TABLE_OR_PAGE
}

/// 构造 2MiB **块描述符**（boot 低半临时身份表用）。bits[1:0]=0b01。
#[inline(always)]
pub const fn block_descriptor(pa: usize, attr: u64, shareability: u64) -> u64 {
    (pa as u64) | VALID | attr | AP_EL1_RW | shareability | AF
}

/// 构造带 `PXN | UXN` 的 2MiB **不可执行块描述符**（`TTBR1` 高半直映 RAM 与设备 MMIO 用）。
#[inline(always)]
pub const fn block_descriptor_nx(pa: usize, attr: u64, shareability: u64) -> u64 {
    block_descriptor(pa, attr, shareability) | PXN | UXN
}

/// 构造 `TTBR1` 内核 4KiB **全局页描述符**（`nG=0`，跨 ASID 切换常驻 TLB；`perm` 含 `AP | PXN | UXN`）。
#[inline(always)]
pub const fn kernel_page_descriptor(pa: usize, attr: u64, shareability: u64, perm: u64) -> u64 {
    (pa as u64) | VALID | TABLE_OR_PAGE | attr | shareability | AF | perm
}

/// nG（non-Global，bit[11]）：用户叶子必置 1——per-ASID 语义的地基。
const NG: u64 = 1 << 11;

/// 构造 `TTBR0` 4KiB **用户页描述符基底**（L3 叶子，`nG=1`；调用方按 `Perm` OR 上 `AP | PXN | UXN`）。
#[inline(always)]
pub const fn page_descriptor(pa: usize, attr: u64, shareability: u64) -> u64 {
    (pa as u64) | VALID | TABLE_OR_PAGE | attr | AP_EL1_RW | shareability | AF | NG
}
