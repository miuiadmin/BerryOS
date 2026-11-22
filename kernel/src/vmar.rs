//! Vmar（地址区间句柄投影，`内存管理设计.md` §3；2b-3）。
//!
//! F1-1 架构重构：实体硬件地址空间、物理页表树（`l1_pa`、每空间 `pt_lock`、中间表帧账本）
//! 与层级区间树（`vmarface::VmarTree`）解耦至 [`crate::aspace::AddressSpace`]。
//! 本文件中的 [`Vmar`] 降级为指向该地址空间的轻量句柄投影，包含 `Arc<AddressSpace>`、
//! `vmar_id`、`base` 与 `len`。
//!
//! 子 `Vmar` 经由 `allocate` 创建，管辖界收窄，与父空间完全共享同一页表根与 ASID；子 `Vmar`
//! 销毁（`Drop`）时通过 `vmarface::destroy_vmar` 递归清除子区间预留与叶映射，父空间对应
//! 预留立即恢复可映射。按需调页根表（`DEMAND_ROOTS`）改按 `*const AddressSpace` 登记，
//! 补齐 `Acquire/Release` 硬件内存序并彻底消除父子 `Vmar` `OwnerConflict`。

use alloc::sync::Arc;
use berry_hal_aarch64 as hal;
use berry_mm::frame::FrameAllocator;
use berry_mm::va::{phys_to_virt, PAGE_SIZE};
use berry_vslotface::droot;
use berry_vmarface::{VmarId, ROOT_VMAR_ID};
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};

use crate::aspace::AddressSpace;
use crate::vmo::Vmo;

/// 用户 VA 上界：T0SZ=25 → TTBR0 区 = `[0, 2^39)`。
pub const USER_VA_TOP: usize = 1 << 39;

// ---- per-AddressSpace 页表自旋锁守卫与当前任务快路径 ----

/// PT 锁 RAII 守卫：跟随 [`AddressSpace`] 实例的 `pt_lock`（废除 64KiB 全局 `PT_LOCKS`）。
pub struct PtGuard {
    lock: *const AtomicBool,
}

impl PtGuard {
    #[inline]
    pub(crate) const fn from_lock(lock: &AtomicBool) -> Self {
        Self {
            lock: lock as *const AtomicBool,
        }
    }

    #[inline]
    pub const fn noop() -> Self {
        Self {
            lock: core::ptr::null(),
        }
    }
}

impl Drop for PtGuard {
    fn drop(&mut self) {
        if !self.lock.is_null() {
            #[cfg(feature = "lockdep")]
            hal::lockdep::release(hal::lockdep::CLASS_PT);
            // SAFETY: 守卫存续期内目标 AddressSpace 由运行中任务或调用者引用保活。
            unsafe { (*self.lock).store(false, Ordering::Release) };
        }
    }
}

/// 引导期裸 `UserProcess`（`current_pinned() == 0` 的常驻测试/守护任务，全树仅 8 例）反查表。
const BOOT_ASPACE_SLOTS: usize = 16;
static BOOT_ASPACES: [(AtomicU16, AtomicUsize); BOOT_ASPACE_SLOTS] =
    [const { (AtomicU16::new(0), AtomicUsize::new(0)) }; BOOT_ASPACE_SLOTS];

/// 登记引导期常驻 `UserProcess` 的 `AddressSpace`（供 `current_pinned() == 0` 时 `pt_lock_current` 定位）。
pub fn register_boot_aspace(asid: u16, aspace: *const AddressSpace) {
    if asid == 0 || aspace.is_null() {
        return;
    }
    for (a, p) in &BOOT_ASPACES {
        let cur = a.load(Ordering::Acquire);
        if cur == asid {
            p.store(aspace as usize, Ordering::Release);
            return;
        }
        if cur == 0 {
            p.store(aspace as usize, Ordering::Release);
            a.store(asid, Ordering::Release);
            return;
        }
    }
    panic!("register_boot_aspace: 16 槽引导地址空间表溢出");
}

/// 读者侧入口（`syscall.rs` `copy_*` 链 EL0 路）：直接锁当前运行进程的 `AddressSpace.pt_lock`。
pub fn pt_lock_current() -> PtGuard {
    let pinned = berry_sched::current_pinned();
    if pinned != 0 {
        // SAFETY: current_pinned() 指向当前运行线程钉住的 Process ObjRef，全 syscall 窗有效。
        let proc = unsafe {
            (*(pinned as *const crate::handle::ObjRef))
                .obj
                .as_process()
                .expect("current_pinned 必指向 Process")
        };
        return proc.aspace.lock_pt();
    }
    let asid = hal::current_asid();
    if asid == 0 {
        return PtGuard::noop();
    }
    for (a, p) in &BOOT_ASPACES {
        if a.load(Ordering::Acquire) == asid {
            let ptr = p.load(Ordering::Acquire) as *const AddressSpace;
            if !ptr.is_null() {
                // SAFETY: UserProcess 的根 Vmar 终身驻留不释放。
                return unsafe { (*ptr).lock_pt() };
            }
        }
    }
    PtGuard::noop()
}

