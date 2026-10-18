//! berry-fsproto —— fs 服务 RPC 协议纯函数库（P6 A-1，`berry-loader` 先例：
//! 「解析器即产品」，零分配、零 unsafe、零依赖，输入只借用）。
//!
//! 拓扑：fsmgr（RAM-fs 服务，`用户态/bins/fsmgr`）持通道一端，bsh 持对端
//! dup（env 块 gift[3] 交付，见 svcmgr P6 段）——**单对双向**（send(A)→B 队
//! / send(B)→A 队）+ **严格串行 RPC**（一请求一应答，队列深 1 用量，无乱序
//! 死锁面）。本库只定字节布局与编解码，通道/等待语义在两端各自实现。
//!
//! 消息 = channel inline（≤256B），数据分片粒度 [`CHUNK`]=192（libc
//! `console_write`/berry-exec feeder 同款——inline 帽 256 留协议头余量）。
//! 全数值 LE。布局（v1，全定宽）：
//!
//! - 请求 `[op u8][nlen u8][name nlen B]…`：
//!   - PUT(1)：`…[off u32][dlen u8][data dlen B]`（≤CHUNK——写侧分片）
//!   - TRUNC(2)：`…[size u32]`（定长；size=0 兼 create）
//!   - GET(3)：`…[off u32]`（→ 应答 192B 窗 + more 链，读侧循环拼全量）
//!   - LIST(4)：恰 1 字节（尾随字节 = 野值拒）
//! - 应答 `[st u8]…`：st = [`ST_OK`]/[`ST_NOENT`]/[`ST_FULL`]/[`ST_BAD`]；
//!   - PUT/TRUNC 应答：恰 1 字节
//!   - GET 应答（st=0）：`[0][off u32][dlen u8][more u8][data dlen B]`
//!   - LIST 应答（st=0）：`[0][dlen u8][names dlen B]`（空白连接的名列）
//!
//! GET/GET 应答按**请求种类**各自解析（客户端知道自己发了什么——RPC 模型
//! 下无全局回复判别字；`parse_reply_get`/`parse_reply_list` 分立函数）。
//! 错误面（截断/野值/尾随字节）host 单测全盖——「AI 测试友好化」：协议
//! 正确性不依赖 QEMU 就能验证。

#![cfg_attr(not(test), no_std)]

/// PUT：分片写（client：192B 片循环 + 尾随 TRUNC 定长）。
pub const OP_PUT: u8 = 1;
/// TRUNC：定长（size=0 兼 create——空输出重定向 `echo > f` 的建文件面）。
pub const OP_TRUNC: u8 = 2;
/// GET：分片读（client：off += dlen 循环到 more=0）。
pub const OP_GET: u8 = 3;
/// LIST：名列（空白连接，≤200B 容纳截断）。
pub const OP_LIST: u8 = 4;

/// 应答状态：成功。
pub const ST_OK: u8 = 0;
/// 应答状态：目标不存在（GET）。
pub const ST_NOENT: u8 = 1;
/// 应答状态：帽尽（文件数 / 单文件尺寸）。
pub const ST_FULL: u8 = 2;
/// 应答状态：请求坏（解析失败——长度/野值/尾随字节）。
pub const ST_BAD: u8 = 3;

/// 数据分片粒度：单消息数据上限（channel inline 帽 256 − 协议头 7 − 名帽）。
pub const CHUNK: usize = 192;
/// 文件名帽（含 NUL 不允许——nlen 定界，名内字节原样透明）。
pub const NAME_MAX: usize = 48;
/// 单文件尺寸帽（服务端执法；客户端 GET 循环同值兜底 more 失控面）。
pub const FILE_SIZE_MAX: usize = 16 * 1024;
/// 文件数帽（服务端执法；静态堆 256KiB 预算内 8×16KiB=128KiB 稳态 + Vec
/// 增长 transient）。
pub const FILE_COUNT_MAX: usize = 8;
/// LIST 应答名区帽（8 文件 × 名帽最坏 392B 超单消息——按容纳截断，截断 =
/// 后续文件不列，调用方可见）。
pub const LIST_MAX: usize = 200;

