//! vnet TCP 会话驱动（T-c 自验 + T-d 服务化共用——「不长第二套」，
//! `docs/TCP栈设计草案.md` §3/§8）：guest 主动 connect SLIRP 宿主回显
//! 服务——10.0.2.2 = host 本机（QEMU user 网标准，guest 主动出站由 SLIRP
//! 转发宿主 127.0.0.1 同端口，**不用 hostfwd**——那是 host 连 guest 的反向
//! 面 v2）；宿主侧 = `tools/echod.py`（smoke/justfile 起，被动关闭——client
//! 恰好验主动关闭全路径）。
//!
//! 分工（§3 分层裁决）：berry-tcp 状态机管状态与序号推进，本模块管 IO——
//! Action 翻译成 tx_send、入段经四元组过滤翻译成 TcpSeg、RTO/TIME_WAIT
//! 定时由 now() 驱动（纯函数层不吃时钟）。组帧照 dns_roundtrip 家族：TCP
//! 段先进独立栈缓冲，再 eth/IP 层层包裹进 IOB（独立缓冲无借用冲突、无自
//! 重叠搬移——A-7「组帧在位就不要再搬」纪律）。
//!
//! [`Sess`] = 一次会话的完整生命周期（connect 握手 → N × roundtrip →
//! shutdown 挥手）：`selftest`（`TCP ok echo=64`）与 serve() 的 TCP 三 op
//! （T-d）同一驱动。定时面：RTO 挂臂（二刀臂语义 = 覆盖全环最晚段尾——
//! Syn/Data/Fin 出段即重挂、臂端只进不退；对账 = snd_una wrapping 覆盖臂
//! 尾或 state==Closed 即清——LastAck 例外：FIN 的 ACK 走 on_seg_lastack
//! 不推进 snd_una，靠 Closed 兜）+ TIME_WAIT 2s（**每步按状态重挂**——
//! v1 环境窗内唯一合法到达帧 = 重复 FIN 本就应重挂，杂散/被四元组过滤跳过
//! 的段重挂无害且各相位帽兜底）。重传载荷源 = [`Sess`] 的 tx 发送暂存
//!（op 首段整批拷入、重发按 (seq−tx_seq0) 切片；环空即失效——berry-tcp
//! 维持零拷贝纯状态机，载荷归属本 IO 层）。
//!
//! ---------- 会话表（A2-2 段③：LISTEN/ACCEPT/POLL + hostfwd 入站）----------
//!
//! [`TcpMux`] = 单收发线程的会话多路复用表（槽 = netdproto sid 域
//! 1..=TCP_SESSIONS_MAX）：Listen 口 + 被动打开（Pending→Ready）+ 已接
//! 受（Data）三类并存，[`route_seg`] 对每帧先四元组配既有会话、无匹配再
//! SYN×Listen 口被动打开（accept 帽满/无空槽 → 丢——TCP 规范不应答即
//! 丢）。**单线程互锁的结构性解**：客户端阻塞等握手（等对端 SYN-ACK）与
//! 服务端被动打开（等对端 SYN）在单线程里互为前置——入站自验
//! （[`listen_selftest`]，`LSN ok n=7`）把**全部会话（含客户端）进表**，
//! 由 [`drive_until`] 泵统一驱动（收帧路由 + RTO/TIME_WAIT 定时）；同一
//! 泵在 serve() 空闲窗步进（hostfwd 入站不等 client RPC——POLL 拉模型的
//! 服务端活性面）。阻塞法（connect/roundtrip/shutdown）保留给 session-0
//! legacy 面，其 step() 的非匹配帧从「丢弃」改路由进表——表空时行为与
//! v1 逐字节恒等（零漂移按构造成立）。hostfwd 环回自连链：guest 主动
//! connect 10.0.2.2:18081 → SLIRP 转发宿主 loopback:18081 → hostfwd 规则
//! 转回 guest:18081（echod 同机制的方向反转）；服务端会话四元组取自入
//! 站 SYN（对端口 = SLIRP 分配），客户端侧 peer 恒 10.0.2.2:18081。

use crate::{now, recv_frame, trap, tx_send, Dma, GW_IP, IOB, OUR_IP, POLL_STEP_NS};
use berry_netdproto as ndp;
use berry_netproto as np;
use berry_tcp as bt;

/// echod 端口（宿主 127.0.0.1:18080，`tools/echod.py` 定值）——自验目标。
const ECHO_PORT: u16 = 18080;
/// 自验源端口（避开 DNS 5353；四元组过滤 + SLIRP 原路回发）。serve() 的
/// 服务会话从 48081 起发号（main.rs `TCP_SPORT_NEXT`），与自验口错开。
const TCP_SPORT: u16 = 48080;
/// 自验各相位帽（ns）：握手/往返/挥手单相位预算（§6 RPC 成本模型：单帧
/// poll ~1s 级 TCG；成功路径三相位合计 ≈10 帧往返）。
const SELFTEST_CAP_NS: u64 = 60_000_000_000;
/// send_data 分批入环的阻塞泵帽（ns，二刀）：环满/零窗等位 ACK 的相位预算
///（serve RT 臂另持 TCPS_RT_CAP_NS 60s 收全 ACK——本帽只管入环相）。
const SEND_CAP_NS: u64 = 30_000_000_000;
/// 单帧组帧缓冲：TCP 头 20 + MSS 选项 4 + 段体 MSS=128 = 152B。
const SEG_BUF_SZ: usize = np::TCP_HDR_SZ + np::TCP_OPT_MSS_SZ + bt::MSS as usize;
/// 收段 payload 上限（首刀入站大段容忍：满 MTU TCP 段 1460B）。vnet 侧
/// 定值，与 bt::MSS（发送帽 + 通告几何）解耦——NET-01 §3.3 实证 SLIRP
/// 不读我方 MSS/WINDOW 通告、按 MTU 拼段，>128B 入站段是常态而非违约；
/// 发送面几何不动（停等 128B，吞吐抬升属二刀）。
const RX_SEG_MAX: usize = 1460;
/// 会话入站收缓冲幅（首刀 512→2048）：容纳 1×1460 大段 + 常规段余量。
/// 与 bt::WINDOW（通告 512）解耦——通告几何不动（窗口执法/滑窗属二刀），
/// 本值只钉「实际能收」；netdproto DRAIN 128B 拉模型天然多次拉取。表
/// 8 槽静态段随之 +约 11KB（boot 冒烟面，见 TCPTABLE 注）。
const RX_SESS_BUF: usize = 2048;

// T-c 对抗审查 ③（首刀换锚）：旧钉「对端按我方通告（MSS=128）回的最大
// 段」假设已被 NET-01 实证证伪——SLIRP 不读通告按 MTU 拼段，1460B 段是
// 常态入站。RX 侧改钉「满 MTU 段 + 帧开销 + vnet_hdr + 选项必须装进
// RX_BUF_SZ」（否则 main.rs recv_frame 的 len 检查拒收 → 入站黑洞第一道
// 闸复现）；recv_frame 整帧读入 IOB → RX_BUF_SZ ≤ IOB_SZ 同机器钉住。
// TX 侧整帧装进 IOB 不变。v2 抬过线即编译期炸响，A-4 SECTOR_SZ const
// assert 同族先例。
const _: () = assert!(
    berry_virtio::NET_HDR_SZ + np::ETH_HDR_SZ + np::IPV4_HDR_SZ + np::TCP_HDR_SZ
        + np::TCP_OPT_MSS_SZ + RX_SEG_MAX
        <= crate::RX_BUF_SZ
);
const _: () = assert!(
    berry_virtio::NET_HDR_SZ + np::ETH_HDR_SZ + np::IPV4_HDR_SZ + SEG_BUF_SZ
        <= crate::IOB_SZ
);
const _: () = assert!(crate::RX_BUF_SZ <= crate::IOB_SZ);
// T-d 对抗审查 ②（二刀换锚）：旧钉「TCP_DATA_MAX == bt::MSS」的单段假设
// 已由多段化裁决解除——TCP_DATA_MAX（RPC 单 op 载荷帽 250）与 bt::MSS
//（段长 128）解耦，一个 op 拆 ceil(250/128)=2 段在途。换双向几何钉：
// ① TCP_DATA_MAX > MSS（多段钉——单侧抬 MSS 回到一段/op 即编译期炸响）；
// ② TX_SLOTS × MSS ≥ TCP_DATA_MAX（在途环容量钉——缩环/抬帽即炸，op 整
// 批入环的容量不变式）。MSS 单侧改动另有 berry-tcp consts_pinned 字面
// 128 钉板把守。
const _: () = assert!(berry_netdproto::TCP_DATA_MAX > bt::MSS as usize);
const _: () = assert!(bt::TX_SLOTS * bt::MSS as usize >= berry_netdproto::TCP_DATA_MAX);

