//! 协作调度器核心（plan Phase 2a Step 3）。
//!
//! 单核、抢占点（G0 协作式）：`yield_now` / `exit_current` 是唯一切换点，**无 timer
//! 驱动**（Step 4/6 起）。`current` 指向当前任务；`IDLE_TASK` 是 boot 上下文
//! （`rust_main` 的活栈——Bug 4 懒捕获，首切出时由 `switch` 把活 callee-saved+sp
//! 存进它的 ctx）。
//!
//! 承载不变式（plan Bug 2）：`switch` 永不在 `PSTATE.I=0`（中断解蔽）时到达——
//! `yield_now`/`exit_current` 均先 `disable_irq` 再切。

use crate::class::{Fair, SchedClass};
use crate::runqueue::{Runqueue, Topo};
use crate::spin::SpinLock;
use crate::task::{
    Task, TaskState, WakeReason, HELD_RQ0, HELD_RQ1, HELD_REAPQ, HELD_SQ, HELD_TABLE,
};
use berry_hal_aarch64 as hal;
use core::cell::Cell;
use core::ptr::{addr_of_mut, NonNull};
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// IRQ tick 计数（`on_irq_tick` 自增）。金丝雀：长跑中持续增长 = timer/EOIR 管线健康
/// （rearm 对、无 EOIR 饿死——plan R4/R5）；停增即管线坏。
/// S-2 E0 前置件 A12：原子化（写点全在 KERN_LOCK 锁窗，读侧锁外——Relaxed 无
/// 序角色、仅计数；同时执法 static mut 别名 UB）。
static IRQ_TICKS: AtomicU64 = AtomicU64::new(0);
/// 上下文切换计数（六切换原语位共享递增——[`bump_switches`]）。里程碑金丝雀：
/// 两任务在 timer slice 下分时 → 计数持续涨（≥数千 = G1 过关）；冻结在某值 =
/// switch 坏（卡死/饿死）。A12 同上原子化。
static SWITCHES: AtomicU64 = AtomicU64::new(0);
/// S-6 ③ 心跳诊断：上一行打点时的双核 ticks_pre 总数（跨 500 节流——
/// [`on_irq_tick`] 心跳行用）。诊断面，定谳后随探针一并退役。
static HB_LAST: AtomicU64 = AtomicU64::new(0);
/// S-6 ③ 卡死等待者扫描：上一轮扫描时的双核 ticks_pre 总数（跨 5000
/// 节流——[`on_irq_tick`] stuck 行用）。诊断面，定谳后随探针一并退役。
static STUCK_LAST: AtomicU64 = AtomicU64::new(0);
/// S-6 ② 拉取金丝雀行真序号（run5 曾误用 diag_readout().2——那是 CPU1
/// 派发计数非拉取数，行号全标 #1）。仅诊断打印用。
static PULL_CNT: AtomicU64 = AtomicU64::new(0);

/// SWITCHES 递增 + 金丝雀打印（S-6 ①：全域唯一递增/打印口径）。
/// 全部十一个切换原语位（yield/exit/block 三函数的直切臂 + park 切点 + tick
/// 抢占 + SGI resched 直切 + try_preempt 直切 + park/idle 两派发位）共享本
/// helper：%100 判定走**全局**计数——哪个位跨点哪个位打印。
/// 沿革九十四发现①收口（两层）：打印若只挂 tick 抢占支（旧单点）SMP 稳态
/// 忙对各占核时结构性冷；更深的洞 = **直切臂（出选有货即切）旧完全不计数**
/// ——park 臂冷、直切臂热，漏计 = 金丝雀对 SMP 稳态主干结构性失明（S-6
/// 复跑 0/53K 切换字符实证）。A12「SWITCHES 计入金丝雀」至此全量兑现。
/// 返回 true = 本次跨过打印点（调用方可据此做**锁窗已证面**的附带走查）。
///
/// SAFETY 契约（print_lock_raw 要求）：调用点 IRQ 屏蔽窗内——六位皆然
/// （切换原语锁窗/屏蔽窗，见各位注）。只取 PRINT_LOCK（与任务态
/// with_print_lock 同锁——G43 拼行互斥保持），不触 RQ/SQ/REAPQ 结构锁；
/// 任务持 PRINT_LOCK 期不可达切换原语（with_print_lock 为作用域闭包、
/// 窗内禁切）⇒ 无自死锁面。
fn bump_switches() -> bool {
    let sw = SWITCHES.fetch_add(1, Ordering::Relaxed) + 1;
    // bring-up 诊断（G1）：每 N 次切换打一行计数，证分时持续、计数不冻。
    const SOAK_PRINT_EVERY: u64 = 100;
    if sw % SOAK_PRINT_EVERY == 0 {
        // S-2 E0 打印面（草案 3.1-3）：金丝雀三段在 PRINT_LOCK 内拼行原子
        //（IRQ 屏蔽窗内调用——print 对不带 irqsave，纪律见 kern_lock 模块头）；
        // sw 快照供整除判定与打印同值。
        print_lock_raw();
        hal::uart_puts(b"switches=");
        hal::uart_put_dec(sw);
        // S-3c c-2c 前置微刀：行尾拼 KL 争用计数（C20 收缩前后对照观测，
        // 非门禁）。kl_contention 经 hal 直调——with_print_lock 同款先例
        //（ops 槽只服务取放对，读数无语境约束）。
        hal::uart_puts(b" kl=");
        hal::uart_put_dec(hal::kern_lock::kl_contention());
        hal::uart_puts(b"\n");
        print_unlock_raw();
        return true;
    }
    false
}

/// 当前在 CPU 上跑的任务。S-2b 起为**回落位**：主位 = 本核 `SmpCpu.current`
///（TPIDR_EL1 寻址，经 [`current_task`]/[`set_current`] 存取）；boot 起 =
/// `&IDLE_TASK`（由 [`init`] 设）。
static mut CURRENT: *mut Task = core::ptr::null_mut();

/// CURRENT per-CPU 读（S-2b §2.3）：本核 `SmpCpu.current` 槽 Some → volatile
/// 读主位；None（TPIDR_EL1 未装——ktest/`cpu0_init` 前防御窗）→ 回落本 static
///（与 `last_vct_slot` 同款契约，单核语义逐位对齐）。副核 park 态 = null
/// 空哨兵（空态契约：消费点 = `on_irq_tick` 判空走 park 派发 / SVC 面
/// `svc_window` 等 null 跳过——副核 park 期无任务即无 SVC）。
#[inline]
unsafe fn current_task() -> *mut Task {
    match hal::smp::current_slot() {
        // SAFETY: 槽非零即本核钉入的 SmpCpu.current 字段地址（本核独占写，
        // 切换原语锁窗内单写者；读点全在锁窗/屏蔽窗）。
        Some(p) => p.read_volatile() as *mut Task,
        None => CURRENT,
    }
}

/// CURRENT per-CPU 写（`current_task` 的写侧同款契约）。
#[inline]
unsafe fn set_current(t: *mut Task) {
    match hal::smp::current_slot() {
        Some(p) => p.write_volatile(t as u64),
        None => CURRENT = t,
    }
}

// ---- G3-1a（P-A 抢占点加密基座，草案 §8.A.3/§8.C）：per-CPU preempt_count ----
//
// 语义：>0 = 本核禁切窗（「计数 > 0 窗内禁切换」不变量——等价今日 IRQ 屏蔽窗
// 的「非零即禁」，把第三窗（SVC 主体无锁段/长循环段）纳入同一执法面）。g3-1a
// 为零行为变更步：本刀只落计数基座 + 切换原语入口配平断言，无人 inc ⇒ 恒 0、
// 断言恒过；真消费者 = g3-1b 检查点族（探峰前自查）与 g3-1c 域解蔽（拷贝分块
// preempt_disable）。

/// PREEMPT_COUNT 回落位（TPIDR_EL1 未装窗 = ktest/cpu0_init 前防御——与
/// `CURRENT` 回落同款契约；该窗只可能在 CPU0，单核语义正确）。
static PREEMPT_COUNT: AtomicU64 = AtomicU64::new(0);

/// 本核 preempt_count 槽（主位 = `SmpCpu.preempt_count`；None 回落本 static）。
#[inline]
fn preempt_slot() -> &'static AtomicU64 {
    hal::smp::preempt_count_slot().unwrap_or(&PREEMPT_COUNT)
}

/// 开本核禁切窗（可嵌套；须与 [`preempt_enable`] 严格配平）。窗内禁切换——
/// 切换原语入口配平断言（debug_assert count==0）执法。
///
/// 原子 RMW（f4 m3 修复批）：g3-1c 起禁切窗在 IRQ 解蔽态开合——非原子
/// read-modify-write 会被 tick 打断在读/写之间（tick 见 count 仍 0 判可切 =
/// 撕裂窗漏切）；fetch_add 单指令序，tick 要么见自增前要么见自增后。Relaxed
/// 口径 = 本核顺序即够（per-CPU 字段，跨核零序角色；执法点同核 IRQ 屏蔽读）。
pub fn preempt_disable() {
    preempt_slot().fetch_add(1, Ordering::Relaxed);
}

/// 关本核禁切窗一层（嵌套弹一层；归零点 = 可切点）。下溢 = 配平违约，
/// debug 构建 fail-loud（fetch_sub 前值判——sub 已落地才判，违约时计数翻
/// u64::MAX = 永禁切方向（fail-safe：宁可卡死调度不静默漏禁），dev 面
/// panic 先停机）。
pub fn preempt_enable() {
    let prev = preempt_slot().fetch_sub(1, Ordering::Relaxed);
    debug_assert!(prev > 0, "preempt_enable: count 下溢（disable/enable 配平违约）");
}

/// 读本核禁切计数（诊断/检查点探峰自查面——g3-1b 消费）。
pub fn preempt_count() -> u64 {
    preempt_slot().load(Ordering::Relaxed)
}

/// 切换原语入口配平断言（G3-1a 执法面）：进切换原语 = 本核必须可切
///（count==0）。主面 `just build` = dev profile ⇒ debug_assert 活执法
///（2026-11-04 勘正；ktest preempt API 位（计数语义）双面执法）。
#[inline]
fn assert_preempt_balanced(where_: &str) {
    debug_assert_eq!(preempt_count(), 0, "切换原语 {}：preempt_count 非零（禁切窗内禁切换）", where_);
}

/// G3-1b：显式抢占检查点（草案 §8.C 落点族）——带 irqsave 包络的
///「计数==0 探峰 + 可能直切」。计数 >0（禁切窗）直返；否则 try_preempt
/// 语义（探峰集 == 出选集，幻影出选防线内置）。切换原语全员要求 IRQ
/// 屏蔽（调用方契约，草案 §8 交织铁则）⇒ 探峰+直切窗内局部重屏蔽：
/// 存 DAIF → 屏蔽 → 探/切 → 恢复（直切发生时，恢复点 = 被切任务日后续跑
/// 至此，其保存 DAIF 即其检查点前态；被切走期间新任务走自身上下文的
/// DAIF 形态，不继承本窗）。
/// 零行为变更步（g3-1b）：现役 SVC 全程 IRQ 屏蔽 ⇒ 单核 rq 状态窗内
/// 不可变 ⇒ 探峰恒空 = 纯空探（无直切）；g3-1c 域解蔽后本检查点真承重。
///
/// g3-1d 观测面：真探形（count==0 支）命中计数——递增点在 try_preempt
/// 之前 = **rq_lock 窗外**，无锁原子加（Relaxed 单指令序），不引入任何
/// 锁序面（f4 g3-1d 预警①）；空探（计数>0 直返）不计。
static PREEMPT_CKPT_HITS: AtomicU64 = AtomicU64::new(0);

pub fn preempt_checkpoint() {
    let daif = hal::irq_save();
    hal::disable_irq();
    if preempt_count() == 0 {
        PREEMPT_CKPT_HITS.fetch_add(1, Ordering::Relaxed);
        try_preempt();
    }
    hal::irq_restore(daif);
}

/// g3-1d：检查点真探形命中数（boot 起累计；fa ktest 尾 `ckpt=` 字段打印）。
pub fn preempt_checkpoint_hits() -> u64 {
    PREEMPT_CKPT_HITS.load(Ordering::Relaxed)
}

/// G3-1c-1：PRINT_LOCK 窗 preempt 包络对（with_print_lock 的 enter/exit 槽
/// 接线——kernel boot 经 set_preempt_envelope 注册）。
///
/// enter = [`preempt_disable`]（+1 计数；**不触 DAIF**——入口屏蔽者维持屏蔽）。
///
/// exit = 配平 + **按 DAIF 态分诊的检查点**：出口 IRQ 解蔽（= 入口解蔽——
/// 包络与 f() 均不改 DAIF，IRQ 往返保调用方态）⇒ 任务态打印（console 域
/// 正主）：enable 归零后立即探峰——禁切期被双门（tick/SGI 抢占支）挡下的
/// 切换诉求在出锁边界兑现。兑现分档（errata m2 勘正）：严格更高 eff 者
/// 检查点直切 = µs 级上界；同 eff 轮转诉求 try_preempt 不切（严格更高才
/// 切），落回下个 tick 重探（rearm-at-entry 名义整片 100ms、TCG 实效
/// ≈200ms）。出口屏蔽（= 入口屏蔽：klock 窗 / SVC 主体 / boot / ktest）
/// ⇒ **跳检查点**：窗内 IRQ 屏蔽 ⇒ 无 tick/SGI ⇒ 无新增切换诉求；
/// handler 自身唤醒的诉求由 g3-1b 落点①
/// （SVC 派发尾检查点，svc_exit_class 毕 = 零锁持有）安全承接——此处若探
/// = 持 KL/结构锁窗内切换，pick 取 RQ 锁 = 七类锁序（队列>表）逆序违约。
pub fn print_window_enter() {
    preempt_disable();
}

/// 退出半（文档见 [`print_window_enter`]——DAIF 分诊论证在彼）。
pub fn print_window_exit() {
    preempt_enable();
    if hal::kern_lock::irq_enabled() {
        preempt_checkpoint();
    }
}

/// 本核 pick 过滤器（S-2b §2.3「内核串行/用户并行」）：CPU0 全量视图（与旧
/// `pick_next` 单核语义恒等）；副核（cpu_id != 0）`user_only`——只出选
/// `pinned != 0` 的用户任务（svcmgr `process_start` 派生的 EL0 进程；内核任务
/// 与 boot 裸表进程全钉 CPU0）。`my_cur` = 本核当前任务（Running-elsewhere
/// 自豁免指针；park 派发态传 null——park 上下文非调度语义 Task，永不入环）。
/// per-CPU runqueue（S-3b 步 b-1，子案 §9.6 / §8.C A4）：定长数组按核分诊，
/// 索引 = 核号（v1 = 2）。锁界 = KERN_LOCK 全量窗（S-3b 事实——SVC 入口仍
/// 全量锁，一切入/出环手术跨核互斥）；A4 侦察形的 per-queue lock 字段曾因
/// 「sched 架构无关不引 arch 锁类型」被否决——c-2a 已以 crate 内自持
/// `SpinLock`（`runqueue.rs` 字段落形，零 arch 依赖）兑现「届时再扩」，
/// c-2b 起取放启用。单核形态（gate 恒关）`cpu_id()` 恒 0 →
/// `[0]` 行为等价旧全局 RUNQUEUE（零漂移根基）。
static mut PER_CPU_RQ: [Runqueue; 2] = [Runqueue::new(), Runqueue::new()];
/// CPU 拓扑（capacity 由 [`init`] 从 HAL `cpu_capacity()` 填——S-3b 真化；
/// 单核默认 `[1024, 0]` 同构占位，QEMU virt 双核 1024/1024，真差异容量 =
/// EAS 面 v2+）。
static mut TOPO: Topo = Topo::single();
/// 调度类（round-robin，G0/G1 默认）。
static RR: Fair = Fair;

/// idle / 初始任务 = `rust_main` 的活上下文（boot 栈 `__stack_top`）。**不分配栈**
/// （Bug 4）：`kstack=None`、`ctx` 留零——首切出时由 `switch` 覆盖为活的 callee-saved
/// + boot 栈 SP。idle 永不在 runqueue 上（runqueue 空时停在此）。
static mut IDLE_TASK: Task = Task {
    ctx: hal::task::TaskContext::ZERO,
    kstack: None,
    name: b"idle",
    state: TaskState::Running,
    slice_left: 0,
    vruntime: 0,
    vdeadline: 0,
    ttbr0: 0,
    asid: 0,
    prio_base: 0,
    prio_effective: 0,
    pi_inherited: 0,
    duration_hint_ns: 0,
    epp: crate::task::EppHint::Efficiency,
    preempt_threshold: 0,
    intent_declared: 0,
    intent_effective: 0,
    intent_hop: 0,
    intent_since: 0,
    intent_acc: 0,
    proc_tag: 0,
    pinned: 0,
    thread_key: 0,
    home_cpu: 0,
    spread: false,
    placement_hint: 0,
    held_locks: 0,
    svc_mask: 0,
    rq_prev: core::cell::Cell::new(None),
    rq_next: core::cell::Cell::new(None),
    sleep_prev: core::cell::Cell::new(None),
    sleep_next: core::cell::Cell::new(None),
    wait_obj: 0,
    wait_signals: 0,
    wait_deadline: 0,
    wait_since: 0,
    wakeup: core::cell::Cell::new(crate::task::WakeReason::Pending),
    wake_vct: core::cell::Cell::new(0),
};

/// IDLE 判定（v1.3 §7 util 记账门）：`t` 是否 CPU0 idle 上下文
/// （`IDLE_TASK`）。消费面 = [`crate::class::Fair::tick`]——idle 上跑的
/// tick 不计入 utilization EWMA（idle 时间是低 util 信号本身，不是运行
/// 时间；否则 CPU0 空转等睡眠者时 util 恒虚高满格）。副核 park 态
/// cur==null 早退，不经此门。
pub(crate) fn is_idle(t: *const Task) -> bool {
    // 取裸地址不出引用（edition 2024 下 addr_of_mut! 于 static mut 为安全形）。
    addr_of_mut!(IDLE_TASK) as usize == t as usize
}

/// 取**本核** runqueue 的 `&mut`（静态单例；KERN_LOCK 窗内跨核互斥、单核
/// 顺序访问，借用不重叠）。
#[inline]
unsafe fn rq() -> &'static mut Runqueue {
    rq_of(hal::smp::cpu_id())
}

/// 取指定核 runqueue（S-3b：放置路由/唤醒入队的跨核目标；调用侧须在
/// KERN_LOCK 窗内——数组槽位 per-CPU 独立，互斥仍靠全量锁）。`cpu` 越界
/// = 调用面违约（内部核号恒 0/1），未设防。
#[inline]
unsafe fn rq_of(cpu: usize) -> &'static mut Runqueue {
    let arr: &'static mut [Runqueue; 2] = &mut *addr_of_mut!(PER_CPU_RQ);
    &mut arr[cpu]
}

/// 拓扑只读视图（S-3b：`static mut` 化后调用面统一走本访问器；init 一次性
/// 写、其后只读）。
#[inline]
unsafe fn topo() -> &'static Topo {
    &*addr_of_mut!(TOPO)
}

// ---- S-2 E0（BKL 纪元）：KERN_LOCK 操作槽 + 切换舞蹈分诊（SMP-S2实施设计草案
// §2.1/§2.3）----
//
// 依赖方向（arch(hal) ← sched ← kernel）：sched 架构无关不直引 arch 锁符号 →
// 经函数指针槽注入（EXIT_HOOK / set_irq_dispatch 同款反转先例）。**kernel main
// 在 ktest 前安装**（syscall::install 之后）；None = 直通零开销——覆盖安装前
// 窗口（ktest 早期/boot 最早路径），单核语义全等（锁无竞争纯装饰）。

/// 锁原语槽（arch `kern_lock` 四函数）。
#[derive(Clone, Copy)]
pub struct KlockOps {
    /// KERN_LOCK 取锁（TTAS 自旋；持锁者永不睡眠——睡眠点先放锁）。
    pub kern_acquire: fn(),
    /// KERN_LOCK 放锁。
    pub kern_release: fn(),
    /// PRINT_LOCK 取（**调用方须已在 IRQ 屏蔽窗**——本对不带 irqsave，唯一
    /// 调用点 on_irq_tick 恒 PSTATE.I=1；任务态拼行走 arch 侧 with_print_lock）。
    pub print_lock: fn(),
    /// PRINT_LOCK 放。
    pub print_unlock: fn(),
}

/// 锁原语槽本体（boot 单线程注册，此后只读）。
static mut KLOCK_OPS: Option<KlockOps> = None;

/// 安装锁原语槽（kernel main，ktest 前恰一次）。
pub fn set_klock_ops(ops: KlockOps) {
    // SAFETY: boot 单线程注册；此后只读。
    unsafe { KLOCK_OPS = Some(ops) };
}

/// tick/IRQ 语境直取 KERN_LOCK（非任务上下文——无 Task 位参与）。
/// 舞蹈位感知 KL 臂直调 KERN_LOCK（3-1b 起唯一调用方——klock_acquire_
/// switch/klock_release_switch 的 TABLE 位分诊；tick/IRQ 面 KL 包络已剥）。
fn klock_acquire_raw() {
    // SAFETY: 槽安装后只读。
    if let Some(o) = unsafe { KLOCK_OPS } {
        (o.kern_acquire)();
    }
}

/// 舞蹈位感知 KL 臂直放 KERN_LOCK（同上）。
fn klock_release_raw() {
    // SAFETY: 同上。
    if let Some(o) = unsafe { KLOCK_OPS } {
        (o.kern_release)();
    }
}

/// PRINT_LOCK 取（sched 金丝雀拼行用；IRQ 屏蔽窗内调用）。
fn print_lock_raw() {
    // SAFETY: 同上。
    if let Some(o) = unsafe { KLOCK_OPS } {
        (o.print_lock)();
    }
}

/// PRINT_LOCK 放。
fn print_unlock_raw() {
    // SAFETY: 同上。
    if let Some(o) = unsafe { KLOCK_OPS } {
        (o.print_unlock)();
    }
}

/// 切走前分诊（舞蹈步「放」）：位集**内→外**物理放——**终态序（c-2c
/// slice 3-1b 定谳，§A112 原子刀）**：TABLE(bit5=KL) 最先放（最内）→
/// REAPQ(bit4) → RQ1(bit3) → RQ0(bit2) → SQ(bit1) 最后放（最外），与
/// SVC 入口/出口（结构先行、KL 类内末取/最先放）严格逆序对称——全局
/// 锁序 **KL 最内**（七层冻结序 SQ>队列>REAPQ>表；IRQ 面已无 KL，一切
/// 多锁位点恒升序前缀 ⇒ 跨核 ABBA 不可能）。**位保持置位**——跨挂起
/// 逻辑持有义务（位只在任务自身执行流写 ⇒ 挂起期稳定），切回由「取」步
/// 按同位集重取；物理锁在挂起期对其余核可见为空闲（自旋等待绝不跨挂起）。
/// 位=0（内核任务直调/半装窗）幂等跳过。bit5 单诊路径（内核任务 klock
/// 窗）本序下仅放 KL——行为逐位不变（零漂移）。
unsafe fn klock_release_switch(task: *mut Task) {
    let held = (*task).held_locks;
    if held & HELD_TABLE != 0 {
        klock_release_raw();
    }
    if held & HELD_REAPQ != 0 {
        reapq_release();
    }
    if held & HELD_RQ1 != 0 {
        rq_release(1);
    }
    if held & HELD_RQ0 != 0 {
        rq_release(0);
    }
    if held & HELD_SQ != 0 {
        sq_release();
    }
}

/// 切回后续行分诊（舞蹈步「取」）：位集**外→内**物理重取——**终态序
/// （c-2c slice 3-1b）**：SQ(bit1) → RQ0(bit2) → RQ1(bit3) → REAPQ(bit4)
/// → TABLE(bit5=KL) 最后取（最内），与 SVC 入口（结构先行、KL 类内末取）
/// 单向一致，防跨核 ABBA。位已置（「放」步物理放锁不清位）⇒ 本步不触位；
/// wait 重验循环/遥测/后续 handler 逻辑回到锁内；位=0 幂等跳过。bit5 单诊
/// 路径仅取 KL——行为逐位不变。
unsafe fn klock_acquire_switch(task: *mut Task) {
    let held = (*task).held_locks;
    if held & HELD_SQ != 0 {
        sq_acquire();
    }
    if held & HELD_RQ0 != 0 {
        rq_acquire(0);
    }
    if held & HELD_RQ1 != 0 {
        rq_acquire(1);
    }
    if held & HELD_REAPQ != 0 {
        reapq_acquire();
    }
    if held & HELD_TABLE != 0 {
        klock_acquire_raw();
    }
}

