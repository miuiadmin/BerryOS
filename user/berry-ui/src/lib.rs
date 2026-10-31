//! berry-ui：声明式 UI 引擎内核——保留树 arena + 布局 + 脏区合并 +
//! 遮挡剔除渲染 + 输入手势 referee + 动画表 + 三态停帧帧循环 + 双轨遥测
//! （GUI-3，`docs/GUI-3实施设计草案.md` §6 冻结契约；ADR #52 自研
//! berry-ui 声明式框架 / ADR #53 三态停帧 RUN-SETTLE-IDLE）。
//!
//! # 拓扑（§2）
//!
//! 引擎**只产出** damage 与 DisplayList、**只消费** typed 事件——双依赖：
//! `berry-gfx`（渲染消费面：CommandEncoder/GlyphPos/Color/FontSize——
//! DisplayList 的 `'a` 钉在**调用方 scratch 与 `'static` 图集**上，不钉
//! 引擎，树与列表解耦无自引用，gfx 坑 1/2 对策）+ `berry-guiproto`
//! （常量/类型单源非收发：Rect(u32)/DAMAGE_MAX/RESAMPLE_*/SCENE_*；
//! wire 编解码/channel/VMO 全在 embedder——引擎不假设直摸 vblank/hdi
//! 域，对齐条 19）。**embedder = GUI-5 留位**（guest 侧 gui-wm/gfxdemo
//! 接驳挂账不在本刀；host 侧 embedder = tests/ 离屏 [u8] 纹理对拍）。
//!
//! # 纪律铁律（§0）
//!
//! - 全库零 alloc、零 panic：定容数组满 → loud `Err` 绝不静默丢；敌意
//!   几何防御采样/checked/饱和。
//! - **坐标三套显式换算**（gfx 坑 10）：内部几何 i32（`damage::RectI`）/
//!   damage 与输入面 u32（guiproto `Rect` 同口径）/容量 usize——跨界
//!   一律显式转换 + checked，禁裸 `as`（u32→i32 负侧饱和、i64→u32
//!   clamp 收口）。
//! - 单线程语境立法：`Ui` 一律 `&mut self` 方法，无 Cell 无 Sync。
//! - request-driven 落法：引擎不持定时器——`next_wakeup() -> Option<u64>`
//!   交 embedder 睡/等事件；`None` = IDLE（撤软 vsync 定时器，R3 反面
//!   清单对策）。
#![cfg_attr(not(test), no_std)]

pub mod anim;
pub mod damage;
pub mod dep;
pub mod desc;
pub mod frame;
pub mod hit;
pub mod layout;
pub mod paint;
pub mod resample;
pub mod telemetry;
// arena 内嵌本文件（Node/槽表私有——§6）；NodeId 在 desc.rs 公开。

// 扁平面 re-export：门面签名里不限定模块路径的名字（§6 用法对齐）。
pub use desc::{AstError, Msg, MsgBuf, NodeId, Props, TextBuf, ViewKind, ViewSpec};

use berry_gfx::cmd::{CommandEncoder, GlyphPos};
use berry_guiproto::{FRAME_DISCARDED, FRAME_PRESENTED, PHASE_MOVE};
use resample::Sample;

// ---------- 常量族（§6 冻结） ----------

pub const MAX_NODES: usize = 256;
pub const TEXT_INLINE: usize = 64;
pub const MSG_MAX: usize = 8;
pub const ANIM_MAX: usize = 16;
pub const INPUT_BUF_MAX: usize = 8;
pub const DEP_EDGES_MAX: usize = 64;
pub const DRAG_SLOP_PX: i32 = 8;
pub const OVERDRAW_BUDGET_X256: u32 = 640; // 2.5×256（GUI-10 反清单 7）
pub const IDLE_AFTER_MS: u64 = 2000; // OH 常量：静默强降 IDLE
pub type UiMain = Ui<MAX_NODES>;

/// 默认软 vsync 周期（≈60Hz）——`set_vsync` 前的 rate_hint/expected 轨
/// 兜底值（协商倒数注入前的确定值，embedder 可覆写）。
const DEFAULT_VSYNC_PERIOD_NS: u64 = 16_666_666;

/// 槽索引空值（§3.1——`u32::MAX`；pub(crate) 供各子模组树走查共用）。
pub(crate) const NONE: u32 = u32::MAX;

// ---------- 错误族（§6 冻结） ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiError {
    /// arena 定容满——loud。
    NodeFull,
    /// 代数不匹配（悬空 NodeId）/坏槽。
    NodeBad,
    /// 容器位传叶子 / Root 不能为子。
    BadParent,
    /// > `TEXT_INLINE`。
    TextTooLong { len: usize },
    /// 定容族。
    AnimFull,
    DepFull,
    MsgFull,
    /// 需先 `root()`。
    NoRoot,
}

// ---------- 节点内部形态（§6 冻结字段——impl 三方共享） ----------

