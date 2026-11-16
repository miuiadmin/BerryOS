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

/// 加载全部具名符号（symtab 优先，strip 件回落 dynsym）——两层滤：
/// ① **SHF_ALLOC 装载节**（`.debug_*` 的 `$d`/`$x` 映射标实测 265 个、
/// st_value 低位域会污染最近符号命中）；② **`$` 前缀名**（AArch64 映射标
/// ——`.text` 内也有合法装载节 `$x`/`$d` 标指令/数据边界，真内核 ELF
/// 实测滤不净；它们永不为人类答案）。
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
    let shnum_raw = rd.u16(0x3c)? as usize;
    // 节头：name u32 / type u32 / flags u64 / off u64 / size u64 / link u32。
    // 野 shoff/shentsize 的加乘溢出面全走 checked（Err 出不 panic——轮 2
    // 审计 [U1]：朴素 `+` 在 dev 剖面 panic、release 回绕出「错位成功读」
    // 形——shoff=u64::MAX 时 o+4 回绕 3 恰好落在 ELF 头内）。
    let shof = |o: usize, d: usize| -> Result<usize> {
        o.checked_add(d).ok_or_else(|| bad("节表偏移溢出"))
    };
    let sh = |i: usize| -> Result<(u32, u64, usize, usize, u32)> {
        let o = shof(shoff, i.checked_mul(shentsize).ok_or_else(|| bad("节表偏移溢出"))?)?;
        Ok((rd.u32(shof(o, 4)?)?, rd.u64(shof(o, 8)?)?,
            rd.u64(shof(o, 0x18)?)? as usize,
            rd.u64(shof(o, 0x20)?)? as usize, rd.u32(shof(o, 0x28)?)?))
    };
    // shnum==0 = 扩展节计数（≥ 0xff00 节形态）：真节数在 sh[0].sh_size——
    // 夹到文件长/shentsize 帽（野值防巨分配）。
    let shcap = elf.len() / shentsize.max(1);
    let shnum = if shnum_raw == 0 {
        let (_, _, _, size, _) = sh(0)?;
        size.min(shcap)
    } else {
        shnum_raw
    };
    // 每节 SHF_ALLOC（0x2）位表：符号按 st_shndx 查节装载性。
    let mut alloc = vec![false; shnum];
    for i in 0..shnum {
        let (_, flags, _, _, _) = sh(i)?;
        alloc[i] = flags & 0x2 != 0;
    }
    for want in [2usize, 11usize] {
        // SHT_SYMTAB → SHT_DYNSYM 回落序
        for i in 0..shnum {
            let (typ, _flags, off, size, link) = sh(i)?;
            if typ != want as u32 || size == 0 {
                continue;
            }
            let (_, _, str_off, str_size, _) = sh(link as usize)?;
            if let Some(syms) = parse_symtab(elf, off, size, str_off, str_size, &alloc) {
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

/// 单表解析：24B Elf64_Sym 流 + 关联 strtab；`alloc` 节装载位表滤
/// `st_shndx`（SHN_UNDEF/特殊索引/非装载节全弃）。无效表返回 None（试下一
/// 候选）。
fn parse_symtab(
    elf: &[u8],
    off: usize,
    size: usize,
    str_off: usize,
    str_size: usize,
    alloc: &[bool],
) -> Option<Vec<Sym>> {
    let strtab = elf.get(str_off..str_off.checked_add(str_size)?)?;
    let mut syms = Vec::new();
    // 轮 2 审计 [U1]：迭代帽 = 节内**完整** 24B 项（`pos + 24 <= end`）——
    // sh_size 非 24 倍数时步进 range 会多访一个跨界残项（旧 step_by 形把
    // strtab 首字节当 symtab 项读——name_off 野值可致整表作废或幻影符号）。
    // 节内完整项越出文件（截断件）仍 None（`?`）= 换下一候选表，语义不变。
    let end = off.checked_add(size)?;
    let mut pos = off;
    while pos + 24 <= end {
        let e = elf.get(pos..pos + 24)?;
        pos += 24; // 读毕即进——下方 continue 滤路径不落步进（for/step_by 迁移面）
        let name_off = u32::from_le_bytes([e[0], e[1], e[2], e[3]]) as usize;
        let shndx = u16::from_le_bytes([e[6], e[7]]) as usize;
        let value = u64::from_le_bytes([
            e[8], e[9], e[10], e[11], e[12], e[13], e[14], e[15],
        ]);
        if value == 0 || name_off == 0 {
            continue;
        }
        if !matches!(alloc.get(shndx), Some(true)) {
            continue; // 非装载节（.debug_* 的 $d/$x）或特殊索引
        }
        let rest = strtab.get(name_off..)?;
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        let name = String::from_utf8_lossy(&rest[..end]).into_owned();
        if name.is_empty() || name.starts_with('$') {
            continue; // 空名 / AArch64 映射标（$x/$d——真内核 ELF 装载节内也有）
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

/// 合成 ELF 测试件构造（crashlog 测试共享——`testutil::build_elf`）。
/// 布局：节表 = [空, .text(SHF_ALLOC), .symtab, .strtab, .debug_line(非
/// ALLOC)]；符号 shndx=1（.text 装载节）。**恒附一枚 `$d` 污染标**
/// （shndx=4 非装载节、st_value 低位）——每次 load_symbols 都在钉
/// SHF_ALLOC 滤（真内核 ELF 实测 .debug_* 映射标污染形态）。
#[cfg(test)]
pub(crate) mod testutil {
    /// 合成最小 ELF64（真实布局字段序；`syms` shndx=1 进 .text）。
    pub fn build_elf(syms: &[(&str, u64)]) -> Vec<u8> {
        let mut strtab = vec![0u8];
        let mut name_off = Vec::new();
        for (n, _) in syms {
            name_off.push(strtab.len());
            strtab.extend_from_slice(n.as_bytes());
            strtab.push(0);
        }
        // 污染标两枚：`$d` @ 低位、非装载节（钉名字滤 + 装载滤）；
        // `debug_marker` 非装载节无 `$` 名（单独钉 SHF_ALLOC 滤）。
        let poll_off = strtab.len();
        strtab.extend_from_slice(b"$d\0");
        let dm_off = strtab.len();
        strtab.extend_from_slice(b"debug_marker\0");
        let mut symtab = Vec::new();
        let mut emit_sym = |name: usize, v: u64, shndx: u16| {
            symtab.extend_from_slice(&(name as u32).to_le_bytes());
            symtab.push(0x12); // info：STB_GLOBAL|STT_FUNC
            symtab.push(0); // other
            symtab.extend_from_slice(&shndx.to_le_bytes());
            symtab.extend_from_slice(&v.to_le_bytes());
            symtab.extend_from_slice(&0x10u64.to_le_bytes()); // size
        };
        for (i, (_, v)) in syms.iter().enumerate() {
            emit_sym(name_off[i], *v, 1); // .text 装载节
        }
        emit_sym(poll_off, 0x500, 4); // .debug_line 非装载节
        emit_sym(dm_off, 0x600, 4); // 同上（名字不带 $）
        let ehsz = 0x40;
        let sym_off = ehsz;
        let sym_sz = symtab.len();
        let str_off = sym_off + sym_sz;
        let str_sz = strtab.len();
        let shoff = (str_off + str_sz + 15) & !15;
        let shentsize = 64;
        let shnum = 5;
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
        let mut sh = |i: usize, typ: u32, flags: u64, off: usize, size: usize, link: u32| {
            let o = shoff + i * shentsize;
            d[o + 4..o + 8].copy_from_slice(&typ.to_le_bytes());
            d[o + 8..o + 0x10].copy_from_slice(&flags.to_le_bytes());
            d[o + 0x18..o + 0x20].copy_from_slice(&(off as u64).to_le_bytes());
            d[o + 0x20..o + 0x28].copy_from_slice(&(size as u64).to_le_bytes());
            d[o + 0x28..o + 0x2c].copy_from_slice(&link.to_le_bytes());
        };
        sh(1, 1, 0x2, ehsz, 0x100, 0); // .text：PROGBITS + SHF_ALLOC
        sh(2, 2, 0, sym_off, sym_sz, 3); // SHT_SYMTAB → strtab=节3
        sh(3, 3, 0, str_off, str_sz, 0); // SHT_STRTAB
        sh(4, 1, 0, ehsz, 0x40, 0); // .debug_line：PROGBITS 无 ALLOC
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbolize::testutil::build_elf;

    #[test]
    fn nearest_exact_inside_and_miss() {
        let elf = build_elf(&[("aaa", 0x1000), ("mmm", 0x2000), ("zzz", 0x3000)]);
        let syms = load_symbols(&elf).unwrap();
        let (s, off) = nearest(&syms, 0x2000).unwrap();
        assert_eq!((s.name.as_str(), off), ("mmm", 0));
        let (s, off) = nearest(&syms, 0x2abc).unwrap();
        assert_eq!((s.name.as_str(), off), ("mmm", 0xabc));
        // 首装载符号之前 = None（$d@0x500 污染标被 SHF_ALLOC 滤——不在域内
        // 抢「最近」）。
        assert!(nearest(&syms, 0xfff).is_none());
        assert!(!syms.iter().any(|s| s.name == "$d"));
    }

    #[test]
    fn stripped_elf_errors_with_hint() {
        // symtab 只剩被滤的污染标 → 等效双空路径。
        let elf = build_elf(&[]);
        assert!(load_symbols(&elf).unwrap_err().hint
            .as_deref()
            .unwrap_or("")
            .contains("berry-kernel"));
    }

    #[test]
    fn not_an_elf_rejected() {
        assert!(load_symbols(b"MZ......").is_err());
    }

    /// 轮 2 审计 [U1]：野几何三面全走 Err/夹取，不 panic 不幻读。
    #[test]
    fn wild_section_geometry_never_panics() {
        let d = build_elf(&[("fn", 0x1000)]);
        let shoff = u64::from_le_bytes(d[0x28..0x30].try_into().unwrap()) as usize;
        let entsz = 64usize;
        // ① shoff = u64::MAX：o+4 朴素加法回绕 3 = 「错位成功读」形（读到
        //    ELF 头内字节当节字段）——checked 路必须 Err。
        let mut w = d.clone();
        w[0x28..0x30].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(load_symbols(&w).is_err());
        // ② 扩展计数 + sh[0].sh_size 野值（u64::MAX）：夹到 文件长/shentsize
        //    帽 = 挡 `vec![false; shnum]` 巨分配的第一道闸；夹后仍越真节表
        //    （帽不感知 shoff 偏移，本件帽 7 > 真表 5）→ 节读越界 Err 收口
        //    （非 panic/幻读——两道防线各司其职）。
        let mut w = d.clone();
        w[shoff + 0x20..shoff + 0x28].copy_from_slice(&u64::MAX.to_le_bytes());
        w[0x3c..0x3e].copy_from_slice(&0u16.to_le_bytes());
        assert!(load_symbols(&w).is_err());
        // ③ e_shnum 野大值（0xFFFF）+ 短文件：节表读越界 → Err（非越界读）。
        let mut w = d.clone();
        w[0x3c..0x3e].copy_from_slice(&0xFFFFu16.to_le_bytes());
        assert!(load_symbols(&w).is_err());
        // ④ symtab sh_size 非 24 倍数（+8 = 尾巴跨界残项）：strtab 首字节
        //    曾被当 symtab 项读（name_off 野值 → `?` 整表作废）——新帽只收
        //    完整项，符号照常解析。
        let o2 = shoff + 2 * entsz;
        let mut sz = u64::from_le_bytes(d[o2 + 0x20..o2 + 0x28].try_into().unwrap());
        sz += 8;
        let mut w = d.clone();
        w[o2 + 0x20..o2 + 0x28].copy_from_slice(&sz.to_le_bytes());
        let syms = load_symbols(&w).unwrap();
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "fn");
    }

    #[test]
    fn extended_shnum_eligible() {
        // shnum==0 扩展节计数：真节数在 sh[0].sh_size（≥ 0xff00 节形态）。
        let mut d = build_elf(&[("fn", 0x1000)]);
        let shoff = u64::from_le_bytes(
            d[0x28..0x30].try_into().unwrap(),
        ) as usize;
        let shentsize = u16::from_le_bytes(d[0x3a..0x3c].try_into().unwrap()) as usize;
        let real: u64 = 5;
        d[shoff + 0x20..shoff + 0x28].copy_from_slice(&real.to_le_bytes()); // sh[0].sh_size
        d[0x3c..0x3e].copy_from_slice(&0u16.to_le_bytes()); // e_shnum = 0
        let syms = load_symbols(&d).unwrap();
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "fn");
        assert_eq!(shentsize, 64);
    }

    #[test]
    fn dynsym_fallback() {
        // symtab 废（改 SHT_STRTAB）→ SHT_DYNSYM 回落：节4 改 DYNSYM 指向
        // 追加的 symtab 字节拷贝（真符号 shndx=1 装载；$d shndx=4 非装载
        // ——回落路上滤同生效）。
        let mut d = build_elf(&[("aaa", 0x1000), ("mmm", 0x2000)]);
        let shoff =
            u64::from_le_bytes(d[0x28..0x30].try_into().unwrap()) as usize;
        let entsz = 64usize;
        let o2 = shoff + 2 * entsz;
        let sym_sz =
            u64::from_le_bytes(d[o2 + 0x20..o2 + 0x28].try_into().unwrap()) as usize;
        let sym_off = 0x40usize;
        d[o2 + 4..o2 + 8].copy_from_slice(&3u32.to_le_bytes()); // 废 symtab
        let content = d[sym_off..sym_off + sym_sz].to_vec();
        let dyn_off = d.len();
        d.extend_from_slice(&content);
        let o4 = shoff + 4 * entsz;
        d[o4 + 4..o4 + 8].copy_from_slice(&11u32.to_le_bytes()); // SHT_DYNSYM
        d[o4 + 0x18..o4 + 0x20].copy_from_slice(&(dyn_off as u64).to_le_bytes());
        d[o4 + 0x20..o4 + 0x28].copy_from_slice(&(sym_sz as u64).to_le_bytes());
        d[o4 + 0x28..o4 + 0x2c].copy_from_slice(&3u32.to_le_bytes()); // link=strtab
        let syms = load_symbols(&d).unwrap();
        assert!(syms.iter().any(|s| s.name == "mmm"));
        assert!(!syms.iter().any(|s| s.name == "$d"));
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
        // 映射标不进域：非装载节 265 个 + 装载节内 .text $x/$d 边界标。
        assert!(!syms.iter().any(|s| s.name.starts_with('$')));
    }
}
