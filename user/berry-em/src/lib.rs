//! berry-em —— EM 表第一张 + EAS 能量比较律最简形（CP-09 前置·host 纯面）。
//!
//! **任务锚**：调度彻底计划 CP-09（docs/调度彻底计划-v1.2.md:105——「EAS
//! 选核算法 compute_energy/FEuC …能量比较单源 host 钉板」；:70 裁四「判定
//! 律先 host 钉板后进内核」）。现状两源：EM **形状**已单源 berry-emface
//! （W3-Q14——emface/src/lib.rs:1-35，sched/src/em.rs:32 re-export 消费），
//! 而**能量算法**「留 M2/真机」（docs/调度器设计-v1.2.md:357「零算法」；
//! :356 QEMU virt 默认表）；01 号实测 EM「有产无消」（research/调度彻底
//! 计划/01-调度现状全景与在途线收编.md:144「util EWMA（产）/placement
//! hint（产）/EM 表（数据面）三件全部有产无消」）。本 crate 补**消**侧
//! 第一块纯面：容量查询 + 选核打分（EAS 式 energy_diff 最简形）+ QEMU
//! virt 第一张假想表。**首个内核消费面已接**（06 号评估终判「水通管道」
//! ②·2026-11-16）：sched `place()` → `eas_place` 消费 `eas_pick_cpu`
//! （placement hint 经 prev 入座；root workspace exclude 收编本件为
//! sched path 依赖——形态从「独立小工作区零消费」升「独立根 + 被依赖」，
//! 版本治理字面值不变）；选核决策遥测/overutil 回退执法余面归 CP-09。
//!
//! **形状单源声明**：EmPoint/EmDomain/EM_POINTS_MAX/EM_DOMAINS_MAX/
//! em_shape_ok/em_domain_from/em_max_capacity_of/em_min_power_of/
//! OVERUTIL_THRESHOLD/overutil_judgment 单源 = berry-emface
//! （emface/src/lib.rs:44-147/:229-244），本 crate 经 path 依赖单源消费
//! （零第二实现——berry-power 消费 emface 同例，user/bins/berry-power/
//! Cargo.toml [dependencies]）。不走 slackface 零依赖镜像路线：path 依赖
//! 可通时镜像 = 自造第二事实源。下方 re-export 仅为消费便利
//! （sched/src/em.rs:32 同例）。
//!
//! **Linux 对照**（外部事实，2026-11 web_fetch 取证
//! docs.kernel.org/power/energy-model.html 与
//! docs.kernel.org/scheduler/sched-energy.html）：
//! ① struct em_perf_state{performance, frequency, power, cost, flags}——
//! performance = 该频点 CPU 容量、frequency = kHz（与 CPUFreq 一致）、
//! power = 该级功耗（单 CPU 或设备口径，可含静态）；BerryOS
//! EmPoint{capacity, freq_khz, power_mw} 三件同构；cost 域不落——cost =
//! 10×power×fmax/freq 是内核 EAS 的系数化实现，本面用原始三点直接算，
//! 系数化留 CP-09 接线窗裁。
//! ② compute_energy 语义（sched-energy.rst §4 Example 2 逐数可复算）：
//! **域级 OPP 由域内 max_util 定**（em_pd_get_efficient_state——最低够用
//! 档，capacity ≥ max_util 即止），域能耗 = Σ 域内 CPU util/OPP 容量 ×
//! OPP 功率 = sum_util × power/capacity（「单域一频、按 util 线性摊能」；
//! em_cpu_energy(pd, max_util, sum_util) 同参数面）。
//! ③ 选核两段（find_energy_efficient_cpu）：每性能域取最高 spare
//! capacity 候选，再与 prev_cpu 比能量，**只在严格更省时迁移**。
//! ④ over-utilization 80% 门槛（sched-energy.rst §5；emface
//! OVERUTIL_THRESHOLD=819 同值同律——本面只引用不重复实现）。
//! ⑤ EAS 对 mW 与 abstract scale 等价（sched-energy.rst §6.2）——QEMU
//! 假想表 mW 口径合法。
//! ⑥ Linux EAS 不支持对称拓扑（sched-energy.rst §1/§6.1）——QEMU 对称
//! 双核表只验机制不验收益（sched/src/em.rs:99-101「TCG 对称核建模不了
//! 大小核」同口径）；kill criteria（调度彻底计划 K3「EAS placement 打
//! 不过零假设即撤」）压力真实在册。
//!
//! **底景观口径**（与 Linux 唤醒放置同构）：`utils` = **任务外底景**——
//! 被放置任务的 util 不在任何 CPU 账上（唤醒面任务不在任何 runqueue；
//! 其在 prev 的 PELT 残留由调用侧衰减口径处理，本面不双记账）。文档
//! Example 2 的底景含 P 于 CPU0（CPU0 400 含 P 的 200）——本面取
//! little 底景 [200,100]（= 400 − P），放置候选即得文档三案例分量，
//! 逐数对拍见测试 linux_example2_anchor。
//!
//! **QEMU virt 第一张假想表**（数值推断标注——TCG 无功耗计，机制验证用
//! 发明值非测量值；真机 EM = CP-18）：
//! - [`QEMU_VIRT_DUAL`]：双核对称（mask 0b11）——**镜像现役表**
//!   sched/src/em.rs:102-111（250MHz/5mW/容量 400 · 500MHz/15mW/容量
//!   750 · 1GHz/50mW/容量 1024）。同步义务：sched 侧改表须同步本 const
//!   （测试 qemu_values_pinned 逐位钉值，漂移即咬）。
//! - [`QEMU_VIRT_SINGLE`]：单核（mask 0b1）同三点——**假想**（推断：同
//!   A72 核，域容量与核数无关；power 单 CPU 口径——Linux em_perf_state
//!   power「by 1 CPU」同构，单双核同值合法；现役无单核注册面，随双核表
//!   同批镜像）。
//! - Linux Example 2 little/big 对照表：cfg(test) fixture（见测试模块）——
//!   容量/功耗取自 sched-energy.rst Example 2 原表（little 170/50 ·
//!   341/150 · 512/300，big 512/400 · 768/800 · 1024/1700），freq 列原表
//!   没有——**推断填充**（能量律只用容量/功耗，freq 仅为形状合格填充，
//!   数值无物理含义）。
//!
//! **能量律最简形**（本 crate 新增单源，全部纯函数、零分配、整数算术）：
//! - [`eff_opp_index`]：最低够用档（capacity ≥ util 即止；util 超顶钳顶
//!   档；空域 None）。
//! - [`capacity_for_util`]/[`spare_capacity`]：容量查询面（OPP 容量投影/
//!   1024 尺度余量——CP-09 选核与 CP-08 schedutil 同消费）。
//! - [`domain_energy_uw`]：域能耗 μW 整数（「单域一频、按 util 线性摊
//!   能」；u128 中间量饱和回 u64——零浮点）。
//! - [`energy_diff_uw`]：放置边际（候选 CPU 加任务前后域能耗差——EAS
//!   energy_diff 最简形；CP-09 选核打分即此）。
//! - [`eas_pick_cpu`]：两段选核最简形（每域最高 spare capacity 候选 ∪
//!   prev_cpu，最小边际能耗胜出，平局 prev 胜——「只在严格更省时迁移」；
//!   放不下的域不候选；全域无据 → None 回退负载均衡）。
//!
//! **口径纪律**：util 输入 0..=1024（与 EM capacity/Topo.capacity 同尺度
//! ——调用侧 EWMA 饱和钳 1024 后传入，emface ewma_fold 同源）；能耗 μW
//! 整数 floor（Linux 文档展示值为显示截断，本面逐位确定）；能量律单调的
//! 前提 = 点表功耗/容量随频升序（现役表全满足；emface 形状律只执法频升序
//! ——容量/功耗倒挂表的能量语义未裁，留 CP-09）；本面无 idle 能耗态
//! （Linux EM 亦只含活跃功耗，idle 归 cpuidle 另册——空载域按最低档活跃
//! 功耗记账，过度记账如实注）。

