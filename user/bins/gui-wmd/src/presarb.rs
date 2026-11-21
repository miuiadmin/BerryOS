//! presarb —— present 域单飞仲裁（云桌面反复卡死根治刀 · 架构面 A；
//! 2026-11-21 重做——8a0065f10 原样取回 + HEAD 嫁接，差异见文末注记）。
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
//!   apps 表在册对账），owner 属主失踪 → wmd 代发 RELEASE_WAIT
//!   排空孤儿（[`drain_orphans`]）——「死 app 永占 credits」面就此
//!   结构性消失。
//!
//! 不变量（仲裁成立后 guifbd 侧自然成立）：owner=None ⇒ guifbd 无在途
//! 事务（pending 已 latch、held 只剩孤儿）⇒ drain 恰好只回收孤儿。
//!
//! 纯逻辑面（Port/RelayIo 缝外零 I/O）：host 单测全盖（真值表见 tests）。
//!
//! **重做嫁接注记（2026-11-21，相对 8a0065f10 的差异，勿照抄原刀）**：
//! - 原刀随全量回退退役的近因 = svcmgr per-app input channel_pair() trap
//!   杀 init——本刀**不动 svcmgr**（inputfeeder 归后续 B-infra 刀），只做
//!   wmd 单侧。
//! - [`drain_orphans`] 原住 relay.rs（8a0065f10），本刀纪律限三文件
//!   （lib/main/presarb）迁入本模块；其清场帽到点的陈旧记账原置
//!   RelayState 毒化标记（HEAD 的 `slot` 私有，跨模块无法置位），改挂
//!   [`PresentArb`] 的 stale_release 位——效果面等价：present 域中继
//!   一律快答封锁 + serve 顶 [`PresentArb::sweep_stale`] 非阻塞回收清位。
//! - 死亡对账的在册查询原为 table::app_alive（随回退退役），main 侧改走
//!   `apps_handles` 现有面（纪律不动 table.rs）。

use berry_guiproto as gp;
use berry_sys::Handle;
use crate::relay::{ChRecv, ChSend, Domain, RelayIo, RelayState};

/// 属主租约：last-activity 起 5s——合法事务上界（TCG 慢帧数百 ms）的
/// 一个量级余量；帽外 = 挂死属主可被抢占（桌面自愈上界 = 本值 + 一帧）。
pub const PRESENT_LEASE_NS: u64 = 5 * 1_000_000_000;

/// 运行期孤儿清场帽（[`drain_orphans`]）：send/recv 两相各 35s——recv 相
/// 容忍 pending 在途 park 至 latch（合法上界一个帧周期）+ TCG/vgpu 余量
/// （RELAY_RECV_CAP_NS 30s 同源再放宽）；帽到点 = false 降级不 fail。
/// （8a0065f10 原名迁入——值与两相语义原样。）
pub const RELAY_DRAIN_CAP_NS: u64 = 35 * 1_000_000_000;

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
    /// 陈旧清场应答在席（嫁接面）：drain_orphans recv 相帽到点置位——
    /// 代发的 RELEASE_WAIT 应答在途未归，present 域中继一律快答封锁
    /// （配对红线），serve 顶 [`PresentArb::sweep_stale`] 回收清位。
    stale_release: bool,
}

impl PresentArb {
    /// 全清初始态（boot 后健康期）。
    pub const fn new() -> Self {
        PresentArb { owner: None, owner_last_ns: 0, stale_release: false }
    }

    /// 事务属主查询（main 死亡对账面）。
    pub fn owner(&self) -> Option<Handle> {
        self.owner
    }

