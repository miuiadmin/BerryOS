//! berry-ui 布局引擎（GUI-3——`docs/GUI-3实施设计草案-v1.0.md` §3.3）。
//!
//! - **单遍自顶向下**：容器分配子界——Col/Row 累进游标 + gap；Pad 四
//!   向内缩；Panel = 可绘背景 + padding/gap 的纵向内容容器；Root =
//!   viewport 持有者。叶子尺寸 = 定值（`Option<u32>`，
//!   None = 随父——主轴 None 的 Rect 折内在 0，十字轴 None = 吃满容
//!   器分配）或内在（Text = `len × cell`——`FontSize::cell_w/cell_h`
//!   单源，ch u8 字节推进）。
//! - **Measure/Render 保留**（缓存）：每节点存内在尺寸 `meas_w/h` +
//!   `layout_dirty`——改色/改 paint 位 → 0 重测（`LayoutStat.measured`
//!   == 0 可断言）；改文/改尺寸 → 仅脏子树 + 受影响祖先聚合链重测。
//! - **摆位粒度**：节点重摆 iff（祖先强制 ∨ 自身重测）——祖先强制 =
//!   该祖先几何变更 ∨ 自身重测（游标/内缩分配面变）。重摆节点新旧绝
//!   对界（含 ox/oy paint 偏移）均进 damage。
//! - **摆位坐标 = 父相对 x/y**（§6 Node 注）；绝对链含沿途 ox/oy（视
//!   觉足迹口径，与 hit/anim 同源）——i64 累计防 i32 链加法溢出。
//! - 零 panic / 零分配：环/死槽防御以 nodes.len() 为帽；`freed` 参数
//!   为 §6 冻结面（树走查自根，空闲槽不在链上天然跳过——签名保留）。

use crate::damage::{i64_to_i32_sat, u32_to_i32_sat, u64_to_u32_sat, DamageSet, RectI};
use crate::desc::{Orientation, Props, ViewKind};
use crate::metrics::{IconSource, TextMetrics};
use crate::{FlexEntry, Node};

/// 布局产出统计（§6 冻结字段）：`measured` = 重测内在尺寸的节点数
/// （缓存有效性的断言面——0 = 全缓存命中）；`placed` = 重摆 x/y/w/h
/// 的节点数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutStat {
    pub measured: u32,
    pub placed: u32,
}

/// 十字轴对齐（GUI-6 §4.1/D9 面；wire 0/1/2）。v1 缺省恒 Stretch（=
/// 现状语义）。pub 因 Col/Row/Grid props 公有字段携载（desc wire 经
/// `desc::cross_to_u8/from_u8` 单源换算）；Center/Start 摆位消费 = 段C。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAlign {
    Start,
    Center,
    Stretch,
}

/// 根播种约束（GUI-14 L1 最小面）。v1 measure 仍约束盲；本约束只影响
/// Root 的 intrinsic、Root place avail 与终值三处。`Viewport` = 现状；
/// `Tight` = 精确盒；`Loose` = 子包络 × payload 上限；`Infinite` = 子
/// 包络不设上限。damage clip 恒用真实 viewport，不用约束盒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraints {
    Viewport,
    Tight { w: u32, h: u32 },
    Loose { w: u32, h: u32 },
    Infinite,
}

/// LAYOUT-L4 四元组约束。`min <= max` 是结构不变量；非法构造 loud
/// 拒，不做静默 normalize。字段私有，消费面只经代数方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxConstraints {
    min_w: u32,
    max_w: u32,
    min_h: u32,
    max_h: u32,
}

impl BoxConstraints {
    /// 构造约束盒；任一轴 min > max 返回 [`crate::UiError::NodeBad`]。
    pub fn new(
        min_w: u32,
        max_w: u32,
        min_h: u32,
        max_h: u32,
    ) -> Result<Self, crate::UiError> {
        if min_w > max_w || min_h > max_h {
            return Err(crate::UiError::NodeBad);
        }
        Ok(Self { min_w, max_w, min_h, max_h })
    }

    pub const fn tight(w: u32, h: u32) -> Self {
        Self { min_w: w, max_w: w, min_h: h, max_h: h }
    }

    pub const fn loose(max_w: u32, max_h: u32) -> Self {
        Self { min_w: 0, max_w, min_h: 0, max_h }
    }

    pub const fn infinite() -> Self {
        Self { min_w: 0, max_w: u32::MAX, min_h: 0, max_h: u32::MAX }
    }

    pub const fn min(&self) -> (u32, u32) {
        (self.min_w, self.min_h)
    }

    pub const fn max(&self) -> (u32, u32) {
        (self.max_w, self.max_h)
    }

    pub const fn is_tight(&self) -> bool {
        self.min_w == self.max_w && self.min_h == self.max_h
    }

    pub const fn is_infinite(&self) -> bool {
        self.max_w == u32::MAX && self.max_h == u32::MAX
    }

    /// sizes up：把 intrinsic extent 独立夹入四元组。
    pub const fn fit(&self, w: u32, h: u32) -> (u32, u32) {
        (
            if w < self.min_w {
                self.min_w
            } else if w > self.max_w {
                self.max_w
            } else {
                w
            },
            if h < self.min_h {
                self.min_h
            } else if h > self.max_h {
                self.max_h
            } else {
                h
            },
        )
    }

    /// constraints down：扣除 padding/gap/固定占位。min 下折 0，max 饱和减。
    pub const fn deflate(&self, w: u32, h: u32) -> Self {
        Self {
            min_w: self.min_w.saturating_sub(w),
            max_w: self.max_w.saturating_sub(w),
            min_h: self.min_h.saturating_sub(h),
            max_h: self.max_h.saturating_sub(h),
        }
    }

    /// 两盒相交；空盒返 None。
    pub fn intersect(&self, other: Self) -> Option<Self> {
        let min_w = self.min_w.max(other.min_w);
        let max_w = self.max_w.min(other.max_w);
        let min_h = self.min_h.max(other.min_h);
        let max_h = self.max_h.min(other.max_h);
        (min_w <= max_w && min_h <= max_h).then(|| Self { min_w, max_w, min_h, max_h })
    }
}

impl Constraints {
    /// L1 四档 → L4 四元组构造糖；damage clip 仍单独持真实 viewport。
    pub(crate) fn to_box(self, vp_w: u32, vp_h: u32) -> BoxConstraints {
        match self {
            Self::Viewport => BoxConstraints::tight(vp_w, vp_h),
            Self::Tight { w, h } => BoxConstraints::tight(w, h),
            Self::Loose { w, h } => BoxConstraints::loose(w, h),
            Self::Infinite => BoxConstraints::infinite(),
        }
    }
}

/// 走查/位图容量：`2 * MAX_NODES` 常量表达式（D10 单源——MAX_NODES=512
/// 契约的两倍余量）；超帽（病态巨 arena）退化全量保守重排（见 relayout
/// 头注）。
pub(crate) const CAP: usize = 2 * crate::MAX_NODES;

/// damage 追加收口单源（GUI-6 D17：layout / lib::damage_rect / anim 三
/// 调用点同语义——先 clip viewport，负界/越界折界内残片；零面积丢弃）。
pub(crate) fn dmg_clip_add(damage: &mut DamageSet, r: &RectI, vp: &RectI) {
    let c = r.intersect(vp);
    if !c.is_empty() {
        damage.add(c.to_wire());
    }
}

