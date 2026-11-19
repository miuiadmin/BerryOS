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
//! - TCP 三 op（T-d，`docs/TCP栈设计草案-v1.0.md` §8）：OPEN `[op][port u16 LE]`
//!   3B（port 0 拒）/ ROUNDTRIP `[op][dlen u8][data]` = 2+dlen（dlen ∈
//!   1..=[`TCP_DATA_MAX`] = RPC 单 op 载荷帽——inline 通道几何定值，与
//!   berry-tcp MSS 解耦：op 载荷可拆多段在途（分段归 vnet/berry-tcp），
//!   回显严格等长但可多段）/ CLOSE `[op]` 1B；应答按形状分两形：**1B `[st]`**（OPEN/
//!   CLOSE 应答 + 一切非 OK 错误应答）与 **`[st][dlen][data]`**（仅
//!   ROUNDTRIP 成功，回传服务端**真实收到的字节**——对拍归 client，驱动不
//!   知道期望答案，DNS「addr 该是多少归 svcmgr」同款分工）。单会话独占：
//!   会话已被占用时 OPEN → [`ST_BUSY`]。
//! - TCP 会话化面（A2-2，`docs/事件循环A2设计草案-v1.0.md` §3.2）：session 字段
//!   以**尾随 u32 LE** 携带——session 0 = 默认会话（v1 字节逐字节相同，
//!   现行 op 序零漂移），1..=[`TCP_SESSIONS_MAX`] = 命名会话（v1 形 +4B
//!   尾随）；新 op LISTEN/ACCEPT/POLL 无 v1 形各自恰长执法；应答 sid 形
//!   `[st][sid u32 LE]` 与 poll 形 `[st][flags u8][rbytes u16 LE]` 仅新 op
//!   使用（Open/Roundtrip/Close 会话化应答复用 v1 两形）。
//! - RT 请求序号形（四刀流水线）：session-0 ROUNDTRIP 请求/应答各增 +1B
//!   尾随 seq u8 旁路形（[`encode_rt_seq_request`]；应答经
//!   [`encode_tcp_reply`]/[`parse_tcp_reply`] 的 seq 出参）——K 窗流水线的
//!   迟到/错配应答配对面；载荷域纯净（seq 字节不进回显流）。窗深
//!   [`RT_PIPE_K`] 双侧共源；无 seq 请求恒无 seq 应答（旧消费者零漂移）。
//!
//! - TCP VMO 面（e 门续刀）：DRAIN_VMO/RT_VMO 二 op 载荷/收割字节走随行
//!   handle（客户端 VMO——blkproto 超帽走 VMO 先例的 TCP 数据面移植），
//!   请求只携元数据定长帧（5B/8B）、dlen 抬 u16 破 250B inline 帽；应答
//!   3B/4B 定长无 inline（非 OK 恒零 dlen）；显式 sid 字段 = 命名会话专属
//!   （0 拒）。
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

/// 通道 inline 帽镜像（kernel chanface `CHAN_MSG_MAX` 单源同值——本库零依赖
/// 不可 import kernel 侧，镜像值 + 下方断言族机器钉：后续抬 `TCP_DATA_MAX`
/// 先过通道几何，真源抬值时镜像同批改（ksproto 断言族先例同款）。
pub const CHAN_MSG_MAX: usize = 256;

/// TCP 单 op 载荷帽（字节，二刀 128→250）——inline 通道几何定值：会话形请求
/// `[op][dlen][data][sid u32]` = 6+dlen ≤ [`CHAN_MSG_MAX`] ⇒ dlen ≤ 250 恰满
/// 域。与 berry-tcp MSS 解耦：op 载荷在 TCP 层可拆多段在途（分段归
/// vnet/berry-tcp 发送面），回显严格等长（可多段）。
pub const TCP_DATA_MAX: usize = 250;

// inline 几何断言族（二刀 P2 机器钉）：一切帧形 ≤ 通道帽——会话形请求
// [op][dlen][data][sid u32] = 6+TCP_DATA_MAX ≤ CHAN_MSG_MAX（恰满式）；
// 请求/应答帽符号派生自动跟随。超帽即编译期炸响（A-4 const assert 同族）。
// 四刀 P1 扩族：RT seq 形请求 [op][dlen][data][seq u8] = 3+TCP_DATA_MAX 与
// seq 形应答 TCP_REP_MAX+1 同帽钉（两钉）。
const _: () = assert!(6 + TCP_DATA_MAX <= CHAN_MSG_MAX);
const _: () = assert!(REQ_LEN_MAX <= CHAN_MSG_MAX);
const _: () = assert!(TCP_REP_MAX <= CHAN_MSG_MAX);
const _: () = assert!(REQ_S_LEN_MAX <= CHAN_MSG_MAX);
const _: () = assert!(RT_SEQ_REQ_MAX <= CHAN_MSG_MAX);
const _: () = assert!(TCP_REP_MAX + 1 <= CHAN_MSG_MAX);
// e 门续刀扩族：VMO 形四帧皆定长小帧（5/8/3/4B ≤ 帽——载荷走随行 handle
// 不占 inline；机器钉防后续扩字段破帽）。
const _: () = assert!(DRAIN_VMO_REQ_LEN <= CHAN_MSG_MAX);
const _: () = assert!(RT_VMO_REQ_LEN <= CHAN_MSG_MAX);
const _: () = assert!(VMO_REP_LEN <= CHAN_MSG_MAX);
const _: () = assert!(RT_VMO_REP_LEN <= CHAN_MSG_MAX);

/// 请求线上长度上界（DNS 2+NAME_MAX 与 TCP ROUNDTRIP 2+TCP_DATA_MAX 取大）。
pub const REQ_LEN_MAX: usize = 2 + TCP_DATA_MAX;

/// TCP 应答线上长度上界（`[st][dlen][TCP_DATA_MAX]`）。
pub const TCP_REP_MAX: usize = 2 + TCP_DATA_MAX;

/// session-0 RT 请求 seq 形线上长度上界（四刀 P1：`[op][dlen][data][seq u8]`
/// = 3+TCP_DATA_MAX = 253 ≤ CHAN_MSG_MAX）。尾随 seq 形与尾随 session 形
/// 同族旁路：载荷域纯净（回显流不掺配对字节——echo 逐字节契约不破）。
pub const RT_SEQ_REQ_MAX: usize = 3 + TCP_DATA_MAX;

/// RT seq 形应答线上长度上界（`[st][dlen][data][seq]` 同 3+TCP_DATA_MAX；
/// 2B `[st][seq]` 形结构性更短——vnet serve 应答缓冲取本值）。
pub const RT_SEQ_REP_MAX: usize = 3 + TCP_DATA_MAX;

