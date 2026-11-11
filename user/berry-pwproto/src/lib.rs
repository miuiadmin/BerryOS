//! berry-pwproto —— sim 设备快照协议纯函数库（sim-S1，`berry-netdproto`
//! 先例：「解析器即产品」，零分配、零 unsafe、零依赖，输入只借用）。
//!
//! ## Overview (for crates.io users)
//!
//! Wire protocol for the [BerryOS](https://github.com/miuiadmin/BerryOS)
//! sim-device channel (battery/thermal/modem snapshots over a UART byte
//! stream, plus a guest→host ACK). Atomic-snapshot framing: every frame
//! carries the full device state and a monotonically increasing `seq`,
//! so a torn or stale frame can never be half-applied (the goldfish
//! per-field register-write ordering pitfall, avoided at the message
//! layer). Pure no_std encoder/parser with a byte-at-a-time stream
//! decoder; channel/wait semantics live at each end of the wire.
//!
//! Status: experimental (0.1.0) — the byte layouts move with the BerryOS
//! v1 services. Repo docs are mostly zh-CN.
//!
//! ## 帧布局（v1，`docs/sim设备通道与契约设计-v1.1.md` §5 冻结）
//!
//! 共同头 8B + 定长载荷（per 设备全量快照）+ 事件位图 u32 = 每帧全量
//! 替换、宿主禁止多帧拼装（§4.1「原子快照」）。多字节数值全 LE：
//!
//! - 头 `[magic u32 LE = "PWSM"][dev u8][ver u8][seq u16 LE]`；dev ∈
//!   {1 电池 / 2 热 / 3 modem}（[`DEV_BATTERY`] 族常量），ver = [`VER`]
//!   （学 OH HDI IDL 版本目录）；seq = 递增序号（原子快照的防重放/防
//!   陈旧锚——消费侧自持 last_seq，回退 → [`ST_STALE`]）。
//! - 电池载荷 38B（goldfish 寄存器集 ∪ Fuchsia BatteryInfo 15 字段 ∪
//!   P8 measured/modeled 位，PM-16 §4.1 表）：present/ac_online/status
//!   (4 态)/health (8 态)/capacity_pct (≤100)/voltage_mV u16/temp_mC
//!   i32/current_now_uA i32/current_avg_uA i32/charge_full_uAh u32/
//!   charge_counter_uAh u32/cycle_count u16/timestamp_ns u64/measured。
//! - 热载荷 86B：sensor_count ≤ [`THERMAL_SENSORS_MAX`] = 2、per 传感器
//!   {kind, temp_mc i32, trip_count ≤ [`THERMAL_TRIPS_MAX`] = 4, trips×
//!   {level u8, threshold_mc i32, clr_mc i32} = 9B}、level_hint u8 四档。
//! - modem 载荷 7B：rrc_state u8/tail_ms u32/signal_tier u8/
//!   in_maint_window u8。
//! - ACK（guest→host 同通道回写，9B）：头 5 字段 + `st u8`——[`ST_OK`]
//!   已应用 / [`ST_BAD`] 解析或值域拒 / [`ST_STALE`] seq 回退。
//!
//! ## 解析纪律（与 berry-netdproto 同款）
//!
//! 严格恰长解析：截断/尾随/未知 dev/ver 不符/值域越界（capacity>100、
//! 布尔位非 0/1、sensor_count 越帽…）全拒——「AI 测试友好化」：协议正确
//! 性不依赖 QEMU 就能 host 验证。编码器 None 拒绝契约：编不出就拒绝、
//! 不截断。**流式面特殊语义**：字节流无帧边界（UART1 逐 chunk 转发），
//! [`StreamDecoder`] 逐字节喂入——魔数失配/未知 dev 的头按杂音**再同步
//! 扫描**（静默丢弃至下一个魔数候选）；头合法且帧长收齐但**语义拒**
//! （值域/ver）则显式 yield [`Step::Bad`]——服务侧回 ACK [`ST_BAD`]，
//! 宿主可区分「没收到」（无 ACK）与「收到但拒」（BAD ACK），对抗注入
//! 回显歧义（PM-16 §4.1 回声行）。

#![cfg_attr(not(test), no_std)]

