//! berry-strmem host 测试——mem/str 族字节序语义钉板。
//!
//! 向量原则：每组正例 + 边界 + 拒/退化面；对齐快路/未对齐逐字节两形态
//! 都过（u64 快路只在双 8 对齐启用——同 buffer 内偏移 0/1 各跑一遍）；
//! 与 musl/C 语义的分歧点（strncpy pad-to-n / strrchr c=0 / atoi 夹紧）
//! 逐条注释钉死。

use berry_strmem::*;

/// memcpy：对齐快路 + 未对齐逐字节 + 尾段三种余量（8/9/7）+ 零长。
#[test]
fn memcpy_copies_all_lengths_both_alignment_forms() {
    let src: Vec<u8> = (0u8..=32).collect();
    for off in [0usize, 1] {
        // dst 故意反序初始化，验证逐字节真拷而非跳写
        for n in [0usize, 1, 7, 8, 9, 16, 17, 32] {
            let mut dst = vec![0xA5u8; 64];
            let s = &src[off..off + n];
            unsafe { memcpy_sem(dst.as_mut_ptr().add(off), s.as_ptr(), n) };
            assert_eq!(&dst[off..off + n], s, "off={off} n={n}");
        }
    }
}

/// memcpy 源内容保持不变（volatile 写不腐蚀读侧）。
#[test]
fn memcpy_preserves_source() {
    let src: Vec<u8> = (0u8..=15).collect();
    let src_copy = src.clone();
    let mut dst = vec![0u8; 16];
    memcpy_sem(dst.as_mut_ptr(), src.as_ptr(), 16);
    assert_eq!(src, src_copy);
}

/// memset：模式字节 ×3 长度族 + 0xFF 全 1 位模式（u64 扩展律）+ 零长。
#[test]
fn memset_fills_pattern_bytes() {
    for val in [0u8, 1, 0xFF, 0x80] {
        for n in [0usize, 1, 7, 8, 9, 24] {
            let mut dst = vec![0x5Au8; 32];
            memset_sem(dst.as_mut_ptr(), val, n);
            assert!(dst[..n].iter().all(|&b| b == val), "val={val} n={n}");
            assert!(dst[n..].iter().all(|&b| b == 0x5A), "tail val={val} n={n}");
        }
    }
}

/// memmove 重叠三态：dst<src 正向、dst>src 回退拷（前向拷必脏——这是
/// memmove 存在的理由）、恰好相邻不重叠。
#[test]
fn memmove_overlap_dst_above_src_backward_copy() {
    // dst > src 重叠：期望 = 先把整段源语义快照后平移
    let mut buf = [0u8; 16];
    buf[..6].copy_from_slice(b"abcdef");
    memmove_sem(unsafe { buf.as_mut_ptr().add(2) }, buf.as_ptr(), 6);
    assert_eq!(&buf[..8], b"ababcdef");

    // dst < src：正向拷，前缀不受写侧影响
    let mut buf2 = [0u8; 16];
    buf2[4..10].copy_from_slice(b"abcdef");
    memmove_sem(buf2.as_mut_ptr(), unsafe { buf2.as_ptr().add(4) }, 6);
    // buf2[4..10]="abcdef" → [0..6)="abcdef"，[6..10) 原值 "cdef" 不动
    assert_eq!(&buf2[..10], b"abcdefcdef");
}

/// memcmp：首差字节无符号差（`'a'-'A'=32` 正差；高位字节 0xFF vs 0x01
/// = 254 正差——c_int 差不截位）+ 等长全等 0 + 零长恒 0。
#[test]
fn memcmp_unsigned_byte_difference() {
    let a = b"abc\0";
    let b = b"abd\0";
    assert_eq!(memcmp_sem(a.as_ptr(), a.as_ptr(), 4), 0);
    assert_eq!(memcmp_sem(a.as_ptr(), b.as_ptr(), 4), b'c' as i32 - b'd' as i32);
    assert_eq!(memcmp_sem(b.as_ptr(), a.as_ptr(), 4), -(b'c' as i32 - b'd' as i32));
    let hi = [0xFFu8, 0x00];
    let lo = [0x01u8, 0x00];
    assert_eq!(memcmp_sem(hi.as_ptr(), lo.as_ptr(), 1), 0xFF - 0x01);
    assert_eq!(memcmp_sem(a.as_ptr(), b.as_ptr(), 0), 0);
}

