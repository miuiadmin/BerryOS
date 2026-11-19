//! desc：数据驱动描述层——视图集/属性族/Msg 面 + **AST 二进制 TLV
//! 双入口**（`docs/GUI-3实施设计草案-v1.0.md` §3.2/§6 冻结契约；ADR #52
//! 「API 面数据驱动 + 描述结构可序列化双入口」day-one 落法）。
//!
//! # AST 二进制格式（§3.2 + GUI-6 §8——逐字节定义，roundtrip 字节相等靠格式自洽）
//!
//! 前序 TLV 流，节点连续排列（无总长域——流长即树界）：
//!
//! ```text
//! node   := kind u8 | flags u8 | key u64 LE（仅 flags.bit0）| props
//!         | size 块 8B?（flags.bit1）| a11y 块 2..=66B?（flags.bit2）
//!         | align 块 1B?（flags.bit3）| child_count u8
//! flags  := bit0 = has_key / bit1 = has_size / bit2 = has_a11y /
//!           bit3 = has_align（bit4+ 未知 → UnknownFlags 拒）
//! props  := kind 定长：Root 0B / Rect 11B / Text 变长(6..=9+len) /
//!           Col·Row·Pad 4B / Image 10B / Icon 13B / Grid 7B（GUI-6 §8.1）；
//!           组件七件（Phase 1 清单，tag 9..=15 add-only）：Button 变长
//!           (6..=70) / Toggle·Progress 5B / Slider 16B / Scroll·List 8B /
//!           Tabs 12B（u32 域全 LE）；P1 九件（组件清单多数集合批，
//!           tag 16..=24 add-only）：Stack 1B / Spacer 4B / Divider 12B /
//!           Check·Badge 5B / Radio·Sbar 9B / Step 20B / Seg 12B（u32 域
//!           全 LE）；P0 尾四件（组件清单收尾批，tag 25..=28 add-only）：
//!           Select 12B / Input 变长(6..=70) / Edit 变长(10..=74) /
//!           Canvas 4B（u32 域全 LE）；P1 尾五件（组件清单 P1 kind 件
//!           收尾批，tag 29..=33 add-only）：Tip 变长(2..=66) / Refresh
//!           9B / SwipeItem 16B / DatePick 24B / TimePick 13B；P2 七件
//!           （借来件批，tag 34..=40 add-only）：Collapse 5B / Chip
//!           变长(6..=70) / Fab 变长(9..=73) / Card 8B / Gauge 5B /
//!           Qr 12B / DragReorder 12B（u32 域全 LE）
//! ```
//!
//! - `child_count` 是**树形走查域**（不属于节点本体）：[`parse_node`] 只
//!   解析节点本体（kind/flags/key/props/扩展块），child_count 由树走查
//!   方（[`apply_tree`]/[`DescBuilder`]）读写——两处同一字节位，格式自洽。
//! - Rect 11B = bg 3B + w/h 各 4B（`u32::MAX` 哨兵 = None「随父」）。
//! - Col/Row/Pad 4B = gap/all u32 LE；Grid 7B = cols u8 + rows u8 +
//!   gap u32 LE + cross u8（cross 在 props 块内——Grid 无 ALIGN 块，
//!   §8.1 定长铁则）；Image 10B = res u16 LE + w/h 各 4B 哨兵对；
//!   Icon 13B = id u16 LE + fg 3B + w/h 各 4B 哨兵对。
//! - 扩展块序恒 SIZE→A11Y→ALIGN（§8 D5——解析/发射/builder 三面同序）；
//!   size 块 8B = w/h 各 4B（`SIZE_NONE` 哨兵）；a11y 块 = role u8 +
//!   len u8 + label ≤64B；align 块 1B = cross u8。**块在 ⟺ 有义**
//!   （size 双 None / align 载 Stretch 缺省 = 非规范形拒——块存在性可
//!   从 arena 状态推导，byte-exact roundtrip 的前提）。
//! - 组件七件（Phase 1，`docs` 组件清单）：Button/Toggle/Slider/Progress
//!   = 新叶（叶带子拒面与 Rect/Text/Image/Icon 同列）；Scroll/List/Tabs
//!   = 容器（AST/builder 面可带子）。Button 变长 = len u8 + label +
//!   style u8 + on_tap u32；Toggle 5B = checked u8 + on_change；Slider
//!   16B = value·min·max·on_change（value 0..=1000 permille——免 FP，
//!   f32 破 Eq/整数纪律）；Progress 5B = value + style；Scroll 8B =
//!   content_height + offset_y；List 8B = item_count + item_height；
//!   Tabs 12B = tab_count·active·on_switch。值域拒面 [`AstError::BadValue`]
//!   （add-only）。渲染/布局/手势面 Phase 1 不实现（paint 走查无视觉、
//!   layout 占位——Phase 2 接管）。
//! - P1 九件（组件清单多数集合批，tag 16..=24 add-only）：Stack = 叠层
//!   容器（子按 z 序全幅叠放——AST/builder 面可带子；命令面同 Phase 1
//!   容器边界）；八新叶 Spacer/Divider/Check/Radio/Step/Seg/Badge/Sbar
//!   入叶带子拒面。wire：Stack 1B（align u8 0/1/2）、Spacer 4B（flex）、
//!   Divider 12B（thickness·inset_start·inset_end）、Check 5B（checked
//!   u8 + on_change）、Radio 9B（selected u8 + group + on_change）、
//!   Step 20B（value·min·max·step·on_change）、Seg 12B（segment_count·
//!   active·on_change）、Badge 5B（count + show_dot u8）、Sbar 9B
//!   （orientation u8 + thumb_start·thumb_end 各 0..=1000 permille）。
//!   BadValue 拒面：Step 零步长/min>max/value 越界、Seg 零段/active
//!   越段、Sbar thumb 越千分域/start>end；bool 子域（checked/selected/
//!   show_dot）与枚举子域（align/orientation）域外 UnknownProp。渲染/
//!   布局/手势面同 Phase 1 不实现（挂 Phase 2）。
//! - P0 尾四件（组件清单收尾批，tag 25..=28 add-only）：Select = 下拉
//!   选择器（选项计数面——选项内容不进数据层，展开面挂 Phase 2；Seg
//!   叶化同族）/ Input = 单行输入（**实际文本内容不走 Props**——走输入
//!   焦点协议 Phase 2 接线，数据层只载外观与约束）/ Edit = 多行编辑
//!   （内容同 Input 不入 Props；max_lines=0 无限制）/ Canvas = 自绘面
//!   （**叶子**——绘制内容不是子节点；op 流走绘制协议，props 只记
//!   paint_ops 计数）。四件全叶（叶带子拒面同列）。wire：Select 12B =
//!   option_count·selected·on_change；Input 变长 6..=70 = len u8 +
//!   placeholder + max_len u32 + input_kind u8；Edit 变长 10..=74 =
//!   同 Input 前缀 + max_lines u32 + readonly u8；Canvas 4B =
//!   paint_ops。BadValue 拒面：Select selected 越项（option_count > 0
//!   时——零项合法 = 空选择器，与 Tabs/Seg 零段拒不同律：空下拉有义）；
//!   枚举子域 input_kind 域外 UnknownProp、bool 子域 readonly 域外
//!   UnknownProp、placeholder 超长 TextTooLong。渲染/布局/手势面同前
//!   两批不实现（挂 Phase 2）。
//! - P1 尾五件（组件清单 P1 kind 件收尾批，tag 29..=33 add-only）：
//!   Refresh = 下拉刷新**容器**（可带子——包裹 List 等；AST/builder 面
//!   可带子，命令面同前批容器边界）/ 四新叶 Tip（工具提示——label 同
//!   Button 走内联 TextBuf）、SwipeItem（滑动操作项——reveal 两端各
//!   0..=1000 permille）、DatePick（日期选择器）、TimePick（时间选择
//!   器）入叶带子拒面。wire：Tip 变长 2..=66 = len u8 + text +
//!   placement u8；Refresh 9B = refreshing u8 + offset·on_refresh 各
//!   u32 LE；SwipeItem 16B = action_count·reveal_start·reveal_end·
//!   on_action；DatePick 24B = year·month·day·min_year·max_year·
//!   on_change；TimePick 13B = hour·minute·on_change 各 u32 LE +
//!   use_24h u8。BadValue 拒面：Refresh offset 越千分域、SwipeItem
//!   reveal 任一端越千分域、DatePick month 非 1..=12 / day 非 1..=31 /
//!   min_year > max_year / year 越界、TimePick hour > 23 / minute > 59
//!   （日历语义——day 不做月长感知，2 月 31 结构合法消费面钳）；枚举
//!   子域 placement 域外 UnknownProp、bool 子域（refreshing/use_24h）
//!   域外 UnknownProp、text 超长 TextTooLong。渲染/布局/手势面同前三
//!   批不实现（挂 Phase 2）。
//! - P2 七件（借来件批，tag 34..=40 add-only）：Collapse = 折叠组
//!   **容器**（可带子——折叠内容；展开/收起动画复用 anim，挂 Phase 2）
//!   / Card = 卡片**容器**（可带子——卡片内容；elevation 阴影与圆角
//!   视觉挂 Phase 2）/ 五新叶 Chip（胶囊标签——本质 Button 变体但独立
//!   kind 为 a11y role）、Fab（悬浮操作按钮——纯圆形 + elevation）、
//!   Gauge（仪表盘——value 0..=1000 permille 免 FP 同 Slider 纪律）、
//!   Qr（二维码——数据层只记 module_count 与双色，点阵生成挂 Phase 2）、
//!   DragReorder（拖拽重排手柄）入叶带子拒面。wire：Collapse 5B =
//!   expanded u8 + on_toggle u32 LE；Chip 变长 6..=70 = len u8 + label +
//!   selected u8 + on_tap；Fab 变长 9..=73 = icon u32 LE + len u8 +
//!   extended_label + on_tap（空标签 = 纯圆形）；Card 8B = elevation·
//!   corner_radius 各 u32 LE；Gauge 5B = value u32 LE + style u8（0/1/2
//!   = Ring/Bar/Arc）；Qr 12B = module_count·dark_color·light_color 各
//!   u32 LE；DragReorder 12B = item_index·total_items·on_reorder 各
//!   u32 LE。BadValue 拒面：Card elevation > 3、Gauge value > 1000、
//!   Qr module_count 不在合法 QR 版本域 {21,25,29,...,177}（版本
//!   1..=40 → 21+4×(v-1)）、DragReorder item_index >= total_items
//!   （total_items > 0 时——零项合法 = 空表，Select 同律）；bool 子域
//!   （expanded/selected）与枚举子域（gauge style）域外 UnknownProp、
//!   文本超长 TextTooLong。渲染/布局/手势面同前四批不实现（挂
//!   Phase 2）。
//! - 块×kind 合法域（§8.2 表）：SIZE 容器通则 Col/Row/Grid 合法（Text/
//!   Image/Icon 双源歧义拒——尺寸已在 props 定长域；Pad/Root 拒）；
//!   ALIGN 仅 Col/Row；A11Y 全 kind 合法（落点 = Ui 侧 a11y 表，D12）。
//! - 旧流（v1 六 kind、flags ∈ {0,1}）逐字节不变——bit1/2/3 立法对
//!   不立块的流零扰动（GOLDEN_V1 守卫）。
//! - 拒面全 loud（guiproto 同款纪律）：未知 kind / flags bit4+ / 截断 /
//!   尾随字节 / 叶子带子（Rect/Text/Image/Icon/Button/Toggle/Slider/
//!   Progress + P1 八叶 Spacer/Divider/Check/Radio/Step/Seg/Badge/Sbar
//!   + P0 尾四叶 Select/Input/Edit/Canvas + P1 尾四叶 Tip/SwipeItem/
//!   DatePick/TimePick + P2 五叶 Chip/Fab/Gauge/Qr/DragReorder）/
//!   Root 为子 / 文本超长 / 块×kind 失配 / Grid 零维·超格 /
//!   组件值域越界，一律 `Err`。
//!
//! # 内存纪律
//!
//! 全模块零 alloc、零 panic：`TextBuf` 内联 64B（超长 [`crate::UiError::TextTooLong`]
//! loud）；`MsgBuf` 定容 8（满 → [`crate::UiError::MsgFull`]，绝不静默丢）；
//! 整数运算全先验界内（敌意巨参不炸）。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;

// ---------- 节点身份（arena 代数——§3.1） ----------

/// 节点句柄：`gen<<16 | idx` 打包（gen u16 代数防 remove 后旧 id 误用
/// ——悬空/坏槽在 [`crate::Ui`] 侧统一 `UiError::NodeBad` loud）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) u32);

// ---------- 视图集（v1 六件 + GUI-6 段B 三新件 + Phase 1 组件七件 + P1 九件 + P0 尾四件 + P1 尾五件 + P2 七件——§3.2/§8） ----------

/// 视图种类：`Root/Rect/Text`、容器 `Col/Row/Pad/Grid/Stack` 与新叶
/// `Image/Icon`（叶子带子 = AST 拒面——Rect/Text/Image/Icon 四叶）。
/// Phase 1 组件七件（tag 9..=15 add-only）：新叶 `Button/Toggle/Slider/
/// Progress`（叶带子拒面同列）+ 容器 `Scroll/List/Tabs`（可带子——
/// 渲染/布局/手势面未实现，仅数据结构层）。P1 九件（组件清单多数集合
/// 批，tag 16..=24 add-only）：容器 `Stack`（叠层——子按 z 序全幅叠放）
/// + 八新叶 `Spacer/Divider/Check/Radio/Step/Seg/Badge/Sbar`（叶带子
/// 拒面同列——数据层先行，视觉/交互挂 Phase 2）。P0 尾四件（组件清单
/// 收尾批，tag 25..=28 add-only）：四新叶 `Select/Input/Edit/Canvas`
/// （叶带子拒面同列——数据层先行，输入焦点/绘制协议挂 Phase 2）。
/// P1 尾五件（组件清单 P1 kind 件收尾批，tag 29..=33 add-only）：容器
/// `Refresh`（下拉刷新——包裹 List 等）+ 四新叶 `Tip/SwipeItem/
/// DatePick/TimePick`（叶带子拒面同列——数据层先行，视觉/交互挂
/// Phase 2）。P2 七件（借来件批，tag 34..=40 add-only）：容器
/// `Collapse`（折叠组——子 = 折叠内容）/ `Card`（卡片——子 = 卡片
/// 内容）+ 五新叶 `Chip/Fab/Gauge/Qr/DragReorder`（叶带子拒面同列
/// ——数据层先行，视觉/交互挂 Phase 2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Root,
    Rect,
    Text,
    Col,
    Row,
    Pad,
    Image,
    Icon,
    Grid,
    Button,
    Toggle,
    Slider,
    Progress,
    Scroll,
    List,
    Tabs,
    Stack,
    Spacer,
    Divider,
    Check,
    Radio,
    Step,
    Seg,
    Badge,
    Sbar,
    Select,
    Input,
    Edit,
    Canvas,
    Tip,
    Refresh,
    SwipeItem,
    DatePick,
    TimePick,
    Collapse,
    Chip,
    Fab,
    Card,
    Gauge,
    Qr,
    DragReorder,
}

/// 矩形叶子属性：`None` = 尺寸随父（layout 分配）。
///
/// 刀 3（OH 形）：`radius` = 圆角半径（px；0 = 直角）。wire 面 radius
/// **不在 props 定长域**（保 V1 golden 11B 零漂移）——走 flags bit4
/// 扩展块（2B LE，radius>0 ⟺ 块在；0 无块）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectProps {
    pub bg: Color,
    pub w: Option<u32>,
    pub h: Option<u32>,
    pub radius: u16,
}

/// 文本样式：fg 着墨 / bg 不透明衬底（`None` = 透明底）/ scale 字号档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub fg: Color,
    pub bg: Option<Color>,
    pub scale: FontSize,
}

/// 文本叶子属性（内联文本——超长 loud）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextProps {
    pub style: TextStyle,
    pub text: TextBuf,
}

/// 纵向容器属性（子间缝隙 + GUI-6 段B 十字轴/定尺寸面——w/h 经
/// HAS_SIZE 块落、cross 经 props→ALIGN 块规范形二源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColProps {
    pub gap: u32,
    /// 定宽（`None` = 随子聚合内在——段C 消费；wire 走 HAS_SIZE 块）。
    pub w: Option<u32>,
    /// 定高（同上）。
    pub h: Option<u32>,
    /// 十字轴对齐（wire 非 Stretch 才落 ALIGN 块——规范形单源）。
    pub cross: crate::layout::CrossAlign,
}

/// 横向容器属性（同 [`ColProps`] 镜像——主轴横、十字轴纵）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowProps {
    pub gap: u32,
    pub w: Option<u32>,
    pub h: Option<u32>,
    pub cross: crate::layout::CrossAlign,
}

/// 内缩容器属性（四向同缩——v1 单值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadProps {
    pub all: u32,
}

/// 网格容器属性（GUI-6 §8.1：cols×rows 格 + 格隙 + 十字轴——**cross 在
/// props 定长块内**（Grid 无 ALIGN 块，7B 定长铁则）；w/h 经 HAS_SIZE
/// 块落。cols/rows = 0 → `GridZeroDims` 拒；cols×rows < 子数 →
/// `GridNotFit` 拒（超格不容忍静默截断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridProps {
    pub cols: u8,
    pub rows: u8,
    pub gap: u32,
    pub cross: crate::layout::CrossAlign,
    pub w: Option<u32>,
    pub h: Option<u32>,
}

/// 位图叶子属性（res = 资源 id——内容供给走 D28 icon/image 资源缝；
/// w/h 哨兵对在 props 定长域内，**不吃 HAS_SIZE 块**——双源歧义拒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageProps {
    pub res: u16,
    pub w: Option<u32>,
    pub h: Option<u32>,
}

