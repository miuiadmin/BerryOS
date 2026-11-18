//! 匿名 VMO（内存对象，`内存管理设计.md` §2；2b-3 物理页后备先行）。
//!
//! v1 范围（ADR #20）：`create/commit/read/write`——commit **立即**从帧分配器
//! 出页并清零（启动时全映射）；`decommit`（3-2）归还已提交且**未被映射**的
//! 物理页。G4-1 起 demand 先形（ADR #4/#9 激活）：[`Vmo::create_demand`] +
//! 缺页路径逐页 commit（两值页态不动，只移 commit 时机）；EL0 pager 服务与
//! 0x21 ABI 面 = G4-2。`share/CoW` 与 5 类后备全集（ADR #29）留 Phase 3+。
//!
//! 放置说明（与 ADR #29「直接落 `mm/`」的偏差，已记设计文档落地记录）：
//! VMO 是内核对象（Ziron 务实派，ADR #4），页记账 `Vec` 需堆——落 `kernel/`
//! 对象层；物理页仍走 berry-mm `FrameAllocator`，berry-mm 保持 no-alloc/no-HAL
//! 纯度（host 可测核心不因对象壳失守）。**W3-Q3 起记账状态机单源外迁**
//! `berry-vmoledger`（纯 crate，host 表驱动测试钉板）：本文件 = 物理执行臂
//! （帧分配/清零/归还、direct map 拷贝）+ 对象壳，资格判定与簿记迁移
//! [`berry_vmoledger::PageLedger`]——**唯一事实源，非镜像**。
//!
//! 操作类 API 返回 `Result`（syscall 边界无 panic 纪律——Phase 3 直接折状态码）；
//! 查询面（`is_*`/`size`/`page_pa` 等）返回裸值。

use berry_mm::frame::FrameAllocator;
use berry_mm::va::{phys_to_virt, PAGE_SHIFT, PAGE_SIZE};
use berry_mm::MAX_ORDER;
use berry_vmoledger::{DemandLedger, PageLedger};
use core::sync::atomic::{AtomicUsize, Ordering};

pub use berry_vmoledger::VmoError;

/// 匿名 VMO：`size` 字节、按页提交的物理页后备（§2 后备类①「物理页（匿名）」）。
/// P6 A-2 起第二种后备：**MMIO 物理窗口**（`mmio_pa = Some(pa_base)`）——
/// Zircon `zx_vmo_create_physical` 的收紧版（无 syscall 面，boot-only 构造
/// [`Vmo::create_mmio`]——物理窗口是 boot 授的能力，EL0 不可自造）。
pub struct Vmo {
    /// 页记账（W3-Q3 单源外迁 berry-vmoledger）：提交状态/映射计数/
    /// MMIO 窗口——资格判定与簿记全在纯 crate。
    ledger: PageLedger,
    /// demand 区间钉计数（G4-2 修批 P0-1）：与 mapcnt 职责正交（见
    /// [`berry_vmoledger::DemandLedger`] 结构注释）。
    demand_pins: DemandLedger,
    /// 按需后备（G4-1，ADR #4/#9）：`true` = 本 VMO 页由 pager 路径在缺页时
    /// 逐页 commit（对象级属性——`vmo_create` options 对象级落位的内核侧
    /// 先形，ABI 位激活留 G4-2）。两值页态（`pages`）不动：demand 只是
    /// 「commit 的触发时机」从 map 前（eager）移到缺页时，提交语义复用。
    demand: bool,
    /// **ZFOD 匿名零页标志**（P1-3）：`true` = 本 demand VMO 已被 pager 以
    /// **零臂**（0x21/0x23/0x24 的 `src=0`——供给页 = 纯清零页，无内容
    /// 后备）供给过 ≥1 页 ⇒ 后续缺页走内核直供快路径（dpg `probe_el0`：
    /// KL 窗内 `supply_page` 出新零页填叶，零 pager 往返）。
    ///
    /// **为什么由供给面学习而非创建面声明**：v1 ABI（`vmo_create` options
    /// 唯 bit0 `VMO_OPT_DEMAND`）不区分匿名/文件后备——berry-exec 的
    /// file 窗与零窗**同形创建**（`vmo_create(len, VMO_OPT_DEMAND)`），
    /// 种类区别只活在 svcmgr 用户态窗登记表（`PagerReq{dv,sv,delta}` 的
    /// sv=0 vs sv=rootfs）。内核侧唯一能观察到「此 VMO 无内容后备」的
    /// 权威时点 = pager 自己用零臂喂它的那一刻（`pager_supply_one` 的
    /// `sv.is_none()` 供给成功臂）——零臂语义 = 「供给页即清零页」
    /// （ABI §0x21 词面），与 ZFOD 直供的页内容**逐字节等价**。
    ///
    /// **设计契约（一 VMO 一种类）**：标志置位即终身（对象生存期），要求
    /// pager 对单 VMO 的供给种类恒定。现役消费面结构成立：berry-exec
    /// 一窗一 VMO（file/zero/hybrid 三色窗各自 `vmo_create`），svcmgr
    /// 登记表一 dv 一 (sv,delta)，全树无「同 VMO 先零臂后有源臂」流
    /// （dpgp/DPGS/DPGN/dpgf/bench/foot 族全部 sv=真源；零窗族恒 sv=0）。
    /// 混种类 pager（假想）若先零臂后内容臂：后续缺页被 ZFOD 直供零页
    /// 且不再武装——pager 见不到请求；此为该形态的语义收窄，非损坏
    /// （v1 无此形态，挂账见笔记）。
    ///
    /// 有源臂（`sv.is_some()`）供给**不置位**——即使供给中途源读失败
    /// （DPGN ③ `-EBADSTATE` 形：子被恒唤收零页 exit 9——该 VMO 后续
    /// 缺页仍走 pager 链，语义零漂移）。EL1 面（`dpg_pager` 内核 pager）
    /// 不经供给面（供给后写 magic）→ EL1 demand VMO 永不置位，
    /// `probe`（EL1 探针）不加快路径。
    zfod: bool,
}

