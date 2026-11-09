//! BerryUI SDK 冻结面（F2 core + F4 组件 + F5 guest 供给槽）。
//!
//! F4 组件与 F5 uitest 只从本 facade 引用 core API；新增类型可以先留在
//! 具体模块，但进入本 `sdk` re-export 前必须满足三件事：Plain Data 或
//! `&mut self` 引擎门面、host 值级测试、no_std 零分配。要破坏这里的名字
//! 或签名，先改 F2/F4 对应规格的接口表。

pub use crate::damage::RectI;
pub use crate::desc::{
    Axis, CircleProps, ClipProps, ColProps, GridProps, IconProps, ImageProps, Msg, MsgBuf, NodeId,
    PadProps, PanelProps, Props, RectProps, RowProps, TextBuf, TextProps, TextStyle, ViewKind,
    ViewSpec,
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
pub use crate::{Ui, UiError, UiMain};
