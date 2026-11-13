//! berry-memface host 钉板（W3-Q80）——块尾算法语义 vs 逐字节参考模型
//! 全量对拍 + 切分几何/方向分治/首异定位独立锚。冻结锚与 kernel mem.rs
//! 注释互指（GUI-0 SIMD-free 合同 builtins 半边）。

use berry_memface::*;

// ---- 切分几何 ----

#[test]
fn block_model_table() {
    assert_eq!(block_model(0), Some((0, 0)));
    assert_eq!(block_model(1), Some((0, 1)));
    assert_eq!(block_model(7), Some((0, 7)));
    assert_eq!(block_model(8), Some((1, 0)));
    assert_eq!(block_model(9), Some((1, 1)));
    assert_eq!(block_model(4096), Some((512, 0)));
    assert_eq!(block_model(4097), Some((512, 1)));
    // 全域段和恰等不变量：blocks*8 + tail == n。
    for n in 0..200usize {
        let (b, t) = block_model(n).unwrap();
        assert_eq!(b * 8 + t, n);
    }
}

#[test]
fn fill_word_law() {
    assert_eq!(fill_word(0), 0);
    assert_eq!(fill_word(0xFF), u64::MAX);
    assert_eq!(fill_word(0x01), 0x0101_0101_0101_0101);
    // 256 值全域：每字节位组相等（广播律）。
    for c in 0..=255u8 {
        let w = fill_word(c);
        for i in 0..8 {
            assert_eq!(((w >> (i * 8)) & 0xFF) as u8, c);
        }
    }
}

// ---- 首异定位（LE 升序 = memcmp 符号语义）----

#[test]
fn cmp_first_diff_law() {
    assert_eq!(cmp_first_diff(0, 0), None);
    assert_eq!(cmp_first_diff(u64::MAX, u64::MAX), None);
    // 首异在最低字节：0x02 vs 0x01 → (0, 2, 1)。
    assert_eq!(cmp_first_diff(2, 1), Some((0, 2, 1)));
    // 高位相等低位异：块内升序定位。
    assert_eq!(cmp_first_diff(0x0000_0000_0000_0100, 0x0000_0000_0000_0000), Some((1, 1, 0)));
    // 低字节相等高字节异：首异在 index 7。
    assert_eq!(cmp_first_diff(0x0100_0000_0000_0000, 0x0000_0000_0000_0000), Some((7, 1, 0)));
    // 符号语义对拍：cmp_first_diff 差 = memcmp 返回差的符号面。
    let wa = 0xAB_CD_00_11_22_33_44_55u64;
    let wb = 0xAB_CD_00_11_22_33_44_99u64;
    let (i, xa, xb) = cmp_first_diff(wa, wb).unwrap();
    assert_eq!(i, 0); // LE：LSB（0x55 vs 0x99）即 index 0 首异。
    assert_eq!(xa as i32 - xb as i32, -0x44);
}

// ---- 方向分治 ----

#[test]
fn move_split_law() {
    assert_eq!(move_split(100, 100), (false, true)); // 同址恒等
    assert_eq!(move_split(50, 100), (false, false)); // 前向正拷
    assert_eq!(move_split(100, 50), (true, false)); // 后向逆拷
    // 边界：邻接（重叠 1 字节）两向。
    assert_eq!(move_split(9, 8), (true, false));
    assert_eq!(move_split(8, 9), (false, false));
}

// ---- 模型族对拍（mem.rs 块算法的语义等价证明）----

/// 块算法重建（与 kernel mem.rs 逐字同构——用安全切片原语替代 unsafe
/// unaligned 读写；对拍两模型逐字节一致）。
fn memset_blocks(dst: &mut [u8], c: u8, n: usize) {
    let n = n.min(dst.len());
    let word = fill_word(c).to_le_bytes();
    let (blocks, tail) = block_model(n).unwrap();
    let mut off = 0;
    for _ in 0..blocks {
        dst[off..off + 8].copy_from_slice(&word);
        off += 8;
    }
    for _ in 0..tail {
        dst[off] = c;
        off += 1;
    }
}

