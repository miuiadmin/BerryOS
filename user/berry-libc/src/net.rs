//! # net —— socket ABI 面（A3-1 地基 + A3-2 桩批转真）
//!
//! `docs/事件循环A3设计草案-v1.0.md` §3 的 libc 侧落点：
//! - 字节序四件（htons/ntohs/htonl/ntohl——sockaddr_in 构造消费面）；
//! - sockaddr/addrinfo ABI `#[repr(C)]` + offset_of 全字段编译期钉板
//!   （C-2a Stat 布局案同族执法：「同尺寸不同字段序 = 最阴的静默错」；
//!   布局对源 = libc crate linux_like 非 android 分支——addrinfo 的
//!   ai_addr 紧随 ai_addrlen（glibc 序，musl 同；hermit 是 canonname
//!   先行的异序反例，不可跨 OS 平移））；
//! - socket fd 表宿主（`SOCKETS` static = berry-fsclient `SocketTable`，
//!   fd 15..=22 段）+ fcntl F_GETFL/F_SETFL 旗投影（stubs.rs fcntl 的
//!   socket 腿消费）；
//! - **A3-2 桩批转真**：socket/connect/bind/listen/accept(+4)/send/recv/
//!   setsockopt/getsockopt 真身（RPC 底座 = bsh netd_rpc 移植——严格
//!   串行、超时不重发、netd gift = crt0 GIFT_H[4]）；
//! - getaddrinfo/freeaddrinfo 真实现（IP 字面量快路 + **DNS 慢路**
//!   （A4-2）：miss → 预检本地拒 → netd `OP_DNS_QUERY` RPC → 错码
//!   全表（ST_TIMEOUT → EAI_AGAIN）；netd gift 缺席 = 静默门
//!   EAI_NONAME——拓扑降级同 evs/skt）。
//!
//! ## v1 语义裁定（A3-2 实录，wire 实读定形）
//!
//! - **协议无出站地址域**：OP_TCP_OPEN 只携 port，vnet 段构造 dst=GW_IP
//!   （10.0.2.2）钉死——connect 只认网关地址（其余 ENETUNREACH 诚实拒，
//!   不静默连到别处）；bind/listen 的本地地址同理忽略（单网卡单址）。
//! - **无命名出站连接**：「Open 携 sid = 形域外」（vnet serve 域执法）
//!   ——connect 走 legacy session-0（vnet `sess` 单例，**系统同时至多一个
//!   客户端会话**，二连 ST_BUSY → EIO）。session-0 无 POLL/DRAIN 面：
//!   客户端收 = legacy RT 应答随行回显入槽缓冲（`RESIDUE`），recv 缓冲
//!   空时恒 EAGAIN（独立阻塞收留 v2 协议）；「每写必回等长应答」对端
//!   （echo/回显族）上 POSIX 写读流同构可用。
//! - **服务端会话（listen/accept 产出的 dsid）**：send = 命名 RT 纯发送
//!   臂（A3-1 案②裁定）、recv = POLL 节拍 + DRAIN 收割（bsh evs 同款
//!   1s 节拍）；CLOSED 旗 = 可读余量 + EOF 收口、NOSESSION = 会话已收
//!   口（recv 返 0 = POSIX EOF）。
//! - send 恒 RPC 至应答（v1 无「仅入队」语义——O_NONBLOCK 对 send 不
//!   分岔，A3-1 残留条目同源文档化）；四刀起 Client 臂升级 **K 窗流水线**
//!   （`TCP_PIPE_K`=2：K 个 RT op 在途、应答按尾随 seq 配对收割、陈值
//!   弃、BUSY 同 seq 退避重发——八 op 对外语义零漂移，纯实现面；DNS/
//!   慢 op/Server 臂保持严格串行 v1 形）；connect 恒阻塞至握手帽（无
//!   EINPROGRESS 谎报面）。
//!
//! 本模块无 host 测试（berry-tls 先例：libc 同名符号在 host 测试二进制
//! 对 libSystem 符号插入劫自旋——test-host recipe 注释）；纯函数面
//! （错码映射/revents 映射/容量钉板）在 berry-fsclient，钉板全走编译期
//! const assert + fsclient host 测双执法。

use crate::{c_char, c_int, c_void, ssize_t};
use berry_fsclient::{
    poll_revents, sock_domain_err, sock_rpc_errno, dns_name_ok, dns_rpc_eai, EpollCtlErr,
    EpollTable, SockDomErr, SockEntry, SockKind, SockOp, SocketTable, AF_INET, EPOLLERR, EPOLLHUP,
    EPOLL_CTL_DEL, EPOLL_FD_BASE, EPOLL_FD_MAX, IPPROTO_TCP, POLLERR, POLLHUP, POLLIN, POLLOUT,
    SOCK_FD_BASE, SOCK_FD_MAX, SOCK_STREAM, parse_ipv4, EADDRINUSE, EAGAIN, EAI_AGAIN, EAI_NONAME,
    EIO, EINVAL, ENODEV, EPIPE, ETIMEDOUT,
};
use berry_netdproto as ndp;
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_object_wait_one,
    berry_thread_sleep, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE,
    SIGNAL_WRITABLE,
};

// —— 字节序四件（`arpa/inet.h`；小端 AArch64 上 to_be/from_be = 字节
//    交换——核心一行族，`rotate` 非必需）——

#[no_mangle]
pub extern "C" fn htons(v: u16) -> u16 {
    v.to_be()
}

#[no_mangle]
pub extern "C" fn ntohs(v: u16) -> u16 {
    u16::from_be(v)
}

#[no_mangle]
pub extern "C" fn htonl(v: u32) -> u32 {
    v.to_be()
}

#[no_mangle]
pub extern "C" fn ntohl(v: u32) -> u32 {
    u32::from_be(v)
}

// —— sockaddr ABI（musl aarch64 asm-generic 口径，草案 §3.4）——

pub type sa_family_t = u16;
pub type in_port_t = u16;
pub type in_addr_t = u32;

/// IPv4 套接字地址（16B）：sin_port/sin_addr 网络序（BE——构造经
/// htons/htonl），sin_zero 恒零。
#[repr(C)]
pub struct SockAddrIn {
    pub sin_family: sa_family_t,
    pub sin_port: in_port_t,
    pub sin_addr: in_addr_t,
    pub sin_zero: [u8; 8],
}

/// 通用套接字地址（16B：family + 14B 数据区）。
#[repr(C)]
pub struct SockAddr {
    pub sa_family: sa_family_t,
    pub sa_data: [u8; 14],
}

/// `sockaddr_storage`（128B、8 对齐——POSIX「装下任意 sockaddr」的通用
/// 槽；A3-2 钉板：accept/connect 出参族的调用方缓冲形态）。零消费面时
/// 不长第二套（A3-1 纪律）——出参面真消费的正是本刀。
#[repr(C, align(8))]
pub struct SockAddrStorage {
    pub ss_family: sa_family_t,
    pub __data: [u8; 126],
}

/// getaddrinfo 结果链节点（48B；libc crate linux_like 字段序：ai_addrlen
/// u32 + 4B 自然填充 → ai_addr@24。std（musl 预编译）经 libc crate 绑定
/// 读 ai_addr/ai_next——布局错位 = 调用方读野指针，编译期钉死）。
#[repr(C)]
pub struct AddrInfo {
    pub ai_flags: c_int,
    pub ai_family: c_int,
    pub ai_socktype: c_int,
    pub ai_protocol: c_int,
    pub ai_addrlen: u32,
    /// 显式填充（repr(C) 自然填充的具名化——加字段时错位炸得更响）。
    pub __pad: u32,
    pub ai_addr: *mut SockAddr,
    pub ai_canonname: *mut c_char,
    pub ai_next: *mut AddrInfo,
}

// 布局钉板（fs.rs Stat 全字段钉板同族；相邻同型对 ai_family↔ai_socktype
// ↔ai_protocol、ai_addr↔ai_canonname↔ai_next 全在覆盖内）。
const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<SockAddrIn>() == 16);
    assert!(offset_of!(SockAddrIn, sin_family) == 0);
    assert!(offset_of!(SockAddrIn, sin_port) == 2);
    assert!(offset_of!(SockAddrIn, sin_addr) == 4);
    assert!(offset_of!(SockAddrIn, sin_zero) == 8);
    assert!(size_of::<SockAddr>() == 16);
    assert!(offset_of!(SockAddr, sa_family) == 0);
    assert!(size_of::<SockAddrStorage>() == 128);
    assert!(offset_of!(SockAddrStorage, ss_family) == 0);
    assert!(size_of::<AddrInfo>() == 48);
    assert!(offset_of!(AddrInfo, ai_flags) == 0);
    assert!(offset_of!(AddrInfo, ai_family) == 4);
    assert!(offset_of!(AddrInfo, ai_socktype) == 8);
    assert!(offset_of!(AddrInfo, ai_protocol) == 12);
    assert!(offset_of!(AddrInfo, ai_addrlen) == 16);
    assert!(offset_of!(AddrInfo, ai_addr) == 24);
    assert!(offset_of!(AddrInfo, ai_canonname) == 32);
    assert!(offset_of!(AddrInfo, ai_next) == 40);
};

