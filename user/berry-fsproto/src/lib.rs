//! berry-fsproto —— fs 服务 RPC 协议纯函数库（P6 A-1，`berry-loader` 先例：
//! 「解析器即产品」，零分配、零 unsafe、零依赖，输入只借用）。
//!
//! ## Overview (for crates.io users)
//!
//! Wire protocol for the [BerryOS](https://github.com/miuiadmin/BerryOS)
//! filesystem-service RPC: 10 ops (incl. v2 NUL-segment paths and
//! STATFS), strict request/reply byte layouts with a 4-byte
//! transaction id (txid) echoed in every reply. Pure no_std
//! encoder/parser, host-testable; protocol-only — channel/wait
//! semantics live at each end of the wire. "The parser is the
//! product": host unit tests cover the full error surface.
//!
//! Status: experimental — the byte layouts move with the BerryOS v1
//! services. Repo docs are mostly zh-CN.
//!
//! 拓扑：fsmgr（RAM-fs 服务，`user/bins/fsmgr`）持通道一端，bsh 持对端
//! dup（env 块 gift[3] 交付，见 svcmgr P6 段）——**单对双向**（send(A)→B 队
//! / send(B)→A 队）+ **严格串行 RPC**（一请求一应答，队列深 1 用量，无乱序
//! 死锁面）。本库只定字节布局与编解码，通道/等待语义在两端各自实现。
//!
//! **txid（P1-1 请求号）**：svcmgr 对同一 fs_a dup 多授（bsh/stdhello/
//! uu_* ktest 子/GUI app 共享同一应答队列），并发 client 交叠时应答可
//! 混流；协议级防线 = 公共头带 4 字节请求号——client 发请求时自增取号
//! （berry-fsclient `rpc`）、服务端应答原样回显（fsmgr）、client 收侧按
//! 号匹配（不匹配 = 他者/迟到应答，弃收续等）。0 保留（服务端对取不出
//! txid 的截断帧回显 0；真请求永不取 0，错配即弃）。注意 txid 是**甄别
//! 位不是路由位**：共享队列里被错收的他者应答弃收后其真主仍收不到
//! （丢失变超时——诚实失败优于静默错数据）；多 client 全并发须
//! per-client channel（v2 再议）。
//!
//! 消息 = channel inline（≤256B，内核 `CHAN_MSG_MAX` 同值），数据分片粒度
//! [`CHUNK`]=192（libc `console_write`/berry-exec feeder 同款——inline 帽
//! 256 留协议头余量）。全数值 LE。**公共头（双向同构）= `[op|st u8]
//! [txid u32]` 恰 [`HDR`]=5 字节**。布局（全定宽）：
//!
//! - 请求 `[op u8][txid u32][nlen u8][name nlen B]…`：
//!   - PUT(1)：`…[off u32][dlen u8][data dlen B]`（≤CHUNK——写侧分片）
//!   - TRUNC(2)：`…[size u32]`（定长；size=0 兼 create）
//!   - GET(3)：`…[off u32]`（→ 应答 192B 窗 + more 链，读侧循环拼全量）
//!   - LIST(4)：`[op][txid]` 恰 5 字节（尾随字节 = 野值拒）
//! - 应答 `[st u8][txid u32]…`：st = [`ST_OK`]/[`ST_NOENT`]/[`ST_FULL`]/[`ST_BAD`]；
//!   **错误应答（st≠0）全 op 同形 = 恰 5 字节**（st + 回显 txid）：
//!   - PUT/TRUNC 应答：恰 5 字节（成功面 = `[0][txid]`）
//!   - GET 应答（st=0）：`[0][txid][off u32][dlen u8][more u8][data dlen B]`
//!   - LIST 应答（st=0）：`[0][txid][dlen u8][names dlen B]`（空白连接的名列）
//!
//! GET/GET 应答按**请求种类**各自解析（客户端知道自己发了什么——RPC 模型
//! 下无全局回复判别字；`parse_reply_get`/`parse_reply_list` 分立函数）。
//! 错误面（截断/野值/尾随字节）host 单测全盖——「AI 测试友好化」：协议
//! 正确性不依赖 QEMU 就能验证。
//!
//! v2（C-0）：名域从「单名」升 **NUL 分段路径整串**（总长 ≤[`PATH_MAX`]、
//! 段数 ≤[`PATH_DEPTH_MAX`]、每段 ≤[`NAME_MAX`]，空段/首尾 NUL 拒——env
//! 块 argv 字节串先例）；**单段路径与 v1 名逐字节同形（D2 零迁移——txid
//! 头对两代一致追加，同形性保持）**，多段 NUL 串即新形态。新五 op 请求均
//! `[op][txid][nlen][path]` 恰好（无尾随字段）：
//! - LOOKUP(5)：存在性 → 状态应答（5 字节）
//! - MKDIR(6)：建目录 → 状态应答（父缺 = ST_NOENT / 目标已在（文件或目录
//!   同拒）= ST_EXISTS / 帽尽 = ST_FULL）
//! - UNLINK(7)：删文件/空目录 → 状态应答（非空目录 = [`ST_NOEMPTY`]）
//! - READDIR(8)：目录项列 → LIST 同形 `[0][txid][dlen][names]`（names =
//!   NUL 连接——段内无 NUL 由路径校验构造保证，可分性免检）
//! - STAT(9)：→ `[0][txid][type u8][size u32]` 恰 10 字节（type =
//!   [`TYPE_FILE`]/[`TYPE_DIR`]）
//!
//! C-2 刀 2：
//! - STATFS(10)：卷级几何/空闲真值（df/statfs 面）——**无路径参**（v1 单
//!   FAT 卷、卷级），请求恰 5 字节（LIST 同形，尾随字节 = 野值拒）；应答
//!   （st=0）：`[0][txid][total u32][spc u32][cluster_count u32]
//!   [free_clusters u32][sector_sz u32]` 恰 25 字节（[`STATFS_REPLY_LEN`]）。
//!
//! PUT/TRUNC/GET 名域同升路径（多段 = 子目录内文件——服务端树 C-0 刀 2）。
//! 严格串行 RPC、编码器 None 拒绝契约、帽常量双侧同步义务三纪律照旧；
//! txid 回显是第四纪律（P1-1：应答必带、原样回显、畸形帧取不出回 0）。
//!
//! **空路径串（nlen=0）= 根寻址**（C-1 刀 5 立法）：读面三 op（LOOKUP/
//! READDIR/STAT）专属——encode 侧放行、parse 侧白名单对偶；服务端 fsmgr
//! 根特判（LOOKUP=OK / READDIR=overlay 并合 / STAT=(TYPE_DIR, 根项数)）。
//! 写面与 GET 拒空不变（根不可建/删/写，也不是文件读定向）。

#![cfg_attr(not(test), no_std)]

/// PUT：分片写（client：192B 片循环 + 尾随 TRUNC 定长）。
pub const OP_PUT: u8 = 1;
/// TRUNC：定长（size=0 兼 create——空输出重定向 `echo > f` 的建文件面）。
pub const OP_TRUNC: u8 = 2;
/// GET：分片读（client：off += dlen 循环到 more=0）。
pub const OP_GET: u8 = 3;
/// LIST：名列（空白连接，≤200B 容纳截断）。
pub const OP_LIST: u8 = 4;
/// LOOKUP：存在性探询（v2 路径面）。
pub const OP_LOOKUP: u8 = 5;
/// MKDIR：建目录（父须存在）。
pub const OP_MKDIR: u8 = 6;
/// UNLINK：删文件/空目录。
pub const OP_UNLINK: u8 = 7;
/// READDIR：目录项列。
pub const OP_READDIR: u8 = 8;
/// STAT：类型 + 尺寸。
pub const OP_STAT: u8 = 9;
/// STATFS：卷级几何/空闲（df/statfs 面——无路径参，v1 单 FAT 卷、卷级）。
pub const OP_STATFS: u8 = 10;

