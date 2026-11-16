//! berry-power —— 电源中枢服务纯函数核（调度彻底计划「调度-PM 契约
//! 空转」接缝的服务侧本体骨架）。
//!
//! 语境（docs/调度彻底计划-v1.2.md §3 裁九/§4.1 CP-08/§4.2 CP-10）：
//! ADR #66/#67/#68 立法三件齐备而 **berry-power 服务本体未建** =
//! 三大接缝之二「util/placement/EM 有产无消」的最大空转机制
//! （research/调度彻底计划/01-调度现状全景与在途线收编.md §2.4）。本
//! crate = 该本体的 lib 纯面 + bin 壳（`src/main.rs` 循环骨架——常驻/
//! svcmgr 挂接/真实通道全留窗注释，见其头注窗表）。
//!
//! 四件核心面：
//!
//! 1. [`cap`]——**频率帽聚合点**：多源帽（thermal/调度 hint/低电）→ 取
//!    min → 唯一输出。**帽不新开通道**执法锚 ADR #66 ②（频率帽汇入
//!    berry-power 同一内核聚合点取 min = 唯一合法帽书写面；对调度器 =
//!    系统态叠加票 #42 只叠加）。
//! 2. [`charge`]——**充电限流票仲裁**：JEITA 票 + 低电票 + 用户帽 → min
//!    （chargerd 契约位；ADR #68 `CHG_OP_VOTE_LIMIT{limit_pct, reason:u8}`
//!    最少者赢/断线自动撤票/chargerd 持有；JEITA 出票方 = berry-thermal，
//!    ADR #66 ⑤）。**票面单位 = mA（F1 终裁，04 号复核 §6-3：#66 电流版
//!    为终态形态；#68 pct 面 = 冻结快照，经设计容量桥兼容读写——
//!    `charge::{pct_of_ma, ma_of_pct}`，升版改口）**。
//! 3. [`util`]——**util EWMA 消费位**：输入口 + 衰减算式（折叠数学单源
//!    `berry-emface::ewma_fold`，零第二实现）+ schedutil 同构
//!    `f ≈ util × fmax / capacity`（CP-08 验收线原文）。
//! 4. [`session`]——**berry-pwproto 帧接收/应答**（loopback 可测；协议
//!    单源 user/berry-pwproto，ADR #68 ③ 零新 syscall 载体）。
//!
//! [`PowerCore`] 把四件组装成可驱动的裁决机：sim 快照字节 → 会话/策略
//! （battery 喂低电策略；thermal/modem 只进遥测——**本服务不养第二套热
//! 表**，热帽/JEITA 票归 berry-thermal，ADR #66 边界）+ util 窗账 →
//! [`PowerCore::decide`] 出统一裁决（帽/票/频率）。策略参数 =
//! [`PolicyParams`]（推断标注见字段注）。
//!
//! 纯函数纪律（berry-pwproto/berry-emface 同款）：零分配、零 unsafe、
//! 零 panic 路径（坏输入一律 Result/Option 拒面）；no_std（test 例外）。

#![cfg_attr(not(test), no_std)]

pub mod cap;
pub mod charge;
pub mod hint;
pub mod session;
pub mod util;

pub use berry_pwproto::{BatterySnapshot, Snapshot};

pub use cap::{CapDecision, CapErr, FreqCapAgg, CAP_SOURCES, CAP_SRC_LOWBATT, CAP_SRC_SCHEDHINT, CAP_SRC_THERMAL};
pub use charge::{
    ma_of_pct, pct_of_ma, ChgArbiter, ChgDecision, ChgErr, CHG_LIMIT_UNLIMITED_MA,
    CHG_LIMIT_UNLIMITED_PCT, CHG_REASON_JEITA, CHG_REASON_LOWBATT, CHG_REASON_NONE,
    CHG_REASON_USER, CHG_SLOT_JEITA, CHG_SLOT_LOWBATT, CHG_SLOT_USER, CHG_SLOTS,
};
pub use session::{SessEvent, PwSession};
pub use util::{schedutil_freq_khz, UtilTrack};
// v3 四动作 hint 产面（本刀）：①placement/②frequency/③weight/④EPP 一次
// 读全——①③④经 policy_bind 表条目字段注入、②PM 域自家裁决（`hint`
// 模块注定谳）。
pub use hint::{
    action_hints, epp_flags, weight_delta, ActionHints, THERMAL_EFF_LEVEL, UTIL_HI, UTIL_LO,
    WEIGHT_STEP,
};

