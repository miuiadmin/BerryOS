//! berry-appproto —— app 域 RPC 协议纯函数库（GUI-8 段①，`berry-psproto`/
//! `berry-fsproto` 先例：「解析器即产品」，零分配、零 unsafe、零依赖，
//! 输入只借用）。
//!
//! 拓扑（docs/GUI-8实施设计草案.md §3-裁决二 + §5）：launcher arm 门内
//! `channel_pair()` 一对，服务端 svcmgr 稳态 loop 挂第二通道常驻 serve
//! （guifbd `wait_many` 多路先例），客户端端随 env extras[2] 赠 launcher
//! （libc `app_cli_h()` 新增，同槽惯例）——单对双向 + **严格串行 RPC**
//! （一请求一应答/分片流，队列深 1 用量，fsclient 同款纪律；v1 无请求
//! 序号，迟到应答滞留队列无害）。launcher 零 rootfs VMO、零守卫复刻——
//! spawn 全守卫（bap_scan→manifest→compat→ui→fs 授能→spawn→reap）留
//! svcmgr `app_launch` 单源，RPC 对端就是用户态服务（微内核「先问
//! 能不能放用户态」）。
//!
//! wire（草案 §5 冻结；全数值 LE，逐字节显式拼装非 `#[repr(C)]` 直拷）：
//! - 请求低带 `[op u8]`：APP_LIST_REQ(1，空——恰 1B，尾随字节 = 野值拒)
//!   / APP_SPAWN_REQ(2，`[op][id 24B NUL 补齐]`)；APP_EVENT(3) **op 码
//!   留位零实施**（.bap 增删推送——v2 /apps 树前提；常量在册保号
//!   add-only，请求面拒）。
//! - 应答帧高带 0x10..（帧首 op 区分方向；多帧分片流不设 st 前缀，错误
//!   收口见下「拒面」）：
//!   - APP_LIST 应答 = N × `APP_ENTRY(0x10){id[24], label[64], icon_w,
//!     icon_h, blob_len 全 u32 LE}` 恰 [`ENTRY_LEN`] + 每条目 icon blob
//!     按 192B 分片跟帧 `APP_CHUNK(0x11) [op][idx u32 LE][data 1..=192B]`
//!     （BYI1 原样字节，launcher 侧 decode——两端零解码逻辑重复；图标
//!     ≤65536px 帽由 BYI1 域执法，本 crate 不管）+ 终帧
//!     `APP_LIST_END(0x12) [op][n u32 LE]`。
//!   - APP_SPAWN 应答 = `APP_SPAWN_ACK(0x13) [op][ok u8][err u32 LE]`。
//! - id/label 定宽字段 **NUL 补齐纪律**：内容 1..=宽、无内嵌 NUL、首 NUL
//!   后恒零。id 宽 24 = `berry-manifest::ID_MAX` 同源同值（注释锚 + 测试
//!   钉，不引依赖）；label 宽 64 = 草案冻结 wire 值（源头 manifest
//!   LABEL_MAX=32 留裕）。缺 label 的 app 由服务端回填（如 id 本串）——
//!   缺席语义归数据源不进 wire，berry-layout「无文件 = 消费方默认」
//!   同款哲学。
//! - **分片序号 v1 冻结**（0c 钉：一次到位，勿重蹈 DESC 帽两刀补课）：
//!   每分片帧携 idx，**条目内自 0 起每帧恰 +1**——乱序/重放/跳帧一律
//!   ST_BAD 域拒面（`feed` 返 None，半张桌面清单不进渲染面）；条目切换
//!   idx 归零（idx 编 blob 分片位，非全流位置）。分片粒度不执法（data
//!   ≤192 即合法，满 192 切法是发送侧惯例非 wire 不变量）；blob 收支
//!   对账执法（分片字节合计恰 = blob_len，欠收即 END 拒、溢出即帧拒）。
//! - APP_SPAWN_ACK 耦合律：ok∈{0,1}；ok=1 ⇒ err 恒 0、ok=0 ⇒ err≠0
//!   （编码侧执法 + 解析侧拒）——「失败无理由码」的歧义形态线上不
//!   存在；err 值域 v1 无分类面（svcmgr app_launch Err 侧自定，全宽
//!   携带定宽优先）。
//! - 服务端畸形请求拒面 = 1B `[ST_BAD]`（guiproto 应答烘焙线不变量：
//!   非 OK 应答恒 1B 永不携带数据）。
//!
//! 帧帽 [`FRAME_MAX`]=256B（channel inline 上限）编译期断言——全部五帧
//! 型逐一钉。严格解析：截断/尾随字节/野 op/野 ok/字段失配（idx 单调
//! +1、blob 收支、END n 对账）一律 None（「编不出就拒绝，不截断」）；
//! 编码器 None 契约同 psproto（变值入参拒面），定长帧编码器（`-> usize`
//! 形）buf 不足 = 调用方程序员错，assert 炸响。

#![cfg_attr(not(test), no_std)]

// ---------- op 表（请求低带 1..3 / 应答高带 0x10..，方向即纪律）----------

/// 请求：拉 app 清单（空——分片流应答，见 [`ListDecoder`]）。
pub const OP_APP_LIST_REQ: u8 = 1;
/// 请求：拉起 app（svcmgr 走 `app_launch` 全守卫；reap 异步收尾不阻塞
/// 回执）。
pub const OP_APP_SPAWN_REQ: u8 = 2;
/// 留位零实施：.bap 增删推送（v2 /apps 树前提——v1 rootfs 静态 cpio 无
/// 运行期增删物理通道）。常量在册保号 add-only；**请求面拒**（解析侧
/// 不在册，编码器无此面）。
pub const OP_APP_EVENT: u8 = 3;

/// 应答帧：app 条目头（定长 [`ENTRY_LEN`]；后随该条目 blob 分片帧）。
pub const OP_APP_ENTRY: u8 = 0x10;
/// 应答帧：blob 分片（`[op][idx][data 1..=192B]`——BYI1 原样字节）。
pub const OP_APP_CHUNK: u8 = 0x11;
/// 应答帧：清单终帧（n = 条目数对账）。
pub const OP_APP_LIST_END: u8 = 0x12;
/// 应答帧：spawn 回执（`{ok, err}` 耦合律见模块注）。
pub const OP_APP_SPAWN_ACK: u8 = 0x13;

/// 应答状态码（u8 协议错误空间，非 errno；psproto 同族）。
pub const ST_OK: u8 = 0;
/// 畸形请求服务端拒面（1B `[ST_BAD]`）；解码侧 idx 乱序/重放等拒面同
/// 域（`feed`/parse 返 None，客户端弃流打诊断）。
pub const ST_BAD: u8 = 1;

// ---------- 宽帽 / 帧长（草案 §5 冻结值）----------

