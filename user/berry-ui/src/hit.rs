//! berry-ui 命中检测 + 手势 referee（GUI-3——`docs/GUI-3实施设计草案.md`
//! §3.6；ADR #52 Msg-only：引擎产 `MsgBuf`、app 的 update 消费，引擎内
//! 零闭包零回调副作用）。
//!
//! - [`node_at`]：topmost 优先（z 序逆走 = 兄弟链尾侧先探——画家算法
//!   后画者在顶），命中含 paint 偏移 ox/oy（Transform 动画后命中随视觉
//!   走，§3.7 分型的消费面）；v1 无 clip 视图（挂账——子命中不受父界
//!   裁剪，保守正确）。
//! - 手势 referee（每 pointer_id 独立状态 {down_node, down_xy,
//!   dragging}）：DOWN→hit→`Press`；MOVE 距按下点 > [`crate::DRAG_SLOP_PX`]
//!   → `DragStart` 此后 `DragMove`；UP→在拖 `DragEnd` / 不在
//!   `Release`+`Click`；CANCEL 终止手势**不补 UP**（对齐条 14）——拖中
//!   `DragEnd`+`Cancel` 双发、非拖仅 `Cancel`。
//! - GUI-6 D15 referee v2：每指针位 8 样本速度环（96ms 窗首末差分 →
//!   DragEnd.vx/vy，px/s 钳 ±32768）+ 锁轴（slop **保持欧氏判定**零漂；
//!   触发时位移主轴 `|dx|>=|dy|`→X（tie 取 X），DragMove 只透传锁定轴
//!   分量——跨轴分量冻结在 DragStart 触发样本值）。
//! - 多指：pointer_id = 身份键（device_id v1 恒 0 勿读——对齐条 14）；
//!   定容 [`PTR_MAX`] 槽满计 [`Gesture::lost_events`] loud 不静默。
//! - 坐标三套（§0 铁律）：wire u32（PtrEvent/Msg 载荷）↔ 内部 i32
//!   （slop 距离 i64 平方比较防溢出）——换算一律显式饱和，无裸 `as`。
//! - 零 panic / 零分配：树结构异常（兄弟链/父链环、死槽）→ 放弃命中
//!   返 None，绝不挂死绝不 unwrap。

use crate::desc::{Axis, Msg, MsgBuf, NodeId};
use crate::Node;

/// 槽索引空哨兵（§6 Node 注：NONE = u32::MAX——lib.rs 同值，本模块自带
/// 防跨模块未定名依赖）。
const NONE: u32 = u32::MAX;

/// 并发指针槽容量（v1 定值 4——五指以上属 v2 面；满帽计
/// [`Gesture::lost_events`]，不自陷不静默）。
pub(crate) const PTR_MAX: usize = 4;

/// 速度环样本容量（D15：每指针位 8 样本——§9 栈/尺寸账 4×8×16B）。
pub(crate) const VEL_RING_MAX: usize = 8;

/// 速度环差分窗（D15）：自最新样本回看 96ms——窗外老样本不参与差分
/// （Android VelocityTracker HORIZON 同式）。
const VEL_WINDOW_NS: u64 = 96_000_000;

/// DragEnd 速度钳（D15）：px/s，i32 域 ±32768（wire 面防溢出的立法值）。
const VEL_CLAMP: i32 = 32_768;

/// 速度环样本（原始指针位——resample 替换**前**的 (ts,x,y)；16B/条）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VelSample {
    ts_ns: u64,
    x: i32,
    y: i32,
}

const VEL_ZERO: VelSample = VelSample { ts_ns: 0, x: 0, y: 0 };

/// 单指针手势状态（referee 的 per-pointer 私有形态）。
#[derive(Debug, Clone, Copy)]
struct PtrGesture {
    pointer_id: u16,
    active: bool,
    /// DOWN 时刻命中槽（NONE = 按在空白——后续 Click/Release/Drag 锚
    /// 定此槽不重命中：手势语义 = 按下点归属，非抬起点）。
    down_node: u32,
    down_x: i32,
    down_y: i32,
    dragging: bool,
    /// 锁轴（D15）：欧氏 slop 触发时裁定，DragStart 携出；默认 X 无义
    /// （未拖前不读）。
    axis: Axis,
    /// 跨轴冻结坐标（D15）：DragStart 触发样本的 wire 坐标——DragMove
    /// 只透传锁定轴分量，跨轴分量恒回放此值（防对角抖动）。
    lock_x: u32,
    lock_y: u32,
    /// 速度环（D15）：8 样本时序环（`ring[..ring_n]` 按时间序）——
    /// 入环点 = resample 替换前原始样本；DOWN/UP/CANCEL 清环。
    ring: [VelSample; VEL_RING_MAX],
    ring_n: usize,
}

