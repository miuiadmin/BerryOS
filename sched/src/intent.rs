//! berry_intent 编码冻结 + intent 随消息传播（ADR #42，`调度器设计.md` §3.8、
//! `IPC设计.md` §2.1 五件套；4-3c）。
//!
//! v1 语义面 = **传播机制**（TCB 字段 + send 快照 / recv 结算 + 三道帽），
//! 无调度消费点——IntentProfile→latency-nice/EPP 的第一消费者随 EEVDF/v2；
//! 「查表键先于消费者落地」= ADR #42 第一段的形状（免 TCB 布局返工）。
//!
//! **数值约定：u8 越小档越高**（0x00=UNSPECIFIED 不设限为顶——「未声明」不是
//! 「声明了低档」，XNU 式）。故「借入取 max(own, 借来的)」按语义强度取，
//! 数值上即 `min`；未声明任务（0x00）发消息 = 借出最强档（不设限的保守方向，
//! 永不因未分类而错杀），通胀计数器（telemetry）即其可观测信号。
//!
//! 「reply 即回退」的 v1 形态 = **下次 dequeue 重导**（每条消息独立重算
//! `intent_effective = min(declared, msg_intent)`，非单调累积——QNX MsgReply
//! 继承不回退的粘性膨胀是已出货病态，这里从机制上不可能粘滞到下一条消息
//! 之后）。v1 无 reply syscall；两消息之间保持当前值（=「server 处理该消息
//! 期间持有」）。
//!
//! W3-Q13：判定律/编码冻结/三道帽单源已迁 `berry-intface`（host 可测钉板，
//! 18 测）；本模块保留 `Task` 字段桥接（[`IntentState`] 投影 ↔ 写回）——
//! `PI_MAX_HOPS` 迁 `berry-piface` 同款纪律。

use crate::task::Task;
use berry_intface::IntentState;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

// ---- 编码冻结 + 三道帽（单源 = berry-intface；此处 re-export 供
// kernel/ktest 既有消费面零改动）----

pub use berry_intface::{
    intent_valid, MsgIntent, SettleOut, SnapOut, INTENT_BACKGROUND,
    INTENT_CHAIN_BUDGET_NS, INTENT_HOP_BUDGET_NS, INTENT_HOP_MAX, INTENT_INTERACTIVE,
    INTENT_LATENCY_CRITICAL, INTENT_MAINTENANCE, INTENT_UNSPECIFIED, INTENT_UTILITY,
};

// ---- G5（§1.6-补）：intent_default——新任务缺省 intent 的产品级覆盖 ----

/// 新任务 declared/effective 的产品级缺省档（boot-params
/// `intent_default`——kernel/src/bootparams.rs 消费注入）。缺省
/// [`INTENT_UNSPECIFIED`] = 现役行为零漂移（空 params 镜像逐位不变；
/// §1.6-补「现役 0x00=UNSPECIFIED……改缺省 = 调度域行为变更」的落体：
/// 变更只在产品显式声明时发生）。原子 Relaxed：boot 单写者 + 之后
/// 任务装配点多核只读（免 unsafe 面——hal timer NS_MUL 同款纪律）。
static INTENT_DEFAULT: AtomicU8 = AtomicU8::new(INTENT_UNSPECIFIED);

/// 设置新任务缺省 intent（boot 早期、首个 spawn 前；编码域执法归调用
/// 方——词表映射在 bootparams::parse）。
pub fn set_intent_default(code: u8) {
    INTENT_DEFAULT.store(code, Ordering::Relaxed);
}

/// 读新任务缺省 intent（`Task` 装配点消费——task.rs `try_assemble`）。
pub fn intent_default() -> u8 {
    INTENT_DEFAULT.load(Ordering::Relaxed)
}

// ---- CP-11（§3.8 机制面·2026-11-15）：timer_slack——系统缺省 timer slack ----

/// 系统缺省 timer slack（纳秒；boot-params 第三键 `timer_slack`——
/// kernel/src/bootparams.rs 消费注入，slackface `PolicyTriple.timer_slack_ns`
/// 的内核全局存）。缺省 0 = 无松弛 = 现役行为零漂移（空 params 镜像逐位
/// 不变——`INTENT_DEFAULT` 同款合同）。原子 Relaxed：boot 单写者 + 之后
/// `block_current` 多核只读（免 unsafe 面——`INTENT_DEFAULT` 同款纪律）。
static TIMER_SLACK_NS: AtomicU64 = AtomicU64::new(0);