#![cfg_attr(not(test), no_std)]

// ---- 形状单源 re-export（berry-emface——消费便利，sched/src/em.rs:32 同例）----

pub use berry_emface::{
    em_domain_from, em_max_capacity_of, em_min_power_of, em_shape_ok, overutil_judgment,
    EmDomain, EmPoint, EM_DOMAINS_MAX, EM_POINTS_MAX, OVERUTIL_THRESHOLD,
};

// ---- QEMU virt 第一张假想表（数值推断标注——头注节）----

/// QEMU virt 双核对称表（mask 0b11，CPU0-1 同域）——**镜像现役表**
/// sched/src/em.rs:102-111（`em_register_qemu_virt` 注册同值；docs/调度器
/// 设计-v1.1.md:345 同锚）。同步义务：sched 侧改表须同步本 const。数值
/// 推断标注：三点为发明值——容量 400/750 非线性缩放（250M/500M/1G），
/// 功耗 5/15/50mW 超线性（P∝V²f 量级感），只验机制不验收益。
pub const QEMU_VIRT_DUAL: EmDomain = EmDomain {
    cpu_mask: 0b11,
    points: [
        EmPoint { freq_khz: 250_000, power_mw: 5, capacity: 400 },
        EmPoint { freq_khz: 500_000, power_mw: 15, capacity: 750 },
        EmPoint { freq_khz: 1_000_000, power_mw: 50, capacity: 1024 },
        EmPoint::ZERO,
    ],
    npoints: 3,
};

