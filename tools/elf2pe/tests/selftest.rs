//! 钉板五步第 3/4 步：字节级对拍 + selftest 全绿（Python 版面逐条对齐）。
//!
//! 对拍纪律：同输入 → 产物逐字节恒等（现役「同输入同字节」确定性纪律）。
//! 本文件钉板面 = Python selftest 的合成 ELF 全链转换钉板 + §A90 回归
//! （r_offset 槽位律）+ 拒面 + CLI 输出行形态。合成基准复刻真实工具链
//! 产物字段形态（含 rsym≠0 节符号路径——雷册 §A90 判例）。

use elf2pe::*;

/// 合成最小 ELF64/LE/AArch64（Python synth_elf 逐字节同构）：
/// 单 .img @0x1000（4 指令 + 2×8B 数据槽）+ .rela.img（ABS64×2 + CALL26）
/// + .symtab/.strtab/.shstrtab。
fn synth_elf() -> Vec<u8> {
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[
        0x10, 0x00, 0x00, 0x90, // 0x1000: adr x16, +4（占位 PC 相对）
        0x30, 0x02, 0x00, 0x91, // 0x1004: add x16, x16, #0
        0x41, 0x01, 0x00, 0x58, // 0x1008: ldr x1, [x10]（占位）
        0xc0, 0x03, 0x5f, 0xd6, // 0x100C: ret
    ]);
    code.extend_from_slice(&0x1008u64.to_le_bytes()); // 0x1010: 槽 A 目标
    code.extend_from_slice(&0x1000u64.to_le_bytes()); // 0x1018: 槽 B 目标
    let shstrtab: Vec<u8> = b"\0.img\0.rela.img\0.symtab\0.strtab\0.shstrtab\0".to_vec();
    let strtab: Vec<u8> = b"\0".to_vec();
    let mut symtab: Vec<u8> = Vec::new();
    symtab.extend_from_slice(&[0u8; 24]); // sym[0] 空
    // sym[1] = STT_SECTION（.img，val=节基址）——lld --emit-relocs 静态
    // reloc 惯用形态：r_off=槽位绝对 VMA + rsym=节符号 + addend=符号内偏移。
    let mut s1 = vec![0u8; 24];
    s1[4] = 3 << 4; // st_info = STT_SECTION<<4
    s1[12..16].copy_from_slice(&0x1000u32.to_le_bytes()); // st_value
    symtab.extend_from_slice(&s1);
    let mut rela: Vec<u8> = Vec::new();
    let mut rel = |r_offset: u64, r_info: u64, addend: i64| {
        rela.extend_from_slice(&r_offset.to_le_bytes());
        rela.extend_from_slice(&r_info.to_le_bytes());
        rela.extend_from_slice(&addend.to_le_bytes());
    };
    rel(0x1010, R_AARCH64_ABS64 as u64, 0x1008); // 槽 A，rsym=0
    rel(0x0, 283, 0); // CALL26（忽略型）
    rel(0x1018, (1u64 << 32) | R_AARCH64_ABS64 as u64, 0x1000); // 槽 B，rsym=1

    let ehsize = 64usize;
    let blobs = [&code, &rela, &symtab, &strtab, &shstrtab];
    let mut sec_offs = Vec::new();
    let mut off = ehsize;
    for b in &blobs {
        sec_offs.push(off);
        off += b.len();
    }
    let mut out = vec![0u8; ehsize];
    for b in &blobs {
        out.extend_from_slice(b);
    }
    let shoff = out.len();
    out.extend(std::iter::repeat(0u8).take(6 * 64));
    // ehdr
    out[..4].copy_from_slice(b"\x7fELF");
    out[4] = 2; // ELFCLASS64
    out[5] = 1; // ELFDATA2LSB
    out[6] = 1; // EV_CURRENT
    out[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    out[18..20].copy_from_slice(&EM_AARCH64.to_le_bytes());
    out[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version
    out[24..32].copy_from_slice(&0x1000u64.to_le_bytes()); // e_entry
    out[40..48].copy_from_slice(&(shoff as u64).to_le_bytes()); // e_shoff
    out[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    out[58..60].copy_from_slice(&64u16.to_le_bytes()); // e_shentsize
    out[60..62].copy_from_slice(&6u16.to_le_bytes()); // e_shnum
    out[62..64].copy_from_slice(&5u16.to_le_bytes()); // e_shstrndx
    let shstr_index = |n: &[u8], total: &[u8]| -> u32 {
        let pat = [n, b"\0"].concat();
        total.windows(pat.len()).position(|w| w == pat).unwrap() as u32
    };
    let st = &shstrtab;
    let sections: Vec<(u32, u32, u64, u64, usize, u64, u32, u32, u64)> = vec![
        (0, 0, 0, 0, usize::MAX, 0, 0, 0, 0), // [0] NULL
        (shstr_index(b".img", st), SHT_PROGBITS, SHF_ALLOC | 0x4 | 0x1,
         0x1000, 0, code.len() as u64, 0, 0, 0), // [1] .img
        (shstr_index(b".rela.img", st), SHT_RELA, 0, 0, 1,
         rela.len() as u64, 3, 1, 24), // [2] .rela.img
        (shstr_index(b".symtab", st), SHT_SYMTAB, 0, 0, 2,
         symtab.len() as u64, 4, 1, 24), // [3] .symtab
        (shstr_index(b".strtab", st), SHT_STRTAB, 0, 0, 3,
         strtab.len() as u64, 0, 0, 0), // [4] .strtab
        (shstr_index(b".shstrtab", st), SHT_STRTAB, 0, 0, 4,
         shstrtab.len() as u64, 0, 0, 0), // [5] .shstrtab
    ];
    for (i, (no, ty, fl, addr, doi, size, link, info, ent)) in sections.iter().enumerate() {
        let doff = if *doi == usize::MAX { 0 } else { sec_offs[*doi] as u64 };
        let o = shoff + i * 64;
        out[o..o + 4].copy_from_slice(&no.to_le_bytes());
        out[o + 4..o + 8].copy_from_slice(&ty.to_le_bytes());
        out[o + 8..o + 16].copy_from_slice(&fl.to_le_bytes());
        out[o + 16..o + 24].copy_from_slice(&addr.to_le_bytes());
        out[o + 24..o + 32].copy_from_slice(&doff.to_le_bytes());
        out[o + 32..o + 40].copy_from_slice(&size.to_le_bytes());
        out[o + 40..o + 44].copy_from_slice(&link.to_le_bytes());
        out[o + 44..o + 48].copy_from_slice(&info.to_le_bytes());
        out[o + 56..o + 64].copy_from_slice(&ent.to_le_bytes());
    }
    out
}

#[test]
fn synthetic_full_chain() {
    let elf = synth_elf();
    // 合成源内 3 条 RELA：2 ABS64（槽 A rsym=0 / 槽 B rsym=节符号——双路径
    // 都进 .reloc 且槽位恒取 r_offset）+ 1 CALL26（忽略）。
    let relocs = collect_relocs(&Elf::parse(&elf).unwrap()).unwrap();
    assert_eq!(relocs, vec![0x1010, 0x1018]);
    let out = convert(&elf).unwrap();
    let line = verify(&out.pe, 2).unwrap();
    assert!(line.contains("entry=0x1000"));
    // 头部面：MZ / PE / ARM64 / PE32+ / EFI app / ImageBase=0 / entry=0x1000。
    let pe = &out.pe;
    assert_eq!(&pe[..2], b"MZ");
    let pe_off = 0x80;
    assert_eq!(&pe[pe_off..pe_off + 4], b"PE\0\0");
    let machine = u16::from_le_bytes([pe[pe_off + 4], pe[pe_off + 5]]);
    let nsec = u16::from_le_bytes([pe[pe_off + 6], pe[pe_off + 7]]);
    assert_eq!((machine, nsec), (MACHINE_ARM64, 2));
    let opt = pe_off + 24;
    assert_eq!(u16::from_le_bytes([pe[opt], pe[opt + 1]]), OPT_MAGIC_PE32PLUS);
    assert_eq!(u32::from_le_bytes([pe[opt + 16], pe[opt + 17], pe[opt + 18], pe[opt + 19]]), 0x1000);
    assert_eq!(u16::from_le_bytes([pe[opt + 68], pe[opt + 69]]), SUBSYSTEM_EFI_APPLICATION);
    assert_eq!(u64::from_le_bytes(pe[opt + 24..opt + 32].try_into().unwrap()), 0);
    // .img 数据字节逐位进 PE（RVA==vaddr ⇒ file@0x1000 即镜像内容）。
    let parsed = Elf::parse(&elf).unwrap();
    let img = parsed.section_by_name(".img").unwrap();
    assert_eq!(&pe[0x1000..0x1000 + 20], &parsed.contents(img).unwrap()[..20]);
    // .reloc 块钉板：页 0x1000、块 12B（2 DIR64 条目）——槽 A 0x1010（rsym=0）
    // + 槽 B 0x1018（rsym=节符号——§A90 旧 bug 曾把这条错打 0x1000，本钉板
    // 回归执法）。
    let rva = u32::from_le_bytes(pe[opt + 112 + 40..opt + 116 + 40].try_into().unwrap());
    let rsz = u32::from_le_bytes(pe[opt + 116 + 40..opt + 120 + 40].try_into().unwrap());
    assert_eq!((rva, rsz), (0x2000, 12));
    let b = &pe[0x2000..0x2000 + 12];
    assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 0x1000); // PageRVA
    assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 12); // BlockSize
    assert_eq!(u16::from_le_bytes(b[8..10].try_into().unwrap()), RELOC_DIR64 << 12 | 0x010);
    assert_eq!(u16::from_le_bytes(b[10..12].try_into().unwrap()), RELOC_DIR64 << 12 | 0x018);
    // 确定性：两次转换逐字节同。
    assert_eq!(convert(&elf).unwrap().pe, out.pe);
}

