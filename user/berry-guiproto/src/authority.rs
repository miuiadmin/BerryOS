//! gui-wmd 权威策略纯函数（R1）。
//!
//! 这些函数不持窗口表，只裁决“某 client 类型能否造某层/某 parent/某
//! damage”。gui-wmd 消费；host 测试在这里覆盖安全边界，避免为窗口表
//! 引入第二实现。

use crate::{Rect, WM_CLIENT_APP, WM_CLIENT_SYSTEM, WM_CREATE_F_AVOID_SYS,
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
        && flags & !WM_CREATE_F_AVOID_SYS == 0
        && (client_type != WM_CLIENT_SYSTEM || flags & WM_CREATE_F_AVOID_SYS == 0)
}

/// 焦点策略：CREATE/SHOW 只让 APP 层接焦；SYSTEM 层是 overlay/输入法宿主，
/// 不得因创建或显示抢占键盘焦点。
pub fn wm_policy_focus_allowed(layer: u8) -> bool {
    layer == WM_LAYER_APP
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
