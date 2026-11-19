//! present/hdi 双域透传中继芯（GUI-5 §3 #6 → 停摆根治刀 §1 去自杀化）——
//! guifbd 零改动，客户端端换手 wmd：wm 客户端的 present/hdi 请求原样转发
//! guifbd，应答原样回投（帧字节级零观测差）。**禁 wmd 合成 present 域
//! 成功应答**（真值源恒 guifbd，Q15）——本芯只转发收到的字节；wmd 自产
//! 面仅 wm 域与 1B 状态拒线/降级线（ST_BAD 拒收 + ST_BUSY 快答——状态
//! 字节非帧形真值应答，netdproto ST_BUSY 同款语义）。
//!
//! **去自杀化契约（§1，2026-11-19 定谳链改造）**：guifbd 的 parked 型
//! op（RELEASE/CALLBACK 的 latch 等待、VBLANK 拍点）应答前不回字节——旧
//! 中继在此挂满 300s 后 `trap()` 自杀（单线程 serve 整体停摆 → ps 谎账
//! 无拉起 → 整场永久死）。现行分相帽 + 快答 + 毒化清淤：
//!
//! - **send 相**（请求未交付 guifbd）：帽 [`RELAY_SEND_CAP_NS`] 10s（纯
//!   队列容量面——guifbd serve 活着时 poll-first 每轮排空，此面平时瞬
//!   时）；到点 = 干净失败——随行关收（send 未成 handle 仍归 wmd，不关
//!   即泄漏）+ 1B ST_BUSY 快答 + **返回 serve 继续**。
//! - **recv 相**（已交付、应答未归——危险面）：帽 [`RELAY_RECV_CAP_NS`]
//!   30s（审查修正：合法 park 上界 1s + TCG/vgpu 余量；真异常仍触发，
//!   防线 = 与 send 相分开，send 相不受此帽牵连）；到点 = ST_BUSY 快答 +
//!   该域标 **poisoned**（present/hdi 各自独立标记）。
//! - **poisoned 期间**：该域新中继一律立即 ST_BUSY 不转发（随行关收）；
//!   serve 每轮顶 [`RelayState::drain`] 非阻塞清淤——单线程串行 ⇒ 每次
//!   毒化至多 1 条陈旧应答在途（guifbd 一请求一应答，毒化期零转发），
//!   清到即丢（随行关收防 fence/别名泄漏）+ 解污染。guifbd 永不补发则
//!   永久 poisoned = 呈现面降级快失败，wm 域/推送/输入零影响。
//! - **陈旧应答先到**（§1.3 防御带）：recv 相收到帧时域已 poisoned →
//!   丢弃陈旧 + 本请求按超时处理（不配对错乱）。
//! - **保留 trap**（真协议破缺/链断——[`RelayIo::fail`] loud）：应答随行
//!   数超窗 / 空应答 / send 非 BUSY 错码 / recv 非 EAGAIN 错码。旧三层
//!   兜底链已被 e3dabfd46（客户端帽同步抬 300s）拆毁——「客户端先炸」
//!   前提不成立，失败面改由本芯自答，trap 只兜真死亡面（svcmgr 监督
//!   重启另刀）。
//! - **应答回投帽** [`REPLY_SEND_CAP_NS`] 10s（[`send_capped`]——原
//!   main.rs send_rep/send_rep_handles 的去自杀化芯）：慢客户端不排空
//!   10s = 丢该帧 + 随行关收 + 继续（快照真值恒经 TABLE_GET；客户端自
//!   有 RPC 帽兜底）——「单个慢客户端拖死全服务」的第二自杀面拆除。
//!
//! 双面 crate 纪律（gui-inputd 先例）：本芯入 lib（host 单测 mock 注入
//! guifbd 端满形/在途应答脚本位全盖 §4 测 a-d），I/O 全走 [`RelayIo`]
//! 缝，零 berry-libc/berry-sys 调用点（仅 `Handle = u32` 类型引用）；
//! 诊断行经 [`RelayIo::diag`]（错码 + 静态文案）与返回契约
//! [`RelayOutcome`]（guest 壳映射 println——帽到点/毒化行保留旧字符串
//! 前缀供串口验尸预言对拍）。
//!
//! RELEASE=ReleaseWait **长 park 边界**：运行期 wm 客户端 ReleaseWait
//! 经中继最多 park 一个 [`RELAY_RECV_CAP_NS`]（30s 到点即快答降级，不再
//! 挂死全服务——park 语义根治属 guifbd 刀另立）。present GETINFO 逐调用
//! 中继（真值源恒 guifbd——wmd 不持第二份 present 常量副本），启动期一
//! 次缓存例外（[`boot_getinfo`]）。

use berry_guiproto as gp;
use berry_sys::Handle;

/// 转发 send 相帽（§1.4 分相——纯队列容量面：guifbd serve 活着时此面
/// 平时瞬时；帽只兜 guifbd 死/停摆的持续 EBUSY，10s 足辨真伪）。
pub const RELAY_SEND_CAP_NS: u64 = 10 * 1_000_000_000;

