//! berry-thermal —— 热治理常驻服务壳（骨架；ADR #66 L1，`docs/架构决策.md`
//! 决策总表 #66 行）。仲裁引擎全在 lib（`src/lib.rs`，host 单测盖）；本壳只做
//! main 循环骨架——**输入订阅/三类输出写面全部留窗**，未接线相 = 永静默
//! park（被拉起也不产出动作、不写任何帽/票/发布位）。
//!
//! ## 挂接留窗（接线刀逐件补，本刀零碰）
//!
//! 1. **常驻预拉/rootfs 装配留窗**：svcmgr = owner 占用（表位/门控/spawn 形
//!    态照 simbattery 先例：表项在席 = 门开相）；justfile build-user musl 链
//!    配方（berry-ld + aarch64-unknown-linux-musl）同窗。
//! 2. **输入面留窗**：驱动自报 trips 订阅（中断 trip 优先 + 轮询兜底，
//!    ADR #66 ①）——tick 组装 = pwproto `ThermalSnapshot`（DEV_THERMAL）→
//!    `Tick`（sensor_id 由通道端点位显式化）；电池温取电池驱动自报
//!    （`BatterySnapshot::temp_mc` 同源）。**F2 终裁（04 号复核 §6-4）：
//!    sim 流按设备分端——`DEV_THERMAL` 帧归本服务**（Tick 组装订阅面；
//!    `DEV_BATTERY` 归 simbattery、`DEV_MODEM` 归 berry-power 遥测——双
//!    消费者分帧错乱的裁决答案；分端执法归通道拓扑接线刀）。
//! 3. **输出① 频率帽请求留窗**：写面 = berry-power 内核聚合点取 min（帽不
//!    新开通道，#66 ②）——聚合点未建（三大接缝之二本体缺位），本壳不写。
//! 4. **输出② 充电限流票留窗**：P11 电流版票 IPC（chargerd 仲裁，#66 ⑤）
//!    ——chargerd 未建。**票面 mA = F1 终裁单位**（04 号复核 §6-3）；线面
//!    若持 #68 `limit_pct` 冻结形，经 berry-power 侧设计容量兼容桥
//!    （`user/bins/berry-power/src/charge.rs` `pct_of_ma`/`ma_of_pct`）换算，
//!    协议升版携 mA 直传（升版改口锚）。
//! 5. **输出③ thermal_level 发布留窗**：只读+订阅两口（PM-15 §4 草案
//!    `berry_thermal_level`/`berry_thermal_subscribe` 形，berry-sys 服务
//!    客户端包装）。
//! 6. **失败安全留窗**：「热服务亡→帽保持」的执法位 = berry-power 聚合点
//!    （last-writer 保持）；本壳以 lib `hold_outputs`/`OutputLatch` 表达契约
//!    （消费侧语义），critical 关机短链在内核、独立于本服务存活。
//! 7. **遥测环留窗**：档位迁移/帽变化进 THERM 臂（PM-07 §4-7 smoke 断言
//!    四件套挂此窗）。
//!
//! 自检：起机一拍 nominal 契约自证（引擎活面）——失败 = 单行诊断 + park
//! （拒绝起，安全侧 loud；标记纪律：无 smoke 契约不打标记）。

use berry_thermal::{
    Engine, OutputLatch, Tick, SensorInput, LEVEL_NOMINAL,
};

/// 自检 nominal 温（25 °C 毫摄氏，JEITA NORMAL 区内）。
const SELF_TEMP_MC: i32 = 25_000;
/// 无 trip 传感面（自检用：nominal 必达）。
const SELF_NO_TRIPS: [berry_thermal::ThermalTrip; 0] = [];

fn main() {
    let mut engine = Engine::new();
    let mut latch = OutputLatch::new();
    boot_selfcheck(&mut engine, &mut latch);
    loop {
        // 【留窗 2·输入面】订阅驱动自报 trips → 组装 Tick（全量快照、tick
        // 原子性由调方保证；中断优先+轮询兜底）。骨架相无输入源。
        // let tick: Tick = ...;
        // 【留窗·仲裁】latch.commit(engine.evaluate(&tick))——Err（坏输入
        // 拒面）= 前值保持不松帽（lib 失败安全位）。
        // let verdict = latch.commit(engine.evaluate(&tick));
        // 【留窗 3·输出①】verdict.freq_cap_pct → berry-power 聚合点写请求。
        // 【留窗 4·输出②】verdict.charge_limit_ma → chargerd P11 出票。
        // 【留窗 5·输出③】verdict.thermal_level → 只读+订阅发布位。
        // 【留窗 7·遥测】档位迁移/帽变化 → THERM 臂。
        // 骨架相：无输入源 = 永静默等待（接线刀把 park 换通道 wait——
        // simbattery recv_blocking 形先例）。
        berry_libc::park();
    }
}

/// 起机契约自证：nominal 输入一拍过引擎，必得 nominal 档 + 无帽 + 全速充电
/// 档（JEITA NORMAL）；过检即把 nominal 裁决落锁（锁存器起始态 = nominal，
/// 非 None——服务在册即有基线输出）。失败 = 服务拒绝起（park + 单行诊断）
/// ——仲裁面坏了的热服务不该活着（安全侧）。
fn boot_selfcheck(engine: &mut Engine, latch: &mut OutputLatch) {
    let sensors = [SensorInput {
        sensor_id: 0,
        kind: berry_thermal::THERMAL_KIND_REAL,
        temp_mc: SELF_TEMP_MC,
        trips: &SELF_NO_TRIPS,
    }];
    let tick = Tick { sensors: &sensors, battery_temp_mc: SELF_TEMP_MC };
    let ok = match engine.evaluate(&tick) {
        Ok(v) => {
            let nominal_ma = berry_thermal::JEITA_TABLE
                [berry_thermal::JEITA_NORMAL as usize]
                .2;
            let pass = v.thermal_level == LEVEL_NOMINAL
                && v.freq_cap_pct == berry_thermal::FREQ_CAP_NONE_PCT
                && v.charge_limit_ma == nominal_ma;
            if pass {
                latch.commit(Ok(v)); // 基线落锁（留窗·仲裁路径的首拍）
            }
            pass
        }
        Err(_) => false,
    };
    if !ok {
        eprintln!("berry-thermal: selfcheck failed");
        berry_libc::park();
    }
}