// errno 单源对拍（fsclient 映射面用 i32 立值、libc lib.rs 同名 c_int
// ——两侧任一漂移编译期炸响；O_NONBLOCK 单源纪律的镜像面）。
const _: () = {
    assert!(EIO == crate::EIO as i32);
    assert!(EAGAIN == crate::EAGAIN as i32);
    assert!(EINVAL == crate::EINVAL as i32);
    assert!(EPIPE == crate::EPIPE as i32);
    assert!(ENODEV == crate::ENODEV as i32);
    assert!(EADDRINUSE == crate::EADDRINUSE as i32);
    assert!(ETIMEDOUT == crate::ETIMEDOUT as i32);
    // EAI 面（A4-2 DNS 档映射锚——getaddrinfo 直接返回码，非 errno）。
    assert!(EAI_NONAME == crate::EAI_NONAME as i32);
    assert!(EAI_AGAIN == crate::EAI_AGAIN as i32);
};

/// `SockDomErr` → errno（A3-1 立常量时的挂账映射面，本刀消费）。
fn dom_errno(e: SockDomErr) -> c_int {
    match e {
        SockDomErr::Af => crate::EAFNOSUPPORT,
        SockDomErr::Type => crate::ESOCKTNOSUPPORT,
        SockDomErr::Proto => crate::EPROTONOSUPPORT,
    }
}

// —— RPC 底座（bsh netd_rpc 移植，A2-3；语义逐字保真）——

/// SLIRP 网关（v1 唯一可达对端——host 序 10.0.2.2；vnet 段构造
/// dst=GW_IP 钉死、OP_TCP_OPEN 无地址域）。
const GW_ADDR_HOST: u32 = 0x0A00_0202;

/// 快 op 收侧帽（LISTEN/ACCEPT/POLL/DRAIN——bsh netd_rpc 同口径 10s）。
const CAP_FAST_NS: u64 = 10_000_000_000;
/// 慢 op 收侧帽（OPEN/RT/CLOSE——vnet 内帽 60s + 余量 30s（S-2b §5.4 整族
/// ×2）：「服务慢 ≠ 服务死」，P6 A-6 纪律）。
const CAP_SLOW_NS: u64 = 90_000_000_000;
/// 阻塞循环轮询节拍（accept-wait/data-wait——bsh evs 1s 同款）。
const POLL_TICK_NS: u64 = 1_000_000_000;

/// Client 臂 BUSY 全局帽（F87 liveness 收口）：单 op 自首次 ST_BUSY 起持续
/// 背压超此时长 → bail EIO。对齐 vnet 侧 op 帽 TCPS_RT_CAP_NS=60s 的语义
/// ——服务端 60s 内必给终局（OK/TIMEOUT），持续 BUSY 超窗 = ops 环占死/
/// 对抗背压迫使客户端弃写（此前无帽形：退避 1s 重发无限循环，send 永不返）。
const BUSY_CAP_NS: u64 = 60_000_000_000;

fn now() -> u64 {
    let mut t = 0u64;
    // 失败 t=0 → 帽值退化为「now+帽」下界，无害。
    berry_clock_read(CLOCK_MONOTONIC, &mut t);
    t
}

/// 阻塞循环的 1s 节拍睡（berry_thread_sleep 绝对时刻帽——park() 同底座；
/// bsh sleep_ms 的 Event-帽形态在 libc 侧无事件资产，线程睡眠直取）。
fn sleep_tick() {
    let mut t = 0u64;
    berry_clock_read(CLOCK_MONOTONIC, &mut t);
    if t != 0 {
        berry_thread_sleep(t + POLL_TICK_NS);
    }
}

/// 严格串行 RPC 的线上层（A2-3 底座；四刀 P2 拆 issue/harvest 两壳）：
/// encode 产物字节 → send（满队 -EBUSY 阻塞等 WRITABLE 重试）→ 应答阻塞
/// 帽等 READABLE → recv 原长。**超时不重发**：晚到应答滞留队列毒化下一
/// 请求是 v1 设计面——四刀起 Client 臂 K 窗（[`sock_send`]）用 seq 配对
/// 收割（陈值 = seq ∉ 在途集即弃），本单发壳供 DNS/OPEN/CLOSE/慢 op/
/// Server 臂保持 v1 严格串行。**五刀①（2026-11-17）**：共享通道漂应答
/// 的毒化面收口——K 窗与单发壳（[`netd_rpc_raw_valid`] 消费面）对非本
/// 请求形状的外来消息弃置续收（帽内 ≤[`FOREIGN_DROP_MAX`] 条，弃置即
/// 消费出通道）；同形陈应答（v1 无关联号）仍不可辨——bail 弃留的兄弟
/// op 迟到应答每具误伤恰一次（跨套接字面，survey F147 口径），dup 通道
/// 多消费者全偿归 wave10/14 债。netd gift 缺席（netd_h()==0）与链断
/// 同返 None（调用方按 op 定 errno——gift 面由公开入口先查，此处 None 即
/// 链断面）。A4-2 抽出：TcpSReq 包装层（[`netd_rpc`]）与 DNS 请求层
/// （[`dns_query`]——请求形态非 TcpSReq）共用同一线上协议。
fn netd_rpc_raw(req: &[u8], cap_ns: u64, rep: &mut [u8]) -> Option<usize> {
    netd_rpc_issue(req)?;
    netd_rpc_harvest(cap_ns, rep)
}

/// 外来应答弃置界（五刀①）：单发壳/K 窗收割弹出的消息若非本请求形状
/// （他请求的迟到应答——共享 netd 通道 wave10/14 已知债的活性面，
/// 2026-11-17 复现靴活体实证），弃置续收而非立即 EIO——帽内至多
/// [`FOREIGN_DROP_MAX`] 条，超出 = 真病面（洪泛/协议破缺）loud 返 None。
/// 弃置即消费：被弃应答出通道（对后续消费者的毒化面顺带清偿一条）。
const FOREIGN_DROP_MAX: usize = 16;

/// 单发壳的帽内外来弃置收割（五刀① P2）：issue 一次后帽内循环收割——
/// 每条消息先经 `valid` 判形，非本请求形状弃置续收；链断/帽到/弃置越界
/// 返 None。收割入 253B 满幅 scratch 再判形（vnet 烘焙线应答 ≤253——
/// 小幅 rep 直读会被超幅外来消息 EINVAL 误判链断）。
fn netd_rpc_raw_valid(
    req: &[u8],
    cap_ns: u64,
    rep: &mut [u8],
    valid: fn(&[u8]) -> bool,
) -> Option<usize> {
    netd_rpc_issue(req)?;
    let dl = now() + cap_ns;
    let mut scratch = [0u8; ndp::RT_SEQ_REP_MAX];
    let mut dropped = 0usize;
    loop {
        let n = netd_rpc_harvest_dl(dl, &mut scratch)?;
        if n > rep.len() {
            // 过检帧超本请求应答幅（如 v1 面弹出 253B seq 形外来帧）——
            // 截断会伪装成合法 v1 形落成功路（审查 P2 实抓），按外来弃置。
            dropped += 1;
            if dropped > FOREIGN_DROP_MAX {
                return None;
            }
            continue;
        }
        if valid(&scratch[..n]) {
            rep[..n].copy_from_slice(&scratch[..n]);
            return Some(n);
        }
        dropped += 1;
        if dropped > FOREIGN_DROP_MAX {
            return None;
        }
    }
}

/// [`netd_rpc`] 的判形收割形（rpc_st/rpc_data/rpc_sid 消费面）。
fn netd_rpc_valid(
    req: ndp::TcpSReq<'_>,
    session: u32,
    cap_ns: u64,
    rep: &mut [u8],
    valid: fn(&[u8]) -> bool,
) -> Option<usize> {
    let mut buf = [0u8; ndp::REQ_S_LEN_MAX];
    let n = ndp::encode_tcp_s_request(&mut buf, req, session)?;
    // buf 前 n 字节已初始化（encode 产物）。
    netd_rpc_raw_valid(&buf[..n], cap_ns, rep, valid)
}

/// RPC 发出相（四刀 P2 拆壳）：send 满队 -EBUSY 阻塞等 WRITABLE 重试
/// （CAP_FAST 帽）；链断 None。K 窗的 issue 相直用。
fn netd_rpc_issue(req: &[u8]) -> Option<()> {
    let netd = crate::io::netd_h();
    if netd == 0 {
        return None;
    }
    let send_dl = now() + CAP_FAST_NS;
    loop {
        let s = berry_channel_send(netd, req.as_ptr(), req.len(), core::ptr::null(), 0);
        if s == 0 {
            return Some(());
        }
        if s != BERRY_ERR_BUSY {
            return None; // 链断面（非满队）——非慢而活
        }
        let mut obs = 0u64;
        if berry_object_wait_one(netd, SIGNAL_WRITABLE, send_dl, &mut obs) != 0 {
            return None;
        }
    }
}

/// RPC 收割相（四刀 P2 拆壳）：应答阻塞帽等 READABLE → recv 原长；帽到/
/// 链断 None（不重发——迟到应答按陈值由 seq 收割面弃）。
fn netd_rpc_harvest(cap_ns: u64, rep: &mut [u8]) -> Option<usize> {
    netd_rpc_harvest_dl(now() + cap_ns, rep)
}

/// 收割相的绝对时刻形（K 窗用：收割帽可收到 BUSY 退避时刻——空等帽
/// 不吞重发窗）。
fn netd_rpc_harvest_dl(dl: u64, rep: &mut [u8]) -> Option<usize> {
    let netd = crate::io::netd_h();
    if netd == 0 {
        return None;
    }
    loop {
        let mut nb = rep.len();
        let mut nhd = 0usize;
        // rep 可写区 = nb；调用方保证缓冲 ≥ 最大应答。
        let s =
            berry_channel_recv(netd, rep.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nhd);
        if s == 0 {
            return Some(nb);
        }
        if s != BERRY_ERR_AGAIN {
            return None; // 坏 handle 类链断——立即死不空等
        }
        let mut obs = 0u64;
        if berry_object_wait_one(netd, SIGNAL_READABLE, dl, &mut obs) != 0 {
            return None; // 帽到无应答 = 断链面（不重发）
        }
    }
}

