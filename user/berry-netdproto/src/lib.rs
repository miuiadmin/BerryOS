//! berry-netdproto —— 网络服务 RPC 协议纯函数库（P6 A-8，`berry-fsproto`
//! /`berry-blkproto` 先例：「解析器即产品」，零分配、零 unsafe、零依赖，
//! 输入只借用）。
//!
//! ## Overview (for crates.io users)
//!
//! Wire protocol for the [BerryOS](https://github.com/miuiadmin/BerryOS)
//! network-service RPC: strict request/reply byte layouts for DNS and
//! TCP session ops (plus session-suffixed trailing fields). Pure no_std
//! encoder/parser, host-testable; protocol-only — channel/wait semantics
//! live at each end of the wire. Strict exact-length parsing: anything
//! that cannot be encoded cleanly is rejected.
//!
//! Status: experimental (0.1.0) — the byte layouts move with the BerryOS
//! v1 services. Repo docs are mostly zh-CN.
//!
//! 拓扑：vnet（常驻网络驱动服务，`drivers/bins/vnet`）持通道一端（env 块
//! gift[2]，libc `gift2_h()`），svcmgr（A-8c 起消费自验方）持对端——**单对
//! 双向 + 严格串行 RPC**（一请求一应答，队列深 1 用量，无乱序死锁面；
//! fsmgr/vblk 同款约定）。本库只定字节布局与编解码，通道/等待语义在两端
//! 各自实现。
//!
//! **名字面走 inline 不走 VMO**：DNS 查询名 ≤ [`NAME_MAX`] = 64B ≪ channel
//! inline 帽 256B——与 berry-blkproto（512B 扇区超帽走 VMO）相反的取舍，
//! 无数据面即无随行 handle 面。地址应答 4B 同在 inline。
//!
//! 布局（v1，全定宽；DNS 面无多字节数值字段——addr 是 IPv4 点分序四字节原序；
//! TCP 面唯一数值字段 port 是 LE u16）：
//! - DNS 请求 `[op u8][nlen u8][name nlen B]` = 2+nlen（恰长，尾随字节 = 野值
//!   拒；nlen ∈ 1..=[`NAME_MAX`]，0 拒——空名非法定义在协议层不是调用方
//!   约定）；应答 `[st u8][addr 4B]` = [`REPLY_LEN`] 5 B（恰长，尾随拒）
//! - TCP 三 op（T-d，`docs/TCP栈设计草案.md` §8）：OPEN `[op][port u16 LE]`
//!   3B（port 0 拒）/ ROUNDTRIP `[op][dlen u8][data]` = 2+dlen（dlen ∈
//!   1..=[`TCP_DATA_MAX`] = berry-tcp MSS 通告值——对端遵守我方通告，回显
//!   单段不超帽）/ CLOSE `[op]` 1B；应答按形状分两形：**1B `[st]`**（OPEN/
//!   CLOSE 应答 + 一切非 OK 错误应答）与 **`[st][dlen][data]`**（仅
//!   ROUNDTRIP 成功，回传服务端**真实收到的字节**——对拍归 client，驱动不
//!   知道期望答案，DNS「addr 该是多少归 svcmgr」同款分工）。单会话独占：
//!   会话已被占用时 OPEN → [`ST_BUSY`]。
//! - TCP 会话化面（A2-2，`docs/事件循环A2设计草案.md` §3.2）：session 字段
//!   以**尾随 u32 LE** 携带——session 0 = 默认会话（v1 字节逐字节相同，
//!   现行 op 序零漂移），1..=[`TCP_SESSIONS_MAX`] = 命名会话（v1 形 +4B
//!   尾随）；新 op LISTEN/ACCEPT/POLL 无 v1 形各自恰长执法；应答 sid 形
//!   `[st][sid u32 LE]` 与 poll 形 `[st][flags u8][rbytes u16 LE]` 仅新 op
//!   使用（Open/Roundtrip/Close 会话化应答复用 v1 两形）。
//!
//! **应答烘焙线不变量**：未知 st 拒；DNS 面**非 OK 状态必须携带全零 addr**、
//! TCP 面**非 OK 状态必须不携 data**（编码/解析两侧对称执法——线上不存在
//! 「超时了但带数据」的歧义形态，消费方无需防御性判断）。
//!
//! 错误面（截断/野值/尾随/未知码/空名/越帽名）host 单测全盖——「AI 测试友
//! 好化」：协议正确性不依赖 QEMU 就能验证。编码器 None 拒绝契约：编不出
//! 就拒绝、不截断（含空 out 切片——`berry-fsproto` 对抗审查实抓的同族坑，
//! day-one 盖）。

#![cfg_attr(not(test), no_std)]

/// DNS A 查询：name → 服务端发包解析，应答 addr。
pub const OP_DNS_QUERY: u8 = 1;
/// TCP 主动打开（T-d）：port → 服务端握手至 Established，应答 1B st。
pub const OP_TCP_OPEN: u8 = 2;
/// TCP 会话往返：data → 服务端发一段收齐等长回显，应答 `[st][dlen][data]`。
pub const OP_TCP_ROUNDTRIP: u8 = 3;
/// TCP 主动关闭：服务端 FIN→TIME_WAIT→Closed 全路径，应答 1B st。
pub const OP_TCP_CLOSE: u8 = 4;

/// 应答状态：成功（addr = 解析出的 IPv4 地址）。
pub const ST_OK: u8 = 0;
/// 应答状态：请求坏（解析失败 / 名字面非法 / 收到的应答帧解析失败）。
pub const ST_BAD: u8 = 1;
/// 应答状态：查询超时（服务端整体帽内未收到匹配应答——重发与否由 client
/// 决定，服务端不自动重试）。v1 无请求序号：树内 client（svcmgr dns_rpc）
/// 的防线是**超帽不重发**（迟到应答滞留队列无害——不再发下一请求）；后续
/// 重试型 client 须自实现会话毒化或请求序号（fsproto 族 FS_SESSION_OK
/// 同款），否则迟到应答错配下一请求 = 无告警错答案。
pub const ST_TIMEOUT: u8 = 2;
/// 应答状态：会话被占用（仅 TCP 面——单会话独占，OPEN 撞在席会话时回；
/// DNS 无会话语义，DNS 面编码器不收此值）。
pub const ST_BUSY: u8 = 3;

/// 名字长度帽（字节）。DNS wire 名 ≤255B（berry-netproto 执法），64 是
/// 服务面足够且请求帧 2+64=66B ≪ inline 帽 256B 的安全子集。
pub const NAME_MAX: usize = 64;

/// TCP 单段数据帽（字节）= berry-tcp MSS 通告值（对端遵守我方通告，回显
/// 单段不超帽）。
pub const TCP_DATA_MAX: usize = 128;

/// 请求线上长度上界（DNS 2+NAME_MAX 与 TCP ROUNDTRIP 2+TCP_DATA_MAX 取大）。
pub const REQ_LEN_MAX: usize = 2 + TCP_DATA_MAX;

/// TCP 应答线上长度上界（`[st][dlen][TCP_DATA_MAX]`）。
pub const TCP_REP_MAX: usize = 2 + TCP_DATA_MAX;

/// 应答线上长度。
pub const REPLY_LEN: usize = 5;

/// 请求（解析产物；name 借用原缓冲）。
#[derive(Debug, PartialEq, Eq)]
pub enum Req<'a> {
    /// DNS A 查询。
    DnsQuery { name: &'a [u8] },
}

/// 应答（解析产物）。
#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    pub st: u8,
    /// IPv4 地址点分序四字节（[127,0,0,1] = 127.0.0.1）；非 OK 恒全零。
    pub addr: [u8; 4],
}

/// 编码请求（恰 2+name.len() 字节；坏 op / 空名 / 越帽名 / out 太短 →
/// None，不截断）。
pub fn encode_request(out: &mut [u8], op: u8, name: &[u8]) -> Option<usize> {
    if op != OP_DNS_QUERY {
        return None;
    }
    if name.is_empty() || name.len() > NAME_MAX {
        return None;
    }
    let total = 2 + name.len();
    let m = out.get_mut(..total)?;
    m[0] = op;
    m[1] = name.len() as u8;
    m[2..].copy_from_slice(name);
    Some(total)
}

/// 解析请求：恰长执法（尾随字节 = 野值拒）；坏 op / nlen=0 / nlen 越帽 /
/// nlen 与 buf 长度不符 → None。
pub fn parse_request(buf: &[u8]) -> Option<Req<'_>> {
    let h = buf.get(..2)?;
    let op = h[0];
    let nlen = h[1] as usize;
    if op != OP_DNS_QUERY {
        return None;
    }
    if nlen == 0 || nlen > NAME_MAX {
        return None;
    }
    let name = buf.get(2..2 + nlen)?;
    if buf.len() != 2 + nlen {
        return None; // 尾随字节 = 野值
    }
    Some(Req::DnsQuery { name })
}

/// 编码应答（恰 [`REPLY_LEN`] 字节）。烘焙线不变量：未知 st → None；
/// **非 OK 状态携非零 addr → None**（调用方传零区即可，库执法不靠自觉）；
/// out 太短 → None。
pub fn encode_reply(out: &mut [u8], st: u8, addr: [u8; 4]) -> Option<usize> {
    if st != ST_OK && st != ST_BAD && st != ST_TIMEOUT {
        return None;
    }
    if st != ST_OK && addr != [0u8; 4] {
        return None;
    }
    let m = out.get_mut(..REPLY_LEN)?;
    m[0] = st;
    m[1..5].copy_from_slice(&addr);
    Some(REPLY_LEN)
}

