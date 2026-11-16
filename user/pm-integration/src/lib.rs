//! pm-integration —— PM 四 crate 联合场景（全链纯面验证，host 测试面）。
//!
//! ## 定位（CP-08 接线前的全链纯面）
//!
//! berry-power（频率帽聚合点 + 充电限流票 mA 域仲裁）、berry-thermal
//! （step_wise 查表+迟滞 + JEITA 五档）、berry-em（EM 表 + EAS 能量比较
//! 律）、berry-slackface（timer slack 聚合判定律）四件各自独立测试全绿
//! （31+13+8+12），但**从未联合**——四件间的数据流只在 ADR 契约与调度
//! 彻底计划里存在（#66 ②「频率帽汇入 berry-power 同一聚合点取 min」/
//! #66 ⑤「JEITA 票由 berry-thermal 出票」/#68 P11「最少者赢」；CP-08 段
//! `docs/调度彻底计划-v1.2.md:99`）。本 crate 以只读 path 依赖把四件接成
//! 三个端到端场景，验证「上游出→下游收」的跨 crate 数据流正确性：
//!
//! - **场景 A（热降频全链）**：berry-thermal 模拟温度穿 trip → 出
//!   `freq_cap_pct` + `charge_limit_ma` → pct/kHz 桥 → berry-power 三源
//!   聚合 min（热帽/调度 hint/低电）→ 帽值与归因（热帽最紧时 src=thermal）；
//!   berry-em 在帽截 OPP 域上选核 → 选核结果随帽变（prev 反转 + 放不下
//!   回退 None）。
//! - **场景 B（slack+EM 联合）**：六档 slack 域 → 10 定时器分桶 → 唤醒
//!   窗口（fire/window/span）→ 桶批需求对 EM 域容量的检查（唤醒不超容量，
//!   超容 overutil 让位）。
//! - **场景 C（充电票+关机边界）**：JEITA 温区票（mA）→ ChgArbiter 三席
//!   仲裁 → 低电票叠加 → min(三票) + 归因切换 + 不限哨兵穿透 +
//!   critical 停充边界（关机短链在内核，ADR #66 ③——用户态纯面验到
//!   「票 0 + 帽 25% + 发布 3」为止）。
//!
//! ## 两座测试桥（推断标注——桥非裁决）
//!
//! 1. [`thermal_cap_khz`]（pct→kHz）：berry-thermal 出帽面 = `%`（u8，
//!    `user/bins/berry-thermal/src/lib.rs` `Verdict::freq_cap_pct` 注明
//!    「写面 = berry-power 聚合点取 min，本引擎不出通道」），berry-power
//!    聚合面 = kHz（`FreqCapAgg::set`，`user/bins/berry-power/src/cap.rs`）。
//!    pct↔kHz 换算属「内核聚合点写通道 ABI」未立法面（04 号复核 §6-2，
//!    `research/调度彻底计划/04-PM双服务骨架复核.md`；CP-08/CP-10 窗），
//!    本桥 = 测试装配面：`cap_khz = pct × fmax / 100` 向下取整（只收紧），
//!    fmax 取 `PolicyParams::default_sim().fmax_khz`（1GHz，与 berry-em
//!    QEMU 表顶频点同值——`contract_pins` 钉板）。【推断标注】
//! 2. [`cap_limited_domain`]（帽→EM 容量）：模型 = 帽截 OPP 表（帽内最高
//!    频点容量 = 域有效容量——Linux arch_scale「容量随频缩放」的保守
//!    直译；帽下选核在放不下时回退 None 与 EAS 让位负载均衡同构）。真实
//!    EAS 帽接线归 CP-09（验收线「选核决策进遥测 + overutil 回退」）。
//!    【推断标注】
//!
//! 充电票面无桥：F1 终裁后 berry-thermal `charge_limit_ma` 与 berry-power
//! `ChgArbiter::vote` 同为 mA 域直连（哨兵同值同语义，
//! `.agents/notes/implemented/2026-11-13-f1-f2-charge-unit-ma-and-sim-stream-split.md`）。
//!
//! ## 边界与纪律
//!
//! - 四 crate 只读 path 依赖，零改其源；本件不立第二套立法（两桥全推断
//!   标注，接线刀裁走后本件桥随之退役）。
//! - lib 桥面 no_std 兼容（四 crate 同款 `#![cfg_attr(not(test), no_std)]`），
//!   场景全在 `#[cfg(test)]`。
//! - host 测试面：仓根 `.cargo/config.toml` 锁 `build.target =
//!   aarch64-unknown-none`，host 测试须显式 `--target aarch64-apple-darwin`
//!   覆盖（四 crate 头注同款纪律）。
//! - 未接线如实注：justfile test-host 未挂本件（justfile = owner 占用
//!   零碰，04 号复核 F3 同款留窗）；不进 rootfs/产线。

#![cfg_attr(not(test), no_std)]

