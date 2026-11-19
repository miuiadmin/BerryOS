//! PullRefresh：三方共性下拉刷新控制器。
//!
//! 这是 app 侧普通库代码，与 SwipePager 同范式：不新增 ViewKind、不新增
//! Msg、不新增第二布局/命中/焦点引擎。控制器只持有手势与呈现状态；
//! “数据正在刷新/何时完成”归 app。时间推进由 app 显式调用
//! `tick(now_ns)`，Msg 路径不读 wall-clock。
//!
//! 指示器是 ScrollView Clip 内的顶层兄弟：静止时停在 port 上方并被裁掉；
//! 下拉时 app 内容向下平移 `pull_px`，指示器被揭示。触发阈值就是指示器
//! 自身高度，因此不引入第二套密度魔数。

use crate::anim::{self, Easing};
use crate::desc::{ColProps, Msg, NodeId, Props, ViewKind, ViewSpec};
use crate::layout::CrossAlign;
use crate::theme::{SpacingToken, TypographyToken};
use crate::widgets::{
    retained_props, text_spec, LoadingIndicator, ScrollView, ScrollViewAxis, WidgetContext,
};
use crate::{Ui, UiError, ROLE_TEXT};

pub const PULL_REFRESH_SETTLE_NS: u64 = 240_000_000;
pub const PULL_REFRESH_PHASE_NS: u64 = 200_000_000;
pub const PULL_REFRESH_DRAG_DIV: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PullRefreshSpec<'a> {
    pub idle_label: &'a [u8],
    pub armed_label: &'a [u8],
    pub busy_label: &'a [u8],
}

