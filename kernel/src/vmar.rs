//! Vmar（地址区间，`内存管理设计.md` §3；2b-3）。
//!
//! root Vmar 覆盖整个用户低半 VA `[0x1000, USER_VA_TOP)`（页 0 留 NULL 解引用
//! 保卫）；`map` 即填用户页表（walk L1→L2→L3、缺中间表按需建帧），`unmap`/
//! `protect` 改叶 + 按 ASID 失效（用户叶 nG=1，`tlbi aside1is` 可达）。
//! `allocate` 切子 Vmar（共享表根/ASID，管辖界收窄——Zircon `zx_vmar_allocate`
//! 语义；v1 用绝对 VA，Phase 3 syscall 层再折 offset 语义）。
//!
//! 区间记账：本 Vmar 直接管辖的 `regions` 线性表（v1 区域个位数；子 Vmar 的
//! 预留在父表登记，防父越子映射）。ASLR 随机化（§3）留 v2（需 RNG）。
//! v1 不回收页表帧/解除预留（对象销毁统一在 Phase 3 能力层）。
//!
//! SMP（S-4 4-2）：写者（map/unmap/protect）全程持 per-AS PT 锁（键 =
//! ASID，见 [`PT_LOCKS`]）；读者（syscall copy 链经 [`walk_leaf`]）由调用
//! 方同窗持锁（walk+copy 同窗铁律，判例 §A113）。跨核改叶后 `broadcast()`
//! 发 SGI #2 广播（非阻塞；回执等待在 SVC 派发尾，见 HAL `tlbshoot`）。
//!
//! 全 API 返回 `Result`（syscall 边界无 panic 纪律）。

use crate::vmo::{zero_frame, Vmo};
use alloc::vec::Vec;
use berry_hal_aarch64 as hal;
use berry_mm::frame::FrameAllocator;
use berry_mm::va::{phys_to_virt, PAGE_SIZE};
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};

/// 用户 VA 上界：T0SZ=25 → TTBR0 区 = `[0, 2^39)`。
pub const USER_VA_TOP: usize = 1 << 39;

/// 每级表项数（4KiB 表 / 8 字节）与索引掩码。
const IDX_MASK: usize = 0x1ff;
/// 表描述符 bit[1]（block 此位=0；用户树中间层只可能出现表描述符）。
const TABLE_BIT: u64 = 1 << 1;
/// 描述符里的输出地址掩码（bits[47:12]；PA < 4GiB 绰绰有余）。
const ADDR_MASK: u64 = 0x0000_FFFF_FFFF_F000;
/// AP 字段（bits[7:6]）掩码——protect 改权用。
const AP_MASK: u64 = 0b11 << 6;

// ---- per-AS 页表自旋锁（S-4 4-2；SMP-S4 草案 §2 4-2①④ + §3.1）----

/// 锁键 = ASID：写者（map/unmap/protect）持 `self.asid`，读者（syscall copy
/// 链）持 `hal::current_asid()`（与 `current_ttbr0_pa` 同一 TTBR0 读点 = 同
/// 窗一致键——walk_leaf 无对象句柄，键控解析闭合此缺口）。定长 64KiB BSS
/// 换 O(1) 无注册解析：8 位硬件域只用前 256 槽、16 位域满用（余槽死重零
/// 成本——BSS 清零不占镜像）。子 Vmar 经共享 asid 天然共享同锁（同树同
/// 锁）。ASID 回卷后同号新 AS 复用同槽 = 仅过串行化（互斥语义不破；回卷
/// 世代精确隔离 = 4-3 领地）。ASID 0（内核空表）永不取锁——空表无用户叶、
/// 无写者。
///
/// 层位：KL/表之右、分配器之上（§3.1：写者 PT 嵌 KL 窗取放 = §A108 过渡
/// 态合法；map 建中间表出帧 = PT 锁内摸分配器，帧分配器纯 KL 串行故锁序
/// 一致）。**不进 HELD 位图、不绑 CURRENT**（§A113 三位说谎面）；TTAS
/// 形 = kern_lock KERN_LOCK 同款（CAS(Acquire) 外环 + Relaxed 内环）。
static PT_LOCKS: [AtomicBool; 0x1_0000] = [const { AtomicBool::new(false) }; 0x1_0000];

/// PT 锁 RAII 守卫：Drop 放锁。map 中途回滚臂等多出口路径零漏放（no_std
/// 无 unwind、panic=halt，Drop 只走正常控制流）。
pub(crate) struct PtGuard {
    asid: u16,
}

