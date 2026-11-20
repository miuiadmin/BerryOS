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
/// `OsSchedTimeConvertFreq` 式。G5 起分母可经 boot-params
/// `sched_slice_hz` 产品级覆盖——缺省编译期常量 SLICE_HZ 不变，见
/// `hal timer::set_slice_hz`）。
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

/// **S2b 纯 EEVDF 开关**（实验轨道；W3-Q51 起单源
/// `berry_ringface::scan::PURE_EEVDF`——ScanAcc 带比较门与本常量同值
/// 执法，两处漂移 = 编译红/语义红）：
/// - `false`（默认，S2a）：严格带序 + 带内 EEVDF——PI boost 即时生效
///   （高带先出），deadline 带（≥1000）恒先于 fair。v1 安全形态。
/// - `true`（S2b）：去带序——所有任务纯 `(eligible, vdeadline, vruntime)`
///   竞争。PI boost 经 weight 影响 VD（软优先——高权→短 VD→常先但不保证）。
///   deadline 带语义失效（绝对截止期 VD 与 fair VD 尺度不可比——需 v2
///   归一化）。**仅实验用——v1 测试全按 S2a 假设构建**。
pub use berry_ringface::scan::PURE_EEVDF;

/// 兼容别名：既有调用点仍以 `RoundRobin` 指类型（1.1/S2 语义已是 Fair）。
pub type RoundRobin = Fair;

// W3-Q11：权重/虚拟片/片预算数学单源在 berry-sliceface（host 可测纯面
// ——本 crate 只 aarch64 可编，预算语义回归面此前 host 不可达）；此处
// 消费即唯一事实源（wqmatch/iprofile 同款纪律）。
use berry_sliceface::{
    fair_rearm_vd, period_due, replenish as replenish_budget, slice_virtual as slice_virtual_of,
    tick_account, throttle_admissible, DeadlineState, SCALE,
};
// W3-Q11：deadline 带优先级常量单源 berry-sliceface——本模块 re-export
// （lib.rs `pub use class::PRIO_DEADLINE` 消费链不变，band.rs 同）。
pub use berry_sliceface::PRIO_DEADLINE;
// 预算授片分派刀（2026-11-19）：授片尺寸律单源 berry-sliceface——本模块
// re-export（lib.rs `pub use class::dl_grant`——ktest 消费走
// berry_sched::dl_grant 形，PRIO_DEADLINE 同款链）。
pub use berry_sliceface::dl_grant;

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

/// ns → CNTVCT 周期数（M4 并线：带参数 ns → 账面周期数的唯一换算点
/// ——`hal::timer::ticks_to_ns` 的逆式，u128 中间积防溢；freq==0 → 0
/// 防御面 = 配额/期长 0 = 不武装，方向安全）。
fn ns_to_cycles(ns: u64) -> u64 {
    let f = hal::timer::cntfrq() as u128;
    if f == 0 {
        return 0;
    }
    ((ns as u128) * f / 1_000_000_000) as u64
}

/// 带权重直入的虚拟片（`slice_virtual` 的 raw-weight 形——M4 并线）：
/// `weight_of_prio` 域 [SCALE, CAP]（基线 1024 起）表达不了带包络中
/// <1024 的下档权重（utility 64..512 / interactive 低界 256），故按
/// **同一数学**（`grant × SCALE / weight`、整除取整抬 1、u128 中间积）
/// 直算。律漂移由 ktest `BANDM4` M4G 数值断言（`vd−vr == grant×SCALE/w`
/// 在 w 整除 SCALE 时整数精确）钉住。
fn slice_virtual_weight(grant: u64, weight: u32) -> u64 {
    let s = grant as u128 * SCALE as u128 / (weight.max(1) as u128);
    (s as u64).max(1)
}

