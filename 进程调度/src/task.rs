//! 任务（内核线程）对象 + 状态。

use alloc::boxed::Box;
use alloc::vec;
use bos_hal_aarch64::task::TaskContext;
use core::cell::Cell;
use core::ptr::NonNull;

/// 内核线程的默认栈大小（16KiB）。v1 切换在 IRQ-mask 中（无嵌套 IRQ）、无深递归，
/// 够用；守卫页 TODO（plan R7）。
pub const STACK_SIZE: usize = 16 * 1024;

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
pub struct Task {
    /// 协作者切换上下文（HAL 的 callee-saved 子集）。
    pub ctx: TaskContext,
    /// 该任务私有内核栈（栈顶 16 对齐，由分配器页对齐保证）。
    pub kstack: Box<[u8]>,
    /// 调试名（串口打印）。
    pub name: &'static [u8],
    pub state: TaskState,
    /// 剩余时间片（timer tick 计数）；0 表示该被重调度。Step 6 起由 `tick` 递减。
    pub slice_left: u32,

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
        let kstack: Box<[u8]> = vec![0u8; STACK_SIZE].into_boxed_slice();
        // 栈向下长，初始 sp = 栈顶（最高地址）。>2KiB 走分配器页路径，页对齐 →
        // 栈顶天然 16 对齐（TaskContext::new_kernel 内仍有断言兜底）。
        let stack_top = kstack.as_ptr() as usize + STACK_SIZE;
        let ctx = TaskContext::new_kernel(entry, arg, stack_top);
        Box::new(Task {
            ctx,
            kstack,
            name,
            state: TaskState::Ready,
            slice_left: 0,
            rq_prev: Cell::new(None),
            rq_next: Cell::new(None),
        })
    }
}
