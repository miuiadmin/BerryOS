//! berry-balface —— sched SMP 门/偷取过滤/金丝雀节流纯函数面（W3-Q82）。
//!
//! **动机**：`sched/src/sched.rs` 的 `on_irq_tick` slice 燃尽支内嵌定时
//! 拉取门 + `try_steal_one` 的 victim 双过滤 + `bump_switches` 的金丝雀
//! 时间节流门，判定律只活在 aarch64-only crate——sched 依赖 HAL
//! global_asm，host 测不可达，ktest 只覆盖 boot 相。这些门的回归语义
//! （失衡 K=2 与「近空闲」联合门、偷取过滤的三值合取、金丝雀节流的
//! 饱和减边界）需要一张 host 可测的**钉板**：本 crate 把它们按值域抽成
//! 纯函数 + 表驱动测试，`sched` 侧消费同一函数（零第二实现——抽离即
//! 唯一事实源，非镜像；wqmatch/sliceface 同款纪律）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`PULL_K`]：定时拉取失衡阈值（theirs ≥ mine + 2——「对核显著更重」）。
//! - [`pull_due`]：定时拉取门——`mine <= 1 && theirs >= mine + PULL_K`
//!   联合（近空闲补偿：本核只剩 monopolist 时才向对核拉一任务；两核各
//!   有可轮转负载时持续拉取 = 反复迁移，run6 实录 20 万+ 切换）。
//! - [`steal_filter`]：偷取 victim 过滤——`spread && ttbr0 != 0 && eligible`
//!   三值合取（F-E2 双过滤 + eligible 滤跳过——参数注入判据，纯面不读
//!   Task 结构；sched 侧以字段值调用）。
//! - [`CANARY_PERIOD`]/[`canary_due`]：金丝雀打印时间节流（专段见下）。
//!
//! 域界（物理臂留 sched）：per-CPU 原子槽（SWITCHES 双槽）、金丝雀
//! CANARY_LAST 单 static、TTAS 锁窗、`try_steal_one` 环扫描/unlink/home
//! 随迁、打印发射——本面只钉「门怎么判」的数定律。HAL 度（CNTVCT 直读/
//! rq len）以参数注入。

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

/// 定时拉取失衡阈值（K=2，place() ⑤ 同 K）：`theirs >= mine + 2`。
pub const PULL_K: u64 = 2;

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

// ---- per-CPU 计数器路由/求和判定律（04 号调度审计机会三，W3-Q96）----
//
// [`sched.rs`] 四个最高频计数器（IRQ_TICKS/SWITCHES/SWITCHES_TT/
// PREEMPT_CKPT_HITS）原为单 `static AtomicU64`——SMP 参与态下双核对同一
// cache line 做 Relaxed RMW，忙对稳态（两核各跑一个分时任务）每 tick +
// 每 switch + 每 syscall 至少四次跨核行乒乓（04 号审计「SMP 经典反模式」
// 判词）。per-CPU 化 = 写侧按 `cpu_id()` 写本核槽（LB_TICKS `[AtomicU64; 2]`
// 判例同款，sched.rs）、读侧求和。本面钉「槽怎么路由、和怎么读」的
// 数定律；原子槽物理臂留 sched（同域界纪律）。
//
// 语义保持律（读侧全为快照/增长型断言，无等值比较依赖——审计原文
// 机会三「风险与守卫」节论证）：
// - watchdog `switches > last`（增长型）与 soakmon 周期采样：求和快照安全
//   （两槽和有 ns 级非原子窗，增长型断言不受影响）；
// - G1.5 `tel >= switches_tt`（klock 窗内同窗双快照）：求和后仍同窗。

/// per-CPU 槽数（SMP 双核 K=2——与 `LB_TICKS`/`SmpCpu` 槽位制同宽）。
pub const PERCPU_SLOTS: usize = 2;

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