/// 收应答 recv 相帽（§1.4 收紧 300s→30s——对抗审查四维一致 CONFIRMED
/// 修正：2s 会误杀 TCG 重载下的合法单 op 慢应答〔p6 保底 1s + 1080p
/// blit 数百 ms + vgpu reap ≤10s 帽 ⇒ 实测最坏 ~11s〕，且 appshell pump
/// 对非 OK 状态一律 fail loud——ST_BUSY 快答在瞬态带会间接杀客户端。
/// 30s = 合法上界 1s 的 30× TCG 裕量、≪ 客户端 300s 帽、≪ 旧 300s 楔
/// 死阈；防楔死内核（有界等待+不 trap+毒化清淤）零损——真异常仍在
/// 30s 内触发毒化快答，wmd 不再挂死整场）。
pub const RELAY_RECV_CAP_NS: u64 = 30 * 1_000_000_000;

/// 应答回投帽（§1.6——原 send_rep 10s 帽到点行为由 trap 改丢帧；帽值
/// 不变）。慢客户端（不排空应答通道 10s）= 丢该帧继续——单客户端慢不
/// 拖死全服务。
pub const REPLY_SEND_CAP_NS: u64 = 10 * 1_000_000_000;

/// 运行期孤儿清场帽（[`drain_orphans`]）：send/recv 两相各 35s——recv 相
/// 容忍 pending 在途 park 至 latch（合法上界一个帧周期）+ TCG/vgpu 余量
/// （RELAY_RECV_CAP_NS 30s 同源再放宽）；帽到点 = false 降级不 fail。
pub const RELAY_DRAIN_CAP_NS: u64 = 35 * 1_000_000_000;

/// boot 期 rpc0 帽（启动序②③专用：guifbd 空转即答无长 park；保持旧
/// 300s 宽帽防 TCG 竞速（宿主争用下 guest 帽翻车先例）误杀 boot 面——
/// boot 失败面仍 trap，监督重启归 svcmgr 刀）。
const RELAY_BOOT_CAP_NS: u64 = 300 * 1_000_000_000;

/// 中继域（present/hdi 独立毒化标记的分度——on_wm 按 op 带分派）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    /// present 域（op 1..=8，present_cli 通道）。
    Present,
    /// hdi 域（op 0x40..=0x45，hdi_cli 通道）。
    Hdi,
}

/// 非阻塞单发结果（RelayIo 收窄回传面——berry_sys 错码不进策略语义，
/// Err 携带原始 i32 仅供诊断行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChSend {
    /// 已入队（随行 handle 已迁出调用者表）。
    Sent,
    /// 对端队列满。
    Busy,
    /// 余错码（链断——loud 面）。
    Err(i32),
}

/// 非阻塞收帧结果（帧/随行已拷入出参窗，`(nbytes, nhandles)` 由枚举
/// 携带——出参 in-out 面收窄）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChRecv {
    /// 收到一帧。
    Frame(usize, usize),
    /// 空（-EAGAIN）。
    Empty,
    /// 余错码（链断——loud 面）。
    Err(i32),
}

/// 中继 I/O 缝（host 单测 mock + guest ChanPort 双实现——Port 缝的
/// present/hdi 通道面对偶：转发/收帧/分相等待/诊断）。
pub trait RelayIo {
    /// 单调钟现值（ns）。
    fn now_ns(&mut self) -> u64;
    /// 非阻塞单发（帧+随行 → 通道；随行仅在 Sent 时迁出）。
    fn send_try(&mut self, ch: Handle, rep: &[u8], hs: &[Handle]) -> ChSend;
    /// 等 WRITABLE 至 deadline：true = 可写信号（重验 send 兜底）/
    /// false = 到点（分相帽触发）。
    fn wait_writable(&mut self, ch: Handle, deadline: u64) -> bool;
    /// 非阻塞收帧（容量由出参窗长度自述；`Frame(nb, nh)` 已拷入）。
    fn recv_try(&mut self, cli: Handle, buf: &mut [u8], hout: &mut [Handle]) -> ChRecv;
    /// 等 READABLE 至 deadline：true = 可读信号 / false = 到点。
    fn wait_readable(&mut self, cli: Handle, deadline: u64) -> bool;
    /// close 一组 handle（拒收/丢弃随行的关收公共出口）。
    fn close_all(&mut self, hs: &[Handle]);
    /// 诊断行（错码 + 静态文案——guest 壳 println，测试记录面）。
    fn diag(&mut self, msg: &str, code: i32);
    /// 防御面 loud（不可达分支/真协议破缺——真实现自陷只死自己；测试
    /// panic）。
    fn fail(&mut self) -> !;
}

