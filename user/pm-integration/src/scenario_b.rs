//! 场景 B —— slack+EM 联合：六档 slack 域 → 10 定时器分桶 → 唤醒窗口 →
//! 窗口内批需求的 EM 容量检查（唤醒不超容量）。
//!
//! 数据流（调度彻底计划 CP-11「timer slack＋IntentProfile 表」与 CP-09
//! EAS 的消费者侧串联——两件的接线本体均留窗，本场景验证判定律纯面之间
//! 的数据流正确性）：
//!
//! ```text
//! PolicyTriple + intents（六档全覆盖 + 系统缺省档）
//!        ▼ resolve_slack / slack_for_intent
//! TimerReq{deadline_ns, slack_ns} × 10 ──→ coalesce ──→ Bucket{fire/window/span/members}
//!        ▼（L1/L2/L3 三律 laws_hold 对拍）
//! 每桶批需求 Σw_i（成员定时器唤醒工作量的 util 账）
//!        ▼ berry-em em_max_capacity_of / capacity_for_util / overutil_judgment
//! 唤醒不超容量：批需求 ≤ 域有效容量；超容 → overutil（EAS 让位面）
//! ```

use crate::common::{eval_tick, feed_battery, ladder3};
use berry_em::{capacity_for_util, em_max_capacity_of, overutil_judgment, QEMU_VIRT_DUAL};
use berry_power::{CapDecision, PolicyParams, PowerCore, CAP_SRC_SCHEDHINT, CAP_SRC_THERMAL};
use berry_slackface::{
    coalesce, laws_hold, policy_triple_valid, resolve_slack, slack_for_intent, PolicyTriple,
    TimerReq, GRID_RECOMMEND_NS, SLACK_BACKGROUND_NS, SLACK_INTERACTIVE_NS,
    SLACK_LATENCY_CRITICAL_NS, SLACK_MAINTENANCE_NS, SLACK_UNSPECIFIED_NS, SLACK_UTILITY_NS,
};
use berry_thermal::Engine;

/// 10 定时器输入：六档 intent 全覆盖 + 1 件系统定时器（intent 语境缺席 →
/// 三元组系统缺省兜底——Linux 线程 default/current 双值同构）。`w` = 该
/// 定时器唤醒工作量（util 账，0..=1024 尺度——与 EM 容量同尺度）。
struct Tim {
    intent: Option<u8>,
    d_ms: u64,
    w: u32,
}

fn ten_timers() -> [Tim; 10] {
    [
        Tim { intent: Some(0x02), d_ms: 95, w: 150 },  // t0 INTERACTIVE
        Tim { intent: Some(0x01), d_ms: 100, w: 100 }, // t1 LATENCY_CRITICAL
        Tim { intent: Some(0x00), d_ms: 112, w: 200 }, // t2 UNSPECIFIED
        Tim { intent: Some(0x03), d_ms: 140, w: 200 }, // t3 UTILITY
        Tim { intent: Some(0x04), d_ms: 158, w: 300 }, // t4 BACKGROUND
        Tim { intent: Some(0x05), d_ms: 200, w: 50 },  // t5 MAINTENANCE
        Tim { intent: Some(0x02), d_ms: 205, w: 150 }, // t6 INTERACTIVE
        Tim { intent: Some(0x04), d_ms: 250, w: 400 }, // t7 BACKGROUND
        Tim { intent: None, d_ms: 300, w: 100 },       // t8 系统定时器（缺省档）
        Tim { intent: Some(0x03), d_ms: 340, w: 250 }, // t9 UTILITY
    ]
}

/// 三元组（intent_default=UTILITY、系统缺省 slack = 50ms——与 UNSPECIFIED
/// 同值的保守行；t8 的 None 分支与 Some(0x00) 行在此值上重合，解析链两
/// 分支都过、数值不歧义）。
fn triple() -> PolicyTriple {
    PolicyTriple {
        slice_hz: 100,
        intent_default: 0x03,
        timer_slack_ns: SLACK_UNSPECIFIED_NS,
    }
}

fn reqs_of(timers: &[Tim]) -> Vec<TimerReq> {
    let p = triple();
    timers
        .iter()
        .map(|t| TimerReq {
            deadline_ns: t.d_ms * 1_000_000,
            slack_ns: resolve_slack(&p, t.intent).expect("词表内 intent 必解析"),
        })
        .collect()
}

