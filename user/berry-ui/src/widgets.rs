//! BerryUI F4 组件第一版。
//!
//! 本模块只组合 `Ui` 保留树、Berry Design token 与 a11y 侧表，不新增
//! 布局/渲染/输入引擎。所有构造函数都是 `&mut Ui<N>` + Plain Data 入参，
//! 返回稳定 NodeId 句柄；业务状态由 app 持有，组件只提供状态应用与
//! Msg 识别辅助，不承载业务回调。

use crate::desc::{
    CircleProps, ClipProps, ColProps, IconProps, Msg, NodeId, PanelProps, Props, RectProps, RowProps,
    TextBuf, TextProps, TextStyle, ViewKind, ViewSpec,
};
use crate::layout::CrossAlign;
use crate::text_input::{TextInputKind, TextInputState};
use crate::theme::{
    InteractionState, RadiusToken, SpacingToken, StrokeToken, Theme, ThemeMetrics,
    TypographyToken,
};
use crate::{
    SemEntry, Ui, UiError, SEM_ACTION_ACTIVATE, SEM_ACTION_TEXT_EDIT, SEM_STATE_CHECKED,
    SEM_STATE_DISABLED, ROLE_BUTTON, ROLE_LIST_ITEM, ROLE_TEXT, ROLE_TOGGLE,
};
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

fn text_caret_width(ctx: &WidgetContext) -> u32 {
    ctx.metrics.stroke(StrokeToken::Thin).max(1)
}

/// GUI-T1 TextField 构造规格。v1 只有单行；`kind` 同时定义输入过滤与
/// P0 IME purpose 的 add-only 映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFieldSpec {
    pub kind: TextInputKind,
    pub width: u32,
}

/// TextField：Panel + Clip + selection/Text/caret 组合控件。
///
/// 业务值与 selection 归 app 的 `TextInputState`；本控件只把状态应用到
/// 既有 ViewSpec/Props。Enter/Space 的编辑优先级由 `sdk::key_route` 单源
/// 裁决，控件本身不合成 Click、不请求软键盘、不接触 IME Host。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextField {
    pub panel: NodeId,
    pub port: NodeId,
    pub selection: NodeId,
    pub value_text: NodeId,
    pub caret: NodeId,
    kind: TextInputKind,
    focused: bool,
    scroll_x: i32,
}

