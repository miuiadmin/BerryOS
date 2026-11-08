//! BerryUI SDK 最小冻结面（GUI F2）。
//!
//! F4 组件与 F5 uitest 只从本 facade 引用 core API；新增类型可以先留在
//! 具体模块，但进入本 `sdk` re-export 前必须满足三件事：Plain Data 或
//! `&mut self` 引擎门面、host 值级测试、no_std 零分配。要破坏这里的名字
//! 或签名，先改 `docs/BerryUI-F2核心盘点-v1.0.md` 的接口表。

pub use crate::damage::RectI;
pub use crate::desc::{
    Axis, CircleProps, ColProps, GridProps, IconProps, ImageProps, Msg, MsgBuf, NodeId,
    PadProps, Props, RectProps, RowProps, TextBuf, TextProps, TextStyle, ViewKind, ViewSpec,
};
pub use crate::diagnostic::{HitDumpEntry, LayoutDumpEntry};
pub use crate::layout::{CrossAlign, LayoutStat};
pub use crate::metrics::{EmptyIconSource, IconSource, MonospaceTextMetrics, TextMetrics};
pub use crate::{Ui, UiError, UiMain};