/// B-1 六档聚合 + 10 定时器分桶 + 唤醒窗口：手算轨迹（贪心 + 1ms 网格
/// 对齐——ms 粒度值对齐不动点）：
/// b1{t0,t1}@100ms（LC 钉点——INT 顺延 5ms ≤ 10ms）、b2{t2,t3,t4}@162ms
/// （UNSPEC 预算 50ms 恰用满——L2/L3 双边界恰点）、b3{t5,t6}@215ms（INT
/// 预算 10ms 恰用满）、b4{t7,t8,t9}@350ms。唤醒 10→4（省 60%），三律全批
/// 成立。
#[test]
fn six_tier_ten_timer_bucketing_with_windows() {
    let p = triple();
    assert!(policy_triple_valid(&p));
    // 六档域值钉板（词表单源值——档间序 LC < INT < UNSPEC=UTILITY < BG < MAINT）。
    assert_eq!(slack_for_intent(0x01), Some(SLACK_LATENCY_CRITICAL_NS));
    assert_eq!(slack_for_intent(0x02), Some(SLACK_INTERACTIVE_NS));
    assert_eq!(slack_for_intent(0x00), Some(SLACK_UNSPECIFIED_NS));
    assert_eq!(slack_for_intent(0x03), Some(SLACK_UTILITY_NS));
    assert_eq!(slack_for_intent(0x04), Some(SLACK_BACKGROUND_NS));
    assert_eq!(slack_for_intent(0x05), Some(SLACK_MAINTENANCE_NS));
    assert!(SLACK_LATENCY_CRITICAL_NS < SLACK_INTERACTIVE_NS);
    assert!(SLACK_INTERACTIVE_NS < SLACK_UNSPECIFIED_NS);
    assert_eq!(SLACK_UNSPECIFIED_NS, SLACK_UTILITY_NS);
    assert!(SLACK_BACKGROUND_NS < SLACK_MAINTENANCE_NS);

    let timers = ten_timers();
    let reqs = reqs_of(&timers);
    // 解析链两分支：intent 可得 → 域行；None → 系统缺省（50ms）。
    assert_eq!(resolve_slack(&p, Some(0x02)), Some(SLACK_INTERACTIVE_NS));
    assert_eq!(resolve_slack(&p, None), Some(SLACK_UNSPECIFIED_NS));

    let out = coalesce(&reqs, 0, GRID_RECOMMEND_NS).expect("合法批必聚合");
    assert_eq!((out.timers_in, out.wakeups_before, out.wakeups_after), (10, 10, 4));
    assert_eq!(out.wakeups_saved_pct, 60);

    // 桶几何钉板：(成员, fire, window, span)——手算轨迹见模块注。
    let ms = 1_000_000u64;
    let want: [(Vec<usize>, u64, u64, u64); 4] = [
        (vec![0, 1], 100 * ms, 0, 5 * ms),
        (vec![2, 3, 4], 162 * ms, 4 * ms, 50 * ms),
        (vec![5, 6], 215 * ms, 10 * ms, 15 * ms),
        (vec![7, 8, 9], 350 * ms, 10 * ms, 100 * ms),
    ];
    for (b, (members, fire, window, span)) in out.buckets.iter().zip(want) {
        assert_eq!(b.members, members);
        assert_eq!(b.fire_ns, fire);
        assert_eq!(b.window_ns, window);
        assert_eq!(b.span_ns, span);
    }
    // 三律全批（L1 late-only / L2 最紧 slack 不越 / L3 防饥饿上界）——
    // b2 首成员 UNSPEC 恰用满 50ms 预算、b3 的 INT 恰用满 10ms：边界等值
    // 合法（律只禁越界）。
    assert!(laws_hold(&reqs, &out));
}

