//! presarb —— present 域单飞仲裁（云桌面反复卡死根治刀 · 架构面 A）。
//!
//! **缺陷本体**：guifbd 的 present 域是单客户端时代几何——`held` 别名表
//! 全局单表（CREDITS=2）、pending/cb_park/rel_park 全单槽、新 P2 SUBMIT
//! 无条件压弃在途帧。K1 per-app wm 通道后 present 域多客户端并存（各
//! 客户端经 wmd 中继同端到达 guifbd），三个结构性断裂随之激活：
//!
//! 1. **跨代 RELEASE 排空**：任一客户端的 RELEASE_WAIT（含新客户端 boot
//!    跨代清账）应答 = held 全量迁给请求者——他人在途别名被迁走（被迁方
//!    count=0）；
//! 2. **跨客户端 squash**：客户端 B 的 P2 SUBMIT 压弃客户端 A 的在途帧 →
//!    A 的 FRAME_CALLBACK 得 DISCARDED → A 侧 `frame status=2 → fail→park`
//!    永死（app 冻结 / launcher park = 桌面全灭实抓形态）；
//! 3. **credits 全局耗尽**：held 全局计 2——并发在途 + 死客户端孤儿别名
//!    永占 → 一切 SUBMIT ST_FULL → 全桌面连坐死。
//!
//! **为什么修在 wmd 不修在 guifbd**：guifbd 只见一条 present 通道（wmd
//! 中继换手件），wire 无会话标识——RELEASE_WAIT 无身份域，guifbd 结构性
//! 不可能知道「该客户端自己的别名」是哪些。客户端身份（来源通道）只存在
//! 于 wmd 分发层。本模块在 wmd 把 present 域还原成**单飞事务**：
//!
//! - **事务 = SUBMIT..RELEASE_WAIT**（客户端帧尾序：SUBMIT → FRAME_
//!   CALLBACK → FRAME_TIMING → RELEASE_WAIT）。SUBMIT/RELEASE 是事务
//!   边界 op；其余 present op（GETINFO/CALLBACK/TIMING/VOTE/POWER/PULSE）
//!   无事务副作用（即答或只 park 在属主自己的帧上），恒放行。
//! - **owner 单槽**：`SUBMIT` 从 owner=None 起（或 owner=self 续）获事务；
//!   他客户端事务在途 → **ST_BUSY 快答不转发**（随行别名关收——alias
//!   未达 guifbd，零孤儿面）。
//! - **RELEASE_WAIT 恒为终界**：属主 RELEASE = 事务收口（owner=None + 在
//!   wmd 侧视角 held 清账）；非属主 RELEASE（boot 跨代清账）在属主活跃
//!   窗内同样 BUSY 让位，属主空闲/帽外 = 抢占清场（先 [`DrainForward`]
//!   排空 guifbd held/park 残留再放行）。
//! - **租约（[`PRESENT_LEASE_NS`]）**：属主 last-activity 起 5s。合法事务
//!   （render+blit+3 RPC，TCG 重载实测 100-400ms/帧）远短于租约；属主
//!   mid-transaction 挂死（pump fail→park 非死、通道无错码——wmd 无死亡
//!   信号面）时租约到点 = 下一个 SUBMIT 抢占（drain 清场后放行），桌面
//!   最迟 5s 自愈而非永死。
//! - **死亡收口**：app 通道 recv 错码摘表 / DETACH 时（main 侧
//!   `table.app_alive` 对账），owner 属主失踪 → wmd 代发 RELEASE_WAIT
//!   排空孤儿（`relay::drain_orphans`）——「死 app 永占 credits」面就此
//!   结构性消失。
//!
//! 不变量（仲裁成立后 guifbd 侧自然成立）：owner=None ⇒ guifbd 无在途
//! 事务（pending 已 latch、held 只剩孤儿）⇒ drain 恰好只回收孤儿。
//!
//! 纯逻辑面（Port/RelayIo 缝外零 I/O）：host 单测全盖（真值表见 tests）。

use berry_sys::Handle;

/// 属主租约：last-activity 起 5s——合法事务上界（TCG 慢帧数百 ms）的
/// 一个量级余量；帽外 = 挂死属主可被抢占（桌面自愈上界 = 本值 + 一帧）。
pub const PRESENT_LEASE_NS: u64 = 5 * 1_000_000_000;

