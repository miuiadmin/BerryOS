//! berry-slackface —— timer slack 聚合判定律(host 纯函数面,零依赖)。
//!
//! **设计锚**:docs/调度器设计-v1.1.md §3.8 :155「timer 原语带 slack+policy
//! 三元组(学 `zx_timer_set`,v1 day-one)——intent 的 timer_slack_ns 列的
//! 执行载体,也是省电第二杠杆(唤醒合并)的内核机制」。01 号对账
//! (research/调度彻底计划/01-调度现状全景与在途线收编.md :30/:222 硬缺口
//! #9)实测该 day-one 承诺「代码零命中」——本 crate = 承诺的**判定律纯面**
//! 先行:「查表键先于消费者落地」同款纪律(sched/src/intent.rs:4-6 先例)。
//!
//! **边界声明**:sched 接线归接线窗——sched 域 owner 在途(kernel/sched/
//! 壳族/launcher/svcmgr/justfile 零碰)。本件只立律不动机制:唤醒合并在
//! 内核定时器路径的落线、bootparams 第三键(`timer_slack`)解析、
//! `zx_timer_set` 形三元组 syscall 面,全部归后续接线刀。
//!
//! **ADR 锚**:#42 intent 两段式(docs/架构决策.md:51)——intent = u8 查表键,
//! timer slack 是 IntentProfile 六档列之一(§3.8 :126「LC=0(exact 逃生舱
//! day-one 保留:闹钟/心率告警绝不顺延)」、:146 LC 行 slack=0 exact);
//! #58 单一词汇表归一(docs/架构决策.md:67)——六档编码冻结单源 =
//! berry-intface(intface/src/lib.rs:42-56;sched/src/intent.rs:30-34
//! re-export)。
//!
//! **Linux 对照**(外部事实,man7 PR_SET_TIMERSLACK(2const),2026-11 取证):
//! ① 每线程「default/current」双 slack 值,init 缺省 50000ns(50μs);
//! ② slack 分组邻近到期——「expirations may be up to the specified number
//! of nanoseconds late (but will never expire early)」(**只迟不早**——本律
//! L1 late-only 同构);③ 分组省电动因 = 「minimizing CPU wake-ups」
//! (省电第二杠杆的量产同款);④ RT 调度线程免 slack(本律 LC 档 slack=0
//! exact 逃生舱同位);⑤ fork 继承/execve 保留。kernel docs hrtimers.rst
//! 另证:timeout 类定时器「granularity and precision tradeoffs」可接受——
//! slack 是用精度换唤醒的合法交易。Zircon `zx_timer_set` 三元组
//! (deadline, slack_amount, slack_policy CENTER/LATE/EARLY)是设计 §3.8
//! 点名学习对象;本律取 **LATE 单形**(只迟不早),CENTER 形留接线窗裁。
//!
//! ## 判定律四件
//!
//! 1. **任务 slack 域**(①):intent 档 → 可顺延量纳秒。六档数值 =
//!    iprofile 手表表 `timer_slack_ns` 列的**零依赖镜像**(见下节同步义务)。
//!    档间序:LC=0 < INTERACTIVE=10ms < UNSPECIFIED=UTILITY=50ms <
//!    BACKGROUND=500ms < MAINTENANCE=10s。
//! 2. **聚合分桶**(②):同窗定时器批量唤醒。按 deadline 稳定排序后贪心
//!    扫描开桶,桶持两不变量:`max_d`(成员最迟 deadline = late-only 下界)
//!    与 `min_e`(成员最紧预算 `min(d_i+s_i)` = 最迟许可醒点)。候选 j
//!    进桶判据 `d_j ≤ min_e`(j 的 deadline 不晚于桶最迟许可醒点——否则
//!    纳入 j 必须把醒点推过某成员预算,违 L2)。贪心非最优装箱(区间图
//!    团覆盖),判定律取确定性贪心——可证不变量优先于最优性。
//! 3. **最小松弛保持**(③·三条可证律):
//!    - L1 **late-only**:∀成员 `fire ≥ d_i`(永不提前——Linux slack 同构)。
//!    - L2 **最紧 slack 不越**:∀成员 `fire − d_i ≤ s_i`(聚合窗永不越任何
//!      成员自身预算;**交互零牺牲**即推论——桶内 INTERACTIVE 成员把桶钉在
//!      其 +10ms 内,LC 成员把桶钉在其 deadline 恰点)。醒点取
//!      `fire = max(max_d, floor_align(min_e, grid))`——窗内最迟 + 网格
//!      向下对齐(对齐只可能提前,永不破 L2;对齐落穿 `max_d` 时回退
//!      `max_d`——**律赢于对齐**)。网格对齐意图:周期定时器跨装配相位
//!      收敛到公共醒点边界(Linux 定时器对齐到 jiffy 粒度同思想)。
//!    - L3 **防饥饿上界**:桶跨度 `fire − d_first ≤ s_first`(首成员 =
//!      开桶者)。证明:fire ≤ min_e ≤ d_first + s_first。松弛永远按**原始
//!      锚点 deadline**计,绝不因聚合重锚——顺延逐桶有界,饿不死。
//! 4. **policy 三元组**(④):`(slice, intent_default, timer_slack)` 形
//!    (G5 bootparams `sched_slice_hz`/`intent_default` 两键之第三键补全;
//!    slice 域 [16,1024] 镜像 kernel/src/bootparams.rs:62-63,
//!    intent_default 域 = 编码冻结 0x00..=0x05,`timer_slack_ns` = 系统
//!    缺省 timer slack——timer 原语不带显式 slack 且 intent 行未解析时的
//!    兜底,与 Linux 线程「default」值同位)。
//!
//! ## 零依赖镜像(同步义务)
//!
//! 本 crate 零依赖(任务纪律),以下三处为**镜像而非单源**,升版同步义务
//! 在此注记(接线窗若解除零依赖,应改 path 依赖消镜像):
//! - slack 六档值 ← iprofile/src/lib.rs:102 `WATCH_PROFILE_TABLE`
//!   `timer_slack_ns` 列(:113/:125/:136/:147/:158/:170 六值,W3-Q1);
//! - intent 编码冻结字面 ← intface/src/lib.rs:42-56;
//! - slice_hz 域 [16,1024] ← kernel/src/bootparams.rs:62-63。
//!
//! **数值推断标注**:①任务书括号序「INTERACTIVE 最小/BG 最大/MAINTENANCE
//! 中」与单源不符——iprofile 实序 MAINTENANCE(10s)> BACKGROUND(500ms),
//! 依一事实一归属从单源,特此标注差异;②推荐网格 [`GRID_RECOMMEND_NS`]
//! (1ms)为推断值(真机醒潜势/功耗计测后校准);③桶醒点取「窗内最迟」
//! (latest-first,合并机会最大化——省电第二杠杆取向)为设计取舍非外部
//! 事实,Linux 实现按 expiry 向上取整到公共边界,思想同源。

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::vec::Vec;