impl Vmo {
    /// 建空 VMO（不占物理页）。`size` 向上取整到页。size==0 → [`VmoError::OutOfRange`]；
    /// 记账 try 化（3b3 审查 H2）见 [`PageLedger::new_anon`]。
    pub fn create(size: usize) -> Result<Self, VmoError> {
        Ok(Self {
            ledger: PageLedger::new_anon(size)?,
            demand_pins: DemandLedger::default(),
            demand: false,
            zfod: false,
        })
    }

    /// 建按需 VMO（G4-1）：同 [`Vmo::create`] 的记账面，但标记 demand——
    /// 页在缺页时由 pager 路径逐页 commit（`Vmar::supply_page`）。boot/测试
    /// 直构面（无 syscall 口——options 位激活 = G4-2 ABI 面）。
    pub fn create_demand(size: usize) -> Result<Self, VmoError> {
        let mut v = Self::create(size)?;
        v.demand = true;
        Ok(v)
    }

    /// 建钉死匿名 VMO（F0-4 `VMO_OPT_PINNED`：禁止中途 `decommit`）。
    pub fn create_pinned(size: usize) -> Result<Self, VmoError> {
        let mut v = Self::create(size)?;
        v.ledger.set_pinned();
        Ok(v)
    }

    /// 建物理连续且钉死的 DMA VMO（F0-4 `VMO_OPT_CONTIGUOUS`）：
    /// 按 `npages.next_power_of_two()` 计算伙伴阶 `order`，经 `ALLOC_LOCK` 一次弹出物理连续块，
    /// 在锁外用 `DC ZVA` 清零整块后完成全页预提交记账；`Vec` 预留失败时自动将整阶块退回伙伴分配器。
    pub fn create_contiguous_global(size: usize) -> Result<Self, VmoError> {
        if size == 0 {
            return Err(VmoError::OutOfRange);
        }
        let npages = size.div_ceil(PAGE_SIZE);
        let Some(block_pages) = npages.checked_next_power_of_two() else {
            return Err(VmoError::OutOfRange);
        };
        let order = block_pages.trailing_zeros() as usize;
        if order > MAX_ORDER {
            return Err(VmoError::NoFrames);
        }
        let pfn = crate::mm::with_frames(|fa| fa.alloc_order(order))
            .map_err(|_| VmoError::NoFrames)?;
        let pa_base = pfn << PAGE_SHIFT;
        for i in 0..block_pages {
            // SAFETY: 刚弹出的独占连续物理页块尚无任何外部别名，锁外逐页清零安全。
            unsafe { zero_page_4k(phys_to_virt(pa_base + i * PAGE_SIZE) as *mut u8) };
        }
        match PageLedger::new_contiguous(size, pa_base, order as u8) {
            Ok(ledger) => Ok(Self {
                ledger,
                demand_pins: DemandLedger::default(),
                demand: false,
                zfod: false,
            }),
            Err(e) => {
                crate::mm::with_frames(|fa| {
                    let _ = fa.free(pfn, order);
                });
                Err(e)
            }
        }
    }

