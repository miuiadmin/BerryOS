//! berry-thermal —— 热治理仲裁引擎（纯函数面；ADR #66 L1「热管理边界」立法
//! 的第一个服务实体件，契约单源 = `docs/架构决策.md` 决策总表 #66 行全文）。
//!
//! ## 定位与边界
//!
//! 调度彻底计划三大接缝之二「调度-PM 契约空转：berry-power 服务本体未建、
//! util/placement/EM 有产无消」——本 crate 是该接缝的第一块实体：berry-thermal
//! 服务本体的 **lib 纯函数仲裁引擎**（bin 壳见 `src/main.rs`，循环骨架 + 挂接
//! 留窗）。纯函数纪律照 `berry-pwproto` 先例：零分配、零 unsafe、零外部依赖
//! （path 依赖仅 berry-pwproto）、输入只借用、`#![cfg_attr(not(test), no_std)]`，
//! host 单测全盖（`cargo test -p berry-thermal --target <host>`）。
//!
//! ## 契约六件（ADR #66 决策列逐件锚）
//!
//! 1. **trips 自报输入面**（#66 ①）：`SensorInput{sensor_id, kind, temp_mc,
//!    trips[{level, threshold_mc, clr_mc}]}`——结构单源 = berry-pwproto
//!    `ThermalTrip`（`user/berry-pwproto/src/lib.rs`，DEV_THERMAL 面）；对齐
//!    「驱动自报 power levels」先例（simbattery 同族），中断 trip 优先+轮询
//!    兜底归 bin 壳接线（本 lib 只吃整拍快照，tick 原子性由调方保证）。
//! 2. **step_wise 查表 + 迟滞**（#66 ①，v1 保守防抖；PID over EM 留 v2）：
//!    升档须 `temp >= threshold_mc`、降档须 `temp <= clr_mc`（迟滞窗
//!    `(clr_mc, threshold_mc)` 内保持原态不抖）；Linux step_wise 的逐 poll
//!    单步 ramp 不取——v1 档位即表位，一步到位保守防抖（推断标注：档间
//!    逐拍单步走留 v2 与 PID 同窗裁）。
//! 3. **三类输出位**（#66 ①）——`Verdict` 三面：
//!    (a) 频率帽请求 `freq_cap_pct`：**帽不新开通道**（#66 ②）——写请求汇入
//!    berry-power 同一内核聚合点取 min（唯一合法帽书写面），本引擎只产请求
//!    值不持有通道；对调度器 = 系统态叠加票（#42 只叠加）。
//!    (b) 充电限流票 `charge_limit_ma`：P11 电流版 charge_current_limit，
//!    berry-thermal 出票、chargerd 仲裁（#66 ⑤/#68；PM-14 §4.3）。
//!    (c) thermal_level 发布 `thermal_level`：u8 四档（0 nominal/1 fair/
//!    2 serious/3 critical）只读+订阅；温度只进遥测不进契约（#66 ④）。
//! 4. **失败安全位**（#66 ③）：「热服务亡→帽保持」以输出保持函数
//!    [`hold_outputs`] 表达（消费侧契约：本拍无新裁决 → 输出=前值原样保持，
//!    安全侧=保持限频不自动解除）；critical trip 内核短链关机兜底独立于本
//!    服务存活（orderly→emergency 同构），关机充电态由内核 trip+充电固件
//!    共保——都不在 lib 面，注释锚在此。
//! 5. **JEITA 五档第一张系统热表**（#66 ⑤）：[`JEITA_TABLE`] T1–T5 温区→
//!    充电电流档（硬安全约束优先于性能热档——充电限流 = min(JEITA 档, 性能
//!    档)）；数值边界行业标准、电流档行业典型值推断标注（见表注）。
//! 6. **sim-thermal 同接口**（#66 ⑥）：注入走同一自报面不旁路——本 lib 即
//!    唯一仲裁路径，bin 壳组装 Tick 时无特例位（smoke 断言四件套挂接线刀）。
//!
//! ## 失败语义（拒面即安全面）
//!
//! 坏输入（`Result::Err`）= **整拍拒收、引擎状态零动**（拒面原子性）：传感面
//! 越帽/迟滞窗非法/档位越界/温域出界/重复 sensor_id 全拒——拒后消费侧走
//! [`hold_outputs`] 保持前值（不松帽）。传感器**停报**（本拍缺席）= 该传感器
//! 锁存档保持（不自动解除，传感面失败安全同款）；热表热换（同 id 表变）=
//! 纯穿线重铺不保持旧锁存（推断标注：表换保守保持留 v2 裁）。
//!
//! ## 推断标注总表（策略数值面 = 不承诺面，表项可调+遥测导出，ADR #68 ⑥）
//!
//! - 档→频率帽阶梯 [100, 70, 45, 25]%：Linux cpufreq 热降频阶梯/OH 热动作
//!   表典型值【推断】；
//! - 档→充电限流 [无帽, 1200, 1000, 0] mA：fair/serious 取 OH 配置实证
//!   1500→1200→1000（PM-07 §1.7）同构【推断】，critical 停充【推断】；
//! - JEITA 边界 0/10/45/60 °C：行业标准（PM-07 §2，TI slyt365/JEITA 官方
//!   指南）；电流档 0/500/1500/750/0 mA：行业典型值【推断】（normal 1500
//!   对齐 OH 全速实证）；
//! - 温度可信域 [-100, 200] °C：工程推断（PM-07 教训 10 传感器误报防线，
//!   出域=疯报拒拍）；
//! - 引擎传感面帽 8：工程推断（sim 通道 2 帽是 sim 面契约
//!   `THERMAL_SENSORS_MAX`，服务引擎不受 sim 面约束）。