/// 映射权限（F0-2 四态硬件 W^X + SMEP `PXN` 矩阵）：
/// - `Rw` / `Ro`：仅 EL1 访问，强制 `PXN | UXN` 不可执行；
/// - `UserRo`：`AP=11`（EL0 只读），强制 `PXN | UXN` 不可执行；
/// - `UserRw`：`AP=01`（EL0 读写），强制 `PXN | UXN` 不可执行（严格 W^X 数据/栈页）；
/// - `UserRx`：`AP=11`（EL0 只读可执行），强制 `PXN`（SMEP：禁 EL1 执行用户代码页）；
/// - `UserRwxCompat`：过渡期平铺镜像兼容态（`AP=01 | PXN`），强制置 `PXN = 1` 阻断 EL1 ret2usr。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perm {
    Rw = 0,
    Ro = 1,
    UserRo = 2,
    UserRw = 3,
    UserRx = 4,
    UserRwxCompat = 5,
}

impl From<Perm> for u8 {
    fn from(p: Perm) -> u8 {
        p as u8
    }
}

impl TryFrom<u8> for Perm {
    type Error = VmarError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Perm::Rw),
            1 => Ok(Perm::Ro),
            2 => Ok(Perm::UserRo),
            3 => Ok(Perm::UserRw),
            4 => Ok(Perm::UserRx),
            5 => Ok(Perm::UserRwxCompat),
            _ => Err(VmarError::NotMapped),
        }
    }
}

impl Perm {
    /// 合成页表叶描述符权限位（`AP[2:1] | PXN | UXN`）。
    pub const fn pte_bits(self) -> u64 {
        match self {
            Perm::Rw => hal::PXN | hal::UXN,
            Perm::Ro => hal::AP_RO | hal::PXN | hal::UXN,
            Perm::UserRo => hal::AP_USER_RO | hal::PXN | hal::UXN,
            Perm::UserRw => hal::AP_USER_RW | hal::PXN | hal::UXN,
            Perm::UserRx => hal::AP_USER_RO | hal::PXN,
            Perm::UserRwxCompat => hal::AP_USER_RW | hal::PXN,
        }
    }

    /// 本权限是否允许 EL0 取指执行。
    pub const fn allows_exec(self) -> bool {
        matches!(self, Perm::UserRx | Perm::UserRwxCompat)
    }
}

/// Vmar 错误。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum VmarError {
    /// VA/offset/len 未页对齐。
    Unaligned,
    /// 越出本 Vmar 管辖界。
    OutOfBounds,
    /// 与已映射区间/子 Vmar 预留重叠。
    Overlap,
    /// unmap/protect 找不到匹配的已映射区间。
    NotMapped,
    /// VMO 覆盖的页未全部 commit。
    VmoNotCommitted,
    /// 帧分配器无空闲页。
    NoFrames,
    /// 堆 OOM。
    NoMem,
}

/// 地址区间句柄投影（F1-1：持有底层 `Arc<AddressSpace>` 引用）。
pub struct Vmar {
    pub(crate) aspace: Arc<AddressSpace>,
    pub(crate) vmar_id: VmarId,
    pub(crate) base: usize,
    pub(crate) len: usize,
}

impl Vmar {
    /// 建 root Vmar：创建底层 `AddressSpace` 并赋予根区间管辖权 `[0x1000, USER_VA_TOP)`。
    pub fn new_root(fa: &mut FrameAllocator, asid: u16) -> Result<Self, VmarError> {
        let aspace = AddressSpace::new(fa, asid)?;
        Ok(Self {
            aspace,
            vmar_id: ROOT_VMAR_ID,
            base: PAGE_SIZE,
            len: USER_VA_TOP - PAGE_SIZE,
        })
    }

    /// 运行时安全建 root Vmar（`ALLOC_LOCK` 护住帧分配）。
    pub fn new_root_global(asid: u16) -> Result<Self, VmarError> {
        let aspace = AddressSpace::new_global(asid)?;
        Ok(Self {
            aspace,
            vmar_id: ROOT_VMAR_ID,
            base: PAGE_SIZE,
            len: USER_VA_TOP - PAGE_SIZE,
        })
    }

    /// 表根 PA（`Task::ttbr0` / 激活用）。
    pub fn ttbr0_pa(&self) -> usize {
        self.aspace.ttbr0_pa()
    }

    /// ASID。
    pub fn asid(&self) -> u16 {
        self.aspace.asid()
    }

    /// 本区间管辖起始基址。
    pub const fn base(&self) -> usize {
        self.base
    }