    /// 对已提交匿名页区间执行 PoC 数据缓存维护（F0-4 `0x1C vmo_cache_op`）。
    pub fn cache_op(&self, clean_only: bool, offset: usize, len: usize) -> Result<(), VmoError> {
        berry_vmoledger::copy_gate(&self.ledger, offset, len)?;
        berry_vmoledger::walk_copy(&self.ledger, offset, len, |pa, off, n| {
            let kva = phys_to_virt(pa + off);
            if clean_only {
                berry_hal_aarch64::cache_poc_clean(kva, n);
            } else {
                berry_hal_aarch64::cache_poc_clean_invalidate(kva, n);
            }
        });
        Ok(())
    }

    /// 按需后备？（fault 探针分派用：demand 区间的缺页走 pager 协议。）
    pub fn is_demand(&self) -> bool {
        self.demand
    }

    /// ZFOD 匿名零页 VMO？（P1-3——`probe_el0` 快路径判定：真 = 缺页
    /// 内核直供新零页，零 pager 往返。）置位语义见结构体 `zfod` 字段注。
    pub fn is_zfod(&self) -> bool {
        self.zfod
    }

    /// 置 ZFOD 标志（P1-3——供给面学习点：0x21/0x23/0x24 **零臂**
    /// （`src=0`）供给成功时由 `pager_supply_one` 调用；KL 窗内串行，
    /// 与探针读点同锁域互斥）。幂等（重复置位无操作）。
    pub(crate) fn mark_zfod(&mut self) {
        self.zfod = true;
    }

    /// 建 MMIO 物理窗口 VMO（P6 A-2，**boot-only**——无 syscall 面：物理窗口
    /// 是 boot 授的能力，EL0 不可自造；Zircon `zx_vmo_create_physical` 收紧
    /// 版）。语义面（窗口恒空记账/内容四拒/双侧对齐执法）见
    /// [`PageLedger::new_mmio`]。
    pub fn create_mmio(pa_base: usize, size: usize) -> Result<Self, VmoError> {
        Ok(Self {
            ledger: PageLedger::new_mmio(pa_base, size)?,
            demand_pins: DemandLedger::default(),
            demand: false,
            zfod: false,
        })
    }

    /// MMIO 物理窗口后备？（vmar map 的 attr 分派用：Device-nGnRnE + SH_NONE）。
    pub fn is_mmio(&self) -> bool {
        self.ledger.is_mmio()
    }

    /// 提交页区间：立即出页 + 清零。幂等（已提交页跳过）；整页粒度。
    pub fn commit(
        &mut self,
        fa: &mut FrameAllocator,
        offset: usize,
        len: usize,
    ) -> Result<(), VmoError> {
        let range = self.ledger.commit_pages(offset, len)?;
        for page in range.iter() {
            if self.ledger.page_pa(page).is_none() {
                let pa = zero_frame(fa)?;
                self.ledger.mark_committed(page, Some(pa));
            }
        }
        Ok(())
    }

    /// 运行时安全提交（F0-3）：逐页经 `ALLOC_LOCK` 分配新物理帧并在锁外完成 `DC ZVA` 清零。
    pub fn commit_global(&mut self, offset: usize, len: usize) -> Result<(), VmoError> {
        let range = self.ledger.commit_pages(offset, len)?;
        for page in range.iter() {
            if self.ledger.page_pa(page).is_none() {
                let pa = zero_frame_global()?;
                self.ledger.mark_committed(page, Some(pa));
            }
        }
        Ok(())
    }

    /// 归还已提交页（3-2）：先整段验「全已提交且**无映射**」再逐页释放（先查后
    /// 动——不留半截副作用）。映射中的页拒绝（[`VmoError::Mapped`]）——先 unmap。
    pub fn decommit(
        &mut self,
        fa: &mut FrameAllocator,
        offset: usize,
        len: usize,
    ) -> Result<(), VmoError> {
        let (_, pas) = self.ledger.decommit_pages(offset, len)?;
        for (page, _registered) in pas {
            let pa = self
                .ledger
                .mark_decommitted(page)
                .expect("上已校验全提交");
            debug_assert_eq!(Some(pa), Some(_registered));
            // SAFETY: 该页独占（mapcnt==0 → 无映射别名）且 PA 来自 alloc。
            let _ = fa.free(pa >> PAGE_SHIFT, 0); // 元数据异常只泄漏不 panic
        }
        Ok(())
    }

