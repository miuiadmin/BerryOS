//! berry-vslotface：Vmar 槽位/区间判定律纯面（W3-Q17）。
//!
//! 自 `kernel/src/vmar.rs` 外迁的纯函数判序（零 unsafe、零 HAL 依赖）：
//! - [`check_slot_const`]：槽位三查判序——对齐（vaddr/len 双 4KiB 倍 + 非零）→
//!   管辖界（checked 加法防回卷 + base..base+span 闭开域）→ 重叠扫描。
//!   三查次序即旧内核逐字序：Unaligned 先于 OutOfBounds 先于 Overlap
//!   （错误码优先级钉板——测试 `check_unaligned_first` 钉死）。
//! - [`find_exact_at`]/[`find_mapped_at`]：精确匹配查找（start+len 双等 +
//!   kind 过滤——exact 收 Mapped|Demand、mapped 收 Mapped-only）。
//! - [`demand_hit_at`]：fault 探针命中律——闭开域包含 + kind == Demand。
//!
//! 两条消费形态（kernel 零分配接线）：
//! 1. **const 谓词**（`*_const`）：无表——对齐/界两查编译期可评，重叠由
//!   消费方逐区间调 [`overlaps`] 组合（kernel `check_slot` 壳 = 两查 +
//!   `regions.iter().any(overlaps)`，与旧序逐字等价）。
//! 2. **trait 泛型**（`*_at`）：[`SlotView`] 抽象「只读区间表」——消费方
//!   impl 一次（`len()`/`get(i)`），四个判定律全表驱动可用；host 测试用
//!   `[SlotRegion]` 切片直接驱动。
//!
//! 纪律与 W3-Q2/Q5/Q14/Q15/Q16 同款：判定律单源迁此，kernel vmar.rs
//! 消费面换调用，行为逐字等价（含错误码次序）。

#![no_std]

use core::result::Result;

/// 页大小（4 KiB = 0x1000）。
pub const PAGE_SIZE: usize = 0x1000;

/// 区间种类（判定面镜像——kernel `RegionKind` 的纯投影，不携带 Perm/Vmo）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// 预留未映射。
    Reserved,
    /// 已映射（供给完成）。
    Mapped,
    /// demand 区间（缺页按需供给）。
    Demand,
}

/// 已登记区间纯快照（判定律只看 start/len/kind 三元）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotRegion {
    pub start: usize,
    pub len: usize,
    pub kind: SlotKind,
}

/// 判定错误（kernel `VmarError` 判定子集的纯投影，次序即优先级）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotError {
    /// 非对齐或零长（三查第一关）。
    Unaligned,
    /// 界外或地址回卷（三查第二关）。
    OutOfBounds,
    /// 与既有区间重叠（三查第三关）。
    Overlap,
    /// 未登记（查找面）。
    NotMapped,
}

/// 只读区间表抽象（消费方零分配接线——kernel `Vec<Region>` 直投）。
pub trait SlotView {
    /// 区间数。
    fn slot_len(&self) -> usize;
    /// 第 `i` 个区间的纯快照（i < slot_len）。
    fn slot_get(&self, i: usize) -> SlotRegion;
}

/// `[SlotRegion]` 切片天然实现（host 测试直驱）。
impl SlotView for [SlotRegion] {
    fn slot_len(&self) -> usize {
        self.len()
    }
    fn slot_get(&self, i: usize) -> SlotRegion {
        self[i]
    }
}

/// 定长数组同款直驱。
impl<const N: usize> SlotView for [SlotRegion; N] {
    fn slot_len(&self) -> usize {
        N
    }
    fn slot_get(&self, i: usize) -> SlotRegion {
        self[i]
    }
}

// ---- 查 1/3 + 查 2/3：const 谓词（编译期可评 + 消费方组合重叠扫描）----

/// 查 1：对齐 + 非零长（三查第一关）。
pub const fn aligned_nonzero(vaddr: usize, len: usize) -> bool {
    vaddr % PAGE_SIZE == 0 && len % PAGE_SIZE == 0 && len != 0
}