/// 保留树节点（lib.rs 私有 arena 的槽内容；layout/paint/anim 子模组经
/// `pub(crate)` 共享——冻结字段，一字不改）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Node {
    pub gen: u16,
    pub alive: bool,
    pub kind: desc::ViewKind,
    pub key: u64,
    pub props: desc::Props,
    pub parent: u32,
    pub first_child: u32,
    pub next_sib: u32,
    pub prev_sib: u32, // 槽索引；NONE = u32::MAX
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32, // 父相对摆位 + 尺寸（layout 产物）
    pub meas_w: u32,
    pub meas_h: u32, // 内在尺寸缓存（Measure 保留）
    pub layout_dirty: bool,
    pub paint_dirty: bool,
    pub ox: i32,
    pub oy: i32, // paint 偏移（Transform 动画）
}

// ---------- Ui 门面 ----------

/// 引擎门面：保留树 arena（显式 free 链——防「分配器线性扫骰子」K-1
/// 雷同型）+ 布局 + 脏区 + 输入缓冲/手势态 + 动画表 + 三态帧循环 +
/// 遥测环 + 依赖边 + vsync 协商倒数 + credits 簿记。全 `&mut self`
/// 单线程语境（§2 立法）。
pub struct Ui<const N: usize> {
    nodes: [Option<Node>; N],
    gens: [u16; N], // 每槽代数（free 时 +1——悬空 id 判据单源）
    free_stack: [u32; N],
    free_count: usize,
    root_slot: u32,
    vp_w: u32,
    vp_h: u32,
    damage: damage::DamageSet,
    input_buf: [Option<hit::PtrEvent>; INPUT_BUF_MAX],
    input_n: usize,
    gesture: hit::Gesture,
    move_hist: [Sample; INPUT_BUF_MAX], // 活跃 MOVE 滑窗（重采样消费）
    hist_len: usize,
    anims: [Option<anim::Anim>; ANIM_MAX],
    phase: frame::FramePhase,
    last_work_ns: u64,
    last_now_ns: u64, // next_wakeup 的「当前时刻」锚（引擎无钟——最近帧时刻）
    vsync_period_ns: u64,
    vsync_phase_ns: u64,
    credits: frame::Credits, // 背压簿记单源 frame.rs（outstanding 口径）
    inflight: [(u32, u64); 2], // (seq, expected_ns)——CREDITS_MAX=2 在途位
    inflight_n: usize,
    seq: u32,
    last_input_ts_ns: u64,
    last_frame_seq: u32,
    last_expected_ns: u64,
    spinup_anchor: Option<u64>, // wake 时刻的下一 tick（起振延迟遥测锚）
    deps: dep::DepTable, // 依赖边表单源 dep.rs（幂等去重 + unbind/clear）
    telem: telemetry::Telemetry,
}

// —— 内部小工具（零 panic：全 checked/饱和/先验界内） ——

fn pack_id(idx: usize, gen: u16) -> NodeId {
    NodeId(((gen as u32) << 16) | ((idx as u32) & 0xFFFF))
}

fn clamp_u32(v: i64) -> u32 {
    if v < 0 {
        0
    } else if v > u32::MAX as i64 {
        u32::MAX
    } else {
        v as u32
    }
}

/// props 几何等价判定（纯着色变 → 0 重排——Measure/Render 保留的执法点）。
fn props_geom_equal(a: &Props, b: &Props) -> bool {
    match (a, b) {
        (Props::Root, Props::Root) => true,
        (Props::Rect(x), Props::Rect(y)) => x.w == y.w && x.h == y.h,
        (Props::Text(x), Props::Text(y)) => {
            x.style.scale == y.style.scale && x.text.as_bytes() == y.text.as_bytes()
        }
        (Props::Col(x), Props::Col(y)) => x.gap == y.gap,
        (Props::Row(x), Props::Row(y)) => x.gap == y.gap,
        (Props::Pad(x), Props::Pad(y)) => x.all == y.all,
        _ => false, // 变体切换按几何变处理（保守重排）
    }
}

impl<const N: usize> Ui<N> {
    pub fn new(viewport_w: u32, viewport_h: u32) -> Self {
        let mut free_stack = [0u32; N];
        let mut i = 0usize;
        while i < N {
            free_stack[i] = N as u32 - 1 - i as u32; // 栈顶 = 槽 0
            i += 1;
        }
        Ui {
            nodes: [None; N],
            gens: [0; N],
            free_stack,
            free_count: N,
            root_slot: NONE,
            vp_w: viewport_w,
            vp_h: viewport_h,
            damage: damage::DamageSet::new(),
            input_buf: [None; INPUT_BUF_MAX],
            input_n: 0,
            gesture: hit::Gesture::new(),
            move_hist: [Sample { ts_ns: 0, x: 0, y: 0 }; INPUT_BUF_MAX],
            hist_len: 0,
            anims: [None; ANIM_MAX],
            phase: frame::FramePhase::Idle,
            last_work_ns: 0,
            last_now_ns: 0,
            vsync_period_ns: DEFAULT_VSYNC_PERIOD_NS,
            vsync_phase_ns: 0,
            credits: frame::Credits::new(),
            inflight: [(0, 0); 2],
            inflight_n: 0,
            seq: 0,
            last_input_ts_ns: 0,
            last_frame_seq: 0,
            last_expected_ns: 0,
            spinup_anchor: None,
            deps: dep::DepTable::new(),
            telem: telemetry::Telemetry::new(),
        }
    }

