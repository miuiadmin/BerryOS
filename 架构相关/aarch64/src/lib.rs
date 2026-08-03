//! bos-hal-aarch64 —— ARM64 HAL（arch 层，unsafe 收敛地）。
//!
//! Phase 0：boot 入口（EL2→EL1 降级，见 `boot.rs`）+ PL011 UART 输出 + halt。
//! Phase 1 Step 4：stage-1 页表 + 开 MMU（见 [`mmu`]、[`paging`]）。
//! 仅 `#![no_std]`；`panic_handler` / `no_main` 由最终 bin crate（`内核/`）提供。
#![no_std]

mod boot;
mod mmio;
mod mmu;
mod paging;

// ---- 平台内存图（QEMU `virt`）----
// RAM 几何原在 bos-mm，现挪进 HAL（平台层；bos-mm 保持 arch 无关，由调用方传入）。

/// 物理内存起点（QEMU `virt` RAM）。
pub const RAM_BASE: usize = 0x4000_0000;
/// 物理内存终点（128MiB）。
pub const RAM_TOP: usize = 0x4800_0000;
/// QEMU `virt` PL011 UART0 基址。
pub const UART0_BASE: usize = 0x0900_0000;
/// GICv2 distributor 基址（QEMU `virt`）。
pub const GICD_BASE: usize = 0x0800_0000;
/// GICv2 CPU interface 基址（QEMU `virt`）。
pub const GICC_BASE: usize = 0x0801_0000;

/// 启动早期初始化：开 MMU（Step 4 起）。后续串入 gic/trap/timer（Step 7–8）。
///
/// 必须在 `rust_main`、单核、EL1、MMU 关时调用一次。
pub fn init() {
    // SAFETY: 启动单线程早期、EL1、MMU 关；boot.asm 已清 BSS、降 EL。
    unsafe { mmu::init() };
}

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
