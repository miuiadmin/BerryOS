//! berry-ui 动画推进（GUI-3——`docs/GUI-3实施设计草案.md` §3.7：
//! Transform/Layout 分型 day-one，为 v2 服务端动画/合成留缝——钦定面）。
//!
//! - [`AnimProp::OffsetX`]/[`AnimProp::OffsetY`] = **Transform 型**：落
//!   节点 paint 偏移 ox/oy，**不重排**；damage = 新旧绝对界包络（§3.7）。
//! - [`AnimProp::Width`]/[`AnimProp::Height`]/[`AnimProp::Gap`] =
//!   **Layout 型**：改 props + `layout_dirty` 重排；damage 记旧绝对界
//!   （新界随 relayout 入账——layout 段单一事实源）。
//! - 定容表 `[Option<Anim>; ANIM_MAX=16]`（lib.rs 持有，AnimFull loud 在
//!   anim_add）；tick 在帧内推进，完成项移除；返回 anim_active 喂帧
//!   循环信号。
//! - 完成语义：`dur_ns == 0` 或 `now ≥ start+dur` → 应用**精确 `to`
//!   终点值**（终点钉板——浮点路径的最后值不外溢）；未起始
//!   （now < start）占帧不应用。
//! - 死节点/代数不匹配/prop 与 kind 不匹配（Gap on Rect 等）→ 不应用
//!   但**动画仍完成移除**——防 anim_active 卡死 = 帧循环永不 Idle 的
//!   电量坑（R3 三态停帧的诚实前提）。
//! - 零 panic / 零分配：i128 插值中间量防 i32×u64 乘法溢出；父链累计
//!   i64 + 步数帽防环。

use crate::damage::{DamageSet, RectI};
use crate::desc::NodeId;
use crate::Node;

/// 槽索引空哨兵（§6 Node 注：NONE = u32::MAX）。
const NONE: u32 = u32::MAX;

/// 动画属性（§6 冻结）：Offset* = Transform 型；其余 Layout 型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimProp {
    OffsetX,
    OffsetY,
    Width,
    Height,
    Gap,
}

/// 缓动（v1 两档）：Linear 线性插值；StepEnd 恒持 from、终点跳 to
/// （数字翻转/步进指示类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    StepEnd,
}

/// 单条动画（§6 冻结字段——from/to: i32 内部坐标口径）。
#[derive(Debug, Clone, Copy)]
pub struct Anim {
    pub node: NodeId,
    pub prop: AnimProp,
    pub from: i32,
    pub to: i32,
    pub start_ns: u64,
    pub dur_ns: u64,
    pub easing: Easing,
}

/// NodeId 解包 (slot, gen)——gen<<16 | idx（§3.1）。
fn split_id(id: NodeId) -> (usize, u16) {
    ((id.0 & 0xFFFF) as usize, (id.0 >> 16) as u16)
}

fn live(nodes: &[Option<Node>], slot: usize, gen: u16) -> bool {
    match nodes.get(slot) {
        Some(Some(n)) => n.alive && n.gen == gen,
        _ => false,
    }
}

fn node_mut<'a>(nodes: &'a mut [Option<Node>], slot: usize, gen: u16) -> Option<&'a mut Node> {
    match nodes.get_mut(slot) {
        Some(opt) => match opt {
            Some(n) if n.alive && n.gen == gen => Some(n),
            _ => None,
        },
        None => None,
    }
}

