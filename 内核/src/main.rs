//! bos-kernel —— BerryOS 内核镜像（Phase 0：boot + 打印 + halt）。
#![no_std]
#![no_main]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    bos_hal_aarch64::uart_puts(b"\nPANIC\n");
    bos_hal_aarch64::halt();
}

/// 内核 Rust 入口（由 HAL boot asm 的 `bl rust_main` 调用）。
/// C-ABI 约定（ADR #18），符号名与 asm 精确匹配。
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    bos_hal_aarch64::init();
    bos_hal_aarch64::uart_puts(b"BerryOS boot\n");
    bos_hal_aarch64::halt();
}
