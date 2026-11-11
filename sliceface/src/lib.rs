//! berry-sliceface —— 时间片预算数学纯函数面（W3-Q11）。
//!
//! **动机**：调点调度 1.1 的时间片预算数学（`调度器设计-v1.1.md` §3 /
//! `调点调度-v1.1.md` 记账/预算准入）此前只活在 `sched/src/class.rs`——
//! sched 依赖 HAL global_asm，只 aarch64-unknown-none 可编，host 测不可
//! 达，ktest 只覆盖 boot 相。片预算语义的回归面（饱和递减律、耗尽判、
//! 权重换算帽、deadline 周期到判与 replenish、VD 重臂条件）需要一张
//! host 可测的**钉板**：本 crate 把预算数学按值域抽成纯函数 + 表驱动
//! 测试，`sched` 侧消费同一函数（零第二实现——抽离即唯一事实源，非
//! 镜像；wqmatch/chanface 同款纪律）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`SCALE`]/[`WEIGHT_BOOST_UNIT`]/[`WEIGHT_CAP`]：权重刻度常量单源
//!   （自 class.rs 迁此，sched `pub use` re-export）。
//! - [`weight_of`]/[`slice_virtual`]:EEVDF 权重与虚拟片换算——基线
//!   1024、每级 boost +64、封顶 1<<20（防 u64 溢出）；虚拟片 =
//!   grant×SCALE/weight，下限 1（整除取整永不归零）。
//! - [`tick_account`]:tick 记账一步——slice 饱和递减 + vruntime 饱和
//!   累加 vdelta（grant 域内 delta 的像恒 < 2^63，saturating_add 防御
//!   路径保留）。
//! - [`slice_exhausted`]:片耗尽判（==0 即耗尽——饱和递减保证只减不增
//!   负片）。
//! - [`fair_rearm_vd`]:fair 任务片耗尽后的 VD 重臂资格（非 deadline
//!   才重臂——deadline 任务的手动绝对截止期不碰，1.1/S5）。
//! - [`DeadlineState`]/[`period_due`]/[`replenish`]:deadline 周期面——
//!   周期到判（now ≥ vdeadline 且 period>0）+ 重臂（vdeadline += period
//!   饱和）+ 新预算（budget>0 才授予，0 保持现值——「无预算面」任务
//!   不被 0 预算踩死）。
//! - [`throttle_admissible`]:S5-c 预算执法准入——deadline + 预算已设
//!   （preempt_threshold>0）+ 未在限流（HELD_THROTTLE 位清）三与门。
//!
//! 与冻结面的关系：`SchedClass` trait 冻结签名零触碰；SLEEPQ/RQ 物理
//! 结构不在此 crate（裸指针机械留 sched——本面只钉「预算怎么算」的
//! 数定律，不钉「环怎么转」的机械）。HAL 时基（`slice_cycles`/`cntvct`）
//! 以参数注入——纯面无时基依赖。

#![cfg_attr(not(test), no_std)]
#![allow(clippy::result_unit_err)]

/// 虚拟时间刻度（权重基准）。自 class.rs 单源迁此。
pub const SCALE: u64 = 1024;

/// 权重 boost 步进（PI/策略每级 +64）。自 class.rs 字面迁此。
pub const WEIGHT_BOOST_UNIT: u64 = 64;

/// 权重封顶（1<<20——防 u128 中间积溢出）。自 class.rs 字面迁此。
pub const WEIGHT_CAP: u64 = 1 << 20;

/// EEVDF 权重：基线 1024；boost 每级 +64，封顶 1<<20。
///
/// `prio_effective < 0`（理论上 PI 深链可达负值）取 0 → 纯基线；
/// 与 class.rs 旧式 `t.prio_effective.max(0) as u64` 逐位一致。
#[inline]
pub fn weight_of_prio(prio_effective: i32) -> u64 {
    let boost = prio_effective.max(0) as u64;
    (SCALE + boost.saturating_mul(WEIGHT_BOOST_UNIT)).min(WEIGHT_CAP)
}

