//! berry-vmoledger —— VMO 页面生命周期簿记纯函数面（W3-Q3）。
//!
//! **动机**：匿名 VMO 的页记账状态机（`pages: Vec<Option<usize>>` +
//! `mapcnt: Vec<u16>` + `demand_mapped` 标量）此前只活在
//! `kernel/src/vmo.rs` 对象层——kernel 依赖 HAL global_asm，只
//! aarch64-unknown-none 可编，host 测不可达；ktest 只覆盖 boot 相。
//! 页面生命周期的回归面（提交/归还资格、映射计数防线、demand 钉
//! 计数、MMIO 窗口恒空记账）需要一张 host 可测的**钉板**：本 crate
//! 把记账状态机抽成纯数据结构 + 纯函数 + 表驱动测试，
//! `kernel/src/vmo.rs` 消费同一状态机（**唯一事实源，非镜像**——改
//! 这里的语义 = 改内核 VMO 行为）。
//!
//! 放置说明：`mm/` 保持 no-alloc/no-HAL 纯度（`内存管理设计.md`：
//! 页记账 `Vec` 需堆——落 kernel 对象层；berry-mm 不收 alloc 面），
//! 故本面按 berry-wqmatch/berry-iprofile 同款独立纯 crate 落位，
//! 不下沉 mm/。
//!
//! 抽离面（与 kernel/src/vmo.rs 逐字对齐）：
//! - [`VmoError`]：错误域（syscall 折状态码的单一来源）。
//! - [`PageLedger`]：纯页记账——提交/归还资格、映射计数（悬空 PTE
//!   防线）、MMIO 窗口恒空不变量。**零物理副作用**：alloc/free PA
//!   由调用方（kernel 侧 FrameAllocator）执行，本面只管「谁有资格
//!   动哪页」的判定与簿记状态迁移。
//! - [`DemandLedger`]：demand 区间钉计数（G4-2 修批 P0-1——与
//!   mapcnt 职责正交：mapcnt 起点 = 供给成功，本钉起点 = 登记即取）。
//! - [`check_bounds`]：越界 + 溢出校验（read/write/commit 共用）。
//!
//! 与冻结面的关系：syscall ABI 零触碰（纯内核内部语义）；物理帧
//! 分配器、direct map 拷贝（unsafe 面）不在此 crate——本面只钉
//! 「记账怎么走」的判定律，不钉「物理页怎么动」的机械。

#![cfg_attr(not(test), no_std)]
#![allow(clippy::result_unit_err)]

#[cfg(not(test))]
extern crate alloc;

#[cfg(not(test))]
use alloc::vec::Vec;

/// 页大小（4KiB）与页移位——与 berry-mm `va` 同值单源在此钉板
/// （纯 crate 不依赖 berry-mm，避免 kernel↔mm 之外新耦合）。
pub const PAGE_SIZE: usize = 4096;
/// log2(PAGE_SIZE)。
pub const PAGE_SHIFT: usize = 12;

/// VMO 错误（自 kernel/src/vmo.rs 单源迁此；kernel re-export 维持
/// `crate::vmo::VmoError` 引用路径零改动）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum VmoError {
    /// offset/len 越界或加法溢出。
    OutOfRange,
    /// offset/len 未页对齐（v1 整页粒度，与 Zircon 一致）。
    Unaligned,
    /// 读/写的页尚未 commit（eager VMO 面映射前须显式提交；demand VMO
    /// 走缺页供给——G4-1 起，无此错）。
    NotCommitted,
    /// 帧分配器无空闲页。
    NoFrames,
    /// 区间内仍有页被映射（decommit 前须先 unmap——归还仍被映射的页 =
    /// PA 复用后经残留 PTE 静默写坏别人，3-2 映射计数防线）。
    Mapped,
    /// 页记账 Vec 预留失败（堆 OOM）。`Vmo::create` try 化（3b3 审查 H2）：
    /// 原 `vec!` 双分配 OOM 直接 abort——EL0 经 `vmo_create` 可触发，违
    /// ABI §8.0「永不 panic」；现折 Err 由调用方映射 -ENOMEM。
    NoMem,
    /// MMIO 物理窗口 VMO 的内容面拒绝（P6 A-2）：设备窗口只能经映射访问
    /// （volatile 设备读不可经 direct map 拷贝语义中介——commit/read/write/
    /// decommit 一律拒，-ENOTSUP）。映射/unmap/映射计数照常。
    NotSupported,
}

