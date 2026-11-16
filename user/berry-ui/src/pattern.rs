//! berry-ui 模式件（波2 控件底座——六波方案步骤 3「模式件
//! user/berry-ui/src/pattern.rs（TopBar 56vp/BottomTabs 48vp/ListItem
//! 48vp 五态/Nav 页栈/AppBar）」＋ 波2 移植对账第 4 项扩容）。
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
//! # 波2 移植扩容（RECON-PORT 第 4 项——19 配方族）
//!
//! 移植源 = `../ui-wave2` worktree 的 pattern.rs（刀 3 资产 23 配方，
//! 只读）；主树已有 4 件（TopBar/BottomTabs/ListItem 常量 + NavStack
//! 数据面 + ItemState 五态表）API 形态保持不动，新配方向其看齐。主
//! 树适配三律（沿革与逐件偏差单源 = worktree 根 `PORT-XB.md`）：
//!
//! - **主题面**：源 `Palette` 六槽入参改主树 [`crate::theme::Tokens`]
//!   组引用（`Tokens::pick` 单点取值）；`accent_dim` 派生 = brand 上
//!   页面底 12% 预混（[`dim_accent`] 本面单源——premix 原语归属
//!   theme 面，路 1 落地后收编）。
//! - **五态默认**：源 `FiveState`/`StyleParams` 落表机制在主树未立，
//!   按主树查表 [`ItemState`]（已含 `Disabled` 档 = 扩展后形态）编
//!   程——交互件产物携 `pub state: ItemState`（默认 `Normal`），消
//!   费面 `state.bg()` 直取换底色；XA 的 paint 侧 `set_disabled` 落
//!   地前本面无引擎消费面（数据面先行，无 cfg 门需求）。
//! - **kind 直用**：主树 41 kind 全通电且 `set_props` 全 kind 开口
//!   ——源「0×0 占位锚 + 重挂同步」技巧退役：Card/Collapse/
//!   SwipeItem/Refresh 以 kind 容器本义承载（自绘面底/header 带/
//!   reveal 带），态位翻转走 `set_props`（节点 id 稳定，子树保持）；
//!   值域执法单源 `widget_value_ok`（add 面）——配方不重复先验。
//!
//! 纪律：零 alloc、零 panic（定容数组满 → loud Err）；文案 ASCII（本
//! 轮 UI 文案限 ASCII 硬约束）。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;

use crate::anim::{self, Easing};
use crate::desc::{
    BadgeProps, CardProps, ChipProps, ColProps, CollapseProps, DividerProps, DragReorderProps,
    FabProps, GaugeProps, GaugeStyle, GridProps, IconProps, NodeId, Props, QrProps, RectProps,
    RefreshProps, RowProps, StackAlign, StackProps, SwipeItemProps, TextBuf, TextProps, TextStyle,
    ViewKind, ViewSpec,
};
use crate::layout::CrossAlign;
use crate::UiMain;
use crate::swipe::{FLIP_VEL_PX_S, SNAP_DUR_NS};
use crate::theme::{ON_ACCENT, Tokens};
use crate::{Ui, UiError};

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

// ---------- 波2 扩容：色派生与图标 id 约定 ----------

/// 淡强调底派生（active 页签/分段换底档）：brand 上页面底 12% 预混
/// ——整数千分比零浮点、const 可编译期预算（源 `Palette::from_tokens`
/// 派生律照搬）。值随入参 token——light/dark 两档各得其值（硬预混
/// const 只能钉单档，故取派生式）；premix 原语归属 theme 面，路 1
/// 落地后本面收编单源。
const fn dim_accent(t: Tokens) -> Color {
    const A: u32 = 120;
    Color {
        r: ((t.brand.r as u32 * (1000 - A) + t.bg_page.r as u32 * A) / 1000) as u8,
        g: ((t.brand.g as u32 * (1000 - A) + t.bg_page.g as u32 * A) / 1000) as u8,
        b: ((t.brand.b as u32 * (1000 - A) + t.bg_page.b as u32 * A) / 1000) as u8,
    }
}

/// 模式件引用的图标字形 id 约定域（IconSource 先例：未注册 = 空绘；
/// 配方一律带显式 w/h 故几何不塌）。
pub mod icons {
    /// AppBar 返回键。
    pub const BACK: u16 = 0x0101;
    /// Step 已完成勾。
    pub const CHECK: u16 = 0x0110;
    /// Fab 常用加号。
    pub const ADD: u16 = 0x0120;
    /// Refresh 下拉指示。
    pub const REFRESH: u16 = 0x0121;
    /// 底部页签示例字形族（入参由宿主给 id——此三件仅约定示例值）。
    pub const HOME: u16 = 0x0130;
    pub const GRID: u16 = 0x0131;
    pub const SETTINGS: u16 = 0x0132;
}

// ---------- 波2 扩容：尺寸常数（px @3x；「拟定」= 非 OH 摘录） ----------

/// AppBar 返回键方寸 24vp（拟定）。
pub const APP_BAR_BACK: u32 = 24 * DENSITY;
/// AppBar 返回键前留白 8vp（拟定）。
pub const APP_BAR_LEAD: u32 = 8 * DENSITY;
/// AppBar 返回键 → 标题间距 8vp（拟定）。
pub const APP_BAR_GAP: u32 = 8 * DENSITY;
/// AppBar/TopBar 标题起始内边距 16vp（源 OH_APPBAR_TITLE_INSET 同值）。
pub const APPBAR_TITLE_INSET: u32 = 16 * DENSITY;
/// TopBar 尾随动作位数帽。
pub const TOPBAR_ACTIONS_MAX: usize = 3;
/// TopBar 动作位方寸 24vp（拟定）。
pub const TOPBAR_ACT: u32 = 24 * DENSITY;
/// TopBar 标题 → 动作位间距 8vp（拟定）。
pub const TOPBAR_GAP: u32 = 8 * DENSITY;
/// Seg 分段选择器高 32vp（拟定——名别于 lib Seg kind 的 SEG_H=36px）。
pub const SEG_BAR_H: u32 = 32 * DENSITY;
/// Seg 段缝（拟定）。
pub const SEG_GAP: u32 = 2;
/// Step 步点方寸 16vp（拟定）。
pub const STEP_DOT: u32 = 16 * DENSITY;
/// Step 连接线宽 16vp（拟定）。
pub const STEP_LINE_W: u32 = 16 * DENSITY;
/// Step 连接线厚 2vp（拟定）。
pub const STEP_LINE_H: u32 = 2 * DENSITY;
/// Step 步数帽。
pub const STEP_MAX: usize = 8;
/// 通用图标方寸 24vp（拟定）。
pub const ICON_SZ: u32 = 24 * DENSITY;
/// 页栈过渡时长 300ms（cubicBezier(0.2,0,0.1,1.0) 的现四档最近似——
/// CubicOut 减速形）。
pub const NAV_ANIM_NS: u64 = 300_000_000;
/// SwipeItem 展开面宽 96vp（拟定：两 48vp 动作位）。
pub const SWIPE_REVEAL_W: u32 = 96 * DENSITY;
/// 页签/分段项数帽（对齐 swipe::SWIPE_PAGES_MAX=8 联动域）。
pub const TABS_MAX: usize = 8;
/// DragReorder 窗内小规模帽。
pub const REORDER_MAX: usize = 8;
/// Refresh 触发阈 32vp（拟定）。
pub const REFRESH_THRESH_PX: u32 = 32 * DENSITY;
/// Refresh 指示器方寸 16vp（拟定）。
pub const REFRESH_IND: u32 = 16 * DENSITY;
/// Drawer 开启手势起拖边域 16vp（拟定——闭合态自此域内起拖才开）。
pub const DRAWER_EDGE_PX: u32 = 16 * DENSITY;
/// 分隔线厚 1px。
pub const DIVIDER_H: u32 = 1;

// ---------- mount 小工具（crate 内共享） ----------

/// 单节点 spec 构造（key=0——调和键路径挂账 v1.5）。
pub(crate) fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

/// 文本节点 spec（超长 → `TextTooLong` loud 沿 mount 链上抛）。
pub(crate) fn text_spec(
    fg: Color,
    bg: Option<Color>,
    scale: FontSize,
    bytes: &[u8],
) -> Result<ViewSpec, UiError> {
    Ok(vspec(
        ViewKind::Text,
        Props::Text(TextProps { style: TextStyle { fg, bg, scale }, text: TextBuf::new(bytes)? }),
    ))
}

/// 图标节点 spec（铁则：显式 w/h——空供件下几何不塌）。
pub(crate) fn icon_spec(id: u16, fg: Color, w: u32, h: u32) -> ViewSpec {
    vspec(ViewKind::Icon, Props::Icon(IconProps { id, fg, w: Some(w), h: Some(h) }))
}

/// a11y 登记尽力面（表满 32 / 超长 = 跳过不阻断建树）。
pub(crate) fn a11y<const N: usize>(ui: &mut Ui<N>, node: NodeId, role: u8, label: &[u8]) {
    let _ = ui.set_a11y(node, role, label);
}

// ---------- 结构配方三件（源 kit/layout.rs 移植——kind 主树化） ----------

/// 弹性留白（定尺寸空 Col——无视觉、无子；w/h 定值覆盖内在。主树
/// Spacer kind 无尺寸面（flex 挂后续布局刀）——Col 定尺寸承载）。
pub fn spacer<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    w: u32,
    h: u32,
) -> Result<NodeId, UiError> {
    ui.add(
        parent,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: Some(w), h: Some(h), cross: CrossAlign::Stretch })),
    )
}

/// 横向分隔线：主树 Divider kind 直载（厚 1px、宽随父、色单源 paint
/// 面 OUTLINE——源 color 入参随 kind 化退役）。用于 Col 主轴；Row 内
/// 主轴宽 = 内在 0 会塌，Row 内请用显式 w 的 Rect。
pub fn divider<const N: usize>(ui: &mut Ui<N>, parent: NodeId) -> Result<NodeId, UiError> {
    ui.add(
        parent,
        vspec(ViewKind::Divider, Props::Divider(DividerProps { thickness: DIVIDER_H, inset_start: 0, inset_end: 0 })),
    )
}

/// 竖向分隔线：w=1px、h 随父（主树 v1 无纵线 kind 面——paint 自认
/// 「纵线挂后续刀」，Rect 组合承载）。用于 Row 十字轴——Col 内会塌
/// 同上。
pub fn divider_v<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    color: Color,
) -> Result<NodeId, UiError> {
    ui.add(parent, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: color, w: Some(DIVIDER_H), h: None })))
}

/// 叠放容器（主树 Stack kind 直载——内在 = 子 max×max、子各自
/// (0,0) 起满幅叠放、z 序 = 子序（后子盖上）。源「Pad(0) Σ 内在」技
/// 巧退役：装饰面层（底色 Rect / 组件 kind 锚）一律 `w/h = None`（零
/// 内在贡献），高度由内容层定尺寸撑起）。
pub fn stack<const N: usize>(ui: &mut Ui<N>, parent: NodeId) -> Result<NodeId, UiError> {
    ui.add(parent, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Start })))
}

// ---------- 应用骨架（源 kit/app.rs 移植 + TopBar 增量件） ----------

/// AppBar 产物（title 可 `Ui::set_text` 换文）。
#[derive(Debug, Clone, Copy)]
pub struct AppBar {
    /// 声明高容器（Col h=56vp）。
    pub frame: NodeId,
    /// 内容行（Row cross=Center）。
    pub bar: NodeId,
    /// 标题文本节点。
    pub title: NodeId,
    /// 返回键图标节点（`back=false` 时 None）。
    pub back: Option<NodeId>,
}

/// AppBar Mini 配方（OH：高 56vp、返回键左上、标题起始 16vp）。
pub fn app_bar<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    title: &[u8],
    back: bool,
) -> Result<AppBar, UiError> {
    let frame = ui.add(
        parent,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: Some(TOPBAR_H), cross: CrossAlign::Stretch })),
    )?;
    let layers = stack(ui, frame)?;
    // 面层（零内在 w/h None——stack max 铁则；终值吃满 stack 分配）。
    ui.add(layers, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_page, w: None, h: None })))?;
    let bar = ui.add(
        layers,
        vspec(ViewKind::Row, Props::Row(RowProps {
            gap: if back { APP_BAR_GAP } else { 0 },
            w: None,
            h: Some(TOPBAR_H), // 内容层定尺寸——撑 stack 内在
            cross: CrossAlign::Center,
        })),
    )?;
    let lead = if back { APP_BAR_LEAD } else { APPBAR_TITLE_INSET };
    spacer(ui, bar, lead, 1)?;
    let mut back_node = None;
    if back {
        let b = ui.add(bar, icon_spec(icons::BACK, t.font_primary, APP_BAR_BACK, APP_BAR_BACK))?;
        back_node = Some(b);
        a11y(ui, b, crate::ROLE_BUTTON, b"back");
    }
    let t_node = ui.add(bar, text_spec(t.font_primary, None, FontSize::S24x48, title)?)?;
    a11y(ui, t_node, crate::ROLE_TEXT, title);
    Ok(AppBar { frame, bar, title: t_node, back: back_node })
}

/// TopBar 产物（波 2 增量件：标题 + 尾随动作位，无返回键语义）。
#[derive(Debug, Clone, Copy)]
pub struct TopBar {
    /// 声明高容器（Col h=56vp）。
    pub frame: NodeId,
    /// 内容行。
    pub bar: NodeId,
    /// 标题文本节点。
    pub title: NodeId,
    /// 尾随动作位（Icon 节点——交互态宿主对 icon 登记）。
    pub actions: [Option<NodeId>; TOPBAR_ACTIONS_MAX],
}

/// TopBar 配方（OH 高 56vp；标题起始 16vp + 尾随动作位）。v1 无 flex
/// ——动作位随标题右移顺排（右对齐挂 flex 先导，诚实标注）。
pub fn top_bar<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    title: &[u8],
    actions: &[u16],
) -> Result<TopBar, UiError> {
    if actions.len() > TOPBAR_ACTIONS_MAX {
        return Err(UiError::NodeBad);
    }
    let frame = ui.add(
        parent,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: Some(TOPBAR_H), cross: CrossAlign::Stretch })),
    )?;
    let layers = stack(ui, frame)?;
    ui.add(layers, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_page, w: None, h: None })))?;
    let bar = ui.add(
        layers,
        vspec(ViewKind::Row, Props::Row(RowProps { gap: 0, w: None, h: Some(TOPBAR_H), cross: CrossAlign::Center })),
    )?;
    spacer(ui, bar, APPBAR_TITLE_INSET, 1)?;
    let t_node = ui.add(bar, text_spec(t.font_primary, None, FontSize::S24x48, title)?)?;
    a11y(ui, t_node, crate::ROLE_TEXT, title);
    let mut acts = [None; TOPBAR_ACTIONS_MAX];
    for (i, id) in actions.iter().enumerate() {
        spacer(ui, bar, TOPBAR_GAP, 1)?; // 标题 → 动作位缝（Row gap=0——标题起始 16vp 不被推）
        let a = ui.add(bar, icon_spec(*id, t.font_primary, TOPBAR_ACT, TOPBAR_ACT))?;
        a11y(ui, a, crate::ROLE_BUTTON, b"action");
        acts[i] = Some(a);
    }
    Ok(TopBar { frame, bar, title: t_node, actions: acts })
}

