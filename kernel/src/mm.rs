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

use berry_hal_aarch64::{RAM_BASE, RAM_TOP, RAM_TOP_DEFAULT};
use berry_mm::frame::FrameAllocator;
use berry_mm::va::PAGE_SIZE;
use berry_mm::MAX_ORDER;
use core::sync::atomic::{AtomicBool, Ordering};

/// F0-3：第 5 层（`CLASS_ALLOC`，rank 5：位于 `TABLE`/`PT` 之内、`ASID`/`PRINT` 之外）
/// 物理帧与 Slab 小对象统一分配器自旋锁，彻底消除 `HT_DRAINQ` 异步回收与并发系统调用对
/// `FRAMES` / `SlabHeap` 的跨核数据竞争及 `&'static mut` 别名 UB。
static ALLOC_LOCK: AtomicBool = AtomicBool::new(false);

/// `ALLOC_LOCK` RAII 守卫（自带 `DAIF` 保存/恢复与 `lockdep` `CLASS_ALLOC` 记账）。
pub struct AllocGuard {
    saved_daif: u64,
}

#[inline]
pub fn alloc_lock() -> AllocGuard {
    let saved_daif = berry_hal_aarch64::irq_save();
    berry_hal_aarch64::disable_irq();
    #[cfg(feature = "lockdep")]
    berry_hal_aarch64::lockdep::acquire(berry_hal_aarch64::lockdep::CLASS_ALLOC);
    while ALLOC_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        while ALLOC_LOCK.load(Ordering::Relaxed) {
            core::hint::spin_loop();
        }
    }
    AllocGuard { saved_daif }
}

impl Drop for AllocGuard {
    #[inline]
    fn drop(&mut self) {
        #[cfg(feature = "lockdep")]
        berry_hal_aarch64::lockdep::release(berry_hal_aarch64::lockdep::CLASS_ALLOC);
        ALLOC_LOCK.store(false, Ordering::Release);
        berry_hal_aarch64::irq_restore(self.saved_daif);
    }
}

/// buddy 后备定容：按整段 RAM 计（RAM 几何来自 HAL 平台常量）。
const MAX_PAGES: usize = (RAM_TOP - RAM_BASE) / PAGE_SIZE; // 65536（256MiB 上限）

/// buddy 后备：每页一个 bool（是否空闲块头）。
static mut FRAME_FREE: [bool; MAX_PAGES] = [false; MAX_PAGES];
/// buddy 后备：每页一个 u8（块阶 / 尾页哨兵）。
static mut FRAME_ORDER: [u8; MAX_PAGES] = [0; MAX_PAGES];
/// buddy 后备：每阶空闲头位图（K-1 起——`find_free_head` 从全窗线性扫换
/// 位图 + 摘要两级索引；几何/不变量见 `berry_mm::buddy` 模块文档）。
/// 阶数 × 每阶字数：32768 页 → 每阶 512 字 × 14 阶 = 57,344B。
const BMAP_WORDS: usize = (MAX_PAGES + 63) / 64;
static mut FRAME_BMAPS: [u64; (MAX_ORDER + 1) * BMAP_WORDS] = [0; (MAX_ORDER + 1) * BMAP_WORDS];
/// buddy 后备：位图摘要（每阶 ceil(512/64)=8 字 × 14 阶 = 896B）——空阶
/// 查询只付摘要扫（K-1 回归的病态路径正是空阶的全窗页扫：32768×7 指令
/// ×59，见 `docs/基准与压力测试设计-v1.0.md` §13 归因段）。
const SUM_WORDS: usize = (BMAP_WORDS + 63) / 64;
static mut FRAME_SUMS: [u64; (MAX_ORDER + 1) * SUM_WORDS] = [0; (MAX_ORDER + 1) * SUM_WORDS];

/// 全局帧分配器（`init()` 后可用）。
static mut FRAMES: Option<FrameAllocator<'static>> = None;

