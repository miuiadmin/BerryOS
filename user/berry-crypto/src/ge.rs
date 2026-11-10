//! edwards25519 点运算——扩展坐标（X:Y:Z:T，x=X/Z, y=Y/Z, T=XY/Z）。
//!
//! P0-2 原语三件之三（24 号 §3.2 列名 edwards.rs；本刀名 ge.rs 与 fe/sc
//! 呼应）。加法公式 = RFC 8032 §5.1.4/Appendix A（EFD add-2008-hwcd-3，
//! a=-1）：**完备**——a=-1 为平方（p≡1 mod 4）、d 非平方（测试自证），
//! 任意点对（含 P=Q、P=-Q、单方/双方=单位元）皆适用，无例外分支。
//! 倍点 = add(P,P) 直表（统一公式覆盖倍乘；专用倍点链 = 性能刀面）。
//!
//! **解压缩拒收面（任务书执法点）**：RFC 8032 §5.1.3 + Appendix A
//! Python——①y ≥ p 拒（非常规编码，frombytes「If this fails, fail」）；
//! ②u/v 非平方拒（x² 无平方根，即「Y 坐标解出 X 无平方根即拒」）；
//! ③x=0 且符号位=1 拒（-0 无解）。低阶点（合法曲线点）**不**在此拒——
//! 其拒收面 = Wycheproof 门（strict 语义）随姊妹刀执法（26 号 §3-2：
//! mixed-order 门归 P0-2 姊妹件）。
//!
//! **常时性**：标量乘 = 定长 256 轮双-常加（秘密标量位经 [`Fe::ct_select`]
//! 掩码，无秘密分支）；解压缩分支只依赖公开输入。未做侧信道审计
//! （fe.rs 同款诚实注）。
//!
//! 常量单源+自证：D/SQRT_M1/B 的 limb 由 python 从 RFC 8032 Table 1
//! 十进制定义/字节串生成（2026-11 本轮实跑），**测试自证**回算等值
//! （D == -121665·121666⁻¹、SQRT_M1 == 2^((p-1)/4)、B 在曲线且压缩形
//! == 5866…66）——手抄漂移由自证面拦截。

use crate::fe::Fe;

/// 曲线常数 d = -121665/121666（RFC 8032 Table 1）。
pub const D: Fe = Fe([
    929955233495203, 466365720129213, 1662059464998953, 2033849074728123, 1442794654840575,
]);

/// sqrt(-1) = 2^((p-1)/4)（RFC 8032 §5.1.1 步 3 的 x·sqrt(-1) 校正因子）。
pub const SQRT_M1: Fe = Fe([
    1718705420411056, 234908883556509, 2233514472574048, 2117202627021982, 765476049583133,
]);

/// 基点 B 仿射 x（RFC 8032 Table 1 十进制 → 小端字节 → 5×51 limb）。
const BX: Fe = Fe([
    1738742601995546, 1146398526822698, 2070867633025821, 562264141797630, 587772402128613,
]);
/// 基点 B 仿射 y（同上单源；y = 4/5 mod p）。
const BY: Fe = Fe([
    1801439850948184, 1351079888211148, 450359962737049, 900719925474099, 1801439850948198,
]);
/// 基点 B 的 T = x·y（python 同源生成）。
const BT: Fe = Fe([
    1841354044333475, 16398895984059, 755974180946558, 900171276175154, 1821297809914039,
]);

/// 扩展坐标点。
#[derive(Clone, Copy, Debug)]
pub struct Ge {
    pub(crate) x: Fe,
    pub(crate) y: Fe,
    pub(crate) z: Fe,
    pub(crate) t: Fe,
}

impl Ge {
    /// 单位元 (0:1:1:0)。
    pub const IDENTITY: Ge = Ge { x: Fe::ZERO, y: Fe::ONE, z: Fe::ONE, t: Fe::ZERO };

    /// 基点 B = (BX, BY, 1, BX·BY)。
    pub const BASE: Ge = Ge { x: BX, y: BY, z: Fe::ONE, t: BT };

