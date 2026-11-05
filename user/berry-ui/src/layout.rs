//! berry-ui 布局引擎（GUI-3——`docs/GUI-3实施设计草案.md` §3.3）。
//!
//! - **单遍自顶向下**：容器分配子界——Col/Row 累进游标 + gap；Pad 四
//!   向内缩；Root = viewport 持有者。叶子尺寸 = 定值（`Option<u32>`，
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
use crate::desc::{Props, ViewKind};
use crate::metrics::{IconSource, TextMetrics};
use crate::Node;

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

/// 容器约束（GUI-6 D9 立缝）：v1 只接 `Viewport`（= 现状根调用形态）；
/// 其余三档 debug_assert loud + 按 Viewport 处理（案 1 layout.rs v2
/// 接实现——独立刀，本刀零预埋）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraints {
    Viewport,
    Tight { w: u32, h: u32 },
    Loose { w: u32, h: u32 },
    Infinite,
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
/// 占位）；`cs` = D9 约束档，v1 只接 Viewport（其余三档 debug_assert
/// loud 后按 Viewport 处理——案 1 v2 接实现）。
pub(crate) fn relayout(
    nodes: &mut [Option<Node>],
    vp_w: u32,
    vp_h: u32,
    damage: &mut DamageSet,
    freed: &[u32],
    tm: &dyn TextMetrics,
    icons: &dyn IconSource,
    cs: Constraints,
) -> LayoutStat {
    let _ = freed;
    if !matches!(cs, Constraints::Viewport) {
        // D9：v1 只接 Viewport——非 Viewport 档 = 调用面漂移（案 1 未
        // 落），loud 后按 Viewport 保守处理（不静默吞）。
        debug_assert!(false, "Constraints 非 Viewport 档未实现（案 1 v2）: {cs:?}");
    }
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
    measure(nodes, root, vp_w, vp_h, tm, icons, &mut mctx, oversize, 0);
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
    let mut placed = 0u32;
    place(
        nodes,
        root,
        0,
        0,
        0,
        0,
        u32_to_i32_sat(vp_w),
        u32_to_i32_sat(vp_h),
        false,
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
    let Some(n) = nodes.get(idx).and_then(|o| o.as_ref()) else { return false };
    if !n.alive {
        return false;
    }
    // 子先（后序）——链走查带环防御
    let mut child_affected = false;
    let mut cur = n.first_child;
    let mut guard = 0usize;
    while cur != u32::MAX && guard <= nodes.len() {
        guard += 1;
        let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
        let next = c.next_sib;
        child_affected |= measure(nodes, cur, vp_w, vp_h, tm, icons, mctx, oversize, hops + 1);
        cur = next;
    }
    let dirty = nodes[idx].as_ref().map(|n| n.layout_dirty).unwrap_or(false);
    if !dirty && !child_affected && !oversize {
        return false; // 全缓存命中
    }
    let (w, h) = intrinsic(nodes, idx, vp_w, vp_h, tm, icons);
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

/// 内在尺寸（读子 meas——后序保证已新鲜）。Text 臂经 `tm.measure` 走
/// 供给缝（D8：scale id 与 desc wire 域同序——Monospace 供件逐位复刻
/// `len × cell_w` 现状，换件面 = GUI-2 增件⑧）；Image/Icon 叶经
/// `icons.dims`（§4.3）。Col/Row/Grid 的 w/h Some 折入内在（§4.1 定尺寸
/// 覆盖一切——父游标推进/聚合/Start·Center 自身_extent 单源用 meas）。
fn intrinsic(
    nodes: &[Option<Node>],
    idx: usize,
    vp_w: u32,
    vp_h: u32,
    tm: &dyn TextMetrics,
    icons: &dyn IconSource,
) -> (u32, u32) {
    let n = match nodes[idx].as_ref() {
        Some(n) => n,
        None => return (0, 0),
    };
    match &n.props {
        Props::Root => (vp_w, vp_h),
        Props::Rect(p) => (p.w.unwrap_or(0), p.h.unwrap_or(0)),
        Props::Text(p) => {
            tm.measure(p.text.as_bytes(), crate::desc::scale_to_u8(p.style.scale))
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
        Props::Image(p) => icon_dims(icons, p.res, p.w, p.h),
        Props::Icon(p) => icon_dims(icons, p.id, p.w, p.h),
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
/// 整体吃满分配。段C（§4.1/§4.3）：Col/Row/Grid 的 HAS_SIZE 定尺寸
/// 优先于 avail（定尺寸覆盖一切）；Grid = 随父型子（Col 同形——十字
/// 吃 avail、纵轴内在）；Image/Icon = 内在型叶恒内在（Some 定值 /
/// None = dims 折入 meas），**不吃 avail**（§4.1 豁免行——十字轴拉
/// 伸只及随父型子）。
fn final_size(n: &Node, avail_w: i32, avail_h: i32) -> (i32, i32) {
    match &n.props {
        Props::Root => (avail_w, avail_h),
        Props::Rect(p) => (
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
        Props::Image(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_w)),
            p.h.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_h)),
        ),
        Props::Icon(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_w)),
            p.h.map(u32_to_i32_sat).unwrap_or(u32_to_i32_sat(n.meas_h)),
        ),
    }
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
        place_children(nodes, slot, ncx, ncy, false, recomputed, old_abs, vp, damage, placed, hops);
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
    place_children(nodes, slot, ncx, ncy, geom_changed || rec, recomputed, old_abs, vp, damage, placed, hops);
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
                    force_kids, recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cur = next;
            }
        }
        Props::Col(p) => {
            let gap = p.gap as i64;
            let mut cursor: i64 = 0;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                let ch_h = u32_to_i32_sat(c.meas_h); // 主轴请求 = 子内在（None 折 0）
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
                    recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cursor += ch_h as i64 + gap;
                cur = next;
            }
        }
        Props::Row(p) => {
            let gap = p.gap as i64;
            let mut cursor: i64 = 0;
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                let cw = u32_to_i32_sat(c.meas_w);
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
                    recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cursor += cw as i64 + gap;
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
                    recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                i += 1;
                cur = next;
            }
        }
        // Root（及其余不可达——Rect/Text/Image/Icon 已在叶卫拦下）：viewport
        // 整给——子各自 (0,0) 起满幅
        _ => {
            let mut cur = first;
            let mut guard = 0usize;
            while cur != u32::MAX && guard <= nodes.len() {
                guard += 1;
                let Some(c) = nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
                let next = c.next_sib;
                place(
                    nodes, cur, ncx, ncy, 0, 0, w, h, force_kids, recomputed, old_abs, vp, damage,
                    placed, hops + 1,
                );
                cur = next;
            }
        }
    }
}
