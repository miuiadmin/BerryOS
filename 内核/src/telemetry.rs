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

/// 唤醒钩子本体（`init` 注册进 `berry_sched`）。**零分配**：只写静态环 + 计数。
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

// ---- 4-3c intent 传播计数器（`IPC设计.md` §2.1 五件套之 5；IRQ 屏蔽路径
// 静态计数，零分配零拷贝——「值通胀」的可观测信号）----

/// 借入生效次数（msg_intent 抬升 server 档位的每次结算；高频公共服务被
/// 高档客户端触达即此数飙升 = 「一切皆顶带 = 没有 band」的早期信号）。
static mut INTENT_BORROWS: u64 = 0;
/// 消息链跳数帽拒绝传播次数（`intent::INTENT_HOP_MAX`）。
static mut INTENT_HOP_CAPPED: u64 = 0;
/// 时间帽拒绝传播次数（每跳预算 + 链累计封顶两路合计）。
static mut INTENT_BUDGET_CAPPED: u64 = 0;

/// 记一次借入生效（recv 结算 slowpath）。
pub fn intent_borrowed() {
    // SAFETY: 写点全程 IRQ 屏蔽（syscall 主体），单核串行。
    unsafe { INTENT_BORROWS += 1 };
}

/// 记一次跳数帽拒绝（recv 结算 slowpath）。
pub fn intent_hop_capped_inc() {
    // SAFETY: 同上。
    unsafe { INTENT_HOP_CAPPED += 1 };
}

/// 记一次时间帽拒绝（send 快照降级 / recv 结算拒传播）。
pub fn intent_budget_capped() {
    // SAFETY: 同上。
    unsafe { INTENT_BUDGET_CAPPED += 1 };
}

/// 借入生效累计。
pub fn intent_borrows() -> u64 {
    // SAFETY: 只读。
    unsafe { INTENT_BORROWS }
}

/// 跳数帽累计。
pub fn intent_hop_capped_total() -> u64 {
    // SAFETY: 只读。
    unsafe { INTENT_HOP_CAPPED }
}

/// 时间帽累计。
pub fn intent_budget_capped_total() -> u64 {
    // SAFETY: 只读。
    unsafe { INTENT_BUDGET_CAPPED }
}

// ---- 4-4 IPC fastpath/slowpath 计数器（`IPC设计.md` §7——快/慢路配比的
// 可观测信号；IRQ 屏蔽路径静态计数，零分配）----

/// fastpath 命中数（检查链全过 → 定点唤醒 + 直切判定）。
static mut IPC_FAST_HITS: u64 = 0;
/// slowpath 走数（检查链任一不满足 → `wake_matching` 全匹配——含「无等待者」
/// 的常态流水线 send；配比 = 快路覆盖率）。
static mut IPC_FAST_MISSES: u64 = 0;
/// 直切发生数（fastpath 唤醒后 `try_preempt` 真的切走——严格更高 eff 才切）。
static mut IPC_DIRECT: u64 = 0;

/// 记一次 fastpath 命中（send 检查链全过）。
pub fn ipc_fast_hit() {
    // SAFETY: 写点全程 IRQ 屏蔽（syscall 主体），单核串行。
    unsafe { IPC_FAST_HITS += 1 };
}

/// 记一次 slowpath（检查链不满足或无等待者）。
pub fn ipc_fast_miss() {
    // SAFETY: 同上。
    unsafe { IPC_FAST_MISSES += 1 };
}

/// 记一次直切（`try_preempt` 切走）。
pub fn ipc_direct_switch() {
    // SAFETY: 同上。
    unsafe { IPC_DIRECT += 1 };
}

/// fastpath 命中累计。
pub fn ipc_fast_hits() -> u64 {
    // SAFETY: 只读。
    unsafe { IPC_FAST_HITS }
}

/// slowpath 累计。
pub fn ipc_fast_misses() -> u64 {
    // SAFETY: 只读。
    unsafe { IPC_FAST_MISSES }
}

/// 直切累计。
pub fn ipc_direct_total() -> u64 {
    // SAFETY: 只读。
    unsafe { IPC_DIRECT }
}
