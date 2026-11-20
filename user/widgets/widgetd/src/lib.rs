//! berry-widgetd——G3 刀 12：小组件生命周期与 Timeline Provider。
//!
//! 三面：
//! 1. **网格排布**：2×2 / 4×2 小组件在桌面网格中的位置管理。
//! 2. **Timeline Provider**：每个小组件的数据过期时间线——下次刷新
//!    预测与到期排序。
//! 3. **Smart Stack**：同格叠放的小组件轮换——按时序/相关性选当前
//!    显示件。
//!
//! 纯面纪律：排布算法/时间线排序/栈轮换 = 零 I/O 纯函数（host 可测）。
#![cfg_attr(not(test), no_std)]

#[cfg(not(test))]
extern crate alloc;
#[cfg(not(test))]
use alloc::vec::Vec;

/// 小组件尺寸（格数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetSize {
    Small,  // 2×2
    Medium, // 4×2
}

impl WidgetSize {
    pub fn grid_cells(&self) -> (u16, u16) {
        match self {
            WidgetSize::Small => (2, 2),
            WidgetSize::Medium => (4, 2),
        }
    }
}

/// 小组件条目（排布面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetEntry {
    pub id: u32,
    pub size: WidgetSize,
    /// 网格列（0 基）。
    pub col: u16,
    /// 网格行（0 基）。
    pub row: u16,
}

/// 桌面网格——固定 4 列（2 行帽=8 槽位标准桌面）。
pub struct WidgetGrid {
    pub cols: u16,
    pub rows: u16,
    widgets: [Option<WidgetEntry>; 16],
    count: usize,
}

impl WidgetGrid {
    pub fn new() -> WidgetGrid {
        WidgetGrid {
            cols: 4,
            rows: 4,
            widgets: [None; 16],
            count: 0,
        }
    }

    /// 尝试放置小组件——碰撞检测（矩形重叠）→ 放置或 None。
    pub fn place(&mut self, id: u32, size: WidgetSize, col: u16, row: u16) -> bool {
        let (w, h) = size.grid_cells();
        if col + w > self.cols || row + h > self.rows {
            return false;
        }
        // 碰撞检测
        for slot in self.widgets.iter().flatten() {
            let (sw, sh) = slot.size.grid_cells();
            if col < slot.col + sw
                && col + w > slot.col
                && row < slot.row + sh
                && row + h > slot.row
            {
                return false;
            }
        }
        // 找空槽位
        for slot in self.widgets.iter_mut() {
            if slot.is_none() {
                *slot = Some(WidgetEntry { id, size, col, row });
                self.count += 1;
                return true;
            }
        }
        false
    }

    /// 自动放置——从左上角扫描首个可用位置。
    pub fn place_auto(&mut self, id: u32, size: WidgetSize) -> Option<(u16, u16)> {
        let (w, h) = size.grid_cells();
        for row in 0..self.rows {
            for col in 0..self.cols {
                if self.place(id, size, col, row) {
                    return Some((col, row));
                }
            }
        }
        let _ = (w, h); // w/h 已在 place 内部消费
        None
    }

