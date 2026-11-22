//! 每任务用户地址空间的组装层（2b-2 引入；2b-3 起填表改走 **Vmar/Vmo 正式映射
//! 路径**，手工 L2/L3 填表退役）。
//!
//! F1-1 / F1-2 架构重构：将硬件页表根（`l1_pa`）、`atomic_asid`、每空间独立 `pt_lock`、
//! 物理页表帧账本（`page_tables`）、两阶段拆解门闩（`torn_down`）与基于 `BTreeMap` 的
//! 层级区间树（`vmarface::VmarTree`）收口为实体对象 [`AddressSpace`]。
//! 末线程退出或对象析构时经 [`AddressSpace::teardown`] 单点清空叶映射、释放 `ObjRef`
//! 映射保活计数、按 ASID 失效 TLB 并 100% 归还 L3/L2/L1 页表物理帧与 ASID。

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use berry_hal_aarch64 as hal;
use berry_mm::asid::AsidAllocator;
use berry_mm::frame::FrameAllocator;
use berry_mm::va::{phys_to_virt, PAGE_SIZE};
use berry_vmarface::{
    aligned_nonzero, bounds_end, ActiveLeaf, LeafKind, RemovedSpan, SliceMeta, VmarId, VmarTree,
    VmarTreeError,
};

use crate::handle::ObjRef;
use crate::vmar::{
    deregister_demand_aspace, table_base, Perm, PtGuard, Vmar, VmarError, USER_VA_TOP,
};
use crate::vmo::{zero_frame, zero_frame_global, Vmo};

/// 测试/任务私有页的低半 VA（页 1——Vmar root 管辖界 `[0x1000, USER_VA_TOP)`，
/// 页 0 留作 NULL 解引用保护区，2b-4 起对用户禁 0 页）。
pub const USER_VA: usize = 0x1000;

/// 每级表项数（4KiB 表 / 8 字节）与索引掩码。
const IDX_MASK: usize = 0x1ff;
/// 表描述符 bit[1]（block 此位=0；用户树中间层只可能出现表描述符）。
const TABLE_BIT: u64 = 1 << 1;
/// 描述符里的输出地址掩码（bits[47:12]）。
const ADDR_MASK: u64 = 0x0000_FFFF_FFFF_F000;
/// AP + PXN + UXN 权限位总掩码。
const PERM_MASK: u64 = (0b11 << 6) | hal::PXN | hal::UXN;

/// 独立 TTAS 自旋锁（护住区间树与页表帧清单）。
pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        SpinLockGuard { lock: self }
    }
}

impl<'a, T> Deref for SpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: SpinLockGuard 独占互斥期。
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: SpinLockGuard 独占互斥期。
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

unsafe impl<T: Send> Send for SpinLock<T> {}
unsafe impl<T: Send> Sync for SpinLock<T> {}

/// 区间树中叶节点绑定的 VMO 元数据投影（含可选 `ObjRef` 页级映射保活引用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmoRegionMeta {
    pub vmo: *mut Vmo,
    pub vmo_obj: Option<NonNull<ObjRef>>,
    pub vmo_off: usize,
    pub is_mmio: bool,
}

impl SliceMeta for VmoRegionMeta {
    fn slice_at(&self, offset: usize) -> Self {
        Self {
            vmo: self.vmo,
            vmo_obj: self.vmo_obj,
            vmo_off: self.vmo_off + offset,
            is_mmio: self.is_mmio,
        }
    }
}

unsafe impl Send for VmoRegionMeta {}
unsafe impl Sync for VmoRegionMeta {}

#[inline]
fn release_removed_span(span: &RemovedSpan<VmoRegionMeta>) {
    // SAFETY: Vmo 本体在句柄表路径由 ObjRef.map_refs 保活，在直构测试路径由声明序保活。
    unsafe {
    // W3-38 probe: span 释放点（vmo 裸指针 + vmo_obj 保活柄）
    crate::klog::dbg_trace(b"[rsp v=");
    crate::klog::dbg_hex(span.meta.vmo as usize);
    if let Some(vo) = span.meta.vmo_obj {
        crate::klog::dbg_trace(b" vo=");
        crate::klog::dbg_hex(vo.as_ptr() as usize);
    }
    crate::klog::dbg_trace(b"]\n");
    (*span.meta.vmo).untrack_map(span.meta.vmo_off, span.len);
        if span.kind == LeafKind::Demand {
            (*span.meta.vmo).untrack_demand(span.len);
        }
        if let Some(vo) = span.meta.vmo_obj {
            ObjRef::release_map(vo, span.len / PAGE_SIZE);
        }
    }
}

/// 将 `vmarface` 的区间树错误码严格投影为 `VmarError`。
pub fn tree_err(e: VmarTreeError) -> VmarError {
    match e {
        VmarTreeError::Unaligned => VmarError::Unaligned,
        VmarTreeError::OutOfBounds => VmarError::OutOfBounds,
        VmarTreeError::Overlap => VmarError::Overlap,
        VmarTreeError::NotMapped | VmarTreeError::InvalidVmar => VmarError::NotMapped,
    }
}

