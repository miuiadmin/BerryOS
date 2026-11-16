//! elf2flat 纯转换核——ELF64 AArch64 → flat binary（G-eco 编译侧，
//! docs/G-eco编译侧设计草案.md D2）。
//!
//! 彻底 Rust 化轨道 b 层 #8（docs/彻底Rust化计划-v1.0.md §2）：Python 版
//! （tools/elf2flat.py）降格为 oracle，本件按 §5 钉板五步落位——同输入
//! 逐字节恒等是硬验收线（现役「同输入同字节」确定性纪律是杠杆）。
//!
//! 语义与 Python 版逐条对齐（拒面全谱见 tests/selftest.rs）：
//! - 解析 PT_LOAD 段，从约定基址起铺平镜像：段内取 p_filesz 字节、间隙补零；
//! - 尾部补零到 max(p_vaddr+p_memsz) 向上取整页（.bss 覆盖由文件侧兑现）；
//! - 拒面一律 Err 非 0 退出（坏魔数/非 ELF64 小端/非 AArch64/ET_DYN/
//!   基址漂移/段重叠/超 --max/无 PT_LOAD）——缺产物或坏产物 = 下游
//!   include_bytes/断言炸响，不静默。

use std::fmt;

/// 拒面错误——每变体对应 Python 版一条 `sys.exit` 分支，文案逐字对齐。
#[derive(Debug, PartialEq, Eq)]
pub enum FlatError {
    BadMagic,
    NotElf64Le,
    NotAarch64(u16),
    NotExec(u16),
    NoPtLoad,
    Overlap { v1: u64, v2: u64 },
    BaseDrift { lo: u64, base: u64 },
    OverMax { total: u64, max: u64 },
}

impl fmt::Display for FlatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlatError::BadMagic => write!(f, "elf2flat: 坏魔数"),
            FlatError::NotElf64Le => write!(f, "elf2flat: 须 ELF64 小端"),
            FlatError::NotAarch64(m) => {
                write!(f, "elf2flat: 须 AArch64 (e_machine=183)，得 {m}")
            }
            FlatError::NotExec(t) => write!(
                f,
                "elf2flat: 须 ET_EXEC(2)，得 {t}（v1 固定基址无 PIE，见草案 D1）"
            ),
            FlatError::NoPtLoad => write!(f, "elf2flat: 无 PT_LOAD"),
            FlatError::Overlap { v1, v2 } => write!(
                f,
                "elf2flat: PT_LOAD 重叠 @{v1:#x}..{v2:#x}（链接器异常）"
            ),
            FlatError::BaseDrift { lo, base } => write!(
                f,
                "elf2flat: 链接基址 {lo:#x} ≠ 约定 {base:#x}（link-user.ld 漂移？）"
            ),
            FlatError::OverMax { total, max } => write!(
                f,
                "elf2flat: 镜像 {total}B 超 --max {max}（单页墙——多页放宽随 G-eco-b/D3）"
            ),
        }
    }
}

impl std::error::Error for FlatError {}

/// 转换结果（与 Python 版 `build_flat` 返回五元组对齐）。
#[derive(Debug, PartialEq, Eq)]
pub struct FlatOut {
    pub img: Vec<u8>,
    pub base: u64,
    pub file_len: u64,
    pub mem_len: u64,
    pub total: u64,
}

const EM_AARCH64: u16 = 183;
const PT_LOAD: u32 = 1;

