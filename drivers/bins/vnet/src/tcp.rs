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
//! （T-d）同一驱动。定时面：RTO 挂臂（Syn/Data/Fin 出现即挂段尾 +
//! `rto_ms(backoff)`；对账 = 每次 on_seg/on_rto 后 snd_una wrapping 覆盖段
//! 尾或 state==Closed 即清——LastAck 例外：FIN 的 ACK 走 on_seg_lastack
//! 不推进 snd_una，靠 Closed 兜）+ TIME_WAIT 2s（**每步按状态重挂**——
//! v1 环境窗内唯一合法到达帧 = 重复 FIN 本就应重挂，杂散/被四元组过滤跳过
//! 的段重挂无害且各相位帽兜底）。

use crate::{now, recv_frame, trap, tx_send, Dma, GW_IP, IOB, OUR_IP};
use berry_netproto as np;
use berry_tcp as bt;

/// echod 端口（宿主 127.0.0.1:18080，`tools/echod.py` 定值）——自验目标。
const ECHO_PORT: u16 = 18080;
/// 自验源端口（避开 DNS 5353；四元组过滤 + SLIRP 原路回发）。serve() 的
/// 服务会话从 48081 起发号（main.rs `TCP_SPORT_NEXT`），与自验口错开。
const TCP_SPORT: u16 = 48080;
/// 自验各相位帽（ns）：握手/往返/挥手单相位预算（§6 RPC 成本模型：单帧
/// poll ~1s 级 TCG；成功路径三相位合计 ≈10 帧往返）。
const SELFTEST_CAP_NS: u64 = 30_000_000_000;
/// 单帧组帧缓冲：TCP 头 20 + MSS 选项 4 + 段体 MSS=128 = 152B。
const SEG_BUF_SZ: usize = np::TCP_HDR_SZ + np::TCP_OPT_MSS_SZ + bt::MSS as usize;

// T-c 对抗审查 ③：MSS=128 的定值安全真约束原先只活在两份注释里——RX 侧
// 「对端按我方通告回的最大段」必须装进 RX_BUF_SZ（否则 main.rs recv_frame
// 的 len 检查拒收→自验超时）、TX 侧整帧必须装进 IOB。v2 抬 MSS 过线即编译
// 期炸响，A-4 SECTOR_SZ/BLK_SECTOR_SZ const assert 同族先例。
const _: () = assert!(
    berry_virtio::NET_HDR_SZ + np::ETH_HDR_SZ + np::IPV4_HDR_SZ + np::TCP_HDR_SZ
        + np::TCP_OPT_MSS_SZ + bt::MSS as usize
        <= crate::RX_BUF_SZ
);
const _: () = assert!(
    berry_virtio::NET_HDR_SZ + np::ETH_HDR_SZ + np::IPV4_HDR_SZ + SEG_BUF_SZ
        <= crate::IOB_SZ
);
// T-d 对抗审查 ②：TCP_DATA_MAX 与 bt::MSS 是弹道交汇的两份定值——
// netdproto parse 放行 data ≤ TCP_DATA_MAX，而 Sess::roundtrip 的
// conn.send 被 berry-tcp `len > MSS` 拒 → RtRes::Bad → 静默 ST_BAD 零
// 诊断。单侧抬 TCP_DATA_MAX 即编译期炸响（A-4 SECTOR_SZ const assert
// 同族）；MSS 单侧改动另有 berry-tcp consts_pinned 字面 128 钉板把守。
const _: () = assert!(berry_netdproto::TCP_DATA_MAX == bt::MSS as usize);

/// 回显载荷 64B（ICMP 32B probe 同族确定性 pattern）。
static PATTERN: &[u8; 64] = b"berryos-tcp-probe-0123456789012345678901234567890123456789abcdef";
const _: () = assert!(PATTERN.len() == 64);

/// RTO 挂臂：in-flight 段尾序号 + 到期时刻（wrapping 覆盖判定见下）。
struct Arm {
    end: u32,
    deadline: u64,
}

/// roundtrip 结果（serve() 翻译成 netdproto st）。
pub(crate) enum RtRes {
    /// 收齐等长回显且数据段 ACK 到（arm 清）——n = 回显字节数（out 前缀有效）。
    Ok(usize),
    /// 相位帽到（不销毁会话——in-flight 占位使后续 roundtrip 恒 Bad，
    /// 唯 close 可收口：FIN 覆盖单槽）。
    Timeout,
    /// 会话态不可用（非 Established / in-flight 占位 / 会话已被 RST 或重传
    /// 到帽终结）。
    Bad,
}