/// 回显载荷 64B（ICMP 32B probe 同族确定性 pattern）。
static PATTERN: &[u8; 64] = b"berryos-tcp-probe-0123456789012345678901234567890123456789abcdef";
const _: () = assert!(PATTERN.len() == 64);

/// RTO 挂臂：in-flight 段尾序号 + 到期时刻（wrapping 覆盖判定见下）。
#[derive(Clone, Copy)]
struct Arm {
    end: u32,
    deadline: u64,
}

/// roundtrip 结果（serve() 翻译成 netdproto st）。
pub(crate) enum RtRes {
    /// 收齐等长回显且全环 ACK 到（arm 清）——n = 回显字节数（out 前缀有效）。
    Ok(usize),
    /// 相位帽到（不销毁会话——在途环占用使后续 roundtrip 恒 Bad，
    /// 唯 close 可收口：FIN 排队尾、队头重发清偿）。
    Timeout,
    /// 会话态不可用（非 Established / 在途环占用 / 会话已被 RST 或重传
    /// 到帽终结）。
    Bad,
}

/// 一次 TCP 会话：状态机 + 四元组 + RTO/TIME_WAIT 挂臂。session-0 独占
/// （草案 §8：v1 并发 TCP 无面）——serve() 持 `Option<Sess>`，OPEN 撞在席
/// 会话 ST_BUSY；段③ 起表会话（TcpMux 槽）另持副本，rx 面见下。
#[derive(Clone, Copy)]
pub(crate) struct Sess {
    conn: bt::Conn,
    sport: u16,
    dport: u16,
    arm: Option<Arm>,
    tw: Option<u64>,
    /// 入站交付缓冲（段③ 表会话 rx 面）：无读端游标——读取走 poll_of 的
    /// rbytes 快照 + 自验对拍拷出（rx_copy）；客户端阻塞面（roundtrip）不
    /// 经此（交付直拷调用方 out）。幅 = RX_SESS_BUF（与 WINDOW 通告解耦，
    /// 见常量注；准入预检见 route_seg）。
    rx: [u8; RX_SESS_BUF],
    rx_len: usize,
    /// 发送暂存（二刀重传载荷源）：op 首段发送时整批拷入（≤TCP_DATA_MAX，
    /// 基点 tx_seq0），重发按 (seq−tx_seq0, len) 切片组帧——berry-tcp 维持
    /// 零拷贝纯状态机，载荷归属 IO 层。环空即失效（post_account 收账，下
    /// 一 op 重立基）；净生成面 = step/pump_timers 两处 RTO 路径 act(&[])。
    tx: [u8; ndp::TCP_DATA_MAX],
    tx_len: usize,
    tx_seq0: u32,
}

/// RTO 挂臂对账：snd_una wrapping 覆盖段尾（差值落在前半 = 已越过）或会话
/// 已终（LastAck 的 ACK 不推进 snd_una，靠 Closed 兜）即清。
fn arm_settled(conn: &bt::Conn, arm: &Arm) -> bool {
    conn.state == bt::State::Closed || conn.snd_una.wrapping_sub(arm.end) < 0x8000_0000
}

/// 组一个 TCP 段并发送（段先进独立栈缓冲，再层层包裹进 IOB——dns_roundtrip
/// 家族模板；返回值 None 契约 = 参数面错，一律 trap loud）。
#[allow(clippy::too_many_arguments)]
fn send_seg(
    d: &mut Dma,
    regs: *mut u32,
    mac: &[u8; 6],
    gw_mac: &[u8; 6],
    sport: u16,
    dport: u16,
    seq: u32,
    ack: u32,
    flags: np::TcpFlags,
    mss: Option<u16>,
    payload: &[u8],
) {
    let mut seg = [0u8; SEG_BUF_SZ];
    let tl = match np::encode_tcp(
        &mut seg, OUR_IP, GW_IP, sport, dport, seq, ack, flags, bt::WINDOW, mss, payload,
    ) {
        Some(n) => n,
        None => trap(),
    };
    {
        let iob: &mut [u8; crate::IOB_SZ] = unsafe { &mut *core::ptr::addr_of_mut!(IOB) };
        if np::encode_eth(&mut iob[berry_virtio::NET_HDR_SZ..], gw_mac, mac, np::ET_IPV4).is_none() {
            trap();
        }
        if np::encode_ipv4(
            &mut iob[berry_virtio::NET_HDR_SZ + np::ETH_HDR_SZ..],
            64,
            np::IP_PROTO_TCP,
            OUR_IP,
            GW_IP,
            &seg[..tl],
        )
        .is_none()
        {
            trap();
        }
    }
    tx_send(d, regs, np::ETH_HDR_SZ + np::IPV4_HDR_SZ + tl);
}

impl Sess {
    /// 主动打开会话构造（connect / 表 open_active 同构）。
    fn from_active(conn: bt::Conn, sport: u16, dport: u16) -> Sess {
        Sess {
            conn, sport, dport, arm: None, tw: None,
            rx: [0; RX_SESS_BUF], rx_len: 0,
            tx: [0; ndp::TCP_DATA_MAX], tx_len: 0, tx_seq0: 0,
        }
    }

    /// 被动打开会话构造（表 route_seg SYN×Listen 口）：sport = 本地监听口、
    /// dport = 对端源口——与主动面同向约定（出段 sport→dport、入段四元组
    /// 反查）。
    fn from_passive(conn: bt::Conn, local_port: u16, remote_port: u16) -> Sess {
        Sess {
            conn, sport: local_port, dport: remote_port, arm: None, tw: None,
            rx: [0; RX_SESS_BUF], rx_len: 0,
            tx: [0; ndp::TCP_DATA_MAX], tx_len: 0, tx_seq0: 0,
        }
    }

    /// 入站交付追加 rx：溢出在准入预检（route_seg 喂 on_seg 前的「宁拒不
    /// 腐化」闸 + session-0 同构预检）后结构性不可达——防御 trap。原「丢
    /// 新字节 + loud」策略的防御假设「SLIRP 遵守通告」已被 NET-01 实证
    /// 证伪；而状态机交付时 rcv_nxt 已推进，截断 = 字节永久丢 + ACK 谎报
    /// 的静默腐化面——闸门必须前移到状态机喂入之前（首刀核心正确性件）。
    fn rx_push(&mut self, del: &[u8]) {
        let room = self.rx.len() - self.rx_len;
        if del.len() > room {
            println!("vnet: tcp rx 溢出（防御面不应到达） {}->{} room={} got={}", self.dport, self.sport, room, del.len());
            trap();
        }
        self.rx[self.rx_len..self.rx_len + del.len()].copy_from_slice(del);
        self.rx_len += del.len();
    }

    /// 挂臂对账 + 发送暂存失效 + TIME_WAIT 按状态重挂（原 step 尾内联段，
    /// 段③ 提取——表槽会话喂完后同口径收账）。环空（全 ACK 收口）即暂存
    /// 失效——下一 op 首段重立基点。
    fn post_account(&mut self) {
        if self.conn.tx_inflight() == 0 {
            self.tx_len = 0;
        }
        if let Some(a) = &self.arm {
            if arm_settled(&self.conn, a) {
                self.arm = None;
            }
        }
        self.tw = if self.conn.state == bt::State::TimeWait {
            Some(now() + bt::TIME_WAIT_MS * 1_000_000)
        } else {
            None
        };
    }

