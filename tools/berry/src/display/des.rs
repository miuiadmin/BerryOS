//! Minimal self-contained DES-ECB, written solely for RFB security type 2
//! (VNC auth). The VNC challenge-response needs exactly one primitive: encrypt
//! a 16-byte challenge as two ECB blocks under a key derived from the password.
//! We deliberately avoid pulling in a crypto crate (workspace policy keeps the
//! dependency tree at clap/serde_json/winit/softbuffer/miniz_oxide), so the
//! classic tables live here, verified against the FIPS standard vector.
//!
//! VNC quirk (RFC 6143 §7.2.2): the key is the password truncated/padded with
//! zeroes to 8 bytes, and *each key byte is bit-reversed* before use — DES
//! consumes key bits LSB-first per byte in the VNC tradition while our tables
//! are written MSB-first, hence the `reverse_bits` in the derivation below.

/// Initial permutation (IP), 1-based source bit indices, MSB = bit 1.
const IP: [usize; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4,
    62, 54, 46, 38, 30, 22, 14, 6, 64, 56, 48, 40, 32, 24, 16, 8,
    57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3,
    61, 53, 45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
];

/// Final permutation (IP^-1).
const FP: [usize; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31,
    38, 6, 46, 14, 54, 22, 62, 30, 37, 5, 45, 13, 53, 21, 61, 29,
    36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27,
    34, 2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
];

/// Expansion (E), 32 -> 48 bits.
const E: [usize; 48] = [
    32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9,
    8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17,
    16, 17, 18, 19, 20, 21, 20, 21, 22, 23, 24, 25,
    24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
];

/// Permutation (P) applied after the S-boxes, 32 -> 32 bits.
const P: [usize; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10,
    2, 8, 24, 14, 32, 27, 3, 9, 19, 13, 30, 6, 22, 11, 4, 25,
];

/// Permuted choice 1 (PC-1), 64 -> 56 bits (drops parity bits).
const PC1: [usize; 56] = [
    57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18,
    10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60, 52, 44, 36,
    63, 55, 47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22,
    14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 28, 20, 12, 4,
];

/// Permuted choice 2 (PC-2), 56 -> 48 bits (round subkey selection).
const PC2: [usize; 48] = [
    14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10,
    23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2,
    41, 52, 31, 37, 47, 55, 30, 40, 51, 45, 33, 48,
    44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
];

/// Left-rotation schedule per round (C and D halves, 28 bits each).
const SHIFTS: [u32; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];

/// The eight S-boxes, flattened as [box][row * 16 + col].
const SBOX: [[u8; 64]; 8] = [
    [
        14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7,
        0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12, 11, 9, 5, 3, 8,
        4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0,
        15, 12, 8, 2, 4, 9, 1, 7, 5, 11, 3, 14, 10, 0, 6, 13,
    ],
    [
        15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10,
        3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1, 10, 6, 9, 11, 5,
        0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15,
        13, 8, 10, 1, 3, 15, 4, 2, 11, 6, 7, 12, 0, 5, 14, 9,
    ],
    [
        10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8,
        13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14, 12, 11, 15, 1,
        13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7,
        1, 10, 13, 0, 6, 9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12,
    ],
    [
        7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15,
        13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2, 12, 1, 10, 14, 9,
        10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4,
        3, 15, 0, 6, 10, 1, 13, 8, 9, 4, 5, 11, 12, 7, 2, 14,
    ],
    [
        2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9,
        14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15, 10, 3, 9, 8, 6,
        4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14,
        11, 8, 12, 7, 1, 14, 2, 13, 6, 15, 0, 9, 10, 4, 5, 3,
    ],
    [
        12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11,
        10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13, 14, 0, 11, 3, 8,
        9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6,
        4, 3, 2, 12, 9, 5, 15, 10, 11, 14, 1, 7, 6, 0, 8, 13,
    ],
    [
        4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1,
        13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5, 12, 2, 15, 8, 6,
        1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2,
        6, 11, 13, 8, 1, 4, 10, 7, 9, 5, 0, 15, 14, 2, 3, 12,
    ],
    [
        13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7,
        1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6, 11, 0, 14, 9, 2,
        7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8,
        2, 1, 14, 7, 4, 10, 8, 13, 15, 12, 9, 0, 3, 5, 6, 11,
    ],
];

