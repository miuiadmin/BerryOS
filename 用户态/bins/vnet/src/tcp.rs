//! vnet TCP 自验会话（T-c，`文档/TCP栈设计草案.md` §6）：guest 主动 connect
//! SLIRP 宿主回显服务——10.0.2.2 = host 本机（QEMU user 网标准，guest 主动出站
//! 由 SLIRP 转发宿主 127.0.0.1 同端口，**不用 hostfwd**——那是 host 连 guest 的
//! 反向面 v2）；宿主侧 = `工具链/echod.py`（smoke/justfile 起，被动关闭——client
//! 恰好验主动关闭全路径）。
//!
//! 分工（§3 分层裁决）：berry-tcp 状态机管状态与序号推进，本模块管 IO——
//! Action 翻译成 tx_send、入段经四元组过滤翻译成 TcpSeg、RTO/TIME_WAIT 定时
//! 由 now() 驱动（纯函数层不吃时钟）。组帧照 dns_roundtrip 家族：TCP 段先进
//! 独立栈缓冲，再 eth/IP 层层包裹进 IOB（独立缓冲无借用冲突、无自重叠搬移
//! ——A-7「组帧在位就不要再搬」纪律）。
//!
//! 定时面：RTO 挂臂（`Arm{end,deadline}`——run_action 统一挂，Syn/Data/Fin
//! 出现即挂段尾+`rto_ms(backoff)`；对账在每次 on_seg 后：snd_una wrapping 覆盖
//! 段尾或 state==Closed 即清——LastAck 例外：FIN 的 ACK 走 on_seg_lastack 不
//! 推进 snd_una，靠 Closed 兜）+ TIME_WAIT 2s（**循环尾按状态重挂**——处于
//! TimeWait 即重挂、不依赖 Ack 动作：v1 环境窗内唯一合法到达帧 = 重复 FIN 本
//! 就应重挂，杂散/被四元组过滤跳过的段重挂无害且 30s 整体帽兜底）。主循环
//! deadline = min(整体帽, arm, tw)，recv_frame 返 None 按到点者分派。
//!
//! 自验帽 30s（§6 RPC 成本模型：9-11 帧往返 × TCG 单帧 poll ~1s 级）；成功打
//! `TCP ok echo=64`（带参形态——`FS ok n=` 先例），失败 println 诊断 + trap
//! loud（标记缺失 = smoke FAIL）。

use crate::{now, recv_frame, trap, tx_send, Dma, GW_IP, IOB, OUR_IP};
use berry_netproto as np;
use berry_tcp as bt;

/// echod 端口（宿主 127.0.0.1:18080，`工具链/echod.py` 定值）。
const ECHO_PORT: u16 = 18080;
/// 本会话源端口（避开 DNS 5353；四元组过滤 + SLIRP 原路回发）。
const TCP_SPORT: u16 = 48080;
/// 整体帽（ns）：握手→数据→挥手全链 9-11 帧往返的 TCG 预算。
const TCP_CAP_NS: u64 = 30_000_000_000;
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

/// 回显载荷 64B（ICMP 32B probe 同族确定性 pattern）。
static PATTERN: &[u8; 64] = b"berryos-tcp-probe-0123456789012345678901234567890123456789abcdef";
const _: () = assert!(PATTERN.len() == 64);

/// RTO 挂臂：in-flight 段尾序号 + 到期时刻（wrapping 覆盖判定见模块头注）。
struct Arm {
    end: u32,
    deadline: u64,
}

