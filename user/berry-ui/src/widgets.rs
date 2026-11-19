//! BerryUI F4 组件第一版。
//!
//! 本模块只组合 `Ui` 保留树、Berry Design token 与 a11y 侧表，不新增
//! 布局/渲染/输入引擎。所有构造函数都是 `&mut Ui<N>` + Plain Data 入参，
//! 返回稳定 NodeId 句柄；业务状态由 app 持有，组件只提供状态应用与
//! Msg 识别辅助，不承载业务回调。

use crate::desc::{
    CircleProps, ClipProps, ColProps, GridProps, IconProps, Msg, NodeId, PanelProps, Props,
    RectProps, RowProps, TextBuf, TextProps, TextStyle, ViewKind, ViewSpec,
};
use crate::layout::CrossAlign;
use crate::text_input::{TextInputKind, TextInputState};
use crate::theme::{
    InteractionState, RadiusToken, SpacingToken, StrokeToken, Theme, ThemeMetrics,
    TypographyToken,
};
use crate::{
    SemEntry, Ui, UiError, SEM_ACTION_ACTIVATE, SEM_ACTION_ADJUST, SEM_ACTION_TEXT_EDIT,
    SEM_STATE_CHECKED, SEM_STATE_DISABLED, SEM_STATE_SELECTED, ROLE_BUTTON, ROLE_LIST_ITEM,
    ROLE_TEXT, ROLE_TOGGLE,
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

    /// RUN-K1 UIContext seam：从 `Ui` 的权威 viewport 快照构造组件上下文，
    /// 消除 app 侧 `open_window -> WidgetContext -> Ui::new` 的手抄宽高。
    /// 本构造不读屏幕、设备档、SafeArea、系统主题或 R3 配置。
    pub fn for_ui<const N: usize>(theme: Theme, ui: &Ui<N>) -> Self {
        let (w, h) = ui.viewport();
        Self::for_viewport(theme, w, h)
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

pub(crate) fn text_spec(bytes: &[u8], fg: Color, scale: berry_gfx::font::FontSize) -> Result<ViewSpec, UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(bytes)?,
        }),
    })
}

pub(crate) fn panel_spec(
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

/// Scaffold： AppBar + body + 可选 bottom 栏槽的页面骨架。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scaffold {
    pub root: NodeId,
    pub app_bar: AppBar,
    pub body: NodeId,
    /// Fixed-height slot above `safe_bottom`. The app owns its content and
    /// selected state; Scaffold owns only safe-area geometry.
    pub bottom: Option<NodeId>,
    body_h: u32,
}

impl Scaffold {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
    ) -> Result<Self, UiError> {
        Self::build(ui, parent, ctx, title, None)
    }

    /// Light 56dp title-only shell with system insets (home/top-level pages).
    pub fn new_light_safe<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        safe_top: u32, safe_bottom: u32,
    ) -> Result<Self, UiError> {
        Self::safe_shell(
            ui, parent, ctx, title, None, None, false, None, safe_top,
            safe_bottom,
        )
    }

    /// Light title-only shell with a fixed-height bottom bar slot.
    pub fn new_light_bottom_safe<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        bottom_h: u32, safe_top: u32, safe_bottom: u32,
    ) -> Result<Self, UiError> {
        Self::safe_shell(
            ui, parent, ctx, title, None, None, false, Some(bottom_h),
            safe_top, safe_bottom,
        )
    }

    /// Large-title scaffold with explicit system insets. This is the static
    /// variant: no scrolling collapse or animation is implied.
    pub fn large_with_back_safe<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        subtitle: &[u8], back_icon: u16, active_icon: u16, safe_top: u32,
        safe_bottom: u32,
    ) -> Result<Self, UiError> {
        Self::safe_shell(
            ui, parent, ctx, title, Some(subtitle),
            Some((back_icon, active_icon)), true, None, safe_top, safe_bottom,
        )
    }

    /// 带真实导航返回按钮的页面骨架。按钮像素由 app 的 IconSource 供给；
    /// 点击语义仍归 app（返回栈/退出由业务状态机持有）。
    pub fn with_back<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        back_icon: u16, active_icon: u16,
    ) -> Result<Self, UiError> {
        Self::build(ui, parent, ctx, title, Some((back_icon, active_icon)))
    }

    /// OpenHarmony-style light navigation shell with explicit system insets.
    ///
    /// `safe_top` moves the app bar below a persistent system status bar;
    /// `safe_bottom` reserves the system navigation/dock band. Both are raw
    /// viewport pixels from Berry Room's occlusion snapshot.
    pub fn with_back_safe<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        back_icon: u16, active_icon: u16, safe_top: u32, safe_bottom: u32,
    ) -> Result<Self, UiError> {
        Self::safe_shell(
            ui, parent, ctx, title, None, Some((back_icon, active_icon)),
            false, None, safe_top, safe_bottom,
        )
    }

    /// Modern back shell with a fixed-height bottom bar slot above the system
    /// navigation reserve. `bottom_h` must not be folded into `safe_bottom`.
    pub fn with_back_bottom_safe<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        back_icon: u16, active_icon: u16, bottom_h: u32, safe_top: u32,
        safe_bottom: u32,
    ) -> Result<Self, UiError> {
        Self::safe_shell(
            ui, parent, ctx, title, None, Some((back_icon, active_icon)),
            false, Some(bottom_h), safe_top, safe_bottom,
        )
    }

    /// Body height frozen at construction. A viewport resize requires a full
    /// page rebuild; this is not a second layout truth.
    pub const fn body_height(&self) -> u32 {
        self.body_h
    }

    fn safe_shell<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        subtitle: Option<&[u8]>, back_icon: Option<(u16, u16)>, large: bool,
        bottom_h: Option<u32>, safe_top: u32, safe_bottom: u32,
    ) -> Result<Self, UiError> {
        if bottom_h == Some(0) {
            return Err(UiError::NodeBad);
        }
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
        if safe_top != 0 {
            // Edge-to-edge inset extension: the app bar color continues under
            // the camera/status cutout while all interactive AppBar content
            // remains below `safe_top`.
            add(
                ui,
                root,
                panel_spec(ctx.theme.colors.surface, None, Some(safe_top), 0, 0, 0),
            )?;
        }
        let app_bar = if large {
            AppBar::large(ui, root, ctx, title, subtitle, back_icon)?
        } else if let Some((back_icon, active_icon)) = back_icon {
            AppBar::build(ui, root, ctx, title, Some((back_icon, active_icon)))?
        } else {
            AppBar::light(ui, root, ctx, title)?
        };
        let (_, viewport_h) = ui.viewport();
        let body_h = viewport_h
            .saturating_sub(safe_top)
            .saturating_sub(app_bar.instance_height(ctx))
            .saturating_sub(bottom_h.unwrap_or(0))
            .saturating_sub(safe_bottom);
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
        let bottom = match bottom_h {
            Some(height) => Some(add(
                ui,
                root,
                ViewSpec {
                    kind: ViewKind::Col,
                    key: 0,
                    props: Props::Col(ColProps {
                        gap: 0,
                        w: None,
                        h: Some(height),
                        cross: CrossAlign::Stretch,
                    }),
                },
            )?),
            None => None,
        };
        if safe_bottom != 0 {
            // The app's bottom chrome color continues beneath the transparent
            // navigation overlay. Interactive body/bottom content stays above
            // this reserve, so three-button or floating-bar layouts remain
            // lifted by the authoritative safe inset.
            add(
                ui,
                root,
                panel_spec(ctx.theme.colors.surface, None, Some(safe_bottom), 0, 0, 0),
            )?;
        }
        Ok(Self { root, app_bar, body, bottom, body_h })
    }

    fn build<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        back_icon: Option<(u16, u16)>,
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
        let app_bar = AppBar::build(ui, root, ctx, title, back_icon)?;
        let (_, viewport_h) = ui.viewport();
        let body_h = viewport_h.saturating_sub(app_bar.instance_height(ctx));
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
        Ok(Self { root, app_bar, body, bottom: None, body_h })
    }
}

/// AppBar：legacy 标题栏，或 `with_back` 的现代导航栏。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppBar {
    pub panel: NodeId,
    pub title: NodeId,
    pub subtitle: Option<NodeId>,
    pub back: Option<IconButton>,
    /// Trailing action container. Modern light/back bars expose a real Row;
    /// legacy bars alias the panel for API compatibility.
    pub actions: NodeId,
    modern: bool,
    large: bool,
}

impl AppBar {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
    ) -> Result<Self, UiError> {
        Self::build(ui, parent, ctx, title, None)
    }

    /// Static Large AppBar: a normal 56dp action row plus a large title and
    /// optional subtitle below it. Scrolling collapse/enterAlways animation is
    /// intentionally not implemented; an app can replace the bar on its own
    /// navigation-state change.
    pub fn large<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        subtitle: Option<&[u8]>, back_icon: Option<(u16, u16)>,
    ) -> Result<Self, UiError> {
        let action_h = Self::height(ctx);
        let total_h = Self::expanded_height(ctx, subtitle.is_some());
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, None, Some(total_h), 0, 0, 0),
        )?;
        let action_row = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.metrics.effective_px(16),
                    w: None,
                    h: Some(action_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let back = match back_icon {
            Some((normal, active)) => Some(IconButton::new(
                ui,
                action_row,
                ctx,
                normal,
                active,
                b"Back",
                InteractionState::Normal,
                false,
            )?),
            None => None,
        };
        let leading = add(
            ui,
            action_row,
            panel_spec(ctx.theme.colors.surface, None, Some(action_h), 0, 0, 0),
        )?;
        ui.set_flex(leading, 1)?;
        let actions = add(
            ui,
            action_row,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: None,
                    h: Some(action_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let content = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: None,
                    h: None,
                    cross: CrossAlign::Stretch,
                }),
            },
        )?;
        let pad = ctx.spacing(SpacingToken::S);
        let title_node = add(
            ui,
            content,
            text_spec(
                title,
                ctx.theme.colors.text_primary,
                ctx.metrics.typography(TypographyToken::Title),
            )?,
        )?;
        ui.set_a11y(title_node, ROLE_TEXT, title)?;
        let subtitle_node = match subtitle.filter(|bytes| !bytes.is_empty()) {
            Some(bytes) => Some(add(
                ui,
                content,
                text_spec(
                    bytes,
                    ctx.theme.colors.text_secondary,
                    ctx.metrics.typography(TypographyToken::Body),
                )?,
            )?),
            None => None,
        };
        if let Some(node) = subtitle_node {
            ui.set_a11y(node, ROLE_TEXT, subtitle.unwrap_or(b""))?;
        }
        // The content column's measured height derives from text; give it an
        // explicit reserved area and vertical padding so the outer panel's
        // total height is stable and symmetric.
        ui.set_offset(
            content,
            0,
            i32::try_from(pad).unwrap_or(i32::MAX),
        )?;
        let content_h = total_h.saturating_sub(action_h);
        if let Some(props) = retained_props(ui, content) {
            if let Props::Col(mut props) = props {
                props.h = Some(content_h.saturating_sub(pad.saturating_mul(2)));
                ui.set_props(content, Props::Col(props))?;
            }
        }
        Ok(Self {
            panel,
            title: title_node,
            subtitle: subtitle_node,
            back,
            actions,
            modern: true,
            large: true,
        })
    }

    /// Static expanded height. `has_subtitle` keeps geometry deterministic
    /// before any node is created.
    pub fn expanded_height(ctx: &WidgetContext, has_subtitle: bool) -> u32 {
        let action = Self::height(ctx);
        let title = ctx.metrics.typography(TypographyToken::Title).cell_h() as u32;
        let subtitle = ctx.metrics.typography(TypographyToken::Body).cell_h() as u32;
        let pad = ctx.spacing(SpacingToken::S).saturating_mul(2);
        let gap = if has_subtitle {
            ctx.spacing(SpacingToken::Xs)
        } else {
            0
        };
        action
            .saturating_add(title)
            .saturating_add(if has_subtitle { subtitle } else { 0 })
            .saturating_add(gap)
            .saturating_add(pad)
    }

    pub fn with_back<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        back_icon: u16, active_icon: u16,
    ) -> Result<Self, UiError> {
        Self::build(ui, parent, ctx, title, Some((back_icon, active_icon)))
    }

    /// OH light 56dp title-only bar (top-level pages without a back stack).
    pub fn light<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
    ) -> Result<Self, UiError> {
        let bar_h = Self::height(ctx);
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, None, Some(bar_h), 0, 0, 0),
        )?;
        let (viewport_w, _) = ui.viewport();
        let line = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: 0,
                    w: Some(viewport_w),
                    h: Some(bar_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let leading = add(
            ui,
            line,
            panel_spec(ctx.theme.colors.surface, None, Some(bar_h), 0, 0, 0),
        )?;
        ui.set_flex(leading, 1)?;
        let title_id = add(
            ui,
            line,
            text_spec(
                title,
                ctx.theme.colors.text_primary,
                ctx.metrics.typography(TypographyToken::Title),
            )?,
        )?;
        let trailing = add(
            ui,
            line,
            panel_spec(ctx.theme.colors.surface, None, Some(bar_h), 0, 0, 0),
        )?;
        ui.set_flex(trailing, 1)?;
        let actions = add(
            ui,
            trailing,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: None,
                    h: Some(bar_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        ui.set_a11y(title_id, ROLE_TEXT, title)?;
        Ok(Self {
            panel,
            title: title_id,
            subtitle: None,
            back: None,
            actions,
            modern: true,
            large: false,
        })
    }

    /// 56dp 现代导航栏高度（Product @3x = 168px；Compact @2x = 112px）。
    pub fn height(ctx: &WidgetContext) -> u32 {
        ctx.metrics.effective_px(224)
    }

    fn build<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        back_icon: Option<(u16, u16)>,
    ) -> Result<Self, UiError> {
        // 无导航位：保留 F4 legacy 标题栏，既有 app/golden 零漂；现代
        // 56dp 导航栏只由 with_back 显式启用，使用 OH light surface + text。
        if back_icon.is_none() {
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
            return Ok(Self {
                panel,
                title: title_id,
                subtitle: None,
                back: None,
                actions: panel,
                modern: false,
                large: false,
            });
        }

        let bar_h = Self::height(ctx);
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, None, Some(bar_h), 0, 0, 0),
        )?;
        let (viewport_w, _) = ui.viewport();
        let line = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.metrics.effective_px(16),
                    w: Some(viewport_w),
                    h: Some(bar_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;

        let icons = back_icon.expect("checked above");
        let back = Some(IconButton::new(
            ui,
            line,
            ctx,
            icons.0,
            icons.1,
            b"Back",
            InteractionState::Normal,
            false,
        )?);
        let leading_flex = add(
            ui,
            line,
            panel_spec(ctx.theme.colors.surface, None, Some(bar_h), 0, 0, 0),
        )?;
        ui.set_flex(leading_flex, 1)?;
        let title_id = add(
            ui,
            line,
            text_spec(
                title,
                ctx.theme.colors.text_primary,
                ctx.metrics.typography(TypographyToken::Title),
            )?,
        )?;
        let trailing_flex = add(
            ui,
            line,
            panel_spec(ctx.theme.colors.surface, None, Some(bar_h), 0, 0, 0),
        )?;
        ui.set_flex(trailing_flex, 1)?;
        let actions = add(
            ui,
            trailing_flex,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: None,
                    h: Some(bar_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;

        ui.set_a11y(title_id, ROLE_TEXT, title)?;
        Ok(Self {
            panel,
            title: title_id,
            subtitle: None,
            back,
            actions,
            modern: true,
            large: false,
        })
    }

    /// 本实例建成高度（legacy 与 with_back 两形态单源）。
    pub fn instance_height(&self, ctx: &WidgetContext) -> u32 {
        if self.large {
            return Self::expanded_height(ctx, self.subtitle.is_some());
        }
        if self.modern {
            Self::height(ctx)
        } else {
            let title_h = ctx.metrics.typography(TypographyToken::Title).cell_h() as u32;
            title_h.saturating_add(ctx.spacing(SpacingToken::S).saturating_mul(2))
        }
    }
}

/// IconButton：顶栏/工具栏图标按钮。像素经 IconSource；本组件只负责
/// 48dp 命中面、状态色、a11y/focus 与 Press/Click 识别。
///
/// `on_primary` 声明按钮位于 primary 表面：Normal 使用 primary/on_primary，
/// Pressed/Focused 切 surface_variant/primary，形成现代系统按压反馈。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconButton {
    pub panel: NodeId,
    pub icon: NodeId,
    normal_icon: u16,
    active_icon: u16,
    base: InteractionState,
    focused: bool,
    on_primary: bool,
}