/// 帧魔数：线序字节 `"PWSM"`（Power-Well SiM bus）。
pub const MAGIC: u32 = u32::from_le_bytes(*b"PWSM");

/// 协议版本（ver 字段当前值）。
pub const VER: u8 = 1;

/// 设备号：电池（goldfish ∪ BatteryInfo ∪ P8 measured 位）。
pub const DEV_BATTERY: u8 = 1;
/// 设备号：热（PM-07 §4-2 自报接口形状）。
pub const DEV_THERMAL: u8 = 2;
/// 设备号：modem（PM-05 §4.5 尾能/弱信号档）。
pub const DEV_MODEM: u8 = 3;

/// ACK 状态：已应用（快照生效）。
pub const ST_OK: u8 = 0;
/// ACK 状态：帧收齐但解析/值域拒（编码器与解析器对称执法面）。
pub const ST_BAD: u8 = 1;
/// ACK 状态：seq 回退（消费侧自持 last_seq 判定，服务逻辑不在本库）。
pub const ST_STALE: u8 = 2;

/// 共同头长（magic 4 + dev 1 + ver 1 + seq 2）。
pub const HEADER_LEN: usize = 8;

/// 电池快照载荷长（定长）。
pub const BATTERY_PAYLOAD_LEN: usize = 38;
/// 热快照载荷长（定长，sensor_count + 2×42 + level_hint）。
pub const THERMAL_PAYLOAD_LEN: usize = 86;
/// modem 快照载荷长（定长）。
pub const MODEM_PAYLOAD_LEN: usize = 7;

/// 热传感器数帽（定长契约：数组按帽铺满，count 前缀指示有效段）。
pub const THERMAL_SENSORS_MAX: usize = 2;
/// 热单传感器 trip 数帽。
pub const THERMAL_TRIPS_MAX: usize = 4;

/// ACK 帧总长。
pub const ACK_LEN: usize = HEADER_LEN + 1;

/// 全帧长上界（thermal 98B 最大）。
pub const FRAME_MAX: usize = HEADER_LEN + THERMAL_PAYLOAD_LEN + 4;

/// 电池状态 4 态上界（0..=3：unknown/charging/discharging/full）。
pub const BATTERY_STATUS_MAX: u8 = 3;
/// 电池健康 8 态上界（0..=7：goldfish BATTERY_HEALTH_* 族）。
pub const BATTERY_HEALTH_MAX: u8 = 7;
/// 热档提示 4 档上界（PM-07 §4-5 thermal_level）。
pub const THERMAL_LEVEL_MAX: u8 = 3;
/// 热传感器 kind：real（物理传感器）。
pub const THERMAL_KIND_REAL: u8 = 0;
/// 热传感器 kind：virtual（加权/派生虚拟传感器）。
pub const THERMAL_KIND_VIRTUAL: u8 = 1;

/// 电池全量快照（14 字段；字段集冻结见 crate 头注）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatterySnapshot {
    pub present: u8,
    pub ac_online: u8,
    pub status: u8,
    pub health: u8,
    pub capacity_pct: u8,
    pub voltage_mv: u16,
    pub temp_mc: i32,
    pub current_now_ua: i32,
    pub current_avg_ua: i32,
    pub charge_full_uah: u32,
    pub charge_counter_uah: u32,
    pub cycle_count: u16,
    pub timestamp_ns: u64,
    /// P8「归因诚实」位：1 = 实测、0 = 模型推算（PM-12 回填 #12）。
    pub measured: u8,
}

/// 热单个 trip 点：{level, threshold_mc, clr_mc}（迟滞窗 = threshold→clr
/// 回落解除，PM-16 §4.1 thermal 行「迟滞窗不抖动」断言的契约锚）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalTrip {
    pub level: u8,
    pub threshold_mc: i32,
    pub clr_mc: i32,
}

/// 热单个传感器自报面（PM-07 §4-2：sensor_id 由通道端点位隐含，结构
/// 只承内容——kind/temp/trips 表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalSensor {
    pub kind: u8,
    pub temp_mc: i32,
    pub trip_count: u8,
    pub trips: [ThermalTrip; THERMAL_TRIPS_MAX],
}

