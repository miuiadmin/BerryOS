//! berry-npuproto —— virtio-compute wire 编解码（ai-runtime 契约二.2
//! Tier 1 客机侧首刀；侦查件：`.agents/research/2026-11-22-ai-runtime-
//! tier1宿主直通可行性侦查.md`）。virtio 无标准 compute 设备——本 crate
//! 即 wire 契约单源（对拍纪律同 berry-virtio gpu.rs：wire 事实注释钉板、
//! 编不出即拒 None 不截断）。算子语义单源在 berry-tensor（op 值镜像对齐，
//! 见 [`Op`]）；宿主侧 Metal Compute 桥与 `.bmod` 页帧共享 DMA 归
//! tools/berry 模拟器面（边界外，owner 声明队列）。
//!
//! ## wire 事实（全 LE）
//!
//! - 设备 config（20B）：`{0 magic "VNPU"[4], 4 version u32（=1）,
//!   8 caps u32（bit0=matmul_q4、bit1=rmsnorm、bit2=dot_q4_q8）,
//!   12 max_queues u16, 14 queue_pow2 u8（2..=11）, 15 reserved[5]（零）}`。
//! - 命令头（24B，virtio-gpu 控制头同形）：`{type u32, flags u32,
//!   fence_id u64, ctx_id u32, ring u8, pad[3]}`。
//! - MATVEC 描述符（头 + 32B）：`{w_blob u64, x_blob u64, y_blob u64,
//!   rows u32, blocks u32}`——blob 句柄指设备侧注册页帧表条目。
//! - RMSNORM 描述符（头 + 24B）：`{x_blob u64, y_blob u64, len u32,
//!   eps_bits u32（f32 位形）}`。
//! - blob 表条目（24B）：`{handle u64, off u64（4096 对齐）, len u64}`。
//! - 完成环条目（16B）：`{fence_id u64, status u32（0=OK、1=算子域外、
//!   2=超时看门狗复位）、pad u32}`。

#![cfg_attr(not(test), no_std)]

/// 设备 config 幅。
pub const CONFIG_SZ: usize = 20;
/// 命令头幅（virtio-gpu 控制头同形）。
pub const CMD_HDR_SZ: usize = 24;
/// MATVEC 描述符载荷幅。
pub const OP_MATVEC_BODY_SZ: usize = 32;
/// RMSNORM 描述符载荷幅。
pub const OP_RMSNORM_BODY_SZ: usize = 24;
/// blob 表条目幅。
pub const BLOB_ENTRY_SZ: usize = 24;
/// 完成环条目幅。
pub const COMPLETION_SZ: usize = 16;
/// 设备 magic。
pub const MAGIC: &[u8; 4] = b"VNPU";
/// wire 版本（唯一接受值）。
pub const VERSION: u32 = 1;
/// 页帧对齐（blob off 执法值——与 .bmod 张量段同物理事实）。
pub const PAGE: u64 = 4096;

/// caps 位。
pub const CAP_MATVEC: u32 = 1 << 0;
pub const CAP_RMSNORM: u32 = 1 << 1;
/// op 值与 berry-tensor 算子族对齐（语义单源在彼）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    MatvecQ4Q8,
    Rmsnorm,
}

impl Op {
    pub const fn tag(self) -> u32 {
        match self {
            Self::MatvecQ4Q8 => 0x100,
            Self::Rmsnorm => 0x101,
        }
    }
    pub const fn from_tag(t: u32) -> Option<Self> {
        match t {
            0x100 => Some(Self::MatvecQ4Q8),
            0x101 => Some(Self::Rmsnorm),
            _ => None,
        }
    }
}

fn le16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}
fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
fn le64(b: &[u8]) -> u64 {
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}
fn put16(b: &mut [u8], v: u16) {
    b[..2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], v: u32) {
    b[..4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], v: u64) {
    b[..8].copy_from_slice(&v.to_le_bytes());
}

/// 解析后的设备 config。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub caps: u32,
    pub max_queues: u16,
    pub queue_pow2: u8,
}

/// config 解析（畸形即 None——family 纪律：编不出就拒绝，不截断）。
pub fn parse_config(buf: &[u8]) -> Option<Config> {
    if buf.len() < CONFIG_SZ || &buf[0..4] != MAGIC {
        return None;
    }
    if le32(&buf[4..8]) != VERSION {
        return None;
    }
    if buf[15..20].iter().any(|&b| b != 0) {
        return None;
    }
    let qp = buf[14];
    if !(2..=11).contains(&qp) {
        return None;
    }
    Some(Config { caps: le32(&buf[8..12]), max_queues: le16(&buf[12..14]), queue_pow2: qp })
}