/// 矢量图标叶子属性（id = 图标集 id；fg 前景着色；w/h 同
/// [`ImageProps`] 定长域语义——不吃 HAS_SIZE 块）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconProps {
    pub id: u16,
    pub fg: Color,
    pub w: Option<u32>,
    pub h: Option<u32>,
}

// ---------- Phase 1 组件七件（数据结构层——渲染/布局/手势挂 Phase 2） ----------

/// 按钮风格（wire u8 0/1/2 = Filled/Outlined/Text）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Filled,
    Outlined,
    Text,
}

/// 按钮叶属性（Phase 1 清单裁决：独立 kind——a11y role 与 golden 对拍
/// 锚；本质 = Rect 背景 + Text 标签 + hit 点击区的组合件，视觉合成挂
/// Phase 2）。`label` 用内联 [`TextBuf`]（≤ `TEXT_INLINE`）而非
/// `&'static str`——零 alloc Copy 值面纪律 + TLV 流自含（与 Text 同款）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonProps {
    pub label: TextBuf,
    pub style: ButtonStyle,
    /// 事件 ID（hit 层派发——Phase 1 仅数据面，不接线）。
    pub on_tap: u32,
}

/// 开关叶属性（视觉 = 圆角轨道 + 圆形滑块；checked 切换位移动画复用
/// anim——Phase 2）。wire checked u8 ∈ {0,1}，域外 UnknownProp 拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleProps {
    pub checked: bool,
    /// 事件 ID（hit 层派发——Phase 1 仅数据面）。
    pub on_change: u32,
}

/// 滑块叶属性（视觉 = 轨道 + 填充 + 拖动圆点；拖动复用 hit 的 drag
/// 事件——Phase 2）。`value` = 0..=1000 permille（清单草图 f32 的免 FP
/// 替身——f32 破 Eq derive 与 crate 整数纪律）；语义值 = min +
/// value×(max-min)/1000（消费面折算）。min > max = BadValue 拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliderProps {
    pub value: u32,
    pub min: u32,
    pub max: u32,
    /// 事件 ID（hit 层派发——Phase 1 仅数据面）。
    pub on_change: u32,
}

/// 进度风格（wire u8 0/1/2 = Linear/Circular/Indeterminate）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressStyle {
    Linear,
    Circular,
    Indeterminate,
}

/// 进度叶属性（`value` = 0..=1000 permille，域外 BadValue 拒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressProps {
    pub value: u32,
    pub style: ProgressStyle,
}

/// 滚动容器属性（滚动 = 子内容 clipping + offset，手势复用 swipe——
/// Phase 2）。`offset_y` > `content_height` = BadValue 拒（视口无关的
/// 结构不变量；对视口的紧上界由消费面钳）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollProps {
    /// 内容总高（> 视口时滚动——视口高由布局语境定，不进 props）。
    pub content_height: u32,
    /// 当前滚动偏移。
    pub offset_y: u32,
}

/// 列表容器属性（v1 全量渲染等高项——懒化等 layout v2，Phase 1 不做）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListProps {
    pub item_count: u32,
    pub item_height: u32,
}

/// 页签容器属性（视觉 = 顶部指示条 + 底部内容区，滑动切换复用
/// SwipePager——Phase 2）。子节点 = 内容区（AST/builder 面可带子；
/// 命令面 add() 父位白名单未收编——BadParent）。零页 / active 越页 =
/// BadValue 拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabsProps {
    pub tab_count: u32,
    pub active: u32,
    /// 事件 ID（hit 层派发——Phase 1 仅数据面）。
    pub on_switch: u32,
}

// ---------- P1 九件（组件清单多数集合批——数据结构层，渲染/布局/手势挂 Phase 2） ----------

/// 叠层容器子元素对齐（wire u8 0/1/2 = Start/Center/End——主轴和交叉
/// 轴同值；无 Stretch 档：叠层子恒全幅或按内在对齐，无「拉满」义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackAlign {
    Start,
    Center,
    End,
}

/// 叠层容器属性（子元素按 z 序**全幅叠放**——前序子在下、后序子盖上；
/// 摆位 = place_children `_` 臂 Root 型分配，天然合义）。容器成员：
/// AST/builder 面可带子；命令面 add() 父位白名单未收编（Phase 1 容器
/// 同款边界）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackProps {
    /// 子元素对齐（内在小于分配时的摆位——消费面 Phase 2）。
    pub align: StackAlign,
}

/// 弹性占位叶属性（剩余空间按 flex 权重等分——1 = 与同权兄弟等分；
/// 0 = 零权重占位。分配语义挂 Phase 2 布局刀，本层纯数据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpacerProps {
    pub flex: u32,
}

/// 分隔线叶属性（走向随容器主轴——消费面定；thickness=0 合法 = 零厚
/// 空绘，几何退化留给 Phase 2 视觉面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DividerProps {
    /// 像素厚度。
    pub thickness: u32,
    /// 起端留白（像素）。
    pub inset_start: u32,
    /// 末端留白（像素）。
    pub inset_end: u32,
}

/// 复选框叶属性（视觉 = 方框 + 勾选符；wire checked u8 ∈ {0,1}，域外
/// UnknownProp 拒——Toggle 同款）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckProps {
    pub checked: bool,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_change: u32,
}

/// 单选按钮叶属性（`group` = 组 ID，同组互斥——互斥执法在 app/引擎
/// 接线面挂 Phase 2，数据层只载组号）。wire selected u8 ∈ {0,1}。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioProps {
    pub selected: bool,
    /// 组 ID（同组互斥）。
    pub group: u32,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_change: u32,
}

/// 步进器叶属性（value/min/max/step 全 u32 绝对值域——Slider 的
/// permille 是「范围内比例」，Step 是「绝对刻度」，两域不同源）。
/// BadValue 拒面（检序即此序）：step == 0（零步长步进无义）、
/// min > max、value < min、value > max。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepProps {
    pub value: u32,
    pub min: u32,
    pub max: u32,
    pub step: u32,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_change: u32,
}

/// 分段按钮叶属性（Tabs 的叶化形态——无内容区子面板，纯选择器）。
/// 零段 / active 越段 = BadValue 拒（Tabs 同律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegProps {
    pub segment_count: u32,
    pub active: u32,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_change: u32,
}

/// 角标叶属性（`count` = 0 隐藏；99+ 封顶显示是**视觉面**语义——数据
/// 层载原值不截断，消费面自钉「>99 显示 99+」）。wire show_dot u8
/// ∈ {0,1}（纯圆点模式——count 语义让位圆点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadgeProps {
    pub count: u32,
    /// 纯圆点模式（无数字角标）。
    pub show_dot: bool,
}

/// 滚动条走向（wire u8 0/1 = Vertical/Horizontal）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

/// 滚动条叶属性（thumb 区间 = 视口在内容里的位置比例，两端各
/// 0..=1000 permille——免 FP 同 Slider 纪律）。BadValue 拒面：任一端
/// > 1000、thumb_start > thumb_end（零长合法 = 恰好一端）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbarProps {
    pub orientation: Orientation,
    /// 滑块起点（0..=1000 permille）。
    pub thumb_start: u32,
    /// 滑块终点（0..=1000 permille）。
    pub thumb_end: u32,
}

// ---------- P0 尾四件（组件清单收尾批——数据结构层，输入焦点/绘制协议挂 Phase 2） ----------

/// 下拉选择器叶属性（选项集 = 计数面：选项内容不进数据层——展开面/
/// 选项供给挂 Phase 2；Tabs 的内容区子面板与 Seg 的纯选择器之间取
/// Seg 形：无子面板，纯选择器）。BadValue 拒面：`selected >=
/// option_count`（option_count > 0 时）；option_count == 0 合法 = 空
/// 选择器（与 Tabs/Seg 零段拒不同律——空下拉有义，selected 恒 0）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectProps {
    /// 选项数。
    pub option_count: u32,
    /// 当前选中索引。
    pub selected: u32,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_change: u32,
}

/// 单行输入类型（wire u8 0/1/2 = Text/Number/Password——键盘面/掩码
/// 面消费，Phase 2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Text,
    Number,
    Password,
}

/// 单行输入叶属性（**实际文本内容不走 Props**——走输入焦点协议
/// （Phase 2 接线），数据层只描述外观和约束：占位文本 + 长度帽 +
/// 输入类型）。`placeholder` 用内联 [`TextBuf`]（≤ `TEXT_INLINE`）
/// ——Button label 同款零 alloc Copy 值面纪律 + TLV 流自含。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputProps {
    /// 占位文本（空文本合法 = 无占位）。
    pub placeholder: TextBuf,
    /// 最大字符数（输入面执法挂 Phase 2——数据层只载约束）。
    pub max_len: u32,
    /// 输入类型（键盘面/掩码面消费——Phase 2）。
    pub input_kind: InputKind,
}

/// 多行编辑叶属性（内容同 [`InputProps`] 不走 Props——输入焦点协议
/// Phase 2 接线；`max_lines` = 0 无限制）。wire readonly u8 ∈ {0,1}，
/// 域外 UnknownProp 拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditProps {
    /// 占位文本。
    pub placeholder: TextBuf,
    /// 最大字符数。
    pub max_len: u32,
    /// 最大行数（0 = 无限制）。
    pub max_lines: u32,
    /// 只读位。
    pub readonly: bool,
}

/// 自绘面叶属性（**叶子**——绘制内容不是子节点：`paint_ops` 只记
/// 绘制操作计数，实际 op 流走绘制协议（Phase 2）；数据层单字段记数
/// 供 damage/telemetry 预算面消费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasProps {
    /// 绘制操作计数。
    pub paint_ops: u32,
}

// ---------- P1 尾五件（组件清单 P1 kind 件收尾批——数据结构层，渲染/布局/手势挂 Phase 2） ----------

/// 工具提示方位（wire u8 0/1/2/3 = Top/Bottom/Start/End——Start/End
/// 取布局走向语义：LTR 下即左/右）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipPlacement {
    Top,
    Bottom,
    Start,
    End,
}

/// 工具提示叶属性（`text` 用内联 [`TextBuf`]（≤ `TEXT_INLINE`）——
/// Button label 同款零 alloc Copy 值面纪律 + TLV 流自含。提示浮层/
/// 定位面挂 Phase 2，数据层只载文本与方位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TipProps {
    /// 提示文本。
    pub text: TextBuf,
    /// 显示方位。
    pub placement: TipPlacement,
}

/// 下拉刷新容器属性（包裹 List 等——子 = 被包裹内容；拉动手势与指示
/// 器视觉挂 Phase 2）。`offset` = 0..=1000 permille（免 FP 同 Slider
/// 纪律——拉动偏移对触发阈的比例）。wire refreshing u8 ∈ {0,1}，域外
/// UnknownProp 拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshProps {
    /// 刷新中状态（指示器转圈/驻留——消费面 Phase 2）。
    pub refreshing: bool,
    /// 当前拉动偏移（0..=1000 permille，域外 BadValue 拒）。
    pub offset: u32,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_refresh: u32,
}

/// 滑动操作项叶属性（List 行的滑动操作载体——左右各可滑出多动作；
/// 动作内容不进数据层，`action_count` 只记数（Select 选项计数面同律）。
/// `reveal_*` 各 0..=1000 permille——起末端滑出量，两端独立无序（与
/// Sbar thumb 的 start≤end 区间律不同源：两侧动作区各自展开）。
/// BadValue 拒面：任一端 > 1000。零动作合法 = 纯行项（List 零项同律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwipeItemProps {
    /// 滑出动作数（左/右合计）。
    pub action_count: u32,
    /// 起端已滑出量（0..=1000 permille）。
    pub reveal_start: u32,
    /// 末端已滑出量（0..=1000 permille）。
    pub reveal_end: u32,
    /// 动作事件 ID（hit 层派发——本批仅数据面）。
    pub on_action: u32,
}

/// 日期选择器叶属性（滚轮/日历面板视觉挂 Phase 2）。**日历语义只做
/// 结构域检**：month 1..=12、day 1..=31（不做月长感知——2 月 31 结构
/// 合法，消费面钳）、min_year ≤ max_year、year ∈ [min_year, max_year]
/// （min/max 为 0 时含 0——数据层不立法历法纪元）。BadValue 拒面（检
/// 序即此序）：month 越域、day 越域、min_year > max_year、year 越界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatePickProps {
    /// 当前选中年（∈ [min_year, max_year]）。
    pub year: u32,
    /// 月（1..=12）。
    pub month: u32,
    /// 日（1..=31）。
    pub day: u32,
    /// 年域下界。
    pub min_year: u32,
    /// 年域上界。
    pub max_year: u32,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_change: u32,
}

/// 时间选择器叶属性（滚轮视觉挂 Phase 2）。BadValue 拒面：hour > 23、
/// minute > 59。wire use_24h u8 ∈ {0,1}，域外 UnknownProp 拒。12 小时
/// 制下 hour 仍存 0..=23（上下午折叠是视觉面语义——消费面自换算）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimePickProps {
    /// 时（0..=23）。
    pub hour: u32,
    /// 分（0..=59）。
    pub minute: u32,
    /// 24 小时制。
    pub use_24h: bool,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_change: u32,
}

// ---------- P2 七件（借来件批——数据结构层，渲染/布局/手势挂 Phase 2） ----------

/// 折叠组容器属性（子 = 折叠内容；展开/收起动画复用 anim——Phase 2。
/// 收起态的子内容裁剪也是视觉面语义，数据层只载开关位）。wire
/// expanded u8 ∈ {0,1}，域外 UnknownProp 拒（Toggle 同律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollapseProps {
    /// 展开状态（false = 收起）。
    pub expanded: bool,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_toggle: u32,
}

/// 胶囊标签叶属性（视觉 = 胶囊形背景 + Text + 选中态——本质 Button
/// 变体，独立 kind 为 a11y role 锚；视觉合成挂 Phase 2）。`label` 用
/// 内联 [`TextBuf`]（≤ `TEXT_INLINE`）——Button label 同款零 alloc
/// Copy 值面纪律 + TLV 流自含。wire selected u8 ∈ {0,1}，域外
/// UnknownProp 拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChipProps {
    /// 标签文本。
    pub label: TextBuf,
    /// 选中态（选中底色/描边是视觉面——数据层只载位）。
    pub selected: bool,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_tap: u32,
}

/// 悬浮操作按钮叶属性（视觉 = 圆形按钮 + elevation——Phase 2）。
/// `extended_label` 空 = 纯圆形；非空 = 扩展形态（圆形 + 侧标签）。
/// `icon` = 图标资源 ID（供给走 D28 资源缝）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FabProps {
    /// 图标资源 ID。
    pub icon: u32,
    /// 扩展标签（空 = 纯圆形）。
    pub extended_label: TextBuf,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_tap: u32,
}

/// 卡片容器属性（子 = 卡片内容；elevation 阴影层级与圆角视觉挂
/// Phase 2，数据层只载几何参数）。BadValue 拒面：elevation > 3。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardProps {
    /// 阴影层级（0..=3）。
    pub elevation: u32,
    /// 圆角像素。
    pub corner_radius: u32,
}

/// 仪表盘风格（wire u8 0/1/2 = Ring/Bar/Arc）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaugeStyle {
    Ring,
    Bar,
    Arc,
}

/// 仪表盘叶属性（`value` = 0..=1000 permille，域外 BadValue 拒——
/// Slider/Progress 同款免 FP 纪律；环形/条形/弧形视觉挂 Phase 2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaugeProps {
    /// 当前值（0..=1000 permille）。
    pub value: u32,
    /// 表盘风格。
    pub style: GaugeStyle,
}

/// 二维码叶属性（数据层只记模块数与双色——点阵矩阵生成/定位图案
/// 挂 Phase 2；颜色 = RGBA u32 打包，与 PRESSED_BG 同口径）。
/// BadValue 拒面：module_count 不在合法 QR 版本域 {21,25,29,...,177}
/// （版本 1..=40 → 21+4×(v-1)）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QrProps {
    /// 模块数（QR 版本对应：21/25/29/.../177）。
    pub module_count: u32,
    /// 暗色模块 RGBA。
    pub dark_color: u32,
    /// 亮色模块 RGBA。
    pub light_color: u32,
}

/// 拖拽重排手柄叶属性（List 行的重排载体；拖动手势与落位动画挂
/// Phase 2）。BadValue 拒面：`item_index >= total_items`（total_items
/// > 0 时）；total_items == 0 合法 = 空表（Select 零项同律——手柄在
/// 空表上无操作位但有义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragReorderProps {
    /// 手柄所辖项索引。
    pub item_index: u32,
    /// 列表总项数。
    pub total_items: u32,
    /// 事件 ID（hit 层派发——本批仅数据面）。
    pub on_reorder: u32,
}

/// 属性族（Plain Data，Copy——「声明即数据」的值面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Props {
    Root,
    Rect(RectProps),
    Text(TextProps),
    Col(ColProps),
    Row(RowProps),
    Pad(PadProps),
    Image(ImageProps),
    Icon(IconProps),
    Grid(GridProps),
    Button(ButtonProps),
    Toggle(ToggleProps),
    Slider(SliderProps),
    Progress(ProgressProps),
    Scroll(ScrollProps),
    List(ListProps),
    Tabs(TabsProps),
    Stack(StackProps),
    Spacer(SpacerProps),
    Divider(DividerProps),
    Check(CheckProps),
    Radio(RadioProps),
    Step(StepProps),
    Seg(SegProps),
    Badge(BadgeProps),
    Sbar(SbarProps),
    Select(SelectProps),
    Input(InputProps),
    Edit(EditProps),
    Canvas(CanvasProps),
    Tip(TipProps),
    Refresh(RefreshProps),
    SwipeItem(SwipeItemProps),
    DatePick(DatePickProps),
    TimePick(TimePickProps),
    Collapse(CollapseProps),
    Chip(ChipProps),
    Fab(FabProps),
    Card(CardProps),
    Gauge(GaugeProps),
    Qr(QrProps),
    DragReorder(DragReorderProps),
}

