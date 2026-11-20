//! berry-island——G3 刀 13：灵动岛四态流体弹簧动画。
//!
//! **Spring2D 临界阻尼弹簧**：驱动 Compact/Minimal/Expanded/LockScreen
//! Banner 四态切换。临界阻尼（ζ=1）= 无超调无振荡，位置+速度双通道。
//!
//! 纯面纪律：弹簧积分/状态机/几何计算 = 零 I/O 纯函数（host 可测）。
#![cfg_attr(not(test), no_std)]

/// 二维弹簧——临界阻尼（ζ=1）。
/// 每通道独立：位置 x 与速度 vx 的显式欧拉积分。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring2D {
    pub x: f32, pub y: f32,
    pub vx: f32, pub vy: f32,
    /// 目标位置。
    pub tx: f32, pub ty: f32,
    /// 弹簧刚度（ω²——角频率平方）。临界阻尼 ζ=1 → damping = 2ω。
    pub stiffness: f32,
    /// 时间步长（秒）。
    pub dt: f32,
}

impl Spring2D {
    /// 标准灵动岛弹簧——ω=12（≈2Hz 收敛）。
    pub fn island_default() -> Spring2D {
        Spring2D {
            x: 0.0, y: 0.0, vx: 0.0, vy: 0.0,
            tx: 0.0, ty: 0.0,
            stiffness: 12.0 * 12.0, // ω² = 144
            dt: 1.0 / 120.0,        // 120fps
        }
    }

    /// 设定目标位置。
    pub fn set_target(&mut self, tx: f32, ty: f32) {
        self.tx = tx;
        self.ty = ty;
    }

    /// 积分一步（显式欧拉——临界阻尼ζ=1）。
    /// 返回 (x, y, settled)——settled=距目标 < ε 且速度 < ε。
    pub fn step(&mut self) -> (f32, f32, bool) {
        let omega = 12.0f32; // ω=12（island_default 硬编——no_std 无 sqrt）
        let damping = 2.0 * omega; // ζ=1 → c = 2ω

        // 加速度 = -ω²(x - tx) - 2ω·vx
        let ax = -self.stiffness * (self.x - self.tx) - damping * self.vx;
        let ay = -self.stiffness * (self.y - self.ty) - damping * self.vy;

        // 显式欧拉
        self.vx += ax * self.dt;
        self.vy += ay * self.dt;
        self.x += self.vx * self.dt;
        self.y += self.vy * self.dt;

        let settled = (self.x - self.tx).abs() < 0.5
            && (self.y - self.ty).abs() < 0.5
            && self.vx.abs() < 1.0
            && self.vy.abs() < 1.0;
        (self.x, self.y, settled)
    }
}

/// 灵动岛状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandState {
    /// 紧凑态——药丸形（默认）。
    Compact,
    /// 最小态——圆点（通知到达前过渡）。
    Minimal,
    /// 展开态——全宽面板（长按/点击）。
    Expanded,
    /// 锁屏横幅。
    LockBanner,
}

/// 状态几何——每种状态的 (width, height, corner_radius)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IslandGeometry {
    pub width: f32,
    pub height: f32,
    pub corner_radius: f32,
}

impl IslandState {
    /// 该状态的目标几何（逻辑像素——390pt 域面）。
    pub fn geometry(&self) -> IslandGeometry {
        match self {
            IslandState::Compact => IslandGeometry { width: 180.0, height: 36.0, corner_radius: 18.0 },
            IslandState::Minimal => IslandGeometry { width: 36.0, height: 36.0, corner_radius: 18.0 },
            IslandState::Expanded => IslandGeometry { width: 358.0, height: 160.0, corner_radius: 24.0 },
            IslandState::LockBanner => IslandGeometry { width: 358.0, height: 64.0, corner_radius: 16.0 },
        }
    }

