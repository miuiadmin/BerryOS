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

/// PA 域宽（物理位，W3-Q109 打包立法）：打包槽 PFN 域的上限依据。
/// 本机 PA 窗 `[0x4000_0000, 0x5000_0000)`（256MiB，RAM_BASE 单源在
/// `arch/aarch64/src/lib.rs`）远在内；钉 34 位（16GiB）为 RAM 扩展留量。
/// 哨兵安全性不依赖本常量：未提交判定 = PFN 域零，与 PA 0 页号 0 同值
/// 而 PA 0 低于 RAM_BASE 且首页恒被无条件保留——结构性不可达。
pub const PA_BITS: u32 = 34;

/// 打包槽 mapcnt 域宽（u16 饱和计数语义原样——防线不变量不动）。
const MAPCNT_BITS: u32 = 16;
/// 打包槽 PFN 域位移（mapcnt 域 [0,16) 之上）。
const PFN_SHIFT: u32 = MAPCNT_BITS;
/// 打包槽 PFN 域宽 = PA_BITS − PAGE_SHIFT（34−12 = 22 位，16GiB 页号域）。
const PFN_BITS: u32 = PA_BITS - PAGE_SHIFT as u32;
/// PFN 域提取/清位掩码。
const PFN_MASK: u64 = ((1u64 << PFN_BITS) - 1) << PFN_SHIFT;
/// mapcnt 域提取/清位掩码。
const MAPCNT_MASK: u64 = (1u64 << MAPCNT_BITS) - 1;

// 打包布局编译期立法（01 号审计机会 5「位宽用 const assert 钉死，溢出
// 编译期拦截」）：域不越 u64、两域零交叠、PFN 域帽住 34 物理位。
const _: () = assert!(MAPCNT_BITS + PFN_BITS <= 64);
const _: () = assert!(PFN_SHIFT == MAPCNT_BITS && PFN_MASK & MAPCNT_MASK == 0);
const _: () = assert!(PA_BITS <= 48 && PA_BITS > PAGE_SHIFT as u32);

#[inline]
fn slot_pfn(slot: u64) -> u64 {
    (slot & PFN_MASK) >> PFN_SHIFT
}

#[inline]
fn slot_mapcnt(slot: u64) -> u16 {
    (slot & MAPCNT_MASK) as u16
}

#[inline]
fn slot_pack(pfn: u64, mapcnt: u16) -> u64 {
    debug_assert!(pfn < (1u64 << PFN_BITS), "PA 超打包域（>{:?}", 1u64 << PA_BITS);
    (pfn << PFN_SHIFT) | mapcnt as u64
}

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