const INACTIVE: PtrGesture = PtrGesture {
    pointer_id: 0,
    active: false,
    down_node: NONE,
    down_x: 0,
    down_y: 0,
    dragging: false,
    axis: Axis::X,
    lock_x: 0,
    lock_y: 0,
    ring: [VEL_ZERO; VEL_RING_MAX],
    ring_n: 0,
};

/// pressure 0x8000 哨兵的折叠值：500 毫单位 = W3C 0.5（对齐条 13——
/// 引擎侧义务，wire 侧哨兵语义不外溢到 Msg 消费面）。
const PRESSURE_DEFAULT_MI: u16 = 500;

/// wire → 引擎面指针事件（§6 冻结字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtrEvent {
    pub phase: u8,
    pub pointer_id: u16,
    pub x: u32,
    pub y: u32,
    pub ts_ns: u64,
    pub buttons: u8,
    pub pressure_mi: u16,
}

impl PtrEvent {
    /// wire → 引擎面：pressure 0x8000 哨兵折 500（W3C 0.5——引擎侧
    /// 义务，对齐条 13）；buttons 位域原样透传（BUTTON_L/R/M 见
    /// berry-guiproto）；phase 值域校验在 guiproto 侧，引擎信任入参。
    pub fn from_wire(
        phase: u8,
        pointer_id: u16,
        x: u32,
        y: u32,
        ts_ns: u64,
        buttons: u8,
        pressure: u16,
    ) -> Self {
        let pressure_mi = if pressure == berry_guiproto::PRESSURE_UNAVAILABLE {
            PRESSURE_DEFAULT_MI
        } else {
            pressure
        };
        Self { phase, pointer_id, x, y, ts_ns, buttons, pressure_mi }
    }
}

/// 手势 referee 状态机（§6 冻结为「per-pointer 状态」不透明体）。
pub(crate) struct Gesture {
    slots: [PtrGesture; PTR_MAX],
    /// `MsgBuf` 满（UiError::MsgFull）被丢的 Msg 计数——「定容 8 loud
    /// 满」的可观测面（§3.6；dispatch 返回 ()，拒面落计数不落 panic）。
    /// 诊断计数面：写不读（宿主/测试读取挂集成段）。
    #[allow(dead_code)]
    pub(crate) lost_msgs: u32,
    /// PTR_MAX 指针槽满时被丢的事件计数（防御面——v1 单指/少指常态恒
    /// 0）。诊断计数面：写不读（同上）。
    #[allow(dead_code)]
    pub(crate) lost_events: u32,
}

impl Gesture {
    pub(crate) const fn new() -> Self {
        Self { slots: [INACTIVE; PTR_MAX], lost_msgs: 0, lost_events: 0 }
    }

    fn find(&self, pid: u16) -> Option<usize> {
        self.slots.iter().position(|s| s.active && s.pointer_id == pid)
    }

    /// 任一活跃指针处于拖拽态（vote_hint 的 interacting 第三信号——
    /// §3.8 票面 INTERACTIVE 档（GUI-8 §4 段①折算：拖拽相位 →
    /// INTERACTIVE，旧 SCENE_DRAG 档退役）的判定源；非拖按住不算
    /// 交互进行中）。
    pub(crate) fn any_dragging(&self) -> bool {
        self.slots.iter().any(|s| s.active && s.dragging)
    }

    /// DOWN 占槽：同 pointer_id 重复 DOWN（无 UP 的防御面）复用槽重开
    /// 手势；无空槽 → None（调用方计 lost_events）。
    fn acquire(&mut self, pid: u16) -> Option<usize> {
        if let Some(i) = self.find(pid) {
            return Some(i);
        }
        let i = self.slots.iter().position(|s| !s.active)?;
        self.slots[i] = PtrGesture {
            pointer_id: pid,
            active: true,
            ..INACTIVE
        };
        Some(i)
    }

