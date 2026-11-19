//! elf2pe 纯转换核——静态 ELF（AArch64）→ PE32+ EFI 应用（V-0a 刀 1，
//! 配对消费端 = QEMU AAVMF 固件 DXE 加载器）。
//!
//! 彻底 Rust 化轨道 b 层 #8（docs/彻底Rust化计划-v1.0.md §2 #9）：Python
//! 版已降格为 oracle（tools/elf2pe_oracle.py，对拍取证专用），本件按 §5 钉板五步落位首刀（lib 纯面 +
//! CLI + 测试接线）；二刀产链换轨（build-boot 的 python3 调用 → 本 bin +
//! Python 降格 oracle）另件。
//!
//! 布局不变量（与 Python 版逐条对齐，链接脚本执法）：
//! - 单一 alloc 节 .img @ 0x1000（RWE 合一）；
//! - ImageBase=0 ⇒ RVA == ELF vaddr——PC 相对立即数原样有效；
//! - 基址重定位只收 R_AARCH64_ABS64（257）→ PE DIR64（type 10）；PC 相对/
//!   页相对族在均匀基址平移下不变——忽略；白名单外类型一律 loud Err
//!   （静默错比崩更糟）。
//! - .reloc 槽位 = r_offset（最终镜像绝对 VMA）——V-0b 刀 2 判例（雷册
//!   §A90）：sym/addend 描述槽内容、不是槽位；旧 bug 形态（sym 值当槽位）
//!   的回归钉板见 tests/selftest.rs。
//!
//! 确定性：TimeDateStamp=0、无 debug 目录、无 PDB 名——产物字节只随内容变。

use std::fmt;

// —— PE 常量（与 Python 版逐字对齐）。
pub const DOS_E_LFANEW_OFF: usize = 0x3C;
pub const PE_SIG: [u8; 4] = *b"PE\0\0";
pub const MACHINE_ARM64: u16 = 0xAA64;
pub const SUBSYSTEM_EFI_APPLICATION: u16 = 10;
pub const OPT_MAGIC_PE32PLUS: u16 = 0x20B;
pub const NUM_DATA_DIRS: u32 = 16;
pub const SIZEOF_OPT_HEADER: usize = 240; // 112 字段面 + 16×8 数据目录
pub const SECTION_ALIGNMENT: u64 = 0x1000;
pub const FILE_ALIGNMENT: u64 = 0x200;
pub const SCN_CODE_EXEC_READ_WRITE: u32 = 0xE0000020; // CNT_CODE|EXECUTE|READ|WRITE
pub const SCN_DATA_DISCARDABLE_READ: u32 = 0x42000040; // CNT_INITIALIZED_DATA|DISCARDABLE|READ
pub const IMAGE_FILE_EXECUTABLE_IMAGE: u16 = 0x0002;
pub const RELOC_DIR64: u16 = 10;
pub const RELOC_ABSOLUTE: u16 = 0; // no-op 填充型（空 .reloc 兜底块用）
pub const IDX_BASE_RELOC: usize = 5;

// —— ELF 常量（只吃 ELF64 / LE / AArch64）。
pub const EM_AARCH64: u16 = 183;
pub const SHT_PROGBITS: u32 = 1;
pub const SHT_SYMTAB: u32 = 2;
pub const SHT_STRTAB: u32 = 3;
pub const SHT_RELA: u32 = 4;
pub const SHT_NOBITS: u32 = 8;
pub const SHF_ALLOC: u64 = 0x2;
pub const R_AARCH64_ABS64: u32 = 257;

/// 均匀基址平移下不变的类型（PC 相对/页相对/低 12 位）——可忽略。
pub const R_PCREL_INVARIANT: [u32; 16] = [
    260, 261, 262, // PREL64/32/16
    274, 275, 276, // ADR_PREL_LO21 / PG_HI21 / PG_HI21_NC
    277, // ADD_ABS_LO12_NC
    278, // LDST8_ABS_LO12_NC
    279, 280, // TSTBR14 / CONDBR19
    282, 283, // JUMP26 / CALL26
    284, 285, 286, // LDST16/32/64_ABS_LO12_NC
    288, // LDST128_ABS_LO12_NC
];