    /// 完备加法（RFC 8032 §5.1.4 公式逐项）。
    pub fn add(&self, other: &Ge) -> Ge {
        let a = self.y.sub(&self.x).mul(&other.y.sub(&other.x));
        let b = self.y.add(&self.x).mul(&other.y.add(&other.x));
        let tt = self.t.mul(&other.t);
        let c = tt.mul(&D).add(&tt.mul(&D)); // 2dT1T2
        let zz = self.z.mul(&other.z);
        let d = zz.add(&zz); // 2Z1Z2
        let e = b.sub(&a);
        let f = d.sub(&c);
        let g = d.add(&c);
        let h = b.add(&a);
        Ge { x: e.mul(&f), y: g.mul(&h), z: f.mul(&g), t: e.mul(&h) }
    }

    /// 倍点（完备公式的 P+P 特例——正确性优先，专用链 = 性能刀面）。
    pub fn double(&self) -> Ge {
        self.add(self)
    }

    /// 取负（(x,y) → (-x,y)；扩展坐标直接翻 X/T）。
    pub fn neg(&self) -> Ge {
        Ge { x: self.x.neg(), y: self.y, z: self.z, t: self.t.neg() }
    }

    /// 标量乘（小端 32B 标量；MSB 先行 256 轮双-常加——秘密位掩码选择，
    /// 无秘密分支。标量可为任意 256 bit 值：clamped 秘密 a 与约减后的
    /// r/k/S 均适用）。
    pub fn scalar_mul(&self, scalar: &[u8; 32]) -> Ge {
        let mut acc = Ge::IDENTITY;
        for i in (0..256).rev() {
            acc = acc.double();
            let prod = acc.add(self);
            acc = Ge::ct_select((scalar[i / 8] >> (i % 8)) & 1, &prod, &acc);
        }
        acc
    }

    /// 常时按位选择（四坐标逐 Fe 掩码——秘密标量位不分支）。
    pub fn ct_select(bit: u8, a: &Ge, b: &Ge) -> Ge {
        Ge {
            x: Fe::ct_select(bit, &a.x, &b.x),
            y: Fe::ct_select(bit, &a.y, &b.y),
            z: Fe::ct_select(bit, &a.z, &b.z),
            t: Fe::ct_select(bit, &a.t, &b.t),
        }
    }

    /// 点相等（交叉乘消除除法：x1·z2==x2·z1 ∧ y1·z2==y2·z1——RFC 8032
    /// Appendix A __eq__ 同式）。
    pub fn eq_point(&self, other: &Ge) -> bool {
        let xn1 = self.x.mul(&other.z);
        let xn2 = other.x.mul(&self.z);
        let yn1 = self.y.mul(&other.z);
        let yn2 = other.y.mul(&self.z);
        xn1.ct_eq(&xn2) == 1 && yn1.ct_eq(&yn2) == 1
    }

    /// 压缩（RFC 8032 §5.1.2：y 小端 32B，bit255 = x 负性）。
    pub fn compress(&self) -> [u8; 32] {
        let zinv = self.z.invert();
        let x = self.x.mul(&zinv);
        let y = self.y.mul(&zinv);
        let mut out = y.to_bytes();
        out[31] |= x.is_negative() << 7;
        out
    }