/// 策略参数（**全部推断标注**——初值 + 真机/KC 循环定标窗，ADR #68 ⑥
/// 「初值+策略表项可调+遥测导出」口径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyParams {
    /// 低电帽阈值：`capacity_pct ≤ 此值` → 低电频率帽生效。
    /// 推断标注：15 = Android 低电警告档（PowerManager）同档位初值。
    pub lowbatt_cap_pct: u8,
    /// 低电频率帽（kHz）。推断标注：fmax 之半初值（保关机储备 + 摊薄
    /// 峰值功耗）；**0 = 禁用该源**（帽面 `Some(0)` 拒 = 拒面一致）。
    pub lowbatt_cap_khz: u32,
    /// 深亏电预充阈值：`capacity_pct ≤ 此值` → 低电票生效（锂电深亏
    /// 预充限流）。推断标注：10% 初值。
    pub precharge_pct: u8,
    /// 深亏电预充票面（mA，F1 终裁单位）。推断标注：2400 初值 = 0.5C
    /// 预充惯例（设计容量 sim 基准 4800 mAh 的 C 率换算——原 50% 快照值
    /// 的 mA 终裁形；真机设计容量定标随策略表可调窗，ADR #68 ⑥）。
    pub precharge_limit_ma: u32,
    /// 频率域顶（kHz）。推断标注：QEMU 期虚拟基准 1GHz（virt 无 DVFS
    /// 真表——真机 EM/OPP 随 CP-09/CP-18 落）。
    pub fmax_khz: u32,
    /// 域容量（0..=1024 尺度，schedutil 分母）。推断标注：1024 满格
    /// （无大小核拓扑的 QEMU 期口径）。
    pub capacity: u32,
}

impl PolicyParams {
    /// QEMU/sim 期默认参数（各值推断标注见字段注）。
    pub const fn default_sim() -> Self {
        PolicyParams {
            lowbatt_cap_pct: 15,
            lowbatt_cap_khz: 500_000,
            precharge_pct: 10,
            precharge_limit_ma: 2_400,
            fmax_khz: 1_000_000,
            capacity: util::UTIL_FULL,
        }
    }
}

/// 频率裁决：schedutil 请求帽前请求后（eff = min(req, 生效帽)；无帽 =
/// req）。**唯一输出面纪律**：内核聚合点写面只吃 eff（帽不新开通道，
/// ADR #66 ②——req 是 governor 意图，eff 才是可写频率）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreqVerdict {
    pub req_khz: u32,
    pub eff_khz: u32,
}

/// 统一裁决快照（帽 + 票 + 频率——每帧应用后/每 util 窗后的全量态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    pub cap: Option<CapDecision>,
    pub chg: ChgDecision,
    pub freq: FreqVerdict,
}

/// 会话帧级事件（bin 壳循环的驱动事件——ACK 回写 + 裁决快照一次交付）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreEvent {
    /// 帧未收齐（通道静默——常驻循环里即继续收）。
    Idle,
    /// 帧应用：策略面已更新，携带新裁决（ACK = `ST_OK`）。
    Applied { dev: u8, seq: u16, verdict: Verdict },
    /// 帧拒（BAD/STALE——策略面零改动；ACK 照发诊断）。
    Rejected { dev: u8, seq: u16, st: u8 },
}

/// berry-power 裁决机：四件纯面的组装（策略应用 + 裁决读出）。
#[derive(Debug, Clone)]
pub struct PowerCore {
    /// 策略参数（推断标注——PolicyParams 字段注）。
    pub policy: PolicyParams,
    /// 面①：频率帽聚合点（唯一书写面）。
    pub caps: FreqCapAgg,
    /// 面②：充电限流票仲裁（chargerd 契约位）。
    pub votes: ChgArbiter,
    /// 面③：util EWMA 消费位（per-CPU）。
    pub util: UtilTrack,
    /// 面④：pwproto 帧会话机。
    pub sess: PwSession,
    // ---- 遥测摘要（决策不消费的温度/尾能只进遥测——不进契约，ADR #66 ④ 同向纪律）----
    /// 最近电池快照（低电策略已折叠进 caps/votes，原值留遥测）。
    pub last_batt: Option<BatterySnapshot>,
    /// 最近热档提示（遥测 only——热帽/JEITA 归 berry-thermal，窗注释见
    /// `apply_snapshot`）。
    pub thermal_level: Option<u8>,
    /// 最近 modem 尾能窗（遥测 only——PM-05 尾能面，消费随维护窗刀）。
    pub modem_tail_ms: Option<u32>,
}