    /// 本区间管辖长度。
    pub const fn len(&self) -> usize {
        self.len
    }

    /// 区间树内部节点 ID。
    pub const fn vmar_id(&self) -> VmarId {
        self.vmar_id
    }

    /// 本 Vmar 是否为拥有完整用户低半 VA 的根区间。
    pub const fn is_root(&self) -> bool {
        self.vmar_id == ROOT_VMAR_ID
    }

    /// 获取底层地址空间共享引用。
    pub fn aspace(&self) -> &Arc<AddressSpace> {
        &self.aspace
    }

    /// 获取底层 `AddressSpace` 稳定堆地址（根与任意后代子 `Vmar` 恒同址）。
    pub fn aspace_ptr(&self) -> *const AddressSpace {
        Arc::as_ptr(&self.aspace)
    }

    /// 激活本 AS（TTBR0+CONTEXTIDR+按号失效）。
    pub fn activate(&self) {
        self.aspace.activate();
    }

    /// 切子 Vmar：在区间树中分配 `[vaddr, vaddr+len)` 预留节点，返回共享同一地址空间的新 Vmar 句柄。
    pub fn allocate(&mut self, vaddr: usize, len: usize) -> Result<Vmar, VmarError> {
        let child_id = self.aspace.allocate_child(self.vmar_id, vaddr, len)?;
        Ok(Vmar {
            aspace: self.aspace.clone(),
            vmar_id: child_id,
            base: vaddr,
            len,
        })
    }

    /// 把 VMO 的 `[vmo_offset, vmo_offset+len)` 映射到 `[vaddr, vaddr+len)`。
    pub fn map(
        &mut self,
        fa: &mut FrameAllocator,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        self.aspace
            .map(fa, self.vmar_id, vmo, vaddr, vmo_offset, len, perm)
    }

    /// 运行时安全映射（持 `pt_lock` 在外，按需经 `ALLOC_LOCK` 申请中间页表帧）。
    pub fn map_global(
        &mut self,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        self.aspace.map_global_with_obj(
            self.vmar_id,
            None,
            vmo,
            vaddr,
            vmo_offset,
            len,
            perm,
        )
    }

    /// 句柄表路径安全映射：附带 `ObjRef` 页级映射保活计数，使任意进程销毁序下零 UAF、零泄漏。
    pub fn map_obj_global(
        &mut self,
        vmo_obj: core::ptr::NonNull<crate::handle::ObjRef>,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        self.aspace.map_global_with_obj(
            self.vmar_id,
            Some(vmo_obj),
            vmo,
            vaddr,
            vmo_offset,
            len,
            perm,
        )
    }

    /// 按需映射：不填叶、零帧分配，仅登记 `Demand` 区间并钉住 VMO。
    pub fn map_demand(
        &mut self,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        self.aspace.map_demand_with_obj(
            self.vmar_id,
            None,
            vmo,
            vaddr,
            vmo_offset,
            len,
            perm,
        )
    }

    /// 句柄表路径按需映射：附带 `ObjRef` 页级映射保活计数。
    pub fn map_demand_obj(
        &mut self,
        vmo_obj: core::ptr::NonNull<crate::handle::ObjRef>,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        self.aspace.map_demand_with_obj(
            self.vmar_id,
            Some(vmo_obj),
            vmo,
            vaddr,
            vmo_offset,
            len,
            perm,
        )
    }

    /// 供给一页（无预填）。
    #[allow(dead_code)]
    pub fn supply_page(&mut self, fa: &mut FrameAllocator, vaddr: usize) -> Result<(), VmarError> {
        self.aspace.supply_page(fa, vaddr)
    }

    /// 运行时安全按需供给（无预填）。
    #[allow(dead_code)]
    pub fn supply_page_global(&mut self, vaddr: usize) -> Result<(), VmarError> {
        self.aspace.supply_page_global(vaddr)
    }

    /// 供给一页并在装入有效 PTE 之前执行帧内预填闭包。
    pub fn supply_page_init(
        &mut self,
        fa: &mut FrameAllocator,
        vaddr: usize,
        init_before_pte: impl FnOnce(&mut Vmo, usize),
    ) -> Result<(), VmarError> {
        self.aspace.supply_page_init(fa, vaddr, init_before_pte)
    }

    /// 运行时安全按需供给并预填物理帧。
    pub fn supply_page_init_global(
        &mut self,
        vaddr: usize,
        init_before_pte: impl FnOnce(&mut Vmo, usize),
    ) -> Result<(), VmarError> {
        self.aspace.supply_page_init_global(vaddr, init_before_pte)
    }

    /// fault 探针面：O(log N) 查询 `far` 是否命中 `Demand` 区间。
    pub fn demand_region_at(&self, far: usize) -> Option<(*mut Vmo, usize, usize)> {
        self.aspace.demand_region_at(far)
    }

