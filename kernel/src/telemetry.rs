//! 调度遥测环（3-4c；`可观测性设计.md` per-CPU 无锁环 + `AI调度设计.md` §4
//! 第一级观测量 / §9「v1 埋遥测 instrumentation」）。
//!
//! v1 形态 = **静态预储环形缓冲**（无堆、无锁）：写入点经 `berry_sched` 切换
//! 钩子（[`set_switch_hook`] 注册的 [`on_switch`]），三个切换位（yield/抢占/
//! exit）全程 IRQ 屏蔽 → 单核天然串行，写入路径**零分配**（try 化批的
//! 「IRQ 热路径零分配」纪律——`heap.rs` 分配上下文纪律不得从这里重开）。
//!
//! 记录量（AI调度 §4 第一级，QEMU 可靠子集）：切换时刻 CNTVCT（相邻差 =
//! prev 运行时长 → 运行时长分布）、任务 id（名字 FNV-1a 稳定哈希）、切换
//! 原因分型、是否切入 idle（→ util）。**唤醒事件已随 4-0 落地**（唤醒环
//! [`WAKE_RING`]：block→wake 延迟即「唤醒模式」分布数据，经 [`set_wake_hook`]
//! 注册的 [`on_wake`]——两个唤醒位 Signaled/Timeout 全程 IRQ 屏蔽，同款
//! 零分配纪律）。**消费路径留 v3**（`telemetry_open` 映射环给 trace 服务/
//! AI 策略服务）；v1 读点 = watchdog 存活性断言（环在涨、vct 单调、延迟
//! 非负）。
//!
//! 单核 v1 单环；SMP（G2）按核拆 per-CPU 环 + 写方限本核（设计文档原案）。

use berry_sched::{SwitchKind, Task, WakeReason};
use core::sync::atomic::{AtomicU64, Ordering};

/// 环容量（静态预储——2KB 级常驻 .bss，boot 后永不增长）。
pub const RING_LEN: usize = 64;

/// 单条切换记录（`#[repr(C)]`：将来 `telemetry_open` 映射给用户态的 ABI 形状
/// 从 day-one 定型；字段全定宽）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SwitchRec {
    /// 切换时刻 CNTVCT（原始周期数——换算 ns 需 CNTFRQ，留给消费方）。
    pub vct: u64,
    /// 切出任务 id（名字 FNV-1a-32；0 = idle/boot 前）。
    pub prev: u32,
    /// 切入任务 id。
    pub next: u32,
    /// 切换原因（[`SwitchKind`] 的 u8 编码：0=Yield 1=Preempt 2=Exit）。
    pub kind: u8,
    /// 切入是否为 idle 任务（idle 判据 = `kstack.is_none()`——boot 活上下文
    /// 不分配栈，见 `sched.rs` IDLE_TASK）。
    pub idle_next: bool,
}

/// 环本体 + 游标（单核 IRQ 屏蔽串行写；读点同 crate 单核顺序）。
static mut RING: [SwitchRec; RING_LEN] = [SwitchRec {
    vct: 0,
    prev: 0,
    next: 0,
    kind: 0,
    idle_next: false,
}; RING_LEN];
/// 下一写入槽（环形 +1；`TOTAL < RING_LEN` 时其前全有效）。
static mut HEAD: usize = 0;
/// 累计写入数（不减——wrap 后 `span()` 给环内有效条数）。
static mut TOTAL: u64 = 0;

