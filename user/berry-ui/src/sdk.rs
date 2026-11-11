//! BerryUI SDK 冻结面（F2 core + F4 组件 + F5 guest 供给槽）。
//!
//! F4 组件与 F5 uitest 只从本 facade 引用 core API；新增类型可以先留在
//! 具体模块，但进入本 `sdk` re-export 前必须满足三件事：Plain Data 或
//! `&mut self` 引擎门面、host 值级测试、no_std 零分配。要破坏这里的名字
//! 或签名，先改 F2/F4 对应规格的接口表。

pub use crate::damage::RectI;
pub use crate::desc::{
    Axis, CircleProps, ClipProps, ColProps, FocusDir, GridProps, IconProps, ImageProps, Msg,
    MsgBuf, NodeId, PadProps, PanelProps, Props, RectProps, RowProps, TextBuf, TextProps,
    TextStyle, ViewKind, ViewSpec,
};
pub use crate::diagnostic::{HitDumpEntry, LayoutDumpEntry};
pub use crate::layout::{Constraints, CrossAlign, LayoutStat};
pub use crate::metrics::{
    EmptyIconSource, FontSource, IconSource, MonospaceTextMetrics, TextMetrics,
};
pub use crate::theme::{
    Density, InteractionState, RadiusToken, SpacingToken, StateColors, StrokeToken, Theme,
    ThemeColors, ThemeMetrics, TypographyToken,
};
pub use crate::text_input::{
    ime_overlap_px, text_key_intent, TextInputEdit, TextInputKeyOp, TextInputKind,
    TextInputOutcome, TextInputState, TextSelection, TEXT_INPUT_MAX,
};
pub use crate::widgets::{
    AppBar, Button, Card, EmptyState, ListDetail, ListDetailSpec, ListItem, Scaffold, ScrollView,
    Switch, TextVariant, TextField, TextFieldSpec,
    WidgetContext, column, divider, icon, row, text,
};
pub use crate::{FlexEntry, FocusEntry, Ui, UiError, UiMain};

/// L1 自适应槽位判定结果（显式本地基准宽，不读全局屏幕/设备档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotVariant {
    Compact,
    Expanded,
}

/// L1 自适应槽位判定：`basis_w >= threshold` → Expanded（半开区间同
/// RectI 纪律）。basis 由调用方明示（raw 或 usable 均可，口径归调用
/// 方）；签名无 Ui/Room 参数，编译期不可能隐式读全局屏幕。
pub fn slot_variant(basis_w: u32, threshold: u32) -> SlotVariant {
    if basis_w >= threshold {
        SlotVariant::Expanded
    } else {
        SlotVariant::Compact
    }
}

/// component focus 键意图（纯函数；引擎 FocusDir 与 keycode 分域）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusOp {
    Next,
    Prev,
    Activate,
}

/// Linux keymap 最小面：Tab/Shift+Tab 一维导航，Enter/Space 激活。
/// UP 恒 None；Tab 只认 DOWN（防 REPEAT 连发），Activate 允许 REPEAT。
pub fn focus_key_intent(code: u8, action: u8, modifiers: u8) -> Option<FocusOp> {
    use berry_guiproto::{KEY_ACTION_DOWN, KEY_ACTION_REPEAT, KEY_ACTION_UP, KEY_MOD_SHIFT};
    if action == KEY_ACTION_UP {
        return None;
    }
    match code {
        15 if action == KEY_ACTION_DOWN => Some(if modifiers & KEY_MOD_SHIFT != 0 {
            FocusOp::Prev
        } else {
            FocusOp::Next
        }),
        28 | 57 if action == KEY_ACTION_DOWN || action == KEY_ACTION_REPEAT => {
            Some(FocusOp::Activate)
        }
        _ => None,
    }
}

/// C1 + GUI-T1 的总键路由。`text_focused` 由调用方以 C1 component focus
/// 是否落在 TextField 语义锚判定；窗口焦点仍在 wmd。
///
/// 裁决：焦点在 TextField 时，Enter/Space 先进入文本状态机（Submit /
/// 字面空格），不再合成组件 Activate；Tab/Shift+Tab 永远交还 C1 导航。
/// 非文本焦点保持既有 C1 keymap 零漂移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRoute {
    Text(TextInputKeyOp),
    Focus(FocusOp),
    None,
}

pub fn key_route(
    code: u8,
    action: u8,
    modifiers: u8,
    kind: TextInputKind,
    text_focused: bool,
) -> KeyRoute {
    if !text_focused {
        return match focus_key_intent(code, action, modifiers) {
            Some(op) => KeyRoute::Focus(op),
            None => KeyRoute::None,
        };
    }
    match text_key_intent(code, action, modifiers, kind) {
        Some(TextInputKeyOp::FocusNext) => KeyRoute::Focus(FocusOp::Next),
        Some(TextInputKeyOp::FocusPrev) => KeyRoute::Focus(FocusOp::Prev),
        Some(op) => KeyRoute::Text(op),
        None => KeyRoute::None,
    }
}
