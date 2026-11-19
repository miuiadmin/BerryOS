//! berry-pwproto 帧会话机 —— 接收/应答（lib 核面 ④，loopback 可测）。
//!
//! 契约单源 = `user/berry-pwproto`（sim-S1 冻结：原子快照帧 + 9B ACK +
//! `StreamDecoder` 流式再同步；ADR #68 ③ SDK 面零新 syscall 的协议载体）。
//! 本面 = berry-power 侧会话状态机：逐字节喂入 → 帧事件（应用/拒）→
//! ACK 线帧——**loopback 可测**：host 上编码器产帧 → [`PwSession::push`]
//! → [`ack_of`] 产 9B 回帧 → `berry_pwproto::decode_ack` 对拍，全链不
//! 出纯函数面。
//!
//! seq 语义（simbattery 先例同款——同一根线、同一判据，不长第二套会话
//! 规则）：per-dev `last_seq` 自持，**严格递增**才应用；回退/重放 →
//! [`ST_STALE`]（不应用、ACK 照发）。杂音/未知 dev 的再同步语义归
//! `StreamDecoder`（魔数扫描），头合法帧收齐但语义拒（ver/值域）→
//! [`ST_BAD`]——「没收到」（无 ACK）与「收到但拒」（BAD ACK）线上可分。
//!
//! 接受面（**F2 终裁：sim 流按设备分端**——04 号复核 §6-4/§7-F2 裁决）：
//! 同一 UART1 字节流（task_simcon 转发）消费权按设备分流——`DEV_BATTERY`
//! 归 simbattery（标记面，现状）、`DEV_THERMAL` 归 berry-thermal（Tick
//! 组装订阅面）、`DEV_MODEM` 归 berry-power（遥测）——本服务在 sim 流上
//! 的份额 = modem 遥测，**不主张全流消费权**（双消费者分帧错乱的裁决
//! 答案；分端执法归通道拓扑接线刀）。会话机本身 = pwproto 通用消费面：
//! 三设备帧均可解（真机相 battery 策略输入 = 电池驱动自报经同一会话机；
//! host 测试/合成流喂全三设备 = loopback 面，非 sim 流消费权主张）；seq
//! 追踪 per-dev 独立（宿主注入端三设备各自单调，互不牵连——pwproto 头注
//! seq 语义）。
//!
//! **窗注释（真实通道）**：QEMU 期真线 = svcmgr 预拉 + fd0 帧通道
//! （task_simcon 转发 UART1 字节流）+ gift ACK 回程端——形态照
//! simbattery（recv_blocking 长帽重臂 + send_retry）；本面只持会话状态，
//! 通道/等待/挂接全留窗（见 main.rs 窗 2）。

use berry_pwproto::{
    encode_ack, StreamDecoder, Step, Snapshot, DEV_BATTERY, DEV_MODEM, DEV_THERMAL, ST_BAD,
    ST_OK, ST_STALE,
};

/// 会话产出事件（一帧最多一事件；ACK 编码信息随事件携带）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessEvent {
    /// 帧未收齐（含流式再同步吞位——静默）。
    NeedMore,
    /// 帧通过（seq 严格递增）→ 快照移交策略面（ACK = `ST_OK`）。
    Applied { dev: u8, seq: u16, snap: Snapshot },
    /// 帧收齐但拒：值域/ver → `ST_BAD`；seq 回退/重放 → `ST_STALE`
    /// （快照不应用，ACK 照发——宿主可分诊断）。
    Rejected { dev: u8, seq: u16, st: u8 },
}

/// dev 号 → seq 槽位（`DEV_BATTERY`=1→0 … `DEV_MODEM`=3→2；未知 dev 不达
/// 本面——StreamDecoder 杂音再同步吞掉）。
fn dev_slot(dev: u8) -> Option<usize> {
    match dev {
        DEV_BATTERY => Some(0),
        DEV_THERMAL => Some(1),
        DEV_MODEM => Some(2),
        _ => None,
    }
}

