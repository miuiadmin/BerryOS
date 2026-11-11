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
//! - **irqsave 纪律（G43，G3-1c-1 console 写域解蔽改版）**：任务态经
//!   [`with_print_lock`] 取锁 = **preempt 计数包络**（enter 槽 +1 / exit 槽
//!   配平+检查点——槽反转接线，HAL 不知 scheduler 之名）：持 PRINT_LOCK 期
//!   IRQ 不再定向屏蔽（调用方入口本在屏蔽窗则维持原样——包络不触 DAIF），
//!   本核 tick/SGI 仍可入但抢占支按 preempt_count 闭门不切（sched 侧门）；
//!   同核硬死锁的旧解（irqsave）由新解（禁切）承接——tick 的 switches=
//!   金丝雀行要同一把锁，持锁被切走 = 持锁者对自旋于己核 IRQ 语境不可让，
//!   两种包络同效。sched 侧槽对（`print_lock`/`print_unlock`）与 arch 侧裸
//!   `print_lock` **不带**包络——IRQ 语境裸调用面两处（on_irq_tick 金丝雀行 /
//!   trap 未知 IRQ 诊断臂），**门纪律（§A114 追记二，m5 立法）**：IRQ 语境裸取
//!   PRINT_LOCK 前必先过 preempt_count 门——count>0 = 被打断者可能在窗内，
//!   让位不取（同核自旋等不可运行者 = 自死锁）。tick 金丝雀行天然在门后
//!   （抢占支 go 分支闭门早退），trap 诊断臂自带同款门（errata m1）。

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// 内核入口大锁：一切 syscall 主体 / IRQ tick 主体 / EL0 fault kill 链的临界区
/// 边界（E0 话术：**内核串行 + 用户并行**——S-3 才可说内核并发）。持锁者
/// **永不睡眠**——睡眠点先放锁（sched 侧按 Task 位舞蹈，草案 §2.3）。
static KERN_LOCK: AtomicBool = AtomicBool::new(false);
/// KERN_LOCK 争用计数（S-3c c-2c 前置微刀）：kern_acquire 外层 CAS 失败体
/// +1（按趟计非按轮计——轮数计数有观察者效应；快路径零新指令）。消费面 =
/// on_irq_tick switches= 金丝雀行尾 ` kl=<n>`（C20 KL 收缩前后对照观测，
/// 非门禁）。Relaxed：纯计数无序语义。
static KL_CONTENDED_CAS: AtomicU64 = AtomicU64::new(0);
/// KERN_LOCK 持有者镜像（3-1b recompute TABLE 子窗位感知判据）：0 = 无人
/// 持，n+1 = 核 n 持。**为什么任务位不够**：ktest/boot 窗 CURRENT=null 位
/// 记账不存在，但 SVC 主体 HAL 直取 KL 的物理持有态与位无关
/// （sys_handle_close 直调 recompute 的重入面）——判据必须问锁本身。
/// 维护序：acquire CAS 成功即置；release **先清再放**（先放后清 = 别核
/// 立刻 acquire 置新值、被本核迟到的清写覆盖 = 记账丢失；先清 = 窗口期
/// 「holder=0 而 LOCK=true」只对本核可见，本核此刻不会再读）。kl_held
/// 读点恒在 IRQ 屏蔽窗（recompute 发射点全在锁窗/屏蔽窗）⇒ 无撕裂窗口。
static KL_HELD: AtomicUsize = AtomicUsize::new(0);
/// S-6 ③b：KERN_LOCK 持有起算 cntvct（看门狗年龄算子；诊断面，定谳后
/// 随探针退役）。维护序同 KL_HELD——acquire 成功即写。
static KL_HELD_SINCE: AtomicU64 = AtomicU64::new(0);
/// S-6 ③b 持者点名（run11 定谳：held_by=CPU0 + age=0s 连刷 = 快速循环
/// 取放活锁——差最后一环「CPU0 上是谁在刷」）：本核当前任务名钩子，sched
/// init 反向注入（arch 不依赖 sched，KlockOps 同款反转先例）。返回
/// (名指针, 名长)；(0,0) = park 态/防御窗无当前任务。名 = 任务字面量
/// &'static [u8]，跨核读不悬垂。
static mut NAME_HOOK: Option<fn() -> (usize, usize)> = None;
/// 持者名镜像（acquire 成功即写，与 KL_HELD 同窗）。
static KL_HELD_TAG_PTR: AtomicUsize = AtomicUsize::new(0);
static KL_HELD_TAG_LEN: AtomicUsize = AtomicUsize::new(0);