/// id 定宽字段宽——**与 `berry-manifest::ID_MAX` 同源同值 24**（不引
/// 依赖：注释锚 + 测试绝对钉，berry-layout 对偶侧同款纪律）。
pub const ID_LEN: usize = 24;
/// label 定宽字段宽 = 草案 §5 冻结 wire 值 64（源头 manifest
/// LABEL_MAX=32 留裕——wire 不执法 32，只执法自身 64）。
pub const LABEL_LEN: usize = 64;
/// 单分片 data 帽（草案 §5：blob 按 192B 分片）。
pub const CHUNK_DATA_MAX: usize = 192;
/// 帧帽 = channel inline 上限（psproto REPLY_MAX 断言同值 CHAN_MSG_MAX）。
pub const FRAME_MAX: usize = 256;

/// APP_LIST_REQ 恰长（空请求）。
pub const REQUEST_LEN: usize = 1;
/// APP_ENTRY 恰长：op + id + label + icon_w/h + blob_len。
pub const ENTRY_LEN: usize = 1 + ID_LEN + LABEL_LEN + 4 + 4 + 4;
/// APP_CHUNK 帧头（op + idx）；帧长 = 头 + data（1..=[`CHUNK_DATA_MAX`]）。
pub const CHUNK_HEAD: usize = 5;
/// APP_CHUNK 帧长上界（头 + data 帽）。
pub const CHUNK_FRAME_MAX: usize = CHUNK_HEAD + CHUNK_DATA_MAX;
/// APP_LIST_END 恰长（op + n）。
pub const END_LEN: usize = 5;
/// APP_SPAWN_REQ 恰长（op + id）。
pub const SPAWN_REQ_LEN: usize = 1 + ID_LEN;
/// APP_SPAWN_ACK 恰长（op + ok + err）。
pub const ACK_LEN: usize = 6;

// 帧帽编译期断言：六帧型逐一钉（草案 §5「帧帽 256B」——改布局超帽 =
// 编译期炸响，非运行期拒）。
const _: () = assert!(REQUEST_LEN <= FRAME_MAX, "请求帧超帽");
const _: () = assert!(ENTRY_LEN <= FRAME_MAX, "ENTRY 帧超帽");
const _: () = assert!(CHUNK_FRAME_MAX <= FRAME_MAX, "CHUNK 帧超帽");
const _: () = assert!(END_LEN <= FRAME_MAX, "END 帧超帽");
const _: () = assert!(SPAWN_REQ_LEN <= FRAME_MAX, "SPAWN_REQ 帧超帽");
const _: () = assert!(ACK_LEN <= FRAME_MAX, "ACK 帧超帽");

// ---------- 请求面（launcher → svcmgr）----------

/// 编码 APP_LIST_REQ（buf ≥ [`REQUEST_LEN`]；恒成功——定宽 1B）。
pub fn encode_list_request(buf: &mut [u8]) -> usize {
    assert!(!buf.is_empty(), "appproto: req buf 不足");
    buf[0] = OP_APP_LIST_REQ;
    REQUEST_LEN
}

/// 编码 APP_SPAWN_REQ：id 1..=[`ID_LEN`] 且无内嵌 NUL（NUL 补齐到定宽）
/// 才收；buf 不足 / id 野值返 None——调用方降级（编码器 None 契约）。
pub fn encode_spawn_request(buf: &mut [u8], id: &[u8]) -> Option<usize> {
    if buf.len() < SPAWN_REQ_LEN || !write_padded(&mut buf[1..], id) {
        return None;
    }
    buf[0] = OP_APP_SPAWN_REQ;
    Some(SPAWN_REQ_LEN)
}

/// 解析请求（服务端分派）：恰 1B 且 op 在册（LIST/SPAWN；APP_EVENT 留位
/// 拒——零实施面不进请求分派）；否则 None（服务端以 1B `[ST_BAD]` 拒）。
pub fn parse_request(buf: &[u8], len: usize) -> Option<u8> {
    if len != REQUEST_LEN || buf.len() < len {
        return None;
    }
    match buf[0] {
        OP_APP_LIST_REQ | OP_APP_SPAWN_REQ => Some(buf[0]),
        _ => None,
    }
}

/// 解析 APP_SPAWN_REQ 的 id 段（恰 [`SPAWN_REQ_LEN`] + NUL 补齐纪律）。
/// 读以帧长为界（`buf` 可长于帧——recv 复用缓冲帧外残留不参与解析，
/// psproto 家族「读一律以 len 为界」同律）。
pub fn parse_spawn_request(buf: &[u8], len: usize) -> Option<&[u8]> {
    if len != SPAWN_REQ_LEN || len > buf.len() {
        return None;
    }
    read_padded(&buf[1..1 + ID_LEN])
}

/// 编码服务端畸形请求拒面回执：1B `[ST_BAD]`（恒成功——定宽 1B）。
pub fn encode_err_reply(buf: &mut [u8]) -> usize {
    assert!(!buf.is_empty(), "appproto: err buf 不足");
    buf[0] = ST_BAD;
    1
}

/// 解析 1B 错误回执：恰 1B 且 st=ST_BAD（ST_OK 裸帧无语义——成功应答
/// 皆带 op 头，非 OK 应答恒 1B）。
pub fn parse_err_reply(buf: &[u8], len: usize) -> Option<()> {
    if len == 1 && buf.len() >= 1 && buf[0] == ST_BAD {
        Some(())
    } else {
        None
    }
}

// ---------- APP_LIST 流：服务端编码 ----------

/// 编码 APP_ENTRY 帧：id/label 走 NUL 补齐纪律（1..=宽、无内嵌 NUL），
/// 三 u32 全宽携带（icon_w/h 顶格合法——像素帽归 BYI1 域）；buf 不足或
/// 字段野值返 None。
pub fn encode_entry(
    buf: &mut [u8],
    id: &[u8],
    label: &[u8],
    icon_w: u32,
    icon_h: u32,
    blob_len: u32,
) -> Option<usize> {
    if buf.len() < ENTRY_LEN
        || !write_padded(&mut buf[1..1 + ID_LEN], id)
        || !write_padded(&mut buf[1 + ID_LEN..1 + ID_LEN + LABEL_LEN], label)
    {
        return None;
    }
    buf[0] = OP_APP_ENTRY;
    let mut o = 1 + ID_LEN + LABEL_LEN;
    buf[o..o + 4].copy_from_slice(&icon_w.to_le_bytes());
    o += 4;
    buf[o..o + 4].copy_from_slice(&icon_h.to_le_bytes());
    o += 4;
    buf[o..o + 4].copy_from_slice(&blob_len.to_le_bytes());
    Some(ENTRY_LEN)
}