#![cfg_attr(not(test), no_std)]

// trips 结构/常量单源复用（不双写）：DEV_THERMAL 面（user/berry-pwproto/
// src/lib.rs）。
pub use berry_pwproto::{
    ThermalTrip, THERMAL_KIND_REAL, THERMAL_KIND_VIRTUAL, THERMAL_LEVEL_MAX,
    THERMAL_TRIPS_MAX,
};

// ---- 契约常量 ----

/// 引擎传感面帽【推断：工程值，服务订阅真实驱动自报，不受 sim 通道 2 帽约束】。
pub const ENGINE_SENSORS_MAX: usize = 8;

/// thermal_level 四档（#66 ④：u8 只读+订阅，4–255 保留）。
pub const LEVEL_NOMINAL: u8 = 0;
/// 同上：1 = fair（轻度降档）。
pub const LEVEL_FAIR: u8 = 1;
/// 同上：2 = serious（重度降档）。
pub const LEVEL_SERIOUS: u8 = 2;
/// 同上：3 = critical（critical 档无豁免；关机短链在内核，非本引擎动作面）。
pub const LEVEL_CRITICAL: u8 = 3;

/// 无帽值（%）：nominal 档不申请帽（写面零动作——聚合点无请求即无帽）。
pub const FREQ_CAP_NONE_PCT: u8 = 100;

/// 档→频率帽阶梯（%）。`FREQ_CAP_NONE_PCT` = 无帽（不申请）【推断：Linux/OH
/// 降频阶梯典型值，策略表可调】。写面 = berry-power 内核聚合点取 min（帽不
/// 新开通道，#66 ②）。
pub const LEVEL_FREQ_CAP_PCT: [u8; 4] =
    [FREQ_CAP_NONE_PCT, 70, 45, 25];

/// 充电限流「无热侧约束」哨兵（min 组合下穿透：min(u32::MAX, x) = x）。
pub const CHARGE_NO_CAP_MA: u32 = u32::MAX;

/// 档→热侧充电限流（mA）【推断：OH 1500→1200→1000 实证同构 + critical 停充，
/// 策略表可调】。P11 电流版票面，出票方 berry-thermal、chargerd 仲裁。
pub const LEVEL_CHARGE_LIMIT_MA: [u32; 4] =
    [CHARGE_NO_CAP_MA, 1200, 1000, 0];

/// JEITA 温区档（#66 ⑤ 第一张系统热表，T1–T5）。
pub const JEITA_COLD: u8 = 0;
/// 同上：T2 COOL。
pub const JEITA_COOL: u8 = 1;
/// 同上：T3 NORMAL。
pub const JEITA_NORMAL: u8 = 2;
/// 同上：T4 WARM。
pub const JEITA_WARM: u8 = 3;
/// 同上：T5 HOT。
pub const JEITA_HOT: u8 = 4;

/// JEITA 五档表：`[温区 lo_mc, hi_mc) 半开 + 充电电流档 mA`（左闭右开，首档
/// lo = i32::MIN 铺底、末档 hi 开到无穷）。
///
/// 边界 0/10/45/60 °C = 行业标准（PM-07 §2：「10–45°C 全速快充；0–10°C 降流
/// 防析锂；45–60°C 降压降流；>60°C 或 <0°C 停充」，TI slyt365/JEITA 官方
/// 指南）；**恰 60.0°C 归 HOT 停充**（原文「>60」严格侧可给降流，此处取保守
/// 侧——边界值归严档）【边界归属推断标注】。电流档 = 行业典型值【推断，
/// 策略表可调】：cool 500（0.2–0.3C 降流典型）、normal 1500（对齐 OH 全速
/// 实证）、warm 750（降压降流减半典型）、冷/热 0（停充）。
///
/// 表随电池驱动自报可换（PM-07 §4-8「JEITA 五温区硬约束表随电池驱动自报，
/// 优先级高于性能热档」）——v1 骨架内置默认表，自报表装载留接线窗；边界
/// 迟滞留窗（v1 纯查表，消抖归充电固件侧共保）。
pub const JEITA_TABLE: [(i32, i32, u32); 5] = [
    (i32::MIN, 0, 0),         // T1 COLD：温 <0°C 停充
    (0, 10_000, 500),         // T2 COOL：0..10°C 降流（防析锂）
    (10_000, 45_000, 1500),   // T3 NORMAL：10..45°C 全速快充
    (45_000, 60_000, 750),    // T4 WARM：45..60°C 降压降流
    (60_000, i32::MAX, 0),    // T5 HOT：≥60°C 停充（保守侧归属）
];

/// 温度可信域下界（-100 °C，毫摄氏）【推断：PM-07 教训 10 传感器误报防线】。
pub const TEMP_MC_MIN: i32 = -100_000;
/// 温度可信域上界（200 °C，毫摄氏）【同上】。
pub const TEMP_MC_MAX: i32 = 200_000;

// ---- 输入面（#66 ① trips 自报结构）----

/// 单传感器一拍自报：内容面 = pwproto `ThermalSensor` 同源（kind/temp_mc/
/// trips），外加通道端点位隐含的 `sensor_id`（pwproto 注记：sensor_id 由通道
/// 端点位隐含，服务侧组装时显式化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorInput<'a> {
    pub sensor_id: u8,
    pub kind: u8,
    pub temp_mc: i32,
    pub trips: &'a [ThermalTrip],
}