/// 带级预算的 delta 预读（M4 并线——`Fair::tick` 记账基准的**只读孪生**：
/// 读同一 last_vct 槽 + 调用方传入的 `now` 算同一 delta，**不写基准**；
/// 04 号审计机会四②（2026-11-17）起自读 CNTVCT 退役——`now` 由
/// `FairBand::tick` 入口单读线程化下发，`Fair::tick` 随后的正规读/写
/// 自会得出同值——两者差 = tick 入口读与 `Fair::tick` 读（两次 cntvct
/// 读）之间的指令数，微秒以下）。`last == 0`（首拍无基准）→ 0，同
/// `Fair::tick` 律。
fn peek_tick_delta_at(now: u64) -> u64 {
    // SAFETY: 只读不写（不消费基准位）；IRQ 上下本文核顺序访问契约同
    // Fair::tick（per-CPU 槽 / 回落位单核窗）。
    let last = unsafe {
        match hal::smp::last_vct_slot() {
            Some(p) => p.read_volatile(),
            None => (&raw const LAST_VCT).read_volatile(),
        }
    };
    if last == 0 {
        0
    } else {
        now.wrapping_sub(last)
    }
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
/// 者周期到：解冻（清 HELD_THROTTLE）+ 重臂 vdeadline（+= duration_hint_ns =
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
        // W3-Q31 重放：LAST_VCT 回绕窗增量单源 berry-tickface（last==0
        // 首读哨兵 + wrapping_sub 回绕防御——host 钉板 vct_delta）。
        let delta = unsafe {
            match hal::smp::last_vct_slot() {
                Some(p) => {
                    let last = p.read_volatile();
                    p.write_volatile(now);
                    hal::timer::vct_delta(now, last)
                }
                None => {
                    let last = (&raw const LAST_VCT).read_volatile();
                    (&raw mut LAST_VCT).write_volatile(now);
                    hal::timer::vct_delta(now, last)
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
        //（HELD_THROTTLE 置位 → eligible 过滤 → 不再出选）。周期到 →
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
        // S5-c lazy unthrottle（04 号审计附记一收口——2026-11-17）：与
        // trait `pick_next` 同头调用——生产出选两口（本函数七点 +
        // `pick_next_if_above` try_preempt 一点，W3-Q93 现形；pick_next
        // 仅 ktest 直调）头部同源解冻，此前 deadline 任务预算限流后生产
        // 无解冻路径。
        // dl_on_ring 门 0 时零成本（一次 Relaxed 读 + 早退），v1 常态全员
        // fair 门常闭；语义与 pick_next 侧逐位一致（同函数单源）。
        unsafe { lazy_unthrottle_deadlines(rq) };
        let chosen = self.scan_filtered(rq, my_cur)?;
        // SAFETY: scan_filtered 契约（KERN_LOCK 窗内、节点在环）。
        unsafe { rq.advance_past(chosen) };
        let t = unsafe { &mut *chosen.as_ptr() };
        crate::policy::apply(t);
        // OBS-01（06 号 §5 观测维补面）：出选记账——intent 全档 + 带任务
        // per-band（生产出选两口 = 本函数 + `pick_next_if_above`，两口均
        // note_pick；`SchedClass::pick_next` 是 ktest 直调面不经此）。
        // Relaxed 纯计数（stats.rs 纪律）。
        crate::stats::note_pick(t);
        Some(t)
    }

    /// W3-Q93（04 机会一② 双圈合一）：try_preempt 专用出选面——
    /// 「带门探峰 + 出选」单圈合成。
    ///
    /// 旧形（sched.rs try_preempt）：`has_eff_above_filtered` 只读探峰一圈
    /// （早退）→ true 时 `pick_next_filtered` 二圈扫描出选。合一后本函数
    /// 一次扫描（解冻 → 单圈 → 带门 → 出选三件套），语义逐位等价：
    /// - 「环上存在 eligible 且 eff 严格高于 v」⟺ 单圈累加器的
    ///   `best_eff > v`（[`berry_ringface::scan::ScanAcc::best_eff`] 单源
    ///   ——零第二实现，host 钉板 best_eff_above_gate_matches_reference
    ///   3000 例对拍）；
    /// - 门假（无更高带）不出选、不消耗 head 槽位——幻影出选防线语义
    ///   逐位保持（旧 has_eff_above false 早退同义）；
    /// - 门真出选者 = `pick_next_filtered` 出选者（同一扫描单源 +
    ///   advance_past + policy::apply + note_pick 三件套）。
    ///
    /// **语义差异面（审计明示接受）**：解冻时序前移——旧形探峰 false 时
    /// 不经 pick_next_filtered 头部、不解冻；合一后解冻发生在扫描前
    /// （S5-c 头部同源），探峰假也解冻。方向 = 解冻面扩大（限流 deadline
    /// 任务自愈更早），04 号审计附记一（正确性缺口）收口面的加强非削弱；
    /// dl_on_ring 门 0 时零成本，v1 常态零行为差。
    ///
    /// PURE_EEVDF 实验形（v1 恒 false）：带比较恒假 → best_eff 恒哨兵
    /// i32::MIN → 带门对一切有限 v 恒假——直切语义本就随 pure 形失效
    /// （同带竞争无「严格更高」），本函数仅 v1 生产形消费。
    pub fn pick_next_if_above(
        &self,
        rq: &Runqueue,
        _topo: &Topo,
        my_cur: *mut Task,
        v: i32,
    ) -> Option<&mut Task> {
        debug_assert!(!PURE_EEVDF, "pure 形带门退化，本面仅 v1 生产形消费");
        // S5-c 解冻头部与 pick_next_filtered 同源（见彼注）。
        unsafe { lazy_unthrottle_deadlines(rq) };
        let (best, best_eff) = self.scan_with_eff(rq, my_cur);
        // 带门（旧 has_eff_above_filtered 探峰谓词的累加器内形）：
        // 无 eligible 带严格高于 v ⟹ 必不直切 ⟹ 不消耗 head 槽位
        //（幻影出选防线逐位保持——best_eff 哨兵 i32::MIN 覆盖空环/
        // 全过滤两形，旧探峰空环返 false 同义）。
        if best_eff <= v {
            return None;
        }
        let chosen = best?;
        // SAFETY: scan_with_eff 契约同 scan_filtered（KERN_LOCK 窗内、
        // 节点在环）。
        unsafe { rq.advance_past(chosen) };
        let t = unsafe { &mut *chosen.as_ptr() };
        crate::policy::apply(t);
        crate::stats::note_pick(t);
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
        self.scan_with_eff(rq, my_cur).0
    }

    /// [`ScanAcc::best_eff`](berry_ringface::scan::ScanAcc::best_eff) 伴生
    /// 导形（W3-Q93）：同一单圈走查额外带出累加器最高 eligible 带——
    /// `pick_next_if_above` 带门的唯一数据源（零第二扫描）。
    fn scan_with_eff(
        &self,
        rq: &Runqueue,
        my_cur: *mut Task,
    ) -> (Option<NonNull<Task>>, i32) {
        // 空环：无走查即返（哨兵 i32::MIN = 旧探峰空环 false 同义）。
        let Some(h) = rq.head() else {
            return (None, i32::MIN);
        };
        let avg = avg_vruntime(rq).unwrap_or(0);
        // W3-Q51：三分支带序状态机骨架单源 berry-ringface::scan::ScanAcc
        //（host 钉板：带序重置/退化带首见/过滤前置/PURE_EEVDF 两形/敌意
        // 全表 2000 例与两圈参考实现逐例一致）；本函数只留环走查
        //（rq_next 推进 + 回 head 闭合判）与 NonNull 投影的物理臂。
        use berry_ringface::scan::ScanAcc;
        // SAFETY: 环上任务均调度器拥有且在环；调用侧全在 KERN_LOCK 窗内
        // （SVC/tick/klock 包络）——扫描期间无并发出/入环（E0 BKL 执法面）。
        unsafe {
            let mut acc = ScanAcc::new();
            let mut cur = h;
            // SMP-01：带限流跨核执法的扫描门（零武装 = unit 单带负载的常态
            // ——一次 Relaxed 原子读 + 逐节点一个分支，零执法动作）。门开时
            // 逐节点 lazy 双向对账（限流补位 / 翻期残留愈合——band.rs
            // lazy_enforce 注；先于 offer = 本扫的 eligible 判定即见新位）。
            let lazy = crate::band::any_armed();
            loop {
                let p = cur.as_ptr();
                if lazy {
                    crate::band::lazy_enforce(p);
                }
                acc.offer(&ring_desc(&*p), cur, p == my_cur, avg);
                cur = (*p).rq_next.get().expect("环上节点必有 next");
                if cur == h {
                    break;
                }
            }
            (acc.best(), acc.best_eff())
        }
    }
}

// ---- M4 带实例化（ADR #42 第二段——class_fair 按 intent 实例化带实例；
// 调度优化终窗 2026-11-15 骨架落，2026-11-16 首批 band 激活——两带现役、
// 参数差异生效；2026-11-16 集成面收口〔06 号 §5 任务①〕——class 赋值面
// `class_of_task` 接通 spawn→调度原语六点；2026-11-16 第五刀 M4 并线
// 〔R4 语义审计链 1 补刀——「分派通、行为差异=0」的最后一毫米〕：trait
// 行为差异化落地〔tick 带参数 + 四 API 执法并线；enqueue 纯委托保
// 首选序——见 trait impl doc〕；2026-11-17 CP-09 EAS M2 批次〔06 号 §2
// Top-3 ③〕——四带全开：LC(0x01)/BG(0x04) 入列，现役集 ×2→×4----
//
// 立法约束（ADR #42 / 重议触发器表「class_fair 单实例 → ×N 带实例」条）：
// - **只许按 intent 实例化同一 `SchedClass` trait、禁改签名**——本段零
//   trait 改动、零既有签名改动；现役单带 [`Fair`] 原样保留（`RoundRobin`
//   别名与全部既有调用点零漂移）。
// - 现役带集 = 四带全开 0x01–0x04（M4 首批 ×2 于 2026-11-17 升 ×4——
//   LC/BG 激活，06 号 §2 Top-3 ③；四带 quantum/权重包络/latency_target
//   参数差异经 [`class_fair_intent`] 工厂生效，ktest `BANDM4` 断言）；
//   0x05 MAINTENANCE 编码冻结不启用、0x00 UNSPECIFIED 无专带（XNU 式
//   顶档待定档）——词汇单源 `berry_iprofile::band_enabled`（ADR #42 ×4
//   终态自此兑现）。
// - **入场券证据面已过、第五刀并线**：loadgen 四组真差异双 run 复现
//   （R4 审计链 3：ut_p50 758→10776〔slack〕、p99 1507→37916〔band〕
//   双日志复现）后，trait 行为差异化落地——[`FairBand`] 的 tick 按带
//   参数执法（quantum/权重/latency_target 三轴进热路径），带预算四
//   API（`band.rs`）经 [`FairBand::consume`] 等包装并线；enqueue 保持
//   纯委托（首选序零漂移——见 trait impl doc 的编舞实证）；unit
//   [`Fair`] 路径逐位不动（unit 任务零漂移——`band_period_sweep` 无
//   武装槽早退、`TaskClass::grant_cycles` 的 Unit 臂返全局 slice）。
// - 数值不写死：[`IntentBand`] 表 = 骨架缺省基线（推断值，锚 = 手表表
//   权重包络 + 现役 SLICE_HZ quantum 锚——见 `berry-iprofile` 表注）；
//   M4 真数字经启动配置参数表覆盖（ADR #42「数字进启动配置不写死」）
//   ——显式 configure 优先于懒武装（已武装槽不覆写）。

use berry_iprofile::IntentBand;

/// class_fair 的**参数化带实例**（M4 并线形）：与 [`Fair`] 同一 fair 语义
/// （带内 EEVDF——逐任务记账委托 unit `Fair` 单源），携带 id + 一份
/// [`IntentBand`] 带参数。**第五刀（2026-11-16）起带参数真消费**：
/// quantum_ns → 片授予（[`TaskClass::grant_cycles`〕/VD 重臂 grant）+
/// 带配额（[`Self::consume`] 懒武装 quota）；latency_target_ns → 带预算
/// 期长（[`Self::period_check`]）；low/high_weight → 限流/解限流与片耗尽
/// 的 VD 重臂权重（替代全局 prio_effective）。
///
/// 构造走 [`class_fair_intent`] 工厂（按 intent 档建带）或 [`FairBand::new`]
/// （显式带参数——M4 启动配置覆盖路径的入口形状）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FairBand {
    /// 带 id = intent 档编码（带键 = intent，§11；工厂路径恒现役集——
    /// 四带 0x01–0x04，`band_of_intent` 门保证；`new` 显式路径的域执法归
    /// 调用方〔启动配置面，band_id_of_intent 判〕）。
    pub band_id: u8,
    /// 本带参数（M4 并线消费源：quantum/权重包络/latency_target 三轴
    /// 全部进热路径——见本类型 doc 与 [`Self::consume`] 族）。
    pub band: IntentBand,
}

impl FairBand {
    /// 显式带参数构造（M4 启动配置覆盖路径入口形状——数字进启动配置，
    /// 不写死；band_id 域执法归调用方）。
    pub const fn new(band_id: u8, band: IntentBand) -> FairBand {
        FairBand { band_id, band }
    }

    /// yield 片归还委托（[`Fair::yield_slice_return`] 同款——trait 外
    /// inherent 方法，[`TaskClass`] 分派臂消费；行为与 unit Fair 逐位一致，
    /// M4 入场券纪律）。
    pub fn yield_slice_return(&self, t: &mut Task, rq: &Runqueue) {
        Fair.yield_slice_return(t, rq)
    }

    // —— M4 并线（第五刀·R4 语义审计链 1 补刀）：带参数执法面——
    // band.rs 四 API（consume/period_check/throttle_all/unthrottle_all）
    // 的带参数包装：quota = quantum_ns（替代 unit Fair 的全局 slice 锚）、
    // 期长 = latency_target_ns、限流/解限流 VD 重臂权重 = low/high_weight
    // （替代全局 prio_effective）。带预算槽**懒武装**——首消费时按表行
    // configure；显式 configure（启动配置/ktest 手驱）优先，已武装不覆写。——

    /// 本带 quantum 的周期数（片授予〔`TaskClass::grant_cycles`〕与带
    /// 配额〔`Self::consume` 懒武装〕的共用换算源）。
    pub fn quantum_cycles(&self) -> u64 {
        ns_to_cycles(self.band.quantum_ns)
    }

    /// quantum 的武装槽镜像优先形（04 号审计机会四①（2026-11-17））：
    /// [`crate::band::quota_cycles_of`] 命中 → 槽 quota 镜像（免每授片
    /// 一次 cntfrq MRS + u128 软除法——数十指令）；未武装 / 越界 → 回落
    /// 现算 [`Self::quantum_cycles`]。两值恒等（镜像一致性依据 = quota
    /// 唯一写点 configure + 表行 ns 与 cntfrq 运行期恒定，见
    /// `quota_cycles_of` doc；显式 configure 下授片跟随槽 quota 的语义
    /// 变化同彼处明示）。私有——消费面三处（`TaskClass::grant_cycles`
    /// Band 臂 / [`Self::rearm_vd`] / [`Self::rearm_members_vd`]）；
    /// [`Self::arm`] 不动 = 写点保持现算（「现算 → configure → 槽镜像」
    /// 数据流单向——可达路径上镜像恒 None，arm 时刻写入的 quota 即镜像
    /// 源头）。
    fn quantum_cached(&self) -> u64 {
        crate::band::quota_cycles_of(self.band_id).unwrap_or_else(|| self.quantum_cycles())
    }

    /// 本带 latency_target 的周期数（带预算期长——`Self::period_check`
    /// 判到期的参数源）。
    pub fn period_cycles(&self) -> u64 {
        ns_to_cycles(self.band.latency_target_ns)
    }

    /// 带预算槽懒武装：未武装才 configure（quota = 本带 quantum、period =
    /// 本带 latency_target）。已武装槽不覆写——启动配置/ktest 显式
    /// configure 优先（重配即清账翻期语义归 `band::configure` 单源）。
    pub fn arm(&self, now: u64) -> bool {
        if crate::band::band_on(self.band_id) {
            return true;
        }
        crate::band::configure(self.band_id, self.quantum_cycles(), self.period_cycles(), now)
    }

    /// 带配额微记账（四 API·consume）：`delta` 周期数记入本带本期账——
    /// **quota = 本带 `quantum_ns`**（band 特有配额，替代 unit Fair 的
    /// 全局 slice 锚）。**CBS 期内律**：只记本期内的周期数——`delta`
    /// 先剪辑到 `now − period_start`（period_start = deadline − period，
    /// deadline 恒未来首期故剪辑界 ≤ period），跨期（旧期）部分不记入
    /// 新期账；**窗口端点 = delta 端点**（04 号审计机会四②（2026-11-17）
    /// 收口——`now` 形参与 [`Self::period_check`] 同参：剪辑界与到期判
    /// 同一时间戳 ⇒「剪辑界 > period」的理论窗构造性消灭〔不变式收紧〕
    /// ；原各点自读时的语义差 = 亚 µs 级指令窗 vs 最小期 1ms，方向单边
    /// 保守——见 `FairBand::tick` 入口注）。没有本律时：翻期面粒度
    /// （tick/sweep 间隔）> period 的带（interactive 10ms 配额/8ms 期 vs
    /// 100ms tick）会把跨期运行累计进单期账 → quota>period 的不可越线带
    /// 被误越线（ktest M4G 实测抓到——6 次假越线）；有本律则该类带任何
    /// 记账粒度下恒不越线。返 true = 本期**首**越线（调用方随后
    /// [`Self::throttle_all`]——记账与环执法分两步，同 `band::consume`
    /// 契约）。判定律/原子账单源 `band::consume`（此处只携参 + 剪辑）。
    pub fn consume(&self, delta: u64, now: u64) -> bool {
        if !self.arm(now) {
            return false;
        }
        let credited = match crate::band::state(self.band_id) {
            Some(s) if s.period > 0 => {
                let in_period = now.saturating_sub(s.deadline.saturating_sub(s.period));
                delta.min(in_period)
            }
            _ => delta,
        };
        crate::band::consume(self.band_id, credited)
    }

    /// 带翻期判（四 API·period_check）：到期判 = **本带
    /// `latency_target_ns`**（band 特有延迟目标作预算期长）。返 true =
    /// 已翻期（调用方随后 [`Self::unthrottle_all`]）。翻期数学/CAS 防双翻
    /// 单源 `band::period_check`。
    pub fn period_check(&self, now: u64) -> bool {
        if !self.arm(now) {
            return false;
        }
        crate::band::period_check(self.band_id, now)
    }

    /// 带级限流（四 API·throttle）：环扫置 HELD_THROTTLE + 带成员 VD 按
    /// **`band.low_weight`** 重臂（被限流者垫后——带权重替代全局
    /// prio_effective 的消费点）。deadline 任务豁免（`band_member`）。
    pub fn throttle_all(&self, rq: &Runqueue) -> usize {
        let n = crate::band::throttle_all(self.band_id, rq);
        if n > 0 {
            self.rearm_members_vd(rq, self.band.low_weight);
        }
        n
    }

    /// 带级解限流（四 API·unthrottle）：环扫清 HELD_THROTTLE + 带成员
    /// VD 按 **`band.high_weight`** 重臂（回场竞争给早截止——高权带回场
    /// 不排尾）。
    pub fn unthrottle_all(&self, rq: &Runqueue) -> usize {
        let n = crate::band::unthrottle_all(self.band_id, rq);
        if n > 0 {
            self.rearm_members_vd(rq, self.band.high_weight);
        }
        n
    }

    /// 本带当前 VD 重臂权重：带限流期 = low_weight（垫后）、正常 =
    /// high_weight（`state().throttled` 镜像——首越线置/翻期清）。
    fn vd_weight(&self) -> u32 {
        let throttled =
            crate::band::state(self.band_id).map(|s| s.throttled).unwrap_or(false);
        if throttled {
            self.band.low_weight
        } else {
            self.band.high_weight
        }
    }

    /// 单任务片耗尽 VD 重臂（带参数形）：grant = 本带 quantum、weight =
    /// 带权重（[`Self::vd_weight`]）——`Fair::tick` 的全局 grant/权重
    /// 重臂被本形覆写（enqueue 面不覆写——首选序零漂移，见 trait impl
    /// doc）。
    fn rearm_vd(&self, t: &mut Task) {
        t.vdeadline = t
            .vruntime
            .saturating_add(slice_virtual_weight(self.quantum_cached(), self.vd_weight()));
    }

    /// 环扫带成员 VD 重臂（throttle/unthrottle 包装的权重消费步——§A136
    /// 走查同 `band.rs` 环扫；deadline 任务经 `band_member` 豁免；幂等
    /// ——被限流者 vruntime 冻结〔不跑不记〕，重复重臂同值）。
    ///
    /// # Safety 契约（同 `band::throttle_all`）
    /// rq 环节点有效且在环；屏蔽窗内调用（tick / pick_next 语境）。
    fn rearm_members_vd(&self, rq: &Runqueue, w: u32) {
        let Some(h) = rq.head() else { return };
        let grant = self.quantum_cached();
        // SAFETY: 环上任务均调度器拥有且在环；调用侧屏蔽窗内（同 pick）。
        unsafe {
            let mut cur = h;
            loop {
                let p = cur.as_ptr();
                if crate::band::band_member(
                    (*p).intent_effective,
                    (*p).prio_effective,
                    self.band_id,
                ) {
                    (*p).vdeadline =
                        (*p).vruntime.saturating_add(slice_virtual_weight(grant, w));
                }
                cur = (*p).rq_next.get().expect("环上节点必有 next");
                if cur == h {
                    break;
                }
            }
        }
    }
}

/// **class_fair_intent 工厂**：按 intent 档建 [`FairBand`] 带实例——查
/// `berry_iprofile::band_of_intent`（现役集四带 0x01–0x04 → 该档带参数
/// 行；首批外档/保留域 → `None` = 无带，调用方回落现役单带 [`Fair`]，
/// 缺省行为零漂移）。
///
/// **集成面（06 号 §5「水通管道」任务①收口，2026-11-16）**：task→band
/// 赋值已接——[`class_of_task`]（class 赋值面单源）按 `intent_effective`
/// 查本工厂得带实例，`sched.rs` per-task 调度原语六点消费
/// （`enqueue_with_kick`〔spawn+唤醒〕/偷取重入环/tick/dequeue×2/yield）；
/// intent 双落点 = `sched.rs::spawn_intent`（内核 spawn 面）与 EL0 attr 面
/// （`apply_sched_attr_to_task`——spawn 后落 intent，动态派生下一次原语
/// 调用即改带）。策略覆盖通道 = `policy::band_of`（band 选择位，v3
/// `policy_bind` 写路）仍归 v3——本工厂只认 intent 档。
pub fn class_fair_intent(intent: u8) -> Option<FairBand> {
    berry_iprofile::band_of_intent(intent).map(|band| FairBand::new(intent, band))
}

/// 带实例的 trait 面：**第五刀 M4 并线起 tick 行为差异化**——逐任务
/// EEVDF 记账仍委托 unit [`Fair`]（单源：slice 递减/vruntime/Σ 账本
/// /util 面零复制），差异叠加三处：①片耗尽的 VD 重臂走**带参数**
/// （grant = 本带 quantum、weight = 带包络——替代全局 slice_cycles/
/// prio_effective）；②tick 内自翻期（本带到期即翻 + 解限——事件驱动
/// 清账，CBS 期内律见 [`Self::consume`] 注）；③带预算执法
/// （[`Self::consume`] 首越线 → [`Self::throttle_all`]）。dequeue/
/// pick_next/priority 仍纯委托（生产 pick 是队列级
/// `Fair::pick_next_filtered` 不经本面；`priority()` 返 0——带间严格序
/// 映射到既有签名仍归后续刀）。
///
/// **enqueue 特例保持纯委托**（零漂移）：入环 VD 若按带权重重臂，会改
/// 变新任务的首选序（utility 偏移 40ms virt < unit 100ms virt → 后 spawn
/// 的 utility 任务反超先跑）——boot 结算链等隐式编舞依赖 spawn 序
/// （ktest 实证：it_sinkA 抢到 it_mid 前 → 非阻塞 recv 空
/// `ERR_AGAIN`）。带权重只在**竞争路径**消费（片耗尽/限流/解限重臂）
/// ——秒回任务（入环即出选跑完）不受扰动，差异化作用于真正烧 CPU 的
/// 带任务。
impl SchedClass for FairBand {
    fn enqueue(&self, t: &mut Task, rq: &mut Runqueue) {
        // 纯委托（零漂移）——VD 带参数重臂不在此面，见本 impl doc。
        Fair.enqueue(t, rq)
    }
    fn dequeue(&self, t: &mut Task, rq: &mut Runqueue) {
        Fair.dequeue(t, rq)
    }
    fn pick_next(&self, rq: &Runqueue, topo: &Topo) -> Option<&mut Task> {
        Fair.pick_next(rq, topo)
    }
    fn tick(&self, t: &mut Task, rq: &mut Runqueue) {
        // tick 入口单读 now（04 号审计机会四②（2026-11-17）——带 tick
        // 路径四次 CNTVCT 读归二）：带面三消费点（delta 预读 / 自翻期 /
        // 记账剪辑）共用一份时间戳；`Fair::tick` 委托内的 last_vct 基准读
        // 保持原位——它是**写点**（记账基准前推），不并入本读。
        // 语义差（不写「零语义变化」）：入口读与原各点自读间的指令窗
        // （亚 µs 级 vs 最小期 1ms）使恰边界翻期/首越线可迟一拍，方向
        // 单边保守——限流多持一会儿、记账只少不多、封顶下一拍 sweep/tick
        // 补翻（例外：懒武装首臂 deadline 锚随入口读前移一指令窗，首次
        // 解限可早一拍——一次性、每带仅首臂、同亚 µs 量级，审查轮补记）（[`band_period_sweep`] + lazy_enforce 双兜底在席 = 与
        // try_rq_lock 失败跳环扫同类已接受偏差）；反向收益 = consume 剪辑
        // 与 period_check 同参，「剪辑界 > period」理论窗消灭（不变式收紧
        // ——见 [`Self::consume`] 注）。
        let now = hal::timer::cntvct();
        // 带级 delta 预读（Fair.tick 记账基准的只读孪生——带账与任务账
        // 同 delta 口径，见 peek_tick_delta_at 注）。
        let delta = peek_tick_delta_at(now);
        // OBS-01：per-band runtime 累积（delta 口径同上；unit Fair 路径
        // 零触碰——unit 任务零漂移纪律）。
        crate::stats::note_runtime(self.band_id, delta);
        Fair.tick(t, rq);
        // ① quantum 差异化：片耗尽 VD 重臂走带参数（grant = 本带
        // quantum / weight = 带包络——Fair.tick 的全局重臂被覆写；门与
        // Fair.tick 同款 fair_rearm_vd）。
        if t.slice_left == 0 && fair_rearm_vd(t.prio_effective, is_deadline(t)) {
            self.rearm_vd(t);
        }
        // ② 带预算执法·自翻期：本带到期即翻 + 解限（事件驱动——记账前
        // 清账，防 sweep/tick 粒度下的跨期累积；CBS 期内律见 consume 注）。
        // 入口扫 [`band_period_sweep`] 仍是全限流带的兜底翻期面。
        // F3（SMP-01 §3.3）：环扫（含 rearm_members_vd 走查）入
        // [`crate::sched::try_rq_lock`] 非阻塞窗——对核 try_steal_one 持本环
        // RQ 锁 unlink（被摘节点双字清 None），免锁走查可读半摘节点撕态
        // （§A113 族）。try 失败跳过本轮环扫（下轮 tick 追上 + lazy_enforce
        // pick 面兜底）——跳的是环扫不是记账：period_check 的原子翻期在
        // 窗外照常落账。永不阻塞 ⇒ 永不 wedge（tick 面禁自旋等锁判例见
        // sched::try_rq_lock_of 注）。
        if self.period_check(now) {
            if let Some(phys) = crate::sched::try_rq_lock() {
                self.unthrottle_all(rq);
                crate::sched::try_rq_unlock(phys);
            }
        }
        // ③ 带预算执法·记账：consume（quota = 本带 quantum）首越线 →
        // throttle_all（low_weight VD 重臂）。环扫同入 F3 try 窗（②同款：
        // consume 的原子记账/越线镜像位在窗外已落，try 失败只迟置环上位
        // ——出选扫描的 lazy_enforce 补位，至多迟一个片）。
        if self.consume(delta, now) {
            if let Some(phys) = crate::sched::try_rq_lock() {
                self.throttle_all(rq);
                crate::sched::try_rq_unlock(phys);
            }
        }
    }
    fn priority(&self) -> i32 {
        Fair.priority()
    }
}

// ---- M4 集成面收口（06 号 §5「水通管道」任务①，2026-11-16）+ 第五刀
// 行为并线（同日·R4 语义审计链 1 补刀）：class 赋值面单源——intent →
// class 实例的最后一环，及带参数的热路径消费 ----
//
// 链路（管道闭合）：manifest 声明 intent → svcmgr 透传 → kernel spawn
// 落 intent（`spawn_intent` intent 参数 / EL0 thread attr 双落
// `intent_effective`）→ **[`class_of_task`] 分派（本段）** → 调度原语消费
// （`sched.rs` 六点）。带键 = `intent_effective`（§11「band 不加 attr
// 字段——intent 即带键」）：零 Task 新字段，intent 结算借入改档后下一次
// 原语调用自然跟随。现役集（四带 0x01–0x04）走 [`FairBand`]（带参数自
// `INTENT_BAND_TABLE` 表行分化），首批外档/未声明回落现役单带 [`Fair`]
// （行为零漂移）。第五刀起 FairBand 的 tick 真消费带参数
// （quantum/权重/latency_target 三轴），带预算四 API 并线
// （[`FairBand::consume`] 族——enqueue 纯委托保首选序，见 trait impl
// doc）；unit Fair 单带路径逐位不动（`grant_cycles` Unit 臂 = 全局
// slice、[`band_period_sweep`] 无武装槽早退）。

/// 任务的 class 实例（M4 集成面）：unit [`Fair`] 或带实例 [`FairBand`] 的
/// 二元分派——[`class_of_task`] 的返回值。`Copy` 零分配（IRQ 热路径——
/// 每次调度原语即时派生，不缓存不落 Task 字段）。`Unit` 无载荷（unit
/// `Fair` 是无状态单例，分派臂现取 `Fair` 值——零成本）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskClass {
    /// 现役单带（未声明 intent/首批外档/保留域——与 `Fair` 逐位等价，
    /// 缺省行为零漂移）。
    Unit,
    /// 带实例（现役集四带 0x01–0x04——`class_fair_intent` 工厂产物，带参数
    /// 携表行 quantum/权重包络/latency_target）。
    Band(FairBand),
}

