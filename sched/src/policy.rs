//! AI 策略注入——**三槽 RCU 式原子换表基础设施**（3-4c 留桩的机制面升级；
//! `AI调度设计.md` §2/§9、`路线图.md` Phase 3 出口增项）。
//!
//! 形态演进：策略表不再持 `&'static [Entry]` 热路径线性扫，而是**快照化**——
//! [`bind`] 把 `&[Entry]` 语义映射进 [`Snap`]（哈希桶 + 定长槽），经 [`publish`]
//! 原子换入三槽轮转区；决策路径（[`apply`]/[`weight_of`]）经 [`ACTIVE`]
//! Acquire 载入取**一份稳定快照**指针，O(1) 开放寻址查表（≤[`PROBE_MAX`] 步
//! 上界）。这是 v3 `policy_bind`（0x83）换表协议的机制底座**提前落地**——
//! 「事务化原子换表快照」半件已在本模块，共享环 + 键域扩展半件随 v3 协议落。
//!
//! 空态等价性不变（§2「AI 不在线时表为空/默认，等价于纯启发式」）：未绑
//! （[`ACTIVE`] = [`SENTINEL_EMPTY`] 哨兵 → [`EMPTY_SNAP`），或已绑空表
//! （`count == 0`）→ [`apply`] 两次分支即返、**零写入**，与「接口缺席」逐位
//! 等价（ktest `POLICY ok` 断言，非口号）。
//!
//! 动作空间四类封顶（§2 战略裁决）：①放置 hint；②频率/热限 hint（**走 PM
//! 服务**、不经调度路径——不在表里）；③包络内权重微调；④EPP 二值。**没有
//! 第五类**（尤其没有「直接改 RT 优先级/创建/杀任务」）。
//!
//! 并发纪律（单核，同 `sched.rs` 计数器）：[`apply`]/[`weight_of`] 只读——
//! IRQ 屏蔽的决策路径调用，每次调用取一次快照指针、窗内不换（RCU 读临界区
//! 形态）；写点 [`bind`]/[`unbind`] 仅在 SVC/IRQ 屏蔽的 boot/ktest 期（v3
//! 由 `policy_bind` 经共享环换表，仍是单写者）。槽一经发布内容不再改写
//! （写者只写**非活跃**槽），读者手里的引用至多在下下次轮转被复用——单核
//! 屏蔽窗下读者早已退出，无踩踏面。
//!
//! 语义映射的边界（写者是受信内核侧，拒收面在此收口）：
//! - **key == 0 保留**：空槽哨兵键，注入条目带 0 键直接丢弃（`proc_tag = 0`
//!   的内核任务本就不可注入）；
//! - **重复键 first-wins**：与旧线性扫 `find` 语义一致（先到先得）；
//! - **溢出拒收**：条目数超 [`TBL_MAX`] 或探测窗（[`PROBE_MAX`] 步）满 →
//!   丢弃该条——O(1) 查表上界的代价面；
//! - **权重三点夹紧**：build 时夹 + [`apply`]/[`weight_of`] 消费点再夹
//!   （±[`WEIGHT_MAX`] 单调收窄，F9②——夹取是唯一方向，无提权路径）。

use crate::task::{EppHint, Task};
use core::sync::atomic::{fence, AtomicU32, AtomicU64, Ordering};

/// 权重微调包络（±）：注入值由内核**夹紧**到本界内（动作空间③的 min/max
/// 夹紧——包络是内核执法，不信任注入方自律）。
pub const WEIGHT_MAX: i32 = 64;

/// 快照桶数（2^8——与 [`bucket_of`] 的移位量 56 绑定，改桶数须同步改散列）。
const BUCKETS: usize = 256;
/// 开放寻址线性探测步数上界（首桶 + 7 步；插入/查表同窗——插入必落在查表
/// 窗内，查表遇空槽早退才可靠，见 [`lookup_o1`]）。
const PROBE_MAX: usize = 8;
/// 单次绑定收表上限（装载因子 ≤ 50%——线性探测聚簇概率的上界控制）。
const TBL_MAX: usize = 128;
/// [`ACTIVE`] 的哨兵值：三槽 0..=2 之外的 3 = OFF/回退态（指向 [`EMPTY_SNAP`]）。
const SENTINEL_EMPTY: u32 = 3;

