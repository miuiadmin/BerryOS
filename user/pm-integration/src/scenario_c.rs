//! 场景 C —— 充电票+关机边界：JEITA 温区票（mA）→ ChgArbiter 三席仲裁 →
//! 低电票叠加 → min(三票) + 哨兵边界 + critical 停充。
//!
//! 数据流（ADR #66 ⑤「charge_current_limit 票（P11 电流版）由
//! berry-thermal 出票」+ #68 P11「最少者赢、断线自动撤票」；F1 终裁后
//! 两面同为 mA 域直连——无桥无换算）：
//!
//! ```text
//! berry-thermal Engine::evaluate ──→ Verdict.charge_limit_ma(u32 mA)
//!                                        │ = min(JEITA 温区档, 性能档)——上游自仲裁
//!                                        ▼ mA 直连（类型/值域原样）
//! ChgArbiter::vote(CHG_SLOT_JEITA, ma, CHG_REASON_JEITA)
//!   ∪ 低电票（PowerCore 会话面真帧 → CHG_SLOT_LOWBATT 2400mA）
//!   ∪ 用户帽（vote_pct 兼容投递 → CHG_SLOT_USER）
//!                                        ▼ 最少者赢（平票席号小者）
//! ChgDecision{limit_ma, reason} ──→ limit_pct(CAP_UAH) #68 线面兼容读
//! ```
//!
//! 关机边界（ADR #66 ③④）：critical trip 的关机短链在内核
//! （orderly→emergency 同构，独立于用户态存活）、critical 档无豁免——
//! 用户态纯面验到「票 0 停充 + 帽 25% + 发布 3」为止，短链本体不在本面
//! （#66 行尾 smoke 断言四件套之 (d) 挂接线刀 QEMU 相）。

use crate::common::{eval_tick, feed_battery, ladder3, CAP_UAH};
use berry_power::{
    ChgArbiter, ChgDecision, PolicyParams, PowerCore, CHG_LIMIT_UNLIMITED_MA,
    CHG_REASON_JEITA, CHG_REASON_LOWBATT, CHG_REASON_NONE, CHG_REASON_USER,
    CHG_SLOT_JEITA, CHG_SLOT_LOWBATT, CHG_SLOT_USER,
};
use berry_thermal::{Engine, OutputLatch, Verdict, CHARGE_NO_CAP_MA, JEITA_COOL, JEITA_HOT, JEITA_NORMAL, JEITA_WARM};

/// C-1 JEITA 温区票 ramp × min(三票)：传感器恒 25°C（性能轴不动——两轴
/// 独立，JEITA 用电池温），电池温穿四温区 → thermal 票 [1500, 750, 500, 0]；
/// 三席在位（JEITA 真票 + 低电 2400 真帧 + 用户 3840=80%）→ min 归因逐拍
/// 验证；用户帽收紧到 25%（1200mA）时归因切用户席——arbitrate 取真 min
/// 不认席位特权。
#[test]
fn jeita_ramp_min_of_three_tickets() {
    let trips = ladder3();
    let mut eng = Engine::new();
    let mut core = PowerCore::new(PolicyParams::default_sim());
    // 低电票经真帧入席（battery 8% ≤10 → 预充 2400mA）。
    feed_battery(&mut core, 1, 8);
    assert_eq!(
        core.votes.get(CHG_SLOT_LOWBATT).unwrap(),
        Some((2400, CHG_REASON_LOWBATT))
    );
    // 用户帽 80% 经 #68 兼容投递（pct→mA 桥落票 3840）。
    core.votes.vote_pct(CHG_SLOT_USER, 80, CHG_REASON_USER, CAP_UAH).unwrap();
    assert_eq!(core.votes.get(CHG_SLOT_USER).unwrap(), Some((3840, CHG_REASON_USER)));

    // (电池温, jeita_zone, thermal 票 mA, 期望 min mA, 期望归因)
    let steps: [(i32, u8, u32, u32, u8); 4] = [
        (25_000, JEITA_NORMAL, 1500, 1500, CHG_REASON_JEITA),
        (50_000, JEITA_WARM, 750, 750, CHG_REASON_JEITA),
        (5_000, JEITA_COOL, 500, 500, CHG_REASON_JEITA),
        (60_000, JEITA_HOT, 0, 0, CHG_REASON_JEITA), // 恰 60 归 HOT 停充（保守侧）
    ];
    for (i, (btemp, zone, ticket, min_ma, reason)) in steps.iter().enumerate() {
        let v = eval_tick(&mut eng, 25_000, &trips, *btemp);
        // 上游出票面：温区档与 mA 票（= min(JEITA, 性能档)，性能轴 nominal
        // → 纯 JEITA 管）。
        assert_eq!((v.jeita_zone, v.charge_limit_ma), (*zone, *ticket), "btemp={btemp}");
        // mA 直连投递 + 下游收到（值域原样、reason 归席）。
        core.votes.vote(CHG_SLOT_JEITA, v.charge_limit_ma, CHG_REASON_JEITA).unwrap();
        assert_eq!(
            core.votes.get(CHG_SLOT_JEITA).unwrap(),
            Some((*ticket, CHG_REASON_JEITA)),
            "btemp={btemp}"
        );
        // 首拍后用户帽收紧 25%（1200mA）→ NORMAL 温区下 min 归因切用户席
        // （1500 < 2400 但 > 1200）；其后温区票收紧重新接管。
        if i == 0 {
            core.votes.vote_pct(CHG_SLOT_USER, 25, CHG_REASON_USER, CAP_UAH).unwrap();
            assert_eq!(
                core.decide().chg,
                ChgDecision { limit_ma: 1200, reason: CHG_REASON_USER }
            );
            continue;
        }
        // min(三票) + 胜出归因。
        assert_eq!(
            core.decide().chg,
            ChgDecision { limit_ma: *min_ma, reason: *reason },
            "btemp={btemp}"
        );
    }
    // 跨 crate 哨兵并轨钉（F1：两面同值同语义——mA 直连的前提）。
    assert_eq!(CHARGE_NO_CAP_MA, CHG_LIMIT_UNLIMITED_MA);
}

