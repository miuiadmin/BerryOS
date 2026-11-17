//! berry-balface —— on_irq_tick 内嵌 SMP 门与偷取过滤纯函数面（W3-Q82）。
//!
//! **动机**：`sched/src/sched.rs` 的 `on_irq_tick` slice 燃尽支内嵌三个
//! SMP 门 + `try_steal_one` 的 victim 双过滤，判定律只活在 aarch64-only
//! crate——sched 依赖 HAL global_asm，host 测不可达，ktest 只覆盖 boot 相。
//! 这些门的回归语义（折叠除法边界、饱和年龄判、失衡 K=2 与「近空闲」
//! 联合门、偷取过滤的三值合取）需要一张 host 可测的**钉板**：本 crate
//! 把它们按值域抽成纯函数 + 表驱动测试，`sched` 侧消费同一函数（零第二
//! 实现——抽离即唯一事实源，非镜像；wqmatch/sliceface 同款纪律）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`HEARTBEAT_DIV`]：心跳行折叠周期（tot/500 变档判——每 500 tot-tick
//!   打一行双核 ticks_pre 对照）。
//! - [`heartbeat_due`]：心跳折叠门——`tot/HEARTBEAT_DIV != prev/DIV` 即
//!   本档首 tick（swap 语义投影为纯函数：prev 是被换出的旧值）。
//! - [`STUCK_DIV`]：卡死扫描折叠周期（tot/2500 变档判——每 2500 tot-tick
//!   扫一次 SLEEPQ 高龄等待者）。
//! - [`stuck_scan_due`]：卡扫折叠门（同 [`heartbeat_due`] 形，独立周期）。
//! - [`STUCK_AGE_SECS`]：高龄判阈值（秒）——调用侧 `age > freq*10` 的
//!   host 投影。
//! - [`stuck_aged`]：高龄等待者判——CNTVCT 差值换算秒域后严格大于阈值
//!   （div 形与调用侧报告字段 `age / freq` 同一数学；freq==0 防御窗恒
//!   false 不 panic）。
//! - [`STUCK_MAX`]：点名帽（8——扫描窗内最多记录 8 条，防无界数组）。
//! - [`PULL_K`]：定时拉取失衡阈值（theirs ≥ mine + 2——「对核显著更重」）。
//! - [`pull_due`]：定时拉取门——`mine <= 1 && theirs >= mine + PULL_K`
//!   联合（近空闲补偿：本核只剩 monopolist 时才向对核拉一任务；两核各
//!   有可轮转负载时持续拉取 = 反复迁移，run6 实录 20 万+ 切换）。
//! - [`steal_filter`]：偷取 victim 过滤——`spread && ttbr0 != 0 && eligible`
//!   三值合取（F-E2 双过滤 + eligible 滤跳过——参数注入判据，纯面不读
//!   Task 结构；sched 侧以字段值调用）。
//!
//! 域界（物理臂留 sched）：per-CPU 原子槽（HB_LAST/STUCK_LAST/PULL_CNT）、
//! TTAS 锁窗、`try_steal_one` 环扫描/unlink/home 随迁、打印发射——本面
//! 只钉「门怎么判」的数定律。HAL 度（diag_readout/rq len）以参数注入。

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

/// 心跳行折叠周期（S-6 ③ per-CPU 心跳诊断面）：`tot / 500` 变档 = 打一行。
pub const HEARTBEAT_DIV: u64 = 500;

/// 卡死扫描折叠周期（S-6 ③ 高龄等待者扫描）：`tot / 2500` 变档 = 扫一轮。
pub const STUCK_DIV: u64 = 2500;

/// 高龄等待者判阈值（秒）：`age > freq * 10`（>10s = 永久丢醒候选）。
pub const STUCK_AGE_SECS: u64 = 10;

/// 卡扫点名帽：单轮扫描最多记录的等待者数（hits 数组定长）。
pub const STUCK_MAX: usize = 8;

/// 定时拉取失衡阈值（K=2，place() ⑤ 同 K）：`theirs >= mine + 2`。
pub const PULL_K: u64 = 2;

