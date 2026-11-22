//! NavStack 纯状态机（P1）。
//!
//! BerryUI 既裁决“路由/返回栈归 app 状态机，不入 UI 库”。本模块只提供
//! 无 NodeId、无 Ui、无回调的 Plain Data 路由栈，用来统一 push/pop/replace
//! 的错误面与事件形状；页面树重建仍由 app 负责。

use crate::UiError;

pub const NAV_STACK_MAX: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavEvent {
    None,
    Pushed { route: u16, depth: usize },
    Popped { route: u16, depth: usize },
    Replaced { route: u16, depth: usize },
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavStack {
    routes: [u16; NAV_STACK_MAX],
    len: usize,
}

impl NavStack {
    pub fn new(root: u16) -> Result<Self, UiError> {
        if root == 0 {
            return Err(UiError::NodeBad);
        }
        let mut routes = [0; NAV_STACK_MAX];
        routes[0] = root;
        Ok(Self { routes, len: 1 })
    }

    pub fn depth(&self) -> usize {
        self.len
    }

    pub fn top(&self) -> u16 {
        self.routes[self.len - 1]
    }

    pub fn route_at(&self, depth: usize) -> Option<u16> {
        if depth < self.len {
            Some(self.routes[depth])
        } else {
            None
        }
    }

    pub fn can_pop(&self) -> bool {
        self.len > 1
    }

    pub fn push(&mut self, route: u16) -> Result<NavEvent, UiError> {
        if route == 0 {
            return Err(UiError::NodeBad);
        }
        if self.len >= NAV_STACK_MAX {
            return Err(UiError::NodeFull);
        }
        self.routes[self.len] = route;
        self.len += 1;
        Ok(NavEvent::Pushed { route, depth: self.len - 1 })
    }

    pub fn pop(&mut self) -> NavEvent {
        if self.len == 1 {
            return NavEvent::None;
        }
        let route = self.routes[self.len - 1];
        self.len -= 1;
        NavEvent::Popped { route, depth: self.len - 1 }
    }

    pub fn replace(&mut self, route: u16) -> Result<NavEvent, UiError> {
        if route == 0 {
            return Err(UiError::NodeBad);
        }
        self.routes[self.len - 1] = route;
        Ok(NavEvent::Replaced { route, depth: self.len - 1 })
    }

    pub fn pop_to_root(&mut self) -> NavEvent {
        if self.len == 1 {
            return NavEvent::None;
        }
        self.len = 1;
        NavEvent::Reset
    }
}
