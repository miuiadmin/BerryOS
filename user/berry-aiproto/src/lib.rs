//! berry-aiproto —— `.bmod` 页对齐安全模型容器格式纯函数 crate
//! （ai-runtime 总体设计契约一.1 首刀：`.agents/plans/ai-runtime/design.md`
//! 「零反序列化直接映射格式 `.bmod`」——模型文件 = Ed25519 签名头 + 有界
//! TLV 超参数表 + 4096B 页对齐只读张量数据段，从格式层根绝反序列化代码
//! 执行漏洞：没有任意 Pickle、没有脚本、没有嵌套容器，只有定宽头 +
//! 平铺 TLV + 定宽段表，未知字段一律透传不解释）。「解析器即产品」
//! 家族形态（berry-fatfs/berry-partition 同款）：零 unsafe、零依赖、
//! 零 IO、零分配——整文件镜像经 `&[u8]` 借入，全接口零拷贝切片返回；
//! `cfg_attr(not(test), no_std)`：库本体 no_std（workspace none-target
//! 全量构建可编），test 构建放开 std 仅为测试挂具。host 测试经 justfile
//! test-host 显式行跑。消费面 = airtd 模型池按需调页 VMO 映射（契约
//! 一.2，后续刀；本刀纯新增 crate 不动消费面）。
//!
//! ## 结构执法纪律（严格 loud 拒）
//!
//! - 头部一页闭合：`header_len` ≤ [`PAGE`]（4096）且 ≥ [`PREAMBLE`] +
//!   `tlv_len` + `seg_table_len`——超参数表的有界性由页 0 物理闭合派生
//!   （契约「超参数有界 TLV 表」的机器形态），无第二套数字帽。
//! - TLV 面：平铺 `{tag u8, len u8, value}`，尾部条目截断即
//!   [`Error::TruncatedTlv`]；语义类型（ctx_len/vocab 等）归消费侧解释，
//!   本库只保证良构。
//! - 段表面：`seg_table_len` 必须恰为 [`SEG_ENTRY`]×`seg_count`，条目
//!   数为 0 即 [`Error::BadSegCount`]（无张量的模型文件拒收）。
//! - 张量段：绝对偏移 `off ≥ PAGE` 且 `off % PAGE == 0`（页对齐是
//!   零拷贝 mmap 的直接映射前提——不对齐即 [`Error::SegNotPageAligned`]/
//!   [`Error::SegOffBelowPage`]）；`len > 0` 且 `off + len ≤ 文件长`
//!   （[`Error::SegOutOfBounds`]）；量化格式 tag 域外即
//!   [`Error::UnknownQuant`]；条目 reserved 非零即 [`Error::BadReserved`]
//!   （前向兼容面：未知位必须显式为零，静默吞未知位 = 漂移温床）。
//! - **不执法面**：段间重叠不拒（tied weights 等别名共享是合法形态，
//!   归消费侧策略）；Ed25519 签名的密码学校验归消费侧（berry-crypto/
//!   airtd——本库只钉 [`Bmod::signed_span`] 的覆盖域与
//!   [`Bmod::signature`] 的字节面）；`flags` 除已定义位外不校验
//!   （版本化前向兼容）。
//!
//! ## 字段布局（全 LE 多字节，v1）
//!
//! - 前导（[`PREAMBLE`] = 96B）：`{0 magic "BMOD"[4], 4 version u16（=1）,
//!   6 flags u32（bit0=加密扩展卷）, 10 sig_algo u16（=0 Ed25519）,
//!   12 header_len u32, 16 signature[64], 80 tlv_len u32, 84 seg_count u16,
//!   86 seg_table_len u32, 90 reserved[6]（全零）}`。
//! - TLV 区（偏移 96 起 `tlv_len` 字节）：平铺条目 `{tag u8, len u8,
//!   value[len]}`。
//! - 段表区（TLV 后 `seg_table_len` 字节）：每条 [`SEG_ENTRY`] = 24B
//!   `{quant u16, reserved u16（=0）, off u64, len u64}`。
//! - 签名覆盖域 = 除前导外的全文件（偏移 [`PREAMBLE`]..EOF，含页 0 尾
//!   填充、TLV、段表与全部张量页）——固定前导携签名自身，故排除；
//!   覆盖域由 [`Bmod::signed_span`] 单源给出。