/// 单趟中继结果契约（main.rs 分派面消费——诊断行映射与停摆面审计；
/// 「成功 / 快答失败 / 毒化标记」三态定形）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayOutcome {
    /// 应答已透传回投客户端（正常全链）。
    Relayed,
    /// 应答回投帽到点丢帧（guifbd 侧配对完好、域未毒化——纯客户端慢
    /// 面，客户端自有 RPC 帽兜底）。
    ReplyDropped,
    /// 随行数失配拒收（ST_BAD + close——既有执法面）。
    BadAttach,
    /// send 相帽到点：随行已关收 + ST_BUSY 快答（干净失败，域未毒化）。
    SendBusy,
    /// recv 相帽到点：ST_BUSY 快答 + **域已毒化**（陈旧应答在途 1 条，
    /// 待 serve 顶 drain 清淤）。
    RecvCapPoisoned,
    /// 毒化期新中继快答（未转发 + 随行关收——域保持毒化）。
    PoisonedBusy,
    /// 陈旧应答先到丢弃 + 本请求按超时处理（§1.3 防御带——单线程下
    /// 由 ② 毒化快答前置不可达，结构性保险）。
    StaleDropped,
}

/// poisoned 通道清淤结果（serve 每轮顶 [`RelayState::drain`] 消费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainStale {
    /// 域未毒化（零动作——健康期每轮恒走此 fast path）。
    Clean,
    /// 毒化中但 FIFO 空（陈旧应答未归——保持毒化）。
    Still,
    /// 清到陈旧应答（丢弃 + 随行关收 + **解污染**——后续中继恢复转发）。
    Cleared,
}

/// 应答帽 send 结果（[`send_capped`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendCap {
    /// 已投出。
    Sent,
    /// 帽到点丢帧（随行已关收）。
    Dropped,
}

/// 双域中继毒化账（present/hdi 各自独立标记——单域停摆不传染他域）。
/// serve 循环持单例：[`RelayState::relay`] 读写标记、每轮顶
/// [`RelayState::drain`] 清淤。
pub struct RelayState {
    present: bool,
    hdi: bool,
}

impl RelayState {
    /// 全清初始态（boot 后健康期）。
    pub const fn new() -> Self {
        RelayState { present: false, hdi: false }
    }

    /// 域毒化查询（诊断/测试面）。
    pub fn poisoned(&self, dom: Domain) -> bool {
        match dom {
            Domain::Present => self.present,
            Domain::Hdi => self.hdi,
        }
    }

    fn slot(&mut self, dom: Domain) -> &mut bool {
        match dom {
            Domain::Present => &mut self.present,
            Domain::Hdi => &mut self.hdi,
        }
    }

    /// 透传单趟（§1.1-1.3 全契约）：随行数执法 → 毒化快答 → 转发（send
    /// 相帽 10s）→ 收应答（recv 相帽 30s）→ 原样回投（应答帽 10s）。recv
    /// 窗 `[u8;256]+[Handle;8]`（wm 客户端面按 CHAN_MSG_MAX/CHAN_HND_MAX
    /// 全窗收——应答超窗 = kernel 侧先拒，本面防御性 fail）。
    pub fn relay<R: RelayIo>(&mut self, io: &mut R, dom: Domain, ch: Handle, cli: Handle,
                             msg: &[u8], hs: &[Handle]) -> RelayOutcome {
        // —— ① 随行数执法（既有——send 即 take，违规面必须先收下再关，
        //    坏客户端不得杀服务）——
        if let Some((lo, hi)) = expect_attach(msg) {
            if hs.len() < lo || hs.len() > hi {
                io.close_all(hs);
                st_rep_cap(io, ch, gp::ST_BAD);
                return RelayOutcome::BadAttach;
            }
        }
        // —— ② 毒化期快答（§1.2——陈旧应答在途，转发 = 配对错乱面）——
        if self.poisoned(dom) {
            io.close_all(hs);
            st_rep_cap(io, ch, gp::ST_BUSY);
            return RelayOutcome::PoisonedBusy;
        }
        // —— ③ 转发 send 相（§1.1——帽到点：随行关收 + ST_BUSY + 继续
        //    serve；非 BUSY 错码 = 链断保留 loud）——
        let deadline = io.now_ns() + RELAY_SEND_CAP_NS;
        loop {
            match io.send_try(cli, msg, hs) {
                ChSend::Sent => break,
                ChSend::Busy => {
                    if !io.wait_writable(cli, deadline) {
                        io.close_all(hs);
                        st_rep_cap(io, ch, gp::ST_BUSY);
                        return RelayOutcome::SendBusy;
                    }
                }
                ChSend::Err(s) => {
                    io.diag("gui-wmd: relay send 失败", s);
                    io.fail();
                }
            }
        }
        // —— ④ 收应答 recv 相（§1.2——帽到点：ST_BUSY + 毒化；§1.3——
        //    陈旧先到丢弃；§1.5——防御两面 fail）——
        let deadline = io.now_ns() + RELAY_RECV_CAP_NS;
        let mut rep = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        loop {
            match io.recv_try(cli, &mut rep, &mut rhs) {
                ChRecv::Frame(nb, nh) => {
                    if nh > rhs.len() || nb == 0 {
                        io.fail(); // 应答随行数超窗 / 空应答——防御面（kernel 侧先拒）
                    }
                    if self.poisoned(dom) {
                        // §1.3 陈旧应答先到——丢弃（随行关收防 fence/别名
                        // 泄漏）+ 本请求按超时处理（不配对错乱）。
                        io.close_all(&rhs[..nh]);
                        st_rep_cap(io, ch, gp::ST_BUSY);
                        return RelayOutcome::StaleDropped;
                    }
                    return match send_capped(io, ch, &rep[..nb], &rhs[..nh], REPLY_SEND_CAP_NS) {
                        SendCap::Sent => RelayOutcome::Relayed,
                        SendCap::Dropped => RelayOutcome::ReplyDropped,
                    };
                }
                ChRecv::Empty => {
                    if !io.wait_readable(cli, deadline) {
                        // recv 相帽到点：请求已交付 guifbd、应答未归——毒
                        // 化防错配（至多 1 条陈旧在途，drain 收口）。
                        *self.slot(dom) = true;
                        st_rep_cap(io, ch, gp::ST_BUSY);
                        return RelayOutcome::RecvCapPoisoned;
                    }
                }
                ChRecv::Err(s) => {
                    io.diag("gui-wmd: relay recv 失败", s);
                    io.fail();
                }
            }
        }
    }