    /// 解除映射（支持精确匹配与任意连续子区间切分；对已拆解进程空间幂等返回 `Ok(())`）。
    pub fn unmap(&mut self, vaddr: usize, len: usize) -> Result<(), VmarError> {
        self.aspace
            .unmap(self.vmar_id, self.base, self.len, vaddr, len)
    }

    /// 改权限（支持精确匹配与任意连续子区间切分）。
    pub fn protect(&mut self, vaddr: usize, len: usize, perm: Perm) -> Result<(), VmarError> {
        self.aspace.protect(self.vmar_id, vaddr, len, perm)
    }
}

// ---- asid → AddressSpace 按需调页反查登记表（F1-1：Acquire/Release + 消解子 Vmar OwnerConflict）----

pub use berry_vslotface::droot::DEMAND_ROOT_SLOTS;
const _: () = assert!(DEMAND_ROOT_SLOTS == 16);

pub use berry_vslotface::droot::DemandRootError;

static DEMAND_ROOTS: [(AtomicU16, AtomicUsize); DEMAND_ROOT_SLOTS] =
    [const { (AtomicU16::new(0), AtomicUsize::new(0)) }; DEMAND_ROOT_SLOTS];

struct AtomicRootSlots;

impl berry_vslotface::droot::RootSlots for AtomicRootSlots {
    fn slot(&self, i: usize) -> berry_vslotface::droot::RootSlot {
        let (a, p) = &DEMAND_ROOTS[i];
        // F1-1：Acquire 加载先读 asid 再读 ptr，与 set() 的 Release 发布严格配对。
        let asid = a.load(Ordering::Acquire);
        let ptr = p.load(Ordering::Acquire);
        berry_vslotface::droot::RootSlot { asid, ptr }
    }
    fn set(&mut self, i: usize, s: berry_vslotface::droot::RootSlot) {
        let (a, p) = &DEMAND_ROOTS[i];
        if s.ptr != 0 {
            p.store(s.ptr, Ordering::Release);
            a.store(s.asid, Ordering::Release);
        } else {
            a.store(0, Ordering::Release);
            p.store(0, Ordering::Release);
        }
    }
}

/// 登记 `asid -> *const AddressSpace`（同一地址空间的根/子 `Vmar` 共享同一指针，天然幂等零冲突）。
pub fn register_demand_aspace(
    asid: u16,
    aspace: *const AddressSpace,
) -> Result<(), DemandRootError> {
    let mut slots = AtomicRootSlots;
    droot::register_step(&mut slots, asid, aspace as usize).map(|_| ())
}

/// 反查 `asid -> *const AddressSpace`。
pub fn demand_aspace(asid: u16) -> Option<*const AddressSpace> {
    let slots = AtomicRootSlots;
    droot::lookup_step(&slots, asid).map(|p| p as *const AddressSpace)
}

/// 注销 `asid -> *const AddressSpace`（`asid + ptr` 双匹配防误清复用槽）。
pub fn deregister_demand_aspace(asid: u16, aspace: *const AddressSpace) {
    let mut slots = AtomicRootSlots;
    droot::deregister_step(&mut slots, asid, aspace as usize);
}

impl Drop for Vmar {
    fn drop(&mut self) {
        if !self.is_root() {
            // 子 Vmar 释放：解除其在父空间中的预留并回收内部残余叶映射。
            // 根地址空间的调页根注销与 ASID 归还在最后一个 Arc<AddressSpace> Drop 时统一完成。
            let _ = self.aspace.destroy_vmar(self.vmar_id);
        }
    }
}

/// 只读走查任意表树的 L3 叶描述符。
pub(crate) fn walk_leaf(l1_pa: usize, va: usize) -> Option<u64> {
    const PAGE_VALID: u64 = 1;
    let l1 = table_base(l1_pa);
    // SAFETY: 只读页表槽。
    let e0 = unsafe { l1.add((va >> 30) & 0x1ff).read_volatile() };
    if e0 & (1 << 1) == 0 {
        return None;
    }
    let l2 = table_base((e0 as usize) & 0x0000_FFFF_FFFF_F000);
    // SAFETY: 只读页表槽。
    let e1 = unsafe { l2.add((va >> 21) & 0x1ff).read_volatile() };
    if e1 & (1 << 1) == 0 {
        return None;
    }
    let l3 = table_base((e1 as usize) & 0x0000_FFFF_FFFF_F000);
    // SAFETY: 只读页表槽。
    let leaf = unsafe { l3.add((va >> 12) & 0x1ff).read_volatile() };
    (leaf & PAGE_VALID != 0).then_some(leaf)
}

/// 表 PA → 经 direct map 的表基指针。
pub fn table_base(pa: usize) -> *mut u64 {
    phys_to_virt(pa) as *mut u64
}
