//! 唤醒源语境 cell + HAL 侧 IRQ 到达计数（刀 1：内核 wakeup 记账面；
//! PM-17 终审 §5）。常量同源 [`crate::wake_gen`]（`tools/abi.toml`
//! `[[wakesrc]]` 第五产物，add-only）。
//!
//! 两件东西：
//!
//! 1. **per-CPU 语境 cell**（`AtomicU8`）：当前「唤醒归因语境」——下一次
//!    `note_wake`（→ 内核 `on_wake` per-source 记账）应归的唤醒源。写点三处：
//!    - `berry_sched::note_switch` 顶（任务语境基线 = `WAKE_SRC_IPC`：切入
//!      任务/idle 即设；park 派发 prev=null 不走 note_switch，故
//!      `dispatch_from_park_ctx` 在 context_switch 前显式同写）；
//!    - `exception_irq` 四臂前（save 旧值 → store `src_of(id)` → 分派 →
//!      **单出口**恢复；SPURIOUS 早退在 store 之前，零字节窗口）。IRQ 不
//!      嵌套（handler 全程 PSTATE.I=1）⇒ save/restore 窗同核串行闭合；
//!    - 初值：CPU0 = `KERNEL_BOOT`（rust_main 直跑窗的归因——首任务切入后
//!      冻结），CPU1 = `NONE`（副核 bring-up/park 前无任务语境；NONE 到达
//!      记账 = 绊网 panic，L0 执法）。
//! 2. **HAL 侧 IRQ 到达计数**：SGI×2 与未知臂没有内核回调槽可挂（timer 槽
//!    只对 CNTV、rx 槽只对 PL011），计数留在 HAL、内核 G1.5 快照合并
//!    （`telemetry::wake_src_merge_hal`）——`SGI_SEEN`（trap.rs SGI_RESCHED
//!    臂既有，本模块给读者）、[`TLB_SEEN`]（新增）、[`UNKNOWN_SEEN`（自
//!    trap.rs 未知臂函数内 static 升格）。
//!
//! Relaxed 足够：cell 与计数都只被本核读写（on_wake 在唤醒发生核上跑），
//! 无跨核序承重；跨核读者只看单调计数（快照语义）。

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use crate::wake_gen::{
    WAKE_SRC_IPC, WAKE_SRC_KERNEL_BOOT, WAKE_SRC_NONE, WAKE_SRC_POWER_KEY, WAKE_SRC_SGI_RESCHED,
    WAKE_SRC_SGI_TLB, WAKE_SRC_TIMER, WAKE_SRC_UART_RX, WAKE_SRC_UNKNOWN,
};

// W3-Q66：归因链判定律单源迁 berry-wakeface（号→源映射/保存值域判/槽位
// 钳位——host 钉板在 wakeface；wsrcface 姊妹件：本面钉号怎么归源，wsrcface
// 钉源怎么分类进环）。号表锚以 gic.rs 冻结号参数化注入——HAL 改号时
// wakeface 镜像测试红双执法。原子 cell/到达计数物理臂留壳。
use berry_wakeface::{restore_valid as wf_restore_valid, slot_index as wf_slot_index, IrqMap};

/// per-CPU cell 数（QEMU virt `-smp 2` 钉 2——canary 表同口径）。
const CPUS: usize = 2;

/// per-CPU 唤醒归因语境 cell。初值见模块头；拓扑槽外读收敛尾槽（防御，
/// boot 期 mp 不明窗——`ring_write` 同惯例）。
static CELL: [AtomicU8; CPUS] =
    [AtomicU8::new(WAKE_SRC_KERNEL_BOOT), AtomicU8::new(WAKE_SRC_NONE)];

#[inline]
fn cell() -> &'static AtomicU8 {
    &CELL[crate::smp::cpu_id().min(CPUS - 1)]
}

/// 读当前核的唤醒归因语境（内核 `on_wake` per-source 记账读点）。
pub fn wake_src_current() -> u8 {
    cell().load(Ordering::Relaxed)
}

/// 槽位钳位读（W3-Q66 单源 re-export——kernel G1.5 快照合并/到达记账
/// 读点消费面；wsrcface 姊妹件合钉同律）。
pub fn wake_src_slot_index(src: u8) -> usize {
    wf_slot_index(src)
}