impl IconButton {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, normal_icon: u16,
        active_icon: u16, label: &[u8], base: InteractionState, on_primary: bool,
    ) -> Result<Self, UiError> {
        let target = ctx.metrics.effective_px(192);
        let visual = ctx.metrics.effective_px(96);
        let inset = target.saturating_sub(visual) / 2;
        let (bg, fg) = Self::colors(ctx, base, false, on_primary);
        let panel = add(ui, parent, panel_spec(bg, Some(target), Some(target), inset, 0, target / 2))?;
        let icon_id_node = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Icon,
                key: 0,
                props: Props::Icon(IconProps {
                    id: normal_icon,
                    fg,
                    w: Some(visual),
                    h: Some(visual),
                }),
            },
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
        Ok(Self {
            panel,
            icon: icon_id_node,
            normal_icon,
            active_icon,
            base,
            focused: false,
            on_primary,
        })
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

    fn colors(
        ctx: &WidgetContext, state: InteractionState, focused: bool, on_primary: bool,
    ) -> (Color, Color) {
        if !on_primary {
            let visual = ctx.theme.state(state);
            return (visual.background, visual.content);
        }
        match state {
            InteractionState::Normal if focused => {
                (ctx.theme.colors.surface_variant, ctx.theme.colors.text_primary)
            }
            InteractionState::Normal => (ctx.theme.colors.primary, ctx.theme.colors.on_primary),
            InteractionState::Pressed => {
                (ctx.theme.colors.surface_variant, ctx.theme.colors.primary)
            }
            InteractionState::Disabled => {
                (ctx.theme.colors.surface_variant, ctx.theme.colors.text_secondary)
            }
            InteractionState::Selected | InteractionState::Focused => {
                (ctx.theme.colors.surface_variant, ctx.theme.colors.primary)
            }
        }
    }

    fn apply<const N: usize>(
        &self, ui: &mut Ui<N>, ctx: &WidgetContext, state: InteractionState,
    ) -> Result<(), UiError> {
        let (bg, fg) = Self::colors(ctx, state, self.focused, self.on_primary);
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = bg;
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.icon) {
            if let Props::Icon(mut p) = props {
                p.id = match state {
                    InteractionState::Normal => self.normal_icon,
                    _ => self.active_icon,
                };
                p.fg = fg;
                ui.set_props(self.icon, Props::Icon(p))?;
            }
        }
        Ok(())
    }

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
        if node != self.panel && node != self.icon {
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
            Msg::FocusIn { .. } => self.set_focused(ui, ctx, true)?,
            Msg::FocusOut { .. } => self.set_focused(ui, ctx, false)?,
            _ => {}
        }
        Ok(false)
    }
}

/// Card：可按压表面容器。调用方直接向 `panel` 添加内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    pub panel: NodeId,
    pressed: bool,
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
        Ok(Self { panel, pressed: false })
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    /// Apply card press feedback without changing its layout footprint. This
    /// is a token-colored background plus a 2dp paint offset; width and
    /// height stay immutable so siblings and nested content never relayout.
    pub fn set_pressed<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, pressed: bool,
    ) -> Result<(), UiError> {
        self.pressed = pressed;
        let visual = ctx.theme.state(if pressed {
            InteractionState::Pressed
        } else {
            InteractionState::Normal
        });
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut panel) = props {
                panel.bg = visual.background;
                ui.set_props(self.panel, Props::Panel(panel))?;
            }
        }
        let nudge = if pressed {
            i32::try_from(ctx.metrics.effective_px(8)).unwrap_or(2)
        } else {
            0
        };
        ui.set_offset(self.panel, 0, nudge)?;
        ui.layout();
        Ok(())
    }

    /// Card is a container, not the business action owner. It only consumes
    /// direct Press/Release/Cancel visuals on its surface; Click activation
    /// remains the app's decision.
    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        let pressed = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::Click { node } if *node == self.panel => {
                matches!(msg, Msg::Press { .. })
            }
            _ => return Ok(false),
        };
        self.set_pressed(ui, ctx, pressed)?;
        Ok(false)
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
    max_width: u32,
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
        if spec.width == 0 || spec.kind == TextInputKind::Multiline {
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
                    radius: 0,
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
                    radius: 0,
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
            max_width: spec.width,
            focused: false,
            scroll_x: 0,
        })
    }

    pub fn kind(&self) -> TextInputKind {
        self.kind
    }

    /// LAYOUT-L5 shrink-to-width 最小面：把 panel 宽收敛到 `avail`，但
    /// 不低于“左右 Xs padding + 一个 Body 字宽 + caret 宽”的可输入下限，
    /// 且永不超出构造规格宽度。返回实际应用宽；调用方可用返回值识别
    /// floor 溢出。
    pub fn shrink_to_width<const N: usize>(
        &mut self,
        ui: &mut Ui<N>,
        ctx: &WidgetContext,
        avail_w: u32,
    ) -> Result<u32, UiError> {
        let pad = ctx.spacing(SpacingToken::Xs);
        let body = ctx.metrics.typography(TypographyToken::Body);
        let floor = (body.cell_w() as u32)
            .saturating_add(pad.saturating_mul(2))
            .saturating_add(text_caret_width(ctx));
        let target = self.max_width.min(avail_w.max(floor));
        if target == 0 {
            return Err(UiError::NodeBad);
        }
        let Some(props) = retained_props(ui, self.panel) else {
            return Err(UiError::NodeBad);
        };
        let Some(Props::Panel(mut panel)) = Some(props) else {
            return Err(UiError::NodeBad);
        };
        panel.w = Some(target);
        ui.set_props(self.panel, Props::Panel(panel))?;
        Ok(target)
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

/// TextField 的 label / supporting / error 槽位规格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFieldSlotsSpec<'a> {
    pub field: TextFieldSpec,
    pub label: Option<&'a [u8]>,
    pub supporting: Option<&'a [u8]>,
}

/// 槽位下方消息类别。error 是视觉/可读性状态，业务校验归 app。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldMessageKind {
    Supporting,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldMessage<'a> {
    pub kind: FieldMessageKind,
    pub text: &'a [u8],
}

/// TextFieldSlots：不改变既有 TextField，外裹 label、field 与消息行。
///
/// label/message 是静态可读文本，不额外占用 a11y/SEM；panel 仍是唯一
/// 焦点与语义锚。Click 请求焦点仍由 app 显式调用 request_focus。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFieldSlots {
    pub root: NodeId,
    pub label: Option<NodeId>,
    pub field: TextField,
    pub message: NodeId,
    message_kind: FieldMessageKind,
}

impl TextFieldSlots {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext,
        spec: TextFieldSlotsSpec<'_>,
    ) -> Result<Self, UiError> {
        if spec.field.width == 0 {
            return Err(UiError::NodeBad);
        }
        let root = add(
            ui,
            parent,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: Some(spec.field.width),
                    h: None,
                    cross: CrossAlign::Stretch,
                }),
            },
        )?;
        let label = match spec.label.filter(|bytes| !bytes.is_empty()) {
            Some(bytes) => Some(add(
                ui,
                root,
                text_spec(
                    bytes,
                    ctx.theme.colors.text_secondary,
                    ctx.metrics.typography(TypographyToken::Body),
                )?,
            )?),
            None => None,
        };
        let field = TextField::new(ui, root, ctx, spec.field)?;
        let message = add(
            ui,
            root,
            text_spec(
                b"",
                ctx.theme.colors.text_secondary,
                ctx.metrics.typography(TypographyToken::Body),
            )?,
        )?;
        let mut slots = Self {
            root,
            label,
            field,
            message,
            message_kind: FieldMessageKind::Supporting,
        };
        if let Some(text) = spec.supporting.filter(|bytes| !bytes.is_empty()) {
            slots.set_message(
                ui,
                ctx,
                Some(FieldMessage {
                    kind: FieldMessageKind::Supporting,
                    text,
                }),
            )?;
        }
        Ok(slots)
    }

    pub fn message_kind(&self) -> FieldMessageKind {
        self.message_kind
    }

    pub fn set_message<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, message: Option<FieldMessage<'_>>,
    ) -> Result<(), UiError> {
        let (text, fg, kind) = match message {
            Some(message) => (
                message.text,
                if message.kind == FieldMessageKind::Error {
                    ctx.theme.colors.danger
                } else {
                    ctx.theme.colors.text_secondary
                },
                message.kind,
            ),
            None => (
                b"" as &[u8],
                ctx.theme.colors.text_secondary,
                FieldMessageKind::Supporting,
            ),
        };
        self.message_kind = kind;
        ui.set_text(self.message, text)?;
        if let Some(props) = retained_props(ui, self.message) {
            if let Props::Text(mut props) = props {
                props.style.fg = fg;
                ui.set_props(self.message, Props::Text(props))?;
            }
        }
        if let Some(label) = self.label {
            if let Some(props) = retained_props(ui, label) {
                if let Props::Text(mut props) = props {
                    props.style.fg = if kind == FieldMessageKind::Error {
                        ctx.theme.colors.danger
                    } else {
                        ctx.theme.colors.text_secondary
                    };
                    ui.set_props(label, Props::Text(props))?;
                }
            }
        }
        Ok(())
    }

    pub fn owns(&self, node: NodeId) -> bool {
        self.field.owns(node) || node == self.root || self.label == Some(node) || node == self.message
    }

    pub fn request_focus<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext,
    ) -> Result<crate::desc::MsgBuf, UiError> {
        self.field.request_focus(ui, ctx)
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        self.field.on_msg(ui, ctx, msg)
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

/// 静态位图叶子。像素供给仍走唯一 Icon/Image 资源缝；w/h 为 None 时
/// 表示由源资源尺寸与父约束推导。标签进 a11y，图片内容不进语义 value。
pub fn image<const N: usize>(
    ui: &mut Ui<N>, parent: NodeId, res: u16, w: Option<u32>, h: Option<u32>,
    label: &[u8],
) -> Result<NodeId, UiError> {
    let node = add(
        ui,
        parent,
        ViewSpec {
            kind: ViewKind::Image,
            key: 0,
            props: Props::Image(crate::desc::ImageProps { res, w, h }),
        },
    )?;
    ui.set_a11y(node, crate::ROLE_IMAGE, label)?;
    Ok(node)
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
                radius: 0,
            }),
        },
    )
}

/// Button 的三方共性强调档。`Standard` 保持 BerryUI 原状态色；High 对齐
/// filled/prominent，Medium 对齐 tonal/tinted，Low 对齐 plain/text。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEmphasis {
    Standard,
    High,
    Medium,
    Low,
}

