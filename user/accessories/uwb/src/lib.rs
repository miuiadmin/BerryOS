//! berry-uwb——G4 刀 18：UWB ToF 测距与 PDoA 角度。
//!
//! ToF（飞行时间）→ 距离；PDoA（到达相位差）→ 方位角。
//! 光速 299,792,458 m/s；UWB 信道 6.5GHz≈信道 5/9。
//!
//! 纯面纪律：ToF/PDoA 计算 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std)]

/// 光速（m/s）。
pub const C: f32 = 299_792_458.0;

/// ToF 测距——往返时间（皮秒）→ 距离（米）。
pub fn tof_distance_ns(rt_ns: f32) -> f32 {
    // d = c * rt / 2（往返）
    C * rt_ns * 1e-9 / 2.0
}

/// PDoA 方位角——双天线相位差（度）→ 到达角（度）。
/// λ = c / f（f=6.5GHz → λ≈46mm）；d = 天线间距。
pub fn pdoa_angle(phase_deg: f32, antenna_mm: f32, freq_ghz: f32) -> f32 {
    let lambda_mm = C / (freq_ghz * 1e9) * 1e3;
    // sin(θ) = λ * Δφ / (2π * d)
    let ratio = (lambda_mm * phase_deg) / (360.0 * antenna_mm);
    // asin 近似（小角度域）——|ratio| ≤ 1 时有效
    if ratio.abs() > 1.0 { return 90.0 * ratio.signum(); }
    // 简单 asin 近似（v1 桩——真面用 berry-crypto 或 libm）
    ratio * 57.2958 // 度近似
}

/// UWB 测距结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangingResult {
    /// 距离（米）。
    pub distance_m: f32,
    /// 方位角（度）。
    pub angle_deg: f32,
    /// 置信度（0-1）。
    pub confidence: f32,
}

/// 三维定位——三锚点 ToF 三边测量。
pub fn trilaterate(anchors: &[(f32, f32, f32)], distances: &[f32]) -> Option<(f32, f32, f32)> {
    if anchors.len() < 3 || distances.len() < 3 {
        return None;
    }
    // 简化三边：取三对差分方程线性化
    let (x1, y1, z1) = anchors[0];
    let (x2, y2, z2) = anchors[1];
    let (x3, y3, z3) = anchors[2];
    let d1 = distances[0];
    let d2 = distances[1];
    let d3 = distances[2];

    // 线性方程 A*p = b（差分形式）
    let a11 = 2.0 * (x2 - x1); let a12 = 2.0 * (y2 - y1); let a13 = 2.0 * (z2 - z1);
    let a21 = 2.0 * (x3 - x1); let a22 = 2.0 * (y3 - y1); let a23 = 2.0 * (z3 - z1);
    let b1 = d1 * d1 - d2 * d2 + x2 * x2 - x1 * x1 + y2 * y2 - y1 * y1 + z2 * z2 - z1 * z1;
    let b2 = d1 * d1 - d3 * d3 + x3 * x3 - x1 * x1 + y3 * y3 - y1 * y1 + z3 * z3 - z1 * z1;

    // Cramer 法则解 2×2 投影（z 取均值近似——v1 桩）
    let det = a11 * a22 - a12 * a21;
    if det.abs() < 1e-10 { return None; }
    let px = (b1 * a22 - b2 * a12) / det;
    let py = (a11 * b2 - a21 * b1) / det;
    let pz = (z1 + z2 + z3) / 3.0;
    Some((px, py, pz))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tof_basic() {
        // 1 米往返 ≈ 6.67ns
        let d = tof_distance_ns(6.6712819);
        assert!((d - 1.0).abs() < 0.01, "d={}", d);
    }

    #[test]
    fn tof_zero() {
        assert_eq!(tof_distance_ns(0.0), 0.0);
    }

    #[test]
    fn pdoa_zero_offset() {
        // 零相位差 → 零角度
        assert!(pdoa_angle(0.0, 22.0, 6.5).abs() < 0.1);
    }

    #[test]
    fn pdoa_saturated() {
        // 极端相位差+极小天线 → 饱和 ±90°（λ≈46mm / 天线 0.01mm → ratio>>1）。
        let angle = pdoa_angle(360.0, 0.01, 6.5);
        assert!((angle - 90.0).abs() < 1.0, "angle={}", angle);
    }

    #[test]
    fn trilaterate_2d() {
        // 三锚点 (0,0), (10,0), (0,10)——目标 (3,4)
        let anchors = [(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (0.0, 10.0, 0.0)];
        let d_to = |x: f32, y: f32, ax: f32, ay: f32| ((x - ax) * (x - ax) + (y - ay) * (y - ay)).sqrt();
        let distances = [d_to(3.0, 4.0, 0.0, 0.0), d_to(3.0, 4.0, 10.0, 0.0), d_to(3.0, 4.0, 0.0, 10.0)];
        let (px, py, _) = trilaterate(&anchors, &distances).unwrap();
        assert!((px - 3.0).abs() < 0.5, "px={}", px);
        assert!((py - 4.0).abs() < 0.5, "py={}", py);
    }
}