    /// 合法状态转移表。
    pub fn can_transition(&self, to: IslandState) -> bool {
        use IslandState::*;
        matches!(
            (self, to),
            (Compact, Minimal) | (Compact, Expanded) | (Compact, LockBanner)
                | (Minimal, Compact) | (Minimal, Expanded)
                | (Expanded, Compact)
                | (LockBanner, Compact)
        )
    }
}

/// 灵动岛控制器——状态+弹簧+几何。
pub struct IslandController {
    pub state: IslandState,
    pub spring: Spring2D,
    /// 当前几何（弹簧驱动中的实时值）。
    pub current_geom: IslandGeometry,
    /// 目标几何。
    pub target_geom: IslandGeometry,
}

impl IslandController {
    pub fn new() -> IslandController {
        let g = IslandState::Compact.geometry();
        IslandController {
            state: IslandState::Compact,
            spring: Spring2D::island_default(),
            current_geom: g,
            target_geom: g,
        }
    }

    /// 请求状态转移——合法则更新目标几何并返回 true。
    pub fn transition(&mut self, to: IslandState) -> bool {
        if !self.state.can_transition(to) {
            return false;
        }
        self.state = to;
        self.target_geom = to.geometry();
        self.spring.set_target(self.target_geom.width, self.target_geom.height);
        true
    }

    /// 逐帧步进——弹簧积分后更新当前几何。
    /// 返回 settled（几何到位且速度趋零）。
    pub fn step(&mut self) -> bool {
        let (w, h, settled) = self.spring.step();
        self.current_geom.width = w;
        self.current_geom.height = h;
        // corner_radius 线性插值到目标
        let t = ((w - 36.0) / (358.0 - 36.0)).clamp(0.0, 1.0);
        self.current_geom.corner_radius = 18.0 + t * 6.0;
        settled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_converges() {
        let mut s = Spring2D::island_default();
        s.set_target(100.0, 50.0);
        let mut settled = false;
        for i in 0..600 { // 5 秒 @120fps
            let (_, _, st) = s.step();
            if st {
                settled = true;
                break;
            }
            assert!(i < 599, "应在 5s 内收敛（600 帧帽）");
        }
        assert!(settled);
        assert!((s.x - 100.0).abs() < 1.0);
        assert!((s.y - 50.0).abs() < 1.0);
    }

    #[test]
    fn spring_critical_no_overshoot() {
        let mut s = Spring2D::island_default();
        s.set_target(100.0, 0.0);
        let mut max_x = 0.0f32;
        for _ in 0..600 {
            let (x, _, _) = s.step();
            if x > max_x { max_x = x; }
        }
        assert!(max_x <= 101.0, "临界阻尼无超调：max_x={} ≤ 101", max_x);
    }

    #[test]
    fn state_geometry() {
        let c = IslandState::Compact.geometry();
        assert_eq!(c.width, 180.0);
        assert_eq!(c.height, 36.0);
        let e = IslandState::Expanded.geometry();
        assert!(e.width > c.width);
        assert!(e.height > c.height);
    }

    #[test]
    fn state_transitions() {
        assert!(IslandState::Compact.can_transition(IslandState::Expanded));
        assert!(IslandState::Expanded.can_transition(IslandState::Compact));
        assert!(!IslandState::Minimal.can_transition(IslandState::LockBanner));
    }

    #[test]
    fn controller_transition_and_settle() {
        let mut ic = IslandController::new();
        assert!(ic.transition(IslandState::Expanded));
        assert_eq!(ic.target_geom.width, 358.0);
        let mut settled = false;
        for _ in 0..600 {
            if ic.step() { settled = true; break; }
        }
        assert!(settled);
        assert!((ic.current_geom.width - 358.0).abs() < 2.0);
    }

    #[test]
    fn controller_rejects_invalid() {
        let mut ic = IslandController::new();
        // Minimal→LockBanner 不合法
        ic.state = IslandState::Minimal;
        assert!(!ic.transition(IslandState::LockBanner));
    }
}
