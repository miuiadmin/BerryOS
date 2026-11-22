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

<<<<<<< HEAD
/// 区间登记项。
struct Region {
    start: usize,
    len: usize,
    kind: RegionKind,
    /// `Mapped`/`Demand`：指向关联的 Vmo（`map`/`map_demand` 经 `&mut`
    /// 传入）。**指针有效性依托三条政策**：handle 表路径 close/replace 对
    /// 映射中 **或 demand 登记中** VMO 拒 -EBUSY（判据
    /// `any_mapped() || demand_mapped()`——demand 分量 = G4-2 修批 P0-1
    /// 面 a/c：未供给页 mapcnt 恒 0 而指针已活，独立钉住）；drain（进程
    /// 退出清表）对同判据的 refs 将归零 VMO **整对象泄漏**不递减（handle.rs
    /// 对抗审查修——无拒面则用泄漏兜住本指针）；boot/ktest 直构对象由
    /// 调用方保证生存期（v1 泄漏持有——见 `user.rs`）。
    ///
    /// **第四条（地址恒定，2026-11-22 帧归还刀 UAF 判例立法）**：上述三
    /// 条保「VMO 不先于 Region 死」，但 Rust 对 `Vmo` 无地址稳定保证——
    /// 栈局部映射后随构造/返回**移动**，指针即悬空而对象仍活。旧 `Drop`
    /// 不解引用 Mapped 区间故无害；帧归还刀起 `Drop` 清账解引用一切非
    /// null `r.vmo`，悬空实崩（ktest `test_aspace` a/b 栈 Drop：
    /// `untrack_map` 读死帧垃圾当 `slots.ptr`，FAR=测试常量 `MAGIC_A`）
    /// 。故契约升格：**被映射的 Vmo 在区间生存期内不得移动地址**——handle
    /// 表 Box / 属主 `Box<Vmo>`（`aspace.rs` MiniAspace、`user.rs`
    /// UserProcess，2026-11-22 修形）/ `Box::leak`（dpg.rs init 先例）/
    /// static（vdso）四形合法；「栈局部映射再移入结构体」禁形。
    /// `Reserved` 恒 null。
    vmo: *mut Vmo,
    /// `Mapped`：本区间对应 VMO 内偏移（unmap 回调映射计数用）。
    vmo_off: usize,
}

/// W3-Q17：Region 切片只读投影（判定律表抽象单点装配——newtype 过
/// orphan rule，零拷贝纯转发）。
struct RegionSlots<'a>(&'a [Region]);

impl SlotView for RegionSlots<'_> {
    fn slot_len(&self) -> usize {
        self.0.len()
    }
    fn slot_get(&self, i: usize) -> SlotRegion {
        let r = &self.0[i];
        SlotRegion {
            start: r.start,
            len: r.len,
            kind: match r.kind {
                RegionKind::Mapped(_) => SlotKind::Mapped,
                RegionKind::Demand(_) => SlotKind::Demand,
                RegionKind::Reserved => SlotKind::Reserved,
            },
        }
    }
}

/// W3-Q17：判定错误投影 → VmarError（四态一一对应）。
fn slot_err(e: SlotError) -> VmarError {
    match e {
        SlotError::Unaligned => VmarError::Unaligned,
        SlotError::OutOfBounds => VmarError::OutOfBounds,
        SlotError::Overlap => VmarError::Overlap,
        SlotError::NotMapped => VmarError::NotMapped,
    }
}

/// `Mapped` 带权限是为 Phase 3 `query`（§3 操作之一）留数据；v1 无读取点。
#[allow(dead_code)]
enum RegionKind {
    /// 已映射（带权限——protect 改写）。
    Mapped(Perm),
    /// 子 Vmar 预留（allocate 占位，禁止父直接映射）。
    Reserved,
    /// 按需映射（G4-1）：登记区间不填叶——首触缺页走 fault 探针 → pager
    /// 协议逐页供给（`supply_page` 填叶后该页与 `Mapped` 同形）。带权限 =
    /// 供给时叶 AP 位来源。unmap 收割（供给页清叶 + untrack，未供给页天然
    /// 零动作）；protect 对 demand 区间 v1 拒（`NotMapped`——混合供给态的
    /// 语义留 G4-2 后裁）。
    Demand(Perm),
}

