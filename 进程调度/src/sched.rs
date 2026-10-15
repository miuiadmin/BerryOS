//! 协作调度器核心（plan Phase 2a Step 3）。
//!
//! 单核、抢占点（G0 协作式）：`yield_now` / `exit_current` 是唯一切换点，**无 timer
//! 驱动**（Step 4/6 起）。`current` 指向当前任务；`IDLE_TASK` 是 boot 上下文
//! （`rust_main` 的活栈——Bug 4 懒捕获，首切出时由 `switch` 把活 callee-saved+sp
//! 存进它的 ctx）。
//!
//! 承载不变式（plan Bug 2）：`switch` 永不在 `PSTATE.I=0`（中断解蔽）时到达——
//! `yield_now`/`exit_current` 均先 `disable_irq` 再切。

use crate::class::{RoundRobin, SchedClass};
use crate::runqueue::{Runqueue, Topo};
use crate::task::{Task, TaskState};
use berry_hal_aarch64 as hal;
use core::ptr::addr_of_mut;

/// IRQ tick 计数（`on_irq_tick` 自增）。金丝雀：长跑中持续增长 = timer/EOIR 管线健康
/// （rearm 对、无 EOIR 饿死——plan R4/R5）；停增即管线坏。
static mut IRQ_TICKS: u64 = 0;
/// 上下文切换计数（`on_irq_tick` 抢占切一次自增）。里程碑金丝雀：两任务在 timer slice
/// 下分时 → 计数持续涨（≥数千 = G1 过关）；冻结在某值 = switch 坏（卡死/饿死）。
static mut SWITCHES: u64 = 0;

/// 当前在 CPU 上跑的任务。boot 起 = `&IDLE_TASK`（由 [`init`] 设）。
static mut CURRENT: *mut Task = core::ptr::null_mut();
/// 全局 runqueue（G0 单核：一个）。
static mut RUNQUEUE: Runqueue = Runqueue::new();
/// CPU 拓扑（单核）。
static TOPO: Topo = Topo::single();
/// 调度类（round-robin，G0/G1 默认）。
static RR: RoundRobin = RoundRobin;

/// idle / 初始任务 = `rust_main` 的活上下文（boot 栈 `__stack_top`）。**不分配栈**
/// （Bug 4）：`kstack=None`、`ctx` 留零——首切出时由 `switch` 覆盖为活的 callee-saved
/// + boot 栈 SP。idle 永不在 runqueue 上（runqueue 空时停在此）。
static mut IDLE_TASK: Task = Task {
    ctx: hal::task::TaskContext::ZERO,
    kstack: None,
    name: b"idle",
    state: TaskState::Running,
    slice_left: 0,
    ttbr0: 0,
    asid: 0,
    prio_base: 0,
    prio_effective: 0,
    duration_hint_ns: 0,
    epp: crate::task::EppHint::Efficiency,
    preempt_threshold: 0,
    proc_tag: 0,
    rq_prev: core::cell::Cell::new(None),
    rq_next: core::cell::Cell::new(None),
};

/// 取全局 runqueue 的 `&mut`（静态单例；单核顺序访问，借用不重叠）。
#[inline]
unsafe fn rq() -> &'static mut Runqueue {
    &mut *addr_of_mut!(RUNQUEUE)
}

