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

// ---- 编码冻结 + 三道帽（单源 = berry-intface；此处 re-export 供
// kernel/ktest 既有消费面零改动）----

pub use berry_intface::{
    intent_valid, MsgIntent, SettleOut, SnapOut, INTENT_BACKGROUND,
    INTENT_CHAIN_BUDGET_NS, INTENT_HOP_BUDGET_NS, INTENT_HOP_MAX, INTENT_INTERACTIVE,
    INTENT_LATENCY_CRITICAL, INTENT_MAINTENANCE, INTENT_UNSPECIFIED, INTENT_UTILITY,
};

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
