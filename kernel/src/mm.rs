//! 内核内存管理初始化：帧分配器的静态后备 + 一次初始化的全局实例。
//!
//! G0（单核、无抢占）下用 `static mut` + 一次 init 即可，无并发；SMP（G2）后需替换为
//! 合适的并发原语（架构综合纲领 §3 点名「全局单例」为多核隐患——此处是受控的唯一例外）。
//!
//! 后备数组放 BSS（boot.asm 已清零）。`FrameAllocator` 借 `'static` 后备。
//!
//! V-1b 起构造口径 = **全窗** `[RAM_BASE, RAM_TOP)` + 显式 [`reserve`]：
//! 1. **无条件先保内核自身足迹** `[RAM_BASE, kernel_end)`（两条路同源）——
//!    `__kernel_end` 是内核真相：含 256KiB 主栈（链接脚本用纯 `. += 0x40000`
//!    保留、**不进任何 PT_LOAD**——装载器与固件图都看不见它；2026-10 EFI
//!    实测错位：__kernel_end=168 页 vs 固件 md0 只认领 PT_LOAD 窗 104 页，
//!    错位 64 页被固件图标 Conventional，不先保则 buddy 把主栈页发给新任务
//!    当内核栈 = 主任务栈被踩、ret 到 0 崩——V-1b 首版实踩）。
//! 2. EFI 路（handoff 携 memmap）：整图形状校验（`map_ok`）→ 两遍走图——
//!    pass 1 全描述符校验（任一 Bad → **整图拒绝降级**，绝不走半张图）；
//!    pass 2 非 Conventional 段逐段 reserve。打 `MMAP ok segs=<n> pages=<N>`
//!    （EFI 路专属标记；QEMU 路零新增输出）。md0 与内核足迹交叠保留幂等。
//! 3. QEMU `-kernel` 路（无 handoff）/坏图降级路：只有第 1 步。
//!
//! 分配序口径（诚实声明）：全窗 + reserve 前缀与旧「base=kernel_end」构造
//! **不保分配序**——旧构造小碎片在窗尾（首分配 ≈ RAM 顶），新构造在保留
//! 上界（首分配 = kernel_end 页）。两侧同为合法 buddy 序、可分配总量守恒
//! （buddy 测试 `reserve_prefix_*` 钉板）；smoke 零漂移不依赖分配序（标记
//! 全是逻辑断言，PA 不进输出/断言）。
//!
//! `static_mut_refs` 在 G0 下是良定义的（唯一可变访问、启动单线程）；SMP 后整体重构。
#![allow(static_mut_refs)]

use berry_hal_aarch64::{RAM_BASE, RAM_TOP};
use berry_mm::frame::FrameAllocator;
use berry_mm::va::PAGE_SIZE;

/// buddy 后备定容：按整段 RAM 计（RAM 几何来自 HAL 平台常量）。
const MAX_PAGES: usize = (RAM_TOP - RAM_BASE) / PAGE_SIZE; // 32768

/// buddy 后备：每页一个 bool（是否空闲块头）。
static mut FRAME_FREE: [bool; MAX_PAGES] = [false; MAX_PAGES];
/// buddy 后备：每页一个 u8（块阶 / 尾页哨兵）。
static mut FRAME_ORDER: [u8; MAX_PAGES] = [0; MAX_PAGES];

/// 全局帧分配器（`init()` 后可用）。
static mut FRAMES: Option<FrameAllocator<'static>> = None;

