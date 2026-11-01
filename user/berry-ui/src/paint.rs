//! berry-ui paint 段（GUI-3——`docs/GUI-3实施设计草案.md` §3.5）。
//!
//! - **前到后剔除、后到前发射**（OH ui_lite 同路）：按 damage 矩形逐
//!   个走树——逆前序（尾兄先、子先于父 = 画家算法后画者先看）维护
//!   [`RectSet`] 已覆盖集；**仅 Rect 叶子（实心不透明）计遮挡源**
//!   （Text 蜂格有洞不计——保守正确）。可见区（bounds ∩ damage −
//!   covered）空 → skip（`culled++`）。
//! - 发射序 = 收集序的逆 = 前序（画家算法背到前：后者盖前者）。逐节
//!   点：Rect → `fill_rect`；Text → GlyphPos run 组进调用方 scratch →
//!   `draw_glyph_run`（`style.bg` 衬底透传，None = 透明底——§6
//!   TextStyle 字段语义优先）。
//! - **尾部必录 `Present{damage 包络}`**（soft no-op 也录——FULL →
//!   viewport、空集 → 零矩形）。
//! - overdraw_x256 = drawn_px×256/vp 面素（发射足迹 ∩ viewport；u128
//!   中间量饱和收口）——预算 ≤ [`crate::OVERDRAW_BUDGET_X256`] 的断
//!   言面在测试侧。
//! - DisplayList `'a` 钉**调用方 scratch**（不钉引擎）：多个 Text run
//!   经 `split_at_mut` 游标取互斥子片（零 unsafe——mem::replace 借移
//!   产出与整体同寿命 'a 的独占片）；scratch 不足的 run 截断到余量。
//! - 零 panic / 零分配：cmds 满帽（CmdError::Full）停录不炸；树环/死
//!   槽防御以 nodes.len() 为帽。

use crate::damage::{i64_to_i32_sat, DamageSet, RectI, RectSet};
use crate::desc::{Props, ViewKind};
use crate::Node;
use berry_gfx::cmd::{CommandEncoder, GlyphPos, GlyphRun, Rect as GfxRect};

/// 槽索引空哨兵（§6 Node 注：NONE = u32::MAX——hit.rs 同值，本模块自
/// 带防跨模块未定名依赖）。
const NONE: u32 = u32::MAX;

/// 遮挡覆盖集容量（damage 矩形 ≤ 8 的两倍余量；溢出保守少记覆盖 →
/// 多画不少画，见 RectSet 类型注）。
const COVERED_CAP: usize = 16;

/// 单 damage 矩形的可见发射序缓冲（>512 可见叶子的病态树触发 cull 溢
/// 出 → `emit_walk_all` 回退直发射——不丢画面；MAX_NODES=256 契约的
/// 两倍余量）。
const ORDER_CAP: usize = 512;

/// paint 产出统计（§6 冻结字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaintReport {
    /// 成功录入 display list 的命令数（含尾 Present）。
    pub cmds: usize,
    /// 发射足迹面素（节点界 ∩ viewport 之和）。
    pub drawn_px: u64,
    /// 可见区空被剔除的叶子数（含 damage 外与被盖两面）。
    pub culled: u32,
    /// overdraw 倍率×256（drawn_px×256/vp 面素）。
    pub overdraw_x256: u32,
}