impl Default for PullRefreshSpec<'_> {
    fn default() -> Self {
        Self {
            idle_label: b"Pull to refresh",
            armed_label: b"Release to refresh",
            busy_label: b"Refreshing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRefreshPhase {
    Idle,
    Pulling { pull_px: u32, armed: bool },
    Refreshing,
    Settling { pull_px: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRefreshEvent {
    None,
    PullChanged { pull_px: u32, armed: bool },
    Triggered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Pulling,
    Refreshing,
    Settling,
}

#[derive(Debug, Clone, Copy)]
struct Snap {
    from: i32,
    to: i32,
    start_ns: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct PullRefresh {
    host: NodeId,
    indicator: LoadingIndicator,
    label: NodeId,
    port: NodeId,
    content: NodeId,
    zone_h: u32,
    pull_px: u32,
    armed: bool,
    dragging: bool,
    start_y: i32,
    base_pull: u32,
    state: State,
    snap: Option<Snap>,
    spinner_anchor_ns: Option<u64>,
    last_dot: u8,
    labels: PullRefreshSpec<'static>,
}

impl PullRefresh {
    pub fn attach<const N: usize>(
        ui: &mut Ui<N>, scroll: &ScrollView, ctx: &WidgetContext,
        spec: &PullRefreshSpec<'static>,
    ) -> Result<Self, UiError> {
        if scroll.axis() != ScrollViewAxis::Vertical {
            return Err(UiError::NodeBad);
        }
        let host = ui.add(
            scroll.port,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: ctx.metrics.spacing(SpacingToken::Xs),
                    w: Some(0),
                    h: None,
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let indicator = LoadingIndicator::new(ui, host, ctx)?;
        let label = ui.add(
            host,
            text_spec(
                spec.idle_label,
                ctx.theme.colors.text_secondary,
                ctx.metrics.typography(TypographyToken::Body),
            )?,
        )?;
        ui.set_a11y(host, ROLE_TEXT, spec.idle_label)?;
        Ok(Self {
            host,
            indicator,
            label,
            port: scroll.port,
            content: scroll.content,
            zone_h: 0,
            pull_px: 0,
            armed: false,
            dragging: false,
            start_y: 0,
            base_pull: 0,
            state: State::Idle,
            snap: None,
            spinner_anchor_ns: None,
            last_dot: 0,
            labels: PullRefreshSpec {
                idle_label: spec.idle_label,
                armed_label: spec.armed_label,
                busy_label: spec.busy_label,
            },
        })
    }

    /// Park the indicator behind the Clip top and derive the trigger height.
    /// Call after `ScrollView::sync_bounds`; it is idempotent after rebuild.
    pub fn sync<const N: usize>(
        &mut self, ui: &mut Ui<N>, scroll: &ScrollView,
    ) -> Result<(), UiError> {
        if scroll.port != self.port || scroll.content != self.content {
            return Err(UiError::NodeBad);
        }
        ui.layout();
        let port_w = ui.bounds(self.port)?.w.max(0) as u32;
        if let Some(props) = retained_props(ui, self.host) {
            if let Props::Col(mut props) = props {
                props.w = Some(port_w);
                ui.set_props(self.host, Props::Col(props))?;
            }
        }
        ui.layout();
        let zone = ui.bounds(self.host)?.h.max(0) as u32;
        if zone == 0 {
            return Err(UiError::NodeBad);
        }
        self.zone_h = zone;
        ui.set_offset_sparse(self.host, 0, -(zone as i32))?;
        Ok(())
    }

    pub fn host(&self) -> NodeId {
        self.host
    }

    pub fn phase(&self) -> PullRefreshPhase {
        match self.state {
            State::Idle => PullRefreshPhase::Idle,
            State::Pulling => PullRefreshPhase::Pulling {
                pull_px: self.pull_px,
                armed: self.armed,
            },
            State::Refreshing => PullRefreshPhase::Refreshing,
            State::Settling => PullRefreshPhase::Settling {
                pull_px: self.pull_px,
            },
        }
    }

    fn label_for(&self, armed: bool, busy: bool) -> &'static [u8] {
        if busy {
            self.labels.busy_label
        } else if armed {
            self.labels.armed_label
        } else {
            self.labels.idle_label
        }
    }

    fn apply_pull<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, pull: u32,
    ) -> Result<(), UiError> {
        self.pull_px = pull;
        ui.set_offset_sparse(self.content, 0, pull as i32)?;
        let armed = pull >= self.zone_h;
        if armed != self.armed {
            self.armed = armed;
            let label = self.label_for(armed, false);
            ui.set_text(self.label, label)?;
            if let Some(props) = retained_props(ui, self.label) {
                if let Props::Text(mut props) = props {
                    props.style.fg = if armed {
                        ctx.theme.colors.text_primary
                    } else {
                        ctx.theme.colors.text_secondary
                    };
                    ui.set_props(self.label, Props::Text(props))?;
                }
            }
            ui.set_a11y(self.host, ROLE_TEXT, label)?;
        }
        Ok(())
    }

    fn begin_snap(&mut self, to: u32) {
        self.snap = Some(Snap {
            from: self.pull_px as i32,
            to: to as i32,
            start_ns: None,
        });
        self.state = State::Settling;
    }

    fn owns_top_gesture<const N: usize>(
        &self, ui: &Ui<N>, scroll: &ScrollView, x: u32, y: u32,
    ) -> Result<bool, UiError> {
        if scroll.offset() != 0 {
            return Ok(false);
        }
        let bounds = ui.bounds(self.port)?;
        Ok(bounds.contains(
            crate::hit::u32_to_i32_sat(x),
            crate::hit::u32_to_i32_sat(y),
        ))
    }

    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, scroll: &ScrollView,
        msg: &Msg,
    ) -> Result<PullRefreshEvent, UiError> {
        if scroll.port != self.port || scroll.content != self.content {
            return Err(UiError::NodeBad);
        }
        match *msg {
            Msg::DragStart { axis: crate::desc::Axis::Y, x, y, .. }
                if matches!(self.state, State::Idle | State::Settling)
                    && self.owns_top_gesture(ui, scroll, x, y)? =>
            {
                self.snap = None;
                self.state = State::Pulling;
                self.dragging = true;
                self.start_y = crate::hit::u32_to_i32_sat(y);
                self.base_pull = self.pull_px;
                if self.armed {
                    self.armed = false;
                }
                Ok(PullRefreshEvent::None)
            }
            Msg::DragMove { y, .. } if self.dragging => {
                let current = crate::hit::u32_to_i32_sat(y);
                let delta = current.saturating_sub(self.start_y) / PULL_REFRESH_DRAG_DIV;
                let raw = self.base_pull as i32 + delta;
                let max = self.zone_h.saturating_mul(2) as i32;
                let pull = raw.clamp(0, max) as u32;
                if pull == self.pull_px {
                    return Ok(PullRefreshEvent::None);
                }
                self.apply_pull(ui, ctx, pull)?;
                Ok(PullRefreshEvent::PullChanged {
                    pull_px: pull,
                    armed: self.armed,
                })
            }
            Msg::DragEnd { .. } if self.dragging => {
                self.dragging = false;
                self.base_pull = 0;
                if self.armed {
                    self.begin_snap(self.zone_h);
                    return Ok(PullRefreshEvent::Triggered);
                }
                self.begin_snap(0);
                Ok(PullRefreshEvent::None)
            }
            Msg::Cancel { .. } if self.dragging => {
                self.dragging = false;
                self.base_pull = 0;
                self.begin_snap(0);
                Ok(PullRefreshEvent::None)
            }
            _ => Ok(PullRefreshEvent::None),
        }
    }

    pub fn finish<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext,
    ) -> Result<(), UiError> {
        if self.state == State::Refreshing {
            self.set_label(ui, ctx, self.labels.idle_label, ctx.theme.colors.text_secondary)?;
            self.armed = false;
            self.begin_snap(0);
        }
        Ok(())
    }

    pub fn cancel<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext,
    ) -> Result<(), UiError> {
        if matches!(self.state, State::Idle | State::Refreshing) {
            return Ok(());
        }
        self.dragging = false;
        self.base_pull = 0;
        self.set_label(ui, ctx, self.labels.idle_label, ctx.theme.colors.text_secondary)?;
        self.armed = false;
        self.begin_snap(0);
        Ok(())
    }

    fn set_label<const N: usize>(
        &self, ui: &mut Ui<N>, _ctx: &WidgetContext, label: &[u8], fg: berry_gfx::cmd::Color,
    ) -> Result<(), UiError> {
        ui.set_text(self.label, label)?;
        if let Some(props) = retained_props(ui, self.label) {
            if let Props::Text(mut props) = props {
                props.style.fg = fg;
                ui.set_props(self.label, Props::Text(props))?;
            }
        }
        ui.set_a11y(self.host, ROLE_TEXT, label)
    }

    pub fn tick<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, now_ns: u64,
    ) -> Result<bool, UiError> {
        let mut changed = false;
        if let Some(snap) = self.snap.as_mut() {
            let start = *snap.start_ns.get_or_insert(now_ns);
            let elapsed = now_ns.saturating_sub(start);
            if elapsed >= PULL_REFRESH_SETTLE_NS {
                let to = snap.to;
                self.snap = None;
                self.apply_pull(ui, ctx, to.max(0) as u32)?;
                changed = true;
                if to as u32 == self.zone_h {
                    self.state = State::Refreshing;
                    self.spinner_anchor_ns = Some(now_ns);
                    self.last_dot = 0;
                    let _ = self.indicator.set_phase(ui, ctx, 0);
                    self.set_label(
                        ui,
                        ctx,
                        self.labels.busy_label,
                        ctx.theme.colors.text_primary,
                    )?;
                } else {
                    self.state = State::Idle;
                    self.pull_px = 0;
                    self.armed = false;
                    self.spinner_anchor_ns = None;
                    self.set_label(
                        ui,
                        ctx,
                        self.labels.idle_label,
                        ctx.theme.colors.text_secondary,
                    )?;
                }
            } else {
                let value = anim::interp(
                    Easing::QuadOut,
                    snap.from,
                    snap.to,
                    elapsed,
                    PULL_REFRESH_SETTLE_NS,
                );
                let value = value.max(0) as u32;
                if value != self.pull_px {
                    self.apply_pull(ui, ctx, value)?;
                    changed = true;
                }
            }
        }
        if self.state == State::Refreshing {
            if let Some(anchor) = self.spinner_anchor_ns {
                let dot = ((now_ns.saturating_sub(anchor) / PULL_REFRESH_PHASE_NS) % 3) as u8;
                if dot != self.last_dot {
                    self.last_dot = dot;
                    let _ = self.indicator.set_phase(ui, ctx, dot);
                    changed = true;
                }
            }
        }
        Ok(changed)
    }
}