    /// 运行时安全归还（F0-3）：先在锁外收集待退页表（含 `Vec` 堆分配），再持 `ALLOC_LOCK` 借用遍历批量退还，
    /// 锁释放后再在锁外析构临时 `Vec`（严禁在 `ALLOC_LOCK` 窗内触发 `SlabHeap::dealloc` 自重入）。
    pub fn decommit_global(&mut self, offset: usize, len: usize) -> Result<(), VmoError> {
        let (_, pas) = self.ledger.decommit_pages(offset, len)?;
        crate::mm::with_frames(|fa| {
            for &(page, _registered) in pas.iter() {
                let pa = self
                    .ledger
                    .mark_decommitted(page)
                    .expect("上已校验全提交");
                debug_assert_eq!(Some(pa), Some(_registered));
                let _ = fa.free(pa >> PAGE_SHIFT, 0);
            }
        });
        Ok(())
    }

    /// 经内核 direct map 读已提交区（字节粒度；先查后拷——不留半截副作用）。
    /// W3-Q78 起三道门判序 + 分段推进单源 `berry_vmoledger::copy_gate`/
    /// `walk_copy`（判序即正确性：mmio 拒先于 bounds 先于 committed）。
    pub fn read(&self, offset: usize, out: &mut [u8]) -> Result<(), VmoError> {
        berry_vmoledger::copy_gate(&self.ledger, offset, out.len())?;
        let mut done = 0usize;
        berry_vmoledger::walk_copy(&self.ledger, offset, out.len(), |pa, off, n| {
            // SAFETY: 独占物理页经 direct map 只读；页已提交（copy_gate 过）。
            unsafe {
                core::ptr::copy_nonoverlapping(
                    phys_to_virt(pa + off) as *const u8,
                    out.as_mut_ptr().add(done),
                    n,
                );
            }
            done += n;
        });
        Ok(())
    }

    /// 经内核 direct map 写已提交区（字节粒度；同上，先查后拷）。
    /// W3-Q78 起三道门判序 + 分段推进单源（read 同款）。
    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), VmoError> {
        berry_vmoledger::copy_gate(&self.ledger, offset, data.len())?;
        let mut done = 0usize;
        berry_vmoledger::walk_copy(&self.ledger, offset, data.len(), |pa, off, n| {
            // SAFETY: 独占物理页经 direct map 写；页已提交（copy_gate 过）。
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr().add(done),
                    phys_to_virt(pa + off) as *mut u8,
                    n,
                );
            }
            done += n;
        });
        Ok(())
    }

    /// 容量（字节，已页对齐）。
    pub fn size(&self) -> usize {
        self.ledger.size()
    }

    /// 第 `page_idx` 页的后备 PA（未提交 → `None`）。Vmar map 逐页取用。
    /// MMIO 窗口后备：页 PA = `pa_base + idx*PAGE_SIZE`（连续设备物理区间，
    /// 恒「已提交」——无 commit 语义）。
    pub fn page_pa(&self, page_idx: usize) -> Option<usize> {
        self.ledger.page_pa(page_idx)
    }

    // ---- 映射计数（vmar map/unmap 回调；悬空 PTE 防线，见 ledger 注释）----

    /// 区间映射计数 +1（vmar `map` 成功后回调）。
    pub(crate) fn track_map(&mut self, offset: usize, len: usize) {
        self.ledger.track_map(offset, len);
    }

    /// 区间映射计数 -1（vmar `unmap` 成功后回调，经 Region 存的对象指针）。
    pub(crate) fn untrack_map(&mut self, offset: usize, len: usize) {
        self.ledger.untrack_map(offset, len);
    }

    /// 任一页仍被映射？（handle close 拒绝面：映射未解除的 VMO 不许销毁。）
    pub fn any_mapped(&self) -> bool {
        self.ledger.any_mapped()
    }

    // ---- demand 区间钉计数（G4-2 修批 P0-1；vmar map_demand/unmap/Vmar::Drop
    //      回调，见 demand_pins 注释——与 mapcnt 职责正交）----

    /// demand 区间钉 +len 页（`Vmar::map_demand` 登记成功后回调——**登记即
    /// 取**，与 track_map 的「供给成功才取」差一档：未供给页指针已活）。
    pub(crate) fn track_demand(&mut self, len: usize) {
        self.demand_pins.track(len);
    }

    /// demand 区间钉 -len 页（`Vmar::unmap` / `Vmar::Drop` 收 Demand 区间
    /// 回调——Region 即死，钉即放）。
    pub(crate) fn untrack_demand(&mut self, len: usize) {
        self.demand_pins.untrack(len);
    }

    /// 任一 demand 区间仍登记着本对象？（close/replace 拒面 + drain 泄漏
    /// 判据的第二分量——与 `any_mapped` 并集即「有 Region 指着它」。）
    pub fn demand_mapped(&self) -> bool {
        self.demand_pins.is_mapped()
    }
}