/// Button：状态视觉 + a11y + Press/Click 识别。业务动作由 app 处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Button {
    pub panel: NodeId,
    pub label: NodeId,
    base: InteractionState,
    emphasis: ButtonEmphasis,
    focused: bool,
}

impl Button {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, label: &[u8],
        base: InteractionState,
    ) -> Result<Self, UiError> {
        Self::with_emphasis(ui, parent, ctx, label, base, ButtonEmphasis::Standard)
    }

    pub fn with_emphasis<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, label: &[u8],
        base: InteractionState, emphasis: ButtonEmphasis,
    ) -> Result<Self, UiError> {
        let pad = ctx.spacing(SpacingToken::S);
        let (background, foreground) = Self::palette(ctx, base, emphasis);
        let panel = add(ui, parent, panel_spec(background, None, None, pad, 0, ctx.metrics.radius(RadiusToken::Small)))?;
        let text_id = add(
            ui,
            panel,
            text_spec(label, foreground, ctx.metrics.typography(TypographyToken::Body))?,
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
        Ok(Self {
            panel,
            label: text_id,
            base,
            emphasis,
            focused: false,
        })
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

    fn colors(
        &self, ctx: &WidgetContext, state: InteractionState,
    ) -> (berry_gfx::cmd::Color, berry_gfx::cmd::Color) {
        Self::palette(ctx, state, self.emphasis)
    }

    fn palette(
        ctx: &WidgetContext, state: InteractionState, emphasis: ButtonEmphasis,
    ) -> (berry_gfx::cmd::Color, berry_gfx::cmd::Color) {
        if emphasis == ButtonEmphasis::Standard || state == InteractionState::Disabled {
            let visual = ctx.theme.state(state);
            return (visual.background, visual.content);
        }
        match emphasis {
            ButtonEmphasis::High => match state {
                InteractionState::Normal => (ctx.theme.colors.primary, ctx.theme.colors.on_primary),
                InteractionState::Pressed
                | InteractionState::Focused
                | InteractionState::Selected => {
                    let visual = ctx.theme.state(InteractionState::Pressed);
                    (visual.background, ctx.theme.colors.primary)
                }
                InteractionState::Disabled => {
                    let visual = ctx.theme.state(state);
                    (visual.background, visual.content)
                }
            },
            ButtonEmphasis::Medium => match state {
                InteractionState::Pressed => {
                    let visual = ctx.theme.state(state);
                    (visual.background, ctx.theme.colors.primary)
                }
                _ => (ctx.theme.colors.surface_variant, ctx.theme.colors.primary),
            },
            ButtonEmphasis::Low => match state {
                InteractionState::Pressed => {
                    let visual = ctx.theme.state(state);
                    (visual.background, visual.content)
                }
                _ => (ctx.theme.colors.surface, ctx.theme.colors.text_primary),
            },
            ButtonEmphasis::Standard => {
                let visual = ctx.theme.state(state);
                (visual.background, visual.content)
            }
        }
    }

    fn apply<const N: usize>(
        &self, ui: &mut Ui<N>, ctx: &WidgetContext, state: InteractionState,
    ) -> Result<(), UiError> {
        let (background, foreground) = self.colors(ctx, state);
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = background;
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.label) {
            if let Props::Text(mut p) = props {
                p.style.fg = foreground;
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

pub(crate) fn retained_props<const N: usize>(ui: &Ui<N>, node: NodeId) -> Option<Props> {
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
            if selected { SEM_STATE_SELECTED } else { 0 },
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
            semantic.state = if selected { SEM_STATE_SELECTED } else { 0 };
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
        if let Some(props) = retained_props(ui, self.title) {
            if let Props::Text(mut p) = props {
                p.style.fg = visual.content;
                ui.set_props(self.title, Props::Text(p))?;
            }
        }
        Ok(())
    }

    fn owns(&self, node: NodeId) -> bool {
        node == self.panel || node == self.title || self.detail == Some(node)
    }

    /// ListItem 只消费焦点视觉；激活语义由 app 合成 Click 自理。
    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        let node = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::Click { node }
            | Msg::FocusIn { node }
            | Msg::FocusOut { node } => *node,
            _ => return Ok(false),
        };
        if !self.owns(node) {
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

/// ListTile 构造规格（Plain Data；激活语义归 app）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListTileSpec<'a> {
    pub title: &'a [u8],
    pub detail: Option<&'a [u8]>,
    pub leading_icon: u16,
    pub trailing: &'a [u8],
    pub selected: bool,
    pub width: u32,
}

/// ListTile：现代设置/目录行——卡片表面、leading icon、双行文字与 trailing
/// 指示。48dp 最小行高、48dp 触摸面语义由 panel 承担；激活仍由 app 合成。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListTile {
    pub panel: NodeId,
    pub title: NodeId,
    pub detail: Option<NodeId>,
    pub leading: NodeId,
    pub trailing: NodeId,
    selected: bool,
    focused: bool,
}

impl ListTile {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, spec: &ListTileSpec<'_>,
    ) -> Result<Self, UiError> {
        let height = ctx.metrics.effective_px(192);
        let media = ctx.metrics.effective_px(96);
        let media_pad = ctx.metrics.effective_px(16);
        let state = ctx.theme.state(if spec.selected {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        });
        let panel = add(
            ui,
            parent,
            panel_spec(
                state.background,
                Some(spec.width),
                Some(height),
                0,
                0,
                ctx.metrics.radius(RadiusToken::Medium),
            ),
        )?;
        let row = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.metrics.effective_px(16),
                    w: Some(spec.width),
                    h: Some(height),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let leading_panel = add(
            ui,
            row,
            panel_spec(
                ctx.theme.colors.primary,
                Some(media),
                Some(media),
                media_pad,
                0,
                media / 2,
            ),
        )?;
        let leading = add(
            ui,
            leading_panel,
            ViewSpec {
                kind: ViewKind::Icon,
                key: 0,
                props: Props::Icon(IconProps {
                    id: spec.leading_icon,
                    fg: ctx.theme.colors.on_primary,
                    w: Some(media.saturating_sub(media_pad.saturating_mul(2))),
                    h: Some(media.saturating_sub(media_pad.saturating_mul(2))),
                }),
            },
        )?;
        let content = add(ui, row, panel_spec(state.background, None, None, 0, 0, 0))?;
        ui.set_flex(content, 1)?;
        let title = add(
            ui,
            content,
            text_spec(spec.title, state.content, ctx.metrics.typography(TypographyToken::Body))?,
        )?;
        let detail = match spec.detail {
            Some(bytes) => Some(add(
                ui,
                content,
                text_spec(
                    bytes,
                    ctx.theme.colors.text_secondary,
                    ctx.metrics.typography(TypographyToken::Body),
                )?,
            )?),
            None => None,
        };
        let trailing = add(
            ui,
            row,
            text_spec(
                spec.trailing,
                ctx.theme.colors.text_secondary,
                ctx.metrics.typography(TypographyToken::Body),
            )?,
        )?;
        ui.set_a11y(panel, ROLE_LIST_ITEM, spec.title)?;
        ui.set_focusable(panel, Some(panel), true)?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ACTIVATE,
            if spec.selected { SEM_STATE_SELECTED } else { 0 },
            b"",
            0,
            0,
            false,
        )?)?;
        Ok(Self { panel, title, detail, leading, trailing, selected: spec.selected, focused: false })
    }

    pub fn owns(&self, node: NodeId) -> bool {
        node == self.panel
            || node == self.title
            || self.detail == Some(node)
            || node == self.leading
            || node == self.trailing
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
        if let Some(props) = retained_props(ui, self.title) {
            if let Props::Text(mut p) = props {
                p.style.fg = visual.content;
                ui.set_props(self.title, Props::Text(p))?;
            }
        }
        Ok(())
    }

    pub fn set_selected<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, selected: bool,
    ) -> Result<(), UiError> {
        self.selected = selected;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.state = if selected { SEM_STATE_SELECTED } else { 0 };
            ui.set_sem(semantic)?;
        }
        self.apply(ui, ctx, self.visual_state())
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        let node = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::FocusIn { node }
            | Msg::FocusOut { node } => *node,
            _ => return Ok(false),
        };
        if !self.owns(node) {
            return Ok(false);
        }
        match msg {
            Msg::Press { .. } => self.apply(ui, ctx, InteractionState::Pressed)?,
            Msg::Release { .. } | Msg::Cancel { .. } => {
                self.apply(ui, ctx, self.visual_state())?
            }
            Msg::FocusIn { .. } => {
                self.focused = true;
                self.apply(ui, ctx, InteractionState::Focused)?
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

/// Switch：双态开关。状态由 app 持有；`on_msg` 可在 Click 时翻转视觉态。
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
                props: Props::Rect(RectProps { bg: state.background, w: Some(spacer_w), h: Some(1), radius: 0 }),
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

/// 三方共性选择控件：Checkbox。状态由 app 持有，Click 只翻转视觉与语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checkbox {
    pub panel: NodeId,
    pub mark: NodeId,
    checked: bool,
    disabled: bool,
    pressed: bool,
    focused: bool,
}

impl Checkbox {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, checked: bool,
        disabled: bool,
    ) -> Result<Self, UiError> {
        let size = ctx.spacing(SpacingToken::L);
        let panel_w = ctx.spacing(SpacingToken::Xl).saturating_mul(2);
        let panel = add(
            ui,
            parent,
            panel_spec(
                ctx.theme.colors.surface,
                Some(panel_w),
                Some(panel_w),
                (panel_w.saturating_sub(size)) / 2,
                0,
                ctx.metrics.radius(RadiusToken::Small),
            ),
        )?;
        let visual = Self::visual(ctx, checked, disabled, false, false);
        let mark = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: if checked { visual.content } else { visual.border },
                    w: Some(size),
                    h: Some(size),
                    radius: 0,
                }),
            },
        )?;
        let label: &[u8] = if checked { b"checked" } else { b"unchecked" };
        ui.set_a11y(panel, ROLE_TOGGLE, label)?;
        ui.set_focusable(panel, Some(panel), !disabled)?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ACTIVATE,
            (if checked { SEM_STATE_CHECKED } else { 0 })
                | (if disabled { SEM_STATE_DISABLED } else { 0 }),
            label,
            0,
            0,
            false,
        )?)?;
        Ok(Self {
            panel,
            mark,
            checked,
            disabled,
            pressed: false,
            focused: false,
        })
    }

    fn visual(
        ctx: &WidgetContext, checked: bool, disabled: bool, pressed: bool, focused: bool,
    ) -> crate::theme::StateColors {
        let state = if disabled {
            InteractionState::Disabled
        } else if pressed {
            InteractionState::Pressed
        } else if focused {
            InteractionState::Focused
        } else if checked {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        };
        ctx.theme.state(state)
    }

    fn apply<const N: usize>(&self, ui: &mut Ui<N>, ctx: &WidgetContext) -> Result<(), UiError> {
        let visual = Self::visual(ctx, self.checked, self.disabled, self.pressed, self.focused);
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = if self.disabled || self.checked {
                    visual.background
                } else {
                    ctx.theme.colors.surface_variant
                };
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.mark) {
            if let Props::Rect(mut p) = props {
                p.bg = if self.checked { visual.content } else { visual.border };
                ui.set_props(self.mark, Props::Rect(p))?;
            }
        }
        Ok(())
    }

    pub fn set_checked<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, checked: bool,
    ) -> Result<(), UiError> {
        self.checked = checked;
        ui.set_a11y(
            self.panel,
            ROLE_TOGGLE,
            if checked { b"checked" } else { b"unchecked" },
        )?;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.state = (if checked { SEM_STATE_CHECKED } else { 0 })
                | (if self.disabled { SEM_STATE_DISABLED } else { 0 });
            ui.set_sem(semantic)?;
        }
        self.apply(ui, ctx)
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        let node = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::Click { node }
            | Msg::FocusIn { node }
            | Msg::FocusOut { node } => *node,
            _ => return Ok(false),
        };
        if node != self.panel || self.disabled {
            return Ok(false);
        }
        match msg {
            Msg::Press { .. } => {
                self.pressed = true;
                self.apply(ui, ctx)?;
            }
            Msg::Release { .. } | Msg::Cancel { .. } => {
                self.pressed = false;
                self.apply(ui, ctx)?;
            }
            Msg::Click { .. } => {
                self.pressed = false;
                self.set_checked(ui, ctx, !self.checked)?;
                return Ok(true);
            }
            Msg::FocusIn { .. } => {
                self.focused = true;
                self.apply(ui, ctx)?;
            }
            Msg::FocusOut { .. } => {
                self.focused = false;
                self.apply(ui, ctx)?;
            }
            _ => {}
        }
        Ok(false)
    }
}

/// 三方共性选择控件：单选圆点。单选组状态归 app；本控件只呈现一个选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Radio {
    pub panel: NodeId,
    pub dot: NodeId,
    checked: bool,
    disabled: bool,
    pressed: bool,
    focused: bool,
}

