//! berry-intentd——G3 刀 14：意图路由与快捷指令自动化执行器。
//!
//! 架构：
//! - **Intent 模型**：action + category + data URI + extras。
//! - **应用清单**：app.manifest.json 中的 intent-filter 注册。
//! - **路由决策**：action+category 匹配 → 候选集 → 优先级/最近使用排序。
//! - **快捷指令**：意图链（Intent Chain）——多步自动化的原子单元。
//!
//! 纯面纪律：路由匹配/优先级排序/意图链求值 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std)]

#[cfg(not(test))]
extern crate alloc;
#[cfg(not(test))]
use alloc::vec::Vec;

/// Intent Action——标准化动词（Android Intent 同族语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    View,        // android.intent.action.VIEW
    Send,        // ACTION_SEND
    Edit,        // ACTION_EDIT
    Pick,        // ACTION_PICK
    Search,      // ACTION_SEARCH
    SetWallpaper,
    PlayMedia,
    OpenUrl,
    Custom(u16), // 应用自定义 action
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::View => "VIEW",
            Action::Send => "SEND",
            Action::Edit => "EDIT",
            Action::Pick => "PICK",
            Action::Search => "SEARCH",
            Action::SetWallpaper => "SET_WALLPAPER",
            Action::PlayMedia => "PLAY_MEDIA",
            Action::OpenUrl => "OPEN_URL",
            Action::Custom(_) => "CUSTOM",
        }
    }
}

/// Intent Category——分类标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Default,
    Browsable,
    Launcher,
    Share,
}

/// Intent——应用间通信的原子单元。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intent {
    pub action: Action,
    pub category: Category,
    /// URI 数据（hash 简化——真面为字符串）。
    pub data_hash: u64,
}

/// 应用注册的 intent-filter。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntentFilter {
    pub app_id: u32,
    pub action: Action,
    pub category: Category,
    /// 数据 URI 模式 hash（0 = 通配）。
    pub data_pattern: u64,
    /// 优先级（0-1000，高优先）。
    pub priority: u16,
}

/// 路由决策——从候选集中选出目标应用。
pub struct IntentRouter {
    filters: [IntentFilter; 32],
    count: usize,
}

impl IntentRouter {
    pub fn new() -> IntentRouter {
        IntentRouter { filters: [IntentFilter { app_id: 0, action: Action::View, category: Category::Default, data_pattern: 0, priority: 0 }; 32], count: 0 }
    }

    /// 注册 intent-filter——同 (app_id, action, category) 覆盖。
    pub fn register(&mut self, filter: IntentFilter) -> bool {
        // 已有同 (app_id, action) 则更新
        for f in self.filters[..self.count].iter_mut() {
            if f.app_id == filter.app_id && f.action == filter.action {
                *f = filter;
                return true;
            }
        }
        if self.count < self.filters.len() {
            self.filters[self.count] = filter;
            self.count += 1;
            true
        } else {
            false
        }
    }

    /// 路由——返回匹配的 app_id（优先级最高者）；无匹配 → None。
    pub fn route(&self, intent: &Intent) -> Option<u32> {
        self.filters[..self.count]
            .iter()
            .filter(|f| {
                f.action == intent.action
                    && f.category == intent.category
                    && (f.data_pattern == 0 || f.data_pattern == intent.data_hash)
            })
            .max_by_key(|f| f.priority)
            .map(|f| f.app_id)
    }

    /// 列出所有能处理此 intent 的应用（候选集）。
    pub fn candidates(&self, intent: &Intent) -> Vec<u32> {
        self.filters[..self.count]
            .iter()
            .filter(|f| {
                f.action == intent.action
                    && (f.data_pattern == 0 || f.data_pattern == intent.data_hash)
            })
            .map(|f| f.app_id)
            .collect()
    }
}

/// 意图链——快捷指令的多步自动化。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntentStep {
    pub intent: Intent,
    /// 上一步成功后才执行（false = 无条件执行）。
    pub require_success: bool,
    /// 延迟（ms——执行间等待）。
    pub delay_ms: u32,
}

/// 意图链执行器——逐步求值。
pub struct IntentChain {
    pub name: &'static str,
    pub steps: [IntentStep; 8],
    pub count: usize,
    /// 当前执行到第几步。
    pub cursor: usize,
}