impl Drop for Vmo {
    /// 对象销毁回收物理页（补 3-0 裁剪项）：归还 mapcnt==0 的已提交页；**仍被
    /// 映射的页泄漏不还**（悬空 PTE 防线的最后一环）。handle 表路径到不了这里
    /// （close/replace 对映射中 **或 demand 登记中** VMO 拒 -EBUSY；drain 对
    /// 同判据（`any_mapped() || demand_mapped()`）整对象泄漏不递减——Plan B
    /// 对抗审查修，见 handle.rs；demand 分量 = G4-2 修批 P0-1 面 a/c）；本
    /// Drop 只兜 boot/ktest 直构对象——其映射常驻不解除（如用户进程
    /// code/data/stack 页），泄漏即 v1 政策。
    /// MMIO 窗口后备（P6 A-2）：`pages` 恒空 → 还帧面天然零动作——设备 PA 从
    /// 不进帧记账，无「还设备页进 buddy」面。
    fn drop(&mut self) {
        // F0-4：未残留映射的 MMIO 窗口销毁时，自动将交集内活跃 VirtIO 槽写 STATUS=0 复位，阻断遗留异步 DMA。
        if let Some((pa_base, size)) = self.ledger.mmio_pa_range() {
            if !self.ledger.any_mapped() {
                berry_hal_aarch64::reset_virtio_mmio_window(pa_base, size);
            }
            return;
        }
        // F0-4：物理连续阶块 VMO 在全无映射时整阶归还伙伴分配器（严禁拆成 order-0 双还）。
        if let Some((pa_base, order)) = self.ledger.contig_drop_block() {
            crate::mm::with_frames(|fa| {
                let _ = fa.free(pa_base >> PAGE_SHIFT, order);
            });
            return;
        }
        // F0-3：经第 5 层 ALLOC_LOCK 归还无映射页，消除 HT_DRAINQ 异步析构与并发分配的数据竞争。
        crate::mm::with_frames(|fa| {
            for (_page, pa) in self.ledger.droppable_pages() {
                let _ = fa.free(pa >> PAGE_SHIFT, 0);
            }
        });
    }
}

/// 从帧分配器取一页并清零，返回 **PA**（`alloc` 给 PFN 须左移；buddy 只管元数据，
/// 帧内容不保证干净）。vmo/vmar 共用（`pub(crate)`）。
pub(crate) fn zero_frame(fa: &mut FrameAllocator) -> Result<usize, VmoError> {
    let pfn = fa.alloc().map_err(|_| VmoError::NoFrames)?;
    let pa = pfn << PAGE_SHIFT;
    // SAFETY: 独占新分配帧（尚无别名引用）；Normal WB 直映（清零臂前提，
    // 见 `zero_page_4k` 合同注）。
    unsafe { zero_page_4k(phys_to_virt(pa) as *mut u8) };
    Ok(pa)
}

/// 运行时安全取零帧（F0-3）：仅在 `fa.alloc()` 瞬间持 `ALLOC_LOCK`，随后在锁外完成 `DC ZVA` 4KiB 清零。
pub(crate) fn zero_frame_global() -> Result<usize, VmoError> {
    let pfn = crate::mm::with_frames(|fa| fa.alloc()).map_err(|_| VmoError::NoFrames)?;
    let pa = pfn << PAGE_SHIFT;
    // SAFETY: 刚弹出的独占物理帧尚无任何外部可见别名，锁外直映清零安全无争用。
    unsafe { zero_page_4k(phys_to_virt(pa) as *mut u8) };
    Ok(pa)
}

