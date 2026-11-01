//! GUI-3 §8 resample 行：resample_to_target 常量边界 + 插值/外推值级。
//!
//! - 常量单源对拍：边界样本用 `berry_guiproto::RESAMPLE_*` 现算——常量
//!   变更时测试随动（不在测试里长第二套数值）。
//! - 拒面：<2 采样 / 非严格递增（同 ts·回退）/ 跨度 < MIN 或 > MAX /
//!   target < 首（后向外推不提供）。
//! - 括点插值：区间内线性、命中采样点取原值、ts 报 target。
//! - 前向外推：斜率外推 + 视界帽 min(跨度/2, MAX_PREDICTION)——超帽
//!   位置停在帽沿、ts 诚实报 target；负方向（回退轴）同式。
//! - target_from_expected = expected − LATENCY（饱和减）。
//!
//! 确定性：纯函数、定值样本，无随机无时钟。

use berry_ui::resample::{target_from_expected, resample_to_target, Sample};
use berry_guiproto::{
    RESAMPLE_LATENCY_MS, RESAMPLE_MAX_DELTA_MS, RESAMPLE_MAX_PREDICTION_MS, RESAMPLE_MIN_DELTA_MS,
};

const MS: u64 = 1_000_000;
/// 样本时刻基（远离 0——后向外推拒面测试要 target < first 可造）。
const T0: u64 = 1_000_000_000;

fn s(ts: u64, x: i64, y: i64) -> Sample {
    Sample { ts_ns: ts, x, y }
}

#[test]
fn span_boundaries_single_sourced_from_guiproto() {
    // 跨度恰 = MIN_DELTA → 收（拒面是严格小于）
    let h = [s(T0, 0, 0), s(T0 + RESAMPLE_MIN_DELTA_MS * MS, 10, 20)];
    let mid = T0 + RESAMPLE_MIN_DELTA_MS * MS / 2;
    assert!(resample_to_target(&h, mid).is_some(), "span==MIN 可用");

    // 跨度 MIN−1ns → 拒（信息量不足）
    let h = [s(T0, 0, 0), s(T0 + RESAMPLE_MIN_DELTA_MS * MS - 1, 10, 20)];
    assert!(resample_to_target(&h, T0).is_none(), "span<MIN 拒");

    // 跨度恰 = MAX_DELTA → 收
    let h = [s(T0, 0, 0), s(T0 + RESAMPLE_MAX_DELTA_MS * MS, 10, 20)];
    let mid = T0 + RESAMPLE_MAX_DELTA_MS * MS / 2;
    assert!(resample_to_target(&h, mid).is_some(), "span==MAX 可用");

    // 跨度 MAX+1ns → 拒（太陈旧）
    let h = [s(T0, 0, 0), s(T0 + RESAMPLE_MAX_DELTA_MS * MS + 1, 10, 20)];
    assert!(resample_to_target(&h, T0 + RESAMPLE_MAX_DELTA_MS * MS).is_none(), "span>MAX 拒");
}

#[test]
fn degenerate_and_non_monotonic_refused() {
    // 少于 2 采样：空/单样本退化 → None
    assert!(resample_to_target(&[], T0).is_none());
    assert!(resample_to_target(&[s(T0, 1, 1)], T0).is_none());

    // 非严格递增全拒：首尾同 ts / 区间重复 / 回退
    let dup_ends = [s(T0, 0, 0), s(T0 + 10 * MS, 5, 5), s(T0 + 10 * MS, 9, 9)];
    assert!(resample_to_target(&dup_ends, T0 + 5 * MS).is_none(), "尾双同 ts 拒");
    let dup_mid = [s(T0, 0, 0), s(T0 + 5 * MS, 5, 5), s(T0 + 5 * MS, 6, 6), s(T0 + 9 * MS, 9, 9)];
    assert!(resample_to_target(&dup_mid, T0 + 6 * MS).is_none(), "区间重复 ts 拒");
    let back = [s(T0 + 10 * MS, 0, 0), s(T0, 9, 9)];
    assert!(resample_to_target(&back, T0).is_none(), "回退 ts 拒");
}

#[test]
fn backward_target_refused() {
    // target < 首：后向外推不提供
    let h = [s(T0, 0, 0), s(T0 + 10 * MS, 100, 200)];
    assert!(resample_to_target(&h, T0 - 1).is_none());
    assert!(resample_to_target(&h, 0).is_none(), "target=0 极小值同拒");
}

