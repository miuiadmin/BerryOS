//! berry-tickface——CNTV 滴答账纯函数面（W3-Q31，自 `arch/aarch64/src/
//! timer.rs` 外迁）。
//!
//! 单源纪律（同 berry-dtbface/hofface）：**判定律唯一事实源在此**，
//! `arch/aarch64/src/timer.rs` 机械壳 re-export 消费（寄存器臂
//! `mrs`/`msr`、原子 static、`static mut` 留壳）。零依赖纯值域——
//! host 可测钉板（`cfg_attr(not(test), no_std)` 双面）。
//!
//! 六面：
//! 1. [`ns_reciprocal_for`]：tick→ns 倒数乘-移对预计算（最大 shift 精度
//!    最优 + mul 上界无溢判，`const fn`——编译期可独立钉板）。
//! 2. [`ns_fast_path`]/[`ns_slow_path`]：ticks→ns 快/慢双路（快路资格判
//!    `mul != 0 ∧ tick < 2^52`；慢路两除精确式——先商后余防中间溢出）。
//! 3. [`arm_slice_clamp`]：S-6 deadline 单调夹取律（`now < cur < want`
//!    保留 `cur`——防块驱动快速切换的 deadline 跑步机饿死 tick）。
//! 4. [`slice_budget`]：slice 预算再装数学（`freq / SLICE_HZ`；freq==0
//!    → 0 防御）。
//! 5. [`slice_cap`]：授片臂收紧律（`0 < grant < slice` 取 grant、否则回落
//!    slice——arm_slice_cap 的片长帽，arm_slice 收紧刀 2026-11-22）。
//! 6. [`vct_delta`]：LAST_VCT 回绕窗增量（`last==0` 首读零增量 +
//!    `wrapping_sub` u64 回绕安全——计数器自然回绕下 delta 仍正确）。
//!
//! 纪律（自 timer.rs 逐字继承）：
//! - **纯函数、零 panic、零 unsafe**——异常频率（0 / >1GHz）→ (0,0) 只走
//!   慢路，语义与旧两除式逐位一致。
//! - 溢出界证明见各函数 doc；全域快慢两路逐位一致由 host 测试 + 编译期
//!   `const _` 钉板双保险。

#![cfg_attr(not(test), no_std)]

/// 时间片频率（每秒切片数）。10Hz → 每 100ms 一 tick（slice=6.25M cycles
/// @62.5MHz）。调小→切片长（切换少、开销低）；调大→更公平但切换多。
pub const SLICE_HZ: u64 = 10;

/// 快路适用域上界：tick < 2^52（@62.5MHz ≈ 2.35 年单调 uptime；24MHz ≈
/// 6.1 年）。与 mul 上界（[`NS_MUL_CAP`]）联立保证快路单乘无溢。
pub const NS_FAST_DOMAIN_TICKS: u64 = 1 << 52;

/// mul 上界 = 2^(64-52)：`tick < 2^52 ∧ mul < 2^12 ⇒ tick·mul < 2^64`。
pub const NS_MUL_CAP: u64 = 1 << 12;

