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
//! 全 API 返回 `Result`（syscall 边界无 panic 纪律——Phase 3 直接折状态码）。

use berry_mm::frame::FrameAllocator;
use berry_mm::va::{phys_to_virt, PAGE_SHIFT, PAGE_SIZE};
use berry_vmoledger::{DemandLedger, PageLedger};

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
}

impl Vmo {
    /// 建空 VMO（不占物理页）。`size` 向上取整到页。size==0 → [`VmoError::OutOfRange`]；
    /// 记账 try 化（3b3 审查 H2）见 [`PageLedger::new_anon`]。
    pub fn create(size: usize) -> Result<Self, VmoError> {
        Ok(Self {
            ledger: PageLedger::new_anon(size)?,
            demand_pins: DemandLedger::default(),
            demand: false,
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

    /// 按需后备？（fault 探针分派用：demand 区间的缺页走 pager 协议。）
    pub fn is_demand(&self) -> bool {
        self.demand
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

    /// 经内核 direct map 读已提交区（字节粒度；先查后拷——不留半截副作用）。
    pub fn read(&self, offset: usize, out: &mut [u8]) -> Result<(), VmoError> {
        if self.ledger.is_mmio() {
            return Err(VmoError::NotSupported); // 设备读不可经拷贝语义中介
        }
        berry_vmoledger::check_bounds(self.ledger.size(), offset, out.len())?;
        self.ledger.ensure_committed(offset, out.len())?;
        let mut pos = offset;
        while pos < offset + out.len() {
            let (pa, off, n) = self.ledger.page_span(pos, (offset + out.len()) - pos);
            // SAFETY: 独占物理页经 direct map 只读；页已提交（ensure_committed 过）。
            unsafe {
                core::ptr::copy_nonoverlapping(
                    phys_to_virt(pa + off) as *const u8,
                    out.as_mut_ptr().add(pos - offset),
                    n,
                );
            }
            pos += n;
        }
        Ok(())
    }

    /// 经内核 direct map 写已提交区（字节粒度；同上，先查后拷）。
    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), VmoError> {
        if self.ledger.is_mmio() {
            return Err(VmoError::NotSupported);
        }
        berry_vmoledger::check_bounds(self.ledger.size(), offset, data.len())?;
        self.ledger.ensure_committed(offset, data.len())?;
        let mut pos = offset;
        while pos < offset + data.len() {
            let (pa, off, n) = self.ledger.page_span(pos, (offset + data.len()) - pos);
            // SAFETY: 独占物理页经 direct map 写；页已提交。
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr().add(pos - offset),
                    phys_to_virt(pa + off) as *mut u8,
                    n,
                );
            }
            pos += n;
        }
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
        for (_page, pa) in self.ledger.droppable_pages() {
            // SAFETY: 独占页（无映射别名）；PA 来自 FrameAllocator::alloc。
            let _ = crate::mm::frames().free(pa >> PAGE_SHIFT, 0);
        }
    }
}

/// 从帧分配器取一页并清零，返回 **PA**（`alloc` 给 PFN 须左移；buddy 只管元数据，
/// 帧内容不保证干净）。vmo/vmar 共用（`pub(crate)`）。
pub(crate) fn zero_frame(fa: &mut FrameAllocator) -> Result<usize, VmoError> {
    let pfn = fa.alloc().map_err(|_| VmoError::NoFrames)?;
    let pa = pfn << PAGE_SHIFT;
    // SAFETY: 独占新分配帧（尚无别名引用）；Normal WB。
    unsafe { core::ptr::write_bytes(phys_to_virt(pa) as *mut u8, 0, PAGE_SIZE) };
    Ok(pa)
}
