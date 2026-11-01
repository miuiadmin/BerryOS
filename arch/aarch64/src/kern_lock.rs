//! S-2 E0（BKL 纪元）：KERN_LOCK / PRINT_LOCK —— TTAS 自旋锁对。
//! 设计单一事实源 = `docs/SMP-S2实施设计草案.md` §2.1/§2.2。
//!
//! - **TTAS**（test-and-test-and-set）：争用时只本地 `load(Relaxed)` + `spin_loop()`
//!   探测释放、释放时一次 Acquire CAS 拿锁——等释放期间不碰共享行（TAS 的 LL/SC
//!   失败重试风暴、TCG 原子慢路径都绕开）。**wfe 否决**：TCG `wfe` = 立即返回的
//!   hint（S-1 实测副核忙转拖主核 ~40% 吞吐——雷册 §A99），E0 纯自旋；
//!   长自旋 = E0 违约信号（草案 §6 R1 观测面）。
//! - **屏障**：`Acquire`/`Release` 映射 LDAXR/STLXR，临界区与锁外访问的序由序
//!   语义保证——不手写 dmb（重复发明）；dsb 唯一保留场景 = S-1 门铃序（smp.rs），
//!   本锁不经邮箱传递。cortex-a72 无 LSE（ARMv8.0——outline-atomics 先例）→
//!   `compare_exchange` 走 LL/SC 循环，正确。
//! - **const-init**：`static` + 常量初值，无 init 调用——boot 最早路径（含
//!   ktest/bench 窗）即可用；单核非竞争路径 = 每次 SVC 一对原子操作纯开销。
//! - **PRINT_LOCK**：独立实例，粒度 = 整条消息（多段 uart 拼行原子）；嵌套单向
//!   KERN→PRINT（持 PRINT_LOCK 者禁取任何锁）；**panic bypass**——panic 路径
//!   直写 UART 绕锁（停机面前锁无意义，也免 panic-in-lock 递归死锁）。
//! - **irqsave 纪律（G43）**：任务态经 [`with_print_lock`] 取锁 = 先存 DAIF.I +
//!   屏蔽、放后恢复——裸持 PRINT_LOCK 被本核 tick 打断（tick 的 switches= 金丝雀
//!   行要同一把锁）= 同核硬死锁。sched 侧槽对（`print_lock`/`print_unlock`）
//!   **不带** irqsave——唯一调用点 on_irq_tick 恒在 IRQ 屏蔽上下文（PSTATE.I=1）。

use core::sync::atomic::{AtomicBool, Ordering};

/// 内核入口大锁：一切 syscall 主体 / IRQ tick 主体 / EL0 fault kill 链的临界区
/// 边界（E0 话术：**内核串行 + 用户并行**——S-3 才可说内核并发）。持锁者
/// **永不睡眠**——睡眠点先放锁（sched 侧按 Task 位舞蹈，草案 §2.3）。
static KERN_LOCK: AtomicBool = AtomicBool::new(false);
/// 打印互斥锁（整条消息粒度）。
static PRINT_LOCK: AtomicBool = AtomicBool::new(false);

/// KERN_LOCK 取锁（TTAS）。
#[inline]
pub fn kern_acquire() {
    while KERN_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        while KERN_LOCK.load(Ordering::Relaxed) {
            core::hint::spin_loop();
        }
    }
}

/// KERN_LOCK 放锁。
#[inline]
pub fn kern_release() {
    KERN_LOCK.store(false, Ordering::Release);
}

// ---- SVC 派发包络（草案 3.1-1）：取锁 + 置当前任务持锁义务位 ----

/// 当前任务位 置/清 槽（kernel main 装 `berry_sched::klock_flag_current`；
/// HAL 不依赖 sched——`set_user_fault_exit` 同款反转先例）。None = 未装窗
///（boot 最早路径）——锁仍直取直放，位跳过。
static mut KLOCK_FLAG: Option<unsafe extern "C" fn(bool)> = None;

/// 注册当前任务位回调（kernel main，ktest 前恰一次）。
pub fn set_klock_flag_hook(f: unsafe extern "C" fn(bool)) {
    // SAFETY: boot 单线程注册；此后只读。
    unsafe { KLOCK_FLAG = Some(f) };
}