/// bcmp：与 memcmp 同构转发（musl 同形——非零即不等，符号无承诺）。
#[test]
fn bcmp_matches_memcmp() {
    let a = b"xyz\0";
    let b = b"xya\0";
    assert_eq!(bcmp_sem(a.as_ptr(), b.as_ptr(), 4), memcmp_sem(a.as_ptr(), b.as_ptr(), 4));
    assert_eq!(bcmp_sem(a.as_ptr(), a.as_ptr(), 4), 0);
}

/// strlen/strnlen：NUL 不计、空串 0、strnlen 上帽（帽内无 NUL 返帽、
/// 帽外有 NUL 看帽）。
#[test]
fn strlen_strnlen_boundaries() {
    let s = b"hello\0extra";
    assert_eq!(strlen_sem(s.as_ptr()), 5);
    assert_eq!(strnlen_sem(s.as_ptr(), 5), 5);
    assert_eq!(strnlen_sem(s.as_ptr(), 3), 3);
    assert_eq!(strnlen_sem(s.as_ptr(), 0), 0);
    let empty = b"\0";
    assert_eq!(strlen_sem(empty.as_ptr()), 0);
    assert_eq!(strnlen_sem(empty.as_ptr(), 8), 0);
    // 帽大于实际长度：返实际（min 律）
    assert_eq!(strnlen_sem(s.as_ptr(), 100), 5);
}

/// strcmp：等串 0 / 首差无符号差 / 空串对非空 = 负（0 - 'a'）/
/// 双空 0 / 前缀串对全长串（"ab" vs "abc" = 0 - 'c'）。
#[test]
fn strcmp_lexicographic_and_prefix() {
    let (a, b) = (b"abc\0".as_ptr(), b"abc\0".as_ptr());
    assert_eq!(strcmp_sem(a, b), 0);
    assert_eq!(strcmp_sem(b"abc\0".as_ptr(), b"abd\0".as_ptr()), b'c' as i32 - b'd' as i32);
    assert_eq!(strcmp_sem(b"\0".as_ptr(), b"a\0".as_ptr()), -(b'a' as i32));
    assert_eq!(strcmp_sem(b"\0".as_ptr(), b"\0".as_ptr()), 0);
    assert_eq!(strcmp_sem(b"ab\0".as_ptr(), b"abc\0".as_ptr()), -(b'c' as i32));
    // 高位字节（>0x7F）：无符号差——0x80 vs 0x7F = +1，非符号扩展负差
    assert_eq!(strcmp_sem([0x80, 0].as_ptr(), [0x7F, 0].as_ptr()), 1);
}

/// strchr：首命中偏移 / 未命中 None / c=0 命中 NUL 终结位（C 语义：
/// strchr(s,'\0') 返回指向终结符的指针）。
#[test]
fn strchr_first_hit_and_nul_terminator() {
    let s = b"banana\0";
    assert_eq!(strchr_sem(s.as_ptr(), b'a'), Some(1));
    assert_eq!(strchr_sem(s.as_ptr(), b'n'), Some(2));
    assert_eq!(strchr_sem(s.as_ptr(), b'x'), None);
    assert_eq!(strchr_sem(s.as_ptr(), 0), Some(6)); // NUL 终结位
    assert_eq!(strchr_sem(b"\0".as_ptr(), b'a'), None);
}

/// strrchr：最右命中 / 未命中 None / c=0 恒命中 NUL 位（strlen 位点——
/// 与 strchr 的 c=0 同位共识）。
#[test]
fn strrchr_rightmost_hit_and_nul() {
    let s = b"banana\0";
    assert_eq!(strrchr_sem(s.as_ptr(), b'a'), Some(5));
    assert_eq!(strrchr_sem(s.as_ptr(), b'b'), Some(0));
    assert_eq!(strrchr_sem(s.as_ptr(), b'x'), None);
    assert_eq!(strrchr_sem(s.as_ptr(), 0), Some(6));
    // 单字符重复串的最右 = 长度-1
    assert_eq!(strrchr_sem(b"aaaa\0".as_ptr(), b'a'), Some(3));
}

