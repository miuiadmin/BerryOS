//! bos-kernel —— BerryOS 内核镜像。
//!
//! Phase 2a Step 4：协作调度 + tickless timer。spawn 两个内核线程 task_a/task_b 在
//! `yield_now` 抢占点上分时（`ABABAB`），退出后进 idle_loop（runqueue 空 → mask CNTV
//! + wfi，tickless 干净停）。IRQ 驱动抢占在 Step 6。
//! 异常向量已装（HAL `trap`）；panic handler 输出消息 + 位置（file:line:col）（G0 判据），
//! CPU 异常寄存器/ESR/FAR dump 在 HAL `trap::exception_sync`。
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

    // Phase 2a Step 4：协作调度 + tickless timer（primitives 已就绪；IRQ 驱动在 Step 5+）。
    // SAFETY: EL1、启动单线程、仅一次：缓存 CNTFRQ/算 SLICE（不武装）。
    unsafe { bos_hal_aarch64::timer::init() };
    // IRQ 此刻仍屏蔽（boot DAIF）；spawn 要求屏蔽态（alloc/enqueue 不可打断）。
    bos_sched::init();
    bos_sched::spawn(task_a, 0, b"task_a");
    bos_sched::spawn(task_b, 0, b"task_b");
    bos_sched::yield_now(); // idle(main) 让出 → task_a，协作分时 ABABAB。
    // demo 退出 → 进 idle_loop（runqueue 空 → mask CNTV + wfi = 干净 tickless halt）。
    bos_sched::idle_loop();
}

/// 协作 demo 任务 A：打 `A` 并 `yield_now` 三次后退出。
extern "C" fn task_a(_arg: usize) -> ! {
    for _ in 0..3 {
        bos_hal_aarch64::uart_puts(b"A");
        bos_sched::yield_now();
    }
    bos_sched::exit_current();
}

/// 协作 demo 任务 B：打 `B` 并 `yield_now` 三次后退出。
extern "C" fn task_b(_arg: usize) -> ! {
    for _ in 0..3 {
        bos_hal_aarch64::uart_puts(b"B");
        bos_sched::yield_now();
    }
    bos_sched::exit_current();
}
