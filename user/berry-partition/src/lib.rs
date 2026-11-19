//! berry-partition —— GPT/MBR 分区表解析纯函数 crate（存储栈 P0 缺口
//! 第二项第一刀：`.agents/research/2026-11-18-驱动子系统综合研究报告.md`
//! P0-2「无分区表解析」——fsmgr 卷定位是约定式探测（BPB 签名 + hidden
//! 对账）、vblk 整盘当一分区暴露，全仓无分区表解析器；教学场景需要
//! 学生能看到多分区）。「解析器即产品」家族形态（berry-fatfs/
//! berry-blkproto 同款）：零 unsafe、零依赖、零 IO——盘读经注入闭包
//! [`parse`] 的 `block_reader`，库本体不触盘不分配器外的环境面，host
//! 测试镜像构造即可全验。消费面 = vblk（drivers/bins/vblk 多分区暴露
//! ——下一刀接线，本刀纯新增 crate 不动消费面）。
//!
//! `cfg_attr(not(test), no_std)` + `extern crate alloc`：库本体 no_std
//! （workspace none-target 全量构建可编），test 构建放开 std 仅为测试
//! 挂具；std 消费侧（vblk，musl target）alloc 由 std 提供。host 测试经
//! justfile test-host 显式行跑。
//!
//! ## 解析纪律（严格 loud 拒）
//!
//! - MBR 面：LBA0 尾签名 0x55AA 失配即 [`Error::BadMbrSignature`]；
//!   4×16B 槽位 type=0 跳过（空槽），type=0xEE 即转 GPT（protective
//!   MBR 语义——其余 MBR 槽位无论有无内容全忽略）；lba_start=0（覆盖
//!   分区表自身区）或 sectors=0 即 [`Error::BadEntrySpan`]。
//! - GPT 面：LBA1 头签名 "EFI PART" 失配即 [`Error::BadGptSignature`]；
//!   my_lba≠1、entry_size≠128、num_entries 越帽（0 或 > [`GPT_ENTRIES_MAX`]）
//!   全显式拒；type_guid 全零条目跳过（未用槽）；first_lba=0（覆盖
//!   protective MBR 与头区）或 first>last 即 [`Error::BadEntrySpan`]。
//! - **不执法面**（教学盘像由宿主工具产出，恒规整）：GPT 头 CRC32 与
//!   条目区 CRC32 不校验（消费侧可选加强）；分区 LBA 域与盘容量的对账
//!   （库不知盘尾）归消费侧；MBR/GPT 混合布局（hybrid MBR）不识别——
//!   0xEE 出现即整面转 GPT。
//!
//! ## 字段布局（全 LE 多字节）
//!
//! - MBR 条目（LBA0 偏移 446 起 4×16B）：`{status u8, chs_start[3],
//!   type u8, chs_end[3], lba_start u32le, sectors u32le}`；last_lba =
//!   lba_start + sectors - 1。
//! - GPT 头（LBA1，92B 有效载荷）：偏移 0 签名 "EFI PART"、24 my_lba
//!   u64le、72 partition_entry_lba u64le、80 num_entries u32le、84
//!   entry_size u32le（=128）。
//! - GPT 条目（partition_entry_lba 起 num_entries×128B）：`{type_guid[16],
//!   unique_guid[16], first_lba u64le, last_lba u64le, attrs u64le,
//!   name[72] utf16le}`——name 截首个 NUL，空名 [`None`]，无效代理对
//!   折 U+FFFD（不 loud 拒——名字是展示面非结构面）。