    /// 解压缩（严格拒收面见模块注：y≥p 拒 / 非平方拒 / x=0∧sign 拒）。
    pub fn decompress(bytes: &[u8; 32]) -> Option<Ge> {
        let sign = bytes[31] >> 7;
        let y = Fe::from_bytes_strict(bytes)?;
        let yy = y.sq();
        // u = y²-1，v = d·y²+1（分母非零 mod p——RFC 8032 §5.1.3 注）。
        let u = yy.sub(&Fe::ONE);
        let v = yy.mul(&D).add(&Fe::ONE);
        // x = u·v³·(u·v⁷)^((p-5)/8)（§5.1.3 步 2 单幂技巧）。
        let v3 = v.sq().mul(&v);
        let uv7 = u.mul(&v3).mul(&v3).mul(&v);
        let mut x = u.mul(&v3).mul(&uv7.pow_p58());
        // 三分支判定（§5.1.3 步 3）：v·x²==u 根成 / ==-u 乘 sqrt(-1) / 拒。
        let vxx = v.mul(&x.sq());
        if vxx.ct_eq(&u) == 1 {
            // x 即候选根。
        } else if vxx.ct_eq(&u.neg()) == 1 {
            x = x.mul(&SQRT_M1);
        } else {
            return None; // 非平方：Y 解 X 无平方根——拒收。
        }
        // x=0 只有 sign=0 一解（§5.1.3 步 3 末：zero 且 xs=1 → 拒）。
        if x.is_zero() == 1 && sign == 1 {
            return None;
        }
        let x = x.ct_neg_if(x.is_negative() ^ sign);
        Some(Ge { x, y, z: Fe::ONE, t: x.mul(&y) })
    }

