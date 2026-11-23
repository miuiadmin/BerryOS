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
///
/// # 配额语义（2026-11-24 记账修正：垫片不计配额）
///
/// `cap` 只计**交付给驱动的字节**（Σ size），[`Self::used`] 与之同尺
/// （`capacity() - used()` = 配额余量）。按绝对地址对齐产生的垫片走另一
/// 本账——由蓄水池自身物理长 `cap + DMA_ALIGN - 1` 兜底，两门各自独立：
///
/// - 配额门：`charged + size > cap` → `Oom`；
/// - 物理门：`对齐后游标 + size > pool.len()` → `Oom`（垫片累积把物理池
///   吃尽时触发——`used()` 可仍小于 `capacity()`，用 [`Self::pool_used`]
///   分辨是哪一门）。
///
/// 由此得到的可兑现性保证（对任意堆基址残差）：单笔 `cap` 字节**必成**
/// （基址垫片 ≤ 63 恰被物理长吸收）；size 全为 64 倍数时**配额恰可兑现**
/// （零分配间垫片，物理门永不先于配额门——`Oom` ⟺ Σ size > cap）。
/// size 非 64 倍数时，分配间垫片（每笔 ≤ 63 字节）累积可能先于配额把物理
/// 池吃尽——最少两笔即可触发（如 cap=100 两笔 50 在基址残差 1..=14 时第二
/// 笔触物理门）——此时仍返回 `Oom`，值不炸、`used()` 与实际交付一致。
/// 任何路径都不越界写蓄水池、不改已分配指针（bump 单调、池 Vec 定长不
/// 重分配）。
pub struct BumpDmaAlloc {
    /// 蓄水池（host 形态 = 普通堆分配；真机形态 = 一致性映射窗）。物理长
    /// `cap + DMA_ALIGN - 1`：多留对齐垫片——Vec 堆基址对齐只保 1，游标自
    /// 下一 64 边界起，兑现 cacheline 对齐契约且保持零 unsafe 面。物理门
    /// 按本长度执法（垫片的兜底账本）。
    pool: Vec<u8>,
    /// 授额容量（`new` 入参；配额门按此执法——只计交付字节，垫片不计）。
    cap: usize,
    /// bump 物理游标（池内字节偏移，起点 0，含全部垫片；物理门的基点）。
    next: usize,
    /// 配额已计字节（Σ 交付 size，不含垫片——`used()` 的源；单调）。
    charged: usize,
    /// 在册分配数（诊断面：free 销账、alloc 记账）。
    live: usize,
}

/// DMA 一致性对齐（AArch64 cacheline = 64B）。
pub const DMA_ALIGN: usize = 64;

/// 对齐与物理门规划（纯函数——基址残差可穷举测试）：池基址绝对地址
/// `base`、池内物理游标 `cursor`、请求 `size`、物理长 `pool_len`，返回
/// `(对齐后池内起偏移, 结束池内偏移)`。按绝对地址推到下一 64 边界（只前进
/// 不回退——`addr ≥ base` 恒成立，减法安全）；地址空间溢出或物理门不过走
/// `Oom`（值不炸）。调用方先过配额门再调本函数。
fn plan(base: usize, cursor: usize, size: usize, pool_len: usize) -> Result<(usize, usize), HalError> {
    let addr = base.checked_add(cursor).ok_or(HalError::Oom)?;
    let aligned_addr = addr.checked_add(DMA_ALIGN - 1).ok_or(HalError::Oom)? & !(DMA_ALIGN - 1);
    let aligned = aligned_addr - base;
    let end = aligned.checked_add(size).ok_or(HalError::Oom)?;
    if end > pool_len {
        return Err(HalError::Oom); // 蓄水池兜底：垫片把物理池吃尽
    }
    Ok((aligned, end))
}

