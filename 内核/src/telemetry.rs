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
//! 原因分型、是否切入 idle（→ util）。唤醒事件 v1 无源（无阻塞原语）——
//! P4 object_wait 落地后补。**消费路径留 v3**（`telemetry_open` 映射环给
//! trace 服务/AI 策略服务）；v1 读点 = watchdog 存活性断言（环在涨、vct
//! 单调）。
//!
//! 单核 v1 单环；SMP（G2）按核拆 per-CPU 环 + 写方限本核（设计文档原案）。

use berry_sched::{SwitchKind, Task};

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
