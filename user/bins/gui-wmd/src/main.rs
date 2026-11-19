//! gui-wmd —— GUI-5（ADR #61）窗口管理守护 + 波3 焦点/返回路由/事件通道：
//! 呈现（guifbd）/窗口策略（本服务）分离。guifbd 零改动；gfxdemo 改连
//! wm 域（同 op 换端——present/hdi 八+六 op 经本服务透传中继（refresh 刀
//! 1-6：0x07 INPUT_PULSE / 0x08 FRAME_TIMING / 0x44 PANEL_CAPS / 0x45
//! VBLANK_TIMING 入域），wm op 本地权威处理（0x60..=0x6A——K1 I1 增
//! 0x69 attach（on_wm 分发层随行开口）/0x6A detach + per-app apps 通道
//! 表：launcher/各 [ui] app 各独占一通道，应答参数化回来源通道——共享
//! dup(WMD_CLI_DUP) 单 FIFO 应答互截缺陷的结构性解））；inputd 追加推送
//! 端收窗口表快照（WM_TABLE_WAIT 阻塞应答形——波3 冻结面，零新增驻留者）。
//!
//! **boot 授能链**：EFI 相 svcmgr guifbd arm Ok 后 wmd arm 预拉本服务
//! （门 = present_cli≠0 ∧ cpio 命中），extras 4 枚 = [wm_srv（wm 客户端
//! 面）, push_srv（inputd 客户端面）, present_cli, hdi_cli（guifbd 客户端
//! 端换手件）] 加 base 4 恰 8/8（§7 Q12 冻结）。rootfs 相（-kernel 无
//! fb）svcmgr 探测失败不 spawn——本服务恒只在 EFI 链存活。
//!
//! **启动序五步**（§3 #4）：①收割四端（0 值 = 协议破缺面：eprintln +
//! park 挂起——**非 exit(0)**，svcmgr 已 ps_reg ALIVE，真退出无终态回写
//! 路径 = 永久 ALIVE 谎账；标记缺失 = smoke FAIL loud 兜底）→ ②present
//! 首发 RELEASE_WAIT 清前世孤儿别名 → ③present GETINFO 缓存几何（真值
//! 源恒 guifbd——wmd 不持第二份 present 常量副本，Q15）→ ④表初始化空表
//! version≥1 → ⑤`gui-wmd: up` 诊断行（小写 wmd 前缀防误配 `WM ok` 标记
//! ——本行永不进 MARKERS）转 serve。
//!
//! **serve poll-first（波3 事件通道 + K1 I1 per-app apps 面）**：每轮顶依次
//! ⓪poisoned 中继通道非阻塞清淤（停摆根治刀 §1.2——清到即丢+解污染，
//! 健康期 Clean fast path 零 I/O）→ ①返回兜底软定时到点检查（500ms 单槽
//! ——无在途帽不动作）→ ②事件通道非阻塞收割（BACK_ACK/订阅端忽略帧）
//! → ③wm_srv→push_srv→apps 通道段确定性轮询（各路至多一条：请求面严格
//! 串行 RPC；apps 段 recv 错码 = close+摘表——坏 app 客户端不杀服务）→
//! 全 EAGAIN 才 `berry_object_wait_many` 并等；wait 集 = wm_srv + push_srv + 事件通道
//! （属主+订阅者）+ apps（表序稳定）截 [`WAIT_SET_MAX`]=16（kernel
//! `WAIT_MANY_MAX` 单源镜像）——**越窗判定走需求总数**
//! `2 + wait_set_needed()`（不截断值——K1 I1 重造：旧 evt_handles 式拼装
//! 被出参物理截断恒 ≤16，越窗不可感 = 短帽死码坑）；越窗时截断优先级 =
//! wm_srv/push_srv/属主 evt 恒进，subs/apps 按序进到满，截断面靠 100ms
//! 短帽回环兜底无丢唤醒；deadline = min(空闲帽 10s, 返回兜底截止, 越窗
//! 短帽)——**进调用本身**（醒来后补查不够）；到点 = 合法空闲重臂续等
//! （真断链面 = recv 非 -EAGAIN 错码——wm_srv/push_srv/事件通道 trap，
//! apps 通道摘表）。每轮 poll 前重置 nb/nh 容量（recv 出参 in-out，漏
//! 重置 = 恒 -EINVAL 死循环）。
//!
//! **新鲜度结构注记（§7 Q10）**：单 wm 客户端串行下中继期间表不可能变 =
//! 零新鲜度损失——serve 单线程，中继透传（present/hdi RPC）阻塞期间不可
//! 能处理另一 CREATE/SET，TABLE_GET/TABLE_WAIT 应答携带的快照与请求到达
//! 时表态一致；gfxdemo 期 wm 客户端恰一个（严格串行 RPC 更无并发两请求）。
//! **K1 I1 起 per-app 多客户端成为常态**（launcher + 各 [ui] app 独占
//! 通道）：中继阻塞期间对端可变更表 → TABLE_GET 应答快照与「请求到达
//! 时」表态可能有出入（新鲜度窗口 = 一次中继时长）；某 app 的长 park
//! 中继（RELEASE_WAIT 等）会顺延他人——**停摆根治刀（§1）起顺延上界
//! = 分相帽**（send 10s / recv 2s，帽到点快答 ST_BUSY 返回 serve 不自
//! 杀；毒化通道清淤恢复——见 `relay` 模块注），新鲜度窗口同此上界；
//! 快照戳挂账后刀。
//! **波3 裁决（波3设计 §9）**：多客户端跟随面不走 TABLE_WAIT（冻结、
//! 零新增驻留者——parked 单车位仍只归 inputd push 面），表跟随一律走
//! 属主/订阅者事件通道 WM_EVENT_TABLE 通知 + TABLE_GET 快照（通知尽力
//! 而为，丢弃至多迟一拍）。
//!
//! **双面 crate（gui-inputd 先例）**：策略核在 lib（`gui_wmd::table`——
//! host 单测 mock 客户端全盖）；本文件 = guest 入口 I/O 壳（ChanPort 接
//! berry_sys）。berry-libc 按目标门控（host 测试构建空壳 main）。