/// 组一个 TCP 段并发送（段先进独立栈缓冲，再层层包裹进 IOB——dns_roundtrip
/// 家族模板；返回值 None 契约 = 参数面错，一律 trap loud）。
#[allow(clippy::too_many_arguments)]
fn send_seg(
    d: &mut Dma,
    regs: *mut u32,
    mac: &[u8; 6],
    gw_mac: &[u8; 6],
    seq: u32,
    ack: u32,
    flags: np::TcpFlags,
    mss: Option<u16>,
    payload: &[u8],
) {
    let mut seg = [0u8; SEG_BUF_SZ];
    let tl = match np::encode_tcp(
        &mut seg, OUR_IP, GW_IP, TCP_SPORT, ECHO_PORT, seq, ack, flags, bt::WINDOW, mss, payload,
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

/// Action → wire 段统一翻译 + RTO 挂臂（Syn/Data/Fin 出现即挂段尾 +
/// `rto_ms(backoff)`；on_rto 重发同经此处重挂——backoff 已 +1 用新值；
/// Ack 无需重传不挂）。Data 内容按 (seq,len) 从 PATTERN 取（seq 从 iss+1
/// 起连续——越界 = 状态机与 IO 面失配，trap loud）。
fn run_action(
    d: &mut Dma,
    regs: *mut u32,
    mac: &[u8; 6],
    gw_mac: &[u8; 6],
    conn: &bt::Conn,
    action: bt::Action,
    arm: &mut Option<Arm>,
) {
    let arm_at = |end: u32| Some(Arm { end, deadline: now() + bt::rto_ms(conn.rto_backoff) * 1_000_000 });
    match action {
        bt::Action::None => {}
        bt::Action::Syn { seq } => {
            send_seg(d, regs, mac, gw_mac, seq, 0, np::TcpFlags { syn: true, ..Default::default() }, Some(bt::MSS), &[]);
            *arm = arm_at(seq.wrapping_add(1));
        }
        bt::Action::Data { seq, len } => {
            let off = seq.wrapping_sub(conn.snd_iss.wrapping_add(1));
            let end = off as u64 + len as u64;
            if end > PATTERN.len() as u64 {
                println!("vnet: tcp data 段越界 seq={} len={}", seq, len);
                trap();
            }
            let payload = &PATTERN[off as usize..end as usize];
            send_seg(d, regs, mac, gw_mac, seq, conn.rcv_nxt, np::TcpFlags { ack: true, ..Default::default() }, None, payload);
            *arm = arm_at(seq.wrapping_add(len));
        }
        bt::Action::Fin { seq } => {
            send_seg(d, regs, mac, gw_mac, seq, conn.rcv_nxt, np::TcpFlags { fin: true, ack: true, ..Default::default() }, None, &[]);
            *arm = arm_at(seq.wrapping_add(1));
        }
        bt::Action::Ack => {
            send_seg(d, regs, mac, gw_mac, conn.snd_nxt, conn.rcv_nxt, np::TcpFlags { ack: true, ..Default::default() }, None, &[]);
        }
    }
}

/// RTO 挂臂对账：snd_una wrapping 覆盖段尾（差值落在前半 = 已越过）或会话
/// 已终（LastAck 的 ACK 不推进 snd_una，靠 Closed 兜）即清。
fn arm_settled(conn: &bt::Conn, arm: &Arm) -> bool {
    conn.state == bt::State::Closed || conn.snd_una.wrapping_sub(arm.end) < 0x8000_0000
}

/// TCP 自验主循环：主动 connect → 握手 → 64B pattern 回显对拍 → 主动 FIN →
/// 挥手完成 → 打 `TCP ok echo=64`。插在 `NET ok` 自验之后、serve() 之前
/// （A-8b 同款：自验毕转常驻、DNS 面零回归）。
pub fn selftest(d: &mut Dma, regs: *mut u32, mac: &[u8; 6], gw_mac: &[u8; 6]) {
    let iss = now() as u32;
    let (mut conn, out) = bt::Conn::open(iss);
    let mut arm: Option<Arm> = None;
    let mut tw: Option<u64> = None;
    run_action(d, regs, mac, gw_mac, &conn, out.action, &mut arm);
    let cap = now() + TCP_CAP_NS;
    let mut got = 0usize; // 已对拍字节数
    let mut sent = false; // 64B 已发
    let mut closed = false; // close() 已调（终态判别用）
    loop {
        // 进度门（状态谓词驱动，不赌时序）。
        if !sent && conn.state == bt::State::Established {
            sent = true;
            let out = conn.send(PATTERN.len() as u32);
            run_action(d, regs, mac, gw_mac, &conn, out.action, &mut arm);
        }
        if sent && got == PATTERN.len() && !closed
            && conn.state == bt::State::Established
            && arm.is_none() // 数据已确认（close 前置：v1 调用约定）
        {
            closed = true;
            let out = conn.close();
            run_action(d, regs, mac, gw_mac, &conn, out.action, &mut arm);
        }
        // 终态：正常收口（主动关闭全路径走完）或异常终止（RST/重传到帽）。
        if conn.state == bt::State::Closed {
            if closed && got == PATTERN.len() {
                println!("TCP ok echo={}", got);
                return;
            }
            println!("vnet: tcp 会话异常终止 state={:?} got={}", conn.state, got);
            trap();
        }
        // deadline = min(整体帽, RTO 挂臂, TIME_WAIT)。
        let mut dl = cap;
        if let Some(a) = &arm {
            dl = dl.min(a.deadline);
        }
        if let Some(t) = tw {
            dl = dl.min(t);
        }
        // 收帧（杂散帧跳过重收——SLIRP 周期 IPv6 RA 同 DNS 链过滤面）。
        let frame = match recv_frame(d, regs, dl) {
            Some(f) => f,
            None => {
                let t = now();
                if let Some(a) = &arm {
                    if t >= a.deadline {
                        let out = conn.on_rto(); // 重发（或到帽放弃 → Closed）
                        arm = None;
                        run_action(d, regs, mac, gw_mac, &conn, out.action, &mut arm);
                        continue;
                    }
                }
                if let Some(x) = tw {
                    if t >= x {
                        conn.expire_timewait();
                        tw = None;
                        continue;
                    }
                }
                println!("vnet: tcp 自验超时 state={:?} got={}", conn.state, got);
                trap();
            }
        };
        // 四元组过滤（GW→我、TCP、echod→sport）+ parse + 喂状态机（闭包统一
        // `?` 早退——非目标帧（杂散 ARP/IPv6 RA）返 None 跳过重收）。
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
            if t.sport != ECHO_PORT || t.dport != TCP_SPORT {
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
            let out = conn.on_seg(&seg);
            // 交付对拍须在 run_action 写 IOB 前完成（deliver 借用收帧视图）。
            if let Some(del) = out.deliver {
                let end = got + del.len();
                if end > PATTERN.len() || &PATTERN[got..end] != del {
                    println!("vnet: tcp 回显对拍失败 @{}", got);
                    trap();
                }
                got = end;
            }
            run_action(d, regs, mac, gw_mac, &conn, out.action, &mut arm);
            Some(())
        })();
        // 挂臂对账 + TIME_WAIT（重）挂（按状态：处于 TimeWait 即重挂——模块头注）。
        if let Some(a) = &arm {
            if arm_settled(&conn, a) {
                arm = None;
            }
        }
        tw = if conn.state == bt::State::TimeWait {
            Some(now() + bt::TIME_WAIT_MS * 1_000_000)
        } else {
            None
        };
    }
}
