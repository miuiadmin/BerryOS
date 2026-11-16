//! 策略四动作 hint 产面 —— v3 全四动作激活刀的纯函数位（lib 核面 ⑤）。
//!
//! 语境（ADR #42 动作空间四类 + 15 号 EAS-PA 刀顺延）：berry-power 从
//! 「EAS placement 单动作」扩为完整策略服务——消费 util EWMA 与 thermal
//! 遥测产出四动作 hint：①placement（低核偏好——[`UtilTrack::
//! placement_hint`]，15 号已有）；②frequency（schedutil 同构
//! `f ≈ util × fmax / capacity`——`util` 面单源公式直用）；③weight 微调
//! （包络内 ±Δ）；④EPP（thermal_level 联动 → 效率档）。
//!
//! **注入形态定谳（15 号 §0-1 顺延）**：①③④不是三个 op——`policy_bind`
//! 0x83 的动作面 = COMMIT（op=1）**表条目字段**（placement@12 / weight@8 /
//! epp_flags@13——`user/berry-sys` `PolicyEntry` 16B 镜像），三字段同条目
//! 一次整表提交；②**无 0x83 载体**——动作空间立法原文「频率/热限 hint
//! （走 PM 服务、不经调度路径——不在表里）」（`sched/src/policy.rs`
//! 模块注），而本服务就是 PM：频率 hint 落自家频率裁决（`FreqVerdict`
//! req/eff），内核聚合点写面留窗（main.rs 窗 4）。
//!
//! 词汇与包络单源：EPP 两位打包与权重包络复用 `berry-polface`
//! （`EPP_HAS`/`WEIGHT_MAX`——kernel `sched/policy.rs` 同源消费，本面零
//! 第二实现；用户态 ABI 位 [`berry_sys::POLICY_EPP_HAS`] 亦同值镜像）。
//!
//! **推断标注**（策略数值面 = 不承诺面，表项可调，ADR #68 ⑥ 口径）：
//! - weight 步长 16 = 包络 ±64 的四分之一（首刀保守档）；
//! - 高/低档阈 896/128 = util 满格 1024 的 7/8 与 1/8（对称分数档）；
//! - thermal_level ≥ 1（fair/serious/critical 四档词汇——berry-thermal
//!   lib.rs「0 nominal/1 fair/2 serious/3 critical」）→ Efficiency；0
//!   （nominal）→ 无意见不覆盖——二值词汇（Efficiency/Perf）无 balance
//!   档，内核默认 Efficiency（省电身份支柱，iprofile 注）已在位，武断
//!   Perf 不如沉默；fair 起偏效率对齐 berry-thermal 频率帽阶梯（fair 档
//!   已开始限频 70%——热动作同档起效）。

use crate::util::{schedutil_freq_khz, UtilTrack};
use berry_polface::{EPP_HAS, WEIGHT_MAX};

/// ③ weight 微调步长（|Δ| ≤ [`WEIGHT_MAX`] 包络——推断标注见模块注）。
pub const WEIGHT_STEP: i32 = 16;
/// ③ util 高档阈（≥ → `+WEIGHT_STEP`；满格 7/8——推断标注见模块注）。
pub const UTIL_HI: u32 = 896;
/// ③ util 低档阈（≤ → `-WEIGHT_STEP`；满格 1/8——推断标注见模块注）。
pub const UTIL_LO: u32 = 128;
/// ④ thermal_level 起 Efficiency 档（≥ 此值 → 效率优先；0 nominal →
/// 无意见——推断标注见模块注）。
pub const THERMAL_EFF_LEVEL: u8 = 1;

/// 四动作 hint 快照（[`crate::PowerCore::action_hints`] 的产出、bin 壳
/// `policy_bind` 提交的数据源——四动作一次读全，字段间零隐式耦合）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionHints {
    /// ① placement hint（1 偏 little / 2 偏 big——`UtilTrack::
    /// placement_hint` 低核偏好）。
    pub placement: u8,
    /// ② frequency hint（schedutil 请求 kHz——**PM 域动作，无 0x83 载体**
    /// ：落自家频率裁决 req，聚合帽 min 后 eff 才是可写频率；capacity=0
    /// （EM 未注册）→ 0 = 无据不求频）。
    pub freq_req_khz: u32,
    /// ③ weight 微调（±`WEIGHT_STEP`，构造性 ≤ [`WEIGHT_MAX`] 包络——
    /// 内核侧 handler/build/消费点三点夹紧仍执法，产面自律是第一道）。
    pub weight_delta: i32,
    /// ④ EPP flags（ABI 字节：0 = 无意见；`EPP_HAS` = Efficiency 覆盖
    /// 〔val 位 0〕——Perf 档词汇在位但本产面不发，热联动单向）。
    pub epp_flags: u8,
}