/// config 编码（恰 [`CONFIG_SZ`] 字节写入 `out`）。
pub fn encode_config(c: &Config, out: &mut [u8]) -> Option<()> {
    if out.len() < CONFIG_SZ {
        return None;
    }
    out[0..4].copy_from_slice(MAGIC);
    put32(&mut out[4..8], VERSION);
    put32(&mut out[8..12], c.caps);
    put16(&mut out[12..14], c.max_queues);
    out[14] = c.queue_pow2;
    out[15..20].fill(0);
    Some(())
}

/// 描述符（已解出的 op + 载荷字段——语义单源 berry-tensor）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Descriptor {
    MatvecQ4Q8 { fence: u64, ctx: u32, w: u64, x: u64, y: u64, rows: u32, blocks: u32 },
    Rmsnorm { fence: u64, ctx: u32, x: u64, y: u64, len: u32, eps_bits: u32 },
}

/// 命令描述符解码（头 + op 载荷；畸形/域外 op 即 None）。
pub fn parse_descriptor(buf: &[u8]) -> Option<Descriptor> {
    if buf.len() < CMD_HDR_SZ {
        return None;
    }
    let op = le32(&buf[0..4]);
    let flags = le32(&buf[4..8]);
    if flags != 0 {
        return None; // v1 保留位非零拒（前向兼容显式为零）
    }
    let fence = le64(&buf[8..16]);
    let ctx = le32(&buf[16..20]);
    if buf[21..24].iter().any(|&b| b != 0) {
        return None;
    }
    match Op::from_tag(op)? {
        Op::MatvecQ4Q8 => {
            if buf.len() < CMD_HDR_SZ + OP_MATVEC_BODY_SZ {
                return None;
            }
            let b = &buf[CMD_HDR_SZ..];
            Some(Descriptor::MatvecQ4Q8 {
                fence,
                ctx,
                w: le64(&b[0..8]),
                x: le64(&b[8..16]),
                y: le64(&b[16..24]),
                rows: le32(&b[24..28]),
                blocks: le32(&b[28..32]),
            })
        }
        Op::Rmsnorm => {
            if buf.len() < CMD_HDR_SZ + OP_RMSNORM_BODY_SZ {
                return None;
            }
            let b = &buf[CMD_HDR_SZ..];
            Some(Descriptor::Rmsnorm {
                fence,
                ctx,
                x: le64(&b[0..8]),
                y: le64(&b[8..16]),
                len: le32(&b[16..20]),
                eps_bits: le32(&b[20..24]),
            })
        }
    }
}

/// 描述符编码（写入 `out`，返回消费字节数）。
pub fn encode_descriptor(d: &Descriptor, out: &mut [u8]) -> Option<usize> {
    match *d {
        Descriptor::MatvecQ4Q8 { fence, ctx, w, x, y, rows, blocks } => {
            if out.len() < CMD_HDR_SZ + OP_MATVEC_BODY_SZ {
                return None;
            }
            put32(&mut out[0..4], Op::MatvecQ4Q8.tag());
            put32(&mut out[4..8], 0);
            put64(&mut out[8..16], fence);
            put32(&mut out[16..20], ctx);
            out[20] = 0;
            out[21..24].fill(0);
            let b = &mut out[CMD_HDR_SZ..];
            put64(&mut b[0..8], w);
            put64(&mut b[8..16], x);
            put64(&mut b[16..24], y);
            put32(&mut b[24..28], rows);
            put32(&mut b[28..32], blocks);
            Some(CMD_HDR_SZ + OP_MATVEC_BODY_SZ)
        }
        Descriptor::Rmsnorm { fence, ctx, x, y, len, eps_bits } => {
            if out.len() < CMD_HDR_SZ + OP_RMSNORM_BODY_SZ {
                return None;
            }
            put32(&mut out[0..4], Op::Rmsnorm.tag());
            put32(&mut out[4..8], 0);
            put64(&mut out[8..16], fence);
            put32(&mut out[16..20], ctx);
            out[20] = 0;
            out[21..24].fill(0);
            let b = &mut out[CMD_HDR_SZ..];
            put64(&mut b[0..8], x);
            put64(&mut b[8..16], y);
            put32(&mut b[16..20], len);
            put32(&mut b[20..24], eps_bits);
            Some(CMD_HDR_SZ + OP_RMSNORM_BODY_SZ)
        }
    }
}

