//! SHA-512 纯 Rust 实现（FIPS 180-4 / RFC 6234 §4.2+§6.4——零依赖 no_std）。
//!
//! P0-2 原语三件之一（24 号 §3.2 模块清单；26 号 §3-1 自研终裁）。为
//! ed25519（RFC 8032 §5.1：H = SHA-512）供哈希核；信封摘要用的
//! SHA-512/256 截断变体已随 P1 mkapp 集成刀落（[`IV_256`]/
//! [`Sha512::new_256`]/[`digest256`]——信封 alg_id=1 的 bap_digest/
//! manifest 关键域摘要 32B 用，v1.2 §三 :53）。
//!
//! 形态纪律（「解析器即产品」家族）：零分配（上下文 ~212B 栈上定长）、
//! 零 unsafe、零依赖、输入只借用、永不 panic。
//!
//! 向量门（RFC 6234 §8.5 官方向量，两源核）：源 1 = rfc-editor.org
//! rfc6234.txt §8.5 测试数组（本轮直抓；TEST1 "abc"/TEST2_2 双块长消息/
//! TEST3 百万 a/TEST4 640B/TEST6 单字节 0xD0——TEST5/7/9 为非整字节
//! 消息，字节流 API 不表达，不属本门）；源 2 = OpenSSL hashlib
//! （本机 3.6.4）逐条对拍全同。空串向量 RFC 6234 无载，源 = OpenSSL
//! hashlib（cf83e135…）——注记在案。

/// SHA-512 分组字节数（1024 bit）。
pub const BLOCK_LEN: usize = 128;

/// SHA-512 摘要字节数（512 bit）。
pub const DIGEST_LEN: usize = 64;

/// 轮常数 K[0..80]（RFC 6234 §5.2——前 80 素数立方根小数部前 64 bit）。
const K: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
    0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
    0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
    0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
    0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
    0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
    0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
    0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
    0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
    0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

/// 初始链值 H(0)（RFC 6234 §6.4.1——SHA-512/224 族截断换装点，本刀仅
/// SHA-512 本体）。
const IV: [u64; 8] = [
    0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
    0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
];

/// SHA-512/256 截断变体初始链值（FIPS 180-4 §6.4.2 Table 2——SHA-512
/// 族第 81–96 素数立方根小数部前 64 bit，与 SHA-512 IV 分源：防两变体
/// 间换用/长度延展混流）。P1 mkapp 集成刀落（本文件头注「P1 另落」
/// 兑现）：信封 alg_id=1 摘要用（envelope.rs `DIGEST_LEN` 32B）。
const IV_256: [u64; 8] = [
    0x22312194fc2bf72c, 0x9f555fa3c84c64c2, 0x2393b86b6f53b151, 0x963877195940eabd,
    0x96283ee2a88effe3, 0xbe5e1e2553863992, 0x2b0199fc2c85b8aa, 0x0eb72ddc81c52ca2,
];

#[inline(always)]
fn load_be64(b: &[u8]) -> u64 {
    let mut w = [0u8; 8];
    w.copy_from_slice(&b[..8]);
    u64::from_be_bytes(w)
}

/// 布尔函数 Ch（RFC 6234 §5.2）。
#[inline(always)]
fn ch(x: u64, y: u64, z: u64) -> u64 {
    z ^ (x & (y ^ z))
}

/// 布尔函数 Maj（RFC 6234 §5.2）。
#[inline(always)]
fn maj(x: u64, y: u64, z: u64) -> u64 {
    (x & y) | (z & (x | y))
}

/// 大Sigma0（ROTR^28 ⊕ ROTR^34 ⊕ ROTR^39）。
#[inline(always)]
fn big_sigma0(x: u64) -> u64 {
    x.rotate_right(28) ^ x.rotate_right(34) ^ x.rotate_right(39)
}

/// 大Sigma1（ROTR^14 ⊕ ROTR^18 ⊕ ROTR^41）。
#[inline(always)]
fn big_sigma1(x: u64) -> u64 {
    x.rotate_right(14) ^ x.rotate_right(18) ^ x.rotate_right(41)
}

/// 小sigma0（ROTR^1 ⊕ ROTR^8 ⊕ SHR^7）。
#[inline(always)]
fn small_sigma0(x: u64) -> u64 {
    x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7)
}

/// 小sigma1（ROTR^19 ⊕ ROTR^61 ⊕ SHR^6）。
#[inline(always)]
fn small_sigma1(x: u64) -> u64 {
    x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6)
}

