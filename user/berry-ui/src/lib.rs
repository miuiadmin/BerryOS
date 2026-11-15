//! berry-ui：声明式 UI 引擎内核——保留树 arena + 布局 + 脏区合并 +
//! 遮挡剔除渲染 + 输入手势 referee + 动画表 + 三态停帧帧循环 + 双轨遥测
//! （GUI-3，`docs/GUI-3实施设计草案.md` §6 冻结契约；ADR #52 自研
//! berry-ui 声明式框架 / ADR #53 三态停帧 RUN-SETTLE-IDLE）。
//!
//! # 拓扑（§2）
//!
//! 引擎**只产出** damage 与 DisplayList、**只消费** typed 事件——双依赖：
//! `berry-gfx`（渲染消费面：CommandEncoder/GlyphPos/Color/FontSize——
//! DisplayList 的 `'a` 钉在**调用方 scratch 与 `'static` 图集**上，不钉
//! 引擎，树与列表解耦无自引用，gfx 坑 1/2 对策）+ `berry-guiproto`
//! （常量/类型单源非收发：Rect(u32)/DAMAGE_MAX/RESAMPLE_*/Intent 六档
//! （GUI-8 §4 段①起票面 scene 位携 intent 值；SCENE_* 退役 deprecated）；
//! wire 编解码/channel/VMO 全在 embedder——引擎不假设直摸 vblank/hdi
//! 域，对齐条 19）。**embedder = GUI-5 留位**（guest 侧 gui-wm/gfxdemo
//! 接驳挂账不在本刀；host 侧 embedder = tests/ 离屏 [u8] 纹理对拍）。
//!
//! # 组件件（Phase 2——Button/Toggle/Slider/Progress 首批 + Check/Radio/Step/Seg/Stack/Spacer/Divider 第二批 + Badge/Sbar/Tip/Collapse/Chip/Fab/Card 第三批）
//!
//! 从数据壳接通四面：渲染（paint 固定色板 + 已有原语组合）、布局（固
//! 定内在——「组件 v1 几何常量」块单源）、交互（pump 期
//! `widget_react`：Button 派 on_tap / Toggle·Check 翻转 + on_change /
//! Slider 拖跟 + on_change / Radio 置位 + on_change / Step·Seg 击点
//! 分段 + on_change——产物经 `Ui::take_events` 值面携出；ADR #52
//! Msg-only 零闭包不动、desc.rs 冻结不加 Msg 变体）、命令面（add/
//! set_props 开口 + 值域先验）。固定值起步（主题/样式系统落地后迁
//! 出）。布局三件无交互：Stack = 子聚合 max 叠层容器（`_` 臂全幅叠
//! 放）、Spacer = 0×0 占位吃满 avail、Divider = 厚度线随父宽。第三
//! 批七件：显示三件无交互（Badge 角标 / Sbar 滚动条 / Tip 工具提
//! 示）、组合四件（Collapse 折叠组容器——Click 翻转 expanded 派
//! on_toggle、Chip 胶囊标签——Click 翻转 selected 派 on_tap、Fab 圆
//! 钮——Click 派 on_tap、Card 卡片容器——双 fill_rect 阴影无交互）。
//!
//! # 纪律铁律（§0）
//!
//! - 全库零 alloc、零 panic：定容数组满 → loud `Err` 绝不静默丢；敌意
//!   几何防御采样/checked/饱和。
//! - **坐标三套显式换算**（gfx 坑 10）：内部几何 i32（`damage::RectI`）/
//!   damage 与输入面 u32（guiproto `Rect` 同口径）/容量 usize——跨界
//!   一律显式转换 + checked，禁裸 `as`（u32→i32 负侧饱和、i64→u32
//!   clamp 收口）。
//! - 单线程语境立法：`Ui` 一律 `&mut self` 方法，无 Cell 无 Sync。
//! - request-driven 落法：引擎不持定时器——`next_wakeup() -> Option<u64>`
//!   交 embedder 睡/等事件；`None` = IDLE（撤软 vsync 定时器，R3 反面
//!   清单对策）。
#![cfg_attr(not(test), no_std)]

pub mod anim;
pub mod damage;
pub mod dep;
pub mod desc;
pub mod frame;
pub mod hit;
pub mod layout;
#[cfg(feature = "s7")]
pub mod mask;
pub mod metrics;
pub mod paint;
// 波2 控件底座（六波方案步骤 3）：模式件 + 主题 token 两个新模块。
pub mod pattern;
pub mod theme;
pub mod resample;
pub mod swipe;
pub mod telemetry;
// arena 内嵌本文件（Node/槽表私有——§6）；NodeId 在 desc.rs 公开。

// 扁平面 re-export：门面签名里不限定模块路径的名字（§6 用法对齐）。
pub use desc::{AstError, Axis, Msg, MsgBuf, NodeId, Props, TextBuf, ViewKind, ViewSpec};
pub use metrics::{EmptyIconSource, IconSource, MonospaceTextMetrics, ProportionalTextMetrics, TextMetrics};

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_guiproto::{FRAME_DISCARDED, FRAME_PRESENTED, PHASE_MOVE};
use resample::Sample;

// ---------- 常量族（§6 冻结） ----------

/// 走查/位图/序缓冲容量单源（D10：layout::CAP 与 paint::ORDER_CAP =
/// `2 * MAX_NODES` 常量表达式；CAP 注释五处耦合同步）。
pub const MAX_NODES: usize = 512;
pub const TEXT_INLINE: usize = 64;
pub const MSG_MAX: usize = 8;
pub const ANIM_MAX: usize = 16;
/// S7 anim staging 帽（§6/D1）：与动画槽帽 ANIM_MAX 同源——单 tick 应用
/// 的 Layout 型动画 ≤ 槽数 → staging 条数结构性不触帽（满 = 既有 loud
/// 形态 debug_assert 防御面）。B 面（cfg s7）编译——A 面无 staging。
#[cfg(feature = "s7")]
pub const ANIM_STAGING_MAX: usize = ANIM_MAX;
pub const INPUT_BUF_MAX: usize = 8;
pub const DEP_EDGES_MAX: usize = 64;
pub const DRAG_SLOP_PX: i32 = 8;
pub const OVERDRAW_BUDGET_X256: u32 = 640; // 2.5×256（GUI-10 反清单 7）
pub const IDLE_AFTER_MS: u64 = 2000; // OH 常量：静默强降 IDLE
pub type UiMain = Ui<MAX_NODES>;

// ---------- 组件四件 v1 几何常量（Phase 2 首批——Button/Toggle/Slider/Progress） ----------
//
// 布局 intrinsic（layout.rs）与渲染/命中映射（paint.rs / 本文件
// slider_permille_at）的单源；固定值起步（主题/样式系统落地后迁出——
// 文档化豁免同 PRESSED_BG 先例）。pub(crate)：非契约面，随视觉迭代
// 可调。

/// Button 标签每字节字宽（= S8x16 档 cell_w——paint 渲染推进同值，
/// 测量/渲染不分叉）。
pub(crate) const BUTTON_CHAR_W: u32 = 8;
/// Button 水平留白（左右合计——标签两侧各 16）。
pub(crate) const BUTTON_PAD_X: u32 = 32;
/// Button 最小宽（短标签的下限）。
pub(crate) const BUTTON_MIN_W: u32 = 64;
/// Button 恒高（触摸最小可点区 48px）。
pub(crate) const BUTTON_H: u32 = 48;
/// Toggle 固定内在宽（标准开关）。
pub(crate) const TOGGLE_W: u32 = 52;
/// Toggle 固定内在高。
pub(crate) const TOGGLE_H: u32 = 32;
/// Slider 固定内在宽（最小可拖区）。
pub(crate) const SLIDER_W: u32 = 200;
/// Slider 固定内在高。
pub(crate) const SLIDER_H: u32 = 44;
/// Slider 拖动圆直径（行程映射 inset = D/2——渲染与命中单源）。
pub(crate) const SLIDER_KNOB_D: u32 = 20;
/// Progress 固定内在宽（线性进度条）。
pub(crate) const PROGRESS_W: u32 = 200;
/// Progress 固定内在高。
pub(crate) const PROGRESS_H: u32 = 8;
/// Slider/Progress value 上域（desc.rs SliderProps/ProgressProps 契约
/// 「0..=1000 permille」的命令面执法值——AST/builder 面 BadValue 同律）。
pub(crate) const PERMILLE_MAX: u32 = 1000;

// ---------- 组件七件 v1 几何常量（Phase 2 第二批——Check/Radio/Step/Seg + Stack/Spacer/Divider） ----------
//
// 同首批块纪律：布局 intrinsic（layout.rs）与渲染分段（paint.rs）/命中
// 分段（本文件 widget_react）的单源；固定值起步（主题/样式系统落地后
// 迁出——文档化豁免同 PRESSED_BG 先例）。

/// Check 固定内在宽（标准复选方框）。
pub(crate) const CHECK_W: u32 = 24;
/// Check 固定内在高。
pub(crate) const CHECK_H: u32 = 24;
/// Radio 固定内在宽（标准单选圆）。
pub(crate) const RADIO_W: u32 = 24;
/// Radio 固定内在高。
pub(crate) const RADIO_H: u32 = 24;
/// Step 固定内在宽（左减号 + 值 + 右加号三段）。
pub(crate) const STEP_W: u32 = 120;
/// Step 固定内在高。
pub(crate) const STEP_H: u32 = 36;
/// Seg 单段宽（段数 = segment_count）。
pub(crate) const SEG_SEG_W: u32 = 80;
/// Seg 固定内在高。
pub(crate) const SEG_H: u32 = 36;
/// Tabs 页签条高（波2 批 A——paint TABS_BAR_H 同值 i32 面）。
pub(crate) const TABS_BAR_H: i32 = 48;
/// Tabs 最小内在宽（单页签下限）。
pub(crate) const TABS_MIN_W: u32 = 96;
/// Sbar 固定内在长（横轨长/竖轨长——第三批定稿 200：竖 4×200 / 横
/// 200×4，批 A 的 100 为半成品遗留值）。
pub(crate) const SBAR_LONG: u32 = 200;
/// Sbar 固定内在厚（轨厚）。
pub(crate) const SBAR_THICK: u32 = 4;
/// Badge 圆点直径（show_dot 档；= 数字档高 16——双档同直径单源）。
pub(crate) const BADGE_DOT_D: u32 = 16;
/// Badge 数字档水平留白（左右合计——数字两侧各 4；宽随位档 = 位档×8
/// + 此值）。
pub(crate) const BADGE_PAD_X: u32 = 8;
/// Select 固定内在宽（下拉标准行——批 C）。
pub(crate) const SELECT_W: u32 = 200;
/// Input 固定内在宽（单行输入——批 D）。
pub(crate) const INPUT_W: u32 = 200;
/// Input 固定内在高（= 按钮行高 48）。
pub(crate) const INPUT_H: u32 = 48;
/// Edit 固定内在宽（多行编辑——三行档）。
pub(crate) const EDIT_W: u32 = 200;
/// Edit 固定内在高（三行 48×2 + 行距）。
pub(crate) const EDIT_H: u32 = 96;
/// Canvas 固定内在边长（自绘面占位——批 D 视觉底）。
pub(crate) const CANVAS_D: u32 = 200;
/// 输入内容侧表容量（批 D：Input/Edit 实际文本不走 props——引擎侧
/// 定容表 keyed by NodeId；满 = NodeFull loud 同族）。
pub const INPUT_TEXT_MAX: usize = 8;
/// Tip 高（= S8x16 cell_h——Tip 文本与 Button 标签同档）。
pub(crate) const TIP_H: u32 = 16;
/// Collapse header 高（内在高恒定基 + expanded 子摆放纵向偏移——布
/// 局/渲染单源）。
pub(crate) const COLLAPSE_HEADER_H: u32 = 32;
/// Chip 高（胶囊标签标准高）。
pub(crate) const CHIP_H: u32 = 32;
/// Chip 水平留白（左右合计——标签两侧各 8）。
pub(crate) const CHIP_PAD_X: u32 = 16;
/// Fab 圆形直径（触摸最小可点区同 Button 48 上调至 Material 56）。
pub(crate) const FAB_D: u32 = 56;
/// Fab 扩展标签与圆的间隙。
pub(crate) const FAB_LABEL_GAP: u32 = 8;
/// Gauge 固定内在边长（批 F：表盘方格——值条 + 值文本两带）。
pub(crate) const GAUGE_D: u32 = 64;
/// Qr 每模块像素边长（批 F：21 模块 → 84×84；内在 = module_count × 此值）。
pub(crate) const QR_MODULE_PX: u32 = 4;
/// DragReorder 固定内在宽（批 F：拖拽手柄列）。
pub(crate) const DRAG_W: u32 = 24;
/// DragReorder 固定内在高（= 行高 36 档）。
pub(crate) const DRAG_H: u32 = 36;

/// arena 运行期树深帽（GUI-6 D29）：add() 执法（超深 → NodeBad loud）
/// ——递归面（measure/place/cull/emit）未迭代化，深链 × ~200B/帧穿
/// 32KB guest 栈；96×~200B + 数组面 < 32KB 安全。与 D11 builder 帽
/// DESC_DEPTH_MAX=64 分工（构建期 vs 运行期——64 < 96 自洽）。
pub const TREE_DEPTH_MAX: u32 = 96;

// ---------- a11y 侧表（GUI-6 D12 段B 最小落位） ----------

/// a11y 侧表容量（D12：a11y 条目数帽——超 → apply 面 TooSmall loud）。
pub const A11Y_MAX: usize = 32;

/// a11y 角色常量域（wire u8；§6 冻结——枚举外的开放值域，host 可扩）。
pub const ROLE_NONE: u8 = 0;
pub const ROLE_BUTTON: u8 = 1;
pub const ROLE_TEXT: u8 = 2;
pub const ROLE_IMAGE: u8 = 3;
pub const ROLE_TOGGLE: u8 = 4;
pub const ROLE_LIST_ITEM: u8 = 5;

