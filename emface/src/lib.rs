//! berry-emface —— EM 表形状执法 + util EWMA 窗口数学纯函数面（W3-Q14）。
//!
//! **动机**：调点调度 v1.3 的 EM（Energy Model）表形状执法（`调点调度-v1.3.md`
//! §7 预研建议②「EM 表落形状不落算法」）与 per-CPU utilization EWMA 窗口
//! 数学（§6「簇 util 通路」/§7 建议④「频率归 PM」）此前只活在
//! `sched/src/em.rs` 与 `sched/src/util.rs`——sched 依赖 HAL global_asm，
//! 只 aarch64-unknown-none 可编，host 测不可达，ktest 只覆盖 boot 相。
//! EM 拒收判据（严格升序/容量上限/掩码门）与 EWMA 折叠律（α=1/8 移位、
//! elapsed 分母、饱和钳位）的回归面需要一张 host 可测的**钉板**：本 crate
//! 把这些数学按值域抽成纯函数 + 表驱动测试，`sched` 侧消费同一函数（零第
//! 二实现——抽离即唯一事实源，非镜像；wqmatch/sliceface 同款纪律）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`EmPoint`]/[`EmDomain`]/[`EM_POINTS_MAX`]/[`EM_DOMAINS_MAX`]：EM 表
//!   形状数据结构单源（自 sched/src/em.rs 迁此，sched `pub use` re-export）。
//! - [`em_shape_ok`]:注册形状执法——域号在册、点数 1..=上限、cpu_mask
//!   非零、频率**严格升序**（等频/降序同罪拒收）。拒收 = 零写入语义在
//!   调用侧（宿主表不动），本函数只判。
//! - [`em_domain_from`]:点表 → [`EmDomain`] 装配（拒收输入 → None——
//!   `npoints`/尾槽零填充不变量入口收口）。
//! - [`em_max_capacity_of`]/[`em_min_power_of`]:域快照的极值读面（升序
//!   不变量下尾点 = 最高容量/首点 = 最低功耗；空域 → 0）。
//! - [`EWMA_ALPHA_SHIFT`]/[`UTIL_WINDOW_HZ`]/[`UTIL_NR_CPU`]:EWMA 常量
//!   单源（自 sched/src/util.rs 迁此）。
//! - [`window_state`]:翻窗资格三态判——首调/回绕开新窗、未到期零动作、
//!   到期折叠（elapsed 分母 = 实际跨度非名义窗长——判序单源）。
//! - [`ewma_fold`]:窗口折叠一步——`util_raw = min(accum×1024/elapsed,
//!   1024)` + `ewma ± diff>>3`（α=1/8 移位零除法，差 <8 量化死区）。
//! - [`OVERUTIL_THRESHOLD`]/[`overutil_judgment`]:overutilized 阈值律——
//!   `util > cap×819/1024`（严格 >，EM 缺席钳 1 → 保守回退）。
//!
//! 与冻结面的关系：EM 全局表（`static mut EM_TABLE` + 原子发布序）与
//! per-CPU EWMA 槽是**裸并发机械**，留 sched（`&raw mut`/Relaxed 原子纪律
//! 不变）——本面只钉「形状怎么判/窗口怎么折」的数定律，不钉存储机械。
//! HAL 时基（`cntfrq`）以参数注入——纯面无时基依赖。

#![cfg_attr(not(test), no_std)]
#![allow(clippy::result_unit_err)]

// ---------------------------------------------------------------------------
// EM 表形状（自 sched/src/em.rs 单源迁此——sched 侧 re-export 后零第二实现）
// ---------------------------------------------------------------------------

/// 每性能域（= 同频簇）的能量模型条目。Day-one：只落形状（每簇 2-3 个
/// {freq, power} 点）——算法（compute_energy/FEuC）留 M2/真机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmPoint {
    /// 频率（kHz——与 Linux EM 同口径）。
    pub freq_khz: u32,
    /// 功率（mW——该频点稳态功耗）。
    pub power_mw: u32,
    /// 容量（该频点的算力基准——DMIPS/MHz × freq / 1024 缩放；与
    /// `Topo::capacity` 字段同口径，1024 = 域内最高频点满格）。
    pub capacity: u32,
}

impl EmPoint {
    /// 零点（表尾无效槽填充位）。
    pub const ZERO: EmPoint = EmPoint { freq_khz: 0, power_mw: 0, capacity: 0 };
}

