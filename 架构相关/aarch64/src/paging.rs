//! ARM64 stage-1 页表项定义与描述符构造（4KiB granule、39-bit VA）。
//!
//! 描述符格式见 ARMv8 ARM D8.2（VMSAv8-64 stage-1）。boot 页表用 **2MiB 块描述符**
//! （L2 叶子）做大区身份映射——这是启动期标准做法（rCore-aarch64 / Linux early boot
//! 同此），**不违反** ADR「无大页」：那条针对用户可见的大页特性 / 分配器，而非 boot
//! 期块映射。未来动态映射（Phase 2 每进程）用 4KiB 叶子（[`page_descriptor`] 已备）。
//!
//! 位布局与 zCore（MIT 参考）一致：table/page 描述符 bit[1]=1（0b11），块描述符 bit[1]=0
//! （0b01）；AF(bit10) 必须 1（QEMU 上否则 fault）。

/// 每张表项数（4KiB 表 / 8 字节）。
pub const PTES_PER_TABLE: usize = 512;

// ---- 描述符位（stage-1）----
const VALID: u64 = 1 << 0; // 有效
const TABLE_OR_PAGE: u64 = 1 << 1; // 表描述符 / L3 页描述符（块描述符此位=0）
const AF: u64 = 1 << 10; // Access Flag：QEMU 上必须为 1

/// MAIR attr 字段移位 + 取值。
const ATTRIDX_SHIFT: u64 = 2;
/// attr 索引 0 → MAIR attr0 = 0x00 = Device-nGnRnE。
pub const ATTR_DEVICE: u64 = 0 << ATTRIDX_SHIFT;
/// attr 索引 1 → MAIR attr1 = 0xFF = Normal WB RA+WA。
pub const ATTR_NORMAL: u64 = 1 << ATTRIDX_SHIFT;

/// 可共享性（stage-1 SH 字段，bits[9:8]）。
pub const SH_NONE: u64 = 0;
pub const SH_INNER: u64 = 0b11 << 8;

/// AP bits[7:6]=00：仅 EL1 读写（EL0 不可访问）。Phase 2 用户页放宽到 AP=01。
const AP_EL1_RW: u64 = 0 << 6;

/// MAIR_EL1 取值：attr0=Device-nGnRnE、attr1=Normal WB RA+WA、attr2=Normal 非缓存。
pub const MAIR_EL1: u64 = 0x00 | (0xFF << 8) | (0x44 << 16);

/// 构造 L1/L2 **表描述符**（指向下一级表）。bits[1:0]=0b11。
#[inline(always)]
pub const fn table_descriptor(next_table_addr: usize) -> u64 {
    (next_table_addr as u64) | VALID | TABLE_OR_PAGE
}

/// 构造 2MiB **块描述符**（L2 叶子）。bits[1:0]=0b01。
///
/// `pa` 须 2MiB 对齐（低位归零，与 bits[11:0] 的属性字段不重叠）。
#[inline(always)]
pub const fn block_descriptor(pa: usize, attr: u64, shareability: u64) -> u64 {
    (pa as u64) | VALID | attr | AP_EL1_RW | shareability | AF
}

/// 构造 4KiB **页描述符**（L3 叶子）。bits[1:0]=0b11。boot 表暂不用，Phase 2 动态映射用。
#[allow(dead_code)]
#[inline(always)]
pub const fn page_descriptor(pa: usize, attr: u64, shareability: u64) -> u64 {
    (pa as u64) | VALID | TABLE_OR_PAGE | attr | AP_EL1_RW | shareability | AF
}
