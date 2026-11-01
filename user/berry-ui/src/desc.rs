//! desc：数据驱动描述层——视图集/属性族/Msg 面 + **AST 二进制 TLV
//! 双入口**（`docs/GUI-3实施设计草案.md` §3.2/§6 冻结契约；ADR #52
//! 「API 面数据驱动 + 描述结构可序列化双入口」day-one 落法）。
//!
//! # AST 二进制格式（§3.2——逐字节定义，roundtrip 字节相等靠格式自洽）
//!
//! 前序 TLV 流，节点连续排列（无总长域——流长即树界）：
//!
//! ```text
//! node   := kind u8 | flags u8 | key u64 LE（仅 flags.bit0）| props | child_count u8
//! flags  := bit0 = has_key（其余位未知 → UnknownFlags 拒）
//! props  := kind 定长：Root 0B / Rect 11B / Text 变长(6..=9+len) / Col·Row·Pad 4B
//! ```
//!
//! - `child_count` 是**树形走查域**（不属于节点本体）：[`parse_node`] 只
//!   解析节点本体（kind/flags/key/props），child_count 由树走查方
//!   （[`apply_tree`]/[`DescBuilder`]）读写——两处同一字节位，格式自洽。
//! - Rect 11B = bg 3B + w/h 各 4B（`u32::MAX` 哨兵 = None「随父」）。
//! - Col/Row/Pad 4B = gap/all u32 LE。
//! - Text 变长 = len u8 + 文本字节 + scale u8 + fg 3B + bg tag u8
//!   （0=None / 1=Some）+ bg 3B（仅 Some）——max = 1+64+1+3+1+3 = 73B
//!   （§3.2 原稿写「≤69B」未计 TextStyle.bg 的 Option 载荷 3B + tag；
//!   格式自洽优先，roundtrip 性质不受影响）。
//! - 拒面全 loud（guiproto 同款纪律）：未知 kind / flags 未知位 / 截断 /
//!   尾随字节 / 叶子带子 / Root 为子 / 文本超长，一律 `Err`。
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

// ---------- 视图集（v1 六件——§3.2） ----------

/// 视图种类：`Root/Rect/Text` 与容器 `Col/Row/Pad`（叶子带子 = AST 拒面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Root,
    Rect,
    Text,
    Col,
    Row,
    Pad,
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

/// 纵向容器属性（子间缝隙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColProps {
    pub gap: u32,
}

/// 横向容器属性（子间缝隙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowProps {
    pub gap: u32,
}

/// 内缩容器属性（四向同缩——v1 单值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadProps {
    pub all: u32,
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

/// 输入消息七变体（§3.6 手势 referee 产物；引擎内零闭包零回调副作用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    Press { node: NodeId, x: u32, y: u32 },
    Release { node: NodeId, x: u32, y: u32 },
    Click { node: NodeId },
    DragStart { node: NodeId, x: u32, y: u32 },
    DragMove { node: NodeId, x: u32, y: u32, resampled: bool },
    DragEnd { node: NodeId },
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
    /// flags 未知位（bit0 = has_key 之外）。
    UnknownFlags { got: u8 },
    /// 属性域未知子值（Text 的 scale / bg tag）。
    UnknownProp { kind: ViewKind, got: u8 },
    /// 输入流截断（节点/props/child_count 中途尽）。
    Truncated,
    /// 根树解析毕仍有尾随字节。
    TrailingBytes { extra: usize },
    /// 树形拒面：流首非 Root / Root 为子 / 叶子带子 / begin·end 失配。
    BadParent,
    /// 文本超 `TEXT_INLINE`。
    TextTooLong { len: usize },
}

// ---------- TLV 格式常量（wire 值域——编解码双侧单源在此） ----------

