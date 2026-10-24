//! EFI 内存图消费面（V-1b）：UEFI `GetMemoryMap` 快照 → buddy reserve 决策。
//!
//! 数据源 = BerryHandoff 的 `memmap_*` 四字段（BerryBoot EBS 前最后一份快照，
//! V-0b 刀 4 填充）。两段式纪律：
//! 1. [`map_ok`] 整图形状校验——不过则内核**整图拒绝降级**（绝不走半张图：
//!    中途弃图会把 firmware 拥有的页当空闲发出去 = 数据腐化）；
//! 2. 逐描述符 [`desc_range`]——Conventional（type 7）= 可分配跳过，其余
//!    一律 Reserve（kernel 窗/handoff 页/memmap 缓冲本就在非 Conventional 段
//!    ——它们都是 `AllocatePages`/`AllocatePool` 的产物）。
//!
//! `EfiMemdesc` 是 `bootloader/berryboot/berry-efi` 的 `EfiMemoryDescriptor`
//! 的**逐字节镜像**（双镜像纪律同 handoff / abi_gen：两侧各带同一张 const
//! assert 偏移表，改任一侧必须同步另一侧，漂移编译期炸响）。

use core::mem::{offset_of, size_of};

/// 页大小的 u64 视图（`va::PAGE_SIZE` 同值）。
const PAGE_U64: u64 = 4096;

/// EFI_MEMORY_DESCRIPTOR 镜像（40B，`#[repr(C)]` 自然布局）。
#[repr(C)]
pub struct EfiMemdesc {
    pub kind: u32,            // @0   EFI_MEMORY_TYPE
    pub _pad: u32,            // @4   对齐填充（EFI 头文件同形）
    pub physical_start: u64,  // @8
    pub virtual_start: u64,   // @16
    pub number_of_pages: u64, // @24
    pub attribute: u64,       // @32
}

// 布局执法表（与 berry-efi 侧 host 测试 memory_descriptor_layout 钉板互为对偶）。
const _: () = {
    assert!(size_of::<EfiMemdesc>() == 40);
    assert!(offset_of!(EfiMemdesc, kind) == 0);
    assert!(offset_of!(EfiMemdesc, _pad) == 4);
    assert!(offset_of!(EfiMemdesc, physical_start) == 8);
    assert!(offset_of!(EfiMemdesc, virtual_start) == 16);
    assert!(offset_of!(EfiMemdesc, number_of_pages) == 24);
    assert!(offset_of!(EfiMemdesc, attribute) == 32);
};

/// EfiConventionalMemory：唯一可分配类型（v1 白名单就这一个）。
pub const EFI_CONVENTIONAL: u32 = 7;
/// GetMemoryMap 描述符版本（EDK2 实现 = 1；不符 = 不认识的协议 → 拒图）。
pub const EFI_DESC_VERSION: u32 = 1;

/// 单描述符 → 保留决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescDecision {
    /// 跳过：Conventional 可分配，或与 RAM 窗零交（MMIO 段等）。
    Skip,
    /// 保留 `[start, end)`（已 clamp 到 RAM 窗；非 Conventional 占用）。
    Reserve(u64, u64),
    /// 畸形描述符（区间算术回卷）——**任一**描述符 Bad 则整图拒绝降级。
    Bad,
}

/// 单描述符决策：窗 clamp → Conventional 判别；算术回卷一律 Bad。
pub fn desc_range(d: &EfiMemdesc, ram_base: u64, ram_top: u64) -> DescDecision {
    // checked：敌意/损坏的 npages、start 不得回卷成巨窗。
    let Some(len) = d.number_of_pages.checked_mul(PAGE_U64) else {
        return DescDecision::Bad;
    };
    let Some(end) = d.physical_start.checked_add(len) else {
        return DescDecision::Bad;
    };
    let (lo, hi) = (d.physical_start.max(ram_base), end.min(ram_top));
    if lo >= hi {
        // 窗外零交（QEMU virt 的 MMIO 段在 0x0900_0000 一带，低于 RAM 基）。
        return DescDecision::Skip;
    }
    if d.kind == EFI_CONVENTIONAL {
        DescDecision::Skip
    } else {
        DescDecision::Reserve(lo, hi)
    }
}

/// 整图形状校验：全过 → `Some(描述符个数)`；任一不过 → `None`（降级路）。
///
/// 执法点：版本必须 = 1（协议不认识不猜）；`desc_size >= 40`（镜像头必完整，
/// spec 允许更大步长——尾部附加字段内核不读）；`size` 非零且整除步长（缓冲只
/// 含描述符）；`pa` 8 对齐 + 整图落 RAM 窗（checked 防回卷）——窗外图不可
/// 高半直读。
pub fn map_ok(
    pa: u64,
    size: u32,
    desc_size: u32,
    version: u32,
    ram_base: u64,
    ram_top: u64,
) -> Option<usize> {
    if version != EFI_DESC_VERSION {
        return None;
    }
    if desc_size < size_of::<EfiMemdesc>() as u32 {
        return None;
    }
    if size == 0 || size % desc_size != 0 {
        return None;
    }
    if pa & 7 != 0 {
        return None;
    }
    let end = pa.checked_add(size as u64)?;
    if pa < ram_base || end > ram_top {
        return None;
    }
    Some((size / desc_size) as usize)
}