impl SchedClass for TaskClass {
    fn enqueue(&self, t: &mut Task, rq: &mut Runqueue) {
        match self {
            TaskClass::Unit => Fair.enqueue(t, rq),
            TaskClass::Band(b) => b.enqueue(t, rq),
        }
    }
    fn dequeue(&self, t: &mut Task, rq: &mut Runqueue) {
        match self {
            TaskClass::Unit => Fair.dequeue(t, rq),
            TaskClass::Band(b) => b.dequeue(t, rq),
        }
    }
    fn pick_next(&self, rq: &Runqueue, topo: &Topo) -> Option<&mut Task> {
        match self {
            TaskClass::Unit => Fair.pick_next(rq, topo),
            TaskClass::Band(b) => b.pick_next(rq, topo),
        }
    }
    fn tick(&self, t: &mut Task, rq: &mut Runqueue) {
        match self {
            TaskClass::Unit => Fair.tick(t, rq),
            TaskClass::Band(b) => b.tick(t, rq),
        }
    }
    fn priority(&self) -> i32 {
        match self {
            TaskClass::Unit => Fair.priority(),
            TaskClass::Band(b) => b.priority(),
        }
    }
}

impl TaskClass {
    /// yield 片归还的分派形（[`Fair::yield_slice_return`] 的 per-task
    /// 消费点——两臂同委托 unit Fair，行为零漂移）。
    pub fn yield_slice_return(&self, t: &mut Task, rq: &Runqueue) {
        match self {
            TaskClass::Unit => Fair.yield_slice_return(t, rq),
            TaskClass::Band(b) => b.yield_slice_return(t, rq),
        }
    }

