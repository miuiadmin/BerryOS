//! berry-ui 模式件（波2 控件底座——六波方案步骤 3「模式件
//! user/berry-ui/src/pattern.rs（TopBar 56vp/BottomTabs 48vp/ListItem
//! 48vp 五态/Nav 页栈/AppBar）」）。
//!
//! 模式件 = 组合模式的**值面单源**：不新建 ViewKind、不动 paint——
//! 用既有容器/组件/Rect 组合出的标准结构，几何常量（vp → px @3x）与
//! 组合步骤固化在本模块，应用（波4 七 appstub 翻新）零手拼。
//!
//! - **TopBar 56vp**：返回位 + 标题 + 动作位的三段横条（169px @3x）。
//! - **BottomTabs 48vp**：底部页签条（144px @3x——Tabs 件底座之上的
//!   结构位；v1 直接消费 Tabs 件，本模块供几何与文案排布常量）。
//! - **ListItem 48vp 五态**：48vp 标准行（normal/pressed/selected/
//!   disabled/focus——v1 无 alpha/焦点环原语，五态经色 token 预混 +
//!   press_map 换底表达；色值单源 theme.rs）。
//! - **Nav 页栈**：NavStack 定容页栈（push/pop/peel 值面——应用内页
//!   面层级的管理件，无子树移动（remove/add 由消费面执行），本模块
//!   管「页序与返回裁决」纯数据）。
//! - **AppBar**：TopBar 的应用版（标题 + 主动作 Button 组合）。
//!
//! 纪律：零 alloc、零 panic（定容数组满 → loud Err）；文案 ASCII（本
//! 轮 UI 文案限 ASCII 硬约束）。

use crate::UiError;

/// OH phone 档密度（1080×1920 = 360×640vp @3x——几何单源）。
pub const DENSITY: u32 = 3;

/// TopBar 高（56vp → 168px）。
pub const TOPBAR_H: u32 = 56 * DENSITY;

/// BottomTabs 高（48vp → 144px）。
pub const BOTTOMTABS_H: u32 = 48 * DENSITY;

/// ListItem 高（48vp → 144px）。
pub const LISTITEM_H: u32 = 48 * DENSITY;

/// ListItem 圆角（8vp → 24px——卡片同律）。
pub const LISTITEM_RADIUS: u32 = 8 * DENSITY;

/// ListItem 水平留白（16vp → 48px）。
pub const LISTITEM_PAD_X: u32 = 16 * DENSITY;

/// Nav 页栈容量（应用内页深帽——D29 树深帽 96 之内）。
pub const NAV_DEPTH_MAX: usize = 8;

/// ListItem 五态底色（v1 预混表：与白底组合后的最终色直接入表——
/// theme.rs token 消费面）。normal = 组件面白；pressed = brand 20%
/// 预混白（OH 0.95 按压 + 遮罩观感近似）；selected = brand 10% 预混；
/// disabled = 灰；focus = normal + 焦点环语义挂 2d 批（焦点环原语缺，
/// 值面先占位同 normal）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    Normal,
    Pressed,
    Selected,
    Disabled,
    Focus,
}

impl ItemState {
    /// 五态底色（RGB 单源表——消费面直接 fill）。
    pub fn bg(self) -> u32 {
        match self {
            ItemState::Normal | ItemState::Focus => 0xFF_FF_FF,
            ItemState::Pressed => 0xCC_E3_FF, // brand 20% 预混白
            ItemState::Selected => 0xE5_F1_FF, // brand 10% 预混白
            ItemState::Disabled => 0xF2_F2_F2,
        }
    }
}

/// Nav 页栈条目（页 key + 可选标题——纯值面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavPage {
    /// 页标识（消费面 NodeId 映射的 key）。
    pub key: u32,
    /// 页标题（ASCII）。
    pub title: [u8; 16],
    pub title_len: u8,
}

/// 定容 Nav 页栈（push/pop 纯数据面——子树操作归消费面）。
#[derive(Debug, Clone, Copy)]
pub struct NavStack {
    pages: [Option<NavPage>; NAV_DEPTH_MAX],
    n: usize,
}

impl NavStack {
    /// 空栈。
    pub fn new() -> Self {
        NavStack { pages: [None; NAV_DEPTH_MAX], n: 0 }
    }

    /// 压页（满 → NodeFull loud 同族）。
    pub fn push(&mut self, page: NavPage) -> Result<(), UiError> {
        if self.n >= NAV_DEPTH_MAX {
            return Err(UiError::NodeFull);
        }
        self.pages[self.n] = Some(page);
        self.n += 1;
        Ok(())
    }

    /// 弹页（空栈 → NodeBad loud——根页退出走消费面「优雅退出」面，
    /// 本件不越权）。
    pub fn pop(&mut self) -> Result<NavPage, UiError> {
        if self.n == 0 {
            return Err(UiError::NodeBad);
        }
        self.n -= 1;
        let p = self.pages[self.n].take().unwrap_or(NavPage {
            key: 0,
            title: [0; 16],
            title_len: 0,
        });
        Ok(p)
    }

    /// 栈深。
    pub fn len(&self) -> usize {
        self.n
    }

    /// 空谓词。
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// 顶页（无页 None）。
    pub fn top(&self) -> Option<NavPage> {
        if self.n == 0 {
            return None;
        }
        self.pages[self.n - 1]
    }
}

impl Default for NavStack {
    fn default() -> Self {
        Self::new()
    }
}

// ---------- 模式件单元测试 ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn page(key: u32) -> NavPage {
        NavPage { key, title: [b'P'; 16], title_len: 1 }
    }

    /// 几何常量 @3x 值级（vp→px 单源表完整性门）。
    #[test]
    fn geometry_constants_at_3x() {
        assert_eq!(TOPBAR_H, 168);
        assert_eq!(BOTTOMTABS_H, 144);
        assert_eq!(LISTITEM_H, 144);
        assert_eq!(LISTITEM_RADIUS, 24);
        assert_eq!(LISTITEM_PAD_X, 48);
    }

    /// 五态底色互异（normal/pressed/selected/disabled 可区分）。
    #[test]
    fn item_states_distinct() {
        let n = ItemState::Normal.bg();
        assert_ne!(n, ItemState::Pressed.bg());
        assert_ne!(n, ItemState::Selected.bg());
        assert_ne!(n, ItemState::Disabled.bg());
        assert_eq!(ItemState::Focus.bg(), n, "focus 环挂 2d 批——值面先同 normal");
    }

    /// Nav 栈 push/pop/top/full/empty 全行为。
    #[test]
    fn nav_stack_push_pop_full_loud() {
        let mut s = NavStack::new();
        assert!(s.is_empty());
        s.push(page(1)).unwrap();
        s.push(page(2)).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s.top().unwrap().key, 2);
        let p = s.pop().unwrap();
        assert_eq!(p.key, 2);
        assert_eq!(s.top().unwrap().key, 1);
        // 满帽 loud。
        let mut f = NavStack::new();
        for k in 0..NAV_DEPTH_MAX {
            f.push(page(k as u32)).unwrap();
        }
        assert_eq!(f.push(page(99)), Err(UiError::NodeFull));
        // 掏空后 pop → NodeBad loud。
        let mut e = NavStack::new();
        assert_eq!(e.pop(), Err(UiError::NodeBad));
    }
}
