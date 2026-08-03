//! bos-hal-aarch64 —— ARM64 HAL（arch 层，unsafe 收敛地）。
//!
//! Phase 0：boot 入口（EL2→EL1 降级，见 `boot.rs`）+ PL011 UART 输出 + halt。
//! 仅 `#![no_std]`；`panic_handler` / `no_main` 由最终 bin crate（`内核/`）提供。
#![no_std]

mod boot;
mod mmio;

/// QEMU `virt` PL011 UART0 基址。
pub const UART0_BASE: usize = 0x0900_0000;

/// 早期初始化桩（Phase 0 无需配置 PL011，QEMU 即开即用）。
pub fn init() {}

/// 输出一个字节到 UART0。
#[inline]
pub fn uart_putc(c: u8) {
    // unsafe 收敛在此：MMIO 写。
    unsafe { mmio::write(UART0_BASE, c as u32) };
}

/// 输出一段字节串。
pub fn uart_puts(s: &[u8]) {
    for &b in s {
        uart_putc(b);
    }
}

/// 永久挂起（`wfe` 循环）。
pub fn halt() -> ! {
    loop {
        // unsafe 收敛在此：特权指令。
        unsafe { core::arch::asm!("wfe") };
    }
}
