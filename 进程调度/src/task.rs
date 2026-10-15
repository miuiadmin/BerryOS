//! 任务（内核线程）对象 + 状态。

use alloc::boxed::Box;
use bos_hal_aarch64::task::TaskContext;
use core::cell::Cell;
use core::ptr::NonNull;

/// Fallible 装箱（3b3 审查 H2 try 化；与 `内核/src/heap.rs` 的 `try_box` 同义
/// ——本 crate 不依赖内核 crate，故本地一份）：`Box::new` OOM 即 abort，EL0
/// 经 `process_start` 装配任务可触发，违 ABI §8.0；syscall 可达路径一律走
/// try 变体，boot/ktest 可信语境仍可 expect。
fn try_box<T>(val: T) -> Option<Box<T>> {
    let layout = core::alloc::Layout::for_value(&val);
    // SAFETY: layout 非零（T 至少 1 字节）；走全局分配器。
    let ptr = unsafe { alloc::alloc::alloc(layout) } as *mut T;
    if ptr.is_null() {
        return None; // OOM：不 abort（Box::new 走 handle_alloc_error 即 abort）
    }
    // SAFETY: ptr 有效未初始化——write 转移所有权。
    unsafe { ptr.write(val) };
    // SAFETY: 该块按全局分配器 + for_value 布局分配，from_raw 收回构成合法 Box
    // （std `Box::try_new` 内部实现同构；try_new 本体仍锁 allocator_api）。
    Some(unsafe { Box::from_raw(ptr) })
}

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
    /// 3-4a：任务所属进程的标签（调用方传入的不透明指针，0 = 无——内核任务/
    /// 早期 boot）。语义归 `内核` crate（syscall 层解释为该任务的 handle 表指针，
    /// per-process 表的派发载体）；sched 层只存取、不解释——保持 crate 边界
    /// 干净（进程调度不反向依赖内核）。
    pub proc_tag: usize,

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
    /// **可信语境便捷包装**（OOM 直接 expect）；EL0 可达路径用 try 系。
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
        Self::try_new_kernel_with_as(entry, arg, name, ttbr0, asid)
            .expect("内核线程装配（boot/ktest 可信语境）")
    }

    /// [`Task::new_kernel_with_as`] 的 try 版（3b3 H2）：栈/TCB 任一分配失败返
    /// `None` 而非 abort。
    pub fn try_new_kernel_with_as(
        entry: extern "C" fn(usize) -> !,
        arg: usize,
        name: &'static [u8],
        ttbr0: usize,
        asid: u16,
    ) -> Option<Box<Task>> {
        let (kstack, stack_top) = Self::try_alloc_kstack()?;
        let ctx = TaskContext::new_kernel(entry, arg, stack_top);
        Self::try_assemble(ctx, kstack, name, ttbr0, asid, 0)
    }

    /// 用户线程版（2b-4 首个 EL0）：`entry_va`/`sp0_va` 为**用户 VA**（须已在
    /// `ttbr0` 页表映射、AP 放行 EL0——`Perm::UserRw`），内核侧仍走 trampoline
    /// 接线（首切 `eret` 降 EL0，见 HAL `user_entry_trampoline`）。内核栈同
    /// 16KiB（EL0 异常帧 + handler 都跑在它上面）。`proc_tag` = 所属进程标签
    /// （3-4a：syscall 层映射为该进程的 handle 表指针；0 = 无表——内核任务）。
    /// **可信语境便捷包装**（boot 直构用户进程用；OOM 直接 expect）。
    pub fn new_user_with_as(
        entry_va: usize,
        arg: usize,
        sp0_va: usize,
        name: &'static [u8],
        ttbr0: usize,
        asid: u16,
        proc_tag: usize,
    ) -> Box<Task> {
        Self::try_new_user_with_as(entry_va, arg, sp0_va, name, ttbr0, asid, proc_tag)
            .expect("用户线程装配（boot 可信语境）")
    }

    /// [`Task::new_user_with_as`] 的 try 版（3b3 H2）：16KiB 栈 + TCB 装箱任一
    /// OOM 返 `None`——EL0 经 `process_start` 触发时内核侧折 -ENOMEM（原
    /// `vec!`/`Box::new` 直接 abort = EL0 可触发 panic，违 ABI §8.0）。
    pub fn try_new_user_with_as(
        entry_va: usize,
        arg: usize,
        sp0_va: usize,
        name: &'static [u8],
        ttbr0: usize,
        asid: u16,
        proc_tag: usize,
    ) -> Option<Box<Task>> {
        let (kstack, stack_top) = Self::try_alloc_kstack()?;
        let ctx = TaskContext::new_user(entry_va, arg, sp0_va, stack_top);
        Self::try_assemble(ctx, kstack, name, ttbr0, asid, proc_tag)
    }

    /// 出 16KiB 内核栈 + 栈顶地址（栈向下长，顶 = 最高地址；页对齐 → 16 对齐）。
    /// try 化（3b3 H2）：原 `vec![0u8; 16KiB]` 分配 OOM 即 abort——EL0 经
    /// `process_start` 可触发；先 `try_reserve_exact` 再填充，失败返 `None`。
    fn try_alloc_kstack() -> Option<(Box<[u8]>, usize)> {
        let mut v: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
        v.try_reserve_exact(STACK_SIZE).ok()?;
        v.resize(STACK_SIZE, 0);
        let kstack: Box<[u8]> = v.into_boxed_slice();
        let top = kstack.as_ptr() as usize + STACK_SIZE;
        Some((kstack, top))
    }

    /// 装配 Task 主体（两种入口共用；表示法字段全默认值）。try 化（3b3 H2）。
    fn try_assemble(
        ctx: TaskContext,
        kstack: Box<[u8]>,
        name: &'static [u8],
        ttbr0: usize,
        asid: u16,
        proc_tag: usize,
    ) -> Option<Box<Task>> {
        try_box(Task {
            ctx,
            kstack: Some(kstack),
            name,
            state: TaskState::Ready,
            slice_left: 0,
            ttbr0,
            asid,
            proc_tag,
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