/// 越界 + 溢出校验（read/write/commit 共用；对齐检查各自做——commit 整页、
/// read/write 字节粒度）。
pub fn check_bounds(size: usize, offset: usize, len: usize) -> Result<(), VmoError> {
    match offset.checked_add(len) {
        Some(end) if end <= size => Ok(()),
        _ => Err(VmoError::OutOfRange),
    }
}

/// 页区间索引迭代面：`[offset, offset+len)` 覆盖的页号集合（整页粒度；
/// 调用方已保证对齐与界内——见 [`PageLedger::page_range`] 的前置校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    first: usize,
    last: usize, // 含
}

impl PageRange {
    /// 由字节区间构造（**不校验**——校验在 PageLedger 入口统一做，见
    /// `pages_in`）。
    pub fn new(offset: usize, len: usize) -> Self {
        Self {
            first: offset / PAGE_SIZE,
            last: (offset + len) / PAGE_SIZE - 1,
        }
    }

    /// 页数。
    pub fn count(&self) -> usize {
        self.last - self.first + 1
    }

    /// 首页号。
    pub fn first(&self) -> usize {
        self.first
    }

    /// 末页号（含）。
    pub fn last(&self) -> usize {
        self.last
    }

    /// 页号迭代器。
    pub fn iter(&self) -> impl Iterator<Item = usize> {
        self.first..=self.last
    }
}

/// 纯页记账：每页后备 PA + 每页映射计数 + MMIO 窗口基址（值域钉板）。
///
/// 与 kernel 侧 `Vmo` 的分工：`Vmo` 持本结构 + 物理执行臂（FrameAllocator
/// 分配/清零/归还、direct map 拷贝），本结构只持状态与判定律。
/// **零物理副作用**——所有 `Result` 都是纯校验。
pub struct PageLedger {
    /// 容量（字节，已页对齐）。
    size: usize,
    /// MMIO 窗口基 PA（`Some` = 设备物理窗口后备；`None` = 匿名页后备）。
    /// 窗口后备下 `pages` **恒空**：设备 PA 绝不进帧记账——`Drop` 的还帧
    /// 循环天然零动作（误填会把设备地址 free 进 buddy = 灾难），
    /// `mapcnt` 照建（track/untrack/any_mapped 悬空 PTE 防线对设备窗口
    /// 同样成立）。
    mmio_pa: Option<usize>,
    /// 每页后备 PA；`None` = 未提交。索引 = offset / PAGE_SIZE。
    pages: Vec<Option<usize>>,
    /// 每页映射计数（vmar `map` +1 / `unmap` -1，3-2）。**悬空 PTE 防线**：
    /// 归还仍被映射的物理页 → PA 被 buddy 复用 → 残留 PTE 经旧 VA 静默
    /// 写坏新主人。本计数让 `decommit`/对象销毁拒绝（或泄漏）此类页。
    mapcnt: Vec<u16>,
}

impl PageLedger {
    /// 建匿名记账（不占物理页）。`size` 向上取整到页；size==0 →
    /// [`VmoError::OutOfRange`]；记账双 Vec **try 化**（3b3 审查 H2）：
    /// 预留失败返 [`VmoError::NoMem`] 而非 abort。
    pub fn new_anon(size: usize) -> Result<Self, VmoError> {
        if size == 0 {
            return Err(VmoError::OutOfRange); // syscall 面已前置 -EINVAL，此处兜底
        }
        let npages = size.div_ceil(PAGE_SIZE);
        let mut pages = Vec::new();
        pages.try_reserve_exact(npages).map_err(|_| VmoError::NoMem)?;
        pages.resize(npages, None);
        let mut mapcnt = Vec::new();
        mapcnt.try_reserve_exact(npages).map_err(|_| VmoError::NoMem)?;
        mapcnt.resize(npages, 0);
        Ok(Self {
            size: npages * PAGE_SIZE,
            mmio_pa: None,
            pages,
            mapcnt,
        })
    }

