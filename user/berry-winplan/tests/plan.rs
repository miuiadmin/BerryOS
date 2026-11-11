//! berry-winplan host 测试（X-5 窗规划纯面——自 berry-exec 外迁解锁）。
//!
//! 覆盖：三色窗分解的完备铺满不变量、congruent 门 (a')-(e') 各拒面、
//! musl 族多 k 布局过门、合成 ELF 全链（合成 ehdr/phdr → `elf_head` 解析
//! → `plan_wins` 规划——与 berry-exec 真实调用链同构）。
//!
//! 注：ElfHead 的构造面（`segments()`/`image_min`/`image_span`）由
//! `berry_loader::elf_head` 从合成 ELF 字节解析产出——测试侧不绕过
//! 解析器直造结构（「解析器即产品」同族纪律）。

use berry_loader::{elf_head, ElfHead};
use berry_winplan::{plan_wins, WinKind, WinPlan, WIN_MAX};

const PAGE: u64 = 4096;

/// 空窗哨兵（构造 out 数组用——berry-exec 同款字面量形态）。
fn zero_win() -> WinPlan {
    WinPlan { start: 0, len: 0, kind: WinKind::Zero }
}


// ---- 合成 ELF 面（berry-loader 测试同款 helper 形态） ----

fn phdr(ptype: u32, offset: u64, vaddr: u64, filesz: u64, memsz: u64) -> Vec<u8> {
    let mut p = vec![0u8; 56];
    p[0..4].copy_from_slice(&ptype.to_le_bytes());
    p[4..8].copy_from_slice(&5u32.to_le_bytes()); // p_flags RX
    p[8..16].copy_from_slice(&offset.to_le_bytes());
    p[16..24].copy_from_slice(&vaddr.to_le_bytes());
    p[32..40].copy_from_slice(&filesz.to_le_bytes());
    p[40..48].copy_from_slice(&memsz.to_le_bytes());
    p
}