impl PowerCore {
    /// 新建（给定策略；全源无帽/无票/EWMA 0）。
    pub fn new(policy: PolicyParams) -> Self {
        PowerCore {
            policy,
            caps: FreqCapAgg::new(),
            votes: ChgArbiter::new(),
            util: UtilTrack::new(),
            sess: PwSession::new(),
            last_batt: None,
            thermal_level: None,
            modem_tail_ms: None,
        }
    }

    /// sim 通道字节输入口（面④入口 + 策略应用 + 裁决快照一次走完）。
    /// 真实通道形态（fd0/gift/wait 长帽重臂）= 窗注释（main.rs 窗 2）。
    pub fn push_byte(&mut self, b: u8) -> CoreEvent {
        match self.sess.push(b) {
            SessEvent::NeedMore => CoreEvent::Idle,
            SessEvent::Applied { dev, seq, snap } => {
                self.apply_snapshot(&snap);
                CoreEvent::Applied { dev, seq, verdict: self.decide() }
            }
            SessEvent::Rejected { dev, seq, st } => CoreEvent::Rejected { dev, seq, st },
        }
    }

    /// util 窗账输入口（面③入口；CP-08 环 ABI 留窗——main.rs 窗 3）。
    /// 拒面同 [`UtilTrack::ingest_window`]（cpu 越界/elapsed=0 → None）。
    pub fn ingest_util_window(&mut self, cpu: usize, accum: u64, elapsed: u64) -> Option<u32> {
        self.util.ingest_window(cpu, accum, elapsed)
    }

    /// v3 四动作 hint 读出（①placement/②frequency/③weight/④EPP 一次
    /// 读全——纯读、零副作用）：`thermal_level` = sim 热帧遥测折叠
    /// （`apply_snapshot` 落位；None = 尚无热帧 → ④ 无意见），util/policy
    /// 参数同 [`Self::decide`] 消费面。bin 壳据此组 `policy_bind` 表条目
    /// （①③④字段）与 PM 域频率裁决（②——`hint` 模块注入形态定谳）。
    pub fn action_hints(&self) -> hint::ActionHints {
        hint::action_hints(
            &self.util,
            self.thermal_level,
            self.policy.fmax_khz,
            self.policy.capacity,
        )
    }

    /// 裸 util 样本输入口（单核直投面——快照载荷的逐核分解形态；
    /// 拒面同 [`UtilTrack::ingest_sample`]）。
    pub fn ingest_util_sample(&mut self, cpu: usize, util: u32) -> Option<u32> {
        self.util.ingest_sample(cpu, util)
    }

    /// 全核 util 快照输入口（面③通道载荷面——CP-08 首环已裁：快照
    /// syscall `sched_util_snapshot` 0x40 pull 读，拒面全或无同
    /// [`UtilTrack::ingest_snapshot`]）。
    pub fn ingest_util_snapshot(&mut self, util: &[u32]) -> Option<()> {
        self.util.ingest_snapshot(util)
    }

    /// 统一裁决读出（帽/票/频率——纯读、零副作用、幂等）。
    ///
    /// 频率侧：域内最大 EWMA（v1 单频域保守侧——util 模块注）→ schedutil
    /// 请求 → 生效帽 min。`capacity == 0`（EM 未注册形态）→ 请求 0 =
    /// 无据不求频（保守静默——真机 EM 随 CP-09）。
    pub fn decide(&self) -> Verdict {
        let cap = self.caps.aggregate();
        let chg = self.votes.arbitrate();
        let req =
            schedutil_freq_khz(self.util.max_est(), self.policy.fmax_khz, self.policy.capacity)
                .unwrap_or(0);
        let eff = match cap {
            Some(c) => req.min(c.cap_khz),
            None => req,
        };
        Verdict { cap, chg, freq: FreqVerdict { req_khz: req, eff_khz: eff } }
    }