impl Drop for PtGuard {
    fn drop(&mut self) {
        // G3-3 lockdep（g3-3c）：物理放点记账——集中本 Drop 覆盖全树一切
        // PT 放点（RAII 单出口）；feature 关 = 编译消除。
        #[cfg(feature = "lockdep")]
        hal::lockdep::release(hal::lockdep::CLASS_PT);
        PT_LOCKS[self.asid as usize].store(false, Ordering::Release);
    }
}

/// 取本 AS PT 锁（TTAS）。无环论证（SMP-S4 草案 §3.2）：持有者**可被断但
/// 有界**——SVC 多数号全程 IRQ 屏蔽（g3-1c-2 起唯 send/recv 拷贝窗局部解蔽：
/// 持有者可被窗内 tick/RX/SGI 打断，但段计数 >0 禁切 + handler 体有界 →
/// 断有界 → 等待者最终必得）；**铁律：窗内可达 IRQ handler 禁取 PT 锁**——
/// 持有者被断期间同核自旋等不可运行者 = 硬死锁（今日零触达；新源先过审计，
/// 雷册 §A126 追记二）。PT 恒在 KL 窗内取放（全系统单一嵌套向，无反转面）。
/// boot/ktest 直调路径同样 DAIF 屏蔽。
fn pt_lock(asid: u16) -> PtGuard {
    // G3-3 lockdep（g3-3c）：物理取点记账——集中本函数 + PtGuard::drop 一对
    // 位点即覆盖全树 6 物理取点（写者 map/unmap/protect 经 self.asid 直达 +
    // 读者 copy 链三辅助经 pt_lock_current 汇聚于此）；feature 关 = 编译消除。
    // 重入检粒度前提：v1 全树无「持 A 槽再取 B 槽」嵌套形态（穷举见
    // lockdep.rs CLASS_PT 注）——单类号无假阳性面。
    #[cfg(feature = "lockdep")]
    hal::lockdep::acquire(hal::lockdep::CLASS_PT);
    // TTAS（test-and-test-and-set）：CAS 外环避免每次写行争、Relaxed 内环
    // 读自旋降总线流量——kern_lock KERN_LOCK 同款形态。
    while PT_LOCKS[asid as usize]
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        while PT_LOCKS[asid as usize].load(Ordering::Relaxed) {
            core::hint::spin_loop();
        }
    }
    PtGuard { asid }
}

/// 读者侧入口（syscall.rs copy_* 链 EL0 路）：锁**当前** AS——`access_ok`
/// 走查与 copy store 同窗（§A113 walk+copy 同窗铁律）。EL1 路（ktest 静态
/// 缓冲）不取：高半直映无用户表。
pub(crate) fn pt_lock_current() -> PtGuard {
    pt_lock(hal::current_asid())
}

/// 映射权限（v1 三档；AP 编码见 `paging.rs`——Rw=00 仅 EL1 读写、Ro=10 只读、
/// UserRw=01 EL0/EL1 读写——2b-4 首个 EL0 用户页。UserRo=11 留 v2 按需）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perm {
    Rw,
    Ro,
    UserRw,
}