/// 实体硬件虚拟地址空间（F1-1 / F1-2 核心实体）。
pub struct AddressSpace {
    l1_pa: usize,
    asid: AtomicU16,
    torn_down: AtomicBool,
    page_tables: SpinLock<Vec<usize>>,
    tree: SpinLock<VmarTree<VmoRegionMeta>>,
    pt_lock: AtomicBool,
}

impl AddressSpace {
    /// 新建空地址空间：分配 L1 根表帧并初始化区间树。
    pub fn new(fa: &mut FrameAllocator, asid: u16) -> Result<Arc<Self>, VmarError> {
        let l1_pa = zero_frame(fa).map_err(|_| VmarError::NoFrames)?;
        let mut pt_frames = Vec::new();
        pt_frames.try_reserve(4).map_err(|_| VmarError::NoMem)?;
        pt_frames.push(l1_pa);
        Ok(Arc::new(Self {
            l1_pa,
            asid: AtomicU16::new(asid),
            torn_down: AtomicBool::new(false),
            page_tables: SpinLock::new(pt_frames),
            tree: SpinLock::new(VmarTree::new_root(PAGE_SIZE, USER_VA_TOP - PAGE_SIZE)),
            pt_lock: AtomicBool::new(false),
        }))
    }

    /// 运行时安全新建空地址空间（`ALLOC_LOCK` 护住帧分配）。
    pub fn new_global(asid: u16) -> Result<Arc<Self>, VmarError> {
        let l1_pa = zero_frame_global().map_err(|_| VmarError::NoFrames)?;
        let mut pt_frames = Vec::new();
        if pt_frames.try_reserve(4).is_err() {
            let _ = crate::mm::with_frames(|fa| fa.free(l1_pa >> berry_mm::va::PAGE_SHIFT, 0));
            return Err(VmarError::NoMem);
        }
        pt_frames.push(l1_pa);
        Ok(Arc::new(Self {
            l1_pa,
            asid: AtomicU16::new(asid),
            torn_down: AtomicBool::new(false),
            page_tables: SpinLock::new(pt_frames),
            tree: SpinLock::new(VmarTree::new_root(PAGE_SIZE, USER_VA_TOP - PAGE_SIZE)),
            pt_lock: AtomicBool::new(false),
        }))
    }

    /// `TTBR0` 表根物理地址。
    pub fn ttbr0_pa(&self) -> usize {
        self.l1_pa
    }

    /// 硬件 ASID。
    pub fn asid(&self) -> u16 {
        self.asid.load(Ordering::Acquire)
    }

    /// 本地址空间是否已完成两阶段拆解。
    pub fn is_torn_down(&self) -> bool {
        self.torn_down.load(Ordering::Acquire)
    }

    /// 取本地址空间的页表自旋锁守卫（集中记账 lockdep `CLASS_PT`）。
    pub fn lock_pt(&self) -> PtGuard {
        #[cfg(feature = "lockdep")]
        hal::lockdep::acquire(hal::lockdep::CLASS_PT);
        while self
            .pt_lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.pt_lock.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        PtGuard::from_lock(&self.pt_lock)
    }

    /// 激活本地址空间。
    pub fn activate(&self) {
        hal::activate_aspace(self.l1_pa, self.asid());
    }

    /// O(log N) 查找包含 `va` 的活跃叶区间。
    pub fn find_leaf_at(&self, va: usize) -> Option<ActiveLeaf<VmoRegionMeta>> {
        self.tree.lock().find_leaf_at(va)
    }

    /// O(log N) 缺页反查 `Demand` 区间。
    pub fn demand_region_at(&self, far: usize) -> Option<(*mut Vmo, usize, usize)> {
        let leaf = self.tree.lock().demand_hit(far)?;
        Some((leaf.meta.vmo, leaf.meta.vmo_off, leaf.start))
    }

    /// 在父 `Vmar` 下切分子 `Vmar` 预留。
    pub fn allocate_child(
        &self,
        parent_id: VmarId,
        vaddr: usize,
        len: usize,
    ) -> Result<VmarId, VmarError> {
        let _pt = self.lock_pt();
        if self.torn_down.load(Ordering::Acquire) {
            return Err(VmarError::NotMapped);
        }
        self.tree
            .lock()
            .allocate_child(parent_id, vaddr, len)
            .map_err(tree_err)
    }

    /// 销毁指定子 `Vmar`：递归释放其内部所有叶区间与后代子 `Vmar`，使父空间对应预留立即恢复可映射。
    pub fn destroy_vmar(&self, vmar_id: VmarId) -> Result<(), VmarError> {
        let _pt = self.lock_pt();
        if self.torn_down.load(Ordering::Acquire) {
            return Ok(());
        }
        let removed = self.tree.lock().destroy_vmar(vmar_id);
        for span in &removed {
            self.clear_leaves(span.start, span.len / PAGE_SIZE);
            release_removed_span(span);
        }
        if !removed.is_empty() {
            let asid = self.asid();
            if asid != 0 {
                // SAFETY: 特权内存屏障。
                unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
                hal::tlbi_aside1(asid);
                hal::tlbshoot::broadcast();
            }
        }
        Ok(())
    }