impl Radio {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, checked: bool,
        disabled: bool, label: &[u8],
    ) -> Result<Self, UiError> {
        let outer = ctx.spacing(SpacingToken::L);
        let dot = outer.saturating_sub(ctx.spacing(SpacingToken::S)).max(1);
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, Some(outer), Some(outer), 0, 0, outer / 2),
        )?;
        let state = if disabled {
            InteractionState::Disabled
        } else if checked {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        };
        let visual = ctx.theme.state(state);
        let dot_id = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Circle,
                key: 0,
                props: Props::Circle(CircleProps {
                    bg: if checked { visual.content } else { visual.background },
                    w: Some(dot),
                    h: Some(dot),
                }),
            },
        )?;
        ui.set_a11y(panel, ROLE_BUTTON, label)?;
        ui.set_focusable(panel, Some(panel), !disabled)?;
        Ok(Self {
            panel,
            dot: dot_id,
            checked,
            disabled,
            pressed: false,
            focused: false,
        })
    }

    fn visual(
        &self,
        ctx: &WidgetContext,
    ) -> crate::theme::StateColors {
        let state = if self.disabled {
            InteractionState::Disabled
        } else if self.pressed {
            InteractionState::Pressed
        } else if self.focused {
            InteractionState::Focused
        } else if self.checked {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        };
        ctx.theme.state(state)
    }

    fn apply<const N: usize>(&self, ui: &mut Ui<N>, ctx: &WidgetContext) -> Result<(), UiError> {
        let visual = self.visual(ctx);
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = visual.background;
                ui.set_props(self.panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, self.dot) {
            if let Props::Circle(mut p) = props {
                p.bg = if self.checked { visual.content } else { visual.background };
                ui.set_props(self.dot, Props::Circle(p))?;
            }
        }
        Ok(())
    }

    pub fn set_checked<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, checked: bool,
    ) -> Result<(), UiError> {
        self.checked = checked;
        self.apply(ui, ctx)
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        let node = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::Click { node }
            | Msg::FocusIn { node }
            | Msg::FocusOut { node } => *node,
            _ => return Ok(false),
        };
        if node != self.panel && node != self.dot {
            return Ok(false);
        }
        if self.disabled {
            return Ok(false);
        }
        match msg {
            Msg::Press { .. } => {
                self.pressed = true;
                self.apply(ui, ctx)?;
            }
            Msg::Release { .. } | Msg::Cancel { .. } => {
                self.pressed = false;
                self.apply(ui, ctx)?;
            }
            Msg::Click { .. } => {
                self.pressed = false;
                self.set_checked(ui, ctx, true)?;
                return Ok(true);
            }
            Msg::FocusIn { .. } => {
                self.focused = true;
                self.apply(ui, ctx)?;
            }
            Msg::FocusOut { .. } => {
                self.focused = false;
                self.apply(ui, ctx)?;
            }
            _ => {}
        }
        Ok(false)
    }
}

/// 三方共性值调节控件：Slider。`permille` 为 0..=1000 的 Plain Data 值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slider {
    pub panel: NodeId,
    pub track: NodeId,
    pub spacer: NodeId,
    pub thumb: NodeId,
    permille: u16,
    disabled: bool,
    focused: bool,
}

impl Slider {
    const PERMILLE_MAX: u16 = 1000;

    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, permille: u16,
        disabled: bool, label: &[u8],
    ) -> Result<Self, UiError> {
        let track_w = ctx.metrics.effective_px(288);
        let track_h = ctx.metrics.effective_px(8).max(2);
        let thumb = ctx.spacing(SpacingToken::M);
        let panel_h = thumb.saturating_add(ctx.spacing(SpacingToken::Xs).saturating_mul(2));
        let panel = add(
            ui,
            parent,
            panel_spec(
                if disabled { ctx.theme.state(InteractionState::Disabled).background } else { ctx.theme.colors.surface },
                Some(track_w),
                Some(panel_h),
                0,
                0,
                panel_h / 2,
            ),
        )?;
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
        let p = permille.min(Self::PERMILLE_MAX);
        let leading = (u32::from(p) * track_w.saturating_sub(thumb)) / 1000;
        let spacer = add(
            ui,
            track,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: ctx.theme.colors.surface, w: Some(leading), h: Some(1), radius: 0 }),
            },
        )?;
        let thumb_id = add(
            ui,
            track,
            ViewSpec {
                kind: ViewKind::Circle,
                key: 0,
                props: Props::Circle(CircleProps {
                    bg: if disabled { ctx.theme.colors.text_secondary } else { ctx.theme.colors.primary },
                    w: Some(thumb),
                    h: Some(thumb),
                }),
            },
        )?;
        ui.set_a11y(panel, ROLE_BUTTON, label)?;
        ui.set_focusable(panel, Some(panel), !disabled)?;
        let (value, len) = signed_decimal_bytes(i32::from(p));
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ADJUST,
            if disabled { SEM_STATE_DISABLED } else { 0 },
            &value[..len],
            0,
            0,
            false,
        )?)?;
        Ok(Self {
            panel,
            track,
            spacer,
            thumb: thumb_id,
            permille: p,
            disabled,
            focused: false,
        })
    }

    pub fn permille(&self) -> u16 { self.permille }

    pub fn set_permille<const N: usize>(
        &mut self, ui: &mut Ui<N>, _ctx: &WidgetContext, permille: u16,
    ) -> Result<(), UiError> {
        self.permille = permille.min(Self::PERMILLE_MAX);
        let Some(bounds) = ui.bounds(self.track).ok() else { return Err(UiError::NodeBad) };
        let Some(props) = retained_props(ui, self.thumb) else { return Err(UiError::NodeBad) };
        let Props::Circle(circle) = props else { return Err(UiError::NodeBad) };
        let thumb_w = circle.w.unwrap_or(0);
        let leading = (u32::from(self.permille)
            * (bounds.w as u32).saturating_sub(thumb_w))
            / 1000;
        if let Some(props) = retained_props(ui, self.spacer) {
            if let Props::Rect(mut p) = props {
                p.w = Some(leading);
                ui.set_props(self.spacer, Props::Rect(p))?;
            }
        }
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            let (bytes, len) = signed_decimal_bytes(i32::from(self.permille));
            semantic.value = TextBuf::new(&bytes[..len])?;
            ui.set_sem(semantic)?;
        }
        Ok(())
    }

    fn apply_focus<const N: usize>(
        &self, ui: &mut Ui<N>, ctx: &WidgetContext,
    ) -> Result<(), UiError> {
        let fg = if self.disabled {
            ctx.theme.colors.text_secondary
        } else if self.focused {
            ctx.theme.state(InteractionState::Focused).content
        } else {
            ctx.theme.colors.primary
        };
        if let Some(props) = retained_props(ui, self.thumb) {
            if let Props::Circle(mut p) = props {
                p.bg = fg;
                ui.set_props(self.thumb, Props::Circle(p))?;
            }
        }
        Ok(())
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<(), UiError> {
        if self.disabled { return Ok(()) }
        let (node, x) = match msg {
            Msg::Click { node } | Msg::Release { node, .. } => (*node, None),
            Msg::Press { node, x, .. }
            | Msg::DragMove { node, x, .. }
            | Msg::DragStart { node, x, .. } => (*node, Some(*x)),
            Msg::FocusIn { node } | Msg::FocusOut { node } => (*node, None),
            _ => return Ok(()),
        };
        if node != self.panel && node != self.track && node != self.spacer && node != self.thumb {
            return Ok(());
        }
        if let Some(x) = x {
            if let Some(bounds) = ui.bounds(self.track).ok() {
                let x = (i64::from(x) - i64::from(bounds.x)).clamp(0, i64::from(bounds.w)) as u32;
                let value = if bounds.w == 0 { 0 } else { x * 1000 / bounds.w as u32 };
                self.set_permille(ui, ctx, value as u16)?;
            }
        }
        match msg {
            Msg::FocusIn { .. } => {
                self.focused = true;
                self.apply_focus(ui, ctx)?;
            }
            Msg::FocusOut { .. } => {
                self.focused = false;
                self.apply_focus(ui, ctx)?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// 三方共性进度指示：定量线性进度。值归 app，组件只应用像素状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressIndicator {
    pub panel: NodeId,
    pub track: NodeId,
    pub fill: NodeId,
}

impl ProgressIndicator {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, width: u32,
        permille: u16,
    ) -> Result<Self, UiError> {
        if width == 0 { return Err(UiError::NodeBad) }
        let height = ctx.metrics.effective_px(12).max(4);
        let panel = add(ui, parent, panel_spec(ctx.theme.colors.surface, Some(width), Some(height), 0, 0, height / 2))?;
        let track = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps { gap: 0, w: Some(width), h: Some(height), cross: CrossAlign::Stretch }),
            },
        )?;
        let fill_w = ((width as u64 * u64::from(permille.min(1000)) + 999) / 1000) as u32;
        let fill = add(
            ui,
            track,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: ctx.theme.colors.primary, w: Some(fill_w.min(width)), h: Some(height), radius: 0 }),
            },
        )?;
        add(
            ui,
            track,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: ctx.theme.colors.surface_variant, w: Some(width.saturating_sub(fill_w.min(width))), h: Some(height), radius: 0 }),
            },
        )?;
        Ok(Self { panel, track, fill })
    }

    pub fn set_permille<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, permille: u16,
    ) -> Result<(), UiError> {
        let p = permille.min(1000);
        let Some(bounds) = ui.bounds(self.track).ok() else { return Err(UiError::NodeBad) };
        let fill = ((u64::from(bounds.w as u32) * u64::from(p) + 999) / 1000) as u32;
        if let Some(props) = retained_props(ui, self.fill) {
            if let Props::Rect(mut r) = props {
                r.w = Some(fill.min(bounds.w as u32));
                ui.set_props(self.fill, Props::Rect(r))?;
            }
        }
        let _ = ctx;
        Ok(())
    }
}

/// 三方共性不定进度：三枚相位点。动画 tick 归 app；本组件提供纯相位应用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadingIndicator {
    pub panel: NodeId,
    dots: [NodeId; 3],
}

impl LoadingIndicator {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext,
    ) -> Result<Self, UiError> {
        let dot = ctx.spacing(SpacingToken::S).max(4);
        let gap = ctx.spacing(SpacingToken::Xs);
        let width = dot.saturating_mul(3).saturating_add(gap.saturating_mul(2));
        let panel = add(ui, parent, panel_spec(ctx.theme.colors.surface, Some(width), Some(dot), 0, gap, 0))?;
        let mut dots = [panel; 3];
        for slot in dots.iter_mut() {
            *slot = add(
                ui,
                panel,
                ViewSpec {
                    kind: ViewKind::Circle,
                    key: 0,
                    props: Props::Circle(CircleProps { bg: ctx.theme.colors.outline, w: Some(dot), h: Some(dot) }),
                },
            )?;
        }
        Ok(Self { panel, dots })
    }

    pub fn set_phase<const N: usize>(
        &self, ui: &mut Ui<N>, ctx: &WidgetContext, phase: u8,
    ) -> Result<(), UiError> {
        for (index, &dot) in self.dots.iter().enumerate() {
            let active = u8::try_from(index).ok() == Some(phase % 3);
            let bg = if active { ctx.theme.colors.primary } else { ctx.theme.colors.outline };
            if let Some(props) = retained_props(ui, dot) {
                if let Props::Circle(mut p) = props {
                    p.bg = bg;
                    ui.set_props(dot, Props::Circle(p))?;
                }
            }
        }
        Ok(())
    }
}

/// Tab/BottomNav 共用条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabItemSpec {
    pub label: &'static [u8],
    pub icon: Option<u16>,
}

