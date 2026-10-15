//! 任务（内核线程）对象 + 状态。

use alloc::boxed::Box;
use alloc::vec;
use bos_hal_aarch64::task::TaskContext;
use core::cell::Cell;
use core::ptr::NonNull;

/// 内核线程的默认栈大小（16KiB）。v1 切换在 IRQ-mask 中（无嵌套 IRQ）、无深递归，
/// 够用；守卫页 TODO（plan R7）。
pub const STACK_SIZE: usize = 16 * 1024;

/// 能效倾向 hint（EPP，`调度器设计.md` §6）：指引大小核放置与改频。字段先落地
/// （2b-4），消费语义随放置路径（M1b）/PM 服务（M3）兑现。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EppHint {
    /// 效率优先（默认——省电是身份支柱）。
    Efficiency,
    /// 性能优先。
    Perf,
}

/// 任务生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// 在 runqueue 上、可被 pick。
    Ready,
    /// 正在 CPU 上跑（`current`）。
    Running,
    /// 已从入口返回 / 主动退出；v1 暂不回收（idle 循环不 free，见 plan Step 3）。
    Exited,
}

/// 内核线程。拥有自己的内核栈；`ctx` 存协作者切换所需的 callee-saved（HAL）。
///
/// runqueue 链表是**侵入式双向环形**的（`rq_prev`/`rq_next`），用 `Cell` 以便
/// `pick_next(&Runqueue)` 在不可变借用下旋转游标（[`crate::runqueue::Runqueue`]）。
///
/// **所有权/稳定性**：`Task` 由调度器以 `Box<Task>` 拥有、地址稳定（runqueue 与
/// `current` 持有的裸指针即源自该 Box——须在其被引用期间保持存活、不移动）。
///
/// **idle/初始任务例外**（plan Bug 4）：复用 boot 栈、`kstack=None`，作为 `const`
/// 静态存在（首切出时由 `switch` 把活 callee-saved+boot 栈 SP 覆盖进 `ctx`）。
pub struct Task {
    /// 协作者切换上下文（HAL 的 callee-saved 子集）。
    pub ctx: TaskContext,
    /// 该任务私有内核栈（栈顶 16 对齐）。`None` = 复用 boot 栈（仅 idle/初始任务）。
    pub kstack: Option<Box<[u8]>>,
    /// 调试名（串口打印）。
    pub name: &'static [u8],
    pub state: TaskState,
    /// 剩余时间片（**周期数**，2b-4 起）：由 `tick` 按 CNTVCT 增量递减（DVFS 后
    /// 实际时长不随频率漂移）；0 表示该被重调度。授予量 = `timer::slice_cycles()`。
    pub slice_left: u64,
    /// 2b-2：任务地址空间 = TTBR0 页表 PA + ASID。`ttbr0 == 0` = 无 AS（内核空表，
    /// 低半必然 fault——身份表已退役）。switch 时经 HAL 激活（变化才写寄存器）。
    pub ttbr0: usize,
    /// 2b-2：`ttbr0` 对应的 ASID（0 = 无 AS）。
    pub asid: u16,

    // ---- 2b-4 表示法字段（`调度器设计.md` §3.6/§6：字段随 Phase 2b 落地，语义随后续
    // Phase 兑现——RR 不消费，为 EAS/PI/AI 调度预埋，免后改 TCB 布局返工）----
    /// 基础优先级（越大越高；`class_rt`/`class_deadline` 用，RR 不用）。
    pub prio_base: i32,
    /// 有效优先级 = max(base, 继承)（§3.6 PI 双优先级：判定一律看 effective）。
    /// v1 无 IPC/继承，恒等于 `prio_base`；P4 channel 落地后开始分叉。
    pub prio_effective: i32,
    /// 预期运行时长 hint（纳秒；0 = 未指定）——AI 调度慢/快循环切分词汇
    /// （`AI调度设计.md`）。
    pub duration_hint_ns: u64,
    /// 能效倾向（EPP，§6）。
    pub epp: EppHint,
    /// 抢占阈值（ThreadX 语义：优先级不高于阈值者不抢占本任务；0 = 不设）——
    /// v3 AI 护栏词汇（`AI调度设计.md` 护栏节）。
    pub preempt_threshold: i32,

