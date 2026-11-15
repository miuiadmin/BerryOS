//! TextArea：多行纯文本输入组合件（v1）。
//!
//! 换行真相只有一份：app 持有的 `TextInputState` 字节。渲染域视觉行由
//! `text_wrap::plan` 单源派生；编辑域逻辑行/粘滞列由 `text_input` 的纯
//! helper 派生。这里不复制文本、不新增 ViewKind、不新增第二布局/光栅/
//! 焦点/语义引擎。
//!
//! v1 显式不做点击定位 caret、拖拽选择、IME composition 直入、光标闪烁
//! 和富文本。Enter 换行由 `key_route(..., Multiline, true)` 收敛到
//! `Insert('\n')`，app 只应用编辑结果并调用 `sync`。

use crate::desc::{
    ClipProps, Msg, NodeId, Props, RectProps, TextBuf, ViewKind, ViewSpec,
};
use crate::metrics::TextMetrics;
use crate::text_input::TextInputState;
use crate::text_wrap::{self, WrapPlan};
use crate::theme::{
    InteractionState, RadiusToken, SpacingToken, StrokeToken, TypographyToken,
};
use crate::utf8;
use crate::widgets::{
    panel_spec, retained_props, text_spec, WidgetContext,
};
use crate::{Ui, UiError, ROLE_TEXT, SEM_ACTION_TEXT_EDIT, SemEntry};

