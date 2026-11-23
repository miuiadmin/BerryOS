//! 统一硬件抽象层（HAL）——驱动子系统研究报告 P1 结构性缺口件 + 波 2 架构件。
//!
//! 报告 §4.1 诊断的结构性缺口：各驱动（vprobe/vblk/vnet）各自实现 MMIO
//! volatile 读写、IRQ 注册、DMA 分配——三面胶水无统一抽象，新驱动被迫长
//! 第二套。本 crate 钉四条窄 trait 契约（[`Mmio`]/[`IrqCtl`]/[`DmaAlloc`]/
//! [`Clock`]）+ 统一错误面 [`HalError`]：驱动只写业务逻辑，平台差异沉到
//! trait 实现侧（首个实现 = [`virtio_impl::VirtioMmio`]，berry-virtio 窗）。
//! 对应报告 §4.3 波 2.2「DeviceCap 三铸件」的用户态半边（MMIO 窗 VMO +
//! Interrupt + Bti 占位）——MMIO 面真接线已落 vprobe（2026-11-20）与 vblk
//! （2026-11-23）两件消费者；Interrupt/Bti 面归波 2.1/2.2 后续刀，本刀只
//! 钉契约。
//!
//! 与莓核 Wave 4 刀 4.1 的内核侧 berry-hal（CPU/MMU/IRQ/Timer/SMP 五窄口）
//! 同名异层：那颗服务内核 arch 抽象，本颗服务用户态驱动进程，契约面互不
//! 相交（报告 §6 节奏耦合段两半边分说）。
//!
//! 与 berry-driver 的关系（2026-11-20 债务清理刀定谳：**契约面正交组合**）：
//! 那颗钉**生命周期**契约（probe 认领 → start 服役 → stop 退役），本颗
//! 钉**硬件访问**契约——组合发生在消费侧：驱动实现方同时实现 Driver
//! trait 并持有本 crate 的 trait 对象（driver 的示例模块即此组合形；
//! vprobe、vblk 当前只持 HAL 半边、未实现 Driver trait——故 driver→hal
//! 的 crate 依赖是可选的单向箭头，本 crate 永不反向依赖 driver）。
//! 生命周期契约不被硬件访问契约绑架（反之亦然——两契约可各自独立
//! 测试）。缝合成铸件（DeviceCap：MMIO 窗
//! VMO + Interrupt + Bti）归波 2.2 后续刀。
//!
//! 形态 = berry-driver 族例：`cfg_attr(not(test), no_std)` 库本体 + alloc
//! （[`BumpDmaAlloc`] 蓄水池的 Vec 面）；test 构建放开 std 仅为默认测试
//! 挂具。库本体零 `unsafe` 面（volatile 的 unsafe 收敛在 virtio_impl 实现
//! 内部，契约侧纯安全接口）。

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod virtio_impl;

pub use virtio_impl::VirtioMmio;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// MMIO 寄存器窗读写——`off` 为窗内字节偏移（寄存器偏移表的落点值，如
/// `berry_virtio::mmio::STATUS`）。读越界/未实现寄存器返回 0、写则丢弃
/// （值不炸：真机语义 = 授窗 VMO 界外本就不可达，mock 语义 = 零层面板）。
pub trait Mmio {
    fn read32(&self, off: usize) -> u32;
    fn write32(&mut self, off: usize, v: u32);
}

/// IRQ 注册与使能管理。`handler` 为静态函数指针（上下文归实现侧闭包表/
/// 转发面——trait 面保持对象安全不泛型化）；重复注册同号走
/// [`HalError::IrqTaken`]（单设备单主人，报告 §2.3 认领仲裁同律）。
pub trait IrqCtl {
    fn register(&mut self, irq: u32, handler: fn()) -> Result<(), HalError>;
    fn unregister(&mut self, irq: u32);
    fn enable(&mut self, irq: u32);
    fn disable(&mut self, irq: u32);
}

/// DMA 一致性内存分配——返回指针对 cacheline 对齐（AArch64 = 64B），设备
/// 与 CPU 双侧可见无需显式 cache 维护（`dc cvac/ivac` 面归波 2.4 cache
/// 维护 DMA API 收口刀）。
pub trait DmaAlloc {
    fn alloc_coherent(&mut self, size: usize) -> Result<*mut u8, HalError>;
    fn free_coherent(&mut self, ptr: *mut u8);
}

/// 时钟（延时）——`now_us` 单调微秒读数；`spin_us` 忙等到读数前进 `us`
/// 微秒（mock 实现可虚拟直推零真延时，真机实现读系统计数器）。
pub trait Clock {
    fn now_us(&self) -> u64;
    fn spin_us(&self, us: u64);
}

/// HAL 统一错误面（值不炸——失败走错误值，不 panic）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HalError {
    /// 设备/资源不存在（如 IRQ 号无主、窗后无设备）。
    NoDevice,
    /// IRQ 已被占用（单主人仲裁拒绝）。
    IrqTaken,
    /// DMA 内存耗尽（池尽/无后备 VMO）。
    Oom,
    /// 参数非法（null 窗/零长分配/越界偏移）。
    Invalid,
}

