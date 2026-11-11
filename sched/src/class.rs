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
/// 一律看 effective」的 v1 兑现——PI 捐赠与 AI 权重微调的共同消费点）：单圈
/// 扫描同时追踪最高带与带内最优（S2 合并形——旧两圈：一圈记最高、一圈取
/// 第一个 == 最高者），选中后游标前移到其后继——**同 effective 层内轮转、
/// 跨层严格优先**。全员同 effective（v1 常态：prio 全 0）时与纯 rotate
/// 逐轮等价，既有矩阵零扰动。
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

/// **S2b 纯 EEVDF 开关**（实验轨道）：
/// - `false`（默认，S2a）：严格带序 + 带内 EEVDF——PI boost 即时生效
///   （高带先出），deadline 带（≥1000）恒先于 fair。v1 安全形态。
/// - `true`（S2b）：去带序——所有任务纯 `(eligible, vdeadline, vruntime)`
///   竞争。PI boost 经 weight 影响 VD（软优先——高权→短 VD→常先但不保证）。
///   deadline 带语义失效（绝对截止期 VD 与 fair VD 尺度不可比——需 v2
///   归一化）。**仅实验用——v1 测试全按 S2a 假设构建**。
pub const PURE_EEVDF: bool = false;

/// 兼容别名：既有调用点仍以 `RoundRobin` 指类型（1.1/S2 语义已是 Fair）。
pub type RoundRobin = Fair;

// W3-Q11：权重/虚拟片/片预算数学单源在 berry-sliceface（host 可测纯面
// ——本 crate 只 aarch64 可编，预算语义回归面此前 host 不可达）；此处
// 消费即唯一事实源（wqmatch/iprofile 同款纪律）。
use berry_sliceface::{
    fair_rearm_vd, period_due, replenish as replenish_budget, slice_virtual as slice_virtual_of,
    tick_account, throttle_admissible, DeadlineState,
};
// W3-Q11：deadline 带优先级常量单源 berry-sliceface——本模块 re-export
// （lib.rs `pub use class::PRIO_DEADLINE` 消费链不变，band.rs 同）。
pub use berry_sliceface::PRIO_DEADLINE;

/// 判定：是否 deadline 任务（带序 ≥ PRIO_DEADLINE）。S5-c 计数门起
/// `runqueue` 原语层（`DL_ON_RING` 账点）同判据消费——单一事实源。
#[inline]
pub(crate) fn is_deadline(t: &Task) -> bool {
    t.prio_effective >= PRIO_DEADLINE
}

#[inline]
fn slice_virtual(t: &Task) -> u64 {
    // W3-Q11：虚拟片换算单源 berry-sliceface（grant×SCALE/weight 下限 1，
    // host 钉板 slice_virtual_law）。
    slice_virtual_of(hal::timer::slice_cycles(), t.prio_effective)
}

/// 环上平均虚拟时间（EEVDF 的 V 的工程近似；**O(1)**——S2b+ 从全环扫描改
/// `Runqueue` 增量账本直除：`sum_vruntime / len`，pick_next 热路径少一圈
/// O(n)。账点 = link_back/unlink（结构原语层，steal 直调 unlink 也入账）+
/// 本类 `tick` 随 vdelta 入账（在环门控）；一致性执法 = `ring_check` 走查
/// 对账。空环 None 语义与旧扫描 `head()?` 同义（len==0 ⟺ head==None）；
/// 商与旧 u128 累加路径逐位一致（账本不溢出前提——界析见
/// `Runqueue::sum_vruntime` 字段注）。
fn avg_vruntime(rq: &Runqueue) -> Option<u64> {
    let n = rq.len();
    if n == 0 {
        return None;
    }
    Some(rq.vruntime_sum() / n as u64)
}