/// TcpSReq 形 RPC 包装（A2-3 原型——语义逐字保真，encode 委托后走
/// [`netd_rpc_raw`]）。应答形随 op 三分 Sid/Poll/Tcp，恰长 parse 归调用方
/// 包装层。
fn netd_rpc(req: ndp::TcpSReq<'_>, session: u32, cap_ns: u64, rep: &mut [u8]) -> Option<usize> {
    let mut buf = [0u8; ndp::REQ_S_LEN_MAX];
    let n = ndp::encode_tcp_s_request(&mut buf, req, session)?;
    // buf 前 n 字节已初始化（encode 产物）。
    netd_rpc_raw(&buf[..n], cap_ns, rep)
}

/// DNS A 查询（A4-2）：name → netd `OP_DNS_QUERY` RPC → 点分序四字节
/// 地址。应答错码映射走 [`dns_rpc_eai`] 全表（ST_TIMEOUT → EAI_AGAIN /
/// ST_BAD → EAI_NONAME）；RPC 链断（含 netd gift 缺席——公开入口已先查
/// 静默门，此处 None = 真链断）与畸形应答（parse_reply 烘焙线拒）同路
/// **EAI_NONAME**（拓扑降级同 evs/skt——v1 无 EAI_SYSTEM 档，链断对
/// 调用方 = 「此名此路不可解」，musl 对解析器不可达同用 NONAME 族）。
/// 帽 = CAP_FAST（快 op——单 A 查询往返，服务侧 20s 帽 < 本帽余量足）。
fn dns_query(name: &[u8]) -> Result<[u8; 4], c_int> {
    let mut req = [0u8; 2 + ndp::NAME_MAX];
    let n = match ndp::encode_request(&mut req, ndp::OP_DNS_QUERY, name) {
        Some(n) => n,
        None => return Err(crate::EAI_NONAME), // 空/越帽——预检已拦，防御面
    };
    let mut rep = [0u8; ndp::REPLY_LEN];
    let got = netd_rpc_raw_valid(&req[..n], CAP_FAST_NS, &mut rep, |b| {
        ndp::parse_reply(b).is_some()
    });
    let r = got
        .and_then(|n| ndp::parse_reply(&rep[..n]))
        .ok_or(crate::EAI_NONAME)?; // 链断/畸形同路
    match r.st {
        ndp::ST_OK => Ok(r.addr),
        st => Err(dns_rpc_eai(st)),
    }
}

/// Tcp 形应答（OPEN / 命名 RT / DRAIN）的 st-only 消费：ST_OK → Ok；
/// 非 OK → op 参数化 errno；RPC/parse 失败 → EIO（链断 loud）。五刀①：
/// 非 Tcp 形外来应答经 [`netd_rpc_valid`] 弃置续收——同形陈应答（v1 无
/// 关联号）不可辨，维持 wave10/14 边界（头注见 [`netd_rpc_raw`]）。
fn rpc_st(op: SockOp, req: ndp::TcpSReq<'_>, session: u32, cap_ns: u64) -> Result<(), c_int> {
    let mut rep = [0u8; ndp::TCP_REP_MAX];
    // 五刀①（审查收紧）：本面请求恒 v1 无 seq——seq 形应答结构性外来，
    // 判形加 seq 必 None（否则 253B seq 帧经幅差截断可伪装 v1 落成功路）。
    let n = netd_rpc_valid(req, session, cap_ns, &mut rep, |b| {
        ndp::parse_tcp_reply(b).map_or(false, |r| r.seq.is_none())
    })
    .ok_or(crate::EIO)?;
    let r = ndp::parse_tcp_reply(&rep[..n]).ok_or(crate::EIO)?;
    if r.st == ndp::ST_OK {
        Ok(())
    } else {
        Err(sock_rpc_errno(op, r.st))
    }
}

/// Tcp 形应答的携数据消费（legacy RT 回显 / DRAIN 收割）：数据拷入 out
/// （截 min），返回拷出长度。
fn rpc_data(
    op: SockOp,
    req: ndp::TcpSReq<'_>,
    session: u32,
    cap_ns: u64,
    out: &mut [u8],
) -> Result<usize, c_int> {
    let mut rep = [0u8; ndp::TCP_REP_MAX];
    // 五刀①（审查收紧）：同 rpc_st——seq 形应答对本面结构性外来。
    let n = netd_rpc_valid(req, session, cap_ns, &mut rep, |b| {
        ndp::parse_tcp_reply(b).map_or(false, |r| r.seq.is_none())
    })
    .ok_or(crate::EIO)?;
    let r = ndp::parse_tcp_reply(&rep[..n]).ok_or(crate::EIO)?;
    if r.st != ndp::ST_OK {
        return Err(sock_rpc_errno(op, r.st));
    }
    let c = r.data.len().min(out.len());
    out[..c].copy_from_slice(&r.data[..c]);
    Ok(c)
}

/// Sid 形应答（LISTEN session=0 恰长形 / ACCEPT 尾随 lsid 形）：ST_OK →
/// sid；非 OK → op 参数化 errno。
fn rpc_sid(op: SockOp, req: ndp::TcpSReq<'_>, session: u32, cap_ns: u64) -> Result<u32, c_int> {
    let mut rep = [0u8; ndp::SID_REP_LEN];
    let valid: fn(&[u8]) -> bool = match op {
        SockOp::Listen => |b| ndp::parse_listen_reply(b).is_some(),
        _ => |b| ndp::parse_accept_reply(b).is_some(),
    };
    let n = netd_rpc_valid(req, session, cap_ns, &mut rep, valid).ok_or(crate::EIO)?;
    let r = match op {
        SockOp::Listen => ndp::parse_listen_reply(&rep[..n]),
        _ => ndp::parse_accept_reply(&rep[..n]),
    }
    .ok_or(crate::EIO)?;
    if r.st == ndp::ST_OK {
        Ok(r.sid)
    } else {
        Err(sock_rpc_errno(op, r.st))
    }
}

/// POLL 快照三分：活会话（flags/rbytes）/ 会话已收口（NOSESSION——recv
/// 的 EOF 语义位，非 errno 面）/ 错误。
enum PollState {
    Live { flags: u8, rbytes: u16 },
    Gone,
}

fn rpc_poll(session: u32) -> Result<PollState, c_int> {
    let mut rep = [0u8; ndp::POLL_REP_LEN];
    // 五刀①（审查补）：Server 臂最热收割面换免疫形——4B 小幅 rep 会被
    // 超幅外来消息 EINVAL 回滚钉死队首（recv 恒 EIO/epoll 恒 POLLERR）。
    let n = netd_rpc_valid(ndp::TcpSReq::Poll, session, CAP_FAST_NS, &mut rep, |b| {
        ndp::parse_poll_reply(b).is_some()
    })
    .ok_or(crate::EIO)?;
    let r = ndp::parse_poll_reply(&rep[..n]).ok_or(crate::EIO)?;
    match r.st {
        ndp::ST_OK => Ok(PollState::Live {
            flags: r.flags,
            rbytes: r.rbytes,
        }),
        ndp::ST_NOSESSION => Ok(PollState::Gone),
        st => Err(sock_rpc_errno(SockOp::Recv, st)),
    }
}

// —— socket fd 表（宿主；单线程 static mut 直写——fs.rs FDT/io.rs
//    ALIASES 同口径）——

static mut SOCKETS: SocketTable = SocketTable::new();

/// fd 是在册 socket（15..=22 且槽非空）——stubs.rs fcntl/poll 腿与
/// io.rs read/write/close 分派链尾的归属谓词。
pub(crate) fn sock_valid(fd: c_int) -> bool {
    // SAFETY: 只读；唯一写点 = 本模块（单线程 lib.rs ABI 口径）。
    unsafe { SOCKETS.get(fd).is_some() }
}

/// F_GETFL 旗投影：socket fd 恒读写两用（POSIX 访问模式位 O_RDWR），
/// O_NONBLOCK 按表项旗。非在册 socket = None（EBADF 面，调用方收口）。
pub(crate) fn sock_getfl(fd: c_int) -> Option<c_int> {
    // SAFETY: 只读借用（生命周期不跨调用）。
    unsafe {
        SOCKETS
            .get(fd)
            .map(|e| crate::fs::O_RDWR | if e.nonblock { crate::fs::O_NONBLOCK } else { 0 })
    }
}

/// F_SETFL 旗写入：v1 只认 O_NONBLOCK 一位（其余位忽略不拒——std 的
/// 读改写形态 F_GETFL→改位→F_SETFL 天然兼容：读回什么写回什么，只有
/// O_NONBLOCK 真生效）。非在册 = None（EBADF 面）。
pub(crate) fn sock_setfl(fd: c_int, arg: c_int) -> Option<()> {
    // SAFETY: get_mut 短借写旗（单线程无并发 take）。
    unsafe { SOCKETS.get_mut(fd).map(|e| e.nonblock = (arg & crate::fs::O_NONBLOCK) != 0) }
}

/// 表安装口：fd 15..=22 最低空位，满表 None → EMFILE。
fn sock_alloc(e: SockEntry) -> Option<c_int> {
    // SAFETY: 单线程 static mut 直写（lib.rs ABI 口径）。
    unsafe { SOCKETS.alloc(e) }
}

/// fd → 槽号（sock_valid 已保证在段内；RESIDUE 寻址用）。
fn slot_of(fd: c_int) -> usize {
    (fd - SOCK_FD_BASE) as usize
}