/// QEMU virt 单核表（mask 0b1，CPU0 单域）——**假想表**（推断标注：同
/// A72 核同 OPP 三点，域容量与核数无关；power 单 CPU 口径——Linux
/// em_perf_state.power「by 1 CPU」同构，与双核域同值合法；现役无单核
/// 注册面，数值随 [`QEMU_VIRT_DUAL`] 同批镜像，同步义务同挂）。
pub const QEMU_VIRT_SINGLE: EmDomain = EmDomain {
    cpu_mask: 0b1,
    points: [
        EmPoint { freq_khz: 250_000, power_mw: 5, capacity: 400 },
        EmPoint { freq_khz: 500_000, power_mw: 15, capacity: 750 },
        EmPoint { freq_khz: 1_000_000, power_mw: 50, capacity: 1024 },
        EmPoint::ZERO,
    ],
    npoints: 3,
};

// ---- 容量查询面（选核/schedutil 共消费）----

/// 最低够用档（Linux `em_pd_get_efficient_state` 最简形）：返回首个
/// capacity ≥ util 的档位下标——OPP 跟 util 走（schedutil 假设，sched-
/// energy.rst §6.4）；util 超顶档容量 → 顶档（钳位——超载账不静默破界，
/// 恰是 overutil 回退的动机）；空域 → None（EM 无据）。
#[inline]
pub fn eff_opp_index(d: &EmDomain, util: u32) -> Option<usize> {
    if d.npoints == 0 {
        return None;
    }
    for i in 0..d.npoints {
        if d.points[i].capacity >= util {
            return Some(i);
        }
    }
    Some(d.npoints - 1)
}

/// 容量查询：util 会被哪档容量服务（[`eff_opp_index`] 的容量投影——
/// CP-08 schedutil「f ≈ util×fmax/capacity」与 CP-09 选核共消费）；
/// 空域 → 0（无据，同 emface 极值读面口径）。
#[inline]
pub fn capacity_for_util(d: &EmDomain, util: u32) -> u32 {
    match eff_opp_index(d, util) {
        Some(i) => d.points[i].capacity,
        None => 0,
    }
}

/// 1024 尺度余量（域顶容量 − util，饱和下界 0）——Linux「highest spare
/// capacity」候选律的查询面（选核/负载均衡共口径；util 与容量同 1024 尺度）。
#[inline]
pub fn spare_capacity(domain_max_cap: u32, util: u32) -> u32 {
    domain_max_cap.saturating_sub(util)
}

// ---- 能量律（EAS compute_energy 最简形）----

/// 域能耗芯（max/sum 代数单源）：OPP 由 max_util 定（最低够用档），能耗
/// = sum_util × OPP.power_mw × 1000 / OPP.capacity（μW 整数 floor；u128
/// 中间量饱和回 u64——util×mW×千换算零浮点）。容量 0 档钳 1 防除零
/// （形状律不执法容量 > 0——该档能量记账无意义，语义留 CP-09 裁）。
#[inline]
fn energy_of(d: &EmDomain, max_util: u32, sum_util: u128) -> Option<u64> {
    let opp = &d.points[eff_opp_index(d, max_util)?];
    Some(
        ((sum_util * opp.power_mw as u128 * 1000) / opp.capacity.max(1) as u128)
            .min(u64::MAX as u128) as u64,
    )
}