/// 拒面错误——每变体对应 Python 版一条 `fail()` 分支（文案面对齐）。
#[derive(Debug, PartialEq, Eq)]
pub enum PeError {
    NotElf,
    NotElf64Le,
    NotAarch64(u16),
    NoSectionHeaders,
    MissingShstrtab,
    /// 白名单外重定位型：(rtype, r_offset, target 节名)。
    UnknownReloc { rtype: u32, r_offset: u64, sec: String },
    MissingImg,
    ImgNotAlloc,
    MultipleAlloc { name: String },
    ImgVaddrNotAligned(u64),
    EntryOutsideImg { entry: u64 },
    /// 节头/段数据越界（Python struct.unpack 越界炸——语义同为 loud 拒）。
    Truncated,
}

impl fmt::Display for PeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeError::NotElf => write!(f, "elf2pe: 非 ELF（魔数不匹配）"),
            PeError::NotElf64Le => write!(f, "elf2pe: 只吃 ELF64 小端"),
            PeError::NotAarch64(m) => {
                write!(f, "elf2pe: machine=0x{m:X} 非 AArch64（183）")
            }
            PeError::NoSectionHeaders => {
                write!(f, "elf2pe: 无节头表——--emit-relocs 的输入必有节头")
            }
            PeError::MissingShstrtab => write!(f, "elf2pe: 缺 .shstrtab"),
            PeError::UnknownReloc { rtype, r_offset, sec } => write!(
                f,
                "elf2pe: 未归类重定位型 {rtype}@0x{r_offset:X}（节 {sec}）——白名单外\
                 （本仓纪律：静默错比崩更糟，先扩白名单论证再放行）"
            ),
            PeError::MissingImg => write!(
                f,
                "elf2pe: 缺 .img 节（链接脚本必须把全部内容并进单一 .img @0x1000）"
            ),
            PeError::ImgNotAlloc => write!(f, "elf2pe: .img 节非 SHF_ALLOC"),
            PeError::MultipleAlloc { name } => {
                write!(f, "elf2pe: 多 alloc 节 {name}——单 .img 不变量破坏")
            }
            PeError::ImgVaddrNotAligned(a) => write!(
                f,
                "elf2pe: .img vaddr=0x{a:X} ≠ 0x1000（RVA==vaddr 前提破坏）"
            ),
            PeError::EntryOutsideImg { entry } => {
                write!(f, "elf2pe: entry=0x{entry:X} 落 .img 外")
            }
            PeError::Truncated => write!(f, "elf2pe: 输入截断（节头/段数据越界）"),
        }
    }
}

impl std::error::Error for PeError {}

// ---------------------------------------------------------------- ELF 解析。

#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    pub sh_type: u32,
    pub flags: u64,
    pub addr: u64,
    pub offset: u64,
    pub size: u64,
    pub link: u32,
    pub info: u32,
    pub entsize: u64,
}

#[derive(Debug)]
pub struct Elf {
    pub e_entry: u64,
    pub sections: Vec<Section>,
    data: Vec<u8>,
}