/// berry-pwproto 帧会话机（流式解码 + per-dev seq 状态）。
#[derive(Debug, Clone)]
pub struct PwSession {
    dec: StreamDecoder,
    last_seq: [Option<u16>; 3],
}

impl PwSession {
    /// 新建（空累积、无 seq 史）。
    pub fn new() -> Self {
        PwSession { dec: StreamDecoder::new(), last_seq: [None; 3] }
    }

    /// 喂一字节（通道字节流的逐字节入口）。零 panic 路径：解码器语义拒
    /// 与未知 dev 全走拒/静默分支。
    pub fn push(&mut self, b: u8) -> SessEvent {
        match self.dec.push(b) {
            Step::NeedMore => SessEvent::NeedMore,
            Step::Frame(f) => {
                let slot = match dev_slot(f.dev) {
                    Some(s) => s,
                    // 防御面（理论不可达——StreamDecoder 只 yield 已知 dev）：
                    // 按语义拒处理，不 panic。
                    None => {
                        return SessEvent::Rejected { dev: f.dev, seq: f.seq, st: ST_BAD };
                    }
                };
                if self.last_seq[slot].map_or(true, |l| f.seq > l) {
                    self.last_seq[slot] = Some(f.seq);
                    SessEvent::Applied { dev: f.dev, seq: f.seq, snap: f.snap }
                } else {
                    SessEvent::Rejected { dev: f.dev, seq: f.seq, st: ST_STALE }
                }
            }
            Step::Bad { dev, seq } => SessEvent::Rejected { dev, seq, st: ST_BAD },
        }
    }

    /// 某设备当前 `last_seq`（遥测/测试面；未知 dev → `None`）。
    pub fn last_seq(&self, dev: u8) -> Option<u16> {
        dev_slot(dev).and_then(|s| self.last_seq[s])
    }

    /// 事件 → 9B ACK 线帧（回写面的纯函数半边：真实通道的 SEND 端重试
    /// 形态留窗，见 main.rs 窗 2/窗 4）。`NeedMore` → `None`（无帧无应答）；
    /// 编码失败（缓冲 < 9B，`berry_pwproto::ACK_LEN`）→ `None`（调用面
    /// 自查容量——测试面用恰长缓冲钉板）。
    pub fn ack_of(out: &mut [u8], ev: &SessEvent) -> Option<usize> {
        match ev {
            SessEvent::NeedMore => None,
            SessEvent::Applied { dev, seq, .. } => encode_ack(out, *dev, *seq, ST_OK),
            SessEvent::Rejected { dev, seq, st } => encode_ack(out, *dev, *seq, *st),
        }
    }
}