/// 域能耗（μW 整数）——compute_energy 最简形的单域面：域级 OPP 由域内
/// **max util** 定，域能耗 = **sum util** × OPP 功率/OPP 容量（「单域一
/// 频、按 util 线性摊能」——sched-energy.rst §4 Example 2 逐数可复算，
/// 测试 linux_example2_anchor 同锚）。utils = **系统 per-CPU util 底景**
/// （索引 = CPU 号，任务外、0..=1024 口径——调用侧 EWMA 钳位后传入）；
/// **域只计 cpu_mask 内成员**（拓扑过滤单源——len 外/非成员 CPU 不入本
/// 域账；越界 util 原样记账——顶档线性外推，超载能耗单调升恰是 overutil
/// 回退的动机）。空域 → 0（无据）。
pub fn domain_energy_uw(d: &EmDomain, utils: &[u32]) -> u64 {
    if d.npoints == 0 {
        return 0;
    }
    let mut max_util = 0u32;
    let mut sum_util = 0u128;
    for (cpu, &u) in utils.iter().enumerate() {
        if cpu >= 32 || d.cpu_mask & (1u32 << cpu) == 0 {
            continue; // 非本域成员不计账
        }
        if u > max_util {
            max_util = u;
        }
        sum_util += u as u128;
    }
    energy_of(d, max_util, sum_util).unwrap_or(0)
}

/// 放置边际（EAS **energy_diff 最简形**）：把 task_util 加到 utils[cpu]
/// 后的域能耗 − 现域能耗——候选打分，越小越省。utils = **任务外底景**
/// （头注「底景观口径」节——唤醒面任务不在账上，prev 的 PELT 残留归
/// 调用侧）；cpu 必须在 utils 索引内**且**在 d.cpu_mask 中（非本域 CPU
/// → None——放错域）；放置后该 CPU util 饱和钳 1024（util 尺度上界）；
/// 空域 → None。域能耗只依赖 (max, sum)——放置边际无需物化「加任务
/// 后」数组（no_std 零分配，一遍扫描算 before/after 两组极值）。
pub fn energy_diff_uw(d: &EmDomain, utils: &[u32], task_util: u32, cpu: usize) -> Option<i64> {
    if d.npoints == 0 || cpu >= utils.len() || cpu >= 32 || d.cpu_mask & (1u32 << cpu) == 0 {
        return None;
    }
    let mut max_b = 0u32;
    let mut sum_b = 0u128;
    let mut max_a = 0u32;
    let mut sum_a = 0u128;
    for (i, &u) in utils.iter().enumerate() {
        if i >= 32 || d.cpu_mask & (1u32 << i) == 0 {
            continue; // 非本域成员不计账
        }
        let v = if i == cpu { u.saturating_add(task_util).min(1024) } else { u };
        if u > max_b {
            max_b = u;
        }
        sum_b += u as u128;
        if v > max_a {
            max_a = v;
        }
        sum_a += v as u128;
    }
    let after = energy_of(d, max_a, sum_a)?;
    let before = energy_of(d, max_b, sum_b)?;
    Some(after as i64 - before as i64)
}

/// 入座律（两段选核共用的平局保护）：后来者须**严格更省**才替换——
/// prev 先入座即得「只在严格更省时迁移」语义（sched-energy.rst §4）。
fn consider(best: &mut Option<(usize, i64)>, cpu: usize, score: i64) {
    if best.map_or(true, |(_, bs)| score < bs) {
        *best = Some((cpu, score));
    }
}

