//! Generic virtual timer（CNTV）驱动 — Phase 1 里程碑「干净 timer IRQ」。
//!
//! 用 CNTV（PPI IRQ 27），**非物理定时器**：CNTV 不经 `CNTHCTL_EL2`（EL1 直访），
//! `run`（起 EL1）/`run-el2`（起 EL2 降 EL1）统一工作。频率读 `CNTFRQ_EL0`
//! （QEMU `virt` 62.5MHz，**勿硬编码**）。周期 = `freq / HZ`。
//!
//! **热路径零分配**（设计约束）：[`handle`] 不碰堆、无锁（G0 单核）。
//! **电平触发 + 解蔽序**：arm → `daifclr #2`（在 `rust_main` 收尾）；handler 内
//! 先 [`rearm`]（写 TVAL 去活电平）再 `EOIR`，避免电平仍高时 EOIR 后立即重投。

use crate::{uart_put_dec, uart_puts};

/// tick 频率：1Hz——每秒一 tick，肉眼可验周期。
const HZ: u64 = 1;
/// 打到上限即关 timer 停刷屏（证周期往返即可，避免无限刷）。
const MAX_TICKS: u64 = 5;

static mut PERIOD: u64 = 0;
static mut TICK_COUNT: u64 = 0;

/// 读 `CNTFRQ_EL0` + 武装 CNTV（周期 = `freq/HZ`）。返回频率（供上层打印验）。
///
/// # Safety
/// 写 CNTV 系统寄存器；EL1。
pub unsafe fn init() -> u64 {
    let freq = read_cntfrq();
    PERIOD = if HZ > 0 { freq / HZ } else { freq };
    arm(PERIOD);
    freq
}

/// IRQ handler：重武装（去活电平）+ 打 `tick N`。返回 `false` = 到上限、调方应 [`disable`]。
pub fn handle() -> bool {
    // SAFETY: G0 单核、IRQ 上下文无并发。
    unsafe {
        rearm(PERIOD);
        TICK_COUNT += 1;
        uart_puts(b"tick ");
        uart_put_dec(TICK_COUNT);
        uart_puts(b"\n");
        TICK_COUNT < MAX_TICKS
    }
}

/// 关 CNTV（到上限停刷屏）。
pub fn disable() {
    let ctl: u64 = 0; // bit0(enable) 清零
    unsafe {
        core::arch::asm!("msr cntv_ctl_el0, {0}", in(reg) ctl, options(nostack, preserves_flags));
        core::arch::asm!("isb", options(nostack, preserves_flags));
    }
}

unsafe fn arm(period: u64) {
    core::arch::asm!("msr cntv_tval_el0, {0}", in(reg) period, options(nostack, preserves_flags));
    // CNTV_CTL: bit0=ENABLE、bit1=IMASK（1=屏蔽）。故开+不屏蔽 = 0b01（非 0b11）。
    let ctl: u64 = 0b01;
    core::arch::asm!("msr cntv_ctl_el0, {0}", in(reg) ctl, options(nostack, preserves_flags));
    core::arch::asm!("isb", options(nostack, preserves_flags));
}

unsafe fn rearm(period: u64) {
    core::arch::asm!("msr cntv_tval_el0, {0}", in(reg) period, options(nostack, preserves_flags));
    core::arch::asm!("isb", options(nostack, preserves_flags));
}

fn read_cntfrq() -> u64 {
    let v: u64;
    unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, options(nostack, preserves_flags)) };
    v
}