    /// poisoned 通道非阻塞清淤（§1.2——serve 每轮顶调用）：毒化期该 cli
    /// 通道上一切到达帧皆陈旧（单线程串行 ⇒ 至多 1 条在途；belt 逐条
    /// drain 到空）——清到即丢 + 随行关收 + 解污染；FIFO 空保持毒化
    /// （guifbd 永不补发 = 永久 poisoned，呈现面降级快失败）。
    pub fn drain<R: RelayIo>(&mut self, io: &mut R, dom: Domain, cli: Handle) -> DrainStale {
        if !self.poisoned(dom) {
            return DrainStale::Clean;
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
                    io.diag("gui-wmd: relay drain 失败", s);
                    io.fail();
                }
            }
        }
        if stale > 0 {
            *self.slot(dom) = false;
            DrainStale::Cleared
        } else {
            DrainStale::Still
        }
    }
}

/// 期望随行数（`Some((min, max))` = 执法；`None` = 不执法——畸形帧透传
/// 交 guifbd 拒（wmd 不做 present/hdi 域第二解析器，帧形校验单源）。
/// 定值：SUBMIT=恰 1（damage VMO）；PREPARE=恰 1（buffer VMO）；REG_
/// VBLANK 使能形恰 1（event）/关闭形 {0,1}（guifbd 服务端宽收同款）；
/// 余 op 恒 0。
fn expect_attach(msg: &[u8]) -> Option<(usize, usize)> {
    match msg.first() {
        Some(&gp::OP_SUBMIT) | Some(&gp::OP_PREPARE) => Some((1, 1)),
        Some(&gp::OP_REG_VBLANK) => match msg.get(1) {
            Some(&1) => Some((1, 1)),
            Some(&0) => Some((0, 1)),
            _ => None,
        },
        Some(&op)
            if (gp::OP_GETINFO..=gp::OP_FRAME_TIMING).contains(&op)
                || (gp::OP_VBLANK_WAIT..=gp::OP_VBLANK_TIMING).contains(&op) =>
        {
            Some((0, 0))
        }
        _ => None,
    }
}

/// 应答帽 send（§1.6——原 main.rs send_rep/send_rep_handles 的去自杀化
/// 芯）：EBUSY → wait WRITABLE 重臂重验；总帽到点 = 丢帧 + 随行关收
/// （send 未成 handle 未迁出，不关即泄漏）+ [`SendCap::Dropped`]；非
/// BUSY 错码 = 链断 loud。 ChanPort 的 Port::send/send_handles 与本芯
/// 回投面共用——全服务应答面统一「帽到点丢帧继续」语义。
pub fn send_capped<R: RelayIo>(io: &mut R, ch: Handle, rep: &[u8], hs: &[Handle],
                               cap_ns: u64) -> SendCap {
    let deadline = io.now_ns() + cap_ns;
    loop {
        match io.send_try(ch, rep, hs) {
            ChSend::Sent => return SendCap::Sent,
            ChSend::Busy => {
                if !io.wait_writable(ch, deadline) {
                    io.close_all(hs);
                    return SendCap::Dropped;
                }
            }
            ChSend::Err(s) => {
                io.diag("gui-wmd: send 失败", s);
                io.fail();
            }
        }
    }
}

/// 1B 状态应答（快答面共用——ST_BAD 拒线 / ST_BUSY 降级线；非 present
/// 域真值应答，Q15 语义见模块注）。
fn st_rep_cap<R: RelayIo>(io: &mut R, ch: Handle, st: u8) {
    let mut out = [0u8; 1];
    let n = gp::encode_status_rep(&mut out, st);
    send_capped(io, ch, &out[..n], &[], REPLY_SEND_CAP_NS);
}

