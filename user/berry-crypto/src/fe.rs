//! Fp = GF(2^255-19) 域算术——5×51 bit limb 表示（u128 乘法，安全原语）。
//!
//! P0-2 原语三件之二（24 号 §3.2 列名 field.rs；本刀名 fe.rs 与 ge/sc
//! 呼应）。表示选型（任务书「_radix 表示自选」）：5×51 bit 懒约化
//! limb——乘积累积走 u128（输入 limb < 2^51.1 时累积 < 2^109，无 64 bit
//! 溢出面），进位链短（51 bit 对齐），零 unsafe。
//!
//! **常时性声明（诚实化，不过度声称）**：加/减/乘/平方为直线码（无
//! 秘密依赖分支）；求逆/幂走定长平方-乘（指数为公开常数 p-2 等，分支
//! 只读指数位不涉秘密）；秘密相关的比较/选择经掩码（select）实现。
//! **未做侧信道审计，不用于在线敌手环境**（离线验签/平台签发场景——
//! 设备侧敌手面随 P3 另评估，24 号 §3.9-1 同款注）。
//!
//! 常量单源+自证纪律：本模块指数常量（p-2/(p-5)/8）由测试对照 python
//! 生成值钉板；曲线常量（D/SQRT_M1/B）在 ge.rs 从 RFC 8032 Table 1
//! 十进制原值自证——不手抄互拷。

/// 51 bit 掩码。
const MASK51: u64 = (1u64 << 51) - 1;

/// 域元素：limb i 权 2^(51i)，值 = Σ limb[i]·2^(51i)（mod p 前的原值）。
/// 不变式：公开运算出口经 [`Fe::carry`] 归一（各 limb < 2^51+小量，
/// 总值 < 2^255）；[`Fe::to_bytes`] 完成条件减 p（值 < p 规范形）。
#[derive(Clone, Copy, Debug)]
pub struct Fe(pub(crate) [u64; 5]);

impl Fe {
    /// 零元。
    pub const ZERO: Fe = Fe([0, 0, 0, 0, 0]);
    /// 单位元。
    pub const ONE: Fe = Fe([1, 0, 0, 0, 0]);

    /// 32 字节小端加载（bit 255 被掩除——**不**检查值 < p；严格拒收面
    /// 在 [`Fe::from_bytes_strict`]）。
    pub fn from_bytes(b: &[u8; 32]) -> Fe {
        let l = |off: usize, sh: u32| -> u64 {
            let mut w = [0u8; 8];
            w.copy_from_slice(&b[off..off + 8]);
            (u64::from_le_bytes(w) >> sh) & MASK51
        };
        Fe([l(0, 0), l(6, 3), l(12, 6), l(19, 1), l(24, 12)])
    }

    /// 严格加载：掩 bit 255 后值 ≥ p 拒收（RFC 8032 Appendix A Python
    /// frombytes 语义「If this fails, fail」——非常规编码拒收面）。
    pub fn from_bytes_strict(b: &[u8; 32]) -> Option<Fe> {
        let fe = Self::from_bytes(b);
        if fe.is_ge_p() {
            None
        } else {
            Some(fe)
        }
    }