/// 每性能域的 EM 表频点数上限（OPP 表常见 3-5 点，取 4）。
pub const EM_POINTS_MAX: usize = 4;

/// 每性能域的 EM 表（点表按频率**严格升序**——入口 [`em_shape_ok`] 收口，
/// 尾点即最高频点、首点即最低频点）。PartialEq = ktest 形状逐位对拍面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmDomain {
    /// 域内 CPU 掩码（bit i = CPU i 属于此域）。
    pub cpu_mask: u32,
    /// EM 点表（按频率升序；有效前缀 = `[0..npoints]`，尾槽恒零填充）。
    pub points: [EmPoint; EM_POINTS_MAX],
    /// 有效点数（1..=EM_POINTS_MAX）。
    pub npoints: usize,
}

impl EmDomain {
    /// 空域（表初值；拒收路径不动已注册内容，无需「空化」写面）。
    pub const EMPTY: EmDomain = EmDomain {
        cpu_mask: 0,
        points: [EmPoint::ZERO; EM_POINTS_MAX],
        npoints: 0,
    };
}

/// 全局 EM 表域数上限（每域一条——QEMU virt 单域，RPi4 单域 4 核同构：
/// 1 用 1 备，真机大小核 = 2 域即满配）。
pub const EM_DOMAINS_MAX: usize = 2;

/// 表体形状判（私有芯——`em_shape_ok`/`em_domain_from` 共用，判据单源）：
/// 点数 1..=上限 + 频率严格升序（`w[0] >= w[1]` 即拒——等频/降序同罪）。
fn points_ok(points: &[EmPoint]) -> bool {
    if points.is_empty() || points.len() > EM_POINTS_MAX {
        return false;
    }
    for w in points.windows(2) {
        if w[0].freq_khz >= w[1].freq_khz {
            return false; // 非严格升序拒收（等频/降序同罪）
        }
    }
    true
}

/// 注册形状执法（`em_register` 拒收判据单源）：域号在册（`domain <
/// EM_DOMAINS_MAX`）、点数 1..=上限、cpu_mask 非零、频率**严格升序**。
///
/// 拒收 = 零写入的语义在调用侧（宿主表不动）；本函数纯判零副作用。
#[inline]
pub fn em_shape_ok(domain: usize, cpu_mask: u32, points: &[EmPoint]) -> bool {
    domain < EM_DOMAINS_MAX && cpu_mask != 0 && points_ok(points)
}

/// 点表 → 域表装配（`em_register` 受理路径单源）：形状合格才装配——
/// 拷贝有效前缀 + 尾槽保持零填充 + `npoints` 记数。拒收输入（掩码零/
/// 表体形状坏）→ None（调用侧零写入）。域号界门不在本函数（装配无域号
/// ——入座在调用侧的 `domain < EM_DOMAINS_MAX` 门后）。
#[inline]
pub fn em_domain_from(cpu_mask: u32, points: &[EmPoint]) -> Option<EmDomain> {
    if cpu_mask == 0 || !points_ok(points) {
        return None;
    }
    let mut d = EmDomain::EMPTY;
    d.cpu_mask = cpu_mask;
    d.points[..points.len()].copy_from_slice(points);
    d.npoints = points.len();
    Some(d)
}

/// 域快照最高频点容量（overutilized 阈值/选核的容量基准——升序不变量下
/// 尾点即最高频点；空域/未注册 → 0）。
#[inline]
pub fn em_max_capacity_of(d: &EmDomain) -> u32 {
    if d.npoints == 0 {
        return 0;
    }
    d.points[d.npoints - 1].capacity // npoints≥1 注册面保证
}

/// 域快照最低频点功耗（域最低能耗/空闲基准——首点即最低频点；空域/
/// 未注册 → 0）。
#[inline]
pub fn em_min_power_of(d: &EmDomain) -> u32 {
    if d.npoints == 0 {
        return 0;
    }
    d.points[0].power_mw
}

// ---------------------------------------------------------------------------
// util EWMA 窗口数学（自 sched/src/util.rs 单源迁此）
// ---------------------------------------------------------------------------

/// EWMA 平滑系数 α = 1/8（`>>`[`EWMA_ALPHA_SHIFT`] 移位实现——no_std 零
/// 浮点零除法；半衰期 ≈ 5.2 窗）。α 越大跟得越紧、噪声越大。
pub const EWMA_ALPHA_SHIFT: u32 = 3;