    /// 速度环入环（D15）：pump_input 在 **resample 替换前**调用（§7.1
    /// 入环点——resampled 样本不入环防速度低估）；无活跃槽（无 DOWN
    /// 前置的 MOVE）no-op 与 dispatch 忽略语义对齐。满环丢最老样本
    /// （时序窗语义：差分只看首末，中段丢失不偏置）。
    pub(crate) fn note_sample(&mut self, pid: u16, ts_ns: u64, x: i32, y: i32) {
        let Some(i) = self.find(pid) else { return };
        let g = &mut self.slots[i];
        if g.ring_n >= VEL_RING_MAX {
            g.ring.copy_within(1..VEL_RING_MAX, 0);
            g.ring_n = VEL_RING_MAX - 1;
        }
        g.ring[g.ring_n] = VelSample { ts_ns, x, y };
        g.ring_n += 1;
    }
}

/// 收尾速度（D15）：环内 96ms 窗首末差分——自最新样本回看收窗，窗内
/// 最老样本为首；样本 <2 / 无严格更老的窗内样本 / Δt==0（同拍双样本）
/// → (0, 0)。i128 中间量（dx×1e9 可达 2^63——i64 必溢），末钳 ±32768。
fn velocity_at_end(g: &PtrGesture) -> (i32, i32) {
    let n = g.ring_n;
    if n < 2 {
        return (0, 0);
    }
    let newest = g.ring[n - 1];
    // 环时序回扫：首个落窗外（或时序异常非更老）样本即停——更老样本
    // 必也窗外（环按入环时序排列）。
    let mut first = n - 1;
    let mut i = n - 1;
    while i > 0 {
        i -= 1;
        match newest.ts_ns.checked_sub(g.ring[i].ts_ns) {
            Some(d) if d <= VEL_WINDOW_NS => first = i,
            _ => break,
        }
    }
    if first == n - 1 {
        return (0, 0); // 无严格更老的窗内样本（有效样本 <2）
    }
    let f = g.ring[first];
    let dt = newest.ts_ns - f.ts_ns; // > 0 已证（checked_sub 收窗内严格更老）
    if dt == 0 {
        return (0, 0);
    }
    let vx = (newest.x as i64 - f.x as i64) as i128 * 1_000_000_000 / (dt as i128);
    let vy = (newest.y as i64 - f.y as i64) as i128 * 1_000_000_000 / (dt as i128);
    (clamp_vel(vx), clamp_vel(vy))
}

/// px/s → i32 钳 ±32768（§0 显式 clamp 后 cast 纪律）。
fn clamp_vel(v: i128) -> i32 {
    v.clamp(-(VEL_CLAMP as i128), VEL_CLAMP as i128) as i32
}

/// wire u32 → 内部 i32 饱和换算（§0 坐标三套铁律：显式换算不隐式收缩）。
pub(crate) fn u32_to_i32_sat(v: u32) -> i32 {
    if v > i32::MAX as u32 {
        i32::MAX
    } else {
        v as i32
    }
}

/// 树命中检测（topmost 优先）：`root` 为根槽（NONE/死槽 → None）。
/// pub(crate) 供 lib.rs `Ui::node_at` 委托与 dispatch 共用单源。
pub(crate) fn node_at(nodes: &[Option<Node>], root: u32, x: i32, y: i32) -> Option<NodeId> {
    if root == NONE {
        return None;
    }
    match nodes.get(root as usize) {
        Some(Some(n)) if n.alive => {}
        _ => return None,
    }
    hit_walk(nodes, root, 0, 0, x as i64, y as i64, 0)
}