    // —— arena 内部（显式 free 链 + 代数） ——

    fn alloc_slot(&mut self) -> Option<usize> {
        if self.free_count == 0 {
            return None;
        }
        self.free_count -= 1;
        let idx = self.free_stack[self.free_count] as usize;
        if idx >= N {
            return None;
        }
        Some(idx)
    }

    fn free_slot(&mut self, idx: usize) {
        if idx >= N {
            return;
        }
        self.nodes[idx] = None;
        self.gens[idx] = self.gens[idx].wrapping_add(1); // 旧 id 全悬空
        if self.free_count < N {
            self.free_stack[self.free_count] = idx as u32;
            self.free_count += 1;
        }
    }

    fn slot_of(&self, id: NodeId) -> Result<usize, UiError> {
        let idx = (id.0 & 0xFFFF) as usize;
        let gen = (id.0 >> 16) as u16;
        if idx >= N {
            return Err(UiError::NodeBad);
        }
        match self.nodes[idx].as_ref() {
            Some(n) if n.alive && n.gen == gen => Ok(idx),
            _ => Err(UiError::NodeBad),
        }
    }

    fn child_tail(&self, parent_idx: usize) -> u32 {
        let Some(p) = self.nodes[parent_idx].as_ref() else { return NONE };
        let mut c = p.first_child;
        while c != NONE {
            let Some(n) = self.nodes.get(c as usize).and_then(|o| o.as_ref()) else { return NONE };
            if n.next_sib == NONE {
                return c;
            }
            c = n.next_sib;
        }
        NONE
    }

    fn link_child(&mut self, parent_idx: usize, child: u32) {
        let tail = self.child_tail(parent_idx);
        if tail == NONE {
            if let Some(p) = self.nodes[parent_idx].as_mut() {
                p.first_child = child;
            }
        } else if let Some(t) = self.nodes[tail as usize].as_mut() {
            t.next_sib = child;
        }
        if let Some(c) = self.nodes[child as usize].as_mut() {
            c.prev_sib = tail;
        }
    }

    fn unlink(&mut self, idx: usize) {
        let (parent, prev, next) = match self.nodes[idx].as_ref() {
            Some(n) => (n.parent, n.prev_sib, n.next_sib),
            None => return,
        };
        if parent != NONE {
            if let Some(p) = self.nodes[parent as usize].as_mut() {
                if p.first_child == idx as u32 {
                    p.first_child = next;
                }
            }
        }
        if prev != NONE {
            if let Some(p) = self.nodes[prev as usize].as_mut() {
                p.next_sib = next;
            }
        }
        if next != NONE {
            if let Some(n) = self.nodes[next as usize].as_mut() {
                n.prev_sib = prev;
            }
        }
    }

    /// 兄弟链递归收集（子树槽清单；容量帽 = 清单长，满即止——活树 ≤ N 恒不触）。
    fn collect_sibs(&self, head: u32, out: &mut [u32], n: &mut usize) {
        let mut c = head;
        while c != NONE {
            let Some(node) = self.nodes.get(c as usize).and_then(|o| o.as_ref()) else { return };
            let (fc, ns) = (node.first_child, node.next_sib);
            if *n < out.len() {
                out[*n] = c;
                *n += 1;
            }
            self.collect_sibs(fc, out, n);
            c = ns;
        }
    }

    fn mark_ancestors_dirty(&mut self, mut slot: u32) {
        let mut hops = 0usize;
        while slot != NONE && hops <= N {
            let idx = slot as usize;
            let Some(n) = self.nodes.get_mut(idx).and_then(|o| o.as_mut()) else { return };
            n.layout_dirty = true;
            slot = n.parent;
            hops += 1;
        }
    }

    /// 绝对原点（含沿途 ox/oy paint 偏移——命中随视觉走，§3.6）。
    fn abs_origin(&self, idx: usize) -> (i32, i32) {
        let mut ax = 0i32;
        let mut ay = 0i32;
        let mut cur = idx as u32;
        let mut hops = 0usize;
        while hops <= N {
            let Some(n) = self.nodes.get(cur as usize).and_then(|o| o.as_ref()) else { break };
            ax = ax.saturating_add(n.x).saturating_add(n.ox);
            ay = ay.saturating_add(n.y).saturating_add(n.oy);
            if n.parent == NONE {
                break;
            }
            cur = n.parent;
            hops += 1;
        }
        (ax, ay)
    }

    fn abs_rect_of(&self, idx: usize) -> damage::RectI {
        let (ax, ay) = self.abs_origin(idx);
        match self.nodes[idx].as_ref() {
            Some(n) => damage::RectI::new(ax, ay, n.w, n.h),
            None => damage::RectI::new(0, 0, 0, 0),
        }
    }

    fn damage_rect(&mut self, r: damage::RectI) {
        // §3.4 add 流程：先 clip 到 viewport（负界/越界折界内残片；零面
        // 积 no-op 丢弃——gfx 坑 5 收口在此，wire 化后恒 ≥0）。
        let vp = damage::RectI::new(
            0,
            0,
            damage::u32_to_i32_sat(self.vp_w),
            damage::u32_to_i32_sat(self.vp_h),
        );
        let c = r.intersect(&vp);
        if !c.is_empty() {
            self.damage.add(c.to_wire());
        }
    }

