//! BerryUI F4 组件第一版。
//!
//! 本模块只组合 `Ui` 保留树、Berry Design token 与 a11y 侧表，不新增
//! 布局/渲染/输入引擎。所有构造函数都是 `&mut Ui<N>` + Plain Data 入参，
//! 返回稳定 NodeId 句柄；业务状态由 app 持有，组件只提供状态应用与
//! Msg 识别辅助，不承载业务回调。

use crate::desc::{
    CircleProps, ColProps, IconProps, Msg, NodeId, PanelProps, Props, RectProps, RowProps,
    TextBuf, TextProps, TextStyle, ViewKind, ViewSpec,
};
use crate::layout::CrossAlign;
use crate::theme::{
    InteractionState, RadiusToken, SpacingToken, StrokeToken, Theme, ThemeMetrics,
    TypographyToken,
};
use crate::{Ui, UiError, ROLE_BUTTON, ROLE_LIST_ITEM, ROLE_TEXT, ROLE_TOGGLE};
use berry_gfx::cmd::Color;

/// 组件构造上下文：主题与当前视口密度一次快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetContext {
    pub theme: Theme,
    pub metrics: ThemeMetrics,
}

impl WidgetContext {
    pub const fn new(theme: Theme, metrics: ThemeMetrics) -> Self {
        Self { theme, metrics }
    }

    pub fn for_viewport(theme: Theme, w: u32, h: u32) -> Self {
        Self::new(theme, theme.metrics_for_viewport(w, h))
    }

    fn spacing(&self, token: SpacingToken) -> u32 {
        self.metrics.spacing(token)
    }
}

/// 文本语义档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextVariant {
    Title,
    Body,
    Caption,
}

fn text_spec(bytes: &[u8], fg: Color, scale: berry_gfx::font::FontSize) -> Result<ViewSpec, UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(bytes)?,
        }),
    })
}

fn panel_spec(
    bg: Color, w: Option<u32>, h: Option<u32>, padding: u32, gap: u32, radius: u32,
) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Panel,
        key: 0,
        props: Props::Panel(PanelProps { bg, w, h, padding, gap, radius }),
    }
}

fn add<const N: usize>(
    ui: &mut Ui<N>, parent: NodeId, spec: ViewSpec,
) -> Result<NodeId, UiError> {
    ui.add(parent, spec)
}

/// Scaffold： AppBar + 可滚动/可放置内容的页面骨架。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scaffold {
    pub root: NodeId,
    pub app_bar: AppBar,
    pub body: NodeId,
}

impl Scaffold {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
    ) -> Result<Self, UiError> {
        let root = add(
            ui,
            parent,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: None,
                    h: None,
                    cross: CrossAlign::Stretch,
                }),
            },
        )?;
        let app_bar = AppBar::new(ui, root, ctx, title)?;
        let pad = ctx.spacing(SpacingToken::S);
        let title_h = ctx.metrics.typography(TypographyToken::Title).cell_h() as u32;
        let bar_h = title_h.saturating_add(pad.saturating_mul(2));
        let (_, viewport_h) = ui.viewport();
        let body_h = viewport_h.saturating_sub(bar_h);
        let body = add(
            ui,
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: None,
                    h: Some(body_h),
                    cross: CrossAlign::Stretch,
                }),
            },
        )?;
        Ok(Self { root, app_bar, body })
    }
}

/// AppBar：主题色标题栏。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppBar {
    pub panel: NodeId,
    pub title: NodeId,
}

impl AppBar {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
    ) -> Result<Self, UiError> {
        let pad = ctx.spacing(SpacingToken::S);
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.primary, None, None, pad, pad, 0),
        )?;
        let title_id = add(
            ui,
            panel,
            text_spec(
                title,
                ctx.theme.colors.on_primary,
                ctx.metrics.typography(TypographyToken::Title),
            )?,
        )?;
        ui.set_a11y(title_id, ROLE_TEXT, title)?;
        Ok(Self { panel, title: title_id })
    }
}

/// Card：表面容器。调用方直接向 `panel` 添加内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    pub panel: NodeId,
}

impl Card {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext,
    ) -> Result<Self, UiError> {
        let pad = ctx.spacing(SpacingToken::M);
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, None, None, pad, ctx.spacing(SpacingToken::S), ctx.metrics.radius(RadiusToken::Medium)),
        )?;
        Ok(Self { panel })
    }
}

/// Column 组件：主题间距包装的纵向容器。
pub fn column<const N: usize>(
    ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext,
) -> Result<NodeId, UiError> {
    add(
        ui,
        parent,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: ctx.spacing(SpacingToken::S),
                w: None,
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    )
}