/// builder 入口（入口①）的单节点描述：`key != 0` = 调和键（v1 仅随
/// AST 透传，reconcile 键路径 diff 挂账 v1.5/S7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewSpec {
    pub kind: ViewKind,
    pub key: u64,
    pub props: Props,
}

// ---------- 内联文本 ----------

/// 内联文本缓冲（no_std 零 alloc）：定长 64B + len。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextBuf {
    pub bytes: [u8; crate::TEXT_INLINE],
    pub len: u8,
}

impl TextBuf {
    /// 构造：超 `TEXT_INLINE` → [`crate::UiError::TextTooLong`] loud。
    pub fn new(bytes: &[u8]) -> Result<Self, crate::UiError> {
        if bytes.len() > crate::TEXT_INLINE {
            return Err(crate::UiError::TextTooLong { len: bytes.len() });
        }
        let mut buf = [0u8; crate::TEXT_INLINE];
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(TextBuf { bytes: buf, len: bytes.len() as u8 })
    }

    /// 有效字节切片。
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    /// 空文本谓词。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

// ---------- Msg 面（Msg-only 回调——引擎产、app update 消费） ----------

/// 拖拽锁轴（GUI-6 D15/D25）：slop 欧氏触发时按位移主轴锁定
/// （`|dx| >= |dy|` → X，tie 取 X——文档化裁定），此后 DragMove 只透传
/// 锁定轴分量（跨轴分量冻结在 DragStart 触发样本值——防对角抖动）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// 输入消息七变体（§3.6 手势 referee 产物；引擎内零闭包零回调副作用）。
/// GUI-6 D25 扩域（趁 gfxdemo/launcher 零消费者窗口）：DragStart 携
/// `axis`（锁轴）、DragEnd 携 `vx`/`vy`（速度环差分产物，px/s，i32 钳
/// ±32768）——ADR #52 形态不动（引擎产/app 消费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    Press { node: NodeId, x: u32, y: u32 },
    Release { node: NodeId, x: u32, y: u32 },
    Click { node: NodeId },
    DragStart { node: NodeId, axis: Axis, x: u32, y: u32 },
    DragMove { node: NodeId, x: u32, y: u32, resampled: bool },
    /// `vx`/`vy` = 释放时速度环 96ms 窗差分（px/s；样本 <2 / Δt==0 /
    /// 非拖收尾 = 0；i32 钳 ±32768）。
    DragEnd { node: NodeId, vx: i32, vy: i32 },
    Cancel { node: NodeId },
}

/// 定容消息缓冲（MSG_MAX=8；满 → `UiError::MsgFull` loud——不自陷不静默丢）。
pub struct MsgBuf {
    slots: [Option<Msg>; crate::MSG_MAX],
    n: usize,
}

impl MsgBuf {
    /// 空缓冲（引擎 pump_input / hit::dispatch 的出参构造口）。
    pub(crate) fn new() -> Self {
        MsgBuf { slots: [None; crate::MSG_MAX], n: 0 }
    }

    /// 追加（crate 内：hit::dispatch 消费）。满 → `UiError::MsgFull`。
    pub(crate) fn push(&mut self, m: Msg) -> Result<(), crate::UiError> {
        if self.n >= crate::MSG_MAX {
            return Err(crate::UiError::MsgFull);
        }
        self.slots[self.n] = Some(m);
        self.n += 1;
        Ok(())
    }

    /// 顺序迭代（产出序即手势时序）。
    pub fn iter(&self) -> impl Iterator<Item = &Msg> {
        self.slots[..self.n].iter().filter_map(|s| s.as_ref())
    }

    /// 末位 Msg 可变引用（空 → None）。crate 内消费面 = pump_input 对
    /// 重采样路径产物 Msg 的 `resampled` 标注（§3.9 引擎侧义务）。
    pub(crate) fn last_mut(&mut self) -> Option<&mut Msg> {
        if self.n == 0 {
            None
        } else {
            self.slots[self.n - 1].as_mut()
        }
    }

    /// 按序取 Msg 副本（越界 → None）。crate 内消费面 = pump_input 的
    /// D18 pressed 短路径——本事件产出的 Msg 逐条即时反应（Msg: Copy，
    /// 副本出借不持 buf 引用，`&mut Ui` 可同帧改 props）。
    pub(crate) fn at(&self, i: usize) -> Option<Msg> {
        self.slots.get(i).and_then(|s| *s)
    }

    /// 已收消息数。
    pub fn len(&self) -> usize {
        self.n
    }

    /// 空谓词。
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
}

// ---------- AST 编解码错误（拒面值级断言面） ----------

/// AST 二进制拒面（全 loud；derive 值级断言面——§0 风格铁律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstError {
    /// 输出缓冲不足（编码/构造面容量拒）；亦承载 child_count u8 域溢出
    /// （>255 子）与 arena 容量映射（need = 欲纳节点数）。
    TooSmall { need: usize, got: usize },
    /// 未知视图种类字节。
    UnknownKind { got: u8 },
    /// flags 未知位（bit0..3 = has_key/has_size/has_a11y/has_align 之外）。
    UnknownFlags { got: u8 },
    /// 属性域未知/失配子值：Text 的 scale / bg tag；块×kind 失配
    /// （§8.2 合法域表拒——`got` = 块旗位或块载荷值）；非规范形块载荷
    /// （size 双 None / align 载 Stretch 缺省）。
    UnknownProp { kind: ViewKind, got: u8 },
    /// 输入流截断（节点/props/child_count 中途尽）。
    Truncated,
    /// 根树解析毕仍有尾随字节。
    TrailingBytes { extra: usize },
    /// 树形拒面：流首非 Root / Root 为子 / 叶子带子 / begin·end 失配。
    BadParent,
    /// 文本超 `TEXT_INLINE`。
    TextTooLong { len: usize },
    /// Grid 拒面（D21 add-only）：cols/rows = 0（零维格无几何义——
    /// 不容忍缺省补 1）。
    GridZeroDims,
    /// Grid 拒面：cols×rows < children（超格不容忍静默截断/溢排）。
    /// `children` = 不适配时的子数（builder 面即被拒的第 n+1 子计数）。
    GridNotFit { cols: u8, rows: u8, children: u32 },
    /// 组件值域拒面（Phase 1 + P1 批 add-only——`UnknownProp` 的 u32
    /// 载荷版：越界值超 u8 不堪载）：Slider value > 1000 或 min > max、
    /// Progress value > 1000、Scroll offset_y > content_height、Tabs
    /// tab_count == 0 或 active >= tab_count；P1 批追加——Step step == 0
    /// 或 min > max 或 value 越界 [min,max]、Seg segment_count == 0 或
    /// active >= segment_count、Sbar thumb 任一端 > 1000 或
    /// thumb_start > thumb_end；P0 尾四件批追加——Select
    /// option_count > 0 时 selected >= option_count；P1 尾五件批追加
    /// ——Refresh offset > 1000、SwipeItem reveal 任一端 > 1000、
    /// DatePick month 非 1..=12 或 day 非 1..=31 或 min_year > max_year
    /// 或 year 越界、TimePick hour > 23 或 minute > 59；P2 七件批追加
    /// ——Card elevation > 3、Gauge value > 1000、Qr module_count 不在
    /// {21,25,29,...,177}、DragReorder total_items > 0 时
    /// item_index >= total_items。`got` = 违规值。
    BadValue { kind: ViewKind, got: u32 },
}

// ---------- TLV 格式常量（wire 值域——编解码双侧单源在此） ----------

/// kind 字节域（编解码对偶表；新增视图种类 = 双侧同步 + UnknownKind 拒旧流）。
pub const KIND_ROOT: u8 = 0;
pub const KIND_RECT: u8 = 1;
pub const KIND_TEXT: u8 = 2;
pub const KIND_COL: u8 = 3;
pub const KIND_ROW: u8 = 4;
pub const KIND_PAD: u8 = 5;
/// GUI-6 D5 版本域扩展：Image/Icon/Grid（旧流值域 0..=5 不受扰——
/// 6..=8 旧读端本就 UnknownKind 拒，无双义重编）。
pub const KIND_IMAGE: u8 = 6;
pub const KIND_ICON: u8 = 7;
pub const KIND_GRID: u8 = 8;
/// Phase 1 组件七件（add-only 追加——旧流值域 0..=8 不受扰，9..=15
/// 旧读端本就 UnknownKind 拒，无双义重编）。
pub const KIND_BUTTON: u8 = 9;
pub const KIND_TOGGLE: u8 = 10;
pub const KIND_SLIDER: u8 = 11;
pub const KIND_PROGRESS: u8 = 12;
pub const KIND_SCROLL: u8 = 13;
pub const KIND_LIST: u8 = 14;
pub const KIND_TABS: u8 = 15;
/// P1 九件（组件清单多数集合批，add-only 追加——旧流值域 0..=15 不受
/// 扰，16..=24 旧读端本就 UnknownKind 拒，无双义重编）。
pub const KIND_STACK: u8 = 16;
pub const KIND_SPACER: u8 = 17;
pub const KIND_DIVIDER: u8 = 18;
pub const KIND_CHECK: u8 = 19;
pub const KIND_RADIO: u8 = 20;
pub const KIND_STEP: u8 = 21;
pub const KIND_SEG: u8 = 22;
pub const KIND_BADGE: u8 = 23;
pub const KIND_SBAR: u8 = 24;
/// P0 尾四件（组件清单收尾批，add-only 追加——旧流值域 0..=24 不受
/// 扰，25..=28 旧读端本就 UnknownKind 拒，无双义重编）。
pub const KIND_SELECT: u8 = 25;
pub const KIND_INPUT: u8 = 26;
pub const KIND_EDIT: u8 = 27;
pub const KIND_CANVAS: u8 = 28;
/// P1 尾五件（组件清单 P1 kind 件收尾批，add-only 追加——旧流值域
/// 0..=28 不受扰，29..=33 旧读端本就 UnknownKind 拒，无双义重编）。
pub const KIND_TIP: u8 = 29;
pub const KIND_REFRESH: u8 = 30;
pub const KIND_SWIPE_ITEM: u8 = 31;
pub const KIND_DATE_PICK: u8 = 32;
pub const KIND_TIME_PICK: u8 = 33;
/// P2 七件（借来件批，add-only 追加——旧流值域 0..=33 不受扰，34..=40
/// 旧读端本就 UnknownKind 拒，无双义重编）。
pub const KIND_COLLAPSE: u8 = 34;
pub const KIND_CHIP: u8 = 35;
pub const KIND_FAB: u8 = 36;
pub const KIND_CARD: u8 = 37;
pub const KIND_GAUGE: u8 = 38;
pub const KIND_QR: u8 = 39;
pub const KIND_DRAG_REORDER: u8 = 40;

/// Slider/Progress 的 permille 值域上界（0..=1000——清单草图 f32 0.0..=1.0
/// 的整数替身；编解码双侧单源）。
pub const PERMILLE_MAX: u32 = 1000;

/// Card 阴影层级上界（0..=3——Material 四档阴影；编解码双侧单源）。
pub const CARD_ELEVATION_MAX: u32 = 3;

/// Qr 模块数合法域边界（QR 版本 1..=40 → 21+4×(v-1)：{21,25,29,...,
/// 177} 步长 4——编解码双侧单源；域内成员判定 [`qr_module_count_valid`]）。
pub const QR_MODULE_MIN: u32 = 21;
pub const QR_MODULE_MAX: u32 = 177;

/// Qr 模块数合法域成员判定（21 ≤ m ≤ 177 且 (m-21) % 4 == 0）。
/// pub(crate)：lib.rs 命令面值域执法（widget_value_ok）同律共用——
/// AST/builder/命令面三口单源。
pub(crate) fn qr_module_count_valid(m: u32) -> bool {
    (QR_MODULE_MIN..=QR_MODULE_MAX).contains(&m) && (m - QR_MODULE_MIN) % 4 == 0
}

/// flags bit0 = has_key（v1）。
pub const FLAG_HAS_KEY: u8 = 1;
/// flags bit1 = has_size（GUI-6 D5：size 块 8B——w/h 各 u32 LE 哨兵对）。
pub const FLAG_HAS_SIZE: u8 = 1 << 1;
/// flags bit2 = has_a11y（role u8 + len u8 + label ≤64B——落 Ui 侧表）。
pub const FLAG_HAS_A11Y: u8 = 1 << 2;
/// flags bit3 = has_align（cross u8——仅 Col/Row）。
pub const FLAG_HAS_ALIGN: u8 = 1 << 3;
/// 刀 3（OH 形）：radius 扩展块位——Rect 圆角（radius>0 ⟺ 块在；0 无块
/// = V1 流零漂移）。块 = 2B LE，序在 ALIGN 后（扩展块尾）。
pub const FLAG_HAS_RADIUS: u8 = 1 << 4;
/// flags 已知位掩码（bit4+ → UnknownFlags 拒——绊线改钉 0x10）。
pub const FLAG_KNOWN: u8 = FLAG_HAS_KEY | FLAG_HAS_SIZE | FLAG_HAS_A11Y | FLAG_HAS_ALIGN | FLAG_HAS_RADIUS;

/// size 块/定长域 `Option<u32>` 的 wire 哨兵（「未定」档——与 Rect 的
/// OPT_NONE 同值同义，GUI-6 起升 pub 单源名）。
pub const SIZE_NONE: u32 = u32::MAX;

/// `Option<u32>` 的 wire 哨兵（Rect w/h「随父」档——v1 原名，Rect 专用）。
const OPT_NONE: u32 = u32::MAX;

/// DescBuilder 缓冲容量（整树字节上界；D10 容量帽一次到位 4096→8192
/// ——MAX_NODES=512 契约树的水位余量）。
pub const DESC_MAX: usize = 8192;

/// DescBuilder 嵌套深度帽（容器嵌套上界；超 → TooSmall loud）。D11：
/// 16→64（护**构建期**递归；与 D29 arena 运行期树深帽 TREE_DEPTH_MAX=96
/// 分工——64 < 96 自洽，lib.rs const 断言钉死）。
pub const DESC_DEPTH_MAX: usize = 64;

/// child_count u8 域上界。
const CHILD_MAX: usize = 255;

fn kind_to_u8(k: ViewKind) -> u8 {
    match k {
        ViewKind::Root => KIND_ROOT,
        ViewKind::Rect => KIND_RECT,
        ViewKind::Text => KIND_TEXT,
        ViewKind::Col => KIND_COL,
        ViewKind::Row => KIND_ROW,
        ViewKind::Pad => KIND_PAD,
        ViewKind::Image => KIND_IMAGE,
        ViewKind::Icon => KIND_ICON,
        ViewKind::Grid => KIND_GRID,
        ViewKind::Button => KIND_BUTTON,
        ViewKind::Toggle => KIND_TOGGLE,
        ViewKind::Slider => KIND_SLIDER,
        ViewKind::Progress => KIND_PROGRESS,
        ViewKind::Scroll => KIND_SCROLL,
        ViewKind::List => KIND_LIST,
        ViewKind::Tabs => KIND_TABS,
        ViewKind::Stack => KIND_STACK,
        ViewKind::Spacer => KIND_SPACER,
        ViewKind::Divider => KIND_DIVIDER,
        ViewKind::Check => KIND_CHECK,
        ViewKind::Radio => KIND_RADIO,
        ViewKind::Step => KIND_STEP,
        ViewKind::Seg => KIND_SEG,
        ViewKind::Badge => KIND_BADGE,
        ViewKind::Sbar => KIND_SBAR,
        ViewKind::Select => KIND_SELECT,
        ViewKind::Input => KIND_INPUT,
        ViewKind::Edit => KIND_EDIT,
        ViewKind::Canvas => KIND_CANVAS,
        ViewKind::Tip => KIND_TIP,
        ViewKind::Refresh => KIND_REFRESH,
        ViewKind::SwipeItem => KIND_SWIPE_ITEM,
        ViewKind::DatePick => KIND_DATE_PICK,
        ViewKind::TimePick => KIND_TIME_PICK,
        ViewKind::Collapse => KIND_COLLAPSE,
        ViewKind::Chip => KIND_CHIP,
        ViewKind::Fab => KIND_FAB,
        ViewKind::Card => KIND_CARD,
        ViewKind::Gauge => KIND_GAUGE,
        ViewKind::Qr => KIND_QR,
        ViewKind::DragReorder => KIND_DRAG_REORDER,
    }
}

fn kind_from_u8(v: u8) -> Option<ViewKind> {
    match v {
        KIND_ROOT => Some(ViewKind::Root),
        KIND_RECT => Some(ViewKind::Rect),
        KIND_TEXT => Some(ViewKind::Text),
        KIND_COL => Some(ViewKind::Col),
        KIND_ROW => Some(ViewKind::Row),
        KIND_PAD => Some(ViewKind::Pad),
        KIND_IMAGE => Some(ViewKind::Image),
        KIND_ICON => Some(ViewKind::Icon),
        KIND_GRID => Some(ViewKind::Grid),
        KIND_BUTTON => Some(ViewKind::Button),
        KIND_TOGGLE => Some(ViewKind::Toggle),
        KIND_SLIDER => Some(ViewKind::Slider),
        KIND_PROGRESS => Some(ViewKind::Progress),
        KIND_SCROLL => Some(ViewKind::Scroll),
        KIND_LIST => Some(ViewKind::List),
        KIND_TABS => Some(ViewKind::Tabs),
        KIND_STACK => Some(ViewKind::Stack),
        KIND_SPACER => Some(ViewKind::Spacer),
        KIND_DIVIDER => Some(ViewKind::Divider),
        KIND_CHECK => Some(ViewKind::Check),
        KIND_RADIO => Some(ViewKind::Radio),
        KIND_STEP => Some(ViewKind::Step),
        KIND_SEG => Some(ViewKind::Seg),
        KIND_BADGE => Some(ViewKind::Badge),
        KIND_SBAR => Some(ViewKind::Sbar),
        KIND_SELECT => Some(ViewKind::Select),
        KIND_INPUT => Some(ViewKind::Input),
        KIND_EDIT => Some(ViewKind::Edit),
        KIND_CANVAS => Some(ViewKind::Canvas),
        KIND_TIP => Some(ViewKind::Tip),
        KIND_REFRESH => Some(ViewKind::Refresh),
        KIND_SWIPE_ITEM => Some(ViewKind::SwipeItem),
        KIND_DATE_PICK => Some(ViewKind::DatePick),
        KIND_TIME_PICK => Some(ViewKind::TimePick),
        KIND_COLLAPSE => Some(ViewKind::Collapse),
        KIND_CHIP => Some(ViewKind::Chip),
        KIND_FAB => Some(ViewKind::Fab),
        KIND_CARD => Some(ViewKind::Card),
        KIND_GAUGE => Some(ViewKind::Gauge),
        KIND_QR => Some(ViewKind::Qr),
        KIND_DRAG_REORDER => Some(ViewKind::DragReorder),
        _ => None,
    }
}