/// 自 `slot` 子树的深度优先命中（兄弟逆序 = 后画者优先 = topmost）。
/// `ex/ey` = 祖先链累计原点（含各节点 x/y 与 ox/oy paint 偏移）；
/// i64 累计防 i32 祖先链加法溢出。环/死槽防御：步数以 nodes.len() 为
/// 帽，异常树放弃命中返 None（零 panic）。
fn hit_walk(
    nodes: &[Option<Node>],
    slot: u32,
    ex: i64,
    ey: i64,
    x: i64,
    y: i64,
    depth: u32,
) -> Option<NodeId> {
    // 树深不可能超过节点数——超帽 = 父/子链环，防御放弃
    if depth as usize > nodes.len() {
        return None;
    }
    let n = match nodes.get(slot as usize) {
        Some(Some(n)) if n.alive => n,
        _ => return None,
    };
    // 本节点绝对界（含 paint 偏移——Transform 动画后命中随视觉走）
    let bx = ex + n.x as i64 + n.ox as i64;
    let by = ey + n.y as i64 + n.oy as i64;
    // 负 w/h 视为零尺寸（RectI::new 同口径——gfx 坑 5 收口在 damage 侧，
    // 命中侧同语义不另造规则）
    let inside =
        x >= bx && y >= by && x < bx + (n.w as i64).max(0) && y < by + (n.h as i64).max(0);

    // 子优先（v1 无 clip：子命中不受本节点界裁剪）；兄弟链尾 = 顶面。
    // 先沿 next_sib 走到链尾，再沿 prev_sib 回走（无堆栈的逆序遍历）。
    let mut tail = n.first_child;
    let mut guard = 0usize;
    while tail != NONE {
        guard += 1;
        if guard > nodes.len() {
            return None; // next_sib 环防御
        }
        match nodes.get(tail as usize) {
            Some(Some(c)) if c.alive => {
                if c.next_sib == NONE {
                    break;
                }
                tail = c.next_sib;
            }
            _ => return None, // 链上死槽/越界 = 树结构异常，防御放弃
        }
    }
    let mut sib = tail;
    let mut back = 0usize;
    while sib != NONE {
        back += 1;
        if back > nodes.len() {
            return None; // prev_sib 环防御
        }
        if let Some(id) = hit_walk(nodes, sib, bx, by, x, y, depth + 1) {
            return Some(id);
        }
        match nodes.get(sib as usize) {
            Some(Some(c)) if c.alive => sib = c.prev_sib,
            _ => return None,
        }
    }

    // Root = viewport 持有者**不参与命中**（背景按压 = 无命中——与
    // lib.rs `Ui::node_at` 同语义：公共查询与 pump 消费两入口单一结论）
    if inside && n.kind != crate::desc::ViewKind::Root {
        // gen<<16 | idx 打包（§3.1——代数防 use-after-free 误用）
        return Some(NodeId(((n.gen as u32) << 16) | slot));
    }
    None
}

/// Msg 入 buf 的 loud 面：满（UiError::MsgFull）计数不丢诊断（§3.6
/// 「定容 8 loud 满」——dispatch 返回 ()，拒面落计数）。
fn push_msg(gesture: &mut Gesture, out: &mut MsgBuf, m: Msg) {
    if out.push(m).is_err() {
        gesture.lost_msgs += 1;
    }
}

