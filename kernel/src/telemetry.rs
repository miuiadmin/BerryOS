//! 调度遥测环（3-4c；`可观测性设计.md` per-CPU 无锁环 + `AI调度设计.md` §4
//! 第一级观测量 / §9「v1 埋遥测 instrumentation」）。
//!
//! v1 形态 = **静态预储环形缓冲**（无堆、无锁）：写入点经 `berry_sched` 切换
//! 钩子（[`set_switch_hook`] 注册的 [`on_switch`]），三个切换位（yield/抢占/
//! exit）→ 写入路径**零分配**（try 化批的「IRQ 热路径零分配」纪律——
//! `heap.rs` 分配上下文纪律不得从这里重开）。
//!
//! 记录量（AI调度 §4 第一级，QEMU 可靠子集）：切换时刻 CNTVCT（相邻差 =
//! prev 运行时长 → 运行时长分布）、任务 id（名字 FNV-1a 稳定哈希）、切换
//! 原因分型、是否切入 idle（→ util）。**唤醒事件已随 4-0 落地**（唤醒环：
//! block→wake 延迟即「唤醒模式」分布数据，经 [`set_wake_hook`] 注册的
//! [`on_wake`]）。**消费路径留 v3**（`telemetry_open` 映射环给 trace 服务/
//! AI 策略服务）；v1 读点 = watchdog 存活性断言（环在涨、vct 单调、延迟
//! 非负）。
//!
//! **S-3c B12/B13：per-CPU 双环 + seqlock（拆 KERN_LOCK 前置件）**。写方限
//! 本核（设计原案「per-CPU 环 + 写方限本核」兑现）：切换/唤醒钩子跑在发生
//! 切换/唤醒的核上，只写自己的环——写-写天然无竞态；跨核读者（watchdog/
//! BENCH 采集跑在任一核）经 **seqlock**（版本号奇偶 + Release/Acquire 对）
//! 取一致快照。KL 纪元（S-3b 前）写点仍全程 KL 窗内串行，seqlock 是零成本
//! 冗余；c-2 拆锁后它就是唯一保护。直方图五计数器原子化（fetch_add/
//! fetch_min/fetch_max——单核直写形态退役），写序 = 桶/极值在前、N 的
//! Release 递增**最后**，读者 N 的 Acquire 读**最先**——「桶和 ≥ n」与
//! 「n>0 ⇒ min≠哨兵」两条形状断言在无锁窗下仍立（读序纪律见 [`wake_hist`]）。
//!
//! **刀1（PM-17 §5）：per-source 唤醒记账**（[`WAKE_SRC_TABLE`]——归因 =
//! hal 语境 cell，见 `berry_hal_aarch64::wake`）。**不随 trace_off 关**
//! （计数器族先例——观测计数 ≠ 环记录面，双跑只切后者）；环条目随之带
//! `source` 字段（`WakeRec`），G1.5 出守恒/双路径/冻结断言 + `WAKESRC`
//! 单行。

use berry_hal_aarch64::smp::cpu_id;
use berry_hal_aarch64::wake_gen::{
    WAKE_SRC_IPC, WAKE_SRC_KERNEL_BOOT, WAKE_SRC_NONE, WAKE_SRC_POWER_KEY, WAKE_SRC_SGI_RESCHED,
    WAKE_SRC_SGI_TLB, WAKE_SRC_TIMER, WAKE_SRC_UART_RX, WAKE_SRC_UNKNOWN,
};
use berry_sched::{SwitchKind, Task, WakeReason};
use berry_telface::HIST_BUCKETS;
use core::sync::atomic::{fence, AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};

/// 环容量（静态预储——2KB 级常驻 .bss，boot 后永不增长；per-CPU 后总量
/// = 2×两环，仍 2KB 级）。
pub const RING_LEN: usize = 64;

/// per-CPU 环槽数（S-2b 起参与上限 2 核；拓扑真化后扩——遥测环静态定容，
/// boot 前注册不受 HAL 活性时序影响）。
const NCPUS: usize = 2;

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

/// 名字 → 稳定短 id：**单源 = berry-telface（W3-Q10 迁出）**——FNV-1a-32
/// 稳定哈希（offset basis/prime 是日志对照面的契约常量）；本文件 re-export
/// 保 crate 内路径稳定，host 钉板在 telface（已知向量/序判别/稳定性）。
pub use berry_telface::name_id;

/// `kind` 的 u8 编码（与 `SwitchKind` 一一对应；解码归消费方）。
fn kind_code(kind: SwitchKind) -> u8 {
    match kind {
        SwitchKind::Yield => 0,
        SwitchKind::Preempt => 1,
        SwitchKind::Exit => 2,
        SwitchKind::Block => 3,
    }
}

// ---- per-CPU 切换环（B12）----

/// 单核环本体（写方 = 本核钩子**唯一**；读者任意核经 seqlock）。
struct SwitchRing {
    /// seqlock 版本：写方 even→odd（进入）→even（退出）；读者见 odd 或版本
    /// 变化即重试。
    seq: AtomicU64,
    /// 下一写入槽（环形 +1；`total < RING_LEN` 时其前全有效）——写方私有。
    head: usize,
    /// 累计写入数（不减——wrap 后 [`ring_span`] 给环内有效条数）——写方私有。
    total: u64,
    /// 环槽——写方私有（读者经版本号校验后只读）。
    recs: [SwitchRec; RING_LEN],
}

impl SwitchRing {
    const fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            head: 0,
            total: 0,
            recs: [SwitchRec { vct: 0, prev: 0, next: 0, kind: 0, idle_next: false }; RING_LEN],
        }
    }
}

/// per-CPU 切换环（静态预储；`static mut` + 写方限本核 = 全仓内核静态惯式）。
static mut SWITCH_RINGS: [SwitchRing; NCPUS] = [SwitchRing::new(), SwitchRing::new()];

/// 本核环写入一笔（seqlock 写方协议：odd 进 → 数据 → Release even 出）。
unsafe fn ring_write(rec: SwitchRec) {
    let cpu = cpu_id().min(NCPUS - 1); // 拓扑槽外防御：收敛尾槽（boot 期 mp 不明）
    // SAFETY: 写方限本核——本核槽无他核写者；读者经 seq 校验不读半写。
    let r = &mut (*core::ptr::addr_of_mut!(SWITCH_RINGS))[cpu];
    let s = r.seq.load(Ordering::Relaxed);
    r.seq.store(s + 1, Ordering::Relaxed); // odd：写方进入
    fence(Ordering::Release); // 数据写不得前移出窗
    r.recs[r.head] = rec;
    r.head = (r.head + 1) % RING_LEN;
    r.total = r.total.wrapping_add(1);
    fence(Ordering::Release);
    r.seq.store(s + 2, Ordering::Release); // even：数据先于版本号可见
}

/// 切换钩子本体（`init` 注册进 `berry_sched`）。**零分配**：只写本核静态环
/// + 计数。
fn on_switch(prev: &Task, next: &Task, kind: SwitchKind) {
    if trace_off() {
        return; // G3-4b 记录面关：rec 构造（cntvct + 双名字哈希）与环写整体让位。
    }
    let rec = SwitchRec {
        vct: berry_hal_aarch64::timer::cntvct(),
        prev: name_id(prev.name),
        next: name_id(next.name),
        kind: kind_code(kind),
        idle_next: next.kstack.is_none(),
    };
    // SAFETY: 写方限本核（见 ring_write）；调用点 = 三个切换位。
    unsafe { ring_write(rec) }
}

