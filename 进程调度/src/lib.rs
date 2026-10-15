//! berry-sched —— 调度器核心（架构无关 Rust + HAL 的 TaskContext/switch）。
//!
//! Phase 2a Step 2 落地的是**类型骨架**：`Task` / `Runqueue` / `Topo` /
//! `SchedClass`(trait) / `RoundRobin`。能编、最小运行时；Step 3 起接调度循环。
//!
//! 设计硬约束（plan + `调度器设计.md` §2/§3.2）：
//! - **机制进内核（本 crate），策略可插拔**：trait [`SchedClass`] 冻结（签名一字不改）。
//! - **runqueue 持裸指针**（`NonNull<Task>`），不拥有可变 `Task`——故
//!   `pick_next(&Runqueue)` 能返回 `Option<&mut Task>`（由指针派生，非由 `&Runqueue`
//!   借用——plan Bug 5）。
//! - **runqueue 链表侵入式 + interior mutability（`Cell`）**：`pick_next` 在 `&Runqueue`
//!   下仍能旋转游标（round-robin），无需改冻结合约为 `&mut Runqueue`。
//! - **IRQ 热路径零分配**：`enqueue`/`dequeue`/`pick_next`/`tick` 不 `Box`；任务在
//!   `spawn` 预分配。

#![no_std]

extern crate alloc;

pub mod class;
pub mod policy;
pub mod runqueue;
pub mod sched;
pub mod task;

pub use class::{RoundRobin, SchedClass};
pub use runqueue::{Runqueue, Topo};
pub use sched::{
    current_tag, exit_current, idle_loop, init, last_killed, on_irq_tick, set_switch_hook,
    spawn, spawn_as, spawn_user, switches, try_spawn_user, user_fault_exit, user_kills,
    yield_now, SwitchHook, SwitchKind,
};
pub use task::{EppHint, Task, TaskState, STACK_SIZE};