/// 给定 CNTFRQ 预计算 tick→ns 倒数乘-移对：`(tick·mul) >> shift ≈
/// tick·1e9/freq`。取最大 shift（精度最优）且 mul < [`NS_MUL_CAP`]（与
/// [`NS_FAST_DOMAIN_TICKS`] 联立保证快路域内单乘无溢）。mul = round(2^shift·
/// 1e9/freq)（round-half-up）→ 每 tick 误差 ≤ 半 ulp ≤ 0.5ns——「<1ns/tick」
/// 合同对任意频率构造成立；ns/tick 二进精准的频率（QEMU virt 62.5MHz =
/// 16ns）误差恰 0。freq==0 / >1GHz（ns/tick < 1，现实 CNTFRQ 1–100MHz 外
/// 的防御面）→ (0, 0)（只走慢路）。纯 const fn：不写 static，编译期可
/// 独立钉板。
pub const fn ns_reciprocal_for(freq: u64) -> (u64, u32) {
    if freq == 0 || freq > 1_000_000_000 {
        return (0, 0);
    }
    // 最大 shift 判据（全整数，免 u128）：round(2^s·1e9/f) ≤ CAP−1
    //   ⟺ 2^s·1e9/f < CAP − 1/2 ⟺ 2^s·2e9 < f·(2·CAP−1)。
    // 溢界：s ≤ 43 ⇒ 2^s·2e9 ≤ 8.8e18 < 2^64（循环界 44 即防溢界）；
    // f ≤ 1e9 ⇒ f·(2·CAP−1) < 8.2e12 同无溢（freq 守卫已锁）。
    let bound = freq * (2 * NS_MUL_CAP - 1);
    let mut s: u32 = 0;
    while s < 44 && (1u64 << s) * 2_000_000_000 < bound {
        s += 1;
    }
    if s == 0 {
        return (0, 0); // round(1e9/f) ≥ CAP（频率 <244Hz）：快路给不出精度，弃
    }
    let num = (1u64 << (s - 1)) * 1_000_000_000;
    // s−1 ≤ 42 ⇒ num ≤ 4.4e18，+freq/2 ≤ +5e8——无溢。
    ((num + freq / 2) / freq, s - 1)
}

/// ticks→ns 快路资格判 + 值：`mul != 0`（init 预计算有效哨兵）`∧ tick <
/// [`NS_FAST_DOMAIN_TICKS`]`（域界内单乘无溢）时返回 `(tick·mul) >> shift`；
/// 否则 `None`（走 [`ns_slow_path`]）。纯 const fn——寄存器/原子面留壳，
/// 编译期钉板可直达。
pub const fn ns_fast_path(ticks: u64, mul: u64, shift: u32) -> Option<u64> {
    if mul != 0 && ticks < NS_FAST_DOMAIN_TICKS {
        // 域内无溢：tick < 2^52 ∧ mul < 2^12 ⇒ 乘积 < 2^64。
        Some((ticks * mul) >> shift)
    } else {
        None
    }
}

/// ticks→ns 慢路（两除精确式）：`(ticks/freq)·1e9 + (ticks%freq)·1e9/freq`
/// ——先商后余：余段 `(ticks%freq) < freq ≤ 1e9 ⇒ 余·1e9 ≤ 9.2e17` 无溢
/// （`freq` 守卫在合同域；`ticks/freq` 商段随输入域收缩，现实 CNTVCT 域
/// < 2^52 下商·1e9 < 2^64 无溢）。freq==0 → 0（旧契约：帽不触发，方向
/// 安全）。与旧 timer.rs 内联两除式逐位一致。const fn——编译期钉板可直达。
pub const fn ns_slow_path(ticks: u64, freq: u64) -> u64 {
    if freq == 0 {
        return 0;
    }
    (ticks / freq) * 1_000_000_000 + ((ticks % freq) * 1_000_000_000) / freq
}

/// ticks→ns 双路合流（快路优先，快路资格判失格即慢路）——`timer::
/// ticks_to_ns` 单源形态。boot 早期（init 前 mul==0）或 tick 超域自动落
/// 慢路（freq 由调用侧直读 CNTFRQ，不依赖 init 顺序）。
pub fn ticks_to_ns(ticks: u64, mul: u64, shift: u32, freq: u64) -> u64 {
    match ns_fast_path(ticks, mul, shift) {
        Some(v) => v,
        None => ns_slow_path(ticks, freq),
    }
}