impl TextField {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>,
        parent: NodeId,
        ctx: &WidgetContext,
        spec: TextFieldSpec,
    ) -> Result<Self, UiError> {
        if spec.width == 0 {
            return Err(UiError::NodeBad);
        }
        let pad = ctx.spacing(SpacingToken::Xs);
        let line_h = ctx.metrics.typography(TypographyToken::Body).cell_h() as u32;
        let height = line_h.saturating_add(pad.saturating_mul(2));
        let panel = add(
            ui,
            parent,
            panel_spec(
                ctx.theme.colors.surface_variant,
                Some(spec.width),
                Some(height),
                pad,
                0,
                ctx.metrics.radius(RadiusToken::Small),
            ),
        )?;
        let port = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Clip,
                key: 0,
                props: Props::Clip(ClipProps { w: None, h: Some(line_h) }),
            },
        )?;
        let selection = add(
            ui,
            port,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: ctx.theme.state(InteractionState::Selected).background,
                    w: Some(0),
                    h: Some(line_h),
                }),
            },
        )?;
        let value_text = add(
            ui,
            port,
            text_spec(b"", ctx.theme.colors.text_primary, ctx.metrics.typography(TypographyToken::Body))?,
        )?;
        let caret = add(
            ui,
            port,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: ctx.theme.colors.primary,
                    w: Some(0),
                    h: Some(line_h),
                }),
            },
        )?;
        ui.set_a11y(panel, ROLE_TEXT, b"input")?;
        ui.set_focusable(panel, Some(panel), true)?;
        ui.set_sem(SemEntry::new(
            panel, SEM_ACTION_TEXT_EDIT, 0, b"", 0, 0, false,
        )?)?;
        Ok(Self {
            panel,
            port,
            selection,
            value_text,
            caret,
            kind: spec.kind,
            focused: false,
            scroll_x: 0,
        })
    }

    pub fn kind(&self) -> TextInputKind {
        self.kind
    }

    pub fn is_focused<const N: usize>(&self, ui: &Ui<N>) -> bool {
        ui.focus_node() == Some(self.panel)
    }

    /// 显式请求组件焦点。返回 C1 FocusIn/FocusOut 消息，调用方必须继续
    /// 分发；本方法只负责同步自身视觉，不吞事件。
    pub fn request_focus<const N: usize>(
        &mut self,
        ui: &mut Ui<N>,
        ctx: &WidgetContext,
    ) -> Result<crate::desc::MsgBuf, UiError> {
        let messages = ui.focus_set(self.panel)?;
        self.set_focused(ui, ctx, true)?;
        Ok(messages)
    }

    /// 把 app 持有的文本状态同步到组合树；selection 高亮与 caret 走既有
    /// props/set_offset damage，不新增引擎路径。
    pub fn sync<const N: usize>(
        &mut self,
        ui: &mut Ui<N>,
        ctx: &WidgetContext,
        state: &TextInputState,
    ) -> Result<(), UiError> {
        let scale = ctx.metrics.typography(TypographyToken::Body);
        let cell_w = scale.cell_w() as i32;
        let line_h = scale.cell_h() as u32;
        let display = state.display_value(self.kind)?;
        ui.set_text(self.value_text, display.as_bytes())?;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            // Password 明文不进入长期语义树；Text/Number value/range 与
            // GUI-T1 TextInputState 保持同步。
            let semantic_value = if self.kind == TextInputKind::Password {
                b""
            } else {
                state.value().as_bytes()
            };
            semantic.value = crate::desc::TextBuf::new(semantic_value)?;
            if self.kind == TextInputKind::Password {
                semantic.text_range.anchor_cp = 0;
                semantic.text_range.caret_cp = 0;
            } else {
                // P0 语义 range 存无向区间；GUI-T1 反向 selection（anchor >
                // caret）在投影边界排序，不改变文本状态方向。
                let (start_cp, end_cp) = state.selection().ordered();
                semantic.text_range.anchor_cp = start_cp;
                semantic.text_range.caret_cp = end_cp;
            }
            ui.set_sem(semantic)?;
        }

        let (start_cp, end_cp) = state.selection().ordered();
        let sel_w = (end_cp.saturating_sub(start_cp) as i32).saturating_mul(cell_w).max(0) as u32;
        if let Some(props) = retained_props(ui, self.selection) {
            if let Props::Rect(mut p) = props {
                p.w = Some(sel_w);
                p.h = Some(line_h);
                ui.set_props(self.selection, Props::Rect(p))?;
            }
        }

        let caret_col = state.caret_cp() as i32;
        let caret_x = caret_col.saturating_mul(cell_w);
        let caret_w = if self.focused {
            text_caret_width(ctx)
        } else {
            0
        };
        if let Some(props) = retained_props(ui, self.caret) {
            if let Props::Rect(mut p) = props {
                p.w = Some(caret_w);
                p.h = Some(line_h);
                ui.set_props(self.caret, Props::Rect(p))?;
            }
        }

        // 先收敛布局，再按 port 宽度做水平 caret-visible 滚动。
        ui.layout();
        let port = ui.bounds(self.port)?;
        let port_w = port.w.max(0);
        self.scroll_x = self.scroll_x.clamp(0, i32::MAX);
        if caret_x < self.scroll_x {
            self.scroll_x = caret_x;
        }
        let caret_end = caret_x.saturating_add(caret_w as i32);
        if caret_end > self.scroll_x.saturating_add(port_w) {
            self.scroll_x = (caret_end - port_w).max(0);
        }
        let sel_x = start_cp as i32 * cell_w;
        ui.set_offset(self.value_text, -self.scroll_x, 0)?;
        ui.set_offset(self.selection, sel_x - self.scroll_x, 0)?;
        ui.set_offset(self.caret, caret_x - self.scroll_x, 0)?;

        let visual = ctx.theme.state(if self.focused {
            InteractionState::Focused
        } else {
            InteractionState::Normal
        });
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = visual.background;
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        Ok(())
    }

    pub fn set_focused<const N: usize>(
        &mut self,
        ui: &mut Ui<N>,
        ctx: &WidgetContext,
        focused: bool,
    ) -> Result<(), UiError> {
        self.focused = focused;
        let visual = ctx.theme.state(if focused {
            InteractionState::Focused
        } else {
            InteractionState::Normal
        });
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = visual.background;
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.caret) {
            if let Props::Rect(mut p) = props {
                p.w = if focused {
                    Some(text_caret_width(ctx))
                } else {
                    Some(0)
                };
                ui.set_props(self.caret, Props::Rect(p))?;
            }
        }
        ui.layout();
        Ok(())
    }

    pub fn owns(&self, node: NodeId) -> bool {
        node == self.panel
            || node == self.port
            || node == self.selection
            || node == self.value_text
            || node == self.caret
    }

    /// 只消费 FocusIn/Out 视觉；Click 请求焦点由 `request_focus` 显式完成。
    pub fn on_msg<const N: usize>(
        &mut self,
        ui: &mut Ui<N>,
        ctx: &WidgetContext,
        msg: &Msg,
    ) -> Result<bool, UiError> {
        let (node, focus_in) = match msg {
            Msg::FocusIn { node } => (*node, true),
            Msg::FocusOut { node } => (*node, false),
            _ => return Ok(false),
        };
        if !self.owns(node) {
            return Ok(false);
        }
        self.set_focused(ui, ctx, focus_in)?;
        Ok(true)
    }
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
    focused: bool,
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
        ui.set_focusable(panel, Some(panel), base != InteractionState::Disabled)?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ACTIVATE,
            if base == InteractionState::Disabled { SEM_STATE_DISABLED } else { 0 },
            b"",
            0,
            0,
            false,
        )?)?;
        Ok(Self { panel, label: text_id, base, focused: false })
    }

    fn visual_state(&self) -> InteractionState {
        if self.base == InteractionState::Disabled {
            InteractionState::Disabled
        } else if self.focused {
            InteractionState::Focused
        } else {
            self.base
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
        if base == InteractionState::Disabled {
            self.focused = false;
        }
        ui.set_focusable(self.panel, Some(self.panel), base != InteractionState::Disabled)?;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.state = if base == InteractionState::Disabled { SEM_STATE_DISABLED } else { 0 };
            ui.set_sem(semantic)?;
        }
        self.apply(ui, ctx, self.visual_state())
    }

    /// Focus ring 视觉（Focused 状态色；Disabled 恒不显示）。
    pub fn set_focused<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, focused: bool,
    ) -> Result<(), UiError> {
        self.focused = focused;
        self.apply(ui, ctx, self.visual_state())
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
            | Msg::Click { node }
            | Msg::FocusIn { node }
            | Msg::FocusOut { node } => *node,
            _ => return Ok(false),
        };
        if node != self.panel && node != self.label {
            return Ok(false);
        }
        match msg {
            Msg::Press { .. } => self.apply(ui, ctx, InteractionState::Pressed)?,
            Msg::Release { .. } | Msg::Cancel { .. } => {
                self.apply(ui, ctx, self.visual_state())?
            }
            Msg::Click { .. } => {
                self.apply(ui, ctx, self.visual_state())?;
                return Ok(true);
            }
            Msg::FocusIn { .. } => {
                self.focused = true;
                self.apply(ui, ctx, InteractionState::Focused)?
            }
            Msg::FocusOut { .. } => {
                self.focused = false;
                self.apply(ui, ctx, self.base)?
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
    focused: bool,
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
        ui.set_focusable(panel, Some(panel), true)?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ACTIVATE,
            if selected { SEM_STATE_CHECKED } else { 0 },
            b"",
            0,
            0,
            false,
        )?)?;
        Ok(Self { panel, title: title_id, detail: detail_id, selected, focused: false })
    }

    fn visual_state(&self) -> InteractionState {
        if self.focused {
            InteractionState::Focused
        } else if self.selected {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        }
    }

    fn state(ctx: &WidgetContext, selected: bool) -> crate::theme::StateColors {
        ctx.theme.state(if selected { InteractionState::Selected } else { InteractionState::Normal })
    }

    pub fn set_selected<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, selected: bool,
    ) -> Result<(), UiError> {
        self.selected = selected;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.state = if selected { SEM_STATE_CHECKED } else { 0 };
            ui.set_sem(semantic)?;
        }
        let state = ctx.theme.state(self.visual_state());
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

    /// ListItem 只消费焦点视觉；激活语义由 app 合成 Click 自理。
    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        let node = match msg {
            Msg::FocusIn { node } | Msg::FocusOut { node } => *node,
            _ => return Ok(false),
        };
        if node != self.panel && node != self.title {
            return Ok(false);
        }
        match msg {
            Msg::FocusIn { .. } => self.set_focused(ui, ctx, true)?,
            Msg::FocusOut { .. } => self.set_focused(ui, ctx, false)?,
            _ => {}
        }
        Ok(false)
    }

    /// Focus ring 视觉（selected 与 focused 分域）。
    pub fn set_focused<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, focused: bool,
    ) -> Result<(), UiError> {
        self.focused = focused;
        let state = if focused {
            InteractionState::Focused
        } else if self.selected {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        };
        let visual = ctx.theme.state(state);
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = visual.background;
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.title) {
            if let Props::Text(mut p) = props {
                p.style.fg = visual.content;
                ui.set_props(self.title, Props::Text(p))?;
            }
        }
        Ok(())
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
    focused: bool,
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
        ui.set_focusable(panel, Some(panel), !disabled)?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ACTIVATE,
            if checked { SEM_STATE_CHECKED } else { 0 } |
                if disabled { SEM_STATE_DISABLED } else { 0 },
            label,
            0,
            0,
            false,
        )?)?;
        Ok(Self {
            panel,
            track,
            spacer,
            thumb: thumb_id,
            checked,
            disabled,
            focused: false,
        })
    }

    fn visual_state(&self) -> InteractionState {
        if self.disabled {
            InteractionState::Disabled
        } else if self.focused {
            InteractionState::Focused
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
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.state = if checked { SEM_STATE_CHECKED } else { 0 } |
                if self.disabled { SEM_STATE_DISABLED } else { 0 };
            semantic.value = crate::desc::TextBuf::new(label)?;
            ui.set_sem(semantic)?;
        }
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

    pub fn set_focused<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, focused: bool,
    ) -> Result<(), UiError> {
        self.focused = focused;
        self.apply(ui, ctx, self.visual_state())
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
            | Msg::Click { node }
            | Msg::FocusIn { node }
            | Msg::FocusOut { node } => *node,
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
            Msg::FocusIn { .. } => {
                self.focused = true;
                self.apply(ui, ctx, self.visual_state())?
            }
            Msg::FocusOut { .. } => {
                self.focused = false;
                self.apply(ui, ctx, self.visual_state())?
            }
            _ => {}
        }
        Ok(false)
    }
}