/// 手势 referee 主入口（§6 冻结签名）：单事件进 → Msg 出。
/// phase 未知值（含 hover 5–7 留位）静默忽略——v1 无 hover 面。
pub(crate) fn dispatch(gesture: &mut Gesture, nodes: &[Option<Node>], root: u32, ev: PtrEvent, out: &mut MsgBuf) {
    // wire u32 → 内部 i32（slop 几何用）；Msg 载荷仍携 wire u32 原值
    let x = u32_to_i32_sat(ev.x);
    let y = u32_to_i32_sat(ev.y);

    match ev.phase {
        berry_guiproto::PHASE_DOWN => {
            let hit = node_at(nodes, root, x, y);
            let i = match gesture.acquire(ev.pointer_id) {
                Some(i) => i,
                None => {
                    gesture.lost_events += 1;
                    return; // 指针槽满：整事件拒收（无状态可关联后续）
                }
            };
            let g = &mut gesture.slots[i];
            g.down_x = x;
            g.down_y = y;
            g.dragging = false;
            g.ring_n = 0; // D15：DOWN 清环（跨手势样本不串速度）
            match hit {
                Some(id) => {
                    g.down_node = id.0;
                    push_msg(gesture, out, Msg::Press { node: id, x: ev.x, y: ev.y });
                }
                None => g.down_node = NONE, // 按在空白：跟踪继续但无 Press
            }
        }
        berry_guiproto::PHASE_MOVE => {
            let i = match gesture.find(ev.pointer_id) {
                Some(i) => i,
                None => return, // 无 DOWN 前置的 MOVE（hover 类）——忽略
            };
            let g = &mut gesture.slots[i];
            if !g.active || g.down_node == NONE {
                return; // 空白按压的 MOVE：无手势可推进
            }
            let node = NodeId(g.down_node);
            if g.dragging {
                // D15 锁轴：只透传锁定轴分量——跨轴分量回放 DragStart
                // 触发样本冻结值（防对角抖动）。resampled 标记是
                // pump_input 重采样层的注记义务（§3.9），本层事件无合成
                // 位——恒 false。
                let (mx, my) = if g.axis == Axis::X {
                    (ev.x, g.lock_y)
                } else {
                    (g.lock_x, ev.y)
                };
                push_msg(gesture, out, Msg::DragMove { node, x: mx, y: my, resampled: false });
            } else {
                // slop 判定：按下点欧氏距离平方 > 8²（i64 平方比较无
                // 开方无溢出；严格 > ——恰在 slop 圈上不算拖）。D15：判定
                // **保持欧氏**零漂（input 既有钉板不动）；触发时锁轴 =
                // 位移主轴（|dx|>=|dy| → X，tie 取 X——对角对称例无 Y 偏置）。
                let dx = x as i64 - g.down_x as i64;
                let dy = y as i64 - g.down_y as i64;
                let slop = crate::DRAG_SLOP_PX as i64;
                if dx * dx + dy * dy > slop * slop {
                    g.dragging = true;
                    g.axis = if dx.abs() >= dy.abs() { Axis::X } else { Axis::Y };
                    g.lock_x = ev.x; // 跨轴冻结 = 触发样本坐标
                    g.lock_y = ev.y;
                    let (ax, mx, my) = (g.axis, ev.x, ev.y);
                    push_msg(gesture, out, Msg::DragStart { node, axis: ax, x: mx, y: my });
                }
            }
        }
        berry_guiproto::PHASE_UP => {
            let (node, dragging, vx, vy) = match gesture.find(ev.pointer_id) {
                Some(i) if gesture.slots[i].active => {
                    let g = &mut gesture.slots[i];
                    // D15：收尾速度先于清环计算（UP 不入环——清环语义）
                    let v = if g.dragging { velocity_at_end(g) } else { (0, 0) };
                    let r = (g.down_node, g.dragging, v.0, v.1);
                    *g = INACTIVE; // 手势终止，槽释放（环随清）
                    r
                }
                _ => return, // 无 DOWN 前置的 UP——忽略
            };
            if node == NONE {
                return; // 空白按压收尾：无 Msg
            }
            let node = NodeId(node);
            if dragging {
                push_msg(gesture, out, Msg::DragEnd { node, vx, vy });
            } else {
                push_msg(gesture, out, Msg::Release { node, x: ev.x, y: ev.y });
                push_msg(gesture, out, Msg::Click { node });
            }
        }
        berry_guiproto::PHASE_CANCEL => {
            // CANCEL 终止手势不补 UP（对齐条 14）：拖中 DragEnd（收尾，
            // 速度同式取环——指面消失的收尾速度仍为最后窗差分真值）
            // + Cancel（终止通知）双发；非拖仅 Cancel
            let (node, dragging, vx, vy) = match gesture.find(ev.pointer_id) {
                Some(i) if gesture.slots[i].active => {
                    let g = &mut gesture.slots[i];
                    let v = if g.dragging { velocity_at_end(g) } else { (0, 0) };
                    let r = (g.down_node, g.dragging, v.0, v.1);
                    *g = INACTIVE;
                    r
                }
                _ => return,
            };
            if node == NONE {
                return;
            }
            let node = NodeId(node);
            if dragging {
                push_msg(gesture, out, Msg::DragEnd { node, vx, vy });
            }
            push_msg(gesture, out, Msg::Cancel { node });
        }
        _ => {} // hover 5–7 留位（guiproto 值域）——v1 无 hover 面，忽略
    }
}
