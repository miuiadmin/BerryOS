//! 每任务用户地址空间的组装层（2b-2 引入；2b-3 起填表改走 **Vmar/Vmo 正式映射
//! 路径**，手工 L2/L3 填表退役）。ASID 记账与激活约定不变（`内存管理设计.md`
//! §5.1/§5.2）。

use crate::vmar::{Perm, Vmar};
use crate::vmo::Vmo;
use berry_mm::asid::AsidAllocator;
use berry_mm::frame::FrameAllocator;
use berry_mm::va::PAGE_SIZE;
use core::sync::atomic::{AtomicBool, Ordering};

/// 测试/任务私有页的低半 VA（页 1——Vmar root 管辖界 `[0x1000, USER_VA_TOP)`，
/// 页 0 留作 NULL 解引用保护区，2b-4 起对用户禁 0 页）。
pub const USER_VA: usize = 0x1000;

/// 全局 ASID 分配器（S-4 4-3 起双核并发——`sys_process_create` 于任一核
/// SVC 窗调用，计数器推进由 [`ASID_LOCK`] 互斥）。宽度首用时探测注入。
static mut ASIDS: Option<AsidAllocator> = None;

/// ASID 分配器 TTAS 自旋锁（vmar `pt_lock` 同款形态）。锁序第六层位：
/// KL/分配器之下、打印之上（`SMP-S4实施设计草案.md` §3.2）——持 KL 窗内
/// 取本锁 = 正序嵌套。持锁窗只护计数器推进（取号 + 回卷代数 bump），无
/// 等待、无打印、无分配。无环：SVC 窗持有者 IRQ 屏蔽有界；内核任务窗
///（asidroll 测试）持有者可抢占但纯计数器操作即毕，等待者旋即得锁。
static ASID_LOCK: AtomicBool = AtomicBool::new(false);

/// 取 ASID 锁（TTAS：CAS 外环防写行争、Relaxed 内环自旋降流量）。
fn asid_lock() {
    // G3-3 lockdep（g3-3c）：物理取点记账——集中本函数覆盖两取点
    // （alloc_asid + asid_force_near_roll）；feature 关 = 编译消除。
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

/// 取一个新 ASID（首调用惰性初始化；`just` 关卡输出里打印过实测宽度）。
/// S-4 4-3：计数器推进由 [`ASID_LOCK`] 互斥（原「单核顺序访问」假设随
/// `sys_process_create` 双核化失效）；回卷 quiesce 恒在**锁外**——锁内
/// 取号 + 回卷代数 bump（代数序 = 取号序），rolled 即放锁，锁外
/// [`roll_quiesce_arm`]（本核全量失效 + 广播置闩，**不等待**——SVC 路等待
/// 归 dispatch 尾，内核任务路显式驱动）。单核（boot 前期 / ktest）零争用
/// 直过，arm 短路为仅本地失效（与 4-3 前行为零漂移）。
pub fn alloc_asid() -> u16 {
    asid_lock();
    // SAFETY: ASID_LOCK 临界区（互斥）；惰性初始化同临界区内完成。boot
    // DAIF 窗 / SVC 屏蔽窗 / 测试任务窗均可达，持锁窗有界（纯计数器操作）。
    let (asid, rolled) = unsafe {
        let slot = &raw mut ASIDS;
        if (*slot).is_none() {
            *slot = Some(AsidAllocator::new(berry_hal_aarch64::asid_bits()));
        }
        let (asid, rolled) = (*slot).as_mut().unwrap().alloc();
        if rolled {
            berry_hal_aarch64::tlbshoot::roll_gen_bump();
        }
        (asid, rolled)
    };
    // G3-3 lockdep（g3-3c）：物理放点（一）；同款见 asid_force_near_roll。
    #[cfg(feature = "lockdep")]
    berry_hal_aarch64::lockdep::release(berry_hal_aarch64::lockdep::CLASS_ASID);
    ASID_LOCK.store(false, Ordering::Release);
    if rolled {
        // 锁外武装：本核全量失效 + 广播置闩（本函数绝不等待）。
        berry_hal_aarch64::tlbshoot::roll_quiesce_arm();
    }
    asid
}

/// 测试注入（fw_cfg `opt/berry/asidroll` 门内专用——smoke-smp 相 4）：
/// 计数器快进到回卷前，下次 [`alloc_asid`] 即真回卷。生产路径零调用。
pub fn asid_force_near_roll() {
    asid_lock();
    // SAFETY: ASID_LOCK 临界区（同 alloc_asid——互斥 + 惰性初始化同窗）。
    unsafe {
        let slot = &raw mut ASIDS;
        if (*slot).is_none() {
            *slot = Some(AsidAllocator::new(berry_hal_aarch64::asid_bits()));
        }
        (*slot).as_mut().unwrap().force_near_roll();
    }
    // G3-3 lockdep（g3-3c）：物理放点（二）；同款见 alloc_asid。
    #[cfg(feature = "lockdep")]
    berry_hal_aarch64::lockdep::release(berry_hal_aarch64::lockdep::CLASS_ASID);
    ASID_LOCK.store(false, Ordering::Release);
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