// —— 槽缓冲（RESIDUE）：客户端 legacy RT 回显 + 服务端 DRAIN 短读余量
//    （io.rs PEND 同款偏移口径；单槽容量四刀 P3 = K×TCP_DATA_MAX——
//    「恰一应答」收发粒度帽随 K 窗同步放宽，准入式见 sock_send）——

/// 槽缓冲容量（四刀 P3）：250 → K×TCP_DATA_MAX=500——K 窗在途 op 的
/// 回显暂存上限（SOCK_FD_MAX=8，静态 +2KB）。
const RES_MAX: usize = ndp::RT_PIPE_K * ndp::TCP_DATA_MAX;
static mut RESIDUE: [[u8; RES_MAX]; SOCK_FD_MAX] = [[0; RES_MAX]; SOCK_FD_MAX];
static mut RES_LEN: [usize; SOCK_FD_MAX] = [0; SOCK_FD_MAX];
static mut RES_OFF: [usize; SOCK_FD_MAX] = [0; SOCK_FD_MAX];

fn res_pending(slot: usize) -> usize {
    // SAFETY: 只读。
    unsafe { RES_LEN[slot] - RES_OFF[slot] }
}

/// 追加（尾空间不足先压缩 off 前缀——data ≤ RES_MAX 压缩后必容；空表
/// 且超容 = false，调用方保证不达）。
fn res_append(slot: usize, data: &[u8]) {
    // SAFETY: 单线程 static mut 直写；data ≠ RESIDUE 同槽区（RPC 应答
    // 缓冲是栈上独立对象）。
    unsafe {
        if RES_MAX - RES_LEN[slot] < data.len() {
            let keep = RES_LEN[slot] - RES_OFF[slot];
            core::ptr::copy(
                RESIDUE[slot].as_ptr().add(RES_OFF[slot]),
                RESIDUE[slot].as_mut_ptr(),
                keep,
            );
            RES_LEN[slot] = keep;
            RES_OFF[slot] = 0;
        }
        let l = RES_LEN[slot];
        RESIDUE[slot][l..l + data.len()].copy_from_slice(data);
        RES_LEN[slot] = l + data.len();
    }
}

/// 取出（min(avail, out.len())；读穿归零复位）。
fn res_take(slot: usize, out: &mut [u8]) -> usize {
    // SAFETY: 单线程 static mut 读写；out 是调用方缓冲。
    unsafe {
        let avail = RES_LEN[slot] - RES_OFF[slot];
        if avail == 0 {
            return 0;
        }
        let c = avail.min(out.len());
        out[..c].copy_from_slice(&RESIDUE[slot][RES_OFF[slot]..RES_OFF[slot] + c]);
        RES_OFF[slot] += c;
        if RES_OFF[slot] == RES_LEN[slot] {
            RES_LEN[slot] = 0;
            RES_OFF[slot] = 0;
        }
        c
    }
}

fn res_clear(slot: usize) {
    // SAFETY: 单线程 static mut 直写。
    unsafe {
        RES_LEN[slot] = 0;
        RES_OFF[slot] = 0;
    }
}

// —— socket 族真身（A3-2；stubs.rs 同名桩已摘除）——

/// `socket(2)`：域执法（AF_INET + SOCK_STREAM + proto ∈ {0, IPPROTO_TCP}）
/// + 本地表安装（不发包）。type 旗位（SOCK_NONBLOCK/SOCK_CLOEXEC——
/// Linux 2.6.27+ 原子旗）：剥后域检，NONBLOCK 入表项；CLOEXEC v1 无
/// exec 面忽略（O_NONBLOCK 与 SOCK_NONBLOCK 同值 0x800——Linux 故意
/// 对齐，单一常量面）。
#[no_mangle]
pub extern "C" fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int {
    const SOCK_CLOEXEC: c_int = 0o2000000; // 0x80000（Linux/musl 同值）
    let masked = ty & !(SOCK_CLOEXEC | crate::fs::O_NONBLOCK);
    if let Err(e) = sock_domain_err(domain, masked, protocol) {
        crate::set_errno(dom_errno(e));
        return -1;
    }
    let entry = SockEntry {
        kind: SockKind::Unconnected,
        port: 0,
        sid: 0,
        nonblock: (ty & crate::fs::O_NONBLOCK) != 0,
    };
    match sock_alloc(entry) {
        Some(fd) => fd,
        None => {
            crate::set_errno(crate::EMFILE);
            -1
        }
    }
}

/// `connect(2)`：目标 = legacy OP_TCP_OPEN（session-0 恰长形）。**地址
/// 裁定（v1）**：协议无出站地址域（目的地恒 = SLIRP 网关 10.0.2.2，vnet
/// 段构造钉死）——sin_addr 非网关 → ENETUNREACH 诚实拒；port 0 →
/// EINVAL；family 域外 → EAFNOSUPPORT。恒阻塞至握手帽（vnet 30s →
/// client 45s；O_NONBLOCK 不分岔——无 EINPROGRESS 谎报面，异步连接 v1
/// 无协议承载）。成功 → Client 表项（sid=0：session-0 系统独占——
/// 二连由 vnet ST_BUSY → EIO 收口）。
#[no_mangle]
pub unsafe extern "C" fn connect(fd: c_int, addr: *const SockAddr, len: u32) -> c_int {
    let mut e = match sock_entry(fd) {
        Some(e) => e,
        None => return err_ret(crate::EBADF),
    };
    if e.kind != SockKind::Unconnected {
        return err_ret(crate::EINVAL); // 已连/已听：v1 近似档（POSIX EISCONN 族不立案）
    }
    // SAFETY: 调用方保证 addr 指向 len 字节有效区（libc 契约）。
    let sa = match sockaddr_in_view(addr, len) {
        Some(sa) => sa,
        None => return err_ret(crate::EINVAL),
    };
    if sa.family != AF_INET as sa_family_t {
        return err_ret(crate::EAFNOSUPPORT);
    }
    if sa.port == 0 {
        return err_ret(crate::EINVAL);
    }
    if sa.addr != GW_ADDR_HOST {
        return err_ret(crate::ENETUNREACH);
    }
    if crate::io::netd_h() == 0 {
        return err_ret(crate::ENODEV); // 无 netd gift（EFI/裸/无盘拓扑）
    }
    match rpc_st(SockOp::Connect, ndp::TcpSReq::Open { port: sa.port }, 0, CAP_SLOW_NS) {
        Ok(()) => {
            e.kind = SockKind::Client;
            // SAFETY: 短借写回（Copy 值语义——RPC 期间无表操作）。
            unsafe {
                if let Some(slot) = SOCKETS.get_mut(fd) {
                    *slot = e;
                }
            }
            0
        }
        Err(errno) => err_ret(errno),
    }
}

/// `bind(2)`：纯本地端口记账（netdproto LISTEN 自带 port——bind/listen
/// 拆 POSIX 面、合 RPC 面，草案 §3.2）。addr 值忽略（协议无本地地址域
/// ——单网卡单址）；port 0 收下（listen 时拒——netdproto LISTEN 无 0 端
/// 口编码面，v1 无临时端口分配）。
#[no_mangle]
pub unsafe extern "C" fn bind(fd: c_int, addr: *const SockAddr, len: u32) -> c_int {
    let mut e = match sock_entry(fd) {
        Some(e) => e,
        None => return err_ret(crate::EBADF),
    };
    if e.kind != SockKind::Unconnected {
        return err_ret(crate::EINVAL);
    }
    // SAFETY: libc 契约（同 connect）。
    let sa = match sockaddr_in_view(addr, len) {
        Some(sa) => sa,
        None => return err_ret(crate::EINVAL),
    };
    if sa.family != AF_INET as sa_family_t {
        return err_ret(crate::EAFNOSUPPORT);
    }
    e.port = sa.port;
    // SAFETY: 短借写回。
    unsafe {
        if let Some(slot) = SOCKETS.get_mut(fd) {
            *slot = e;
        }
    }
    0
}

/// `listen(2)`：OP_TCP_LISTEN（恰长形 session=0）→ lsid → Listen 表项。
/// 需先 bind 显式端口（port 0 → EINVAL——v1 无临时端口面）；backlog v1
/// 忽略（netd 会话表容量 8 是真上界）。
#[no_mangle]
pub extern "C" fn listen(fd: c_int, _backlog: c_int) -> c_int {
    let mut e = match sock_entry(fd) {
        Some(e) => e,
        None => return err_ret(crate::EBADF),
    };
    if e.kind != SockKind::Unconnected {
        return err_ret(crate::EINVAL);
    }
    if e.port == 0 {
        return err_ret(crate::EINVAL);
    }
    if crate::io::netd_h() == 0 {
        return err_ret(crate::ENODEV);
    }
    match rpc_sid(SockOp::Listen, ndp::TcpSReq::Listen { port: e.port }, 0, CAP_FAST_NS) {
        Ok(lsid) => {
            e.kind = SockKind::Listen;
            e.sid = lsid;
            // SAFETY: 短借写回。
            unsafe {
                if let Some(slot) = SOCKETS.get_mut(fd) {
                    *slot = e;
                }
            }
            0
        }
        Err(errno) => err_ret(errno),
    }
}

