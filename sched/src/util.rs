//! Per-CPU utilization EWMA——调度器给 PM 的**频率信号**（v1.3
//! `调点调度-v1.3.md` §6「簇 util 通路」最小件 / §7 预研建议④「频率归
//! PM」：调度器只出 util/placement hint，频率裁决单点归 PM 服务
//! berry-power——调度路径零频率写点，红线成文 §10）。
//!
//! 信号形状 = 「窗口内运行时间 / 窗口时长」的 EWMA（0..=1024，与
//! [`crate::runqueue::Topo::capacity`] / [`crate::em`] 的 capacity 同尺度、
//! 1024 = 满格）。PM 消费面 schedutil 同构：`f ≈ util × fmax / capacity`
//! （[`crate::em::em_max_capacity`] 即 capacity 侧基准）——频率选择权
//! **全归 PM**，本模块只记账 + 读出，零频率决策。
//!
//! 记账面（写者）：[`crate::class::Fair::tick`] 每 tick 把本核 CNTVCT 增量
//! 累进 [`util_accumulate`]（**非 idle 门**：`IDLE_TASK` 上的 tick 不计——
//! idle 时间是低 util 信号本身，不是运行时间）；窗口翻转检查同点
//! [`util_tick_window`]（idle tick 也翻窗——空窗折叠低 util 样本，EWMA 随
//! 空闲衰减）。tickless 深睡期无 tick = 无折叠，醒后首窗 elapsed 摊大、
//! busy 占比被 idle 稀释 → 自然落底（idle 路径无需专门翻窗）。
//!
//! 并发纪律：per-CPU 槽（v1 = 2 核，同 [`crate::runqueue::Topo::capacity`]
//! 定长数组口径），本核 IRQ 上下文单写者 + 跨核读者（PM/遥测）——纯原子
//! 静态（无 `static mut`，不入 `tools/writable_globals.txt` 册）；Relaxed
//! 全序（统计信号无序角色，同 `sched.rs::IRQ_TICKS` 先例）。

use berry_hal_aarch64 as hal;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// EWMA 平滑系数 α = 1/8（`>>`[`EWMA_ALPHA_SHIFT`] 移位实现——no_std 零
/// 浮点零除法；半衰期 ≈ 5.2 窗）。α 越大跟得越紧、噪声越大。
pub const EWMA_ALPHA_SHIFT: u32 = 3;

/// util 采样窗口频率（100Hz → ~10ms）。窗长 = 名义翻窗**下限**（防密集
/// tick——SLEEPQ deadline 醒等早 tick——过折叠）；实际采样跨度见
/// [`util_tick_window`] 的 elapsed 分母注。
pub const UTIL_WINDOW_HZ: u64 = 100;

/// 窗长参考锚（QEMU virt 62.5MHz × 10ms = 625_000 VCT cycles）——仅文档
/// 值，频率**不硬编码**（`timer.rs` 头注纪律）：真窗长运行时按 CNTFRQ
/// 换算（[`util_window_vct`]）。
pub const UTIL_WINDOW_VCT_REF: u64 = 625_000;

/// per-CPU 槽数（v1 = 2 核——同 `Topo::capacity` / `PER_CPU_RQ` 定长口径）。
pub const UTIL_NR_CPU: usize = 2;

/// 当前 EWMA 值（0..=1024，PM 读面）。
static UTIL_EWMA: [AtomicU32; UTIL_NR_CPU] = [const { AtomicU32::new(0) }; UTIL_NR_CPU];
/// 本窗口已累计运行 cycles（Fair::tick 的 CNTVCT 增量累加）。
static UTIL_ACCUM: [AtomicU64; UTIL_NR_CPU] = [const { AtomicU64::new(0) }; UTIL_NR_CPU];
/// 窗口起点（VCT cycles；0 = 未开窗——首调翻窗即置位）。
static UTIL_WINDOW_START: [AtomicU64; UTIL_NR_CPU] = [const { AtomicU64::new(0) }; UTIL_NR_CPU];

/// 窗长（VCT cycles）= `CNTFRQ / UTIL_WINDOW_HZ`（62.5MHz → 625_000 ≈
/// 10ms）。直读寄存器不依赖 `timer::init` 缓存——boot 早期/ktest 均可调
/// （同 `hal::timer::cntfrq` 纪律）。
#[inline]
pub fn util_window_vct() -> u64 {
    hal::timer::cntfrq() / UTIL_WINDOW_HZ
}

/// tick 面调用：累计本核运行时间（每 tick 的 CNTVCT delta cycles）。
/// 越界 cpu 静默拒（IRQ 热路径不 panic）；fetch_add 跨核读者不撕裂。
pub fn util_accumulate(cpu: usize, delta_cycles: u64) {
    if cpu >= UTIL_NR_CPU {
        return;
    }
    UTIL_ACCUM[cpu].fetch_add(delta_cycles, Ordering::Relaxed);
}