/// 安装本核任务名钩子（sched::init，boot 单线程恰一次，此后只读）。
pub fn set_kl_name_hook(f: fn() -> (usize, usize)) {
    // SAFETY: boot 单线程安装（第二核上线前）。
    unsafe { NAME_HOOK = Some(f) };
}
/// 打印互斥锁（整条消息粒度）。
static PRINT_LOCK: AtomicBool = AtomicBool::new(false);

// ---- S-6 ③b 自旋看门狗（arch 双锁自报；诊断面，定谳后随探针退役）----

/// bark 节流锚（上次 bark 的 cntvct）。
static BARK_LAST: AtomicU64 = AtomicU64::new(0);
/// bark 总量帽记账。
static BARK_CNT: AtomicU64 = AtomicU64::new(0);

/// 自旋看门狗 bark（与 sched 侧 spin.rs 槽同款纪律）：par 门（单核零
/// 漂移）+ 1s 全局节流 + 32 行总量帽，**原始 UART 直写不吃 PRINT_LOCK**
/// （泄漏锁可能就是它）。`held_by`：KERN_LOCK 面 = KL_HELD 持者核号
/// （0 = 无人持/已清）；PRINT_LOCK 面传 0。
fn lock_bark(what: &[u8], held_by: usize, since: u64) {
    if !crate::smp::par_enabled() {
        return;
    }
    let now = crate::timer::cntvct();
    if now.saturating_sub(BARK_LAST.load(Ordering::Relaxed)) < crate::timer::cntfrq() {
        return;
    }
    if BARK_CNT.fetch_add(1, Ordering::Relaxed) >= 32 {
        return;
    }
    BARK_LAST.store(now, Ordering::Relaxed);
    crate::uart::uart_puts(b"spin! ");
    crate::uart::uart_puts(what);
    crate::uart::uart_puts(b" cpu=");
    crate::uart::uart_put_dec(crate::smp::cpu_id() as u64);
    crate::uart::uart_puts(b" held_by=");
    crate::uart::uart_put_dec(held_by as u64);
    if held_by != 0 {
        crate::uart::uart_puts(b" task='");
        let (p, l) = (
            KL_HELD_TAG_PTR.load(Ordering::Relaxed),
            KL_HELD_TAG_LEN.load(Ordering::Relaxed),
        );
        if p != 0 && l != 0 && l <= 32 {
            // SAFETY: sched 侧钩子契约——任务名 &'static 字面量，不悬垂。
            let name: &[u8] = unsafe { core::slice::from_raw_parts(p as *const u8, l) };
            crate::uart::uart_puts(name);
        }
        crate::uart::uart_puts(b"' age=");
        // ms 分辨率（run11 的 s 级显示全落 0s——快速取放必须看亚秒）。
        crate::uart::uart_put_dec(now.saturating_sub(since) * 1000 / crate::timer::cntfrq());
        crate::uart::uart_puts(b"ms");
    }
    crate::uart::uart_puts(b"\n");
}

/// KERN_LOCK 取锁（TTAS）。
#[inline]
pub fn kern_acquire() {
    // G3-3 lockdep：物理取点记账（feature 关 = 编译消除；类 = TABLE——七类
    // 全序位 4，KL 类内末取立法）。
    #[cfg(feature = "lockdep")]
    crate::lockdep::acquire(crate::lockdep::CLASS_TABLE);
    // S-6 ③b：内层自旋超限（2^23 次）看门狗自报（非 wedge 态计数远够不到
    // ——争用突发 µs 量级；零漂移）。
    let mut bark_n = 0u32;
    while KERN_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        KL_CONTENDED_CAS.fetch_add(1, Ordering::Relaxed);
        while KERN_LOCK.load(Ordering::Relaxed) {
            core::hint::spin_loop();
            bark_n = bark_n.wrapping_add(1);
            if bark_n >= (1 << 23) {
                bark_n = 0;
                lock_bark(
                    b"kern",
                    KL_HELD.load(Ordering::Relaxed),
                    KL_HELD_SINCE.load(Ordering::Relaxed),
                );
            }
        }
    }
    KL_HELD.store(crate::smp::cpu_id() + 1, Ordering::Relaxed);
    KL_HELD_SINCE.store(crate::timer::cntvct(), Ordering::Relaxed);
    // S-6 ③b：持者点名镜像（钩子未装窗 = boot 最早路径，(0,0) 空过）。
    // SAFETY: boot 安装后只读。
    if let Some(h) = unsafe { NAME_HOOK } {
        let (p, l) = h();
        KL_HELD_TAG_PTR.store(p, Ordering::Relaxed);
        KL_HELD_TAG_LEN.store(l, Ordering::Relaxed);
    }
}