#![cfg_attr(not(test), no_std)]
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// MBR 分区表在 LBA0 内的偏移（4×16B 槽位自此起，尾随 0x55AA 签名）。
pub const MBR_TABLE_OFF: usize = 446;
/// MBR 槽数（经典 4 主分区面；扩展分区链不解析——教学面直式布局）。
pub const MBR_SLOTS: usize = 4;
/// MBR 保护条类型：出现即整面转 GPT 解析（UEFI protective MBR）。
pub const MBR_TYPE_GPT_PROTECTIVE: u8 = 0xEE;
/// GPT 头签名（LBA1 起首 8B）。
pub const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
/// GPT 条目长（本库唯一接受值——UEFI 规范最小值；另要求块长整除之）。
pub const GPT_ENTRY_SIZE: usize = 128;
/// GPT 条目数帽：防病态头把读取面撑爆（真实盘恒 128）。
pub const GPT_ENTRIES_MAX: u32 = 512;
/// 块长下帽：MBR 512B 面 + GPT 头 LBA1 同块落位的最小块长。
pub const BLOCK_SIZE_MIN: u64 = 512;
/// 块长上帽：块缓冲单块驻栈外分配的上界（教学盘 512/4096 恒在带内）。
pub const BLOCK_SIZE_MAX: u64 = 65536;

/// 分区类型识别：MBR 单字节类型码，或 GPT 16B 类型 GUID（原样字节，
/// 不做 GUID 登记表翻译——教学面按字节对拍即可）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionKind {
    /// MBR 面条目（type 字节，如 0x83 Linux、0x0B FAT32）。
    Mbr(u8),
    /// GPT 面条目（type_guid 原样 16B LE 盘序）。
    Gpt([u8; 16]),
}

/// 一个分区：表内槽位号 + 类型 + LBA 域 + 名（GPT 独有——MBR 恒
/// [`None`]；GPT 空名亦 [`None`]）。
///
/// `index` 语义 = 表内槽位号非发现序：MBR 面 1..=4（槽位字面位置），
/// GPT 面即条目号 1..=num_entries（与 UEFI 工具显示的 Partition N 对齐）。
/// `last_lba` 含端（闭区间——GPT 原生语义；MBR 面 = lba_start+sectors-1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    /// 表内槽位号（MBR 1..=4 / GPT 条目号 1..=num_entries）。
    pub index: u32,
    /// 类型识别（MBR type 字节 / GPT type GUID）。
    pub kind: PartitionKind,
    /// 首 LBA（含端）。
    pub first_lba: u64,
    /// 尾 LBA（含端，闭区间）。
    pub last_lba: u64,
    /// 分区名（GPT name 域 utf16le 解码，截 NUL；空或 MBR 面 [`None`]）。
    pub name: Option<String>,
}

/// 解析失败面（严格 loud 拒——「AI 测试友好化」：不依赖 QEMU 就能
/// host 验证全部拒路径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// 块长越 [`BLOCK_SIZE_MIN`]..=[`BLOCK_SIZE_MAX`] 带，或 GPT 面块长
    /// 不整除条目长 128（逐条目按块内偏移切窗的前提失守）。
    BadBlockSize(u64),
    /// 读块 IO 失败（由注入的 `block_reader` 返回，库透传不吞）。
    Reader,
    /// LBA0 尾 0x55AA 签名失配（无 MBR 亦无 protective MBR——裸盘/
    /// 垃圾首块，消费侧决定整盘 superfloppy 语义，本库只报事实）。
    BadMbrSignature,
    /// LBA1 头 "EFI PART" 签名失配。
    BadGptSignature,
    /// GPT 头 my_lba ≠ 1（头自称不在 LBA1——镜像/偏移病态）。
    BadMyLba(u64),
    /// GPT 头 partition_entry_lba 域病态（<2 覆盖头区，或加条目区
    /// 偏移溢出 u64）。
    BadHeaderLba(u64),
    /// GPT 头 entry_size ≠ 128（本库唯一接受值）。
    BadEntrySize(u32),
    /// GPT 头 num_entries 越帽（0 或 > [`GPT_ENTRIES_MAX`]）。
    TooManyEntries(u32),
    /// 分区条目 LBA 域病态：first_lba=0（覆盖表区）或 first>last（倒挂）。
    BadEntrySpan {
        /// 表内槽位号（MBR 1..=4 / GPT 条目号）。
        index: u32,
        /// 病态条目的 first_lba 原值。
        first_lba: u64,
        /// 病态条目的 last_lba 原值。
        last_lba: u64,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadBlockSize(bs) => {
                write!(f, "block size {bs} out of range or not multiple of {GPT_ENTRY_SIZE} for GPT")
            }
            Error::Reader => write!(f, "block read failed (reader-side I/O error)"),
            Error::BadMbrSignature => write!(f, "bad MBR signature (0x55AA) at LBA0"),
            Error::BadGptSignature => write!(f, "bad GPT signature (EFI PART) at LBA1"),
            Error::BadMyLba(v) => write!(f, "GPT header my_lba={v} (expect 1)"),
            Error::BadHeaderLba(v) => write!(f, "GPT header partition_entry_lba={v} out of range"),
            Error::BadEntrySize(v) => write!(f, "GPT entry_size={v} (expect {GPT_ENTRY_SIZE})"),
            Error::TooManyEntries(v) => write!(f, "GPT num_entries={v} out of range"),
            Error::BadEntrySpan { index, first_lba, last_lba } => {
                write!(f, "partition #{index} bad LBA span [{first_lba},{last_lba}]")
            }
        }
    }
}

