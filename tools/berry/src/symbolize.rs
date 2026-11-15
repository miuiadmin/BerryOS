//! ELF64 符号化（cbox 刀 3，`.agents/plans/cbox/design.md` 工具面）。
//!
//! 零依赖手写解析：ELF 头 → 节表 → SHT_SYMTAB（strip 件回落 SHT_DYNSYM）。
//! 最近符号语义 = `st_value <= pc` 的最大者，报 `名+偏移`。
//!
//! 符号源现实（刀 3 定谳）：
//! - **内核 ELF**（berry-ld 链但保留 symtab）——内核名直查；`user_<名>` 族
//!   内嵌 blob 任务（`kernel/src/user.rs` asm 模板，任务空间基址
//!   `USER_CODE_VA = 0x1000`）经 `__user_<名>` 符号重定基后查。
//! - **userbins release ELF 被 berry-ld 全 strip**（仅余 .shstrtab）——
//!   双表皆空按 spec 报错指路构建产物位；userbins 符号面挂构建改造
//!   （design.md 刀 3 边界），不静默造假。

use crate::error::{BerryError, Result};

/// 符号表项（只留最近符号语义所需的两字段）。
#[derive(Debug)]
pub struct Sym {
    pub name: String,
    pub value: u64,
}

/// ELF64 little-endian 定长小读取（越界即 Err，不 panic）。
struct Rd<'a> {
    d: &'a [u8],
}

