//! BerryOS 内存管理子系统（架构无关核心）。
//!
//! Phase 1 Step 2 落地的是**纯 Rust buddy 页帧分配器核心**——刻意满足：
//! - **不持有全局状态**（架构综合纲领 §3 把「全局单例 + UPSafeCell」列为多核隐患）；
//! - **无 `unsafe`**；
//! - **不依赖 `alloc`**（后备由调用方借入的切片承载）。
//!
//! 因此核心算法可完全在 host 上 `cargo test` 覆盖，内核侧（Step 3）只做静态后备 +
//! 一次初始化的薄包装，无需重写算法。
//!
//! 内核 `#![no_std]`：本 crate 在非 test 构建下也是 `no_std`（见下方 cfg_attr）。

#![cfg_attr(not(test), no_std)]
#![allow(clippy::result_unit_err)]

pub mod asid;
pub mod buddy;
pub mod frame;
pub mod memmap;
pub mod va;

pub use buddy::{AllocError, BuddyAllocator, MAX_ORDER};
pub use frame::FrameAllocator;