/// a11y 条目（D12：Node 冻结 19 字段无 a11y 位——侧表存 NodeId + 角色
/// + 内联标签）。`node` 的 gen 位与 arena 三重对拍（发射面防悬空残项）。
/// 完整生命周期（增删查 pub API）= 段D；段B 只落 apply/encode 数据面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct A11yEntry {
    pub node: NodeId,
    pub role: u8,
    pub label: TextBuf,
}

// ---------- 组件事件面（Phase 2 首批——Button/Toggle/Slider 派发） ----------

/// 组件事件缓冲容量（MsgBuf 同形定容 8；满 = loud 计数不静默丢——
/// [`Ui::lost_widget_events`]）。
pub const WIDGET_EVENT_MAX: usize = 8;

/// 组件事件（widget_react 派发产物——引擎产、app 消费，ADR #52 零闭
/// 包同律：不新增 Msg 变体（desc.rs 冻结），事件 ID 面（props 的
/// on_tap/on_change）经本值面携出）。
///
/// `value` 语义随 kind：Button = 恒 0；Toggle = 翻转后 checked（0/1）；
/// Slider = 新 permille 值（0..=1000）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetEvent {
    pub node: NodeId,
    /// 事件 ID（props 携载：on_tap / on_change）。
    pub event: u32,
    pub value: u32,
}

/// 定容组件事件缓冲（MsgBuf 同形：`iter`/`len`/`is_empty` 公共面；
/// push 为 crate 内派发面，满 → `UiError::MsgFull` loud）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventBuf {
    slots: [Option<WidgetEvent>; WIDGET_EVENT_MAX],
    n: usize,
}

impl EventBuf {
    pub(crate) fn new() -> Self {
        EventBuf { slots: [None; WIDGET_EVENT_MAX], n: 0 }
    }

    pub(crate) fn push(&mut self, e: WidgetEvent) -> Result<(), UiError> {
        if self.n >= WIDGET_EVENT_MAX {
            return Err(UiError::MsgFull);
        }
        self.slots[self.n] = Some(e);
        self.n += 1;
        Ok(())
    }

    /// 顺序迭代（产出序即手势时序）。
    pub fn iter(&self) -> impl Iterator<Item = &WidgetEvent> {
        self.slots[..self.n].iter().filter_map(|s| s.as_ref())
    }

    /// 已收事件数。
    pub fn len(&self) -> usize {
        self.n
    }

    /// 空谓词。
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
}

// ---------- press_map 侧表（GUI-6 D18——段D） ----------

/// press_map 容量（D18：声明式 pressed-state 映射帽；超 = NodeFull loud
/// ——D30 容量族单源立法：UiError 无侧表满变体，NodeFull 兼任「Ui 容量
/// 域满」，与 a11y 满 32 / arena 满同族。波2 批 A 扩 16→28：launcher
/// 建树尾全量登记（图标格 + Dock + 返回/导航键）。
pub const PRESS_MAP_MAX: usize = 28;

/// pressed 视觉底色（D18：RGB u32 打包 `r<<16|g<<8|b`）。设计意图 =
/// 「bg 亮度减半」——v1 无样式系统，硬编码常量单源（文档化豁免；样式
/// 系统落地后由此常量迁出）。
pub const PRESSED_BG: u32 = 0x40_40_40;

/// press_map 单条（D18）：`trigger` = Press/Release/Cancel 锚定节点，
/// `visual` = 视觉换底节点（Rect/Text——bg 承载面），`saved_bg` = Press
/// 时刻原底色（`None` = Text 原透明底——还原 = 移除 bg），`active` =
/// 已换底未还原（防二次 Press 覆写 saved_bg 断还原链）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PressEntry {
    trigger: NodeId,
    visual: NodeId,
    saved_bg: Option<u32>,
    active: bool,
}

/// Color → RGB u32 打包（`r<<16|g<<8|b`——D18 saved_bg 存储形）。
fn pack_color(c: Color) -> u32 {
    ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}

/// RGB u32 打包 → Color（unpack 逆函数）。
fn unpack_color(v: u32) -> Color {
    Color { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 }
}

/// 组件值域先验（Phase 2 两批）：Slider value ≤ 1000 且 min ≤ max、
/// Progress value ≤ 1000、Step step>0 且 min ≤ max 且 value ∈ [min,max]、
/// Seg segment_count>0 且 active < segment_count——desc.rs props 契约
/// （BadValue 域）的命令面执法（add/set_props 两口共用；AST/builder 面
/// 已同律拒）。UiError 族冻结——违者 NodeBad loud（Grid 零维先验同形
/// 兼任）。
fn widget_value_ok(p: &Props) -> bool {
    match p {
        Props::Slider(s) => s.value <= PERMILLE_MAX && s.min <= s.max,
        Props::Progress(v) => v.value <= PERMILLE_MAX,
        Props::Step(s) => {
            s.step > 0 && s.min <= s.max && s.value >= s.min && s.value <= s.max
        }
        Props::Seg(s) => s.segment_count > 0 && s.active < s.segment_count,
        // 波2 批 A：Tabs 零页/active 越页（AST BadValue 同律）、Sbar
        // thumb 两端 permille 域 + start≤end、Scroll offset ≤ content。
        Props::Tabs(t) => t.tab_count > 0 && t.active < t.tab_count,
        Props::Sbar(s) => {
            s.thumb_start <= PERMILLE_MAX
                && s.thumb_end <= PERMILLE_MAX
                && s.thumb_start <= s.thumb_end
        }
        Props::Scroll(s) => s.offset_y <= s.content_height,
        // 组件第三批：Card elevation 0..=3（AST BadValue 同律——
        // desc::CARD_ELEVATION_MAX 单源；corner_radius 无域）。
        Props::Card(c) => c.elevation <= desc::CARD_ELEVATION_MAX,
        // 波2 批 C：Select 选中索引域（option_count == 0 空选择器合法
        // ——selected 恒 0）。
        Props::Select(sl) => sl.option_count == 0 || sl.selected < sl.option_count,
        // 波2 批 E：Refresh offset / SwipeItem reveal 两端 permille 域、
        // DatePick 月日域（AST BadValue 同律）、TimePick 时分域。
        Props::Refresh(r) => r.offset <= PERMILLE_MAX,
        Props::SwipeItem(w) => {
            w.reveal_start <= PERMILLE_MAX && w.reveal_end <= PERMILLE_MAX
        }
        Props::DatePick(d) => {
            (1..=12).contains(&d.month)
                && (1..=31).contains(&d.day)
                && d.min_year <= d.max_year
                && d.year >= d.min_year
                && d.year <= d.max_year
        }
        Props::TimePick(t) => t.hour <= 23 && t.minute <= 59,
        // 批 F（末刀三件）：Gauge value 越千分域、Qr module_count 不在
        // QR 版本域 {21,25,29,...,177}、DragReorder item_index 越项
        //（total_items > 0 时；total_items == 0 空表合法——Select 零项
        // 同律）。AST BadValue 面同律。
        Props::Gauge(g) => g.value <= PERMILLE_MAX,
        Props::Qr(q) => desc::qr_module_count_valid(q.module_count),
        Props::DragReorder(d) => d.total_items == 0 || d.item_index < d.total_items,
        _ => true,
    }
}

// ---------- const 断言族（D10——冻结形态编译期钉死） ----------

/// Node 冻结 19 字段 = 144B（GUI-3 §2.2——字段一字不改的执法面）。
const _: () = assert!(core::mem::size_of::<Node>() == 144, "Node 冻结 19 字段 = 144B（§2.2）");

/// builder 深帽 < arena 深帽（D11/D29 自洽——构建期帽先行收敛，运行期
/// 帽兜底；倒挂即两帽分工破坏）。
const _: () = assert!(
    (desc::DESC_DEPTH_MAX as u32) < TREE_DEPTH_MAX,
    "DESC_DEPTH_MAX < TREE_DEPTH_MAX 自洽（D11/D29）"
);

/// CAP = 2*MAX_NODES 常量表达式联动（D10 单源——MAX_NODES 动则 CAP 走）。
const _: () = assert!(layout::CAP == 2 * MAX_NODES, "layout::CAP = 2*MAX_NODES（D10 单源）");
const _: () = assert!(paint::ORDER_CAP == 2 * MAX_NODES, "paint::ORDER_CAP = 2*MAX_NODES（D10 单源）");

/// 默认软 vsync 周期（≈60Hz）——`set_vsync` 前的 rate_hint/expected 轨
/// 兜底值（协商倒数注入前的确定值，embedder 可覆写）。
const DEFAULT_VSYNC_PERIOD_NS: u64 = 16_666_666;

/// 槽索引空值（§3.1——`u32::MAX`；pub(crate) 供各子模组树走查共用）。
pub(crate) const NONE: u32 = u32::MAX;

// ---------- 错误族（§6 冻结） ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiError {
    /// arena 定容满——loud。
    NodeFull,
    /// 代数不匹配（悬空 NodeId）/坏槽。
    NodeBad,
    /// 容器位传叶子 / Root 不能为子。
    BadParent,
    /// > `TEXT_INLINE`。
    TextTooLong { len: usize },
    /// 定容族。
    AnimFull,
    DepFull,
    MsgFull,
    /// 需先 `root()`。
    NoRoot,
}

// ---------- 节点内部形态（§6 冻结字段——impl 三方共享） ----------

/// 保留树节点（lib.rs 私有 arena 的槽内容；layout/paint/anim 子模组经
/// `pub(crate)` 共享——冻结字段，一字不改）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Node {
    pub gen: u16,
    pub alive: bool,
    pub kind: desc::ViewKind,
    pub key: u64,
    pub props: desc::Props,
    pub parent: u32,
    pub first_child: u32,
    pub next_sib: u32,
    pub prev_sib: u32, // 槽索引；NONE = u32::MAX
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32, // 父相对摆位 + 尺寸（layout 产物）
    pub meas_w: u32,
    pub meas_h: u32, // 内在尺寸缓存（Measure 保留）
    pub layout_dirty: bool,
    pub paint_dirty: bool,
    pub ox: i32,
    pub oy: i32, // paint 偏移（Transform 动画）
}

// ---------- Ui 门面 ----------

/// 引擎门面：保留树 arena（显式 free 链——防「分配器线性扫骰子」K-1
/// 雷同型）+ 布局 + 脏区 + 输入缓冲/手势态 + 动画表 + 三态帧循环 +
/// 遥测环 + 依赖边 + vsync 协商倒数 + credits 簿记。全 `&mut self`
/// 单线程语境（§2 立法）。
pub struct Ui<const N: usize> {
    nodes: [Option<Node>; N],
    gens: [u16; N], // 每槽代数（free 时 +1——悬空 id 判据单源）
    free_stack: [u32; N],
    free_count: usize,
    root_slot: u32,
    vp_w: u32,
    vp_h: u32,
    damage: damage::DamageSet,
    input_buf: [Option<hit::PtrEvent>; INPUT_BUF_MAX],
    input_n: usize,
    gesture: hit::Gesture,
    move_hist: [Sample; INPUT_BUF_MAX], // 活跃 MOVE 滑窗（重采样消费）
    hist_len: usize,
    anims: [Option<anim::Anim>; ANIM_MAX],
    phase: frame::FramePhase,
    last_work_ns: u64,
    last_now_ns: u64, // next_wakeup 的「当前时刻」锚（引擎无钟——最近帧时刻）
    vsync_period_ns: u64,
    vsync_phase_ns: u64,
    credits: frame::Credits, // 背压簿记单源 frame.rs（outstanding 口径）
    inflight: [(u32, u64); 2], // (seq, expected_ns)——CREDITS_MAX=2 在途位
    inflight_n: usize,
    seq: u32,
    last_input_ts_ns: u64,
    last_frame_seq: u32,
    last_expected_ns: u64,
    spinup_anchor: Option<u64>, // wake 时刻的下一 tick（起振延迟遥测锚）
    deps: dep::DepTable, // 依赖边表单源 dep.rs（幂等去重 + unbind/clear）
    telem: telemetry::Telemetry,
    tm: &'static dyn metrics::TextMetrics, // D28 供给槽（默认 Monospace）
    icons: &'static dyn metrics::IconSource, // D28 供给槽（默认 Empty）
    a11y: [Option<A11yEntry>; A11Y_MAX], // a11y 侧表（D12——apply 落/encode 读）
    press_map: [PressEntry; PRESS_MAP_MAX], // pressed 短路径表（D18——pump 期消费）
    press_map_n: usize,
    focus: Option<NodeId>, // 输入焦点窗（批 D——Input/Edit 焦点环/caret 单源）
    input_bufs: [Option<(NodeId, desc::TextBuf)>; INPUT_TEXT_MAX], // 输入内容侧表（批 D）
    input_buf_n: usize,
    widget_events: EventBuf, // 组件事件缓冲（widget_react 期产——take_events 消费）
    lost_widget_events: u32, // 缓冲满被丢的事件计数（loud 诊断面——Gesture::lost_msgs 同形）
}

/// D28 默认供件（host 测试与 v1 guest 的开箱面——Monospace 逐位复刻
/// 现状 Text 测量 / 图标资源未注册恒 None）。
static DEFAULT_TEXT_METRICS: MonospaceTextMetrics = MonospaceTextMetrics;
static DEFAULT_ICON_SOURCE: EmptyIconSource = EmptyIconSource;

// —— 内部小工具（零 panic：全 checked/饱和/先验界内） ——

fn pack_id(idx: usize, gen: u16) -> NodeId {
    NodeId(((gen as u32) << 16) | ((idx as u32) & 0xFFFF))
}

fn clamp_u32(v: i64) -> u32 {
    if v < 0 {
        0
    } else if v > u32::MAX as i64 {
        u32::MAX
    } else {
        v as u32
    }
}

/// Badge 数字宽档（第三批几何面）：0 = 隐藏（0×0）、1 = 单位（16 宽）、
/// 2 = 两位（24 宽）、3 = 99+ 封顶档（32 宽——>99 三字节文案）——同档
/// 变值纯绘不重排（layout.rs Badge 臂与 paint 文案同律单源）。
fn badge_width_class(c: u32) -> u32 {
    if c == 0 {
        0
    } else if c < 10 {
        1
    } else if c < 100 {
        2
    } else {
        3
    }
}