/// 设置系统缺省 timer slack（boot 早期；值域 [0, 999_999_999] 执法归
/// bootparams 词表——本面信任调用方）。
pub fn set_timer_slack_ns(ns: u64) {
    TIMER_SLACK_NS.store(ns, Ordering::Relaxed);
}

/// 读系统缺省 timer slack（`block_current` 消费点）。
pub fn timer_slack_ns() -> u64 {
    TIMER_SLACK_NS.load(Ordering::Relaxed)
}

/// timer slack 的 LATE-only 顺延判定律（纯函数；判定律单源在
/// `user/berry-slackface`——L1 late-only / L2 最紧不越两律的内核消费面，
/// 聚合分桶不在此重实现）。
///
/// 语义：`intent` 档 **>= UTILITY**（`INTENT_UTILITY..=INTENT_MAINTENANCE`
/// 三档——UTILITY/BACKGROUND/MAINTENANCE）才顺延；**交互零牺牲**（§3.8
/// :156——INTERACTIVE/LATENCY_CRITICAL 不加，slackface L2 的推论）+ 未声明
/// 零漂移（UNSPECIFIED 不加——「未声明不是声明了低档」，保存量任务
/// BLOCK/INTENT/G1.5 时序逐位不变）；保留域（0x06..）保守回 exact。
/// 数学面：零 slack / 异常频率 / 无限等哨兵（`u64::MAX`）恒等；否则 ns→tick
/// **截尾**换算（floor——实际顺延恒 ≤ 声明 slack，LATE-only 永不提前），
/// 顶域夹 `u64::MAX - 1`（不回绕、不撞无限等哨兵）。
pub fn timer_slack_defer(intent: u8, deadline_vct: u64, slack_ns: u64, freq: u64) -> u64 {
    if slack_ns == 0 || freq == 0 || deadline_vct == u64::MAX {
        return deadline_vct;
    }
    if !(INTENT_UTILITY..=INTENT_MAINTENANCE).contains(&intent) {
        return deadline_vct;
    }
    let delta = slack_ns as u128 * freq as u128 / 1_000_000_000;
    if delta == 0 {
        return deadline_vct;
    }
    let deferred = deadline_vct as u128 + delta;
    let cap = (u64::MAX - 1) as u128;
    deferred.min(cap) as u64
}

/// `Task` intent 五字段 → 纯值域投影（W3-Q13：判定律只读这五个数）。
#[inline]
fn state_of(t: &Task) -> IntentState {
    IntentState::new(
        t.intent_declared,
        t.intent_effective,
        t.intent_hop,
        t.intent_since,
        t.intent_acc,
    )
}

/// 投影写回（借入/回落后的 effective + 记录三元组）。
#[inline]
fn write_back(t: &mut Task, s: IntentState) {
    t.intent_effective = s.effective;
    t.intent_hop = s.hop;
    t.intent_since = s.since;
    t.intent_acc = s.acc;
}

/// send 侧快照：发送者 `intent_effective` 打包进消息头（fastpath 增量 = 一次
/// 结构体填充；记账/帽判定全在 recv 结算 slowpath——五件套之 2「fastpath
/// 零记账」）。判定律单源 [`berry_intface::snapshot`]。
pub fn snapshot(t: &Task, now_ns: u64) -> (MsgIntent, SnapOut) {
    berry_intface::snapshot(&state_of(t), now_ns)
}

/// recv 侧结算（slowpath，dequeue 即结算）：**每消息重导**
/// `intent_effective = min(declared, msg_intent)`（数值越小档越高）——非单调
/// 累积，下一条弱消息自然回落（reply 即回退的无 reply 等价物）。借入生效时
/// 记 hop/since/acc（转发快照与两道时间帽的数据源）。opt-out 通道（五件套
/// 之 4）与超帽一律回落 declared——固定优先级 server 永不借入。
/// 判定律单源 [`berry_intface::settle`]。
pub fn settle(t: &mut Task, m: MsgIntent, opt_out: bool, now_ns: u64) -> SettleOut {
    let mut s = state_of(t);
    let out = berry_intface::settle(&mut s, m, opt_out, now_ns);
    write_back(t, s);
    out
}