/// 读 KERN_LOCK 争用计数（S-3c c-2c 观测面；非门禁——§4 :101 同款裁决）。
pub fn kl_contention() -> u64 {
    KL_CONTENDED_CAS.load(Ordering::Relaxed)
}

/// **本核**是否已持 KERN_LOCK（3-1b recompute TABLE 子窗位感知退化判据）：
/// 已持 → 子窗跳物理取放（防同核重入自旋死锁——合法重入面 = SVC TABLE
/// 类主体内经 close/block 发射 PI 重推）。未持核调 kern_acquire = 正常
/// 跨核互斥自旋（持有者终放，非死锁）。
pub fn kl_held() -> bool {
    KL_HELD.load(Ordering::Relaxed) == crate::smp::cpu_id() + 1
}

/// KERN_LOCK 放锁。
#[inline]
pub fn kern_release() {
    #[cfg(feature = "lockdep")]
    crate::lockdep::release(crate::lockdep::CLASS_TABLE);
    KL_HELD.store(0, Ordering::Relaxed); // 先清镜像再放锁（见 KL_HELD 注）
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
/// 串行，本号 handler 纯睡眠面零表触）。死锁安全（终态序 slice 3-1b 后 +
/// S-4 判例 klock_enter 收编）：全域结构物理取用恒升序前缀（本类入口/舞蹈
/// 取步恒持完整 SQ+RQ0+RQ1 三元组；KL 取用点（svc TABLE 类号/recompute
/// TABLE 子窗/klock_enter 全谱窗）一律结构先取 KL 后取）——「入口持 RQ
/// 旋 KL」「持 KL 旋结构」双边皆不存在。
pub const SVC_CLASS_SQ_RQ: u8 = 0x0E;
/// start 双号（0x32/0x35）：RQ 双核升序 + 表。
pub const SVC_CLASS_RQ_TABLE: u8 = 0x2C;
/// 全谱（0x33/0x36/0x3B + vmo/vmar 0x11..0x1B + create 族 0x31/0x34/0x51/0x54）。
pub const SVC_CLASS_ALL: u8 = 0x3E;

/// SVC 入口**结构锁窗**槽（slice 2b）：kernel main 装
/// `berry_sched::svc_window` 对（(class, true)=取 / (class, false)=放）。
/// HAL 侧只管 KL——类含 TABLE 位则**类内末取**（slice 3-1b 终态正序：
/// 结构先行、KL 最内——七层冻结序 SQ>队列>REAPQ>表；与舞蹈取步/recompute
/// TABLE 子窗全域单向一致，跨核 ABBA 不可能；槽体只管结构位集不触 KL）。
/// 结构位集的物理
/// 取/放 + held 记账 + `svc_mask` 声明全经槽交 sched 侧。None = 未装窗
///（boot 最早路径）——KL 仍直取直放（物理配平恒成立），结构窗跳过
///（handler 内层窗照旧自取，今日形态）。
static mut SVC_WINDOW: Option<unsafe extern "C" fn(u8, bool)> = None;

/// 注册 SVC 结构锁窗回调（kernel main，ktest 前恰一次；slice 2b）。
pub fn set_svc_window_hook(f: unsafe extern "C" fn(u8, bool)) {
    // SAFETY: boot 单线程注册；此后只读。
    unsafe { SVC_WINDOW = Some(f) };
}

/// SVC 入口按类包络（slice 2b 真行为变更点；slice 3-1b 翻**终态正序**）：
/// 先经槽让 sched 侧结构升序物理取（SQ→RQ0→RQ1→REAPQ）+ 全类位记账 +
/// `svc_mask=bits`——handler 内层窗（sq_lock/rq_lock 等）由此全退化（位已
/// 置 → 跳物理取放，位归本对收口）；类含 TABLE 位 → `kern_acquire` **类内
/// 末取**（终态全局锁序 KL 最内：七层冻结序 SQ>队列>REAPQ>表——与舞蹈取步
/// /recompute TABLE 子窗同序，跨核 ABBA 不可能）。类不含 TABLE 位（NONE
/// 类）= 零锁零槽直达 handler。槽未装（boot 窗）= 只取 KL 直进（今日形态）。
/// `-> !` handler（0x33/0x36）不归——放锁在 sched.rs `switch_away_forever_with`
/// 舞蹈收口，出口对对其不可达。
pub fn svc_enter_class(class: u8) {
    if let Some(f) = unsafe { SVC_WINDOW } {
        // SAFETY: 槽在 boot 单线程期注册；f 是 C-ABI fn 指针。
        unsafe { f(class, true) };
    }
    if class & SVC_CLASS_TABLE != 0 {
        kern_acquire();
    }
}

/// SVC 出口按类包络（slice 3-1b 翻终态对称）：KL **先放**（最内先放）→ 槽侧
/// 结构降序物理放 + 断言 held==class 清账。取放严格逆序（取 = 结构先行 KL
/// 末取 / 放 = KL 先行结构后放）——与舞蹈放步同形。`regs[0]` 栈写不触共享
/// 结构。仅 dispatch 正常返可达。
pub fn svc_exit_class(class: u8) {
    if class & SVC_CLASS_TABLE != 0 {
        kern_release();
    }
    if let Some(f) = unsafe { SVC_WINDOW } {
        // SAFETY: 同上。
        unsafe { f(class, false) };
    }
}

/// SVC 入口包络（全量形）：EL0 fault kill 链专用（`trap.rs` 非 0x15 臂）——
/// dump + 杀链可触任何结构 ⇒ 五锁全取、全位记账、mask=全谱。等价历史
/// `svc_enter`（BKL 纪元）＋ slice 2b 结构位集；收口同为舞蹈按位放。
pub fn svc_enter() {
    svc_enter_class(SVC_CLASS_ALL);
}

// ---- 内核任务直调包络（草案 §2.2/§2.3；F31-F36/F39-F42 改造点）----

/// 内核任务直调临界区入口：= 屏蔽 IRQ + 结构先行全谱包络 + KL 类内末取，一次
/// 完成（草案 §2.2）。F 位点原形态均为任务语境 `disable_irq()` 无条件对
/// （入口 IRQ 解蔽态）。睡眠点（`block_current` 等）在窗内由 sched 舞蹈按位
/// 放锁/重取——舞蹈分诊对 ALL 位集通用（KL 臂 = kern_acquire/release 本体，
/// KL_HELD 随切走清/切回置）。
///
/// **S-4 判例（直调族取锁序与 SVC 族不同序 = 潜在跨核 AB-BA 边，2026-11-03
/// 全谱化收编）**：历史形态 = KL 先取 + 只置 TABLE 位（KLOCK_FLAG 槽），窗体
/// （wake_matching/block_current/for_each_sleeper）物理取 SQ = 「持 KL 等 SQ」
/// 违序边；对核 KL-free SQ 窗（tick expire / rx）内 recompute 又「持 SQ 等
/// KL」⇒ 结构性跨核互锁边（理论边——**未捕获实尸**：同日三轮 lldb 尸检的
/// ai panic 定谳为 wait deadline 单位错配（雷册 §A118 修订版），双核现场均
/// 在自由推进，本边未触发）。工程裁决：边真实存在且修法干净——改道
/// [`svc_enter_class`] 全谱窗（EL0 kill 链 `svc_enter` 同形）：结构先行 KL
/// 最内，窗内结构锁全退化 ⇒ KL 持有者恒无等待、一切 KL 等待有限，防御性
/// 收编不挂尸检名。boot 窗（CURRENT 空）= 槽跳过仅取 KL——与历史形态等价。
///
/// **次序铁则**：`disable_irq` 必须先于一切取锁——反例 = 取锁后被本核 tick
/// 打断，tick 语境的结构锁窗自旋于本核持有的锁（IRQ 上下文不能让出 CPU）
/// = 同核硬死锁（雷 2 判例）。
pub fn klock_enter() {
    crate::disable_irq();
    svc_enter_class(SVC_CLASS_ALL);
}

/// 对称出口：KL 先放（最内先放）→ 结构降序物理放 + 断言 held==ALL 清账 →
/// 解蔽。`enable_irq` 必须最后——放锁前解蔽则 tick 可在持锁期闯入（同款
/// 死锁推理，见 [`klock_enter`]）。
pub fn klock_exit() {
    svc_exit_class(SVC_CLASS_ALL);
    crate::enable_irq();
}

// ---- PRINT_LOCK ----

/// PRINT_LOCK 取锁（裸对——**不含 preempt 包络**：IRQ 语境调用者（tick go 支
/// 金丝雀行 + trap 未知 IRQ 诊断臂）各带 preempt_count 门——count>0 = 本核有
/// 禁切窗（被打断者可能正持 PRINT_LOCK），让位不取；任务态一律走
/// [`with_print_lock`]）。
pub fn print_lock() {
    // G3-3 lockdep：类 = PRINT（rank 7 最内）——「持 PRINT_LOCK 者禁取任何
    // 锁」由此自动执法（其后任何取锁 = rank 降 = violation）。
    #[cfg(feature = "lockdep")]
    crate::lockdep::acquire(crate::lockdep::CLASS_PRINT);
    // S-6 ③b：看门狗自报（同 kern_acquire；PRINT 面无持者镜像——自报即定位）。
    let mut bark_n = 0u32;
    while PRINT_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        while PRINT_LOCK.load(Ordering::Relaxed) {
            core::hint::spin_loop();
            bark_n = bark_n.wrapping_add(1);
            if bark_n >= (1 << 23) {
                bark_n = 0;
                lock_bark(b"print", 0, 0);
            }
        }
    }
}