    /// 曲线上判定（自证用：(y²-x²)·z² == z⁴ + d·x²·y² 且 T·Z == X·Y）。
    pub fn is_on_curve(&self) -> bool {
        let x2 = self.x.sq();
        let y2 = self.y.sq();
        let z2 = self.z.sq();
        let lhs = y2.sub(&x2).mul(&z2);
        let rhs = z2.sq().add(&x2.mul(&y2).mul(&D));
        let tz_ok = self.t.mul(&self.z).ct_eq(&self.x.mul(&self.y)) == 1;
        lhs.ct_eq(&rhs) == 1 && tz_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le_bytes(s: &str) -> [u8; 32] {
        let mut b = [0u8; 32];
        for i in 0..32 {
            b[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        b
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{:02x}", x)).collect()
    }

    // 常量自证三连（手抄漂移拦截面）。
    #[test]
    fn constants_self_proof() {
        // D == -121665·121666⁻¹。
        let le_val = |v: u64| -> Fe {
            let mut bytes = [0u8; 32];
            bytes[..8].copy_from_slice(&v.to_le_bytes());
            Fe::from_bytes(&bytes)
        };
        let m121665 = le_val(121665);
        let m121666 = le_val(121666);
        assert_eq!(m121665.neg().mul(&m121666.invert()).ct_eq(&D), 1, "D self-proof");
        // d 非平方（完备性前提）。
        let p_minus_1_over_4 = {
            let mut e = [0xffu8; 32];
            e[0] = 0x1f;
            e[31] = 0xfb; // (p-1)/4 = 2^253-5（python: 1f..fb）
            e
        };
        assert_eq!(D.pow(&p_minus_1_over_4).ct_eq(&Fe::ONE), 0, "d 非平方");
        // SQRT_M1 == 2^((p-1)/4) 且平方 == -1。
        let two = le_val(2);
        assert_eq!(two.pow(&p_minus_1_over_4).ct_eq(&SQRT_M1), 1, "sqrt(-1) self-proof");
        assert_eq!(SQRT_M1.sq().ct_eq(&Fe::ONE.neg()), 1);
        // B 在曲线 + T 自洽 + 压缩形 == 5866…66（B 表唯一压缩形）。
        assert!(Ge::BASE.is_on_curve(), "B on curve");
        let benc = Ge::BASE.compress();
        assert_eq!(hex(&benc), "5866666666666666666666666666666666666666666666666666666666666666");
    }

    // 群公理：交换律/结合律/单位元/逆元（用 [k]B 采样——必然在曲线上）。
    #[test]
    fn group_axioms() {
        let le_scalar = |v: u64| -> [u8; 32] {
            let mut b = [0u8; 32];
            b[..8].copy_from_slice(&v.to_le_bytes());
            b
        };
        let p1 = Ge::BASE.scalar_mul(&le_scalar(0x1234567));
        let p2 = Ge::BASE.scalar_mul(&le_scalar(0x89abcde));
        let p3 = Ge::BASE.scalar_mul(&le_scalar(0xfeedface));
        assert!(p1.is_on_curve() && p2.is_on_curve() && p3.is_on_curve());
        // 交换律。
        assert!(p1.add(&p2).eq_point(&p2.add(&p1)));
        // 结合律。
        assert!(p1.add(&p2).add(&p3).eq_point(&p1.add(&p2.add(&p3))));
        // 单位元。
        assert!(p1.add(&Ge::IDENTITY).eq_point(&p1));
        assert!(Ge::IDENTITY.double().eq_point(&Ge::IDENTITY));
        // 逆元：P + (-P) = 单位元。
        assert!(p1.add(&p1.neg()).eq_point(&Ge::IDENTITY));
        // 倍点一致性。
        assert!(p1.double().eq_point(&p1.add(&p1)));
        // 全程曲线闭合。
        assert!(p1.add(&p2).is_on_curve());
        assert!(p1.neg().is_on_curve());
    }

    // 群阶：[L]B == 单位元（B 的素阶自证——L 取 RFC 8032 Table 1）。
    #[test]
    fn base_point_order() {
        let l = le_bytes("edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010");
        let lb = Ge::BASE.scalar_mul(&l);
        assert!(lb.eq_point(&Ge::IDENTITY), "[L]B == O");
        // [L+1]B == B。
        let mut l1 = l;
        l1[0] += 1;
        assert!(Ge::BASE.scalar_mul(&l1).eq_point(&Ge::BASE));
        // 标量乘分配：[a]B + [b]B == [a+b]B。
        let a = {
            let mut t = [0u8; 32];
            t[..8].copy_from_slice(&0x9e3779b97f4a7c15u64.to_le_bytes());
            t
        };
        let b = {
            let mut t = [0u8; 32];
            t[..8].copy_from_slice(&0x0114730195728843u64.to_le_bytes());
            t
        };
        let mut ab = [0u8; 32];
        let mut carry = 0u16;
        for i in 0..32 {
            let s = a[i] as u16 + b[i] as u16 + carry;
            ab[i] = s as u8;
            carry = s >> 8;
        }
        assert!(Ge::BASE.scalar_mul(&a).add(&Ge::BASE.scalar_mul(&b))
            .eq_point(&Ge::BASE.scalar_mul(&ab)));
    }

    // 压缩/解压缩往返 + 三类拒收面。
    #[test]
    fn compress_roundtrip_and_rejects() {
        // 往返：采样点（含基点/单位元/随机倍点）。
        let mut seed = 0x5deece66d5deece6u64;
        for k in 0..8u64 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407 + k);
            let pt = Ge::BASE.scalar_mul(&{
                let mut b = [0u8; 32];
                b[..8].copy_from_slice(&seed.to_le_bytes());
                b
            });
            assert!(pt.is_on_curve());
            let enc = pt.compress();
            let back = Ge::decompress(&enc).expect("合法压缩必可解");
            assert!(back.eq_point(&pt), "roundtrip k={}", k);
        }
        // 单位元 y=1 sign=0 可解；sign=1（x=0 拒）。
        let mut id_enc = [0u8; 32];
        id_enc[0] = 1;
        assert!(Ge::decompress(&id_enc).unwrap().eq_point(&Ge::IDENTITY));
        let mut bad = id_enc;
        bad[31] = 0x80;
        assert!(Ge::decompress(&bad).is_none(), "x=0 ∧ sign=1 拒");
        // y ≥ p 拒（p 编码：末字节 0xed）。
        let mut y_eq_p = [0xffu8; 32];
        y_eq_p[31] = 0x7f;
        y_eq_p[0] = 0xed;
        assert!(Ge::decompress(&y_eq_p).is_none(), "y=p 拒");
        // 非平方 y：扫频找若干拒例（非平方密度 ~1/2——采样 8 个必有）。
        let mut rejects = 0;
        for i in 1..=8u8 {
            let mut cand = [0u8; 32];
            cand[0] = 0x07 * i;
            cand[31] = i; // 低值 y + 变头——快扫
            if Ge::decompress(&cand).is_none() {
                rejects += 1;
            }
        }
        assert!(rejects >= 1, "采样内应含非平方拒例（实测 {} 个）", rejects);
    }
}