#[cfg(target_os = "linux")]
use berry_guiproto as gp;
#[cfg(target_os = "linux")]
use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_clock_read,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_many,
    berry_object_wait_one, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, BERRY_ERR_TIMEDOUT,
    CLOCK_MONOTONIC, RIGHT_TRANSFER, SIGNAL_READABLE, SIGNAL_WRITABLE,
};

#[cfg(target_os = "linux")]
use gui_wmd::{AppRecv, Port, SendEvtStatus};
#[cfg(target_os = "linux")]
use gui_wmd::presarb::{ArbGate, PresentArb, PresentOp};
#[cfg(target_os = "linux")]
use gui_wmd::relay::{self, Domain, RelayIo, RelayState};
#[cfg(target_os = "linux")]
use gui_wmd::table::WmTable;

// 链接种子（guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc 的 crt0
// （含 `_start`）不进链接；`#[used]` 防死码消除。
#[cfg(target_os = "linux")]
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// serve 空闲并等帽：wait_many deadline 外层上界（合法空闲非死线——到点
/// 重臂续等；guifbd `IDLE_CAP_NS` 同款）。
#[cfg(target_os = "linux")]
const WMD_IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// wait 集越窗短帽：**需求总数**（2 底 + 属主 evt + subs + apps）越
/// [`WAIT_SET_MAX`] 截断时的重询周期——被截通道靠本帽回环兜底无丢唤醒
/// （BACK_ACK/请求至多迟一帽）。K1 I1 重造：旧拼装面被出参物理截断
/// 恒 ≤16，`n > WAIT_SET_MAX` 分支不可达 = 本帽死码（S1 实抓）；
/// `wait_set_needed` 需求总数外溢判定落成后本帽**首次真实武装**（apps
/// 8 路入集后越窗从边缘态变常态——2+15+4+8=29 > 16）。
#[cfg(target_os = "linux")]
const WMD_EVT_OVERFLOW_POLL_NS: u64 = 100 * 1_000_000;

/// wait_many 并等帽（kernel `WAIT_MANY_MAX` 单源镜像——sys_object_wait_many
/// n ∈ [1,16] 越界 -EINVAL）。
#[cfg(target_os = "linux")]
const WAIT_SET_MAX: usize = 16;

/// 单调钟现值（失败 = clock 面断链，loud）。
#[cfg(target_os = "linux")]
pub(crate) fn now_ns() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}