/// props 几何等价判定（纯着色变 → 0 重排——Measure/Render 保留的执法点）。
/// 段B 新面：Col/Row/Grid 的 w/h/cross 入几何（块字段全比）；Image res/
/// w/h、Icon id/w/h 全比；Icon fg 不比（纯着色——paint 位换色不动布局）。
/// S7 B 面无消费者（props_diff 位映射接管）——A 面（cfg not s7）独占。
#[cfg_attr(feature = "s7", allow(dead_code))]
fn props_geom_equal(a: &Props, b: &Props) -> bool {
    match (a, b) {
        (Props::Root, Props::Root) => true,
        (Props::Rect(x), Props::Rect(y)) => x.w == y.w && x.h == y.h,
        (Props::Text(x), Props::Text(y)) => {
            x.style.scale == y.style.scale && x.text.as_bytes() == y.text.as_bytes()
        }
        (Props::Col(x), Props::Col(y)) => {
            x.gap == y.gap && x.w == y.w && x.h == y.h && x.cross == y.cross
        }
        (Props::Row(x), Props::Row(y)) => {
            x.gap == y.gap && x.w == y.w && x.h == y.h && x.cross == y.cross
        }
        (Props::Pad(x), Props::Pad(y)) => x.all == y.all,
        (Props::Grid(x), Props::Grid(y)) => {
            x.cols == y.cols
                && x.rows == y.rows
                && x.gap == y.gap
                && x.cross == y.cross
                && x.w == y.w
                && x.h == y.h
        }
        (Props::Image(x), Props::Image(y)) => x.res == y.res && x.w == y.w && x.h == y.h,
        (Props::Icon(x), Props::Icon(y)) => x.id == y.id && x.w == y.w && x.h == y.h,
        // 组件四件（Phase 2 首批）：Button label/style 入几何（标签宽/
        // 底形变），on_tap 纯事件面不比；Toggle/Slider/Progress 内在恒
        // 定尺寸（几何常量单源）——checked/value/min/max/style 全纯绘
        // 面（滑块/填充位置与颜色），几何等价恒真。
        (Props::Button(x), Props::Button(y)) => {
            x.style == y.style && x.label.as_bytes() == y.label.as_bytes()
        }
        (Props::Toggle(_), Props::Toggle(_)) => true, // checked 纯绘、on_change 纯事件——全不比
        (Props::Slider(_), Props::Slider(_)) => true, // value/min/max 纯绘/纯语义——全不比
        (Props::Progress(_), Props::Progress(_)) => true, // value/style 纯绘——全不比
        // 组件七件（Phase 2 第二批）：Check/Radio/Step 内在恒定尺寸
        //（几何常量单源）——checked/selected/group/value/min/max/step
        // 全纯绘/纯语义面；Seg 段数入几何（段数变 → 内在宽变）、active/
        // on_change 不比；Divider 厚度入几何（内在高）、inset 两端纯绘
        //（分隔线长随父——起点/终点留白不动布局）。
        (Props::Check(_), Props::Check(_)) => true,
        (Props::Radio(_), Props::Radio(_)) => true,
        (Props::Step(_), Props::Step(_)) => true,
        (Props::Seg(x), Props::Seg(y)) => x.segment_count == y.segment_count,
        (Props::Divider(x), Props::Divider(y)) => x.thickness == y.thickness,
        // 波2 批 A + 第三批：Tabs/Sbar/Badge/Scroll/List + Tip/Collapse/
        // Chip/Fab/Card。Tabs 段数入几何（段宽变 → 指示条/分隔线变）、
        // active/on_switch 不比；Sbar **orientation 入几何**（第三批内
        // 在随走向互转 4×200↔200×4——批 A 恒几何的半成品遗留已修）、
        // thumb 区间纯绘；Badge 圆点位 + 数字宽档入几何（count 同档变
        // 值纯绘——第三批宽随位档 16/24/32）；Scroll content_height 入
        // 几何、offset 纯绘（子上移）；List item_count×item_height 入
        // 几何；Tip 文本入几何（宽随文）、placement 数据携载不比；
        // Collapse expanded 入几何（内在高折叠/展开）；Chip label 入几
        // 何、selected 纯绘；Fab 扩展标签入几何（圆↔扩展互转）、icon
        // 纯绘（v1 文本占位）；Card elevation/corner_radius 纯绘（阴
        // 影条在界内——视觉足迹不越界）。
        (Props::Tabs(x), Props::Tabs(y)) => x.tab_count == y.tab_count,
        (Props::Sbar(x), Props::Sbar(y)) => x.orientation == y.orientation,
        (Props::Badge(x), Props::Badge(y)) => {
            x.show_dot == y.show_dot
                && badge_width_class(x.count) == badge_width_class(y.count)
        }
        // Scroll content_height 入几何；offset_y 入几何（子整体上移 =
        // 摆位面，非纯绘——geom_same 误判会让子钉在旧 offset）。
        (Props::Scroll(x), Props::Scroll(y)) => {
            x.content_height == y.content_height && x.offset_y == y.offset_y
        }
        (Props::List(x), Props::List(y)) => {
            x.item_count == y.item_count && x.item_height == y.item_height
        }
        // 波2 批 C：Select 内在恒定（几何常量单源）——selected/
        // option_count 全纯绘面。
        (Props::Select(_), Props::Select(_)) => true,
        (Props::Tip(x), Props::Tip(y)) => x.text.as_bytes() == y.text.as_bytes(),
        (Props::Collapse(x), Props::Collapse(y)) => x.expanded == y.expanded,
        (Props::Chip(x), Props::Chip(y)) => x.label.as_bytes() == y.label.as_bytes(),
        (Props::Fab(x), Props::Fab(y)) => {
            x.extended_label.as_bytes() == y.extended_label.as_bytes()
        }
        (Props::Card(_), Props::Card(_)) => true,
        // 批 F：Gauge/DragReorder 内在恒定（几何常量单源）——value/
        // style/item_index/total_items 全纯绘面；Qr module_count 入几何
        //（内在 = module_count×4 方阵）——dark/light 双色纯绘。
        (Props::Gauge(_), Props::Gauge(_)) => true,
        (Props::Qr(x), Props::Qr(y)) => x.module_count == y.module_count,
        (Props::DragReorder(_), Props::DragReorder(_)) => true,
        _ => false, // 变体切换按几何变处理（保守重排）
    }
}

impl<const N: usize> Ui<N> {
    pub fn new(viewport_w: u32, viewport_h: u32) -> Self {
        let mut free_stack = [0u32; N];
        let mut i = 0usize;
        while i < N {
            free_stack[i] = N as u32 - 1 - i as u32; // 栈顶 = 槽 0
            i += 1;
        }
        Ui {
            nodes: [None; N],
            gens: [0; N],
            free_stack,
            free_count: N,
            root_slot: NONE,
            vp_w: viewport_w,
            vp_h: viewport_h,
            damage: damage::DamageSet::new(),
            input_buf: [None; INPUT_BUF_MAX],
            input_n: 0,
            gesture: hit::Gesture::new(),
            move_hist: [Sample { ts_ns: 0, x: 0, y: 0 }; INPUT_BUF_MAX],
            hist_len: 0,
            anims: [None; ANIM_MAX],
            phase: frame::FramePhase::Idle,
            last_work_ns: 0,
            last_now_ns: 0,
            vsync_period_ns: DEFAULT_VSYNC_PERIOD_NS,
            vsync_phase_ns: 0,
            credits: frame::Credits::new(),
            inflight: [(0, 0); 2],
            inflight_n: 0,
            seq: 0,
            last_input_ts_ns: 0,
            last_frame_seq: 0,
            last_expected_ns: 0,
            spinup_anchor: None,
            deps: dep::DepTable::new(),
            telem: telemetry::Telemetry::new(),
            tm: &DEFAULT_TEXT_METRICS,
            icons: &DEFAULT_ICON_SOURCE,
            a11y: [None; A11Y_MAX],
            press_map: [PressEntry {
                trigger: NodeId(0),
                visual: NodeId(0),
                saved_bg: None,
                active: false,
            }; PRESS_MAP_MAX],
            press_map_n: 0,
            focus: None,
            input_bufs: [None; INPUT_BUF_MAX],
            input_buf_n: 0,            widget_events: EventBuf::new(),
            lost_widget_events: 0,
        }
    }

    // —— arena 内部（显式 free 链 + 代数） ——

    fn alloc_slot(&mut self) -> Option<usize> {
        if self.free_count == 0 {
            return None;
        }
        self.free_count -= 1;
        let idx = self.free_stack[self.free_count] as usize;
        if idx >= N {
            return None;
        }
        Some(idx)
    }

    fn free_slot(&mut self, idx: usize) {
        if idx >= N {
            return;
        }
        self.nodes[idx] = None;
        self.gens[idx] = self.gens[idx].wrapping_add(1); // 旧 id 全悬空
        if self.free_count < N {
            self.free_stack[self.free_count] = idx as u32;
            self.free_count += 1;
        }
    }

    fn slot_of(&self, id: NodeId) -> Result<usize, UiError> {
        let idx = (id.0 & 0xFFFF) as usize;
        let gen = (id.0 >> 16) as u16;
        if idx >= N {
            return Err(UiError::NodeBad);
        }
        match self.nodes[idx].as_ref() {
            Some(n) if n.alive && n.gen == gen => Ok(idx),
            _ => Err(UiError::NodeBad),
        }
    }

    fn child_tail(&self, parent_idx: usize) -> u32 {
        let Some(p) = self.nodes[parent_idx].as_ref() else { return NONE };
        let mut c = p.first_child;
        while c != NONE {
            let Some(n) = self.nodes.get(c as usize).and_then(|o| o.as_ref()) else { return NONE };
            if n.next_sib == NONE {
                return c;
            }
            c = n.next_sib;
        }
        NONE
    }

    fn link_child(&mut self, parent_idx: usize, child: u32) {
        let tail = self.child_tail(parent_idx);
        if tail == NONE {
            if let Some(p) = self.nodes[parent_idx].as_mut() {
                p.first_child = child;
            }
        } else if let Some(t) = self.nodes[tail as usize].as_mut() {
            t.next_sib = child;
        }
        if let Some(c) = self.nodes[child as usize].as_mut() {
            c.prev_sib = tail;
        }
    }

    fn unlink(&mut self, idx: usize) {
        let (parent, prev, next) = match self.nodes[idx].as_ref() {
            Some(n) => (n.parent, n.prev_sib, n.next_sib),
            None => return,
        };
        if parent != NONE {
            if let Some(p) = self.nodes[parent as usize].as_mut() {
                if p.first_child == idx as u32 {
                    p.first_child = next;
                }
            }
        }
        if prev != NONE {
            if let Some(p) = self.nodes[prev as usize].as_mut() {
                p.next_sib = next;
            }
        }
        if next != NONE {
            if let Some(n) = self.nodes[next as usize].as_mut() {
                n.prev_sib = prev;
            }
        }
    }

    /// 兄弟链递归收集（子树槽清单；容量帽 = 清单长，满即止——活树 ≤ N 恒不触）。
    fn collect_sibs(&self, head: u32, out: &mut [u32], n: &mut usize) {
        let mut c = head;
        while c != NONE {
            let Some(node) = self.nodes.get(c as usize).and_then(|o| o.as_ref()) else { return };
            let (fc, ns) = (node.first_child, node.next_sib);
            if *n < out.len() {
                out[*n] = c;
                *n += 1;
            }
            self.collect_sibs(fc, out, n);
            c = ns;
        }
    }

    fn mark_ancestors_dirty(&mut self, mut slot: u32) {
        let mut hops = 0usize;
        while slot != NONE && hops <= N {
            let idx = slot as usize;
            let Some(n) = self.nodes.get_mut(idx).and_then(|o| o.as_mut()) else { return };
            n.layout_dirty = true;
            slot = n.parent;
            hops += 1;
        }
    }

    /// 绝对原点（含沿途 ox/oy paint 偏移——命中随视觉走，§3.6）。
    fn abs_origin(&self, idx: usize) -> (i32, i32) {
        let mut ax = 0i32;
        let mut ay = 0i32;
        let mut cur = idx as u32;
        let mut hops = 0usize;
        while hops <= N {
            let Some(n) = self.nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
            ax = ax.saturating_add(n.x).saturating_add(n.ox);
            ay = ay.saturating_add(n.y).saturating_add(n.oy);
            if n.parent == NONE {
                break;
            }
            cur = n.parent;
            hops += 1;
        }
        (ax, ay)
    }

    fn abs_rect_of(&self, idx: usize) -> damage::RectI {
        let (ax, ay) = self.abs_origin(idx);
        match self.nodes[idx].as_ref() {
            Some(n) => damage::RectI::new(ax, ay, n.w, n.h),
            None => damage::RectI::new(0, 0, 0, 0),
        }
    }

    fn damage_rect(&mut self, r: damage::RectI) {
        // §3.4 add 流程：先 clip 到 viewport（负界/越界折界内残片；零面
        // 积 no-op 丢弃——gfx 坑 5 收口在此，wire 化后恒 ≥0）。GUI-6
        // D17：与 layout/anim 三调用点收敛单源 `layout::dmg_clip_add`。
        let vp = damage::RectI::new(
            0,
            0,
            damage::u32_to_i32_sat(self.vp_w),
            damage::u32_to_i32_sat(self.vp_h),
        );
        layout::dmg_clip_add(&mut self.damage, &r, &vp);
    }

    fn reset_arena(&mut self) {
        let mut i = 0usize;
        while i < N {
            self.nodes[i] = None;
            self.gens[i] = self.gens[i].wrapping_add(1);
            self.free_stack[i] = N as u32 - 1 - i as u32;
            i += 1;
        }
        self.free_count = N;
        self.root_slot = NONE;
        let mut i = 0usize;
        while i < ANIM_MAX {
            self.anims[i] = None;
            i += 1;
        }
        self.deps.clear(); // 旧树 NodeId 全悬空——边表整清（dep 表侧单源）
        self.a11y = [None; A11Y_MAX]; // 同理整清（D12 侧表随树生命周期）
        self.press_map_n = 0; // 同理整清（D18 表随树生命周期——悬空 id 无还原义）
        self.focus = None; // 焦点随树清（批 D——悬空 id 无义）
        self.input_bufs = [None; INPUT_BUF_MAX]; // 内容侧表同理整清
        self.input_buf_n = 0;
        self.widget_events = EventBuf::new(); // 同理整清（组件事件随树生命周期——旧 NodeId 悬空无义）
        self.lost_widget_events = 0;
        self.input_n = 0;
        self.hist_len = 0;
        self.gesture = hit::Gesture::new(); // 旧树 NodeId 全悬空——手势态随之归零
    }