/// 布局主入口（§6 冻结签名 + GUI-6 扩展三参）：脏子树重排、新界进
/// damage。
///
/// `freed` = 当前空闲槽清单（§6 签名面）——树走查自根、空闲槽已
/// `None` 天然跳过，此处不消费（removal 面的旧界 damage 在 Ui::remove
/// 侧先行入账）。`tm`/`icons` = D28 供给（Text 臂经 `tm.measure` 走缝
/// ——Monospace 逐位复刻现状；icons 段C Image/Icon 叶 dims 消费，本段
/// 占位）；`cs` = 根播种约束（L1 四档全实现——仅影响 Root intrinsic、
/// Root avail 与终值；damage clip 恒真实 viewport）。
pub(crate) fn relayout(
    nodes: &mut [Option<Node>],
    vp_w: u32,
    vp_h: u32,
    damage: &mut DamageSet,
    freed: &[u32],
    tm: &dyn TextMetrics,
    icons: &dyn IconSource,
    cs: BoxConstraints,
    flex: &[Option<FlexEntry>],
    wrap: &mut [Option<crate::TextWrapEntry>],
) -> LayoutStat {
    let _ = freed;
    // 根 = 唯一 parent==NONE 的存活 Root
    let mut root = u32::MAX;
    for (i, o) in nodes.iter().enumerate() {
        if let Some(n) = o {
            if n.alive && n.kind == ViewKind::Root && n.parent == u32::MAX {
                root = i as u32;
                break;
            }
        }
    }
    if root == u32::MAX {
        return LayoutStat { measured: 0, placed: 0 };
    }
    // 无脏位 → 全缓存命中（Measure/Render 保留——改色面 measured==0）
    if !nodes.iter().any(|o| matches!(o, Some(n) if n.alive && n.layout_dirty)) {
        return LayoutStat { measured: 0, placed: 0 };
    }
    let oversize = nodes.len() > CAP;

    // ① 测量（后序）：脏位/受影响链重测 + 清 layout_dirty + 位图落账
    let mut mctx = MeasCtx {
        recomputed: [false; CAP],
        changed: [false; CAP],
        measured: 0,
    };
    measure(nodes, root, vp_w, vp_h, cs, wrap, tm, icons, &mut mctx, oversize, 0);
    if oversize {
        // 病态巨 arena：位图覆盖不住的槽全部按「已重测」保守处理
        for b in mctx.recomputed.iter_mut() {
            *b = true;
        }
    }

    // ② 旧视觉界快照（摆放改写前——绝对链含 ox/oy）
    let mut old_abs: [Option<RectI>; CAP] = [None; CAP];
    if !oversize {
        snapshot(nodes, root, 0, 0, &mut old_abs, 0);
    }

    // ③ 摆放（前序）：force ∨ 自身重测 → 重摆 + 新旧界 damage
    let vp_rect = RectI::new(0, 0, u32_to_i32_sat(vp_w), u32_to_i32_sat(vp_h));
    // Root 播种盒：四元组已在 intrinsic Root 臂 fit；Tight/min=max
    // 自然覆盖子包络，Loose/Infinite 取 clamp 后包络。meas==placed 同源。
    let (root_w, root_h) = nodes[root as usize]
        .as_ref()
        .map(|n| (n.meas_w, n.meas_h))
        .unwrap_or((0, 0));
    let mut placed = 0u32;
    place(
        nodes,
        root,
        0,
        0,
        0,
        0,
        u32_to_i32_sat(root_w),
        u32_to_i32_sat(root_h),
        false,
        flex,
        &mctx.recomputed,
        &old_abs,
        &vp_rect,
        damage,
        &mut placed,
        0,
    );
    LayoutStat { measured: mctx.measured, placed }
}

// ---------- 测量（后序 + 变更传播） ----------

struct MeasCtx {
    recomputed: [bool; CAP],
    changed: [bool; CAP],
    measured: u32,
}

/// 后序测量：节点重测 iff（layout_dirty ∨ 任一子受影响 ∨ oversize）。
/// 返回「本子树受影响」——祖先聚合链据此重测。layout_dirty 在重测时
/// 清位（消费语义——下次 relayout 不再重复计入）。
fn measure(
    nodes: &mut [Option<Node>],
    slot: u32,
    vp_w: u32,
    vp_h: u32,
    cs: BoxConstraints,
    wrap: &mut [Option<crate::TextWrapEntry>],
    tm: &dyn TextMetrics,
    icons: &dyn IconSource,
    mctx: &mut MeasCtx,
    oversize: bool,
    hops: u32,
) -> bool {
    if hops as usize > nodes.len() {
        return false; // 父/子链环防御
    }
    let idx = slot as usize;
    let Some(node) = nodes.get(idx).and_then(|o| o.as_ref()) else { return false };
    if !node.alive {
        return false;
    }
    // Node/Props 均 Copy；先复制父投影所需值，结束 immutable arena borrow
    // 后才能递归传 `&mut nodes`。
    let n = *node;
    // 子先（后序）——链走查带环防御
    let mut child_affected = false;
    let mut child_count = 0usize;
    let mut count_cur = n.first_child;
    while count_cur != u32::MAX && child_count <= nodes.len() {
        let Some(c) = nodes.get(count_cur as usize).and_then(|o| o.as_ref()) else { break };
        if !c.alive {
            break;
        }
        child_count += 1;
        count_cur = c.next_sib;
    }
    let mut child_index = 0usize;
    let mut cur = n.first_child;
    let mut guard = 0usize;
    while cur != u32::MAX && guard <= nodes.len() {
        guard += 1;
        let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
        let next = c.next_sib;
        let child_box = child_measure_box(&n, cs, child_index, child_count);
        child_index += 1;
        child_affected |= measure(
            nodes,
            cur,
            vp_w,
            vp_h,
            child_box,
            wrap,
            tm,
            icons,
            mctx,
            oversize,
            hops + 1,
        );
        cur = next;
    }
    let dirty = nodes[idx].as_ref().map(|n| n.layout_dirty).unwrap_or(false);
    if !dirty && !child_affected && !oversize {
        return false; // 全缓存命中
    }
    let (w, h) = intrinsic(nodes, idx, vp_w, vp_h, cs, wrap, tm, icons);
    let n = nodes[idx].as_mut().unwrap();
    let changed = n.meas_w != w || n.meas_h != h;
    n.meas_w = w;
    n.meas_h = h;
    n.layout_dirty = false;
    if idx < CAP {
        mctx.recomputed[idx] = true;
        mctx.changed[idx] = changed;
    }
    mctx.measured += 1;
    // D4 截断缝（S7 段E——B 面 cfg s7）：返 `changed`——内在不变的
    // 受影响祖先上行截断（不重测/不重摆）；A 面恒 true（现状逐字保持
    // ——受影响链全量聚合，诚实基线）。oversize 路恒 true（位图盖不
    // 住的槽保守视为受影响——文档化口径，两面同）。
    #[cfg(feature = "s7")]
    {
        oversize || changed
    }
    #[cfg(not(feature = "s7"))]
    {
        true
    }
}

/// Root 子包络（stack 语义：宽/高各取 max，非 Col 的 Σ 高）。Loose/
/// Infinite 的 shrink-wrap 基准；Stretch 型子（None/None）内在为 0 是
/// v1 约束盲 measure 的已文档 wart——调用方需 HAS_SIZE 或内在叶。
fn child_extent(nodes: &[Option<Node>], first: u32) -> (u32, u32) {
    let mut w = 0u32;
    let mut h = 0u32;
    let mut cur = first;
    let mut guard = 0usize;
    while cur != u32::MAX && guard <= nodes.len() {
        guard += 1;
        let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
        if !c.alive {
            break;
        }
        w = w.max(c.meas_w);
        h = h.max(c.meas_h);
        cur = c.next_sib;
    }
    (w, h)
}