/// paint 主入口（§6 冻结内部签名）：`scratch` 与 encoder 的 `'a` 同源
/// ——glyph run 引用钉调用方缓冲，引擎不持引用。
pub(crate) fn paint<'a>(
    nodes: &[Option<Node>],
    root: u32,
    vp: (u32, u32),
    damage: &DamageSet,
    scratch: &'a mut [GlyphPos],
    enc: &mut CommandEncoder<'a>,
) -> PaintReport {
    let mut rep =
        PaintReport { cmds: 0, drawn_px: 0, culled: 0, overdraw_x256: 0 };
    if root == NONE {
        return rep; // 无树（lib.rs 入口已挡，防御双保险——不录 Present）
    }
    match nodes.get(root as usize) {
        Some(Some(n)) if n.alive => {}
        _ => return rep,
    }
    let vp_rect = RectI::new(
        0,
        0,
        crate::damage::u32_to_i32_sat(vp.0),
        crate::damage::u32_to_i32_sat(vp.1),
    );

    // damage 矩形族（FULL → 单一 viewport 矩形）
    let mut dmgs = [RectI::new(0, 0, 0, 0); berry_guiproto::DAMAGE_MAX];
    let mut n_dmgs = 0usize;
    if damage.full {
        dmgs[0] = vp_rect;
        n_dmgs = 1;
    } else {
        for r in damage.iter() {
            let ri = RectI::from_wire(r);
            if !ri.is_empty() && n_dmgs < dmgs.len() {
                dmgs[n_dmgs] = ri;
                n_dmgs += 1;
            }
        }
    }

    // scratch 游标（互斥子片切分——每 Text run 冻结一片）
    let mut rest: &'a mut [GlyphPos] = scratch;

    for d in dmgs[..n_dmgs].iter() {
        let mut covered: RectSet<COVERED_CAP> = RectSet::new();
        let mut order = [NONE; ORDER_CAP];
        let mut bnds = [RectI::new(0, 0, 0, 0); ORDER_CAP];
        let mut n_order = 0usize;
        let mut overflow = false;
        let culled_before = rep.culled;
        cull_walk(
            nodes, root, 0, 0, d, &mut covered, &mut order, &mut bnds, &mut n_order,
            &mut rep.culled, &mut overflow, 0,
        );
        if overflow {
            // 病态巨树回退（>ORDER_CAP 可见叶子）：本 damage 矩形整段重走
            // ——前序直发射、无遮挡剔除、无序缓冲（painter 序由走查序
            // 自证，overdraw 偏高但**不丢画面**；主路径的 culled 计数回
            // 卷到本段前值，由回退走查按 damage 外叶子重计）。契约树
            // （N ≤ MAX_NODES=256 < ORDER_CAP/2）结构性不达此面——防御
            // 路径不设竞速断言，仅 offscreen 回归测试钉板行为。
            rep.culled = culled_before;
            emit_walk_all(nodes, root, 0, 0, d, &vp_rect, &mut rest, enc, &mut rep, 0);
        } else {
            // 发射 = 收集序之逆（前序 = 画家算法背到前）
            for k in (0..n_order).rev() {
                emit_node(nodes, order[k], &bnds[k], &vp_rect, &mut rest, enc, &mut rep);
            }
        }
    }

    // 尾部必录 Present{damage 包络}（FULL → viewport；空集 → 零矩形）
    let pres = present_rect(damage, &vp_rect);
    if enc.present(GfxRect::new(pres.x, pres.y, pres.w, pres.h)).is_ok() {
        rep.cmds += 1;
    }

    // overdraw 倍率（u128 中间量；零面素 viewport 折 0）
    let vp_area = vp.0 as u128 * vp.1 as u128;
    rep.overdraw_x256 = if vp_area == 0 {
        0
    } else {
        ((rep.drawn_px as u128 * 256 / vp_area).min(u32::MAX as u128)) as u32
    };
    rep
}

/// Present 载荷：FULL → viewport、有矩形 → 包络、空集 → 零矩形。
fn present_rect(damage: &DamageSet, vp_rect: &RectI) -> RectI {
    if damage.full {
        return *vp_rect;
    }
    match damage.envelope() {
        Some(r) => RectI::from_wire(&r),
        None => RectI::new(0, 0, 0, 0),
    }
}