// ---- ① 任务 slack 域(intent 档映射;数值 = iprofile 零依赖镜像)----

/// LATENCY_CRITICAL(0x01)档 slack——0 = exact(闹钟/心率告警绝不顺延;
/// §3.8 :126/:146 冻结语义,Linux RT 免 slack同位)。
pub const SLACK_LATENCY_CRITICAL_NS: u64 = 0;
/// INTERACTIVE(0x02)档 slack——10ms(iprofile 手表表 :136)。
pub const SLACK_INTERACTIVE_NS: u64 = 10_000_000;
/// UNSPECIFIED(0x00)档 slack——50ms(iprofile 手表表 :113;XNU 式最高
/// 可用档的保守行,不借出 exact 优势)。
pub const SLACK_UNSPECIFIED_NS: u64 = 50_000_000;
/// UTILITY(0x03)档 slack——50ms(iprofile 手表表 :147;推断缺省落点)。
pub const SLACK_UTILITY_NS: u64 = 50_000_000;
/// BACKGROUND(0x04)档 slack——500ms(iprofile 手表表 :158)。
pub const SLACK_BACKGROUND_NS: u64 = 500_000_000;
/// MAINTENANCE(0x05)档 slack——10s(iprofile 手表表 :170;**全域最宽**,
/// 任务书括号序与本行不符处以单源为准,头注「数值推断标注」①)。
pub const SLACK_MAINTENANCE_NS: u64 = 10_000_000_000;

/// intent u8 → 档位 slack(纳秒)。编码冻结镜像(0x00..=0x05 直映,
/// 单源 = intface `intent_valid` 同律);0x06 起保留域 `None`——入口
/// `-EINVAL` 面(与 options 纪律一致,§3.8 编码冻结表)。
pub fn slack_for_intent(intent: u8) -> Option<u64> {
    match intent {
        0x00 => Some(SLACK_UNSPECIFIED_NS),
        0x01 => Some(SLACK_LATENCY_CRITICAL_NS),
        0x02 => Some(SLACK_INTERACTIVE_NS),
        0x03 => Some(SLACK_UTILITY_NS),
        0x04 => Some(SLACK_BACKGROUND_NS),
        0x05 => Some(SLACK_MAINTENANCE_NS),
        _ => None,
    }
}