impl Elf {
    pub fn parse(data: &[u8]) -> Result<Elf, PeError> {
        if data.len() < 64 || data[..4] != [0x7f, b'E', b'L', b'F'] {
            return Err(PeError::NotElf);
        }
        if data[4] != 2 || data[5] != 1 {
            return Err(PeError::NotElf64Le);
        }
        let rd_u16 = |o: usize| -> Result<u16, PeError> {
            if o + 2 > data.len() {
                return Err(PeError::Truncated);
            }
            Ok(u16::from_le_bytes([data[o], data[o + 1]]))
        };
        let rd_u32 = |o: usize| -> Result<u32, PeError> {
            if o + 4 > data.len() {
                return Err(PeError::Truncated);
            }
            Ok(u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]))
        };
        let rd_u64 = |o: usize| -> Result<u64, PeError> {
            if o + 8 > data.len() {
                return Err(PeError::Truncated);
            }
            let mut b = [0u8; 8];
            b.copy_from_slice(&data[o..o + 8]);
            Ok(u64::from_le_bytes(b))
        };
        let e_machine = rd_u16(18)?;
        if e_machine != EM_AARCH64 {
            return Err(PeError::NotAarch64(e_machine));
        }
        let e_entry = rd_u64(24)?;
        let e_shoff = rd_u64(40)?;
        let e_shentsize = rd_u16(58)? as usize;
        let e_shnum = rd_u16(60)? as usize;
        let e_shstrndx = rd_u16(62)? as usize;
        if e_shoff == 0 {
            return Err(PeError::NoSectionHeaders);
        }
        let mut raw: Vec<(u32, u32, u64, u64, u64, u64, u32, u32, u64)> = Vec::new();
        for i in 0..e_shnum {
            let off = e_shoff as usize + i * e_shentsize;
            raw.push((
                rd_u32(off)?,      // sh_name
                rd_u32(off + 4)?,  // sh_type
                rd_u64(off + 8)?,  // sh_flags
                rd_u64(off + 16)?, // sh_addr
                rd_u64(off + 24)?, // sh_offset
                rd_u64(off + 32)?, // sh_size
                rd_u32(off + 40)?, // sh_link
                rd_u32(off + 44)?, // sh_info
                rd_u64(off + 56)?, // sh_entsize
            ));
        }
        // 名字补丁：从 .shstrtab 逐节解 NUL 结尾 ASCII。
        if e_shstrndx == 0 || e_shstrndx >= raw.len() {
            return Err(PeError::MissingShstrtab);
        }
        let st = raw[e_shstrndx];
        let (st_off, st_sz) = (st.4 as usize, st.5 as usize);
        if st_off + st_sz > data.len() {
            return Err(PeError::Truncated);
        }
        let strtab = &data[st_off..st_off + st_sz];
        let name_of = |no: u32| -> String {
            let no = no as usize;
            if no >= strtab.len() {
                return String::new();
            }
            let end = strtab[no..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| no + p)
                .unwrap_or(strtab.len());
            String::from_utf8_lossy(&strtab[no..end]).into_owned()
        };
        let sections = raw
            .into_iter()
            .map(|(name_off, sh_type, flags, addr, offset, size, link, info, entsize)| Section {
                name: name_of(name_off),
                sh_type,
                flags,
                addr,
                offset,
                size,
                link,
                info,
                entsize,
            })
            .collect();
        Ok(Elf { e_entry, sections, data: data.to_vec() })
    }

    pub fn section_by_name(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    pub fn contents(&self, s: &Section) -> Result<Vec<u8>, PeError> {
        let lo = s.offset as usize;
        let hi = lo + s.size as usize;
        if hi > self.data.len() {
            // Python 切片静默截断——显式拒防坏产物（a-3 U4）。
            return Err(PeError::Truncated);
        }
        Ok(self.data[lo..hi].to_vec())
    }
}

/// ABS64 槽位列表（已排序）。槽位 = r_offset（V-0b 判例：sym/addend 描述槽
/// **内容**——链接期已解析，与本转换无关；旧版误用 sym 值当槽位 ⇒ DIR64
/// 全错打 RVA 0x1000，固件装载 +delta 砸穿首函数头，雷册 §A90）。
pub fn collect_relocs(elf: &Elf) -> Result<Vec<u64>, PeError> {
    let mut out = Vec::new();
    for s in &elf.sections {
        if s.sh_type != SHT_RELA {
            continue;
        }
        let target = elf
            .sections
            .get(s.info as usize)
            .ok_or(PeError::Truncated)?;
        if target.flags & SHF_ALLOC == 0 {
            continue; // （.eh_frame 等 DISCARD 目标的 RELA——target 非 alloc）
        }
        let blob = elf.contents(s)?;
        if blob.len() % 24 != 0 {
            return Err(PeError::Truncated);
        }
        for ent in blob.chunks_exact(24) {
            let mut b8 = [0u8; 8];
            b8.copy_from_slice(&ent[..8]);
            let r_offset = u64::from_le_bytes(b8);
            b8.copy_from_slice(&ent[8..16]);
            let r_info = u64::from_le_bytes(b8);
            let rtype = (r_info & 0xFFFF_FFFF) as u32;
            if rtype == 0 {
                continue;
            }
            if rtype == R_AARCH64_ABS64 {
                out.push(r_offset);
            } else if R_PCREL_INVARIANT.contains(&rtype) {
                continue;
            } else {
                return Err(PeError::UnknownReloc {
                    rtype,
                    r_offset,
                    sec: target.name.clone(),
                });
            }
        }
    }
    out.sort();
    Ok(out)
}

