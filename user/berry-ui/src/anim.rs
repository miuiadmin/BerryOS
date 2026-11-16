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
//! - [`SpringProp`]（维三最小刀）= **定点弹簧第二动画面**：质量-刚度-
//!   阻尼离散积分（Q16.16、半隐式 Euler、子步帽）、初速度入参、重定
//!   目标以当前状态续接、双阈值收敛吸附——与既有五属性四档缓动零
//!   交叠（见弹簧节首注）。

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

// ---------- 弹簧（维三最小刀——UI-02 维三裁决两刀之一） ----------
//
// 与 `Anim`（from/to/start/dur/easing 定长时间线）平行的**第二动画面**：
// 定点弹簧 SpringProp——质量-刚度-阻尼离散积分（Q16.16 纯整数，同
// interp 的 i128 全程中间量纪律）。覆盖 OH `curves.springMotion` 与
// iOS `interpolatingSpring` 的主干：初速度入参（速度环 96ms 窗差分
// 输出 px/s 可直入）+ 重定目标以当前位置/速度续接（= iOS
// velocity/shouldMerge 与 OH 终点变化自动接管的主干等价物）。
//
// 不扩 `AnimProp` 枚举本体：AnimProp 语义 = 「驱动哪个属性」（五属性
// 之一），Spring 不是属性而是**驱动方式**——无属性面的变体在
// `kind_of`/`apply` 的 exhaustive match 纪律下只能落死臂（递归载荷又
// 需 Box，热路径禁）。挂载面 = lib.rs 弹簧表（`spring_add`，与 anims
// 表同帽同错）+ `SpringProp.prop` 指认五属性之一经同一 `apply` 落点。

/// 弹簧刚度/阻尼预设两档（质量恒 1——k/c 已按 m=1 折算；ω = 无阻尼
/// 固有频率，ζ = 阻尼比，k=ω²、c=2ζω）：
/// - [`SpringPreset::Smooth`] = **临界阻尼**（ω=12、ζ=1.0 → k=144、
///   c=24，收敛≈0.5s）：无过冲单调收敛——OH `curves.springMotion`
///   默认档（response≈0.55s、dampingFraction≈0.83 近临界）与 iOS
///   `.smooth` 的共同气质；翻页/吸附主干。
/// - [`SpringPreset::Bouncy`] = **欠阻尼**（ω=16、ζ=0.625 → k=256、
///   c=20，过冲≈8%）：iOS `.bouncy` 气质；按压回弹主干。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpringPreset {
    Smooth,
    Bouncy,
}

impl SpringPreset {
    /// 刚度 k（Q16.16，1/s²）。
    const fn stiffness_q(self) -> i64 {
        match self {
            SpringPreset::Smooth => 144 << 16,
            SpringPreset::Bouncy => 256 << 16,
        }
    }

    /// 阻尼 c（Q16.16，1/s）。
    const fn damping_q(self) -> i64 {
        match self {
            SpringPreset::Smooth => 24 << 16,
            SpringPreset::Bouncy => 20 << 16,
        }
    }
}

/// 积分子步（16ms = 60Hz 帧步）。半隐式 Euler 稳定域 dt·ω < 2——两档
/// ω ≤ 16 → 子步上限 125ms，取 16ms 留 8 倍裕量。
const SPRING_SUBSTEP_NS: u64 = 16_000_000;

/// 单 tick 子步帽：16 步 = 256ms 实际物理推进——长帧/时钟大跳走**慢
/// 动作追赶**而非爆积分（确定性：同 now 序列同子步序列，无随机）。
const SPRING_SUBSTEP_MAX: u32 = 16;

/// 终止判据——位移阈（Q16.16：0.5px，亚像素不可见）。
const SPRING_POS_EPS_Q: i64 = 32_768;

/// 终止判据——速度阈（Q16.16：20px/s，低于可感知平移率）。
const SPRING_VEL_EPS_Q: i64 = 20 << 16;

/// 定点弹簧条目（弹簧表元素——lib.rs `spring_add` 入表；SwipePager
/// 自管路径（同 `anim::interp` 单源复用风格）直接持一条步进）。
/// 终止判据 = 位移与速度**双阈值**收敛后吸附目标（`done` 置位、
/// `value()` 返精确 target——与 anim::tick 完成帧「终点钉板」同语义）。
#[derive(Debug, Clone, Copy)]
pub struct SpringProp {
    /// 驱动节点（引擎表挂载面经 `apply` 分派；死节点 no-op 但照常
    /// 收敛移除——与 anim 表同纪律，防 anim_active 卡死）。
    pub node: NodeId,
    /// 驱动属性（既有五属性之一——弹簧是驱动方式非属性，见节首注）。
    pub prop: AnimProp,
    /// 目标值（重定目标 = `retarget`——以当前位置/速度重起算）。
    pub target: i32,
    /// 刚度/阻尼预设两档。
    pub preset: SpringPreset,
    /// 位置积分态（Q16.16 px）。
    pos_q: i64,
    /// 速度积分态（Q16.16 px/s——重定目标续接的接口态）。
    vel_q: i64,
    /// 上次步进时刻（None = 未起拍：首次 step 定基线、dt=0 持初值
    /// ——与 SwipePager 吸附「起拍 = 首次 tick 的 now_ns」同式）。
    last_ns: Option<u64>,
    /// 收敛完成旗（吸附目标后置位；retarget 复位续跑）。
    done: bool,
}