/// RT 流水线窗深（四刀 2、五刀③ 2→3）：libc Client 臂 K 窗与 vnet
/// session-0 挂起 op 队列的共源几何定值——K=3 保守案：K×250=750 ≤
/// min(TX_CAP, WINDOW)=1024 双界（五刀③环扩 TX_SLOTS 4→8 + 通告窗
/// 512→1024 联动，余 274B+2 槽；K=4 恰满零余量拒；vnet tcp.rs 消费侧
/// const assert 机器钉）；回绕距 u8 全域 256 ≫ K 无歧义。双侧（libc
/// TCP_PIPE_K / vnet RT_K）必同值。
pub const RT_PIPE_K: usize = 3;

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
/// 四刀 P1 扩 `seq` 出参：session-0 RT 的 seq 形应答携原请求尾随 seq
/// （`[st][seq]` 2B / `[st][dlen][data][seq]` 3+dlen）——无 seq 请求 →
/// 无 seq 应答（v1 两形零漂移，旧消费者永不见 seq 形）。
#[derive(Debug, PartialEq, Eq)]
pub struct TcpReply<'a> {
    pub st: u8,
    /// 仅 ROUNDTRIP 成功非空；OPEN/CLOSE 应答与一切非 OK 应答恒空。
    pub data: &'a [u8],
    /// seq 形应答的请求配对号（v1 两形恒 None）。
    pub seq: Option<u8>,
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
/// 非 OK）= 1B `[st]`；携 data（仅 OK）= `[st][dlen][data]`。四刀 P1 扩
/// `seq` 形（session-0 RT 应答专属）：`[st][seq]` 2B / `[st][dlen][data][seq]`
/// ——ST_BUSY 背压应答必携原 seq（客户端同 seq 退避重发）；v1 消费者传
/// `None` 走 v1 两形。烘焙线不变量：未知 st → None；**非 OK 状态携 data →
/// None**（与 DNS addr 烘焙线同族——线上不存在「超时了但带回显」的歧义
/// 形态）；data 越帽 / out 太短 → None。
pub fn encode_tcp_reply(out: &mut [u8], st: u8, data: &[u8], seq: Option<u8>) -> Option<usize> {
    if !tcp_st_known(st) {
        return None;
    }
    if st != ST_OK && !data.is_empty() {
        return None;
    }
    if st == ST_OK && data.is_empty() && seq.is_some() {
        return None; // OK 空数据（DRAIN 零收成）恒 v1 1B 形——seq 2B 形域外
    }
    if data.is_empty() {
        let n = if seq.is_some() { 2 } else { 1 };
        let m = out.get_mut(..n)?;
        m[0] = st;
        if let Some(s) = seq {
            m[1] = s;
        }
        Some(n)
    } else {
        if data.len() > TCP_DATA_MAX {
            return None;
        }
        let total = 2 + data.len() + usize::from(seq.is_some());
        let m = out.get_mut(..total)?;
        m[0] = st;
        m[1] = data.len() as u8;
        m[2..2 + data.len()].copy_from_slice(data);
        if let Some(s) = seq {
            m[2 + data.len()] = s;
        }
        Some(total)
    }
}

/// v1 两形解析（seq 形消解的基座：1B `[st]` 或 2+dlen 恰长）。
fn parse_tcp_reply_v1(buf: &[u8]) -> Option<TcpReply<'_>> {
    let st = *buf.first()?;
    if !tcp_st_known(st) {
        return None;
    }
    if buf.len() == 1 {
        return Some(TcpReply { st, data: &[], seq: None });
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
    Some(TcpReply { st, data: buf.get(2..)?, seq: None })
}

/// 解析 TCP 应答：v1 两形（1B `[st]` / `[st][dlen][data]` 仅 OK）优先恰长，
/// 再试 +1 尾随 seq 形（`[st][seq]` / `[st][dlen][data][seq]`——session 尾随
/// 先例同款；长度歧义结构上不存在：两形 dlen 同字节权威，v1 需 2+dlen、
/// seq 形需 3+dlen 恒不相等，2B 形的 v1 读法 dlen=0 恒拒）；烘焙线（非 OK
/// 携 data 拒 / dlen 0 或越帽 / 与 buf 长度不符 / 未知 st）双形共执法。
pub fn parse_tcp_reply(buf: &[u8]) -> Option<TcpReply<'_>> {
    if let Some(r) = parse_tcp_reply_v1(buf) {
        return Some(r);
    }
    if buf.len() < 2 {
        return parse_tcp_reply_v1(buf); // 1B/空：v1 面执法定夺（含 None）
    }
    let seq = *buf.last()?;
    let r = parse_tcp_reply_v1(&buf[..buf.len() - 1]).map(|r| TcpReply { seq: Some(seq), ..r })?;
    if r.st == ST_OK && r.data.is_empty() {
        return None; // OK 空数据恒 v1 1B 形（与编码侧对称——2B seq 形域外）
    }
    Some(r)
}

/// 编码 session-0 RT 请求 seq 形（四刀 P1，libc K 窗专用）：
/// `[op][dlen][data][seq u8]` = 3+dlen ≤ [`RT_SEQ_REQ_MAX`]。尾随 seq 与
/// 尾随 session 互斥（命名 RT 携 seq = 3+dlen+4 = 257 > CHAN_MSG_MAX 几何
/// 阻死——无此 API 面，互操作矩阵单测钉）。None 契约：data 空/越帽 /
/// out 太短。
pub fn encode_rt_seq_request(out: &mut [u8], data: &[u8], seq: u8) -> Option<usize> {
    if data.is_empty() || data.len() > TCP_DATA_MAX {
        return None;
    }
    let total = 3 + data.len();
    let m = out.get_mut(..total)?;
    m[0] = OP_TCP_ROUNDTRIP;
    m[1] = data.len() as u8;
    m[2..2 + data.len()].copy_from_slice(data);
    m[2 + data.len()] = seq;
    Some(total)
}