/// 注册切换钩子（`rust_main` 在 `berry_sched::init` 后、首个切换前调用一次）。
pub fn init() {
    // G3-4b trace 开/关双跑门（草案 §4）：fw_cfg 具名项 `opt/berry/trace` 值
    // "0"（恰 1 字节）= 记录面关；缺席/他值 = 开（默认面零漂移——run/smoke
    // 不递送该键，全部既有通道行为不变；rt/lat 同形具名门先例）。一次定值
    // 在此（init 早于首个钩子发射；boot 串行窗无竞速面），此后只读。
    if let Some(e) = berry_hal_aarch64::fwcfg::find_file("opt/berry/trace") {
        if e.size == 1 {
            let mut v = [0u8; 1];
            if berry_hal_aarch64::fwcfg::read_to(&e, &mut v) && v[0] == b'0' {
                // Release/Acquire〔f4 F11〕：今日跨核可见性由 boot 序结构
                // 承重（本 init 早于副核出生 + PSCI CPU_ON 串行边界，副核
                // 首次读即见终值）；显式边为真机移植/bring-up 提前的形态
                // 兜底——钩子槽写与读本标志间不再依赖位置巧合。
                TRACE_OFF.store(true, Ordering::Release);
            }
        }
    }
    berry_sched::set_switch_hook(Some(on_switch));
    berry_sched::set_wake_hook(Some(on_wake));
}

/// 遥测记录面关标志（G3-4b 双跑通道的 off 臂）。关 = 两个钩子整体让位
/// （环写 seqlock + 24/32B 拷贝 + 栅栏 + 直方图五计数器）——被测的就是
/// 这笔热路径足迹。**计数器族（intent/IPC fastpath）与 `note_wake` 戳不
/// 随关**：前者是 boot 断言面（intent_borrows 等在册执法），后者是
/// bench-lat 测量基座（G3-4a）——观测计数 ≠ 环记录面，双跑只切后者。
static TRACE_OFF: AtomicBool = AtomicBool::new(false);

/// 记录面是否关（true = 环/直方图写让位；watchdog 的 G1.5 环断言与
/// BENCHWAKE 打印同让位——断言对象不在场还执法即错杀，`user.rs` 同步门）。
pub fn trace_off() -> bool {
    TRACE_OFF.load(Ordering::Acquire) // 与置位 Release 配对〔f4 F11〕
}

/// seqlock 读者：取 `cpu` 环内第 k 新条目（k=0 最新；空/越界 None；版本
/// 变化重试）。head/total/槽三读须同窗一致——版本号兜底。
fn ring_peek(cpu: usize, k: usize) -> Option<SwitchRec> {
    // SAFETY: 只读；半写窗由 seq 校验排除（重试）。
    let r = unsafe { &(*core::ptr::addr_of!(SWITCH_RINGS))[cpu] };
    loop {
        let s1 = r.seq.load(Ordering::Acquire);
        if s1 & 1 == 1 {
            continue; // 写方在窗内
        }
        let total = r.total;
        let head = r.head;
        if k >= total.min(RING_LEN as u64) as usize {
            // 版本校验仍要走完（防「早退读了撕裂数据」路径被优化依赖）。
            fence(Ordering::Acquire);
            let s2 = r.seq.load(Ordering::Relaxed);
            if s1 == s2 {
                return None;
            }
            continue;
        }
        let idx = (head + RING_LEN - 1 - k) % RING_LEN;
        let rec = r.recs[idx];
        fence(Ordering::Acquire); // 槽读不得后移过复检
        let s2 = r.seq.load(Ordering::Relaxed);
        if s1 == s2 {
            return Some(rec);
        }
    }
}

/// `cpu` 环内有效条数（min(total, RING_LEN)）。
fn ring_span(cpu: usize) -> usize {
    // SAFETY: 只读 total（写方限本核，u64 读撕裂不在单核原子性讨论域）。
    let t = unsafe { (*core::ptr::addr_of!(SWITCH_RINGS))[cpu].total };
    (t as usize).min(RING_LEN)
}

/// `cpu` 环累计写入数。
fn ring_total(cpu: usize) -> u64 {
    // SAFETY: 只读。
    unsafe { (*core::ptr::addr_of!(SWITCH_RINGS))[cpu].total }
}

/// 累计写入数（全环求和——每环单调不减，和单调；遥测存活断言用：须 ≥
/// `berry_sched::switches_tt()`——环只记任务↔任务切换，S-6 ① 起分母同口径
/// 双计数器；SWITCHES 11 位全计含 park 语境，非本断言比较面）。
pub fn total() -> u64 {
    ring_total(0) + ring_total(1)
}

/// 全环合并有效条数（各环 min(total, RING_LEN) 之和，帽 2×RING_LEN）。
pub fn span() -> usize {
    ring_span(0) + ring_span(1)
}

/// 按新→旧序取合并第 `i` 条（0 = 最新；两环按 vct 归并——锁序=时序=vct 序
/// 三位一体（义务⑨），vct 归并序 = 全局真序；越界 None）。
pub fn last(i: usize) -> Option<SwitchRec> {
    let mut k = [0usize; NCPUS]; // 各环已取条数（游标）
    for _ in 0..=i {
        // 两环各取当前游标条，vct 大者（更新）胜出。
        let cands: [Option<SwitchRec>; NCPUS] = [ring_peek(0, k[0]), ring_peek(1, k[1])];
        let winner = match (cands[0], cands[1]) {
            (Some(a), Some(b)) => {
                if a.vct >= b.vct {
                    0
                } else {
                    1
                }
            }
            (Some(_), None) => 0,
            (None, Some(_)) => 1,
            (None, None) => return None, // 合并流尽
        };
        let picked = cands[winner];
        k[winner] += 1;
        if k[0] + k[1] - 1 == i {
            return picked;
        }
    }
    None // 不可达（循环内必命中或流尽）
}

/// 环健康自检（watchdog 周期断言）：①有货；②vct 从旧到新单调不减（CNTVCT
/// 只前进——两环各自单调（单写者），vct 归并不破单调）。
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
    /// 唤醒归因源（刀1：[`WAKE_SRC_*`](berry_hal_aarch64::wake_gen)——
    /// `on_wake` 读 hal cell；NONE/UNKNOWN 写点 panic，环内恒 ∈ 6 真源）。
    pub source: u8,
    /// 定宽对齐垫（ABI 演进保留，恒 0）。
    pub _pad: [u8; 2],
}
const _: () = assert!(core::mem::size_of::<WakeRec>() == 32);

/// `WakeReason` → 环内 u8 编码（`Pending` 到不了这里——`note_wake` 只在
/// 置因之后发射）。
fn reason_code(reason: WakeReason) -> u8 {
    match reason {
        WakeReason::Pending => 0, // 不可达；显式映射防新变体漏网
        WakeReason::Signaled => REASON_SIGNALED,
        WakeReason::Timeout => REASON_TIMEOUT,
    }
}

/// 单核唤醒环（同 [`SwitchRing`] 协议：写方限本核 + seqlock 读者）。
struct WakeRing {
    seq: AtomicU64,
    head: usize,
    total: u64,
    recs: [WakeRec; WAKE_RING_LEN],
}