/// blob 分片编码器（服务端）：`new` 绑 blob → `next` 逐帧编满 192 切片、
/// idx 自 0 每帧 +1（与 [`ListDecoder`] 单源共识——idx 纪律两端不长第
/// 二套）→ 收干返 None。空 blob = 零帧（首调即 None，解码侧 blob_len=0
/// 亦零分片期望）。
pub struct ChunkEncoder<'a> {
    blob: &'a [u8],
    pos: usize,
    next_idx: u32,
}

impl<'a> ChunkEncoder<'a> {
    pub fn new(blob: &'a [u8]) -> Self {
        Self { blob, pos: 0, next_idx: 0 }
    }

    /// 编下一分片帧，返帧长；blob 收干返 None。buf 不足 = 调用方程序员
    /// 错（定长帽调用点），assert 炸响。
    pub fn next(&mut self, buf: &mut [u8]) -> Option<usize> {
        if self.pos >= self.blob.len() {
            return None;
        }
        let n = (self.blob.len() - self.pos).min(CHUNK_DATA_MAX);
        assert!(buf.len() >= CHUNK_HEAD + n, "appproto: chunk buf 不足");
        buf[0] = OP_APP_CHUNK;
        buf[1..CHUNK_HEAD].copy_from_slice(&self.next_idx.to_le_bytes());
        buf[CHUNK_HEAD..CHUNK_HEAD + n].copy_from_slice(&self.blob[self.pos..self.pos + n]);
        self.pos += n;
        self.next_idx += 1;
        Some(CHUNK_HEAD + n)
    }
}

/// blob 分片流编码器（服务端·零整读窗形）：与 [`ChunkEncoder`] 同帧同
/// idx 纪律（host 等价测逐字节钉——两实现单源共识不长第三套），但 blob
/// 不整驻内存：`new` 只携声明总长，`next` 逐帧回调 `fill(dst, off)`
/// （off = blob 内偏移）由数据源自填本帧 ≤192B 载荷。`fill` 返 false =
/// 源读败——该帧载荷零填充（dst 内容视为未定义），**流长恒与声明一致**
/// （解码侧 idx/对账执法不破，坏由调用方 eco 可见化）；后续帧独立重试探
/// （源恢复则续真数据）。空声明 = 零帧（与 [`ChunkEncoder`] 同形）。
/// 动机：svcmgr 侧 64KiB 整读静态窗会被 elf2flat 补零物化进平像（.bss
/// 文件侧兑现），单页墙 131072 顶破——VMO 逐 ≤192B 直读进帧载荷域即免窗。
pub struct ChunkStream {
    len: usize,
    pos: usize,
    next_idx: u32,
}

impl ChunkStream {
    pub fn new(len: usize) -> Self {
        Self { len, pos: 0, next_idx: 0 }
    }

    /// 编下一分片帧（`fill` 直写 buf 载荷域），返帧长；声明长收干返 None。
    /// buf 不足 = 调用方程序员错（定长帽调用点），assert 炸响。
    pub fn next<F: FnMut(&mut [u8], usize) -> bool>(
        &mut self,
        buf: &mut [u8],
        fill: &mut F,
    ) -> Option<usize> {
        if self.pos >= self.len {
            return None;
        }
        let n = (self.len - self.pos).min(CHUNK_DATA_MAX);
        assert!(buf.len() >= CHUNK_HEAD + n, "appproto: chunk buf 不足");
        buf[0] = OP_APP_CHUNK;
        buf[1..CHUNK_HEAD].copy_from_slice(&self.next_idx.to_le_bytes());
        if !fill(&mut buf[CHUNK_HEAD..CHUNK_HEAD + n], self.pos) {
            buf[CHUNK_HEAD..CHUNK_HEAD + n].fill(0);
        }
        self.pos += n;
        self.next_idx += 1;
        Some(CHUNK_HEAD + n)
    }
}

/// 编码 APP_LIST_END 终帧（n = 服务端已发条目数；对账由解码侧执法）。
pub fn encode_list_end(buf: &mut [u8], n: u32) -> usize {
    assert!(buf.len() >= END_LEN, "appproto: end buf 不足");
    buf[0] = OP_APP_LIST_END;
    buf[1..END_LEN].copy_from_slice(&n.to_le_bytes());
    END_LEN
}

// ---------- APP_LIST 流：客户端解码 ----------

/// 一帧的解码产物（借用入参 buf——零分配；roundtrip 对拍面先例：
/// guiproto `PumpEvent` 同款 derive）。
#[derive(Debug, PartialEq, Eq)]
pub enum ListFrame<'a> {
    /// app 条目头（随后 blob_len 字节经 [`ListFrame::Chunk`] 分片到货）。
    Entry {
        id: &'a [u8],
        label: &'a [u8],
        icon_w: u32,
        icon_h: u32,
        blob_len: u32,
    },
    /// blob 分片（idx 条目内自 0 恰 +1；data = BYI1 原样字节）。
    Chunk { idx: u32, data: &'a [u8] },
    /// 清单终帧（n 与已收条目数对账过才产出）。
    End { n: u32 },
}

/// APP_LIST 分片流解码器（launcher 侧）：逐帧 `feed`，**idx 单调 +1 /
/// blob 收支 / END 对账三执法内置**（0c 钉 v1 冻结——乱序/重放/跳帧/
/// 欠收/多收/n 谎报一律 None，调用方弃整流打诊断，半张清单不进渲染
/// 面）。None 后流已废（状态未损但逻辑不可续——续喂产物无意义）。
#[derive(Debug)]
pub struct ListDecoder {
    next_idx: u32,
    remain: u32,
    entries: u32,
    done: bool,
}

impl ListDecoder {
    pub fn new() -> Self {
        Self { next_idx: 0, remain: 0, entries: 0, done: false }
    }

    /// END 已收（流终判据——此后任何帧皆尾随拒）。
    pub fn finished(&self) -> bool {
        self.done
    }