/// 心跳折叠门：本 tick 是否打行（`tot` 变档判——prev 为上一 tick 的
/// swap 换出值）。同档内恒 false（一档一行）；变档首 tick true。
#[inline]
pub fn heartbeat_due(tot: u64, prev: u64) -> bool {
    tot / HEARTBEAT_DIV != prev / HEARTBEAT_DIV
}

/// 卡扫折叠门：本 tick 是否扫 SLEEPQ（独立周期 2500，与心跳门分立——
/// 2500 = 500 的 5 倍，每第 5 行心跳伴一轮扫描的相位关系由两门独立
/// 折叠自然产生，非嵌套设计）。
#[inline]
pub fn stuck_scan_due(tot: u64, prev: u64) -> bool {
    tot / STUCK_DIV != prev / STUCK_DIV
}

/// 高龄等待者判：CNTVCT 差值严格大于 `freq * STUCK_AGE_SECS`（与内核
/// 判据 `age > freq * 10` 逐字同形——乘域判、报告字段 `age / freq` 只做
/// 展示换算，两数学独立，不得混用；调用侧已做 `now.saturating_sub(since)`
/// 饱和减防回绕。`saturating_mul` = 防御形：真实 freq 域（~24MHz）永不
/// 饱和，与裸乘在实域逐位一致；freq==0 防御窗恒 false 不 panic）。
#[inline]
pub fn stuck_aged(age_cntvct: u64, freq: u64) -> bool {
    freq != 0 && age_cntvct > freq.saturating_mul(STUCK_AGE_SECS)
}

/// 定时拉取门（S-6 ②）：近空闲补偿联合判——本核环只剩 monopolist
/// （`mine <= 1`）且对核显著更重（`theirs >= mine + PULL_K`）才放行
/// 拉取。两核各有可轮转负载（mine >= 2）时恒 false（反复迁移防线，
/// run6 实录 20 万+ 切换与 KL 争用）。域 = usize（Runqueue::len 的
/// 原生域——消费点零转换）。
#[inline]
pub fn pull_due(mine: usize, theirs: usize) -> bool {
    mine <= 1 && theirs >= mine + PULL_K as usize
}

/// 偷取 victim 过滤（F-E2 双过滤 + eligible 滤跳过）：三值合取——
/// `spread`（可铺核）∧ `ttbr0 != 0`（有地址空间——EL0 用户进程与挂表
/// 内核任务皆可偷）∧ `eligible`（出选资格，sched class 判据注入）。
/// 任一 false 即跳过该候选（扫描继续）。ttbr0 域 = usize（Task 字段
/// 原生域）。
#[inline]
pub fn steal_filter(spread: bool, ttbr0: usize, eligible: bool) -> bool {
    spread && ttbr0 != 0 && eligible
}

// ---- F4 wake 迁移门（SMP-01 §3.4 遗留修的判定律单源，W3-Q87）----
//
// [`sched.rs::eas_wake_pick`]（2026-11-16 owner 跨根合流笔 c1f695be9 落
// base）双门的判定律——负载动机迁移（非能量判据）：
// - 触发门：home 核 util > 80%（1024 归一 → > 819）——home 不忙不迁
//   （防乒乓第一门，`crate::policy::placement_hint_ungated` 头注自述
//   「防乒乓第一门 = overutil_judgment 同尺」——emface 80% 常量族）；
// - 容量门：对核 util < 75%（spare > 25% → util < 768）——对核不闲
//   无处可去；
// - 不吃 overutil 让位（wake 面的触发条件恰是 home 过载——若沿用
//   overutil 门控则迁移结构性死火；Linux 平行律：overutil 时 EAS
//   退场，但 wake 路负载均衡照常工作）；
// - 防乒乓：迁移后改写 `home_cpu`（下次 wake 从新 home 出发）——物理
//   臂留 sched（Task 指针写不在纯面）；
// - 单核零改：`par_enabled == false` 恒 home——**此短路在 sched 壳**
//   （HAL 调用不入纯面），纯面只钉双门数学。
//
// 域界：util 读（`crate::util::cpu_util`）与 home_cpu 改写留 sched
// （原子/指针物理臂）；本面只钉「两门怎么判」的数定律。

