//! berry-governanced——G5 刀 19：专注模式治理状态机。
//!
//! 四专注模式：Work/Personal/Sleep/Drive——状态机切换+通知白名单
//! 动态过滤+桌面页面联动+勿扰自动策略。
//!
//! 纯面纪律：状态转移表/白名单匹配/策略决策 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std)]

/// 专注模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    Personal, // 默认个人域
    Work,     // 工作域——仅白名单应用+通知
    Sleep,    // 睡眠——全部静默+紧急例外
    Drive,    // 驾驶——仅导航+通话
}

/// 通知分级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationPriority {
    Critical, // 紧急（来电/闹钟/SOS）
    High,     // 高（即时消息）
    Normal,   // 普通
    Low,      // 低（推广/汇总）
}

/// 专注模式策略。
pub struct GovernancePolicy {
    pub mode: FocusMode,
    /// 白名单应用 ID 列表（定容 8）。
    pub allowed_apps: [u32; 8],
    pub allowed_count: usize,
    /// DND（勿扰）是否激活。
    pub dnd_active: bool,
}

impl GovernancePolicy {
    pub fn personal() -> GovernancePolicy {
        GovernancePolicy { mode: FocusMode::Personal, allowed_apps: [0; 8], allowed_count: 0, dnd_active: false }
    }

    /// 切换专注模式——设定对应白名单与 DND。
    pub fn enter(&mut self, mode: FocusMode) {
        self.mode = mode;
        self.allowed_count = 0;
        self.dnd_active = false;
        match mode {
            FocusMode::Personal => { /* 全通——无白名单 */ }
            FocusMode::Work => {
                self.allowed_apps = [100, 101, 102, 0, 0, 0, 0, 0]; // 邮件/日历/IM
                self.allowed_count = 3;
            }
            FocusMode::Sleep => {
                self.dnd_active = true;
                self.allowed_apps = [200, 0, 0, 0, 0, 0, 0, 0]; // 闹钟
                self.allowed_count = 1;
            }
            FocusMode::Drive => {
                self.allowed_apps = [300, 301, 0, 0, 0, 0, 0, 0]; // 导航/通话
                self.allowed_count = 2;
            }
        }
    }

    /// 通知是否放行——按模式+优先级+白名单综合判定。
    pub fn allow_notification(&self, app_id: u32, priority: NotificationPriority) -> bool {
        match self.mode {
            FocusMode::Personal => !self.dnd_active || priority == NotificationPriority::Critical,
            FocusMode::Work => {
                self.allowed_apps[..self.allowed_count].contains(&app_id)
                    || priority == NotificationPriority::Critical
            }
            FocusMode::Sleep => priority == NotificationPriority::Critical,
            FocusMode::Drive => {
                self.allowed_apps[..self.allowed_count].contains(&app_id)
                    && priority != NotificationPriority::Low
            }
        }
    }

    /// 桌面页面索引联动——各模式对应不同桌面页。
    pub fn desktop_page(&self) -> u8 {
        match self.mode {
            FocusMode::Personal => 0,
            FocusMode::Work => 1,
            FocusMode::Sleep => 2,
            FocusMode::Drive => 3,
        }
    }

    /// 合法模式转移表。
    pub fn can_transition(to: FocusMode) -> bool {
        // 全连通——任意模式间可切（Sleep→Drive 由系统自动触发场景）。
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_mode_allows_all() {
        let g = GovernancePolicy::personal();
        assert!(g.allow_notification(999, NotificationPriority::Low));
        assert_eq!(g.desktop_page(), 0);
    }

    #[test]
    fn work_mode_whitelist() {
        let mut g = GovernancePolicy::personal();
        g.enter(FocusMode::Work);
        assert!(g.allow_notification(100, NotificationPriority::Normal)); // 白名单内
        assert!(!g.allow_notification(999, NotificationPriority::Normal)); // 白名单外
        assert!(g.allow_notification(999, NotificationPriority::Critical)); // 紧急恒通
        assert_eq!(g.desktop_page(), 1);
    }

    #[test]
    fn sleep_mode_dnd() {
        let mut g = GovernancePolicy::personal();
        g.enter(FocusMode::Sleep);
        assert!(g.dnd_active);
        assert!(!g.allow_notification(999, NotificationPriority::High));
        assert!(g.allow_notification(999, NotificationPriority::Critical)); // 紧急穿透
        assert_eq!(g.desktop_page(), 2);
    }

    #[test]
    fn drive_mode_navigation_only() {
        let mut g = GovernancePolicy::personal();
        g.enter(FocusMode::Drive);
        assert!(g.allow_notification(300, NotificationPriority::High)); // 导航
        assert!(!g.allow_notification(300, NotificationPriority::Low)); // 低优先级导航仍拒
        assert!(!g.allow_notification(999, NotificationPriority::Normal));
        assert_eq!(g.desktop_page(), 3);
    }
}