/// 两段选核最简形（Linux `find_energy_efficient_cpu` 同构）：
/// ①每域候选 = 域内 spare capacity（域顶容量 − CPU util）最大者（平局取
/// 最小编号——prev_cpu 已单独入候选集，域内平局无需再偏好）；
/// ②候选集 = 各域候选 ∪ {prev_cpu}，按 [`energy_diff_uw`] 打分（utils =
/// 任务外底景——头注「底景观口径」节），最小者胜；prev 先入座、后来者
/// 须严格更小才替换——平局 prev 胜（「只在严格更省时迁移」）；prev 不
/// 可用时平局取域序最小编号。
/// 除名律：域表空（EM 无据）、task_util > 域顶容量（放不下——Linux
/// capacity fit 门同构）的候选除名；候选空 → None（EAS 回退普通负载
/// 均衡——overutil/无 EM 同构退路）。
///
/// utils 按 CPU 编号索引（与 EmDomain.cpu_mask 同轴）；不属于任何域的
/// CPU 不候选（其 util 也不入任何域能耗账）。overutil **系统级门控在
/// 调用侧**（`berry_emface::overutil_judgment`——任一核超阈即整域让位
/// 负载均衡，sched-energy.rst §5；本函数不做系统级判断）。
pub fn eas_pick_cpu(
    domains: &[EmDomain],
    utils: &[u32],
    task_util: u32,
    prev_cpu: usize,
) -> Option<usize> {
    // 域归属：首个含该位且表非空的域；cpu ≥ utils.len() 或 ≥ 32 或无域
    // → None（cpu_mask u32 口径上限——v1 NR_CPU=2，泛化留 CP-19 拓扑演进）。
    let domain_of = |cpu: usize| -> Option<usize> {
        if cpu >= utils.len() || cpu >= 32 {
            return None;
        }
        domains
            .iter()
            .position(|d| d.npoints > 0 && d.cpu_mask & (1u32 << cpu) != 0)
    };
    // 候选资格 = 有域且放得下（task_util ≤ 域顶容量）。
    let eligible = |cpu: usize| -> Option<usize> {
        let di = domain_of(cpu)?;
        if task_util <= em_max_capacity_of(&domains[di]) {
            Some(di)
        } else {
            None
        }
    };
    let score = |di: usize, cpu: usize| -> i64 {
        energy_diff_uw(&domains[di], utils, task_util, cpu).expect("资格门内 diff 必 Some")
    };
    // prev 先入座（平局保护——后来者须严格更省才替换）。
    let mut best: Option<(usize, i64)> = None;
    if let Some(di) = eligible(prev_cpu) {
        best = Some((prev_cpu, score(di, prev_cpu)));
    }
    for (di, d) in domains.iter().enumerate() {
        if d.npoints == 0 || task_util > em_max_capacity_of(d) {
            continue; // 无据/放不下：本域不候选
        }
        // 域内候选：spare capacity 最大者（平局最小编号）。
        let mut cand: Option<(usize, u32)> = None;
        for cpu in 0..utils.len().min(32) {
            if d.cpu_mask & (1u32 << cpu) == 0 {
                continue;
            }
            let sp = spare_capacity(em_max_capacity_of(d), utils[cpu]);
            if cand.map_or(true, |(_, best_sp)| sp > best_sp) {
                cand = Some((cpu, sp));
            }
        }
        if let Some((cpu, _)) = cand {
            consider(&mut best, cpu, score(di, cpu));
        }
    }
    best.map(|(cpu, _)| cpu)
}