/// C-2 哨兵与撤票边界（ADR #68「最少者赢/断线自动撤票」的 mA 域边界钉）：
/// 全席缺席 → 不限档{MAX, NONE}；单席不限票仍归因（出票即有责）；不限
/// 哨兵 min 穿透不拦实际限值（含 thermal 侧哨兵值直投——F1 并轨面）；
/// 平票双不限 → 席号小者（JEITA）；撤票后顶位链；禁充 0 压过一切；
/// #68 线面兼容读（limit_pct）还原与无据拒面。
#[test]
fn sentinel_and_withdraw_boundaries() {
    let mut arb = ChgArbiter::new();
    // 全席缺席 = 不限（chargerd 满充档）。
    assert_eq!(arb.arbitrate(), ChgDecision { limit_ma: CHG_LIMIT_UNLIMITED_MA, reason: CHG_REASON_NONE });
    // 单席不限票（用户 100% 兼容投递 → MAX 哨兵）：仍归因用户席。
    arb.vote_pct(CHG_SLOT_USER, 100, CHG_REASON_USER, CAP_UAH).unwrap();
    assert_eq!(arb.get(CHG_SLOT_USER).unwrap(), Some((CHG_LIMIT_UNLIMITED_MA, CHG_REASON_USER)));
    assert_eq!(arb.arbitrate().reason, CHG_REASON_USER);
    // 低电真票 2400 → min 穿透：哨兵不拦实际限值。
    arb.vote(CHG_SLOT_LOWBATT, 2400, CHG_REASON_LOWBATT).unwrap();
    assert_eq!(arb.arbitrate(), ChgDecision { limit_ma: 2400, reason: CHG_REASON_LOWBATT });
    // thermal 侧哨兵值（CHARGE_NO_CAP_MA）经 mA 直投面落 JEITA 席——同值
    // 同语义在仲裁面的实证（v1 档表不产此值，投递面合法性钉板）。
    arb.vote(CHG_SLOT_JEITA, CHARGE_NO_CAP_MA, CHG_REASON_JEITA).unwrap();
    assert_eq!(arb.arbitrate().limit_ma, 2400);
    // 低电撤票（断线自动撤票库面）→ 双不限平票 → 席号小者（JEITA）胜。
    arb.withdraw(CHG_SLOT_LOWBATT).unwrap();
    assert_eq!(
        arb.arbitrate(),
        ChgDecision { limit_ma: CHG_LIMIT_UNLIMITED_MA, reason: CHG_REASON_JEITA }
    );
    // 禁充 0（JEITA 冷/热区最严档）：压过不限哨兵——硬安全优先。
    arb.vote(CHG_SLOT_JEITA, 0, CHG_REASON_JEITA).unwrap();
    assert_eq!(arb.arbitrate(), ChgDecision { limit_ma: 0, reason: CHG_REASON_JEITA });
    // #68 线面兼容读：设计容量桥还原（2400→50%、750→15%、不限→100）与
    // 无据拒面（容量 0 → None——mA 面为真相不虚构 pct）。
    assert_eq!(ChgDecision { limit_ma: 2400, reason: 0 }.limit_pct(CAP_UAH), Some(50));
    assert_eq!(ChgDecision { limit_ma: 750, reason: 0 }.limit_pct(CAP_UAH), Some(15));
    assert_eq!(
        ChgDecision { limit_ma: CHG_LIMIT_UNLIMITED_MA, reason: 0 }.limit_pct(CAP_UAH),
        Some(100)
    );
    assert_eq!(ChgDecision { limit_ma: 2400, reason: 0 }.limit_pct(0), None);
}