// ---- 4KiB 专用清零臂（快赢 ③：DC ZVA 主档 + GPR 回退，2026-11-17）----
//
// 为什么：旧路径 `write_bytes` → 本仓 memset（`kernel/src/mem.rs:35`）＝
// 8B GPR 循环 512 迭代/页——旧 icount 基线（改前，2026-11-17）里
// vmo_commit1p=1683、清零占大头（`research/内核深度进化/01-内存管理.md`
// 机会 1）。DC ZVA（Data Cache Zero by VA，ARMv8 系统指令）按缓存行零化：
// 64B 行下 64 指令零完 4KiB——落地实测 vmo_commit1p 1683→396。
// 非 SIMD——不碰 FP/NEON 寄存器，内核 SIMD-free 合同（`kernel/src/mem.rs`
// 头注 + ktest `test_fp` ③-b canary）不破。
//
// 语义前提（`zero_frame` 三个调用点结构性成立：vmo commit 页 / vmar
// `new_root` / `next_table` 均 fresh-from-buddy）：①帧独占无别名；②页
// 4KiB 对齐 + 行整除 4096（探测门保证）→ 行序列恰覆页内不越页；③Normal
// WB + inner-shareable 直映（`kernel/src/vmar.rs` 匿名页属性同款）——ZVA
// 零化至一致性点（PoC），shareable 域内对全体核可见，与 memset 版读写
// 语义逐字节等价。
//
// 探测（F0-1 修）：读 `DCZID_EL0`（ARM DDI 0487 D24.2.42）而非 `CTR_EL0`——
// `DCZID_EL0.DZP`（bit 4）＝0 才许执行 `DC ZVA`（置 1 时执行即 UNDEFINED），
// 零化块大小 = `4 << DCZID_EL0.BS`（bits[3:0]），解码律单源在 `berry_memface`。
// 禁止或块大小不整除 4096 → GPR 回退（`write_bytes` → 本仓整数 `memset`）。

/// DC ZVA 探测缓存：0 = 未探测；`NO_ZVA` = 已探测不可用；其他 = 零化块字节数。
/// 惰性首探落在 boot 单线程窗（首个 `zero_frame` = 建表/建 VMO 期）；SMP
/// 起核后并发首探是良性竞态——同型 CPU 写同值（Relaxed 足够：值只依赖
/// 只读 `DCZID_EL0`，无跨序依赖面）。
static ZVA_PROBE: AtomicUsize = AtomicUsize::new(0);
/// 「已探测、不可用」哨兵（防不可用形态每调重探 mrs）。
const NO_ZVA: usize = usize::MAX;

/// 探测并缓存 `DC ZVA` 零化块字节数（不可用 = `NO_ZVA`）。
fn zva_probe_cached() -> usize {
    let cached = ZVA_PROBE.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }
    // SAFETY: dczid_el0 为架构定义的块零化 ID 只读寄存器，EL1 直读无副作用。
    let dczid: u64;
    unsafe {
        core::arch::asm!(
            "mrs {}, dczid_el0",
            out(reg) dczid,
            options(nostack, preserves_flags)
        );
    }
    let v = berry_memface::dczid_block_size(dczid, PAGE_SIZE).unwrap_or(NO_ZVA);
    ZVA_PROBE.store(v, Ordering::Relaxed);
    v
}

/// 返回实测 `DC ZVA` 块大小（供 ktest 核验硬件探测结果；禁用态返 `None`）。
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn zva_block_size_probed() -> Option<usize> {
    let v = zva_probe_cached();
    (v != NO_ZVA).then_some(v)
}

/// 返回实测 `DC ZVA` 块大小（裸机 boot ktest 探针）。
#[cfg(not(test))]
pub(crate) fn zva_block_size_probed() -> Option<usize> {
    let v = zva_probe_cached();
    (v != NO_ZVA).then_some(v)
}

/// 4KiB 页专用清零：DC ZVA 主档（行步进环）+ GPR 回退档（旧 `write_bytes`
/// 路径原样）。调用方合同：`va` 4KiB 对齐、独占、Normal WB 直映——
/// `zero_frame` 是唯一调用点，前提继承帧分配语义。
///
/// SAFETY: `va` 须指向独占可写的 4KiB Normal WB 页（直映）。
unsafe fn zero_page_4k(va: *mut u8) {
    let line = zva_probe_cached();
    if line != NO_ZVA {
        // SAFETY: 页对齐 + line 整除 PAGE_SIZE（探测门）→ 恰覆页内；dc zva
        // 写内存（PoC 零化）不碰栈与 NZCV。
        let mut p = va;
        let end = va.add(PAGE_SIZE);
        while p < end {
            core::arch::asm!("dc zva, {0}", in(reg) p, options(nostack, preserves_flags));
            p = p.add(line);
        }
    } else {
        // SAFETY: 同旧 write_bytes 合同（GPR 回退——mem.rs 整数 memset）。
        core::ptr::write_bytes(va, 0, PAGE_SIZE);
    }
}