/// 解析应答：恰长执法（尾随拒）；未知 st / 非 OK 携非零 addr → None
/// （与 [`encode_reply`] 对称——线上不变量双侧钉死）。
pub fn parse_reply(buf: &[u8]) -> Option<Reply> {
    let m = buf.get(..REPLY_LEN)?;
    if buf.len() != REPLY_LEN {
        return None; // 尾随字节 = 野值
    }
    let st = m[0];
    let addr: [u8; 4] = m[1..5].try_into().ok()?;
    if st != ST_OK && st != ST_BAD && st != ST_TIMEOUT {
        return None;
    }
    if st != ST_OK && addr != [0u8; 4] {
        return None;
    }
    Some(Reply { st, addr })
}

/// TCP 请求（T-d 会话相位；data 借用原缓冲）。
#[derive(Debug, PartialEq, Eq)]
pub enum TcpReq<'a> {
    /// 主动打开（port 0 拒——协议层定义，非调用方约定）。
    Open { port: u16 },
    /// 会话上发一段收等长回显（data 空/越帽拒）。
    Roundtrip { data: &'a [u8] },
    /// 主动关闭。
    Close,
}

/// TCP 应答（解析产物；data 借用原缓冲——回显真实字节，对拍归 client）。
#[derive(Debug, PartialEq, Eq)]
pub struct TcpReply<'a> {
    pub st: u8,
    /// 仅 ROUNDTRIP 成功非空；OPEN/CLOSE 应答与一切非 OK 应答恒空。
    pub data: &'a [u8],
}

/// TCP 面 st 合法域（含 [`ST_BUSY`]；DNS 面编码器不收 BUSY——无会话语义）。
/// NODEV（A3-3）：vnet deviceless serve 对 OPEN 也答此码（无 NIC 拓扑的
/// 确定性跳过信号——LISTEN 域先例 A2-3 扩面；既有消费者答码零漂移按构造）。
const fn tcp_st_known(st: u8) -> bool {
    st == ST_OK || st == ST_BAD || st == ST_TIMEOUT || st == ST_BUSY || st == ST_NODEV
}

/// 编码 TCP 请求（None 拒绝契约同 DNS 对：port 0 / data 空或越帽 / out 太短
/// → None，不截断）。
pub fn encode_tcp_request(out: &mut [u8], req: TcpReq<'_>) -> Option<usize> {
    match req {
        TcpReq::Open { port } => {
            if port == 0 {
                return None;
            }
            let m = out.get_mut(..3)?;
            m[0] = OP_TCP_OPEN;
            m[1..3].copy_from_slice(&port.to_le_bytes());
            Some(3)
        }
        TcpReq::Roundtrip { data } => {
            if data.is_empty() || data.len() > TCP_DATA_MAX {
                return None;
            }
            let total = 2 + data.len();
            let m = out.get_mut(..total)?;
            m[0] = OP_TCP_ROUNDTRIP;
            m[1] = data.len() as u8;
            m[2..].copy_from_slice(data);
            Some(total)
        }
        TcpReq::Close => {
            let m = out.get_mut(..1)?;
            m[0] = OP_TCP_CLOSE;
            Some(1)
        }
    }
}

/// 解析 TCP 请求：按 op 分形各自恰长执法（尾随/截断 = 野值拒）；坏 op /
/// port 0 / dlen 0 或越帽 / dlen 与 buf 长度不符 → None。
pub fn parse_tcp_request(buf: &[u8]) -> Option<TcpReq<'_>> {
    match *buf.first()? {
        OP_TCP_OPEN => {
            if buf.len() != 3 {
                return None;
            }
            let port = u16::from_le_bytes([buf[1], buf[2]]);
            if port == 0 {
                return None;
            }
            Some(TcpReq::Open { port })
        }
        OP_TCP_ROUNDTRIP => {
            let dlen = *buf.get(1)? as usize;
            if dlen == 0 || dlen > TCP_DATA_MAX {
                return None;
            }
            if buf.len() != 2 + dlen {
                return None; // 尾随/截断一并拒
            }
            Some(TcpReq::Roundtrip { data: buf.get(2..)? })
        }
        OP_TCP_CLOSE => {
            if buf.len() != 1 {
                return None;
            }
            Some(TcpReq::Close)
        }
        _ => None,
    }
}

/// 编码 TCP 应答。形状按携数据与否分两形：空 data（OPEN/CLOSE 应答 + 一切
/// 非 OK）= 1B `[st]`；携 data（仅 OK）= `[st][dlen][data]`。烘焙线不变量：
/// 未知 st → None；**非 OK 状态携 data → None**（与 DNS addr 烘焙线同族——
/// 线上不存在「超时了但带回显」的歧义形态）；data 越帽 / out 太短 → None。
pub fn encode_tcp_reply(out: &mut [u8], st: u8, data: &[u8]) -> Option<usize> {
    if !tcp_st_known(st) {
        return None;
    }
    if st != ST_OK && !data.is_empty() {
        return None;
    }
    if data.is_empty() {
        let m = out.get_mut(..1)?;
        m[0] = st;
        Some(1)
    } else {
        if data.len() > TCP_DATA_MAX {
            return None;
        }
        let total = 2 + data.len();
        let m = out.get_mut(..total)?;
        m[0] = st;
        m[1] = data.len() as u8;
        m[2..].copy_from_slice(data);
        Some(total)
    }
}

/// 解析 TCP 应答：1B `[st]` 形（一切 st）或 `[st][dlen][data]` 形（仅 OK——
/// 非 OK 携 data = 烘焙线违约拒）；dlen 0（2B 形非法）/ 越帽 / 与 buf 长度
/// 不符 / 未知 st → None。
pub fn parse_tcp_reply(buf: &[u8]) -> Option<TcpReply<'_>> {
    let st = *buf.first()?;
    if !tcp_st_known(st) {
        return None;
    }
    if buf.len() == 1 {
        return Some(TcpReply { st, data: &[] });
    }
    let dlen = *buf.get(1)? as usize;
    if dlen == 0 || dlen > TCP_DATA_MAX {
        return None;
    }
    if buf.len() != 2 + dlen {
        return None; // 尾随/截断一并拒
    }
    if st != ST_OK {
        return None; // 烘焙线：非 OK 恒不携 data
    }
    Some(TcpReply { st, data: buf.get(2..)? })
}

// ---------- TCP 会话化面（A2-2 段①：协议层）----------
//
// `docs/事件循环A2设计草案.md` §3.2——单会话 ST_BUSY 升会话表。**零漂移轴**：
// session 字段以尾随 u32 LE 携带，session 0 = 默认会话编码与 v1 字节**逐字节
// 相同**（无尾随——`tcp_s_session0_bytes_identical_to_legacy` 钉板执法），
// 现行 TCPS/vnet 消费链零改动即兼容；session ∈ 1..=[`TCP_SESSIONS_MAX`]
// = v1 形 + 4B 尾随。尾随 sid 域执法：0（该用 v1 形——规范形单一）与越帽
// 均野值拒。
//
// 新 op 无 v1 形（恰长直接定形）：LISTEN 请求 `[op][port u16 LE]` 3B（sid
// 由服务端分配经应答回传——请求面无既存会话上下文，encode 收 session≠0 =
// 野值拒）；ACCEPT 请求 `[op][listen_sid u32 LE]` 5B（会话即 listen 会话，
// 0 拒）；POLL 请求 `[op]` 1B（默认会话）或 `[op][sid u32 LE]` 5B（Open
// 双形同款）；DRAIN 请求（A2-3）`[op][sid u32 LE]` 5B（数据会话收割，0 拒
// ——默认会话的收割走 legacy ROUNDTRIP 面）。
//
// 应答：Open/Roundtrip/Close（含 sessioned 形）**复用 v1 两形零改动**
// （encode_tcp_reply/parse_tcp_reply——会话不在应答面回显，严格串行 RPC
// 下调用方自知会话）；LISTEN/ACCEPT 应答 `[st][sid u32 LE]` = 5B 仅 OK（
// sid ∈ 1..=MAX——**成功必携 sid、非 OK 恒不携**，DNS addr 烘焙线同族）；
// POLL 应答 `[st][flags u8][rbytes u16 LE]` = 4B 仅 OK（快照查询无
// TIMEOUT/EAGAIN——「无就绪」是 OK 应答的合法内容不是错误）；DRAIN 应答
// （A2-3）复用 v1 两形（ST_OK 携收割字节；零收成 = 1B OK 形——与 POLL
// 「无就绪是合法 OK」同族）。