/// SVC 入口/出口**结构锁窗**槽回调（S-3c c-2c slice 2b；arch `kern_lock`
/// SVC_WINDOW 槽，kernel main 装）。KL 由 HAL 侧类内末取/先放（3-1b 终态
/// 正序——KL 最内）；本对只管结构位集：
/// enter —— 位集升序物理取（SQ→RQ0→RQ1→REAPQ；RQ 类位一律双核升序全取）
///   + `held_locks |= class` 记账 + `svc_mask = class` 声明。入口预取位让
///   handler 体内层窗（sq_lock/rq_lock/reapq 等）全退化（位已置 → 跳物理
///   取放、位不清）——位归本对收口，穿越体内全部内层窗直达舞蹈切点。
/// exit —— debug 断言 `held == class`（泄漏/越类执法——内层退化窗不触位，
///   出口不等于声明 = 有锁越类持有或泄漏）→ 结构降序物理放 + 清位 +
///   mask 清零。`-> !` 号（0x33/0x36）与 fault 链不达出口——放锁在
///   `switch_away_forever_with` 舞蹈按位收口。
/// CURRENT 空（ktest/boot 窗）→ 全跳过（KL 由 HAL 恒配平，handler 内层窗
/// 照旧自取——今日形态）。NONE 类（0x00）= 位集空 → 零物理取放（mask 写 0
/// 幂等）。
pub unsafe extern "C" fn svc_window(class: u8, enter: bool) {
    let cur = current_task();
    if cur.is_null() {
        return;
    }
    if enter {
        if class & HELD_SQ != 0 {
            sq_acquire();
        }
        if class & HELD_RQ0 != 0 {
            rq_acquire(0);
        }
        if class & HELD_RQ1 != 0 {
            rq_acquire(1);
        }
        if class & HELD_REAPQ != 0 {
            reapq_acquire();
        }
        (*cur).held_locks |= class;
        (*cur).svc_mask = class;
    } else {
        debug_assert_eq!(
            (*cur).held_locks,
            class,
            "svc_window exit: held 位集须恰等于声明类（越类持有/泄漏）"
        );
        if class & HELD_REAPQ != 0 {
            reapq_release();
        }
        if class & HELD_RQ1 != 0 {
            rq_release(1);
        }
        if class & HELD_RQ0 != 0 {
            rq_release(0);
        }
        if class & HELD_SQ != 0 {
            sq_release();
        }
        (*cur).held_locks &= !class;
        (*cur).svc_mask = 0;
    }
}

/// 结构锁持有位记账（S-3c c-2a 义务⑥面）：置/清当前任务 `held_locks` 对应
/// 位。CURRENT 空（ktest/boot/park 窗——结构锁原语在 IRQ 语境也可达，被
/// 打断任务可能为 park 空态）跳过：位只服务 c-2c 切换舞蹈「非零即禁」执法
/// 与排障可观测，锁本身的互斥不依赖位；窗内禁切换 ⇒ 位与锁恒同窗配平。
#[inline]
unsafe fn held_mark(bit: u8, on: bool) {
    let cur = current_task();
    if !cur.is_null() {
        if on {
            (*cur).held_locks |= bit;
        } else {
            (*cur).held_locks &= !bit;
        }
    }
}

/// 持有位测试（c-2c slice 2a 位感知退化面）：位已置 = 外层（同锁外层窗 /
/// 2b 起入口位集预取）已逻辑持有 → 物理取放退化跳过。CURRENT 空窗
/// （ktest/boot/park）恒 false = 照常真取（今日形态不变）。
#[inline]
unsafe fn held_test(bit: u8) -> bool {
    let cur = current_task();
    !cur.is_null() && ((*cur).held_locks & bit) != 0
}

/// 按核取 RQ 持有位（c-2b 义务⑥位面启用：bit2/bit3）。
#[inline]
fn held_rq_bit(cpu: usize) -> u8 {
    if cpu == 0 { HELD_RQ0 } else { HELD_RQ1 }
}

/// 取指定核队列锁（S-3c c-2b：RQ 结构面真取放——c-2a 落形字段的启用）。
/// 包络 = 一切环结构手术：pick（游标推进）、enqueue 接线、dequeue/unlink
/// 摘环、len/环走查读。**被选者置 Running 必须在放锁前落定**——被选者留环
/// （cur-in-ring），Running 标记是对核偷取的唯一结构屏障（`eligible` 只排
/// Running；picked-but-Ready 中间态在终态 = 同任务双核双跑）。prev 的
/// Ready 化同理入窗（其暴露面 = Ready 置位到切换 asm 落帧——过渡态由 KL
/// 全量遮蔽；C20 收缩时须另立切换护栏，c-2c 立法面）。
///
/// **位感知退化形（c-2c slice 2a）**：位已置（外层逻辑持有）→ 不物理取、
/// 返 `false`；配对 [`rq_unlock_of`] 收 `false` 同退化跳过。2a 全号仍全谱
/// ⇒ 位集在窗点恒零 ⇒ 退化支不可达（机制惰性就绪）；2b 入口位集预取后
/// 内层窗由此省物理取放。
///
/// 终态（slice 3-1b 起，§A112 原子刀落地）：RQ 号路径脱离 KL 包络，
/// KL> RQ 逆序边消解——RQ 锁独立串行，全局七层冻结序
/// （SQ>队列>REAPQ>表，KL 最内）单向成立。窗纪律同
/// `SLEEPQ_LOCK`：窗内禁切换（切点前必放）、不可重入（同核环同锁再取 =
/// 自死锁——EXIT_HOOK 可 wake_matching 入队，钩子面必须留在窗外）。
///
/// # Safety
/// IRQ 屏蔽；`cpu` ∈ {0,1}。
#[inline]
unsafe fn rq_lock_of(cpu: usize) -> bool {
    if held_test(held_rq_bit(cpu)) {
        return false; // 位已置——外层逻辑持有，物理退化（c-2c slice 2a）
    }
    rq_acquire(cpu);
    held_mark(held_rq_bit(cpu), true);
    true
}

/// 放指定核队列锁（先清位后放——位与锁恒同窗；`phys=false` 退化窗不触
/// ——物理持有归外层，配对 [`rq_lock_of`] 的返回值）。
///
/// # Safety
/// 同 [`rq_lock_of`]。
#[inline]
unsafe fn rq_unlock_of(cpu: usize, phys: bool) {
    if !phys {
        return; // 退化窗——物理持有归外层
    }
    held_mark(held_rq_bit(cpu), false);
    rq_release(cpu);
}

/// 取/放本核队列锁（pick×8 + dequeue×3 直调面的便捷形——`rq()` 的锁侧
/// 同款按核分诊；语义/纪律/退化形同 [`rq_lock_of`]）。
///
/// # Safety
/// 同 [`rq_lock_of`]。
#[inline]
unsafe fn rq_lock() -> bool {
    rq_lock_of(hal::smp::cpu_id())
}

/// # Safety
/// 同 [`rq_unlock_of`]。
#[inline]
unsafe fn rq_unlock(phys: bool) {
    rq_unlock_of(hal::smp::cpu_id(), phys)
}

// ---- G3-3 lockdep 物理取放包装组（feature 关 = 纯转发零成本；spin.rs 零
// 依赖纪律不破——钩子挂语义命名层，非 SpinLock 本体）----
// 纪律：**只在物理取放点调**——位感知退化支（位已置跳物理取放）不触，故
// 钩子不记退化窗；SLEEPQ/REAPQ/RQ 全树物理直取点收敛到本组（runqueue.rs
// 方法保持纯净，RQ 类号由本组 idx 分诊）。

/// SLEEPQ_LOCK 物理取（lockdep 类 = SQ）。
#[inline]
fn sq_acquire() {
    #[cfg(feature = "lockdep")]
    hal::lockdep::acquire(hal::lockdep::CLASS_SQ);
    SLEEPQ_LOCK.acquire();
}

/// SLEEPQ_LOCK 物理放。
#[inline]
fn sq_release() {
    #[cfg(feature = "lockdep")]
    hal::lockdep::release(hal::lockdep::CLASS_SQ);
    SLEEPQ_LOCK.release();
}

/// REAPQ_LOCK 物理取（类 = REAPQ）。
#[inline]
fn reapq_acquire() {
    #[cfg(feature = "lockdep")]
    hal::lockdep::acquire(hal::lockdep::CLASS_REAPQ);
    REAPQ_LOCK.acquire();
}

/// REAPQ_LOCK 物理放。
#[inline]
fn reapq_release() {
    #[cfg(feature = "lockdep")]
    hal::lockdep::release(hal::lockdep::CLASS_REAPQ);
    REAPQ_LOCK.release();
}

/// 指定核 RQ 锁物理取（RQ0/RQ1 分立类号——「双核升序」立法由此可检）。
///
/// # Safety
/// 同 [`rq_lock_of`]（IRQ 屏蔽；`idx` ∈ {0,1}）。
#[inline]
unsafe fn rq_acquire(idx: usize) {
    #[cfg(feature = "lockdep")]
    hal::lockdep::acquire(if idx == 0 {
        hal::lockdep::CLASS_RQ0
    } else {
        hal::lockdep::CLASS_RQ1
    });
    rq_of(idx).lock_acquire();
}

/// 指定核 RQ 锁物理放。
///
/// # Safety
/// 同 [`rq_acquire`]。
#[inline]
unsafe fn rq_release(idx: usize) {
    #[cfg(feature = "lockdep")]
    hal::lockdep::release(if idx == 0 {
        hal::lockdep::CLASS_RQ0
    } else {
        hal::lockdep::CLASS_RQ1
    });
    rq_of(idx).lock_release();
}

/// 初始化调度器：`current = &IDLE_TASK`。须在 `spawn` / `yield_now` 前、`mm::init`
/// 后调用一次。S-2b：经 [`set_current`] 落**本核主位**（CPU0 `cpu0_init` 装槽
/// 后 init 写 SmpCpu0.current；未装窗回落 static——语义逐位对齐）。
pub fn init() {
    // SAFETY: 启动单线程、init 仅调用一次。
    unsafe {
        set_current(addr_of_mut!(IDLE_TASK));
        // S-3b：capacity 填表（HAL 源——QEMU virt 同核 1024；真差异 = EAS v2+）。
        let t = &mut *addr_of_mut!(TOPO);
        for c in 0..t.nr_cpus.min(2) as usize {
            t.capacity[c] = hal::smp::cpu_capacity(c as u32);
        }
    }
    // S-6 ③b：自旋看门狗 bark 自装（spin.rs 零依赖槽的 hal 面）。
    crate::spin::set_spin_bark(spin_bark_hal);
    // S-6 ③b：KERN_LOCK 持者点名钩子自装（arch 反向注入槽）。
    hal::kern_lock::set_kl_name_hook(current_name_ptrs);
}

/// S-6 ③b：本核当前任务名指针（arch KERN_LOCK 看门狗持者点名；诊断面，
/// 定谳后随探针一并退役）。返回 (名指针, 名长)；(0, 0) = park 态/防御窗。
/// 名 = 任务字面量 &'static [u8]——跨核读不悬垂。
fn current_name_ptrs() -> (usize, usize) {
    // SAFETY: current_task 本核槽读契约（锁窗/屏蔽窗内由持锁调用点满足）；
    // 名切片 'static 字面量。
    unsafe {
        let t = current_task();
        if t.is_null() {
            return (0, 0);
        }
        let task: &Task = &*t;
        (task.name.as_ptr() as usize, task.name.len())
    }
}

// ---- S-6 ③b 自旋看门狗 bark（hal 面；诊断面，定谳后随探针一并退役）----

/// bark 节流锚（上次 bark 的 cntvct；0 起点 = 首报即过门）。
static SPIN_BARK_LAST: AtomicU64 = AtomicU64::new(0);
/// bark 总量帽记账（超 32 行静默——wedge 尸检足够，串口不再刷屏）。
static SPIN_BARK_CNT: AtomicU64 = AtomicU64::new(0);

/// 自旋看门狗 bark：par 门（单核零漂移）+ 1s 全局节流 + 32 行总量帽，
/// **原始 UART 直写不吃 PRINT_LOCK**（泄漏锁可能就是它——arch 侧
/// PRINT_LOCK 自旋同款自报）。lock 地址供尸检 `nm` 反查（REAPQ/SLEEPQ
/// 静态有名；Runqueue.lock 动态地址按区段归属）。
fn spin_bark_hal(addr: usize) {
    if !hal::smp::par_enabled() {
        return;
    }
    let now = hal::timer::cntvct();
    if now.saturating_sub(SPIN_BARK_LAST.load(Ordering::Relaxed)) < hal::timer::cntfrq() {
        return; // 1s 节流（全局；多锁并发 wedge 轮流报，总量帽兜底）
    }
    if SPIN_BARK_CNT.fetch_add(1, Ordering::Relaxed) >= 32 {
        return;
    }
    SPIN_BARK_LAST.store(now, Ordering::Relaxed);
    hal::uart_puts(b"spin! cpu=");
    hal::uart_put_dec(hal::smp::cpu_id() as u64);
    hal::uart_puts(b" lock=0x");
    hal::uart_put_hex64(addr as u64);
    hal::uart_puts(b"\n");
}

/// S-3b（子案 §9.6 填充点）bring-up 成功后真化拓扑：`nr_cpus=2` +
/// `capacity[1]` 填表（[`init`] 时 `nr_cpus` 恒 1 只填 [0]）。调用点 =
/// main `smp_active` 块（**par_enable 前**——gate 的 Release 序使本写入对
/// 副核 gate 后的一切读全可见；DAIF 屏蔽窗单写点）。单核形态不被调（
/// `smp_active` 假）——零漂移。现消费者 = LB 金丝雀/未来 EAS 面；v1 pick
/// 不读 nr_cpus（放置路由读队列长），纯账面真化。
pub fn topo_smp_online() {
    // SAFETY: boot 单线程（DAIF 屏蔽窗）、一次性写；此后只读（topo() 契约）。
    unsafe {
        let t = &mut *addr_of_mut!(TOPO);
        t.nr_cpus = 2;
        t.capacity[1] = hal::smp::cpu_capacity(1);
    }
}

// ---- S-3b 步 b-1：放置路由 + 跨核唤醒戳（子案 §9.4 义务⑧ / §8.C A4）----

/// 放置路由 v1：决定新任务的归属核（[`Task::home_cpu`] 定死一次的写入源）。
/// 判据全静态度——**永不读对核 park 态**（F-E6/F-A3 纪律）：
/// ①内核任务（`ttbr0==0`）恒 CPU0（内核串行纪元不变量）；
/// ②boot 裸表用户进程（`pinned==0`——ktest/boot 直构族，共享 BOOTSTRAP_HT/
/// 共表特形）恒 CPU0——S-2b user_only 出选臂（`pinned!=0`）判据的**放置面
/// 前移**：路由集 ≡ 旧副核出选集（恒等即正确性）；
/// ③单核形态（gate 关）恒 CPU0；
/// ④不可铺核（`spread==false`，b-1 判谳补）恒 CPU0——process 根
/// （服务/IPC 公民）路由上快核后以即时派发垄断共享通道槽（唤醒序/上限
/// 均救不了派发延迟差：快核 µs vs 本核轮转 ms——排障三连实证），v1 只铺
/// thread_start 的同进程计算线程（并行收益面）；b-2 偷取迁移再放开；
/// ⑤可铺核任务本核优先 + 深度失衡 K=2：本核队列长 ≥ 对核 + 2 → 改道对核
///   （「4-burst 第 3 个直接路由」语义；len 读 = AtomicUsize 免锁读——
///   c-2b slice 3 Q3 收口：C20 后表锁语境合法无逆序边，K=2 启发式容忍
///   跨核近似值）。
/// 只在 spawn 调（新放语义）；wake/expire 再入队回 home 不改道（防 sleeper
/// 被唤醒方弹核）；b-2 偷取迁移时随迁改写 home。
fn place(task: &Task) -> usize {
    if task.ttbr0 == 0 || task.pinned == 0 || !task.spread || !hal::smp::par_enabled() {
        return 0;
    }
    let me = hal::smp::cpu_id();
    let other = 1 - me;
    // SAFETY: rq_of 触 static mut 访问（spawn 单线程语境）；len 为
    // AtomicUsize（c-2b slice 3），Relaxed 免锁读无撕裂、不触环结构
    //（环面操作仍须 RQ 锁窗）。
    let (mine, theirs) = unsafe { (rq_of(me).len(), rq_of(other).len()) };
    if mine >= theirs + 2 { other } else { me }
}

/// 入队 + 跨核唤醒戳（义务⑧回归——S-3a 判词降级件，发送条件就此成立：
/// 路由/归属使「目标核队列真有货」恒真，深 park 核的 peek 不再被恒 Ready
/// child 遮蔽）。发送判据 = `cpu != cpu_id()` 两静态度，**绝不读对核 park
/// 态**（F-E6/F-A3）；单核形态（gate 关）本核恒等 → 零发送（零漂移）。
/// 副核收 SGI → eret 回 park 环 → `park_recheck` 读**本核**队列自然出选
/// （S-3a 睡穿边界收口）。
///
/// # Safety
/// KERN_LOCK/IRQ 屏蔽窗内（spawn/wake/expire 调用面契约）；`cpu` ∈ {0,1}。
unsafe fn enqueue_with_kick(task: &mut Task, cpu: usize) {
    task.home_cpu = cpu as u8;
    // RQ 窗（c-2b）：目标核环接线入窗（本核或远核——SQ 窗内调用面（wake/
    // expire）= SQ>队列 正序嵌套；spawn 面 = 纯 RQ 窗）。SGI 戳留窗外
    //（MMIO 非阻塞、不触环——窗最小化）。
    let phys = rq_lock_of(cpu);
    RR.enqueue(task, rq_of(cpu));
    rq_unlock_of(cpu, phys);
    if cpu != hal::smp::cpu_id() && hal::smp::par_enabled() {
        // SAFETY: 写 GICD_SGIR（EL1）；List 位图 = 路由目标核。
        unsafe {
            hal::gic::send_sgi(
                hal::gic::SGI_RESCHEDULE,
                hal::gic::SgiTarget::List(1 << cpu),
            )
        };
    }
}

/// 生成一个内核线程：分配 16KiB 栈 + 接线 + 入 runqueue（无地址空间）。
///
/// **调用方须保证 IRQ 屏蔽**（alloc/enqueue 不可被打断——plan「IRQ 热路径零分配」）：
/// Step 3 main 在 boot 屏蔽态调用；Step 6+ 若从任务体 spawn，需自行 disable/enable 包络。
/// v1 不回收栈（leak；Step 7+ 加 reap）。
pub fn spawn(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8]) {
    spawn_as(entry, arg, name, 0, 0);
}

/// 同 [`spawn`]，但挂地址空间（2b-2）：`ttbr0` 页表 PA + `asid`（0/0 = 无 AS）。
/// 表须已填好且对页表走查可见（调用方 `dsb ish`），activate 只管寄存器。
pub fn spawn_as_spread(entry: extern "C" fn(usize) -> !, arg: usize,
                          name: &'static [u8], ttbr0: usize, asid: u16) {
    let task: &'static mut Task =
        alloc::boxed::Box::leak(Task::new_kernel_with_as(entry, arg, name, ttbr0, asid));
    // S-6：显式可迁移内核带 AS 任务（矩阵对等金丝雀）。服务/ pager 根任务
    // 保持 spread=false 恒家核；跨核偷取只消费显式可迁移面。
    task.spread = true;
    unsafe {
        enqueue_with_kick(task, place(task));
    }
}

pub fn spawn_as(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8],
                ttbr0: usize, asid: u16) {
    // SAFETY: CURRENT 已由 init 设好；allocator 已就绪。
    let task: &'static mut Task =
        alloc::boxed::Box::leak(Task::new_kernel_with_as(entry, arg, name, ttbr0, asid));
    unsafe {
        enqueue_with_kick(task, place(task));
    }
}

/// 同 [`spawn`]，但带基础优先级（4-3b PI 验收：高优 client 的捐赠源）。
/// `prio` 直接落 `prio_base`/`prio_effective` 双字段（pick 出选看 effective，
/// 4-3a）——boot 可信语境一步到位。
pub fn spawn_with(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8],
                  prio: i32) {
    // SAFETY: 同 spawn_as（CURRENT 已设、allocator 就绪）。
    let task: &'static mut Task =
        alloc::boxed::Box::leak(Task::new_kernel_with_as(entry, arg, name, 0, 0));
    task.prio_base = prio;
    task.prio_effective = prio;
    unsafe {
        enqueue_with_kick(task, place(task));
    }
}

/// 同 [`spawn_with`]，但挂内核侧**私有 handle 表** tag（4-5 前置：常驻内核服务
/// 不进共享内核侧表——PI 受捐判定按持有表反查，共享表上的睡眠者会被视作持有
/// 表内一切 handle（4-3b v1 粒度），常驻任务 = 每轮捐赠周期错捐 + 级联）。
/// `tag` 须为 boot 侧 `Box::leak` 常驻 `HandleTable` 指针（内核 crate 分配，
/// 此处不透明 usize；语义层 `caller_table` EL1 分支与 `pi::holder_table` 均按
/// 非 0 tag 走私有表——与 EL0 每进程表同粒度）。
pub fn spawn_with_tag(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8],
                      prio: i32, tag: usize) {
    assert!(tag != 0, "spawn_with_tag：tag 不得为 0（共享表任务用 spawn/spawn_with）");
    // SAFETY: 同 spawn_as（CURRENT 已设、allocator 就绪）。
    let task: &'static mut Task =
        alloc::boxed::Box::leak(Task::new_kernel_with_as(entry, arg, name, 0, 0));
    task.prio_base = prio;
    task.prio_effective = prio;
    task.proc_tag = tag;
    unsafe {
        enqueue_with_kick(task, place(task));
    }
}

/// 生成一个**用户**线程（2b-4 首个 EL0）：内核栈 + `new_user` 上下文（首切经
/// trampoline `eret` 降 EL0）+ 入 runqueue。`entry_va`/`sp0_va` 是用户 VA（该 AS
/// 已映射、AP 放行 EL0）。`proc_tag` = 所属进程标签（3-4a：syscall 层解释为该
/// 进程的 handle 表指针——EL0 syscall 按任务派发到自己的表；0 = 无表）。
/// 前置同 [`spawn`]（IRQ 屏蔽态）。
pub fn spawn_user(entry_va: usize, arg: usize, sp0_va: usize, name: &'static [u8],
                  ttbr0: usize, asid: u16, proc_tag: usize) {
    // boot 可信语境一步到位；EL0 路径（process_start）走 [`try_spawn_user`]
    // 折 -ENOMEM（3b3 H2——原 Task 装配内嵌 vec!/Box::new，OOM 直接 abort）。
    // 返回的任务指针 boot 手搭场景不消费（无 Process 钉住——UserProcess 裸表
    // 形态），丢弃即可。
    try_spawn_user(entry_va, arg, sp0_va, name, ttbr0, asid, proc_tag, 0, 0, false)
        .expect("spawn_user：用户任务装配（boot 可信语境）");
}

/// 同 [`spawn_user`] 但 try 化（3b3 审查 H2）：16KiB 栈 / TCB 装箱任一 OOM 返
/// `None`、**不入队、零副作用**——EL0 经 `process_start` 触发时内核侧折
/// -ENOMEM（原路径 abort = EL0 可触发内核 panic，违 ABI §8.0）。前置同
/// [`spawn`]（IRQ 屏蔽态）。
///
/// Plan B v0.5：返回新任务的裸指针（usize 不透明值——语义层写入 `pinned`
/// 钉住字段；crate 边界纪律同 `proc_tag`）。任务已入 runqueue，指针即
/// `&'static mut Task` 的地址（Box::leak）。
pub fn try_prepare_user(entry_va: usize, arg: usize, sp0_va: usize, name: &'static [u8],
                         ttbr0: usize, asid: u16, proc_tag: usize,
                         pinned: usize, thread_key: usize, spread: bool) -> Option<usize> {
    let task = Task::try_new_user_with_as(entry_va, arg, sp0_va, name, ttbr0, asid, proc_tag)?;
    // SAFETY: CURRENT 已由 init 设好；allocator 已就绪；IRQ 屏蔽态。
    let task: &'static mut Task = alloc::boxed::Box::leak(task);
    // F-E1 / S-6 SMP：pinned/thread_key/spread 在 enqueue 前写入。SMP 下
    // 远端核可能在入环后立刻偷取并运行新任务；一切调用侧簿记（started、
    // live_threads、retain、调度属性）都必须在 enqueue 前完成。
    task.pinned = pinned;
    task.thread_key = thread_key;
    task.spread = spread;
    Some(task as *mut Task as usize)
}

/// 把 [`try_prepare_user`] 产出的已初始化任务入环。调用侧必须已完成一切
/// 可被新任务退出钩子/属性消费观察的簿记；本函数不再分配、不可失败。
///
/// # Safety
/// IRQ 屏蔽窗内；`task` 来自 `try_prepare_user` 且尚未入任何 runqueue。
pub unsafe fn enqueue_prepared_user(task_ptr: usize) {
    let task = task_ptr as *mut Task;
    // SAFETY: 上述调用契约；place 只读本/对核长度。
    let cpu = place(unsafe { &*task });
    // SAFETY: 同 try_spawn_user 旧路径。
    unsafe { enqueue_with_kick(&mut *task, cpu) };
}