/// Scaffold 产物。
#[derive(Debug, Clone, Copy)]
pub struct Scaffold {
    /// 骨架 Col。
    pub frame: NodeId,
    /// AppBar 槽产物（`title = None` 时 None）。
    pub top: Option<AppBar>,
    /// 内容锚（Col cross=Start——宿主 `ui.add` 内容）。
    pub content: NodeId,
    /// BottomTabs 槽产物（`bottom_items = None` 时 None）。
    pub bottom: Option<BottomTabs>,
}

/// Col 骨架配方：`title = Some((标题, 带返回键))` 装 AppBar；
/// `bottom_items = Some(...)` 装 BottomTabs；内容锚居中。v1 无 flex：
/// 骨架高 = 环架内在（宿主内容撑高），沉底随先导。
pub fn scaffold<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    title: Option<(&[u8], bool)>,
    bottom_items: Option<&[NavItem<'_>]>,
) -> Result<Scaffold, UiError> {
    let frame = ui.add(
        parent,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch })),
    )?;
    let top = match title {
        Some((t_title, back)) => Some(app_bar(ui, frame, t, t_title, back)?),
        None => None,
    };
    let content = ui.add(
        frame,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Start })),
    )?;
    let bottom = match bottom_items {
        Some(items) => Some(bottom_tabs(ui, frame, t, items, 0)?),
        None => None,
    };
    Ok(Scaffold { frame, top, content, bottom })
}

/// 页栈过渡簿记（一次至多一条——SwipePager Snap 同形）。
struct NavAnim {
    node: NodeId,
    from: i32,
    to: i32,
    start: Option<u64>,
    /// 过渡完毕移交 `popped`（宿主 remove）。
    pop_on_done: bool,
}

/// 页面栈最小版（源 NavStack 状态机移植——主树 `NavStack` 纯数据名
/// 占用，状态机改名 NavHost）：零闭包、出栈移交由宿主执行。用法：
/// `attach` 建叠放容器 → 宿主在栈容器下 `ui.add` 页面（按栈序追加子
/// ——后子盖上）→ `push(ui, page)` 登记并起入栈过渡 → `pop` 起出栈
/// 过渡 → 帧循环 `tick` → `take_popped` 取完结页自 remove。
pub struct NavHost {
    root: Option<NodeId>,
    pages: [Option<NodeId>; NAV_DEPTH_MAX],
    depth: usize,
    anim: Option<NavAnim>,
    popped: Option<NodeId>,
    vp_w: u32,
}

impl NavHost {
    pub fn new() -> Self {
        NavHost { root: None, pages: [None; NAV_DEPTH_MAX], depth: 0, anim: None, popped: None, vp_w: 0 }
    }

    /// 建栈容器（Stack 叠放——全页同起点覆盖）；页宽基准 = 当前视口宽。
    pub fn attach<const N: usize>(
        &mut self,
        ui: &mut Ui<N>,
        parent: NodeId,
    ) -> Result<NodeId, UiError> {
        let (vp_w, _) = ui.viewport();
        self.vp_w = vp_w;
        let r = stack(ui, parent)?;
        self.root = Some(r);
        self.pages = [None; NAV_DEPTH_MAX];
        self.depth = 0;
        Ok(r)
    }

    /// 入栈（页须已挂在栈容器下）：自右侧滑入（OffsetX 300ms CubicOut）。
    /// 满栈（> [`NAV_DEPTH_MAX`]）→ `NodeFull` loud。
    pub fn push<const N: usize>(&mut self, ui: &mut Ui<N>, page: NodeId) -> Result<(), UiError> {
        if self.root.is_none() {
            return Err(UiError::NoRoot);
        }
        if self.depth >= NAV_DEPTH_MAX {
            return Err(UiError::NodeFull);
        }
        ui.set_offset(page, self.vp_w as i32, 0)?;
        self.anim = Some(NavAnim {
            node: page,
            from: self.vp_w as i32,
            to: 0,
            start: None,
            pop_on_done: false,
        });
        self.pages[self.depth] = Some(page);
        self.depth += 1;
        Ok(())
    }

    /// 出栈：逻辑弹栈即时（`top`/`depth` 立刻反映）、视觉延迟——顶页自
    /// 左滑出，过渡完毕经 `take_popped` 移交宿主 remove。
    pub fn pop<const N: usize>(&mut self, ui: &mut Ui<N>) -> bool {
        if self.depth == 0 {
            return false;
        }
        self.depth -= 1;
        let page = self.pages[self.depth];
        self.pages[self.depth] = None;
        if let Some(p) = page {
            let _ = ui.set_offset(p, 0, 0);
            self.anim = Some(NavAnim {
                node: p,
                from: 0,
                to: self.vp_w as i32,
                start: None,
                pop_on_done: true,
            });
        }
        true
    }

    /// 过渡推进（帧循环调；无过渡 → false 零开销；起拍 = 首次 tick）。
    pub fn tick<const N: usize>(&mut self, ui: &mut Ui<N>, now_ns: u64) -> bool {
        let Some(a) = &mut self.anim else { return false };
        let start = match a.start {
            Some(t) => t,
            None => {
                a.start = Some(now_ns);
                now_ns
            }
        };
        let elapsed = now_ns.saturating_sub(start);
        if elapsed >= NAV_ANIM_NS {
            let (node, to, pop) = (a.node, a.to, a.pop_on_done);
            let _ = ui.set_offset(node, to, 0);
            self.anim = None;
            if pop {
                self.popped = Some(node);
            }
            return true;
        }
        let v = anim::interp(Easing::CubicOut, a.from, a.to, elapsed, NAV_ANIM_NS);
        let _ = ui.set_offset(a.node, v, 0);
        true
    }

    /// 取出栈完结页（宿主对其实施 `Ui::remove`）。
    pub fn take_popped(&mut self) -> Option<NodeId> {
        self.popped.take()
    }

    /// 栈顶页（出栈过渡期 = 新栈顶——逻辑弹栈即时语义）。
    pub fn top(&self) -> Option<NodeId> {
        if self.depth == 0 { None } else { self.pages[self.depth - 1] }
    }

    /// 栈深。
    pub fn depth(&self) -> usize {
        self.depth
    }
}

impl Default for NavHost {
    fn default() -> Self {
        Self::new()
    }
}

// ---------- 导航件（源 kit/nav.rs 移植；BottomTabs 改名件） ----------

/// 导航项入参（图标 id 按宿主 IconSource 注册面给）。
pub struct NavItem<'a> {
    pub icon: u16,
    pub label: &'a [u8],
}

/// 单元锚组（bg 承载换底；icon/label 承载换色；text/icon_id = 重建
/// props 的字节/字形源——引擎无 props 读面，值随 Cell 单源）。
#[derive(Debug, Clone, Copy)]
struct Cell {
    bg: NodeId,
    icon: Option<NodeId>,
    icon_id: Option<u16>,
    icon_sz: u32,
    label: NodeId,
    text: TextBuf,
}

impl Cell {
    fn hit(&self, node: NodeId) -> bool {
        self.bg == node || self.label == node || self.icon == Some(node)
    }
}

/// 换底 + 换色（active ↔ idle 两档；同几何 set_props——纯着色零重排
/// 面；bg 层恒 w/h None 重建——面层零内在铁则）。
#[allow(clippy::too_many_arguments)]
fn restyle_cell<const N: usize>(
    ui: &mut Ui<N>,
    c: &Cell,
    on: bool,
    label_scale: FontSize,
    active_bg: Color,
    active_fg: Color,
    idle_bg: Color,
    idle_fg: Color,
) {
    let bg = if on { active_bg } else { idle_bg };
    let _ = ui.set_props(c.bg, Props::Rect(RectProps { bg, w: None, h: None }));
    let fg = if on { active_fg } else { idle_fg };
    if let (Some(i), Some(id)) = (c.icon, c.icon_id) {
        let sz = c.icon_sz;
        let _ = ui.set_props(i, Props::Icon(IconProps { id, fg, w: Some(sz), h: Some(sz) }));
    }
    let _ = ui.set_props(
        c.label,
        Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale: label_scale },
            text: c.text,
        }),
    );
}

/// 页签栏（OH：高 48vp；active 换底 + 换色）——Tabs 的辅助函数。
#[derive(Debug, Clone, Copy)]
pub struct TabBar {
    pub frame: NodeId,
    cells: [Option<Cell>; TABS_MAX],
    pub n: usize,
    pub active: usize,
    tokens: Tokens,
    /// 五态默认（主树查表 [`ItemState`]——消费面 `state.bg()` 直取）。
    pub state: ItemState,
}

impl TabBar {
    /// active 换底/换色（同档 no-op false；域外拒 false）。
    pub fn set_active<const N: usize>(&mut self, ui: &mut Ui<N>, i: usize) -> bool {
        if i >= self.n || i == self.active {
            return false;
        }
        let t = self.tokens;
        for (idx, on) in [(self.active, false), (i, true)] {
            if let Some(c) = self.cells[idx] {
                restyle_cell(ui, &c, on, FontSize::S24x48, dim_accent(t), t.brand, t.bg_page, t.font_secondary);
            }
        }
        self.active = i;
        true
    }

    /// 点击命中（bg/icon/label 任一锚 = 该页签）。
    pub fn hit(&self, node: NodeId) -> Option<usize> {
        self.cells.iter().position(|c| c.is_some_and(|c| c.hit(node)))
    }
}

/// 页签栏配方（labels 空/越 [`TABS_MAX`] → `NodeBad` loud）。
pub fn tab_bar<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    labels: &[&[u8]],
    active: usize,
) -> Result<TabBar, UiError> {
    let n = labels.len();
    if n == 0 || n > TABS_MAX {
        return Err(UiError::NodeBad);
    }
    let frame = ui.add(
        parent,
        vspec(ViewKind::Grid, Props::Grid(GridProps {
            cols: n as u8,
            rows: 1,
            gap: 0,
            cross: CrossAlign::Stretch,
            w: None,
            h: Some(BOTTOMTABS_H),
        })),
    )?;
    let mut cells = [None; TABS_MAX];
    for (i, lb) in labels.iter().enumerate() {
        let on = i == active;
        let fg = if on { t.brand } else { t.font_secondary };
        let cell = stack(ui, frame)?;
        let bg = ui.add(
            cell,
            vspec(ViewKind::Rect, Props::Rect(RectProps {
                bg: if on { dim_accent(*t) } else { t.bg_page },
                w: None,
                h: None, // 面层零内在（stack max 铁则）
            })),
        )?;
        let col = ui.add(
            cell,
            vspec(ViewKind::Col, Props::Col(ColProps {
                gap: 0,
                w: None,
                h: Some(BOTTOMTABS_H),
                cross: CrossAlign::Center,
            })),
        )?;
        spacer(ui, col, 1, (BOTTOMTABS_H - 48) / 2)?; // 标签垂直居中垫
        let buf = TextBuf::new(lb)?;
        let label = ui.add(col, text_spec(fg, None, FontSize::S24x48, lb)?)?;
        a11y(ui, label, crate::ROLE_BUTTON, lb);
        cells[i] = Some(Cell { bg, icon: None, icon_id: None, icon_sz: 0, label, text: buf });
    }
    Ok(TabBar { frame, cells, n, active, tokens: *t, state: ItemState::Normal })
}

/// Seg 分段选择器（高 32vp 拟定；Grid 等分段 + active 高亮）。
#[derive(Debug, Clone, Copy)]
pub struct Seg {
    pub frame: NodeId,
    cells: [Option<Cell>; TABS_MAX],
    pub n: usize,
    pub active: usize,
    tokens: Tokens,
    /// 五态默认（主树查表 [`ItemState`]）。
    pub state: ItemState,
}

impl Seg {
    pub fn set_active<const N: usize>(&mut self, ui: &mut Ui<N>, i: usize) -> bool {
        if i >= self.n || i == self.active {
            return false;
        }
        let t = self.tokens;
        for (idx, on) in [(self.active, false), (i, true)] {
            if let Some(c) = self.cells[idx] {
                restyle_cell(ui, &c, on, FontSize::S24x48, t.brand, ON_ACCENT, t.bg_comp, t.font_primary);
            }
        }
        self.active = i;
        true
    }

    /// 点击命中（bg/label 锚）。
    pub fn hit(&self, node: NodeId) -> Option<usize> {
        self.cells.iter().position(|c| c.is_some_and(|c| c.hit(node)))
    }
}

/// Seg 配方（options 空/越帽 → `NodeBad` loud）。
pub fn seg<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    options: &[&[u8]],
    active: usize,
) -> Result<Seg, UiError> {
    let n = options.len();
    if n == 0 || n > TABS_MAX {
        return Err(UiError::NodeBad);
    }
    let frame = ui.add(
        parent,
        vspec(ViewKind::Grid, Props::Grid(GridProps {
            cols: n as u8,
            rows: 1,
            gap: SEG_GAP,
            cross: CrossAlign::Stretch,
            w: None,
            h: Some(SEG_BAR_H),
        })),
    )?;
    let mut cells = [None; TABS_MAX];
    for (i, lb) in options.iter().enumerate() {
        let on = i == active;
        let cell = stack(ui, frame)?;
        let bg = ui.add(
            cell,
            vspec(ViewKind::Rect, Props::Rect(RectProps {
                bg: if on { t.brand } else { t.bg_comp },
                w: None,
                h: None, // 面层零内在（stack max 铁则）
            })),
        )?;
        let col = ui.add(
            cell,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: Some(SEG_BAR_H), cross: CrossAlign::Center })),
        )?;
        spacer(ui, col, 1, (SEG_BAR_H - 48) / 2)?; // 标签垂直居中垫
        let buf = TextBuf::new(lb)?;
        let label = ui.add(
            col,
            text_spec(if on { ON_ACCENT } else { t.font_primary }, None, FontSize::S24x48, lb)?,
        )?;
        a11y(ui, label, crate::ROLE_BUTTON, lb);
        cells[i] = Some(Cell { bg, icon: None, icon_id: None, icon_sz: 0, label, text: buf });
    }
    Ok(Seg { frame, cells, n, active, tokens: *t, state: ItemState::Normal })
}

/// Step 步进指示（静态配方：Rect+Icon 段拼；进度变 = 重挂）。
pub struct Step {
    pub frame: NodeId,
    /// 步点锚（完成 = 勾 Icon、当前 = brand Rect、未达 = 次底 Rect）。
    pub marks: [Option<NodeId>; STEP_MAX],
    pub n: usize,
}

/// Step 配方（steps 0/越 [`STEP_MAX`] → `NodeBad` loud）。
pub fn step<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    steps: usize,
    current: usize,
) -> Result<Step, UiError> {
    if steps == 0 || steps > STEP_MAX {
        return Err(UiError::NodeBad);
    }
    let frame = ui.add(
        parent,
        vspec(ViewKind::Row, Props::Row(RowProps { gap: APP_BAR_GAP, w: None, h: None, cross: CrossAlign::Center })),
    )?;
    let mut marks = [None; STEP_MAX];
    for i in 0..steps {
        let m = if i < current {
            ui.add(frame, icon_spec(icons::CHECK, t.brand, STEP_DOT, STEP_DOT))?
        } else {
            ui.add(
                frame,
                vspec(ViewKind::Rect, Props::Rect(RectProps {
                    bg: if i == current { t.brand } else { t.bg_comp },
                    w: Some(STEP_DOT),
                    h: Some(STEP_DOT),
                })),
            )?
        };
        marks[i] = Some(m);
        if i + 1 < steps {
            ui.add(
                frame,
                vspec(ViewKind::Rect, Props::Rect(RectProps {
                    bg: t.font_secondary,
                    w: Some(STEP_LINE_W),
                    h: Some(STEP_LINE_H),
                })),
            )?;
        }
    }
    Ok(Step { frame, marks, n: steps })
}

