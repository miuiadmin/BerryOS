//! berry-iprofile —— intent 六档 IntentProfile 映射（纯数据 + 纯函数）。
//!
//! **IntentProfile 六档映射表**（ADR #42 第一段 + #58 单一词汇表归一；W3-Q1）。
//!
//! v1/v2 语义 = **查表键先于消费者落地**：intent u8 → 软参数轴（latency-nice /
//! weight 包络 / EPP / uclamp / timer slack / maintenance_eligible），零新层级
//! 机制、无带间选择器、无预算记账（§3.8「不为 band 提前缴复杂度税」）。
//!
//! 三件套设计（`调度器设计-v1.1.md` §3.8 IntentProfile 映射表）：
//! - **纯数据、按设备档位选表**：手表表（单核、无 capacity 放置列）为主验
//!   绑定表（QEMU virt 单核）；PC 表（含 capacity hint 列）v2+ 随 EEVDF
//!   消费点落列——形状先定、实现后补（表示法纪律）。
//! - **运行时更新 + 模型不可信回退保守通道**：回退表
//!   （[`FALLBACK_PROFILE_TABLE``]）全部列取**最保守**值（宁可不省、不可
//!   错放——XNU `qos_policy_params` 回退同构）。「保守」= 数值方向上**不借出
//!   任何优势**：latency_nice 不提前、weight 包络不放大、uclamp 不抬频。
//! - **deadline/RT 任务也查此表**（EPP/uclamp 列）——RT 存在即被钉 MAX 频率
//!   是电池杀手（Linux uclamp 之前 SCHED_FIFO 的教训）；RT 屏蔽 AI ≠ RT 免治。
//!
//! 全模块纯函数 + `const` 数据（IRQ 热路径零分配、零 `unsafe`、host 可测）。
//! 词表单源 = `intent.rs` 编码冻结（`INTENT_*` 常量），本表按档位给行——
//! 禁第二份编码手抄（#58：berry-manifest::Intent 之外的镜像即违例）。
//!
//! **u8 越小档越高**（0x00=UNSPECIFIED 顶）；六个档位各一行（含 0x05
//! MAINTENANCE——编码冻结、v1 不启用，表行先落保守值：查表键完整化 = 含
//! 保留档的行，运行期拒绝面仍在 `intent_valid` 入口）。

#![cfg_attr(not(test), no_std)]

// ---- EppHint 单源（自 berry-sched 迁入；sched::task re-export 保持既有
// 引用路径零改动——W3-Q1。能效倾向 hint（EPP，`调度器设计.md` §6）：指引
// 大小核放置与改频。字段先落地（2b-4），消费语义随放置路径（M1b）/PM 服务
// （M3）兑现。----

/// 能效倾向（EPP）二值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EppHint {
    /// 效率优先（默认——省电是身份支柱）。
    Efficiency,
    /// 性能优先。
    Perf,
}

// ---- 列值域常量（表数据执法的编译期下界）----

/// EEVDF latency-nice 负向界（i8 符号偏移；负 = 更早虚拟截止期）。
pub const LATENCY_NICE_MIN: i8 = -20;
/// latency-nice 正向界（正 = 更晚截止期；background/maintenance 档用）。
pub const LATENCY_NICE_MAX: i8 = 19;

/// weight 包络下界（u32；AI ④类动作的带内包络——min/max 夹紧）。
pub const WEIGHT_FLOOR_MIN: u32 = 1;
/// weight 包络上界（1024 = CFS default weight 量级，1:1 无放大）。
pub const WEIGHT_CEILING_MAX: u32 = 1024;

/// uclamp 值域上界（0..1024，喂 PM/DVFS）。
pub const UCLAMP_MAX: u16 = 1024;

/// 交互类 timer slack 缺省（50ms——XNU QoS UTILITY 档同量级）。
pub const SLACK_INTERACTIVE_DEFAULT_NS: u64 = 50_000_000;

/// MAINTENANCE 档 timer slack（宽松上限方向；10s——与 #42 debounce 基线
/// 同量级但语义独立：这是「允许顺延多少」的帽，非 debounce 判据窗）。
pub const MAINTENANCE_SLACK_NS: u64 = 10_000_000_000;

// ---- IntentProfile 结构（§3.8 形状钉子）----

