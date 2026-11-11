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
pub use crate::layout::{CrossAlign, LayoutStat};
pub use crate::metrics::{
    EmptyIconSource, FontSource, IconSource, MonospaceTextMetrics, TextMetrics,
};
pub use crate::theme::{
    Density, InteractionState, RadiusToken, SpacingToken, StateColors, StrokeToken, Theme,
    ThemeColors, ThemeMetrics, TypographyToken,
};
pub use crate::widgets::{
    AppBar, Button, Card, EmptyState, ListItem, Scaffold, ScrollView, Switch, TextVariant,
    WidgetContext, column, divider, icon, row, text,
};
pub use crate::{FocusEntry, Ui, UiError, UiMain};

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