/// 虚拟片（EEVDF 的 V 尺度片长）= `grant × SCALE / weight`，下限 1。
///
/// 整除取整：grant=1/weight=WEIGHT_CAP 时商 0 → 抬 1（VD 至少前进一个
/// 虚拟刻度，防 0 宽片恒 eligible）；u128 中间积：grant ≤ u64 域时
/// 积 < 2^127 无溢出（WEIGHT_CAP 封顶保证除数 ≥ 1024）。
#[inline]
pub fn slice_virtual(grant: u64, prio_effective: i32) -> u64 {
    let s = grant as u128 * SCALE as u128 / weight_of_prio(prio_effective) as u128;
    (s as u64).max(1)
}

/// tick 记账一步（纯值域形）：返回 `(slice_left', vruntime', vdelta)`。
///
/// - slice：**饱和递减**（`saturating_sub`）——delta 超剩余即归 0，
///   永不出负片（u64 域无负，饱和 = 耗尽的唯一表达）。
/// - vruntime：**饱和累加** vdelta = `delta × SCALE / weight`——
///   grant 域内 delta 的像恒小，saturating 防御路径保留（vruntime
///   逼近 u64::MAX 的假想态防御，与 class.rs 逐字同款）。
/// - vdelta 同时返回（Σvruntime 账本随迁的消费值——避免调用侧二次
///   换算漂移）。
#[inline]
pub fn tick_account(
    slice_left: u64,
    vruntime: u64,
    delta: u64,
    prio_effective: i32,
) -> (u64, u64, u64) {
    let slice = slice_left.saturating_sub(delta);
    let vdelta = (delta as u128 * SCALE as u128 / weight_of_prio(prio_effective) as u128) as u64;
    let vrt = vruntime.saturating_add(vdelta);
    (slice, vrt, vdelta)
}

/// 片耗尽判：==0 即耗尽。饱和递减保证 slice_left 单调不增（只减不增
/// 负片），==0 = 本片/本期预算用完。
#[inline]
pub fn slice_exhausted(slice_left: u64) -> bool {
    slice_left == 0
}

/// fair 任务片耗尽后的 VD 重臂资格：非 deadline（prio < PRIO_DEADLINE）
/// 才重臂。deadline 任务的手动绝对截止期（1.1/S5）不碰。
#[inline]
pub fn fair_rearm_vd(prio_effective: i32, is_deadline: bool) -> bool {
    !is_deadline && prio_effective < PRIO_DEADLINE
}

/// deadline 带优先级（≥ 此值 = deadline 任务）。与 sched 侧
/// `class::PRIO_DEADLINE` 同值单源（本面自带一份供判据用；sched 侧
/// 常量 re-export 本面后即零第二实现）。
pub const PRIO_DEADLINE: i32 = 1000;

/// deadline 运行时预算状态（值域快照——`Task` 相关字段的纯投影）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlineState {
    /// 绝对截止期（CNTVCT cycles）。
    pub vdeadline: u64,
    /// 剩余预算（cycles）。
    pub slice_left: u64,
    /// 周期（duration_hint_ns 存的 period_cycles）。
    pub period: u64,
    /// 预算尺寸（preempt_threshold——0 = 未设预算面）。
    pub budget: i32,
}

/// 周期到判：`now ≥ vdeadline` 且 `period > 0`（0 周期 = 无重臂面，
/// 永不周期到——lazy_unthrottle 的 duration_hint_ns > 0 门同款）。
#[inline]
pub fn period_due(now: u64, st: &DeadlineState) -> bool {
    now >= st.vdeadline && st.period > 0
}

/// 重臂 + replenish 一步：返回新 `(vdeadline, slice_left)`。
///
/// - vdeadline += period（饱和——u64 顶饱和即停，防 wrap 回远古：
///   wrap 后 now ≥ vdeadline 恒真 = 假周期到死循环）。
/// - slice_left = budget（budget ≤ 0 时保持现值——「未设预算面」的
///   deadline 任务不被 0 预算踩死，lazy_unthrottle 的 `if b > 0` 门
///   同款）。
#[inline]
pub fn replenish(now: u64, st: &DeadlineState) -> (u64, u64) {
    let _ = now;
    let vd = st.vdeadline.saturating_add(st.period);
    let sl = if st.budget > 0 {
        st.budget as u64
    } else {
        st.slice_left
    };
    (vd, sl)
}