impl WakeRing {
    const fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            head: 0,
            total: 0,
            recs: [WakeRec {
                vct: 0,
                since_vct: 0,
                wait_obj: 0,
                task_id: 0,
                reason: 0,
                source: 0,
                _pad: [0; 2],
            }; WAKE_RING_LEN],
        }
    }
}

static mut WAKE_RINGS: [WakeRing; NCPUS] = [WakeRing::new(), WakeRing::new()];

/// 本核唤醒环写入一笔（seqlock 写方协议同 [`ring_write`]）。
unsafe fn wake_ring_write(rec: WakeRec) {
    let cpu = cpu_id().min(NCPUS - 1);
    // SAFETY: 写方限本核；读者经 seq 校验。
    let r = &mut (*core::ptr::addr_of_mut!(WAKE_RINGS))[cpu];
    let s = r.seq.load(Ordering::Relaxed);
    r.seq.store(s + 1, Ordering::Relaxed);
    fence(Ordering::Release);
    r.recs[r.head] = rec;
    r.head = (r.head + 1) % WAKE_RING_LEN;
    r.total = r.total.wrapping_add(1);
    fence(Ordering::Release);
    r.seq.store(s + 2, Ordering::Release);
}

/// 唤醒钩子本体（`init` 注册进 `berry_sched`）。**零分配**：只写本核静态环
/// + 原子计数 + R-0 增量直方图（同窗 O(1) 算术，与环写同量级——热路径纪律
/// 不破）。
fn on_wake(task: &Task, reason: WakeReason) {
    // 刀1 per-source 记账（**不随 trace_off 关**——计数器族先例，见
    // [`TRACE_OFF`] 注：观测计数 ≠ 环记录面）。归因读 hal cell：任务语境
    // 基线 IPC / IRQ 臂源（TIMER/UART_RX/SGI）/ boot 期 KERNEL_BOOT。
    // NONE 到场 = cell 未接线语境（写点漏路由）、UNKNOWN 到场 = 未知 IRQ
    // 有唤醒路径（src_of 漏臂）——皆写路径 bug，写点 panic（L0 执法，盖
    // 全历史——负延迟同族）。
    let src = berry_hal_aarch64::wake::wake_src_current();
    assert!(
        src != WAKE_SRC_NONE,
        "WAKE: 源=NONE 到达记账（cell 未接线语境——note_switch/exception_irq 写点漏路由）"
    );
    assert!(
        src != WAKE_SRC_UNKNOWN,
        "WAKE: 源=UNKNOWN 到达记账（未知 IRQ 不该有唤醒路径——src_of 漏臂或 cell 直写）"
    );
    // sim-S2 A4②：POWER_KEY 同 NONE/UNKNOWN 写点纪律——v1 电源键恒无持有者
    //（无 Event/wake_matching 面，通知槽回报恒 false），到达只许 IRQ 级计数；
    // 此处到场 = 无主 IRQ 长出了唤醒路径（v2 电源服务面违约前置），L0 执行
    // 在写点炸响（环侧 wake_check ⑤ 是事后审计面）。
    assert!(
        src != WAKE_SRC_POWER_KEY,
        "WAKE: 源=POWER_KEY 到达记账（v1 电源键恒无唤醒路径——通知槽契约违约）"
    );
    let now = berry_hal_aarch64::timer::cntvct();
    {
        let slot = &WAKE_SRC_TABLE[(src as usize).min(WAKE_SRC_SLOTS - 1)];
        slot.wakes.fetch_add(1, Ordering::Relaxed);
        slot.first_ts.fetch_min(now, Ordering::Relaxed);
        slot.last_ts.fetch_max(now, Ordering::Relaxed);
        slot.last_woken_id.store(name_id(task.name), Ordering::Relaxed);
        let cls = wake_src_class(src);
        if cls != 0 {
            slot.flags.fetch_or(cls, Ordering::Relaxed);
        }
    }
    // 独立总账（守恒断言的右侧：Σ 槽 wakes == 本值——漏记/双记执法）。
    // Release 末位与 wake_hist N 同纪律：读者先 Acquire 总账，则凡已计入
    // N 的样本槽写已可见；并发瞬态只保证 Σ≥N。
    WAKE_ACCT_N.fetch_add(1, Ordering::Release);
    if trace_off() {
        return; // 环/直方图记录面关（上方 per-source 计数不随关）
    }
    let rec = WakeRec {
        vct: now,
        since_vct: task.wait_since,
        wait_obj: task.wait_obj as u64,
        task_id: name_id(task.name),
        reason: reason_code(reason),
        source: src,
        _pad: [0; 2],
    };
    // SAFETY: 写方限本核（见 wake_ring_write）；调用点 = 两个唤醒位。
    unsafe { wake_ring_write(rec) }
    // R-0 增量直方图：延迟 = 唤醒时刻 − 入睡时刻（原始周期数——公约 5，
    // 不走 ns 换算）。负延迟 = 时钟倒走 = 睡眠戳/写点 bug → 严格断言
    // panic（写点执法盖**全历史**，比 wake_check 的环内条目更全；设计
    // §9 门禁授权的 L0 路径）。log2 桶：lat=0（同刻醒）归桶 0；否则
    // idx = 63 − leading_zeros（lat ≥ 1 ⇒ lz ≤ 63，lat ≥ 2^63 恰归桶
    // 63——无下溢无溢出）。
    let lat = match rec.vct.checked_sub(rec.since_vct) {
        Some(l) => l,
        None => {
            panic!("WAKE: 负延迟（vct={} < since={}）= 睡眠戳/写点 bug", rec.vct, rec.since_vct)
        }
    };
    // 桶律单源 = berry-telface::latency_bucket（W3-Q10——host 钉板全域
    // 边界；此处消费同函数，零第二实现）。
    let idx = berry_telface::latency_bucket(lat);
    // S-3c B13：直方图原子化（单核直写退役）——写序纪律 = 桶/极值/分计在
    // 前，N 的 Release 递增**最后**（读者 N 的 Acquire 读最先，见 wake_hist）。
    WAKE_HIST[idx].fetch_add(1, Ordering::Relaxed);
    WAKE_HIST_MIN.fetch_min(lat, Ordering::Relaxed);
    WAKE_HIST_MAX.fetch_max(lat, Ordering::Relaxed);
    if rec.reason == REASON_SIGNALED {
        WAKE_HIST_SIG.fetch_add(1, Ordering::Relaxed);
    } else {
        // Timeout（Pending 编码 0 不可达——note_wake 只在置因后发射）。
        WAKE_HIST_TMO.fetch_add(1, Ordering::Relaxed);
    }
    WAKE_HIST_N.fetch_add(1, Ordering::Release);
}

// ---- 刀1 per-source 唤醒记账（PM-17 终审 §5；不随 trace_off 关）----
//
// 形状决策（vs 裁决原案的两处记录在案偏差，详见 docs/内核唤醒记账设计）：
// 槽表 128 槽**值直索引**（裁决案 32 槽计数枚举——值直索引让 UNKNOWN 0x7F
// 有真槽可落到达计数，槽表 .bss 8KB 同量级）；`last_woken_id` 记名字哈希
//（Process 无 pid 面，id 稳定性名字承重——演进节）。

/// 槽表槽数 = 值域上界 + 1（源值 ≤ 0x7F；值直索引——`table[src]`）。
pub const WAKE_SRC_SLOTS: usize = 128;

