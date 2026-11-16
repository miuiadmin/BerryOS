//! gui-wmd 权威策略纯函数（R1）。
//!
//! 这些函数不持窗口表，只裁决“某 client 类型能否造某层/某 parent/某
//! damage”。gui-wmd 消费；host 测试在这里覆盖安全边界，避免为窗口表
//! 引入第二实现。

use crate::{Rect, WM_CLIENT_APP, WM_CLIENT_SYSTEM, WM_CREATE_F_AVOID_SYS,
    WM_CREATE_F_EDGE_TO_EDGE,
            WM_LAYER_APP, WM_LAYER_SYSTEM};

/// WM_CREATE 层权限：APP 恒 APP 层；SYSTEM 可造 SYSTEM 层（系统壳/IME
/// 宿主），但不得再造普通 APP 层，避免系统会话伪装普通 owner。
pub fn wm_create_layer_allowed(client_type: u8, layer: u8) -> bool {
    match client_type {
        WM_CLIENT_APP => layer == WM_LAYER_APP,
        WM_CLIENT_SYSTEM => layer == WM_LAYER_SYSTEM,
        _ => false,
    }
}

/// parent 权限：0 表示顶层合法；非 0 parent 必须在册、同 owner，且不得
/// 指向自身（由调用方传入 parent_owner/current id）。
pub fn wm_parent_allowed(parent: u16, parent_owner: Option<u16>, owner: u16, id: u16) -> bool {
    parent == 0 || (parent != id && parent_owner == Some(owner))
}

/// AVOID_SYS 只对 APP 层有语义；SYSTEM 层自身携带该旗一律拒绝。
pub fn wm_create_flags_allowed(client_type: u8, flags: u8) -> bool {
    let known = client_type == WM_CLIENT_APP || client_type == WM_CLIENT_SYSTEM;
    known
        && flags & !(WM_CREATE_F_AVOID_SYS | WM_CREATE_F_EDGE_TO_EDGE) == 0
        && (flags & (WM_CREATE_F_AVOID_SYS | WM_CREATE_F_EDGE_TO_EDGE)
            != WM_CREATE_F_AVOID_SYS | WM_CREATE_F_EDGE_TO_EDGE)
        && (client_type != WM_CLIENT_SYSTEM
            || flags & (WM_CREATE_F_AVOID_SYS | WM_CREATE_F_EDGE_TO_EDGE) == 0)
}

/// 焦点策略：CREATE/SHOW 只让 APP 层接焦；SYSTEM 层是 overlay/输入法宿主，
/// 不得因创建或显示抢占键盘焦点。
pub fn wm_policy_focus_allowed(layer: u8) -> bool {
    layer == WM_LAYER_APP
}

/// R2 surface 双几何：`raw` 是客户端申请/分配 buffer 的窗口矩形；
/// `visible` 是 wmd 遮挡推导后的可呈现/可命中矩形。二者必须同源出自
/// WmTable，禁止客户端或 compositor 各自推导第二份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceGeometry {
    raw_x: u16,
    raw_y: u16,
    raw_w: u16,
    raw_h: u16,
    visible_x: u16,
    visible_y: u16,
    visible_w: u16,
    visible_h: u16,
}

impl SurfaceGeometry {
    /// raw 窗口矩形（buffer/stride/layout 基准）。
    pub fn raw_bounds(&self) -> (u16, u16, u16, u16) {
        (self.raw_x, self.raw_y, self.raw_w, self.raw_h)
    }

    /// raw 尺寸（宽、高）。
    pub fn raw_size(&self) -> (u32, u32) {
        (u32::from(self.raw_w), u32::from(self.raw_h))
    }
    /// 校验并构造双几何。不变量：visible ⊆ raw；raw/visible 尺寸非零。
    /// 全遮挡窗由调用侧以 `None`/ST_DENIED 处理，不构造 w/h=0 假几何。
    pub fn new(
        raw: (u16, u16, u16, u16),
        visible: (u16, u16, u16, u16),
    ) -> Option<Self> {
        let (rx, ry, rw, rh) = (u32::from(raw.0), u32::from(raw.1), u32::from(raw.2), u32::from(raw.3));
        let (vx, vy, vw, vh) = (u32::from(visible.0), u32::from(visible.1), u32::from(visible.2), u32::from(visible.3));
        if rw == 0 || rh == 0 || vw == 0 || vh == 0
            || vx < rx
            || vy < ry
            || vx.saturating_add(vw) > rx.saturating_add(rw)
            || vy.saturating_add(vh) > ry.saturating_add(rh)
        {
            return None;
        }
        Some(Self {
            raw_x: raw.0,
            raw_y: raw.1,
            raw_w: raw.2,
            raw_h: raw.3,
            visible_x: visible.0,
            visible_y: visible.1,
            visible_w: visible.2,
            visible_h: visible.3,
        })
    }