/// 应答 send（无随行 handle 面；EBUSY → wait WRITABLE 重臂重验）。与
/// guifbd 无限重臂不同：**总帽 10s 到点丢帧 + 诊断行 + 继续**（停摆根
/// 治刀 §1.6——旧 trap 面 = 「单个慢客户端不排空拖死全服务」的第二自
/// 杀面；快照真值恒经 TABLE_GET 可迟一拍重取，客户端自有 RPC 帽兜底）。
/// 芯在 [`relay::send_capped`]（host 单测 §4 d 直盖）。
#[cfg(target_os = "linux")]
pub(crate) fn send_rep(h: Handle, rep: &[u8]) {
    let mut p = ChanPort;
    if let relay::SendCap::Dropped = relay::send_capped(&mut p, h, rep, &[], relay::REPLY_SEND_CAP_NS) {
        println!("gui-wmd: send busy 帽到点（丢帧继续）");
    }
}

/// 应答 send（随行 handle 面——快照/present 域中继应答/gift ack：handle
/// 随应答迁回客户端表；帽语义同 [`send_rep`]——到点丢帧 + **随行关收**
/// + 继续）。
#[cfg(target_os = "linux")]
pub(crate) fn send_rep_handles(h: Handle, rep: &[u8], hs: &[Handle]) {
    let mut p = ChanPort;
    if let relay::SendCap::Dropped = relay::send_capped(&mut p, h, rep, hs, relay::REPLY_SEND_CAP_NS) {
        println!("gui-wmd: send busy 帽到点（丢帧关随行继续）");
    }
}

/// 1B 状态应答（非 OK 烘焙线 + DESTROY/SET 成功形——策略核 Port 面与
/// [`relay`] 拒线共用）。
#[cfg(target_os = "linux")]
pub(crate) fn st_rep(h: Handle, st: u8) {
    let mut out = [0u8; 1];
    let n = gp::encode_status_rep(&mut out, st);
    send_rep(h, &out[..n]);
}

/// close 请求随行迁入的全部 handle（拒收公共出口——坏客户端不得杀服务，
/// close 失败忽略）。
#[cfg(target_os = "linux")]
pub(crate) fn close_all(hs: &[Handle]) {
    for &h in hs {
        let _ = berry_handle_close(h);
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （guifbd 同款——链断 loud，标记缺失 = smoke FAIL 兜底）。
#[cfg(target_os = "linux")]
#[cold]
pub(crate) fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（0x4000_0000 与用户布局无交集——家族惯例值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}

/// Port 真实现（guest I/O 壳）：阻塞应答 = send 族（帽到点丢帧继续
/// ——§1.6）；事件面 = 恒非阻塞（满即丢/病态关闭归策略核裁量）；建联 =
/// channel_create options=0（intent 结算默认开——GUI 服务不 opt-out）。
#[cfg(target_os = "linux")]
struct ChanPort;

/// RelayIo 真实现（present/hdi 通道面——relay 芯的 guest 缝：非阻塞
/// 单发/收帧 + 分相等待 + 诊断行直打串口）。
#[cfg(target_os = "linux")]
impl RelayIo for ChanPort {
    fn now_ns(&mut self) -> u64 {
        now_ns()
    }
    fn send_try(&mut self, ch: Handle, rep: &[u8], hs: &[Handle]) -> relay::ChSend {
        let s = if hs.is_empty() {
            berry_channel_send(ch, rep.as_ptr(), rep.len(), core::ptr::null(), 0)
        } else {
            berry_channel_send(ch, rep.as_ptr(), rep.len(), hs.as_ptr(), hs.len())
        };
        match s {
            0 => relay::ChSend::Sent,
            BERRY_ERR_BUSY => relay::ChSend::Busy,
            s => relay::ChSend::Err(s),
        }
    }
    fn wait_writable(&mut self, ch: Handle, deadline: u64) -> bool {
        let mut obs = 0u64;
        berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut obs) == 0
    }
    fn recv_try(&mut self, cli: Handle, buf: &mut [u8], hout: &mut [Handle]) -> relay::ChRecv {
        let mut nb = buf.len();
        let mut nh = hout.len();
        let s = berry_channel_recv(cli, buf.as_mut_ptr(), &mut nb, hout.as_mut_ptr(), &mut nh);
        match s {
            0 => relay::ChRecv::Frame(nb, nh),
            BERRY_ERR_AGAIN => relay::ChRecv::Empty,
            s => relay::ChRecv::Err(s),
        }
    }
    fn wait_readable(&mut self, cli: Handle, deadline: u64) -> bool {
        let mut obs = 0u64;
        berry_object_wait_one(cli, SIGNAL_READABLE, deadline, &mut obs) == 0
    }
    fn close_all(&mut self, hs: &[Handle]) {
        for &h in hs {
            let _ = berry_handle_close(h);
        }
    }
    fn diag(&mut self, msg: &str, code: i32) {
        println!("{} {}", msg, code);
    }
    fn fail(&mut self) -> ! {
        trap()
    }
}