pub const TABS_MAX: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabBarStyle { Top, Bottom }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TabHandle {
    panel: NodeId,
    icon: Option<NodeId>,
    label: NodeId,
    badge: Option<NodeId>,
    badge_label: Option<NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tabs {
    pub panel: NodeId,
    items: [TabHandle; TABS_MAX],
    counts: [u32; TABS_MAX],
    cell_w: u32,
    count: usize,
    selected: usize,
    pressed: Option<usize>,
    focused: Option<usize>,
}

impl Tabs {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, style: TabBarStyle,
        items: &[TabItemSpec], selected: usize,
    ) -> Result<Self, UiError> {
        if items.is_empty() || items.len() > TABS_MAX || selected >= items.len() {
            return Err(UiError::NodeBad);
        }
        let (vp_w, _) = ui.viewport();
        let height = match style {
            TabBarStyle::Top => ctx.metrics.effective_px(96),
            TabBarStyle::Bottom => ctx.metrics.effective_px(144),
        };
        let panel = add(ui, parent, panel_spec(ctx.theme.colors.surface, Some(vp_w), Some(height), 0, 0, 0))?;
        let row = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps { gap: 0, w: Some(vp_w), h: Some(height), cross: CrossAlign::Center }),
            },
        )?;
        let cell = vp_w / items.len() as u32;
        let mut handles = [{
            let n = NodeId(0);
            (n, None, n, None, None)
        }; TABS_MAX];
        for (index, item) in items.iter().enumerate() {
            let state = if index == selected {
                ctx.theme.state(InteractionState::Selected)
            } else {
                ctx.theme.state(InteractionState::Normal)
            };
            let tab = add(
                ui,
                row,
                panel_spec(state.background, Some(cell), Some(height), 0, 0, 0),
            )?;
            let mut icon_node = None;
            if let Some(icon) = item.icon {
                icon_node = Some(
                    add(
                        ui,
                        tab,
                        ViewSpec {
                            kind: ViewKind::Icon,
                            key: 0,
                            props: Props::Icon(crate::desc::IconProps { id: icon, fg: state.content, w: Some(ctx.metrics.effective_px(48)), h: Some(ctx.metrics.effective_px(48)) }),
                        },
                    )?,
                );
            }
            let label = add(ui, tab, text_spec(item.label, state.content, ctx.metrics.typography(TypographyToken::Caption))?)?;
            ui.set_a11y(tab, ROLE_BUTTON, item.label)?;
            ui.set_focusable(tab, Some(tab), true)?;
            handles[index] = (tab, icon_node, label, None, None);
        }
        Ok(Self {
            panel,
            items: handles.map(|(panel, icon, label, badge, badge_label)| TabHandle {
                panel,
                icon,
                label,
                badge,
                badge_label,
            }),
            counts: [0; TABS_MAX],
            cell_w: cell,
            count: items.len(),
            selected,
            pressed: None,
            focused: None,
        })
    }

    pub fn selected(&self) -> usize { self.selected }

    pub fn item_panel(&self, index: usize) -> Result<NodeId, UiError> {
        self.items.get(index).map(|item| item.panel).ok_or(UiError::NodeBad)
    }

    /// Current badge count. Zero means no badge is attached.
    pub fn badge(&self, index: usize) -> Result<u32, UiError> {
        self.counts.get(index).copied().ok_or(UiError::NodeBad)
    }

    /// Badge visual node, if present. The badge is hit-transparent and does
    /// not add an a11y/SEM anchor; the owning tab remains the single anchor.
    pub fn badge_panel(&self, index: usize) -> Option<NodeId> {
        self.items.get(index).and_then(|item| item.badge)
    }

    /// Set or remove a tab count badge. Zero removes it; values above 999 are
    /// displayed as 999 while the app-facing count keeps its true value.
    pub fn set_badge<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, index: usize, count: u32,
    ) -> Result<(), UiError> {
        if index >= self.count {
            return Err(UiError::NodeBad);
        }
        self.items[index].badge_label = None;
        if let Some(old) = self.items[index].badge.take() {
            ui.remove(old)?;
        }
        self.counts[index] = count;
        if count == 0 {
            return Ok(());
        }
        let shown = count.min(999);
        let height = ctx.metrics.effective_px(24);
        let width = if shown < 10 {
            height
        } else {
            ctx.metrics.effective_px(36)
        };
        let panel = add(
            ui,
            self.items[index].panel,
            panel_spec(ctx.theme.colors.danger, Some(width), Some(height), 0, 0, height / 2),
        )?;
        let (bytes, len) = signed_decimal_bytes(shown as i32);
        let label = add(
            ui,
            panel,
            text_spec(
                &bytes[..len],
                ctx.theme.colors.on_primary,
                ctx.metrics.typography(TypographyToken::Caption),
            )?,
        )?;
        let pad = ctx.spacing(SpacingToken::Xs);
        let x = self.cell_w.saturating_sub(width).saturating_sub(pad);
        ui.set_offset(panel, x as i32, pad as i32)?;
        self.items[index].badge = Some(panel);
        self.items[index].badge_label = Some(label);
        Ok(())
    }

    fn state_for(&self, index: usize) -> InteractionState {
        if self.pressed == Some(index) {
            InteractionState::Pressed
        } else if self.focused == Some(index) {
            InteractionState::Focused
        } else if index == self.selected {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        }
    }

    fn apply_item<const N: usize>(
        &self, ui: &mut Ui<N>, ctx: &WidgetContext, index: usize,
    ) -> Result<(), UiError> {
        let visual = ctx.theme.state(self.state_for(index));
        let item = self.items.get(index).ok_or(UiError::NodeBad)?;
        if let Some(props) = retained_props(ui, item.panel) {
            if let Props::Panel(mut p) = props {
                p.bg = visual.background;
                ui.set_props(item.panel, Props::Panel(p))?;
            }
        }
        if let Some(icon) = item.icon {
            if let Some(props) = retained_props(ui, icon) {
                if let Props::Icon(mut p) = props {
                    p.fg = visual.content;
                    ui.set_props(icon, Props::Icon(p))?;
                }
            }
        }
        if let Some(props) = retained_props(ui, item.label) {
            if let Props::Text(mut p) = props {
                p.style.fg = visual.content;
                ui.set_props(item.label, Props::Text(p))?;
            }
        }
        Ok(())
    }

    fn index_of(&self, node: NodeId) -> Option<usize> {
        self.items.iter().take(self.count).position(|item| {
            node == item.panel
                || node == item.label
                || item.icon.is_some_and(|icon| node == icon)
                || item.badge.is_some_and(|badge| node == badge)
                || item.badge_label.is_some_and(|badge_label| node == badge_label)
        })
    }

    pub fn set_selected<const N: usize>(
        &mut self, ui: &mut Ui<N>, _ctx: &WidgetContext, selected: usize,
    ) -> Result<(), UiError> {
        if selected >= self.count {
            return Err(UiError::NodeBad);
        }
        self.selected = selected;
        for index in 0..self.count {
            self.apply_item(ui, _ctx, index)?;
        }
        Ok(())
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        let node = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::Click { node }
            | Msg::FocusIn { node }
            | Msg::FocusOut { node } => *node,
            _ => return Ok(false),
        };
        let Some(index) = self.index_of(node) else {
            return Ok(false);
        };
        match msg {
            Msg::Press { .. } => {
                self.pressed = Some(index);
                self.apply_item(ui, ctx, index)?;
            }
            Msg::Release { .. } | Msg::Cancel { .. } => {
                self.pressed = None;
                self.apply_item(ui, ctx, index)?;
            }
            Msg::Click { .. } => {
                self.pressed = None;
                self.set_selected(ui, ctx, index)?;
                return Ok(true);
            }
            Msg::FocusIn { .. } => {
                self.focused = Some(index);
                self.apply_item(ui, ctx, index)?;
            }
            Msg::FocusOut { .. } => {
                self.focused = None;
                self.apply_item(ui, ctx, index)?;
            }
            _ => {}
        }
        Ok(false)
    }
}


/// SegmentedControl 条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentedItemSpec {
    pub label: &'static [u8],
    pub disabled: bool,
}

pub const SEGMENTED_MAX: usize = 4;

/// 三方共性分段选择。单选状态归组件视觉、业务含义归 app。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentedControl {
    pub panel: NodeId,
    items: [(NodeId, NodeId); SEGMENTED_MAX],
    disabled: [bool; SEGMENTED_MAX],
    count: usize,
    selected: usize,
    pressed: Option<usize>,
    focused: Option<usize>,
}

impl SegmentedControl {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, items: &[SegmentedItemSpec],
        selected: usize,
    ) -> Result<Self, UiError> {
        if items.is_empty() || items.len() > SEGMENTED_MAX || selected >= items.len() {
            return Err(UiError::NodeBad);
        }
        let (vp_w, _) = ui.viewport();
        let width = ctx.metrics.effective_px(360).min(vp_w);
        let height = ctx.metrics.effective_px(48);
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface_variant, Some(width), Some(height), 0, 0, 0),
        )?;
        let row = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: 0,
                    w: Some(width),
                    h: Some(height),
                    cross: CrossAlign::Stretch,
                }),
            },
        )?;
        let cell = width / items.len() as u32;
        let mut handles = [(NodeId(0), NodeId(0)); SEGMENTED_MAX];
        let mut disabled = [false; SEGMENTED_MAX];
        for (index, item) in items.iter().enumerate() {
            disabled[index] = item.disabled;
            let state = if item.disabled {
                InteractionState::Disabled
            } else if index == selected {
                InteractionState::Selected
            } else {
                InteractionState::Normal
            };
            let visual = ctx.theme.state(state);
            let cell_panel = add(
                ui,
                row,
                panel_spec(visual.background, Some(cell), Some(height), 0, 0, 0),
            )?;
            let label = add(
                ui,
                cell_panel,
                text_spec(item.label, visual.content, ctx.metrics.typography(TypographyToken::Body))?,
            )?;
            ui.set_a11y(cell_panel, ROLE_BUTTON, item.label)?;
            ui.set_focusable(cell_panel, Some(cell_panel), !item.disabled)?;
            ui.set_sem(SemEntry::new(
                cell_panel,
                SEM_ACTION_ACTIVATE,
                (if item.disabled { SEM_STATE_DISABLED } else { 0 })
                    | (if index == selected { SEM_STATE_SELECTED } else { 0 }),
                item.label,
                0,
                0,
                false,
            )?)?;
            handles[index] = (cell_panel, label);
        }
        Ok(Self {
            panel,
            items: handles,
            disabled,
            count: items.len(),
            selected,
            pressed: None,
            focused: None,
        })
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn item_panel(&self, index: usize) -> Result<NodeId, UiError> {
        self.items.get(index).map(|item| item.0).ok_or(UiError::NodeBad)
    }

    pub fn owns(&self, node: NodeId) -> bool {
        self.items
            .iter()
            .take(self.count)
            .any(|(panel, label)| node == *panel || node == *label)
    }

    fn state_for(&self, index: usize) -> InteractionState {
        if self.disabled[index] {
            InteractionState::Disabled
        } else if self.pressed == Some(index) {
            InteractionState::Pressed
        } else if self.focused == Some(index) {
            InteractionState::Focused
        } else if index == self.selected {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        }
    }

    fn apply_item<const N: usize>(
        &self, ui: &mut Ui<N>, ctx: &WidgetContext, index: usize,
    ) -> Result<(), UiError> {
        let visual = ctx.theme.state(self.state_for(index));
        let (panel, label) = self.items.get(index).ok_or(UiError::NodeBad)?;
        if let Some(props) = retained_props(ui, *panel) {
            if let Props::Panel(mut p) = props {
                p.bg = visual.background;
                ui.set_props(*panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, *label) {
            if let Props::Text(mut p) = props {
                p.style.fg = visual.content;
                ui.set_props(*label, Props::Text(p))?;
            }
        }
        Ok(())
    }

    pub fn set_selected<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, selected: usize,
    ) -> Result<(), UiError> {
        if selected >= self.count || self.disabled[selected] {
            return Err(UiError::NodeBad);
        }
        self.selected = selected;
        for index in 0..self.count {
            self.apply_item(ui, ctx, index)?;
            if let Some(mut semantic) = ui.sem_of(self.items[index].0) {
                semantic.state = (if self.disabled[index] { SEM_STATE_DISABLED } else { 0 })
                    | (if index == selected { SEM_STATE_SELECTED } else { 0 });
                ui.set_sem(semantic)?;
            }
        }
        Ok(())
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        let node = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::Click { node }
            | Msg::FocusIn { node }
            | Msg::FocusOut { node } => *node,
            _ => return Ok(false),
        };
        let Some(index) = self
            .items
            .iter()
            .take(self.count)
            .position(|(panel, label)| node == *panel || node == *label)
        else {
            return Ok(false);
        };
        match msg {
            Msg::Press { .. } => {
                self.pressed = Some(index);
                self.apply_item(ui, ctx, index)?;
            }
            Msg::Release { .. } | Msg::Cancel { .. } => {
                self.pressed = None;
                self.apply_item(ui, ctx, index)?;
            }
            Msg::Click { .. } => {
                self.pressed = None;
                if self.disabled[index] {
                    return Ok(false);
                }
                self.set_selected(ui, ctx, index)?;
                return Ok(true);
            }
            Msg::FocusIn { .. } => {
                self.focused = Some(index);
                self.apply_item(ui, ctx, index)?;
            }
            Msg::FocusOut { .. } => {
                self.focused = None;
                self.apply_item(ui, ctx, index)?;
            }
            _ => {}
        }
        Ok(false)
    }
}

fn signed_decimal_bytes(value: i32) -> ([u8; 16], usize) {
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut out = [0u8; 16];
    let mut len = 0usize;
    if magnitude == 0 {
        out[0] = b'0';
        return (out, 1);
    }
    while magnitude != 0 && len < out.len() {
        out[len] = b'0' + (magnitude % 10) as u8;
        magnitude /= 10;
        len += 1;
    }
    if negative && len < out.len() {
        out[len] = b'-';
        len += 1;
    }
    out[..len].reverse();
    (out, len)
}

/// 三方共性步进器。值区间由 app 明示；越界输入被夹取，不产生第二真相。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stepper {
    pub panel: NodeId,
    pub decrement: Button,
    pub increment: Button,
    pub value_label: NodeId,
    value: i32,
    min: i32,
    max: i32,
}

impl Stepper {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, value: i32, min: i32, max: i32,
    ) -> Result<Self, UiError> {
        if min > max {
            return Err(UiError::NodeBad);
        }
        let initial = value.clamp(min, max);
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, None, None, ctx.spacing(SpacingToken::Xs), 0, 0),
        )?;
        let line = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: None,
                    h: None,
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let decrement = Button::new(ui, line, ctx, b"-", InteractionState::Normal)?;
        let (bytes, len) = signed_decimal_bytes(initial);
        let value_label = add(
            ui,
            line,
            text_spec(
                &bytes[..len],
                ctx.theme.colors.text_primary,
                ctx.metrics.typography(TypographyToken::Body),
            )?,
        )?;
        let increment = Button::new(ui, line, ctx, b"+", InteractionState::Normal)?;
        ui.set_a11y(panel, ROLE_TEXT, b"Stepper")?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ADJUST,
            0,
            &bytes[..len],
            0,
            0,
            true,
        )?)?;
        Ok(Self {
            panel,
            decrement,
            increment,
            value_label,
            value: initial,
            min,
            max,
        })
    }

    pub fn value(&self) -> i32 {
        self.value
    }

    pub fn set_value<const N: usize>(
        &mut self, ui: &mut Ui<N>, _ctx: &WidgetContext, value: i32,
    ) -> Result<(), UiError> {
        let clamped = value.clamp(self.min, self.max);
        self.value = clamped;
        let (bytes, len) = signed_decimal_bytes(clamped);
        ui.set_text(self.value_label, &bytes[..len])?;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.value = TextBuf::new(&bytes[..len])?;
            ui.set_sem(semantic)?;
        }
        Ok(())
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        if self.decrement.on_msg(ui, ctx, msg)? {
            self.set_value(ui, ctx, self.value.saturating_sub(1))?;
            return Ok(true);
        }
        if self.increment.on_msg(ui, ctx, msg)? {
            self.set_value(ui, ctx, self.value.saturating_add(1))?;
            return Ok(true);
        }
        Ok(false)
    }
}