/// blob 页帧表条目（off 页对齐执法——零拷贝映射前提，与 .bmod 同律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobEntry {
    pub handle: u64,
    pub off: u64,
    pub len: u64,
}

pub fn parse_blob(buf: &[u8]) -> Option<BlobEntry> {
    if buf.len() < BLOB_ENTRY_SZ {
        return None;
    }
    let e = BlobEntry { handle: le64(&buf[0..8]), off: le64(&buf[8..16]), len: le64(&buf[16..24]) };
    if e.len == 0 || e.off % PAGE != 0 {
        return None;
    }
    Some(e)
}

pub fn encode_blob(e: &BlobEntry, out: &mut [u8]) -> Option<()> {
    if out.len() < BLOB_ENTRY_SZ || e.len == 0 || e.off % PAGE != 0 {
        return None;
    }
    put64(&mut out[0..8], e.handle);
    put64(&mut out[8..16], e.off);
    put64(&mut out[16..24], e.len);
    Some(())
}

/// 完成态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionStatus {
    Ok,
    /// op 值域外（宿主不识——客机侧降级 Tier 0 依据）。
    Unsupported,
    /// 看门狗复位（>100ms 单批次——契约二.3 降级语义）。
    Watchdog,
}

/// 完成环条目解码。
pub fn parse_completion(buf: &[u8]) -> Option<(u64, CompletionStatus)> {
    if buf.len() < COMPLETION_SZ || le32(&buf[12..16]) != 0 {
        return None;
    }
    let st = match le32(&buf[8..12]) {
        0 => CompletionStatus::Ok,
        1 => CompletionStatus::Unsupported,
        2 => CompletionStatus::Watchdog,
        _ => return None,
    };
    Some((le64(&buf[0..8]), st))
}

pub fn encode_completion(fence: u64, st: CompletionStatus, out: &mut [u8]) -> Option<()> {
    if out.len() < COMPLETION_SZ {
        return None;
    }
    let s = match st {
        CompletionStatus::Ok => 0u32,
        CompletionStatus::Unsupported => 1,
        CompletionStatus::Watchdog => 2,
    };
    put64(&mut out[0..8], fence);
    put32(&mut out[8..12], s);
    put32(&mut out[12..16], 0);
    Some(())
}

/// virtio 标准描述符表条目（16B：addr u64 + len u32 + flags u16 +
/// next u16——链式描述符；compute 面常用旗：WRITE = 设备可写）。
pub const DESC_SZ: usize = 16;
/// 描述符旗：设备写（结果缓冲方向）。
pub const DESC_F_WRITE: u16 = 1 << 1;
/// avail 环头幅（flags + idx；ring 体按 queue_pow2 供电侧定容）。
pub const AVAIL_HDR_SZ: usize = 4;
/// used 环条目（8B：id u32 + len u32）。
pub const USED_ENTRY_SZ: usize = 8;

/// 描述符条目解码（畸形即 None）。
pub fn parse_desc(buf: &[u8]) -> Option<(u64, u32, u16, u16)> {
    if buf.len() < DESC_SZ {
        return None;
    }
    Some((
        le64(&buf[0..8]),
        le32(&buf[8..12]),
        le16(&buf[12..14]),
        le16(&buf[14..16]),
    ))
}

/// 描述符条目编码。
pub fn encode_desc(out: &mut [u8], addr: u64, len: u32, flags: u16, next: u16) -> Option<()> {
    if out.len() < DESC_SZ {
        return None;
    }
    put64(&mut out[0..8], addr);
    put32(&mut out[8..12], len);
    put16(&mut out[12..14], flags);
    put16(&mut out[14..16], next);
    Some(())
}

/// avail 环头解码（flags, idx）。
pub fn parse_avail_hdr(buf: &[u8]) -> Option<(u16, u16)> {
    if buf.len() < AVAIL_HDR_SZ {
        return None;
    }
    Some((le16(&buf[0..2]), le16(&buf[2..4])))
}