    fn reset_arena(&mut self) {
        let mut i = 0usize;
        while i < N {
            self.nodes[i] = None;
            self.gens[i] = self.gens[i].wrapping_add(1);
            self.free_stack[i] = N as u32 - 1 - i as u32;
            i += 1;
        }
        self.free_count = N;
        self.root_slot = NONE;
        let mut i = 0usize;
        while i < ANIM_MAX {
            self.anims[i] = None;
            i += 1;
        }
        self.deps.clear(); // 旧树 NodeId 全悬空——边表整清（dep 表侧单源）
        self.input_n = 0;
        self.hist_len = 0;
        self.gesture = hit::Gesture::new(); // 旧树 NodeId 全悬空——手势态随之归零
    }

    // —— 视口 ——

    pub fn set_viewport(&mut self, w: u32, h: u32) {
        self.vp_w = w;
        self.vp_h = h;
        let mut i = 0usize;
        while i < N {
            if let Some(n) = self.nodes[i].as_mut() {
                n.layout_dirty = true;
            }
            i += 1;
        }
        self.damage.add_full();
    }

    pub fn viewport(&self) -> (u32, u32) {
        (self.vp_w, self.vp_h)
    }

    // —— 树构造（builder 入口①） ——

    pub fn root(&mut self) -> NodeId {
        // 单根幂等。
        if self.root_slot != NONE {
            if let Some(n) = self.nodes[self.root_slot as usize].as_ref() {
                return pack_id(self.root_slot as usize, n.gen);
            }
        }
        // 冻结签名非 Result：容量哨兵 = NodeId(u32::MAX)（N=0/满场——
        // builder 面不可达，loud 哨兵供调用方 kind_of 验证）。
        let Some(idx) = self.alloc_slot() else { return NodeId(NONE) };
        let gen = self.gens[idx];
        self.nodes[idx] = Some(Node {
            gen,
            alive: true,
            kind: ViewKind::Root,
            key: 0,
            props: Props::Root,
            parent: NONE,
            first_child: NONE,
            next_sib: NONE,
            prev_sib: NONE,
            x: 0,
            y: 0,
            w: self.vp_w as i32,
            h: self.vp_h as i32,
            meas_w: self.vp_w,
            meas_h: self.vp_h,
            layout_dirty: true,
            paint_dirty: true,
            ox: 0,
            oy: 0,
        });
        self.root_slot = idx as u32;
        pack_id(idx, gen)
    }

