//! 调度类 trait（**冻结**）+ Fair（1.1/S2：带内 EEVDF）实现。

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
///
/// S-2 前置件 2（A11）起这是**回落位**：本核 SmpCpu.last_vct（TPIDR_EL1
/// 寻址）才是主位——`cpu0_init` 前防御窗（RR.tick 实际不可达）走本 static。
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
/// 公平调度类（1.1/S2）：`prio_effective` 作为带序，带内 EEVDF。
///
/// 带内排序键 = `(eligible, vdeadline, vruntime)`；同键保持 head 侧先见
/// （游标推进 = 等权任务的 FIFO 轮转，既有 RR 矩阵零扰动）。`lag` 的工程
/// 近似 = `vruntime <= avg_vruntime`；带内无 eligible 时退化为最小
/// `(vdeadline, vruntime)`，不空转。
pub struct Fair;

/// 兼容别名：既有调用点仍以 `RoundRobin` 指类型（1.1/S2 语义已是 Fair）。
pub type RoundRobin = Fair;

/// 虚拟时间刻度（权重基准）。
const SCALE: u64 = 1024;

#[inline]
fn weight_of(t: &Task) -> u64 {
    // 基线 1024；PI/策略 boost 每级 +64，封顶 1<<20（防 u64 溢出）。
    let boost = t.prio_effective.max(0) as u64;
    (SCALE + boost.saturating_mul(64)).min(1 << 20)
}

#[inline]
fn slice_virtual(t: &Task) -> u64 {
    let s = hal::timer::slice_cycles() as u128 * SCALE as u128 / weight_of(t) as u128;
    (s as u64).max(1)
}

/// 环上平均虚拟时间（EEVDF 的 V 的工程近似；O(n)、零分配）。
fn avg_vruntime(rq: &Runqueue) -> Option<u64> {
    let h = rq.head()?;
    // SAFETY: 环上任务均调度器拥有；调用方在 IRQ 屏蔽/调度窗内（同 pick）。
    unsafe {
        let mut cur = h;
        let mut sum: u128 = 0;
        let mut n: u64 = 0;
        loop {
            sum += (*cur.as_ptr()).vruntime as u128;
            n += 1;
            cur = (*cur.as_ptr()).rq_next.get().expect("环上节点必有 next");
            if cur == h {
                break;
            }
        }
        Some((sum / n as u128) as u64)
    }
}

impl SchedClass for Fair {
    fn enqueue(&self, t: &mut Task, rq: &mut Runqueue) {
        t.state = TaskState::Ready;
        // 新/醒任务不得带着睡眠期攒下的 lag 回场：抬到当前平均虚拟时间。
        let avg = avg_vruntime(rq).unwrap_or(0);
        if t.vruntime < avg {
            t.vruntime = avg;
        }
        t.vdeadline = t.vruntime.saturating_add(slice_virtual(t));
        // SAFETY: t 是调度器拥有、当前不在任何 runqueue 上的新任务。
        unsafe { rq.link_back(NonNull::from(&mut *t)) };
        // [S-3a 义务①除名] enqueue 尾跨核 SGI 戳被实验证伪除名，降级 S-3b
        // per-CPU 队列路由化回归——判词全文 → 子案 §9.5「义务①除名判词」。
    }

    fn dequeue(&self, t: &mut Task, rq: &mut Runqueue) {
        // SAFETY: t 在本 runqueue 上（或已 Exited——unlink 幂等）。
        unsafe { rq.unlink(NonNull::from(&mut *t)) };
        t.state = TaskState::Exited;
    }

    fn pick_next(&self, rq: &Runqueue, _topo: &Topo) -> Option<&mut Task> {
        let chosen = self.scan_filtered(rq, core::ptr::null_mut())?;
        // SAFETY: scan_filtered 契约（环上节点在环、调用侧屏蔽窗）。
        unsafe { rq.advance_past(chosen) };
        let t = unsafe { &mut *chosen.as_ptr() };
        // AI 策略注入留桩（3-4c）：出选后查表——未绑/空表零写入，与接口
        // 缺席逐位等价；4-3a 起注入的权重真消费（写 prio_effective，带内
        // EEVDF 的 weight_of 下一轮读它）。
        crate::policy::apply(t);
        Some(t)
    }

    fn tick(&self, t: &mut Task, _rq: &mut Runqueue) {
        // 周期数记账（2b-4）：按 CNTVCT 增量递减时间片；同时按权重累加
        // 虚拟运行时间（EEVDF 记账）。片耗尽时重算虚拟截止期。
        let now = hal::timer::cntvct();
        // SAFETY: IRQ 上下文、本核独占写（per-CPU 块 / 回落位单核窗）。
        let delta = unsafe {
            match hal::smp::last_vct_slot() {
                Some(p) => {
                    let last = p.read_volatile();
                    p.write_volatile(now);
                    if last == 0 { 0 } else { now.wrapping_sub(last) }
                }
                None => {
                    let last = (&raw const LAST_VCT).read_volatile();
                    (&raw mut LAST_VCT).write_volatile(now);
                    if last == 0 { 0 } else { now.wrapping_sub(last) }
                }
            }
        };
        t.slice_left = t.slice_left.saturating_sub(delta);
        let vdelta = (delta as u128 * SCALE as u128 / weight_of(t) as u128) as u64;
        t.vruntime = t.vruntime.saturating_add(vdelta);
        if t.slice_left == 0 {
            t.vdeadline = t.vruntime.saturating_add(slice_virtual(t));
        }
    }