    /// 授片量（per-task 分派——M4 并线第五刀）：`Unit` = 全局
    /// `slice_cycles`（**零漂移**——现役所有非 band 任务不受影响）；
    /// `Band` = 武装槽 quota 镜像优先（04 号审计机会四①（2026-11-17）——
    /// [`FairBand::quantum_cached`]），未武装回落现算——懒武装路径两值恒等；显式 configure 下授片跟随槽 quota（语义变化明示见 quota_cycles_of doc）（band
    /// 特有配额替代全局片）。deadline 例外（预算授片分派刀 2026-11-19）：
    /// 带预算 deadline 任务的授片不取本面值——dl_grant 双门先行授 budget
    /// （[`TaskClass::grant_cycles_of`] per-task 面，本面成为其回落臂），
    /// 故「Unit=零漂移」对带预算 deadline 任务不成立；本面原样保留 = class
    /// 面事实源（ktest class 面断言零动）。原直接消费面 = `sched.rs` 八
    /// Band 可达授片点 + idle 授片（Unit——`sched.rs::on_irq_tick` idle 臂
    /// intent 0 不出带），interactive 短片/ utility 长片的差异化源头。
    pub fn grant_cycles(&self) -> u64 {
        match self {
            TaskClass::Unit => hal::timer::slice_cycles(),
            TaskClass::Band(b) => b.quantum_cached(),
        }
    }