/// CrossAlign → wire u8（0/1/2——编解码双侧 + builder ALIGN 块单源）。
pub(crate) fn cross_to_u8(c: crate::layout::CrossAlign) -> u8 {
    match c {
        crate::layout::CrossAlign::Start => 0,
        crate::layout::CrossAlign::Center => 1,
        crate::layout::CrossAlign::Stretch => 2,
    }
}

/// wire u8 → CrossAlign（值域外 → UnknownProp——`kind` 入载荷供拒面定位）。
pub(crate) fn cross_from_u8(kind: ViewKind, v: u8) -> Result<crate::layout::CrossAlign, AstError> {
    match v {
        0 => Ok(crate::layout::CrossAlign::Start),
        1 => Ok(crate::layout::CrossAlign::Center),
        2 => Ok(crate::layout::CrossAlign::Stretch),
        _ => Err(AstError::UnknownProp { kind, got: v }),
    }
}

/// FontSize → 档位 id（wire 值域 0/1/2/3；TextMetrics 缝同序单源——
/// pub(crate) 供 layout Text 臂经 `tm.measure(text, id)` 走缝）。0..=2
/// 是 GUI-6 冻结旧值；3 = 32×48 HD 档，add-only 扩域。
pub(crate) fn scale_to_u8(s: FontSize) -> u8 {
    match s {
        FontSize::S8x16 => 0,
        FontSize::S16x32 => 1,
        FontSize::S24x48 => 2,
        FontSize::S32x48 => 3,
    }
}

fn scale_from_u8(kind: ViewKind, v: u8) -> Result<FontSize, AstError> {
    match v {
        0 => Ok(FontSize::S8x16),
        1 => Ok(FontSize::S16x32),
        2 => Ok(FontSize::S24x48),
        3 => Ok(FontSize::S32x48),
        _ => Err(AstError::UnknownProp { kind, got: v }),
    }
}

/// ButtonStyle → wire u8（0/1/2——编解码双侧单源）。
fn button_style_to_u8(s: ButtonStyle) -> u8 {
    match s {
        ButtonStyle::Filled => 0,
        ButtonStyle::Outlined => 1,
        ButtonStyle::Text => 2,
    }
}

/// wire u8 → ButtonStyle（值域外 → UnknownProp——`kind` 入载荷供拒面定位）。
fn button_style_from_u8(kind: ViewKind, v: u8) -> Result<ButtonStyle, AstError> {
    match v {
        0 => Ok(ButtonStyle::Filled),
        1 => Ok(ButtonStyle::Outlined),
        2 => Ok(ButtonStyle::Text),
        _ => Err(AstError::UnknownProp { kind, got: v }),
    }
}

/// ProgressStyle → wire u8（0/1/2——编解码双侧单源）。
fn progress_style_to_u8(s: ProgressStyle) -> u8 {
    match s {
        ProgressStyle::Linear => 0,
        ProgressStyle::Circular => 1,
        ProgressStyle::Indeterminate => 2,
    }
}

/// wire u8 → ProgressStyle（值域外 → UnknownProp）。
fn progress_style_from_u8(kind: ViewKind, v: u8) -> Result<ProgressStyle, AstError> {
    match v {
        0 => Ok(ProgressStyle::Linear),
        1 => Ok(ProgressStyle::Circular),
        2 => Ok(ProgressStyle::Indeterminate),
        _ => Err(AstError::UnknownProp { kind, got: v }),
    }
}

/// StackAlign → wire u8（0/1/2——编解码双侧单源；缺省 Start 与 cross 的
/// Stretch 缺省不同源：叠层无拉满义，Start 是自然缺省）。
fn stack_align_to_u8(a: StackAlign) -> u8 {
    match a {
        StackAlign::Start => 0,
        StackAlign::Center => 1,
        StackAlign::End => 2,
    }
}

/// wire u8 → StackAlign（值域外 → UnknownProp——`kind` 入载荷供拒面定位）。
fn stack_align_from_u8(kind: ViewKind, v: u8) -> Result<StackAlign, AstError> {
    match v {
        0 => Ok(StackAlign::Start),
        1 => Ok(StackAlign::Center),
        2 => Ok(StackAlign::End),
        _ => Err(AstError::UnknownProp { kind, got: v }),
    }
}

/// Orientation → wire u8（0/1——编解码双侧单源）。
fn orientation_to_u8(o: Orientation) -> u8 {
    match o {
        Orientation::Vertical => 0,
        Orientation::Horizontal => 1,
    }
}

/// wire u8 → Orientation（值域外 → UnknownProp）。
fn orientation_from_u8(kind: ViewKind, v: u8) -> Result<Orientation, AstError> {
    match v {
        0 => Ok(Orientation::Vertical),
        1 => Ok(Orientation::Horizontal),
        _ => Err(AstError::UnknownProp { kind, got: v }),
    }
}

/// InputKind → wire u8（0/1/2——编解码双侧单源）。
fn input_kind_to_u8(k: InputKind) -> u8 {
    match k {
        InputKind::Text => 0,
        InputKind::Number => 1,
        InputKind::Password => 2,
    }
}

/// wire u8 → InputKind（值域外 → UnknownProp——`kind` 入载荷供拒面定位）。
fn input_kind_from_u8(kind: ViewKind, v: u8) -> Result<InputKind, AstError> {
    match v {
        0 => Ok(InputKind::Text),
        1 => Ok(InputKind::Number),
        2 => Ok(InputKind::Password),
        _ => Err(AstError::UnknownProp { kind, got: v }),
    }
}

/// TipPlacement → wire u8（0/1/2/3——编解码双侧单源）。
fn tip_placement_to_u8(p: TipPlacement) -> u8 {
    match p {
        TipPlacement::Top => 0,
        TipPlacement::Bottom => 1,
        TipPlacement::Start => 2,
        TipPlacement::End => 3,
    }
}

/// wire u8 → TipPlacement（值域外 → UnknownProp——`kind` 入载荷供拒面定位）。
fn tip_placement_from_u8(kind: ViewKind, v: u8) -> Result<TipPlacement, AstError> {
    match v {
        0 => Ok(TipPlacement::Top),
        1 => Ok(TipPlacement::Bottom),
        2 => Ok(TipPlacement::Start),
        3 => Ok(TipPlacement::End),
        _ => Err(AstError::UnknownProp { kind, got: v }),
    }
}

/// GaugeStyle → wire u8（0/1/2 = Ring/Bar/Arc——编解码双侧单源）。
fn gauge_style_to_u8(s: GaugeStyle) -> u8 {
    match s {
        GaugeStyle::Ring => 0,
        GaugeStyle::Bar => 1,
        GaugeStyle::Arc => 2,
    }
}

/// wire u8 → GaugeStyle（值域外 → UnknownProp——`kind` 入载荷供拒面定位）。
fn gauge_style_from_u8(kind: ViewKind, v: u8) -> Result<GaugeStyle, AstError> {
    match v {
        0 => Ok(GaugeStyle::Ring),
        1 => Ok(GaugeStyle::Bar),
        2 => Ok(GaugeStyle::Arc),
        _ => Err(AstError::UnknownProp { kind, got: v }),
    }
}

/// 组件七件值域先验（builder begin 面——与 parse_props 流面同律，Grid
/// 零维检同型先例；命令面 add() 在 lib.rs，Phase 1 未收编——见模块
/// 注记）。kind×props 失配不在此检（既有非 Grid 面同状——流面
/// parse_props 按 kind 定长读，结构性拒）。
fn validate_phase1(kind: ViewKind, props: &Props) -> Result<(), AstError> {
    match (kind, props) {
        (ViewKind::Slider, Props::Slider(p)) if p.value > PERMILLE_MAX => {
            Err(AstError::BadValue { kind, got: p.value })
        }
        (ViewKind::Slider, Props::Slider(p)) if p.min > p.max => {
            Err(AstError::BadValue { kind, got: p.min })
        }
        (ViewKind::Progress, Props::Progress(p)) if p.value > PERMILLE_MAX => {
            Err(AstError::BadValue { kind, got: p.value })
        }
        (ViewKind::Scroll, Props::Scroll(p)) if p.offset_y > p.content_height => {
            Err(AstError::BadValue { kind, got: p.offset_y })
        }
        (ViewKind::Tabs, Props::Tabs(p)) if p.tab_count == 0 => {
            Err(AstError::BadValue { kind, got: 0 })
        }
        (ViewKind::Tabs, Props::Tabs(p)) if p.active >= p.tab_count => {
            Err(AstError::BadValue { kind, got: p.active })
        }
        _ => Ok(()),
    }
}

/// P1 九件值域先验（builder begin 面——与 parse_props 流面同律，
/// [`validate_phase1`] 同型第二批；检序与流面镜像：Step 零步长 →
/// min>max → value 越界，Seg 零段 → active 越段，Sbar thumb 越千分
/// 域 → start>end）。
fn validate_p1_nine(kind: ViewKind, props: &Props) -> Result<(), AstError> {
    match (kind, props) {
        (ViewKind::Step, Props::Step(p)) if p.step == 0 => {
            Err(AstError::BadValue { kind, got: 0 })
        }
        (ViewKind::Step, Props::Step(p)) if p.min > p.max => {
            Err(AstError::BadValue { kind, got: p.min })
        }
        (ViewKind::Step, Props::Step(p)) if p.value < p.min => {
            Err(AstError::BadValue { kind, got: p.value })
        }
        (ViewKind::Step, Props::Step(p)) if p.value > p.max => {
            Err(AstError::BadValue { kind, got: p.value })
        }
        (ViewKind::Seg, Props::Seg(p)) if p.segment_count == 0 => {
            Err(AstError::BadValue { kind, got: 0 })
        }
        (ViewKind::Seg, Props::Seg(p)) if p.active >= p.segment_count => {
            Err(AstError::BadValue { kind, got: p.active })
        }
        (ViewKind::Sbar, Props::Sbar(p)) if p.thumb_start > PERMILLE_MAX => {
            Err(AstError::BadValue { kind, got: p.thumb_start })
        }
        (ViewKind::Sbar, Props::Sbar(p)) if p.thumb_end > PERMILLE_MAX => {
            Err(AstError::BadValue { kind, got: p.thumb_end })
        }
        (ViewKind::Sbar, Props::Sbar(p)) if p.thumb_start > p.thumb_end => {
            Err(AstError::BadValue { kind, got: p.thumb_start })
        }
        _ => Ok(()),
    }
}

/// P0 尾四件值域先验（builder begin 面——与 parse_props 流面同律，
/// [`validate_phase1`]/[`validate_p1_nine`] 同型第三批）：Select
/// selected 越项（option_count > 0 时——零项合法 = 空选择器，不与
/// Tabs/Seg 零段同律）。Input/Edit 的 placeholder 长度帽在 TextBuf
/// 构造面（值面即拒 TextTooLong）、InputKind/readonly 枚举域在 wire
/// 编解码面——均不入此检。
fn validate_p0_four(kind: ViewKind, props: &Props) -> Result<(), AstError> {
    match (kind, props) {
        (ViewKind::Select, Props::Select(p))
            if p.option_count > 0 && p.selected >= p.option_count =>
        {
            Err(AstError::BadValue { kind, got: p.selected })
        }
        _ => Ok(()),
    }
}

/// P1 尾五件值域先验（builder begin 面——与 parse_props 流面同律，
/// [`validate_phase1`]/[`validate_p1_nine`]/[`validate_p0_four`] 同型
/// 第四批；检序与流面镜像：DatePick month → day → min>max → year 越界）。
/// Tip 文本长度帽在 TextBuf 构造面（值面即拒 TextTooLong）、
/// TipPlacement/refreshing/use_24h 枚举与 bool 子域在 wire 编解码面
/// （域外 UnknownProp）——均不入此检。
fn validate_p1_last_five(kind: ViewKind, props: &Props) -> Result<(), AstError> {
    match (kind, props) {
        (ViewKind::Refresh, Props::Refresh(p)) if p.offset > PERMILLE_MAX => {
            Err(AstError::BadValue { kind, got: p.offset })
        }
        (ViewKind::SwipeItem, Props::SwipeItem(p)) if p.reveal_start > PERMILLE_MAX => {
            Err(AstError::BadValue { kind, got: p.reveal_start })
        }
        (ViewKind::SwipeItem, Props::SwipeItem(p)) if p.reveal_end > PERMILLE_MAX => {
            Err(AstError::BadValue { kind, got: p.reveal_end })
        }
        (ViewKind::DatePick, Props::DatePick(p)) if p.month == 0 || p.month > 12 => {
            Err(AstError::BadValue { kind, got: p.month })
        }
        (ViewKind::DatePick, Props::DatePick(p)) if p.day == 0 || p.day > 31 => {
            Err(AstError::BadValue { kind, got: p.day })
        }
        (ViewKind::DatePick, Props::DatePick(p)) if p.min_year > p.max_year => {
            Err(AstError::BadValue { kind, got: p.min_year })
        }
        (ViewKind::DatePick, Props::DatePick(p))
            if p.year < p.min_year || p.year > p.max_year =>
        {
            Err(AstError::BadValue { kind, got: p.year })
        }
        (ViewKind::TimePick, Props::TimePick(p)) if p.hour > 23 => {
            Err(AstError::BadValue { kind, got: p.hour })
        }
        (ViewKind::TimePick, Props::TimePick(p)) if p.minute > 59 => {
            Err(AstError::BadValue { kind, got: p.minute })
        }
        _ => Ok(()),
    }
}

/// P2 七件值域先验（builder begin 面——与 parse_props 流面同律，
/// [`validate_phase1`]/[`validate_p1_nine`]/[`validate_p0_four`]/
/// [`validate_p1_last_five`] 同型第五批）：Card elevation > 3、
/// Gauge value 越千分域、Qr module_count 不在合法 QR 版本域、
/// DragReorder item_index 越项（total_items > 0 时）。Collapse/Chip/
/// Fab 无 BadValue 面（expanded/selected bool 域与 Chip/Fab 文本长度
/// 帽在 wire 编解码/TextBuf 构造面），不入此检。
fn validate_p2_seven(kind: ViewKind, props: &Props) -> Result<(), AstError> {
    match (kind, props) {
        (ViewKind::Card, Props::Card(p)) if p.elevation > CARD_ELEVATION_MAX => {
            Err(AstError::BadValue { kind, got: p.elevation })
        }
        (ViewKind::Gauge, Props::Gauge(p)) if p.value > PERMILLE_MAX => {
            Err(AstError::BadValue { kind, got: p.value })
        }
        (ViewKind::Qr, Props::Qr(p)) if !qr_module_count_valid(p.module_count) => {
            Err(AstError::BadValue { kind, got: p.module_count })
        }
        (ViewKind::DragReorder, Props::DragReorder(p))
            if p.total_items > 0 && p.item_index >= p.total_items =>
        {
            Err(AstError::BadValue { kind, got: p.item_index })
        }
        _ => Ok(()),
    }
}

// 小写入器（越界 → TooSmall，零 panic）
fn put_u8(out: &mut [u8], v: u8) -> Result<usize, AstError> {
    if out.is_empty() {
        return Err(AstError::TooSmall { need: 1, got: 0 });
    }
    out[0] = v;
    Ok(1)
}

fn put_u32le(out: &mut [u8], v: u32) -> Result<usize, AstError> {
    if out.len() < 4 {
        return Err(AstError::TooSmall { need: 4, got: out.len() });
    }
    out[..4].copy_from_slice(&v.to_le_bytes());
    Ok(4)
}

fn put_u16le(out: &mut [u8], v: u16) -> Result<usize, AstError> {
    if out.len() < 2 {
        return Err(AstError::TooSmall { need: 2, got: out.len() });
    }
    out[..2].copy_from_slice(&v.to_le_bytes());
    Ok(2)
}

fn put_u64le(out: &mut [u8], v: u64) -> Result<usize, AstError> {
    if out.len() < 8 {
        return Err(AstError::TooSmall { need: 8, got: out.len() });
    }
    out[..8].copy_from_slice(&v.to_le_bytes());
    Ok(8)
}

// ---------- props 编解码（§6 冻结签名） ----------