/// Row 组件：主题间距包装的横向容器。
pub fn row<const N: usize>(
    ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext,
) -> Result<NodeId, UiError> {
    add(
        ui,
        parent,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: ctx.spacing(SpacingToken::S),
                w: None,
                h: None,
                cross: CrossAlign::Center,
            }),
        },
    )
}

/// Text 组件：语义档 + 主题色文本。
pub fn text<const N: usize>(
    ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, variant: TextVariant,
    bytes: &[u8],
) -> Result<NodeId, UiError> {
    let (fg, scale) = match variant {
        TextVariant::Title => (
            ctx.theme.colors.text_primary,
            ctx.metrics.typography(TypographyToken::Title),
        ),
        TextVariant::Body => (
            ctx.theme.colors.text_primary,
            ctx.metrics.typography(TypographyToken::Body),
        ),
        TextVariant::Caption => (
            ctx.theme.colors.text_secondary,
            ctx.metrics.typography(TypographyToken::Caption),
        ),
    };
    let id = add(ui, parent, text_spec(bytes, fg, scale)?)?;
    ui.set_a11y(id, ROLE_TEXT, bytes)?;
    Ok(id)
}

/// Icon 组件：主题前景与密度尺寸；像素仍经 IconSource 单源供给。
pub fn icon<const N: usize>(
    ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, id: u16,
) -> Result<NodeId, UiError> {
    let size = ctx.spacing(SpacingToken::L);
    add(
        ui,
        parent,
        ViewSpec {
            kind: ViewKind::Icon,
            key: 0,
            props: Props::Icon(IconProps {
                id,
                fg: ctx.theme.colors.primary,
                w: Some(size),
                h: Some(size),
            }),
        },
    )
}

/// Divider：细线。
pub fn divider<const N: usize>(
    ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext,
) -> Result<NodeId, UiError> {
    add(
        ui,
        parent,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps {
                bg: ctx.theme.colors.outline,
                w: None,
                h: Some(ctx.metrics.stroke(StrokeToken::Thin)),
            }),
        },
    )
}

/// Button：状态视觉 + a11y + Press/Click 识别。业务动作由 app 处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Button {
    pub panel: NodeId,
    pub label: NodeId,
    base: InteractionState,
}

impl Button {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, label: &[u8],
        base: InteractionState,
    ) -> Result<Self, UiError> {
        let pad = ctx.spacing(SpacingToken::S);
        let state = ctx.theme.state(base);
        let panel = add(ui, parent, panel_spec(state.background, None, None, pad, 0, ctx.metrics.radius(RadiusToken::Small)))?;
        let text_id = add(
            ui,
            panel,
            text_spec(label, state.content, ctx.metrics.typography(TypographyToken::Body))?,
        )?;
        ui.set_a11y(panel, ROLE_BUTTON, label)?;
        Ok(Self { panel, label: text_id, base })
    }

    fn apply<const N: usize>(
        &self, ui: &mut Ui<N>, ctx: &WidgetContext, state: InteractionState,
    ) -> Result<(), UiError> {
        let visual = ctx.theme.state(state);
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = visual.background;
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.label) {
            if let Props::Text(mut p) = props {
                p.style.fg = visual.content;
                ui.set_props(self.label, Props::Text(p))?;
            }
        }
        Ok(())
    }

    pub fn set_base<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, base: InteractionState,
    ) -> Result<(), UiError> {
        self.base = base;
        self.apply(ui, ctx, base)
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        if self.base == InteractionState::Disabled {
            return Ok(false);
        }
        let node = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::Click { node } => *node,
            _ => return Ok(false),
        };
        if node != self.panel && node != self.label {
            return Ok(false);
        }
        match msg {
            Msg::Press { .. } => self.apply(ui, ctx, InteractionState::Pressed)?,
            Msg::Release { .. } | Msg::Cancel { .. } => self.apply(ui, ctx, self.base)?,
            Msg::Click { .. } => {
                self.apply(ui, ctx, self.base)?;
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
}

fn retained_props<const N: usize>(ui: &Ui<N>, node: NodeId) -> Option<Props> {
    let slot = (node.0 & 0xFFFF) as usize;
    let gen = (node.0 >> 16) as u16;
    ui.nodes
        .get(slot)
        .and_then(|option| option.as_ref())
        .filter(|n| n.alive && n.gen == gen)
        .map(|n| n.props)
}

/// ListItem：标题 + 可选说明 + 选中态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListItem {
    pub panel: NodeId,
    pub title: NodeId,
    pub detail: Option<NodeId>,
    selected: bool,
}