#[cfg(target_os = "linux")]
impl Port for ChanPort {
    fn now_ns(&mut self) -> u64 {
        now_ns()
    }
    fn send(&mut self, ch: Handle, rep: &[u8]) {
        send_rep(ch, rep)
    }
    fn send_handles(&mut self, ch: Handle, rep: &[u8], hs: &[Handle]) {
        send_rep_handles(ch, rep, hs)
    }
    fn send_nowait(&mut self, ch: Handle, frame: &[u8]) -> SendEvtStatus {
        let s = berry_channel_send(ch, frame.as_ptr(), frame.len(), core::ptr::null(), 0);
        match s {
            0 => SendEvtStatus::Queued,
            BERRY_ERR_BUSY => SendEvtStatus::Full,
            _ => SendEvtStatus::Dead,
        }
    }
    fn send_rep_try(&mut self, ch: Handle, rep: &[u8]) -> bool {
        // 内核 channel send 本就非阻塞（满即 -EBUSY）——单发即「试发不
        // 等待」；满/死 false 由调用方自决（app 通道拆除面跳过 ack）。
        berry_channel_send(ch, rep.as_ptr(), rep.len(), core::ptr::null(), 0) == 0
    }
    fn recv_nowait(&mut self, ch: Handle, buf: &mut [u8]) -> Option<usize> {
        let mut hout = [0 as Handle; 8];
        let mut nb = buf.len();
        let mut nh = hout.len();
        let s = berry_channel_recv(ch, buf.as_mut_ptr(), &mut nb, hout.as_mut_ptr(), &mut nh);
        match s {
            0 => Some(nb),
            BERRY_ERR_AGAIN => None,
            _ => {
                println!("gui-wmd: evt recv 失败 {}", s);
                trap(); // 非 -EAGAIN 错码 = 通道链断（boot 定值/建联面，真死亡面）
            }
        }
    }
    fn recv_app(&mut self, ch: Handle, buf: &mut [u8], hout: &mut [Handle]) -> AppRecv {
        let mut nb = buf.len();
        let mut nh = hout.len();
        let s = berry_channel_recv(ch, buf.as_mut_ptr(), &mut nb, hout.as_mut_ptr(), &mut nh);
        match s {
            0 => AppRecv::Frame(nb, nh),
            BERRY_ERR_AGAIN => AppRecv::Empty,
            s => {
                // app 通道是动态建联面——病态 = 摘表不杀服务（对照上面的
                // trap 面：那只适用 boot 定值柄 wm_srv/push_srv/事件通道）。
                println!("gui-wmd: app recv 失败 {} 摘除 {}", s, ch);
                AppRecv::Dead
            }
        }
    }
    fn dup_transfer(&mut self, h: Handle) -> Option<Handle> {
        let mut alias = 0 as Handle;
        if berry_handle_duplicate(h, RIGHT_TRANSFER, &mut alias) == 0 && alias != 0 {
            Some(alias)
        } else {
            None
        }
    }
    fn close(&mut self, h: Handle) {
        let _ = berry_handle_close(h);
    }
    fn channel_create(&mut self) -> Option<(Handle, Handle)> {
        let (mut srv, mut cli) = (0 as Handle, 0 as Handle);
        let s = berry_channel_create(0, &mut srv, &mut cli);
        if s == 0 {
            Some((srv, cli))
        } else {
            None
        }
    }
    fn fail(&mut self) -> ! {
        trap()
    }
}