    /// 建 MMIO 窗口记账（P6 A-2，boot-only）：`pages` 恒空、`mapcnt` 照建。
    /// `[pa_base, +size)` 双侧页对齐执法（匿名 create 的取整是成文设计
    /// （多授零页无害），MMIO 相反——静默取整会把请求范围外的相邻设备
    /// PA 经 Device 映射漏给 EL0，能力过授不告警（比 Zircon 更严是
    /// 有意的收紧版）。
    pub fn new_mmio(pa_base: usize, size: usize) -> Result<Self, VmoError> {
        if size == 0 {
            return Err(VmoError::OutOfRange);
        }
        if pa_base % PAGE_SIZE != 0 {
            return Err(VmoError::Unaligned);
        }
        if size % PAGE_SIZE != 0 {
            return Err(VmoError::Unaligned);
        }
        let npages = size / PAGE_SIZE;
        let mut mapcnt = Vec::new();
        mapcnt.try_reserve_exact(npages).map_err(|_| VmoError::NoMem)?;
        mapcnt.resize(npages, 0);
        Ok(Self {
            size: npages * PAGE_SIZE,
            mmio_pa: Some(pa_base),
            pages: Vec::new(),
            mapcnt,
        })
    }

    /// MMIO 物理窗口后备？
    pub fn is_mmio(&self) -> bool {
        self.mmio_pa.is_some()
    }

    /// 容量（字节，已页对齐）。
    pub fn size(&self) -> usize {
        self.size
    }

    /// 页数。
    pub fn npages(&self) -> usize {
        self.mapcnt.len()
    }

    /// 第 `page_idx` 页的后备 PA（未提交 → `None`）。MMIO 窗口后备：
    /// 页 PA = `pa_base + idx*PAGE_SIZE`（连续设备物理区间，恒「已提交」
    /// ——无 commit 语义）。
    pub fn page_pa(&self, page_idx: usize) -> Option<usize> {
        if let Some(pa) = self.mmio_pa {
            return if page_idx < self.mapcnt.len() {
                Some(pa + page_idx * PAGE_SIZE)
            } else {
                None
            };
        }
        self.pages.get(page_idx).copied().flatten()
    }

    /// `pos` 所在页的 `(PA, 页内偏移, 本步最多拷贝字节)`（read/write 步进；
    /// 前置 = 该页已提交——调用方 ensure 过）。
    pub fn page_span(&self, pos: usize, remaining: usize) -> (usize, usize, usize) {
        let page = pos / PAGE_SIZE;
        let off = pos % PAGE_SIZE;
        let n = core::cmp::min(PAGE_SIZE - off, remaining);
        (self.page_pa(page).expect("前置已校验全提交"), off, n)
    }

    /// 校验 `[offset, offset+len)` 全部落已提交页（read/write 前置）。
    pub fn ensure_committed(&self, offset: usize, len: usize) -> Result<(), VmoError> {
        for page in offset / PAGE_SIZE..(offset + len).div_ceil(PAGE_SIZE) {
            if self.page_pa(page).is_none() {
                return Err(VmoError::NotCommitted);
            }
        }
        Ok(())
    }

    // ---- 提交/归还资格（先查后动——不留半截副作用）----

    /// commit 区间资格判：MMIO 拒（NotSupported）；对齐/界内校验；返回
    /// 需出页的页号集合（已提交页跳过——幂等）。**不执行任何状态迁移**
    /// ——物理出页由 kernel 臂做，落账走 [`mark_committed`]。
    pub fn commit_pages(&self, offset: usize, len: usize) -> Result<PageRange, VmoError> {
        if self.mmio_pa.is_some() {
            return Err(VmoError::NotSupported);
        }
        if offset % PAGE_SIZE != 0 || len % PAGE_SIZE != 0 {
            return Err(VmoError::Unaligned);
        }
        check_bounds(self.size, offset, len)?;
        Ok(PageRange::new(offset, len))
    }

    /// 落账一页提交（kernel 臂出页成功后回调；`Some(pa)`）或预登记
    /// （`None`——预留位语义，测试面用）。**幂等覆写防线**：已提交页
    /// 覆写 = 丢帧泄漏——debug_assert 拦（生产由 commit_pages 资格判
    /// 保证不触）。
    pub fn mark_committed(&mut self, page_idx: usize, pa: Option<usize>) {
        debug_assert!(
            self.pages[page_idx].is_none(),
            "mark_committed 覆写已提交页 = 丢帧泄漏"
        );
        self.pages[page_idx] = pa;
    }