/// 单档位软参数行（纯数据；XNU `qos_policy_params` 四轴表同构 + BerryOS
/// 扩项：weight 包络 + maintenance_eligible）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntentProfile {
    /// EEVDF 虚拟截止期偏移（负 = 更早截止期）；v1/v2 唯一硬效应列。
    pub latency_nice: i8,
    /// AI ④类动作的带内包络下界（min/max 夹紧——包络是内核执法，不信任
    /// 注入方自律；同 `policy.rs` ±WEIGHT_MAX 夹紧纪律）。
    pub weight_floor: u32,
    /// 包络上界。
    pub weight_ceiling: u32,
    /// 能效倾向（perf / efficiency）。
    pub epp: EppHint,
    /// 频率下限（0..1024，喂 PM/DVFS；0 = 无下限请求）。
    pub uclamp_min: u16,
    /// 频率上限（0..1024；1024 = 不设限）。
    pub uclamp_max: u16,
    /// 定时器可顺延量（纳秒；LC 档 = 0 exact——闹钟/心率告警绝不顺延）。
    pub timer_slack_ns: u64,
    /// 是否可进 maintenance 组（0x04 BACKGROUND 门控；0x05 恒 true）。
    pub maintenance_eligible: bool,
    // PC 表追加列（v2+）：capacity 放置 hint；手表表无此列（单核无选位）。
    // v1 不落列（形状纪律：表示法定、实现 v2+）——放置倾向先由 epp+uclamp
    // 软参数间接表达。
}

/// 表行数（六档全行——含 0x05 MAINTENANCE 保守行）。
pub const INTENT_PROFILE_ROWS: usize = 6;

// ---- 手表表（v1 绑定——QEMU virt 单核主验）----

/// **手表表**：单核、无 capacity 放置列；省电是身份支柱——非 LC/交互档
/// EPP 全 efficiency、uclamp_max 逐档收紧。数值域 = §3.8 设计（latency-nice
/// -20..19、weight 1..1024、uclamp 0..1024、LC slack=0 exact）。
pub const WATCH_PROFILE_TABLE: [IntentProfile; INTENT_PROFILE_ROWS] = [
    // [0] 0x00 UNSPECIFIED——XNU 式最高可用档：不设限（可被角色表/分类器
    // 定档，pledge 豁免）。保守行：不给提前截止期、不抬频、不设限频——
    // 「未声明」≠「声明了高档」，优势由后续定档授予而非缺省借出。
    IntentProfile {
        latency_nice: 0,
        weight_floor: 1,
        weight_ceiling: 1024,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: SLACK_INTERACTIVE_DEFAULT_NS,
        maintenance_eligible: false,
    },
    // [1] 0x01 LATENCY_CRITICAL——Job 级授权（manifest/特权）；slack=0
    // exact（day-one 逃生舱：闹钟/心率告警绝不顺延）；AI 屏蔽（#42）。
    IntentProfile {
        latency_nice: LATENCY_NICE_MIN, // -20：最早截止期
        weight_floor: 512,
        weight_ceiling: 1024,
        epp: EppHint::Perf,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: 0, // exact——LC 档铁律
        maintenance_eligible: false,
    },
    // [2] 0x02 INTERACTIVE——交互。
    IntentProfile {
        latency_nice: -8,
        weight_floor: 256,
        weight_ceiling: 1024,
        epp: EppHint::Perf,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: 10_000_000, // 10ms
        maintenance_eligible: false,
    },
    // [3] 0x03 UTILITY——推断缺省落点。
    IntentProfile {
        latency_nice: 6,
        weight_floor: 64,
        weight_ceiling: 512,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: SLACK_INTERACTIVE_DEFAULT_NS,
        maintenance_eligible: false,
    },
    // [4] 0x04 BACKGROUND——maintenance_eligible 门控开。
    IntentProfile {
        latency_nice: LATENCY_NICE_MAX, // 19：最晚截止期
        weight_floor: 1,
        weight_ceiling: 64,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 512, // 收紧（省电优先）
        timer_slack_ns: 500_000_000, // 500ms
        maintenance_eligible: true,
    },
    // [5] 0x05 MAINTENANCE——编码冻结、v1 不启用（用户裁决 2026-10-16）；
    // 保守行先行（比 BACKGROUND 更紧：weight 顶 16、uclamp_max 256）。
    IntentProfile {
        latency_nice: LATENCY_NICE_MAX,
        weight_floor: 1,
        weight_ceiling: 16,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 256,
        timer_slack_ns: MAINTENANCE_SLACK_NS,
        maintenance_eligible: true,
    },
];