/// 事务边界 op 分类（main 分发层按 op 字节折此三值；非边界 op 恒放行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentOp {
    /// OP_SUBMIT：事务起点（占用 owner）。
    Submit,
    /// OP_RELEASE：事务终点（释放 owner；非属主形 = 清账/抢占面）。
    Release,
    /// 其余 present op：无事务副作用（属主活跃度刷新面）。
    Other,
}

/// 单飞闸门裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArbGate {
    /// 放行转发（guifbd 侧事务状态由 [`PresentArb::on_sent`] 记账）。
    Forward,
    /// 他客户端事务在途（租约窗内）——ST_BUSY 快答不转发（随行关收）。
    Busy,
    /// 属主挂死/空闲帽外——先代发 RELEASE_WAIT 清场再放行本请求
    /// （清场即抢占：guifbd held/park 残留全量回收后 owner=None）。
    DrainForward,
}

/// present 域单飞仲裁器（serve 循环单例——单线程独占，无锁面）。
pub struct PresentArb {
    /// 事务属主（客户端来源通道句柄；None = 无在途事务）。
    owner: Option<Handle>,
    /// 属主最近活跃时刻（任意已放行 present op 的 on_sent 拍；租约起算点）。
    owner_last_ns: u64,
}

impl PresentArb {
    /// 全清初始态（boot 后健康期）。
    pub const fn new() -> Self {
        PresentArb { owner: None, owner_last_ns: 0 }
    }

    /// 事务属主查询（main 死亡对账面）。
    pub fn owner(&self) -> Option<Handle> {
        self.owner
    }

    /// 请求闸门（转发前裁决）。`now` = 单调钟现值；`ch` = 请求来源通道；
    /// `op` = 事务边界分类。真值表：
    ///
    /// | op | owner | 裁决 |
    /// |----|-------|------|
    /// | Other | 任意 | Forward |
    /// | Submit/Release | None | Forward |
    /// | Submit/Release | Some(ch) | Forward（属主自续/自收） |
    /// | Submit/Release | Some(他) ∧ 活跃 | Busy |
    /// | Submit/Release | Some(他) ∧ 帽外 | DrainForward |
    pub fn gate(&mut self, now: u64, ch: Handle, op: PresentOp) -> ArbGate {
        match op {
            PresentOp::Other => ArbGate::Forward,
            PresentOp::Submit | PresentOp::Release => match self.owner {
                None => ArbGate::Forward,
                Some(o) if o == ch => ArbGate::Forward,
                Some(_) => {
                    if now.saturating_sub(self.owner_last_ns) > PRESENT_LEASE_NS {
                        ArbGate::DrainForward
                    } else {
                        ArbGate::Busy
                    }
                }
            },
        }
    }

    /// 已放行请求的记账（guifbd 消费请求后调用——main 以 RelayOutcome 区分
    /// 「已到达 guifbd」与「快答未转发」：未到达面不记账，owner 不动）。
    /// Release（无论属主是谁——DrainForward 清场后的非属主清账同拍）= 事务
    /// 终界：owner 清空。
    pub fn on_sent(&mut self, now: u64, ch: Handle, op: PresentOp) {
        match op {
            PresentOp::Submit => {
                self.owner = Some(ch);
                self.owner_last_ns = now;
            }
            PresentOp::Release => {
                self.owner = None;
            }
            PresentOp::Other => {
                if self.owner == Some(ch) {
                    self.owner_last_ns = now;
                }
            }
        }
    }

    /// 通道拆除/死亡对账：owner 属主失踪 → true（调用方触发 drain_orphans
    /// 清场；owner 随之清空）。非属主通道失踪 = 无事务面，false。
    pub fn forget(&mut self, ch: Handle) -> bool {
        if self.owner == Some(ch) {
            self.owner = None;
            true
        } else {
            false
        }
    }
}

