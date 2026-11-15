//! berry-ui 组件层（刀 3，OH 形）：声明式树直接拼「标题栏 + 卡片列表 +
//! 底部页签」页的公共装配面——TopBar / BottomTabs / ListItem。
//!
//! 设计纪律（分刀方案 §刀 3）：
//! - **纯 ViewSpec 构造器 + Ui 装配助手**：组件层零新状态、零新 wire 面
//!   （radius 圆角与预混色在原语层，组件只组参数）。
//! - **OH 参数单源**：TopBar 56vp / BottomTabs 48vp / 卡片圆角 16vp /
//!   按压 20% 暗化——全部按 `density`（px/vp）换算，消费方传档位。
//! - **五态样式**（normal/pressed/hover/focus/disabled）v1 落两态
//!   （normal/pressed）——其余态随输入体系后续刀位；API 预留
//!   `pressed: bool` 形参，后续态零改签名。
//!
//! 消费方：launcher（桌面 dock 卡片化）、appdemo（示范应用页——刀 5）、
//! OOBE（刀 6 复用）。测试：tests/components.rs。

use crate::desc::{
    RectProps, TextProps, TextStyle, ViewKind, ViewSpec, Props,
};
use crate::TextBuf;
use crate::Ui;
use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;

// —— OH 视觉 token（单源常量；消费方经 density 换算 px）——

/// OH TopBar 高 56vp。
pub const TOPBAR_VP: u32 = 56;
/// OH BottomTabs 高 48vp。
pub const TABS_VP: u32 = 48;
/// OH 卡片圆角 16vp（radius 单位 = px——调用方传 `16 * density`）。
pub const CARD_RADIUS_VP: u32 = 16;
/// OH 列表卡内边距 16vp。
pub const CARD_PAD_VP: u32 = 16;
/// 列表卡间隙 12vp。
pub const CARD_GAP_VP: u32 = 12;

/// 页底色（OH 浅色页面背景）。
pub const PAGE_BG: Color = Color { r: 0xF1, g: 0xF3, b: 0xF5 };
/// 卡片底色（OH 白卡）。
pub const CARD_BG: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// 主文本色（OH 黑字）。
pub const TEXT_PRIMARY: Color = Color { r: 0x18, g: 0x24, b: 0x31 };
/// 次文本色（OH 灰字）。
pub const TEXT_SECONDARY: Color = Color { r: 0x66, g: 0x6F, b: 0x7A };
/// 分隔线（OH 灰白线）。
pub const DIVIDER: Color = Color { r: 0xE5, g: 0xE8, b: 0xEB };
/// 按压态底色（20% 黑预混白卡）。
pub const CARD_PRESSED_BG: Color = Color { r: 0xCC, g: 0xCC, b: 0xCC };

/// 预混原语（刀 3）：`fg` 按 `alpha/255` 混入 `under`——no-alpha 渲染面
/// （v1 纯不透明）的「半透明遮罩」等价实现。alpha=255 → fg；0 → under。
pub fn mix_under(fg: Color, under: Color, alpha: u8) -> Color {
    let a = alpha as u16;
    let mix_ch = |f: u8, u: u8| -> u8 {
        ((f as u16 * a + u as u16 * (255 - a)) / 255) as u8
    };
    Color {
        r: mix_ch(fg.r, under.r),
        g: mix_ch(fg.g, under.g),
        b: mix_ch(fg.b, under.b),
    }
}

/// 卡片按压底色（消费方背景自适应形）：20% 黑预混 `base`。
pub fn card_pressed_bg(base: Color) -> Color {
    mix_under(Color { r: 0, g: 0, b: 0 }, base, 51)
}

/// 卡片容器 Spec（白底圆角 16vp；`pressed` = 按压态底色 20% 黑预混）。
pub fn card_spec(density: u32, pressed: bool) -> ViewSpec {
    let radius = (CARD_RADIUS_VP * density).min(u16::MAX as u32) as u16;
    let bg = if pressed { CARD_PRESSED_BG } else { CARD_BG };
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg, w: None, h: None, radius }),
    }
}

