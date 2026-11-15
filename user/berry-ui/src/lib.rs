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
pub mod diagnostic;
pub mod desc;
pub mod frame;
pub mod hit;
pub mod layout;
#[cfg(feature = "s7")]
pub mod mask;
pub mod metrics;
pub mod navigation;
pub mod pull_refresh;
pub mod nav_stack;
pub mod gesture;
pub mod paint;
pub mod resample;
pub mod sdk;
pub mod sem;
pub mod swipe;
pub mod telemetry;
pub mod text_input;
pub mod text_wrap;
pub mod widgets;
pub mod theme;
pub mod utf8;
// arena 内嵌本文件（Node/槽表私有——§6）；NodeId 在 desc.rs 公开。

// 扁平面 re-export：门面签名里不限定模块路径的名字（§6 用法对齐）。
pub use desc::{AstError, Axis, Msg, MsgBuf, NodeId, Props, TextBuf, ViewKind, ViewSpec};
pub use sem::{a11y_region_roundtrip, SemEntry, SEM_ACTION_ACTIVATE, SEM_ACTION_ADJUST, SEM_ACTION_FOCUS, SEM_ACTION_TEXT_EDIT, SEM_LABEL_MAX, SEM_MAX, SEM_STATE_CHECKED, SEM_STATE_DISABLED, SEM_STATE_SELECTED};
pub use metrics::{EmptyIconSource, IconSource, MonospaceTextMetrics, TextMetrics};

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_guiproto::surface::{encode_a11y_region, A11yRegion, A11Y_ACTION_FOCUS, A11Y_ROLE_GAME_REGION, A11Y_STATE_FOCUSED, A11Y_STATE_VISIBLE};
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

/// arena 运行期树深帽（GUI-6 D29）：add() 执法（超深 → NodeBad loud）
/// ——递归面（measure/place/cull/emit）未迭代化，深链 × ~200B/帧穿
/// 32KB guest 栈；96×~200B + 数组面 < 32KB 安全。与 D11 builder 帽
/// DESC_DEPTH_MAX=64 分工（构建期 vs 运行期——64 < 96 自洽）。
pub const TREE_DEPTH_MAX: u32 = 96;

// ---------- a11y 侧表（GUI-6 D12 段B 最小落位） ----------

/// a11y 侧表容量（D12：a11y 条目数帽——超 → apply 面 TooSmall loud）。
pub const A11Y_MAX: usize = 32;

/// a11y 角色常量域（wire u8；§6 冻结——枚举外的开放值域，host 可扩）。
/// P0 起值域单源转发 BerrySession semantic sidecar：BerryUI 可扩展导出，
/// 但 0..=5 的既有语义不得另抄第二份。
pub const ROLE_NONE: u8 = berry_guiproto::surface::A11Y_ROLE_NONE;
pub const ROLE_BUTTON: u8 = berry_guiproto::surface::A11Y_ROLE_BUTTON;
pub const ROLE_TEXT: u8 = berry_guiproto::surface::A11Y_ROLE_TEXT;
pub const ROLE_IMAGE: u8 = berry_guiproto::surface::A11Y_ROLE_IMAGE;
pub const ROLE_TOGGLE: u8 = berry_guiproto::surface::A11Y_ROLE_TOGGLE;
pub const ROLE_LIST_ITEM: u8 = berry_guiproto::surface::A11Y_ROLE_LIST_ITEM;

/// a11y 条目（D12：Node 冻结 19 字段无 a11y 位——侧表存 NodeId + 角色
/// + 内联标签）。`node` 的 gen 位与 arena 三重对拍（发射面防悬空残项）。
/// 完整生命周期（增删查 pub API）= 段D；段B 只落 apply/encode 数据面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct A11yEntry {
    pub node: NodeId,
    pub role: u8,
    pub label: TextBuf,
}

// ---------- component focus 侧表（GUI-14 C1） ----------

/// component focusable 容量（与 a11y 侧表同帽；超表 → NodeFull loud）。
pub const FOCUS_MAX: usize = 32;

/// 可聚焦组件登记项：`node` 是语义焦点锚（Button/Switch panel），`visual`
/// 是焦点变化时需要 invalidate 的视觉承载节点；禁用项保留在表中但不参与
/// 导航，便于同一 widget 从 Disabled 恢复后无需重建顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusEntry {
    pub node: NodeId,
    pub visual: Option<NodeId>,
    pub enabled: bool,
}

// ---------- flex ratio 侧表（GUI-14 L2） ----------

/// flex weight 侧表容量；超表 → NodeFull loud（D30 容量族）。
pub const FLEX_MAX: usize = 32;

/// 主轴比例分配登记项。叶子面 = Rect/Circle 双 None；L3 transposed
/// pane 面 = Row 中 Col/Grid/Panel 主轴宽 None、Col 中 Row 主轴高 None。
/// 使用前提是父 Row/Col 主轴已有确定终值。未固定时：叶子内在为 0 则
/// 塌缩；有内在的 pane 会按权重重分其内在总量（不 shrink），不是拒面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlexEntry {
    pub node: NodeId,
    /// L2 兼容字段名；L6a 起等价于 `FlexSpec::grow`。
    pub weight: u16,
    pub shrink: u16,
    pub basis: FlexBasis,
}

/// L6a flex 基准：`Auto` 读子主轴内在，`Px` 是确定基准。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexBasis {
    Auto,
    Px(u32),
}

/// L6a runtime-only flex 规格。`grow` 只消费正余量；`shrink` 只消费负
/// 余量；两者可独立存在，但全零是死登记，写入面 loud 拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlexSpec {
    pub grow: u16,
    pub shrink: u16,
    pub basis: FlexBasis,
}

// ---------- LAYOUT-L5 text wrap 侧表 ----------

/// text wrap 侧表容量；超表 → NodeFull loud（D30 容量族）。
pub const TEXT_WRAP_MAX: usize = 32;

/// runtime-only Text wrap 登记：不进 Node/Props/AST/wire。`last_max_w`
/// 由最近一次 measure walk 写入，是 paint 复用同一 wrap plan 的缓存；
/// 调用方只声明 `enabled`，宽上界来自 per-node incoming BoxConstraints。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextWrapEntry {
    pub node: NodeId,
    pub enabled: bool,
    pub last_max_w: u32,
}

// ---------- press_map 侧表（GUI-6 D18——段D） ----------

/// press_map 容量（D18：声明式 pressed-state 映射帽；超 = NodeFull loud
/// ——D30 容量族单源立法：UiError 无侧表满变体，NodeFull 兼任「Ui 容量
/// 域满」，与 a11y 满 32 / arena 满同族）。
pub const PRESS_MAP_MAX: usize = 16;

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
    /// 运行期字形供件槽。默认 host 面用静态图集；guest-font-vmo 档由
    /// Berry Room 注入系统 A8 VMO，避免 app 复制 CJK/HD 字表。
    fonts: Option<&'static dyn metrics::FontSource>,
    a11y: [Option<A11yEntry>; A11Y_MAX], // a11y 侧表（D12——apply 落/encode 读）
    sem: [Option<SemEntry>; SEM_MAX], // SEM-A1 runtime-only 语义旁表（不进 AST/wire）
    focus: [Option<FocusEntry>; FOCUS_MAX], // component focusable 侧表
    focused: Option<NodeId>, // 当前 component focus（窗口焦点仍在 wmd）
    flex: [Option<FlexEntry>; FLEX_MAX], // L2 主轴比例侧表
    wrap: [Option<TextWrapEntry>; TEXT_WRAP_MAX], // L5 runtime-only wrap 侧表
    press_map: [PressEntry; PRESS_MAP_MAX], // pressed 短路径表（D18——pump 期消费）
    press_map_n: usize,
}

/// D28 默认供件（host 测试与 v1 guest 的开箱面——Monospace 逐位复刻
/// 现状 Text 测量 / 图标资源未注册恒 None）。
static DEFAULT_TEXT_METRICS: MonospaceTextMetrics = MonospaceTextMetrics;
static DEFAULT_ICON_SOURCE: EmptyIconSource = EmptyIconSource;

// —— 内部小工具（零 panic：全 checked/饱和/先验界内） ——

fn pack_id(idx: usize, gen: u16) -> NodeId {
    NodeId(((gen as u32) << 16) | ((idx as u32) & 0xFFFF))
}

fn i32_to_u16_region(value: i32) -> Result<u16, UiError> {
    if value < 0 || value > u16::MAX as i32 {
        return Err(UiError::NodeBad);
    }
    Ok(value as u16)
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

/// props 几何等价判定（纯着色变 → 0 重排——Measure/Render 保留的执法点）。
/// 段B 新面：Col/Row/Grid 的 w/h/cross 入几何（块字段全比）；Image res/
/// w/h、Icon id/w/h 全比；Icon fg 不比（纯着色——paint 位换色不动布局）。
/// S7 B 面无消费者（props_diff 位映射接管）——A 面（cfg not s7）独占。
#[cfg_attr(feature = "s7", allow(dead_code))]
fn props_geom_equal(a: &Props, b: &Props) -> bool {
    match (a, b) {
        (Props::Root, Props::Root) => true,
        (Props::Clip(a), Props::Clip(b)) => a.w == b.w && a.h == b.h,
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
        (Props::Panel(x), Props::Panel(y)) => {
            x.padding == y.padding && x.gap == y.gap && x.w == y.w && x.h == y.h
        }
        (Props::Image(x), Props::Image(y)) => x.res == y.res && x.w == y.w && x.h == y.h,
        (Props::Icon(x), Props::Icon(y)) => x.id == y.id && x.w == y.w && x.h == y.h,
        _ => false, // 变体切换按几何变处理（保守重排）
    }
}

/// RUN-K1 重应用统计：一次 keyed 调和的值级证据面。`rederives` 是本
/// 次调用内树身份/几何推导轮数；当前实现单轮完成，恒 1。未来若引入
/// re-entrant derive，必须保持 <= [`SETTLE_DERIVE_MAX`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReapplyStat {
    pub matched_clean: u32,
    pub updated: u32,
    pub moved: u32,
    pub added: u32,
    pub removed: u32,
    pub rederives: u32,
    pub fallback_full: bool,
}