// ---- 金丝雀打印时间节流判定律（金丝雀时间节流刀，2026-11-19）----
//
// `switches=` 行是宿主三件（smoke-smp canary_check 硬门、smoke-image
// 阶段 4/6 硬门、bcts-parkprobe churn 判据）唯一可 grep 的活性载体——
// 宿主读不到内核内存，打印节律即门的死活。旧判定 soak_crossed（计数
// % SOAK_PRINT_EVERY 恰跨点，sched 侧实喂本核槽值）把行频钉在切换率上：
// TCG 相窗双核各 ~500 切换永不满 10000 跨点 ⇒ 结构性零行、门必红
// （0ea3d11ad 既账）——被测对象越慢（最需要活性证据时）行越少，按
// 事件数定节奏是自拆台。
//
// 定案论据——活性证据与切换率解耦：判定改 CNTVCT 差值 ≥ 周期的时间
// 节流，快机更稀（≤1 行/15s，保 0ea3d11ad 降频收益且改善）、慢机必在
// （400s 金丝雀窗内快慢机型都稳定出 ≥3 行）；打印值仍为锁内两槽和
// 快照——打印者自身增量必含于其和读，结构性严格递增（顺带消灭旧
// 取模形态相邻行可逐位相等的理论红面）。宿主三件零改动自动复活。
//
// 退役史：soak_crossed/SOAK_PRINT_EVERY（W3-Q96 事件计数跨点判定）随
// 本刀退役；心跳/卡扫折叠门族随探针族退役刀（2026-11-19）同线退役。
// 物理臂留 sched `bump_switches`：全局单 CANARY_LAST static（vct 快照）
// + CNTVCT 直读 + CAS 取档防双打（输者跳过打印，计数照记）——本面只
// 钉「周期怎么判」的数定律。

/// 金丝雀时间节流周期（vct 域）：15 秒 × 62.5MHz（QEMU virt 实测
/// CNTFRQ——timer.rs「不硬编码」口径的运行值）= 9.375e8 周期。存 vct
/// 不存秒：`canary_due` 两参签名无 freq 位，调用侧（切换热路径）传
/// CNTVCT 直读值零折算零除法——与旧 stuck 族「秒常量+freq 参数注入
/// 折算」先例的分流由签名形定谳。起步值待 TCG 实跑定标（行体积过大/
/// 过小改此一处，常量单源）；异频机型墙钟周期随频线性缩放（半率
/// 31.25MHz → 30s 墙钟，smoke-image 120s 帽 1.3× 裕量在案）。
pub const CANARY_PERIOD: u64 = 15 * 62_500_000;

/// 金丝雀时间节流门：距上次打行是否已满一个周期（`now`/`last` 均为
/// CNTVCT 直读值——sched 侧 last 为全局单 CANARY_LAST static 的快照，
/// 初值 0 ⇒ 首行落在计数原点起满一周期时（QEMU 复位起 ~15s），无
/// last==0 哨兵特例——纯函数形定谳）。饱和减防回绕/回退窗（now < last
/// 恒 false 不 panic——IRQ 上下文不 panic 纪律）；恰满一周期 = due
/// （≥ 含恰值）。
#[inline]
pub fn canary_due(now: u64, last: u64) -> bool {
    now.saturating_sub(last) >= CANARY_PERIOD
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // 常量冻结锚：K=2（place() ⑤ 同 K；旧锚随 stuck 族冻结测退役，
        // 移此续锚——表内 want 字面 +2 与常量改值必联动红）。
        assert_eq!(PULL_K, 2);
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

    // ---- per-CPU 计数器路由/求和判定律（W3-Q96）----

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

    // ---- canary_due：金丝雀时间节流门（金丝雀时间节流刀，2026-11-19）----

    #[test]
    fn canary_first_shot_from_zero_last() {
        // 首拍（last=0——CANARY_LAST 初值/boot 前快照）：计数原点起满一
        // 周期即 due；期内（vct < 周期）恒 false ⇒ 首行落在复位起 ~15s。
        assert!(!canary_due(0, 0));
        assert!(!canary_due(CANARY_PERIOD - 1, 0));
        assert!(canary_due(CANARY_PERIOD, 0));
        assert!(canary_due(u64::MAX, 0));
        // 常量冻结锚：15s × 62.5MHz（QEMU virt 实测 CNTFRQ）= 9.375e8。
        assert_eq!(CANARY_PERIOD, 937_500_000);
    }

    #[test]
    fn canary_within_period_false() {
        // 期内恒 false：距上次打行不足一周期（恰差 1 也不 due）。
        let last = 7 * CANARY_PERIOD; // 任意历史拍（非零）。
        assert!(!canary_due(last, last));
        assert!(!canary_due(last + 1, last));
        assert!(!canary_due(last + CANARY_PERIOD - 1, last));
        // 回退/回绕防御窗（now < last）：saturating_sub 归 0 恒 false。
        assert!(!canary_due(last - 1, last));
        assert!(!canary_due(0, u64::MAX));
    }

    #[test]
    fn canary_across_period_true() {
        // 跨期 true：恰满一周期（≥ 含恰值）与超期均 due。
        let last = 3 * CANARY_PERIOD;
        assert!(canary_due(last + CANARY_PERIOD, last));
        assert!(canary_due(last + CANARY_PERIOD + 1, last));
        assert!(canary_due(u64::MAX, 42));
    }

}