    // —— 视口 ——

    pub fn set_viewport(&mut self, w: u32, h: u32) {
        self.vp_w = w;
        self.vp_h = h;
        let mut i = 0usize;
        while i < N {
            if let Some(n) = self.nodes[i].as_mut() {
                n.layout_dirty = true;
            }
            i += 1;
        }
        self.damage.add_full();
    }

    pub fn viewport(&self) -> (u32, u32) {
        (self.vp_w, self.vp_h)
    }

    // —— 树构造（builder 入口①） ——

    pub fn root(&mut self) -> NodeId {
        // 单根幂等。
        if self.root_slot != NONE {
            if let Some(n) = self.nodes[self.root_slot as usize].as_ref() {
                return pack_id(self.root_slot as usize, n.gen);
            }
        }
        // 冻结签名非 Result：容量哨兵 = NodeId(u32::MAX)（N=0/满场——
        // builder 面不可达，loud 哨兵供调用方 kind_of 验证）。
        let Some(idx) = self.alloc_slot() else { return NodeId(NONE) };
        let gen = self.gens[idx];
        self.nodes[idx] = Some(Node {
            gen,
            alive: true,
            kind: ViewKind::Root,
            key: 0,
            props: Props::Root,
            parent: NONE,
            first_child: NONE,
            next_sib: NONE,
            prev_sib: NONE,
            x: 0,
            y: 0,
            w: self.vp_w as i32,
            h: self.vp_h as i32,
            meas_w: self.vp_w,
            meas_h: self.vp_h,
            layout_dirty: true,
            paint_dirty: true,
            ox: 0,
            oy: 0,
        });
        self.root_slot = idx as u32;
        pack_id(idx, gen)
    }

    pub fn add(&mut self, parent: NodeId, spec: desc::ViewSpec) -> Result<NodeId, UiError> {
        let pidx = self.slot_of(parent)?;
        match self.nodes[pidx].as_ref().map(|n| n.kind) {
            // 容器白名单（Grid 入列——D24；Stack 入列——Phase 2 第二批：
            // 叠层容器，子经 place_children `_` 臂 Root 型全幅叠放；波2
            // 批 A：Tabs（内容区面板子）/Scroll（滚动视口子）/List（行
            // 子）入列——place_children 各自臂已落；第三批：Collapse
            //（折叠内容子——place_children Collapse 臂）/Card（卡片内容
            // 子——`_` 臂全幅给子）入列）。
            Some(ViewKind::Root) | Some(ViewKind::Col) | Some(ViewKind::Row) | Some(ViewKind::Pad)
            | Some(ViewKind::Grid) | Some(ViewKind::Stack) | Some(ViewKind::Tabs)
            | Some(ViewKind::Scroll) | Some(ViewKind::List) | Some(ViewKind::Collapse)
            | Some(ViewKind::Card) => {}
            // 叶带子显式列名（Rect/Text 原有 + Image/Icon 新叶 + 组件七
            // 件第二批的六叶——desc.rs AST 面「叶带子拒面」同律）：容器位
            // 传叶子 → BadParent。Grid 格满/零维先验在 Root-子检后（段C
            // 已落——AST/builder/add/set_props 三面同律齐）。
            Some(ViewKind::Rect) | Some(ViewKind::Text) | Some(ViewKind::Image)
            | Some(ViewKind::Icon)
            | Some(ViewKind::Spacer)
            | Some(ViewKind::Divider)
            | Some(ViewKind::Check)
            | Some(ViewKind::Radio)
            | Some(ViewKind::Step)
            | Some(ViewKind::Seg)
            // 第三批五新叶（Badge/Sbar 已随批 A 建档；Tip/Chip/Fab 补列
            // ——desc.rs AST 面「叶带子拒面」同律）。
            | Some(ViewKind::Badge)
            | Some(ViewKind::Sbar)
            | Some(ViewKind::Tip)
            | Some(ViewKind::Chip)
            | Some(ViewKind::Fab) => return Err(UiError::BadParent),
            _ => return Err(UiError::BadParent), // 死槽防御
        }
        if spec.kind == ViewKind::Root {
            return Err(UiError::BadParent); // Root 不能为子
        }
        // Grid spec 先验（§4.2 拒面 add 侧；AST/builder 面在 desc.rs 双
        // 拒）：零维 spec 拒——UiError 族冻结（D21），NodeBad 兼任。
        if let Props::Grid(g) = &spec.props {
            if g.cols == 0 || g.rows == 0 {
                return Err(UiError::NodeBad);
            }
        }
        // 组件值域先验（Phase 2 两批）：Slider/Progress permille 域、
        // Step 刻度域、Seg 段域——AST/builder 面 BadValue 同律，命令面
        // add/set_props 两口齐。
        if !widget_value_ok(&spec.props) {
            return Err(UiError::NodeBad);
        }
        // Grid 父位格满拒（GUI-6 段C——D24/add 面）：子数 ≥ cols×rows
        // 再 add = BadParent（「父位不容再子」族；AST 面 GridNotFit 同
        // 律）。父零维 = 状态漂移（add/set_props 两面已拦新值）——
        // NodeBad loud 防静默错格。
        if let Some(Props::Grid(g)) = self.nodes[pidx].as_ref().map(|n| &n.props) {
            if g.cols == 0 || g.rows == 0 {
                return Err(UiError::NodeBad);
            }
            let cap = g.cols as u32 * g.rows as u32;
            let mut cnt = 0u32;
            let mut cur = self.nodes[pidx].as_ref().map(|n| n.first_child).unwrap_or(NONE);
            let mut hops = 0usize;
            while cur != NONE && hops <= N {
                hops += 1;
                match self.nodes.get(cur as usize) {
                    Some(Some(c)) if c.alive => {
                        cnt += 1;
                        cur = c.next_sib;
                    }
                    _ => break,
                }
            }
            if cnt >= cap {
                return Err(UiError::BadParent);
            }
        }
        // D29 arena 树深帽：新节点深度 = 父链节点数 + 1（根 = 深度 1）
        // ——超 TREE_DEPTH_MAX 拒（递归面 measure/place/cull/emit 未迭
        // 代化，深链穿 32KB guest 栈）。链走查带环防御（hops 帽 = N）。
        let mut chain: u32 = 0;
        let mut cur = pidx as u32;
        let mut hops = 0usize;
        while hops <= N {
            hops += 1;
            let Some(n) = self.nodes.get(cur as usize).and_then(|o| o.as_ref()) else {
                break; // 链断（结构异常）：以已计深度为准保守放行
            };
            chain += 1;
            if n.parent == NONE {
                break;
            }
            cur = n.parent;
        }
        if chain.saturating_add(1) > TREE_DEPTH_MAX {
            return Err(UiError::NodeBad); // 超深 loud（§6 UiError 族无深帽变体——NodeBad 兼任）
        }
        let Some(idx) = self.alloc_slot() else { return Err(UiError::NodeFull) };
        let gen = self.gens[idx];
        self.nodes[idx] = Some(Node {
            gen,
            alive: true,
            kind: spec.kind,
            key: spec.key,
            props: spec.props,
            parent: pidx as u32,
            first_child: NONE,
            next_sib: NONE,
            prev_sib: NONE,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            meas_w: 0,
            meas_h: 0,
            layout_dirty: true,
            paint_dirty: true,
            ox: 0,
            oy: 0,
        });
        self.link_child(pidx, idx as u32);
        // 容器重排 + 既有兄弟位移面 damage（超集覆盖——正确性优先）。
        let pr = self.abs_rect_of(pidx);
        self.damage_rect(pr);
        self.mark_ancestors_dirty(idx as u32);
        Ok(pack_id(idx, gen))
    }

    pub fn remove(&mut self, node: NodeId) -> Result<(), UiError> {
        // 连子树；先收旧绝对界进 damage 再整族释放（§3.1）。
        let idx = self.slot_of(node)? as usize;
        let mut list = [NONE; N];
        list[0] = idx as u32;
        let mut cnt = 1usize;
        let fc = self.nodes[idx].as_ref().map(|n| n.first_child).unwrap_or(NONE);
        self.collect_sibs(fc, &mut list, &mut cnt);
        let parent = self.nodes[idx].as_ref().map(|n| n.parent).unwrap_or(NONE);
        let mut i = 0usize;
        while i < cnt {
            let s = list[i] as usize;
            let r = self.abs_rect_of(s);
            self.damage_rect(r);
            i += 1;
        }
        if parent != NONE {
            let pr = self.abs_rect_of(parent as usize);
            self.damage_rect(pr);
        }
        self.unlink(idx);
        if idx as u32 == self.root_slot {
            self.root_slot = NONE;
        }
        // 依赖边随子树解绑——**先于 free_slot**（gen 推进前以当前
        // NodeId 精确解绑，表侧 unbind_node 全清该节点边；挪到 free
        // 之后则须靠悬空 id 扫描兜——两形等价但先验解绑无扫描面）。
        let mut i = 0usize;
        while i < cnt {
            if let Some(n) = self.nodes[list[i] as usize].as_ref() {
                self.deps.unbind_node(pack_id(list[i] as usize, n.gen));
            }
            i += 1;
        }
        let mut i = 0usize;
        while i < cnt {
            self.free_slot(list[i] as usize);
            i += 1;
        }
        // 随行动画清理（悬空 id 判据 = slot_of 失败——gen 已推进）。
        let mut i = 0usize;
        while i < ANIM_MAX {
            if let Some(a) = self.anims[i] {
                if self.slot_of(a.node).is_err() {
                    self.anims[i] = None;
                }
            }
            i += 1;
        }
        if parent != NONE {
            self.mark_ancestors_dirty(parent);
        }
        // D12 生命周期①：子树连带清 a11y 条目（槽位匹配——旧 gen 残项
        // 同清，「remove 后无悬挂」钉板对象；条目仅创建于活节点，槽位
        // 命中即本子树成员或既有残项，两清皆正确）。
        let mut k = 0usize;
        while k < A11Y_MAX {
            let hit = self.a11y[k]
                .as_ref()
                .map(|e| list[..cnt].contains(&(e.node.0 & 0xFFFF)))
                .unwrap_or(false);
            if hit {
                self.a11y[k] = None;
            }
            k += 1;
        }
        Ok(())
    }

    /// S7 单源分派（§5/D2——B 面，cfg s7）：mask=0 零操作直返；GEOM 位
    /// → 节点重排脏 + 旧界与父链各界 damage（经 dmg_clip_add 交
    /// viewport——D17 单源）；PAINT 位 → paint 脏保位 + 本界 damage。
    ///
    /// 与 §5 伪码的一处有据差异（§11-6 钦定钉板反推）：**不预标脏祖
    /// 先**——上行传播改由 D4 截断承担（measure 返 changed：内在不变
    /// 的祖先不重测/不重摆）；父链在此只读收集 damage。预标脏即全测，
    /// 「孙 GEOM 变、中间聚合内在吸收 → 顶层 measured==0」不可能成立。
    #[cfg(feature = "s7")]
    fn apply_change(&mut self, idx: usize, mask: mask::ChangeMask) {
        if mask.is_empty() {
            return; // D2：mask=0 零操作（同 props/同文直返）
        }
        if mask.intersects(mask::ChangeMask::GEOM) {
            let old_r = self.abs_rect_of(idx);
            self.damage_rect(old_r); // 旧界
            let mut p = self.nodes.get(idx).and_then(|o| o.as_ref()).map(|n| n.parent).unwrap_or(NONE);
            let mut hops = 0usize;
            while p != NONE && hops <= N {
                let pr = self.abs_rect_of(p as usize);
                self.damage_rect(pr); // 父链各界（只读收集——不标脏，见函数注）
                p = self.nodes.get(p as usize).and_then(|o| o.as_ref()).map(|n| n.parent).unwrap_or(NONE);
                hops += 1;
            }
            if let Some(n) = self.nodes.get_mut(idx).and_then(|o| o.as_mut()) {
                n.layout_dirty = true;
                n.paint_dirty = true;
            }
        }
        if mask.intersects(mask::ChangeMask::PAINT) {
            if let Some(n) = self.nodes.get_mut(idx).and_then(|o| o.as_mut()) {
                n.paint_dirty = true; // 保位（v1 写侧簿记——§1.2 保持不消费）
            }
            let r = self.abs_rect_of(idx);
            self.damage_rect(r); // 纯绘：同界重绘
        }
    }