    /// decommit 区间资格判（3-2）：MMIO 拒；对齐/界内校验；**先整段验**
    /// 「全已提交且无映射」再动（先查后动）。返回可归还页号集合与
    /// 各页 PA——kernel 臂执行还帧后调 [`mark_decommitted`]。
    pub fn decommit_pages(
        &self,
        offset: usize,
        len: usize,
    ) -> Result<(PageRange, Vec<(usize, usize)>), VmoError> {
        if self.mmio_pa.is_some() {
            return Err(VmoError::NotSupported);
        }
        if offset % PAGE_SIZE != 0 || len % PAGE_SIZE != 0 {
            return Err(VmoError::Unaligned);
        }
        check_bounds(self.size, offset, len)?;
        let range = PageRange::new(offset, len);
        for page in range.iter() {
            if self.pages[page].is_none() {
                return Err(VmoError::NotCommitted);
            }
            if self.mapcnt[page] > 0 {
                return Err(VmoError::Mapped);
            }
        }
        let pas = range
            .iter()
            .map(|p| (p, self.pages[p].expect("上已校验全提交")))
            .collect();
        Ok((range, pas))
    }

    /// decommit 落账（kernel 臂还帧后逐页回调；**只清账不还帧**）。
    pub fn mark_decommitted(&mut self, page_idx: usize) -> Option<usize> {
        self.pages[page_idx].take()
    }

    // ---- 映射计数（vmar map/unmap 回调；悬空 PTE 防线，见结构体注释）----

    /// 区间映射计数 +1（vmar `map` 成功后回调）。饱和加：计数是防线不是
    /// 账本，溢出方向的保守错误（多泄漏一页）可接受。
    pub fn track_map(&mut self, offset: usize, len: usize) {
        for c in self.mapcnt[offset / PAGE_SIZE..(offset + len) / PAGE_SIZE].iter_mut() {
            *c = c.saturating_add(1);
        }
    }

    /// 区间映射计数 -1（vmar `unmap` 成功后回调，经 Region 存的对象指针）。
    pub fn untrack_map(&mut self, offset: usize, len: usize) {
        for c in self.mapcnt[offset / PAGE_SIZE..(offset + len) / PAGE_SIZE].iter_mut() {
            *c = c.saturating_sub(1);
        }
    }

    /// 任一页仍被映射？（handle close 拒绝面：映射未解除的 VMO 不许销毁。）
    pub fn any_mapped(&self) -> bool {
        self.mapcnt.iter().any(|&c| c > 0)
    }

    /// 对象销毁回收判（Drop 面）：归还 mapcnt==0 的已提交页、**仍被映射的
    /// 页泄漏不还**（悬空 PTE 防线的最后一环）。返回 (页号, PA) 迭代面
    /// ——kernel 臂执行还帧。
    pub fn droppable_pages(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.pages
            .iter()
            .enumerate()
            .filter(|&(page, backing)| {
                backing.is_some() && self.mapcnt[page] == 0
            })
            .map(|(page, backing)| (page, backing.expect("filter 已筛 Some")))
    }

    // ---- 测试面（host 单测）----

    /// 测试/观测面：第 `page_idx` 页映射计数（生产不可达——ktest 观测走
    /// any_mapped 布尔面）。
    #[cfg(test)]
    pub(crate) fn mapcnt_of(&self, page_idx: usize) -> u16 {
        self.mapcnt[page_idx]
    }

    /// 测试/观测面：已提交页数。
    #[cfg(test)]
    pub(crate) fn committed_count(&self) -> usize {
        self.pages.iter().filter(|p| p.is_some()).count()
    }

    /// 测试/观测面：已提交页号集合。
    #[cfg(test)]
    pub(crate) fn committed_pages(&self) -> Vec<usize> {
        self.pages
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_some())
            .map(|(i, _)| i)
            .collect()
    }
}