/// wm_srv/apps 通道帧分发（§3 #6 分发表执法 + 波3 新 op + K1 I1 0x69）：
/// present 1..=8 → **presarb 单飞闸门后** present_cli 透传（refresh 刀3
/// A6：0x07 INPUT_PULSE 入域；刀4 P3：0x08 FRAME_TIMING 入域——呈现回执
/// 查询面；云桌面根治刀：SUBMIT/RELEASE 事务边界 op 受属主闸门——他客户
/// 端事务在途 ST_BUSY 快答不转发〔随行关收，alias 未达 guifbd 零孤儿〕，
/// 属主挂死租约帽外先 drain_orphans 清场再放行）；hdi 0x40..=0x45 →
/// hdi_cli 透传（refresh 刀1-6：0x44 PANEL_CAPS / 0x45 VBLANK_TIMING
/// 入域；桌面客户端只用 PANEL_CAPS 即答面——v1 不入闸门）；**0x69 ATTACH
/// 在 wm 域分发之前开口**（恰 1B + 随行恰 1 执法在本层——table::
/// handle_wm 的零随行一刀切门之前，wm 域唯一合法随行 op：随行 = 对端
/// srv 端，迁入 apps 表，ack/ST_FULL 经新通道回）；wm 0x60..=0x7F →
/// 本地表处理（0x60..=0x6A 十一枚中十枚：GETINFO/CREATE/DESTROY/SET/
/// TABLE_GET/TABLE_WAIT + 波3 FOCUS 0x66/BACK 0x67/SUBSCRIBE 0x68 + K1
/// DETACH 0x6A；0x6B..=0x7F 未定形 → 1B ST_BAD）；**其余一切 op（含
/// 0x20..=0x22 input 域错投、0x90..=0x9F 事件帧型错投）→ 1B ST_BAD +
/// close 随行**。`ch` = 帧来源通道（wm_srv 或 app 通道——应答参数化回
/// 它，多客户端隔离的结构基础；present 属主身份也以此为本）。中继臂过
/// [`RelayState`]（毒化账跨轮驻留——serve 循环持单例）+ [`PresentArb`]
/// （单飞属主账）。
#[cfg(target_os = "linux")]
fn on_wm(table: &mut WmTable, rstate: &mut RelayState, arb: &mut PresentArb,
         present_cli: Handle, hdi_cli: Handle, ch: Handle, msg: &[u8], hs: &[Handle]) {
    match msg.first() {
        Some(&op) if (gp::OP_GETINFO..=gp::OP_FRAME_TIMING).contains(&op) => {
            let pop = if op == gp::OP_SUBMIT {
                PresentOp::Submit
            } else if op == gp::OP_RELEASE {
                PresentOp::Release
            } else {
                PresentOp::Other
            };
            match arb.gate(now_ns(), ch, pop) {
                ArbGate::Busy => {
                    // 他客户端事务在途（租约窗内）——快答不转发（alias
                    // 零孤儿面；客户端 pump 侧步进重试）。
                    close_all(hs);
                    st_rep(ch, gp::ST_BUSY);
                }
                ArbGate::DrainForward => {
                    // 属主挂死/空闲帽外——先代发 RELEASE_WAIT 清场（held/
                    // parked 残留全量回收 + owner=None），成则放行本请求。
                    // 毒化在席 = 清场禁发（协议配对红线）——ST_BUSY 降级，
                    // serve 顶既有清淤路自愈后再试。
                    let mut port = ChanPort;
                    if relay::drain_orphans(rstate, &mut port, Domain::Present, present_cli) {
                        arb.on_sent(now_ns(), ch, PresentOp::Release);
                        relay_dispatch(rstate, arb, ch, pop, present_cli, msg, hs);
                    } else {
                        println!("gui-wmd: presarb drain 未成（毒化/帽到点——快答 ST_BUSY present）");
                        close_all(hs);
                        st_rep(ch, gp::ST_BUSY);
                    }
                }
                ArbGate::Forward => {
                    relay_dispatch(rstate, arb, ch, pop, present_cli, msg, hs);
                }
            }
        }
        Some(&op) if (gp::OP_REG_VBLANK..=gp::OP_VBLANK_TIMING).contains(&op) => {
            relay_dispatch(rstate, arb, ch, PresentOp::Other, hdi_cli, msg, hs);
        }
        // 0x69 开口：恰 1B + 随行恰 1（畸形/随行数失配 = ST_BAD + close
        // 随行——坏客户端不杀服务）。ack 走新通道（table::app_attach）。
        Some(&op) if op == gp::OP_WM_ATTACH => {
            if gp::parse_wm_attach_req(msg).is_some() && hs.len() == 1 {
                let mut port = ChanPort;
                table.app_attach(&mut port, hs[0]);
            } else {
                close_all(hs);
                st_rep(ch, gp::ST_BAD);
            }
        }
        Some(&op) if (gp::OP_WM_GETINFO..=0x7F).contains(&op) => {
            let mut port = ChanPort;
            table.handle_wm(&mut port, ch, msg, hs);
        }
        _ => {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
        }
    }
}