pub fn try_spawn_user(entry_va: usize, arg: usize, sp0_va: usize, name: &'static [u8],
                      ttbr0: usize, asid: u16, proc_tag: usize,
                      pinned: usize, thread_key: usize, spread: bool) -> Option<usize> {
    let task_ptr = try_prepare_user(
        entry_va, arg, sp0_va, name, ttbr0, asid, proc_tag,
        pinned, thread_key, spread,
    )?;
    // SAFETY: prepare 返回尚未入环；本 wrapper 无调用侧额外簿记。
    unsafe { enqueue_prepared_user(task_ptr) };
    Some(task_ptr)
}

/// 当前任务的进程标签（3-4a per-process handle 表派发用）。0 = 无（内核任务/
/// `init` 之前的 boot 期——ktest 在 `init` 前跑，此时 CURRENT 为 null → 0）。
/// 语义归 `内核` crate；本函数只透传，单核只读。
pub fn current_tag() -> usize {
    // SAFETY: 只读；锁窗/屏蔽窗内单核顺序访问（写点全在切换原语锁窗）。
    let cur = unsafe { current_task() };
    if cur.is_null() {
        0
    } else {
        unsafe { (*cur).proc_tag }
    }
}

/// 当前任务钉住的 Process 对象键（C-4b `process_terminate` 的自杀判别用）：
/// `Task.pinned` 只读透传（写点 = [`try_spawn_user`] 装配期，F-E1 前移）。
/// 0 = 无（内核任务 / boot 裸表用户进程 / ktest 期 CURRENT=null）——调用方
/// 拿它跟目标 Process ObjRef 地址比对，0 恒不等任何在表对象指针。单核只读。
pub fn current_pinned() -> usize {
    // SAFETY: 只读；锁窗/屏蔽窗内单核顺序访问（写点全在切换原语锁窗）。
    let cur = unsafe { current_task() };
    if cur.is_null() {
        0
    } else {
        unsafe { (*cur).pinned }
    }
}

/// 1.1/S3-a：冻结前置谓词——按钉住键扫描，任一任务命中即返 false。
/// 检查项：① `pi_inherited != 0`（PI 捐赠边）；② `wait_signals` 含内核
/// 派生位（READABLE/WRITABLE——donor 面）；③ `held_locks != 0` 或
/// `svc_mask != 0`（SMP Running 者防御面——单核 Ready/Blocked 结构性 0）。
pub fn freeze_precheck_by_pinned(pinned: usize) -> bool {
    // ABI 冻结值（abi_gen SIGNAL_READABLE=1 / SIGNAL_WRITABLE=2——sched crate
    // 不依赖 kernel handle 模块，本地镜像同值）。
    const SIG_READABLE: u64 = 1;
    const SIG_WRITABLE: u64 = 2;
    if pinned == 0 {
        return false;
    }
    // SAFETY: SVC 路径 KL 已持；只读扫描。
    unsafe {
        // app-1 段 a-1（2026-11-08 实证）：svc_mask 只在 SVC 窗口出口清零
        //（:486），**睡在 syscall 内的任务（block_current 舞蹈放锁入队）带着
        // 非零 mask 挂 SLEEPQ**——若对睡眠者也查 svc_mask，任何睡着的进程
        // 结构性不可冻（16×50ms 采样全拒实证），与「到期 sleep 摘 RQ 挂起」
        // 的冻结设计矛盾。睡眠者已放全部结构锁（舞蹈语义），mask 只是
        // 「syscall 尚未走完出口」的记账残影——真正在飞（可被抢占于 SVC
        // 窗内）的形态 = RQ 在席。故 svc_mask 只对 RQ 任务拒。

        // app-1 段 a-1 双残影定谳（2026-11-08 实证 16 试全拒，其中睡眠者
        // 持锁残影样本 ×15）：svc_mask 与 held_locks 位都是 SVC 窗
        // 记账，随「睡在 syscall 内」的任务带进 SLEEPQ（block 舞蹈放的是
        // 物理锁，位不清——醒后重取复用）。睡眠者位是残影、锁已放：冻结
        // 只暂停执行、不动锁，无停摆面（防停摆的是 pi_inherited 检查，保留
        // ——PI 边上冻结 = 捐赠链断）。真在飞 = RQ 在席（窗内可抢占形），
        // svc_mask + held 结构位都对 RQ 任务拒。冻结可能延迟「睡眠者真持
        // 锁时的等待者」= 策略域代价（AM-01 Q1-c 唤醒即解冻配套），非谓词域。
        let sleep_ok = |t: &Task| -> bool {
            t.pi_inherited == 0
                && t.wait_signals & (SIG_READABLE | SIG_WRITABLE) == 0
        };
        let rq_ok = |t: &Task| -> bool {
            sleep_ok(t)
                && t.svc_mask == 0
                // 排除 HELD_FROZEN（bit6，嵌套冻结）+ HELD_THROTTLE（bit7，
                // S5-c 预算限流——调度态非持锁，限流中的 deadline 任务仍可
                // 被进程冻结）。只有结构锁位 bit1–5 才拒。
                && t.held_locks
                    & !(crate::task::HELD_FROZEN | crate::task::HELD_THROTTLE)
                    == 0
        };
        // SLEEPQ
        let mut cur = sleepq().head.get();
        while let Some(c) = cur {
            let t = c.as_ptr();
            cur = (*t).sleep_next.get();
            if (*t).pinned == pinned && !sleep_ok(&*t) {
                return false;
            }
        }
        // 双 RQ
        for cpu in 0..2 {
            let head = rq_of(cpu).head();
            let mut cur = head;
            while let Some(c) = cur {
                let t = c.as_ptr();
                // §A136：先访当前节点，再推进并判回 head——先 break 会漏尾/单元素。
                if (*t).pinned == pinned && !rq_ok(&*t) {
                    return false;
                }
                cur = (*t).rq_next.get();
                if cur == head { break; } // 环终止

            }
        }
    }
    true
}

/// 1.1/S3-a：按钉住键冻结该进程全部任务（SLEEPQ + 双 RQ 扫描）。
/// 仅置 `frozen` 位——不改队列结构（任务留队，`Fair::eligible` 过滤）。
/// 返回**匹配**任务数（0 = 无匹配任务——嵌套第二 token 不要求找到新的
/// 未冻任务，`sys_process_suspend` 注释契约；置位幂等 = `freeze_by_thread`
/// 同款形——AM-1.5 前实现已冻跳过不计数，嵌套进程冻恒折 -EBADSTATE，
/// 与 depth 帽 8 的嵌套设计自相矛盾，本刀对齐）。调用侧：SVC 路径
///（KL 已持——单核无并发）。
pub fn freeze_by_pinned(pinned: usize) -> usize {
    if pinned == 0 {
        return 0;
    }
    let mut count = 0;
    // SAFETY: SVC 路径 KL 已持（单核无并发队列变更）；只写 Cell 不动链。
    unsafe {
        // SLEEPQ 扫描
        let mut cur = sleepq().head.get();
        while let Some(c) = cur {
            let t = c.as_ptr();
            cur = (*t).sleep_next.get();
            if (*t).pinned == pinned {
                (*t).held_locks |= crate::task::HELD_FROZEN; // 幂等置位
                count += 1;
            }
        }
        // 双 RQ 扫描
        for cpu in 0..2 {
            let head = rq_of(cpu).head();
            let mut cur = head;
            while let Some(c) = cur {
                let t = c.as_ptr();
                // §A136：先访当前节点，再推进并判回 head。
                if (*t).pinned == pinned {
                    (*t).held_locks |= crate::task::HELD_FROZEN; // 幂等置位
                    count += 1;
                }
                cur = (*t).rq_next.get();
                if cur == head { break; } // 环终止

            }
        }
    }
    count
}

/// 1.1/S3-a：按钉住键解冻（SuspendToken Drop 路径调用）。
pub fn thaw_by_pinned(pinned: usize) {
    if pinned == 0 {
        return;
    }
    // SAFETY: 同 freeze_by_pinned——handle close 也在 SVC/IRQ 屏蔽路径。
    unsafe {
        let mut cur = sleepq().head.get();
        while let Some(c) = cur {
            let t = c.as_ptr();
            cur = (*t).sleep_next.get();
            if (*t).pinned == pinned {
                (*t).held_locks &= !crate::task::HELD_FROZEN;
            }
        }
        for cpu in 0..2 {
            let head = rq_of(cpu).head();
            let mut cur = head;
            while let Some(c) = cur {
                let t = c.as_ptr();
                // §A136：先访当前节点，再推进并判回 head。
                if (*t).pinned == pinned {
                    (*t).held_locks &= !crate::task::HELD_FROZEN;
                }
                cur = (*t).rq_next.get();
                if cur == head { break; } // 环终止

            }
        }
    }
}

/// 同 [`spawn`]，但带声明档（4-3c intent 验收：借入/拒借的档位差源）。
/// `intent` 直落 `intent_declared`/`intent_effective` 双字段——boot 可信语境
/// 一步到位；非法值（编码域外）直接 panic（同 `new_kernel` 系 expect 政策）。
pub fn spawn_intent(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8],
                    intent: u8) {
    assert!(crate::intent::intent_valid(intent), "spawn_intent：编码域外");
    // SAFETY: 同 spawn_as（CURRENT 已设、allocator 就绪）。
    let task: &'static mut Task =
        alloc::boxed::Box::leak(Task::new_kernel_with_as(entry, arg, name, 0, 0));
    task.intent_declared = intent;
    task.intent_effective = intent;
    unsafe {
        enqueue_with_kick(task, place(task));
    }
}

/// 4-3c：send 侧 intent 快照（syscall 层调用；语义在 [`crate::intent`]）。
/// CURRENT 无效（ktest 在 `init` 前跑）→ `None`——调用方按零值打包消息头，
/// 不记遥测。IRQ 屏蔽窗内单核访问。
pub fn intent_snapshot_current(now_ns: u64) -> Option<(crate::intent::MsgIntent, crate::intent::SnapOut)> {
    // SAFETY: 只读 CURRENT；写点全在 IRQ 屏蔽路径（单核顺序）。
    let cur = unsafe { current_task() };
    if cur.is_null() {
        None
    } else {
        Some(crate::intent::snapshot(unsafe { &*cur }, now_ns))
    }
}

/// 4-3c：recv 侧 intent 结算（slowpath；语义在 [`crate::intent`]）。CURRENT
/// 无效 → `None`（消息照常投递，不结算不记遥测）。IRQ 屏蔽窗内单核访问。
/// W3-Q6：结算后附 `(declared, effective)` 双档读出（`intent_settle_note`
/// 的档位直方/通胀计数源——`Task` 字段在锁窗内单核读安全）。
pub fn intent_settle_current_ext(
    m: crate::intent::MsgIntent,
    opt_out: bool,
    now_ns: u64,
) -> Option<(crate::intent::SettleOut, u8, u8)> {
    // SAFETY: CURRENT 可写；syscall 主体 IRQ 屏蔽（免锁根基），单核顺序。
    let cur = unsafe { current_task() };
    if cur.is_null() {
        None
    } else {
        // SAFETY: 同上。
        let t = unsafe { &mut *cur };
        let out = crate::intent::settle(t, m, opt_out, now_ns);
        Some((out, t.intent_declared, t.intent_effective))
    }
}

/// 4-3c：recv 侧 intent 结算（旧面——W3-Q6 前 9 个调用点零改动；
/// 实现在 [`intent_settle_current_ext`]）。
pub fn intent_settle_current(m: crate::intent::MsgIntent, opt_out: bool,
                             now_ns: u64) -> Option<crate::intent::SettleOut> {
    intent_settle_current_ext(m, opt_out, now_ns).map(|(out, _, _)| out)
}

/// 2b-2：任务地址空间激活——`ttbr0`/`asid` **变化才**写 TTBR0+CONTEXTIDR+`tlbi`
/// （`内存管理设计.md` §5.1：switch 必带按号失效；同 AS 连续切换零额外开销）。
/// 调用点：三个切换位（yield/exit/IRQ 抢占）在 `CURRENT = next` 之后、
/// `context_switch` 之前——先换表再切栈，全程高半执行，无低半依赖。
#[inline]
unsafe fn activate_if_changed(prev: *mut Task, next: *mut Task) {
    if (*prev).ttbr0 != (*next).ttbr0 || (*prev).asid != (*next).asid {
        hal::activate_aspace((*next).ttbr0, (*next).asid);
    }
}

/// 协作让出：切到 `pick_next` 选出的下一个任务（若有且非自己）。
/// Bug 2：`disable_irq` → `switch` → （被切回时）`enable_irq`。
///
/// S-3b：pick 走本核队列（per-CPU 分诊 + eligible Running 排除单臂——
/// PickFilter user_only 臂随放置纯度退役）；副核 pick None = 本核队无货 →
/// 切回 park 探测环（**防御面**——生产不可达；park 三切点保舞蹈完备）。
pub fn yield_now() {
    yield_now_impl(true);
}

/// SVC 0x3F 专用协作让出：与 [`yield_now`] 同舞蹈，但**出口保持 IRQ 屏蔽**——
/// syscall 派发尾仍需 `svc_exit_class` 放锁；提前解蔽 = 「持锁开窗」违反
/// S-3c 锁窗纪律（tick 可在锁内闯入）。eret 由异常返回路径恢复 PSTATE。
pub fn yield_now_keep_irq() {
    yield_now_impl(false);
}

fn yield_now_impl(enable_irq_at_end: bool) {
    hal::disable_irq();
    assert_preempt_balanced("yield_now");
    // SAFETY: IRQ 已屏蔽；CURRENT/本核 runqueue 顺序访问。
    unsafe {
        let prev = current_task();
        // RQ 窗（c-2b）：出选 + 状态定形同窗——next 置 Running = 防对核偷取
        // 屏障（放锁前落定）；prev Ready 化同窗。切换舞蹈在窗外（窗内禁切换）。
        let phys = rq_lock();
        let picked: Option<*mut Task> = RR
            .pick_next_filtered(rq(), topo(), prev)
            .map(|n| n as *mut Task);
        if let Some(next_ptr) = picked {
            if next_ptr != prev {
                (*prev).state = TaskState::Ready;
                // yield 片归还（moss yield_slice 语义的 BerryOS 形态）——让出者
                // 不得带未耗虚拟优势回场，提案 2026-11-08，SMP 前单核窗内安全；
                // 双任务真让出补刀（同日）：抬到环上 vruntime max（原抬均——
                // 双任务环 avg<max 恒有零操作区），让出者本轮垫底（POSIX
                // sched_yield 对齐）。冻结（HELD_FROZEN）与 deadline 任务路径
                // 不受影响——只动 vruntime，S5 手动 vdeadline 不碰。
                RR.yield_slice_return(&mut *prev, rq());
                (*next_ptr).state = TaskState::Running;
            }
        }
        rq_unlock(phys);
        if let Some(next) = picked {
            let next_ptr: *mut Task = next;
            if next_ptr != prev {
                (*next_ptr).slice_left = hal::timer::slice_cycles();
                // 授片即重编（`调度器设计.md` §4.1 绝对截止时间纪律）：以授片时刻的
                // CNTVCT 现读重算绝对截止——新任务从授片时刻起拿满片，不继承 prev
                // 的残余倒计。
                // SAFETY: IRQ 已被 disable_irq 屏蔽；写 CNTV 系统寄存器；EL1。
                hal::timer::arm_slice();
                set_current(next_ptr);
                // S-6 ① 补数：直切臂（出选有货）旧不计 SWITCHES——SMP 稳态忙对
                // 占双核时 park 切点结构性冷、本臂才是热路径，漏计=金丝雀盲区。
                bump_switches();
                hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
                activate_if_changed(prev, next_ptr);
                note_switch(prev, next_ptr, SwitchKind::Yield);
                // S-2 E0 舞蹈（草案 §2.3）：SVC 持锁窗内走到切换点 → 切前放锁
                //（内核任务直调位=0 幂等跳过）；切回即重取——续行回锁窗。
                klock_release_switch(prev);
                hal::task::context_switch(&raw mut (*prev).ctx, &raw const (*next_ptr).ctx);
                klock_acquire_switch(prev);
                // <- 被切回 prev 时续行于此（CURRENT==prev）。
            }
        } else if hal::smp::cpu_id() != 0 {
            // S-2b park 切点（yield 形态，防御面）：置 null 空哨兵 + 回 park。
            // park 上下文跑内核高半（ttbr0!=0 才回内核空表）；SWITCHES 计入
            // 金丝雀（A12 口径：写点在锁窗/屏蔽窗内）。
            set_current(core::ptr::null_mut());
            if (*prev).ttbr0 != 0 {
                hal::activate_aspace(0, 0);
            }
            bump_switches(); // S-6 ①：递增+打印共享 helper（A12 兑现）
            hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
            note_switch(prev, core::ptr::null_mut(), SwitchKind::Yield);
            klock_release_switch(prev);
            hal::task::context_switch(&raw mut (*prev).ctx, hal::smp::sec_park_ctx() as *const _);
            // 被 park 派发切回时续行于此（IRQ 仍屏蔽——yield_now 包络对称）。
            klock_acquire_switch(prev);
        }
    }
    // Plan B：切回后顺手回收退出任务（drop Box + 内核栈）——本上下文非被回收
    // 者自身栈；无切换发生时也 drain（环形忙转下 idle 可能长期不来）。
    // S-2b：参与态下 REAPQ push 可来自 CPU1 锁窗退出链——drain 经 REAPQ 锁
    // 串行（c-2b slice 2；3-1b 起无 KL 包络——reap_drain 位感知自持 REAPQ
    // 窗，独立单锁面）。副核防御 yield 面不 drain（REAPQ 由 CPU0
    // idle/tick 消化）。
    if hal::smp::cpu_id() == 0 {
        reap_drain();
    }
    if enable_irq_at_end {
        hal::enable_irq();
    }
}

/// 当前任务退出：出 runqueue + 切到下一个（或 idle）。永不返回。
/// Plan B v0.5：经 [`switch_away_forever_with`]（退出码 0——fault/kill 与
///「无 Process」路径的码面；真退出码走 [`exit_current_with`]）。
pub fn exit_current() -> ! {
    hal::disable_irq();
    // SAFETY: IRQ 已屏蔽；cur 已 dequeue 后无人切回它。
    unsafe { switch_away_forever_with(0) }
}

/// 当前任务带退出码退出（Plan B v0.5：`process_exit`/`thread_exit` 的
/// syscall 语义层调用）。钩子把码落进 Process 账面再切走，永不返回。
pub fn exit_current_with(code: i32) -> ! {
    hal::disable_irq();
    // SAFETY: 同 exit_current（SVC 语境 IRQ 本就屏蔽——disable_irq 幂等）。
    unsafe { switch_away_forever_with(code) }
}

/// 用户任务致死计数（G1.5 验收：watchdog 断言 == 期望杀数）。
/// S-2 E0 前置件 A12：原子化（写点在锁内 kill 链，读侧 watchdog 锁外）。
static USER_KILLS: AtomicU64 = AtomicU64::new(0);

/// 最近被杀用户任务名（G1.5 验收：watchdog 断言**杀的是 rogue**——杀数==1 但错杀
/// good/abuser 不得过关）。A12 原子化：胖指针拆 `AtomicPtr + AtomicUsize` 对
///（名字本体 = Task.name 的 `&'static` 字面量，写后永不失效）；ptr/len 对经 len
/// 的 Release/Acquire 定序——撕裂最坏形 = 新 ptr 配旧 len，watchdog 的整切片
/// 比较天然免疫（诊断读、非正确性依赖）。
static LAST_KILLED_PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static LAST_KILLED_LEN: AtomicUsize = AtomicUsize::new(0);

/// 当前用户任务因致命故障被杀（HAL `exception_sync_el0` 经函数指针槽调；2b-4
/// G1.5——**只死自己**，内核与其余任务无恙）。打印 + 计数 + 切走，永不返回：
/// 被弃的是故障帧所在的内核栈（同步异常入口 PSTATE.I=1，无 GIC 状态要收拾）。
///
/// # Safety
/// 同步异常上下文（PSTATE.I=1）；C-ABI fn 指针槽；CURRENT 是用户任务。
/// S-2 E0：fault 臂 dump 前已 `svc_enter`（trap.rs 3.1-2）——本函数全程在
/// KERN_LOCK 锁窗内，切走经 switch_away_forever_with 按位收口。
pub unsafe extern "C" fn user_fault_exit() -> ! {
    let cur = current_task();
    USER_KILLS.fetch_add(1, Ordering::Relaxed);
    LAST_KILLED_PTR.store((&(*cur).name).as_ptr() as *mut u8, Ordering::Relaxed);
    LAST_KILLED_LEN.store((&(*cur).name).len(), Ordering::Release);
    hal::uart_puts(b"\nG1.5: kill user task '");
    hal::uart_puts((*cur).name);
    hal::uart_puts(b"'\n");
    // Plan B：fault 杀也走退出钩子（Process 置 Exited——exit_code 0 是诚实
    // 注释面：杀因观察缺位，码面留给 v2 get_info）；码 0 与旧 exit_current 同。
    switch_away_forever_with(0)
}

/// c-2c slice 2a 执法面：EXIT_HOOK 三调用点前置断言——结构位（bits1-4）
/// 全零（钩子 = 结构锁窗外契约的执法点：任一置位 = 位/锁失同步或窗纪律
/// 破缺，dev 构建 fail-loud）。bit5 表锁随调用语境可置（SVC 包络/直调
/// 包络）可清（boot 早期内核任务直退），不诊。钩子内 wake_matching 自持
/// 开窗不受本断言约束（窗外契约针对**入口**持有态）。
#[inline]
unsafe fn assert_exit_hook_windows_closed() {
    let cur = current_task();
    if !cur.is_null() {
        // c-2c slice 2b 演化：SVC 全谱/含结构类号（0x2E/0x2C/0x3E）合法携带
        // 入口预取结构位过此点（收口在尾部舞蹈）⇒ 断言基从「无结构位」放宽
        // 为「结构位 ⊆ svc_mask 声明类」（TABLE 位豁免——内核任务/直调面
        // 无声明恒携带）。越类结构位 = 泄漏，照杀。**空转面**：全谱死亡路径
        // （0x33/0x36/EL0 fault 链，mask=0x3E）四结构位恒 ⊆ mask——谓词在该
        // 面不设防，防漏靠入口预取物理配平 + 舞蹈按位放；快照级执法（held
        // == 入口记录）= slice 3-2 挂账加固。
        debug_assert!(
            (*cur).held_locks & !(*cur).svc_mask & !HELD_TABLE == 0,
            "exit hook: structural lock bit beyond declared class"
        );
    }
}

/// exit/kill 共用尾部：当前任务出 runqueue + 切到下一个（或 idle），永不回到它。
/// Plan B v0.5 扩两刀：①退出前调**任务退出钩子**（`内核` crate 注册——Process
/// Exited 落账 + TERMINATED 唤醒 + drain 子表 + release 运行钉住；未注册/
/// pinned=0 空转）；②cur 的 Task Box + 16KiB 内核栈压 **REAPQ**（退出即泄漏
/// 不可接受——bsh `run` 子进程无界；真正释放在其它上下文的 [`reap_drain`]，
/// 此刻还在 cur 栈上跑，不能自己拆自己的栈）。
///
/// # Safety
/// IRQ 已屏蔽（显式 disable_irq 或异常入口硬件屏蔽）；CURRENT 有效。
unsafe fn switch_away_forever_with(code: i32) -> ! {
    let cur = current_task();
    assert_preempt_balanced("exit 族（switch_away_forever_with）");
    debug_assert!((*cur).state != TaskState::Exited, "double exit");
    assert_exit_hook_windows_closed();
    if let Some(hook) = EXIT_HOOK {
        // 语义层钩子：pinned=0（内核任务/boot 裸表进程）直通。钩子内可能
        // wake_matching（唤醒者入 runqueue——下方 pick 自然可见）。
        hook((*cur).pinned, (*cur).thread_key, code);
    }
    // RQ 窗（c-2b）：dequeue（自摘环置 Exited）+ 出选 + 被选者置 Running 同
    // 窗；reapq_push 在内（REAPQ 锁嵌套 = 终态 RQ>REAPQ 正序——c-2b slice 2
    // 落形）。EXIT_HOOK 留窗前（钩子可 wake_matching 入队 = 同核环同锁重
    // 入，绝不入窗）。
    let phys = rq_lock();
    RR.dequeue(&mut *cur, rq());
    reapq_push(cur);
    // S-2b：pick 走本核过滤器——副核环上无可见货（pinned!=0 全无）时不落
    // IDLE fallthrough（IDLE_TASK 是 CPU0 boot 上下文，副核不得切入）而切回
    // park 探测环（exit 形态 park 切点，真实可达面：CPU1 用户任务退出）。
    let picked: Option<*mut Task> = RR
        .pick_next_filtered(rq(), topo(), cur)
        .map(|n| n as *mut Task);
    if let Some(p) = picked {
        (*p).state = TaskState::Running;
    }
    rq_unlock(phys);
    let next_ptr: *mut Task = match picked {
        Some(p) => p,
        None if hal::smp::cpu_id() != 0 => {
            set_current(core::ptr::null_mut());
            if (*cur).ttbr0 != 0 {
                hal::activate_aspace(0, 0);
            }
            bump_switches(); // S-6 ①：递增+打印共享 helper（A12 兑现）
            hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
            note_switch(cur, core::ptr::null_mut(), SwitchKind::Exit);
            // 「-> ! 的 syscall 永不返回」同款：cur 已 dequeue 永不回——park
            // 切走后本上下文废弃（halt 不可达兜底保持）。
            klock_release_switch(cur);
            hal::task::context_switch(&raw mut (*cur).ctx, hal::smp::sec_park_ctx() as *const _);
            hal::halt();
        }
        None => addr_of_mut!(IDLE_TASK),
    };
    // 被选者置 Running 已入 RQ 窗（防偷屏障）；IDLE 臂恒 Running（static
    // 初值，无人改写）——本行旧置位为同值幂等，退役。
    (*next_ptr).slice_left = hal::timer::slice_cycles();
    // 授片即重编（同 yield_now；`调度器设计.md` §4.1 绝对截止时间纪律）——授片给
    // 真任务才 arm：切去 idle 不 arm（idle_loop 会 mask CNTV，arm 反引入一次空 tick）。
    // SAFETY: IRQ 已屏蔽（disable_irq 或异常入口硬件屏蔽）；写 CNTV 系统寄存器；EL1。
    if next_ptr != addr_of_mut!(IDLE_TASK) {
        hal::timer::arm_slice();
    }
    set_current(next_ptr);
    // S-6 ① 补数：退出直切臂（含切 IDLE）旧不计——同 park 臂口径补齐。
    bump_switches();
    hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
    activate_if_changed(cur, next_ptr);
    note_switch(cur, next_ptr, SwitchKind::Exit);
    // S-2 E0（草案 3.2 边界②收口点）：`-> !` 唯一放锁点——EXIT_HOOK（Process
    // 落账/wake_matching/drain）至本行全在锁窗内；放锁后仅剩 context_switch
    // 纯 asm（§3.4「放锁后零共享触达」）。SVC 路径位=1 放锁；内核任务直调
    // （exit_current/disable_irq 前无锁）位=0 幂等跳过。切走后 cur 永不回来
    //（位随 Task Box 进 REAPQ 废弃）。
    klock_release_switch(cur);
    // prev=cur（已出 runqueue、将废弃）；切走后永不回到 cur。
    hal::task::context_switch(&raw mut (*cur).ctx, &raw const (*next_ptr).ctx);
    // 不可达：cur 已 dequeue，无人切回。halt 兜底。
    hal::halt();
}