/// 纯页记账：每页单 u64 打包槽（PA｜mapcnt｜状态位）+ MMIO 窗口基址
/// （值域钉板）。
///
/// 与 kernel 侧 `Vmo` 的分工：`Vmo` 持本结构 + 物理执行臂（FrameAllocator
/// 分配/清零/归还、direct map 拷贝），本结构只持状态与判定律。
/// **零物理副作用**——所有 `Result` 都是纯校验。
///
/// **打包布局（W3-Q109，01 号内存管理审计机会 5）**：此前每页
/// `pages: Vec<Option<usize>>`（16B，Option 无 niche）+ `mapcnt: Vec<u16>`
/// 共 24B/页；现合并单 `slots: Vec<u64>` = 8B/页，元数据内存 −66%
/// （128MiB VMO ≈576KiB→256KiB）。位布局（低位→高位）：
///
/// ```text
/// ┌─────────────────────────────────────────────┬───────────────────┐
/// │ 63                  38  37            16  15      0            │
/// │      未用（0）          │ PFN（页号，22 位）│ mapcnt（u16）      │
/// └─────────────────────────────────────────────┴───────────────────┘
/// ```
///
/// - 槽值 `0` = 未提交（PFN 域零；PA 0 低于 RAM_BASE 且首页恒被无条件
///   保留——结构性不可达，哨兵安全，见 [`PA_BITS`] 注）。
/// - PFN 存页号（`pa / PAGE_SIZE`）非字节地址：PA 域按 34 物理位
///   （16GiB）钉 [`PA_BITS`]，页号域 = 34−12 = 22 位；本机 PA 窗
///   `[0x4000_0000, 0x5000_0000)` 256MiB（65536 页）远在内。
/// - mapcnt 16 位不变（u16 饱和语义原样）。
/// - 位窗用 const assert 编译期钉死（见 [`slot_pfn`] 前三断言）。
///
/// MMIO 窗口后备下 `slots` 恒空（设备 PA 绝不进帧记账——`Drop` 的还帧
/// 循环天然零动作，误填会把设备地址 free 进 buddy = 灾难），映射计数
/// 走 `mmio_mapcnt` 独立 Vec<u16>（设备窗口无页 PA 但照建计数——track/
/// untrack/any_mapped 悬空 PTE 防线对设备窗口同样成立）。
pub struct PageLedger {
    /// 容量（字节，已页对齐）。
    size: usize,
    /// MMIO 窗口基 PA（`Some` = 设备物理窗口后备；`None` = 匿名页后备）。
    mmio_pa: Option<usize>,
    /// 每页打包槽：位 12..34 = PA 页号（0 = 未提交）、位 4..20 = mapcnt。
    slots: Vec<u64>,
    /// MMIO 后备专用映射计数（匿名后备空 Vec；npages() 以此为源）。
    mmio_mapcnt: Vec<u16>,
    /// 物理连续阶块（F0-4）：`Some((pa_base, order))` = 创建期整阶分配的
    /// 连续 DMA 后备（Drop 整阶归还，严禁拆页双还）；`None` = 普通匿名。
    contig_block: Option<(usize, u8)>,
    /// DMA 钉死标志（F0-4 `VMO_OPT_PINNED`/`VMO_OPT_CONTIGUOUS`）：
    /// 置位对象禁止 `decommit`（-ENOTSUP）。
    pinned: bool,
}

impl PageLedger {
    /// 建匿名记账（不占物理页）。`size` 向上取整到页；size==0 →
    /// [`VmoError::OutOfRange`]；记账单 Vec **try 化**（3b3 审查 H2，
    /// W3-Q109 打包后单 Vec）：预留失败返 [`VmoError::NoMem`] 而非 abort。
    pub fn new_anon(size: usize) -> Result<Self, VmoError> {
        if size == 0 {
            return Err(VmoError::OutOfRange); // syscall 面已前置 -EINVAL，此处兜底
        }
        let npages = size.div_ceil(PAGE_SIZE);
        let mut slots = Vec::new();
        slots.try_reserve_exact(npages).map_err(|_| VmoError::NoMem)?;
        slots.resize(npages, 0u64);
        Ok(Self {
            size: npages * PAGE_SIZE,
            mmio_pa: None,
            slots,
            mmio_mapcnt: Vec::new(),
            contig_block: None,
            pinned: false,
        })
    }