/// 中继单趟 + 返回契约消费（§1 诊断行映射——帽到点/毒化行保留旧字符串
/// 前缀「gui-wmd: relay send/recv …帽到点」供串口验尸预言与长稳探针对
/// 拍：修后这些行可现，但**不再**伴随 G1.5 kill 行）+ presarb 记账：请求
/// **已到达 guifbd** 的四态（Relayed/RecvCapPoisoned/StaleDropped/
/// ReplyDropped）on_sent（Submit 占主 / Release 清主 / Other 刷活跃）；
/// 未到达三态（BadAttach/SendBusy/PoisonedBusy）不动 owner——guifbd 侧
/// 事务状态与请求同步为零迁移。
#[cfg(target_os = "linux")]
fn relay_dispatch(rstate: &mut RelayState, arb: &mut PresentArb, ch: Handle, pop: PresentOp,
                  cli: Handle, msg: &[u8], hs: &[Handle]) {
    use gui_wmd::relay::RelayOutcome::*;
    let mut port = ChanPort;
    let dom = if matches!(msg.first(), Some(&op) if (gp::OP_REG_VBLANK..=gp::OP_VBLANK_TIMING).contains(&op)) {
        Domain::Hdi
    } else {
        Domain::Present
    };
    let name = match dom {
        Domain::Present => "present",
        Domain::Hdi => "hdi",
    };
    let out = rstate.relay(&mut port, dom, ch, cli, msg, hs);
    match out {
        Relayed => arb.on_sent(now_ns(), ch, pop),
        RecvCapPoisoned => {
            // 请求已被 guifbd 消费（应答未归毒化）——事务状态照记。
            arb.on_sent(now_ns(), ch, pop);
            println!("gui-wmd: relay recv 帽到点（快答 ST_BUSY+毒化 {}）", name);
        }
        StaleDropped => {
            arb.on_sent(now_ns(), ch, pop);
            println!("gui-wmd: relay 陈旧应答丢弃（{}）", name);
        }
        ReplyDropped => {
            arb.on_sent(now_ns(), ch, pop);
            println!("gui-wmd: send busy 帽到点（中继应答丢弃 {}）", name);
        }
        BadAttach => {} // 既有执法面（ST_BAD 应答已自答）——未到达
        SendBusy => println!("gui-wmd: relay send busy 帽到点（快答 ST_BUSY {}）", name),
        PoisonedBusy => println!("gui-wmd: relay 毒化期快答 ST_BUSY（{}）", name),
    }
}

/// push_srv 帧分发：**只收 TABLE_WAIT**（inputd 单客户端串行 WAIT 面——
/// 波3 冻结：唯一合法驻留者）；其余帧（含其它 wm op 错投）1B ST_BAD +
/// close 随行。
#[cfg(target_os = "linux")]
fn on_push(table: &mut WmTable, ch: Handle, msg: &[u8], hs: &[Handle]) {
    if !hs.is_empty() {
        close_all(hs);
        st_rep(ch, gp::ST_BAD);
        return;
    }
    match gp::parse_wm_table_wait_req(msg) {
        Some(last_seen) => {
            let mut port = ChanPort;
            table.wait(&mut port, ch, last_seen);
        }
        None => st_rep(ch, gp::ST_BAD),
    }
}