/// [`SnapSlot::epp_flags`] 位定义：bit0 = 带 EPP 覆盖，bit1 = EPP 取值
/// （0 = Efficiency / 1 = Perf）——`Option<EppHint>` 的两位打包。
const EPP_HAS: u8 = 1 << 0;
const EPP_VAL: u8 = 1 << 1;

/// 桶数须为 2 的幂（探测用 `& (BUCKETS-1)` 回绕 + 散列取高 log2 位）。
const _: () = assert!(BUCKETS.is_power_of_two());

/// 单条策略指令（v1 词汇表：③权重 + ④EPP + ①放置记录）。
#[derive(Debug, Clone, Copy)]
pub struct Directive {
    /// ① 放置 hint（目标核/簇；0 = 无意见）。v1 单核不消费，仅落字段。
    pub placement: u8,
    /// ③ 包络内权重微调（应用时夹紧 ±[`WEIGHT_MAX`]，写入 `prio_effective`）。
    pub weight: i32,
    /// ④ EPP 二值（`None` = 不覆盖）。
    pub epp: Option<EppHint>,
}

/// 表条目：`task_key` 命中的任务应用 `directive`。
///
/// v1 key = [`Task::proc_tag`]（进程标签；内核任务共 0——已有唯一稳定载体；
/// **0 键被空槽哨兵保留、注入即弃**）；v3 真 ABI 的 per-thread 键随
/// `policy_bind` 协议定（届时换键源，接口不变）。
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub task_key: u64,
    pub directive: Directive,
}

// ---- 快照表示（#[repr(C)]：槽内 key 落偏移 0，可按位审计）----

/// 快照槽：一条指令的扁平表示（16 字节）。`key == 0` = 空槽。
#[repr(C)]
struct SnapSlot {
    key: u64,
    weight: i32,
    placement: u8,
    /// bit0 = has_epp，bit1 = epp_val（见 [`EPP_HAS`]/[`EPP_VAL`]）。
    epp_flags: u8,
    _rsv: u16,
}

impl SnapSlot {
    /// 空槽（const 语境可用的全零字面量）。
    const EMPTY: SnapSlot = SnapSlot {
        key: 0,
        weight: 0,
        placement: 0,
        epp_flags: 0,
        _rsv: 0,
    };
}

/// 策略表快照：发布后**内容不可变**（写者只写非活跃槽——RCU 式换表）。
#[repr(C)]
struct Snap {
    /// 世代计数（每次 [`bind`] 递增；v3 遥测/换表协议的世代词汇先落地）。
    gen: u64,
    /// 实际入槽数（0 = 空表——[`apply`]/[`weight_of`] 的快路径分支位）。
    count: u32,
    _pad: u32,
    slots: [SnapSlot; BUCKETS],
}

impl Snap {
    /// 全零空快照（const fn——静态初始化 + build 基底两用）。
    const fn empty() -> Snap {
        Snap {
            gen: 0,
            count: 0,
            _pad: 0,
            slots: [SnapSlot::EMPTY; BUCKETS],
        }
    }
}

// ---- 三槽轮转 + 哨兵空表 ----

/// 三槽轮转区：写者写**非活跃**槽、翻转 [`ACTIVE`] 发布——读者经指针取整表。
/// `static mut`（写面——`&raw mut` 取裸指针，不立引用；`static` 非 mut 拒
/// 可变裸指针借用，E0596——同 `class.rs` LAST_VCT 纪律）。
static mut SNAPS: [Snap; 3] = [const { Snap::empty() }; 3];
/// 哨兵空表——OFF/回退态（`ACTIVE == SENTINEL_EMPTY` 时指向这里）。
static EMPTY_SNAP: Snap = Snap::empty();
/// 活跃槽号（0..=2）或 [`SENTINEL_EMPTY`]。发布序：槽写完 → fence(Release)
/// → 本位 store(Release)；读取 Acquire——读者看见槽号即看见槽内容。
static ACTIVE: AtomicU32 = AtomicU32::new(SENTINEL_EMPTY);
/// 世代计数器（单写者递增，非通信值——Relaxed 足够）。
static GEN_NEXT: AtomicU64 = AtomicU64::new(1);