    /// 主动打开：SYN → SYN-ACK → ACK 握手至 Established。整体帽到 / 重传
    /// 到帽 / RST → None（调用方按超时语义应答，会话不成立不留残）。
    pub(crate) fn connect(
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        table: &mut TcpMux,
        sport: u16,
        dport: u16,
        deadline: u64,
    ) -> Option<Sess> {
        let iss = now() as u32;
        let (conn, out) = bt::Conn::open(iss);
        let mut sess = Sess::from_active(conn, sport, dport);
        sess.act(d, regs, mac, gw_mac, out.action, &[]);
        // 幅 = TCP_DATA_MAX：握手相交付面防御（SYN-ACK 空载常态；对端同段
        // 捎带 ≤250 数据可容，越幅 = 准入拒段）。取舍：泵期交付字节被丢弃
        // （rcv_nxt 已推进 + 已 ACK，但 session-0 connect 无半途数据承接
        // 面）——SYN-ACK 捎带数据在 echod/SLIRP 域结构性不出现，不为此
        // 设通路。
        let mut scratch = [0u8; ndp::TCP_DATA_MAX];
        loop {
            if sess.conn.state == bt::State::Established {
                return Some(sess);
            }
            if sess.conn.state == bt::State::Closed {
                return None; // RST / 重传到帽
            }
            match sess.step(d, regs, mac, gw_mac, table, deadline, &mut scratch) {
                Ok(_) => {} // 交付字节弃置（取舍见上注释）
                Err(()) => return None, // 整体帽
            }
        }
    }

    /// 会话上发一批收等长回显（二刀多段化）：data 按 MSS 分批入环
    /// （128+…，in-flight>1；环/窗阻塞时 step 泵等位）→ 回显段交付拷入
    /// out → 全环 ACK 到（arm 清）即 Ok。echod 逐段回显——发送相泵期的
    /// 交付并入 got（先拷后发不丢字节）。相位帽 Timeout；会话被 RST/重传
    /// 到帽终结 Bad；上一 op 未收口（在途环占用）Bad（Timeout 契约延续）。
    pub(crate) fn roundtrip(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        table: &mut TcpMux,
        data: &[u8],
        out: &mut [u8],
        deadline: u64,
    ) -> RtRes {
        if self.conn.tx_inflight() != 0 {
            return RtRes::Bad; // 上一 op 在途未收口（老 in-flight 占位契约保持）
        }
        let mut got = 0usize;
        // 幅 = TCP_DATA_MAX：覆盖 SLIRP 不理通告形态的单段 250B 回显；越
        // 250 回显 = 契约外（step 准入拒段 + 相位帽 TIMEOUT）。
        let mut scratch = [0u8; ndp::TCP_DATA_MAX];
        // 发送相：分批入环。
        let mut off = 0usize;
        while off < data.len() {
            let end = (off + bt::MSS as usize).min(data.len());
            let ev = self.conn.send((end - off) as u32);
            if let bt::Action::Data { .. } = ev.action {
                self.act(d, regs, mac, gw_mac, ev.action, &data[off..end]);
                off = end;
                continue;
            }
            if self.conn.state != bt::State::Established {
                return RtRes::Bad; // 会话态不可用（RST/重传到帽/已收口）
            }
            // 环满/越窗：泵等位（ACK 弹槽/窗更新 ACK 解锁）。
            match self.step(d, regs, mac, gw_mac, table, deadline, &mut scratch) {
                Ok(n) => {
                    if got + n > out.len() {
                        return RtRes::Bad; // 超收防御（对端违约大段面，§5 第二道门）
                    }
                    out[got..got + n].copy_from_slice(&scratch[..n]);
                    got += n; // n=0 无交付，两行皆无操作
                }
                Err(()) => return RtRes::Timeout,
            }
        }
        // 收流相：回显收齐 + 全环 ACK 到。
        loop {
            if got == data.len() && self.arm.is_none() {
                return RtRes::Ok(got);
            }
            if self.conn.state == bt::State::Closed {
                return RtRes::Bad; // RST / 重传到帽
            }
            match self.step(d, regs, mac, gw_mac, table, deadline, &mut scratch) {
                Ok(n) => {
                    if got + n > out.len() {
                        return RtRes::Bad; // 超收防御（对端违约大段面，§5 第二道门）
                    }
                    out[got..got + n].copy_from_slice(&scratch[..n]);
                    got += n; // n=0 无交付，两行皆无操作
                }
                Err(()) => return RtRes::Timeout,
            }
        }
    }

    /// 主动关闭：close()（Established→FinWait1 / CloseWait→LastAck，其余态
    /// 无动作照常驱动剩余挥手）→ 挥手四帧 + TIME_WAIT 至 Closed。正常收口
    /// true；相位帽 false（会话由调用方弃置——内存态，无设备面残留）。
    pub(crate) fn shutdown(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        table: &mut TcpMux,
        deadline: u64,
    ) -> bool {
        if self.conn.state == bt::State::Closed {
            return true; // 幂等收口（前次 op 已终）
        }
        let ev = self.conn.close();
        self.act(d, regs, mac, gw_mac, ev.action, &[]);
        // 幅 = TCP_DATA_MAX：挥手相对端末段数据 ≤250 可交付（FIN 同段捎带
        // 面）；越幅 = 准入拒段靠对端重传。取舍：可交付 ≠ 被消费——泵期
        // 交付字节被静默丢弃（rcv_nxt 已推进 + 已 ACK，对端不再重传，
        // 挥手相带尾数据无人承接），echod/SLIRP 域挥手空载结构性不达。
        let mut scratch = [0u8; ndp::TCP_DATA_MAX];
        loop {
            if self.conn.state == bt::State::Closed {
                return true;
            }
            match self.step(d, regs, mac, gw_mac, table, deadline, &mut scratch) {
                Ok(_) => {} // 交付字节弃置（取舍见上注释）
                Err(()) => return false,
            }
        }
    }

    /// 单步推进：deadline = min(相位帽, RTO 挂臂, TIME_WAIT, **表最早定时**
    /// ——段③：阻塞 legacy op 期间表会话的 RTO/TIME_WAIT 也准点唤醒，不被
    /// legacy 相位帽压到错过重传窗) 收帧——目标段（四元组过滤 + parse）→
    /// on_seg → 交付拷 rcv（**先拷后应答**——deliver 借用收帧视图，act 写
    /// IOB 即失效，T-c 实踩）→ act（应答/重挂）→ 挂臂对账 + TIME_WAIT 按
    /// 状态重挂 + 表定时泵；**非匹配 GW TCP 段路由进表**（段③：杂散非 TCP
    /// 帧仍跳过——表空时无槽可配等同丢弃，行为与 v1 逐字节恒等）。到点：
    /// RTO 挂臂到 → on_rto 重发重挂 / TIME_WAIT 到 → expire / 表定时到 →
    /// 泵后 Ok(0) 续驱动（自会话无事件）/ 其余 = 相位帽 Err。返回交付字节数
    /// （0 = 本步无交付）。
    fn step(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        table: &mut TcpMux,
        cap: u64,
        rcv: &mut [u8],
    ) -> Result<usize, ()> {
        let mut dl = cap;
        if let Some(a) = &self.arm {
            dl = dl.min(a.deadline);
        }
        if let Some(t) = self.tw {
            dl = dl.min(t);
        }
        if let Some(t) = table.earliest_timer() {
            dl = dl.min(t);
        }
        let frame = match recv_frame(d, regs, dl) {
            Some(f) => f,
            None => {
                let t = now();
                if let Some(a) = self.arm {
                    if t >= a.deadline {
                        // RTO 到：on_rto 重发队头（act 内 arm_extend 保尾重挂
                        //——backoff 已 +1 用新值；重发载荷切 Sess.tx 暂存）；
                        // 无重发面（环空/到帽放弃 → Closed）清臂。
                        let ev = self.conn.on_rto();
                        if matches!(ev.action, bt::Action::None) {
                            self.arm = None;
                        } else {
                            self.act(d, regs, mac, gw_mac, ev.action, &[]);
                        }
                        pump_timers(d, regs, mac, gw_mac, table);
                        return Ok(0);
                    }
                }
                if let Some(x) = self.tw {
                    if t >= x {
                        self.conn.expire_timewait();
                        self.tw = None;
                        pump_timers(d, regs, mac, gw_mac, table);
                        return Ok(0);
                    }
                }
                pump_timers(d, regs, mac, gw_mac, table);
                if t >= cap {
                    return Err(()); // 相位帽
                }
                return Ok(0); // 表定时到点唤醒（自会话无事件）——续驱动
            }
        };
        // 目标段喂状态机；非匹配 GW TCP 段路由进表。payload 先拷 pbuf——
        // deliver/路由都借用收帧 IOB 视图，任何 act 写 IOB 即失效（T-c
        // 「先拷后应答」的路由版）。
        let mut delivered = 0usize;
        let mut pbuf = [0u8; RX_SEG_MAX];
        if let Some((sport, dport, seg)) = parse_gw_tcp(frame, &mut pbuf) {
            if sport == self.dport && dport == self.sport {
                // 准入预检（宁拒不腐化，route_seg 同构）：交付缓冲放不下
                // 整段 → 不喂状态机、回当前 ACK（重复 ACK）靠对端重传
                // ——rcv_nxt 不推进；截断交付 = rcv_nxt 已推进 + 字节丢 =
                // ACK 谎报的静默腐化面。客户端 op 契约收等长 ≤MSS 回显，
                // 超规格回显语义外 = 相位帽 TIMEOUT（首刀后不再有截断
                // 腐化可能，行为变化仅消除腐化面）。
                if seg.payload.len() > rcv.len() {
                    println!(
                        "vnet: tcp session-0 准入拒段 dport={} seg={} rcv={}",
                        self.dport, seg.payload.len(), rcv.len()
                    );
                    self.act(d, regs, mac, gw_mac, bt::Action::Ack, &[]);
                } else {
                    let out = self.conn.on_seg(&seg);
                    if let Some(del) = out.deliver {
                        let n = del.len().min(rcv.len());
                        rcv[..n].copy_from_slice(&del[..n]);
                        delivered = n;
                    }
                    self.act(d, regs, mac, gw_mac, out.action, &[]);
                }
            } else {
                route_seg(d, regs, mac, gw_mac, table, sport, dport, &seg);
            }
        }
        // 挂臂对账 + TIME_WAIT 按状态重挂（模块头注）+ 表定时泵。
        self.post_account();
        pump_timers(d, regs, mac, gw_mac, table);
        Ok(delivered)
    }