// ---- 手机/PC 表（v2+ 形状占位——capacity 放置列随 EEVDF/v2 落列）----
// v1 手表表单表绑定；PC 表差异面 = capacity hint 列 + 交互档 uclamp_min 非 0
// （大小核放置：LC/交互钉大核）——列未落，表不建（禁半列空表）。

// ---- 回退保守表（模型不可信回退通道；宁可不省、不可错放）----

/// **回退保守表**：全部列取最保守值——latency_nice 全 0（不提前）、weight
/// 包络全 1..1024（不放大）、EPP 全 efficiency（省电方向）、uclamp 全
/// 0..1024（不抬频不设限）、slack 50ms 缺省、maintenance_eligible 仅
/// 0x04/0x05 档 true（不跨档放行）。
///
/// 唯一保留档间差异 = LC 的 slack=0 exact 与 maintenance 门控位——两者是
/// **已冻结语义**（编码冻结 + #42 用户裁决），回退通道撤语义面 = 冻结面
/// 漂移；回退只撤「数值优势」，不撤「语义铁律」。
pub const FALLBACK_PROFILE_TABLE: [IntentProfile; INTENT_PROFILE_ROWS] = [
    // [0] 0x00 UNSPECIFIED
    IntentProfile {
        latency_nice: 0,
        weight_floor: 1,
        weight_ceiling: 1024,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: SLACK_INTERACTIVE_DEFAULT_NS,
        maintenance_eligible: false,
    },
    // [1] 0x01 LATENCY_CRITICAL——回退不借出 LC 数值优势（截止期不提前、
    // EPP 回 efficiency、包络不放大）；slack=0 exact 保留（语义铁律非优势）。
    IntentProfile {
        latency_nice: 0,
        weight_floor: 1,
        weight_ceiling: 1024,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: 0,
        maintenance_eligible: false,
    },
    // [2] 0x02 INTERACTIVE
    IntentProfile {
        latency_nice: 0,
        weight_floor: 1,
        weight_ceiling: 1024,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: SLACK_INTERACTIVE_DEFAULT_NS,
        maintenance_eligible: false,
    },
    // [3] 0x03 UTILITY
    IntentProfile {
        latency_nice: 0,
        weight_floor: 1,
        weight_ceiling: 1024,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: SLACK_INTERACTIVE_DEFAULT_NS,
        maintenance_eligible: false,
    },
    // [4] 0x04 BACKGROUND
    IntentProfile {
        latency_nice: 0,
        weight_floor: 1,
        weight_ceiling: 1024,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: SLACK_INTERACTIVE_DEFAULT_NS,
        maintenance_eligible: true,
    },
    // [5] 0x05 MAINTENANCE
    IntentProfile {
        latency_nice: 0,
        weight_floor: 1,
        weight_ceiling: 1024,
        epp: EppHint::Efficiency,
        uclamp_min: 0,
        uclamp_max: 1024,
        timer_slack_ns: SLACK_INTERACTIVE_DEFAULT_NS,
        maintenance_eligible: true,
    },
];

// ---- 查表与执法纯函数 ----

/// 单表行自洽执法（表数据下界）：值域 + 包络序（floor ≤ ceiling）+ uclamp
/// 序（min ≤ max）。选表/换表（运行时更新路径）先过本判据——违表 = 编译期
/// `const` 数据错，运行期新表拒装（保守拒，不 panic）。
pub fn profile_row_valid(p: &IntentProfile) -> bool {
    (LATENCY_NICE_MIN..=LATENCY_NICE_MAX).contains(&p.latency_nice)
        && p.weight_floor >= WEIGHT_FLOOR_MIN
        && p.weight_ceiling <= WEIGHT_CEILING_MAX
        && p.weight_floor <= p.weight_ceiling
        && p.uclamp_min <= UCLAMP_MAX
        && p.uclamp_max <= UCLAMP_MAX
        && p.uclamp_min <= p.uclamp_max
}

/// 整表执法：六行全过 [`profile_row_valid`] 才可装（长度由类型钉死）。
pub fn profile_table_valid(t: &[IntentProfile]) -> bool {
    t.len() == INTENT_PROFILE_ROWS && t.iter().all(profile_row_valid)
}