/// 走图取第 `i` 个描述符（步长 = `desc_size`，只读头 40B 镜像）。
///
/// # Safety
/// 调用方须已让 `pa/size/desc_size` 过 [`map_ok`]（窗内 → 高半直映全覆盖），
/// 且 `i < size / desc_size`；boot 单线程只读窗口。
pub unsafe fn desc_at(pa: u64, desc_size: u32, i: usize) -> &'static EfiMemdesc {
    let p = pa as usize + i * desc_size as usize;
    // SAFETY: 契约由调用方保证（见上）。
    unsafe { &*(crate::va::phys_to_virt(p) as *const EfiMemdesc) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAM_BASE: u64 = 0x4000_0000;
    const RAM_TOP: u64 = 0x4800_0000;

    fn desc(kind: u32, start: u64, npages: u64) -> EfiMemdesc {
        EfiMemdesc {
            kind,
            _pad: 0,
            physical_start: start,
            virtual_start: 0,
            number_of_pages: npages,
            attribute: 0,
        }
    }

    #[test]
    fn desc_dispatch_by_kind_and_window() {
        // Conventional 窗内 → Skip（可分配白名单唯一成员）。
        assert_eq!(desc_range(&desc(7, RAM_BASE, 16), RAM_BASE, RAM_TOP), DescDecision::Skip);
        // 非 Conventional 窗内 → Reserve 原区间。
        assert_eq!(
            desc_range(&desc(4, 0x4006_6000, 8), RAM_BASE, RAM_TOP),
            DescDecision::Reserve(0x4006_6000, 0x4006_6000 + 8 * 4096)
        );
        // 窗外（MMIO 0x0900_0000 一带）非 Conventional → 零交 Skip。
        assert_eq!(
            desc_range(&desc(11, 0x0900_0000, 1), RAM_BASE, RAM_TOP),
            DescDecision::Skip
        );
        // 半交叠：低端越窗 clamp 到窗基（[0x3F00_0000, 0x4700_0000) 交 [窗]）。
        assert_eq!(
            desc_range(&desc(3, 0x3F00_0000, 0x0800_0000 / 4096), RAM_BASE, RAM_TOP),
            DescDecision::Reserve(RAM_BASE, 0x4700_0000)
        );
        // 高端越窗 clamp 到窗顶。
        assert_eq!(
            desc_range(&desc(5, 0x4700_0000, 0x2000_0000 / 4096), RAM_BASE, RAM_TOP),
            DescDecision::Reserve(0x4700_0000, RAM_TOP)
        );
    }

    #[test]
    fn desc_overflow_is_bad() {
        // npages * 4096 回卷 / start + len 回卷 → Bad（整图拒绝降级的触发面）。
        assert_eq!(
            desc_range(&desc(7, 0, u64::MAX / 4096 + 1), RAM_BASE, RAM_TOP),
            DescDecision::Bad
        );
        assert_eq!(
            desc_range(&desc(7, u64::MAX - 4096, 2), RAM_BASE, RAM_TOP),
            DescDecision::Bad
        );
    }

    #[test]
    fn map_ok_happy_and_rejects() {
        // happy：31 段 × 40B = 1240B，窗内 8 对齐。
        assert_eq!(map_ok(RAM_BASE + 0x1000, 31 * 40, 40, 1, RAM_BASE, RAM_TOP), Some(31));
        // 步长 > 40 合法（spec 允许尾部附加字段），个数按步长算。
        assert_eq!(map_ok(RAM_BASE + 0x1000, 2 * 48, 48, 1, RAM_BASE, RAM_TOP), Some(2));
        // 拒面：版本 / 步长 < 40 / 非整除 / 零长 / 不对齐 / 窗外 / 回卷。
        assert_eq!(map_ok(RAM_BASE + 0x1000, 40, 40, 2, RAM_BASE, RAM_TOP), None);
        assert_eq!(map_ok(RAM_BASE + 0x1000, 40, 36, 1, RAM_BASE, RAM_TOP), None);
        assert_eq!(map_ok(RAM_BASE + 0x1000, 41, 40, 1, RAM_BASE, RAM_TOP), None);
        assert_eq!(map_ok(RAM_BASE + 0x1000, 0, 40, 1, RAM_BASE, RAM_TOP), None);
        assert_eq!(map_ok(RAM_BASE + 0x1004, 40, 40, 1, RAM_BASE, RAM_TOP), None);
        assert_eq!(map_ok(0x0900_0000, 40, 40, 1, RAM_BASE, RAM_TOP), None);
        assert_eq!(
            map_ok(u64::MAX - 8, 40, 40, 1, RAM_BASE, RAM_TOP),
            None // checked_add 回卷
        );
    }
}