/// kind 字节域（编解码对偶表；新增视图种类 = 双侧同步 + UnknownKind 拒旧流）。
pub const KIND_ROOT: u8 = 0;
pub const KIND_RECT: u8 = 1;
pub const KIND_TEXT: u8 = 2;
pub const KIND_COL: u8 = 3;
pub const KIND_ROW: u8 = 4;
pub const KIND_PAD: u8 = 5;

/// flags bit0 = has_key（其余位未知拒）。
pub const FLAG_HAS_KEY: u8 = 1;

/// `Option<u32>` 的 wire 哨兵（Rect w/h「随父」档）。
const OPT_NONE: u32 = u32::MAX;

/// DescBuilder 缓冲容量（整树字节上界）。
pub const DESC_MAX: usize = 4096;

/// DescBuilder 嵌套深度帽（容器嵌套上界；超 → TooSmall loud）。
pub const DESC_DEPTH_MAX: usize = 16;

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
        _ => None,
    }
}

fn scale_to_u8(s: FontSize) -> u8 {
    match s {
        FontSize::S8x16 => 0,
        FontSize::S16x32 => 1,
        FontSize::S24x48 => 2,
    }
}

fn scale_from_u8(kind: ViewKind, v: u8) -> Result<FontSize, AstError> {
    match v {
        0 => Ok(FontSize::S8x16),
        1 => Ok(FontSize::S16x32),
        2 => Ok(FontSize::S24x48),
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
            Ok((Props::Col(ColProps { gap: u32at(0) }), 4))
        }
        ViewKind::Row => {
            need(4)?;
            Ok((Props::Row(RowProps { gap: u32at(0) }), 4))
        }
        ViewKind::Pad => {
            need(4)?;
            Ok((Props::Pad(PadProps { all: u32at(0) }), 4))
        }
    }
}