    /// Action → wire 段统一翻译 + RTO 挂臂（二刀臂语义升级为「覆盖全环最晚
    /// 段尾」：Syn/Data/Fin 出段即重挂，臂端只进不退——RTO 队头重发不清
    /// 尾，部分 ACK 不清臂、snd_una 推进后 arm_settled 判全环清；on_rto
    /// 重发同经此处按 backoff+1 重挂；Ack 无需重传不挂）。Data 臂双面：
    /// 初次发送（data 非空且恰长）拷入暂存再组帧；重发（data 空）按
    /// (seq−tx_seq0) 切暂存组帧——失配/无暂存/越界 trap loud（调用方契约
    /// 违约不静默）。
    fn act(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        action: bt::Action,
        data: &[u8],
    ) {
        // 臂端 wrapping 取更晚（只进不退：重发队头不改尾、新段扩尾）。
        let arm_extend = |prev: Option<Arm>, end: u32, backoff: u8| {
            let end = match prev {
                Some(a) if end.wrapping_sub(a.end) < 0x8000_0000 => a.end,
                _ => end,
            };
            Arm { end, deadline: now() + bt::rto_ms(backoff) * 1_000_000 }
        };
        match action {
            bt::Action::None => {}
            bt::Action::Syn { seq } => {
                send_seg(
                    d, regs, mac, gw_mac, self.sport, self.dport, seq, 0,
                    np::TcpFlags { syn: true, ..Default::default() }, Some(bt::MSS), &[],
                );
                self.arm = Some(arm_extend(self.arm, seq.wrapping_add(1), self.conn.rto_backoff));
            }
            // 服务端握手应答（A2-2 段② 与 berry-tcp 服务端半边同步入册；会话表
            // 接线在段③——本臂 = SYN|ACK 线上编码 + RTO 挂臂（SYN 占一序号同 Syn）。
            bt::Action::SynAck { seq, ack } => {
                send_seg(
                    d, regs, mac, gw_mac, self.sport, self.dport, seq, ack,
                    np::TcpFlags { syn: true, ack: true, ..Default::default() }, Some(bt::MSS), &[],
                );
                self.arm = Some(arm_extend(self.arm, seq.wrapping_add(1), self.conn.rto_backoff));
            }
            bt::Action::Data { seq, len } => {
                let payload: &[u8] = if !data.is_empty() {
                    // 初次发送：整批拷入暂存（基点未立先立——环空后首段）。
                    if data.len() != len as usize {
                        println!("vnet: tcp data 段失配 len={} data={}", len, data.len());
                        trap();
                    }
                    if self.tx_len == 0 {
                        self.tx_seq0 = seq;
                    }
                    let off = seq.wrapping_sub(self.tx_seq0) as usize;
                    if off + data.len() > self.tx.len() {
                        println!(
                            "vnet: tcp tx 暂存越界 seq=0x{:x} base=0x{:x} len={} cap={}",
                            seq, self.tx_seq0, data.len(), self.tx.len()
                        );
                        trap();
                    }
                    self.tx[off..off + data.len()].copy_from_slice(data);
                    if off + data.len() > self.tx_len {
                        self.tx_len = off + data.len();
                    }
                    &data[..]
                } else {
                    // 重发：切暂存（RTO 路径零调用方上下文——载荷源唯一）。
                    let off = seq.wrapping_sub(self.tx_seq0) as usize;
                    if self.tx_len == 0 || off + len as usize > self.tx_len {
                        println!(
                            "vnet: tcp tx 重发无源 seq=0x{:x} base=0x{:x} len={} stash={}",
                            seq, self.tx_seq0, len, self.tx_len
                        );
                        trap();
                    }
                    &self.tx[off..off + len as usize]
                };
                send_seg(
                    d, regs, mac, gw_mac, self.sport, self.dport, seq, self.conn.rcv_nxt,
                    np::TcpFlags { ack: true, ..Default::default() }, None, payload,
                );
                self.arm = Some(arm_extend(self.arm, seq.wrapping_add(len), self.conn.rto_backoff));
            }
            bt::Action::Fin { seq } => {
                send_seg(
                    d, regs, mac, gw_mac, self.sport, self.dport, seq, self.conn.rcv_nxt,
                    np::TcpFlags { fin: true, ack: true, ..Default::default() }, None, &[],
                );
                self.arm = Some(arm_extend(self.arm, seq.wrapping_add(1), self.conn.rto_backoff));
            }
            bt::Action::Ack => {
                send_seg(
                    d, regs, mac, gw_mac, self.sport, self.dport, self.conn.snd_nxt,
                    self.conn.rcv_nxt, np::TcpFlags { ack: true, ..Default::default() }, None, &[],
                );
            }
        }
    }
}

// ==================== 会话表（A2-2 段③）====================

/// 表容量 = netdproto sid 域上限（sid = 槽号+1，1..=TCP_SESSIONS_MAX）。
const SLOTS: usize = ndp::TCP_SESSIONS_MAX as usize;

/// 槽位：Free / Listen 口（非会话）/ 被动打开三阶段（Pending 握手中 →
/// Ready 就绪待 accept → Data 已被客户端认领）。Sess 为 Copy——槽位存值，
/// 喂状态机走「拷出副本改写后整写回」两段式（`match &mut` 内对同槽赋值
/// 违约借用）。
#[derive(Clone, Copy)]
enum Slot {
    Free,
    Listen { port: u16 },
    Pending(Sess),
    Ready(Sess),
    Data(Sess),
}

/// 单收发线程会话多路复用表：Listen 口 + 被动/主动会话并存，每帧四元组
/// 反查（`route_seg`）。
pub(crate) struct TcpMux {
    slots: [Slot; SLOTS],
}

/// 表单例（首刀几何注：Sess rx 512→2048 后 8 槽表 ≈ 17KB；连带 IOB 扩容，
/// .bss 实测增量 +9.5KB——boot 冒烟面，128M 帧边敏感见 NET-01 环境战记。
/// 二刀追加 Sess.tx 发送暂存 250B/槽 ≈ +2KB）。
static mut TCPTABLE: TcpMux = TcpMux { slots: [Slot::Free; SLOTS] };

/// 全局表单例（vnet 单线程：自验与 serve() 串行，无并发面——SAFETY：无
/// 别名窗口，每个函数体经此入口取唯一 &mut；`addr_of_mut!` 绕开
/// static_mut_refs 拒绝）。
pub(crate) fn mux() -> &'static mut TcpMux {
    unsafe { &mut *core::ptr::addr_of_mut!(TCPTABLE) }
}

/// LISTEN 失败形（serve() 翻译：Dup→ST_BAD / Full→ST_FULL）。
#[derive(Debug)]
pub(crate) enum ListenErr {
    /// 同口重听。
    Dup,
    /// 表满（无 Free 槽）。
    Full,
}

/// ACCEPT 失败形（serve() 翻译：NoSession→ST_NOSESSION / Again→ST_EAGAIN）。
#[derive(Debug)]
pub(crate) enum AcceptErr {
    /// lsid 非 Listen 槽（越界/已撤/非口）。
    NoSession,
    /// 口在但无就绪连接。
    Again,
}