/// 任务语境基线写点（`berry_sched::note_switch` 顶调）：切入任务/idle 语境
/// 即归 IPC。park↔任务 null 对同样先落基线再早退——park 醒来（IRQ 语境外）
/// 的 wake_matching 归任务侧基线；NONE 只属于副核首任务切入前的未接线窗。
pub fn wake_src_task_ctx() {
    cell().store(WAKE_SRC_IPC, Ordering::Relaxed);
}

/// IRQ 入口：存入本臂归因源，返回旧值（供 [`wake_src_irq_exit`] 恢复）。
/// 仅 `exception_irq` 在 SPURIOUS 早退**之后**调用。
pub(crate) fn wake_src_irq_enter(src: u8) -> u8 {
    let c = cell();
    let saved = c.load(Ordering::Relaxed);
    c.store(src, Ordering::Relaxed);
    saved
}

/// IRQ 单出口：恢复 [`wake_src_irq_enter`] 返回的旧值。
///
/// W3-Q66：保存值域判单源 = `berry_wakeface::restore_valid`（九源集——
/// debug 形执法：非法保存值 = cell 被外物写坏，fail-loud；release 形直写
/// 不放大损伤——毒值本就已在 cell 里，恢复不引入第二份）。
pub(crate) fn wake_src_irq_exit(saved: u8) {
    debug_assert!(
        wf_restore_valid(saved),
        "WAKE: 恢复值 {saved:#04x} 不在已定义源集（cell 被外物写坏或保存寄存器被踩）"
    );
    cell().store(saved, Ordering::Relaxed);
}

/// GIC INTID → 唤醒源。未知号一律 `UNKNOWN`（只进到达计数、永不进环）。
///
/// W3-Q66：判序单源 = `berry_wakeface::IrqMap::src_of`（host 钉板在
/// wakeface——板上六活锚/SGI 段正交/SPURIOUS 不进号表钉板）；本函数
/// 薄壳传真常量表。SPURIOUS(1023) 在 `exception_irq` enter 之前早退，
/// 不进号表（语义上也是未知臂）。
pub(crate) fn src_of(id: u32) -> u8 {
    static BOARD: IrqMap = IrqMap {
        cntv: crate::gic::IRQ_CNTV as u32,
        pl011_rx: crate::gic::IRQ_PL011_RX as u32,
        pl1_rx: crate::gic::IRQ_PL1_RX as u32,
        pl061: crate::gic::IRQ_PL061 as u32,
        sgi_resched: crate::gic::SGI_RESCHEDULE as u32,
        sgi_tlb: crate::gic::SGI_TLB_SHOOTDOWN as u32,
        src_timer: WAKE_SRC_TIMER,
        src_uart_rx: WAKE_SRC_UART_RX,
        src_power_key: WAKE_SRC_POWER_KEY,
        src_sgi_resched: WAKE_SRC_SGI_RESCHED,
        src_sgi_tlb: WAKE_SRC_SGI_TLB,
        src_unknown: WAKE_SRC_UNKNOWN,
    };
    BOARD.src_of(id)
}

/// SGI_TLB_SHOOTDOWN 到达计数（无唤醒路径，只计到达；G1.5 快照合并消费）。
pub(crate) static TLB_SEEN: AtomicU32 = AtomicU32::new(0);

/// 未知 IRQ 号到达计数（loud 打印止于首 4 次，计数永续；只进 IRQ 级计数、
/// 永不进环）。
pub(crate) static UNKNOWN_SEEN: AtomicU32 = AtomicU32::new(0);

/// SGI_RESCHED 到达数（全核到达口径；源 = trap.rs 既有 `SGI_SEEN` 臂计数）。
pub fn sgi_resched_arrivals() -> u32 {
    crate::trap::SGI_SEEN.load(Ordering::Relaxed)
}

/// SGI_TLB 到达数（[`TLB_SEEN`] 读点）。
pub fn sgi_tlb_arrivals() -> u32 {
    TLB_SEEN.load(Ordering::Relaxed)
}

/// 未知号到达数（[`UNKNOWN_SEEN`] 读点）。
pub fn unknown_arrivals() -> u32 {
    UNKNOWN_SEEN.load(Ordering::Relaxed)
}
