//! Ed25519 PureEdDSA（RFC 8032 §5.1.6 sign / §5.1.7 verify）——公开 API。
//!
//! P0-2 主刀出口（24 号 §3.2 列名 ed25519.rs）。**verify 优先**（设备/
//! 工具链验签是 SDK-2 第一消费面），sign 供平台签发（P1 mkapp 消费——
//! 任务书口径）。
//!
//! **verify 语义注（如实）**：
//! - 步 1 严格解析：A'/R 解压缩失败拒（y≥p / 非平方 / x=0∧sign，见
//!   [`crate::ge`]）；S 段 `0 ≤ S < L` 越界拒（RFC 8032 §3.4「rejected
//!   if parsing fails (including S being out of range)」）。
//! - 步 3 群方程取 **cofactorless** 检查 `[S]B == R + [k]A'`——RFC 8032
//!   §5.1.7 明示「It's sufficient, but not required, to instead check
//!   [8S][B] = [8R] + [8k][A']」：cofactorless 为其**充分**替代（更严
//!   ——该「更严」只对小群**残差**分量成立：非规范 R'（+小群分量）在
//!   8· 形下或平而在本形下拒；对与混阶公钥方程**精确一致**的签名
//!   （Wycheproof acceptable 例）不覆盖——29 号 §F1 实测，故步 1.5
//!   另设子群检查）；与 RFC 语义对齐如实注记。
//! - 步 1.5 公钥子群检查（strict torsion 拒收，29 号 §F1 修正）：
//!   [8]A==O 拒（小阶/identity 键）+ [L]A≠O 拒（混阶键）——小阶/
//!   混阶公钥拒收战役随 Wycheproof 210 门本刀合流执法（26 号 §3-2）。
//! - dom2 = 空串（PureEdDSA，Ed25519 无 context）。
//!
//! **sign 语义注**：按 §5.1.6 确定性 nonce（r = SHA-512(prefix‖M) mod L
//! ——RFC 8032 §8.2 确定性随机性）；公钥 A 由 sk 现场重导（不接收外部
//! pk——防 sk/pk 误配对，平台签发面）。
//!
//! **常时性声明（诚实化，不过度声称）**：纯 Rust 软实现，常数时间
//! **风格**（秘密相关分支为零：标量位经掩码选择、约减条件减经掩码；
//! 解压缩/S 越界检查分支于公开输入。**唯一例外**：`sc_muladd` 尾进位
//! 传播 while 是秘密依赖分支——量级/口径见 sc.rs F2 注记，29 号 §F2），
//! **未做侧信道审计，不用于在线敌手环境**（设备侧敌手面随 P3 评估，
//! 24 号 §3.9-1）。

use crate::ge::Ge;
use crate::sc::{sc_is_less_l, sc_muladd, sc_reduce};
use crate::sha512::{digest, Sha512};

/// 私钥展宽（RFC 8032 §5.1.5）：h = SHA-512(sk)；a = clamp(h[0..32])，
/// prefix = h[32..64]。
fn expand(sk: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let h = digest(sk);
    let mut a = [0u8; 32];
    a.copy_from_slice(&h[..32]);
    // clamp：清 bit0..2，置 bit254，清 bit255（RFC 8032 §5.1.5）。
    a[0] &= 0xf8;
    a[31] &= 0x7f;
    a[31] |= 0x40;
    let mut prefix = [0u8; 32];
    prefix.copy_from_slice(&h[32..]);
    (a, prefix)
}

/// 私钥 → 公钥（A = compress([a]B)——RFC 8032 §5.1.5；P1 mkapp/平台
/// 签发消费面）。
pub fn public_key(sk: &[u8; 32]) -> [u8; 32] {
    let (a, _) = expand(sk);
    Ge::BASE.scalar_mul(&a).compress()
}

/// 签名（RFC 8032 §5.1.6，PureEdDSA/dom2 空）：出 R‖S 64B。
pub fn sign(sk: &[u8; 32], msg: &[u8]) -> [u8; 64] {
    let (a, prefix) = expand(sk);
    let a_enc = Ge::BASE.scalar_mul(&a).compress();
    // r = SHA-512(prefix‖M) mod L。
    let mut h = Sha512::new();
    h.update(&prefix);
    h.update(msg);
    let mut hs = [0u8; 64];
    h.finalize(&mut hs);
    let r = sc_reduce(&hs);
    // R = [r]B。
    let r_point = Ge::BASE.scalar_mul(&r);
    let r_enc = r_point.compress();
    // k = SHA-512(R‖A‖M) mod L。
    let mut h = Sha512::new();
    h.update(&r_enc);
    h.update(&a_enc);
    h.update(msg);
    h.finalize(&mut hs);
    let k = sc_reduce(&hs);
    // S = (r + k·a) mod L——sc_muladd(a,b,c) = a·b+c。
    let s = sc_muladd(&k, &a, &r);
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&r_enc);
    sig[32..].copy_from_slice(&s);
    sig
}