/// scratch 游标切分：产出与整体同寿命 `'a` 的独占子片（mem::replace
/// 借移——零 unsafe 的流式 split）；余量不足 n → None。
fn take_scratch<'a>(rest: &mut &'a mut [GlyphPos], n: usize) -> Option<&'a mut [GlyphPos]> {
    if n == 0 || rest.len() < n {
        return None;
    }
    let owned: &'a mut [GlyphPos] = core::mem::replace(rest, &mut []);
    let (head, tail): (&'a mut [GlyphPos], &'a mut [GlyphPos]) = owned.split_at_mut(n);
    *rest = tail;
    Some(head)
}

/// 逆前序剔除走查（front-to-back）：子（尾兄先）→ 自身。可见叶子记
/// 入发射序（槽 + 视觉界快照免重算）；Rect 叶子界∩damage 计遮挡源。
/// `overflow` = 发射序缓冲满帽哨兵（调用方切回退走查——本走查的收集
/// 产物在溢出面被整段弃用）。
#[allow(clippy::too_many_arguments)]
fn cull_walk(
    nodes: &[Option<Node>],
    slot: u32,
    ex: i64,
    ey: i64,
    d: &RectI,
    covered: &mut RectSet<COVERED_CAP>,
    order: &mut [u32; ORDER_CAP],
    bnds: &mut [RectI; ORDER_CAP],
    n_order: &mut usize,
    culled: &mut u32,
    overflow: &mut bool,
    hops: u32,
) {
    if hops as usize > nodes.len() {
        return; // 环防御
    }
    let Some(n) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else { return };
    if !n.alive {
        return;
    }
    let bx = ex + n.x as i64 + n.ox as i64;
    let by = ey + n.y as i64 + n.oy as i64;

    // 子先：沿 next_sib 走到链尾再 prev_sib 回走（尾兄先 = 后画者先看）
    let mut tail = n.first_child;
    let mut guard = 0usize;
    while tail != NONE {
        guard += 1;
        if guard > nodes.len() {
            return; // next_sib 环防御
        }
        match nodes.get(tail as usize) {
            Some(Some(c)) if c.alive => {
                if c.next_sib == NONE {
                    break;
                }
                tail = c.next_sib;
            }
            _ => return,
        }
    }
    let mut sib = tail;
    let mut back = 0usize;
    while sib != NONE {
        back += 1;
        if back > nodes.len() {
            return; // prev_sib 环防御
        }
        cull_walk(nodes, sib, bx, by, d, covered, order, bnds, n_order, culled, overflow, hops + 1);
        match nodes.get(sib as usize) {
            Some(Some(c)) if c.alive => sib = c.prev_sib,
            _ => return,
        }
    }

    // 自身（子树最底 = 最后画）。Root/Col/Row/Pad 无视觉内容不参与。
    if !matches!(n.kind, ViewKind::Rect | ViewKind::Text) {
        return;
    }
    let b = RectI::new(i64_to_i32_sat(bx), i64_to_i32_sat(by), n.w, n.h);
    let vis = b.intersect(d);
    if vis.is_empty() || covered.is_covered(&vis) {
        *culled += 1; // §3.5：可见区空 → skip（culled++）
        return;
    }
    if n.kind == ViewKind::Rect {
        covered.add(vis); // 实心不透明才计遮挡源（Text 蜂格有洞不计）
    }
    if *n_order < ORDER_CAP {
        order[*n_order] = slot;
        bnds[*n_order] = b;
        *n_order += 1;
    } else {
        *culled += 1; // 病态巨树缓冲收口（>512 可见叶子——契约 2x 外）
        *overflow = true; // 调用方切回退走查（可见不丢画面）
    }
}