/// `accept4(2)`：OP_TCP_ACCEPT（尾随 lsid）→ dsid → Server 表项 + 新
/// fd。空队 ST_EAGAIN 立即返——阻塞档 1s 节拍重拉（bsh evs accept-wait
/// 同款；ACCEPT 非阻塞快返是 wire 真值），非阻塞档（表项旗或 SOCK_
/// NONBLOCK 旗）EAGAIN 直返。flags 只收 SOCK_NONBLOCK（新套接字旗，POSIX
/// 不继承 listen fd 旗）/SOCK_CLOEXEC（v1 无 exec 面，忽略）。对端地址
/// 出参 = 钉 10.0.2.2（hostfwd 宿主源——wire 不携对端端口，sin_port=0
/// 诚实未知）；addr/addrlen 任一 NULL 跳写。
#[no_mangle]
pub unsafe extern "C" fn accept4(
    fd: c_int,
    addr: *mut SockAddr,
    addrlen: *mut u32,
    flags: c_int,
) -> c_int {
    const SOCK_CLOEXEC: c_int = 0o2000000;
    if flags & !(SOCK_CLOEXEC | crate::fs::O_NONBLOCK) != 0 {
        return err_ret(crate::EINVAL);
    }
    let e = match sock_entry(fd) {
        Some(e) => e,
        None => return err_ret(crate::EBADF),
    };
    if e.kind != SockKind::Listen {
        return err_ret(crate::EINVAL); // POSIX「非监听套接字」档
    }
    if crate::io::netd_h() == 0 {
        return err_ret(crate::ENODEV);
    }
    loop {
        match rpc_sid(SockOp::Accept, ndp::TcpSReq::Accept, e.sid, CAP_FAST_NS) {
            Ok(dsid) => {
                let entry = SockEntry {
                    kind: SockKind::Server,
                    port: e.port,
                    sid: dsid,
                    nonblock: (flags & crate::fs::O_NONBLOCK) != 0,
                };
                let nfd = match sock_alloc(entry) {
                    Some(n) => n,
                    None => {
                        // 表满：netd 侧会话即弃（best-effort CLOSE 释 vnet
                        // 槽——不弃 = 会话表泄一个真容量位）。
                        let _ = rpc_st(SockOp::Close, ndp::TcpSReq::Close, dsid, CAP_SLOW_NS);
                        return err_ret(crate::EMFILE);
                    }
                };
                if !addr.is_null() {
                    // SAFETY: 出参写点（libc 契约）。
                    unsafe {
                        *addr = SockAddr {
                            sa_family: AF_INET as sa_family_t,
                            sa_data: [
                                0, 0, // sin_port = 0（未知）
                                10, 0, 2, 2, // sin_addr = 10.0.2.2（网关/hostfwd 源）
                                0, 0, 0, 0, 0, 0, 0, 0,
                            ],
                        };
                        if !addrlen.is_null() {
                            *addrlen = core::mem::size_of::<SockAddrIn>() as u32;
                        }
                    }
                }
                return nfd;
            }
            Err(crate::EAGAIN) => {
                if e.nonblock {
                    return err_ret(crate::EAGAIN);
                }
                sleep_tick();
            }
            Err(errno) => return err_ret(errno),
        }
    }
}

/// `accept(2)` = accept4(flags=0) 委托（musl 同形）。
#[no_mangle]
pub unsafe extern "C" fn accept(fd: c_int, addr: *mut SockAddr, addrlen: *mut u32) -> c_int {
    unsafe { accept4(fd, addr, addrlen, 0) }
}

/// `setsockopt(2)`：谎报白名单（草案 §3.2——std::net connect 链设
/// TCP_NODELAY/SO_KEEPALIVE 族；v1 无 sockopt 语义面，「v1 无此语义」≠
/// 诚实失败：上游消费面把这些失败当异常。C-2a errno 档位纪律同族）。
/// 在册 socket 恒 0；非 socket fd → EBADF。
#[no_mangle]
pub unsafe extern "C" fn setsockopt(
    fd: c_int,
    _level: c_int,
    _optname: c_int,
    _optval: *const c_void,
    _optlen: u32,
) -> c_int {
    if !sock_valid(fd) {
        return err_ret(crate::EBADF);
    }
    0
}

/// `getsockopt(2)`：SO_TYPE → SOCK_STREAM / SO_ERROR → 0 / 其余定值 0
/// （白名单面同 setsockopt）。optval/optlen 空缺跳写（EFAULT 档不立——
/// 现役消费面恒携有效出参）。
#[no_mangle]
pub unsafe extern "C" fn getsockopt(
    fd: c_int,
    level: c_int,
    optname: c_int,
    optval: *mut c_void,
    optlen: *mut u32,
) -> c_int {
    const SOL_SOCKET: c_int = 1;
    const SO_TYPE: c_int = 3;
    if !sock_valid(fd) {
        return err_ret(crate::EBADF);
    }
    let v: c_int = if level == SOL_SOCKET && optname == SO_TYPE {
        SOCK_STREAM
    } else {
        0
    };
    if !optval.is_null() && !optlen.is_null() {
        // SAFETY: 出参写点（长度帽内 4B——i32 对齐由 c_int 指针类型承载）。
        unsafe {
            if *optlen >= 4 {
                *(optval as *mut c_int) = v;
                *optlen = 4;
            }
        }
    }
    0
}

// —— send/recv（分形：Client = legacy RT 回显缓冲；Server = 命名 RT /
//    POLL+DRAIN——v1 语义裁定见模块头注）——

/// errno 置位 + -1 收口（pub 面小助手）。
fn err_ret(e: c_int) -> c_int {
    crate::set_errno(e);
    -1
}

/// 表项拷出（Copy 值语义——RPC 期间不持表借用）。
fn sock_entry(fd: c_int) -> Option<SockEntry> {
    // SAFETY: 只读拷出。
    unsafe { SOCKETS.get(fd).copied() }
}

/// sockaddr_in 视图（len < 16 → None；字段已转主机序）。
struct AddrView {
    family: sa_family_t,
    port: u16,
    addr: u32,
}

/// # Safety
/// addr 指向至少 len 字节有效区（libc 契约）。
unsafe fn sockaddr_in_view(addr: *const SockAddr, len: u32) -> Option<AddrView> {
    if addr.is_null() || (len as usize) < core::mem::size_of::<SockAddrIn>() {
        return None;
    }
    // SAFETY: 上方长度已验；repr(C) 布局编译期钉板（本文件 const 块）。
    let sa = unsafe { &*(addr as *const SockAddrIn) };
    Some(AddrView {
        family: sa.sin_family,
        port: u16::from_be(sa.sin_port),
        addr: u32::from_be(sa.sin_addr),
    })
}

/// Client 臂 K 窗流水线深度（四刀 P2）：与 vnet session-0 挂起 op 队列
/// 共源（netdproto [`ndp::RT_PIPE_K`] 单源——恰满 berry-tcp 环 500 ≤ 512）。
const TCP_PIPE_K: usize = ndp::RT_PIPE_K;

/// K 窗在途 op 槽（issue/harvest 两相的暂存：请求序号 + 回显缓冲——
/// BUSY 重发时 buf 前缀即请求原文，got>0 后被回显覆写）。
#[derive(Clone, Copy)]
struct PipeOp {
    seq: u8,
    len: usize,
    got: usize,
    /// ST_BUSY 退避重发时刻（0 = 无待重发）。
    retry_dl: u64,
    /// 首次 ST_BUSY 时刻（0 = 从未背压；F87 全局帽起算点——重发-再 BUSY
    /// 循环不顺延，仅 ST_OK 收割清零）。
    busy_since: u64,
    echo: [u8; ndp::TCP_DATA_MAX],
}

/// `send(2)` / write 链尾（sock_send）：按 kind 分形——
/// - Client：legacy RT（session-0）分片 ≤TCP_DATA_MAX 走 **K 窗流水线**
///   （四刀 P2：issue/harvest 两相——K 个 op 在途、应答按 seq 配对收割、
///   陈值弃、BUSY 同 seq 退避重发＋持续背压 60s 全局帽到 EIO（F87）；回显按 op 序入槽缓冲——每 op 应答
///   仍严格等长携本 op 回显，ROUNDTRIP 对外语义零漂移）；缓冲余量不足
///   → 部分写返回（首片即不足 = EAGAIN——读写节奏面，容量 = K×250）；
/// - Server：命名 RT 纯发送臂（应答不携数据，A3-1 案②）；
/// - Unconnected/Listen → EPIPE（Linux 口径：非连接套接字 send）。
pub(crate) fn sock_send(fd: c_int, buf: *const u8, n: usize) -> ssize_t {
    let e = match sock_entry(fd) {
        Some(e) => e,
        None => return err_ret(crate::EBADF) as ssize_t,
    };
    match e.kind {
        SockKind::Unconnected | SockKind::Listen => err_ret(crate::EPIPE) as ssize_t,
        SockKind::Client => sock_send_pipe(fd, buf, n),
        SockKind::Server => {
            let mut sent = 0usize;
            while sent < n {
                let chunk = (n - sent).min(ndp::TCP_DATA_MAX);
                // SAFETY: 同上。
                let data = unsafe { core::slice::from_raw_parts(buf.add(sent), chunk) };
                match rpc_st(SockOp::Send, ndp::TcpSReq::Roundtrip { data }, e.sid, CAP_SLOW_NS) {
                    Ok(()) => sent += chunk,
                    Err(errno) => {
                        return if sent > 0 {
                            sent as ssize_t
                        } else {
                            err_ret(errno) as ssize_t
                        };
                    }
                }
            }
            sent as ssize_t
        }
    }
}