    /// visible 窗口全局矩形（合成/overlay 命中消费面）。
    pub fn visible_bounds(&self) -> (u16, u16, u16, u16) {
        (self.visible_x, self.visible_y, self.visible_w, self.visible_h)
    }

    /// visible 在 raw buffer 坐标系中的本地矩形。SUBMIT 的源 offset/stride
    /// 必须以此为准：top 遮挡时从 raw row `visible_y-raw_y` 开始读，而不是
    /// 从 row 0 重排。
    pub fn visible_local(&self) -> Rect {
        Rect {
            x: u32::from(self.visible_x - self.raw_x),
            y: u32::from(self.visible_y - self.raw_y),
            w: u32::from(self.visible_w),
            h: u32::from(self.visible_h),
        }
    }

    /// persistent occlusion insets（raw buffer 坐标系）。这不是 IME/gesture
    /// 的完整 typed inset 词表；R2 只暴露 AVOID_SYS 持久遮挡。
    pub fn occlusion_insets(&self) -> (u16, u16, u16, u16) {
        let left = self.visible_x - self.raw_x;
        let top = self.visible_y - self.raw_y;
        let raw_right = u32::from(self.raw_x) + u32::from(self.raw_w);
        let visible_right = u32::from(self.visible_x) + u32::from(self.visible_w);
        let raw_bottom = u32::from(self.raw_y) + u32::from(self.raw_h);
        let visible_bottom = u32::from(self.visible_y) + u32::from(self.visible_h);
        let right = raw_right.saturating_sub(visible_right);
        let bottom = raw_bottom.saturating_sub(visible_bottom);
        (left, top, right as u16, bottom as u16)
    }

    /// damage 先裁到 raw buffer，再与 visible 本地矩形求交。输入仍是
    /// raw surface-local 坐标；被持久遮挡的区域不得进入 present。
    pub fn clip_damage_to_visible(&self, r: Rect) -> Option<Rect> {
        let base = clip_damage_to_surface(
            r,
            u32::from(self.raw_w),
            u32::from(self.raw_h),
        )?;
        let visible = self.visible_local();
        let x0 = base.x.max(visible.x);
        let y0 = base.y.max(visible.y);
        let x1 = base.x.saturating_add(base.w).min(visible.x.saturating_add(visible.w));
        let y1 = base.y.saturating_add(base.h).min(visible.y.saturating_add(visible.h));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
    }
}

/// damage 必须先与本 surface 的本地矩形求交；空矩形丢弃。返回的矩形仍
/// 保持 surface-local 坐标，供 compositor 加窗口原点后送 present。
pub fn clip_damage_to_surface(r: Rect, w: u32, h: u32) -> Option<Rect> {
    if w == 0 || h == 0 {
        return None;
    }
    let x0 = r.x.min(w);
    let y0 = r.y.min(h);
    let x1 = r.x.saturating_add(r.w).min(w);
    let y1 = r.y.saturating_add(r.h).min(h);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_to_edge_is_app_only_and_composes_with_avoid_sys_domain() {
        assert!(wm_create_flags_allowed(WM_CLIENT_APP, WM_CREATE_F_EDGE_TO_EDGE));
        assert!(!wm_create_flags_allowed(
            WM_CLIENT_SYSTEM,
            WM_CREATE_F_EDGE_TO_EDGE
        ));
        // Both mode bits together is ambiguous and rejected without assigning
        // a second precedence rule.
        assert!(!wm_create_flags_allowed(
            WM_CLIENT_APP,
            WM_CREATE_F_AVOID_SYS | WM_CREATE_F_EDGE_TO_EDGE
        ));
        assert!(!wm_create_flags_allowed(WM_CLIENT_APP, 0x04));
    }

    #[test]
    fn surface_geometry_exposes_visible_bounds_without_second_derivation() {
        let geometry = SurfaceGeometry::new((10, 20, 100, 200), (20, 30, 80, 180)).unwrap();
        assert_eq!(geometry.visible_bounds(), (20, 30, 80, 180));
    }
}
