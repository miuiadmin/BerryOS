//! berry-utrace —— util EWMA 窗口机盒纯面（W3-Q23）。
//!
//! **动机**：`sched/src/util.rs` 的 per-CPU 原子槽机械（`UTIL_EWMA`/
//! `UTIL_ACCUM`/`UTIL_WINDOW_START` 三数组 + accumulate/tick_window/
//! read/reset 四操作）此前只活在内联原子代码里——W3-Q14 已把判序
//! （[`berry_emface::window_state`]）与折叠数学
//! （[`berry_emface::ewma_fold`]）单源外迁 berry-emface，但「槽状态机
//! 怎么推进」本身（三数组如何被三态结果改写、首调开窗丢弃既往账、
//! swap-取走-折叠-回存的数据流）仍无 host 可测钉板。本 crate 把窗口
//! 机盒按纯结构 [`UtilSlots`] 复刻——判序与折叠**继续单源调
//! berry-emface**（零第二实现），本面只钉「存储机械的推进律」。
//!
//! ## 与 sched 侧的对应（改这里的状态推进语义 = 改 sched 行为）
//!
//! | sched/src/util.rs（原子壳） | 本面（纯结构镜像） |
//! |---|---|
//! | `UTIL_ACCUM[cpu].fetch_add(delta)` | [`accumulate`](UtilSlots::accumulate) |
//! | `util_tick_window` 三态 match | [`tick_window`](UtilSlots::tick_window) |
//! | `cpu_util` 读 EWMA | [`read`](UtilSlots::read) |
//! | `util_reset` 清场 | [`reset`](UtilSlots::reset) |
//!
//! kernel 侧判序传 `accum=0`（真账以 `swap(0)` 原子取走为准——热路径
//! 免一次原子读）；纯面模型还原真实数据流：判序只用 start/now/窗长
//! （accum 不进判序），折叠用**真 accum**。
//!
//! ## 接线取舍（本件结论：钉板不接线）
//!
//! sched 侧 static 须为原子数组（跨核读者），`UtilSlots` 为纯值结构
//! ——kernel 直接实例化需每 tick 原子→纯结构来回拷贝，热路径开销换
//! 零行为收益。故 sched/src/util.rs **保持现状**（原子壳 + 单源调
//! emface 纯函数，W3-Q14 后已无第二实现）；本 crate 定位 = host 钉板
//! + 真账流文档化基准（未来 ktest/遥测面可消费 [`TickOutcome`]）。

#![cfg_attr(not(test), no_std)]

use berry_emface::{ewma_fold, window_state, WinState};

/// `tick_window` 三态结果（telemetry/测试断言消费面）。
///
/// 与 [`berry_emface::WinState`] 判序三态一一对应，但携带**推进后的
/// 结果载荷**（判序态 + 折叠产物），kernel 遥测面可直接断言越界拒/
/// 开窗/折叠三路。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickOutcome {
    /// 越界 cpu 拒（与 kernel `util_tick_window` 同形——IRQ 热路径
    /// 不 panic）或未到期零动作：状态不变。
    Ignored,
    /// 首调/回绕重开窗：既往 accum **丢弃**（store 0 语义）、
    /// window_start ← now、ewma 不动。
    NewWindow,
    /// 到期折叠：`util_raw`（本窗 min(accum×1024/elapsed,1024)）与
    /// 推进后的 ewma。
    Folded { util_raw: u32, ewma: u32 },
}

/// per-CPU util 槽纯投影（窗口机盒）。
///
/// 三数组与 `sched/src/util.rs` 的 static 一一对应；`N` 默认
/// [`berry_emface::UTIL_NR_CPU`]（v1 = 2 核）。const 全零构造——
/// host/ktest/boot 均可静态初始化。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtilSlots<const N: usize = { berry_emface::UTIL_NR_CPU }> {
    /// 当前 EWMA（0..=1024，PM 读面）——kernel 侧 `UTIL_EWMA`。
    pub ewma: [u32; N],
    /// 本窗口已累计运行 cycles——kernel 侧 `UTIL_ACCUM`。
    pub accum: [u64; N],
    /// 窗口起点（VCT cycles；0 = 未开窗哨兵）——kernel 侧
    /// `UTIL_WINDOW_START`。
    pub window_start: [u64; N],
}

impl<const N: usize> UtilSlots<N> {
    /// 全零构造（与 kernel static 初值一致）。
    pub const fn new() -> Self {
        Self {
            ewma: [0; N],
            accum: [0; N],
            window_start: [0; N],
        }
    }

    /// tick 面累计（kernel `util_accumulate` 镜像）：越界 cpu 静默拒。
    pub fn accumulate(&mut self, cpu: usize, delta_cycles: u64) {
        if cpu >= N {
            return;
        }
        self.accum[cpu] = self.accum[cpu].wrapping_add(delta_cycles);
    }

    /// 窗口到期检查（kernel `util_tick_window` 镜像，数据流还原）：
    /// 判序 `window_state(start, now, window_len, accum)`（纯面直接传
    /// 真 accum——kernel 传 0 只是免一次原子读的优化，判序不用它），
    /// 三态推进：
    /// - `NewWindow`（首调 start==0 / 回绕 now<start）→ 丢弃既往 accum
    ///   （store 0）、start ← now、ewma 不动；
    /// - `NotDue` → 零动作（状态逐字段不变）；
    /// - `Fold{elapsed}` → `ewma_fold(accum, elapsed, ewma)` 单源折叠、
    ///   accum ← 0（swap 语义）、start ← now。
    pub fn tick_window(&mut self, cpu: usize, now: u64, window_len: u64) -> TickOutcome {
        if cpu >= N {
            return TickOutcome::Ignored;
        }
        match window_state(self.window_start[cpu], now, window_len, self.accum[cpu]) {
            WinState::NewWindow => {
                // 丢弃既往账（半窗账无窗长可比——记进首窗会虚高首样）。
                self.accum[cpu] = 0;
                self.window_start[cpu] = now;
                TickOutcome::NewWindow
            }
            WinState::NotDue => TickOutcome::Ignored,
            WinState::Fold { accum, elapsed } => {
                // 折叠数学单源 emface；kernel 侧真账 swap(0) 取走与此处
                // 值语义一致（判序载荷 accum == 本结构 accum[cpu]）。
                let next = ewma_fold(accum, elapsed, self.ewma[cpu]);
                let util_raw = ((accum as u128 * 1024) / (elapsed as u128)).min(1024) as u32;
                self.ewma[cpu] = next;
                self.accum[cpu] = 0;
                self.window_start[cpu] = now;
                TickOutcome::Folded {
                    util_raw,
                    ewma: next,
                }
            }
        }
    }

    /// PM 读面（kernel `cpu_util` 镜像）：越界 → 0。
    pub fn read(&self, cpu: usize) -> u32 {
        if cpu >= N {
            return 0;
        }
        self.ewma[cpu]
    }

    /// 清零重开窗（kernel `util_reset` 镜像）：三数组全清、越界静默拒。
    pub fn reset(&mut self, cpu: usize) {
        if cpu >= N {
            return;
        }
        self.ewma[cpu] = 0;
        self.accum[cpu] = 0;
        self.window_start[cpu] = 0;
    }
}

impl<const N: usize> Default for UtilSlots<N> {
    fn default() -> Self {
        Self::new()
    }
}