/// props 域编码（kind 定长；Text 变长 6..=9+len）。输出缓冲不足 →
/// `TooSmall`。返回写入字节数。
pub(crate) fn encode_props(props: &Props, out: &mut [u8]) -> Result<usize, AstError> {
    let mut c = 0usize;
    match props {
        Props::Root => {}
        Props::Rect(p) => {
            c += put_u8(&mut out[c..], p.bg.r)?;
            c += put_u8(&mut out[c..], p.bg.g)?;
            c += put_u8(&mut out[c..], p.bg.b)?;
            c += put_u32le(&mut out[c..], p.w.unwrap_or(OPT_NONE))?;
            c += put_u32le(&mut out[c..], p.h.unwrap_or(OPT_NONE))?;
        }
        Props::Text(p) => {
            let bytes = p.text.as_bytes();
            c += put_u8(&mut out[c..], bytes.len() as u8)?;
            if out.len() < c + bytes.len() {
                return Err(AstError::TooSmall { need: c + bytes.len(), got: out.len() });
            }
            out[c..c + bytes.len()].copy_from_slice(bytes);
            c += bytes.len();
            c += put_u8(&mut out[c..], scale_to_u8(p.style.scale))?;
            c += put_u8(&mut out[c..], p.style.fg.r)?;
            c += put_u8(&mut out[c..], p.style.fg.g)?;
            c += put_u8(&mut out[c..], p.style.fg.b)?;
            c += put_u8(&mut out[c..], if p.style.bg.is_some() { 1 } else { 0 })?;
            if let Some(bg) = p.style.bg {
                c += put_u8(&mut out[c..], bg.r)?;
                c += put_u8(&mut out[c..], bg.g)?;
                c += put_u8(&mut out[c..], bg.b)?;
            }
        }
        Props::Col(p) => {
            c += put_u32le(&mut out[c..], p.gap)?;
        }
        Props::Row(p) => {
            c += put_u32le(&mut out[c..], p.gap)?;
        }
        Props::Pad(p) => {
            c += put_u32le(&mut out[c..], p.all)?;
        }
        // Image 10B：res u16 + w/h 哨兵对（定长域内——永不吃 HAS_SIZE 块）
        Props::Image(p) => {
            c += put_u16le(&mut out[c..], p.res)?;
            c += put_u32le(&mut out[c..], p.w.unwrap_or(SIZE_NONE))?;
            c += put_u32le(&mut out[c..], p.h.unwrap_or(SIZE_NONE))?;
        }
        // Icon 13B：id u16 + fg 3B + w/h 哨兵对
        Props::Icon(p) => {
            c += put_u16le(&mut out[c..], p.id)?;
            c += put_u8(&mut out[c..], p.fg.r)?;
            c += put_u8(&mut out[c..], p.fg.g)?;
            c += put_u8(&mut out[c..], p.fg.b)?;
            c += put_u32le(&mut out[c..], p.w.unwrap_or(SIZE_NONE))?;
            c += put_u32le(&mut out[c..], p.h.unwrap_or(SIZE_NONE))?;
        }
        // Grid 7B：cols/rows/gap/cross（cross 在 props 块内——Grid 无
        // ALIGN 块；w/h 由 emit 侧 HAS_SIZE 块另载，不在此域）
        Props::Grid(p) => {
            c += put_u8(&mut out[c..], p.cols)?;
            c += put_u8(&mut out[c..], p.rows)?;
            c += put_u32le(&mut out[c..], p.gap)?;
            c += put_u8(&mut out[c..], cross_to_u8(p.cross))?;
        }
        // Button 变长 6..=70：len u8 + label + style u8 + on_tap u32 LE
        Props::Button(p) => {
            let bytes = p.label.as_bytes();
            c += put_u8(&mut out[c..], bytes.len() as u8)?;
            if out.len() < c + bytes.len() {
                return Err(AstError::TooSmall { need: c + bytes.len(), got: out.len() });
            }
            out[c..c + bytes.len()].copy_from_slice(bytes);
            c += bytes.len();
            c += put_u8(&mut out[c..], button_style_to_u8(p.style))?;
            c += put_u32le(&mut out[c..], p.on_tap)?;
        }
        // Toggle 5B：checked u8 + on_change u32 LE
        Props::Toggle(p) => {
            c += put_u8(&mut out[c..], p.checked as u8)?;
            c += put_u32le(&mut out[c..], p.on_change)?;
        }
        // Slider 16B：value·min·max·on_change 各 u32 LE
        Props::Slider(p) => {
            c += put_u32le(&mut out[c..], p.value)?;
            c += put_u32le(&mut out[c..], p.min)?;
            c += put_u32le(&mut out[c..], p.max)?;
            c += put_u32le(&mut out[c..], p.on_change)?;
        }
        // Progress 5B：value u32 LE + style u8
        Props::Progress(p) => {
            c += put_u32le(&mut out[c..], p.value)?;
            c += put_u8(&mut out[c..], progress_style_to_u8(p.style))?;
        }
        // Scroll 8B：content_height·offset_y 各 u32 LE
        Props::Scroll(p) => {
            c += put_u32le(&mut out[c..], p.content_height)?;
            c += put_u32le(&mut out[c..], p.offset_y)?;
        }
        // List 8B：item_count·item_height 各 u32 LE
        Props::List(p) => {
            c += put_u32le(&mut out[c..], p.item_count)?;
            c += put_u32le(&mut out[c..], p.item_height)?;
        }
        // Tabs 12B：tab_count·active·on_switch 各 u32 LE
        Props::Tabs(p) => {
            c += put_u32le(&mut out[c..], p.tab_count)?;
            c += put_u32le(&mut out[c..], p.active)?;
            c += put_u32le(&mut out[c..], p.on_switch)?;
        }
        // Stack 1B：align u8
        Props::Stack(p) => {
            c += put_u8(&mut out[c..], stack_align_to_u8(p.align))?;
        }
        // Spacer 4B：flex u32 LE
        Props::Spacer(p) => {
            c += put_u32le(&mut out[c..], p.flex)?;
        }
        // Divider 12B：thickness·inset_start·inset_end 各 u32 LE
        Props::Divider(p) => {
            c += put_u32le(&mut out[c..], p.thickness)?;
            c += put_u32le(&mut out[c..], p.inset_start)?;
            c += put_u32le(&mut out[c..], p.inset_end)?;
        }
        // Check 5B：checked u8 + on_change u32 LE
        Props::Check(p) => {
            c += put_u8(&mut out[c..], p.checked as u8)?;
            c += put_u32le(&mut out[c..], p.on_change)?;
        }
        // Radio 9B：selected u8 + group·on_change 各 u32 LE
        Props::Radio(p) => {
            c += put_u8(&mut out[c..], p.selected as u8)?;
            c += put_u32le(&mut out[c..], p.group)?;
            c += put_u32le(&mut out[c..], p.on_change)?;
        }
        // Step 20B：value·min·max·step·on_change 各 u32 LE
        Props::Step(p) => {
            c += put_u32le(&mut out[c..], p.value)?;
            c += put_u32le(&mut out[c..], p.min)?;
            c += put_u32le(&mut out[c..], p.max)?;
            c += put_u32le(&mut out[c..], p.step)?;
            c += put_u32le(&mut out[c..], p.on_change)?;
        }
        // Seg 12B：segment_count·active·on_change 各 u32 LE
        Props::Seg(p) => {
            c += put_u32le(&mut out[c..], p.segment_count)?;
            c += put_u32le(&mut out[c..], p.active)?;
            c += put_u32le(&mut out[c..], p.on_change)?;
        }
        // Badge 5B：count u32 LE + show_dot u8
        Props::Badge(p) => {
            c += put_u32le(&mut out[c..], p.count)?;
            c += put_u8(&mut out[c..], p.show_dot as u8)?;
        }
        // Sbar 9B：orientation u8 + thumb_start·thumb_end 各 u32 LE
        Props::Sbar(p) => {
            c += put_u8(&mut out[c..], orientation_to_u8(p.orientation))?;
            c += put_u32le(&mut out[c..], p.thumb_start)?;
            c += put_u32le(&mut out[c..], p.thumb_end)?;
        }
        // Select 12B：option_count·selected·on_change 各 u32 LE
        Props::Select(p) => {
            c += put_u32le(&mut out[c..], p.option_count)?;
            c += put_u32le(&mut out[c..], p.selected)?;
            c += put_u32le(&mut out[c..], p.on_change)?;
        }
        // Input 变长 6..=70：len u8 + placeholder + max_len u32 LE +
        // input_kind u8（内容不入流——输入焦点协议 Phase 2）
        Props::Input(p) => {
            let bytes = p.placeholder.as_bytes();
            c += put_u8(&mut out[c..], bytes.len() as u8)?;
            if out.len() < c + bytes.len() {
                return Err(AstError::TooSmall { need: c + bytes.len(), got: out.len() });
            }
            out[c..c + bytes.len()].copy_from_slice(bytes);
            c += bytes.len();
            c += put_u32le(&mut out[c..], p.max_len)?;
            c += put_u8(&mut out[c..], input_kind_to_u8(p.input_kind))?;
        }
        // Edit 变长 10..=74：len u8 + placeholder + max_len·max_lines
        // 各 u32 LE + readonly u8
        Props::Edit(p) => {
            let bytes = p.placeholder.as_bytes();
            c += put_u8(&mut out[c..], bytes.len() as u8)?;
            if out.len() < c + bytes.len() {
                return Err(AstError::TooSmall { need: c + bytes.len(), got: out.len() });
            }
            out[c..c + bytes.len()].copy_from_slice(bytes);
            c += bytes.len();
            c += put_u32le(&mut out[c..], p.max_len)?;
            c += put_u32le(&mut out[c..], p.max_lines)?;
            c += put_u8(&mut out[c..], p.readonly as u8)?;
        }
        // Canvas 4B：paint_ops u32 LE（数据层只记数——op 流走绘制协议）
        Props::Canvas(p) => {
            c += put_u32le(&mut out[c..], p.paint_ops)?;
        }
        // Tip 变长 2..=66：len u8 + text + placement u8（Button label
        // 同款内联文本律）
        Props::Tip(p) => {
            let bytes = p.text.as_bytes();
            c += put_u8(&mut out[c..], bytes.len() as u8)?;
            if out.len() < c + bytes.len() {
                return Err(AstError::TooSmall { need: c + bytes.len(), got: out.len() });
            }
            out[c..c + bytes.len()].copy_from_slice(bytes);
            c += bytes.len();
            c += put_u8(&mut out[c..], tip_placement_to_u8(p.placement))?;
        }
        // Refresh 9B：refreshing u8 + offset·on_refresh 各 u32 LE
        Props::Refresh(p) => {
            c += put_u8(&mut out[c..], p.refreshing as u8)?;
            c += put_u32le(&mut out[c..], p.offset)?;
            c += put_u32le(&mut out[c..], p.on_refresh)?;
        }
        // SwipeItem 16B：action_count·reveal_start·reveal_end·on_action
        // 各 u32 LE
        Props::SwipeItem(p) => {
            c += put_u32le(&mut out[c..], p.action_count)?;
            c += put_u32le(&mut out[c..], p.reveal_start)?;
            c += put_u32le(&mut out[c..], p.reveal_end)?;
            c += put_u32le(&mut out[c..], p.on_action)?;
        }
        // DatePick 24B：year·month·day·min_year·max_year·on_change
        // 各 u32 LE
        Props::DatePick(p) => {
            c += put_u32le(&mut out[c..], p.year)?;
            c += put_u32le(&mut out[c..], p.month)?;
            c += put_u32le(&mut out[c..], p.day)?;
            c += put_u32le(&mut out[c..], p.min_year)?;
            c += put_u32le(&mut out[c..], p.max_year)?;
            c += put_u32le(&mut out[c..], p.on_change)?;
        }
        // TimePick 13B：hour·minute·on_change 各 u32 LE + use_24h u8
        Props::TimePick(p) => {
            c += put_u32le(&mut out[c..], p.hour)?;
            c += put_u32le(&mut out[c..], p.minute)?;
            c += put_u32le(&mut out[c..], p.on_change)?;
            c += put_u8(&mut out[c..], p.use_24h as u8)?;
        }
        // Collapse 5B：expanded u8 + on_toggle u32 LE
        Props::Collapse(p) => {
            c += put_u8(&mut out[c..], p.expanded as u8)?;
            c += put_u32le(&mut out[c..], p.on_toggle)?;
        }
        // Chip 变长 6..=70：len u8 + label + selected u8 + on_tap u32 LE
        Props::Chip(p) => {
            let bytes = p.label.as_bytes();
            c += put_u8(&mut out[c..], bytes.len() as u8)?;
            if out.len() < c + bytes.len() {
                return Err(AstError::TooSmall { need: c + bytes.len(), got: out.len() });
            }
            out[c..c + bytes.len()].copy_from_slice(bytes);
            c += bytes.len();
            c += put_u8(&mut out[c..], p.selected as u8)?;
            c += put_u32le(&mut out[c..], p.on_tap)?;
        }
        // Fab 变长 9..=73：icon u32 LE + len u8 + extended_label +
        // on_tap u32 LE（空标签 = 纯圆形）
        Props::Fab(p) => {
            c += put_u32le(&mut out[c..], p.icon)?;
            let bytes = p.extended_label.as_bytes();
            c += put_u8(&mut out[c..], bytes.len() as u8)?;
            if out.len() < c + bytes.len() {
                return Err(AstError::TooSmall { need: c + bytes.len(), got: out.len() });
            }
            out[c..c + bytes.len()].copy_from_slice(bytes);
            c += bytes.len();
            c += put_u32le(&mut out[c..], p.on_tap)?;
        }
        // Card 8B：elevation·corner_radius 各 u32 LE
        Props::Card(p) => {
            c += put_u32le(&mut out[c..], p.elevation)?;
            c += put_u32le(&mut out[c..], p.corner_radius)?;
        }
        // Gauge 5B：value u32 LE + style u8（Progress 同形）
        Props::Gauge(p) => {
            c += put_u32le(&mut out[c..], p.value)?;
            c += put_u8(&mut out[c..], gauge_style_to_u8(p.style))?;
        }
        // Qr 12B：module_count·dark_color·light_color 各 u32 LE（点阵
        // 生成挂 Phase 2——数据层只记参数）
        Props::Qr(p) => {
            c += put_u32le(&mut out[c..], p.module_count)?;
            c += put_u32le(&mut out[c..], p.dark_color)?;
            c += put_u32le(&mut out[c..], p.light_color)?;
        }
        // DragReorder 12B：item_index·total_items·on_reorder 各 u32 LE
        Props::DragReorder(p) => {
            c += put_u32le(&mut out[c..], p.item_index)?;
            c += put_u32le(&mut out[c..], p.total_items)?;
            c += put_u32le(&mut out[c..], p.on_reorder)?;
        }
    }
    Ok(c)
}