/// 虚拟时钟参考实现——`AtomicU64` 计数器直推（host 测试与无真机时钟面的
/// 通用回退；真机实现 = 系统计数器时钟，如 AArch64 CNTPCT 换算面）。
/// `spin_us` 语义 = 虚拟时间前进 `us` 微秒（零真延时，测试快）。
#[derive(Debug)]
pub struct VirtualClock {
    now: AtomicU64,
}

impl VirtualClock {
    /// 零起点时钟。
    pub const fn new() -> Self {
        Self { now: AtomicU64::new(0) }
    }
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for VirtualClock {
    fn now_us(&self) -> u64 {
        self.now.load(Ordering::Relaxed)
    }

    fn spin_us(&self, us: u64) {
        self.now.fetch_add(us, Ordering::Relaxed);
    }
}

/// bump 形 DMA 蓄水池——[`DmaAlloc`] 的通用回退实现（真机形态 = 授窗 VMO
/// 映射后备，接口同形）。一次性预置池，bump 游标单调分配、cacheline 对齐
/// （64B）；**不回收复用**——`free_coherent` 只销账（池内指针 live 计数
/// 减一），池外指针静默忽略（值不炸）；池尽 [`HalError::Oom`]，真回收面
/// 归波 2.4 连续页 DMA VMO 刀。
pub struct BumpDmaAlloc {
    /// 蓄水池（host 形态 = 普通堆分配；真机形态 = 一致性映射窗。物理长
    /// `cap + DMA_ALIGN - 1`：多留对齐垫片——Vec 堆基址对齐只保 1，游标
    /// 自下一 64 边界起，兑现 cacheline 对齐契约且保持零 unsafe 面）。
    pool: Vec<u8>,
    /// 授额容量（Oom 门按此执法——垫片不计可用）。
    cap: usize,
    /// bump 游标（池内字节偏移，起点 0；分配期按绝对地址对齐消化垫片）。
    next: usize,
    /// 在册分配数（诊断面：free 销账、alloc 记账）。
    live: usize,
}

/// DMA 一致性对齐（AArch64 cacheline = 64B）。
pub const DMA_ALIGN: usize = 64;

impl BumpDmaAlloc {
    /// 预置 `cap` 字节蓄水池（零容量拒——`Invalid`：无池即全 Oom 的哑池
    /// 无诊断价值，不如建池时就拒绝）。
    pub fn new(cap: usize) -> Result<Self, HalError> {
        if cap == 0 {
            return Err(HalError::Invalid);
        }
        // 零初始化：DMA 缓冲首读前本就该由驱动显式填装（零值面可预测）。
        // 物理长 = cap + 63：垫片预算——分配期按绝对地址对齐到 64 边界
        // 最多吃 63 字节，授额 cap 全程可兑现（Oom 门在 slice 索引前执法）。
        Ok(Self { pool: alloc::vec![0u8; cap + DMA_ALIGN - 1], cap, next: 0, live: 0 })
    }

    /// 授额容量（字节——`new` 的入参，垫片不计）。
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// 已分配字节数（bump 游标现值，含基址垫片与分配间对齐垫片——单调）。
    pub fn used(&self) -> usize {
        self.next
    }

    /// 在册分配数。
    pub fn live_count(&self) -> usize {
        self.live
    }

    /// 指针是否落在池内（start..end 半开区间，对池基址判）。
    fn in_pool(&self, ptr: *mut u8) -> bool {
        let base = self.pool.as_ptr() as usize;
        let p = ptr as usize;
        p >= base && p < base + self.pool.len()
    }
}

impl DmaAlloc for BumpDmaAlloc {
    fn alloc_coherent(&mut self, size: usize) -> Result<*mut u8, HalError> {
        if size == 0 {
            return Err(HalError::Invalid);
        }
        // 按绝对地址对齐：Vec<u8> 堆基址对齐只保 1，需把游标推到
        // base + off ≡ 0 (mod 64) 的绝对 64 边界（对齐垫片由此消化，
        // 只前进不回退——addr ≥ base 恒成立，减法安全）。
        let base = self.pool.as_ptr() as usize;
        let addr = match base.checked_add(self.next) {
            Some(v) => v,
            None => return Err(HalError::Oom),
        };
        let aligned_addr = match addr.checked_add(DMA_ALIGN - 1) {
            Some(v) => v & !(DMA_ALIGN - 1),
            None => return Err(HalError::Oom),
        };
        let aligned = aligned_addr - base;
        let end = match aligned.checked_add(size) {
            Some(v) => v,
            None => return Err(HalError::Oom),
        };
        if end > self.cap {
            return Err(HalError::Oom); // 授额耗尽（垫片区不计可用）
        }
        // 双界安全切片取指针（end ≤ cap < 物理长，slice 索引自带界检）。
        let ptr = self.pool[aligned..end].as_mut_ptr();
        self.next = end;
        self.live += 1;
        Ok(ptr)
    }

    fn free_coherent(&mut self, ptr: *mut u8) {
        // 池内销账（内存不收回——bump 单调）；池外静默忽略（值不炸：
        // free 面 void 无错误通道，误释放不炸库）。
        if self.in_pool(ptr) {
            self.live = self.live.saturating_sub(1);
        }
    }
}