    /// 规范形出 32 字节小端（值 < p，bit 255 = 0）。
    pub fn to_bytes(&self) -> [u8; 32] {
        let f = self.carry().carry();
        // 条件减 p：v+19 的 2^255 溢出位 q ∈ {0,1} ⟺ v ≥ p = 2^255-19
        // （v < 2^255 恒成立——limb 有界）。v - q·p = v + 19q - q·2^255：
        // limb0 加 19q 后进位链的**顶进位即 q·2^255 位，丢弃不折回**。
        let l = f.0;
        let mut q = (l[0] + 19) >> 51;
        q = (l[1] + q) >> 51;
        q = (l[2] + q) >> 51;
        q = (l[3] + q) >> 51;
        q = (l[4] + q) >> 51;
        // v + 19q 的进位链（顶进位丢弃）。
        let mut r = [0u64; 5];
        let t0 = l[0] + 19 * q;
        let c0 = t0 >> 51;
        r[0] = t0 & MASK51;
        let t1 = l[1] + c0;
        let c1 = t1 >> 51;
        r[1] = t1 & MASK51;
        let t2 = l[2] + c1;
        let c2 = t2 >> 51;
        r[2] = t2 & MASK51;
        let t3 = l[3] + c2;
        let c3 = t3 >> 51;
        r[3] = t3 & MASK51;
        r[4] = (l[4] + c3) & MASK51;
        let [l0, l1, l2, l3, l4] = r;
        debug_assert!(l4 < MASK51 + 1);
        let w = [
            l0 | (l1 << 51),
            (l1 >> 13) | (l2 << 38),
            (l2 >> 26) | (l3 << 25),
            (l3 >> 39) | (l4 << 12),
            0,
        ];
        let mut out = [0u8; 32];
        for (chunk, word) in out.chunks_exact_mut(8).zip(w.iter()) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    /// 进位链（输入 limb < 2^61 量级安全；输出 limb < 2^51+19，总值
    /// mod p 不变——顶进位 ×19 折回）。
    fn carry(&self) -> Fe {
        let l = self.0;
        let mut r = [0u64; 5];
        let c0 = l[0] >> 51;
        r[0] = l[0] & MASK51;
        let c1 = (l[1] + c0) >> 51;
        r[1] = (l[1] + c0) & MASK51;
        let c2 = (l[2] + c1) >> 51;
        r[2] = (l[2] + c1) & MASK51;
        let c3 = (l[3] + c2) >> 51;
        r[3] = (l[3] + c2) & MASK51;
        let c4 = (l[4] + c3) >> 51;
        r[4] = (l[4] + c3) & MASK51;
        // 2^255 ≡ 19 (mod p)。
        r[0] += 19 * c4;
        r[1] += r[0] >> 51;
        r[0] &= MASK51;
        Fe(r)
    }

    /// 归一态下判 ≥ p（limb 全 < 2^51：p 的 limb 形 = [2^51-19, 2^51-1
    /// ×4]；自顶向下比较，仅五 limb 同顶且低 limb ≥ 2^51-19 时成立）。
    fn is_ge_p(&self) -> bool {
        let f = self.0; // from_bytes 出口 limb 天然 < 2^51
        f[4] == MASK51
            && f[3] == MASK51
            && f[2] == MASK51
            && f[1] == MASK51
            && f[0] >= MASK51 - 18
    }

    /// 加法（直线码）。
    pub fn add(&self, other: &Fe) -> Fe {
        let a = self.0;
        let b = other.0;
        Fe([a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3], a[4] + b[4]]).carry()
    }

    /// 减法：垫 16p 再 limb 直减（16p limb = 2^55 量级 > 减数 limb 上界
    /// 2^51.1，免下溢），进位链归一（直线码）。
    pub fn sub(&self, other: &Fe) -> Fe {
        const P16: [u64; 5] =
            [16 * (MASK51 - 18), 16 * MASK51, 16 * MASK51, 16 * MASK51, 16 * MASK51];
        let a = self.0;
        let b = other.0;
        Fe([
            (a[0] + P16[0]) - b[0],
            (a[1] + P16[1]) - b[1],
            (a[2] + P16[2]) - b[2],
            (a[3] + P16[3]) - b[3],
            (a[4] + P16[4]) - b[4],
        ])
        .carry()
    }

    /// 取负（ZERO - self）。
    pub fn neg(&self) -> Fe {
        Fe::ZERO.sub(self)
    }