// ---------- TCP 会话化面（A2-2 段①：协议层）----------
//
// `docs/事件循环A2设计草案-v1.0.md` §3.2——单会话 ST_BUSY 升会话表。**零漂移轴**：
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
/// 后 v1 解析必须过。四刀 P1 扩第三形：ROUNDTRIP 的 +1B 尾随 seq 形
/// （session-0 专属——`[op][dlen][data][seq u8]`，出参 `Some(seq)`；命名
/// 会话形永不携 seq，257 > CHAN_MSG_MAX 几何阻死）。三形消解序 = v1 恰长
/// → −4B sid → −1B seq（同字节 dlen 权威，结构上互斥无歧义——单测钉板）。
/// 新 op 各自恰长执法（同 v1 面全部尾随/截断拒）。
pub fn parse_tcp_s_request(buf: &[u8]) -> Option<(TcpSReq<'_>, u32, Option<u8>)> {
    match *buf.first()? {
        OP_TCP_OPEN | OP_TCP_CLOSE => {
            if let Some(r) = parse_tcp_request(buf) {
                return Some((r.into(), 0, None)); // v1 恰长形
            }
            let n = buf.len().checked_sub(4)?;
            let sid = u32::from_le_bytes(buf.get(n..)?.try_into().ok()?);
            if sid == 0 || sid > TCP_SESSIONS_MAX {
                return None; // 尾随 sid 野值
            }
            let r = parse_tcp_request(buf.get(..n)?)?;
            Some((r.into(), sid, None))
        }
        OP_TCP_ROUNDTRIP => {
            if let Some(r) = parse_tcp_request(buf) {
                return Some((r.into(), 0, None)); // v1 恰长形
            }
            // −4B 尾随 sid 形（命名会话）：前缀解析失败**落到** seq 形——用户
            // 数据可构造「尾 4B 恰似合法 sid」的 seq 形帧，`?` 早退会误吞。
            if buf.len() >= 4 {
                if let Some(t) = buf.get(buf.len() - 4..) {
                    if let Ok(b) = <[u8; 4]>::try_from(t) {
                        let sid = u32::from_le_bytes(b);
                        if sid != 0 && sid <= TCP_SESSIONS_MAX {
                            if let Some(r) = buf.get(..buf.len() - 4).and_then(parse_tcp_request) {
                                return Some((r.into(), sid, None));
                            }
                        }
                    }
                }
            }
            // −1B 尾随 seq 形（session-0 专属）。
            let seq = *buf.last()?;
            let body = buf.get(..buf.len().checked_sub(1)?)?;
            let r = parse_tcp_request(body)?;
            Some((r.into(), 0, Some(seq)))
        }
        OP_TCP_LISTEN => {
            if buf.len() != 3 {
                return None;
            }
            let port = u16::from_le_bytes([buf[1], buf[2]]);
            if port == 0 {
                return None;
            }
            Some((TcpSReq::Listen { port }, 0, None))
        }
        OP_TCP_ACCEPT => {
            if buf.len() != 5 {
                return None;
            }
            let sid = u32::from_le_bytes(buf[1..5].try_into().ok()?);
            if sid == 0 || sid > TCP_SESSIONS_MAX {
                return None;
            }
            Some((TcpSReq::Accept, sid, None))
        }
        OP_TCP_POLL => {
            if buf.len() == 1 {
                return Some((TcpSReq::Poll, 0, None)); // 默认会话
            }
            if buf.len() != 5 {
                return None;
            }
            let sid = u32::from_le_bytes(buf[1..5].try_into().ok()?);
            if sid == 0 || sid > TCP_SESSIONS_MAX {
                return None;
            }
            Some((TcpSReq::Poll, sid, None))
        }
        OP_TCP_DRAIN => {
            if buf.len() != 5 {
                return None;
            }
            let sid = u32::from_le_bytes(buf[1..5].try_into().ok()?);
            if sid == 0 || sid > TCP_SESSIONS_MAX {
                return None;
            }
            Some((TcpSReq::Drain, sid, None))
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

// ---------- TCP VMO 面（e 门续刀：DRAIN_VMO/RT_VMO）----------
//
// 载荷/收割字节走随行 handle（客户端 VMO），请求帧只携元数据——dlen 抬
// u16 破 v1 inline 帽 250B（A2-3 实抓坑：表会话 DRAIN 割幅 >250B 入 v1
// 应答即 encode 越帽，只能循环 250B 割；RT 大段发送同受 250B 拆段）。
// blkproto 先例（512B 扇区超 inline 帽走 VMO）的 TCP 数据面移植。随行
// 1 handle ≤ CHAN_HND_MAX=8 恒法（通道几何面；协议层只定字节面不验
// handle——svcmgr blk_rpc duplicate→send 随行 / vblk 臂 nh==1→vmo_read/
// write 的通道层职辖）。
//
// 两 op 无 v1 形各自恰长定形（LISTEN/POLL 先例；op 首字节 9/10 与 v1 八
// op 天然可分——v1/会话化解析器按未知 op 拒，旧 op 零漂移按构造）：
// - DRAIN_VMO 请求 `[op][sid u32 LE]` = [`DRAIN_VMO_REQ_LEN`] 5B 随行
//   1 handle（收割目标 VMO）；应答恰 [`VMO_REP_LEN`] 3B 定长 `[st][dlen
//   u16 LE]` 无 inline（收割字节已直写 VMO），非 OK 恒零 dlen；OK dlen=0
//   合法（零收成诚实值——v1 DRAIN 1B OK 形同族）。
// - RT_VMO 请求 `[op][dlen u16 LE][seq u8][sid u32 LE]` = [`RT_VMO_REQ_LEN`]
//   8B 随行 1 handle（预填载荷的客户端 VMO）；应答恰 [`RT_VMO_REP_LEN`]
//   4B `[st][dlen u16 LE][seq u8]`——seq 形 RT 配对（v1 无 seq 形不涉；
//   BUSY 背压必携原 seq——客户端同 seq 退避重发）；OK dlen=0 拒（RT
//   载荷非空族法的应答侧对称）。
//
// sid 域：显式 sid 字段 = 命名会话专属（∈ 1..=[`TCP_SESSIONS_MAX`]，0 拒
// ——DRAIN/ACCEPT 同法：默认会话的收割/往返走 legacy ROUNDTRIP 面）。
// 应答 st 域 = v1 TCP 域 + [`ST_NOSESSION`]（专用形解锁 v1 两形字节面
// 锁死的死 sid 诊断位——Sid/Poll 会话族先例）；EAGAIN/FULL 不入（零收成
// 是 OK dlen=0 的合法内容；表满归 LISTEN/ACCEPT 造会话面）。应答两形与
// v1 两形无消解关系：严格串行 RPC 下调用方按所发 op 选解析器（Sid/Poll
// 专用形同款），且 3B 形可能与 v1 `[st][dlen][data]` 形结构巧合（如
// `[ST_OK,1,0]` 两读皆合法）——不跨面解析即无歧义。；4B rt_vmo 形同类巧合：`[ST_OK,1,0,5]` 同为合法 v1 seq 形（dlen=1 前缀+尾 seq）与合法 rt_vmo 形（dlen=1,seq=5）——POLL OK 形 `[st][flags][rbytes LE]` 亦同幅。应答面跨面禁令是串行 RPC 约定（按所发 op 选解析器）非机制——双读钉板见 vmo_cross_face_coincidence_pinned。

/// TCP 收割接收缓冲 VMO 形（e 门续刀）：数据会话 → 收割字节直写随行
/// 客户端 VMO（>250B 大割幅一次成），应答 3B 定长无 inline。
pub const OP_TCP_DRAIN_VMO: u8 = 9;
/// TCP 会话往返 VMO 形（e 门续刀）：预填载荷的客户端 VMO 随行发送，应答
/// 4B 定长携 seq（K 窗流水线配对面——v1 无 seq 形不涉）。
pub const OP_TCP_RT_VMO: u8 = 10;

/// DRAIN_VMO 请求线上长度（`[op][sid u32 LE]`）。
pub const DRAIN_VMO_REQ_LEN: usize = 5;
/// RT_VMO 请求线上长度（`[op][dlen u16 LE][seq u8][sid u32 LE]`）。
pub const RT_VMO_REQ_LEN: usize = 8;
/// DRAIN_VMO 应答线上长度（`[st][dlen u16 LE]` 定长——非 OK 也 3B 恒零
/// 值，非 v1 的 1B 缩形）。
pub const VMO_REP_LEN: usize = 3;
/// RT_VMO 应答线上长度（`[st][dlen u16 LE][seq u8]`）。
pub const RT_VMO_REP_LEN: usize = 4;

/// vmo 应答形 st 域：v1 TCP 域（[`tcp_st_known`]）+ [`ST_NOSESSION`]——
/// 专用形不复用 v1 两形，死 sid 诊断位入域（Sid/Poll 会话族先例）。
const fn vmo_st_known(st: u8) -> bool {
    tcp_st_known(st) || st == ST_NOSESSION
}

/// 编码 DRAIN_VMO 请求（恰 [`DRAIN_VMO_REQ_LEN`] 5B；随行 1 handle 由通道
/// 层携带，协议层只定字节面）。None 契约：sid 0 / 越帽 / out 太短，不截断。
pub fn encode_drain_vmo_request(out: &mut [u8], sid: u32) -> Option<usize> {
    if sid == 0 || sid > TCP_SESSIONS_MAX {
        return None;
    }
    let m = out.get_mut(..DRAIN_VMO_REQ_LEN)?;
    m[0] = OP_TCP_DRAIN_VMO;
    m[1..5].copy_from_slice(&sid.to_le_bytes());
    Some(DRAIN_VMO_REQ_LEN)
}

/// 解析 DRAIN_VMO 请求：恰长执法（尾随/截断一并拒）；坏 op 首字节 /
/// sid 0 / 越帽 → None（与编码侧对称）。
pub fn parse_drain_vmo_request(buf: &[u8]) -> Option<u32> {
    if buf.len() != DRAIN_VMO_REQ_LEN {
        return None;
    }
    if buf[0] != OP_TCP_DRAIN_VMO {
        return None;
    }
    let sid = u32::from_le_bytes(buf[1..5].try_into().ok()?);
    if sid == 0 || sid > TCP_SESSIONS_MAX {
        return None;
    }
    Some(sid)
}

/// 编码 RT_VMO 请求（恰 [`RT_VMO_REQ_LEN`] 8B；随行 1 handle = 预填载荷
/// 的客户端 VMO，dlen 是 VMO 内载荷长度——VMO 面无 250B inline 帽，u16
/// 全域合法）。None 契约：dlen 0（RT 载荷非空族法）/ sid 0 / 越帽 /
/// out 太短。
pub fn encode_rt_vmo_request(out: &mut [u8], dlen: u16, seq: u8, sid: u32) -> Option<usize> {
    if dlen == 0 {
        return None;
    }
    if sid == 0 || sid > TCP_SESSIONS_MAX {
        return None;
    }
    let m = out.get_mut(..RT_VMO_REQ_LEN)?;
    m[0] = OP_TCP_RT_VMO;
    m[1..3].copy_from_slice(&dlen.to_le_bytes());
    m[3] = seq;
    m[4..8].copy_from_slice(&sid.to_le_bytes());
    Some(RT_VMO_REQ_LEN)
}

/// 解析 RT_VMO 请求：恰长 8B 执法（尾随/截断一并拒）；坏 op 首字节 /
/// dlen 0 / sid 0 / 越帽 → None（与编码侧对称）。
pub fn parse_rt_vmo_request(buf: &[u8]) -> Option<(u16, u8, u32)> {
    if buf.len() != RT_VMO_REQ_LEN {
        return None;
    }
    if buf[0] != OP_TCP_RT_VMO {
        return None;
    }
    let dlen = u16::from_le_bytes([buf[1], buf[2]]);
    let seq = buf[3];
    let sid = u32::from_le_bytes(buf[4..8].try_into().ok()?);
    if dlen == 0 {
        return None;
    }
    if sid == 0 || sid > TCP_SESSIONS_MAX {
        return None;
    }
    Some((dlen, seq, sid))
}

/// 编码 DRAIN_VMO 应答（恰 [`VMO_REP_LEN`] 3B 定长——非 OK 也 3B 恒零值；
/// 收割字节已在随行 VMO 内，无 inline）。烘焙线：域外 st → None；**非 OK
/// 携非零 dlen → None**（「超时了但带收割量」歧义形态不线上存在——DNS
/// addr 烘焙线同族）；OK dlen=0 合法（零收成诚实值）。out 太短 → None。
pub fn encode_tcp_reply_vmo(out: &mut [u8], st: u8, dlen: u16) -> Option<usize> {
    if !vmo_st_known(st) {
        return None;
    }
    if st != ST_OK && dlen != 0 {
        return None;
    }
    let m = out.get_mut(..VMO_REP_LEN)?;
    m[0] = st;
    m[1..3].copy_from_slice(&dlen.to_le_bytes());
    Some(VMO_REP_LEN)
}

/// 解析 DRAIN_VMO 应答：恰 3B 定长执法（尾随/截断/v1 1B 缩形一并拒）；
/// 域外 st / 非 OK 携非零 dlen → None（与编码侧对称）。
pub fn parse_tcp_reply_vmo(buf: &[u8]) -> Option<(u8, u16)> {
    if buf.len() != VMO_REP_LEN {
        return None;
    }
    let st = buf[0];
    if !vmo_st_known(st) {
        return None;
    }
    let dlen = u16::from_le_bytes([buf[1], buf[2]]);
    if st != ST_OK && dlen != 0 {
        return None;
    }
    Some((st, dlen))
}

/// 编码 RT_VMO 应答（恰 [`RT_VMO_REP_LEN`] 4B——seq 形 RT 配对；回显字节
/// 已写回随行 VMO，dlen = 回显长度）。烘焙线：域外 st → None；非 OK 携
/// 非零 dlen → None；**OK 携 dlen=0 → None**（RT 载荷非空族法的应答侧
/// 对称——v1 seq 形 2B 域外同族）；BUSY 背压携原 seq 合法（客户端同 seq
/// 退避重发）。out 太短 → None。
pub fn encode_rt_vmo_reply(out: &mut [u8], st: u8, dlen: u16, seq: u8) -> Option<usize> {
    if !vmo_st_known(st) {
        return None;
    }
    if st != ST_OK && dlen != 0 {
        return None;
    }
    if st == ST_OK && dlen == 0 {
        return None;
    }
    let m = out.get_mut(..RT_VMO_REP_LEN)?;
    m[0] = st;
    m[1..3].copy_from_slice(&dlen.to_le_bytes());
    m[3] = seq;
    Some(RT_VMO_REP_LEN)
}

/// 解析 RT_VMO 应答：恰 4B 定长执法；域外 st / 非 OK 携非零 dlen / OK 携
/// dlen=0 → None（与编码侧对称）。
pub fn parse_rt_vmo_reply(buf: &[u8]) -> Option<(u8, u16, u8)> {
    if buf.len() != RT_VMO_REP_LEN {
        return None;
    }
    let st = buf[0];
    if !vmo_st_known(st) {
        return None;
    }
    let dlen = u16::from_le_bytes([buf[1], buf[2]]);
    if st != ST_OK && dlen != 0 {
        return None;
    }
    if st == ST_OK && dlen == 0 {
        return None;
    }
    Some((st, dlen, buf[3]))
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

    /// dlen 帽：恰 TCP_DATA_MAX 收 / 0 拒 / +1 拒（编码/解析两侧对称）。
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

    /// 二刀几何钉：dlen=250 会话形恰满 CHAN_MSG_MAX（[op][dlen][250B]
    /// [sid u32] = 256）编解码往返；dlen=251 编码拒/解析拒（越帽）。
    #[test]
    fn tcp_s_dlen_250_fills_inline_cap() {
        let full = [5u8; TCP_DATA_MAX];
        assert_eq!(full.len(), 250);
        let mut out = [0u8; REQ_S_LEN_MAX];
        let n = encode_tcp_s_request(&mut out, TcpSReq::Roundtrip { data: &full }, 7).unwrap();
        assert_eq!(n, CHAN_MSG_MAX); // 会话形恰满：2+250+4=256
        assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Roundtrip { data: &full[..] }, 7, None))
        );
        let over = [5u8; TCP_DATA_MAX + 1];
        assert_eq!(encode_tcp_s_request(&mut out, TcpSReq::Roundtrip { data: &over }, 7), None);
        assert_eq!(parse_tcp_s_request(&[OP_TCP_ROUNDTRIP, 251]), None); // dlen 越帽
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
        let n = encode_tcp_reply(&mut out, ST_TIMEOUT, b"", None).unwrap();
        assert_eq!(&out[..n], &[ST_TIMEOUT]);
        let n = encode_tcp_reply(&mut out, ST_BUSY, b"", None).unwrap();
        assert_eq!(&out[..n], &[ST_BUSY]);
        let n = encode_tcp_reply(&mut out, ST_OK, b"abc", None).unwrap();
        assert_eq!(&out[..n], &[ST_OK, 3, b'a', b'b', b'c']);
    }

    /// 应答两形往返：1B `[st]`（OPEN/CLOSE/错误）与 `[st][dlen][data]`
    /// （仅 OK）；ST_BUSY 1B 形合法（TCP 面 st 域）。
    #[test]
    fn tcp_reply_roundtrip() {
        let mut out = [0u8; TCP_REP_MAX];
        let n = encode_tcp_reply(&mut out, ST_OK, b"", None).unwrap();
        assert_eq!(n, 1);
        assert_eq!(parse_tcp_reply(&out[..n]), Some(TcpReply { st: ST_OK, data: &[], seq: None }));

        let n = encode_tcp_reply(&mut out, ST_BUSY, b"", None).unwrap();
        assert_eq!(n, 1);
        assert_eq!(parse_tcp_reply(&out[..n]), Some(TcpReply { st: ST_BUSY, data: &[], seq: None }));

        // A3-3：NODEV 入域（deviceless serve 对 OPEN 也答此码——B4 拓扑
        // 真值；1B 形合法、非 OK 恒不携 data 对称面）。
        let n = encode_tcp_reply(&mut out, ST_NODEV, b"", None).unwrap();
        assert_eq!(n, 1);
        assert_eq!(parse_tcp_reply(&out[..n]), Some(TcpReply { st: ST_NODEV, data: &[], seq: None }));
        assert_eq!(encode_tcp_reply(&mut out, ST_NODEV, b"x", None), None);

        let n = encode_tcp_reply(&mut out, ST_OK, b"echo!", None).unwrap();
        assert_eq!(n, 2 + 5);
        assert_eq!(
            parse_tcp_reply(&out[..n]),
            Some(TcpReply { st: ST_OK, data: b"echo!", seq: None })
        );
    }

    /// 应答烘焙线：非 OK 携 data 编码拒 + 解析拒（DNS addr 烘焙线同族，
    /// 双侧对称）。
    #[test]
    fn tcp_reply_non_ok_empty_data_invariant() {
        let mut out = [0u8; TCP_REP_MAX];
        assert_eq!(encode_tcp_reply(&mut out, ST_TIMEOUT, b"x", None), None);
        assert_eq!(encode_tcp_reply(&mut out, ST_BUSY, b"x", None), None);
        assert_eq!(parse_tcp_reply(&[ST_TIMEOUT, 1, b'x']), None);
        assert_eq!(parse_tcp_reply(&[ST_BUSY, 1, b'x']), None);
    }

    /// 应答恰长/截断/2B 形（dlen=0 非法）/未知 st 拒。
    #[test]
    fn tcp_reply_shapes_rejected() {
        assert_eq!(parse_tcp_reply(&[]), None);
        assert_eq!(parse_tcp_reply(&[ST_OK, 2, b'a']), None); // dlen 与实长不符
        assert_eq!(parse_tcp_reply(&[ST_OK, 0]), None); // dlen=0（2B 形非法）
        assert_eq!(parse_tcp_reply(&[ST_OK, 1, b'a', 0, 0]), None); // 尾随（4B 的 [st][1][a][x] 自四刀起 = 合法 seq 形）
        assert_eq!(parse_tcp_reply(&[ST_OK, TCP_DATA_MAX as u8 + 1, b'a']), None); // dlen 越帽
        assert_eq!(parse_tcp_reply(&[0xFF]), None); // 未知 st
        assert_eq!(encode_tcp_reply(&mut [0u8; 8][..], 0xFF, b"", None), None);
    }

    /// 应答恰满 128B 帽收 + out 不足/空 out None 不截断。
    #[test]
    fn tcp_reply_cap_and_out_boundaries() {
        let full = [9u8; TCP_DATA_MAX];
        let mut out = [0u8; TCP_REP_MAX];
        let n = encode_tcp_reply(&mut out, ST_OK, &full, None).unwrap();
        assert_eq!(n, TCP_REP_MAX);
        match parse_tcp_reply(&out[..n]) {
            Some(TcpReply { st, data, seq: _ }) => {
                assert_eq!(st, ST_OK);
                assert_eq!(data, &full[..]);
            }
            other => panic!("cap-len reply must parse: {other:?}"),
        }
        let mut small = [0u8; 3];
        assert_eq!(encode_tcp_reply(&mut small, ST_OK, b"abcd", None), None);
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_tcp_reply(&mut empty, ST_OK, b"", None), None);
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
            assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Open { port: 18080 }, sid, None))
        );

            let n = encode_tcp_s_request(&mut out, TcpSReq::Roundtrip { data: b"abcd" }, sid).unwrap();
            assert_eq!(n, 6 + if sid == 0 { 0 } else { 4 });
            assert_eq!(
                parse_tcp_s_request(&out[..n]),
                Some((TcpSReq::Roundtrip { data: b"abcd" }, sid, None))
            );

            let n = encode_tcp_s_request(&mut out, TcpSReq::Close, sid).unwrap();
            assert_eq!(n, 1 + if sid == 0 { 0 } else { 4 });
            assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Close, sid, None))
        );
        }
        let n = encode_tcp_s_request(&mut out, TcpSReq::Listen { port: 18081 }, 0).unwrap();
        assert_eq!(n, 3);
        assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Listen { port: 18081 }, 0, None))
        );

        let n = encode_tcp_s_request(&mut out, TcpSReq::Accept, 5).unwrap();
        assert_eq!(n, 5);
        assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Accept, 5, None))
        );

        let n = encode_tcp_s_request(&mut out, TcpSReq::Poll, 0).unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Poll, 0, None))
        );
        let n = encode_tcp_s_request(&mut out, TcpSReq::Poll, 5).unwrap();
        assert_eq!(n, 5);
        assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Poll, 5, None))
        );
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
        assert_eq!(
            parse_tcp_s_request(&[OP_TCP_LISTEN, 0xFF, 0xFF]),
            Some((TcpSReq::Listen { port: u16::MAX }, 0, None))
        );
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
            assert_eq!(encode_tcp_reply(&mut out, st, b"", None), None);
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
            Some((TcpSReq::Roundtrip { data }, sid, seq)) => {
                assert_eq!(data, &full[..]);
                assert_eq!(sid, TCP_SESSIONS_MAX);
                assert_eq!(seq, None);
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
            assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Drain, sid, None))
        );
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

    // ---------- RT 请求序号形（四刀 P1）----------

    /// seq 形请求往返 + wire 钉板：`[op][dlen][data][seq u8]`——恰满 250B
    /// 载荷 = RT_SEQ_REQ_MAX 253 ≤ CHAN_MSG_MAX；尾随位/端序漂移在此炸响。
    #[test]
    fn rt_seq_request_roundtrip_and_wire_pinned() {
        let mut out = [0u8; RT_SEQ_REQ_MAX];
        let n = encode_rt_seq_request(&mut out, b"xy", 7).unwrap();
        assert_eq!(n, 3 + 2);
        assert_eq!(&out[..n], &[OP_TCP_ROUNDTRIP, 2, b'x', b'y', 7]);
        assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Roundtrip { data: b"xy" }, 0, Some(7)))
        );

        let full = [5u8; TCP_DATA_MAX];
        let n = encode_rt_seq_request(&mut out, &full, 0xAB).unwrap();
        assert_eq!(n, RT_SEQ_REQ_MAX); // 3+250 = 253 恰满
        assert_eq!(out[n - 1], 0xAB);
        assert_eq!(
            parse_tcp_s_request(&out[..n]),
            Some((TcpSReq::Roundtrip { data: &full[..] }, 0, Some(0xAB)))
        );

        // None 契约：空/越帽/out 太短。
        assert_eq!(encode_rt_seq_request(&mut out, b"", 1), None);
        let over = [5u8; TCP_DATA_MAX + 1];
        assert_eq!(encode_rt_seq_request(&mut out, &over, 1), None);
        let mut small = [0u8; 3];
        assert_eq!(encode_rt_seq_request(&mut small, b"abcd", 1), None);
        // seq 全域滚动（0 与 255 皆合法值——回绕距 256 ≫ K）。
        assert_eq!(encode_rt_seq_request(&mut out, b"a", 0).unwrap(), 4);
        assert_eq!(encode_rt_seq_request(&mut out, b"a", 255).unwrap(), 4);
    }

    /// seq 形应答往返 + wire 钉板：`[st][dlen][data][seq]`（仅 OK）与
    /// `[st][seq]` 2B 形（非 OK——ST_BUSY 背压必携原 seq）。
    #[test]
    fn rt_seq_reply_roundtrip_and_wire_pinned() {
        let mut out = [0u8; TCP_REP_MAX + 1];
        let n = encode_tcp_reply(&mut out, ST_OK, b"abc", Some(9)).unwrap();
        assert_eq!(n, 3 + 3);
        assert_eq!(&out[..n], &[ST_OK, 3, b'a', b'b', b'c', 9]);
        assert_eq!(
            parse_tcp_reply(&out[..n]),
            Some(TcpReply { st: ST_OK, data: b"abc", seq: Some(9) })
        );

        let n = encode_tcp_reply(&mut out, ST_BUSY, b"", Some(0x5A)).unwrap();
        assert_eq!(&out[..n], &[ST_BUSY, 0x5A]);
        assert_eq!(
            parse_tcp_reply(&out[..n]),
            Some(TcpReply { st: ST_BUSY, data: &[], seq: Some(0x5A) })
        );
        let n = encode_tcp_reply(&mut out, ST_TIMEOUT, b"", Some(255)).unwrap();
        assert_eq!(
            parse_tcp_reply(&out[..n]),
            Some(TcpReply { st: ST_TIMEOUT, data: &[], seq: Some(255) })
        );

        // 恰满：250B 回显 + seq = 253 ≤ CHAN_MSG_MAX。
        let full = [7u8; TCP_DATA_MAX];
        let n = encode_tcp_reply(&mut out, ST_OK, &full, Some(1)).unwrap();
        assert_eq!(n, 3 + TCP_DATA_MAX);
        match parse_tcp_reply(&out[..n]) {
            Some(TcpReply { st, data, seq }) => {
                assert_eq!(st, ST_OK);
                assert_eq!(data, &full[..]);
                assert_eq!(seq, Some(1));
            }
            other => panic!("cap-len seq reply must parse: {other:?}"),
        }

        // 烘焙线（seq 形同执法）：非 OK 携 data 拒 / OK 空数据 2B 形拒 /
        // 未知 st 拒 / dlen 0 或越帽拒 / 尾随截断拒。
        assert_eq!(encode_tcp_reply(&mut out, ST_TIMEOUT, b"x", Some(1)), None);
        assert_eq!(parse_tcp_reply(&[ST_TIMEOUT, 1, b'x', 1]), None);
        assert_eq!(encode_tcp_reply(&mut out, ST_OK, b"", Some(1)), None);
        assert_eq!(parse_tcp_reply(&[ST_OK, 0]), None); // v1 dlen=0 与 seq 0 形双拒
        assert_eq!(parse_tcp_reply(&[0xFF, 1]), None);
        assert_eq!(parse_tcp_reply(&[ST_OK, 251, b'a', 1]), None);
        assert_eq!(parse_tcp_reply(&[ST_OK, 2, b'a', 1, 0, 9]), None); // 尾随（5B 的 [st][2][a][x][y] = 合法 seq 形）
        assert_eq!(parse_tcp_reply(&[ST_OK, 2, b'a']), None); // 截断
        assert_eq!(parse_tcp_reply(&[]), None);
    }

    /// 三形互斥钉板（长度歧义消解）：v1 恰长 / −4B sid / −1B seq 对同一帧
    /// 至多一形可解——同字节 dlen 权威；「尾 4B 恰似合法 sid」的 seq 形帧
    /// （用户数据可构造）不被 sid 形误吞。
    #[test]
    fn rt_three_form_disambiguation() {
        // v1 形优先：2+dlen 恰长永不落尾随形。
        let v1 = [OP_TCP_ROUNDTRIP, 2, b'x', b'y'];
        assert_eq!(parse_tcp_s_request(&v1), Some((TcpSReq::Roundtrip { data: b"xy" }, 0, None)));
        assert_eq!(parse_tcp_reply(&[ST_OK, 2, b'x', b'y']), Some(TcpReply { st: ST_OK, data: b"xy", seq: None }));

        // sid 形：6+dlen（dlen 同字节）与 seq 形 3+dlen 恒不相等——同帧无双解。
        let mut f = [0u8; 10];
        f[0] = OP_TCP_ROUNDTRIP;
        f[1] = 4;
        f[2..6].copy_from_slice(b"abcd");
        f[6..10].copy_from_slice(&3u32.to_le_bytes());
        assert_eq!(
            parse_tcp_s_request(&f),
            Some((TcpSReq::Roundtrip { data: b"abcd" }, 3, None))
        );

        // 「尾 4B 恰似合法 sid」的 seq 形帧：[op][4][d0..d3][seq]，其中
        // d1..d3+seq = [k,0,0,0]（k ∈ 1..=8）——sid 形前缀解析失败必须落到
        // seq 形（`?` 早退误吞形态的单测钉板）。
        let mut g = [0u8; 7];
        g[0] = OP_TCP_ROUNDTRIP;
        g[1] = 4;
        g[2..6].copy_from_slice(&[0xAA, 5, 0, 0]);
        g[6] = 0;
        assert_eq!(
            parse_tcp_s_request(&g),
            Some((TcpSReq::Roundtrip { data: &[0xAA, 5, 0, 0] }, 0, Some(0)))
        );

        // legacy 面零接触：seq 形在 v1 解析器全拒（零漂移面的反向）。
        let mut out = [0u8; RT_SEQ_REQ_MAX];
        let n = encode_rt_seq_request(&mut out, b"xy", 5).unwrap();
        assert_eq!(parse_tcp_request(&out[..n]), None);
    }

    /// named RT 携 seq 的 257>256 几何阻死钉：编码面无此 API（结构上产
    /// 不出）；解析面对手搓 257B 帧（seq 形 + 4B 尾随 sid——超通道帽，线
    /// 上结构性不可达）恒 None——服务端面流水线届时先解通道帽再谈。
    #[test]
    fn rt_seq_plus_session_geometrically_blocked() {
        assert_eq!(RT_SEQ_REQ_MAX + 4, 257);
        assert!(RT_SEQ_REQ_MAX + 4 > CHAN_MSG_MAX);
        let full = [9u8; TCP_DATA_MAX];
        let mut f = [0u8; 257];
        f[0] = OP_TCP_ROUNDTRIP;
        f[1] = full.len() as u8;
        f[2..2 + full.len()].copy_from_slice(&full);
        f[2 + full.len()] = 0x42; // seq
        f[3 + full.len()..].copy_from_slice(&3u32.to_le_bytes()); // sid 尾随
        assert_eq!(parse_tcp_s_request(&f), None);
        // 应答面对称：TCP_REP_MAX + seq + sid 同超帽（253+1+4=258）。
        assert!(TCP_REP_MAX + 1 + 4 > CHAN_MSG_MAX);
    }

    /// RT_PIPE_K 几何钉（五刀③ 2→3）：K=3 保守案 750 ≤ min(TX_CAP,
    /// WINDOW)=1024 双界（环扩 8×128 与通告窗同值对齐，余 274B+2 槽；
    /// K=4 恰满零余量拒；vnet tcp.rs 消费侧 const assert 同值单源）；
    /// 回绕距 256 ≫ K。
    #[test]
    fn rt_pipe_k_geometry_pinned() {
        assert_eq!(RT_PIPE_K, 3);
        assert!(RT_PIPE_K * TCP_DATA_MAX <= 8 * 128);
        assert!(RT_PIPE_K < 256);
    }

    // ---------- TCP VMO 面（e 门续刀）----------

    /// 双 op 请求往返 + wire 钉板：drain_vmo `[op][sid LE]` 5B 与 rt_vmo
    /// `[op][dlen u16 LE][seq][sid LE]` 8B——端序/字段序漂移在此炸响；
    /// dlen/seq 全域边界与 sid 帽两端往返。
    #[test]
    fn vmo_request_roundtrip_and_wire_pinned() {
        let mut out = [0u8; RT_VMO_REQ_LEN];
        let n = encode_drain_vmo_request(&mut out, 5).unwrap();
        assert_eq!(n, DRAIN_VMO_REQ_LEN);
        assert_eq!(&out[..n], &[OP_TCP_DRAIN_VMO, 5, 0, 0, 0]);
        assert_eq!(parse_drain_vmo_request(&out[..n]), Some(5));

        // sid 帽两端（1 与 TCP_SESSIONS_MAX）往返。
        assert_eq!(encode_drain_vmo_request(&mut out, 1).unwrap(), DRAIN_VMO_REQ_LEN);
        assert_eq!(parse_drain_vmo_request(&out[..DRAIN_VMO_REQ_LEN]), Some(1));
        let n = encode_drain_vmo_request(&mut out, TCP_SESSIONS_MAX).unwrap();
        assert_eq!(parse_drain_vmo_request(&out[..n]), Some(TCP_SESSIONS_MAX));

        let n = encode_rt_vmo_request(&mut out, 0x1234, 7, 2).unwrap();
        assert_eq!(n, RT_VMO_REQ_LEN);
        assert_eq!(&out[..n], &[OP_TCP_RT_VMO, 0x34, 0x12, 7, 2, 0, 0, 0]);
        assert_eq!(parse_rt_vmo_request(&out[..n]), Some((0x1234, 7, 2)));

        // dlen u16 全域边界（1 与 MAX——VMO 面无 250 inline 帽）；seq 回绕
        // 全域（0 与 255 皆合法——v1 seq 形同法）。
        assert_eq!(encode_rt_vmo_request(&mut out, 1, 0, 1).unwrap(), RT_VMO_REQ_LEN);
        assert_eq!(parse_rt_vmo_request(&out), Some((1, 0, 1)));
        let n = encode_rt_vmo_request(&mut out, u16::MAX, 255, TCP_SESSIONS_MAX).unwrap();
        assert_eq!(
            &out[..n],
            &[OP_TCP_RT_VMO, 0xFF, 0xFF, 0xFF, TCP_SESSIONS_MAX as u8, 0, 0, 0]
        );
        assert_eq!(
            parse_rt_vmo_request(&out[..n]),
            Some((u16::MAX, 255, TCP_SESSIONS_MAX))
        );

        // out 不足/空 out：None 不截断。
        let mut small = [0u8; 4];
        assert_eq!(encode_drain_vmo_request(&mut small, 1), None);
        let mut small = [0u8; 7];
        assert_eq!(encode_rt_vmo_request(&mut small, 1, 1, 1), None);
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_drain_vmo_request(&mut empty, 1), None);
        assert_eq!(encode_rt_vmo_request(&mut empty, 1, 1, 1), None);
    }

    /// 双 op 请求野值/恰长执法：sid 0 与越帽双侧拒、rt dlen 0 拒（RT 载荷
    /// 非空族法）、坏 op 首字节拒、截断/尾随一并拒。
    #[test]
    fn vmo_request_wild_values_and_length_rejected() {
        let mut out = [0u8; RT_VMO_REQ_LEN];
        // sid 0：显式 sid 字段 = 命名会话专属（DRAIN 同法），编码产不出、
        // 解析双侧拒。
        assert_eq!(encode_drain_vmo_request(&mut out, 0), None);
        assert_eq!(parse_drain_vmo_request(&[OP_TCP_DRAIN_VMO, 0, 0, 0, 0]), None);
        assert_eq!(encode_rt_vmo_request(&mut out, 4, 1, 0), None);
        assert_eq!(parse_rt_vmo_request(&[OP_TCP_RT_VMO, 4, 0, 1, 0, 0, 0, 0]), None);
        // 越帽（编码 u32::MAX / 解析帽+1）
        assert_eq!(encode_drain_vmo_request(&mut out, u32::MAX), None);
        assert_eq!(
            parse_drain_vmo_request(&[OP_TCP_DRAIN_VMO, 0, 0, 0, TCP_SESSIONS_MAX as u8 + 1]),
            None
        );
        assert_eq!(encode_rt_vmo_request(&mut out, 4, 1, u32::MAX), None);
        assert_eq!(
            parse_rt_vmo_request(&[OP_TCP_RT_VMO, 4, 0, 1, 0, 0, 0, TCP_SESSIONS_MAX as u8 + 1]),
            None
        );
        // rt dlen 0 双侧拒。
        assert_eq!(encode_rt_vmo_request(&mut out, 0, 1, 1), None);
        assert_eq!(parse_rt_vmo_request(&[OP_TCP_RT_VMO, 0, 0, 1, 1, 0, 0, 0]), None);
        // 坏 op 首字节：对面 op 的合法帧互拒。
        assert_eq!(parse_drain_vmo_request(&[OP_TCP_RT_VMO, 1, 0, 0, 0]), None);
        assert_eq!(parse_rt_vmo_request(&[OP_TCP_DRAIN_VMO, 1, 0, 0, 0, 0, 0, 0]), None);
        // 恰长执法：截断/尾随一并拒（drain 5B / rt 8B 定长单形）。
        assert_eq!(parse_drain_vmo_request(&[]), None);
        assert_eq!(parse_drain_vmo_request(&[OP_TCP_DRAIN_VMO]), None);
        assert_eq!(parse_drain_vmo_request(&[OP_TCP_DRAIN_VMO, 2, 0, 0]), None);
        assert_eq!(parse_drain_vmo_request(&[OP_TCP_DRAIN_VMO, 2, 0, 0, 0, 0]), None); // 尾随 6B
        assert_eq!(parse_rt_vmo_request(&[OP_TCP_RT_VMO, 1, 0, 1, 1, 0, 0]), None); // 截断 7B
        assert_eq!(parse_rt_vmo_request(&[OP_TCP_RT_VMO, 1, 0, 1, 1, 0, 0, 0, 0]), None); // 尾随 9B
    }

    /// vmo 应答两形往返 + wire 钉板：3B `[st][dlen u16 LE]`（定长——非 OK
    /// 也 3B 恒零值，非 v1 的 1B 缩形）与 4B `[st][dlen LE][seq]`；OK
    /// dlen=0 域分：drain 合法（零收成）/ rt 拒（载荷非空族法）。
    #[test]
    fn vmo_reply_roundtrip_and_wire_pinned() {
        let mut out = [0u8; RT_VMO_REP_LEN];
        let n = encode_tcp_reply_vmo(&mut out, ST_OK, 1000).unwrap();
        assert_eq!(n, VMO_REP_LEN);
        assert_eq!(&out[..n], &[ST_OK, 0xE8, 0x03]); // 1000 = 0x03E8 LE
        assert_eq!(parse_tcp_reply_vmo(&out[..n]), Some((ST_OK, 1000)));

        // OK dlen=0 = 零收成诚实值（v1 DRAIN 1B OK 形同族——此形定长无缩形）。
        let n = encode_tcp_reply_vmo(&mut out, ST_OK, 0).unwrap();
        assert_eq!(&out[..n], &[ST_OK, 0, 0]);
        assert_eq!(parse_tcp_reply_vmo(&out[..n]), Some((ST_OK, 0)));

        // 非 OK 定长 3B 恒零值 + NOSESSION 入域（专用形解锁 v1 形锁死的
        // 死 sid 诊断位）。
        let n = encode_tcp_reply_vmo(&mut out, ST_NOSESSION, 0).unwrap();
        assert_eq!(&out[..n], &[ST_NOSESSION, 0, 0]);
        assert_eq!(parse_tcp_reply_vmo(&out[..n]), Some((ST_NOSESSION, 0)));
        let n = encode_tcp_reply_vmo(&mut out, ST_TIMEOUT, 0).unwrap();
        assert_eq!(parse_tcp_reply_vmo(&out[..n]), Some((ST_TIMEOUT, 0)));

        // dlen u16 上界往返。
        let n = encode_tcp_reply_vmo(&mut out, ST_OK, u16::MAX).unwrap();
        assert_eq!(&out[..n], &[ST_OK, 0xFF, 0xFF]);
        assert_eq!(parse_tcp_reply_vmo(&out[..n]), Some((ST_OK, u16::MAX)));

        // rt_vmo 形：OK 携 dlen LE + seq；非 OK 携原 seq（BUSY 背压配对
        // 面——v1 seq 形同法）。
        let n = encode_rt_vmo_reply(&mut out, ST_OK, 0x0100, 0x5A).unwrap();
        assert_eq!(n, RT_VMO_REP_LEN);
        assert_eq!(&out[..n], &[ST_OK, 0x00, 0x01, 0x5A]);
        assert_eq!(parse_rt_vmo_reply(&out[..n]), Some((ST_OK, 0x0100, 0x5A)));

        let n = encode_rt_vmo_reply(&mut out, ST_BUSY, 0, 0x42).unwrap();
        assert_eq!(&out[..n], &[ST_BUSY, 0, 0, 0x42]);
        assert_eq!(parse_rt_vmo_reply(&out[..n]), Some((ST_BUSY, 0, 0x42)));
        let n = encode_rt_vmo_reply(&mut out, ST_TIMEOUT, 0, 255).unwrap();
        assert_eq!(parse_rt_vmo_reply(&out[..n]), Some((ST_TIMEOUT, 0, 255)));

        // out 不足/空 out：None 不截断。
        let mut small = [0u8; 2];
        assert_eq!(encode_tcp_reply_vmo(&mut small, ST_OK, 1), None);
        let mut small = [0u8; 3];
        assert_eq!(encode_rt_vmo_reply(&mut small, ST_OK, 1, 1), None);
        let mut empty: [u8; 0] = [];
        assert_eq!(encode_tcp_reply_vmo(&mut empty, ST_OK, 0), None);
        assert_eq!(encode_rt_vmo_reply(&mut empty, ST_BAD, 0, 1), None);
    }

    /// vmo 应答烘焙线与形状执法：域外 st（未知值/EAGAIN/FULL）双侧拒；非
    /// OK 携非零 dlen 双侧拒；rt OK dlen=0 双侧拒；恰长（截断/尾随/1B 缩
    /// 形）拒。
    #[test]
    fn vmo_reply_baking_line_and_shapes() {
        let mut out = [0u8; RT_VMO_REP_LEN];
        // 未知 st / 域外 st（EAGAIN=非阻塞语义归 ACCEPT、FULL=造会话面）。
        assert_eq!(encode_tcp_reply_vmo(&mut out, 0xFF, 0), None);
        assert_eq!(parse_tcp_reply_vmo(&[0xFF, 0, 0]), None);
        assert_eq!(encode_tcp_reply_vmo(&mut out, ST_EAGAIN, 0), None);
        assert_eq!(encode_tcp_reply_vmo(&mut out, ST_FULL, 0), None);
        assert_eq!(parse_tcp_reply_vmo(&[ST_EAGAIN, 0, 0]), None);
        assert_eq!(encode_rt_vmo_reply(&mut out, ST_FULL, 0, 1), None);
        assert_eq!(parse_rt_vmo_reply(&[ST_EAGAIN, 0, 0, 1]), None);
        // 非 OK 携非零 dlen（「超时了但带收割量」歧义形态不线上存在）。
        assert_eq!(encode_tcp_reply_vmo(&mut out, ST_TIMEOUT, 1), None);
        assert_eq!(parse_tcp_reply_vmo(&[ST_TIMEOUT, 1, 0]), None);
        assert_eq!(encode_rt_vmo_reply(&mut out, ST_TIMEOUT, 1, 1), None);
        assert_eq!(parse_rt_vmo_reply(&[ST_TIMEOUT, 1, 0, 1]), None);
        // rt OK dlen=0（载荷非空族法的应答侧对称——v1 seq 形 2B 域外同族）。
        assert_eq!(encode_rt_vmo_reply(&mut out, ST_OK, 0, 1), None);
        assert_eq!(parse_rt_vmo_reply(&[ST_OK, 0, 0, 1]), None);
        // 恰长：drain 3B / rt 4B 定长单形，截断与尾随一并拒（非 OK 1B 缩
        // 形域外——v1 形不同款）。
        assert_eq!(parse_tcp_reply_vmo(&[]), None);
        assert_eq!(parse_tcp_reply_vmo(&[ST_BAD]), None); // v1 1B 非 OK 形非此面
        assert_eq!(parse_tcp_reply_vmo(&[ST_OK, 0xE8]), None);
        assert_eq!(parse_tcp_reply_vmo(&[ST_OK, 0xE8, 0x03, 0]), None); // 尾随
        assert_eq!(parse_rt_vmo_reply(&[]), None);
        assert_eq!(parse_rt_vmo_reply(&[ST_BUSY, 0, 0]), None); // 截断 3B
        assert_eq!(parse_rt_vmo_reply(&[ST_OK, 1, 0, 1, 0]), None); // 尾随 5B
    }

    /// 零漂移反向钉板：新二 op 请求在 v1/会话化解析器全拒（op 首字节 9/10
    /// 天然可分——旧消费者零接触）；op 取值钉板防意外重编号。应答两形不
    /// 经 v1 应答面消费（串行 RPC 按所发 op 选形——Sid/Poll 专用形同款，
    /// 此处只钉请求面）。
    #[test]
    fn vmo_new_ops_rejected_by_legacy_parsers() {
        let mut out = [0u8; RT_VMO_REQ_LEN];
        let n = encode_drain_vmo_request(&mut out, 3).unwrap();
        assert_eq!(parse_tcp_request(&out[..n]), None);
        assert_eq!(parse_tcp_s_request(&out[..n]), None);
        let n = encode_rt_vmo_request(&mut out, 16, 1, 3).unwrap();
        assert_eq!(parse_tcp_request(&out[..n]), None);
        assert_eq!(parse_tcp_s_request(&out[..n]), None);
        // op 域邻界钉板：8=DRAIN 旧面 / 9、10 新面。
        assert_eq!(OP_TCP_DRAIN, 8);
        assert_eq!(OP_TCP_DRAIN_VMO, 9);
        assert_eq!(OP_TCP_RT_VMO, 10);
    }
}
#[test]
    fn vmo_cross_face_coincidence_pinned() {
        // 双读巧合钉板（审查补）：3B/4B VMO 应答形与 v1 形的有意巧合冻成
        // 机器事实——防后人误当 bug 修掉或误加防御（跨面解析禁令靠串行
        // RPC 约定，巧合帧两读皆 Some 是设计现状）。
        let f3 = [ST_OK, 1, 0u8];
        assert!(parse_tcp_reply(&f3).is_some(), "3B 巧合:v1 读合法");
        assert!(parse_tcp_reply_vmo(&f3).is_some(), "3B 巧合:vmo 读合法");
        let f4 = [ST_OK, 1, 0u8, 5];
        assert!(parse_tcp_reply(&f4).is_some(), "4B 巧合:v1 seq 形读合法");
        assert!(parse_rt_vmo_reply(&f4).is_some(), "4B 巧合:rt_vmo 读合法");
    }

    