/// intent u8 → 档位行索引（0x00–0x05 直映；0x06 起保留域 None——与
/// `intent_valid` 同口径，入口治理单源不二写：查表侧只收合法键）。
pub const fn intent_index(v: u8) -> Option<usize> {
    if v <= 0x05 {
        Some(v as usize)
    } else {
        None
    }
}

/// 查表主入口：intent u8 → 该档 [`IntentProfile`]。
///
/// 保留域（0x06 起）与 `None` 表 = **回落回退表同行**（`intent_valid` 入口
/// 已拒，这里防御性双保险——查表永不 panic、永不越界，超顶回落保守方向）。
pub fn lookup(t: &[IntentProfile], v: u8) -> IntentProfile {
    match intent_index(v) {
        Some(i) if i < t.len() => t[i],
        _ => FALLBACK_PROFILE_TABLE[intent_index(v).unwrap_or(0)],
    }
}

/// `lookup` 的绑定简写：手表表（v1 绑定表）。
pub fn lookup_watch(v: u8) -> IntentProfile {
    lookup(&WATCH_PROFILE_TABLE, v)
}

/// AI/策略注入的 weight 夹紧执法：夹进档位包络 `[weight_floor, weight_ceiling]`
/// （包络是内核执法——#42 AI ④类动作带内；同 `policy.rs` 夹紧纪律）。
pub fn clamp_weight(p: &IntentProfile, w: u32) -> u32 {
    w.max(p.weight_floor).min(p.weight_ceiling)
}

/// 运行时更新通道的保守门（§3.8「支持运行时更新」）：候选表过
/// [`profile_table_valid`] 则整表拷进调用方自有的安装槽，否则槽位整体回落
/// [`FALLBACK_PROFILE_TABLE`]（宁可不省、不可错放）；返回候选是否被接受。
/// v1 无运行时安装点（编译期绑定手表表）——门函数形状先落，安装点随启动
/// 配置加载落地；纯函数无内部可变态，host 直测。
pub fn sanitize(
    candidate: &[IntentProfile],
    slot: &mut [IntentProfile; INTENT_PROFILE_ROWS],
) -> bool {
    if profile_table_valid(candidate) {
        slot.copy_from_slice(candidate);
        true
    } else {
        *slot = FALLBACK_PROFILE_TABLE;
        false
    }
}