/// S5-c 预算执法准入（throttle 置位三与门）：
/// deadline + 预算已设（preempt_threshold>0）+ 片耗尽 + 未在限流
/// （throttled 位清）。任一不满足 = 不准入。
#[inline]
pub fn throttle_admissible(
    prio_effective: i32,
    preempt_threshold: i32,
    slice_left: u64,
    throttled: bool,
) -> bool {
    prio_effective >= PRIO_DEADLINE
        && preempt_threshold > 0
        && slice_exhausted(slice_left)
        && !throttled
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表驱动钉板：weight_of_prio 全域。
    #[test]
    fn weight_domain() {
        // 基线：负值（PI 深链假想态）与 0 都取纯基线 1024。
        assert_eq!(weight_of_prio(i32::MIN), SCALE);
        assert_eq!(weight_of_prio(-1), SCALE);
        assert_eq!(weight_of_prio(0), SCALE);
        // 每级 +64。
        assert_eq!(weight_of_prio(1), SCALE + 64);
        assert_eq!(weight_of_prio(16), SCALE + 1024);
        // 封顶：boost ≥ (1<<20 - 1024)/64 = 16368 级起恒 1<<20。
        let cap_prio = ((WEIGHT_CAP - SCALE) / WEIGHT_BOOST_UNIT) as i32;
        assert_eq!(weight_of_prio(cap_prio), WEIGHT_CAP);
        assert_eq!(weight_of_prio(cap_prio + 1), WEIGHT_CAP);
        assert_eq!(weight_of_prio(i32::MAX), WEIGHT_CAP);
        // 单调不降全域抽查。
        let mut p = i32::MIN;
        while p < i32::MAX - 100 {
            assert!(weight_of_prio(p) <= weight_of_prio(p + 100));
            p += 101;
        }
    }

    /// 表驱动钉板：slice_virtual 换算律。
    #[test]
    fn slice_virtual_law() {
        // 基线等权：grant 原样过（weight=SCALE 时商 = grant）。
        assert_eq!(slice_virtual(1_000_000, 0), 1_000_000);
        // boost 16 级 = 权重 2048 = 恰半：虚拟片减半。
        assert_eq!(slice_virtual(1_000_000, 16), 500_000);
        // 下限 1：小 grant + 顶权重整除归 0 → 抬 1。
        assert_eq!(slice_virtual(1, i32::MAX), 1);
        assert_eq!(slice_virtual(0, 0), 1); // 零宽片也抬 1
        assert_eq!(slice_virtual(1023, i32::MAX), 1); // 1023*1024/2^20 = 0.999 → 1
        // grant = u64::MAX 极端防御：u128 积 2^74 不溢出。
        assert_eq!(
            slice_virtual(u64::MAX, 0),
            u64::MAX
        );
        // 单调：同 grant 下 boost 越高虚拟片越短（不增）。
        let mut prev = slice_virtual(10_000_000, 0);
        for p in [1, 2, 4, 8, 16, 100, 1000, 20_000, i32::MAX] {
            let cur = slice_virtual(10_000_000, p);
            assert!(cur <= prev);
            prev = cur;
        }
    }

    /// tick 记账：饱和递减 + vruntime 累加对拍。
    #[test]
    fn tick_account_law() {
        // 常规递减。
        assert_eq!(tick_account(1000, 5000, 300, 0), (700, 5000 + 300, 300));
        // 恰耗尽边界。
        assert_eq!(tick_account(300, 5000, 300, 0), (0, 5000 + 300, 300));
        // 超耗尽 → 饱和 0（不出负片）。
        assert_eq!(tick_account(100, 5000, u64::MAX, 0), (0, _huge_vdelta(), _huge_vdelta()));
        // boost 任务 vdelta 缩权：16 级 = 半权。
        assert_eq!(tick_account(1000, 0, 1000, 16), (0, 500, 500));
        // vruntime 顶饱和防御。
        assert_eq!(
            tick_account(10, u64::MAX, 10, 0),
            (0, u64::MAX, 10)
        );
        // delta=0 零推进（idle tick 防御面）。
        assert_eq!(tick_account(1000, 5000, 0, 0), (1000, 5000, 0));
    }

    fn _huge_vdelta() -> u64 {
        (u64::MAX as u128 * SCALE as u128 / weight_of_prio(0) as u128) as u64
    }

    /// 耗尽判。
    #[test]
    fn exhausted() {
        assert!(slice_exhausted(0));
        assert!(!slice_exhausted(1));
        assert!(!slice_exhausted(u64::MAX));
    }

    /// fair VD 重臂资格。
    #[test]
    fn fair_rearm_gate() {
        assert!(fair_rearm_vd(0, false)); // 常态 fair
        assert!(fair_rearm_vd(999, false)); // 恰低于 deadline 带
        assert!(!fair_rearm_vd(1000, true)); // deadline 不重臂
        assert!(!fair_rearm_vd(1001, true));
        // 判据以 prio 为准：两参矛盾时仍按带序判。
        assert!(!fair_rearm_vd(0, true)); // prio fair 但标 deadline → 拒
        assert!(!fair_rearm_vd(1000, false)); // prio 带内未标 → 仍拒
    }

    /// deadline 周期面：period_due × replenish 状态机。
    #[test]
    fn deadline_period_cycle() {
        let st = |vd: u64, sl: u64, p: u64, b: i32| DeadlineState {
            vdeadline: vd,
            slice_left: sl,
            period: p,
            budget: b,
        };
        // 周期未到。
        assert!(!period_due(99, &st(100, 0, 10, 5)));
        // 恰到边界（≥ 含等）。
        assert!(period_due(100, &st(100, 0, 10, 5)));
        assert!(period_due(101, &st(100, 0, 10, 5)));
        // 0 周期 = 无重臂面，永不周期到。
        assert!(!period_due(u64::MAX, &st(0, 0, 0, 5)));
        // replenish：vd += period + 新预算。
        let (vd, sl) = replenish(100, &st(100, 0, 1000, 500));
        assert_eq!((vd, sl), (1100, 500));
        // budget=0：保持现值（不踩死）。
        let (vd, sl) = replenish(100, &st(100, 77, 1000, 0));
        assert_eq!((vd, sl), (1100, 77));
        // budget<0：同 0 形。
        let (vd, sl) = replenish(100, &st(100, 77, 1000, -3));
        assert_eq!((vd, sl), (1100, 77));
        // vd 顶饱和防御：不 wrap 回远古。
        let (vd, sl) = replenish(u64::MAX, &st(u64::MAX, 0, 10, 5));
        assert_eq!((vd, sl), (u64::MAX, 5));
        // 周期链：连续 replenish 3 期无漂移。
        let mut s = st(100, 500, 100, 500);
        for k in 1..=3u64 {
            let (vd, sl) = replenish(0, &s);
            s = st(vd, sl, s.period, s.budget);
            assert_eq!((s.vdeadline, s.slice_left), (100 + 100 * k, 500));
        }
    }

    /// S5-c throttle 准入三与门。
    #[test]
    fn throttle_gate() {
        // 全满足 → 准入。
        assert!(throttle_admissible(1000, 100, 0, false));
        // 非 deadline → 拒。
        assert!(!throttle_admissible(999, 100, 0, false));
        // 预算未设 → 拒。
        assert!(!throttle_admissible(1000, 0, 0, false));
        assert!(!throttle_admissible(1000, -1, 0, false));
        // 片未耗尽 → 拒。
        assert!(!throttle_admissible(1000, 100, 1, false));
        // 已限流 → 拒（幂等防重复置位）。
        assert!(!throttle_admissible(1000, 100, 0, true));
    }
}