/// ③ weight 微调产面：util 高 → `+STEP`（忙段提带早完——race-to-idle：
/// `prio_effective` 是带序键、高 = 优〔`sched/src/class.rs` 4-3a pick 按
/// effective 出选〕，忙任务早跑完早进深空闲，电池净赢）；util 低 →
/// `-STEP`（闲任务礼貌降带）；中带 → 0（无意见）。输入 = 域内最大 EWMA
/// （v1 单频域系统负载口径——与 ② 频率面同源 `max_est`）。
pub fn weight_delta(util: u32) -> i32 {
    if util >= UTIL_HI {
        WEIGHT_STEP
    } else if util <= UTIL_LO {
        -WEIGHT_STEP
    } else {
        0
    }
}

/// ④ EPP 产面：thermal_level 联动（None = 无热帧 → 无意见；0 nominal →
/// 无意见不覆盖；≥ [`THERMAL_EFF_LEVEL`] → Efficiency）。返回 ABI flags
/// 字节（polface 两位打包单源——bit0 带覆盖、bit1 取值 0=Efficiency；
/// 野值 level 防御侧同 Efficiency〔热向单调：越热越省〕）。
pub fn epp_flags(thermal_level: Option<u8>) -> u8 {
    match thermal_level {
        Some(l) if l >= THERMAL_EFF_LEVEL => EPP_HAS, // Efficiency（val 位 0）
        _ => 0, // 无意见（None/nominal——不覆盖内核默认）
    }
}

/// 四动作 hint 一次产出：①③④消费同一 [`UtilTrack`] 态（placement 逐核
/// 比较、weight 取域内最大），②同源公式直用——四字段一次读全，与分面
/// 重算逐位一致（不长第二套数学）。
pub fn action_hints(
    util: &UtilTrack,
    thermal_level: Option<u8>,
    fmax_khz: u32,
    capacity: u32,
) -> ActionHints {
    let max = util.max_est();
    ActionHints {
        placement: util.placement_hint(),
        freq_req_khz: schedutil_freq_khz(max, fmax_khz, capacity).unwrap_or(0),
        weight_delta: weight_delta(max),
        epp_flags: epp_flags(thermal_level),
    }
}