// ---- 宿主单元测试（`#![cfg_attr(not(test), no_std)]` 面；QEMU ktest 面
// 由 kernel 侧消费时另行钉板，本面覆盖纯函数全部分支）----

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码冻结字面量（钉板纪律：测试断言直写 0x00–0x05，与
    /// `berry-manifest` `intent_discriminants_pinned` 同款——防常量定义
    /// 本身漂移；单源 `sched/src/intent.rs`，本 crate 不复制常量）。
    const UNSPECIFIED: u8 = 0x00;
    const LATENCY_CRITICAL: u8 = 0x01;
    const INTERACTIVE: u8 = 0x02;
    const UTILITY: u8 = 0x03;
    const BACKGROUND: u8 = 0x04;
    const MAINTENANCE: u8 = 0x05;

    /// 行索引直映 + 保留域 None（与 intent_valid 同口径）。
    #[test]
    fn intent_index_maps() {
        for v in 0x00u8..=0x05 {
            assert_eq!(intent_index(v), Some(v as usize));
        }
        for v in [0x06u8, 0x0F, 0x10, 0x7F, 0x80, 0xFF] {
            assert_eq!(intent_index(v), None, "保留域 {:#x} 须 None", v);
        }
    }

    /// 两张表整表合法（编译期 const 数据的自洽执法）。
    #[test]
    fn builtin_tables_valid() {
        assert!(profile_table_valid(&WATCH_PROFILE_TABLE));
        assert!(profile_table_valid(&FALLBACK_PROFILE_TABLE));
    }

    /// 手表表档位序单调性：latency_nice 在**声明档区间**（0x01–0x05）随档位
    /// 号单调不降（越小档越高 → 截止期偏移逐档推后）；0x00 UNSPECIFIED 例外
    /// ——保守行钉 0（不借出提前截止期优势），不参与声明档单调序。
    #[test]
    fn watch_latency_nice_monotonic() {
        let t = &WATCH_PROFILE_TABLE;
        for i in 2..t.len() {
            assert!(
                t[i - 1].latency_nice <= t[i].latency_nice,
                "声明档 {} → {} latency_nice 须单调不降",
                i - 1,
                i
            );
        }
        assert_eq!(t[0].latency_nice, 0, "UNSPECIFIED 保守行钉 0");
        assert_eq!(t[1].latency_nice, LATENCY_NICE_MIN);
        assert_eq!(t[5].latency_nice, LATENCY_NICE_MAX);
    }

    /// weight 包络逐档收窄序：ceiling 单调不升（0x01 顶 1024 → 0x05 顶 16）。
    #[test]
    fn watch_weight_ceiling_funnel() {
        let t = &WATCH_PROFILE_TABLE;
        for i in 1..t.len() {
            assert!(
                t[i - 1].weight_ceiling >= t[i].weight_ceiling,
                "档 {} → {} weight_ceiling 须单调不升",
                i - 1,
                i
            );
        }
        assert_eq!(t[1].weight_ceiling, WEIGHT_CEILING_MAX);
    }

    /// LC 档铁律三件：slack=0 exact、latency_nice=MIN、AI 屏蔽面
    /// （maintenance_eligible=false）。
    #[test]
    fn lc_row_ironclad() {
        let lc = lookup_watch(LATENCY_CRITICAL);
        assert_eq!(lc.timer_slack_ns, 0);
        assert_eq!(lc.latency_nice, LATENCY_NICE_MIN);
        assert!(!lc.maintenance_eligible);
        assert_eq!(lc.epp, EppHint::Perf);
    }

    /// maintenance 门控位：仅 0x04/0x05 true，其余全 false（手表表 + 回退表）。
    #[test]
    fn maintenance_gate_only_low_tiers() {
        for t in [&WATCH_PROFILE_TABLE, &FALLBACK_PROFILE_TABLE] {
            for (i, row) in t.iter().enumerate() {
                assert_eq!(
                    row.maintenance_eligible,
                    i >= 4,
                    "表档 {} maintenance_eligible 须 {}",
                    i,
                    i >= 4
                );
            }
        }
    }

    /// 回退保守表 = 全列最保守：latency_nice 全 0、包络全 1..1024、EPP 全
    /// efficiency、uclamp 全 0..1024（LC slack=0 例外——语义铁律保留）。
    #[test]
    fn fallback_all_conservative() {
        for (i, row) in FALLBACK_PROFILE_TABLE.iter().enumerate() {
            assert_eq!(row.latency_nice, 0, "回退档 {} latency_nice", i);
            assert_eq!(row.weight_floor, 1);
            assert_eq!(row.weight_ceiling, 1024);
            assert_eq!(row.epp, EppHint::Efficiency);
            assert_eq!(row.uclamp_min, 0);
            assert_eq!(row.uclamp_max, 1024);
            let slack_is_ironclad = i == 1; // LC slack=0 语义铁律
            if !slack_is_ironclad {
                assert_eq!(row.timer_slack_ns, SLACK_INTERACTIVE_DEFAULT_NS);
            } else {
                assert_eq!(row.timer_slack_ns, 0);
            }
        }
    }

    /// 查表回落防御：保留域/野值/空表 → 回退表同行、永不 panic。
    #[test]
    fn lookup_falls_back_on_bad_key() {
        for v in [0x06u8, 0x42, 0xFF] {
            assert_eq!(
                lookup(&WATCH_PROFILE_TABLE, v),
                FALLBACK_PROFILE_TABLE[0],
                "野键 {:#x} 须回落回退表 [0]",
                v
            );
        }
        assert_eq!(lookup(&[], UTILITY), FALLBACK_PROFILE_TABLE[3]);
    }

    /// 合法键逐档直映行内容（含 0x05 保留档保守行）。
    #[test]
    fn lookup_watch_all_tiers() {
        let t = &WATCH_PROFILE_TABLE;
        assert_eq!(lookup_watch(UNSPECIFIED), t[0]);
        assert_eq!(lookup_watch(LATENCY_CRITICAL), t[1]);
        assert_eq!(lookup_watch(INTERACTIVE), t[2]);
        assert_eq!(lookup_watch(UTILITY), t[3]);
        assert_eq!(lookup_watch(BACKGROUND), t[4]);
        assert_eq!(lookup_watch(MAINTENANCE), t[5]);
    }

    /// clamp_weight 三带：低于 floor 抬到 floor、高于 ceiling 压到 ceiling、
    /// 带内原样（含恰在边界值）。
    #[test]
    fn clamp_weight_envelope() {
        let bg = lookup_watch(BACKGROUND); // floor 1, ceiling 64
        assert_eq!(clamp_weight(&bg, 0), 1);
        assert_eq!(clamp_weight(&bg, 1), 1);
        assert_eq!(clamp_weight(&bg, 64), 64);
        assert_eq!(clamp_weight(&bg, 65), 64);
        assert_eq!(clamp_weight(&bg, u32::MAX), 64);
        let lc = lookup_watch(LATENCY_CRITICAL); // 512..1024
        assert_eq!(clamp_weight(&lc, 0), 512);
        assert_eq!(clamp_weight(&lc, 511), 512);
        assert_eq!(clamp_weight(&lc, 512), 512);
        assert_eq!(clamp_weight(&lc, 1024), 1024);
        assert_eq!(clamp_weight(&lc, 1025), 1024);
    }

    /// profile_row_valid 负面：包络倒挂、uclamp 倒挂、值域越界逐列拒。
    #[test]
    fn row_valid_rejects() {
        let good = lookup_watch(UTILITY);
        assert!(profile_row_valid(&good));
        let mut bad = good;
        bad.weight_floor = bad.weight_ceiling + 1; // 包络倒挂
        assert!(!profile_row_valid(&bad));
        let mut bad = good;
        bad.uclamp_min = bad.uclamp_max + 1; // uclamp 倒挂
        assert!(!profile_row_valid(&bad));
        let mut bad = good;
        bad.latency_nice = LATENCY_NICE_MIN - 1; // 越负界
        assert!(!profile_row_valid(&bad));
        let mut bad = good;
        bad.latency_nice = LATENCY_NICE_MAX + 1; // 越正界
        assert!(!profile_row_valid(&bad));
        let mut bad = good;
        bad.weight_ceiling = WEIGHT_CEILING_MAX + 1; // 包络越上界
        assert!(!profile_row_valid(&bad));
        let mut bad = good;
        bad.weight_floor = 0; // 越下界（0 权重无意义）
        assert!(!profile_row_valid(&bad));
        let mut bad = good;
        bad.uclamp_max = UCLAMP_MAX + 1; // uclamp 越界
        assert!(!profile_row_valid(&bad));
    }

    /// profile_table_valid 负面：长度错/单行坏整表拒。
    #[test]
    fn table_valid_rejects() {
        assert!(!profile_table_valid(&[]));
        assert!(!profile_table_valid(&WATCH_PROFILE_TABLE[..5]));
        let mut t = WATCH_PROFILE_TABLE;
        t[2].uclamp_min = 1025;
        assert!(!profile_table_valid(&t));
    }

    /// sanitize 保守门：合法候选整表入槽返回 true；非法候选（长度错/单行
    /// 坏）槽位整体回落回退表返回 false（宁可不省、不可错放）。
    #[test]
    fn sanitize_conservative_gate() {
        let mut slot = FALLBACK_PROFILE_TABLE;
        assert!(sanitize(&WATCH_PROFILE_TABLE, &mut slot));
        assert_eq!(slot, WATCH_PROFILE_TABLE);
        let mut slot = WATCH_PROFILE_TABLE;
        assert!(!sanitize(&[], &mut slot));
        assert_eq!(slot, FALLBACK_PROFILE_TABLE);
        let mut bad = WATCH_PROFILE_TABLE;
        bad[0].latency_nice = 100; // 非法
        let mut slot = WATCH_PROFILE_TABLE;
        assert!(!sanitize(&bad, &mut slot));
        assert_eq!(slot, FALLBACK_PROFILE_TABLE);
    }

    /// 手表表省电梯度抽查：非 LC/交互档 EPP=Efficiency、uclamp_max 逐档
    /// 不升（1024 → 512 → 256）。
    #[test]
    fn watch_power_gradient() {
        let t = &WATCH_PROFILE_TABLE;
        assert_eq!(t[1].epp, EppHint::Perf);
        assert_eq!(t[2].epp, EppHint::Perf);
        for i in [0, 3, 4, 5] {
            assert_eq!(t[i].epp, EppHint::Efficiency, "档 {} EPP", i);
        }
        assert!(t[3].uclamp_max >= t[4].uclamp_max);
        assert!(t[4].uclamp_max >= t[5].uclamp_max);
        assert_eq!(t[5].uclamp_max, 256);
    }
}