/// S-6 deadline 单调夹取判定（arm_slice 资格判）：`cur > now ∧ cur < want`
/// 时**保留已编程截止**（返回 `None` = 不写；块驱动快速切换的 deadline
/// 跑步机——CVAL 被无限前推永不到期 ⇒ tick 饿死核——的楔死防御，判例
/// 见 timer.rs 模块头 S-6）。否则（已过期 `cur ≤ now`，或 `cur` 比整片
/// 更远——如 park 遗留的 SLEEPQ 远截止）返回 `Some(want)` 全新武装。
///
/// 溢出界：`want = now + SLICE` 由调用侧计算——CNTFRQ ≤ 1GHz ∧
/// SLICE_HZ ≥ 1 ⇒ SLICE < 2^30；now 现实域 < 2^52 ⇒ want < 2^53 无溢。
pub fn arm_slice_clamp(now: u64, cur: u64, want: u64) -> Option<u64> {
    if cur > now && cur < want {
        None
    } else {
        Some(want)
    }
}

/// 授片臂收紧律（arm_slice 收紧刀 2026-11-22·深度优化）：`arm_slice_cap`
/// 的片长帽——`grant ∈ (0, slice)` 时收紧到 `grant`（预算片在耗点精确
/// 起爆），否则回落满片 `slice`（`grant == 0` 防御回落——`want == now`
/// 立即起爆面不触；`grant >= slice` 同值退化 = 满片臂逐位零漂移）。纯值
/// 律：`slice` 由调用侧给（freq 换算单源 timer.rs `init` 的 SLICE——本
/// crate 零 freq 知识，与 [`slice_budget`] 分工同款）。
pub fn slice_cap(grant: u64, slice: u64) -> u64 {
    if grant > 0 && grant < slice {
        grant
    } else {
        slice
    }
}

/// slice 预算再装数学：`freq / SLICE_HZ`（10Hz → 6.25M cycles @62.5MHz）。
/// freq==0 → 0（防御——init 前调用者拿 0 片，方向安全）。截尾不四舍五入
/// （与旧 `SLICE = FREQ / SLICE_HZ` 逐位一致）。
pub fn slice_budget(freq: u64) -> u64 {
    if freq == 0 {
        return 0;
    }
    freq / SLICE_HZ
}

/// LAST_VCT 回绕窗增量（tick 记账 delta）：`last == 0` 首读零增量（boot
/// 后第一 tick 前无基线——delta=0 不扣 slice）；否则
/// `now.wrapping_sub(last)`——**u64 回绕窗防御**：CNTVCT 单调递增至 2^64
/// 自然回绕（@62.5MHz 需 ~9,223 年，真机不受击；数学面回绕差 = 真增量
/// mod 2^64，回绕前后 delta 记账连续不断流）。`last == 0` 是「未初始化
/// 哨兵」而非「时刻 0」——首读建基线免计费。
pub fn vct_delta(now: u64, last: u64) -> u64 {
    if last == 0 {
        0
    } else {
        now.wrapping_sub(last)
    }
}

// ---- 编译期钉板（const 求值，零运行时成本）----