    pub fn set_props(&mut self, node: NodeId, props: desc::Props) -> Result<(), UiError> {
        // 几何变→重排脏+damage 新旧界；纯色变→paint 脏（§6 注记）。
        let idx = self.slot_of(node)? as usize;
        // kind 失配拒面（set_text 同型）：冻结 UiError 族无 kind 失配变
        // 体——NodeBad 兼任 loud；防「Rect 槽被塞 Text props」静默错配
        // （paint 发射面按 kind 匹配 props，错配 = 永不发射/读错载荷）。
        let kind_ok = matches!(
            self.nodes[idx]
                .as_ref()
                .map(|n| (&n.kind, &props)),
            Some((ViewKind::Root, Props::Root))
                | Some((ViewKind::Rect, Props::Rect(_)))
                | Some((ViewKind::Text, Props::Text(_)))
                | Some((ViewKind::Col, Props::Col(_)))
                | Some((ViewKind::Row, Props::Row(_)))
                | Some((ViewKind::Pad, Props::Pad(_)))
                | Some((ViewKind::Image, Props::Image(_)))
                | Some((ViewKind::Icon, Props::Icon(_)))
                | Some((ViewKind::Grid, Props::Grid(_)))
                // 组件件（Phase 2 两批——命令面开口）：props 可经
                // set_props 更新（label/checked/value/active/thickness
                // 面）；Stack/Spacer 仍拒（align/flex 未消费——防半落
                // 状态），余组件件同拒（渲染/交互未接线）。
                | Some((ViewKind::Button, Props::Button(_)))
                | Some((ViewKind::Toggle, Props::Toggle(_)))
                | Some((ViewKind::Slider, Props::Slider(_)))
                | Some((ViewKind::Progress, Props::Progress(_)))
                | Some((ViewKind::Check, Props::Check(_)))
                | Some((ViewKind::Radio, Props::Radio(_)))
                | Some((ViewKind::Step, Props::Step(_)))
                | Some((ViewKind::Seg, Props::Seg(_)))
                | Some((ViewKind::Divider, Props::Divider(_)))
                // 波2 批 A（Tabs/Sbar/Badge/Scroll/List）：Tabs/Sbar/Badge
                // 开口（active/thumb/count/圆点面）；Scroll/List 开口
                //（offset/content_height/item_count——消费面滚动）。
                | Some((ViewKind::Tabs, Props::Tabs(_)))
                | Some((ViewKind::Sbar, Props::Sbar(_)))
                | Some((ViewKind::Badge, Props::Badge(_)))
                | Some((ViewKind::Scroll, Props::Scroll(_)))
                | Some((ViewKind::List, Props::List(_)))
                // 波2 批 C：Select 开口（selected/option_count 面）。
                | Some((ViewKind::Select, Props::Select(_)))
                // 波2 批 D：Input/Edit 开口（placeholder/max_len/type 面
                // ——内容走侧表 set_input_text）；Canvas 开口（paint_ops
                // 计数面）。
                | Some((ViewKind::Input, Props::Input(_)))
                | Some((ViewKind::Edit, Props::Edit(_)))
                | Some((ViewKind::Canvas, Props::Canvas(_)))
                // 波2 批 E：Refresh/SwipeItem/DatePick/TimePick 开口
                //（offset/reveal/日期时间面——滚轮/手势消费挂波3/4）。
                | Some((ViewKind::Refresh, Props::Refresh(_)))
                | Some((ViewKind::SwipeItem, Props::SwipeItem(_)))
                | Some((ViewKind::DatePick, Props::DatePick(_)))
                | Some((ViewKind::TimePick, Props::TimePick(_)))
                // 组件第三批（Tip/Collapse/Chip/Fab/Card 开口）：文本/
                // expanded/selected/扩展标签/elevation·corner_radius 面。
                | Some((ViewKind::Tip, Props::Tip(_)))
                | Some((ViewKind::Collapse, Props::Collapse(_)))
                | Some((ViewKind::Chip, Props::Chip(_)))
                | Some((ViewKind::Fab, Props::Fab(_)))
                | Some((ViewKind::Card, Props::Card(_)))
                // 批 F（末刀）：Gauge/Qr/DragReorder 开口（value/style、
                // module_count/双色、item_index/total_items 面）——全
                // 41 kind 命令面收口，无未开口件残余。
                | Some((ViewKind::Gauge, Props::Gauge(_)))
                | Some((ViewKind::Qr, Props::Qr(_)))
                | Some((ViewKind::DragReorder, Props::DragReorder(_)))
        );
        if !kind_ok {
            return Err(UiError::NodeBad);
        }
        // Grid props 先验（§4.2 拒面 set_props 侧——三面齐律）：零维 /
        // 缩格不容现有子 → NodeBad（先验后写，状态保持——kind 失配测
        // 同型纪律）。
        if let Props::Grid(g) = &props {
            if g.cols == 0 || g.rows == 0 {
                return Err(UiError::NodeBad);
            }
            let mut cnt = 0u32;
            let mut cur = self.nodes[idx].as_ref().map(|n| n.first_child).unwrap_or(NONE);
            let mut hops = 0usize;
            while cur != NONE && hops <= N {
                hops += 1;
                match self.nodes.get(cur as usize) {
                    Some(Some(c)) if c.alive => {
                        cnt += 1;
                        cur = c.next_sib;
                    }
                    _ => break,
                }
            }
            if (g.cols as u32 * g.rows as u32) < cnt {
                return Err(UiError::NodeBad);
            }
        }
        // 组件值域先验（Phase 2 两批）：Slider/Progress permille 域、
        // Step 刻度域、Seg 段域——add 面同律（widget_value_ok 单源）；
        // 违者 NodeBad loud。
        if !widget_value_ok(&props) {
            return Err(UiError::NodeBad);
        }
        let Some(old) = self.nodes[idx].as_ref().map(|n| n.props) else { return Err(UiError::NodeBad) };
        // B 面（S7 段E）：props_diff 位映射（§5 表单源）→ apply_change
        // 单源分派（GEOM/PAINT/mask=0 三路——D2）；A 面现状体下方
        // cfg(not) 块逐字保持（§10：同文/同 props 的现状浪费保留）。
        #[cfg(feature = "s7")]
        {
            let m = mask::props_diff(&old, &props);
            if let Some(n) = self.nodes[idx].as_mut() {
                n.props = props;
            }
            self.apply_change(idx, m);
            Ok(())
        }
        #[cfg(not(feature = "s7"))]
        {
            let geom_same = props_geom_equal(&old, &props);
            let old_r = self.abs_rect_of(idx);
            if let Some(n) = self.nodes[idx].as_mut() {
                n.props = props;
                n.paint_dirty = true;
            }
            if geom_same {
                self.damage_rect(old_r); // 纯着色：同界重绘
            } else {
                self.damage_rect(old_r); // 旧界
                let p = self.nodes[idx].as_ref().map(|n| n.parent).unwrap_or(NONE);
                if p != NONE {
                    let pr = self.abs_rect_of(p as usize);
                    self.damage_rect(pr); // 兄弟位移面（新界由 layout 收）
                    self.mark_ancestors_dirty(p);
                }
                if let Some(n) = self.nodes[idx].as_mut() {
                    n.layout_dirty = true;
                }
            }
            Ok(())
        }
    }

    pub fn set_text(&mut self, node: NodeId, bytes: &[u8]) -> Result<(), UiError> {
        let idx = self.slot_of(node)? as usize;
        let buf = desc::TextBuf::new(bytes)?;
        // 冻结 UiError 族无 kind 失配变体——NodeBad 兼任 wrong-kind loud。
        let is_text = matches!(
            self.nodes[idx].as_ref().map(|n| (&n.kind, &n.props)),
            Some((ViewKind::Text, Props::Text(_)))
        );
        if !is_text {
            return Err(UiError::NodeBad);
        }
        // B 面（S7 段E）：D3 三态（§11 术语）——同文（字节相等）→ 零操
        // 作直返；等长换文（len 与 scale 均同；set_text 面恒同 scale）
        // → TEXT|BG 纯 PAINT；变长 → TEXT|BG|W|H（内在变 = GEOM 追加）
        // → apply_change 单源分派。A 面现状体下方 cfg(not) 块逐字保持
        // （无旧值比较——同文全量 relayout 浪费保留，§10 诚实基线）。
        // 比例供件改造（痛点⑥ 03 号 §1.2-4 架构注记收口）：等长不再
        // 蕴含等宽（"iii"≠"WWW"）——等长分支经度量缝比对内在宽，等宽才
        // 走纯 PAINT；Monospace/三档点阵下等长 ⇒ 宽恒等（advance 缺省 =
        // cell_w），判据退化为 v1 行为零漂移（R2-8 钉板不松）。
        #[cfg(feature = "s7")]
        {
            let old = match self.nodes[idx].as_ref().map(|n| &n.props) {
                Some(Props::Text(tp)) => (tp.text, tp.style.scale),
                _ => return Err(UiError::NodeBad), // unreachable（is_text 已验）——loud 防御
            };
            let m = if old.0.as_bytes() == buf.as_bytes() {
                mask::ChangeMask::NONE // 同文：零操作（D2）
            } else if old.0.len == buf.len {
                // 等长换文：内在宽比对（比例供件下等长可不等宽——宽变
                // 即 GEOM 追加重测；等宽供件下恒等 = v1 纯 PAINT 原状）。
                let sc = crate::desc::scale_to_u8(old.1);
                let same_w = self.tm.measure(old.0.as_bytes(), sc)
                    == self.tm.measure(buf.as_bytes(), sc);
                if same_w {
                    mask::ChangeMask::TEXT.union(mask::ChangeMask::BG) // 等长等宽：纯 PAINT
                } else {
                    mask::ChangeMask::TEXT
                        .union(mask::ChangeMask::BG)
                        .union(mask::ChangeMask::W)
                        .union(mask::ChangeMask::H) // 等长不等宽：内在变 → GEOM
                }
            } else {
                mask::ChangeMask::TEXT
                    .union(mask::ChangeMask::BG)
                    .union(mask::ChangeMask::W)
                    .union(mask::ChangeMask::H) // 变长：内在变 → GEOM 追加
            };
            if !m.is_empty() {
                if let Some(n) = self.nodes[idx].as_mut() {
                    if let Props::Text(tp) = &mut n.props {
                        tp.text = buf;
                    }
                }
                self.apply_change(idx, m);
            }
            Ok(())
        }
        #[cfg(not(feature = "s7"))]
        {
            let old_r = self.abs_rect_of(idx);
            self.damage_rect(old_r); // 旧界（内在尺寸变）
            let p = self.nodes[idx].as_ref().map(|n| n.parent).unwrap_or(NONE);
            if p != NONE {
                let pr = self.abs_rect_of(p as usize);
                self.damage_rect(pr);
                self.mark_ancestors_dirty(p);
            }
            if let Some(n) = self.nodes[idx].as_mut() {
                if let Props::Text(tp) = &mut n.props {
                    tp.text = buf;
                }
                n.layout_dirty = true;
            }
            Ok(())
        }
    }

    pub fn invalidate(&mut self, node: NodeId) -> Result<(), UiError> {
        // 视图级重绘（old+new 界进 damage——bounds 含 ox/oy，当前界即两者锚）。
        let idx = self.slot_of(node)? as usize;
        let r = self.abs_rect_of(idx);
        self.damage_rect(r);
        if let Some(n) = self.nodes[idx].as_mut() {
            n.paint_dirty = true;
        }
        Ok(())
    }

    /// offset 公开面（GUI-6 D31）：内部走 anim Transform 同路径——直写
    /// ox/oy 不重排 + 新旧**子树包络** damage（经 D17 clip helper 收口）
    /// ——SwipePager 跟手与 app 直控共用，无第三套 dirty 语义。
    pub fn set_offset(&mut self, node: NodeId, ox: i32, oy: i32) -> Result<(), UiError> {
        let idx = self.slot_of(node)? as usize;
        let before = anim::subtree_abs_rect(&self.nodes, idx, 0);
        if let Some(n) = self.nodes[idx].as_mut() {
            n.ox = ox;
            n.oy = oy;
            n.paint_dirty = true;
        }
        let after = anim::subtree_abs_rect(&self.nodes, idx, 0);
        self.damage_rect(before.envelope(&after));
        Ok(())
    }

    // —— a11y 运行期 API（GUI-6 D12 段D 生命周期②③——§7.1 D12 三面：apply 落表（段B）+ 本运行期面 + remove 连带清（上文）） ——

    /// a11y 条目登记（D12）：`node` 须活（slot_of 拒悬空/坏槽）；同节点
    /// 重复 set = 原槽覆写（in-place 更新，不占新槽）；表满 32 →
    /// [`UiError::NodeFull`] loud（D30 容量族——侧表满兼任「Ui 容量域满」）。
    /// 标签 > `TEXT_INLINE` → `TextTooLong` loud。
    pub fn set_a11y(&mut self, node: NodeId, role: u8, label: &[u8]) -> Result<(), UiError> {
        self.slot_of(node)?; // 活性先验（悬空 id 拒登记——防发射面残项）
        let buf = desc::TextBuf::new(label)?;
        if let Some(e) = self.a11y.iter_mut().find(|e| e.as_ref().is_some_and(|e| e.node == node)) {
            *e = Some(A11yEntry { node, role, label: buf });
            return Ok(()); // in-place 覆写：不占新槽（33 帽语义只数唯一节点）
        }
        let slot = match self.a11y.iter().position(|e| e.is_none()) {
            Some(i) => i,
            None => return Err(UiError::NodeFull),
        };
        self.a11y[slot] = Some(A11yEntry { node, role, label: buf });
        Ok(())
    }

    /// a11y 条目摘除（D12）：命中返 true；未登记/已摘返 false（幂等面）。
    pub fn take_a11y(&mut self, node: NodeId) -> bool {
        if let Some(e) = self.a11y.iter_mut().find(|e| e.as_ref().is_some_and(|e| e.node == node)) {
            *e = None;
            true
        } else {
            false
        }
    }

    /// a11y 活条目迭代（D12）：**活性过滤**——条目节点已死/代数不匹配的
    /// 残项跳过不吐（remove 连带清为尽力面，本过滤为发射面前卫：读侧
    /// 永不见悬空 id）。
    pub fn a11y_iter(&self) -> impl Iterator<Item = &A11yEntry> {
        self.a11y
            .iter()
            .filter_map(|e| e.as_ref())
            .filter(|e| self.slot_of(e.node).is_ok())
    }

    // —— press_map 声明式 pressed 短路径（GUI-6 D18——§7.1） ——