/// TopBar 底条 Spec（56vp 高、页面底色、随父宽）。
pub fn topbar_spec(density: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: PAGE_BG,
            w: None,
            h: Some(TOPBAR_VP * density),
            radius: 0,
        }),
    }
}

/// TopBar 标题文本 Spec（黑字 16×32 档；S32x48 大屏档由调用方换 scale）。
pub fn topbar_title_spec(title: &[u8], scale: FontSize) -> Result<ViewSpec, crate::UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: TEXT_PRIMARY, bg: None, scale },
            text: TextBuf::new(title)?,
        }),
    })
}

/// BottomTabs 底条 Spec（48vp 高、白底、随父宽）。
pub fn bottom_tabs_spec(density: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: CARD_BG,
            w: None,
            h: Some(TABS_VP * density),
            radius: 0,
        }),
    }
}

/// ListItem 完整装配（刀 3 验收面）：白卡圆角 16vp 内「图标 + 标题 +
/// 副标题」横排——返回卡片根 node（调用方拿它挂进列表容器）。
///
/// 结构：Rect(卡片) → Pad(CARD_PAD_VP×density) → Row{gap 12} →
/// Icon(48vp) + Col{gap 2} → Text(标题) + Text(副标题·次色)。
pub fn add_list_item(
    ui: &mut crate::UiMain,
    parent: crate::NodeId,
    density: u32,
    icon: u16,
    title: &[u8],
    subtitle: &[u8],
    pressed: bool,
) -> Result<crate::NodeId, crate::UiError> {
    use crate::desc::{ColProps, IconProps, PadProps, RowProps};
    use crate::layout::CrossAlign;
    // 结构（BadParent 修复 2026-11-15）：Rect 是叶子不容子——卡片 =
    // Pad(all=0) 双子叠加：Rect(白底圆角) 底 + Pad(内边距) 内容层盖上
    //（Pad 对所有子同位全幅分配 = 叠加语义；内容层再自带 16vp 内缩）。
    let card = ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Pad,
            key: 0,
            props: Props::Pad(PadProps { all: 0 }),
        },
    )?;
    let _face = ui.add(card, card_spec(density, pressed))?;
    let pad = ui.add(
        card,
        ViewSpec {
            kind: ViewKind::Pad,
            key: 0,
            props: Props::Pad(PadProps { all: CARD_PAD_VP * density }),
        },
    )?;
    let row = ui.add(
        pad,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 12 * density,
                w: None,
                h: None,
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let _icon = ui.add(
        row,
        ViewSpec {
            kind: ViewKind::Icon,
            key: 0,
            props: Props::Icon(IconProps {
                id: icon,
                fg: TEXT_PRIMARY,
                w: Some(48 * density),
                h: Some(48 * density),
            }),
        },
    )?;
    let col = ui.add(
        row,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 2 * density,
                w: None,
                h: None,
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let t = ui.add(
        col,
        ViewSpec {
            kind: ViewKind::Text,
            key: 0,
            props: Props::Text(TextProps {
                style: TextStyle { fg: TEXT_PRIMARY, bg: None, scale: FontSize::S16x32 },
                text: TextBuf::new(title)?,
            }),
        },
    )?;
    let _ = t;
    let st = ui.add(
        col,
        ViewSpec {
            kind: ViewKind::Text,
            key: 0,
            props: Props::Text(TextProps {
                style: TextStyle { fg: TEXT_SECONDARY, bg: None, scale: FontSize::S8x16 },
                text: TextBuf::new(subtitle)?,
            }),
        },
    )?;
    let _ = st;
    Ok(card)
}

// —— 刀 4 增补（三家并集矩阵 C/D/F 组首批）：Button / Switch / 线性加载条 ——

/// OH/M3 主按钮高 40vp（M3 FilledButton 40dp 同值——三家合体归并点）。
pub const BUTTON_H_VP: u32 = 40;
/// OH 强调色 #007DFF（浅色）——按钮/选中态共用。
pub const ACCENT: Color = Color { r: 0x00, g: 0x7D, b: 0xFF };
/// 按钮白字。
pub const ON_ACCENT: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// 开关轨道宽 52vp（OH Switch 52×28vp 官方值）。
pub const SWITCH_W_VP: u32 = 52;
/// 开关轨道高 28vp。
pub const SWITCH_H_VP: u32 = 28;
/// 开关滑块直径 24vp（28 轨 - 2×2 边距）。
pub const SWITCH_KNOB_VP: u32 = 24;

/// 主按钮（三家并集：M3 FilledButton / iOS Button / OH Button 归并）：
/// 强调色底 + 白字 + 20vp 圆角（OH 按钮圆角 token）。`pressed` 态 =
/// 20% 黑预混强调底（press_map 消费面同卡）。
pub fn button_spec(density: u32, label: &[u8], pressed: bool) -> Result<(ViewSpec, ViewSpec), crate::UiError> {
    let bg = if pressed { card_pressed_bg(ACCENT) } else { ACCENT };
    let face = ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg,
            w: None,
            h: Some(BUTTON_H_VP * density),
            radius: (20 * density) as u16,
        }),
    };
    let text = ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: ON_ACCENT, bg: None, scale: FontSize::S16x32 },
            text: TextBuf::new(label)?,
        }),
    };
    Ok((face, text))
}