/// 地址区间对象。root 与子 Vmar 同构（子共享表根/ASID，base/len 收窄）。
pub struct Vmar {
    /// 页表根（L1）PA——TTBR0 写入值，root 拥有表树。
    l1_pa: usize,
    /// 本 AS ASID（表树属主）。
    asid: u16,
    /// 管辖 VA 下界（root = 0x1000：页 0 留 NULL 保卫）。
    base: usize,
    /// 管辖长度（字节）。
    len: usize,
    /// 直接管辖的区间（线性表；重叠检查 O(n)，v1 区域个位数）。
    regions: Vec<Region>,
    /// 表树属主标记（帧归还刀件 A ④）：`new_root` 置 true、`allocate` 切子
    /// 置 false——子 Vmar 共享父 `l1_pa`/`asid`，**无条件 free_asid 会放掉
    /// 活根的号**（子先死放号 → 根仍在用 → 同号新 AS 复用 = TLB 隔离击穿）
    /// ，故 [`Drop`] 的 ASID 归还只由根执行。
    is_root: bool,
=======
/// 地址区间句柄投影（F1-1：持有底层 `Arc<AddressSpace>` 引用）。
pub struct Vmar {
    pub(crate) aspace: Arc<AddressSpace>,
    pub(crate) vmar_id: VmarId,
    pub(crate) base: usize,
    pub(crate) len: usize,
>>>>>>> private/wip
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
<<<<<<< HEAD
            regions: Vec::new(),
            is_root: true,
=======
>>>>>>> private/wip
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

<<<<<<< HEAD
    /// 切子 Vmar：预留 `[vaddr, vaddr+len)`（登记进父表防父越子映射），返回共享
    /// 表根/ASID、管辖界收窄的新 Vmar。子**不拥有**表树/ASID（帧归还刀件 A ④
    /// ：`is_root=false`，Drop 不 free_asid——号属根，见字段注）。
=======
    /// 切子 Vmar：在区间树中分配 `[vaddr, vaddr+len)` 预留节点，返回共享同一地址空间的新 Vmar 句柄。
>>>>>>> private/wip
    pub fn allocate(&mut self, vaddr: usize, len: usize) -> Result<Vmar, VmarError> {
        let child_id = self.aspace.allocate_child(self.vmar_id, vaddr, len)?;
        Ok(Vmar {
            aspace: self.aspace.clone(),
            vmar_id: child_id,
            base: vaddr,
            len,
<<<<<<< HEAD
            regions: Vec::new(),
            is_root: false,
        })
    }

    /// 把 VMO 的 `[vmo_offset, vmo_offset+len)` 映射到 `[vaddr, vaddr+len)`
    /// （`map 即填用户页表`——§5.1 第 4 步主交付）。v1 要求显式 vaddr（ASLR 留 v2）、
    /// 整页粒度、VMO 覆盖页已全部 commit。返回映射基 VA。成功后回调
    /// `vmo.track_map`（3-2 映射计数——decommit/销毁的悬空 PTE 防线），故收
    /// `&mut Vmo`；Region 存该对象裸指针——**调用方须保 Vmo 此后地址恒定**
    /// （合法四形见 `Region.vmo` 注第四条；2026-11-22 UAF 判例）。
    ///
    /// 失败语义：中途建表失败（`NoFrames`）时已写叶全部回滚（清叶 + `dsb ish` +
    /// 按号失效），返回错误后无残留翻译；regions 未登记该区间，同 VA 重试 map
    /// 可干净重建——中途新建的中间表帧被重试复用（`next_table` 见 TABLE_BIT
    /// 即用、不重复分配），即便不复用也只是 v1 已接受的「不回收页表帧」政策
    /// （见模块头）。
=======
        })
    }

    /// 把 VMO 的 `[vmo_offset, vmo_offset+len)` 映射到 `[vaddr, vaddr+len)`。