/// 一拍仲裁输入：传感面全量 + 电池温（JEITA 面，BatterySnapshot.temp_mc 同源
/// ——电池驱动自报；性能档与充电温区**两轴独立**（PM-07 §2），JEITA 不进
/// thermal_level、trips 不进充电档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick<'a> {
    pub sensors: &'a [SensorInput<'a>],
    pub battery_temp_mc: i32,
}

// ---- 输出面（三类输出位 + 审计面）----

/// 单传感器当前档（审计面：档位迁移可 dump，PM-07 §4-10 可审计）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorLevel {
    pub sensor_id: u8,
    pub level: u8,
}

/// 一拍裁决：三类输出 + 审计件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    /// 输出③ thermal_level 发布（0..=3 四档，#66 ④）。
    pub thermal_level: u8,
    /// 输出① 频率帽请求（%，100 = 无帽不申请）——写面 = berry-power 聚合点
    /// 取 min（#66 ② 帽不新开通道，本引擎不出通道）。
    pub freq_cap_pct: u8,
    /// 输出② 充电限流票（mA，P11 电流版）= min(JEITA 档, 性能档)；哨兵
    /// [`CHARGE_NO_CAP_MA`] = 无热侧约束（不出票）。
    pub charge_limit_ma: u32,
    /// JEITA 温区档（0..=4，T1–T5）——审计/遥测件（温度只进遥测不进契约）。
    pub jeita_zone: u8,
    /// JEITA 档电流（mA，审计件）。
    pub jeita_limit_ma: u32,
    /// 性能档充电限流（mA，审计件；哨兵语义同 [`CHARGE_NO_CAP_MA`]）。
    pub level_charge_limit_ma: u32,
    /// 全体在册传感器（含停报保持者）当前档，前 `per_sensor_n` 个有效。
    pub per_sensor: [SensorLevel; ENGINE_SENSORS_MAX],
    pub per_sensor_n: u8,
}

// ---- 坏输入拒面 ----

/// 拒面枚举（拒 = 整拍拒收、引擎状态零动、消费侧走 [`hold_outputs`] 保持）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadInput {
    /// 传感数超 [`ENGINE_SENSORS_MAX`]。
    TooManySensors,
    /// 同拍重复 sensor_id（档位归属歧义）。
    DuplicateSensorId,
    /// kind 野值（须 pwproto REAL/VIRTUAL 两值之一）。
    BadKind,
    /// 单传感器 trip 数超 [`THERMAL_TRIPS_MAX`]（pwproto 同帽）。
    TooManyTrips,
    /// trip 阈值须严格升序（查表确定性：同表内阈值唯一有序）。
    TripsNotAscending,
    /// clr_mc > threshold_mc（迟滞窗非法——pwproto wire 面同拒，双保险）。
    ClrAboveThreshold,
    /// trip level > [`THERMAL_LEVEL_MAX`]（四档外野值）。
    LevelOutOfRange,
    /// 传感温度出可信域 [`TEMP_MC_MIN`]..[`TEMP_MC_MAX`]。
    TempOutOfRange,
    /// 电池温度出可信域（JEITA 面同域执法）。
    BatteryTempOutOfRange,
}

// ---- 查表辅助（纯函数，越界档取最严档保守侧）----

/// JEITA 温区查表：`[lo, hi)` 半开、首档铺底/末档开到无穷（任意 i32 有档）。
pub fn jeita_zone(temp_mc: i32) -> u8 {
    let last = JEITA_TABLE.len() - 1;
    for (z, &(lo, hi, _)) in JEITA_TABLE.iter().enumerate() {
        if temp_mc >= lo && (z == last || temp_mc < hi) {
            return z as u8;
        }
    }
    JEITA_HOT // 不可达（首档 lo = i32::MIN 铺底）；防御面取停充最严档。
}

/// JEITA 档→充电电流（越界档取 HOT 停充，保守侧）。
pub fn jeita_limit_ma(zone: u8) -> u32 {
    JEITA_TABLE[(zone as usize).min(JEITA_TABLE.len() - 1)].2
}

/// 性能档→频率帽 %（越界档取 critical 最严档，保守侧）。
pub fn level_freq_cap_pct(level: u8) -> u8 {
    LEVEL_FREQ_CAP_PCT[(level as usize).min(LEVEL_FREQ_CAP_PCT.len() - 1)]
}

/// 性能档→热侧充电限流 mA（越界档取 critical 停充，保守侧）。
pub fn level_charge_limit_ma(level: u8) -> u32 {
    LEVEL_CHARGE_LIMIT_MA
        [(level as usize).min(LEVEL_CHARGE_LIMIT_MA.len() - 1)]
}

// ---- 仲裁引擎 ----

/// 单传感器锁存态（迟滞位面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SensorState {
    sensor_id: u8,
    /// 在报表（热换检测用——驱动上报表变更 = 纯穿线重铺）。
    trips: [ThermalTrip; THERMAL_TRIPS_MAX],
    trip_count: usize,
    /// per-trip 迟滞锁存位：`temp >= thr` 置位 / 已置位且 `temp > clr` 保持 /
    /// `temp <= clr` 复位。
    active: [bool; THERMAL_TRIPS_MAX],
    /// 传感器档 = 有效 trip 中最大 level（无有效 = nominal）。
    latched_level: u8,
}

impl SensorState {
    const fn empty(sensor_id: u8) -> Self {
        SensorState {
            sensor_id,
            trips: [ThermalTrip { level: 0, threshold_mc: 0, clr_mc: 0 };
                THERMAL_TRIPS_MAX],
            trip_count: 0,
            active: [false; THERMAL_TRIPS_MAX],
            latched_level: 0,
        }
    }