impl Perm {
    /// AP 位（OR 进 `page_descriptor`；默认 AP_EL1_RW=0 故 Rw 直接加 0）。
    fn ap(self) -> u64 {
        match self {
            Perm::Rw => 0,
            Perm::Ro => hal::AP_RO,
            Perm::UserRw => hal::AP_USER_RW,
        }
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
    /// unmap/protect 找不到精确匹配的已映射区间。
    NotMapped,
    /// VMO 覆盖的页未全部 commit（v1 无 demand paging，映射前须提交）。
    VmoNotCommitted,
    /// 帧分配器无空闲页（建中间表/提交页）。
    NoFrames,
    /// regions Vec 预留失败（堆 OOM）。allocate/map 的 `regions.push` try 化
    /// （3b3 审查 H2）：push 增长 OOM 曾是 abort 点（EL0 经 vmar_map/allocate
    /// 可触发，违 ABI §8.0）；预留前置到一切副作用之前，失败时零残留。
    NoMem,
}

/// 区间登记项。
struct Region {
    start: usize,
    len: usize,
    kind: RegionKind,
    /// `Mapped`：指向映射的 Vmo（`map` 经 `&mut` 传入）。**指针有效性依托三条
    /// 政策**：handle 表路径 close/replace 对映射中 VMO 拒 -EBUSY（对象不可
    /// 销毁）；drain（进程退出清表）对 refs 将归零的映射中 VMO **整对象泄漏**
    /// 不递减（handle.rs 对抗审查修——无拒面则用泄漏兜住本指针）；boot/ktest
    /// 直构对象由调用方保证生存期（v1 泄漏持有——见 `user.rs`）。
    /// `Reserved` 恒 null。
    vmo: *mut Vmo,
    /// `Mapped`：本区间对应 VMO 内偏移（unmap 回调映射计数用）。
    vmo_off: usize,
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
}

impl Vmar {
    /// 建 root Vmar：出一帧清零作 L1 表根。ASID 由调用方分配（`aspace::alloc_asid`）。
    /// 帧耗尽返 [`VmarError::NoFrames`]——boot/ktest 可信调用方 `.expect`；EL0 路径
    /// （`sys_process_create`）折 -ENOMEM（3b3 审查 H1：expect 曾是 EL0 可触发的
    /// 内核停机点，违 ABI §8.0「永不 panic」）。
    pub fn new_root(fa: &mut FrameAllocator, asid: u16) -> Result<Self, VmarError> {
        let l1_pa = zero_frame(fa).map_err(|_| VmarError::NoFrames)?;
        Ok(Self {
            l1_pa,
            asid,
            base: PAGE_SIZE,
            len: USER_VA_TOP - PAGE_SIZE,
            regions: Vec::new(),
        })
    }

    /// 表根 PA（`Task::ttbr0` / 激活用）。
    pub fn ttbr0_pa(&self) -> usize {
        self.l1_pa
    }

    /// ASID。
    pub fn asid(&self) -> u16 {
        self.asid
    }

    /// 激活本 AS（TTBR0+CONTEXTIDR+按号失效，见 HAL）。
    pub fn activate(&self) {
        hal::activate_aspace(self.l1_pa, self.asid);
    }

    /// 切子 Vmar：预留 `[vaddr, vaddr+len)`（登记进父表防父越子映射），返回共享
    /// 表根/ASID、管辖界收窄的新 Vmar。
    pub fn allocate(&mut self, vaddr: usize, len: usize) -> Result<Vmar, VmarError> {
        self.check_slot(vaddr, len)?;
        // regions 预留先于 push（3b3 H2）：失败点落在纯校验后、零副作用。
        self.regions.try_reserve_exact(1).map_err(|_| VmarError::NoMem)?;
        self.regions.push(Region {
            start: vaddr,
            len,
            kind: RegionKind::Reserved,
            vmo: core::ptr::null_mut(),
            vmo_off: 0,
        });
        Ok(Vmar {
            l1_pa: self.l1_pa,
            asid: self.asid,
            base: vaddr,
            len,
            regions: Vec::new(),
        })
    }

