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
/// Copy/Clone：纯 u32/u64 数据镜像（host 钉板值语义构造用）。
#[repr(C)]
#[derive(Clone, Copy)]
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

    // ===== W3-Q43 扩钉板：全类型表 / 窗缘矩阵 / 敌意梯度 / 整图拒 =====

    /// pass1 短路判序复刻（kernel/src/mm.rs 两遍走图的 pass 1——任一 Bad
    /// 即整图拒绝降级，等价 `any` 短路；此处钉板判序语义，物理臂留内核）。
    fn scan_all_ok(descs: &[EfiMemdesc]) -> bool {
        !descs.iter().any(|d| matches!(desc_range(d, RAM_BASE, RAM_TOP), DescDecision::Bad))
    }

    #[test]
    fn desc_full_type_table_whitelist_is_only_conventional() {
        // EFI 类型词表全域 0..=24（spec 最广到 PalCode=22，留两格余量）+
        // 三个越界敌意值：窗内唯一 Skip = kind 7，其余全 Reserve——
        // v1 白名单就这一个成员， widening 须过本表才会红。
        for kind in 0u32..=24 {
            let d = desc(kind, RAM_BASE + 0x1000, 2);
            let want = if kind == EFI_CONVENTIONAL {
                DescDecision::Skip
            } else {
                DescDecision::Reserve(RAM_BASE + 0x1000, RAM_BASE + 0x3000)
            };
            assert_eq!(desc_range(&d, RAM_BASE, RAM_TOP), want, "kind={kind}");
        }
        for kind in [0x8000_0000u32, 0xFFFF_FFFE, u32::MAX] {
            let d = desc(kind, RAM_BASE + 0x1000, 2);
            assert_eq!(
                desc_range(&d, RAM_BASE, RAM_TOP),
                DescDecision::Reserve(RAM_BASE + 0x1000, RAM_BASE + 0x3000),
                "越界 kind={kind} 保守 Reserve"
            );
        }
    }

    #[test]
    fn desc_window_edge_matrix() {
        // 恰触窗基（首页起点 == ram_base）→ 窗内正常 Reserve。
        assert_eq!(
            desc_range(&desc(3, RAM_BASE, 2), RAM_BASE, RAM_TOP),
            DescDecision::Reserve(RAM_BASE, RAM_BASE + 0x2000)
        );
        // 恰触窗顶（end == ram_top，开区间不含）→ 全窗内 Reserve。
        let top_pages = (RAM_TOP - RAM_BASE - 0x1000) / 4096;
        assert_eq!(
            desc_range(&desc(3, RAM_TOP - 0x1000, 1), RAM_BASE, RAM_TOP),
            DescDecision::Reserve(RAM_TOP - 0x1000, RAM_TOP)
        );
        // end 恰 == ram_top + 1：clamp 到窗顶，不多保一字节。
        assert_eq!(
            desc_range(&desc(3, RAM_TOP - 0x1000, 2), RAM_BASE, RAM_TOP),
            DescDecision::Reserve(RAM_TOP - 0x1000, RAM_TOP),
            "top_pages sanity = {top_pages}"
        );
        // start 恰 == ram_top：零交 Skip（窗外一侧）。
        assert_eq!(desc_range(&desc(3, RAM_TOP, 1), RAM_BASE, RAM_TOP), DescDecision::Skip);
        // 零长描述符（npages=0）：len=0 → end==start → lo==hi 零交 Skip，
        // 敌意图零长段不当 Bad（无回卷面）也不当占用。
        assert_eq!(desc_range(&desc(3, RAM_BASE + 0x2000, 0), RAM_BASE, RAM_TOP), DescDecision::Skip);
        // 单字节交叠：end = ram_base + 1 → [lo, hi) = 1 字节 Reserve 原样
        // 透传（页粒度取整归 frame::clip_to_window 的 floor/ceil 律，本面
        // 只做窗口判定不做取整）。
        assert_eq!(
            desc_range(&desc(3, RAM_BASE - 0x1000 + 1, 1), RAM_BASE, RAM_TOP),
            DescDecision::Reserve(RAM_BASE, RAM_BASE + 1)
        );
        // 敌意未对齐 start（EFI 契约 4K 对齐，坏图不保证）：数学直算不炸，
        // clamp 语义 = lo 取 start 原值（不向下取整——帧源窗裁剪另归
        // frame::clip_to_window 的 floor 律，本面只做窗口判定）。
        assert_eq!(
            desc_range(&desc(3, RAM_BASE + 0x1800, 2), RAM_BASE, RAM_TOP),
            DescDecision::Reserve(RAM_BASE + 0x1800, RAM_BASE + 0x3800)
        );
    }

    #[test]
    fn desc_hostile_npages_gradation() {
        // 恰不回卷的极大 npages（len == u64::MAX - start 附近）：
        // npages_max = (u64::MAX - start) / 4096 → start + len ≤ u64::MAX。
        let start = RAM_BASE; // 0x4000_0000
        let npages_max = (u64::MAX - start) / 4096;
        // len 恒 < u64::MAX 不回卷 → 进 clamp：end = start+len ≥ ram_top
        // → hi=ram_top；但 end.min(ram_top) 前先要 checked_add 过。
        let d = desc(3, start, npages_max);
        assert_eq!(
            desc_range(&d, RAM_BASE, RAM_TOP),
            DescDecision::Reserve(start, RAM_TOP),
            "恰不回卷巨窗 clamp 全窗"
        );
        // +1 页 → len 回卷（checked_mul 恰界）或 end 回卷 → Bad。
        let d2 = desc(3, start, npages_max + 1);
        assert_eq!(desc_range(&d2, RAM_BASE, RAM_TOP), DescDecision::Bad);
        // start=0 全零基址 + 巨 npages：len = npages*4096 恰 u64::MAX 内最大
        // 4096 倍数（u64::MAX/4096*4096 = 2^64-4096），checked 过 → clamp 全窗。
        let d3 = desc(3, 0, u64::MAX / 4096);
        assert_eq!(desc_range(&d3, RAM_BASE, RAM_TOP), DescDecision::Reserve(RAM_BASE, RAM_TOP));
        // npages = u64::MAX/4096 + 1 → checked_mul 回卷 → Bad（ktest 同值域对偶）。
        let d4 = desc(3, 0, u64::MAX / 4096 + 1);
        assert_eq!(desc_range(&d4, RAM_BASE, RAM_TOP), DescDecision::Bad);
        // 高位 start 贴 u64::MAX：+1 页即 end 回卷。
        assert_eq!(desc_range(&desc(3, u64::MAX - 4096, 1), RAM_BASE, RAM_TOP), DescDecision::Skip);
        assert_eq!(desc_range(&desc(3, u64::MAX - 4096, 2), RAM_BASE, RAM_TOP), DescDecision::Bad);
    }

    #[test]
    fn map_scan_any_bad_rejects_whole_map() {
        // 整图坏描述符拒（pass1 短路判序钉板）：5 枚好图全过；
        // Bad 在首/中/末三位任一位置 → 整图拒（ reserve 零副作用的前置）。
        let good = |start: u64, npages: u64| desc(3, start, npages);
        let bad = || desc(7, 0, u64::MAX / 4096 + 1); // Conventional 也救不了回卷
        let base_good: [EfiMemdesc; 5] = [
            good(RAM_BASE + 0x10000, 4),
            good(RAM_BASE + 0x30000, 2),
            good(RAM_BASE + 0x50000, 8),
            good(RAM_BASE + 0x70000, 1),
            good(RAM_BASE + 0x90000, 16),
        ];
        assert!(scan_all_ok(&base_good), "全好图不拒");
        // 首位 Bad：哪怕其余全 Conventional 可分配。
        let mut m = base_good;
        m[0] = bad();
        assert!(!scan_all_ok(&m), "首位 Bad 整图拒");
        // 中位 Bad（kernel pass1 break 早退点——短路后不再看末位）。
        let mut m2 = base_good;
        m2[2] = bad();
        assert!(!scan_all_ok(&m2), "中位 Bad 整图拒");
        // 末位 Bad：判序须走完全图才见（无早退误判）。
        let mut m3 = base_good;
        m3[4] = bad();
        assert!(!scan_all_ok(&m3), "末位 Bad 整图拒");
        // 窗外段（MMIO Skip）与 Reserve 段混排不触发整图拒——只有 Bad 触发。
        let mut m4 = base_good;
        m4[1] = desc(11, 0x0900_0000, 1);
        assert!(scan_all_ok(&m4), "Skip/Reserve 混排合法");
        // 单枚图退化形：恰一枚 Bad 即拒 / 恰一枚好即过。
        assert!(!scan_all_ok(&[bad()]));
        assert!(scan_all_ok(&[good(RAM_BASE, 1)]));
    }

    #[test]
    fn map_ok_desc_size_edges() {
        let pa = RAM_BASE + 0x1000;
        // 步长恰 40（镜像头最小完整）+ size 恰一步 = 单描述符图。
        assert_eq!(map_ok(pa, 40, 40, 1, RAM_BASE, RAM_TOP), Some(1));
        // 步长 41（奇数步长 spec 合法——尾部附加 1B）：非 40 倍数整除才算数。
        assert_eq!(map_ok(pa, 41, 41, 1, RAM_BASE, RAM_TOP), Some(1));
        assert_eq!(map_ok(pa, 82, 41, 1, RAM_BASE, RAM_TOP), Some(2));
        // 步长 u32::MAX 敌意：size==dsize 整除过，但 checked_add 大概率窗外拒
        // ——语义 = 不 panic、按窗判拒。
        assert_eq!(map_ok(pa, u32::MAX, u32::MAX, 1, RAM_BASE, RAM_TOP), None);
        // 步长 > size 的非整除（size=40 < dsize=48 但 dsize≥40 合法域）→
        // size % dsize = 40 ≠ 0 → 拒（缓冲含半个描述符）。
        assert_eq!(map_ok(pa, 40, 48, 1, RAM_BASE, RAM_TOP), None);
        // 零步长敌意值：`desc_size < 40` 门在 `%` 取模**前**拒——
        // dsize==0 恒先撞步长门 → 拒而非 panic（u32 % 0 是 UB 面，判序
        // 先后就是安全性；门序钉板：版本门 → 步长门 → 整除门）。
        assert_eq!(map_ok(pa, 40, 0, 1, RAM_BASE, RAM_TOP), None);
    }

    #[test]
    fn map_ok_window_edges() {
        let sz = 80u32; // 两描述符
        // pa 恰窗基（ram_base 自身 4K 对齐 → 8 对齐过）：图贴 RAM 底合法。
        assert_eq!(map_ok(RAM_BASE, sz, 40, 1, RAM_BASE, RAM_TOP), Some(2));
        // end 恰窗顶（pa + size == ram_top，含端点）→ 过。
        assert_eq!(map_ok(RAM_TOP - sz as u64, sz, 40, 1, RAM_BASE, RAM_TOP), Some(2));
        // end 窗顶 + 8 → 拒（窗外图不可高半直读）。
        assert_eq!(map_ok(RAM_TOP - sz as u64 + 8, sz, 40, 1, RAM_BASE, RAM_TOP), None);
        // pa 低于窗基的 8 对齐地址（QEMU MMIO 带）→ 拒。
        assert_eq!(map_ok(0x0900_0000, sz, 40, 1, RAM_BASE, RAM_TOP), None);
        // 敌意巨窗 ram_top=u64::MAX：end 不回卷即过——语义 = 域检查不炸、
        // 窗宽是调用方几何哨兵（kernel 侧 assert dram_top ≤ HAL 窗）的职责。
        assert_eq!(map_ok(RAM_BASE, sz, 40, 1, 0, u64::MAX), Some(2));
        // ram_base > ram_top 的倒置窗：pa < ram_base 恒真 → 拒，不 panic。
        assert_eq!(map_ok(RAM_BASE, sz, 40, 1, RAM_TOP, RAM_BASE), None);
    }

    #[test]
    fn map_ok_version_gate_is_exactly_one() {
        let pa = RAM_BASE + 0x1000;
        // 版本门「协议不认识不猜」：全域唯 1 过，0/2/3..=8 与 u32 极值全拒。
        for v in [0u32, 1, 2, 3, 4, 5, 8, 16, 0x7FFF_FFFF, 0x8000_0000, u32::MAX] {
            let want = if v == EFI_DESC_VERSION { Some(1) } else { None };
            assert_eq!(map_ok(pa, 40, 40, v, RAM_BASE, RAM_TOP), want, "version={v}");
        }
    }
}
