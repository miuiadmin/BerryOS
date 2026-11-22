//! berry-amgrad —— am 终态五级弹性处置梯度决策核（am 总体设计契约一首
//! 刀：`.agents/plans/am/design.md`「终态五级弹性处置梯度」）。本 crate
//! 把五级表单源化为纯决策函数 [`choose`]：输入 [`Facts`]（水位/遮挡/
//! 后台时长/租约/压力持久性事实），输出 [`Tier`]（最高适用梯度）——
//! 执行动作（VMO 弃页/pager 压缩换出/IPI 冰封/job_kill/pkgmgr offload）
//! 归内核机制与 appd 接线刀，本层只钉「哪一级」的判定语义（防 appd 与
//! 内核两侧各拼一套梯度判定的漂移面）。
//!
//! ## 梯度语义（表语义的代码投影）
//!
//! - RAM 梯度（L0 < L2 < L1 < L3——升格序，`Tier` 的偏序即此）：
//!   L0 瞬时弃页（轻度水位或刚切后台）；L2 冰封（遮挡或退后台超防抖
//!   窗且零实证租约——防抖窗默认 3s、内存紧缺 0s，两值是契约钉值）；
//!   L1 压缩换出（**已处 L2** 且 PSI 页压上升——L1 建立在 L2 之上，
//!   表行触发条件原文）；L3 杀进程（重度压力且 L0–L1 仍不足，或驻留
//!   应用超上限）。
//! - 磁盘梯度（L4）正交：90 天未启动且磁盘紧张——RAM 梯度任何一级
//!   触发时优先于 L4（RAM 回收是磁盘瘦身的前置语义；纯冷应用才 L4）。
//! - 事实 [`Facts`] 的阈值字段（`just_backgrounded`、`debounce_ms` 的
//!   紧缺档切换）中，防抖窗双值是契约钉值；「刚切后台」窗宽表未钉，
//!   本层以 [`JUST_BG_WINDOW_MS`] 占位（参数面归 appd 配置刀——真值
//!   到位只改常量不改判定结构）。
//!
//! 家族纪律：零 unsafe、零依赖、零分配；`cfg_attr(not(test), no_std)`。

#![cfg_attr(not(test), no_std)]

use core::fmt;

/// 防抖窗默认值（契约钉值：退后台超此窗且零租约 → L2 冰封候选）。
pub const DEBOUNCE_MS: u32 = 3_000;
/// 内存紧缺档防抖窗（契约钉值：紧缺时 0s 立即冰封候选）。
pub const DEBOUNCE_TIGHT_MS: u32 = 0;
/// 「刚切后台」窗宽占位（表未钉——L0 触发件之一；真值归 appd 配置刀）。
pub const JUST_BG_WINDOW_MS: u32 = 1_000;
/// 冷应用阈值（契约钉值：90 天未启动）。
pub const COLD_APP_DAYS: u32 = 90;

/// 处置梯度（RAM 升格序 L0 < L2 < L1 < L3；L4 磁盘正交——序值仅表
/// RAM 谱系，L4 不参与 RAM 偏序比较）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// L0 瞬时弃页：回收 discardable 缓存页。
    L0,
    /// L2 多核原子冰封：冻结全线程，内存锁死不涨。
    L2,
    /// L1 冻结态压缩换出：L2 之上压缩私有脏页入加密交换池。
    L1,
    /// L3 杀进程 + 持久加密墓碑：100% 回收。
    L3,
    /// L4 冷应用无感瘦身（磁盘侧，正交）。
    L4,
}

/// 实证豁免租约集（契约二.1 的四大权威服务背书——零租约才可 L2）。
#[derive(Debug, Clone, Copy, Default)]
pub struct Leases {
    /// audiod：非静音真实音频流。
    pub audio: bool,
    /// locationd/callmgr：活跃导航或通话。
    pub nav_or_call: bool,
    /// berry-link/pkgmgr：跨端投送或下载进行中。
    pub transfer: bool,
    /// workmgr：系统合并维护窗口（限时 30s）。
    pub maintenance: bool,
}

impl Leases {
    /// 全零实证（L2 冰封的必要条件——契约「一旦全部租约归零，3 秒内
    /// 必入 L2」；判定面在此，3s 计时归 appd）。
    pub fn none(&self) -> bool {
        !(self.audio || self.nav_or_call || self.transfer || self.maintenance)
    }
}

