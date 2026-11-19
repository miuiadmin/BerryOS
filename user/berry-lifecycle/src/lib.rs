//! # berry-lifecycle —— 生命周期三口纯函数底座（app-2）
//!
//! ## Overview (for crates.io users)
//!
//! Pure-function state-container and wire codec for the
//! [BerryOS](https://github.com/miuiadmin/BerryOS) app-lifecycle
//! surface (spec §4.1-5: register a pause callback / save state /
//! restore state — `docs/app-2实施设计草案-v1.0.md` is the
//! authority). This crate owns:
//!
//! - the **32-byte state-blob container header v1** (magic `BTMB`,
//!   ver/hlen, counter, payload_len, crc32 — §3.1 layout) plus the
//!   read-side **hlen dispatch** (v0 12B legacy read, v1 32B, four
//!   corruption shapes → BadFmt, §3.2 compatibility matrix);
//! - a bitwise **CRC32** (poly `0xEDB88320`) mirroring
//!   `tools/berry/src/png.rs` `Crc32` — the two-implementation issue
//!   is pinned by host test vectors (`"123456789" → 0xCBF43926` and
//!   a golden-blob byte snapshot);
//! - the **8-byte frame codec** (frame ⇄ (tag, counter): tag u8 +
//!   3 zero pad bytes + counter u32 LE) shared with the svcmgr
//!   tomb face byte-for-byte;
//! - the **library-held counter state machine** (Restored(N) → N+1 /
//!   beat → +1 / save → header takes the current tick).
//!
//! No syscall face here; the glue (handle acquisition, channel/VMO
//! IO, act orchestration) lives in berry-libc `lifecycle.rs`.
//! no_std core; builds and unit-tests on any host.
//!
//! Status: experimental (0.1.0) — the API moves with the BerryOS v1
//! userland ABI. Repo docs are mostly zh-CN.
//!
//! ## 卡位与形态（app-2 §2.2 分层立法）
//!
//! 三口 API 面 = berry-libc `lifecycle.rs`（`lifecycle_register`/
//! `state_save`/`state_restore` + 机械件 `lifecycle_poll`/
//! `lifecycle_beat`）；本 crate = 其纯函数底座（berry-tls /
//! berry-signal「纯面外迁」先例——独立零依赖 crate，host 测试零
//! libc 符号面）。berry-libc 结构性不可 `cargo test`（lib.rs
//! 「实况勘定」注），故头/帧/counter 全部断言落在本 crate。
//!
//! ### 头域完整性分工（§3.1 立法）
//!
//! CRC 覆盖域 = **payload 全幅不含头**：counter（8..12）单错不落
//! BadFmt（静默 Restored），由 svcmgr `wait_frame` 对账兜底（rctr 臂
//! ——协议域完整性归 svcmgr 对账、数据域归本 crate CRC）；len 错由
//! 「crc 按错 len 计算必不符」+ len 超界臂间接兜底；hlen/ver 由 hlen
//! 分派序直接落 BadFmt。host 测以「金件 blob 翻转 counter 单字节 →
//! 仍 Restored」钉死该契约，防后人好心扩域破 v0 前缀兼容。
//!
//! ### 同刀约束（§3.2 推论立法）
//!
//! `BLOB_VER`/`BLOB_HLEN` 单源常量（写读共源即「v1 头写读两侧同二
//! 进制同刀」的结构性保证）；前缀 12B 布局同位（magic/counter 段
//! 跨版本不动，ver@4/hlen@5 按版本变值）由 offset 常量断言钉板
//! （§8.3 R1）。

#![cfg_attr(not(test), no_std)]
#![allow(clippy::needless_range_loop)]

// —— state VMO / 容器帽（§7 帽表单源；svcmgr STATE_VMO_SZ = 64KiB 对端镜像）——

/// state VMO 定尺寸（64KiB——blob 帽 = 结构性执法，越写物理不可）。
pub const STATE_VMO_SZ: usize = 64 * 1024;

