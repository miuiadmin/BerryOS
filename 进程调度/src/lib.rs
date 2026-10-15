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
pub mod intent;
pub mod policy;
pub mod runqueue;
pub mod sched;
pub mod task;

pub use class::{RoundRobin, SchedClass};
pub use intent::{
    intent_valid, settle, snapshot, MsgIntent, SettleOut, SnapOut, INTENT_BACKGROUND,
    INTENT_CHAIN_BUDGET_NS, INTENT_HOP_BUDGET_NS, INTENT_HOP_MAX, INTENT_INTERACTIVE,
    INTENT_LATENCY_CRITICAL, INTENT_MAINTENANCE, INTENT_UNSPECIFIED, INTENT_UTILITY,
};
pub use runqueue::{Runqueue, Topo};
pub use sched::{
    block_current, current_tag, exit_current, for_each_sleeper, idle_loop, init,
    intent_settle_current, intent_snapshot_current, last_killed, on_irq_tick, refresh_effective,
    set_sleepq_hook, set_switch_hook, set_wake_hook, sleepq_insert, sleepq_unlink, spawn,
    spawn_as, spawn_intent, spawn_user, spawn_with, switches, try_spawn_user, user_fault_exit,
    user_kills, wake_matching, yield_now, SleepqHook, SwitchHook, SwitchKind, WakeHook, WAIT_ANY,
};
pub use task::{EppHint, Task, TaskState, WakeReason, STACK_SIZE};