/// avail 环头编码 + ring 槽写入（`head` = 描述符链头号；`qsize` 幂2
/// 容量——槽址 = hdr + idx % qsize）。
pub fn avail_push(buf: &mut [u8], qsize_pow2: usize, head: u16) -> Option<u16> {
    if buf.len() < AVAIL_HDR_SZ + qsize_pow2 * 2 {
        return None;
    }
    let (_, idx) = parse_avail_hdr(buf)?;
    let slot = idx as usize % qsize_pow2;
    put16(&mut buf[AVAIL_HDR_SZ + slot * 2..], head);
    let next = idx.wrapping_add(1);
    put16(&mut buf[2..4], next);
    Some(next)
}

/// used 环条目解码（idx 槽——设备侧完成写回）。
pub fn used_entry(buf: &[u8], qsize_pow2: usize, idx: u16) -> Option<(u32, u32)> {
    if buf.len() < AVAIL_HDR_SZ + qsize_pow2 as usize * USED_ENTRY_SZ {
        return None;
    }
    let slot = idx as usize % qsize_pow2;
    let e = &buf[AVAIL_HDR_SZ + slot * USED_ENTRY_SZ..];
    Some((le32(&e[0..4]), le32(&e[4..8])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_and_rejects() {
        let mut buf = [0u8; CONFIG_SZ];
        let c = Config { caps: CAP_MATVEC | CAP_RMSNORM, max_queues: 2, queue_pow2: 8 };
        encode_config(&c, &mut buf).unwrap();
        assert_eq!(parse_config(&buf), Some(c));
        buf[0] = b'X';
        assert_eq!(parse_config(&buf), None); // magic
        let mut b2 = [0u8; CONFIG_SZ];
        encode_config(&c, &mut b2).unwrap();
        b2[4] = 2;
        assert_eq!(parse_config(&b2), None); // 版本
        let mut b3 = [0u8; CONFIG_SZ];
        encode_config(&c, &mut b3).unwrap();
        b3[14] = 12;
        assert_eq!(parse_config(&b3), None); // queue_pow2 域外
        b3[14] = 1;
        assert_eq!(parse_config(&b3), None); // pow2 < 2
        let mut b4 = [0u8; CONFIG_SZ];
        encode_config(&c, &mut b4).unwrap();
        b4[15] = 1;
        assert_eq!(parse_config(&b4), None); // reserved 非零
    }

    #[test]
    fn matvec_descriptor_roundtrip() {
        let d = Descriptor::MatvecQ4Q8 { fence: 0xaabb, ctx: 7, w: 1, x: 2, y: 3, rows: 64, blocks: 128 };
        let mut buf = [0u8; CMD_HDR_SZ + OP_MATVEC_BODY_SZ];
        let n = encode_descriptor(&d, &mut buf).unwrap();
        assert_eq!(n, buf.len());
        assert_eq!(parse_descriptor(&buf), Some(d));
        // 截断即 None。
        assert_eq!(parse_descriptor(&buf[..buf.len() - 1]), None);
    }

    #[test]
    fn rmsnorm_descriptor_roundtrip() {
        let d = Descriptor::Rmsnorm { fence: 42, ctx: 1, x: 10, y: 11, len: 512, eps_bits: 0x3dcccccd };
        let mut buf = [0u8; CMD_HDR_SZ + OP_RMSNORM_BODY_SZ];
        encode_descriptor(&d, &mut buf).unwrap();
        assert_eq!(parse_descriptor(&buf), Some(d));
    }

    #[test]
    fn descriptor_rejects() {
        // flags 非零（保留位执法）。
        let mut buf = [0u8; CMD_HDR_SZ + OP_MATVEC_BODY_SZ];
        let d = Descriptor::MatvecQ4Q8 { fence: 1, ctx: 1, w: 1, x: 2, y: 3, rows: 1, blocks: 1 };
        encode_descriptor(&d, &mut buf).unwrap();
        buf[4] = 1;
        assert_eq!(parse_descriptor(&buf), None);
        // 头保留段非零。
        let mut b2 = [0u8; CMD_HDR_SZ + OP_MATVEC_BODY_SZ];
        encode_descriptor(&d, &mut b2).unwrap();
        b2[22] = 9;
        assert_eq!(parse_descriptor(&b2), None);
        // op 域外。
        let mut b3 = [0u8; CMD_HDR_SZ + OP_MATVEC_BODY_SZ];
        encode_descriptor(&d, &mut b3).unwrap();
        b3[0] = 0xff;
        assert_eq!(parse_descriptor(&b3), None);
    }

    #[test]
    fn blob_table_page_alignment_law() {
        let e = BlobEntry { handle: 5, off: 8192, len: 4096 };
        let mut buf = [0u8; BLOB_ENTRY_SZ];
        encode_blob(&e, &mut buf).unwrap();
        assert_eq!(parse_blob(&buf), Some(e));
        // off 非页对齐拒（零拷贝映射前提——与 .bmod 同律）。
        let bad = BlobEntry { handle: 5, off: 4097, len: 4096 };
        assert_eq!(encode_blob(&bad, &mut buf), None);
        let mut b2 = [0u8; BLOB_ENTRY_SZ];
        encode_blob(&e, &mut b2).unwrap();
        b2[8] = 1; // off → 非对齐
        assert_eq!(parse_blob(&b2), None);
        // len 0 拒。
        let zero = BlobEntry { handle: 5, off: 4096, len: 0 };
        assert_eq!(encode_blob(&zero, &mut [0u8; BLOB_ENTRY_SZ]), None);
    }

    #[test]
    fn completion_roundtrip_and_domain() {
        for (st, v) in [
            (CompletionStatus::Ok, 0u32),
            (CompletionStatus::Unsupported, 1),
            (CompletionStatus::Watchdog, 2),
        ] {
            let mut buf = [0u8; COMPLETION_SZ];
            encode_completion(77, st, &mut buf).unwrap();
            assert_eq!(parse_completion(&buf), Some((77, st)));
            assert_eq!(le32(&buf[8..12]), v);
        }
        // status 域外与 pad 非零拒。
        let mut buf = [0u8; COMPLETION_SZ];
        encode_completion(1, CompletionStatus::Ok, &mut buf).unwrap();
        buf[8] = 9;
        assert_eq!(parse_completion(&buf), None);
        let mut b2 = [0u8; COMPLETION_SZ];
        encode_completion(1, CompletionStatus::Ok, &mut b2).unwrap();
        b2[12] = 1;
        assert_eq!(parse_completion(&b2), None);
    }

    #[test]
    fn desc_roundtrip_and_write_flag() {
        let mut buf = [0u8; DESC_SZ];
        encode_desc(&mut buf, 0x5000, 4096, DESC_F_WRITE, 3).unwrap();
        assert_eq!(parse_desc(&buf), Some((0x5000, 4096, DESC_F_WRITE, 3)));
        assert_eq!(parse_desc(&buf[..DESC_SZ - 1]), None);
        assert_eq!(encode_desc(&mut buf[..8], 0, 0, 0, 0), None);
    }

    #[test]
    fn avail_push_wraps_and_increments() {
        // qsize 4：连推 5 头——idx 0..5、槽环回。
        let mut buf = [0u8; AVAIL_HDR_SZ + 4 * 2];
        for i in 0..5u16 {
            let idx = avail_push(&mut buf, 4, i * 10).unwrap();
            assert_eq!(idx, i + 1);
        }
        assert_eq!(parse_avail_hdr(&buf), Some((0, 5)));
        // 槽 0 被第 5 推覆写（idx 4 % 4 = 0 → head 40）。
        assert_eq!(le16(&buf[AVAIL_HDR_SZ..AVAIL_HDR_SZ + 2]), 40);
        assert!(avail_push(&mut buf[..6], 4, 0).is_none()); // 缓冲不足
    }

    #[test]
    fn used_entry_layout() {
        let mut buf = [0u8; AVAIL_HDR_SZ + 2 * USED_ENTRY_SZ];
        put32(&mut buf[AVAIL_HDR_SZ..AVAIL_HDR_SZ + 4], 7);
        put32(&mut buf[AVAIL_HDR_SZ + 4..AVAIL_HDR_SZ + 8], 128);
        assert_eq!(used_entry(&buf, 2, 0), Some((7, 128)));
        assert_eq!(used_entry(&buf, 2, 2), Some((7, 128))); // 环回同槽
        assert!(used_entry(&buf[..8], 2, 0).is_none());
    }

    #[test]
    fn op_tag_domain() {
        assert_eq!(Op::from_tag(Op::MatvecQ4Q8.tag()), Some(Op::MatvecQ4Q8));
        assert_eq!(Op::from_tag(Op::Rmsnorm.tag()), Some(Op::Rmsnorm));
        assert_eq!(Op::from_tag(0), None);
    }
}