/// util 采样窗口频率（100Hz → ~10ms）。窗长 = 名义翻窗**下限**（防密集
/// tick——SLEEPQ deadline 醒等早 tick——过折叠）；实际采样跨度见
/// [`window_state`] 的 elapsed 分母注。
pub const UTIL_WINDOW_HZ: u64 = 100;

/// 窗长参考锚（QEMU virt 62.5MHz × 10ms = 625_000 VCT cycles）——仅文档
/// 值，频率**不硬编码**（`timer.rs` 头注纪律）：真窗长运行时按 CNTFRQ
/// 换算（`cntfrq / UTIL_WINDOW_HZ`——sched 侧 `util_window_vct`）。
pub const UTIL_WINDOW_VCT_REF: u64 = 625_000;

/// per-CPU 槽数（v1 = 2 核——同 `Topo::capacity` / `PER_CPU_RQ` 定长口径）。
pub const UTIL_NR_CPU: usize = 2;

/// 翻窗资格三态（`util_tick_window` 判序单源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinState {
    /// 首调/回绕：重开窗并**丢弃既往 accum**（半窗账无窗长可比——记进
    /// 首个整窗会虚高首样）。
    NewWindow,
    /// 未到期：零动作（窗长常量只做最短翻窗间隔——防密集 tick 过折叠）。
    NotDue,
    /// 到期折叠：`(本窗累计 cycles, 实际跨度 cycles)`——elapsed 分母用
    /// **实际 elapsed**（自上次翻窗）非名义窗长：tick 间隔 > 窗长
    /// （SLICE 100ms > 窗 10ms）时采样跨度 = 上次翻窗至今，占空比诚实；
    /// 深睡长 gap 后首窗 elapsed 摊大 → idle 自动稀释为低 util 样本。
    Fold { accum: u64, elapsed: u64 },
}

/// 翻窗资格判（见 [`WinState`]）。`start == 0` = 未开窗哨兵；`window`
/// = 名义窗长（`cntfrq / UTIL_WINDOW_HZ`，注入纯面——无时基依赖）。
///
/// 判序：`start == 0 || now < start`（首调/计数器回绕防御，正常不可达）
/// → `NewWindow` 优先于到期判——回绕窗不折叠（重开）。
#[inline]
pub fn window_state(start: u64, now: u64, window: u64, accum: u64) -> WinState {
    if start == 0 || now < start {
        return WinState::NewWindow;
    }
    let elapsed = now - start;
    if elapsed < window {
        return WinState::NotDue;
    }
    WinState::Fold { accum, elapsed }
}

/// 窗口折叠一步（`util_tick_window` 折叠数学单源）：返回新 EWMA。
///
/// - `util_raw = min(accum × 1024 / elapsed, 1024)`（u128 分子零溢出；
///   饱和钳位 1024 = 满格——超载账不破界）；
/// - `ewma' = ewma ± |util_raw - ewma| >> 3`（α = 1/8 移位零除法；差 <8
///   折叠 0——量化死区 ≤ 7/1024 ≈ 0.7%，对频率信号无害）。
///
/// 前置：`elapsed ≥ 1`（[`window_state`] 的 `Fold` 门保证 elapsed ≥
/// window ≥ 1——直传 0 = 除零 panic，调用序契约）。
#[inline]
pub fn ewma_fold(accum: u64, elapsed: u64, ewma: u32) -> u32 {
    let util_raw = ((accum as u128 * 1024) / (elapsed as u128)).min(1024) as u32;
    if util_raw >= ewma {
        ewma + ((util_raw - ewma) >> EWMA_ALPHA_SHIFT)
    } else {
        ewma - ((ewma - util_raw) >> EWMA_ALPHA_SHIFT)
    }
}

// ---------------------------------------------------------------------------
// overutilized 阈值律（§7 预研建议⑤「overutilized 回退」——Linux EAS 同名
// 机制的 BerryOS 最小面：任一核 util 超域最高容量 80% → 整个调度域 EAS
// 失效退化纯负载均衡。判据纯函数单源，sched 侧 overutilized() 消费。）
// ---------------------------------------------------------------------------