// ---------------------------------------------------------------- PE 生成。

fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) / a * a
}

/// DIR64 重定位块（按 4KiB 页分组，块尺寸含头恒 4B 对齐）。空表产一个
/// 合法空块（仅块头 8B）——.reloc 目录恒有可走查的落点。
pub fn build_reloc_blocks(vaddrs: &[u64]) -> Vec<u8> {
    let mut pages: std::collections::BTreeMap<u64, Vec<u64>> = Default::default();
    for &v in vaddrs {
        pages.entry(v >> 12).or_default().push(v);
    }
    if pages.is_empty() {
        pages.insert(0, Vec::new());
    }
    let mut out = Vec::new();
    for (page, vs) in &pages {
        let mut ents: Vec<u16> = vs
            .iter()
            .map(|&v| RELOC_DIR64 << 12 | (v & 0xFFF) as u16)
            .collect();
        while (8 + 2 * ents.len()) % 4 != 0 {
            ents.push(RELOC_ABSOLUTE << 12); // 块内 4B 对齐填充（no-op 型）
        }
        out.extend_from_slice(&((page << 12) as u32).to_le_bytes()); // PageRVA（页对齐）
        out.extend_from_slice(&((8 + 2 * ents.len()) as u32).to_le_bytes()); //BlockSize
        for e in ents {
            out.extend_from_slice(&e.to_le_bytes());
        }
    }
    out
}

/// 单 alloc 节形状执法：恰一个 SHF_ALLOC 的 PROGBITS/NOBITS 同体节 .img
/// （berryboot.ld 钉的形状）——多 alloc 节 = 链接脚本漂移，loud Err。
pub fn elf_section_payload(elf: &Elf) -> Result<(Section, Vec<u8>), PeError> {
    let img = elf.section_by_name(".img").ok_or(PeError::MissingImg)?;
    if img.flags & SHF_ALLOC == 0 {
        return Err(PeError::ImgNotAlloc);
    }
    for s in &elf.sections {
        if std::ptr::eq(s, img) || s.sh_type == 0 || s.sh_type == SHT_RELA {
            continue;
        }
        if s.flags & SHF_ALLOC != 0 {
            return Err(PeError::MultipleAlloc { name: s.name.clone() });
        }
    }
    let payload = elf.contents(img)?;
    Ok((img.clone(), payload))
}

/// 转换结果——Python 版 main 的 stdout 摘要字段（verify 行）对齐面。
#[derive(Debug, PartialEq, Eq)]
pub struct PeOut {
    pub pe: Vec<u8>,
    pub entry: u64,
    pub img_vsize: u64,
    pub reloc_count: usize,
    pub n_blocks: usize,
}