/// v0 头长（app-1 自验件试验格式：12B 定长）。
pub const HEAD_V0: usize = 12;
/// v1 头长（§3.1 立法：hlen 扩展追加至 32B）。
pub const HEAD_V1: usize = 32;
/// v0 blob 格式版本（协议侧，非 app 版本）。
pub const VER_V0: u8 = 0;
/// v1 blob 格式版本。
pub const VER_V1: u8 = 1;
/// payload 域帽（64KiB - 32B；写侧 TooLarge 前置拒/读侧满帽拒的对偶基线）。
pub const PAYLOAD_CAP: usize = STATE_VMO_SZ - HEAD_V1;

// —— 头布局 offset 常量（§3.1；前缀 12B 跨版本布局同位——R1 钉板对象）——

/// magic `BTMB` u32 LE（0..4）——v0 同位（svcmgr 记表 gate 首读面零改）。
pub const MAGIC_OFF: usize = 0;
/// ver u8（4）——跨版本变值（v0=0 / v1=1），不属逐字节等值域。
pub const VER_OFF: usize = 4;
/// hlen u8（5）——跨版本变值（12/32），读者按 hlen 分派。
pub const HLEN_OFF: usize = 5;
/// rsv u16 恒 0（6..8）——扩展只经 hlen 追加，不塞 rsv。
pub const RSV_OFF: usize = 6;
/// counter u32 LE（8..12）——v0/v1 同位（hlen 扩展只追加不改前缀布局）。
pub const COUNTER_OFF: usize = 8;
/// payload_len u32 LE（12..16）——v1 新增。
pub const PAYLOAD_LEN_OFF: usize = 12;
/// crc32 u32 LE（16..20）——v1 新增；覆盖域 = payload 全幅不含头。
pub const CRC_OFF: usize = 16;
/// rsv2 12B 恒 0（20..32）——保留区（v2 元数据继续经 hlen 扩展追加）。
pub const RSV2_OFF: usize = 20;

/// blob magic（`"BTMB"` 的 u32 LE 立值）。
pub const BTMB_MAGIC: u32 = u32::from_le_bytes(*b"BTMB");

// —— CRC32（§3.1：berry-sys 纯 raw syscall 镜像章程塞不得；本 crate 内嵌
//    bitwise 实现——64KiB 面上性能非约束（pause 窗 10s ≫ ms 级），查表不做）——

/// CRC-32（IEEE 802.3 反射形）：bitwise 实现，poly `0xEDB88320`、init/
/// final xor `0xFFFFFFFF`——`tools/berry/src/png.rs` `Crc32` 同算法镜像
/// （镜像互指：改动任一侧须过本 crate host 向量钉板——"123456789" →
/// `0xCBF43926`、空输入基线 = 0、金件 blob 逐字节快照）。
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xFFFF_FFFF
}

// —— 写侧（§3.2 写侧规则：state_save 恒写 v1 头——无版本协商，v0 写者
//    随 app-2 刀2 同刀消亡；零长 payload 合法 =「空态保存」）——

/// 编码 v1 头（32B）：`out` 全幅覆写（rsv/rsv2 恒 0）。`payload_len` 与
/// `crc` 由调用方对同一 payload 域算好（`crc32` 本 crate 单源）。
pub fn encode_head_v1(out: &mut [u8; HEAD_V1], counter: u32, payload_len: u32, crc: u32) {
    out[MAGIC_OFF..MAGIC_OFF + 4].copy_from_slice(&BTMB_MAGIC.to_le_bytes());
    out[VER_OFF] = VER_V1;
    out[HLEN_OFF] = HEAD_V1 as u8;
    out[RSV_OFF..RSV_OFF + 2].fill(0);
    out[COUNTER_OFF..COUNTER_OFF + 4].copy_from_slice(&counter.to_le_bytes());
    out[PAYLOAD_LEN_OFF..PAYLOAD_LEN_OFF + 4].copy_from_slice(&payload_len.to_le_bytes());
    out[CRC_OFF..CRC_OFF + 4].copy_from_slice(&crc.to_le_bytes());
    out[RSV2_OFF..HEAD_V1].fill(0);
}

