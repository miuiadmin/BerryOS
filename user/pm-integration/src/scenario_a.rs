//! 场景 A —— 热降频全链：thermal 穿 trip 出票 → 桥 → power 聚合 min →
//! EM 帽下选核。
//!
//! 数据流（ADR #66 ②「频率帽汇入 berry-power 同一内核聚合点取 min（唯一
//! 合法帽书写面）」的纯面串联）：
//!
//! ```text
//! berry-thermal Engine::evaluate ─→ Verdict{freq_cap_pct(u8), charge_limit_ma(u32)}
//!        │ 桥1 thermal_cap_khz(pct, fmax)          │ （mA 直连——场景 C 全链）
//!        ▼                                        ▼
//! FreqCapAgg::set(CAP_SRC_THERMAL, khz)     ChgArbiter::vote(CHG_SLOT_JEITA, …)
//!        ▼
//! FreqCapAgg::aggregate() = min(热帽, 调度hint, 低电) ──→ 帽值 + 胜出源归因
//!        ▼ 桥2 cap_limited_domain（帽截 OPP → 域有效容量）
//! berry-em eas_pick_cpu（帽下选核——结果随帽变）
//! ```

use crate::common::{eval_tick, feed_battery, ladder3};
use crate::{thermal_cap_khz, ThermalCap};
use berry_em::{eas_pick_cpu, em_max_capacity_of, QEMU_VIRT_DUAL};
use berry_power::{
    CapDecision, PolicyParams, PowerCore, CAP_SRC_LOWBATT, CAP_SRC_SCHEDHINT,
    CAP_SRC_THERMAL,
};
use berry_thermal::{level_freq_cap_pct, Engine, OutputLatch, Verdict};

/// 热帽入席（桥 1 → 帽写面）：Ok 才写席、NoCap = 撤帽——nominal 档「写面
/// 零动作」的装配侧形态。
fn seat_thermal_cap(core: &mut PowerCore, v: &Verdict, fmax_khz: u32) {
    match thermal_cap_khz(v.freq_cap_pct, fmax_khz).expect("档表值必过桥") {
        ThermalCap::NoCap => core.caps.set(CAP_SRC_THERMAL, None).unwrap(),
        ThermalCap::CapKhz(khz) => core.caps.set(CAP_SRC_THERMAL, Some(khz)).unwrap(),
    }
}

/// A-1 温度穿程全链：25°C nominal → fair → serious → critical → 回落解除。
/// 三源在席（低电 500kHz 真帧入席 + 调度 hint 600kHz + 热帽经桥入席），
/// 聚合 = min：fair（700kHz）不敌低电帽 → 帽仍归低电席；serious（450kHz）
/// 与 critical（250kHz）热帽最紧 → 帽归热席；回落穿 clr 解除 → 回低电帽。
/// 同程 mA 票随档变（出票面完整性——仲裁全链在场景 C）。
#[test]
fn thermal_ramp_drives_cap_aggregation_attribution() {
    let trips = ladder3();
    let mut eng = Engine::new();
    let mut core = PowerCore::new(PolicyParams::default_sim());
    let fmax = core.policy.fmax_khz;

    // 低电帽/票经真帧入席（battery 8% ≤15 → 低电帽 500kHz；≤10 → 预充票
    // 2400mA——会话面 → 策略面全链，berry-power lib 同款钉值）。
    feed_battery(&mut core, 1, 8);
    assert_eq!(
        core.decide().cap,
        Some(CapDecision { cap_khz: 500_000, src: CAP_SRC_LOWBATT })
    );
    // 调度 hint 帽（#42 系统态叠加票词汇——调度器保守建议面）。
    core.caps.set(CAP_SRC_SCHEDHINT, Some(600_000)).unwrap();

    // (temp, level, pct, 热席 khz, 聚合 khz, 胜出源, mA 票)
    let steps: [(i32, u8, u8, Option<u32>, u32, u8, u32); 5] = [
        (25_000, 0, 100, None, 500_000, CAP_SRC_LOWBATT, 1500),
        (61_000, 1, 70, Some(700_000), 500_000, CAP_SRC_LOWBATT, 1200),
        (71_000, 2, 45, Some(450_000), 450_000, CAP_SRC_THERMAL, 1000),
        (81_000, 3, 25, Some(250_000), 250_000, CAP_SRC_THERMAL, 0),
        (50_000, 0, 100, None, 500_000, CAP_SRC_LOWBATT, 1500),
    ];
    for (temp, level, pct, seat_khz, agg_khz, agg_src, ticket_ma) in steps {
        let v = eval_tick(&mut eng, temp, &trips, 25_000);
        // 上游出票面：档位/帽%/mA 票三面同拍（档表完整性——pct 恒等于
        // 档表行，防表漂）。
        assert_eq!((v.thermal_level, v.freq_cap_pct), (level, pct), "temp={temp}");
        assert_eq!(v.freq_cap_pct, level_freq_cap_pct(v.thermal_level));
        assert_eq!(v.charge_limit_ma, ticket_ma, "temp={temp}");
        // 桥 1 + 帽写面 + 下游收到（get 面回读对拍）。
        seat_thermal_cap(&mut core, &v, fmax);
        assert_eq!(core.caps.get(CAP_SRC_THERMAL).unwrap(), seat_khz, "temp={temp}");
        // 聚合 = min(热帽, 调度 hint, 低电) + 胜出源归因。
        let agg = core.caps.aggregate().expect("低电在席恒有帽");
        assert_eq!((agg.cap_khz, agg.src), (agg_khz, agg_src), "temp={temp}");
    }
}

