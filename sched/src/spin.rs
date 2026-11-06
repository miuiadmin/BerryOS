//! TTAS 自旋锁（S-3c c-2a）：sched 侧结构锁族的底座原语。
//!
//! **零依赖纪律（义务⑦）**：不引 HAL、不经 `KLOCK_OPS`——纯
//! `core::sync::atomic`，可 const 初始化（static / `Runqueue::new` 组合用）。
//! 形态照 `arch/aarch64/src/kern_lock.rs` 同款 TTAS（先 CAS 入主、失败转
//! 廉读自旋——无竞争路径一次原子操作即通过）。
//!
//! **调用方纪律**：①取放须在 IRQ 屏蔽窗内（既有 SLEEPQ/RQ 面契约——雷 2
//! 判例；否则同核 IRQ 重入再取 = 自死锁）；②不可重入（TTAS 非递归——
//! 重叠窗须用 `_locked` 内层分层，不长第二套序列）；③持窗内禁切换（终态
//! 立法——c-2a 起 `block_current` 在切点前释放）。

use core::sync::atomic::{AtomicBool, Ordering};

// S-6 ③b 自旋看门狗（诊断面，定谳后随探针一并退役）：持锁者被杀/泄漏 →
// 取锁者 IRQ 关死自旋（run10 c0 tick 冻结实录）。内层自旋计数超限经函数
// 指针槽自报 lock 地址——本文件零依赖纪律（义务⑦）不破：槽本体纯 core，
// hal 面 bark 由 sched.rs 提供并在 init 自装。None = 零开销直通。
static mut SPIN_BARK: Option<fn(usize)> = None;

/// 安装自旋看门狗 bark（sched::init 自装；boot 单线程恰一次，此后只读）。
pub fn set_spin_bark(f: fn(usize)) {
    // SAFETY: boot 单线程安装（第二核上线前），此后只读。
    unsafe { SPIN_BARK = Some(f) };
}

/// TTAS 自旋锁。`locked == false` = 空闲。
pub struct SpinLock {
    locked: AtomicBool,
}

impl SpinLock {
    /// const 构造（static / `Runqueue::new` 组合用）。
    pub const fn new() -> Self {
        SpinLock {
            locked: AtomicBool::new(false),
        }
    }

    /// 取锁（TTAS：CAS 入主，失败转廉读自旋让出总线——同 `kern_acquire` 形）。
    pub fn acquire(&self) {
        let mut n = 0u32;
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
                // S-6 ③b：内层自旋超限（2^23 次 ≈ 亚秒量级）bark 一次。
                // bark 自带 par 门/节流/总量帽——非 wedge 态零输出零漂移。
                n = n.wrapping_add(1);
                if n >= (1 << 23) {
                    n = 0;
                    // SAFETY: boot 单线程安装后只读。
                    if let Some(bark) = unsafe { SPIN_BARK } {
                        bark(self as *const SpinLock as usize);
                    }
                }
            }
        }
    }

    /// 放锁。
    pub fn release(&self) {
        self.locked.store(false, Ordering::Release);
    }
}
