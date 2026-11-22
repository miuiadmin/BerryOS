//! berry-vqvec —— vault 定点量化向量基线（ai-runtime 总体设计契约四.1
//! 首刀：`.agents/plans/ai-runtime/design.md`「轻量分层可导航小世界图
//! （HNSW 定点量化向量索引）」）。本 crate 钉**量化算术面**：库向量
//! 每 vec 单 f32 scale 的 i8 标量量化（非对称——查询侧保 f32 精度、库
//! 侧省 4×内存并走整数域距离），HNSW 图结构/分层导航、FBE 加密与
//! `VaultQueryCap` 门控归 berry-vault 服务刀。
//!
//! 家族纪律（berry-aiproto/berry-tensor/berry-kvpool 同款）：零 unsafe、
//! 零依赖、零分配（定长缓冲由调用方持）；`cfg_attr(not(test), no_std)`。
//! host 测试经 justfile test-host 显式行跑。
//!
//! ## 量化律
//!
//! - scale = max|v| / 127（全 vec 单 scale——对称域 [-s, s]，i8 覆盖满
//!   域）；全零 vec scale 取 0（量化全零，距离退化 0，无除零）。
//! - 量化：q = round(v / scale) 钳 [-127, 127]（round-half-away-from-zero
//!   手写——core 无 f32::round，`v + 0.5` floor 式对正数、对称处理负数）。
//! - 距离（非对称）：内积 = scale × Σ(q_i × x_i)（i32 整数内积——128 维
//!   i8×f32 上界 128×127×3.4e38 溢出？f32 先乘每项再累加保守——改为
//!   逐项 f32 累积保确定性顺序，与 berry-tensor dot_f32 同纪律）；
//!   L2² = Σ(x_i − scale·q_i)²（重建域距离——量化误差直入距离）。
//!
//! ## 契约纪律
//!
//! - 长度失配/越域 = 程序员错误面，`assert!` 即时响（家族分层同
//!  berry-tensor：输入是已验证的合法索引数据）。
//! - 确定性：量化与距离全顺序累积，同输入恒同输出——HNSW 建图的可
//!   复现前提（检索排序 tie-break 的稳定底座）。

#![cfg_attr(not(test), no_std)]

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VqError {
    /// 查询向量与量化库向量长度失配。
    LenMismatch,
    /// scale 非法（负或非有限——量化域破坏）。
    BadScale,
}

impl fmt::Display for VqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::LenMismatch => "查询/库向量长度失配",
            Self::BadScale => "scale 非法（负或非有限）",
        };
        f.write_str(s)
    }
}

/// round-half-away-from-zero（core 无 f32::round；|x| 对称处理）。
fn round_afz(x: f32) -> f32 {
    if x >= 0.0 {
        // 正数：+0.5 截断（[0, 2^23) 精确域——量化幅度域远小，安全）。
        let t = x + 0.5;
        if t >= 8388608.0 { x } else { trunc_bits(t) }
    } else {
        -round_afz(-x)
    }
}

/// floor(x)（x ≥ 0 域——round_afz 正臂专用，位截断等价 floor）。
fn trunc_bits(x: f32) -> f32 {
    // 非负域 trunc == floor；用整数比较截断（避开 core 无 trunc 的面，
    // 位级：尾数抹到整数格——幅度 < 2^23 时可精确）。
    let b = x.to_bits();
    let e = ((b >> 23) & 0xff) as i32 - 127; // 指数（二进制阶）
    if e < 0 {
        return 0.0; // |x| < 1：floor = 0（x ≥ 0 域）
    }
    if e >= 23 {
        return x; // 已是整数值（尾数幂对齐）
    }
    let mask_frac = 0x7f_ffff_u32 >> e; // 尾数小数位
    f32::from_bits(b & !mask_frac)
}

/// 库向量 i8 量化（每 vec 单 scale）：`v`/`q` 等长，返回 scale =
/// max|v| / 127（全零 vec 返 0.0——调用方距离面退化处理，禁负 scale）。
pub fn quantize_i8(v: &[f32], q: &mut [i8]) -> f32 {
    assert_eq!(v.len(), q.len(), "quantize 长度失配：{} vs {}", v.len(), q.len());
    let mut amax = 0f32;
    for x in v {
        let a = if *x < 0.0 { -*x } else { *x };
        if a > amax {
            amax = a;
        }
    }
    if amax == 0.0 {
        q.fill(0);
        return 0.0;
    }
    let scale = amax / 127.0;
    let inv = 127.0 / amax;
    for i in 0..v.len() {
        let t = round_afz(v[i] * inv);
        // 钳 [-127, 127]（round 边界幅 127.0 内，钳为确定性保险）。
        let t = if t > 127.0 { 127.0 } else if t < -127.0 { -127.0 } else { t };
        q[i] = t as i8;
    }
    scale
}

/// 非对称内积：f32 查询 `x` · 量化库向量 (`q`, scale)。
pub fn dot_asym(x: &[f32], q: &[i8], scale: f32) -> Result<f32, VqError> {
    if x.len() != q.len() {
        return Err(VqError::LenMismatch);
    }
    if !(scale >= 0.0) || scale.is_nan() || scale.is_infinite() {
        return Err(VqError::BadScale);
    }
    let mut acc = 0f32;
    for i in 0..x.len() {
        acc += x[i] * (scale * q[i] as f32);
    }
    Ok(acc)
}

