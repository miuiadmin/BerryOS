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

/// 动画显式分型（GUI-6 D14）：`Transform` = paint 偏移路径（不重排），
/// `Layout` = props 几何路径（重排）。[`kind_of`] 单源分派——加 AnimProp
/// 变体漏分类 = 编译错（exhaustive match 执法）。apply 按 kind 走 D1
/// 双路分派的本体在段E（feature s7）；本类型面先行落位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnimKind {
    Transform,
    Layout,
}

/// prop → kind 单源（D14 exhaustive match——新增 AnimProp 无臂 = 编译错）。
/// 段E 接线前无运行期消费者（pub(crate) 面无法自 tests/ 钉——钉板在
/// 本文件内联 cfg(test) 模块，ADR #60 钉板先行的 crate 内落点）。
#[allow(dead_code)]
pub(crate) fn kind_of(prop: AnimProp) -> AnimKind {
    match prop {
        AnimProp::OffsetX | AnimProp::OffsetY => AnimKind::Transform,
        AnimProp::Width | AnimProp::Height | AnimProp::Gap => AnimKind::Layout,
    }
}

/// 缓动（v1 两档 + GUI-6 D13 扩两档）：Linear 线性插值；StepEnd 恒持
/// from、终点跳 to（数字翻转/步进指示类）；QuadOut = `1-(1-t)²` /
/// CubicOut = `1-(1-t)³`（Q16.16 定点：i128 全程整数中间量，末尾 >>16
/// 回 i32——同纯整数纪律，禁浮点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    StepEnd,
    QuadOut,
    CubicOut,
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

/// S7 anim staging（§6/D1——B 面 cfg s7）：Layout 型动画 tick 期产
/// `(slot, ChangeMask)` 定容暂存，render_frame 在 anim apply 后、
/// relayout 前统一 `Ui::apply_change` 分派——动画漏斗与 set_props/
/// set_text 同一 dirty/damage 单源，无第三套语义。帽 =
/// [`crate::ANIM_STAGING_MAX`]（与动画槽帽同源——单 tick 应用数 ≤
/// 槽数 → 结构性不触帽；满 = debug_assert loud 防御面）。
#[cfg(feature = "s7")]
pub(crate) struct AnimStaging {
    pub items: [(u32, crate::mask::ChangeMask); crate::ANIM_STAGING_MAX],
    pub n: usize,
}

#[cfg(feature = "s7")]
impl AnimStaging {
    pub(crate) const fn new() -> Self {
        Self {
            items: [(0, crate::mask::ChangeMask::NONE); crate::ANIM_STAGING_MAX],
            n: 0,
        }
    }