/// B-2 唤醒不超容量：每桶批需求（成员工作量 Σw）对 QEMU 双核域检查——
/// 四桶 [250, 700, 200, 750] 全部 ≤ 域容量 1024 且不触 overutil 阈（≤819），
/// 服务档位 = capacity_for_util 投影 [400, 750, 400, 750]；对抗件：t7 工作量
/// 400→800 → 该桶批需求 1150 > 1024 → 放不下且 overutil 成立（批次超容
/// → EAS 让位负载均衡的判定面）。
#[test]
fn wakeup_batches_fit_em_capacity() {
    let timers = ten_timers();
    let reqs = reqs_of(&timers);
    let out = coalesce(&reqs, 0, GRID_RECOMMEND_NS).unwrap();
    let cap = em_max_capacity_of(&QEMU_VIRT_DUAL);
    assert_eq!(cap, 1024);

    // (批需求, 服务档容量) 手算钉板。
    let want: [(u32, u32); 4] = [(250, 400), (700, 750), (200, 400), (750, 750)];
    for (b, (demand, serving)) in out.buckets.iter().zip(want) {
        let d: u32 = b.members.iter().map(|&i| timers[i].w).sum();
        assert_eq!(d, demand);
        assert!(d <= cap, "唤醒不超容量：批 {demand} ≤ 域 {cap}");
        assert!(!overutil_judgment(d, cap), "批 {demand} 不触 overutil 阈");
        assert_eq!(capacity_for_util(&QEMU_VIRT_DUAL, d), serving);
    }

    // 对抗件：t7 工作量 800 → b4 批需求 1150 超域容量。
    let mut hot = ten_timers();
    hot[7].w = 800;
    let reqs_hot = reqs_of(&hot);
    let out_hot = coalesce(&reqs_hot, 0, GRID_RECOMMEND_NS).unwrap();
    assert!(laws_hold(&reqs_hot, &out_hot), "工作量账不进 slack 几何——分桶不变");
    let d4: u32 = out_hot.buckets[3].members.iter().map(|&i| hot[i].w).sum();
    assert_eq!(d4, 1150);
    assert!(d4 > cap, "批超容量");
    assert!(overutil_judgment(d4, cap), "超容批次判 overutil（EAS 让位面）");
}

/// B-3 全链收口（thermal → power → EM → slack 批可行性）：serious 热帽
/// （45%@1GHz = 450kHz）经桥入席、聚合胜出 → 帽截 OPP 域（顶容 400）→
/// b2 批需求 700 放不下且 overutil、b1 批 250 仍可行——唤醒批次可行性随
/// 热帽收紧而翻转，四 crate 数据流一拍串完。
#[test]
fn thermal_cap_shrinks_wakeup_capacity() {
    // 热侧：穿 serious（传感器 71°C）。
    let trips = ladder3();
    let mut eng = Engine::new();
    let v = eval_tick(&mut eng, 71_000, &trips, 25_000);
    assert_eq!((v.thermal_level, v.freq_cap_pct), (2, 45));
    // 帽入聚合点：低电 500kHz（真帧）+ 调度 hint 600kHz + 热帽 450kHz
    // → min 归热席。
    let mut core = PowerCore::new(PolicyParams::default_sim());
    feed_battery(&mut core, 1, 8);
    core.caps.set(CAP_SRC_SCHEDHINT, Some(600_000)).unwrap();
    let cap_khz = match crate::thermal_cap_khz(v.freq_cap_pct, core.policy.fmax_khz).unwrap() {
        crate::ThermalCap::CapKhz(k) => k,
        crate::ThermalCap::NoCap => panic!("serious 档必有帽"),
    };
    assert_eq!(cap_khz, 450_000);
    core.caps.set(CAP_SRC_THERMAL, Some(cap_khz)).unwrap();
    assert_eq!(
        core.caps.aggregate(),
        Some(CapDecision { cap_khz: 450_000, src: CAP_SRC_THERMAL })
    );
    // 帽截域：顶容 1024 → 400。
    let dom = crate::cap_limited_domain(&QEMU_VIRT_DUAL, cap_khz);
    assert_eq!(em_max_capacity_of(&dom), 400);
    // slack 批可行性翻转：b2（700）放不下 + overutil；b1（250）仍可行。
    let timers = ten_timers();
    let reqs = reqs_of(&timers);
    let out = coalesce(&reqs, 0, GRID_RECOMMEND_NS).unwrap();
    let d1: u32 = out.buckets[0].members.iter().map(|&i| timers[i].w).sum();
    let d2: u32 = out.buckets[1].members.iter().map(|&i| timers[i].w).sum();
    assert_eq!((d1, d2), (250, 700));
    assert!(d1 <= em_max_capacity_of(&dom) && !overutil_judgment(d1, em_max_capacity_of(&dom)));
    assert!(d2 > em_max_capacity_of(&dom) && overutil_judgment(d2, em_max_capacity_of(&dom)));
}