// ---- Plan B v0.5：任务退出钩子 + 待回收队列 ----

/// 任务退出钩子槽（反转通知，`set_user_fault_exit` 同模式——sched 不依赖
/// `内核` crate）。签名 `(pinned, thread_key, code)`：pinned = Task 钉住的
/// Process ObjRef 地址（0 = 无）；thread_key = Task 钉住的 Thread ObjRef
/// 地址（0 = 无——首线程/内核任务）；code = 退出码（fault/kill 路径 0）。
/// **在 cur 出 runqueue 之前、IRQ 屏蔽窗内调用**——钩子内唤醒/表操作与
/// syscall 主体同纪律。内核内部 C-ABI 槽（非用户 ABI——T-1 扩 thread_key）。
static mut EXIT_HOOK: Option<unsafe extern "C" fn(pinned: usize, thread_key: usize, code: i32)> =
    None;

/// 注册任务退出钩子（`内核` crate 语义层 boot 期调用，见 `syscall::install`）。
pub fn set_exit_hook(f: unsafe extern "C" fn(pinned: usize, thread_key: usize, code: i32)) {
    // SAFETY: boot 单线程注册；此后只读。
    unsafe { EXIT_HOOK = Some(f) };
}

/// 待回收任务队列（cap 32 ≈ 一个 bsh 会话的并发退出上限量级）。溢出 = 泄漏
/// + 首 4 个 loud 打印（`irq? id=` 同款防风暴）——回收是资源回收非正确性
/// 关卡，泄漏可见可接受。
static mut REAPQ: [*mut Task; REAPQ_CAP] = [core::ptr::null_mut(); REAPQ_CAP];
static mut REAPQ_LEN: usize = 0;
static mut REAPQ_WARNED: usize = 0;
const REAPQ_CAP: usize = 32;
/// REAPQ 互斥锁（S-3c c-2a 落形 → **c-2b slice 2 取放启用**；持有位 =
/// `HELD_REAPQ`，记账在 `reapq_push`/`reap_drain` 本面）。七层冻结锁序第三
/// 层；3-1b 起 KL 包络剥除——独立串行面（SLEEPQ_LOCK/RQ 锁同口径）。
static REAPQ_LOCK: SpinLock = SpinLock::new();

/// 压队（退出路径，IRQ 屏蔽窗内；cur 还在自家栈上——只入队不释放）。
/// REAPQ 锁窗（c-2b slice 2）：exit 链调用点在 RQ 窗内 → RQ>REAPQ 正序
/// 嵌套；kill 链调用点在 RQ/SQ 锁窗外（KL 窗内裸 REAPQ 窗）。溢出 loud
/// 打印在锁窗外（错误路径 ≤4 次，窗最短化——REAPQ>打印虽为冻结正序，
/// 错误面不占用锁序推理）。
unsafe fn reapq_push(t: *mut Task) {
    let phys = !held_test(HELD_REAPQ); // 位感知退化（c-2c slice 2a）
    if phys {
        reapq_acquire();
        held_mark(HELD_REAPQ, true);
    }
    let mut loud = false;
    if REAPQ_LEN < REAPQ_CAP {
        REAPQ[REAPQ_LEN] = t;
        REAPQ_LEN += 1;
    } else if REAPQ_WARNED < 4 {
        REAPQ_WARNED += 1;
        loud = true;
    }
    if phys {
        held_mark(HELD_REAPQ, false);
        reapq_release();
    }
    if loud {
        hal::uart_puts(b"reapq overflow (leak)\n");
    }
}

/// 回收退出任务的 Task Box + 内核栈（16KiB/个）。调用面 = `idle_loop` 稳态与
/// `yield_now` 切回后——**永不在被回收者自身上下文**。退出者已出 runqueue/
/// SLEEPQ（REAPQ 两源：自退出 [`switch_away_forever_with`] 从 Running 态；
/// C-4b `process_terminate` / T-1 `process_exit` 经 [`kill_all_tasks_by_pinned`]
/// 先摘队再钩子，从 Blocked（SLEEPQ）/ Ready（runqueue）两态——见其头注）+
/// 钩子已平衡 pinned/thread_key 引用（retain/release 成对；字段残留数值随
/// Box drop 无害）。
/// drop 纯释放零分配；REAPQ 锁串行一切 push/drain（c-2b slice 2——参与态
/// CPU1 锁窗退出链可 push，本读点不再单核顺序假设；3-1b 起 KL 包络剥除，
/// REAPQ 锁独立串行——锁窗正确性自足，KL 争用面随之消失）。
pub fn reap_drain() {
    // SAFETY: REAPQ 写点（push）全在 IRQ 屏蔽退出路径；参与态下 CPU1 锁窗
    // 退出链可 push——本读点持 REAPQ 锁串行。drain 后清零。drop 纯释放零
    // 分配（「IRQ 上下文不碰分配器」铁则不破——释放非分配）。
    unsafe {
        let phys = !held_test(HELD_REAPQ); // 位感知退化（c-2c slice 2a）
        if phys {
            reapq_acquire();
            held_mark(HELD_REAPQ, true);
        }
        let n = REAPQ_LEN;
        for i in 0..n {
            let p = REAPQ[i];
            REAPQ[i] = core::ptr::null_mut();
            if !p.is_null() {
                drop(alloc::boxed::Box::from_raw(p));
            }
        }
        REAPQ_LEN = 0;
        if phys {
            held_mark(HELD_REAPQ, false);
            reapq_release();
        }
    }
}

/// C-4b `process_terminate` / T-1 `process_exit`：按钉住键（Process ObjRef
/// 地址）杀**全部非当前**任务（terminate 的目标进程全部线程；process_exit
/// 的兄弟线程——调用者随后经 `switch_away_forever_with` 自退）。逐个走与
/// [`switch_away_forever_with`] 同一套收口（**不开第二退出面**）——区别仅在
/// 目标不是 CURRENT：无切换位，收尾是 REAPQ 而非 `context_switch`。返回
/// `(首杀任务名, 总杀数)`（观测打印用）；`None` = 无任务持该键（未 start 的
/// 进程——调用方只做 Process 侧收口）。**每任务走一次钩子**：末任务钩子完成
/// Process 收口（mark_exited + 码 + TERMINATED + drain + release 运行钉住）
/// ——terminate 不再做 started 路径的 Process 侧收口（防 drain 双跑/refs
/// 打穿）。
///
/// **外层重扫循环（T-1）**：钩子内 `wake_matching(TERMINATED)` 会摘 SLEEPQ
/// 匹配者入环（join 等待者被唤醒），不得持游标跨轮——每轮从 SLEEPQ 头 /
/// RQ 头全新扫描，杀一个再重扫，直到两队列均无匹配者。n≤任务总数，O(n²)
/// 无碍（terminate/process_exit 一次性路径）。
///
/// **摘队先于退出钩子**（与 `switch_away_forever_with` 的钩子→dequeue 序相反，
/// 有意为之）：钩子内 `wake_matching(TERMINATED)` 会摘 SLEEPQ 匹配者入环——
/// 病理形态下目标自己就在等它自己的 Process（父把子 process handle 经
/// bootstrap 递入、子 wait TERMINATED），钩子先跑会把目标从 SLEEPQ 摘走入环，
/// 随后的本函数摘队即撞「`SleepQueue::unlink` 不幂等」扬队坑。先摘队 = 目标
/// 已离场，钩子的唤醒面无论如何碰不到它。
///
/// **睡眠等待钉住泄漏（安全向）**：睡眠任务的 4-1 等待 retain（覆盖全部睡眠
/// 轮次、原定醒来出口 release）随任务永不续行而泄漏——wait_many 的多对象
/// retain 集合无法从 Task 结构恢复，泄漏是唯一安全 v1 选项（channel 端点互钉
/// 环泄漏同政策）；对象永活，无 UAF。
///
/// # Safety
/// IRQ 屏蔽窗内调用（syscall 主体天然满足）；零阻塞零让出（指针手术 + 钩子
/// + REAPQ 压队——「handler 不得忙等跨任务」铁则不破）。两臂结构锁窗（① SQ
/// / ② RQ）均**闭窗先于退出钩子/REAPQ 面**（c-2b：EXIT_HOOK 可
/// wake_matching 再入同锁 = 自锁面——② 臂 slice 1 / ① 臂 slice 4 立法）。
/// runqueue 扫描**跳过
/// CURRENT**（运行任务在环上且 pinned 匹配——process_exit 杀兄弟会命中自己：
/// 杀 CURRENT = 在自家栈上把自己塞进 REAPQ 后继续跑；terminate 语义下
/// CURRENT=调用者恒不匹配目标键，跳过是双保险）。SLEEPQ 无跳过面（CURRENT
/// 是 Running 恒不在 SLEEPQ）。
pub unsafe fn kill_all_tasks_by_pinned(
    pinned: usize,
    code: i32,
) -> Option<(&'static [u8], usize)> {
    if pinned == 0 {
        return None; // 内核任务/boot 裸表进程无钉住键——无任务面可杀
    }
    let mut first: Option<&'static [u8]> = None;
    let mut count: usize = 0;
    unsafe {
        loop {
            // ① SLEEPQ 扫描（Blocked：wait_one/wait_many/纯 sleep 的目标）。SQ 窗
            // （c-2b slice 4 自锁面第二形）：扫描+摘队+PI 重推发射同窗（发射点
            // 契约 = 五处一律窗内，钩子体 recompute_locked 不自取 SQ——F-L2
            // 不可重入）；**闭窗先于退出钩子/REAPQ 面**（EXIT_HOOK 可
            // wake_matching 再入 SQ = 同核自锁面——② RQ 臂 slice 1「先放锁
            // 再进钩子」同款立法）。
            let mut hit: Option<core::ptr::NonNull<Task>> = None;
            let sq_phys = sq_lock();
            let mut cur = sleepq().head.get();
            while let Some(c) = cur {
                let t = c.as_ptr();
                cur = (*t).sleep_next.get(); // 先存 next 再摘（unlink 清链接字段）
                if (*t).pinned == pinned {
                    hit = Some(c);
                    break;
                }
            }
            if let Some(c) = hit {
                let t = c.as_ptr();
                let tk = (*t).thread_key;
                sleepq().unlink(c); // 在队才摘（刚扫到——契约满足）
                (*t).state = TaskState::Exited;
                note_sleepq_changed(); // 睡眠者集合定型 → PI 全图重推（donor 边随离队消失）
                sq_unlock(sq_phys); // 窗闭先于钩子（EXIT_HOOK 内 wake_matching 再入 SQ）
                assert_exit_hook_windows_closed();
                if let Some(hook) = EXIT_HOOK {
                    hook(pinned, tk, code); // 同一套收口（线程簿记 + 末任务 Process 收口）
                }
                reapq_push(t);
                if first.is_none() {
                    first = Some((*t).name);
                }
                count += 1;
                continue; // 重扫（钩子唤醒可能已搬动两队列）
            }
            sq_unlock(sq_phys); // SLEEPQ 无匹配者——窗闭（② RQ 臂自持窗）
            // ② runqueue 环扫描（Ready；跳过 CURRENT——见 # Safety 注）。
            // RQ 窗（c-2b）：环走查读 + dequeue 同窗；命中后先放锁再进钩子/
            // REAPQ 面（EXIT_HOOK 可 wake_matching 入队 = 同核环同锁重入死锁
            // 面，绝不入窗）。
            let cur_task = current_task();
            let phys = rq_lock();
            let mut hit: Option<core::ptr::NonNull<Task>> = None;
            if let Some(h) = rq().head() {
                let mut cur = h;
                loop {
                    let p = cur.as_ptr();
                    if p != cur_task && (*p).pinned == pinned {
                        hit = Some(cur);
                        break;
                    }
                    // SAFETY: 环上节点必有 next（环形不变式）。
                    cur = (*p).rq_next.get().expect("ring node must have next");
                    if cur == h {
                        break;
                    }
                }
            }
            if let Some(c) = hit {
                let t = c.as_ptr();
                let tk = (*t).thread_key;
                RR.dequeue(&mut *t, rq()); // unlink（幂等）+ 置 Exited
                rq_unlock(phys);
                assert_exit_hook_windows_closed();
                if let Some(hook) = EXIT_HOOK {
                    hook(pinned, tk, code);
                }
                reapq_push(t);
                if first.is_none() {
                    first = Some((*t).name);
                }
                count += 1;
                continue;
            }
            rq_unlock(phys);
            break; // 两队列均无匹配者——收工
        }
    }
    first.map(|name| (name, count))
}

/// switches 金丝雀只读访问（watchdog / 遥测读；A12 原子化后锁外读合法）。
/// 任务↔任务切换计数（S-6 ① 双计数器契约）：只在 [`note_switch`] 非空臂
/// 单点自增 = 遥测环的精确分母（每次自增恰对应一次环写——hook 在场时）。
/// 缘起：SWITCHES 自 A12/S-6 起计全部 11 位（含 park 语境 4 位——3 park
/// 切点 + park 派发），而遥测环只记任务↔任务（note_switch 空参跳过发射）
///——G1.5「遥测环存活」断言旧比 SWITCHES 的契约在 A12 扩面时即过期，
/// S-6 复跑 3 实证踩中（total=456 < switches=457，park 事件先于断言落地）。
/// 金丝雀（switches= 行）继续走 SWITCHES 原口径——park 语境切换也是活性。
static SWITCHES_TT: AtomicU64 = AtomicU64::new(0);

pub fn switches() -> u64 {
    SWITCHES.load(Ordering::Relaxed)
}

/// 任务↔任务切换数（遥测环分母——G1.5 断言比较面，详 [`SWITCHES_TT`] 注）。
pub fn switches_tt() -> u64 {
    SWITCHES_TT.load(Ordering::Relaxed)
}

// ---- 切换遥测钩子（3-4c；`可观测性设计.md` per-CPU 无锁环的注入点）----

/// 切换原因分型（遥测环 `kind` 字段的语义；v1 三值 = 三个切换位一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchKind {
    /// 协作让出（`yield_now`）。
    Yield,
    /// 时间片到期抢占（`on_irq_tick`）。
    Preempt,
    /// 任务退出/被杀后切走（`switch_away_forever`——exit 与 kill 共用）。
    Exit,
    /// 受控睡眠切走（`block_current`——4-0；被唤醒/超时后经 Ready 入队切回）。
    Block,
}

/// 切换钩子签名。**调用契约**：IRQ 屏蔽路径调用（三切换位全程 PSTATE.I=1）
/// → 钩子本身无需加锁，但必须**零分配、零切换、短平快**（IRQ 热路径纪律，
/// `heap.rs`「分配上下文纪律」——try 化批堵住的 OOM-panic 面不得从这里重开）。
pub type SwitchHook = fn(prev: &Task, next: &Task, kind: SwitchKind);

/// 钩子槽（`None` = 无遥测——boot 期零开销；`telemetry::init` 注册）。
static mut SWITCH_HOOK: Option<SwitchHook> = None;

/// 注册/注销切换钩子。仅 boot 期（IRQ 屏蔽、首个切换发生前）调用。
pub fn set_switch_hook(hook: Option<SwitchHook>) {
    // SAFETY: 写点仅 boot 期（单核、IRQ 屏蔽、无并发切换）。
    unsafe { SWITCH_HOOK = hook };
}

/// 切换位点共用的遥测发射点：在 `CURRENT = next` 之后、`context_switch` 之前
/// 调（此刻 prev/next 的状态字段都已定型）。`None` 钩子 = 一次静态读 + 分支。
/// S-2b null 容忍：任一侧为 null（任务 ↔ park 上下文切换——park 非调度语义
/// Task，无 TCB 可记）跳过发射；遥测环只记任务↔任务切换。
#[inline]
unsafe fn note_switch(prev: *mut Task, next: *mut Task, kind: SwitchKind) {
    // 刀1 唤醒归因基线：切入任务/idle 语境即设 IPC（含 boot→首任务与
    // park↔任务 null 对——空对早退**前**先落：park 醒来在 IRQ 语境外的
    // wake_matching 同归任务侧基线；NONE 只属于副核首任务切入前的未接线窗，
    // 到达记账即绊网）。首个切换后 KERNEL_BOOT 归因冻结（G1.5 断言）。
    hal::wake::wake_src_task_ctx();
    if prev.is_null() || next.is_null() {
        return;
    }
    // S-6 ① 双计数器：非空臂 = 遥测环分母同点自增（环写与计数同窗——
    // G1.5 断言 tel >= switches_tt 的配平点；hook 缺席期（boot 前窗）也计
    //——telemetry::init 先于首个切换，实际不可达，防御性保持对称）。
    SWITCHES_TT.fetch_add(1, Ordering::Relaxed);
    // SAFETY: 两指针由调用点保证有效（刚做过状态写入）；IRQ 屏蔽路径。
    unsafe {
        let hook = SWITCH_HOOK;
        if let Some(h) = hook {
            h(&*prev, &*next, kind);
        }
    }
}

// ---- 唤醒遥测钩子（4-0 配套；`telemetry.rs` 唤醒环的注入点）----

/// 唤醒钩子签名。**调用契约**与 [`SwitchHook`] 同：IRQ 屏蔽路径调用（两个
/// 唤醒位：`wake_matching`（Signaled）/ `expire_timeouts`（Timeout））→ 钩子
/// 本身无需加锁，但必须**零分配、零切换、短平快**（IRQ 热路径纪律同 3-4c）。
pub type WakeHook = fn(task: &Task, reason: WakeReason);

/// 钩子槽（`None` = 无遥测；`telemetry::init` 注册）。
static mut WAKE_HOOK: Option<WakeHook> = None;

/// 注册/注销唤醒钩子。仅 boot 期（IRQ 屏蔽、首个切换发生前）调用。
pub fn set_wake_hook(hook: Option<WakeHook>) {
    // SAFETY: 写点仅 boot 期（单核、IRQ 屏蔽、无并发唤醒）。
    unsafe { WAKE_HOOK = hook };
}

/// 两个唤醒位共用的遥测发射点：在任务**摘队、置因、入 runqueue 之后**调
///（此刻状态字段定型；`wait_obj`/`wait_since` 唤醒不清除，钩子可读——正是
/// block→wake 延迟与等待对象的记录来源）。`None` 钩子 = 一次静态读 + 分支。
#[inline]
unsafe fn note_wake(t: *mut Task, reason: WakeReason) {
    // SAFETY: 指针由调用点保证有效（刚做过状态写入）；IRQ 屏蔽路径。
    // G3-4 bench-lat：无条件戳唤醒时刻（不随遥测钩子在否漂移——run 侧
    // 读点 wake_stamp_take 与本戳成对；cntvct 读在置因窗内，与 wait_since
    // 同窗纪律）。
    unsafe {
        (*t).wake_vct.set(hal::timer::cntvct());
    }
    let hook = unsafe { WAKE_HOOK };
    if let Some(h) = hook {
        h(unsafe { &*t }, reason);
    }
}

/// 取走**当前任务**的唤醒时刻戳（G3-4 bench-lat run 侧读点）：返回
/// `Task.wake_vct` 并清零（一次性消费——下次睡眠-唤醒周期由 `note_wake`
/// 重戳）。调用契约 = 刚从 `block_current` 醒来续行的锁窗内（rt_probe 的
/// wake_vct 读序同款：读点与醒点之间零 tick/切换噪声混入）；0 = 无在途
/// 戳（未睡先读 = 调用面违约，消费方自行 L0 断言）。
pub fn wake_stamp_take() -> u64 {
    // SAFETY: current_task 正统读法（per-CPU 槽优先——SMP 形态下 CURRENT
    // 静态从未被写（set_current 恒走槽路径）恒 null，读裸静态 = 空解引用
    // D-abort〔G3-4a 首跑实踩：FAR=0x320 = wake_vct 字段偏移〕）；调用
    // 契约（block_current 返后锁窗内）⇒ 非空（调用者即 current，副核
    // park null 空哨兵语境到不了这里）。读后清零同一窗，无并发写者
    // （note_wake 只在本任务沉睡时写）。
    // 跨核序边承重〔f4 F10〕：waker 的 wake_vct 写在其 SQ 释放（Release）
    // 窗内 → sleeper 经 klock_acquire_switch 的 SQ 获取（Acquire）于
    // block_current 返回前重取该值——边成立依赖这对 SQ 序；未来若拆 wait
    // 路的 SVC 屏蔽窗 SQ 位，本取戳的单消费值序论证随之重审。
    unsafe {
        let cur = current_task();
        let v = (*cur).wake_vct.get();
        (*cur).wake_vct.set(0);
        v
    }
}

// ---- SLEEPQ 变更钩子（4-3b PI；内核 crate 全图重推的注入点）----

/// SLEEPQ 变更钩子：`block_current` 睡下后（切走前）与两个唤醒位
/// （`wake_matching`/`expire_timeouts`）摘队完成后各发射一次——此刻睡眠者
/// 集合已定型。与 [`WakeHook`] 同纪律：**零分配、零切换、短平快**（IRQ
/// 屏蔽热路径）。消费者 = 内核 crate 的 PI 全图重推（`kernel/src/pi.rs`）。
pub type SleepqHook = fn();

/// 钩子槽（`None` = 无 PI；`pi::init` 注册）。
static mut SLEEPQ_HOOK: Option<SleepqHook> = None;

/// 注册/注销 SLEEPQ 变更钩子。仅 boot 期（IRQ 屏蔽、首个切换发生前）调用。
pub fn set_sleepq_hook(hook: Option<SleepqHook>) {
    // SAFETY: 写点仅 boot 期（单核、IRQ 屏蔽、无并发睡眠）。
    unsafe { SLEEPQ_HOOK = hook };
}

/// 睡眠者集合变更的发射点（见 [`SleepqHook`] 契约）。`None` 钩子 = 一次静态
/// 读 + 分支。
#[inline]
unsafe fn note_sleepq_changed() {
    let hook = unsafe { SLEEPQ_HOOK };
    if let Some(h) = hook {
        h();
    }
}

// ---- S-3b 步 b-2 LB 金丝雀记账（子案 §9.6 义务③/⑤；kernel crate 注入判别）----

/// LB 子任务判别注入槽（kernel 侧读自己的 `LB_PROCS[4]` 比对 `thread_key`）。
/// **架构无关纪律**：sched 不持内核对象知识，「哪些任务是 LB 线程」归 kernel
/// （set_wake_hook/policy::bind 同款注入先例）。仅 boot 期（IRQ 屏蔽、lb
/// gate 块内）注册；`None` = 无 LB 相（常规 boot 零开销——采样点一次静态
/// 读 + 分支，同 hook 族形）。
static mut LB_MATCH: Option<fn(key: usize) -> bool> = None;