    pub fn add(&mut self, parent: NodeId, spec: desc::ViewSpec) -> Result<NodeId, UiError> {
        let pidx = self.slot_of(parent)?;
        match self.nodes[pidx].as_ref().map(|n| n.kind) {
            Some(ViewKind::Root) | Some(ViewKind::Col) | Some(ViewKind::Row) | Some(ViewKind::Pad) => {}
            _ => return Err(UiError::BadParent), // 容器位传叶子
        }
        if spec.kind == ViewKind::Root {
            return Err(UiError::BadParent); // Root 不能为子
        }
        let Some(idx) = self.alloc_slot() else { return Err(UiError::NodeFull) };
        let gen = self.gens[idx];
        self.nodes[idx] = Some(Node {
            gen,
            alive: true,
            kind: spec.kind,
            key: spec.key,
            props: spec.props,
            parent: pidx as u32,
            first_child: NONE,
            next_sib: NONE,
            prev_sib: NONE,
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
        self.link_child(pidx, idx as u32);
        // 容器重排 + 既有兄弟位移面 damage（超集覆盖——正确性优先）。
        let pr = self.abs_rect_of(pidx);
        self.damage_rect(pr);
        self.mark_ancestors_dirty(idx as u32);
        Ok(pack_id(idx, gen))
    }

    pub fn remove(&mut self, node: NodeId) -> Result<(), UiError> {
        // 连子树；先收旧绝对界进 damage 再整族释放（§3.1）。
        let idx = self.slot_of(node)? as usize;
        let mut list = [NONE; N];
        list[0] = idx as u32;
        let mut cnt = 1usize;
        let fc = self.nodes[idx].as_ref().map(|n| n.first_child).unwrap_or(NONE);
        self.collect_sibs(fc, &mut list, &mut cnt);
        let parent = self.nodes[idx].as_ref().map(|n| n.parent).unwrap_or(NONE);
        let mut i = 0usize;
        while i < cnt {
            let s = list[i] as usize;
            let r = self.abs_rect_of(s);
            self.damage_rect(r);
            i += 1;
        }
        if parent != NONE {
            let pr = self.abs_rect_of(parent as usize);
            self.damage_rect(pr);
        }
        self.unlink(idx);
        if idx as u32 == self.root_slot {
            self.root_slot = NONE;
        }
        // 依赖边随子树解绑——**先于 free_slot**（gen 推进前以当前
        // NodeId 精确解绑，表侧 unbind_node 全清该节点边；挪到 free
        // 之后则须靠悬空 id 扫描兜——两形等价但先验解绑无扫描面）。
        let mut i = 0usize;
        while i < cnt {
            if let Some(n) = self.nodes[list[i] as usize].as_ref() {
                self.deps.unbind_node(pack_id(list[i] as usize, n.gen));
            }
            i += 1;
        }
        let mut i = 0usize;
        while i < cnt {
            self.free_slot(list[i] as usize);
            i += 1;
        }
        // 随行动画清理（悬空 id 判据 = slot_of 失败——gen 已推进）。
        let mut i = 0usize;
        while i < ANIM_MAX {
            if let Some(a) = self.anims[i] {
                if self.slot_of(a.node).is_err() {
                    self.anims[i] = None;
                }
            }
            i += 1;
        }
        if parent != NONE {
            self.mark_ancestors_dirty(parent);
        }
        Ok(())
    }

    pub fn set_props(&mut self, node: NodeId, props: desc::Props) -> Result<(), UiError> {
        // 几何变→重排脏+damage 新旧界；纯色变→paint 脏（§6 注记）。
        let idx = self.slot_of(node)? as usize;
        // kind 失配拒面（set_text 同型）：冻结 UiError 族无 kind 失配变
        // 体——NodeBad 兼任 loud；防「Rect 槽被塞 Text props」静默错配
        // （paint 发射面按 kind 匹配 props，错配 = 永不发射/读错载荷）。
        let kind_ok = matches!(
            self.nodes[idx]
                .as_ref()
                .map(|n| (&n.kind, &props)),
            Some((ViewKind::Root, Props::Root))
                | Some((ViewKind::Rect, Props::Rect(_)))
                | Some((ViewKind::Text, Props::Text(_)))
                | Some((ViewKind::Col, Props::Col(_)))
                | Some((ViewKind::Row, Props::Row(_)))
                | Some((ViewKind::Pad, Props::Pad(_)))
        );
        if !kind_ok {
            return Err(UiError::NodeBad);
        }
        let Some(old) = self.nodes[idx].as_ref().map(|n| n.props) else { return Err(UiError::NodeBad) };
        let geom_same = props_geom_equal(&old, &props);
        let old_r = self.abs_rect_of(idx);
        if let Some(n) = self.nodes[idx].as_mut() {
            n.props = props;
            n.paint_dirty = true;
        }
        if geom_same {
            self.damage_rect(old_r); // 纯着色：同界重绘
        } else {
            self.damage_rect(old_r); // 旧界
            let p = self.nodes[idx].as_ref().map(|n| n.parent).unwrap_or(NONE);
            if p != NONE {
                let pr = self.abs_rect_of(p as usize);
                self.damage_rect(pr); // 兄弟位移面（新界由 layout 收）
                self.mark_ancestors_dirty(p);
            }
            if let Some(n) = self.nodes[idx].as_mut() {
                n.layout_dirty = true;
            }
        }
        Ok(())
    }

    pub fn set_text(&mut self, node: NodeId, bytes: &[u8]) -> Result<(), UiError> {
        let idx = self.slot_of(node)? as usize;
        let buf = desc::TextBuf::new(bytes)?;
        // 冻结 UiError 族无 kind 失配变体——NodeBad 兼任 wrong-kind loud。
        let is_text = matches!(
            self.nodes[idx].as_ref().map(|n| (&n.kind, &n.props)),
            Some((ViewKind::Text, Props::Text(_)))
        );
        if !is_text {
            return Err(UiError::NodeBad);
        }
        let old_r = self.abs_rect_of(idx);
        self.damage_rect(old_r); // 旧界（内在尺寸变）
        let p = self.nodes[idx].as_ref().map(|n| n.parent).unwrap_or(NONE);
        if p != NONE {
            let pr = self.abs_rect_of(p as usize);
            self.damage_rect(pr);
            self.mark_ancestors_dirty(p);
        }
        if let Some(n) = self.nodes[idx].as_mut() {
            if let Props::Text(tp) = &mut n.props {
                tp.text = buf;
            }
            n.layout_dirty = true;
        }
        Ok(())
    }

    pub fn invalidate(&mut self, node: NodeId) -> Result<(), UiError> {
        // 视图级重绘（old+new 界进 damage——bounds 含 ox/oy，当前界即两者锚）。
        let idx = self.slot_of(node)? as usize;
        let r = self.abs_rect_of(idx);
        self.damage_rect(r);
        if let Some(n) = self.nodes[idx].as_mut() {
            n.paint_dirty = true;
        }
        Ok(())
    }

    pub fn bounds(&self, node: NodeId) -> Result<damage::RectI, UiError> {
        // 绝对界（含 ox/oy）。
        let idx = self.slot_of(node)? as usize;
        Ok(self.abs_rect_of(idx))
    }

    pub fn kind_of(&self, node: NodeId) -> Result<desc::ViewKind, UiError> {
        let idx = self.slot_of(node)? as usize;
        match self.nodes[idx].as_ref() {
            Some(n) => Ok(n.kind),
            None => Err(UiError::NodeBad),
        }
    }

    // —— 布局 / 脏区 ——

    pub fn layout(&mut self) -> layout::LayoutStat {
        // 脏子树重排；新界进 damage（freed = 当前空闲槽清单——removal 面
        // 的陈旧缓存跳过依据）。
        layout::relayout(
            &mut self.nodes,
            self.vp_w,
            self.vp_h,
            &mut self.damage,
            &self.free_stack[..self.free_count],
        )
    }

    pub fn damage(&self) -> &damage::DamageSet {
        &self.damage
    }

    pub fn drain_damage(&mut self) -> damage::DamageSet {
        core::mem::replace(&mut self.damage, damage::DamageSet::new())
    }

    // —— 渲染（DisplayList 'a 钉调用方 scratch，不钉引擎） ——

    pub fn paint<'a>(
        &self,
        scratch: &'a mut [GlyphPos],
        enc: &mut CommandEncoder<'a>,
    ) -> paint::PaintReport {
        if self.root_slot == NONE {
            return paint::PaintReport { cmds: 0, drawn_px: 0, culled: 0, overdraw_x256: 0 };
        }
        paint::paint(&self.nodes, self.root_slot, (self.vp_w, self.vp_h), &self.damage, scratch, enc)
    }

    // —— 输入 ——

    pub fn push_pointer(&mut self, ev: hit::PtrEvent) -> Result<(), UiError> {
        // 缓冲 ≤INPUT_BUF_MAX（满 → MsgFull loud——不自陷不静默丢）。
        if self.input_n >= INPUT_BUF_MAX {
            return Err(UiError::MsgFull);
        }
        self.input_buf[self.input_n] = Some(ev);
        self.input_n += 1;
        Ok(())
    }

    pub fn pump_input(&mut self, expected_present_ns: u64) -> desc::MsgBuf {
        // 帧头：referee + 重采样 + 消费（P7 输入同拍）。
        let mut out = desc::MsgBuf::new();
        let target = resample::target_from_expected(expected_present_ns);
        let n = self.input_n;
        let mut i = 0usize;
        while i < n {
            let Some(mut ev) = self.input_buf[i].take() else {
                i += 1;
                continue;
            };
            if ev.ts_ns > self.last_input_ts_ns {
                self.last_input_ts_ns = ev.ts_ns;
            }
            let mut did_resample = false;
            if ev.phase == PHASE_MOVE {
                // 活跃 MOVE 滑窗维护（非单调/跨度断裂由 resample 内部拒）。
                if self.hist_len >= INPUT_BUF_MAX {
                    self.move_hist.copy_within(1..INPUT_BUF_MAX, 0);
                    self.hist_len = INPUT_BUF_MAX - 1;
                }
                self.move_hist[self.hist_len] =
                    Sample { ts_ns: ev.ts_ns, x: ev.x as i64, y: ev.y as i64 };
                self.hist_len += 1;
                let s = {
                    let h: &[Sample] = &self.move_hist[..self.hist_len];
                    resample::resample_to_target(h, target)
                };
                if let Some(smp) = s {
                    // 目标推导 = expected − LATENCY（Android 同式，§3.9
                    // ——单源 resample::target_from_expected）；重采样坐
                    // 标替换入事件。
                    ev.x = clamp_u32(smp.x);
                    ev.y = clamp_u32(smp.y);
                    did_resample = true;
                }
            } else {
                self.hist_len = 0; // 流断裂（DOWN/UP/CANCEL）——窗清零
            }
            if self.root_slot != NONE {
                let n_before = out.len();
                hit::dispatch(&mut self.gesture, &self.nodes, self.root_slot, ev, &mut out);
                // `resampled` 位标注（§3.9 引擎侧义务——PE_FL_RESAMPLED
                // 的引擎内对应面）：重采样路径产物 DragMove 置 true。本
                // 事件恰产一条 Msg（MOVE 至多一条）且满帽拒收（lost_msgs）
                // 时 n 无增——漏标无害：Msg 未产出。
                if did_resample && out.len() == n_before + 1 {
                    if let Some(Msg::DragMove { resampled, .. }) = out.last_mut() {
                        *resampled = true;
                    }
                }
            }
            i += 1;
        }
        self.input_n = 0;
        out
    }

    pub fn node_at(&self, x: u32, y: u32) -> Option<NodeId> {
        // topmost 优先——单源委托 hit::node_at（z 序逆走 = 兄弟链尾侧先
        // 探；公共查询与 pump 消费两入口单一结论；环/死槽防御同源）。
        hit::node_at(&self.nodes, self.root_slot, hit::u32_to_i32_sat(x), hit::u32_to_i32_sat(y))
    }

    // —— 动画 ——

    pub fn anim_add(&mut self, a: anim::Anim) -> Result<(), UiError> {
        let mut i = 0usize;
        while i < ANIM_MAX {
            if self.anims[i].is_none() {
                self.anims[i] = Some(a);
                return Ok(());
            }
            i += 1;
        }
        Err(UiError::AnimFull)
    }

    pub fn anim_clear(&mut self) {
        let mut i = 0usize;
        while i < ANIM_MAX {
            self.anims[i] = None;
            i += 1;
        }
    }

    pub fn anim_active(&self) -> bool {
        self.anims.iter().any(|a| a.is_some())
    }

    // —— 帧循环 ——

    pub fn set_vsync(&mut self, period_ns: u64, phase_ns: u64) {
        // 周期 = 协商倒数；phase = UI/合成错相注入位（对齐条 12）。
        self.vsync_period_ns = period_ns;
        self.vsync_phase_ns = phase_ns;
    }

    pub fn vsync(&self) -> (u64, u64) {
        (self.vsync_period_ns, self.vsync_phase_ns)
    }

    fn rate_hz(&self) -> u8 {
        if self.vsync_period_ns == 0 {
            return 0;
        }
        let hz = 1_000_000_000u64 / self.vsync_period_ns;
        if hz > 255 {
            255
        } else {
            hz as u8
        }
    }

    pub fn render_frame<'a>(
        &mut self,
        now_ns: u64,
        scratch: &'a mut [GlyphPos],
        enc: &mut CommandEncoder<'a>,
    ) -> frame::FrameOutcome {
        // doFrame 序列（§3.8）：anim tick → layout → paint → 遥测 → 相位推进。
        let anim_on = anim::tick(&mut self.anims, &mut self.nodes, &mut self.damage, now_ns);
        let _stat = layout::relayout(
            &mut self.nodes,
            self.vp_w,
            self.vp_h,
            &mut self.damage,
            &self.free_stack[..self.free_count],
        );
        let drew = !self.damage.is_empty(); // 无 damage 不提交（对齐条 6）
        let report = if drew && self.root_slot != NONE {
            paint::paint(
                &self.nodes,
                self.root_slot,
                (self.vp_w, self.vp_h),
                &self.damage,
                scratch,
                enc,
            )
        } else {
            paint::PaintReport { cmds: 0, drawn_px: 0, culled: 0, overdraw_x256: 0 }
        };
        let dmg = core::mem::replace(&mut self.damage, damage::DamageSet::new());
        // expected 轨单源 frame.rs（拍沿含沿不欠拍；phase mod 归一容忍
        // 敌意 phase ≥ period）。
        let expected = frame::next_vsync_tick(now_ns, self.vsync_period_ns, self.vsync_phase_ns);
        // 起振延迟遥测（增件②——Weston「extra vblank」可观测）：
        // 首帧晚过 wake 锚 tick 即迟到。
        if let Some(anchor) = self.spinup_anchor.take() {
            if now_ns > anchor {
                self.telem.bump_spinup_late();
            }
        }
        let rec = telemetry::FrameRec {
            seq: self.seq,
            input_ts_ns: self.last_input_ts_ns,
            frame_begin_ns: now_ns,
            frame_end_ns: now_ns, // 引擎无钟——单时刻帧；真耗时 embedder 双打点（GUI-9）
            expected_ns: expected,
            actual_ns: 0, // actual 轨 note_presented 回填
            presented: false,
            discarded: false,
            damage_full: dmg.full,
            damage_rects: dmg.count,
            overdraw_x256: report.overdraw_x256,
            jank: false,
        };
        self.telem.record(rec);
        self.last_frame_seq = self.seq;
        self.last_expected_ns = expected;
        self.seq = self.seq.wrapping_add(1);
        if drew || anim_on || self.input_n > 0 {
            self.last_work_ns = now_ns;
        }
        let idle_ms = if now_ns >= self.last_work_ns {
            (now_ns - self.last_work_ns) / 1_000_000
        } else {
            0
        };
        let sig = frame::FrameSignals {
            damage: drew,
            anim: anim_on,
            input: self.input_n > 0,
            present_pending: self.inflight_n > 0, // 在途帧回调未清 → SETTLE
        };
        self.phase = frame::next_phase(self.phase, sig, idle_ms);
        self.last_now_ns = now_ns;
        if !drew {
            // 跳过帧（damage 空）不提交（对齐条 6）——rate_hint 诚实填
            // 0：无提交的帧无帧率话语权（frame.rs 字段注/对齐条 12）。
            return frame::FrameOutcome::skipped(expected);
        }
        frame::FrameOutcome { drew, damage: dmg, expected_present_ns: expected, rate_hint_hz: self.rate_hz() }
    }

