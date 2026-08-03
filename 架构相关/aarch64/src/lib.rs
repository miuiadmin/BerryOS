//! bos-hal-aarch64 —— ARM64 HAL（arch 层，unsafe 收敛地）。
//!
//! Phase 0：boot 入口（EL2→EL1 降级，见 `boot.rs`）+ PL011 UART 输出 + halt。
//! Phase 1 Step 4：stage-1 页表 + 开 MMU（见 [`mmu`]、[`paging`]）。
//! Phase 1 Step 6：异常向量表 + 同步异常/IRQ 处理（见 [`trap`]）。
//! Phase 1 Step 7：GICv2 中断控制器（见 [`gic`]）。
//! Phase 1 Step 8：CNTV 虚拟定时器（见 [`timer`]）。
//! 仅 `#![no_std]`；`panic_handler` / `no_main` 由最终 bin crate（`内核/`）提供。
#![no_std]

mod boot;
pub mod gic;
mod mmio;
mod mmu;
mod paging;
pub mod timer;
pub mod trap;

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

/// 启动早期初始化：开 MMU（Step 4）→ 装异常向量表（Step 6）→ 初始化 GIC（Step 7）。
/// 后续串入 timer 武装（Step 8）。
///
/// 必须在 `rust_main`、单核、EL1、MMU 关时调用一次。
pub fn init() {
    // SAFETY: 启动单线程早期、EL1、MMU 关；boot.asm 已清 BSS、降 EL。
    unsafe { mmu::init() };
    // SAFETY: EL1、MMU 已开（向量表在身份映射的内核镜像内）；任何 DAIF 解蔽之前。
    unsafe { trap::init() };
    // SAFETY: MMIO（Device 映射区）；EL1、单核、DAIF 仍屏蔽。
    unsafe { gic::init() };
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

/// 以十六进制输出 u64（固定 16 位，无 `0x` 前缀）。供 trap dump / panic 格式化复用。
pub fn uart_put_hex64(v: u64) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        let shift = (15 - i) * 4;
        buf[i] = DIGITS[((v >> shift) & 0xf) as usize];
    }
    uart_puts(&buf);
}

/// 以十进制输出 u64（无符号，无前导空格）。供 tick 计数等复用。
pub fn uart_put_dec(v: u64) {
    if v == 0 {
        uart_putc(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut n = v;
    let mut i = 0usize;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        uart_putc(buf[i]);
    }
}

/// 解蔽 IRQ（`daifclr #2`，清 DAIF.I）。解蔽序的最后一步（VBAR→GIC→timer→此）。
pub fn enable_irq() {
    // SAFETY: 写 DAIF 标志位；EL1。
    unsafe { core::arch::asm!("msr daifclr, #2", options(nostack, preserves_flags)) };
}

/// 屏蔽 IRQ（`daifset #2`，置 DAIF.I）。
#[allow(dead_code)]
pub fn disable_irq() {
    unsafe { core::arch::asm!("msr daifset, #2", options(nostack, preserves_flags)) };
}

/// 永久挂起（`wfi` 循环）。DAIF 屏蔽时无中断唤醒 → 真正停；
/// 解蔽后 IRQ 唤醒 wfi、走完向量再回此循环（idle 主循环）。
pub fn halt() -> ! {
    loop {
        // unsafe 收敛在此：特权指令。
        unsafe { core::arch::asm!("wfi") };
    }
}