/// 内在尺寸（读子 meas——后序保证已新鲜）。Text 臂经 `tm.measure` 走
/// 供给缝（D8：scale id 与 desc wire 域同序——Monospace 供件逐位复刻
/// `len × cell_w` 现状，换件面 = GUI-2 增件⑧）；Image/Icon 叶经
/// `icons.dims`（§4.3）。Col/Row/Grid 的 w/h Some 折入内在（§4.1 定尺寸
/// 覆盖一切——父游标推进/聚合/Start·Center 自身_extent 单源用 meas）。
/// LAYOUT-L5 constraints down：父容器把自身 incoming box 投影给子节点。
/// 本刀只有 wrap Text 消费 `max_w`；无 wrap 树的测量结果与 L4 逐字节
/// 等价。固定 w/h 遵循“定尺寸覆盖一切”，先覆盖再扣 padding/gap。
pub(crate) fn child_measure_box(
    parent: &Node,
    incoming: BoxConstraints,
    child_index: usize,
    child_count: usize,
) -> BoxConstraints {
    match &parent.props {
        Props::Root => BoxConstraints::loose(incoming.max_w, incoming.max_h),
        Props::Pad(p) => incoming.deflate(
            p.all.saturating_mul(2),
            p.all.saturating_mul(2),
        ),
        Props::Panel(p) => {
            let inset = p.padding.saturating_mul(2);
            let max_w = p.w.map(|w| w.saturating_sub(inset)).unwrap_or(incoming.max_w.saturating_sub(inset));
            let max_h = p.h.map(|h| h.saturating_sub(inset)).unwrap_or(incoming.max_h.saturating_sub(inset));
            BoxConstraints::loose(max_w, max_h)
        }
        Props::Col(p) => BoxConstraints::loose(p.w.unwrap_or(incoming.max_w), u32::MAX),
        Props::Row(p) => BoxConstraints::loose(u32::MAX, p.h.unwrap_or(incoming.max_h)),
        Props::Grid(p) => {
            // 子盒 = 单个 track，而非整格容器扣 gap。place 面行主序等分
            // 轨道；measure 面必须同口径，否则 wrap Text 会跨格溢出。
            let cols = u32::from(p.cols.max(1));
            let rows_needed = ((child_count as u32).saturating_add(cols - 1)) / cols;
            let rows = rows_needed.max(1);
            let gw = p.gap.saturating_mul(cols - 1);
            let gh = p.gap.saturating_mul(rows - 1);
            let content_w = p.w.unwrap_or(incoming.max_w).saturating_sub(gw);
            let content_h = p.h.unwrap_or(incoming.max_h).saturating_sub(gh);
            let _ = child_index;
            BoxConstraints::loose(content_w / cols, content_h / rows)
        }
        Props::Clip(p) => BoxConstraints::loose(
            p.w.unwrap_or(incoming.max_w),
            p.h.unwrap_or(incoming.max_h),
        ),
        // 叶子无子；保守回传 loose incoming。
        _ => BoxConstraints::loose(incoming.max_w, incoming.max_h),
    }
}