/// 收帧 → GW TCP 段视图（eth→ipv4（src==GW_IP/dst==OUR_IP/proto TCP）→
/// parse_tcp）。**payload 先拷 pbuf**——返回的 TcpSeg 借 pbuf 不借 frame
/// （调用方随后的 act 写 IOB 不影响段视图，「先拷后应答」的 parse 版）；
/// payload 超 RX_SEG_MAX（=1460 满段域）或 pbuf 容量拒收不入状态机
/// （截断会腐化序号记账——宁拒勿断；拒收线锚**实际链路几何**（MTU 域）
/// 而非对端通告——NET-01 实证 SLIRP 不读我方 MSS 通告。超限帧对端重传
/// 同形仍拒 = 死锁面，由以太 MTU 1514 上限结构性兜底；IP/TCP 选项使
/// 帧超 RX_BUF_SZ 时第一道闸（recv_frame len 检查）先行拒收，同不死锁
/// 交付语义）。返回 (源口, 目的口, 段)。
fn parse_gw_tcp<'a>(frame: &[u8], pbuf: &'a mut [u8]) -> Option<(u16, u16, bt::TcpSeg<'a>)> {
    let e = np::parse_eth(frame)?;
    if e.ethertype != np::ET_IPV4 {
        return None;
    }
    let ip = np::parse_ipv4(e.payload)?;
    if ip.src != GW_IP || ip.dst != OUR_IP || ip.proto != np::IP_PROTO_TCP {
        return None;
    }
    let t = np::parse_tcp(ip.payload, ip.src, ip.dst)?;
    let n = t.payload.len();
    if n > RX_SEG_MAX || n > pbuf.len() {
        return None; // 超长段拒收
    }
    pbuf[..n].copy_from_slice(t.payload);
    Some((
        t.sport,
        t.dport,
        bt::TcpSeg {
            seq: t.seq,
            ack: t.ack,
            flags: bt::SegFlags {
                syn: t.flags.syn,
                ack: t.flags.ack,
                fin: t.flags.fin,
                rst: t.flags.rst,
            },
            window: t.window,
            mss: t.mss,
            payload: &pbuf[..n],
        },
    ))
}

/// 非本 Sess（session-0）段的表路由：①四元组匹配既有会话（Pending/Ready/
/// Data——sport=本地口/dport=远口）→ on_seg + rx 面交付 + act + 收账，
/// Pending 收到握手第三步 ACK（→Established）原地转 Ready；②无匹配 SYN（非
/// RST）× Listen 口 → accept 帽（ACCEPT_Q_MAX）未满且有 Free 槽 →
/// passive_open + SynAck（超帽/表满/口无 → 丢——TCP 不应答即丢，client 重传
/// 兜底）；③其余（孤儿 ACK/RST/杂散）丢弃。
#[allow(clippy::too_many_arguments)]
fn route_seg(
    d: &mut Dma,
    regs: *mut u32,
    mac: &[u8; 6],
    gw_mac: &[u8; 6],
    table: &mut TcpMux,
    sport: u16,
    dport: u16,
    seg: &bt::TcpSeg<'_>,
) {
    for i in 0..SLOTS {
        // 第一段（不可变探测）：四元组命中既有会话槽（0/1/2 = 种类标记）。
        let kind = match &table.slots[i] {
            Slot::Pending(s) if s.sport == dport && s.dport == sport => 0,
            Slot::Ready(s) if s.sport == dport && s.dport == sport => 1,
            Slot::Data(s) if s.sport == dport && s.dport == sport => 2,
            _ => continue,
        };
        // 第二段（Copy 拷出改写）：副本喂状态机后整写回。
        let mut s = match table.slots[i] {
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => s,
            _ => continue, // 探测已保证会话槽（防御面）
        };
        // 准入预检（宁拒不腐化，首刀核心正确性件）：会话收缓冲放不下
        // 整段 → 不喂状态机、回当前 ACK（重复 ACK）靠对端重传——rcv_nxt
        // 不推进，DRAIN 腾空后重传段到达；截断交付 = rcv_nxt 已推进 +
        // 字节丢 = ACK 谎报的静默腐化面（NET-01 实证 SLIRP 不理我方窗口
        // 通告，超通告段是常态入站而非违约——原 rx_push 截断策略的防御
        // 假设已证伪）。空段恒过检（纯 ACK/FIN 无收缓冲面）。
        if s.rx_len + seg.payload.len() > s.rx.len() {
            println!(
                "vnet: tcp 准入拒段 sid={} rx_len={} seg={} cap={}",
                i as u32 + 1,
                s.rx_len,
                seg.payload.len(),
                s.rx.len()
            );
            s.act(d, regs, mac, gw_mac, bt::Action::Ack, &[]);
            // 状态机未动（Ack 无挂臂），槽原样回写（arm/tw 记账无变化）。
            table.slots[i] = match kind {
                0 => Slot::Pending(s),
                1 => Slot::Ready(s),
                _ => Slot::Data(s),
            };
            return; // 端口分配唯一 → 四元组命中即唯一
        }
        let out = s.conn.on_seg(seg);
        if let Some(del) = out.deliver {
            s.rx_push(del);
        }
        s.act(d, regs, mac, gw_mac, out.action, &[]);
        s.post_account();
        // SynRcvd→Established（握手第三步 ACK）：Pending → Ready 待 accept；
        // 其余种类不变（Pending/Ready 死会话由 accept 弃置扫收，Data 死会话
        // 留位供 poll CLOSED 旗）。
        let kind = if kind == 0 && s.conn.state == bt::State::Established { 1 } else { kind };
        table.slots[i] = match kind {
            0 => Slot::Pending(s),
            1 => Slot::Ready(s),
            _ => Slot::Data(s),
        };
        return; // 端口分配唯一 → 四元组命中即唯一
    }
    // ② 无匹配 SYN × Listen 口 → 被动打开。
    if seg.flags.syn && !seg.flags.rst {
        let lport = (0..SLOTS).find_map(|li| match table.slots[li] {
            Slot::Listen { port } if port == dport => Some(port),
            _ => None,
        });
        if let Some(port) = lport {
            if table.ready_on(port) < bt::ACCEPT_Q_MAX {
                if let Some(fi) = (0..SLOTS).find(|&j| matches!(table.slots[j], Slot::Free)) {
                    let iss = now() as u32;
                    if let Some((conn, out)) = bt::Conn::passive_open(iss, seg) {
                        let mut s = Sess::from_passive(conn, port, sport);
                        s.act(d, regs, mac, gw_mac, out.action, &[]); // SYN-ACK + RTO 挂臂
                        s.post_account();
                        table.slots[fi] = Slot::Pending(s);
                    }
                }
            }
        }
    }
    // ③ 其余丢弃（TCP 丢段语义）。
}

/// 表定时泵：armed 会话 RTO 到 → on_rto 重发队头（act 内 arm_extend 保尾
/// 重挂、载荷切 Sess.tx 暂存；无重发面清臂——与 step RTO 分支同序）；
/// TIME_WAIT 到 → expire。探测/改写两段式同 route_seg。
fn pump_timers(d: &mut Dma, regs: *mut u32, mac: &[u8; 6], gw_mac: &[u8; 6], table: &mut TcpMux) {
    let t = now();
    for i in 0..SLOTS {
        // 第一段（不可变探测）：arm / tw 到期与否。
        let (arm_due, tw_due) = match &table.slots[i] {
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => (
                matches!(&s.arm, Some(a) if t >= a.deadline),
                matches!(&s.tw, Some(x) if t >= *x),
            ),
            _ => continue,
        };
        if !arm_due && !tw_due {
            continue;
        }
        // 第二段（Copy 拷出改写）。
        let mut s = match table.slots[i] {
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => s,
            _ => continue,
        };
        if arm_due {
            // RTO 到：on_rto 重发队头（act 内 arm_extend 保尾重挂；重发载荷
            // 切 Sess.tx 暂存）；无重发面（环空/到帽）清臂。
            let ev = s.conn.on_rto();
            if matches!(ev.action, bt::Action::None) {
                s.arm = None;
            } else {
                s.act(d, regs, mac, gw_mac, ev.action, &[]);
            }
        }
        if tw_due {
            s.conn.expire_timewait();
            s.tw = None;
        }
        s.post_account();
        table.slots[i] = match table.slots[i] {
            Slot::Pending(_) => Slot::Pending(s),
            Slot::Ready(_) => Slot::Ready(s),
            _ => Slot::Data(s),
        };
    }
}

