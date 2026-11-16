//! berry-bandface —— band 预算包络数学纯函数面（W3-Q19）。
//!
//! **动机**：v1.2 `调点调度-v1.2.md` §7 / ADR #42 第二段的带级预算
//! 包络（band）此前只活在 `sched/src/band.rs`——sched 依赖 HAL
//! global_asm，只 aarch64-unknown-none 可编，host 测不可达，ktest 只覆盖
//! boot 相。首越线单赢家律（`prev < quota ≤ prev + cycles`）、翻期追赶环
//! （饱和步进防死循环）、CAS 防双翻判序、配置三态的回归面需要一张 host
//! 可测的**钉板**：本 crate 把这些数学按值域抽成纯函数 + 表驱动测试，
//! `sched` 侧消费同一函数（零第二实现——抽离即唯一事实源，非镜像；
//! wqmatch/emface/dtbface 同款纪律）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`BAND_SLOTS`]/[`BandBudget`]/[`BandBudget::OFF`]：槽形与关态槽
//!   单源（自 sched/src/band.rs 迁此，sched `pub use` re-export）。
//! - [`slot_ok`]:带号界门（`band < BAND_SLOTS`）。
//! - [`armed`]：执法门——quota/period **均正**才武装（任一 0 = 关）。
//! - [`configure_slot`]:配置三态判——越界拒 / OFF 态（清账 + 哨兵
//!   deadline 0）/ 武装态（首期 deadline = `now + period` 饱和加）。
//! - [`consumed_crossed`]:微记账首越线单赢家律——`prev < quota ≤
//!   prev + cycles` 恰一次 true（已在线上再记不重报；多核同挤也恰一
//!   个 true——fetch_add 返回值单赢家语义的纯面投影）。
//! - [`deadline_next`]:翻期追赶环纯投影——`next > now` 的首个新期
//!   （饱和步进出 u64::MAX；环防死循环律同 sched 侧 `while next <= now`）。
//! - [`period_due`]：翻期资格判——`now < cur` 未到期 false / 到期才
//!   进 CAS 环。
//! - [`band_member`]：带成员豁免判——intent 即带键（§11）且 fair 任务
//!   （`prio_effective < PRIO_DEADLINE`——deadline 任务豁免，RT/deadline
//!   屏蔽不变量 §10）。
//!
//! 与冻结面的关系：`BANDS` static mut 槽镜像 + `CONSUMED`/`DEADLINES`
//! 原子账 + throttle/unthrottle 环扫（`&raw mut`/fetch_add/CAS 裸并发
//! 机械）留 sched（纪律不变）——本面只钉「预算怎么判/翻期怎么追」的
//! 数定律，不钉存储机械。时基（CNTVCT）以参数注入——纯面无时基依赖。
//!
//! PRIO_DEADLINE 单源在 `berry-sliceface`（W3-Q11 同款：sched
//! `class.rs` re-export 消费链不变）——本面自带一份常量供判据用，
//! 值同 `class::PRIO_DEADLINE`（1000）。

#![cfg_attr(not(test), no_std)]

// ---------------------------------------------------------------------------
// 槽形与常量（自 sched/src/band.rs 单源迁此——sched 侧 re-export 后零第二实现）
// ---------------------------------------------------------------------------

/// band 静态表槽数。带词汇 0x00–0x05（intent 冻结编码，#58）+ 余量：
/// v1.2 §7 现役 = 0x01–0x04 ×4 带实例（0x05 MAINTENANCE 冻结不启用），
/// 0x00 = UNSPECIFIED 默认带（不设包络）。
pub const BAND_SLOTS: usize = 8;

/// deadline 任务优先级带序下界（`prio_effective ≥ PRIO_DEADLINE` =
/// deadline 任务——band 环扫豁免面）。值单源同 `berry-sliceface::
/// PRIO_DEADLINE`（sched `class.rs` re-export 链不变）。
pub const PRIO_DEADLINE: i32 = 1000;