fn memcmp_blocks(a: &[u8], b: &[u8], n: usize) -> i32 {
    let n = n.min(a.len()).min(b.len());
    let (blocks, tail) = block_model(n).unwrap();
    let mut off = 0;
    for _ in 0..blocks {
        let wa = u64::from_le_bytes(a[off..off + 8].try_into().unwrap());
        let wb = u64::from_le_bytes(b[off..off + 8].try_into().unwrap());
        if wa != wb {
            if let Some((_, xa, xb)) = cmp_first_diff(wa, wb) {
                return xa as i32 - xb as i32;
            }
        }
        off += 8;
    }
    for _ in 0..tail {
        if a[off] != b[off] {
            return a[off] as i32 - b[off] as i32;
        }
        off += 1;
    }
    0
}

#[test]
fn memset_model_equiv_blocks() {
    let mut a = [0u8; 64];
    let mut b = [0u8; 64];
    for n in 0..=64usize {
        for c in [0u8, 0xFF, 0x5A, 0x01] {
            a.fill(0xEE);
            b.fill(0xEE);
            memset_blocks(&mut a, c, n);
            memset_model(&mut b, c, n);
            assert_eq!(a, b, "n={n} c={c:#x}");
        }
    }
}

#[test]
fn memcmp_model_equiv_blocks() {
    // 全 0 vs 递增、随机形（LCG）、恰异在块界/尾界的扫描。
    let mut a = [0u8; 64];
    let mut b = [0u8; 64];
    let mut seed = 0x1234_5678u64;
    for i in 0..64 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        a[i] = (seed >> 33) as u8;
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        b[i] = (seed >> 33) as u8;
    }
    for n in 0..=64usize {
        assert_eq!(memcmp_blocks(&a, &b, n), memcmp_model(&a, &b, n), "rand n={n}");
    }
    // 恰界差异：i 位异（其余同）。
    for i in 0..64 {
        let mut c = a;
        c[i] = c[i].wrapping_add(1);
        for n in 0..=64usize {
            assert_eq!(memcmp_blocks(&a, &c, n), memcmp_model(&a, &c, n), "diff@{i} n={n}");
        }
    }
    // 全等面：符号 0。
    assert_eq!(memcmp_blocks(&a, &a, 64), 0);
}

#[test]
fn memmove_model_overlap_all_windows() {
    // 同一缓冲内搬移（重叠全窗扫描）：模型（方向分治逐字节）vs Rust
    // 语义 copy_within（memmove 语义真值）。
    let base: Vec<u8> = (0..64u8).collect();
    for src in 0..48usize {
        for dst in 0..48usize {
            for n in 1..=(64 - src.max(dst)).min(16) {
                let mut m = base.clone();
                memmove_model(&mut m, src, dst, n.min(64 - src).min(64 - dst));
                let mut r = base.clone();
                let n = n.min(64 - src).min(64 - dst);
                r.copy_within(src..src + n, dst);
                assert_eq!(m, r, "src={src} dst={dst} n={n}");
            }
        }
    }
}

#[test]
fn memcpy_model_forward_only() {
    // 不重叠合同面：前向搬移与逐字节一致。
    let src: Vec<u8> = (0..32u8).collect();
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    for n in 0..=32usize {
        memset_model(&mut a, 0, 32);
        memset_model(&mut b, 0, 32);
        memcpy_model(&mut a, &src, n);
        for i in 0..n {
            b[i] = src[i];
        }
        assert_eq!(a, b, "n={n}");
    }
}

// ---- 冻结锚（kernel mem.rs 注释/常量互指）----

#[test]
fn frozen_anchors() {
    // 8B 块宽（GUI-0「TCG 下数量级足够——4KiB 零填 ≈ 512 块迭代」）。
    assert_eq!(BLOCK, 8);
    // bcmp = memcmp 伴生（相等面 0/差异面非 0——符号语义不作承诺）；本
    // crate 不独立建 bcmp 模型，符号语义复用 memcmp_model（kernel 侧
    // bcmp 直接调 memset.rs 的 memcmp 同款）。
    let a = [1u8, 2, 3];
    let b = [1u8, 2, 4];
    assert_eq!(memcmp_model(&a, &a, 3), 0);
    assert_ne!(memcmp_model(&a, &b, 3), 0);
}