/// 单发泵：收一帧（step_ns=0 → recv_frame(now()) 非阻塞单 poll 不睡；
/// >0 → now()+step_ns 睡步进）路由 + 表定时收账。serve() EAGAIN 空闲窗
/// （非阻塞 + wait_one 让出 CPU）与 drive_until（睡步进）共用。
pub(crate) fn pump_once(
    d: &mut Dma,
    regs: *mut u32,
    mac: &[u8; 6],
    gw_mac: &[u8; 6],
    table: &mut TcpMux,
    step_ns: u64,
) {
    let dl = if step_ns == 0 { now() } else { now() + step_ns };
    let frame = match recv_frame(d, regs, dl) {
        Some(f) => f,
        None => {
            pump_timers(d, regs, mac, gw_mac, table);
            return;
        }
    };
    let mut pbuf = [0u8; RX_SEG_MAX];
    if let Some((sport, dport, seg)) = parse_gw_tcp(frame, &mut pbuf) {
        route_seg(d, regs, mac, gw_mac, table, sport, dport, &seg);
    }
    pump_timers(d, regs, mac, gw_mac, table);
}

/// 谓词驱动泵：直到谓词真或帽到（表会话收口全靠它——含 TIME_WAIT 泵）。
/// 返回谓词是否达成。A2-3 起 serve 命名 RT 臂直用（ACK 等待谓词）。
pub(crate) fn drive_until(
    d: &mut Dma,
    regs: *mut u32,
    mac: &[u8; 6],
    gw_mac: &[u8; 6],
    table: &mut TcpMux,
    pred: impl Fn(&TcpMux) -> bool,
    cap: u64,
) -> bool {
    while !pred(table) {
        if now() >= cap {
            return false;
        }
        pump_once(d, regs, mac, gw_mac, table, POLL_STEP_NS);
    }
    true
}

impl TcpMux {
    /// sid → 槽索引（sid = 槽号+1，域 1..=SLOTS；0/越界 None）。
    fn idx_of(sid: u32) -> Option<usize> {
        if sid == 0 || sid > SLOTS as u32 {
            None
        } else {
            Some(sid as usize - 1)
        }
    }

    /// 清场（selftest / serve() 起点确定性）。
    pub(crate) fn reset(&mut self) {
        self.slots = [Slot::Free; SLOTS];
    }

    /// LISTEN：占一 Free 槽记口；同口重听 Dup、无空槽 Full。
    pub(crate) fn listen(&mut self, port: u16) -> Result<u32, ListenErr> {
        let mut free: Option<usize> = None;
        for i in 0..SLOTS {
            match self.slots[i] {
                Slot::Listen { port: p } if p == port => return Err(ListenErr::Dup),
                Slot::Free if free.is_none() => free = Some(i),
                _ => {}
            }
        }
        let i = free.ok_or(ListenErr::Full)?;
        self.slots[i] = Slot::Listen { port };
        Ok(i as u32 + 1)
    }

    /// lsid → 监听口（非 Listen 槽 None）。
    fn listen_port_of(&self, lsid: u32) -> Option<u16> {
        match self.slots.get(Self::idx_of(lsid)?)? {
            Slot::Listen { port } => Some(*port),
            _ => None,
        }
    }

    /// 口上待 accept 的就绪数（Ready 且未死——Closed 弃置不计（accept 幻
    /// 影防线）、Pending 握手中不计）。
    fn ready_on(&self, port: u16) -> usize {
        self.slots
            .iter()
            .filter(|s| {
                matches!(s, Slot::Ready(v) if v.sport == port && v.conn.state != bt::State::Closed)
            })
            .count()
    }

    /// ACCEPT：先回收该口 Closed 弃置（Pending/Ready 死会话——无人认领，
    /// 留位只挤占容量）→ Free，再取首条活 Ready → Data（**槽位不变 → sid
    /// 不变**——accept 只改种类）。无活就绪 Again；lsid 非 Listen 槽
    /// NoSession。
    pub(crate) fn accept(&mut self, lsid: u32) -> Result<u32, AcceptErr> {
        let port = self.listen_port_of(lsid).ok_or(AcceptErr::NoSession)?;
        for i in 0..SLOTS {
            let dead = match &self.slots[i] {
                Slot::Pending(s) | Slot::Ready(s) => {
                    s.sport == port && s.conn.state == bt::State::Closed
                }
                _ => false,
            };
            if dead {
                self.slots[i] = Slot::Free;
            }
        }
        for i in 0..SLOTS {
            let hit = match &self.slots[i] {
                Slot::Ready(s) => s.sport == port && s.conn.state != bt::State::Closed,
                _ => false,
            };
            if hit {
                let s = match self.slots[i] {
                    Slot::Ready(s) => s,
                    _ => unreachable!("探测已保证 Ready"),
                };
                self.slots[i] = Slot::Data(s);
                return Ok(i as u32 + 1);
            }
        }
        Err(AcceptErr::Again)
    }

    /// POLL：Listen 槽 → (0, ready_on)（可接数面）；会话槽 → flags
    /// （WRITABLE=Established / CLOSED=Closed）+ rbytes=rx_len；Free/越界
    /// None（serve() 翻 ST_NOSESSION）。
    pub(crate) fn poll_of(&self, sid: u32) -> Option<(u8, u16)> {
        let slot = self.slots.get(Self::idx_of(sid)?)?;
        match slot {
            Slot::Listen { port } => Some((0, self.ready_on(*port) as u16)),
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => {
                let mut flags = 0u8;
                if s.conn.state == bt::State::Established {
                    flags |= ndp::POLL_FLAG_WRITABLE;
                }
                if s.conn.state == bt::State::Closed {
                    flags |= ndp::POLL_FLAG_CLOSED;
                }
                Some((flags, s.rx_len as u16))
            }
            Slot::Free => None,
        }
    }

    /// 会话槽状态视图（自验谓词面）。
    fn state_of(&self, sid: u32) -> Option<bt::State> {
        match self.slots.get(Self::idx_of(sid)?)? {
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => Some(s.conn.state),
            _ => None,
        }
    }

    /// 会话槽 rx 驻留字节数（自验谓词面）。
    fn rx_len_of(&self, sid: u32) -> Option<usize> {
        match self.slots.get(Self::idx_of(sid)?)? {
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => Some(s.rx_len),
            _ => None,
        }
    }

    /// 会话槽 rx 拷出（自验对拍面；返回拷出字节数）。
    fn rx_copy(&self, sid: u32, out: &mut [u8]) -> Option<usize> {
        let s = match self.slots.get(Self::idx_of(sid)?)? {
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => s,
            _ => return None,
        };
        let n = s.rx_len.min(out.len());
        out[..n].copy_from_slice(&s.rx[..n]);
        Some(n)
    }

    /// 会话槽 rx 收割（A2-3 DRAIN 数据面）：拷出 + `copy_within` 压实消费
    ///（memmove 语义，重叠安全——P6 A-7 组帧坑的正面形态）。返回收割字节数
    ///（可为 0——「POLL 报过 rbytes 后串行收割必非零」由调用方节奏保证，
    /// 协议面诚实报 0 而非错）。Free/Listen/越界 None（serve() 翻 ST_BAD
    /// ——Form::Tcp 域内无 NOSESSION 位，诊断位留 Form::Poll）。
    pub(crate) fn rx_drain(&mut self, sid: u32, out: &mut [u8]) -> Option<usize> {
        let s = match self.slots.get_mut(Self::idx_of(sid)?)? {
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => s,
            _ => return None,
        };
        let take = s.rx_len.min(out.len());
        out[..take].copy_from_slice(&s.rx[..take]);
        s.rx.copy_within(take..s.rx_len, 0);
        s.rx_len -= take;
        Some(take)
    }

    /// 会话槽无挂臂谓词（send 的 ACK 已到——自验同步点；A2-3 起 serve
    /// 命名 RT 臂的收口谓词：Closed 态 arm 亦空 = RST-in-wait 可接受裁定）。
    pub(crate) fn arm_idle(&self, sid: u32) -> Option<bool> {
        match self.slots.get(Self::idx_of(sid)?)? {
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => Some(s.arm.is_none()),
            _ => None,
        }
    }