    /// 映射 VMO 区间（填 L3 叶 + 同步 PoU 指令缓存 + 登记区间树）。
    pub fn map(
        &self,
        fa: &mut FrameAllocator,
        vmar_id: VmarId,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        let _pt = self.lock_pt();
        if self.torn_down.load(Ordering::Acquire) {
            return Err(VmarError::NotMapped);
        }
        self.tree
            .lock()
            .check_slot(vmar_id, vaddr, len)
            .map_err(tree_err)?;
        if vmo_offset % PAGE_SIZE != 0 {
            return Err(VmarError::Unaligned);
        }
        let npages = len / PAGE_SIZE;
        let first_page = vmo_offset / PAGE_SIZE;
        for i in 0..npages {
            if vmo.page_pa(first_page + i).is_none() {
                return Err(VmarError::VmoNotCommitted);
            }
        }

        let (attr, sh) = if vmo.is_mmio() {
            (hal::ATTR_DEVICE, hal::SH_NONE)
        } else {
            (hal::ATTR_NORMAL, hal::SH_INNER)
        };
        let mut filled = 0;
        for i in 0..npages {
            let pa = vmo.page_pa(first_page + i).expect("上已校验");
            let va = vaddr + i * PAGE_SIZE;
            let slot = match self.l3_slot_create(fa, va) {
                Ok(s) => s,
                Err(e) => {
                    self.clear_leaves(vaddr, filled);
                    // SAFETY: 特权内存屏障。
                    unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
                    hal::tlbi_aside1(self.asid());
                    hal::tlbshoot::broadcast();
                    return Err(e);
                }
            };
            if !vmo.is_mmio() && perm.allows_exec() {
                hal::sync_icache_pou(phys_to_virt(pa), PAGE_SIZE);
            }
            // SAFETY: 持本空间 pt_lock 独占写 L3 叶槽。
            unsafe {
                slot.write_volatile(hal::page_descriptor(pa, attr, sh) | perm.pte_bits());
            }
            filled += 1;
        }
        // SAFETY: 特权内存屏障。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        vmo.track_map(vmo_offset, len);
        // W3-38 probe: 插叶点（map 直构形）
        crate::klog::dbg_trace(b"[lf+m v=");
        crate::klog::dbg_hex(vmo as *mut Vmo as usize);
        crate::klog::dbg_trace(b" va=");
        crate::klog::dbg_hex(vaddr);
        crate::klog::dbg_trace(b" n=");
        crate::klog::dbg_hex(len / PAGE_SIZE);
        crate::klog::dbg_trace(b"]\n");
        self.tree
            .lock()
            .insert_leaf(
                vmar_id,
                vaddr,
                len,
                LeafKind::Mapped,
                perm.into(),
                VmoRegionMeta {
                    vmo: vmo as *mut Vmo,
                    vmo_obj: None,
                    vmo_off: vmo_offset,
                    is_mmio: vmo.is_mmio(),
                },
            )
            .map_err(tree_err)?;
        Ok(vaddr)
    }

    /// 运行时安全映射（可选绑定句柄表 `ObjRef` 页级映射保活）。
    pub fn map_global_with_obj(
        &self,
        vmar_id: VmarId,
        vmo_obj: Option<NonNull<ObjRef>>,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        let _pt = self.lock_pt();
        if self.torn_down.load(Ordering::Acquire) {
            return Err(VmarError::NotMapped);
        }
        self.tree
            .lock()
            .check_slot(vmar_id, vaddr, len)
            .map_err(tree_err)?;
        if vmo_offset % PAGE_SIZE != 0 {
            return Err(VmarError::Unaligned);
        }
        let npages = len / PAGE_SIZE;
        let first_page = vmo_offset / PAGE_SIZE;
        for i in 0..npages {
            if vmo.page_pa(first_page + i).is_none() {
                return Err(VmarError::VmoNotCommitted);
            }
        }

        let (attr, sh) = if vmo.is_mmio() {
            (hal::ATTR_DEVICE, hal::SH_NONE)
        } else {
            (hal::ATTR_NORMAL, hal::SH_INNER)
        };
        let mut filled = 0;
        for i in 0..npages {
            let pa = vmo.page_pa(first_page + i).expect("上已校验");
            let va = vaddr + i * PAGE_SIZE;
            let slot = match self.l3_slot_create_global(va) {
                Ok(s) => s,
                Err(e) => {
                    self.clear_leaves(vaddr, filled);
                    // SAFETY: 特权内存屏障。
                    unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
                    hal::tlbi_aside1(self.asid());
                    hal::tlbshoot::broadcast();
                    return Err(e);
                }
            };
            if !vmo.is_mmio() && perm.allows_exec() {
                hal::sync_icache_pou(phys_to_virt(pa), PAGE_SIZE);
            }
            // SAFETY: 持本空间 pt_lock 独占写 L3 叶槽。
            unsafe {
                slot.write_volatile(hal::page_descriptor(pa, attr, sh) | perm.pte_bits());
            }
            filled += 1;
        }
        // SAFETY: 特权内存屏障。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        vmo.track_map(vmo_offset, len);
        if let Some(vo) = vmo_obj {
            // SAFETY: 调用方已验证 vo 为活跃 Vmo ObjRef。
            unsafe { (*vo.as_ptr()).retain_map(npages) };
        }
        self.tree
            .lock()
            .insert_leaf(
                vmar_id,
                vaddr,
                len,
                LeafKind::Mapped,
                perm.into(),
                VmoRegionMeta {
                    vmo: vmo as *mut Vmo,
                    vmo_obj,
                    vmo_off: vmo_offset,
                    is_mmio: vmo.is_mmio(),
                },
            )
            .map_err(tree_err)?;
        // W3-38 probe: 插叶点（global 形）
        crate::klog::dbg_trace(b"[lf+g v=");
        crate::klog::dbg_hex(vmo as *mut Vmo as usize);
        if let Some(vo) = vmo_obj {
            crate::klog::dbg_trace(b" vo=");
            crate::klog::dbg_hex(vo.as_ptr() as usize);
        }
        crate::klog::dbg_trace(b" va=");
        crate::klog::dbg_hex(vaddr);
        crate::klog::dbg_trace(b" n=");
        crate::klog::dbg_hex(len / PAGE_SIZE);
        crate::klog::dbg_trace(b"]\n");
        Ok(vaddr)
    }