>>>>>>> private/wip
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
<<<<<<< HEAD
    /// 对象销毁收口（G4-2 修批 P0-1 面 b 单点执法 + 帧归还刀件 A 重写——
    /// 「刻意的正交不对称」政策就此推翻：残留 Mapped/Demand 区间的 mapcnt
    /// 在此一并清账〔clear_leaves + untrack〕，使关联 VMO 的 `Vmo::Drop`
    /// 还帧判据可满足——A 线主罪〔自映射帧面 VMO 无人 unmap → mapcnt>0
    /// 全额跳过还帧，~469 页/轮净失〕由此拔除）。步骤：
    ///
    /// ① 注销 asid 反查槽（DEMAND_ROOTS 永不指已释放对象——探针 stale
    /// 命中由此消源；**先于一切清账与还帧**：armed EL0 请求槽〔dpg 池〕
    /// 的属主解析全走本反查，注销先行 ⇒ 注销后死臂恒折 GC 臂/让位，
    /// 零 VMO 解引用——「demand 注销序先于还帧」的证明锚点）；
    /// ② PT 锁窗内逐 Region 清账（同 `unmap` 的 531-539 先例：per-AS PT
    /// 锁整段串行——本核/他核同 AS 写者〔map/unmap/protect/supply_page〕
    /// 全走 pt_lock(asid) 嵌 KL 正序，Drop 清叶与供给填叶互斥）：
    /// - `Reserved`（vmo == null）**跳过不解引用**（前次实施 boot 崩一级
    ///   崩因：ktest/装配面对 Reserved 区间解引用 null——FAR=0x10 判例）；
    /// - `clear_leaves` **先于** `untrack_map`（**防线分层**〔审查 P2 修正
    ///   回写 2026-11-22〕：KL 全程串行是第一防线——本 Drop 与 Vmo::Drop
    ///   销毁路径、unmap 的全部写者同被 KL 挡，任何次序下并发还帧面不存
    ///   在；叶序（先 clear 后 untrack）是纵深防御——若 KL 假设未来松动
    ///   （如 v2 细锁化），反序会造
    ///   「mapcnt==0 而 PTE 活」的窗口——此刻若 Vmo::Drop 并发跑〔drain
    ///   phase-2〕即还帧，残留 PTE 指已易主 PA = 静默腐败面；先清叶后
    ///   放账，叶死在先，还帧在后，窗口闭合）；
    /// - Demand 区间双记账同 `unmap`：untrack_map 收供给页（未供给页
    ///   saturating_sub(0) 零动作）+ untrack_demand 收整区间钉；
    /// ③ 收尾屏障 + 按号失效 + broadcast（锁窗内只发不待——回执收割点
    /// 见下方「回执收割面」）；
    /// ④ free_asid **限根**（`is_root`——子 Vmar 复制父 l1_pa/asid，
    /// 无条件放号会放掉活根的号，件 A ④）。
    ///
    /// **untrack_map 解引用安全性**（Region.vmo 裸指针活性， Region 注释
    /// 三政策 + 地址恒定第四条在此 Drop 的重验）：handle 表路径 close/
    /// replace 对「映射中或 demand 登记中」VMO 拒 -EBUSY（判据
    /// `any_mapped() || demand_mapped()`）——销毁性 close 到不了带活
    /// Region 的 VMO；drain 双相（handle.rs）phase-1 落 Vmar 时 VMO 槽全
    /// 未动（phase-2 才递减），phase-2 的泄漏判据同上双分量——「有 Region
    /// 指着的 VMO」永不先于 Vmar 销毁；boot/ktest 直构由调用方保证生存期
    /// **与地址恒定**（2026-11-22 判例：MiniAspace 旧形栈局部映射后移动
    /// 两次，本 Drop 解引用即悬空——修形 Box 见 `aspace.rs`/`user.rs`，
    /// 契约全文 Region 注释第四条）。故清账点的 `(*r.vmo)` 解引用恒活。
    ///
    /// **回执收割面**（broadcast 只发不待，闩收割点逐语境点名）：
    /// - SVC 类（handle_close 0x2E / replace 0x36 归零 drop）：SVC 派发尾
    ///   `exit_checks`（W3-Q108）零锁自旋收割——既有面零改动；
    /// - HT_DRAINQ flush（process.rs `ht_drain_flush`，idle/yield 语境——
    ///   无 SVC 派发尾可等）：flush 尾 KL 释放后**显式收割点**（件 A ⑤
    ///   接线——idle 臂永不等来 dispatch 尾，闩不得悬挂）；
    /// - 溢出同步回退（退出钩子内 drain_pinned_process）：闩由本核下一个
    ///   SVC dispatch 尾收割（钩子语境 IRQ 屏蔽 + KL 在身，不可就地等；
    ///   挂闩期安全性 = 死 AS 无执行流 + ASID 复用另有防线〔activate 按
    ///   号失效 + 回卷 quiesce〕）；
    /// - ktest/boot 直构：单核（sched::init 前）broadcast 零发送零闩直返
    ///   ——零扰动。
    ///
    /// 锁：注销 + 清账须 KL 窗（与登记、探针 stage-2 同锁域）。三释放点
    /// 锁态不一——SVC 类 KL 在身；boot/ktest 直构对象的 Drop 在窗外——由
    /// `kl_held()` 门吸收：未持才物理取放（kern_acquire 非计数锁，同核
    /// 重入自旋死锁——门形 = kern_lock.rs 官方重入判据）。PT 锁嵌 KL 正序
    /// （§3.1 单一向）。
    fn drop(&mut self) {
        let held = hal::kern_lock::kl_held();
        if !held {
            hal::kern_lock::kern_acquire();
        }
        // ① 注销先行（KL 窗内；反查表原子臂无需 PT 锁）。
        deregister_demand_root(self.asid, self as *mut Vmar);
        // ② PT 锁窗清账：写者串行同 unmap 先例；Reserved 跳过（null vmo
        // 不解引用——一级崩因防线）。
        {
            let _pt = pt_lock(self.asid);
            for r in self.regions.iter() {
                if r.vmo.is_null() {
                    continue; // Reserved：无叶无账（allocate 只登记不填叶）
                }
                // 先清叶（叶死在先——顺序硬约束见方法注②）。
                self.clear_leaves(r.start, r.len / PAGE_SIZE);
                // SAFETY: Region 的 vmo 指针有效性 = 方法注「untrack_map
                // 解引用安全性」三政策重验；对象销毁中（Drop 后无任何读者
                // 经本对象到达 regions）。
                unsafe {
                    (*r.vmo).untrack_map(r.vmo_off, r.len);
                    if matches!(r.kind, RegionKind::Demand(_)) {
                        (*r.vmo).untrack_demand(r.len);
                    }
                }
            }
            // ③ 收尾（锁窗内）：清叶对走查可见 + 本核旧 TLB 项作废 + 他核
            // 广播（单核 TX=0 零扰动；回执收割面见方法注）。
            // SAFETY: 特权指令；EL1。
            unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
            hal::tlbi_aside1(self.asid);
            hal::tlbshoot::broadcast();
        }
        if !held {
            hal::kern_lock::kern_release();
        }
        // ④ F0-2 分代位图归还——**限根**（件 A ④：子共享父号，无条件放会
        // 放掉活根的号；根死即本树无人再用，还号安全）。v1 不回收表树帧
        // 的政策不变（模块头），ASID 是唯一随 Drop 归还的硬件资源。
        if self.is_root && self.asid != 0 {
            crate::aspace::free_asid(self.asid);
            self.asid = 0;
=======
    fn drop(&mut self) {
        if !self.is_root() {
            // 子 Vmar 释放：解除其在父空间中的预留并回收内部残余叶映射。
            // 根地址空间的调页根注销与 ASID 归还在最后一个 Arc<AddressSpace> Drop 时统一完成。
            let _ = self.aspace.destroy_vmar(self.vmar_id);
>>>>>>> private/wip
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