/// 本库 Result 面（错误恒 [`Error`]）。
pub type Result<T> = core::result::Result<T, Error>;

/// 解析整盘分区表：MBR 起手，type=0xEE 保护条即转 GPT。
///
/// `block_reader(lba, buf)` 契约：把第 `lba` 块的 `buf.len()` 字节填入
/// `buf`（本库恒以 `buf.len() == block_size` 整块调用）；失败返回
/// [`Error::Reader`] 由库透传。`block_size` 越 [`BLOCK_SIZE_MIN`]=
/// [`BLOCK_SIZE_MAX`] 带即 [`Error::BadBlockSize`]。
///
/// 返回顺序恒按表内槽位升序；MBR 面（无 0xEE）返回非空槽条目，GPT 面
/// 返回 type_guid 非零条目（全零条目区 = 合法空表，返回空 Vec）。
pub fn parse<R>(mut block_reader: R, block_size: u64) -> Result<Vec<Partition>>
where
    R: FnMut(u64, &mut [u8]) -> Result<()>,
{
    if !(BLOCK_SIZE_MIN..=BLOCK_SIZE_MAX).contains(&block_size) {
        return Err(Error::BadBlockSize(block_size));
    }
    let mut block = alloc::vec![0u8; block_size as usize];
    // LBA0：MBR 本体（GPT 盘上是 protective MBR——0xEE 条指路）。
    block_reader(0, &mut block)?;
    if block[510] != 0x55 || block[511] != 0xAA {
        return Err(Error::BadMbrSignature);
    }
    let mut parts = Vec::new();
    for slot in 0..MBR_SLOTS as u32 {
        let off = MBR_TABLE_OFF + slot as usize * 16;
        let ptype = block[off + 4];
        if ptype == MBR_TYPE_GPT_PROTECTIVE {
            // 保护条：整面转 GPT，其余 MBR 槽位（无论有无内容）忽略。
            return parse_gpt(&mut block_reader, block_size, &mut block);
        }
        if ptype == 0 {
            continue; // 空槽。
        }
        let first = u32le(&block[off + 8..off + 12]) as u64;
        let sectors = u32le(&block[off + 12..off + 16]) as u64;
        // first=0 覆盖表区、sectors=0 空域——两病态均 loud 拒（u32→u64
        // 加法无溢出面，first+sectors-1 恒安全）。
        if first == 0 || sectors == 0 {
            return Err(Error::BadEntrySpan {
                index: slot + 1,
                first_lba: first,
                last_lba: first,
            });
        }
        parts.push(Partition {
            index: slot + 1,
            kind: PartitionKind::Mbr(ptype),
            first_lba: first,
            last_lba: first + sectors - 1,
            name: None,
        });
    }
    Ok(parts)
}