pub fn convert(data: &[u8]) -> Result<PeOut, PeError> {
    let elf = Elf::parse(data)?;
    let (img, payload) = elf_section_payload(&elf)?;
    if img.addr != SECTION_ALIGNMENT {
        return Err(PeError::ImgVaddrNotAligned(img.addr));
    }
    if !(img.addr <= elf.e_entry && elf.e_entry < img.addr + img.size) {
        return Err(PeError::EntryOutsideImg { entry: elf.e_entry });
    }
    let relocs = collect_relocs(&elf)?;
    let reloc_data = build_reloc_blocks(&relocs);

    let img_raw_size = align_up(payload.len() as u64, FILE_ALIGNMENT);
    let img_va = img.addr;
    let reloc_va = align_up(img_va + std::cmp::max(img.size, img_raw_size), SECTION_ALIGNMENT);
    let reloc_raw = align_up(reloc_data.len() as u64, FILE_ALIGNMENT);
    // 布局不变量：.img/.reloc 的文件偏移 == 各自 RVA（文件 = 头页 + 内存
    // 镜像）——verify 的 RVA 走查即文件走查，无 RVA↔offset 换算面。
    assert_eq!(SECTION_ALIGNMENT, img_va);
    let reloc_ptr = reloc_va;
    let size_of_image = align_up(reloc_va + reloc_data.len() as u64, SECTION_ALIGNMENT);

    // —— 头部页（0x0..0x1000）：DOS + PE + COFF + Optional + 节表，余零。
    let mut hdr = vec![0u8; SECTION_ALIGNMENT as usize];
    let put_u16 = |h: &mut Vec<u8>, o: usize, v: u16| {
        h[o..o + 2].copy_from_slice(&v.to_le_bytes())
    };
    let put_u32 = |h: &mut Vec<u8>, o: usize, v: u32| {
        h[o..o + 4].copy_from_slice(&v.to_le_bytes())
    };
    let put_u64 = |h: &mut Vec<u8>, o: usize, v: u64| {
        h[o..o + 8].copy_from_slice(&v.to_le_bytes())
    };
    put_u16(&mut hdr, 0, 0x5A4D); // e_magic 'MZ'
    put_u32(&mut hdr, DOS_E_LFANEW_OFF, 0x80); // e_lfanew
    let pe_off = 0x80usize;
    hdr[pe_off..pe_off + 4].copy_from_slice(&PE_SIG);
    let coff_off = pe_off + 4;
    put_u16(&mut hdr, coff_off, MACHINE_ARM64);
    put_u16(&mut hdr, coff_off + 2, 2); // NumberOfSections（.img/.reloc）
    put_u32(&mut hdr, coff_off + 4, 0); // TimeDateStamp（确定性：恒 0）
    put_u32(&mut hdr, coff_off + 8, 0); // 符号表偏移（无）
    put_u32(&mut hdr, coff_off + 12, 0); // 符号数
    put_u16(&mut hdr, coff_off + 16, SIZEOF_OPT_HEADER as u16);
    put_u16(&mut hdr, coff_off + 18, IMAGE_FILE_EXECUTABLE_IMAGE);
    let opt_off = coff_off + 20;
    let mut o = vec![0u8; SIZEOF_OPT_HEADER];
    put_u16(&mut o, 0, OPT_MAGIC_PE32PLUS);
    // LinkerVersion 0.0
    put_u32(&mut o, 4, img_raw_size as u32); // SizeOfCode
    put_u32(&mut o, 8, reloc_raw as u32); // SizeOfInitializedData
    put_u32(&mut o, 12, 0); // SizeOfUninitializedData
    put_u32(&mut o, 16, elf.e_entry as u32); // AddressOfEntryPoint
    put_u32(&mut o, 20, img_va as u32); // BaseOfCode
    put_u64(&mut o, 24, 0); // ImageBase=0 ⇒ RVA==vaddr
    put_u32(&mut o, 32, SECTION_ALIGNMENT as u32);
    put_u32(&mut o, 36, FILE_ALIGNMENT as u32);
    // OS/Image/Subsystem 版本：Subsys ≥1.0（其余 0）
    put_u32(&mut o, 48, 1); // SubsystemVersion（u32 写 48-51 = major@48=1, minor@50=0）
    put_u32(&mut o, 52, 0); // Win32VersionValue
    put_u32(&mut o, 56, size_of_image as u32);
    put_u32(&mut o, 60, SECTION_ALIGNMENT as u32); // SizeOfHeaders
    put_u32(&mut o, 64, 0); // CheckSum
    put_u16(&mut o, 68, SUBSYSTEM_EFI_APPLICATION); // Subsystem@68（u16）
    put_u16(&mut o, 70, 0); // DllCharacteristics
    // Stack/Heap（DXE 不用）+ NumberOfRvaAndSizes
    put_u64(&mut o, 72, 0);
    put_u64(&mut o, 80, 0);
    put_u64(&mut o, 88, 0);
    put_u64(&mut o, 96, 0);
    put_u32(&mut o, 108, NUM_DATA_DIRS);
    put_u32(&mut o, 112 + 8 * IDX_BASE_RELOC, reloc_va as u32);
    put_u32(&mut o, 112 + 8 * IDX_BASE_RELOC + 4, reloc_data.len() as u32);
    hdr[opt_off..opt_off + SIZEOF_OPT_HEADER].copy_from_slice(&o);

    let sect = |name: &[u8], vsize: u64, va: u64, rawsz: u64, ptr: u64, chars: u32| -> Vec<u8> {
        let mut e = vec![0u8; 40];
        e[..8].copy_from_slice(name);
        e[8..12].copy_from_slice(&(vsize as u32).to_le_bytes());
        e[12..16].copy_from_slice(&(va as u32).to_le_bytes());
        e[16..20].copy_from_slice(&(rawsz as u32).to_le_bytes());
        e[20..24].copy_from_slice(&(ptr as u32).to_le_bytes());
        e[24..28].copy_from_slice(&0u32.to_le_bytes()); // 重定位/行号偏移
        e[28..32].copy_from_slice(&0u32.to_le_bytes());
        e[32..34].copy_from_slice(&0u16.to_le_bytes()); // 重定位/行号数
        e[34..36].copy_from_slice(&0u16.to_le_bytes());
        e[36..40].copy_from_slice(&chars.to_le_bytes());
        e
    };
    let sh_off = opt_off + SIZEOF_OPT_HEADER;
    let e1 = sect(b".img\0\0\0\0", payload.len() as u64, img_va,
                  img_raw_size, SECTION_ALIGNMENT, SCN_CODE_EXEC_READ_WRITE);
    hdr[sh_off..sh_off + 40].copy_from_slice(&e1);
    let e2 = sect(b".reloc\0\0", reloc_data.len() as u64, reloc_va,
                  reloc_raw, reloc_ptr, SCN_DATA_DISCARDABLE_READ);
    hdr[sh_off + 40..sh_off + 80].copy_from_slice(&e2);

    let mut out = hdr;
    out.extend_from_slice(&payload);
    out.resize(out.len() + (img_raw_size - payload.len() as u64) as usize, 0);
    out.resize(reloc_ptr as usize, 0);
    out.extend_from_slice(&reloc_data);
    out.resize(out.len() + (reloc_raw - reloc_data.len() as u64) as usize, 0);
    assert_eq!(out.len() as u64, reloc_ptr + reloc_raw, "文件长度不变量破坏");

    let n_blocks = {
        let mut n = 0usize;
        let mut p = reloc_va as usize;
        let end = p + reloc_data.len();
        while p < end {
            let bsz = u32::from_le_bytes([out[p + 4], out[p + 5], out[p + 6], out[p + 7]]) as usize;
            n += 1;
            p += bsz;
        }
        n
    };
    Ok(PeOut { pe: out, entry: elf.e_entry, img_vsize: payload.len() as u64, reloc_count: relocs.len(), n_blocks })
}