/// 芯片类别：Assist 只触发动作；Filter 维护选中态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    Assist,
    Filter,
}

/// 三方共性芯片。紧凑令牌动作/过滤，不承载业务回调。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chip {
    pub panel: NodeId,
    pub label: NodeId,
    kind: ChipKind,
    selected: bool,
    pressed: bool,
    focused: bool,
}

impl Chip {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, label: &[u8], kind: ChipKind,
        selected: bool,
    ) -> Result<Self, UiError> {
        let height = ctx.metrics.effective_px(48);
        let state = if selected {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        };
        let visual = ctx.theme.state(state);
        let panel = add(
            ui,
            parent,
            panel_spec(
                visual.background,
                None,
                Some(height),
                ctx.spacing(SpacingToken::S),
                0,
                height / 2,
            ),
        )?;
        let label_node = add(
            ui,
            panel,
            text_spec(label, visual.content, ctx.metrics.typography(TypographyToken::Body))?,
        )?;
        ui.set_a11y(panel, ROLE_BUTTON, label)?;
        ui.set_focusable(panel, Some(panel), true)?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ACTIVATE,
            if selected { SEM_STATE_SELECTED } else { 0 },
            label,
            0,
            0,
            false,
        )?)?;
        Ok(Self {
            panel,
            label: label_node,
            kind,
            selected,
            pressed: false,
            focused: false,
        })
    }

    pub fn selected(&self) -> bool {
        self.selected
    }

    fn state(&self) -> InteractionState {
        if self.pressed {
            InteractionState::Pressed
        } else if self.focused {
            InteractionState::Focused
        } else if self.selected {
            InteractionState::Selected
        } else {
            InteractionState::Normal
        }
    }

    fn apply<const N: usize>(&self, ui: &mut Ui<N>, ctx: &WidgetContext) -> Result<(), UiError> {
        let visual = ctx.theme.state(self.state());
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

    pub fn set_selected<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, selected: bool,
    ) -> Result<(), UiError> {
        self.selected = selected;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.state = if selected { SEM_STATE_SELECTED } else { 0 };
            ui.set_sem(semantic)?;
        }
        self.apply(ui, ctx)
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
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
            Msg::Press { .. } => {
                self.pressed = true;
                self.apply(ui, ctx)?;
            }
            Msg::Release { .. } | Msg::Cancel { .. } => {
                self.pressed = false;
                self.apply(ui, ctx)?;
            }
            Msg::Click { .. } => {
                self.pressed = false;
                if self.kind == ChipKind::Filter {
                    self.set_selected(ui, ctx, !self.selected)?;
                } else {
                    self.apply(ui, ctx)?;
                }
                return Ok(true);
            }
            Msg::FocusIn { .. } => {
                self.focused = true;
                self.apply(ui, ctx)?;
            }
            Msg::FocusOut { .. } => {
                self.focused = false;
                self.apply(ui, ctx)?;
            }
            _ => {}
        }
        Ok(false)
    }
}

/// 三方共性计数/圆点徽标。宿主负责把它作为兄弟节点偏移到目标锚点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Badge {
    pub panel: NodeId,
    pub label: Option<NodeId>,
    count: u32,
}

impl Badge {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, count: u32, x: i32, y: i32,
    ) -> Result<Self, UiError> {
        let shown = count.min(999);
        if shown == 0 {
            let dot = ctx.metrics.effective_px(12);
            let panel = add(
                ui,
                parent,
                ViewSpec {
                    kind: ViewKind::Circle,
                    key: 0,
                    props: Props::Circle(CircleProps {
                        bg: ctx.theme.colors.danger,
                        w: Some(dot),
                        h: Some(dot),
                    }),
                },
            )?;
            ui.set_a11y(panel, ROLE_TEXT, b"0")?;
            ui.set_offset(panel, x, y)?;
            return Ok(Self {
                panel,
                label: None,
                count,
            });
        }
        let height = ctx.metrics.effective_px(24);
        let width = if shown < 10 {
            height
        } else {
            ctx.metrics.effective_px(36)
        };
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.danger, Some(width), Some(height), 0, 0, height / 2),
        )?;
        let (bytes, len) = signed_decimal_bytes(shown as i32);
        let label = add(
            ui,
            panel,
            text_spec(
                &bytes[..len],
                ctx.theme.colors.on_primary,
                ctx.metrics.typography(TypographyToken::Caption),
            )?,
        )?;
        ui.set_a11y(panel, ROLE_TEXT, &bytes[..len])?;
        ui.set_offset(panel, x, y)?;
        Ok(Self {
            panel,
            label: Some(label),
            count,
        })
    }

    pub fn count(&self) -> u32 {
        self.count
    }
}

pub const TEXT_PICKER_MAX: usize = 4;

/// 紧凑 TextPicker：上一项/文本/下一项的确定性选择器。
///
/// 不实现滚轮动画或惯性；这是 Plain Data 语义与 SEM_ACTION_ADJUST 的第一层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPicker {
    pub panel: NodeId,
    pub previous: Button,
    pub value_label: NodeId,
    pub next: Button,
    options: [&'static [u8]; TEXT_PICKER_MAX],
    count: usize,
    selected: usize,
}

impl TextPicker {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext,
        options: &[&'static [u8]], selected: usize,
    ) -> Result<Self, UiError> {
        if options.is_empty() || options.len() > TEXT_PICKER_MAX || selected >= options.len() {
            return Err(UiError::NodeBad);
        }
        let (vp_w, _) = ui.viewport();
        let width = ctx.metrics.effective_px(480).min(vp_w);
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, Some(width), None, ctx.spacing(SpacingToken::Xs), 0, 0),
        )?;
        let line = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: Some(width),
                    h: None,
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let previous = Button::new(ui, line, ctx, b"<", InteractionState::Normal)?;
        let value_label = add(
            ui,
            line,
            text_spec(
                options[selected],
                ctx.theme.colors.text_primary,
                ctx.metrics.typography(TypographyToken::Body),
            )?,
        )?;
        let next = Button::new(ui, line, ctx, b">", InteractionState::Normal)?;
        ui.set_a11y(panel, ROLE_TEXT, options[selected])?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ADJUST,
            SEM_STATE_SELECTED,
            options[selected],
            0,
            0,
            true,
        )?)?;
        let mut stored = [b"" as &[u8]; TEXT_PICKER_MAX];
        stored[..options.len()].copy_from_slice(options);
        Ok(Self {
            panel,
            previous,
            value_label,
            next,
            options: stored,
            count: options.len(),
            selected,
        })
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected<const N: usize>(
        &mut self, ui: &mut Ui<N>, _ctx: &WidgetContext, selected: usize,
    ) -> Result<(), UiError> {
        if selected >= self.count {
            return Err(UiError::NodeBad);
        }
        self.selected = selected;
        ui.set_text(self.value_label, self.options[selected])?;
        ui.set_a11y(self.panel, ROLE_TEXT, self.options[selected])?;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.value = TextBuf::new(self.options[selected])?;
            ui.set_sem(semantic)?;
        }
        Ok(())
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        if self.previous.on_msg(ui, ctx, msg)? {
            let target = self.selected.saturating_sub(1);
            self.set_selected(ui, ctx, target)?;
            return Ok(true);
        }
        if self.next.on_msg(ui, ctx, msg)? {
            let target = (self.selected + 1).min(self.count - 1);
            self.set_selected(ui, ctx, target)?;
            return Ok(true);
        }
        Ok(false)
    }
}

/// DatePicker 的 Plain Data 取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatePickerValue {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

pub const DATE_PICKER_YEAR_MIN: u16 = 1970;
pub const DATE_PICKER_YEAR_MAX: u16 = 2100;

/// 公历月天数。1583 以现代格里历为唯一口径；Berry v1 产品档不引入历法变体。
pub fn days_in_month(year: u16, month: u8) -> Option<u8> {
    if !(1..=12).contains(&month) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    Some(match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    })
}

fn valid_date(value: DatePickerValue) -> bool {
    (DATE_PICKER_YEAR_MIN..=DATE_PICKER_YEAR_MAX).contains(&value.year)
        && days_in_month(value.year, value.month).is_some_and(|days| value.day >= 1 && value.day <= days)
}

fn date_bytes(value: DatePickerValue) -> ([u8; 16], usize) {
    let mut out = [0u8; 16];
    let mut len = 0usize;
    let (year, year_len) = signed_decimal_bytes(i32::from(value.year));
    out[..year_len].copy_from_slice(&year[..year_len]);
    len += year_len;
    out[len] = b'-';
    len += 1;
    let (month, month_len) = signed_decimal_bytes(i32::from(value.month));
    if month_len == 1 {
        out[len] = b'0';
        out[len + 1] = month[0];
    } else {
        out[len] = month[0];
        out[len + 1] = month[1];
    }
    len += 2;
    out[len] = b'-';
    len += 1;
    let (day, day_len) = signed_decimal_bytes(i32::from(value.day));
    if day_len == 1 {
        out[len] = b'0';
        out[len + 1] = day[0];
    } else {
        out[len] = day[0];
        out[len + 1] = day[1];
    }
    len += 2;
    (out, len)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PickerColumn {
    previous: Button,
    value: NodeId,
    next: Button,
}

impl PickerColumn {
    fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, label: &[u8], width: u32,
    ) -> Result<Self, UiError> {
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, Some(width), None, ctx.spacing(SpacingToken::Xs), 0, 0),
        )?;
        let line = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: Some(width),
                    h: None,
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let previous = Button::new(ui, line, ctx, b"<", InteractionState::Normal)?;
        let value = add(
            ui,
            line,
            text_spec(label, ctx.theme.colors.text_primary, ctx.metrics.typography(TypographyToken::Body))?,
        )?;
        ui.set_a11y(value, ROLE_TEXT, label)?;
        let next = Button::new(ui, line, ctx, b">", InteractionState::Normal)?;
        Ok(Self {
            previous,
            value,
            next,
        })
    }

    fn set_label<const N: usize>(&self, ui: &mut Ui<N>, label: &[u8]) -> Result<(), UiError> {
        ui.set_text(self.value, label)?;
        ui.set_a11y(self.value, ROLE_TEXT, label)
    }

    fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<Option<bool>, UiError> {
        if self.previous.on_msg(ui, ctx, msg)? {
            return Ok(Some(true));
        }
        if self.next.on_msg(ui, ctx, msg)? {
            return Ok(Some(false));
        }
        Ok(None)
    }
}

/// 紧凑三方共性 DatePicker：年/月/日三列确定性调节。
///
/// 这是 compact/wheel picker 的 Plain Data 子集，不伪造滚轮惯性和月历格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatePicker {
    pub panel: NodeId,
    year: PickerColumn,
    month: PickerColumn,
    day: PickerColumn,
    value: DatePickerValue,
}

impl DatePicker {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, value: DatePickerValue,
    ) -> Result<Self, UiError> {
        if !valid_date(value) {
            return Err(UiError::NodeBad);
        }
        let (vp_w, _) = ui.viewport();
        let width = ctx.metrics.effective_px(640).min(vp_w);
        let column = width / 3;
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, Some(width), None, ctx.spacing(SpacingToken::Xs), 0, 0),
        )?;
        let line = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: Some(width),
                    h: None,
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let (bytes, len) = date_bytes(value);
        let year_label = b"YYYY";
        let year = PickerColumn::new(ui, line, ctx, year_label, column)?;
        let month = PickerColumn::new(ui, line, ctx, b"MM", column)?;
        let day = PickerColumn::new(ui, line, ctx, b"DD", column)?;
        let mut picker = Self {
            panel,
            year,
            month,
            day,
            value,
        };
        picker.apply_labels(ui)?;
        ui.set_a11y(panel, ROLE_TEXT, &bytes[..len])?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ADJUST,
            SEM_STATE_SELECTED,
            &bytes[..len],
            0,
            0,
            true,
        )?)?;
        Ok(picker)
    }

    pub fn value(&self) -> DatePickerValue {
        self.value
    }

    pub fn year_next(&self) -> NodeId {
        self.year.next.panel
    }

    pub fn month_next(&self) -> NodeId {
        self.month.next.panel
    }

    pub fn day_next(&self) -> NodeId {
        self.day.next.panel
    }

    fn apply_labels<const N: usize>(&mut self, ui: &mut Ui<N>) -> Result<(), UiError> {
        let (year, year_len) = signed_decimal_bytes(i32::from(self.value.year));
        self.year.set_label(ui, &year[..year_len])?;
        let (month, month_len) = signed_decimal_bytes(i32::from(self.value.month));
        self.month.set_label(ui, &month[..month_len])?;
        let (day, day_len) = signed_decimal_bytes(i32::from(self.value.day));
        self.day.set_label(ui, &day[..day_len])
    }

    pub fn set_value<const N: usize>(
        &mut self, ui: &mut Ui<N>, value: DatePickerValue,
    ) -> Result<(), UiError> {
        if !valid_date(value) {
            return Err(UiError::NodeBad);
        }
        self.value = value;
        self.apply_labels(ui)?;
        let (bytes, len) = date_bytes(value);
        ui.set_a11y(self.panel, ROLE_TEXT, &bytes[..len])?;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.value = TextBuf::new(&bytes[..len])?;
            ui.set_sem(semantic)?;
        }
        Ok(())
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        if let Some(dec) = self.year.on_msg(ui, ctx, msg)? {
            let year = if dec {
                self.value.year.saturating_sub(1).max(DATE_PICKER_YEAR_MIN)
            } else {
                self.value.year.saturating_add(1).min(DATE_PICKER_YEAR_MAX)
            };
            let mut value = self.value;
            value.year = year;
            value.day = value.day.min(days_in_month(year, value.month).unwrap_or(28));
            self.set_value(ui, value)?;
            return Ok(true);
        }
        if let Some(dec) = self.month.on_msg(ui, ctx, msg)? {
            let month = if dec {
                self.value.month.saturating_sub(1).max(1)
            } else if self.value.month == 12 {
                1
            } else {
                self.value.month + 1
            };
            let mut value = self.value;
            value.month = month;
            value.day = value.day.min(days_in_month(value.year, month).unwrap_or(28));
            self.set_value(ui, value)?;
            return Ok(true);
        }
        if let Some(dec) = self.day.on_msg(ui, ctx, msg)? {
            let days = days_in_month(self.value.year, self.value.month).unwrap_or(28);
            let day = if dec {
                self.value.day.saturating_sub(1).max(1)
            } else if self.value.day == days {
                1
            } else {
                self.value.day + 1
            };
            let mut value = self.value;
            value.day = day;
            self.set_value(ui, value)?;
            return Ok(true);
        }
        Ok(false)
    }
}