/// GPT 面：LBA1 头校验 + 条目区逐条切窗（`parse` 内部，0xEE 已见）。
fn parse_gpt<R>(block_reader: &mut R, block_size: u64, block: &mut [u8]) -> Result<Vec<Partition>>
where
    R: FnMut(u64, &mut [u8]) -> Result<()>,
{
    // 条目逐条按块内偏移切窗——前提是块长整除条目长（512/4096 恒满足）。
    if block_size as usize % GPT_ENTRY_SIZE != 0 {
        return Err(Error::BadBlockSize(block_size));
    }
    block_reader(1, block)?;
    if &block[0..8] != &GPT_SIGNATURE[..] {
        return Err(Error::BadGptSignature);
    }
    let my_lba = u64le(&block[24..32]);
    let entry_lba = u64le(&block[72..80]);
    let num_entries = u32le(&block[80..84]);
    let entry_size = u32le(&block[84..88]);
    if my_lba != 1 {
        return Err(Error::BadMyLba(my_lba));
    }
    if entry_size != GPT_ENTRY_SIZE as u32 {
        return Err(Error::BadEntrySize(entry_size));
    }
    if num_entries == 0 || num_entries > GPT_ENTRIES_MAX {
        return Err(Error::TooManyEntries(num_entries));
    }
    // 条目区不能覆盖头区；+条目区字节宽不溢出（u64 病态头防御）。
    let entries_bytes = num_entries as u64 * GPT_ENTRY_SIZE as u64;
    if entry_lba < 2 || entry_lba.checked_add(entries_bytes / block_size).is_none() {
        return Err(Error::BadHeaderLba(entry_lba));
    }
    let mut parts = Vec::new();
    let mut cached_lba = u64::MAX; // 同块多条免重读（512B 块恰容 4 条目）。
    for i in 0..num_entries as u64 {
        let byte_off = i * GPT_ENTRY_SIZE as u64;
        let lba = entry_lba + byte_off / block_size; // 域已上方校验，无溢出。
        let off = (byte_off % block_size) as usize;
        if lba != cached_lba {
            block_reader(lba, block)?;
            cached_lba = lba;
        }
        let e = &block[off..off + GPT_ENTRY_SIZE];
        let mut type_guid = [0u8; 16];
        type_guid.copy_from_slice(&e[0..16]);
        if type_guid == [0u8; 16] {
            continue; // 未用条目。
        }
        let first = u64le(&e[32..40]);
        let last = u64le(&e[40..48]);
        if first == 0 || first > last {
            return Err(Error::BadEntrySpan { index: (i + 1) as u32, first_lba: first, last_lba: last });
        }
        parts.push(Partition {
            index: (i + 1) as u32,
            kind: PartitionKind::Gpt(type_guid),
            first_lba: first,
            last_lba: last,
            name: decode_utf16_name(&e[56..128]),
        });
    }
    Ok(parts)
}

/// GPT name 域解码：utf16le → 截首个 NUL → 空名 [`None`]；无效代理对
/// 折 U+FFFD（名字是展示面非结构面，不 loud 拒）。
fn decode_utf16_name(raw: &[u8]) -> Option<String> {
    let units = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]));
    let full: String = core::char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect();
    let name = full.split('\0').next().unwrap_or("");
    if name.is_empty() {
        None
    } else {
        Some(String::from(name))
    }
}

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

#[cfg(test)]
mod tests {
    use super::*;

    const BS: u64 = 512;