/// TCP 监听（A2-2）：port → 服务端建 listen 会话，应答 sid 形（服务端分配）。
pub const OP_TCP_LISTEN: u8 = 5;
/// TCP 收连接（A2-2，非阻塞）：listen 会话 → 取一条挂起连接为数据会话，
/// 无挂起回 [`ST_EAGAIN`]（事件循环轮询语义——不阻塞等待）。
pub const OP_TCP_ACCEPT: u8 = 6;
/// TCP 就绪快照（A2-2）：会话 → flags（可写/断开）+ 可读字节数。
pub const OP_TCP_POLL: u8 = 7;
/// TCP 收割接收缓冲（A2-3）：数据会话 → 应答携被收割字节（**v1 应答两形
/// 复用**——ST_OK + data；零收成 = 1B OK 形）。POLL 报 rbytes 后的读半边
/// ——拉模型数据面，不推进发送臂。
pub const OP_TCP_DRAIN: u8 = 8;

/// 应答状态：非阻塞无就绪（仅 ACCEPT 面——无挂起连接，client 续轮询）。
pub const ST_EAGAIN: u8 = 4;
/// 应答状态：sid 不在会话表（sessioned op 引用不存在/已关闭的会话——client
/// 用法错，区别于 ST_BAD 的帧坏）。
pub const ST_NOSESSION: u8 = 5;
/// 应答状态：会话表满（LISTEN/ACCEPT 造会话时无空槽——client 退避或收摊）。
pub const ST_FULL: u8 = 6;
/// 应答状态：net 设备缺席（A2-3，仅 LISTEN 域收——vnet deviceless serve 的
/// 拓扑信号：MMIO 窗在而 netdev 缺席（B4 掉电相拓扑），vnet 不 trap 改答
/// 此码，bsh EVS 见之静默跳过。其余 op 域不收——deviceless 端对它们答
/// 域内 [`ST_BAD`]，消费者按既有分诊面降级）。
pub const ST_NODEV: u8 = 7;

/// 会话表容量帽（listen + 数据会话共池；双侧同步——vnet 段②按此定表）。
/// echo server 串行验收 2 连接 + listen + 余量。
pub const TCP_SESSIONS_MAX: u32 = 8;

/// POLL flags：可写（对端窗口未满——发送面无积压）。
pub const POLL_FLAG_WRITABLE: u8 = 0x01;
/// POLL flags：断开（对端 FIN/RST——连接终态，readable 面收尾后即死槽）。
pub const POLL_FLAG_CLOSED: u8 = 0x02;

/// 会话化请求线上长度上界（v1 REQ_LEN_MAX + 4B 尾随 sid）。
pub const REQ_S_LEN_MAX: usize = REQ_LEN_MAX + 4;
/// LISTEN/ACCEPT 应答线上长度（`[st][sid u32 LE]`）。
pub const SID_REP_LEN: usize = 5;
/// POLL 应答线上长度（`[st][flags u8][rbytes u16 LE]`）。
pub const POLL_REP_LEN: usize = 4;

/// 会话化 TCP 请求（七 op 全集；data 借用原缓冲）。
#[derive(Debug, PartialEq, Eq)]
pub enum TcpSReq<'a> {
    /// 主动打开（默认会话 session=0 或命名数据会话）。
    Open { port: u16 },
    /// 会话往返（同上双形）。
    Roundtrip { data: &'a [u8] },
    /// 主动关闭（同上双形）。
    Close,
    /// 建监听（port 0 拒；session 恒 0——sid 服务端分配经应答回传）。
    Listen { port: u16 },
    /// 取挂起连接（session = listen 会话 sid ∈ 1..=MAX）。
    Accept,
    /// 就绪快照（session 0 = 默认会话，或命名会话 sid）。
    Poll,
    /// 收割接收缓冲（session = 数据会话 sid ∈ 1..=MAX，0 拒——默认会话的
    /// 收割走 legacy ROUNDTRIP 面；应答复用 v1 两形）。
    Drain,
}

impl<'a> From<TcpReq<'a>> for TcpSReq<'a> {
    fn from(r: TcpReq<'a>) -> Self {
        match r {
            TcpReq::Open { port } => TcpSReq::Open { port },
            TcpReq::Roundtrip { data } => TcpSReq::Roundtrip { data },
            TcpReq::Close => TcpSReq::Close,
        }
    }
}

/// 编码会话化请求。session 0 的 Open/Roundtrip/Close 走 v1 编码器原字节
/// （单源复用——校验与字节面不二次实现）；session ∈ 1..=MAX 追加 4B 尾随。
/// None 契约：session 越帽 / LISTEN 收 session≠0 / ACCEPT 收 session=0 /
/// v1 面一切野值（port 0、data 空或越帽）/ out 太短。
pub fn encode_tcp_s_request(out: &mut [u8], req: TcpSReq<'_>, session: u32) -> Option<usize> {
    if session > TCP_SESSIONS_MAX {
        return None;
    }
    match req {
        TcpSReq::Open { port } => {
            let base = encode_tcp_request(out, TcpReq::Open { port })?;
            append_session(out, base, session)
        }
        TcpSReq::Roundtrip { data } => {
            let base = encode_tcp_request(out, TcpReq::Roundtrip { data })?;
            append_session(out, base, session)
        }
        TcpSReq::Close => {
            let base = encode_tcp_request(out, TcpReq::Close)?;
            append_session(out, base, session)
        }
        TcpSReq::Listen { port } => {
            if session != 0 {
                return None; // 无会话上下文——sid 归服务端分配
            }
            if port == 0 {
                return None;
            }
            let m = out.get_mut(..3)?;
            m[0] = OP_TCP_LISTEN;
            m[1..3].copy_from_slice(&port.to_le_bytes());
            Some(3)
        }
        TcpSReq::Accept => {
            if session == 0 {
                return None; // listen 会话必须显式
            }
            let m = out.get_mut(..5)?;
            m[0] = OP_TCP_ACCEPT;
            m[1..5].copy_from_slice(&session.to_le_bytes());
            Some(5)
        }
        TcpSReq::Poll => {
            if session == 0 {
                let m = out.get_mut(..1)?;
                m[0] = OP_TCP_POLL;
                return Some(1);
            }
            let m = out.get_mut(..5)?;
            m[0] = OP_TCP_POLL;
            m[1..5].copy_from_slice(&session.to_le_bytes());
            Some(5)
        }
        TcpSReq::Drain => {
            if session == 0 {
                return None; // 数据会话必须显式（默认会话收割走 legacy 面）
            }
            let m = out.get_mut(..5)?;
            m[0] = OP_TCP_DRAIN;
            m[1..5].copy_from_slice(&session.to_le_bytes());
            Some(5)
        }
    }
}

/// v1 形后追加 4B 尾随 session（session=0 不入此——调用点已前置分流）。
fn append_session(out: &mut [u8], base: usize, session: u32) -> Option<usize> {
    if session == 0 {
        return Some(base);
    }
    let tail = out.get_mut(base..base + 4)?;
    tail.copy_from_slice(&session.to_le_bytes());
    Some(base + 4)
}

/// 解析会话化请求：v1 恰长形 → session 0（v1 解析器原样复用）；+4B 尾随
/// 形 → 尾随 sid ∈ 1..=MAX（0 = 野值——该用 v1 形；越帽 = 野值）+ 去尾
/// 后 v1 解析必须过。新 op 各自恰长执法（同 v1 面全部尾随/截断拒）。
pub fn parse_tcp_s_request(buf: &[u8]) -> Option<(TcpSReq<'_>, u32)> {
    match *buf.first()? {
        OP_TCP_OPEN | OP_TCP_ROUNDTRIP | OP_TCP_CLOSE => {
            if let Some(r) = parse_tcp_request(buf) {
                return Some((r.into(), 0)); // v1 恰长形
            }
            let n = buf.len().checked_sub(4)?;
            let sid = u32::from_le_bytes(buf.get(n..)?.try_into().ok()?);
            if sid == 0 || sid > TCP_SESSIONS_MAX {
                return None; // 尾随 sid 野值
            }
            let r = parse_tcp_request(buf.get(..n)?)?;
            Some((r.into(), sid))
        }
        OP_TCP_LISTEN => {
            if buf.len() != 3 {
                return None;
            }
            let port = u16::from_le_bytes([buf[1], buf[2]]);
            if port == 0 {
                return None;
            }
            Some((TcpSReq::Listen { port }, 0))
        }
        OP_TCP_ACCEPT => {
            if buf.len() != 5 {
                return None;
            }
            let sid = u32::from_le_bytes(buf[1..5].try_into().ok()?);
            if sid == 0 || sid > TCP_SESSIONS_MAX {
                return None;
            }
            Some((TcpSReq::Accept, sid))
        }
        OP_TCP_POLL => {
            if buf.len() == 1 {
                return Some((TcpSReq::Poll, 0)); // 默认会话
            }
            if buf.len() != 5 {
                return None;
            }
            let sid = u32::from_le_bytes(buf[1..5].try_into().ok()?);
            if sid == 0 || sid > TCP_SESSIONS_MAX {
                return None;
            }
            Some((TcpSReq::Poll, sid))
        }
        OP_TCP_DRAIN => {
            if buf.len() != 5 {
                return None;
            }
            let sid = u32::from_le_bytes(buf[1..5].try_into().ok()?);
            if sid == 0 || sid > TCP_SESSIONS_MAX {
                return None;
            }
            Some((TcpSReq::Drain, sid))
        }
        _ => None,
    }
}

/// LISTEN/ACCEPT 应答（解析产物；sid ∈ 1..=MAX——非 OK 恒 0）。
#[derive(Debug, PartialEq, Eq)]
pub struct SidRep {
    pub st: u8,
    /// OK = 服务端分配的会话 sid；非 OK 恒 0（烘焙线）。
    pub sid: u32,
}