/// 黄金比例乘移散列（乘 2^64/φ 取高 8 位 = 桶号）：键位（指针/标签）分布
/// 不均也摊平；`>> 56` 与 [`BUCKETS`] = 2^8 绑定。
#[inline]
fn bucket_of(key: u64) -> usize {
    (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 56) as usize
}

/// 发布新快照：三槽轮转写 + fence(Release) + ACTIVE.store(Release)。
///
/// 轮转 `next = (cur + 1) % 3` 恒 ≠ 当前活跃槽（读者在读的槽不被写；哨兵
/// 态 3 → 1 同样成立）。单写者（IRQ 屏蔽窗）无需 CAS。
fn publish(new: Snap) {
    let cur = ACTIVE.load(Ordering::Relaxed);
    let next = (cur + 1) % 3; // 三槽轮转：永避当前活跃槽（哨兵 3 → 1）
    // SAFETY: 单写者独占写**非活跃**槽；该槽自上次发布翻转后无读者可达
    // （读者只经 ACTIVE 指针到槽）——写窗内无别名引用。static 存储期 =
    // 整个程序运行期，ptr::write 不越界（槽号 < 3 编译期可证）。
    unsafe { core::ptr::write(&raw mut SNAPS[next as usize], new) };
    fence(Ordering::Release);
    ACTIVE.store(next, Ordering::Release);
}

/// 取活跃快照指针：Acquire 载入槽号映射到静态区地址（哨兵 → [`EMPTY_SNAP`]）。
/// 返回裸指针——发布后槽内容不可变，调用点一次解引用挂 SAFETY（读临界区
/// 内快照稳定）。
fn active_snap() -> *const Snap {
    // SAFETY: `static mut` 取 `&raw const` 裸指针（不立引用——E0133 解除的
    // 就是这一层）；解引用的读窗纪律在调用点（apply/weight_of 各挂 SAFETY）。
    unsafe {
        match ACTIVE.load(Ordering::Acquire) {
            0 => &raw const SNAPS[0],
            1 => &raw const SNAPS[1],
            2 => &raw const SNAPS[2],
            _ => &raw const EMPTY_SNAP, // OFF/回退态
        }
    }
}

/// O(1) 查表：开放寻址线性探测，≤[`PROBE_MAX`] 步；`key == 0`（空槽哨兵）
/// 早退。**空槽早退可靠性**：插入与查表走同一探测序列、插入落**首个**空槽
/// → 键在槽 i 当且仅当其探测序列 0..i 全被占——查表先遇空槽即可断缺席。
fn lookup_o1(snap: &Snap, key: u64) -> Option<&SnapSlot> {
    if key == 0 {
        return None; // 哨兵键：不可注入（内核任务 tag 0）
    }
    let mut idx = bucket_of(key);
    for _ in 0..PROBE_MAX {
        let slot = &snap.slots[idx];
        if slot.key == key {
            return Some(slot);
        }
        if slot.key == 0 {
            return None; // 探测序列遇空槽 → 键必不在表内（见上不变式）
        }
        idx = (idx + 1) & (BUCKETS - 1); // 2 的幂回绕（无除法）
    }
    None // 探测窗尽（表聚簇超窗）——缺席判定的保守侧
}

/// 快照插槽：探测窗内首个空槽写入；重复键 first-wins（旧线性扫 `find`
/// 语义保持）；窗满拒收返 false。返回是否真插入。
fn snap_insert(snap: &mut Snap, key: u64, d: &Directive) -> bool {
    let mut idx = bucket_of(key);
    for _ in 0..PROBE_MAX {
        let slot = &mut snap.slots[idx];
        if slot.key == key {
            return false; // 重复键：先到先得
        }
        if slot.key == 0 {
            slot.key = key;
            // build 点夹紧（第一道；消费点 apply/weight_of 再夹——三点执法）。
            slot.weight = d.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX);
            slot.placement = d.placement;
            slot.epp_flags = match d.epp {
                None => 0,
                Some(EppHint::Efficiency) => EPP_HAS,
                Some(EppHint::Perf) => EPP_HAS | EPP_VAL,
            };
            return true;
        }
        idx = (idx + 1) & (BUCKETS - 1);
    }
    false // 探测窗满：拒收（O(1) 查表上界的代价面）
}