    /// 镜像 reader：LBA→字节窗拷贝，越镜像尾 [`Error::Reader`]。
    fn reader(image: &[u8]) -> impl FnMut(u64, &mut [u8]) -> Result<()> + '_ {
        move |lba, buf| {
            let off = (lba * BS) as usize;
            let end = match off.checked_add(buf.len()) {
                Some(e) if e <= image.len() => e,
                _ => return Err(Error::Reader),
            };
            buf.copy_from_slice(&image[off..end]);
            Ok(())
        }
    }

    fn blank_image(blocks: usize) -> Vec<u8> {
        vec![0u8; blocks * BS as usize]
    }

    /// 盖 MBR 尾签名（不盖的镜像即坏签名用例）。
    fn stamp_mbr(img: &mut [u8]) {
        img[510] = 0x55;
        img[511] = 0xAA;
    }

    /// 写一条 MBR 槽（不盖签名——由 `stamp_mbr` 独立控）。
    fn put_mbr_entry(img: &mut [u8], slot: usize, ptype: u8, lba: u32, sectors: u32) {
        let off = MBR_TABLE_OFF + slot * 16;
        img[off + 4] = ptype;
        img[off + 8..off + 12].copy_from_slice(&lba.to_le_bytes());
        img[off + 12..off + 16].copy_from_slice(&sectors.to_le_bytes());
    }

    /// 盖 GPT 头 + protective MBR（0xEE 条指 LBA1）。
    fn put_gpt_header(img: &mut [u8], num_entries: u32, entry_size: u32) {
        stamp_mbr(img);
        put_mbr_entry(img, 0, 0xEE, 1, 33);
        let h = BS as usize;
        img[h..h + 8].copy_from_slice(b"EFI PART");
        img[h + 24..h + 32].copy_from_slice(&1u64.to_le_bytes()); // my_lba
        img[h + 72..h + 80].copy_from_slice(&2u64.to_le_bytes()); // entry_lba
        img[h + 80..h + 84].copy_from_slice(&num_entries.to_le_bytes());
        img[h + 84..h + 88].copy_from_slice(&entry_size.to_le_bytes());
    }

    /// 写一条 GPT 条目（条目区自 LBA2 起，槽位 i；name utf16le 落位）。
    fn put_gpt_entry(img: &mut [u8], i: usize, guid: [u8; 16], first: u64, last: u64, name: &str) {
        let off = BS as usize * 2 + i * GPT_ENTRY_SIZE;
        img[off..off + 16].copy_from_slice(&guid);
        img[off + 32..off + 40].copy_from_slice(&first.to_le_bytes());
        img[off + 40..off + 48].copy_from_slice(&last.to_le_bytes());
        let units: Vec<u16> = name.encode_utf16().take(35).collect();
        for (k, u) in units.iter().enumerate() {
            img[off + 56 + k * 2..off + 56 + k * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
    }

    #[test]
    fn mbr_two_partitions_parsed() {
        let mut img = blank_image(4);
        stamp_mbr(&mut img);
        put_mbr_entry(&mut img, 0, 0x83, 2048, 4096); // Linux。
        put_mbr_entry(&mut img, 1, 0x0B, 6144, 8192); // FAT32。
        put_mbr_entry(&mut img, 3, 0x07, 14336, 512); // 槽 2 空跳过，槽 3 NTFS。
        let parts = parse(reader(&img), BS).unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].index, 1);
        assert_eq!(parts[0].kind, PartitionKind::Mbr(0x83));
        assert_eq!(parts[0].first_lba, 2048);
        assert_eq!(parts[0].last_lba, 2048 + 4096 - 1); // 含端闭区间。
        assert_eq!(parts[0].name, None);
        assert_eq!(parts[1].index, 2);
        assert_eq!(parts[1].kind, PartitionKind::Mbr(0x0B));
        // 槽位号语义：槽 3（零基）→ index 4，不受空槽 2 压缩。
        assert_eq!(parts[2].index, 4);
        assert_eq!(parts[2].first_lba, 14336);
    }

    #[test]
    fn mbr_all_slots_empty_yields_no_partitions() {
        let mut img = blank_image(1);
        stamp_mbr(&mut img);
        let parts = parse(reader(&img), BS).unwrap();
        assert!(parts.is_empty());
    }

    #[test]
    fn gpt_two_partitions_parsed_with_names() {
        let mut img = blank_image(34); // 头 2 块 + 32 块条目区（128 条）。
        put_gpt_header(&mut img, 128, GPT_ENTRY_SIZE as u32);
        let guid_root: [u8; 16] = [0x48, 0xF3, 0x33, 0x7C, 0x10, 0x4B, 0x4D, 0x2B,
            0xB1, 0xC6, 0x2A, 0x9D, 0xF8, 0x30, 0xB5, 0xC2]; // Linux fs data（乱序样例字节）。
        let guid_data: [u8; 16] = [0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44,
            0x87, 0xC0, 0x68, 0xB6, 0xD7, 0x26, 0x41, 0xFC]; // FAT 混合样例字节。
        put_gpt_entry(&mut img, 0, guid_root, 2048, 10239, "root");
        put_gpt_entry(&mut img, 1, guid_data, 10240, 20479, "数据盘");
        let parts = parse(reader(&img), BS).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].index, 1);
        assert_eq!(parts[0].kind, PartitionKind::Gpt(guid_root));
        assert_eq!(parts[0].first_lba, 2048);
        assert_eq!(parts[0].last_lba, 10239);
        assert_eq!(parts[0].name.as_deref(), Some("root"));
        assert_eq!(parts[1].index, 2);
        assert_eq!(parts[1].name.as_deref(), Some("数据盘")); // utf16 全量往返。
    }

    #[test]
    fn gpt_zero_guid_entries_skipped() {
        let mut img = blank_image(4);
        put_gpt_header(&mut img, 4, GPT_ENTRY_SIZE as u32);
        // 条目区全零 = 合法空表（type_guid 全零即未用条目）。
        let parts = parse(reader(&img), BS).unwrap();
        assert!(parts.is_empty());
    }

    #[test]
    fn protective_mbr_jumps_to_gpt_and_mbr_entries_ignored() {
        let mut img = blank_image(6);
        put_gpt_header(&mut img, 8, GPT_ENTRY_SIZE as u32);
        // 0xEE 槽之外再塞普通 MBR 条（hybrid 形）——必须被 GPT 面整面忽略。
        put_mbr_entry(&mut img, 1, 0x83, 100, 10);
        put_gpt_entry(&mut img, 3, [1u8; 16], 40, 80, "only");
        let parts = parse(reader(&img), BS).unwrap();
        assert_eq!(parts.len(), 1); // MBR 0x83 条不出现，GPT 第 4 条入账。
        assert_eq!(parts[0].index, 4); // GPT 条目号（槽位 3 零基 +1）。
        assert_eq!(parts[0].kind, PartitionKind::Gpt([1u8; 16]));
        assert_eq!(parts[0].name.as_deref(), Some("only"));
    }

    #[test]
    fn bad_mbr_signature_rejected() {
        let img = blank_image(1); // 不盖 0x55AA。
        assert_eq!(parse(reader(&img), BS), Err(Error::BadMbrSignature));
    }

    #[test]
    fn bad_gpt_signature_rejected() {
        let mut img = blank_image(4);
        put_gpt_header(&mut img, 4, GPT_ENTRY_SIZE as u32);
        img[512..516].copy_from_slice(b"XXXX"); // 破头签名（0xEE 仍指 GPT）。
        assert_eq!(parse(reader(&img), BS), Err(Error::BadGptSignature));
    }

    #[test]
    fn gpt_entry_size_mismatch_rejected() {
        let mut img = blank_image(4);
        put_gpt_header(&mut img, 4, 64); // UEFI 最小 128，64 即拒。
        assert_eq!(parse(reader(&img), BS), Err(Error::BadEntrySize(64)));
    }

    #[test]
    fn gpt_my_lba_mismatch_rejected() {
        let mut img = blank_image(4);
        put_gpt_header(&mut img, 4, GPT_ENTRY_SIZE as u32);
        img[512 + 24..512 + 32].copy_from_slice(&5u64.to_le_bytes());
        assert_eq!(parse(reader(&img), BS), Err(Error::BadMyLba(5)));
    }

    #[test]
    fn gpt_num_entries_out_of_range_rejected() {
        let mut img = blank_image(4);
        put_gpt_header(&mut img, 0, GPT_ENTRY_SIZE as u32); // 0 拒。
        assert_eq!(parse(reader(&img), BS), Err(Error::TooManyEntries(0)));
        put_gpt_header(&mut img, GPT_ENTRIES_MAX + 1, GPT_ENTRY_SIZE as u32);
        assert_eq!(parse(reader(&img), BS), Err(Error::TooManyEntries(513)));
    }

    #[test]
    fn gpt_span_inversion_rejected() {
        let mut img = blank_image(4);
        put_gpt_header(&mut img, 4, GPT_ENTRY_SIZE as u32);
        put_gpt_entry(&mut img, 1, [1u8; 16], 500, 100, ""); // first>last。
        assert_eq!(
            parse(reader(&img), BS),
            Err(Error::BadEntrySpan { index: 2, first_lba: 500, last_lba: 100 })
        );
    }

    #[test]
    fn gpt_first_lba_zero_rejected() {
        let mut img = blank_image(4);
        put_gpt_header(&mut img, 4, GPT_ENTRY_SIZE as u32);
        put_gpt_entry(&mut img, 0, [1u8; 16], 0, 100, ""); // 覆盖表区。
        assert_eq!(
            parse(reader(&img), BS),
            Err(Error::BadEntrySpan { index: 1, first_lba: 0, last_lba: 100 })
        );
    }

    #[test]
    fn mbr_zero_sectors_rejected() {
        let mut img = blank_image(1);
        stamp_mbr(&mut img);
        put_mbr_entry(&mut img, 0, 0x83, 100, 0);
        assert_eq!(
            parse(reader(&img), BS),
            Err(Error::BadEntrySpan { index: 1, first_lba: 100, last_lba: 100 })
        );
    }

    #[test]
    fn bad_block_size_rejected() {
        let img = blank_image(4);
        assert_eq!(parse(reader(&img), 256), Err(Error::BadBlockSize(256))); // 下带外。
        assert_eq!(parse(reader(&img), 70000), Err(Error::BadBlockSize(70000))); // 上带外。
    }

    #[test]
    fn reader_error_propagated_from_entry_area() {
        // 镜像只 3 块：条目区第 2 块（LBA3）越界——reader 错透传不吞。
        let mut img = blank_image(3);
        put_gpt_header(&mut img, 8, GPT_ENTRY_SIZE as u32); // 8 条跨 2 块。
        put_gpt_entry(&mut img, 0, [1u8; 16], 40, 80, "a");
        assert_eq!(parse(reader(&img), BS), Err(Error::Reader));
    }

    #[test]
    fn block_size_4096_gpt_parsed() {
        // 4KiB 块：GPT 头 LBA1、条目区 LBA2（同块容 32 条）。
        let bs: u64 = 4096;
        let mut img = vec![0u8; 3 * bs as usize];
        stamp_mbr(&mut img);
        put_mbr_entry(&mut img, 0, 0xEE, 1, 3);
        img[bs as usize..bs as usize + 8].copy_from_slice(b"EFI PART");
        img[bs as usize + 24..bs as usize + 32].copy_from_slice(&1u64.to_le_bytes());
        img[bs as usize + 72..bs as usize + 80].copy_from_slice(&2u64.to_le_bytes());
        img[bs as usize + 80..bs as usize + 84].copy_from_slice(&4u32.to_le_bytes());
        img[bs as usize + 84..bs as usize + 88]
            .copy_from_slice(&(GPT_ENTRY_SIZE as u32).to_le_bytes());
        let off = 2 * bs as usize;
        img[off..off + 16].copy_from_slice(&[7u8; 16]);
        img[off + 32..off + 40].copy_from_slice(&64u64.to_le_bytes());
        img[off + 40..off + 48].copy_from_slice(&191u64.to_le_bytes());
        let parts = parse(
            move |lba, buf| {
                let s = (lba * bs) as usize;
                if s + buf.len() > img.len() {
                    return Err(Error::Reader);
                }
                buf.copy_from_slice(&img[s..s + buf.len()]);
                Ok(())
            },
            bs,
        )
        .unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].first_lba, 64);
        assert_eq!(parts[0].last_lba, 191);
    }
}