/// LISTEN 应答 st 域：BAD / FULL / **NODEV**（本地建表无 TIMEOUT；成功携
/// sid。NODEV = vnet deviceless serve——无 NIC 拓扑的确定性跳过信号，A2-3）。
const LISTEN_ST_DOMAIN: [u8; 4] = [ST_OK, ST_BAD, ST_FULL, ST_NODEV];
/// ACCEPT 应答 st 域：BAD / EAGAIN / NOSESSION / FULL（非阻塞无 TIMEOUT）。
const ACCEPT_ST_DOMAIN: [u8; 5] = [ST_OK, ST_BAD, ST_EAGAIN, ST_NOSESSION, ST_FULL];

/// 编码 LISTEN/ACCEPT 应答（共形分域）。烘焙线：非 OK 携 sid → None；OK
/// 携 sid=0 或越帽 → None（成功必携有效 sid——1B OK 形不存在）。
fn encode_sid_reply(out: &mut [u8], st: u8, sid: u32, domain: &[u8]) -> Option<usize> {
    if !domain.contains(&st) {
        return None;
    }
    if st == ST_OK {
        if sid == 0 || sid > TCP_SESSIONS_MAX {
            return None;
        }
        let m = out.get_mut(..SID_REP_LEN)?;
        m[0] = st;
        m[1..5].copy_from_slice(&sid.to_le_bytes());
        Some(SID_REP_LEN)
    } else {
        if sid != 0 {
            return None;
        }
        let m = out.get_mut(..1)?;
        m[0] = st;
        Some(1)
    }
}

/// 解析 LISTEN/ACCEPT 应答：1B 形（非 OK）或 5B sid 形（仅 OK）；1B OK /
/// 非 OK 5B / sid 0 或越帽 / 域外 st → None（与编码侧对称）。
fn parse_sid_reply(buf: &[u8], domain: &[u8]) -> Option<SidRep> {
    let st = *buf.first()?;
    if !domain.contains(&st) {
        return None;
    }
    if buf.len() == 1 {
        if st == ST_OK {
            return None; // 成功必携 sid
        }
        return Some(SidRep { st, sid: 0 });
    }
    if buf.len() != SID_REP_LEN || st != ST_OK {
        return None; // 尾随/截断 + 非 OK 恒不携 sid
    }
    let sid = u32::from_le_bytes(buf[1..5].try_into().ok()?);
    if sid == 0 || sid > TCP_SESSIONS_MAX {
        return None;
    }
    Some(SidRep { st, sid })
}

/// 编码 LISTEN 应答（域见 [`LISTEN_ST_DOMAIN`]）。
pub fn encode_listen_reply(out: &mut [u8], st: u8, sid: u32) -> Option<usize> {
    encode_sid_reply(out, st, sid, &LISTEN_ST_DOMAIN)
}

/// 解析 LISTEN 应答。
pub fn parse_listen_reply(buf: &[u8]) -> Option<SidRep> {
    parse_sid_reply(buf, &LISTEN_ST_DOMAIN)
}

/// 编码 ACCEPT 应答（域见 [`ACCEPT_ST_DOMAIN`]）。
pub fn encode_accept_reply(out: &mut [u8], st: u8, sid: u32) -> Option<usize> {
    encode_sid_reply(out, st, sid, &ACCEPT_ST_DOMAIN)
}

/// 解析 ACCEPT 应答。
pub fn parse_accept_reply(buf: &[u8]) -> Option<SidRep> {
    parse_sid_reply(buf, &ACCEPT_ST_DOMAIN)
}

/// POLL 应答（解析产物；flags 域见 [`POLL_FLAG_WRITABLE`]/
/// [`POLL_FLAG_CLOSED`]——非 OK 恒全零）。
#[derive(Debug, PartialEq, Eq)]
pub struct PollRep {
    pub st: u8,
    pub flags: u8,
    /// 可读字节数（rbytes=0 且 flags 无 CLOSED = 「暂无就绪」的合法 OK）。
    pub rbytes: u16,
}

/// POLL 应答 st 域：BAD / NOSESSION（快照查询非阻塞——无 EAGAIN/TIMEOUT）。
const POLL_ST_DOMAIN: [u8; 3] = [ST_OK, ST_BAD, ST_NOSESSION];

const fn poll_flags_known(flags: u8) -> bool {
    flags & !(POLL_FLAG_WRITABLE | POLL_FLAG_CLOSED) == 0
}

/// 编码 POLL 应答。烘焙线：OK 携未知 flags 位 → None；非 OK 携非零
/// flags/rbytes → None（「坏会话但带就绪」歧义形态不线上存在）。
pub fn encode_poll_reply(out: &mut [u8], st: u8, flags: u8, rbytes: u16) -> Option<usize> {
    if !POLL_ST_DOMAIN.contains(&st) {
        return None;
    }
    if st == ST_OK {
        if !poll_flags_known(flags) {
            return None;
        }
        let m = out.get_mut(..POLL_REP_LEN)?;
        m[0] = st;
        m[1] = flags;
        m[2..4].copy_from_slice(&rbytes.to_le_bytes());
        Some(POLL_REP_LEN)
    } else {
        if flags != 0 || rbytes != 0 {
            return None;
        }
        let m = out.get_mut(..1)?;
        m[0] = st;
        Some(1)
    }
}