// ---- 表驱动钉板（host 测——任务书三面：表完整性/归一/打分单调）----

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(freq: u32, power: u32, cap: u32) -> EmPoint {
        EmPoint { freq_khz: freq, power_mw: power, capacity: cap }
    }

    /// Linux sched-energy.rst Example 2 对照表（cfg(test) fixture——容量/
    /// 功耗取文档原表，freq 推断填充（能量律只用容量/功耗，数值无物理
    /// 含义）；little 域 CPU0-1、big 域 CPU2-3）。
    fn linux_example_tables() -> (EmDomain, EmDomain) {
        let little = em_domain_from(
            0b0011,
            &[
                pt(250_000, 50, 170),
                pt(500_000, 150, 341),
                pt(1_000_000, 300, 512),
            ],
        )
        .expect("little 三点升序表装配须 Some");
        let big = em_domain_from(
            0b1100,
            &[
                pt(500_000, 400, 512),
                pt(750_000, 800, 768),
                pt(1_000_000, 1700, 1024),
            ],
        )
        .expect("big 三点升序表装配须 Some");
        (little, big)
    }

    /// 表完整性：QEMU 两表 + Linux 对照表全过形状执法（emface 单源），
    /// 且频率/容量/功耗三列各自严格升序、容量 ≤ 1024（能量律单调前提）。
    #[test]
    fn table_shape_complete() {
        let (little, big) = linux_example_tables();
        let tables = [(&QEMU_VIRT_SINGLE, 0usize), (&QEMU_VIRT_DUAL, 0), (&little, 0), (&big, 1)];
        for (d, idx) in tables {
            let pts = &d.points[..d.npoints];
            assert!(em_shape_ok(idx, d.cpu_mask, pts), "mask {:#b} 须过形状执法", d.cpu_mask);
            assert!(!pts.is_empty());
            for w in pts.windows(2) {
                assert!(w[0].freq_khz < w[1].freq_khz, "频率须严格升序");
                assert!(w[0].capacity < w[1].capacity, "容量须严格升序（能量律前提）");
                assert!(w[0].power_mw < w[1].power_mw, "功耗须严格升序（能量律前提）");
            }
            for p in pts {
                assert!(p.capacity <= 1024, "容量 1024 归一上界");
                assert!(p.freq_khz > 0 && p.power_mw > 0);
            }
            // 尾槽零填充不变量（em_domain_from 装配面——QEMU 常量表同核）。
            for i in d.npoints..EM_POINTS_MAX {
                assert_eq!(d.points[i], EmPoint::ZERO, "尾槽须零填充");
            }
        }
    }

    /// 容量 1024 归一：最高频点容量 = 满格（QEMU 单/双核同 1024；Linux
    /// 对照 big=1024、little=512 恰半格——「1024 = 系统最强核满格」的
    /// 相对尺度，sched-energy.rst §3 同口径）。
    #[test]
    fn capacity_1024_normalization() {
        let (little, big) = linux_example_tables();
        assert_eq!(em_max_capacity_of(&QEMU_VIRT_DUAL), 1024);
        assert_eq!(em_max_capacity_of(&QEMU_VIRT_SINGLE), 1024);
        assert_eq!(em_max_capacity_of(&big), 1024);
        assert_eq!(em_max_capacity_of(&little), 512, "小核顶 = 大核一半（文档原值）");
        // 极值读面（emface 单源）与容量投影一致性。
        assert_eq!(capacity_for_util(&QEMU_VIRT_DUAL, 0), 400);
        assert_eq!(capacity_for_util(&QEMU_VIRT_DUAL, 401), 750);
        assert_eq!(capacity_for_util(&QEMU_VIRT_DUAL, 5000), 1024, "超顶钳顶档");
        assert_eq!(capacity_for_util(&EmDomain::EMPTY, 100), 0, "空域无据");
    }

    /// QEMU 表逐位钉值——同步义务守卫（sched/src/em.rs:102-111 现役表
    /// 镜像；sched 侧改表不同步此处即咬）。
    #[test]
    fn qemu_values_pinned() {
        let pts = [
            pt(250_000, 5, 400),
            pt(500_000, 15, 750),
            pt(1_000_000, 50, 1024),
        ];
        assert_eq!(QEMU_VIRT_DUAL.cpu_mask, 0b11);
        assert_eq!(QEMU_VIRT_DUAL.npoints, 3);
        assert_eq!(&QEMU_VIRT_DUAL.points[..3], &pts);
        assert_eq!(QEMU_VIRT_SINGLE.cpu_mask, 0b1);
        assert_eq!(QEMU_VIRT_SINGLE.npoints, 3);
        assert_eq!(&QEMU_VIRT_SINGLE.points[..3], &pts);
    }

    /// 最低够用档边界：0 → 首档；恰在档容量上 → 本档（非次档）；超一 →
    /// 次档；超顶 → 顶档钳位；空域 → None；余量查询饱和下界。
    #[test]
    fn eff_opp_boundaries() {
        let d = &QEMU_VIRT_DUAL;
        assert_eq!(eff_opp_index(d, 0), Some(0));
        assert_eq!(eff_opp_index(d, 400), Some(0), "恰在容量 400 上 → 首档");
        assert_eq!(eff_opp_index(d, 401), Some(1));
        assert_eq!(eff_opp_index(d, 750), Some(1));
        assert_eq!(eff_opp_index(d, 751), Some(2));
        assert_eq!(eff_opp_index(d, 1024), Some(2));
        assert_eq!(eff_opp_index(d, u32::MAX), Some(2), "超顶钳顶档");
        assert_eq!(eff_opp_index(&EmDomain::EMPTY, 1), None);
        assert_eq!(spare_capacity(1024, 300), 724);
        assert_eq!(spare_capacity(1024, 2000), 0, "余量饱和下界 0");
        assert_eq!(spare_capacity(512, 512), 0);
    }

    /// Linux Example 2 数值锚（sched-energy.rst §4 三案例逐数复算）：任务
    /// P util=200、prev=CPU0。本面 utils = 任务外底景——little [200,100]
    /// （文档 CPU0 400 − P 200）、big [600,500] 不变；三案例总量 = 文档
    /// 1364/1437/1485（显示截断）的本面 floor 逐位值 1_365_774/1_438_801/
    /// 1_486_130 μW——同序且最小案例一致（Case 1 胜 = 放 CPU1）。
    #[test]
    fn linux_example2_anchor() {
        let (little, big) = linux_example_tables();
        // utils = 任务外系统底景（little CPU0-1、big CPU2-3；域账只计
        // mask 成员——头注口径）。
        let base = [200u32, 100, 600, 500];
        // 域级 OPP 选择锚：max_util 定档、sum_util 摊能（公式分量逐位）。
        assert_eq!(domain_energy_uw(&little, &base), 131_964); // opp(341,150): 300×150_000/341
        assert_eq!(domain_energy_uw(&big, &base), 1_145_833); // opp(768,800): 1100×800_000/768
        let mut c1u = base;
        c1u[1] += 200; // P 放 CPU1：little [200,300]
        assert_eq!(domain_energy_uw(&little, &c1u), 219_941); // opp(341,150): 500×150_000/341
        let mut c3u = base;
        c3u[0] += 200; // P 留 CPU0：little [400,100]
        assert_eq!(domain_energy_uw(&little, &c3u), 292_968); // opp(512,300): 500×300_000/512
        let mut c2u = base;
        c2u[3] += 200; // P 放 CPU3：big [600,700]
        assert_eq!(domain_energy_uw(&big, &c2u), 1_354_166); // opp(768,800): 1300×800_000/768
        // 三案例总量（文档 Case 1/3/2——放置 CPU1/留 CPU0/放 CPU3）。
        let c1 = 219_941 + 1_145_833;
        let c3 = 292_968 + 1_145_833;
        let c2 = 131_964 + 1_354_166;
        assert_eq!(c1, 1_365_774);
        assert_eq!(c3, 1_438_801);
        assert_eq!(c2, 1_486_130);
        assert!(c1 < c3 && c3 < c2, "三案例全序 c1 < c3 < c2（文档 1364<1437<1485 同序）");
        // 放置边际 = 案例差（任务外底景 + 候选放置——文档判据同构）。
        assert_eq!(energy_diff_uw(&little, &base, 200, 1), Some(87_977), "放 CPU1 增量最小");
        assert_eq!(energy_diff_uw(&little, &base, 200, 0), Some(161_004), "留 CPU0（prev）增量次之");
        assert_eq!(energy_diff_uw(&big, &base, 200, 3), Some(208_333), "放 CPU3 增量最大");
    }

    /// 打分单调：域能耗对单 CPU util 单调不减（OPP 内线性、跨档跳升——
    /// 功耗升序前提）；放置边际对 task_util 单调不减（1024 钳位后饱和
    /// 平坦合法）；跨档跳升显式锚。
    #[test]
    fn energy_scoring_monotonic() {
        let (little, big) = linux_example_tables();
        fn assert_monotone(d: &EmDomain, mk: impl Fn(u32) -> [u32; 4]) {
            let mut prev = None;
            for u in 0..=1024u32 {
                let utils = mk(u);
                let e = domain_energy_uw(d, &utils);
                if let Some(p) = prev {
                    assert!(e >= p, "util 升域能耗不减（mask {:#b} u={u}）", d.cpu_mask);
                }
                prev = Some(e);
            }
        }
        // 每表取本域成员变化的底景（域账只计 mask 成员）。
        assert_monotone(&QEMU_VIRT_DUAL, |u| [100, u, 0, 0]);
        assert_monotone(&QEMU_VIRT_SINGLE, |u| [u, 0, 0, 0]);
        assert_monotone(&little, |u| [100, u, 0, 0]);
        assert_monotone(&big, |u| [0, 0, 100, u]);
        // 放置边际单调（task_util 扫描）。
        let mut prev = None;
        for t in (0..=1024u32).step_by(7) {
            let diff = energy_diff_uw(&QEMU_VIRT_DUAL, &[100, 500], t, 0).unwrap();
            if let Some(p) = prev {
                assert!(diff >= p, "task_util 升边际不减（t={t}）");
            }
            prev = Some(diff);
        }
        // 跨档跳升锚：QEMU 单 CPU 400（首档顶）→ 401（次档）能耗跳升。
        let e400 = domain_energy_uw(&QEMU_VIRT_DUAL, &[400, 0]); // opp0: 400×5000/400
        let e401 = domain_energy_uw(&QEMU_VIRT_DUAL, &[401, 0]); // opp1: 401×15000/750
        assert_eq!(e400, 5_000);
        assert_eq!(e401, 8_020);
        assert!(e401 > e400, "跨档跳升（功耗升序前提）");
    }

    /// 选核两段律：Linux Example 2 全景（胜者 CPU1）+ 放不下让域（超小核
    /// 顶 → 大核）+ 全放不下 → None 回退；对称域平局留 prev；打包效应
    /// （装轻核保低档）与域内 OPP 档界锚。
    #[test]
    fn pick_cpu_laws() {
        let (little, big) = linux_example_tables();
        let doms = [little, big];
        // Linux Example 2：P=200 prev=0 → CPU1（Case 1 胜；边际 87_977 <
        // prev 161_004 < CPU3 208_333）。
        assert_eq!(eas_pick_cpu(&doms, &[200, 100, 600, 500], 200, 0), Some(1));
        // 放不下小核（600 > little 顶 512）：小核域/prev(CPU0) 除名 → big
        // 域候选 CPU3（spare 524 > CPU2 424）。
        assert_eq!(eas_pick_cpu(&doms, &[200, 100, 600, 500], 600, 0), Some(3));
        // 全放不下（2000 > 任何域顶）→ None（回退负载均衡）。
        assert_eq!(eas_pick_cpu(&doms, &[200, 100, 600, 500], 2000, 0), None);
        // 对称双核平局留 prev：两候选边际同（+1000）→ prev=1。
        assert_eq!(eas_pick_cpu(&[QEMU_VIRT_DUAL], &[100, 500], 50, 1), Some(1));
        // 打包效应：加到轻核（CPU0）保域低档——边际 1250 < 加重核 5750。
        assert_eq!(eas_pick_cpu(&[QEMU_VIRT_DUAL], &[100, 400], 100, 1), Some(0));
        // 域内 OPP 档界效应：重载 CPU1 恰在档界（750）上，任务加 1 到轻核
        // CPU0 不破档（+20），加到 CPU1 跨顶档（+24_552）→ CPU0。
        assert_eq!(eas_pick_cpu(&[QEMU_VIRT_DUAL], &[100, 750], 1, 1), Some(0));
        // 单核拓扑：唯一 CPU 必胜。
        assert_eq!(eas_pick_cpu(&[QEMU_VIRT_SINGLE], &[300], 200, 0), Some(0));
        // 数值锚（上列判据本体）。
        assert_eq!(energy_diff_uw(&QEMU_VIRT_DUAL, &[100, 400], 100, 0), Some(1_250));
        assert_eq!(energy_diff_uw(&QEMU_VIRT_DUAL, &[100, 400], 100, 1), Some(5_750));
        assert_eq!(energy_diff_uw(&QEMU_VIRT_DUAL, &[100, 500], 50, 0), Some(1_000));
        assert_eq!(energy_diff_uw(&QEMU_VIRT_DUAL, &[100, 500], 50, 1), Some(1_000));
        // 档界锚：底景 [100,750] opp1(750,15mW)；+1@cpu0 仍在 opp1（+20），
        // +1@cpu1 跨 opp2(1024,50mW)（+24_552）。
        assert_eq!(domain_energy_uw(&QEMU_VIRT_DUAL, &[100, 750]), 17_000);
        assert_eq!(energy_diff_uw(&QEMU_VIRT_DUAL, &[100, 750], 1, 0), Some(20));
        assert_eq!(energy_diff_uw(&QEMU_VIRT_DUAL, &[100, 750], 1, 1), Some(24_552));
    }

    /// 无据防御：空域能耗 0 / 边际 None / 选核 None / 放错域 None；overutil
    /// 阈值律（emface 单源引用在位提醒——819 边界与 emface overutil_law 同锚）。
    #[test]
    fn empty_domain_defensive() {
        assert_eq!(domain_energy_uw(&EmDomain::EMPTY, &[100, 200]), 0);
        assert_eq!(energy_diff_uw(&EmDomain::EMPTY, &[100], 50, 0), None);
        assert_eq!(energy_diff_uw(&QEMU_VIRT_DUAL, &[100], 50, 5), None, "cpu 越界");
        assert_eq!(energy_diff_uw(&QEMU_VIRT_SINGLE, &[100, 100], 50, 1), None, "非本域 CPU（mask 外）");
        assert_eq!(
            eas_pick_cpu(&[EmDomain::EMPTY, EmDomain::EMPTY], &[100, 100], 50, 0),
            None,
            "全域无据 → 回退负载均衡"
        );
        assert!(overutil_judgment(820, 1024));
        assert!(!overutil_judgment(819, 1024), "== 阈值不判（严格 >，emface 单源）");
    }
}