/// 热全量快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalSnapshot {
    pub sensor_count: u8,
    pub sensors: [ThermalSensor; THERMAL_SENSORS_MAX],
    pub level_hint: u8,
}

/// modem 全量快照（尾能档/弱信号档可配置，PM-05 §4.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModemSnapshot {
    pub rrc_state: u8,
    pub tail_ms: u32,
    pub signal_tier: u8,
    pub in_maint_window: u8,
}

/// 解码产物：全量快照 + 归属（dev/seq/changed）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedFrame {
    pub dev: u8,
    pub seq: u16,
    pub changed: u32,
    pub snap: Snapshot,
}

/// 三设备快照枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Snapshot {
    Battery(BatterySnapshot),
    Thermal(ThermalSnapshot),
    Modem(ModemSnapshot),
}

// ---- 原始字节辅助（LE 读写；纯函数、无 unsafe）----

fn rd_u16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn rd_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn rd_i32(b: &[u8]) -> i32 {
    i32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn rd_u64(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    u64::from_le_bytes(a)
}

fn wr_u16(out: &mut [u8], v: u16) {
    out[..2].copy_from_slice(&v.to_le_bytes());
}

fn wr_u32(out: &mut [u8], v: u32) {
    out[..4].copy_from_slice(&v.to_le_bytes());
}

fn wr_i32(out: &mut [u8], v: i32) {
    out[..4].copy_from_slice(&v.to_le_bytes());
}

fn wr_u64(out: &mut [u8], v: u64) {
    out[..8].copy_from_slice(&v.to_le_bytes());
}

/// 共同头写（8B）。dev 未知时编码侧先拒（调方用本库常量）。
fn wr_header(out: &mut [u8], dev: u8, seq: u16) {
    wr_u32(out, MAGIC);
    out[4] = dev;
    out[5] = VER;
    wr_u16(&mut out[6..], seq);
}

// ---- 编码器（None 拒绝契约：编不出就拒绝、不截断）----

/// 电池帧编码：`8 + 38 + 4 = 50B`。值域非法（布尔位非 0/1、capacity>100
/// …）编码侧同拒——线上不存在「编码器收、解析器拒」的不对称形态。
pub fn encode_battery(out: &mut [u8], seq: u16, s: &BatterySnapshot, changed: u32) -> Option<usize> {
    if out.len() < HEADER_LEN + BATTERY_PAYLOAD_LEN + 4 {
        return None;
    }
    if !battery_ok(s) {
        return None;
    }
    wr_header(out, DEV_BATTERY, seq);
    let p = &mut out[HEADER_LEN..];
    p[0] = s.present;
    p[1] = s.ac_online;
    p[2] = s.status;
    p[3] = s.health;
    p[4] = s.capacity_pct;
    wr_u16(&mut p[5..], s.voltage_mv);
    wr_i32(&mut p[7..], s.temp_mc);
    wr_i32(&mut p[11..], s.current_now_ua);
    wr_i32(&mut p[15..], s.current_avg_ua);
    wr_u32(&mut p[19..], s.charge_full_uah);
    wr_u32(&mut p[23..], s.charge_counter_uah);
    wr_u16(&mut p[27..], s.cycle_count);
    wr_u64(&mut p[29..], s.timestamp_ns);
    p[37] = s.measured;
    wr_u32(&mut out[HEADER_LEN + BATTERY_PAYLOAD_LEN..], changed);
    Some(HEADER_LEN + BATTERY_PAYLOAD_LEN + 4)
}

/// 热帧编码：`8 + 86 + 4 = 98B`（全帧长上界）。
pub fn encode_thermal(out: &mut [u8], seq: u16, s: &ThermalSnapshot, changed: u32) -> Option<usize> {
    if out.len() < HEADER_LEN + THERMAL_PAYLOAD_LEN + 4 {
        return None;
    }
    if !thermal_ok(s) {
        return None;
    }
    wr_header(out, DEV_THERMAL, seq);
    let p = &mut out[HEADER_LEN..];
    p[0] = s.sensor_count;
    for (i, sen) in s.sensors.iter().enumerate().take(THERMAL_SENSORS_MAX) {
        let q = &mut p[1 + i * 42..];
        q[0] = sen.kind;
        wr_i32(&mut q[1..], sen.temp_mc);
        q[5] = sen.trip_count;
        for (j, t) in sen.trips.iter().enumerate().take(THERMAL_TRIPS_MAX) {
            let r = &mut q[6 + j * 9..];
            r[0] = t.level;
            wr_i32(&mut r[1..], t.threshold_mc);
            wr_i32(&mut r[5..], t.clr_mc);
        }
    }
    p[THERMAL_PAYLOAD_LEN - 1] = s.level_hint;
    wr_u32(&mut out[HEADER_LEN + THERMAL_PAYLOAD_LEN..], changed);
    Some(HEADER_LEN + THERMAL_PAYLOAD_LEN + 4)
}

/// modem 帧编码：`8 + 7 + 4 = 19B`。
pub fn encode_modem(out: &mut [u8], seq: u16, s: &ModemSnapshot, changed: u32) -> Option<usize> {
    if out.len() < HEADER_LEN + MODEM_PAYLOAD_LEN + 4 {
        return None;
    }
    if !modem_ok(s) {
        return None;
    }
    wr_header(out, DEV_MODEM, seq);
    let p = &mut out[HEADER_LEN..];
    p[0] = s.rrc_state;
    wr_u32(&mut p[1..], s.tail_ms);
    p[5] = s.signal_tier;
    p[6] = s.in_maint_window;
    wr_u32(&mut out[HEADER_LEN + MODEM_PAYLOAD_LEN..], changed);
    Some(HEADER_LEN + MODEM_PAYLOAD_LEN + 4)
}

/// ACK 帧编码（guest→host）：头 + st = 9B。st 须为本库三常量之一。
pub fn encode_ack(out: &mut [u8], dev: u8, seq: u16, st: u8) -> Option<usize> {
    if out.len() < ACK_LEN {
        return None;
    }
    if !matches!(dev, DEV_BATTERY | DEV_THERMAL | DEV_MODEM) || st > ST_STALE {
        return None;
    }
    wr_header(out, dev, seq);
    out[HEADER_LEN] = st;
    Some(ACK_LEN)
}

/// ACK 帧解析（宿主注入 client 用）：恰长，尾随/魔数/dev/ver/st 越界全拒。
pub fn decode_ack(buf: &[u8]) -> Option<(u8, u16, u8)> {
    if buf.len() != ACK_LEN
        || rd_u32(buf) != MAGIC
        || buf[5] != VER
        || !matches!(buf[4], DEV_BATTERY | DEV_THERMAL | DEV_MODEM)
    {
        return None;
    }
    let st = buf[HEADER_LEN];
    if st > ST_STALE {
        return None;
    }
    Some((buf[4], rd_u16(&buf[6..]), st))
}

// ---- 值域执法（编码/解析两侧对称）----

fn bool_ok(v: u8) -> bool {
    v <= 1
}

fn battery_ok(s: &BatterySnapshot) -> bool {
    bool_ok(s.present)
        && bool_ok(s.ac_online)
        && s.status <= BATTERY_STATUS_MAX
        && s.health <= BATTERY_HEALTH_MAX
        && s.capacity_pct <= 100
        && bool_ok(s.measured)
}

fn thermal_ok(s: &ThermalSnapshot) -> bool {
    if s.sensor_count as usize > THERMAL_SENSORS_MAX || s.level_hint > THERMAL_LEVEL_MAX {
        return false;
    }
    // 仅前 sensor_count 个传感器须有效（尾随槽全零非契约——count 外内容
    // 不参与对拍，编码原样搬运、解析原样读出，值域只查有效段）。
    s.sensors.iter().take(s.sensor_count as usize).all(|sen| {
        (sen.kind == THERMAL_KIND_REAL || sen.kind == THERMAL_KIND_VIRTUAL)
            && sen.trip_count as usize <= THERMAL_TRIPS_MAX
            && sen.trips.iter().take(sen.trip_count as usize).all(|t| {
                t.level <= THERMAL_LEVEL_MAX && t.clr_mc <= t.threshold_mc
            })
    })
}

fn modem_ok(s: &ModemSnapshot) -> bool {
    bool_ok(s.in_maint_window)
}

// ---- 流式解码（字节流无帧边界；杂音再同步 + 语义拒显式 yield）----

/// 单字节喂入的产出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// 需要更多字节（含杂音再同步吞掉的位移——静默）。
    NeedMore,
    /// 一帧收齐且全检通过。
    Frame(DecodedFrame),
    /// 头合法、帧长收齐、但语义拒（ver 不符/值域越界）——携带头信息供
    /// ACK BAD 回执。杂音（魔数失配/未知 dev）不 yield Bad：静默再同步。
    Bad { dev: u8, seq: u16 },
}