/// 单路轮询（一路至多一条：请求面客户端严格串行 RPC）。**每轮 poll 前
/// 重置 nb/nh 容量**（recv 出参 in-out，漏重置 = 容量 0 恒 -EINVAL 死
/// 循环）；出参 8 槽（wm 域零随行——非空即拒面 close；中继域随行透传）。
#[cfg(target_os = "linux")]
fn poll(ch: Handle) -> Option<(([u8; 256], usize), ([Handle; 8], usize))> {
    let mut msg = [0u8; 256];
    let mut hout = [0u32; 8];
    let mut nb = msg.len();
    let mut nh = hout.len();
    let s = berry_channel_recv(ch, msg.as_mut_ptr(), &mut nb, hout.as_mut_ptr(), &mut nh);
    if s == BERRY_ERR_AGAIN {
        return None;
    }
    if s != 0 {
        trap(); // 非 -EAGAIN 错码 = 通道链断（boot 定值接线，真死亡面）
    }
    Some(((msg, nb), (hout, nh)))
}

#[cfg(target_os = "linux")]
fn main() {
    // —— 启动序①：收割四端（extras 4 枚恰 8/8——Q12 冻结面）。0 值 =
    // 协议破缺（EFI 链 svcmgr arm Ok 恒随行）：eprintln + park 兜底。
    let wm_srv = berry_libc::io::wm_srv_h();
    if wm_srv == 0 {
        eprintln!("gui-wmd: no wm srv gift");
        berry_libc::park();
    }
    let push_srv = berry_libc::io::wm_push_srv_h();
    if push_srv == 0 {
        eprintln!("gui-wmd: no push srv gift");
        berry_libc::park();
    }
    let present_cli = berry_libc::io::wm_present_h();
    if present_cli == 0 {
        eprintln!("gui-wmd: no present cli gift");
        berry_libc::park();
    }
    let hdi_cli = berry_libc::io::wm_hdi_h();
    if hdi_cli == 0 {
        eprintln!("gui-wmd: no hdi cli gift");
        berry_libc::park();
    }

    // —— 启动序②：present 首发 RELEASE_WAIT 清前世孤儿别名（guifbd
    // boot 期空转即答——无长 park 边界问题；应答随行归还别名全量即关）。
    let mut port = ChanPort;
    relay::boot_release_all(&mut port, present_cli);

    // —— 启动序③：present GETINFO 缓存几何（快照头/wm GETINFO 的
    // disp_w/disp_h 域同源取值）。
    let info = relay::boot_getinfo(&mut port, present_cli);
    // wm wire 几何域 = u16——表示不了的屏 = 协议破缺面（宁可不起）。
    if info.display_w > u16::MAX as u32 || info.display_h > u16::MAX as u32 {
        eprintln!("gui-wmd: display {}x{} over u16", info.display_w, info.display_h);
        berry_libc::park();
    }

    // —— 启动序④：表初始化（空表 + version≥1：counter 起 1 → 冷启动
    // WAIT(0) 恒立即答；epoch = 启动单调钟派生 u8——无持久化下唯一跨
    // 启动区分源）。
    let mut table = WmTable::new(info.display_w as u16, info.display_h as u16, now_ns());
    // 中继毒化账（停摆根治刀 §1.2——present/hdi 独立标记，跨轮驻留）。
    let mut rstate = RelayState::new();
    // present 单飞仲裁账（云桌面根治刀——单事务属主 + 租约 + 死亡对账）。
    let mut arb = PresentArb::new();

    // —— 启动序⑤：诊断行转 serve（小写 wmd 前缀——永不进 MARKERS）。
    println!("gui-wmd: up");

    // —— serve poll-first：每轮顶 = ⓪poisoned 通道非阻塞清淤（§1.2——
    // 清到即丢+诊断行+解污染）+ 返回兜底软定时检查 + 事件通道收割
    // （BACK_ACK 至多迟一轮——请求面处理不饿死收割）；wm_srv→push_srv→
    // apps 通道段确定性轮询（各路至多一条）；全 EAGAIN 才 wait_many 并等
    // （deadline 进调用本身）。命中/到点皆回环顶重询（poll-first 无丢唤醒）。
    loop {
        // ⓪ poisoned 清淤（健康期 Clean fast path 零 I/O）。
        match rstate.drain(&mut port, Domain::Present, present_cli) {
            relay::DrainStale::Cleared => {
                println!("gui-wmd: relay 陈旧应答清淤解污染（present）")
            }
            _ => {}
        }
        match rstate.drain(&mut port, Domain::Hdi, hdi_cli) {
            relay::DrainStale::Cleared => println!("gui-wmd: relay 陈旧应答清淤解污染（hdi）"),
            _ => {}
        }
        table.on_back_tick(&mut port, now_ns());
        table.sweep_evt(&mut port);
        // presarb 死亡对账：owner 属主通道已被摘表/DETACH（poll_apps Dead
        // 面 / app_detach 自摘面）→ 清主 + 代发 drain 清场（孤儿别名/parked
        // 残留回收——「死 app 永占 credits」面结构性消失）。wm_srv（launcher
        // 端）除外——boot 定值柄不在 apps 表，其死亡面 = poll(wm_srv) 的
        // trap→监督重启既有链，不归本对账。app_alive 是 O(N) 定容扫描
        //（N≤8，每轮顶零成本）；drain 只在属主失踪的真死亡拍发生（阻塞 ≤
        // 一帧 park 上界）。
        if let Some(o) = arb.owner() {
            if o != wm_srv && !table.app_alive(o) {
                arb.forget(o);
                if !relay::drain_orphans(&mut rstate, &mut port, Domain::Present, present_cli) {
                    // 毒化在席/帽到点：孤儿面留待下一事务的租约抢占或
                    // serve 顶清淤路——不为死通道炸服务。
                    println!("gui-wmd: presarb 死亡 drain 未成（孤儿面待下次事务抢占）");
                }
            }
        }
        if let Some((msg, hs)) = poll(wm_srv) {
            on_wm(&mut table, &mut rstate, &mut arb, present_cli, hdi_cli, wm_srv, &msg.0[..msg.1], &hs.0[..hs.1]);
            continue;
        }
        if let Some((msg, hs)) = poll(push_srv) {
            on_push(&mut table, push_srv, &msg.0[..msg.1], &hs.0[..hs.1]);
            continue;
        }
        // apps 轮询段（K1 I1）：一路至多一帧（串行 RPC 纪律同上；命中帧
        // 同走 on_wm——应答参数化回该通道；病态通道在段内 close+摘表）。
        if let Some(f) = table.poll_apps(&mut port) {
            on_wm(&mut table, &mut rstate, &mut arb, present_cli, hdi_cli, f.ch, &f.msg[..f.n], &f.hs[..f.nh]);
            continue;
        }
        // wait 集 = [wm_srv, push_srv] + 事件通道（属主+订阅者）+ apps，
        // 截 WAIT_SET_MAX=16（截断优先级：wm_srv/push_srv/属主 evt 恒进，
        // subs/apps 按序到满）；**越窗判定走需求总数**（wait_set_needed
        // 不截断——旧 evt_handles 式拼装恒 ≤16 = 判定死码，K1 I1 重造）
        // + 100ms 短帽回环兜底。deadline = min(空闲帽 10s, 返回兜底截止,
        // 越窗短帽)——到点皆合法重臂。
        let mut hs = [0 as Handle; WAIT_SET_MAX];
        let sigs = [SIGNAL_READABLE; WAIT_SET_MAX];
        hs[0] = wm_srv;
        hs[1] = push_srv;
        let mut n = 2 + table.evt_handles(&mut hs[2..]);
        n += table.apps_handles(&mut hs[n..]);
        let need = 2 + table.wait_set_needed();
        let now = now_ns();
        let mut deadline = now + WMD_IDLE_CAP_NS;
        if let Some(dl) = table.back_deadline() {
            deadline = deadline.min(dl.max(now));
        }
        if need > WAIT_SET_MAX {
            n = WAIT_SET_MAX;
            deadline = deadline.min(now + WMD_EVT_OVERFLOW_POLL_NS);
        }
        let mut idx = 0usize;
        let mut obs = 0u64;
        let s = berry_object_wait_many(
            hs.as_ptr(),
            sigs.as_ptr(),
            n,
            deadline,
            &mut idx,
            &mut obs,
        );
        if s != 0 && s != BERRY_ERR_TIMEDOUT {
            trap(); // 到点 = 空闲重臂（回环顶兜底）；余错 = 死亡面
        }
    }
}

/// host 测试构建空壳（berry-libc 门控对偶——gui-inputd 同法）。
#[cfg(not(target_os = "linux"))]
fn main() {}
