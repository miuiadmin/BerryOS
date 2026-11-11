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
//!
//! W3-Q14：EWMA 常量与窗口折叠/翻窗判序/overutil 阈值判定律单源已迁
//! `berry-emface`（host 可测钉板）；本模块保留 per-CPU 原子槽机械，
//! 判定面消费 emface 纯函数（零第二实现）。

use berry_hal_aarch64 as hal;
use berry_emface::{ewma_fold, overutil_judgment, window_state, WinState};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ---- EWMA 常量（单源 = berry-emface；此处 re-export 供 kernel/ktest
// 既有消费面零改动）----

pub use berry_emface::{EWMA_ALPHA_SHIFT, UTIL_NR_CPU, UTIL_WINDOW_HZ, UTIL_WINDOW_VCT_REF};

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
/// 判序/折叠数学单源 `berry_emface`（[`window_state`] 三态 + [`ewma_fold`]）：
/// `util_raw = min(accum × 1024 / elapsed, 1024)`——分母用**实际 elapsed**
/// （自上次翻窗）非名义窗长；`ewma ± diff>>3`（α = 1/8 移位零除法，
/// 差 <8 量化死区）。首调（start==0）开新窗并**丢弃既往 accum**；
/// `now < start`（计数器回绕防御，正常不可达）同路重开。
pub fn util_tick_window(cpu: usize, now: u64) {
    if cpu >= UTIL_NR_CPU {
        return;
    }
    let start = UTIL_WINDOW_START[cpu].load(Ordering::Relaxed);
    // 判序（start/now/窗长三输入即够——accum 只进 Fold 载荷，sched 侧
    // 真值以 swap(0) 原子取走为准，此处传 0 免热路径多一次原子读）。
    match window_state(start, now, util_window_vct(), 0) {
        WinState::NewWindow => {
            UTIL_ACCUM[cpu].store(0, Ordering::Relaxed);
            UTIL_WINDOW_START[cpu].store(now, Ordering::Relaxed);
        }
        WinState::NotDue => {}
        WinState::Fold { elapsed, .. } => {
            // swap(0) 原子取走本窗账并清零（与旧实现逐字同款——判序值
            // 只定资格，真账以 swap 为准）；ewma 折叠数学单源 emface。
            let accum = UTIL_ACCUM[cpu].swap(0, Ordering::Relaxed);
            let ewma = UTIL_EWMA[cpu].load(Ordering::Relaxed);
            let next = ewma_fold(accum, elapsed, ewma);
            UTIL_EWMA[cpu].store(next, Ordering::Relaxed);
            UTIL_WINDOW_START[cpu].store(now, Ordering::Relaxed);
        }
    }
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

/// overutilized 判定阈值（0..=1024 尺度——80%）。单源 = berry-emface。
pub use berry_emface::OVERUTIL_THRESHOLD;

/// 指定核是否 overutilized（EAS 回退判据——任一核超阈值即整个调度域
/// 回退）。判据 = `cpu_util > 域最高容量 × 80%`（严格大于——恰在阈值
/// 上不算，滞回带留 §7 裁决后续接）——比较律单源 `berry_emface::
/// overutil_judgment`（EM 未注册钳 1 保守回退在彼处收口）。
///
/// 越界 cpu → util 0 → 恒 false（同 [`cpu_util`] 口径）。
pub fn overutilized(cpu: usize) -> bool {
    overutil_judgment(cpu_util(cpu), crate::em::em_max_capacity(0))
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