    /// pressed 视觉映射登记（D18）：`entries` = `(trigger, visual)` 对；
    /// **整表替换**语义（幂等重建面）。帽 [`PRESS_MAP_MAX`] = 16，超 →
    /// [`UiError::NodeFull`] loud（D30 容量族单源——§6 冻结注释「帽 16，
    /// 超 = NodeFull」；冻结 UiError 族无侧表变体，NodeFull 兼任）。
    /// trigger/visual 均须活节点；visual 须 Rect/Text（bg 承载面）——违
    /// 者 `NodeBad` loud（先验后写，半落拒绝不污染现表）。
    pub fn set_press_map(&mut self, entries: &[(NodeId, NodeId)]) -> Result<(), UiError> {
        if entries.len() > PRESS_MAP_MAX {
            return Err(UiError::NodeFull);
        }
        // 先验后写：全对子过验再落表（任一拒 = 原表原样）。
        for &(trig, vis) in entries {
            self.slot_of(trig)?;
            let idx = self.slot_of(vis)? as usize;
            let visual_ok = matches!(
                self.nodes[idx].as_ref().map(|n| n.kind),
                // 波2 批 A：Button 入 visual 白名单——Filled 底的 press
                // 暗化走 PressedVis 查表（paint 面），saved_bg 换底臂
                // 对 Button 不动作（视觉单源在 paint，非 props）。
                Some(ViewKind::Rect)
                    | Some(ViewKind::Text)
                    | Some(ViewKind::Button)
                    | Some(ViewKind::Check)
                    | Some(ViewKind::Radio)
                    | Some(ViewKind::Tabs)
                    | Some(ViewKind::Seg)
                    | Some(ViewKind::Step)
            );
            if !visual_ok {
                return Err(UiError::NodeBad);
            }
        }
        for (i, &(trig, vis)) in entries.iter().enumerate() {
            // active=false 起步：登记面不预换底（Press 事件才换）；
            // saved_bg 于首次 Press 时记。
            self.press_map[i] = PressEntry {
                trigger: trig,
                visual: vis,
                saved_bg: None,
                active: false,
            };
        }
        self.press_map_n = entries.len();
        Ok(())
    }

    /// pump 期 pressed 反应（D18）：Press → visual 换 [`PRESSED_BG`] 底
    /// （记原底色）；Release/Cancel → 还原（Text 透明底 = 移除 bg）。走
    /// 现状 `set_props` 语义（同帧 damage/dirty 生效——主线零依赖 B 面
    /// mask）。零闭包（ADR #52）：表驱动直查，无回调。
    fn press_react(&mut self, m: Msg) {
        match m {
            Msg::Press { node, .. } => {
                let Some(e) = Self::press_entry_of(&self.press_map[..self.press_map_n], node) else {
                    return;
                };
                if e.active {
                    return; // 二次 Press：保首 Press 的 saved_bg（还原链不断）
                }
                let vis = e.visual;
                let idx = match self.slot_of(vis) {
                    Ok(i) => i as usize,
                    Err(_) => return, // visual 已死：跳过（还原义务随节点消亡解除）
                };
                // 读现 bg → 记 saved_bg → 换 PRESSED_BG（set_props 同帧生效）。
                // 波2 批 A：组件件（Button 等）视觉单源在 paint（PressedVis
                // 查表换 ACCENT_PRESSED）——此处不换 props，只置 active +
                // 本界 damage（下一帧 paint 拾取暗化）。
                let is_widget = matches!(
                    self.nodes[idx].as_ref().map(|n| n.kind),
                    Some(ViewKind::Button)
                        | Some(ViewKind::Check)
                        | Some(ViewKind::Radio)
                        | Some(ViewKind::Tabs)
                        | Some(ViewKind::Seg)
                        | Some(ViewKind::Step)
                );
                if is_widget {
                    let r = self.abs_rect_of(idx);
                    self.damage_rect(r);
                    if let Some(e) = Self::press_entry_of_mut(
                        &mut self.press_map[..self.press_map_n],
                        node,
                    ) {
                        e.saved_bg = None;
                        e.active = true;
                    }
                    return;
                }
                let saved = match self.nodes[idx].as_ref().map(|n| n.props) {
                    Some(Props::Rect(rp)) => Some(pack_color(rp.bg)),
                    Some(Props::Text(tp)) => tp.style.bg.map(pack_color),
                    _ => return, // 登记后 kind 被换（异常）：跳过不动
                };
                let new_props = match self.nodes[idx].as_ref().map(|n| n.props) {
                    Some(Props::Rect(mut rp)) => {
                        rp.bg = unpack_color(PRESSED_BG);
                        Props::Rect(rp)
                    }
                    Some(Props::Text(mut tp)) => {
                        tp.style.bg = Some(unpack_color(PRESSED_BG));
                        Props::Text(tp)
                    }
                    _ => return,
                };
                if self.set_props(vis, new_props).is_ok() {
                    if let Some(e) = Self::press_entry_of_mut(
                        &mut self.press_map[..self.press_map_n],
                        node,
                    ) {
                        e.saved_bg = saved;
                        e.active = true;
                    }
                }
            }
            Msg::Release { node, .. } | Msg::Cancel { node } => {
                let Some(e) = Self::press_entry_of(&self.press_map[..self.press_map_n], node)
                else {
                    return;
                };
                if !e.active {
                    return; // 未换底（登记后未 Press）：还原面无动作
                }
                let vis = e.visual;
                let saved = e.saved_bg;
                let Ok(idx) = self.slot_of(vis) else {
                    // visual 已死：还原义务解除（仅清 active 位防重入）。
                    if let Some(e) = Self::press_entry_of_mut(
                        &mut self.press_map[..self.press_map_n],
                        node,
                    ) {
                        e.active = false;
                        e.saved_bg = None;
                    }
                    return;
                };
                let new_props = match self.nodes[idx as usize].as_ref().map(|n| n.props) {
                    Some(Props::Rect(mut rp)) => {
                        rp.bg = unpack_color(saved.unwrap_or(PRESSED_BG));
                        Props::Rect(rp)
                    }
                    Some(Props::Text(mut tp)) => {
                        // None = 原透明底——还原 = 移除 bg
                        tp.style.bg = saved.map(unpack_color);
                        Props::Text(tp)
                    }
                    // 波2 批 A：组件件视觉在 paint（PressedVis）——Release
                    // 只清 active + 本界 damage（还原由下一帧 paint 拾取）。
                    Some(Props::Button(_))
                    | Some(Props::Check(_))
                    | Some(Props::Radio(_))
                    | Some(Props::Tabs(_))
                    | Some(Props::Seg(_))
                    | Some(Props::Step(_)) => {
                        let r = self.abs_rect_of(idx as usize);
                        self.damage_rect(r);
                        if let Some(e) = Self::press_entry_of_mut(
                            &mut self.press_map[..self.press_map_n],
                            node,
                        ) {
                            e.active = false;
                            e.saved_bg = None;
                        }
                        return;
                    }
                    _ => return, // 登记后 kind 被换（异常）：跳过
                };
                if self.set_props(vis, new_props).is_ok() {
                    if let Some(e) = Self::press_entry_of_mut(
                        &mut self.press_map[..self.press_map_n],
                        node,
                    ) {
                        e.active = false;
                        e.saved_bg = None;
                    }
                }
            }
            Msg::Click { .. } | Msg::DragStart { .. } | Msg::DragMove { .. }
            | Msg::DragEnd { .. } => {}
        }
    }

    /// 表查（immutable 面）：首个 trigger 命中的条目副本。
    // —— 波2 批 D：输入焦点 + 文本内容面 ——

    /// 设置输入焦点（None = 清焦点）。目标须 Input/Edit（他 kind NodeBad
    /// loud——焦点域立法）；置焦同帧 damage 本界（焦点环视觉）。
    pub fn set_focus(&mut self, node: Option<NodeId>) -> Result<(), UiError> {
        match node {
            None => {
                if let Some(old) = self.focus {
                    if let Ok(i) = self.slot_of(old) {
                        self.damage_rect(self.abs_rect_of(i));
                    }
                }
                self.focus = None;
                Ok(())
            }
            Some(id) => {
                let idx = self.slot_of(id)? as usize;
                let ok = matches!(
                    self.nodes[idx].as_ref().map(|n| n.kind),
                    Some(ViewKind::Input) | Some(ViewKind::Edit)
                );
                if !ok {
                    return Err(UiError::NodeBad);
                }
                if let Some(old) = self.focus {
                    if old != id {
                        if let Ok(i) = self.slot_of(old) {
                            self.damage_rect(self.abs_rect_of(i));
                        }
                    }
                }
                self.focus = Some(id);
                self.damage_rect(self.abs_rect_of(idx));
                Ok(())
            }
        }
    }

    /// 当前焦点节点（无焦点 None）。
    pub fn focus(&self) -> Option<NodeId> {
        self.focus
    }

    /// 读输入内容（侧表——props 只载约束）。无内容空切片。
    pub fn input_text(&self, node: NodeId) -> Option<&[u8]> {
        self.input_bufs[..self.input_buf_n]
            .iter()
            .find_map(|s| match s {
                Some((id, tb)) if *id == node => Some(tb.as_bytes()),
                _ => None,
            })
    }

    /// 写输入内容（侧表 upsert——add/update；max_len 域执法在此）。非
    /// Input/Edit kind → NodeBad loud；表满新键 → NodeFull loud。
    pub fn set_input_text(&mut self, node: NodeId, bytes: &[u8]) -> Result<(), UiError> {
        let idx = self.slot_of(node)? as usize;
        let ok = matches!(
            self.nodes[idx].as_ref().map(|n| n.kind),
            Some(ViewKind::Input) | Some(ViewKind::Edit)
        );
        if !ok {
            return Err(UiError::NodeBad);
        }
        let max = match self.nodes[idx].as_ref().map(|n| &n.props) {
            Some(Props::Input(p)) => p.max_len,
            Some(Props::Edit(p)) => p.max_len,
            _ => return Err(UiError::NodeBad),
        };
        if bytes.len() as u32 > max || bytes.len() > TEXT_INLINE {
            return Err(UiError::NodeBad); // 帽越域 loud（max_len 与 TextBuf 内联帽取小）
        }
        let tb = desc::TextBuf::new(bytes)?;
        // upsert
        for e in self.input_bufs[..self.input_buf_n].iter_mut() {
            if let Some((id, slot)) = e {
                if *id == node {
                    *slot = tb;
                    let r = self.abs_rect_of(idx);
                    self.damage_rect(r);
                    return Ok(());
                }
            }
        }
        if self.input_buf_n >= INPUT_TEXT_MAX {
            return Err(UiError::NodeFull);
        }
        self.input_bufs[self.input_buf_n] = Some((node, tb));
        self.input_buf_n += 1;
        let r = self.abs_rect_of(idx);
        self.damage_rect(r);
        Ok(())
    }

    /// 焦点槽值（paint 焦点环查表单源——None = u32::MAX）。
    fn focus_slot(&self) -> u32 {
        match self.focus {
            Some(id) => id.0 & 0xFFFF,
            None => u32::MAX,
        }
    }

    /// 输入内容快照（批 D：侧表 → paint 消费面）。
    fn input_texts_snapshot(&self) -> paint::InputTexts {
        let mut t = paint::InputTexts::empty();
        for e in self.input_bufs[..self.input_buf_n].iter().flatten() {
            if t.n < t.entries.len() {
                t.entries[t.n] = Some((e.0 .0 & 0xFFFF, e.1));
                t.n += 1;
            }
        }
        t
    }

    /// pressed 视觉活动集快照（波2 批 A：press_map active 条目的 visual
    /// 槽值——paint 期 Button 等 Filled 底换 ACCENT_PRESSED 的查表面；
    /// 复制进 paint::PressedVis 定容小表，n ≤ PRESS_MAP_MAX 结构保证）。
    fn pressed_snapshot(&self) -> paint::PressedVis {
        let mut pv = paint::PressedVis::empty();
        for e in &self.press_map[..self.press_map_n] {
            if e.active {
                let slot = e.visual.0 & 0xFFFF;
                if pv.n < pv.slots.len() {
                    pv.slots[pv.n] = slot;
                    pv.n += 1;
                }
            }
        }
        pv
    }

    fn press_entry_of(map: &[PressEntry], node: NodeId) -> Option<PressEntry> {
        map.iter().find(|e| e.trigger == node).copied()
    }

    /// 表查（mutable 面）。
    fn press_entry_of_mut(map: &mut [PressEntry], node: NodeId) -> Option<&mut PressEntry> {
        map.iter_mut().find(|e| e.trigger == node)
    }

    // —— 组件四件交互反应（Phase 2 首批——pump 期消费，D18 press_react 同形） ——

    /// 组件事件派发收口：满帽计 [`Self::lost_widget_events`] loud
    /// （MsgBuf::push → Gesture::lost_msgs 同形——不自陷不静默丢）。
    fn fire_widget_event(&mut self, node: NodeId, event: u32, value: u32) {
        if self
            .widget_events
            .push(WidgetEvent { node, event, value })
            .is_err()
        {
            self.lost_widget_events = self.lost_widget_events.saturating_add(1);
        }
    }

    /// 组件事件消费面（app update 侧）：**take 语义**——取走即清（与
    /// drain_damage 同形；未取事件留存至下帧，pump 期继续追加）。
    pub fn take_events(&mut self) -> EventBuf {
        core::mem::replace(&mut self.widget_events, EventBuf::new())
    }

    /// 事件缓冲满被丢的组件事件计数（loud 诊断面——宿主/测试读取）。
    pub fn lost_widget_events(&self) -> u32 {
        self.lost_widget_events
    }

    /// 拖动横坐标 → Slider permille（行程映射）：行程 = [x+圆半径,
    /// x+w−圆半径]（圆半径 = [`SLIDER_KNOB_D`]/2——与 paint 渲染同式
    /// 单源），界外钳 0/1000；i64 中间量防链加法溢出，商域 0..=1000。
    /// 布局前（w=0 防御）恒 0。
    fn slider_permille_at(&self, idx: usize, x: u32) -> u32 {
        let r = self.abs_rect_of(idx);
        let inset = (SLIDER_KNOB_D / 2) as i64;
        let usable = (r.w as i64 - 2 * inset).max(1);
        let rel = (x as i64 - r.x as i64 - inset).clamp(0, usable);
        // rel ≤ usable ≤ 2^31 → rel×1000 不溢 i64；钳域保证 0..=1000。
        (rel * PERMILLE_MAX as i64 / usable) as u32
    }