/// 流式解码器：逐字节 [`push`](StreamDecoder::push)，内部累积到
/// [`FRAME_MAX`]。再同步策略 = 丢 1 字节重扫（魔数 4B 对齐靠扫描不靠
/// 链路），完整帧消费后缓冲清零重来。
#[derive(Debug, Clone)]
pub struct StreamDecoder {
    buf: [u8; FRAME_MAX],
    len: usize,
}

impl StreamDecoder {
    /// 新建（空累积）。
    pub fn new() -> Self {
        StreamDecoder { buf: [0; FRAME_MAX], len: 0 }
    }

    /// 喂一个字节。头未收齐前只累积；头合法后按 dev 定帧长收齐再整帧
    /// 解码。杂音/未知 dev → 丢首字节再同步（本字节可能已入 buf——
    /// 统一经 `resync` 路径处理）。
    pub fn push(&mut self, b: u8) -> Step {
        if self.len < self.buf.len() {
            self.buf[self.len] = b;
            self.len += 1;
        } else {
            // 理论不可达（头 8B 内必判魔数），防御面：整.buf 丢弃重来。
            self.len = 0;
            return Step::NeedMore;
        }
        if self.len < 4 {
            return Step::NeedMore;
        }
        if rd_u32(&self.buf) != MAGIC {
            self.resync();
            return Step::NeedMore;
        }
        if self.len < HEADER_LEN {
            return Step::NeedMore;
        }
        let dev = self.buf[4];
        let seq = rd_u16(&self.buf[6..]);
        let payload = match dev {
            DEV_BATTERY => BATTERY_PAYLOAD_LEN,
            DEV_THERMAL => THERMAL_PAYLOAD_LEN,
            DEV_MODEM => MODEM_PAYLOAD_LEN,
            _ => {
                // 魔数命中但 dev 未知：帧长不可知，按杂音再同步。
                self.resync();
                return Step::NeedMore;
            }
        };
        let total = HEADER_LEN + payload + 4;
        if self.len < total {
            return Step::NeedMore;
        }
        // 整帧在 buf——解码消费（成功/语义拒都清缓冲；编码器对称保证
        // ver/值域双向同拒，这里 Bad 分支实际吃 ver 漂移面）。
        let frame = decode_frame(&self.buf[..total]);
        self.len = 0;
        match frame {
            Some(f) => Step::Frame(f),
            None => Step::Bad { dev, seq },
        }
    }