/// SHA-512 压缩函数（RFC 6234 §6.4.2——80 轮，消息字 big-endian）。
fn compress(h: &mut [u64; 8], block: &[u8; BLOCK_LEN]) {
    let mut w = [0u64; 80];
    for (t, wt) in w.iter_mut().take(16).enumerate() {
        *wt = load_be64(&block[t * 8..]);
    }
    for t in 16..80 {
        w[t] = small_sigma1(w[t - 2])
            .wrapping_add(w[t - 7])
            .wrapping_add(small_sigma0(w[t - 15]))
            .wrapping_add(w[t - 16]);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for t in 0..80 {
        let t1 = hh
            .wrapping_add(big_sigma1(e))
            .wrapping_add(ch(e, f, g))
            .wrapping_add(K[t])
            .wrapping_add(w[t]);
        let t2 = big_sigma0(a).wrapping_add(maj(a, b, c));
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    h[0] = h[0].wrapping_add(a);
    h[1] = h[1].wrapping_add(b);
    h[2] = h[2].wrapping_add(c);
    h[3] = h[3].wrapping_add(d);
    h[4] = h[4].wrapping_add(e);
    h[5] = h[5].wrapping_add(f);
    h[6] = h[6].wrapping_add(g);
    h[7] = h[7].wrapping_add(hh);
}

/// SHA-512 流式上下文（零分配：h + 128B 缓冲 + 128 bit 计数）。
#[derive(Clone)]
pub struct Sha512 {
    h: [u64; 8],
    buf: [u8; BLOCK_LEN],
    buf_len: usize,
    /// 已吸收比特数（u128——RFC 6234 §4.2：长度域 128 bit，溢出面 = 本
    /// 结构饱和错误而非静默回卷；实用输入 ≪ 2^128，不可达）。
    bits: u128,
}

impl Sha512 {
    /// 新上下文（初始链值 = IV）。
    pub const fn new() -> Self {
        Sha512 { h: IV, buf: [0u8; BLOCK_LEN], buf_len: 0, bits: 0 }
    }

    /// 新上下文（SHA-512/256 截断变体——初始链值 = [`IV_256`]，P1 信封
    /// 摘要消费面 = berry-sign/host；非 SHA-512 前 32B 截断）。
    pub const fn new_256() -> Self {
        Sha512 { h: IV_256, buf: [0u8; BLOCK_LEN], buf_len: 0, bits: 0 }
    }

    /// 吸收消息字节流（可分片多次调用）。
    pub fn update(&mut self, mut data: &[u8]) {
        self.bits = self.bits + (data.len() as u128) * 8;
        if self.buf_len > 0 {
            let want = BLOCK_LEN - self.buf_len;
            let take = want.min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == BLOCK_LEN {
                let block = self.buf;
                compress(&mut self.h, &block);
                self.buf_len = 0;
            }
        }
        while data.len() >= BLOCK_LEN {
            let mut block = [0u8; BLOCK_LEN];
            block.copy_from_slice(&data[..BLOCK_LEN]);
            compress(&mut self.h, &block);
            data = &data[BLOCK_LEN..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    /// 终结：按 RFC 6234 §4.2 垫充（1 bit + 0 串 + 128 bit big-endian
    /// 比特长），出 64B 摘要。消费 self（防复用未定义态）。
    pub fn finalize(mut self, out: &mut [u8; DIGEST_LEN]) {
        let bits = self.bits;
        // 垫 0x80：缓冲剩余 < 128 必容纳 1 字节。
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        // 若长度域（16B）放不下 → 先把本块零垫满压缩。
        if BLOCK_LEN - self.buf_len < 16 {
            for b in self.buf[self.buf_len..].iter_mut() {
                *b = 0;
            }
            let block = self.buf;
            compress(&mut self.h, &block);
            self.buf_len = 0;
        }
        for b in self.buf[self.buf_len..BLOCK_LEN - 16].iter_mut() {
            *b = 0;
        }
        self.buf[BLOCK_LEN - 16..].copy_from_slice(&bits.to_be_bytes());
        let block = self.buf;
        compress(&mut self.h, &block);
        for (chunk, word) in out.chunks_exact_mut(8).zip(self.h.iter()) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
    }
}

/// 一次性摘要（selftest/内部路径——ed25519 逐段拼接后走此入口）。
pub fn digest(data: &[u8]) -> [u8; DIGEST_LEN] {
    let mut h = Sha512::new();
    h.update(data);
    let mut out = [0u8; DIGEST_LEN];
    h.finalize(&mut out);
    out
}

/// SHA-512/256 一次性摘要（截断变体，32B——信封 alg_id=1 的
/// bap_digest/manifest 关键域摘要是第一消费面，envelope.rs `DIGEST_LEN`
/// 同长单源；P1 落）。
pub fn digest256(data: &[u8]) -> [u8; DIGEST_LEN / 2] {
    let mut h = Sha512::new_256();
    h.update(data);
    let mut full = [0u8; DIGEST_LEN];
    h.finalize(&mut full);
    let mut out = [0u8; DIGEST_LEN / 2];
    out.copy_from_slice(&full[..DIGEST_LEN / 2]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{:02x}", x)).collect()
    }

    // RFC 6234 §8.5 官方向量（源 1 = rfc-editor txt 直抓；源 2 = OpenSSL
    // hashlib 对拍全同——两源核在册）。TEST5/7/9 非整字节消息不属字节流
    // API 表达域，不载。
    #[test]
    fn rfc6234_official_vectors() {
        // TEST1 "abc"
        assert_eq!(
            hex(&digest(b"abc")),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea2\
             0a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd\
             454d4423643ce80e2a9ac94fa54ca49f"
        );
        // TEST2_2 双块长消息（112 字节）
        assert_eq!(
            hex(&digest(
                b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmno\
                  ijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
            )),
            "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa1\
             7299aeadb6889018501d289e4900f7e4331b99dec4b5433a\
             c7d329eeb6dd26545e96e55b874be909"
        );
        // TEST3 "a" × 1,000,000（百万 a——分片喂入同时钉流式边界）
        let mut h = Sha512::new();
        for _ in 0..1000 {
            h.update(&[b'a'; 1000]);
        }
        let mut out = [0u8; 64];
        h.finalize(&mut out);
        assert_eq!(
            hex(&out),
            "e718483d0ce769644e2e42c7bc15b4638e1f98b13b204428\
             5632a803afa973ebde0ff244877ea60a4cb0432ce577c31b\
             eb009c5c2c49aa2e4eadb217ad8cc09b"
        );
        // TEST4 "01234567…32 字节模式 ×20 = 640B（恰 5 块——整块边界）
        let pat = b"0123456701234567012345670123456701234567012345670123456701234567";
        let mut h = Sha512::new();
        for _ in 0..10 {
            h.update(pat);
        }
        let mut out = [0u8; 64];
        h.finalize(&mut out);
        assert_eq!(
            hex(&out),
            "89d05ba632c699c31231ded4ffc127d5a894dad412c0e024\
             db872d1abd2ba8141a0f85072a9be1e2aa04cf33c765cb51\
             0813a39cd5a84c4acaa64d3f3fb7bae9"
        );
        // TEST6 单字节 0xD0（<1 word 的最小非空块内边界）
        assert_eq!(
            hex(&digest(&[0xd0])),
            "9992202938e882e73e20f6b69e68a0a7149090423d93c81b\
             ab3f21678d4aceeee50e4e8cafada4c85a54ea8306826c4a\
             d6e74cece9631bfa8a549b4ab3fbba15"
        );
    }

    // 空串向量（RFC 6234 §8.5 无零长消息样例——源 = OpenSSL hashlib
    // 对拍单源 + RFC 4.2 垫宽语义推得；两源注记如实：源 2 唯一）。
    #[test]
    fn empty_message() {
        assert_eq!(
            hex(&digest(b"")),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc\
             83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f\
             63b931bd47417a81a538327af927da3e"
        );
    }

    // 流式一致性：任意分片 == 一次性（0..257 遍历片长——钉 update 的
    // 缓冲/整块/尾三分支与 finalize 垫宽交互）。
    #[test]
    fn streaming_equivalence() {
        let msg: Vec<u8> = (0u16..800).map(|i| (i * 7 + 3) as u8).collect();
        let one = digest(&msg);
        for chunk in 1..257usize {
            let mut h = Sha512::new();
            for part in msg.chunks(chunk) {
                h.update(part);
            }
            let mut out = [0u8; 64];
            h.finalize(&mut out);
            assert_eq!(out[..], one[..], "chunk={}", chunk);
        }
    }

    // 上下文复用（Clone 中途快照——两条支路摘要互不污染）。
    #[test]
    fn context_snapshot() {
        let mut h = Sha512::new();
        h.update(b"abc");
        let mut snap = h.clone();
        h.update(b"def");
        let mut a = [0u8; 64];
        h.finalize(&mut a);
        assert_eq!(a[..], digest(b"abcdef")[..]);
        let mut b = [0u8; 64];
        snap.update(b"xyz");
        snap.finalize(&mut b);
        assert_eq!(b[..], digest(b"abcxyz")[..]);
    }

    // SHA-512/256 向量门（P1 信封摘要面——alg_id=1 的 bap_digest/
    // manifest 关键域摘要第一消费面）。两源核：源 1 = OpenSSL
    // `openssl dgst -sha512-256`（2026-11-11 实测）；源 2 = Python
    // hashlib('sha512_256') 对拍全同（空/abc/abcdef 三消息）。
    #[test]
    fn sha512_256_vectors() {
        assert_eq!(
            hex(&digest256(b"")),
            "c672b8d1ef56ed28ab87c3622c5114069bdd3ad7b8f9737498d0c01ecef0967a"
        );
        assert_eq!(
            hex(&digest256(b"abc")),
            "53048e2681941ef99b2e29b76b4c7dabe4c2d0c634fc6d46e0e2f13107e7af23"
        );
        assert_eq!(
            hex(&digest256(b"abcdef")),
            "e4fdcb11d1ac14e698743acd8805174cea5ddc0d312e3e47f6372032571bad84"
        );
        // IV 分源执法面：SHA-512/256 ≠ SHA-512 摘要前 32B 截断（误把
        // digest()[..32] 当截断变体的实现在此炸）。
        let d = digest(b"abc");
        assert_ne!(&d[..32], &digest256(b"abc")[..]);
        // 流式一致性（new_256 上下文同 update/finalize 机制——钉分片路径）。
        let mut h = Sha512::new_256();
        h.update(b"ab");
        h.update(b"cdef");
        let mut full = [0u8; 64];
        h.finalize(&mut full);
        assert_eq!(&full[..32], &digest256(b"abcdef")[..]);
    }
}
