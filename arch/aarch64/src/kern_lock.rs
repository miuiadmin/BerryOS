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

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// 内核入口大锁：一切 syscall 主体 / IRQ tick 主体 / EL0 fault kill 链的临界区
/// 边界（E0 话术：**内核串行 + 用户并行**——S-3 才可说内核并发）。持锁者
/// **永不睡眠**——睡眠点先放锁（sched 侧按 Task 位舞蹈，草案 §2.3）。
static KERN_LOCK: AtomicBool = AtomicBool::new(false);
/// KERN_LOCK 争用计数（S-3c c-2c 前置微刀）：kern_acquire 外层 CAS 失败体
/// +1（按趟计非按轮计——轮数计数有观察者效应；快路径零新指令）。消费面 =
/// on_irq_tick switches= 金丝雀行尾 ` kl=<n>`（C20 KL 收缩前后对照观测，
/// 非门禁）。Relaxed：纯计数无序语义。
static KL_CONTENDED_CAS: AtomicU64 = AtomicU64::new(0);
/// 打印互斥锁（整条消息粒度）。
static PRINT_LOCK: AtomicBool = AtomicBool::new(false);

/// KERN_LOCK 取锁（TTAS）。
#[inline]
pub fn kern_acquire() {
    while KERN_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        KL_CONTENDED_CAS.fetch_add(1, Ordering::Relaxed);
        while KERN_LOCK.load(Ordering::Relaxed) {
            core::hint::spin_loop();
        }
    }
}

/// 读 KERN_LOCK 争用计数（S-3c c-2c 观测面；非门禁——§4 :101 同款裁决）。
pub fn kl_contention() -> u64 {
    KL_CONTENDED_CAS.load(Ordering::Relaxed)
}

/// KERN_LOCK 放锁。
#[inline]
pub fn kern_release() {
    KERN_LOCK.store(false, Ordering::Release);
}

// ---- SVC 派发包络（草案 3.1-1）：取锁 + 置当前任务持锁义务位 ----
// S-3c c-2c slice 2b 起按类分档（入口位集预取 + 内层窗按位退化）。

/// SVC 入口锁类位集（S-3c c-2c slice 2b）：位值镜像 sched `task.rs` 的
/// HELD_*（bit1=SQ 0x02 / bit2=RQ0 0x04 / bit3=RQ1 0x08 / bit4=REAPQ 0x10 /
/// bit5=TABLE 0x20）；跨 crate 位对齐由 kernel main 编译期断言执法。
/// 分类审计（34 号修正版落类表）= `docs/SMP-S3实施设计草案.md`；NONE 类号
/// （默认槽/越界号——实装号零）**完全不取任何锁**；凡含 copy_out/copy_in
/// 出参链的号（walk_leaf 页表读者）或任何共享面禁落 NONE（§A113 读者面）。
pub const SVC_CLASS_NONE: u8 = 0x00;
/// 纯表号（0x02/0x03/0x3A/0x61/0x86/0x87）：仅 KL。
pub const SVC_CLASS_TABLE: u8 = 0x20;
/// wait/send 族（0x01/0x04/0x05/0x06/0x52/0x53）：SQ+RQ+表。
pub const SVC_CLASS_SQ_RQ_TABLE: u8 = 0x2E;
/// thread_sleep 单号（0x37）：SQ+RQ 双核升序、**无表无 KL**（slice 3-1a
/// 降档——`nearest_deadline` KL-only 读面收口 SQ 后，SLEEPQ 全触面纯 SQ
/// 串行，本号 handler 纯睡眠面零表触）。死锁安全（§A112 两态并存禁不
/// 触发）：全域结构物理取用恒升序前缀（本类入口/舞蹈取步恒持完整
/// SQ+RQ0+RQ1 三元组，KL 类同序取结构），KL 单持有者且一切 KL 取用点
/// 不持结构——「入口持 RQ 旋 KL」边不存在。
pub const SVC_CLASS_SQ_RQ: u8 = 0x0E;
/// start 双号（0x32/0x35）：RQ 双核升序 + 表。
pub const SVC_CLASS_RQ_TABLE: u8 = 0x2C;
/// 全谱（0x33/0x36/0x3B + vmo/vmar 0x11..0x1B + create 族 0x31/0x34/0x51/0x54）。
pub const SVC_CLASS_ALL: u8 = 0x3E;

