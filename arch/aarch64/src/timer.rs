//! CNTV 虚拟定时器 — tickless（NO_HZ）驱动（plan Phase 2a Step 4 重写）。
//!
//! 用 CNTV（PPI IRQ 27），**非物理定时器**：CNTV 不经 `CNTHCTL_EL2`（EL1 直访），
//! `run`/`run-el2` 统一工作。频率读 `CNTFRQ_EL0`（QEMU `virt` 62.5MHz，**不硬编码**）。
//!
//! **Tickless（day-one）**：空闲（runqueue 空）时 [`mask`] 屏蔽 + `wfi`——空闲核不收
//! 周期中断（续航第一杠杆，非后期补丁）；有 runnable 任务时 [`arm_slice`] 武装一个
//! 时间片（Step 6 驱动抢占）。替代 Phase 1 的 1Hz 自停 demo。
//!
//! **电平触发 + rearm-before-EOIR**：IRQ handler 须在 EOIR 前 [`arm_slice`]（重写
//! 未来时刻的 CVAL 去活电平，语义不变），否则 EOIR 后电平仍高立即重投。
//! `exception_irq`（Step 5）保此序。
//!
//! **绝对截止时间重编程（纪律，非可选 · `调度器设计.md` §4.1）**：定时器永远编程到
//! 绝对到期时刻——[`arm_slice`] 以计数器现读（CNTVCT）重算，绝不基于上次编程值外推。
//! v1 无其他定时事件源，min(最早超时事件, 时间片终点) 退化为时间片终点；Phase 4+ 有
//! 超时事件后在授片处取 min。
//!
//! **热路径零分配**：本模块无堆、无锁（G0 单核）。

/// 时间片频率（每秒切片数）。10Hz → 每 100ms 一 tick（slice=6.25M cycles @62.5MHz）。
/// 调小→切片长（切换少、开销低）；调大→更公平但切换多。Step 6 demo 以此驱动抢占。
pub const SLICE_HZ: u64 = 10;

static mut FREQ: u64 = 0;
static mut SLICE: u64 = 0;

/// 读 `CNTFRQ_EL0` + 缓存 FREQ/SLICE。返回频率（供上层打印验）。**不武装**——
/// 武装由 [`arm_slice`] 显式（tickless：空闲不 arm）。
///
/// # Safety
/// 写模块静态 + 系统寄存器；EL1、单核启动期、仅一次。
pub unsafe fn init() -> u64 {
    FREQ = read_cntfrq();
    SLICE = FREQ / SLICE_HZ;
    FREQ
}

/// 武装一个时间片：**绝对编程**（`调度器设计.md` §4.1「绝对截止时间重编程」纪律，
/// 非可选）——读 `CNTVCT` 现值、写 `CNTV_CVAL_EL0 = now + SLICE`（显式 CVAL 绝对
/// 截止是该纪律的落地形态；写未来时刻的 CVAL 同样去活电平触发），绝不基于上次编程
/// 值外推。随后写 CTL = ENABLE + 不屏蔽（0b01）。兼任 IRQ handler 内的 rearm
/// （rearm-before-EOIR）。
/// v1 无其他定时事件源，min(最早超时事件, 时间片终点) 退化为时间片终点；Phase 4+ 有
/// 超时事件后在授片处取 min。
///
/// # Safety
/// 读 CNTVCT + 写 CNTV 系统寄存器；EL1。
pub unsafe fn arm_slice() {
    let deadline = cntvct() + SLICE;
    core::arch::asm!(
        "msr cntv_cval_el0, {0}",
        in(reg) deadline,
        options(nostack, preserves_flags)
    );
    // CNTV_CTL: bit0=ENABLE、bit1=IMASK（1=屏蔽）。开+不屏蔽 = 0b01（非 0b11）。
    let ctl: u64 = 0b01;
    core::arch::asm!(
        "msr cntv_ctl_el0, {0}",
        in(reg) ctl,
        options(nostack, preserves_flags)
    );
    core::arch::asm!("isb", options(nostack, preserves_flags));
}

/// 屏蔽 CNTV 中断（IMASK=1，ENABLE 保持）——tickless idle 用。倒计仍在跑，但中断不投。
///
/// # Safety
/// 写 CNTV_CTL；EL1。
pub unsafe fn mask() {
    let ctl: u64 = 0b11; // ENABLE=1, IMASK=1
    core::arch::asm!(
        "msr cntv_ctl_el0, {0}",
        in(reg) ctl,
        options(nostack, preserves_flags)
    );
    core::arch::asm!("isb", options(nostack, preserves_flags));
}

/// 关 CNTV（ENABLE=0）——彻底停（区别于 [`mask`] 的仅屏蔽）。
///
/// # Safety
/// 写 CNTV_CTL；EL1。
#[allow(dead_code)]
pub unsafe fn disable() {
    let ctl: u64 = 0b00;
    core::arch::asm!(
        "msr cntv_ctl_el0, {0}",
        in(reg) ctl,
        options(nostack, preserves_flags)
    );
    core::arch::asm!("isb", options(nostack, preserves_flags));
}

/// IRQ handler 钩子（`exception_irq` 在 EOIR 前调）：rearm 时间片（去活电平）。
/// Step 4 不触发（IRQ 未解蔽）；Step 5 起规律调用。
pub fn handle() {
    // SAFETY: IRQ 上下文、单核无并发。
    unsafe { arm_slice() }
}

/// 读虚拟计数器 `CNTVCT_EL0`（周期数）。调度器时间片按**周期数记账**（2b-4 起，
/// 路线图 Phase 2b：LiteOS-M `OsSchedTimeConvertFreq` 式——DVFS 改频后按新频率
/// 重新换算，时间片实际时长不随频率漂移）；`clock_read`（3-1）由此换算纳秒。
pub fn cntvct() -> u64 {
    let v: u64;
    // SAFETY: 只读计数器。
    unsafe { core::arch::asm!("mrs {0}, cntvct_el0", out(reg) v, options(nostack, preserves_flags)) };
    v
}

/// 读计数器频率 `CNTFRQ_EL0`（Hz；QEMU virt 实测 62.5MHz，**不硬编码**）。
/// 独立于 [`init`](fn.init.html) 的缓存——`clock_read` 在 boot 早期（timer init
/// 前）也可能被调，直接读寄存器无顺序依赖。
pub fn cntfrq() -> u64 {
    let v: u64;
    // SAFETY: 只读计数器频率寄存器。
    unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, options(nostack, preserves_flags)) };
    v
}

/// 一个时间片的周期数（`FREQ / SLICE_HZ`；[`init`](fn.init.html) 后有效）。
/// 时间片授予量（grant）的唯一来源——class 层不再持有 tick 数常量。
pub fn slice_cycles() -> u64 {
    // SAFETY: 单核顺序读；init 先于一切调度路径。
    unsafe { SLICE }
}

fn read_cntfrq() -> u64 {
    let v: u64;
    unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, options(nostack, preserves_flags)) };
    v
}