/// 解析 POLL 应答：1B 形（非 OK）或 4B 形（仅 OK）；1B OK / 非 OK 4B /
/// 未知 flags 位 / 域外 st → None。
pub fn parse_poll_reply(buf: &[u8]) -> Option<PollRep> {
    let st = *buf.first()?;
    if !POLL_ST_DOMAIN.contains(&st) {
        return None;
    }
    if buf.len() == 1 {
        if st == ST_OK {
            return None; // 成功必携快照
        }
        return Some(PollRep { st, flags: 0, rbytes: 0 });
    }
    if buf.len() != POLL_REP_LEN || st != ST_OK {
        return None;
    }
    let flags = buf[1];
    if !poll_flags_known(flags) {
        return None;
    }
    let rbytes = u16::from_le_bytes([buf[2], buf[3]]);
    Some(PollRep { st, flags, rbytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 请求往返：encode → parse 还原同一名字（借用面）。
    #[test]
    fn request_roundtrip() {
        let mut out = [0u8; REQ_LEN_MAX];
        let n = encode_request(&mut out, OP_DNS_QUERY, b"localhost").unwrap();
        assert_eq!(n, 2 + 9);
        assert_eq!(
            parse_request(&out[..n]),
            Some(Req::DnsQuery { name: b"localhost" })
        );
    }

    /// wire 钉板：请求黄金字节（op/nlen/名字原序——无端序数值字段）。
    #[test]
    fn request_wire_pinned() {
        let mut out = [0u8; REQ_LEN_MAX];
        let n = encode_request(&mut out, OP_DNS_QUERY, b"localhost").unwrap();
        assert_eq!(
            &out[..n],
            &[0x01, 0x09, b'l', b'o', b'c', b'a', b'l', b'h', b'o', b's', b't']
        );
    }

    /// 截断拒：< 2B 无头可读。
    #[test]
    fn request_short_rejected() {
        assert_eq!(parse_request(&[]), None);
        assert_eq!(parse_request(&[OP_DNS_QUERY]), None);
    }

    /// 尾随拒：恰长之外的字节 = 野值。
    #[test]
    fn request_trailing_rejected() {
        let mut out = [0u8; REQ_LEN_MAX];
        let n = encode_request(&mut out, OP_DNS_QUERY, b"ab").unwrap();
        let mut buf = [0u8; 8];
        buf[..n].copy_from_slice(&out[..n]);
        assert_eq!(parse_request(&buf), None);
    }

    /// 坏 op：编码/解析两侧拒。
    #[test]
    fn request_bad_op_rejected() {
        let mut out = [0u8; REQ_LEN_MAX];
        assert_eq!(encode_request(&mut out, 0, b"ab"), None);
        assert_eq!(encode_request(&mut out, 2, b"ab"), None);
        assert_eq!(parse_request(&[0, 2, b'a', b'b']), None);
        assert_eq!(parse_request(&[2, 2, b'a', b'b']), None);
    }

    /// 空名拒：nlen=0 是非法值不是边界值（协议层定义，非调用方约定）。
    #[test]
    fn request_empty_name_rejected() {
        let mut out = [0u8; REQ_LEN_MAX];
        assert_eq!(encode_request(&mut out, OP_DNS_QUERY, b""), None);
        assert_eq!(parse_request(&[OP_DNS_QUERY, 0]), None);
    }

    /// 恰满 64B 名收；65B 拒（编码/解析两侧对称）。
    #[test]
    fn request_name_cap_boundaries() {
        let full = [b'a'; NAME_MAX];
        let mut out = [0u8; REQ_LEN_MAX];
        let n = encode_request(&mut out, OP_DNS_QUERY, &full).unwrap();
        assert_eq!(n, 2 + NAME_MAX); // T-d 起 REQ_LEN_MAX 由 TCP 面抬到 130，DNS 锚 = 2+NAME_MAX
        match parse_request(&out[..n]) {
            Some(Req::DnsQuery { name }) => assert_eq!(name, &full[..]),
            other => panic!("cap-len name must parse: {other:?}"),
        }
        let over = [b'a'; NAME_MAX + 1];
        assert_eq!(encode_request(&mut out, OP_DNS_QUERY, &over), None);
        assert_eq!(parse_request(&[OP_DNS_QUERY, (NAME_MAX + 1) as u8, 0, 0]), None);
    }

    /// out 不足/空 out：None 不截断（含空切片的 None 契约）。
    #[test]
    fn request_out_too_small_rejected() {
        let mut small = [0u8; 3];
        assert_eq!(encode_request(&mut small, OP_DNS_QUERY, b"abcd"), None);
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_request(&mut empty, OP_DNS_QUERY, b"a"), None);
    }

    /// 应答往返：OK + 地址原序还原。
    #[test]
    fn reply_roundtrip() {
        let mut out = [0u8; REPLY_LEN];
        let n = encode_reply(&mut out, ST_OK, [127, 0, 0, 1]).unwrap();
        assert_eq!(n, REPLY_LEN);
        assert_eq!(
            parse_reply(&out),
            Some(Reply { st: ST_OK, addr: [127, 0, 0, 1] })
        );
    }

    /// wire 钉板：应答黄金字节（st 首字节 + addr 点分序原序——编码/解码
    /// 被一致改字段序或 addr 排序时 roundtrip 仍绿，此处炸响；请求侧
    /// request_wire_pinned + blkproto wire_format_pinned 同款）。
    #[test]
    fn reply_wire_pinned() {
        let mut out = [0u8; REPLY_LEN];
        let n = encode_reply(&mut out, ST_OK, [127, 0, 0, 1]).unwrap();
        assert_eq!(n, REPLY_LEN);
        assert_eq!(&out, &[ST_OK, 127, 0, 0, 1]);
    }

    /// 非 OK 状态必须全零 addr：编码/解析两侧对称执法（烘焙线不变量）。
    #[test]
    fn reply_non_ok_zero_addr_invariant() {
        let mut out = [0u8; REPLY_LEN];
        assert_eq!(encode_reply(&mut out, ST_TIMEOUT, [0; 4]), Some(REPLY_LEN));
        assert_eq!(
            parse_reply(&out),
            Some(Reply { st: ST_TIMEOUT, addr: [0; 4] })
        );
        assert_eq!(encode_reply(&mut out, ST_TIMEOUT, [1, 0, 0, 0]), None);
        assert_eq!(parse_reply(&[ST_BAD, 0, 0, 0, 1]), None);
    }

    /// 未知 st：编码/解析两侧拒。
    #[test]
    fn reply_unknown_status_rejected() {
        let mut out = [0u8; REPLY_LEN];
        assert_eq!(encode_reply(&mut out, 3, [0; 4]), None);
        assert_eq!(encode_reply(&mut out, 0xFF, [0; 4]), None);
        assert_eq!(parse_reply(&[3, 0, 0, 0, 0]), None);
    }

    /// 应答尾随拒 + 截断拒。
    #[test]
    fn reply_trailing_and_short_rejected() {
        assert_eq!(parse_reply(&[ST_OK, 1, 2, 3, 4, 0]), None);
        assert_eq!(parse_reply(&[ST_OK, 1, 2, 3]), None);
        assert_eq!(parse_reply(&[]), None);
    }

    /// 应答 out 不足/空 out：None 不截断。
    #[test]
    fn reply_out_too_small_rejected() {
        let mut small = [0u8; 4];
        assert_eq!(encode_reply(&mut small, ST_OK, [127, 0, 0, 1]), None);
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_reply(&mut empty, ST_OK, [0; 4]), None);
    }

    // ---------- TCP 三 op（T-d）----------

    /// 请求往返：三 op 各自 encode → parse 还原（port LE / data 借用面）。
    #[test]
    fn tcp_request_roundtrip() {
        let mut out = [0u8; REQ_LEN_MAX];
        let n = encode_tcp_request(&mut out, TcpReq::Open { port: 18080 }).unwrap();
        assert_eq!(n, 3);
        assert_eq!(parse_tcp_request(&out[..n]), Some(TcpReq::Open { port: 18080 }));

        let n = encode_tcp_request(&mut out, TcpReq::Roundtrip { data: b"abcd" }).unwrap();
        assert_eq!(n, 2 + 4);
        assert_eq!(
            parse_tcp_request(&out[..n]),
            Some(TcpReq::Roundtrip { data: b"abcd" })
        );

        let n = encode_tcp_request(&mut out, TcpReq::Close).unwrap();
        assert_eq!(n, 1);
        assert_eq!(parse_tcp_request(&out[..n]), Some(TcpReq::Close));
    }

    /// wire 钉板：OPEN port 18080 = 0x46A0 → LE A0 46（端序/字段序漂移在
    /// 此炸响；blkproto/netproto wire 钉板同款）。
    #[test]
    fn tcp_request_wire_pinned() {
        let mut out = [0u8; 4];
        let n = encode_tcp_request(&mut out, TcpReq::Open { port: 18080 }).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_OPEN, 0xA0, 0x46]);

        let n = encode_tcp_request(&mut out, TcpReq::Close).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_CLOSE]);

        let n = encode_tcp_request(&mut out, TcpReq::Roundtrip { data: b"xy" }).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_ROUNDTRIP, 2, b'x', b'y']);
    }

    /// 请求尾随/截断拒：三 op 各自恰长之外的字节 = 野值。
    #[test]
    fn tcp_request_trailing_and_short_rejected() {
        assert_eq!(parse_tcp_request(&[]), None);
        assert_eq!(parse_tcp_request(&[OP_TCP_OPEN]), None);
        assert_eq!(parse_tcp_request(&[OP_TCP_OPEN, 0xA0]), None);
        assert_eq!(parse_tcp_request(&[OP_TCP_OPEN, 0xA0, 0x46, 0]), None);
        assert_eq!(parse_tcp_request(&[OP_TCP_CLOSE, 0]), None);
        assert_eq!(parse_tcp_request(&[OP_TCP_ROUNDTRIP, 1, b'a', 0]), None);
        assert_eq!(parse_tcp_request(&[OP_TCP_ROUNDTRIP, 2, b'a']), None); // dlen 与实长不符
        assert_eq!(parse_tcp_request(&[5]), None); // legacy 面未知 op（会话化面 = LISTEN，单字节亦截断拒）
    }

    /// port 0 拒：协议层定义非调用方约定（空名同款）；恰 65535 收。
    #[test]
    fn tcp_port_boundaries() {
        let mut out = [0u8; 3];
        assert_eq!(encode_tcp_request(&mut out, TcpReq::Open { port: 0 }), None);
        assert_eq!(parse_tcp_request(&[OP_TCP_OPEN, 0, 0]), None);
        assert_eq!(
            encode_tcp_request(&mut out, TcpReq::Open { port: u16::MAX }),
            Some(3)
        );
        assert_eq!(
            parse_tcp_request(&out),
            Some(TcpReq::Open { port: u16::MAX })
        );
    }

    /// dlen 帽：恰 128 收 / 0 拒 / 129 拒（编码/解析两侧对称）。
    #[test]
    fn tcp_dlen_cap_boundaries() {
        let full = [7u8; TCP_DATA_MAX];
        let mut out = [0u8; REQ_LEN_MAX];
        let n = encode_tcp_request(&mut out, TcpReq::Roundtrip { data: &full }).unwrap();
        assert_eq!(n, REQ_LEN_MAX);
        match parse_tcp_request(&out[..n]) {
            Some(TcpReq::Roundtrip { data }) => assert_eq!(data, &full[..]),
            other => panic!("cap-len data must parse: {other:?}"),
        }
        let over = [7u8; TCP_DATA_MAX + 1];
        assert_eq!(encode_tcp_request(&mut out, TcpReq::Roundtrip { data: &over }), None);
        assert_eq!(
            parse_tcp_request(&[OP_TCP_ROUNDTRIP, (TCP_DATA_MAX + 1) as u8, 0, 0]),
            None
        );
        assert_eq!(encode_tcp_request(&mut out, TcpReq::Roundtrip { data: b"" }), None);
        assert_eq!(parse_tcp_request(&[OP_TCP_ROUNDTRIP, 0]), None);
    }

    /// TCP 请求 out 不足/空 out：None 不截断（含 Close 的 1B 面）。
    #[test]
    fn tcp_request_out_too_small_rejected() {
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_tcp_request(&mut empty, TcpReq::Close), None);
        assert_eq!(encode_tcp_request(&mut empty, TcpReq::Open { port: 1 }), None);
        let mut small = [0u8; 2];
        assert_eq!(encode_tcp_request(&mut small, TcpReq::Open { port: 1 }), None);
        let mut small = [0u8; 3];
        assert_eq!(encode_tcp_request(&mut small, TcpReq::Roundtrip { data: b"abcd" }), None);
    }

    /// wire 钉板：应答两形黄金字节——1B 非 OK 形（st 单独成帧，无 dlen/
    /// data 字段）与 `[st][dlen][data]` 成功形（字段序 + dlen 位置在此
    /// 炸响；request/DNS reply 钉板同款——roundtrip 闭环对对称漂移
    /// （字段序互换/dlen 错位）不设防）。
    #[test]
    fn tcp_reply_wire_pinned() {
        let mut out = [0u8; TCP_REP_MAX];
        let n = encode_tcp_reply(&mut out, ST_TIMEOUT, b"").unwrap();
        assert_eq!(&out[..n], &[ST_TIMEOUT]);
        let n = encode_tcp_reply(&mut out, ST_BUSY, b"").unwrap();
        assert_eq!(&out[..n], &[ST_BUSY]);
        let n = encode_tcp_reply(&mut out, ST_OK, b"abc").unwrap();
        assert_eq!(&out[..n], &[ST_OK, 3, b'a', b'b', b'c']);
    }

    /// 应答两形往返：1B `[st]`（OPEN/CLOSE/错误）与 `[st][dlen][data]`
    /// （仅 OK）；ST_BUSY 1B 形合法（TCP 面 st 域）。
    #[test]
    fn tcp_reply_roundtrip() {
        let mut out = [0u8; TCP_REP_MAX];
        let n = encode_tcp_reply(&mut out, ST_OK, b"").unwrap();
        assert_eq!(n, 1);
        assert_eq!(parse_tcp_reply(&out[..n]), Some(TcpReply { st: ST_OK, data: &[] }));

        let n = encode_tcp_reply(&mut out, ST_BUSY, b"").unwrap();
        assert_eq!(n, 1);
        assert_eq!(parse_tcp_reply(&out[..n]), Some(TcpReply { st: ST_BUSY, data: &[] }));

        // A3-3：NODEV 入域（deviceless serve 对 OPEN 也答此码——B4 拓扑
        // 真值；1B 形合法、非 OK 恒不携 data 对称面）。
        let n = encode_tcp_reply(&mut out, ST_NODEV, b"").unwrap();
        assert_eq!(n, 1);
        assert_eq!(parse_tcp_reply(&out[..n]), Some(TcpReply { st: ST_NODEV, data: &[] }));
        assert_eq!(encode_tcp_reply(&mut out, ST_NODEV, b"x"), None);

        let n = encode_tcp_reply(&mut out, ST_OK, b"echo!").unwrap();
        assert_eq!(n, 2 + 5);
        assert_eq!(
            parse_tcp_reply(&out[..n]),
            Some(TcpReply { st: ST_OK, data: b"echo!" })
        );
    }

    /// 应答烘焙线：非 OK 携 data 编码拒 + 解析拒（DNS addr 烘焙线同族，
    /// 双侧对称）。
    #[test]
    fn tcp_reply_non_ok_empty_data_invariant() {
        let mut out = [0u8; TCP_REP_MAX];
        assert_eq!(encode_tcp_reply(&mut out, ST_TIMEOUT, b"x"), None);
        assert_eq!(encode_tcp_reply(&mut out, ST_BUSY, b"x"), None);
        assert_eq!(parse_tcp_reply(&[ST_TIMEOUT, 1, b'x']), None);
        assert_eq!(parse_tcp_reply(&[ST_BUSY, 1, b'x']), None);
    }

    /// 应答恰长/截断/2B 形（dlen=0 非法）/未知 st 拒。
    #[test]
    fn tcp_reply_shapes_rejected() {
        assert_eq!(parse_tcp_reply(&[]), None);
        assert_eq!(parse_tcp_reply(&[ST_OK, 2, b'a']), None); // dlen 与实长不符
        assert_eq!(parse_tcp_reply(&[ST_OK, 0]), None); // dlen=0（2B 形非法）
        assert_eq!(parse_tcp_reply(&[ST_OK, 1, b'a', 0]), None); // 尾随
        assert_eq!(parse_tcp_reply(&[ST_OK, TCP_DATA_MAX as u8 + 1, b'a']), None); // dlen 越帽
        assert_eq!(parse_tcp_reply(&[0xFF]), None); // 未知 st
        assert_eq!(encode_tcp_reply(&mut [0u8; 8][..], 0xFF, b""), None);
    }

    /// 应答恰满 128B 帽收 + out 不足/空 out None 不截断。
    #[test]
    fn tcp_reply_cap_and_out_boundaries() {
        let full = [9u8; TCP_DATA_MAX];
        let mut out = [0u8; TCP_REP_MAX];
        let n = encode_tcp_reply(&mut out, ST_OK, &full).unwrap();
        assert_eq!(n, TCP_REP_MAX);
        match parse_tcp_reply(&out[..n]) {
            Some(TcpReply { st, data }) => {
                assert_eq!(st, ST_OK);
                assert_eq!(data, &full[..]);
            }
            other => panic!("cap-len reply must parse: {other:?}"),
        }
        let mut small = [0u8; 3];
        assert_eq!(encode_tcp_reply(&mut small, ST_OK, b"abcd"), None);
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_tcp_reply(&mut empty, ST_OK, b""), None);
    }

    /// DNS 面 st 域不含 BUSY（无会话语义——两对编码器 st 域分面钉板）。
    #[test]
    fn dns_st_domain_excludes_busy() {
        let mut out = [0u8; REPLY_LEN];
        assert_eq!(encode_reply(&mut out, ST_BUSY, [0; 4]), None);
        assert_eq!(parse_reply(&[ST_BUSY, 0, 0, 0, 0]), None);
    }

    // ---------- TCP 会话化面（A2-2 段①）----------

    /// 会话化请求往返：六 op × session {0, 5, TCP_SESSIONS_MAX}——v1 三 op
    /// 双形（0 = v1 原形 / >0 = +4B 尾随）+ 新三 op 各自定形。
    #[test]
    fn tcp_s_request_roundtrip() {
        let mut out = [0u8; REQ_S_LEN_MAX];
        for sid in [0u32, 5, TCP_SESSIONS_MAX] {
            let n = encode_tcp_s_request(&mut out, TcpSReq::Open { port: 18080 }, sid).unwrap();
            assert_eq!(n, 3 + if sid == 0 { 0 } else { 4 });
            assert_eq!(parse_tcp_s_request(&out[..n]), Some((TcpSReq::Open { port: 18080 }, sid)));

            let n = encode_tcp_s_request(&mut out, TcpSReq::Roundtrip { data: b"abcd" }, sid).unwrap();
            assert_eq!(n, 6 + if sid == 0 { 0 } else { 4 });
            assert_eq!(
                parse_tcp_s_request(&out[..n]),
                Some((TcpSReq::Roundtrip { data: b"abcd" }, sid))
            );

            let n = encode_tcp_s_request(&mut out, TcpSReq::Close, sid).unwrap();
            assert_eq!(n, 1 + if sid == 0 { 0 } else { 4 });
            assert_eq!(parse_tcp_s_request(&out[..n]), Some((TcpSReq::Close, sid)));
        }
        let n = encode_tcp_s_request(&mut out, TcpSReq::Listen { port: 18081 }, 0).unwrap();
        assert_eq!(n, 3);
        assert_eq!(parse_tcp_s_request(&out[..n]), Some((TcpSReq::Listen { port: 18081 }, 0)));

        let n = encode_tcp_s_request(&mut out, TcpSReq::Accept, 5).unwrap();
        assert_eq!(n, 5);
        assert_eq!(parse_tcp_s_request(&out[..n]), Some((TcpSReq::Accept, 5)));

        let n = encode_tcp_s_request(&mut out, TcpSReq::Poll, 0).unwrap();
        assert_eq!(n, 1);
        assert_eq!(parse_tcp_s_request(&out[..n]), Some((TcpSReq::Poll, 0)));
        let n = encode_tcp_s_request(&mut out, TcpSReq::Poll, 5).unwrap();
        assert_eq!(n, 5);
        assert_eq!(parse_tcp_s_request(&out[..n]), Some((TcpSReq::Poll, 5)));
    }

    /// **零漂移钉板**：session 0 的三 op 编码输出与 v1 编码器逐字节恒等
    /// （含长度）——TCPS ok 既有消费链的字节面保证按构造成立。
    #[test]
    fn tcp_s_session0_bytes_identical_to_legacy() {
        let mut a = [0u8; REQ_S_LEN_MAX];
        let mut b = [0u8; REQ_S_LEN_MAX];
        let na = encode_tcp_s_request(&mut a, TcpSReq::Open { port: 18080 }, 0).unwrap();
        let nb = encode_tcp_request(&mut b, TcpReq::Open { port: 18080 }).unwrap();
        assert_eq!((na, &a[..na]), (nb, &b[..nb]));

        let na = encode_tcp_s_request(&mut a, TcpSReq::Roundtrip { data: b"xy" }, 0).unwrap();
        let nb = encode_tcp_request(&mut b, TcpReq::Roundtrip { data: b"xy" }).unwrap();
        assert_eq!((na, &a[..na]), (nb, &b[..nb]));

        let na = encode_tcp_s_request(&mut a, TcpSReq::Close, 0).unwrap();
        let nb = encode_tcp_request(&mut b, TcpReq::Close).unwrap();
        assert_eq!((na, &a[..na]), (nb, &b[..nb]));
    }

    /// wire 钉板：尾随 session 形黄金字节（sid LE 尾随位 + 新三 op 定形；
    /// 端序/尾随位置漂移在此炸响）。
    #[test]
    fn tcp_s_extended_wire_pinned() {
        let mut out = [0u8; REQ_S_LEN_MAX];
        let n = encode_tcp_s_request(&mut out, TcpSReq::Open { port: 18080 }, 5).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_OPEN, 0xA0, 0x46, 5, 0, 0, 0]);

        let n = encode_tcp_s_request(&mut out, TcpSReq::Roundtrip { data: b"xy" }, 2).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_ROUNDTRIP, 2, b'x', b'y', 2, 0, 0, 0]);

        let n = encode_tcp_s_request(&mut out, TcpSReq::Close, 1).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_CLOSE, 1, 0, 0, 0]);

        let n = encode_tcp_s_request(&mut out, TcpSReq::Listen { port: 18081 }, 0).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_LISTEN, 0xA1, 0x46]); // 18081 = 0x46A1

        let n = encode_tcp_s_request(&mut out, TcpSReq::Accept, 3).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_ACCEPT, 3, 0, 0, 0]);

        let n = encode_tcp_s_request(&mut out, TcpSReq::Poll, TCP_SESSIONS_MAX).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_POLL, TCP_SESSIONS_MAX as u8, 0, 0, 0]);

        let n = encode_tcp_s_request(&mut out, TcpSReq::Drain, 2).unwrap();
        assert_eq!(&out[..n], &[OP_TCP_DRAIN, 2, 0, 0, 0]);
    }

    /// 尾随 sid=0 野值拒：该用 v1 形（规范形单一）；编码侧结构上产不出，
    /// 解析侧双侧拒。
    #[test]
    fn tcp_s_extended_session_zero_rejected() {
        assert_eq!(parse_tcp_s_request(&[OP_TCP_OPEN, 0xA0, 0x46, 0, 0, 0, 0]), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_CLOSE, 0, 0, 0, 0]), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_POLL, 0, 0, 0, 0]), None);
        // ROUNDTRIP 尾随 0：去尾后 v1 形合法但 sid=0 仍拒
        assert_eq!(parse_tcp_s_request(&[OP_TCP_ROUNDTRIP, 1, b'a', 0, 0, 0, 0]), None);
    }

    /// session 帽界：恰 TCP_SESSIONS_MAX 收 / +1 拒 / u32::MAX 拒（编码/
    /// 解析双侧对称——帽常量双侧同步的协议层执法点）。
    #[test]
    fn tcp_s_session_cap_boundaries() {
        let mut out = [0u8; REQ_S_LEN_MAX];
        assert_eq!(
            encode_tcp_s_request(&mut out, TcpSReq::Accept, TCP_SESSIONS_MAX),
            Some(5)
        );
        assert_eq!(
            encode_tcp_s_request(&mut out, TcpSReq::Accept, TCP_SESSIONS_MAX + 1),
            None
        );
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Open { port: 1 }, u32::MAX), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_ACCEPT, 0, 0, 0, TCP_SESSIONS_MAX as u8 + 1]), None);
        assert_eq!(
            parse_tcp_s_request(&[OP_TCP_OPEN, 0xA0, 0x46, 0xFF, 0xFF, 0xFF, 0xFF]),
            None
        );
    }

    /// LISTEN 请求规则：port 0 / 65535 边界、session≠0 编码拒、7B 尾随形
    /// 解析拒（无 v1 形可尾随——恰长 3B 单形）。
    #[test]
    fn tcp_s_listen_request_rules() {
        let mut out = [0u8; REQ_S_LEN_MAX];
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Listen { port: 0 }, 0), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_LISTEN, 0, 0]), None);
        assert_eq!(
            encode_tcp_s_request(&mut out, TcpSReq::Listen { port: u16::MAX }, 0),
            Some(3)
        );
        assert_eq!(parse_tcp_s_request(&[OP_TCP_LISTEN, 0xFF, 0xFF]), Some((TcpSReq::Listen { port: u16::MAX }, 0)));
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Listen { port: 80 }, 3), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_LISTEN, 0x50, 0, 3, 0, 0, 0]), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_LISTEN, 0x50]), None); // 截断
    }

    /// ACCEPT 请求规则：session 0 编码拒（listen 会话必须显式）+ 1B 形解析
    /// 拒 + 尾随拒。
    #[test]
    fn tcp_s_accept_request_rules() {
        let mut out = [0u8; REQ_S_LEN_MAX];
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Accept, 0), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_ACCEPT]), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_ACCEPT, 1, 0, 0, 0, 0]), None); // 尾随
        assert_eq!(parse_tcp_s_request(&[OP_TCP_ACCEPT, 1, 0]), None); // 截断
        assert_eq!(parse_tcp_s_request(&[]), None);
    }

    /// legacy 解析器对新形零接触：尾随 session 形 / 新三 op 在 v1 面全拒
    /// （零漂移面的反向——新形不污染 v1 面）。
    #[test]
    fn tcp_s_new_forms_rejected_by_legacy_parse() {
        let mut out = [0u8; REQ_S_LEN_MAX];
        let n = encode_tcp_s_request(&mut out, TcpSReq::Open { port: 18080 }, 5).unwrap();
        assert_eq!(parse_tcp_request(&out[..n]), None);
        let n = encode_tcp_s_request(&mut out, TcpSReq::Roundtrip { data: b"xy" }, 5).unwrap();
        assert_eq!(parse_tcp_request(&out[..n]), None);
        let n = encode_tcp_s_request(&mut out, TcpSReq::Close, 5).unwrap();
        assert_eq!(parse_tcp_request(&out[..n]), None);
        let n = encode_tcp_s_request(&mut out, TcpSReq::Listen { port: 18081 }, 0).unwrap();
        assert_eq!(parse_tcp_request(&out[..n]), None);
        let n = encode_tcp_s_request(&mut out, TcpSReq::Accept, 5).unwrap();
        assert_eq!(parse_tcp_request(&out[..n]), None);
        let n = encode_tcp_s_request(&mut out, TcpSReq::Poll, 5).unwrap();
        assert_eq!(parse_tcp_request(&out[..n]), None);
    }

    /// 新 st 不入 legacy 域：v1 应答编码器/解析器对 EAGAIN/NOSESSION/FULL
    /// 全拒（st 域分面钉板——DNS 面 BUSY 同款纪律）。
    #[test]
    fn tcp_s_st_not_in_legacy_domain() {
        let mut out = [0u8; TCP_REP_MAX];
        // NODEV 不在此：A3-3 入域（deviceless serve 对 OPEN 也答此码——
        // B4 掉电相 bsh skt client 腿的确定性跳过信号，connect→ENODEV）。
        for st in [ST_EAGAIN, ST_NOSESSION, ST_FULL] {
            assert_eq!(encode_tcp_reply(&mut out, st, b""), None);
            assert_eq!(parse_tcp_reply(&[st]), None);
        }
    }

    /// sid 形应答往返 + wire 钉板：OK 携 sid LE / 非 OK 1B；LISTEN 与
    /// ACCEPT 共形分域。
    #[test]
    fn sid_reply_roundtrip_and_wire_pinned() {
        let mut out = [0u8; SID_REP_LEN];
        let n = encode_listen_reply(&mut out, ST_OK, 8).unwrap();
        assert_eq!(n, SID_REP_LEN);
        assert_eq!(&out[..n], &[ST_OK, 8, 0, 0, 0]);
        assert_eq!(parse_listen_reply(&out[..n]), Some(SidRep { st: ST_OK, sid: 8 }));

        let n = encode_accept_reply(&mut out, ST_OK, 1).unwrap();
        assert_eq!(&out[..n], &[ST_OK, 1, 0, 0, 0]);
        assert_eq!(parse_accept_reply(&out[..n]), Some(SidRep { st: ST_OK, sid: 1 }));

        let n = encode_accept_reply(&mut out, ST_EAGAIN, 0).unwrap();
        assert_eq!(&out[..n], &[ST_EAGAIN]);
        assert_eq!(parse_accept_reply(&out[..n]), Some(SidRep { st: ST_EAGAIN, sid: 0 }));

        let n = encode_listen_reply(&mut out, ST_FULL, 0).unwrap();
        assert_eq!(&out[..n], &[ST_FULL]);
        assert_eq!(parse_listen_reply(&out[..n]), Some(SidRep { st: ST_FULL, sid: 0 }));

        // A2-3 NODEV：1B 形（deviceless serve 的拓扑跳过信号——仅 LISTEN 域收）
        let n = encode_listen_reply(&mut out, ST_NODEV, 0).unwrap();
        assert_eq!(&out[..n], &[ST_NODEV]);
        assert_eq!(parse_listen_reply(&out[..n]), Some(SidRep { st: ST_NODEV, sid: 0 }));
    }

    /// sid 形域执法：LISTEN 不收 EAGAIN/NOSESSION；ACCEPT 不收 TIMEOUT/BUSY；
    /// 域外 st 编码/解析双侧拒。
    #[test]
    fn sid_reply_domains() {
        let mut out = [0u8; SID_REP_LEN];
        // LISTEN 域外：ACCEPT 专属值
        assert_eq!(encode_listen_reply(&mut out, ST_EAGAIN, 0), None);
        assert_eq!(encode_listen_reply(&mut out, ST_NOSESSION, 0), None);
        assert_eq!(parse_listen_reply(&[ST_EAGAIN]), None);
        assert_eq!(parse_listen_reply(&[ST_NOSESSION]), None);
        // ACCEPT 域外：v1 专属值
        assert_eq!(encode_accept_reply(&mut out, ST_TIMEOUT, 0), None);
        assert_eq!(encode_accept_reply(&mut out, ST_BUSY, 0), None);
        assert_eq!(parse_accept_reply(&[ST_TIMEOUT]), None);
        assert_eq!(parse_accept_reply(&[ST_BUSY]), None);
        // 双域外
        assert_eq!(encode_listen_reply(&mut out, 0xFF, 0), None);
        assert_eq!(parse_accept_reply(&[0xFF]), None);
        // NODEV 单域收（A2-3）：LISTEN 收 / ACCEPT 拒（bsh 仅在 LISTEN 见之跳过）
        assert_eq!(encode_accept_reply(&mut out, ST_NODEV, 0), None);
        assert_eq!(parse_accept_reply(&[ST_NODEV]), None);
    }

    /// sid 形烘焙线：非 OK 携 sid 编码拒 + 非 OK 5B 解析拒；OK 携 sid=0 /
    /// 越帽双向拒；1B OK 形（成功必携 sid）拒；尾随/截断拒。
    #[test]
    fn sid_reply_baking_line_and_shapes() {
        let mut out = [0u8; SID_REP_LEN];
        assert_eq!(encode_accept_reply(&mut out, ST_EAGAIN, 1), None);
        assert_eq!(parse_accept_reply(&[ST_EAGAIN, 1, 0, 0, 0]), None);
        assert_eq!(encode_listen_reply(&mut out, ST_OK, 0), None);
        assert_eq!(parse_listen_reply(&[ST_OK, 0, 0, 0, 0]), None);
        assert_eq!(encode_listen_reply(&mut out, ST_OK, TCP_SESSIONS_MAX + 1), None);
        assert_eq!(parse_listen_reply(&[ST_OK, 9, 0, 0, 0]), None);
        assert_eq!(parse_listen_reply(&[ST_OK]), None); // 1B OK 形
        assert_eq!(parse_accept_reply(&[ST_OK, 1, 0, 0, 0, 0]), None); // 尾随
        assert_eq!(parse_accept_reply(&[ST_OK, 1, 0]), None); // 截断
        assert_eq!(parse_listen_reply(&[]), None);
        // out 不足 None 契约
        let mut small = [0u8; 4];
        assert_eq!(encode_accept_reply(&mut small, ST_OK, 1), None);
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_listen_reply(&mut empty, ST_FULL, 0), None);
    }

    /// POLL 应答往返 + wire 钉板：flags 位组合 / rbytes LE / 非 OK 1B；
    /// rbytes=0 且 flags=0 的「暂无就绪」是合法 OK。
    #[test]
    fn poll_reply_roundtrip_and_wire_pinned() {
        let mut out = [0u8; POLL_REP_LEN];
        let n = encode_poll_reply(&mut out, ST_OK, POLL_FLAG_WRITABLE | POLL_FLAG_CLOSED, 1000).unwrap();
        assert_eq!(n, POLL_REP_LEN);
        assert_eq!(&out[..n], &[ST_OK, 0x03, 0xE8, 0x03]); // 1000 = 0x03E8 LE
        assert_eq!(
            parse_poll_reply(&out[..n]),
            Some(PollRep { st: ST_OK, flags: 0x03, rbytes: 1000 })
        );

        let n = encode_poll_reply(&mut out, ST_OK, 0, 0).unwrap();
        assert_eq!(&out[..n], &[ST_OK, 0, 0, 0]);
        assert_eq!(parse_poll_reply(&out[..n]), Some(PollRep { st: ST_OK, flags: 0, rbytes: 0 }));

        let n = encode_poll_reply(&mut out, ST_NOSESSION, 0, 0).unwrap();
        assert_eq!(&out[..n], &[ST_NOSESSION]);
        assert_eq!(
            parse_poll_reply(&out[..n]),
            Some(PollRep { st: ST_NOSESSION, flags: 0, rbytes: 0 })
        );

        // rbytes 上界
        let n = encode_poll_reply(&mut out, ST_OK, 0, u16::MAX).unwrap();
        assert_eq!(parse_poll_reply(&out[..n]), Some(PollRep { st: ST_OK, flags: 0, rbytes: u16::MAX }));
    }

    /// POLL 域与形状执法：TIMEOUT/EAGAIN 域外拒；未知 flags 位双向拒；
    /// 非 OK 携就绪编码拒 + 非 OK 4B 解析拒；1B OK / 尾随 / 截断拒。
    #[test]
    fn poll_reply_domain_and_shapes() {
        let mut out = [0u8; POLL_REP_LEN];
        assert_eq!(encode_poll_reply(&mut out, ST_TIMEOUT, 0, 0), None);
        assert_eq!(encode_poll_reply(&mut out, ST_EAGAIN, 0, 0), None);
        assert_eq!(parse_poll_reply(&[ST_TIMEOUT]), None);
        assert_eq!(parse_poll_reply(&[ST_EAGAIN]), None);
        assert_eq!(encode_poll_reply(&mut out, ST_OK, 0x04, 0), None); // 未知位
        assert_eq!(parse_poll_reply(&[ST_OK, 0x04, 0, 0]), None);
        assert_eq!(encode_poll_reply(&mut out, ST_BAD, POLL_FLAG_WRITABLE, 0), None);
        assert_eq!(encode_poll_reply(&mut out, ST_NOSESSION, 0, 1), None);
        assert_eq!(parse_poll_reply(&[ST_BAD, 0x01, 0, 0]), None);
        assert_eq!(parse_poll_reply(&[ST_OK]), None); // 1B OK 形
        assert_eq!(parse_poll_reply(&[ST_OK, 0, 0, 0, 0]), None); // 尾随
        assert_eq!(parse_poll_reply(&[ST_OK, 0, 0]), None); // 截断
        assert_eq!(parse_poll_reply(&[]), None);
        let mut small = [0u8; 3];
        assert_eq!(encode_poll_reply(&mut small, ST_OK, 0, 0), None);
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_poll_reply(&mut empty, ST_BAD, 0, 0), None);
    }

    /// 会话化请求 out 不足 None 契约 + 恰满帽（ROUNDTRIP 128B + 4B 尾随
    /// = REQ_S_LEN_MAX）。
    #[test]
    fn tcp_s_request_cap_and_out_boundaries() {
        let full = [7u8; TCP_DATA_MAX];
        let mut out = [0u8; REQ_S_LEN_MAX];
        let n = encode_tcp_s_request(&mut out, TcpSReq::Roundtrip { data: &full }, TCP_SESSIONS_MAX).unwrap();
        assert_eq!(n, REQ_S_LEN_MAX);
        match parse_tcp_s_request(&out[..n]) {
            Some((TcpSReq::Roundtrip { data }, sid)) => {
                assert_eq!(data, &full[..]);
                assert_eq!(sid, TCP_SESSIONS_MAX);
            }
            other => panic!("cap-len sessioned roundtrip must parse: {other:?}"),
        }
        // v1 野值在会话化面同拒（单源复用的回归面）
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Open { port: 0 }, 5), None);
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Roundtrip { data: b"" }, 5), None);
        let over = [7u8; TCP_DATA_MAX + 1];
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Roundtrip { data: &over }, 5), None);
        // out 不足：恰 v1 长度装不下 +4B 尾随
        let mut small = [0u8; REQ_LEN_MAX];
        assert_eq!(encode_tcp_s_request(&mut small, TcpSReq::Roundtrip { data: &full }, 1), None);
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_tcp_s_request(&mut empty, TcpSReq::Close, 0), None);
        assert_eq!(encode_tcp_s_request(&mut empty, TcpSReq::Listen { port: 80 }, 0), None);
    }

    /// DRAIN 请求规则（A2-3）：恰长 5B 单形 / session 0 编码与解析双侧拒
    /// （默认会话收割走 legacy ROUNDTRIP 面）/ 尾随与截断拒 / 帽界对称 /
    /// 帽内往返。应答面零新增（复用 v1 两形——1B OK 形即「零收成」合法值，
    /// encode_tcp_reply/parse_tcp_reply 既有面覆盖，不重测）。
    #[test]
    fn tcp_s_drain_request_rules() {
        let mut out = [0u8; REQ_S_LEN_MAX];
        // 帽内往返（两端界）
        for sid in [1u32, TCP_SESSIONS_MAX] {
            let n = encode_tcp_s_request(&mut out, TcpSReq::Drain, sid).unwrap();
            assert_eq!(n, 5);
            assert_eq!(parse_tcp_s_request(&out[..n]), Some((TcpSReq::Drain, sid)));
        }
        // session 0 双侧拒
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Drain, 0), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_DRAIN, 0, 0, 0, 0]), None);
        // 越帽（编码 u32::MAX / 解析帽+1）
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Drain, u32::MAX), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_DRAIN, 0, 0, 0, TCP_SESSIONS_MAX as u8 + 1]), None);
        // 恰长执法：截断 1B / 尾随 6B 均拒（无 v1 形可尾随——与 ACCEPT 同款单形）
        assert_eq!(parse_tcp_s_request(&[OP_TCP_DRAIN]), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_DRAIN, 2, 0, 0, 0, 0]), None);
        // out 不足
        let mut small = [0u8; 4];
        assert_eq!(encode_tcp_s_request(&mut small, TcpSReq::Drain, 1), None);
    }
}
