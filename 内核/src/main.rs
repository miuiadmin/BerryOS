//! bos-kernel —— BerryOS 内核镜像。
//!
//! Phase 2a Step 5：IRQ 派发钩子接线 + timer 管线。boot 注册 `on_irq_tick`（HAL→sched
//! 函数指针槽反转，避循环依赖）→ 武装 CNTV slice → 解蔽 IRQ。两个**忙循环**任务让 CPU
//! 永忙，timer 每 slice（10Hz）落 `exception_irq`：`ack → rearm → EOIR → on_irq`，串口
//! 规律 `irq tick N`（证 rearm 对、无 EOIR 饿死——异于 Phase 1 的 5 自停）。
//! **本步 `on_irq` 刻意不 switch**：坏则必是 timer/GIC/dispatch，与 switch 无关（Step 6 才接
//! switch）。故 task_a 独占、task_b 饿死（无 `B` 输出 = 无切换的直接证据）。
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

    // Phase 2a Step 5：IRQ 派发钩子 + timer 管线（规律 tick，**不从 IRQ 调 switch**）。
    // SAFETY: EL1、启动单线程、仅一次：缓存 CNTFRQ/算 SLICE（不武装）。
    unsafe { bos_hal_aarch64::timer::init() };
    // IRQ 此刻仍屏蔽（boot DAIF）；spawn 要求屏蔽态（alloc/enqueue 不可打断）。
    bos_sched::init();
    bos_sched::spawn(task_a, 0, b"task_a");
    bos_sched::spawn(task_b, 0, b"task_b");
    // 注册 IRQ 派发（HAL→sched 反转，避循环依赖；须在 enable_irq 前）。Step 5 只打 tick。
    bos_hal_aarch64::set_irq_dispatch(bos_sched::on_irq_tick);
    // 武装首个时间片 + 解蔽 IRQ：之后 timer 每 slice（10Hz）落 exception_irq → on_irq_tick。
    // SAFETY: 写 CNTV_CTL/TVAL；EL1。arm_slice 设 ENABLE+不屏蔽（0b01）。
    unsafe { bos_hal_aarch64::timer::arm_slice() };
    bos_hal_aarch64::enable_irq();
    bos_sched::yield_now(); // idle(main) 让出 → task_a 忙循环永占；timer 打 irq tick N。
    // 不可达（Step 5 task_a 不 yield）；防御兜底：若被切回则进 tickless idle。
    bos_sched::idle_loop();
}

/// 忙循环任务 A（Step 5/6 demo）：纯忙转、不打 yield——证 timer 驱动。
/// Step 5：`on_irq` 不 switch → task_a 独占、task_b 饿死（无 `B` 输出）。
/// Step 6：`on_irq` 接 switch → task_a/task_b 在 slice 下交替。
/// 每 2M 次打一 `A`（atomic 自增防 optimize 掉；节流免刷屏）。
extern "C" fn task_a(_arg: usize) -> ! {
    static A_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    loop {
        let c = A_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if c >= 2_000_000 {
            A_COUNT.store(0, core::sync::atomic::Ordering::Relaxed);
            bos_hal_aarch64::uart_puts(b"A");
        }
    }
}

/// 忙循环任务 B（同 A，打 `B`）。Step 5 饿死（无切换）；Step 6 被调起。
extern "C" fn task_b(_arg: usize) -> ! {
    static B_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    loop {
        let c = B_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if c >= 2_000_000 {
            B_COUNT.store(0, core::sync::atomic::Ordering::Relaxed);
            bos_hal_aarch64::uart_puts(b"B");
        }
    }
}