/// 验签（RFC 8032 §5.1.7，严格 + cofactorless——模块头语义注）。
pub fn verify(pk: &[u8; 32], sig: &[u8; 64], msg: &[u8]) -> bool {
    let mut r_enc = [0u8; 32];
    r_enc.copy_from_slice(&sig[..32]);
    let mut s = [0u8; 32];
    s.copy_from_slice(&sig[32..]);
    // 步 1：三方解析，任一失败即拒。
    let a_point = match Ge::decompress(pk) {
        Some(p) => p,
        None => return false,
    };
    let r_point = match Ge::decompress(&r_enc) {
        Some(p) => p,
        None => return false,
    };
    if !sc_is_less_l(&s) {
        return false; // S ≥ L：越界拒
    }
    // 步 1.5：公钥子群检查（torsion 拒收——F1 修正，29 号 §F1/§5 补救面）。
    // 小阶/混阶公钥即使验签方程精确成立也拒（Wycheproof acceptable 29 条
    // strict 语义，26 号 §3-2）。两臂合围（29 号实测分类为准）：
    // - [8]A == O 拒：A ∈ 8-torsion——小阶 8 条全中（含 identity 键；
    //   [L] 形对 identity 失明——[L]O == O，故此臂不可省）；
    // - [L]A != O 拒：A 不在素阶子群 <B>——混阶 21 条全中（A = A_p + T
    //   时 [L]A = [L]T ≠ O；[8]A = [8]A_p ≠ O 单臂**不**收混阶——
    //   29 号 §5「[8]A 形」单按其字面只盖小阶，混阶按同节「或复用
    //   scalar_mul([L]) == IDENTITY」补全）。
    // valid 117 不受影响：素阶键 [8]A = [8]A' ≠ O 且 [L]A == O。
    if a_point.double().double().double().eq_point(&Ge::IDENTITY) {
        return false; // 小阶（8-torsion，含 identity）公钥拒
    }
    if !a_point.scalar_mul(&crate::sc::L_BYTES).eq_point(&Ge::IDENTITY) {
        return false; // 混阶（不在素阶子群）公钥拒
    }
    // 步 2：k = SHA-512(R‖A‖M) mod L（dom2 空）。
    let mut h = Sha512::new();
    h.update(&r_enc);
    h.update(pk);
    h.update(msg);
    let mut hs = [0u8; 64];
    h.finalize(&mut hs);
    let k = sc_reduce(&hs);
    // 步 3：[S]B == R + [k]A'（cofactorless——充分形式，模块头注）。
    let lhs = Ge::BASE.scalar_mul(&s);
    let rhs = r_point.add(&a_point.scalar_mul(&k));
    lhs.eq_point(&rhs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex32(s: &str) -> [u8; 32] {
        let mut b = [0u8; 32];
        for (i, x) in b.iter_mut().enumerate() {
            *x = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        b
    }

    fn unhex64(s: &str) -> [u8; 64] {
        let mut b = [0u8; 64];
        for (i, x) in b.iter_mut().enumerate() {
            *x = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        b
    }

    // TEST 1 端到端（sk→pk→sign→verify 全链；向量归 vectors.rs 门，此处
    // 只钉 API 语义）。
    #[test]
    fn test1_pipeline() {
        let sk = unhex32(
            "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        );
        let expect_pk = unhex32(
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
        );
        let expect_sig = unhex64(
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e0652249015\
             55fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
        );
        let pk = public_key(&sk);
        assert_eq!(pk, expect_pk);
        let sig = sign(&sk, b"");
        assert_eq!(sig, expect_sig);
        assert!(verify(&pk, &sig, b""));
        // 同键同消息确定性（RFC 8032 §8.2 确定性 nonce）。
        assert_eq!(sign(&sk, b""), expect_sig);
    }

    // 篡改拒面：载荷/签名/公钥各翻 1 bit 必拒。
    #[test]
    fn tamper_rejection() {
        let sk = unhex32(
            "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
        );
        let pk = public_key(&sk);
        let msg = b"tamper probe";
        let sig = sign(&sk, msg);
        assert!(verify(&pk, &sig, msg));
        // 载荷翻位。
        let mut m2 = msg.to_vec();
        m2[0] ^= 1;
        assert!(!verify(&pk, &sig, &m2), "msg bit flip");
        // 签名翻位（R 段/S 段各一）。
        let mut s1 = sig;
        s1[0] ^= 1;
        assert!(!verify(&pk, &s1, msg), "R bit flip");
        let mut s2 = sig;
        s2[63] ^= 1;
        assert!(!verify(&pk, &s2, msg), "S bit flip");
        // 公钥翻位。
        let mut p1 = pk;
        p1[0] ^= 1;
        assert!(!verify(&p1, &sig, msg), "pk bit flip");
        // S = L 越界拒（S 段换 L——若解析器漏 S<L 检查此处假绿）。
        let mut s3 = sig;
        s3[32..].copy_from_slice(&crate::sc::L_BYTES);
        assert!(!verify(&pk, &s3, msg), "S == L reject");
        // 长度面由签名类型承载（[u8;64] 编译期钉死——不测运行时）。
    }

    // S ≥ L 且恰为 L+真值的换单独面：S 段 = 真值 + L 应拒（非规范 S——
    // malleability 拒面，cofactorless 语义下 S<L 硬门先拦）。
    #[test]
    fn non_canonical_s_reject() {
        let sk = unhex32(
            "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
        );
        let pk = public_key(&sk);
        let sig = sign(&sk, &[0xaf, 0x82]);
        assert!(verify(&pk, &sig, &[0xaf, 0x82]));
        // S' = S + L ≥ L → 拒（大端加法在小端 limb 上做：u256 + L）。
        let mut sig2 = sig;
        let mut carry = 0u16;
        for i in 0..32 {
            let s = sig2[32 + i] as u16 + crate::sc::L_BYTES[i] as u16 + carry;
            sig2[32 + i] = s as u8;
            carry = s >> 8;
        }
        assert!(carry == 0, "S+L 不进位（S < L）");
        assert!(!verify(&pk, &sig2, &[0xaf, 0x82]), "S+L must reject");
    }

    // 解压缩拒面贯通 verify：非平方 R 编码 / y≥p 公钥。
    #[test]
    fn verify_rejects_malformed_points() {
        let sk = unhex32(
            "833fe62409237b9d62ec77587520911e9a759cec1d19755b7da901b96dca3d42",
        );
        let pk = public_key(&sk);
        let msg = b"malformed probe";
        let sig = sign(&sk, msg);
        assert!(verify(&pk, &sig, msg));
        // 非平方 R：扫一小组找不可解编码替换 R 段。
        let mut found = false;
        for i in 1..16u8 {
            let mut bad = sig;
            bad[0] = 0x1f;
            bad[1] = i;
            if Ge::decompress(&{
                let mut r = [0u8; 32];
                r.copy_from_slice(&bad[..32]);
                r
            })
            .is_none()
            {
                assert!(!verify(&pk, &bad, msg), "非平方 R 拒(i={})", i);
                found = true;
                break;
            }
        }
        assert!(found, "扫频内必有非平方 R 编码");
        // y = p 的公钥拒。
        let mut bad_pk = [0xffu8; 32];
        bad_pk[31] = 0x7f;
        bad_pk[0] = 0xed;
        assert!(!verify(&bad_pk, &sig, msg), "pk y=p 拒");
    }

    // 消息边界：0B/1B/111B（跨 SHA-512 块边界 111+64=175B 两块流）。
    #[test]
    fn message_length_boundaries() {
        let sk = unhex32("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let pk = public_key(&sk);
        for len in [0usize, 1, 111, 112, 127, 128, 129, 255, 256] {
            let msg: Vec<u8> = (0..len).map(|i| (i * 31 + 7) as u8).collect();
            let sig = sign(&sk, &msg);
            assert!(verify(&pk, &sig, &msg), "len={}", len);
            if len > 0 {
                let mut m = msg.clone();
                m[len - 1] ^= 0x80;
                assert!(!verify(&pk, &sig, &m), "tamper len={}", len);
            }
        }
    }

    // clamp 语义钉板：展宽标量 bit0..2 清、bit254 置、bit255 清。
    #[test]
    fn clamp_bits() {
        let sk = unhex32("f5e5767cf153319517630f226876b86c8160cc583bc013744c6bf255f5cc0ee5");
        let (a, prefix) = expand(&sk);
        assert_eq!(a[0] & 0x07, 0, "低 3 bit 清");
        assert_eq!(a[31] & 0x80, 0, "bit255 清");
        assert_eq!(a[31] & 0x40, 0x40, "bit254 置");
        assert_ne!(a, [0u8; 32]);
        assert_ne!(prefix, [0u8; 32]);
        // a 与 prefix 不重叠（SHA-512 前后 32B）。
        let h = digest(&sk);
        assert_eq!(a[..], {
            let mut c = [0u8; 32];
            c.copy_from_slice(&h[..32]);
            c[0] &= 0xf8;
            c[31] &= 0x7f;
            c[31] |= 0x40;
            c
        }[..]);
    }
}