    /// 按需映射：不填叶、零帧分配，仅登记 `Demand` 区间并钉住 VMO（可选绑定 `ObjRef` 保活）。
    pub fn map_demand_with_obj(
        &self,
        vmar_id: VmarId,
        vmo_obj: Option<NonNull<ObjRef>>,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        let _pt = self.lock_pt();
        if self.torn_down.load(Ordering::Acquire) {
            return Err(VmarError::NotMapped);
        }
        self.tree
            .lock()
            .check_slot(vmar_id, vaddr, len)
            .map_err(tree_err)?;
        if vmo_offset % PAGE_SIZE != 0 {
            return Err(VmarError::Unaligned);
        }
        if !vmo.is_demand() {
            return Err(VmarError::VmoNotCommitted);
        }
        if vmo_offset.checked_add(len).map_or(true, |end| end > vmo.size()) {
            return Err(VmarError::OutOfBounds);
        }
        self.tree
            .lock()
            .insert_leaf(
                vmar_id,
                vaddr,
                len,
                LeafKind::Demand,
                perm.into(),
                VmoRegionMeta {
                    vmo: vmo as *mut Vmo,
                    vmo_obj,
                    vmo_off: vmo_offset,
                    is_mmio: false,
                },
            )
            .map_err(tree_err)?;
        vmo.track_demand(len);
        if let Some(vo) = vmo_obj {
            // SAFETY: 调用方已验证 vo 为活跃 Vmo ObjRef。
            unsafe { (*vo.as_ptr()).retain_map(len / PAGE_SIZE) };
        }
        // W3-38 probe: 插叶点（demand 形）
        crate::klog::dbg_trace(b"[lf+d v=");
        crate::klog::dbg_hex(vmo as *mut Vmo as usize);
        if let Some(vo) = vmo_obj {
            crate::klog::dbg_trace(b" vo=");
            crate::klog::dbg_hex(vo.as_ptr() as usize);
        }
        crate::klog::dbg_trace(b" va=");
        crate::klog::dbg_hex(vaddr);
        crate::klog::dbg_trace(b" n=");
        crate::klog::dbg_hex(len / PAGE_SIZE);
        crate::klog::dbg_trace(b"]\n");
        Ok(vaddr)
    }

    /// 解除映射（支持精确与连续子区间切分；对已完成 `teardown` 的已退进程空间幂等返回 `Ok(())`）。
    pub fn unmap(
        &self,
        vmar_id: VmarId,
        scope_base: usize,
        scope_len: usize,
        vaddr: usize,
        len: usize,
    ) -> Result<(), VmarError> {
        let _pt = self.lock_pt();
        if !aligned_nonzero(vaddr, len) {
            return Err(VmarError::Unaligned);
        }
        if self.torn_down.load(Ordering::Acquire) {
            if bounds_end(vaddr, len, scope_base, scope_len).is_none() {
                return Err(VmarError::NotMapped);
            }
            return Ok(());
        }
        let removed = self
            .tree
            .lock()
            .unmap_range(vmar_id, vaddr, len)
            .map_err(tree_err)?;
        for span in &removed {
            self.clear_leaves(span.start, span.len / PAGE_SIZE);
            release_removed_span(span);
        }
        // SAFETY: 特权内存屏障。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        hal::tlbi_aside1(self.asid());
        hal::tlbshoot::broadcast();
        Ok(())
    }