/// RUN-K1 settle cap。GUI-15 E7 的上限不是给全树重建留余地，而是禁止
/// 交互帧内无限 derive；当前单轮调和必须一次 settle。
pub const SETTLE_DERIVE_MAX: u32 = 3;

/// RUN-K1 只读调和计划。所有数组按 const N 定容，零分配；计划完成后
/// 才允许修改 arena。
struct KeyPlan<const N: usize> {
    source_to_target: [u32; N],
    source_add: [bool; N],
    source_moved: [bool; N],
    target_keep: [bool; N],
    target_remove: [bool; N],
    matched_clean: u32,
    updated: u32,
    moved: u32,
    added: u32,
    removed: u32,
}

/// RUN-K1 keyed 通道的实验期 source scratch 帽。`apply_desc` full oracle
/// 仍支持 MAX_NODES=512；keyed 面先用固定 128 节点/24 层的小栈切片落地，
/// 防止把 D29 32KB guest 栈预算扩大到 embedder 才发现溢出。
const KEYED_SOURCE_MAX: usize = 128;
pub const KEYED_DEPTH_MAX: u32 = 24;

fn source_max_depth<const N: usize>(nodes: &[Option<Node>]) -> Result<u32, desc::AstError> {
    let mut max_depth = 0u32;
    for (_slot, option) in nodes.iter().enumerate() {
        let Some(node) = option.as_ref() else { continue };
        if !node.alive {
            continue;
        }
        let mut depth = 1u32;
        let mut parent = node.parent;
        let mut hops = 0usize;
        while parent != NONE && hops <= N {
            hops += 1;
            let Some(parent_node) = nodes.get(parent as usize).and_then(|o| o.as_ref()) else {
                return Err(desc::AstError::Truncated);
            };
            depth += 1;
            parent = parent_node.parent;
        }
        max_depth = max_depth.max(depth);
    }
    Ok(max_depth)
}

impl<const N: usize> KeyPlan<N> {
    fn new() -> Self {
        Self {
            source_to_target: [NONE; N],
            source_add: [false; N],
            source_moved: [false; N],
            target_keep: [false; N],
            target_remove: [false; N],
            matched_clean: 0,
            updated: 0,
            moved: 0,
            added: 0,
            removed: 0,
        }
    }
}

fn mark_source_add<const N: usize>(
    nodes: &[Option<Node>],
    slot: u32,
    plan: &mut KeyPlan<N>,
) -> u32 {
    let Some(node) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else {
        return 0;
    };
    if plan.source_add[slot as usize] {
        return 0;
    }
    plan.source_add[slot as usize] = true;
    let mut count = 1u32;
    let mut cur = node.first_child;
    let mut hops = 0usize;
    while cur != NONE && hops <= N {
        hops += 1;
        let next = nodes
            .get(cur as usize)
            .and_then(|o| o.as_ref())
            .map(|n| n.next_sib)
            .unwrap_or(NONE);
        count += mark_source_add(nodes, cur, plan);
        cur = next;
    }
    count
}

fn mark_target_remove<const N: usize>(
    nodes: &[Option<Node>],
    slot: u32,
    plan: &mut KeyPlan<N>,
) -> u32 {
    let Some(node) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else {
        return 0;
    };
    if plan.target_remove[slot as usize] {
        return 0;
    }
    plan.target_remove[slot as usize] = true;
    let mut count = 1u32;
    let mut cur = node.first_child;
    let mut hops = 0usize;
    while cur != NONE && hops <= N {
        hops += 1;
        let next = nodes
            .get(cur as usize)
            .and_then(|o| o.as_ref())
            .map(|n| n.next_sib)
            .unwrap_or(NONE);
        count += mark_target_remove(nodes, cur, plan);
        cur = next;
    }
    count
}

