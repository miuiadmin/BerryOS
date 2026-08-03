//! bos-kernel —— BerryOS 内核镜像。
//!
//! Phase 1 Step 3：boot → HAL init → mm init（帧分配器）→ ktest（FRAME ok）→ halt。
#![no_std]
#![no_main]

mod ktest;
mod mm;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Step 6 升级为寄存器/PC/ESR/FAR dump + 回溯（G0 判据）。
    bos_hal_aarch64::uart_puts(b"\nPANIC\n");
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

    bos_hal_aarch64::halt();
}
