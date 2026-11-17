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
pub use crate::theme::mix_under;

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;

pub use crate::theme::{
    COMPONENT_ACCENT as ACCENT, COMPONENT_CARD_BG as CARD_BG,
    COMPONENT_CARD_PRESSED_BG as CARD_PRESSED_BG, COMPONENT_DIVIDER as DIVIDER,
    COMPONENT_ON_ACCENT as ON_ACCENT, COMPONENT_PAGE_BG as PAGE_BG,
    COMPONENT_TEXT_PRIMARY as TEXT_PRIMARY, COMPONENT_TEXT_SECONDARY as TEXT_SECONDARY,
};

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


/// 卡片按压底色（消费方背景自适应形）：20% 黑预混 `base`。
pub fn card_pressed_bg(base: Color) -> Color {
    mix_under(crate::theme::COMPONENT_BLACK, base, 51)
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

// —— AppShell（刀 4）：应用页脚手架——TopBar 返回键+标题 / 内容槽 /
//    可选 TabBar 底栏（三家并集 E 组落地：TopAppBar=NavigationBar=Navigation
//    → TopBar；Tab Bar=NavigationBar(M3)=Tabs(OH) → TabBar）——

/// TopBar 高 56vp（OH Navigation Mini 模式 = M3 TopAppBar 同值合体）。
pub const TOPBAR_H_VP: u32 = 56;
/// BerryOS 标准图标 id 约定表（供件侧 IconSource 按 id 绘制）：
/// 1 = back 箭头（TopBar 返回键）。应用自有图标从 0x100 起自编。
pub const ICON_ID_BACK: u16 = 1;
/// 返回键图标位 24vp（M3/OH 图标标准尺寸）。
pub const ICON_VP: u32 = 24;
/// TopBar 标题字号（OH body1 16fp 档）。
pub const TITLE_SCALE: FontSize = FontSize::S16x32;

/// AppShell 布局产物句柄。
pub struct AppShell {
    /// 根容器（挂应用窗）。
    pub root: crate::NodeId,
    /// 内容槽（应用自体内容挂点——位于 TopBar 下、TabBar 上）。
    pub content: crate::NodeId,
    /// 返回键触发节点（press_map/Click 消费）。
    pub back: crate::NodeId,
    /// 标题文本节点（set_text 可换）。
    pub title: crate::NodeId,
    /// TabBar 页签触发节点组（无 TabBar = count 0；帽 5——iOS TabBar
    /// 官方建议 3-5 页签）。
    pub tabs: [crate::NodeId; 5],
    /// 页签实数。
    pub tab_count: usize,
}

/// 装配 AppShell：Col(根) → [TopBar(Row: back + 标题 + 右槽), 内容 Pad,
/// TabBar(Row: N 页签)]。`tab_labels` 空 = 无底栏（纯 TopBar 页）。
/// 按压反馈：back 与每页签登记 press_map 面由调用方收表（shell 不自持
/// ——press_map 是 Ui 单表，应用自汇总，壳只给 face 节点）。
pub fn add_app_shell(
    ui: &mut crate::UiMain,
    parent: crate::NodeId,
    density: u32,
    title: &[u8],
    tab_labels: &[&[u8]],
) -> Result<AppShell, crate::UiError> {
    use crate::desc::{ColProps, PadProps, RowProps, TextProps, TextStyle, IconProps};
    use crate::layout::CrossAlign;
    let root = ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }),
        },
    )?;
    // —— TopBar：白底行 + 返回键（icon）+ 标题左对齐 + 弹性空 ——

    let topbar = ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Pad,
            key: 0,
            props: Props::Pad(PadProps { all: 4 * density }),
        },
    )?;
    let bar_bg = ui.add(
        topbar,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps { bg: CARD_BG, w: None, h: Some(TOPBAR_H_VP * density), radius: 0 }),
        },
    )?;
    let bar_row = ui.add(
        topbar,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 12 * density,
                w: None,
                h: Some(TOPBAR_H_VP * density),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    // 返回键：Pad 叠加容器 {Rect 按压面, Icon 箭头}——同 launcher nav 键
    // 结构（Rect 是叶子不容子——BadParent 判例 2026-11-15）。
    let back_holder = ui.add(
        bar_row,
        ViewSpec {
            kind: ViewKind::Pad,
            key: 0,
            props: Props::Pad(PadProps { all: 0 }),
        },
    )?;
    let back_face = ui.add(
        back_holder,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps {
                bg: CARD_BG,
                w: Some(ICON_VP * density),
                h: Some(ICON_VP * density),
                radius: (8 * density) as u16,
            }),
        },
    )?;
    let back = ui.add(
        back_holder,
        ViewSpec {
            kind: ViewKind::Icon,
            key: 0,
            props: Props::Icon(IconProps {
                // 标准图标 id（BerryOS 约定表）：1=back 箭头——供件侧
                //（IconSource 实现）按 id 绘制；launcher 侧映射 NAV_BACK。
                id: ICON_ID_BACK,
                fg: TEXT_PRIMARY,
                w: Some(ICON_VP * density),
                h: Some(ICON_VP * density),
            }),
        },
    )?;
    let title_node = ui.add(bar_row, text_spec_title(title, density)?)?;
    // —— 内容槽：TopBar 下满宽 ——

    let content = ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Pad,
            key: 0,
            props: Props::Pad(PadProps { all: 0 }),
        },
    )?;
    // —— TabBar（可选）：BottomTabs 48vp + 页签 Icon+文（v1 文本页签）——

    let mut tabs = [crate::NodeId(0); 5];
    let mut tab_count = 0usize;
    if !tab_labels.is_empty() {
        let tabbar = ui.add(
            root,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: 0,
                    w: None,
                    h: Some(TABS_VP * density),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let _bar_bg2 = ui.add(
            tabbar,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: CARD_BG, w: None, h: Some(TABS_VP * density), radius: 0 }),
            },
        )?;
        for (ti, label) in tab_labels.iter().enumerate().take(5) {
            let t = ui.add(
                tabbar,
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
            let tf = ui.add(
                t,
                ViewSpec {
                    kind: ViewKind::Text,
                    key: 0,
                    props: Props::Text(TextProps {
                        style: TextStyle { fg: TEXT_PRIMARY, bg: None, scale: FontSize::S8x16 },
                        text: TextBuf::new(label)?,
                    }),
                },
            )?;
            tabs[ti] = tf;
            tab_count += 1;
        }
    }
    let _ = (bar_bg, back_face);
    Ok(AppShell { root, content, back, title: title_node, tabs, tab_count })
}

fn text_spec_title(text: &[u8], _density: u32) -> Result<ViewSpec, crate::UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: TEXT_PRIMARY, bg: None, scale: TITLE_SCALE },
            text: TextBuf::new(text)?,
        }),
    })
}
