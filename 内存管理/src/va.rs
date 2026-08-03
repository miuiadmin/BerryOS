//! 内核虚拟地址布局常量 + 物理地址转换的**单一切换点** + 平台 RAM 几何。
//!
//! Phase 1 采用**身份映射**（plan 裁决 #3，ADR #20「只前移当前关卡所需」）：
//! 单页表 `TTBR0_EL1 == TTBR1_EL1 == &boot_pt`，`VA == PA`。
//! G0 无需高半内核；架构无关代码**绝不** baked `VA == PA`，统一走这里的
//! `phys_to_virt()`——Phase 2 切高半布局时只改这一个函数 + 重链即可。
//!
//! > RAM_BASE/RAM_TOP 是 QEMU `virt` 的平台常量（Phase 7 跨平台时挪到 HAL/arch）。

/// 基础页大小（字节）。ARM64 stage-1 本阶段仅用 4KiB 页（ADR「无大页」，v2 再加）。
pub const PAGE_SIZE: usize = 4096;
/// log2(PAGE_SIZE)。
pub const PAGE_SHIFT: u32 = 12;

/// 物理内存起点（QEMU `virt` RAM）。
pub const RAM_BASE: usize = 0x4000_0000;
/// 物理内存终点（128MiB）。
pub const RAM_TOP: usize = 0x4800_0000;
/// 最大页数（按整段 RAM 计），用于静态后备数组定容。
pub const MAX_PAGES: usize = (RAM_TOP - RAM_BASE) / PAGE_SIZE; // 32768

/// 物理地址 → 内核虚拟地址。
///
/// 身份映射阶段为恒等；这是唯一交换点（见模块文档）。
#[inline(always)]
pub fn phys_to_virt(pa: usize) -> usize {
    pa
}

/// 内核虚拟地址 → 物理地址（身份时恒等）。
#[inline(always)]
pub fn virt_to_phys(va: usize) -> usize {
    va
}