    // ---- 侵入式双向环形链表链接（runqueue 用；`Cell` 以支持 `&Runqueue` 下旋转）----
    pub(crate) rq_prev: Cell<Option<NonNull<Task>>>,
    pub(crate) rq_next: Cell<Option<NonNull<Task>>>,
}

impl Task {
    /// 分配一个新内核线程：16KiB 栈 + 由 `new_kernel` 接线的初始上下文。
    ///
    /// 返回的 `Box<Task>` 地址即 runqueue / `current` 持有的裸指针来源。`entry` 须
    /// `-> !`（`exit` 原语或 loop 收尾；误返回经 trampoline 落 `task_exited_trap`）。
    pub fn new_kernel(
        entry: extern "C" fn(usize) -> !,
        arg: usize,
        name: &'static [u8],
    ) -> Box<Task> {
        Self::new_kernel_with_as(entry, arg, name, 0, 0)
    }

    /// 同 [`Task::new_kernel`]，但挂地址空间（2b-2：`ttbr0`/`asid`，0/0 = 无 AS）。
    pub fn new_kernel_with_as(
        entry: extern "C" fn(usize) -> !,
        arg: usize,
        name: &'static [u8],
        ttbr0: usize,
        asid: u16,
    ) -> Box<Task> {
        let (kstack, stack_top) = Self::alloc_kstack();
        let ctx = TaskContext::new_kernel(entry, arg, stack_top);
        Self::assemble(ctx, kstack, name, ttbr0, asid)
    }

    /// 用户线程版（2b-4 首个 EL0）：`entry_va`/`sp0_va` 为**用户 VA**（须已在
    /// `ttbr0` 页表映射、AP 放行 EL0——`Perm::UserRw`），内核侧仍走 trampoline
    /// 接线（首切 `eret` 降 EL0，见 HAL `user_entry_trampoline`）。内核栈同
    /// 16KiB（EL0 异常帧 + handler 都跑在它上面）。
    pub fn new_user_with_as(
        entry_va: usize,
        arg: usize,
        sp0_va: usize,
        name: &'static [u8],
        ttbr0: usize,
        asid: u16,
    ) -> Box<Task> {
        let (kstack, stack_top) = Self::alloc_kstack();
        let ctx = TaskContext::new_user(entry_va, arg, sp0_va, stack_top);
        Self::assemble(ctx, kstack, name, ttbr0, asid)
    }

    /// 出 16KiB 内核栈 + 栈顶地址（栈向下长，顶 = 最高地址；页对齐 → 16 对齐）。
    fn alloc_kstack() -> (Box<[u8]>, usize) {
        let kstack: Box<[u8]> = vec![0u8; STACK_SIZE].into_boxed_slice();
        let top = kstack.as_ptr() as usize + STACK_SIZE;
        (kstack, top)
    }

    /// 装配 Task 主体（两种入口共用；表示法字段全默认值）。
    fn assemble(
        ctx: TaskContext,
        kstack: Box<[u8]>,
        name: &'static [u8],
        ttbr0: usize,
        asid: u16,
    ) -> Box<Task> {
        Box::new(Task {
            ctx,
            kstack: Some(kstack),
            name,
            state: TaskState::Ready,
            slice_left: 0,
            ttbr0,
            asid,
            prio_base: 0,
            prio_effective: 0,
            duration_hint_ns: 0,
            epp: EppHint::Efficiency,
            preempt_threshold: 0,
            rq_prev: Cell::new(None),
            rq_next: Cell::new(None),
        })
    }
}