/// Client 臂 K 窗实现（四刀 P2）。
fn sock_send_pipe(fd: c_int, buf: *const u8, n: usize) -> ssize_t {
    let slot = slot_of(fd);
    let mut sent = 0usize;
    // 窗口槽 FIFO（占用区 head..head+inflight）；seq 滚动发号（回绕距
    // 256 ≫ K——陈值判定 = seq ∉ 在途集即弃，超时 op 的迟到应答不毒化
    // 下一窗）。
    let mut win: [Option<PipeOp>; TCP_PIPE_K] = [None; TCP_PIPE_K];
    let (mut head, mut inflight) = (0usize, 0usize);
    // 五刀①：外来消息（非 Tcp 形/陈 seq）弃置计数——帽内续收，越界
    // （> FOREIGN_DROP_MAX）= 洪泛/协议破缺真病面，loud bail。
    let mut foreign = 0usize;
    /// 错误返回收口：已发部分 → 部分写；首片 → errno（在途 op 的迟到
    /// 应答按陈值弃——seq 收割面纪律，echo 不再入槽）。
    fn bail(sent: usize, errno: c_int) -> ssize_t {
        if sent > 0 {
            sent as ssize_t
        } else {
            err_ret(errno) as ssize_t
        }
    }
    while sent < n || inflight > 0 {
        // —— issue 相 ——
        // ① BUSY 退避到期：同 seq 重发（请求原文在 echo 前缀——BUSY 即
        //    未入环必无回显覆写）。
        for w in win.iter_mut() {
            if let Some(op) = w {
                if op.retry_dl != 0 && now() >= op.retry_dl {
                    let mut req = [0u8; ndp::RT_SEQ_REQ_MAX];
                    if let Some(rn) =
                        ndp::encode_rt_seq_request(&mut req, &op.echo[..op.len], op.seq)
                    {
                        let _ = netd_rpc_issue(&req[..rn]); // 链断留收割相收口
                    }
                    op.retry_dl = 0;
                }
            }
        }
        // ② 空槽 + 余量准入 → 发下一片（seq 形）。
        while inflight < TCP_PIPE_K && sent < n {
            let chunk = (n - sent).min(ndp::TCP_DATA_MAX);
            // 余量准入（P3）：槽缓冲余量 ≥ 在途 + 新片（在途 op 的回显
            // 将全部入槽）——不足时不再 issue，先收割腾位。
            if res_pending(slot) + (inflight + 1) * ndp::TCP_DATA_MAX > RES_MAX {
                break;
            }
            // SAFETY: 调用方缓冲前 n 字节有效（libc 契约）。
            let data = unsafe { core::slice::from_raw_parts(buf.add(sent), chunk) };
            let seq = seq_next();
            let mut req = [0u8; ndp::RT_SEQ_REQ_MAX];
            let rn = match ndp::encode_rt_seq_request(&mut req, data, seq) {
                Some(rn) => rn,
                None => break, // 参数面（chunk ≤ TCP_DATA_MAX 恒过——防御）
            };
            if netd_rpc_issue(&req[..rn]).is_none() {
                if inflight == 0 {
                    return bail(sent, crate::EIO);
                }
                break;
            }
            let i = (head + inflight) % TCP_PIPE_K;
            let mut echo = [0u8; ndp::TCP_DATA_MAX];
            echo[..chunk].copy_from_slice(data);
            win[i] = Some(PipeOp { seq, len: chunk, got: 0, retry_dl: 0, busy_since: 0, echo });
            inflight += 1;
            sent += chunk;
        }
        // —— 收割相前置判据：窗空且全发完 = 收工；窗空但发不出 = 节奏面
        //（首片即不足 = EAGAIN——读写节奏面，调用方先读再写）。
        if inflight == 0 {
            if sent >= n {
                return sent as ssize_t;
            }
            return bail(sent, crate::EAGAIN);
        }
        // —— harvest 相：帽内等一应答（有 BUSY 待重发时收割帽收到退避
        // 时刻——空等帽不吞重发窗）；seq 配对，陈值弃续收。——
        let mut cap = now() + CAP_SLOW_NS;
        for w in win.iter() {
            if let Some(op) = w {
                if op.retry_dl != 0 && op.retry_dl < cap {
                    cap = op.retry_dl;
                }
            }
        }
        let mut rep = [0u8; ndp::RT_SEQ_REP_MAX];
        let got = match netd_rpc_harvest_dl(cap, &mut rep) {
            Some(k) => k,
            None => {
                // 帽到：若因退避时刻截短（重发窗到）→ 回 issue 相；真帽到
                // → 部分写/EIO。
                let retry_due = win.iter().any(|w| {
                    w.as_ref().map(|o| o.retry_dl != 0 && now() >= o.retry_dl) == Some(true)
                });
                if retry_due {
                    continue;
                }
                return bail(sent, crate::EIO);
            }
        };
        let r = match ndp::parse_tcp_reply(&rep[..got]) {
            Some(r) => r,
            None => {
                // 五刀①：非 Tcp 形外来应答（他请求迟到——共享通道活性面）
                // 弃置续收，不 bail 本窗口；越界 = 真病面 loud。
                foreign += 1;
                if foreign > FOREIGN_DROP_MAX {
                    return bail(sent, crate::EIO);
                }
                continue;
            }
        };
        // seq 配对：命中在途 op 收割；陈值（∉ 在途集——迟到/无 seq 形）
        // 弃置续收（五刀① 起计入弃置界——陈值洪泛同 loud）。
        let hit = (0..TCP_PIPE_K).find(|&i| win[i].as_ref().map(|o| o.seq) == r.seq);
        let Some(i) = hit else {
            foreign += 1;
            if foreign > FOREIGN_DROP_MAX {
                return bail(sent, crate::EIO);
            }
            continue;
        };
        match r.st {
            ndp::ST_OK => {
                let Some(op) = win[i].as_mut() else { continue };
                if r.data.len() != op.len {
                    // 回显短斤/超长 = 协议破缺（loud 错误面）。
                    return bail(sent, crate::EIO);
                }
                op.echo[..op.len].copy_from_slice(r.data);
                op.got = op.len;
                op.busy_since = 0;
            }
            ndp::ST_BUSY => {
                // 环/op 容量背压：同 seq 退避重发（1s 节拍，收割帽内）。
                // F87 liveness 帽：自首次 BUSY 起 60s 仍持续背压（vnet 侧
                // ops 环占死/对抗形——TIMEOUT 终局被占死吞掉）→ bail，send
                // 不再无限退避。
                let Some(op) = win[i].as_mut() else { continue };
                let t = now();
                if op.busy_since == 0 {
                    // now() 读失败返 0 与哨兵同值——t.max(1) 消除首样本
                    // 恰采 0 时的记账顺延面（F107；持续读失败形态由收割
                    // 帽快速 bail 兜底，无 liveness 洞）。
                    op.busy_since = t.max(1);
                } else if t >= op.busy_since + BUSY_CAP_NS {
                    return bail(sent, crate::EIO);
                }
                op.retry_dl = t + POLL_TICK_NS;
                continue;
            }
            st => {
                // TIMEOUT/BAD 等 op 级错误。
                return bail(sent, sock_rpc_errno(SockOp::Send, st));
            }
        }
        // —— 队头收割入槽（按 op 序——回显流序不变式）。——
        while inflight > 0 {
            let done = match &win[head] {
                Some(op) => op.got == op.len && op.retry_dl == 0,
                None => false,
            };
            if !done {
                break;
            }
            let op = win[head].take().unwrap();
            res_append(slot, &op.echo[..op.len]);
            head = (head + 1) % TCP_PIPE_K;
            inflight -= 1;
        }
    }
    sent as ssize_t
}

/// Client 臂 K 窗 seq 发号器（滚动 u8；单线程 static mut 直写——本模块
/// 单线程 ABI 口径）。回绕距 256 ≫ K=2：同 seq 复用需 256 op 间隔，迟到
/// 应答窗口（vnet 内帽 60s）内结构性无碰撞。
fn seq_next() -> u8 {
    // SAFETY: 单线程读写。
    static mut SEQ: u8 = 0;
    unsafe {
        SEQ = SEQ.wrapping_add(1);
        SEQ
    }
}

/// `recv(2)` / read 链尾（sock_recv）：按 kind 分形——
/// - Client：槽缓冲（send 的回显随行）优先；空 → EAGAIN（v1 客户端无
///   独立收面——session-0 无 POLL/DRAIN，阻塞档同返：文档化取舍）；
/// - Server：POLL 节拍 + DRAIN 收割——CLOSED 旗 = 余量终 drain 后 EOF
///   （0）；NOSESSION（会话已收口）= EOF（0）；非阻塞空队 → EAGAIN；
/// - Unconnected/Listen → EPIPE。
pub(crate) fn sock_recv(fd: c_int, buf: *mut u8, n: usize) -> ssize_t {
    let e = match sock_entry(fd) {
        Some(e) => e,
        None => return err_ret(crate::EBADF) as ssize_t,
    };
    if n == 0 {
        return 0; // POSIX recv 零长读 = 0（不触 RPC）
    }
    let slot = slot_of(fd);
    match e.kind {
        SockKind::Unconnected | SockKind::Listen => err_ret(crate::EPIPE) as ssize_t,
        SockKind::Client => {
            // SAFETY: 调用方缓冲前 n 字节可写（libc 契约）。
            let out = unsafe { core::slice::from_raw_parts_mut(buf, n) };
            let c = res_take(slot, out);
            if c > 0 {
                return c as ssize_t;
            }
            err_ret(crate::EAGAIN) as ssize_t
        }
        SockKind::Server => {
            // 槽缓冲优先（上一轮 DRAIN 短读余量）。
            // SAFETY: 同上。
            let out = unsafe { core::slice::from_raw_parts_mut(buf, n) };
            let c = res_take(slot, out);
            if c > 0 {
                return c as ssize_t;
            }
            loop {
                match rpc_poll(e.sid) {
                    Ok(PollState::Gone) => return 0, // 会话已收口 = EOF
                    Ok(PollState::Live { flags, rbytes }) => {
                        if flags & ndp::POLL_FLAG_CLOSED != 0 {
                            if rbytes > 0 {
                                match drain_once(e.sid, slot, out) {
                                    Ok(k) => return k as ssize_t,
                                    Err(errno) => return err_ret(errno) as ssize_t,
                                }
                            }
                            return 0; // 对端收口无余量 = EOF
                        }
                        if rbytes > 0 {
                            match drain_once(e.sid, slot, out) {
                                Ok(k) => return k as ssize_t,
                                Err(errno) => return err_ret(errno) as ssize_t,
                            }
                        }
                        if e.nonblock {
                            return err_ret(crate::EAGAIN) as ssize_t;
                        }
                        sleep_tick();
                    }
                    Err(errno) => return err_ret(errno) as ssize_t,
                }
            }
        }
    }
}