    /// 把 VMO 的 `[vmo_offset, vmo_offset+len)` 映射到 `[vaddr, vaddr+len)`
    /// （`map 即填用户页表`——§5.1 第 4 步主交付）。v1 要求显式 vaddr（ASLR 留 v2）、
    /// 整页粒度、VMO 覆盖页已全部 commit。返回映射基 VA。成功后回调
    /// `vmo.track_map`（3-2 映射计数——decommit/销毁的悬空 PTE 防线），故收
    /// `&mut Vmo`。
    ///
    /// 失败语义：中途建表失败（`NoFrames`）时已写叶全部回滚（清叶 + `dsb ish` +
    /// 按号失效），返回错误后无残留翻译；regions 未登记该区间，同 VA 重试 map
    /// 可干净重建——中途新建的中间表帧被重试复用（`next_table` 见 TABLE_BIT
    /// 即用、不重复分配），即便不复用也只是 v1 已接受的「不回收页表帧」政策
    /// （见模块头）。
    pub fn map(
        &mut self,
        fa: &mut FrameAllocator,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        // per-AS PT 锁（S-4 4-2①）：整方法串行——不止 PTE：同 Vmar 并发调
        // 用的 regions Vec push/check_slot 竞态同窗关死。RAII：中途回滚臂
        // `return Err` 亦放。
        let _pt = pt_lock(self.asid);
        self.check_slot(vaddr, len)?;
        if vmo_offset % PAGE_SIZE != 0 {
            return Err(VmarError::Unaligned);
        }
        // 先查后填：VMO 覆盖页须全部提交（v1 无 demand paging）。
        let npages = len / PAGE_SIZE;
        let first_page = vmo_offset / PAGE_SIZE;
        for i in 0..npages {
            if vmo.page_pa(first_page + i).is_none() {
                return Err(VmarError::VmoNotCommitted);
            }
        }
        // regions 预留前置到一切副作用之前（3b3 H2）：若在收尾 push 时才失败，
        // 叶已填 + track_map 已加——须整链回滚；前置后失败点全在纯校验区。
        self.regions.try_reserve_exact(1).map_err(|_| VmarError::NoMem)?;
        // 填叶（nG=1 内建于 page_descriptor；AP 按权限 OR 上）。filled 计已写
        // 叶数：中途建表失败时 0..filled 页叶已入表而 regions 未登记——残留
        // PTE 是查不到区间的有效翻译（unmap/protect 永远 NotMapped、同 VA
        // 重 map 绕过重叠检测、已激活 AS 即刻可见半填映射），须先回滚再报错。
        // 内存属性按后备分派（P6 A-2）：MMIO 窗口 = Device-nGnRnE + 不共享
        // （内核直映 MMIO 同款，`mmu.rs` 惯例——设备访问不可缓存不可推测，
        // 无一致性与共享语义）；匿名页 = Normal WB + inner-shareable。
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
                Ok(slot) => slot,
                Err(e) => {
                    // 回滚已写叶（这些页的中间表失败前已建好，peek 必得槽位）；
                    // 本路径不分配帧、不可失败，filled==0 时空循环照走屏障无害。
                    // 回滚后 regions 无此区间，同 VA 重试 map 可干净重建（语义
                    // 见上文档注释）。
                    self.clear_leaves(vaddr, filled);
                    // SAFETY: 特权指令；EL1。回滚叶对走查可见 + 旧 TLB 项作废。
                    unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
                    hal::tlbi_aside1(self.asid);
                    // S-4 4-2⑤：他核可能已走查缓存回滚前的半填叶（读者走查
                    // 与本写者靠 PT 锁互斥，但**锁外**的历史缓存项作废只能靠
                    // 广播）。锁窗内只发不待——回执等待在 SVC 派发尾；单核
                    // par_enabled=false 时 TX=0 零扰动（boot/ktest 路径）。
                    hal::tlbshoot::broadcast();
                    return Err(e);
                }
            };
            // SAFETY: 槽位属本表树独占；volatile 防重排。
            unsafe {
                slot.write_volatile(hal::page_descriptor(pa, attr, sh) | perm.ap());
            }
            filled += 1;
        }
        // SAFETY: 特权指令；EL1。表写入对页表走查可见。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        vmo.track_map(vmo_offset, len);
        self.regions.push(Region {
            start: vaddr,
            len,
            kind: RegionKind::Mapped(perm),
            vmo: vmo as *mut Vmo,
            vmo_off: vmo_offset,
        });
        Ok(vaddr)
    }

    /// 按需映射（G4-1）：登记 `RegionKind::Demand` 区间——**不查 commit、不填
    /// 叶、零帧分配**（中间表留待供给时按需建）。VMO 须 demand 后备
    /// （[`crate::vmo::Vmo::is_demand`]——混投 eager VMO 显式拒
    /// [`VmarError::VmoNotCommitted`]，语义 = 「此 VMO 不是按需后备」）。
    /// 成功后**不** track_map（记账起点 = 供给填叶成功时，§5-⑥ 裁决）。
    /// fault 语境的反查经 [`register_demand_root`] 全局登记表（本 AS 有
    /// demand 区间方可被探针命中）。
    ///
    /// 返回映射基 VA。错误集 = 纯校验面（`Unaligned`/`OutOfBounds`/`Overlap`
    /// /`NoMem`）——无副作用中途态，无回滚臂。
    pub fn map_demand(
        &mut self,
        vmo: &mut Vmo,
        vaddr: usize,
        vmo_offset: usize,
        len: usize,
        perm: Perm,
    ) -> Result<usize, VmarError> {
        let _pt = pt_lock(self.asid);
        self.check_slot(vaddr, len)?;
        if vmo_offset % PAGE_SIZE != 0 {
            return Err(VmarError::Unaligned);
        }
        if !vmo.is_demand() {
            return Err(VmarError::VmoNotCommitted);
        }
        self.regions.try_reserve_exact(1).map_err(|_| VmarError::NoMem)?;
        self.regions.push(Region {
            start: vaddr,
            len,
            kind: RegionKind::Demand(perm),
            vmo: vmo as *mut Vmo,
            vmo_off: vmo_offset,
        });
        Ok(vaddr)
    }

    /// 供给一页（G4-1，pager 侧）：`vaddr` 所在 demand 区间内该页 commit +
    /// 填叶 + 屏障失效——此后该 VA 翻译有效，fault 线程 eret 重试即命中。
    /// **幂等**：叶已存在（并发供给/虚醒重供给）直接返回 Ok——重供给不是
    /// 错误，页内容经 VMO 写路径保证一致。
    ///
    /// 错误：VA 不在任何 demand 区间 → `NotMapped`（探针命中后区间被 unmap
    /// 的竞窗 = 供给方拿 `Err` 自理）；建表/出帧失败 → `NoFrames`（pager
    /// fail loud 面）。
    pub fn supply_page(&mut self, fa: &mut FrameAllocator, vaddr: usize) -> Result<(), VmarError> {
        // 页内对齐收口（探针传 FAR 原值时容页中偏移）。
        let va = vaddr & !(PAGE_SIZE - 1);
        let _pt = pt_lock(self.asid);
        let idx = self.demand_index(va).ok_or(VmarError::NotMapped)?;
        let (vmo, vmo_off) = {
            // SAFETY: Region 的 vmo 指针有效性同 map 的两层政策（见 Region 注释）。
            let r = &self.regions[idx];
            (unsafe { &mut *r.vmo }, r.vmo_off)
        };
        // 幂等门：叶已在（前次供给）→ 零动作成功。
        if let Some(slot) = self.l3_slot_peek(va) {
            // SAFETY: 只读表项。
            if unsafe { slot.read_volatile() } & 1 != 0 {
                return Ok(());
            }
        }
        let page_off = vmo_off + (va - self.regions[idx].start);
        // commit 幂等（已提交页跳过）——两值页态复用，见 vmo 模块注释。
        vmo.commit(fa, page_off, PAGE_SIZE).map_err(|e| match e {
            crate::vmo::VmoError::NoFrames => VmarError::NoFrames,
            crate::vmo::VmoError::NoMem => VmarError::NoMem,
            _ => VmarError::NotMapped, // 区间/页参数已上验——不可达兜底
        })?;
        let pa = vmo.page_pa(page_off / PAGE_SIZE).expect("刚 commit 过");
        let slot = self.l3_slot_create(fa, va)?;
        // 供给叶 AP 位取区间登记权限（Demand 构造时定）。
        let ap = match self.regions[idx].kind {
            RegionKind::Demand(p) => p.ap(),
            _ => unreachable!("demand_index 已验 kind"),
        };
        // SAFETY: 槽位属本表树独占；volatile 防重排。匿名 demand 页 = Normal
        // WB + inner-shareable（eager map 同款——demand VMO 无 MMIO 后备）。
        unsafe {
            slot.write_volatile(hal::page_descriptor(pa, hal::ATTR_NORMAL, hal::SH_INNER) | ap);
        }
        // SAFETY: 特权指令；EL1。表写入对页表走查可见。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        hal::tlbi_aside1(self.asid);
        // S-4 4-2⑤：他核历史缓存项作废——锁窗内只发不待（map/unmap 同款）。
        hal::tlbshoot::broadcast();
        // §5-⑥：mapcnt 记账起点 = 供给成功（unmap 的整体 untrack 对未供给
        // 页 saturating_sub 天然零动作——防线对 demand 区间自洽）。
        vmo.track_map(page_off, PAGE_SIZE);
        Ok(())
    }

    /// `va` 所在 demand 区间下标（只读查——fault 探针命中判定 + 供给定位）。
    fn demand_index(&self, va: usize) -> Option<usize> {
        self.regions.iter().position(|r| {
            va >= r.start
                && va < r.start + r.len
                && matches!(r.kind, RegionKind::Demand(_))
        })
    }

    /// fault 探针面（G4-1）：`far` 命中本 AS 的 demand 区间 → `(vmo 指针,
    /// vmo_off, 区间基 VA)`——探针据此进 pager 协议；未命中（含 Mapped/
    /// Reserved/界外）→ `None` = 原杀链不变量保持。只读不锁（探针在异常
    /// 上下文；区间表仅经 PT 锁窗变更——单核屏蔽窗 = 快照一致，SMP 面由
    /// 探针→供给链的 PT 锁窗收口）。
    pub fn demand_region_at(&self, far: usize) -> Option<(*mut Vmo, usize, usize)> {
        let idx = self.demand_index(far)?;
        let r = &self.regions[idx];
        Some((r.vmo, r.vmo_off, r.start))
    }

    /// 解除映射：清 L3 叶（此后访问该 VA 必 fault）+ 按号失效 + 回调 VMO 映射
    /// 计数 -1（此后该页可 decommit/销毁）。**不退物理页**（页属 VMO；v1 也不
    /// 回收中间表帧）。区间须与本 Vmar 登记的映射**精确匹配**（子区间解映射留 v2）。
    pub fn unmap(&mut self, vaddr: usize, len: usize) -> Result<(), VmarError> {
        // per-AS PT 锁（S-4 4-2①），整方法串行（同 map）。
        let _pt = pt_lock(self.asid);
        let idx = self.find_exact(vaddr, len)?;
        // SAFETY: Region 的 vmo 指针经 map 的 &mut 传入；有效性依托两层政策
        // （见 Region 注释：handle 层 close -EBUSY 拒面 + drain 泄漏兜底）。
        // 并发面（S-4 4-2 起 SMP，原「单核 + IRQ 屏蔽」假设退役）：本方法
        // 全程 PT 锁 + KL 双串行——同 AS 他核写者被 PT 锁挡；同 VMO 映两 AS
        // 的跨核 untrack_map 被 KL 挡；对象销毁被 -EBUSY 挡在 PTE 外。
        // demand 区间（G4-1）整体 untrack 安全：track_map 只在供给成功时
        // +1（§5-⑥），未供给页 saturating_sub(0) 零动作——防线自洽。
        unsafe {
            let r = &self.regions[idx];
            (*r.vmo).untrack_map(r.vmo_off, r.len);
        }
        self.clear_leaves(vaddr, len / PAGE_SIZE);
        // SAFETY: 特权指令；EL1。改叶对走查可见 + 旧 TLB 项作废。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        hal::tlbi_aside1(self.asid);
        // S-4 4-2⑤：清叶广播——本核 tlbi 管不到他核缓存项。锁窗内只发不
        // 待（回执等待在 SVC 派发尾零锁自旋）；单核 TX=0 零扰动。
        hal::tlbshoot::broadcast();
        self.regions.swap_remove(idx);
        Ok(())
    }

    /// 改权限：重写叶 AP 位 + 按号失效 + 更新区间登记。精确匹配语义同 unmap。
    pub fn protect(&mut self, vaddr: usize, len: usize, perm: Perm) -> Result<(), VmarError> {
        // per-AS PT 锁（S-4 4-2①），整方法串行（同 map）。
        let _pt = pt_lock(self.asid);
        let idx = self.find_mapped(vaddr, len)?;
        for i in 0..len / PAGE_SIZE {
            let va = vaddr + i * PAGE_SIZE;
            if let Some(slot) = self.l3_slot_peek(va) {
                // SAFETY: 槽位属本表树独占。
                unsafe {
                    let e = slot.read_volatile();
                    slot.write_volatile((e & !AP_MASK) | perm.ap());
                }
            }
        }
        // SAFETY: 特权指令；EL1。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        hal::tlbi_aside1(self.asid);
        // S-4 4-2⑤：权限降级（如 Rw→Ro）后他核 stale 可写项是安全面——广播
        // 作废（同 unmap，锁窗内只发不待）。
        hal::tlbshoot::broadcast();
        self.regions[idx].kind = RegionKind::Mapped(perm);
        Ok(())
    }

    // ---- 内部：区间校验 / 走查 ----

    /// 槽位三查：对齐、管辖界、与既有区间/预留不重叠。
    fn check_slot(&self, vaddr: usize, len: usize) -> Result<(), VmarError> {
        if vaddr % PAGE_SIZE != 0 || len % PAGE_SIZE != 0 || len == 0 {
            return Err(VmarError::Unaligned);
        }
        let end = vaddr.checked_add(len).ok_or(VmarError::OutOfBounds)?;
        if vaddr < self.base || end > self.base + self.len {
            return Err(VmarError::OutOfBounds);
        }
        if self.regions.iter().any(|r| vaddr < r.start + r.len && r.start < end) {
            return Err(VmarError::Overlap);
        }
        Ok(())
    }

    /// 找精确匹配的已登记区间（Mapped 或 Demand——G4-1 起 unmap 对 demand
    /// 区间同收割），返回其下标。protect 仍走 [`Self::find_mapped`]
    /// （Mapped-only——demand 区间 protect v1 拒）。
    fn find_exact(&self, vaddr: usize, len: usize) -> Result<usize, VmarError> {
        self.regions
            .iter()
            .position(|r| {
                r.start == vaddr
                    && r.len == len
                    && matches!(r.kind, RegionKind::Mapped(_) | RegionKind::Demand(_))
            })
            .ok_or(VmarError::NotMapped)
    }

    /// 找精确匹配的已映射区间，返回其下标。
    fn find_mapped(&self, vaddr: usize, len: usize) -> Result<usize, VmarError> {
        self.regions
            .iter()
            .position(|r| {
                r.start == vaddr
                    && r.len == len
                    && matches!(r.kind, RegionKind::Mapped(_))
            })
            .ok_or(VmarError::NotMapped)
    }

    /// 表 PA → 经 direct map 的表基指针。
    fn table_of(pa: usize) -> *mut u64 {
        table_base(pa)
    }

    /// 读槽位；无效则建新表（清零帧）写入，返回下一级表 PA。SMP（S-4 4-2）：
    /// 写者（map）全程持 per-AS PT 锁——跨核双建表竞态已闭，下方「非零非表」
    /// 断言在 SMP 下同样成立（断言留作双保险）。
    fn next_table(fa: &mut FrameAllocator, slot: *mut u64) -> Result<usize, VmarError> {
        // SAFETY: 槽位属本表树独占；volatile 防重排。
        let e = unsafe { slot.read_volatile() };
        if e & TABLE_BIT != 0 {
            Ok((e as usize) & ADDR_MASK as usize)
        } else {
            assert!(e == 0, "Vmar: 中间层出现非表描述符（用户树只可能由本路径写入）");
            let pa = zero_frame(fa).map_err(|_| VmarError::NoFrames)?;
            // SAFETY: 同上。
            unsafe { slot.write_volatile(hal::table_descriptor(pa)) };
            Ok(pa)
        }
    }

    /// 取 `va` 的 L3 槽位指针（缺中间表则按需建）。VA 须在本 AS 管辖内。
    fn l3_slot_create(&self, fa: &mut FrameAllocator, va: usize) -> Result<*mut u64, VmarError> {
        let l2_pa = Self::next_table(fa, unsafe { Self::table_of(self.l1_pa).add((va >> 30) & IDX_MASK) })?;
        let l3_pa = Self::next_table(fa, unsafe { Self::table_of(l2_pa).add((va >> 21) & IDX_MASK) })?;
        Ok(unsafe { Self::table_of(l3_pa).add((va >> 12) & IDX_MASK) })
    }

    /// 只读走查（不建表）：中间层缺失/无效 → `None`。unmap/protect 用。
    fn l3_slot_peek(&self, va: usize) -> Option<*mut u64> {
        let l1 = Self::table_of(self.l1_pa);
        // SAFETY: 只读表项。
        let e0 = unsafe { l1.add((va >> 30) & IDX_MASK).read_volatile() };
        if e0 & TABLE_BIT == 0 {
            return None;
        }
        let l2 = Self::table_of((e0 as usize) & ADDR_MASK as usize);
        // SAFETY: 只读表项。
        let e1 = unsafe { l2.add((va >> 21) & IDX_MASK).read_volatile() };
        if e1 & TABLE_BIT == 0 {
            return None;
        }
        let l3 = Self::table_of((e1 as usize) & ADDR_MASK as usize);
        Some(unsafe { l3.add((va >> 12) & IDX_MASK) })
    }

    /// 清一段区间的 L3 叶（逐槽写 0；不建表、不分配帧，中间表缺失的页静默
    /// 跳过）。unmap 与 map 失败回滚共用；屏障/失效/区间记账由调用方收尾。
    fn clear_leaves(&self, vaddr: usize, npages: usize) {
        for i in 0..npages {
            let va = vaddr + i * PAGE_SIZE;
            if let Some(slot) = self.l3_slot_peek(va) {
                // SAFETY: 槽位属本表树独占。
                unsafe { slot.write_volatile(0) };
            }
        }
    }
}