    /// 表热换检测：trip 段（长度+内容）任一变 = 换表。
    fn table_changed(&self, trips: &[ThermalTrip]) -> bool {
        self.trips[..self.trip_count] != *trips
    }

    /// 落新表 + 纯穿线重铺（active = `temp >= thr`，不保持旧锁存——旧位号与
    /// 新表错位；保守保持留 v2【推断标注】）。
    fn rearm(&mut self, trips: &[ThermalTrip], temp_mc: i32) {
        for (i, t) in trips.iter().enumerate() {
            self.trips[i] = *t;
            self.active[i] = temp_mc >= t.threshold_mc;
        }
        self.trip_count = trips.len();
        self.relatch();
    }

    /// 迟滞步进（step_wise 查表 + 迟滞，#66 ①）：升穿 `threshold_mc` 置位、
    /// 迟滞窗 `(clr_mc, threshold_mc)` 内保持、`<= clr_mc` 复位——窗内往返
    /// 不抖（smoke 断言 (c) 的纯函数锚）。
    fn hysteresis_step(&mut self, temp_mc: i32) {
        for i in 0..self.trip_count {
            let t = self.trips[i];
            let crossed_up = temp_mc >= t.threshold_mc;
            let held = self.active[i] && temp_mc > t.clr_mc;
            self.active[i] = crossed_up || held;
        }
        self.relatch();
    }

    /// 档重锁 = max(active trip level)。
    fn relatch(&mut self) {
        let mut lv = LEVEL_NOMINAL;
        for i in 0..self.trip_count {
            if self.active[i] {
                let l = self.trips[i].level;
                if l > lv {
                    lv = l;
                }
            }
        }
        self.latched_level = lv;
    }
}

/// step_wise 查表 + 迟滞仲裁引擎（状态 = 传感面锁存态；纯函数面，无时钟无
/// 通道——轮询节拍由 bin 壳驱动）。
#[derive(Debug, Clone)]
pub struct Engine {
    sensors: [SensorState; ENGINE_SENSORS_MAX],
    count: usize,
}

impl Engine {
    pub fn new() -> Self {
        Engine { sensors: [SensorState::empty(0); ENGINE_SENSORS_MAX], count: 0 }
    }

    /// 单拍仲裁：校验（拒面原子性——Err 时状态零动）→ 传感面迟滞步进 →
    /// 三类输出 + JEITA 组装。
    pub fn evaluate(&mut self, tick: &Tick) -> Result<Verdict, BadInput> {
        validate(tick)?;
        // 传感面步进：在报者迟滞更新/新报者入册；停报者**不触不动**（锁存档
        // 保持——传感面失败安全：帽不因传感缺席自动解除）。
        for inp in tick.sensors {
            let idx = match self.find_slot(inp.sensor_id) {
                Some(i) => i,
                None => {
                    let i = self.count;
                    self.sensors[i] = SensorState::empty(inp.sensor_id);
                    self.count += 1;
                    i
                }
            };
            let st = &mut self.sensors[idx];
            if st.table_changed(inp.trips) {
                st.rearm(inp.trips, inp.temp_mc);
            } else {
                st.hysteresis_step(inp.temp_mc);
            }
        }
        Ok(self.build_verdict(tick.battery_temp_mc))
    }

    fn find_slot(&self, sensor_id: u8) -> Option<usize> {
        (0..self.count).find(|&i| self.sensors[i].sensor_id == sensor_id)
    }