/// S5-c lazy unthrottle：扫环上被冻结的 deadline 任务——`cntvct() ≥ vdeadline`
/// 者周期到：解冻（清 HELD_FROZEN）+ 重臂 vdeadline（+= duration_hint_ns =
/// period_cycles）+ 授新预算 slice（preempt_threshold）。CBS 同形语义：
/// 每期至多 budget CPU 时长；超了等下期。
///
/// S5-c 计数门：头部先查 [`crate::runqueue::dl_on_ring`]（全域在环
/// deadline 任务账本）——0 时整圈扫描跳过（一次 Relaxed 原子读替代
/// O(n) 环走查）。v1 常态全员 fair，门常闭。门语义**单边安全**：账本
/// 只可能偏多不可能偏少（多计来源与执法面见 `DL_ON_RING` 字段注——
/// `integrity_check` 金丝雀单边对账），0 ⟹ 全域环上无 link 时即 DL 的
/// 任务 ⟹ 扫描必然空手——跳过零语义差。
///
/// # Safety
/// rq 环节点有效；调用侧屏蔽窗（pick_next 语境）。
unsafe fn lazy_unthrottle_deadlines(rq: &Runqueue) {
    // 快路径：全域环上无 deadline 任务（含本环）——零扫描。
    if crate::runqueue::dl_on_ring() == 0 {
        return;
    }
    let now = hal::timer::cntvct();
    let head = rq.head();
    let mut cur = head;
    while let Some(c) = cur {
        let t = c.as_ptr();
        cur = (*t).rq_next.get();
        // 快路径：非 deadline 或未冻结 → 一比较跳过。
        if (*t).prio_effective >= PRIO_DEADLINE
            && (*t).held_locks & crate::task::HELD_THROTTLE != 0
            && (*t).duration_hint_ns > 0
            && now >= (*t).vdeadline
        {
            // 慢路径：周期到 → 解限流（只清 THROTTLE 位——FROZEN 位是进程
            // 冻结独占语义，由 SuspendToken close 解除，不受预算周期影响）。
            (*t).held_locks &= !crate::task::HELD_THROTTLE;
            // W3-Q11：周期到判 + 重臂 + replenish 数学单源 berry-sliceface
            // （period_due/replenish——host 钉板 deadline_period_cycle）。
            dl_refill(&mut *t, now);
        }
        if cur == head { break; } // 环回起点 → 一圈完成
    }
}

/// W3-Q11：lazy_unthrottle 慢路径的「周期到 → 重臂 + replenish」步
/// 单源 berry-sliceface（`period_due` + `replenish`——host 钉板
/// deadline_period_cycle）；本函数只留环走查 + THROTTLE 位清的机械。
/// 安全引用形——裸解引用留在 unsafe fn 调用语境（unsafe 台账零新增）。
#[inline]
fn dl_refill(t: &mut Task, now: u64) {
    let st = DeadlineState {
        vdeadline: t.vdeadline,
        slice_left: t.slice_left,
        period: t.duration_hint_ns,
        budget: t.preempt_threshold,
    };
    if !period_due(now, &st) {
        return;
    }
    let (vd, sl) = replenish_budget(now, &st);
    t.vdeadline = vd;
    t.slice_left = sl;
}