/// TimePicker 的 Plain Data 取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimePickerValue {
    pub hour: u8,
    pub minute: u8,
}

fn valid_time(value: TimePickerValue) -> bool {
    value.hour < 24 && value.minute < 60
}

fn time_bytes(value: TimePickerValue) -> ([u8; 8], usize) {
    let hour = signed_decimal_bytes(i32::from(value.hour));
    let minute = signed_decimal_bytes(i32::from(value.minute));
    let mut out = [0u8; 8];
    if hour.1 == 1 {
        out[0] = b'0';
        out[1] = hour.0[0];
    } else {
        out[..2].copy_from_slice(&hour.0[..2]);
    }
    let mut len = 2usize;
    out[len] = b':';
    len += 1;
    if minute.1 == 1 {
        out[len] = b'0';
        out[len + 1] = minute.0[0];
    } else {
        out[len] = minute.0[0];
        out[len + 1] = minute.0[1];
    }
    len += 2;
    (out, len)
}

/// 紧凑三方共性 TimePicker：时/分两列确定性调节，不伪造滚轮惯性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimePicker {
    pub panel: NodeId,
    hour: PickerColumn,
    minute: PickerColumn,
    value: TimePickerValue,
}

impl TimePicker {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, value: TimePickerValue,
    ) -> Result<Self, UiError> {
        if !valid_time(value) {
            return Err(UiError::NodeBad);
        }
        let (vp_w, _) = ui.viewport();
        let width = ctx.metrics.effective_px(480).min(vp_w);
        let column = width / 2;
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, Some(width), None, ctx.spacing(SpacingToken::Xs), 0, 0),
        )?;
        let line = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: Some(width),
                    h: None,
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let hour = PickerColumn::new(ui, line, ctx, b"HH", column)?;
        let minute = PickerColumn::new(ui, line, ctx, b"MM", column)?;
        let (bytes, len) = time_bytes(value);
        let mut picker = Self {
            panel,
            hour,
            minute,
            value,
        };
        picker.apply_labels(ui)?;
        ui.set_a11y(panel, ROLE_TEXT, &bytes[..len])?;
        ui.set_sem(SemEntry::new(
            panel,
            SEM_ACTION_ADJUST,
            SEM_STATE_SELECTED,
            &bytes[..len],
            0,
            0,
            true,
        )?)?;
        Ok(picker)
    }

    pub fn value(&self) -> TimePickerValue {
        self.value
    }

    pub fn minute_next(&self) -> NodeId {
        self.minute.next.panel
    }

    fn apply_labels<const N: usize>(&mut self, ui: &mut Ui<N>) -> Result<(), UiError> {
        let (hour, hour_len) = signed_decimal_bytes(i32::from(self.value.hour));
        self.hour.set_label(ui, &hour[..hour_len])?;
        let (minute, minute_len) = signed_decimal_bytes(i32::from(self.value.minute));
        self.minute.set_label(ui, &minute[..minute_len])
    }

    pub fn set_value<const N: usize>(
        &mut self, ui: &mut Ui<N>, value: TimePickerValue,
    ) -> Result<(), UiError> {
        if !valid_time(value) {
            return Err(UiError::NodeBad);
        }
        self.value = value;
        self.apply_labels(ui)?;
        let (bytes, len) = time_bytes(value);
        ui.set_a11y(self.panel, ROLE_TEXT, &bytes[..len])?;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.value = TextBuf::new(&bytes[..len])?;
            ui.set_sem(semantic)?;
        }
        Ok(())
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        if let Some(dec) = self.hour.on_msg(ui, ctx, msg)? {
            let hour = if dec {
                self.value.hour.saturating_sub(1)
            } else if self.value.hour == 23 {
                0
            } else {
                self.value.hour + 1
            };
            let mut value = self.value;
            value.hour = hour;
            self.set_value(ui, value)?;
            return Ok(true);
        }
        if let Some(dec) = self.minute.on_msg(ui, ctx, msg)? {
            let mut value = self.value;
            if dec {
                value.minute = value.minute.saturating_sub(1);
            } else if value.minute == 59 {
                value.minute = 0;
                value.hour = (value.hour + 1) % 24;
            } else {
                value.minute += 1;
            }
            self.set_value(ui, value)?;
            return Ok(true);
        }
        Ok(false)
    }
}

/// SearchField：前置搜索 icon、TextField、后置清除按钮的组合件。
///
/// 查询文本与 selection 仍归 app 的 TextInputState；清除 Click 只返回激活。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchField {
    pub panel: NodeId,
    pub field: TextField,
    pub clear: IconButton,
}

impl SearchField {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, width: u32, search_icon: u16,
        clear_icon: u16,
    ) -> Result<Self, UiError> {
        let leading_target = ctx.metrics.effective_px(48);
        let clear_target = ctx.metrics.effective_px(192);
        let icon_visual = ctx.metrics.effective_px(24);
        let gaps = ctx.spacing(SpacingToken::Xs).saturating_mul(2);
        if width <= leading_target + clear_target + gaps {
            return Err(UiError::NodeBad);
        }
        let panel = add(
            ui,
            parent,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: ctx.spacing(SpacingToken::Xs),
                    w: Some(width),
                    h: None,
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Icon,
                key: 0,
                props: Props::Icon(IconProps {
                    id: search_icon,
                    fg: ctx.theme.colors.text_secondary,
                    w: Some(icon_visual),
                    h: Some(icon_visual),
                }),
            },
        )?;
        let field_width = width
            .saturating_sub(leading_target + clear_target + gaps);
        let field = TextField::new(
            ui,
            panel,
            ctx,
            TextFieldSpec {
                kind: TextInputKind::Text,
                width: field_width,
            },
        )?;
        let clear = IconButton::new(
            ui,
            panel,
            ctx,
            clear_icon,
            clear_icon,
            b"Clear",
            InteractionState::Normal,
            false,
        )?;
        Ok(Self {
            panel,
            field,
            clear,
        })
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        self.clear.on_msg(ui, ctx, msg)
    }
}

/// 顶层浮层宿主：full-screen scrim + 可偏移 content。
///
/// `parent` 必须是 Ui root。Root 的每个直接子树都拿全 viewport 且兄弟后画
/// 者在上，因此不引入第二布局/命中引擎即可获得真实 z-order；dismiss 通过
/// remove 整棵浮层完成。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlaySurface {
    pub root: NodeId,
    pub content: NodeId,
}

impl OverlaySurface {
    pub fn new_centered<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, content_w: u32,
        content_h: u32,
    ) -> Result<Self, UiError> {
        if content_w == 0 || content_h == 0 {
            return Err(UiError::NodeBad);
        }
        let (vp_w, vp_h) = ui.viewport();
        let root = add(
            ui,
            parent,
            panel_spec(
                ctx.theme.colors.text_primary,
                Some(vp_w),
                Some(vp_h),
                0,
                0,
                0,
            ),
        )?;
        let content = add(
            ui,
            root,
            panel_spec(
                ctx.theme.colors.surface,
                Some(content_w.min(vp_w)),
                Some(content_h.min(vp_h)),
                ctx.spacing(SpacingToken::L),
                ctx.spacing(SpacingToken::S),
                ctx.metrics.radius(RadiusToken::Large),
            ),
        )?;
        let x = ((vp_w.saturating_sub(content_w)) / 2) as i32;
        let y = ((vp_h.saturating_sub(content_h)) / 2) as i32;
        ui.set_offset(content, x, y)?;
        Ok(Self { root, content })
    }

    pub fn dismiss<const N: usize>(&self, ui: &mut Ui<N>) -> Result<(), UiError> {
        ui.remove(self.root)
    }
}

/// AlertDialog 动作结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogAction {
    None,
    Confirm,
    Cancel,
}

/// 三方共性模态：标题、正文、确认/取消动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlertDialog {
    pub surface: OverlaySurface,
    pub confirm: Button,
    pub cancel: Option<Button>,
}

impl AlertDialog {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
        body: &[u8], confirm_label: &[u8], cancel_label: Option<&[u8]>,
    ) -> Result<Self, UiError> {
        let (vp_w, _) = ui.viewport();
        let width = vp_w.min(ctx.metrics.effective_px(640)).max(ctx.metrics.effective_px(320));
        let height = ctx.metrics.effective_px(320);
        let surface = OverlaySurface::new_centered(ui, parent, ctx, width, height)?;
        text(ui, surface.content, ctx, TextVariant::Title, title)?;
        text(ui, surface.content, ctx, TextVariant::Body, body)?;
        let actions = row(ui, surface.content, ctx)?;
        let confirm = Button::with_emphasis(
            ui,
            actions,
            ctx,
            confirm_label,
            InteractionState::Normal,
            ButtonEmphasis::High,
        )?;
        let cancel = match cancel_label {
            Some(label) => Some(Button::with_emphasis(
                ui,
                actions,
                ctx,
                label,
                InteractionState::Normal,
                ButtonEmphasis::Medium,
            )?),
            None => None,
        };
        ui.set_a11y(surface.root, ROLE_BUTTON, b"Dialog")?;
        Ok(Self { surface, confirm, cancel })
    }

    pub fn dismiss<const N: usize>(&self, ui: &mut Ui<N>) -> Result<(), UiError> {
        self.surface.dismiss(ui)
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<DialogAction, UiError> {
        if self.confirm.on_msg(ui, ctx, msg)? {
            return Ok(DialogAction::Confirm);
        }
        if let Some(cancel) = self.cancel.as_mut() {
            if cancel.on_msg(ui, ctx, msg)? {
                return Ok(DialogAction::Cancel);
            }
        }
        if let Msg::Click { node } = *msg {
            if node == self.surface.root {
                return Ok(DialogAction::Cancel);
            }
        }
        Ok(DialogAction::None)
    }
}

/// Toast / Snackbar：非阻塞短消息。显示时长与 dismiss 时机归 app。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toast {
    pub root: NodeId,
    pub label: NodeId,
    pub action: Option<Button>,
}