/// A-2 EM 帽下选核：同一任务同一底景，帽收紧扣域有效容量 → 选核结果随帽
/// 变——两形态：①归因反转（打包偏好消失，平局回 prev——帽截掉跨档跳升
/// 后「装轻核保低档」的能量差被抹平）；②放不下让域（task_util > 帽后域
/// 顶容量 → 候选除名 → None 回退负载均衡，EAS 让位语义）。
#[test]
fn em_selection_changes_with_cap() {
    let dom_full = QEMU_VIRT_DUAL;
    let dom_fair = crate::cap_limited_domain(&dom_full, 700_000); // fair 70%@1GHz
    let dom_serious = crate::cap_limited_domain(&dom_full, 450_000); // serious 45%
    // 域有效容量阶梯：1024 → 750 → 400（帽截 OPP 的容量语义）。
    assert_eq!(
        (em_max_capacity_of(&dom_full), em_max_capacity_of(&dom_fair), em_max_capacity_of(&dom_serious)),
        (1024, 750, 400)
    );

    // ① 归因反转：底景 [100,400]、任务 100、prev=CPU1。无帽：装轻核
    // CPU0 保域低档（边际 1250 < 5750——berry-em pick_cpu_laws 钉值）→
    // Some(0)；serious 帽下单点域（400 容量/5mW）：两候选边际同（1250）
    // → 平局 prev 胜 → Some(1)。同一任务同一底景，帽变则选核变。
    let utils = [100u32, 400];
    assert_eq!(eas_pick_cpu(&[dom_full], &utils, 100, 1), Some(0));
    assert_eq!(eas_pick_cpu(&[dom_serious], &utils, 100, 1), Some(1));

    // ② 放不下让域：任务 800 —— 无帽放得下（≤1024，选 CPU0）；fair 帽
    // （顶容 750）放不下 → None；serious 更甚。任务 600 在 fair 仍放得下
    // （Some(0)）、serious 放不下 → None——帽阶梯逐级把大任务逐出 EAS。
    for (task, want_full, want_fair, want_serious) in
        [(600u32, Some(0usize), Some(0), None), (800, Some(0), None, None)]
    {
        assert_eq!(eas_pick_cpu(&[dom_full], &utils, task, 0), want_full, "task={task}");
        assert_eq!(eas_pick_cpu(&[dom_fair], &utils, task, 0), want_fair, "task={task}");
        assert_eq!(eas_pick_cpu(&[dom_serious], &utils, task, 0), want_serious, "task={task}");
    }
}

/// A-3 失败安全贯穿链（ADR #66 ③「热服务亡→帽保持」的装配侧面）：坏拍
/// （电池温出可信域）整拍拒收、引擎状态零动——装配侧纪律 = Err 不触席
/// （OutputLatch commit(Err) 保持前值），热席保持 serious 帽、聚合不变。
/// 传感面停报同款：id 0 缺席 → 锁存档保持，系统档不回落。
#[test]
fn failure_hold_keeps_thermal_seat() {
    let trips = ladder3();
    let mut eng = Engine::new();
    let mut core = PowerCore::new(PolicyParams::default_sim());
    let fmax = core.policy.fmax_khz;
    feed_battery(&mut core, 1, 8); // 低电 500kHz 在席
    core.caps.set(CAP_SRC_SCHEDHINT, Some(600_000)).unwrap();
    let mut latch = OutputLatch::new();

    // 穿到 serious：热席 450kHz、聚合归热席。
    let v = eval_tick(&mut eng, 71_000, &trips, 25_000);
    seat_thermal_cap(&mut core, &v, fmax);
    latch.commit(Ok(v));
    assert_eq!(core.caps.aggregate().unwrap().cap_khz, 450_000);

    // 坏拍：电池温 500°C 出可信域 → 整拍 Err（拒面原子性）→ 锁存器保持
    // 前值；装配侧 Err 不写席 → 热席/聚合原样（帽不自动解除）。
    let sensors = [berry_thermal::SensorInput {
        sensor_id: 0,
        kind: berry_thermal::THERMAL_KIND_REAL,
        temp_mc: 71_000,
        trips: &trips,
    }];
    let bad = eng.evaluate(&berry_thermal::Tick { sensors: &sensors, battery_temp_mc: 500_000 });
    assert!(bad.is_err());
    let held = latch.commit(bad).expect("拒面前有 serious 裁决在锁");
    assert_eq!((held.thermal_level, held.freq_cap_pct), (2, 45));
    assert_eq!(core.caps.get(CAP_SRC_THERMAL).unwrap(), Some(450_000));
    assert_eq!(core.caps.aggregate().unwrap().cap_khz, 450_000);

    // 传感面停报：本拍只报新传感器 id 1（nominal）——id 0 锁存档保持
    // serious，系统档 = max(2, 0) = 2，帽不回落。
    let s1 = [berry_thermal::SensorInput {
        sensor_id: 1,
        kind: berry_thermal::THERMAL_KIND_VIRTUAL,
        temp_mc: 25_000,
        trips: &trips,
    }];
    let v2 = eng
        .evaluate(&berry_thermal::Tick { sensors: &s1, battery_temp_mc: 25_000 })
        .expect("合法输入必过");
    assert_eq!((v2.thermal_level, v2.freq_cap_pct), (2, 45));
    seat_thermal_cap(&mut core, &v2, fmax);
    assert_eq!(core.caps.aggregate().unwrap().cap_khz, 450_000);
}