/// props 域解码（`src` 起点 = props 首字节）。截断 → `Truncated`。
/// 返回（属性, 消费字节数）。
fn parse_props(kind: ViewKind, src: &[u8]) -> Result<(Props, usize), AstError> {
    let need = |n: usize| -> Result<(), AstError> {
        if src.len() < n {
            Err(AstError::Truncated)
        } else {
            Ok(())
        }
    };
    let u32at = |i: usize| -> u32 {
        u32::from_le_bytes([src[i], src[i + 1], src[i + 2], src[i + 3]])
    };
    match kind {
        ViewKind::Root => Ok((Props::Root, 0)),
        ViewKind::Rect => {
            need(11)?;
            let w = u32at(3);
            let h = u32at(7);
            Ok((
                Props::Rect(RectProps {
                    bg: Color { r: src[0], g: src[1], b: src[2] },
                    w: if w == OPT_NONE { None } else { Some(w) },
                    h: if h == OPT_NONE { None } else { Some(h) },
                    radius: 0,
                }),
                11,
            ))
        }
        ViewKind::Text => {
            need(1)?;
            let tlen = src[0] as usize;
            if tlen > crate::TEXT_INLINE {
                return Err(AstError::TextTooLong { len: tlen });
            }
            // len(1) + 文本 + scale(1) + fg(3) + bg tag(1) [+ bg(3)]
            need(1 + tlen + 5)?;
            let scale = scale_from_u8(kind, src[1 + tlen])?;
            let fg = Color { r: src[1 + tlen + 1], g: src[1 + tlen + 2], b: src[1 + tlen + 3] };
            let tag = src[1 + tlen + 4];
            let bg = match tag {
                0 => None,
                1 => {
                    need(1 + tlen + 8)?;
                    Some(Color {
                        r: src[1 + tlen + 5],
                        g: src[1 + tlen + 6],
                        b: src[1 + tlen + 7],
                    })
                }
                _ => return Err(AstError::UnknownProp { kind, got: tag }),
            };
            let text = TextBuf::new(&src[1..1 + tlen])
                .map_err(|_| AstError::TextTooLong { len: tlen })?;
            let used = 1 + tlen + 5 + if bg.is_some() { 3 } else { 0 };
            Ok((Props::Text(TextProps { style: TextStyle { fg, bg, scale }, text }), used))
        }
        ViewKind::Col => {
            need(4)?;
            Ok((
                Props::Col(ColProps {
                    gap: u32at(0),
                    w: None,
                    h: None,
                    cross: crate::layout::CrossAlign::Stretch,
                }),
                4,
            ))
        }
        ViewKind::Row => {
            need(4)?;
            Ok((
                Props::Row(RowProps {
                    gap: u32at(0),
                    w: None,
                    h: None,
                    cross: crate::layout::CrossAlign::Stretch,
                }),
                4,
            ))
        }
        ViewKind::Pad => {
            need(4)?;
            Ok((Props::Pad(PadProps { all: u32at(0) }), 4))
        }
        // Image 10B（先长检后语义检——截断与零维/值域拒可分钉）
        ViewKind::Image => {
            need(10)?;
            let w = u32at(2);
            let h = u32at(6);
            Ok((
                Props::Image(ImageProps {
                    res: u16::from_le_bytes([src[0], src[1]]),
                    w: if w == SIZE_NONE { None } else { Some(w) },
                    h: if h == SIZE_NONE { None } else { Some(h) },
                }),
                10,
            ))
        }
        // Icon 13B
        ViewKind::Icon => {
            need(13)?;
            let w = u32at(5);
            let h = u32at(9);
            Ok((
                Props::Icon(IconProps {
                    id: u16::from_le_bytes([src[0], src[1]]),
                    fg: Color { r: src[2], g: src[3], b: src[4] },
                    w: if w == SIZE_NONE { None } else { Some(w) },
                    h: if h == SIZE_NONE { None } else { Some(h) },
                }),
                13,
            ))
        }
        // Grid 7B：零维拒（GridZeroDims——定长域读完再语义检，截断优先）
        ViewKind::Grid => {
            need(7)?;
            let (cols, rows) = (src[0], src[1]);
            if cols == 0 || rows == 0 {
                return Err(AstError::GridZeroDims);
            }
            Ok((
                Props::Grid(GridProps {
                    cols,
                    rows,
                    gap: u32at(2),
                    cross: cross_from_u8(kind, src[6])?,
                    w: None,
                    h: None,
                }),
                7,
            ))
        }
        // Button 变长 6..=70：len(1) + label + style(1) + on_tap(4)——
        // 超长 TextTooLong、style 域外 UnknownProp（截断优先于语义检）
        ViewKind::Button => {
            need(1)?;
            let llen = src[0] as usize;
            if llen > crate::TEXT_INLINE {
                return Err(AstError::TextTooLong { len: llen });
            }
            need(6 + llen)?;
            let style = button_style_from_u8(kind, src[1 + llen])?;
            let label = TextBuf::new(&src[1..1 + llen])
                .map_err(|_| AstError::TextTooLong { len: llen })?;
            Ok((
                Props::Button(ButtonProps { label, style, on_tap: u32at(2 + llen) }),
                6 + llen,
            ))
        }
        // Toggle 5B：checked ∈ {0,1}（域外 UnknownProp）+ on_change
        ViewKind::Toggle => {
            need(5)?;
            let checked = match src[0] {
                0 => false,
                1 => true,
                v => return Err(AstError::UnknownProp { kind, got: v }),
            };
            Ok((Props::Toggle(ToggleProps { checked, on_change: u32at(1) }), 5))
        }
        // Slider 16B：值域检在定长域读完后（截断优先）；value > 1000 /
        // min > max → BadValue（got = 违规值）
        ViewKind::Slider => {
            need(16)?;
            let (value, min, max, on_change) = (u32at(0), u32at(4), u32at(8), u32at(12));
            if value > PERMILLE_MAX {
                return Err(AstError::BadValue { kind, got: value });
            }
            if min > max {
                return Err(AstError::BadValue { kind, got: min });
            }
            Ok((Props::Slider(SliderProps { value, min, max, on_change }), 16))
        }
        // Progress 5B：value 值域 + style 域外拒
        ViewKind::Progress => {
            need(5)?;
            let value = u32at(0);
            if value > PERMILLE_MAX {
                return Err(AstError::BadValue { kind, got: value });
            }
            let style = progress_style_from_u8(kind, src[4])?;
            Ok((Props::Progress(ProgressProps { value, style }), 5))
        }
        // Scroll 8B：offset_y > content_height → BadValue（视口无关
        // 结构不变量——对视口紧上界由消费面钳）
        ViewKind::Scroll => {
            need(8)?;
            let (content_height, offset_y) = (u32at(0), u32at(4));
            if offset_y > content_height {
                return Err(AstError::BadValue { kind, got: offset_y });
            }
            Ok((Props::Scroll(ScrollProps { content_height, offset_y }), 8))
        }
        // List 8B：等高项计数 + 项高（v1 全量——零项合法 = 空表）
        ViewKind::List => {
            need(8)?;
            Ok((Props::List(ListProps { item_count: u32at(0), item_height: u32at(4) }), 8))
        }
        // Tabs 12B：零页 / active 越页 → BadValue（got = 违规值）
        ViewKind::Tabs => {
            need(12)?;
            let (tab_count, active, on_switch) = (u32at(0), u32at(4), u32at(8));
            if tab_count == 0 {
                return Err(AstError::BadValue { kind, got: 0 });
            }
            if active >= tab_count {
                return Err(AstError::BadValue { kind, got: active });
            }
            Ok((Props::Tabs(TabsProps { tab_count, active, on_switch }), 12))
        }
        // Stack 1B：align 域外（>2）→ UnknownProp（截断优先——need(1) 先）
        ViewKind::Stack => {
            need(1)?;
            let align = stack_align_from_u8(kind, src[0])?;
            Ok((Props::Stack(StackProps { align }), 1))
        }
        // Spacer 4B：flex u32 LE（零权重合法——占位面无值域）
        ViewKind::Spacer => {
            need(4)?;
            Ok((Props::Spacer(SpacerProps { flex: u32at(0) }), 4))
        }
        // Divider 12B：thickness·inset_start·inset_end 各 u32 LE（零厚
        // 合法——几何退化留视觉面）
        ViewKind::Divider => {
            need(12)?;
            Ok((
                Props::Divider(DividerProps {
                    thickness: u32at(0),
                    inset_start: u32at(4),
                    inset_end: u32at(8),
                }),
                12,
            ))
        }
        // Check 5B：checked ∈ {0,1}（域外 UnknownProp）+ on_change
        ViewKind::Check => {
            need(5)?;
            let checked = match src[0] {
                0 => false,
                1 => true,
                v => return Err(AstError::UnknownProp { kind, got: v }),
            };
            Ok((Props::Check(CheckProps { checked, on_change: u32at(1) }), 5))
        }
        // Radio 9B：selected ∈ {0,1} + group·on_change（互斥执法在
        // 接线面——数据层只载组号）
        ViewKind::Radio => {
            need(9)?;
            let selected = match src[0] {
                0 => false,
                1 => true,
                v => return Err(AstError::UnknownProp { kind, got: v }),
            };
            Ok((
                Props::Radio(RadioProps { selected, group: u32at(1), on_change: u32at(5) }),
                9,
            ))
        }
        // Step 20B：值域检在定长域读完后（截断优先）；检序与
        // validate_p1_nine 镜像——零步长 → min>max → value 越界
        ViewKind::Step => {
            need(20)?;
            let (value, min, max, step, on_change) =
                (u32at(0), u32at(4), u32at(8), u32at(12), u32at(16));
            if step == 0 {
                return Err(AstError::BadValue { kind, got: 0 });
            }
            if min > max {
                return Err(AstError::BadValue { kind, got: min });
            }
            if value < min {
                return Err(AstError::BadValue { kind, got: value });
            }
            if value > max {
                return Err(AstError::BadValue { kind, got: value });
            }
            Ok((Props::Step(StepProps { value, min, max, step, on_change }), 20))
        }
        // Seg 12B：零段 / active 越段 → BadValue（Tabs 同律）
        ViewKind::Seg => {
            need(12)?;
            let (segment_count, active, on_change) = (u32at(0), u32at(4), u32at(8));
            if segment_count == 0 {
                return Err(AstError::BadValue { kind, got: 0 });
            }
            if active >= segment_count {
                return Err(AstError::BadValue { kind, got: active });
            }
            Ok((Props::Seg(SegProps { segment_count, active, on_change }), 12))
        }
        // Badge 5B：count u32 LE + show_dot ∈ {0,1}（99+ 封顶显示是
        // 视觉面语义——数据层载原值）
        ViewKind::Badge => {
            need(5)?;
            let show_dot = match src[4] {
                0 => false,
                1 => true,
                v => return Err(AstError::UnknownProp { kind, got: v }),
            };
            Ok((Props::Badge(BadgeProps { count: u32at(0), show_dot }), 5))
        }
        // Sbar 9B：orientation ∈ {0,1}；thumb 两端各 0..=1000 permille
        //（截断优先 → 枚举域检 → 值域检，检序与 validate_p1_nine 镜像）
        ViewKind::Sbar => {
            need(9)?;
            let orientation = orientation_from_u8(kind, src[0])?;
            let (thumb_start, thumb_end) = (u32at(1), u32at(5));
            if thumb_start > PERMILLE_MAX {
                return Err(AstError::BadValue { kind, got: thumb_start });
            }
            if thumb_end > PERMILLE_MAX {
                return Err(AstError::BadValue { kind, got: thumb_end });
            }
            if thumb_start > thumb_end {
                return Err(AstError::BadValue { kind, got: thumb_start });
            }
            Ok((
                Props::Sbar(SbarProps { orientation, thumb_start, thumb_end }),
                9,
            ))
        }
        // Select 12B：selected 越项 → BadValue（got = selected；零项
        // 合法 = 空选择器——Tabs/Seg 零段拒不同律）
        ViewKind::Select => {
            need(12)?;
            let (option_count, selected, on_change) = (u32at(0), u32at(4), u32at(8));
            if option_count > 0 && selected >= option_count {
                return Err(AstError::BadValue { kind, got: selected });
            }
            Ok((
                Props::Select(SelectProps { option_count, selected, on_change }),
                12,
            ))
        }
        // Input 变长 6..=70：len(1) + placeholder + max_len(4) +
        // input_kind(1)——超长 TextTooLong、input_kind 域外 UnknownProp
        // （截断优先于语义检，Button 同律）
        ViewKind::Input => {
            need(1)?;
            let plen = src[0] as usize;
            if plen > crate::TEXT_INLINE {
                return Err(AstError::TextTooLong { len: plen });
            }
            need(6 + plen)?;
            let input_kind = input_kind_from_u8(kind, src[5 + plen])?;
            let placeholder = TextBuf::new(&src[1..1 + plen])
                .map_err(|_| AstError::TextTooLong { len: plen })?;
            Ok((
                Props::Input(InputProps { placeholder, max_len: u32at(1 + plen), input_kind }),
                6 + plen,
            ))
        }
        // Edit 变长 10..=74：len(1) + placeholder + max_len(4) +
        // max_lines(4) + readonly(1)——readonly ∈ {0,1}（域外
        // UnknownProp）；max_lines = 0 合法（无限制）
        ViewKind::Edit => {
            need(1)?;
            let plen = src[0] as usize;
            if plen > crate::TEXT_INLINE {
                return Err(AstError::TextTooLong { len: plen });
            }
            need(10 + plen)?;
            let readonly = match src[9 + plen] {
                0 => false,
                1 => true,
                v => return Err(AstError::UnknownProp { kind, got: v }),
            };
            let placeholder = TextBuf::new(&src[1..1 + plen])
                .map_err(|_| AstError::TextTooLong { len: plen })?;
            Ok((
                Props::Edit(EditProps {
                    placeholder,
                    max_len: u32at(1 + plen),
                    max_lines: u32at(1 + plen + 4),
                    readonly,
                }),
                10 + plen,
            ))
        }
        // Canvas 4B：paint_ops u32 LE（op 流走绘制协议——数据层只记数）
        ViewKind::Canvas => {
            need(4)?;
            Ok((Props::Canvas(CanvasProps { paint_ops: u32at(0) }), 4))
        }
        // Tip 变长 2..=66：len(1) + text + placement(1)——超长
        // TextTooLong、placement 域外 UnknownProp（截断优先于语义检，
        // Button 同律）
        ViewKind::Tip => {
            need(1)?;
            let tlen = src[0] as usize;
            if tlen > crate::TEXT_INLINE {
                return Err(AstError::TextTooLong { len: tlen });
            }
            need(2 + tlen)?;
            let placement = tip_placement_from_u8(kind, src[1 + tlen])?;
            let text = TextBuf::new(&src[1..1 + tlen])
                .map_err(|_| AstError::TextTooLong { len: tlen })?;
            Ok((Props::Tip(TipProps { text, placement }), 2 + tlen))
        }
        // Refresh 9B：refreshing ∈ {0,1}（域外 UnknownProp）+
        // offset·on_refresh；offset 越千分域 → BadValue（截断优先——
        // need(9) 先，检序与 validate_p1_last_five 镜像）
        ViewKind::Refresh => {
            need(9)?;
            let refreshing = match src[0] {
                0 => false,
                1 => true,
                v => return Err(AstError::UnknownProp { kind, got: v }),
            };
            let (offset, on_refresh) = (u32at(1), u32at(5));
            if offset > PERMILLE_MAX {
                return Err(AstError::BadValue { kind, got: offset });
            }
            Ok((
                Props::Refresh(RefreshProps { refreshing, offset, on_refresh }),
                9,
            ))
        }
        // SwipeItem 16B：action_count·reveal_start·reveal_end·on_action
        // 各 u32 LE；reveal 任一端越千分域 → BadValue（两端独立无序——
        // 与 Sbar thumb 的 start≤end 区间律不同源）
        ViewKind::SwipeItem => {
            need(16)?;
            let (action_count, reveal_start, reveal_end, on_action) =
                (u32at(0), u32at(4), u32at(8), u32at(12));
            if reveal_start > PERMILLE_MAX {
                return Err(AstError::BadValue { kind, got: reveal_start });
            }
            if reveal_end > PERMILLE_MAX {
                return Err(AstError::BadValue { kind, got: reveal_end });
            }
            Ok((
                Props::SwipeItem(SwipeItemProps {
                    action_count,
                    reveal_start,
                    reveal_end,
                    on_action,
                }),
                16,
            ))
        }
        // DatePick 24B：结构域检在定长域读完后（截断优先）；检序与
        // validate_p1_last_five 镜像——month 越域 → day 越域 → min>max →
        // year 越界（day 不做月长感知——2 月 31 结构合法消费面钳）
        ViewKind::DatePick => {
            need(24)?;
            let (year, month, day, min_year, max_year, on_change) =
                (u32at(0), u32at(4), u32at(8), u32at(12), u32at(16), u32at(20));
            if month == 0 || month > 12 {
                return Err(AstError::BadValue { kind, got: month });
            }
            if day == 0 || day > 31 {
                return Err(AstError::BadValue { kind, got: day });
            }
            if min_year > max_year {
                return Err(AstError::BadValue { kind, got: min_year });
            }
            if year < min_year || year > max_year {
                return Err(AstError::BadValue { kind, got: year });
            }
            Ok((
                Props::DatePick(DatePickProps {
                    year,
                    month,
                    day,
                    min_year,
                    max_year,
                    on_change,
                }),
                24,
            ))
        }
        // TimePick 13B：hour·minute·on_change 各 u32 LE + use_24h ∈ {0,1}
        //（域外 UnknownProp）；hour > 23 / minute > 59 → BadValue（检序
        // 与 validate_p1_last_five 镜像）
        ViewKind::TimePick => {
            need(13)?;
            let (hour, minute, on_change) = (u32at(0), u32at(4), u32at(8));
            if hour > 23 {
                return Err(AstError::BadValue { kind, got: hour });
            }
            if minute > 59 {
                return Err(AstError::BadValue { kind, got: minute });
            }
            let use_24h = match src[12] {
                0 => false,
                1 => true,
                v => return Err(AstError::UnknownProp { kind, got: v }),
            };
            Ok((
                Props::TimePick(TimePickProps { hour, minute, use_24h, on_change }),
                13,
            ))
        }
        // Collapse 5B：expanded ∈ {0,1}（域外 UnknownProp）+ on_toggle
        ViewKind::Collapse => {
            need(5)?;
            let expanded = match src[0] {
                0 => false,
                1 => true,
                v => return Err(AstError::UnknownProp { kind, got: v }),
            };
            Ok((Props::Collapse(CollapseProps { expanded, on_toggle: u32at(1) }), 5))
        }
        // Chip 变长 6..=70：len(1) + label + selected(1) + on_tap(4)——
        // 超长 TextTooLong、selected 域外 UnknownProp（截断优先于语义检，
        // Button 同律）
        ViewKind::Chip => {
            need(1)?;
            let llen = src[0] as usize;
            if llen > crate::TEXT_INLINE {
                return Err(AstError::TextTooLong { len: llen });
            }
            need(6 + llen)?;
            let selected = match src[1 + llen] {
                0 => false,
                1 => true,
                v => return Err(AstError::UnknownProp { kind, got: v }),
            };
            let label = TextBuf::new(&src[1..1 + llen])
                .map_err(|_| AstError::TextTooLong { len: llen })?;
            Ok((
                Props::Chip(ChipProps { label, selected, on_tap: u32at(2 + llen) }),
                6 + llen,
            ))
        }
        // Fab 变长 9..=73：icon(4) + len(1) + extended_label + on_tap(4)
        // ——超长 TextTooLong（截断优先于语义检，Button 同律；空标签 =
        // 纯圆形合法）
        ViewKind::Fab => {
            need(5)?; // icon(4) + len(1) 先读 len 域
            let llen = src[4] as usize;
            if llen > crate::TEXT_INLINE {
                return Err(AstError::TextTooLong { len: llen });
            }
            need(9 + llen)?;
            let extended_label = TextBuf::new(&src[5..5 + llen])
                .map_err(|_| AstError::TextTooLong { len: llen })?;
            Ok((
                Props::Fab(FabProps { icon: u32at(0), extended_label, on_tap: u32at(5 + llen) }),
                9 + llen,
            ))
        }
        // Card 8B：elevation 值域检在定长域读完后（截断优先）；
        // elevation > 3 → BadValue（got = elevation）
        ViewKind::Card => {
            need(8)?;
            let (elevation, corner_radius) = (u32at(0), u32at(4));
            if elevation > CARD_ELEVATION_MAX {
                return Err(AstError::BadValue { kind, got: elevation });
            }
            Ok((Props::Card(CardProps { elevation, corner_radius }), 8))
        }
        // Gauge 5B：value 值域 + style 域外拒（Progress 同律）
        ViewKind::Gauge => {
            need(5)?;
            let value = u32at(0);
            if value > PERMILLE_MAX {
                return Err(AstError::BadValue { kind, got: value });
            }
            let style = gauge_style_from_u8(kind, src[4])?;
            Ok((Props::Gauge(GaugeProps { value, style }), 5))
        }
        // Qr 12B：module_count 版本域检在定长域读完后（截断优先）；
        // 不在 {21,25,29,...,177} → BadValue（got = module_count）
        ViewKind::Qr => {
            need(12)?;
            let module_count = u32at(0);
            if !qr_module_count_valid(module_count) {
                return Err(AstError::BadValue { kind, got: module_count });
            }
            Ok((
                Props::Qr(QrProps { module_count, dark_color: u32at(4), light_color: u32at(8) }),
                12,
            ))
        }
        // DragReorder 12B：item_index 越项 → BadValue（got = item_index；
        // total_items == 0 合法 = 空表——Select 零项同律）
        ViewKind::DragReorder => {
            need(12)?;
            let (item_index, total_items, on_reorder) = (u32at(0), u32at(4), u32at(8));
            if total_items > 0 && item_index >= total_items {
                return Err(AstError::BadValue { kind, got: item_index });
            }
            Ok((
                Props::DragReorder(DragReorderProps { item_index, total_items, on_reorder }),
                12,
            ))
        }
    }
}