    /// 表会话上发数据（二刀多段化）：data 按 MSS 分批入环（环/窗阻塞时
    /// pump_once 泵等位，帽 = SEND_CAP_NS 内）。仅 Ready/Data 槽（Pending
    /// 握手中 / Closed 不可发）→ false；上一 op 未收口（在途环占用）→ false
    ///（老 in-flight 占位契约保持，serve 翻 ST_BAD）。拷出改写两段式同
    /// route_seg——阻塞泵后表内状态已变，逐分片重取副本。A2-3 起 serve
    /// 命名 RT 臂直用（echo 回发面，返回后由 serve RT 臂 drive_until
    /// arm_idle 收全 ACK）。
    pub(crate) fn send_data(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        sid: u32,
        data: &[u8],
    ) -> bool {
        let i = match Self::idx_of(sid) {
            Some(i) => i,
            None => return false,
        };
        let ready_kind = match &self.slots[i] {
            Slot::Ready(_) => 1,
            Slot::Data(_) => 2,
            _ => return false,
        };
        let s = match self.slots[i] {
            Slot::Ready(s) | Slot::Data(s) => s,
            _ => return false,
        };
        if s.conn.tx_inflight() != 0 {
            // 上一 op 在途未收口（TIMEOUT 残留）——拒面 loud 诊断（排障期；
            // 绿路零输出）。
            println!(
                "vnet: rt bad sid={} len={} state={:?} infl={} rx={} una=0x{:x} nxt=0x{:x}",
                sid,
                data.len(),
                s.conn.state,
                s.conn.inflight_kind(),
                s.rx_len,
                s.conn.snd_una,
                s.conn.snd_nxt
            );
            self.slots[i] = if ready_kind == 1 { Slot::Ready(s) } else { Slot::Data(s) };
            return false;
        }
        let mut off = 0usize;
        let send_cap = now() + SEND_CAP_NS;
        while off < data.len() {
            // 泵后表内状态已变：逐分片重取副本（send/act 改写即写回）。
            let mut s = match self.slots[i] {
                Slot::Ready(s) | Slot::Data(s) => s,
                _ => return false, // 防御面（泵不换槽种类）
            };
            let end = (off + bt::MSS as usize).min(data.len());
            let ev = s.conn.send((end - off) as u32);
            if let bt::Action::Data { .. } = ev.action {
                s.act(d, regs, mac, gw_mac, ev.action, &data[off..end]);
                s.post_account();
                self.slots[i] = if ready_kind == 1 { Slot::Ready(s) } else { Slot::Data(s) };
                off = end;
                continue;
            }
            if s.conn.state != bt::State::Established {
                // 拒面 loud 诊断（排障期；绿路零输出）：state≠Established /
                // len 越域（0 或 >MSS）两面一次可辨。
                println!(
                    "vnet: rt bad sid={} len={} state={:?} infl={} rx={} una=0x{:x} nxt=0x{:x}",
                    sid,
                    data.len(),
                    s.conn.state,
                    s.conn.inflight_kind(),
                    s.rx_len,
                    s.conn.snd_una,
                    s.conn.snd_nxt
                );
                self.slots[i] = if ready_kind == 1 { Slot::Ready(s) } else { Slot::Data(s) };
                return false;
            }
            // 环满/越窗：泵一步等位（ACK 弹槽/窗更新解锁；帽内）。
            if now() >= send_cap {
                println!("vnet: rt send 等位帽到 sid={} off={}", sid, off);
                return false;
            }
            pump_once(d, regs, mac, gw_mac, self, POLL_STEP_NS);
        }
        true
    }

    /// 表会话主动关闭第一段：close() → act（FIN 段；Established→FinWait1 /
    /// CloseWait→LastAck，其余态 Action::None 同受理 true——挥手剩余帧由
    /// 泵收口）。仅非会话槽（Free/Listen/越界）拒 false。
    fn close_init(&mut self, d: &mut Dma, regs: *mut u32, mac: &[u8; 6], gw_mac: &[u8; 6], sid: u32) -> bool {
        let i = match Self::idx_of(sid) {
            Some(i) => i,
            None => return false,
        };
        let kind = match &self.slots[i] {
            Slot::Pending(_) => 0,
            Slot::Ready(_) => 1,
            Slot::Data(_) => 2,
            _ => return false,
        };
        let mut s = match self.slots[i] {
            Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => s,
            _ => return false,
        };
        let ev = s.conn.close();
        if !matches!(ev.action, bt::Action::None) {
            s.act(d, regs, mac, gw_mac, ev.action, &[]);
        }
        s.post_account();
        self.slots[i] = match kind {
            0 => Slot::Pending(s),
            1 => Slot::Ready(s),
            _ => Slot::Data(s),
        };
        true
    }

    /// 服务面 CLOSE：Listen → 纯撤表 Some(true)；会话 → close_init +
    /// drive_until 本槽 Closed（TIME_WAIT 由泵收口）后**无论成败释槽**
    /// （v1 无重试语义面，槽容量优先——帽到 Some(false) 但槽已释）；Free/
    /// 越界 None（Form::Tcp 域内翻 ST_BAD——诊断位留 Form::Poll）。
    /// A2-3 serve 命名 CLOSE 臂已接线（dead_code 冻结期收束）。
    pub(crate) fn close_sid(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        sid: u32,
        deadline: u64,
    ) -> Option<bool> {
        let i = Self::idx_of(sid)?;
        let is_listen = matches!(self.slots[i], Slot::Listen { .. });
        if is_listen {
            self.slots[i] = Slot::Free;
            return Some(true);
        }
        if matches!(self.slots[i], Slot::Free) {
            return None;
        }
        // 会话：先试主动关闭（可关态发 FIN，已 Closed 则直过），泵到本槽
        // Closed 或帽，然后释槽。
        {
            let mut s = match self.slots[i] {
                Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s) => s,
                _ => return None, // 探测已排除（防御面）
            };
            let ev = s.conn.close();
            if !matches!(ev.action, bt::Action::None) {
                s.act(d, regs, mac, gw_mac, ev.action, &[]);
            }
            s.post_account();
            self.slots[i] = match self.slots[i] {
                Slot::Pending(_) => Slot::Pending(s),
                Slot::Ready(_) => Slot::Ready(s),
                _ => Slot::Data(s),
            };
        }
        let ok = drive_until(
            d, regs, mac, gw_mac, self,
            |t| {
                matches!(&t.slots[i],
                    Slot::Pending(s) | Slot::Ready(s) | Slot::Data(s)
                        if s.conn.state == bt::State::Closed)
            },
            deadline,
        );
        self.slots[i] = Slot::Free; // 无论成败释槽
        Some(ok)
    }

    /// 表上主动打开（自验客户端面 / serve 不用——client 走 legacy session-0
    /// 或 A2-3 事件循环）：Conn::open + SYN（act 含 RTO 挂臂）→ Data 槽；
    /// 无空槽 None。
    fn open_active(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        sport: u16,
        dport: u16,
    ) -> Option<u32> {
        let i = (0..SLOTS).find(|&j| matches!(self.slots[j], Slot::Free))?;
        let iss = now() as u32;
        let (conn, out) = bt::Conn::open(iss);
        let mut s = Sess::from_active(conn, sport, dport);
        s.act(d, regs, mac, gw_mac, out.action, &[]); // SYN + RTO 挂臂
        s.post_account();
        self.slots[i] = Slot::Data(s);
        Some(i as u32 + 1)
    }

    /// 表最早定时（RTO 挂臂 / TIME_WAIT）——step() dl min 与 serve() 空闲
    /// 步进共用；空表 None（dl 不受影响——legacy 零漂移按构造）。
    fn earliest_timer(&self) -> Option<u64> {
        self.slots
            .iter()
            .filter_map(|s| match s {
                Slot::Pending(v) | Slot::Ready(v) | Slot::Data(v) => {
                    v.arm.map(|a| a.deadline).or(v.tw)
                }
                _ => None,
            })
            .min()
    }
}

/// 入站自验监听口 = hostfwd 规则 `tcp::18081-:18081` 定值对齐（guest 18081
/// 入站经 SLIRP 转回）；与 18080 echod / 48080 自验出站源口 / 48081 服务面
/// （serve() TCP_SPORT_NEXT）全错开。
const LSN_PORT: u16 = 18081;
/// 入站自验客户端源口（48080/48081 错开——hostfwd 回程 PORT_X 无四元组
/// 交叉，边界例亦验证过）。
const LSN_SPORT: u16 = 48082;
/// 入站自验各相位帽（ns）——同 SELFTEST_CAP_NS 口径。
const LSN_CAP_NS: u64 = 60_000_000_000;

