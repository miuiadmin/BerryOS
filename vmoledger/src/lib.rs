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
    /// F0-4：物理连续阶块记账 `Some((pa_base, order))`（`VMO_OPT_CONTIGUOUS` 由 `alloc_order`
    /// 一次弹出 `1 << order` 页并钉死直至对象销毁，禁止中途逐页 `decommit` 撕裂阶块）。
    contig_block: Option<(usize, u8)>,
    /// F0-4：DMA 钉死位（`VMO_OPT_PINNED` 或 `VMO_OPT_CONTIGUOUS` 置真时拒绝 `decommit`）。
    pinned: bool,
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
            contig_block: None,
            pinned: false,
            pages,
            mapcnt,
        })
    }

    /// 建物理连续且钉死的匿名 DMA 记账（F0-4：`pa_base` 为 `alloc_order(order)` 头页物理地址，
    /// `[0..npages)` 创建即全量登记 `Some(pa_base + i * PAGE_SIZE)`，`commit` 幂等空过、`decommit` 拒 `NotSupported`）。
    pub fn new_contiguous(size: usize, pa_base: usize, order: u8) -> Result<Self, VmoError> {
        if size == 0 {
            return Err(VmoError::OutOfRange);
        }
        if pa_base % PAGE_SIZE != 0 {
            return Err(VmoError::Unaligned);
        }
        let npages = size.div_ceil(PAGE_SIZE);
        let Some(block_pages) = 1usize.checked_shl(order as u32) else {
            return Err(VmoError::OutOfRange);
        };
        if npages > block_pages {
            return Err(VmoError::OutOfRange);
        }
        let mut pages = Vec::new();
        pages.try_reserve_exact(npages).map_err(|_| VmoError::NoMem)?;
        for i in 0..npages {
            pages.push(Some(pa_base + i * PAGE_SIZE));
        }
        let mut mapcnt = Vec::new();
        mapcnt.try_reserve_exact(npages).map_err(|_| VmoError::NoMem)?;
        mapcnt.resize(npages, 0);
        Ok(Self {
            size: npages * PAGE_SIZE,
            mmio_pa: None,
            contig_block: Some((pa_base, order)),
            pinned: true,
            pages,
            mapcnt,
        })
    }

    /// 置 DMA 钉死标志（`VMO_OPT_PINNED`）。
    pub fn set_pinned(&mut self) {
        self.pinned = true;
    }

    /// 是否钉死（`VMO_OPT_PINNED` / `VMO_OPT_CONTIGUOUS`）。
    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    /// 是否物理连续阶块后备。
    pub fn is_contiguous(&self) -> bool {
        self.contig_block.is_some()
    }

    /// MMIO 物理窗口区间 `(pa_base, size)`（供 `Vmo::Drop` 执行 VirtIO 自动复位防线）。
    pub fn mmio_pa_range(&self) -> Option<(usize, usize)> {
        self.mmio_pa.map(|pa| (pa, self.size))
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
            contig_block: None,
            pinned: true,
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
        if self.mmio_pa.is_some() || self.pinned {
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

    /// F0-4 连续阶块销毁回收判：仅在全块零残留映射（`!any_mapped()`）时返回 `Some((pa_base, order))`
    /// 供内核以整阶 `fa.free(pa_base >> PAGE_SHIFT, order)` 一次归还；若仍有残留映射则安全泄漏不拆块。
    pub fn contig_drop_block(&self) -> Option<(usize, usize)> {
        let (pa_base, order) = self.contig_block?;
        if self.any_mapped() {
            None
        } else {
            Some((pa_base, order as usize))
        }
    }

    /// 对象销毁回收判（Drop 面）：归还 mapcnt==0 的已提交页、**仍被映射的
    /// 页泄漏不还**（悬空 PTE 防线的最后一环）。返回 (页号, PA) 迭代面
    /// ——kernel 臂执行还帧。连续阶块 VMO 走 [`contig_drop_block`] 整阶归还，此处恒空防 order-0 双还。
    pub fn droppable_pages(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let is_contig = self.contig_block.is_some();
        self.pages
            .iter()
            .enumerate()
            .filter(move |&(page, backing)| {
                !is_contig && backing.is_some() && self.mapcnt[page] == 0
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

/// read/write 三道门判序（W3-Q78 单源；原 kernel/src/vmo.rs read/write
/// 两处内联逐字对齐——判序即正确性：mmio 拒先于 bounds 先于 committed，
/// 换序会把设备窗口的 `-ENOTSUP` 折成越界/未提交误码）。
///
/// 纯判定零副作用：物理拷贝由调用方在 [`walk_copy`] 推进环里执行。
pub fn copy_gate(led: &PageLedger, offset: usize, len: usize) -> Result<(), VmoError> {
    if led.is_mmio() {
        return Err(VmoError::NotSupported);
    }
    check_bounds(led.size(), offset, len)?;
    led.ensure_committed(offset, len)
}

/// read/write 分段推进执行器（W3-Q78 单源；kernel 物理拷贝闭包注入——
/// `copy(dst, src, n)` 语义 = `copy_nonoverlapping`，推进律单源：
/// `pos` 步进 = 每步页内剩余与窗尾取小，环恰在窗尾收敛（§A136 同律）。
///
/// 前置：[`copy_gate`] 已过（本函数不重查——先查后拷，不留半截副作用）。
pub fn walk_copy<F>(led: &PageLedger, offset: usize, buf_len: usize, mut copy: F)
where
    F: FnMut(usize, usize, usize),
{
    let mut pos = offset;
    while pos < offset + buf_len {
        let (pa, off, n) = led.page_span(pos, (offset + buf_len) - pos);
        copy(pa, off, n);
        pos += n;
    }
}

#[cfg(test)]
mod rw_tests {
    use super::*;

    /// 模型页账本：3 页，pa = 0x1000_0000 起连续。
    fn ledger3() -> PageLedger {
        let mut led = PageLedger::new_anon(3 * PAGE_SIZE).unwrap();
        for p in 0..3 {
            led.mark_committed(p, Some(0x1000_0000 + p * PAGE_SIZE));
        }
        led
    }

    fn mmio_ledger() -> PageLedger {
        PageLedger::new_mmio(PAGE_SIZE, PAGE_SIZE).unwrap()
    }

    #[test]
    fn gate_order_mmio_before_bounds_before_committed() {
        // 判序钉板：mmio VMO 上越界 + 未提交并存 → NotSupported（mmio 门最前）。
        assert_eq!(copy_gate(&mmio_ledger(), 0, usize::MAX), Err(VmoError::NotSupported));
        // 匿名 VMO 越界且未提交并存 → OutOfRange（bounds 先于 committed）。
        let led = PageLedger::new_anon(PAGE_SIZE).unwrap();
        assert_eq!(copy_gate(&led, 0, 2 * PAGE_SIZE), Err(VmoError::OutOfRange));
        // 全过形。
        let led = ledger3();
        assert_eq!(copy_gate(&led, 0, 3 * PAGE_SIZE), Ok(()));
    }

    #[test]
    fn gate_pass_through_each_law() {
        let mut led = ledger3();
        // 零长窗在尾界外一点 → OutOfRange（零长不豁免界检）。
        assert_eq!(copy_gate(&led, 3 * PAGE_SIZE + 1, 0), Err(VmoError::OutOfRange));
        // 溢出卷回拒。
        assert_eq!(copy_gate(&led, usize::MAX, 1), Err(VmoError::OutOfRange));
        // 未提交页拒（中页挖空）。
        let mut led = PageLedger::new_anon(3 * PAGE_SIZE).unwrap();
        led.mark_committed(0, Some(0x1000));
        led.mark_committed(2, Some(0x3000));
        assert_eq!(copy_gate(&led, 0, 3 * PAGE_SIZE), Err(VmoError::NotCommitted));
        // 已提交页零长窗尾界 → Ok。
        assert_eq!(copy_gate(&led, 3 * PAGE_SIZE, 0), Ok(()));
    }

    #[test]
    fn walk_segments_page_aligned_and_contiguous() {
        let led = ledger3();
        let mut segs = Vec::new();
        walk_copy(&led, 0, 3 * PAGE_SIZE, |pa, off, n| segs.push((pa, off, n)));
        // 三整页段、页内偏移 0、恰页大。
        assert_eq!(segs, vec![
            (0x1000_0000, 0, PAGE_SIZE),
            (0x1000_0000 + PAGE_SIZE, 0, PAGE_SIZE),
            (0x1000_0000 + 2 * PAGE_SIZE, 0, PAGE_SIZE),
        ]);
        // 覆盖律：段和恰 = buf_len。
        assert_eq!(segs.iter().map(|s| s.2).sum::<usize>(), 3 * PAGE_SIZE);
    }

    #[test]
    fn walk_unaligned_head_tail_split() {
        let led = ledger3();
        let mut segs = Vec::new();
        // 头 100 字节偏移、跨三页窗：首段 = 页内剩余、中段整页、尾段恰残量。
        walk_copy(&led, 100, 2 * PAGE_SIZE + 55, |pa, off, n| segs.push((pa, off, n)));
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], (0x1000_0000, 100, PAGE_SIZE - 100));
        assert_eq!(segs[1], (0x1000_0000 + PAGE_SIZE, 0, PAGE_SIZE));
        assert_eq!(segs[2], (0x1000_0000 + 2 * PAGE_SIZE, 0, 155));
        assert_eq!(segs.iter().map(|s| s.2).sum::<usize>(), 2 * PAGE_SIZE + 55);
    }

    #[test]
    fn walk_zero_len_no_calls() {
        let led = ledger3();
        let mut calls = 0;
        walk_copy(&led, 0, 0, |_, _, _| calls += 1);
        assert_eq!(calls, 0);
        // 尾界零长同样零调用（门已过，环体不进）。
        walk_copy(&led, 3 * PAGE_SIZE, 0, |_, _, _| calls += 1);
        assert_eq!(calls, 0);
    }

    #[test]
    fn walk_single_byte_and_full_span() {
        let led = ledger3();
        let mut segs = Vec::new();
        walk_copy(&led, 0, 1, |pa, off, n| segs.push((pa, off, n)));
        assert_eq!(segs, vec![(0x1000_0000, 0, 1)]);
        // 全幅单页窗（页内偏移不落 0）。
        let mut segs = Vec::new();
        walk_copy(&led, 4095, 1, |pa, off, n| segs.push((pa, off, n)));
        assert_eq!(segs, vec![(0x1000_0000, 4095, 1)]);
        // 末页末字节。
        let mut segs = Vec::new();
        walk_copy(&led, 3 * PAGE_SIZE - 1, 1, |pa, off, n| segs.push((pa, off, n)));
        assert_eq!(segs, vec![(0x1000_0000 + 2 * PAGE_SIZE, PAGE_SIZE - 1, 1)]);
    }

    #[test]
    fn gate_law_mirror_kernel_read_write() {
        // 对拍律：copy_gate 判序 == kernel read/write 原内联三行（注释锚——
        // kernel 侧换单源后本测即文档化契约，行为零漂移由 test_host 全量守）。
        let led = ledger3();
        // kernel read(0, 3*PAGE_SIZE) 形：gate 过 + walk 全幅。
        assert!(copy_gate(&led, 0, 3 * PAGE_SIZE).is_ok());
        // mmio read 恒 NotSupported——不论窗口形状。
        for (off, len) in [(0, 0), (0, 1), (PAGE_SIZE, 0), (0, usize::MAX)] {
            assert_eq!(copy_gate(&mmio_ledger(), off, len), Err(VmoError::NotSupported),
                       "mmio gate ({off},{len})");
        }
    }

    #[test]
    fn walk_pos_never_exceeds_end_invariant() {
        // 推进不变量：敌意 buf_len 全域扫描（1..=2 页 + 1），段和恰等、
        // 末段不越窗尾——环收敛律（§A136）。
        let led = ledger3();
        for len in 1..=(2 * PAGE_SIZE + 1) {
            let mut total = 0;
            let mut last_end = 0;
            walk_copy(&led, 0, len, |pa, off, n| {
                assert!(n > 0 && n <= PAGE_SIZE, "n 域违律 ({len}): {n}");
                assert!(off + n <= PAGE_SIZE, "页内越界 ({len}): off={off} n={n}");
                assert!(pa >= 0x1000_0000 && pa < 0x1000_0003_0000, "pa 出域 ({len}): {pa:#x}");
                last_end = total + n;
                total += n;
            });
            assert_eq!(total, len, "段和恰等 ({len})");
            assert_eq!(last_end, len, "末段恰收窗尾 ({len})");
        }
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

    #[test]
    fn contiguous_and_pinned_ledger_laws() {
        // 5 页（20KiB = vnet DMA 几何）配 order=3（8 页阶块）：创建即全提交且物理连续，
        // commit 幂等放行，decommit 拒 NotSupported，droppable_pages 为空防 order-0 误还，
        // 未映射时 contig_drop_block 返回整块 (pa_base, 3)，映射中返回 None 守悬空 PTE 防线。
        let base_pa = 0x4200_0000;
        let mut c = PageLedger::new_contiguous(5 * KIB4, base_pa, 3).unwrap();
        assert!(c.is_contiguous() && c.is_pinned());
        assert_eq!(c.size(), 5 * KIB4);
        for i in 0..5 {
            assert_eq!(c.page_pa(i), Some(base_pa + i * KIB4));
        }
        assert_eq!(c.page_pa(5), None);
        // 幂等 commit 资格过（全页均已 Some，调用方零新分配）。
        assert_eq!(c.commit_pages(0, 5 * KIB4).unwrap().count(), 5);
        // decommit 一律拒 NotSupported。
        assert_eq!(c.decommit_pages(0, KIB4), Err(VmoError::NotSupported));
        assert_eq!(c.droppable_pages().count(), 0);
        assert_eq!(c.contig_drop_block(), Some((base_pa, 3)));
        // 映射一页后 contig_drop_block 拒绝拆块。
        c.track_map(KIB4, KIB4);
        assert_eq!(c.contig_drop_block(), None);
        c.untrack_map(KIB4, KIB4);
        assert_eq!(c.contig_drop_block(), Some((base_pa, 3)));

        // 非对齐基或阶块容量不足显式拒。
        assert_eq!(
            PageLedger::new_contiguous(5 * KIB4, base_pa + 0x100, 3).err(),
            Some(VmoError::Unaligned)
        );
        assert_eq!(
            PageLedger::new_contiguous(5 * KIB4, base_pa, 2).err(),
            Some(VmoError::OutOfRange)
        );

        // MMIO 区间查询供内核 VirtIO 自动复位使用。
        let m = PageLedger::new_mmio(0x0A00_0000, 4 * KIB4).unwrap();
        assert_eq!(m.mmio_pa_range(), Some((0x0A00_0000, 4 * KIB4)));
    }
}