/// 源分类位（add-only 位面；未定义源恒 0——未知位 mask 语义归消费方）。
pub const WSRC_F_TIMER_CLASS: u16 = 1 << 0; // 定时器类（TIMER）
pub const WSRC_F_IRQ_LINE: u16 = 1 << 1; // 外设线（UART_RX）
pub const WSRC_F_SGI_INTERNAL: u16 = 1 << 2; // SGI 内部（RESCHED/TLB）
pub const WSRC_F_HAD_NO_WAITER: u16 = 1 << 3; // 曾零唤醒 sticky（UART 到达无等待者）
pub const WSRC_F_SLEPT: u16 = 1 << 4; // SLEPT 域事件见过（v2 挂起双钟预留，v1 恒不置）

/// 单源记账槽（`#[repr(C)]` 64B 整——`telemetry_open` 快照面的 ABI 形状
/// day-one 定型；字段全定宽原子——多写者 = 本核 on_wake / IRQ 臂计数点 /
/// G1.5 合并，全原子无锁。原子字段 !Copy !Clone ⇒ 槽表初始化走 inline
/// const 数组（`[const { new() }; 128]`）。
#[repr(C)]
pub struct WakeSrcRec {
    /// 本源唤醒事件数（on_wake 记）。
    pub wakes: AtomicU64,
    /// 本源 IRQ 到达数（TIMER/UART 计数点直入；SGI×2/UNKNOWN = G1.5 快照
    /// 合并——HAL 侧臂计数无内核回调槽）。
    pub irq_count: AtomicU64,
    /// 到达而无等待者（UART：醒-再睡窗内到达的良性竞态——位已置、
    /// poll-first 必命中，无丢唤醒；只出数不设门禁）。
    pub no_waiter: AtomicU64,
    /// 首次唤醒时刻 CNTVCT（哨兵 `u64::MAX`，fetch_min——未触槽可判）。
    pub first_ts: AtomicU64,
    /// 最近唤醒时刻（fetch_max）。
    pub last_ts: AtomicU64,
    /// 最近 IRQ 到达/ack 戳（v1 = 到达戳：TIMER 槽派发点、UART rx_notify
    /// pre-EOI；post-EOI ack 接力精化归 v2——设计文档演进节）。
    pub last_ack_ts: AtomicU64,
    /// 最近被唤醒任务 id（名字 FNV-1a-32）。
    pub last_woken_id: AtomicU32,
    /// 分类/sticky 位（上方 [`WSRC_F_*`]）。
    pub flags: AtomicU16,
    /// 时戳域（v1 恒 0 = CNTVCT 单域；v2 SLEPT 域引入后区分——G1.5 断言
    /// v1 活动槽恒 0）。
    pub ts_domain: u8,
    pub _pad: [u8; 5],
}
const _: () = assert!(core::mem::size_of::<WakeSrcRec>() == 64);

impl WakeSrcRec {
    const fn new() -> Self {
        Self {
            wakes: AtomicU64::new(0),
            irq_count: AtomicU64::new(0),
            no_waiter: AtomicU64::new(0),
            first_ts: AtomicU64::new(u64::MAX),
            last_ts: AtomicU64::new(0),
            last_ack_ts: AtomicU64::new(0),
            last_woken_id: AtomicU32::new(0),
            flags: AtomicU16::new(0),
            ts_domain: 0,
            _pad: [0; 5],
        }
    }
}

/// per-source 记账槽表（值直索引；.bss 8KB 静态预储——boot 后永不增长。
/// `WakeSrcRec` 原子字段 !Copy ⇒ inline const 数组初始化）。
pub static WAKE_SRC_TABLE: [WakeSrcRec; WAKE_SRC_SLOTS] =
    [const { WakeSrcRec::new() }; WAKE_SRC_SLOTS];

/// 唤醒记账独立总账（守恒断言右侧：Σ 全槽 wakes == 本值）。
static WAKE_ACCT_N: AtomicU64 = AtomicU64::new(0);

/// 读独立总账（WAKESRC 打印面；守恒断言走 [`wake_acct_conserved`]）。
pub fn wake_acct_n() -> u64 {
    WAKE_ACCT_N.load(Ordering::Acquire)
}

/// 稳定窗守恒检查：双读总账与槽和，两对读数均无变化且相等才真。写序为
/// 槽先、N 末位 Release；并发瞬态 Σ≥N，一次广播可醒任意多任务，因此不
/// 用固定差值帽。64 次仍不稳定 = 高频写流中的读窗失败，由调用方 loud。
pub fn wake_acct_conserved() -> bool {
    for _ in 0..64 {
        let n1 = WAKE_ACCT_N.load(Ordering::Acquire);
        let s1 = wake_src_total_wakes();
        let n2 = WAKE_ACCT_N.load(Ordering::Acquire);
        let s2 = wake_src_total_wakes();
        if n1 == n2 && s1 == s2 {
            return n1 == s1;
        }
        core::hint::spin_loop();
    }
    false
}

/// 源值 → 分类位（纯函数；ktest 面）。未定义源 = 0。
pub fn wake_src_class(src: u8) -> u16 {
    match src {
        WAKE_SRC_TIMER => WSRC_F_TIMER_CLASS,
        WAKE_SRC_UART_RX | WAKE_SRC_POWER_KEY => WSRC_F_IRQ_LINE,
        WAKE_SRC_SGI_RESCHED | WAKE_SRC_SGI_TLB => WSRC_F_SGI_INTERNAL,
        _ => 0,
    }
}

/// 源值是否「可进环」的已定义唤醒源（NONE 哨兵 / UNKNOWN / POWER_KEY
/// 只进 IRQ 级到达计数、永不进环——写点 panic 在前，本谓词是环条目事后
/// 审计面；ktest 面）。未定义值（0x08–0x7E 预留段——0x07 自 sim-S2 起为
/// POWER_KEY 到达-only 占用）恒 false。POWER_KEY 有意不在白名单：v1 无主
/// 到达恒无唤醒，若进环 = 写路径违约，wake_check ⑤ 在此收口。
pub fn wake_src_valid(src: u8) -> bool {
    matches!(
        src,
        WAKE_SRC_KERNEL_BOOT
            | WAKE_SRC_TIMER
            | WAKE_SRC_UART_RX
            | WAKE_SRC_SGI_RESCHED
            | WAKE_SRC_SGI_TLB
            | WAKE_SRC_IPC
    )
}

/// 源 wakes 计数读点（G1.5 断言/打印）。
pub fn wake_src_wakes(src: u8) -> u64 {
    WAKE_SRC_TABLE[(src as usize).min(WAKE_SRC_SLOTS - 1)].wakes.load(Ordering::Relaxed)
}

/// Σ 全槽 wakes（守恒断言左侧——只有已定义源可递增，NONE/UNKNOWN 写点
/// panic 先拦；POWER_KEY 槽到达-only 不计 wakes，0x08–0x7E 未触）。
pub fn wake_src_total_wakes() -> u64 {
    let mut sum = 0u64;
    for s in &WAKE_SRC_TABLE {
        sum = sum.wrapping_add(s.wakes.load(Ordering::Relaxed));
    }
    sum
}