fn intrinsic(
    nodes: &[Option<Node>],
    idx: usize,
    vp_w: u32,
    vp_h: u32,
    cs: BoxConstraints,
    wrap: &mut [Option<crate::TextWrapEntry>],
    tm: &dyn TextMetrics,
    icons: &dyn IconSource,
) -> (u32, u32) {
    let _ = (vp_w, vp_h);
    let n = match nodes[idx].as_ref() {
        Some(n) => n,
        None => return (0, 0),
    };
    match &n.props {
        Props::Root => {
            let (ew, eh) = child_extent(nodes, n.first_child);
            cs.fit(ew, eh)
        }
        // Clip 内在尺寸随子内容（作为 Col/Row 子时的主轴请求）；终值
        // 仍吃父 avail，port = 自身布局 bounds。
        Props::Clip(p) => {
            let (cw, ch, _) = child_agg(nodes, n.first_child, true);
            let w = p.w.map(|v| v as u64).unwrap_or(cw);
            let h = p.h.map(|v| v as u64).unwrap_or(ch);
            (u64_to_u32_sat(w), u64_to_u32_sat(h))
        }
        Props::Rect(p) => (p.w.unwrap_or(0), p.h.unwrap_or(0)),
        Props::Circle(p) => (p.w.unwrap_or(0), p.h.unwrap_or(0)),
        Props::Text(p) => {
            let id = crate::desc::NodeId(((n.gen as u32) << 16) | (idx as u32 & 0xFFFF));
            let enabled = wrap.iter().flatten().any(|e| e.node == id && e.enabled);
            let scale = crate::desc::scale_to_u8(p.style.scale);
            if enabled {
                let measured = tm.measure_with_box(p.text.as_bytes(), scale, cs.max_w);
                if let Some(entry) = wrap.iter_mut().flatten().find(|e| e.node == id) {
                    entry.last_max_w = cs.max_w;
                }
                measured
            } else {
                tm.measure(p.text.as_bytes(), scale)
            }
        }
        Props::Col(p) => {
            let (w, mut h, k) = child_agg(nodes, n.first_child, true);
            h = h.saturating_add(p.gap as u64 * k.saturating_sub(1).max(0) as u64);
            let w = p.w.map(|v| v as u64).unwrap_or(w);
            let h = p.h.map(|v| v as u64).unwrap_or(h);
            (u64_to_u32_sat(w), u64_to_u32_sat(h))
        }
        Props::Row(p) => {
            let (mut w, h, k) = child_agg(nodes, n.first_child, false);
            w = w.saturating_add(p.gap as u64 * k.saturating_sub(1).max(0) as u64);
            let w = p.w.map(|v| v as u64).unwrap_or(w);
            let h = p.h.map(|v| v as u64).unwrap_or(h);
            (u64_to_u32_sat(w), u64_to_u32_sat(h))
        }
        Props::Pad(p) => {
            let (mut w, mut h, _) = child_agg(nodes, n.first_child, true);
            let both = 2u64.saturating_mul(p.all as u64);
            w = w.saturating_add(both);
            h = h.saturating_add(both);
            (u64_to_u32_sat(w), u64_to_u32_sat(h))
        }
        Props::Grid(p) => {
            // §4.2 measure：行主序子布格——行高/列宽各取格内 max；行数
            // 按子实占 rows_needed = ceil(k/cols)（声明 rows 是 add 面帽，
            // 空行不占内在）、列向取已占列；行间/列间 gap。w/h Some 覆盖
            //（容器通则）。0 维防御折 1（AST/add/set_props 三面已 loud 拒
            // ——release 保守不断轨）。
            let cols = (p.cols as usize).max(1);
            let rows_cap = (p.rows as usize).max(1);
            let mut col_w = [0u32; 256];
            let mut row_h = [0u32; 256];
            let mut k = 0usize;
            let mut cur = n.first_child;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                if !c.alive {
                    break;
                }
                let next = c.next_sib;
                if k < cols * rows_cap {
                    let col = k % cols;
                    let row = k / cols;
                    col_w[col] = col_w[col].max(c.meas_w);
                    row_h[row] = row_h[row].max(c.meas_h);
                }
                k += 1;
                cur = next;
            }
            debug_assert!(
                k <= cols * rows_cap,
                "Grid 子数超格（add/AST/set_props 三面已拒——arena 漂移）"
            );
            // 行数帽 256 = row_h 面（防御：超格子只算前 256 行——release
            // 保守不断轨，debug 上面断言已 loud）
            let rows_needed = ((k + cols - 1) / cols).min(row_h.len());
            let used_cols = cols.min(k);
            let g = p.gap as u64;
            let w: u64 = col_w[..used_cols].iter().map(|v| *v as u64).sum::<u64>()
                .saturating_add(g * used_cols.saturating_sub(1) as u64);
            let h: u64 = row_h[..rows_needed].iter().map(|v| *v as u64).sum::<u64>()
                .saturating_add(g * rows_needed.saturating_sub(1) as u64);
            let w = p.w.map(|v| v as u64).unwrap_or(w);
            let h = p.h.map(|v| v as u64).unwrap_or(h);
            (u64_to_u32_sat(w), u64_to_u32_sat(h))
        }
        Props::Panel(p) => {
            let (mut w, mut h, k) = child_agg(nodes, n.first_child, true);
            w = w.saturating_add(2u64 * p.padding as u64);
            h = h.saturating_add(2u64 * p.padding as u64)
                .saturating_add(p.gap as u64 * k.saturating_sub(1).max(0) as u64);
            let w = p.w.map(|v| v as u64).unwrap_or(w);
            let h = p.h.map(|v| v as u64).unwrap_or(h);
            (u64_to_u32_sat(w), u64_to_u32_sat(h))
        }
        Props::Image(p) => icon_dims(icons, p.res, p.w, p.h),
        Props::Icon(p) => icon_dims(icons, p.id, p.w, p.h),
        // 组件七件的 Phase 2 首批（Button/Toggle/Slider/Progress——本刀）：
        // 内在型叶（Text 同列），固定值起步（几何常量单源 crate::——
        // lib.rs「组件四件 v1 几何常量」块）。Button 宽 = 标签字节长 ×
        // 8（S8x16 档 cell_w——与 paint 渲染推进同值）+ 32 水平留白、下
        // 限 64、恒高 48（触摸最小可点区）；Toggle 52×32 标准开关；
        // Slider 200×44 最小可拖区；Progress 200×8 线性进度条。
        Props::Button(p) => {
            let w = (p.label.len as u64 * crate::BUTTON_CHAR_W as u64)
                .saturating_add(crate::BUTTON_PAD_X as u64)
                .max(crate::BUTTON_MIN_W as u64);
            (u64_to_u32_sat(w), crate::BUTTON_H)
        }
        Props::Toggle(_) => (crate::TOGGLE_W, crate::TOGGLE_H),
        Props::Slider(_) => (crate::SLIDER_W, crate::SLIDER_H),
        Props::Progress(_) => (crate::PROGRESS_W, crate::PROGRESS_H),
        // 波2 批 A（Tabs/Sbar/Badge/Scroll/List）：Tabs 内在 = 页签条高
        // 48 + 子聚合 max（子 = 内容区面板）；Sbar/Badge = 固定内在叶
        //（Badge count=0 且无圆点 = 隐藏 → 0×0 空绘）；Scroll/List =
        // 容器（Scroll = 子聚合宽 × content_height；List = 子聚合宽 ×
        // item_count×item_h——等高项 v1）。Tabs 子高取 **max** 不取 Σ
        //（place 面 z 叠放——Stack 同形；Σ 是批 A 半成品错源，多面板
        // 各异高时会虚高内容区）。
        Props::Tabs(_) => {
            let (cw, _, _) = child_agg(nodes, n.first_child, true); // 宽取子 max
            let (_, ch, _) = child_agg(nodes, n.first_child, false); // 高取子 max（z 叠放）
            (
                u64_to_u32_sat(cw.max(crate::TABS_MIN_W as u64)),
                u64_to_u32_sat(ch.saturating_add(crate::TABS_BAR_H as u64)),
            )
        }
        Props::Sbar(p) => match p.orientation {
            // 第三批定稿：内在随走向互转（竖 4×200 / 横 200×4——批 A 的
            // 横定臂是半成品遗留，paint 两态渲染已在位）。
            Orientation::Vertical => (crate::SBAR_THICK, crate::SBAR_LONG),
            Orientation::Horizontal => (crate::SBAR_LONG, crate::SBAR_THICK),
        },
        Props::Badge(p) => {
            if p.show_dot {
                (crate::BADGE_DOT_D, crate::BADGE_DOT_D)
            } else if p.count == 0 {
                (0, 0) // count=0 隐藏（desc.rs 语义）
            } else {
                // 数字角标随文本（第三批定稿）：宽 = 位档×8 + 8 留白
                //（1/2/3 位——>99 封顶显 99+ 是 paint 侧文案，位档此处
                // 单源定宽；批 A 的固宽 32 是半成品遗留）。
                let chars: u64 = if p.count > 99 { 3 } else if p.count >= 10 { 2 } else { 1 };
                (
                    u64_to_u32_sat(
                        (chars * crate::BUTTON_CHAR_W as u64)
                            .saturating_add(crate::BADGE_PAD_X as u64),
                    ),
                    crate::BADGE_DOT_D,
                )
            }
        }
        Props::Select(_) => (crate::SELECT_W, crate::BUTTON_H),
        // 波2 批 D：Input 200×48 单行框；Edit 200×96 三行档；Canvas
        // 200×200 占位面（自绘协议挂后续刀——内在先定框）。
        Props::Input(_) => (crate::INPUT_W, crate::INPUT_H),
        Props::Edit(_) => (crate::EDIT_W, crate::EDIT_H),
        Props::Canvas(_) => (crate::CANVAS_D, crate::CANVAS_D),
        // 波2 批 E：DatePick/TimePick 固定内在（日期 96×48 / 时间 64×48
        // ——数字定宽显示）；Refresh/SwipeItem = 容器（内在随子聚合）。
        Props::Refresh(_) => {
            let (cw, ch, _) = child_agg(nodes, n.first_child, true);
            (u64_to_u32_sat(cw), u64_to_u32_sat(ch))
        }
        Props::DatePick(_) => (96, 48),
        Props::TimePick(_) => (64, 48),
        Props::Scroll(p) => {
            let (cw, _, _) = child_agg(nodes, n.first_child, true);
            (u64_to_u32_sat(cw), p.content_height)
        }
        Props::List(p) => {
            let (cw, _, _) = child_agg(nodes, n.first_child, true);
            let h = (p.item_count as u64).saturating_mul(p.item_height as u64);
            (u64_to_u32_sat(cw), u64_to_u32_sat(h))
        }
        // 组件七件第二批之交互四件（Check/Radio/Step/Seg——本刀）：内
        // 在型叶（首批四件同列），固定值起步（几何常量单源 crate::——
        // lib.rs「组件七件 v1 几何常量」块）。Check/Radio 24×24 标准
        // 可点方框/圆、Step 120×36 三段（左减号+值+右加号）、Seg =
        // 段数×80 宽 36 高（u64 累计防段数×段宽溢出）。
        Props::Check(_) => (crate::CHECK_W, crate::CHECK_H),
        Props::Radio(_) => (crate::RADIO_W, crate::RADIO_H),
        Props::Step(_) => (crate::STEP_W, crate::STEP_H),
        Props::Seg(p) => {
            let w = p.segment_count as u64 * crate::SEG_SEG_W as u64;
            (u64_to_u32_sat(w), crate::SEG_H)
        }
        // 组件七件第二批之布局三件（Stack/Spacer/Divider——本刀）：
        // Stack = 子聚合 **max×max**（z 序叠层——与 Col/Row 的 Σ 聚合
        // 不同源：子全幅叠放时容器内在取最大子尺寸）；Spacer = 0×0 最
        // 小占位（flex 权重分配挂后续布局刀——final_size 吃满 avail 已
        // 合义）；Divider = 宽 0 高 thickness（宽随父——final_size 臂）。
        Props::Stack(_) => {
            let (mut w, mut h) = (0u32, 0u32);
            let mut cur = n.first_child;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                if !c.alive {
                    break;
                }
                let next = c.next_sib;
                w = w.max(c.meas_w);
                h = h.max(c.meas_h);
                cur = next;
            }
            (w, h)
        }
        Props::Spacer(_) => (0, 0),
        Props::Divider(p) => (0, p.thickness),
        // 组件第三批七件（Badge/Sbar 已上批 A 块定稿；余五件——本刀）：
        // Tip = 内在型叶（文本同列——len×8 宽 ×16 高，S8x16 与
        // BUTTON_CHAR_W 单源）；Collapse = 容器（header 32 恒基 +
        // expanded 时加子 Σ 高——收起折叠，宽恒子 max）；Chip = 内在型
        // 叶（标签×8 + 16 留白 ×32 高）；Fab = 内在型叶（空标签 =
        // 56 圆形，扩展 = 56 + 8 间隙 + 标签×8）；Card = 容器（子聚合
        // max×max——Stack 同形，final 随父填充走 `_` 摆位）。
        Props::Tip(p) => (
            u64_to_u32_sat(p.text.len as u64 * crate::BUTTON_CHAR_W as u64),
            crate::TIP_H,
        ),
        Props::Collapse(p) => {
            let (w, ch, _) = child_agg(nodes, n.first_child, true);
            let ch = if p.expanded { ch } else { 0 };
            (
                u64_to_u32_sat(w),
                u64_to_u32_sat(ch.saturating_add(crate::COLLAPSE_HEADER_H as u64)),
            )
        }
        Props::Chip(p) => (
            u64_to_u32_sat(
                (p.label.len as u64 * crate::BUTTON_CHAR_W as u64)
                    .saturating_add(crate::CHIP_PAD_X as u64),
            ),
            crate::CHIP_H,
        ),
        Props::Fab(p) => {
            if p.extended_label.is_empty() {
                (crate::FAB_D, crate::FAB_D)
            } else {
                (
                    u64_to_u32_sat(
                        (crate::FAB_D as u64)
                            .saturating_add(crate::FAB_LABEL_GAP as u64)
                            .saturating_add(p.extended_label.len as u64 * crate::BUTTON_CHAR_W as u64),
                    ),
                    crate::FAB_D,
                )
            }
        }
        Props::Card(_) => {
            let (mut w, mut h) = (0u32, 0u32);
            let mut cur = n.first_child;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                if !c.alive {
                    break;
                }
                let next = c.next_sib;
                w = w.max(c.meas_w);
                h = h.max(c.meas_h);
                cur = next;
            }
            (w, h)
        }
        // P0 尾（批 C/D 已迁出单列——此组清空防 unreachable）
        // 批 F（末刀三件）：Gauge = 表盘方格 64×64（值条 + 值文本两带）；
        // Qr = module_count×4 点阵方阵（u64 累计防大版本溢出——21 模块
        // = 84×84、177 模块 = 708×708）；DragReorder = 手柄列 24×36。
        Props::Gauge(_) => (crate::GAUGE_D, crate::GAUGE_D),
        Props::Qr(p) => {
            let s = (p.module_count as u64).saturating_mul(crate::QR_MODULE_PX as u64);
            (u64_to_u32_sat(s), u64_to_u32_sat(s))
        }
        Props::DragReorder(_) => (crate::DRAG_W, crate::DRAG_H),
        // P1 尾（占位保持——末件）：SwipeItem 内在应为行高，挂后续布
        // 局刀，本层 0×0 占位（批 E 视觉已接线；随父填充由 final_size
        // 占位臂合义）。
        Props::SwipeItem(_) => (0, 0),
    }
}

