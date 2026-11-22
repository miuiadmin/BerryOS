//! LongPress 纯手势 helper。
//!
//! Msg wire 没有时间戳；与 SwipePager 吸附相同，长按起拍锚定在 Press 后
//! 首次 `tick(now_ns)`。app 帧循环负责传入唯一时刻源，本模块不建 timer、
//! 不读 wall-clock、不接触 Ui。

use crate::desc::{Msg, NodeId};

pub const LONG_PRESS_NS: u64 = 500_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongPressOutcome {
    None,
    Fired { node: NodeId, x: u32, y: u32 },
    ClickSuppressed { node: NodeId },
}

#[derive(Debug, Clone, Copy)]
struct Armed {
    node: NodeId,
    x: u32,
    y: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct LongPressGesture {
    duration_ns: u64,
    armed: Option<Armed>,
    start_ns: Option<u64>,
    fired: Option<NodeId>,
}

impl LongPressGesture {
    pub fn new(duration_ns: u64) -> Self {
        assert!(duration_ns > 0, "LongPressGesture duration must be non-zero");
        Self {
            duration_ns,
            armed: None,
            start_ns: None,
            fired: None,
        }
    }

    pub fn pending(&self) -> bool {
        self.armed.is_some()
    }

    pub fn reset(&mut self) {
        self.armed = None;
        self.start_ns = None;
        self.fired = None;
    }

    pub fn on_msg(&mut self, msg: &Msg) -> LongPressOutcome {
        match *msg {
            Msg::Press { node, x, y } => {
                self.armed = Some(Armed { node, x, y });
                self.start_ns = None;
                self.fired = None;
                LongPressOutcome::None
            }
            Msg::DragStart { node, .. } | Msg::DragMove { node, .. } => {
                if self.armed.is_some_and(|armed| armed.node == node) {
                    self.armed = None;
                    self.start_ns = None;
                }
                LongPressOutcome::None
            }
            Msg::Release { node, .. } | Msg::Cancel { node } => {
                if self.fired.is_none() {
                    self.armed = None;
                    self.start_ns = None;
                }
                let _ = node;
                LongPressOutcome::None
            }
            Msg::Click { node } => match self.fired {
                Some(fired) if fired == node => {
                    self.fired = None;
                    LongPressOutcome::ClickSuppressed { node }
                }
                _ => {
                    self.armed = None;
                    self.start_ns = None;
                    LongPressOutcome::None
                }
            },
            _ => LongPressOutcome::None,
        }
    }

    pub fn tick(&mut self, now_ns: u64) -> LongPressOutcome {
        let Some(armed) = self.armed else {
            return LongPressOutcome::None;
        };
        let start = *self.start_ns.get_or_insert(now_ns);
        if now_ns.saturating_sub(start) < self.duration_ns {
            return LongPressOutcome::None;
        }
        self.armed = None;
        self.start_ns = None;
        self.fired = Some(armed.node);
        LongPressOutcome::Fired {
            node: armed.node,
            x: armed.x,
            y: armed.y,
        }
    }
}