    fn priority(&self) -> i32 {
        0
    }
}

// ---- pick 出选排除参数（S-2b 起源；S-3b 步 b-1 退役改形）----
//
// 原双臂过滤器（user_only + my_cur）的 user_only 臂由**放置路由纯度替代**
// （子案 §9.6/F8）：per-CPU 队列 + `place()` ①使内核任务结构性只入 CPU0 队
// ——CPU1 队列恒无内核任务，副核无需按 pinned 过滤。保留的排除语义 =
// Running-elsewhere 臂：pick 选中后任务仍留环（FIFO 环只推进游标，直到
// block/exit 才离环），环上恰有「正在跑」的本核 cur——扫描须跳过（my_cur
// 自豁免指针；park 派发态传 null：park 上下文非调度语义 Task，park 态环上
// 无 Running 残留，null = 全跳 Running 行为等价）。

/// 出选资格（S-3b 单臂形）：非 Running，或 Running 但正是 `my_cur`
/// （pick 返自身 = 续跑不切）。
#[inline]
pub(crate) fn eligible(my_cur: *mut Task, t: *const Task) -> bool {
    unsafe { !((*t).state == TaskState::Running && (t as *mut Task) != my_cur) }
}

impl Fair {
    /// 过滤版出选（S-2b inherent 方法——trait 冻结不动）：
    /// 先取环上最高 `prio_effective` 带，再在带内按 EEVDF 选。
    ///
    /// 带内：优先 `vruntime <= avg`（eligible）者，键 `(vdeadline, vruntime)`；
    /// 同键保持 head 侧先见（`advance_past` 推进游标 = 等权 FIFO 轮转）。
    /// 带内无 eligible 时取最小键（不空转）。
    pub fn pick_next_filtered(
        &self,
        rq: &Runqueue,
        _topo: &Topo,
        my_cur: *mut Task,
    ) -> Option<&mut Task> {
        let chosen = self.scan_filtered(rq, my_cur)?;
        // SAFETY: scan_filtered 契约（KERN_LOCK 窗内、节点在环）。
        unsafe { rq.advance_past(chosen) };
        let t = unsafe { &mut *chosen.as_ptr() };
        crate::policy::apply(t);
        Some(t)
    }

    /// 单圈两段扫描：先定最高带，再带内 EEVDF（零分配）。
    fn scan_filtered(&self, rq: &Runqueue, my_cur: *mut Task) -> Option<NonNull<Task>> {
        let h = rq.head()?;
        let avg = avg_vruntime(rq).unwrap_or(0);
        // SAFETY: 环上任务均调度器拥有且在环；调用侧全在 KERN_LOCK 窗内
        // （SVC/tick/klock 包络）——扫描期间无并发出/入环（E0 BKL 执法面）。
        unsafe {
            // 段 1：最高 prio_effective 带（严格带序——PI 仍即时生效）。
            let mut max_eff = i32::MIN;
            let mut cur = h;
            loop {
                if eligible(my_cur, cur.as_ptr()) {
                    let e = (*cur.as_ptr()).prio_effective;
                    if e > max_eff {
                        max_eff = e;
                    }
                }
                cur = (*cur.as_ptr()).rq_next.get().expect("环上节点必有 next");
                if cur == h {
                    break;
                }
            }
            if max_eff == i32::MIN {
                return None;
            }
            // 段 2：带内 EEVDF（eligible 优先；同键首见保序）。
            let mut best: Option<NonNull<Task>> = None;
            let mut best_elig = false;
            let mut best_key = (u64::MAX, u64::MAX);
            let mut cur = h;
            loop {
                if eligible(my_cur, cur.as_ptr())
                    && (*cur.as_ptr()).prio_effective == max_eff
                {
                    let t = &*cur.as_ptr();
                    let elig = t.vruntime <= avg;
                    let key = (t.vdeadline, t.vruntime);
                    let better = match best {
                        None => true,
                        Some(_) => {
                            if elig != best_elig {
                                elig
                            } else {
                                key < best_key
                            }
                        }
                    };
                    if better {
                        best = Some(cur);
                        best_elig = elig;
                        best_key = key;
                    }
                }
                cur = (*cur.as_ptr()).rq_next.get().expect("环上节点必有 next");
                if cur == h {
                    break;
                }
            }
            best
        }
    }
}