/// 应答状态：成功。
pub const ST_OK: u8 = 0;
/// 应答状态：目标不存在（GET）。
pub const ST_NOENT: u8 = 1;
/// 应答状态：帽尽（文件数 / 单文件尺寸）。
pub const ST_FULL: u8 = 2;
/// 应答状态：请求坏（解析失败——长度/野值/尾随字节）。
pub const ST_BAD: u8 = 3;
/// 应答状态：目录非空（UNLINK 删非空目录的专属拒面——与 ST_BAD 分立，
/// 「请求合法但语义拒绝」与「请求畸形」不混谈）。
pub const ST_NOEMPTY: u8 = 4;
/// 应答状态：目标已存在（MKDIR 建已存在的文件/目录——刀 2 起在册；
/// 与 ST_BAD 分立同理：语义拒绝 ≠ 畸形请求）。
pub const ST_EXISTS: u8 = 5;

/// 公共头长（P1-1 起双向同构）：`[op|st u8][txid u32 LE]` 恰 5 字节——
/// 判别字 + 请求号。恰此 5 字节即完整消息的形态：LIST/STATFS 请求、
/// 全部错误应答（st≠0）、PUT/TRUNC 成功应答。
pub const HDR: usize = 5;

/// 数据分片粒度：单消息数据上限（channel inline 帽 256 − GET 应答头
/// 11 − 名帽 48 = 197 余量内的定值；PUT 请求 worst = 头 6 + 名 48 +
/// off/dlen 5 + 192 = 251 亦在帽内）。
pub const CHUNK: usize = 192;
/// 段名帽（路径单段长度上界；段内 NUL 天然不可——NUL 即分隔符。v1 名 =
/// 单段路径，同形同帽——D2 兼容根基）。
pub const NAME_MAX: usize = 48;
/// 路径总长帽（v2：NUL 分段整串含分隔符；nlen u8 上界 255 内的执法位）。
pub const PATH_MAX: usize = 128;
/// 路径深度帽（段数上限，根不计——`a\0b\0c` 深 3）。
pub const PATH_DEPTH_MAX: usize = 8;
/// 单文件尺寸帽（服务端执法；客户端 GET 循环同值兜底 more 失控面）。
pub const FILE_SIZE_MAX: usize = 16 * 1024;
/// 文件数帽（服务端执法；静态堆 256KiB 预算内 9×16KiB=144KiB 稳态 + Vec
/// 增长 transient）。C-2 刀 2 起 9 = 8 用户可用 + 1 启动预建保留槽
/// （fsmgr 预建 /etc/mtab 占 1——对抗审查实抓后保语义面：用户可用额
/// 不因系统自占用静默收窄）。
pub const FILE_COUNT_MAX: usize = 9;
/// LIST 应答名区帽（9 文件 × 名帽最坏 441B 超单消息——按容纳截断，截断 =
/// 后续文件不列，调用方可见）。
pub const LIST_MAX: usize = 200;
/// 目录数帽（刀 2：RAM overlay 树全局 Dir 节点计数，服务端执法——文件帽
/// 的目录同族：树深/广度有界，深树 Vec 开销不失控）。C-2 刀 2 起 17 =
/// 16 用户可用 + 1 启动预建保留槽（fsmgr 预建 etc 目录占 1）。
pub const DIR_COUNT_MAX: usize = 17;

/// STAT 应答类型位：文件。
pub const TYPE_FILE: u8 = 0;
/// STAT 应答类型位：目录。
pub const TYPE_DIR: u8 = 1;

/// v2 路径校验：NUL 分段整串。拒：空路径 / 总长超 [`PATH_MAX`] / 空段
/// （前导、尾随、连续 NUL）/ 任一段超 [`NAME_MAX`] / 段数超
/// [`PATH_DEPTH_MAX`]。单段（无 NUL）≤NAME_MAX 与 v1 名同形同规则。
/// （空路径的读面根寻址例外由调用方按 op 分流——本函数不知 op，恒拒空。）
fn path_valid(path: &[u8]) -> bool {
    if path.is_empty() || path.len() > PATH_MAX {
        return false;
    }
    let mut depth = 1usize;
    let mut seg = 0usize;
    for &b in path {
        if b == 0 {
            if seg == 0 || seg > NAME_MAX {
                return false; // 空段 / 段超帽（中段在其 NUL 边界判）
            }
            depth += 1;
            if depth > PATH_DEPTH_MAX {
                return false;
            }
            seg = 0;
        } else {
            seg += 1;
        }
    }
    seg != 0 && seg <= NAME_MAX // 末段非空（尾随 NUL 拒）且不超帽
}