fn put_u32(out: &mut [u8], at: usize, v: u32) {
    out[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn get_u32(m: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([m[at], m[at + 1], m[at + 2], m[at + 3]])
}

/// 请求编码（PUT）：失败 = 参数越帽（名空/名超 [`NAME_MAX`]/片超 [`CHUNK`]）
/// 或 out 容量不足——**编不出就拒绝，不截断**（截断的请求是语义静默错）。
pub fn encode_put(out: &mut [u8], name: &[u8], off: u32, data: &[u8]) -> Option<usize> {
    if name.is_empty() || name.len() > NAME_MAX || data.len() > CHUNK {
        return None;
    }
    let mut i = 2 + name.len();
    if i + 5 + data.len() > out.len() {
        return None;
    }
    out[0] = OP_PUT;
    out[1] = name.len() as u8;
    out[2..i].copy_from_slice(name);
    put_u32(out, i, off);
    i += 4;
    out[i] = data.len() as u8;
    i += 1;
    out[i..i + data.len()].copy_from_slice(data);
    Some(i + data.len())
}

/// 请求编码（TRUNC）。
pub fn encode_trunc(out: &mut [u8], name: &[u8], size: u32) -> Option<usize> {
    if name.is_empty() || name.len() > NAME_MAX {
        return None;
    }
    let i = 2 + name.len();
    if i + 4 > out.len() {
        return None;
    }
    out[0] = OP_TRUNC;
    out[1] = name.len() as u8;
    out[2..i].copy_from_slice(name);
    put_u32(out, i, size);
    Some(i + 4)
}

/// 请求编码（GET）。
pub fn encode_get(out: &mut [u8], name: &[u8], off: u32) -> Option<usize> {
    if name.is_empty() || name.len() > NAME_MAX {
        return None;
    }
    let i = 2 + name.len();
    if i + 4 > out.len() {
        return None;
    }
    out[0] = OP_GET;
    out[1] = name.len() as u8;
    out[2..i].copy_from_slice(name);
    put_u32(out, i, off);
    Some(i + 4)
}

/// 请求编码（LIST）：空 out → None（恰 1 字节即足——与其余编码器的
/// None 拒绝契约对齐，空切片不 panic；P6 A-1 对抗审查实抓）。
pub fn encode_list(out: &mut [u8]) -> Option<usize> {
    if out.is_empty() {
        return None;
    }
    out[0] = OP_LIST;
    Some(1)
}

/// 解析后的请求（服务端分派面）。
#[derive(Debug, PartialEq, Eq)]
pub enum Req<'a> {
    /// 分片写：off 起写入 data（off 超现长 = 零填充扩容，PUT 语义）。
    Put { name: &'a [u8], off: u32, data: &'a [u8] },
    /// 定长（size=0 兼 create）。
    Trunc { name: &'a [u8], size: u32 },
    /// 分片读。
    Get { name: &'a [u8], off: u32 },
    /// 名列。
    List,
}

/// 请求解析（服务端面）：空/未知 op/名空或超帽/字段截断/**尾随字节** → None
/// （调用方回 [`ST_BAD`]——畸形请求不猜语义）。
pub fn parse_request(m: &[u8]) -> Option<Req<'_>> {
    let op = *m.first()?;
    if op == OP_LIST {
        return if m.len() == 1 { Some(Req::List) } else { None };
    }
    if !(op == OP_PUT || op == OP_TRUNC || op == OP_GET) {
        return None;
    }
    let n = *m.get(1)? as usize;
    if n == 0 || n > NAME_MAX {
        return None;
    }
    let name = m.get(2..2 + n)?;
    let i = 2 + n;
    match op {
        OP_PUT => {
            // 长度前置检：off/dlen 字段整体缺席（名完整即止）也须拒——
            // get_u32 直接索引，先检后读（对读探针实锤：PUT 名后零字节曾 panic）。
            if m.len() < i + 5 {
                return None;
            }
            let off = get_u32(m, i);
            let dlen = m[i + 4] as usize;
            let data = m.get(i + 5..i + 5 + dlen)?;
            if i + 5 + dlen != m.len() || dlen > CHUNK {
                return None;
            }
            Some(Req::Put { name, off, data })
        }
        OP_TRUNC => {
            if i + 4 != m.len() {
                return None;
            }
            Some(Req::Trunc { name, size: get_u32(m, i) })
        }
        _ => {
            if i + 4 != m.len() {
                return None;
            }
            Some(Req::Get { name, off: get_u32(m, i) })
        }
    }
}

/// PUT/TRUNC 应答编码（恰 1 字节状态）：空 out → None（同 [`encode_list`]
/// ——「编不出就拒绝」契约全覆盖，调用方对定长缓冲 unwrap 即可）。
pub fn encode_reply_status(out: &mut [u8], st: u8) -> Option<usize> {
    if out.is_empty() {
        return None;
    }
    out[0] = st;
    Some(1)
}

/// GET 应答数据面。
#[derive(Debug, PartialEq, Eq)]
pub struct GetReply<'a> {
    pub off: u32,
    pub data: &'a [u8],
    /// 服务端还有更多数据（client：off += data.len() 续 GET）。
    pub more: bool,
}

/// GET 应答编码（st=0 面；off 回显请求 off——串行 RPC 下冗余，留作对账位）。
pub fn encode_reply_get(out: &mut [u8], off: u32, data: &[u8], more: bool) -> Option<usize> {
    if data.len() > CHUNK {
        return None;
    }
    if 7 + data.len() > out.len() {
        return None;
    }
    out[0] = ST_OK;
    put_u32(out, 1, off);
    out[5] = data.len() as u8;
    out[6] = more as u8;
    out[7..7 + data.len()].copy_from_slice(data);
    Some(7 + data.len())
}

/// GET 应答解析：`Ok(GetReply)` / `Err(st)`（服务端拒绝）；畸形 → None
/// （长度不符/截断——客户端按断链处理）。
pub fn parse_reply_get(m: &[u8]) -> Option<Result<GetReply<'_>, u8>> {
    let st = *m.first()?;
    if st != ST_OK {
        return if m.len() == 1 { Some(Err(st)) } else { None };
    }
    if m.len() < 7 {
        return None;
    }
    let dlen = m[5] as usize;
    if m.len() != 7 + dlen || dlen > CHUNK {
        return None;
    }
    Some(Ok(GetReply { off: get_u32(m, 1), data: &m[7..], more: m[6] != 0 }))
}

