//! berry-intface —— intent 随消息传播机制纯函数面（W3-Q13）。
//!
//! **动机**：4-3c intent 传播（ADR #42 五件套；`调度器设计.md` §3.8、
//! `IPC设计.md` §2.1）的传播语义——send 快照（`snapshot`）/recv 结算
//! （`settle`）三道帽判序 + 每消息重导回落 + 编码合法域（`intent_valid`）
//! ——此前只活在 `sched/src/intent.rs` 的 `Task` 字段读写里：host 测不可达
//! （sched 依赖 HAL global_asm，只 aarch64-unknown-none 可编），ktest 只
//! 覆盖 boot 相（`test_intent` K0–K9）。传播机制的回归面（三道帽超顶降级
//! 为不传播、非单调每消息重导、opt-out 拒借、u8 值越小档越高的 min 语义）
//! 需要一张 host 可测的**钉板**：本 crate 把判序按值域抽成纯函数 + 表驱动
//! 测试，`sched/src/intent.rs` 机械壳化消费同一函数（零第二实现——抽离即
//! 唯一事实源，非镜像）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`IntentState`]：TCB intent 五字段的纯值域投影（declared/effective/
//!   hop/since/acc——`Task` 侧字段组机械搬入）；kernel/sched 经
//!   [`IntentState::of`] / [`IntentState::write_back`] 桥接。
//! - [`snapshot`]/[`settle`]：send/recv 两侧判定律（判序、三道帽、每消息
//!   重导、opt-out 拒借+回落）——与原 `sched/src/intent.rs` 逐字同义。
//! - 编码冻结常量 `INTENT_*` + 三道帽旋钮 + [`intent_valid`]：单源迁此
//!   （sched re-export，kernel/ktest 消费面零改动）。
//!
//! 语义钉板（表驱动测试逐条钉，ktest K0–K8 的 host 版 + 域界扩展）：
//! - 数值序：u8 越小档越高（0x00 UNSPECIFIED 顶），借入 = min；
//! - 判序：opt_out → 链预算帽 → 跳数帽 → 借入（先拒后借）；
//! - 每消息重导：`eff = min(declared, msg)` 非单调，弱消息自然回落，
//!   opt-out 一律重导 declared（含清掉他途借入）；
//! - 快照三态：Fresh（未借入）/Forwarded（借入转发 hop+1、acc 累加）/
//!   Capped（每跳预算超顶降级为自己的 declared 原样打包）；
//! - 饱和：`now < since`（时钟回退形）saturating 不 panic。
//!
//! 与冻结面的关系：syscall ABI 零触碰（纯内核内部语义）；`MsgIntent`
//! 消息头形状（ABI §8.3 内核可见元数据槽）字段名/类型逐字保留。
//! `intent.rs` 零 unsafe（本 crate 同样零 unsafe——纯函数面）。

#![cfg_attr(not(test), no_std)]
#![allow(clippy::result_unit_err)]

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
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
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

/// TCB intent 五字段的纯值域投影（`sched::Task` 的
/// intent_declared/effective/hop/since/acc 字段组——判定律只读这五个数，
/// 值域抽出后 host 可测；`Task` 侧经 `of`/`write_back` 桥接，字段语义
/// 单源在本 crate 的文档面）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IntentState {
    /// 声明档（创建者定；0x00=UNSPECIFIED 不设限；pledge 只降不升）。
    /// **u8 越小档越高**。
    pub declared: u8,
    /// 有效档 = min(declared, 消息借来的)（**每消息重导**——非单调累积，
    /// 弱消息自然回落）。
    pub effective: u8,
    /// 当前借入 intent 的消息链跳数（借入时随消息头记录；转发快照 +1）。
    pub hop: u8,
    /// 本次借入开始时刻（纳秒，结算侧戳——每跳执行预算帽的计时起点）。
    pub since: u64,
    /// 借入消息携带的链累计持有时长（纳秒；转发快照累加自己持有段）。
    pub acc: u64,
}