/// 无随行单趟 RPC（启动期专用：②RELEASE_WAIT / ③GETINFO；boot 期
/// guifbd 空转即答无长 park，帽 [`RELAY_BOOT_CAP_NS`] 宽帽 + 失败面
/// 保留 fail loud——boot 破缺 = 服务不起，监督重启归 svcmgr 刀）。
fn rpc0<R: RelayIo>(io: &mut R, cli: Handle, req: &[u8]) -> ([u8; 256], usize, [Handle; 8], usize) {
    let deadline = io.now_ns() + RELAY_BOOT_CAP_NS;
    loop {
        match io.send_try(cli, req, &[]) {
            ChSend::Sent => break,
            ChSend::Busy => {
                if !io.wait_writable(cli, deadline) {
                    io.diag("gui-wmd: rpc send busy 帽到点", 0);
                    io.fail();
                }
            }
            ChSend::Err(s) => {
                io.diag("gui-wmd: rpc send 失败", s);
                io.fail();
            }
        }
    }
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let deadline = io.now_ns() + RELAY_BOOT_CAP_NS;
    loop {
        match io.recv_try(cli, &mut rep, &mut rhs) {
            ChRecv::Frame(nb, nh) => return (rep, nb, rhs, nh),
            ChRecv::Empty => {
                if !io.wait_readable(cli, deadline) {
                    io.diag("gui-wmd: rpc recv 帽到点", 0);
                    io.fail();
                }
            }
            ChRecv::Err(s) => {
                io.diag("gui-wmd: rpc recv 失败", s);
                io.fail();
            }
        }
    }
}

/// 运行期孤儿清场（presarb 配对面——DrainForward 抢占与属主通道死亡对账
/// 共用）：代发一趟 RELEASE_WAIT，guifbd 应答 = held 全量 + parked 槽清空
/// （pending 在途时 park 至 latch——合法上界一个帧周期；帽值
/// [`RELAY_DRAIN_CAP_NS`]）。应答随行全量关收（孤儿别名/属主残留别名都
/// 只是各客户端帧 VMO 的 dup——关 = 引用计数下降，原柄恒在各客户端）。
///
/// **毒化门（协议配对红线）**：`rstate` 该域 poisoned（上一趟中继应答
/// 未归、≤1 条陈旧帧在途）时**禁发**——本趟应答会与陈旧帧错配（reply
/// mismatch = 协议腐化）；返回 false 由 serve 顶既有 [`RelayState::drain`]
/// 清淤路自愈（guifbd 对 parked op 恒最终应答）。返回 false = 毒化在席/
/// 帽到点（标记毒化防后续错配）/发送失败——**不 fail 服务**，调用方
/// loud 一行 + ST_BUSY 快答降级。
pub fn drain_orphans<R: RelayIo>(
    rstate: &mut RelayState,
    io: &mut R,
    dom: Domain,
    cli: Handle,
) -> bool {
    if rstate.poisoned(dom) {
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
                    // RecvCapPoisoned 同形，毒化标记防后续错配。
                    *rstate.slot(dom) = true;
                    return false;
                }
            }
            ChRecv::Err(_) => return false,
        }
    }
}

/// 启动序②：present 首发 RELEASE_WAIT 清前世孤儿别名（wmd 崩溃重启
/// 残留——guifbd held 表持前世在途别名 credits 永占；guifbd :474-496
/// 语义零改动）。应答随行归还别名全量即关（本 wmd 不持 present credits
/// 状态——中继面无账可清）。
pub fn boot_release_all<R: RelayIo>(io: &mut R, cli: Handle) {
    let mut req = [0u8; 256];
    let n = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
        Some(n) => n,
        None => io.fail(), // 256B 窗恒容 4B 帧——防御面
    };
    let (rep, nb, rhs, nh) = rpc0(io, cli, &req[..n]);
    match gp::parse_release_rep(&rep[..nb]) {
        Some(rel) => {
            if nh != rel.count {
                io.fail(); // 随行数与 count 失配 = 协议破缺
            }
            io.close_all(&rhs[..nh]);
        }
        None => io.fail(), // boot 期 guifbd 恒在（wmd spawn 门在 guifbd Ok 后）
    }
}

/// 启动序③：present GETINFO 缓存几何（快照头/wm GETINFO 的 disp 域
/// 同源；此后 wm GETINFO 应答自缓存，present GETINFO 本域仍逐调用中继）。
pub fn boot_getinfo<R: RelayIo>(io: &mut R, cli: Handle) -> gp::GetinfoRep {
    let mut req = [0u8; 256];
    let n = match gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo) {
        Some(n) => n,
        None => io.fail(),
    };
    let (rep, nb, _rhs, nh) = rpc0(io, cli, &req[..n]);
    if nh != 0 {
        io.fail(); // GETINFO 应答零随行——防御面
    }
    match gp::parse_getinfo_rep(&rep[..nb]) {
        Some(info) => info,
        None => io.fail(),
    }
}