impl Default for PresentArb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Handle = 11;
    const B: Handle = 22;
    const T0: u64 = 1_000_000_000;

    /// 真值表主形：None/self 放行；他属主活跃 Busy；租约帽外 DrainForward。
    #[test]
    fn gate_ownership_truth_table() {
        let mut arb = PresentArb::new();
        // 空闲：一切放行。
        assert_eq!(arb.gate(T0, A, PresentOp::Submit), ArbGate::Forward);
        assert_eq!(arb.gate(T0, A, PresentOp::Release), ArbGate::Forward);
        assert_eq!(arb.gate(T0, B, PresentOp::Other), ArbGate::Forward);

        // A 占事务：B 的边界 op 在租约内 Busy、Other 放行。
        arb.on_sent(T0, A, PresentOp::Submit);
        assert_eq!(arb.gate(T0 + 1, B, PresentOp::Submit), ArbGate::Busy);
        assert_eq!(arb.gate(T0 + 1, B, PresentOp::Release), ArbGate::Busy);
        assert_eq!(arb.gate(T0 + 1, B, PresentOp::Other), ArbGate::Forward);
        // A 自续/自收恒放行。
        assert_eq!(arb.gate(T0 + 1, A, PresentOp::Submit), ArbGate::Forward);
        assert_eq!(arb.gate(T0 + 1, A, PresentOp::Release), ArbGate::Forward);

        // 租约帽外（>5s 无活跃）：B 抢占清场。
        assert_eq!(
            arb.gate(T0 + PRESENT_LEASE_NS + 1, B, PresentOp::Submit),
            ArbGate::DrainForward
        );
        // 恰在帽上（==LEASE）不算帽外——严格 > 语义。
        assert_eq!(arb.gate(T0 + PRESENT_LEASE_NS, B, PresentOp::Submit), ArbGate::Busy);
    }

    /// 属主活跃度刷新：事务中途的 Other op（CALLBACK/TIMING）推租约——
    /// 慢而活的客户端不被误判挂死。
    #[test]
    fn other_ops_refresh_owner_lease() {
        let mut arb = PresentArb::new();
        arb.on_sent(T0, A, PresentOp::Submit);
        // 3s 处 A 的 CALLBACK 到达（活跃刷新）→ 5s 租约从新拍起算。
        arb.on_sent(T0 + 3 * 1_000_000_000, A, PresentOp::Other);
        assert_eq!(
            arb.gate(T0 + 3 * 1_000_000_000 + PRESENT_LEASE_NS, B, PresentOp::Submit),
            ArbGate::Busy
        );
        assert_eq!(
            arb.gate(T0 + 3 * 1_000_000_000 + PRESENT_LEASE_NS + 1, B, PresentOp::Submit),
            ArbGate::DrainForward
        );
        // 非属主的 Other 不刷新 A 的租约。
        let mut arb2 = PresentArb::new();
        arb2.on_sent(T0, A, PresentOp::Submit);
        arb2.on_sent(T0 + 4 * 1_000_000_000, B, PresentOp::Other);
        assert_eq!(
            arb2.gate(T0 + PRESENT_LEASE_NS + 1, B, PresentOp::Submit),
            ArbGate::DrainForward
        );
    }

    /// Release 终界 + 事务循环：A 收口后 B 立即可入——稳态无饿死。
    #[test]
    fn release_ends_transaction_next_client_proceeds() {
        let mut arb = PresentArb::new();
        arb.on_sent(T0, A, PresentOp::Submit);
        arb.on_sent(T0 + 1, A, PresentOp::Other);
        arb.on_sent(T0 + 2, A, PresentOp::Release);
        assert_eq!(arb.owner(), None);
        assert_eq!(arb.gate(T0 + 3, B, PresentOp::Submit), ArbGate::Forward);
        // 非属主 Release 放行后（清账/DrainForward 面）同样清空 owner。
        arb.on_sent(T0 + 4, B, PresentOp::Submit);
        arb.on_sent(T0 + 5, B, PresentOp::Release);
        assert_eq!(arb.owner(), None);
    }

    /// 死亡对账：属主通道拆除 → forget true（触发 drain）；非属主 false；
    /// forget 后空闲。
    #[test]
    fn forget_owner_on_channel_death() {
        let mut arb = PresentArb::new();
        assert!(!arb.forget(A)); // 空闲面
        arb.on_sent(T0, A, PresentOp::Submit);
        assert!(!arb.forget(B)); // 非属主
        assert!(arb.forget(A)); // 属主失踪
        assert_eq!(arb.owner(), None);
        assert_eq!(arb.gate(T0 + 1, B, PresentOp::Submit), ArbGate::Forward);
    }
}