impl SpringProp {
    /// 构造：`from` = 当前位置、`target` = 目标值、`vel0_px_s` = 初速度
    /// （速度环 96ms 窗差分输出 px/s 可直入——hit.rs `velocity_at_end`
    /// 同单位）、`preset` = 刚度/阻尼两档。
    pub fn new(
        node: NodeId,
        prop: AnimProp,
        from: i32,
        target: i32,
        vel0_px_s: i32,
        preset: SpringPreset,
    ) -> Self {
        Self {
            node,
            prop,
            target,
            preset,
            pos_q: (from as i64) << 16,
            vel_q: (vel0_px_s as i64) << 16,
            last_ns: None,
            done: false,
        }
    }

    /// 重定目标：**以当前位置与当前速度重起算**——只换 target，位置/
    /// 速度积分态原样续接（中断恢复语义的主干等价物：iOS
    /// velocity/shouldMerge 合并、OH 终点变化自动接管）。已收敛条目
    /// 复活续跑。
    pub fn retarget(&mut self, target: i32) {
        self.target = target;
        self.done = false;
    }

    /// 步进（dt = 与上次 step 之差；首步 dt=0 持初值起拍）：半隐式
    /// Euler 子步积分，返回当前整值位置。收敛（双阈值）→ 吸附目标。
    pub fn step(&mut self, now_ns: u64) -> i32 {
        let last = match self.last_ns {
            None => {
                self.last_ns = Some(now_ns);
                return self.value(); // 起拍帧：dt=0 持初值
            }
            Some(t) => t,
        };
        let mut remain = now_ns.saturating_sub(last); // 时钟回拨 → 0
        self.last_ns = Some(now_ns);
        let mut steps = 0u32;
        while remain > 0 && steps < SPRING_SUBSTEP_MAX && !self.done {
            let dt = remain.min(SPRING_SUBSTEP_NS);
            self.integrate(dt);
            remain -= dt;
            steps += 1;
        }
        self.value()
    }

    /// 收敛完成旗（双阈值满足 + 吸附目标）。
    pub fn done(&self) -> bool {
        self.done
    }

    /// 当前整值位置（Q16.16 四舍五入 → i32 饱和——§0 坐标铁律收口）。
    pub fn value(&self) -> i32 {
        sat_q_to_i32(self.pos_q)
    }

    /// 当前速度读出（px/s 整值——重定目标/续接调试面）。
    pub fn vel_px_s(&self) -> i32 {
        sat_q_to_i32(self.vel_q)
    }

    /// 单子步积分（Q16.16 全 i128 中间量；pos/vel 饱和钳回 i64——
    /// 极端参数（i32 端点/巨速度）不 panic 只钳制）：
    /// `a = -k·(x - target) - c·v`（m=1 折入常量）→ 先速度后位置。
    fn integrate(&mut self, dt_ns: u64) {
        let k = self.preset.stiffness_q() as i128;
        let c = self.preset.damping_q() as i128;
        let target_q = ((self.target as i64) as i128) << 16;
        let dt_q = ((dt_ns as i128) << 16) / 1_000_000_000; // Q16.16 秒
        let disp = (self.pos_q as i128) - target_q;
        let accel = -((k * disp) >> 16) - ((c * self.vel_q as i128) >> 16);
        let vel_new = (self.vel_q as i128) + ((accel * dt_q) >> 16);
        let pos_new = (self.pos_q as i128) + ((vel_new * dt_q) >> 16);
        self.vel_q = sat_i128_to_i64(vel_new);
        self.pos_q = sat_i128_to_i64(pos_new);
        // 终止判据：位移与速度双阈值 → 吸附目标（终点钉板）。
        let d = (self.pos_q as i128) - target_q;
        if d.abs() <= SPRING_POS_EPS_Q as i128 && self.vel_q.abs() <= SPRING_VEL_EPS_Q {
            self.pos_q = target_q as i64; // |target|≤i32::MAX → <<16 不溢 i64
            self.vel_q = 0;
            self.done = true;
        }
    }
}

/// Q16.16 → i32 饱和（四舍五入到最近整数；i128 中间量防 +32768 溢出）。
fn sat_q_to_i32(q: i64) -> i32 {
    let v = ((q as i128) + 32_768) >> 16;
    v.clamp(i32::MIN as i128, i32::MAX as i128) as i32
}

/// i128 → i64 饱和（积分态钳制——溢出负例不 panic）。
fn sat_i128_to_i64(v: i128) -> i64 {
    v.clamp(i64::MIN as i128, i64::MAX as i128) as i64
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

/// 弹簧表帧内推进（与 `tick` 同帧同时钟；空表全 None 零动静）：逐条
/// `SpringProp::step` 子步积分 → 经同一 `apply` 单源落点（临时 `Anim`
/// 只借 node/prop 分派面——apply 不读 from/to/dur）；收敛项吸附目标
/// 后移除；返回 active（弹簧在飞 = 帧循环 RUN 保持——同 tick 语义）。
/// 死节点 no-op 但照常收敛移除（防 anim_active 卡死的同纪律）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn spring_tick(
    springs: &mut [Option<SpringProp>],
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
    for i in 0..springs.len() {
        let Some(sp) = &mut springs[i] else {
            continue;
        };
        let v = sp.step(now_ns);
        let a = Anim {
            node: sp.node,
            prop: sp.prop,
            from: 0,
            to: sp.target,
            start_ns: 0,
            dur_ns: 0,
            easing: Easing::Linear, // 供 s7 staging 分派——apply 不读缓动面
        };
        #[cfg(feature = "s7")]
        apply(&a, v, nodes, damage, &vp, staging);
        #[cfg(not(feature = "s7"))]
        apply(&a, v, nodes, damage, &vp);
        if sp.done() {
            springs[i] = None; // 收敛吸附后移除（终点钉板）
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