/// 单次 DRAIN 收割：应答数据直拷用户缓冲（截 min），余量入槽缓冲
/// （此时槽缓冲恒空——recv 首步已排空，k ≤ RES_MAX 必容）。
fn drain_once(sid: u32, slot: usize, out: &mut [u8]) -> Result<usize, c_int> {
    let mut tmp = [0u8; RES_MAX];
    let k = rpc_data(SockOp::Recv, ndp::TcpSReq::Drain, sid, CAP_FAST_NS, &mut tmp)?;
    if k == 0 {
        // 串行 RPC 下 POLL 报 rbytes>0 后 DRAIN 零收成 = 协议破缺（loud）。
        return Err(crate::EIO);
    }
    let c = k.min(out.len());
    out[..c].copy_from_slice(&tmp[..c]);
    if k > c {
        res_append(slot, &tmp[c..k]);
    }
    Ok(c)
}

/// `send(2)`：flags 只收 MSG_NOSIGNAL（v1 无信号面 = 语义恒真）；其余
/// MSG_* → EINVAL（MSG_OOB/PEAK 类谎报必致协议错位）。
#[no_mangle]
pub unsafe extern "C" fn send(fd: c_int, buf: *const c_void, n: usize, flags: c_int) -> ssize_t {
    const MSG_NOSIGNAL: c_int = 0x4000; // Linux/musl 同值
    if flags & !MSG_NOSIGNAL != 0 {
        return err_ret(crate::EINVAL) as ssize_t;
    }
    sock_send(fd, buf as *const u8, n)
}

/// `recv(2)`：flags v1 零容忍（MSG_PEEK/DONTWAIT 谎报必致消费面错位）。
#[no_mangle]
pub unsafe extern "C" fn recv(fd: c_int, buf: *mut c_void, n: usize, flags: c_int) -> ssize_t {
    if flags != 0 {
        return err_ret(crate::EINVAL) as ssize_t;
    }
    sock_recv(fd, buf as *mut u8, n)
}

/// close 链尾（io.rs close 分派）：摘表 + 槽缓冲清 + best-effort CLOSE
/// RPC（Client = legacy 恰长形、Listen/Server = 命名形；netd 缺席跳过、
/// RPC 失败不恋战——vnet close_sid 帽到自释槽，POSIX close 恒收口本地
/// fd 面）。返回恒 0。
pub(crate) fn sock_close(fd: c_int) -> c_int {
    // SAFETY: take = 摘表（单线程）。
    let e = match unsafe { SOCKETS.take(fd) } {
        Some(e) => e,
        None => return err_ret(crate::EBADF),
    };
    res_clear(slot_of(fd));
    if e.kind != SockKind::Unconnected && crate::io::netd_h() != 0 {
        let _ = rpc_st(SockOp::Close, ndp::TcpSReq::Close, e.sid, CAP_SLOW_NS);
    }
    0
}

/// poll 链尾（stubs.rs poll 的 socket 项）：单 fd 快照 → revents。
/// Listen fd 恒 0（accept 就绪不建模——wire 无此面，v1 文档化）；Client
/// = 槽缓冲就绪 POLLIN + 恒 POLLOUT（RPC 发送面常开）；Server = POLL
/// 快照经 `poll_revents`（fsclient 钉板）；Gone → POLLIN|POLLHUP；链断
/// → POLLERR（恒报位）。
pub(crate) fn sock_poll_one(fd: c_int, events: i16) -> i16 {
    let e = match sock_entry(fd) {
        Some(e) => e,
        None => return 0,
    };
    let always = events | POLLERR | POLLHUP | berry_fsclient::POLLNVAL;
    match e.kind {
        SockKind::Listen => 0,
        SockKind::Unconnected => POLLOUT & always,
        SockKind::Client => {
            let mut r = POLLOUT;
            if res_pending(slot_of(fd)) > 0 {
                r |= POLLIN;
            }
            r & always
        }
        SockKind::Server => match rpc_poll(e.sid) {
            Ok(PollState::Live { flags, rbytes }) => poll_revents(flags, rbytes, events),
            Ok(PollState::Gone) => (POLLIN | POLLHUP) & always,
            Err(_) => POLLERR,
        },
    }
}

// —— epoll 真实现（A4-1，docs/事件循环A4设计草案-v1.0.md §3.1/§3.2——epoll
//    fd 皮 + interest 注册 + wait 三档节拍循环；语义蓝本 berry-poll：
//    LT / poll-first / timeout 三档，对象面 = socket fd 快照）——

/// epoll fd 段归属谓词（io.rs read/write/writev/close 分派面——fd 四段
/// （console 别名/fs/socket/epoll）区间排他，段内空槽由各入口收口）。
pub(crate) fn epoll_valid(fd: c_int) -> bool {
    (EPOLL_FD_BASE..EPOLL_FD_BASE + EPOLL_FD_MAX as c_int).contains(&fd)
}

/// epoll 事件出参结构：**Linux UAPI packed 12B**（events@0 u32 +
/// data@4 u64）——glibc/musl 全 arch 同款 `__attribute__((packed))`；
/// 草案 §3.3「aarch64 自然对齐 16B」预判被首查推翻（§7 沿革修正）。C-2a
/// Stat 布局案同族执法：下方 offset_of 编译期钉板（「同尺寸不同字段序 =
/// 最阴的静默错」）。字段访问只走值存取（packed 字段禁引用）。
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct EpollEvent {
    pub events: u32,
    pub data: u64,
}

const _: () = assert!(core::mem::size_of::<EpollEvent>() == 12);
const _: () = assert!(core::mem::offset_of!(EpollEvent, events) == 0);
const _: () = assert!(core::mem::offset_of!(EpollEvent, data) == 4);

/// epoll 表静态实例（SOCKETS 同款——单线程，lib.rs ABI 口径）。
static mut EPOLLS: EpollTable = EpollTable::new();

/// EpollCtlErr → errno 映射（fsclient 表面错误与 errno 两层——
/// sock_rpc_errno 同先例）。
fn epoll_ctl_errno(e: EpollCtlErr) -> c_int {
    match e {
        EpollCtlErr::BadFd => crate::EBADF,
        EpollCtlErr::OpBad | EpollCtlErr::NotSock | EpollCtlErr::Mask => crate::EINVAL,
        EpollCtlErr::Exists => crate::EEXIST,
        EpollCtlErr::Noent => crate::ENOENT,
        EpollCtlErr::Full => crate::ENOSPC,
    }
}

/// epoll_create1（A4-1 真身）：flags 只认 0——EPOLL_CLOEXEC v1 无 exec
/// 面诚实拒 EINVAL（收下不执法是谎报）；alloc 满表 EMFILE（4 槽 23..=26）。
#[no_mangle]
pub unsafe extern "C" fn epoll_create1(flags: c_int) -> c_int {
    if flags != 0 {
        return err_ret(crate::EINVAL);
    }
    // SAFETY: 单线程 static mut（SOCKETS 同款）。
    match unsafe { EPOLLS.alloc() } {
        Some(fd) => fd,
        None => err_ret(crate::EMFILE),
    }
}

/// epoll_ctl（A4-1 真身）：ADD/MOD/DEL 三 op。ADD/MOD 必须携 event
/// （NULL → EFAULT）；DEL 的 event POSIX 忽略（v1 同）；epfd 域检在
/// event 解引用**前**（EBADF 优先序——4-2「坏 handle 优先」同款）。
/// target 白名单/掩码/撞槽/帽全在表内执法（fsclient host 钉板）。
#[no_mangle]
pub unsafe extern "C" fn epoll_ctl(
    epfd: c_int,
    op: c_int,
    fd: c_int,
    event: *const EpollEvent,
) -> c_int {
    if !epoll_valid(epfd) {
        return err_ret(crate::EBADF);
    }
    let (events, data) = if op == EPOLL_CTL_DEL {
        (0, 0)
    } else if event.is_null() {
        return err_ret(crate::EFAULT);
    } else {
        // SAFETY: libc 契约——非 NULL 即有效 epoll_event；packed 读 =
        // 字段值拷贝（无字段引用）。
        let e = unsafe { &*event };
        (e.events, e.data)
    };
    // SAFETY: 单线程。
    match unsafe { EPOLLS.ctl(epfd, op, fd, events, data) } {
        Ok(()) => 0,
        Err(e) => err_ret(epoll_ctl_errno(e)),
    }
}