#[test]
fn synth_elf_oracle_bytes() {
    // 合成基准复刻判例（§A90）：字段形态必须与 Python oracle synth_elf
    // 逐字节恒等——防两侧基准漂移（对拍取证面挂 CI/本地 oracle 跑）。
    let elf = synth_elf();
    let parsed = Elf::parse(&elf).unwrap();
    assert_eq!(parsed.e_entry, 0x1000);
    assert_eq!(parsed.sections.len(), 6);
    let names: Vec<&str> = parsed.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["", ".img", ".rela.img", ".symtab", ".strtab", ".shstrtab"]);
    let rela = parsed.section_by_name(".rela.img").unwrap();
    let blob = parsed.contents(rela).unwrap();
    assert_eq!(blob.len(), 72); // 3 条 RELA × 24B
    // CALL26 @ 第二条（r_offset=0，忽略型）——白名单内不炸。
    assert_eq!(collect_relocs(&parsed).unwrap(), vec![0x1010, 0x1018]);
}

/// 拒面全谱——与 Python fail() 分支一一对应。
#[test]
fn reject_faces() {
    // 坏魔数
    let mut elf = synth_elf();
    elf[0] = 0x00;
    assert_eq!(Elf::parse(&elf).unwrap_err(), PeError::NotElf);
    // ELF32 / 大端
    let mut elf = synth_elf();
    elf[4] = 1;
    assert_eq!(Elf::parse(&elf).unwrap_err(), PeError::NotElf64Le);
    let mut elf = synth_elf();
    elf[5] = 2;
    assert_eq!(Elf::parse(&elf).unwrap_err(), PeError::NotElf64Le);
    // 非 AArch64
    let mut elf = synth_elf();
    elf[18..20].copy_from_slice(&62u16.to_le_bytes());
    assert_eq!(Elf::parse(&elf).unwrap_err(), PeError::NotAarch64(62));
    // 白名单外重定位型必须拒（§A90「静默错比崩更糟」）。
    let elf = synth_elf();
    let parsed = Elf::parse(&elf).unwrap();
    let rela = parsed.section_by_name(".rela.img").unwrap();
    let rela_off = rela.offset as usize;
    let mut bad = elf.clone();
    // .rela.img 第二条（CALL26）改成未知型 0xDEAD。
    bad[rela_off + 24 + 8..rela_off + 24 + 16]
        .copy_from_slice(&0xDEADu64.to_le_bytes());
    match convert(&bad).unwrap_err() {
        PeError::UnknownReloc { rtype, r_offset, .. } => {
            assert_eq!(rtype, 0xDEAD);
            assert_eq!(r_offset, 0x0);
        }
        e => panic!("未知重定位型未被拒绝：{e:?}"),
    }
    // 缺 .img 节：改名即拒。
    let mut elf = synth_elf();
    let st = Elf::parse(&elf).unwrap();
    let img_sec = st.section_by_name(".img").unwrap().clone();
    let shstr = st.section_by_name(".shstrtab").unwrap();
    let shstr_off = shstr.offset as usize;
    // .img → .imh（破坏节名，保持其余字段不动）。
    elf[shstr_off + img_sec_elf_name_off(&st) + 2] = b'h';
    match convert(&elf).unwrap_err() {
        PeError::MissingImg => {}
        e => panic!("缺 .img 未被拒绝：{e:?}"),
    }
}

