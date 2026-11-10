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

pub mod band;
pub mod class;
pub mod em;
pub mod hdr;
pub mod intent;
pub mod policy;
pub mod runqueue;
pub mod sched;
pub mod spin;
pub mod task;
pub mod util;

pub use band::{
    band_on, configure, consume, period_check, state, throttle_all, unthrottle_all, BandBudget,
    BAND_SLOTS,
};
pub use class::{Fair, RoundRobin, SchedClass, PRIO_DEADLINE};
pub use em::{
    em_domain, em_max_capacity, em_min_power, em_register, em_register_qemu_virt, em_valid,
    EmDomain, EmPoint, EM_DOMAINS_MAX, EM_POINTS_MAX,
};
pub use hdr::{
    bucket_of, bucket_upper, cpu_hdr, cpu_reset, merged_max, merged_min, merged_percentile,
    merged_total, record_switch_lat, record_wake_lat, CpuHdr, Hdr, LatKind, HDR_BUCKETS,
    HDR_MAX_VALUE, HDR_NR_CPU, HDR_OCTAVES, HDR_OVERFLOW, HDR_SUB, HDR_SUB_BITS,
};
pub use intent::{
    intent_valid, settle, snapshot, MsgIntent, SettleOut, SnapOut, INTENT_BACKGROUND,
    INTENT_CHAIN_BUDGET_NS, INTENT_HOP_BUDGET_NS, INTENT_HOP_MAX, INTENT_INTERACTIVE,
    INTENT_LATENCY_CRITICAL, INTENT_MAINTENANCE, INTENT_UNSPECIFIED, INTENT_UTILITY,
};
pub use runqueue::{linkable, Runqueue, Topo};
pub use util::{
    any_overutilized, cpu_util, overutilized, util_accumulate, util_reset, util_tick_window,
    util_window_vct, EWMA_ALPHA_SHIFT, OVERUTIL_THRESHOLD, UTIL_NR_CPU, UTIL_WINDOW_HZ,
    UTIL_WINDOW_VCT_REF,
};
pub use sched::{
    block_current, current_pinned, current_tag, exit_current,
    exit_current_with, fast_wake_locked,
    find_single_waiter, find_single_waiter_locked, for_each_sleeper, freeze_by_pinned,
    freeze_by_thread, freeze_precheck_by_pinned, freeze_precheck_by_thread,
    current_thread_key, current_deadline_state, idle_loop, init,
    intent_settle_current,
    intent_settle_current_ext,
    intent_snapshot_current, kill_all_tasks_by_pinned, last_killed,
    lb_stats,
    on_irq_tick, on_sgi_resched, park_recheck, preempt_checkpoint, preempt_checkpoint_hits, preempt_count, preempt_disable,
    preempt_enable, print_window_enter, print_window_exit,
    nearest_deadline, refresh_effective, reap_drain,
    set_exit_hook, set_lb_match,
    set_klock_ops, set_sleepq_hook, sq_lock, sq_unlock,
    set_switch_hook, set_wake_hook, sleepq_insert,
    sleepq_insertable, sleepq_unlink, spawn, spawn_as, spawn_as_spread, spawn_intent, spawn_user, spawn_with,
    spawn_with_tag, switches, switches_tt, svc_window, thaw_by_pinned, thaw_by_thread, topo_smp_online,
    try_preempt, try_prepare_user, enqueue_prepared_user, try_spawn_user,
    user_fault_exit,
    user_kills,
    wake_matching, wake_matching_locked, wake_stamp_take, yield_now, yield_now_keep_irq, KlockOps, SleepqHook, SwitchHook, SwitchKind,
    WakeHook, WAIT_ANY,
};
pub use task::{EppHint, Task, TaskState, WakeReason, HELD_FROZEN, HELD_THROTTLE, STACK_SIZE};