    /// 定容入 staging（满 = loud 防御——结构性不可达，见类型注）。
    pub(crate) fn push(&mut self, slot: u32, mask: crate::mask::ChangeMask) {
        debug_assert!(self.n < self.items.len(), "anim staging 满（结构性不可达——单 tick ≤ ANIM_MAX 条）");
        if self.n < self.items.len() {
            self.items[self.n] = (slot, mask);
            self.n += 1;
        }
    }
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
/// GUI-6 D31：升 pub(crate)——`Ui::set_offset` 与 Transform 动画共用
/// 本路径（两条路同函数体，无第三套 dirty 语义）。
pub(crate) fn subtree_abs_rect(nodes: &[Option<Node>], slot: usize, depth: usize) -> RectI {
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

/// damage 入账（GUI-6 D17：经 viewport 交集单源 `layout::dmg_clip_add`
/// ——与 layout/lib 同语义，负界/越界折界内残片；零面积丢弃）。
fn damage_add(damage: &mut DamageSet, r: &RectI, vp: &RectI) {
    crate::layout::dmg_clip_add(damage, r, vp);
}

fn damage_envelope(
    damage: &mut DamageSet,
    before: Option<RectI>,
    after: Option<RectI>,
    vp: &RectI,
) {
    match (before, after) {
        (Some(b), Some(f)) => damage_add(damage, &b.envelope(&f), vp),
        (Some(b), None) => damage_add(damage, &b, vp),
        (None, Some(f)) => damage_add(damage, &f, vp),
        (None, None) => {}
    }
}

/// 缓动插值单源（D13）：`from` + span×ease(elapsed/dur)。Q16.16 定点
/// 全 i128 中间量——t = elapsed<<16/dur（最大 1.0<<16 = 65536，立方
/// ≈2.8e14 « i128 上限），多项式毕 >>16 回值域。`elapsed > dur` 钳到
/// dur（SwipePager tick 侧收尾复用；eval 调用面 tick 已保证界内——
/// Linear 数值路径与 v1 逐位同式零漂）。StepEnd 恒 from（终点跳变由
/// tick 完成分支钉板）。
pub(crate) fn interp(easing: Easing, from: i32, to: i32, elapsed_ns: u64, dur_ns: u64) -> i32 {
    let e = if dur_ns == 0 {
        0 // 除零防御：视作零进度（完成跳变由 tick 完成分支承担）
    } else {
        elapsed_ns.min(dur_ns)
    };
    let span = (to as i64) - (from as i64);
    match easing {
        Easing::Linear => {
            let v = (from as i128) + (span as i128) * (e as i128) / (dur_ns as i128);
            v.clamp(i32::MIN as i128, i32::MAX as i128) as i32
        }
        Easing::StepEnd => from,
        Easing::QuadOut | Easing::CubicOut => {
            // t/inv = Q16.16（65536 = 1.0）；多项式平方/立方后 >>16/>>32
            // 折回 Q16.16；ease = 65536 - poly(inv)
            let t = ((e as i128) << 16) / (dur_ns as i128);
            let inv: i128 = 65_536 - t;
            let eased = if easing == Easing::QuadOut {
                65_536 - ((inv * inv) >> 16)
            } else {
                65_536 - ((inv * inv * inv) >> 32)
            };
            let v = (from as i128) + (((span as i128) * eased) >> 16);
            v.clamp(i32::MIN as i128, i32::MAX as i128) as i32
        }
    }
}

/// 区间求值（调用前提：start ≤ now < start+dur 且 dur > 0——完成跳变
/// 由 tick 完成分支钉板精确 `to`）。
fn eval_value(a: &Anim, now_ns: u64) -> i32 {
    interp(a.easing, a.from, a.to, now_ns - a.start_ns, a.dur_ns)
}

/// 单条动画应用：分型落点（Transform → ox/oy + paint_dirty；Layout →
/// props + layout_dirty）。kind 与 prop 不匹配（Gap on 非 Col/Row、
/// Width/Height on 非 Rect）= no-op（动画仍按 tick 侧语义完成移除）。
/// `vp` = D17 clip 面（damage 经 viewport 交集收口）。
/// B 面（S7 段E D1）：Layout 型改产 `(slot, mask)` 入 `staging`——
/// dirty/damage 延后至 render_frame 期 apply_change 单源分派（时序 =
/// 本函数返回后、relayout 前）；Transform 型两面同路径直写。
#[allow(clippy::too_many_arguments)]
fn apply(
    a: &Anim,
    v: i32,
    nodes: &mut [Option<Node>],
    damage: &mut DamageSet,
    vp: &RectI,
    #[cfg(feature = "s7")] staging: &mut AnimStaging,
) {
    let (slot, gen) = split_id(a.node);
    if !live(nodes, slot, gen) {
        return; // 死节点/悬空 id：不应用——tick 侧仍完成移除（防卡死）
    }
    match a.prop {
        AnimProp::OffsetX | AnimProp::OffsetY => {
            // Transform 型：paint 偏移不重排；damage = 新旧**子树包络**
            //（v1 无 clip：子视觉随偏移走——自身界记账漏子树尾影，
            // §3.4 残像素对策 = subtree_abs_rect；包络经 D17 viewport
            // clip 折界内残片）。
            let before = subtree_abs_rect(nodes, slot, 0);
            if let Some(n) = node_mut(nodes, slot, gen) {
                match a.prop {
                    AnimProp::OffsetX => n.ox = v,
                    _ => n.oy = v,
                }
                n.paint_dirty = true;
            }
            let after = subtree_abs_rect(nodes, slot, 0);
            damage_envelope(damage, Some(before), Some(after), vp);
        }
        AnimProp::Width | AnimProp::Height => {
            // Layout 型（A 面）：Rect props 定值 + layout_dirty；damage 记
            // 旧界（新界随 relayout 入账——§3.7 单一事实源在 layout 段）。
            // B 面（D1）：只写 props + 入 staging（Width→W / Height→H）
            // ——dirty/damage 统一走 apply_change，无直写旁路。
            #[cfg(not(feature = "s7"))]
            {
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
                        damage_add(damage, &b, vp);
                    }
                }
            }
            #[cfg(feature = "s7")]
            {
                if let Some(n) = node_mut(nodes, slot, gen) {
                    if let crate::desc::Props::Rect(rp) = &mut n.props {
                        let u = v.max(0) as u32; // i32 → u32 显式 clamp（§0）
                        let m = if a.prop == AnimProp::Width {
                            rp.w = Some(u);
                            crate::mask::ChangeMask::W
                        } else {
                            rp.h = Some(u);
                            crate::mask::ChangeMask::H
                        };
                        staging.push(slot as u32, m);
                    }
                }
            }
        }
        AnimProp::Gap => {
            #[cfg(not(feature = "s7"))]
            {
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
                        // 设计契约（§5 anim 契约行——R2-6 立法）：Gap 作用域
                        // = Col/Row；Grid（及其余 kind）= no-op 完成语义
                        // （动画照常结束移除、gap 不动）。扩臂支持 Grid =
                        // 后续刀（与 set_props 的 GAP 位路径对齐时一并入册）。
                        _ => {}
                    }
                }
                if changed {
                    if let Some(b) = before {
                        damage_add(damage, &b, vp);
                    }
                }
            }
            #[cfg(feature = "s7")]
            {
                // B 面（D1）：Col/Row gap 定值 + 入 staging（GAP 位）；
                // 非 Col/Row no-op 保持（kind 失配测同语义——不产 staging）。
                if let Some(n) = node_mut(nodes, slot, gen) {
                    let g = v.max(0) as u32; // i32 → u32 显式 clamp（§0）
                    let hit = match &mut n.props {
                        crate::desc::Props::Col(cp) => {
                            cp.gap = g;
                            true
                        }
                        crate::desc::Props::Row(rp) => {
                            rp.gap = g;
                            true
                        }
                        // Gap 作用域 = Col/Row（§5 anim 契约行——R2-6 立法）：
                        // Grid no-op 完成语义，不产 staging（与 A 面同口径）。
                        _ => false,
                    };
                    if hit {
                        staging.push(slot as u32, crate::mask::ChangeMask::GAP);
                    }
                }
            }
        }
    }
}