/// BottomTabs 底部页签（波 2 计划口径名；源 bottom_nav 改名件——
/// OH：高 48vp；icon + 小字 + active 换底换色）。
#[derive(Debug, Clone, Copy)]
pub struct BottomTabs {
    pub frame: NodeId,
    cells: [Option<Cell>; TABS_MAX],
    pub n: usize,
    pub active: usize,
    tokens: Tokens,
    /// 五态默认（主树查表 [`ItemState`]）。
    pub state: ItemState,
}

impl BottomTabs {
    pub fn set_active<const N: usize>(&mut self, ui: &mut Ui<N>, i: usize) -> bool {
        if i >= self.n || i == self.active {
            return false;
        }
        let t = self.tokens;
        for (idx, on) in [(self.active, false), (i, true)] {
            if let Some(c) = self.cells[idx] {
                restyle_cell(ui, &c, on, FontSize::S16x32, dim_accent(t), t.brand, t.bg_page, t.font_secondary);
            }
        }
        self.active = i;
        true
    }

    /// 点击命中。
    pub fn hit(&self, node: NodeId) -> Option<usize> {
        self.cells.iter().position(|c| c.is_some_and(|c| c.hit(node)))
    }
}

/// BottomTabs 配方（items 空/越帽 → `NodeBad` loud）。
pub fn bottom_tabs<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    items: &[NavItem<'_>],
    active: usize,
) -> Result<BottomTabs, UiError> {
    let n = items.len();
    if n == 0 || n > TABS_MAX {
        return Err(UiError::NodeBad);
    }
    let frame = ui.add(
        parent,
        vspec(ViewKind::Grid, Props::Grid(GridProps {
            cols: n as u8,
            rows: 1,
            gap: 0,
            cross: CrossAlign::Stretch,
            w: None,
            h: Some(BOTTOMTABS_H),
        })),
    )?;
    let mut cells = [None; TABS_MAX];
    for (i, it) in items.iter().enumerate() {
        let on = i == active;
        let fg = if on { t.brand } else { t.font_secondary };
        let cell = stack(ui, frame)?;
        let bg = ui.add(
            cell,
            vspec(ViewKind::Rect, Props::Rect(RectProps {
                bg: if on { dim_accent(*t) } else { t.bg_page },
                w: None,
                h: None, // 面层零内在（stack max 铁则）
            })),
        )?;
        let col = ui.add(
            cell,
            vspec(ViewKind::Col, Props::Col(ColProps {
                gap: 0,
                w: None,
                h: Some(BOTTOMTABS_H),
                cross: CrossAlign::Center,
            })),
        )?;
        // (144 − (72 + 8 + 32)) / 2 顶垫。
        spacer(ui, col, 1, (BOTTOMTABS_H - (ICON_SZ + 8 + 32)) / 2)?;
        let icon = ui.add(col, icon_spec(it.icon, fg, ICON_SZ, ICON_SZ))?;
        spacer(ui, col, 1, 8)?; // icon → 字标缝 8px
        let buf = TextBuf::new(it.label)?;
        let label = ui.add(col, text_spec(fg, None, FontSize::S16x32, it.label)?)?;
        a11y(ui, label, crate::ROLE_BUTTON, it.label);
        cells[i] = Some(Cell {
            bg,
            icon: Some(icon),
            icon_id: Some(it.icon),
            icon_sz: ICON_SZ,
            label,
            text: buf,
        });
    }
    Ok(BottomTabs { frame, cells, n, active, tokens: *t, state: ItemState::Normal })
}

// ---------- 列表件（源 kit/list.rs 移植——Card/Collapse kind 容器化） ----------

/// Card 产物（主树 Card kind 容器直用——自绘面底 + elevation 阴影、
/// corner_radius 数据携载（v1 直角）；content = 宿主内容锚）。
#[derive(Debug, Clone, Copy)]
pub struct Card {
    /// Card kind 容器（面底/阴影 = kind 自绘）。
    pub node: NodeId,
    /// 内容锚（Col cross=Start——宿主 `ui.add` 行内容）。
    pub content: NodeId,
    /// 五态默认（主树查表 [`ItemState`]）。
    pub state: ItemState,
}

/// CARD 配方（OH：高 48vp、宽随父 100%）。elevation > 3 由 add 面
/// `widget_value_ok` loud 拒（值域单源——配方不重复执法）；圆角 =
/// OH 8vp（corner_radius 直载，raster 圆角先导后生效）。
pub fn card<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
    elevation: u32,
    leading_icon: Option<u16>,
) -> Result<Card, UiError> {
    let node = ui.add(
        parent,
        vspec(ViewKind::Card, Props::Card(CardProps { elevation, corner_radius: LISTITEM_RADIUS })),
    )?;
    let row = ui.add(
        node,
        vspec(ViewKind::Row, Props::Row(RowProps {
            gap: 0,
            w: None,
            h: Some(LISTITEM_H),
            cross: CrossAlign::Center,
        })),
    )?;
    spacer(ui, row, LISTITEM_PAD_X, 1)?;
    if let Some(id) = leading_icon {
        ui.add(row, icon_spec(id, t.font_secondary, ICON_SZ, ICON_SZ))?;
    }
    let content = ui.add(
        row,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 8, w: None, h: None, cross: CrossAlign::Start })),
    )?;
    spacer(ui, row, LISTITEM_PAD_X, 1)?;
    a11y(ui, node, crate::ROLE_LIST_ITEM, b"card");
    Ok(Card { node, content, state: ItemState::Normal })
}

/// ListItem 行变体（波 2 计划口径名；无前导图标 + 零 elevation 薄别名）。
pub fn list_item<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    t: &Tokens,
) -> Result<Card, UiError> {
    card(ui, parent, t, 0, None)
}

/// 折叠面板（主树 Collapse kind 容器直用：自带 header 带 + 箭头指示
/// （展开朝下/收起朝右）、expanded 翻转 = 子三面守卫（layout 不摆位/
/// paint 不发射/hit 不命中）——源「title+chevron 组合头 + 宿主手动挂
/// 卸 body」形态随 kind 化退役；title 面挂 kind props 递进）。
pub struct Collapse {
    /// Collapse kind 容器（header 视觉 = kind 自绘）。
    pub node: NodeId,
    /// body 锚（Col cross=Start——宿主内容；收起态自动隐藏）。
    pub body: NodeId,
    open: bool,
    on_toggle: u32,
    /// 五态默认（主树查表 [`ItemState`]——头行按压语义）。
    pub state: ItemState,
}

impl Collapse {
    /// 展开态置位：`set_props` 翻转 expanded（主树全 kind set_props
    /// 开口——源「kind 锚重挂」技巧退役：节点 id 稳定、body 子树保
    /// 持）；同态 no-op false。
    pub fn set_open<const N: usize>(&mut self, ui: &mut Ui<N>, open: bool) -> bool {
        if open == self.open {
            return false;
        }
        self.open = open;
        let _ = ui.set_props(self.node, Props::Collapse(CollapseProps { expanded: open, on_toggle: self.on_toggle }));
        true
    }

    /// 翻转。
    pub fn toggle<const N: usize>(&mut self, ui: &mut Ui<N>) -> bool {
        let next = !self.open;
        self.set_open(ui, next)
    }

    /// 当前态。
    pub fn is_open(&self) -> bool {
        self.open
    }
}

/// Collapse 配方（expanded 初值直载；body 锚随附）。
pub fn collapse<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    expanded: bool,
    on_toggle: u32,
) -> Result<Collapse, UiError> {
    let node = ui.add(
        parent,
        vspec(ViewKind::Collapse, Props::Collapse(CollapseProps { expanded, on_toggle })),
    )?;
    let body = ui.add(
        node,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 8, w: None, h: None, cross: CrossAlign::Start })),
    )?;
    Ok(Collapse { node, body, open: expanded, on_toggle, state: ItemState::Normal })
}

/// 吸附簿记（SwipePager::Snap 同形）。
struct Snap {
    from: i32,
    to: i32,
    start: Option<u64>,
}

/// 滑动展开行（源 SwipeItem 状态机移植 + 主树 kind 锚直载：node 直载
/// action_count/reveal_*/on_action——reveal 带视觉 = kind 自绘
/// （reveal_end 驱动）；frame = Stack 叠放帧（content 全幅 + actions
/// 右移层 + kind 锚满幅铺底）。主树 `add` 父位白名单未收编组件 kind
/// ——kind 锚不携子（源「占位锚」技巧保留）；态位翻转走 `set_props`
/// （节点 id 稳定）。注意：主树 SwipeItem 内在 0×0（内在面挂后续布局
/// 刀）——消费面宜 Root/Stack 随父填充语境）。
pub struct SwipeItem {
    /// 叠放帧（Stack：kind 锚 + content + actions 层）。
    pub frame: NodeId,
    /// SwipeItem kind 锚（占位满幅铺底——reveal 带自绘）。
    pub node: NodeId,
    /// 内容锚（宿主行卡——置于下层、吃满）。
    pub content: NodeId,
    /// 动作锚（Stack——宿主动作位；随展开层整体右移）。
    pub actions: NodeId,
    /// 动作层（定尺寸 wrapper——set_offset 目标）。
    layer: NodeId,
    width: i32,
    action_count: u32,
    on_action: u32,
    open: bool,
    ox: i32,
    dragging: bool,
    base_ox: i32,
    x0: i32,
    last_x: i32,
    snap: Option<Snap>,
    /// 五态默认（主树查表 [`ItemState`]）。
    pub state: ItemState,
}

impl SwipeItem {
    /// 装配：`width` = 行全宽（v1 取 vp_w）；动作层初位 = 右缘外。动作
    /// 数 `action_count` 直载 kind（0 合法 = 纯行项）。
    pub fn attach<const N: usize>(
        ui: &mut Ui<N>,
        parent: NodeId,
        t: &Tokens,
        width: u32,
        action_count: u32,
        on_action: u32,
    ) -> Result<SwipeItem, UiError> {
        let frame = stack(ui, parent)?;
        let node = ui.add(
            frame,
            vspec(ViewKind::SwipeItem, Props::SwipeItem(SwipeItemProps {
                action_count,
                reveal_start: 0,
                reveal_end: 0,
                on_action,
            })),
        )?;
        let content = ui.add(
            frame,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch })),
        )?;
        // 动作层 = 定尺寸 wrapper（Row 288×144）内嵌 Stack：底面 Rect
        // 满幅 + 宿主动作位叠放（主树 Row 子宽 = 内在——底面经 Stack
        // 全幅叠放承载）。
        let layer = ui.add(
            frame,
            vspec(ViewKind::Row, Props::Row(RowProps {
                gap: 0,
                w: Some(SWIPE_REVEAL_W),
                h: Some(LISTITEM_H),
                cross: CrossAlign::Stretch,
            })),
        )?;
        let actions = ui.add(layer, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Start })))?;
        // 动作层底色（显式定尺寸——meas 撑起 Stack 内在）。
        ui.add(
            actions,
            vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_comp, w: Some(SWIPE_REVEAL_W), h: Some(LISTITEM_H) })),
        )?;
        let w = width as i32;
        let _ = ui.set_offset(layer, w, 0);
        Ok(SwipeItem {
            frame,
            node,
            content,
            actions,
            layer,
            width: w,
            action_count,
            on_action,
            open: false,
            ox: w,
            dragging: false,
            base_ox: w,
            x0: 0,
            last_x: 0,
            snap: None,
            state: ItemState::Normal,
        })
    }

    /// Msg 消费（横拖域内；几何命中 = 本行 bounds——SwipePager 先例）。
    pub fn on_msg<const N: usize>(&mut self, ui: &mut Ui<N>, msg: &crate::desc::Msg) -> bool {
        use crate::desc::Msg;
        match msg {
            Msg::DragStart { axis, x, y, .. } => {
                if *axis != crate::desc::Axis::X || !self.in_strip(ui, *x, *y) {
                    return false;
                }
                self.snap = None;
                self.dragging = true;
                self.base_ox = self.ox;
                self.x0 = *x as i32;
                self.last_x = self.x0;
                true
            }
            Msg::DragMove { x, .. } => {
                if !self.dragging {
                    return false;
                }
                self.last_x = *x as i32;
                let dx = self.last_x - self.x0;
                let mut ox = self.base_ox + dx;
                let min = self.width - SWIPE_REVEAL_W as i32;
                // 橡皮筋 ×0.5（越界侧 excess/2——SwipePager 同律）。
                if ox > self.width {
                    ox = self.width + (ox - self.width) / 2;
                }
                if ox < min {
                    ox = min + (ox - min) / 2;
                }
                self.apply(ui, ox);
                true
            }
            Msg::DragEnd { vx, .. } => {
                if !self.dragging {
                    return false;
                }
                self.dragging = false;
                let dx = self.last_x - self.x0;
                let threshold = (SWIPE_REVEAL_W as i32 * 4) / 10; // 位移 40%
                let target_open = if vx.abs() >= FLIP_VEL_PX_S {
                    *vx < 0 // 右滑关、左滑开（速度向）
                } else if dx.abs() > threshold {
                    dx < 0
                } else {
                    self.open
                };
                self.begin_snap(ui, target_open);
                true
            }
            Msg::Cancel { .. } => {
                if !self.dragging {
                    return false;
                }
                self.dragging = false;
                self.begin_snap(ui, self.open);
                true
            }
            Msg::Press { .. } | Msg::Release { .. } | Msg::Click { .. }
            | Msg::FocusIn { .. } | Msg::FocusOut { .. } => false,
        }
    }

    /// 吸附推进（帧循环；无吸附 → false 零开销）。
    pub fn tick<const N: usize>(&mut self, ui: &mut Ui<N>, now_ns: u64) -> bool {
        let Some(s) = &mut self.snap else { return false };
        let start = match s.start {
            Some(t) => t,
            None => {
                s.start = Some(now_ns);
                now_ns
            }
        };
        let elapsed = now_ns.saturating_sub(start);
        if elapsed >= SNAP_DUR_NS {
            let to = s.to;
            self.apply(ui, to);
            self.snap = None;
            return true;
        }
        let v = anim::interp(Easing::QuadOut, s.from, s.to, elapsed, SNAP_DUR_NS);
        self.apply(ui, v);
        true
    }

    /// 展开态。
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// 直落开/关（无动画语义变体：起吸附）。
    pub fn set_open<const N: usize>(&mut self, ui: &mut Ui<N>, open: bool) {
        self.begin_snap(ui, open);
    }

    fn begin_snap<const N: usize>(&mut self, ui: &mut Ui<N>, open: bool) {
        if open != self.open {
            self.open = open;
            self.sync_kind(ui);
        }
        let to = if open { self.width - SWIPE_REVEAL_W as i32 } else { self.width };
        if self.ox == to {
            self.snap = None;
            return;
        }
        self.snap = Some(Snap { from: self.ox, to, start: None });
    }

    /// kind 态位同步（reveal permille——`set_props` 原位更新：节点 id
    /// 稳定、子树保持；末端滑出 = 展开 1000‰）。
    fn sync_kind<const N: usize>(&mut self, ui: &mut Ui<N>) {
        let _ = ui.set_props(
            self.node,
            Props::SwipeItem(SwipeItemProps {
                action_count: self.action_count,
                reveal_start: 0,
                reveal_end: if self.open { 1000 } else { 0 },
                on_action: self.on_action,
            }),
        );
    }

    fn apply<const N: usize>(&mut self, ui: &mut Ui<N>, ox: i32) {
        if ui.set_offset(self.layer, ox, 0).is_ok() {
            self.ox = ox;
        }
    }

    fn in_strip<const N: usize>(&self, ui: &Ui<N>, x: u32, y: u32) -> bool {
        in_bounds(ui, self.frame, x, y)
    }
}