/// Image/Icon 叶内在尺寸（§4.3）：w/h Some 定值；None → `dims(id)` 内在；
/// 未注册 → (0,0)。loud 分档（§11-14 两面兼顾）：默认空供件（guest/
/// host 未注册面——`is_empty`）静默 (0,0) 空绘不炸；真供件缺注册 =
/// debug_assert loud（注册 bug 不容静默吞）。
fn icon_dims(
    icons: &dyn IconSource,
    id: u16,
    w: Option<u32>,
    h: Option<u32>,
) -> (u32, u32) {
    let d = icons.dims(id);
    if d.is_none() && !icons.is_empty() {
        debug_assert!(false, "IconSource 真供件缺注册 id={id}（dims None——注册 bug）");
    }
    let (dw, dh) = d.unwrap_or((0, 0));
    (w.unwrap_or(dw), h.unwrap_or(dh))
}

/// 子聚合：`vertical` 真 = Col/Pad 视角（w 取 max、h 取 Σ）；假 = Row
/// 视角（w 取 Σ、h 取 max）。返回 (w, h, 子数)——u64 累计。
fn child_agg(nodes: &[Option<Node>], first: u32, vertical: bool) -> (u64, u64, u64) {
    let mut w: u64 = 0;
    let mut h: u64 = 0;
    let mut k: u64 = 0;
    let mut cur = first;
    let mut guard = 0usize;
    while cur != u32::MAX && guard <= nodes.len() {
        guard += 1;
        let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
        if !c.alive {
            break;
        }
        let next = c.next_sib;
        if vertical {
            w = w.max(c.meas_w as u64);
            h = h.saturating_add(c.meas_h as u64);
        } else {
            w = w.saturating_add(c.meas_w as u64);
            h = h.max(c.meas_h as u64);
        }
        k += 1;
        cur = next;
    }
    (w, h, k)
}

// ---------- 旧界快照 ----------

/// 摆放改写前的视觉绝对界快照（链含 x/y 与 ox/oy——与 anim/hit 同口
/// 径）；仅记 slot < CAP 的槽（oversize 面由 place 侧退化跳过）。
fn snapshot(
    nodes: &[Option<Node>],
    slot: u32,
    ex: i64,
    ey: i64,
    old: &mut [Option<RectI>; CAP],
    hops: u32,
) {
    if hops as usize > nodes.len() {
        return;
    }
    let Some(n) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else { return };
    if !n.alive {
        return;
    }
    let bx = ex + n.x as i64 + n.ox as i64;
    let by = ey + n.y as i64 + n.oy as i64;
    let idx = slot as usize;
    if idx < CAP {
        old[idx] = Some(RectI::new(i64_to_i32_sat(bx), i64_to_i32_sat(by), n.w, n.h));
    }
    let mut cur = n.first_child;
    let mut guard = 0usize;
    while cur != u32::MAX && guard <= nodes.len() {
        guard += 1;
        let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
        let next = c.next_sib;
        snapshot(nodes, cur, bx, by, old, hops + 1);
        cur = next;
    }
}

// ---------- 摆放（前序） ----------

/// 节点终值尺寸（给定容器分配 avail）：十字轴随父、主轴内在——
/// Rect None 随父、Text 内在、Col/Row 主轴内在十字轴吃满、Pad/Root
/// 整体吃满分配；Panel 宽随父、高随内容，定值覆盖 avail。段C
/// （§4.1/§4.3）：Col/Row/Grid 的 HAS_SIZE 定尺寸
/// 优先于 avail（定尺寸覆盖一切）；Grid = 随父型子（Col 同形——十字
/// 吃 avail、纵轴内在）；Image/Icon = 内在型叶恒内在（Some 定值 /
/// None = dims 折入 meas），**不吃 avail**（§4.1 豁免行——十字轴拉
/// 伸只及随父型子）。
fn final_size(n: &Node, avail_w: i32, avail_h: i32) -> (i32, i32) {
    match &n.props {
        Props::Root => (avail_w, avail_h),
        Props::Clip(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(avail_w),
            p.h.map(u32_to_i32_sat).unwrap_or(avail_h),
        ),
        Props::Rect(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(avail_w),
            p.h.map(u32_to_i32_sat).unwrap_or(avail_h),
        ),
        Props::Circle(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(avail_w),
            p.h.map(u32_to_i32_sat).unwrap_or(avail_h),
        ),
        Props::Text(_) => (u32_to_i32_sat(n.meas_w), u32_to_i32_sat(n.meas_h)),
        Props::Col(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(avail_w),
            p.h.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_h)),
        ),
        Props::Row(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_w)),
            p.h.map(u32_to_i32_sat).unwrap_or(avail_h),
        ),
        Props::Pad(_) => (avail_w, avail_h),
        Props::Grid(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(avail_w),
            p.h.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_h)),
        ),
        Props::Panel(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(avail_w),
            p.h.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_h)),
        ),
        Props::Image(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_w)),
            p.h.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_h)),
        ),
        Props::Icon(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_w)),
            p.h.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_h)),
        ),
        // 组件件（Phase 2 两批）：交互八件 = 内在型叶（Text 同列——恒
        // 内在 meas_w/h，**不吃 avail**，§4.1 豁免行同律；十字轴拉伸只
        // 及随父型子）。内在值单源 = intrinsic 各臂（固定几何起步）；
        // 定尺寸面（SIZE 块）§8.2 表未扩，组件拒块——无双源。
        Props::Button(_)
        | Props::Toggle(_)
        | Props::Slider(_)
        | Props::Progress(_)
        | Props::Check(_)
        | Props::Radio(_)
        | Props::Step(_)
        | Props::Seg(_) => (
            u32_to_i32_sat(n.meas_w),
            u32_to_i32_sat(n.meas_h),
        ),
        // Divider = 横线半随父型：高恒内在（thickness），宽吃 avail
        //（随父——Col/Row/Stack 里随容器宽拉通，intrinsic 宽 0 不占聚
        // 合）。
        Props::Divider(_) => (avail_w, u32_to_i32_sat(n.meas_h)),
        // 波2 批 A：Tabs/Sbar/Badge = 内在型叶（Text 同列——恒内在，
        // 不吃 avail）；Scroll/List = 容器随父填充（内在已上表）。SIZE
        // 块同拒。
        Props::Tabs(_)
        | Props::Sbar(_)
        | Props::Badge(_)
        // 组件第三批：Tip/Chip/Fab = 内在型叶同列（Badge/Sbar 批 A 已
        // 入；SIZE 块同拒）。
        | Props::Tip(_)
        | Props::Chip(_)
        | Props::Fab(_)
        | Props::Select(_)
        | Props::Input(_)
        | Props::Edit(_)
        | Props::Canvas(_)
        | Props::DatePick(_)
        | Props::TimePick(_) => (u32_to_i32_sat(n.meas_w), u32_to_i32_sat(n.meas_h)),
        // Collapse = 纵向容器（Col 同形）：十字轴吃 avail、主轴内在
        //（收起折叠至 header 高——expanded 几何位单源 intrinsic）。
        Props::Collapse(_) => (avail_w, u32_to_i32_sat(n.meas_h)),
        // 组件件占位余员（占位同前）：随父填充；Stack 的全幅叠放子分
        // 配由 `_` 臂天然合义（z 序叠层 = Root 型全幅）；Spacer 吃满
        // avail（flex 权重分配挂后续布局刀）。SIZE 块同拒。
        Props::Scroll(_)
        | Props::List(_)
        | Props::Stack(_)
        | Props::Spacer(_)
        // P0 尾三件批 D 已迁内在型叶（Input/Edit/Canvas——上表单列）；
        // 余件占位随父填充。SIZE 块同拒（§8.2 表未随本批扩——无双源）。
        // P1 尾五件（占位同前三批）：随父填充；Refresh 的子分配由 `_`
        // 臂天然合义（包裹容器 = Root 型全幅给子）；SIZE 块同拒。
        | Props::Refresh(_)
        | Props::SwipeItem(_)
        // 批 E：DatePick/TimePick 迁内在型叶（上表）；P2 余件（占位同
        // 前四批）：随父填充；Card 的子分配由 `_` 臂天然合义（包裹容器
        // = Root 型全幅给子——intrinsic 已上表取子 max）；SIZE 块同拒
        //（§8.2 表未随本批扩——无双源）。
        | Props::Card(_) => (avail_w, avail_h),
        // 批 F（末刀）：Gauge/Qr/DragReorder 迁内在型叶（Text 同列——
        // 恒内在不吃 avail；内在单源 = intrinsic 各臂，SIZE 块同拒）。
        Props::Gauge(_)
        | Props::Qr(_)
        | Props::DragReorder(_) => (u32_to_i32_sat(n.meas_w), u32_to_i32_sat(n.meas_h)),
    }
}