/// 注册 LB 子任务判别（仅 boot 期 lb gate 块调用）。键 = `thread_key`
/// （每线程唯一——thread_start 造的计算线程逐只入集，进程 root 为 Process
/// 键天然不入：per-child 归因零 root 污染）。
pub fn set_lb_match(f: Option<fn(key: usize) -> bool>) {
    // SAFETY: 写点仅 boot 期（单核、IRQ 屏蔽、tick/steal 采样未启）。
    unsafe { LB_MATCH = f };
}

/// per-CPU LB 子任务 tick 采样计数（义务③ per-child 归因：本核 tick 打在
/// LB 子身上一次 +1——偷到 CPU1 的子在其上跑、tick 记到 c1；直接路由与
/// 偷取同格合格）。Relaxed：纯计数无序角色。
static LB_TICKS: [core::sync::atomic::AtomicU64; 2] =
    [core::sync::atomic::AtomicU64::new(0), core::sync::atomic::AtomicU64::new(0)];

/// 全局偷取事件计数（st1——`LB ok` 行的诊断字段，9.2 裁决二降格：不做
/// 断言面，c0/c1 才是偷取活证断言）。不筛 LB 子：偷取事件本身稀有，全局
/// 计数对排障（F-A9 静默杀死状同形族）信息量最大。
static LB_STEALS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// LB 统计快照（`(c0, c1, st1)`）——lb_master 在 klock 包络内调（F-A8 Δ
/// 采样点纪律：首入队前/末收割后两快照差值），归因原子性由锁窗序保证。
pub fn lb_stats() -> (u64, u64, u64) {
    (
        LB_TICKS[0].load(Ordering::Relaxed),
        LB_TICKS[1].load(Ordering::Relaxed),
        LB_STEALS.load(Ordering::Relaxed),
    )
}

// ---- 睡眠/唤醒原语（4-0；`IPC设计.md` 阶段化目标 Phase 4 拆步表步 4-0）----

/// `wait_many` 的等待键哨兵（4-2b）：Task 只有单一 `wait_obj` 匹配键，多对象
/// 等待者挂本哨兵——`wake_matching` 对其「任何对象、任何新置位」都命中，醒来
/// 由调用方重验循环定夺（虚假唤醒无害；v1 等待者个位数，全队列误醒开销可忽略）。
///
/// W3-Q2 起单源在 `berry_wqmatch::WAIT_ANY`（host 可测纯面 crate）；此处
/// re-export 维持既有引用路径零改动（`berry_sched::WAIT_ANY` 面——kernel
/// pi.rs 等消费方不动）。值不变（usize::MAX）。
pub use berry_wqmatch::WAIT_ANY;

/// 全局睡眠队列（SLEEPQ）：侵入式双向 + 按 `wait_deadline` 升序（最近到期在头
/// ——超时扫描头未到期即早停）。
///
/// v1 **单队列匹配唤醒**而非「每对象一个 waitq + 超时队列双挂」：Task 只有一对
/// 睡眠链接字段，双挂一致性易错；唤醒方按 `wait_obj`+`wait_signals` 遍历匹配
/// （等待者通常个位数，O(n) 无忧）。§2.1「等待队列按优先级」语义随 4-3 在唤醒
/// 选择侧兑现（遍历选优），队列改 prio 序留优化。
///
/// **非环形**双向链表（与 runqueue 的环形不同）：单元素时 head==tail==该任务、
/// 其 prev/next 均 `None`——与「不在队上」无法靠链接字段区分，故 [`SleepQueue::unlink`]
/// **不做幂等**（调用方契约：只在确知在队时摘——三个调用点都满足）。
struct SleepQueue {
    head: Cell<Option<NonNull<Task>>>,
    tail: Cell<Option<NonNull<Task>>>,
    /// 队长（idle_loop 的「有无睡眠者」判定只读；屏蔽窗内读写，Cell 以便
    /// `&self` 方法维护——同 head/tail）。
    len: Cell<usize>,
}

static mut SLEEPQ: SleepQueue = SleepQueue {
    head: Cell::new(None),
    tail: Cell::new(None),
    len: Cell::new(0),
};

/// 取全局 SLEEPQ 的 `&mut`（静态单例；单核顺序访问，借用不重叠）。
#[inline]
unsafe fn sleepq() -> &'static mut SleepQueue {
    &mut *addr_of_mut!(SLEEPQ)
}

// ---- per-key 等待计数门控（P1：wake_matching 无等待者 O(1) 快路径）----
//
// 架构研究 P1 瓶颈：每次 channel_send/signal/wake 调 `wake_matching` 都
// 全扫 SLEEPQ 链表——N 任务并发睡眠时每次唤醒成本线性涨。本表按等待键
// 粗分桶计数睡眠者，`wake_matching` 入口查桶：**计数 == 0 ⇒ 队上必无该键
// 等待者，O(1) 跳过全扫**（常见「无等待者」场景——send 打空 channel、
// signal 无监听者——彻底免扫）。
//
// 计数维护收口在 [`SleepQueue::insert_sorted`]/[`SleepQueue::unlink`] 两
// 函数——SLEEPQ 成员变更的**仅有**机械面（block/sleepq_insert 入、
// wake_one_inner/expire_timeouts/kill/sleepq_unlink 出，全走这两函数），
// 加减天然成对，无第三写点可漏。
//
// **粗计数语义**（只许假阳性、不许漏醒）：桶按哈希分键，冲突桶非零但无
// 匹配者 = 假阳性 → 照走慢路径，正确性不受影响；反向（桶零但队上有匹配
// 者）由计数收口点封死——add 先于接线、sub 后于摘线（见两函数注释）。

/// 桶数（64 static 桶——零分配；`align(64)` 逐桶缓存行对齐防伪共享）。
const WAIT_BUCKETS: usize = 64;

/// 单桶：桶内等待任务总数（粗计数——键哈希进本桶的睡眠者个数）。
#[repr(C, align(64))]
struct WaitBucket {
    count: AtomicU32,
}

/// 桶零初值（const 具名件——数组重复表达式免 Copy 的稳定形）。
const ZERO_WAIT_BUCKET: WaitBucket = WaitBucket {
    count: AtomicU32::new(0),
};

/// per-key 等待计数表（64 桶 static）。写点全在 SQ 锁窗内（SLEEPQ 两机械
/// 函数收口），读点 = [`wake_matching_locked`] 门控（同窗）——A12 纪律：
/// 原子 static（非 static mut），与 IRQ_TICKS/LB_TICKS 同款，免入册
/// writable_globals（该门禁只执法 static mut / UnsafeCell 直写形态）。
static WAIT_HASH: [WaitBucket; WAIT_BUCKETS] = [ZERO_WAIT_BUCKET; WAIT_BUCKETS];

/// [`WAIT_ANY`] 哨兵等待者（wait_many 收割者）总数。哨兵匹配**任何** obj
/// 的唤醒（`matches_wake` 键门 Any 分支恒真），单键桶罩不住——独立计数，
/// 门控并查（非零 = 任何键都可能命中哨兵，一律慢路径）。
static WAIT_ANY_WAITERS: AtomicU32 = AtomicU32::new(0);

/// 桶号：乘法散列（Fibonacci hashing）取高 6 位——`>> 58` 已落 [0,64)，
/// `% WAIT_BUCKETS` 保险钉域（改桶数时防越界）。
#[inline]
fn wait_bucket(key: usize) -> usize {
    (key.wrapping_mul(0x9E3779B97F4A7C15) >> 58) as usize % WAIT_BUCKETS
}

/// 睡眠者入队计数（+1）——[`SleepQueue::insert_sorted`] 入口调。
/// 三形分账：纯睡（obj=0）永不匹配任何唤醒（wake_matching 入口拒 +
/// 谓词 PureSleep 键门恒 false）——不计数，减一处假阳性源。
#[inline]
fn wait_count_add(obj: usize) {
    if obj == 0 {
        return; // 纯睡者不在任何唤醒集内
    }
    if obj == WAIT_ANY {
        WAIT_ANY_WAITERS.fetch_add(1, Ordering::Relaxed);
    } else {
        WAIT_HASH[wait_bucket(obj)].count.fetch_add(1, Ordering::Relaxed);
    }
}

/// 睡眠者离队计数（-1）——[`SleepQueue::unlink`] 尾调。与 [`wait_count_add`]
/// 同判据分账（同一 `wait_obj` 值——字段在两调用点之间无写者，加减恒对称；
/// 睡眠多轮换键者每轮 block_current 先重写 wait_obj 再入队，逐轮配对）。
#[inline]
fn wait_count_sub(obj: usize) {
    if obj == 0 {
        return;
    }
    if obj == WAIT_ANY {
        WAIT_ANY_WAITERS.fetch_sub(1, Ordering::Relaxed);
    } else {
        WAIT_HASH[wait_bucket(obj)].count.fetch_sub(1, Ordering::Relaxed);
    }
}

/// 门控判据：键桶 + 哨兵计数**双零** → SLEEPQ 上必无「obj 键相等或 WAIT_ANY
/// 哨兵」的等待者（谓词键门仅此两形可命中），可跳过全扫。单核 IRQ 屏蔽窗
/// 内读（计数写点同窗——无竞态）；SMP 实验态读点在 SQ 锁窗内、写点同窗，
/// 锁交接即计数可见性边界——无跨核 staleness 漏醒面。
#[inline]
fn wait_gate_clear(obj: usize) -> bool {
    WAIT_ANY_WAITERS.load(Ordering::Relaxed) == 0
        && WAIT_HASH[wait_bucket(obj)].count.load(Ordering::Relaxed) == 0
}

/// SLEEPQ 互斥锁（S-3c c-2a：锁族落形 + SQ 窗先行——§3 七层冻结锁序最顶层）。
///
/// **过渡态（c-2a/b）语义**：SVC 入口 KL 全量不动 ⇒ 本锁一切取放点都在 KL
/// 窗内 ⇒ 恒单写者、零争用（真原子互斥尚未启用——**窗形先定形，行为零差**；
/// c-2c C20 收缩后才成为真互斥面）。**过渡态逆序标注（勿引为终态锁序）**：
/// 本锁嵌于 KL 窗内 = KL > 结构锁，与终态七层序（结构锁独立分号、SQ 居
/// 首）方向相反——KL 单持有者串行一切结构锁取放点，此嵌套方向上死锁结构
/// 不可能；C20 收缩后 SQ 号路径脱离 KL 窗，本逆序边随之消解（判词 → 子案
/// §9.7 c-2 拆解段）。
///
/// 窗纪律：取放须在 IRQ 屏蔽窗内（既有 SLEEPQ 面契约，雷 2 判例——同核
/// IRQ 重入 `expire_timeouts` 再取 = 自死锁）；窗内禁切换（终态立法前瞻——
/// c-2a 起一切 SQ 窗在切点前释放）；不可重入（TTAS——跨函数同窗用
/// `_locked` 内层分层）。
static SLEEPQ_LOCK: SpinLock = SpinLock::new();

/// 取 SLEEPQ 锁（含持有位记账）。pub 供 kernel 侧 fastpath 义务④跨窗
///（find→push→fast_wake 同窗，`syscall.rs` 调用方持窗——窗内禁切换故
/// 直切判定在窗外）。
///
/// **位感知退化形（c-2c slice 2a）**：位已置（外层逻辑持有）→ 不物理取、
/// 返 `false`；配对 [`sq_unlock`] 收 `false` 同退化跳过（物理持有归外层）。
/// 2a 全号仍全谱 ⇒ 入口位集恒零 ⇒ 退化支不可达（机制惰性就绪，零行为差）。
pub fn sq_lock() -> bool {
    // SAFETY: 位读当前任务字段；CURRENT 空窗恒 false（照常真取）。
    if unsafe { held_test(HELD_SQ) } {
        return false; // 位已置——外层逻辑持有，物理退化（c-2c slice 2a）
    }
    sq_acquire();
    // SAFETY: 记账位写当前任务字段；CURRENT 空窗由 held_mark 自跳过。
    unsafe { held_mark(HELD_SQ, true) };
    true
}

/// 放 SLEEPQ 锁（先清位后放——位与锁恒同窗；`phys=false` 退化窗不触——
/// 配对 [`sq_lock`] 的返回值）。
pub fn sq_unlock(phys: bool) {
    if !phys {
        return; // 退化窗——物理持有归外层
    }
    // SAFETY: 同上。
    unsafe { held_mark(HELD_SQ, false) };
    sq_release();
}

impl SleepQueue {
    /// 按 `wait_deadline` 升序插入；相等插相等段**尾**（同 deadline FIFO——
    /// 先睡先醒，扫描顺序可复现）。
    ///
    /// # Safety
    /// IRQ 屏蔽窗内；`task` 不在 SLEEPQ 上（链接字段均 `None`）且地址稳定。
    unsafe fn insert_sorted(&self, task: NonNull<Task>) {
        let t = task.as_ptr();
        // 入队执法断言（复盘雷 2 残留诊断，同 runqueue link_back 面）：对在队
        // 者重复插入会覆盖写 sleep_prev/sleep_next——非环形链表上直接扬队
        //（被覆盖的旧邻接缝指向失效）。契约「不在 SLEEPQ 上」此前零执法。
        // **单元素例外**（类型注释）：单元素队成员 prev/next 均 None——字段
        // 判据对它失效，须并上「head == 自己」才完整。
        let in_q = (*t).sleep_prev.get().is_some()
            || (*t).sleep_next.get().is_some()
            || self.head.get() == Some(task);
        assert!(!in_q, "sleepq insert: task already in queue");
        // P1 等待计数：入队 +1——**先于接线**（见 wait_count_add 注释；与
        // unlink 尾部 -1 对称成窗：窗口期桶只多不少 → 门控只假阳性不漏醒）。
        wait_count_add((*t).wait_obj);
        // 从头找第一个 `deadline > 新者` 的节点，插它前面；到尾则插尾。
        let mut cur = self.head.get();
        let before = loop {
            match cur {
                None => break false, // 走到尾 → 插尾
                Some(c) => {
                    if (*c.as_ptr()).wait_deadline > (*t).wait_deadline {
                        break true; // c 是第一个更远者 → 插 c 前
                    }
                    cur = (*c.as_ptr()).sleep_next.get();
                }
            }
        };
        if before {
            // SAFETY: 调用方契约（在队节点有效）；此处以 cur 的 prev/next 接线。
            let n = cur.unwrap_unchecked().as_ptr();
            let prev = (*n).sleep_prev.get();
            (*t).sleep_prev.set(prev);
            (*t).sleep_next.set(cur);
            match prev {
                Some(p) => (*p.as_ptr()).sleep_next.set(Some(task)),
                None => self.head.set(Some(task)),
            }
            (*n).sleep_prev.set(Some(task));
        } else {
            match self.tail.get() {
                Some(old) => {
                    (*old.as_ptr()).sleep_next.set(Some(task));
                    (*t).sleep_prev.set(Some(old));
                }
                None => self.head.set(Some(task)), // 空队 → 新头
            }
            (*t).sleep_next.set(None);
            self.tail.set(Some(task));
        }
        self.len.set(self.len.get() + 1);
    }

    /// O(1) 双向摘除。**不幂等**（非环形链表无法廉价判在队，见类型注释）——
    /// 调用方须确知 `task` 在 SLEEPQ 上；对不在队者调用会扬掉整队（head/tail
    /// 被误清）。`prev`/`next` 为 `None` 合法（队长/队尾——单元素时两者皆
    /// `None` 但在队上，正是无法幂等判定的根源）。
    ///
    /// # Safety
    /// IRQ 屏蔽窗内；`task` 当前在 SLEEPQ 上。
    unsafe fn unlink(&self, task: NonNull<Task>) {
        let t = task.as_ptr();
        let prev = (*t).sleep_prev.get();
        let next = (*t).sleep_next.get();
        match prev {
            Some(p) => (*p.as_ptr()).sleep_next.set(next),
            None => self.head.set(next),
        }
        match next {
            Some(n) => (*n.as_ptr()).sleep_prev.set(prev),
            None => self.tail.set(prev),
        }
        (*t).sleep_prev.set(None);
        (*t).sleep_next.set(None);
        self.len.set(self.len.get() - 1);
        // P1 等待计数：离队 -1——**后于摘线**（wait_obj 此刻仍是入睡时键——
        // 链接字段清空不影响它；与 insert_sorted 入口 +1 对称）。
        wait_count_sub((*t).wait_obj);
    }
}

/// 受控睡眠（4-0）：当前任务挂上 SLEEPQ 等待「`obj` 上出现与 `signals` 相交的
/// 信号」或 `deadline_vct` 超时（`u64::MAX` = 无限等），切走；被唤醒方/超时扫描
/// 摘队入 runqueue、此后被调度切回，返回唤醒原因。
///
/// **调用方须保证 IRQ 屏蔽窗**（SVC 上下文天然满足；内核任务显式 `disable_irq`）
/// ——与 `yield_now` 同款承载不变式（模块注释 Bug 2）。单核屏蔽窗原子 =
/// 「入队+Blocked+切走」不可分割，唤醒方「摘队+Ready+入环」同窗——无丢唤醒/
/// 虚假唤醒（SMP(G2) 重做）。
///
/// **调用方须已验条件不满足**（本函数不验——验条件与入队之间的窗口正是屏蔽窗
/// 要消灭的竞态，条件语义归调用方）；唤醒后**必须重验条件**（Timeout 时条件
/// 可能已真——signal 与到期竞争、先到先摘）。
///
/// **续行时的 IRQ 状态**：屏蔽续行（切回点在唤醒方的屏蔽窗或 IRQ 窗口内）。
/// SVC 语境勿解蔽（eret 自然恢复 SPSR——syscall 主体不可抢占免锁根基）；
/// 内核任务语境续行后自行 `enable_irq`（或走 `yield_now` 出口解蔽）。
pub fn block_current(obj: usize, signals: u64, deadline_vct: u64) -> WakeReason {
    // SAFETY: 调用方契约 = IRQ 屏蔽窗；CURRENT 有效；SLEEPQ 单核顺序访问。
    unsafe {
        assert_preempt_balanced("block_current");
        let cur = current_task();
        // S-3c c-2a：SQ 窗开（义务③「查位+入队同窗」段——等待字段四写 +
        // insert_sorted + rq unlink + 置 Blocked + PI 收尾重推，一体不分割）。
        // 窗内含 pick/RQ 读改（终态 SQ>队列序正确嵌套）；**切点前必闭窗**
        //（下方两路 klock_release_switch 前）——终态「结构锁窗内禁切换」
        // 前瞻。过渡态 KL 全量在手 ⇒ 本窗恒零争用（SLEEPQ_LOCK 注释）。
        let sq_phys = sq_lock();
        // 填等待描述（唤醒方 wake_matching 的匹配依据）+ 入睡时刻戳（唤醒
        // 遥测环记 block→wake 延迟——`wait_since` 与 deadline 同窗填，无竞态）。
        (*cur).wait_obj = obj;
        (*cur).wait_signals = signals;
        (*cur).wait_deadline = deadline_vct;
        (*cur).wait_since = hal::timer::cntvct();
        (*cur).wakeup.set(WakeReason::Pending);
        // 入 SLEEPQ（升序）+ 出 runqueue 环 + 置 Blocked。Blocked 任务绝不能
        // 留在环上（rotate 会选中它）——RR.dequeue 会置 Exited（语义不符），
        // 故直接 Runqueue::unlink + 手置 Blocked。
        let nn = NonNull::new_unchecked(cur);
        sleepq().insert_sorted(nn);
        // RQ 窗（c-2b，嵌 SQ 窗内——终态 SQ>队列 正序嵌套）：摘环 + 置
        // Blocked + 出选 + 被选者置 Running 同窗（Blocked 落定防本环再选中
        // 入睡者——eligible 只排 Running；Running = 防对核偷取屏障，均须
        // 放锁前落定）。
        let rq_phys = rq_lock();
        rq().unlink(nn);
        (*cur).state = TaskState::Blocked;
        // 切走：next = pick（runqueue 空 → idle——idle_loop 已按「SLEEPQ 有货则
        // arm tick 扫超时」改板，睡眠者不会被永睡饿死）。
        let picked: Option<*mut Task> = RR
            .pick_next_filtered(rq(), topo(), cur)
            .map(|n| n as *mut Task);
        if let Some(p) = picked {
            (*p).state = TaskState::Running;
        }
        rq_unlock(rq_phys);
        let next_ptr: *mut Task = match picked {
            Some(p) => p,
            None if hal::smp::cpu_id() != 0 => {
                // S-2b park 切点（block 形态，真实可达面：CPU1 用户任务入睡后环上
                // 无本核可见货）——先发 PI 重推（note_sleepq_changed 在原位 :944
                // 语义保持：睡眠者集合定型先于切走）再存档 park。被唤醒方摘队入
                // 环后，由 CPU1 park 派发切回本行续行（wakeup 原因在此消费）。
                // **闭窗先于 CURRENT 翻转**（c-2a 排障定谳）：sq_unlock 的
                // held_mark 绑定 CURRENT——翻转后闭窗会把窗主（入睡者）的持有
                // 位清到 park 空态（跳过）= 位终身不清；正常分支同款铁则。
                note_sleepq_changed();
                sq_unlock(sq_phys); // SQ 窗闭（park 切点前 + CURRENT 翻转前）
                set_current(core::ptr::null_mut());
                if (*cur).ttbr0 != 0 {
                    hal::activate_aspace(0, 0);
                }
                bump_switches(); // S-6 ①：递增+打印共享 helper（A12 兑现）
                hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
                note_switch(cur, core::ptr::null_mut(), SwitchKind::Block);
                klock_release_switch(cur);
                hal::task::context_switch(&raw mut (*cur).ctx, hal::smp::sec_park_ctx() as *const _);
                // <- 被 park 派发切回时续行于此（IRQ 仍屏蔽——调用方契约窗内）。
                klock_acquire_switch(cur);
                return (*cur).wakeup.get();
            }
            None => addr_of_mut!(IDLE_TASK),
        };
        // 被选者置 Running 已入 RQ 窗（IDLE 臂恒 Running 同值幂等退役）。
        // 授片即重编（`调度器设计.md` §4.1 绝对截止时间纪律；同
        // switch_away_forever：切去 idle 不 arm——idle_loop 自管 CNTV）。
        if next_ptr != addr_of_mut!(IDLE_TASK) {
            (*next_ptr).slice_left = hal::timer::slice_cycles();
            // SAFETY: IRQ 已屏蔽；写 CNTV 系统寄存器；EL1。
            hal::timer::arm_slice();
        }
        // 4-3b：新睡眠者已入队、状态定型——PI 全图重推（写 pi/eff 字段，不碰
        // 链接；屏蔽窗内零分配）后才切走。
        // **闭窗先于 set_current 翻转**（c-2a 排障定谳）：sq_unlock 的
        // held_mark 绑定 CURRENT——翻转后闭窗 = 入睡者的位清到被选者头上
        //（窗主位终身不清 + 误清被选者位，双向投毒；fa_pong bit1 终身携带
        // 实证）。结构窗闭窗点必须仍在窗主上下文——全域切换点唯 block 双
        // 分支犯此序，本处即修。
        note_sleepq_changed();
        sq_unlock(sq_phys); // SQ 窗闭（CURRENT 翻转前——窗主上下文闭窗）
        set_current(next_ptr);
        // S-6 ① 补数：入睡直切臂（出选有货）旧不计——服务 IPC 蓄睡/唤醒往返
        // 的主干切点，漏计=金丝雀盲区（SMP 稳态 park 臂结构性冷）。
        bump_switches();
        hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
        activate_if_changed(cur, next_ptr);
        note_switch(cur, next_ptr, SwitchKind::Block);
        // S-2 E0 舞蹈（草案 §2.3）：SLEEPQ 入队后放锁入睡（睡眠点先放锁 = 持锁者
        // 永不睡眠的执法点；多轮睡眠的重复放/取靠随 Task 位不错配）。
        klock_release_switch(cur);
        // SAFETY: IRQ 已屏蔽（调用方契约）；prev/next 有效、不别名。
        hal::task::context_switch(&raw mut (*cur).ctx, &raw const (*next_ptr).ctx);
        // S-2 E0：被唤醒方/超时扫描摘队入 runqueue、此后被 pick 切回，续行于此
        //——位=1（SVC wait 路径）则重取，重验循环回锁内；内核任务位=0 跳过。
        klock_acquire_switch(cur);
        // <- 被唤醒方/超时扫描摘队入 runqueue、此后被 pick 切回，续行于此
        // （此刻 CURRENT==cur，IRQ 仍屏蔽——语义见函数注释）。
        (*cur).wakeup.get()
    }
}

/// 单等待者唤醒内部序列（4-4 抽取）：摘队→置因→入环→唤醒遥测——`wake_matching`
/// 的每匹配者与 fastpath 定点唤醒（[`fast_wake_locked`]）**共用此一处**（「分层不是
/// 特例，不长第二套」——两路唤醒序列一字不差，PI 收尾重推同样经调用方各自
/// 发射 [`note_sleepq_changed`]）。
///
/// # Safety
/// IRQ 屏蔽窗内 + **SQ 锁已持**（c-2a 起两路调用方都在窗内）；`t` 当前在
/// SLEEPQ 上（调用方契约同 `SleepQueue::unlink`——对不在队者调用会扬掉
/// 整队）。
unsafe fn wake_one_inner(t: NonNull<Task>) {
    // SAFETY: 契约如上；SLEEPQ 单核顺序访问。
    unsafe {
        let p = t.as_ptr();
        sleepq().unlink(t);
        (*p).wakeup.set(WakeReason::Signaled);
        enqueue_with_kick(&mut *p, (*p).home_cpu as usize); // 置 Ready + 回 home 队入环
        note_wake(p, WakeReason::Signaled);
    }
}