    /// 喂一帧：帧型分派 + 全执法（见模块注）。恰长执法——ENTRY/END 各有
    /// 定长，CHUNK data 1..=192；全部检查先于状态变更。
    pub fn feed<'a>(&mut self, buf: &'a [u8], len: usize) -> Option<ListFrame<'a>> {
        if self.done || len == 0 || len > buf.len() {
            return None;
        }
        match buf[0] {
            OP_APP_ENTRY if len == ENTRY_LEN => {
                if self.remain != 0 {
                    return None; // 前条目 blob 未收满又开新条目
                }
                let id = read_padded(&buf[1..1 + ID_LEN])?;
                let label = read_padded(&buf[1 + ID_LEN..1 + ID_LEN + LABEL_LEN])?;
                let icon_w = le32(buf, 1 + ID_LEN + LABEL_LEN);
                let icon_h = le32(buf, 1 + ID_LEN + LABEL_LEN + 4);
                let blob_len = le32(buf, 1 + ID_LEN + LABEL_LEN + 8);
                self.remain = blob_len;
                self.next_idx = 0;
                self.entries += 1;
                Some(ListFrame::Entry { id, label, icon_w, icon_h, blob_len })
            }
            OP_APP_CHUNK if len >= CHUNK_HEAD + 1 => {
                let idx = le32(buf, 1);
                let data = &buf[CHUNK_HEAD..len];
                if self.remain == 0
                    || idx != self.next_idx
                    || data.len() > CHUNK_DATA_MAX
                    || data.len() as u32 > self.remain
                {
                    return None; // 无待收 / 乱序·重放·跳帧 / 超 192 / 收支溢出
                }
                self.remain -= data.len() as u32;
                self.next_idx += 1;
                Some(ListFrame::Chunk { idx, data })
            }
            OP_APP_LIST_END if len == END_LEN => {
                if self.remain != 0 {
                    return None; // blob 欠收即终 = 截断流
                }
                let n = le32(buf, 1);
                if n != self.entries {
                    return None; // n 对账失配（谎报条目数）
                }
                self.done = true;
                Some(ListFrame::End { n })
            }
            _ => None, // 野 op（含请求面 op 泄漏进应答流 / ACK 混流）
        }
    }
}

// ---------- APP_SPAWN 回执 ----------

/// 编码 APP_SPAWN_ACK：耦合律执法（合法形 ⟺ `ok == (err == 0)`——ok=1
/// ⇒ err=0；ok=0 ⇒ err≠0，失败必携理由码）；buf 不足 / 耦合失配返 None。
pub fn encode_spawn_ack(buf: &mut [u8], ok: bool, err: u32) -> Option<usize> {
    if buf.len() < ACK_LEN || ok != (err == 0) {
        return None;
    }
    buf[0] = OP_APP_SPAWN_ACK;
    buf[1] = ok as u8;
    buf[2..ACK_LEN].copy_from_slice(&err.to_le_bytes());
    Some(ACK_LEN)
}

/// 解析 APP_SPAWN_ACK：恰 [`ACK_LEN`]、ok∈{0,1}、耦合律同执法；返回
/// `(ok, err)`。
pub fn parse_spawn_ack(buf: &[u8], len: usize) -> Option<(bool, u32)> {
    if len != ACK_LEN || len > buf.len() || buf[0] != OP_APP_SPAWN_ACK {
        return None;
    }
    let ok = match buf[1] {
        0 => false,
        1 => true,
        _ => return None, // 野 ok
    };
    let err = le32(buf, 2);
    if ok != (err == 0) {
        return None; // ok=1 携 err / ok=0 零 err 皆拒（耦合律同执法）
    }
    Some((ok, err))
}

// ---------- 内部：定宽字段 + LE 取数 ----------

/// 定宽 NUL 补齐字段写入：内容 1..=宽且无内嵌 NUL 才 true；余位恒零
/// （全宽内容无补零位）。
fn write_padded(dst: &mut [u8], s: &[u8]) -> bool {
    if s.is_empty() || s.len() > dst.len() || s.contains(&0) {
        return false;
    }
    dst[..s.len()].copy_from_slice(s);
    dst[s.len()..].fill(0);
    true
}

/// 定宽 NUL 补齐字段读出：首 NUL（或全宽）截取；空内容 / NUL 后非零
/// 恒拒（补齐纪律执法——垃圾尾 = 畸形帧）。
fn read_padded(src: &[u8]) -> Option<&[u8]> {
    match src.iter().position(|&b| b == 0) {
        None => Some(src), // 全宽无 NUL（恰 24B id 形态）
        Some(0) => None,
        Some(p) => {
            if src[p + 1..].iter().any(|&b| b != 0) {
                None
            } else {
                Some(&src[..p])
            }
        }
    }
}