impl<'a> Rd<'a> {
    fn u16(&self, o: usize) -> Result<u16> {
        let b = self.d.get(o..o + 2).ok_or_else(|| bad("节表截断"))?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&self, o: usize) -> Result<u32> {
        let b = self.d.get(o..o + 4).ok_or_else(|| bad("节表截断"))?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&self, o: usize) -> Result<u64> {
        let b = self.d.get(o..o + 8).ok_or_else(|| bad("节表截断"))?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
}

fn bad(msg: &str) -> BerryError {
    BerryError::internal(format!("symbolize: {msg}"))
}

/// 加载全部具名符号（symtab 优先，strip 件回落 dynsym）。
///
/// 双表皆空 = 报错指路构建产物位（spec 刀 3 语义——不静默返回空表）。
pub fn load_symbols(elf: &[u8]) -> Result<Vec<Sym>> {
    if elf.len() < 0x40 || &elf[..4] != b"\x7fELF" {
        return Err(bad("不是 ELF 件"));
    }
    if elf[4] != 2 || elf[5] != 1 {
        return Err(bad("仅支持 ELF64 little-endian"));
    }
    let rd = Rd { d: elf };
    let shoff = rd.u64(0x28)? as usize;
    if shoff == 0 {
        return Err(bad("无节表（无法定位 symtab）"));
    }
    let shentsize = rd.u16(0x3a)? as usize;
    let shnum = rd.u16(0x3c)? as usize;
    // 节头：name u32 / type u32 / addr u64 / off u64 / size u64 / link u32。
    let sh = |i: usize| -> Result<(u32, usize, usize, usize, u32)> {
        let o = shoff + i * shentsize;
        Ok((rd.u32(o)?, rd.u32(o + 4)? as usize, rd.u64(o + 0x18)? as usize,
            rd.u64(o + 0x20)? as usize, rd.u32(o + 0x28)?))
    };
    for want in [2usize, 11usize] {
        // SHT_SYMTAB → SHT_DYNSYM 回落序
        for i in 0..shnum {
            let (_name, typ, off, size, link) = sh(i)?;
            if typ != want || size == 0 {
                continue;
            }
            let (_, _, str_off, str_size, _) = sh(link as usize)?;
            if let Some(syms) = parse_symtab(elf, off, size, str_off, str_size) {
                if !syms.is_empty() {
                    return Ok(syms);
                }
            }
        }
    }
    Err(BerryError {
        code: 9,
        message: "symbolize: symtab/dynsym 双空（strip 件）".into(),
        hint: Some(
            "带符号产物位：内核 = cargo-target/aarch64-unknown-none/debug/berry-kernel\
             （berry-ld 保留 symtab）；userbins release 件全 strip，符号面挂构建改造\
             （.agents/plans/cbox/design.md 刀 3 边界）"
                .into(),
        ),
    })
}

/// 单表解析：24B Elf64_Sym 流 + 关联 strtab；无效表返回 None（试下一候选）。
fn parse_symtab(
    elf: &[u8],
    off: usize,
    size: usize,
    str_off: usize,
    str_size: usize,
) -> Option<Vec<Sym>> {
    let strtab = elf.get(str_off..str_off.checked_add(str_size)?)?;
    let mut syms = Vec::new();
    for pos in (off..off.checked_add(size)?).step_by(24) {
        let e = elf.get(pos..pos + 24)?;
        let name_off = u32::from_le_bytes([e[0], e[1], e[2], e[3]]) as usize;
        let value = u64::from_le_bytes([
            e[8], e[9], e[10], e[11], e[12], e[13], e[14], e[15],
        ]);
        if value == 0 || name_off == 0 {
            continue;
        }
        let rest = strtab.get(name_off..)?;
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        let name = String::from_utf8_lossy(&rest[..end]).into_owned();
        if name.is_empty() {
            continue;
        }
        syms.push(Sym { name, value });
    }
    syms.sort_by_key(|s| s.value);
    Some(syms)
}

/// 最近符号：`st_value <= pc` 的最大者（并列取首个——排序稳定无妨）。
/// 命中返回 `(符号, pc - st_value)`。
pub fn nearest<'a>(syms: &'a [Sym], pc: u64) -> Option<(&'a Sym, u64)> {
    let idx = syms.partition_point(|s| s.value <= pc);
    if idx == 0 {
        return None;
    }
    let s = &syms[idx - 1];
    Some((s, pc - s.value))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成最小 ELF64：节表 = [空, .symtab, .strtab]（真实布局字段序）。
    fn build_elf(syms: &[(&str, u64)]) -> Vec<u8> {
        let mut strtab = vec![0u8];
        let mut name_off = Vec::new();
        for (n, _) in syms {
            name_off.push(strtab.len());
            strtab.extend_from_slice(n.as_bytes());
            strtab.push(0);
        }
        let mut symtab = Vec::new();
        for (i, (_, v)) in syms.iter().enumerate() {
            symtab.extend_from_slice(&(name_off[i] as u32).to_le_bytes());
            symtab.push(0x12); // info：STB_GLOBAL|STT_FUNC
            symtab.push(0); // other
            symtab.extend_from_slice(&0u16.to_le_bytes()); // shndx
            symtab.extend_from_slice(&v.to_le_bytes());
            symtab.extend_from_slice(&0x10u64.to_le_bytes()); // size
        }
        let ehsz = 0x40;
        let sym_off = ehsz;
        let sym_sz = symtab.len();
        let str_off = sym_off + sym_sz;
        let str_sz = strtab.len();
        let shoff = (str_off + str_sz + 15) & !15;
        let shentsize = 64;
        let shnum = 3;
        let mut d = vec![0u8; shoff + shnum * shentsize];
        d[..4].copy_from_slice(b"\x7fELF");
        d[4] = 2; // ELF64
        d[5] = 1; // LE
        d[6] = 1; // EV_CURRENT
        d[0x28..0x30].copy_from_slice(&(shoff as u64).to_le_bytes());
        d[0x3a..0x3c].copy_from_slice(&(shentsize as u16).to_le_bytes());
        d[0x3c..0x3e].copy_from_slice(&(shnum as u16).to_le_bytes());
        d[0x3e..0x40].copy_from_slice(&0u16.to_le_bytes()); // shstrndx=0
        d[sym_off..str_off].copy_from_slice(&symtab);
        d[str_off..str_off + str_sz].copy_from_slice(&strtab);
        let mut sh = |i: usize, typ: u32, off: usize, size: usize, link: u32| {
            let o = shoff + i * shentsize;
            d[o + 4..o + 8].copy_from_slice(&typ.to_le_bytes());
            d[o + 0x18..o + 0x20].copy_from_slice(&(off as u64).to_le_bytes());
            d[o + 0x20..o + 0x28].copy_from_slice(&(size as u64).to_le_bytes());
            d[o + 0x28..o + 0x2c].copy_from_slice(&link.to_le_bytes());
        };
        sh(1, 2, sym_off, sym_sz, 2); // SHT_SYMTAB → strtab=节2
        sh(2, 3, str_off, str_sz, 0); // SHT_STRTAB
        d
    }

    #[test]
    fn nearest_exact_inside_and_miss() {
        let elf = build_elf(&[("aaa", 0x1000), ("mmm", 0x2000), ("zzz", 0x3000)]);
        let syms = load_symbols(&elf).unwrap();
        let (s, off) = nearest(&syms, 0x2000).unwrap();
        assert_eq!((s.name.as_str(), off), ("mmm", 0));
        let (s, off) = nearest(&syms, 0x2abc).unwrap();
        assert_eq!((s.name.as_str(), off), ("mmm", 0xabc));
        assert!(nearest(&syms, 0xfff).is_none()); // 首符号之前
    }

    #[test]
    fn stripped_elf_errors_with_hint() {
        let elf = build_elf(&[]); // symtab 空 → 双空路径
        let err = load_symbols(&elf).unwrap_err();
        assert!(err.hint.as_deref().unwrap_or("").contains("berry-kernel"));
    }

    #[test]
    fn not_an_elf_rejected() {
        assert!(load_symbols(b"MZ......").is_err());
    }

    /// 真产物对拍（opt-in）：环境变量 `BERRY_SYMBOLIZE_TEST_ELF` 指向任一
    /// 带 symtab 的 ELF（如内核 debug 件）时跑，缺省跳过——CI 无构建产物
    /// 不许红。
    #[test]
    fn real_artifact_when_present() {
        let Ok(p) = std::env::var("BERRY_SYMBOLIZE_TEST_ELF") else {
            return;
        };
        let d = std::fs::read(&p).unwrap();
        let syms = load_symbols(&d).unwrap();
        assert!(!syms.is_empty());
        assert!(syms.iter().any(|s| s.name.contains("kernel")));
    }
}
