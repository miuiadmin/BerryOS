//! BerryOS 计费域账户 + 压力窗口折卷纯函数面（W3-Q76）。
//!
//! 单源对象 = `kernel/telemetry.rs` 的两段内联判定：
//! 1. **billing 槽分派**（S4-b 区间计费）：`billing_charge` 的槽查找/新建/
//!    表满覆写三态分派律——既有域累加（首命中槽）、空槽 CAS 抢占、表满
//!    覆写**最小账户**（v1 诚实取舍：牺牲最冷域，不静默丢账）。本 crate
//!    出纯判定与目标槽（`ChargeStep`），原子读改写留 telemetry 壳。
//! 2. **dpg 压力窗口折卷**（AM-2b）：`dpg_stall_fold` 的翻窗判（回绕减法
//!    `now.wrapping_sub(start) < len` 快退）+ θ_p 占比判据
//!    （`sum×100 ≥ W×θ_p` 乘法免除法、saturating 防多 waiter Σ>W 病态）
//!    + 窗长换算（`freq×win_ms/1000`，与 `dpg_win_init` 同式）。
//!
//! 纪律：判定律单源（本 crate），原子表/回调注册物理臂留 kernel
//! telemetry.rs（W3-Q10 telface / W3-Q6 itally 同款遥测账本族纪律）。

#![cfg_attr(not(test), no_std)]

// ---- billing 槽分派（S4-b）----

/// 账户表容量（v1 目标 ≤4 deadline 进程 + fair 域——8 槽余量；与
/// telemetry.rs 的 `BILLING_SLOTS` 镜像，const assert 执法）。
pub const BILLING_SLOTS: usize = 8;

/// 空槽哨兵（domain_id = 0 即空槽——SuspendTok key 同款「0 不承载」惯例）。
pub const DOMAIN_EMPTY: u64 = 0;

/// `billing_charge` 的分派决策（纯投影——原子读已完成，调用方按步执行）。
///
/// 判序与 telemetry.rs 旧内联逐语义对齐：
/// 1. `domain_id == 0 || ns == 0` → 无动作（零值不建账不累加）；
/// 2. 顺序扫首命中既有槽 → 累加该槽；
/// 3. 首个空槽 → 抢占（kernel 侧 CAS 防双占，败者重走分派——见
///    [`charge_step`] 的 `Empty` 步注记）；
/// 4. 全满 → 覆写**最小账户**槽（最小值并列取最前——v1 诚实取舍）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeStep {
    /// 零值拒绝：domain_id == 0 或 ns == 0，零动作。
    SkipZero,
    /// 既有域累加：槽号 + 累加 ns。
    Accrue { slot: usize },
    /// 空槽抢占：槽号（CAS 0→domain_id 成功后累加 ns；败者 = 他人先占，
    /// 重走 [`charge_step`]）。
    Claim { slot: usize },
    /// 表满覆写：最小账户槽号（store domain + store ns——覆写非累加）。
    Overwrite { slot: usize },
}

/// billing 槽分派判序（纯函数——`domains`/`cpu_ns` 为原子表快照投影）。
///
/// CAS 竞态吸收：`Claim` 失败后快照已变，调用方以新快照重走本函数即可
/// 收敛（既有域 → `Accrue`；另一空槽 → 新 `Claim`；全满 → `Overwrite`）。
/// 快照读与 CAS 间的窗口语义 = 旧内联实现（telemetry.rs 2026-11 形）。
pub fn charge_step(domain_id: u64, ns: u64, domains: &[u64], cpu_ns: &[u64]) -> ChargeStep {
    debug_assert_eq!(domains.len(), cpu_ns.len());
    if domain_id == DOMAIN_EMPTY || ns == 0 {
        return ChargeStep::SkipZero;
    }
    // 判序 1：既有域首命中槽累加。
    for (i, &d) in domains.iter().enumerate() {
        if d == domain_id {
            return ChargeStep::Accrue { slot: i };
        }
    }
    // 判序 2：首个空槽抢占。
    for (i, &d) in domains.iter().enumerate() {
        if d == DOMAIN_EMPTY {
            return ChargeStep::Claim { slot: i };
        }
    }
    // 判序 3：表满覆写最小账户（并列取最前）。
    let mut min_i = 0;
    let mut min_v = u64::MAX;
    for (i, &v) in cpu_ns.iter().enumerate() {
        if v < min_v {
            min_v = v;
            min_i = i;
        }
    }
    ChargeStep::Overwrite { slot: min_i }
}

/// 域账户查询（`billing_domain_total` 纯投影——首命中槽，缺席 None）。
pub fn domain_total(domain_id: u64, domains: &[u64], cpu_ns: &[u64]) -> Option<u64> {
    debug_assert_eq!(domains.len(), cpu_ns.len());
    if domain_id == DOMAIN_EMPTY {
        return None; // 0 恒空（不承载语义——快照域 0 只能是空槽）
    }
    for (i, &d) in domains.iter().enumerate() {
        if d == domain_id {
            return Some(cpu_ns[i]);
        }
    }
    None
}

// ---- dpg 压力窗口折卷（AM-2b）----

/// θ_p 初值 = 10%（AM-2b 裁决②【推，实测定标】；与 telemetry.rs
/// `DPG_WIN_THETA_P_PERCENT` 镜像，const assert 执法）。
pub const THETA_P_PERCENT: u64 = 10;

/// 折卷前的窗口态（纯投影——三原子快照）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinState {
    /// 窗长（周期数；0 = boot 接线前——快退）。
    pub len: u64,
    /// 窗口起点（CNTVCT）。
    pub start: u64,
    /// 当前窗 Σcyc。
    pub sum: u64,
}

/// 折卷产出（纯投影——胜者执行写臂，败者/未跨窗零动作）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldStep {
    /// 未接线（len == 0）或未跨窗——零动作（非压力态零翻卷成本）。
    Skip,
    /// 跨窗：折卷执行。`fire` = θ_p 判据过（调压力出口回调）；`carry`
    /// = 本窗 Σ（胜者 swap(0) 清账用）。
    Roll { fire: bool, carry: u64 },
}

/// 折卷判定（纯函数——`now` = 计值点 cntvct）。
///
/// 回绕语义：`now.wrapping_sub(start) < len` 即未跨窗（CNTVCT 单调递增
/// 不回绕，wrapping 减法吸收「start 在 future」的敌意快照——差值巨负
/// 即 huge，视作已跨窗走折卷，与 telemetry.rs 旧内联一致）。
pub fn fold_step(now: u64, w: WinState) -> FoldStep {
    if w.len == 0 {
        return FoldStep::Skip; // boot 接线前（防御快退）
    }
    if now.wrapping_sub(w.start) < w.len {
        return FoldStep::Skip; // 未跨窗——非压力态快退
    }
    // θ_p 占比判据（×100 免除法；saturating_mul 防多 waiter Σ>W 病态面
    // ——提案 §二「全局 Σ 分不清并发 waiter 数」同源）。
    let fire = w
        .sum
        .saturating_mul(100)
        >= w
            .len
            .saturating_mul(THETA_P_PERCENT);
    FoldStep::Roll { fire, carry: w.sum }
}

/// 窗长换算（`dpg_win_init` 同式：`freq × win_ms / 1000`，饱和防溢出）。
pub fn win_len_of(freq: u64, win_ms: u64) -> u64 {
    freq.saturating_mul(win_ms) / 1000
}

#[cfg(test)]
mod tests;
