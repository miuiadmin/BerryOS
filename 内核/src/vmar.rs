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
//! 全 API 返回 `Result`（syscall 边界无 panic 纪律）。

use crate::vmo::{zero_frame, Vmo};
use alloc::vec::Vec;
use berry_hal_aarch64 as hal;
use berry_mm::frame::FrameAllocator;
use berry_mm::va::{phys_to_virt, PAGE_SIZE};

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
    /// `Mapped`：指向映射的 Vmo（`map` 经 `&mut` 传入）。**指针有效性依托两层
    /// 政策**：handle 表路径 close 对映射中 VMO 拒 -EBUSY（对象不可销毁）；
    /// boot/ktest 直构对象由调用方保证生存期（v1 泄漏持有——见 `user.rs`）。
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
                    return Err(e);
                }
            };
            // SAFETY: 槽位属本表树独占；volatile 防重排。
            unsafe {
                slot.write_volatile(
                    hal::page_descriptor(pa, hal::ATTR_NORMAL, hal::SH_INNER) | perm.ap(),
                );
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

    /// 解除映射：清 L3 叶（此后访问该 VA 必 fault）+ 按号失效 + 回调 VMO 映射
    /// 计数 -1（此后该页可 decommit/销毁）。**不退物理页**（页属 VMO；v1 也不
    /// 回收中间表帧）。区间须与本 Vmar 登记的映射**精确匹配**（子区间解映射留 v2）。
    pub fn unmap(&mut self, vaddr: usize, len: usize) -> Result<(), VmarError> {
        let idx = self.find_mapped(vaddr, len)?;
        // SAFETY: Region 的 vmo 指针经 map 的 &mut 传入；两层政策保有效（见
        // Region 注释：handle 层 close 拒绝面 + 直构对象泄漏持有），且单核 +
        // syscall 全程 IRQ 屏蔽（无并发销毁窗口）。
        unsafe {
            let r = &self.regions[idx];
            (*r.vmo).untrack_map(r.vmo_off, r.len);
        }
        self.clear_leaves(vaddr, len / PAGE_SIZE);
        // SAFETY: 特权指令；EL1。改叶对走查可见 + 旧 TLB 项作废。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        hal::tlbi_aside1(self.asid);
        self.regions.swap_remove(idx);
        Ok(())
    }

    /// 改权限：重写叶 AP 位 + 按号失效 + 更新区间登记。精确匹配语义同 unmap。
    pub fn protect(&mut self, vaddr: usize, len: usize, perm: Perm) -> Result<(), VmarError> {
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

    /// 读槽位；无效则建新表（清零帧）写入，返回下一级表 PA。
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

/// 只读走查任意表树的 L3 叶描述符（3-2：syscall copy 路径的用户缓冲校验——
/// 经**当前 TTBR0**（= 调用者自身表）逐页查 AP 位）。L1/L2 只可能是表描述符
/// （bit1=1；用户树由 `next_table` 独家产出），L3 须页描述符（bit0=1）；任一层
/// 缺失/形态不符 → `None`。AP 位 [7:6] 由调用方解读（00/01=EL1 可写、10=EL1
/// 只读、11=EL0 专属）。
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