/// L3 orientation-aware eligible 面：父 Row 消费水平主轴、父 Col 消费
/// 纵主轴。叶子维持 L2 双 None 契约；容器 pane 只开放“transposed主轴
/// None”形态——Row 中 Col/Grid/Panel、Col 中 Row。同向嵌套的主轴是
/// intrinsic，不入slot，写/读侧同源拒。
pub(crate) fn flex_eligible_props(
    nodes: &[Option<Node>],
    idx: usize,
    props: &Props,
) -> bool {
    let Some(child) = nodes.get(idx).and_then(|o| o.as_ref()) else {
        return false;
    };
    let parent_kind = nodes.get(child.parent as usize)
        .and_then(|o| o.as_ref())
        .map(|p| p.kind);
    let horizontal = matches!(parent_kind, Some(ViewKind::Row));
    let vertical = matches!(parent_kind, Some(ViewKind::Col));
    let parent_ok = horizontal || vertical;
    // kind+props 成对匹配：防 builder 误配（Row kind + Rect props 等）
    // 进入 flex 读侧。
    match (child.kind, props) {
        (ViewKind::Rect, Props::Rect(p))
            if parent_ok && p.w.is_none() && p.h.is_none() => true,
        (ViewKind::Circle, Props::Circle(p))
            if parent_ok && p.w.is_none() && p.h.is_none() => true,
        (ViewKind::Col, Props::Col(p)) if horizontal && p.w.is_none() => true,
        (ViewKind::Grid, Props::Grid(p)) if horizontal && p.w.is_none() => true,
        (ViewKind::Panel, Props::Panel(p)) if horizontal && p.w.is_none() => true,
        (ViewKind::Row, Props::Row(p)) if vertical && p.h.is_none() => true,
        _ => false,
    }
}

/// 活 flex spec（slot/gen/alive/eligible 四重对拍；残项不参与）。
fn flex_spec_of(
    nodes: &[Option<Node>],
    flex: &[Option<FlexEntry>],
    slot: u32,
) -> Option<crate::FlexSpec> {
    let n = nodes.get(slot as usize).and_then(|o| o.as_ref())?;
    if !n.alive {
        return None;
    }
    let id = crate::NodeId((((n.gen as u32) << 16) | (slot & 0xFFFF)) as u32);
    let eligible = flex_eligible_props(nodes, slot as usize, &n.props);
    flex
        .iter()
        .flatten()
        .find(|e| e.node == id)
        .map(|e| crate::FlexSpec {
            grow: e.weight,
            shrink: e.shrink,
            basis: e.basis,
        })
        .filter(|_| eligible)
}

/// L6a place 期 flex 主轴分配。`slots[i]` 对应第一次兄弟走查中第 i 个
/// 活 flex 子；第二次走查必须保持同一 sibling order。
#[derive(Clone, Copy)]
struct FlexSlots {
    slots: [i32; crate::FLEX_MAX],
}

impl FlexSlots {
    fn none() -> Self {
        Self { slots: [0; crate::FLEX_MAX] }
    }
}