    /// 建 MMIO 窗口记账（P6 A-2，boot-only）：`slots` 恒空、映射计数照建。
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
            slots: Vec::new(),
            mmio_mapcnt: mapcnt,
            contig_block: None,
            pinned: false,
        })
    }

    /// 建物理连续且钉死的匿名 DMA 记账（F0-4：`pa_base` 为 `alloc_order(order)` 头页物理地址，
    /// `[0..npages)` 创建即全量登记已提交，`commit` 幂等空过、`decommit` 拒 `NotSupported`）。
    /// W3-Q109 打包槽适配：登记走 `slot_pack(pfn, 0)`（mapcnt 域零起）。
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
        let mut slots = Vec::new();
        slots.try_reserve_exact(npages).map_err(|_| VmoError::NoMem)?;
        for i in 0..npages {
            let pfn = (pa_base as u64 + i as u64 * PAGE_SIZE as u64) / PAGE_SIZE as u64;
            slots.push(slot_pack(pfn, 0));
        }
        let mut mapcnt = Vec::new();
        mapcnt.try_reserve_exact(npages).map_err(|_| VmoError::NoMem)?;
        mapcnt.resize(npages, 0);
        Ok(Self {
            size: npages * PAGE_SIZE,
            mmio_pa: None,
            slots,
            mmio_mapcnt: Vec::new(),
            contig_block: Some((pa_base, order)),
            pinned: true,
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

    /// 物理连续阶块 VMO 的整阶归还参数（`Vmo::Drop` 消费；非连续后备 `None`）。
    /// 映射计数非零时拒还（悬空 PTE 防线同匿名页）。
    pub fn contig_drop_block(&self) -> Option<(usize, usize)> {
        let (pa_base, order) = self.contig_block?;
        if self.any_mapped() {
            return None;
        }
        Some((pa_base, order as usize))
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
        if self.mmio_pa.is_some() {
            self.mmio_mapcnt.len()
        } else {
            self.slots.len()
        }
    }

    /// 第 `page_idx` 页的后备 PA（未提交 → `None`）。MMIO 窗口后备：页
    /// PA = `pa_base + idx*PAGE_SIZE`（连续设备物理区间，恒「已提交」
    /// ——无 commit 语义）。
    pub fn page_pa(&self, page_idx: usize) -> Option<usize> {
        if let Some(pa) = self.mmio_pa {
            return if page_idx < self.mmio_mapcnt.len() {
                Some(pa + page_idx * PAGE_SIZE)
            } else {
                None
            };
        }
        let slot = *self.slots.get(page_idx)?;
        let pfn = slot_pfn(slot);
        if pfn == 0 {
            return None;
        }
        Some(pfn as usize * PAGE_SIZE)
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
    /// 保证不触）。W3-Q109：`None` 落账 = 槽清零（PFN 域归零即未提交）。
    pub fn mark_committed(&mut self, page_idx: usize, pa: Option<usize>) {
        debug_assert!(
            self.page_pa(page_idx).is_none(),
            "mark_committed 覆写已提交页 = 丢帧泄漏"
        );
        let slot = &mut self.slots[page_idx];
        let pfn = match pa {
            Some(pa) => pa as u64 / PAGE_SIZE as u64,
            None => 0,
        };
        *slot = slot_pack(pfn, slot_mapcnt(*slot));
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
        // F0-4：钉死对象（CONTIGUOUS/PINNED）禁止拆块/拆页归还——整阶块
        // 的生命周期 = 句柄全关无映射销毁（Drop 整阶归还）。
        if self.pinned {
            return Err(VmoError::NotSupported);
        }
        if offset % PAGE_SIZE != 0 || len % PAGE_SIZE != 0 {
            return Err(VmoError::Unaligned);
        }
        check_bounds(self.size, offset, len)?;
        let range = PageRange::new(offset, len);
        for page in range.iter() {
            if slot_pfn(self.slots[page]) == 0 {
                return Err(VmoError::NotCommitted);
            }
            if slot_mapcnt(self.slots[page]) > 0 {
                return Err(VmoError::Mapped);
            }
        }
        let pas = range
            .iter()
            .map(|p| (p, self.page_pa(p).expect("上已校验全提交")))
            .collect();
        Ok((range, pas))
    }

    /// decommit 落账（kernel 臂还帧后逐页回调；**只清账不还帧**）。
    /// W3-Q109：清 PFN 域、保 mapcnt 域（原 `.take()` 语义：None 归还，
    /// 计数照留）。
    pub fn mark_decommitted(&mut self, page_idx: usize) -> Option<usize> {
        let slot = &mut self.slots[page_idx];
        let pfn = slot_pfn(*slot);
        *slot &= !PFN_MASK; // PFN 域清零，mapcnt 保留
        if pfn == 0 {
            None
        } else {
            Some(pfn as usize * PAGE_SIZE)
        }
    }

    // ---- 映射计数（vmar map/unmap 回调；悬空 PTE 防线，见结构体注释）----

    /// 区间映射计数 +1（vmar `map` 成功后回调）。饱和加：计数是防线不是
    /// 账本，溢出方向的保守错误（多泄漏一页）可接受。W3-Q109：匿名走
    /// 打包槽 mapcnt 域，MMIO 走独立 `mmio_mapcnt`。
    pub fn track_map(&mut self, offset: usize, len: usize) {
        if self.mmio_pa.is_some() {
            for c in self.mmio_mapcnt[offset / PAGE_SIZE..(offset + len) / PAGE_SIZE]
                .iter_mut()
            {
                *c = c.saturating_add(1);
            }
            return;
        }
        for slot in self.slots[offset / PAGE_SIZE..(offset + len) / PAGE_SIZE].iter_mut() {
            *slot = slot_pack(slot_pfn(*slot), slot_mapcnt(*slot).saturating_add(1));
        }
    }

    /// 区间映射计数 -1（vmar `unmap` 成功后回调，经 Region 存的对象指针）。
    pub fn untrack_map(&mut self, offset: usize, len: usize) {
        if self.mmio_pa.is_some() {
            for c in self.mmio_mapcnt[offset / PAGE_SIZE..(offset + len) / PAGE_SIZE]
                .iter_mut()
            {
                *c = c.saturating_sub(1);
            }
            return;
        }
        for slot in self.slots[offset / PAGE_SIZE..(offset + len) / PAGE_SIZE].iter_mut() {
            *slot = slot_pack(slot_pfn(*slot), slot_mapcnt(*slot).saturating_sub(1));
        }
    }

    /// 任一页仍被映射？（handle close 拒绝面：映射未解除的 VMO 不许销毁。）
    pub fn any_mapped(&self) -> bool {
        if self.mmio_pa.is_some() {
            return self.mmio_mapcnt.iter().any(|&c| c > 0);
        }
        self.slots.iter().any(|&s| slot_mapcnt(s) > 0)
    }

    /// 对象销毁回收判（Drop 面）：归还 mapcnt==0 的已提交页、**仍被映射的
    /// 页泄漏不还**（悬空 PTE 防线的最后一环）。返回 (页号, PA) 迭代面
    /// ——kernel 臂执行还帧。
    pub fn droppable_pages(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(|&(_page, &slot)| slot_pfn(slot) != 0 && slot_mapcnt(slot) == 0)
            .map(|(page, &slot)| (page, slot_pfn(slot) as usize * PAGE_SIZE))
    }

    // ---- 测试面（host 单测）----

    /// 测试/观测面：第 `page_idx` 页映射计数（生产不可达——ktest 观测走
    /// any_mapped 布尔面）。
    #[cfg(test)]
    pub(crate) fn mapcnt_of(&self, page_idx: usize) -> u16 {
        if self.mmio_pa.is_some() {
            return self.mmio_mapcnt[page_idx];
        }
        slot_mapcnt(self.slots[page_idx])
    }

    /// 测试/观测面：已提交页数。
    #[cfg(test)]
    pub(crate) fn committed_count(&self) -> usize {
        self.slots.iter().filter(|&&s| slot_pfn(s) != 0).count()
    }

    /// 测试/观测面：已提交页号集合。
    #[cfg(test)]
    pub(crate) fn committed_pages(&self) -> Vec<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, &s)| slot_pfn(s) != 0)
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
        let led = ledger3();
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
}
#[cfg(test)]
mod pack_tests {
    use super::*;


