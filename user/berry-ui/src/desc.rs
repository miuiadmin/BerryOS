//! desc：数据驱动描述层——视图集/属性族/Msg 面 + **AST 二进制 TLV
//! 双入口**（`docs/GUI-3实施设计草案.md` §3.2/§6 冻结契约；ADR #52
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
//!           Col·Row·Pad 4B / Image 10B / Icon 13B / Grid 7B（GUI-6 §8.1）
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
//! - 块×kind 合法域（§8.2 表）：SIZE 容器通则 Col/Row/Grid 合法（Text/
//!   Image/Icon 双源歧义拒——尺寸已在 props 定长域；Pad/Root 拒）；
//!   ALIGN 仅 Col/Row；A11Y 全 kind 合法（落点 = Ui 侧 a11y 表，D12）。
//! - 旧流（v1 六 kind、flags ∈ {0,1}）逐字节不变——bit1/2/3 立法对
//!   不立块的流零扰动（GOLDEN_V1 守卫）。
//! - 拒面全 loud（guiproto 同款纪律）：未知 kind / flags bit4+ / 截断 /
//!   尾随字节 / 叶子带子（Rect/Text/Image/Icon）/ Root 为子 / 文本超长 /
//!   块×kind 失配 / Grid 零维·超格，一律 `Err`。
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

// ---------- 视图集（v1 六件 + GUI-6 段B 三新件——§3.2/§8） ----------

/// 视图种类：`Root/Rect/Text`、容器 `Col/Row/Pad/Grid` 与新叶
/// `Image/Icon`（叶子带子 = AST 拒面——Rect/Text/Image/Icon 四叶）。
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
}

/// 矩形叶子属性：`None` = 尺寸随父（layout 分配）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectProps {
    pub bg: Color,
    pub w: Option<u32>,
    pub h: Option<u32>,
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

/// flags bit0 = has_key（v1）。
pub const FLAG_HAS_KEY: u8 = 1;
/// flags bit1 = has_size（GUI-6 D5：size 块 8B——w/h 各 u32 LE 哨兵对）。
pub const FLAG_HAS_SIZE: u8 = 1 << 1;
/// flags bit2 = has_a11y（role u8 + len u8 + label ≤64B——落 Ui 侧表）。
pub const FLAG_HAS_A11Y: u8 = 1 << 2;
/// flags bit3 = has_align（cross u8——仅 Col/Row）。
pub const FLAG_HAS_ALIGN: u8 = 1 << 3;
/// flags 已知位掩码（bit4+ → UnknownFlags 拒——绊线改钉 0x10）。
pub const FLAG_KNOWN: u8 = FLAG_HAS_KEY | FLAG_HAS_SIZE | FLAG_HAS_A11Y | FLAG_HAS_ALIGN;

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
/// 成功后 `*root_out` = 根槽；尾随字节 / 叶带子（Rect/Text/Image/Icon）
/// / Root 为子 / 流首非 Root / Grid 零维·超格 全 loud 拒。
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
    if matches!(spec.kind, ViewKind::Rect | ViewKind::Text | ViewKind::Image | ViewKind::Icon)
        && cc > 0
    {
        return Err(AstError::BadParent); // 叶子带子拒面（Image/Icon 入列——D24）
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
/// Root/叶（Rect/Text/Image/Icon）下不可再 begin（BadParent）；深度帽
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
        if self.depth > 0 {
            let parent_kind = self.frames[self.depth - 1].kind;
            if kind == ViewKind::Root {
                return Err(AstError::BadParent); // Root 不能为子
            }
            if matches!(parent_kind, ViewKind::Rect | ViewKind::Text | ViewKind::Image | ViewKind::Icon)
            {
                return Err(AstError::BadParent); // 叶子不带子（Image/Icon 入列——D24）
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