#![cfg_attr(not(test), no_std)]

use core::fmt;

/// `.bmod` 文件魔数（偏移 0 起 4B）。
pub const MAGIC: &[u8; 4] = b"BMOD";
/// 当前格式版本（唯一接受值）。
pub const FORMAT_VERSION: u16 = 1;
/// 签名算法 tag 0 = Ed25519（唯一接受值；新算法走版本升）。
pub const SIG_ALGO_ED25519: u16 = 0;
/// flags bit0：加密扩展卷形态（`/data/models/` 按需下载面；明文系统卷
/// `/system/models/` 不置位——解密归消费侧，本库只透传）。
pub const FLAG_ENCRYPTED: u32 = 1 << 0;
/// 页大小（字节）——张量段对齐粒度与头部闭合帽共用同一物理事实。
pub const PAGE: u32 = 4096;
/// 固定前导长（字节）——签名覆盖域自此处起。
pub const PREAMBLE: u32 = 96;
/// 段表条目长（字节）。
pub const SEG_ENTRY: u32 = 24;

/// 量化格式 tag（段表 `quant` 域唯一接受集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantFormat {
    /// 4-bit 分组量化（每组 32 权重 + 1×FP16 scale）。
    Q4_0,
    /// 4-bit K-quant 中档（superblock 校准变体）。
    Q4KM,
    /// 8-bit 分组量化。
    Q8_0,
    /// 半精度浮点。
    Fp16,
    /// Brain float 16（指数域宽）。
    Bf16,
}

impl QuantFormat {
    /// tag → 枚举（域外 None）。
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Q4_0),
            2 => Some(Self::Q4KM),
            3 => Some(Self::Q8_0),
            4 => Some(Self::Fp16),
            5 => Some(Self::Bf16),
            _ => None,
        }
    }
    /// 枚举 → tag。
    pub const fn tag(self) -> u16 {
        match self {
            Self::Q4_0 => 1,
            Self::Q4KM => 2,
            Self::Q8_0 => 3,
            Self::Fp16 => 4,
            Self::Bf16 => 5,
        }
    }
}

/// 解析失败（loud 拒——每个变体对应一条被执法的结构律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// 魔数失配（偏移 0 非 "BMOD"）。
    BadMagic,
    /// 版本失配（非 v1）。
    BadVersion,
    /// 签名算法 tag 域外（非 0=Ed25519）。
    BadSigAlgo,
    /// `header_len` 越界（> PAGE 或 < 前导+TLV+段表的实际和）。
    BadHeaderLen,
    /// 前导 reserved[6] 非全零。
    BadPreambleReserved,
    /// TLV 区尾部条目截断（len 声明越过区尾）。
    TruncatedTlv,
    /// `seg_table_len` ≠ SEG_ENTRY×`seg_count`。
    SegTableLenMismatch,
    /// `seg_count` 为 0（无张量段的模型文件拒收）。
    BadSegCount,
    /// 段条目 reserved 非零。
    BadReserved,
    /// 量化格式 tag 域外。
    UnknownQuant,
    /// 段绝对偏移落在页 0（< PAGE——头部区不可被张量覆盖）。
    SegOffBelowPage,
    /// 段偏移非 4096 对齐（零拷贝映射前提破坏）。
    SegNotPageAligned,
    /// 段长为 0 或 `off + len` 越过文件尾。
    SegOutOfBounds,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::BadMagic => "magic 失配（非 BMOD）",
            Self::BadVersion => "版本失配（非 v1）",
            Self::BadSigAlgo => "签名算法 tag 域外（非 0=Ed25519）",
            Self::BadHeaderLen => "header_len 越界（页 0 闭合律破坏）",
            Self::BadPreambleReserved => "前导 reserved 非全零",
            Self::TruncatedTlv => "TLV 尾部条目截断",
            Self::SegTableLenMismatch => "seg_table_len ≠ 24×seg_count",
            Self::BadSegCount => "seg_count 为 0",
            Self::BadReserved => "段条目 reserved 非零",
            Self::UnknownQuant => "量化格式 tag 域外",
            Self::SegOffBelowPage => "段偏移落在页 0",
            Self::SegNotPageAligned => "段偏移非 4096 对齐",
            Self::SegOutOfBounds => "段长 0 或越过文件尾",
        };
        f.write_str(s)
    }
}