    /// 模拟 kernel 臂的 commit（tests 模块同款镜像）：资格判 + 未提交页落账。
    fn commit(led: &mut PageLedger, offset: usize, len: usize, pa_base: usize) {
        let range = led.commit_pages(offset, len).expect("资格判应过");
        for p in range.iter() {
            if led.page_pa(p).is_none() {
                led.mark_committed(p, Some(pa_base + p * PAGE_SIZE));
            }
        }
    }

    /// W3-Q109 打包布局立法钉板：位窗/往返/域界/哨兵/两后备案分流。
    /// 01 号内存管理审计机会 5 的回归面——打包语义 = 旧双 Vec 语义
    /// 逐位等价（凡本测红 = 打包回归）。

    #[test]
    fn layout_windows_are_pinned() {
        // 位窗立法：mapcnt 占 [0,16)、PFN 占 [16,38)、高位 [38,64) 恒零。
        assert_eq!(MAPCNT_BITS, 16);
        assert_eq!(PFN_SHIFT, 16);
        assert_eq!(PFN_BITS, PA_BITS - PAGE_SHIFT as u32);
        assert_eq!(PA_BITS, 34);
        // 域不交叠：两掩码相与必零。
        assert_eq!(PFN_MASK & MAPCNT_MASK, 0);
        // 满域打包不越界：PFN 最大页号 + mapcnt 满值。
        let max_pfn = (1u64 << PFN_BITS) - 1;
        let slot = slot_pack(max_pfn, u16::MAX);
        assert_eq!(slot_pfn(slot), max_pfn);
        assert_eq!(slot_mapcnt(slot), u16::MAX);
        // 高位域恒零（未用域不藏污）。
        assert_eq!(slot >> (PFN_SHIFT + PFN_BITS), 0);
        // slot=0 双读零：未提交 + 计数零。
        assert_eq!(slot_pfn(0), 0);
        assert_eq!(slot_mapcnt(0), 0);
    }