/// 节点本体解析（§6 冻结签名 + GUI-6 §8 扩展块）：`kind u8 | flags u8 |
/// key u64 LE? | props | size 块? | a11y 块? | align 块?`。**不含
/// child_count**（树形走查域——由 apply_tree/DescBuilder 读写）。
/// 返回（节点描述, 消费字节数, a11y 块载荷——a11y 非节点字段（D12 落
/// Ui 侧表），经出参移交树走查方）。拒面：截断 / 未知 kind / flags
/// bit4+ / props 域畸形 / 块×kind 失配（§8.2 表）/ 非规范形块载荷。
pub(crate) fn parse_node(
    src: &[u8],
) -> Result<(ViewSpec, usize, Option<(u8, TextBuf)>), AstError> {
    if src.len() < 2 {
        return Err(AstError::Truncated);
    }
    let kind = kind_from_u8(src[0]).ok_or(AstError::UnknownKind { got: src[0] })?;
    let flags = src[1];
    if flags & !FLAG_KNOWN != 0 {
        return Err(AstError::UnknownFlags { got: flags });
    }
    let mut pos = 2usize;
    let key = if flags & FLAG_HAS_KEY != 0 {
        if src.len() < pos + 8 {
            return Err(AstError::Truncated);
        }
        let k = u64::from_le_bytes([
            src[pos], src[pos + 1], src[pos + 2], src[pos + 3],
            src[pos + 4], src[pos + 5], src[pos + 6], src[pos + 7],
        ]);
        pos += 8;
        k
    } else {
        0
    };
    let (mut props, plen) = parse_props(kind, &src[pos..])?;
    pos += plen;
    // 扩展块序恒 SIZE→A11Y→ALIGN（§8 D5——三面同序，块序即流序）
    if flags & FLAG_HAS_SIZE != 0 {
        props = apply_size_block(kind, props, &src[pos..], &mut pos)?;
    }
    let mut a11y = None;
    if flags & FLAG_HAS_A11Y != 0 {
        // a11y 块 = role u8 + len u8 + label（≤ TEXT_INLINE——与内联文本同帽）
        if src.len() < pos + 2 {
            return Err(AstError::Truncated);
        }
        let role = src[pos];
        let llen = src[pos + 1] as usize;
        if llen > crate::TEXT_INLINE {
            return Err(AstError::TextTooLong { len: llen });
        }
        if src.len() < pos + 2 + llen {
            return Err(AstError::Truncated);
        }
        let label = TextBuf::new(&src[pos + 2..pos + 2 + llen])
            .map_err(|_| AstError::TextTooLong { len: llen })?;
        a11y = Some((role, label));
        pos += 2 + llen;
    }
    if flags & FLAG_HAS_ALIGN != 0 {
        props = apply_align_block(kind, props, &src[pos..], &mut pos)?;
    }
    // 刀 3：radius 块（扩展块尾——ALIGN 后）。仅 Rect 合法；2B LE；
    // radius=0 空载块非规范形拒（块在 ⟺ 有义）。
    if flags & FLAG_HAS_RADIUS != 0 {
        if !matches!(kind, ViewKind::Rect) {
            return Err(AstError::UnknownProp { kind, got: FLAG_HAS_RADIUS });
        }
        if src.len() < pos + 2 {
            return Err(AstError::Truncated);
        }
        let radius = u16::from_le_bytes([src[pos], src[pos + 1]]);
        if radius == 0 {
            return Err(AstError::UnknownProp { kind, got: FLAG_HAS_RADIUS });
        }
        if let Props::Rect(p) = &mut props {
            p.radius = radius;
        }
        pos += 2;
    }
    Ok((ViewSpec { kind, key, props }, pos, a11y))
}

/// size 块落值（§8.2 合法域表）：Col/Row/Grid/Rect 合法、Image/Icon/
/// Pad/Root 拒。Col/Row/Grid 块值落扩展字段；**Rect 为容读折入面**——
/// 尺寸本在 props 定长域，块值折入 RectProps（块胜），再编码归一为
/// 无块形式（builder 面拒 Rect+size，保双入口收敛——见 DescBuilder::size）。
/// Image/Icon 拒 = 双源歧义；Text 拒 = TextProps 满载 Node 冻结无落位；
/// Pad/Root = 无定尺寸语义。合法域检在长检前（`[Root, 0x02]` 两字节即
/// 拒 UnknownProp——拒因可钉）。块 = w/h 各 u32 LE 哨兵对；**双 None =
/// 空载块拒**（规范形：块在 ⟺ 任一 Some——块存在性可从 arena 推导，
/// roundtrip 字节相等的前提）。
fn apply_size_block(
    kind: ViewKind,
    mut props: Props,
    src: &[u8],
    pos: &mut usize,
) -> Result<Props, AstError> {
    match kind {
        ViewKind::Col | ViewKind::Row | ViewKind::Grid | ViewKind::Rect => {}
        _ => return Err(AstError::UnknownProp { kind, got: FLAG_HAS_SIZE }),
    }
    if src.len() < 8 {
        return Err(AstError::Truncated);
    }
    let w = u32::from_le_bytes([src[0], src[1], src[2], src[3]]);
    let h = u32::from_le_bytes([src[4], src[5], src[6], src[7]]);
    if w == SIZE_NONE && h == SIZE_NONE {
        return Err(AstError::UnknownProp { kind, got: FLAG_HAS_SIZE }); // 空载块非规范形
    }
    let w = if w == SIZE_NONE { None } else { Some(w) };
    let h = if h == SIZE_NONE { None } else { Some(h) };
    match &mut props {
        Props::Col(p) => {
            p.w = w;
            p.h = h;
        }
        Props::Row(p) => {
            p.w = w;
            p.h = h;
        }
        Props::Grid(p) => {
            p.w = w;
            p.h = h;
        }
        Props::Rect(p) => {
            p.w = w;
            p.h = h; // 容读折入（块胜）——再编码归一无块形式
        }
        // kind 已过滤——props↔kind 失配 = 内部不变量破坏，防御 loud
        _ => return Err(AstError::UnknownProp { kind, got: FLAG_HAS_SIZE }),
    }
    *pos += 8;
    Ok(props)
}

/// align 块落值（§8.2：仅 Col/Row）。块 = cross u8；**载 Stretch 缺省
/// 拒**（规范形：块在 ⟺ 非 Stretch——同 size 块可推导性论证）。值域外
/// → cross_from_u8 的 UnknownProp。
fn apply_align_block(
    kind: ViewKind,
    mut props: Props,
    src: &[u8],
    pos: &mut usize,
) -> Result<Props, AstError> {
    match kind {
        ViewKind::Col | ViewKind::Row => {}
        _ => return Err(AstError::UnknownProp { kind, got: FLAG_HAS_ALIGN }),
    }
    if src.len() < 1 {
        return Err(AstError::Truncated);
    }
    let v = src[0];
    if v == cross_to_u8(crate::layout::CrossAlign::Stretch) {
        return Err(AstError::UnknownProp { kind, got: v }); // 载缺省非规范形
    }
    let cross = cross_from_u8(kind, v)?;
    match &mut props {
        Props::Col(p) => p.cross = cross,
        Props::Row(p) => p.cross = cross,
        _ => return Err(AstError::UnknownProp { kind, got: FLAG_HAS_ALIGN }), // 防御（同上）
    }
    *pos += 1;
    Ok(props)
}

// ---------- 整树编码（Ui::encode_desc 委托口） ----------

/// 活树前序编码进 `out`（返回写入字节数）。无根 → `Ok(0)`（空流；
/// 空流 apply 侧 `Truncated` 拒——空树不可重建根，非对称是显式语义）。
/// `a11y` = Ui 侧 a11y 表（D12——A11Y 块的唯一发射源，槽+代数+alive
/// 三重对拍命中才落块：remove 后残项天然不发射）。调用方保证 `nodes`
/// 槽满 `[Option<Node>]`、`root` 指活根或 `u32::MAX`。
pub(crate) fn encode_tree(
    nodes: &[Option<crate::Node>],
    root: u32,
    a11y: &[Option<crate::A11yEntry>],
    out: &mut [u8],
) -> Result<usize, AstError> {
    if root == u32::MAX {
        return Ok(0);
    }
    let mut cursor = 0usize;
    emit_node(nodes, root, a11y, out, &mut cursor)?;
    Ok(cursor)
}

/// a11y 表命中判定：槽位 + 代数 + alive 三重对拍（gen 推进的悬空项/
/// 死槽残项不发射——D12 侧表与 arena 的失配防御）。
fn a11y_of<'a>(
    a11y: &'a [Option<crate::A11yEntry>],
    nodes: &[Option<crate::Node>],
    slot: u32,
) -> Option<&'a crate::A11yEntry> {
    let n = nodes.get(slot as usize).and_then(|o| o.as_ref())?;
    if !n.alive {
        return None;
    }
    a11y.iter().flatten().find(|e| {
        (e.node.0 & 0xFFFF) == slot && (e.node.0 >> 16) as u16 == n.gen
    })
}

fn emit_node(
    nodes: &[Option<crate::Node>],
    slot: u32,
    a11y: &[Option<crate::A11yEntry>],
    out: &mut [u8],
    cursor: &mut usize,
) -> Result<(), AstError> {
    // 槽来自活树遍历——不可达分支映射 Truncated（防御零 panic）。
    let n = nodes
        .get(slot as usize)
        .and_then(|o| o.as_ref())
        .ok_or(AstError::Truncated)?;
    // 子数（先数后写——u8 域溢出 loud 拒）
    let mut children = 0usize;
    let mut c = n.first_child;
    while c != u32::MAX {
        children += 1;
        let next = nodes
            .get(c as usize)
            .and_then(|o| o.as_ref())
            .ok_or(AstError::Truncated)?
            .next_sib;
        c = next;
    }
    if children > CHILD_MAX {
        return Err(AstError::TooSmall { need: CHILD_MAX + 1, got: children });
    }
    // 块规范形（§8 D5——块在 ⟺ 有义，与 apply 侧拒面镜像）：
    // SIZE 块 ⟺ Col/Row/Grid 任一 w/h Some（Rect 折入面不产块——归一
    // 无块形式）；ALIGN 块 ⟺ Col/Row cross 非 Stretch；A11Y 块 ⟺ 侧表命中。
    let has_size = match &n.props {
        Props::Col(p) => p.w.is_some() || p.h.is_some(),
        Props::Row(p) => p.w.is_some() || p.h.is_some(),
        Props::Grid(p) => p.w.is_some() || p.h.is_some(),
        _ => false,
    };
    let has_align = match &n.props {
        Props::Col(p) => p.cross != crate::layout::CrossAlign::Stretch,
        Props::Row(p) => p.cross != crate::layout::CrossAlign::Stretch,
        _ => false,
    };
    let a11y_hit = a11y_of(a11y, nodes, slot);
    let mut flags = 0u8;
    if n.key != 0 {
        flags |= FLAG_HAS_KEY;
    }
    if has_size {
        flags |= FLAG_HAS_SIZE;
    }
    if a11y_hit.is_some() {
        flags |= FLAG_HAS_A11Y;
    }
    if has_align {
        flags |= FLAG_HAS_ALIGN;
    }
    let has_radius = matches!(&n.props, Props::Rect(p) if p.radius > 0);
    if has_radius {
        flags |= FLAG_HAS_RADIUS;
    }
    let mut w = 0usize;
    let base = *cursor;
    let seg = &mut out[base..];
    w += put_u8(seg, kind_to_u8(n.kind))?;
    w += put_u8(&mut seg[w..], flags)?;
    if n.key != 0 {
        w += put_u64le(&mut seg[w..], n.key)?;
    }
    let pl = encode_props(&n.props, &mut seg[w..])?;
    w += pl;
    // 扩展块序恒 SIZE→A11Y→ALIGN（与 parse_node 读序镜像）
    if has_size {
        let (bw, bh) = match &n.props {
            Props::Col(p) => (p.w, p.h),
            Props::Row(p) => (p.w, p.h),
            Props::Grid(p) => (p.w, p.h),
            // has_size 仅 Col/Row/Grid 真——防御（Rect 折入面不产块）
            _ => (None, None),
        };
        w += put_u32le(&mut seg[w..], bw.unwrap_or(SIZE_NONE))?;
        w += put_u32le(&mut seg[w..], bh.unwrap_or(SIZE_NONE))?;
    }
    if let Some(e) = a11y_hit {
        let label = e.label.as_bytes();
        w += put_u8(&mut seg[w..], e.role)?;
        w += put_u8(&mut seg[w..], label.len() as u8)?;
        if seg.len() < w + label.len() {
            return Err(AstError::TooSmall { need: w + label.len(), got: base + seg.len() });
        }
        seg[w..w + label.len()].copy_from_slice(label);
        w += label.len();
    }
    if has_align {
        let cross = match &n.props {
            Props::Col(p) => p.cross,
            Props::Row(p) => p.cross,
            // has_align 仅 Col/Row 真——防御
            _ => crate::layout::CrossAlign::Stretch,
        };
        w += put_u8(&mut seg[w..], cross_to_u8(cross))?;
    }
    // 刀 3：radius 块（扩展块尾——ALIGN 后；radius>0 ⟺ 块在）。
    if has_radius {
        if let Props::Rect(p) = &n.props {
            w += put_u16le(&mut seg[w..], p.radius)?;
        }
    }
    w += put_u8(&mut seg[w..], children as u8)?;
    *cursor += w;
    // 子树前序递归
    let mut c = n.first_child;
    while c != u32::MAX {
        emit_node(nodes, c, a11y, out, cursor)?;
        let next = nodes
            .get(c as usize)
            .and_then(|o| o.as_ref())
            .ok_or(AstError::Truncated)?
            .next_sib;
        c = next;
    }
    Ok(())
}

// ---------- 整树重建（Ui::apply_desc 委托口） ----------

/// AST 流重建整树进 arena（调用方先 reset；槽分配按前序游走确定性
/// 0..N，超容 → `TooSmall`）。`a11y_out` = Ui 侧 a11y 表落点（D12——
/// a11y 块载荷按槽序写入，满帽 TooSmall loud；调用方随 reset 清表）。
/// 成功后 `*root_out` = 根槽；尾随字节 / 叶带子（Rect/Text/Image/Icon/
/// Button/Toggle/Slider/Progress + P1 八叶 + P0 尾四叶
/// Select/Input/Edit/Canvas + P1 尾四叶 Tip/SwipeItem/DatePick/TimePick
/// + P2 五叶 Chip/Fab/Gauge/Qr/DragReorder）/
/// Root 为子 / 流首非 Root /
/// Grid 零维·超格 全 loud 拒。
pub(crate) fn apply_tree(
    src: &[u8],
    nodes: &mut [Option<crate::Node>],
    gens: &mut [u16],
    root_out: &mut u32,
    a11y_out: &mut [Option<crate::A11yEntry>],
) -> Result<(), AstError> {
    if src.is_empty() {
        return Err(AstError::Truncated);
    }
    let mut pos = 0usize;
    let mut next_slot = 0usize;
    // depth 0 基递归层号（Root = 0；R2-1 wire 深帽进 build_node 单源执法）
    let root = build_node(src, &mut pos, None, 0, nodes, gens, &mut next_slot, a11y_out)?;
    if pos != src.len() {
        return Err(AstError::TrailingBytes { extra: src.len() - pos });
    }
    *root_out = root;
    Ok(())
}

