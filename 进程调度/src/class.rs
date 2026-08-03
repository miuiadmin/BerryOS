//! 调度类 trait（**冻结**）+ RoundRobin 实现。

use crate::runqueue::{Runqueue, Topo};
use crate::task::{Task, TaskState};
use core::ptr::NonNull;

/// 调度策略接口（**冻结**——签名一字不改，见 `调度器设计.md` §3.2 / plan）。
///
/// 机制（runqueue / switch / timer tick）留内核（`sched` 模块）；策略做成可注册
/// trait（`class_rt` / `class_fair` / `class_deadline` / `class_idle`）。各方法须
/// **零分配**（IRQ 热路径）——任务在 `spawn` 预分配。
pub trait SchedClass {
    /// 把任务放入 runqueue（`spawn` / 唤醒时）。
    fn enqueue(&self, t: &mut Task, rq: &mut Runqueue);
    /// 把任务移出 runqueue（阻塞 / 退出时）。
    fn dequeue(&self, t: &mut Task, rq: &mut Runqueue);
    /// 选下一个要跑的任务（不改冻结合约：`&Runqueue` 下经内部 interior mutability
    /// 旋转游标，返回 `&mut Task` 由裸指针派生——plan Bug 5）。
    fn pick_next(&self, rq: &Runqueue, topo: &Topo) -> Option<&mut Task>;
    /// 每 timer tick 的记账（递减时间片等）。
    fn tick(&self, t: &mut Task, rq: &mut Runqueue);
    /// 该类的优先级（round-robin 不用）。
    fn priority(&self) -> i32;
}

/// round-robin 默认时间片（timer tick 数）。1 = 每 tick 切一次（最公平、便于压测）。
pub const RR_QUANTUM: u32 = 1;

/// 简单轮转调度类（G0/G1 默认）。侵入式环形 FIFO 游标 + 固定时间片。
///
/// pick 旋转游标、**不摘除**任务（runnable 任务常驻环；当前任务也在环里）——故
/// 协作 `yield_now` / 抢占 `tick` 只需调一次 `pick_next`，无需每次 enqueue/dequeue
/// 翻动。仅 `spawn`(enqueue) 与 `exit`(dequeue) 动环。
pub struct RoundRobin;

impl SchedClass for RoundRobin {
    fn enqueue(&self, t: &mut Task, rq: &mut Runqueue) {
        t.state = TaskState::Ready;
        // SAFETY: t 是调度器拥有、当前不在任何 runqueue 上的新任务。
        unsafe { rq.link_back(NonNull::from(&mut *t)) };
    }

    fn dequeue(&self, t: &mut Task, rq: &mut Runqueue) {
        // SAFETY: t 在本 runqueue 上（或已 Exited——unlink 幂等）。
        unsafe { rq.unlink(NonNull::from(&mut *t)) };
        t.state = TaskState::Exited;
    }

    fn pick_next(&self, rq: &Runqueue, _topo: &Topo) -> Option<&mut Task> {
        // SAFETY: rotate 返回的 NonNull 指向调度器拥有、仍在环上的任务；调度器不变式
        // 「一任务至多在一个 runqueue、至多被一个核 pick」（G0 单核平凡成立）保证
        // 返回的 &mut 不与别的活跃借用别名。
        let p = unsafe { rq.rotate()? };
        Some(unsafe { &mut *p.as_ptr() })
    }

    fn tick(&self, t: &mut Task, _rq: &mut Runqueue) {
        // 时间片递减（饱和到 0）；scheduler 读 `slice_left == 0` 决定重调度。
        t.slice_left = t.slice_left.saturating_sub(1);
    }

    fn priority(&self) -> i32 {
        0
    }
}