/// 每带预算槽（CBS 同形语义，同 S5-c：每期至多 quota CPU 时长，超了等
/// 下期——整数周期数记账）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BandBudget {
    /// 每期预算（CNTVCT 周期数；0 = 该带不设包络）。
    pub quota: u64,
    /// 周期长度（CNTVCT 周期数；0 = 该带不设包络）。
    pub period: u64,
    /// 本期已耗（整数微记账；快照镜像——真账在 sched 侧 CONSUMED 原子
    /// 数组，state 合并点回填）。
    pub consumed: u64,
    /// 本期截止时刻（CNTVCT 周期数；事件驱动——无周期 tick。快照镜像
    /// ——真账在 sched 侧 DEADLINES 原子数组，翻期 CAS 面）。
    pub deadline: u64,
    /// 本期预算是否已耗尽（consume 首越线置位；period_check 翻期清）。
    pub throttled: bool,
}

impl BandBudget {
    /// 关闭态槽（quota/period 0 = 不设包络——表初值）。
    pub const OFF: BandBudget = BandBudget {
        quota: 0,
        period: 0,
        consumed: 0,
        deadline: 0,
        throttled: false,
    };
}

/// 带号界门（`slot(band)` 的 Option 面）：`band < BAND_SLOTS` 才在表内。
#[inline]
pub fn slot_ok(band: u8) -> bool {
    (band as usize) < BAND_SLOTS
}

/// 该带是否武装（`band_on` 判据单源）：quota/period 均正且带在表内。
/// 槽形参入（纯面无存储）——sched 侧传 `&BANDS[i]` 快照。
#[inline]
pub fn armed(b: &BandBudget) -> bool {
    b.quota != 0 && b.period != 0
}

// ---------------------------------------------------------------------------
// 配置三态（configure 判序单源）
// ---------------------------------------------------------------------------

/// [`configure_slot`] 的三态出参（configure 语义钉板）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigOut {
    /// 带越界 / quota/period 任一 0 → 关态：槽 = OFF、原子账清零、
    /// deadline 哨兵 0。`deadline` 字段 = 须写回 DEADLINES 的值（0）。
    Off,
    /// 武装：首期 deadline = `now + period`（饱和加），清账开新期。
    Armed { deadline: u64 },
}

/// 配置某带包络的判定芯（`configure` 判序单源）：越界 → [`ConfigOut::Off`]
/// （sched 侧 early-return false 不写面——这里显式分态）；quota/period
/// 任一 0 → Off（清账翻期义）；均正 → Armed（首期 deadline = now +
/// period 饱和加）。
///
/// 注意：越界（None）与关态（Off）在 sched 侧返回值同为 false——差异在
/// 写入面：越界零写入（不碰表/原子账），关态写 OFF + 清账。返回 bool 由
/// Armed 判（sched 壳 `matches!(out, ConfigOut::Armed { .. })`）。
#[inline]
pub fn configure_slot(band: u8, quota: u64, period: u64, now: u64) -> Option<ConfigOut> {
    if !slot_ok(band) {
        return None; // 越界：sched 侧零写入 false（与 Off 区分——不碰表）
    }
    if quota == 0 || period == 0 {
        return Some(ConfigOut::Off);
    }
    Some(ConfigOut::Armed { deadline: now.saturating_add(period) })
}

// ---------------------------------------------------------------------------
// 微记账首越线（consume 判定律单源）
// ---------------------------------------------------------------------------

/// 微记账一步的判定芯（`consume` 单赢家律单源）：返回 `Some(crossed)`
/// = 已记账后判定（crossed = 首越线）；`None` = 不记账（带越界 / 包络
/// 关——sched 侧 early-return false）。
///
/// 首越线律：`now >= quota && prev < quota`——prev 是 fetch_add 返回的
/// 旧值，now = prev + cycles 饱和加。已在线上（prev ≥ quota）再记不重
/// 报（一次性信号）；多核同挤越线 fetch_add 返回值全局恰一次落在
/// `prev < quota` 侧——单赢家。u64 环绕 = 单期累计 2^64 CNTVCT 周期
/// （@1GHz ≈ 584 年）——不可达，不设防。
#[inline]
pub fn consumed_crossed(band: u8, b: &BandBudget, cycles: u64, prev: u64) -> Option<bool> {
    if !slot_ok(band) || !armed(b) {
        return None;
    }
    let now = prev.saturating_add(cycles);
    Some(now >= b.quota && prev < b.quota)
}

