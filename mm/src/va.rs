//! 内核虚拟地址布局常量 + 物理地址转换的**单一切换点**。
//!
//! 2b-1 起高半 direct map（`内存管理设计.md` §5）：`phys_to_virt(pa) = base + pa`，
//! base 由平台在 boot 注入（ARM64 = `KERNEL_VA_BASE = 0xFFFF_FF80_0000_0000`；
//! x86_64 最小集 = 0 → 恒等，行为不变）。架构无关代码**绝不** baked `VA == PA`，
//! 统一走这里——换架构/换布局只改注入值，不改调用方。
//!
//! 平台 RAM 几何（RAM_BASE/RAM_TOP）属 HAL（arch 层），见 `berry_hal_aarch64`；
//! 帧分配器在构造时由调用方传入 `ram_top`。

use core::sync::atomic::{AtomicUsize, Ordering};

/// 基础页大小（字节）。ARM64 stage-1 本阶段仅用 4KiB 页（ADR「无大页」，v2 再加）。
pub const PAGE_SIZE: usize = 4096;
/// log2(PAGE_SIZE)。
pub const PAGE_SHIFT: u32 = 12;

/// 内核 direct map 基址。0 = 恒等（x86_64 最小集 / 2b-1 之前）。
/// boot 期单线程注入一次，此后只读——Relaxed 足够（先于一切消费者执行）。
static KERNEL_VA_BASE: AtomicUsize = AtomicUsize::new(0);

/// 注入内核 direct map 基址。`rust_main` 第一件事、任何 `phys_to_virt` 消费之前，
/// 启动单线程调用一次。
pub fn set_kernel_va_base(base: usize) {
    KERNEL_VA_BASE.store(base, Ordering::Relaxed);
}

/// 物理地址 → 内核虚拟地址（唯一交换点，见模块文档）。
#[inline(always)]
pub fn phys_to_virt(pa: usize) -> usize {
    pa + KERNEL_VA_BASE.load(Ordering::Relaxed)
}

/// 内核虚拟地址 → 物理地址（direct map 区内的地址才合法）。
#[inline(always)]
pub fn virt_to_phys(va: usize) -> usize {
    va - KERNEL_VA_BASE.load(Ordering::Relaxed)
}