impl SchedClass for Fair {
    fn enqueue(&self, t: &mut Task, rq: &mut Runqueue) {
        t.state = TaskState::Ready;
        // 新/醒任务不得带着睡眠期攒下的 lag 回场：抬到当前平均虚拟时间。
        let avg = avg_vruntime(rq).unwrap_or(0);
        if t.vruntime < avg {
            t.vruntime = avg;
        }
        // 1.1/S5：deadline 任务不覆盖 vdeadline（保留手动设的绝对截止期）。
        if !is_deadline(t) {
            t.vdeadline = t.vruntime.saturating_add(slice_virtual(t));
        }
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
        // S5-c lazy unthrottle：周期到的冻结 deadline 任务——解冻 + 重臂
        // vdeadline（+= period，存 duration_hint_ns）+ 授新预算 slice。
        // 零额外定时器：pick_next 高频调用的顺带扫（is_deadline 一比较快
        // 路过；冻结位零任务直接跳过）。S5-c 计数门起环上无 deadline 任务
        // （全域账本 0）时函数头即返——整圈扫描跳过。
        unsafe { lazy_unthrottle_deadlines(rq) };
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

    fn tick(&self, t: &mut Task, rq: &mut Runqueue) {
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
        // W3-Q11：tick 记账一步单源 berry-sliceface（饱和递减 + vdelta
        // 缩权累加，host 钉板 tick_account_law）。
        let (slice, vrt, vdelta) = tick_account(
            t.slice_left,
            t.vruntime,
            delta,
            t.prio_effective,
        );
        t.slice_left = slice;
        t.vruntime = vrt;
        // Σvruntime 账本随迁（S2b+ 增量维护，rq 参数自此真消费）：在环
        // 任务（cur 常驻环——pick 只推游标不摘）的 vruntime 前进 = 账本
        // 同额前进；离环者不入账——IDLE_TASK 永不在环（idle tick 不入账）
        // 与 ktest 手搓离环任务（UTIL-9）都不污染账本，与旧全环扫描口径
        // （从不含环外任务）一致。
        if t.rq_prev.get().is_some() {
            rq.vruntime_add(vdelta);
        }
        // v1.3 §7（频率归 PM + util 信号）：本核运行 cycles 进 per-CPU
        // utilization EWMA（`util.rs`——调度器给 PM 的频率 hint，schedutil
        // 同构 f ≈ util×fmax/capacity，频率选择权全归 PM）。**非 idle 门**：
        // IDLE_TASK 上的 tick 不累计（idle 时间是低 util 信号本身，非运行
        // 时间）；窗口翻转两态都查（idle tick 翻窗 → 空窗折叠低 util，
        // EWMA 随空闲衰减）。Relaxed 原子零锁——统计信号无序角色，IRQ 热
        // 路径零额外屏障。
        let cpu = hal::smp::cpu_id();
        if !crate::sched::is_idle(t) {
            crate::util::util_accumulate(cpu, delta);
        }
        crate::util::util_tick_window(cpu, now);
        // W3-Q11：fair 片耗尽 VD 重臂资格单源（非 deadline 才重臂——
        // deadline 手动绝对截止期不碰，1.1/S5）。
        if t.slice_left == 0 && fair_rearm_vd(t.prio_effective, is_deadline(t)) {
            t.vdeadline = t.vruntime.saturating_add(slice_virtual(t));
        }
        // S5-c 预算执法：deadline 任务的 slice = 预算尺寸（spawn 时由
        // preempt_threshold 设定）。slice 耗尽 = 本期预算用完 → throttle
        //（HELD_FROZEN 置位 → eligible 过滤 → 不再出选）。周期到 →
        // lazy_unthrottle（pick_next 面扫）解冻 + 重臂 + 新预算。
        // W3-Q11：准入三与门单源 berry-sliceface（host 钉板 throttle_gate）。
        if throttle_admissible(
            t.prio_effective,
            t.preempt_threshold,
            t.slice_left,
            t.held_locks & crate::task::HELD_THROTTLE != 0,
        ) {
            t.held_locks |= crate::task::HELD_THROTTLE;
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

/// 出选资格（S-3b 单臂形 + 1.1/S3 冻结位）：非 Running（或 Running 但正是
/// `my_cur`），且未被冻结。冻结任务留在环上但永不出选——thaw 后恢复。
/// W3-Q12 起判定律单源 `berry_ringface::eligible`（host 钉板——逐位/双臂
/// 独立 AND/自豁免面全测）；此处纯投影 + 转发，零第二实现。
#[inline]
pub(crate) fn eligible(my_cur: *mut Task, t: *const Task) -> bool {
    berry_ringface::eligible(&ring_desc(unsafe { &*t }), t as *mut Task == my_cur)
}

/// W3-Q12：`&Task` → `berry_ringface::TaskDesc` 纯值域投影（环序语义消费
/// 的唯一投影点——state as u8 序与 ringface 常量同钉，见
/// ringface tests::state_projection_pinned）。
#[inline]
fn ring_desc(t: &Task) -> berry_ringface::TaskDesc {
    berry_ringface::TaskDesc {
        state: t.state as u8,
        held_locks: t.held_locks,
        prio_effective: t.prio_effective,
        vruntime: t.vruntime,
        vdeadline: t.vdeadline,
    }
}

impl Fair {
    /// yield 片归还（moss `yield_slice` 语义的 BerryOS 形态）——让出者不得带
    /// 未耗虚拟优势回场，提案 2026-11-08，SMP 前单核窗内安全；双任务真让出
    /// 补刀（同日）：从「抬到 avg」强化为「抬到**环上 vruntime max**」。
    ///
    /// 语义 = 协作让出者本轮垫底（POSIX `sched_yield` 对齐——EEVDF 形态的
    /// 排队尾）。抬均形态在双任务环有数学零操作区：让出者 vruntime < max
    /// 但 ≥ avg 时抬均后仍最 eligible（avg < max 恒留该区）——yield 空操作；
    /// 抬 max 封死之。让出者抬后仍在环（tick 继续累加其 vruntime——防饥饿
    /// 兜底已有）；锁退避场景方向正确（退避者让路）。
    ///
    /// **只动 vruntime**：冻结（HELD_FROZEN）与 deadline 任务路径不受影响
    /// （S5 手动 vdeadline 不碰）。调用契约同 pick——RQ 锁/屏蔽窗内（让出
    /// 者仍在本环上，环 max 含其自身——已是 max 时零操作）。
    pub fn yield_slice_return(&self, t: &mut Task, rq: &Runqueue) {
        // 扫环取 vruntime max（avg_vruntime 旧全环扫描同款形态；O(n) 可接受
        // ——yield 冷路径）。空环零操作（防御面：调用契约下让出者在环，
        // 不可达）。
        let Some(h) = rq.head() else { return };
        // SAFETY: 环上任务均调度器拥有且在环；调用侧 RQ 锁/屏蔽窗内
        // （yield_now_impl 语境）——扫描期间无并发出/入环（E0 BKL 执法面）。
        // 走查同款不变式（§A136 形状：先访当前节点、后判回 head）。
        let max = unsafe {
            let mut m = 0u64;
            let mut cur = h;
            loop {
                let p = cur.as_ptr();
                if (*p).vruntime > m {
                    m = (*p).vruntime;
                }
                cur = (*p).rq_next.get().expect("环上节点必有 next");
                if cur == h {
                    break;
                }
            }
            m
        };
        if t.vruntime < max {
            // Σvruntime 账本同步（S2b+ 增量账本调和，2026-11-08）：抬值改
            // vruntime 必须经账点，否则 ring_check 百切换对账金丝雀漂移
            // panic。
            rq.vruntime_add(max - t.vruntime);
            t.vruntime = max;
        }
    }

    /// 过滤版出选（S-2b inherent 方法——trait 冻结不动）：
    /// 单圈扫描同时定最高 `prio_effective` 带与带内 EEVDF 最优（scan_filtered）。
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

    /// 单圈合并扫描：同时追踪最高带与带内 EEVDF 最优（零分配）。
    ///
    /// 合并前形态 = 两圈（段 1 全环定最高 `prio_effective` 带、段 2 带内
    /// EEVDF）；现单圈三分支等价合并——任一扫描前缀上不变式：`best_eff` =
    /// 前缀 eligible 的最高带，`best` = 该带内 EEVDF 最优（同键首见保序）。
    /// 逐元素三分支：
    /// - 更高带（`e > best_eff`）→ 重置 best（旧段 1 max_eff 更新 + 段 2
    ///   最优清空的合并点）；
    /// - 同带（`e == best_eff`）→ 旧段 2 原样 EEVDF 比较（eligible 优先、
    ///   键 `(vdeadline, vruntime)`、严格 `<` 保首见）；
    /// - 低带 → 跳过（无出选资格）。
    ///
    /// 初值 `best_eff = i32::MIN` + `best = None`：首个 eligible 必落「更高
    /// 带重置」（e > i32::MIN）或退化 i32::MIN 带「同带首见」（best=None
    /// 兜底选中）；环上无 eligible 时 best 保持 None 返 None（旧段 1
    /// `max_eff == i32::MIN` 早退同义）。
    ///
    /// S2b PURE 开关：`!PURE_EEVDF &&` 门使带比较恒假 → 所有任务同带，
    /// 只剩 EEVDF 分支（旧 `in_band` 恒 true 同义；const 折叠零运行时开销）。
    fn scan_filtered(&self, rq: &Runqueue, my_cur: *mut Task) -> Option<NonNull<Task>> {
        let h = rq.head()?;
        let avg = avg_vruntime(rq).unwrap_or(0);
        // SAFETY: 环上任务均调度器拥有且在环；调用侧全在 KERN_LOCK 窗内
        // （SVC/tick/klock 包络）——扫描期间无并发出/入环（E0 BKL 执法面）。
        unsafe {
            let mut best: Option<NonNull<Task>> = None;
            let mut best_eff = i32::MIN;
            let mut best_elig = false;
            let mut best_key = (u64::MAX, u64::MAX);
            let mut cur = h;
            loop {
                let p = cur.as_ptr();
                // eligible 过滤（Running-elsewhere/FROZEN/THROTTLE）在带比较
                // 前——被过滤者不参与 best_eff 竞争（旧两段同口径）。
                if eligible(my_cur, p) {
                    let e = (*p).prio_effective;
                    if !PURE_EEVDF && e > best_eff {
                        // 更高带 → 重置带内最优。
                        let t = &*p;
                        best_eff = e;
                        best = Some(cur);
                        best_elig = t.vruntime <= avg;
                        best_key = (t.vdeadline, t.vruntime);
                    } else if PURE_EEVDF || e == best_eff {
                        // 同带 → EEVDF（eligible 优先；同键首见保序——
                        // `key < best_key` 严格小于，首见不替换）。
                        // W3-Q12 起比较律单源 berry_ringface::pick_better
                        // （host 钉板：eligible 优先档/键序/首见保序/
                        // None 兜底）。
                        let t = &*p;
                        let elig = t.vruntime <= avg;
                        let better = berry_ringface::pick_better(
                            &ring_desc(t),
                            elig,
                            // SAFETY: best 指环上节点（同圈扫描前缀内的
                            // 已见者）——外层 unsafe 块同窗覆盖，裸解引用
                            // 合法（unsafe 台账：scan_filtered 既有 unsafe
                            // 块内，零新增出现点）。
                            best.as_ref().map(|b| {
                                let bt = &*b.as_ptr();
                                (ring_desc(bt), best_elig)
                            }),
                            best_key,
                            avg,
                        );
                        if better {
                            best = Some(cur);
                            best_elig = elig;
                            best_key = (t.vdeadline, t.vruntime);
                        }
                    }
                    // e < best_eff → 低带无资格，跳过。
                }
                cur = (*p).rq_next.get().expect("环上节点必有 next");
                if cur == h {
                    break;
                }
            }
            best
        }
    }
}
