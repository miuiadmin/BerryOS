//! oneshot 收紧律 host 钉板（忙核 arm-to-nearest-deadline）。
//!
//! 律单源 = `sched/src/oneshot.rs`（sched crate 本体 no_std + HAL 依赖不编
//! host——本面板经 `#[path]` mod 同源编译，一事实一归属；镜像 W3-Q2 起
//! 「face crate 单源」纪律的窗内变体：本刀律小、不立新 crate）。
//!
//! 跑法（justfile 配方 `test-oneshot` 同源）：
//!
//! ```sh
//! rustc --test sched/host/oneshot_law_test.rs -o target-cargo/oneshot_law_test \
//!   && target-cargo/oneshot_law_test
//! ```
// 律单源 = ../src/oneshot.rs：以 #[path] mod 挂入（include! 会把模块级 `//!`
// 内文档注释错置到非 crate 根——E0753；mod 形态下内文档注释合法保留）。
#[path = "../src/oneshot.rs"]
mod oneshot;

use oneshot::tighten_deadline;

/// 无到期 → 不收紧：现臂保持 = 周期 tick 兜底（忙核每 ≤1 slice 必有 tick）。
#[test]
fn t1_no_deadline_keep_periodic() {
    assert_eq!(tighten_deadline(None, 1_000), None);
    assert_eq!(tighten_deadline(None, u64::MAX), None);
}

/// 最近到期早于现臂 → 收紧到该截止（oneshot 精确唤醒的本体）。
#[test]
fn t2_earlier_deadline_tightens() {
    assert_eq!(tighten_deadline(Some(500), 1_000), Some(500));
    // 只差 1 拍也收（严格早即收——唤醒粒度塌缩靠的就是不取整）。
    assert_eq!(tighten_deadline(Some(999), 1_000), Some(999));
}

/// 同值幂等：恰等于现臂不重臂（省一次系统寄存器写，语义不变）。
#[test]
fn t3_equal_keeps() {
    assert_eq!(tighten_deadline(Some(1_000), 1_000), None);
    assert_eq!(tighten_deadline(Some(u64::MAX), u64::MAX), None);
}

/// 永不推迟（S-6 单调律同向）：晚于现臂的截止不触发重编——片终点/更早
/// 遗留截止仍是醒点，slice 硬底不丢（「deadline 跑步机饿死 tick」防线）。
#[test]
fn t4_later_never_pushes() {
    assert_eq!(tighten_deadline(Some(1_001), 1_000), None);
    assert_eq!(tighten_deadline(Some(u64::MAX), 1_000), None);
}

/// 过期头合法（park 先例语义）：d ≤ now < armed 时收紧到过期值——
/// arm_at compare 立即成立即醒、回环 expire_timeouts 承接；block 臂上
/// 「睡一个已过去的时间」即时醒（与 idle/park 路径同口径）。
#[test]
fn t5_expired_head_tightens_immediately() {
    let now = 5_000u64;
    let armed = now + 6_250_000; // 满片终点 @62.5MHz 口径
    assert_eq!(tighten_deadline(Some(now), armed), Some(now));
    assert_eq!(tighten_deadline(Some(0), armed), Some(0));
}

/// 极值域：armed = u64::MAX（深 park mask 残留等防御面）时除 MAX 外全收；
/// armed = 0（复位态）时无可收紧者——恒 None。
#[test]
fn t6_extremes() {
    assert_eq!(tighten_deadline(Some(u64::MAX - 1), u64::MAX), Some(u64::MAX - 1));
    assert_eq!(tighten_deadline(Some(0), u64::MAX), Some(0));
    assert_eq!(tighten_deadline(Some(0), 0), None);
    assert_eq!(tighten_deadline(None, 0), None);
}

/// 单调律全域抽查（确定性伪随机——同 seed 可复现）：Some 结果必严格小于
/// 现臂（只在更早方向移动），None 时最近截止必 ≥ 现臂——两分支合取即律
/// 的完备刻画。
#[test]
fn t7_monotonic_never_at_or_after_armed() {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15; // 黄金比例常数起手（repo 散列先例）
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..10_000 {
        let armed = next();
        let nearest = next() % 8; // 混入 None 与重值域
        let n = if nearest == 7 { None } else { Some(next()) };
        match tighten_deadline(n, armed) {
            Some(d) => assert!(d < armed, "收紧结果必须严格早于现臂"),
            None => {
                if let Some(d) = n {
                    assert!(d >= armed, "不收紧时最近截止必须不早于现臂");
                }
            }
        }
    }
}