/// 用链接脚本导出的 `__kernel_end`（高半 VMA）减 `KERNEL_VA_BASE` 得 PA 侧下界。
/// （不直接导入 `__kernel_end_pa`：高半代码 ADRP 引用低半绝对符号会越界，
/// 高半符号 + 运行时减法才是同半寻址。）
///
/// 实际 RAM 顶：BerryBoot 的 `dram_top` 可能仍是 128MiB 模板；用 EFI 图里
/// Conventional 段的最大 end 修正（cap 到 HAL `RAM_TOP` 256MiB）。无图/坏图
/// 时退回 handoff `dram_top`。
///
/// W3-Q35 起折叠律单源在 `berry_mm::ramtop`（host 钉板——checked 双拒/
/// 非 Conventional 不参与/cap+clamp 判序）；本壳只做走图（`desc_at` raw
/// 指针臂留此）+ 纯投影入参装配。
fn efi_ram_top(h: &crate::handoff::BerryHandoff) -> usize {
    let n = berry_mm::memmap::map_ok(
        h.memmap_pa,
        h.memmap_size,
        h.memmap_desc_size,
        h.memmap_desc_version,
        RAM_BASE as u64,
        RAM_TOP as u64,
    );
    // SAFETY: map_ok 已过（窗内可直读）；boot 单线程只读。
    let projs = berry_mm::ramtop::DescProj::project_raw(n, |i| {
        let d = unsafe { berry_mm::memmap::desc_at(h.memmap_pa, h.memmap_desc_size, i) };
        EfiMemdescCopy::copy_of(d)
    });
    berry_mm::ramtop::efi_ram_top(h.dram_top, projs, RAM_BASE as u64, RAM_TOP as u64) as usize
}

/// `EfiMemdesc` 非 Copy（镜像 40B 协议结构刻意裸型）——走图闭包按字段
/// 重构造一份按值返回（`project_raw` 的 `FnMut(usize) -> EfiMemdesc` 契约）。
struct EfiMemdescCopy;
impl EfiMemdescCopy {
    #[inline]
    fn copy_of(d: &berry_mm::memmap::EfiMemdesc) -> berry_mm::memmap::EfiMemdesc {
        berry_mm::memmap::EfiMemdesc {
            kind: d.kind,
            _pad: d._pad,
            physical_start: d.physical_start,
            virtual_start: d.virtual_start,
            number_of_pages: d.number_of_pages,
            attribute: d.attribute,
        }
    }
}

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
    // 全窗构造（V-1b）。位图/摘要后备随 K-1 索引一并借入。
    // SAFETY: 单核启动期，无并发；后备数组生命周期 'static。
    let ram_top = berry_mm::ramtop::frame_window(
        handoff.map(efi_ram_top).map(|t| t as u64),
        RAM_TOP_DEFAULT as u64,
    ) as usize;
    let mut fa = FrameAllocator::new(
        RAM_BASE,
        ram_top,
        &mut FRAME_FREE,
        &mut FRAME_ORDER,
        &mut FRAME_BMAPS,
        &mut FRAME_SUMS,
    );

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
        assert!(
            h.dram_top >= RAM_BASE as u64 && h.dram_top <= RAM_TOP as u64,
            "handoff dram_top outside HAL window"
        );
        let pa = h.memmap_pa;
        let size = h.memmap_size;
        let dsize = h.memmap_desc_size;
        let ver = h.memmap_desc_version;

        if let Some(n) = berry_mm::memmap::map_ok(pa, size, dsize, ver, RAM_BASE as u64, ram_top as u64) {
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
                        berry_mm::memmap::desc_range(d, RAM_BASE as u64, ram_top as u64)
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

/// 在第 5 层 [`ALLOC_LOCK`] 保护下访问全局帧分配器（推荐全运行时路径使用；闭包内严禁触发堆分配或取更外层锁）。
#[inline]
pub fn with_frames<R>(f: impl FnOnce(&mut FrameAllocator<'static>) -> R) -> R {
    let mut guard = alloc_lock();
    f(frames_locked(&mut guard))
}

/// 凭已持有的 [`AllocGuard`] 零重入借用全局帧分配器（供 `SlabHeap` 缺页补仓/大页直退同窗复用）。
#[inline]
pub fn frames_locked<'a>(_guard: &'a mut AllocGuard) -> &'a mut FrameAllocator<'static> {
    // SAFETY: 调用方以 `&mut AllocGuard` 静态证明本核独占持有 ALLOC_LOCK 且已屏蔽 IRQ。
    unsafe {
        (*(&raw mut FRAMES))
            .as_mut()
            .expect("mm::init() 未调用就访问 frames_locked()")
    }
}

/// 线程安全读取当前已分配物理页总数（SMP 遥测与压测对账单口）。
#[inline]
pub fn allocated_pages() -> usize {
    with_frames(|fa| fa.allocated_pages())
}

/// 取全局帧分配器的可变引用（仅供启动单线程期、`ktest` 与 `bench` 直测窗口使用；SMP 运行时一律走 [`with_frames`]）。
pub fn frames() -> &'static mut FrameAllocator<'static> {
    // SAFETY: 启动单线程 / ktest / bench 受控窗口使用。
    unsafe {
        FRAMES
            .as_mut()
            .expect("mm::init() 未调用就访问 frames()")
    }
}