fn plan_keyed_node<const N: usize>(
    source: &[Option<Node>],
    source_slot: u32,
    target: &[Option<Node>],
    target_slot: u32,
    plan: &mut KeyPlan<N>,
) -> Result<(), desc::AstError> {
    // 逐链扫描而不落 [u32; N] 子清单：D29 guest 栈预算按每层约 200B 设
    // 计；若每层复制三个 N 长数组，N=512/depth=96 会放大到数百 KB。
    // target_keep 即“本父下已匹配”标记，复用计划状态且消除死字段。
    let source_head = source
        .get(source_slot as usize)
        .and_then(|o| o.as_ref())
        .map(|n| n.first_child)
        .unwrap_or(NONE);
    let target_head = target
        .get(target_slot as usize)
        .and_then(|o| o.as_ref())
        .map(|n| n.first_child)
        .unwrap_or(NONE);

    let mut source_cur = source_head;
    let mut source_i = 0usize;
    let mut source_hops = 0usize;
    while source_cur != NONE && source_hops <= N {
        source_hops += 1;
        let Some(child) = source.get(source_cur as usize).and_then(|o| o.as_ref()) else {
            return Err(desc::AstError::Truncated);
        };
        let next = child.next_sib;
        let child_slot = source_cur;
        let child_key = child.key;

        let mut matched = false;
        if child_key != 0 {
            let mut target_cur = target_head;
            let mut target_i = 0usize;
            let mut target_hops = 0usize;
            while target_cur != NONE && target_hops <= N {
                target_hops += 1;
                let Some(candidate) =
                    target.get(target_cur as usize).and_then(|o| o.as_ref())
                else {
                    break;
                };
                let candidate_next = candidate.next_sib;
                let candidate_slot = target_cur;
                let already_matched =
                    plan.target_keep.get(candidate_slot as usize).copied().unwrap_or(true);
                if !already_matched && candidate.key == child_key {
                    if candidate.kind != child.kind {
                        return Err(desc::AstError::KeyKindMismatch { key: child_key });
                    }
                    plan.source_to_target[child_slot as usize] = candidate_slot;
                    plan.target_keep[candidate_slot as usize] = true;
                    let changed = candidate.props != child.props;
                    if changed {
                        plan.updated += 1;
                    }
                    if source_i != target_i {
                        plan.source_moved[child_slot as usize] = true;
                        plan.moved += 1;
                    } else if !changed {
                        plan.matched_clean += 1;
                    }
                    plan_keyed_node(source, child_slot, target, candidate_slot, plan)?;
                    matched = true;
                    break;
                }
                target_cur = candidate_next;
                target_i += 1;
            }
        }
        if !matched {
            plan.added += mark_source_add(source, child_slot, plan);
        }
        source_cur = next;
        source_i += 1;
    }

    let mut target_cur = target_head;
    let mut target_hops = 0usize;
    while target_cur != NONE && target_hops <= N {
        target_hops += 1;
        let Some(candidate) = target.get(target_cur as usize).and_then(|o| o.as_ref()) else {
            break;
        };
        let next = candidate.next_sib;
        let kept = plan.target_keep.get(target_cur as usize).copied().unwrap_or(false);
        if !kept {
            plan.removed += mark_target_remove(target, target_cur, plan);
        }
        target_cur = next;
    }
    Ok(())
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
            fonts: None,
            a11y: [None; A11Y_MAX],
            sem: [None; SEM_MAX],
            focus: [None; FOCUS_MAX],
            focused: None,
            flex: [None; FLEX_MAX],
            wrap: [None; TEXT_WRAP_MAX],
            press_map: [PressEntry {
                trigger: NodeId(0),
                visual: NodeId(0),
                saved_bg: None,
                active: false,
            }; PRESS_MAP_MAX],
            press_map_n: 0,
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
        self.sem = [None; SEM_MAX]; // SEM-A1 runtime-only 语义随整树重建清空
        self.focus = [None; FOCUS_MAX];
        self.focused = None;
        self.flex = [None; FLEX_MAX];
        self.wrap = [None; TEXT_WRAP_MAX];
        self.press_map_n = 0; // 同理整清（D18 表随树生命周期——悬空 id 无还原义）
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
            Some(ViewKind::Root) | Some(ViewKind::Col) | Some(ViewKind::Row) | Some(ViewKind::Pad)
            | Some(ViewKind::Grid) | Some(ViewKind::Panel) | Some(ViewKind::Clip) => {}
            // 容器白名单（Grid 入列——D24；Clip 入列——C1 结构性 port）
            // 叶四件显式列名（Rect/Text 原有 + Image/Icon 新叶）：容器位
            // 传叶子 → BadParent。Grid 格满/零维先验在 Root-子检后（段C
            // 已落——AST/builder/add/set_props 三面同律齐）。
            Some(ViewKind::Rect) | Some(ViewKind::Text) | Some(ViewKind::Image)
            | Some(ViewKind::Icon) => return Err(UiError::BadParent),
            _ => return Err(UiError::BadParent), // 死槽防御
        }
        if spec.kind == ViewKind::Root {
            return Err(UiError::BadParent); // Root 不能为子
        }
        // RUN-K1：同父非零 key 唯一。builder 可分次构造，冲突在最低公共
        // 入口先行拒绝；AST 面由 desc::build_node 单源同律执法。
        if spec.key != 0 {
            let mut cur = self.nodes[pidx].as_ref().map(|n| n.first_child).unwrap_or(NONE);
            let mut hops = 0usize;
            while cur != NONE && hops <= N {
                hops += 1;
                let Some(n) = self.nodes.get(cur as usize).and_then(|o| o.as_ref()) else {
                    break;
                };
                if n.key == spec.key {
                    return Err(UiError::NodeBad);
                }
                cur = n.next_sib;
            }
        }
        // Grid spec 先验（§4.2 拒面 add 侧；AST/builder 面在 desc.rs 双
        // 拒）：零维 spec 拒——UiError 族冻结（D21），NodeBad 兼任。
        if let Props::Grid(g) = &spec.props {
            if g.cols == 0 || g.rows == 0 {
                return Err(UiError::NodeBad);
            }
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
        // SEM-A1 语义旁表同理：子树摘除/残项一并清，不悬挂。
        let mut k = 0usize;
        while k < SEM_MAX {
            let hit = self.sem[k]
                .as_ref()
                .map(|e| list[..cnt].contains(&(e.node.0 & 0xFFFF)))
                .unwrap_or(false);
            if hit {
                self.sem[k] = None;
            }
            k += 1;
        }
        // C1 focus 生命周期同理：子树语义锚清除；当前焦点若在被摘子树，
        // component focus 归空（窗口焦点是否保留由 wmd 单独裁决）。
        let mut k = 0usize;
        while k < FOCUS_MAX {
            let hit = self.focus[k]
                .as_ref()
                .map(|e| list[..cnt].contains(&(e.node.0 & 0xFFFF)))
                .unwrap_or(false);
            if hit {
                self.focus[k] = None;
            }
            k += 1;
        }
        if let Some(node) = self.focused {
            if list[..cnt].contains(&(node.0 & 0xFFFF)) {
                self.focused = None;
            }
        }
        // L2 flex 生命周期同理：子树登记随摘除清空；悬空残项在布局读侧
        // 也有 gen 对拍双保险。
        let mut k = 0usize;
        while k < FLEX_MAX {
            let hit = self.flex[k]
                .as_ref()
                .map(|e| list[..cnt].contains(&(e.node.0 & 0xFFFF)))
                .unwrap_or(false);
            if hit {
                self.flex[k] = None;
            }
            k += 1;
        }
        // L5 wrap 生命周期同理：子树登记随摘除清空；paint 只经活性过滤。
        let mut k = 0usize;
        while k < TEXT_WRAP_MAX {
            let hit = self.wrap[k]
                .as_ref()
                .map(|e| list[..cnt].contains(&(e.node.0 & 0xFFFF)))
                .unwrap_or(false);
            if hit {
                self.wrap[k] = None;
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
                | Some((ViewKind::Clip, Props::Clip(_)))
                | Some((ViewKind::Rect, Props::Rect(_)))
                | Some((ViewKind::Text, Props::Text(_)))
                | Some((ViewKind::Col, Props::Col(_)))
                | Some((ViewKind::Row, Props::Row(_)))
                | Some((ViewKind::Pad, Props::Pad(_)))
                | Some((ViewKind::Image, Props::Image(_)))
                | Some((ViewKind::Icon, Props::Icon(_)))
                | Some((ViewKind::Grid, Props::Grid(_)))
                | Some((ViewKind::Circle, Props::Circle(_)))
                | Some((ViewKind::Panel, Props::Panel(_)))
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
        // L2：flex 只允许双 None 的 Rect/Circle。props 离开 eligible
        // 形态时连带摘除，防“固定尺寸却携 weight”的静默无效登记。本块
        // 之后当前无再拒路径；若未来追加后验拒，需把摘除挪到最终写入
        // 成功后，保持半落不污染。
        let flex_eligible =
            layout::flex_eligible_props(&self.nodes, idx, &props);
        if !flex_eligible {
            let node = pack_id(idx, self.nodes[idx].as_ref().map(|n| n.gen).unwrap_or(0));
            let slot = self.flex.iter().position(|e| {
                e.as_ref().is_some_and(|e| e.node == node)
            });
            if let Some(slot) = slot {
                self.flex[slot] = None;
                self.mark_flex_dirty(node);
            }
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
            if m.intersects(mask::ChangeMask::GEOM) {
                self.mark_wrapped_descendants_dirty(idx as u32);
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
            if !geom_same {
                self.mark_wrapped_descendants_dirty(idx as u32);
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
        #[cfg(feature = "s7")]
        {
            let (old, scale) = match self.nodes[idx].as_ref().map(|n| &n.props) {
                Some(Props::Text(tp)) => (tp.text, tp.style.scale),
                _ => return Err(UiError::NodeBad), // unreachable（is_text 已验）——loud 防御
            };
            let scale_id = desc::scale_to_u8(scale);
            let old_size = self.tm.measure(old.as_bytes(), scale_id);
            let new_size = self.tm.measure(buf.as_bytes(), scale_id);
            let m = if old.as_bytes() == buf.as_bytes() {
                mask::ChangeMask::NONE // 同文：零操作（D2）
            } else if old.len == buf.len && old_size == new_size {
                // 等长且内在宽同（ASCII 等宽常态）：纯 PAINT。UTF-8 下等字节
                // 长不必然等码点数，故尺寸面入判（CJK 首刀）。
                mask::ChangeMask::TEXT.union(mask::ChangeMask::BG)
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
        self.damage_node_rect(idx, r);
        if let Some(n) = self.nodes[idx].as_mut() {
            n.paint_dirty = true;
        }
        Ok(())
    }

    /// Sparse translate damage for scrolling layers: record painted-leaf old/new
    /// bounds instead of promoting a transparent container subtree to a full
    /// port repaint. SwipePager uses this path for follow-finger and snap.
    /// Bands write straight into `self.damage` via y±2 merging
    /// (`DamageSet::add_row_band`) so two visible pages and repeated calls in
    /// one frame (multiple MOVE msgs per pump batch) merge instead of
    /// overflowing DAMAGE_MAX into FULL.
    pub fn set_offset_sparse(
        &mut self,
        node: NodeId,
        ox: i32,
        oy: i32,
    ) -> Result<(), UiError> {
        let idx = self.slot_of(node)? as usize;
        let (old_ox, old_oy) = {
            let n = self.nodes[idx].as_ref().ok_or(UiError::NodeBad)?;
            (n.ox, n.oy)
        };
        let (dx, dy) = (ox.saturating_sub(old_ox), oy.saturating_sub(old_oy));
        let old_clip = self.clip_chain(idx);
        Self::collect_sparse_offset(&self.nodes, idx, dx, dy, old_clip, &mut self.damage, 0);
        if let Some(n) = self.nodes[idx].as_mut() {
            n.ox = ox;
            n.oy = oy;
            n.paint_dirty = true;
        }
        let new_clip = self.clip_chain(idx);
        if new_clip != old_clip {
            Self::collect_sparse_offset(&self.nodes, idx, dx, dy, new_clip, &mut self.damage, 0);
        }
        Ok(())
    }

    /// Grid 页的稀疏平移 damage：Grid 直接子项按行带（y±2 合并）入
    /// `out`（= 调用方 `self.damage`——跨页/跨调用同带并包络，见
    /// `set_offset_sparse` 注）。SwipePager 的页是 Col→Grid→cells；
    /// launcher 网格行间大空白保持不传输，避免两个可见页都退化成整页
    /// 包络。非该形态回落 generic 包络。
    fn add_sparse_grid_page_move(
        nodes: &[Option<Node>],
        page_slot: usize,
        dx: i32,
        clip: damage::RectI,
        out: &mut damage::DamageSet,
    ) -> bool {
        let Some(page) = nodes.get(page_slot).and_then(|n| n.as_ref()) else {
            return false;
        };
        if page.kind != ViewKind::Col {
            return false;
        }
        let grid = page.first_child;
        let Some(grid_node) = nodes.get(grid as usize).and_then(|n| n.as_ref()) else {
            return false;
        };
        if grid == NONE || grid_node.kind != ViewKind::Grid {
            return false;
        }
        let mut cell = grid_node.first_child;
        let mut hops = 0usize;
        while cell != NONE {
            hops += 1;
            if hops > nodes.len() {
                return false;
            }
            let Some(cell_node) = nodes.get(cell as usize).and_then(|n| n.as_ref()) else {
                return false;
            };
            let next = cell_node.next_sib;
            let Some(r) = Self::painted_subtree_rect(nodes, cell as usize, 0) else {
                cell = next;
                continue;
            };
            let moved = damage::RectI::new(r.x.saturating_add(dx), r.y, r.w, r.h);
            let inside = r.envelope(&moved).intersect(&clip);
            if inside.is_empty() {
                cell = next;
                continue;
            }
            // 行带按 y±2 并入全集：同页同行 cell、跨页同带、同帧多次写
            // 都合成一条；独立带数超 DAMAGE_MAX 时 add_row_band 落
            // add_exact 帽 → 保守 FULL（不漏画方向）。
            out.add_row_band(inside.to_wire());
            cell = next;
        }
        true
    }

    fn collect_sparse_offset(
        nodes: &[Option<Node>],
        slot: usize,
        dx: i32,
        dy: i32,
        clip: damage::RectI,
        out: &mut damage::DamageSet,
        depth: usize,
    ) {
        if depth > nodes.len() {
            return;
        }
        let Some(node) = nodes.get(slot).and_then(|n| n.as_ref()) else {
            return;
        };
        if !node.alive {
            return;
        }
        // A scrolling track has few page children but many painted leaves.
        // Aggregate leaves per direct child before DamageSet's 8-rect cap;
        // otherwise a normal launcher page can degrade to FULL and defeat the
        // sparse path.
        if depth == 0 {
            let mut child = node.first_child;
            let mut hops = 0usize;
            while child != NONE {
                hops += 1;
                if hops > nodes.len() {
                    return;
                }
                let next = match nodes.get(child as usize).and_then(|n| n.as_ref()) {
                    Some(c) if c.alive => c.next_sib,
                    _ => return,
                };
                if Self::add_sparse_grid_page_move(nodes, child as usize, dx, clip, out) {
                    child = next;
                    continue;
                }
                if let Some(r) = Self::painted_subtree_rect(nodes, child as usize, 0) {
                    let moved = damage::RectI::new(
                        r.x.saturating_add(dx),
                        r.y.saturating_add(dy),
                        r.w,
                        r.h,
                    );
                    let inside = r.envelope(&moved).intersect(&clip);
                    out.add(inside.to_wire());
                }
                child = next;
            }
            return;
        }
        if matches!(
            node.kind,
            ViewKind::Rect
                | ViewKind::Text
                | ViewKind::Image
                | ViewKind::Icon
                | ViewKind::Circle
                | ViewKind::Panel
        ) {
            if let Some(r) = anim::abs_rect(nodes, slot) {
                let moved =
                    damage::RectI::new(r.x.saturating_add(dx), r.y.saturating_add(dy), r.w, r.h);
                let inside = r.envelope(&moved).intersect(&clip);
                out.add(inside.to_wire());
            }
        }
        let mut child = node.first_child;
        let mut hops = 0usize;
        while child != NONE {
            hops += 1;
            if hops > nodes.len() {
                return;
            }
            let next = match nodes.get(child as usize).and_then(|n| n.as_ref()) {
                Some(c) if c.alive => c.next_sib,
                _ => return,
            };
            Self::collect_sparse_offset(nodes, child as usize, dx, dy, clip, out, depth + 1);
            child = next;
        }
    }

    fn painted_subtree_rect(
        nodes: &[Option<Node>],
        slot: usize,
        depth: usize,
    ) -> Option<damage::RectI> {
        if depth > nodes.len() {
            return None;
        }
        let node = nodes.get(slot).and_then(|n| n.as_ref())?;
        if !node.alive {
            return None;
        }
        let mut acc = if matches!(
            node.kind,
            ViewKind::Rect
                | ViewKind::Text
                | ViewKind::Image
                | ViewKind::Icon
                | ViewKind::Circle
                | ViewKind::Panel
        ) {
            anim::abs_rect(nodes, slot)
        } else {
            None
        };
        let mut child = node.first_child;
        let mut hops = 0usize;
        while child != NONE {
            hops += 1;
            if hops > nodes.len() {
                return acc;
            }
            let next = match nodes.get(child as usize).and_then(|n| n.as_ref()) {
                Some(c) if c.alive => c.next_sib,
                _ => return acc,
            };
            if let Some(r) = Self::painted_subtree_rect(nodes, child as usize, depth + 1) {
                acc = Some(match acc {
                    Some(old) => old.envelope(&r),
                    None => r,
                });
            }
            child = next;
        }
        acc
    }

    /// offset 公开面（GUI-6 D31）：内部走 anim Transform 同路径——直写
    /// ox/oy 不重排 + 新旧**子树包络** damage（经 D17 clip helper 收口）
    /// ——SwipePager 跟手与 app 直控共用，无第三套 dirty 语义。
    pub fn set_offset(&mut self, node: NodeId, ox: i32, oy: i32) -> Result<(), UiError> {
        let idx = self.slot_of(node)? as usize;
        let before = anim::subtree_abs_rect(&self.nodes, idx, 0);
        let old_clip = self.clip_chain(idx);
        if let Some(n) = self.nodes[idx].as_mut() {
            n.ox = ox;
            n.oy = oy;
            n.paint_dirty = true;
        }
        let after = anim::subtree_abs_rect(&self.nodes, idx, 0);
        let new_clip = self.clip_chain(idx);
        let envelope = before.envelope(&after);
        // 结构性 Clip 内平移只需 port 内旧新包络；若被裁剪节点自身移动
        // 改变 port，则旧/新可见域分别入账（两矩形可能合并）。
        self.damage_rect(envelope.intersect(&old_clip));
        self.damage_rect(envelope.intersect(&new_clip));
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
            // SEM overlay 依赖 a11y 薄面的 role/label；薄面摘除时连带清，
            // 不留无单源的 resident 残项。
            let _ = self.take_sem(node);
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

    // —— SEM-A1 runtime-only 语义旁表（导出面，不进 AST/wire） ——

    /// 登记/覆写语义 overlay。节点必须已登记 legacy a11y（role/label
    /// 单源）；未知词表、FOCUS 存储位、非法 UTF-8、value>64 或 range
    /// 越界 loud 拒；表满 8 → NodeFull。
    pub fn set_sem(&mut self, entry: SemEntry) -> Result<(), UiError> {
        entry.validate()?;
        self.slot_of(entry.node)?;
        if !self.a11y_iter().any(|legacy| legacy.node == entry.node) {
            return Err(UiError::NodeBad);
        }
        if let Some(slot) = self
            .sem
            .iter_mut()
            .find(|e| e.as_ref().is_some_and(|e| e.node == entry.node))
        {
            *slot = Some(entry);
            return Ok(());
        }
        let Some(slot) = self.sem.iter().position(|e| e.is_none()) else {
            return Err(UiError::NodeFull);
        };
        self.sem[slot] = Some(entry);
        Ok(())
    }

    /// 摘除语义条目；幂等。
    pub fn take_sem(&mut self, node: NodeId) -> bool {
        let Some(entry) = self.sem.iter_mut().find(|e| {
            e.as_ref().is_some_and(|entry| entry.node == node)
        }) else {
            return false;
        };
        *entry = None;
        true
    }

    /// 语义条目活性迭代（登记序；快照投影另按树前序）。
    pub fn sem_iter(&self) -> impl Iterator<Item = &SemEntry> {
        self.sem
            .iter()
            .filter_map(|e| e.as_ref())
            .filter(|e| self.slot_of(e.node).is_ok())
    }

    /// 快照中的 P0 node_id（slot+1）。活性节点返回 id；悬空节点返回 None。
    pub fn semantic_snapshot_id(&self, node: NodeId) -> Option<u32> {
        let slot = self.slot_of(node).ok()? as u32;
        Some(slot + 1)
    }

    /// 读取某节点 runtime 语义（P0 wire 无 value/range，本口是 host/app
    /// 消费这些字段的唯一读面）。
    pub fn sem_of(&self, node: NodeId) -> Option<SemEntry> {
        self.sem_iter().find(|e| e.node == node).copied()
    }

    /// 当前活语义条目数。
    pub fn semantic_count(&self) -> usize {
        self.sem_iter().count()
    }

    /// 把 SEM + legacy a11y 并集投影为 P0 52B 记录。
    ///
    /// 规则：树前序；SEM wins；bounds = 绝对 bounds ∩ Clip 链，空界跳过；
    /// FOCUS action / FOCUSED state 从 C1 表现场派生；parent = 最近已发射
    /// 语义祖先；node_id = slot+1（raw NodeId 可为 0，不能直接进 P0）。
    /// `merge_descendants` 只跳过后代语义，不改视觉/hit/focus。
    pub fn sem_snapshot_into(
        &self,
        out: &mut [A11yRegion],
    ) -> Result<usize, UiError> {
        let mut slots = [NONE; N];
        let mut slot_count = 0usize;
        self.collect_sibs(self.root_slot, &mut slots, &mut slot_count);
        let mut emitted = [false; N];
        let mut count = 0usize;

        for raw_slot in slots.iter().take(slot_count) {
            let slot = *raw_slot as usize;
            let Some(node) = self.nodes.get(slot).and_then(|option| option.as_ref()) else {
                continue;
            };
            let node_id = pack_id(slot, node.gen);
            let sem = self.sem_iter().find(|entry| entry.node == node_id);
            let Some(legacy) = self.a11y_iter().find(|entry| entry.node == node_id) else {
                continue;
            };
            if self.has_merge_semantic_ancestor(slot) {
                continue;
            }

            let rect = self
                .abs_rect_of(slot)
                .intersect(&self.clip_chain(slot));
            if rect.is_empty() {
                continue;
            }
            let role = legacy.role;
            let label = legacy.label.as_bytes();
            if role > A11Y_ROLE_GAME_REGION || label.len() > sem::SEM_LABEL_MAX {
                return Err(UiError::NodeBad);
            }

            let mut actions = sem.map(|e| e.actions).unwrap_or(0);
            if self
                .focus
                .iter()
                .flatten()
                .any(|e| e.node == node_id && e.enabled)
            {
                actions |= A11Y_ACTION_FOCUS;
            }
            let mut state = sem.map(|e| e.state).unwrap_or(0) | A11Y_STATE_VISIBLE;
            if self.focus_node() == Some(node_id) {
                state |= A11Y_STATE_FOCUSED;
            }

            let parent_slot = self.nearest_emitted_semantic_slot(slot, &emitted);
            let mut label_buf = [0u8; sem::SEM_LABEL_MAX];
            label_buf[..label.len()].copy_from_slice(label);
            let region = A11yRegion {
                node_id: slot as u32 + 1,
                parent_id: parent_slot.map(|parent| parent as u32 + 1).unwrap_or(0),
                x: i32_to_u16_region(rect.x)?,
                y: i32_to_u16_region(rect.y)?,
                w: i32_to_u16_region(rect.w)?,
                h: i32_to_u16_region(rect.h)?,
                role,
                actions,
                state,
                label_len: label.len() as u8,
                label: label_buf,
            };
            let mut wire = [0u8; berry_guiproto::surface::A11Y_REGION_LEN];
            if encode_a11y_region(&mut wire, &region).is_none() {
                return Err(UiError::NodeBad);
            }
            if count >= out.len() {
                return Err(UiError::NodeFull);
            }
            out[count] = region;
            emitted[slot] = true;
            count += 1;
        }
        Ok(count)
    }

    /// 视觉命中的语义目标：向上找最近 SEM/legacy a11y 锚。它不是第二命中
    /// 权威，输入路由仍以 hit/wmd/C1 为准。
    pub fn sem_at(&self, x: u32, y: u32) -> Option<NodeId> {
        let mut node = self.node_at(x, y)?;
        let mut hops = 0usize;
        while hops <= N {
            let slot = self.slot_of(node).ok()? as usize;
            let Some(n) = self.nodes.get(slot).and_then(|option| option.as_ref()) else {
                return None;
            };
            let has_semantic = self.sem_iter().any(|e| e.node == node)
                || self.a11y_iter().any(|e| e.node == node);
            if has_semantic {
                return Some(node);
            }
            if n.parent == NONE {
                return None;
            }
            let parent_node = self
                .nodes
                .get(n.parent as usize)
                .and_then(|option| option.as_ref())?;
            node = pack_id(n.parent as usize, parent_node.gen);
            hops += 1;
        }
        None
    }

    fn has_merge_semantic_ancestor(&self, slot: usize) -> bool {
        let Some(node) = self.nodes.get(slot).and_then(|option| option.as_ref()) else {
            return false;
        };
        let mut parent = node.parent;
        let mut hops = 0usize;
        while parent != NONE && hops <= N {
            let Some(parent_node) = self.nodes.get(parent as usize).and_then(|option| option.as_ref())
            else {
                return false;
            };
            let parent_id = pack_id(parent as usize, parent_node.gen);
            if self
                .sem_iter()
                .any(|e| e.node == parent_id && e.merge_descendants)
            {
                return true;
            }
            parent = parent_node.parent;
            hops += 1;
        }
        false
    }

    fn nearest_emitted_semantic_slot(&self, slot: usize, emitted: &[bool]) -> Option<usize> {
        let Some(node) = self.nodes.get(slot).and_then(|option| option.as_ref()) else {
            return None;
        };
        let mut parent = node.parent;
        let mut hops = 0usize;
        while parent != NONE && hops <= N {
            let parent_slot = parent as usize;
            let Some(parent_node) = self.nodes.get(parent_slot).and_then(|option| option.as_ref())
            else {
                return None;
            };
            let parent_id = pack_id(parent_slot, parent_node.gen);
            let has_semantic = self.sem_iter().any(|e| e.node == parent_id)
                || self.a11y_iter().any(|e| e.node == parent_id);
            if has_semantic && emitted.get(parent_slot).copied().unwrap_or(false) {
                return Some(parent_slot);
            }
            parent = parent_node.parent;
            hops += 1;
        }
        None
    }

    // —— flex ratio（GUI-14 L2：固定主轴 Row/Col 的剩余空间比例分配） ——

    /// L6a：登记/覆写完整 flex spec。L2/L3 eligibility 矩阵不变；全零
    /// spec、悬空 id、不合规 kind/props/父位 → NodeBad，表满 → NodeFull。
    pub fn set_flex_spec(
        &mut self,
        node: NodeId,
        spec: FlexSpec,
    ) -> Result<(), UiError> {
        let idx = self.slot_of(node)? as usize;
        if spec.grow == 0 && spec.shrink == 0 {
            return Err(UiError::NodeBad);
        }
        let eligible = layout::flex_eligible_props(&self.nodes, idx, {
            let Some(n) = self.nodes[idx].as_ref() else {
                return Err(UiError::NodeBad);
            };
            &n.props
        });
        if !eligible {
            return Err(UiError::NodeBad);
        }
        let entry = FlexEntry {
            node,
            weight: spec.grow,
            shrink: spec.shrink,
            basis: spec.basis,
        };
        if let Some(e) = self.flex.iter_mut().find(|e| {
            e.as_ref().is_some_and(|e| e.node == node)
        }) {
            let changed = *e != Some(entry);
            *e = Some(entry);
            if changed {
                self.mark_flex_dirty(node);
            }
            return Ok(());
        }
        let Some(slot) = self.flex.iter().position(|e| e.is_none()) else {
            return Err(UiError::NodeFull);
        };
        self.flex[slot] = Some(entry);
        self.mark_flex_dirty(node);
        Ok(())
    }

    /// 读取 runtime-only flex spec；无登记返回 None。
    pub fn flex_spec(&self, node: NodeId) -> Option<FlexSpec> {
        let entry = self
            .flex_iter()
            .find(|entry| entry.node == node)
            .copied()?;
        Some(FlexSpec {
            grow: entry.weight,
            shrink: entry.shrink,
            basis: entry.basis,
        })
    }

    /// L2 冻结语法糖：`weight` 表示 grow、basis=0、不 shrink。
    pub fn set_flex(&mut self, node: NodeId, weight: u16) -> Result<(), UiError> {
        self.set_flex_spec(
            node,
            FlexSpec { grow: weight, shrink: 0, basis: FlexBasis::Px(0) },
        )
    }

    /// 摘除 flex weight；幂等。摘除后需 layout 才收敛新几何。
    pub fn take_flex(&mut self, node: NodeId) -> bool {
        let Some(slot) = self.flex.iter().position(|e| {
            e.as_ref().is_some_and(|e| e.node == node)
        }) else {
            return false;
        };
        self.flex[slot] = None;
        self.mark_flex_dirty(node);
        true
    }

    /// 活 flex 登记迭代（gen/alive 经 slot_of 过滤）。
    pub fn flex_iter(&self) -> impl Iterator<Item = &FlexEntry> {
        self.flex.iter().flatten().filter(|e| self.slot_of(e.node).is_ok())
    }

    /// LAYOUT-L5：登记/覆写 Text wrap。宽上界不在此声明；measure walk 按
    /// per-node incoming BoxConstraints 派生，并在测量时写入
    /// `last_max_w` 供 paint 使用同一断行计划。非 Text / 悬空 id →
    /// NodeBad；表满 → NodeFull；幂等同值 no-op。
    pub fn set_text_wrap(&mut self, node: NodeId, enabled: bool) -> Result<(), UiError> {
        let idx = self.slot_of(node)?;
        if !matches!(
            self.nodes.get(idx).and_then(|o| o.as_ref().map(|n| n.kind)),
            Some(ViewKind::Text)
        ) {
            return Err(UiError::NodeBad);
        }
        let existing = self
            .wrap
            .iter()
            .find(|e| e.as_ref().is_some_and(|e| e.node == node))
            .and_then(|e| e.as_ref().copied());
        if let Some(old) = existing {
            if old.enabled == enabled {
                return Ok(());
            }
            if let Some(e) = self.wrap.iter_mut().find(|e| {
                e.as_ref().is_some_and(|e| e.node == node)
            }) {
                *e = Some(TextWrapEntry { node, enabled, last_max_w: old.last_max_w });
            }
            self.mark_flex_dirty(node);
            return Ok(());
        }
        let Some(slot) = self.wrap.iter().position(|e| e.is_none()) else {
            return Err(UiError::NodeFull);
        };
        self.wrap[slot] = Some(TextWrapEntry { node, enabled, last_max_w: u32::MAX });
        self.mark_flex_dirty(node);
        Ok(())
    }

    /// 摘除 Text wrap；幂等。
    pub fn take_text_wrap(&mut self, node: NodeId) -> bool {
        let Some(slot) = self.wrap.iter().position(|e| {
            e.as_ref().is_some_and(|e| e.node == node)
        }) else {
            return false;
        };
        self.wrap[slot] = None;
        self.mark_flex_dirty(node);
        true
    }

    /// 活性 wrap 登记迭代（gen/alive 过滤）。
    pub fn text_wrap_iter(&self) -> impl Iterator<Item = &TextWrapEntry> {
        self.wrap
            .iter()
            .flatten()
            .filter(|e| self.slot_of(e.node).is_ok())
    }

    /// LAYOUT-L5：祖先几何/容器盒变化时，重标子树内 wrap Text。父节点
    /// 自身 dirty 只会触发布局递归，不会自动重测缓存命中的叶子；wrap 叶
    /// 子的 intrinsic 依赖 incoming max_w，因此必须显式加入重测链。
    fn mark_wrapped_descendants_dirty(&mut self, ancestor: u32) {
        let mut entries = [None; TEXT_WRAP_MAX];
        let mut count = 0usize;
        for entry in self.wrap.iter().flatten() {
            if count < entries.len() {
                entries[count] = Some(entry.node);
                count += 1;
            }
        }
        for node in entries[..count].iter().flatten().copied() {
            let Ok(slot) = self.slot_of(node) else { continue };
            let mut cur = slot as u32;
            let mut hops = 0usize;
            let mut hit = false;
            while cur != NONE && hops <= N {
                hops += 1;
                if cur == ancestor {
                    hit = true;
                    break;
                }
                let Some(node) = self.nodes.get(cur as usize).and_then(|o| o.as_ref()) else {
                    break;
                };
                cur = node.parent;
            }
            if hit {
                self.mark_flex_dirty(node);
            }
        }
    }

    fn mark_flex_dirty(&mut self, node: NodeId) {
        let Ok(idx) = self.slot_of(node) else { return };
        let parent = self.nodes.get(idx).and_then(|o| o.as_ref()).map(|n| n.parent).unwrap_or(NONE);
        if let Some(n) = self.nodes.get_mut(idx).and_then(|o| o.as_mut()) {
            n.layout_dirty = true;
        }
        if parent != NONE {
            self.mark_ancestors_dirty(parent);
        }
    }

    // —— component focus（GUI-14 C1：树序一维导航；窗口焦点权威仍在 wmd） ——

    /// 登记/覆写 focusable。`visual` 为 None 时焦点变化只 dirty 语义锚；
    /// widget 通常传 panel 自身。Disabled 项可保留登记但不参与导航。
    pub fn set_focusable(
        &mut self, node: NodeId, visual: Option<NodeId>, enabled: bool,
    ) -> Result<(), UiError> {
        self.slot_of(node)?;
        if let Some(v) = visual {
            self.slot_of(v)?;
        }
        if self.focused == Some(node) && !enabled {
            self.set_focus_internal(None);
        }
        if let Some(e) = self.focus.iter_mut().find(|e| {
            e.as_ref().is_some_and(|e| e.node == node)
        }) {
            *e = Some(FocusEntry { node, visual, enabled });
            return Ok(());
        }
        let Some(slot) = self.focus.iter().position(|e| e.is_none()) else {
            return Err(UiError::NodeFull);
        };
        self.focus[slot] = Some(FocusEntry { node, visual, enabled });
        Ok(())
    }

    /// 摘除 focusable；幂等。持焦时 component focus 归空并 dirty 视觉，
    /// 但不合成 FocusOut——视觉属主必须在禁用/摘除路径同步还原 ring。
    pub fn take_focusable(&mut self, node: NodeId) -> bool {
        let Some(slot) = self.focus.iter().position(|e| {
            e.as_ref().is_some_and(|e| e.node == node)
        }) else {
            return false;
        };
        self.focus[slot] = None;
        if self.focused == Some(node) {
            self.set_focus_internal(None);
        }
        true
    }

    /// focusable 活性迭代（按登记序，非导航序；导航序见
    /// [`Self::focus_order_into`]）。
    pub fn focus_iter(&self) -> impl Iterator<Item = &FocusEntry> {
        self.focus.iter().flatten().filter(|e| self.slot_of(e.node).is_ok())
    }

    /// 当前 component focus 语义锚。
    pub fn focus_node(&self) -> Option<NodeId> {
        self.focused.filter(|n| self.slot_of(*n).is_ok())
    }

    /// 节点是否持 component focus（活性过滤）。
    pub fn is_focused(&self, node: NodeId) -> bool {
        self.focus_node() == Some(node)
    }

    /// a11y state 位：已登记 a11y 的节点返回 VISIBLE，焦点节点再 OR
    /// FOCUSED。组件焦点不要求节点必然登记 a11y；未登记返 None。
    pub fn a11y_state(&self, node: NodeId) -> Option<u8> {
        if !self.a11y_iter().any(|e| e.node == node) {
            return None;
        }
        let mut state = berry_guiproto::surface::A11Y_STATE_VISIBLE;
        if self.is_focused(node) {
            state |= berry_guiproto::surface::A11Y_STATE_FOCUSED;
        }
        Some(state)
    }

    /// 树前序 focusable 顺序写入 out，返回数量。禁用项不进序。
    pub fn focus_order_into(&self, out: &mut [NodeId]) -> usize {
        let mut slots = [NONE; N];
        let mut n = 0usize;
        self.collect_sibs(self.root_slot, &mut slots, &mut n);
        let mut count = 0usize;
        for slot in slots.iter().take(n) {
            let Some(node) = self.nodes.get(*slot as usize).and_then(|o| o.as_ref()) else {
                continue;
            };
            let id = pack_id(*slot as usize, node.gen);
            if self.focus.iter().flatten().any(|e| e.node == id && e.enabled)
                && count < out.len()
            {
                out[count] = id;
                count += 1;
            }
        }
        count
    }

    /// 显式聚焦指定 focusable。活但未登记/禁用 → 空 MsgBuf；悬空 →
    /// NodeBad。成功且状态变化时输出 FocusOut（旧值存在）+ FocusIn。
    pub fn focus_set(&mut self, node: NodeId) -> Result<desc::MsgBuf, UiError> {
        self.slot_of(node)?;
        if !self.focus.iter().flatten().any(|e| e.node == node && e.enabled) {
            return Ok(desc::MsgBuf::new());
        }
        let mut out = desc::MsgBuf::new();
        let old = self.focus_node();
        if old == Some(node) {
            return Ok(out);
        }
        if let Some(old) = old {
            out.push(Msg::FocusOut { node: old }).map_err(|_| UiError::MsgFull)?;
        }
        self.set_focus_internal(Some(node));
        out.push(Msg::FocusIn { node }).map_err(|_| UiError::MsgFull)?;
        Ok(out)
    }

    /// 清空 component focus；输出 FocusOut。窗口焦点不受影响。
    pub fn focus_clear(&mut self) -> desc::MsgBuf {
        let mut out = desc::MsgBuf::new();
        let Some(old) = self.focus_node() else { return out };
        self.set_focus_internal(None);
        let _ = out.push(Msg::FocusOut { node: old });
        out
    }

    /// 树前序一维焦点导航（v1 环绕，对齐 Compose 1D Tab 语义）。
    /// 零可焦项或唯一项自指 → 空 MsgBuf，不产生 FocusOut/In 抖动。
    pub fn focus_move(&mut self, dir: desc::FocusDir) -> desc::MsgBuf {
        let mut out = desc::MsgBuf::new();
        let mut order = [NodeId(0); N];
        let n = self.focus_order_into(&mut order);
        if n == 0 {
            return out;
        }
        let current = self.focus_node();
        let target = match current.and_then(|cur| order[..n].iter().position(|x| *x == cur)) {
            Some(i) => match dir {
                desc::FocusDir::Next => order[(i + 1) % n],
                desc::FocusDir::Prev => order[(i + n - 1) % n],
            },
            None => order[0],
        };
        if current == Some(target) {
            return out;
        }
        if let Some(old) = current {
            let _ = out.push(Msg::FocusOut { node: old });
        }
        self.set_focus_internal(Some(target));
        let _ = out.push(Msg::FocusIn { node: target });
        out
    }

    /// 内部落焦：dirty 旧/新语义锚与登记视觉锚。不产生事件。
    fn set_focus_internal(&mut self, new: Option<NodeId>) {
        let old = self.focused;
        self.focused = new;
        for node in [old, new].into_iter().flatten() {
            let visual = self
                .focus
                .iter()
                .flatten()
                .find(|e| e.node == node)
                .and_then(|e| e.visual)
                .unwrap_or(node);
            let _ = self.invalidate(visual);
        }
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
                Some(ViewKind::Rect) | Some(ViewKind::Text)
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

    /// 保持 trigger 的 pressed 视觉，不经过输入事件。用于 launcher 在
    /// Click 后进入真实的 launching 状态：DOWN/UP 可能同批到达并在同一帧
    /// 抵消，本 API 让嵌入器显式持有反馈直到真实结果到达。
    pub fn hold_press(&mut self, trigger: NodeId) {
        self.press_react(Msg::Press { node: trigger, x: 0, y: 0 });
    }

    /// 显式释放 [`Ui::hold_press`] 建立的 pressed 视觉。未持有时为 no-op；
    /// 与普通 Release 一样恢复首次持有时保存的背景。
    pub fn release_press(&mut self, trigger: NodeId) {
        self.press_react(Msg::Release { node: trigger, x: 0, y: 0 });
    }

    /// 请求下一帧全屏重绘。SYSTEM 嵌入器在 WM 权威可见性变化后使用，
    /// 清除被隐藏 app 遗留在合成 VMO 中的旧像素。
    pub fn invalidate_full(&mut self) {
        self.damage.add_full();
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
            | Msg::DragEnd { .. } | Msg::FocusIn { .. } | Msg::FocusOut { .. } => {}
        }
    }

    /// 表查（immutable 面）：首个 trigger 命中的条目副本。
    fn press_entry_of(map: &[PressEntry], node: NodeId) -> Option<PressEntry> {
        map.iter().find(|e| e.trigger == node).copied()
    }

    /// 表查（mutable 面）。
    fn press_entry_of_mut(map: &mut [PressEntry], node: NodeId) -> Option<&mut PressEntry> {
        map.iter_mut().find(|e| e.trigger == node)
    }

    // —— 供给槽（D28：门面 layout/render_frame 签名不变——槽取传下） ——

    /// 文本度量供件换装（默认 [`MonospaceTextMetrics`]；换件后脏位重测
    /// 生效——set_viewport 触发全量重测）。
    pub fn set_text_metrics(&mut self, tm: &'static dyn TextMetrics) {
        self.tm = tm;
        // 供件换装会改变 intrinsic 与 wrap plan。即便新旧尺寸相同，断行
        // 也可能不同；全树 layout/paint dirty + FULL damage 是单源收口。
        let mut i = 0usize;
        while i < N {
            if let Some(n) = self.nodes[i].as_mut() {
                n.layout_dirty = true;
                n.paint_dirty = true;
            }
            i += 1;
        }
        self.damage.add_full();
    }

    /// 图标/图片资源供件换装（默认 [`EmptyIconSource`]——dims 恒 None）。
    pub fn set_icon_source(&mut self, src: &'static dyn IconSource) {
        self.icons = src;
    }

    /// 运行期字形供件换装。guest-font-vmo 档必须注入；布局测量仍由
    /// TextMetrics 单源负责。
    pub fn set_font_source(&mut self, source: &'static dyn metrics::FontSource) {
        self.fonts = Some(source);
    }

    pub fn bounds(&self, node: NodeId) -> Result<damage::RectI, UiError> {
        // 绝对界（含 ox/oy）。
        let idx = self.slot_of(node)? as usize;
        Ok(self.abs_rect_of(idx))
    }

    /// 节点有效结构性 Clip 链（含祖先与自身 Clip，再交 viewport）。
    /// `bounds` 仍返回未裁剪视觉界；本口仅供诊断/精确 damage 消费。
    pub fn clip_of(&self, node: NodeId) -> Result<damage::RectI, UiError> {
        let idx = self.slot_of(node)? as usize;
        Ok(self.clip_chain(idx))
    }

    fn clip_chain(&self, idx: usize) -> damage::RectI {
        let mut clip = damage::RectI::new(
            0,
            0,
            damage::u32_to_i32_sat(self.vp_w),
            damage::u32_to_i32_sat(self.vp_h),
        );
        let mut cur = idx as u32;
        let mut hops = 0usize;
        while hops <= N {
            let Some(n) = self.nodes.get(cur as usize).and_then(|option| option.as_ref()) else {
                break;
            };
            if n.kind == desc::ViewKind::Clip {
                let bounds = self.abs_rect_of(cur as usize);
                clip = clip.intersect(&bounds);
                if clip.is_empty() {
                    return clip;
                }
            }
            if n.parent == NONE {
                break;
            }
            cur = n.parent;
            hops += 1;
        }
        clip
    }

    fn damage_node_rect(&mut self, idx: usize, rect: damage::RectI) {
        let clip = self.clip_chain(idx);
        self.damage_rect(rect.intersect(&clip));
    }

    pub fn kind_of(&self, node: NodeId) -> Result<desc::ViewKind, UiError> {
        let idx = self.slot_of(node)? as usize;
        match self.nodes[idx].as_ref() {
            Some(n) => Ok(n.kind),
            None => Err(UiError::NodeBad),
        }
    }

    /// F2 布局诊断 dump：前序保留树 + 相对摆位 + 视觉绝对界 + dirty 位。
    /// `out` 过小 → `NodeFull`，绝不静默截断。
    pub fn layout_dump(
        &self, out: &mut [diagnostic::LayoutDumpEntry],
    ) -> Result<usize, UiError> {
        diagnostic::layout(&self.nodes, self.root_slot, out)
    }

    /// F2 命中诊断 dump：逐点输出 topmost 节点与命中界。
    pub fn hit_test_dump(
        &self, points: &[(u32, u32)], out: &mut [diagnostic::HitDumpEntry],
    ) -> Result<usize, UiError> {
        diagnostic::hit_test(&self.nodes, self.root_slot, points, out)
    }

    // —— 布局 / 脏区 ——

    pub fn layout(&mut self) -> layout::LayoutStat {
        self.layout_with(layout::Constraints::Viewport)
    }

    /// 根播种约束布局（GUI-14 L1）。Pull-based 一次性面：不存上次
    /// 约束；净树调用 = 文档化 no-op `{0,0}`。切换约束需调用方先经
    /// 几何变更/set_viewport 制造 dirty。render_frame 发起的 relayout 请求
    /// 仍为 Viewport；净树不自动恢复上次 layout_with 约束。
    pub fn layout_with(&mut self, cs: layout::Constraints) -> layout::LayoutStat {
        let bc = cs.to_box(self.vp_w, self.vp_h);
        debug_assert!(bc.min().0 <= bc.max().0 && bc.min().1 <= bc.max().1);
        layout::relayout(
            &mut self.nodes,
            self.vp_w,
            self.vp_h,
            &mut self.damage,
            &self.free_stack[..self.free_count],
            self.tm,
            self.icons,
            bc,
            &self.flex,
            &mut self.wrap,
        )
    }

    /// LAYOUT-L4 四元组布局。与 `layout_with` 同为 pull-based 一次性面；
    /// 不保存约束，净树 no-op，damage clip 恒真实 viewport。
    pub fn layout_box(
        &mut self,
        bc: layout::BoxConstraints,
    ) -> Result<layout::LayoutStat, UiError> {
        if bc.min().0 > bc.max().0 || bc.min().1 > bc.max().1 {
            return Err(UiError::NodeBad);
        }
        Ok(layout::relayout(
            &mut self.nodes,
            self.vp_w,
            self.vp_h,
            &mut self.damage,
            &self.free_stack[..self.free_count],
            self.tm,
            self.icons,
            bc,
            &self.flex,
            &mut self.wrap,
        ))
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
            self.fonts,
            self.tm,
            &self.wrap,
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
                // D18 pressed 短路径：本事件产出的 Msg 即时消费（Press 换
                // 底 / Release·Cancel 还原——走现状 set_props 语义同帧生
                // 效，主线零依赖 B 面 mask）。
                let mut k = n_before;
                while k < out.len() {
                    if let Some(m) = out.at(k) {
                        self.press_react(m);
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
            layout::Constraints::Viewport.to_box(self.vp_w, self.vp_h),
            &self.flex,
            &mut self.wrap,
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
                self.fonts, // F5：guest 档可由系统字形 VMO 供给
                self.tm,
                &self.wrap,
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

    /// RUN-K1 keyed re-apply：先在临时 arena 解析/计划，成功后才修改当前
    /// 树。key 匹配节点保持 `(slot, generation)`，focus/sem/flex/anim/dep
    /// 等运行期侧表随 NodeId 存活；声明式 a11y 在末尾按 source 重建。
    pub fn apply_desc_keyed(&mut self, src: &[u8]) -> Result<ReapplyStat, desc::AstError> {
        // Pass 0：源树完整解析。当前树在此阶段零修改，因此非法源保持旧树。
        let mut source = [None; KEYED_SOURCE_MAX];
        let mut source_gens = [0u16; KEYED_SOURCE_MAX];
        let mut source_root = NONE;
        let mut source_a11y: [Option<A11yEntry>; A11Y_MAX] = [None; A11Y_MAX];
        desc::apply_tree(
            src,
            &mut source,
            &mut source_gens,
            &mut source_root,
            &mut source_a11y,
        )?;
        let source_live = source.iter().flatten().count() as u32;
        let source_depth = source_max_depth::<KEYED_SOURCE_MAX>(&source)?;
        if source_depth > KEYED_DEPTH_MAX {
            return Err(desc::AstError::TooSmall {
                need: source_depth as usize,
                got: KEYED_DEPTH_MAX as usize,
            });
        }
        // source scratch 与目标 arena 的 N 分域：先验最终节点数，避免
        // source 前序 slot >= N 时误写 KeyPlan::<N>。
        if source_live > N as u32 {
            return Err(desc::AstError::TooSmall {
                need: source_live as usize,
                got: N,
            });
        }

        // 当前空树没有可保身份：直接走 full oracle（源树已验合法）。
        if self.root_slot == NONE {
            self.apply_desc(src)?;
            return Ok(ReapplyStat {
                matched_clean: 0,
                updated: 0,
                moved: 0,
                added: source_live,
                removed: 0,
                rederives: 1,
                fallback_full: true,
            });
        }

        let mut plan = KeyPlan::<N>::new();
        let target_root = self.root_slot;
        plan.source_to_target[source_root as usize] = target_root;
        plan.target_keep[target_root as usize] = true;
        let (root_source, root_target) = (
            source[source_root as usize].as_ref(),
            self.nodes[target_root as usize].as_ref(),
        );
        match (root_source, root_target) {
            (Some(source_root_node), Some(target_root_node)) => {
                if source_root_node.props != target_root_node.props
                    || source_root_node.key != target_root_node.key
                {
                    plan.updated += 1;
                } else {
                    plan.matched_clean += 1;
                }
            }
            _ => return Err(desc::AstError::Truncated),
        }
        plan_keyed_node(&source, source_root, &self.nodes, target_root, &mut plan)?;

        let target_live = self.nodes.iter().flatten().count() as u32;
        // 大 churn 时 full rebuild 是正确 oracle，不给低效 keyed 路径付
        // side-table 全清 + 逐点调和的双重成本。
        if plan.added + plan.removed >= target_live {
            self.apply_desc(src)?;
            return Ok(ReapplyStat {
                matched_clean: 0,
                updated: 0,
                moved: 0,
                added: source_live,
                removed: target_live,
                rederives: 1,
                fallback_full: true,
            });
        }
        let needed = target_live - plan.removed + plan.added;
        if needed > N as u32 {
            return Err(desc::AstError::TooSmall { need: needed as usize, got: N });
        }
        // a11y upsert 容量也必须先验：既有运行期条目不因 keyed 小改清空，
        // source 新声明若需更多空槽，必须在修改 arena 前拒绝。
        let mut free_a11y = self.a11y.iter().filter(|entry| entry.is_none()).count();
        for existing in self.a11y.iter().flatten() {
            let slot = (existing.node.0 & 0xFFFF) as usize;
            if slot < N && plan.target_remove[slot] {
                free_a11y += 1; // remove 阶段会连带清该子树 a11y
            }
        }
        for entry in source_a11y.iter().flatten() {
            let source_slot = (entry.node.0 & 0xFFFF) as usize;
            let target_slot = plan.source_to_target[source_slot];
            let existing_hit = if plan.source_add[source_slot] {
                false
            } else {
                match self.nodes.get(target_slot as usize).and_then(|option| option.as_ref()) {
                    Some(target_node) => {
                        let target_id = pack_id(target_slot as usize, target_node.gen);
                        self.a11y.iter().flatten().any(|existing| existing.node == target_id)
                    }
                    None => return Err(desc::AstError::Truncated),
                }
            };
            if !existing_hit {
                if free_a11y == 0 {
                    return Err(desc::AstError::TooSmall {
                        need: A11Y_MAX + 1,
                        got: A11Y_MAX,
                    });
                }
                free_a11y -= 1;
            }
        }

        // Pass 2：先走既有 remove，复用全部侧表/代数/damage 生命周期。
        for slot in 0..N {
            if !plan.target_remove[slot] {
                continue;
            }
            let Some(node) = self.nodes[slot].as_ref() else { continue };
            if !node.alive {
                continue;
            }
            let id = pack_id(slot, node.gen);
            self.remove(id).map_err(|_| desc::AstError::Truncated)?;
        }

        // Pass 3：source 前序调和。matched 保持身份；added 走 add 单源。
        self.reconcile_keyed_node(&source, source_root, target_root, &mut plan)?;

        // Pass 4：source a11y 只 upsert，不整清。keyed re-apply 的职责是
        // 保运行期侧表身份；widget/app 在树建立后登记的 a11y/sem 不能因
        // 后续小改 AST 被抹掉。显式摘除仍走 take_a11y/take_sem。
        for entry in source_a11y.iter().flatten() {
            let source_slot = (entry.node.0 & 0xFFFF) as usize;
            let target_slot = plan.source_to_target[source_slot] as usize;
            let Some(target_node) = self.nodes.get(target_slot).and_then(|o| o.as_ref()) else {
                return Err(desc::AstError::Truncated);
            };
            let target_id = pack_id(target_slot, target_node.gen);
            let updated = A11yEntry {
                node: target_id,
                role: entry.role,
                label: entry.label,
            };
            if let Some(slot) = self
                .a11y
                .iter_mut()
                .find(|e| e.as_ref().is_some_and(|e| e.node == target_id))
            {
                *slot = Some(updated);
            } else {
                let Some(slot) = self.a11y.iter_mut().find(|e| e.is_none()) else {
                    return Err(desc::AstError::TooSmall { need: A11Y_MAX + 1, got: A11Y_MAX });
                };
                *slot = Some(updated);
            }
        }

        Ok(ReapplyStat {
            matched_clean: plan.matched_clean,
            updated: plan.updated,
            moved: plan.moved,
            added: plan.added,
            removed: plan.removed,
            rederives: 1,
            fallback_full: false,
        })
    }

    fn reconcile_keyed_node(
        &mut self,
        source: &[Option<Node>],
        source_slot: u32,
        parent_target: u32,
        plan: &mut KeyPlan<N>,
    ) -> Result<u32, desc::AstError> {
        let Some(source_node) = source.get(source_slot as usize).and_then(|o| o.as_ref()) else {
            return Err(desc::AstError::Truncated);
        };
        let is_add = plan.source_add[source_slot as usize];
        let target_slot = if is_add {
            let parent_id = match self.nodes.get(parent_target as usize).and_then(|o| o.as_ref()) {
                Some(node) => pack_id(parent_target as usize, node.gen),
                None => return Err(desc::AstError::Truncated),
            };
            let spec = desc::ViewSpec {
                kind: source_node.kind,
                key: source_node.key,
                props: source_node.props,
            };
            let id = self
                .add(parent_id, spec)
                .map_err(|error| match error {
                    UiError::NodeFull => desc::AstError::TooSmall { need: N + 1, got: N },
                    UiError::BadParent => desc::AstError::BadParent,
                    UiError::NodeBad if source_node.key != 0 => {
                        desc::AstError::DuplicateKey { key: source_node.key }
                    }
                    _ => desc::AstError::UnknownProp { kind: source_node.kind, got: 0 },
                })?;
            let slot = (id.0 & 0xFFFF) as u32;
            plan.source_to_target[source_slot as usize] = slot;
            slot
        } else {
            plan.source_to_target[source_slot as usize]
        };

        if !is_add {
            let id = match self.nodes.get(target_slot as usize).and_then(|o| o.as_ref()) {
                Some(node) => pack_id(target_slot as usize, node.gen),
                None => return Err(desc::AstError::Truncated),
            };
            if self.nodes[target_slot as usize].as_ref().map(|n| n.props)
                != Some(source_node.props)
            {
                self.set_props(id, source_node.props).map_err(|error| match error {
                    UiError::NodeBad => {
                        desc::AstError::KeyKindMismatch { key: source_node.key }
                    }
                    _ => desc::AstError::UnknownProp { kind: source_node.kind, got: 0 },
                })?;
            }
            if let Some(node) = self.nodes[target_slot as usize].as_mut() {
                node.key = source_node.key;
            }

            // 先摘所有 matched 子，再按 source 序回链；unkeyed/新增子不
            // 参与旧身份匹配。移动节点先收旧视觉界，回链后收新界。链式
            // 走查同样不落 [u32; N] 子清单，保持 D29 每层小栈预算。
            let mut child_cur = source_node.first_child;
            let mut hops = 0usize;
            while child_cur != NONE && hops <= N {
                hops += 1;
                let Some(child) = source.get(child_cur as usize).and_then(|o| o.as_ref()) else {
                    return Err(desc::AstError::Truncated);
                };
                let next = child.next_sib;
                let child_slot = child_cur;
                if !plan.source_add[child_slot as usize] {
                    let child_target = plan.source_to_target[child_slot as usize];
                    if plan.source_moved[child_slot as usize] {
                        let old_rect = self.abs_rect_of(child_target as usize);
                        self.damage_rect(old_rect);
                    }
                    self.unlink(child_target as usize);
                    // unlink 只摘邻接，不清被摘节点自身 next/prev；回链前
                    // 必须归零，否则 link_child 的 tail 扫描会把旧 next 误
                    // 当已在链尾，构造 self-cycle。
                    if let Some(child) =
                        self.nodes.get_mut(child_target as usize).and_then(|o| o.as_mut())
                    {
                        child.prev_sib = NONE;
                        child.next_sib = NONE;
                    }
                }
                child_cur = next;
            }
        }

        let mut child_cur = source_node.first_child;
        let mut hops = 0usize;
        while child_cur != NONE && hops <= N {
            hops += 1;
            let Some(child) = source.get(child_cur as usize).and_then(|o| o.as_ref()) else {
                return Err(desc::AstError::Truncated);
            };
            let next = child.next_sib;
            let child_slot = child_cur;
            let child_add = plan.source_add[child_slot as usize];
            let child_target =
                self.reconcile_keyed_node(source, child_slot, target_slot, plan)?;
            if !child_add {
                if let Some(child) =
                    self.nodes.get_mut(child_target as usize).and_then(|o| o.as_mut())
                {
                    child.parent = target_slot;
                }
                self.link_child(target_slot as usize, child_target);
                if plan.source_moved[child_slot as usize] {
                    let new_rect = self.abs_rect_of(child_target as usize);
                    self.damage_rect(new_rect);
                    self.mark_ancestors_dirty(child_target);
                }
            }
            child_cur = next;
        }
        Ok(target_slot)
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