/// 解析后的 `.bmod` 视图（借入整文件镜像，全接口零拷贝）。
#[derive(Debug, Clone, Copy)]
pub struct Bmod<'a> {
    image: &'a [u8],
    flags: u32,
    tlv: &'a [u8],
    seg_table: &'a [u8],
}

/// 平铺 TLV 超参数条目（零拷贝视图）。
#[derive(Debug, Clone, Copy)]
pub struct HyperParam<'a> {
    pub tag: u8,
    pub value: &'a [u8],
}

/// 段表条目（零拷贝视图，张量字节经 [`Bmod::tensor`] 取）。
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    pub quant: QuantFormat,
    /// 绝对偏移（已执法 ≥ PAGE 且页对齐）。
    pub off: u64,
    /// 字节长（已执法 > 0 且不越文件尾）。
    pub len: u64,
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

impl<'a> Bmod<'a> {
    /// 解析并结构执法整文件镜像（loud 拒；无分配、无 IO）。
    pub fn parse(image: &'a [u8]) -> Result<Self, Error> {
        if image.len() < PREAMBLE as usize || &image[0..4] != MAGIC {
            return Err(Error::BadMagic);
        }
        if le16(&image[4..6]) != FORMAT_VERSION {
            return Err(Error::BadVersion);
        }
        let flags = le32(&image[6..10]);
        if le16(&image[10..12]) != SIG_ALGO_ED25519 {
            return Err(Error::BadSigAlgo);
        }
        let header_len = le32(&image[12..16]);
        let tlv_len = le32(&image[80..84]) as usize;
        let seg_count = le16(&image[84..86]) as usize;
        let seg_table_len = le32(&image[86..90]) as usize;
        if image[90..96].iter().any(|&b| b != 0) {
            return Err(Error::BadPreambleReserved);
        }
        // 页 0 闭合律：TLV 有界性的唯一数字帽（单一事实，无第二套上限）。
        let body = PREAMBLE as usize + tlv_len + seg_table_len;
        if header_len < body as u32 || header_len > PAGE {
            return Err(Error::BadHeaderLen);
        }
        if image.len() < body {
            return Err(Error::BadHeaderLen);
        }
        if seg_count == 0 {
            return Err(Error::BadSegCount);
        }
        if seg_table_len != SEG_ENTRY as usize * seg_count {
            return Err(Error::SegTableLenMismatch);
        }
        // TLV 良构：平铺步进必须恰好耗尽全区（尾条目头/value 截断均拒，
        // 读 len 字节前先界检——畸形输入零 panic 是本库底线）。
        let tlv = &image[PREAMBLE as usize..PREAMBLE as usize + tlv_len];
        let mut p = 0usize;
        while p < tlv.len() {
            if p + 2 > tlv.len() {
                return Err(Error::TruncatedTlv);
            }
            let l = tlv[p + 1] as usize;
            p += 2 + l;
        }
        if p != tlv.len() {
            return Err(Error::TruncatedTlv);
        }
        let seg_table =
            &image[PREAMBLE as usize + tlv_len..PREAMBLE as usize + tlv_len + seg_table_len];
        let m = Self { image, flags, tlv, seg_table };
        // 段表逐条执法（借构造器复用同一读路径，测试构造镜像同律）。
        for i in 0..seg_count {
            _ = m.entry(i)?;
        }
        Ok(m)
    }

