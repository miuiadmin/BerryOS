//! bos-kernel —— BerryOS 内核镜像。
//!
//! Phase 1 Step 8：ktest 后武装 CNTV + 解蔽 IRQ（里程碑「干净 timer IRQ」）。
//! 异常向量已装（HAL `trap`）；panic handler 升级为消息 + 位置（file:line:col）输出
//! （G0 判据），CPU 异常寄存器/ESR/FAR dump 在 HAL `trap::exception_sync`。
#![no_std]
#![no_main]

extern crate alloc;

mod heap;
mod ktest;
mod mm;

use heap::SlabHeap;

/// 全局分配器：内核 slab 堆（小对象走 slab，大对象走整页）。
#[global_allocator]
static ALLOC: SlabHeap = SlabHeap::new();

/// 把 `core::fmt` 格式串直接打到 UART（无缓冲、逐字节）——panic 消息/位置用。
struct UartWriter;

impl core::fmt::Write for UartWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        bos_hal_aarch64::uart_puts(s.as_bytes());
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    // G0 判据：panic 必须有人可读的 dump。消息 + file:line:col。
    // （CPU 异常的寄存器/PC/ESR/FAR dump 另走 HAL `trap::exception_sync`。）
    let _ = writeln!(UartWriter, "\nKERNEL PANIC");
    if let Some(loc) = info.location() {
        let _ = writeln!(UartWriter, "  at {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    // PanicMessage 实现了 Display（1.95 起 message() 不再返回 Option）。
    let _ = writeln!(UartWriter, "  {}", info.message());
    bos_hal_aarch64::halt();
}

/// 内核 Rust 入口（由 HAL boot asm 的 `bl rust_main` 调用）。
/// C-ABI 约定（ADR #18），符号名与 asm 精确匹配。
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    bos_hal_aarch64::init();
    bos_hal_aarch64::uart_puts(b"BerryOS boot\n");

    // SAFETY: 启动单线程、`rust_main` 入口仅调用一次。
    unsafe { mm::init() };

    ktest::run();

    // Step 8 解蔽序最后两步：武装 CNTV timer → daifclr（VBAR→GIC→timer→此）。
    // SAFETY: EL1；GIC 已配 IRQ27；此时 DAIF 仍屏蔽（首 IRQ 在 daifclr 后）。
    let freq = unsafe { bos_hal_aarch64::timer::init() };
    bos_hal_aarch64::uart_puts(b"timer armed freq=");
    bos_hal_aarch64::uart_put_dec(freq);
    bos_hal_aarch64::uart_puts(b"\n");
    bos_hal_aarch64::enable_irq();

    // idle：wfi 睡眠，timer IRQ（1Hz）唤醒打 tick，eret 回此循环。到 5 tick 自停。
    bos_hal_aarch64::halt();
}