// ---------------------------------------------------------------------------
// 翻期数学（period_check 判序单源）
// ---------------------------------------------------------------------------

/// 翻期追赶环纯投影（sched 侧 `while next <= now` 环的数学等价）：
/// 返回 `next > now` 的**首个**未来期（自 `cur` 起 + period 饱和步进；
/// `cur` 钉 u64::MAX 时出环返 MAX——防死循环律同 sched）。
///
/// 长眠一次跨多期：一步追到（deadline 永只指向未来首期——旧期不逐一
/// 补翻，账面只清一次）。
#[inline]
pub fn deadline_next(cur: u64, period: u64, now: u64) -> u64 {
    let mut next = cur;
    while next <= now {
        let step = next.saturating_add(period);
        if step == next {
            break; // 钉 u64::MAX：饱和步进不动 = 出环（防死循环）
        }
        next = step;
    }
    next
}

/// 翻期资格判（period_check CAS 环的入环门单源）：`now < cur` 未到期
/// false（含他核刚翻过的未来期）；到期（now ≥ cur）才进追赶环 + CAS。
/// 越界 / 包络关 → None（sched 侧 early-return false）。
#[inline]
pub fn period_due(band: u8, b: &BandBudget, cur: u64, now: u64) -> Option<bool> {
    if !slot_ok(band) || !armed(b) {
        return None;
    }
    Some(now >= cur)
}

// ---------------------------------------------------------------------------
// 带成员豁免判（band_member 单源——throttle/unthrottle 环扫共享）
// ---------------------------------------------------------------------------

/// 带成员判定（`band_member` 单源）：intent 即带键（§11，读 effective
/// ——§3.6 一律看 effective）；deadline 任务豁免（RT/deadline 屏蔽不变
/// 量 §10——其 THROTTLE 位归 S5-c 逐任务执法独占）。
#[inline]
pub fn band_member(intent_effective: u8, prio_effective: i32, band: u8) -> bool {
    intent_effective == band && prio_effective < PRIO_DEADLINE
}