/// 纯转换核：ELF 字节 → 平像。拒面一律 Err（CLI 层转非 0 退出码）。
pub fn build_flat(data: &[u8], base: u64, page: u64, max_: u64) -> Result<FlatOut, FlatError> {
    if data.len() < 4 || &data[..4] != b"\x7fELF" {
        return Err(FlatError::BadMagic);
    }
    if data.len() < 6 || data[4] != 2 || data[5] != 1 {
        return Err(FlatError::NotElf64Le);
    }
    if data.len() < 64 {
        // ehdr 不足——与 Python 短输入同路：struct.unpack 越界即异常退出。
        return Err(FlatError::NotElf64Le);
    }
    let e_type = u16::from_le_bytes([data[16], data[17]]);
    let e_machine = u16::from_le_bytes([data[18], data[19]]);
    if e_machine != EM_AARCH64 {
        return Err(FlatError::NotAarch64(e_machine));
    }
    if e_type != 2 {
        return Err(FlatError::NotExec(e_type));
    }
    let rd_u16 = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
    let rd_u32 = |o: usize| {
        u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]])
    };
    let rd_u64 = |o: usize| {
        let mut b = [0u8; 8];
        b.copy_from_slice(&data[o..o + 8]);
        u64::from_le_bytes(b)
    };
    let e_phoff = rd_u64(32);
    let e_phentsize = rd_u16(54) as usize;
    let e_phnum = rd_u16(56) as usize;

    // (p_vaddr, p_filesz, p_memsz, p_offset)
    let mut segs: Vec<(u64, u64, u64, u64)> = Vec::new();
    for i in 0..e_phnum {
        let off = e_phoff as usize + i * e_phentsize;
        if off + 56 > data.len() {
            // phdr 越界——Python struct.unpack 同炸，这里显式拒。
            return Err(FlatError::NoPtLoad);
        }
        let p_type = rd_u32(off);
        let p_offset = rd_u64(off + 8);
        let p_vaddr = rd_u64(off + 16);
        let p_filesz = rd_u64(off + 32);
        let p_memsz = rd_u64(off + 40);
        if p_type == PT_LOAD && p_memsz > 0 {
            segs.push((p_vaddr, p_filesz, p_memsz, p_offset));
        }
    }
    if segs.is_empty() {
        return Err(FlatError::NoPtLoad);
    }

    segs.sort();
    for w in segs.windows(2) {
        let (v1, _f1, m1, _o1) = w[0];
        let (v2, _f2, _m2, _o2) = w[1];
        if v1 + m1 > v2 {
            return Err(FlatError::Overlap { v1, v2 });
        }
    }

    let lo = segs[0].0;
    if lo != base {
        return Err(FlatError::BaseDrift { lo, base });
    }
    let hi_file = segs.iter().map(|&(v, f, _, _)| v + f).max().unwrap();
    let hi_mem = segs.iter().map(|&(v, _, m, _)| v + m).max().unwrap();
    // 按相对长度取整（base 页对齐时与绝对取整同值，但语义以镜像长度为准）。
    let total = (hi_mem - lo + page - 1) / page * page;
    if max_ != 0 && total > max_ {
        return Err(FlatError::OverMax { total, max: max_ });
    }

    let mut img = vec![0u8; total as usize];
    for &(v, f, _m, o) in &segs {
        let dst_lo = (v - lo) as usize;
        let dst_hi = dst_lo + f as usize;
        let src_lo = o as usize;
        let src_hi = src_lo + f as usize;
        if src_hi > data.len() || dst_hi > img.len() {
            // 段文件字节越界——Python 切片静默截断，显式拒防平像静默劣化
            // （门禁自身坏 = 平像静默劣化，a-3 U4）。
            return Err(FlatError::BadMagic);
        }
        img[dst_lo..dst_hi].copy_from_slice(&data[src_lo..src_hi]);
    }
    Ok(FlatOut { img, base: lo, file_len: hi_file - lo, mem_len: hi_mem - lo, total })
}

/// 合成最小 ELF64（elf2pe synth_elf 同款纪律）：ehdr + 逐段 PT_LOAD phdr
/// + 段文件字节顺次 appended（字节模式 (k*31 + 7 + 段号) % 256——自检侧
/// 同式重导做逐字节对拍，勿手抄常量）。
pub fn synth_elf(
    segs: &[(u64, u64, u64)],
    e_type: u16,
    e_machine: u16,
    ident: &[u8; 16],
) -> Vec<u8> {
    let phoff = 64usize;
    let data_off = phoff + 56 * segs.len();
    let mut out = vec![0u8; data_off];
    out[..16].copy_from_slice(ident);
    // struct.pack_into("<HHIQQQIHHHHHH", ehdr, 16, type, machine, 1, 0,
    //                  phoff, 0, 0, 64, 56, n, 0, 0, 0)
    out[16..18].copy_from_slice(&e_type.to_le_bytes());
    out[18..20].copy_from_slice(&e_machine.to_le_bytes());
    out[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version
    out[24..32].copy_from_slice(&0u64.to_le_bytes()); // e_entry
    out[32..40].copy_from_slice(&(phoff as u64).to_le_bytes()); // e_phoff
    out[40..48].copy_from_slice(&0u64.to_le_bytes()); // e_shoff
    out[48..52].copy_from_slice(&0u32.to_le_bytes()); // e_flags
    out[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    out[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    out[56..58].copy_from_slice(&(segs.len() as u16).to_le_bytes()); // e_phnum
    let mut blob_len = 0usize;
    for (i, &(vaddr, filesz, _memsz)) in segs.iter().enumerate() {
        let off = phoff + i * 56;
        out[off..off + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        out[off + 4..off + 8].copy_from_slice(&0x6u32.to_le_bytes()); // p_flags
        out[off + 8..off + 16]
            .copy_from_slice(&((data_off + blob_len) as u64).to_le_bytes()); // p_offset
        out[off + 16..off + 24].copy_from_slice(&vaddr.to_le_bytes()); // p_vaddr
        out[off + 24..off + 32].copy_from_slice(&vaddr.to_le_bytes()); // p_paddr
        out[off + 32..off + 40].copy_from_slice(&filesz.to_le_bytes()); // p_filesz
        out[off + 40..off + 48].copy_from_slice(&_memsz.to_le_bytes()); // p_memsz
        out[off + 48..off + 56].copy_from_slice(&0x1000u64.to_le_bytes()); // p_align
        blob_len += filesz as usize;
    }
    let mut blob = Vec::with_capacity(blob_len);
    for (i, &(_, filesz, _)) in segs.iter().enumerate() {
        for k in 0..filesz {
            blob.push(((k * 31 + 7 + i as u64) % 256) as u8);
        }
    }
    out.extend_from_slice(&blob);
    out
}