// 包络自律编译期钉（WEIGHT_STEP 越包络 = 产面违约，编译期响）。
const _: () = assert!(WEIGHT_STEP >= 0 && WEIGHT_STEP <= WEIGHT_MAX);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::UTIL_FULL;
    use berry_polface::EPP_VAL;

    /// ③ 三带 + 阈端含端语义 + 包络自律（产面恒 |Δ| ≤ WEIGHT_MAX——
    /// 内核夹紧执法外的第一道构造性保证）。
    #[test]
    fn weight_delta_three_bands() {
        // 低带（含 0 与阈端 128——≤ 语义）。
        assert_eq!(weight_delta(0), -WEIGHT_STEP);
        assert_eq!(weight_delta(UTIL_LO), -WEIGHT_STEP, "低档阈含端（≤）");
        // 中带零意见（129..=895 开区间）。
        assert_eq!(weight_delta(UTIL_LO + 1), 0);
        assert_eq!(weight_delta(UTIL_FULL / 2), 0, "中带无意见");
        assert_eq!(weight_delta(UTIL_HI - 1), 0);
        // 高带（含阈端 896 与满格——≥ 语义）。
        assert_eq!(weight_delta(UTIL_HI), WEIGHT_STEP, "高档阈含端（≥）");
        assert_eq!(weight_delta(UTIL_FULL), WEIGHT_STEP);
        // 包络自律：全域三支均 ≤ WEIGHT_MAX（±64——policy.rs 动作空间）。
        for u in [0u32, UTIL_LO, UTIL_LO + 1, 512, UTIL_HI - 1, UTIL_HI, UTIL_FULL] {
            assert!(weight_delta(u).abs() <= WEIGHT_MAX, "u={u}");
        }
    }

    /// ④ 热档联动：None/nominal 无意见、fair 起 Efficiency（val 位 0）、
    /// flags 只用 polface 两位（无未定义位——内核 -EINVAL 拒面的产面
    /// 免疫构造性钉板）。
    #[test]
    fn epp_flags_thermal_bands() {
        assert_eq!(epp_flags(None), 0, "无热帧（无 sim 热帧）无意见");
        assert_eq!(epp_flags(Some(0)), 0, "nominal 不覆盖（balance 的二值诚实形）");
        // 四档词汇（berry-thermal lib.rs：0 nominal/1 fair/2 serious/
        // 3 critical）——fair 起效率优先。
        for l in 1..=3u8 {
            assert_eq!(epp_flags(Some(l)), EPP_HAS, "档 {l} → Efficiency");
        }
        // 野值（> THERMAL_LEVEL_MAX=3 防御）——热向单调同 Efficiency。
        assert_eq!(epp_flags(Some(255)), EPP_HAS);
        // 位自律：产面 flags 只用 polface 两位（bit2.. 未定义位恒 0）。
        assert_eq!(EPP_HAS & !(EPP_HAS | EPP_VAL), 0);
        assert_eq!(epp_flags(Some(2)) & !(EPP_HAS | EPP_VAL), 0);
    }

    /// 四动作联合：同 UtilTrack 态一次产出，四字段与分面重算逐位一致
    /// （联合面零第二套数学的钉板）+ 忙轨高档正向 + EM 缺席形态
    /// （capacity=0 → ② freq 0 无据不求频，①③④不受污染）。
    #[test]
    fn action_hints_end_to_end() {
        // 低带轨：CPU0 300 样本 ×4 → EWMA 122（≤128 低档），CPU1 静。
        let mut t = UtilTrack::new();
        for _ in 0..4 {
            t.ingest_sample(0, 300);
        }
        assert_eq!(t.max_est(), 122, "前置：EWMA 折叠期望值");
        let h = action_hints(&t, Some(1), 1_000_000, 1024);
        assert_eq!(h.placement, t.placement_hint(), "① 与分面同源");
        assert_eq!(
            h.freq_req_khz,
            schedutil_freq_khz(t.max_est(), 1_000_000, 1024).unwrap(),
            "② 与 util 面单源公式一致"
        );
        assert_eq!(h.weight_delta, weight_delta(t.max_est()), "③ 与分面同源");
        assert_eq!(h.epp_flags, epp_flags(Some(1)), "④ 与分面同源");
        assert_eq!(h.placement, 2, "CPU0 独忙 → 避开偏 big");
        assert_eq!(h.weight_delta, -WEIGHT_STEP, "低带礼貌降带");
        assert_eq!(h.epp_flags, EPP_HAS, "fair → Efficiency");
        // 忙轨：满格样本 ×24 → EWMA ≥896 入高档 → +STEP（race-to-idle）；
        // ④ 与 ③ 独立（无热帧 → 无意见）。
        let mut b = UtilTrack::new();
        for _ in 0..24 {
            b.ingest_sample(0, UTIL_FULL);
        }
        assert!(b.max_est() >= UTIL_HI, "前置：忙轨须入高档（{}）", b.max_est());
        let hb = action_hints(&b, None, 1_000_000, 1024);
        assert_eq!(hb.weight_delta, WEIGHT_STEP, "忙段提带");
        assert_eq!(hb.epp_flags, 0, "无热帧——④与③独立");
        // EM 缺席（capacity=0）：② 保守静默 0，①③④ 零污染。
        let h0 = action_hints(&t, Some(2), 1_000_000, 0);
        assert_eq!(h0.freq_req_khz, 0, "EM 未注册不求频");
        assert_eq!(
            (h0.placement, h0.weight_delta, h0.epp_flags),
            (h.placement, h.weight_delta, h.epp_flags),
            "①③④不受 EM 缺席污染"
        );
    }
}