impl IntentState {
    /// 从散字段构造（`of(t)` 的值域形；kernel/sched 桥接点用）。
    pub const fn new(
        declared: u8,
        effective: u8,
        hop: u8,
        since: u64,
        acc: u64,
    ) -> Self {
        Self { declared, effective, hop, since, acc }
    }
}

/// send 侧快照：发送者 `intent_effective` 打包进消息头（fastpath 增量 = 一次
/// 结构体填充；记账/帽判定全在 recv 结算 slowpath——五件套之 2「fastpath
/// 零记账」）。
pub fn snapshot(s: &IntentState, now_ns: u64) -> (MsgIntent, SnapOut) {
    if s.effective >= s.declared {
        // 未借入（相等含默认）：首手 intent，hop/acc 清零。
        return (MsgIntent { intent: s.declared, hop: 0, acc_ns: 0 }, SnapOut::Fresh);
    }
    let held = now_ns.saturating_sub(s.since);
    if held > INTENT_HOP_BUDGET_NS {
        // 每跳预算帽：超顶降级为不传播（自己的 declared 原样打包）。
        return (MsgIntent { intent: s.declared, hop: 0, acc_ns: 0 }, SnapOut::Capped);
    }
    (
        MsgIntent {
            intent: s.effective,
            hop: s.hop.saturating_add(1),
            acc_ns: s.acc.saturating_add(held),
        },
        SnapOut::Forwarded,
    )
}