/// 条件方唤醒（4-0）：摘除所有「`wait_obj == obj` 且请求位与 `set_signals`
/// 相交」的睡眠者——置 Signaled、入 runqueue（**不立即抢占**：唤醒方自身不切
/// 换，等 tick/yield 再切；4-3a 起 pick 按 `prio_effective` 出选——被唤醒/
/// boost 者下一轮 pick 自然优先，无优先级抢占点；4-4 fastpath 例外——见
/// [`try_preempt`] 的直切点）。返回唤醒数。**IRQ 屏蔽窗内调用**（signal /
/// 数据到达路径——4-1 起 Event signal、Channel send/recv；4-0 消费者 = boot
/// waker 验收任务）。
///
/// `obj == 0` 非法（0 = 纯 sleep，只能超时自醒）——直接返回 0。纯 sleep 者
/// `wait_signals == 0`，与任何掩码不相交，本就不会被误摘。
///
/// [`WAIT_ANY`] 等待者（wait_many，4-2b）无条件命中任何 `set_signals != 0` 的
/// 唤醒——醒来重验归调用方（哨兵 ≠ 真对象地址，永不与 `obj` 相等，靠谓词的
/// 哨兵分支命中）。
///
/// **b-1 排障定谳（S-3b）**：唤醒面 = 纯广播，**公平/上限绝不进唤醒选择器**。
/// 曾试「按 `wait_since` 到达序醒 1（send/recv 容量配对）」——广播同集合仅
/// 换序即断 CHAIN 链：到达序是**内生量**（唤醒序→入队序→运行序→重睡序→
/// 到达序闭环，选择器零熵），首个被选者永久锁死独占腾出的每个槽（实测
/// user_child 连胜 60/60 轮，pinger/ponger 链 30s 断供）。容量槽公平归
/// 放置面（[`place`] 的 thread/process 切分）与 b-2 偷取迁移，不归唤醒面。
///
/// **S-3c c-2a 起**：全体包 SQ 锁窗（扫描摘队 + PI 收尾一体；窗内
/// `enqueue_with_kick` 的 RQ 手术 = 终态 SQ>队列序正确嵌套）。**c-2b
/// slice 4 起**：本体 = 取放锁薄壳 + 持窗版 [`wake_matching_locked`]（自锁
/// 面：IRQ 生产者「写位+扫描同窗」复用；#[inline] 内联回原指令流）。
///
/// **P1 门控**（per-key 等待计数）：入口先查 [`wait_gate_clear`]——该键
/// 桶 + 哨兵计数双零 ⇒ 队上必无匹配者，O(1) 返回免全扫（无等待者的
/// send/signal 常见场景）；非零（含哈希冲突假阳性）走原有全扫，语义不变。
pub fn wake_matching(obj: usize, set_signals: u64) -> usize {
    if obj == 0 {
        return 0;
    }
    let phys = sq_lock(); // SQ 窗开（写位+扫描同窗——义务③/④对偶面）
    let n = wake_matching_locked(obj, set_signals);
    sq_unlock(phys); // SQ 窗闭
    n
}

/// [`wake_matching`] 的持窗版（c-2b slice 4 自锁面第三形——IRQ 生产者消费）：
/// **须已持 SLEEPQ 锁**（`sq_lock`）且同窗配对 `sq_unlock`——扫描摘队 + PI
/// 收尾发射一体（与自持窗版同体，仅去取放锁）；窗内禁切换。消费面 =
/// `rx_irq_notify`（F-L2 第三处「IRQ 上下文 SQ 面」）：**写位+扫描同窗**（义务③
/// IRQ 面）——Event 位写与本扫描同一 SQ 窗内原子，防分段后「位已置而队上
/// 无等待者」窗口丢唤醒。`#[inline]` 使自持窗版壳内联回原指令流（icount
/// 等形——slice 1-3 先例同纪律）。
///
/// P1 门控在本版入口（SQ 锁窗内——计数读写点同窗）：[`wait_gate_clear`]
/// 双零即 O(1) 返回 0，免全扫与 PI 钩子发射（队未动）。
#[inline]
pub fn wake_matching_locked(obj: usize, set_signals: u64) -> usize {
    if obj == 0 {
        return 0;
    }
    // P1 门控：该键桶 + 哨兵计数双零 → 队上必无匹配者（谓词键门两形：
    // obj 相等 / WAIT_ANY——计数同两形罩全），O(1) 返回免全扫。判位在
    // SQ 锁窗内（自持窗版壳先取锁后进本体系；持窗直调者同窗）——计数
    // 一读写点同窗，单核屏蔽窗无竞态、SMP 实验态锁交接即发布。假阳性
    // （冲突桶非零）照走下方慢路径，唤醒语义逐字不变。
    if wait_gate_clear(obj) {
        return 0; // 无等待者——队未动，PI 钩子（note_sleepq_changed）免发
    }
    let mut n = 0;
    // SAFETY: 调用方持窗（屏蔽窗等价）；SLEEPQ 单核顺序访问；遍历中摘除的
    // 节点仍有效（调度器拥有其 Box）。
    unsafe {
        let mut cur = sleepq().head.get();
        while let Some(c) = cur {
            let t = c.as_ptr();
            cur = (*t).sleep_next.get(); // 先存 next 再摘（unlink 清链接字段）
            // W3-Q2：谓词单源 berry_wqmatch::matches_wake（键门+位门；
            // b-1 广播不变量 host 钉板面）。手写内联逻辑与此逐字等价。
            let d = berry_wqmatch::WaiterDesc {
                key: berry_wqmatch::WaitKey::from_raw((*t).wait_obj),
                wait_signals: (*t).wait_signals,
            };
            if berry_wqmatch::matches_wake(&d, obj, set_signals) {
                wake_one_inner(c);
                n += 1;
            }
        }
    }
    // 4-3b：摘队完成 = 睡眠者集合定型（离队者冻结 boost、在队者重算捐赠图）。
    // SAFETY: 同上；发射点契约 = SQ 窗内（钩子体 recompute_locked 不自取）。
    unsafe { note_sleepq_changed() };
    n
}

// ---- fastpath/slowpath 分层原语（4-4；`IPC设计.md` §7 检查链的调度侧）----

/// 4-4 fastpath 检查链的 SLEEPQ 侧：找「睡单一对象 `obj` 等与 `signals` 相交
/// 位」的等待者。`Some(t)` = 恰有一个非哨兵匹配者；`None` = 零/多个匹配者，
/// 或队列上任一处存在**位相交**的 [`WAIT_ANY`] 哨兵等待者（哨兵与唤醒位
/// 相交 = wake_matching 也会摘它——单点唤醒的集合 ≠ 全匹配集，一律退
/// slowpath 保两路语义逐条对齐；位不相交的哨兵不在唤醒集内，不挡 fastpath
/// ——b-2 LB 修正：wait_many 收割者常驻挂 TERMINATED 位，若无条件退
/// slowpath，其存活期 = 全系统 send 直切关停，ai_ping 类 poll-0 直切确定
/// 性测试结构必断）。**IRQ 屏蔽窗内调用**（S-2 起锁窗等价：`klock_enter` 先
/// disable_irq 后取锁——KERN_LOCK 窗内 IRQ 必屏蔽，SMP 下另享跨核互斥）；
/// 返回指针仅在**同一窗口内**有效（窗口内无插队——[`fast_wake_locked`] 的
/// 契约）。
///
/// S-3c c-2a 起：**自持窗版**（取放 SQ 锁自包）——消费面 = ktest FAST L0-L4
/// 谓词层直调（boot 屏蔽态零争用，每调用一平衡窗）。生产 fastpath 用
/// [`find_single_waiter_locked`]（义务④跨 find→push→fast_wake 单窗）。
pub fn find_single_waiter(obj: usize, signals: u64) -> Option<NonNull<Task>> {
    let phys = sq_lock();
    let r = find_single_waiter_locked(obj, signals);
    sq_unlock(phys);
    r
}

/// [`find_single_waiter`] 的持窗版：**须已持 SLEEPQ 锁**（`sq_lock`）且整段
/// 使用窗内完成（同窗配对 `sq_unlock` / [`fast_wake_locked`]）——义务④
/// fastpath 的 find 臂。谓词语义与自持窗版一字不差。
pub fn find_single_waiter_locked(obj: usize, signals: u64) -> Option<NonNull<Task>> {
    if obj == 0 || signals == 0 {
        return None;
    }
    let mut found: Option<NonNull<Task>> = None;
    // SAFETY: IRQ 屏蔽窗（调用方契约）；只读遍历、不摘除。
    unsafe {
        let mut cur = sleepq().head.get();
        while let Some(c) = cur {
            let t = c.as_ptr();
            cur = (*t).sleep_next.get();
            // W3-Q2：资格判单源 berry_wqmatch（哨兵位相交挡门 + 多匹配者
            // 挡门 + b-2 LB 修正「位不相交哨兵不挡」——host 钉板面）。
            // 与 berry_wqmatch::single_waiter 的循环逐字同构（那边返回
            // 下标、这边持有指针）。
            let d = berry_wqmatch::WaiterDesc {
                key: berry_wqmatch::WaitKey::from_raw((*t).wait_obj),
                wait_signals: (*t).wait_signals,
            };
            if d.key == berry_wqmatch::WaitKey::Any
                && (d.wait_signals & signals) != 0
            {
                return None; // 位相交哨兵在唤醒集内：唤醒集不可约 → slowpath
            }
            if berry_wqmatch::matches_wake(&d, obj, signals) {
                if found.is_some() {
                    return None; // 多匹配者 → slowpath（wake_matching 全摘）
                }
                found = Some(c);
            }
        }
    }
    found
}

/// 4-4 fastpath 定点唤醒：唤醒同窗 [`find_single_waiter_locked`] 找到的唯一
/// 等待者（内部序列与 `wake_matching` 每匹配者共用 [`wake_one_inner`] + 同款
/// 收尾 PI 全图重推）。供 `sys_channel_send` fastpath 调用——检查链命中即
/// 「一次 SLEEPQ 定点查找替代全队列匹配扫描」。
///
/// # Safety
/// `t` 须为**同一 SQ 窗内** `find_single_waiter_locked` 的返回值（窗内无
/// 插队 ⇒ 此刻仍在 SLEEPQ 上）；**SQ 锁已持**（c-2a 起契约——调用方窗内
/// 收尾 `sq_unlock`，窗内禁切换故直切判定在窗外）。
pub unsafe fn fast_wake_locked(t: NonNull<Task>) {
    // SAFETY: 契约如上。
    unsafe {
        wake_one_inner(t);
        note_sleepq_changed();
    }
}

/// 4-4 直切点（§7「直切投递」，seL4 direct switch 的 v1 形态）：fastpath 唤醒
/// 后调用——若 `pick_next` 出选者 `prio_effective` **严格高于**当前任务，立即
/// 切过去（高优等待者被投递后不必等 tick/yield 才上 CPU——4-0 唤醒「不立即
/// 抢占」在 fastpath 的例外出口）；否则原样返回（同层 FIFO：等/低优先等待者
/// 不因 IPC 插队——4-3a 语义）。返回是否真的发生直切。
///
/// **幻影出选防线（4-6 修）**：先 [`Runqueue::has_eff_above`] 只读探峰——环上
/// 无严格更高者**根本不调 `pick_next`** 再判不切。`pick_next` 不是纯查询：选中
/// 即 `advance_past` 消耗 head 槽位——「出选后不切走」= 幻影出选，游标被白耗
/// 一格。稳态矩阵全员 eff 相同（4-3a 同层）时每条 fastpath send 都幻影消耗一格，
/// 而真实出选（block/yield/tick）只认 head——重复性 IPC 节奏会把真实出选相位
/// 锁死在环上固定节点集，其余节点**结构性饿死**（4-6 实测：echo 服务被饿死
/// 永久冻结——出选:切入 = 29:1，其 client svcmgr 10s 等超时自陷）。
/// 探峰是只读一圈 O(n)，n = 环长；不可能切走时零副作用返回。
///
/// **调用方保证 IRQ 屏蔽窗**（SVC 天然满足；内核任务自行 `disable_irq`）——
/// 协议同 [`block_current`]：可从 syscall 主体中途切走（当前任务留在其内核
/// 栈上的异常帧不动），切回点续行于此、IRQ 仍屏蔽，SVC 语境 eret 自然恢复
/// SPSR。遥测记 `SwitchKind::Preempt`（同 tick 抢占分型；4-4 直切次数另有
/// telemetry 专用计数器区分）。
pub fn try_preempt() -> bool {
    // SAFETY: 契约如上；CURRENT/本核 runqueue 顺序访问。
    unsafe {
        assert_preempt_balanced("try_preempt");
        let cur = current_task();
        // ktest 护栏（复盘雷 3）：`sched::init` 前 CURRENT=null——fastpath 若在
        // 手搭睡眠者在队时被走到（ktest 纪律明令禁止、但仅注释执法），下行的
        // has_eff_above 解引用会直接挂死在 boot 期、难归因。null 早退把违约
        // 显形为「无直切」；正常运行期 CURRENT 恒非 null（SVC/IRQ 上下文），
        // 本分支零成本。
        if cur.is_null() {
            return false;
        }
        // S-3b：探峰集 == 出选集（同 eligible 门）；PickFilter 退役改形。
        // 幻影出选防线：环上无严格更高 eff → 必不直切 → 不消耗 head 槽位。
        // RQ 窗（c-2b）：探峰走查 + 出选 + 状态定形同窗（探峰也是环读——
        // 终态无锁走查可读到摘环中的 stale 链接）；切换舞蹈在窗外。
        let phys = rq_lock();
        let mut go: Option<*mut Task> = None;
        if rq().has_eff_above_filtered((*cur).prio_effective, cur) {
            if let Some(next) = RR.pick_next_filtered(rq(), topo(), cur) {
                let next_ptr: *mut Task = next;
                if next_ptr != cur && (*next_ptr).prio_effective > (*cur).prio_effective {
                    (*cur).state = TaskState::Ready;
                    (*next_ptr).state = TaskState::Running;
                    go = Some(next_ptr);
                }
            }
        }
        rq_unlock(phys);
        if let Some(next_ptr) = go {
            (*next_ptr).slice_left = hal::timer::slice_cycles();
            // 授片即重编（同 yield_now——`调度器设计.md` §4.1 绝对截止时间
            // 纪律）：直切即授满片，以现读 CNTVCT 重算绝对截止。
            // SAFETY: IRQ 已屏蔽（调用方契约）；写 CNTV 系统寄存器；EL1。
            hal::timer::arm_slice();
            set_current(next_ptr);
            // S-6 ① 补数：try_preempt 直切臂（fastpath/send 授片切换）旧不计
            //——IPC ping-pong 最热切点，漏计=金丝雀对服务往返结构性失明。
            bump_switches();
            hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
            activate_if_changed(cur, next_ptr);
            note_switch(cur, next_ptr, SwitchKind::Preempt);
            // S-2 E0 舞蹈（草案 §2.3）：fastpath/send 授片切换——切前放锁
            //（SVC 语境位=1；bench ping/pong 等内核任务直调位=0 跳过）。
            klock_release_switch(cur);
            // SAFETY: IRQ 已屏蔽；prev/next 有效、不别名。
            hal::task::context_switch(&raw mut (*cur).ctx, &raw const (*next_ptr).ctx);
            klock_acquire_switch(cur);
            // <- 被切回时续行于此（IRQ 仍屏蔽——SVC eret 自然恢复）。
            return true;
        }
    }
    false
}

/// SLEEPQ 超时扫描（`on_irq_tick` 开头调；tick 粗粒度——deadline 过后最迟一个
/// slice 被摘，tickless 精确 arm 到最近 deadline 留后）。升序队列头未到期即
/// 早停（空队近零开销）。S-3c c-2a 起：全体包 SQ 锁窗（扫描摘队 + PI 收尾
/// 一体；IRQ 语境调用 ⇒ 同核重入不可能——窗纪律见 `SLEEPQ_LOCK`）。
///
/// # Safety
/// IRQ 屏蔽窗内（on_irq_tick 调用）。
unsafe fn expire_timeouts() {
    let now = hal::timer::cntvct();
    let phys = sq_lock(); // SQ 窗开
    loop {
        let Some(h) = sleepq().head.get() else { break };
        if (*h.as_ptr()).wait_deadline > now {
            break; // 头未到期 → 后面更远，早停
        }
        // 摘头 → Timeout → 入 runqueue（enqueue 置 Ready）。
        let t = h.as_ptr();
        sleepq().unlink(h);
        (*t).wakeup.set(WakeReason::Timeout);
        enqueue_with_kick(&mut *t, (*t).home_cpu as usize);
        note_wake(t, WakeReason::Timeout);
    }
    // 4-3b：超时摘队也改变睡眠者集合（纯 sleep 者超时离队 → 冻结其 boost）。
    unsafe { note_sleepq_changed() };
    sq_unlock(phys); // SQ 窗闭
}

// ---- PI 图推导支撑原语（4-3b；消费者 = `kernel/src/pi.rs`，图语义在那侧）----

/// 遍历全局 SLEEPQ 上所有睡眠者（head → tail）。**调用契约**：IRQ 屏蔽窗内
/// （block/wake/syscall 尾路径天然满足；S-2 起锁窗等价——`klock_enter` 先
/// disable_irq 后取锁，KERN_LOCK 窗内 IRQ 必屏蔽、SMP 下另享跨核互斥）；回调
/// 中不得改动任何任务的睡眠链接字段（`sleep_prev`/`sleep_next`——遍历中
/// 接线会扬队）、不得 block/wake。
/// 只读字段或写 `pi_inherited`/`prio_*` 等非链接字段安全（PI 重推的用法）。
pub fn for_each_sleeper(mut f: impl FnMut(&mut Task)) {
    // SAFETY: 契约如上；SLEEPQ 单核顺序访问；摘除中的节点由调度器拥有。
    unsafe {
        let mut cur = sleepq().head.get();
        while let Some(n) = cur {
            let t = &mut *n.as_ptr();
            cur = t.sleep_next.get(); // 先存 next 再交给回调（回调可能改其它字段）
            f(t);
        }
    }
}

/// 重算 `t` 的有效优先级（4-3b 统一合成公式）：
/// `prio_effective = max(prio_base + 策略权重, pi_inherited)`——与 pick 侧
/// [`crate::policy::apply`] 同公式（两写入者不互踩）。冻结语义见
/// [`Task::pi_inherited`]：只对 SLEEPQ 在队者调用，runnable 者保持冻结值。
pub fn refresh_effective(t: &mut Task) {
    let w = crate::policy::weight_of(t);
    t.prio_effective = (t.prio_base + w).max(t.pi_inherited);
}

/// 入队前置判据（ktest RING3 直测用；生产执法 = `SleepQueue::insert_sorted`
/// 入口 assert 同一判据）：`sleep_prev`/`sleep_next` 均 `None` **且** 自己
/// 不是队头——单元素队成员两字段均 None（`SleepQueue` 类型注释），字段判据
/// 单用对它失效。
pub fn sleepq_insertable(t: &Task) -> bool {
    // SAFETY: 只读全局 SLEEPQ head（单核顺序；ktest 在 init 前读 const 初值）。
    let is_head = unsafe { sleepq().head.get() == Some(core::ptr::NonNull::from(t)) };
    !(t.sleep_prev.get().is_some() || t.sleep_next.get().is_some() || is_head)
}

/// 把 `t` 按 deadline 升序压入全局 SLEEPQ。**ktest 手搭睡眠者专用**（生产
/// 路径一律走 [`block_current`]——本函数不切走、不置 Blocked）。契约同
/// `SleepQueue::insert_sorted`：IRQ 屏蔽窗内、`t` 不在队（链接字段均
/// `None`）且地址稳定（ktest `Box::leak`）。
pub fn sleepq_insert(t: &mut Task) {
    // SAFETY: 契约如上。
    unsafe { sleepq().insert_sorted(NonNull::from(t)) };
}

/// 把 `t` 摘出全局 SLEEPQ（ktest 清理专用）。**不幂等**——对不在队者调用会
/// 扬队（`SleepQueue::unlink` 注释）；调用方须确知 `t` 在队。
pub fn sleepq_unlink(t: &mut Task) {
    // SAFETY: 契约如上。
    unsafe { sleepq().unlink(NonNull::from(t)) };
}

/// 用户任务致死计数只读访问（G1.5 watchdog 断言用；A12 原子化）。
pub fn user_kills() -> u64 {
    USER_KILLS.load(Ordering::Relaxed)
}

/// 最近被杀用户任务名只读访问（watchdog 断言「杀对了人」用；空串 = 尚无杀）。
/// A12：ptr/len 对组装（len 的 Acquire 保证见到 ptr 的新值——见 LAST_KILLED_*）。
pub fn last_killed() -> &'static [u8] {
    let len = LAST_KILLED_LEN.load(Ordering::Acquire);
    if len == 0 {
        return b"";
    }
    let p = LAST_KILLED_PTR.load(Ordering::Relaxed) as *const u8;
    // SAFETY: 指针源自 Task.name（&'static 字面量），写后永不失效；len 与该
    // 指针同源成对写入（Release/Acquire 定序）。
    unsafe { core::slice::from_raw_parts(p, len) }
}

/// 环/SLEEPQ 完整性周期走查（复盘雷 2 残留诊断）：环分裂成「头可达活圈 +
/// 孤儿段」对 pick 完全不可见——smoke 标记全部分裂前打完，30s 长跑五任务
/// 静默失联由此漏网。本检查对账两笔：①runqueue 走查步数 == len 记账
///（步数 < len = 孤儿段；到帽 = 断链）；②SLEEPQ 链走计数 == len 记账。
/// 消费点 = `on_irq_tick` 金丝雀分支（每 100 切换一次，O(n) 极小）与 ktest。
/// **零分配、成功路径零输出**；违约即 panic 带时刻响亮失败（失败路径才
/// 格式化）。调用契约：IRQ 上下文（环/队变更屏蔽窗的串行等价）。
fn integrity_check() {
    // S-3c slice 3-1a 分窗走查（拆解勘误同刀）：原 KL 串行裸走查是侦察
    // 「全域最后一位 KL-only 读者」论漏数的**第二位**——0x37 降档免 KL 后
    // 其 block/wake 的 SQ/RQ 写与本对账走查失去共同串行化点（§A113 读者
    // 面：对账撕读 = assert 假火 = 内核 fatal，读者面随降档同刀收口）。
    // **裸物理窗而非位感知退化对**：调用点在 set_current(next) 之后、
    // context_switch 之前——current=next 义务位置位而物理锁未重取（重取
    // 在 next 自身续行点），位在此说谎；被抢占者持锁 ⟂ 可抢占（屏蔽窗
    // 矛盾律）⇒ 本核无自持，裸取无自死锁。三窗独立开闭无嵌套（3-1b ③
    // 终态形提前落定——KL 剥离后本面零改）。
    // SAFETY: IRQ 上下文（调用方保证——KL 窗内 + 屏蔽窗）；只读走查 +
    // Cell 记账读；窗内禁切换（对账两读同窗一致）。
    unsafe {
        // S-3b：双队列各自走查对账（per-CPU 分诊后账面 = 每核独立环 + 全局
        // SLEEPQ 三段；孤儿段形态对 pick 不可见，只有记账对账显形）。恒走查
        // 两槽——未参与核槽恒空环（walk 0 == len 0 恒过，零成本）。
        for cpu in 0..2 {
            rq_acquire(cpu);
            let steps = rq_of(cpu).ring_check();
            let want = rq_of(cpu).len();
            // S5-c 计数门对账（计数 vs 真扫）：本环真扫 DL 数不得高于全局
            // 账本——**单边**断言（多计合法：ktest 本地环 / on-ring 降带只
            // 增不减；漏计是危险向 = 门会跳过需要的解冻扫描，须响亮失败）。
            // 真扫与账本读**同锁窗**：本环成员的 link/unlink+记账在本队列
            // RQ 锁下整体落定（锁全序），窗外读会被随后摘环的 fetch_sub
            // 打穿（走查见 1 → 摘线减账 → 读到 0 = 假火）；跨核在途迁移
            // （对核已 unlink、本核未 link）不在本环走查集也不在账本——
            // 单边断言对在途态免疫。
            let dl_walk = rq_of(cpu).dl_on_ring_walk();
            let dl_ledger = crate::runqueue::dl_on_ring();
            rq_release(cpu);
            assert!(
                steps == want,
                "integrity: cpu{} ring walk {} != len {} (orphan segment or broken links)",
                cpu,
                steps,
                want
            );
            assert!(
                dl_walk as u32 <= dl_ledger,
                "integrity: cpu{} ring has {} deadline tasks but DL_ON_RING ledger {} \
                 (under-count: unthrottle gate would skip needed scans)",
                cpu,
                dl_walk,
                dl_ledger
            );
        }
        sq_acquire();
        let mut n = 0;
        let mut cur = sleepq().head.get();
        while let Some(c) = cur {
            n += 1;
            assert!(
                n <= crate::runqueue::RING_WALK_CAP,
                "integrity: sleepq walk exceeds cap (broken links?)"
            );
            cur = (*c.as_ptr()).sleep_next.get();
        }
        let want = sleepq().len.get();
        sq_release();
        assert!(n == want, "integrity: sleepq walk {} != len {}", n, want);
    }
}