/// 名字 → 稳定短 id（FNV-1a-32；展示端反查靠名字表，环内只存定宽 id）。
fn name_id(name: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in name {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// `kind` 的 u8 编码（与 `SwitchKind` 一一对应；解码归消费方）。
fn kind_code(kind: SwitchKind) -> u8 {
    match kind {
        SwitchKind::Yield => 0,
        SwitchKind::Preempt => 1,
        SwitchKind::Exit => 2,
        SwitchKind::Block => 3,
    }
}

/// 切换钩子本体（`init` 注册进 `berry_sched`）。**零分配**：只写静态环 + 计数。
fn on_switch(prev: &Task, next: &Task, kind: SwitchKind) {
    // SAFETY: 写点只经本函数；调用点 = 三个切换位，全程 IRQ 屏蔽（单核串行）。
    unsafe {
        let rec = SwitchRec {
            vct: berry_hal_aarch64::timer::cntvct(),
            prev: name_id(prev.name),
            next: name_id(next.name),
            kind: kind_code(kind),
            idle_next: next.kstack.is_none(),
        };
        RING[HEAD] = rec;
        HEAD = (HEAD + 1) % RING_LEN;
        TOTAL = TOTAL.wrapping_add(1);
    }
}

/// 注册切换钩子（`rust_main` 在 `berry_sched::init` 后、首个切换前调用一次）。
pub fn init() {
    berry_sched::set_switch_hook(Some(on_switch));
    berry_sched::set_wake_hook(Some(on_wake));
}

/// 累计写入数（遥测存活断言用：须 ≥ `berry_sched::switches()`——SWITCHES 只计
/// 抢占切换，遥测环计全部三型）。
pub fn total() -> u64 {
    // SAFETY: 只读；单核顺序（写点 IRQ 屏蔽串行）。
    unsafe { TOTAL }
}

/// 环内有效条数（min(TOTAL, RING_LEN)）。
pub fn span() -> usize {
    // SAFETY: 只读。
    let t = unsafe { TOTAL };
    (t as usize).min(RING_LEN)
}

/// 按新→旧序取第 `i` 条（0 = 最新；越界 None）。
pub fn last(i: usize) -> Option<SwitchRec> {
    // SAFETY: 只读；槽位在 span 内必有效。
    let s = span();
    if i >= s {
        return None;
    }
    // SAFETY: 只读。
    let head = unsafe { HEAD };
    let idx = (head + RING_LEN - 1 - i) % RING_LEN;
    // SAFETY: 只读静态环。
    Some(unsafe { RING[idx] })
}

/// 环健康自检（watchdog 周期断言）：①有货；②vct 从旧到新单调不减（CNTVCT
/// 只前进——切换记录乱序 = 环写路径有并发/覆盖 bug）。
pub fn check() -> bool {
    let s = span();
    if s == 0 {
        return false;
    }
    // last(i) 是新→旧；倒着走 = 旧→新，vct 必须单调不减。
    for i in (1..s).rev() {
        let (a, b) = (last(i).expect("span 内必有"), last(i - 1).expect("span 内必有"));
        if b.vct < a.vct {
            return false;
        }
    }
    true
}

// ---- 唤醒环（4-0 配套；block→wake 延迟 = AI调度 §4 第一级「唤醒模式」）----

/// 唤醒环容量（与切换环同规格——静态预储）。
pub const WAKE_RING_LEN: usize = 64;

/// 唤醒原因 u8 编码（`WakeReason` 消费面子集——`Pending` 是入队初值，
/// 永不进环；出现即写路径 bug，`wake_check` 拒）。
pub const REASON_SIGNALED: u8 = 1;
pub const REASON_TIMEOUT: u8 = 2;

/// 单条唤醒记录（`#[repr(C)]`：将来 `telemetry_open` 映射的 ABI 形状 day-one
/// 定型；字段全定宽，无 usize/bool——32B 整）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct WakeRec {
    /// 唤醒时刻 CNTVCT（减 `since_vct` = block→wake 延迟，原始周期数——
    /// 换算 ns 需 CNTFRQ，归消费方）。
    pub vct: u64,
    /// 入睡时刻 CNTVCT（`block_current` 戳，与 deadline 同屏蔽窗填）。
    pub since_vct: u64,
    /// 等待对象标识（0 = 纯 sleep；非 0 = 对象地址——展示端反查归消费方）。
    pub wait_obj: u64,
    /// 被唤醒任务 id（名字 FNV-1a-32）。
    pub task_id: u32,
    /// 唤醒原因（[`REASON_SIGNALED`]/[`REASON_TIMEOUT`]）。
    pub reason: u8,
    /// 定宽对齐垫（ABI 演进保留，恒 0）。
    pub _pad: [u8; 3],
}

/// 唤醒环本体 + 游标（写点 = 两个唤醒位，IRQ 屏蔽串行；读点同 crate 顺序）。
static mut WAKE_RING: [WakeRec; WAKE_RING_LEN] = [WakeRec {
    vct: 0,
    since_vct: 0,
    wait_obj: 0,
    task_id: 0,
    reason: 0,
    _pad: [0; 3],
}; WAKE_RING_LEN];
static mut WAKE_HEAD: usize = 0;
static mut WAKE_TOTAL: u64 = 0;

/// `WakeReason` → 环内 u8 编码（`Pending` 到不了这里——`note_wake` 只在
/// 置因之后发射）。
fn reason_code(reason: WakeReason) -> u8 {
    match reason {
        WakeReason::Pending => 0, // 不可达；显式映射防新变体漏网
        WakeReason::Signaled => REASON_SIGNALED,
        WakeReason::Timeout => REASON_TIMEOUT,
    }
}

/// 唤醒钩子本体（`init` 注册进 `berry_sched`）。**零分配**：只写静态环 + 计数
/// + R-0 增量直方图（同窗 O(1) 算术，与环写同量级——IRQ 屏蔽热路径纪律不破）。
fn on_wake(task: &Task, reason: WakeReason) {
    // SAFETY: 写点只经本函数；调用点 = 两个唤醒位，全程 IRQ 屏蔽（单核串行）。
    unsafe {
        let rec = WakeRec {
            vct: berry_hal_aarch64::timer::cntvct(),
            since_vct: task.wait_since,
            wait_obj: task.wait_obj as u64,
            task_id: name_id(task.name),
            reason: reason_code(reason),
            _pad: [0; 3],
        };
        WAKE_RING[WAKE_HEAD] = rec;
        WAKE_HEAD = (WAKE_HEAD + 1) % WAKE_RING_LEN;
        WAKE_TOTAL = WAKE_TOTAL.wrapping_add(1);
        // R-0 增量直方图：延迟 = 唤醒时刻 − 入睡时刻（原始周期数——公约 5，
        // 不走 ns 换算）。负延迟 = 时钟倒走 = 睡眠戳/写点 bug → 严格断言
        // panic（写点执法盖**全历史**，比 wake_check 的环内 64 条更全；设计
        // §9 门禁授权的 L0 路径）。log2 桶：lat=0（同刻醒）归桶 0；否则
        // idx = 63 − leading_zeros（lat ≥ 1 ⇒ lz ≤ 63，lat ≥ 2^63 恰归桶
        // 63——无下溢无溢出）。
        let lat = match rec.vct.checked_sub(rec.since_vct) {
            Some(l) => l,
            None => {
                panic!("WAKE: 负延迟（vct={} < since={}）= 睡眠戳/写点 bug", rec.vct, rec.since_vct)
            }
        };
        let idx = if lat == 0 { 0 } else { 63 - lat.leading_zeros() as usize };
        (*core::ptr::addr_of_mut!(WAKE_HIST))[idx] += 1;
        *core::ptr::addr_of_mut!(WAKE_HIST_N) += 1;
        if lat < *core::ptr::addr_of!(WAKE_HIST_MIN) {
            *core::ptr::addr_of_mut!(WAKE_HIST_MIN) = lat;
        }
        if lat > *core::ptr::addr_of!(WAKE_HIST_MAX) {
            *core::ptr::addr_of_mut!(WAKE_HIST_MAX) = lat;
        }
        if rec.reason == REASON_SIGNALED {
            *core::ptr::addr_of_mut!(WAKE_HIST_SIG) += 1;
        } else {
            // Timeout（Pending 编码 0 不可达——note_wake 只在置因后发射）。
            *core::ptr::addr_of_mut!(WAKE_HIST_TMO) += 1;
        }
    }
}

/// 唤醒环累计写入数（watchdog 断言用——boot sleeper 双路径后必 ≥ 2）。
pub fn wake_total() -> u64 {
    // SAFETY: 只读；单核顺序（写点 IRQ 屏蔽串行）。
    unsafe { WAKE_TOTAL }
}

/// 唤醒环内有效条数（min(WAKE_TOTAL, WAKE_RING_LEN)）。
pub fn wake_span() -> usize {
    // SAFETY: 只读。
    let t = unsafe { WAKE_TOTAL };
    (t as usize).min(WAKE_RING_LEN)
}

/// 按新→旧序取第 `i` 条（0 = 最新；越界 None）。
pub fn wake_last(i: usize) -> Option<WakeRec> {
    let s = wake_span();
    if i >= s {
        return None;
    }
    // SAFETY: 只读；槽位在 span 内必有效。
    let head = unsafe { WAKE_HEAD };
    let idx = (head + WAKE_RING_LEN - 1 - i) % WAKE_RING_LEN;
    Some(unsafe { WAKE_RING[idx] })
}

/// 环内出现过的唤醒原因掩码（bit0=Signaled、bit1=Timeout）。watchdog 断言
/// boot 双路径覆盖用——**仅环未淘汰时有效**（boot 记录被新条目顶掉后，
/// 掩码只反映环内现状，不再断言）。
pub fn wake_reason_mask() -> u8 {
    let mut m = 0u8;
    for i in 0..wake_span() {
        let r = wake_last(i).expect("span 内必有");
        if r.reason == REASON_SIGNALED {
            m |= 1;
        } else if r.reason == REASON_TIMEOUT {
            m |= 2;
        }
    }
    m
}

/// 唤醒环健康自检（watchdog 周期断言）：①有货；②每条原因合法（∈
/// Signaled/Timeout——出现 0 = Pending 漏进环 = 写路径 bug）；③延迟非负
///（`vct ≥ since_vct`——唤醒时刻早于入睡 = 戳或环有 bug）；④vct 从旧到新
/// 单调不减。
pub fn wake_check() -> bool {
    let s = wake_span();
    if s == 0 {
        return false;
    }
    for i in 0..s {
        let r = wake_last(i).expect("span 内必有");
        if (r.reason != REASON_SIGNALED && r.reason != REASON_TIMEOUT)
            || r.vct < r.since_vct
        {
            return false;
        }
    }
    for i in (1..s).rev() {
        let (a, b) =
            (wake_last(i).expect("span 内必有"), wake_last(i - 1).expect("span 内必有"));
        if b.vct < a.vct {
            return false;
        }
    }
    true
}

// ---- R-0 唤醒延迟增量直方图（`基准与压力测试设计.md` §9；与唤醒环**并存**：
// 环 = 最近 64 条明细（wrap 丢头），桶 = 全程覆盖的 O(1) 增量统计——看形状
// 靠桶、看明细靠环；精确分位归 R-1 采样集全排序，桶只给保守上界）----

/// log2 延迟桶数：桶 i 计延迟 ∈ [2^i, 2^(i+1)) 的唤醒次数；延迟 0（同刻醒）
/// 归桶 0，延迟 ≥ 2^63 归桶 63（打印端上界给 u64::MAX——2^64 溢出防）。
pub const WAKE_HIST_BUCKETS: usize = 64;

/// 直方图快照（打印端一次取走；[u64;64] ≈ 512B 栈副本，boot 尾一次，非热路径）。
pub struct WakeHist {
    /// log2 延迟桶计数（下标 i ⇔ 延迟 ∈ [2^i, 2^(i+1)) 周期数）。
    pub buckets: [u64; WAKE_HIST_BUCKETS],
    /// 样本总数（与 [`wake_total`] 同函数同窗递增，恒相等）。
    pub n: u64,
    /// 精确最小延迟（u64::MAX 哨兵 ⇔ 零样本——打印端 n>0 门控后必真值）。
    pub min: u64,
    /// 精确最大延迟（0 ⇔ 零样本）。
    pub max: u64,
    /// Signaled 唤醒分计数。
    pub sig: u64,
    /// Timeout 唤醒分计数。
    pub tmo: u64,
}

static mut WAKE_HIST: [u64; WAKE_HIST_BUCKETS] = [0; WAKE_HIST_BUCKETS];
static mut WAKE_HIST_N: u64 = 0;
static mut WAKE_HIST_MIN: u64 = u64::MAX;
static mut WAKE_HIST_MAX: u64 = 0;
static mut WAKE_HIST_SIG: u64 = 0;
static mut WAKE_HIST_TMO: u64 = 0;

/// 直方图快照（只读）。**读序纪律**（读方可抢占、写方在 IRQ 屏蔽单窗内完成
/// 全部字段更新，撕裂只可能是「n 旧、桶/极值新一位」）：n 先读 → 后读的桶
/// 之和恒 ≥ n（分位扫描必达，不耗尽）；min 先于 max 读 → min ≤ max 恒立
///（min 只减、max 只增，min_old ≤ max_old ≤ max_new）——打印端的两条形状
/// 断言（n>0、min≤max）在撕裂窗内也不会假阳性。
pub fn wake_hist() -> WakeHist {
    // SAFETY: 只读；单核顺序（写点 IRQ 屏蔽串行），按上述读序取快照。
    let n = unsafe { *core::ptr::addr_of!(WAKE_HIST_N) };
    let src = core::ptr::addr_of!(WAKE_HIST);
    let mut buckets = [0u64; WAKE_HIST_BUCKETS];
    for i in 0..WAKE_HIST_BUCKETS {
        // SAFETY: 只读静态数组；i 在界内。
        buckets[i] = unsafe { (*src)[i] };
    }
    // SAFETY: 只读。
    let min = unsafe { *core::ptr::addr_of!(WAKE_HIST_MIN) };
    // SAFETY: 只读（须后于 min——见读序纪律）。
    let max = unsafe { *core::ptr::addr_of!(WAKE_HIST_MAX) };
    // SAFETY: 只读。
    let sig = unsafe { *core::ptr::addr_of!(WAKE_HIST_SIG) };
    // SAFETY: 只读。
    let tmo = unsafe { *core::ptr::addr_of!(WAKE_HIST_TMO) };
    WakeHist { buckets, n, min, max, sig, tmo }
}

// ---- 4-3c intent 传播计数器（`IPC设计.md` §2.1 五件套之 5；IRQ 屏蔽路径
// 静态计数，零分配零拷贝——「值通胀」的可观测信号）。S-2 I50 起原子化
//（fetch_add/load Relaxed）：写点仍在锁窗内单写者，读点（watchdog/断言/
// 采集）可在锁窗外安全读——S-2b 副核 PAR 金丝雀的消费面。----

/// 借入生效次数（msg_intent 抬升 server 档位的每次结算；高频公共服务被
/// 高档客户端触达即此数飙升 = 「一切皆顶带 = 没有 band」的早期信号）。
static INTENT_BORROWS: AtomicU64 = AtomicU64::new(0);
/// 消息链跳数帽拒绝传播次数（`intent::INTENT_HOP_MAX`）。
static INTENT_HOP_CAPPED: AtomicU64 = AtomicU64::new(0);
/// 时间帽拒绝传播次数（每跳预算 + 链累计封顶两路合计）。
static INTENT_BUDGET_CAPPED: AtomicU64 = AtomicU64::new(0);

/// 记一次借入生效（recv 结算 slowpath）。
pub fn intent_borrowed() {
    INTENT_BORROWS.fetch_add(1, Ordering::Relaxed);
}

/// 记一次跳数帽拒绝（recv 结算 slowpath）。
pub fn intent_hop_capped_inc() {
    INTENT_HOP_CAPPED.fetch_add(1, Ordering::Relaxed);
}

/// 记一次时间帽拒绝（send 快照降级 / recv 结算拒传播）。
pub fn intent_budget_capped() {
    INTENT_BUDGET_CAPPED.fetch_add(1, Ordering::Relaxed);
}

/// 借入生效累计。
pub fn intent_borrows() -> u64 {
    INTENT_BORROWS.load(Ordering::Relaxed)
}

/// 跳数帽累计。
pub fn intent_hop_capped_total() -> u64 {
    INTENT_HOP_CAPPED.load(Ordering::Relaxed)
}

/// 时间帽累计。
pub fn intent_budget_capped_total() -> u64 {
    INTENT_BUDGET_CAPPED.load(Ordering::Relaxed)
}

// ---- 4-4 IPC fastpath/slowpath 计数器（`IPC设计.md` §7——快/慢路配比的
// 可观测信号；IRQ 屏蔽路径静态计数，零分配）。S-2 I50 起原子化（同上：
// 写点锁窗内单写者，读点锁窗外安全）----

/// fastpath 命中数（检查链全过 → 定点唤醒 + 直切判定）。
static IPC_FAST_HITS: AtomicU64 = AtomicU64::new(0);
/// slowpath 走数（检查链任一不满足 → `wake_matching` 全匹配——含「无等待者」
/// 的常态流水线 send；配比 = 快路覆盖率）。
static IPC_FAST_MISSES: AtomicU64 = AtomicU64::new(0);
/// 直切发生数（fastpath 唤醒后 `try_preempt` 真的切走——严格更高 eff 才切）。
static IPC_DIRECT: AtomicU64 = AtomicU64::new(0);

/// 记一次 fastpath 命中（send 检查链全过）。
pub fn ipc_fast_hit() {
    IPC_FAST_HITS.fetch_add(1, Ordering::Relaxed);
}

/// 记一次 slowpath（检查链不满足或无等待者）。
pub fn ipc_fast_miss() {
    IPC_FAST_MISSES.fetch_add(1, Ordering::Relaxed);
}

/// 记一次直切（`try_preempt` 切走）。
pub fn ipc_direct_switch() {
    IPC_DIRECT.fetch_add(1, Ordering::Relaxed);
}

/// fastpath 命中累计。
pub fn ipc_fast_hits() -> u64 {
    IPC_FAST_HITS.load(Ordering::Relaxed)
}

/// slowpath 累计。
pub fn ipc_fast_misses() -> u64 {
    IPC_FAST_MISSES.load(Ordering::Relaxed)
}

/// 直切累计。
pub fn ipc_direct_total() -> u64 {
    IPC_DIRECT.load(Ordering::Relaxed)
}