/// PRINT_LOCK 放锁。
pub fn print_unlock() {
    #[cfg(feature = "lockdep")]
    crate::lockdep::release(crate::lockdep::CLASS_PRINT);
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

/// 读全宽 DAIF（p1 哨兵专用：包络契约「不触 DAIF」= enter/exit 全值相等，
/// 只看 bit 7 盖不住 D/A/F 位被改的形态）。
#[inline]
fn daif_raw() -> u64 {
    let daif: u64;
    // SAFETY: 同 [`irq_enabled`]——只读系统寄存器，无内存副作用；EL1。
    unsafe { core::arch::asm!("mrs {}, daif", out(reg) daif, options(nostack, preserves_flags)) };
    daif
}

// ---- G3-1c-1（console 写域解蔽）：PRINT_LOCK preempt 包络槽（HAL→sched
// 反转，避循环依赖——同 SVC_EXIT_CHECKPOINT 模式，HAL 永不知 scheduler 之名）----
//
// with_print_lock 的窗包络从 irqsave 换 preempt 计数：enter = preempt_disable
//（+1 计数）；exit = 配平 + 条件检查点（屏蔽窗调用者跳过——详 sched 侧
// print_window_exit 注）。槽空（boot 注册前）= 平凡直过：该窗 IRQ 恒屏蔽
//（单线程 boot），零行为差。
static mut PREEMPT_ENVELOPE: Option<(fn(), fn())> = None;

/// 注册 PRINT_LOCK 窗 preempt 包络（kernel boot 在 enable_irq 前调用一次）。
pub fn set_preempt_envelope(enter: fn(), exit: fn()) {
    // SAFETY: boot 单线程（IRQ 未开）注册，此后全系统只读。
    unsafe { PREEMPT_ENVELOPE = Some((enter, exit)) };
}

#[inline]
fn preempt_enter() {
    // SAFETY: 只读快照；set_preempt_envelope 全在 boot 单线程期。
    if let Some((e, _)) = unsafe { PREEMPT_ENVELOPE } {
        e();
    }
}

#[inline]
fn preempt_exit() {
    // SAFETY: 同上。
    if let Some((_, x)) = unsafe { PREEMPT_ENVELOPE } {
        x();
    }
}

/// 拼行 API（G44）：整条消息在 PRINT_LOCK 内一次拼完（多段 `uart_puts` 原子）。
/// 窗包络 = preempt 计数对（G43 改版，G3-1c-1 console 写域解蔽）：持锁期 IRQ
/// 不再定向屏蔽——入口解蔽者（任务态打印，console 域正主）保持解蔽，tick/
/// SGI 可入不可切（抢占支按计数闭门）；入口屏蔽者（klock 窗/SVC 主体/boot/
/// ktest——包络不触 DAIF）维持屏蔽，exit 槽按 DAIF 态跳检查点（sched 侧分诊
/// ——持 KL/结构锁的 SVC 主体窗内切换违七类锁序，切换诉求由 g3-1b 落点①
/// 派发尾检查点在零锁点承接）。锁语义不变：跨核互斥仍靠 TTAS 自旋；
/// 「持 PRINT_LOCK 者禁取任何锁」单向序不动。panic 路径 bypass 直写（模块头）。
pub fn with_print_lock<R>(f: impl FnOnce() -> R) -> R {
    // p1 哨兵（errata 批）：包络契约「不触 DAIF」机器化——f() 内改本核
    // DAIF = 分诊前提失效（exit 槽按出口态选检查点支，enter/exit 态不齐
    // = 走错支），debug_assert fail loud；release 编译消除。
    #[cfg(debug_assertions)]
    let daif0 = daif_raw();
    preempt_enter();
    print_lock();
    let r = f();
    print_unlock();
    preempt_exit();
    #[cfg(debug_assertions)]
    debug_assert_eq!(daif_raw(), daif0, "with_print_lock: f() 内改了 DAIF（包络契约 = 不触 DAIF，分诊前提失效）");
    r
}