/// 用链接脚本导出的 `__kernel_end`（高半 VMA）减 `KERNEL_VA_BASE` 得 PA 侧下界。
/// （不直接导入 `__kernel_end_pa`：高半代码 ADRP 引用低半绝对符号会越界，
/// 高半符号 + 运行时减法才是同半寻址。）
///
/// `handoff` = `handoff::probe` 命中的交接结构（EFI 路）；`None` = QEMU
/// `-kernel` 直加载路。EFI 路且图过校验 → 按 EFI 内存图 reserve；否则
/// （QEMU 路 / 无图 / 坏图）降级 reserve `[RAM_BASE, kernel_end)`。
///
/// # Safety
/// 仅在启动单线程、`rust_main` 入口调用一次；之后只经 [`frames`] 访问。
pub unsafe fn init(handoff: Option<&'static crate::handoff::BerryHandoff>) {
    extern "C" {
        static __kernel_end: u8;
    }
    let kernel_end =
        core::ptr::addr_of!(__kernel_end) as usize - berry_hal_aarch64::KERNEL_VA_BASE;
    // 全窗构造（V-1b）。
    // SAFETY: 单核启动期，无并发；后备数组生命周期 'static。
    let mut fa = FrameAllocator::new(RAM_BASE, RAM_TOP, &mut FRAME_FREE, &mut FRAME_ORDER);

    // 内核自身足迹无条件先保（**两条路同源，第一步**）：__kernel_end 含
    // ELF 外足迹（256KiB 主栈——纯 `. += 0x40000` 保留、不进 PT_LOAD，
    // 固件图看不见 → md0 只认 PT_LOAD 窗，错位段在图里是 Conventional）。
    // 先保内核再走图：图是外围真相，kernel_end 是内核真相，并集才完整。
    fa.reserve(RAM_BASE, kernel_end - RAM_BASE);

    // EFI 路：两遍走图。两遍纪律——pass 1 全描述符校验通过之前**不得 reserve
    // 任何一段**（中途降级会留下已发出去的 firmware 页 = 腐化源）。
    let mut efi_map = false;
    if let Some(h) = handoff {
        // 几何哨兵：handoff 窗与 HAL 常量不符 = 配置错位（改 -m 必须同步 HAL），
        // boot 期 fail-loud 优于带错几何继续。
        assert_eq!(h.dram_base, RAM_BASE as u64, "handoff dram_base != HAL RAM_BASE");
        assert_eq!(h.dram_top, RAM_TOP as u64, "handoff dram_top != HAL RAM_TOP");
        let pa = h.memmap_pa;
        let size = h.memmap_size;
        let dsize = h.memmap_desc_size;
        let ver = h.memmap_desc_version;
        if let Some(n) = berry_mm::memmap::map_ok(pa, size, dsize, ver, RAM_BASE as u64, RAM_TOP as u64) {
            // pass 1：全量决策预演——任一 Bad 即弃图降级（reserve 零副作用）。
            let mut bad = false;
            for i in 0..n {
                // SAFETY: map_ok 已过（窗内可直读）；boot 单线程只读。
                let d = unsafe { berry_mm::memmap::desc_at(pa, dsize, i) };
                if matches!(
                    berry_mm::memmap::desc_range(d, RAM_BASE as u64, RAM_TOP as u64),
                    berry_mm::memmap::DescDecision::Bad
                ) {
                    bad = true;
                    break;
                }
            }
            if !bad {
                // pass 2：非 Conventional 段逐段 reserve，累计保留页数。
                let mut reserved: usize = 0;
                for i in 0..n {
                    // SAFETY: 同 pass 1。
                    let d = unsafe { berry_mm::memmap::desc_at(pa, dsize, i) };
                    if let berry_mm::memmap::DescDecision::Reserve(lo, hi) =
                        berry_mm::memmap::desc_range(d, RAM_BASE as u64, RAM_TOP as u64)
                    {
                        reserved += fa.reserve(lo as usize, (hi - lo) as usize);
                    }
                }
                efi_map = true;
                berry_hal_aarch64::uart_puts(b"MMAP ok segs=");
                berry_hal_aarch64::uart_put_dec(n as u64);
                berry_hal_aarch64::uart_puts(b" pages=");
                berry_hal_aarch64::uart_put_dec(reserved as u64);
                berry_hal_aarch64::uart_puts(b"\n");
            }
        }
    }
    if !efi_map {
        // QEMU 路 / 坏图降级：内核足迹已在上面无条件保过（两路共用的第一步）。
        // EFI 路图被拒（形状不过 / 描述符 Bad）须 loud——降级窗不保护 firmware
        // 页，静默降级 = 日后腐化无对账线索。
        if handoff.is_some() {
            berry_hal_aarch64::uart_puts(b"MMAP degrade\n");
        }
    }
    // SAFETY: 单核启动期；此后只经 frames() 访问。
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