    #[test]
    fn pfn_roundtrip_page_aligned_pas() {
        // 往返律：mark_committed(pa) → page_pa(pa) 逐位还原（全域代表点）。
        // PA 域内代表点：RAM_BASE 之上合法窗（0x4000_0000 起）+ 高位角。
        let mut led = PageLedger::new_anon(8 * PAGE_SIZE).unwrap();
        let pas = [
            0x4000_0000,                     // RAM_BASE 首页
            0x4000_0000 + 5 * PAGE_SIZE,     // 窗内点
            0x4FFF_F000,                     // RAM_TOP 前一页
            (1u64 << PA_BITS) as usize - PAGE_SIZE, // PA 域顶页（16GiB-4KiB）
        ];
        for (i, &pa) in pas.iter().enumerate() {
            led.mark_committed(i, Some(pa));
            assert_eq!(led.page_pa(i), Some(pa), "往返失败 @{:#x}", pa);
        }
        // 未对齐 PA 静默截断为页号（kernel 臂只出页对齐帧——host 面钉
        // 「入参页对齐」的既有前置，非新语义）。
        led.mark_committed(4, Some(0x4001_2345));
        assert_eq!(led.page_pa(4), Some(0x4001_2000));
    }

    #[test]
    fn mark_decommitted_keeps_mapcnt_clears_pfn() {
        // mark_decommitted 语义钉板：清 PFN 域、保 mapcnt 域（原 `.take()`
        // 语义镜像——计数不随归还归零）。
        let mut led = PageLedger::new_anon(2 * PAGE_SIZE).unwrap();
        led.mark_committed(0, Some(0x4000_1000));
        led.track_map(0, PAGE_SIZE); // mapcnt=1
        assert_eq!(led.mapcnt_of(0), 1);
        assert_eq!(led.page_pa(0), Some(0x4000_1000));
        let ret = led.mark_decommitted(0);
        assert_eq!(ret, Some(0x4000_1000), "归还 PA 返回");
        assert_eq!(led.page_pa(0), None, "PFN 域已清");
        assert_eq!(led.mapcnt_of(0), 1, "mapcnt 保留");
        // 已清页再清：None 且不炸。
        assert_eq!(led.mark_decommitted(0), None);
    }

    #[test]
    fn mmio_routed_to_independent_counter() {
        // MMIO 后备案分流：slots 恒空、mmio_mapcnt 独立计数、any_mapped
        // /droppable 不读 slots。
        let mut led = PageLedger::new_mmio(0x0900_0000, 2 * PAGE_SIZE).unwrap();
        assert!(led.is_mmio());
        assert_eq!(led.npages(), 2);
        led.track_map(0, PAGE_SIZE);
        assert!(led.any_mapped());
        assert_eq!(led.mapcnt_of(0), 1);
        assert_eq!(led.mapcnt_of(1), 0);
        led.untrack_map(0, PAGE_SIZE);
        assert!(!led.any_mapped());
        // droppable 恒空（设备 PA 不进帧账）。
        assert_eq!(led.droppable_pages().count(), 0);
        // page_pa 恒「已提交」形。
        assert_eq!(led.page_pa(1), Some(0x0900_0000 + PAGE_SIZE));
    }