    /// 修改权限（支持精确与连续子区间切分）：重写叶 PTE、按需同步 PoU 缓存并广播 TLB。
    pub fn protect(
        &self,
        vmar_id: VmarId,
        vaddr: usize,
        len: usize,
        perm: Perm,
    ) -> Result<(), VmarError> {
        let _pt = self.lock_pt();
        if self.torn_down.load(Ordering::Acquire) {
            return Err(VmarError::NotMapped);
        }
        if vaddr % PAGE_SIZE != 0 || len % PAGE_SIZE != 0 || len == 0 {
            return Err(VmarError::Unaligned);
        }
        {
            let tree = self.tree.lock();
            let mut curr = vaddr;
            let end = vaddr.checked_add(len).ok_or(VmarError::NotMapped)?;
            while curr < end {
                if let Some(leaf) = tree.find_leaf_at(curr) {
                    if leaf.meta.is_mmio && perm.allows_exec() {
                        return Err(VmarError::NotMapped);
                    }
                    curr = leaf.start + leaf.len;
                } else {
                    break;
                }
            }
        }
        let protected = self
            .tree
            .lock()
            .protect_range(vmar_id, vaddr, len, perm.into())
            .map_err(tree_err)?;
        for span in protected {
            let sync_pou = !span.meta.is_mmio && perm.allows_exec();
            let npages = span.len / PAGE_SIZE;
            for i in 0..npages {
                let va = span.start + i * PAGE_SIZE;
                if let Some(slot) = self.l3_slot_peek(va) {
                    // SAFETY: 持本空间 pt_lock 独占更新 L3 叶权限位。
                    unsafe {
                        let e = slot.read_volatile();
                        if sync_pou && (e & 1) != 0 {
                            let pa = (e & ADDR_MASK) as usize;
                            hal::sync_icache_pou(phys_to_virt(pa), PAGE_SIZE);
                        }
                        slot.write_volatile((e & !PERM_MASK) | perm.pte_bits());
                    }
                }
            }
        }
        // SAFETY: 特权内存屏障。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        hal::tlbi_aside1(self.asid());
        hal::tlbshoot::broadcast();
        Ok(())
    }

    /// 供给一页按需映射并执行预填闭包。
    pub fn supply_page_init(
        &self,
        fa: &mut FrameAllocator,
        vaddr: usize,
        init_before_pte: impl FnOnce(&mut Vmo, usize),
    ) -> Result<(), VmarError> {
        let va = vaddr & !(PAGE_SIZE - 1);
        let _pt = self.lock_pt();
        if self.torn_down.load(Ordering::Acquire) {
            return Err(VmarError::NotMapped);
        }
        let (vmo, vmo_off, region_start, perm_u8) = {
            let tree = self.tree.lock();
            let leaf = tree.demand_hit(va).ok_or(VmarError::NotMapped)?;
            (leaf.meta.vmo, leaf.meta.vmo_off, leaf.start, leaf.perm)
        };
        if let Some(slot) = self.l3_slot_peek(va) {
            // SAFETY: 只读探查叶项有效位。
            if unsafe { slot.read_volatile() } & 1 != 0 {
                return Ok(());
            }
        }
        let page_off = vmo_off + (va - region_start);
        // SAFETY: demand 钉与 ObjRef.map_refs 保活 Vmo。
        let vmo_ref = unsafe { &mut *vmo };
        vmo_ref.commit(fa, page_off, PAGE_SIZE).map_err(|e| match e {
            crate::vmo::VmoError::NoFrames => VmarError::NoFrames,
            crate::vmo::VmoError::NoMem => VmarError::NoMem,
            _ => VmarError::NotMapped,
        })?;
        let pa = vmo_ref.page_pa(page_off / PAGE_SIZE).expect("刚 commit 过");
        let slot = self.l3_slot_create(fa, va)?;
        let perm = Perm::try_from(perm_u8)?;
        init_before_pte(vmo_ref, page_off);
        if perm.allows_exec() {
            hal::sync_icache_pou(phys_to_virt(pa), PAGE_SIZE);
        }
        let pte_bits = perm.pte_bits();
        // SAFETY: 持本空间 pt_lock 独占写叶项并发屏障。
        unsafe {
            slot.write_volatile(
                hal::page_descriptor(pa, hal::ATTR_NORMAL, hal::SH_INNER) | pte_bits,
            );
            core::arch::asm!("dsb ish", options(nostack, preserves_flags));
        }
        hal::tlbi_aside1(self.asid());
        hal::tlbshoot::broadcast();
        vmo_ref.track_map(page_off, PAGE_SIZE);
        Ok(())
    }

    /// 运行时安全按需供给一页并执行预填闭包。
    pub fn supply_page_init_global(
        &self,
        vaddr: usize,
        init_before_pte: impl FnOnce(&mut Vmo, usize),
    ) -> Result<(), VmarError> {
        let va = vaddr & !(PAGE_SIZE - 1);
        let _pt = self.lock_pt();
        if self.torn_down.load(Ordering::Acquire) {
            return Err(VmarError::NotMapped);
        }
        let (vmo, vmo_off, region_start, perm_u8) = {
            let tree = self.tree.lock();
            let leaf = tree.demand_hit(va).ok_or(VmarError::NotMapped)?;
            (leaf.meta.vmo, leaf.meta.vmo_off, leaf.start, leaf.perm)
        };
        if let Some(slot) = self.l3_slot_peek(va) {
            // SAFETY: 只读探查叶项有效位。
            if unsafe { slot.read_volatile() } & 1 != 0 {
                return Ok(());
            }
        }
        let page_off = vmo_off + (va - region_start);
        // SAFETY: demand 钉与 ObjRef.map_refs 保活 Vmo。
        let vmo_ref = unsafe { &mut *vmo };
        vmo_ref.commit_global(page_off, PAGE_SIZE).map_err(|e| match e {
            crate::vmo::VmoError::NoFrames => VmarError::NoFrames,
            crate::vmo::VmoError::NoMem => VmarError::NoMem,
            _ => VmarError::NotMapped,
        })?;
        let pa = vmo_ref.page_pa(page_off / PAGE_SIZE).expect("刚 commit_global 过");
        let slot = self.l3_slot_create_global(va)?;
        let perm = Perm::try_from(perm_u8)?;
        init_before_pte(vmo_ref, page_off);
        if perm.allows_exec() {
            hal::sync_icache_pou(phys_to_virt(pa), PAGE_SIZE);
        }
        let pte_bits = perm.pte_bits();
        // SAFETY: 持本空间 pt_lock 独占写叶项并发屏障。
        unsafe {
            slot.write_volatile(
                hal::page_descriptor(pa, hal::ATTR_NORMAL, hal::SH_INNER) | pte_bits,
            );
            core::arch::asm!("dsb ish", options(nostack, preserves_flags));
        }
        hal::tlbi_aside1(self.asid());
        hal::tlbshoot::broadcast();
        vmo_ref.track_map(page_off, PAGE_SIZE);
        Ok(())
    }