/// 一次 TCP 会话：状态机 + 四元组 + RTO/TIME_WAIT 挂臂。单会话独占
/// （草案 §8：v1 并发 TCP 无面）——serve() 持 `Option<Sess>`，OPEN 撞在席
/// 会话 ST_BUSY。
pub(crate) struct Sess {
    conn: bt::Conn,
    sport: u16,
    dport: u16,
    arm: Option<Arm>,
    tw: Option<u64>,
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
        let iob: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(IOB) };
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
    /// 主动打开：SYN → SYN-ACK → ACK 握手至 Established。整体帽到 / 重传
    /// 到帽 / RST → None（调用方按超时语义应答，会话不成立不留残）。
    pub(crate) fn connect(
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        sport: u16,
        dport: u16,
        deadline: u64,
    ) -> Option<Sess> {
        let iss = now() as u32;
        let (conn, out) = bt::Conn::open(iss);
        let mut sess = Sess { conn, sport, dport, arm: None, tw: None };
        sess.act(d, regs, mac, gw_mac, out.action, &[]);
        let mut scratch = [0u8; bt::MSS as usize]; // 握手无数据交付（SYN-ACK 空）
        loop {
            if sess.conn.state == bt::State::Established {
                return Some(sess);
            }
            if sess.conn.state == bt::State::Closed {
                return None; // RST / 重传到帽
            }
            match sess.step(d, regs, mac, gw_mac, deadline, &mut scratch) {
                Ok(_) => {}
                Err(()) => return None, // 整体帽
            }
        }
    }

    /// 会话上发一段收等长回显：send（in-flight=1，占用/空数据/非
    /// Established → Bad）→ 回显段交付拷入 out → 数据段 ACK 到（arm 清）
    /// 即 Ok；相位帽 Timeout；会话被 RST/重传到帽终结 Bad。
    pub(crate) fn roundtrip(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        data: &[u8],
        out: &mut [u8],
        deadline: u64,
    ) -> RtRes {
        let ev = self.conn.send(data.len() as u32);
        if !matches!(ev.action, bt::Action::Data { .. }) {
            return RtRes::Bad; // send 拒：非 Established / in-flight 占位 / 空数据
        }
        self.act(d, regs, mac, gw_mac, ev.action, data);
        let mut got = 0usize;
        let mut scratch = [0u8; bt::MSS as usize];
        loop {
            if got == data.len() && self.arm.is_none() {
                return RtRes::Ok(got); // 回显收齐 + 数据段 ACK 到
            }
            if self.conn.state == bt::State::Closed {
                return RtRes::Bad; // RST / 重传到帽
            }
            match self.step(d, regs, mac, gw_mac, deadline, &mut scratch) {
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
        deadline: u64,
    ) -> bool {
        if self.conn.state == bt::State::Closed {
            return true; // 幂等收口（前次 op 已终）
        }
        let ev = self.conn.close();
        self.act(d, regs, mac, gw_mac, ev.action, &[]);
        let mut scratch = [0u8; bt::MSS as usize];
        loop {
            if self.conn.state == bt::State::Closed {
                return true;
            }
            match self.step(d, regs, mac, gw_mac, deadline, &mut scratch) {
                Ok(_) => {}
                Err(()) => return false,
            }
        }
    }

    /// 单步推进：deadline = min(相位帽, RTO 挂臂, TIME_WAIT) 收帧——目标段
    /// （四元组过滤 + parse）→ on_seg → 交付拷 rcv（**先拷后应答**——
    /// deliver 借用 IOB 收帧视图，act 写 IOB 即失效，T-c 实踩）→ act（应答
    /// /重挂）→ 挂臂对账 + TIME_WAIT 按状态重挂；杂散帧（SLIRP IPv6 RA/
    /// 迟到旧帧）跳过重收。到点：RTO 挂臂到 → on_rto 重发重挂 / TIME_WAIT
    /// 到 → expire / 其余 = 相位帽 Err。返回交付字节数（0 = 本步无交付）。
    fn step(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
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
        let frame = match recv_frame(d, regs, dl) {
            Some(f) => f,
            None => {
                let t = now();
                if let Some(a) = &self.arm {
                    if t >= a.deadline {
                        let ev = self.conn.on_rto(); // 重发（或到帽放弃 → Closed）
                        self.arm = None;
                        self.act(d, regs, mac, gw_mac, ev.action, &[]);
                        return Ok(0);
                    }
                }
                if let Some(x) = self.tw {
                    if t >= x {
                        self.conn.expire_timewait();
                        self.tw = None;
                        return Ok(0);
                    }
                }
                return Err(()); // 相位帽
            }
        };
        // 目标段喂状态机（闭包统一 `?` 早退——非目标帧返 None 跳过重收）。
        let mut delivered = 0usize;
        (|| -> Option<()> {
            let e = np::parse_eth(frame)?;
            if e.ethertype != np::ET_IPV4 {
                return None;
            }
            let ip = np::parse_ipv4(e.payload)?;
            if ip.src != GW_IP || ip.dst != OUR_IP || ip.proto != np::IP_PROTO_TCP {
                return None;
            }
            let t = np::parse_tcp(ip.payload, ip.src, ip.dst)?;
            if t.sport != self.dport || t.dport != self.sport {
                return None;
            }
            let seg = bt::TcpSeg {
                seq: t.seq,
                ack: t.ack,
                flags: bt::SegFlags { syn: t.flags.syn, ack: t.flags.ack, fin: t.flags.fin, rst: t.flags.rst },
                window: t.window,
                mss: t.mss,
                payload: t.payload,
            };
            let out = self.conn.on_seg(&seg);
            // 交付先拷后应答（deliver 借用收帧视图，act 写 IOB 即失效）。
            if let Some(del) = out.deliver {
                let n = del.len().min(rcv.len());
                rcv[..n].copy_from_slice(&del[..n]);
                delivered = n;
            }
            self.act(d, regs, mac, gw_mac, out.action, &[]);
            Some(())
        })();
        // 挂臂对账 + TIME_WAIT 按状态重挂（模块头注）。
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
        Ok(delivered)
    }

    /// Action → wire 段统一翻译 + RTO 挂臂（Syn/Data/Fin 出现即挂段尾 +
    /// `rto_ms(backoff)`；on_rto 重发同经此处重挂——backoff 已 +1 用新值；
    /// Ack 无需重传不挂）。Data 内容由调用方随动作传入（send 按请求数发段，
    /// 长度失配 = 调用方契约违约，trap loud）。
    fn act(
        &mut self,
        d: &mut Dma,
        regs: *mut u32,
        mac: &[u8; 6],
        gw_mac: &[u8; 6],
        action: bt::Action,
        data: &[u8],
    ) {
        let arm_at =
            |end: u32| Some(Arm { end, deadline: now() + bt::rto_ms(self.conn.rto_backoff) * 1_000_000 });
        match action {
            bt::Action::None => {}
            bt::Action::Syn { seq } => {
                send_seg(
                    d, regs, mac, gw_mac, self.sport, self.dport, seq, 0,
                    np::TcpFlags { syn: true, ..Default::default() }, Some(bt::MSS), &[],
                );
                self.arm = arm_at(seq.wrapping_add(1));
            }
            bt::Action::Data { seq, len } => {
                if data.len() != len as usize {
                    println!("vnet: tcp data 段失配 len={} data={}", len, data.len());
                    trap();
                }
                send_seg(
                    d, regs, mac, gw_mac, self.sport, self.dport, seq, self.conn.rcv_nxt,
                    np::TcpFlags { ack: true, ..Default::default() }, None, data,
                );
                self.arm = arm_at(seq.wrapping_add(len));
            }
            bt::Action::Fin { seq } => {
                send_seg(
                    d, regs, mac, gw_mac, self.sport, self.dport, seq, self.conn.rcv_nxt,
                    np::TcpFlags { fin: true, ack: true, ..Default::default() }, None, &[],
                );
                self.arm = arm_at(seq.wrapping_add(1));
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

/// TCP 自验主循环（T-c，rootfs 相第 19 标记）：`Sess` 全生命周期走一遍——
/// connect → 64B pattern roundtrip 逐字节对拍 → shutdown → `TCP ok echo=64`。
/// 插在 `NET ok` 自验之后、serve() 之前（A-8b 同款：自验毕转常驻、DNS/TCP
/// 服务面零回归）。失败各相位 println 诊断 + trap loud（标记缺失 = smoke
/// FAIL 兜底）。
pub fn selftest(d: &mut Dma, regs: *mut u32, mac: &[u8; 6], gw_mac: &[u8; 6]) {
    let mut sess = match Sess::connect(d, regs, mac, gw_mac, TCP_SPORT, ECHO_PORT, now() + SELFTEST_CAP_NS) {
        Some(s) => s,
        None => {
            println!("vnet: tcp 自验握手失败");
            trap();
        }
    };
    let mut out = [0u8; bt::MSS as usize];
    match sess.roundtrip(d, regs, mac, gw_mac, PATTERN, &mut out, now() + SELFTEST_CAP_NS) {
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
    if !sess.shutdown(d, regs, mac, gw_mac, now() + SELFTEST_CAP_NS) {
        println!("vnet: tcp 挥手超时 state={:?}", sess.conn.state);
        trap();
    }
    println!("TCP ok echo=64");
}