/// ScrollView：纵向偏移容器。Panel 背景与 Clip port 分层：内容可溢出
/// 布局界，但 paint/hit 都被 Clip 到 viewport 内。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollView {
    pub panel: NodeId,
    pub port: NodeId,
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
        let port = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Clip,
                key: 0,
                props: Props::Clip(ClipProps { w: None, h: Some(viewport_h) }),
            },
        )?;
        let content = add(
            ui,
            port,
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
        Ok(Self { panel, port, content, offset: 0, max_offset: 0, last_y: None })
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
                if *node == self.panel || *node == self.port || *node == self.content {
                    self.last_y = None;
                    return Ok(true);
                }
                return Ok(false);
            }
            _ => return Ok(false),
        };
        if node != self.panel && node != self.port && node != self.content {
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

/// L3 ListDetail 布局规格（Plain Data；无 Route/Back/选择状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListDetailSpec {
    pub width: u32,
    pub height: u32,
    pub gap: u32,
    pub list_weight: u16,
    pub detail_weight: u16,
}

/// ListDetail：最小双 pane 布局原语。只创建固定主轴 Row + 两个可
/// flex 的 Panel pane，并登记运行期权重；业务列表/详情内容由 app 挂到
/// `list`/`detail`，选择与返回栈不进 BerryUI。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListDetail {
    pub row: NodeId,
    pub list: NodeId,
    pub detail: NodeId,
}

impl ListDetail {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>,
        parent: NodeId,
        ctx: &WidgetContext,
        spec: ListDetailSpec,
    ) -> Result<Self, UiError> {
        if spec.list_weight == 0 || spec.detail_weight == 0 {
            return Err(UiError::NodeBad);
        }
        let row = add(
            ui,
            parent,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: spec.gap,
                    w: Some(spec.width),
                    h: Some(spec.height),
                    cross: CrossAlign::Stretch,
                }),
            },
        )?;
        let pad = ctx.spacing(SpacingToken::S);
        let inner_gap = ctx.spacing(SpacingToken::S);
        let list = add(
            ui,
            row,
            panel_spec(
                ctx.theme.colors.surface_variant,
                None,
                Some(spec.height),
                pad,
                inner_gap,
                0,
            ),
        )?;
        let detail = add(
            ui,
            row,
            panel_spec(ctx.theme.colors.surface, None, Some(spec.height), pad, inner_gap, 0),
        )?;
        ui.set_flex(list, spec.list_weight)?;
        ui.set_flex(detail, spec.detail_weight)?;
        Ok(Self { row, list, detail })
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
