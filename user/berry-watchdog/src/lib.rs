//! # berry-watchdog —— 软件看门狗骨架 v1
//!
//! 驱动子系统研究报告（2026-11-18）P1-10「硬件看门狗完全缺失」的 v1
//! 骨架件：QEMU virt 机没有 WDT 设备（goldfish-WDT/PL031 系均未挂），
//! 真硬件驱动是后续刀——本刀先落**纯软件超时窗**：[`Watchdog::new`]
//! 装表 → [`Watchdog::kick`] 喂狗（内部取单调钟锚定）→
//! [`Watchdog::is_expired`] 查询超时。教学演示足够（监督进程主循环喂狗、
//! 卡死即窗口过期可观测）；真硬件接线时 kick 面换成 WDT 寄存器写/重启
//! 动作，三口契约不动。
//!
//! 时基纪律：kick 与 is_expired 同源**单调钟微秒**（`berry_clock_read
//! (CLOCK_MONOTONIC)` ns → µs）。REALTIME 绝不作看门狗时基——壁钟可跳，
//! 跳变后要么假超要么永超，单调性是超时语义的前置。查询面注入 now_us
//! （消费面拿什么 now 查什么，库零 IO——berry-partition 读盘注入闭包
//! 同款纪律）；host 测试注入 thread-local 假钟（berry-appshell Host 绑定
//! 表注入先例；每测试线程独立，并行测试零竞争）。
//!
//! 形态照 berry-softime 家族：`cfg_attr(not(test), no_std)` + host 测试；
//! syscall 面 `cfg(not(test))`（berry-fsclient 纪律——host 测试二进制零
//! svc 面）。真消费接线（svcmgr 监督循环/launcher 卡死检测喂狗）挂账
//! 后续刀——本刀纯新增面，零行为变化。

#![cfg_attr(not(test), no_std)]

/// 软件看门狗：上次喂狗锚点 + 超时窗（µs）。Copy 语义——快照查询
/// （`is_expired(&self)`）不锁状态，喂狗独占 `&mut self`。
#[derive(Clone, Copy, Debug)]
pub struct Watchdog {
    timeout_us: u64,
    last_kick_us: u64,
}

impl Watchdog {
    /// 装表即锚定（构造 = 首次喂狗语义——监督循环 spawn 后立即可查）。
    /// `timeout_ms` × 1000 饱和乘：u64::MAX 级域外值饱和为「永不超」的
    /// 诚实退化，不 panic；`timeout_ms = 0` = 零窗恒超（含等号口径）。
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            timeout_us: timeout_ms.saturating_mul(1000),
            last_kick_us: now_us(),
        }
    }

    /// 喂狗：`last_kick_us` 重锚当前单调钟 µs（窗口整体平移，旧窗作废）。
    pub fn kick(&mut self) {
        self.last_kick_us = now_us();
    }

    /// 查询超时：`now_us` 距上次喂狗 ≥ 超时窗即过期（**含等号**——到期
    /// 点即咬：`new(1000)` 在恰 1000ms 处已超；999ms 未超/1001ms 已超的
    /// 任务书场景见测试）。`now_us` 早于锚点（时钟回拨防御）→
    /// saturating_sub 得 0，仅零窗恒超。与 [`Watchdog::kick`] 同时基
    /// （单调钟 µs）——消费面传同一口钟的读数。
    pub fn is_expired(&self, now_us: u64) -> bool {
        now_us.saturating_sub(self.last_kick_us) >= self.timeout_us
    }

    /// 本窗口到期时刻（µs）= 锚 + 窗（饱和加）。可观测面——过期日志/
    /// dmesg 报点用，判定语义仍以 [`Watchdog::is_expired`] 单源。
    pub fn deadline_us(&self) -> u64 {
        self.last_kick_us.saturating_add(self.timeout_us)
    }
}

/// 喂狗取时：guest = `CLOCK_MONOTONIC` syscall µs（失败 → 0——内核对
/// 单调钟恒成功，结构性不可达；诚实注记不 panic）；host 测试 =
/// thread-local 假钟（`set_now_us` 注入）。
fn now_us() -> u64 {
    #[cfg(not(test))]
    {
        let mut ns: berry_sys::Time = 0;
        if berry_sys::berry_clock_read(berry_sys::CLOCK_MONOTONIC, &mut ns) == 0 {
            ns / 1_000
        } else {
            0
        }
    }
    #[cfg(test)]
    {
        TEST_NOW_US.with(|c| c.get())
    }
}

// —— host 测试假钟（thread-local：cargo test 每测试一线程 → 天然隔离，
//    并行测试零竞争；berry-appshell Host 绑定表注入同族）——
#[cfg(test)]
std::thread_local! {
    static TEST_NOW_US: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// 假钟注入（仅 host 测试构建存在）：拨到 `us` 后，`new`/`kick` 取时即
/// 读该值——时间前进完全由测试剧本驱动。
#[cfg(test)]
fn set_now_us(us: u64) {
    TEST_NOW_US.with(|c| c.set(us));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 任务书场景钉板：new(1000) → kick → 999ms 未超 → 1001ms 超了。
    #[test]
    fn task_scenario_999_ok_1001_bites() {
        set_now_us(0);
        let mut wd = Watchdog::new(1000);
        wd.kick();
        assert!(!wd.is_expired(999_000), "999ms 不得超");
        assert!(wd.is_expired(1_001_000), "1001ms 必超");
    }

    /// 到期点含等号：恰 1000ms = 已超（「到期即咬」口径）；deadline_us
    /// 与判定同源（锚 0 + 窗 1s）。
    #[test]
    fn exact_deadline_bites() {
        set_now_us(0);
        let wd = Watchdog::new(1000);
        assert!(!wd.is_expired(999_999));
        assert!(wd.is_expired(1_000_000));
        assert_eq!(wd.deadline_us(), 1_000_000);
    }

    /// 重喂平移窗口：kick 后旧窗作废、新窗从新锚起算；时钟回拨（now 早于
    /// 锚）不假超。
    #[test]
    fn rekick_rearms_window() {
        set_now_us(0);
        let mut wd = Watchdog::new(1000);
        assert!(!wd.is_expired(500_000)); // 原窗内
        set_now_us(5_000_000);
        wd.kick();
        assert!(!wd.is_expired(5_999_000)); // 新窗 999ms
        assert!(wd.is_expired(6_001_000)); // 新窗 1001ms
        assert!(!wd.is_expired(1_500_000)); // 回拨防御：now < 锚 → 不假超
        assert_eq!(wd.deadline_us(), 6_000_000);
    }

    /// 装表即锚定（无 kick 也已是活窗）；零窗 = 恒超；域外巨窗饱和 =
    /// 永不超的诚实退化（不 panic）。
    #[test]
    fn anchored_new_zero_and_saturated_timeouts() {
        set_now_us(1_000);
        let wd = Watchdog::new(1000);
        assert!(!wd.is_expired(1_000_999)); // 构造锚定生效：未喂也未超
        let zero = Watchdog::new(0);
        assert_eq!(zero.timeout_us, 0);
        assert!(zero.is_expired(1_000)); // 零窗：装表即到期（0 ≥ 0）
        let huge = Watchdog::new(u64::MAX);
        assert_eq!(huge.timeout_us, u64::MAX); // 饱和乘
        assert!(!huge.is_expired(1_000_000));
        assert_eq!(huge.deadline_us(), u64::MAX); // 锚 + 窗饱和加
    }
}
