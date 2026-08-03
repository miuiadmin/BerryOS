//! bos-kernel —— BerryOS 内核镜像。
//!
//! Phase 1 Step 5：boot → HAL init（MMU）→ mm init（帧分配器）→ ktest
//! （FRAME ok / HEAP ok，slab 堆使能 `alloc`）→ halt。
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