    pub fn frame_phase(&self) -> frame::FramePhase {
        self.phase
    }

    pub fn next_wakeup(&self) -> Option<u64> {
        // None = IDLE（撤软 vsync 定时器、睡事件——R3 对策）。单源委托
        // frame::next_wakeup：Run = min(vsync 拍点, 动画期限, 强降闹钟)、
        // Settle = 强降闹钟（在途回调迟到的兜底醒——§3.8）。
        let anim_deadline = self
            .anims
            .iter()
            .flatten()
            .map(|a| a.start_ns.saturating_add(a.dur_ns))
            .min();
        frame::next_wakeup(
            self.phase,
            self.last_now_ns,
            self.last_work_ns,
            self.vsync_period_ns,
            self.vsync_phase_ns,
            anim_deadline,
        )
    }

    pub fn wake(&mut self, now_ns: u64) {
        // IDLE→Run 立即渲不欠拍 + 起振遥测。
        if self.phase == frame::FramePhase::Idle {
            self.telem.bump_idle_exits();
        }
        self.telem.bump_wakes();
        self.phase = frame::FramePhase::Run;
        self.last_work_ns = now_ns;
        self.last_now_ns = now_ns;
        // 下一帧必有产出（不欠拍）——**条件化 FULL**：已有脏区时保留既有
        // 矩形集（增量 wake 不把局部脏放大成整屏；空集才置 FULL——修
        // 复轮 finding 9：无条件 FULL 会吞掉先前 invalidate 的精确脏区）。
        if self.damage.is_empty() {
            self.damage.add_full();
        }
        self.spinup_anchor =
            Some(frame::next_vsync_tick(now_ns, self.vsync_period_ns, self.vsync_phase_ns));
    }