/// 从 `&[Entry]` 构建快照：TBL_MAX 截尾、0 键丢弃、重复/溢出拒收（计数只
/// 记真入槽者）。build 后即不可变——发布后无人再改写。
fn build_snap(tbl: &[Entry], gen: u64) -> Snap {
    let mut snap = Snap::empty();
    let mut n: u32 = 0;
    for e in tbl.iter().take(TBL_MAX) {
        if e.task_key == 0 {
            continue; // 哨兵键保留
        }
        if snap_insert(&mut snap, e.task_key, &e.directive) {
            n += 1;
        }
    }
    snap.gen = gen;
    snap.count = n;
    snap
}

/// 绑定表（v1 仅内核侧 ktest/boot 期调用；v3 由 `policy_bind` 经共享环换表）。
/// 语义映射：`&[Entry]` → [`Snap`] 快照 + [`publish`] 原子换入。
pub fn bind(tbl: &'static [Entry]) {
    let gen = GEN_NEXT.fetch_add(1, Ordering::Relaxed); // 单写者，非通信值
    publish(build_snap(tbl, gen));
}

/// 解绑：翻转 [`ACTIVE`] 回 [`SENTINEL_EMPTY`] 哨兵（读路径回落
/// [`EMPTY_SNAP`]，恢复「接口缺席」态——等价性测试的对照分支；ktest 收尾
/// 清理）。
pub fn unbind() {
    ACTIVE.store(SENTINEL_EMPTY, Ordering::Release);
}

/// 接口是否在位（测试观察用）：已发布快照（含**空表**——接口在位、表空
/// 是等价性测试的主断言态）即 true，哨兵态 false。
pub fn bound() -> bool {
    ACTIVE.load(Ordering::Acquire) != SENTINEL_EMPTY
}

/// 决策路径挂钩：pick_next 出选后调用。查到指令 → 写 TCB 表示法字段
/// （**幂等**：`prio_effective = max(prio_base + 夹紧权重, pi_inherited)`
/// 自基线重算、EPP 直接赋值——非累积，重复 pick 不漂移）；查不到 → **零
/// 写入**，字段保持基线，与接口缺席逐位等价（ktest `POLICY ok` 断言对象）。
/// IRQ 热路径零分配。快路径：哨兵/空表两次分支即返（§2「开销恒定」的
/// 空态兑现）。
pub fn apply(task: &mut Task) {
    // SAFETY: 指针源自 active_snap（静态区槽位/哨兵，'static 生存期）；
    // 已发布槽内容不可变（写者只写非活跃槽）+ 单核读写窗互斥——读窗内快照
    // 稳定，本调用内无写者。
    let snap: &Snap = unsafe { &*active_snap() };
    if snap.count == 0 {
        return; // 空表/哨兵：零写入快路径
    }
    let key = task.proc_tag as u64;
    if key == 0 {
        return; // 哨兵键：内核任务不可注入（同旧表语义边界收紧，见模块注）
    }
    let slot = match lookup_o1(snap, key) {
        Some(s) => s,
        None => return, // 未命中：零写入
    };
    let w = slot.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX);
    // 4-3b：策略权重与 PI 捐赠同合成（冻结的 boost 不被 apply 抹平）。
    task.prio_effective = (task.prio_base + w).max(task.pi_inherited);
    if slot.epp_flags & EPP_HAS != 0 {
        task.epp = if slot.epp_flags & EPP_VAL != 0 {
            EppHint::Perf
        } else {
            EppHint::Efficiency
        };
    }
}

/// 该任务当前绑定的夹紧权重（未绑/无表/空表 → 0）。[`crate::sched::
/// refresh_effective`] 的合成分量——PI 重推刷 effective 时与 pick 侧 apply
/// 用同一份权重，两写入者公式一致才不会互相踩（4-3b）。
pub(crate) fn weight_of(task: &Task) -> i32 {
    // SAFETY: 读纪律同 apply（静态区指针 + 已发布槽不可变）。
    let snap: &Snap = unsafe { &*active_snap() };
    if snap.count == 0 {
        return 0;
    }
    let key = task.proc_tag as u64;
    if key == 0 {
        return 0;
    }
    match lookup_o1(snap, key) {
        Some(s) => s.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX),
        None => 0,
    }
}
