//! berry-stylus——G4 刀 17：手写笔卡尔曼轨迹预测。
//!
//! 240Hz 采样+1D 卡尔曼滤波（位置+速度双通道）→ 向前一步预测。
//!
//! 纯面纪律：滤波/预测 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std)]

/// 1D 卡尔曼滤波器——位置+速度双状态。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kalman1D {
    // 状态
    pub x: f32,    // 位置
    pub v: f32,    // 速度
    // 协方差（2×2 对称——存三元素）
    pub p_xx: f32, pub p_xv: f32, pub p_vv: f32,
    // 过程噪声（加速度方差）
    pub q_a: f32,
    // 测量噪声（位置方差）
    pub r: f32,
    // 时间步长（秒）
    pub dt: f32,
}

impl Kalman1D {
    pub fn new(q_a: f32, r: f32, dt: f32) -> Kalman1D {
        Kalman1D { x: 0.0, v: 0.0, p_xx: 100.0, p_xv: 0.0, p_vv: 100.0, q_a, r, dt }
    }

    /// 预测步——外推状态与协方差。
    pub fn predict(&mut self) {
        // x' = x + v*dt
        let dt = self.dt;
        self.x += self.v * dt;
        // P' = F P F^T + Q
        // F = [[1, dt], [0, 1]]
        // Q = q_a * [[dt^4/4, dt^3/2], [dt^3/2, dt^2]]
        let p_xx = self.p_xx + dt * (self.p_xv + self.p_xv) + dt * dt * self.p_vv;
        let p_xv = self.p_xv + dt * self.p_vv;
        let p_vv = self.p_vv;
        self.p_xx = p_xx + self.q_a * dt * dt * dt * dt / 4.0;
        self.p_xv = p_xv + self.q_a * dt * dt * dt / 2.0;
        self.p_vv = p_vv + self.q_a * dt * dt;
    }

    /// 更新步——融合测量值。
    pub fn update(&mut self, z: f32) {
        // 创新
        let y = z - self.x;
        // S = P_xx + R
        let s = self.p_xx + self.r;
        // K = P / S
        let k_x = self.p_xx / s;
        let k_v = self.p_xv / s;
        // x += K * y
        self.x += k_x * y;
        self.v += k_v * y;
        // P = (I - K H) P
        let p_xx = (1.0 - k_x) * self.p_xx;
        let p_xv = (1.0 - k_x) * self.p_xv;
        let p_vv = self.p_vv - k_v * self.p_xv;
        self.p_xx = p_xx; self.p_xv = p_xv; self.p_vv = p_vv;
    }

    /// 向前 t 秒预测位置（不修改状态）。
    pub fn predict_ahead(&self, t: f32) -> f32 {
        self.x + self.v * t
    }
}

/// 2D 手写笔——双通道卡尔曼（x/y 独立）。
pub struct StylusTracker {
    pub kf_x: Kalman1D,
    pub kf_y: Kalman1D,
    /// 压力（0-4095）。
    pub pressure: u16,
    /// 倾角（度）。
    pub tilt: u16,
    /// 方位角（度）。
    pub azimuth: u16,
}

impl StylusTracker {
    /// 标准手写笔参数——240Hz 采样。
    pub fn stylus_240hz() -> StylusTracker {
        StylusTracker {
            kf_x: Kalman1D::new(1000.0, 1.0, 1.0 / 240.0),
            kf_y: Kalman1D::new(1000.0, 1.0, 1.0 / 240.0),
            pressure: 0, tilt: 0, azimuth: 0,
        }
    }

    /// 更新一帧——预测+测量融合。
    pub fn update(&mut self, x: f32, y: f32, pressure: u16, tilt: u16, azimuth: u16) {
        self.kf_x.predict();
        self.kf_y.predict();
        self.kf_x.update(x);
        self.kf_y.update(y);
        self.pressure = pressure;
        self.tilt = tilt;
        self.azimuth = azimuth;
    }

    /// 预测 ahead_ms 毫秒后的位置。
    pub fn predict_ahead(&self, ahead_ms: f32) -> (f32, f32) {
        let t = ahead_ms / 1000.0;
        (self.kf_x.predict_ahead(t), self.kf_y.predict_ahead(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kalman_converges_to_constant() {
        let mut kf = Kalman1D::new(1000.0, 1.0, 1.0 / 240.0);
        // 喂恒定位置 100.0——100 帧后应收敛
        for _ in 0..100 {
            kf.predict();
            kf.update(100.0);
        }
        assert!((kf.x - 100.0).abs() < 1.0, "x={}", kf.x);
        assert!(kf.v.abs() < 1.0, "v={}", kf.v);
    }

    #[test]
    fn kalman_tracks_velocity() {
        let mut kf = Kalman1D::new(1000.0, 1.0, 1.0 / 240.0);
        // 匀速运动 100px/s——200 帧（~0.83s）
        for i in 0..200 {
            kf.predict();
            let z = i as f32 * 100.0 / 240.0; // 100 px/s * dt * i
            kf.update(z);
        }
        assert!((kf.v - 100.0).abs() < 10.0, "v={}", kf.v);
    }

    #[test]
    fn stylus_predict_ahead() {
        let mut st = StylusTracker::stylus_240hz();
        // 匀速向右——x 递增 y 恒定
        for i in 0..200 {
            st.update(i as f32 * 0.5, 100.0, 1024, 45, 90);
        }
        // 预测 4ms（一帧）
        let (px, py) = st.predict_ahead(4.0);
        // 应在当前位置附近（≤2px 误差）
        let cx = st.kf_x.x;
        assert!((px - cx).abs() < 2.0, "px={} cx={}", px, cx);
        assert!((py - 100.0).abs() < 2.0);
    }

    #[test]
    fn stylus_pressure_range() {
        let mut st = StylusTracker::stylus_240hz();
        st.update(0.0, 0.0, 4095, 0, 0);
        assert_eq!(st.pressure, 4095);
        st.update(0.0, 0.0, 0, 90, 180);
        assert_eq!(st.pressure, 0);
    }
}