    /// per-task 授片面（预算授片分派刀 2026-11-19）：deadline 正交轴前置
    /// 于 class 分派——[`dl_grant`] 双门（deadline + preempt_threshold>0）
    /// 命中授 budget 尺寸片，否则回落 class 面 [`TaskClass::grant_cycles`]。
    /// 三点口径：① class 面 `grant_cycles` 原样（ktest class 面断言零动）；
    /// ② deadline 轴正交于 intent 带轴——Band+deadline 组合实存（ktest
    /// SMPLZ sldl 形；生产 `apply_sched_attr_to_task` 的 INTENT 块与
    /// policy=3 块独立 if 可并置），命中时预算压过本带 quantum；③ 回落 =
    /// class 面（budget=0 是 ktest 手造与生产 sched_setattr 的合法态——
    /// 无预算 deadline 任务不被 0 预算踩死，replenish 回落律同款；Band
    /// 无预算 deadline 保带 quantum 逐位零漂移）。消费面 = `sched.rs`
    /// 授片习语单形（八授片点 + idle 点——idle 免疫由双门结构性保证：
    /// IDLE_TASK prio/threshold 双零，两合取各自独立不满足）。
    pub fn grant_cycles_of(&self, t: &Task) -> u64 {
        if let Some(b) = dl_grant(t.prio_effective, t.preempt_threshold) {
            return b;
        }
        self.grant_cycles()
    }