/// idle 主循环（tickless）：runqueue 空且**无睡眠者**时屏蔽 CNTV + `wfi`
/// （空闲核不收周期中断）；有 runnable 任务时武装 slice + 让出。
///
/// **4-0：睡眠者在队时不能 mask 永睡**——超时扫描靠 on_irq_tick 驱动，SLEEPQ
/// 非空而 runqueue 空（全阻塞）时改为 arm slice + 空转等 tick：tick 扫出超时者
/// 入 runqueue → 下轮即正常分时（粗粒度扫描；tickless 精确 arm 到最近 deadline
/// 留后）。
///
/// **先 `disable_irq` 再判/`mask`**：保证 mask 与解蔽之间无窗口漏进早 IRQ（风险 R6）。
///
/// **S-2b 参与态改形（§2.3 主核 idle）→ S-3b b-2 A7 合流**：回收/出选/偷取
/// 入 KERN_LOCK 窗（REAPQ push 可来自 CPU1 锁窗退出链——drain 须同窗串行）；
/// 派发 = park 式直接切换（[`dispatch_from_idle`]——不再「恒 arm_slice + wfi
/// 等 tick 锁内 pick」间接链），皆空深/浅 park（masked-wfi）。IDLE_TASK 降格
/// 为 CPU0 idle ctx（current 恒 IDLE_TASK 不置 null——block/exit 的 pick-None
/// 臂与 on_irq_tick/on_sgi_resched 的 cur 判据都依赖非 null）。单核/未参与
/// 形态零漂移（原样）。
pub fn idle_loop() -> ! {
    loop {
        hal::disable_irq();
        if hal::smp::par_enabled() {
            // S-3b b-2（子案 §9.6 件② A7 合流 + 雷① 收口）：CPU0 idle 与副核
            // park 环同形——锁窗内 reap_drain（义务⑥ 维持）→ 本核出选（F5
            // 空判真消费：pick 本身即判，`_empty` 死变量就此消解）→ 偷取探测
            // → 皆空深/浅 park。派发 = park 式直接切换（雷① wedge 收口：
            // S-2b 形「恒 arm_slice + wfi 等 tick 锁内 pick」的间接链拆断——
            // idle 不再等 tick，RR.tick(IDLE) 片衰减延迟边同步消解；环顶重查
            // 承接一切 wfi 醒（在-handler 派发先切走时切回点同径）。REAPQ 深
            // park 滞留面：跨核退出必经 wake_matching 路径的事件入队 SGI 醒
            // 本核（exit 钩子唤醒链），醒即 drain——静默退出者由下次任意醒
            // 承接，cap 32 有界（义务⑥ 账面不变）。
            reap_drain();
            // SAFETY: IRQ 屏蔽窗内（3-1b 起无 KL 包络——drain 自持 REAPQ 窗）；
            // my_cur=null 出选
            // （idle 非 Running 驻环者——环上无 Running 残留，同 park_recheck
            // 判据）；偷取同窗（对核环手术）。
            // RQ 窗（c-2b）：本核出选 + 置 Running 同窗（防偷屏障——helper
            // 首行同值重写幂等，为偷取臂保留）；偷取在窗外（c-2b slice 5
            // 真锁——远端窗+本核窗两段自持，均不嵌本核出选窗）。
            let picked: Option<*mut Task> = {
                let p: Option<*mut Task> = unsafe {
                    let phys = rq_lock();
                    let picked = RR
                        .pick_next_filtered(rq(), topo(), core::ptr::null_mut())
                        .map(|n| n as *mut Task);
                    if let Some(np) = picked {
                        (*np).state = TaskState::Running;
                    }
                    rq_unlock(phys);
                    picked
                };
                p.or_else(|| unsafe { try_steal_one() })
            };
            match picked {
                Some(next_ptr) => {
                    // SAFETY: PSTATE.I=1（loop 顶 disable）；结构锁窗在
                    // dispatch 体内按位收口（3-1b 起无 KL）。切回点 = helper 尾
                    // （任务侧 block/exit 的 pick-None
                    // 臂切回 IDLE_TASK.ctx = helper prev 同槽）→ 分支尾 → 环顶
                    // 重检（PSTATE.I=1 续承——block/exit 切换不变式）。
                    unsafe { dispatch_from_idle(next_ptr) };
                }
                None => {
                    // 深浅 park（副核 secondary_main 环同形）：SLEEPQ 全局升序
                    // 队头即最近到期（两核对同一 deadline 双 arm = 至多一次
                    // 冗余醒，锁内 expire 串行无撕）；尽空深 park（mask）醒源
                    // 只剩 SGI/外设 IRQ。
                    match nearest_deadline() {
                        // SAFETY: 本核 CVAL 编程到绝对 deadline（过期即醒，环
                        // 顶 expire 承接——tick 进 expire 后 pick 到期者即时
                        // 派发，RR.tick(IDLE) 片衰减边不再挡路）。
                        Some(d) => unsafe { hal::timer::arm_at(d) },
                        // SAFETY: CNTV IMASK=1（深 park）；REAPQ 滞留面见上注。
                        None => unsafe { hal::timer::mask() },
                    }
                    // masked-wfi 三步闭环（丢唤醒窗收口，secondary_main 同款）：
                    // wfi 全程 PSTATE.I=1——醒于 GIC pending 线而不服务；醒后
                    // enable 即取该中断（handler 内 SGI 快速重调度/入队皆可），
                    // 再 disable 回环顶重检。
                    hal::smp::note_park_cpu(); // S-5a per-CPU park 统计（本核独占写）
                    hal::wfi();
                    hal::enable_irq();
                    hal::disable_irq();
                }
            }
        } else {
            // 单核/未参与形态：零漂移（原样）。
            // Plan B：idle 是天然回收点（退出者已不可能在任何执行上下文）——
            // drop Task Box + 内核栈，纯释放零分配。
            reap_drain();
            // SAFETY: IRQ 已屏蔽；本核 runqueue/SLEEPQ 顺序访问。
            let empty = unsafe { rq().is_empty() && sleepq().len.get() == 0 };
            if empty {
                // tickless：空闲不 arm、屏蔽 CNTV，wfi 睡。
                // SAFETY: 写 CNTV_CTL；EL1。
                unsafe { hal::timer::mask() };
                hal::smp::note_park_cpu(); // S-5a per-CPU park 统计（本核独占写）
                hal::enable_irq(); // 允许 IRQ 唤醒 wfi（Step 4 无源 → 永睡 = 干净 halt）
                hal::wfi();
                // 被唤醒（Step 5+）→ 回 loop 顶重检 runqueue。
            } else {
                // 有活（runnable 或睡眠者）：武装一个时间片，让给 runnable 任务或
                // 空 tick 扫超时（Step 6 起由 IRQ 驱动抢占）。
                // SAFETY: 写 CNTV 系统寄存器；EL1。
                unsafe { hal::timer::arm_slice() };
                hal::enable_irq();
                yield_now();
            }
        }
    }
}

/// IRQ 派发回调（HAL `exception_irq` 经函数指针槽调，EOIR 之后）。
///
/// **Step 6（里程碑）**：真抢占。`RR.tick` 减当前 `slice_left`；归 0 则 `pick_next`，
/// 若 `Some(next) != current` 则 `switch`。timer 每 slice（10Hz）落一次 → 两忙循环任务
/// 在 slice 下交替（**无任何 yield 调用**）= 里程碑「两个内核任务在定时器上分时」。
///
/// **切换在中断里正确**（plan 核心正确性 trace）：`switch` 是纯叶子 asm——存的 SP 正是
/// `load_frame` 所需 SP（cur 栈上 TrapFrame 下方，扣除冻结的 `exception_irq`/本函数帧）。
/// 切回 cur 时续行于 `context_switch` 之后、经 `exception_irq` 回退到 `load_frame` →
/// `eret` 回 cur 被中断的 PC。IRQ 路径 `PSTATE.I=1`（硬件入口屏蔽），无需再 disable
/// （Bug 2 不变式：switch 永不在 `PSTATE.I=0` 到达）。rearm 已由 `timer::handle`（EOIR
/// 前）完成。
///
/// # Safety
/// IRQ 上下文（`exception_irq` 调，`PSTATE.I=1`）；C-ABI fn 指针槽。

/// 当前任务的 thread_key（S3-b 线程级 freeze 自冻判别用）。
pub fn current_thread_key() -> usize {
    let cur = unsafe { current_task() };
    if cur.is_null() { 0 } else { unsafe { (*cur).thread_key } }
}

/// 1.1/v1.2：当前任务的运行时 deadline 状态（sched_getattr 自查面消费）。
/// 返回 `(vdeadline, slice_left)`——(0,0) = 无当前任务或非 deadline。
/// vdeadline = 绝对截止期（CNTVCT cycles）；slice_left = 剩余预算/片。
pub fn current_deadline_state() -> (u64, u64) {
    let cur = unsafe { current_task() };
    if cur.is_null() {
        return (0, 0);
    }
    unsafe {
        if (*cur).prio_effective < crate::class::PRIO_DEADLINE {
            return (0, 0); // 非 deadline 任务无运行时截止态
        }
        ((*cur).vdeadline, (*cur).slice_left)
    }
}

/// 1.1/S3-b：按 thread_key 冻结前置谓词（单线程版 freeze_precheck）。
pub fn freeze_precheck_by_thread(thread_key: usize) -> bool {
    // 扫双 RQ + SLEEPQ 找匹配 thread_key 的任务，检查 PI/派生位/持锁。
    // SAFETY: SVC 路径 KL 已持；只读扫描。
    unsafe {
        // app-1 段 a-1 同款修正（见 freeze_precheck_by_pinned 注）：svc_mask
        // 是 SVC 出口清零的窗内记账——睡在 syscall 内的任务带非零 mask 挂
        // SLEEPQ，睡眠者不查（已放全部结构锁）；RQ 在席（真在飞/窗内可抢
        // 占）才拒。
        let sleep_ok = |t: &Task| -> bool { t.pi_inherited == 0 && t.held_locks == 0 };
        let rq_ok = |t: &Task| -> bool { sleep_ok(t) && t.svc_mask == 0 };
        for cpu in 0..2 {
            let head = rq_of(cpu).head();
            let mut cur = head;
            while let Some(c) = cur {
                let t = c.as_ptr();
                // §A136：先访当前节点，再推进并判回 head。
                if (*t).thread_key == thread_key && !rq_ok(&*t) { return false; }
                cur = (*t).rq_next.get();
                if cur == head { break; } // 环终止

            }
        }
        let mut cur = sleepq().head.get();
        while let Some(c) = cur {
            let t = c.as_ptr();
            cur = (*t).sleep_next.get();
            if (*t).thread_key == thread_key && !sleep_ok(&*t) { return false; }
        }
    }
    true
}

/// 1.1/S3-b：按 thread_key 冻结单任务（返回冻结数 0/1）。
pub fn freeze_by_thread(thread_key: usize) -> usize {
    // SAFETY: SVC 路径 KL 已持。
    unsafe {
        for cpu in 0..2 {
            let head = rq_of(cpu).head();
            let mut cur = head;
            while let Some(c) = cur {
                let t = c.as_ptr();
                // §A136：先访当前节点，再推进并判回 head。
                if (*t).thread_key == thread_key {
                    (*t).held_locks |= crate::task::HELD_FROZEN;
                    return 1;
                }
                cur = (*t).rq_next.get();
                if cur == head { break; } // 环终止

            }
        }
        let mut cur = sleepq().head.get();
        while let Some(c) = cur {
            let t = c.as_ptr();
            cur = (*t).sleep_next.get();
            if (*t).thread_key == thread_key {
                (*t).held_locks |= crate::task::HELD_FROZEN;
                return 1;
            }
        }
    }
    0
}

/// 1.1/S3-b：按 thread_key 解冻。
pub fn thaw_by_thread(thread_key: usize) {
    // SAFETY: handle close 路径。
    unsafe {
        for cpu in 0..2 {
            let head = rq_of(cpu).head();
            let mut cur = head;
            while let Some(c) = cur {
                let t = c.as_ptr();
                // §A136：先访当前节点，再推进并判回 head。
                if (*t).thread_key == thread_key {
                    (*t).held_locks &= !crate::task::HELD_FROZEN;
                }
                cur = (*t).rq_next.get();
                if cur == head { break; } // 环终止

            }
        }
        let mut cur = sleepq().head.get();
        while let Some(c) = cur {
            let t = c.as_ptr();
            cur = (*t).sleep_next.get();
            if (*t).thread_key == thread_key {
                (*t).held_locks &= !crate::task::HELD_FROZEN;
            }
        }
    }
}

pub unsafe extern "C" fn on_irq_tick(_tf: &mut hal::trap::TrapFrame) {
    // S-2b 排障期诊断面：tick 入口计数（pre 涨/ticks 冻 = 卡锁；
    // 同冻 = CNTV/IRQ 链死）——含下方 gate 早退轮，两核都计。
    hal::smp::note_tick_entry();
    // S-2 E0（草案 3.1-3）→ 3-1b KL 剥除：expire（自持 SQ 窗）/RR.tick
    // （免锁——账本写侧 RMW 化，见下方调用点注）/pick（RQ 窗）/打印
    //（PRINT_LOCK 自持）各自独立结构窗串行——KL 包络退役，窗口纪律不变
    //（副核调度原子性前提 = 各结构锁窗，非全局 KL）。槽空直通（安装前窗/
    // 单核装饰窗）。
    // S-2b 未参与副核早退（gate 检查与参与态两态合一——secondary_main 探测
    // 环被 tick 打醒但 gate 未开：只记账零共享触达，防夺走 CPU0 的锁窗）。
    if !hal::smp::par_enabled() && hal::smp::cpu_id() != 0 {
        return;
    }
    // SAFETY: IRQ 上下文；计数 Relaxed——无序角色，仅计数。
    IRQ_TICKS.fetch_add(1, Ordering::Relaxed);
    // S-2b 诊断面：per-CPU tick 计数（两核都计——PAR absent 时区分「副核
    // tick 死」vs「tick 活但派发不出」）。
    hal::smp::note_tick();
    // 4-0：先扫 SLEEPQ 超时（到期者置 Timeout 入 runqueue——本 tick 后续的
    // pick 即可能选中它们；头未到期早停，空队近零开销）。
    expire_timeouts();
    let cur = current_task();
    // S-2b PAR 金丝雀采样（草案 §5.2 D1）：EL0 tick 纪事钉板 / 非 EL0 清钉。
    // is_el0 判据 = ttbr0 != 0（有地址空间的任务 = EL0 用户进程；内核任务/
    // IDLE 恒 0；park 态 cur=null 走清钉——witness 不误置）。
    hal::smp::par_sample(
        cur as usize,
        !cur.is_null() && (*cur).ttbr0 != 0,
    );
    // S-3b b-2 LB 金丝雀采样（义务③ per-child 归因）：本核 tick 时 cur 是
    // LB 计算线程（kernel 注入判别 thread_key ∈ LB_PROCS）→ 本核计数 +1。
    // 直接路由到本核与被偷到本核同格记账（裁决二）——LB 均衡度 witness 的
    // 唯一记账点。
    // SAFETY: 读点仅 boot 期写（lb gate 块）；纯计数原子。
    if let Some(f) = unsafe { LB_MATCH } {
        if !cur.is_null() && f((*cur).thread_key) {
            LB_TICKS[hal::smp::cpu_id() as usize].fetch_add(1, Ordering::Relaxed);
        }
    }
    if cur.is_null() {
        // S-3a（子案 §9.5 步 A）：park 派发整体迁出至 [`park_recheck`]——SGI
        // 醒与 CNTV 醒经 eret 回 secondary_main park 环后收敛单路径承接；本槽
        // 只留 expire/par_sample（上方已毕），cur==null 无记账面即回。
        // mod4 限频门随 tickless park 退役（义务⑥——副核参与节奏改由 CPU0
        // 入队 SGI 事件定步：console 消费一轮才唤醒生产一轮 = 往返背压，
        // 替代占空比限频；复发盯面见 smp.rs park_due 注）。
        return;
    }
    // Σvruntime 账本纪律（SMP 相真 bug 修，沿革一八五移交）：`Fair::tick` 是
    // 三账点中唯一免锁到达的——S2b+ 把账本入账挂进 tick 后，账本即成**多
    // 写者**（本核 tick 无锁窗 + 对核锁窗内 link/unlink），写侧原语随刀改
    // fetch_add/fetch_sub RMW（`Runqueue::sum_vruntime` 字段注——同
    // DL_ON_RING 先例）。此处**不得取 RQ 锁**收窗：SVC 窗（RQ 类位）与
    // SGI 回执忙等等路径持 RQ 锁跨核等待，tick（IRQ 上下文）取锁 = 与
    // 对核「持锁等本核」成环——实测双核对 spin（RQ0/RQ1 锁地址对
    // 0x...a68/0x...ab0，7.7K 切换即 wedge）。
    RR.tick(&mut *cur, rq()); // 策略部分：减 slice_left + vruntime/账本（RMW）入账
    if (*cur).slice_left == 0 {
        // G3-1c-1（console 写域解蔽）：禁切窗内 slice 燃尽——本次 tick 不切。
        // 诉求分档承接（errata m2 勘正）：出窗检查点只兑现**严格更高 eff**
        // 的直切诉求（try_preempt 判定）；slice 燃尽 = 同 eff 轮转诉求，
        // 检查点不切、落本支下个 tick 重探（rearm-at-entry 名义整片
        // 100ms、TCG ≈200ms）。窗 = with_print_lock preempt 包络（µs 级
        // 指窗本身）。早退点在结构窗开启前（expire/采样/LB 记账全毕、零锁
        // 持有）——pick 白跑免掉。slice_left 已饱和 0，重探成本 = 每 tick
        // 一次 count 原子读。tick go 支金丝雀行在 go 分支内 = 被门挡在
        // 持 PRINT_LOCK 窗外（G43 改版的死锁防线另一半）。
        if preempt_count() > 0 {
            return;
        }
        // S-6 ③ per-CPU 心跳（诊断面，定谳后退役）：双核 ticks_pre 快照对照
        // 行——挂死尸检一跑定谳「单侧冻结 = 核 wedge（tick/抢占链死）」vs
        // 「双爬 = 内核活、服务协议死锁」。安全位 = preempt_count 门后
        // （PRINT_LOCK 持窗任务被 tick 打断不进本区，G43 同防线）；par 门
        // 内 = 单核零漂移；tot 每 500 跳一行（phase2 ~840s 量级 ≈ 数十行）。
        if hal::smp::par_enabled() {
            let (hb0, hb1, _) = hal::smp::diag_readout();
            let tot = hb0 + hb1;
            if tot / 500 != HB_LAST.swap(tot, Ordering::Relaxed) / 500 {
                hal::kern_lock::with_print_lock(|| {
                    hal::uart_puts(b"hb c0=");
                    hal::uart_put_dec(hb0);
                    hal::uart_puts(b" c1=");
                    hal::uart_put_dec(hb1);
                    hal::uart_puts(b"\n");
                });
            }
            // S-6 ③ 卡死等待者扫描（诊断面，与心跳同退役）：每 2500 tot-tick
            //（run10 定谳：wedge 后全靠对核独爬，5000 门槛只扫到一轮即被
            // smoke 超时杀——降到 2500 换死前两轮）持 SQ 窗走 SLEEPQ，>10s
            // 高龄等待者点名（名/obj/年龄）。与心跳行配对一跑定谳「谁在对谁
            // 断供」：尸末年龄持续爬升的 stuck 行 = 永久丢醒（跨核唤醒丢失）
            // 候选；出现一两次即消失 = 慢链路非死锁。窗纪律同 expire_timeouts
            // （SQ 自旋锁跨核互斥）；采集在窗内、打印在窗外——不新增锁序边。
            if tot / 2500 != STUCK_LAST.swap(tot, Ordering::Relaxed) / 2500 {
                let freq = hal::timer::cntfrq();
                let now = hal::timer::cntvct();
                const STUCK_MAX: usize = 8;
                let mut hits = [(0usize, 0usize, 0usize, 0u64); STUCK_MAX];
                let mut n_hits = 0usize;
                let phys = sq_lock();
                for_each_sleeper(|t| {
                    let age = now.saturating_sub(t.wait_since);
                    if age > freq * 10 && n_hits < STUCK_MAX {
                        hits[n_hits] =
                            (t.name.as_ptr() as usize, t.name.len(), t.wait_obj, age / freq);
                        n_hits += 1;
                    }
                });
                sq_unlock(phys);
                for i in 0..n_hits {
                    // SAFETY: 指针取自 Task.name（&'static [u8] 字面量——
                    // 'static 生命周期，任务离队/退出均不悬垂）。
                    let name: &[u8] = unsafe {
                        core::slice::from_raw_parts(hits[i].0 as *const u8, hits[i].1)
                    };
                    hal::kern_lock::with_print_lock(|| {
                        hal::uart_puts(b"stuck '");
                        hal::uart_puts(name);
                        hal::uart_puts(b"' obj=0x");
                        hal::uart_put_hex64(hits[i].2 as u64);
                        hal::uart_puts(b" age=");
                        hal::uart_put_dec(hits[i].3);
                        hal::uart_puts(b"s\n");
                    });
                }
            }
        }
        // S-6 ②（沿革九十四发现②收口）：定时拉取——设计 §3.4「空闲核
        // **定时**或被唤醒时从忙核拉任务」的定时半句最小兑现。旧触发面仅
        // 「本核 pick None」（idle/park 空转点）：对核忙但独占（副核单任务
        // 连续运行）→ 本核永不为空 → 一次偷取后拓扑锁死（run4 实证：22+
        // 任务挤一核各得公平内卷份额，独占者 672:1）。判据 = 环深失衡
        // K=2（place() ⑤ 同 K，AtomicUsize 免锁读——失衡不成立零锁成本）；
        // 拉取复用 try_steal_one 全套（只持远端锁形 / F-E2 双过滤 / 随迁
        // 改写 home / 回插本环置 Ready）——**不直派**：入环即归，本 tick
        // 随后出选自然轮转（直派是 idle 空转点的特权——本点 cur 在跑）。
        // 单向拉 + 迁移粘性 = 无乒乓；触发在 slice 燃尽支 = 每 slice 至多
        // 一拉天然限速；单核 gate 零漂移。
        if hal::smp::par_enabled() {
            let me = hal::smp::cpu_id();
            // SAFETY: rq_of 触 static mut 访问（IRQ 屏蔽窗）；len 为
            // AtomicUsize（c-2b）Relaxed 免锁读，不触环结构。
            let (mine, theirs) = unsafe { (rq_of(me).len(), rq_of(1 - me).len()) };
            // 定时拉取只做“近空闲补偿”：本核环只有当前 monopolist（len<=1）
            // 且对核显著更重时才拉一任务。若两核各自已有可轮转负载仍持续
            // 按 K=2 拉取，会把服务/子进程反复迁移，SMP TCG 下形成 20 万+
            // 切换与 KL 争用（run6 实录），rootfs 后段 FAT/DIR/DF 族超时。
            if mine <= 1 && theirs >= mine + 2 {
                // SAFETY: 调用契约面扩张（第三调用点）：IRQ 屏蔽 ✓、本核
                // RQ 窗未开（rq_lock 在后）✓、preempt_count==0（PRINT_LOCK
                // 持窗已让）✓——与 idle/park 两点的差异仅「本核出选非
                // None」：失衡判据已保证只在对核显著更重时拉。
                if let Some(p) = unsafe { try_steal_one() } {
                    let st = PULL_CNT.fetch_add(1, Ordering::Relaxed) + 1;
                    if st <= 8 {
                        hal::kern_lock::with_print_lock(|| {
                            hal::uart_puts(b"sS6: pull #");
                            hal::uart_put_dec(st);
                            hal::uart_puts(b" '");
                            hal::uart_puts((*p).name);
                            hal::uart_puts(b"'\n");
                        });
                    }
                }
            }
        }
        // S-3a 义务⑥：mod4 切回 park 门退役——副核 slice 尽与 CPU0 同径出选；
        // 副核 park 化只经任务自身 block/exit/yield 的切回 sec_park_ctx 点，
        // 不再由 tick 强制（参与节奏由入队 SGI 事件定步，与上支注同源）。
        // S-3b（F8）：peek/pick 双视图退役——per-CPU 队列游标私有，副核
        // 回卷饥饿前提（共享游标）不复在，两核同径消费版（FIFO 推进器）。
        // RQ 窗（c-2b）：出选 + 状态定形同窗——Running 置位 = 防对核偷取
        // 屏障（放锁前落定）；RR.tick 免锁在前（账本 RMW 化——见其调用点
        // 注）不入本窗，canary 打印/切换舞蹈在窗外（窗内禁切换）。
        let phys = rq_lock();
        let picked = RR.pick_next_filtered(rq(), topo(), cur);
        let mut go: Option<*mut Task> = None;
        if let Some(next) = picked {
            let next_ptr: *mut Task = next;
            if next_ptr != cur {
                // G3-1a 断言补位（f4 M1 修复批）：tick 抢占支 = 第七切换入口
                //（函数入口不断言——expire/采样/LB 记账无切换语义的面不执法）。
                // 定形**前**断言：定形后才断言 = 违约时留下「已定形未切换」
                // 破态（cur=Ready/next=Running 但无切换发生）。
                assert_preempt_balanced("on_irq_tick（tick 抢占支）");
                (*cur).state = TaskState::Ready;
                (*next_ptr).state = TaskState::Running;
                go = Some(next_ptr);
            }
        }
        rq_unlock(phys);
        if let Some(next_ptr) = go {
            (*next_ptr).slice_left = hal::timer::slice_cycles();
            // 授片重编已由 timer::handle 的 rearm-before-EOIR 在 IRQ 入口覆盖
            // （同一中断上下文、微秒级差）——CVAL 已是满片终点，此处不再 arm。
            set_current(next_ptr);
            // S-6 ①：递增+打印迁共享 helper（六位全域口径——沿革九十四发现①
            // 收口；SMP 稳态忙对各占核时本支结构性冷，打印只挂此位 = 金丝雀
            // 0 条假败）。
            let crossed = bump_switches();
            hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
            activate_if_changed(cur, next_ptr);
            note_switch(cur, next_ptr, SwitchKind::Preempt);
            // 完整性走查（复盘雷 2 残留诊断）：金丝雀拍点顺带对账环/SLEEPQ。
            // 只在 tick 抢占支跑（rq_unlock 先行 + preempt_count 门挡 PRINT_LOCK 窗）。
            if crossed {
                integrity_check();
            }
            // 切出 cur：存 callee-saved+sp，载 next；ret 进 next（首切→trampoline，
            // 否则→next 上次切出处）。切回 cur 时续行于下一行。
            // S-2 E0 舞蹈（草案 3.1-3，五切走点之一）→ 3-1b KL 剥除：切走前
            // 无锁可放（3-1a 起结构窗已闭、KL 已退役）；**切走后不重取**——
            // 被打断者不持锁（可被抢占 ⟹ 不在屏蔽窗 ⟹ 非 SVC 主体），续行方
            // next 在它自己的挂起点按自身位自理。切走前仅剩
            // context_switch 纯 asm（零共享触达）；early return 防 cur
            // 恢复路径（本函数尾 + IRQ 收尾 eret）双重放锁。
            // SAFETY: PSTATE.I=1（IRQ 屏蔽）；prev/next 有效、不别名。
            hal::task::context_switch(&raw mut (*cur).ctx, &raw const (*next_ptr).ctx);
            return;
        }
    }
    // 未切走路：直回（3-1b 起无 KL 配平义务；结构窗均已闭）。
}

