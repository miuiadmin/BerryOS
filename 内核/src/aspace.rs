//! 每任务用户地址空间的组装层（2b-2 引入；2b-3 起填表改走 **Vmar/Vmo 正式映射
//! 路径**，手工 L2/L3 填表退役）。ASID 记账与激活约定不变（`内存管理设计.md`
//! §5.1/§5.2）。

use crate::vmar::{Perm, Vmar};
use crate::vmo::Vmo;
use bos_mm::asid::AsidAllocator;
use bos_mm::frame::FrameAllocator;
use bos_mm::va::PAGE_SIZE;

/// 测试/任务私有页的低半 VA（页 1——Vmar root 管辖界 `[0x1000, USER_VA_TOP)`，
/// 页 0 留作 NULL 解引用保护区，2b-4 起对用户禁 0 页）。
pub const USER_VA: usize = 0x1000;

/// 全局 ASID 分配器（单核顺序使用；回卷时全量失效）。宽度首用时探测注入。
static mut ASIDS: Option<AsidAllocator> = None;

/// 取一个新 ASID（首调用惰性初始化；`just` 关卡输出里打印过实测宽度）。
pub fn alloc_asid() -> u16 {
    // SAFETY: 单核顺序访问（ktest / rust_main spawn 段，IRQ 屏蔽）。
    unsafe {
        let slot = &raw mut ASIDS;
        if (*slot).is_none() {
            *slot = Some(AsidAllocator::new(bos_hal_aarch64::asid_bits()));
        }
        let (asid, rolled) = (*slot).as_mut().unwrap().alloc();
        if rolled {
            bos_hal_aarch64::flush_all_asid();
        }
        asid
    }
}

/// 一个最小任务地址空间：root Vmar + 一页 VMO 映在 `USER_VA`（Normal RW，nG=1）。
///
/// 2b-3 起内部为「`Vmar::new_root` + `Vmo::create/commit/write` + `map`」——
/// 即 2b-4 进程组装的同一条路径；`ttbr0_pa`/`asid`/`activate` 对外不变。
pub struct MiniAspace {
    /// L1 表 PA——`hal::activate_aspace` / `Task::ttbr0` 用的就是它。
    pub ttbr0_pa: usize,
    /// 本空间 ASID。
    pub asid: u16,
    /// 表树属主（`ttbr0_pa` 即其根）。
    vmar: Vmar,
    /// 私有页后备所有权（页归 VMO；v1 对象不销毁故无读取点——查询接口留 2b-4+）。
    _vmo: Vmo,
}

impl MiniAspace {
    /// 建空间并在私有页首写 `magic`（任务体/测试用来自证「我在自己的空间里」）。
    pub fn new(fa: &mut FrameAllocator, asid: u16, magic: u64) -> Self {
        let mut vmar = Vmar::new_root(fa, asid).expect("MiniAspace: 建 root Vmar（boot 可信语境）");
        let mut vmo = Vmo::create(PAGE_SIZE).expect("MiniAspace: 装 VMO（boot 可信语境）");
        vmo.commit(fa, 0, PAGE_SIZE).expect("MiniAspace: commit 私有页");
        vmo.write(0, &magic.to_le_bytes()).expect("MiniAspace: 写 magic");
        vmar.map(fa, &mut vmo, USER_VA, 0, PAGE_SIZE, Perm::Rw)
            .expect("MiniAspace: map 私有页");
        Self { ttbr0_pa: vmar.ttbr0_pa(), asid: vmar.asid(), vmar, _vmo: vmo }
    }

    /// 激活本空间（TTBR0+CONTEXTIDR+`tlbi aside1is`，见 HAL）。
    pub fn activate(&self) {
        self.vmar.activate();
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