// ---------------------------------------------------------------- 回读断言。

/// 转换后恒回读自断言（Python verify 对齐）——结构性断言（machine /
/// subsystem / entry 落 .img / .reloc 闭合）非 0 退出；返回机器可读摘要行。
pub fn verify(pe: &[u8], reloc_count_in: usize) -> Result<String, PeError> {
    if pe.len() < 2 || pe[..2] != [0x4D, 0x5A] {
        return Err(PeError::NotElf); // 无 MZ——回读拒（文案面复用拒变体）
    }
    let pe_off = u32::from_le_bytes([pe[0x3C], pe[0x3D], pe[0x3E], pe[0x3F]]) as usize;
    if pe[pe_off..pe_off + 4] != PE_SIG {
        return Err(PeError::NotElf);
    }
    let machine = u16::from_le_bytes([pe[pe_off + 4], pe[pe_off + 5]]);
    let nsec = u16::from_le_bytes([pe[pe_off + 6], pe[pe_off + 7]]);
    if machine != MACHINE_ARM64 {
        return Err(PeError::NotAarch64(machine));
    }
    let opt = pe_off + 24;
    let magic = u16::from_le_bytes([pe[opt], pe[opt + 1]]);
    if magic != OPT_MAGIC_PE32PLUS {
        return Err(PeError::NotElf64Le); // 非 PE32+——回读拒
    }
    let entry = u32::from_le_bytes([pe[opt + 16], pe[opt + 17], pe[opt + 18], pe[opt + 19]]);
    let subsys = u16::from_le_bytes([pe[opt + 68], pe[opt + 69]]);
    if subsys != SUBSYSTEM_EFI_APPLICATION {
        return Err(PeError::NotAarch64(subsys)); // subsystem≠10——回读拒
    }
    let soh = u32::from_le_bytes([pe[opt + 60], pe[opt + 61], pe[opt + 62], pe[opt + 63]]);
    if soh != SECTION_ALIGNMENT as u32 {
        return Err(PeError::ImgVaddrNotAligned(soh as u64)); // SizeOfHeaders 拒
    }
    let rva = u32::from_le_bytes([
        pe[opt + 112 + 8 * IDX_BASE_RELOC],
        pe[opt + 113 + 8 * IDX_BASE_RELOC],
        pe[opt + 114 + 8 * IDX_BASE_RELOC],
        pe[opt + 115 + 8 * IDX_BASE_RELOC],
    ]) as usize;
    let rsz = u32::from_le_bytes([
        pe[opt + 116 + 8 * IDX_BASE_RELOC],
        pe[opt + 117 + 8 * IDX_BASE_RELOC],
        pe[opt + 118 + 8 * IDX_BASE_RELOC],
        pe[opt + 119 + 8 * IDX_BASE_RELOC],
    ]) as usize;
    let shs = u16::from_le_bytes([pe[pe_off + 20], pe[pe_off + 21]]) as usize;
    let sh = opt + shs;
    let name_of = |i: usize| -> Vec<u8> {
        let b = &pe[sh + 40 * i..sh + 40 * i + 8];
        b.iter().take_while(|&&c| c != 0).cloned().collect()
    };
    let names: Vec<Vec<u8>> = (0..nsec as usize).map(name_of).collect();
    if names.first() != Some(&b".img".to_vec()) || !names.contains(&b".reloc".to_vec()) {
        return Err(PeError::MissingImg); // 节名面拒（文案复用）
    }
    let img_va = u32::from_le_bytes([pe[sh + 12], pe[sh + 13], pe[sh + 14], pe[sh + 15]]) as u64;
    let img_vs = u32::from_le_bytes([pe[sh + 8], pe[sh + 9], pe[sh + 10], pe[sh + 11]]) as u64;
    if !(img_va <= entry as u64 && (entry as u64) < img_va + img_vs) {
        return Err(PeError::EntryOutsideImg { entry: entry as u64 });
    }
    if rsz == 0 {
        return Err(PeError::NoSectionHeaders); // .reloc 目录空——回读拒
    }
    let mut n_blocks = 0usize;
    let mut p = rva;
    while p < rva + rsz {
        if p + 8 > pe.len() {
            return Err(PeError::Truncated);
        }
        let bsz = u32::from_le_bytes([pe[p + 4], pe[p + 5], pe[p + 6], pe[p + 7]]) as usize;
        if bsz == 0 {
            return Err(PeError::Truncated); // 零尺寸块 = 死循环守卫
        }
        n_blocks += 1;
        p += bsz;
    }
    if p != rva + rsz {
        return Err(PeError::Truncated); // 重定位块尺寸不闭合
    }
    Ok(format!(
        "elf2pe: ok entry=0x{entry:X} img=0x{img_vs:X}B relocs={reloc_count_in} blocks={n_blocks}"
    ))
}