/// 入队 SGI（`SGI_RESCHED_US`）接收侧半件（义务⑧ S-3b b-1）：CPU0 快速重调度。
///
/// b-1 排障实证的根因：跨核唤醒落 CPU0 队列后只等 tick（无 preempt-on-wake，
/// TCG 有效 tick ≈ SLICE_HZ/2 → 最长整片），而 CPU1 唤醒派发即时（park 环 +
/// SGI + 空 rq[1]）——**唤醒延迟不对称**让快核上的通道灌沟者每次抢走对端腾出
/// 的槽位（console 8 槽全满形态，svcmgr 等 `cur=0 want=2` 转到 10s 帽 -110 被
/// 杀）。本槽把 tickful 核的片尽轮转语义搬到事件面：SGI 到 = rq[0] 刚入新
/// 任务，立即旋转出选（**同 eff 也切**——探峰只防幻影出选，不限制同层轮转）。
/// 窗内例外（errata m2 注）：G3-1c-1 门在场（preempt_count>0）= 禁切窗，
/// 本槽闭门早退——「同 eff 也切」事件面语义窗内暂停，同 eff 诉求落下个
/// tick、严格更高者出窗检查点直切（分档详 print_window_enter 注）。
///
/// 与 [`on_irq_tick`] 分立（为何不共用 tick 槽）：无 `timer::handle`（无
/// rearm——授片显式 `arm_slice`，同 [`try_preempt`]/yield/park_recheck 形）、
/// 无 `RR.tick` 片记账（SGI 不减片）、无 `expire_timeouts`/`par_sample`/
/// `IRQ_TICKS`（tick 语义面与遥测不稀释）。「SGI 不进 tick 槽」纪律（S-3a
/// §9.5）不破——副核仍经 eret 回 park 环 `park_recheck` 单路径承接；本槽
/// 仅 tickful 核（CPU0）进，副核被 gate 挡在锁窗之外零共享触达。
///
/// # Safety
/// IRQ 上下文（`exception_irq` 经 SGI 槽调，`PSTATE.I=1`）；C-ABI fn 指针槽。
pub unsafe extern "C" fn on_sgi_resched(_tf: &mut hal::trap::TrapFrame) {
    // 副核/park 形态 gate（探判在锁窗外）：park 承接架构不改——副核 SGI 醒
    // 仍经 park 环派发；ktest SGI probe（pre-gate）同被挡下，只留 trap 侧计数。
    if !hal::smp::par_enabled() || hal::smp::cpu_id() != 0 {
        return;
    }
    let cur = current_task();
    // 装饰窗护栏（同 on_irq_tick null 支；正常期 CPU0 current 恒非 null）。
    if cur.is_null() {
        return;
    }
    // G3-1c-1（console 写域解蔽）：禁切窗内 SGI 到——闭门不切。诉求分档
    // 承接（errata m2 勘正）：严格更高 eff 出窗检查点直切（µs 级），同
    // eff 轮转（本槽「同 eff 也切」语义）窗内被闭门、落本支下个 tick。
    // on_irq_tick 同款门，先于结构窗开启。原 g3-1a 配平断言退居防线位
    //（门后恒 count==0，零成本）。
    if preempt_count() > 0 {
        return;
    }
    assert_preempt_balanced("on_sgi_resched");
    // 幻影出选防线（同 try_preempt 注）：环上无他者必不出选，不消耗 head
    // 槽位。len 计 cur 自身（Running 仍驻环），≥2 = 有他者可出选。
    // RQ 窗（c-2b）：len 读 + 出选 + 状态定形同窗（len 读也是环面，同窗才
    // 不撕裂）；授片/切换在窗外（窗内禁切换）。
    let phys = rq_lock();
    let mut go: Option<*mut Task> = None;
    if rq().len() >= 2 {
        let picked = RR.pick_next_filtered(rq(), topo(), cur);
        if let Some(next) = picked {
            let next_ptr: *mut Task = next;
            if next_ptr != cur {
                (*cur).state = TaskState::Ready;
                (*next_ptr).state = TaskState::Running;
                go = Some(next_ptr);
            }
        }
    }
    rq_unlock(phys);
    if let Some(next_ptr) = go {
        (*next_ptr).slice_left = hal::timer::slice_cycles();
        // 授片即重编（绝对截止时间纪律，同 try_preempt/park_recheck）：
        // SGI 入口无 timer::handle 的 rearm-before-EOIR，CVAL 不重编 =
        // 授片无界——必须显式 arm。
        // SAFETY: IRQ 屏蔽窗内；写 CNTV 系统寄存器；EL1。
        hal::timer::arm_slice();
        set_current(next_ptr);
        // S-6 ① 补数：SGI resched 直切臂旧不计——跨核唤醒直切主干，同口径补齐。
        bump_switches();
        hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
        activate_if_changed(cur, next_ptr);
        note_switch(cur, next_ptr, SwitchKind::Preempt);
        // 切走前无锁可放 + 切后不重取（on_irq_tick 同款协议 3-1b 形，详彼处注）：
        // 被打断者不持锁（IRQ 屏蔽 ⇒ 非 SVC 主体），续行方按自身位自理。
        // SAFETY: PSTATE.I=1（IRQ 屏蔽）；prev/next 有效、不别名。
        hal::task::context_switch(&raw mut (*cur).ctx, &raw const (*next_ptr).ctx);
        // <- 被切回时续行于此（early-return：恢复路径经
        // IRQ 收尾 eret 回 cur 被断点——无锁可重取可放，防双重放锁）。
        return;
    }
    // 未切走路：直回（3-1b 起无 KL 配平义务；结构窗均已闭）。
}

/// S-3a 副核 park 重查（子案 §9.5 步 A）——自 `on_irq_tick` 的 cur==null
/// park 派发段整体迁出：**SGI 醒与 CNTV 醒收敛单路径**（exception_irq 的 SGI
/// 分支对副核只计数不派发——「SGI 不进 tick 槽」纪律不破；CPU0 侧 b-1 起经
/// [`on_sgi_resched`] 快速重调度，与本路径分立；两类 wfi 唤醒都经 eret
/// 回 secondary_main park 环后到此处）。
///
/// 语义与原段逐位同（RQ 窗出选 → 置 Running/授片/set_current →
/// activate → context_switch 入任务；切回点即本函数 context_switch
/// 调用行——任务侧 block/preempt 回 park 从此续行，return 回 park 环）。
/// 差异一处：派发路径**自行 arm_slice**（S-3a ⑤二分——授片 arm ≠ rearm，
/// rearm 锚仍在 timer::handle IRQ 入口：原「CVAL 已满片终点」假设只成立于
/// CNTV 醒；SGI 醒无 CNTV IRQ，CVAL 或为深 park mask 残留，不重编 = 授片
/// 无界）。
///
/// 仅副核（secondary_main park 环）调用；CPU0 侧结构性不达（其 idle 不进
/// park 环——S-3b IDLE 深化件）。gate 未开 = 早退零共享触达（探测态轮询
/// 免疫：gate 开启由 park 环 arm_slice 周期醒承载，≤1 slice 内此处开始
/// 真派发）。
///
/// # Safety
/// PSTATE.I=1（park 环屏蔽窗内调用）；结构窗自管（RQ/偷取各自独立窗）。
pub unsafe fn park_recheck() {
    if !hal::smp::par_enabled() {
        return;
    }
    assert_preempt_balanced("park_recheck");
    // S-3b（F8）：peek 只读版退役——本核队列游标私有，无回卷饥饿面；
    // 出选即消费版（advance_past 推进本核 FIFO）。my_cur=null：park 态
    // 环上无 Running 残留（任务离环只在 block/exit，切回 park 前已离）。
    // RQ 窗（c-2b）：出选 + 置 Running 同窗（防偷屏障——派发 helper 首行
    // 同值重写幂等，为偷取臂保留）；打名诊断/切换在窗外。
    let phys = rq_lock();
    let picked: Option<*mut Task> = RR
        .pick_next_filtered(rq(), topo(), core::ptr::null_mut())
        .map(|n| n as *mut Task);
    if let Some(np) = picked {
        (*np).state = TaskState::Running;
    }
    rq_unlock(phys);
    if let Some(next) = picked {
        let next_ptr: *mut Task = next;
        // S-2b 排障期诊断面（随段迁入）：派发计数 + 前 8 次打名（c1 恒 0 时
        // 证派发发生过、派发了谁——任务没活到下个 tick 的归因起点）。
        hal::smp::note_dispatch();
        {
            let d1 = hal::smp::diag_readout().2;
            if d1 <= 8 {
                hal::kern_lock::with_print_lock(|| {
                    hal::uart_puts(b"sS2b: disp #");
                    hal::uart_put_dec(d1);
                    hal::uart_puts(b" '");
                    hal::uart_puts((*next_ptr).name);
                    hal::uart_puts(b"'\n");
                });
            }
        }
        // S-3b b-2：派发尾提取 [`dispatch_from_park_ctx`]（本核出选与偷取
        // 同径；置 Running/授片/显式 arm/set_current/SWITCHES/激活/切入
        // 全在 helper——S-3a ⑤二分的授片侧语义不变）。
        // SAFETY: PSTATE.I=1（park 环屏蔽窗）；结构锁窗在 helper 体内已闭。
        dispatch_from_park_ctx(next_ptr);
        return;
    }
    // S-3b b-2 件①（子案 §9.6，义务②）：本核无货 → 偷取探测（对核环首
    // eligible 用户任务——F-E2 双过滤见 [`try_steal_one`]）；偷到随迁 home
    // 后与本核出选**同径派发**（LB 记账面同格——件④ per-child 归因的机制
    // 前提）。
    if let Some(next_ptr) = try_steal_one() {
        hal::smp::note_dispatch();
        // 偷取臂同款前 8 次打名（run7/8 c1=0 归因缺口：出选臂打印、偷取臂
        // 哑——st1 计数之外零可见性，偷了谁不可考；「副核只偷了一只常驻者
        // 后粘性占用、探测停摆」的定谳就缺这行字）。
        {
            let d1 = hal::smp::diag_readout().2;
            if d1 <= 8 {
                hal::kern_lock::with_print_lock(|| {
                    hal::uart_puts(b"sS2b: steal #");
                    hal::uart_put_dec(d1);
                    hal::uart_puts(b" '");
                    hal::uart_puts((*next_ptr).name);
                    hal::uart_puts(b"'\n");
                });
            }
        }
        // SAFETY: PSTATE.I=1（park 环屏蔽窗）；结构锁窗在 helper 体内已闭。
        dispatch_from_park_ctx(next_ptr);
        return;
    }
    // 无货：直回环（park_arm 承接武装策略；3-1b 起无 KL 配平义务）。
}

// ---- S-3b 步 b-2（子案 §9.6）：偷取初版 + A7 IDLE 合流 ----

/// 偷取初版（义务②）：本核出选 None 后扫对核环，摘首 eligible 有表任务。
/// F-E2 双过滤：①有地址空间者才偷（`ttbr0 != 0`——**无表**内核任务永不
/// 跨核（内核串行纪元不变量），被此臂滤出；带表内核任务（task_a/task_b
/// 隔离金丝雀类，EL1 跑）与 EL0 用户进程皆过此臂——b-2a 实测前者跨核迁移
/// 随环轮转健康）②非 Running 态（环上构造性 Ready；[`crate::class::eligible`]
/// 单臂 my_cur=null 排一切 Running——对核 cur 在其环上 Running，天然跳过，
/// 偷不得）。
/// 触发 = 本核 pick None（idle/park 空转点——非负载阈值探测）；粒度单任务；
/// 双向对称。无乒乓：偷到即**随迁改写 home**（`place()` 注的 b-2 唯一改写点
/// ——此后 wake/expire 再入队回新家核，迁移粘性）；**摘环即回插本核环**
/// （体尾 F-E2 补——出选臂 pick「留环只推游标」，偷来者同形，绝无裸跑）。
///
/// # Safety
/// IRQ 屏蔽 + 独立远端 RQ 真锁窗（3-1b 起无 KL 包络——窗口正确性自足）；
/// 本核出选已 None 且**本核 RQ 窗已闭**（调用契约：先 pick 放锁后
/// 偷——两调用面 idle_loop/park_recheck 均在 rq_unlock 之后）。
/// 环手术锁形 = §3 特则「**只持远端**」（c-2b slice 5 真锁包络）：远端
/// 扫描+摘环一窗 → 放远端 → 无归属窗 → 回插本核再取本地——**绝不双持**
/// （F-L1「双持一律核号升序」特则本路径构造性不触发——无归属窗单点入队
/// 即「双队列同时出现」不变量的构造保证）。
unsafe fn try_steal_one() -> Option<*mut Task> {
    if !hal::smp::par_enabled() {
        return None;
    }
    let me = hal::smp::cpu_id();
    let other = 1 - me;
    let src = rq_of(other);
    // —— 远端窗（只持远端）：对核环扫描（has_eff_above_filtered 同款走查
    // 形）：首过双过滤者即 victim；整圈无货 None。扫描与摘环同窗——victim
    // 定型到 unlink 原子，防扫描后摘环前被对核 pick/出队抢走。断链形态由
    // link/unlink 入口断言执法（环不变式），走查「环上必有 next」与
    // ring_check 同口径。
    let phys = rq_lock_of(other);
    let Some(h) = src.head() else {
        rq_unlock_of(other, phys); // 对核空环——窗闭
        return None;
    };
    let mut cur = h;
    let mut victim = None;
    loop {
        let p = cur.as_ptr();
        if (*p).spread
            && (*p).ttbr0 != 0
            && crate::class::eligible(core::ptr::null_mut(), p)
        {
            victim = Some(cur);
            break;
        }
        // SAFETY: 环上节点必有 next（锁窗内无并发出/入环）。
        let next = (*p).rq_next.get().expect("steal scan: on-ring node must have next");
        if next == h {
            break; // 整圈扫完
        }
        cur = next;
    }
    let Some(v) = victim else {
        rq_unlock_of(other, phys); // 整圈无 eligible——窗闭
        return None;
    };
    let p = v.as_ptr();
    // SAFETY: 远端锁窗内；unlink 幂等语义不涉（v 构造性在 src 环上）；
    // head==v 时游标自前移（对核 pick 不失位）。不走 RR.dequeue
    // ——那会置 Exited（block_current 同款规避，偷取者保持 Ready）。
    src.unlink(v);
    // 远端窗闭——victim 自此刻无队列归属（§3 特则四步：放远端 → 无归属
    // 窗 → 入本核再取本地；home 随迁在无归属窗内独占写——对核任何路径
    // 此刻都触不到它）。
    rq_unlock_of(other, phys);
    (*p).home_cpu = me as u8; // 随迁改写 home（迁移粘性）
    // 摘环后必须回插本核环（F-E2 补：偷取与出选的形态对齐——出选臂经
    // pick_next_filtered 者「留环只推游标」，偷来者不可裸跑）。缺此步则
    // victim 一旦走 preempt 让出（忙循环任务必经——无 block/wake 重入环
    // 链）即成「Ready 却不在任何环」的孤儿，永不被 pick：b-2a 首版实测
    // task_a/task_b 被偷后饿死绝迹、rq0 偷空后 CPU0 深 park 无醒源永睡
    // （tick/tele 全死），短路偷取 400s 复跑全绿定谳。enqueue 置 Ready、
    // 调用侧 dispatch 随即置 Running——与出选派发同序。
    // —— 本核窗（再取本地）：与前远端窗不重叠（只持远端——绝不双持）。
    let phys = rq_lock_of(me);
    RR.enqueue(&mut *p, rq_of(me));
    rq_unlock_of(me, phys);
    // 偷取事件全局计数（st1 诊断面——`LB ok` 行诊断字段，不做断言）。
    LB_STEALS.fetch_add(1, Ordering::Relaxed);
    Some(p)
}

/// park 派发公共尾（副核）：置 Running + 授片 + **显式 arm**（S-3a ⑤二分的
/// 授片侧——本路径无 CNTV IRQ 先行，SGI 醒/浅 park 到点醒后 CVAL 非满片
/// 终点，不重编 = 授片无界）+ set_current + SWITCHES（A12 全上下文切换计数）
/// + 无条件激活（park 帧无 AS 归属——上次派发者表可能仍驻 TTBR0，故恒显式
/// 激活；F-E2 偷取过滤 = `ttbr0 != 0` = **有地址空间者**——EL0 用户进程与
/// 挂表内核任务（task_a/task_b 隔离金丝雀类，EL1 跑）皆可偷，b-2a 实测
/// 后者跨核迁移随环轮转健康）+ 切入。本核出选与偷取同径（LB 同格
/// 合格）。note_switch prev=null 跳过发射（park 非调度语义上下文）；sS2b
/// 派发诊断（note_dispatch/打名）留调用侧（出选/偷取两臂各记）。
///
/// # Safety
/// PSTATE.I=1（park 环屏蔽窗）；结构锁窗已闭（RQ 窗/偷取窗在调用侧）；
/// `next_ptr` 有效且已在本核环（出选留环 / 偷取回插——派发即置 Running 于
/// 环，cur-in-ring 常态，对核 pick 的 eligible 滤跳过）。
unsafe fn dispatch_from_park_ctx(next_ptr: *mut Task) {
    (*next_ptr).state = TaskState::Running;
    (*next_ptr).slice_left = hal::timer::slice_cycles();
    // SAFETY: 本核 CVAL 重臂；PSTATE.I=1。
    unsafe { hal::timer::arm_slice() };
    set_current(next_ptr);
    bump_switches(); // S-6 ①：递增+打印共享 helper（A12 兑现）
    hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
    // 刀1：park 派发无 note_switch（prev=null 不发射遥测），但任务语境基线
    // 仍须落 IPC——否则 CPU1 首任务/下次 park 醒来前的唤醒会读到初始 NONE。
    hal::wake::wake_src_task_ctx();
    hal::activate_aspace((*next_ptr).ttbr0, (*next_ptr).asid);
    // SAFETY: prev=本核 park ctx（惰性捕获——首次派发把调用帧存进本核槽）、
    // next 有效。next 首切经 trampoline eret 落 EL0；任务侧 block/preempt 回
    // park 从本调用行下续行 → return 回环。
    unsafe {
        hal::task::context_switch(
            hal::smp::sec_park_ctx(),
            &raw const (*next_ptr).ctx,
        )
    };
}

/// CPU0 idle 派发（A7 合流形，雷① 收口）：prev = **IDLE_TASK.ctx**（非
/// sec_park_ctx——block/exit 的 pick-None 臂切回 `IDLE_TASK.ctx`，与本处
/// prev 同槽帧一致：idle_loop 派发帧被捕获进 IDLE.ctx，任务侧空转落回时
/// 恰续行于本函数 context_switch 调用行下 → return 回 idle 环顶）。
/// activate_if_changed（idle 在主表或上次 next 表，差量激活——与 tick/SGI
/// preempt-from-IDLE 同形；next 可为内核任务 ttbr0=0，不可无条件激活）。
/// note_switch prev=IDLE（真调度语义上下文，Preempt 记账与 tick 派发对齐）。
///
/// # Safety
/// PSTATE.I=1（idle 环屏蔽窗）；结构锁窗已闭（RQ 窗/偷取窗在 idle_loop）；
/// `next_ptr` 有效。
unsafe fn dispatch_from_idle(next_ptr: *mut Task) {
    // G3-1a 断言补位（f4 m1 修复批）：idle 派发恒切换（无「出选空」支），
    // 入口即切换原语语义——第八入口，至此切换原语配平断言全员在席（八入口）。
    assert_preempt_balanced("dispatch_from_idle");
    let idle = addr_of_mut!(IDLE_TASK);
    (*next_ptr).state = TaskState::Running;
    (*next_ptr).slice_left = hal::timer::slice_cycles();
    // SAFETY: 本核 CVAL 重臂（授片侧显式 arm——同 dispatch_from_park_ctx，
    // idle 派发无 CNTV IRQ 先行：wfi 醒 / SGI 醒 / 环顶重查三种入口皆然）。
    unsafe { hal::timer::arm_slice() };
    set_current(next_ptr);
    bump_switches(); // S-6 ①：递增+打印共享 helper（A12 兑现）
    hal::smp::note_switch_cpu(); // S-5a per-CPU 统计（本核独占写）
    activate_if_changed(idle, next_ptr);
    note_switch(idle, next_ptr, SwitchKind::Preempt);
    // SAFETY: prev=IDLE.ctx（idle_loop 帧惰性捕获，与任务侧 None 臂切回点
    // 同槽——见函数头）、next 有效。切回即本行下 → return 回 idle 环。
    unsafe {
        hal::task::context_switch(&raw mut (*idle).ctx, &raw const (*next_ptr).ctx)
    };
}

/// SLEEPQ 最近截止时刻（S-3a tickless park 武装面——子案 §9.5 步 A；
/// 头未到期即最近，空队 None = 深 park）。锁窗内读队头（与
/// [`expire_timeouts`] 同口径——升序队头即最近到期）。
///
/// 调用契约：IRQ 屏蔽窗内（secondary_main park 环天然满足）；**不持任何
/// 结构位**（今日两处调用点皆 park 空态——CURRENT=null 恒真取；任务语境
/// 误入持非 SQ 位 = SQ 内嵌序破，调用方自证）。
///
/// S-3c slice 3-1a：KL-only 裸读（§A113 判定的全域最后一位）改 SQ 锁对
/// ——c-2b slice 4 起 SLEEPQ 一切写者已持 SQ 窗（五发射点），本读换 SQ
/// 即闭合「SLEEPQ 全触面纯 SQ 串行」，0x37 随刀降档免 KL（syscall.rs）。
pub fn nearest_deadline() -> Option<u64> {
    let phys = sq_lock();
    // SAFETY: SQ 锁窗内只读队头（head 经 `sleepq()` 静态单例；SLEEPQ 一切
    // 变更同窗串行——写者清单 = slice 4 五发射点 + 本读，KL 依赖就此归零）。
    let d = unsafe {
        sleepq().head.get().map(|h| (*h.as_ptr()).wait_deadline)
    };
    sq_unlock(phys);
    d
}