/// 当前任务位 置/清 槽（kernel main 装 `berry_sched::klock_flag_current`；
/// HAL 不依赖 sched——`set_user_fault_exit` 同款反转先例）。None = 未装窗
///（boot 最早路径）——锁仍直取直放，位跳过。
static mut KLOCK_FLAG: Option<unsafe extern "C" fn(bool)> = None;

/// SVC 入口**结构锁窗**槽（slice 2b）：kernel main 装
/// `berry_sched::svc_window` 对（(class, true)=取 / (class, false)=放）。
/// HAL 侧只管 KL——类含 TABLE 位则先取 KL（**过渡态 KL-first 入口序**：
/// 与现役 IRQ 面（tick/sgi/park_recheck/idle 等 klock_acquire_raw 先取）
/// 保持单向全局序 KL 最外，§A108 不变式延续；终态翻正序（结构先行）=
/// slice 3 与 IRQ 面 KL 剥离原子同刀，两态不可并存）。结构位集的物理
/// 取/放 + held 记账 + `svc_mask` 声明全经槽交 sched 侧。None = 未装窗
///（boot 最早路径）——KL 仍直取直放（物理配平恒成立），结构窗跳过
///（handler 内层窗照旧自取，今日形态）。
static mut SVC_WINDOW: Option<unsafe extern "C" fn(u8, bool)> = None;

/// 注册当前任务位回调（kernel main，ktest 前恰一次）。
pub fn set_klock_flag_hook(f: unsafe extern "C" fn(bool)) {
    // SAFETY: boot 单线程注册；此后只读。
    unsafe { KLOCK_FLAG = Some(f) };
}

/// 注册 SVC 结构锁窗回调（kernel main，ktest 前恰一次；slice 2b）。
pub fn set_svc_window_hook(f: unsafe extern "C" fn(u8, bool)) {
    // SAFETY: boot 单线程注册；此后只读。
    unsafe { SVC_WINDOW = Some(f) };
}

/// SVC 入口按类包络（slice 2b 真行为变更点）：类含 TABLE 位 → `kern_acquire`
/// **先取**（KL-first 过渡序）；再经槽让 sched 侧结构升序物理取 + 全类位
/// 记账 + `svc_mask=bits`——handler 内层窗（sq_lock/rq_lock 等）由此全退化
/// （位已置 → 跳物理取放，位归本对收口）。类不含 TABLE 位（NONE 类）= 零锁
/// 零槽直达 handler。槽未装（boot 窗）= 只取 KL 直进（今日形态）。
/// `-> !` handler（0x33/0x36）不归——放锁在 sched.rs `switch_away_forever_with`
/// 舞蹈收口，出口对对其不可达。
pub fn svc_enter_class(class: u8) {
    if class & SVC_CLASS_TABLE != 0 {
        kern_acquire();
    }
    if let Some(f) = unsafe { SVC_WINDOW } {
        // SAFETY: 槽在 boot 单线程期注册；f 是 C-ABI fn 指针。
        unsafe { f(class, true) };
    }
}

/// SVC 出口按类包络：槽侧结构降序物理放 + 断言 held==class 清账 → KL **最后**
/// 放（KL-last 与入口 KL-first 对称——全局序 KL 最外不变）。`regs[0]` 栈写不
/// 触共享结构。仅 dispatch 正常返可达。
pub fn svc_exit_class(class: u8) {
    if let Some(f) = unsafe { SVC_WINDOW } {
        // SAFETY: 同上。
        unsafe { f(class, false) };
    }
    if class & SVC_CLASS_TABLE != 0 {
        kern_release();
    }
}

/// SVC 入口包络（全量形）：EL0 fault kill 链专用（`trap.rs` 非 0x15 臂）——
/// dump + 杀链可触任何结构 ⇒ 五锁全取、全位记账、mask=全谱。等价历史
/// `svc_enter`（BKL 纪元）＋ slice 2b 结构位集；收口同为舞蹈按位放。
pub fn svc_enter() {
    svc_enter_class(SVC_CLASS_ALL);
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