/// i64 → i32 饱和收缩（§0 坐标铁律：显式 clamp 后 cast）。
fn sat_i64_to_i32(v: i64) -> i32 {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// 节点绝对界（含 ox/oy——damage 面）：沿父链累计 x/y/ox/oy（i64 防链式
/// i32 溢出），w/h 负值经 RectI::new 收口归零；父链环步数帽防御返 None。
pub(crate) fn abs_rect(nodes: &[Option<Node>], slot: usize) -> Option<RectI> {
    let n0 = match nodes.get(slot) {
        Some(Some(n)) if n.alive => n,
        _ => return None,
    };
    let (w, h) = (n0.w, n0.h);
    let mut x = 0i64;
    let mut y = 0i64;
    let mut cur = slot as u32;
    let mut steps = 0usize;
    loop {
        let n = match nodes.get(cur as usize) {
            Some(Some(n)) if n.alive => n,
            _ => return None,
        };
        x += n.x as i64 + n.ox as i64;
        y += n.y as i64 + n.oy as i64;
        if n.parent == NONE {
            break;
        }
        cur = n.parent;
        steps += 1;
        if steps > nodes.len() {
            return None; // 父链环防御
        }
    }
    Some(RectI::new(sat_i64_to_i32(x), sat_i64_to_i32(y), w, h))
}

/// 子树包络（含 ox/oy——Transform 动画的 damage 面）：自身界与全部后
/// 代界的包络。v1 无 clip：子视觉可溢出父界，偏移后**整个子树**随偏
/// 移走——只记节点自身界会漏子树尾影成残像素（修复轮 finding 10）。
/// 防御语义：子失败/死槽/链环**非致命**——已累计包络保留、异常分支
/// 丢弃（保守并入自身面，绝不 panic）；深度帽 > nodes.len() 返零矩形。
fn subtree_abs_rect(nodes: &[Option<Node>], slot: usize, depth: usize) -> RectI {
    if depth > nodes.len() {
        return RectI::new(0, 0, 0, 0); // 深度帽（环防御）
    }
    let Some(n0) = nodes.get(slot).and_then(|o| o.as_ref()) else {
        return RectI::new(0, 0, 0, 0);
    };
    if !n0.alive {
        return RectI::new(0, 0, 0, 0);
    }
    let mut acc = abs_rect(nodes, slot).unwrap_or(RectI::new(0, 0, 0, 0));
    let mut c = n0.first_child;
    let mut hops = 0usize;
    while c != NONE {
        hops += 1;
        if hops > nodes.len() {
            break; // 子链环防御：已累计包络保留
        }
        let ns = match nodes.get(c as usize) {
            Some(Some(ch)) if ch.alive => ch.next_sib,
            _ => break, // 链上死槽（结构异常）：保守停
        };
        acc = acc.envelope(&subtree_abs_rect(nodes, c as usize, depth + 1));
        c = ns;
    }
    acc
}

/// damage 入账：空界丢弃（gfx 坑 5 零面积 no-op 口径），i32→u32 负侧
/// 饱和由 `RectI::to_wire` 收口（§6 damage 冻结语义）。
fn damage_add(damage: &mut DamageSet, r: &RectI) {
    if !r.is_empty() {
        damage.add(r.to_wire());
    }
}

fn damage_envelope(damage: &mut DamageSet, before: Option<RectI>, after: Option<RectI>) {
    match (before, after) {
        (Some(b), Some(f)) => damage_add(damage, &b.envelope(&f)),
        (Some(b), None) => damage_add(damage, &b),
        (None, Some(f)) => damage_add(damage, &f),
        (None, None) => {}
    }
}

/// 区间求值（调用前提：start ≤ now < start+dur 且 dur > 0）。
/// Linear：i128 中间量（(to−from)×elapsed 可达 2^63×u64——i64 必溢，
/// i128 乘除后 clamp 回 i32）；StepEnd：恒持 from（终点值由完成分支
/// 钉板，不在此处跳变）。
fn eval_value(a: &Anim, now_ns: u64) -> i32 {
    let elapsed = now_ns - a.start_ns;
    match a.easing {
        Easing::Linear => {
            let span = (a.to as i64) - (a.from as i64);
            let v = (a.from as i128) + (span as i128) * (elapsed as i128) / (a.dur_ns as i128);
            v.clamp(i32::MIN as i128, i32::MAX as i128) as i32
        }
        Easing::StepEnd => a.from,
    }
}

/// 单条动画应用：分型落点（Transform → ox/oy + paint_dirty；Layout →
/// props + layout_dirty）。kind 与 prop 不匹配（Gap on 非 Col/Row、
/// Width/Height on 非 Rect）= no-op（动画仍按 tick 侧语义完成移除）。
fn apply(a: &Anim, v: i32, nodes: &mut [Option<Node>], damage: &mut DamageSet) {
    let (slot, gen) = split_id(a.node);
    if !live(nodes, slot, gen) {
        return; // 死节点/悬空 id：不应用——tick 侧仍完成移除（防卡死）
    }
    match a.prop {
        AnimProp::OffsetX | AnimProp::OffsetY => {
            // Transform 型：paint 偏移不重排；damage = 新旧**子树包络**
            //（v1 无 clip：子视觉随偏移走——自身界记账漏子树尾影，
            // §3.4 残像素对策 = subtree_abs_rect）。
            let before = subtree_abs_rect(nodes, slot, 0);
            if let Some(n) = node_mut(nodes, slot, gen) {
                match a.prop {
                    AnimProp::OffsetX => n.ox = v,
                    _ => n.oy = v,
                }
                n.paint_dirty = true;
            }
            let after = subtree_abs_rect(nodes, slot, 0);
            damage_envelope(damage, Some(before), Some(after));
        }
        AnimProp::Width | AnimProp::Height => {
            // Layout 型：Rect props 定值 + layout_dirty；damage 记旧界
            //（新界随 relayout 入账——§3.7 单一事实源在 layout 段）
            let before = abs_rect(nodes, slot);
            let mut changed = false;
            if let Some(n) = node_mut(nodes, slot, gen) {
                if let crate::desc::Props::Rect(rp) = &mut n.props {
                    // i32 → u32 显式 clamp（§0 坐标铁律：负宽无意义归 0）
                    let u = v.max(0) as u32;
                    if a.prop == AnimProp::Width {
                        rp.w = Some(u);
                    } else {
                        rp.h = Some(u);
                    }
                    n.layout_dirty = true;
                    changed = true;
                }
            }
            if changed {
                if let Some(b) = before {
                    damage_add(damage, &b);
                }
            }
        }
        AnimProp::Gap => {
            let before = abs_rect(nodes, slot);
            let mut changed = false;
            if let Some(n) = node_mut(nodes, slot, gen) {
                let g = v.max(0) as u32; // i32 → u32 显式 clamp（§0）
                match &mut n.props {
                    crate::desc::Props::Col(cp) => {
                        cp.gap = g;
                        n.layout_dirty = true;
                        changed = true;
                    }
                    crate::desc::Props::Row(rp) => {
                        rp.gap = g;
                        n.layout_dirty = true;
                        changed = true;
                    }
                    _ => {} // 非 Col/Row：no-op
                }
            }
            if changed {
                if let Some(b) = before {
                    damage_add(damage, &b);
                }
            }
        }
    }
}

/// 帧内推进（§6 冻结签名）：逐条求值应用；完成项（dur==0 或
/// now ≥ start+dur）应用精确 `to` 终点后移除；未起始项占帧跳过。
/// 返回 anim_active（仍有未完成项——喂帧循环信号 §3.8）。
pub(crate) fn tick(anims: &mut [Option<Anim>], nodes: &mut [Option<Node>], damage: &mut DamageSet, now_ns: u64) -> bool {
    let mut active = false;
    for i in 0..anims.len() {
        let Some(a) = anims[i] else {
            continue; // Anim: Copy——此处取值副本，槽位仍可写回 None
        };
        if now_ns < a.start_ns {
            active = true; // 已排程未起始：占帧不应用
            continue;
        }
        let end = a.start_ns.saturating_add(a.dur_ns);
        let done = a.dur_ns == 0 || now_ns >= end;
        let v = if done { a.to } else { eval_value(&a, now_ns) };
        apply(&a, v, nodes, damage);
        if done {
            anims[i] = None; // 完成项移除（§3.7）
        } else {
            active = true;
        }
    }
    active
}
