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

use crate::damage::{i64_to_i32_sat, u32_to_i32_sat, DamageSet, RectI};
use crate::desc::{Props, ViewKind};
use crate::Node;
use berry_gfx::font::FontSize;

/// 布局产出统计（§6 冻结字段）：`measured` = 重测内在尺寸的节点数
/// （缓存有效性的断言面——0 = 全缓存命中）；`placed` = 重摆 x/y/w/h
/// 的节点数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutStat {
    pub measured: u32,
    pub placed: u32,
}

/// 走查/位图容量：MAX_NODES=256 契约的两倍余量；超帽（病态巨 arena）
/// 退化全量保守重排（见 relayout 头注）。
const CAP: usize = 512;

/// 内在尺寸聚合（u64 → u32 饱和收口）。
fn u64_to_u32_sat(v: u64) -> u32 {
    if v > u32::MAX as u64 {
        u32::MAX
    } else {
        v as u32
    }
}

/// damage 追加收口（§3.4 add 流程：先 clip viewport——负界/越界折界内
/// 残片；零面积丢弃。Ui::damage_rect 同语义的本面内副本，damage 单口）。
fn dmg_add(damage: &mut DamageSet, r: &RectI, vp: &RectI) {
    let c = r.intersect(vp);
    if !c.is_empty() {
        damage.add(c.to_wire());
    }
}

/// 布局主入口（§6 冻结签名）：脏子树重排、新界进 damage。
///
/// `freed` = 当前空闲槽清单（§6 签名面）——树走查自根、空闲槽已
/// `None` 天然跳过，此处不消费（removal 面的旧界 damage 在 Ui::remove
/// 侧先行入账）。
pub(crate) fn relayout(
    nodes: &mut [Option<Node>],
    vp_w: u32,
    vp_h: u32,
    damage: &mut DamageSet,
    freed: &[u32],
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
    measure(nodes, root, vp_w, vp_h, &mut mctx, oversize, 0);
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
        child_affected |= measure(nodes, cur, vp_w, vp_h, mctx, oversize, hops + 1);
        cur = next;
    }
    let dirty = nodes[idx].as_ref().map(|n| n.layout_dirty).unwrap_or(false);
    if !dirty && !child_affected && !oversize {
        return false; // 全缓存命中
    }
    let (w, h) = intrinsic(nodes, idx, vp_w, vp_h);
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
    true
}

/// 内在尺寸（读子 meas——后序保证已新鲜）。
fn intrinsic(nodes: &[Option<Node>], idx: usize, vp_w: u32, vp_h: u32) -> (u32, u32) {
    let n = match nodes[idx].as_ref() {
        Some(n) => n,
        None => return (0, 0),
    };
    match &n.props {
        Props::Root => (vp_w, vp_h),
        Props::Rect(p) => (p.w.unwrap_or(0), p.h.unwrap_or(0)),
        Props::Text(p) => {
            let scale: FontSize = p.style.scale;
            (u64_to_u32_sat(p.text.len as u64 * scale.cell_w() as u64), scale.cell_h() as u32)
        }
        Props::Col(p) => {
            let (w, mut h, k) = child_agg(nodes, n.first_child, true);
            h = h.saturating_add(p.gap as u64 * k.saturating_sub(1).max(0) as u64);
            (u64_to_u32_sat(w), u64_to_u32_sat(h))
        }
        Props::Row(p) => {
            let (mut w, h, k) = child_agg(nodes, n.first_child, false);
            w = w.saturating_add(p.gap as u64 * k.saturating_sub(1).max(0) as u64);
            (u64_to_u32_sat(w), u64_to_u32_sat(h))
        }
        Props::Pad(p) => {
            let (mut w, mut h, _) = child_agg(nodes, n.first_child, true);
            let both = 2u64.saturating_mul(p.all as u64);
            w = w.saturating_add(both);
            h = h.saturating_add(both);
            (u64_to_u32_sat(w), u64_to_u32_sat(h))
        }
    }
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
/// 整体吃满分配。
fn final_size(n: &Node, avail_w: i32, avail_h: i32) -> (i32, i32) {
    match &n.props {
        Props::Root => (avail_w, avail_h),
        Props::Rect(p) => (
            p.w.map(u32_to_i32_sat).unwrap_or(avail_w),
            p.h.map(u32_to_i32_sat).unwrap_or(avail_h),
        ),
        Props::Text(_) => (u32_to_i32_sat(n.meas_w), u32_to_i32_sat(n.meas_h)),
        Props::Col(_) => (avail_w, u32_to_i32_sat(n.meas_h)),
        Props::Row(_) => (u32_to_i32_sat(n.meas_w), avail_h),
        Props::Pad(_) => (avail_w, avail_h),
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
        dmg_add(damage, &ob, vp);
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
    dmg_add(damage, &nb, vp);
    // 子分配语境变 = 本节点几何变 ∨ 本节点重测（gap/all 等分配面属性）
    place_children(nodes, slot, ncx, ncy, geom_changed || rec, recomputed, old_abs, vp, damage, placed, hops);
}

/// 子分配（kind 分派）：Root/Pad 内缩整给、Col/Row 游标累进、叶子无
/// 子。`ncx/ncy` = 本节点视觉链累计原点（子的 cx/cy）。
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
    if matches!(kind, ViewKind::Rect | ViewKind::Text) || first == u32::MAX {
        return; // 叶子无子 / 空容器
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
                place(
                    nodes, cur, ncx, ncy, 0, i64_to_i32_sat(cursor), w, ch_h, force_kids,
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
                place(
                    nodes, cur, ncx, ncy, i64_to_i32_sat(cursor), 0, cw, h, force_kids,
                    recomputed, old_abs, vp, damage, placed, hops + 1,
                );
                cursor += cw as i64 + gap;
                cur = next;
            }
        }
        // Root（及其余）：viewport 整给——子各自 (0,0) 起满幅
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