    pub fn note_submitted(&mut self) {
        // credits 簿记单源 frame.rs（≤CREDITS_MAX——不轰炸，对齐条 8）。
        // 门返回值丢弃：note_submitted 是「已提交」事后记账（提交与否的
        // 门由 embedder 提交前自查——backpressure 是义务不是建议）；帽
        // 满时 outstanding 不再增，漂移由 note_presented 应答重同步收口
        // （显示服务报的剩余信用是真值源）。
        let _ = self.credits.submit();
        if self.inflight_n < self.inflight.len() {
            self.inflight[self.inflight_n] = (self.last_frame_seq, self.last_expected_ns);
            self.inflight_n += 1;
        }
    }

    pub fn note_presented(&mut self, seq: u32, presented_ns: u64, status: u8, credits_remaining: u8) {
        // actual 轨 + SETTLE→IDLE 确认 + jank 判定（超半周期 deadline_miss）。
        let mut found = None;
        let mut i = 0usize;
        while i < self.inflight_n {
            if self.inflight[i].0 == seq {
                found = Some(i);
                break;
            }
            i += 1;
        }
        if let Some(i) = found {
            let expected = self.inflight[i].1;
            let deadline = expected.checked_add(self.vsync_period_ns / 2);
            let jank = match deadline {
                Some(d) => presented_ns > d,
                None => false,
            };
            self.telem.fill_presented(
                seq,
                presented_ns,
                status == FRAME_PRESENTED,
                status == FRAME_DISCARDED,
                jank,
            );
            self.inflight_n -= 1;
            self.inflight[i] = self.inflight[self.inflight_n]; // swap-remove（序无义）
        }
        // 背压重同步：显示服务报的**剩余信用**是真值源（outstanding =
        // MAX − remaining；未知应答不猜测——对齐条 8）。
        self.credits.on_presented(credits_remaining);
    }