/// 拖拽簿记（拖项索引 + 起拖 y + 起拖时偏移——视觉连续性基准）。
#[derive(Debug, Clone, Copy)]
struct Drag {
    idx: usize,
    y0: u32,
    base_ofs: i32,
}

/// 窗内小规模拖拽重排（源状态机移植 + 主树 kind 手柄叶：每行经
/// [`DragReorder::add_item`] 挂「Stack 行格（行 spec + 24×36 手柄
/// 锚）」——手柄 meas 由行格 max 吸收、不扰行几何；手柄直载
/// item_index/total_items/on_reorder，totals 收口走 `set_props`）。
pub struct DragReorder {
    /// 行容器（Col——等高行经 [`DragReorder::add_item`] 挂入）。
    pub container: NodeId,
    items: [Option<NodeId>; REORDER_MAX],
    /// kind 手柄锚（行 i 的 wire 数据面）。
    pub handles: [Option<NodeId>; REORDER_MAX],
    /// 视觉槽 → 项索引（初值恒等）。
    slots: [usize; REORDER_MAX],
    n: usize,
    item_h: i32,
    drag: Option<Drag>,
    on_reorder: u32,
    /// 五态默认（主树查表 [`ItemState`]——行按压语义）。
    pub state: ItemState,
}

impl DragReorder {
    /// 装配（`item_h` = 等高行高——判定几何单源）。
    pub fn attach<const N: usize>(
        ui: &mut Ui<N>,
        parent: NodeId,
        item_h: u32,
    ) -> Result<DragReorder, UiError> {
        let container = ui.add(
            parent,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch })),
        )?;
        Ok(DragReorder {
            container,
            items: [None; REORDER_MAX],
            handles: [None; REORDER_MAX],
            slots: [0; REORDER_MAX],
            n: 0,
            item_h: item_h as i32,
            drag: None,
            on_reorder: 0,
            state: ItemState::Normal,
        })
    }

    /// 挂行（满 [`REORDER_MAX`] → `NodeFull` loud）：行 spec 与 kind
    /// 手柄锚同入 Stack 行格（`total_items` 暂记 n+1——终值
    /// [`DragReorder::sync_totals`]）。
    pub fn add_item<const N: usize>(
        &mut self,
        ui: &mut Ui<N>,
        spec: ViewSpec,
        on_reorder: u32,
    ) -> Result<NodeId, UiError> {
        if self.n >= REORDER_MAX {
            return Err(UiError::NodeFull);
        }
        self.on_reorder = on_reorder;
        let cell = ui.add(self.container, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Start })))?;
        let id = ui.add(cell, spec)?;
        let h = ui.add(
            cell,
            vspec(ViewKind::DragReorder, Props::DragReorder(DragReorderProps {
                item_index: self.n as u32,
                total_items: (self.n + 1) as u32,
                on_reorder,
            })),
        )?;
        self.items[self.n] = Some(id);
        self.handles[self.n] = Some(h);
        self.slots[self.n] = self.n;
        self.n += 1;
        Ok(id)
    }

    /// 手柄锚 totals 收口（`set_props` 原位更新——源「整锚重挂」退
    /// 役；挂完行后调一次）。
    pub fn sync_totals<const N: usize>(&mut self, ui: &mut Ui<N>) {
        for i in 0..self.n {
            if let Some(h) = self.handles[i] {
                let _ = ui.set_props(
                    h,
                    Props::DragReorder(DragReorderProps {
                        item_index: i as u32,
                        total_items: self.n as u32,
                        on_reorder: self.on_reorder,
                    }),
                );
            }
        }
    }

    /// Msg 消费（纵拖几何命中 = 容器 bounds 内——节点不定形）。
    pub fn on_msg<const N: usize>(&mut self, ui: &mut Ui<N>, msg: &crate::desc::Msg) -> bool {
        use crate::desc::Msg;
        match msg {
            Msg::DragStart { axis, y, .. } => {
                if *axis != crate::desc::Axis::Y {
                    return false;
                }
                let Some(row) = self.row_at(ui, *y) else { return false };
                let idx = self.slots[row];
                self.drag = Some(Drag { idx, y0: *y, base_ofs: (row as i32 - idx as i32) * self.item_h });
                true
            }
            Msg::DragMove { y, .. } => {
                let Some(d) = self.drag else { return false };
                let dy = *y as i32 - d.y0 as i32;
                // 拖项视觉中心定槽（连续跟手——base_ofs + dy 无跳变）。
                let vis_top = d.idx as i64 * self.item_h as i64 + d.base_ofs as i64 + dy as i64;
                let target =
                    ((vis_top + self.item_h as i64 / 2) / self.item_h as i64).clamp(0, self.n as i64 - 1) as usize;
                let home = self.slot_of(d.idx);
                if target != home {
                    self.shift(ui, d.idx, home, target);
                }
                if let Some(n) = self.items[d.idx] {
                    let _ = ui.set_offset(n, 0, d.base_ofs + dy);
                }
                true
            }
            Msg::DragEnd { .. } | Msg::Cancel { .. } => {
                let Some(d) = self.drag.take() else { return false };
                // 落位吸附：拖项贴齐目标槽（offset = 槽差 × 行高）。
                if let Some(n) = self.items[d.idx] {
                    let snap = (self.slot_of(d.idx) as i32 - d.idx as i32) * self.item_h;
                    let _ = ui.set_offset(n, 0, snap);
                }
                true
            }
            Msg::Press { .. } | Msg::Release { .. } | Msg::Click { .. }
            | Msg::FocusIn { .. } | Msg::FocusOut { .. } => false,
        }
    }

    /// 行数。
    pub fn len(&self) -> usize {
        self.n
    }

    /// 空谓词。
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// 视觉槽序快照（`out[slot] = 项索引`；返回填充数）。
    pub fn order(&self, out: &mut [usize]) -> usize {
        let k = out.len().min(self.n);
        for s in 0..k {
            out[s] = self.slots[s];
        }
        k
    }

    /// 清视觉偏移（宿主重建树前的复位面）。
    pub fn reset_offsets<const N: usize>(&mut self, ui: &mut Ui<N>) {
        for i in 0..self.n {
            if let Some(n) = self.items[i] {
                let _ = ui.set_offset(n, 0, 0);
            }
            self.slots[i] = i;
        }
        self.drag = None;
    }

    fn slot_of(&self, idx: usize) -> usize {
        self.slots.iter().position(|&s| s == idx).unwrap_or(idx)
    }

    /// 槽移位：拖项 home → target，区间项逆步进；移位项 set_offset 贴新槽。
    fn shift<const N: usize>(&mut self, ui: &mut Ui<N>, idx: usize, home: usize, target: usize) {
        let step = if target > home { -1i32 } else { 1i32 };
        let (lo, hi) = if target > home { (home + 1, target) } else { (target, home) };
        for s in lo..=hi {
            let moved = self.slots[s];
            self.slots[(s as i32 + step) as usize] = moved;
            if let Some(n) = self.items[moved] {
                let new_slot = (s as i32 + step) as usize;
                let _ = ui.set_offset(n, 0, (new_slot as i32 - moved as i32) * self.item_h);
            }
        }
        self.slots[target] = idx;
    }

    fn row_at<const N: usize>(&self, ui: &Ui<N>, y: u32) -> Option<usize> {
        let b = ui.bounds(self.container).ok()?;
        let rel = y as i64 - b.y as i64;
        if rel < 0 || self.n == 0 {
            return None;
        }
        let row = (rel / self.item_h as i64) as usize;
        if row >= self.n {
            return None;
        }
        Some(row)
    }
}

// ---------- 小件（源 kit/chip.rs + frozen.rs 移植——kind 直载化） ----------

/// Chip 产物（主树 Chip kind 直载：label/selected/on_tap 直载——胶囊
/// 底 + 居中标签 + 选中色变 = kind 自绘；内在 = 标签×8+16 留白 ×32）。
#[derive(Debug, Clone, Copy)]
pub struct Chip {
    /// Chip kind 锚。
    pub node: NodeId,
    /// 五态默认（主树查表 [`ItemState`]）。
    pub state: ItemState,
}

/// Chip 配方（kind 直载——selected 位/事件 id 直载 props；选中翻转
/// 走宿主 `set_props`（主树全 kind 开口——源「重挂或宿主重建」退役）。
pub fn chip<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    label: &[u8],
    selected: bool,
    on_tap: u32,
) -> Result<Chip, UiError> {
    let node = ui.add(
        parent,
        vspec(ViewKind::Chip, Props::Chip(ChipProps { label: TextBuf::new(label)?, selected, on_tap })),
    )?;
    a11y(ui, node, crate::ROLE_BUTTON, label);
    Ok(Chip { node, state: ItemState::Normal })
}

/// 角标配方（主树 Badge kind 直载：count=0 隐藏（0×0 空绘）、>99 封
/// 顶「99+」、红底白字 = kind 自绘——源「Pad+Text 衬底」组合形态随
/// kind 化退役）。
pub fn badge<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    count: u32,
) -> Result<NodeId, UiError> {
    let node = ui.add(parent, vspec(ViewKind::Badge, Props::Badge(BadgeProps { count, show_dot: false })))?;
    a11y(ui, node, crate::ROLE_TEXT, b"badge");
    Ok(node)
}

/// Qr 显示面（主树 Qr kind 直载：module_count + 双色直载；点阵方阵 =
/// module×4 px、定位图案 = kind 自绘）。module_count 须在 QR 版本域
/// {21,25,...,177}（21+4×(v−1)，版本 1..=40）——域外 add 面
/// `widget_value_ok` loud 拒（值域单源）。
pub fn qr_show<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    module_count: u32,
    dark_color: u32,
    light_color: u32,
) -> Result<NodeId, UiError> {
    ui.add(parent, vspec(ViewKind::Qr, Props::Qr(QrProps { module_count, dark_color, light_color })))
}

/// Fab 产物（主树 Fab kind 直载：icon/extended_label/on_tap 直载——
/// 八角圆形 ACCENT 底 + '+' + 扩展标签带 = kind 自绘；空标签 = 56 圆
/// 形、扩展 = 56+8+标签×8）。
#[derive(Debug, Clone, Copy)]
pub struct Fab {
    /// Fab kind 锚。
    pub node: NodeId,
    /// 五态默认（主树查表 [`ItemState`]）。
    pub state: ItemState,
}

/// Fab 配方（kind 直载；`extended_label` 空 = 纯圆形，非空 = 扩展形态）。
pub fn fab<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    icon: u32,
    extended_label: &[u8],
    on_tap: u32,
) -> Result<Fab, UiError> {
    let node = ui.add(
        parent,
        vspec(ViewKind::Fab, Props::Fab(FabProps { icon, extended_label: TextBuf::new(extended_label)?, on_tap })),
    )?;
    a11y(ui, node, crate::ROLE_BUTTON, b"fab");
    Ok(Fab { node, state: ItemState::Normal })
}

/// IconButton 配方（Button 族小档——主树 props 无尺寸档，v1 与 fab
/// 同 kind 直载（空扩展标签），触位/圆角小档随 props 递进挂后续刀）。
pub fn icon_button<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    icon: u32,
    on_tap: u32,
) -> Result<Fab, UiError> {
    let node = ui.add(
        parent,
        vspec(ViewKind::Fab, Props::Fab(FabProps { icon, extended_label: TextBuf::new(b"")?, on_tap })),
    )?;
    a11y(ui, node, crate::ROLE_BUTTON, b"icon button");
    Ok(Fab { node, state: ItemState::Normal })
}

/// Gauge 产物（主树 Gauge kind 直载：value permille + Ring/Bar/Arc
/// 三档直载——v1 简化粗水平条视觉 = kind 自绘；内在 64×64）。
#[derive(Debug, Clone, Copy)]
pub struct Gauge {
    /// Gauge kind 锚。
    pub node: NodeId,
    /// 五态默认（主树查表 [`ItemState`]——纯显示件挂 Disabled 同底
    /// 查表位）。
    pub state: ItemState,
}

/// Gauge 配方（`value` = 0..=1000 permille——域外 add 面
/// `widget_value_ok` loud 拒，值域单源）。
pub fn gauge<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    value_permille: u32,
    style: GaugeStyle,
) -> Result<Gauge, UiError> {
    let node = ui.add(parent, vspec(ViewKind::Gauge, Props::Gauge(GaugeProps { value: value_permille, style })))?;
    Ok(Gauge { node, state: ItemState::Normal })
}

// ---------- 波 2 增量件（Sheet/Drawer/Refresh） ----------

/// 纵向吸附簿记（Sheet/Drawer 共用——Snap 同形换轴）。
struct Slide {
    from: i32,
    to: i32,
    start: Option<u64>,
}

/// 底部抽屉（波 2 增量件）：面板自锚区下缘滑入——`attach` 建叠放容器
/// （高 = 面板高，宿主锚于视口底缘）+ 面板（closed oy = 面板高 = 全
/// 藏于下）。跟手纵拖 + 速度/位移判据 + QuadOut 吸附（SwipeItem 同
/// 律）。
pub struct Sheet {
    /// 叠放容器（Stack，h = 面板高）。
    pub frame: NodeId,
    /// 面板锚（Col w×h + 内层 Stack 底面——宿主 `ui.add` 面板内容于
    /// [`Sheet::content`]）。
    pub panel: NodeId,
    /// 面板内容锚（叠于底面上层）。
    pub content: NodeId,
    h: i32,
    open: bool,
    oy: i32,
    dragging: bool,
    base_oy: i32,
    y0: u32,
    last_y: u32,
    slide: Option<Slide>,
}