// —— 读侧分派（§3.2 hlen 分派序）——

/// 头分派三分型 + v0 兼容臂（Zero/Restored/BadFmt 的纯面：payload 与
/// crc 的过验在调用方读出 payload 后经 [`check_len`]/[`crc_matches`]
/// 收口）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadKind {
    /// 非 BTMB（零态）——全新启动语义（Fresh）。
    Zero,
    /// BTMB ∧ ver=0 ∧ hlen=12：v0 路径（旧档兼容读）——counter 位续接，
    /// payload 域不存在（v0 blob 只有头）。
    V0 { counter: u32 },
    /// BTMB ∧ ver=1 ∧ hlen=32：v1 路径——len/crc 过验前的中介态。
    V1 {
        counter: u32,
        payload_len: u32,
        crc: u32,
    },
    /// BTMB ∧ (ver 超前 ∨ hlen ∉ {12,32} ∨ ver/hlen 混合对)——弃档
    /// BadFmt（v0 读者同款白名单判据 `ver==0∧hlen==12` 的收口语义）。
    Bad,
}

/// 读头分派。`head` 须 ≥ [`HEAD_V1`]（v1 读者恒读满 32B——state VMO
/// commit 全幅，读 32B 结构性可行；v0 头只消费前 12B）。
pub fn classify_head(head: &[u8]) -> HeadKind {
    assert!(head.len() >= HEAD_V1, "head slice must cover hlen=32");
    let magic = u32::from_le_bytes([
        head[MAGIC_OFF],
        head[MAGIC_OFF + 1],
        head[MAGIC_OFF + 2],
        head[MAGIC_OFF + 3],
    ]);
    if magic != BTMB_MAGIC {
        return HeadKind::Zero;
    }
    let counter = u32::from_le_bytes([
        head[COUNTER_OFF],
        head[COUNTER_OFF + 1],
        head[COUNTER_OFF + 2],
        head[COUNTER_OFF + 3],
    ]);
    // 白名单对 (ver, hlen)：恰 (0,12) 与 (1,32) 两组——混合对（如 ver=1 ∧
    // hlen=12）落 Bad（v0 读者 `ver==VER ∧ hlen==HLEN` 复合判据同义收口，
    // 不 misparse）。
    match (head[VER_OFF], head[HLEN_OFF] as usize) {
        (VER_V0, HEAD_V0) => HeadKind::V0 { counter },
        (VER_V1, HEAD_V1) => HeadKind::V1 {
            counter,
            payload_len: u32::from_le_bytes([
                head[PAYLOAD_LEN_OFF],
                head[PAYLOAD_LEN_OFF + 1],
                head[PAYLOAD_LEN_OFF + 2],
                head[PAYLOAD_LEN_OFF + 3],
            ]),
            crc: u32::from_le_bytes([
                head[CRC_OFF],
                head[CRC_OFF + 1],
                head[CRC_OFF + 2],
                head[CRC_OFF + 3],
            ]),
        },
        _ => HeadKind::Bad,
    }
}

/// v1 读侧量纲校验（§3.2 读侧 buf 契约 + len 超界臂）。
///
/// 序：先档侧（`payload_len` > [`PAYLOAD_CAP`] = 档坏——诚实写者结构性
/// 不可达，len 域被翻的腐败面）后调用方侧（> `buf.len()` = buf-short
/// ——满帽拒读，不截断不半档）。两臂同落 BadFmt 但诊断分词不同
/// （§3.2：buf-short ≠ 档坏）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LenIssue {
    /// len 超界（> 64KiB-32B）——档坏臂。
    OverCap,
    /// payload_len > buf.len()（buf 不足帽）——调用方契约臂。
    BufShort,
}

/// v1 payload 量纲预检（读 VMO 前置：拒读分支不产生半档读）。
pub fn check_len(payload_len: u32, buf_len: usize) -> Result<(), LenIssue> {
    if payload_len as usize > PAYLOAD_CAP {
        return Err(LenIssue::OverCap);
    }
    if payload_len as usize > buf_len {
        return Err(LenIssue::BufShort);
    }
    Ok(())
}