/// basis/grow/shrink 单源分配：
///
/// - base = Px 或 Auto(子主轴 meas)；unregistered 子 base = meas。
/// - 正余量按 grow cumulative-prefix floor 精确分完。
/// - 负余量若 shrink pool 非零，按 shrink×base 比例收缩；单子超额即
///   冻结并重分配，最多 FLEX_MAX 轮。
/// - L2 sugar(basis=0, shrink=0) 数学退化为旧 flex_space 分配。
fn flex_slots(
    nodes: &[Option<Node>],
    flex: &[Option<FlexEntry>],
    first: u32,
    vertical: bool,
    container: i32,
    gap: u32,
) -> FlexSlots {
    if !flex.iter().flatten().next().is_some() {
        return FlexSlots::none(); // 无登记树保持原单走查性能面。
    }

    let mut bases = [0u64; crate::FLEX_MAX];
    let mut grows = [0u16; crate::FLEX_MAX];
    let mut shrinks = [0u16; crate::FLEX_MAX];
    let mut active = [false; crate::FLEX_MAX];
    let mut count = 0usize;
    let mut sibling_count = 0i64;
    let mut base_sum = 0i64;
    let mut cur = first;
    let mut guard = 0usize;
    while cur != u32::MAX && guard <= nodes.len() {
        guard += 1;
        let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
        if !c.alive {
            break;
        }
        let next = c.next_sib;
        let meas = if vertical { c.meas_h } else { c.meas_w } as u64;
        match flex_spec_of(nodes, flex, cur) {
            Some(spec) if count < crate::FLEX_MAX => {
                let base = match spec.basis {
                    crate::FlexBasis::Px(value) => value as u64,
                    crate::FlexBasis::Auto => meas,
                };
                bases[count] = base;
                grows[count] = spec.grow;
                shrinks[count] = spec.shrink;
                active[count] = spec.shrink > 0;
                base_sum = base_sum.saturating_add(base as i64);
                count += 1;
            }
            Some(_) => {
                // 结构性不可达（FLEX_MAX 写入帽）；防御时仍计入 sibling/gap。
                debug_assert!(
                    false,
                    "flex entries exceeded FLEX_MAX without table expansion"
                );
            }
            None => {
                base_sum = base_sum.saturating_add(meas as i64);
            }
        }
        sibling_count += 1;
        cur = next;
    }

    let gaps = gap as i64 * (sibling_count - 1).max(0);
    let free = (container as i64)
        .saturating_sub(gaps)
        .saturating_sub(base_sum);
    let mut result = [0i32; crate::FLEX_MAX];

    if free >= 0 {
        let total_grow: u64 = grows[..count].iter().map(|v| u64::from(*v)).sum();
        if total_grow > 0 {
            let mut cumulative = 0u64;
            let mut previous_target = 0u64;
            for i in 0..count {
                cumulative += u64::from(grows[i]);
                let target = free as u64 * cumulative / total_grow;
                let share = target.saturating_sub(previous_target);
                previous_target = target;
                // grow=0 的子在此轮 share=0，保留 base。
                result[i] = i32::try_from(bases[i].saturating_add(share))
                    .unwrap_or(i32::MAX);
            }
        } else {
            for (slot, base) in result.iter_mut().zip(bases.iter()).take(count) {
                *slot = i32::try_from(*base).unwrap_or(i32::MAX);
            }
        }
        return FlexSlots { slots: result };
    }

    // free < 0：CSS 7.2 型 capped proportional shrink + freeze loop。
    let deficit = u64::try_from(free.saturating_neg()).unwrap_or(u64::MAX);
    let mut takes = [0u64; crate::FLEX_MAX];
    let mut remaining = deficit;
    let mut passes = 0usize;
    while remaining > 0 && passes < crate::FLEX_MAX {
        passes += 1;
        let total_factor: u128 = (0..count)
            .filter(|&i| active[i])
            .map(|i| u128::from(shrinks[i]) * u128::from(bases[i]))
            .sum::<u128>();
        if total_factor == 0 {
            break;
        }
        let mut cumulative = 0u128;
        let mut previous_target = 0u128;
        let mut proposed = takes;
        let mut froze = false;
        for i in 0..count {
            if !active[i] {
                continue;
            }
            cumulative += u128::from(shrinks[i]) * u128::from(bases[i]);
            let target = u128::from(remaining) * cumulative / total_factor;
            let increment = target.saturating_sub(previous_target);
            previous_target = target;
            let candidate = takes[i].saturating_add(u64::try_from(increment).unwrap_or(u64::MAX));
            proposed[i] = candidate;
            if candidate > bases[i] {
                froze = true;
            }
        }
        if froze {
            // 本轮有子冻结时，未冻结子的“比例预分配”不能保留——它与冻结
            // 子的 base 相加可能超过 deficit。回滚未冻结子到上一轮，再对
            // 剩余 active 子重分配；冻结子保 base。
            for i in 0..count {
                if active[i] {
                    proposed[i] = if proposed[i] > bases[i] {
                        bases[i]
                    } else {
                        takes[i]
                    };
                }
            }
        }
        takes = proposed;
        if froze {
            for i in 0..count {
                if active[i] && takes[i] >= bases[i] {
                    active[i] = false;
                }
            }
        }
        let taken: u64 = takes[..count].iter().copied().sum();
        remaining = deficit.saturating_sub(taken);
        if !froze {
            break;
        }
    }

    for i in 0..count {
        let slot = bases[i].saturating_sub(takes[i]);
        result[i] = i32::try_from(slot).unwrap_or(i32::MAX);
    }
    FlexSlots { slots: result }
}

#[allow(clippy::too_many_arguments)]
fn place(
    nodes: &mut [Option<Node>],
    slot: u32,
    cx: i64,
    cy: i64,
    rel_x: i32,
    rel_y: i32,
    avail_w: i32,
    avail_h: i32,
    force: bool,
    flex: &[Option<FlexEntry>],
    recomputed: &[bool; CAP],
    old_abs: &[Option<RectI>; CAP],
    vp: &RectI,
    damage: &mut DamageSet,
    placed: &mut u32,
    hops: u32,
) {
    if hops as usize > nodes.len() {
        return; // 环防御
    }
    let idx = slot as usize;
    let Some(n) = nodes.get(idx).and_then(|o| o.as_ref()) else { return };
    if !n.alive {
        return;
    }
    let rec = if idx < CAP { recomputed[idx] } else { true };
    if !(force || rec) {
        // 几何语境未变——子仍按各自 rec 自决（force=false 递归）；
        // 链累计原点取本节点**现行**位（未重摆）
        let Some(n0) = nodes.get(idx).and_then(|o| o.as_ref()) else { return };
        let ncx = cx + n0.x as i64 + n0.ox as i64;
        let ncy = cy + n0.y as i64 + n0.oy as i64;
        place_children(nodes, slot, ncx, ncy, false, flex, recomputed, old_abs, vp, damage, placed, hops);
        return;
    }
    *placed += 1;
    // 旧界 damage（快照；oversize 槽无快照——保守只记新界）
    if let Some(ob) = old_abs.get(idx).copied().flatten() {
        dmg_clip_add(damage, &ob, vp);
    }
    // 新几何
    let (w, h) = final_size(nodes[idx].as_ref().unwrap(), avail_w, avail_h);
    let n = nodes[idx].as_mut().unwrap();
    let geom_changed = n.x != rel_x || n.y != rel_y || n.w != w || n.h != h;
    n.x = rel_x;
    n.y = rel_y;
    n.w = w;
    n.h = h;
    // 新视觉界（含 ox/oy）damage + 子链累计原点
    let ncx = cx + rel_x as i64 + n.ox as i64;
    let ncy = cy + rel_y as i64 + n.oy as i64;
    let nb = RectI::new(i64_to_i32_sat(ncx), i64_to_i32_sat(ncy), w, h);
    dmg_clip_add(damage, &nb, vp);
    // 子分配语境变 = 本节点几何变 ∨ 本节点重测（gap/all 等分配面属性）
    place_children(nodes, slot, ncx, ncy, geom_changed || rec, flex, recomputed, old_abs, vp, damage, placed, hops);
}