    /// 丢首字节、余段前移（杂音再同步——下一候选从头 1 字节后起扫）。
    fn resync(&mut self) {
        self.buf.copy_within(1.., 0);
        self.len -= 1;
    }
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// 整帧解码（恰长执法 + 值域执法；头魔数/ver 由调用面或此处双检）。
fn decode_frame(buf: &[u8]) -> Option<DecodedFrame> {
    if buf.len() < HEADER_LEN || rd_u32(buf) != MAGIC || buf[5] != VER {
        return None;
    }
    let dev = buf[4];
    let seq = rd_u16(&buf[6..]);
    let p = &buf[HEADER_LEN..];
    let (snap, changed_off) = match dev {
        DEV_BATTERY => {
            if p.len() < BATTERY_PAYLOAD_LEN + 4 {
                return None;
            }
            let s = BatterySnapshot {
                present: p[0],
                ac_online: p[1],
                status: p[2],
                health: p[3],
                capacity_pct: p[4],
                voltage_mv: rd_u16(&p[5..]),
                temp_mc: rd_i32(&p[7..]),
                current_now_ua: rd_i32(&p[11..]),
                current_avg_ua: rd_i32(&p[15..]),
                charge_full_uah: rd_u32(&p[19..]),
                charge_counter_uah: rd_u32(&p[23..]),
                cycle_count: rd_u16(&p[27..]),
                timestamp_ns: rd_u64(&p[29..]),
                measured: p[37],
            };
            if !battery_ok(&s) {
                return None;
            }
            (Snapshot::Battery(s), BATTERY_PAYLOAD_LEN)
        }
        DEV_THERMAL => {
            if p.len() < THERMAL_PAYLOAD_LEN + 4 {
                return None;
            }
            let mut sensors =
                [ThermalSensor { kind: 0, temp_mc: 0, trip_count: 0, trips: [ThermalTrip { level: 0, threshold_mc: 0, clr_mc: 0 }; THERMAL_TRIPS_MAX] };
                    THERMAL_SENSORS_MAX];
            for (i, sen) in sensors.iter_mut().enumerate().take(THERMAL_SENSORS_MAX) {
                let q = &p[1 + i * 42..];
                sen.kind = q[0];
                sen.temp_mc = rd_i32(&q[1..]);
                sen.trip_count = q[5];
                for (j, t) in sen.trips.iter_mut().enumerate().take(THERMAL_TRIPS_MAX) {
                    let r = &q[6 + j * 9..];
                    t.level = r[0];
                    t.threshold_mc = rd_i32(&r[1..]);
                    t.clr_mc = rd_i32(&r[5..]);
                }
            }
            let s = ThermalSnapshot {
                sensor_count: p[0],
                sensors,
                level_hint: p[THERMAL_PAYLOAD_LEN - 1],
            };
            if !thermal_ok(&s) {
                return None;
            }
            (Snapshot::Thermal(s), THERMAL_PAYLOAD_LEN)
        }
        DEV_MODEM => {
            if p.len() < MODEM_PAYLOAD_LEN + 4 {
                return None;
            }
            let s = ModemSnapshot {
                rrc_state: p[0],
                tail_ms: rd_u32(&p[1..]),
                signal_tier: p[5],
                in_maint_window: p[6],
            };
            if !modem_ok(&s) {
                return None;
            }
            (Snapshot::Modem(s), MODEM_PAYLOAD_LEN)
        }
        _ => return None,
    };
    Some(DecodedFrame { dev, seq, changed: rd_u32(&p[changed_off..]), snap })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery_sample() -> BatterySnapshot {
        BatterySnapshot {
            present: 1,
            ac_online: 0,
            status: 2,
            health: 3,
            capacity_pct: 77,
            voltage_mv: 3990,
            temp_mc: 28_500,
            current_now_ua: -412_000,
            current_avg_ua: -395_000,
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
            kind: THERMAL_KIND_REAL,
            temp_mc: 31_200,
            trip_count: 1,
            trips: [trip0; THERMAL_TRIPS_MAX],
        };
        ThermalSnapshot {
            sensor_count: 1,
            sensors: [sen0; THERMAL_SENSORS_MAX],
            level_hint: 1,
        }
    }

    fn modem_sample() -> ModemSnapshot {
        ModemSnapshot { rrc_state: 5, tail_ms: 8_000, signal_tier: 2, in_maint_window: 0 }
    }

    #[test]
    fn battery_roundtrip() {
        let mut buf = [0u8; 64];
        let s = battery_sample();
        let n = encode_battery(&mut buf, 7, &s, 0x5).unwrap();
        assert_eq!(n, HEADER_LEN + BATTERY_PAYLOAD_LEN + 4);
        let mut dec = StreamDecoder::new();
        let mut got = None;
        for &b in &buf[..n] {
            match dec.push(b) {
                Step::Frame(f) => got = Some(f),
                Step::Bad { .. } => panic!("battery 帧语义拒"),
                Step::NeedMore => {}
            }
        }
        let f = got.expect("50B 全喂必出一帧");
        assert_eq!(f.dev, DEV_BATTERY);
        assert_eq!(f.seq, 7);
        assert_eq!(f.changed, 0x5);
        assert_eq!(f.snap, Snapshot::Battery(s));
    }

    #[test]
    fn thermal_modem_roundtrip() {
        let mut buf = [0u8; 128];
        let n = encode_thermal(&mut buf, 9, &thermal_sample(), 1).unwrap();
        assert_eq!(n, HEADER_LEN + THERMAL_PAYLOAD_LEN + 4);
        let mut dec = StreamDecoder::new();
        let mut got = None;
        for &b in &buf[..n] {
            if let Step::Frame(f) = dec.push(b) {
                got = Some(f);
            }
        }
        assert_eq!(got.unwrap().snap, Snapshot::Thermal(thermal_sample()));

        let n = encode_modem(&mut buf, 300, &modem_sample(), 0).unwrap();
        assert_eq!(n, HEADER_LEN + MODEM_PAYLOAD_LEN + 4);
        let mut dec = StreamDecoder::new();
        let mut got = None;
        for &b in &buf[..n] {
            if let Step::Frame(f) = dec.push(b) {
                got = Some(f);
            }
        }
        let f = got.unwrap();
        assert_eq!((f.dev, f.seq), (DEV_MODEM, 300));
        assert_eq!(f.snap, Snapshot::Modem(modem_sample()));
    }

    #[test]
    fn ack_roundtrip_and_rejects() {
        let mut buf = [0u8; ACK_LEN];
        assert_eq!(encode_ack(&mut buf, DEV_BATTERY, 42, ST_OK), Some(ACK_LEN));
        assert_eq!(decode_ack(&buf), Some((DEV_BATTERY, 42, ST_OK)));
        // st 越界 / 未知 dev / 尾随字节 / 魔数破坏 全拒（编码与解码对称）。
        assert_eq!(encode_ack(&mut buf, DEV_BATTERY, 1, 3), None);
        assert_eq!(encode_ack(&mut buf, 9, 1, ST_OK), None);
        encode_ack(&mut buf, DEV_BATTERY, 1, ST_OK);
        let mut bad_dev = buf;
        bad_dev[4] = 9;
        assert_eq!(decode_ack(&bad_dev), None);
        assert_eq!(decode_ack(&buf[..ACK_LEN - 1]), None);
        let mut bad = buf;
        bad[0] ^= 0xFF;
        assert_eq!(decode_ack(&bad), None);
    }

    #[test]
    fn encoder_none_on_short_out_and_bad_values() {
        let mut short = [0u8; 16];
        assert_eq!(encode_battery(&mut short, 0, &battery_sample(), 0), None);
        let mut s = battery_sample();
        s.capacity_pct = 101;
        let mut buf = [0u8; 64];
        assert_eq!(encode_battery(&mut buf, 0, &s, 0), None);
        s.capacity_pct = 100;
        s.present = 2;
        assert_eq!(encode_battery(&mut buf, 0, &s, 0), None);
        s.present = 1;
        s.status = 4;
        assert_eq!(encode_battery(&mut buf, 0, &s, 0), None);
        // 热值域：count 越帽 / kind 野值 / trip_count 越帽 / clr > threshold。
        let mut t = thermal_sample();
        t.sensor_count = 3;
        assert_eq!(encode_thermal(&mut buf, 0, &t, 0), None);
        t.sensor_count = 2;
        t.sensors[0].kind = 7;
        assert_eq!(encode_thermal(&mut buf, 0, &t, 0), None);
        t.sensors[0].kind = THERMAL_KIND_VIRTUAL;
        t.sensors[0].trip_count = 5;
        assert_eq!(encode_thermal(&mut buf, 0, &t, 0), None);
        t.sensors[0].trip_count = 1;
        t.sensors[0].trips[0] = ThermalTrip { level: 1, threshold_mc: 40_000, clr_mc: 41_000 };
        assert_eq!(encode_thermal(&mut buf, 0, &t, 0), None);
        // modem：in_maint_window 布尔位。
        let mut m = modem_sample();
        m.in_maint_window = 5;
        assert_eq!(encode_modem(&mut buf, 0, &m, 0), None);
    }

    #[test]
    fn stream_splits_at_every_offset() {
        let mut buf = [0u8; 64];
        let n = encode_battery(&mut buf, 33, &battery_sample(), 0).unwrap();
        // 每个切点拆两段喂——流式无边界契约的主钉板。
        for cut in 0..=n {
            let mut dec = StreamDecoder::new();
            let mut got = None;
            for chunk in [&buf[..cut], &buf[cut..n]] {
                for &b in chunk {
                    if let Step::Frame(f) = dec.push(b) {
                        got = Some(f);
                    }
                }
            }
            assert_eq!(got.expect("任意切点必出一帧").seq, 33, "cut={cut}");
        }
    }

    #[test]
    fn stream_resync_on_noise_and_unknown_dev() {
        let mut buf = [0u8; 64];
        let n = encode_battery(&mut buf, 5, &battery_sample(), 0).unwrap();
        let mut noisy = [0u8; 128];
        // 前缀杂音 + 帧中插一个「魔数命中但 dev 野」的伪头。
        noisy[..3].copy_from_slice(b"\x00\xAB\xFF");
        let mut len = 3;
        noisy[len..len + 4].copy_from_slice(&MAGIC.to_le_bytes());
        noisy[len + 4] = 9; // 未知 dev → 再同步起点
        noisy[len + 5] = VER;
        noisy[len + 6] = 0;
        noisy[len + 7] = 0;
        len += 8;
        noisy[len..len + n].copy_from_slice(&buf[..n]);
        len += n;
        let mut dec = StreamDecoder::new();
        let mut got = None;
        for &b in &noisy[..len] {
            if let Step::Frame(f) = dec.push(b) {
                got = Some(f);
            }
        }
        assert_eq!(got.expect("杂音+伪头后再同步必出真帧").seq, 5);
    }

    #[test]
    fn stream_bad_on_complete_but_invalid() {
        // 头合法帧收齐但 capacity=101 → 显式 Bad（服务回 ACK ST_BAD）。
        let mut buf = [0u8; 64];
        let mut s = battery_sample();
        s.capacity_pct = 101;
        // 直接手写线（编码器会拒，绕过它构造收齐的坏帧）。
        buf[..4].copy_from_slice(&MAGIC.to_le_bytes());
        buf[4] = DEV_BATTERY;
        buf[5] = VER;
        wr_u16(&mut buf[6..], 8);
        buf[HEADER_LEN] = 1; // present
        buf[HEADER_LEN + 4] = 101; // capacity_pct 越界
        let total = HEADER_LEN + BATTERY_PAYLOAD_LEN + 4;
        let mut dec = StreamDecoder::new();
        let mut bad = None;
        for i in 0..total {
            match dec.push(buf[i]) {
                Step::Bad { dev, seq } => bad = Some((dev, seq)),
                Step::Frame(_) => panic!("坏帧不该过"),
                Step::NeedMore => {}
            }
        }
        assert_eq!(bad, Some((DEV_BATTERY, 8)));
    }

    #[test]
    fn stream_bad_on_version_mismatch() {
        // ver=2 的完整帧：结构可解长度但版本不符 → Bad（宿主得知 ver 错）。
        let mut buf = [0u8; 64];
        let n = encode_modem(&mut buf, 2, &modem_sample(), 0).unwrap();
        buf[5] = 2;
        let mut dec = StreamDecoder::new();
        let mut bad = None;
        for &b in &buf[..n] {
            if let Step::Bad { dev, seq } = dec.push(b) {
                bad = Some((dev, seq));
            }
        }
        assert_eq!(bad, Some((DEV_MODEM, 2)));
    }

    #[test]
    fn golden_bytes_battery() {
        // 黄金字节钉板（C-3 psproto 同款）：全零快照 + seq=1 + changed=0 的
        // 首帧线值逐字节钉死——host 注入 client 与 guest 解码器共享此基线。
        let s = BatterySnapshot {
            present: 0,
            ac_online: 0,
            status: 0,
            health: 0,
            capacity_pct: 0,
            voltage_mv: 0,
            temp_mc: 0,
            current_now_ua: 0,
            current_avg_ua: 0,
            charge_full_uah: 0,
            charge_counter_uah: 0,
            cycle_count: 0,
            timestamp_ns: 0,
            measured: 0,
        };
        let mut buf = [0u8; 64];
        let n = encode_battery(&mut buf, 1, &s, 0).unwrap();
        assert_eq!(n, 50);
        assert_eq!(&buf[..8], &[0x50, 0x57, 0x53, 0x4D, 0x01, 0x01, 0x01, 0x00]);
        assert!(buf[8..n].iter().all(|&b| b == 0));
    }
}