/// LIST 应答编码（st=0 面；names 超单消息容量 → None，服务端先按
/// [`LIST_MAX`] 截断再编码）。
pub fn encode_reply_list(out: &mut [u8], names: &[u8]) -> Option<usize> {
    if names.len() > 255 || 2 + names.len() > out.len() {
        return None;
    }
    out[0] = ST_OK;
    out[1] = names.len() as u8;
    out[2..2 + names.len()].copy_from_slice(names);
    Some(2 + names.len())
}

/// LIST 应答解析：`Ok(names)` / `Err(st)`；畸形 → None。
pub fn parse_reply_list(m: &[u8]) -> Option<Result<&[u8], u8>> {
    let st = *m.first()?;
    if st != ST_OK {
        return if m.len() == 1 { Some(Err(st)) } else { None };
    }
    let dlen = *m.get(1)? as usize;
    if m.len() != 2 + dlen {
        return None;
    }
    Some(Ok(&m[2..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_put(name: &[u8], off: u32, data: &[u8]) {
        let mut b = [0u8; 256];
        let n = encode_put(&mut b, name, off, data).unwrap();
        assert_eq!(
            parse_request(&b[..n]),
            Some(Req::Put { name, off, data })
        );
    }

    #[test]
    fn put_roundtrip() {
        round_put(b"a.bin", 0, b"hello");
        round_put(b"f", u32::MAX, &[]); // 空片合法（off 前伸）
    }

    #[test]
    fn put_full_chunk() {
        let data = [0x5a; CHUNK];
        round_put(b"full.bin", 192, &data); // 满片边界
    }

    #[test]
    fn put_caps_rejected() {
        let mut b = [0u8; 256];
        let long = [b'x'; NAME_MAX + 1];
        assert_eq!(encode_put(&mut b, &long, 0, b""), None); // 名超帽
        assert_eq!(encode_put(&mut b, b"", 0, b""), None); // 名空
        let big = [0u8; CHUNK + 1];
        assert_eq!(encode_put(&mut b, b"a", 0, &big), None); // 片超帽
    }

    #[test]
    fn out_too_small() {
        let mut tiny = [0u8; 4];
        assert_eq!(encode_put(&mut tiny, b"abcdefgh", 0, b""), None);
        assert_eq!(encode_trunc(&mut tiny, b"abcdefgh", 0), None);
        assert_eq!(encode_get(&mut tiny, b"abcdefgh", 0), None);
    }

    #[test]
    fn trunc_roundtrip() {
        let mut b = [0u8; 256];
        let n = encode_trunc(&mut b, b"h.hex", 2156).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Trunc { name: b"h.hex", size: 2156 }));
        let n = encode_trunc(&mut b, b"empty", 0).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Trunc { name: b"empty", size: 0 }));
    }

    #[test]
    fn get_roundtrip() {
        let mut b = [0u8; 256];
        let n = encode_get(&mut b, b"t.bin", 384).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Get { name: b"t.bin", off: 384 }));
    }

    #[test]
    fn list_roundtrip() {
        let mut b = [0u8; 256];
        let n = encode_list(&mut b).unwrap();
        assert_eq!(n, 1);
        assert_eq!(parse_request(&b[..n]), Some(Req::List));
    }

    #[test]
    fn request_malformed() {
        assert_eq!(parse_request(b""), None); // 空
        assert_eq!(parse_request(b"\x05"), None); // 未知 op
        assert_eq!(parse_request(b"\x01"), None); // 缺 nlen
        assert_eq!(parse_request(b"\x01\x03ab"), None); // 名截断
        assert_eq!(parse_request(b"\x01\x02ab"), None); // 名完整但字段零字节（panic 探针实锤后补）
        assert_eq!(parse_request(b"\x01\x01a\x00\x00\x00"), None); // off 4 字节缺 1
        assert_eq!(parse_request(b"\x01\x00"), None); // 名空（nlen=0）
        assert_eq!(parse_request(b"\x04\x00"), None); // LIST 尾随字节
        assert_eq!(parse_request(b"\x03\x02ab\x01"), None); // GET off 截断
        assert_eq!(parse_request(b"\x03\x02ab\x01\x02\x03"), None); // GET 尾随字节
        let long = [b'n'; NAME_MAX + 1];
        let mut m = vec![OP_GET, long.len() as u8];
        m.extend_from_slice(&long);
        m.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(parse_request(&m), None); // 名超帽
        assert_eq!(parse_request(b"\x02\x01a\x00\x00\x00\x00\x00"), None); // TRUNC 尾随
        assert_eq!(parse_request(b"\x01\x01a\x00\x00\x00\x00\x02xy\x00"), None); // PUT 尾随
    }

    #[test]
    fn put_dlen_mismatch() {
        // dlen 声 3 实给 2（与消息总长不符）→ 拒。
        let m = [OP_PUT, 1, b'a', 0, 0, 0, 0, 3, b'x', b'y'];
        assert_eq!(parse_request(&m), None);
        // dlen 超 CHUNK → 拒。
        let mut m = vec![OP_PUT, 1, b'a', 0, 0, 0, 0, CHUNK as u8 + 1];
        m.resize(7 + CHUNK + 1, 0);
        assert_eq!(parse_request(&m), None);
    }

    #[test]
    fn reply_status_roundtrip() {
        let mut b = [0u8; 8];
        assert_eq!(encode_reply_status(&mut b, ST_OK), Some(1));
        assert_eq!(b[0], ST_OK);
        assert_eq!(encode_reply_status(&mut b, ST_FULL), Some(1));
        assert_eq!(b[0], ST_FULL);
    }

    #[test]
    fn reply_get_roundtrip() {
        let mut b = [0u8; 256];
        let data = b"0123456789abcdef";
        let n = encode_reply_get(&mut b, 192, data, true).unwrap();
        match parse_reply_get(&b[..n]).unwrap() {
            Ok(g) => {
                assert_eq!(g.off, 192);
                assert_eq!(g.data, data);
                assert!(g.more);
            }
            Err(_) => panic!("expected ok"),
        }
        // 尾片：more=0 + 空片合法（文件尾/空文件面）。
        let n = encode_reply_get(&mut b, 0, &[], false).unwrap();
        match parse_reply_get(&b[..n]).unwrap() {
            Ok(g) => {
                assert_eq!(g.data, &[] as &[u8]);
                assert!(!g.more);
            }
            Err(_) => panic!("expected ok"),
        }
        // 错误应答：恰 1 字节 st。
        let n = encode_reply_status(&mut b, ST_NOENT).unwrap();
        assert_eq!(parse_reply_get(&b[..n]), Some(Err(ST_NOENT)));
    }

    #[test]
    fn reply_get_malformed() {
        assert_eq!(parse_reply_get(b""), None);
        assert_eq!(parse_reply_get(b"\x00\x01"), None); // 头截断
        assert_eq!(parse_reply_get(b"\x01\x00"), None); // 错误应答带尾随
        let mut b = [0u8; 256];
        let n = encode_reply_get(&mut b, 0, b"abc", false).unwrap();
        // dlen 声 3 实截 2。
        let mut bad = b[..n].to_vec();
        bad.truncate(n - 1);
        assert_eq!(parse_reply_get(&bad), None);
        // dlen 超 CHUNK。
        let mut m = vec![ST_OK, 0, 0, 0, 0, CHUNK as u8 + 1, 0];
        m.resize(7 + CHUNK + 1, 0);
        assert_eq!(parse_reply_get(&m), None);
    }

    #[test]
    fn reply_list_roundtrip() {
        let mut b = [0u8; 256];
        let names = b"a.bin h.hex t.bin";
        let n = encode_reply_list(&mut b, names).unwrap();
        assert_eq!(parse_reply_list(&b[..n]), Some(Ok(names.as_slice())));
        // 空名列合法（无文件）。
        let n = encode_reply_list(&mut b, b"").unwrap();
        assert_eq!(parse_reply_list(&b[..n]), Some(Ok(&[] as &[u8])));
        // 错误应答。
        let n = encode_reply_status(&mut b, ST_BAD).unwrap();
        assert_eq!(parse_reply_list(&b[..n]), Some(Err(ST_BAD)));
        // dlen 与总长不符。
        assert_eq!(parse_reply_list(b"\x00\x03ab"), None);
        // 错误应答带尾随字节（补盲区：错误面只测过恰 1 字节形态）。
        assert_eq!(parse_reply_list(b"\x01\x00"), None);
    }

    #[test]
    fn encode_reply_caps() {
        let mut b = [0u8; 256];
        let big = [0u8; CHUNK + 1];
        assert_eq!(encode_reply_get(&mut b, 0, &big, false), None); // 片超帽
        let names = [b'n'; 256];
        assert_eq!(encode_reply_list(&mut b, &names), None); // 超 dlen u8
    }

    #[test]
    fn out_exact_fit() {
        // out 恰好等于所需容量：`>` 判定下贴合应通过——容量门回归成 `>=`
        // 时本测显形（此前只测过小侧，P6 A-1 对抗审查补盲区）。
        let mut b = [0u8; 2 + 5 + 4]; // trunc "h.hex"：恰 11
        assert_eq!(encode_trunc(&mut b, b"h.hex", 7), Some(11));
        let mut b = [0u8; 2 + 1 + 4 + 1 + 2]; // put "a"+"hi"：恰 10
        assert_eq!(encode_put(&mut b, b"a", 0, b"hi"), Some(10));
        let mut b = [0u8; 7 + 3]; // reply_get "abc"：恰 10
        assert_eq!(encode_reply_get(&mut b, 0, b"abc", false), Some(10));
        let mut b = [0u8; 2 + 3]; // reply_list "abc"：恰 5
        assert_eq!(encode_reply_list(&mut b, b"abc"), Some(5));
        let mut b = [0u8; 1]; // list：恰 1
        assert_eq!(encode_list(&mut b), Some(1));
        let mut b = [0u8; 1]; // reply_status：恰 1
        assert_eq!(encode_reply_status(&mut b, ST_OK), Some(1));
    }

    #[test]
    fn out_empty_rejected() {
        // 空 out：全部编码器一律 None（encode_list/encode_reply_status 契约
        // 对齐后可测——空切片曾直接 panic，P6 A-1 对抗审查实抓）。
        assert_eq!(encode_list(&mut []), None);
        assert_eq!(encode_reply_status(&mut [], ST_OK), None);
        assert_eq!(encode_put(&mut [], b"a", 0, b""), None);
        assert_eq!(encode_trunc(&mut [], b"a", 0), None);
        assert_eq!(encode_get(&mut [], b"a", 0), None);
    }

    #[test]
    fn reply_get_more_wildcard() {
        // more 位野值（非 0/1，如 2）：解析面 `m[6] != 0` 判真（宽容面）。
        let m = [ST_OK, 0, 0, 0, 0, 1, 2, b'x'];
        let g = parse_reply_get(&m).unwrap().unwrap();
        assert!(g.more);
        assert_eq!(g.data, b"x");
    }

    #[test]
    fn wire_name_exact_max() {
        // 线上形态 nlen==NAME_MAX 恰满：接受路径（此前只测过 49 拒绝侧）。
        let name = [b'n'; NAME_MAX];
        let mut m = vec![OP_GET, NAME_MAX as u8];
        m.extend_from_slice(&name);
        m.extend_from_slice(&7u32.to_le_bytes());
        assert_eq!(
            parse_request(&m),
            Some(Req::Get { name: name.as_slice(), off: 7 })
        );
        // encode 侧同名恰满 roundtrip。
        let mut b = [0u8; 256];
        let n = encode_get(&mut b, &name, 7).unwrap();
        assert_eq!(
            parse_request(&b[..n]),
            Some(Req::Get { name: name.as_slice(), off: 7 })
        );
    }
}