    fn entry(&self, i: usize) -> Result<Segment, Error> {
        let e = &self.seg_table[i * SEG_ENTRY as usize..(i + 1) * SEG_ENTRY as usize];
        let quant = QuantFormat::from_tag(le16(&e[0..2]))
            .ok_or(Error::UnknownQuant)?;
        if le16(&e[2..4]) != 0 {
            return Err(Error::BadReserved);
        }
        let off = le64(&e[8..16]);
        let len = le64(&e[16..24]);
        if off < PAGE as u64 {
            return Err(Error::SegOffBelowPage);
        }
        if off % PAGE as u64 != 0 {
            return Err(Error::SegNotPageAligned);
        }
        if len == 0 || off > self.image.len() as u64
            || len > self.image.len() as u64 - off
        {
            return Err(Error::SegOutOfBounds);
        }
        Ok(Segment { quant, off, len })
    }

    /// flags 面（bit0 = [`FLAG_ENCRYPTED`]）。
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Ed25519 签名字节（64B；密码学校验归消费侧）。
    pub fn signature(&self) -> &'a [u8; 64] {
        self.image[16..80].try_into().unwrap()
    }

    /// 签名覆盖域（偏移 PREAMBLE..EOF 的零拷贝切片——含 TLV、段表、
    /// 页 0 尾填充与全部张量页；固定前导携签名自身故排除）。
    pub fn signed_span(&self) -> &'a [u8] {
        &self.image[PREAMBLE as usize..]
    }

    /// Ed25519 签名校验（R20）：覆盖域单源 [`Bmod::signed_span`]，原语
    /// 走 berry-crypto（wycheproof 在册）；`pk` = 模型分发公钥。
    pub fn verify_signature(&self, pk: &[u8; 32]) -> bool {
        berry_crypto::ed25519::verify(pk, self.signature(), self.signed_span())
    }

    /// 平铺超参数条目迭代（良构已执法；语义解释归消费侧）。
    pub fn hyperparams(&self) -> impl Iterator<Item = HyperParam<'a>> + '_ {
        let mut p = 0usize;
        core::iter::from_fn(move || {
            if p >= self.tlv.len() {
                return None;
            }
            let tag = self.tlv[p];
            let l = self.tlv[p + 1] as usize;
            let v = &self.tlv[p + 2..p + 2 + l];
            p += 2 + l;
            Some(HyperParam { tag, value: v })
        })
    }

    /// 按 tag 取首条超参数值（未命中 None）。
    pub fn hyper(&self, tag: u8) -> Option<&'a [u8]> {
        self.hyperparams().find(|h| h.tag == tag).map(|h| h.value)
    }

    /// 段数。
    pub fn seg_count(&self) -> usize {
        self.seg_table.len() / SEG_ENTRY as usize
    }

    /// 段表条目（已执法视图）。
    pub fn segment(&self, i: usize) -> Segment {
        self.entry(i).expect("parse 已执法，索引域内恒 Ok")
    }

    /// 第 i 段张量字节（零拷贝）。
    pub fn tensor(&self, i: usize) -> &'a [u8] {
        let s = self.segment(i);
        &self.image[s.off as usize..(s.off + s.len) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成镜像构造器（与 v1 布局同源；测试专用，非产品面）。
    struct Fixture {
        buf: Vec<u8>,
    }

    impl Fixture {
        fn new(tlv: &[(u8, &[u8])], segs: &[(u16, u64)]) -> Self {
            let tlv_len: usize = tlv.iter().map(|(_, v)| 2 + v.len()).sum();
            let seg_len = 24 * segs.len();
            let mut buf = vec![0u8; PREAMBLE as usize + tlv_len + seg_len];
            buf[0..4].copy_from_slice(MAGIC);
            buf[4..6].copy_from_slice(&1u16.to_le_bytes());
            buf[6..10].copy_from_slice(&0u32.to_le_bytes());
            buf[10..12].copy_from_slice(&0u16.to_le_bytes());
            buf[12..16].copy_from_slice(
                &(PREAMBLE + tlv_len as u32 + seg_len as u32).to_le_bytes(),
            );
            // signature 摄 64B 非零样式字节（校验零拷贝回读）。
            for (i, b) in buf[16..80].iter_mut().enumerate() {
                *b = (i % 251) as u8;
            }
            buf[80..84].copy_from_slice(&(tlv_len as u32).to_le_bytes());
            buf[84..86]
                .copy_from_slice(&(segs.len() as u16).to_le_bytes());
            buf[86..90]
                .copy_from_slice(&(seg_len as u32).to_le_bytes());
            let mut p = PREAMBLE as usize;
            for (tag, v) in tlv {
                buf[p] = *tag;
                buf[p + 1] = v.len() as u8;
                buf[p + 2..p + 2 + v.len()].copy_from_slice(v);
                p += 2 + v.len();
            }
            let mut off = 4096u64;
            for (i, (quant, len)) in segs.iter().enumerate() {
                let e = PREAMBLE as usize + tlv_len + 24 * i;
                buf[e..e + 2].copy_from_slice(&quant.to_le_bytes());
                buf[e + 8..e + 16].copy_from_slice(&off.to_le_bytes());
                buf[e + 16..e + 24].copy_from_slice(&len.to_le_bytes());
                off += len.div_ceil(4096) * 4096;
            }
            buf.resize(off as usize, 0xA5);
            Self { buf }
        }
    }

    const TAG_CTX: u8 = 1;

    #[test]
    fn roundtrip_ok() {
        let f = Fixture::new(
            &[(TAG_CTX, &1024u32.to_le_bytes()), (7, b"bert")],
            &[(QuantFormat::Q4_0.tag(), 4096), (QuantFormat::Fp16.tag(), 4100)],
        );
        let m = Bmod::parse(&f.buf).unwrap();
        assert_eq!(m.flags(), 0);
        assert_eq!(m.seg_count(), 2);
        assert_eq!(m.hyper(TAG_CTX), Some(&1024u32.to_le_bytes()[..]));
        assert_eq!(m.hyper(7), Some(&b"bert"[..]));
        assert_eq!(m.hyper(9), None);
        let hs: Vec<_> = m.hyperparams().collect();
        assert_eq!(hs.len(), 2);
        assert_eq!(m.segment(0).quant, QuantFormat::Q4_0);
        assert_eq!(m.segment(0).off, 4096);
        assert_eq!(m.segment(1).off, 8192); // 首段 4096B 占一页后对齐推进
        assert_eq!(m.tensor(1).len(), 4100);
        assert_eq!(m.tensor(1)[0], 0xA5);
    }

    #[test]
    fn signature_and_signed_span() {
        let f = Fixture::new(&[], &[(QuantFormat::Q8_0.tag(), 4096)]);
        let m = Bmod::parse(&f.buf).unwrap();
        assert_eq!(m.signature().len(), 64);
        assert_eq!(m.signature()[0], 0);
        assert_eq!(m.signature()[63], 63 % 251);
        // 覆盖域 = 前导外全文件。
        assert_eq!(m.signed_span().len(), f.buf.len() - PREAMBLE as usize);
    }

    #[test]
    fn bad_magic() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        f.buf[0] = b'X';
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::BadMagic);
    }

    #[test]
    fn bad_version() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        f.buf[4..6].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::BadVersion);
    }

    #[test]
    fn bad_sig_algo() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        f.buf[10..12].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::BadSigAlgo);
    }

    #[test]
    fn bad_preamble_reserved() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        f.buf[95] = 1;
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::BadPreambleReserved);
    }

    #[test]
    fn header_len_page_closure() {
        // TLV 超过页 0 容量：header_len 必然 > PAGE，页 0 闭合律拒。
        let big = [7u8; 4000];
        let f = Fixture::new(&[(2, &big)], &[(1, 4096)]);
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::BadHeaderLen);
    }

    #[test]
    fn header_len_below_body() {
        let mut f = Fixture::new(&[(1, &[9, 9])], &[(1, 4096)]);
        f.buf[12..16].copy_from_slice(&(PREAMBLE + 4 + 24 - 1).to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::BadHeaderLen);
    }

    #[test]
    fn truncated_tlv() {
        let mut f = Fixture::new(&[(1, &[1, 2, 3, 4])], &[(1, 4096)]);
        // 尾条目截断：tlv_len 声明覆盖到条目头但不含全部 value。
        f.buf[80..84].copy_from_slice(&5u32.to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::TruncatedTlv);
    }

    #[test]
    fn tlv_one_byte_tail() {
        let mut f = Fixture::new(&[(1, &[1, 2, 3, 4])], &[(1, 4096)]);
        // 1 字节尾巴：len 字节本身缺失——界检腿，畸形输入零 panic。
        f.buf[80..84].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::TruncatedTlv);
    }

    #[test]
    fn seg_table_len_mismatch() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        // 段表声明 25B ≠ 24×1；header_len 同步抬到 121 保持页 0 闭合律
        // 绿——单打段表失配腿（否则 BadHeaderLen 先响）。
        f.buf[12..16].copy_from_slice(&121u32.to_le_bytes());
        f.buf[86..90].copy_from_slice(&25u32.to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::SegTableLenMismatch);
    }

    #[test]
    fn seg_count_zero() {
        let f = Fixture::new(&[], &[]);
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::BadSegCount);
    }

    #[test]
    fn seg_off_below_page() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        let e = PREAMBLE as usize;
        f.buf[e + 8..e + 16].copy_from_slice(&0u64.to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::SegOffBelowPage);
    }

    #[test]
    fn seg_not_page_aligned() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        let e = PREAMBLE as usize;
        f.buf[e + 8..e + 16].copy_from_slice(&4097u64.to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::SegNotPageAligned);
    }

    #[test]
    fn seg_out_of_bounds() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        let e = PREAMBLE as usize;
        f.buf[e + 16..e + 24].copy_from_slice(&4097u64.to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::SegOutOfBounds);
        f.buf[e + 16..e + 24].copy_from_slice(&0u64.to_le_bytes());
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::SegOutOfBounds);
    }

    #[test]
    fn unknown_quant() {
        let f = Fixture::new(&[], &[(6, 4096)]);
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::UnknownQuant);
    }

    #[test]
    fn seg_reserved_nonzero() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        f.buf[PREAMBLE as usize + 2] = 9;
        assert_eq!(Bmod::parse(&f.buf).unwrap_err(), Error::BadReserved);
    }

    #[test]
    fn verify_signature_roundtrip_tamper_and_wrong_key() {
        // 真密钥对（berry-crypto 原语自产）：签名覆盖域与校验胶接对拍。
        let sk = [7u8; 32];
        let pk = berry_crypto::ed25519::public_key(&sk);
        let mut f = Fixture::new(&[(1, &42u32.to_le_bytes())], &[(1, 4096)]);
        // 签名覆盖域 = signed_span；写回头部签名槽。
        let span_start = PREAMBLE as usize;
        let span = f.buf[span_start..].to_vec();
        let sig = berry_crypto::ed25519::sign(&sk, &span);
        f.buf[16..80].copy_from_slice(&sig);
        let m = Bmod::parse(&f.buf).unwrap();
        assert!(m.verify_signature(&pk));
        // 篡改张量字节 → 拒。
        let last = f.buf.len() - 1;
        f.buf[last] ^= 0x01;
        let m2 = Bmod::parse(&f.buf).unwrap();
        assert!(!m2.verify_signature(&pk));
        // 错公钥 → 拒。
        let bad_pk = [9u8; 32];
        assert!(!m2.verify_signature(&bad_pk));
        // 恢复后原签名仍过（结构未变）。
        f.buf[last] ^= 0x01;
        let m3 = Bmod::parse(&f.buf).unwrap();
        assert!(m3.verify_signature(&pk));
    }

    #[test]
    fn encrypted_flag_passthrough() {
        let mut f = Fixture::new(&[], &[(1, 4096)]);
        f.buf[6..10].copy_from_slice(&FLAG_ENCRYPTED.to_le_bytes());
        let m = Bmod::parse(&f.buf).unwrap();
        assert_eq!(m.flags() & FLAG_ENCRYPTED, FLAG_ENCRYPTED);
    }

    #[test]
    fn truncated_image_rejected() {
        let f = Fixture::new(&[], &[(1, 4096)]);
        let m = &f.buf[..f.buf.len() - 1];
        // 段越界（off+len 越文件尾）而非 panic——截尾镜像 loud 拒。
        assert_eq!(Bmod::parse(m).unwrap_err(), Error::SegOutOfBounds);
        assert_eq!(Bmod::parse(&f.buf[..10]).unwrap_err(), Error::BadMagic);
    }
}
