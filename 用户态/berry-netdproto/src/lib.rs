//! berry-netdproto —— 网络服务 RPC 协议纯函数库（P6 A-8，`berry-fsproto`
//! /`berry-blkproto` 先例：「解析器即产品」，零分配、零 unsafe、零依赖，
//! 输入只借用）。
//!
//! 拓扑：vnet（常驻网络驱动服务，`用户态/bins/vnet`）持通道一端（env 块
//! gift[2]，libc `gift2_h()`），svcmgr（A-8c 起消费自验方）持对端——**单对
//! 双向 + 严格串行 RPC**（一请求一应答，队列深 1 用量，无乱序死锁面；
//! fsmgr/vblk 同款约定）。本库只定字节布局与编解码，通道/等待语义在两端
//! 各自实现。
//!
//! **名字面走 inline 不走 VMO**：DNS 查询名 ≤ [`NAME_MAX`] = 64B ≪ channel
//! inline 帽 256B——与 berry-blkproto（512B 扇区超帽走 VMO）相反的取舍，
//! 无数据面即无随行 handle 面。地址应答 4B 同在 inline。
//!
//! 布局（v1，全定宽、无多字节数值字段——addr 是 IPv4 点分序四字节原序，
//! 无端序面）：
//! - 请求 `[op u8][nlen u8][name nlen B]` = 2+nlen（恰长，尾随字节 = 野值
//!   拒；nlen ∈ 1..=[`NAME_MAX`]，0 拒——空名非法定义在协议层不是调用方
//!   约定）
//! - 应答 `[st u8][addr 4B]` = [`REPLY_LEN`] 5 B（恰长，尾随拒）
//!
//! **应答烘焙线不变量**：未知 st 拒；**非 OK 状态必须携带全零 addr**（编
//! 码/解析两侧对称执法——线上不存在「超时了但带地址」的歧义形态，消费方
//! 无需防御性判断）。
//!
//! 错误面（截断/野值/尾随/未知码/空名/越帽名）host 单测全盖——「AI 测试友
//! 好化」：协议正确性不依赖 QEMU 就能验证。编码器 None 拒绝契约：编不出
//! 就拒绝、不截断（含空 out 切片——`berry-fsproto` 对抗审查实抓的同族坑，
//! day-one 盖）。

#![cfg_attr(not(test), no_std)]

/// DNS A 查询：name → 服务端发包解析，应答 addr。
pub const OP_DNS_QUERY: u8 = 1;

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

/// 名字长度帽（字节）。DNS wire 名 ≤255B（berry-netproto 执法），64 是
/// 服务面足够且请求帧 2+64=66B ≪ inline 帽 256B 的安全子集。
pub const NAME_MAX: usize = 64;

/// 请求线上长度上界（op + nlen + NAME_MAX）。
pub const REQ_LEN_MAX: usize = 2 + NAME_MAX;

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
        assert_eq!(n, REQ_LEN_MAX);
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
}