/// 回退走查（cull_walk 溢出面）：前序（自身先于子、头兄先）**直接发
/// 射**——无遮挡剔除、无序缓冲（画家序由走查序自证：先访问者先画、
/// 后访问者覆盖 = 与主路径前序发射同序）。damage 外叶子计 culled（与
/// 主路径语义一致）；环/死槽防御同主路径以 nodes.len() 为帽。
#[allow(clippy::too_many_arguments)]
fn emit_walk_all<'a>(
    nodes: &[Option<Node>],
    slot: u32,
    ex: i64,
    ey: i64,
    d: &RectI,
    vp_rect: &RectI,
    rest: &mut &'a mut [GlyphPos],
    enc: &mut CommandEncoder<'a>,
    rep: &mut PaintReport,
    hops: u32,
) {
    if hops as usize > nodes.len() {
        return; // 环防御
    }
    let Some(n) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else { return };
    if !n.alive {
        return;
    }
    let bx = ex + n.x as i64 + n.ox as i64;
    let by = ey + n.y as i64 + n.oy as i64;

    // 自身先（前序 = 画家算法背到前：先画者先发射）。Root/Col/Row/Pad
    // 无视觉内容不参与（主路径同语义）。
    if matches!(n.kind, ViewKind::Rect | ViewKind::Text) {
        let b = RectI::new(i64_to_i32_sat(bx), i64_to_i32_sat(by), n.w, n.h);
        if b.intersect(d).is_empty() {
            rep.culled += 1; // damage 外叶子
        } else {
            emit_node(nodes, slot, &b, vp_rect, rest, enc, rep);
        }
    }

    // 子：头兄起流序（painter 序：后画者后访问 = 覆盖在前画者上）。
    let mut c = n.first_child;
    let mut guard = 0usize;
    while c != NONE {
        guard += 1;
        if guard > nodes.len() {
            return; // 子链环防御
        }
        match nodes.get(c as usize) {
            Some(Some(ch)) if ch.alive => {
                emit_walk_all(nodes, c, bx, by, d, vp_rect, rest, enc, rep, hops + 1);
                c = ch.next_sib;
            }
            _ => return, // 链上死槽 = 树结构异常，防御放弃余段
        }
    }
}

/// 单节点发射（painter 序段）：Rect → fill_rect；Text → glyph run 组
/// scratch 子片。命令满帽（CmdError::Full）停录不炸；drawn_px 按节点
/// 界 ∩ viewport 计（发射足迹）。
fn emit_node<'a>(
    nodes: &[Option<Node>],
    slot: u32,
    b: &RectI,
    vp_rect: &RectI,
    rest: &mut &'a mut [GlyphPos],
    enc: &mut CommandEncoder<'a>,
    rep: &mut PaintReport,
) {
    let Some(n) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else { return };
    match &n.props {
        Props::Rect(p) => {
            if enc.fill_rect(GfxRect::new(b.x, b.y, b.w, b.h), p.bg).is_ok() {
                rep.cmds += 1;
            }
        }
        Props::Text(p) => {
            // run 截断到 scratch 余量（容量不足的尾部字形丢弃——调用
            // 方 scratch 尺寸义务见 §7 用例）
            let take = (p.text.len as usize).min(rest.len());
            if take > 0 {
                if let Some(piece) = take_scratch(rest, take) {
                    let cw = p.style.scale.cell_w() as i64;
                    for i in 0..take {
                        piece[i] = GlyphPos {
                            ch: p.text.bytes[i],
                            x: i64_to_i32_sat(b.x as i64 + i as i64 * cw),
                            y: b.y,
                        };
                    }
                    let glyphs: &'a [GlyphPos] = piece; // 冻结为共享——run 引用钉调用方缓冲
                    let run = GlyphRun {
                        atlas: p.style.scale.atlas(),
                        glyphs,
                        fg: p.style.fg,
                        bg: p.style.bg,
                    };
                    if enc.draw_glyph_run(run).is_ok() {
                        rep.cmds += 1;
                    }
                }
            }
        }
        _ => {} // Root/Col/Row/Pad：无视觉内容（子树各自发射）
    }
    rep.drawn_px = rep.drawn_px.saturating_add(b.intersect(vp_rect).area());
}