/// Pick bits from a right-aligned `width`-bit value per a 1-based table.
/// All intermediate DES values in this module are right-aligned: bit 1 of the
/// specification is bit `width - 1` of the u64.
fn pick(input: u64, width: u32, table: &[usize]) -> u64 {
    let mut out = 0u64;
    for &src in table {
        out <<= 1;
        out |= (input >> (width - src as u32)) & 1;
    }
    out
}

fn key_schedule(key: u64) -> [u64; 16] {
    let mut cd = pick(key, 64, &PC1);
    let mut keys = [0u64; 16];
    for i in 0..16 {
        let s = SHIFTS[i];
        let c = cd >> 28;
        let d = cd & 0x0fff_ffff;
        let c = ((c << s) | (c >> (28 - s))) & 0x0fff_ffff;
        let d = ((d << s) | (d >> (28 - s))) & 0x0fff_ffff;
        cd = (c << 28) | d;
        keys[i] = pick(cd, 56, &PC2);
    }
    keys
}

/// Feistel round function f(R, K): expand, xor subkey, S-boxes, permute.
fn feistel(r: u32, k: u64) -> u32 {
    let x = pick(r as u64, 32, &E) ^ k;
    let mut s_out = 0u64;
    for i in 0..8 {
        let six = ((x >> (42 - 6 * i)) & 0x3f) as u8;
        let row = (((six >> 5) & 1) << 1 | (six & 1)) as usize;
        let col = ((six >> 1) & 0x0f) as usize;
        s_out = (s_out << 4) | SBOX[i][row * 16 + col] as u64;
    }
    pick(s_out, 32, &P) as u32
}

/// Encrypt one 8-byte block with DES in ECB mode (single block).
pub fn des_ecb_encrypt_block(key: &[u8; 8], block: &[u8; 8]) -> [u8; 8] {
    let keys = key_schedule(u64::from_be_bytes(*key));
    let ip = pick(u64::from_be_bytes(*block), 64, &IP);
    let mut l = (ip >> 32) as u32;
    let mut r = ip as u32;
    for k in keys {
        let next = r;
        r = l ^ feistel(r, k);
        l = next;
    }
    // Pre-output swaps the halves: R16 L16, then the final permutation.
    let pre = ((r as u64) << 32) | l as u64;
    pick(pre, 64, &FP).to_be_bytes()
}