/// 内存压力档（L0 触发 = Low；防抖窗紧缺档切换 = High；L3 判定 = High
/// 且回收不足——档间 Med 只影响防抖窗否（不紧缺）与 L0 否）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressure {
    Low,
    Med,
    High,
}

/// 决策输入事实（全部是可观测布尔/计数——无推导无猜测）。
#[derive(Debug, Clone, Copy)]
pub struct Facts {
    /// 内存压力档（PSI 侧输入）。
    pub pressure: Pressure,
    /// 应用刚切后台（在 JUST_BG_WINDOW_MS 窗内——L0 触发件）。
    pub just_backgrounded: bool,
    /// 窗口完全遮挡（L2 触发件之一，与防抖窗无关）。
    pub occluded: bool,
    /// 已退后台毫秒数。
    pub bg_ms: u32,
    /// 实证租约集。
    pub leases: Leases,
    /// 已处于 L2 冰封态（L1 的前置——表行原文「已处 L2 冰封态」）。
    pub frozen: bool,
    /// PSI 等页压上升（L1 触发件）。
    pub psi_rising: bool,
    /// L0–L1 回收后仍不足（L3 触发件——重度压力下的耗尽事实）。
    pub reclaim_exhausted: bool,
    /// 驻留应用数超上限（L3 触发件——与压力档无关）。
    pub resident_over_cap: bool,
    /// 未启动天数。
    pub idle_days: u32,
    /// 磁盘空间紧张。
    pub disk_tight: bool,
}

