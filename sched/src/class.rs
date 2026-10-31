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
        //
        // S-2 前置件 2（A11 per-CPU 化）：基准挪本核 SmpCpu.last_vct——双 tick
        // 结构性必须（两核共享全局时每核只对「自对方核上次 tick 以来的增量」
        // 扣减 = slice 约半速不确定燃烧，§3.4 红线）；本核只写本核块（tick
        // 锁窗内单写者）。槽未装（TPIDR_EL1==0，cpu0_init 前防御窗——本路径
        // 实际不可达）回落旧全局 static，null-safe 零新 panic 面。
        let now = hal::timer::cntvct();
        // SAFETY: IRQ 上下文、本核独占写（per-CPU 块 / 回落位单核窗）。
        unsafe {
            match hal::smp::last_vct_slot() {
                Some(p) => {
                    let last = p.read_volatile();
                    p.write_volatile(now);
                    t.slice_left = t.slice_left.saturating_sub(now.wrapping_sub(last));
                }
                None => {
                    let last = (&raw const LAST_VCT).read_volatile();
                    (&raw mut LAST_VCT).write_volatile(now);
                    t.slice_left = t.slice_left.saturating_sub(now.wrapping_sub(last));
                }
            }
        }
    }

    fn priority(&self) -> i32 {
        0
    }
}

// ---- S-2b：副核参与的 pick 过滤器（草案 §2.3）----
//
// E0 BKL 纪元「内核串行 / 用户并行」的出选过滤面：副核（CPU1）只允许
// `pinned != 0` 的任务（= svcmgr `process_start` → `set_task_pinned` 派生的
// EL0 用户进程）入选出选；内核任务与 boot 裸表进程全钉 CPU0（pinned=0），
// 结构性不越核。`Running-elsewhere` 排除：他核正在跑的任务（state==Running
// 且非本核当前）绝不被本核二派发——park 派发与 preempt 出选共用本谓词。
//
// CPU0 全量视图（user_only=false）：单核唯一 Running==my_cur，与旧
// `pick_next` 逐位对齐（无过滤漂移）。

/// pick 出选过滤器（S-2b）。`user_only` = 只出选 `pinned != 0`（副核）；
/// `my_cur` = 本核当前任务（Running-elsewhere 排除的自豁免指针，park 派发
/// 态传 null——park 上下文不是调度语义 Task，永不入环）。
pub struct PickFilter {
    pub user_only: bool,
    pub my_cur: *mut Task,
}

impl PickFilter {
    /// CPU0 全量视图（单核语义与旧 `pick_next` 恒等：唯一 Running 即 my_cur，
    /// 恒过第二臂；pinned 不设门）。
    pub fn all(my_cur: *mut Task) -> Self {
        PickFilter { user_only: false, my_cur }
    }

    /// 副核用户面视图（`pinned != 0` 才可见；park 派发态 my_cur=null）。
    pub fn user_only(my_cur: *mut Task) -> Self {
        PickFilter { user_only: true, my_cur }
    }

    /// 出选资格：①user_only 时排除 pinned==0；②恒排除他核 Running
    /// （state==Running 且指针 != my_cur——Ready 态才可出选；本核当前在环
    /// 且 eligible，pick 返自身 = 续跑不切）。
    pub(crate) fn eligible(&self, t: *const Task) -> bool {
        unsafe {
            if self.user_only && (*t).pinned == 0 {
                return false;
            }
            if (*t).state == TaskState::Running && (t as *mut Task) != self.my_cur {
                return false;
            }
            true
        }
    }
}