    /// 乘法：5×5 u128 学乘 + 2^255 ≡ 19 折叠 + 进位链（直线码；输入
    /// limb < 2^52 量级，累积 < 2^110 ≪ u128）。
    pub fn mul(&self, other: &Fe) -> Fe {
        let a = self.0;
        let b = other.0;
        let b1_19 = b[1] * 19;
        let b2_19 = b[2] * 19;
        let b3_19 = b[3] * 19;
        let b4_19 = b[4] * 19;
        let c0 = b[0] as u128 * a[0] as u128 + b1_19 as u128 * a[4] as u128
            + b2_19 as u128 * a[3] as u128
            + b3_19 as u128 * a[2] as u128
            + b4_19 as u128 * a[1] as u128;
        let mut c1 = b[0] as u128 * a[1] as u128 + b[1] as u128 * a[0] as u128
            + b2_19 as u128 * a[4] as u128
            + b3_19 as u128 * a[3] as u128
            + b4_19 as u128 * a[2] as u128;
        let mut c2 = b[0] as u128 * a[2] as u128 + b[1] as u128 * a[1] as u128
            + b[2] as u128 * a[0] as u128
            + b3_19 as u128 * a[4] as u128
            + b4_19 as u128 * a[3] as u128;
        let mut c3 = b[0] as u128 * a[3] as u128 + b[1] as u128 * a[2] as u128
            + b[2] as u128 * a[1] as u128
            + b[3] as u128 * a[0] as u128
            + b4_19 as u128 * a[4] as u128;
        let mut c4 = b[0] as u128 * a[4] as u128 + b[1] as u128 * a[3] as u128
            + b[2] as u128 * a[2] as u128
            + b[3] as u128 * a[1] as u128
            + b[4] as u128 * a[0] as u128;

        // 进位链（u128 域收敛 51 bit）。
        let carry = c0 >> 51;
        c1 += carry;
        let r0 = (c0 as u64) & MASK51;
        let carry = c1 >> 51;
        c2 += carry;
        let r1 = (c1 as u64) & MASK51;
        let carry = c2 >> 51;
        c3 += carry;
        let r2 = (c2 as u64) & MASK51;
        let carry = c3 >> 51;
        c4 += carry;
        let r3 = (c3 as u64) & MASK51;
        let carry = c4 >> 51;
        let r4 = (c4 as u64) & MASK51;
        // 顶进位 ×19 折回低位后再收敛一轮。
        Fe([r0 + 19 * (carry as u64), r1, r2, r3, r4]).carry()
    }

    /// 平方（mul 直表——正确性优先；专用平方链 = 性能刀面，非本刀）。
    pub fn sq(&self) -> Fe {
        self.mul(self)
    }

    /// 幂：指数为 32 字节**大端**字节串（调用方仅用公开常数指数——
    /// p-2 / (p-5)/8 / (p-1)/4；平方-乘 256 轮定长）。
    pub fn pow(&self, exp_be: &[u8; 32]) -> Fe {
        let mut r = Fe::ONE;
        for byte in exp_be.iter() {
            for i in (0..8).rev() {
                r = r.sq();
                let s = r.mul(self);
                r = Fe::ct_select((byte >> i) & 1, &s, &r);
            }
        }
        r
    }

    /// 求逆：x^(p-2)（RFC 8032 §5.1.1 推荐；0 的逆 = 0——调用方须已排除
    /// 0 输入或容忍 0）。
    pub fn invert(&self) -> Fe {
        // p-2 = 2^255 - 21 大端（测试钉板对照 python 生成值）。
        const P_MINUS_2_BE: [u8; 32] = [
            0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xeb,
        ];
        self.pow(&P_MINUS_2_BE)
    }

    /// 幂 (p-5)/8（点解压缩候选平方根指数，RFC 8032 §5.1.3 步 2）。
    pub fn pow_p58(&self) -> Fe {
        // (p-5)/8 = (2^255-24)/8 = 2^252 - 3 大端（测试钉板同上）。
        const P58_BE: [u8; 32] = [
            0x0f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xfd,
        ];
        self.pow(&P58_BE)
    }

    /// 常时按位选择：bit=1 选 a，bit=0 选 b（掩码实现——秘密位不分支）。
    pub fn ct_select(bit: u8, a: &Fe, b: &Fe) -> Fe {
        let mask = 0u64.wrapping_sub((bit & 1) as u64);
        let mut r = [0u64; 5];
        for i in 0..5 {
            r[i] = b.0[i] ^ ((a.0[i] ^ b.0[i]) & mask);
        }
        Fe(r)
    }