/// strncpy pad-to-n 律：src 短于 n → 拷 src_len 后**余量整段零填**（C
/// 标准：不是只写一个 NUL）；src ≥ n → 恰拷 n 无零填；返回值 = 拷贝数。
#[test]
fn strncpy_pads_to_exact_n() {
    let mut dst = [0xEEu8; 12];
    let copied = strncpy_sem(dst.as_mut_ptr(), b"ab\0cdef".as_ptr(), 6);
    assert_eq!(copied, 2);
    assert_eq!(&dst[..6], b"ab\0\0\0\0"); // 拷 "ab"（strnlen=2）+ 4 零填
    assert_eq!(&dst[6..], &[0xEE; 6]); // 帽外一字节不动

    // src 前 n 字节无 NUL：恰拷 n，零填 0 字节
    let mut dst2 = [0xEEu8; 8];
    let copied2 = strncpy_sem(dst2.as_mut_ptr(), b"abcdefg".as_ptr(), 4);
    assert_eq!(copied2, 4);
    assert_eq!(&dst2[..4], b"abcd");
    assert_eq!(&dst2[4..], &[0xEE; 4]);

    // n=0：零操作
    let mut dst3 = [0xEEu8; 2];
    assert_eq!(strncpy_sem(dst3.as_mut_ptr(), b"x".as_ptr(), 0), 0);
    assert_eq!(dst3, [0xEE; 2]);
}

/// atoi 全谱：正/负/前导空格/一元符号/无数字 0/仅符号 0/截断尾垃圾/
/// 溢出双饱和（INT_MAX/INT_MIN——i64 中转 + 饱和点 MAX+1 单调夹）。
#[test]
fn atoi_sign_space_garbage_and_saturation() {
    let f = |s: &[u8]| -> i32 { atoi_sem(s.as_ptr()) };
    assert_eq!(f(b"42\0"), 42);
    assert_eq!(f(b"-42\0"), -42);
    assert_eq!(f(b"   7\0"), 7); // 前导空格仅 ' '
    assert_eq!(f(b"+13\0"), 13);
    assert_eq!(f(b"12abc\0"), 12); // 尾垃圾截断
    assert_eq!(f(b"abc\0"), 0); // 无数字
    assert_eq!(f(b"-\0"), 0); // 仅符号
    assert_eq!(f(b"+\0"), 0);
    assert_eq!(f(b"0\0"), 0);
    assert_eq!(f(b"2147483647\0"), i32::MAX); // 恰 MAX 不夹
    assert_eq!(f(b"2147483648\0"), i32::MAX); // MAX+1 饱和上夹
    assert_eq!(f(b"99999999999999\0"), i32::MAX); // 远溢出
    assert_eq!(f(b"-2147483648\0"), i32::MIN); // 恰 MIN 不夹
    assert_eq!(f(b"-2147483649\0"), i32::MIN); // MIN-1 饱和下夹
    assert_eq!(f(b"-99999999999999\0"), i32::MIN);
}

/// atoi 溢出夹紧的中转数学：饱和点在 i64 域取 `i32::MAX as i64 + 1`
/// （正负对称）——负串夹到 MIN 是 `-（MAX+1）= MIN` 恰好，无需独立
/// 负饱和逻辑。钉板：i64 饱和值的负 = i32::MIN（位形自证）。
#[test]
fn atoi_saturation_point_math() {
    let sat = i32::MAX as i64 + 1;
    assert_eq!((-sat) as i32, i32::MIN);
    // 与 atoi 端到端共识：-2147483648 走 -sat 路
    assert_eq!(atoi_sem(b"-2147483648\0".as_ptr()), (-sat) as i32);
}

/// byte_at：volatile 读底座直钉（memcpy/memcmp 等全族的读侧单源）。
#[test]
fn byte_at_reads_offset() {
    let s = b"ABC\0";
    assert_eq!(byte_at_sem(s.as_ptr(), 0), b'A');
    assert_eq!(byte_at_sem(s.as_ptr(), 3), 0);
}

/// u64 快路对齐门槛：src/dst 任一未对齐即全程逐字节（语义等价、路径
/// 不同）——未对齐 buffer 上 24 字节拷贝与对齐 buffer 结果逐字节相等。
#[test]
fn memcpy_unaligned_falls_back_identical_result() {
    let src: Vec<u8> = (0u8..32).collect();
    let mut aligned = vec![0u8; 40];
    let mut unaligned = vec![0u8; 40];
    memcpy_sem(aligned.as_mut_ptr(), src.as_ptr(), 24);
    memcpy_sem(unsafe { unaligned.as_mut_ptr().add(3) }, unsafe { src.as_ptr().add(1) }, 24);
    assert_eq!(&aligned[..24], &src[..24]);
    assert_eq!(&unaligned[3..27], &src[1..25]);
}