// ---- 桥 1：热帽 pct → kHz（推断标注，模块注）----

/// 热帽桥产物：`NoCap` = 上游不申请帽（该席撤帽——聚合面写 `None`）；
/// `CapKhz(khz)` = 申请的频率上限（聚合面写 `Some(khz)`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalCap {
    NoCap,
    CapKhz(u32),
}

/// 热帽桥坏输入（拒面——零 panic 路径，四 crate 同款纪律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalCapErr {
    /// `pct == 0`：0 不是频率（`FreqCapAgg::set` 的 `ZeroCap` 拒面前置）。
    ZeroPct,
    /// `pct > 100`：越「无帽值」上界（防御面——v1 档表最高 100 不产此值）。
    PctOutOfRange,
    /// `fmax == 0`：换算无据（EM 未注册形态——`schedutil_freq_khz` 的
    /// `capacity == 0 → None` 同语义）。
    BadFmax,
}

/// 热帽 pct→kHz 桥（测试装配面，非裁决——模块注「两座测试桥」1）：
/// `khz = pct × fmax_khz / 100` 向下取整（只收紧不放宽，限值换算保守侧
/// 纪律——berry-power charge.rs F1 桥同向）；`pct == FREQ_CAP_NONE_PCT`
/// （100）→ `NoCap`（nominal 档不申请帽 = 写面零动作，聚合点无请求即无帽）。
pub fn thermal_cap_khz(pct: u8, fmax_khz: u32) -> Result<ThermalCap, ThermalCapErr> {
    if pct == berry_thermal::FREQ_CAP_NONE_PCT {
        return Ok(ThermalCap::NoCap);
    }
    if pct == 0 {
        return Err(ThermalCapErr::ZeroPct);
    }
    if pct > berry_thermal::FREQ_CAP_NONE_PCT {
        return Err(ThermalCapErr::PctOutOfRange);
    }
    if fmax_khz == 0 {
        return Err(ThermalCapErr::BadFmax);
    }
    // pct ≤ 100 ⇒ 商 ≤ fmax ≤ u32::MAX，u64 中间积零溢出、回 cast 安全。
    let khz = (pct as u64) * (fmax_khz as u64) / 100;
    Ok(ThermalCap::CapKhz(khz as u32))
}

// ---- 桥 2：帽 → EM 域有效容量（推断标注，模块注）----

/// 帽截 OPP 域（测试装配面，非裁决——模块注「两座测试桥」2）：保留
/// `freq_khz ≤ cap_khz` 的频点、经 `em_domain_from` 单源装配（形状执法
/// 不双写）；帽低于最低频点 → [`EmDomain::EMPTY`]（无可用 OPP = 域无据，
/// EAS 回退负载均衡）。子集截断不改频序，形状必过——`unwrap_or` 仅防御
/// 面（零 panic 路径）。
pub fn cap_limited_domain(d: &berry_em::EmDomain, cap_khz: u32) -> berry_em::EmDomain {
    let mut pts = [berry_em::EmPoint::ZERO; berry_em::EM_POINTS_MAX];
    let mut n = 0usize;
    for p in d.points[..d.npoints].iter() {
        if p.freq_khz <= cap_khz {
            pts[n] = *p;
            n += 1;
        }
    }
    if n == 0 {
        return berry_em::EmDomain::EMPTY;
    }
    berry_em::em_domain_from(d.cpu_mask, &pts[..n]).unwrap_or(berry_em::EmDomain::EMPTY)
}

// ---- 场景与共享测试件 ----

#[cfg(test)]
pub mod common;

#[cfg(test)]
mod scenario_a;
#[cfg(test)]
mod scenario_b;
#[cfg(test)]
mod scenario_c;

/// 跨 crate 契约钉板（数据流完整性的常量面——「上游出→下游收」的值域
/// 与席位约定一旦漂移，桥与场景立刻咬）。
#[cfg(test)]
mod contract_pins {
    use berry_em::{QEMU_VIRT_DUAL, EM_POINTS_MAX};
    use berry_power::{PolicyParams, CHG_LIMIT_UNLIMITED_MA, CHG_SLOTS, CAP_SOURCES};
    use berry_thermal::{
        LEVEL_CRITICAL, LEVEL_FREQ_CAP_PCT, THERMAL_LEVEL_MAX, CHARGE_NO_CAP_MA,
    };

    /// F1 并轨锚（跨 crate 值钉）：不限哨兵同值同语义——thermal 出票面
    /// `CHARGE_NO_CAP_MA` 与 power 仲裁面 `CHG_LIMIT_UNLIMITED_MA` 均
    /// u32::MAX、min 组合下穿透。漂移即咬（mA 直连数据流的前提）。
    #[test]
    fn unlimited_sentinel_pinned_across_crates() {
        assert_eq!(CHARGE_NO_CAP_MA, u32::MAX);
        assert_eq!(CHG_LIMIT_UNLIMITED_MA, u32::MAX);
        assert_eq!(CHARGE_NO_CAP_MA, CHG_LIMIT_UNLIMITED_MA);
    }