/// demand 区间钉计数（G4-2 修批 P0-1，页数累计标量）：`vmar::map_demand`
/// 登记即 +、`unmap`/`Vmar::Drop` 收区间即 −。**与 mapcnt 职责正交**：
/// mapcnt 防「页 PA 复用经残留 PTE 写坏新主人」（供给语义、per-页，起点
/// = 供给成功 §5-⑥）；本计数防「Region 裸指针指向已释放对象」（区间
/// 语义、对象级，起点 = 登记即取）——demand 区间未供给页 mapcnt 恒 0
/// 而指针已活，须独立钉（close/replace 拒面 + drain 泄漏判据的第二个
/// 分量）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DemandLedger {
    /// 已登记 demand 区间页数（饱和维护）。
    demand_mapped: usize,
}

impl DemandLedger {
    /// demand 区间钉 +len 页（`Vmar::map_demand` 登记成功后回调——**登记即
    /// 取**，与 track_map 的「供给成功才取」差一档：未供给页指针已活）。
    /// 饱和加。
    pub fn track(&mut self, len: usize) {
        self.demand_mapped = self.demand_mapped.saturating_add(len / PAGE_SIZE);
    }

    /// demand 区间钉 -len 页（`Vmar::unmap` / `Vmar::Drop` 收 Demand 区间
    /// 回调——Region 即死，钉即放）。饱和减（防线不是账本，多放一页方向
    /// 由「Region 死后无人再读」吸收）。
    pub fn untrack(&mut self, len: usize) {
        self.demand_mapped = self.demand_mapped.saturating_sub(len / PAGE_SIZE);
    }

    /// 任一 demand 区间仍登记着本对象？（close/replace 拒面 + drain 泄漏
    /// 判据的第二分量——与 `any_mapped` 并集即「有 Region 指着它」。）
    pub fn is_mapped(&self) -> bool {
        self.demand_mapped > 0
    }

