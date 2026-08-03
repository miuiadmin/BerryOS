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
//! TVAL 去活电平），否则 EOIR 后电平仍高立即重投。`exception_irq`（Step 5）保此序。
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

/// 武装一个时间片：写 TVAL（重置倒计 = 去活电平）+ ENABLE + 不屏蔽（0b01）。
/// 兼任 IRQ handler 内的 rearm（rearm-before-EOIR）。
///
/// # Safety
/// 写 CNTV 系统寄存器；EL1。
pub unsafe fn arm_slice() {
    let slice = SLICE;
    core::arch::asm!(
        "msr cntv_tval_el0, {0}",
        in(reg) slice,
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

fn read_cntfrq() -> u64 {
    let v: u64;
    unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, options(nostack, preserves_flags)) };
    v
}