// ---- ② 聚合分桶(同窗定时器批量唤醒)----

/// 推荐网格对齐粒度(1ms;**推断值**——头注「数值推断标注」②,接线窗
/// 真机校准)。`grid_ns = 0` 关闭对齐(醒点 = 最迟许可点原值)。
pub const GRID_RECOMMEND_NS: u64 = 1_000_000;

/// 单定时器仲裁请求:`deadline_ns` = 绝对到期时刻(vct 域;§4.1 绝对截止
/// 纪律——醒后按硬件计数器重算,本律不涉重编程);`slack_ns` = 可顺延量
/// (调用方经 [`slack_for_intent`] 解析或显式携带)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerReq {
    pub deadline_ns: u64,
    pub slack_ns: u64,
}

impl TimerReq {
    /// 预算上界 `d + s`(checked——u64 溢出 = 坏输入,调用面整批拒)。
    #[inline]
    fn budget_end(&self) -> Option<u64> {
        self.deadline_ns.checked_add(self.slack_ns)
    }
}

/// 聚合桶:一次硬件唤醒承载的全部成员。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    /// 桶醒点(L1/L2/L3 三律约束下的取值,见 [`coalesce`])。
    pub fire_ns: u64,
    /// 许可窗宽 = 最迟许可醒点 − 最迟 deadline(`saturating_sub`——
    /// 桶含 exact 成员时窗塌缩为 0)。
    pub window_ns: u64,
    /// 桶跨度 = 醒点 − 首成员 deadline(L3 防饥饿上界的实侧值,
    /// 恒 ≤ 首成员 slack)。
    pub span_ns: u64,
    /// 成员(输入下标,按进桶序;首元素 = 开桶者)。
    pub members: Vec<usize>,
}

/// 聚合结果 + 收益账(唤醒次数降比)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coalesced {
    pub buckets: Vec<Bucket>,
    /// 输入定时器数。
    pub timers_in: usize,
    /// 聚合前唤醒数 = 每定时器一醒(`timers_in` 同值;对账面单列)。
    pub wakeups_before: usize,
    /// 聚合后唤醒数 = 桶数。
    pub wakeups_after: usize,
    /// 唤醒节省整数百分比(截断向零;`(before−after)×100/before`,
    /// 空输入 = 0。整数不动 f64——内核可复用同式)。
    pub wakeups_saved_pct: u32,
}

/// 网格向下对齐(律赢于对齐:对齐结果低于 late-only 地板时由调用处钳回,
/// 本函数只做 `x − x % grid`;`grid == 0` = 原值直返)。
#[inline]
fn floor_align(x: u64, grid: u64) -> u64 {
    if grid == 0 {
        x
    } else {
        x - (x % grid)
    }
}

/// timer slack 聚合判定律(纯函数;坏输入整批拒 `None`):
///
/// - 输入按 `deadline_ns` 稳定排序(同 deadline 保输入序——确定性)后
///   贪心开桶;进桶判据与三律证明见模块注 ②/③。
/// - 坏输入拒面:任一 `deadline_ns < now_ns`(往昔锚点 = 违 §4.1 绝对
///   截止纪律的装配错)或任一 `d + s` u64 溢出 → `None` 整批拒;
///   空输入 = 合法平凡件(`Some` 空结果)。
///
/// `now_ns` 仅作往昔拒,不参与桶几何(醒点永远由 deadline/slack 决定,
/// 与「当前时刻」无关——重编程纪律归接线窗)。
pub fn coalesce(reqs: &[TimerReq], now_ns: u64, grid_ns: u64) -> Option<Coalesced> {
    // 前置执法:往昔锚点 / 预算溢出(一次扫全批,零改动拒)。
    for r in reqs {
        if r.deadline_ns < now_ns {
            return None;
        }
        r.budget_end()?;
    }

    // 确定性排序视图:下标按 (deadline, 输入序) 排。
    let mut order: Vec<usize> = (0..reqs.len()).collect();
    order.sort_unstable_by_key(|&i| (reqs[i].deadline_ns, i));

    let mut buckets: Vec<Bucket> = Vec::new();
    // 开桶中状态:max_d = late-only 下界;min_e = 最迟许可醒点(L2 机构)。
    let mut members: Vec<usize> = Vec::new();
    let mut max_d: u64 = 0;
    let mut min_e: u64 = 0;
    let mut open = false;

    let close = |members: &mut Vec<usize>, max_d: u64, min_e: u64, out: &mut Vec<Bucket>| {
        if members.is_empty() {
            return;
        }
        let fire = max_d.max(floor_align(min_e, grid_ns)); // 律赢于对齐
        out.push(Bucket {
            fire_ns: fire,
            window_ns: min_e.saturating_sub(max_d),
            span_ns: fire - reqs[members[0]].deadline_ns,
            members: core::mem::take(members),
        });
    };

    for &i in &order {
        let r = reqs[i];
        let end = r.budget_end().expect("前置已扫溢出");
        if open && r.deadline_ns <= min_e {
            // 进桶(判据 ②):加入后两不变量单调收紧——max_d 只增、min_e
            // 只减,故醒点永不因新成员后推(防饥饿的机构面)。
            members.push(i);
            max_d = max_d.max(r.deadline_ns);
            min_e = min_e.min(end);
        } else {
            close(&mut members, max_d, min_e, &mut buckets);
            members.push(i);
            max_d = r.deadline_ns;
            min_e = end;
            open = true;
        }
    }
    close(&mut members, max_d, min_e, &mut buckets);

    let before = reqs.len();
    let after = buckets.len();
    let saved_pct = if before == 0 {
        0
    } else {
        (((before - after) as u64 * 100) / before as u64).min(u32::MAX as u64) as u32
    };
    Some(Coalesced {
        buckets,
        timers_in: before,
        wakeups_before: before,
        wakeups_after: after,
        wakeups_saved_pct: saved_pct,
    })
}