/// v1 payload CRC 过验（`payload` 长度须已 == 头中 payload_len——由调用
/// 方按 len 读满后送检；空 payload 基线 = crc32(&[]) == 0 过验 =
/// 「空态保存」合法形）。
pub fn crc_matches(crc: u32, payload: &[u8]) -> bool {
    crc32(payload) == crc
}

// —— 8B 帧（§2.2：帧⇄(tag,counter) 纯函数；与 svcmgr tomb 面 wire 逐字节
//    同源——迁移不改协议字节 = 刀2 判据线）——

/// lifecycle 消息定长（8B）。
pub const FRAME_SZ: usize = 8;

/// 上报：全新启动。
pub const TAG_UP: u8 = 1;
/// 上报：业务节拍心跳（库代发、节拍由应用主循环驱动）。
pub const TAG_BEAT: u8 = 2;
/// 上报：解冻回执（判据 = 帧本身，counter 带当拍值）。
pub const TAG_RESUMED: u8 = 3;
/// 下行：解冻通知（appd close(token) 后）。
pub const TAG_RESUME: u8 = 4;
/// 上报：读档复活（counter 续接 = N+1）。
pub const TAG_RESTORED: u8 = 5;
/// 下行：暂停令 v1（无参——counter 位恒 0）。
pub const TAG_PAUSESV1: u8 = 6;
/// 上报：保存回执（写成功才发）。
pub const TAG_SAVED: u8 = 7;
/// 下行：AM-1.5 WAKE 臂冻结窗探针。
pub const TAG_PROBE: u8 = 8;
/// 上报：探针回环回执。
pub const TAG_PROBED: u8 = 9;

/// 解码后的 8B 帧。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub tag: u8,
    pub counter: u32,
}

/// 编码 8B 帧：`out` 全幅覆写（tag @0 + pad 3B 恒 0 + counter u32 LE）。
pub fn encode_frame(out: &mut [u8; FRAME_SZ], tag: u8, counter: u32) {
    out[0] = tag;
    out[1..4].fill(0);
    out[4..8].copy_from_slice(&counter.to_le_bytes());
}

/// 解码 8B 帧（pad 位不校验——对端权威在 svcmgr wait_frame 的
/// tag+counter 对账面，读侧只取语义域）。
pub fn decode_frame(f: &[u8; FRAME_SZ]) -> Frame {
    Frame {
        tag: f[0],
        counter: u32::from_le_bytes([f[4], f[5], f[6], f[7]]),
    }
}

// —— counter 代持状态机（§2.2 立法：counter = 协议对账域，库代持——应用
//    payload 不含协议域；RESTORED c=N+1 续接对账由库兑现）——

/// 库代持 counter（主循环线程独占口径——五口契约 §2.2，无锁 static mut
/// 落 berry-libc；本结构只做纯转移）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counter {
    v: u32,
}

impl Counter {
    /// 全新起点（0）。
    pub const fn new() -> Self {
        Counter { v: 0 }
    }

    /// 幕① Fresh/BadFmt 路径：置 0，帧 UP c=0。
    pub fn on_up(&mut self) -> u32 {
        self.v = 0;
        self.v
    }

    /// 幕① Restored 路径：archived N → 置 N+1（帧 RESTORED c=N+1——
    /// wrapping 同 svcmgr `saved_c.wrapping_add(1)` 对账口径）。
    pub fn on_restored(&mut self, archived: u32) -> u32 {
        self.v = archived.wrapping_add(1);
        self.v
    }

    /// 幕③ 心跳拍：+1（帧 BEAT c=当拍），返当拍值（诊断辅证）。
    pub fn on_beat(&mut self) -> u32 {
        self.v = self.v.wrapping_add(1);
        self.v
    }