/// 非对称 L2²（重建域）：Σ(x_i − scale·q_i)²。
pub fn l2sq_asym(x: &[f32], q: &[i8], scale: f32) -> Result<f32, VqError> {
    if x.len() != q.len() {
        return Err(VqError::LenMismatch);
    }
    if !(scale >= 0.0) || scale.is_nan() || scale.is_infinite() {
        return Err(VqError::BadScale);
    }
    let mut acc = 0f32;
    for i in 0..x.len() {
        let d = x[i] - scale * q[i] as f32;
        acc += d * d;
    }
    Ok(acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_afz_half_away_and_negatives() {
        assert_eq!(round_afz(0.5), 1.0);
        assert_eq!(round_afz(1.5), 2.0);
        assert_eq!(round_afz(2.5), 3.0);
        assert_eq!(round_afz(-0.5), -1.0);
        assert_eq!(round_afz(-1.5), -2.0);
        assert_eq!(round_afz(2.4), 2.0);
        assert_eq!(round_afz(0.0), 0.0);
    }

    #[test]
    fn quantize_exact_scale_domain() {
        // 可精确表示域：amax=1.0 → inv=127.0；0.5×127 = 63.5 精确半点
        // → half-away 取 64（浮点半点须可表示——0.635 类会落单侧）。
        let v = [1.0, -1.0, 0.0, 0.5];
        let mut q = [0i8; 4];
        let s = quantize_i8(&v, &mut q);
        assert_eq!(q, [127, -127, 0, 64]);
        assert!((s - 1.0 / 127.0).abs() < 1e-9);
    }

    #[test]
    fn quantize_zero_vector_degenerate() {
        let mut q = [7i8; 3];
        assert_eq!(quantize_i8(&[0.0; 3], &mut q), 0.0);
        assert_eq!(q, [0, 0, 0]);
        // 距离面退化：scale=0 → dot=0、L2²=|x|²。
        assert_eq!(dot_asym(&[1.0, 2.0], &q[..2], 0.0), Ok(0.0));
        assert_eq!(l2sq_asym(&[1.0, 2.0], &q[..2], 0.0), Ok(5.0));
    }

    #[test]
    fn asym_dot_matches_reconstruction() {
        let v = [0.5, -1.0, 0.25, 0.9, -0.3];
        let mut q = [0i8; 5];
        let s = quantize_i8(&v, &mut q);
        // 查询 = 原 vec：dot ≈ |v|²（量化误差 ≤ len × (scale/2)² 量级内）。
        let got = dot_asym(&v, &q, s).unwrap();
        let exact: f32 = v.iter().map(|a| a * a).sum();
        assert!((got - exact).abs() < 5.0 * s * s, "got={} exact={}", got, exact);
    }

    #[test]
    fn asym_dot_exact_integer_domain() {
        // 显式精确域：scale=0.01，q=[100, -50]，x=[2.0, 1.0]
        // → dot = 2.0×(0.01×100) + 1.0×(0.01×(-50)) = 2.0 - 0.5 = 1.5。
        let q = [100i8, -50];
        assert_eq!(dot_asym(&[2.0, 1.0], &q, 0.01), Ok(1.5));
        // L2²：(2-1)² + (1-(-0.5))² = 1 + 2.25 = 3.25。
        assert_eq!(l2sq_asym(&[2.0, 1.0], &q, 0.01), Ok(3.25));
    }

    #[test]
    fn errors_len_and_scale() {
        let q = [1i8, 2];
        assert_eq!(dot_asym(&[1.0], &q, 0.01), Err(VqError::LenMismatch));
        assert_eq!(l2sq_asym(&[1.0], &q, 0.01), Err(VqError::LenMismatch));
        assert_eq!(dot_asym(&[1.0, 1.0], &[1, 1], -0.1), Err(VqError::BadScale));
        assert_eq!(l2sq_asym(&[1.0, 1.0], &[1, 1], f32::NAN), Err(VqError::BadScale));
        assert_eq!(dot_asym(&[1.0, 1.0], &[1, 1], f32::INFINITY), Err(VqError::BadScale));
    }

    #[test]
    #[should_panic(expected = "长度失配")]
    fn quantize_len_mismatch_loud() {
        let mut q = [0i8; 2];
        let _ = quantize_i8(&[1.0], &mut q);
    }

    #[test]
    fn ranking_order_preserved() {
        // HNSW 检索排序的底座：量化距离的序在典型间隔下保真。
        let mut q1 = [0i8; 2];
        let s1 = quantize_i8(&[1.0, 0.0], &mut q1);
        let mut q2 = [0i8; 2];
        let s2 = quantize_i8(&[0.0, 0.9], &mut q2);
        let x = [0.9, 0.1];
        let d1 = l2sq_asym(&x, &q1, s1).unwrap();
        let d2 = l2sq_asym(&x, &q2, s2).unwrap();
        assert!(d1 < d2, "近邻序失真：d1={} d2={}", d1, d2);
    }
}