impl Toast {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, message: &[u8],
        action_label: Option<&[u8]>,
    ) -> Result<Self, UiError> {
        let (vp_w, vp_h) = ui.viewport();
        let width = vp_w.min(ctx.metrics.effective_px(720)).max(ctx.metrics.effective_px(320));
        let height = ctx.metrics.effective_px(96);
        let root = add(
            ui,
            parent,
            panel_spec(
                ctx.theme.colors.text_primary,
                Some(width),
                Some(height),
                ctx.spacing(SpacingToken::M),
                ctx.spacing(SpacingToken::S),
                ctx.metrics.radius(RadiusToken::Medium),
            ),
        )?;
        let line = row(ui, root, ctx)?;
        let label = text(ui, line, ctx, TextVariant::Body, message)?;
        let action = match action_label {
            Some(bytes) => Some(Button::new(ui, line, ctx, bytes, InteractionState::Normal)?),
            None => None,
        };
        let x = ((vp_w.saturating_sub(width)) / 2) as i32;
        let y = (vp_h.saturating_sub(height).saturating_sub(ctx.metrics.effective_px(192))) as i32;
        ui.set_offset(root, x, y)?;
        ui.set_a11y(root, ROLE_TEXT, message)?;
        Ok(Self { root, label, action })
    }

    pub fn dismiss<const N: usize>(&self, ui: &mut Ui<N>) -> Result<(), UiError> {
        ui.remove(self.root)
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<bool, UiError> {
        if let Some(action) = self.action.as_mut() {
            if action.on_msg(ui, ctx, msg)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Dropdown Menu 条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuItemSpec {
    pub label: &'static [u8],
    pub dangerous: bool,
}

/// 锚定浮层菜单。命中只覆盖菜单本身；外部点击由 app 关闭。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Menu {
    pub root: NodeId,
    items: [(NodeId, NodeId); 6],
    dangerous: [bool; 6],
    count: usize,
    pressed: Option<usize>,
}

impl Menu {
    pub fn anchored_size(
        ctx: &WidgetContext, count: usize,
    ) -> Result<(u32, u32), UiError> {
        if count == 0 || count > 6 {
            return Err(UiError::NodeBad);
        }
        let width = ctx.metrics.effective_px(360);
        let height = ctx.metrics.effective_px(72).saturating_mul(count as u32);
        Ok((width, height))
    }

    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, x: i32, y: i32,
        items: &[MenuItemSpec],
    ) -> Result<Self, UiError> {
        if items.is_empty() || items.len() > 6 {
            return Err(UiError::NodeBad);
        }
        let (vp_w, _) = ui.viewport();
        let width = vp_w.min(ctx.metrics.effective_px(360)).max(ctx.metrics.effective_px(220));
        let height = ctx.metrics.effective_px(72).saturating_mul(items.len() as u32);
        let root = add(
            ui,
            parent,
            panel_spec(
                ctx.theme.colors.surface,
                Some(width),
                Some(height),
                ctx.spacing(SpacingToken::Xs),
                ctx.spacing(SpacingToken::Xs),
                ctx.metrics.radius(RadiusToken::Medium),
            ),
        )?;
        let mut handles = [(NodeId(0), NodeId(0)); 6];
        let mut dangerous = [false; 6];
        for (index, item) in items.iter().enumerate() {
            dangerous[index] = item.dangerous;
            let fg = if item.dangerous {
                ctx.theme.colors.danger
            } else {
                ctx.theme.colors.text_primary
            };
            let row_id = add(
                ui,
                root,
                panel_spec(ctx.theme.colors.surface, None, Some(ctx.metrics.effective_px(72)), ctx.spacing(SpacingToken::S), 0, ctx.metrics.radius(RadiusToken::Small)),
            )?;
            let label = text(ui, row_id, ctx, TextVariant::Body, item.label)?;
            if let Some(props) = retained_props(ui, label) {
                if let Props::Text(mut p) = props {
                    p.style.fg = fg;
                    ui.set_props(label, Props::Text(p))?;
                }
            }
            ui.set_a11y(row_id, ROLE_BUTTON, item.label)?;
            ui.set_focusable(row_id, Some(row_id), true)?;
            handles[index] = (row_id, label);
        }
        ui.set_offset(root, x, y)?;
        Ok(Self { root, items: handles, dangerous, count: items.len(), pressed: None })
    }

    pub fn dismiss<const N: usize>(&self, ui: &mut Ui<N>) -> Result<(), UiError> {
        ui.remove(self.root)
    }

    pub fn item_panel(&self, index: usize) -> Result<NodeId, UiError> {
        self.items.get(index).map(|(panel, _)| *panel).ok_or(UiError::NodeBad)
    }

    fn state_for(&self, index: usize) -> InteractionState {
        if self.pressed == Some(index) {
            InteractionState::Pressed
        } else {
            InteractionState::Normal
        }
    }

    fn apply_item<const N: usize>(
        &self, ui: &mut Ui<N>, ctx: &WidgetContext, index: usize, dangerous: bool,
    ) -> Result<(), UiError> {
        let Some((panel, label)) = self.items.get(index) else {
            return Err(UiError::NodeBad);
        };
        let visual = ctx.theme.state(self.state_for(index));
        let fg = if dangerous {
            ctx.theme.colors.danger
        } else {
            visual.content
        };
        if let Some(props) = retained_props(ui, *panel) {
            if let Props::Panel(mut p) = props {
                p.bg = visual.background;
                ui.set_props(*panel, Props::Panel(p))?;
            }
        }
        if let Some(props) = retained_props(ui, *label) {
            if let Props::Text(mut p) = props {
                p.style.fg = fg;
                ui.set_props(*label, Props::Text(p))?;
            }
        }
        Ok(())
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<Option<usize>, UiError> {
        let node = match msg {
            Msg::Press { node, .. }
            | Msg::Release { node, .. }
            | Msg::Cancel { node }
            | Msg::Click { node } => *node,
            _ => return Ok(None),
        };
        let Some(index) = self
            .items
            .iter()
            .take(self.count)
            .position(|(panel, label)| node == *panel || node == *label)
        else {
            return Ok(None);
        };
        let dangerous = self.dangerous[index];
        match msg {
            Msg::Press { .. } => {
                self.pressed = Some(index);
                self.apply_item(ui, ctx, index, dangerous)?;
            }
            Msg::Release { .. } | Msg::Cancel { .. } => {
                self.pressed = None;
                self.apply_item(ui, ctx, index, dangerous)?;
            }
            Msg::Click { .. } => {
                self.pressed = None;
                self.apply_item(ui, ctx, index, dangerous)?;
                return Ok(Some(index));
            }
            _ => {}
        }
        Ok(None)
    }
}


/// ContextMenu 结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuOutcome {
    None,
    Selected(usize),
    Dismissed,
}

/// 长按/右键上下文菜单：全屏 scrim + 锚定 Menu。
///
/// scrim 与 Dialog 一样挂 Ui root 后画兄弟，点击外部返回 Dismissed。
/// 触发手势由 app 的 LongPressGesture 或输入设备决定；本组件不合成事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextMenu {
    pub scrim: NodeId,
    pub menu: Menu,
}

impl ContextMenu {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, ctx: &WidgetContext, anchor_x: i32, anchor_y: i32,
        items: &[MenuItemSpec],
    ) -> Result<Self, UiError> {
        let (vp_w, vp_h) = ui.viewport();
        let (nominal_w, menu_h) = Menu::anchored_size(ctx, items.len())?;
        let menu_w = nominal_w.min(vp_w);
        let margin = ctx.metrics.effective_px(16) as i32;
        let max_x = (vp_w.saturating_sub(menu_w).saturating_sub(margin as u32)).max(0) as i32;
        let max_y = (vp_h.saturating_sub(menu_h).saturating_sub(margin as u32)).max(0) as i32;
        let x = anchor_x.clamp(margin, max_x.max(margin));
        let y = anchor_y.clamp(margin, max_y.max(margin));
        let overlay_root = ui.root();
        let scrim = add(
            ui,
            overlay_root,
            panel_spec(ctx.theme.colors.text_primary, Some(vp_w), Some(vp_h), 0, 0, 0),
        )?;
        let menu = Menu::new(ui, scrim, ctx, x, y, items)?;
        ui.set_a11y(scrim, ROLE_BUTTON, b"Context menu")?;
        Ok(Self { scrim, menu })
    }

    pub fn dismiss<const N: usize>(&self, ui: &mut Ui<N>) -> Result<(), UiError> {
        ui.remove(self.scrim)
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
    ) -> Result<ContextMenuOutcome, UiError> {
        if let Some(index) = self.menu.on_msg(ui, ctx, msg)? {
            self.dismiss(ui)?;
            return Ok(ContextMenuOutcome::Selected(index));
        }
        match msg {
            Msg::Click { node } if *node == self.scrim => {
                self.dismiss(ui)?;
                Ok(ContextMenuOutcome::Dismissed)
            }
            Msg::Cancel { .. } => {
                self.dismiss(ui)?;
                Ok(ContextMenuOutcome::Dismissed)
            }
            _ => Ok(ContextMenuOutcome::None),
        }
    }
}

/// 底部 Sheet。detent 高度由 app 给出；dismiss 归 app 状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sheet {
    pub root: NodeId,
    pub content: NodeId,
}

impl Sheet {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, height: u32,
    ) -> Result<Self, UiError> {
        let (vp_w, vp_h) = ui.viewport();
        if height == 0 || height > vp_h {
            return Err(UiError::NodeBad);
        }
        let root = add(
            ui,
            parent,
            panel_spec(
                ctx.theme.colors.surface,
                Some(vp_w),
                Some(height),
                ctx.spacing(SpacingToken::M),
                ctx.spacing(SpacingToken::S),
                ctx.metrics.radius(RadiusToken::Large),
            ),
        )?;
        let content = add(
            ui,
            root,
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
        ui.set_offset(root, 0, (vp_h.saturating_sub(height)) as i32)?;
        Ok(Self { root, content })
    }

    pub fn dismiss<const N: usize>(&self, ui: &mut Ui<N>) -> Result<(), UiError> {
        ui.remove(self.root)
    }
}

/// 分组列表容器：标题 + 内容。三端 List Section/Form Section 的共同语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    pub panel: NodeId,
    pub header: NodeId,
    pub content: NodeId,
}

impl Section {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, title: &[u8],
    ) -> Result<Self, UiError> {
        let pad = ctx.spacing(SpacingToken::M);
        let panel = add(ui, parent, panel_spec(ctx.theme.colors.surface, None, None, pad, ctx.spacing(SpacingToken::S), ctx.metrics.radius(RadiusToken::Medium)))?;
        let header = add(ui, panel, text_spec(title, ctx.theme.colors.text_secondary, ctx.metrics.typography(TypographyToken::Caption))?)?;
        let content = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: ctx.spacing(SpacingToken::Xs), w: None, h: None, cross: CrossAlign::Stretch }),
            },
        )?;
        Ok(Self { panel, header, content })
    }
}

/// Berry 断点与 safe area 的 Plain Data 汇聚。来源必须仍归 Berry Room；
/// 本结构只消除 Scaffold/应用侧的散参数口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SafeInsets {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub ime: u32,
}

/// 现有 GridProps 的薄包装：构造时执法非零行列，返回 content 供 app 摆格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub content: NodeId,
}

impl Grid {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, cols: u8, rows: u8, gap: u32,
    ) -> Result<Self, UiError> {
        if cols == 0 || rows == 0 { return Err(UiError::NodeBad) }
        let content = add(
            ui,
            parent,
            ViewSpec {
                kind: ViewKind::Grid,
                key: 0,
                props: Props::Grid(GridProps {
                    cols,
                    rows,
                    gap,
                    cross: CrossAlign::Center,
                    w: None,
                    h: None,
                }),
            },
        )?;
        Ok(Self { content })
    }
}

/// ScrollView 轴向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollViewAxis {
    Vertical,
    Horizontal,
}

/// ScrollView：纵向/横向偏移容器。Panel 背景与 Clip port 分层：内容可溢出
/// 布局界，但 paint/hit 都被 Clip 到 viewport 内。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollView {
    pub panel: NodeId,
    pub port: NodeId,
    pub content: NodeId,
    axis: ScrollViewAxis,
    offset: i32,
    max_offset: i32,
    last_position: Option<i32>,
}

impl ScrollView {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, viewport_h: u32,
    ) -> Result<Self, UiError> {
        Self::new_axis(ui, parent, ctx, viewport_h, ScrollViewAxis::Vertical)
    }

    pub fn new_axis<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, viewport_len: u32,
        axis: ScrollViewAxis,
    ) -> Result<Self, UiError> {
        if viewport_len == 0 {
            return Err(UiError::NodeBad);
        }
        let (panel_w, panel_h, clip_w, clip_h) = match axis {
            ScrollViewAxis::Vertical => (None, Some(viewport_len), None, Some(viewport_len)),
            ScrollViewAxis::Horizontal => (Some(viewport_len), None, Some(viewport_len), None),
        };
        let panel = add(
            ui,
            parent,
            panel_spec(ctx.theme.colors.surface, panel_w, panel_h, 0, 0, 0),
        )?;
        let port = add(
            ui,
            panel,
            ViewSpec {
                kind: ViewKind::Clip,
                key: 0,
                props: Props::Clip(ClipProps { w: clip_w, h: clip_h }),
            },
        )?;
        let content = match axis {
            ScrollViewAxis::Vertical => add(
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
            )?,
            ScrollViewAxis::Horizontal => add(
                ui,
                port,
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
            )?,
        };
        Ok(Self {
            panel,
            port,
            content,
            axis,
            offset: 0,
            max_offset: 0,
            last_position: None,
        })
    }

    pub fn axis(&self) -> ScrollViewAxis {
        self.axis
    }

    pub fn sync_bounds<const N: usize>(&mut self, ui: &mut Ui<N>) -> Result<(), UiError> {
        ui.layout();
        let view = ui.bounds(self.panel)?;
        let content = ui.bounds(self.content)?;
        self.max_offset = match self.axis {
            ScrollViewAxis::Vertical => (content.h - view.h).max(0),
            ScrollViewAxis::Horizontal => (content.w - view.w).max(0),
        };
        self.clamp(ui)
    }

    pub fn scroll_by<const N: usize>(
        &mut self, ui: &mut Ui<N>, delta: i32,
    ) -> Result<(), UiError> {
        self.offset = self.offset.saturating_add(delta).clamp(0, self.max_offset);
        self.clamp(ui)
    }

    fn clamp<const N: usize>(&self, ui: &mut Ui<N>) -> Result<(), UiError> {
        match self.axis {
            ScrollViewAxis::Vertical => ui.set_offset(self.content, 0, -self.offset),
            ScrollViewAxis::Horizontal => ui.set_offset(self.content, -self.offset, 0),
        }
    }

    pub fn offset(&self) -> i32 {
        self.offset
    }

    /// Content extent minus viewport extent; negative is never stored.
    pub fn max_offset(&self) -> i32 {
        self.max_offset
    }

    /// Deterministic load-more/scroll-end predicate. Threshold is explicit;
    /// no wall clock and no callback are introduced.
    pub fn near_edge(&self, threshold_px: u32) -> bool {
        if threshold_px == 0 {
            return self.max_offset == 0;
        }
        let distance = self.max_offset.saturating_sub(self.offset).max(0);
        u32::try_from(distance).unwrap_or(u32::MAX) <= threshold_px
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, msg: &Msg,
    ) -> Result<bool, UiError> {
        let (node, position) = match msg {
            Msg::DragStart { node, x, y, .. } | Msg::DragMove { node, x, y, .. } => {
                let value = match self.axis {
                    ScrollViewAxis::Vertical => *y,
                    ScrollViewAxis::Horizontal => *x,
                };
                (*node, crate::hit::u32_to_i32_sat(value))
            }
            Msg::DragEnd { node, .. } => {
                if *node == self.panel || *node == self.port || *node == self.content {
                    self.last_position = None;
                    return Ok(true);
                }
                return Ok(false);
            }
            _ => return Ok(false),
        };
        if node != self.panel && node != self.port && node != self.content {
            return Ok(false);
        }
        if let Some(old) = self.last_position {
            let delta = old.saturating_sub(position);
            self.scroll_by(ui, delta)?;
        }
        self.last_position = Some(position);
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