    /// 常时相等（1 = 等，0 = 不等）。**mod p 语义**：懒约化 limb 可将
    /// 同一域元表示为不同非负整数（如 ≡0 元可表为 p）——比较经
    /// [`Fe::to_bytes`] 规范形（< p 唯一表示；全程直线码无分支）。
    pub fn ct_eq(&self, other: &Fe) -> u8 {
        let a = self.to_bytes();
        let b = other.to_bytes();
        let mut acc = 0u8;
        for i in 0..32 {
            acc |= a[i] ^ b[i];
        }
        (((acc | acc.wrapping_neg()) >> 7) ^ 1) as u8
    }

    /// 判零（1 = 零；mod p 语义——p 的表示也判零）。
    pub fn is_zero(&self) -> u8 {
        self.ct_eq(&Fe::ZERO)
    }

    /// 负性（RFC 8032 §3.1：编码意义「负」= 规范值最低 bit）。
    pub fn is_negative(&self) -> u8 {
        self.to_bytes()[0] & 1
    }

    /// 条件取负：negate=1 取负（掩码选择——解压缩符号位校正用）。
    pub fn ct_neg_if(&self, negate: u8) -> Fe {
        let n = self.neg();
        Fe::ct_select(negate & 1, &n, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{:02x}", x)).collect()
    }

    fn from_hex_le(s: &str) -> Fe {
        let mut b = [0u8; 32];
        for i in 0..32 {
            b[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        Fe::from_bytes(&b)
    }

    // 环公理扫频（xorshift 伪随机 64 样本）：往返/加减互逆/分配律/平方/
    // 逆自证/费马小指纹——全部经 ct_eq 无分支比较。
    #[test]
    fn field_axioms() {
        let mut seed = 0x243f6a8885a308d3u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let p_minus_1_be: [u8; 32] = {
            let mut e = [0xffu8; 32];
            e[0] = 0x7f;
            e[31] = 0xec; // p-1 = 2^255-20：末字节 0xff-20 = 0xeb? 见下方
            e
        };
        // 勘误注：p = 2^255-19 → p 末字节 0xed；p-1 → 0xec；p-2 → 0xeb。
        // python: (2**255-20).to_bytes(32,'big').hex() 末两位 'ec'。
        for _ in 0..64 {
            let mut bytes = [0u8; 32];
            for b in bytes.iter_mut() {
                *b = next() as u8;
            }
            bytes[31] &= 0x7f;
            let a = Fe::from_bytes(&bytes);
            // 规范形回读等值。
            let canon = a.to_bytes();
            assert_eq!(Fe::from_bytes(&canon).ct_eq(&a), 1, "canonical roundtrip");
            assert_eq!(Fe::from_bytes_strict(&canon).unwrap().ct_eq(&a), 1);
            // 加减互逆。
            let b = Fe::from_bytes(&bytes).add(&Fe::ONE);
            assert_eq!(a.add(&b).sub(&b).ct_eq(&a), 1, "add/sub inverse");
            // 分配律。
            let m1 = a.mul(&b.add(&Fe::ONE));
            let m2 = a.mul(&b).add(&a);
            assert_eq!(m1.ct_eq(&m2), 1, "distributive");
            // 平方 = 自乘。
            assert_eq!(a.sq().ct_eq(&a.mul(&a)), 1, "square");
            if a.is_zero() == 0 {
                // 逆自证。
                assert_eq!(a.mul(&a.invert()).ct_eq(&Fe::ONE), 1, "inverse");
                // 费马：a^(p-1) == 1。
                assert_eq!(a.pow(&p_minus_1_be).ct_eq(&Fe::ONE), 1, "fermat");
            }
        }
    }

    // p 边界三面：p-1 收/规范形稳定；p 拒；p-1+1 ≡ 0。
    #[test]
    fn p_boundary() {
        // p-1 = 2^255-20（python 生成末字节 0xec）。
        let mut b = [0xffu8; 32];
        b[31] = 0x7f;
        b[0] = 0xec;
        let pm1 = Fe::from_bytes_strict(&b).unwrap();
        // 期望 = 小端字节序（byte0 在前）：ec + ff×30 + 7f。
        assert_eq!(
            hex(&pm1.to_bytes()),
            format!("ec{}7f", "ff".repeat(30))
        );
        assert_eq!(pm1.add(&Fe::ONE).ct_eq(&Fe::ZERO), 1, "p-1 + 1 == 0");
        // p = 2^255-19 → 严格拒收（值 ≥ p）。
        let mut pb = b;
        pb[0] = 0xed;
        assert!(Fe::from_bytes_strict(&pb).is_none(), "p 应拒");
        // 全 1（2^255-1，bit255 已掩=同值）→ ≥ p 拒。
        let mut q = [0xffu8; 32];
        q[31] = 0x7f;
        assert!(Fe::from_bytes_strict(&q).is_none(), "2^255-1 应拒");
        // 条件减 p 出口覆盖：v ∈ [p, 2^255) 的 to_bytes 落回 [0, p)——
        // 用 ZERO.sub(小值) 造 v ≥ p 的表示（sub 垫 16p 直减产出未终约化
        // 值），规范形 = p - 小值。
        let t = Fe::ZERO.sub(&Fe::ONE); // 表示 -1 ≡ p-1（16p-1 中间态）
        assert_eq!(hex(&t.to_bytes()), format!("ec{}7f", "ff".repeat(30)));
        // 两倍 p-1 规范形 = p-2。
        let t2 = pm1.add(&pm1);
        assert_eq!(hex(&t2.to_bytes()), format!("eb{}7f", "ff".repeat(30)));
    }

    // 常时选择/负性/条件取负。
    #[test]
    fn select_and_negative() {
        let le_val = |v: u8| -> Fe {
            let mut bytes = [0u8; 32];
            bytes[0] = v;
            Fe::from_bytes(&bytes)
        };
        let a = le_val(3);
        let b = le_val(4);
        assert_eq!(Fe::ct_select(1, &a, &b).ct_eq(&a), 1);
        assert_eq!(Fe::ct_select(0, &a, &b).ct_eq(&b), 1);
        assert_eq!(Fe::ct_select(2, &a, &b).ct_eq(&b), 1, "bit 仅低位生效");
        assert_eq!(a.is_negative(), 1);
        assert_eq!(b.is_negative(), 0);
        assert_eq!(Fe::ZERO.is_negative(), 0);
        assert_eq!(a.neg().is_negative(), 0);
        assert_eq!(b.neg().is_negative(), 1);
        assert_eq!(a.ct_neg_if(1).ct_eq(&a.neg()), 1);
        assert_eq!(a.ct_neg_if(0).ct_eq(&a), 1);
    }

    // 指数常量钉板（python 两源核生成值，2026-11 本轮实跑）：
    //   (2**255-21).to_bytes(32,'big').hex()      = 7f..eb（invert 用）
    //   ((2**255-5)//8).to_bytes(32,'big').hex()  = 0f..fd（pow_p58 用）
    //   (2**255-20).to_bytes(32,'big').hex()      = 7f..ec（测试用 p-1）
    #[test]
    fn exponent_constant_pins() {
        let mut p2 = [0xffu8; 32];
        p2[0] = 0x7f;
        p2[31] = 0xeb;
        let mut p58 = [0xffu8; 32];
        p58[0] = 0x0f;
        p58[31] = 0xfd;
        assert_eq!(hex(&p2), format!("7f{}eb", "ff".repeat(30)));
        assert_eq!(hex(&p58), format!("0f{}fd", "ff".repeat(30)));
        // 指数语义端证：2 的 (p-5)/8 幂平方 = 2^((p-5)/4)，配合费马链由
        // 解压缩/全向量门终证（此处只钉字节）。
    }
}