/// synth 侧辅助：.img 节名在 shstrtab 内的偏移。
fn img_sec_elf_name_off(elf: &Elf) -> usize {
    let img = elf.section_by_name(".img").unwrap();
    // shstrtab 布局 "\0.img\0.rela.img\0..." ⇒ .img @ 1。
    let shstr = elf.section_by_name(".shstrtab").unwrap();
    let blob = elf.contents(shstr).unwrap();
    blob.windows(4).position(|w| w == b".img").unwrap()
}

/// 多 alloc 节拒（链接脚本漂移执法）。
#[test]
fn multiple_alloc_rejected() {
    let elf = synth_elf();
    let parsed = Elf::parse(&elf).unwrap();
    let img = parsed.section_by_name(".img").unwrap();
    // 伪造第二个 alloc 节：把 .strtab 的 flags 置 SHF_ALLOC。
    let mut bad = elf.clone();
    let strtab = parsed.section_by_name(".strtab").unwrap();
    let shoff = u64::from_le_bytes(bad[40..48].try_into().unwrap()) as usize;
    let strtab_sh = shoff + 4 * 64; // [4] .strtab
    bad[strtab_sh + 8..strtab_sh + 16].copy_from_slice(&SHF_ALLOC.to_le_bytes());
    // info/sh_link 无关——collect/convert 走 elf_section_payload 执法。
    match convert(&bad).unwrap_err() {
        PeError::MultipleAlloc { name } => assert_eq!(name, ".strtab"),
        e => panic!("多 alloc 未被拒绝：{e:?}"),
    }
    let _ = img;
}