pub const TEXT_AREA_LINES_MAX: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextAreaSpec {
    pub width: u32,
    pub lines: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextArea {
    pub panel: NodeId,
    pub port: NodeId,
    pub sel_head: NodeId,
    pub sel_band: NodeId,
    pub sel_tail: NodeId,
    pub value_text: NodeId,
    pub caret: NodeId,
    width: u32,
    lines: u32,
    focused: bool,
    scroll_y: i32,
    selection_pos: [(i32, i32); 3],
}

impl TextArea {
    pub fn new<const N: usize>(
        ui: &mut Ui<N>, parent: NodeId, ctx: &WidgetContext, spec: TextAreaSpec,
    ) -> Result<Self, UiError> {
        let body = ctx.metrics.typography(TypographyToken::Body);
        let pad = ctx.metrics.spacing(SpacingToken::Xs);
        if spec.width == 0
            || spec.lines == 0
            || spec.lines as usize > TEXT_AREA_LINES_MAX
            || (body.cell_h() as u32).saturating_mul(u32::from(spec.lines))
                .saturating_add(pad.saturating_mul(2))
                == 0
        {
            return Err(UiError::NodeBad);
        }
        let line_h = body.cell_h() as u32;
        let height = line_h
            .saturating_mul(u32::from(spec.lines))
            .saturating_add(pad.saturating_mul(2));
        let panel = ui.add(
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
        match Self::build(ui, panel, ctx, spec, line_h) {
            Ok(value) => Ok(value),
            Err(error) => {
                let _ = ui.remove(panel);
                Err(error)
            }
        }
    }

    fn build<const N: usize>(
        ui: &mut Ui<N>, panel: NodeId, ctx: &WidgetContext, spec: TextAreaSpec,
        line_h: u32,
    ) -> Result<Self, UiError> {
        let port = ui.add(
            panel,
            ViewSpec {
                kind: ViewKind::Clip,
                key: 0,
                props: Props::Clip(ClipProps {
                    w: None,
                    h: Some(line_h.saturating_mul(u32::from(spec.lines))),
                }),
            },
        )?;
        let selected = ctx.theme.state(InteractionState::Selected).background;
        let sel_head = ui.add(
            port,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: selected, w: Some(0), h: Some(line_h) }),
            },
        )?;
        let sel_band = ui.add(
            port,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: selected, w: Some(0), h: Some(0) }),
            },
        )?;
        let sel_tail = ui.add(
            port,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: selected, w: Some(0), h: Some(line_h) }),
            },
        )?;
        let value_text = ui.add(
            port,
            text_spec(
                b"",
                ctx.theme.colors.text_primary,
                ctx.metrics.typography(TypographyToken::Body),
            )?,
        )?;
        let caret = ui.add(
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
        ui.set_a11y(panel, ROLE_TEXT, b"textarea")?;
        ui.set_focusable(panel, Some(panel), true)?;
        ui.set_sem(SemEntry::new(
            panel, SEM_ACTION_TEXT_EDIT, 0, b"", 0, 0, false,
        )?)?;
        ui.set_text_wrap(value_text, true)?;
        Ok(Self {
            panel,
            port,
            sel_head,
            sel_band,
            sel_tail,
            value_text,
            caret,
            width: spec.width,
            lines: u32::from(spec.lines),
            focused: false,
            scroll_y: 0,
            selection_pos: [(0, 0); 3],
        })
    }

    pub fn is_focused<const N: usize>(&self, ui: &Ui<N>) -> bool {
        ui.focus_node() == Some(self.panel)
    }

    pub fn request_focus<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext,
    ) -> Result<crate::desc::MsgBuf, UiError> {
        let messages = ui.focus_set(self.panel)?;
        self.set_focused(ui, ctx, true)?;
        Ok(messages)
    }

    pub fn set_focused<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, focused: bool,
    ) -> Result<(), UiError> {
        self.focused = focused;
        let visual = ctx.theme.state(if focused {
            InteractionState::Focused
        } else {
            InteractionState::Normal
        });
        if let Some(props) = retained_props(ui, self.panel) {
            if let Props::Panel(mut panel) = props {
                panel.bg = visual.background;
                ui.set_props(self.panel, Props::Panel(panel))?;
            }
        }
        let caret_w = if focused {
            ctx.metrics.stroke(StrokeToken::Thin).max(1)
        } else {
            0
        };
        if let Some(props) = retained_props(ui, self.caret) {
            if let Props::Rect(mut rect) = props {
                rect.w = Some(caret_w);
                ui.set_props(self.caret, Props::Rect(rect))?;
            }
        }
        ui.layout();
        Ok(())
    }

    pub fn scroll_offset(&self) -> i32 {
        self.scroll_y
    }

    pub fn owns(&self, node: NodeId) -> bool {
        node == self.panel
            || node == self.port
            || node == self.sel_head
            || node == self.sel_band
            || node == self.sel_tail
            || node == self.value_text
            || node == self.caret
    }

    /// 将 app 持有的多行状态同步到组合树。selection/caret 的视觉几何从
    /// 同一 wrap plan 派生；这里不保存文本副本。
    pub fn sync<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, state: &TextInputState,
    ) -> Result<(), UiError> {
        let value = state.value().as_bytes();
        let font = ctx.metrics.typography(TypographyToken::Body);
        let scale = crate::desc::scale_to_u8(font);
        let line_h = font.cell_h() as u32;
        ui.set_text(self.value_text, value)?;
        if let Some(mut semantic) = ui.sem_of(self.panel) {
            semantic.value = TextBuf::new(value)?;
            let (start_cp, end_cp) = state.selection().ordered();
            semantic.text_range.anchor_cp = start_cp;
            semantic.text_range.caret_cp = end_cp;
            ui.set_sem(semantic)?;
        }
        ui.layout();

        let port = ui.bounds(self.port)?;
        let inner_w = self.width.saturating_sub(ctx.metrics.spacing(SpacingToken::Xs).saturating_mul(2));
        let max_w = ui
            .text_wrap_iter()
            .find(|entry| entry.node == self.value_text && entry.enabled)
            .map(|entry| entry.last_max_w)
            .filter(|width| *width != u32::MAX)
            .unwrap_or(inner_w);
        let tm = ui.text_metrics();
        let planned = text_wrap::plan(value, scale, max_w, tm);

        let caret_byte = cp_to_byte(value, state.caret_cp());
        let caret_line = caret_line(&planned, value, caret_byte);
        let caret_line_start = if caret_line < planned.len() {
            planned.start(caret_line)
        } else {
            value.len()
        };
        let caret_x = if caret_line < planned.len() {
            tm.measure(&value[caret_line_start..caret_byte], scale).0
        } else {
            0
        } as i32;
        let caret_y = caret_line as i32 * line_h as i32;
        self.apply_selection(
            ui, tm, value, scale, &planned, state, line_h, inner_w,
        )?;

        let caret_w = if self.focused {
            ctx.metrics.stroke(StrokeToken::Thin).max(1)
        } else {
            0
        };
        set_rect(ui, self.caret, Some(caret_w), Some(line_h))?;
        ui.layout();
        let content_h = text_wrap::measure(value, scale, &planned, tm)
            .1
            .max((caret_line as u32).saturating_add(1).saturating_mul(line_h));
        let port_h = port.h.max(0) as u32;
        let max_scroll = content_h.saturating_sub(port_h) as i32;
        if caret_y < self.scroll_y {
            self.scroll_y = caret_y;
        }
        if caret_y + line_h as i32 > self.scroll_y + port_h as i32 {
            self.scroll_y = caret_y + line_h as i32 - port_h as i32;
        }
        self.scroll_y = self.scroll_y.clamp(0, max_scroll.max(0));

        ui.set_offset(self.value_text, 0, -self.scroll_y)?;
        ui.set_offset(
            self.sel_head,
            self.selection_pos[0].0,
            self.selection_pos[0].1 - self.scroll_y,
        )?;
        ui.set_offset(
            self.sel_band,
            self.selection_pos[1].0,
            self.selection_pos[1].1 - self.scroll_y,
        )?;
        ui.set_offset(
            self.sel_tail,
            self.selection_pos[2].0,
            self.selection_pos[2].1 - self.scroll_y,
        )?;
        ui.set_offset(self.caret, caret_x, caret_y - self.scroll_y)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_selection<const N: usize>(
        &mut self, ui: &mut Ui<N>, tm: &dyn TextMetrics, value: &[u8], scale: u8,
        planned: &WrapPlan, state: &TextInputState, line_h: u32, inner_w: u32,
    ) -> Result<(), UiError> {
        let (start_cp, end_cp) = state.selection().ordered();
        let start = cp_to_byte(value, start_cp);
        let end = cp_to_byte(value, end_cp);
        let mut first: Option<(usize, u32, u32)> = None;
        let mut last: Option<(usize, u32, u32)> = None;
        for line in 0..planned.len() {
            let line_start = planned.start(line);
            let line_end = planned.end(line).max(line_start);
            let visible_start = start.max(line_start);
            let visible_end = end.min(line_end);
            if visible_end <= visible_start {
                continue;
            }
            let x = tm.measure(&value[line_start..visible_start], scale).0;
            let w = tm.measure(&value[visible_start..visible_end], scale).0;
            let item = (line, x, w);
            if first.is_none() {
                first = Some(item);
            }
            last = Some(item);
        }

        let (head, band, tail) = match (first, last) {
            (Some((first_line, x, w)), Some((last_line, _, _))) => {
                let head = (x as i32, first_line as i32 * line_h as i32, w as i32, line_h as i32);
                let tail = if last_line == first_line {
                    (0i32, 0i32, 0i32, 0i32)
                } else {
                    let (_, tail_x, tail_w) = last.expect("last is Some");
                    (tail_x as i32, last_line as i32 * line_h as i32, tail_w as i32, line_h as i32)
                };
                let band = if last_line > first_line + 1 {
                    (
                        0i32,
                        (first_line as i32 + 1) * line_h as i32,
                        inner_w as i32,
                        (last_line - first_line - 1) as i32 * line_h as i32,
                    )
                } else {
                    (0, 0, 0, 0)
                };
                (head, band, tail)
            }
            _ => ((0, 0, 0, 0), (0, 0, 0, 0), (0, 0, 0, 0)),
        };
        let shapes = [head, band, tail];
        self.selection_pos =
            [(head.0, head.1), (band.0, band.1), (tail.0, tail.1)];
        for (node, (x, y, w, h)) in [
            (self.sel_head, shapes[0]),
            (self.sel_band, shapes[1]),
            (self.sel_tail, shapes[2]),
        ] {
            set_rect(ui, node, Some(w.max(0) as u32), Some(h.max(0) as u32))?;
            ui.set_offset(node, x, y)?;
        }
        Ok(())
    }

    /// v1 只消费 FocusIn/Out 视觉；Click 请求焦点由 app 显式完成，与
    /// TextField 契约一致。
    pub fn on_msg<const N: usize>(
        &mut self, ui: &mut Ui<N>, ctx: &WidgetContext, msg: &Msg,
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

fn set_rect<const N: usize>(
    ui: &mut Ui<N>, node: NodeId, w: Option<u32>, h: Option<u32>,
) -> Result<(), UiError> {
    if let Some(props) = retained_props(ui, node) {
        if let Props::Rect(mut rect) = props {
            rect.w = w;
            rect.h = h;
            ui.set_props(node, Props::Rect(rect))?;
        }
    }
    Ok(())
}

fn cp_to_byte(value: &[u8], cp_target: u16) -> usize {
    let target = cp_target as usize;
    let mut cp = 0usize;
    let mut i = 0usize;
    while i < value.len() && cp < target {
        let (_, step) = utf8::next(&value[i..]).unwrap_or((utf8::REPLACEMENT, 1));
        i += step.max(1);
        cp += 1;
    }
    i
}

fn caret_line(plan: &WrapPlan, value: &[u8], byte: usize) -> usize {
    if value.is_empty() {
        return 0;
    }
    if byte >= value.len() && value.last() == Some(&b'\n') {
        return plan.len();
    }
    let mut line = 0usize;
    while line + 1 < plan.len() && byte >= plan.start(line + 1) {
        line += 1;
    }
    line
}