    /// 组件交互反应（Phase 2 两批——pump_input 期逐 Msg 即时消费——
    /// D18 press_react 同形同拍，Msg: Copy 副本出借不持 buf 引用）：
    /// - Click 落 **Button** → 派发 on_tap（value=0）；
    /// - Click 落 **Toggle** → 翻转 checked（经 set_props 单漏斗——
    ///   damage/dirty 同帧生效，视觉滑块随下次 paint 换位）+ 派发
    ///   on_change（value = 翻转后 0/1）；
    /// - Click 落 **Check** → 翻转 checked + 派发 on_change（Toggle
    ///   同形，value = 翻转后 0/1）；
    /// - Click 落 **Radio** → 置 selected + 派发 on_change（value 恒
    ///   1 置位语义；组互斥执法在 app 面——group 仅数据携载）；
    /// - Click 落 **Collapse** → 翻转 expanded + 派发 on_toggle（第三
    ///   批；value = 翻转后 0/1——收起折叠/展开经 set_props 单漏斗同
    ///   帧生效，子摆位随下次 layout 增减）；
    /// - Click 落 **Chip** → 翻转 selected + 派发 on_tap（第三批；
    ///   Toggle 同形）；
    /// - Click 落 **Fab** → 派发 on_tap（第三批；Button 同形 value=0
    ///   ——无状态件）；
    /// - DragStart/DragMove 落 **Slider** → 跳跟/跟手按压横坐标（行程
    ///   映射 [`Self::slider_permille_at`]，值变才写才派发 on_change）；
    /// - DragStart/DragMove 落 **DragReorder** → 派发 on_reorder（批 F；
    ///   value = 拖拽纵坐标 wire 原值——v1 记录 drag 位置，落位/交换语
    ///   义挂波3/波4 消费面）；
    /// - Release 落 **Step** → 击点横坐标定半区（左半减/右半加 step，
    ///   clamp min/max）+ 值变才派发 on_change（Slider 同形）；
    /// - Release 落 **Seg** → 击点横坐标折段索引 + 值变（≠ active）
    ///   才写才派发 on_change。
    ///
    /// Step/Seg 挂 **Release** 臂而非 Click：分段命中需要击点坐标，
    /// 而 `Msg::Click` 无坐标面（desc.rs 冻结不加字段）——Release 与
    /// Click 在非拖 up 路径同拍成对双发且携 x/y，坐标消费面挂此臂零
    /// 新面。
    ///
    /// 零闭包（ADR #52）：表驱动直查无回调；**不劫持 Msg 面**——
    /// Click/Drag 原样交 app（app 亦可自管状态双消费，幂等面文档化）。
    fn widget_react(&mut self, m: Msg) {
        match m {
            Msg::Click { node } => {
                let idx = match self.slot_of(node) {
                    Ok(i) => i as usize,
                    Err(_) => return,
                };
                match self.nodes[idx].as_ref().map(|n| n.props) {
                    Some(Props::Button(p)) => {
                        let on_tap = p.on_tap;
                        self.fire_widget_event(node, on_tap, 0);
                    }
                    Some(Props::Input(_)) | Some(Props::Edit(_)) => {
                        // 波2 批 D：点击置焦（焦点环/caret 随焦）——值面
                        // 无事件（on_* props 未载）；再击幂等。
                        let _ = self.set_focus(Some(node));
                    }
                    Some(Props::Select(p)) => {
                        // 波2 批 C：Select 点击循环下一项（末项回 0；
                        // 空选择器零操作）。展开面板挂波3/波4 消费面。
                        if p.option_count == 0 {
                            return;
                        }
                        let next = if p.selected + 1 >= p.option_count {
                            0
                        } else {
                            p.selected + 1
                        };
                        let on_change = p.on_change;
                        let np = Props::Select(desc::SelectProps { selected: next, ..p });
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_change, next);
                        }
                    }
                    Some(Props::Toggle(p)) => {
                        let checked = !p.checked;
                        let on_change = p.on_change;
                        let np = Props::Toggle(desc::ToggleProps { checked, on_change });
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_change, checked as u32);
                        }
                    }
                    Some(Props::Check(p)) => {
                        let checked = !p.checked;
                        let on_change = p.on_change;
                        let np = Props::Check(desc::CheckProps { checked, on_change });
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_change, checked as u32);
                        }
                    }
                    Some(Props::Radio(p)) => {
                        let on_change = p.on_change;
                        let np = Props::Radio(desc::RadioProps {
                            selected: true,
                            group: p.group,
                            on_change,
                        });
                        // 恒派发（置位语义）：已选再击仍是合法 on_change
                        // ——组互斥的「选谁」裁决在 app。
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_change, 1);
                        }
                    }
                    // 组件第三批（组合交互三件）：Collapse 翻转 expanded +
                    // on_toggle（value = 翻转后 0/1——几何面收起折叠/展开
                    // 经 set_props 单漏斗同帧生效）；Chip 翻转 selected +
                    // on_tap（Toggle 同形）；Fab 纯动作 on_tap（Button 同
                    // 形，value=0——无状态件）。
                    Some(Props::Collapse(p)) => {
                        let expanded = !p.expanded;
                        let on_toggle = p.on_toggle;
                        let np = Props::Collapse(desc::CollapseProps { expanded, on_toggle });
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_toggle, expanded as u32);
                        }
                    }
                    Some(Props::Chip(p)) => {
                        let selected = !p.selected;
                        let on_tap = p.on_tap;
                        let np =
                            Props::Chip(desc::ChipProps { label: p.label, selected, on_tap });
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_tap, selected as u32);
                        }
                    }
                    Some(Props::Fab(p)) => {
                        self.fire_widget_event(node, p.on_tap, 0);
                    }
                    _ => {}
                }
            }
            Msg::Release { node, x, .. } => {
                let idx = match self.slot_of(node) {
                    Ok(i) => i as usize,
                    Err(_) => return,
                };
                match self.nodes[idx].as_ref().map(|n| n.props) {
                    Some(Props::Step(p)) => {
                        let r = self.abs_rect_of(idx);
                        // 半区判据 = 击点与节点水平中线（i64 中间量同
                        // slider_permille_at 纪律）。
                        let left = (x as i64) < r.x as i64 + r.w as i64 / 2;
                        let nv = if left {
                            p.value.saturating_sub(p.step).max(p.min)
                        } else {
                            p.value.saturating_add(p.step).min(p.max)
                        };
                        if nv == p.value {
                            return; // clamp 贴边：值未变零操作（Slider 同形）
                        }
                        let on_change = p.on_change;
                        let np = Props::Step(desc::StepProps { value: nv, ..p });
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_change, nv);
                        }
                    }
                    Some(Props::Tabs(p)) => {
                        // 波2 批 A：Tabs 点击切页——击点横坐标折段索引
                        //（Seg 同形）；active 未变零操作。on_switch 派发
                        //（value = 新 active）。
                        let r = self.abs_rect_of(idx);
                        let cnt = p.tab_count as i64;
                        let tw = (r.w as i64 / cnt).max(1);
                        let rel = (x as i64 - r.x as i64).max(0);
                        let tab = (rel / tw).min(cnt - 1) as u32;
                        if tab == p.active {
                            return;
                        }
                        let on_switch = p.on_switch;
                        let np = Props::Tabs(desc::TabsProps { active: tab, ..p });
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_switch, tab);
                        }
                    }
                    Some(Props::Seg(p)) => {
                        let r = self.abs_rect_of(idx);
                        let cnt = p.segment_count as i64;
                        // 段宽 = w/段数（整除向下）；段索引 = 相对横坐标
                        // 折段 + 越界钳尾段（sw=0 防御折 0 段——widget_
                        // value_ok 保证 count>0，w=0 布局前面）。
                        let sw = (r.w as i64 / cnt).max(1);
                        let rel = (x as i64 - r.x as i64).max(0);
                        let seg = (rel / sw).min(cnt - 1) as u32;
                        if seg == p.active {
                            return; // 值未变：零操作（Slider 同形）
                        }
                        let on_change = p.on_change;
                        let np = Props::Seg(desc::SegProps { active: seg, ..p });
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_change, seg);
                        }
                    }
                    _ => {}
                }
            }
            Msg::DragStart { node, x, y, .. } | Msg::DragMove { node, x, y, .. } => {
                let idx = match self.slot_of(node) {
                    Ok(i) => i as usize,
                    Err(_) => return,
                };
                match self.nodes[idx].as_ref().map(|n| n.props) {
                    Some(Props::Slider(p)) => {
                        let v = self.slider_permille_at(idx, x);
                        if v == p.value {
                            return; // 值未变：零操作（不重排不派发）
                        }
                        let on_change = p.on_change;
                        let np = Props::Slider(desc::SliderProps { value: v, ..p });
                        if self.set_props(node, np).is_ok() {
                            self.fire_widget_event(node, on_change, v);
                        }
                    }
                    // 批 F：DragReorder 拖拽记录——on_reorder(value = 拖拽
                    // 纵坐标 wire 原值)逐样本派发；无 props 写面（item_
                    // index 不随手势变——落位裁决在 app，重排挂消费面）。
                    Some(Props::DragReorder(p)) => {
                        if p.total_items == 0 {
                            return; // 空表手柄：零操作（Select 零项同律）
                        }
                        self.fire_widget_event(node, p.on_reorder, y);
                    }
                    _ => {}
                }
            }
            Msg::Press { .. } | Msg::DragEnd { .. } | Msg::Cancel { .. } => {}
        }
    }

    // —— 供给槽（D28：门面 layout/render_frame 签名不变——槽取传下） ——

    /// 文本度量供件换装（默认 [`MonospaceTextMetrics`]；换件后脏位重测
    /// 生效——set_viewport 触发全量重测）。
    pub fn set_text_metrics(&mut self, tm: &'static dyn TextMetrics) {
        self.tm = tm;
    }

    /// 图标/图片资源供件换装（默认 [`EmptyIconSource`]——dims 恒 None）。
    pub fn set_icon_source(&mut self, src: &'static dyn IconSource) {
        self.icons = src;
    }

    pub fn bounds(&self, node: NodeId) -> Result<damage::RectI, UiError> {
        // 绝对界（含 ox/oy）。
        let idx = self.slot_of(node)? as usize;
        Ok(self.abs_rect_of(idx))
    }

    pub fn kind_of(&self, node: NodeId) -> Result<desc::ViewKind, UiError> {
        let idx = self.slot_of(node)? as usize;
        match self.nodes[idx].as_ref() {
            Some(n) => Ok(n.kind),
            None => Err(UiError::NodeBad),
        }
    }

    // —— 布局 / 脏区 ——

    pub fn layout(&mut self) -> layout::LayoutStat {
        // 脏子树重排；新界进 damage（freed = 当前空闲槽清单——removal 面
        // 的陈旧缓存跳过依据）。GUI-6 D28：tm/icons 从供给槽取传（门面
        // 签名不变）；D9：v1 只接 Viewport 档。
        layout::relayout(
            &mut self.nodes,
            self.vp_w,
            self.vp_h,
            &mut self.damage,
            &self.free_stack[..self.free_count],
            self.tm,
            self.icons,
            layout::Constraints::Viewport,
        )
    }

    pub fn damage(&self) -> &damage::DamageSet {
        &self.damage
    }

    pub fn drain_damage(&mut self) -> damage::DamageSet {
        core::mem::replace(&mut self.damage, damage::DamageSet::new())
    }

    // —— 渲染（DisplayList 'a 钉调用方 scratch，不钉引擎） ——

    pub fn paint<'a>(
        &self,
        scratch: &'a mut [GlyphPos],
        enc: &mut CommandEncoder<'a>,
    ) -> paint::PaintReport {
        if self.root_slot == NONE {
            return paint::PaintReport { cmds: 0, drawn_px: 0, culled: 0, overdraw_x256: 0 };
        }
        paint::paint(
            &self.nodes,
            self.root_slot,
            (self.vp_w, self.vp_h),
            &self.damage,
            scratch,
            enc,
            self.icons, // D28 槽取传（门面签名不变）
            &self.pressed_snapshot(),
            self.focus_slot(),
            &self.input_texts_snapshot(),
        )
    }

    // —— 输入 ——

    pub fn push_pointer(&mut self, ev: hit::PtrEvent) -> Result<(), UiError> {
        // 缓冲 ≤INPUT_BUF_MAX（满 → MsgFull loud——不自陷不静默丢）。
        if self.input_n >= INPUT_BUF_MAX {
            return Err(UiError::MsgFull);
        }
        self.input_buf[self.input_n] = Some(ev);
        self.input_n += 1;
        Ok(())
    }

    pub fn pump_input(&mut self, expected_present_ns: u64) -> desc::MsgBuf {
        // 帧头：referee + 重采样 + 消费（P7 输入同拍）。
        let mut out = desc::MsgBuf::new();
        let target = resample::target_from_expected(expected_present_ns);
        let n = self.input_n;
        let mut i = 0usize;
        while i < n {
            let Some(mut ev) = self.input_buf[i].take() else {
                i += 1;
                continue;
            };
            if ev.ts_ns > self.last_input_ts_ns {
                self.last_input_ts_ns = ev.ts_ns;
            }
            let mut did_resample = false;
            if ev.phase == PHASE_MOVE {
                // 活跃 MOVE 滑窗维护（非单调/跨度断裂由 resample 内部拒）。
                if self.hist_len >= INPUT_BUF_MAX {
                    self.move_hist.copy_within(1..INPUT_BUF_MAX, 0);
                    self.hist_len = INPUT_BUF_MAX - 1;
                }
                self.move_hist[self.hist_len] =
                    Sample { ts_ns: ev.ts_ns, x: ev.x as i64, y: ev.y as i64 };
                self.hist_len += 1;
                // D15 速度环入环点：resample **替换前**的原始 (ts,x,y)
                //（resampled 样本不入环——防速度低估；§7.1）。
                self.gesture.note_sample(
                    ev.pointer_id,
                    ev.ts_ns,
                    hit::u32_to_i32_sat(ev.x),
                    hit::u32_to_i32_sat(ev.y),
                );
                let s = {
                    let h: &[Sample] = &self.move_hist[..self.hist_len];
                    resample::resample_to_target(h, target)
                };
                if let Some(smp) = s {
                    // 目标推导 = expected − LATENCY（Android 同式，§3.9
                    // ——单源 resample::target_from_expected）；重采样坐
                    // 标替换入事件。
                    ev.x = clamp_u32(smp.x);
                    ev.y = clamp_u32(smp.y);
                    did_resample = true;
                }
            } else {
                self.hist_len = 0; // 流断裂（DOWN/UP/CANCEL）——窗清零
            }
            if self.root_slot != NONE {
                let n_before = out.len();
                hit::dispatch(&mut self.gesture, &self.nodes, self.root_slot, ev, &mut out);
                // `resampled` 位标注（§3.9 引擎侧义务——PE_FL_RESAMPLED
                // 的引擎内对应面）：重采样路径产物 DragMove 置 true。本
                // 事件恰产一条 Msg（MOVE 至多一条）且满帽拒收（lost_msgs）
                // 时 n 无增——漏标无害：Msg 未产出。
                if did_resample && out.len() == n_before + 1 {
                    if let Some(Msg::DragMove { resampled, .. }) = out.last_mut() {
                        *resampled = true;
                    }
                }
                // D18 pressed 短路径 + 组件交互反应（Phase 2 两批）：
                // 本事件产出的 Msg 即时消费（Press 换底 / Release·Cancel
                // 还原；Button on_tap / Toggle·Check 翻转 / Radio 置位 /
                // Slider 跳跟 / Step·Seg 击点分段——走现状 set_props 语
                // 义同帧生效，主线零依赖 B 面 mask）。
                let mut k = n_before;
                while k < out.len() {
                    if let Some(m) = out.at(k) {
                        self.press_react(m);
                        self.widget_react(m);
                    }
                    k += 1;
                }
            }
            i += 1;
        }
        self.input_n = 0;
        out
    }

    pub fn node_at(&self, x: u32, y: u32) -> Option<NodeId> {
        // topmost 优先——单源委托 hit::node_at（z 序逆走 = 兄弟链尾侧先
        // 探；公共查询与 pump 消费两入口单一结论；环/死槽防御同源）。
        hit::node_at(&self.nodes, self.root_slot, hit::u32_to_i32_sat(x), hit::u32_to_i32_sat(y))
    }

    // —— 动画 ——

    pub fn anim_add(&mut self, a: anim::Anim) -> Result<(), UiError> {
        let mut i = 0usize;
        while i < ANIM_MAX {
            if self.anims[i].is_none() {
                self.anims[i] = Some(a);
                return Ok(());
            }
            i += 1;
        }
        Err(UiError::AnimFull)
    }

    pub fn anim_clear(&mut self) {
        let mut i = 0usize;
        while i < ANIM_MAX {
            self.anims[i] = None;
            i += 1;
        }
    }

    pub fn anim_active(&self) -> bool {
        self.anims.iter().any(|a| a.is_some())
    }

    // —— 帧循环 ——

    pub fn set_vsync(&mut self, period_ns: u64, phase_ns: u64) {
        // 周期 = 协商倒数；phase = UI/合成错相注入位（对齐条 12）。
        self.vsync_period_ns = period_ns;
        self.vsync_phase_ns = phase_ns;
    }

    pub fn vsync(&self) -> (u64, u64) {
        (self.vsync_period_ns, self.vsync_phase_ns)
    }

    fn rate_hz(&self) -> u8 {
        if self.vsync_period_ns == 0 {
            return 0;
        }
        let hz = 1_000_000_000u64 / self.vsync_period_ns;
        if hz > 255 {
            255
        } else {
            hz as u8
        }
    }

    pub fn render_frame<'a>(
        &mut self,
        now_ns: u64,
        scratch: &'a mut [GlyphPos],
        enc: &mut CommandEncoder<'a>,
    ) -> frame::FrameOutcome {
        // doFrame 序列（§3.8）：anim tick → layout → paint → 遥测 → 相位推进。
        // D17：anim damage 经 viewport clip——tick 收 vp_w/vp_h。
        // B 面（S7 段E D1）：Layout 型动画产 (slot, mask) 入 staging——
        // tick 后、relayout 前统一 apply_change 分派（帧内时序锚），三
        // 漏斗 dirty/damage 无第三套语义；A 面 tick 直写现状保持。
        #[cfg(feature = "s7")]
        let mut staging = anim::AnimStaging::new();
        #[cfg(feature = "s7")]
        let anim_on = anim::tick(
            &mut self.anims,
            &mut self.nodes,
            &mut self.damage,
            now_ns,
            self.vp_w,
            self.vp_h,
            &mut staging,
        );
        #[cfg(not(feature = "s7"))]
        let anim_on = anim::tick(
            &mut self.anims,
            &mut self.nodes,
            &mut self.damage,
            now_ns,
            self.vp_w,
            self.vp_h,
        );
        #[cfg(feature = "s7")]
        for k in 0..staging.n {
            let (slot, m) = staging.items[k];
            self.apply_change(slot as usize, m);
        }
        let _stat = layout::relayout(
            &mut self.nodes,
            self.vp_w,
            self.vp_h,
            &mut self.damage,
            &self.free_stack[..self.free_count],
            self.tm,
            self.icons,
            layout::Constraints::Viewport,
        );
        let drew = !self.damage.is_empty(); // 无 damage 不提交（对齐条 6）
        let report = if drew && self.root_slot != NONE {
            paint::paint(
                &self.nodes,
                self.root_slot,
                (self.vp_w, self.vp_h),
                &self.damage,
                scratch,
                enc,
                self.icons, // D28 槽取传
                &self.pressed_snapshot(),
                self.focus_slot(),
                &self.input_texts_snapshot(),
            )
        } else {
            paint::PaintReport { cmds: 0, drawn_px: 0, culled: 0, overdraw_x256: 0 }
        };
        let dmg = core::mem::replace(&mut self.damage, damage::DamageSet::new());
        // expected 轨单源 frame.rs（拍沿含沿不欠拍；phase mod 归一容忍
        // 敌意 phase ≥ period）。
        let expected = frame::next_vsync_tick(now_ns, self.vsync_period_ns, self.vsync_phase_ns);
        // 起振延迟遥测（增件②——Weston「extra vblank」可观测）：
        // 首帧晚过 wake 锚 tick 即迟到。
        if let Some(anchor) = self.spinup_anchor.take() {
            if now_ns > anchor {
                self.telem.bump_spinup_late();
            }
        }
        let rec = telemetry::FrameRec {
            seq: self.seq,
            input_ts_ns: self.last_input_ts_ns,
            frame_begin_ns: now_ns,
            frame_end_ns: now_ns, // 引擎无钟——单时刻帧；真耗时 embedder 双打点（GUI-9）
            expected_ns: expected,
            actual_ns: 0, // actual 轨 note_presented 回填
            presented: false,
            discarded: false,
            damage_full: dmg.full,
            damage_rects: dmg.count,
            damage_adds: dmg.adds,
            relayout_ns: 0, // 引擎无钟——host bench 双打点（见字段注）
            overdraw_x256: report.overdraw_x256,
            jank: false,
        };
        self.telem.record(rec);
        self.last_frame_seq = self.seq;
        self.last_expected_ns = expected;
        self.seq = self.seq.wrapping_add(1);
        if drew || anim_on || self.input_n > 0 {
            self.last_work_ns = now_ns;
        }
        let idle_ms = if now_ns >= self.last_work_ns {
            (now_ns - self.last_work_ns) / 1_000_000
        } else {
            0
        };
        let sig = frame::FrameSignals {
            damage: drew,
            anim: anim_on,
            input: self.input_n > 0,
            present_pending: self.inflight_n > 0, // 在途帧回调未清 → SETTLE
        };
        self.phase = frame::next_phase(self.phase, sig, idle_ms);
        self.last_now_ns = now_ns;
        if !drew {
            // 跳过帧（damage 空）不提交（对齐条 6）——rate_hint 诚实填
            // 0：无提交的帧无帧率话语权（frame.rs 字段注/对齐条 12）。
            return frame::FrameOutcome::skipped(expected);
        }
        frame::FrameOutcome { drew, damage: dmg, expected_present_ns: expected, rate_hint_hz: self.rate_hz() }
    }

    pub fn frame_phase(&self) -> frame::FramePhase {
        self.phase
    }

    pub fn next_wakeup(&self) -> Option<u64> {
        // None = IDLE（撤软 vsync 定时器、睡事件——R3 对策）。单源委托
        // frame::next_wakeup：Run = min(vsync 拍点, 动画期限, 强降闹钟)、
        // Settle = 强降闹钟（在途回调迟到的兜底醒——§3.8）。
        let anim_deadline = self
            .anims
            .iter()
            .flatten()
            .map(|a| a.start_ns.saturating_add(a.dur_ns))
            .min();
        frame::next_wakeup(
            self.phase,
            self.last_now_ns,
            self.last_work_ns,
            self.vsync_period_ns,
            self.vsync_phase_ns,
            anim_deadline,
        )
    }

    pub fn wake(&mut self, now_ns: u64) {
        // IDLE→Run 立即渲不欠拍 + 起振遥测。
        if self.phase == frame::FramePhase::Idle {
            self.telem.bump_idle_exits();
        }
        self.telem.bump_wakes();
        self.phase = frame::FramePhase::Run;
        self.last_work_ns = now_ns;
        self.last_now_ns = now_ns;
        // 下一帧必有产出（不欠拍）——**条件化 FULL**：已有脏区时保留既有
        // 矩形集（增量 wake 不把局部脏放大成整屏；空集才置 FULL——修
        // 复轮 finding 9：无条件 FULL 会吞掉先前 invalidate 的精确脏区）。
        if self.damage.is_empty() {
            self.damage.add_full();
        }
        self.spinup_anchor =
            Some(frame::next_vsync_tick(now_ns, self.vsync_period_ns, self.vsync_phase_ns));
    }

    pub fn note_submitted(&mut self) {
        // credits 簿记单源 frame.rs（≤CREDITS_MAX——不轰炸，对齐条 8）。
        // 门返回值丢弃：note_submitted 是「已提交」事后记账（提交与否的
        // 门由 embedder 提交前自查——backpressure 是义务不是建议）；帽
        // 满时 outstanding 不再增，漂移由 note_presented 应答重同步收口
        // （显示服务报的剩余信用是真值源）。
        let _ = self.credits.submit();
        if self.inflight_n < self.inflight.len() {
            self.inflight[self.inflight_n] = (self.last_frame_seq, self.last_expected_ns);
            self.inflight_n += 1;
        }
    }

    pub fn note_presented(&mut self, seq: u32, presented_ns: u64, status: u8, credits_remaining: u8) {
        // actual 轨 + SETTLE→IDLE 确认 + jank 判定（超半周期 deadline_miss）。
        let mut found = None;
        let mut i = 0usize;
        while i < self.inflight_n {
            if self.inflight[i].0 == seq {
                found = Some(i);
                break;
            }
            i += 1;
        }
        if let Some(i) = found {
            let expected = self.inflight[i].1;
            let deadline = expected.checked_add(self.vsync_period_ns / 2);
            let jank = match deadline {
                Some(d) => presented_ns > d,
                None => false,
            };
            self.telem.fill_presented(
                seq,
                presented_ns,
                status == FRAME_PRESENTED,
                status == FRAME_DISCARDED,
                jank,
            );
            self.inflight_n -= 1;
            self.inflight[i] = self.inflight[self.inflight_n]; // swap-remove（序无义）
        }
        // 背压重同步：显示服务报的**剩余信用**是真值源（outstanding =
        // MAX − remaining；未知应答不猜测——对齐条 8）。
        self.credits.on_presented(credits_remaining);
    }

    pub fn vote_hint(&self) -> frame::VoteBallot {
        // 票面单源 frame::ballot_for（场景定值表 §3.8；仲裁在显示服务
        // ——对齐条 12，产票不仲裁）。三信号：动画表活跃 / 手势拖拽进
        // 行中（interacting 第三信号——非拖按住不算）/ 帧循环 Idle。
        frame::ballot_for(
            self.anim_active(),
            self.gesture.any_dragging(),
            self.phase == frame::FramePhase::Idle,
        )
    }

    // —— AST 双入口（入口②） ——

    pub fn encode_desc(&self, out: &mut [u8]) -> Result<usize, desc::AstError> {
        // 无根树 → Ok(0)（空流；apply 空流 Truncated 拒——空树不可重建根）。
        // a11y 表透传（D12——A11Y 块唯一发射源）。
        desc::encode_tree(&self.nodes, self.root_slot, &self.a11y, out)
    }

    pub fn apply_desc(&mut self, src: &[u8]) -> Result<(), desc::AstError> {
        // 整树重建 + FULL damage；拒面零半执行（失败重建空场）。
        // reset_arena 已清 a11y 表（满帽拒绝的半落条目随重建归零）。
        self.reset_arena();
        match desc::apply_tree(src, &mut self.nodes, &mut self.gens, &mut self.root_slot, &mut self.a11y)
        {
            Ok(()) => {
                self.damage.add_full();
                Ok(())
            }
            Err(e) => {
                self.reset_arena();
                Err(e)
            }
        }
    }

    // —— 依赖骨架（v1 记录不消费——失效仍视图级 Invalidate，裁决不跳步） ——

    pub fn dep_bind(&mut self, node: NodeId, dep: dep::DepId) -> Result<(), UiError> {
        // 单源委托 dep::DepTable：同边重复 bind 幂等 Ok（不重复占槽）、
        // 满帽 DepFull loud（§3.10）；liveness 校验在本入口。
        self.slot_of(node)?; // 悬空 id loud
        self.deps.bind(node, dep)
    }

    pub fn dep_dependents(&self, dep: dep::DepId, out: &mut [NodeId]) -> usize {
        // 表侧查询 + liveness 过滤；返回**存活依赖总数**（可 > out.len()
        // ——调用方以返回值判截断，零分配哨兵面不静默缩水）。
        let mut tmp = [NodeId(0); DEP_EDGES_MAX];
        let total = self.deps.dependents(dep, &mut tmp);
        let mut n = 0usize;
        for k in 0..total {
            let nid = tmp[k];
            if self.slot_of(nid).is_ok() {
                if n < out.len() {
                    out[n] = nid;
                }
                n += 1;
            }
        }
        n
    }

    pub fn telemetry(&self) -> &telemetry::Telemetry {
        &self.telem
    }
}