/// 开关（三家并集：M3 Switch / iOS SwitchToggle / OH ToggleSwitch）：
/// 轨道胶囊（52×28vp）+ 滑块圆（24vp）。`on` 态轨道 = 强调色、滑块右置；
/// off 态轨道 = 次背景灰、滑块左置。返回 (轨道, 滑块)——挂 Pad 叠加容器。
pub fn switch_spec(density: u32, on: bool) -> (ViewSpec, ViewSpec) {
    let track_bg = if on { ACCENT } else { DIVIDER };
    let track = ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: track_bg,
            w: Some(SWITCH_W_VP * density),
            h: Some(SWITCH_H_VP * density),
            radius: u16::MAX, // 胶囊——paint 侧 r = min(w,h)/2 截断
        }),
    };
    let knob = ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: CARD_BG,
            w: Some(SWITCH_KNOB_VP * density),
            h: Some(SWITCH_KNOB_VP * density),
            radius: u16::MAX, // 圆
        }),
    };
    (track, knob)
}

/// 线性加载条（三家并集：M3 LinearProgressIndicator / iOS ProgressView /
/// OH LoadingProgress）：底槽 + 填充胶囊（f32 比例 0..=1）。
pub fn progress_spec(density: u32, ratio: f32) -> (ViewSpec, ViewSpec) {
    let r = ratio.clamp(0.0, 1.0);
    let h = 8 * density;
    let track = ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: DIVIDER,
            w: None,
            h: Some(h),
            radius: u16::MAX,
        }),
    };
    let filled = ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: ACCENT,
            w: Some((r * 100.0) as u32 * density), // 调用方按容器宽折算——v1 定宽形
            h: Some(h),
            radius: u16::MAX,
        }),
    };
    (track, filled)
}

/// 开关完整装配（叠加容器 + 轨道 + 滑块）——返回容器（a11y 锚点）。
pub fn add_switch(
    ui: &mut crate::UiMain,
    parent: crate::NodeId,
    density: u32,
    on: bool,
) -> Result<crate::NodeId, crate::UiError> {
    use crate::desc::PadProps;
    let holder = ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Pad,
            key: 0,
            props: Props::Pad(PadProps { all: 2 * density }),
        },
    )?;
    let (track, knob) = switch_spec(density, on);
    let _track = ui.add(holder, track)?;
    // 滑块定位：on 右置（x 偏移 = 轨宽-球径-边距）；off 左置（0）。
    let knob_off = if on { (SWITCH_W_VP - SWITCH_KNOB_VP) * density } else { 0 };
    let k = ui.add(holder, knob)?;
    ui.set_offset(k, knob_off as i32, 0)?;
    Ok(holder)
}