impl ListItem {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        detail: Option<&[u8]>, selected: bool,
    ) -> Result<Self, UiError> {
        let state = Self::state(ctx, selected);
        let pad = ctx.spacing(SpacingToken::M);
        let panel = add(ui, parent, panel_spec(state.background, None, None, pad, ctx.spacing(SpacingToken::Xs), 0))?;
        let title_id = add(
            ui,
            panel,
            text_spec(title, state.content, ctx.metrics.typography(TypographyToken::Body))?,
        )?;
        let detail_id = match detail {
            // F6 起客机系统 VMO 只有 32×48 A8 档；detail 用 Body 避免
            // Caption 拉入 S24 静态表或在 guest 档空绘。Caption 仍是公开
            // TextVariant，由 host widget 测试单独钉板。
            Some(bytes) => Some(add(
                ui,
                panel,
                text_spec(bytes, ctx.theme.colors.text_secondary, ctx.metrics.typography(TypographyToken::Body))?,
            )?),
            None => None,
        };
        ui.set_a11y(panel, ROLE_LIST_ITEM, title)?;
        Ok(Self { panel, title: title_id, detail: detail_id, selected })
    }

    fn state(ctx: &WidgetContext, selected: bool) -> crate::theme::StateColors {
        ctx.theme.state(if selected { InteractionState::Selected } else { InteractionState::Normal })
    }

    pub fn set_selected<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, selected: bool,
    ) -> Result<(), UiError> {
        self.selected = selected;
        let state = Self::state(ctx, selected);
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = state.background;
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.title) {
            if let Props::Text(mut p) = props {
                p.style.fg = state.content;
                ui.set_props(self.title, Props::Text(p))?;
            }
        }
        Ok(())
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }
}

/// Switch：双态开关。状态由 app 持有；`on_msg` 可在 Click 时翻转视觉态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switch {
    pub panel: NodeId,
    pub track: NodeId,
    pub spacer: NodeId,
    pub thumb: NodeId,
    checked: bool,
    disabled: bool,
}