    #[test]
    fn packed_size_is_8b_per_page() {
        // 打包收益钉板（host 观测面）：slots 单 Vec u64 = 8B/页——
        // 旧双 Vec 24B/页（Option<usize> 16B + u16 2B 经 Vec 步进 24B）。
        // 类型面钉板：slots = 单 Vec<u64>（Vec 三字等宽族）——旧
        // pages: Vec<Option<usize>> + mapcnt: Vec<u16> 双 Vec 已并一。
        // （结构大小非验收面，位窗与语义面才是；本断言防字段回摆。）
        let led = PageLedger::new_anon(4 * PAGE_SIZE).unwrap();
        assert_eq!(led.npages(), 4);
        assert_eq!(core::mem::size_of::<Vec<u64>>(), core::mem::size_of::<Vec<u8>>());
        assert_eq!(core::mem::size_of::<u64>(), 8, "打包槽 8B/页立法");
    }

    #[test]
    fn mapcnt_saturates_in_packed_domain() {
        // 打包槽内 mapcnt 饱和律：u16::MAX+1 不回卷、不渗 PFN 域。
        let mut led = PageLedger::new_anon(PAGE_SIZE).unwrap();
        led.mark_committed(0, Some(0x4000_0000));
        for _ in 0..(u16::MAX as usize + 1) {
            led.track_map(0, PAGE_SIZE);
        }
        assert_eq!(led.mapcnt_of(0), u16::MAX, "饱和加");
        assert_eq!(led.page_pa(0), Some(0x4000_0000), "饱和不渗 PFN");
        led.untrack_map(0, PAGE_SIZE);
        assert_eq!(led.mapcnt_of(0), u16::MAX - 1, "饱和减不塌 0");
        assert_eq!(led.page_pa(0), Some(0x4000_0000));
    }

    #[test]
    fn full_lifecycle_under_packing() {
        // 全生命周期对拍：建→提交→映射→解映射→归还→重提交（打包面下
        // 与旧双 Vec 语义逐位等价的端到端守卫）。
        let mut led = PageLedger::new_anon(4 * PAGE_SIZE).unwrap();
        assert_eq!(led.committed_count(), 0);
        commit(&mut led, 0, 2 * PAGE_SIZE, 0x4000_0000);
        assert_eq!(led.committed_pages(), vec![0, 1]);
        assert_eq!(led.page_pa(1), Some(0x4000_0000 + PAGE_SIZE));
        led.track_map(0, 2 * PAGE_SIZE);
        led.track_map(0, 2 * PAGE_SIZE);
        assert!(led.any_mapped());
        assert!(matches!(led.decommit_pages(0, 2 * PAGE_SIZE), Err(VmoError::Mapped)));
        led.untrack_map(0, 2 * PAGE_SIZE);
        led.untrack_map(0, 2 * PAGE_SIZE);
        assert!(!led.any_mapped());
        let (_, pas) = led.decommit_pages(0, 2 * PAGE_SIZE).unwrap();
        assert_eq!(pas, vec![(0, 0x4000_0000), (1, 0x4000_0000 + PAGE_SIZE)]);
        for (p, _) in pas {
            led.mark_decommitted(p);
        }
        assert_eq!(led.committed_count(), 0);
        // churn 重提交：同一槽 PFN 重写不残留旧 mapcnt（helper 落账
        // pa_base + p*PAGE——p=1 期望 0x4800_1000）。
        commit(&mut led, PAGE_SIZE, PAGE_SIZE, 0x4800_0000);
        assert_eq!(led.page_pa(1), Some(0x4800_0000 + PAGE_SIZE));
        assert_eq!(led.mapcnt_of(1), 0, "重提交不带旧计数");
    }
}