/// epoll_wait（A4-1 真身）：快照 + 节拍循环。**LT 语义** = 每轮全量
/// 复报（berry-poll 同款免费——静态 interest 无消费即再报）；**timeout
/// 三档**：0 = 纯快照立即返；-1 = 无帽节拍循环（v1 无跨线程 ctl 唤醒
/// 面，1s 粒度文档化）；>0 = ms→ns deadline 到帽返 0（POSIX 超时 = 0
/// 事件成功返）。快照源 = 逐 interest 项 `sock_poll_one`（EPOLL↔POLL
/// 位数值同源 fsclient const 对拍钉板——i16 revents 直转 u32 零换算）；
/// interest 中 fd 已 close（socket 表不在册）→ 该项报 EPOLLERR|
/// EPOLLHUP（POSIX 近似——真实 epoll 由内核摘 interest，v1 无生命周
/// 期联动，fd 号复用风险 v1 文档化，草案 §3.2）。命中项 events = re
/// 掩码档 + data 原样透传；就绪数超 cap 截断（LT 下轮复报）。
#[no_mangle]
pub unsafe extern "C" fn epoll_wait(
    epfd: c_int,
    events: *mut EpollEvent,
    maxevents: c_int,
    timeout: c_int,
) -> c_int {
    // errno 优先序（Linux 同款）：EBADF → EINVAL → EFAULT。
    if !epoll_valid(epfd) {
        return err_ret(crate::EBADF);
    }
    if maxevents <= 0 {
        return err_ret(crate::EINVAL);
    }
    if events.is_null() {
        return err_ret(crate::EFAULT);
    }
    // SAFETY: 单线程值快照（wait 循环持副本——与表后续变更解耦）。
    let interest = match unsafe { EPOLLS.snapshot(epfd) } {
        Some(i) => i,
        None => return err_ret(crate::EBADF),
    };
    let cap = core::cmp::min(maxevents as usize, berry_fsclient::EPOLL_ITEMS_MAX);
    let deadline = if timeout > 0 {
        Some(now() + timeout as u64 * 1_000_000)
    } else {
        None
    };
    loop {
        let mut n = 0usize;
        for it in interest.iter().flatten() {
            let re: u32 = if sock_valid(it.fd) {
                // 订阅掩码 u32 白名单位（≤0x10）→ i16 无损；re 非负位
                // 域（≤0x3F）→ u32 直转。
                sock_poll_one(it.fd, it.events as i16) as u32
            } else {
                EPOLLERR | EPOLLHUP
            };
            if re != 0 && n < cap {
                // SAFETY: 出参区（libc 契约；n < cap ≤ maxevents 界
                // 内）；packed 写 = 字段值存取（无字段引用）。
                let slot = unsafe { &mut *events.add(n) };
                slot.events = re;
                slot.data = it.data;
                n += 1;
            }
            if n == cap {
                break;
            }
        }
        if n > 0 {
            return n as c_int;
        }
        match timeout {
            0 => return 0,
            -1 => {}
            _ => {
                if now() >= deadline.unwrap_or(u64::MAX) {
                    return 0;
                }
            }
        }
        // 1s 节拍重快照（accept-wait/data-wait 同款底座）。
        sleep_tick();
    }
}

/// close 链尾（io.rs close 的 epoll 段）：释槽 + interest 全清（wait 侧
/// 持值快照，无悬挂面）；不在册（段内空槽）→ EBADF。
pub(crate) fn epoll_close(fd: c_int) -> c_int {
    // SAFETY: 单线程。
    if unsafe { EPOLLS.free(fd) } {
        0
    } else {
        err_ret(crate::EBADF)
    }
}

// —— getaddrinfo/freeaddrinfo 真实现（IP 字面量档，草案 §3.3）——

/// C 字符串 → 字节切片（止于首 NUL；strlen 同 crate 单源）。
unsafe fn cstr<'a>(p: *const c_char) -> &'a [u8] {
    // SAFETY: 调用方保证 NUL 终结（C 契约）。
    unsafe { core::slice::from_raw_parts(p as *const u8, crate::strmem::strlen(p)) }
}

/// service → 端口（host 序）：NULL = 0（不解析端口，sin_port=0）；十
/// 进制（前导零按十进制——strtoul 基 10 口径）、值 ≤65535；空串/非数字/
/// 超域 = None（EAI_SERVICE——v1 无 getservbyname，服务名档拒）。
fn parse_port(service: *const c_char) -> Option<u16> {
    if service.is_null() {
        return Some(0);
    }
    // SAFETY: C 字符串契约。
    let s = unsafe { cstr(service) };
    if s.is_empty() {
        return None;
    }
    let mut v: u32 = 0;
    for &c in s {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v * 10 + u32::from(c - b'0');
        if v > 65535 {
            return None;
        }
    }
    Some(v as u16)
}

/// getaddrinfo（真实现·IP 字面量快路 + DNS 慢路，A4-2）：node 先试点分
/// 四段 IPv4 字面量（musl 语义：字面量自消化不走 DNS——现状零漂移按
/// 构造）；miss 且名字过预检（[`dns_name_ok`]——非法/超长 → EAI_NONAME
/// 纯本地拒零发包）→ netd `OP_DNS_QUERY` RPC（netd gift 缺席 = 静默门
/// EAI_NONAME——拓扑降级同 evs/skt，EFI/无盘路行为与现状零漂移按构造；
/// 应答错码 [`dns_rpc_eai`] 全表——ST_TIMEOUT → EAI_AGAIN）。node NULL
/// → EAI_NONAME（AI_PASSIVE 0.0.0.0 档 v1 不做，std::net 消费面不走此
/// 形，文档化）。hints 域检：family ∈ {AF_INET, 0}（外 → EAI_FAMILY）、
/// socktype ∈ {SOCK_STREAM, 0}（外 → EAI_SERVICE——musl 对不支持
/// socktype 用此码）；AI_* flags v1 忽略（ai_canonname 恒 NULL）。结果 =
/// 单节点链（malloc ai 48B + sa 16B；任一失败释放已分配块 → EAI_MEMORY，
/// 无泄漏路径）。成功 0 且 `*res` 指链头。
#[no_mangle]
pub unsafe extern "C" fn getaddrinfo(
    node: *const c_char,
    service: *const c_char,
    hints: *const AddrInfo,
    res: *mut *mut AddrInfo,
) -> c_int {
    // SAFETY: 出参写点（libc 契约：错误路径也置 NULL）。
    unsafe { *res = core::ptr::null_mut() };
    if node.is_null() {
        return crate::EAI_NONAME;
    }
    if !hints.is_null() {
        // SAFETY: hints 指调用方 AddrInfo（libc 契约）。
        let h = unsafe { &*hints };
        if h.ai_family != 0 && h.ai_family != AF_INET {
            return crate::EAI_FAMILY;
        }
        if h.ai_socktype != 0 && h.ai_socktype != SOCK_STREAM {
            return crate::EAI_SERVICE;
        }
    }
    // SAFETY: C 字符串契约。
    let name = unsafe { cstr(node) };
    let addr = match parse_ipv4(name) {
        Some(a) => a, // 快路命中：字面量自消化零 RPC
        None => {
            // DNS 慢路（A4-2）：预检本地拒 → 静默门 → RPC + 错码全表。
            if !dns_name_ok(name) {
                return crate::EAI_NONAME;
            }
            if crate::io::netd_h() == 0 {
                return crate::EAI_NONAME; // 拓扑降级（同 evs/skt 静默门）
            }
            match dns_query(name) {
                Ok(octets) => u32::from_be_bytes(octets),
                Err(eai) => return eai,
            }
        }
    };
    let port = match parse_port(service) {
        Some(p) => p,
        None => return crate::EAI_SERVICE,
    };
    let ai = crate::alloc::malloc(core::mem::size_of::<AddrInfo>()) as *mut AddrInfo;
    if ai.is_null() {
        return crate::EAI_MEMORY;
    }
    let sa = crate::alloc::malloc(core::mem::size_of::<SockAddrIn>()) as *mut SockAddrIn;
    if sa.is_null() {
        crate::alloc::free(ai as *mut c_void);
        return crate::EAI_MEMORY;
    }
    // SAFETY: malloc 返回块 ≥16B 对齐（alloc.rs 堆口径）——指针字段对齐
    // 达标；两块均为本调用私有（未逃逸），全字段初始化无读未定义。
    unsafe {
        *sa = SockAddrIn {
            sin_family: AF_INET as sa_family_t,
            sin_port: htons(port),
            sin_addr: htonl(addr),
            sin_zero: [0; 8],
        };
        *ai = AddrInfo {
            ai_flags: 0,
            ai_family: AF_INET,
            ai_socktype: SOCK_STREAM,
            ai_protocol: IPPROTO_TCP,
            ai_addrlen: core::mem::size_of::<SockAddrIn>() as u32,
            __pad: 0,
            ai_addr: sa as *mut SockAddr,
            ai_canonname: core::ptr::null_mut(),
            ai_next: core::ptr::null_mut(),
        };
        *res = ai;
    }
    0
}

/// freeaddrinfo（真实现·链释放）：沿 ai_next 逐节点释放——每节点先取
/// next 再 free（UAF 防线）、先 free ai_addr 再 free 本体；free(NULL)
/// no-op（alloc.rs 在册）。NULL 入参 = 空循环。与 getaddrinfo 的 malloc
/// 面严格对偶。
#[no_mangle]
pub unsafe extern "C" fn freeaddrinfo(ai: *mut AddrInfo) {
    let mut p = ai;
    while !p.is_null() {
        // SAFETY: p 指本链节点（getaddrinfo 产物或 NULL——libc 契约）；
        // 先取 next/addr 再释放本节点。
        unsafe {
            let next = (*p).ai_next;
            let addr = (*p).ai_addr;
            crate::alloc::free(addr as *mut c_void);
            crate::alloc::free(p as *mut c_void);
            p = next;
        }
    }
}