/// 窗口到期检查（tick 内调用；设计上 idle 路径亦可调）：到期则折叠一窗
/// EWMA 并翻窗，未到期零动作。
///
/// `util_raw = min(accum × 1024 / elapsed, 1024)`——分母用**实际 elapsed**
/// （自上次翻窗），非名义窗长：tick 间隔 > 窗长（SLICE 100ms > 窗 10ms）
/// 时采样跨度 = 上次翻窗至今，占空比诚实；深睡长 gap 后首窗 elapsed 摊大
/// → idle 自动稀释为低 util 样本。窗长常量只做最短翻窗间隔。
///
/// `ewma = ewma + (util_raw - ewma) >> 3`（α = 1/8 移位零除法；负差分支
/// 减法同式）。差 < 8 时折叠 0（量化死区 ≤ 7/1024 ≈ 0.7%——对频率信号
/// 无害，移位实现的代价）。
///
/// 首调（start==0）开新窗并**丢弃既往 accum**（半窗账无窗长可比——记进
/// 首个整窗会虚高首样）；`now < start`（计数器回绕防御，正常不可达）同
/// 路重开。
pub fn util_tick_window(cpu: usize, now: u64) {
    if cpu >= UTIL_NR_CPU {
        return;
    }
    let start = UTIL_WINDOW_START[cpu].load(Ordering::Relaxed);
    if start == 0 || now < start {
        UTIL_ACCUM[cpu].store(0, Ordering::Relaxed);
        UTIL_WINDOW_START[cpu].store(now, Ordering::Relaxed);
        return;
    }
    let elapsed = now - start;
    if elapsed < util_window_vct() {
        return; // 未到期
    }
    let accum = UTIL_ACCUM[cpu].swap(0, Ordering::Relaxed);
    let util_raw = ((accum as u128 * 1024) / (elapsed as u128)).min(1024) as u32;
    let ewma = UTIL_EWMA[cpu].load(Ordering::Relaxed);
    let next = if util_raw >= ewma {
        ewma + ((util_raw - ewma) >> EWMA_ALPHA_SHIFT)
    } else {
        ewma - ((ewma - util_raw) >> EWMA_ALPHA_SHIFT)
    };
    UTIL_EWMA[cpu].store(next, Ordering::Relaxed);
    UTIL_WINDOW_START[cpu].store(now, Ordering::Relaxed);
}

/// PM 消费面：读当前 EWMA（0..=1024——与 `Topo::capacity` / EM capacity
/// 同尺度可直接除；未启/越界 → 0）。`f ≈ util × fmax / capacity` 的
/// util 侧唯一读出口（PM 环 v1.3 形态留 §11，本面先行）。
pub fn cpu_util(cpu: usize) -> u32 {
    if cpu >= UTIL_NR_CPU {
        return 0;
    }
    UTIL_EWMA[cpu].load(Ordering::Relaxed)
}

// ---- overutilized 判定（v1.3 §7 预研建议⑤「overutilized 回退」）----
//
// Linux EAS 同名机制的 BerryOS 最小面：任一核 util 超域最高容量 80% →
// 整个调度域 EAS 失效退化纯负载均衡（消费点 = `policy::placement_hint`
// 门控）。EAS 永远是机会主义——饱和系统里能量/放置比较失真（所有候选
// 都「贵」），省电让位吞吐。

/// overutilized 判定阈值（0..=1024 尺度——80%）：1024 × 0.8 = 819.2 →
/// 819（整数截断；Linux `sched_capacity_margin` 的 80% 同款口径）。
pub const OVERUTIL_THRESHOLD: u32 = 819;

/// 指定核是否 overutilized（EAS 回退判据——任一核超阈值即整个调度域
/// 回退）。判据 = `cpu_util > 域最高容量 × 80%`（严格大于——恰在阈值
/// 上不算，滞回带留 §7 裁决后续接）。util 与 [`crate::em::
/// em_max_capacity`] 同尺度（1024 = 满格）直接比较。
///
/// 退化面：EM 未注册（capacity 0 → 钳 1 → 阈值 0）时任何非零 util 即
/// over——保守侧正确：无能量模型则 EAS 无据可依，本就该回退普通均衡。
/// 越界 cpu → util 0 → 恒 false（同 [`cpu_util`] 口径）。
pub fn overutilized(cpu: usize) -> bool {
    let util = cpu_util(cpu);
    let cap = crate::em::em_max_capacity(0).max(1); // 域 0 最高容量
    // util 已是 0..=1024 与 cap 同尺度——直接比较
    util > (cap * OVERUTIL_THRESHOLD) / 1024
}

/// 全域 overutilized（任一核超 → true——Linux EAS 全域回退口径：
/// overutilized 是调度域级标志，单核饱和即全域 EAS 失效，恢复须全域
/// 阈下）。[`crate::policy::placement_hint`] 的门控判据。
pub fn any_overutilized() -> bool {
    (0..UTIL_NR_CPU).any(|i| overutilized(i))
}

/// 清零重开窗（ktest/boot 期清场专用——生产路径无重置消费面；单写者
/// 语境调用）。
pub fn util_reset(cpu: usize) {
    if cpu >= UTIL_NR_CPU {
        return;
    }
    UTIL_EWMA[cpu].store(0, Ordering::Relaxed);
    UTIL_ACCUM[cpu].store(0, Ordering::Relaxed);
    UTIL_WINDOW_START[cpu].store(0, Ordering::Relaxed);
}