/// recv 侧结算（slowpath，dequeue 即结算）：**每消息重导**
/// `intent_effective = min(declared, msg_intent)`（数值越小档越高）——非单调
/// 累积，下一条弱消息自然回落（reply 即回退的无 reply 等价物）。借入生效时
/// 记 hop/since/acc（转发快照与两道时间帽的数据源）。opt-out 通道（五件套
/// 之 4）与超帽一律回落 declared——固定优先级 server 永不借入。
pub fn settle(s: &mut IntentState, m: MsgIntent, opt_out: bool, now_ns: u64) -> SettleOut {
    if opt_out {
        s.effective = s.declared; // 拒借 + 重导回落（含清掉他途借入）
        return SettleOut::Own;
    }
    if m.acc_ns > INTENT_CHAIN_BUDGET_NS {
        s.effective = s.declared;
        return SettleOut::CappedBudget;
    }
    if m.hop >= INTENT_HOP_MAX {
        s.effective = s.declared;
        return SettleOut::CappedHop;
    }
    let eff = s.declared.min(m.intent);
    s.effective = eff;
    if eff < s.declared {
        s.hop = m.hop;
        s.since = now_ns;
        s.acc = m.acc_ns;
        SettleOut::Borrowed
    } else {
        SettleOut::Own
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 基准常量（ktest test_intent 的 NOW 同款——任意 vct_ns 域基准）----
    const NOW: u64 = 1_000_000_000;

    fn state(declared: u8, effective: u8) -> IntentState {
        IntentState::new(declared, effective, 0, NOW, 0)
    }

    // ---- 编码域 ----

    #[test]
    fn intent_valid_domain() {
        for v in 0x00..=0x05 {
            assert!(intent_valid(v), "{:#04x} 须合法", v);
        }
        for v in [0x06u8, 0x0F, 0x10, 0x7F, 0x80, 0xFF] {
            assert!(!intent_valid(v), "{:#04x} 保留/野值须拒", v);
        }
    }

    // ---- snapshot 三态 ----

    #[test]
    fn snapshot_fresh_when_not_borrowed() {
        // 未借入：effective >= declared（含相等默认态）→ 首手 intent。
        for d in 0x00..=0x05 {
            let s = state(d, d);
            let (m, out) = snapshot(&s, NOW);
            assert_eq!(out, SnapOut::Fresh);
            assert_eq!(m, MsgIntent { intent: d, hop: 0, acc_ns: 0 });
            // 数值越界态（effective > declared，防御面）：仍 Fresh、装 declared。
            let s2 = state(d, d.saturating_add(1));
            let (m2, out2) = snapshot(&s2, NOW);
            assert_eq!(out2, SnapOut::Fresh);
            assert_eq!(m2.intent, d);
        }
    }

    #[test]
    fn snapshot_forwarded_within_budget() {
        // 借入中（effective < declared）、持有段未超每跳预算 → 转发：
        // intent=effective、hop+1、acc 累加持有段。
        let s = IntentState::new(
            INTENT_BACKGROUND,       // declared 0x04
            INTENT_LATENCY_CRITICAL, // 借入 0x01
            3,                       // 已 3 跳
            NOW - 10_000_000,        // 持有 10ms < 50ms 预算
            20_000_000,              // 链累计 20ms
        );
        let (m, out) = snapshot(&s, NOW);
        assert_eq!(out, SnapOut::Forwarded);
        assert_eq!(m.intent, INTENT_LATENCY_CRITICAL);
        assert_eq!(m.hop, 4);
        assert_eq!(m.acc_ns, 30_000_000);
    }

    #[test]
    fn snapshot_capped_over_hop_budget() {
        // 每跳预算超顶（held > 50ms）：降级为自己的 declared 原样打包、
        // hop/acc 清零、自身 effective 不动（转发降级 ≠ 自身回落——4-3c）。
        let mut s = IntentState::new(
            INTENT_UTILITY,
            INTENT_INTERACTIVE,
            2,
            NOW - (INTENT_HOP_BUDGET_NS + 1),
            5_000_000,
        );
        let (m, out) = snapshot(&s, NOW);
        assert_eq!(out, SnapOut::Capped);
        assert_eq!(m, MsgIntent { intent: INTENT_UTILITY, hop: 0, acc_ns: 0 });
        assert_eq!(s.effective, INTENT_INTERACTIVE, "Capped 只降转发，不回落自身");
        // 恰预算边界（held == 预算）不触帽——严格大于才拒。
        s.since = NOW - INTENT_HOP_BUDGET_NS;
        let (_, out2) = snapshot(&s, NOW);
        assert_eq!(out2, SnapOut::Forwarded, "held == 预算须 Forwarded（严格 > 拒）");
    }

    #[test]
    fn snapshot_saturating_on_clock_backwards() {
        // since > now（时钟回退形）：held saturating 到 0，不 panic、不回绕。
        let s = IntentState::new(
            INTENT_BACKGROUND,
            INTENT_LATENCY_CRITICAL,
            0,
            NOW + 7_000_000, // since 在未来
            123,
        );
        let (m, out) = snapshot(&s, NOW);
        assert_eq!(out, SnapOut::Forwarded);
        assert_eq!(m.hop, 1);
        assert_eq!(m.acc_ns, 123, "held=0 只携带旧 acc");
    }

    #[test]
    fn snapshot_hop_acc_saturate() {
        // hop=u8::MAX / acc=u64::MAX 的极端态：saturating 不 panic 不回绕。
        let s = IntentState::new(
            INTENT_MAINTENANCE,
            INTENT_UNSPECIFIED,
            u8::MAX,
            NOW,
            u64::MAX,
        );
        let (m, out) = snapshot(&s, NOW);
        assert_eq!(out, SnapOut::Forwarded);
        assert_eq!(m.hop, u8::MAX, "hop 饱和不回绕");
        assert_eq!(m.acc_ns, u64::MAX, "acc 饱和不回绕");
    }

    // ---- settle 判序 ----

    #[test]
    fn settle_order_optout_first() {
        // 判序钉板：opt_out 最先——即便消息超链预算/超跳数帽也返回 Own
        // （拒借通道先于帽判定；且重导回落清掉他途借入）。
        let mut s = IntentState::new(
            INTENT_BACKGROUND,
            INTENT_LATENCY_CRITICAL, // 他途借入残留
            1, NOW, 100,
        );
        let out = settle(&mut s, MsgIntent {
            intent: INTENT_LATENCY_CRITICAL,
            hop: INTENT_HOP_MAX,              // 超跳帽
            acc_ns: INTENT_CHAIN_BUDGET_NS + 1, // 超链预算
        }, true, NOW);
        assert_eq!(out, SettleOut::Own);
        assert_eq!(s.effective, INTENT_BACKGROUND, "opt-out 须清他途借入");
        assert_eq!(s.hop, 1, "opt-out 不动 hop/since/acc 记录字段");
    }

    #[test]
    fn settle_order_budget_before_hop() {
        // 判序钉板：链预算帽先于跳数帽（两帽同超 → CappedBudget）。
        let mut s = state(INTENT_UTILITY, INTENT_UTILITY);
        let out = settle(&mut s, MsgIntent {
            intent: INTENT_LATENCY_CRITICAL,
            hop: INTENT_HOP_MAX,
            acc_ns: INTENT_CHAIN_BUDGET_NS + 1,
        }, false, NOW);
        assert_eq!(out, SettleOut::CappedBudget);
        assert_eq!(s.effective, INTENT_UTILITY);
    }

    #[test]
    fn settle_capped_budget_boundary() {
        let mut s = state(INTENT_BACKGROUND, INTENT_BACKGROUND);
        // 恰等于链预算：不拒（严格 > 拒）——可借入。
        let out = settle(&mut s, MsgIntent {
            intent: INTENT_LATENCY_CRITICAL,
            hop: 2,
            acc_ns: INTENT_CHAIN_BUDGET_NS,
        }, false, NOW);
        assert_eq!(out, SettleOut::Borrowed);
        assert_eq!(s.acc, INTENT_CHAIN_BUDGET_NS);
        // +1 超顶：拒。
        let mut s2 = state(INTENT_BACKGROUND, INTENT_BACKGROUND);
        let out2 = settle(&mut s2, MsgIntent {
            intent: INTENT_LATENCY_CRITICAL,
            hop: 2,
            acc_ns: INTENT_CHAIN_BUDGET_NS + 1,
        }, false, NOW);
        assert_eq!(out2, SettleOut::CappedBudget);
    }

    #[test]
    fn settle_capped_hop_boundary() {
        let mut s = state(INTENT_UTILITY, INTENT_UTILITY);
        // hop == HOP_MAX：拒（>= 判）；hop == HOP_MAX-1：收。
        let out = settle(&mut s, MsgIntent {
            intent: INTENT_INTERACTIVE,
            hop: INTENT_HOP_MAX,
            acc_ns: 0,
        }, false, NOW);
        assert_eq!(out, SettleOut::CappedHop);
        let out2 = settle(&mut s, MsgIntent {
            intent: INTENT_INTERACTIVE,
            hop: INTENT_HOP_MAX - 1,
            acc_ns: 0,
        }, false, NOW);
        assert_eq!(out2, SettleOut::Borrowed);
        assert_eq!(s.hop, INTENT_HOP_MAX - 1);
    }

    // ---- 每消息重导（非单调）----

    #[test]
    fn settle_borrow_then_weak_falls_back() {
        // 强消息借入 → 弱消息自然回落（每消息重导，非历史 max 累积——
        // QNX 粘性膨胀的机制性防线）。
        let mut s = state(INTENT_BACKGROUND, INTENT_BACKGROUND);
        let out = settle(&mut s, MsgIntent {
            intent: INTENT_LATENCY_CRITICAL, hop: 0, acc_ns: 0,
        }, false, NOW);
        assert_eq!(out, SettleOut::Borrowed);
        assert_eq!(s.effective, INTENT_LATENCY_CRITICAL);

        let out2 = settle(&mut s, MsgIntent {
            intent: INTENT_INTERACTIVE, hop: 0, acc_ns: 0,
        }, false, NOW);
        assert_eq!(out2, SettleOut::Borrowed);
        assert_eq!(s.effective, INTENT_INTERACTIVE, "重导 = min(declared, msg)");

        let out3 = settle(&mut s, MsgIntent {
            intent: INTENT_MAINTENANCE, hop: 0, acc_ns: 0,
        }, false, NOW);
        assert_eq!(out3, SettleOut::Own, "弱于声明的消息不借");
        assert_eq!(s.effective, INTENT_BACKGROUND, "回落 declared（非保持旧借入）");
    }

    #[test]
    fn settle_unspecified_borrows() {
        // 未声明（0x00 顶档）任务收任意消息：只要消息档 < 0x00 不可能——
        // 恒 Own（已是顶档）；声明低档者收 0x00 消息：借入到 0x00。
        let mut s = state(INTENT_UNSPECIFIED, INTENT_UNSPECIFIED);
        let out = settle(&mut s, MsgIntent {
            intent: INTENT_LATENCY_CRITICAL, hop: 0, acc_ns: 0,
        }, false, NOW);
        assert_eq!(out, SettleOut::Own, "0x00 顶档无从借入");
        assert_eq!(s.effective, INTENT_UNSPECIFIED);

        let mut s2 = state(INTENT_MAINTENANCE, INTENT_MAINTENANCE);
        let out2 = settle(&mut s2, MsgIntent {
            intent: INTENT_UNSPECIFIED, hop: 1, acc_ns: 42,
        }, false, NOW);
        assert_eq!(out2, SettleOut::Borrowed);
        assert_eq!(s2.effective, INTENT_UNSPECIFIED);
        assert_eq!((s2.hop, s2.since, s2.acc), (1, NOW, 42), "借入记三元组");
    }

    #[test]
    fn settle_equal_is_own_not_borrowed() {
        // eff == declared（消息恰等于声明档）：Own 非 Borrowed——
        // 「借入」严格定义 = 抬升到强于声明的档。
        let mut s = state(INTENT_INTERACTIVE, INTENT_INTERACTIVE);
        let out = settle(&mut s, MsgIntent {
            intent: INTENT_INTERACTIVE, hop: 3, acc_ns: 7,
        }, false, NOW);
        assert_eq!(out, SettleOut::Own);
        assert_eq!(s.effective, INTENT_INTERACTIVE);
        assert_eq!((s.hop, s2acc(&s)), (0, 0), "Own 不记消息三元组");
    }

    fn s2acc(s: &IntentState) -> u64 {
        s.acc
    }

    #[test]
    fn settle_caps_do_not_touch_records() {
        // 两帽拒时只回落 effective，hop/since/acc 不被消息污染。
        let mut s = IntentState::new(
            INTENT_UTILITY,
            INTENT_UNSPECIFIED, // 旧借入在身
            2, NOW - 100, 999,
        );
        let out = settle(&mut s, MsgIntent {
            intent: INTENT_LATENCY_CRITICAL,
            hop: INTENT_HOP_MAX,
            acc_ns: 0,
        }, false, NOW);
        assert_eq!(out, SettleOut::CappedHop);
        assert_eq!(s.effective, INTENT_UTILITY, "帽拒回落 declared");
        assert_eq!((s.hop, s.acc), (2, 999), "帽拒不覆盖记录三元组");
    }

    // ---- snapshot ↔ settle 往返（转发链形）----

    #[test]
    fn forward_chain_roundtrip() {
        // 三跳链形：A(0x04 声明) 借 0x01 → 快照转发 → B(0x03) 结算借入 →
        // B 快照再转发 → C(0x02) 结算。跳数/acc 沿链单调推进、档位 min 传播。
        let mut a = state(INTENT_BACKGROUND, INTENT_LATENCY_CRITICAL); // A 已借 0x01
        let a_since = NOW - 5_000_000;
        a.since = a_since;
        let (m1, o1) = snapshot(&a, NOW);
        assert_eq!(o1, SnapOut::Forwarded);
        assert_eq!(m1.hop, 1);

        let mut b = state(INTENT_UTILITY, INTENT_UTILITY);
        let ob = settle(&mut b, m1, false, NOW + 1_000_000);
        assert_eq!(ob, SettleOut::Borrowed);
        assert_eq!(b.effective, INTENT_LATENCY_CRITICAL);
        assert_eq!(b.hop, 1);

        let b_since = NOW + 3_000_000;
        b.since = b_since;
        let (m2, o2) = snapshot(&b, NOW + 8_000_000); // 持有 5ms
        assert_eq!(o2, SnapOut::Forwarded);
        assert_eq!(m2.hop, 2, "B 转发 = A 跳 +1");
        assert_eq!(m2.acc_ns, m1.acc_ns + 5_000_000, "acc 沿链累加持有段");

        let mut c = state(INTENT_INTERACTIVE, INTENT_INTERACTIVE);
        let oc = settle(&mut c, m2, false, NOW + 8_000_000);
        assert_eq!(oc, SettleOut::Borrowed);
        assert_eq!(c.effective, INTENT_LATENCY_CRITICAL, "档位 min 传播");
        assert_eq!(c.hop, 2);
    }

    #[test]
    fn forward_chain_hop_cap_terminates() {
        // 长链推进到 HOP_MAX 后结算拒——恶意/失控服务链的传播终止面。
        let mut cur = state(INTENT_MAINTENANCE, INTENT_MAINTENANCE);
        let mut m = MsgIntent { intent: INTENT_LATENCY_CRITICAL, hop: 0, acc_ns: 0 };
        let mut t = NOW;
        let mut borrowed = 0;
        loop {
            let out = settle(&mut cur, m, false, t);
            match out {
                SettleOut::Borrowed => borrowed += 1,
                SettleOut::CappedHop => break,
                _ => panic!("链推进意外 {:?}（hop={}）", out, m.hop),
            }
            // 下一跳：持有 1ms 转发（远低于每跳预算）。
            t += 1_000_000;
            let (m2, o) = snapshot(&cur, t);
            assert_eq!(o, SnapOut::Forwarded);
            m = m2;
            assert!(m.acc_ns <= INTENT_CHAIN_BUDGET_NS, "链内 acc 须在帽内");
        }
        assert_eq!(m.hop, INTENT_HOP_MAX, "终止于跳数帽");
        assert_eq!(borrowed, INTENT_HOP_MAX as usize, "0..HOP_MAX-1 共 8 次借入");
        assert_eq!(cur.effective, INTENT_MAINTENANCE, "终止后回落 declared");
    }

    #[test]
    fn settle_budget_gate_catches_forged_acc() {
        // 诚实链不变量：Forwarded 快照 acc ≤ hop × 每跳预算 ≤ 7×50ms=350ms
        // < 链帽 400ms——两帽数值耦合（CHAIN = HOP×BUDGET）下跳数帽恒先触
        // （上测钉板）。链预算帽的真值面 = **伪造消息头**（§A41：ktest 超帽
        // 样本 = 伪造消息直压收端队列）：恶意/越权写者灌超帽 acc，结算侧
        // 独立闸拒绝。
        for hop in [0u8, 1, 3, INTENT_HOP_MAX - 1] {
            let mut s = state(INTENT_UTILITY, INTENT_UTILITY);
            let out = settle(&mut s, MsgIntent {
                intent: INTENT_LATENCY_CRITICAL,
                hop,
                acc_ns: INTENT_CHAIN_BUDGET_NS + 1,
            }, false, NOW);
            assert_eq!(out, SettleOut::CappedBudget, "hop={} 伪 acc 须链帽拒", hop);
            assert_eq!(s.effective, INTENT_UTILITY);
        }
        // 诚实链上界复核：任意 hop < HOP_MAX 的转发 acc 上界 < 链帽。
        for hop in 0..INTENT_HOP_MAX {
            assert!(
                hop as u64 * INTENT_HOP_BUDGET_NS < INTENT_CHAIN_BUDGET_NS,
                "诚实链 hop={} 的 acc 上界须 < 链帽", hop,
            );
        }
    }

    // ---- 常量自洽 ----

    #[test]
    fn constants_self_consistent() {
        assert_eq!(INTENT_CHAIN_BUDGET_NS, INTENT_HOP_BUDGET_NS * INTENT_HOP_MAX as u64);
        assert_eq!(INTENT_HOP_MAX, 8);
        // 六档编码连续无洞（0x00–0x05）。
        let rows = [
            INTENT_UNSPECIFIED, INTENT_LATENCY_CRITICAL, INTENT_INTERACTIVE,
            INTENT_UTILITY, INTENT_BACKGROUND, INTENT_MAINTENANCE,
        ];
        for (i, v) in rows.iter().enumerate() {
            assert_eq!(*v, i as u8, "六档须按 0x00–0x05 连续编址");
        }
    }
}