/// 初始化调度器：`current = &IDLE_TASK`。须在 `spawn` / `yield_now` 前、`mm::init`
/// 后调用一次。
pub fn init() {
    // SAFETY: 启动单线程、init 仅调用一次。
    unsafe {
        CURRENT = addr_of_mut!(IDLE_TASK);
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
pub fn spawn_as(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8],
                ttbr0: usize, asid: u16) {
    // SAFETY: CURRENT 已由 init 设好；allocator 已就绪。
    let task: &'static mut Task =
        alloc::boxed::Box::leak(Task::new_kernel_with_as(entry, arg, name, ttbr0, asid));
    unsafe {
        RR.enqueue(task, rq());
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
    try_spawn_user(entry_va, arg, sp0_va, name, ttbr0, asid, proc_tag)
        .expect("spawn_user：用户任务装配（boot 可信语境）")
}

/// 同 [`spawn_user`] 但 try 化（3b3 审查 H2）：16KiB 栈 / TCB 装箱任一 OOM 返
/// `None`、**不入队、零副作用**——EL0 经 `process_start` 触发时内核侧折
/// -ENOMEM（原路径 abort = EL0 可触发内核 panic，违 ABI §8.0）。前置同
/// [`spawn`]（IRQ 屏蔽态）。
pub fn try_spawn_user(entry_va: usize, arg: usize, sp0_va: usize, name: &'static [u8],
                      ttbr0: usize, asid: u16, proc_tag: usize) -> Option<()> {
    let task = Task::try_new_user_with_as(entry_va, arg, sp0_va, name, ttbr0, asid, proc_tag)?;
    // SAFETY: CURRENT 已由 init 设好；allocator 已就绪；IRQ 屏蔽态。
    let task: &'static mut Task = alloc::boxed::Box::leak(task);
    unsafe {
        RR.enqueue(task, rq());
    }
    Some(())
}

/// 当前任务的进程标签（3-4a per-process handle 表派发用）。0 = 无（内核任务/
/// `init` 之前的 boot 期——ktest 在 `init` 前跑，此时 CURRENT 为 null → 0）。
/// 语义归 `内核` crate；本函数只透传，单核只读。
pub fn current_tag() -> usize {
    // SAFETY: 只读；单核顺序访问（写点全在 IRQ 屏蔽路径）。
    let cur = unsafe { CURRENT };
    if cur.is_null() {
        0
    } else {
        unsafe { (*cur).proc_tag }
    }
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
pub fn yield_now() {
    hal::disable_irq();
    // SAFETY: IRQ 已屏蔽；CURRENT/RUNQUEUE 单核顺序访问。
    unsafe {
        let prev = CURRENT;
        if let Some(next) = RR.pick_next(rq(), &TOPO) {
            let next_ptr: *mut Task = next;
            if next_ptr != prev {
                (*prev).state = TaskState::Ready;
                (*next_ptr).state = TaskState::Running;
                (*next_ptr).slice_left = hal::timer::slice_cycles();
                // 授片即重编（`调度器设计.md` §4.1 绝对截止时间纪律）：以授片时刻的
                // CNTVCT 现读重算绝对截止——新任务从授片时刻起拿满片，不继承 prev
                // 的残余倒计。
                // SAFETY: IRQ 已被 disable_irq 屏蔽；写 CNTV 系统寄存器；EL1。
                hal::timer::arm_slice();
                CURRENT = next_ptr;
                activate_if_changed(prev, next_ptr);
                note_switch(prev, next_ptr, SwitchKind::Yield);
                hal::task::context_switch(&raw mut (*prev).ctx, &raw const (*next_ptr).ctx);
                // <- 被切回 prev 时续行于此（CURRENT==prev）。
            }
        }
    }
    hal::enable_irq();
}

/// 当前任务退出：出 runqueue + 切到下一个（或 idle）。永不返回。
pub fn exit_current() -> ! {
    hal::disable_irq();
    // SAFETY: IRQ 已屏蔽；cur 已 dequeue 后无人切回它。
    unsafe { switch_away_forever() }
}

/// 用户任务致死计数（G1.5 验收：watchdog 断言 == 期望杀数）。
static mut USER_KILLS: u64 = 0;

/// 最近被杀用户任务名（G1.5 验收：watchdog 断言**杀的是 rogue**——杀数==1 但错杀
/// good/abuser 不得过关；单核 IRQ 屏蔽路径写，watchdog 只读）。
static mut LAST_KILLED: &[u8] = b"";

/// 当前用户任务因致命故障被杀（HAL `exception_sync_el0` 经函数指针槽调；2b-4
/// G1.5——**只死自己**，内核与其余任务无恙）。打印 + 计数 + 切走，永不返回：
/// 被弃的是故障帧所在的内核栈（同步异常入口 PSTATE.I=1，无 GIC 状态要收拾）。
///
/// # Safety
/// 同步异常上下文（PSTATE.I=1）；C-ABI fn 指针槽；CURRENT 是用户任务。
pub unsafe extern "C" fn user_fault_exit() -> ! {
    let cur = CURRENT;
    USER_KILLS = USER_KILLS.wrapping_add(1);
    LAST_KILLED = (*cur).name;
    hal::uart_puts(b"\nG1.5: kill user task '");
    hal::uart_puts((*cur).name);
    hal::uart_puts(b"'\n");
    switch_away_forever()
}

/// exit/kill 共用尾部：当前任务出 runqueue + 切到下一个（或 idle），永不回到它。
///
/// # Safety
/// IRQ 已屏蔽（显式 disable_irq 或异常入口硬件屏蔽）；CURRENT 有效。
unsafe fn switch_away_forever() -> ! {
    let cur = CURRENT;
    debug_assert!((*cur).state != TaskState::Exited, "double exit");
    RR.dequeue(&mut *cur, rq());
    let next_ptr: *mut Task = match RR.pick_next(rq(), &TOPO) {
        Some(n) => n,
        None => addr_of_mut!(IDLE_TASK),
    };
    (*next_ptr).state = TaskState::Running;
    (*next_ptr).slice_left = hal::timer::slice_cycles();
    // 授片即重编（同 yield_now；`调度器设计.md` §4.1 绝对截止时间纪律）——授片给
    // 真任务才 arm：切去 idle 不 arm（idle_loop 会 mask CNTV，arm 反引入一次空 tick）。
    // SAFETY: IRQ 已屏蔽（disable_irq 或异常入口硬件屏蔽）；写 CNTV 系统寄存器；EL1。
    if next_ptr != addr_of_mut!(IDLE_TASK) {
        hal::timer::arm_slice();
    }
    CURRENT = next_ptr;
    activate_if_changed(cur, next_ptr);
    note_switch(cur, next_ptr, SwitchKind::Exit);
    // prev=cur（已出 runqueue、将废弃）；切走后永不回到 cur。
    hal::task::context_switch(&raw mut (*cur).ctx, &raw const (*next_ptr).ctx);
    // 不可达：cur 已 dequeue，无人切回。halt 兜底。
    hal::halt();
}

/// switches 金丝雀只读访问（watchdog / 遥测读；单核顺序）。
pub fn switches() -> u64 {
    // SAFETY: 只读；单核顺序访问。
    unsafe { SWITCHES }
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
#[inline]
unsafe fn note_switch(prev: *mut Task, next: *mut Task, kind: SwitchKind) {
    // SAFETY: 两指针由调用点保证有效（刚做过状态写入）；IRQ 屏蔽路径。
    let hook = unsafe { SWITCH_HOOK };
    if let Some(h) = hook {
        h(unsafe { &*prev }, unsafe { &*next }, kind);
    }
}

/// 用户任务致死计数只读访问（G1.5 watchdog 断言用）。
pub fn user_kills() -> u64 {
    // SAFETY: 只读；单核顺序访问。
    unsafe { USER_KILLS }
}

/// 最近被杀用户任务名只读访问（watchdog 断言「杀对了人」用；空串 = 尚无杀）。
pub fn last_killed() -> &'static [u8] {
    // SAFETY: 只读；单核顺序访问。
    unsafe { LAST_KILLED }
}

/// idle 主循环（tickless）：runqueue 空时屏蔽 CNTV + `wfi`（空闲核不收周期中断）；
/// 有 runnable 任务时武装 slice + 让出。Step 4 跑空分支（mask+wfi = 干净 tickless
/// halt，无 IRQ 源）；Step 5+ 被 IRQ 唤醒后回顶重检 runqueue。
///
/// **先 `disable_irq` 再判/`mask`**：保证 mask 与解蔽之间无窗口漏进早 IRQ（风险 R6）。
pub fn idle_loop() -> ! {
    loop {
        hal::disable_irq();
        // SAFETY: IRQ 已屏蔽；单核顺序访问 RUNQUEUE。
        let empty = unsafe { rq().is_empty() };
        if empty {
            // tickless：空闲不 arm、屏蔽 CNTV，wfi 睡。
            // SAFETY: 写 CNTV_CTL；EL1。
            unsafe { hal::timer::mask() };
            hal::enable_irq(); // 允许 IRQ 唤醒 wfi（Step 4 无源 → 永睡 = 干净 halt）
            hal::wfi();
            // 被唤醒（Step 5+）→ 回 loop 顶重检 runqueue。
        } else {
            // 有活：武装一个时间片，让给 runnable 任务（Step 6 起由 IRQ 驱动抢占）。
            // SAFETY: 写 CNTV 系统寄存器；EL1。
            unsafe { hal::timer::arm_slice() };
            hal::enable_irq();
            yield_now();
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
pub unsafe extern "C" fn on_irq_tick(_tf: &mut hal::trap::TrapFrame) {
    // SAFETY: IRQ 上下文、单核无并发（IRQ 入口硬件屏蔽，不嵌套）。
    IRQ_TICKS = IRQ_TICKS.wrapping_add(1);
    let cur = CURRENT;
    RR.tick(&mut *cur, rq()); // 策略部分：减 slice_left（饱和到 0）
    if (*cur).slice_left == 0 {
        if let Some(next) = RR.pick_next(rq(), &TOPO) {
            let next_ptr: *mut Task = next;
            if next_ptr != cur {
                (*cur).state = TaskState::Ready;
                (*next_ptr).state = TaskState::Running;
                (*next_ptr).slice_left = hal::timer::slice_cycles();
                // 授片重编已由 timer::handle 的 rearm-before-EOIR 在 IRQ 入口覆盖
                // （同一中断上下文、微秒级差）——CVAL 已是满片终点，此处不再 arm。
                CURRENT = next_ptr;
                SWITCHES = SWITCHES.wrapping_add(1);
                activate_if_changed(cur, next_ptr);
                note_switch(cur, next_ptr, SwitchKind::Preempt);
                // bring-up 诊断（G1）：每 N 次切换打一行计数，证分时持续、计数不冻。
                // 过关后移除或 feature 门控（plan Step 7）。
                const SOAK_PRINT_EVERY: u64 = 100;
                if SWITCHES % SOAK_PRINT_EVERY == 0 {
                    hal::uart_puts(b"switches=");
                    hal::uart_put_dec(SWITCHES);
                    hal::uart_puts(b"\n");
                }
                // 切出 cur：存 callee-saved+sp，载 next；ret 进 next（首切→trampoline，
                // 否则→next 上次切出处）。切回 cur 时续行于下一行。
                // SAFETY: PSTATE.I=1（IRQ 屏蔽）；prev/next 有效、不别名。
                hal::task::context_switch(&raw mut (*cur).ctx, &raw const (*next_ptr).ctx);
            }
        }
    }
}