/// overutilized 判定阈值（0..=1024 尺度——80%）：1024 × 0.8 = 819.2 →
/// 819（整数截断；Linux `sched_capacity_margin` 的 80% 同款口径）。
pub const OVERUTIL_THRESHOLD: u32 = 819;

/// overutilized 判据（`overutilized` 核心比较单源）：`util > cap × 819 /
/// 1024`（严格大于——恰在阈值上不算，滞回带留 §7 裁决后续接）。
///
/// `cap` 传 `em_max_capacity(domain).max(1)`（EM 未注册 → capacity 0 钳
/// 1 → 阈值 0）时任何非零 util 即 over——保守侧正确：无能量模型则 EAS
/// 无据可依，本就该回退普通均衡。本函数收口 `max(1)` 钳位（调用侧直传
/// 原容量亦可——钳位单源在此）。
#[inline]
pub fn overutil_judgment(util: u32, cap: u32) -> bool {
    // u64 中间积：cap 域可达 u32::MAX（EM capacity 是 u32），u32 直乘
    // 819 会溢出 panic；cap ≤ 1024 的现役域与旧式 `cap*819/1024` 逐位
    // 一致（u64 域内同商）。
    (util as u64) > (cap.max(1) as u64 * OVERUTIL_THRESHOLD as u64) / 1024
}