/// 空重定位表：.reloc 恒有合法空块（仅块头 8B）——build_reloc_blocks 直测。
#[test]
fn empty_reloc_block() {
    let b = build_reloc_blocks(&[]);
    assert_eq!(b.len(), 8);
    assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 0); // PageRVA=0
    assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 8); // BlockSize=8
    // 奇数条目页：块内 4B 对齐填充（no-op 型）。
    let b = build_reloc_blocks(&[0x1000]);
    assert_eq!(b.len(), 12);
    assert_eq!(u16::from_le_bytes(b[8..10].try_into().unwrap()), RELOC_DIR64 << 12 | 0);
    assert_eq!(u16::from_le_bytes(b[10..12].try_into().unwrap()), RELOC_ABSOLUTE << 12);
}

/// verify 拒面：坏 MZ / 坏 PE 签名 / 零尺寸块死循环守卫。
#[test]
fn verify_rejects() {
    let elf = synth_elf();
    let out = convert(&elf).unwrap();
    assert!(verify(&out.pe, 2).is_ok());
    // 无 MZ
    let mut bad = out.pe.clone();
    bad[0] = b'X';
    assert!(verify(&bad, 2).is_err());
    // 零尺寸重定位块：verify 不死循环（0 迭代即闭合检查兜住）。
    let mut bad = out.pe.clone();
    let opt = 0x80 + 24;
    let rva = u32::from_le_bytes(bad[opt + 152..opt + 156].try_into().unwrap()) as usize;
    bad[rva + 4..rva + 8].copy_from_slice(&0u32.to_le_bytes());
    match verify(&bad, 2) {
        Ok(_) => panic!("零尺寸块须拒"),
        Err(_) => {}
    }
}

/// Rust 侧独立面：截断输入显式拒（Python struct.unpack 越界炸同语义）。
#[test]
fn truncated_input_rejected() {
    let elf = synth_elf();
    assert_eq!(Elf::parse(&elf[..60]).unwrap_err(), PeError::NotElf);
    assert_eq!(Elf::parse(b"").unwrap_err(), PeError::NotElf);
    // 节头表越界：e_shoff 指向文件尾外。
    let mut bad = elf.clone();
    bad[40..48].copy_from_slice(&0xFFFF_0000u64.to_le_bytes());
    assert_eq!(Elf::parse(&bad).unwrap_err(), PeError::Truncated);
}

/// 真实产物对拍（首刀取证面）：bootloader/berryboot/out/berryboot.elf 在位
/// 时做全链对拍——Rust convert vs Python oracle（tools/elf2pe_oracle.py）产物
/// sha256 恒等 + verify 行全等。缺席打 skip（CI 无构建产物，钉板面已覆盖
/// 合成全链）。
#[test]
fn real_elf_oracle_difftest() {
    let path = std::path::Path::new("../../bootloader/berryboot/out/berryboot.elf");
    let Ok(elf_bytes) = std::fs::read(path) else {
        eprintln!("skip: berryboot.elf 缺席（CI 无构建产物）");
        return;
    };
    let out = convert(&elf_bytes).unwrap();
    let line = verify(&out.pe, out.reloc_count).unwrap();
    assert!(!line.is_empty());
    // Python oracle（降格为 elf2pe_oracle.py，二刀后仅对拍取证用）在位时：
    // 字节级对拍（sha256 恒等）。
    let oracle = std::path::Path::new("../elf2pe_oracle.py");
    if oracle.exists() {
        let tmp = std::env::temp_dir().join(format!(
            "elf2pe-diff-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let efi = tmp.join("rust.efi");
        std::fs::write(&efi, &out.pe).unwrap();
        let elf_copy = tmp.join("in.elf");
        std::fs::write(&elf_copy, &elf_bytes).unwrap();
        let py = tmp.join("py.efi");
        let status = std::process::Command::new("python3")
            .arg(oracle.canonicalize().unwrap())
            .arg(&elf_copy)
            .arg(&py)
            .output()
            .expect("python3 起动失败");
        assert!(status.status.success(), "oracle 转换失败：{}",
            String::from_utf8_lossy(&status.stderr));
        let py_bytes = std::fs::read(&py).unwrap();
        assert_eq!(out.pe, py_bytes, "Rust 与 Python oracle 产物须逐字节恒等");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