// —— host 单测（§4 测 a-d + 随行执法面钉板）：RelayIoMock 注入 guifbd 端
//    send 满形 / 在途应答 / 陈旧应答脚本位，三记录面（转发/应答/关收）
//    逐条断言 ——

/// 测试共用 mock（lib.rs 集成测（e）同用——pub(crate)）。
#[cfg(test)]
pub(crate) mod mock {
    use super::{ChRecv, ChSend, RelayIo};
    use berry_sys::Handle;
    use std::vec::Vec;

    /// mock guifbd 端 cli 通道句柄。
    pub(crate) const CLI: Handle = 90;
    /// mock wm 客户端应答通道句柄。
    pub(crate) const CH_WM: Handle = 1;
    const MS: u64 = 1_000_000;

    pub(crate) struct RelayIoMock {
        pub now: u64,
        pub cli: Handle,
        /// guifbd 端 send 满形注入（§4 a——转发 send_try 恒 Busy）。
        pub fwd_full: bool,
        /// 客户端端 send 满形注入（§4 d——应答回投恒 Busy）。
        pub rep_full: bool,
        /// cli 通道已到应答（含随行）。
        pub cli_inbox: Vec<(Vec<u8>, Vec<Handle>)>,
        /// 在途应答（绝对到达时刻, 帧, 随行）——wait_readable 到点投递。
        pub inflight: Option<(u64, Vec<u8>, Vec<Handle>)>,
        // 记录面：转发出的请求 / 回投的应答（含 1B 快答）/ 关收的 handle。
        pub sent: Vec<(Handle, Vec<u8>, Vec<Handle>)>,
        pub replies: Vec<(Handle, Vec<u8>, Vec<Handle>)>,
        pub closed: Vec<Handle>,
    }

    impl RelayIoMock {
        pub fn new(cli: Handle) -> Self {
            RelayIoMock {
                now: 1_000_000,
                cli,
                fwd_full: false,
                rep_full: false,
                cli_inbox: Vec::new(),
                inflight: None,
                sent: Vec::new(),
                replies: Vec::new(),
                closed: Vec::new(),
            }
        }

        /// 注入一条「已到达」cli 通道应答（陈旧清淤面）。
        pub fn arrive(&mut self, frame: &[u8], hs: &[Handle]) {
            self.cli_inbox.push((frame.to_vec(), hs.to_vec()));
        }

        /// 编排在途应答（`ms_from_now` 后到达——wait_readable 投递）。
        pub fn inflight_at(&mut self, ms_from_now: u64, frame: &[u8], hs: &[Handle]) {
            self.inflight = Some((self.now + ms_from_now * MS, frame.to_vec(), hs.to_vec()));
        }
    }