    /// 系统档 = 全体在册传感器（含停报保持者）锁存档最大值；三类输出按表
    /// 组装（充电票 = min(JEITA 硬安全档, 性能档)，硬约束优先同 min 语义）。
    fn build_verdict(&self, battery_temp_mc: i32) -> Verdict {
        let mut level = LEVEL_NOMINAL;
        let mut per = [SensorLevel { sensor_id: 0, level: 0 }; ENGINE_SENSORS_MAX];
        for (i, st) in self.sensors[..self.count].iter().enumerate() {
            per[i] = SensorLevel { sensor_id: st.sensor_id, level: st.latched_level };
            if st.latched_level > level {
                level = st.latched_level;
            }
        }
        let zone = jeita_zone(battery_temp_mc);
        let jeita_ma = jeita_limit_ma(zone);
        let lv_ma = level_charge_limit_ma(level);
        Verdict {
            thermal_level: level,
            freq_cap_pct: level_freq_cap_pct(level),
            charge_limit_ma: jeita_ma.min(lv_ma),
            jeita_zone: zone,
            jeita_limit_ma: jeita_ma,
            level_charge_limit_ma: lv_ma,
            per_sensor: per,
            per_sensor_n: self.count as u8,
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

/// 坏输入拒面校验（纯读、零副作用——Err 时引擎状态零动的保证面）。wire 面
/// （pwproto `thermal_ok`）已拒的值域在此双保险：引擎可被非 wire 面调用
/// （sim-thermal/未来真实驱动）。
fn validate(tick: &Tick) -> Result<(), BadInput> {
    if tick.sensors.len() > ENGINE_SENSORS_MAX {
        return Err(BadInput::TooManySensors);
    }
    if !(TEMP_MC_MIN..=TEMP_MC_MAX).contains(&tick.battery_temp_mc) {
        return Err(BadInput::BatteryTempOutOfRange);
    }
    for (i, s) in tick.sensors.iter().enumerate() {
        if s.kind != THERMAL_KIND_REAL && s.kind != THERMAL_KIND_VIRTUAL {
            return Err(BadInput::BadKind);
        }
        if !(TEMP_MC_MIN..=TEMP_MC_MAX).contains(&s.temp_mc) {
            return Err(BadInput::TempOutOfRange);
        }
        if s.trips.len() > THERMAL_TRIPS_MAX {
            return Err(BadInput::TooManyTrips);
        }
        for t in s.trips {
            if t.level > THERMAL_LEVEL_MAX {
                return Err(BadInput::LevelOutOfRange);
            }
            if t.clr_mc > t.threshold_mc {
                return Err(BadInput::ClrAboveThreshold);
            }
        }
        for w in s.trips.windows(2) {
            if w[0].threshold_mc >= w[1].threshold_mc {
                return Err(BadInput::TripsNotAscending);
            }
        }
        // 同拍重复 sensor_id（含与表内容无关的纯 id 撞号）。
        if tick.sensors[..i].iter().any(|p| p.sensor_id == s.sensor_id) {
            return Err(BadInput::DuplicateSensorId);
        }
    }
    Ok(())
}

// ---- 失败安全位（#66 ③「热服务亡→帽保持」）----

/// 输出保持函数——失败安全位的纯函数表达（ADR #66 ③ 原文「热服务亡→帽保持
/// （安全侧=保持限频不自动解除）」）。
///
/// 消费侧（berry-power 帽聚合点 / chargerd 票面 / thermal_level 订阅通知）每
/// 拍以 `update = Some(新裁决)` 调用落新值；`update = None` 表示本拍无新裁决
/// （服务亡/停跳/坏输入拒面）→ **输出 = 前值原样保持**（Option::None 前态 =
/// 服务从未起，无帽可保 = nominal 起始态）。
///
/// 注释锚（不在本函数内执法的共保件）：①频率帽的保持真正执法位 = berry-power
/// 内核聚合点（取 min 聚合器对书写者消亡保持 last-writer 值——区别于 Linux
/// freq QoS 断链自动撤请求，热安全侧刻意不自动解除）；②P11 票 holder 断线
/// 自动撤票是 chargerd 语义（P1 同语义），JEITA 硬安全由充电固件共保（#66 ③
/// 「关机充电态由内核 trip+充电固件共保」）；③critical trip 关机短链在内核
/// （orderly→emergency 同构），独立于本服务存活——smoke 断言 (d) 的锚。
pub fn hold_outputs(prev: Option<Verdict>, update: Option<Verdict>) -> Option<Verdict> {
    match update {
        Some(v) => Some(v),
        None => prev,
    }
}

/// 输出锁存器：服务侧 [`hold_outputs`] 的便利封装（bin 壳用——每拍 commit，
/// 消费面读 current；坏输入拒面 commit(Err) = 保持前值不松帽）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutputLatch {
    last: Option<Verdict>,
}

impl OutputLatch {
    pub fn new() -> Self {
        OutputLatch { last: None }
    }

    /// 落一拍：Ok 提交新裁决；Err（拒面）= 前值保持（拒面即安全面）。
    pub fn commit(&mut self, r: Result<Verdict, BadInput>) -> Option<Verdict> {
        if let Ok(v) = r {
            self.last = Some(v);
        }
        self.last
    }

    /// 当前生效裁决（None = 服务从未产出——nominal 起始态）。
    pub fn current(&self) -> Option<Verdict> {
        self.last
    }

    /// 失败安全读：服务亡/停拍后消费侧应继续施加的输出（= current 原样；
    /// 语义别名，锚 [`hold_outputs`]——帽保持不自动解除）。
    pub fn hold_on_failure(&self) -> Option<Verdict> {
        self.last
    }
}

// ---- host 单测（cargo test -p berry-thermal --target <host>）----
// 盖任务书四面：升降迟滞不抖 / 穿 trip 帽变回落解除 / JEITA 边界 / 四档发布
// / 坏输入拒面（+ 失败安全保持、传感面停报保持、表热换重铺）。

#[cfg(test)]
mod tests {
    use super::*;

    /// 三档梯表（fair 60/55、serious 70/65、critical 80/75，°C→毫摄氏）。
    fn ladder3() -> [ThermalTrip; 3] {
        [
            ThermalTrip { level: 1, threshold_mc: 60_000, clr_mc: 55_000 },
            ThermalTrip { level: 2, threshold_mc: 70_000, clr_mc: 65_000 },
            ThermalTrip { level: 3, threshold_mc: 80_000, clr_mc: 75_000 },
        ]
    }

    /// 单传感器拍评估（传感器 0/real/给定 trips 表）——合法输入必过；传感器
    /// 数组在本函数栈上存活整个 evaluate（输入只借用纪律的测试面形态）。
    fn ev0(
        e: &mut Engine,
        temp_mc: i32,
        trips: &[ThermalTrip],
        battery_temp_mc: i32,
    ) -> Verdict {
        let sensors = [SensorInput {
            sensor_id: 0,
            kind: THERMAL_KIND_REAL,
            temp_mc,
            trips,
        }];
        e.evaluate(&Tick { sensors: &sensors, battery_temp_mc })
            .expect("合法输入必过")
    }

    // ① 升降迟滞不抖：迟滞窗内往返不产生档位抖动（smoke 断言 (c) 纯函数锚）。
    #[test]
    fn hysteresis_no_flap_in_window() {
        let trips = ladder3();
        let mut e = Engine::new();
        // 25°C → nominal；60°C 恰穿 fair 阈 → 1。
        assert_eq!(ev0(&mut e, 25_000, &trips, 25_000).thermal_level, 0);
        assert_eq!(ev0(&mut e, 60_000, &trips, 25_000).thermal_level, 1);
        // 迟滞窗 (55, 60) 内往返 5 轮：全程保持 1（升须 ≥60、降须 ≤55）。
        for temp in [59_000, 56_000, 59_900, 55_100, 57_500] {
            assert_eq!(
                ev0(&mut e, temp, &trips, 25_000).thermal_level,
                1,
                "窗内 temp={} 不抖",
                temp
            );
        }
        // 恰 clr=55 → 解除；恰 clr+1 → 仍保持（<=clr 解除语义钉板）。
        assert_eq!(ev0(&mut e, 55_001, &trips, 25_000).thermal_level, 1);
        assert_eq!(ev0(&mut e, 55_000, &trips, 25_000).thermal_level, 0);
        // 回升 56（窗内）仍 0——升须再穿 60。
        assert_eq!(ev0(&mut e, 56_000, &trips, 25_000).thermal_level, 0);
    }

    // ② 多档迟滞独立位：serious 解除不影响 fair 持锁；高档降穿 clr 落低档。
    #[test]
    fn multi_trip_independent_release() {
        let trips = ladder3();
        let mut e = Engine::new();
        assert_eq!(ev0(&mut e, 80_000, &trips, 25_000).thermal_level, 3);
        // 降 76（serious 窗 (65,70) 外上方？76 > 75 critical 窗内）→ 仍 3。
        assert_eq!(ev0(&mut e, 76_000, &trips, 25_000).thermal_level, 3);
        // 恰 75 → critical 解除；76 在 serious 窗内且 ≥70 → 2。
        assert_eq!(ev0(&mut e, 75_000, &trips, 25_000).thermal_level, 2);
        // 66 在 fair 窗 (55,60) 外上方、serious 窗 (65,70) 内 → 2。
        assert_eq!(ev0(&mut e, 66_000, &trips, 25_000).thermal_level, 2);
        // 60 → fair 仍在（≥60）、serious 解除（60 ≤ 65）→ 1。
        assert_eq!(ev0(&mut e, 60_000, &trips, 25_000).thermal_level, 1);
    }

    // ③ 穿 trip 帽变 / 回落解除：频率帽请求随档变、回落穿 clr 解除回 100。
    #[test]
    fn cap_changes_on_cross_and_releases_on_fall() {
        let trips = ladder3();
        let mut e = Engine::new();
        let v = ev0(&mut e, 25_000, &trips, 25_000);
        assert_eq!((v.thermal_level, v.freq_cap_pct), (0, 100));
        let v = ev0(&mut e, 61_000, &trips, 25_000); // 穿 fair
        assert_eq!((v.thermal_level, v.freq_cap_pct), (1, 70));
        let v = ev0(&mut e, 71_000, &trips, 25_000); // 穿 serious
        assert_eq!((v.thermal_level, v.freq_cap_pct), (2, 45));
        let v = ev0(&mut e, 64_000, &trips, 25_000); // 降穿 serious clr=65
        assert_eq!((v.thermal_level, v.freq_cap_pct), (1, 70));
        let v = ev0(&mut e, 50_000, &trips, 25_000); // 降穿 fair clr=55
        assert_eq!((v.thermal_level, v.freq_cap_pct), (0, 100)); // 帽解除
    }

    // ④ 四档发布：0→1→2→3 逐档发布面（u8 四档钉板）+ 逐档回落的发布序。
    #[test]
    fn four_level_publish_ladder() {
        let trips = ladder3();
        let mut e = Engine::new();
        let seq_rise = [25_000, 60_000, 70_000, 80_000];
        for (i, &t) in seq_rise.iter().enumerate() {
            assert_eq!(ev0(&mut e, t, &trips, 25_000).thermal_level, i as u8);
        }
        let seq_fall = [75_000, 65_000, 55_000];
        for (i, &t) in seq_fall.iter().enumerate() {
            assert_eq!(
                ev0(&mut e, t, &trips, 25_000).thermal_level,
                2 - i as u8
            );
        }
        // critical 档三面同拍：帽 25% + 充电停（性能档 0）+ 发布 3。
        let v = ev0(&mut e, 80_000, &trips, 25_000);
        assert_eq!((v.thermal_level, v.freq_cap_pct, v.charge_limit_ma), (3, 25, 0));
    }

    // ⑤ 多传感器取 max：系统档 = 各传感器档最大值；per_sensor 审计面在册。
    #[test]
    fn multi_sensor_max_aggregation() {
        let trips = ladder3();
        let s0 = SensorInput { sensor_id: 0, kind: THERMAL_KIND_REAL, temp_mc: 25_000, trips: &trips };
        let s1 = SensorInput { sensor_id: 1, kind: THERMAL_KIND_VIRTUAL, temp_mc: 71_000, trips: &trips };
        let tick = Tick { sensors: &[s0, s1], battery_temp_mc: 25_000 };
        let mut e = Engine::new();
        let v = e.evaluate(&tick).expect("合法输入必过");
        assert_eq!(v.thermal_level, 2); // max(0, 2)
        assert_eq!(v.freq_cap_pct, 45);
        assert_eq!(v.per_sensor_n, 2);
        assert_eq!(v.per_sensor[0], SensorLevel { sensor_id: 0, level: 0 });
        assert_eq!(v.per_sensor[1], SensorLevel { sensor_id: 1, level: 2 });
        // s1 停报（缺席）→ 其档保持（传感面失败安全），系统档不回落。
        let v = ev0(&mut e, 25_000, &trips, 25_000);
        assert_eq!(v.thermal_level, 2);
        assert_eq!(v.per_sensor[1].level, 2);
        assert_eq!(v.per_sensor_n, 2); // 在册不退场
    }

    // ⑥ JEITA 边界：五温区边界值（0/10/45/60 °C）逐点钉板 + 停充档。
    #[test]
    fn jeita_boundaries() {
        // (temp_mc, 期望 zone, 期望 mA)
        let cases: [(i32, u8, u32); 12] = [
            (-100_000, JEITA_COLD, 0),    // -100°C 停充
            (-1, JEITA_COLD, 0),          // <0 停充
            (0, JEITA_COOL, 500),         // 恰 0 归 COOL（降流侧）
            (9_999, JEITA_COOL, 500),
            (10_000, JEITA_NORMAL, 1500), // 恰 10 归 NORMAL
            (44_999, JEITA_NORMAL, 1500),
            (45_000, JEITA_WARM, 750),    // 恰 45 归 WARM（降流侧）
            (59_999, JEITA_WARM, 750),
            (60_000, JEITA_HOT, 0),       // 恰 60 归 HOT 停充（保守侧）
            (200_000, JEITA_HOT, 0),      // 可信域上界仍停充
            (i32::MIN, JEITA_COLD, 0),    // 铺底
            (i32::MAX, JEITA_HOT, 0),     // 开到无穷
        ];
        for (t, z, ma) in cases {
            assert_eq!(jeita_zone(t), z, "temp={}", t);
            assert_eq!(jeita_limit_ma(jeita_zone(t)), ma, "temp={}", t);
        }
    }

    // ⑦ 充电限流 = min(JEITA 硬安全档, 性能档)：两轴独立、硬约束优先同 min；
    // 无热侧约束哨兵穿透 min。
    #[test]
    fn charge_limit_is_min_of_jeita_and_level() {
        let trips = ladder3();
        let mut e = Engine::new();
        // nominal + NORMAL 温区：min(1500, 无帽哨兵) = 1500（纯 JEITA 管）。
        let v = ev0(&mut e, 25_000, &trips, 25_000);
        assert_eq!((v.charge_limit_ma, v.level_charge_limit_ma), (1500, CHARGE_NO_CAP_MA));
        // serious（1000）+ WARM（750）：min = 750（JEITA 硬安全压过性能档）。
        let v = ev0(&mut e, 71_000, &trips, 50_000);
        assert_eq!((v.thermal_level, v.jeita_zone, v.charge_limit_ma), (2, JEITA_WARM, 750));
        // fair（1200）+ COOL（500）：min = 500。
        let v = ev0(&mut e, 60_000, &trips, 5_000);
        assert_eq!(v.charge_limit_ma, 500);
        // critical（0）+ NORMAL（1500）：性能档停充更严 → 0。
        let v = ev0(&mut e, 80_000, &trips, 25_000);
        assert_eq!(v.charge_limit_ma, 0);
    }

    // ⑧ 坏输入拒面：全变体 Err + 拒后引擎状态零动（迟滞锁存不被坏拍搬动）。
    #[test]
    fn bad_input_rejected_and_state_untouched() {
        let trips = ladder3();
        let mut e = Engine::new();
        // 先进 fair 持锁（60 穿、58 窗内保持）。
        ev0(&mut e, 60_000, &trips, 25_000);
        ev0(&mut e, 58_000, &trips, 25_000);
        let before = ev0(&mut e, 58_000, &trips, 25_000);
        assert_eq!(before.thermal_level, 1);

        let bad_kind = SensorInput { sensor_id: 0, kind: 7, temp_mc: 25_000, trips: &trips };
        let bad_temp = SensorInput { sensor_id: 0, kind: THERMAL_KIND_REAL, temp_mc: 200_001, trips: &trips };
        let bad_trips_more = [
            ThermalTrip { level: 1, threshold_mc: 40_000, clr_mc: 35_000 },
            ThermalTrip { level: 1, threshold_mc: 50_000, clr_mc: 45_000 },
            ThermalTrip { level: 2, threshold_mc: 60_000, clr_mc: 55_000 },
            ThermalTrip { level: 2, threshold_mc: 70_000, clr_mc: 65_000 },
            ThermalTrip { level: 3, threshold_mc: 80_000, clr_mc: 75_000 }, // 第 5 个越帽
        ];
        let bad_level = [ThermalTrip { level: 4, threshold_mc: 60_000, clr_mc: 55_000 }];
        let bad_clr = [ThermalTrip { level: 1, threshold_mc: 60_000, clr_mc: 60_001 }];
        let bad_unsorted = [
            ThermalTrip { level: 1, threshold_mc: 70_000, clr_mc: 65_000 },
            ThermalTrip { level: 2, threshold_mc: 60_000, clr_mc: 55_000 }, // 阈值逆序
        ];
        let cases: &[(&str, Tick)] = &[
            ("kind 野值", Tick { sensors: &[bad_kind], battery_temp_mc: 25_000 }),
            ("temp 出域", Tick { sensors: &[bad_temp], battery_temp_mc: 25_000 }),
            ("trip 越帽", Tick {
                sensors: &[SensorInput { sensor_id: 0, kind: THERMAL_KIND_REAL, temp_mc: 25_000, trips: &bad_trips_more }],
                battery_temp_mc: 25_000,
            }),
            ("level 越界", Tick {
                sensors: &[SensorInput { sensor_id: 0, kind: THERMAL_KIND_REAL, temp_mc: 25_000, trips: &bad_level }],
                battery_temp_mc: 25_000,
            }),
            ("clr>threshold", Tick {
                sensors: &[SensorInput { sensor_id: 0, kind: THERMAL_KIND_REAL, temp_mc: 25_000, trips: &bad_clr }],
                battery_temp_mc: 25_000,
            }),
            ("阈值非升序", Tick {
                sensors: &[SensorInput { sensor_id: 0, kind: THERMAL_KIND_REAL, temp_mc: 25_000, trips: &bad_unsorted }],
                battery_temp_mc: 25_000,
            }),
            ("电池温出域", Tick {
                sensors: &[SensorInput { sensor_id: 0, kind: THERMAL_KIND_REAL, temp_mc: 25_000, trips: &trips }],
                battery_temp_mc: -100_001,
            }),
            ("同拍重复 id", Tick {
                sensors: &[
                    SensorInput { sensor_id: 0, kind: THERMAL_KIND_REAL, temp_mc: 25_000, trips: &trips },
                    SensorInput { sensor_id: 0, kind: THERMAL_KIND_VIRTUAL, temp_mc: 30_000, trips: &trips },
                ],
                battery_temp_mc: 25_000,
            }),
        ];
        for (name, t) in cases {
            assert!(e.evaluate(t).is_err(), "须拒：{}", name);
        }
        // 传感面越帽（9 件）。
        let many: Vec<SensorInput> = (0..9)
            .map(|i| SensorInput { sensor_id: i, kind: THERMAL_KIND_REAL, temp_mc: 25_000, trips: &trips })
            .collect();
        assert_eq!(e.evaluate(&Tick { sensors: &many, battery_temp_mc: 25_000 }), Err(BadInput::TooManySensors));

        // 拒后状态零动：同参重评 = 拒面前原值（迟滞锁存原样，窗内 58 仍 1）。
        let after = ev0(&mut e, 58_000, &trips, 25_000);
        assert_eq!(after, before);
    }

    // ⑨ 失败安全位：hold_outputs 保持 / None 前态 = nominal 起始；OutputLatch
    // 拒面 commit 不松帽（「服务亡→帽保持」纯函数锚，ADR #66 ③）。
    #[test]
    fn failure_hold_keeps_outputs() {
        let trips = ladder3();
        let mut e = Engine::new();
        let v2 = ev0(&mut e, 71_000, &trips, 25_000); // serious
        assert_eq!((v2.thermal_level, v2.freq_cap_pct), (2, 45));

        // 纯函数：有更新落新；无更新（服务亡/停跳/拒面）= 前值原样。
        let v1 = ev0(&mut e, 60_000, &trips, 25_000);
        assert_eq!(hold_outputs(Some(v2), Some(v1)), Some(v1));
        assert_eq!(hold_outputs(Some(v2), None), Some(v2)); // 帽保持
        assert_eq!(hold_outputs(None, None), None); // 从未起 = 无帽可保

        // 锁存器：commit(Err) 保持前值（拒面即安全面——不松帽）。
        let mut latch = OutputLatch::new();
        assert_eq!(latch.current(), None);
        latch.commit(Ok(v2));
        assert_eq!(latch.current(), Some(v2));
        let sensors_bad = [SensorInput { sensor_id: 0, kind: THERMAL_KIND_REAL, temp_mc: 25_000, trips: &trips }];
        let bad = Tick { sensors: &sensors_bad, battery_temp_mc: 500_000 }; // 电池温出域
        latch.commit(e.evaluate(&bad));
        assert_eq!(latch.hold_on_failure(), Some(v2)); // 仍持 serious 帽
    }

    // ⑩ 表热换重铺：同 id 新表 = 纯穿线重铺（不保持旧锁存），确定性行为钉板。
    #[test]
    fn table_change_rearms_fresh() {
        let trips = ladder3();
        let mut e = Engine::new();
        ev0(&mut e, 60_000, &trips, 25_000); // fair 持锁
        assert_eq!(ev0(&mut e, 58_000, &trips, 25_000).thermal_level, 1);
        // 新表（阈值 90）：58 未穿任何阈值 → 重铺后 nominal。
        let new_trips = [ThermalTrip { level: 1, threshold_mc: 90_000, clr_mc: 85_000 }];
        let v = ev0(&mut e, 58_000, &new_trips, 25_000);
        assert_eq!(v.thermal_level, 0);
        // 新表穿线正常：90 穿 → 1。
        assert_eq!(ev0(&mut e, 90_000, &new_trips, 25_000).thermal_level, 1);
    }

    // ⑪ 空传感面/无 trip 传感器：nominal 起始 + JEITA 独立管充电（两轴独立）。
    #[test]
    fn empty_face_is_nominal_and_jeita_independent() {
        let mut e = Engine::new();
        let empty: [SensorInput; 0] = [];
        let v = e
            .evaluate(&Tick { sensors: &empty, battery_temp_mc: 25_000 })
            .expect("空传感面合法");
        assert_eq!((v.thermal_level, v.freq_cap_pct), (0, 100));
        assert_eq!(v.per_sensor_n, 0);
        // 无 trip 传感器：恒 nominal（遥测件，无档位面）。
        let notrips: [ThermalTrip; 0] = [];
        let v = ev0(&mut e, 99_000, &notrips, 60_000);
        assert_eq!(v.thermal_level, 0);
        assert_eq!(v.jeita_zone, JEITA_HOT); // 电池温 60 独立停充
        assert_eq!(v.charge_limit_ma, 0);
    }

    // ⑫ 越界档保守侧：查表辅助对野档取最严档（防御面，wire/引擎面已拒）。
    #[test]
    fn wild_level_clamps_to_strictest() {
        assert_eq!(level_freq_cap_pct(4), 25);
        assert_eq!(level_freq_cap_pct(255), 25);
        assert_eq!(level_charge_limit_ma(9), 0);
        assert_eq!(jeita_limit_ma(5), 0);
        assert_eq!(jeita_limit_ma(255), 0);
    }
}