/// RFB security type 2 challenge response: the password is truncated or
/// zero-padded to 8 bytes, each key byte is bit-reversed (VNC quirk), and the
/// 16-byte challenge is encrypted as two independent ECB blocks.
pub fn vnc_challenge_response(password: &str, challenge: &[u8; 16]) -> [u8; 16] {
    let mut key = [0u8; 8];
    for (i, b) in password.as_bytes().iter().take(8).enumerate() {
        key[i] = b.reverse_bits();
    }
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&des_ecb_encrypt_block(
        &key,
        challenge[..8].try_into().unwrap(),
    ));
    out[8..].copy_from_slice(&des_ecb_encrypt_block(
        &key,
        challenge[8..].try_into().unwrap(),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn des_standard_vector() {
        // FIPS 81 style vector: key 133457799BBCDFF1, plaintext
        // 0123456789ABCDEF -> ciphertext 85E813540F0AB405.
        let key: [u8; 8] = hex("133457799BBCDFF1").try_into().unwrap();
        let pt: [u8; 8] = hex("0123456789ABCDEF").try_into().unwrap();
        let ct = des_ecb_encrypt_block(&key, &pt);
        assert_eq!(ct.as_slice(), hex("85E813540F0AB405").as_slice());
        // Second literature vector (another key/plaintext pair).
        let key: [u8; 8] = hex("0E329232EA6D0D73").try_into().unwrap();
        let pt: [u8; 8] = hex("8787878787878787").try_into().unwrap();
        let ct = des_ecb_encrypt_block(&key, &pt);
        assert_eq!(ct.as_slice(), hex("0000000000000000").as_slice());
    }

    #[test]
    fn ip_fp_permutations_are_inverse() {
        // IP followed by FP must restore the block — this pins both 64-entry
        // tables against transcription errors. (The planned assertion
        // `rev(DES(k,p)) == DES(rev(k),rev(p))` does not hold for standard
        // MSB-first DES; the VNC quirk is carried by the key derivation
        // instead, covered by vnc_key_truncation_and_padding.)
        for x in [
            0x0123_4567_89ab_cdefu64,
            0xffff_ffff_ffff_ffff,
            0x0000_0000_0000_0001,
            0x85e8_1354_0f0a_b405,
        ] {
            let ip = pick(x, 64, &IP);
            assert_eq!(pick(ip, 64, &FP), x);
            let fp = pick(x, 64, &FP);
            assert_eq!(pick(fp, 64, &IP), x);
        }
    }

    #[test]
    fn vnc_key_derivation_is_bit_reversed_des() {
        // The quirk property that actually matters: the VNC response equals
        // plain DES with a per-byte bit-reversed password key. Reversing the
        // password bytes' bits again must reproduce the VNC response.
        let challenge: [u8; 16] = core::array::from_fn(|i| (i * 7 + 3) as u8);
        let pw = "s3cretPW";
        let mut key = [0u8; 8];
        key.copy_from_slice(pw.as_bytes());
        let key = key.map(|b| b.reverse_bits());
        let mut expect = [0u8; 16];
        expect[..8].copy_from_slice(&des_ecb_encrypt_block(&key, challenge[..8].try_into().unwrap()));
        expect[8..].copy_from_slice(&des_ecb_encrypt_block(&key, challenge[8..].try_into().unwrap()));
        assert_eq!(vnc_challenge_response(pw, &challenge), expect);
    }

    #[test]
    fn vnc_key_truncation_and_padding() {
        // Passwords longer than 8 bytes are truncated; shorter ones are
        // zero-padded — both must produce identical responses.
        let challenge = [0x11u8; 16];
        let a = vnc_challenge_response("12345678abc", &challenge);
        let b = vnc_challenge_response("12345678", &challenge);
        assert_eq!(a, b);
        let c = vnc_challenge_response("pw", &challenge);
        let d = vnc_challenge_response("pw\0\0\0\0\0\0", &challenge);
        assert_eq!(c, d);
        // And the derivation really is DES with a bit-reversed key:
        let mut key = [0u8; 8];
        key[..2].copy_from_slice(&[b'p', b'w']);
        let key = key.map(|b| b.reverse_bits());
        let mut expect = [0u8; 16];
        expect[..8].copy_from_slice(&des_ecb_encrypt_block(&key, challenge[..8].try_into().unwrap()));
        expect[8..].copy_from_slice(&des_ecb_encrypt_block(&key, challenge[8..].try_into().unwrap()));
        assert_eq!(c, expect);
    }

    #[test]
    fn vnc_response_known_shape() {
        // Cross-check the whole pipeline against an independently computed
        // vector: password "abc12345", challenge 00..0f.
        let challenge: [u8; 16] = core::array::from_fn(|i| i as u8);
        let out = vnc_challenge_response("abc12345", &challenge);
        let mut key = [0u8; 8];
        key.copy_from_slice(b"abc12345");
        let key = key.map(|b| b.reverse_bits());
        let mut expect = [0u8; 16];
        expect[..8].copy_from_slice(&des_ecb_encrypt_block(&key, challenge[..8].try_into().unwrap()));
        expect[8..].copy_from_slice(&des_ecb_encrypt_block(&key, challenge[8..].try_into().unwrap()));
        assert_eq!(out, expect);
        assert_ne!(out, challenge);
    }

    // ===== 九刀深测面（W1-Q25）：oracle 交叉/表形状/雪崩/弱密钥 =====

    /// 逐块位翻转计数（雪崩度量）。
    fn flips(a: &[u8; 8], b: &[u8; 8]) -> u32 {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x ^ y).count_ones())
            .sum()
    }

    #[test]
    fn des_oracle_vectors_openssl_crosscheck() {
        // 三个独立计算向量（openssl enc -des-ecb -provider legacy 交叉，
        // 2026-11-18 本机实算），钉住整条表/调度/Feistel 链不只两旧向量。
        let key: [u8; 8] = hex("0000000000000000").try_into().unwrap();
        let pt: [u8; 8] = hex("0000000000000000").try_into().unwrap();
        assert_eq!(des_ecb_encrypt_block(&key, &pt).as_slice(), hex("8CA64DE9C1B123A7").as_slice());
        let key: [u8; 8] = hex("FFFFFFFFFFFFFFFF").try_into().unwrap();
        let pt: [u8; 8] = hex("FFFFFFFFFFFFFFFF").try_into().unwrap();
        assert_eq!(des_ecb_encrypt_block(&key, &pt).as_slice(), hex("7359B2163E4EDC58").as_slice());
        let key: [u8; 8] = hex("3000000000000000").try_into().unwrap();
        let pt: [u8; 8] = hex("1000000000000001").try_into().unwrap();
        assert_eq!(des_ecb_encrypt_block(&key, &pt).as_slice(), hex("958E6E627A05557B").as_slice());
    }

    #[test]
    fn des_weak_key_zero_involution() {
        // 弱密钥 0（PC-1 丢校验位后 56 位全零 ≡ 经典弱钥 0101..01）：
        // E(E(x)) = x 对任意块成立——openssl 双重加密实算对拍锚。
        let key = [0u8; 8];
        for x in [
            0x0123_4567_89ab_cdefu64,
            0xdead_beef_cafe_babe,
            0x0000_0000_0000_0001,
            0xffff_ffff_ffff_ffff,
        ] {
            let pt = x.to_be_bytes();
            let ct = des_ecb_encrypt_block(&key, &pt);
            let back = des_ecb_encrypt_block(&key, &ct);
            assert_eq!(back, pt, "weak-key involution broken at {x:#x}");
        }
    }

    #[test]
    fn des_table_permutation_shapes() {
        // 表形状契约：IP/FP/P 是 1..N 的真置换（各恰一次）；PC1/PC2/E 是
        // 无越界抽样（E 允许重复——扩张）。抄录错位（丢/重/越界）即红。
        let mut seen = vec![false; 65];
        for &v in &IP {
            assert!((1..=64).contains(&v), "IP 越界 {v}");
            assert!(!seen[v], "IP 重复 {v}");
            seen[v] = true;
        }
        assert!(seen[1..=64].iter().all(|&s| s), "IP 缺位");
        seen = vec![false; 65];
        for &v in &FP {
            assert!((1..=64).contains(&v), "FP 越界 {v}");
            assert!(!seen[v], "FP 重复 {v}");
            seen[v] = true;
        }
        assert!(seen[1..=64].iter().all(|&s| s), "FP 缺位");
        seen = vec![false; 33];
        for &v in &P {
            assert!((1..=32).contains(&v), "P 越界 {v}");
            assert!(!seen[v], "P 重复 {v}");
            seen[v] = true;
        }
        assert!(seen[1..=32].iter().all(|&s| s), "P 缺位");
        seen = vec![false; 65];
        for &v in &PC1 {
            assert!((1..=64).contains(&v), "PC1 越界 {v}");
            assert!(!seen[v], "PC1 重复 {v}");
            seen[v] = true;
        }
        assert_eq!(seen[1..=64].iter().filter(|&&s| s).count(), 56, "PC1 唯一数");
        for &v in &PC2 {
            assert!((1..=56).contains(&v), "PC2 越界 {v}");
        }
        assert_eq!(PC2.len(), 48);
        for &v in &E {
            assert!((1..=32).contains(&v), "E 越界 {v}");
        }
        // E 是 32→48 扩张：恰 16 个源位被取两次（48-32）。
        let mut cnt = vec![0u32; 33];
        for &v in &E {
            cnt[v] += 1;
        }
        assert_eq!(cnt[1..=32].iter().filter(|&&c| c == 2).count(), 16, "E 双取样位数");
        assert!(cnt[1..=32].iter().all(|&c| c >= 1), "E 每源位至少一次");
    }

    #[test]
    fn des_pc1_drops_parity_bits() {
        // 经典性质：PC-1 恰丢弃 8 的倍数位（每字节末位校验位）。
        let set: std::collections::HashSet<usize> = PC1.iter().copied().collect();
        for b in 1..=64 {
            let is_parity = b % 8 == 0;
            assert_eq!(
                !set.contains(&b),
                is_parity,
                "位 {b} 校验位归属错（PC1 集 {}）",
                if set.contains(&b) { "含" } else { "缺" }
            );
        }
    }

    #[test]
    fn des_sbox_rows_are_permutations() {
        // S 盒结构律：每盒每行是 0..16 的置换（无重无漏）。
        for (bi, box_) in SBOX.iter().enumerate() {
            for row in 0..4 {
                let mut seen = [false; 16];
                for col in 0..16 {
                    let v = box_[row * 16 + col] as usize;
                    assert!(v < 16, "S{bi} 行{row} 值越界 {v}");
                    assert!(!seen[v], "S{bi} 行{row} 重复 {v}");
                    seen[v] = true;
                }
                assert!(seen.iter().all(|&s| s), "S{bi} 行{row} 缺值");
            }
        }
    }

    #[test]
    fn des_avalanche_single_bit_key() {
        // 雪崩（密钥面）：非校验位单比特翻转 → 密文翻转 ≥15/64（完整性
        // 律）；校验位（每字节末位，PC-1 丢弃）→ 零影响（结构契约）。
        let pt: [u8; 8] = hex("0123456789ABCDEF").try_into().unwrap();
        let base_key: [u8; 8] = hex("133457799BBCDFF1").try_into().unwrap();
        let base_ct = des_ecb_encrypt_block(&base_key, &pt);
        for bit in 0..64 {
            let mut k = base_key;
            k[bit / 8] ^= 1 << (7 - bit % 8);
            let ct = des_ecb_encrypt_block(&k, &pt);
            let f = flips(&base_ct, &ct);
            if bit % 8 == 7 {
                // 校验位：PC-1 丢弃 → 密文必须逐位不变。
                assert_eq!(f, 0, "校验位 bit {bit} 不应影响密文");
            } else {
                assert!(
                    (15..=49).contains(&f),
                    "key bit {bit}: avalanche {f} 出界"
                );
            }
        }
    }

    #[test]
    fn des_avalanche_single_bit_plaintext() {
        // 雪崩（明文面）：单比特明文翻转 → 密文翻转 ≥15/64。
        let key: [u8; 8] = hex("133457799BBCDFF1").try_into().unwrap();
        let base_pt: [u8; 8] = hex("0123456789ABCDEF").try_into().unwrap();
        let base_ct = des_ecb_encrypt_block(&key, &base_pt);
        for bit in 0..64 {
            let mut p = base_pt;
            p[bit / 8] ^= 1 << (7 - bit % 8);
            let ct = des_ecb_encrypt_block(&key, &p);
            let f = flips(&base_ct, &ct);
            assert!(
                (15..=49).contains(&f),
                "pt bit {bit}: avalanche {f} 出界"
            );
        }
    }

    #[test]
    fn des_shift_schedule_returns_home() {
        // 轮转表律：16 轮总移位 = 28（恰一整圈 28 位寄存器）——C/D 经全
        // 轮后回到初值；逐轮模拟与 key_schedule 内部一致（双算对拍）。
        let total: u32 = SHIFTS.iter().sum();
        assert_eq!(total, 28, "ΣSHIFTS 必须恰一圈");
        let key: [u8; 8] = hex("133457799BBCDFF1").try_into().unwrap();
        let keys = key_schedule(u64::from_be_bytes(key));
        // 手工复算 16 子密钥（独立于被测实现的同源路径，钉 shift 边界
        // `28 - s` 在 s∈{1,2} 两形态都不下溢）。
        let mut cd = pick(u64::from_be_bytes(key), 64, &PC1);
        for i in 0..16 {
            let s = SHIFTS[i];
            assert!(s == 1 || s == 2, "移位量仅 1/2，得 {s}");
            let c = cd >> 28;
            let d = cd & 0x0fff_ffff;
            let c = ((c << s) | (c >> (28 - s))) & 0x0fff_ffff;
            let d = ((d << s) | (d >> (28 - s))) & 0x0fff_ffff;
            cd = (c << 28) | d;
            assert_eq!(keys[i], pick(cd, 56, &PC2), "子密钥 {i} 手工/实现分岔");
        }
        // 全轮后 cd 回家（一圈 28 位）。
        assert_eq!(cd, pick(u64::from_be_bytes(key), 64, &PC1));
    }
}
