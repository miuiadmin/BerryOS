//! 钉板五步第 3/4 步：字节级对拍 + selftest 全绿（Python 版面逐条对齐）。
//!
//! 对拍纪律：同输入 → 产物逐字节恒等（现役「同输入同字节」确定性纪律）。
//! 本文件钉板面 = Python selftest 的双段正样本 + 8 拒面 + Rust 侧独立面
//! （bin CLI 行为面挂 justfile 接线，属二刀换轨件——本刀仅 lib 纯面）。

use elf2flat::*;

const IDENT_OK: [u8; 16] = [
    0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

fn ident_bad_magic() -> [u8; 16] {
    let mut i = IDENT_OK;
    i[0] = 0x00;
    i
}

fn ident_elf32() -> [u8; 16] {
    let mut i = IDENT_OK;
    i[4] = 1;
    i
}

/// Python selftest 正样本同款：段1 [0x1000,0x1020) 恰 file；段2 vaddr=0x2000
/// 跨页 + memsz>filesz（bss 尾零面）——total = ceil(0x1030/0x1000) = 2 页。
#[test]
fn double_segment_positive() {
    let seg1: Vec<u8> = (0..0x20u64).map(|k| ((k * 31 + 7) % 256) as u8).collect();
    let seg2: Vec<u8> = (0..0x10u64).map(|k| ((k * 31 + 8) % 256) as u8).collect();
    let elf = synth_elf(&[(0x1000, 0x20, 0x20), (0x2000, 0x10, 0x30)], 2, 183, &IDENT_OK);
    let f = build_flat(&elf, 0x1000, 4096, 0).expect("双段正样本须过");
    assert_eq!((f.base, f.file_len, f.mem_len, f.total), (0x1000, 0x1010, 0x1030, 0x2000));
    assert_eq!(f.img.len(), 0x2000);
    assert_eq!(&f.img[0x0..0x20], &seg1[..], "段1 文件字节未落位");
    assert!(f.img[0x20..0x1000].iter().all(|&b| b == 0), "段间 gap 未零填");
    assert_eq!(&f.img[0x1000..0x1010], &seg2[..], "段2 文件字节未落位");
    assert!(f.img[0x1010..].iter().all(|&b| b == 0), "bss 尾/页取整零面破");
    // 转换确定性：两次调用逐字节恒等。
    assert_eq!(build_flat(&elf, 0x1000, 4096, 0).unwrap().img, f.img, "转换不确定");
}

/// 拒面全谱 ×8——与 Python expect_reject 一一对应。
#[test]
fn reject_faces() {
    // 坏魔数
    let elf = synth_elf(&[(0x1000, 0x10, 0x10)], 2, 183, &ident_bad_magic());
    assert_eq!(build_flat(&elf, 0x1000, 4096, 0), Err(FlatError::BadMagic));
    // ELF32
    let elf = synth_elf(&[(0x1000, 0x10, 0x10)], 2, 183, &ident_elf32());
    assert_eq!(build_flat(&elf, 0x1000, 4096, 0), Err(FlatError::NotElf64Le));
    // 非 AArch64
    let elf = synth_elf(&[(0x1000, 0x10, 0x10)], 2, 62, &IDENT_OK);
    assert_eq!(build_flat(&elf, 0x1000, 4096, 0), Err(FlatError::NotAarch64(62)));
    // ET_DYN
    let elf = synth_elf(&[(0x1000, 0x10, 0x10)], 3, 183, &IDENT_OK);
    assert_eq!(build_flat(&elf, 0x1000, 4096, 0), Err(FlatError::NotExec(3)));
    // 基址漂移
    let elf = synth_elf(&[(0x2000, 0x10, 0x10)], 2, 183, &IDENT_OK);
    assert_eq!(build_flat(&elf, 0x1000, 4096, 0), Err(FlatError::BaseDrift { lo: 0x2000, base: 0x1000 }));
    // PT_LOAD 重叠
    let elf = synth_elf(&[(0x1000, 0x10, 0x2000), (0x2000, 0x10, 0x10)], 2, 183, &IDENT_OK);
    assert_eq!(build_flat(&elf, 0x1000, 4096, 0), Err(FlatError::Overlap { v1: 0x1000, v2: 0x2000 }));
    // 超 --max
    let elf = synth_elf(&[(0x1000, 0x20, 0x20), (0x2000, 0x10, 0x30)], 2, 183, &IDENT_OK);
    assert_eq!(build_flat(&elf, 0x1000, 4096, 4096), Err(FlatError::OverMax { total: 0x2000, max: 4096 }));
    // 无 PT_LOAD
    let elf = synth_elf(&[], 2, 183, &IDENT_OK);
    assert_eq!(build_flat(&elf, 0x1000, 4096, 0), Err(FlatError::NoPtLoad));
}

/// Python 版无显式拒的短输入面：Rust 侧显式拒（防平像静默劣化）。
#[test]
fn short_input_rejected() {
    assert_eq!(build_flat(b"\x7fELF", 0x1000, 4096, 0), Err(FlatError::NotElf64Le));
    assert_eq!(build_flat(b"", 0x1000, 4096, 0), Err(FlatError::BadMagic));
    assert_eq!(build_flat(&[0x7f, b'E', b'L', b'F', 2, 1], 0x1000, 4096, 0), Err(FlatError::NotElf64Le));
}

/// 短 ehdr（<64B）拒——Python struct.unpack 越界炸，语义同为非 0 退出。
#[test]
fn short_ehdr_rejected() {
    let elf = synth_elf(&[(0x1000, 0x10, 0x10)], 2, 183, &IDENT_OK);
    let short = &elf[..60];
    assert_eq!(build_flat(short, 0x1000, 4096, 0), Err(FlatError::NotElf64Le));
}

/// synth_elf 字节模式重导对拍（勿手抄常量纪律）。
#[test]
fn synth_pattern_roundtrip() {
    let elf = synth_elf(&[(0x1000, 8, 8)], 2, 183, &IDENT_OK);
    let f = build_flat(&elf, 0x1000, 4096, 0).expect("单段须过");
    // 段内首 8 字节 = (k*31+7)%256，k=0..8
    let expect: Vec<u8> = (0..8u64).map(|k| ((k * 31 + 7) % 256) as u8).collect();
    assert_eq!(&f.img[0..8], &expect[..]);
}