/// 梯度决策（五级表单源）：返回最高适用梯度；无任何触发件 = [`None`]
/// （无梯度动作——决策面禁造默认动作）。
///
/// 升格序：L3 > L1 > L2 > L0（RAM）；L4 仅在 RAM 面全静默且冷应用+
/// 磁盘紧时。L1 要求 `frozen`（表行「应用已处 L2 冰封态且 PSI 上升」）。
pub fn choose(f: &Facts) -> Option<Tier> {
    // L3：重度压力耗尽 或 驻留超上限（两路独立触发，均 100% 回收语义）。
    if (f.pressure == Pressure::High && f.reclaim_exhausted) || f.resident_over_cap {
        return Some(Tier::L3);
    }
    // L1：已冰封 + 页压上升 + 零租约（租约在场 = 解冻在途——豁免深化：
    // 契约二.1「唤醒即解冻」，冻结进程获实证租约时不再加深处置）。
    if f.frozen && f.psi_rising && f.leases.none() {
        return Some(Tier::L1);
    }
    // L2：遮挡或退后台超防抖窗（紧缺档窗 0——`bg_ms > 0` 即已退后台；
    // bg_ms == 0 = 前台，永不因窗触发），且零实证租约。
    let debounce = if f.pressure == Pressure::High { DEBOUNCE_TIGHT_MS } else { DEBOUNCE_MS };
    let bg_past_debounce = f.bg_ms > 0 && f.bg_ms >= debounce;
    if (f.occluded || bg_past_debounce) && f.leases.none() {
        return Some(Tier::L2);
    }
    // L0：轻度水位 或 刚切后台（瞬时弃页——最低梯度）。
    if f.pressure == Pressure::Low || f.just_backgrounded {
        return Some(Tier::L0);
    }
    // L4：RAM 面全静默，冷应用 + 磁盘紧（磁盘正交梯度）。
    if f.idle_days >= COLD_APP_DAYS && f.disk_tight {
        return Some(Tier::L4);
    }
    None
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Tier::L0 => "L0 瞬时弃页",
            Tier::L1 => "L1 冻结态压缩换出",
            Tier::L2 => "L2 多核原子冰封",
            Tier::L3 => "L3 杀进程+墓碑",
            Tier::L4 => "L4 冷应用瘦身",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calm() -> Facts {
        Facts {
            pressure: Pressure::Med,
            just_backgrounded: false,
            occluded: false,
            bg_ms: 0,
            leases: Leases::default(),
            frozen: false,
            psi_rising: false,
            reclaim_exhausted: false,
            resident_over_cap: false,
            idle_days: 0,
            disk_tight: false,
        }
    }

    #[test]
    fn calm_app_no_action() {
        assert_eq!(choose(&calm()), None);
    }

    #[test]
    fn l0_light_pressure_or_just_bg() {
        let mut f = calm();
        f.pressure = Pressure::Low;
        assert_eq!(choose(&f), Some(Tier::L0));
        let mut f2 = calm();
        f2.just_backgrounded = true;
        assert_eq!(choose(&f2), Some(Tier::L0));
    }

    #[test]
    fn l2_occlusion_and_debounce_window() {
        let mut f = calm();
        f.occluded = true;
        assert_eq!(choose(&f), Some(Tier::L2));
        // 退后台 3s 整（≥ 窗）→ L2；租约在场 → 不冰封（实证豁免）。
        let mut f2 = calm();
        f2.bg_ms = DEBOUNCE_MS;
        assert_eq!(choose(&f2), Some(Tier::L2));
        f2.leases.audio = true;
        assert_eq!(choose(&f2), None); // 音频租约豁免（且 Med 档无 L0）
        // 紧缺档：防抖窗 0——刚退后台即冰封候选。
        let mut f3 = calm();
        f3.pressure = Pressure::High;
        f3.bg_ms = 1;
        assert_eq!(choose(&f3), Some(Tier::L2));
        // 非紧缺 1ms：未过窗、无其他触发 → 无动作。
        let mut f4 = calm();
        f4.bg_ms = 1;
        assert_eq!(choose(&f4), None);
    }

    #[test]
    fn l1_requires_frozen_and_psi_rising() {
        let mut f = calm();
        f.psi_rising = true;
        assert_eq!(choose(&f), None); // 未冰封不 L1
        f.frozen = true;
        assert_eq!(choose(&f), Some(Tier::L1));
        // 四租约任一在场豁免的是 L2 冰封——L1 是已冰封进程的换出，
        // 租约在场时应先解冻：租约在场 + frozen → 决策面不升 L1
        //（解冻归 appd 执行序，本层以「租约豁免压制 RAM 梯度」表达）。
        let mut f2 = f;
        f2.leases.nav_or_call = true;
        assert_eq!(choose(&f2), None);
    }

    #[test]
    fn l3_pressure_exhausted_or_cap() {
        let mut f = calm();
        f.pressure = Pressure::High;
        f.reclaim_exhausted = true;
        assert_eq!(choose(&f), Some(Tier::L3));
        let mut f2 = calm();
        f2.resident_over_cap = true;
        assert_eq!(choose(&f2), Some(Tier::L3));
        // 高压未耗尽 → 不 L3（还不到杀）。
        let mut f3 = calm();
        f3.pressure = Pressure::High;
        assert_eq!(choose(&f3), None);
    }

    #[test]
    fn escalation_order() {
        // 同事实叠加时最高档胜：L3 > L1 > L2 > L0。
        let mut f = calm();
        f.pressure = Pressure::High;
        f.reclaim_exhausted = true;
        f.frozen = true;
        f.psi_rising = true;
        f.occluded = true;
        f.just_backgrounded = true;
        assert_eq!(choose(&f), Some(Tier::L3));
        f.reclaim_exhausted = false;
        assert_eq!(choose(&f), Some(Tier::L1));
        f.psi_rising = false;
        assert_eq!(choose(&f), Some(Tier::L2));
    }

    #[test]
    fn l4_cold_app_disk_only_when_ram_silent() {
        let mut f = calm();
        f.idle_days = COLD_APP_DAYS;
        f.disk_tight = true;
        assert_eq!(choose(&f), Some(Tier::L4));
        // RAM 面任何触发优先于 L4。
        f.pressure = Pressure::Low; // L0 触发
        assert_eq!(choose(&f), Some(Tier::L0));
        // 89 天不冷。
        let mut f2 = calm();
        f2.idle_days = COLD_APP_DAYS - 1;
        f2.disk_tight = true;
        assert_eq!(choose(&f2), None);
    }

    #[test]
    fn tier_ordering_partial() {
        // RAM 谱系偏序：L0 < L2 < L1 < L3（L4 不比——正交）。
        assert!(Tier::L0 < Tier::L2);
        assert!(Tier::L2 < Tier::L1);
        assert!(Tier::L1 < Tier::L3);
        assert_eq!(format!("{}", Tier::L3), "L3 杀进程+墓碑");
    }
}