/// 三律自检(L1/L2/L3——接线窗遥测断言/金丝雀可复用;host 测试逐桶对拍)。
/// 输入须与 [`coalesce`] 同批(成员下标域一致);任一违律 → `false`。
pub fn laws_hold(reqs: &[TimerReq], out: &Coalesced) -> bool {
    let mut seen = 0usize;
    for b in &out.buckets {
        if b.members.is_empty() {
            return false;
        }
        let first = reqs[b.members[0]];
        // L3 防饥饿:桶跨度 ≤ 开桶者 slack。
        if b.fire_ns - first.deadline_ns > first.slack_ns {
            return false;
        }
        for &i in &b.members {
            let r = reqs[i];
            // L1 late-only:永不提前。
            if b.fire_ns < r.deadline_ns {
                return false;
            }
            // L2 最紧 slack 不越:顺延不越自身预算。
            if b.fire_ns - r.deadline_ns > r.slack_ns {
                return false;
            }
        }
        seen += b.members.len();
    }
    seen == out.timers_in
}

// ---- ④ policy 三元组((slice, intent_default, timer_slack) 形)----

/// slice 档下界(镜像 kernel/src/bootparams.rs:62 `SLICE_HZ_MIN`——
/// 同步义务见模块注「零依赖镜像」)。
pub const SLICE_HZ_MIN: u32 = 16;
/// slice 档上界(同上镜像 :63)。
pub const SLICE_HZ_MAX: u32 = 1024;

/// 调度 policy 三元组(设计 §3.8 :155「slack+policy 三元组」的
/// bootparams 形——G5 两键 `sched_slice_hz`/`intent_default` 之第三键
/// `timer_slack` 补全;`zx_timer_set` 形 (deadline, slack_amount,
/// slack_policy) 是 syscall 面同语义异形,接线窗定 ABI)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyTriple {
    /// 时间片频率(周期数记账的换算基——2b-4 `slice_left` 消费;域
    /// [16,1024] 镜像 bootparams)。
    pub slice_hz: u32,
    /// 新任务缺省 intent(编码冻结域 0x00..=0x05;G5 `intent_default`
    /// 同名键)。
    pub intent_default: u8,
    /// 系统缺省 timer slack(timer 原语不带显式 slack 且 intent 行未解析
    /// 时的兜底;Linux 线程「default」值同位——PR_SET_TIMERSLACK(2const)
    /// 语义:current 显式、default 兜底。域 [0, 全域最宽 MAINTENANCE])。
    pub timer_slack_ns: u64,
}

/// 三元组整律执法:三键各自值域全过才可注入(保守拒,不 panic——
/// `profile_row_valid` 同款纪律)。
pub fn policy_triple_valid(p: &PolicyTriple) -> bool {
    (SLICE_HZ_MIN..=SLICE_HZ_MAX).contains(&p.slice_hz)
        && p.intent_default <= 0x05 // 编码冻结域(intent_valid 同律)
        && p.timer_slack_ns <= SLACK_MAINTENANCE_NS
}