impl BumpDmaAlloc {
    /// 预置 `cap` 字节蓄水池（零容量拒——`Invalid`：无池即全 Oom 的哑池
    /// 无诊断价值，不如建池时就拒绝）。
    pub fn new(cap: usize) -> Result<Self, HalError> {
        if cap == 0 {
            return Err(HalError::Invalid);
        }
        // 零初始化：DMA 缓冲首读前本就该由驱动显式填装（零值面可预测）。
        // 物理长 = cap + 63：垫片预算——基址垫片最多 63 字节，恰被吸收，
        // 故单笔 cap 字节必成（配额只计交付字节，见类型文档）。
        Ok(Self {
            pool: alloc::vec![0u8; cap + DMA_ALIGN - 1],
            cap,
            next: 0,
            charged: 0,
            live: 0,
        })
    }

    /// 授额容量（字节——`new` 的入参，只计交付字节，垫片不计）。
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// 配额已计字节（Σ 交付 size，不含垫片，单调——`free` 不退）。与
    /// [`Self::capacity`] 同尺：`capacity() - used()` = 配额余量。
    pub fn used(&self) -> usize {
        self.charged
    }

    /// 蓄水池物理已用（bump 游标现值，含基址垫片与分配间垫片，单调）。
    /// 物理长 = `capacity() + DMA_ALIGN - 1`；`used()` 未满而本值已近物理
    /// 长时，下一笔 `Oom` 来自物理门（垫片吃尽物理池）而非配额门。
    pub fn pool_used(&self) -> usize {
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
        // 配额门（先于物理门；失败路径零记账——两门都不动游标）。
        let charged = match self.charged.checked_add(size) {
            Some(v) if v <= self.cap => v,
            _ => return Err(HalError::Oom),
        };
        // 物理门 + 对齐规划（纯函数 `plan`：按绝对地址推到下一 64 边界，
        // 垫片由蓄水池物理长兜底）。
        let (aligned, end) = plan(self.pool.as_ptr() as usize, self.next, size, self.pool.len())?;
        // 双界安全切片取指针（aligned < end ≤ pool.len()，slice 索引自带界检；
        // 已分配指针落定后池 Vec 定长不重分配，值不变）。
        let ptr = self.pool[aligned..end].as_mut_ptr();
        self.next = end;
        self.charged = charged;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 纯规划面的序列镜像（两门序同 `alloc_coherent`：配额门先、`plan`
    /// 物理门后；失败不记账）。只用于穷举基址残差——端到端面见
    /// tests/traits.rs。
    fn simulate(
        base: usize,
        cap: usize,
        sizes: &[usize],
    ) -> (Vec<Result<usize, HalError>>, usize, usize) {
        let pool_len = cap + DMA_ALIGN - 1;
        let (mut next, mut charged) = (0usize, 0usize);
        let out = sizes
            .iter()
            .map(|&s| {
                let c = charged + s;
                if c > cap {
                    return Err(HalError::Oom);
                }
                match plan(base, next, s, pool_len) {
                    Ok((_, end)) => {
                        next = end;
                        charged = c;
                        Ok(end)
                    }
                    Err(e) => Err(e),
                }
            })
            .collect();
        (out, charged, next)
    }

    /// 全部 64 种基址残差的代表基址（高址避开溢出面，残差 = 下标）。
    fn bases() -> Vec<usize> {
        (0..DMA_ALIGN).map(|r| 0x10_0000_0000 + r).collect()
    }

    #[test]
    fn plan_aligns_every_base_residue_and_never_regresses() {
        for base in bases() {
            for cursor in 0..=4 * DMA_ALIGN {
                let (aligned, end) = plan(base, cursor, 1, usize::MAX).unwrap();
                assert_eq!((base + aligned) % DMA_ALIGN, 0, "base%64={} cursor={cursor}", base % 64);
                assert!(cursor <= aligned && aligned - cursor < DMA_ALIGN);
                assert_eq!(end, aligned + 1);
            }
        }
    }

    #[test]
    fn plan_address_space_overflow_is_oom() {
        assert_eq!(plan(usize::MAX, 1, 1, usize::MAX).err(), Some(HalError::Oom)); // base+cursor 溢出
        assert_eq!(plan(usize::MAX - 10, 0, 1, usize::MAX).err(), Some(HalError::Oom)); // +63 溢出
        assert_eq!(plan(0x1001, 0, usize::MAX, usize::MAX).err(), Some(HalError::Oom)); // aligned(63)+size 溢出
    }

    #[test]
    fn single_alloc_of_cap_fits_physical_pool_for_every_base_residue() {
        // 单笔 cap 必成：基址垫片 ≤ 63 恰被物理长 cap + 63 吸收。
        for cap in [1usize, 7, 100, 4096, 1 << 20] {
            for base in bases() {
                let p0 = (DMA_ALIGN - base % DMA_ALIGN) % DMA_ALIGN;
                let (out, charged, next) = simulate(base, cap, &[cap]);
                assert_eq!(out, vec![Ok(cap + p0)], "cap={cap} base%64={}", base % 64);
                assert_eq!((charged, next), (cap, cap + p0));
            }
        }
    }

    #[test]
    fn two_allocs_of_half_cap_hit_two_gates_by_base_residue() {
        // 回归钉板（基线缺陷形）：cap=100 两笔 50——基线按含垫片相对偏移
        // 执法时第二笔恒 Oom（0..50 → 对齐到 64 → 64 + 50 = 114 > 100），
        // 且 used()=50 < cap 自相矛盾。垫片不计配额后：残差 r=0 或 r≥15
        // （基址垫片 p0 ≤ 49）两笔必成、第三笔由**配额门**拒；残差 r∈1..=14
        // （p0 ≥ 50）第二笔由**物理门**拒（p0 + 64 + 50 > 163）——两门都
        // 值不炸、不越界，charged 与交付一致。
        for base in bases() {
            let r = base % DMA_ALIGN;
            let (out, charged, next) = simulate(base, 100, &[50, 50, 50]);
            if r == 0 || r >= 15 {
                assert!(out[0].is_ok() && out[1].is_ok(), "r={r}");
                assert_eq!(out[2], Err(HalError::Oom));
                assert_eq!(charged, 100);
            } else {
                assert!(out[0].is_ok(), "r={r}");
                assert_eq!(out[1], Err(HalError::Oom), "r={r}");
                assert_eq!(charged, 50);
            }
            assert!(next <= 100 + DMA_ALIGN - 1);
        }
    }

    #[test]
    fn cacheline_multiple_sizes_redeem_quota_exactly_for_every_base_residue() {
        // size 全为 64 倍数：零分配间垫片，物理门永不先于配额门——64×64
        // 恰兑满 cap=4096，第 65 笔由配额门拒（基线在残差 16 时第 64 笔即
        // Oom——实抓于宿主首实例）。
        for base in bases() {
            let sizes = vec![64usize; 65];
            let (out, charged, next) = simulate(base, 4096, &sizes);
            assert!(out[..64].iter().all(Result::is_ok), "base%64={}", base % 64);
            assert_eq!(out[64], Err(HalError::Oom));
            assert_eq!((charged, next), (4096, 4096 + (64 - base % 64) % 64));
        }
    }

    #[test]
    fn sub_cacheline_sizes_may_hit_physical_gate_before_quota() {
        // 1 字节 × N：每笔 63 字节分配间垫片累积——物理门先于配额门。
        // 第 65 笔仅当基址残差为 1（基址垫片 63）时触物理门，否则成；
        // 第 66 笔任何残差都触物理门；配额 charged 远小于 cap。
        for base in bases() {
            let sizes = vec![1usize; 66];
            let (out, charged, next) = simulate(base, 4096, &sizes);
            assert!(out[..64].iter().all(Result::is_ok), "base%64={}", base % 64);
            assert_eq!(out[64].is_ok(), base % 64 != 1, "base%64={}", base % 64);
            assert_eq!(out[65], Err(HalError::Oom));
            let n = if base % 64 != 1 { 65 } else { 64 };
            assert_eq!(charged, n);
            assert!(next <= 4096 + 63);
        }
    }

    #[test]
    fn failed_alloc_mutates_no_state_on_either_gate() {
        let mut d = BumpDmaAlloc::new(128).unwrap();
        let p = d.alloc_coherent(64).unwrap();
        let snap = (d.used(), d.pool_used(), d.live_count());
        // 配额门拒（65 > 128 - 64）：零状态变化。
        assert_eq!(d.alloc_coherent(65), Err(HalError::Oom));
        assert_eq!((d.used(), d.pool_used(), d.live_count()), snap);
        // Invalid 同理。
        assert_eq!(d.alloc_coherent(0), Err(HalError::Invalid));
        assert_eq!((d.used(), d.pool_used(), d.live_count()), snap);
        // 成功一笔后：used 恰加 size、pool_used ≥ used、live +1。
        let q = d.alloc_coherent(64).unwrap();
        assert_eq!(d.used(), 128);
        assert!(d.pool_used() >= d.used() && d.pool_used() <= 128 + DMA_ALIGN - 1);
        assert_eq!(d.live_count(), 2);
        assert!(q as usize > p as usize && q as usize % DMA_ALIGN == 0);
        // 配额满后再拒。
        assert_eq!(d.alloc_coherent(1), Err(HalError::Oom));
        assert_eq!((d.used(), d.pool_used(), d.live_count()), (128, d.pool_used(), 2));
    }

    #[test]
    fn pool_tail_untouched_and_prior_blocks_stable() {
        // 不越界：垫片区与游标之后的池尾保持零；已分配块内容不被后续分配
        // 改写（指针稳定 + 不重叠）。
        let cap = 512;
        let mut d = BumpDmaAlloc::new(cap).unwrap();
        let base = d.pool.as_ptr() as usize;
        let mut blocks: Vec<(usize, *mut u8)> = Vec::new();
        for (i, s) in [1usize, 7, 64, 100, 33, 65, 200].iter().enumerate() {
            match d.alloc_coherent(*s) {
                Ok(p) => {
                    assert_eq!(p as usize % DMA_ALIGN, 0);
                    // 写入模式标记块。
                    unsafe { core::ptr::write_bytes(p, 0xa0 + i as u8, *s) };
                    blocks.push((*s, p));
                }
                Err(HalError::Oom) => break, // 配额或物理门拒——都合法
                Err(e) => panic!("{e:?}"),
            }
        }
        assert!(blocks.len() >= 6, "sequence should fit at least six blocks: got {}", blocks.len());
        // 块两两不重叠且顺序单调。
        for w in blocks.windows(2) {
            let (s0, p0) = w[0];
            let (_, p1) = w[1];
            assert!((p0 as usize + s0) <= p1 as usize);
        }
        // 每块模式仍在（后续分配未改写）。
        for (i, (s, p)) in blocks.iter().enumerate() {
            let buf = unsafe { core::slice::from_raw_parts(*p, *s) };
            assert!(buf.iter().all(|&b| b == 0xa0 + i as u8), "block {i} clobbered");
        }
        // 游标之后池尾全零；垫片区（块间空隙）全零。
        let next = d.pool_used();
        assert!(d.pool[next..].iter().all(|&b| b == 0));
        let mut cursor = 0usize;
        for (s, p) in &blocks {
            let off = *p as usize - base;
            assert!(d.pool[cursor..off].iter().all(|&b| b == 0), "gap before block not zero");
            cursor = off + s;
        }
        assert!(d.pool[cursor..next].iter().all(|&b| b == 0));
        // 所有块都在物理池内。
        for (s, p) in &blocks {
            let off = *p as usize - base;
            assert!(off + s <= d.pool.len());
        }
    }
}
