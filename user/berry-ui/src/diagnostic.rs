//! BerryUI F2 诊断面：布局与命中测试的确定性 dump。
//!
//! 这不是渲染协议，也不进 guest wire；它把引擎内部保留树、布局盒、
//! dirty 位和 topmost 命中结论投影成 Plain Data，供 host 测试、F5 uitest
//! 与 GUI 反 mock 关卡做值级对拍。输出顺序固定为树的前序遍历；容量由
//! 调用方提供，截断一律 `UiError::NodeFull` loud。

use crate::damage::RectI;
use crate::desc::{NodeId, ViewKind};
use crate::hit;
use crate::{Node, UiError, NONE};

/// 单个节点的布局/保留状态快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutDumpEntry {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub kind: ViewKind,
    pub key: u64,
    pub depth: u32,
    /// 父相对摆位（layout 产物，不含 paint offset）。
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// 视觉绝对界（含祖先链与自身的 ox/oy）。
    pub bounds: RectI,
    pub measured_w: u32,
    pub measured_h: u32,
    pub layout_dirty: bool,
    pub paint_dirty: bool,
}

/// 一个命中探针的结果。`node=None` 表示空白/Root；Root 恒不参与命中。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitDumpEntry {
    pub x: u32,
    pub y: u32,
    pub node: Option<NodeId>,
    pub kind: Option<ViewKind>,
    pub bounds: Option<RectI>,
}

fn id_of(slot: u32, n: &Node) -> NodeId {
    NodeId(((n.gen as u32) << 16) | (slot & 0xFFFF))
}

fn dump_subtree(
    nodes: &[Option<Node>],
    slot: u32,
    parent: Option<NodeId>,
    depth: u32,
    parent_x: i32,
    parent_y: i32,
    out: &mut [LayoutDumpEntry],
    len: &mut usize,
) -> Result<(), UiError> {
    if depth as usize > nodes.len() {
        return Err(UiError::NodeBad);
    }
    let Some(node) = nodes.get(slot as usize).and_then(|option| option.as_ref()) else {
        return Err(UiError::NodeBad);
    };
    if !node.alive || *len >= out.len() {
        return Err(UiError::NodeFull);
    }
    let id = id_of(slot, node);
    let absolute_x = parent_x.saturating_add(node.x).saturating_add(node.ox);
    let absolute_y = parent_y.saturating_add(node.y).saturating_add(node.oy);
    out[*len] = LayoutDumpEntry {
        id,
        parent,
        first_child: (node.first_child != NONE)
            .then(|| nodes.get(node.first_child as usize).and_then(|o| o.as_ref()).map(|c| id_of(node.first_child, c)))
            .flatten(),
        next_sibling: (node.next_sib != NONE)
            .then(|| nodes.get(node.next_sib as usize).and_then(|o| o.as_ref()).map(|s| id_of(node.next_sib, s)))
            .flatten(),
        kind: node.kind,
        key: node.key,
        depth,
        x: node.x,
        y: node.y,
        w: node.w,
        h: node.h,
        bounds: RectI::new(absolute_x, absolute_y, node.w, node.h),
        measured_w: node.meas_w,
        measured_h: node.meas_h,
        layout_dirty: node.layout_dirty,
        paint_dirty: node.paint_dirty,
    };
    *len += 1;

    let mut child = node.first_child;
    let mut hops = 0usize;
    while child != NONE {
        hops += 1;
        if hops > nodes.len() {
            return Err(UiError::NodeBad);
        }
        let Some(child_node) = nodes.get(child as usize).and_then(|option| option.as_ref()) else {
            return Err(UiError::NodeBad);
        };
        dump_subtree(
            nodes,
            child,
            Some(id),
            depth + 1,
            absolute_x,
            absolute_y,
            out,
            len,
        )?;
        child = child_node.next_sib;
    }
    Ok(())
}

/// 前序布局 dump。返回写入条数；`out` 过小 = `NodeFull`，绝不静默截断。
pub(crate) fn layout(
    nodes: &[Option<Node>],
    root: u32,
    out: &mut [LayoutDumpEntry],
) -> Result<usize, UiError> {
    let mut len = 0usize;
    if root == NONE {
        return Ok(len);
    }
    dump_subtree(nodes, root, None, 0, 0, 0, out, &mut len)?;
    Ok(len)
}

/// 命中测试 dump：逐点复用 `hit::node_at` 的 topmost 单源结论。
pub(crate) fn hit_test(
    nodes: &[Option<Node>],
    root: u32,
    points: &[(u32, u32)],
    out: &mut [HitDumpEntry],
) -> Result<usize, UiError> {
    if points.len() > out.len() {
        return Err(UiError::NodeFull);
    }
    for (i, &(x, y)) in points.iter().enumerate() {
        let node = hit::node_at(nodes, root, hit::u32_to_i32_sat(x), hit::u32_to_i32_sat(y));
        let mut kind = None;
        let mut bounds = None;
        if let Some(id) = node {
            let slot = (id.0 & 0xFFFF) as usize;
            let gen = (id.0 >> 16) as u16;
            if let Some(Some(n)) = nodes.get(slot).map(|option| option.as_ref()) {
                if n.alive && n.gen == gen {
                    kind = Some(n.kind);
                    let mut absolute_x = 0i32;
                    let mut absolute_y = 0i32;
                    let mut cur = slot as u32;
                    let mut hops = 0usize;
                    while let Some(Some(walk)) = nodes.get(cur as usize).map(|option| option.as_ref()) {
                        absolute_x = absolute_x.saturating_add(walk.x).saturating_add(walk.ox);
                        absolute_y = absolute_y.saturating_add(walk.y).saturating_add(walk.oy);
                        if walk.parent == NONE || hops > nodes.len() {
                            break;
                        }
                        cur = walk.parent;
                        hops += 1;
                    }
                    bounds = Some(RectI::new(absolute_x, absolute_y, n.w, n.h));
                }
            }
        }
        out[i] = HitDumpEntry { x, y, node, kind, bounds };
    }
    Ok(points.len())
}