impl RoundRobin {
    /// 过滤版出选（S-2b inherent 方法——trait 冻结不动，`docs/SMP-S2实施设计草案.md`
    /// §2.3）。语义 = 旧 `pick_next`（两圈：最高 effective、同高取 head 侧等待
    /// 最久者）叠加 [`PickFilter::eligible`] 门。
    ///
    /// 实现为**单圈严格 > 替换**：首个 eligible 者起步，遇严格更高者替换——
    /// 与两圈「记最高再从头取第一个 == 最高」恒等（严格 > 保住首见位置 = head
    /// 侧优先），且正确处理 head 不 eligible 形态（两圈版 max 初值取自 head、
    /// 头部被滤时二圈起点也是 head——本实现直接以首个 eligible 为锚，无该
    /// 隐患）。选中后 `advance_past` + `policy::apply` 与旧版逐位同序。
    /// **消费版**出选（CPU0 专用）：选中即 `advance_past` 消耗共享游标
    /// （FIFO 公平轮转的推进器）+ `policy::apply`。
    pub fn pick_next_filtered(
        &self,
        rq: &Runqueue,
        _topo: &Topo,
        filter: &PickFilter,
    ) -> Option<&mut Task> {
        let chosen = self.scan_filtered(rq, filter)?;
        // SAFETY: scan_filtered 契约（KERN_LOCK 窗内、节点在环）。
        unsafe { rq.advance_past(chosen) };
        let t = unsafe { &mut *chosen.as_ptr() };
        crate::policy::apply(t);
        Some(t)
    }

    /// **只读版**出选（S-2b 修正：副核专用）——不消费共享游标。
    ///
    /// E0 单 RUNQUEUE 单游标下，副核 pick 若走消费版，`advance_past` 会把
    /// CPU0 的 RR FIFO 出选序周期性回卷到 pinned 任务身后——副核 user_only
    /// 视图唯一 eligible 时（裸相 = user_child）每 tick 必回卷一次 = 游标恒
    /// 定钉死在小段内摆动，远段任务结构性饿死（S-2b 首跑实锤：pinger 等
    /// ponger 通道 WRITABLE 30s 不醒、30 次出选机会全被回卷吃掉）。副核
    /// 「借走」任务由 `state=Running` + PickFilter 的他核 Running 排除臂
    /// 表达，无需消耗 head；CPU0 游标恢复单核 FIFO 语义。副核内多 pinned
    /// 任务的公平轮转归 S-3 per-CPU 队（E0 下 pinned 常驻 ≤1 无饿面）。
    pub fn peek_next_filtered(
        &self,
        rq: &Runqueue,
        _topo: &Topo,
        filter: &PickFilter,
    ) -> Option<&mut Task> {
        let chosen = self.scan_filtered(rq, filter)?;
        let t = unsafe { &mut *chosen.as_ptr() };
        crate::policy::apply(t);
        Some(t)
    }

    /// 单圈严格 > 替换扫描（两包装的公共体；选中者未出环、未动游标）。
    fn scan_filtered(
        &self,
        rq: &Runqueue,
        filter: &PickFilter,
    ) -> Option<NonNull<Task>> {
        // SAFETY: 环上任务均调度器拥有且在环；调用侧全在 KERN_LOCK 窗内
        // （SVC/tick/klock 包络）——扫描期间无并发出/入环（E0 BKL 执法面）。
        unsafe {
            let mut cur = rq.head()?;
            // 锚定首个 eligible 节点（环上节点必有 next——环形不变式）。
            while !filter.eligible(cur.as_ptr()) {
                cur = (*cur.as_ptr()).rq_next.get().expect("环上节点必有 next");
                if cur == rq.head().expect("head 已取过") {
                    return None; // 整圈无 eligible
                }
            }
            let mut best = cur;
            let mut best_eff = (*cur.as_ptr()).prio_effective;
            loop {
                cur = (*cur.as_ptr()).rq_next.get().expect("环上节点必有 next");
                if cur == rq.head().expect("head 已取过") {
                    break; // 整圈扫完
                }
                if !filter.eligible(cur.as_ptr()) {
                    continue;
                }
                let e = (*cur.as_ptr()).prio_effective;
                if e > best_eff {
                    best = cur;
                    best_eff = e;
                }
            }
            Some(best)
        }
    }
}