impl IntentChain {
    pub fn new(name: &'static str) -> IntentChain {
        IntentChain { name, steps: [IntentStep { intent: Intent { action: Action::View, category: Category::Default, data_hash: 0 }, require_success: false, delay_ms: 0 }; 8], count: 0, cursor: 0 }
    }

    /// 添加步骤——返 false 链满。
    pub fn push(&mut self, step: IntentStep) -> bool {
        if self.count >= self.steps.len() {
            return false;
        }
        self.steps[self.count] = step;
        self.count += 1;
        true
    }

    /// 下一步——返回 Some(step)；链尾/因 require_success 中断 → None。
    /// `last_ok`：上一步是否成功（首步忽略）。
    pub fn next(&mut self, last_ok: bool) -> Option<IntentStep> {
        if self.cursor >= self.count {
            return None;
        }
        let step = self.steps[self.cursor];
        if self.cursor > 0 && step.require_success && !last_ok {
            return None; // 依赖失败——链中断
        }
        self.cursor += 1;
        Some(step)
    }

    /// 重置游标（可重跑）。
    pub fn reset(&mut self) {
        self.cursor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_exact_match() {
        let mut r = IntentRouter::new();
        r.register(IntentFilter { app_id: 1, action: Action::View, category: Category::Default, data_pattern: 0, priority: 100 });
        let intent = Intent { action: Action::View, category: Category::Default, data_hash: 42 };
        assert_eq!(r.route(&intent), Some(1));
    }

    #[test]
    fn router_priority() {
        let mut r = IntentRouter::new();
        r.register(IntentFilter { app_id: 1, action: Action::View, category: Category::Default, data_pattern: 0, priority: 50 });
        r.register(IntentFilter { app_id: 2, action: Action::View, category: Category::Default, data_pattern: 0, priority: 100 });
        let intent = Intent { action: Action::View, category: Category::Default, data_hash: 0 };
        assert_eq!(r.route(&intent), Some(2)); // 优先级高者胜
    }

    #[test]
    fn router_data_pattern() {
        let mut r = IntentRouter::new();
        r.register(IntentFilter { app_id: 1, action: Action::View, category: Category::Default, data_pattern: 0xABC, priority: 100 });
        // 匹配——data_hash 同 pattern
        assert_eq!(r.route(&Intent { action: Action::View, category: Category::Default, data_hash: 0xABC }), Some(1));
        // 不匹配——data_hash 异
        assert_eq!(r.route(&Intent { action: Action::View, category: Category::Default, data_hash: 0x123 }), None);
    }

    #[test]
    fn router_no_match() {
        let r = IntentRouter::new();
        assert_eq!(r.route(&Intent { action: Action::Send, category: Category::Share, data_hash: 0 }), None);
    }

    #[test]
    fn chain_sequential() {
        let mut c = IntentChain::new("test");
        c.push(IntentStep { intent: Intent { action: Action::View, category: Category::Default, data_hash: 1 }, require_success: false, delay_ms: 0 });
        c.push(IntentStep { intent: Intent { action: Action::Send, category: Category::Share, data_hash: 2 }, require_success: true, delay_ms: 100 });
        let s1 = c.next(true).unwrap();
        assert_eq!(s1.intent.action, Action::View);
        let s2 = c.next(true).unwrap();
        assert_eq!(s2.intent.action, Action::Send);
        assert!(c.next(true).is_none()); // 链尾
    }

    #[test]
    fn chain_interrupt_on_failure() {
        let mut c = IntentChain::new("abort");
        c.push(IntentStep { intent: Intent { action: Action::View, category: Category::Default, data_hash: 0 }, require_success: false, delay_ms: 0 });
        c.push(IntentStep { intent: Intent { action: Action::Send, category: Category::Default, data_hash: 0 }, require_success: true, delay_ms: 0 });
        c.next(true); // 步 1
        // 步 2 依赖成功但 last_ok=false → 中断
        assert!(c.next(false).is_none());
    }

    #[test]
    fn chain_reset() {
        let mut c = IntentChain::new("reset");
        c.push(IntentStep { intent: Intent { action: Action::View, category: Category::Default, data_hash: 0 }, require_success: false, delay_ms: 0 });
        c.next(true);
        assert!(c.next(true).is_none());
        c.reset();
        assert!(c.next(true).is_some());
    }
}