impl Sheet {
    /// 装配（`width`/`height` = 面板尺寸；closed = 面板顶与锚区下缘齐）。
    /// 面板底面经内层 Stack 全幅叠放（主树 Col 子高 = 内在——Rect
    /// w/h None 折 0，直挂会塌）。
    pub fn attach<const N: usize>(
        ui: &mut Ui<N>,
        parent: NodeId,
        t: &Tokens,
        width: u32,
        height: u32,
    ) -> Result<Sheet, UiError> {
        let frame = stack(ui, parent)?;
        let panel = ui.add(
            frame,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: Some(width), h: Some(height), cross: CrossAlign::Stretch })),
        )?;
        let inner = ui.add(panel, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Start })))?;
        // 面板底面（定高 meas 撑起 inner 内在；终值吃满 inner 分配）。
        ui.add(inner, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_comp, w: None, h: Some(height) })))?;
        let content = ui.add(
            inner,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch })),
        )?;
        let h = height as i32;
        let _ = ui.set_offset(panel, 0, h);
        Ok(Sheet { frame, panel, content, h, open: false, oy: h, dragging: false, base_oy: h, y0: 0, last_y: 0, slide: None })
    }

    /// Msg 消费（纵拖域内——几何命中 = 面板 bounds；下拉关闭向）。
    pub fn on_msg<const N: usize>(&mut self, ui: &mut Ui<N>, msg: &crate::desc::Msg) -> bool {
        use crate::desc::Msg;
        match msg {
            Msg::DragStart { axis, x, y, .. } => {
                if *axis != crate::desc::Axis::Y || !self.in_strip(ui, *x, *y) {
                    return false;
                }
                self.slide = None;
                self.dragging = true;
                self.base_oy = self.oy;
                self.y0 = *y;
                self.last_y = *y;
                true
            }
            Msg::DragMove { y, .. } => {
                if !self.dragging {
                    return false;
                }
                self.last_y = *y;
                let dy = *y as i32 - self.y0 as i32;
                let oy = (self.base_oy + dy).clamp(0, self.h);
                self.apply(ui, oy);
                true
            }
            Msg::DragEnd { vy, .. } => {
                if !self.dragging {
                    return false;
                }
                self.dragging = false;
                let dy = self.last_y as i32 - self.y0 as i32;
                let threshold = self.h * 4 / 10;
                let target_open = if vy.abs() >= FLIP_VEL_PX_S {
                    *vy < 0 // 上滑开、下滑关（速度向）
                } else if dy.abs() > threshold {
                    dy < 0
                } else {
                    self.open
                };
                self.begin_slide(target_open);
                true
            }
            Msg::Cancel { .. } => {
                if !self.dragging {
                    return false;
                }
                self.dragging = false;
                self.begin_slide(self.open);
                true
            }
            Msg::Press { .. } | Msg::Release { .. } | Msg::Click { .. }
            | Msg::FocusIn { .. } | Msg::FocusOut { .. } => false,
        }
    }

    /// 吸附推进（帧循环；无吸附 → false 零开销；起拍 = 首次 tick）。
    pub fn tick<const N: usize>(&mut self, ui: &mut Ui<N>, now_ns: u64) -> bool {
        let Some(s) = &mut self.slide else { return false };
        let start = match s.start {
            Some(t) => t,
            None => {
                s.start = Some(now_ns);
                now_ns
            }
        };
        let elapsed = now_ns.saturating_sub(start);
        if elapsed >= SNAP_DUR_NS {
            let to = s.to;
            self.apply(ui, to);
            self.slide = None;
            return true;
        }
        let v = anim::interp(Easing::QuadOut, s.from, s.to, elapsed, SNAP_DUR_NS);
        self.apply(ui, v);
        true
    }

    /// 展开态。
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// 直落开/关（起吸附）。
    pub fn set_open<const N: usize>(&mut self, _ui: &mut Ui<N>, open: bool) {
        self.begin_slide(open);
    }

    fn begin_slide(&mut self, open: bool) {
        self.open = open;
        let to = if open { 0 } else { self.h };
        if self.oy == to {
            self.slide = None;
            return;
        }
        self.slide = Some(Slide { from: self.oy, to, start: None });
    }

    fn apply<const N: usize>(&mut self, ui: &mut Ui<N>, oy: i32) {
        if ui.set_offset(self.panel, 0, oy).is_ok() {
            self.oy = oy;
        }
    }

    fn in_strip<const N: usize>(&self, ui: &Ui<N>, x: u32, y: u32) -> bool {
        in_bounds(ui, self.panel, x, y)
    }
}

/// 侧滑抽屉（波 2 增量件）：面板自左缘滑入覆盖内容（叠放 z 序 = 子
/// 序——面板在上层；scrim 无 alpha 通道不做，宿主可另叠——诚实标
/// 注）。闭合态起拖限 [`DRAWER_EDGE_PX`] 边域（不偷全域横拖）。
pub struct Drawer {
    /// 叠放容器（Stack：content 在下层、panel 盖上）。
    pub frame: NodeId,
    /// 内容锚（Col cross=Stretch——宿主主内容）。
    pub content: NodeId,
    /// 抽屉面板锚（Col w×h + 内层 Stack 底面——宿主 `ui.add` 抽屉内
    /// 容于 [`Drawer::panel_content`]）。
    pub panel: NodeId,
    /// 面板内容锚（叠于底面上层）。
    pub panel_content: NodeId,
    w: i32,
    open: bool,
    ox: i32,
    dragging: bool,
    base_ox: i32,
    x0: i32,
    last_x: i32,
    slide: Option<Slide>,
}

impl Drawer {
    /// 装配（`width`/`height` = 面板尺寸；closed = 面板藏于左缘外）。
    pub fn attach<const N: usize>(
        ui: &mut Ui<N>,
        parent: NodeId,
        t: &Tokens,
        width: u32,
        height: u32,
    ) -> Result<Drawer, UiError> {
        let frame = stack(ui, parent)?;
        let content = ui.add(
            frame,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch })),
        )?;
        let panel = ui.add(
            frame,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: Some(width), h: Some(height), cross: CrossAlign::Stretch })),
        )?;
        let inner = ui.add(panel, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Start })))?;
        ui.add(inner, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_comp, w: None, h: Some(height) })))?;
        let panel_content = ui.add(
            inner,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch })),
        )?;
        let w = width as i32;
        let _ = ui.set_offset(panel, -w, 0);
        Ok(Drawer { frame, content, panel, panel_content, w, open: false, ox: -w, dragging: false, base_ox: -w, x0: 0, last_x: 0, slide: None })
    }

    /// Msg 消费（横拖：闭合态起拖点须在 [`DRAWER_EDGE_PX`] 边域内；
    /// 展开态全域可拖关）。
    pub fn on_msg<const N: usize>(&mut self, ui: &mut Ui<N>, msg: &crate::desc::Msg) -> bool {
        use crate::desc::Msg;
        match msg {
            Msg::DragStart { axis, x, y, .. } => {
                if *axis != crate::desc::Axis::X {
                    return false;
                }
                let (x, y) = (*x as i32, *y);
                if !self.open && x > DRAWER_EDGE_PX as i32 {
                    return false; // 闭合态：左缘边域起拖才开
                }
                if !self.in_strip(ui, x.max(0) as u32, y) {
                    return false;
                }
                self.slide = None;
                self.dragging = true;
                self.base_ox = self.ox;
                self.x0 = x;
                self.last_x = x;
                true
            }
            Msg::DragMove { x, .. } => {
                if !self.dragging {
                    return false;
                }
                self.last_x = *x as i32;
                let dx = self.last_x - self.x0;
                let ox = (self.base_ox + dx).clamp(-self.w, 0);
                self.apply(ui, ox);
                true
            }
            Msg::DragEnd { vx, .. } => {
                if !self.dragging {
                    return false;
                }
                self.dragging = false;
                let dx = self.last_x - self.x0;
                let threshold = self.w * 4 / 10;
                let target_open = if vx.abs() >= FLIP_VEL_PX_S {
                    *vx > 0 // 右滑开、左滑关（速度向）
                } else if dx.abs() > threshold {
                    dx > 0
                } else {
                    self.open
                };
                self.begin_slide(target_open);
                true
            }
            Msg::Cancel { .. } => {
                if !self.dragging {
                    return false;
                }
                self.dragging = false;
                self.begin_slide(self.open);
                true
            }
            Msg::Press { .. } | Msg::Release { .. } | Msg::Click { .. }
            | Msg::FocusIn { .. } | Msg::FocusOut { .. } => false,
        }
    }

    /// 吸附推进（帧循环；起拍 = 首次 tick）。
    pub fn tick<const N: usize>(&mut self, ui: &mut Ui<N>, now_ns: u64) -> bool {
        let Some(s) = &mut self.slide else { return false };
        let start = match s.start {
            Some(t) => t,
            None => {
                s.start = Some(now_ns);
                now_ns
            }
        };
        let elapsed = now_ns.saturating_sub(start);
        if elapsed >= SNAP_DUR_NS {
            let to = s.to;
            self.apply(ui, to);
            self.slide = None;
            return true;
        }
        let v = anim::interp(Easing::QuadOut, s.from, s.to, elapsed, SNAP_DUR_NS);
        self.apply(ui, v);
        true
    }

    /// 展开态。
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// 直落开/关（起吸附）。
    pub fn set_open<const N: usize>(&mut self, _ui: &mut Ui<N>, open: bool) {
        self.begin_slide(open);
    }

    fn begin_slide(&mut self, open: bool) {
        self.open = open;
        let to = if open { 0 } else { -self.w };
        if self.ox == to {
            self.slide = None;
            return;
        }
        self.slide = Some(Slide { from: self.ox, to, start: None });
    }

    fn apply<const N: usize>(&mut self, ui: &mut Ui<N>, ox: i32) {
        if ui.set_offset(self.panel, ox, 0).is_ok() {
            self.ox = ox;
        }
    }

    fn in_strip<const N: usize>(&self, ui: &Ui<N>, x: u32, y: u32) -> bool {
        in_bounds(ui, self.frame, x, y)
    }
}

/// 下拉刷新（源结构移植 + 主树 Refresh kind 锚直载：node 直载
/// refreshing/offset/on_refresh——拉动指示带 = kind 自绘（offset
/// permille 驱动）；frame = Stack 叠放帧（kind 锚满幅铺底 + 跟手指示
/// + 内容锚，内在 = 子 max）。主树 `add` 父位白名单未收编组件 kind
/// ——kind 锚不携子（「占位锚」技巧保留）；态位翻转走 `set_props`
/// 原位更新（源「kind 锚重挂」退役）。
pub struct Refresh {
    /// 叠放帧（Stack：kind 锚 + 指示 + 内容锚）。
    pub frame: NodeId,
    /// Refresh kind 锚（占位满幅铺底——指示带自绘）。
    pub node: NodeId,
    /// 下拉指示 Icon（藏于锚上缘外 oy=-48；拉动跟手露出）。
    pub indicator: NodeId,
    /// 内容锚（Col cross=Stretch——宿主内容）。
    pub content: NodeId,
    on_refresh: u32,
    refreshing: bool,
    pulling: bool,
    y0: u32,
    last_y: u32,
}

impl Refresh {
    /// 装配（指示器初位 = 锚上缘外）。
    pub fn attach<const N: usize>(
        ui: &mut Ui<N>,
        parent: NodeId,
        t: &Tokens,
        on_refresh: u32,
    ) -> Result<Refresh, UiError> {
        let frame = stack(ui, parent)?;
        let node = ui.add(
            frame,
            vspec(ViewKind::Refresh, Props::Refresh(RefreshProps { refreshing: false, offset: 0, on_refresh })),
        )?;
        let indicator = ui.add(frame, icon_spec(icons::REFRESH, t.brand, REFRESH_IND, REFRESH_IND))?;
        let _ = ui.set_offset(indicator, 0, -(REFRESH_IND as i32));
        let content = ui.add(
            frame,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch })),
        )?;
        Ok(Refresh { frame, node, indicator, content, on_refresh, refreshing: false, pulling: false, y0: 0, last_y: 0 })
    }

    /// Msg 消费（纵拖域内——几何命中 = frame bounds）。越阈
    /// （[`REFRESH_THRESH_PX`]）释放 → 触发刷新（kind `set_props`
    /// refreshing=true + offset=1000），返回 true；未越阈回弹。
    pub fn on_msg<const N: usize>(&mut self, ui: &mut Ui<N>, msg: &crate::desc::Msg) -> bool {
        use crate::desc::Msg;
        match msg {
            Msg::DragStart { axis, x, y, .. } => {
                if *axis != crate::desc::Axis::Y || !self.in_strip(ui, *x, *y) {
                    return false;
                }
                self.pulling = true;
                self.y0 = *y;
                self.last_y = *y;
                true
            }
            Msg::DragMove { y, .. } => {
                if !self.pulling {
                    return false;
                }
                self.last_y = *y;
                let dy = (*y as i32 - self.y0 as i32).max(0);
                let vis = dy.min(REFRESH_IND as i32); // 跟手露出（≤ 指示高）
                let _ = ui.set_offset(self.indicator, 0, vis - REFRESH_IND as i32);
                true
            }
            Msg::DragEnd { vy, .. } => {
                if !self.pulling {
                    return false;
                }
                self.pulling = false;
                let dy = self.last_y as i32 - self.y0 as i32;
                let fired = dy >= REFRESH_THRESH_PX as i32 || *vy >= FLIP_VEL_PX_S;
                if fired && !self.refreshing {
                    self.refreshing = true;
                    let _ = ui.set_offset(self.indicator, 0, 0); // 驻留全露
                    self.sync_kind(ui, 1000);
                } else {
                    let _ = ui.set_offset(self.indicator, 0, -(REFRESH_IND as i32)); // 回弹
                }
                fired
            }
            Msg::Cancel { .. } => {
                if !self.pulling {
                    return false;
                }
                self.pulling = false;
                let _ = ui.set_offset(self.indicator, 0, -(REFRESH_IND as i32));
                true
            }
            Msg::Press { .. } | Msg::Release { .. } | Msg::Click { .. }
            | Msg::FocusIn { .. } | Msg::FocusOut { .. } => false,
        }
    }

    /// 刷新完结（宿主拉取完成后调——kind `set_props` refreshing=false）。
    pub fn settle<const N: usize>(&mut self, ui: &mut Ui<N>) {
        if !self.refreshing {
            return;
        }
        self.refreshing = false;
        let _ = ui.set_offset(self.indicator, 0, -(REFRESH_IND as i32));
        self.sync_kind(ui, 0);
    }

    /// 刷新中谓词。
    pub fn is_refreshing(&self) -> bool {
        self.refreshing
    }

    /// 触发事件 id。
    pub fn on_refresh(&self) -> u32 {
        self.on_refresh
    }

    /// kind 态位同步（refreshing/offset permille——`set_props` 原位更
    /// 新：节点 id 稳定、指示/内容子树保持）。
    fn sync_kind<const N: usize>(&mut self, ui: &mut Ui<N>, offset: u32) {
        let refreshing = self.refreshing;
        let on_refresh = self.on_refresh;
        let _ = ui.set_props(
            self.node,
            Props::Refresh(RefreshProps { refreshing, offset, on_refresh }),
        );
    }

    fn in_strip<const N: usize>(&self, ui: &Ui<N>, x: u32, y: u32) -> bool {
        in_bounds(ui, self.frame, x, y)
    }
}

/// 几何命中小工具（节点 bounds 含 offset 的绝对界内判定——
/// SwipeItem/Sheet/Drawer/Refresh 共用）。
fn in_bounds<const N: usize>(ui: &Ui<N>, node: NodeId, x: u32, y: u32) -> bool {
    match ui.bounds(node) {
        Ok(b) => {
            let (x0, y0) = (b.x.max(0) as u32, b.y.max(0) as u32);
            let (x1, y1) = ((b.x + b.w).max(0) as u32, (b.y + b.h).max(0) as u32);
            x >= x0 && x < x1 && y >= y0 && y < y1
        }
        Err(_) => false,
    }
}