impl Switch {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, checked: bool,
        disabled: bool,
    ) -> Result<Self, UiError> {
        let pad = ctx.spacing(SpacingToken::Xs);
        let track_w = ctx.spacing(SpacingToken::Xl).saturating_mul(2);
        let track_h = ctx.spacing(SpacingToken::Xl);
        let thumb = track_h.saturating_sub(pad.saturating_mul(2)).max(1);
        let panel_w = track_w.saturating_add(pad.saturating_mul(2));
        let panel_h = track_h.saturating_add(pad.saturating_mul(2));
        let state = if disabled {
            ctx.theme.state(InteractionState::Disabled)
        } else if checked {
            ctx.theme.state(InteractionState::Selected)
        } else {
            ctx.theme.state(InteractionState::Normal)
        };
        let panel = add(ui, parent, panel_spec(state.background, Some(panel_w), Some(panel_h), pad, 0, panel_h / 2))?;
        let track = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: 0,
                    w: Some(track_w),
                    h: Some(track_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let travel = track_w.saturating_sub(thumb);
        let spacer_w = if checked { travel } else { 0 };
        let spacer = add(
            ui,
            track,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: state.background, w: Some(spacer_w), h: Some(1) }),
            },
        )?;
        let thumb_id = add(
            ui,
            track,
            ViewSpec {
                kind: ViewKind::Circle,
                key: 0,
                props: Props::Circle(CircleProps { bg: state.content, w: Some(thumb), h: Some(thumb) }),
            },
        )?;
        let label: &[u8] = if checked { b"on" } else { b"off" };
        ui.set_a11y(panel, ROLE_TOGGLE, label)?;
        Ok(Self { panel, track, spacer, thumb: thumb_id, checked, disabled })
    }

    fn visual_state(&self) -> InteractionState {
        if self.disabled {
            InteractionState::Disabled
        } else if self.checked {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        }
    }

    fn apply<const N: usize>(
        &self, ui: &mut Ui<N>, ctx: &WidgetContext, state: InteractionState,
    ) -> Result<(), UiError> {
        let visual = ctx.theme.state(state);
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = visual.background;
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.spacer) {
            if let Props::Rect(mut p) = props {
                p.bg = visual.background;
                ui.set_props(self.spacer, Props::Rect(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.thumb) {
            if let Props::Circle(mut p) = props {
                p.bg = visual.content;
                ui.set_props(self.thumb, Props::Circle(p))?;
            }
        }
        Ok(())
    }

    pub fn set_checked<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, checked: bool,
    ) -> Result<(), UiError> {
        self.checked = checked;
        let state = self.visual_state();
        let label: &[u8] = if checked { b"on" } else { b"off" };
        ui.set_a11y(self.panel, ROLE_TOGGLE, label)?;
        let travel = Self::travel(ui, ctx);
        let width = if checked { travel } else { 0 };
        if let Some(props) = retained_props(ui, self.spacer) {
            if let Props::Rect(mut p) = props {
                p.w = Some(width);
                ui.set_props(self.spacer, Props::Rect(p))?;
            }
        }
        self.apply(ui, ctx, state)
    }

    fn travel<const N: usize>(ui: &Ui<N>, ctx: &WidgetContext) -> u32 {
        let _ = ui;
        let pad = ctx.spacing(SpacingToken::Xs);
        let track_w = ctx.spacing(SpacingToken::Xl).saturating_mul(2);
        let track_h = ctx.spacing(SpacingToken::Xl);
        let thumb = track_h.saturating_sub(pad.saturating_mul(2)).max(1);
        track_w.saturating_sub(thumb)
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        if self.disabled {
            return Ok(false);
        }
        let node = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::Click { node } => *node,
            _ => return Ok(false),
        };
        if node != self.panel && node != self.track && node != self.spacer && node != self.thumb {
            return Ok(false);
        }
        match msg {
            Msg::Press { .. } => self.apply(ui, ctx, InteractionState::Pressed)?,
            Msg::Release { .. } | Msg::Cancel { .. } => {
                self.apply(ui, ctx, self.visual_state())?
            }
            Msg::Click { .. } => {
                let checked = !self.checked;
                self.set_checked(ui, ctx, checked)?;
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
}

/// ScrollView：纵向偏移容器。v1 引擎无 clip，溢出内容仍可见；调用方应
/// 保证内容合理或等待 P0 Surface Mode 后的 clipping 语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollView {
    pub panel: NodeId,
    pub content: NodeId,
    offset: i32,
    max_offset: i32,
    last_y: Option<i32>,
}

impl ScrollView {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, viewport_h: u32,
    ) -> Result<Self, UiError> {
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, None, Some(viewport_h), 0, 0, 0),
        )?;
        let content = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: ctx.spacing(SpacingToken::S),
                    w: None,
                    h: None,
                    cross: CrossAlign::Stretch,
                }),
            },
        )?;
        Ok(Self { panel, content, offset: 0, max_offset: 0, last_y: None })
    }

    pub fn sync_bounds<const N: usize>(&mut self, ui: &mut Ui<N>) -> Result<(), UiError> {
        ui.layout();
        let view = ui.bounds(self.panel)?;
        let content = ui.bounds(self.content)?;
        self.max_offset = (content.h - view.h).max(0);
        self.clamp(ui)
    }

    pub fn scroll_by<const N: usize>(
        &mut self, ui: &mut Ui<N>, delta: i32,
    ) -> Result<(), UiError> {
        self.offset = self.offset.saturating_add(delta).clamp(0, self.max_offset);
        self.clamp(ui)
    }

    fn clamp<const N: usize>(&self, ui: &mut Ui<N>) -> Result<(), UiError> {
        ui.set_offset(self.content, 0, -self.offset)
    }

    pub fn offset(&self) -> i32 {
        self.offset
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, msg: &Msg,
    ) -> Result<bool, UiError> {
        let (node, y) = match msg {
            Msg::DragStart { node, y, .. } | Msg::DragMove { node, y, .. } => {
                (*node, crate::hit::u32_to_i32_sat(*y))
            }
            Msg::DragEnd { node, .. } => {
                if *node == self.panel || *node == self.content {
                    self.last_y = None;
                    return Ok(true);
                }
                return Ok(false);
            }
            _ => return Ok(false),
        };
        if node != self.panel && node != self.content {
            return Ok(false);
        }
        match self.last_y {
            Some(old) => {
                let delta = old.saturating_sub(y);
                self.scroll_by(ui, delta)?;
            }
            None => {}
        }
        self.last_y = Some(y);
        Ok(true)
    }
}

/// EmptyState：图标 + 标题 + 说明。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyState {
    pub panel: NodeId,
    pub icon: NodeId,
    pub title: NodeId,
    pub message: NodeId,
}

impl EmptyState {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, icon_id: u16,
        title: &[u8], message: &[u8],
    ) -> Result<Self, UiError> {
        let pad = ctx.spacing(SpacingToken::L);
        let panel = add(ui, parent, panel_spec(ctx.theme.colors.background, None, None, pad, ctx.spacing(SpacingToken::M), 0))?;
        let stack = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: ctx.spacing(SpacingToken::S),
                    w: None,
                    h: None,
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let icon_id_node = icon(ui, stack, ctx, icon_id)?;
        let title_id = text(ui, stack, ctx, TextVariant::Title, title)?;
        let message_id = text(ui, stack, ctx, TextVariant::Body, message)?;
        Ok(Self { panel, icon: icon_id_node, title: title_id, message: message_id })
    }
}