    /// 供给一页（无预填）。
    #[allow(dead_code)]
    pub fn supply_page(&self, fa: &mut FrameAllocator, vaddr: usize) -> Result<(), VmarError> {
        self.supply_page_init(fa, vaddr, |_, _| {})
    }

    /// 运行时安全供给一页（无预填）。
    pub fn supply_page_global(&self, vaddr: usize) -> Result<(), VmarError> {
        self.supply_page_init_global(vaddr, |_, _| {})
    }

    /// 完整拆解地址空间（F1-2 两阶段确定性销毁第一阶段，幂等）：
    /// 1. 在 `pt_lock` 内置位 `torn_down`，封死后续一切对已释放页表帧的触碰；
    /// 2. 若本核当前 `TTBR0_EL1` 仍指向 `self.l1_pa`，立即切回空表 `(0, 0)`；
    /// 3. 注销 `DEMAND_ROOTS`；
    /// 4. 排空区间树，清零叶 PTE，执行全量 `untrack_map` / `untrack_demand` 与 `ObjRef::release_map`；
    /// 5. 执行单次 Inner Shareable 按 ASID 跨核 TLB 失效屏障；
    /// 6. 将全部 L3/L2/L1 页表物理帧 100% 归还 `Buddy` 并释放硬件 ASID。
    pub fn teardown(&self) {
        let _pt = self.lock_pt();
        if self.torn_down.swap(true, Ordering::AcqRel) {
            return;
        }
        if hal::current_ttbr0_pa() == self.l1_pa {
            hal::activate_aspace(0, 0);
        }
        let asid = self.asid.swap(0, Ordering::AcqRel);
        if asid != 0 {
            deregister_demand_aspace(asid, self as *const AddressSpace);
        }
        let removed = self.tree.lock().drain_all();
        crate::klog::dbg_trace(b"[td-drain n=");
        crate::klog::dbg_hex(removed.len());
        crate::klog::dbg_trace(b"]\n");
        for span in &removed {
            crate::klog::dbg_trace(b"[td-sp v=");
            crate::klog::dbg_hex(span.meta.vmo as usize);
            crate::klog::dbg_trace(b"]\n");
            self.clear_leaves(span.start, span.len / PAGE_SIZE);
            release_removed_span(span);
        }
        if asid != 0 {
            // SAFETY: 特权内存屏障；tlbi aside1is 硬件广播全 Inner Shareable 域。
            unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
            hal::tlbi_aside1(asid);
        }

        let frames: Vec<usize> = {
            let mut pt = self.page_tables.lock();
            core::mem::take(&mut *pt)
        };
        for pa in frames {
            crate::klog::dbg_trace(b"[td-fr pa="); crate::klog::dbg_hex(pa);
            crate::klog::dbg_trace(b"]\n");
            let _ = crate::mm::with_frames(|fa| fa.free(pa >> berry_mm::va::PAGE_SHIFT, 0));
        }

        if asid != 0 {
            free_asid(asid);
        }
    }

    // ---- 内部页表走查与清叶 ----

    fn next_table(
        fa: &mut FrameAllocator,
        slot: *mut u64,
        page_tables: &SpinLock<Vec<usize>>,
    ) -> Result<usize, VmarError> {
        // SAFETY: 持 pt_lock 独占读中间表槽。
        let e = unsafe { slot.read_volatile() };
        if e & TABLE_BIT != 0 {
            Ok((e as usize) & ADDR_MASK as usize)
        } else {
            assert!(e == 0, "AddressSpace: 中间层出现非表描述符");
            let mut pt = page_tables.lock();
            pt.try_reserve(1).map_err(|_| VmarError::NoMem)?;
            let pa = zero_frame(fa).map_err(|_| VmarError::NoFrames)?;
            pt.push(pa);
            drop(pt);
            // SAFETY: 持 pt_lock 独占写中间表槽。
            unsafe { slot.write_volatile(hal::table_descriptor(pa)) };
            Ok(pa)
        }
    }