    /// 带参数投影（`Unit` = None）——ktest spawn 面断言与诊断面消费。
    pub fn band(&self) -> Option<IntentBand> {
        match self {
            TaskClass::Unit => None,
            TaskClass::Band(b) => Some(b.band),
        }
    }
}

/// **class 赋值面单源**（M4 集成面收口）：task → class 实例——
/// `intent_effective`（§11 带键）经 [`class_fair_intent`] 工厂查表：现役
/// 集（四带 0x01–0x04）→ [`TaskClass::Band`]（表行带参数）；其余/未声明
/// → [`TaskClass::Unit`]（现役单带，缺省行为零漂移）。**动态派生、零
/// 缓存**：intent 结算借入改档（`sched::intent`）或 EL0 attr spawn 后落
/// 档后，下一次调度原语调用自然改带——「spawn 时 band 赋值」与「运行期
/// band 跟随」同一实现。
///
/// 消费面 = `sched.rs` per-task 调度原语六点（`enqueue_with_kick`〔spawn+
/// 唤醒〕/偷取重入环/`tick`/`dequeue`×2/yield）；`pick_next_filtered`
/// 是队列级扫描（混合带同环出选），不经 per-task 分派。
#[inline]
pub fn class_of_task(t: &Task) -> TaskClass {
    match class_fair_intent(t.intent_effective) {
        Some(band) => TaskClass::Band(band),
        None => TaskClass::Unit,
    }
}