    /// 桥 1 的 fmax 基与 EM 表顶点一致：`PolicyParams::default_sim().fmax_khz`
    /// == QEMU 双核表最高频点（1GHz）、域容量 1024 == policy.capacity
    /// （pct→kHz 换算基与「帽截容量」模型共用同一张表——两桥自洽前提）。
    #[test]
    fn bridge_fmax_basis_matches_em_table_top() {
        let p = PolicyParams::default_sim();
        assert_eq!(p.fmax_khz, 1_000_000);
        assert_eq!(QEMU_VIRT_DUAL.points[QEMU_VIRT_DUAL.npoints - 1].freq_khz, p.fmax_khz);
        assert_eq!(berry_em::em_max_capacity_of(&QEMU_VIRT_DUAL), p.capacity);
        assert_eq!(p.capacity, 1024);
        assert!(QEMU_VIRT_DUAL.npoints <= EM_POINTS_MAX);
    }

    /// 席位/档位约定钉：thermal 档表四值 [100,70,45,25]、critical = 3 =
    /// `THERMAL_LEVEL_MAX`（pwproto 单源）；帽源席/票席号均在册
    /// （`CAP_SRC_THERMAL`/`CHG_SLOT_JEITA` 各归其位——场景 A/C 的写席
    /// 约定）。
    #[test]
    fn seat_and_level_contracts_pinned() {
        assert_eq!(LEVEL_FREQ_CAP_PCT, [100, 70, 45, 25]);
        assert_eq!(LEVEL_CRITICAL, 3);
        assert_eq!(THERMAL_LEVEL_MAX, 3);
        assert!(berry_power::CAP_SRC_THERMAL < CAP_SOURCES as u8);
        assert!(berry_power::CHG_SLOT_JEITA < CHG_SLOTS as u8);
        // util per-CPU 面（UtilTrack，berry-power util 模块 re-export 自
        // berry-emface 单源）与 EM 双核域同宽——util 消费位与 EM 选核的
        // CPU 编号轴一致。
        assert_eq!(berry_power::util::UTIL_NR_CPU, 2);
        assert_eq!(QEMU_VIRT_DUAL.cpu_mask.count_ones() as usize, berry_power::util::UTIL_NR_CPU);
    }

    /// 桥 1 拒面与取整：pct 0 → ZeroPct、101 → PctOutOfRange、fmax 0 →
    /// BadFmax；33% × 999_999kHz = 329_999.67 → 329999（向下取整只收紧）；
    /// 无帽值 100 → NoCap。
    #[test]
    fn thermal_cap_bridge_rejects_and_floors() {
        use crate::ThermalCap::{self, *};
        use crate::ThermalCapErr::{self, *};
        let r: Result<ThermalCap, ThermalCapErr> = crate::thermal_cap_khz(0, 1_000_000);
        assert_eq!(r, Err(ZeroPct));
        assert_eq!(crate::thermal_cap_khz(101, 1_000_000), Err(PctOutOfRange));
        assert_eq!(crate::thermal_cap_khz(255, 1_000_000), Err(PctOutOfRange));
        assert_eq!(crate::thermal_cap_khz(45, 0), Err(BadFmax));
        assert_eq!(crate::thermal_cap_khz(100, 1_000_000), Ok(NoCap));
        assert_eq!(crate::thermal_cap_khz(45, 1_000_000), Ok(CapKhz(450_000)));
        assert_eq!(crate::thermal_cap_khz(33, 999_999), Ok(CapKhz(329_999)));
    }

    /// 桥 2 截断钉：700kHz 帽留两点（250k/500k，顶容量 750）、450kHz 帽留
    /// 一点（顶容量 400）、低于最低频点 → 空域（无据）；截断面频序保持
    /// （em_domain_from 形状执法过）。
    #[test]
    fn cap_limited_domain_truncates() {
        let d700 = crate::cap_limited_domain(&QEMU_VIRT_DUAL, 700_000);
        assert_eq!(d700.npoints, 2);
        assert_eq!(berry_em::em_max_capacity_of(&d700), 750);
        let d450 = crate::cap_limited_domain(&QEMU_VIRT_DUAL, 450_000);
        assert_eq!(d450.npoints, 1);
        assert_eq!(berry_em::em_max_capacity_of(&d450), 400);
        assert_eq!(crate::cap_limited_domain(&QEMU_VIRT_DUAL, 249_999), berry_em::EmDomain::EMPTY);
        // 恰在频点上含本点（半开区间右侧闭——帽=频点不截掉该点）。
        assert_eq!(crate::cap_limited_domain(&QEMU_VIRT_DUAL, 500_000).npoints, 2);
    }
}
