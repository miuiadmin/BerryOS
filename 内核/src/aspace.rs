//! 最小用户地址空间（2b-2）：三级页表帧 + 一页数据 + ASID。
//!
//! 「帧分配器出页 → 填表 → TTBR0 激活」的地基（`内存管理设计.md` §5.1 第 3 步）；
//! 2b-3 的 VMO/Vmar 接管映射管理后，本模块的填表部分由正式映射路径取代，
//! ASID 记账与激活约定不变。

use bos_hal_aarch64 as hal;
use bos_mm::asid::AsidAllocator;
use bos_mm::frame::FrameAllocator;
use bos_mm::va::{phys_to_virt, PAGE_SHIFT, PAGE_SIZE};

/// 测试/任务私有页的低半 VA（页 1——页 0 留作 NULL 解引用保护区，2b-4 起对用户禁 0 页）。
pub const USER_VA: usize = 0x1000;

/// 全局 ASID 分配器（单核顺序使用；回卷时全量失效）。宽度首用时探测注入。
static mut ASIDS: Option<AsidAllocator> = None;

/// 取一个新 ASID（首调用惰性初始化；`just` 关卡输出里打印过实测宽度）。
pub fn alloc_asid() -> u16 {
    // SAFETY: 单核顺序访问（ktest / rust_main spawn 段，IRQ 屏蔽）。
    unsafe {
        let slot = &raw mut ASIDS;
        if (*slot).is_none() {
            *slot = Some(AsidAllocator::new(hal::asid_bits()));
        }
        let (asid, rolled) = (*slot).as_mut().unwrap().alloc();
        if rolled {
            hal::flush_all_asid();
        }
        asid
    }
}

/// 从帧分配器取一页并清零，返回 **PA**（`FrameAllocator::alloc` 给的是 PFN，这里
/// 左移折算；帧内容不保证干净——buddy 只管元数据，前手可能写过哨兵）。
fn zero_page(fa: &mut FrameAllocator) -> usize {
    let pfn = fa.alloc().expect("MiniAspace: 无空闲帧");
    let pa = pfn << PAGE_SHIFT;
    // SAFETY: 独占新分配帧（尚无别名引用）；Normal WB，同核写后 dsb 可见。
    unsafe { core::ptr::write_bytes(phys_to_virt(pa) as *mut u8, 0, PAGE_SIZE) };
    pa
}

/// 一个最小地址空间：L1[0]→L2[0]→L3[1]→数据页（VA `0x1000`，Normal RW，nG=1）。
pub struct MiniAspace {
    /// L1 表 PA——`hal::activate_aspace` / `Task::ttbr0` 用的就是它。
    pub ttbr0_pa: usize,
    /// 本空间 ASID。
    pub asid: u16,
    /// 所有权记账（v1 不归还；下划线前缀 = 只在 Drop-less 所有权语义里被持有）。
    _l2_pa: usize,
    _l3_pa: usize,
    _page_pa: usize,
}

impl MiniAspace {
    /// 建表并在数据页首写 `magic`（任务体/测试用来自证「我在自己的空间里」）。
    pub fn new(fa: &mut FrameAllocator, asid: u16, magic: u64) -> Self {
        let l1_pa = zero_page(fa);
        let l2_pa = zero_page(fa);
        let l3_pa = zero_page(fa);
        let page_pa = zero_page(fa);

        // 填表：描述符输出 PA；经高半直映写表内容。
        // SAFETY: 独占帧；volatile 防重排，dsb 保证对页表走查可见。
        unsafe {
            let l1 = phys_to_virt(l1_pa) as *mut u64;
            let l2 = phys_to_virt(l2_pa) as *mut u64;
            let l3 = phys_to_virt(l3_pa) as *mut u64;
            l1.add(0).write_volatile(hal::table_descriptor(l2_pa));
            l2.add(0).write_volatile(hal::table_descriptor(l3_pa));
            l3.add(1).write_volatile(hal::page_descriptor(page_pa, hal::ATTR_NORMAL, hal::SH_INNER));
            (phys_to_virt(page_pa) as *mut u64).write_volatile(magic);
            core::arch::asm!("dsb ish", options(nostack, preserves_flags));
        }

        Self { ttbr0_pa: l1_pa, asid, _l2_pa: l2_pa, _l3_pa: l3_pa, _page_pa: page_pa }
    }

    /// 激活本空间（TTBR0+CONTEXTIDR+`tlbi aside1is`，见 HAL）。
    pub fn activate(&self) {
        hal::activate_aspace(self.ttbr0_pa, self.asid);
    }

    /// 经**当前 TTBR0** 读 `USER_VA`（证翻译——须已激活本空间或等价表）。
    pub fn read_va(&self) -> u64 {
        // SAFETY: 只读一页 Normal 映射（未映射则同步异常——测试语境即断言失败信号）。
        unsafe { (USER_VA as *const u64).read_volatile() }
    }

    /// 经当前 TTBR0 写 `USER_VA`。
    pub fn write_va(&self, v: u64) {
        // SAFETY: 写自身映射页（同上）。
        unsafe { (USER_VA as *mut u64).write_volatile(v) };
    }
}