    pub fn vote_hint(&self) -> frame::VoteBallot {
        // 票面单源 frame::ballot_for（场景定值表 §3.8；仲裁在显示服务
        // ——对齐条 12，产票不仲裁）。三信号：动画表活跃 / 手势拖拽进
        // 行中（interacting 第三信号——非拖按住不算）/ 帧循环 Idle。
        frame::ballot_for(
            self.anim_active(),
            self.gesture.any_dragging(),
            self.phase == frame::FramePhase::Idle,
        )
    }

    // —— AST 双入口（入口②） ——

    pub fn encode_desc(&self, out: &mut [u8]) -> Result<usize, desc::AstError> {
        // 无根树 → Ok(0)（空流；apply 空流 Truncated 拒——空树不可重建根）。
        desc::encode_tree(&self.nodes, self.root_slot, out)
    }

    pub fn apply_desc(&mut self, src: &[u8]) -> Result<(), desc::AstError> {
        // 整树重建 + FULL damage；拒面零半执行（失败重建空场）。
        self.reset_arena();
        match desc::apply_tree(src, &mut self.nodes, &mut self.gens, &mut self.root_slot) {
            Ok(()) => {
                self.damage.add_full();
                Ok(())
            }
            Err(e) => {
                self.reset_arena();
                Err(e)
            }
        }
    }

    // —— 依赖骨架（v1 记录不消费——失效仍视图级 Invalidate，裁决不跳步） ——

    pub fn dep_bind(&mut self, node: NodeId, dep: dep::DepId) -> Result<(), UiError> {
        // 单源委托 dep::DepTable：同边重复 bind 幂等 Ok（不重复占槽）、
        // 满帽 DepFull loud（§3.10）；liveness 校验在本入口。
        self.slot_of(node)?; // 悬空 id loud
        self.deps.bind(node, dep)
    }

    pub fn dep_dependents(&self, dep: dep::DepId, out: &mut [NodeId]) -> usize {
        // 表侧查询 + liveness 过滤；返回**存活依赖总数**（可 > out.len()
        // ——调用方以返回值判截断，零分配哨兵面不静默缩水）。
        let mut tmp = [NodeId(0); DEP_EDGES_MAX];
        let total = self.deps.dependents(dep, &mut tmp);
        let mut n = 0usize;
        for k in 0..total {
            let nid = tmp[k];
            if self.slot_of(nid).is_ok() {
                if n < out.len() {
                    out[n] = nid;
                }
                n += 1;
            }
        }
        n
    }

    pub fn telemetry(&self) -> &telemetry::Telemetry {
        &self.telem
    }
}