#[test]
fn bracket_interpolation_exact_values() {
    // (0ms→0, 10ms→100)：5ms 中点 = 50；ts 报 target
    let h = [s(T0, 0, 0), s(T0 + 10 * MS, 100, 40)];
    let out = resample_to_target(&h, T0 + 5 * MS).unwrap();
    assert_eq!(out, Sample { ts_ns: T0 + 5 * MS, x: 50, y: 20 });

    // 命中采样点：取原值（lerp 退化）
    let out = resample_to_target(&h, T0 + 10 * MS).unwrap();
    assert_eq!(out, Sample { ts_ns: T0 + 10 * MS, x: 100, y: 40 });
    // target == 首：x=from
    let out = resample_to_target(&h, T0).unwrap();
    assert_eq!(out, Sample { ts_ns: T0, x: 0, y: 0 });

    // 三采样跨段：4ms 在 (2ms→20, 6ms→60) 段内插 40
    let h = [
        s(T0, 0, 0),
        s(T0 + 2 * MS, 20, 0),
        s(T0 + 6 * MS, 60, 0),
    ];
    let out = resample_to_target(&h, T0 + 4 * MS).unwrap();
    assert_eq!(out, Sample { ts_ns: T0 + 4 * MS, x: 40, y: 0 });
}

#[test]
fn forward_extrapolation_capped_by_half_span() {
    // 跨度 10ms → 帽 = min(5ms, 8ms) = 5ms；斜率 10/ms
    let h = [s(T0, 0, 0), s(T0 + 10 * MS, 100, 0)];
    // 帽内外推（+2ms）：100 + 10×2 = 120
    let out = resample_to_target(&h, T0 + 12 * MS).unwrap();
    assert_eq!(out, Sample { ts_ns: T0 + 12 * MS, x: 120, y: 0 });
    // 帽沿（+5ms）：100 + 50 = 150
    let out = resample_to_target(&h, T0 + 15 * MS).unwrap();
    assert_eq!(out, Sample { ts_ns: T0 + 15 * MS, x: 150, y: 0 });
    // 超帽（+8ms > 帽 5ms）：位置停在帽沿 150、ts 诚实报 target
    let out = resample_to_target(&h, T0 + 18 * MS).unwrap();
    assert_eq!(
        out,
        Sample { ts_ns: T0 + 18 * MS, x: 150, y: 0 },
        "位置截停在帽沿，ts 诚实报 target"
    );
}

#[test]
fn forward_extrapolation_capped_by_max_prediction() {
    // 跨度 20ms → 帽 = min(10ms, 8ms) = 8ms；斜率 10/ms
    let h = [s(T0, 0, 0), s(T0 + 20 * MS, 200, 0)];
    let out = resample_to_target(&h, T0 + 28 * MS).unwrap();
    assert_eq!(out, Sample { ts_ns: T0 + 28 * MS, x: 280, y: 0 }, "恰在 8ms 帽沿");
    let out = resample_to_target(&h, T0 + 35 * MS).unwrap();
    assert_eq!(
        out,
        Sample { ts_ns: T0 + 35 * MS, x: 280, y: 0 },
        "超帽位置停 MAX_PREDICTION 帽沿"
    );
}

#[test]
fn negative_direction_extrapolation() {
    // 回退轴：斜率 −10/ms；帽内 3ms → −30（signed 外推同式）
    let h = [s(T0, 100, 0), s(T0 + 10 * MS, 0, 0)];
    let out = resample_to_target(&h, T0 + 13 * MS).unwrap();
    assert_eq!(out, Sample { ts_ns: T0 + 13 * MS, x: -30, y: 0 });
}

#[test]
fn target_from_expected_subtracts_latency() {
    // expected − LATENCY（饱和减；常量 guiproto 单源）
    assert_eq!(target_from_expected(10 * MS), 10 * MS - RESAMPLE_LATENCY_MS * MS);
    assert_eq!(target_from_expected(5 * MS), 0, "恰等 LATENCY → 0");
    assert_eq!(target_from_expected(3 * MS), 0, "小于 LATENCY 饱和 0 不回绕");
    assert_eq!(target_from_expected(0), 0);
    // 单源对拍：当前冻结值 5ms（漂移即测试随动失败）
    assert_eq!(RESAMPLE_LATENCY_MS, 5);
    assert_eq!(RESAMPLE_MIN_DELTA_MS, 2);
    assert_eq!(RESAMPLE_MAX_DELTA_MS, 20);
    assert_eq!(RESAMPLE_MAX_PREDICTION_MS, 8);
}
