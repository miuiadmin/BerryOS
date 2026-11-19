#![cfg_attr(not(test), no_std)]
//! berry-appshell —— 应用壳 overlay 组件 v1（刀 C/波 3 补缺件）。
//!
//! 承载六波总纲步骤 4 的「Dialog/Menu/Toast v1 落 berry-appshell」：
//! in-app overlay 形态（宿主把 overlay 根挂到内容 Stack 末位 = z 序顶）。
//! **SERVICE 层接点**（系统级弹层、跨窗遮罩、显隐路由）随波 3 分支并主
//! （`wm_layer_known` 收宽 layer=2 + WM_EVENT 通道）后接线——本版 API
//! 形已按「宿主持有 overlay 根、组件只建子树」设计，接线不改组件面。
//!
//! 纪律：零手拼（面板一律 Card kind 圆角直载、按压一律 Button kind、
//! 取色一律 [`Tokens`] 单源）；文案 ASCII（字形帽约束）；零 alloc——
//! 组件只用 const 泛型 arena，不建堆结构。
//!
//! 色彩豁免台账（同暗色刀「豁免只减不增」口径）：`TOAST_BG`/`TOAST_FG`
//! 一对中性恒色——OH toast 双主题同形（暗底白字是不随主题换档的不变
//! 量，非主题缩放色），登记于 C-IMPL 豁免清单，theme 出 `on_scrim` 类
//! token 时收编。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_ui::desc::{
    ButtonProps, ButtonStyle, CardProps, ColProps, Props, RectProps, RowProps, StackAlign,
    StackProps, TextBuf, TextProps, TextStyle, ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::theme::Tokens;
use berry_ui::{NodeId, Ui, UiError};

/// 遮罩色：页底混 40% 黑（整数预混 `c*3/5`——暗色刀 20%/40% 遮罩律同源）。
pub fn scrim(t: &Tokens) -> Color {
    let p = t.bg_page;
    Color { r: p.r / 5 * 3, g: p.g / 5 * 3, b: p.b / 5 * 3 }
}

/// OH toast 中性恒色对（豁免台账见模块头注）。
pub const TOAST_BG: Color = Color { r: 0x2c, g: 0x2c, b: 0x2c };
/// toast 文字色（白）。
pub const TOAST_FG: Color = Color { r: 0xff, g: 0xff, b: 0xff };

/// toast 常驻时长（六波总纲原参数：2s 帧节拍驱动）。
pub const TOAST_TTL_MS: u64 = 2_000;

/// Dialog 面板宽（@3x，约 187vp——OH 弹窗 80% 幅量级）。
const DIALOG_W: u32 = 560;
/// Menu 面板宽（@3x 140vp）。
const MENU_W: u32 = 420;
/// 圆角（@3x 8vp = 24px——OH_RADIUS 单源同值；Card corner_radius 直载）。
const OVERLAY_RADIUS: u32 = 24;
/// 面板内距（@3x 8vp）。
const OVERLAY_PAD: u32 = 24;
/// 弹层阴影层级。
const OVERLAY_ELEVATION: u32 = 2;
/// 菜单行高（@3x 48vp——LISTITEM_H 同源值）。
const MENU_ROW_H: u32 = 144;

fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

fn text(fg: Color, scale: FontSize, bytes: &[u8]) -> Result<ViewSpec, UiError> {
    Ok(vspec(
        ViewKind::Text,
        Props::Text(TextProps { style: TextStyle { fg, bg: None, scale }, text: TextBuf::new(bytes)? }),
    ))
}

/// overlay 根：Stack（后序子盖上——scrim 先、面板后）。
fn overlay_stack<const N: usize>(ui: &mut Ui<N>, parent: NodeId, align: StackAlign) -> Result<NodeId, UiError> {
    ui.add(parent, vspec(ViewKind::Stack, Props::Stack(StackProps { align })))
}

fn scrim_layer<const N: usize>(ui: &mut Ui<N>, parent: NodeId, t: &Tokens) -> Result<NodeId, UiError> {
    // 全幅（w/h None = 随父分配——Rect 聚合语义）。
    ui.add(parent, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: scrim(t), w: None, h: None, radius: 0 })))
}

fn panel<const N: usize>(ui: &mut Ui<N>, parent: NodeId, w: u32) -> Result<NodeId, UiError> {
    ui.add(
        parent,
        vspec(ViewKind::Card, Props::Card(CardProps { elevation: OVERLAY_ELEVATION, corner_radius: OVERLAY_RADIUS })),
    )
    .and_then(|c| {
        ui.set_props(c, Props::Card(CardProps { elevation: OVERLAY_ELEVATION, corner_radius: OVERLAY_RADIUS }))?;
        let _ = w; // Card w 走 Col 定宽子（下方 content）承载——面板幅 = 内容幅。
        Ok(c)
    })
}

fn button<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    label: &[u8],
    style: ButtonStyle,
    on_tap: u32,
) -> Result<NodeId, UiError> {
    let b = ui.add(
        parent,
        vspec(
            ViewKind::Button,
            Props::Button(ButtonProps { label: TextBuf::new(label)?, style, on_tap }),
        ),
    )?;
    let _ = t; // Button 视觉单源在 kind（五态查表）——配方不带色。
    ui.set_a11y(b, berry_ui::ROLE_BUTTON, label)?;
    Ok(b)
}