// ---------- 模式件单元测试 ----------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desc::Msg;
    use crate::theme::{ThemeMode, Tokens as Tk};
    use crate::UiMain;

    fn page(key: u32) -> NavPage {
        NavPage { key, title: [b'P'; 16], title_len: 1 }
    }

    /// 测试 token 组（light 档——主树 Tokens::pick 单源）。
    fn tok() -> Tokens {
        Tokens::pick(ThemeMode::Light)
    }

    fn host() -> (UiMain, NodeId) {
        let mut ui = UiMain::new(1080, 1920);
        let root = ui.root();
        (ui, root)
    }

    fn host_col(ui: &mut UiMain, root: NodeId) -> NodeId {
        ui.add(
            root,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch })),
        )
        .unwrap()
    }

    fn host_col_start(ui: &mut UiMain, root: NodeId) -> NodeId {
        ui.add(
            root,
            vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Start })),
        )
        .unwrap()
    }

    fn rect(bg: Color, w: Option<u32>, h: Option<u32>) -> ViewSpec {
        vspec(ViewKind::Rect, Props::Rect(RectProps { bg, w, h }))
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

    // ----- 结构三件 -----

    #[test]
    fn spacer_sizes_to_request() {
        let (mut ui, root) = host();
        let s = spacer(&mut ui, root, 120, 60).unwrap();
        ui.layout();
        let b = ui.bounds(s).unwrap();
        assert_eq!((b.w, b.h), (120, 60), "spacer 定尺寸");
        assert_eq!(ui.kind_of(s).unwrap(), ViewKind::Col, "空 Col 承载（无视觉）");
    }

    #[test]
    fn spacer_advances_col_cursor() {
        let (mut ui, root) = host();
        let col = host_col(&mut ui, root);
        let r1 = ui.add(col, rect(Color { r: 0, g: 0, b: 0 }, Some(100), Some(50))).unwrap();
        let _ = spacer(&mut ui, col, 1, 60).unwrap();
        let r2 = ui.add(col, rect(Color { r: 0, g: 0, b: 0 }, Some(100), Some(50))).unwrap();
        ui.layout();
        let b1 = ui.bounds(r1).unwrap();
        let b2 = ui.bounds(r2).unwrap();
        assert_eq!(b1.y, 0);
        assert_eq!(b2.y, 110, "spacer 主轴推进 50+60（无视觉占位）");
    }

    #[test]
    fn divider_kind_full_width_one_px() {
        let (mut ui, root) = host();
        let d = divider(&mut ui, root).unwrap();
        ui.layout();
        assert_eq!(ui.kind_of(d).unwrap(), ViewKind::Divider, "主树 Divider kind 直载");
        let b = ui.bounds(d).unwrap();
        assert_eq!((b.w, b.h), (1080, 1), "横缝宽随父、厚 1px（色单源 paint 面）");
    }

    #[test]
    fn divider_v_one_px_tall() {
        let (mut ui, root) = host();
        let v = divider_v(&mut ui, root, tok().divider).unwrap();
        ui.layout();
        assert_eq!(ui.kind_of(v).unwrap(), ViewKind::Rect, "竖缝 = Rect 组合（主树 v1 无纵线 kind 面）");
        let b = ui.bounds(v).unwrap();
        assert_eq!((b.w, b.h), (1, 1920), "竖缝厚 1px、高随父（Root 直挂 = 视口高）");
    }

    #[test]
    fn stack_overlays_children_at_same_origin() {
        let (mut ui, root) = host();
        let hostcol = ui
            .add(root, vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: Some(168), cross: CrossAlign::Stretch })))
            .unwrap();
        let st = stack(&mut ui, hostcol).unwrap();
        let under = ui.add(st, rect(Color { r: 255, g: 255, b: 255 }, None, None)).unwrap();
        let over = ui.add(st, rect(Color { r: 0, g: 0, b: 0 }, Some(320), Some(168))).unwrap();
        ui.layout();
        let bu = ui.bounds(under).unwrap();
        let bo = ui.bounds(over).unwrap();
        assert_eq!((bu.x, bu.y), (0, 0), "底层同起点");
        assert_eq!((bo.x, bo.y), (0, 0), "上层同起点（叠放而非顺排）");
        assert_eq!((bu.w, bu.h), (1080, 168), "底层吃满分配");
        assert_eq!((bo.w, bo.h), (320, 168), "上层显式尺寸");
    }

    #[test]
    fn stack_intrinsic_is_child_max() {
        let (mut ui, root) = host();
        let col = host_col_start(&mut ui, root);
        let st = stack(&mut ui, col).unwrap();
        ui.add(st, rect(Color { r: 0, g: 0, b: 0 }, Some(48), Some(48))).unwrap();
        ui.add(st, rect(Color { r: 0, g: 0, b: 0 }, None, None)).unwrap(); // 零内在面层不抬
        ui.layout();
        let bs = ui.bounds(st).unwrap();
        assert_eq!((bs.w, bs.h), (48, 48), "主树 Stack 内在 = 子 max（面层零内在不扰）");
    }

    // ----- AppBar / TopBar / Scaffold / NavHost -----

    #[test]
    fn app_bar_oh_dims_and_centering() {
        let (mut ui, root) = host();
        let ab = app_bar(&mut ui, root, &tok(), b"Settings", true).unwrap();
        ui.layout();
        let bf = ui.bounds(ab.frame).unwrap();
        assert_eq!((bf.w, bf.h), (1080, 168), "AppBar Mini 高 56vp、宽随父");
        let bt = ui.bounds(ab.title).unwrap();
        assert_eq!((bt.x, bt.y, bt.h), (144, 60, 48), "标题起始 = 24+24+72+24（lead+gap+back+gap）、S24x48 垂直居中");
        let bb = ui.bounds(ab.back.unwrap()).unwrap();
        assert_eq!((bb.x, bb.y, bb.w, bb.h), (48, 48, 72, 72), "返回键左上垂直居中");
    }

    #[test]
    fn app_bar_no_back_title_inset_48px() {
        let (mut ui, root) = host();
        let ab = app_bar(&mut ui, root, &tok(), b"Title", false).unwrap();
        ui.layout();
        let bt = ui.bounds(ab.title).unwrap();
        assert_eq!(bt.x, 48, "无返回键：标题起始内边距 16vp（48px）");
        assert!(ab.back.is_none());
    }

    #[test]
    fn top_bar_title_and_actions() {
        let (mut ui, root) = host();
        let tb = top_bar(&mut ui, root, &tok(), b"Inbox", &[icons::GRID, icons::SETTINGS]).unwrap();
        ui.layout();
        let bf = ui.bounds(tb.frame).unwrap();
        assert_eq!((bf.w, bf.h), (1080, 168), "TopBar 高 56vp");
        let bt = ui.bounds(tb.title).unwrap();
        assert_eq!(bt.x, 48, "标题起始 16vp");
        let a0 = ui.bounds(tb.actions[0].unwrap()).unwrap();
        let a1 = ui.bounds(tb.actions[1].unwrap()).unwrap();
        assert_eq!((a0.w, a0.h), (72, 72), "动作位 24vp 方寸");
        assert_eq!(a1.x - a0.x, 72 + 24, "动作位随标题顺排（缝 8vp——v1 无 flex）");
        assert!(tb.actions[2].is_none());
    }

    #[test]
    fn top_bar_actions_cap_loud() {
        let (mut ui, root) = host();
        assert!(
            top_bar(&mut ui, root, &tok(), b"T", &[1, 2, 3, 4]).is_err(),
            "动作位越 TOPBAR_ACTIONS_MAX → loud"
        );
    }

    #[test]
    fn scaffold_slots_stack_vertically() {
        let (mut ui, root) = host();
        let items = [
            NavItem { icon: icons::HOME, label: b"Home" },
            NavItem { icon: icons::SETTINGS, label: b"Set" },
        ];
        let sc = scaffold(&mut ui, root, &tok(), Some((b"App", true)), Some(&items)).unwrap();
        ui.layout();
        let top = ui.bounds(sc.top.unwrap().frame).unwrap();
        let content = ui.bounds(sc.content).unwrap();
        let bottom = ui.bounds(sc.bottom.unwrap().frame).unwrap();
        assert_eq!((top.y, top.h), (0, 168), "AppBar 槽在顶");
        assert_eq!(content.y, 168, "内容锚紧随 AppBar");
        assert_eq!((bottom.y, bottom.h), (168, 144), "BottomTabs 槽顺排其后（无 flex——骨架高 = 环架内在）");
    }

    #[test]
    fn scaffold_minimal_no_slots() {
        let (mut ui, root) = host();
        let sc = scaffold(&mut ui, root, &tok(), None, None).unwrap();
        ui.layout();
        assert!(sc.top.is_none());
        assert!(sc.bottom.is_none());
        let c = ui.bounds(sc.content).unwrap();
        assert_eq!(c.y, 0, "无槽：内容锚在顶");
    }

    #[test]
    fn nav_host_push_pop_cycle() {
        let (mut ui, root) = host();
        let mut nav = NavHost::new();
        let frame = nav.attach(&mut ui, root).unwrap();
        let p1 = ui
            .add(frame, vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: Some(1080), h: Some(400), cross: CrossAlign::Stretch })))
            .unwrap();
        nav.push(&mut ui, p1).unwrap();
        assert_eq!(nav.depth(), 1);
        assert_eq!(nav.top(), Some(p1));
        let b0 = ui.bounds(p1).unwrap();
        assert_eq!(b0.x, 1080, "入栈起点 = 视口右缘（OffsetX 滑入）");
        assert!(nav.tick(&mut ui, 0));
        assert!(nav.tick(&mut ui, NAV_ANIM_NS));
        let b1 = ui.bounds(p1).unwrap();
        assert_eq!(b1.x, 0, "过渡完结页落位 0");
        assert!(nav.pop(&mut ui));
        assert_eq!(nav.depth(), 0, "逻辑弹栈即时");
        assert!(nav.tick(&mut ui, 0), "出栈过渡起拍");
        assert!(nav.tick(&mut ui, NAV_ANIM_NS));
        assert_eq!(nav.take_popped(), Some(p1), "出栈完结移交宿主 remove");
        assert_eq!(nav.take_popped(), None, "移交幂等一次性");
    }

    #[test]
    fn nav_host_depth_cap_loud() {
        let (mut ui, root) = host();
        let mut nav = NavHost::new();
        let frame = nav.attach(&mut ui, root).unwrap();
        for _ in 0..NAV_DEPTH_MAX {
            let p = ui
                .add(frame, vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: Some(1080), h: Some(400), cross: CrossAlign::Stretch })))
                .unwrap();
            nav.push(&mut ui, p).unwrap();
        }
        let p9 = ui
            .add(frame, vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: Some(1080), h: Some(400), cross: CrossAlign::Stretch })))
            .unwrap();
        assert!(nav.push(&mut ui, p9).is_err(), "满栈 push → NodeFull loud（NAV_DEPTH_MAX=8 主树页深帽）");
    }

    // ----- 导航件 -----

    #[test]
    fn tab_bar_equal_tracks_and_active_tint() {
        let (mut ui, root) = host();
        let bar = tab_bar(&mut ui, root, &tok(), &[b"A", b"B", b"C"], 1).unwrap();
        ui.layout();
        let bf = ui.bounds(bar.frame).unwrap();
        assert_eq!((bf.w, bf.h), (1080, 144), "页签栏高 48vp、宽随父");
        let c0 = ui.bounds(bar.cells[0].unwrap().bg).unwrap();
        let c2 = ui.bounds(bar.cells[2].unwrap().bg).unwrap();
        assert_eq!((c0.x, c0.w), (0, 360), "Grid 等分轨");
        assert_eq!((c2.x, c2.w), (720, 360));
        let l0 = ui.bounds(bar.cells[0].unwrap().label).unwrap();
        assert_eq!((l0.x, l0.y), (168, 48), "单字标签水平/垂直居中");
        assert_eq!(bar.active, 1);
    }

    #[test]
    fn tab_bar_set_active_restyle_no_relayout() {
        let (mut ui, root) = host();
        let mut bar = tab_bar(&mut ui, root, &tok(), &[b"A", b"B"], 0).unwrap();
        ui.layout();
        let before = ui.bounds(bar.cells[0].unwrap().label).unwrap();
        assert!(bar.set_active(&mut ui, 1));
        let stat = ui.layout();
        let after = ui.bounds(bar.cells[0].unwrap().label).unwrap();
        assert_eq!(before, after, "纯着色零重摆（同几何 set_props）");
        assert_eq!(stat.measured, 0, "measured==0 面（零重排）");
        assert_eq!(bar.active, 1);
        assert!(!bar.set_active(&mut ui, 1), "同档 no-op");
        assert!(!bar.set_active(&mut ui, 9), "域外拒");
    }

    #[test]
    fn seg_equal_segments_highlight() {
        let (mut ui, root) = host();
        let mut s = seg(&mut ui, root, &tok(), &[b"X", b"Y"], 0).unwrap();
        ui.layout();
        let f = ui.bounds(s.frame).unwrap();
        assert_eq!((f.w, f.h), (1080, 96), "Seg 高 32vp（拟定）");
        let c0 = ui.bounds(s.cells[0].unwrap().bg).unwrap();
        assert_eq!((c0.x, c0.w), (0, 539), "段宽 = (1080−缝 2)/2 向下取整");
        assert!(s.set_active(&mut ui, 1));
        assert_eq!(s.active, 1);
        assert!(!s.set_active(&mut ui, 1), "同档 no-op");
    }

    #[test]
    fn seg_hit_and_domain_loud() {
        let (mut ui, root) = host();
        let s = seg(&mut ui, root, &tok(), &[b"X", b"Y"], 0).unwrap();
        assert_eq!(s.hit(s.cells[1].unwrap().label), Some(1), "label 锚可命中");
        assert_eq!(s.hit(NodeId(9999)), None, "域外锚不命中");
        assert!(seg(&mut ui, root, &tok(), &[], 0).is_err(), "零段 loud");
        let many: [&[u8]; 9] = [b"a"; 9];
        assert!(seg(&mut ui, root, &tok(), &many, 0).is_err(), "越 TABS_MAX loud");
    }

    #[test]
    fn step_marks_and_connectors() {
        let (mut ui, root) = host();
        let col = host_col_start(&mut ui, root);
        let st = step(&mut ui, col, &tok(), 3, 1).unwrap();
        ui.layout();
        let f = ui.bounds(st.frame).unwrap();
        assert_eq!((f.w, f.h), (336, 48), "3×48 步点 + 2×48 连线 + 4×24 gap");
        let m0 = ui.bounds(st.marks[0].unwrap()).unwrap();
        let m1 = ui.bounds(st.marks[1].unwrap()).unwrap();
        assert_eq!((m0.x, m0.w), (0, 48), "完成步 = 勾 Icon（显式 48×48）");
        assert_eq!((m1.x, m1.w), (144, 48), "当前步 = brand Rect（48+24+48+24 起）");
    }

    #[test]
    fn step_domain_loud() {
        let (mut ui, root) = host();
        assert!(step(&mut ui, root, &tok(), 0, 0).is_err(), "零步 loud");
        assert!(step(&mut ui, root, &tok(), STEP_MAX + 1, 0).is_err(), "越 STEP_MAX loud");
    }

    #[test]
    fn bottom_tabs_icon_label_cell() {
        let (mut ui, root) = host();
        let items = [
            NavItem { icon: icons::HOME, label: b"Home" },
            NavItem { icon: icons::SETTINGS, label: b"Set" },
        ];
        let mut bn = bottom_tabs(&mut ui, root, &tok(), &items, 0).unwrap();
        ui.layout();
        let f = ui.bounds(bn.frame).unwrap();
        assert_eq!((f.w, f.h), (1080, 144), "底部页签高 48vp");
        let c0 = bn.cells[0].unwrap();
        let icon = ui.bounds(c0.icon.unwrap()).unwrap();
        assert_eq!((icon.x, icon.y, icon.w, icon.h), (234, 16, 72, 72), "icon 水平居中、顶垫 16");
        let label = ui.bounds(c0.label).unwrap();
        assert_eq!((label.y, label.h), (96, 32), "小字 S16x32 紧随 icon（16+72+8）");
        assert!(bn.set_active(&mut ui, 1));
        assert_eq!(bn.active, 1);
        assert!(bn.hit(c0.icon.unwrap()).is_some(), "icon 锚可命中");
    }

    #[test]
    fn bottom_tabs_empty_items_loud() {
        let (mut ui, root) = host();
        assert!(bottom_tabs(&mut ui, root, &tok(), &[], 0).is_err(), "零项 loud");
    }

    // ----- Card / ListItem / Collapse / SwipeItem / DragReorder -----

    #[test]
    fn card_kind_container_dims_and_insets() {
        let (mut ui, root) = host();
        let list = host_col(&mut ui, root);
        let c = card(&mut ui, list, &tok(), 1, Some(icons::GRID)).unwrap();
        ui.layout();
        assert_eq!(ui.kind_of(c.node).unwrap(), ViewKind::Card, "主树 Card kind 容器直用（面底/阴影 = kind 自绘）");
        let bf = ui.bounds(c.node).unwrap();
        assert_eq!((bf.w, bf.h), (1080, 144), "CARD 高 48vp、宽 100% 随父");
        let cb = ui.bounds(c.content).unwrap();
        assert_eq!(cb.x, 48 + 72, "内容锚起点 = 左内边 48（16vp 主树律）+ 前导图标 72");
        assert_eq!(c.state, ItemState::Normal, "五态默认 = 主树查表 Normal 档");
    }

    #[test]
    fn list_item_row_variant_and_elevation_loud() {
        let (mut ui, root) = host();
        let list = host_col(&mut ui, root);
        let c = card(&mut ui, list, &tok(), 0, None).unwrap();
        let li = list_item(&mut ui, list, &tok()).unwrap();
        ui.layout();
        let lf = ui.bounds(li.node).unwrap();
        assert_eq!((lf.w, lf.h, lf.y), (1080, 144, 144), "行变体顺排其下（Col 游标）");
        let lb = ui.bounds(li.content).unwrap();
        assert_eq!(lb.x, 48, "无前导图标：内容锚 = 左内边 48px");
        let _ = c;
        assert!(card(&mut ui, list, &tok(), 4, None).is_err(), "elevation > 3 → add 面 widget_value_ok loud（值域单源）");
    }

    #[test]
    fn collapse_container_show_hide_and_stable_id() {
        let (mut ui, root) = host();
        let mut col = collapse(&mut ui, root, false, 7).unwrap();
        assert!(!col.is_open());
        assert_eq!(ui.kind_of(col.node).unwrap(), ViewKind::Collapse, "Collapse kind 容器直载");
        // body 内容 1080×100。
        ui.add(col.body, rect(Color { r: 0, g: 0, b: 0 }, Some(1080), Some(100))).unwrap();
        ui.layout();
        let b_closed = ui.bounds(col.node).unwrap();
        assert_eq!((b_closed.w, b_closed.h), (1080, 32), "收起 = header 带 32px（子三面守卫折叠）");
        let old = col.node;
        assert!(col.set_open(&mut ui, true));
        assert!(col.is_open());
        assert_eq!(col.node, old, "set_props 原位翻转——节点 id 稳定（源重挂技巧退役）");
        assert_eq!(ui.kind_of(col.node).unwrap(), ViewKind::Collapse);
        ui.layout();
        let b_open = ui.bounds(col.node).unwrap();
        assert_eq!(b_open.h, 32 + 100, "展开 = header 32 + body Σ");
        let body_child = ui.bounds(col.body).unwrap();
        assert_eq!(body_child.y, 32, "body 锚在 header 带下方");
        assert!(!col.set_open(&mut ui, true), "同态 no-op");
        assert!(col.toggle(&mut ui));
        assert!(!col.is_open());
    }

    #[test]
    fn collapse_open_attach_body_visible() {
        let (mut ui, root) = host();
        let col = collapse(&mut ui, root, true, 1).unwrap();
        let inner = ui.add(col.body, rect(Color { r: 0, g: 0, b: 0 }, Some(1080), Some(60))).unwrap();
        ui.layout();
        assert!(col.is_open());
        let b = ui.bounds(inner).unwrap();
        assert_eq!((b.y, b.h), (32, 60), "展开态挂 body 内容在 header 下可见");
    }

    #[test]
    fn swipe_item_reveal_snap_and_kind_stable() {
        let (mut ui, root) = host();
        let mut si = SwipeItem::attach(&mut ui, root, &tok(), 1080, 2, 9).unwrap();
        ui.add(si.content, rect(Color { r: 255, g: 255, b: 255 }, Some(1080), Some(144)))
            .unwrap();
        assert_eq!(ui.kind_of(si.node).unwrap(), ViewKind::SwipeItem, "SwipeItem kind 锚 = 叠放帧合一");
        ui.layout();
        let a0 = ui.bounds(si.actions).unwrap();
        assert_eq!(a0.x, 1080, "动作层初始藏于右缘外");
        let f = si.node;
        assert!(si.on_msg(&mut ui, &Msg::DragStart { node: f, axis: crate::desc::Axis::X, x: 500, y: 72 }));
        assert!(si.on_msg(&mut ui, &Msg::DragMove { node: f, x: 300, y: 72, resampled: false }));
        let a1 = ui.bounds(si.actions).unwrap();
        assert_eq!(a1.x, 880, "跟手：ox = 1080 + (300−500)");
        let old = si.node;
        assert!(si.on_msg(&mut ui, &Msg::DragEnd { node: f, vx: 0, vy: 0 }));
        assert_eq!(si.node, old, "set_props 原位同步 reveal_end——节点 id 稳定");
        assert!(si.tick(&mut ui, 0));
        assert!(si.tick(&mut ui, SNAP_DUR_NS));
        let a2 = ui.bounds(si.actions).unwrap();
        assert_eq!(a2.x, 1080 - 288, "dx 200 越阈（115）→ 吸附展开（右缘 96vp 露出）");
        assert!(si.is_open());
        si.set_open(&mut ui, false);
        assert!(si.tick(&mut ui, 0));
        assert!(si.tick(&mut ui, SNAP_DUR_NS));
        let a3 = ui.bounds(si.actions).unwrap();
        assert_eq!(a3.x, 1080, "收起回藏");
        assert!(!si.is_open());
    }

    #[test]
    fn swipe_item_cancel_snaps_home() {
        let (mut ui, root) = host();
        let mut si = SwipeItem::attach(&mut ui, root, &tok(), 1080, 1, 1).unwrap();
        ui.add(si.content, rect(Color { r: 255, g: 255, b: 255 }, Some(1080), Some(144)))
            .unwrap();
        ui.layout();
        let f = si.node;
        assert!(si.on_msg(&mut ui, &Msg::DragStart { node: f, axis: crate::desc::Axis::X, x: 500, y: 72 }));
        assert!(si.on_msg(&mut ui, &Msg::DragMove { node: f, x: 400, y: 72, resampled: false }));
        assert!(si.on_msg(&mut ui, &Msg::Cancel { node: f }));
        assert!(si.tick(&mut ui, 0));
        assert!(si.tick(&mut ui, SNAP_DUR_NS));
        let a = ui.bounds(si.actions).unwrap();
        assert_eq!(a.x, 1080, "Cancel → 吸附回原位");
        assert!(!si.is_open());
    }

    #[test]
    fn drag_reorder_visual_swap_order_and_handles() {
        let (mut ui, root) = host();
        let mut dr = DragReorder::attach(&mut ui, root, 144).unwrap();
        let mut ids = [NodeId(0); 3];
        for i in 0..3 {
            ids[i] = dr
                .add_item(&mut ui, rect(Color { r: 40, g: 40, b: 40 }, Some(1080), Some(144)), 5)
                .unwrap();
        }
        assert_eq!(dr.len(), 3);
        dr.sync_totals(&mut ui);
        for h in dr.handles.iter().take(3) {
            assert_eq!(ui.kind_of(h.unwrap()).unwrap(), ViewKind::DragReorder, "kind 手柄锚在位");
        }
        ui.layout();
        let c = dr.container;
        assert!(dr.on_msg(&mut ui, &Msg::DragStart { node: c, axis: crate::desc::Axis::Y, x: 100, y: 72 }));
        assert!(dr.on_msg(&mut ui, &Msg::DragMove { node: c, x: 100, y: 252, resampled: false }));
        assert!(dr.on_msg(&mut ui, &Msg::DragEnd { node: c, vx: 0, vy: 0 }));
        let b0 = ui.bounds(ids[0]).unwrap();
        let b1 = ui.bounds(ids[1]).unwrap();
        assert_eq!((b0.x, b0.y), (0, 144), "项 0 视觉落槽 1");
        assert_eq!((b1.x, b1.y), (0, 0), "项 1 逆步进槽 0");
        let mut order = [0usize; 3];
        assert_eq!(dr.order(&mut order), 3);
        assert_eq!(order, [1, 0, 2], "槽序快照");
        dr.reset_offsets(&mut ui);
        let b0r = ui.bounds(ids[0]).unwrap();
        assert_eq!(b0r.y, 0, "复位清偏移");
    }

    #[test]
    fn drag_reorder_cap_and_handle_geometry() {
        let (mut ui, root) = host();
        let mut dr = DragReorder::attach(&mut ui, root, 144).unwrap();
        for _ in 0..REORDER_MAX {
            dr.add_item(&mut ui, rect(Color { r: 40, g: 40, b: 40 }, Some(1080), Some(144)), 1)
                .unwrap();
        }
        assert!(
            dr.add_item(&mut ui, rect(Color { r: 0, g: 0, b: 0 }, Some(108), Some(144)), 1).is_err(),
            "满 REORDER_MAX → NodeFull loud"
        );
        dr.sync_totals(&mut ui);
        ui.layout();
        for (i, h) in dr.handles.iter().take(REORDER_MAX).enumerate() {
            let b = ui.bounds(h.unwrap()).unwrap();
            assert_eq!((b.w, b.h), (24, 36), "手柄锚 i={} 主树内在 24×36（行格 max 吸收不扰行高）", i);
        }
        assert_eq!(ui.bounds(dr.container).unwrap().h, 144 * REORDER_MAX as i32, "行高不被手柄抬升");
    }

    // ----- Chip / Badge / Qr / Fab / IconButton / Gauge -----

    #[test]
    fn chip_kind_intrinsic_and_set_selected() {
        let (mut ui, root) = host();
        let col = host_col_start(&mut ui, root);
        let c = chip(&mut ui, col, b"OK", false, 3).unwrap();
        assert_eq!(ui.kind_of(c.node).unwrap(), ViewKind::Chip, "Chip kind 直载（胶囊视觉 = kind 自绘）");
        ui.layout();
        let b = ui.bounds(c.node).unwrap();
        assert_eq!((b.w, b.h), (2 * 8 + 16, 32), "主树内在 = 标签×8 + 16 留白 × 32 高");
        assert_eq!(c.state, ItemState::Normal);
        // set_props 原位翻转（源「重挂」退役）。
        let c2 = chip(&mut ui, col, b"Go", false, 4).unwrap();
        let old = c2.node;
        ui.set_props(old, Props::Chip(ChipProps { label: TextBuf::new(b"Go").unwrap(), selected: true, on_tap: 4 })).unwrap();
        assert_eq!(c2.node, old, "选中翻转节点 id 稳定");
        assert_eq!(ui.kind_of(old).unwrap(), ViewKind::Chip);
    }

    #[test]
    fn chip_label_too_long_loud() {
        let (mut ui, root) = host();
        let long = [b'x'; crate::TEXT_INLINE + 1];
        assert!(chip(&mut ui, root, &long, false, 1).is_err(), "标签越 TEXT_INLINE → TextTooLong loud");
    }

    #[test]
    fn badge_kind_counts_and_cap() {
        let (mut ui, root) = host();
        let anchor = stack(&mut ui, root).unwrap();
        let b3 = badge(&mut ui, anchor, 3).unwrap();
        let b999 = badge(&mut ui, anchor, 999).unwrap();
        ui.layout();
        assert_eq!(ui.kind_of(b3).unwrap(), ViewKind::Badge, "主树 Badge kind 直载（源 Pad+Text 组合退役）");
        let b = ui.bounds(b3).unwrap();
        assert_eq!((b.w, b.h), (1 * 8 + 8, 16), "一位数角标 = 位档×8 + 8 留白 × 16 高");
        let b9 = ui.bounds(b999).unwrap();
        assert_eq!(b9.w, 3 * 8 + 8, "「99+」三位档封顶");
    }

    #[test]
    fn badge_zero_hidden() {
        let (mut ui, root) = host();
        let anchor = stack(&mut ui, root).unwrap();
        let b0 = badge(&mut ui, anchor, 0).unwrap();
        ui.layout();
        let b = ui.bounds(b0).unwrap();
        assert_eq!((b.w, b.h), (0, 0), "count=0 隐藏（kind 0×0 空绘——源 Ok(None) 不建树形态退役）");
    }

    #[test]
    fn qr_show_domain_loud() {
        let (mut ui, root) = host();
        assert!(qr_show(&mut ui, root, 22, 0, 0xffffff).is_err(), "module 22 域外 loud（add 面值域单源）");
        assert!(qr_show(&mut ui, root, 181, 0, 0xffffff).is_err(), "module 181 域外 loud");
        assert!(qr_show(&mut ui, root, 23, 0, 0).is_err(), "21+4k 之外的 23 loud");
        assert!(qr_show(&mut ui, root, 25, 0, 0).is_ok(), "版本 2（25 模块）合法");
    }

    #[test]
    fn qr_show_kind_and_module_size() {
        let (mut ui, root) = host();
        let col = host_col_start(&mut ui, root);
        let q = qr_show(&mut ui, col, 21, 0x000000ff, 0xffffffff).unwrap();
        assert_eq!(ui.kind_of(q).unwrap(), ViewKind::Qr, "Qr kind 直载");
        ui.layout();
        let b = ui.bounds(q).unwrap();
        assert_eq!((b.w, b.h), (21 * 4, 21 * 4), "点阵方阵 = module×4 px");
    }

    #[test]
    fn fab_kind_intrinsic_forms() {
        let (mut ui, root) = host();
        let col = host_col_start(&mut ui, root);
        let f = fab(&mut ui, col, icons::ADD as u32, b"Add", 2).unwrap();
        let f_solo = fab(&mut ui, col, 1, b"", 3).unwrap();
        assert_eq!(ui.kind_of(f.node).unwrap(), ViewKind::Fab, "Fab kind 直载");
        assert_eq!(f.state, ItemState::Normal);
        ui.layout();
        let b = ui.bounds(f.node).unwrap();
        assert_eq!((b.w, b.h), (56 + 8 + 3 * 8, 56), "扩展形态 = 56 圆 + 8 间隙 + 标签×8");
        let bs = ui.bounds(f_solo.node).unwrap();
        assert_eq!((bs.w, bs.h), (56, 56), "空标签 = 纯 56 圆形");
    }

    #[test]
    fn icon_button_small_form_and_fab_label_loud() {
        let (mut ui, root) = host();
        let col = host_col_start(&mut ui, root);
        let ib = icon_button(&mut ui, col, icons::BACK as u32, 3).unwrap();
        assert_eq!(ui.kind_of(ib.node).unwrap(), ViewKind::Fab, "IconButton = Fab kind 小档（props 无尺寸档——诚实标注）");
        ui.layout();
        let b = ui.bounds(ib.node).unwrap();
        assert_eq!((b.w, b.h), (56, 56), "空扩展标签 = 纯圆形");
        let long = [b'x'; crate::TEXT_INLINE + 1];
        assert!(fab(&mut ui, root, 1, &long, 1).is_err(), "扩展标签越长 loud");
    }

    #[test]
    fn gauge_domain_loud_and_kind() {
        let (mut ui, root) = host();
        assert!(gauge(&mut ui, root, 1001, GaugeStyle::Ring).is_err(), "permille > 1000 loud（add 面值域单源）");
        let g = gauge(&mut ui, root, 750, GaugeStyle::Arc).unwrap();
        assert_eq!(ui.kind_of(g.node).unwrap(), ViewKind::Gauge, "Gauge kind 直载（Ring/Bar/Arc 三档）");
        let g0 = gauge(&mut ui, root, 0, GaugeStyle::Bar).unwrap();
        let g1 = gauge(&mut ui, root, 1000, GaugeStyle::Ring).unwrap();
        ui.layout();
        assert_eq!(ui.bounds(g0.node).unwrap(), ui.bounds(g1.node).unwrap(), "域界值合法同挂");
        assert_eq!(g.state, ItemState::Normal);
    }

    #[test]
    fn gauge_intrinsic_square() {
        let (mut ui, root) = host();
        let col = host_col_start(&mut ui, root);
        let g = gauge(&mut ui, col, 500, GaugeStyle::Ring).unwrap();
        ui.layout();
        let b = ui.bounds(g.node).unwrap();
        assert_eq!((b.w, b.h), (64, 64), "主树 Gauge 内在 64×64 表盘方格");
    }

    // ----- Sheet / Drawer / Refresh -----

    #[test]
    fn sheet_attach_closed_open_cycle() {
        let (mut ui, root) = host();
        let mut sh = Sheet::attach(&mut ui, root, &tok(), 1080, 400).unwrap();
        ui.layout();
        let p0 = ui.bounds(sh.panel).unwrap();
        assert_eq!(p0.y, 400, "closed：面板顶 = 锚区下缘（oy = 面板高）");
        assert_eq!((p0.w, p0.h), (1080, 400), "面板显式尺寸");
        assert!(!sh.is_open());
        sh.set_open(&mut ui, true);
        assert!(sh.tick(&mut ui, 0));
        assert!(sh.tick(&mut ui, SNAP_DUR_NS));
        let p1 = ui.bounds(sh.panel).unwrap();
        assert_eq!(p1.y, 0, "open：面板滑入锚区（oy = 0）");
        assert!(sh.is_open());
        // 下拉 200px（阈 400×40% = 160）→ 关。
        assert!(sh.on_msg(&mut ui, &Msg::DragStart { node: sh.panel, axis: crate::desc::Axis::Y, x: 100, y: 50 }));
        assert!(sh.on_msg(&mut ui, &Msg::DragMove { node: sh.panel, x: 100, y: 250, resampled: false }));
        assert!(sh.on_msg(&mut ui, &Msg::DragEnd { node: sh.panel, vx: 0, vy: 0 }));
        assert!(sh.tick(&mut ui, 0));
        assert!(sh.tick(&mut ui, SNAP_DUR_NS));
        let p2 = ui.bounds(sh.panel).unwrap();
        assert_eq!(p2.y, 400, "下拉越阈吸附关闭");
        assert!(!sh.is_open());
    }

    #[test]
    fn sheet_drag_clamped_to_height() {
        let (mut ui, root) = host();
        let mut sh = Sheet::attach(&mut ui, root, &tok(), 1080, 400).unwrap();
        ui.layout();
        sh.set_open(&mut ui, true);
        assert!(sh.tick(&mut ui, 0));
        assert!(sh.tick(&mut ui, SNAP_DUR_NS));
        // 上滑越顶（dy = 50−300 = -250）→ oy 钳 0 不越。
        assert!(sh.on_msg(&mut ui, &Msg::DragStart { node: sh.panel, axis: crate::desc::Axis::Y, x: 100, y: 300 }));
        assert!(sh.on_msg(&mut ui, &Msg::DragMove { node: sh.panel, x: 100, y: 50, resampled: false }));
        let p = ui.bounds(sh.panel).unwrap();
        assert_eq!(p.y, 0, "oy 钳 [0, h]——上滑越顶不出界");
    }

    #[test]
    fn drawer_attach_closed_open_cycle() {
        let (mut ui, root) = host();
        let mut dr = Drawer::attach(&mut ui, root, &tok(), 288, 800).unwrap();
        ui.layout();
        let p0 = ui.bounds(dr.panel).unwrap();
        assert_eq!(p0.x, -288, "closed：面板藏于左缘外（ox = -w）");
        assert_eq!((p0.w, p0.h), (288, 800), "面板显式尺寸");
        assert_eq!(ui.kind_of(dr.content).unwrap(), ViewKind::Col, "内容锚在下层");
        assert!(!dr.is_open());
        dr.set_open(&mut ui, true);
        assert!(dr.tick(&mut ui, 0));
        assert!(dr.tick(&mut ui, SNAP_DUR_NS));
        let p1 = ui.bounds(dr.panel).unwrap();
        assert_eq!(p1.x, 0, "open：面板滑入贴左缘");
        assert!(dr.is_open());
    }

    #[test]
    fn drawer_edge_gesture_gate_and_close() {
        let (mut ui, root) = host();
        let mut dr = Drawer::attach(&mut ui, root, &tok(), 288, 800).unwrap();
        ui.layout();
        // 闭合态：起拖点越 DRAWER_EDGE_PX（48）不消费。
        assert!(!dr.on_msg(&mut ui, &Msg::DragStart { node: dr.frame, axis: crate::desc::Axis::X, x: 200, y: 100 }));
        // 左缘起拖（x=20）右滑 200px（阈 288×40% = 115）→ 开。
        assert!(dr.on_msg(&mut ui, &Msg::DragStart { node: dr.frame, axis: crate::desc::Axis::X, x: 20, y: 100 }));
        assert!(dr.on_msg(&mut ui, &Msg::DragMove { node: dr.frame, x: 220, y: 100, resampled: false }));
        let pm = ui.bounds(dr.panel).unwrap();
        assert_eq!(pm.x, -88, "跟手：ox = -288 + 200");
        assert!(dr.on_msg(&mut ui, &Msg::DragEnd { node: dr.frame, vx: 0, vy: 0 }));
        assert!(dr.tick(&mut ui, 0));
        assert!(dr.tick(&mut ui, SNAP_DUR_NS));
        let p1 = ui.bounds(dr.panel).unwrap();
        assert_eq!(p1.x, 0, "右滑越阈吸附开");
        assert!(dr.is_open());
        // 展开态全域可拖：左滑 200px 关。
        assert!(dr.on_msg(&mut ui, &Msg::DragStart { node: dr.frame, axis: crate::desc::Axis::X, x: 250, y: 100 }));
        assert!(dr.on_msg(&mut ui, &Msg::DragMove { node: dr.frame, x: 50, y: 100, resampled: false }));
        assert!(dr.on_msg(&mut ui, &Msg::DragEnd { node: dr.frame, vx: 0, vy: 0 }));
        assert!(dr.tick(&mut ui, 0));
        assert!(dr.tick(&mut ui, SNAP_DUR_NS));
        let p2 = ui.bounds(dr.panel).unwrap();
        assert_eq!(p2.x, -288, "左滑越阈吸附关");
        assert!(!dr.is_open());
    }

    #[test]
    fn refresh_pull_indicator_and_fire() {
        let (mut ui, root) = host();
        let mut rf = Refresh::attach(&mut ui, root, &tok(), 42).unwrap();
        assert_eq!(ui.kind_of(rf.node).unwrap(), ViewKind::Refresh, "Refresh kind 锚 = 叠放帧合一");
        ui.add(rf.content, rect(Color { r: 255, g: 255, b: 255 }, Some(1080), Some(800)))
            .unwrap();
        ui.layout();
        let i0 = ui.bounds(rf.indicator).unwrap();
        assert_eq!(i0.y, -48, "指示器初藏于锚上缘外");
        let f = rf.node;
        // 拉动 60px：跟手露出 60（≥ 指示高 48 → 全露）。
        assert!(rf.on_msg(&mut ui, &Msg::DragStart { node: f, axis: crate::desc::Axis::Y, x: 100, y: 200 }));
        assert!(rf.on_msg(&mut ui, &Msg::DragMove { node: f, x: 100, y: 260, resampled: false }));
        let i1 = ui.bounds(rf.indicator).unwrap();
        assert_eq!(i1.y, 0, "拉动跟手露出（60 ≥ 48 全露）");
        // 未越阈（60 < 96）回弹。
        assert!(!rf.on_msg(&mut ui, &Msg::DragEnd { node: f, vx: 0, vy: 0 }), "60px 未越阈不触发");
        let i2 = ui.bounds(rf.indicator).unwrap();
        assert_eq!(i2.y, -48, "回弹藏回");
        assert!(!rf.is_refreshing());
        // 拉动 150px 越阈 → 触发。
        assert!(rf.on_msg(&mut ui, &Msg::DragStart { node: f, axis: crate::desc::Axis::Y, x: 100, y: 200 }));
        assert!(rf.on_msg(&mut ui, &Msg::DragMove { node: f, x: 100, y: 350, resampled: false }));
        let old = rf.node;
        assert!(rf.on_msg(&mut ui, &Msg::DragEnd { node: f, vx: 0, vy: 0 }), "150px 越阈触发");
        assert!(rf.is_refreshing());
        assert_eq!(rf.node, old, "set_props 原位同步（refreshing=true/offset=1000）——节点 id 稳定");
        assert_eq!(ui.kind_of(rf.node).unwrap(), ViewKind::Refresh);
        assert_eq!(rf.on_refresh(), 42, "触发事件 id 透出");
        let i3 = ui.bounds(rf.indicator).unwrap();
        assert_eq!(i3.y, 0, "刷新驻留指示全露");
        rf.settle(&mut ui);
        assert!(!rf.is_refreshing(), "完结复位");
        let i4 = ui.bounds(rf.indicator).unwrap();
        assert_eq!(i4.y, -48, "完结指示藏回");
    }

    #[test]
    fn refresh_not_in_strip_rejects() {
        let (mut ui, root) = host();
        // 有界宿主（Col h=Some(360)）：frame 内在 = 子 max（内容 300 ≥
        // 指示 48——指示藏于上缘外不抬内容高）——strip 以 frame bounds
        // 为准。
        let col = ui
            .add(root, vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: None, h: Some(360), cross: CrossAlign::Stretch })))
            .unwrap();
        let mut rf = Refresh::attach(&mut ui, col, &tok(), 1).unwrap();
        ui.add(rf.content, rect(Color { r: 255, g: 255, b: 255 }, Some(1080), Some(300)))
            .unwrap();
        ui.layout();
        assert_eq!(ui.bounds(rf.frame).unwrap().h, 300, "Stack 帧内在 = 子 max（指示不抬内容高）");
        // 横拖不消费。
        assert!(!rf.on_msg(&mut ui, &Msg::DragStart { node: rf.frame, axis: crate::desc::Axis::X, x: 100, y: 100 }));
        // 纵拖但 y 在 strip 外（y=400 > 300）不消费。
        assert!(!rf.on_msg(&mut ui, &Msg::DragStart { node: rf.frame, axis: crate::desc::Axis::Y, x: 100, y: 400 }));
    }

    // ----- 五态默认（主树 ItemState 查表——扩展后形态） -----

    #[test]
    fn recipe_state_defaults_item_state_table() {
        let (mut ui, root) = host();
        let t = &tok();
        let items = [NavItem { icon: icons::HOME, label: b"Home" }];
        let bar = tab_bar(&mut ui, root, t, &[b"A", b"B"], 0).unwrap();
        let s = seg(&mut ui, root, t, &[b"X"], 0).unwrap();
        let c = card(&mut ui, root, t, 0, None).unwrap();
        let ch = chip(&mut ui, root, b"OK", false, 1).unwrap();
        let f = fab(&mut ui, root, 1, b"", 1).unwrap();
        let g = gauge(&mut ui, root, 500, GaugeStyle::Ring).unwrap();
        let bt = bottom_tabs(&mut ui, root, t, &items, 0).unwrap();
        for st in [bar.state, s.state, c.state, ch.state, f.state, g.state, bt.state] {
            assert_eq!(st, ItemState::Normal, "交互件五态默认 = 查表 Normal 档");
        }
        // 扩展后查表形态：Disabled 档在表（XA paint 侧消费面挂落地）。
        assert_eq!(ItemState::Disabled.bg(), 0xF2_F2_F2);
        assert_ne!(ItemState::Disabled.bg(), ItemState::Normal.bg());
    }

    #[test]
    fn dim_accent_follows_tokens_both_themes() {
        let l = dim_accent(Tokens::pick(ThemeMode::Light));
        let d = dim_accent(Tokens::pick(ThemeMode::Dark));
        // light：brand(0,125,255) 上 bg_page(241,243,245) 12% = (28,139,253)。
        assert_eq!(l, Color { r: 28, g: 139, b: 253 });
        assert_ne!(l, d, "派生随入参 token——dark 档各得其值（硬预混单档不可达）");
        assert_ne!(d, Tokens::pick(ThemeMode::Dark).brand, "dim 档与原 brand 可区分");
    }

    // ----- 引入面整洁（未用 import 不落） -----

    #[test]
    fn tokens_type_alias_visible() {
        // 编译面自证：Tokens 经本模块 use 可达（主题适配 API 形态钉板）。
        let t: Tk = Tokens::pick(ThemeMode::Light);
        assert_eq!(t.brand, crate::theme::BRAND);
    }
}