/// C-3 critical 停充+关机边界面：传感器 81°C 穿 critical → 三面同拍
/// （发布 3 / 帽 25% / 票 0 停充——性能档 0 压过 JEITA NORMAL 1500）；
/// 帽经桥入聚合点（低电/调度 hint 在席仍归热席——250kHz 最紧）；util
/// 请求超帽被钳（eff = 帽值——「唯一输出面吃 eff」纪律）；坏拍后锁存
/// 保持（帽/票不自动解除——关机边界的失败安全侧）。
#[test]
fn critical_stop_charge_and_shutdown_boundary_face() {
    let trips = ladder3();
    let mut eng = Engine::new();
    let mut core = PowerCore::new(PolicyParams::default_sim());
    let fmax = core.policy.fmax_khz;
    let mut latch = OutputLatch::new();
    feed_battery(&mut core, 1, 8); // 低电 500kHz + 预充票 2400
    core.caps.set(berry_power::CAP_SRC_SCHEDHINT, Some(600_000)).unwrap();

    // 穿 critical：三面同拍。
    let v = eval_tick(&mut eng, 81_000, &trips, 25_000);
    assert_eq!((v.thermal_level, v.freq_cap_pct, v.charge_limit_ma), (3, 25, 0));
    latch.commit(Ok(v));
    // 帽入席 → 聚合最紧归热席（250 < 500 < 600）。
    let cap_khz = match crate::thermal_cap_khz(v.freq_cap_pct, fmax).unwrap() {
        crate::ThermalCap::CapKhz(k) => k,
        crate::ThermalCap::NoCap => panic!("critical 档必有帽"),
    };
    assert_eq!(cap_khz, 250_000);
    core.caps.set(berry_power::CAP_SRC_THERMAL, Some(cap_khz)).unwrap();
    assert_eq!(
        core.caps.aggregate(),
        Some(berry_power::CapDecision { cap_khz: 250_000, src: berry_power::CAP_SRC_THERMAL })
    );
    // 停充票入席：min(0, 2400, 3840) = 0@JEITA。
    core.votes.vote(CHG_SLOT_JEITA, v.charge_limit_ma, CHG_REASON_JEITA).unwrap();
    core.votes.vote_pct(CHG_SLOT_USER, 80, CHG_REASON_USER, CAP_UAH).unwrap();
    assert_eq!(core.decide().chg, ChgDecision { limit_ma: 0, reason: CHG_REASON_JEITA });
    // 频率面：util 满忙 10 窗 → EWMA 752（0→128→240→338→423→498→563→
    // 620→670→714→752，α=1/8 折叠）→ req 734375 > 帽 → eff 钳帽值
    //（内核聚合点写面只吃 eff——req 是意图 eff 才可写）。
    for _ in 0..10 {
        core.ingest_util_sample(0, 1024).unwrap();
    }
    let d = core.decide();
    assert_eq!(d.freq.req_khz, 734_375);
    assert_eq!(d.freq.eff_khz, 250_000);

    // 坏拍（电池温出域）→ 拒面原子性 → 锁存保持 critical 三面；席/聚合/
    // 票全保持（帽不自动解除——关机充电态由内核 trip+充电固件共保，本面
    // 只证用户态输出侧不松）。
    let sensors = [berry_thermal::SensorInput {
        sensor_id: 0,
        kind: berry_thermal::THERMAL_KIND_REAL,
        temp_mc: 81_000,
        trips: &trips,
    }];
    let bad = eng.evaluate(&berry_thermal::Tick { sensors: &sensors, battery_temp_mc: 500_000 });
    assert!(bad.is_err());
    let held: Verdict = latch.commit(bad).expect("critical 裁决在锁");
    assert_eq!((held.thermal_level, held.freq_cap_pct, held.charge_limit_ma), (3, 25, 0));
    assert_eq!(core.caps.aggregate().unwrap().cap_khz, 250_000);
    assert_eq!(core.decide().chg.limit_ma, 0);
    // 电池帧轴独立：battery 真帧回升 50%（低电票撤——50 > 预充阈值 10）
    // 后停充票仍 0@JEITA——JEITA 席持 critical 0 票不被电源域策略松动
    // （帧温 25°C 是电池遥测轴，JEITA 输入 = 热引擎 Tick.battery_temp_mc
    // 单源，两轴互不相干）。
    feed_battery(&mut core, 2, 50);
    assert_eq!(core.votes.get(CHG_SLOT_LOWBATT).unwrap(), None);
    assert_eq!(core.decide().chg, ChgDecision { limit_ma: 0, reason: CHG_REASON_JEITA });
}