/// wake 迁移触发门阈值（80%，1024 归一）：home util **严格大于** 819
/// 才开迁移评估（819 = 80% × 1024 / 1000 = 819.2 → 819——与内核内联
/// 魔数 819 逐字对齐）。与 emface `overutil` 80% 常量族同尺（EAS 让位
/// 律的唤醒路镜像——wake 面不吃让位但同尺触发）。
pub const WAKE_MIGRATE_UTIL: u32 = 819;

/// wake 迁移容量门阈值（75%，1024 归一）：对核 util **严格小于** 768
/// 才有 spare（768 = 75% × 1024 / 1000 = 768 恰值——spare > 25%）。对核
/// util ≥ 768 = 无余量，无处可去。
pub const WAKE_SPARE_UTIL: u32 = 768;

/// F4 wake 迁移双门（纯判定律——`sched.rs::eas_wake_pick` 消费）：
/// `home_util > 819`（触发门：home 忙）∧ `other_util < 768`（容量门：
/// 对核有 spare）→ 迁移。任一门不过 → 留 home。恰界值钉板（819 拒 /
/// 820 过 / 767 过 / 768 拒）见测试；判序 = 触发门先（home 不忙的常见
/// 态在第一门即返，省一次对核读——消费侧短路序同形）。域 = u32
/// （`util::cpu_util` 的 1024 归一原生域——消费点零转换）。
///
/// 单核短路（`par_enabled()==false` 恒 home）与防乒乓改写（迁移后
/// `home_cpu = other`）是 sched 壳的物理臂，不在此面——纯面输入已是
/// 两核 util 快照，无 HAL 依赖。
#[inline]
pub fn wake_migrate_gate(home_util: u32, other_util: u32) -> bool {
    home_util > WAKE_MIGRATE_UTIL && other_util < WAKE_SPARE_UTIL
}

// ---- per-CPU 计数器路由/求和/跨点判定律（04 号调度审计机会三，W3-Q96）----
//
// [`sched.rs`] 四个最高频计数器（IRQ_TICKS/SWITCHES/SWITCHES_TT/
// PREEMPT_CKPT_HITS）原为单 `static AtomicU64`——SMP 参与态下双核对同一
// cache line 做 Relaxed RMW，忙对稳态（两核各跑一个分时任务）每 tick +
// 每 switch + 每 syscall 至少四次跨核行乒乓（04 号审计「SMP 经典反模式」
// 判词）。per-CPU 化 = 写侧按 `cpu_id()` 写本核槽（LB_TICKS `[AtomicU64; 2]`
// 判例同款，sched.rs）、读侧求和。本面钉「槽怎么路由、和怎么读、跨点
// 怎么判」的数定律；原子槽物理臂留 sched（同域界纪律）。
//
// 语义保持三律（读侧全为快照/增长型断言，无等值比较依赖——审计原文
// 机会三「风险与守卫」节论证）：
// - watchdog `switches > last`（增长型）与 soakmon 周期采样：求和快照安全
//   （两槽和有 ns 级非原子窗，增长型断言不受影响）；
// - G1.5 `tel >= switches_tt`（klock 窗内同窗双快照）：求和后仍同窗；
// - `switches=` 打印频度按全局和（SOAK_PRINT_EVERY 跨点判定读和——本面
//   `soak_crossed` 钉板该数学）。

/// per-CPU 槽数（SMP 双核 K=2——与 `LB_TICKS`/`SmpCpu` 槽位制同宽）。
pub const PERCPU_SLOTS: usize = 2;

/// soak 打点跨点周期（与 sched.rs `bump_switches` 的 SOAK_PRINT_EVERY
/// 同值冻结——改值须两侧同改，此常量是 host 钉板锚）。
pub const SOAK_PRINT_EVERY: u64 = 10000;