/// per-source 记账快照（G1.5 消费；各字段独立原子读，非一致快照——计数族
/// 只增不减，撕裂只在读窗内自差，断言面不承重）。
#[derive(Debug, Clone, Copy)]
pub struct WakeSrcSnapshot {
    pub src: u8,
    pub wakes: u64,
    pub irq: u64,
    pub no_waiter: u64,
    pub first_ts: u64,
    pub last_ts: u64,
    pub last_ack_ts: u64,
    pub last_woken_id: u32,
    pub flags: u16,
    pub ts_domain: u8,
}

/// 取 `src` 槽快照（未触槽：first_ts = `u64::MAX`、计数全 0——ktest 面）。
pub fn wake_src_snapshot(src: u8) -> WakeSrcSnapshot {
    let s = &WAKE_SRC_TABLE[(src as usize).min(WAKE_SRC_SLOTS - 1)];
    WakeSrcSnapshot {
        src,
        wakes: s.wakes.load(Ordering::Relaxed),
        irq: s.irq_count.load(Ordering::Relaxed),
        no_waiter: s.no_waiter.load(Ordering::Relaxed),
        first_ts: s.first_ts.load(Ordering::Relaxed),
        last_ts: s.last_ts.load(Ordering::Relaxed),
        last_ack_ts: s.last_ack_ts.load(Ordering::Relaxed),
        last_woken_id: s.last_woken_id.load(Ordering::Relaxed),
        flags: s.flags.load(Ordering::Relaxed),
        ts_domain: s.ts_domain,
    }
}

/// IRQ 到达记账（内核侧计数点：TIMER tick 槽包装 / UART rx_notify）。
/// `stamp` = 到达/派发戳（`last_ack_ts` 语义见 [`WakeSrcRec`]）。IRQ 语境
/// 纪律同 rx_irq_notify：零分配、纯原子。
pub fn note_irq_arrival(src: u8, stamp: u64) {
    let slot = &WAKE_SRC_TABLE[(src as usize).min(WAKE_SRC_SLOTS - 1)];
    slot.irq_count.fetch_add(1, Ordering::Relaxed);
    slot.last_ack_ts.store(stamp, Ordering::Relaxed);
    let cls = wake_src_class(src);
    if cls != 0 {
        slot.flags.fetch_or(cls, Ordering::Relaxed);
    }
}

/// 「到达而无等待者」记账（UART rx_notify n==0 臂——良性竞态，见
/// [`WSRC_F_HAD_NO_WAITER`]；计数 + sticky，不设门禁）。
pub fn note_irq_no_waiter(src: u8) {
    let slot = &WAKE_SRC_TABLE[(src as usize).min(WAKE_SRC_SLOTS - 1)];
    slot.no_waiter.fetch_add(1, Ordering::Relaxed);
    slot.flags.fetch_or(WSRC_F_HAD_NO_WAITER, Ordering::Relaxed);
}

/// HAL 侧到达计数快照合并（G1.5 boot 尾一次）：SGI×2 / 未知臂计数在 HAL
/// （无内核回调槽），此处并入槽表——store 单调值幂等，可重入。
pub fn wake_src_merge_hal() {
    let r = berry_hal_aarch64::wake::sgi_resched_arrivals() as u64;
    WAKE_SRC_TABLE[(WAKE_SRC_SGI_RESCHED as usize).min(WAKE_SRC_SLOTS - 1)]
        .irq_count
        .store(r, Ordering::Relaxed);
    WAKE_SRC_TABLE[(WAKE_SRC_SGI_RESCHED as usize).min(WAKE_SRC_SLOTS - 1)]
        .flags
        .fetch_or(WSRC_F_SGI_INTERNAL, Ordering::Relaxed);
    let t = berry_hal_aarch64::wake::sgi_tlb_arrivals() as u64;
    WAKE_SRC_TABLE[(WAKE_SRC_SGI_TLB as usize).min(WAKE_SRC_SLOTS - 1)]
        .irq_count
        .store(t, Ordering::Relaxed);
    WAKE_SRC_TABLE[(WAKE_SRC_SGI_TLB as usize).min(WAKE_SRC_SLOTS - 1)]
        .flags
        .fetch_or(WSRC_F_SGI_INTERNAL, Ordering::Relaxed);
    let u = berry_hal_aarch64::wake::unknown_arrivals() as u64;
    WAKE_SRC_TABLE[(WAKE_SRC_UNKNOWN as usize).min(WAKE_SRC_SLOTS - 1)]
        .irq_count
        .store(u, Ordering::Relaxed);
}

/// seqlock 读者：取 `cpu` 唤醒环内第 k 新条目（同 [`ring_peek`] 协议）。
fn wake_ring_peek(cpu: usize, k: usize) -> Option<WakeRec> {
    // SAFETY: 只读；半写窗由 seq 校验排除。
    let r = unsafe { &(*core::ptr::addr_of!(WAKE_RINGS))[cpu] };
    loop {
        let s1 = r.seq.load(Ordering::Acquire);
        if s1 & 1 == 1 {
            continue;
        }
        let total = r.total;
        let head = r.head;
        if k >= total.min(WAKE_RING_LEN as u64) as usize {
            fence(Ordering::Acquire);
            let s2 = r.seq.load(Ordering::Relaxed);
            if s1 == s2 {
                return None;
            }
            continue;
        }
        let idx = (head + WAKE_RING_LEN - 1 - k) % WAKE_RING_LEN;
        let rec = r.recs[idx];
        fence(Ordering::Acquire);
        let s2 = r.seq.load(Ordering::Relaxed);
        if s1 == s2 {
            return Some(rec);
        }
    }
}

fn wake_ring_span(cpu: usize) -> usize {
    // SAFETY: 只读 total。
    let t = unsafe { (*core::ptr::addr_of!(WAKE_RINGS))[cpu].total };
    (t as usize).min(WAKE_RING_LEN)
}

fn wake_ring_total(cpu: usize) -> u64 {
    // SAFETY: 只读。
    unsafe { (*core::ptr::addr_of!(WAKE_RINGS))[cpu].total }
}

/// 唤醒环累计写入数（全环求和；watchdog 断言用——boot sleeper 双路径后
/// 必 ≥ 2）。
pub fn wake_total() -> u64 {
    wake_ring_total(0) + wake_ring_total(1)
}

/// 唤醒环合并有效条数（各环 min 之和）。
pub fn wake_span() -> usize {
    wake_ring_span(0) + wake_ring_span(1)
}

/// 按新→旧序取合并第 `i` 条（0 = 最新；vct 归并同 [`last`]；越界 None）。
pub fn wake_last(i: usize) -> Option<WakeRec> {
    let mut k = [0usize; NCPUS];
    for _ in 0..=i {
        let cands: [Option<WakeRec>; NCPUS] =
            [wake_ring_peek(0, k[0]), wake_ring_peek(1, k[1])];
        let winner = match (cands[0], cands[1]) {
            (Some(a), Some(b)) => {
                if a.vct >= b.vct {
                    0
                } else {
                    1
                }
            }
            (Some(_), None) => 0,
            (None, Some(_)) => 1,
            (None, None) => return None,
        };
        let picked = cands[winner];
        k[winner] += 1;
        if k[0] + k[1] - 1 == i {
            return picked;
        }
    }
    None
}

/// 环内出现过的唤醒原因掩码（bit0=Signaled、bit1=Timeout）。watchdog 断言
/// boot 双路径覆盖用——**仅环未淘汰时有效**（boot 记录被新条目顶掉后，
/// 掩码只反映环内现状，不再断言；boot 期单核 → 记录全在环 0，per-CPU 化
/// 不改此断言面）。
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

