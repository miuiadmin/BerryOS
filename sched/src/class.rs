//! 调度类 trait（**冻结**）+ RoundRobin 实现。

use crate::runqueue::{Runqueue, Topo};
use crate::task::{Task, TaskState};
use berry_hal_aarch64 as hal;
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

/// 上次 tick 的 CNTVCT 快照（周期数记账基准）。G0 单核静态例外——与 `sched.rs`
/// 的计数器同款纪律：仅 IRQ 上下文顺序访问。
static mut LAST_VCT: u64 = 0;

/// 简单轮转调度类（G0/G1 默认）。侵入式环形 FIFO 游标 + 固定时间片。
///
/// 时间片授予量自 2b-4 起不再用本 crate 常量（原 `RR_QUANTUM` tick 数退役）：
/// grant = `hal::timer::slice_cycles()`（= FREQ/SLICE_HZ，随探测频率换算——
/// DVFS 改频后重新换算即可，时间片实际时长不随频率漂移；LiteOS-M
/// `OsSchedTimeConvertFreq` 式）。
///
/// **4-3a 起 pick 按 `prio_effective` 出选**（`调度器设计.md` §3.6「调度决策
/// 一律看 effective」的 v1 兑现——PI 捐赠与 AI 权重微调的共同消费点）：两圈
/// 扫描（一圈记最高、一圈从 head 起取第一个 == 最高者），选中后游标前移到
/// 其后继——**同 effective 层内轮转、跨层严格优先**。全员同 effective
/// （v1 常态：prio 全 0）时与纯 rotate 逐轮等价，既有矩阵零扰动。
///
/// pick 扫描、**不摘除**任务（runnable 任务常驻环；当前任务也在环里）——故
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
        // SAFETY: 环上任务均调度器拥有且在环（rotate 同款不变式）；单核无并发
        // pick（IRQ 屏蔽切换位互斥）——扫描期间无并发出/入环。
        let chosen = unsafe {
            let h = rq.head()?;
            // 一圈记最高 effective（环上节点必有 next——环形不变式）。
            let mut max = (*h.as_ptr()).prio_effective;
            let mut cur = (*h.as_ptr()).rq_next.get().expect("环上节点必有 next");
            while cur != h {
                let e = (*cur.as_ptr()).prio_effective;
                if e > max {
                    max = e;
                }
                cur = (*cur.as_ptr()).rq_next.get().expect("环上节点必有 next");
            }
            // 二圈从 head 起取第一个 == 最高者（同高 FIFO——head 侧即等待最久者，
            // 层内轮转公平保持）。max 必在环上（来自第一圈），循环至多一圈终止。
            let mut cur = h;
            while (*cur.as_ptr()).prio_effective != max {
                cur = (*cur.as_ptr()).rq_next.get().expect("环上节点必有 next");
            }
            rq.advance_past(cur);
            cur
        };
        let t = unsafe { &mut *chosen.as_ptr() };
        // AI 策略注入留桩（3-4c，`AI调度设计.md` §2/§9）：出选后查表——未绑/空表
        // 零写入，与接口缺席逐位等价（ktest POLICY 段断言）。4-3a 起注入的权重
        // 真消费（pick 看 effective——写进 TCB 即影响下一轮出选）。
        crate::policy::apply(t);
        Some(t)
    }

    fn tick(&self, t: &mut Task, _rq: &mut Runqueue) {
        // 周期数记账（2b-4）：按 CNTVCT 增量递减时间片（饱和到 0）；scheduler 读
        // `slice_left == 0` 决定重调度。基准 0 起步 → 首次 tick 增量巨大 → 必切
        // （与旧「每 tick 减 1、quantum=1」行为等价：仍一次 slice 一换）。
        let now = hal::timer::cntvct();
        // SAFETY: IRQ 上下文、单核无并发（IRQ 入口硬件屏蔽，不嵌套）。
        let last = unsafe { LAST_VCT };
        unsafe { LAST_VCT = now };
        t.slice_left = t.slice_left.saturating_sub(now.wrapping_sub(last));
    }

    fn priority(&self) -> i32 {
        0
    }
}