/// Dialog 句柄（宿主按 on_confirm/on_cancel 收 Msg 后 `ui.remove(node)`）。
pub struct Dialog {
    /// overlay 根（Stack——remove 即撤整个弹层）。
    pub node: NodeId,
    /// 确认钮事件 id。
    pub on_confirm: u32,
    /// 取消钮事件 id。
    pub on_cancel: u32,
}

/// 对话框（标题 + 正文 + 双钮；六波总纲「Dialog v1」原句形态）。
pub fn dialog<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    title: &[u8],
    body: &[u8],
    confirm_label: &[u8],
    cancel_label: &[u8],
    on_confirm: u32,
    on_cancel: u32,
) -> Result<Dialog, UiError> {
    let root = overlay_stack(ui, parent, StackAlign::Center)?;
    scrim_layer(ui, root, t)?;
    let card = panel(ui, root, DIALOG_W)?;
    let col = ui.add(
        card,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 16, w: Some(DIALOG_W), h: None, cross: CrossAlign::Start })),
    )?;
    let padded = ui.add(col, vspec(ViewKind::Pad, Props::Pad(pad_props(OVERLAY_PAD))))?;
    let title_node = ui.add(padded, text(t.font_primary, FontSize::S24x48, title)?)?;
    ui.set_a11y(title_node, berry_ui::ROLE_TEXT, title)?;
    ui.add(padded, text(t.font_secondary, FontSize::S16x32, body)?)?;
    let row = ui.add(
        padded,
        vspec(ViewKind::Row, Props::Row(RowProps { gap: 24, w: None, h: None, cross: CrossAlign::Center })),
    )?;
    button(ui, row, t, cancel_label, ButtonStyle::Outlined, on_cancel)?;
    button(ui, row, t, confirm_label, ButtonStyle::Filled, on_confirm)?;
    Ok(Dialog { node: root, on_confirm, on_cancel })
}

fn pad_props(all: u32) -> berry_ui::desc::PadProps {
    // Pad v1 四向同缩单值（desc PadProps 字段名单源）。
    berry_ui::desc::PadProps { all }
}

/// Menu 句柄：`on_pick_base + i` = 第 i 项事件 id（宿主差分路由）。
pub struct Menu {
    /// overlay 根。
    pub node: NodeId,
    /// 事件基址。
    pub on_pick_base: u32,
    /// 项数。
    pub len: usize,
}

/// 菜单（锚点列表 + 选中回填——六波总纲「Menu v1（含 Context）」in-app 形态）。
pub fn menu<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    entries: &[&[u8]],
    on_pick_base: u32,
) -> Result<Menu, UiError> {
    let root = overlay_stack(ui, parent, StackAlign::Center)?;
    scrim_layer(ui, root, t)?;
    let card = panel(ui, root, MENU_W)?;
    let col = ui.add(
        card,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: Some(MENU_W), h: None, cross: CrossAlign::Start })),
    )?;
    let padded = ui.add(col, vspec(ViewKind::Pad, Props::Pad(pad_props(OVERLAY_PAD / 2))))?;
    for (i, e) in entries.iter().enumerate() {
        let row = ui.add(
            padded,
            vspec(ViewKind::Row, Props::Row(RowProps { gap: 12, w: None, h: Some(MENU_ROW_H), cross: CrossAlign::Center })),
        )?;
        button(ui, row, t, e, ButtonStyle::Text, on_pick_base + i as u32)?;
    }
    Ok(Menu { node: root, on_pick_base, len: entries.len() })
}

/// Toast 句柄（宿主帧拍轮询 [`toast_expired`]，到期 `ui.remove(node)`）。
pub struct Toast {
    /// toast 根（Card——底位摆放归宿主容器）。
    pub node: NodeId,
    /// 文本节点。
    pub text: NodeId,
    /// 出生时刻（宿主帧钟 ms）。
    pub born_ms: u64,
}

/// 瞬态提示（底部 2s——六波总纲「Toast v1」原参数；SERVICE 层全局位随波 3 接线）。
pub fn toast<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    msg: &[u8],
    now_ms: u64,
) -> Result<Toast, UiError> {
    let card = ui.add(
        parent,
        vspec(ViewKind::Card, Props::Card(CardProps { elevation: 3, corner_radius: OVERLAY_RADIUS })),
    )?;
    let padded = ui.add(card, vspec(ViewKind::Pad, Props::Pad(pad_props(OVERLAY_PAD / 2))))?;
    let txt = ui.add(padded, text(TOAST_FG, FontSize::S16x32, msg)?)?;
    // Card 底色走 kind 默认（bg_comp）——toast 暗底由宿主层叠 TOAST_BG Rect
    // 或 theme 收编后直载；v1 双层恒色对见模块头注豁免台账。
    let _ = TOAST_BG;
    ui.set_a11y(txt, berry_ui::ROLE_TEXT, msg)?;
    Ok(Toast { node: card, text: txt, born_ms: now_ms })
}

/// 到期判定（`born_ms + TOAST_TTL_MS <= now_ms`）。
pub fn toast_expired(t: &Toast, now_ms: u64) -> bool {
    now_ms.saturating_sub(t.born_ms) >= TOAST_TTL_MS
}