/// 环健康自检（watchdog 周期断言）：①有货；②每条原因合法（∈
/// Signaled/Timeout——出现 0 = Pending 漏进环 = 写路径 bug）；③延迟非负
///（`vct ≥ since_vct`——唤醒时刻早于入睡 = 戳或环有 bug）；④vct 从旧到新
/// 单调不减（两环各单调 + vct 归并不破单调）；⑤刀1：归因源合法（∈ 6 真源
/// ——NONE/UNKNOWN/未定义值进环 = on_wake 断言被绕过或环撕写）。
pub fn wake_check() -> bool {
    let s = wake_span();
    if s == 0 {
        return false;
    }
    for i in 0..s {
        let r = wake_last(i).expect("span 内必有");
        if (r.reason != REASON_SIGNALED && r.reason != REASON_TIMEOUT)
            || !wake_src_valid(r.source)
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
// 环 = 最近条目明细（wrap 丢头），桶 = 全程覆盖的 O(1) 增量统计——看形状
// 靠桶、看明细靠环；精确分位归 R-1 采样集全排序，桶只给保守上界）----

/// log2 延迟桶数：桶 i 计延迟 ∈ [2^i, 2^(i+1)) 的唤醒次数；延迟 0（同刻醒）
/// 归桶 0，延迟 ≥ 2^63 归桶 63（打印端上界给 u64::MAX——2^64 溢出防）。
/// **桶数与桶律单源 = berry-telface（W3-Q10）**——本常量 re-export（静态
/// 数组长度承重），桶函数 `latency_bucket` 在 telface。
pub const WAKE_HIST_BUCKETS: usize = HIST_BUCKETS;

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

// S-3c B13：五计数器 + 桶全原子（拆 KL 后多核并发 fetch_*；KL 纪元单写者
// 语义不变——fetch_* 是直写的严格上位替代）。
static WAKE_HIST: [AtomicU64; WAKE_HIST_BUCKETS] =
    [const { AtomicU64::new(0) }; WAKE_HIST_BUCKETS];
static WAKE_HIST_N: AtomicU64 = AtomicU64::new(0);
static WAKE_HIST_MIN: AtomicU64 = AtomicU64::new(u64::MAX);
static WAKE_HIST_MAX: AtomicU64 = AtomicU64::new(0);
static WAKE_HIST_SIG: AtomicU64 = AtomicU64::new(0);
static WAKE_HIST_TMO: AtomicU64 = AtomicU64::new(0);

/// 直方图快照（只读）。**读序纪律**（S-3c B13 无锁窗版）：N 先读（Acquire，
/// 与写方末位 Release 递增配对——凡计入 n 的样本，其桶/极值写已可见）→
/// 后读的桶之和恒 ≥ n（之后落桶的只增不减，分位扫描必达，不耗尽）→ min
/// 先于 max 读（min 只减、max 只增，min_read ≤ min_old ≤ max_old ≤
/// max_read 恒立）——打印端的两条形状断言（n>0 ⇒ min≠哨兵、min≤max）在
/// 并发窗内也不会假阳性。
pub fn wake_hist() -> WakeHist {
    let n = WAKE_HIST_N.load(Ordering::Acquire);
    let mut buckets = [0u64; WAKE_HIST_BUCKETS];
    for i in 0..WAKE_HIST_BUCKETS {
        buckets[i] = WAKE_HIST[i].load(Ordering::Relaxed);
    }
    let min = WAKE_HIST_MIN.load(Ordering::Relaxed);
    // 须后于 min（见读序纪律）。
    let max = WAKE_HIST_MAX.load(Ordering::Relaxed);
    let sig = WAKE_HIST_SIG.load(Ordering::Relaxed);
    let tmo = WAKE_HIST_TMO.load(Ordering::Relaxed);
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

/// 1.1/S4：记一次跨域消息（recv 侧 billing.domain_id != 0——受益者付费
/// 的观测面；S5 起从此计数升级为 per-domain CPU 时长账户）。
pub fn billing_cross_domain() {
    BILLING_CROSS_DOMAIN.fetch_add(1, Ordering::Relaxed);
}

/// S4 跨域消息累计。
#[allow(dead_code)] // S5-b 起遥测环消费
pub fn billing_cross_domain_total() -> u64 {
    BILLING_CROSS_DOMAIN.load(Ordering::Relaxed)
}

// ---- 1.1/S5.11 freeze 遥测（进现有环——不建第二数据库 #45）----

/// S5.11：freeze 成功计数（process_suspend 成功尾）。
pub fn freeze_ok() {
    FREEZE_OK.fetch_add(1, Ordering::Relaxed);
}

/// S5.11：freeze 拒绝计数（谓词不过/token 上限/pager 拒）。
pub fn freeze_busy() {
    FREEZE_BUSY.fetch_add(1, Ordering::Relaxed);
}

/// S5.11：auto-thaw 计数（SuspendToken Drop → thaw）。
pub fn auto_thaw() {
    AUTO_THAW.fetch_add(1, Ordering::Relaxed);
}

pub fn freeze_ok_total() -> u64 { FREEZE_OK.load(Ordering::Relaxed) }
pub fn freeze_busy_total() -> u64 { FREEZE_BUSY.load(Ordering::Relaxed) }
pub fn auto_thaw_total() -> u64 { AUTO_THAW.load(Ordering::Relaxed) }

/// sim-S3（PM-16 §4.4 步3）：深睡降级计数（request_deep_sleep 落 freeze
/// 原语降级路径——PWRDEG `d=` 判据载体，进 freeze 族现有环）。
pub fn note_degrade_freeze() {
    DEGRADE_FREEZE.fetch_add(1, Ordering::Relaxed);
}

/// sim-S3：深睡降级累计（ktest PWRDEG 标记行直读台账）。
pub fn degrade_freeze_total() -> u64 { DEGRADE_FREEZE.load(Ordering::Relaxed) }

static FREEZE_OK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static FREEZE_BUSY: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static AUTO_THAW: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static DEGRADE_FREEZE: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

// ---- am/AM-2 等页停顿遥测（plans/am/design.md §压力信号；进现有环——
// 不建第二数据库 #45。口径 = CNTVCT 原始周期数，同 bench.rs 纪律：TCG 下
// CNTV tick ≈ SLICE_HZ/2 频偏——只作相对/单调判据，绝对阈值定标挂 HDR
// 实测（design AM-2【推】，lmkd 70/700ms 量级仅参考不落码）。记哪三处：
// probe(EL1) FAULT_KEY 睡眠段 / probe_el0 EFAULT_KEYS[slot] 睡眠段 /
// probe_el0 池满 EOVERFLOW_KEY convoy 等待段——AM-01「等页时间 = PSI 等
// 价」判据的 BerryOS 落点，v1 消费者 = 查询面（ktest/faulter 自证）；阈值
// 事件带内出口 = AM-2b 压力 Event 用户位（2026-11-08 提案候选①拍板——
// 见下窗口聚合块）。----

/// AM-2：等页停顿记账（睡前提快照、醒后差值——调用点在 dpg 两臂探针）。
/// max 饱和更新竞态下可能瞬时低估——遥测面可接受（非断言面）。
/// AM-2b 裁决④：+1 窗口累计原子（同窗 fetch_add——判阈/翻卷在 note 点
/// fold 惰性付费，见下窗口聚合块；非压力态零翻卷成本）。
pub fn dpg_stall_note(cyc: u64) {
    DPG_STALL_CYC.fetch_add(cyc, Ordering::Relaxed);
    DPG_STALL_N.fetch_add(1, Ordering::Relaxed);
    DPG_WIN_CYC.fetch_add(cyc, Ordering::Relaxed);
    let mut cur = DPG_STALL_MAX.load(Ordering::Relaxed);
    while cyc > cur {
        match DPG_STALL_MAX.compare_exchange_weak(cur, cyc, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(c) => cur = c,
        }
    }
}

pub fn dpg_stall_cyc_total() -> u64 { DPG_STALL_CYC.load(Ordering::Relaxed) }
pub fn dpg_stall_n_total() -> u64 { DPG_STALL_N.load(Ordering::Relaxed) }
pub fn dpg_stall_max() -> u64 { DPG_STALL_MAX.load(Ordering::Relaxed) }

static DPG_STALL_CYC: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static DPG_STALL_N: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static DPG_STALL_MAX: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

// ---- AM-2b 压力窗口聚合（2026-11-08 提案 §二·候选①主会话裁决）：
// partial 占比判据 Σcyc/W ≥ θ_p（PSI some 等价；θ_p 初值 10%【推，实测
// 定标】），跨阈经 boot 注册的出口回调置系统压力 Event 用户位 1<<40
//（main.rs `pressure_partial_cross`——rx_irq_notify 形写位+SQ 窗唤醒，
// 裁决⑤）；complete（700ms）位登记预留不置位（waiter 深度跟踪后置，
// 裁决②）。翻卷 = note 点惰性（dpg 三计值点已有 cntvct，跨窗才付费）；
// 写者保持内核任务语境（dpg 探针路径），免 event.rs 非 SVC 写者修约
//（裁决④）。窗口长 boot 期按 CNTFRQ 换算周期数（TCG 频偏——禁编译期
// 常量，裁决②）。----

/// 当前窗口长（周期数；0 = boot 接线前——dpg 计值点全在 init 后，fold 快退）。
static DPG_WIN_LEN: AtomicU64 = AtomicU64::new(0);
/// 当前窗口起点（CNTVCT；翻卷时 CAS 推进——胜者折卷）。
static DPG_WIN_START: AtomicU64 = AtomicU64::new(0);
/// 当前窗口 Σcyc（`dpg_stall_note` 同窗 fetch_add——共享单累计、双观察窗
/// 双阈值的省原子形，提案 §二；complete 窗启用后同源续判）。
static DPG_WIN_CYC: AtomicU64 = AtomicU64::new(0);
/// 跨阈出口回调（boot 单线程注册后不变——RX_EV_REF 同款纪律；None 承载
/// 「未接线」态而非伪装 fn）。
static mut PRESSURE_OUTLET: Option<fn()> = None;

/// θ_p 初值 = 10%（裁决②【推，实测定标】；判据式 Σcyc×100 ≥ W×θ_p——
/// 乘法实现免除法，翻卷点才付）。
const DPG_WIN_THETA_P_PERCENT: u64 = 10;

/// AM-2b：boot 接线——窗口长按 CNTFRQ 换算 + 起点钉当前 + 出口注册
///（main 单线程调，先于 ktest::run 与一切 dpg 计值点）。
pub(crate) fn dpg_win_init(win_ms: u64, freq: u64, now: u64, outlet: fn()) {
    DPG_WIN_LEN.store(freq.saturating_mul(win_ms) / 1000, Ordering::Relaxed);
    DPG_WIN_START.store(now, Ordering::Relaxed);
    // SAFETY: boot 单线程写、先于一切读者（RX_EV_H 口径）。
    unsafe { *core::ptr::addr_of_mut!(PRESSURE_OUTLET) = Some(outlet); }
}

/// 读当前窗口长（ktest 测毕还原 boot 换算值用——换算公式单源在本模块）。
#[inline]
pub(crate) fn dpg_win_len() -> u64 {
    DPG_WIN_LEN.load(Ordering::Relaxed)
}

/// AM-2b：note 点惰性翻卷（dpg 三计值点 note 后调，`now` = 该点 cntvct）。
/// 未跨窗快退（非压力态零翻卷成本）；跨窗以起点 CAS 收敛并发折卷（败者
/// 即返——Σ 由胜者 swap 清零、判阈由胜者独占）。边界诚实近似：翻卷前夜
/// note 进本的 cyc 记旧窗账（telemetry 级可接受，同 DPG_STALL 族 Relaxed
/// 口径）；静默窗推迟到下一 note 才折卷 = 只在压力持续（note 不断流）时
/// 通知及时（提案 §二迟滞论证）。
pub(crate) fn dpg_stall_fold(now: u64) {
    let len = DPG_WIN_LEN.load(Ordering::Relaxed);
    if len == 0 {
        return; // boot 接线前（不可达——计值点全在 init 后，防御快退）
    }
    let start = DPG_WIN_START.load(Ordering::Relaxed);
    if now.wrapping_sub(start) < len {
        return; // 未跨窗——非压力态快退
    }
    if DPG_WIN_START
        .compare_exchange(start, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return; // 并发折卷败者（单核抢占点下面——胜者已清 Σ/判阈）
    }
    let sum = DPG_WIN_CYC.swap(0, Ordering::Relaxed);
    // θ_p 占比判据（×100 免除法；saturating_mul 防多 waiter Σ>W 病态面
    //——提案 §二「全局 Σ 分不清并发 waiter 数」同源）。
    if sum.saturating_mul(100) >= len.saturating_mul(DPG_WIN_THETA_P_PERCENT) {
        // SAFETY: boot 注册后不变（只读）；未注册 = 未接线（不可达——
        // ktest 与 dpg 计值点均在 boot 接线后，防御跳过）。
        if let Some(f) = unsafe { *core::ptr::addr_of!(PRESSURE_OUTLET) } {
            f();
        }
    }
}

// ---- AM-2b ktest 注入口（#[cfg] 不适用——ktest 与本体同 crate 直调；
// pub(crate) + 调用点仅 ktest = 非测试面零成本。注入走 DPG_WIN_CYC 专用
// 累计、不经 `dpg_stall_note`——E3g 台账零态不变量不污染）----

/// 合成窗口：钉窗长/起点并清 Σ（测毕以 boot 换算值还原——`dpg_win_len`
/// 存底）。
#[inline]
pub(crate) fn dpg_win_ktest_setup(len: u64, start: u64) {
    DPG_WIN_LEN.store(len, Ordering::Relaxed);
    DPG_WIN_START.store(start, Ordering::Relaxed);
    DPG_WIN_CYC.store(0, Ordering::Relaxed);
}

/// 注入窗口累计（合成序列——真路径由 `dpg_stall_note` 的 fetch_add 走
/// 同一槽位）。
#[inline]
pub(crate) fn dpg_win_ktest_inject(cyc: u64) {
    DPG_WIN_CYC.fetch_add(cyc, Ordering::Relaxed);
}

static BILLING_CROSS_DOMAIN: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

// ---- 1.1/S4-b：per-domain CPU 时长账户（区间计费法）----

/// S4-b 区间计费 per-CPU 槽（SMP 数组化——cpu_id() 索引）：recv 侧设
/// domain + CNTVCT 戳，send 侧算差 → billing_charge。v1 诚实近似：send
/// 前可能穿插其他任务（wall-clock 而非 pure CPU——精确 per-task 需
/// TCB 扩字段，留 S5-c 后）。
pub static BILLING_RECV_DOMAIN: [core::sync::atomic::AtomicU64; 2] = [
    const { core::sync::atomic::AtomicU64::new(0) }; 2];
pub static BILLING_RECV_VCT: [core::sync::atomic::AtomicU64; 2] = [
    const { core::sync::atomic::AtomicU64::new(0) }; 2];

/// 当前 CPU 的计费槽索引（SMP 安全——cpu_id() 本核恒有效）。
#[inline]
pub fn billing_slot() -> usize {
    berry_hal_aarch64::smp::cpu_id() as usize & 1 // 2 核帽
}

/// 账户表容量（v1 目标 ≤4 deadline 进程 + fair 域——8 槽余量）。
pub const BILLING_SLOTS: usize = 8;

/// 账户槽：`(domain_id, cpu_ns)` 对——domain_id=0 = 空槽。
static BILLING_DOMAINS: [core::sync::atomic::AtomicU64; BILLING_SLOTS] = [
    const { core::sync::atomic::AtomicU64::new(0) }; BILLING_SLOTS];
static BILLING_CPU_NS: [core::sync::atomic::AtomicU64; BILLING_SLOTS] = [
    const { core::sync::atomic::AtomicU64::new(0) }; BILLING_SLOTS];

/// S4-b：向域账户充 CPU 时长（ns）。域不存在 → 新槽；表满 → 最小槽覆写
///（v1 诚实取舍：满表时牺牲最冷域，不静默丢账）。
pub fn billing_charge(domain_id: u64, ns: u64) {
    if domain_id == 0 || ns == 0 {
        return;
    }
    use core::sync::atomic::Ordering;
    // 槽查找/新建。
    for i in 0..BILLING_SLOTS {
        let d = BILLING_DOMAINS[i].load(Ordering::Relaxed);
        if d == domain_id {
            BILLING_CPU_NS[i].fetch_add(ns, Ordering::Relaxed);
            return;
        }
        if d == 0 {
            // 空槽 → 抢占（CAS 防双占）。
            if BILLING_DOMAINS[i]
                .compare_exchange(0, domain_id, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                BILLING_CPU_NS[i].fetch_add(ns, Ordering::Relaxed);
                return;
            }
        }
    }
    // 表满 → 覆写最小账户（诚实取舍，防静默丢账）。
    let mut min_i = 0;
    let mut min_v = u64::MAX;
    for i in 0..BILLING_SLOTS {
        let v = BILLING_CPU_NS[i].load(Ordering::Relaxed);
        if v < min_v {
            min_v = v;
            min_i = i;
        }
    }
    BILLING_DOMAINS[min_i].store(domain_id, Ordering::Relaxed);
    BILLING_CPU_NS[min_i].store(ns, Ordering::Relaxed);
}

/// S4-b：查询域账户（`Some(ns)` / None = 无账户）。
pub fn billing_domain_total(domain_id: u64) -> Option<u64> {
    use core::sync::atomic::Ordering;
    for i in 0..BILLING_SLOTS {
        if BILLING_DOMAINS[i].load(Ordering::Relaxed) == domain_id {
            return Some(BILLING_CPU_NS[i].load(Ordering::Relaxed));
        }
    }
    None
}

/// S4-b：按槽索引查询（telemetry 打印面——非零槽遍历）。
pub fn billing_slot_at(i: usize) -> (u64, u64) {
    use core::sync::atomic::Ordering;
    (
        BILLING_DOMAINS[i].load(Ordering::Relaxed),
        BILLING_CPU_NS[i].load(Ordering::Relaxed),
    )
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

// ---- W3-Q6：intent 档位计数/通胀计数（`berry-itally` 单源——档位直方 +
// inflate 数学的语义面在 crate，此处只装静态表与写点；advisor.py F6 摘要
// 行族的消费面 = main.rs tele_dump 的 `tele: intent_tally` 行）----

/// 档位计数静态表（六槽值直索引；写点 recv 结算 slowpath 单写者，读点
/// tele_dump 锁窗外安全——计数器族先例同款）。
static INTENT_TALLY: berry_itally::IntentTally = berry_itally::IntentTally::NEW;

/// 记一次 recv 结算（Own/Borrowed 皆记档位分布；借入另累通胀——写点在
/// `syscall.rs` 结算 slowpath，`berry_itally::record` 纯投影后打原子）。
pub fn intent_settle_note(declared: u8, effective: u8) {
    INTENT_TALLY.note_settle(declared, effective);
}

/// 档位计数表只读引用（tele_dump 行装配用——`tally_line` 快照在锁窗内
/// 一次取走，非热路径）。
pub fn intent_tally() -> &'static berry_itally::IntentTally {
    &INTENT_TALLY
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

// ---- consent 审批事件词汇预留（`AI同意面设计-v1.0.md` §2 审计层事件词汇
// + §5 先落件一；trace `consent` 类别 = 可观测性 §2 类别表 add-only 扩一，
// 现有七类不动）。**v1 词汇预留无消费者**——纯 u8 常量先行定型（ABI 形状
// day-one 纪律），环与消费面按可观测性 §7 阶段走；事件面 v2 随
// berry-launcher 审批 UI（钩子一 SET_POLICY 委托面）落地，同意面计数组
//（P1-a 四计数同形）届时同刀，不随本次词汇先行。设计事件名小写蛇形
//（request/approve/decline/expire/ran_anyway），按仓内常量惯例（大写蛇形
// + 域前缀，对齐 [`REASON_SIGNALED`]/[`WAKE_SRC_*`]）适配为
// `CONSENT_EV_*`，逐常量 doc 注释映射回设计名；0 留哨兵（0 不进环，同
// [`REASON_SIGNALED`] 起值惯例）。----

/// 审批请求（设计名 `request`，§2 钩子二拦截步）：越档动作进 Pending
///（绝不先跑），审批单达 UI。
#[allow(dead_code)] // v1 词汇预留无消费者——v2 随 berry-launcher 审批 UI 落地
pub const CONSENT_EV_REQUEST: u8 = 1;
/// 审批通过（设计名 `approve`，§2 执行段）：批准后以原参数重放执行。
#[allow(dead_code)] // 同上——v1 词汇预留无消费者
pub const CONSENT_EV_APPROVE: u8 = 2;
/// 审批拒绝（设计名 `decline`，§2 执行段）：不执行 + notice 补达。
#[allow(dead_code)] // 同上——v1 词汇预留无消费者
pub const CONSENT_EV_DECLINE: u8 = 3;
/// 审批过期（设计名 `expire`，§2 审批异步段）：Pending TTL 失效（Veda
/// 180s 只借「有时效」语义，定值随实施裁定）。
#[allow(dead_code)] // 同上——v1 词汇预留无消费者
pub const CONSENT_EV_EXPIRE: u8 = 4;
/// 事后告知（设计名 `ran_anyway`，§2 执行段）：已启动动作被打断后跑完
/// 不假装没发生（RanAnyway 语义）。
#[allow(dead_code)] // 同上——v1 词汇预留无消费者
pub const CONSENT_EV_RAN_ANYWAY: u8 = 5;