    /// 已登记页数（观测面）。
    pub fn pages(&self) -> usize {
        self.demand_mapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIB4: usize = PAGE_SIZE;
    const KIB12: usize = 3 * PAGE_SIZE;

    /// 模拟 kernel 臂的 commit（出页 + 落账）——镜像 kernel 循环：
    /// 只对未提交页出页落账（幂等语义：已提交页跳过，不重复出页）。
    fn commit(led: &mut PageLedger, offset: usize, len: usize, pa_base: usize) {
        let range = led.commit_pages(offset, len).expect("资格判应过");
        for p in range.iter() {
            if led.page_pa(p).is_none() {
                led.mark_committed(p, Some(pa_base + p * PAGE_SIZE));
            }
        }
    }

    #[test]
    fn anon_create_rounds_up_and_try_reserves() {
        // 匿名建：size 向上取整到页；size==0 拒；mapcnt 全零起始。
        let led = PageLedger::new_anon(KIB4 + 1).unwrap();
        assert_eq!(led.size(), 2 * KIB4, "1 页 + 1 字节 → 取整 2 页");
        assert_eq!(led.npages(), 2);
        assert!(!led.is_mmio());
        assert!(!led.any_mapped());
        assert!(matches!(PageLedger::new_anon(0), Err(VmoError::OutOfRange)));
    }

    #[test]
    fn mmio_create_enforces_double_alignment_and_empty_pages() {
        // MMIO 建：双侧页对齐执法（静默取整 = 能力过授不告警）；pages 恒空
        // 而 mapcnt 照建；page_pa = 窗口基 + idx*PAGE（恒「已提交」）。
        let base = 0x0900_0000;
        assert!(matches!(
            PageLedger::new_mmio(base + 0x100, 0x200),
            Err(VmoError::Unaligned)
            ), "基不对齐");
        assert!(matches!(
            PageLedger::new_mmio(base, 0x2200),
            Err(VmoError::Unaligned)
            ), "size 不对齐");
        assert!(matches!(PageLedger::new_mmio(base, 0), Err(VmoError::OutOfRange)));
        let led = PageLedger::new_mmio(base, 4 * KIB4).unwrap();
        assert!(led.is_mmio());
        assert_eq!(led.npages(), 4);
        assert_eq!(led.page_pa(0), Some(base));
        assert_eq!(led.page_pa(3), Some(base + 3 * KIB4));
        assert_eq!(led.page_pa(4), None, "越幅 None");
        // 内容面四拒：commit/decommit 资格判全 NotSupported。
        assert!(matches!(
            led.commit_pages(0, KIB4),
            Err(VmoError::NotSupported)
            ));
        let (_, _) = match led.decommit_pages(0, KIB4) {
            Err(VmoError::NotSupported) => ((), ()), // 期望
            _ => panic!("decommit 应 NotSupported"),
        };
    }

    #[test]
    fn commit_idempotent_and_bounds() {
        // commit 资格：未对齐拒；越界拒（含溢出卷回）；幂等（已提交页
        // 资格判不重复出页——kernel 臂按 pages[page].is_none() 分派，本面
        // PageRange 全量返回、mark_committed 幂等防线拦截覆写）。
        let mut led = PageLedger::new_anon(4 * KIB4).unwrap();
        assert!(matches!(
            led.commit_pages(0x800, KIB4),
            Err(VmoError::Unaligned)
        ));
        assert!(matches!(
            led.commit_pages(0, 0x800),
            Err(VmoError::Unaligned)
        ));
        assert!(matches!(
            led.commit_pages(4 * KIB4, KIB4),
            Err(VmoError::OutOfRange)
        ));
        // 溢出面：offset 对齐且极大、len 叠加后超幅——check_bounds 的
        // checked_add 兜底（对齐前提下单值 PAGE_SIZE 不卷绕，超幅即拒）。
        assert!(matches!(
            led.commit_pages(3 * KIB4, 2 * KIB4),
            Err(VmoError::OutOfRange)
        ));
        commit(&mut led, 0, 2 * KIB4, 0x1000_0000);
        assert_eq!(led.committed_pages(), vec![0, 1]);
        commit(&mut led, 0, 2 * KIB4, 0x1000_0000); // 幂等重提
        assert_eq!(led.committed_pages(), vec![0, 1], "重提不增页");
        assert_eq!(led.page_pa(2), None, "未提交页 None");
    }

    #[test]
    fn commit_unaligned_offset_boundary_exact() {
        // commit_pages(0x800+0x800, ...) 上面已测；补 offset 恰界内、
        // len 使 end == size 的边界（end == size 合法）。
        let led = PageLedger::new_anon(3 * KIB4).unwrap();
        let r = led.commit_pages(2 * KIB4, KIB4).unwrap();
        assert_eq!((r.first(), r.last()), (2, 2));
        // 匿名建 3K+1 取整到 4 页幅（成文设计：多授零页无害）——4K 恰界
        // 合法（end == size）、12K+1 未对齐先拒。
        let r = led.commit_pages(0, 3 * KIB4 + 1);
        assert!(matches!(r, Err(VmoError::Unaligned)));
        let r = led.commit_pages(0, 3 * KIB4).unwrap();
        assert_eq!((r.first(), r.last()), (0, 2), "end == size 恰界合法");
    }

    #[test]
    fn decommit_rejects_uncommitted_and_mapped() {
        // decommit 资格三拒：未提交（NotCommitted）/ 映射中（Mapped）/
        // 未对齐；先整段验——半段已提交半段未提交也整段拒（不留半截
        // 副作用）。
        let mut led = PageLedger::new_anon(4 * KIB4).unwrap();
        assert!(matches!(
            led.decommit_pages(0, 0x800),
            Err(VmoError::Unaligned)
            ));
        commit(&mut led, 0, 2 * KIB4, 0x1000_0000);
        // 半段未提交：整段拒。
        assert!(matches!(
            led.decommit_pages(KIB4, 2 * KIB4),
            Err(VmoError::NotCommitted)
            ));
        // 全段已提交但页 1 映射中：整段拒（先查后动）。
        led.track_map(KIB4, KIB4);
        assert!(matches!(led.decommit_pages(0, 2 * KIB4), Err(VmoError::Mapped)));
        // 解除后可归还；落账后 page_pa → None。
        led.untrack_map(KIB4, KIB4);
        let (range, pas) = led.decommit_pages(0, 2 * KIB4).unwrap();
        assert_eq!((range.first(), range.last(), range.count()), (0, 1, 2));
        let freed: Vec<(usize, usize)> = pas;
        assert_eq!(
            freed,
            vec![(0, 0x1000_0000), (1, 0x1000_0000 + KIB4)],
            "归还 PA 序列与登记一致"
        );
        for (p, _) in freed {
            led.mark_decommitted(p);
        }
        assert_eq!(led.committed_count(), 0);
        assert_eq!(led.page_pa(0), None);
    }

    #[test]
    fn mapcnt_track_untrack_saturating_and_any() {
        // 映射计数：区间 +1/-1 对称；u16 饱和不回卷；any_mapped 判据。
        let mut led = PageLedger::new_anon(4 * KIB4).unwrap();
        led.track_map(0, 2 * KIB4);
        assert_eq!(led.mapcnt_of(0), 1);
        assert_eq!(led.mapcnt_of(1), 1);
        assert_eq!(led.mapcnt_of(2), 0, "区间外不动");
        assert!(led.any_mapped());
        // 同区间再 map（双 AS 别名）→ 计数 2；两次 unmap 归零。
        led.track_map(0, 2 * KIB4);
        assert_eq!(led.mapcnt_of(0), 2);
        led.untrack_map(0, 2 * KIB4);
        led.untrack_map(0, 2 * KIB4);
        assert_eq!(led.mapcnt_of(0), 0);
        assert!(!led.any_mapped());
        // 饱和：u16::MAX+1 不回卷（保守多泄漏方向）。
        let mut sat = PageLedger::new_anon(KIB4).unwrap();
        for _ in 0..(u16::MAX as usize + 1) {
            sat.track_map(0, KIB4);
        }
        assert_eq!(sat.mapcnt_of(0), u16::MAX, "饱和加");
        sat.untrack_map(0, KIB4);
        assert_eq!(sat.mapcnt_of(0), u16::MAX - 1, "饱和减不塌 0");
    }

    #[test]
    fn droppable_pages_skip_mapped_and_uncommitted() {
        // Drop 面：只归还 mapcnt==0 的已提交页；映射中页泄漏不还（悬空
        // PTE 防线最后一环）；未提交页天然跳过。
        let mut led = PageLedger::new_anon(4 * KIB4).unwrap();
        commit(&mut led, 0, 3 * KIB4, 0x2000_0000);
        led.track_map(KIB4, KIB4); // 页 1 映射中
        let droppable: Vec<(usize, usize)> = led.droppable_pages().collect();
        assert_eq!(
            droppable,
            vec![(0, 0x2000_0000), (2, 0x2000_0000 + 2 * KIB4)],
            "页 1 泄漏不还；页 3 未提交跳过"
        );
    }

    #[test]
    fn page_span_and_ensure_committed() {
        // page_span：跨页步进 (PA, off, n) 三元组；ensure_committed 半段
        // 未提交拒。
        let mut led = PageLedger::new_anon(2 * KIB4).unwrap();
        commit(&mut led, 0, KIB4, 0x3000_0000);
        assert_eq!(led.page_span(0x10, 0x100), (0x3000_0000, 0x10, 0x100));
        assert_eq!(
            led.page_span(KIB4 - 0x10, 0x100),
            (0x3000_0000, KIB4 - 0x10, 0x10),
            "页尾截断到页界"
        );
        assert!(led.ensure_committed(0, KIB4).is_ok());
        assert!(matches!(
            led.ensure_committed(0, KIB4 + 1),
            Err(VmoError::NotCommitted)
            ), "半段未提交整段拒");
    }

    #[test]
    fn check_bounds_table() {
        // 表驱动：界内/恰界/越界/溢出四态。
        let cases: &[(usize, usize, usize, bool)] = &[
            (KIB12, 0, KIB12, true),        // 全幅
            (KIB12, KIB12, 0, true),        // len==0 尾界（零长区间合法）
            (KIB12, KIB12 + 1, 0, false),   // 尾界外 1 字节
            (KIB12, 0, KIB12 + 1, false),   // 超 1 字节
            (KIB12, usize::MAX, 1, false),  // 溢出卷回
            (KIB12, usize::MAX - 1, usize::MAX, false), // 双溢出
        ];
        for &(size, off, len, ok) in cases {
            assert_eq!(
                check_bounds(size, off, len).is_ok(),
                ok,
                "check_bounds({size:#x}, {off:#x}, {len:#x})"
            );
        }
    }

    #[test]
    fn demand_ledger_track_untrack_is_mapped() {
        // demand 钉计数：登记即取（track）→ is_mapped；收区间（untrack）
        // 归零；饱和两面；与 mapcnt 正交（本测试不触 PageLedger——独立
        // 结构）。
        let mut d = DemandLedger::default();
        assert!(!d.is_mapped());
        d.track(2 * KIB4);
        assert!(d.is_mapped());
        assert_eq!(d.pages(), 2);
        d.track(KIB4);
        assert_eq!(d.pages(), 3, "多区间累加");
        d.untrack(3 * KIB4);
        assert!(!d.is_mapped(), "全收归零");
        assert_eq!(d.pages(), 0);
        // 饱和减：多放方向由「Region 死后无人再读」吸收。
        let mut s = DemandLedger::default();
        s.untrack(KIB4);
        assert_eq!(s.pages(), 0, "空账 untrack 不负");
        // 大段计量：两段各 2^50 页（字节面 2^62 不溢出）——累计无卷绕。
        let mut m = DemandLedger::default();
        let big = 1usize << 50;
        m.track(big * PAGE_SIZE);
        assert_eq!(m.pages(), big);
        m.track(big * PAGE_SIZE);
        assert_eq!(m.pages(), 2 * big, "累计不回卷");
    }

    #[test]
    fn ledger_state_machine_table() {
        // 表驱动全生命周期：建 → 提交 → 映射 → （读/写资格）→ 解映射 →
        // 归还 → 空账。每步断言可观测面（committed/mapcnt/any_mapped）。
        let mut led = PageLedger::new_anon(4 * KIB4).unwrap();
        assert_eq!(led.committed_count(), 0);
        // 提交两页。
        commit(&mut led, 0, 2 * KIB4, 0x4000_0000);
        assert_eq!(led.committed_count(), 2);
        assert!(led.ensure_committed(0, 2 * KIB4).is_ok());
        // 映射两页（两 AS 别名 → 计数 2）。
        led.track_map(0, 2 * KIB4);
        led.track_map(0, 2 * KIB4);
        assert!(led.any_mapped());
        // decommit 拒（Mapped）。
        assert!(matches!(led.decommit_pages(0, 2 * KIB4), Err(VmoError::Mapped)));
        // 全解 → decommit 过 → 落账 → 空账。
        led.untrack_map(0, 2 * KIB4);
        led.untrack_map(0, 2 * KIB4);
        assert!(!led.any_mapped());
        let (_, pas) = led.decommit_pages(0, 2 * KIB4).unwrap();
        for (p, _) in pas {
            led.mark_decommitted(p);
        }
        assert_eq!(led.committed_count(), 0);
        assert!(!led.any_mapped());
        // 重提交重归还（churn 不留残账）。
        commit(&mut led, KIB4, 2 * KIB4, 0x5000_0000);
        assert_eq!(led.committed_pages(), vec![1, 2]);
        let (_, pas) = led.decommit_pages(KIB4, 2 * KIB4).unwrap();
        for (p, _) in pas {
            led.mark_decommitted(p);
        }
        assert_eq!(led.committed_count(), 0, "churn 后归零");
    }

    #[test]
    fn two_vmo_aliases_independent() {
        // 两 VMO 记账独立（对象级隔离）：A 的 track 不影响 B 的资格。
        let mut a = PageLedger::new_anon(KIB4).unwrap();
        let b = PageLedger::new_anon(KIB4).unwrap();
        commit(&mut a, 0, KIB4, 0x6000_0000);
        assert_eq!(b.page_pa(0), None, "B 未随 A 提交");
        a.track_map(0, KIB4);
        assert!(!b.any_mapped(), "A 的映射不进 B 账");
        assert!(matches!(
            b.decommit_pages(0, KIB4),
            Err(VmoError::NotCommitted)
            ));
    }

    #[test]
    fn page_range_iter_count() {
        // PageRange 值域：构造/页数/迭代序。
        let r = PageRange::new(2 * KIB4, 3 * KIB4);
        assert_eq!(r.first(), 2);
        assert_eq!(r.last(), 4);
        assert_eq!(r.count(), 3);
        assert_eq!(r.iter().collect::<Vec<_>>(), vec![2, 3, 4]);
    }
}