/// BottomTabs 页位（图标 + 标签竖排；选中态高亮）。
#[derive(Debug, Clone, Copy)]
pub struct TabSlot {
    /// 图标资产 id（IconSource 消费）。
    pub icon: u16,
    /// 标签文本（ASCII）。
    pub label: &'static [u8],
    /// 选中态（true = brand 色标签 + 顶部指示条）。
    pub selected: bool,
}

/// BottomTabs 建树：满宽 48vp 高底条 + N 页位横排（图标 24vp + 标签
/// 12vp 竖排，选中页 label 染 brand 色 + 2vp 顶指示条）。
/// 返回页位节点序（消费面挂 a11y/命中）。
pub fn bottom_tabs_slots(
    ui: &mut UiMain,
    parent: NodeId,
    slots: &[TabSlot],
) -> Result<[NodeId; 8], UiError> {
    let (vp_w, _) = ui.viewport();
    // 底条：白底（非沉浸桌面页底色）+ 满宽 144px 高。
    ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps {
                bg: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                w: Some(vp_w),
                h: Some(BOTTOMTABS_H),
            }),
        },
    )?;
    let row = ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(vp_w),
                h: Some(BOTTOMTABS_H),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let mut nodes = [NodeId(0); 8];
    for (i, t) in slots.iter().enumerate() {
        // 页位 = 竖排（图标上、标签下），等宽分摊（flex 由消费面给 w）。
        let cell = ui.add(
            row,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 2,
                    w: Some(vp_w / slots.len() as u32),
                    h: Some(BOTTOMTABS_H),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        // 选中指示条：顶 2vp 满格宽 brand 色横条。
        if t.selected {
            ui.add(
                cell,
                ViewSpec {
                    kind: ViewKind::Rect,
                    key: 0,
                    props: Props::Rect(RectProps {
                        bg: Color { r: 0x00, g: 0x7D, b: 0xFF },
                        w: Some(vp_w / slots.len() as u32),
                        h: Some(2 * DENSITY),
                    }),
                },
            )?;
        }
        // 图标 24vp。
        ui.add(
            cell,
            ViewSpec {
                kind: ViewKind::Icon,
                key: 0,
                props: Props::Icon(IconProps {
                    id: t.icon,
                    fg: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                    w: Some(24 * DENSITY),
                    h: Some(24 * DENSITY),
                }),
            },
        )?;
        // 标签 12vp（选中 = brand，未选 = 灰）。
        let label_color = if t.selected {
            Color { r: 0x00, g: 0x7D, b: 0xFF }
        } else {
            Color { r: 0x99, g: 0x99, b: 0x99 }
        };
        ui.add(
            cell,
            ViewSpec {
                kind: ViewKind::Text,
                key: 0,
                props: Props::Text(TextProps {
                    style: TextStyle { fg: label_color, bg: None, scale: FontSize::S8x16 },
                    text: TextBuf::new(t.label)?,
                }),
            },
        )?;
        nodes[i] = cell;
    }
    Ok(nodes)
}