    fn next_table_global(
        slot: *mut u64,
        page_tables: &SpinLock<Vec<usize>>,
    ) -> Result<usize, VmarError> {
        // SAFETY: 持 pt_lock 独占读中间表槽。
        let e = unsafe { slot.read_volatile() };
        if e & TABLE_BIT != 0 {
            Ok((e as usize) & ADDR_MASK as usize)
        } else {
            assert!(e == 0, "AddressSpace: 中间层出现非表描述符");
            {
                let mut pt = page_tables.lock();
                pt.try_reserve(1).map_err(|_| VmarError::NoMem)?;
            }
            let pa = zero_frame_global().map_err(|_| VmarError::NoFrames)?;
            page_tables.lock().push(pa);
            // SAFETY: 持 pt_lock 独占写中间表槽。
            unsafe { slot.write_volatile(hal::table_descriptor(pa)) };
            Ok(pa)
        }
    }

    fn l3_slot_create(&self, fa: &mut FrameAllocator, va: usize) -> Result<*mut u64, VmarError> {
        let l1 = table_base(self.l1_pa);
        let l2_pa = Self::next_table(
            fa,
            // SAFETY: 索引经 0x1ff 掩码约束在 4KiB 表内。
            unsafe { l1.add((va >> 30) & IDX_MASK) },
            &self.page_tables,
        )?;
        let l2 = table_base(l2_pa);
        let l3_pa = Self::next_table(
            fa,
            // SAFETY: 同上。
            unsafe { l2.add((va >> 21) & IDX_MASK) },
            &self.page_tables,
        )?;
        let l3 = table_base(l3_pa);
        // SAFETY: 同上。
        Ok(unsafe { l3.add((va >> 12) & IDX_MASK) })
    }

    fn l3_slot_create_global(&self, va: usize) -> Result<*mut u64, VmarError> {
        let l1 = table_base(self.l1_pa);
        let l2_pa = Self::next_table_global(
            // SAFETY: 索引经 0x1ff 掩码约束在 4KiB 表内。
            unsafe { l1.add((va >> 30) & IDX_MASK) },
            &self.page_tables,
        )?;
        let l2 = table_base(l2_pa);
        let l3_pa = Self::next_table_global(
            // SAFETY: 同上。
            unsafe { l2.add((va >> 21) & IDX_MASK) },
            &self.page_tables,
        )?;
        let l3 = table_base(l3_pa);
        // SAFETY: 同上。
        Ok(unsafe { l3.add((va >> 12) & IDX_MASK) })
    }

    pub fn l3_slot_peek(&self, va: usize) -> Option<*mut u64> {
        let l1 = table_base(self.l1_pa);
        // SAFETY: 只读表项。
        let e0 = unsafe { l1.add((va >> 30) & IDX_MASK).read_volatile() };
        if e0 & TABLE_BIT == 0 {
            return None;
        }
        let l2 = table_base((e0 as usize) & ADDR_MASK as usize);
        // SAFETY: 只读表项。
        let e1 = unsafe { l2.add((va >> 21) & IDX_MASK).read_volatile() };
        if e1 & TABLE_BIT == 0 {
            return None;
        }
        let l3 = table_base((e1 as usize) & ADDR_MASK as usize);
        // SAFETY: 索引约束在 4KiB 表内。
        Some(unsafe { l3.add((va >> 12) & IDX_MASK) })
    }

    pub fn clear_leaves(&self, vaddr: usize, npages: usize) {
        for i in 0..npages {
            let va = vaddr + i * PAGE_SIZE;
            if let Some(slot) = self.l3_slot_peek(va) {
                // SAFETY: 持 pt_lock 清零叶项。
                unsafe { slot.write_volatile(0) };
            }
        }
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        // 若进程末线程已先行执行过 teardown()，此处 swap(true) 立即 O(1) 返回；
        // 若为未启动进程关闭或 standalone 根 Vmar 析构，此处完整回收页表帧与 ASID。
        let _kl = hal::kern_lock::kern_acquire_irqsave_reentrant();
        self.teardown();
    }
}

// ---- 全局 ASID 分配器管理 ----

/// 全局 ASID 分配器。
static mut ASIDS: Option<AsidAllocator> = None;

/// ASID 分配器 TTAS 自旋锁。
static ASID_LOCK: AtomicBool = AtomicBool::new(false);

/// 取 ASID 锁。
fn asid_lock() {
    #[cfg(feature = "lockdep")]
    berry_hal_aarch64::lockdep::acquire(berry_hal_aarch64::lockdep::CLASS_ASID);
    while ASID_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        while ASID_LOCK.load(Ordering::Relaxed) {
            core::hint::spin_loop();
        }
    }
}

