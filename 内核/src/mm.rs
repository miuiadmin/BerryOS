//! 内核内存管理初始化：帧分配器的静态后备 + 一次初始化的全局实例。
//!
//! G0（单核、无抢占）下用 `static mut` + 一次 init 即可，无并发；SMP（G2）后需替换为
//! 合适的并发原语（架构综合纲领 §3 点名「全局单例」为多核隐患——此处是受控的唯一例外）。
//!
//! 后备数组放 BSS（boot.asm 已清零）。`FrameAllocator` 借 `'static` 后备。
//!
//! `static_mut_refs` 在 G0 下是良定义的（唯一可变访问、启动单线程）；SMP 后整体重构。
#![allow(static_mut_refs)]

use bos_mm::frame::FrameAllocator;
use bos_mm::va::{MAX_PAGES, RAM_TOP};

/// buddy 后备：每页一个 bool（是否空闲块头）。
static mut FRAME_FREE: [bool; MAX_PAGES] = [false; MAX_PAGES];
/// buddy 后备：每页一个 u8（块阶 / 尾页哨兵）。
static mut FRAME_ORDER: [u8; MAX_PAGES] = [0; MAX_PAGES];

/// 全局帧分配器（`init()` 后可用）。
static mut FRAMES: Option<FrameAllocator<'static>> = None;

/// 用链接脚本导出的 `__kernel_end` 初始化帧分配器，接管 `[__kernel_end, RAM_TOP)`。
///
/// # Safety
/// 仅在启动单线程、`rust_main` 入口调用一次；之后只经 [`frames`] 访问。
pub unsafe fn init() {
    extern "C" {
        static __kernel_end: u8;
    }
    let kernel_end = core::ptr::addr_of!(__kernel_end) as usize;
    // SAFETY: 单核启动期，无并发；后备数组生命周期 'static。
    let fa = FrameAllocator::new(kernel_end, RAM_TOP, &mut FRAME_FREE, &mut FRAME_ORDER);
    // SAFETY: 同上。
    core::ptr::addr_of_mut!(FRAMES).write(Some(fa));
}

/// 取全局帧分配器的可变引用。
///
/// # Safety（调用方视角）
/// G0 单核无抢占下安全。SMP 后需加锁或改每核实例。
pub fn frames() -> &'static mut FrameAllocator<'static> {
    // SAFETY: G0 下唯一可变访问点。
    unsafe {
        FRAMES
            .as_mut()
            .expect("mm::init() 未调用就访问 frames()")
    }
}