/// 节点本体解析（§6 冻结签名）：`kind u8 | flags u8 | key u64 LE? | props`。
/// **不含 child_count**（树形走查域——由 apply_tree/DescBuilder 读写）。
/// 拒面：截断 / 未知 kind / flags 未知位 / props 域畸形。
pub(crate) fn parse_node(src: &[u8]) -> Result<(ViewSpec, usize), AstError> {
    if src.len() < 2 {
        return Err(AstError::Truncated);
    }
    let kind = kind_from_u8(src[0]).ok_or(AstError::UnknownKind { got: src[0] })?;
    let flags = src[1];
    if flags & !FLAG_HAS_KEY != 0 {
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
    let (props, plen) = parse_props(kind, &src[pos..])?;
    pos += plen;
    Ok((ViewSpec { kind, key, props }, pos))
}

// ---------- 整树编码（Ui::encode_desc 委托口） ----------

/// 活树前序编码进 `out`（返回写入字节数）。无根 → `Ok(0)`（空流；
/// 空流 apply 侧 `Truncated` 拒——空树不可重建根，非对称是显式语义）。
/// 调用方保证 `nodes` 槽满 `[Option<Node>]`、`root` 指活根或 `u32::MAX`。
pub(crate) fn encode_tree(
    nodes: &[Option<crate::Node>],
    root: u32,
    out: &mut [u8],
) -> Result<usize, AstError> {
    if root == u32::MAX {
        return Ok(0);
    }
    let mut cursor = 0usize;
    emit_node(nodes, root, out, &mut cursor)?;
    Ok(cursor)
}

fn emit_node(
    nodes: &[Option<crate::Node>],
    slot: u32,
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
    let mut w = 0usize;
    let base = *cursor;
    let seg = &mut out[base..];
    w += put_u8(seg, kind_to_u8(n.kind))?;
    let flags = if n.key != 0 { FLAG_HAS_KEY } else { 0 };
    w += put_u8(&mut seg[w..], flags)?;
    if n.key != 0 {
        w += put_u64le(&mut seg[w..], n.key)?;
    }
    let pl = encode_props(&n.props, &mut seg[w..])?;
    w += pl;
    w += put_u8(&mut seg[w..], children as u8)?;
    *cursor += w;
    // 子树前序递归
    let mut c = n.first_child;
    while c != u32::MAX {
        emit_node(nodes, c, out, cursor)?;
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
/// 0..N，超容 → `TooSmall`）。链回/拒面后清理归调用方（零半执行）。
/// 成功后 `*root_out` = 根槽；尾随字节 / 叶带子 / Root 为子 / 流首非
/// Root 全 loud 拒。
pub(crate) fn apply_tree(
    src: &[u8],
    nodes: &mut [Option<crate::Node>],
    gens: &mut [u16],
    root_out: &mut u32,
) -> Result<(), AstError> {
    if src.is_empty() {
        return Err(AstError::Truncated);
    }
    let mut pos = 0usize;
    let mut next_slot = 0usize;
    let root = build_node(src, &mut pos, None, nodes, gens, &mut next_slot)?;
    if pos != src.len() {
        return Err(AstError::TrailingBytes { extra: src.len() - pos });
    }
    *root_out = root;
    Ok(())
}

fn build_node(
    src: &[u8],
    pos: &mut usize,
    parent: Option<u32>,
    nodes: &mut [Option<crate::Node>],
    gens: &mut [u16],
    next_slot: &mut usize,
) -> Result<u32, AstError> {
    let (spec, used) = parse_node(&src[*pos..])?;
    // 树形拒面：流首必须 Root；Root 不能为子。
    match (parent, spec.kind) {
        (None, ViewKind::Root) => {}
        (None, _) | (Some(_), ViewKind::Root) => return Err(AstError::BadParent),
        (Some(_), _) => {}
    }
    *pos += used;
    // child_count（树形走查域——节点本体之后恰一字节）
    let cc = *src.get(*pos).ok_or(AstError::Truncated)?;
    *pos += 1;
    if matches!(spec.kind, ViewKind::Rect | ViewKind::Text) && cc > 0 {
        return Err(AstError::BadParent); // 叶子带子拒面
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
    // 子链按流序 append（保前序稳定）
    let mut prev = u32::MAX;
    for _ in 0..cc {
        let child = build_node(src, pos, Some(slot as u32), nodes, gens, next_slot)?;
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

/// AST 流构造器：`begin(kind, key, props)` … `end()` 嵌套描述树，child_count
/// 由 end 回填（占位-补数——与 apply_tree 读位自洽）。首 begin 必须 Root、
/// Root/叶下不可再 begin（BadParent）；深度帽 DESC_DEPTH_MAX、缓冲帽
/// DESC_MAX、子数帽 255——全 TooSmall loud。
pub struct DescBuilder {
    buf: [u8; DESC_MAX],
    len: usize,
    frames: [BFrame; DESC_DEPTH_MAX],
    depth: usize,
}

#[derive(Clone, Copy)]
struct BFrame {
    kind: ViewKind,
    count_pos: usize,
    children: u8,
}

impl DescBuilder {
    /// 空构造器。
    pub fn new() -> Self {
        DescBuilder { buf: [0; DESC_MAX], len: 0, frames: [BFrame { kind: ViewKind::Root, count_pos: 0, children: 0 }; DESC_DEPTH_MAX], depth: 0 }
    }

    /// 开层（写节点本体 + child_count 占位字节）。
    pub fn begin(&mut self, kind: ViewKind, key: u64, props: &Props) -> Result<(), AstError> {
        if self.len == 0 && kind != ViewKind::Root {
            return Err(AstError::BadParent); // 流首必须 Root
        }
        if self.depth > 0 {
            let parent_kind = self.frames[self.depth - 1].kind;
            if kind == ViewKind::Root {
                return Err(AstError::BadParent); // Root 不能为子
            }
            if matches!(parent_kind, ViewKind::Rect | ViewKind::Text) {
                return Err(AstError::BadParent); // 叶子不带子
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
        self.frames[self.depth] = BFrame { kind, count_pos: base + w - 1, children: 0 };
        self.depth += 1;
        self.len += w;
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