    /// 陈旧清场应答在席查询（main present 域中继前置闸——嫁接面）。
    pub fn stale_release(&self) -> bool {
        self.stale_release
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

    /// serve 顶陈旧清场应答非阻塞回收（嫁接面）：drain_orphans 帽到点遗留
    /// 的迟到 RELEASE_WAIT 应答——非阻塞收帧（单飞下至多 1 条，belt 逐条
    /// drain 到空）→ 随行关收 + 清位。返回 true = 本拍回收（present 域
    /// 恢复放行）。guifbd 永不补发则永不清位 = 呈现面降级快失败（与
    /// RelayState 毒化面同哲）。
    pub fn sweep_stale<R: RelayIo>(&mut self, io: &mut R, cli: Handle) -> bool {
        if !self.stale_release {
            return false;
        }
        let mut rep = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let mut stale = 0usize;
        loop {
            match io.recv_try(cli, &mut rep, &mut rhs) {
                ChRecv::Frame(_, nh) => {
                    io.close_all(&rhs[..nh.min(rhs.len())]);
                    stale += 1;
                }
                ChRecv::Empty => break,
                ChRecv::Err(s) => {
                    io.diag("gui-wmd: presarb sweep 失败", s);
                    io.fail();
                }
            }
        }
        if stale > 0 {
            self.stale_release = false;
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

/// 运行期孤儿清场（presarb 配对面——DrainForward 抢占与属主通道死亡对账
/// 共用）：代发一趟 RELEASE_WAIT，guifbd 应答 = held 全量 + parked 槽清空
/// （pending 在途时 park 至 latch——合法上界一个帧周期；帽值
/// [`RELAY_DRAIN_CAP_NS`]）。应答随行全量关收（孤儿别名/属主残留别名都
/// 只是各客户端帧 VMO 的 dup——关 = 引用计数下降，原柄恒在各客户端）。
///
/// **毒化门（协议配对红线）双腿**：`rstate` 该域 poisoned（上一趟中继
/// 应答未归）或 [`PresentArb`] 陈旧清场应答在席（上一趟清场帽到点遗留）
/// 时**禁发**——本趟应答会与陈旧帧错配（reply mismatch = 协议腐化）；
/// 返回 false 由 serve 顶既有 [`RelayState::drain`]/[`PresentArb::sweep_stale`]
/// 清淤路自愈（guifbd 对 parked op 恒最终应答）。返回 false = 毒化在席/
/// 帽到点（陈旧标记防后续错配）/发送失败——**不 fail 服务**，调用方
/// loud 一行 + ST_BUSY 快答降级。
pub fn drain_orphans<R: RelayIo>(
    rstate: &mut RelayState,
    arb: &mut PresentArb,
    io: &mut R,
    dom: Domain,
    cli: Handle,
) -> bool {
    if rstate.poisoned(dom) || arb.stale_release {
        return false;
    }
    let mut req = [0u8; 256];
    let n = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
        Some(n) => n,
        None => return false, // 256B 窗恒容 4B 帧——防御面
    };
    let deadline = io.now_ns() + RELAY_DRAIN_CAP_NS;
    loop {
        match io.send_try(cli, &req[..n], &[]) {
            ChSend::Sent => break,
            ChSend::Busy => {
                if !io.wait_writable(cli, deadline) {
                    return false;
                }
            }
            ChSend::Err(_) => return false, // 链断面归毒化/relay 既有防线
        }
    }
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let deadline = io.now_ns() + RELAY_DRAIN_CAP_NS;
    loop {
        match io.recv_try(cli, &mut rep, &mut rhs) {
            ChRecv::Frame(nb, nh) => {
                // 毒化门已排除陈旧在途——本帧即本趟应答（帧形不细究：
                // drain 语义 = 「排空到空」，随行关收即返）。
                let _ = nb;
                io.close_all(&rhs[..nh.min(rhs.len())]);
                return true;
            }
            ChRecv::Empty => {
                if !io.wait_readable(cli, deadline) {
                    // 帽到点：本趟请求已交付 guifbd、应答未归——与中继
                    // RecvCapPoisoned 同形，陈旧标记防后续错配（嫁接面：
                    // 原刀此位置 RelayState 毒化位，效果等价）。
                    arb.stale_release = true;
                    return false;
                }
            }
            ChRecv::Err(_) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::mock::{RelayIoMock, CH_WM, CLI};
    use crate::relay::Domain;

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

    /// 孤儿清场毒化门/帽（8a0065f10 relay.rs 同名测迁入，断言面按嫁接
    /// 差异改陈旧位）：健康态清场 = 代发 RELEASE_WAIT + 应答随行关收 +
    /// true；RelayState 毒化在席 = 禁发（零 send）+ false；清场帽到点 =
    /// arb 陈旧标记 + false + 后续 drain 同禁发；迟到应答由 sweep_stale
    /// 非阻塞回收清位。
    #[test]
    fn drain_orphans_poison_gate_and_cap() {
        // 健康态：应答（随行 2 别名）5ms 后到达 → 关收 + true
        //（`inflight_at` 首参 = 毫秒——mock 契约）。
        let mut st = RelayState::new();
        let mut arb = PresentArb::new();
        let mut io = RelayIoMock::new(CLI);
        io.inflight_at(5, &[gp::ST_OK, 0, 0], &[7, 8]);
        assert!(drain_orphans(&mut st, &mut arb, &mut io, Domain::Present, CLI));
        assert_eq!(io.sent.len(), 1, "代发恰一趟 RELEASE_WAIT");
        assert!(io.closed.contains(&7) && io.closed.contains(&8), "应答随行关收");
        assert!(!arb.stale_release(), "成功清场零陈旧遗留");

        // 毒化在席（RelayState 面）：禁发（零 send）+ false。
        let mut st2 = RelayState::new();
        let mut io2 = RelayIoMock::new(CLI);
        let _ = st2.relay(&mut io2, Domain::Present, CH_WM, CLI, &[gp::OP_GETINFO], &[]);
        assert!(st2.poisoned(Domain::Present));
        let mut arb2 = PresentArb::new();
        let mut io3 = RelayIoMock::new(CLI);
        assert!(!drain_orphans(&mut st2, &mut arb2, &mut io3, Domain::Present, CLI));
        assert!(io3.sent.is_empty(), "毒化禁发");
        assert!(!arb2.stale_release(), "毒化拒发不涉陈旧标记");

        // 清场帽到点：本趟请求已交付、应答未归 → arb 陈旧标记 + false；
        // 陈旧在席（第二道门）= 后续 drain 同禁发（零 send）。
        let mut st3 = RelayState::new();
        let mut arb3 = PresentArb::new();
        let mut io4 = RelayIoMock::new(CLI);
        assert!(!drain_orphans(&mut st3, &mut arb3, &mut io4, Domain::Present, CLI));
        assert_eq!(io4.sent.len(), 1, "请求已交付");
        assert!(arb3.stale_release(), "帽到点陈旧标记");
        let mut io5 = RelayIoMock::new(CLI);
        io5.inflight_at(1, &[gp::ST_OK], &[]);
        assert!(!drain_orphans(&mut st3, &mut arb3, &mut io5, Domain::Present, CLI));
        assert!(io5.sent.is_empty(), "陈旧在席禁发");

        // 迟到应答回收：FIFO 空时 sweep 保持；到达后清位 + 随行关收。
        assert!(!arb3.sweep_stale(&mut io4, CLI), "应答未归保持陈旧");
        io4.arrive(&[gp::ST_OK, 1, 2], &[9]);
        assert!(arb3.sweep_stale(&mut io4, CLI), "迟到应答回收");
        assert!(io4.closed.contains(&9), "迟到应答随行关收");
        assert!(!arb3.stale_release(), "回收清位");
    }
}