impl Default for PwSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use berry_pwproto::{
        decode_ack, encode_battery, encode_modem, encode_thermal, BatterySnapshot, ModemSnapshot,
        ThermalSensor, ThermalSnapshot, ThermalTrip, ACK_LEN, THERMAL_SENSORS_MAX,
        THERMAL_TRIPS_MAX,
    };

    fn battery_sample(pct: u8) -> BatterySnapshot {
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
            timestamp_ns: 0x1DD0_1234_5678_9ABC,
            measured: 1,
        }
    }

    fn thermal_sample() -> ThermalSnapshot {
        let trip0 = ThermalTrip { level: 2, threshold_mc: 45_000, clr_mc: 43_000 };
        let sen0 = ThermalSensor {
            kind: berry_pwproto::THERMAL_KIND_REAL,
            temp_mc: 31_200,
            trip_count: 1,
            trips: [trip0; THERMAL_TRIPS_MAX],
        };
        ThermalSnapshot { sensor_count: 1, sensors: [sen0; THERMAL_SENSORS_MAX], level_hint: 1 }
    }

    fn modem_sample() -> ModemSnapshot {
        ModemSnapshot { rrc_state: 5, tail_ms: 8_000, signal_tier: 2, in_maint_window: 0 }
    }

    /// 喂整帧字节流，收集事件（杂音吞位静默——只留帧级事件）。
    fn feed(s: &mut PwSession, bytes: &[u8]) -> Vec<SessEvent> {
        let mut evs = Vec::new();
        for &b in bytes {
            let ev = s.push(b);
            if !matches!(ev, SessEvent::NeedMore) {
                evs.push(ev);
            }
        }
        evs
    }

    /// 帧 round-trip 主钉板：编码器产 battery 帧 → 会话应用 → ACK 线帧 →
    /// decode_ack 对拍（ST_OK）；seq 回退重放 → STALE。
    #[test]
    fn battery_roundtrip_ok_then_stale() {
        let mut buf = [0u8; 128];
        let n = encode_battery(&mut buf, 7, &battery_sample(77), 0x5).unwrap();
        let mut s = PwSession::new();
        let evs = feed(&mut s, &buf[..n]);
        assert_eq!(evs.len(), 1);
        match evs[0] {
            SessEvent::Applied { dev, seq, ref snap } => {
                assert_eq!((dev, seq), (DEV_BATTERY, 7));
                assert_eq!(*snap, Snapshot::Battery(battery_sample(77)));
            }
            ref e => panic!("期望应用事件，得 {e:?}"),
        }
        assert_eq!(s.last_seq(DEV_BATTERY), Some(7));
        // ACK 回帧对拍：9B、decode_ack 还原 (dev,seq,ST_OK)。
        let mut ack = [0u8; ACK_LEN];
        let m = PwSession::ack_of(&mut ack, &evs[0]).unwrap();
        assert_eq!(m, ACK_LEN);
        assert_eq!(decode_ack(&ack), Some((DEV_BATTERY, 7, ST_OK)));
        // 同帧重放（seq=7 未推进）→ STALE，不应用。
        let evs = feed(&mut s, &buf[..n]);
        assert_eq!(
            evs,
            vec![SessEvent::Rejected { dev: DEV_BATTERY, seq: 7, st: ST_STALE }]
        );
        let m = PwSession::ack_of(&mut ack, &evs[0]).unwrap();
        assert_eq!(m, ACK_LEN);
        assert_eq!(decode_ack(&ack), Some((DEV_BATTERY, 7, ST_STALE)));
    }

    /// 三设备帧各自 round-trip + per-dev seq 独立（thermal seq 不被 battery
    /// 牵连）。
    #[test]
    fn multi_dev_roundtrip_independent_seq() {
        let mut s = PwSession::new();
        let mut buf = [0u8; 128];

        let n = encode_battery(&mut buf, 100, &battery_sample(50), 0).unwrap();
        feed(&mut s, &buf[..n]);
        // thermal 首帧 seq=1 < battery 的 100——per-dev 独立，照常应用。
        let n = encode_thermal(&mut buf, 1, &thermal_sample(), 1).unwrap();
        let evs = feed(&mut s, &buf[..n]);
        assert!(matches!(evs[0], SessEvent::Applied { dev: DEV_THERMAL, seq: 1, .. }));
        // modem 帧应用。
        let n = encode_modem(&mut buf, 2, &modem_sample(), 0).unwrap();
        let evs = feed(&mut s, &buf[..n]);
        assert!(matches!(evs[0], SessEvent::Applied { dev: DEV_MODEM, seq: 2, .. }));
        assert_eq!(s.last_seq(DEV_BATTERY), Some(100));
        assert_eq!(s.last_seq(DEV_THERMAL), Some(1));
        assert_eq!(s.last_seq(DEV_MODEM), Some(2));
        // battery 回退（100 → 99）→ STALE；thermal 推进（1 → 2）→ OK。
        let n = encode_battery(&mut buf, 99, &battery_sample(50), 0).unwrap();
        let evs = feed(&mut s, &buf[..n]);
        assert_eq!(evs[0], SessEvent::Rejected { dev: DEV_BATTERY, seq: 99, st: ST_STALE });
        let n = encode_thermal(&mut buf, 2, &thermal_sample(), 0).unwrap();
        let evs = feed(&mut s, &buf[..n]);
        assert!(matches!(evs[0], SessEvent::Applied { seq: 2, .. }));
    }

    /// 坏输入拒面（线上侧）：杂音前缀再同步；头合法帧收齐但值域越界
    /// （capacity=101）→ BAD；ver 漂移 → BAD；两者 ACK 均可回。
    #[test]
    fn noise_resync_and_semantic_bad() {
        // 杂音前缀 + 真帧：再同步后照常应用。
        let mut buf = [0u8; 128];
        let n = encode_modem(&mut buf, 3, &modem_sample(), 0).unwrap();
        let mut noisy = [0u8; 160];
        noisy[..3].copy_from_slice(b"\x00\xAB\xFF");
        noisy[3..3 + n].copy_from_slice(&buf[..n]);
        let mut s = PwSession::new();
        let evs = feed(&mut s, &noisy[..3 + n]);
        assert!(matches!(evs[0], SessEvent::Applied { dev: DEV_MODEM, seq: 3, .. }));

        // 手写坏帧（编码器会拒，绕过它——pwproto 同款钉法）：capacity=101。
        let mut bad = [0u8; 64];
        bad[..4].copy_from_slice(&berry_pwproto::MAGIC.to_le_bytes());
        bad[4] = DEV_BATTERY;
        bad[5] = berry_pwproto::VER;
        bad[6..8].copy_from_slice(&8u16.to_le_bytes());
        bad[berry_pwproto::HEADER_LEN] = 1; // present
        bad[berry_pwproto::HEADER_LEN + 4] = 101; // capacity_pct 越界
        let total = berry_pwproto::HEADER_LEN + berry_pwproto::BATTERY_PAYLOAD_LEN + 4;
        let evs = feed(&mut s, &bad[..total]);
        assert_eq!(evs, vec![SessEvent::Rejected { dev: DEV_BATTERY, seq: 8, st: ST_BAD }]);
        let mut ack = [0u8; ACK_LEN];
        PwSession::ack_of(&mut ack, &evs[0]).unwrap();
        assert_eq!(decode_ack(&ack), Some((DEV_BATTERY, 8, ST_BAD)));

        // ver 漂移帧：结构可解长度但版本不符 → BAD。
        let n = encode_modem(&mut buf, 4, &modem_sample(), 0).unwrap();
        buf[5] = 2;
        let evs = feed(&mut s, &buf[..n]);
        assert_eq!(evs, vec![SessEvent::Rejected { dev: DEV_MODEM, seq: 4, st: ST_BAD }]);
        // BAD 不污染 seq 史：同 dev 后续更大 seq 照常应用。
        let n = encode_modem(&mut buf, 5, &modem_sample(), 0).unwrap();
        let evs = feed(&mut s, &buf[..n]);
        assert!(matches!(evs[0], SessEvent::Applied { dev: DEV_MODEM, seq: 5, .. }));
    }

    /// ACK 面坏输入拒面：NeedMore → None；短缓冲 → None。
    #[test]
    fn ack_of_rejects_idle_and_short_buf() {
        assert_eq!(PwSession::ack_of(&mut [0u8; ACK_LEN], &SessEvent::NeedMore), None);
        let ev = SessEvent::Rejected { dev: DEV_BATTERY, seq: 1, st: ST_BAD };
        assert_eq!(PwSession::ack_of(&mut [0u8; ACK_LEN - 1], &ev), None);
        let mut ok = [0u8; ACK_LEN];
        assert_eq!(PwSession::ack_of(&mut ok, &ev), Some(ACK_LEN));
    }

    /// 未知 dev 防御面：直达 push 产物不 panic（构造：Frame 事件不经通道
    /// 不可达——以 dev_slot 直接钉板其全拒）。
    #[test]
    fn dev_slot_total() {
        assert_eq!(dev_slot(DEV_BATTERY), Some(0));
        assert_eq!(dev_slot(DEV_THERMAL), Some(1));
        assert_eq!(dev_slot(DEV_MODEM), Some(2));
        for d in [0u8, 4, 9, 255] {
            assert_eq!(dev_slot(d), None);
            assert_eq!(PwSession::new().last_seq(d), None);
        }
    }
}
