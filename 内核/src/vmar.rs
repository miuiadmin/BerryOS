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
use bos_hal_aarch64 as hal;
use bos_mm::frame::FrameAllocator;
use bos_mm::va::{phys_to_virt, PAGE_SIZE};

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
}

/// 区间登记项。
struct Region {
    start: usize,
    len: usize,
    kind: RegionKind,
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
    pub fn new_root(fa: &mut FrameAllocator, asid: u16) -> Self {
        let l1_pa = zero_frame(fa).expect("Vmar root: 无空闲帧");
        Self {
            l1_pa,
            asid,
            base: PAGE_SIZE,
            len: USER_VA_TOP - PAGE_SIZE,
            regions: Vec::new(),
        }
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
        self.regions.push(Region { start: vaddr, len, kind: RegionKind::Reserved });
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
    /// 整页粒度、VMO 覆盖页已全部 commit。返回映射基 VA。
    pub fn map(
        &mut self,
        fa: &mut FrameAllocator,
        vmo: &Vmo,
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
        // 填叶（nG=1 内建于 page_descriptor；AP 按权限 OR 上）。
        for i in 0..npages {
            let pa = vmo.page_pa(first_page + i).expect("上已校验");
            let va = vaddr + i * PAGE_SIZE;
            let slot = self.l3_slot_create(fa, va)?;
            // SAFETY: 槽位属本表树独占；volatile 防重排。
            unsafe {
                slot.write_volatile(
                    hal::page_descriptor(pa, hal::ATTR_NORMAL, hal::SH_INNER) | perm.ap(),
                );
            }
        }
        // SAFETY: 特权指令；EL1。表写入对页表走查可见。
        unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
        self.regions.push(Region { start: vaddr, len, kind: RegionKind::Mapped(perm) });
        Ok(vaddr)
    }

    /// 解除映射：清 L3 叶（此后访问该 VA 必 fault）+ 按号失效。**不退物理页**
    /// （页属 VMO；v1 也不回收中间表帧——Phase 3 对象销毁统一处理）。
    /// 区间须与本 Vmar 登记的映射**精确匹配**（子区间解映射留 v2）。
    pub fn unmap(&mut self, vaddr: usize, len: usize) -> Result<(), VmarError> {
        let idx = self.find_mapped(vaddr, len)?;
        for i in 0..len / PAGE_SIZE {
            let va = vaddr + i * PAGE_SIZE;
            if let Some(slot) = self.l3_slot_peek(va) {
                // SAFETY: 槽位属本表树独占。
                unsafe { slot.write_volatile(0) };
            }
        }
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
        phys_to_virt(pa) as *mut u64
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
}