/// 子分配（kind 分派）：Root/Pad 内缩整给、Col/Row 游标累进 + §4.1 十
/// 字档、Grid §4.2 等分轨格位、叶子无子。`ncx/ncy` = 本节点视觉链累计
/// 原点（子的 cx/cy）。
#[allow(clippy::too_many_arguments)]
fn place_children(
    nodes: &mut [Option<Node>],
    slot: u32,
    ncx: i64,
    ncy: i64,
    force_kids: bool,
    flex: &[Option<FlexEntry>],
    recomputed: &[bool; CAP],
    old_abs: &[Option<RectI>; CAP],
    vp: &RectI,
    damage: &mut DamageSet,
    placed: &mut u32,
    hops: u32,
) {
    let Some(n) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else { return };
    let (w, h, kind, first, props) = (n.w, n.h, n.kind, n.first_child, n.props);
    if matches!(kind, ViewKind::Rect | ViewKind::Text | ViewKind::Image | ViewKind::Icon)
        || first == u32::MAX
    {
        return; // 叶子无子（Image/Icon 入列——D24）/ 空容器
    }
    match &props {
        Props::Pad(p) => {
            let inset = u32_to_i32_sat(p.all);
            let cw = (w as i64 - 2 * inset as i64).max(0);
            let ch = (h as i64 - 2 * inset as i64).max(0);
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                place(
                    nodes, cur, ncx, ncy, inset, inset, i64_to_i32_sat(cw), i64_to_i32_sat(ch),
                    force_kids, flex, recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cur = next;
            }
        }
        Props::Col(p) => {
            let gap = p.gap as i64;
            let allocation = flex_slots(nodes, flex, first, true, h, p.gap);
            let mut flex_index = 0usize;
            let mut cursor: i64 = 0;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                if !c.alive {
                    break;
                }
                let ch_h = match flex_spec_of(nodes, flex, cur) {
                    Some(_) => {
                        let value = allocation.slots[flex_index];
                        flex_index += 1;
                        value
                    }
                    None => u32_to_i32_sat(c.meas_h), // 主轴请求 = 子内在（None 折 0）
                };
                // §4.1 十字档：Stretch = 现状拉满（零漂）；Start/Center =
                // 子内在宽为 avail（随父型子消费——内在型叶 Text/Image/
                // Icon 豁免不吃 avail），Center 摆位 (可用-内在)/2（可负
                // ——宽溢出钉负偏移，i64 截断前算）
                let m_w = u32_to_i32_sat(c.meas_w);
                let (x_off, aw) = match p.cross {
                    CrossAlign::Stretch => (0, w),
                    CrossAlign::Start => (0, m_w),
                    CrossAlign::Center => {
                        (i64_to_i32_sat((w as i64 - m_w as i64) / 2), m_w)
                    }
                };
                place(
                    nodes, cur, ncx, ncy, x_off, i64_to_i32_sat(cursor), aw, ch_h, force_kids,
                    flex, recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cursor += ch_h as i64 + gap;
                cur = next;
            }
        }
        Props::Row(p) => {
            let gap = p.gap as i64;
            let allocation = flex_slots(nodes, flex, first, false, w, p.gap);
            let mut flex_index = 0usize;
            let mut cursor: i64 = 0;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                if !c.alive {
                    break;
                }
                let cw = match flex_spec_of(nodes, flex, cur) {
                    Some(_) => {
                        let value = allocation.slots[flex_index];
                        flex_index += 1;
                        value
                    }
                    None => u32_to_i32_sat(c.meas_w),
                };
                // §4.1 十字档镜像（Row 十字 = 纵轴）：Start/Center 以子内
                // 在高为 avail、Center 纵向居中
                let m_h = u32_to_i32_sat(c.meas_h);
                let (y_off, ah) = match p.cross {
                    CrossAlign::Stretch => (0, h),
                    CrossAlign::Start => (0, m_h),
                    CrossAlign::Center => {
                        (i64_to_i32_sat((h as i64 - m_h as i64) / 2), m_h)
                    }
                };
                place(
                    nodes, cur, ncx, ncy, i64_to_i32_sat(cursor), y_off, cw, ah, force_kids,
                    flex, recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cursor += cw as i64 + gap;
                cur = next;
            }
        }
        Props::Panel(p) => {
            let inset = u32_to_i32_sat(p.padding);
            let cw = (w as i64 - 2 * inset as i64).max(0);
            let mut cursor = inset as i64;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                let child_h = u32_to_i32_sat(c.meas_h);
                place(
                    nodes, cur, ncx, ncy, inset, i64_to_i32_sat(cursor),
                    i64_to_i32_sat(cw), child_h, force_kids, flex, recomputed, old_abs, vp,
                    damage, placed, hops + 1,
                );
                cursor += child_h as i64 + p.gap as i64;
                cur = next;
            }
        }
        Props::Grid(p) => {
            // §4.2 place：等分轨 track = (avail - gap×(轨数-1)) / 轨数
            //（向下取整，auto/fr 禁入）；cell 原点 = 游标步进（行主序：
            // i → 行 i/cols、列 i%cols）；子按 cross 档以 cell 两轴为可
            // 用域（§4.1 矩阵同律——内在型叶豁免）。轨数：列 = 声明
            // cols、行 = 子实占 rows_needed = ceil(k/cols)（与 measure
            // 同口径——空行不占轨）。0 维防御折 1（三面已 loud 拒）。
            let cols = p.cols.max(1) as usize;
            let gap = p.gap as i64;
            let mut k = 0usize;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                if !c.alive {
                    break;
                }
                k += 1;
                cur = c.next_sib;
            }
            let rows_needed = ((k + cols - 1) / cols).max(1);
            let track_w = i64_to_i32_sat(
                ((w as i64 - gap * (cols as i64 - 1)).max(0)) / cols as i64,
            );
            let track_h = i64_to_i32_sat(
                ((h as i64 - gap * (rows_needed as i64 - 1)).max(0)) / rows_needed as i64,
            );
            let mut i = 0usize;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                if !c.alive {
                    break;
                }
                let next = c.next_sib;
                let cell_x = (i % cols) as i64 * (track_w as i64 + gap);
                let cell_y = (i / cols) as i64 * (track_h as i64 + gap);
                let m_w = u32_to_i32_sat(c.meas_w);
                let m_h = u32_to_i32_sat(c.meas_h);
                let (dx, dy, aw, ah) = match p.cross {
                    CrossAlign::Stretch => (0i64, 0i64, track_w, track_h),
                    CrossAlign::Start => (0i64, 0i64, m_w, m_h),
                    CrossAlign::Center => (
                        (track_w as i64 - m_w as i64) / 2,
                        (track_h as i64 - m_h as i64) / 2,
                        m_w,
                        m_h,
                    ),
                };
                place(
                    nodes, cur, ncx, ncy,
                    i64_to_i32_sat(cell_x + dx),
                    i64_to_i32_sat(cell_y + dy),
                    aw, ah, force_kids,
                    flex, recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                i += 1;
                cur = next;
            }
        }
        // Root/Stack（及其余不可达——Rect/Text/Image/Icon 已在叶卫拦
        // 下）：分配面整给——子各自 (0,0) 起满幅。对 Stack 即叠层语义
        // 本义：全子同位叠放（前序子在下、后序子盖上 = z 序），内在型
        // 子（Text/组件叶）保内在尺寸钉 (0,0)、随父型子（Rect w/h
        // None/Spacer）吃满——**下划线臂对叠层天然合义**（本刀确认）。
        Props::Tabs(_) => {
            // 波2 批 A：Tabs = 页签条（顶 48）+ 内容区（余高全幅给子
            // ——Root 型全幅叠放的纵向偏移版；多面板 z 叠放，active 面
            // 由 app 摆子序控制（后插者盖上））。
            let content_h = i64_to_i32_sat((h as i64 - crate::TABS_BAR_H as i64).max(0));
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                place(
                    nodes, cur, ncx, ncy, 0, crate::TABS_BAR_H, w, content_h, force_kids, flex,
                    recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cur = next;
            }
        }
        Props::Scroll(p) => {
            // 波2 批 A：Scroll = 视口容器：子按内在全幅摆放、纵向上移
            // offset_y（内容高于视口时滚动；paint 期 clip 由双面 clip
            // 横向原语挂账——v1 无 clip 面，子溢出视口可见（保守正确
            // ——launcher 消费面自管 offset 域））。
            let off = p.offset_y as i64;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                place(
                    nodes, cur, ncx, ncy, 0, i64_to_i32_sat(-off), w, h, force_kids, flex,
                    recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cur = next;
            }
        }
        Props::Collapse(p) => {
            // 组件第三批：Collapse = 折叠组容器——expanded 时子在 header
            //（32）下方 Col 型游标排列（宽吃满、高按内在累进）；收起 →
            // 子不摆位（paint 不发射 + hit 不命中三面同守卫——陈旧
            // bounds 一致跳过，收起瞬时旧界 damage 由本节点新旧界收）。
            if !p.expanded {
                return;
            }
            let mut cursor: i64 = u32_to_i32_sat(crate::COLLAPSE_HEADER_H) as i64;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                let ch_h = u32_to_i32_sat(c.meas_h);
                place(
                    nodes, cur, ncx, ncy, 0, i64_to_i32_sat(cursor), w, ch_h, force_kids, flex,
                    recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cursor += ch_h as i64;
                cur = next;
            }
        }
        Props::List(p) => {
            // 波2 批 A 补账：List = 等高行容器——行序子 y = i×item_height
            //、x=0 宽吃满、行槽高 = item_height（批 A 半成品落 `_` 兜臂
            //全叠 (0,0)；内在 item_count×item_height 已单源，摆位同源）。
            let ih = u32_to_i32_sat(p.item_height) as i64;
            let mut i: i64 = 0;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                place(
                    nodes, cur, ncx, ncy, 0, i64_to_i32_sat(i.saturating_mul(ih)), w,
                    i64_to_i32_sat(ih), force_kids, flex,
                    recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                i += 1;
                cur = next;
            }
        }

        // Root / Clip：viewport/裁剪 port 整给——子各自 (0,0) 起满幅。
        // 其余 props 已在上文 exhaustive 分派；防御空臂仅保编译全匹配。
        Props::Stack(_) | Props::Card(_) | Props::Refresh(_)
        | Props::Clip(_) | Props::Root => {
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                place(
                    nodes, cur, ncx, ncy, 0, 0, w, h, force_kids, flex, recomputed, old_abs, vp, damage,
                    placed, hops + 1,
                );
                cur = next;
            }
        }
        _ => {}
    }
}