// ---------------------------------------------------------------------------
// 表驱动钉板（host 测——sched 侧同款判据此前 host 不可达）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 配置三态全表：越界 / 关态 / 武装 + 首期 deadline 饱和加（ktest
    /// BD0 判序的 host 钉板）。
    #[test]
    fn configure_three_states() {
        // 越界：恰上限即越界（8 槽表 0..7 在册）。
        assert_eq!(configure_slot(BAND_SLOTS as u8, 1000, 10_000, 100_000), None);
        assert_eq!(configure_slot(u8::MAX, 1, 1, 0), None);
        // 在册界内 + 均正 → 武装（首期 deadline = now + period）。
        assert_eq!(
            configure_slot(1, 1000, 10_000, 100_000),
            Some(ConfigOut::Armed { deadline: 110_000 })
        );
        // 槽 0（UNSPECIFIED 默认带）界内在册——值域上可配（v1.2 现役
        // 0x01–0x04，但槽形不拒 0——sched 侧同款界门纯值域判）。
        assert_eq!(
            configure_slot(0, 1, 1, 0),
            Some(ConfigOut::Armed { deadline: 1 })
        );
        // 槽 7（末槽）在册。
        assert_eq!(
            configure_slot(7, 5, 7, 10),
            Some(ConfigOut::Armed { deadline: 17 })
        );
        // 关态：quota=0 / period=0 / 均 0。
        assert_eq!(configure_slot(1, 0, 10_000, 100_000), Some(ConfigOut::Off));
        assert_eq!(configure_slot(1, 1000, 0, 100_000), Some(ConfigOut::Off));
        assert_eq!(configure_slot(1, 0, 0, 0), Some(ConfigOut::Off));
        // 首期 deadline 饱和加：now + period 溢出钳 u64::MAX。
        assert_eq!(
            configure_slot(1, 1, 10, u64::MAX),
            Some(ConfigOut::Armed { deadline: u64::MAX })
        );
        assert_eq!(
            configure_slot(1, 1, u64::MAX, 1),
            Some(ConfigOut::Armed { deadline: u64::MAX })
        );
    }

    /// 执法门（band_on 判据）：quota/period 均正才武装。
    #[test]
    fn armed_gate() {
        assert!(!armed(&BandBudget::OFF));
        assert!(!armed(&BandBudget { quota: 1, period: 0, ..BandBudget::OFF }));
        assert!(!armed(&BandBudget { quota: 0, period: 1, ..BandBudget::OFF }));
        assert!(armed(&BandBudget { quota: 1, period: 1, ..BandBudget::OFF }));
        // 槽形带账面字段（consumed/deadline/throttled）不参与执法门。
        assert!(armed(&BandBudget {
            quota: 1000,
            period: 10_000,
            consumed: 9999,
            deadline: 7,
            throttled: true,
        }));
    }

    /// 首越线单赢家律全表（ktest BD1/BD2 数值的纯面投影 + 边界）：
    /// prev < quota ≤ prev+cycles 恰一次 true。
    #[test]
    fn consume_single_winner() {
        let b = BandBudget { quota: 1000, period: 10_000, ..BandBudget::OFF };
        // 未越线：累计不足。
        assert_eq!(consumed_crossed(1, &b, 400, 0), Some(false));
        assert_eq!(consumed_crossed(1, &b, 500, 400), Some(false)); // 900/1000
        // 首越线：prev=900 + 200 ≥ 1000 且 prev < 1000 → true。
        assert_eq!(consumed_crossed(1, &b, 200, 900), Some(true));
        // 已在线上再记不重报（一次性信号）。
        assert_eq!(consumed_crossed(1, &b, 5, 1100), Some(false));
        assert_eq!(consumed_crossed(1, &b, 1, u64::MAX), Some(false));
        // 恰在线上（== quota）= 已越（≥ 含等）：prev=999 +1。
        assert_eq!(consumed_crossed(1, &b, 1, 999), Some(true));
        // prev 恰 quota（已在线）不报——严格 prev < quota。
        assert_eq!(consumed_crossed(1, &b, 1, 1000), Some(false));
        // 一步跨双倍线：prev=0 + 2000 → 首越（不因超卖重报）。
        assert_eq!(consumed_crossed(1, &b, 2000, 0), Some(true));
        // 零增量不越线（prev < quota 时 now==prev 仍 < quota；已在线不动）。
        assert_eq!(consumed_crossed(1, &b, 0, 500), Some(false));
        // 饱和加边界：prev=u64::MAX-5 + 10 → 钳 MAX（已在线，不报）。
        assert_eq!(consumed_crossed(1, &b, 10, u64::MAX - 5), Some(false));
        // quota=1 极小包络：prev=0 + 1 即首越。
        let tiny = BandBudget { quota: 1, period: 1, ..BandBudget::OFF };
        assert_eq!(consumed_crossed(1, &tiny, 1, 0), Some(true));
        assert_eq!(consumed_crossed(1, &tiny, 0, 0), Some(false));
        // 关态/越界 → None 不记账。
        assert_eq!(consumed_crossed(BAND_SLOTS as u8, &b, 1, 0), None);
        assert_eq!(consumed_crossed(1, &BandBudget::OFF, 999, 0), None);
        assert_eq!(consumed_crossed(u8::MAX, &BandBudget::OFF, 1, 0), None);
        // 严格性全域抽查：crossed(q,c,prev) == (prev < q ≤ prev+c)。
        for prev in [0u64, 1, 499, 500, 999, 1000, 1001, 1999, 2000] {
            for cycles in [0u64, 1, 500, 1000, 1001, 5000] {
                let want = prev < 1000 && prev.saturating_add(cycles) >= 1000;
                assert_eq!(
                    consumed_crossed(1, &b, cycles, prev),
                    Some(want),
                    "prev={prev} cycles={cycles}"
                );
            }
        }
    }

    /// 翻期追赶环：一步跨多期 / 恰到期 / 未到期 / 饱和钉 MAX 防死循环。
    #[test]
    fn deadline_catchup() {
        // 未到期不动（next = cur > now：环零圈）。
        assert_eq!(deadline_next(110_000, 10_000, 109_999), 110_000);
        assert_eq!(deadline_next(110_000, 10_000, 0), 110_000);
        // 恰到期（now == cur）：翻一期。
        assert_eq!(deadline_next(110_000, 10_000, 110_000), 120_000);
        // 长眠跨两期：now = 首期 + 2×period + 500 → 一步追到第三期
        //（ktest BD3 锚：NOW+2×PERIOD+500 → NOW+3×PERIOD）。
        assert_eq!(deadline_next(110_000, 10_000, 130_500), 140_000);
        // 跨 5 期。
        assert_eq!(deadline_next(110_000, 10_000, 160_000), 170_000);
        // 极端：now = u64::MAX - 1、period 巨大 → 饱和钳 MAX。
        assert_eq!(deadline_next(u64::MAX - 5, 10, u64::MAX - 1), u64::MAX);
        // cur 钉 MAX：环饱和步进不动 = 出环返 MAX（防死循环律）。
        assert_eq!(deadline_next(u64::MAX, 10_000, u64::MAX), u64::MAX);
        // period=1 密步进不失控：now - cur 距离即圈数，值域可达。
        assert_eq!(deadline_next(1000, 1, 1999), 2000);
        // 注：period=1 × now=u64::MAX 型病理输入（2^64 圈）不在钉板——
        // sched 侧同环同律，真机不可达（584 年长眠），钉它 = 钉死循环。
    }

    /// 翻期资格门：now < cur 未到期 / now ≥ cur 到期 / 越界关态 None。
    #[test]
    fn period_due_gate() {
        let b = BandBudget { quota: 1000, period: 10_000, ..BandBudget::OFF };
        assert_eq!(period_due(1, &b, 110_000, 109_999), Some(false));
        assert_eq!(period_due(1, &b, 110_000, 110_000), Some(true)); // ≥ 含等
        assert_eq!(period_due(1, &b, 110_000, u64::MAX), Some(true));
        assert_eq!(period_due(1, &b, 0, 0), Some(true));
        assert_eq!(period_due(BAND_SLOTS as u8, &b, 0, u64::MAX), None);
        assert_eq!(period_due(1, &BandBudget::OFF, 0, u64::MAX), None);
    }

    /// 带成员豁免判：intent 匹配 + fair 任务；deadline/异带全拒。
    #[test]
    fn membership_exempt() {
        const PRIO_FAIR: i32 = 0;
        const PRIO_DEADLINE_TASK: i32 = PRIO_DEADLINE;
        const PRIO_RT_ABOVE: i32 = PRIO_DEADLINE + 1;
        // 同带 fair：受理。
        assert!(band_member(0x02, PRIO_FAIR, 0x02));
        assert!(band_member(0x04, 500, 0x04)); // 带内高序 fair（< 1000）
        assert!(band_member(0x02, PRIO_DEADLINE - 1, 0x02)); // 恰界下 fair
        // deadline 任务豁免（≥ PRIO_DEADLINE）。
        assert!(!band_member(0x02, PRIO_DEADLINE_TASK, 0x02));
        assert!(!band_member(0x02, PRIO_RT_ABOVE, 0x02));
        // 异带全拒（intent 即带键）。
        assert!(!band_member(0x01, PRIO_FAIR, 0x02));
        assert!(!band_member(0x00, PRIO_FAIR, 0x02));
        // 意图编码域（#58 冻结六档 0x00–0x05）内全档抽查：异带拒。
        for i in [0u8, 1, 2, 3, 4, 5] {
            assert!(band_member(i, PRIO_FAIR, i), "同带 fair 须受理 i={i}");
            assert!(!band_member(i, PRIO_FAIR, (i + 1) % 6), "异带拒 i={i}");
            assert!(!band_member(i, PRIO_DEADLINE, i), "deadline 豁免 i={i}");
        }
    }
}