/// 尝试分配一个空闲且未在任一核 `TTBR0_EL1` 钉死的 ASID（F0-2 分代位图
/// 形；首调用惰性初始化）。S-4 4-3 锁形不变：计数器/位图推进由
/// [`ASID_LOCK`] 互斥（原「单核顺序访问」假设随 `sys_process_create`
/// 双核化失效）；回卷 quiesce 恒在**锁外**——锁内取号 + 回卷代数 bump
/// （代数序 = 取号序），rolled 即放锁，锁外 [`roll_quiesce_arm`]（本核
/// 全量失效 + 广播置闩，**不等待**——SVC 路等待归 dispatch 尾，内核任务
/// 路显式驱动）。单核（boot 前期 / ktest）零争用直过，arm 短路为仅本地
/// 失效（与 4-3 前行为零漂移）。全范围满载时返回 `None`，供
/// `sys_process_create` 折返 `-ENOMEM`，严禁重复发放活跃号击穿 TLB 隔离。
pub fn try_alloc_asid() -> Option<u16> {
    let pinned = berry_hal_aarch64::active_cpu_asids();
    asid_lock();
    // SAFETY: ASID_LOCK 临界区（互斥）；惰性初始化同临界区内完成。boot
    // DAIF 窗 / SVC 屏蔽窗 / 测试任务窗均可达，持锁窗有界（纯位图操作）。
    let picked = unsafe {
        let slot = &raw mut ASIDS;
        if (*slot).is_none() {
            *slot = Some(AsidAllocator::new(berry_hal_aarch64::asid_bits()));
        }
        let res = (*slot).as_mut().unwrap().try_alloc(&pinned);
        if let Some((_, true)) = res {
            berry_hal_aarch64::tlbshoot::roll_gen_bump();
        }
        res
    };
    // G3-3 lockdep（g3-3c）：物理放点（一）；同款见 asid_force_near_roll。
    #[cfg(feature = "lockdep")]
    berry_hal_aarch64::lockdep::release(berry_hal_aarch64::lockdep::CLASS_ASID);
    ASID_LOCK.store(false, Ordering::Release);
    if let Some((asid, rolled)) = picked {
        if rolled {
            berry_hal_aarch64::tlbshoot::roll_quiesce_arm();
        }
        crate::klog::dbg_trace(b"[aa n="); crate::klog::dbg_hex(asid as usize);
        crate::klog::dbg_trace(b"]\n");
        Some(asid)
    } else {
        None
    }
}

/// 取一个新 ASID（boot / ktest 可信语境便捷形；耗尽即 panic——生产路径
/// 用 [`try_alloc_asid`]）。
pub fn alloc_asid() -> u16 {
    try_alloc_asid().expect("alloc_asid: 硬件可用 ASID 全部耗尽")
}

/// 归还已销毁根地址空间的 ASID（F0-2 分代位图：位图精确跟踪在用号，
/// 归还后可安全复用；`0` 或重复释放为空操作）。
pub fn free_asid(asid: u16) {
    if asid == 0 {
        return;
    }
    crate::klog::dbg_trace(b"[fa n="); crate::klog::dbg_hex(asid as usize);
    crate::klog::dbg_trace(b"]\n");
    asid_lock();
    // SAFETY: ASID_LOCK 临界区（互斥）。
    unsafe {
        let slot = &raw mut ASIDS;
        if let Some(alloc) = (*slot).as_mut() {
            alloc.free(asid);
        }
    }
    // G3-3 lockdep（g3-3c）：物理放点（三）；同款见 try_alloc_asid。
    #[cfg(feature = "lockdep")]
    berry_hal_aarch64::lockdep::release(berry_hal_aarch64::lockdep::CLASS_ASID);
    ASID_LOCK.store(false, Ordering::Release);
}

/// 测试注入：计数器快进到回卷前。
pub fn asid_force_near_roll() {
    asid_lock();
    // SAFETY: ASID_LOCK 临界区互斥。
    unsafe {
        let slot = &raw mut ASIDS;
        if (*slot).is_none() {
            *slot = Some(AsidAllocator::new(berry_hal_aarch64::asid_bits()));
        }
        (*slot).as_mut().unwrap().force_near_roll();
    }
    #[cfg(feature = "lockdep")]
    berry_hal_aarch64::lockdep::release(berry_hal_aarch64::lockdep::CLASS_ASID);
    ASID_LOCK.store(false, Ordering::Release);
}

/// 一个最小任务地址空间：root Vmar + 一页 VMO 映在 `USER_VA`。
/// 字段声明序保证 `vmar` 先于 `_vmo` 析构，`AddressSpace::teardown` 解钉时堆上 `Vmo` 仍存活。
pub struct MiniAspace {
    pub ttbr0_pa: usize,
    pub asid: u16,
    vmar: Vmar,
    _vmo: alloc::boxed::Box<Vmo>,
}

impl MiniAspace {
    pub fn new(fa: &mut FrameAllocator, asid: u16, magic: u64) -> Self {
        let mut vmar =
            Vmar::new_root(fa, asid).expect("MiniAspace: 建 root Vmar（boot 可信语境）");
        let mut vmo = alloc::boxed::Box::new(
            Vmo::create(PAGE_SIZE).expect("MiniAspace: 装 VMO（boot 可信语境）"),
        );
        vmo.commit(fa, 0, PAGE_SIZE).expect("MiniAspace: commit 私有页");
        vmo.write(0, &magic.to_le_bytes()).expect("MiniAspace: 写 magic");
        vmar.map(fa, &mut vmo, USER_VA, 0, PAGE_SIZE, Perm::Rw)
            .expect("MiniAspace: map 私有页");
        Self {
            ttbr0_pa: vmar.ttbr0_pa(),
            asid: vmar.asid(),
            vmar,
            _vmo: vmo,
        }
    }

    pub fn activate(&self) {
        self.vmar.activate();
    }

    pub fn read_va(&self) -> u64 {
        // SAFETY: 测试语境已激活本空间。
        unsafe { (USER_VA as *const u64).read_volatile() }
    }

    pub fn write_va(&self, v: u64) {
        // SAFETY: 测试语境已激活本空间。
        unsafe { (USER_VA as *mut u64).write_volatile(v) };
    }
}