    /// 快照策略应用：battery → 低电帽/低电票（电源域自有策略）；thermal/
    /// modem → 遥测 only。
    ///
    /// **窗注释（热策略边界，ADR #66）**：sim 热快照不旁路出热帽/JEITA
    /// 票——真实环境 berry-thermal 订阅热驱动自报面出票、经聚合点/票席
    /// 投递（「sim-thermal = 普通热驱动走同一自报接口，脚本注入不旁路」
    /// #66 ⑥；本服务不养第二套热表）。modem 尾能消费（维护窗判定用
    /// SLEPT 连续钟，ADR #67 ②）挂维护窗刀。
    fn apply_snapshot(&mut self, snap: &Snapshot) {
        match snap {
            Snapshot::Battery(b) => {
                self.last_batt = Some(*b);
                let lowbatt = b.capacity_pct <= self.policy.lowbatt_cap_pct;
                // 源席/票面常量构造性合法（set/vote 拒面零触发——lowbatt
                // _cap_khz = 0 形态 = 显式禁用该源，字段注口径）。
                let _ = self.caps.set(
                    CAP_SRC_LOWBATT,
                    if lowbatt { Some(self.policy.lowbatt_cap_khz) } else { None },
                );
                if b.capacity_pct <= self.policy.precharge_pct {
                    let _ = self.votes.vote(
                        CHG_SLOT_LOWBATT,
                        self.policy.precharge_limit_ma,
                        CHG_REASON_LOWBATT,
                    );
                } else {
                    let _ = self.votes.withdraw(CHG_SLOT_LOWBATT);
                }
            }
            Snapshot::Thermal(t) => {
                self.thermal_level = Some(t.level_hint);
            }
            Snapshot::Modem(m) => {
                self.modem_tail_ms = Some(m.tail_ms);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use berry_pwproto::{
        encode_battery, encode_modem, encode_thermal, BatterySnapshot, ModemSnapshot, ThermalSensor,
        ThermalSnapshot, ThermalTrip, THERMAL_SENSORS_MAX, THERMAL_TRIPS_MAX, DEV_BATTERY,
        DEV_MODEM, DEV_THERMAL, ST_BAD, ST_OK, ST_STALE,
    };

    fn battery(pct: u8) -> BatterySnapshot {
        BatterySnapshot {
            present: 1,
            ac_online: 1,
            status: 2,
            health: 3,
            capacity_pct: pct,
            voltage_mv: 3990,
            temp_mc: 28_500,
            current_now_ua: 412_000,
            current_avg_ua: 395_000,
            charge_full_uah: 4_800_000,
            charge_counter_uah: 3_696_000,
            cycle_count: 143,
            timestamp_ns: 42,
            measured: 1,
        }
    }

    fn thermal(level: u8) -> ThermalSnapshot {
        let sen = ThermalSensor {
            kind: berry_pwproto::THERMAL_KIND_REAL,
            temp_mc: 31_200,
            trip_count: 0,
            trips: [ThermalTrip { level: 0, threshold_mc: 0, clr_mc: 0 }; THERMAL_TRIPS_MAX],
        };
        ThermalSnapshot { sensor_count: 1, sensors: [sen; THERMAL_SENSORS_MAX], level_hint: level }
    }

    fn modem() -> ModemSnapshot {
        ModemSnapshot { rrc_state: 5, tail_ms: 8_000, signal_tier: 2, in_maint_window: 0 }
    }

    /// 喂一帧（编码器产线——会话/策略/裁决全链）。
    fn feed_frame(core: &mut PowerCore, bytes: &[u8]) -> Vec<CoreEvent> {
        let mut evs = Vec::new();
        for &b in bytes {
            let ev = core.push_byte(b);
            if !matches!(ev, CoreEvent::Idle) {
                evs.push(ev);
            }
        }
        evs
    }

    /// 低电策略主链：健康电池（77%）→ 无低电帽/无票；深亏电（8%）→
    /// 低电帽 500kHz 生效 + 低电票 2400mA 生效（< 低电帽阈值 15 与预充阈值
    /// 10 双触发）；回升（40%）→ 帽/票双撤。
    #[test]
    fn battery_policy_lifecycle() {
        let mut core = PowerCore::new(PolicyParams::default_sim());
        let mut buf = [0u8; 128];

        let n = encode_battery(&mut buf, 1, &battery(77), 0).unwrap();
        let evs = feed_frame(&mut core, &buf[..n]);
        match &evs[0] {
            CoreEvent::Applied { dev, verdict, .. } => {
                assert_eq!(*dev, DEV_BATTERY);
                assert_eq!(verdict.cap, None, "健康电池无低电帽");
                assert_eq!(verdict.chg.reason, CHG_REASON_NONE, "无票不限");
            }
            e => panic!("{e:?}"),
        }

        let n = encode_battery(&mut buf, 2, &battery(8), 0).unwrap();
        let evs = feed_frame(&mut core, &buf[..n]);
        match &evs[0] {
            CoreEvent::Applied { verdict, .. } => {
                assert_eq!(
                    verdict.cap,
                    Some(CapDecision { cap_khz: 500_000, src: CAP_SRC_LOWBATT })
                );
                assert_eq!(
                    verdict.chg,
                    ChgDecision { limit_ma: 2_400, reason: CHG_REASON_LOWBATT }
                );
            }
            e => panic!("{e:?}"),
        }
        assert_eq!(core.last_batt.unwrap().capacity_pct, 8);

        let n = encode_battery(&mut buf, 3, &battery(40), 0).unwrap();
        let evs = feed_frame(&mut core, &buf[..n]);
        match &evs[0] {
            CoreEvent::Applied { verdict, .. } => {
                assert_eq!(verdict.cap, None, "回升撤帽");
                assert_eq!(verdict.chg.reason, CHG_REASON_NONE, "回升撤票");
            }
            e => panic!("{e:?}"),
        }
    }

    /// 边界档：恰在阈值上（15%）触发低电帽（≤ 语义）；16% 不触发；恰在
    /// 预充阈值（10%）双触发。
    #[test]
    fn battery_policy_threshold_edges() {
        let mut core = PowerCore::new(PolicyParams::default_sim());
        let mut buf = [0u8; 128];
        let mut seq = 0u16;

        for (pct, want_cap, want_vote) in
            [(16u8, false, false), (15, true, false), (11, true, false), (10, true, true)]
        {
            seq += 1;
            let n = encode_battery(&mut buf, seq, &battery(pct), 0).unwrap();
            let evs = feed_frame(&mut core, &buf[..n]);
            match &evs[0] {
                CoreEvent::Applied { verdict, .. } => {
                    assert_eq!(verdict.cap.is_some(), want_cap, "pct={pct} 帽");
                    let voted = verdict.chg.reason == CHG_REASON_LOWBATT;
                    assert_eq!(voted, want_vote, "pct={pct} 票");
                }
                e => panic!("{e:?}"),
            }
        }
    }

    /// 帽-频率联合：thermal 帽（模拟 berry-thermal 经聚合点投递——测试直
    /// set 源席）压过 schedutil 请求 → eff = min；帽撤 → eff 回 req。
    #[test]
    fn cap_clamps_schedutil_request() {
        let mut core = PowerCore::new(PolicyParams::default_sim());
        // 铺 util：满忙样本 → EWMA 128（1024>>3）→ req = 128×1e6/1024 = 125000。
        core.ingest_util_sample(0, 1024).unwrap();
        let v = core.decide();
        assert_eq!(v.freq.req_khz, 125_000);
        assert_eq!(v.freq.eff_khz, 125_000, "无帽 = req");
        // 热帽 100kHz 更严 → eff = 100kHz（帽聚合点压过 governor 请求）。
        core.caps.set(CAP_SRC_THERMAL, Some(100_000)).unwrap();
        assert_eq!(core.decide().freq.eff_khz, 100_000);
        assert_eq!(core.decide().cap.unwrap().src, CAP_SRC_THERMAL);
        // 调度 hint 帽 60kHz 更严 → min 生效源换席。
        core.caps.set(CAP_SRC_SCHEDHINT, Some(60_000)).unwrap();
        let v = core.decide();
        assert_eq!((v.freq.eff_khz, v.cap.unwrap().src), (60_000, CAP_SRC_SCHEDHINT));
        // 双撤 → eff 回 req。
        core.caps.set(CAP_SRC_THERMAL, None).unwrap();
        core.caps.set(CAP_SRC_SCHEDHINT, None).unwrap();
        assert_eq!(core.decide().freq.eff_khz, 125_000);
    }

    /// EM 缺席形态：capacity=0 → 频率请求 0（无据不求频——保守静默）；
    /// 帽/票裁决不受影响。
    #[test]
    fn em_absent_zero_request() {
        let mut p = PolicyParams::default_sim();
        p.capacity = 0;
        let mut core = PowerCore::new(p);
        core.ingest_util_sample(0, 1024).unwrap();
        let v = core.decide();
        assert_eq!(v.freq, FreqVerdict { req_khz: 0, eff_khz: 0 });
        assert_eq!(v.chg.limit_ma, CHG_LIMIT_UNLIMITED_MA);
    }

    /// 会话拒帧不触策略：STALE 重放帧（同 seq battery）与 BAD 帧后，低电
    /// 帽/票保持旧值（坏输入拒面在组装层的钉板）。
    #[test]
    fn rejected_frames_leave_policy_untouched() {
        let mut core = PowerCore::new(PolicyParams::default_sim());
        let mut buf = [0u8; 128];
        let n = encode_battery(&mut buf, 5, &battery(8), 0).unwrap();
        feed_frame(&mut core, &buf[..n]);
        assert_eq!(core.decide().chg.reason, CHG_REASON_LOWBATT);
        // STALE 重放（seq=5 不推进）。
        let evs = feed_frame(&mut core, &buf[..n]);
        assert_eq!(evs, vec![CoreEvent::Rejected { dev: DEV_BATTERY, seq: 5, st: ST_STALE }]);
        assert_eq!(core.decide().chg.reason, CHG_REASON_LOWBATT, "拒帧零策略改动");
        // 手写 BAD 帧（capacity=101）。
        let mut bad = [0u8; 64];
        bad[..4].copy_from_slice(&berry_pwproto::MAGIC.to_le_bytes());
        bad[4] = DEV_BATTERY;
        bad[5] = berry_pwproto::VER;
        bad[6..8].copy_from_slice(&6u16.to_le_bytes());
        bad[berry_pwproto::HEADER_LEN] = 1;
        bad[berry_pwproto::HEADER_LEN + 4] = 101;
        let total = berry_pwproto::HEADER_LEN + berry_pwproto::BATTERY_PAYLOAD_LEN + 4;
        let evs = feed_frame(&mut core, &bad[..total]);
        assert_eq!(evs, vec![CoreEvent::Rejected { dev: DEV_BATTERY, seq: 6, st: ST_BAD }]);
        assert_eq!(core.last_batt.unwrap().capacity_pct, 8, "BAD 不入遥测");
        assert_eq!(core.decide().cap.unwrap().cap_khz, 500_000);
    }

    /// thermal/modem 帧只进遥测（不出热帽/不出 JEITA 票——ADR #66 边界的
    /// 组装层钉板：sim 快照不旁路出票）。
    #[test]
    fn thermal_modem_telemetry_only() {
        let mut core = PowerCore::new(PolicyParams::default_sim());
        let mut buf = [0u8; 128];
        let n = encode_thermal(&mut buf, 1, &thermal(3), 1).unwrap();
        let evs = feed_frame(&mut core, &buf[..n]);
        assert!(matches!(evs[0], CoreEvent::Applied { dev: DEV_THERMAL, .. }));
        let n = encode_modem(&mut buf, 1, &modem(), 0).unwrap();
        let evs = feed_frame(&mut core, &buf[..n]);
        assert!(matches!(evs[0], CoreEvent::Applied { dev: DEV_MODEM, .. }));
        assert_eq!(core.thermal_level, Some(3));
        assert_eq!(core.modem_tail_ms, Some(8_000));
        let v = core.decide();
        assert_eq!(v.cap, None, "热档不旁路出帽");
        assert_eq!(v.chg.reason, CHG_REASON_NONE, "热档不旁路出 JEITA 票");
    }

    /// util 输入口 + ACK 面联合小钉：ST_OK 事件经 session 面 ack_of 可回
    /// （CoreEvent → SessEvent 语义一致——Applied ⇒ OK、Rejected ⇒ 坏码）。
    #[test]
    fn core_events_map_to_ack_states() {
        let mut core = PowerCore::new(PolicyParams::default_sim());
        let mut buf = [0u8; 128];
        let n = encode_battery(&mut buf, 9, &battery(50), 0).unwrap();
        let evs = feed_frame(&mut core, &buf[..n]);
        let mut ack = [0u8; 9];
        match evs[0] {
            CoreEvent::Applied { dev, seq, .. } => {
                let m = PwSession::ack_of(
                    &mut ack,
                    &SessEvent::Applied { dev, seq, snap: Snapshot::Battery(battery(50)) },
                )
                .unwrap();
                assert_eq!(m, 9);
                assert_eq!(berry_pwproto::decode_ack(&ack), Some((dev, seq, ST_OK)));
            }
            ref e => panic!("{e:?}"),
        }
    }

    /// 组装层 util 拒面透传：cpu 越界/elapsed=0 → None 且裁决不受污染。
    #[test]
    fn core_util_rejects_passthrough() {
        let mut core = PowerCore::new(PolicyParams::default_sim());
        assert_eq!(core.ingest_util_window(99, 1, 1), None);
        assert_eq!(core.ingest_util_window(0, 1, 0), None);
        assert_eq!(core.ingest_util_sample(0, 1025), None);
        assert_eq!(core.decide().freq.req_khz, 0);
    }
}