// ---------------------------------------------------------------------------
// 表驱动钉板（host 测——sched 侧同款判据此前 host 不可达）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(freq: u32, power: u32, cap: u32) -> EmPoint {
        EmPoint { freq_khz: freq, power_mw: power, capacity: cap }
    }

    /// EM 形状执法全表：受理面 × 拒收面（ktest EM-2 同款判据的 host 钉板）。
    #[test]
    fn em_shape_table() {
        let asc = [pt(300_000, 8, 500), pt(800_000, 30, 850)];
        let four: [EmPoint; EM_POINTS_MAX] = [
            pt(100_000, 1, 100),
            pt(200_000, 2, 200),
            pt(300_000, 4, 300),
            pt(400_000, 8, 400),
        ];
        // 受理面：在册域 × 非零掩码 × 1..=4 点 × 严格升序。
        assert!(em_shape_ok(0, 0b11, &asc));
        assert!(em_shape_ok(1, 0b100, &asc));
        assert!(em_shape_ok(0, 1, &[pt(100_000, 1, 100)])); // 单点表
        assert!(em_shape_ok(EM_DOMAINS_MAX - 1, 0b11, &four)); // 恰满 4 点
        // 拒收面：越界域（== 上限即越界）。
        assert!(!em_shape_ok(EM_DOMAINS_MAX, 0b11, &asc));
        assert!(!em_shape_ok(usize::MAX, 0b11, &asc));
        // 空点表 / 超上限。
        assert!(!em_shape_ok(0, 0b11, &[]));
        let five: [EmPoint; EM_POINTS_MAX + 1] = [
            pt(100_000, 1, 100),
            pt(200_000, 2, 200),
            pt(300_000, 4, 300),
            pt(400_000, 8, 400),
            pt(500_000, 16, 500),
        ];
        assert!(!em_shape_ok(0, 0b11, &five));
        // 零掩码。
        assert!(!em_shape_ok(0, 0, &asc));
        // 非严格升序：降序 / 等频。
        assert!(!em_shape_ok(0, 0b11, &[asc[1], asc[0]]));
        assert!(!em_shape_ok(0, 0b11, &[asc[0], asc[0]]));
    }

    /// 域装配：有效前缀 + 尾槽零填充 + npoints 不变量 + 与形状判据一致。
    #[test]
    fn em_assemble() {
        let asc = [pt(300_000, 8, 500), pt(800_000, 30, 850)];
        let d = em_domain_from(0b100, &asc).expect("两点升序表装配须 Some");
        assert_eq!(d.cpu_mask, 0b100);
        assert_eq!(d.npoints, 2);
        assert_eq!(&d.points[..2], &asc);
        assert_eq!(d.points[2], EmPoint::ZERO, "尾槽须零填充");
        assert_eq!(d.points[3], EmPoint::ZERO, "尾槽须零填充");
        // 恰满表：无尾槽（npoints == EM_POINTS_MAX）。
        let four: [EmPoint; EM_POINTS_MAX] = [
            pt(100_000, 1, 100),
            pt(200_000, 2, 200),
            pt(300_000, 4, 300),
            pt(400_000, 8, 400),
        ];
        let d4 = em_domain_from(0b1, &four).expect("恰满 4 点须 Some");
        assert_eq!(d4.npoints, EM_POINTS_MAX);
        // 拒收 → None（零装配）：空表 / 超长 / 非升序 / 零掩码。
        assert!(em_domain_from(0b11, &[]).is_none());
        let five: [EmPoint; EM_POINTS_MAX + 1] = [
            pt(100_000, 1, 100),
            pt(200_000, 2, 200),
            pt(300_000, 4, 300),
            pt(400_000, 8, 400),
            pt(500_000, 16, 500),
        ];
        assert!(em_domain_from(0b11, &five).is_none());
        assert!(em_domain_from(0b11, &[pt(5, 1, 1), pt(5, 1, 1)]).is_none());
        assert!(em_domain_from(0b11, &[pt(9, 1, 1), pt(5, 1, 1)]).is_none());
        assert!(em_domain_from(0, &asc).is_none());
        // 形状执法与装配判据一致性：在册域 + 非零掩码下两判据同真同假
        //（表体判据单源的对拍面——points_ok 芯共用，此测防未来分叉）。
        let tables: [&[EmPoint]; 6] = [
            &[],
            &asc,
            &five,
            &[asc[1], asc[0]],
            &[asc[0], asc[0]],
            &four,
        ];
        for t in tables {
            assert_eq!(
                em_shape_ok(0, 0b11, t),
                em_domain_from(0b11, t).is_some(),
                "形状判据与装配判据须一致（表 {t:?}）"
            );
        }
    }

    /// 极值读面：升序不变量下的首尾投影 + 空域防御 + 容量/功耗可倒挂。
    #[test]
    fn em_extremes() {
        // QEMU virt 默认表形状（ktest EM-1 同款锚）。
        let qemu = em_domain_from(
            0b11,
            &[
                pt(250_000, 5, 400),
                pt(500_000, 15, 750),
                pt(1_000_000, 50, 1024),
            ],
        )
        .unwrap();
        assert_eq!(em_max_capacity_of(&qemu), 1024);
        assert_eq!(em_min_power_of(&qemu), 5);
        // 容量/功耗不要求单调——只有频率单调是形状不变量（构造倒挂表：
        // 最低频点反而容量高/功耗高——读面仍按首尾投影，不重排）。
        let mixed = em_domain_from(0b1, &[pt(100_000, 50, 850), pt(1_000_000, 5, 400)])
            .unwrap();
        assert_eq!(em_max_capacity_of(&mixed), 400, "尾点（最高频）容量");
        assert_eq!(em_min_power_of(&mixed), 50, "首点（最低频）功耗");
        // 空域 → 0（未注册读面防御）。
        assert_eq!(em_max_capacity_of(&EmDomain::EMPTY), 0);
        assert_eq!(em_min_power_of(&EmDomain::EMPTY), 0);
    }

    /// 翻窗资格三态：首调开窗 / 回绕防御 / 未到期 / 到期折叠（ktest
    /// UTIL-1 判序的 host 钉板）。
    #[test]
    fn window_states() {
        let w = UTIL_WINDOW_VCT_REF; // 625_000
        // 未开窗（start==0 哨兵）→ 新窗（丢既往 accum）。
        assert_eq!(window_state(0, 1_000, w, 999), WinState::NewWindow);
        assert_eq!(window_state(0, 0, w, 0), WinState::NewWindow);
        // 回绕防御（now < start，正常不可达）→ 重开。
        assert_eq!(window_state(1_000, 999, w, 5), WinState::NewWindow);
        assert_eq!(window_state(1_000, 0, w, 5), WinState::NewWindow);
        assert_eq!(window_state(5_000, 4_999, w, 100), WinState::NewWindow);
        // 未到期：elapsed < w（半窗 / 恰差一）。
        assert_eq!(window_state(1_000, 1_000 + w - 1, w, 7), WinState::NotDue);
        assert_eq!(window_state(1_000, 1_000 + w / 2, w, w / 2), WinState::NotDue);
        // 到期边界：elapsed == w 恰折叠（≥ 含等）。
        assert_eq!(
            window_state(1_000, 1_000 + w, w, w / 2),
            WinState::Fold { accum: w / 2, elapsed: w }
        );
        // 超窗（tick 间隔 > 窗长——SLICE 100ms > 窗 10ms 常态）：elapsed
        // = 实际跨度 10w（占空比诚实——分母非名义窗长）。
        assert_eq!(
            window_state(1_000, 1_000 + 10 * w, w, w),
            WinState::Fold { accum: w, elapsed: 10 * w }
        );
    }

    /// EWMA 折叠律：首折/收敛/衰减/死区/饱和钳位（ktest UTIL-2..6 同款
    /// 数值对拍——sched 侧判据的 host 钉板）。
    #[test]
    fn ewma_folding() {
        let w = UTIL_WINDOW_VCT_REF;
        // 满载首折：accum=elapsed → raw=1024 → 0 + 1024>>3 = 128。
        assert_eq!(ewma_fold(w, w, 0), 128);
        // 半载首折：raw=512 → 512>>3 = 64。
        assert_eq!(ewma_fold(w / 2, w, 0), 64);
        // idle 空窗：raw=0 → 64 - 64>>3 = 56（负差分支减法同式）。
        assert_eq!(ewma_fold(0, w, 64), 56);
        // 饱和钳位：巨量 accum 不破 1024（钳后同满载首折）。
        assert_eq!(ewma_fold(w.saturating_mul(1 << 40), w, 0), 128);
        // 满载收敛链：ewma_{n+1} = ewma_n + (1024-ewma_n)>>3 逐位精确
        //（ktest UTIL-3 8 窗 + OU 前置 12 折至 815 同款锚）。
        let mut e = 0u32;
        for i in 1..=12u32 {
            e = ewma_fold(w, w, e);
            assert!(e < 1024, "满载收敛链恒 <1024（第 {i} 折 {e}）");
        }
        assert_eq!(e, 815, "12 折满载须至 815（< 819.2 = 80%）");
        // 量化死区（差 <8 折 0）：elapsed=1024 纯数学面（raw = accum
        // 恰整除）——823-820=3>>3=0 不动；828-820=8>>3=1 前进；
        // 817-820=负差 3>>3=0 不动。
        assert_eq!(ewma_fold(823, 1024, 820), 820);
        assert_eq!(ewma_fold(828, 1024, 820), 821);
        assert_eq!(ewma_fold(817, 1024, 820), 820);
        // u128 分子防御：accum=u64::MAX 不 panic（积 2^74 域内）。
        assert_eq!(ewma_fold(u64::MAX, 1, 0), 128); // 钳位后同满载
        // ewma 恰 1024 满格：raw=1024 → 差 0 → 不动（顶稳定）。
        assert_eq!(ewma_fold(w, w, 1024), 1024);
    }

    /// overutilized 阈值律：80% 边界两侧 + EM 缺席回退 + 域容量缩放
    ///（ktest OU-1/OU-2 同款锚的 host 钉板）。
    #[test]
    fn overutil_law() {
        // 域最高容量 1024（QEMU 默认）→ 阈值 819：819 不判 / 820 判。
        assert!(!overutil_judgment(0, 1024));
        assert!(!overutil_judgment(819, 1024), "== 阈值不判（严格 >）");
        assert!(overutil_judgment(820, 1024), "820 > 819 首越阈");
        assert!(overutil_judgment(1024, 1024), "满格必判");
        // EM 缺席（cap=0 → 钳 1 → 阈值 0）：任何非零 util 即 over——保守
        // 回退正确（无 EM 则 EAS 无据可依，本就该回退普通均衡）。
        assert!(!overutil_judgment(0, 0));
        assert!(overutil_judgment(1, 0));
        // 域容量缩放：cap=850 → 阈值 850×819/1024 = 679（整除截断）。
        assert!(!overutil_judgment(679, 850), "== 缩放阈值不判");
        assert!(overutil_judgment(680, 850), "> 缩放阈值判");
        // cap 极值：u32::MAX 乘法不溢出（u32 域内 819×u32::MAX/1024）。
        assert!(!overutil_judgment(u32::MAX / 2, u32::MAX));
        // 严格 > 语义全域抽查：judgment(u, 1024) == (u > 819)。
        for u in [0u32, 1, 100, 511, 512, 700, 818, 819, 820, 1000, 1023, 1024] {
            assert_eq!(overutil_judgment(u, 1024), u > 819, "u={u}");
        }
    }
}