/// 槽路由：核号 → 本核槽下标。K=2 下恒 `cpu & 1`（防御域外核号——
/// 硬件口径下 cpu_id 只产 0/1，`& 1` 与直索引逐位一致；域外值（如
/// 测试注灾 7）钉到 1 而非 panic——IRQ 上下文不 panic 纪律）。
#[inline]
pub fn slot_of(cpu: usize) -> usize {
    cpu & (PERCPU_SLOTS - 1)
}

/// 读侧求和：per-CPU 槽快照求和（读者面 = watchdog/G1.5/soakmon/fa，
/// 全为增长型/下界断言——快照语义，两槽非原子窗安全）。
/// `add` 注入读函数（物理臂留 sched：`|s| s.load(Relaxed)`）。
#[inline]
pub fn slots_sum<F: Fn(usize) -> u64>(read: F) -> u64 {
    let mut sum: u64 = 0;
    for i in 0..PERCPU_SLOTS {
        sum = sum.wrapping_add(read(i));
    }
    sum
}

/// soak 打点跨点判定：本次递增后的全局和是否恰跨过打印点
/// （`sum % SOAK_PRINT_EVERY == 0`——打印频度按全局和，跨点者打行；
/// 与旧全局计数器语义逐位一致：旧 `sw % 10000 == 0` 的 sw 即全局和）。
/// 零值排除：旧形态 sw = fetch_add 返回 +1 恒 ≥ 1（判前必先递增），
/// 0 非跨点（防 boot 前窗求和快照读 0 误打行——与旧语义对齐钉板）。
#[inline]
pub fn soak_crossed(sum: u64) -> bool {
    sum != 0 && sum % SOAK_PRINT_EVERY == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- heartbeat_due / stuck_scan_due：折叠除法变档律 ----

    #[test]
    fn heartbeat_folds_every_500() {
        // 同档恒 false：档内任意两值（含跨 swap 序）。
        assert!(!heartbeat_due(0, 0));
        assert!(!heartbeat_due(499, 100));
        // 恰界 500 = 档 1 首 tick（500/500=1 vs 499/500=0）——变档 true。
        assert!(heartbeat_due(500, 499));
        assert!(!heartbeat_due(500, 500));
        assert!(heartbeat_due(500, 0));
        assert!(heartbeat_due(1000, 999));
        assert!(heartbeat_due(2500, 2499));
    }

    #[test]
    fn heartbeat_band_edges() {
        // 档边界全域：[k*500, k*500+499] 内 prev/tot 同档 false；跨档 true。
        for k in 1u64..6 {
            let lo = k * 500;
            let hi = lo + 499;
            assert!(!heartbeat_due(hi, lo), "band {k} 内必 false");
            assert!(heartbeat_due(lo, lo - 1), "跨入档 {k} 首 tick 必 true");
            assert!(!heartbeat_due(lo, lo + 1), "档 {k} 内第二 tick 必 false");
        }
        // 倒退窗（tot < prev——计数不回退，防御投影）：除法商域同判。
        assert!(heartbeat_due(499, 500)); // 商 0 vs 1 = 变档
        assert!(!heartbeat_due(600, 700)); // 同档 1
    }

    #[test]
    fn stuck_folds_every_2500() {
        assert!(!stuck_scan_due(0, 0));
        assert!(!stuck_scan_due(2499, 0));
        assert!(stuck_scan_due(2500, 2499));
        assert!(!stuck_scan_due(4999, 2500)); // 同档 1
        assert!(stuck_scan_due(5000, 4999));
        // 与心跳门相位独立：500 的倍数档变档处卡扫门不一定变档。
        assert!(heartbeat_due(500, 499));
        assert!(!stuck_scan_due(500, 499)); // 档 0 内
    }

    #[test]
    fn folds_u64_extremes() {
        // 敌意巨值：商域比较不溢出（u64::MAX 与 MAX-1 同档 [MAX/500]——
        // 除法商域窄化后比较恒安全）。
        assert!(!heartbeat_due(u64::MAX, u64::MAX - 1));
        assert!(!heartbeat_due(u64::MAX, u64::MAX / 500 * 500));
        assert!(heartbeat_due(u64::MAX, u64::MAX / 500 * 500 - 1));
        assert!(!stuck_scan_due(u64::MAX, u64::MAX - 1));
        assert!(stuck_scan_due(u64::MAX, u64::MAX / 2500 * 2500 - 1));
        // 零基线。
        assert!(!heartbeat_due(0, 0));
        assert!(!stuck_scan_due(0, 0));
    }

    // ---- stuck_aged：高龄判（秒域 div 形）----

    #[test]
    fn stuck_age_threshold() {
        // QEMU 典型 24MHz：10s = 2.4e8 周期；严格大于律。
        let freq = 24_000_000u64;
        assert!(!stuck_aged(10 * freq, freq)); // 恰 10s = 不报（> 严格）
        assert!(stuck_aged(10 * freq + 1, freq)); // +1 周期 = 报
        assert!(stuck_aged(freq * 3600, freq)); // 1h 高龄
        assert!(!stuck_aged(0, freq));
        assert!(!stuck_aged(10 * freq - 1, freq));
    }

    #[test]
    fn stuck_age_freq_zero_defensive() {
        // freq==0 防御窗：恒 false 不 panic（div by zero 防线）。
        assert!(!stuck_aged(u64::MAX, 0));
        assert!(!stuck_aged(0, 0));
    }

    #[test]
    fn stuck_age_sub_tick_domain() {
        // 与内核判据 `age > freq*10` 逐字同形对拍（乘域——亚秒粒度下
        // div 形会漏报 10s+1tick 形态，两数学不等价已被实跑红抓住）：
        for age in [0u64, 1, freq_v(), 10 * freq_v(), 10 * freq_v() + 1, 11 * freq_v(), u64::MAX] {
            assert_eq!(
                stuck_aged(age, freq_v()),
                age > freq_v().saturating_mul(STUCK_AGE_SECS),
                "age={age}"
            );
        }
    }

    /// 测试用 freq 钉 24MHz（QEMU virt CNTFRQ）。
    fn freq_v() -> u64 {
        24_000_000
    }

    #[test]
    fn stuck_max_cap_frozen() {
        // 点名帽 ABI 冻结值钉板（hits 数组定长——改值 = 改诊断行上限）。
        assert_eq!(STUCK_MAX, 8);
        assert_eq!(HEARTBEAT_DIV, 500);
        assert_eq!(STUCK_DIV, 2500);
        assert_eq!(STUCK_AGE_SECS, 10);
        assert_eq!(PULL_K, 2);
    }

    // ---- pull_due：近空闲补偿联合门 ----

    #[test]
    fn pull_gate_truth_table() {
        // mine<=1 ∧ theirs>=mine+2 全表（小域枚举钉板）。
        for mine in 0usize..4 {
            for theirs in 0usize..7 {
                let want = mine <= 1 && theirs >= mine + 2;
                assert_eq!(pull_due(mine, theirs), want, "mine={mine} theirs={theirs}");
            }
        }
    }

    #[test]
    fn pull_gate_monopolist_forms() {
        // 近空闲态（monopolist 独占）：mine=0/1 两形。
        assert!(pull_due(0, 2)); // 空环 + 对核 2
        assert!(pull_due(1, 3)); // monopolist + 对核 3
        assert!(!pull_due(1, 2)); // theirs 恰 mine+1 = 不显著
        assert!(!pull_due(0, 1));
        assert!(!pull_due(0, 0));
        // 两核各有可轮转负载（mine>=2）：恒 false（反复迁移防线）。
        assert!(!pull_due(2, 10));
        assert!(!pull_due(5, 100));
        assert!(!pull_due(usize::MAX, usize::MAX)); // 饱和窗
    }

    #[test]
    fn pull_gate_extreme_no_overflow() {
        // mine=1, theirs=u64::MAX：mine+2 不在表达式域溢出（u64 加法在
        // theirs 域饱和前先判 mine<=1——编译期常量折叠域，运行时无溢出）。
        assert!(pull_due(1, usize::MAX));
        assert!(pull_due(0, usize::MAX));
    }

    // ---- steal_filter：victim 三值合取 ----

    #[test]
    fn steal_filter_truth_cube() {
        // 2^3 全笛卡尔：仅 (true, ttbr0!=0, true) 通过。
        for spread in [false, true] {
            for ttbr0 in [0usize, 1, 0x1000] {
                for eligible in [false, true] {
                    let want = spread && ttbr0 != 0 && eligible;
                    assert_eq!(
                        steal_filter(spread, ttbr0, eligible),
                        want,
                        "spread={spread} ttbr0={ttbr0:#x} elig={eligible}"
                    );
                }
            }
        }
    }

    #[test]
    fn steal_filter_ttbr0_domain() {
        // ttbr0 值域：非零即过（任何合法表基址——含对齐弱形 1）。
        assert!(steal_filter(true, 1, true));
        assert!(steal_filter(true, 0x4000, true));
        assert!(steal_filter(true, usize::MAX, true));
        // 内核任务（ttbr0==0）不可偷——EL1 裸跑者留原核。
        assert!(!steal_filter(true, 0, true));
        // 不可铺核（spread=false）跳过。
        assert!(!steal_filter(false, 0x4000, true));
        // 出选资格否（eligible=false——cur-in-ring 常态对核 pick 滤跳过）。
        assert!(!steal_filter(true, 0x4000, false));
    }

    // ---- wake_migrate_gate：F4 wake 迁移双门（W3-Q87）----

    #[test]
    fn wake_gate_both_pass_migrates() {
        // 双门全过：home 忙（>819）∧ 对核有 spare（<768）→ 迁移。
        assert!(wake_migrate_gate(820, 767));
        assert!(wake_migrate_gate(819 + 1, 768 - 1));
        assert!(wake_migrate_gate(1024, 0)); // 满载 home + 全空闲对核
        assert!(wake_migrate_gate(u32::MAX, 0)); // 敌意巨值（乘加无——纯比较恒安全）
    }

    #[test]
    fn wake_gate_home_trigger_boundary() {
        // 触发门恰界：819 = 80% 恰值 → 严格大于律拒（home「不忙」）。
        assert!(!wake_migrate_gate(819, 0));
        // 820 = 过（最小过值）。
        assert!(wake_migrate_gate(820, 0));
        // home 闲（0 / 常态 400）：无论对核多空都不迁（防乒乓第一门）。
        assert!(!wake_migrate_gate(0, 0));
        assert!(!wake_migrate_gate(400, 0));
        assert!(!wake_migrate_gate(818, 767)); // 触发门差一拒
    }

    #[test]
    fn wake_gate_other_spare_boundary() {
        // 容量门恰界：768 = 75% 恰值 → 严格小于律拒（spare 不 >25%）。
        assert!(!wake_migrate_gate(1024, 768));
        // 767 = 过（最小 spare 过值）。
        assert!(wake_migrate_gate(1024, 767));
        // 对核也忙（1024/900）：home 再忙也无处可去。
        assert!(!wake_migrate_gate(1024, 1024));
        assert!(!wake_migrate_gate(u32::MAX, 900));
    }

    #[test]
    fn wake_gate_quadrant_full_scan() {
        // 双门四象限全域（步进 1 过界点）：门序独立、无交叠副作用。
        for home in [0u32, 818, 819, 820, 900, 1024] {
            for other in [0u32, 100, 500, 767, 768, 900, 1024] {
                let want = home > 819 && other < 768;
                assert_eq!(
                    wake_migrate_gate(home, other),
                    want,
                    "home={home} other={other}"
                );
            }
        }
    }

    #[test]
    fn wake_gate_constants_norm_alignment() {
        // 常量与归一化尺对齐钉板：819 = 80%·1024/1000 下取整 + 严格大于
        // （即「>80%」与「>=820」等价）；768 = 75% 恰值 + 严格小于
        // （即「<75%」与「<=767」等价）。改阈值必红此测（防漂移钉）。
        assert_eq!(WAKE_MIGRATE_UTIL, 819);
        assert_eq!(WAKE_SPARE_UTIL, 768);
        // 与旧内联魔数（sched.rs eas_wake_pick 源）逐字一致执法：
        // > 819 / < 768 的旧字面在新常量下逐位重现。
        for v in 0u32..=1024 {
            assert_eq!(v > WAKE_MIGRATE_UTIL, v > 819, "trigger @ {v}");
            assert_eq!(v < WAKE_SPARE_UTIL, v < 768, "spare @ {v}");
        }
    }

    // ---- per-CPU 计数器路由/求和/跨点判定律（W3-Q96）----

    #[test]
    fn percpu_slot_routes_all_cpu_ids() {
        // 硬件域全域：cpu_id 只产 0/1，直索引逐位一致。
        assert_eq!(slot_of(0), 0);
        assert_eq!(slot_of(1), 1);
        // 域外防御：钉到低位槽而非 panic（IRQ 上下文不 panic 纪律）。
        assert_eq!(slot_of(2), 0);
        assert_eq!(slot_of(7), 1);
        assert_eq!(slot_of(usize::MAX), 1);
        // K=2 宽度钉板：槽常数与 LB_TICKS 槽位制同宽。
        assert_eq!(PERCPU_SLOTS, 2);
    }

    #[test]
    fn percpu_sum_folds_all_slots() {
        // 全零态求和 = 0；单槽贡献独立；双槽叠加；空读面恒 0。
        assert_eq!(slots_sum(|_| 0), 0);
        assert_eq!(slots_sum(|s| if s == 0 { 41 } else { 0 }), 41);
        assert_eq!(slots_sum(|s| if s == 1 { 7 } else { 0 }), 7);
        assert_eq!(slots_sum(|_| 41), 82);
        // 大值不溢出常态域（u64 两槽和）；回绕防御形（wrapping——与
        // 旧单计数器 u64 语义同宽，溢出面非本刀语义变化）。
        assert_eq!(slots_sum(|_| u64::MAX / 2), u64::MAX - 1);
    }

    #[test]
    fn percpu_soak_cross_points_align_old_semantics() {
        // 跨点判定与旧全局计数器逐位一致：恰 10000/20000 跨点。
        assert!(!soak_crossed(0));
        assert!(!soak_crossed(1));
        assert!(!soak_crossed(9999));
        assert!(soak_crossed(10000));
        assert!(!soak_crossed(10001));
        assert!(soak_crossed(20000));
        // 常量冻结锚：与 sched.rs SOAK_PRINT_EVERY 同值（2026-11-17
        // 降频定谳 100→10000——G1/S-6 里程碑已过，金丝雀语义在 10000
        // 档完整保留）。
        assert_eq!(SOAK_PRINT_EVERY, 10000);
    }

    #[test]
    fn percpu_partition_invariant_under_interleaving() {
        // 分槽不分割全局语义：任意两槽分配形态下，逐次递增后求和的
        // 跨点序 = 旧单计数器跨点序（每 10000 次递增恰一次跨点——
        // 分槽只改写路径不改计数语义的端到端钉板）。
        let mut slots = [0u64; 2];
        let mut crossings = 0;
        let mut expected = 0;
        // 模拟 25000 次递增，交替打在两槽（最坏伪共享拆除后形态）。
        for i in 0..25000u64 {
            let s = slot_of((i % 2) as usize);
            slots[s] += 1;
            let sum = slots[0] + slots[1];
            if soak_crossed(sum) {
                crossings += 1;
            }
            expected += 1;
            // 旧语义对拍：单计数器递增到 expected 时的跨点判定。
            assert_eq!(soak_crossed(sum), expected % SOAK_PRINT_EVERY == 0,
                "divergence at {expected}");
        }
        assert_eq!(crossings, 2); // 10000/20000 两点。
    }

}