const _: () = {
    // QEMU virt CNTFRQ=62.5MHz 的倒数对 = (2048,7)（16ns/tick 二进精准
    // ——误差恰 0）。
    let (mul, shift) = ns_reciprocal_for(62_500_000);
    assert!(mul == 2048 && shift == 7, "QEMU virt reciprocal pair");
    // 域内快慢两路逐位一致（密集 0..4096 + 对数抽样至域界 2^52−1，乘积
    // 全程无溢即顺带实证）；域界本身恰失格。Option 比较在 const 语境不
    // 稳定——match 展开（None 视为失配）。
    const FQ: u64 = 62_500_000;
    let mut t: u64 = 0;
    while t < 4096 {
        let e = ns_slow_path(t, FQ);
        match ns_fast_path(t, mul, shift) {
            Some(v) => assert!(v == e, "dense equivalence"),
            None => assert!(false, "dense: fast path None"),
        }
        t += 1;
    }
    t = 1;
    while t < NS_FAST_DOMAIN_TICKS {
        let e = ns_slow_path(t, FQ);
        match ns_fast_path(t, mul, shift) {
            Some(v) => assert!(v == e, "log-sweep equivalence"),
            None => assert!(false, "log-sweep: fast path None"),
        }
        t = t * 2 + 12345;
    }
    match ns_fast_path(NS_FAST_DOMAIN_TICKS, mul, shift) {
        Some(_) => assert!(false, "domain edge must fail fast path"),
        None => {}
    }
    // 24MHz 非二进频率钉 (2667,6) 且每 tick 误差 ≤ 0.5ns。
    let (m24, s24) = ns_reciprocal_for(24_000_000);
    assert!(m24 == 2667 && s24 == 6, "24MHz reciprocal pair");
    let lhs = m24 * 24_000_000;
    let rhs = 1_000_000_000u64 << s24;
    let d = if lhs > rhs { lhs - rhs } else { rhs - lhs };
    assert!(2 * d <= 24_000_000, "24MHz per-tick error <= 0.5ns");
    // 异常频率防御面 → (0,0)。
    let (m0, s0) = ns_reciprocal_for(0);
    let (m2g, s2g) = ns_reciprocal_for(2_000_000_000);
    assert!(m0 == 0 && s0 == 0 && m2g == 0 && s2g == 0, "abnormal freq guard");
};

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ns_reciprocal_for：乘-移对构造 ----

    #[test]
    fn qemu_virt_reciprocal_pair() {
        // QEMU virt 62.5MHz：16ns/tick 二进精准——(2048,7) 且误差恰 0。
        let (mul, shift) = ns_reciprocal_for(62_500_000);
        assert_eq!((mul, shift), (2048, 7));
        // mul·freq 与 1e9·2^shift 恰等（二进精准的代数定义）。
        assert_eq!(mul * 62_500_000, 1_000_000_000u64 << shift);
    }

    #[test]
    fn reciprocal_24mhz_rounding() {
        // 24MHz 非二进频率：(2667,6) 且每 tick 误差 ≤ 0.5ns。
        let (m, s) = ns_reciprocal_for(24_000_000);
        assert_eq!((m, s), (2667, 6));
        let lhs = m * 24_000_000;
        let rhs = 1_000_000_000u64 << s;
        let d = if lhs > rhs { lhs - rhs } else { rhs - lhs };
        assert!(2 * d <= 24_000_000, "per-tick error <= 0.5ns");
    }

    #[test]
    fn reciprocal_abnormal_freqs_guard() {
        // freq==0（读失败面）与 >1GHz（ns/tick < 1 防御面）→ (0,0)。
        assert_eq!(ns_reciprocal_for(0), (0, 0));
        assert_eq!(ns_reciprocal_for(2_000_000_000), (0, 0));
        assert_eq!(ns_reciprocal_for(u64::MAX), (0, 0));
        // 域内边界恰过：1GHz 恰过（1ns/tick 整）、1GHz+1 恒拒。
        assert_ne!(ns_reciprocal_for(1_000_000_000), (0, 0));
        assert_eq!(ns_reciprocal_for(1_000_000_001), (0, 0));
    }

    #[test]
    fn reciprocal_sub244hz_rejects() {
        // 频率 <244Hz：round(1e9/f) ≥ CAP=4096——快路给不出精度，弃。
        // 244Hz 恰界：round(1e9/244)=4098360 ≥ CAP 拒；243Hz 同拒。
        assert_eq!(ns_reciprocal_for(244), (0, 0));
        assert_eq!(ns_reciprocal_for(243), (0, 0));
        assert_eq!(ns_reciprocal_for(1), (0, 0));
    }

    #[test]
    fn reciprocal_min_acceptable_freq() {
        // 恰可接受的下界：循环判据 2^s·2e9 < f·(2·CAP−1) 首步即
        // f·8191 > 2e9 ⇒ f ≥ 244_171（244_170 恰拒/244_171 恰过，mul=4095
        // = CAP−1 恰容）。
        let (m, s) = ns_reciprocal_for(244_171);
        assert_eq!((m, s), (4095, 0));
        assert_eq!(ns_reciprocal_for(244_170), (0, 0));
    }

    #[test]
    fn reciprocal_max_shift_optimal() {
        // 最大 shift 判据：对 62.5MHz，shift=7 时 mul=2048 < CAP=4096；
        // shift=8 会给 mul=4096 ≥ CAP——判据选 7（精度最优且无溢）。
        let (m, s) = ns_reciprocal_for(62_500_000);
        assert_eq!(s, 7);
        assert!(m < NS_MUL_CAP);
        // 直接验证 shift+1 不可行：round(2^8·1e9/62.5e6) = 4096 ≥ CAP。
        assert_eq!((1u64 << 8) * 1_000_000_000 / 62_500_000, 4096);
        assert_eq!(NS_MUL_CAP, 4096);
    }

    #[test]
    fn reciprocal_1mhz_binary_exact() {
        // 低频 1MHz：1000ns/tick；判据下最大可行 shift=2（shift=3 会给
        // mul=8000 ≥ CAP=4096 被拒）→ mul=4000·1e6 = 4e9 = 1e9·2^2 恰等
        // ——二进精准。
        let (m, s) = ns_reciprocal_for(1_000_000);
        assert_eq!((m, s), (4000, 2));
        assert_eq!(m * 1_000_000, 1_000_000_000u64 << s);
        // shift=3 被拒的直证：round(2^3·1e9/1e6) = 8000 ≥ CAP。
        assert_eq!((1u64 << 3) * 1_000_000_000 / 1_000_000, 8000);
        assert!(8000 >= NS_MUL_CAP);
    }

    // ---- ns_fast_path / ns_slow_path / ticks_to_ns 双路 ----

    #[test]
    fn fast_path_qemu_dense_equivalence() {
        // QEMU virt 域内：快慢两路逐位一致（密集 0..4096）。
        let (mul, shift) = ns_reciprocal_for(62_500_000);
        for t in 0..4096u64 {
            assert_eq!(
                ns_fast_path(t, mul, shift),
                Some(ns_slow_path(t, 62_500_000)),
                "t={t}"
            );
        }
    }

    #[test]
    fn fast_path_log_sweep_to_domain_edge() {
        // 对数抽样至域界：两路逐位一致（乘积全程无溢顺带实证）。
        let (mul, shift) = ns_reciprocal_for(62_500_000);
        let mut t: u64 = 1;
        while t < NS_FAST_DOMAIN_TICKS {
            assert_eq!(
                ns_fast_path(t, mul, shift),
                Some(ns_slow_path(t, 62_500_000)),
                "t={t}"
            );
            t = t * 2 + 12345;
        }
        // 域界本身：tick == 2^52 恰失格（严格小于才快路）→ 慢路承接。
        assert!(ns_fast_path(NS_FAST_DOMAIN_TICKS, mul, shift).is_none());
        assert_eq!(
            ticks_to_ns(NS_FAST_DOMAIN_TICKS, mul, shift, 62_500_000),
            ns_slow_path(NS_FAST_DOMAIN_TICKS, 62_500_000)
        );
    }

    #[test]
    fn fast_path_uninitialized_mul_sentinel() {
        // mul==0（init 前 / 频率异常）恒失格——慢路唯一可达。
        assert!(ns_fast_path(0, 0, 0).is_none());
        assert!(ns_fast_path(1, 0, 7).is_none());
        assert!(ns_fast_path(12345, 0, 0).is_none());
    }

    #[test]
    fn slow_path_zero_freq_contract() {
        // freq==0 → 0（旧契约：帽不触发，方向安全）。
        assert_eq!(ns_slow_path(12345, 0), 0);
        assert_eq!(ticks_to_ns(12345, 0, 0, 0), 0);
    }

    #[test]
    fn slow_path_quot_remainder_form() {
        // 先商后余形式钉板：手算三点（62.5MHz）。
        let f = 62_500_000u64;
        // t=f 恰一秒：1e9 ns。
        assert_eq!(ns_slow_path(f, f), 1_000_000_000);
        // t=f/2 半秒：5e8 ns。
        assert_eq!(ns_slow_path(f / 2, f), 500_000_000);
        // t=3·f+12345：商段 3e9 + 余段 12345·1e9/62.5e6=197520——两段
        // 拼合即总值（先商后余防中间溢出的形式验证）。
        let t = 3 * f + 12345;
        assert_eq!(ns_slow_path(t, f), 3_000_000_000 + 12345 * 1_000_000_000 / f);
    }

    #[test]
    fn ticks_to_ns_dual_path_merge() {
        // 双路合流：mul=0（init 前）→ 恒慢路；域内 → 快路值；域外 → 慢路。
        let (mul, shift) = ns_reciprocal_for(62_500_000);
        assert_eq!(
            ticks_to_ns(1000, 0, 0, 62_500_000),
            ns_slow_path(1000, 62_500_000)
        );
        assert_eq!(ticks_to_ns(1000, mul, shift, 62_500_000), (1000 * mul) >> shift);
        // 域外（tick ≥ 2^52，快路失格）→ 慢路承接。用域界 4 倍量级值
        // （域外但慢路合同域内——u64·1e9 乘积无溢：4·2^52 < 2^54，
        // 商 = 4·2^52/62.5e6 ≈ 2.9e8，商·1e9 ≈ 2.9e17 < 2^64）。
        let t = NS_FAST_DOMAIN_TICKS * 4;
        assert_eq!(
            ticks_to_ns(t, mul, shift, 62_500_000),
            ns_slow_path(t, 62_500_000)
        );
    }

    // ---- arm_slice_clamp：S-6 单调夹取律 ----

    #[test]
    fn clamp_keeps_earlier_deadline() {
        // `now < cur < want` 开区间：保留 cur（None = 不写）。
        assert_eq!(arm_slice_clamp(1000, 1500, 2000), None);
        assert_eq!(arm_slice_clamp(1000, 1001, 2000), None);
        assert_eq!(arm_slice_clamp(1000, 1999, 2000), None);
    }

    #[test]
    fn clamp_rewrites_expired_deadline() {
        // `cur ≤ now`（已过期——tick 入口常态：CVAL 刚过期）：全新武装。
        assert_eq!(arm_slice_clamp(1000, 1000, 2000), Some(2000));
        assert_eq!(arm_slice_clamp(1000, 999, 2000), Some(2000));
        assert_eq!(arm_slice_clamp(1000, 0, 2000), Some(2000));
    }

    #[test]
    fn clamp_rewrites_far_deadline() {
        // `cur ≥ want`（比整片更远——park 遗留 SLEEPQ 远截止）：全新武装。
        assert_eq!(arm_slice_clamp(1000, 2000, 2000), Some(2000));
        assert_eq!(arm_slice_clamp(1000, 5000, 2000), Some(2000));
    }

    #[test]
    fn clamp_boundary_geometry() {
        // 三点几何钉板：now=0/want=SLICE=6_250_000（QEMU virt 实尺）。
        let now = 0u64;
        let want = 6_250_000u64;
        assert_eq!(arm_slice_clamp(now, 0, want), Some(want));
        assert_eq!(arm_slice_clamp(now, want, want), Some(want));
        assert_eq!(arm_slice_clamp(now, 1, want), None);
        assert_eq!(arm_slice_clamp(now, want - 1, want), None);
    }

    // ---- slice_cap：授片臂收紧律（arm_slice 收紧刀 2026-11-22）----

    #[test]
    fn slice_cap_three_cases() {
        // 三例钉板（slice=QEMU virt 实尺 6.25M cycles=100ms；S5-c 探针
        // budget=2ms=125000 cycles 形）。
        let slice = slice_budget(62_500_000);
        // ① budget < slice → 收紧到 budget（预算片精确起爆）。
        assert_eq!(slice_cap(125_000, slice), 125_000);
        assert_eq!(slice_cap(1, slice), 1);
        // ② budget >= slice → 回落满片（同值退化 = arm_slice 零漂移）。
        assert_eq!(slice_cap(slice, slice), slice);
        assert_eq!(slice_cap(slice + 1, slice), slice);
        assert_eq!(slice_cap(u64::MAX, slice), slice);
        // ③ budget == 0 → 回落满片（防御：want==now 立即起爆面不触）。
        assert_eq!(slice_cap(0, slice), slice);
    }

    #[test]
    fn slice_cap_pre_init_zero_slice() {
        // init 前 SLICE==0（timer::init 前调用的防御面）：恒回落 0——与旧
        // arm_slice 前置态（want=now+0）同形，不引入新行为。
        assert_eq!(slice_cap(0, 0), 0);
        assert_eq!(slice_cap(123, 0), 0);
    }

    // ---- slice_budget：预算再装数学 ----

    #[test]
    fn slice_budget_qemu_virt() {
        // 62.5MHz / 10Hz = 6.25M cycles。
        assert_eq!(slice_budget(62_500_000), 6_250_000);
    }

    #[test]
    fn slice_budget_zero_freq_guard() {
        assert_eq!(slice_budget(0), 0);
    }

    #[test]
    fn slice_budget_truncation_law() {
        // 非整除截尾律（不四舍五入；与旧 SLICE = FREQ / SLICE_HZ 逐位
        // 一致）。
        assert_eq!(slice_budget(25_000_003), 2_500_000);
        assert_eq!(slice_budget(25_000_009), 2_500_000);
        assert_eq!(slice_budget(24_000_000), 2_400_000);
        assert_eq!(slice_budget(9), 0);
    }

    // ---- vct_delta：LAST_VCT 回绕窗增量 ----

    #[test]
    fn vct_delta_first_read_zero() {
        // last==0 首读哨兵：零增量（boot 后第一 tick 建基线免计费）。
        assert_eq!(vct_delta(0, 0), 0);
        assert_eq!(vct_delta(12345, 0), 0);
        assert_eq!(vct_delta(u64::MAX, 0), 0);
    }

    #[test]
    fn vct_delta_monotonic_window() {
        // 单调窗（未回绕）：delta = now − last 直算。
        assert_eq!(vct_delta(1000, 900), 100);
        assert_eq!(vct_delta(6_250_000, 0 + 1), 6_249_999);
        // now == last：零增量（同 tick 两次记账面）。
        assert_eq!(vct_delta(500, 500), 0);
    }

    #[test]
    fn vct_delta_wraparound_window() {
        // u64 回绕窗：last=u64::MAX−100, now=99（回绕后）——真增量 200，
        // wrapping_sub 恰还原（mod 2^64 算术下连续不断流）。
        let last = u64::MAX - 100;
        let now = 99u64;
        assert_eq!(vct_delta(now, last), 200);
        // 回绕点本身：MAX → 0 增量 1。
        assert_eq!(vct_delta(0, u64::MAX), 1);
    }

    #[test]
    fn vct_delta_feeds_slice_accounting() {
        // 端到端形式：delta 喂 tick_account（berry-sliceface）的入参域
        // —— QEMU virt 一片 6.25M cycles 内的 delta 值即切片扣减量。
        let f = 62_500_000u64;
        let slice = slice_budget(f);
        // 相邻 tick 间隔 = slice（整片耗尽形态）。
        assert_eq!(vct_delta(slice, 0 + 1), slice - 1);
        // 早醒（半片）：delta = slice/2。
        assert_eq!(vct_delta(1 + slice / 2, 1), slice / 2);
    }
}