// ---- asid → root Vmar 反查登记表（G4-1，草案 §5-④ 裁决：全局小表）----
//
// fault 语境（异常处理器）只有硬件现值——TTBR0_EL1 现读 ASID（`hal::
// current_asid`，单一事实源）——到 root Vmar 对象的映射在本表。**为什么
// 不是 task 字段**（§5-④ 备选）：TTBR0 现读已消掉 asid 来源面，登记面
// 只剩「对象指针」一项；全局小表零 sched 改动、boot 直构/handle 表两语境
// 同形（task 字段对 handle 表 Vmar 还要解决装/卸窗）。
//
// 形：16 槽定长线性扫（boot 语境 demand AS 个位数；探针未命中 16 次原子
// 载入 ≈ 数十周期——fault 快路径可受）。**只登记不注销**（v1 boot-held：
// 登记者 = 对象生存期保证人，map_demand 的调用方持有 Vmar 同寿；G4-2
// process 退出语境再裁注销面）。ASID 0 = 空槽哨兵（分配器从 1 起）。

/// 登记槽数（boot/测试语境上界；满 = 后续登记失败 fail loud）。
const DEMAND_ROOT_SLOTS: usize = 16;

static DEMAND_ROOTS: [(AtomicU16, AtomicUsize); DEMAND_ROOT_SLOTS] =
    [const { (AtomicU16::new(0), AtomicUsize::new(0)) }; DEMAND_ROOT_SLOTS];

