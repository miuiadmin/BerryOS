//! 场景共享测试件（cfg(test)——三场景共用的输入构造与喂帧助手，
//! 形态镜像四 crate 各自测试面的同名助手）。

use berry_power::{BatterySnapshot, CoreEvent, PowerCore};
use berry_thermal::{Engine, SensorInput, Tick, ThermalTrip, THERMAL_KIND_REAL};

/// sim 设计容量单源测试值（4_800_000 µAh = 4800 mAh——berry-power
/// lib/charge 测试面同款样本；场景 C 的 pct 兼容桥换算基）。
pub const CAP_UAH: u32 = 4_800_000;

/// 三档梯表（fair 60/55、serious 70/65、critical 80/75，°C→毫摄氏）——
/// berry-thermal lib 测试面 `ladder3` 同款（穿 trip/迟滞/四档发布共用表）。
pub fn ladder3() -> [ThermalTrip; 3] {
    [
        ThermalTrip { level: 1, threshold_mc: 60_000, clr_mc: 55_000 },
        ThermalTrip { level: 2, threshold_mc: 70_000, clr_mc: 65_000 },
        ThermalTrip { level: 3, threshold_mc: 80_000, clr_mc: 75_000 },
    ]
}

/// 单传感器拍评估（传感器 0/real/给定 trips 表 + 电池温）——合法输入必
/// 过；数组在栈上存活整个 evaluate（输入只借用纪律）。
pub fn eval_tick(
    e: &mut Engine,
    temp_mc: i32,
    trips: &[ThermalTrip],
    battery_temp_mc: i32,
) -> berry_thermal::Verdict {
    let sensors = [SensorInput {
        sensor_id: 0,
        kind: THERMAL_KIND_REAL,
        temp_mc,
        trips,
    }];
    e.evaluate(&Tick { sensors: &sensors, battery_temp_mc }).expect("合法输入必过")
}

/// 电池快照（berry-power lib 测试面 `battery` 同款样本，温 25°C/容量
/// 字段 4800 mAh——temp_mc 与热引擎的 battery_temp_mc 两轴，互不相干）。
pub fn battery_snapshot(pct: u8) -> BatterySnapshot {
    BatterySnapshot {
        present: 1,
        ac_online: 1,
        status: 2,
        health: 3,
        capacity_pct: pct,
        voltage_mv: 3990,
        temp_mc: 25_000,
        current_now_ua: 412_000,
        current_avg_ua: 395_000,
        charge_full_uah: CAP_UAH,
        charge_counter_uah: ((pct as u32) * CAP_UAH / 100) as u32,
        cycle_count: 143,
        timestamp_ns: 42,
        measured: 1,
    }
}

/// 喂一帧 battery（pwproto 编码器产线 → PowerCore 会话面 → 低电策略入席
/// ——低电帽/低电票的唯一真实数据流路径，测试直 set 席不经会话的捷径
/// 不取）。帧必应用（assert 收口）。
pub fn feed_battery(core: &mut PowerCore, seq: u16, pct: u8) {
    let b = battery_snapshot(pct);
    let mut buf = [0u8; 128];
    let n = berry_pwproto::encode_battery(&mut buf, seq, &b, 0).expect("编码必成");
    let mut last = CoreEvent::Idle;
    for &by in &buf[..n] {
        last = core.push_byte(by);
    }
    assert!(
        matches!(last, CoreEvent::Applied { .. }),
        "battery 帧必应用（seq={seq}）"
    );
}