/// slack 解析链(纯函数):intent 可得 → 域行(§3.8 :155「intent 的
/// timer_slack_ns 列的执行载体」);intent 不可得(`None`——timer 原语无
/// intent 语境,如内核内部定时器)→ 三元组系统缺省(Linux 线程
/// 「default/current」双值同构:current = 档位解析,default = 系统兜底);
/// 词表外值硬拒 `None`(不静默落兜底——`-EINVAL` 优先于缺省,与编码
/// 冻结表「内核拒绝」同口径)。三元组自身先过 [`policy_triple_valid`]
/// 是调用面前置,本函数不重复执法。
pub fn resolve_slack(p: &PolicyTriple, intent: Option<u8>) -> Option<u64> {
    match intent {
        None => Some(p.timer_slack_ns),
        Some(v) => slack_for_intent(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000_000;

    // ---- ① slack 域 ----

    /// 域映射全档钉板:档间序 LC=0 < INTERACTIVE=10ms < UNSPEC=UTILITY=50ms
    /// < BG=500ms < MAINT=10s(单源 = iprofile 手表表;MAINT 最宽——任务书
    /// 括号序差异处以单源为准,头注「数值推断标注」①)。
    #[test]
    fn slack_domain_tiers() {
        assert_eq!(slack_for_intent(0x00), Some(SLACK_UNSPECIFIED_NS));
        assert_eq!(slack_for_intent(0x01), Some(0));
        assert_eq!(slack_for_intent(0x02), Some(10 * MS));
        assert_eq!(slack_for_intent(0x03), Some(50 * MS));
        assert_eq!(slack_for_intent(0x04), Some(500 * MS));
        assert_eq!(slack_for_intent(0x05), Some(10_000 * MS));
        // 档间序:非零档里 INTERACTIVE 最小,MAINTENANCE 最大。
        assert!(SLACK_INTERACTIVE_NS < SLACK_UNSPECIFIED_NS);
        assert_eq!(SLACK_UNSPECIFIED_NS, SLACK_UTILITY_NS);
        assert!(SLACK_BACKGROUND_NS < SLACK_MAINTENANCE_NS);
        // 保留域硬拒:0x06..=0xFF 全 None(编码冻结表「内核拒绝」)。
        for v in [0x06u8, 0x0F, 0x10, 0xFF] {
            assert_eq!(slack_for_intent(v), None, "v={v:#x}");
        }
    }

    // ---- ② 分桶边界 ----

    /// 进桶判据边界:`d_j == min_e` 恰进(闭边界——醒点可同时满足两侧
    /// 预算);`d_j == min_e + 1` 开新桶。
    #[test]
    fn bucket_join_boundary() {
        let now = 1000;
        // A: d=10ms, s=5ms → 桶 min_e=15ms;B: d=15ms 恰等于 min_e → 进。
        let reqs = [
            TimerReq { deadline_ns: 10 * MS, slack_ns: 5 * MS },
            TimerReq { deadline_ns: 15 * MS, slack_ns: 5 * MS },
        ];
        let joined = coalesce(&reqs, now, 0).unwrap();
        assert_eq!(joined.wakeups_after, 1);
        assert_eq!(joined.buckets[0].fire_ns, 15 * MS); // = max_d = min_e
        assert_eq!(joined.buckets[0].members, vec![0, 1]);
        // A 顺延恰 5ms = 自身预算(L2 闭边界),B 零顺延。
        assert!(laws_hold(&reqs, &joined));

        // d=15ms+1ns → 越界开新桶。
        let reqs = [
            TimerReq { deadline_ns: 10 * MS, slack_ns: 5 * MS },
            TimerReq { deadline_ns: 15 * MS + 1, slack_ns: 5 * MS },
        ];
        let split = coalesce(&reqs, now, 0).unwrap();
        assert_eq!(split.wakeups_after, 2);
        assert_eq!(split.buckets[0].members, vec![0]);
        assert_eq!(split.buckets[1].members, vec![1]);
        assert_eq!(split.buckets[0].fire_ns, 15 * MS); // A 独享:fire=min_e
        assert!(laws_hold(&reqs, &split));
    }

    /// 网格对齐:窗内最迟点的向下对齐;对齐落穿 late-only 地板 → 回退
    /// `max_d`(律赢于对齐)。
    #[test]
    fn grid_alignment_law_wins() {
        // 窗 [max_d=10.9ms, min_e=11.95ms],grid=1ms → align(11.95)=11ms
        // ≥ max_d → fire=11ms(对齐生效,较最迟点提前 0.95ms,L2 更松)。
        let a = TimerReq { deadline_ns: 10_900_000, slack_ns: 1_050_000 };
        let out = coalesce(&[a], 0, 1 * MS).unwrap();
        assert_eq!(out.buckets[0].fire_ns, 11 * MS);
        assert!(laws_hold(&[a], &out));

        // 窗 [10.9ms, 10.95ms],grid=1ms → align(10.95)=10ms < max_d →
        // 回退 fire=max_d=10.9ms(未对齐原值)。
        let b = TimerReq { deadline_ns: 10_900_000, slack_ns: 50_000 };
        let out = coalesce(&[b], 0, 1 * MS).unwrap();
        assert_eq!(out.buckets[0].fire_ns, 10_900_000);
        assert!(laws_hold(&[b], &out));

        // grid=0 关闭对齐:fire = min_e 原值。
        let out = coalesce(&[a], 0, 0).unwrap();
        assert_eq!(out.buckets[0].fire_ns, 11_950_000);
    }

    // ---- 聚合收益(唤醒次数降比)----

    /// 数值例:10 个定时器散布 100ms 窗(全 INTERACTIVE 档 10ms slack),
    /// deadline [5,12,18,25,31,40,47,55,63,70]ms → 5 桶,唤醒 10→5,
    /// 降比 50%。手算轨迹(头注 ② 判据):b1{5,12}@15ms(b1 min_e=15,
    /// 18>15 关)、b2{18,25}@28、b3{31,40}@41、b4{47,55}@57、b5{63,70}@73。
    #[test]
    fn coalesce_benefit_ten_timers() {
        let ds = [5, 12, 18, 25, 31, 40, 47, 55, 63, 70];
        let reqs: Vec<TimerReq> = ds
            .iter()
            .map(|&d| TimerReq { deadline_ns: d * MS, slack_ns: SLACK_INTERACTIVE_NS })
            .collect();
        let out = coalesce(&reqs, 0, 0).unwrap();
        assert_eq!(out.timers_in, 10);
        assert_eq!(out.wakeups_before, 10);
        assert_eq!(out.wakeups_after, 5);
        assert_eq!(out.wakeups_saved_pct, 50);
        let fires: Vec<u64> = out.buckets.iter().map(|b| b.fire_ns).collect();
        assert_eq!(fires, vec![15 * MS, 28 * MS, 41 * MS, 57 * MS, 73 * MS]);
        // 桶成员对账 + 三律全批成立。
        let all: Vec<usize> = out.buckets.iter().flat_map(|b| b.members.iter().copied()).collect();
        assert_eq!(all, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert!(laws_hold(&reqs, &out));
    }

    /// 降比截断向零:3 定时器 2 桶 → (3−2)×100/3 = 33%(不进位)。
    #[test]
    fn saved_pct_truncates() {
        let reqs = [
            TimerReq { deadline_ns: 0, slack_ns: 10 * MS },
            TimerReq { deadline_ns: 5 * MS, slack_ns: 10 * MS },
            TimerReq { deadline_ns: 20 * MS, slack_ns: 10 * MS },
        ];
        let out = coalesce(&reqs, 0, 0).unwrap();
        assert_eq!(out.wakeups_after, 2);
        assert_eq!(out.wakeups_saved_pct, 33);
    }

    // ---- ③ 最紧 slack 不越 / 交互零牺牲 / LC exact ----

    /// 最紧 slack 不越:桶内混 BG(500ms)与 INT(10ms)——醒点被 INT 钉在
    /// 其 +10ms,BG 的宽预算不外溢给 INT(**交互零牺牲**原则)。
    #[test]
    fn tightest_slack_caps_bucket() {
        let reqs = [
            TimerReq { deadline_ns: 100 * MS, slack_ns: SLACK_INTERACTIVE_NS },
            TimerReq { deadline_ns: 105 * MS, slack_ns: SLACK_BACKGROUND_NS },
        ];
        let out = coalesce(&reqs, 0, 0).unwrap();
        assert_eq!(out.wakeups_after, 1);
        assert_eq!(out.buckets[0].fire_ns, 110 * MS); // = INT 预算恰点
        assert_eq!(out.buckets[0].fire_ns - 100 * MS, SLACK_INTERACTIVE_NS);
        assert!(laws_hold(&reqs, &out));
        // 再添一个 d=112ms 的 BG:112 > min_e=110 → 不进(INT 钉住的桶不因
        // 宽预算成员扩窗)。
        let reqs2 = [
            TimerReq { deadline_ns: 100 * MS, slack_ns: SLACK_INTERACTIVE_NS },
            TimerReq { deadline_ns: 105 * MS, slack_ns: SLACK_BACKGROUND_NS },
            TimerReq { deadline_ns: 112 * MS, slack_ns: SLACK_BACKGROUND_NS },
        ];
        let out2 = coalesce(&reqs2, 0, 0).unwrap();
        assert_eq!(out2.wakeups_after, 2);
        assert!(laws_hold(&reqs2, &out2));
    }

    /// LC exact 逃生舱:slack=0 单发即恰点;入桶则把桶钉在其 deadline
    /// 恰点(他人可搭车、LC 零顺延);更晚的成员绝不拖动 LC。
    #[test]
    fn lc_exact_never_deferred() {
        // 单发:fire == deadline 恰点,窗塌缩 0。
        let lc = TimerReq { deadline_ns: 100 * MS, slack_ns: 0 };
        let out = coalesce(&[lc], 0, 0).unwrap();
        assert_eq!(out.buckets[0].fire_ns, 100 * MS);
        assert_eq!(out.buckets[0].window_ns, 0);
        assert_eq!(out.buckets[0].span_ns, 0);

        // 早到的 BG 搭 LC 恰点车:BG d=95ms(预算至 595ms),LC d=100ms
        // 进桶 → min_e 收至 100 → fire=100=LC deadline,BG 顺延 5ms ≤ 500ms。
        let reqs = [
            TimerReq { deadline_ns: 95 * MS, slack_ns: SLACK_BACKGROUND_NS },
            lc,
        ];
        let out = coalesce(&reqs, 0, 0).unwrap();
        assert_eq!(out.wakeups_after, 1);
        assert_eq!(out.buckets[0].fire_ns, 100 * MS);
        assert!(laws_hold(&reqs, &out));

        // 更晚的 BG(d=101ms)> min_e=100 → 另桶,LC 醒点不动。
        let reqs2 = [
            lc,
            TimerReq { deadline_ns: 101 * MS, slack_ns: SLACK_BACKGROUND_NS },
        ];
        let out2 = coalesce(&reqs2, 0, 0).unwrap();
        assert_eq!(out2.wakeups_after, 2);
        assert_eq!(out2.buckets[0].fire_ns, 100 * MS); // LC 恰点零顺延
        assert_eq!(out2.buckets[1].fire_ns, 601 * MS); // BG 独享最迟点
        assert!(laws_hold(&reqs2, &out2));
    }

    /// L3 防饥饿上界:桶跨度 ≤ 开桶者 slack——MAINTENANCE(10s)开桶吞
    /// 5s 处的 BG,桶随最紧成员(5.5s 预算)关窗;跨度恒有界,松弛按
    /// 原始锚点计不重锚。
    #[test]
    fn anti_starvation_span_bounded() {
        let reqs = [
            TimerReq { deadline_ns: 0, slack_ns: SLACK_MAINTENANCE_NS },
            TimerReq { deadline_ns: 5_000 * MS, slack_ns: SLACK_BACKGROUND_NS },
            TimerReq { deadline_ns: 6_000 * MS, slack_ns: SLACK_BACKGROUND_NS },
        ];
        let out = coalesce(&reqs, 0, 0).unwrap();
        // b1: MAINT(0, min_e=10s) 开;5s ≤ 10s 进(min_e=min(10s,5.5s)=5.5s);
        // 6s > 5.5s 关 → fire=5.5s;6s 另桶 @6.5s。
        assert_eq!(out.wakeups_after, 2);
        assert_eq!(out.buckets[0].fire_ns, 5_500 * MS);
        assert_eq!(out.buckets[0].span_ns, 5_500 * MS); // ≤ s_first=10s
        assert!(out.buckets[0].span_ns <= SLACK_MAINTENANCE_NS);
        assert!(laws_hold(&reqs, &out));
        // 全批逐桶逐成员:fire − d_i ≤ s_i 恒成立(L2 全称钉板)。
        for b in &out.buckets {
            for &i in &b.members {
                assert!(b.fire_ns - reqs[i].deadline_ns <= reqs[i].slack_ns);
            }
        }
    }

    /// 同 deadline 输入序稳定:两个同 deadline 异 slack 的定时器,先入者
    /// 开桶(确定性——同输入恒同输出,遥测可对账);INT 是最紧成员,桶醒
    /// 点 = 其预算上界 20ms(L2 钉顶、窗内最迟取向)。
    #[test]
    fn deterministic_tie_break() {
        let reqs = [
            TimerReq { deadline_ns: 10 * MS, slack_ns: SLACK_INTERACTIVE_NS },
            TimerReq { deadline_ns: 10 * MS, slack_ns: SLACK_BACKGROUND_NS },
        ];
        let out = coalesce(&reqs, 0, 0).unwrap();
        assert_eq!(out.wakeups_after, 1);
        assert_eq!(out.buckets[0].members, vec![0, 1]); // 输入序保持
        assert_eq!(out.buckets[0].fire_ns, 20 * MS); // INT 预算上界钉顶
        assert!(laws_hold(&reqs, &out));
    }

    // ---- 坏输入拒面 ----

    /// 往昔锚点拒(§4.1 绝对截止纪律:deadline < now = 装配错,整批
    /// `None`);预算溢出拒(d+s 超 u64);空输入 = 合法平凡件。
    #[test]
    fn rejects_bad_inputs() {
        // 往昔锚点:now=10ms,d=9ms → None(即使批内其余合法)。
        let ok = TimerReq { deadline_ns: 20 * MS, slack_ns: 1 * MS };
        let past = TimerReq { deadline_ns: 9 * MS, slack_ns: 1 * MS };
        assert_eq!(coalesce(&[ok], 10 * MS, 0).unwrap().wakeups_after, 1);
        assert_eq!(coalesce(&[ok, past], 10 * MS, 0), None);
        // d == now 合法(闭边界——「现在」不算往昔)。
        let now_edge = TimerReq { deadline_ns: 10 * MS, slack_ns: 0 };
        assert!(coalesce(&[now_edge], 10 * MS, 0).is_some());
        // 预算溢出:u64::MAX−1 + slack 2 → checked_add None → 整批拒。
        let ovf = TimerReq { deadline_ns: u64::MAX - 1, slack_ns: 2 };
        assert_eq!(coalesce(&[ovf], 0, 0), None);
        // 极大但不溢:u64::MAX−1 + slack 1 → 单桶 fire=d+s 恰点。
        let edge = TimerReq { deadline_ns: u64::MAX - 1, slack_ns: 1 };
        let out = coalesce(&[edge], 0, 0).unwrap();
        assert_eq!(out.buckets[0].fire_ns, u64::MAX);
        // 空输入:Some 平凡件(0 桶 0 降比)。
        let empty = coalesce(&[], 0, 0).unwrap();
        assert_eq!(empty.buckets.len(), 0);
        assert_eq!(empty.wakeups_saved_pct, 0);
        assert_eq!(empty.wakeups_before, 0);
    }

    // ---- ④ policy 三元组 ----

    /// 三元组三键值域:slice [16,1024] 闭域边界;intent_default 编码
    /// 冻结域;timer_slack [0, MAINTENANCE](系统缺省不得越全域最宽)。
    #[test]
    fn policy_triple_domain() {
        let mk = |hz: u32, intent: u8, slack: u64| {
            PolicyTriple { slice_hz: hz, intent_default: intent, timer_slack_ns: slack }
        };
        // 合法基线 + 三闭边界(slice 16/1027?16 与 1024 两端)。
        assert!(policy_triple_valid(&mk(100, 0x00, 0)));
        assert!(policy_triple_valid(&mk(16, 0x05, SLACK_MAINTENANCE_NS)));
        assert!(policy_triple_valid(&mk(1024, 0x03, 50_000))); // Linux 50μs 同位
        // slice 越界双拒。
        assert!(!policy_triple_valid(&mk(15, 0x00, 0)));
        assert!(!policy_triple_valid(&mk(1027, 0x00, 0)));
        // intent_default 保留域拒(编码冻结 0x06..)。
        assert!(!policy_triple_valid(&mk(100, 0x06, 0)));
        assert!(!policy_triple_valid(&mk(100, 0xFF, 0)));
        // timer_slack 越全域最宽拒(0 合法——系统级 exact 缺省是合法极端)。
        assert!(!policy_triple_valid(&mk(100, 0x00, SLACK_MAINTENANCE_NS + 1)));
        assert!(policy_triple_valid(&mk(100, 0x00, 0)));
    }

    /// 解析链三形:intent 可得 → 域行(含 UNSPECIFIED 50ms 行);intent
    /// 不可得 → 系统缺省(current/default 双值同构);词表外硬拒 `None`
    /// (不静默落兜底——`-EINVAL` 优先)。
    #[test]
    fn resolve_slack_word_list_law() {
        let p = PolicyTriple { slice_hz: 100, intent_default: 0x00, timer_slack_ns: 2 * MS };
        assert_eq!(resolve_slack(&p, Some(0x02)), Some(SLACK_INTERACTIVE_NS));
        assert_eq!(resolve_slack(&p, Some(0x00)), Some(SLACK_UNSPECIFIED_NS));
        assert_eq!(resolve_slack(&p, None), Some(2 * MS)); // 兜底 = 系统缺省
        for v in [0x06u8, 0x0F, 0xFF] {
            assert_eq!(resolve_slack(&p, Some(v)), None, "v={v:#x}");
        }
    }
}