fn elf_of(phdrs: &[u8], entry: u64) -> Vec<u8> {
    let mut e = vec![0u8; 64];
    e[..4].copy_from_slice(b"\x7fELF");
    e[4] = 2;
    e[5] = 1;
    e[6] = 1;
    e[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    e[18..20].copy_from_slice(&183u16.to_le_bytes()); // EM_AARCH64
    e[24..32].copy_from_slice(&entry.to_le_bytes());
    e[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    e[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    e[56..58].copy_from_slice(&((phdrs.len() / 56) as u16).to_le_bytes()); // e_phnum
    e.extend_from_slice(phdrs);
    e
}

/// 合成 ELF → elf_head 解析（file_len 足量盖住所有段文件域）。
fn head_of(segs: &[(u64, u64, u64, u64)], entry: u64) -> ElfHead {
    // (file_off, vaddr, filesz, memsz)
    let mut phdrs = Vec::new();
    let mut file_len = 0u64;
    for &(o, v, f, m) in segs {
        phdrs.extend_from_slice(&phdr(1, o, v, f, m));
        file_len = file_len.max(o + f);
    }
    let bytes = elf_of(&phdrs, entry);
    elf_head(&bytes, file_len).expect("合成 ELF 应解析成功")
}

/// 断言窗表升序完备铺满 [base, base+len)（walk 不变量——reap 逐窗 unmap
/// 即全幅回收的几何前提）。
fn assert_tiling(wins: &[WinPlan], base: u64, len: u64) {
    let mut cursor = base;
    for w in wins {
        assert_eq!(w.start, cursor, "窗基应衔接游标（升序铺满无洞无叠）");
        assert!(w.len > 0 && w.len % PAGE == 0, "窗幅应为正页倍数");
        cursor = w.start + w.len;
    }
    assert_eq!(cursor, base + len, "窗表顶应恰为映像顶");
}

// ---- 基本形 ----

#[test]
fn single_aligned_segment_one_file_window() {
    // 单段 [0x1000, 0x3000) filesz=2 页：恰一 file 窗，无 gap/尾零。
    let h = head_of(&[(0, 0x1000, 2 * PAGE, 2 * PAGE)], 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    let nw = plan_wins(&h, 0x1000, 2 * PAGE, 0x10000, u64::MAX, &mut out).unwrap();
    assert_eq!(nw, 1);
    assert_eq!(
        out[0],
        WinPlan {
            start: 0x1000,
            len: 2 * PAGE,
            kind: WinKind::File { delta: 0x10000 }
        }
    );
    assert_tiling(&out[..nw], 0x1000, 2 * PAGE);
}

#[test]
fn head_gap_and_bss_tail_zero_windows() {
    // 段 [0x2000, 0x4000) filesz=1 页、映像幅 [0x1000, 0x5000)：
    // 首 gap 零窗 + file 窗 + 末 bss 零窗。
    let h = head_of(&[(PAGE, 0x2000, PAGE, 2 * PAGE)], 0x2000);
    let mut out = [zero_win(); WIN_MAX];
    let nw = plan_wins(&h, 0x1000, 4 * PAGE, 0x10000, u64::MAX, &mut out).unwrap();
    assert_eq!(nw, 3);
    assert_eq!(out[0].kind, WinKind::Zero);
    assert_eq!((out[0].start, out[0].len), (0x1000, PAGE));
    assert_eq!(out[1].kind, WinKind::File { delta: (0x10000 + PAGE) as i64 });
    assert_eq!((out[1].start, out[1].len), (0x2000, PAGE));
    assert_eq!(out[2].kind, WinKind::Zero);
    assert_eq!((out[2].start, out[2].len), (0x3000, 2 * PAGE));
    assert_tiling(&out[..nw], 0x1000, 4 * PAGE);
}

#[test]
fn unaligned_filesz_tail_makes_hybrid() {
    // 段 [0x1000, 0x1000+PAGE+100)：file 窗 1 页 + hybrid 尾页。
    let h = head_of(&[(0, 0x1000, PAGE + 100, PAGE + 100)], 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    let nw = plan_wins(&h, 0x1000, 2 * PAGE, 0x10000, u64::MAX, &mut out).unwrap();
    assert_eq!(nw, 2);
    assert_eq!(out[0].kind, WinKind::File { delta: 0x10000 });
    assert_eq!((out[0].start, out[0].len), (0x1000, PAGE));
    match out[1].kind {
        WinKind::Hybrid { soff, flen } => {
            assert_eq!(soff, 0x10000 + PAGE);
            assert_eq!(flen, 100);
        }
        k => panic!("尾窗应为 Hybrid，得 {:?}", k),
    }
    assert_eq!((out[1].start, out[1].len), (0x2000, PAGE));
    assert_tiling(&out[..nw], 0x1000, 2 * PAGE);
}

// ---- musl 族多 k 布局过门（X-5 动机面） ----

#[test]
fn musl_multi_k_layout_passes() {
    // musl 族典型形：k_i = {0, -0x1000, -0x2000, -0x3000}（file_off 落后
    // vaddr 越来越多）——X-3c 单 k 全局门恒拒形，X-5 逐段化过门。
    let segs = [
        (0u64, 0x1000, PAGE, PAGE),                // k=0
        (0x1000, 0x3000, PAGE, 2 * PAGE),          // k=-0x2000, 尾 bss
        (0x2000, 0x5000, PAGE + 60, PAGE + 60),    // k=-0x3000, 尾 hybrid
    ];
    let h = head_of(&segs, 0x1000);
    // 映像幅：min=0x1000, max_end=0x5000+PAGE+60 → span=0x5000 → len=0x6000。
    let base = 0x1000;
    let len = page_ceil(h.image_span);
    let mut out = [zero_win(); WIN_MAX];
    let nw = plan_wins(&h, base, len, 0x20000, u64::MAX, &mut out).unwrap();
    // 段0：file@0x1000(1页)；段1：gap@0x2000(1页)+file@0x3000(1页)+尾零由末窗收口；
    // 段2：gap@0x4000(1页)+file@0x5000(1页)+hybrid@0x6000；末零窗无（top=0x7000=cursor）。
    assert_eq!(nw, 6);
    assert_tiling(&out[..nw], base, len);
    assert_eq!(out[0].kind, WinKind::File { delta: 0x20000 as i64 });
    assert_eq!(out[1].kind, WinKind::Zero); // 段1 前 gap
    assert_eq!(out[2].kind, WinKind::File { delta: (0x20000 - 0x2000 + 0x3000) as i64 });
    assert_eq!(out[3].kind, WinKind::Zero); // 段2 前 gap
    assert_eq!(out[4].kind, WinKind::File { delta: (0x20000 - 0x3000 + 0x5000) as i64 });
    assert!(matches!(out[5].kind, WinKind::Hybrid { flen: 60, .. }));
}

fn page_ceil(v: u64) -> u64 {
    v.div_ceil(PAGE) * PAGE
}

// ---- congruent 门拒面 ----

#[test]
fn gate_b_data_off_misaligned_rejects() {
    let h = head_of(&[(0, 0x1000, PAGE, PAGE)], 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    assert!(plan_wins(&h, 0x1000, PAGE, PAGE + 1, u64::MAX, &mut out).is_none());
}

#[test]
fn gate_c_k_not_page_multiple_rejects() {
    // file_off=PAGE, vaddr=0x1000 → k=PAGE-PAGE… k = 0x1000-0x1000=0? 构造
    // k 非页倍：file_off = 0x1000+8（vaddr=0x1000 → k=8）。
    let h = head_of(&[(0x1008, 0x1000, PAGE, PAGE)], 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    assert!(plan_wins(&h, 0x1000, PAGE, 0, u64::MAX, &mut out).is_none());
}

#[test]
fn gate_a_descending_segments_rejects() {
    // 段序 vaddr 降序（0x3000 在前、0x1000 在后）→ (a') 拒。
    let segs = [(0, 0x3000, PAGE, PAGE), (0x1000, 0x1000, PAGE, PAGE)];
    let h = head_of(&segs, 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    assert!(plan_wins(&h, 0x1000, 3 * PAGE, 0x20000, u64::MAX, &mut out).is_none());
}

#[test]
fn gate_d_window_top_beyond_image_rejects() {
    // 段 memsz 谎大不至（elf_head 已拦 filesz>memsz/vend>USER_TOP）；这里
    // 打映像幅与段不符：len 只给 1 页而段占 2 页 → 窗顶越 top → (d') 拒。
    let h = head_of(&[(0, 0x1000, 2 * PAGE, 2 * PAGE)], 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    assert!(plan_wins(&h, 0x1000, PAGE, 0, u64::MAX, &mut out).is_none());
}

#[test]
fn gate_e_source_beyond_cap_rejects() {
    // file 窗源域越根幅：delta+wlen > cap。
    let h = head_of(&[(0, 0x1000, 2 * PAGE, 2 * PAGE)], 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    // cap 恰差 1 字节盖不住源域 [delta, delta+2P)。
    assert!(plan_wins(&h, 0x1000, 2 * PAGE, 0x10000, 0x10000 + 2 * PAGE - 1, &mut out).is_none());
    // 恰盖住（边界）→ 过。
    assert!(plan_wins(&h, 0x1000, 2 * PAGE, 0x10000, 0x10000 + 2 * PAGE, &mut out).is_some());
}

#[test]
fn gate_e_hybrid_soff_beyond_cap_rejects() {
    // hybrid 源域越根幅：soff+flen > cap。
    let h = head_of(&[(0, 0x1000, PAGE + 100, PAGE + 100)], 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    let cap = 0x10000 + PAGE + 99; // soff+flen = 0x10000+PAGE+100 恰越 1
    assert!(plan_wins(&h, 0x1000, 2 * PAGE, 0x10000, cap, &mut out).is_none());
    let cap = 0x10000 + PAGE + 100;
    assert!(plan_wins(&h, 0x1000, 2 * PAGE, 0x10000, cap, &mut out).is_some());
}

#[test]
fn all_zero_filesz_segments_rejects() {
    // 全段 filesz==0（纯 bss）→ 无 file 面，保守拒（X-3c `k?` 同款）。
    let h = head_of(&[(0, 0x1000, 0, 2 * PAGE)], 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    assert!(plan_wins(&h, 0x1000, 2 * PAGE, 0, u64::MAX, &mut out).is_none());
}

#[test]
fn window_overflow_win_max_rejects() {
    // 交错多段令窗数超 WIN_MAX（out.len()）→ None（eager 兜底不硬塞）。
    // 8 段（elf_head MAX_SEGMENTS 帽）× (file+hybrid) + 7 段间 gap = 23 窗 > 16。
    let mut segs = Vec::new();
    for i in 0..8u64 {
        segs.push((i * 2 * PAGE, 0x1000 + i * 3 * PAGE, PAGE + 100, PAGE + 100));
    }
    let h = head_of(&segs, 0x1000);
    let base = 0x1000;
    let len = page_ceil(h.image_span);
    let mut out = [zero_win(); WIN_MAX];
    assert!(plan_wins(&h, base, len, 0x100000, u64::MAX, &mut out).is_none());
}

#[test]
fn out_slice_shorter_rejects() {
    // out 短于所需窗数（如 2 窗需求给 1 槽）→ None。
    let h = head_of(&[(PAGE, 0x2000, PAGE, 2 * PAGE)], 0x2000);
    let mut out = [zero_win(); 1];
    assert!(plan_wins(&h, 0x1000, 4 * PAGE, 0x10000, u64::MAX, &mut out).is_none());
}

// ---- 溢出/下溢安全面 ----

#[test]
fn base_len_overflow_returns_none() {
    let h = head_of(&[(0, 0x1000, PAGE, PAGE)], 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    assert!(plan_wins(&h, u64::MAX, PAGE, 0, u64::MAX, &mut out).is_none());
}

#[test]
fn pure_bss_gap_between_file_segments_covered_by_zero() {
    // 中置纯 bss 段（filesz=0）：跳过参与，页域由前段末零/后段前 gap 收口。
    let segs = [(0, 0x1000, PAGE, PAGE), (0, 0x2000, 0, PAGE), (PAGE, 0x3000, PAGE, PAGE)];
    let h = head_of(&segs, 0x1000);
    let mut out = [zero_win(); WIN_MAX];
    let nw = plan_wins(&h, 0x1000, 3 * PAGE, 0x10000, u64::MAX, &mut out).unwrap();
    // file@0x1000 + 零@gap[0x2000,0x3000) + file@0x3000。
    assert_eq!(nw, 3);
    assert_eq!(out[1].kind, WinKind::Zero);
    assert_tiling(&out[..nw], 0x1000, 3 * PAGE);
}
