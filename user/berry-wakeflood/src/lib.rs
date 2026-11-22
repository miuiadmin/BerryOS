//! berry-wakeflood —— 唤醒风暴熔断核（am 总体设计契约二.2 首刀：
//! `.agents/plans/am/design.md`「唤醒即解冻与风暴熔断——若某异常
//! 客户端每分钟触发被动解冻超 12 次（Wake-Flood），`appd` 自动切入
//! 节流退避并记入 power-thermal 异常耗电台账」）。本 crate 钉**判定
//! 面**：每客户端分钟桶环形计数（定容零分配）+ 熔断/释放两态机；
//! 计时器驱动、退避投递与台账登记归 appd 服务刀。
//!
//! 语义律：
//! - 分钟桶是滑动落桶形（整分钟对齐落桶 + 桶龄淘汰）——`on_tick`
//!   推进当前分钟号，旧桶自动清零（环形 2 桶：当前 + 上分钟——「每
//!   分钟超 12 次」的窗语义按当前分钟桶计，跨分钟不累计）。
//! - 熔断即进入退避：冷却窗内 `should_throttle` 恒真；冷却期满且新
//!   速率回健康（当前桶 ≤ 阈值）才释放——释放后速率仍异常则立即
//!   再熔断（抖动环防护）。
//! - 阈值 [`THRESHOLD_PER_MIN`] = 12 与冷却窗 [`COOLDOWN_MIN`] = 5
//!   是本刀钉值（表文只钉 12 次/分钟；冷却窗是工程默认，注记归
//!   appd 配置刀）。

#![cfg_attr(not(test), no_std)]

/// 每分钟被动解冻计数阈（契约钉值：超 12 次/分钟 = Wake-Flood）。
pub const THRESHOLD_PER_MIN: u32 = 12;
/// 熔断冷却窗（分钟）——工程默认（表未钉；真值归 appd 配置刀）。
pub const COOLDOWN_MIN: u64 = 5;

/// 每客户端熔断器（定容，零分配）。
#[derive(Debug, Clone, Copy)]
pub struct FloodBreaker {
    minute: u64,
    cur: u32,
    prev: u32,
    tripped: bool,
    trip_minute: u64,
}

impl Default for FloodBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl FloodBreaker {
    pub fn new() -> Self {
        Self { minute: 0, cur: 0, prev: 0, tripped: false, trip_minute: 0 }
    }

    /// 时间推进（当前分钟号单调增；跳分钟即换桶——上分钟桶归 prev，
    /// 再旧清零）。
    pub fn on_tick(&mut self, minute: u64) {
        if minute <= self.minute {
            return;
        }
        self.prev = self.cur;
        self.cur = 0;
        self.minute = minute;
        // 释放纯时间驱动（冷却期满即释）——释放后速率仍异常由 on_wake
        // 的立即再熔断环守（时钟是释放唯一驱动，唤醒计数不驱动释放）。
        if self.tripped && minute >= self.trip_minute + COOLDOWN_MIN {
            self.tripped = false;
        }
    }

    /// 记一次被动解冻（熔断态下仍计数——释放判据要用）。
    pub fn on_wake(&mut self) {
        self.cur += 1;
        if !self.tripped && self.cur > THRESHOLD_PER_MIN {
            self.tripped = true;
            self.trip_minute = self.minute;
        }
    }

    /// 当前分钟桶计数。
    pub fn count(&self) -> u32 {
        self.cur
    }

    /// 是否处于熔断退避（节流投递面）。
    pub fn should_throttle(&self) -> bool {
        self.tripped
    }

    /// 熔断发生分钟（未熔断 None——台账面对账用）。
    pub fn trip_minute(&self) -> Option<u64> {
        if self.tripped { Some(self.trip_minute) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_under_threshold() {
        let mut b = FloodBreaker::new();
        for _ in 0..THRESHOLD_PER_MIN {
            b.on_wake();
        }
        assert!(!b.should_throttle());
        assert_eq!(b.trip_minute(), None);
    }

    #[test]
    fn trips_over_threshold_same_minute() {
        let mut b = FloodBreaker::new();
        for _ in 0..(THRESHOLD_PER_MIN + 1) {
            b.on_wake();
        }
        assert!(b.should_throttle());
        assert_eq!(b.trip_minute(), Some(0));
        assert_eq!(b.count(), THRESHOLD_PER_MIN + 1);
    }

    #[test]
    fn minute_rollover_resets_count() {
        let mut b = FloodBreaker::new();
        for _ in 0..THRESHOLD_PER_MIN {
            b.on_wake();
        }
        b.on_tick(1); // 换分钟：计数归零
        assert_eq!(b.count(), 0);
        assert!(!b.should_throttle());
        b.on_wake();
        assert!(!b.should_throttle());
    }

    #[test]
    fn cross_minute_no_accumulation() {
        // 「每分钟超 12」按单分钟桶——两分钟各 12 次不熔断。
        let mut b = FloodBreaker::new();
        for _ in 0..THRESHOLD_PER_MIN {
            b.on_wake();
        }
        b.on_tick(1);
        for _ in 0..THRESHOLD_PER_MIN {
            b.on_wake();
        }
        assert!(!b.should_throttle());
    }

    #[test]
    fn cooldown_release_and_retrip() {
        let mut b = FloodBreaker::new();
        for _ in 0..(THRESHOLD_PER_MIN + 3) {
            b.on_wake();
        }
        assert!(b.should_throttle());
        // 冷却未满：仍熔断（tick 不释放）。
        b.on_tick(COOLDOWN_MIN - 1);
        assert!(b.should_throttle());
        // 冷却期满 → 释放（时间驱动；换桶后 cur=0）。
        b.on_tick(COOLDOWN_MIN);
        assert!(!b.should_throttle());
        // 释放后速率仍异常 → 立即再熔断（抖动环防护）。
        for _ in 0..(THRESHOLD_PER_MIN + 1) {
            b.on_wake();
        }
        assert!(b.should_throttle());
        assert_eq!(b.trip_minute(), Some(COOLDOWN_MIN));
    }

    #[test]
    fn stuck_minute_no_release_while_flood_continues() {
        // 冷却期满但同分钟内又洪水：on_tick 释放判据在换桶时 cur 已
        // 清零——释放后 on_wake 立即再熔断（同一分钟第 13 次唤醒即跳）。
        let mut b = FloodBreaker::new();
        for _ in 0..(THRESHOLD_PER_MIN + 1) {
            b.on_wake();
        }
        b.on_tick(COOLDOWN_MIN); // 释放
        for _ in 0..(THRESHOLD_PER_MIN + 1) {
            b.on_wake();
        }
        assert!(b.should_throttle());
    }

    #[test]
    fn tick_monotonic_guard() {
        let mut b = FloodBreaker::new();
        b.on_wake();
        b.on_tick(5);
        b.on_tick(3); // 回拨忽略
        assert_eq!(b.count(), 0);
        assert_eq!(b.minute, 5);
    }
}