    /// 移除小组件。
    pub fn remove(&mut self, id: u32) -> bool {
        for slot in self.widgets.iter_mut() {
            if let Some(w) = slot {
                if w.id == id {
                    *slot = None;
                    self.count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

/// Timeline 条目——小组件数据过期预测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineEntry {
    pub widget_id: u32,
    /// 数据时间戳（ns——berry_clock_read 单调钟）。
    pub data_ts: u64,
    /// 预计有效期（ns——数据到此刻后过期）。
    pub ttl: u64,
}

impl TimelineEntry {
    /// 过期时刻。
    pub fn expiry(&self) -> u64 {
        self.data_ts + self.ttl
    }

    /// 是否已过期（相对当前时刻 now）。
    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expiry()
    }

    /// 距过期剩余（ns；已过期返 0）。
    pub fn remaining(&self, now: u64) -> u64 {
        self.expiry().saturating_sub(now)
    }
}

/// Timeline 调度器——管理多组件的过期时间线。
pub struct TimelineScheduler {
    entries: [TimelineEntry; 16],
    count: usize,
}

impl TimelineScheduler {
    pub fn new() -> TimelineScheduler {
        TimelineScheduler { entries: [TimelineEntry { widget_id: 0, data_ts: 0, ttl: 0 }; 16], count: 0 }
    }

    /// 注册/更新小组件的时间线。
    pub fn upsert(&mut self, entry: TimelineEntry) -> bool {
        // 已有则更新
        for e in self.entries[..self.count].iter_mut() {
            if e.widget_id == entry.widget_id {
                *e = entry;
                return true;
            }
        }
        // 新增
        if self.count < self.entries.len() {
            self.entries[self.count] = entry;
            self.count += 1;
            true
        } else {
            false
        }
    }

    /// 最早过期的 N 个组件（升序按 expiry）——下一次刷新目标。
    /// 返回 (widget_id, expiry) 列表。
    pub fn next_expiring(&self, now: u64, n: usize) -> Vec<(u32, u64)> {
        let mut items: Vec<(u32, u64, u64)> = self.entries[..self.count]
            .iter()
            .filter(|e| !e.is_expired(now))
            .map(|e| (e.widget_id, e.expiry(), e.remaining(now)))
            .collect();
        // 按 expiry 升序
        items.sort_by(|a, b| a.1.cmp(&b.1));
        items.into_iter().take(n).map(|(id, exp, _)| (id, exp)).collect()
    }

    /// 当前已过期的组件列表——需立即刷新。
    pub fn expired(&self, now: u64) -> Vec<u32> {
        self.entries[..self.count]
            .iter()
            .filter(|e| e.is_expired(now))
            .map(|e| e.widget_id)
            .collect()
    }
}

/// Smart Stack——同格叠放的组件轮换。
pub struct SmartStack {
    pub stack_id: u32,
    /// 栈内组件 ID（显示顺序——index 0 = 当前显示）。
    pub members: [u32; 4],
    pub count: usize,
    /// 轮换间隔（ns）。
    pub rotate_interval: u64,
}

impl SmartStack {
    pub fn new(stack_id: u32, rotate_interval: u64) -> SmartStack {
        SmartStack { stack_id, members: [0; 4], count: 0, rotate_interval }
    }

    /// 添加组件到栈——返 false 栈满。
    pub fn push(&mut self, widget_id: u32) -> bool {
        if self.count >= self.members.len() {
            return false;
        }
        self.members[self.count] = widget_id;
        self.count += 1;
        true
    }

    /// 轮换到下一个——返回新当前的组件 ID。
    pub fn rotate(&mut self) -> u32 {
        if self.count == 0 {
            return 0;
        }
        let head = self.members[0];
        // 左移一格
        for i in 0..self.count - 1 {
            self.members[i] = self.members[i + 1];
        }
        self.members[self.count - 1] = head;
        self.members[0]
    }

    /// 当前显示的组件。
    pub fn current(&self) -> u32 {
        if self.count == 0 { 0 } else { self.members[0] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_place_and_collide() {
        let mut g = WidgetGrid::new();
        assert!(g.place(1, WidgetSize::Small, 0, 0));
        assert!(g.place(2, WidgetSize::Small, 2, 0));
        // 碰撞——(0,0) 已占
        assert!(!g.place(3, WidgetSize::Small, 0, 0));
        // 越界——4 列网格 col=3 w=4 越右
        assert!(!g.place(4, WidgetSize::Medium, 3, 0));
        assert_eq!(g.count(), 2);
    }

    #[test]
    fn grid_auto_place() {
        let mut g = WidgetGrid::new();
        let pos1 = g.place_auto(1, WidgetSize::Small).unwrap();
        let pos2 = g.place_auto(2, WidgetSize::Small).unwrap();
        assert_eq!(pos1, (0, 0));
        assert_eq!(pos2, (2, 0)); // 紧邻右侧
        let pos3 = g.place_auto(3, WidgetSize::Medium).unwrap();
        assert_eq!(pos3, (0, 2)); // 第三行（前两行被 2×Small 占）
    }

    #[test]
    fn grid_remove() {
        let mut g = WidgetGrid::new();
        g.place(1, WidgetSize::Small, 0, 0);
        assert!(g.remove(1));
        assert_eq!(g.count(), 0);
        assert!(!g.remove(99));
    }

    #[test]
    fn timeline_expiry() {
        let t = TimelineEntry { widget_id: 42, data_ts: 1000, ttl: 500 };
        assert!(!t.is_expired(1400));
        assert!(t.is_expired(1500));
        assert_eq!(t.remaining(1400), 100);
        assert_eq!(t.remaining(1500), 0);
    }

    #[test]
    fn scheduler_next_expiring() {
        let mut s = TimelineScheduler::new();
        s.upsert(TimelineEntry { widget_id: 1, data_ts: 0, ttl: 1000 });
        s.upsert(TimelineEntry { widget_id: 2, data_ts: 0, ttl: 500 });
        s.upsert(TimelineEntry { widget_id: 3, data_ts: 0, ttl: 2000 });
        let next = s.next_expiring(0, 2);
        assert_eq!(next.len(), 2);
        assert_eq!(next[0].0, 2); // ttl 500 最先过期
        assert_eq!(next[1].0, 1); // ttl 1000 次之
    }

    #[test]
    fn scheduler_expired_list() {
        let mut s = TimelineScheduler::new();
        s.upsert(TimelineEntry { widget_id: 1, data_ts: 0, ttl: 100 });
        s.upsert(TimelineEntry { widget_id: 2, data_ts: 0, ttl: 5000 });
        let exp = s.expired(200);
        assert_eq!(exp, vec![1]);
        assert_eq!(s.expired(0), Vec::<u32>::new());
    }

    #[test]
    fn smart_stack_rotate() {
        let mut st = SmartStack::new(0, 5_000_000_000);
        st.push(10);
        st.push(20);
        st.push(30);
        assert_eq!(st.current(), 10);
        assert_eq!(st.rotate(), 20);
        assert_eq!(st.current(), 20);
        assert_eq!(st.rotate(), 30);
        assert_eq!(st.rotate(), 10); // 回环
    }
}