/// LE 取 u32（调用点以恰长守卫保证 4B 在界——定长帧惯用形）。
fn le32(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 服务端整流编码：entries → 帧字节序列（复用生产编码器——共识测试
    /// 不手造正例，encode/decode 两侧只对拍协议不变量）。
    fn build_stream(entries: &[(&[u8], &[u8], u32, u32, &[u8])]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        let mut buf = [0u8; FRAME_MAX];
        for &(id, label, w, h, blob) in entries {
            let n = encode_entry(&mut buf, id, label, w, h, blob.len() as u32).unwrap();
            frames.push(buf[..n].to_vec());
            let mut enc = ChunkEncoder::new(blob);
            while let Some(n) = enc.next(&mut buf) {
                frames.push(buf[..n].to_vec());
            }
        }
        let n = encode_list_end(&mut buf, entries.len() as u32);
        frames.push(buf[..n].to_vec());
        frames
    }

    /// 客户端整流解码 + blob 重组：逐帧 feed 收集，条目按序对拍原输入
    /// （id/label/几何/blob 三重恒等）。
    fn drain_stream(entries: &[(&[u8], &[u8], u32, u32, &[u8])]) {
        let frames = build_stream(entries);
        let mut dec = ListDecoder::new();
        let mut got: Vec<(Vec<u8>, Vec<u8>, u32, u32, Vec<u8>)> = Vec::new();
        for f in &frames {
            match dec.feed(f, f.len()).unwrap() {
                ListFrame::Entry { id, label, icon_w, icon_h, blob_len } => {
                    got.push((
                        id.to_vec(),
                        label.to_vec(),
                        icon_w,
                        icon_h,
                        Vec::with_capacity(blob_len as usize),
                    ));
                }
                ListFrame::Chunk { data, .. } => got.last_mut().unwrap().4.extend_from_slice(data),
                ListFrame::End { n } => assert_eq!(n as usize, entries.len()),
            }
        }
        assert!(dec.finished());
        assert_eq!(got.len(), entries.len());
        for (got, want) in got.iter().zip(entries.iter()) {
            assert_eq!(got.0, want.0, "id 漂移");
            assert_eq!(got.1, want.1, "label 漂移");
            assert_eq!(got.2, want.2, "icon_w 漂移");
            assert_eq!(got.3, want.3, "icon_h 漂移");
            assert_eq!(got.4, want.4, "blob 重组漂移");
        }
    }

    #[test]
    fn request_roundtrip_and_rejects() {
        // LIST：恰 1B 往返。
        let mut buf = [0u8; 4];
        let n = encode_list_request(&mut buf);
        assert_eq!(n, REQUEST_LEN);
        assert_eq!(parse_request(&buf, n), Some(OP_APP_LIST_REQ));
        // 恰长执法：尾随字节 / 空全拒。
        assert_eq!(parse_request(&buf, 2), None);
        assert_eq!(parse_request(&buf, 0), None);
        // 野 op：0（st 值域泄漏）/ 4（在册之外）/ 0x10（应答面泄漏进请求）。
        assert_eq!(parse_request(&[0], 1), None);
        assert_eq!(parse_request(&[OP_APP_EVENT + 1], 1), None);
        assert_eq!(parse_request(&[OP_APP_ENTRY], 1), None);
        // APP_EVENT 留位钉：op=3 常量在册但请求面拒（零实施）。
        assert_eq!(OP_APP_EVENT, 3);
        assert_eq!(parse_request(&[OP_APP_EVENT], 1), None);
    }

    #[test]
    fn spawn_request_id_boundaries() {
        let mut buf = [0u8; SPAWN_REQ_LEN];
        // 最短 id 1B / 常规 7B 往返（NUL 补齐读出还原）。
        assert_eq!(encode_spawn_request(&mut buf, b"A"), Some(SPAWN_REQ_LEN));
        assert_eq!(parse_spawn_request(&buf, SPAWN_REQ_LEN), Some(&b"A"[..]));
        assert_eq!(encode_spawn_request(&mut buf, b"APPDEMO"), Some(SPAWN_REQ_LEN));
        assert_eq!(parse_spawn_request(&buf, SPAWN_REQ_LEN), Some(&b"APPDEMO"[..]));
        // 黄金钉板：op + id + NUL 补齐（LE 无涉——纯字节段）。
        let mut want = vec![OP_APP_SPAWN_REQ];
        want.extend_from_slice(b"APPDEMO");
        want.extend([0u8; 17]);
        assert_eq!(&buf[..SPAWN_REQ_LEN], want.as_slice());
        // 恰 24：全宽无 NUL 收且读出同长。
        let id24 = [b'x'; ID_LEN];
        assert_eq!(encode_spawn_request(&mut buf, &id24), Some(SPAWN_REQ_LEN));
        assert_eq!(parse_spawn_request(&buf, SPAWN_REQ_LEN), Some(&id24[..]));
        // 拒面：25B 超 / 空 / 内嵌 NUL。
        assert_eq!(encode_spawn_request(&mut buf, &[b'y'; ID_LEN + 1]), None);
        assert_eq!(encode_spawn_request(&mut buf, b""), None);
        assert_eq!(encode_spawn_request(&mut buf, b"ap\0p"), None);
        // buf 恰长收 / 少 1 拒。
        let mut small = [0u8; SPAWN_REQ_LEN];
        assert_eq!(encode_spawn_request(&mut small, b"app"), Some(SPAWN_REQ_LEN));
        let mut tiny = [0u8; SPAWN_REQ_LEN - 1];
        assert_eq!(encode_spawn_request(&mut tiny, b"app"), None);
        // 解析拒面：尾随字节 / NUL 后补齐区垃圾（id=app 的终止位之后）。
        assert_eq!(encode_spawn_request(&mut buf, b"app"), Some(SPAWN_REQ_LEN));
        assert_eq!(parse_spawn_request(&buf, SPAWN_REQ_LEN + 1), None);
        let mut junk = buf;
        junk[1 + 3 + 1] = 1; // NUL 终止位后一字节
        assert_eq!(parse_spawn_request(&junk, SPAWN_REQ_LEN), None);
    }

    #[test]
    fn spawn_request_reused_buffer_dirty_tail() {
        // recv 复用缓冲形（svcmgr 家族先例：256B 静态 recv 缓冲复用、
        // 内核恰写 nb 不清洗尾部）：帧外残留字节不得影响解析结果。
        // 对抗审查 MAJOR（越帧读）修复随刀回归钉——手构帧（encode 会
        // 洗净 dst 尾部，构不出脏尾形）。
        let mut buf = [0xABu8; 256];
        buf[0] = OP_APP_SPAWN_REQ;
        buf[1..8].copy_from_slice(b"APPDEMO");
        buf[8..SPAWN_REQ_LEN].fill(0); // 帧内 NUL 补齐；[25..256] 恒 0xAB 脏尾
        // 形一（短 id + 脏尾）：帧长界读出恰 id，不为脏尾误拒。
        assert_eq!(parse_spawn_request(&buf, SPAWN_REQ_LEN), Some(&b"APPDEMO"[..]));
        // 形二（全宽 24B id + 脏尾）：读出恰 24B，不吞帧外脏字节。
        let id24 = [b'z'; ID_LEN];
        buf[1..SPAWN_REQ_LEN].copy_from_slice(&id24);
        assert_eq!(parse_spawn_request(&buf, SPAWN_REQ_LEN), Some(&id24[..]));
    }

    #[test]
    fn id_len_mirror_pinned() {
        // 帽值绝对钉（=24）：berry-manifest::ID_MAX 同源同值的对偶侧——
        // 两侧任一漂移此测必炸（不引依赖，注释锚 + 测试钉纪律）。
        assert_eq!(ID_LEN, 24);
        assert_eq!(LABEL_LEN, 64);
        assert_eq!(CHUNK_DATA_MAX, 192);
    }

    #[test]
    fn entry_roundtrip_and_golden() {
        // 编解码往返（借解码器解析 ENTRY 帧——单帧亦整流）。
        let blob = [0xABu8; 5];
        let frames = build_stream(&[(b"APPDEMO", b"Demo App", 64, 64, &blob)]);
        assert_eq!(frames.len(), 3); // ENTRY + 1 chunk + END
        let mut dec = ListDecoder::new();
        match dec.feed(&frames[0], frames[0].len()).unwrap() {
            ListFrame::Entry { id, label, icon_w, icon_h, blob_len } => {
                assert_eq!(id, b"APPDEMO");
                assert_eq!(label, b"Demo App");
                assert_eq!((icon_w, icon_h, blob_len), (64, 64, 5));
            }
            _ => panic!("首帧应为 ENTRY"),
        }
        // 黄金字节钉板（改布局 = 此测必炸）：op + id + label + 三 u32 LE。
        let mut buf = [0u8; ENTRY_LEN];
        let n = encode_entry(&mut buf, b"APPDEMO", b"Demo App", 0x0102_0304, 64, 193).unwrap();
        assert_eq!(n, ENTRY_LEN);
        let mut want = vec![OP_APP_ENTRY];
        want.extend_from_slice(b"APPDEMO");
        want.extend([0u8; 24 - 7]);
        want.extend_from_slice(b"Demo App");
        want.extend([0u8; 64 - 8]);
        want.extend_from_slice(&0x0102_0304u32.to_le_bytes());
        want.extend_from_slice(&64u32.to_le_bytes());
        want.extend_from_slice(&193u32.to_le_bytes());
        assert_eq!(&buf[..n], want.as_slice());
        // LE 钉板：icon_w 首字节 = 最低位字节 0x04。
        assert_eq!(buf[1 + ID_LEN + LABEL_LEN], 0x04);
        // 值域顶格：u32::MAX 全宽合法（像素帽归 BYI1 域，wire 不执法）。
        let n = encode_entry(&mut buf, b"app", b"l", u32::MAX, u32::MAX, u32::MAX).unwrap();
        assert_eq!(n, ENTRY_LEN);
    }

    #[test]
    fn frame_len_constants_pinned() {
        // 帧型定长/帽常量钉板（编译期断言只执法 ≤256，数值本身此处钉）。
        assert_eq!(ENTRY_LEN, 101); // 1 + 24 + 64 + 12
        assert_eq!(CHUNK_HEAD, 5);
        assert_eq!(CHUNK_FRAME_MAX, 197); // 5 + 192
        assert_eq!(END_LEN, 5);
        assert_eq!(SPAWN_REQ_LEN, 25);
        assert_eq!(ACK_LEN, 6);
        assert_eq!(FRAME_MAX, 256); // CHAN_MSG_MAX 同值
    }

    #[test]
    fn entry_field_rejects() {
        let mut buf = [0u8; ENTRY_LEN];
        // id：超 1B / 空 / 内嵌 NUL。
        assert_eq!(encode_entry(&mut buf, &[b'i'; ID_LEN + 1], b"l", 1, 1, 0), None);
        assert_eq!(encode_entry(&mut buf, b"", b"l", 1, 1, 0), None);
        assert_eq!(encode_entry(&mut buf, b"a\0p", b"l", 1, 1, 0), None);
        // label：恰 64 收 / 65 拒 / 空 / 内嵌 NUL。
        assert_eq!(encode_entry(&mut buf, b"app", &[b'l'; LABEL_LEN], 1, 1, 0), Some(ENTRY_LEN));
        assert_eq!(encode_entry(&mut buf, b"app", &[b'l'; LABEL_LEN + 1], 1, 1, 0), None);
        assert_eq!(encode_entry(&mut buf, b"app", b"", 1, 1, 0), None);
        assert_eq!(encode_entry(&mut buf, b"app", b"la\0bel", 1, 1, 0), None);
        // buf 恰长收 / 少 1 拒。
        let mut small = [0u8; ENTRY_LEN];
        assert_eq!(encode_entry(&mut small, b"app", b"lab", 1, 1, 0), Some(ENTRY_LEN));
        let mut tiny = [0u8; ENTRY_LEN - 1];
        assert_eq!(encode_entry(&mut tiny, b"app", b"lab", 1, 1, 0), None);
        // 解析拒面：label 补齐区垃圾（NUL 终止位之后非零）/ 截断每切点 /
        // 尾随字节。
        let frames = build_stream(&[(b"app", b"lab", 1, 1, &[])]);
        let mut junk = frames[0].clone();
        junk[1 + ID_LEN + 4] = 1; // label=lab 的 NUL 后一字节
        assert_eq!(ListDecoder::new().feed(&junk, junk.len()), None);
        for cut in 0..frames[0].len() {
            assert_eq!(
                ListDecoder::new().feed(&frames[0][..cut], cut),
                None,
                "ENTRY 截断 @{cut} 应拒"
            );
        }
        let mut tail = frames[0].clone();
        tail.push(0xFF);
        assert_eq!(ListDecoder::new().feed(&tail, tail.len()), None);
    }

    #[test]
    fn stream_full_roundtrip_multi_entry() {
        // 三条目混合：空 blob / 两分片 blob（193 = 192+1）/ 单分片——
        // 全流恒等（id/label/几何/blob 四重对拍在 drain_stream 内）。
        let blob2 = (0u8..=192).collect::<Vec<u8>>(); // 193B 恰过 192 帽
        let blob3 = [0xCDu8; 100];
        drain_stream(&[
            (b"appquiet", b"Quiet", 32, 32, &[]),
            (b"appdemo", b"Demo App", 256, 256, &blob2),
            (&[b'z'; ID_LEN], &[b'L'; LABEL_LEN], 65535, 65535, &blob3),
        ]);
    }

    #[test]
    fn stream_blob_len_sweep() {
        // 长度扫描 0..=577（3×192+1 跨三分片界）：分片切法/idx 序列/重组
        // 恒等的穷举面（布局 crate 边界扫描同款纪律）。
        for l in 0..=3 * CHUNK_DATA_MAX + 1 {
            let blob: Vec<u8> = (0..l).map(|i| (i % 251) as u8).collect();
            drain_stream(&[(b"appdemo", b"D", 64, 64, &blob)]);
        }
    }

    #[test]
    fn chunk_boundary_exact_192() {
        // 恰 192：单帧收干；恰 193：192 + 1 两帧（尾帧 1B 合法——粒度
        // 不执法，≤192 即收）。
        let blob192 = [0u8; 192];
        let frames = build_stream(&[(b"a", b"l", 1, 1, &blob192)]);
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[1].len(), CHUNK_FRAME_MAX);
        let blob193 = [0u8; 193];
        let frames = build_stream(&[(b"a", b"l", 1, 1, &blob193)]);
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[1].len(), CHUNK_FRAME_MAX);
        assert_eq!(frames[2].len(), CHUNK_HEAD + 1);
        // idx 序列恰 0,1（feed 面断言）。
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap();
        match dec.feed(&frames[1], frames[1].len()).unwrap() {
            ListFrame::Chunk { idx, data } => {
                assert_eq!(idx, 0);
                assert_eq!(data.len(), 192);
            }
            _ => panic!("应为 CHUNK"),
        }
        match dec.feed(&frames[2], frames[2].len()).unwrap() {
            ListFrame::Chunk { idx, data } => {
                assert_eq!(idx, 1);
                assert_eq!(data.len(), 1);
            }
            _ => panic!("应为 CHUNK"),
        }
    }

    #[test]
    fn chunk_rejects_oversized_and_empty() {
        // 手造 193B data 帧（wire 长度 198 ≤ 256 但超分片帽）→ 拒。
        let blob = [0u8; 193];
        let frames = build_stream(&[(b"a", b"l", 1, 1, &blob)]);
        let mut big = frames[1].clone();
        big.extend([0u8; 1]); // data 192 → 193
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap();
        assert_eq!(dec.feed(&big, big.len()), None);
        // 空 data 帧（恰 5B 头）→ 拒；4B 截断 → 拒。
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap();
        assert_eq!(dec.feed(&[OP_APP_CHUNK, 0, 0, 0, 0], 5), None);
        assert_eq!(dec.feed(&frames[1][..4], 4), None);
    }

    #[test]
    fn chunk_idx_replay_rejected() {
        // 重放：同 idx 再发（0c 钉拒面①）。blob 400B → 帧 0,1,2 后重发帧 1。
        let blob = [0u8; 400];
        let frames = build_stream(&[(b"a", b"l", 1, 1, &blob)]);
        let mut dec = ListDecoder::new();
        for f in &frames[..4] {
            // ENTRY + idx0 + idx1 + idx2
            dec.feed(f, f.len()).unwrap();
        }
        assert_eq!(dec.feed(&frames[2], frames[2].len()), None, "idx=1 重放应拒");
    }

    #[test]
    fn chunk_idx_disorder_rejected() {
        // 乱序：idx 回跳（0c 钉拒面②）。收 idx=0 后直接发 idx=2 帧。
        let blob = [0u8; 576]; // 3 分片
        let frames = build_stream(&[(b"a", b"l", 1, 1, &blob)]);
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap();
        dec.feed(&frames[1], frames[1].len()).unwrap(); // idx 0
        assert_eq!(dec.feed(&frames[3], frames[3].len()), None, "idx=2 跳过 1 应拒");
    }

    #[test]
    fn chunk_idx_skip_rejected() {
        // 跳帧：idx 0 → 2（非 +1 递进）——恰 +1 执法比「单调递增」更强：
        // 可靠有序 channel 上跳号 = 丢帧必拒（免 DESC 帽式两刀补课的
        // 一次到位面）。
        let blob = [0xAA; 385]; // 192+192+1
        let frames = build_stream(&[(b"a", b"l", 1, 1, &blob)]);
        let mut skip = frames[1].clone(); // idx=0 帧
        skip[1] = 2; // 伪造成 idx=2（data 192 ≤ remain 385 合法宽度）
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap();
        assert_eq!(dec.feed(&skip, skip.len()), None, "首分片 idx=2 应拒");
    }

    #[test]
    fn chunk_accounting_rejects() {
        // 收支溢出：data.len() > remain → 拒（blob 10B 来 192B 帧）。
        let blob = [0u8; 10];
        let frames = build_stream(&[(b"a", b"l", 1, 1, &blob)]);
        let mut wide = vec![OP_APP_CHUNK, 0, 0, 0, 0];
        wide.extend([0u8; 192]);
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap();
        assert_eq!(dec.feed(&wide, wide.len()), None, "分片溢出 blob_len 应拒");
        // 无待收：remain=0（空 blob 条目）后裸 chunk → 拒。
        let frames0 = build_stream(&[(b"a", b"l", 1, 1, &[])]);
        let mut dec = ListDecoder::new();
        dec.feed(&frames0[0], frames0[0].len()).unwrap();
        assert_eq!(dec.feed(&frames[1], frames[1].len()), None, "空 blob 后分片应拒");
    }

    #[test]
    fn entry_before_blob_done_rejected() {
        // 前条目 blob 未收满又开新条目 → 拒（截断流护面）。
        let blob = [0u8; 200];
        let frames = build_stream(&[(b"a", b"l", 1, 1, &blob), (b"b", b"l", 1, 1, &blob)]);
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap(); // ENTRY blob_len=200
        dec.feed(&frames[1], frames[1].len()).unwrap(); // idx 0（192B）
        assert_eq!(dec.feed(&frames[3], frames[3].len()), None, "欠收 8B 又开条目应拒");
    }

    #[test]
    fn end_accounting_rejects() {
        // 欠收即终：blob 200B 只收 1 分片（192B）就 END → 拒。
        let blob = [0u8; 200];
        let frames = build_stream(&[(b"a", b"l", 1, 1, &blob)]);
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap();
        dec.feed(&frames[1], frames[1].len()).unwrap();
        assert_eq!(dec.feed(&frames[3], frames[3].len()), None, "blob 欠收 END 应拒");
        // n 对账：n-1 / n+1 皆拒（收满后改终帧 n）。
        let frames = build_stream(&[(b"a", b"l", 1, 1, &[]), (b"b", b"l", 1, 1, &[])]);
        for n in [0u32, 1, 3] {
            let mut end = [0u8; END_LEN];
            encode_list_end(&mut end, n);
            let mut dec = ListDecoder::new();
            dec.feed(&frames[0], frames[0].len()).unwrap();
            dec.feed(&frames[1], frames[1].len()).unwrap();
            assert_eq!(dec.feed(&end, END_LEN), None, "END n={n} 失配应拒");
        }
        // END 长度 4 / 6 皆拒（恰长执法）。
        let mut end = [0u8; END_LEN];
        encode_list_end(&mut end, 1);
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap();
        assert_eq!(dec.feed(&end[..4], 4), None);
        assert_eq!(dec.feed(&[end.as_slice(), &[0u8]].concat(), 6), None);
        // END 后尾随帧（含再 END）→ 拒。
        let mut dec = ListDecoder::new();
        dec.feed(&frames[0], frames[0].len()).unwrap();
        dec.feed(&frames[1], frames[1].len()).unwrap();
        let mut end2 = [0u8; END_LEN];
        encode_list_end(&mut end2, 2); // n=2 与已收两条目对账恰过
        dec.feed(&end2, END_LEN).unwrap();
        assert!(dec.finished());
        assert_eq!(dec.feed(&end2, END_LEN), None, "END 后尾随应拒");
    }

    #[test]
    fn empty_list_end_only() {
        // 空清单：END{0} 首帧恰收（无 app 形态——launcher 回默认桌面）。
        let mut end = [0u8; END_LEN];
        let n = encode_list_end(&mut end, 0);
        assert_eq!(n, END_LEN);
        assert_eq!(&end[..END_LEN], &[OP_APP_LIST_END, 0, 0, 0, 0][..]);
        let mut dec = ListDecoder::new();
        assert_eq!(dec.feed(&end, END_LEN), Some(ListFrame::End { n: 0 }));
        assert!(dec.finished());
    }

    #[test]
    fn decoder_rejects_unknown_op() {
        // 野 op：0x0F / 0x14（高带外）/ 请求面 op 泄漏（1、2）/ ACK 混流。
        for op in [0u8, 0x0F, OP_APP_LIST_REQ, OP_APP_SPAWN_REQ, OP_APP_EVENT, 0x14] {
            let f = [op, 0, 0, 0, 0, 0];
            assert_eq!(ListDecoder::new().feed(&f, f.len()), None, "野 op {op:#x} 应拒");
        }
        let mut ack = [0u8; ACK_LEN];
        encode_spawn_ack(&mut ack, true, 0).unwrap();
        assert_eq!(ListDecoder::new().feed(&ack, ACK_LEN), None);
    }

    #[test]
    fn spawn_ack_roundtrip_and_rejects() {
        let mut buf = [0u8; ACK_LEN];
        // 成功 / 失败双形态往返 + 黄金钉板。
        assert_eq!(encode_spawn_ack(&mut buf, true, 0), Some(ACK_LEN));
        assert_eq!(&buf[..ACK_LEN], &[OP_APP_SPAWN_ACK, 1, 0, 0, 0, 0][..]);
        assert_eq!(parse_spawn_ack(&buf, ACK_LEN), Some((true, 0)));
        assert_eq!(encode_spawn_ack(&mut buf, false, 7), Some(ACK_LEN));
        assert_eq!(&buf[..ACK_LEN], &[OP_APP_SPAWN_ACK, 0, 7, 0, 0, 0][..]);
        assert_eq!(parse_spawn_ack(&buf, ACK_LEN), Some((false, 7)));
        // err 顶格 LE 往返。
        assert_eq!(encode_spawn_ack(&mut buf, false, u32::MAX), Some(ACK_LEN));
        assert_eq!(parse_spawn_ack(&buf, ACK_LEN), Some((false, u32::MAX)));
        assert_eq!(buf[2], 0xFF); // LE 钉板：err 首字节 = 最低位字节
        // 编码耦合律拒面：ok=1 携 err / ok=0 零 err / buf 不足。
        assert_eq!(encode_spawn_ack(&mut buf, true, 1), None);
        assert_eq!(encode_spawn_ack(&mut buf, false, 0), None);
        let mut tiny = [0u8; ACK_LEN - 1];
        assert_eq!(encode_spawn_ack(&mut tiny, true, 0), None);
        // 解析拒面：野 ok=2 / 耦合失配双形 / 截断 / 尾随 / 野帧头。
        let mut bad = buf;
        bad[1] = 2;
        assert_eq!(parse_spawn_ack(&bad, ACK_LEN), None);
        let bad = [OP_APP_SPAWN_ACK, 1, 9, 0, 0, 0];
        assert_eq!(parse_spawn_ack(&bad, ACK_LEN), None);
        let bad = [OP_APP_SPAWN_ACK, 0, 0, 0, 0, 0];
        assert_eq!(parse_spawn_ack(&bad, ACK_LEN), None);
        let ok = [OP_APP_SPAWN_ACK, 1, 0, 0, 0, 0];
        for cut in 0..ACK_LEN {
            assert_eq!(parse_spawn_ack(&ok[..cut], cut), None, "截断 @{cut} 应拒");
        }
        let mut tail = ok.to_vec();
        tail.push(0);
        assert_eq!(parse_spawn_ack(&tail, tail.len()), None);
        let who = [OP_APP_LIST_END, 1, 0, 0, 0, 0];
        assert_eq!(parse_spawn_ack(&who, ACK_LEN), None);
    }

    #[test]
    fn err_reply_roundtrip() {
        let mut buf = [0u8; 2];
        assert_eq!(encode_err_reply(&mut buf), 1);
        assert_eq!(buf[0], ST_BAD);
        assert_eq!(parse_err_reply(&buf, 1), Some(()));
        // 拒面：尾随字节 / ST_OK 裸帧（无语义形）/ 野值。
        assert_eq!(parse_err_reply(&buf, 2), None);
        assert_eq!(parse_err_reply(&[ST_OK], 1), None);
        assert_eq!(parse_err_reply(&[2], 1), None);
        assert_eq!(parse_err_reply(&buf, 0), None);
    }

    #[test]
    fn lying_len_rejected() {
        // 谎报 len 角：len > buf 实长必拒——kernel recv 恒写真 nb 故实跑
        // 不可达，协议面单独钉板（psproto 同款纪律）。
        let mut buf = [0u8; SPAWN_REQ_LEN];
        encode_spawn_request(&mut buf, b"app").unwrap();
        assert_eq!(parse_spawn_request(&buf[..3], 99), None);
        let mut ack = [0u8; ACK_LEN];
        encode_spawn_ack(&mut ack, true, 0).unwrap();
        assert_eq!(parse_spawn_ack(&ack[..2], ACK_LEN), None);
        let mut end = [0u8; END_LEN];
        encode_list_end(&mut end, 0);
        assert_eq!(ListDecoder::new().feed(&end[..2], END_LEN), None);
    }

    #[test]
    fn chunk_stream_encoder_equivalence() {
        // ChunkStream（流式 fill 形）↔ ChunkEncoder（整 blob 借用形）逐字节
        // 等价——同帧长同 idx 同切片（两实现单源纪律的等价钉；含 0/1/恰
        // 192/越界 193/多帧尾零头各形）。
        for len in [0usize, 1, 191, 192, 193, 192 * 3, 192 * 3 + 50] {
            let blob: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let mut a = [0u8; FRAME_MAX];
            let mut b = [0u8; FRAME_MAX];
            let mut enc = ChunkEncoder::new(&blob);
            let mut stream = ChunkStream::new(len);
            let mut fill = |dst: &mut [u8], off: usize| {
                dst.copy_from_slice(&blob[off..off + dst.len()]);
                true
            };
            loop {
                let fa = enc.next(&mut a);
                let fb = stream.next(&mut b, &mut fill);
                assert_eq!(fa, fb, "帧长漂移 len={len}");
                match (fa, fb) {
                    (Some(na), Some(nb)) => {
                        assert_eq!(&a[..na], &b[..nb], "帧字节漂移 len={len}");
                    }
                    (None, None) => break,
                    _ => unreachable!(),
                }
            }
        }
    }

    #[test]
    fn chunk_stream_fill_failure_zero_pads() {
        // fill 读败契约：败帧起零填充、流长恒与声明一致、idx 连贯——解码
        // 侧对账不破（服务端零窗流式的读败容错面，坏由 eco 可见化）。
        let blob: Vec<u8> = (0..500usize).map(|i| (i % 251) as u8).collect();
        let mut stream = ChunkStream::new(500);
        let mut buf = [0u8; FRAME_MAX];
        // off ≥ 192 起读败（首帧 192B 全真，后续帧全零填）。
        let mut fill = |dst: &mut [u8], off: usize| {
            if off >= 192 {
                return false;
            }
            dst.copy_from_slice(&blob[off..off + dst.len()]);
            true
        };
        let n1 = stream.next(&mut buf, &mut fill).unwrap();
        let mut want = [0u8; FRAME_MAX];
        let mut enc = ChunkEncoder::new(&blob);
        let w1 = enc.next(&mut want).unwrap();
        assert_eq!(&buf[..n1], &want[..w1], "首帧（读败前）须与编码器等价");
        // 余帧：500-192=308 → 192 + 116 两帧零载荷、idx 1/2 连贯。
        let mut rest = Vec::new();
        while let Some(n) = stream.next(&mut buf, &mut fill) {
            rest.extend_from_slice(&buf[..n]);
        }
        assert_eq!(rest.len(), (CHUNK_HEAD + 192) + (CHUNK_HEAD + 116));
        let idx = |f: &[u8]| u32::from_le_bytes([f[1], f[2], f[3], f[4]]);
        let (f2, f3) = rest.split_at(CHUNK_HEAD + 192);
        assert_eq!(idx(f2), 1);
        assert!(f2[CHUNK_HEAD..].iter().all(|&b| b == 0), "败帧须零填");
        assert_eq!(idx(f3), 2);
        assert!(f3[CHUNK_HEAD..].iter().all(|&b| b == 0), "败帧须零填");
    }
}