/// SVC 入口包络：EL0/EL1 两向量入口在 `trap.rs` 的 EC 0x15 臂单点汇流于
/// `syscall::dispatch`，本对覆盖一切 syscall。取锁直调（本 crate 原语）；
/// 位置位经槽——CURRENT 空（ktest/boot 窗）跳过：锁由本对恒配平，位只服务
/// sched 切换舞蹈。
pub fn svc_enter() {
    kern_acquire();
    if let Some(f) = unsafe { KLOCK_FLAG } {
        // SAFETY: 槽在 boot 单线程期注册；f 是 C-ABI fn 指针。
        unsafe { f(true) };
    }
}

/// SVC 出口包络：清位 + 放锁（`regs[0]` 栈写不触共享结构——放锁前后皆可，
/// 草案 2.2）。`-> !` handler（0x33/0x36）不归——放锁在 sched.rs
/// `switch_away_forever_with` 收口（草案 3.2），本尾对其不可达（死码勿补）。
pub fn svc_exit() {
    if let Some(f) = unsafe { KLOCK_FLAG } {
        // SAFETY: 同上。
        unsafe { f(false) };
    }
    kern_release();
}

// ---- 内核任务直调包络（草案 §2.2/§2.3；F31-F36/F39-F42 改造点）----

/// 内核任务直调临界区入口：= 屏蔽 IRQ + 取锁 + 置当前任务持锁义务位，一次
/// 完成（草案 §2.2）。F 位点原形态均为任务语境 `disable_irq()` 无条件对
/// （入口 IRQ 解蔽态）——本对语义等价替换，睡眠点（`block_current` 等）在窗内
/// 由 sched 舞蹈自动放锁/重取（位=1 分诊）。
///
/// **次序铁则**：`disable_irq` 必须先于 `kern_acquire`——反例 = 先取锁后被
/// 本核 tick 打断，`on_irq_tick` 的 `klock_acquire_raw` 自旋于本核持有的锁
/// （IRQ 上下文不能让出 CPU）= 同核硬死锁。
pub fn klock_enter() {
    crate::disable_irq();
    kern_acquire();
    if let Some(f) = unsafe { KLOCK_FLAG } {
        // SAFETY: 槽在 boot 单线程期注册；f 是 C-ABI fn 指针。
        unsafe { f(true) };
    }
}

/// 对称出口：清位 → 放锁 → 解蔽。`enable_irq` 必须最后——release 前解蔽则
/// tick 可在持锁期闯入（同款死锁推理，见 [`klock_enter`]）。
pub fn klock_exit() {
    if let Some(f) = unsafe { KLOCK_FLAG } {
        // SAFETY: 同上。
        unsafe { f(false) };
    }
    kern_release();
    crate::enable_irq();
}

// ---- PRINT_LOCK ----

/// PRINT_LOCK 取锁（裸对——调用方须已在 IRQ 屏蔽窗；任务态走 [`with_print_lock`]）。
pub fn print_lock() {
    while PRINT_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        while PRINT_LOCK.load(Ordering::Relaxed) {
            core::hint::spin_loop();
        }
    }
}

/// PRINT_LOCK 放锁。
pub fn print_unlock() {
    PRINT_LOCK.store(false, Ordering::Release);
}

/// 读 DAIF.I（bit 7）：true = IRQ 解蔽。（S-3a probe ③ 观测位跨 crate 复用。）
#[inline]
pub fn irq_enabled() -> bool {
    let daif: u64;
    // SAFETY: 只读系统寄存器；无内存副作用；EL1。
    unsafe { core::arch::asm!("mrs {}, daif", out(reg) daif, options(nostack, preserves_flags)) };
    daif & (1 << 7) == 0
}

/// 拼行 API（G44）：整条消息在 PRINT_LOCK 内一次拼完（多段 `uart_puts` 原子），
/// 含 irqsave 包络（G43）——取锁前存 DAIF.I 并屏蔽、放锁后恢复。任务态打印
/// 一律经此；panic 路径 bypass 直写（见模块头）。
pub fn with_print_lock<R>(f: impl FnOnce() -> R) -> R {
    let had_irq = irq_enabled();
    crate::disable_irq();
    print_lock();
    let r = f();
    print_unlock();
    if had_irq {
        crate::enable_irq();
    }
    r
}