/// 帧内推进（§6 冻结签名 + GUI-6 D17 vp 进参）：逐条求值应用；完成项
/// （dur==0 或 now ≥ start+dur）应用精确 `to` 终点后移除；未起始项占
/// 帧跳过。返回 anim_active（仍有未完成项——喂帧循环信号 §3.8）。
/// `vp_w/vp_h` = damage clip 面（viewport 矩形单次构造）。
pub(crate) fn tick(
    anims: &mut [Option<Anim>],
    nodes: &mut [Option<Node>],
    damage: &mut DamageSet,
    now_ns: u64,
    vp_w: u32,
    vp_h: u32,
    #[cfg(feature = "s7")] staging: &mut AnimStaging,
) -> bool {
    let vp = RectI::new(
        0,
        0,
        crate::damage::u32_to_i32_sat(vp_w),
        crate::damage::u32_to_i32_sat(vp_h),
    );
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
        #[cfg(feature = "s7")]
        apply(&a, v, nodes, damage, &vp, staging);
        #[cfg(not(feature = "s7"))]
        apply(&a, v, nodes, damage, &vp);
        if done {
            anims[i] = None; // 完成项移除（§3.7）
        } else {
            active = true;
        }
    }
    active
}

#[cfg(test)]
mod tests {
    // D14 kind_of 钉板（§11 草案钉板清单段D 项）：pub(crate) 面无法自
    // tests/（外部集成测试 crate）钉——crate 内联落点（ADR #60 钉板
    // 先行的编译面：加 AnimProp 漏分类 = 本 match 编译错）。
    use super::*;

    #[test]
    fn kind_of_classifies_all_props() {
        assert_eq!(kind_of(AnimProp::OffsetX), AnimKind::Transform);
        assert_eq!(kind_of(AnimProp::OffsetY), AnimKind::Transform);
        assert_eq!(kind_of(AnimProp::Width), AnimKind::Layout);
        assert_eq!(kind_of(AnimProp::Height), AnimKind::Layout);
        assert_eq!(kind_of(AnimProp::Gap), AnimKind::Layout);
    }
}