    impl RelayIo for RelayIoMock {
        fn now_ns(&mut self) -> u64 {
            self.now
        }
        fn send_try(&mut self, ch: Handle, rep: &[u8], hs: &[Handle]) -> ChSend {
            if ch == self.cli {
                if self.fwd_full {
                    return ChSend::Busy;
                }
                self.sent.push((ch, rep.to_vec(), hs.to_vec()));
                ChSend::Sent
            } else {
                if self.rep_full {
                    return ChSend::Busy;
                }
                self.replies.push((ch, rep.to_vec(), hs.to_vec()));
                ChSend::Sent
            }
        }
        fn wait_writable(&mut self, ch: Handle, deadline: u64) -> bool {
            // 满形双位：cli 通道看 fwd_full / 客户端通道看 rep_full——满即
            // 持续到帽（时钟步进到 deadline）。
            let full = if ch == self.cli { self.fwd_full } else { self.rep_full };
            if full {
                self.now = self.now.max(deadline);
                false
            } else {
                true
            }
        }
        fn recv_try(&mut self, cli: Handle, buf: &mut [u8], hout: &mut [Handle]) -> ChRecv {
            if cli != self.cli || self.cli_inbox.is_empty() {
                return ChRecv::Empty;
            }
            let (f, hs) = self.cli_inbox.remove(0);
            buf[..f.len()].copy_from_slice(&f);
            for (i, &h) in hs.iter().enumerate() {
                hout[i] = h;
            }
            ChRecv::Frame(f.len(), hs.len())
        }
        fn wait_readable(&mut self, cli: Handle, deadline: u64) -> bool {
            if cli != self.cli {
                return false;
            }
            if !self.cli_inbox.is_empty() {
                return true;
            }
            if let Some((at, f, hs)) = self.inflight.take() {
                if at <= deadline {
                    self.now = self.now.max(at);
                    self.cli_inbox.push((f, hs));
                    return true;
                }
                self.inflight = Some((at, f, hs)); // 未到——放回续等
            }
            self.now = self.now.max(deadline);
            false
        }
        fn close_all(&mut self, hs: &[Handle]) {
            self.closed.extend_from_slice(hs);
        }
        fn diag(&mut self, _msg: &str, _code: i32) {}
        fn fail(&mut self) -> ! {
            panic!("relay mock: 防御面 fail 触达（真协议破缺/不可达分支）");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mock::{RelayIoMock, CH_WM, CLI};
    use berry_guiproto as gp;

    /// (a) §1.1 send 相超时：guifbd 端满形注入 → 帽 10s 到点 = 快答 1B
    /// ST_BUSY + 随行关收（send 未成 handle 仍归 wmd）+ 未转发 + 域不
    /// 毒化（无在途应答——干净失败）+ relay 返回（serve 继续的结构前提）。
    #[test]
    fn relay_send_timeout_busy_reply_and_attach_closed() {
        let mut st = RelayState::new();
        let mut io = RelayIoMock::new(CLI);
        io.fwd_full = true; // guifbd 端 send 满形
        let msg = [gp::OP_SUBMIT, 0, 0, 0, 0]; // 随行恰 1 执法形
        let t0 = io.now;
        let out = st.relay(&mut io, Domain::Present, CH_WM, CLI, &msg, &[7]);
        assert_eq!(out, RelayOutcome::SendBusy);
        // 快答恰 1B ST_BUSY 落客户端通道。
        assert_eq!(io.replies, vec![(CH_WM, vec![gp::ST_BUSY], vec![])]);
        // 随行关收。
        assert!(io.closed.contains(&7), "send 超时随行关收");
        // 未转发。
        assert!(io.sent.is_empty());
        // send 相帽 = 10s（§1.4 分相）。
        assert_eq!(io.now - t0, RELAY_SEND_CAP_NS);
        // 干净失败不毒化：drain fast path = Clean。
        assert!(!st.poisoned(Domain::Present));
        assert_eq!(st.drain(&mut io, Domain::Present, CLI), DrainStale::Clean);
    }

    /// (b) §1.2 recv 相超时：应答不归 → 帽 30s 到点 = ST_BUSY 快答 + 域
    /// 毒化 + FIFO 空 drain 保持毒化 + 毒化期新中继立即 ST_BUSY 不转发
    /// （随行关收）+ hdi 域独立标记不受牵连。
    #[test]
    fn relay_recv_timeout_poisons_and_fast_fails_domain() {
        let mut st = RelayState::new();
        let mut io = RelayIoMock::new(CLI);
        let t0 = io.now;
        let out = st.relay(&mut io, Domain::Present, CH_WM, CLI, &[gp::OP_GETINFO], &[]);
        assert_eq!(out, RelayOutcome::RecvCapPoisoned);
        assert_eq!(io.replies, vec![(CH_WM, vec![gp::ST_BUSY], vec![])]);
        // recv 相帽 = 30s（§1.4 收紧 + 审查修正）。
        assert_eq!(io.now - t0, RELAY_RECV_CAP_NS);
        // 已转发（在途 1 条——毒化根因）。
        assert_eq!(io.sent.len(), 1);
        // 毒化标记立起；FIFO 空 → drain Still。
        assert!(st.poisoned(Domain::Present));
        assert_eq!(st.drain(&mut io, Domain::Present, CLI), DrainStale::Still);
        // 毒化期新中继：立即 ST_BUSY 不转发 + 随行关收。
        let out2 = st.relay(&mut io, Domain::Present, CH_WM, CLI, &[gp::OP_SUBMIT, 0, 0], &[9]);
        assert_eq!(out2, RelayOutcome::PoisonedBusy);
        assert_eq!(io.sent.len(), 1, "毒化期零转发");
        assert!(io.closed.contains(&9), "毒化快答随行关收");
        assert_eq!(io.replies.last(), Some(&(CH_WM, vec![gp::ST_BUSY], vec![])));
        // hdi 域独立标记——present 毒化不传染。
        assert!(!st.poisoned(Domain::Hdi));
    }

    /// (c) §1.2 清淤解污染：陈旧应答到达 → drain 丢弃（随行关收防
    /// fence/别名泄漏）+ 解污染 + 后续中继恢复转发透传。
    #[test]
    fn poisoned_drain_discards_stale_and_recovers() {
        let mut st = RelayState::new();
        let mut io = RelayIoMock::new(CLI);
        assert_eq!(
            st.relay(&mut io, Domain::Present, CH_WM, CLI, &[gp::OP_GETINFO], &[]),
            RelayOutcome::RecvCapPoisoned
        );
        assert_eq!(io.replies.len(), 1); // 首趟 ST_BUSY
        assert!(st.poisoned(Domain::Present));
        // 陈旧应答到达（随行 42 = PREPARE fence 形——丢弃必须关收）。
        io.arrive(&[gp::ST_OK, 1, 2, 3], &[42]);
        assert_eq!(st.drain(&mut io, Domain::Present, CLI), DrainStale::Cleared);
        assert!(io.closed.contains(&42), "陈旧应答随行关收");
        assert!(!st.poisoned(Domain::Present), "解污染");
        // 健康期 drain = Clean（零动作 fast path）。
        assert_eq!(st.drain(&mut io, Domain::Present, CLI), DrainStale::Clean);
        // 后续中继恢复：应答 1ms 后到——透传回投。
        io.inflight_at(1, &[gp::ST_OK, 9], &[]);
        let out = st.relay(&mut io, Domain::Present, CH_WM, CLI, &[gp::OP_GETINFO], &[]);
        assert_eq!(out, RelayOutcome::Relayed);
        assert_eq!(io.sent.len(), 2, "恢复转发");
        assert_eq!(io.replies.last(), Some(&(CH_WM, vec![gp::ST_OK, 9], vec![])));
    }

    /// (d) §1.6 应答帽 send：客户端端满形 10s 到点 = 丢帧 + 随行关收 +
    /// 不 fail（完赛即证）——「慢客户端拖死全服务」第二自杀面拆除；满
    /// 形解除后恢复投递。
    #[test]
    fn reply_send_cap_drops_frame_without_trap() {
        let mut io = RelayIoMock::new(CLI);
        io.rep_full = true; // 客户端端满形——应答回投恒 EBUSY
        let t0 = io.now;
        let out = send_capped(&mut io, CH_WM, &[gp::ST_OK], &[5], REPLY_SEND_CAP_NS);
        assert_eq!(out, SendCap::Dropped);
        assert!(io.replies.is_empty(), "帧已丢");
        assert!(io.closed.contains(&5), "随行关收（未迁出防泄漏）");
        assert_eq!(io.now - t0, REPLY_SEND_CAP_NS);
        // 满形解除 → 恢复投递。
        io.rep_full = false;
        assert_eq!(send_capped(&mut io, CH_WM, &[gp::ST_OK], &[], REPLY_SEND_CAP_NS), SendCap::Sent);
        assert_eq!(io.replies, vec![(CH_WM, vec![gp::ST_OK], vec![])]);
    }

    /// 随行数执法面钉板（原 main.rs 壳内逻辑迁 lib 后首获 host 覆盖）：
    /// SUBMIT 零随行失配 → ST_BAD + close；REG_VBLANK 关闭形 {0,1} 宽收。
    #[test]
    fn relay_attach_mismatch_still_bad() {
        let mut st = RelayState::new();
        let mut io = RelayIoMock::new(CLI);
        // SUBMIT 恰 1 执法——零随行失配。
        let out = st.relay(&mut io, Domain::Present, CH_WM, CLI, &[gp::OP_SUBMIT, 0], &[]);
        assert_eq!(out, RelayOutcome::BadAttach);
        assert_eq!(io.replies, vec![(CH_WM, vec![gp::ST_BAD], vec![])]);
        assert!(io.sent.is_empty(), "失配零转发");
        assert!(!st.poisoned(Domain::Present));
        // REG_VBLANK 关闭形随行 {0,1}——1 随行合法转发（应答即到）。
        io.inflight_at(0, &[gp::ST_OK], &[]);
        let out2 =
            st.relay(&mut io, Domain::Hdi, CH_WM, CLI, &[gp::OP_REG_VBLANK, 0, 0], &[3]);
        assert_eq!(out2, RelayOutcome::Relayed);
        assert_eq!(io.sent.len(), 1);
    }

    /// 孤儿清场（presarb 配对）：健康态清场 = 代发 RELEASE_WAIT + 应答
    /// 随行关收 + 成功；毒化在席 = 禁发（零 send——协议配对红线）；清场
    /// 帽到点 = 毒化标记（后续中继快答防线）。
    #[test]
    fn drain_orphans_poison_gate_and_cap() {
        // 健康态：应答（随行 2 别名）5ms 后到达 → 关收 + true
        //（`inflight_at` 首参 = 毫秒——mock 契约）。
        let mut st = RelayState::new();
        let mut io = RelayIoMock::new(CLI);
        io.inflight_at(5, &[gp::ST_OK, 0, 0], &[7, 8]);
        assert!(drain_orphans(&mut st, &mut io, Domain::Present, CLI));
        assert_eq!(io.sent.len(), 1, "代发恰一趟 RELEASE_WAIT");
        assert!(io.closed.contains(&7) && io.closed.contains(&8), "应答随行关收");

        // 毒化在席：禁发（零 send）+ false。
        let mut st2 = RelayState::new();
        let mut io2 = RelayIoMock::new(CLI);
        let _ = st2.relay(&mut io2, Domain::Present, CH_WM, CLI, &[gp::OP_GETINFO], &[]);
        assert!(st2.poisoned(Domain::Present));
        let mut io3 = RelayIoMock::new(CLI);
        assert!(!drain_orphans(&mut st2, &mut io3, Domain::Present, CLI));
        assert!(io3.sent.is_empty(), "毒化禁发");

        // 清场帽到点：本趟请求已交付、应答未归 → 毒化标记 + false。
        let mut st3 = RelayState::new();
        let mut io4 = RelayIoMock::new(CLI);
        assert!(!drain_orphans(&mut st3, &mut io4, Domain::Present, CLI));
        assert_eq!(io4.sent.len(), 1, "请求已交付");
        assert!(st3.poisoned(Domain::Present), "帽到点毒化标记");
    }
}
