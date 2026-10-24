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

use crate::task::Task;

// ---- 编码冻结（随 ADR #42 冻结；0x06..0x0F 保留，内核拒绝 -EINVAL）----

/// 未声明 = XNU 式最高可用档（不设限；可被 manifest/分类器定档，pledge 豁免）。
pub const INTENT_UNSPECIFIED: u8 = 0x00;
/// 需 Job 级授权（manifest/特权；timer slack=0 exact；AI 屏蔽）。
pub const INTENT_LATENCY_CRITICAL: u8 = 0x01;
/// 交互。
pub const INTENT_INTERACTIVE: u8 = 0x02;
/// 推断缺省落点。
pub const INTENT_UTILITY: u8 = 0x03;
/// maintenance_eligible 门控。
pub const INTENT_BACKGROUND: u8 = 0x04;
/// 编码冻结、v1 不启用（用户裁决 2026-10-16）。
pub const INTENT_MAINTENANCE: u8 = 0x05;

/// intent 值合法域（0x00–0x05；入口治理随声明入口 syscall 落地，判别先冻结）。
pub fn intent_valid(v: u8) -> bool {
    v <= INTENT_MAINTENANCE
}

// ---- 三道帽（§2.1 五件套之 3：仅限跳数不够——恶意/失控服务链能吃光调用方
// 预算，seL4 Johnston 2022；超顶一律「降级为不传播」，绝不 panic）----

/// **消息链**跳数帽（与阻塞链 PI 轮帽 = 8 数值巧合、语义独立——§2.1 两链
/// 不重合：PI 限 send 阻塞链深，这限 intent 随消息转发的跳数）。
pub const INTENT_HOP_MAX: u8 = 8;
/// 每跳执行预算（纳秒）：借入方持有超此预算后，转发快照降级为自己的
/// declared（借来的档不再传播）。v1 旋钮。
pub const INTENT_HOP_BUDGET_NS: u64 = 50_000_000;
/// 消息链累计时间封顶（纳秒）：消息头携带的链累计持有时长超此值，结算侧
/// 拒传播（回落 declared）。v1 旋钮（= 跳数帽 × 每跳预算，同量级收口）。
pub const INTENT_CHAIN_BUDGET_NS: u64 = INTENT_HOP_BUDGET_NS * INTENT_HOP_MAX as u64;

/// 消息随行的 intent 元数据（send 快照填、recv 结算读；`channel.rs` 的
/// `Message` 以此为消息头字段——内核可见元数据槽，ABI §8.3）。
#[derive(Clone, Copy, Default, PartialEq)]
pub struct MsgIntent {
    /// 发送者 `intent_effective` 快照。
    pub intent: u8,
    /// 本 intent 已转发跳数（首手 0；借入方转发 +1）。
    pub hop: u8,
    /// 链累计持有时长（纳秒；每跳转发累加自己持有段）。
    pub acc_ns: u64,
}

/// recv 结算结果（调用方据此记遥测/trace）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleOut {
    /// 借入生效（effective 抬到消息档）。
    Borrowed,
    /// 未借（消息档不强于声明，或 opt-out）——每消息重导的常态。
    Own,
    /// 跳数帽拒绝传播。
    CappedHop,
    /// 累计时间封顶拒绝传播。
    CappedBudget,
}

/// send 快照结果（调用方据此记遥测/trace）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapOut {
    /// 首手 intent（未借入）。
    Fresh,
    /// 借入转发（hop+1、acc 累加）。
    Forwarded,
    /// 每跳预算帽触发——降级为自己的 declared。
    Capped,
}

/// send 侧快照：发送者 `intent_effective` 打包进消息头（fastpath 增量 = 一次
/// 结构体填充；记账/帽判定全在 recv 结算 slowpath——五件套之 2「fastpath
/// 零记账」）。
pub fn snapshot(t: &Task, now_ns: u64) -> (MsgIntent, SnapOut) {
    if t.intent_effective >= t.intent_declared {
        // 未借入（相等含默认）：首手 intent，hop/acc 清零。
        return (MsgIntent { intent: t.intent_declared, hop: 0, acc_ns: 0 }, SnapOut::Fresh);
    }
    let held = now_ns.saturating_sub(t.intent_since);
    if held > INTENT_HOP_BUDGET_NS {
        // 每跳预算帽：超顶降级为不传播（自己的 declared 原样打包）。
        return (MsgIntent { intent: t.intent_declared, hop: 0, acc_ns: 0 }, SnapOut::Capped);
    }
    (
        MsgIntent {
            intent: t.intent_effective,
            hop: t.intent_hop.saturating_add(1),
            acc_ns: t.intent_acc.saturating_add(held),
        },
        SnapOut::Forwarded,
    )
}

/// recv 侧结算（slowpath，dequeue 即结算）：**每消息重导**
/// `intent_effective = min(declared, msg_intent)`（数值越小档越高）——非单调
/// 累积，下一条弱消息自然回落（reply 即回退的无 reply 等价物）。借入生效时
/// 记 hop/since/acc（转发快照与两道时间帽的数据源）。opt-out 通道（五件套
/// 之 4）与超帽一律回落 declared——固定优先级 server 永不借入。
pub fn settle(t: &mut Task, m: MsgIntent, opt_out: bool, now_ns: u64) -> SettleOut {
    if opt_out {
        t.intent_effective = t.intent_declared; // 拒借 + 重导回落（含清掉他途借入）
        return SettleOut::Own;
    }
    if m.acc_ns > INTENT_CHAIN_BUDGET_NS {
        t.intent_effective = t.intent_declared;
        return SettleOut::CappedBudget;
    }
    if m.hop >= INTENT_HOP_MAX {
        t.intent_effective = t.intent_declared;
        return SettleOut::CappedHop;
    }
    let eff = t.intent_declared.min(m.intent);
    t.intent_effective = eff;
    if eff < t.intent_declared {
        t.intent_hop = m.hop;
        t.intent_since = now_ns;
        t.intent_acc = m.acc_ns;
        SettleOut::Borrowed
    } else {
        SettleOut::Own
    }
}