/// **M4 并线·带翻期扫**（`sched::tick` 入口的兜底消费点——第五刀）：
/// 逐武装槽 `period_check` → 翻期即解限流。挂 tick 入口的原因：带预算
/// 越线后全员限流 → 无带任务可作出选/被 tick → [`FairBand::tick`] 面
/// 不可达 → 翻期必须有不依赖「当前任务是带任务」的兜底点，否则带级
/// **永久饿死**。解限流走 [`FairBand::unthrottle_all`]（high_weight VD
/// 重臂）；显式 configure 武装的无带实例槽（ktest 手驱先例——如
/// INTEG 的 X=0x01）回落 raw 面。无武装槽时 8 次冷读早退——unit 单带
/// 负载零语义差（执法面只作用于带任务）。
///
/// 【推】SMP 限制：翻期 CAS 单赢家只解**本核**环成员——跨核限流位
/// 残留归 SMP lazy-pick 执法刀（`band.rs`「SMP §3」同纪律）；单核
/// QEMU 主验面无此窗。
pub(crate) fn band_period_sweep(rq: &Runqueue) {
    let now = hal::timer::cntvct();
    for b in 0u8..(crate::band::BAND_SLOTS as u8) {
        if !crate::band::band_on(b) {
            continue;
        }
        if crate::band::period_check(b, now) {
            if let Some(fb) = class_fair_intent(b) {
                fb.unthrottle_all(rq);
            } else {
                crate::band::unthrottle_all(b, rq);
            }
        }
    }
}