fn put_u32(out: &mut [u8], at: usize, v: u32) {
    out[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn get_u32(m: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([m[at], m[at + 1], m[at + 2], m[at + 3]])
}

/// 公共头 txid 读取（请求/应答共用同偏移 1..5）：消息 <[`HDR`] 字节 →
/// None（截断帧取不出请求号）。两消费面：fsmgr 畸形请求回显（取不出回
/// 0）、berry-fsclient 收侧匹配（他者/迟到应答甄别）。纯偏移读——不判
/// op/st 合法性（判别面归 `parse_request`/`parse_reply_*`）。
pub fn hdr_txid(m: &[u8]) -> Option<u32> {
    (m.len() >= HDR).then(|| get_u32(m, 1))
}

/// 请求编码（PUT）：失败 = 路径坏（空/超帽/空段，[`path_valid`]）或片超
/// [`CHUNK`] 或 out 容量不足——**编不出就拒绝，不截断**（截断的请求是
/// 语义静默错）。`txid` = 请求号（P1-1，client 自增取号；应答原样回显）。
pub fn encode_put(out: &mut [u8], txid: u32, path: &[u8], off: u32, data: &[u8]) -> Option<usize> {
    if !path_valid(path) || data.len() > CHUNK {
        return None;
    }
    let mut i = HDR + 1 + path.len();
    if i + 5 + data.len() > out.len() {
        return None;
    }
    out[0] = OP_PUT;
    put_u32(out, 1, txid);
    out[HDR] = path.len() as u8;
    out[HDR + 1..i].copy_from_slice(path);
    put_u32(out, i, off);
    i += 4;
    out[i] = data.len() as u8;
    i += 1;
    out[i..i + data.len()].copy_from_slice(data);
    Some(i + data.len())
}

/// 请求编码（TRUNC）。
pub fn encode_trunc(out: &mut [u8], txid: u32, path: &[u8], size: u32) -> Option<usize> {
    if !path_valid(path) {
        return None;
    }
    let i = HDR + 1 + path.len();
    if i + 4 > out.len() {
        return None;
    }
    out[0] = OP_TRUNC;
    put_u32(out, 1, txid);
    out[HDR] = path.len() as u8;
    out[HDR + 1..i].copy_from_slice(path);
    put_u32(out, i, size);
    Some(i + 4)
}

/// 请求编码（GET）。
pub fn encode_get(out: &mut [u8], txid: u32, path: &[u8], off: u32) -> Option<usize> {
    if !path_valid(path) {
        return None;
    }
    let i = HDR + 1 + path.len();
    if i + 4 > out.len() {
        return None;
    }
    out[0] = OP_GET;
    put_u32(out, 1, txid);
    out[HDR] = path.len() as u8;
    out[HDR + 1..i].copy_from_slice(path);
    put_u32(out, i, off);
    Some(i + 4)
}

/// 请求编码（LIST）：out < [`HDR`] → None（恰 5 字节即足——与其余编码器
/// 的 None 拒绝契约对齐，短切片不 panic；P6 A-1 对抗审查实抓）。
pub fn encode_list(out: &mut [u8], txid: u32) -> Option<usize> {
    if out.len() < HDR {
        return None;
    }
    out[0] = OP_LIST;
    put_u32(out, 1, txid);
    Some(HDR)
}

/// v2 路径型 op 的请求编码共用：`[op][txid][nlen][path]` 恰好（无尾随
/// 字段）。`root_ok` = 空路径（nlen=0 → 根寻址）是否合法——**读面三 op**
/// （LOOKUP/STAT/READDIR）专属（C-1 刀 5 立法：libc `chdir("/")`/
/// `opendir("/")` 的根定向；服务端 fsmgr 对空 wire 特判根语义）；
/// 写面（MKDIR/UNLINK，及不走本函数的 PUT/TRUNC）拒空不变。
fn encode_path_op(out: &mut [u8], txid: u32, op: u8, path: &[u8], root_ok: bool) -> Option<usize> {
    if path.is_empty() {
        if !root_ok {
            return None;
        }
    } else if !path_valid(path) {
        return None;
    }
    if HDR + 1 + path.len() > out.len() {
        return None;
    }
    out[0] = op;
    put_u32(out, 1, txid);
    out[HDR] = path.len() as u8;
    out[HDR + 1..HDR + 1 + path.len()].copy_from_slice(path);
    Some(HDR + 1 + path.len())
}

/// 请求编码（LOOKUP）。
pub fn encode_lookup(out: &mut [u8], txid: u32, path: &[u8]) -> Option<usize> {
    encode_path_op(out, txid, OP_LOOKUP, path, true)
}

/// 请求编码（MKDIR）。
pub fn encode_mkdir(out: &mut [u8], txid: u32, path: &[u8]) -> Option<usize> {
    encode_path_op(out, txid, OP_MKDIR, path, false)
}

/// 请求编码（UNLINK）。
pub fn encode_unlink(out: &mut [u8], txid: u32, path: &[u8]) -> Option<usize> {
    encode_path_op(out, txid, OP_UNLINK, path, false)
}

/// 请求编码（READDIR）。
pub fn encode_readdir(out: &mut [u8], txid: u32, path: &[u8]) -> Option<usize> {
    encode_path_op(out, txid, OP_READDIR, path, true)
}

/// 请求编码（STAT）。
pub fn encode_stat(out: &mut [u8], txid: u32, path: &[u8]) -> Option<usize> {
    encode_path_op(out, txid, OP_STAT, path, true)
}

/// 解析后的请求（服务端分派面）。txid 不入枚举——服务端在分派前经
/// [`hdr_txid`] 单独取号（畸形请求也要回显，见 fsmgr）。
#[derive(Debug, PartialEq, Eq)]
pub enum Req<'a> {
    /// 分片写：off 起写入 data（off 超现长 = 零填充扩容，PUT 语义）。
    Put { path: &'a [u8], off: u32, data: &'a [u8] },
    /// 定长（size=0 兼 create）。
    Trunc { path: &'a [u8], size: u32 },
    /// 分片读。
    Get { path: &'a [u8], off: u32 },
    /// 名列（根；分层列目录用 [`Req::Readdir`]）。
    List,
    /// 存在性探询（v2 路径面）。
    Lookup { path: &'a [u8] },
    /// 建目录（父须存在）。
    Mkdir { path: &'a [u8] },
    /// 删文件/空目录。
    Unlink { path: &'a [u8] },
    /// 目录项列（names = NUL 连接，应答 LIST 同形）。
    Readdir { path: &'a [u8] },
    /// 类型 + 尺寸。
    Stat { path: &'a [u8] },
    /// 卷级几何/空闲（无路径参——v1 单 FAT 卷、卷级假定在拓扑层）。
    Statfs,
}

/// 请求解析（服务端面）：空/未知 op/头截断（<[`HDR`]）/路径坏（空/超帽/
/// 空段）/字段截断/**尾随字节** → None（调用方回 [`ST_BAD`]——畸形请求
/// 不猜语义；BAD 应答仍经 [`hdr_txid`] 回显取得出的 txid）。
pub fn parse_request(m: &[u8]) -> Option<Req<'_>> {
    let op = *m.first()?;
    if op == OP_LIST {
        return if m.len() == HDR { Some(Req::List) } else { None };
    }
    if op == OP_STATFS {
        // 恰 5 字节（无路径参——尾随字节 = 野值拒，LIST 同门）。
        return if m.len() == HDR { Some(Req::Statfs) } else { None };
    }
    if !(op == OP_PUT || op == OP_TRUNC || op == OP_GET
        || op == OP_LOOKUP || op == OP_MKDIR || op == OP_UNLINK
        || op == OP_READDIR || op == OP_STAT)
    {
        return None;
    }
    let n = *m.get(HDR)? as usize;
    // n 越界（名截断）→ None；n=0 = 根寻址（C-1 刀 5 立法）——读面三
    // op（LOOKUP/READDIR/STAT）白名单放行，写面（PUT/TRUNC/MKDIR/UNLINK）
    // 与 GET 维持 v1 拒面（根不是可定向的写/读对象）。
    let path = m.get(HDR + 1..HDR + 1 + n)?;
    if n == 0 {
        if !(op == OP_LOOKUP || op == OP_READDIR || op == OP_STAT) {
            return None;
        }
    } else if !path_valid(path) {
        return None;
    }
    let i = HDR + 1 + n;
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
            Some(Req::Put { path, off, data })
        }
        OP_TRUNC => {
            if i + 4 != m.len() {
                return None;
            }
            Some(Req::Trunc { path, size: get_u32(m, i) })
        }
        OP_GET => {
            if i + 4 != m.len() {
                return None;
            }
            Some(Req::Get { path, off: get_u32(m, i) })
        }
        // v2 路径型五 op：路径后恰消息尾（无尾随字段）。
        OP_LOOKUP => (i == m.len()).then_some(Req::Lookup { path }),
        OP_MKDIR => (i == m.len()).then_some(Req::Mkdir { path }),
        OP_UNLINK => (i == m.len()).then_some(Req::Unlink { path }),
        OP_READDIR => (i == m.len()).then_some(Req::Readdir { path }),
        _ => (i == m.len()).then_some(Req::Stat { path }),
    }
}

/// PUT/TRUNC 应答编码（恰 [`HDR`] 字节：st + 回显 txid）：out < [`HDR`] →
/// None（同 [`encode_list`]——「编不出就拒绝」契约全覆盖，调用方对定长
/// 缓冲 unwrap 即可）。
pub fn encode_reply_status(out: &mut [u8], txid: u32, st: u8) -> Option<usize> {
    if out.len() < HDR {
        return None;
    }
    out[0] = st;
    put_u32(out, 1, txid);
    Some(HDR)
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
/// txid 同回显（P1-1）。
pub fn encode_reply_get(
    out: &mut [u8],
    txid: u32,
    off: u32,
    data: &[u8],
    more: bool,
) -> Option<usize> {
    if data.len() > CHUNK {
        return None;
    }
    if 11 + data.len() > out.len() {
        return None;
    }
    out[0] = ST_OK;
    put_u32(out, 1, txid);
    put_u32(out, 5, off);
    out[9] = data.len() as u8;
    out[10] = more as u8;
    out[11..11 + data.len()].copy_from_slice(data);
    Some(11 + data.len())
}

/// GET 应答解析：`Ok(GetReply)` / `Err(st)`（服务端拒绝）；畸形 → None
/// （长度不符/截断——客户端按断链处理）。txid 匹配在会话层先经
/// [`hdr_txid`] 完成（ berry-fsclient `rpc`），解析面只管体。
pub fn parse_reply_get(m: &[u8]) -> Option<Result<GetReply<'_>, u8>> {
    let st = *m.first()?;
    if st != ST_OK {
        return if m.len() == HDR { Some(Err(st)) } else { None };
    }
    if m.len() < 11 {
        return None;
    }
    let dlen = m[9] as usize;
    if m.len() != 11 + dlen || dlen > CHUNK {
        return None;
    }
    Some(Ok(GetReply { off: get_u32(m, 5), data: &m[11..], more: m[10] != 0 }))
}

/// LIST 应答编码（st=0 面；names 超单消息容量 → None，服务端先按
/// [`LIST_MAX`] 截断再编码）。
pub fn encode_reply_list(out: &mut [u8], txid: u32, names: &[u8]) -> Option<usize> {
    if names.len() > 255 || 6 + names.len() > out.len() {
        return None;
    }
    out[0] = ST_OK;
    put_u32(out, 1, txid);
    out[5] = names.len() as u8;
    out[6..6 + names.len()].copy_from_slice(names);
    Some(6 + names.len())
}

/// LIST 应答解析：`Ok(names)` / `Err(st)`；畸形 → None。
pub fn parse_reply_list(m: &[u8]) -> Option<Result<&[u8], u8>> {
    let st = *m.first()?;
    if st != ST_OK {
        return if m.len() == HDR { Some(Err(st)) } else { None };
    }
    let dlen = *m.get(5)? as usize;
    if m.len() != 6 + dlen {
        return None;
    }
    Some(Ok(&m[6..]))
}

/// STAT 应答编码（st=0 面）：`[0][txid][type u8][size u32]` 恰 10 字节。
pub fn encode_reply_stat(out: &mut [u8], txid: u32, ty: u8, size: u32) -> Option<usize> {
    if out.len() < 10 {
        return None;
    }
    out[0] = ST_OK;
    put_u32(out, 1, txid);
    out[5] = ty;
    put_u32(out, 6, size);
    Some(10)
}

/// STAT 应答解析：`Ok((type, size))` / `Err(st)`（服务端拒绝）；畸形 →
/// None（长度不符——客户端按断链处理）。
pub fn parse_reply_stat(m: &[u8]) -> Option<Result<(u8, u32), u8>> {
    let st = *m.first()?;
    if st != ST_OK {
        return if m.len() == HDR { Some(Err(st)) } else { None };
    }
    if m.len() != 10 {
        return None;
    }
    Some(Ok((m[5], get_u32(m, 6))))
}

/// STATFS 应答 st=0 面的卷级真值字段（全 u32 LE；df/statfs 的原料——
/// 服务端从 BPB + FAT 扫描填充，客户端整形进 libc statfs 布局）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Statfs {
    /// 卷总扇区（BPB total 16/32 位合成）。
    pub total_sectors: u32,
    /// 每簇扇区。
    pub spc: u32,
    /// 数据区簇数（BPB 派生）。
    pub cluster_count: u32,
    /// 空闲簇数（FAT 扫描真值——非几何推导）。
    pub free_clusters: u32,
    /// 每扇区字节。
    pub sector_sz: u32,
}

/// STATFS 应答总长：公共头 + 五 u32（恰 25 字节——parse 长度执法的锚，
/// 「帽常量双侧同步义务」的服务端/客户端共用位）。
pub const STATFS_REPLY_LEN: usize = HDR + 4 * 5;

/// 请求编码（STATFS）：恰 [`HDR`] 字节（无路径参——v1 单 FAT 卷、卷级；
/// LIST 同形）。out < [`HDR`] → None（None 拒绝契约对齐）。
pub fn encode_statfs(out: &mut [u8], txid: u32) -> Option<usize> {
    if out.len() < HDR {
        return None;
    }
    out[0] = OP_STATFS;
    put_u32(out, 1, txid);
    Some(HDR)
}

/// STATFS 应答编码（st=0 面）：`[0][txid][total][spc][cluster_count][free]
/// [sector_sz]` 五 u32 LE 恰 [`STATFS_REPLY_LEN`] 字节。
pub fn encode_reply_statfs(out: &mut [u8], txid: u32, s: &Statfs) -> Option<usize> {
    if out.len() < STATFS_REPLY_LEN {
        return None;
    }
    out[0] = ST_OK;
    put_u32(out, 1, txid);
    put_u32(out, 5, s.total_sectors);
    put_u32(out, 9, s.spc);
    put_u32(out, 13, s.cluster_count);
    put_u32(out, 17, s.free_clusters);
    put_u32(out, 21, s.sector_sz);
    Some(STATFS_REPLY_LEN)
}

/// STATFS 应答解析：`Ok(Statfs)` / `Err(st)`（服务端拒绝——无卷面语义
/// 复用 [`ST_NOENT`]）；畸形 → None（长度不符——客户端按断链处理）。
pub fn parse_reply_statfs(m: &[u8]) -> Option<Result<Statfs, u8>> {
    let st = *m.first()?;
    if st != ST_OK {
        return if m.len() == HDR { Some(Err(st)) } else { None };
    }
    if m.len() != STATFS_REPLY_LEN {
        return None;
    }
    Some(Ok(Statfs {
        total_sectors: get_u32(m, 5),
        spc: get_u32(m, 9),
        cluster_count: get_u32(m, 13),
        free_clusters: get_u32(m, 17),
        sector_sz: get_u32(m, 21),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 txid 定值（非零非平凡——线形/回显钉板用；真发号器在
    /// berry-fsclient rpc，此处只验协议面透传）。
    const T: u32 = 0xA1_B2_C3_D4;

    fn round_put(name: &[u8], off: u32, data: &[u8]) {
        let mut b = [0u8; 256];
        let n = encode_put(&mut b, T, name, off, data).unwrap();
        assert_eq!(hdr_txid(&b[..n]), Some(T)); // txid 落位于请求头
        assert_eq!(
            parse_request(&b[..n]),
            Some(Req::Put { path: name, off, data })
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
        assert_eq!(encode_put(&mut b, T, &long, 0, b""), None); // 名超帽
        assert_eq!(encode_put(&mut b, T, b"", 0, b""), None); // 名空
        let big = [0u8; CHUNK + 1];
        assert_eq!(encode_put(&mut b, T, b"a", 0, &big), None); // 片超帽
    }

    #[test]
    fn out_too_small() {
        let mut tiny = [0u8; 4];
        assert_eq!(encode_put(&mut tiny, T, b"abcdefgh", 0, b""), None);
        assert_eq!(encode_trunc(&mut tiny, T, b"abcdefgh", 0), None);
        assert_eq!(encode_get(&mut tiny, T, b"abcdefgh", 0), None);
        // 公共头（5B）都容不下的 out：恰头长的编码器也拒。
        let mut nohdr = [0u8; HDR - 1];
        assert_eq!(encode_list(&mut nohdr, T), None);
        assert_eq!(encode_statfs(&mut nohdr, T), None);
        assert_eq!(encode_reply_status(&mut nohdr, T, ST_OK), None);
    }

    #[test]
    fn trunc_roundtrip() {
        let mut b = [0u8; 256];
        let n = encode_trunc(&mut b, T, b"h.hex", 2156).unwrap();
        assert_eq!(
            parse_request(&b[..n]),
            Some(Req::Trunc { path: b"h.hex", size: 2156 })
        );
        let n = encode_trunc(&mut b, T, b"empty", 0).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Trunc { path: b"empty", size: 0 }));
    }

    #[test]
    fn get_roundtrip() {
        let mut b = [0u8; 256];
        let n = encode_get(&mut b, T, b"t.bin", 384).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Get { path: b"t.bin", off: 384 }));
    }

    #[test]
    fn list_roundtrip() {
        let mut b = [0u8; 256];
        let n = encode_list(&mut b, T).unwrap();
        assert_eq!(n, HDR);
        let mut m = vec![OP_LIST];
        m.extend_from_slice(&T.to_le_bytes());
        assert_eq!(&b[..n], &m[..]); // 线形钉板：[op][txid LE] 恰 5 字节
        assert_eq!(parse_request(&b[..n]), Some(Req::List));
    }

    #[test]
    fn request_malformed() {
        let t = T.to_le_bytes();
        assert_eq!(parse_request(b""), None); // 空
        assert_eq!(parse_request(b"\x0b"), None); // 未知 op（11 号未分配——10 号随 C-2 刀 2 入册 STATFS）
        assert_eq!(parse_request(b"\x0b\x00\x00\x00\x00"), None); // 未知 op（带头同拒）
        assert_eq!(parse_request(b"\x05"), None); // LOOKUP 头截断（txid 缺）
        assert_eq!(parse_request(&[OP_LOOKUP, t[0], t[1], t[2], t[3]]), None); // 头完整缺 nlen
        assert_eq!(parse_request(b"\x01"), None); // 缺头
        assert_eq!(
            parse_request(&[OP_PUT, t[0], t[1], t[2], t[3], 3, b'a', b'b']),
            None
        ); // 名截断
        assert_eq!(
            parse_request(&[OP_PUT, t[0], t[1], t[2], t[3], 2, b'a', b'b']),
            None
        ); // 名完整但字段零字节（panic 探针实锤后补）
        assert_eq!(
            parse_request(&[OP_PUT, t[0], t[1], t[2], t[3], 1, b'a', 0, 0, 0]),
            None
        ); // off 4 字节缺 1
        assert_eq!(parse_request(&[OP_PUT, t[0], t[1], t[2], t[3], 0]), None); // 名空（nlen=0）
        assert_eq!(parse_request(&[OP_LIST, t[0], t[1], t[2], t[3], 0]), None); // LIST 尾随字节
        assert_eq!(
            parse_request(&[OP_GET, t[0], t[1], t[2], t[3], 2, b'a', b'b', 1]),
            None
        ); // GET off 截断
        assert_eq!(
            parse_request(&[OP_GET, t[0], t[1], t[2], t[3], 2, b'a', b'b', 1, 2, 3, 4, 5]),
            None
        ); // GET 尾随字节
        let long = [b'n'; NAME_MAX + 1];
        let mut m = vec![OP_PUT];
        m.extend_from_slice(&t);
        m.push(long.len() as u8);
        m.extend_from_slice(&long);
        m.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(parse_request(&m), None); // 名超帽
        assert_eq!(
            parse_request(&[OP_TRUNC, t[0], t[1], t[2], t[3], 1, b'a', 0, 0, 0, 0, 0]),
            None
        ); // TRUNC 尾随
        assert_eq!(
            parse_request(&[OP_PUT, t[0], t[1], t[2], t[3], 1, b'a', 0, 0, 0, 0, 2, b'x', b'y', 0]),
            None
        ); // PUT 尾随
    }

    #[test]
    fn put_dlen_mismatch() {
        let t = T.to_le_bytes();
        // dlen 声 3 实给 2（与消息总长不符）→ 拒。
        let m = [OP_PUT, t[0], t[1], t[2], t[3], 1, b'a', 0, 0, 0, 0, 3, b'x', b'y'];
        assert_eq!(parse_request(&m), None);
        // dlen 超 CHUNK → 拒。
        let mut m = vec![OP_PUT];
        m.extend_from_slice(&t);
        m.extend_from_slice(&[1, b'a']);
        m.extend_from_slice(&0u32.to_le_bytes());
        m.push(CHUNK as u8 + 1);
        m.resize(HDR + 1 + 1 + 4 + 1 + CHUNK + 1, 0);
        assert_eq!(parse_request(&m), None);
    }

    #[test]
    fn reply_status_roundtrip() {
        let mut b = [0u8; 8];
        assert_eq!(encode_reply_status(&mut b, T, ST_OK), Some(HDR));
        let mut m = vec![ST_OK];
        m.extend_from_slice(&T.to_le_bytes());
        assert_eq!(&b[..HDR], &m[..]); // [st][txid LE] 恰 5 字节
        assert_eq!(encode_reply_status(&mut b, T, ST_FULL), Some(HDR));
        assert_eq!(b[0], ST_FULL);
    }

    #[test]
    fn reply_get_roundtrip() {
        let mut b = [0u8; 256];
        let data = b"0123456789abcdef";
        let n = encode_reply_get(&mut b, T, 192, data, true).unwrap();
        assert_eq!(hdr_txid(&b[..n]), Some(T));
        match parse_reply_get(&b[..n]).unwrap() {
            Ok(g) => {
                assert_eq!(g.off, 192);
                assert_eq!(g.data, data);
                assert!(g.more);
            }
            Err(_) => panic!("expected ok"),
        }
        // 尾片：more=0 + 空片合法（文件尾/空文件面）。
        let n = encode_reply_get(&mut b, T, 0, &[], false).unwrap();
        match parse_reply_get(&b[..n]).unwrap() {
            Ok(g) => {
                assert_eq!(g.data, &[] as &[u8]);
                assert!(!g.more);
            }
            Err(_) => panic!("expected ok"),
        }
        // 错误应答：[st][txid] 恰 5 字节。
        let n = encode_reply_status(&mut b, T, ST_NOENT).unwrap();
        assert_eq!(parse_reply_get(&b[..n]), Some(Err(ST_NOENT)));
    }

    #[test]
    fn reply_get_malformed() {
        assert_eq!(parse_reply_get(b""), None);
        assert_eq!(parse_reply_get(b"\x00\x01"), None); // 头截断
        assert_eq!(parse_reply_get(b"\x01\x00"), None); // 错误应答带尾随
        let mut b = [0u8; 256];
        let n = encode_reply_get(&mut b, T, 0, b"abc", false).unwrap();
        // dlen 声 3 实截 2。
        let mut bad = b[..n].to_vec();
        bad.truncate(n - 1);
        assert_eq!(parse_reply_get(&bad), None);
        // dlen 超 CHUNK。
        let mut m = vec![ST_OK];
        m.extend_from_slice(&T.to_le_bytes());
        m.extend_from_slice(&0u32.to_le_bytes()); // off
        m.push(CHUNK as u8 + 1); // dlen
        m.push(0); // more
        m.resize(11 + CHUNK + 1, 0);
        assert_eq!(parse_reply_get(&m), None);
    }

    #[test]
    fn reply_list_roundtrip() {
        let mut b = [0u8; 256];
        let names = b"a.bin h.hex t.bin";
        let n = encode_reply_list(&mut b, T, names).unwrap();
        assert_eq!(hdr_txid(&b[..n]), Some(T));
        assert_eq!(parse_reply_list(&b[..n]), Some(Ok(names.as_slice())));
        // 空名列合法（无文件）。
        let n = encode_reply_list(&mut b, T, b"").unwrap();
        assert_eq!(parse_reply_list(&b[..n]), Some(Ok(&[] as &[u8])));
        // 错误应答。
        let n = encode_reply_status(&mut b, T, ST_BAD).unwrap();
        assert_eq!(parse_reply_list(&b[..n]), Some(Err(ST_BAD)));
        // dlen 与总长不符。
        assert_eq!(parse_reply_list(&[0u8, 0, 0, 0, 0, 3, b'a', b'b']), None);
        // 错误应答带尾随字节（补盲区：错误面只测过恰 5 字节形态）。
        assert_eq!(parse_reply_list(b"\x01\x00"), None);
    }

    #[test]
    fn encode_reply_caps() {
        let mut b = [0u8; 256];
        let big = [0u8; CHUNK + 1];
        assert_eq!(encode_reply_get(&mut b, T, 0, &big, false), None); // 片超帽
        let names = [b'n'; 256];
        assert_eq!(encode_reply_list(&mut b, T, &names), None); // 超 dlen u8
    }

    #[test]
    fn out_exact_fit() {
        // out 恰好等于所需容量：`>` 判定下贴合应通过——容量门回归成 `>=`
        // 时本测显形（此前只测过小侧，P6 A-1 对抗审查补盲区）。
        let mut b = [0u8; HDR + 1 + 5 + 4]; // trunc "h.hex"：恰 15
        assert_eq!(encode_trunc(&mut b, T, b"h.hex", 7), Some(15));
        let mut b = [0u8; HDR + 1 + 1 + 4 + 1 + 2]; // put "a"+"hi"：恰 14
        assert_eq!(encode_put(&mut b, T, b"a", 0, b"hi"), Some(14));
        let mut b = [0u8; 11 + 3]; // reply_get "abc"：恰 14
        assert_eq!(encode_reply_get(&mut b, T, 0, b"abc", false), Some(14));
        let mut b = [0u8; 6 + 3]; // reply_list "abc"：恰 9
        assert_eq!(encode_reply_list(&mut b, T, b"abc"), Some(9));
        let mut b = [0u8; HDR]; // list：恰 5
        assert_eq!(encode_list(&mut b, T), Some(5));
        let mut b = [0u8; HDR]; // reply_status：恰 5
        assert_eq!(encode_reply_status(&mut b, T, ST_OK), Some(5));
        let mut b = [0u8; HDR + 1 + 3]; // lookup "a\0b"：恰 9
        assert_eq!(encode_lookup(&mut b, T, b"a\0b"), Some(9));
        let mut b = [0u8; 10]; // reply_stat：恰 10
        assert_eq!(encode_reply_stat(&mut b, T, TYPE_FILE, 1), Some(10));
        let mut b = [0u8; HDR]; // statfs：恰 5
        assert_eq!(encode_statfs(&mut b, T), Some(5));
        let mut b = [0u8; STATFS_REPLY_LEN]; // reply_statfs：恰 25
        assert_eq!(
            encode_reply_statfs(&mut b, T, &zero_statfs()),
            Some(STATFS_REPLY_LEN)
        );
    }

    #[test]
    fn out_empty_rejected() {
        // 空 out：全部编码器一律 None（encode_list/encode_reply_status 契约
        // 对齐后可测——空切片曾直接 panic，P6 A-1 对抗审查实抓）。
        assert_eq!(encode_list(&mut [], T), None);
        assert_eq!(encode_reply_status(&mut [], T, ST_OK), None);
        assert_eq!(encode_put(&mut [], T, b"a", 0, b""), None);
        assert_eq!(encode_trunc(&mut [], T, b"a", 0), None);
        assert_eq!(encode_get(&mut [], T, b"a", 0), None);
        assert_eq!(encode_lookup(&mut [], T, b"a"), None);
        assert_eq!(encode_mkdir(&mut [], T, b"a"), None);
        assert_eq!(encode_unlink(&mut [], T, b"a"), None);
        assert_eq!(encode_readdir(&mut [], T, b"a"), None);
        assert_eq!(encode_stat(&mut [], T, b"a"), None);
        assert_eq!(encode_reply_stat(&mut [], T, TYPE_FILE, 0), None);
        assert_eq!(encode_statfs(&mut [], T), None);
        assert_eq!(encode_reply_statfs(&mut [], T, &zero_statfs()), None);
    }

    #[test]
    fn reply_get_more_wildcard() {
        // more 位野值（非 0/1，如 2）：解析面 `m[10] != 0` 判真（宽容面）。
        let mut m = vec![ST_OK];
        m.extend_from_slice(&T.to_le_bytes());
        m.extend_from_slice(&0u32.to_le_bytes()); // off
        m.extend_from_slice(&[1, 2, b'x']); // dlen=1, more=2（野值）, data
        let g = parse_reply_get(&m).unwrap().unwrap();
        assert!(g.more);
        assert_eq!(g.data, b"x");
    }

    #[test]
    fn wire_name_exact_max() {
        // 线上形态 nlen==NAME_MAX 恰满：接受路径（此前只测过 49 拒绝侧）。
        let name = [b'n'; NAME_MAX];
        let mut m = vec![OP_GET];
        m.extend_from_slice(&T.to_le_bytes());
        m.push(NAME_MAX as u8);
        m.extend_from_slice(&name);
        m.extend_from_slice(&7u32.to_le_bytes());
        assert_eq!(
            parse_request(&m),
            Some(Req::Get { path: name.as_slice(), off: 7 })
        );
        // encode 侧同名恰满 roundtrip。
        let mut b = [0u8; 256];
        let n = encode_get(&mut b, T, &name, 7).unwrap();
        assert_eq!(
            parse_request(&b[..n]),
            Some(Req::Get { path: name.as_slice(), off: 7 })
        );
    }

    // ---- v2 路径面（C-0 刀 1）----

    fn path_8seg() -> Vec<u8> {
        // 深度恰 8（PATH_DEPTH_MAX）：s1\0s2\0…\0s8。
        let segs: [&[u8]; PATH_DEPTH_MAX] =
            [b"s1", b"s2", b"s3", b"s4", b"s5", b"s6", b"s7", b"s8"];
        segs.join(&0u8)
    }

    #[test]
    fn path_ops_roundtrip() {
        let mut b = [0u8; 256];
        let p = b"d\0sub\0f.bin";
        let n = encode_lookup(&mut b, T, p).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Lookup { path: p }));
        let n = encode_mkdir(&mut b, T, b"d").unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Mkdir { path: b"d" }));
        let n = encode_unlink(&mut b, T, p).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Unlink { path: p }));
        let n = encode_readdir(&mut b, T, b"d\0sub").unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Readdir { path: b"d\0sub" }));
        let n = encode_stat(&mut b, T, p).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Stat { path: p }));
        // 深度恰 8 边界成立。
        let deep = path_8seg();
        let n = encode_lookup(&mut b, T, &deep).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Lookup { path: deep.as_slice() }));
    }

    #[test]
    fn path_depth_rejected() {
        let mut b = [0u8; 256];
        let mut deep9 = path_8seg();
        deep9.extend_from_slice(b"\0s9"); // 9 段
        assert_eq!(encode_lookup(&mut b, T, &deep9), None);
        // 线面伪造同拒：parse 侧 path_valid 同门。
        let mut m = vec![OP_STAT];
        m.extend_from_slice(&T.to_le_bytes());
        m.push(deep9.len() as u8);
        m.extend_from_slice(&deep9);
        assert_eq!(parse_request(&m), None);
    }

    #[test]
    fn path_len_boundary() {
        let mut b = [0u8; 256];
        // 总长恰 128 = PATH_MAX：48 + NUL + 48 + NUL + 30（段帽内三段）。
        let mut p: Vec<u8> = Vec::new();
        p.extend_from_slice(&[b'x'; NAME_MAX]);
        p.push(0);
        p.extend_from_slice(&[b'y'; NAME_MAX]);
        p.push(0);
        p.extend_from_slice(&[b'z'; 30]);
        assert_eq!(p.len(), PATH_MAX);
        let n = encode_mkdir(&mut b, T, &p).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Mkdir { path: p.as_slice() }));
        // 超 1 字节 → 拒。
        p.push(b'w');
        assert_eq!(p.len(), PATH_MAX + 1);
        assert_eq!(encode_mkdir(&mut b, T, &p), None);
    }

    #[test]
    fn path_empty_segment_rejected() {
        let mut b = [0u8; 256];
        // 空路径在 LOOKUP = 根寻址（刀 5 立法）——非空串的空段拒面照旧。
        assert_eq!(encode_lookup(&mut b, T, b"\0a"), None); // 前导 NUL（首段空）
        assert_eq!(encode_lookup(&mut b, T, b"a\0"), None); // 尾随 NUL（末段空）
        assert_eq!(encode_lookup(&mut b, T, b"a\0\0b"), None); // 连续 NUL（中间空段）
    }

    #[test]
    fn root_wire_read_ops_only() {
        // 空 wire（nlen=0）= 根寻址（C-1 刀 5 立法）：读面三 op 专属——
        // encode 放行 `[op][txid][0]` 恰 6 字节 + parse 白名单对偶 roundtrip；
        // 写面（MKDIR/UNLINK）与 GET/PUT/TRUNC 拒空钉死（根不可建/删/写，
        // 也不是文件读定向）。
        let mut b = [0u8; 256];
        let mut wire = vec![0u8]; // op 占位（循环内覆写）
        wire.extend_from_slice(&T.to_le_bytes());
        wire.push(0); // nlen=0（根寻址）
        for (enc, op, want) in [
            (encode_lookup as fn(&mut [u8], u32, &[u8]) -> Option<usize>, OP_LOOKUP, Req::Lookup { path: b"" }),
            (encode_readdir as fn(&mut [u8], u32, &[u8]) -> Option<usize>, OP_READDIR, Req::Readdir { path: b"" }),
            (encode_stat as fn(&mut [u8], u32, &[u8]) -> Option<usize>, OP_STAT, Req::Stat { path: b"" }),
        ] {
            assert_eq!(enc(&mut b, T, b""), Some(HDR + 1));
            wire[0] = op;
            assert_eq!(&b[..HDR + 1], &wire[..]);
            assert_eq!(parse_request(&b[..HDR + 1]), Some(want));
        }
        assert_eq!(encode_mkdir(&mut b, T, b""), None);
        assert_eq!(encode_unlink(&mut b, T, b""), None);
        // 线面伪造同拒：parse 白名单不含写面/GET（GET 恰形也拒——n=0
        // 先于尾随字段校验执行）。
        let t = T.to_le_bytes();
        assert_eq!(parse_request(&[OP_MKDIR, t[0], t[1], t[2], t[3], 0]), None);
        assert_eq!(parse_request(&[OP_UNLINK, t[0], t[1], t[2], t[3], 0]), None);
        assert_eq!(parse_request(&[OP_GET, t[0], t[1], t[2], t[3], 0, 0, 0, 0, 0]), None);
        assert_eq!(parse_request(&[OP_TRUNC, t[0], t[1], t[2], t[3], 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn path_segment_over_name_max() {
        let mut b = [0u8; 256];
        // 单段 49：v1 同拒面回归（n > NAME_MAX 旧规则的单段等价）。
        assert_eq!(encode_get(&mut b, T, &[b'a'; NAME_MAX + 1], 0), None);
        // 中段 49（总长 ≤ PATH_MAX）：段帽仍拒——PATH_MAX 不放宽段帽。
        let mut mid: Vec<u8> = vec![b'a'; NAME_MAX + 1];
        mid.push(0);
        mid.extend_from_slice(b"tail");
        assert!(mid.len() <= PATH_MAX);
        assert_eq!(encode_lookup(&mut b, T, &mid), None);
    }

    #[test]
    fn wire_shape_pinned() {
        // 线形逐字节钉板（P1-1 txid 头 + D2 单段同形）。
        let mut b = [0u8; 256];
        assert_eq!(encode_lookup(&mut b, T, b"f"), Some(HDR + 2));
        let mut m = vec![OP_LOOKUP];
        m.extend_from_slice(&T.to_le_bytes());
        m.extend_from_slice(&[1, b'f']);
        assert_eq!(&b[..m.len()], &m[..]);
        assert_eq!(encode_get(&mut b, T, b"f", 7), Some(HDR + 2 + 4));
        m[0] = OP_GET;
        m.extend_from_slice(&7u32.to_le_bytes());
        assert_eq!(&b[..m.len()], &m[..]);
        let n = encode_put(&mut b, T, b"a.bin", 0, b"hi").unwrap();
        let mut m = vec![OP_PUT];
        m.extend_from_slice(&T.to_le_bytes());
        m.extend_from_slice(&[5]);
        m.extend_from_slice(b"a.bin");
        m.extend_from_slice(&0u32.to_le_bytes());
        m.extend_from_slice(&[2]);
        m.extend_from_slice(b"hi");
        assert_eq!(&b[..n], &m[..]);
    }

    #[test]
    fn v1_ops_take_paths() {
        // PUT/TRUNC/GET 名域升路径：多段路径直通（服务端树 C-0 刀 2 消费）。
        round_put(b"d\0f.bin", 0, b"hi");
        let mut b = [0u8; 256];
        let n = encode_trunc(&mut b, T, b"d\0f.bin", 9).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Trunc { path: b"d\0f.bin", size: 9 }));
        let n = encode_get(&mut b, T, b"d\0f.bin", 3).unwrap();
        assert_eq!(parse_request(&b[..n]), Some(Req::Get { path: b"d\0f.bin", off: 3 }));
    }

    #[test]
    fn path_ops_trailing_rejected() {
        let t = T.to_le_bytes();
        assert_eq!(parse_request(&[OP_MKDIR, t[0], t[1], t[2], t[3], 1, b'a', 0]), None); // MKDIR 尾随
        assert_eq!(parse_request(&[OP_LOOKUP, t[0], t[1], t[2], t[3], 1, b'a', 0]), None); // LOOKUP 尾随
        assert_eq!(
            parse_request(&[OP_STAT, t[0], t[1], t[2], t[3], 2, b'a', 0, b'b', 0]),
            None
        ); // STAT 尾随（尾随字节含 NUL 同拒）
        assert_eq!(
            parse_request(&[OP_UNLINK, t[0], t[1], t[2], t[3], 2, b'a', 0, b'b']),
            None
        ); // UNLINK 尾随
    }

    #[test]
    fn path_parse_nlen_boundary() {
        // nlen 声 200：名在场也拒（总长帽 parse 侧同门）。
        let mut m = vec![OP_LOOKUP];
        m.extend_from_slice(&T.to_le_bytes());
        m.push(200);
        m.extend_from_slice(&[b'q'; 200]);
        assert_eq!(parse_request(&m), None);
        // nlen 声 3 实给 2（名截断）→ None。
        assert_eq!(
            parse_request(&[OP_LOOKUP, T.to_le_bytes()[0], 0, 0, 0, 3, b'a', b'b']),
            None
        );
    }

    #[test]
    fn path_op_out_capacity() {
        // 路径型编码器容量门：贴合过、差一拒（p = 9B 路径 → 恰 15 / 差一 14）。
        let p = b"aaaa\0bbbb";
        let mut b = [0u8; 15];
        assert_eq!(encode_readdir(&mut b, T, p), Some(15));
        let mut b = [0u8; 14];
        assert_eq!(encode_readdir(&mut b, T, p), None);
    }

    #[test]
    fn reply_stat_roundtrip() {
        let mut b = [0u8; 16];
        let n = encode_reply_stat(&mut b, T, TYPE_DIR, 4096).unwrap();
        assert_eq!(n, 10);
        assert_eq!(hdr_txid(&b[..n]), Some(T));
        assert_eq!(parse_reply_stat(&b[..n]), Some(Ok((TYPE_DIR, 4096u32))));
        let n = encode_reply_stat(&mut b, T, TYPE_FILE, 0).unwrap();
        assert_eq!(parse_reply_stat(&b[..n]), Some(Ok((TYPE_FILE, 0u32))));
        // 错误应答：恰 5 字节 st+txid；带尾随 → None。
        let n = encode_reply_status(&mut b, T, ST_NOENT).unwrap();
        assert_eq!(parse_reply_stat(&b[..n]), Some(Err(ST_NOENT)));
        assert_eq!(parse_reply_stat(b"\x01\x00"), None);
        // 畸形：st=0 面长度 ≠ 10（截断/超长）。
        assert_eq!(parse_reply_stat(&[0u8; 9]), None);
        assert_eq!(parse_reply_stat(&[0u8; 11]), None);
        // out 不足。
        let mut tiny = [0u8; 9];
        assert_eq!(encode_reply_stat(&mut tiny, T, TYPE_FILE, 0), None);
    }

    // ---- txid 面（P1-1）----

    #[test]
    fn txid_header_roundtrip() {
        // txid 双向回显钉板：全编码器同偏移落号，hdr_txid 直读同值。
        let mut b = [0u8; 256];
        for txid in [1u32, 7, 0x7F_FF_FF_FF, u32::MAX] {
            let n = encode_put(&mut b, txid, b"f", 0, b"x").unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_trunc(&mut b, txid, b"f", 0).unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_get(&mut b, txid, b"f", 0).unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_list(&mut b, txid).unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_lookup(&mut b, txid, b"f").unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_statfs(&mut b, txid).unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_reply_status(&mut b, txid, ST_FULL).unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_reply_get(&mut b, txid, 0, b"x", false).unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_reply_list(&mut b, txid, b"a b").unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_reply_stat(&mut b, txid, TYPE_DIR, 3).unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
            let n = encode_reply_statfs(&mut b, txid, &zero_statfs()).unwrap();
            assert_eq!(hdr_txid(&b[..n]), Some(txid));
        }
        // 截断帧（<HDR）取不出 txid；恰 HDR 边界可取。
        assert_eq!(hdr_txid(&[OP_LIST, 1, 2, 3]), None);
        let mut m = vec![OP_LIST];
        m.extend_from_slice(&9u32.to_le_bytes());
        assert_eq!(hdr_txid(&m), Some(9));
        // 纯偏移读：不判 op/st 合法性（0 号也直读——判别面归 parse_*）。
        assert_eq!(hdr_txid(&[0xFF, 0, 0, 0, 0]), Some(0));
        // 头长钉板。
        assert_eq!(HDR, 5);
    }

    // ---- STATFS 面（C-2 刀 2）----

    fn zero_statfs() -> Statfs {
        Statfs { total_sectors: 0, spc: 0, cluster_count: 0, free_clusters: 0, sector_sz: 0 }
    }

    #[test]
    fn statfs_roundtrip() {
        // 请求恰 5 字节（无路径参，LIST 同形）+ 应答五字段逐位回读（造数 =
        // mkfat 默认卷几何真值：2032 扇区/4 spc/506 簇/500 闲/512B 扇区）。
        let mut b = [0u8; 32];
        assert_eq!(encode_statfs(&mut b, T), Some(HDR));
        let mut m = vec![OP_STATFS];
        m.extend_from_slice(&T.to_le_bytes());
        assert_eq!(&b[..HDR], &m[..]);
        assert_eq!(parse_request(&b[..HDR]), Some(Req::Statfs));
        let s = Statfs {
            total_sectors: 2032,
            spc: 4,
            cluster_count: 506,
            free_clusters: 500,
            sector_sz: 512,
        };
        let n = encode_reply_statfs(&mut b, T, &s).unwrap();
        assert_eq!(n, STATFS_REPLY_LEN);
        assert_eq!(hdr_txid(&b[..n]), Some(T));
        assert_eq!(parse_reply_statfs(&b[..n]), Some(Ok(s)));
        // u32 极值不漂（五字段全宽钉板）。
        let s = Statfs {
            total_sectors: u32::MAX,
            spc: u32::MAX,
            cluster_count: u32::MAX,
            free_clusters: u32::MAX,
            sector_sz: u32::MAX,
        };
        let n = encode_reply_statfs(&mut b, T, &s).unwrap();
        assert_eq!(parse_reply_statfs(&b[..n]), Some(Ok(s)));
        // 错误应答：恰 5 字节 st+txid（无卷面语义复用 ST_NOENT）。
        let n = encode_reply_status(&mut b, T, ST_NOENT).unwrap();
        assert_eq!(parse_reply_statfs(&b[..n]), Some(Err(ST_NOENT)));
    }

    #[test]
    fn statfs_malformed() {
        // 请求尾随字节 = 野值拒（LIST 同门）；头截断（<HDR）同拒。
        assert_eq!(parse_request(&[OP_STATFS, 0]), None);
        assert_eq!(parse_request(&[OP_STATFS]), None);
        // st=0 面长度 != 25：空 / 截断 / 超长。
        assert_eq!(parse_reply_statfs(b""), None);
        assert_eq!(parse_reply_statfs(&[0u8; STATFS_REPLY_LEN - 1]), None);
        assert_eq!(parse_reply_statfs(&[0u8; STATFS_REPLY_LEN + 1]), None);
        // 错误应答带尾随字节。
        assert_eq!(parse_reply_statfs(b"\x01\x00"), None);
        // out 不足：空 / 差一。
        assert_eq!(encode_statfs(&mut [], T), None);
        assert_eq!(encode_reply_statfs(&mut [], T, &zero_statfs()), None);
        let mut tiny = [0u8; STATFS_REPLY_LEN - 1];
        assert_eq!(encode_reply_statfs(&mut tiny, T, &zero_statfs()), None);
    }

    #[test]
    fn reply_readdir_reuses_list_shape() {
        // READDIR 应答 = LIST 同形 blob；分隔语义在调用方（READDIR 用 NUL
        // 连接——段内无 NUL，可分性由 path_valid 构造保证）。
        let mut b = [0u8; 256];
        let names = b"f.bin\0sub\0t.hex";
        let n = encode_reply_list(&mut b, T, names).unwrap();
        assert_eq!(parse_reply_list(&b[..n]), Some(Ok(names.as_slice())));
        // 空目录：空 blob 合法。
        let n = encode_reply_list(&mut b, T, b"").unwrap();
        assert_eq!(parse_reply_list(&b[..n]), Some(Ok(&[] as &[u8])));
    }

    #[test]
    fn st_codes_unique() {
        // 状态码不撞号（语义按值钉板——AI 可读面：新增码必过此断言再入册）。
        let all = [ST_OK, ST_NOENT, ST_FULL, ST_BAD, ST_NOEMPTY, ST_EXISTS];
        for i in 0..all.len() {
            for j in i + 1..all.len() {
                assert_ne!(all[i], all[j], "状态码撞号：{} vs {}", all[i], all[j]);
            }
        }
        // 刀 2 帽常量在册（双侧同步义务的服务端锚；C-2 刀 2 起含启动
        // 预建保留槽：17 = 16 用户 + 1 etc，9 = 8 用户 + 1 mtab）。
        assert_eq!(DIR_COUNT_MAX, 17);
    }
}