    /// 当拍值：RESUMED 回帧携带 / state_save 写头取值（均不变更状态）。
    pub const fn current(&self) -> u32 {
        self.v
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

// ---- 测试（host；cfg(test) 放开 std——默认测试挂具，berry-signal 同款）----
//
// 验收面 = 草案 §6 刀1 host 测清单全量：头 roundtrip/hlen=12 v0 分派/
// v1 正路径/坏档四型/counter 翻转域外钉板/CRC 校验值 + 金件快照/前缀
// 12B 布局同位两条/payload 帽两臂/读侧 buf 两臂/三分型全转移/8B 帧
// roundtrip/counter 代持全序。CRC 黄金值以 zlib（python3 独立实现）
// 预产对拍，非本 crate 自证。

#[cfg(test)]
mod tests {
    use super::*;

    // —— CRC32（png.rs 镜像钉板：黄金值 = python3 zlib 独立预产）——

    /// 标准校验值向量（草案 §3.1 钉板值）。
    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    /// 空输入基线（init 直出 0——「空态保存」crc 过验的根据，§3.2）。
    #[test]
    fn crc32_empty_baseline() {
        assert_eq!(crc32(&[]), 0);
    }

    /// 金件 payload `"berry"`（5B）——金件 blob 快照的 crc 域值。
    #[test]
    fn crc32_golden_payload() {
        assert_eq!(crc32(b"berry"), 0x4A8D_BAD3);
    }

    // —— 头编解码 roundtrip + 布局立法 ——

    #[test]
    fn head_v1_roundtrip() {
        let mut h = [0u8; HEAD_V1];
        encode_head_v1(&mut h, 7, 5, crc32(b"berry"));
        match classify_head(&h) {
            HeadKind::V1 { counter, payload_len, crc } => {
                assert_eq!(counter, 7);
                assert_eq!(payload_len, 5);
                assert_eq!(crc, 0x4A8D_BAD3);
            }
            other => panic!("classify_head: 意外分型 {other:?}"),
        }
        // 零长 payload 合法（空态保存）：len=0 ∧ crc(空)=0 过验。
        let mut z = [0u8; HEAD_V1];
        encode_head_v1(&mut z, 42, 0, crc32(&[]));
        match classify_head(&z) {
            HeadKind::V1 { counter, payload_len, crc } => {
                assert_eq!((counter, payload_len, crc), (42, 0, 0));
            }
            other => panic!("classify_head: 意外分型 {other:?}"),
        }
    }

    /// v0 旧头兼容分派：手排 12B ver=0/hlen=12 → V0{counter}（§3.2 表
    /// 第 2 行——旧头新读天然兼容）。
    #[test]
    fn head_v0_dispatch() {
        let mut h = [0u8; HEAD_V1];
        h[..4].copy_from_slice(&BTMB_MAGIC.to_le_bytes());
        h[VER_OFF] = VER_V0;
        h[HLEN_OFF] = HEAD_V0 as u8;
        h[COUNTER_OFF..COUNTER_OFF + 4].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(classify_head(&h), HeadKind::V0 { counter: 9 });
        // 12B 后的字节（v1 域）不影响 v0 分派——v0 读者只消费前 12B。
        h[PAYLOAD_LEN_OFF] = 0xFF;
        assert_eq!(classify_head(&h), HeadKind::V0 { counter: 9 });
    }

    /// 零态：非 BTMB（全零/垃圾 magic）→ Zero（Fresh——全新启动语义）。
    #[test]
    fn head_zero_state() {
        assert_eq!(classify_head(&[0u8; HEAD_V1]), HeadKind::Zero);
        let mut h = [0u8; HEAD_V1];
        h[..4].copy_from_slice(b"TOMB"); // 非 BTMB magic
        assert_eq!(classify_head(&h), HeadKind::Zero);
    }

    /// 坏档四型之 ver/hlen 两型 + 混合对（§3.2 末行：ver 超前 ∨ hlen ∉
    /// {12,32} → BadFmt；混合对同落 Bad——不 misparse）。
    #[test]
    fn head_bad_shapes() {
        let mk = |ver: u8, hlen: u8| -> [u8; HEAD_V1] {
            let mut h = [0u8; HEAD_V1];
            h[..4].copy_from_slice(&BTMB_MAGIC.to_le_bytes());
            h[VER_OFF] = ver;
            h[HLEN_OFF] = hlen;
            h
        };
        assert_eq!(classify_head(&mk(VER_V1 + 1, HEAD_V1 as u8)), HeadKind::Bad); // ver 超前
        assert_eq!(classify_head(&mk(0xFF, 0xFF)), HeadKind::Bad); // 双域乱
        assert_eq!(classify_head(&mk(VER_V1, HEAD_V0 as u8)), HeadKind::Bad); // 混合对
        assert_eq!(classify_head(&mk(VER_V0, HEAD_V1 as u8)), HeadKind::Bad); // 混合对
        assert_eq!(classify_head(&mk(VER_V1, 33)), HeadKind::Bad); // hlen 异值
        assert_eq!(classify_head(&mk(VER_V1, 0)), HeadKind::Bad); // hlen 异值（0）
    }

    /// 前缀 12B 布局同位 (a)：offset 常量断言（R1——magic/counter 段跨
    /// 版本不动；ver@4/hlen@5 属变值域不进等值断言）。
    #[test]
    fn prefix_layout_offsets() {
        const _: () = assert!(MAGIC_OFF == 0);
        const _: () = assert!(VER_OFF == 4 && HLEN_OFF == 5 && RSV_OFF == 6);
        const _: () = assert!(COUNTER_OFF == 8); // v0/v1 同位
        const _: () = assert!(PAYLOAD_LEN_OFF == 12 && CRC_OFF == 16 && RSV2_OFF == 20);
        const _: () = assert!(HEAD_V1 == 32 && HEAD_V0 == 12);
        const _: () = assert!(PAYLOAD_CAP == 64 * 1024 - 32);
        // 金件上验证 offset 语义（非仅常量）：magic 字节序 + counter LE 落位。
        let mut h = [0u8; HEAD_V1];
        encode_head_v1(&mut h, 0x0302_0100, 0, 0);
        assert_eq!(&h[MAGIC_OFF..MAGIC_OFF + 4], b"BTMB");
        assert_eq!(
            &h[COUNTER_OFF..COUNTER_OFF + 4],
            &[0x00, 0x01, 0x02, 0x03]
        );
    }

    /// 前缀 12B 布局同位 (b)：v1 头喂 v0 白名单读者（ver==0 ∧ hlen==12
    /// 判定）→ 落 badfmt 臂（§3.2 末行 host 侧对称臂——新头旧读安全红，
    /// 不 misparse）。
    #[test]
    fn v1_head_before_v0_reader_is_badfmt() {
        let mut h = [0u8; HEAD_V1];
        encode_head_v1(&mut h, 7, 0, 0);
        // v0 读者白名单判据（apptomb v0 :215 同式）：
        let v0_reader_accepts =
            h[VER_OFF] == VER_V0 && h[HLEN_OFF] as usize == HEAD_V0;
        assert!(!v0_reader_accepts, "v0 读者不得接受 v1 头");
        // 本 crate 分派同向：v1 头不落 V0 臂。
        assert!(matches!(classify_head(&h), HeadKind::V1 { .. }));
    }

    /// 金件 blob 逐字节快照（counter=7/payload="berry"——黄金值经
    /// python3 zlib 独立预产，改头布局/字节序必炸此锚）。
    #[test]
    fn golden_blob_snapshot() {
        let mut head = [0u8; HEAD_V1];
        encode_head_v1(&mut head, 7, 5, 0x4A8D_BAD3);
        let mut blob = [0u8; HEAD_V1 + 5];
        blob[..HEAD_V1].copy_from_slice(&head);
        blob[HEAD_V1..].copy_from_slice(b"berry");
        let want: [u8; HEAD_V1 + 5] = [
            b'B', b'T', b'M', b'B', // magic LE
            1, 32, 0, 0, // ver=1 / hlen=32 / rsv=0
            7, 0, 0, 0, // counter=7 LE
            5, 0, 0, 0, // payload_len=5 LE
            0xD3, 0xBA, 0x8D, 0x4A, // crc32("berry")=0x4A8DBAD3 LE
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // rsv2 12B
            b'b', b'e', b'r', b'r', b'y', // payload
        ];
        assert_eq!(blob, want);
    }

    /// counter 翻转域外钉板：金件 blob 翻转 counter 单字节 → 仍 V1
    /// {counter=翻转值} 非 Bad——「CRC 不护头」契约（§3.1 头域完整性
    /// 分工：counter 单错由 svcmgr 对账兜底〔rctr 臂〕，不进 host 断言）。
    #[test]
    fn counter_flip_stays_v1() {
        let mut h = [0u8; HEAD_V1];
        encode_head_v1(&mut h, 7, 5, 0x4A8D_BAD3);
        h[COUNTER_OFF] ^= 0x01; // 7 → 6（单字节翻转）
        assert_eq!(
            classify_head(&h),
            HeadKind::V1 { counter: 6, payload_len: 5, crc: 0x4A8D_BAD3 }
        );
    }

    // —— 读侧量纲两臂（§3.2 读侧 buf 契约 + len 超界）——

    /// payload 帽边界两臂（写侧 TooLarge 的纯面基线）。
    #[test]
    fn payload_cap_arms() {
        assert_eq!(check_len(PAYLOAD_CAP as u32, PAYLOAD_CAP), Ok(())); // 恰帽过
        assert_eq!(
            check_len(PAYLOAD_CAP as u32 + 1, PAYLOAD_CAP),
            Err(LenIssue::OverCap) // 越帽拒（档坏臂）
        );
    }

    /// 读侧 buf 两臂：足帽过 / 不足帽拒（buf-short 分词 ≠ 档坏）。
    #[test]
    fn buf_arms() {
        assert_eq!(check_len(100, PAYLOAD_CAP), Ok(())); // 满帽 buf 对任意合法 len 过
        assert_eq!(check_len(100, 99), Err(LenIssue::BufShort));
        // 序钉板：len 超界优先于 buf-short（档坏是档侧属性，分词不随调用方 buf 漂）。
        assert_eq!(
            check_len(u32::MAX, 1),
            Err(LenIssue::OverCap)
        );
    }

    /// crc 不符臂（坏档四型第四型——payload 单字节翻 → 拒）。
    #[test]
    fn crc_mismatch_arm() {
        let mut p = *b"berry";
        assert!(crc_matches(0x4A8D_BAD3, &p));
        p[0] ^= 0x01;
        assert!(!crc_matches(0x4A8D_BAD3, &p));
        // 空态保存：len=0 ∧ crc(空)=0 过验。
        assert!(crc_matches(0, &[]));
    }

    // —— 8B 帧 roundtrip（wire 逐字节同源 svcmgr）——

    #[test]
    fn frame_roundtrip_bytes() {
        let mut f = [0u8; FRAME_SZ];
        encode_frame(&mut f, TAG_SAVED, 0xDEAD_BEEF);
        // 逐字节：tag @0 + pad 恒 0 + counter u32 LE。
        assert_eq!(f, [7, 0, 0, 0, 0xEF, 0xBE, 0xAD, 0xDE]);
        assert_eq!(
            decode_frame(&f),
            Frame { tag: TAG_SAVED, counter: 0xDEAD_BEEF }
        );
        // 全 tag 族 roundtrip（1..9 封闭值表——svcmgr tomb 面同套）。
        let tags = [
            TAG_UP, TAG_BEAT, TAG_RESUMED, TAG_RESUME, TAG_RESTORED, TAG_PAUSESV1, TAG_SAVED,
            TAG_PROBE, TAG_PROBED,
        ];
        for (i, &t) in tags.iter().enumerate() {
            encode_frame(&mut f, t, (i as u32) * 7919);
            assert_eq!(f[0], t);
            assert_eq!(&f[1..4], &[0, 0, 0]);
            assert_eq!(decode_frame(&f).counter, (i as u32) * 7919);
        }
        // 覆写净度：前帧 counter 高位不残留（pad + 全幅覆写）。
        encode_frame(&mut f, TAG_UP, 0);
        assert_eq!(f, [1, 0, 0, 0, 0, 0, 0, 0]);
    }

    // —— counter 代持全序（§2.2：UP=0 / RESTORED=N+1 / RESUMED=当拍 /
    //     save 写头=当拍值——刀2 迁移后 app 侧 c 变量消亡的库侧承接）——

    #[test]
    fn counter_full_sequence() {
        let mut c = Counter::new();
        assert_eq!(c.on_up(), 0); // 幕① Fresh：UP c=0
        assert_eq!(c.on_beat(), 1); // 幕③：BEAT c=1
        assert_eq!(c.on_beat(), 2);
        assert_eq!(c.current(), 2); // RESUMED 回帧携带当拍（不变更）
        // 幕① Restored(N=2)（relaunch 轮）：置 3、帧 RESTORED c=3。
        let mut c2 = Counter::new();
        assert_eq!(c2.on_restored(2), 3);
        assert_eq!(c2.current(), 3);
        assert_eq!(c2.on_beat(), 4);
        // save 写头取当拍值：encode 与 current 同源。
        let mut h = [0u8; HEAD_V1];
        encode_head_v1(&mut h, c2.current(), 0, crc32(&[]));
        match classify_head(&h) {
            HeadKind::V1 { counter, payload_len, .. } => {
                assert_eq!(counter, 4);
                assert_eq!(payload_len, 0);
            }
            other => panic!("classify_head: 意外分型 {other:?}"),
        }
        // wrapping 续接（u32::MAX → 0——svcmgr `wrapping_add` 对账同口径）。
        let mut c3 = Counter::new();
        assert_eq!(c3.on_restored(u32::MAX), 0);
        assert_eq!(c3.on_beat(), 1);
    }

    /// 三分型状态机全转移总览（Zero→Fresh / V0→Restored 续接 / V1 过验→
    /// Restored(n) / V1 拒→BadFmt 按零态——§3.2 表逐行）。
    #[test]
    fn trichotomy_all_transitions() {
        // 行 1：零态 → Fresh（库发 UP）。
        assert_eq!(classify_head(&[0u8; HEAD_V1]), HeadKind::Zero);
        // 行 2：v0 → 兼容读（counter 续接，payload 域不存在 → Restored(0)）。
        let mut v0 = [0u8; HEAD_V1];
        v0[..4].copy_from_slice(&BTMB_MAGIC.to_le_bytes());
        v0[VER_OFF] = VER_V0;
        v0[HLEN_OFF] = HEAD_V0 as u8;
        v0[COUNTER_OFF..COUNTER_OFF + 4].copy_from_slice(&11u32.to_le_bytes());
        match classify_head(&v0) {
            HeadKind::V0 { counter } => assert_eq!(Counter::new().on_restored(counter), 12),
            other => panic!("classify_head: 意外分型 {other:?}"),
        }
        // 行 3/4/5：v1 len/crc 过验 → Restored(n)；buf 不足 → BadFmt
        // （buf-short 分词）；len 超界/crc 不符 → BadFmt 弃档。
        let mut v1 = [0u8; HEAD_V1];
        encode_head_v1(&mut v1, 3, 5, crc32(b"berry"));
        match classify_head(&v1) {
            HeadKind::V1 { counter, payload_len, crc } => {
                assert!(check_len(payload_len, PAYLOAD_CAP).is_ok());
                assert!(crc_matches(crc, b"berry"));
                assert_eq!(Counter::new().on_restored(counter), 4); // RESTORED c=4
            }
            other => panic!("classify_head: 意外分型 {other:?}"),
        }
        // 行 7/8：跨版本两翼（旧头新读兼容 / 新头旧读 badfmt 红已各自钉板）。
    }
}