/// 入站自验（A2-2 段③，rootfs 相 `LSN ok n=7`）：hostfwd 环回自连自验——
/// guest 主动 connect 10.0.2.2:18081 → SLIRP host-alias 代理宿主 loopback:
/// 18081 → hostfwd listener 接受转回 guest:18081（echod 同机制方向反转）。
/// **单线程互锁的结构性解**：全部会话（含客户端）进表，由 drive_until 泵
/// 统一驱动（收帧路由 + RTO/TIME_WAIT 定时）。七步：⓪listen+Dup 拒
/// ①open_active→双 Established+ready_on ②accept+双 poll ③client 64B 发
/// →dsid rx 对拍 ④dsid 回发→csid rx 对拍 ⑤双侧 close→双 Closed（TIME_WAIT
/// 泵收口）⑥accept Again（无幻影）。失败各相位 println 诊断 + trap loud
/// （标记缺失 = smoke FAIL 兜底）。插在 `tcp::selftest` 之后、serve() 之前。
pub(crate) fn listen_selftest(d: &mut Dma, regs: *mut u32, mac: &[u8; 6], gw_mac: &[u8; 6]) {
    let table = mux();
    table.reset();
    let cap = now() + LSN_CAP_NS;

    // ⓪ LISTEN + 同口重听 Dup 拒。
    let lsid = match table.listen(LSN_PORT) {
        Ok(sid) => sid,
        Err(e) => {
            println!("vnet: lsn ⓪ listen 失败 {:?}", e);
            trap();
        }
    };
    if !matches!(table.listen(LSN_PORT), Err(ListenErr::Dup)) {
        println!("vnet: lsn ⓪ 同口重听未拒");
        trap();
    }

    // ① 客户端主动打开（hostfwd 环回）→ 泵到双 Established + 口上就绪 1。
    let csid = match table.open_active(d, regs, mac, gw_mac, LSN_SPORT, LSN_PORT) {
        Some(sid) => sid,
        None => {
            println!("vnet: lsn ① 表满（open_active）");
            trap();
        }
    };
    let ci = TcpMux::idx_of(csid).unwrap();
    if !drive_until(
        d, regs, mac, gw_mac, table,
        |t| {
            matches!(&t.slots[ci], Slot::Data(s) if s.conn.state == bt::State::Established)
                && t.ready_on(LSN_PORT) == 1
        },
        cap,
    ) {
        println!("vnet: lsn ① 握手未达双 Established");
        trap();
    }

    // ② accept（Ready→Data，sid 不变）+ 双 poll 面：listen 口 (0,0)（就绪
    // 数已清）、dsid WRITABLE。
    let dsid = match table.accept(lsid) {
        Ok(sid) => sid,
        Err(e) => {
            println!("vnet: lsn ② accept 失败 {:?}", e);
            trap();
        }
    };
    match table.poll_of(lsid) {
        Some((0, 0)) => {}
        other => {
            println!("vnet: lsn ② listen poll 异常 {:?}", other);
            trap();
        }
    }
    match table.poll_of(dsid) {
        Some((f, 0)) if f & ndp::POLL_FLAG_WRITABLE != 0 => {}
        other => {
            println!("vnet: lsn ② dsid poll 异常 {:?}", other);
            trap();
        }
    }

    // ③ 客户端 64B pattern → 泵到 dsid 收齐 + csid 数据段 ACK 到 → 对拍。
    if !table.send_data(d, regs, mac, gw_mac, csid, PATTERN) {
        println!("vnet: lsn ③ 客户端 send 拒");
        trap();
    }
    if !drive_until(
        d, regs, mac, gw_mac, table,
        |t| t.rx_len_of(dsid) == Some(PATTERN.len()) && t.arm_idle(csid) == Some(true),
        cap,
    ) {
        println!("vnet: lsn ③ 送达泵超时");
        trap();
    }
    let mut rxb = [0u8; PATTERN.len()];
    match table.rx_copy(dsid, &mut rxb) {
        Some(n) if n == PATTERN.len() && rxb == *PATTERN => {}
        _ => {
            println!("vnet: lsn ③ 服务端对拍失败");
            trap();
        }
    }

    // ④ 服务端回发同 pattern → 泵到 csid 收齐 + dsid ACK 到 → 对拍。
    if !table.send_data(d, regs, mac, gw_mac, dsid, PATTERN) {
        println!("vnet: lsn ④ 服务端 send 拒");
        trap();
    }
    if !drive_until(
        d, regs, mac, gw_mac, table,
        |t| t.rx_len_of(csid) == Some(PATTERN.len()) && t.arm_idle(dsid) == Some(true),
        cap,
    ) {
        println!("vnet: lsn ④ 回发泵超时");
        trap();
    }
    let mut rxb2 = [0u8; PATTERN.len()];
    match table.rx_copy(csid, &mut rxb2) {
        Some(n) if n == PATTERN.len() && rxb2 == *PATTERN => {}
        _ => {
            println!("vnet: lsn ④ 客户端对拍失败");
            trap();
        }
    }

    // ⑤ 双侧主动关闭：client FIN → dsid CloseWait → dsid FIN → 双 Closed
    //（client 侧 TIME_WAIT 2s 由泵收口）。
    if !table.close_init(d, regs, mac, gw_mac, csid) {
        println!("vnet: lsn ⑤ client close 拒");
        trap();
    }
    if !drive_until(
        d, regs, mac, gw_mac, table,
        |t| t.state_of(dsid) == Some(bt::State::CloseWait),
        cap,
    ) {
        println!("vnet: lsn ⑤ dsid 未及 CloseWait");
        trap();
    }
    if !table.close_init(d, regs, mac, gw_mac, dsid) {
        println!("vnet: lsn ⑤ dsid close 拒");
        trap();
    }
    if !drive_until(
        d, regs, mac, gw_mac, table,
        |t| {
            t.state_of(csid) == Some(bt::State::Closed) && t.state_of(dsid) == Some(bt::State::Closed)
        },
        cap,
    ) {
        println!("vnet: lsn ⑤ 挥手未收口");
        trap();
    }

    // ⑥ 口上无幻影待接（死会话已由 accept 弃置扫回收）+ 清场。
    match table.accept(lsid) {
        Err(AcceptErr::Again) => {}
        other => {
            println!("vnet: lsn ⑥ accept 幻影 {:?}", other);
            trap();
        }
    }
    table.reset();
    println!("LSN ok n=7");
}

/// TCP 自验主循环（T-c，rootfs 相第 19 标记）：`Sess` 全生命周期走一遍——
/// connect → 64B pattern roundtrip 逐字节对拍 → shutdown → `TCP ok echo=64`。
/// 插在 `NET ok` 自验之后、serve() 之前（A-8b 同款：自验毕转常驻、DNS/TCP
/// 服务面零回归）。失败各相位 println 诊断 + trap loud（标记缺失 = smoke
/// FAIL 兜底）。
pub fn selftest(d: &mut Dma, regs: *mut u32, mac: &[u8; 6], gw_mac: &[u8; 6]) {
    let table = mux();
    table.reset();
    let mut sess = match Sess::connect(d, regs, mac, gw_mac, table, TCP_SPORT, ECHO_PORT, now() + SELFTEST_CAP_NS) {
        Some(s) => s,
        None => {
            println!("vnet: tcp 自验握手失败");
            trap();
        }
    };
    let mut out = [0u8; bt::MSS as usize];
    match sess.roundtrip(d, regs, mac, gw_mac, table, PATTERN, &mut out, now() + SELFTEST_CAP_NS) {
        RtRes::Ok(n) if n == PATTERN.len() && out[..n] == PATTERN[..] => {}
        RtRes::Ok(n) => {
            println!("vnet: tcp 回显对拍失败 got={}", n);
            trap();
        }
        RtRes::Timeout => {
            println!("vnet: tcp 自验超时（roundtrip）state={:?}", sess.conn.state);
            trap();
        }
        RtRes::Bad => {
            println!("vnet: tcp 会话异常终止（roundtrip）");
            trap();
        }
    }
    if !sess.shutdown(d, regs, mac, gw_mac, table, now() + SELFTEST_CAP_NS) {
        println!("vnet: tcp 挥手超时 state={:?}", sess.conn.state);
        trap();
    }
    println!("TCP ok echo=64");
}