/// `depth` = 0 基递归层号（节点 1 基深度 = depth+1——与 add() 的
/// chain+1 口径同源）。
#[allow(clippy::too_many_arguments)]
fn build_node(
    src: &[u8],
    pos: &mut usize,
    parent: Option<u32>,
    depth: usize,
    nodes: &mut [Option<crate::Node>],
    gens: &mut [u16],
    next_slot: &mut usize,
    a11y_out: &mut [Option<crate::A11yEntry>],
) -> Result<u32, AstError> {
    // D29 wire 深帽（R2-1 收编）：apply_desc 直调本递归不经 builder——
    // DESC_DEPTH_MAX 只护构建期 begin 面，此前唯一界 = 槽位帽
    // nodes.len()，恶意 Grid 1×1 嵌套链（cells≥cc 全合法）可建深树入
    // arena，measure/place 递归穿 32KB guest 栈。与 add() 同单源
    // TREE_DEPTH_MAX（lib.rs——勿在此复写字面量）；错误面 add-only
    // 复用 TooSmall（builder 深帽同形态先例，need = 被拒层 1 基深度）。
    if depth as u32 >= crate::TREE_DEPTH_MAX {
        return Err(AstError::TooSmall { need: depth + 1, got: crate::TREE_DEPTH_MAX as usize });
    }
    let (spec, used, a11y) = parse_node(&src[*pos..])?;
    // 树形拒面：流首必须 Root；Root 不能为子。
    match (parent, spec.kind) {
        (None, ViewKind::Root) => {}
        (None, _) | (Some(_), ViewKind::Root) => return Err(AstError::BadParent),
        (Some(_), _) => {}
    }
    *pos += used;
    // child_count（树形走查域——节点本体之后恰一字节）
    let cc = *src.get(*pos).ok_or(AstError::Truncated)? as usize;
    *pos += 1;
    if matches!(
        spec.kind,
        ViewKind::Rect
            | ViewKind::Text
            | ViewKind::Image
            | ViewKind::Icon
            | ViewKind::Button
            | ViewKind::Toggle
            | ViewKind::Slider
            | ViewKind::Progress
            | ViewKind::Spacer
            | ViewKind::Divider
            | ViewKind::Check
            | ViewKind::Radio
            | ViewKind::Step
            | ViewKind::Seg
            | ViewKind::Badge
            | ViewKind::Sbar
            | ViewKind::Select
            | ViewKind::Input
            | ViewKind::Edit
            | ViewKind::Canvas
            | ViewKind::Tip
            | ViewKind::SwipeItem
            | ViewKind::DatePick
            | ViewKind::TimePick
            | ViewKind::Chip
            | ViewKind::Fab
            | ViewKind::Gauge
            | ViewKind::Qr
            | ViewKind::DragReorder
    ) && cc > 0
    {
        return Err(AstError::BadParent); // 叶子带子拒面（Image/Icon 入列——D24；组件叶同列——Phase 1 + P1 + P0 尾四 + P1 尾五 + P2 借来件批）
    }
    // Grid 适配检：cols×rows < 子数 → GridNotFit（超格不容忍静默截断；
    // 零维已在 parse_props 拒——此处 props 必带正维）
    if let Props::Grid(gp) = &spec.props {
        let cells = gp.cols as u32 * gp.rows as u32;
        if cells < cc as u32 {
            return Err(AstError::GridNotFit { cols: gp.cols, rows: gp.rows, children: cc as u32 });
        }
    }
    // 槽分配（确定性前序）
    let slot = *next_slot;
    if slot >= nodes.len() {
        return Err(AstError::TooSmall { need: slot + 1, got: nodes.len() });
    }
    *next_slot += 1;
    nodes[slot] = Some(crate::Node {
        gen: gens[slot],
        alive: true,
        kind: spec.kind,
        key: spec.key,
        props: spec.props,
        parent: parent.unwrap_or(u32::MAX),
        first_child: u32::MAX,
        next_sib: u32::MAX,
        prev_sib: u32::MAX,
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
    // a11y 块载荷落侧表（D12：Node 冻结 19 字段无 a11y 位——表存
    // NodeId（gen<<16|slot，gen 取本槽现行值），发射面三重对拍防悬空）
    if let Some((role, label)) = a11y {
        let entry = crate::A11yEntry {
            node: crate::NodeId(((gens[slot] as u32) << 16) | slot as u32),
            role,
            label,
        };
        let free = a11y_out.iter_mut().find(|e| e.is_none());
        match free {
            Some(e) => *e = Some(entry),
            None => {
                return Err(AstError::TooSmall {
                    need: a11y_out.len() + 1,
                    got: a11y_out.len(),
                })
            }
        }
    }
    // 子链按流序 append（保前序稳定；递归层号 +1——深帽单源在函数头）
    let mut prev = u32::MAX;
    for _ in 0..cc {
        let child = build_node(src, pos, Some(slot as u32), depth + 1, nodes, gens, next_slot, a11y_out)?;
        if let Some(p) = nodes[slot].as_mut() {
            if p.first_child == u32::MAX {
                p.first_child = child;
            }
        }
        if prev != u32::MAX {
            if let Some(pp) = nodes[prev as usize].as_mut() {
                pp.next_sib = child;
            }
        }
        if let Some(cp) = nodes[child as usize].as_mut() {
            cp.prev_sib = prev;
        }
        prev = child;
    }
    Ok(slot as u32)
}

// ---------- DescBuilder（入口②的流构造器——begin/end 嵌套） ----------

/// builder 已落块位图（BFrame.blocks——阶段序 SIZE→A11Y→ALIGN 的执法面）。
const BLK_SIZE: u8 = 1;
const BLK_A11Y: u8 = 2;
const BLK_ALIGN: u8 = 4;

/// AST 流构造器：`begin(kind, key, props)` … `end()` 嵌套描述树，child_count
/// 由 end 回填（占位-补数——与 apply_tree 读位自洽）。首 begin 必须 Root、
/// Root/叶（Rect/Text/Image/Icon/Button/Toggle/Slider/Progress + P1 八叶
/// Spacer/Divider/Check/Radio/Step/Seg/Badge/Sbar + P0 尾四叶
/// Select/Input/Edit/Canvas + P1 尾四叶 Tip/SwipeItem/DatePick/TimePick
/// + P2 五叶 Chip/Fab/Gauge/Qr/DragReorder）
/// 下不可再
/// begin（BadParent）；深度帽
/// DESC_DEPTH_MAX、缓冲帽 DESC_MAX、子数帽 255——全 TooSmall loud。
///
/// GUI-6 §8 扩展块面（开层后、首子前链式落块——块序由阶段位图保证恒
/// SIZE→A11Y→ALIGN）：`.size(w,h)` / `.a11y(role,label)` / `.align(cross)`。
/// 面向 [`size`]：块 API 为 `&mut self + Result`（by-value 链式无法 loud
/// 拒——§6 签名草图的实现面修正）。
pub struct DescBuilder {
    buf: [u8; DESC_MAX],
    len: usize,
    frames: [BFrame; DESC_DEPTH_MAX],
    depth: usize,
}

#[derive(Clone, Copy)]
struct BFrame {
    kind: ViewKind,
    /// 层首字节位（flags 字节 = base+1——块 API 就地点旗）。
    base: usize,
    /// child_count 占位字节位（块插位 = 当前占位处；end 回填）。
    count_pos: usize,
    children: u8,
    /// 已落扩展块位图（阶段序执法——dup/乱序拒）。
    blocks: u8,
    /// Grid 父格容（cols/rows 快照——子 begin 时适配检；非 Grid 恒 0）。
    gcols: u8,
    grows: u8,
}

impl DescBuilder {
    /// 空构造器。
    pub fn new() -> Self {
        DescBuilder {
            buf: [0; DESC_MAX],
            len: 0,
            frames: [BFrame {
                kind: ViewKind::Root,
                base: 0,
                count_pos: 0,
                children: 0,
                blocks: 0,
                gcols: 0,
                grows: 0,
            }; DESC_DEPTH_MAX],
            depth: 0,
        }
    }

    /// 开层（写节点本体 + child_count 占位字节）。
    ///
    /// **基域收窄注记（R2-7 foot-gun 钉）**：`props` 只写 encode_props
    /// 基域——Col/Row 基域 = gap（`w`/`h`/`cross` 静默不入流，须 `.size()`/
    /// `.align()` 块 API 另载）；Grid 基域 = cols/rows/gap/cross（`w`/`h`
    /// 须 `.size()` 另载）。传全量 Props 而漏块 API = 字段静默丢失（如
    /// `GridProps{w: Some(100), ..}` 得全幅 Grid）——调用面自查清单。
    pub fn begin(&mut self, kind: ViewKind, key: u64, props: &Props) -> Result<(), AstError> {
        if self.len == 0 && kind != ViewKind::Root {
            return Err(AstError::BadParent); // 流首必须 Root
        }
        // Grid 本体先验：kind×props 失配 / 零维（泛型 begin 面新 kind 收紧）
        let (gcols, grows) = match (kind, props) {
            (ViewKind::Grid, Props::Grid(gp)) => {
                if gp.cols == 0 || gp.rows == 0 {
                    return Err(AstError::GridZeroDims);
                }
                (gp.cols, gp.rows)
            }
            (ViewKind::Grid, _) => {
                return Err(AstError::UnknownProp { kind, got: KIND_GRID });
            }
            _ => (0, 0),
        };
        // 组件值域先验（builder 生产面与 parse 流面同律——validate_phase1
        // + P1 批 validate_p1_nine + P0 尾四件批 validate_p0_four + P1 尾
        // 五件批 validate_p1_last_five + P2 借来件批 validate_p2_seven，
        // Grid 双面先例）
        validate_phase1(kind, props)?;
        validate_p1_nine(kind, props)?;
        validate_p0_four(kind, props)?;
        validate_p1_last_five(kind, props)?;
        validate_p2_seven(kind, props)?;
        if self.depth > 0 {
            let parent_kind = self.frames[self.depth - 1].kind;
            if kind == ViewKind::Root {
                return Err(AstError::BadParent); // Root 不能为子
            }
            if matches!(
                parent_kind,
                ViewKind::Rect
                    | ViewKind::Text
                    | ViewKind::Image
                    | ViewKind::Icon
                    | ViewKind::Button
                    | ViewKind::Toggle
                    | ViewKind::Slider
                    | ViewKind::Progress
                    | ViewKind::Spacer
                    | ViewKind::Divider
                    | ViewKind::Check
                    | ViewKind::Radio
                    | ViewKind::Step
                    | ViewKind::Seg
                    | ViewKind::Badge
                    | ViewKind::Sbar
                    | ViewKind::Select
                    | ViewKind::Input
                    | ViewKind::Edit
                    | ViewKind::Canvas
                    | ViewKind::Tip
                    | ViewKind::SwipeItem
                    | ViewKind::DatePick
                    | ViewKind::TimePick
                    | ViewKind::Chip
                    | ViewKind::Fab
                    | ViewKind::Gauge
                    | ViewKind::Qr
                    | ViewKind::DragReorder
            ) {
                return Err(AstError::BadParent); // 叶子不带子（Image/Icon 入列——D24；组件叶同列——Phase 1 + P1 + P0 尾四 + P1 尾五 + P2 借来件批）
            }
            // Grid 父适配检：格满再开子 → GridNotFit（payload = 被拒子序号）
            if parent_kind == ViewKind::Grid {
                let pf = self.frames[self.depth - 1];
                let cells = pf.gcols as u32 * pf.grows as u32;
                if pf.children as u32 >= cells {
                    return Err(AstError::GridNotFit {
                        cols: pf.gcols,
                        rows: pf.grows,
                        children: pf.children as u32 + 1,
                    });
                }
            }
        }
        if self.depth >= DESC_DEPTH_MAX {
            return Err(AstError::TooSmall { need: self.depth + 1, got: DESC_DEPTH_MAX });
        }
        let base = self.len;
        let seg = &mut self.buf[base..];
        let mut w = 0usize;
        w += put_u8(seg, kind_to_u8(kind))?;
        let flags = if key != 0 { FLAG_HAS_KEY } else { 0 };
        w += put_u8(&mut seg[w..], flags)?;
        if key != 0 {
            w += put_u64le(&mut seg[w..], key)?;
        }
        w += encode_props(props, &mut seg[w..])?;
        // child_count 占位（end 回填）
        w += put_u8(&mut seg[w..], 0)?;
        self.frames[self.depth] = BFrame {
            kind,
            base,
            count_pos: base + w - 1,
            children: 0,
            blocks: 0,
            gcols,
            grows,
        };
        self.depth += 1;
        self.len += w;
        Ok(())
    }

    /// Grid 开层捷径（props 一次性传——零维/适配检在 [`begin`] 单源）。
    /// 基域 = cols/rows/gap/cross；`props.w`/`props.h` **不入基域**——
    /// 须 `.size()` 块 API 另载（[begin] 基域收窄注记同源）。
    pub fn begin_grid(&mut self, key: u64, props: &GridProps) -> Result<(), AstError> {
        self.begin(ViewKind::Grid, key, &Props::Grid(*props))
    }

    /// Image 开层捷径。
    pub fn begin_image(&mut self, key: u64, props: &ImageProps) -> Result<(), AstError> {
        self.begin(ViewKind::Image, key, &Props::Image(*props))
    }

    /// Icon 开层捷径。
    pub fn begin_icon(&mut self, key: u64, props: &IconProps) -> Result<(), AstError> {
        self.begin(ViewKind::Icon, key, &Props::Icon(*props))
    }

    /// 块落前置检（当前层可落块 = 已 begin 且未开首子——占位字节即流尾；
    /// 阶段序由位值序执法：块位值 1/2/4 = SIZE/A11Y/ALIGN 递增，已有
    /// **同级或更后**块再落即拒——dup 与乱序（如 A11Y 后补 SIZE）同面）。
    /// 返回 (kind, count_pos)。
    fn block_pre(&self, want: u8) -> Result<(ViewKind, usize), AstError> {
        if self.depth == 0 {
            return Err(AstError::BadParent); // 无层可挂块（与 end 无 begin 同族）
        }
        let f = self.frames[self.depth - 1];
        if self.len != f.count_pos + 1 {
            return Err(AstError::BadParent); // 已开子/已闭层——块不落（先验拒）
        }
        if f.blocks >= want {
            return Err(AstError::BadParent); // dup 块或阶段乱序（SIZE 必先）
        }
        Ok((f.kind, f.count_pos))
    }

    /// 就地插块（child_count 占位处——占位后移；当前层必为流尾，无后
    /// 续字节需修——祖先占位均在本层 base 之前不受扰）。
    fn insert_at(&mut self, at: usize, bytes: &[u8]) -> Result<(), AstError> {
        let need = self.len + bytes.len();
        if need > DESC_MAX {
            return Err(AstError::TooSmall { need, got: DESC_MAX });
        }
        self.buf.copy_within(at..self.len, at + bytes.len());
        self.buf[at..at + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }

    /// size 块（w/h 各 u32 LE 哨兵对）。合法域 = Col/Row/Grid（§8.2 表
    /// 生产面：Rect 容读折入面在 builder **拒**——builder 产规范流，
    /// Rect+size 块再编码不可逐字节复原，破双入口收敛；Text/Image/Icon/
    /// Pad/Root 拒 = 解析面同表）。双 None 空载块拒（规范形）。
    pub fn size(&mut self, w: Option<u32>, h: Option<u32>) -> Result<(), AstError> {
        let (kind, at) = self.block_pre(BLK_SIZE)?;
        match kind {
            ViewKind::Col | ViewKind::Row | ViewKind::Grid => {}
            _ => return Err(AstError::UnknownProp { kind, got: FLAG_HAS_SIZE }),
        }
        if w.is_none() && h.is_none() {
            return Err(AstError::UnknownProp { kind, got: FLAG_HAS_SIZE }); // 空载块非规范形
        }
        let mut blk = [0u8; 8];
        blk[..4].copy_from_slice(&w.unwrap_or(SIZE_NONE).to_le_bytes());
        blk[4..].copy_from_slice(&h.unwrap_or(SIZE_NONE).to_le_bytes());
        self.insert_at(at, &blk)?;
        let f = &mut self.frames[self.depth - 1];
        f.count_pos += blk.len();
        f.blocks |= BLK_SIZE;
        self.buf[f.base + 1] |= FLAG_HAS_SIZE; // flags 就地点旗
        Ok(())
    }

    /// a11y 块（role u8 + len u8 + label——全 kind 合法，§8.2）。label 超
    /// `TEXT_INLINE` → TextTooLong（与内联文本同帽）。
    pub fn a11y(&mut self, role: u8, label: &[u8]) -> Result<(), AstError> {
        let (_kind, at) = self.block_pre(BLK_A11Y)?;
        if label.len() > crate::TEXT_INLINE {
            return Err(AstError::TextTooLong { len: label.len() });
        }
        let mut blk = [0u8; 2 + crate::TEXT_INLINE];
        blk[0] = role;
        blk[1] = label.len() as u8;
        blk[2..2 + label.len()].copy_from_slice(label);
        let blen = 2 + label.len();
        self.insert_at(at, &blk[..blen])?;
        let f = &mut self.frames[self.depth - 1];
        f.count_pos += blen;
        f.blocks |= BLK_A11Y;
        self.buf[f.base + 1] |= FLAG_HAS_A11Y;
        Ok(())
    }

    /// align 块（cross u8——仅 Col/Row，§8.2）。载 Stretch 缺省拒
    /// （规范形：块在 ⟺ 非 Stretch——与解析面镜像）。
    pub fn align(&mut self, cross: crate::layout::CrossAlign) -> Result<(), AstError> {
        let (kind, at) = self.block_pre(BLK_ALIGN)?;
        match kind {
            ViewKind::Col | ViewKind::Row => {}
            _ => return Err(AstError::UnknownProp { kind, got: FLAG_HAS_ALIGN }),
        }
        if cross == crate::layout::CrossAlign::Stretch {
            return Err(AstError::UnknownProp { kind, got: cross_to_u8(cross) }); // 载缺省非规范形
        }
        let blk = [cross_to_u8(cross)];
        self.insert_at(at, &blk)?;
        let f = &mut self.frames[self.depth - 1];
        f.count_pos += blk.len();
        f.blocks |= BLK_ALIGN;
        self.buf[f.base + 1] |= FLAG_HAS_ALIGN;
        Ok(())
    }

    /// 闭层（回填 child_count；父层子数 +1）。无层可闭 → BadParent。
    /// 父层子数帽检在**动状态前**（先验拒——失败零半执行：depth 与
    /// child_count 均未动，原地重试幂等；修复轮 finding 17：先减深度
    /// 再拒会把「重试的 end」错关到祖父层）。
    pub fn end(&mut self) -> Result<(), AstError> {
        if self.depth == 0 {
            return Err(AstError::BadParent); // end 无 begin 失配
        }
        if self.depth > 1 {
            let p_children = self.frames[self.depth - 2].children;
            if p_children as usize >= CHILD_MAX {
                return Err(AstError::TooSmall { need: CHILD_MAX + 1, got: p_children as usize });
            }
        }
        self.depth -= 1;
        let f = self.frames[self.depth];
        self.buf[f.count_pos] = f.children;
        if self.depth > 0 {
            self.frames[self.depth - 1].children += 1;
        }
        Ok(())
    }

    /// 已编码流（begin/end 平衡后取用）。
    pub fn bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    /// 已编码字节数。
    pub fn len(&self) -> usize {
        self.len
    }

    /// 空谓词（尚未 begin 根）。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for DescBuilder {
    fn default() -> Self {
        DescBuilder::new()
    }
}