/// 登记 asid → root Vmar（map_demand 的调用方/对象属主；boot DAIF 屏蔽窗
/// 或 PT 锁窗内调用）。满表 panic——fail loud（静默丢登记 = 探针永不命中
/// = demand 页死循环杀链，宁响雷）。
pub fn register_demand_root(asid: u16, root: *mut Vmar) {
    for (a, p) in DEMAND_ROOTS.iter() {
        if a.load(Ordering::Relaxed) == 0 {
            p.store(root as usize, Ordering::Relaxed);
            a.store(asid, Ordering::Release);
            return;
        }
    }
    panic!("vmar: demand root 登记表满（{} 槽）", DEMAND_ROOT_SLOTS);
}

/// 反查 asid → root Vmar（fault 探针用；未登记 → `None` = 探针不命中）。
pub fn demand_root(asid: u16) -> Option<*mut Vmar> {
    if asid == 0 {
        return None;
    }
    for (a, p) in DEMAND_ROOTS.iter() {
        if a.load(Ordering::Acquire) == asid {
            return Some(p.load(Ordering::Acquire) as *mut Vmar);
        }
    }
    None
}

/// 只读走查任意表树的 L3 叶描述符（3-2：syscall copy 路径的用户缓冲校验——
/// 经**当前 TTBR0**（= 调用者自身表）逐页查 AP 位）。L1/L2 只可能是表描述符
/// （bit1=1；用户树由 `next_table` 独家产出），L3 须页描述符（bit0=1）；任一层
/// 缺失/形态不符 → `None`。AP 位 [7:6] 由调用方解读（00/01=EL1 可写、10=EL1
/// 只读、11=EL0 专属）。
///
/// 读者窗纪律（S-4 4-2，判例 §A113 walk+copy 同窗铁律）：调用方（syscall
/// copy 链）须在持本表树 AS 的 PT 锁窗内（[`pt_lock_current`]）走查并完成
/// store——封死「走查通过后写者撤页」的 TOCTOU 窗。
pub(crate) fn walk_leaf(l1_pa: usize, va: usize) -> Option<u64> {
    /// L3 页描述符有效位（bit0）。
    const PAGE_VALID: u64 = 1;
    let l1 = table_base(l1_pa);
    let e0 = unsafe { l1.add((va >> 30) & IDX_MASK).read_volatile() };
    if e0 & TABLE_BIT == 0 {
        return None;
    }
    let l2 = table_base((e0 as usize) & ADDR_MASK as usize);
    let e1 = unsafe { l2.add((va >> 21) & IDX_MASK).read_volatile() };
    if e1 & TABLE_BIT == 0 {
        return None;
    }
    let l3 = table_base((e1 as usize) & ADDR_MASK as usize);
    let leaf = unsafe { l3.add((va >> 12) & IDX_MASK).read_volatile() };
    (leaf & PAGE_VALID != 0).then_some(leaf)
}

/// 表 PA → 经 direct map 的表基指针（walk_leaf 用——walk 不持 Vmar 对象，
/// 不能走 `Vmar::table_of`）。
fn table_base(pa: usize) -> *mut u64 {
    phys_to_virt(pa) as *mut u64
}