/// 查 2：管辖界（含回卷拒）——`Some(端点)` 辖内 / `None` 界外或回卷。
pub const fn bounds_end(vaddr: usize, len: usize, base: usize, span: usize) -> Option<usize> {
    if aligned_nonzero(vaddr, len) {
        match vaddr.checked_add(len) {
            Some(end) => {
                if vaddr >= base && end <= base + span {
                    Some(end)
                } else {
                    None
                }
            }
            None => None,
        }
    } else {
        None
    }
}

/// 查 3：半开重叠谓词——`[vaddr, end)` 与 `[r.start, r.start+r.len)` 有交。
pub const fn overlaps(vaddr: usize, end: usize, r_start: usize, r_len: usize) -> bool {
    vaddr < r_start + r_len && r_start < end
}

/// 槽位三查完整判序（表驱动全量形——host 钉板/轻量消费方用）。
pub fn check_slot_of<V: SlotView + ?Sized>(
    vaddr: usize,
    len: usize,
    base: usize,
    span: usize,
    regions: &V,
) -> Result<(), SlotError> {
    if !aligned_nonzero(vaddr, len) {
        return Err(SlotError::Unaligned);
    }
    let end = match bounds_end(vaddr, len, base, span) {
        Some(e) => e,
        None => return Err(SlotError::OutOfBounds),
    };
    let mut i = 0;
    while i < regions.slot_len() {
        let r = regions.slot_get(i);
        if overlaps(vaddr, end, r.start, r.len) {
            return Err(SlotError::Overlap);
        }
        i += 1;
    }
    Ok(())
}

/// const 上下文可评的完整三查（无表形——空表即纯两查 + 零区间）。
pub const fn check_slot_const(
    vaddr: usize,
    len: usize,
    base: usize,
    span: usize,
) -> Result<(), SlotError> {
    if !aligned_nonzero(vaddr, len) {
        return Err(SlotError::Unaligned);
    }
    match bounds_end(vaddr, len, base, span) {
        Some(_) => Ok(()),
        None => Err(SlotError::OutOfBounds),
    }
}

// ---- 查找面 ----

/// 精确匹配查找（Mapped 或 Demand——G4-1 起 unmap 对 demand 区间同收割）。
///
/// `Ok(下标)` / `Err(NotMapped)`；position 语义 = 首个 start+len 双等且
/// kind 收者。
pub fn find_exact_at<V: SlotView + ?Sized>(
    regions: &V,
    vaddr: usize,
    len: usize,
) -> Result<usize, SlotError> {
    let mut i = 0;
    while i < regions.slot_len() {
        let r = regions.slot_get(i);
        if r.start == vaddr && r.len == len && matches!(r.kind, SlotKind::Mapped | SlotKind::Demand) {
            return Ok(i);
        }
        i += 1;
    }
    Err(SlotError::NotMapped)
}

/// 精确匹配查找（Mapped-only——demand 区间 protect v1 拒）。
pub fn find_mapped_at<V: SlotView + ?Sized>(
    regions: &V,
    vaddr: usize,
    len: usize,
) -> Result<usize, SlotError> {
    let mut i = 0;
    while i < regions.slot_len() {
        let r = regions.slot_get(i);
        if r.start == vaddr && r.len == len && matches!(r.kind, SlotKind::Mapped) {
            return Ok(i);
        }
        i += 1;
    }
    Err(SlotError::NotMapped)
}

/// fault 探针命中律：`va` 落入某 Demand 区间（闭开域）→ 该区间下标。
pub fn demand_hit_at<V: SlotView + ?Sized>(regions: &V, va: usize) -> Option<usize> {
    let mut i = 0;
    while i < regions.slot_len() {
        let r = regions.slot_get(i);
        if va >= r.start && va < r.start + r.len && matches!(r.kind, SlotKind::Demand) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// kernel `RegionKind` → 纯投影（消费点装配——`Mapped(_)/Demand(_)` 带
/// 参形折 `Mapped`/`Demand`，`Reserved` 直投）。
pub fn snapshot_kind_of(kind: SlotKind) -> SlotKind {
    kind
}
