//! berry-psproto —— ps 查询 RPC 协议纯函数库（C-3 段①，`berry-fsproto`
//! 先例：零分配、零 unsafe、零依赖，输入只借用）。
//!
//! 拓扑：svcmgr 持通道一端（ps_a），ps bin 持对端（ps_b——随 env 块
//! gift[2] 迁入，libc `gift2_h()` 消费，见 svcmgr `ps_selftest`）——单对
//! 双向 + 严格串行 RPC（一请求一应答，队列深 1 用量，fsproto 同款纪律）。
//! 真值源 = svcmgr 拉起账本（「表驱动、零新内核面」，C-3 钦定）。
//!
//! 消息 = channel inline 单帧（≤[`REPLY_MAX`] ≤ `CHAN_MSG_MAX`=256B）。
//! 全数值 LE。布局（v0，全定宽）：
//!
//! - 请求 `[op u8]`：恰 1 字节（尾随字节 = 野值拒）——PS_LIST(1) 单 op。
//! - 应答 `[st u8][count u8]` + count × `[name ≤ NAME_MAX B，NUL 终止]
//!   [state u8][code i32 LE]`：
//!   - st = [`ST_OK`]/[`ST_BAD`]（v1 无诊断面——畸形即拒，客户端打一行
//!     诊断退出，标记缺失 = smoke FAIL 兜底）；
//!   - state = [`STATE_ALIVE`]/[`STATE_EXITED`]/[`STATE_SPAWNFAIL`]；code
//!     仅 EXITED 行有意义，编码仍全宽携带（定宽优先，消费端自辨）。
//!
//! 严格解析：count 超帽/名字空或超长/缺 NUL/截断/尾随字节/未知 state/
//! 未知 st 一律 None（「编不出就拒绝，不截断」——截断的表是谎言表）。

#![cfg_attr(not(test), no_std)]

/// 拉全表（v0 单 op；按 pid/名过滤留 v2 随交互面）。
pub const OP_PS_LIST: u8 = 1;

/// 应答状态码。
pub const ST_OK: u8 = 0;
pub const ST_BAD: u8 = 1;

/// 行状态：拉起即常驻（Alive）/ 已退出（code 有效）/ 拉起失败（spawn Err）。
pub const STATE_ALIVE: u8 = 1;
pub const STATE_EXITED: u8 = 2;
pub const STATE_SPAWNFAIL: u8 = 3;

/// 名宽 = 8.3 基名同宽（账本行名：svcmgr/echo/talker/echo2/hello/vblk/
/// fsmgr/vnet/stdhello/bsh/appdemo/appquiet/ps 全 ≤8）。
pub const NAME_MAX: usize = 8;
/// 行数帽（账本构造上界——常驻 + 启动批 + ps 自身；恰 [`MAX_ENTRIES`] 同源）。
pub const MAX_ENTRIES: usize = 16;
/// 请求恰长。
pub const REQUEST_LEN: usize = 1;
/// 应答帧帽 = 头 2B + 行最坏 [`NAME_MAX`]+1(NUL)+1(state)+4(code)。
pub const REPLY_MAX: usize = 2 + MAX_ENTRIES * (NAME_MAX + 1 + 1 + 4);
const _: () = assert!(REPLY_MAX <= 256, "REPLY_MAX 须落在 CHAN_MSG_MAX=256 内");

/// 编码请求（buf ≥ [`REQUEST_LEN`]；恒成功——定宽 1B）。
pub fn encode_request(buf: &mut [u8]) -> usize {
    buf[0] = OP_PS_LIST;
    REQUEST_LEN
}

/// 解析请求：恰 1B 且 op 匹配；否则 None（服务端以 ST_BAD 应答拒面）。
pub fn parse_request(buf: &[u8], len: usize) -> Option<()> {
    if len == REQUEST_LEN && buf.len() >= len && buf[0] == OP_PS_LIST {
        Some(())
    } else {
        None
    }
}

/// 应答增量构造器（服务端）：`new` 写 st/count 占位 → `push` 逐行追加 →
/// `finish` 回填 count 返总长。push 拒绝面（表满/名空或超长或含 NUL/
/// 未知 state/帧帽尽）返 false——调用方决策（账本按构造 ≤ 帽，false =
/// 内部超编，loud 降级不静默截断）。
pub struct ReplyBuilder<'a> {
    buf: &'a mut [u8],
    pos: usize,
    count: u8,
}

impl<'a> ReplyBuilder<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        buf[0] = ST_OK;
        buf[1] = 0;
        Self { buf, pos: 2, count: 0 }
    }

    /// 追加一行；可接纳才返 true（全宽 code，见模块注）。
    pub fn push(&mut self, name: &[u8], state: u8, code: i32) -> bool {
        if self.count as usize >= MAX_ENTRIES
            || name.is_empty()
            || name.len() > NAME_MAX
            || name.contains(&0)
            || !valid_state(state)
            || self.pos + name.len() + 6 > self.buf.len()
        {
            return false;
        }
        self.buf[self.pos..self.pos + name.len()].copy_from_slice(name);
        self.pos += name.len();
        self.buf[self.pos] = 0; // NUL 终止
        self.pos += 1;
        self.buf[self.pos] = state;
        self.pos += 1;
        self.buf[self.pos..self.pos + 4].copy_from_slice(&code.to_le_bytes());
        self.pos += 4;
        self.count += 1;
        true
    }

    /// 回填 count，返帧总长。
    pub fn finish(self) -> usize {
        self.buf[1] = self.count;
        self.pos
    }
}

/// 解析应答（客户端）：st≠ST_OK 返 None（调用方打诊断行）；逐行回调
/// `(name, state, code)`。畸形（截断/名空或超长/缺 NUL/尾随字节/未知
/// state/count 超帽）整体 None——半张表不进渲染面。
pub fn parse_reply(
    buf: &[u8],
    len: usize,
    mut on_row: impl FnMut(&[u8], u8, i32),
) -> Option<()> {
    if len < 2 || len > buf.len() || buf[0] != ST_OK {
        return None;
    }
    let count = buf[1] as usize;
    if count > MAX_ENTRIES {
        return None;
    }
    let mut rest = &buf[2..len];
    for _ in 0..count {
        // 名段：NAME_MAX+1 窗内找 NUL（窗外即超长）。
        let nul = rest.iter().take(NAME_MAX + 1).position(|&b| b == 0)?;
        if nul == 0 || nul > NAME_MAX || rest.len() < nul + 6 {
            return None;
        }
        let state = rest[nul + 1];
        if !valid_state(state) {
            return None;
        }
        let code = i32::from_le_bytes([
            rest[nul + 2],
            rest[nul + 3],
            rest[nul + 4],
            rest[nul + 5],
        ]);
        on_row(&rest[..nul], state, code);
        rest = &rest[nul + 6..];
    }
    if !rest.is_empty() {
        return None; // 恰长执法——尾随字节 = 畸形
    }
    Some(())
}

fn valid_state(s: u8) -> bool {
    s == STATE_ALIVE || s == STATE_EXITED || s == STATE_SPAWNFAIL
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 通用 roundtrip：编码 → 解析回调收表 → 与输入对拍。
    fn roundtrip(rows: &[(&[u8], u8, i32)]) {
        let mut buf = [0u8; REPLY_MAX];
        let mut b = ReplyBuilder::new(&mut buf);
        for &(n, s, c) in rows {
            assert!(b.push(n, s, c), "push 应接纳 {:?}", n);
        }
        let len = b.finish();
        let mut out: Vec<(Vec<u8>, u8, i32)> = Vec::new();
        assert_eq!(parse_reply(&buf, len, |n, s, c| out.push((n.to_vec(), s, c))), Some(()));
        assert_eq!(out.len(), rows.len());
        for (got, want) in out.iter().zip(rows.iter()) {
            assert_eq!(got.0.as_slice(), want.0);
            assert_eq!(got.1, want.1);
            assert_eq!(got.2, want.2);
        }
    }

    #[test]
    fn request_roundtrip_and_rejects() {
        let mut buf = [0u8; 4];
        let n = encode_request(&mut buf);
        assert_eq!(n, 1);
        assert_eq!(parse_request(&buf, n), Some(()));
        // 恰长执法：尾随字节/空/错 op 全拒。
        assert_eq!(parse_request(&buf, 2), None);
        assert_eq!(parse_request(&buf, 0), None);
        buf[0] = OP_PS_LIST + 1;
        assert_eq!(parse_request(&buf, 1), None);
    }

    #[test]
    fn reply_empty_table() {
        roundtrip(&[]);
    }

    #[test]
    fn reply_mixed_states_and_negative_code() {
        roundtrip(&[
            (b"svcmgr", STATE_ALIVE, 0),
            (b"stdhello", STATE_EXITED, 0),
            (b"appquiet", STATE_EXITED, -1), // 负码 LE 往返
            (b"bsh", STATE_ALIVE, 0),
            (b"vblk", STATE_SPAWNFAIL, 0),
        ]);
    }

    #[test]
    fn reply_full_table_boundary() {
        // 16 行 × 最宽名 8B = REPLY_MAX 恰满（builder 用整帧 buf）。
        let rows: Vec<(&[u8], u8, i32)> = (0..MAX_ENTRIES)
            .map(|i| {
                let name: &'static [u8] = match i {
                    0 => b"eightchr",
                    1 => b"svcmgr01",
                    2 => b"svcmgr02",
                    3 => b"svcmgr03",
                    4 => b"svcmgr04",
                    5 => b"svcmgr05",
                    6 => b"svcmgr06",
                    7 => b"svcmgr07",
                    8 => b"svcmgr08",
                    9 => b"svcmgr09",
                    10 => b"svcmgr10",
                    11 => b"svcmgr11",
                    12 => b"svcmgr12",
                    13 => b"svcmgr13",
                    14 => b"svcmgr14",
                    _ => b"svcmgr15",
                };
                (name, STATE_ALIVE, i as i32)
            })
            .collect();
        let mut buf = [0u8; REPLY_MAX];
        let mut b = ReplyBuilder::new(&mut buf);
        for &(n, s, c) in &rows {
            assert!(b.push(n, s, c));
        }
        assert_eq!(b.finish(), REPLY_MAX); // 帽边界恰满
        roundtrip(&rows);
    }

    #[test]
    fn builder_push_rejects() {
        let mut buf = [0u8; REPLY_MAX];
        let mut b = ReplyBuilder::new(&mut buf);
        assert!(!b.push(b"", STATE_ALIVE, 0)); // 空名
        assert!(!b.push(b"ninechars", STATE_ALIVE, 0)); // 9B 超宽
        assert!(!b.push(b"ni\0l", STATE_ALIVE, 0)); // 内嵌 NUL
        assert!(!b.push(b"nil", 0, 0)); // 未知 state（0 非法）
        assert!(!b.push(b"nil", 4, 0)); // 未知 state（4 非法）
        assert!(b.push(b"ok", STATE_ALIVE, 0));
        // 表满拒：灌到 16 行后第 17 行 false。
        let mut buf2 = [0u8; REPLY_MAX];
        let mut b2 = ReplyBuilder::new(&mut buf2);
        for i in 0..MAX_ENTRIES {
            assert!(b2.push(b"ab", STATE_ALIVE, i as i32));
        }
        assert!(!b2.push(b"cd", STATE_ALIVE, 0));
    }

    #[test]
    fn parse_rejects_malformed() {
        // 基准帧：1 行。
        let mut buf = [0u8; REPLY_MAX];
        let mut b = ReplyBuilder::new(&mut buf);
        assert!(b.push(b"echo", STATE_ALIVE, 0));
        let len = b.finish();
        // 截断：每处切点全拒。
        for cut in 0..len {
            assert_eq!(
                parse_reply(&buf[..cut.max(0)], cut, |_, _, _| ()),
                None,
                "截断 @{} 应拒",
                cut
            );
        }
        // 尾随字节。
        let mut tail = buf;
        tail[len] = b'x';
        assert_eq!(parse_reply(&tail, len + 1, |_, _, _| ()), None);
        // count 超帽（改 count=17）。
        let mut over = buf;
        over[1] = MAX_ENTRIES as u8 + 1;
        assert_eq!(parse_reply(&over, len, |_, _, _| ()), None);
        // st 非 OK。
        let mut bad = buf;
        bad[0] = ST_BAD;
        assert_eq!(parse_reply(&bad, len, |_, _, _| ()), None);
        // len < 2。
        assert_eq!(parse_reply(&buf, 1, |_, _, _| ()), None);
    }

    #[test]
    fn parse_rejects_bad_names_and_state() {
        // 名超长：9B 无 NUL（count 声明 1，名段 NAME_MAX+1 窗内无 NUL）。
        let mut buf = [0u8; 64];
        buf[0] = ST_OK;
        buf[1] = 1;
        buf[2..11].copy_from_slice(b"ninechars");
        buf[11] = STATE_ALIVE;
        buf[12..16].copy_from_slice(&0i32.to_le_bytes());
        let len = 16;
        assert_eq!(parse_reply(&buf, len, |_, _, _| ()), None);
        // 未知 state 字节。
        let mut buf2 = [0u8; 64];
        buf2[0] = ST_OK;
        buf2[1] = 1;
        buf2[2..6].copy_from_slice(b"echo");
        buf2[6] = 0; // NUL
        buf2[7] = 9; // 野 state
        buf2[8..12].copy_from_slice(&0i32.to_le_bytes());
        assert_eq!(parse_reply(&buf2, 12, |_, _, _| ()), None);
    }

    #[test]
    fn reply_wire_golden() {
        // 黄金字节钉板：2 行定帧逐字节（改布局 = 此测必炸——fsproto wire
        // 钉板同款纪律）。
        let mut buf = [0u8; REPLY_MAX];
        let mut b = ReplyBuilder::new(&mut buf);
        assert!(b.push(b"bsh", STATE_ALIVE, 0));
        assert!(b.push(b"appdemo", STATE_EXITED, 3));
        let len = b.finish();
        // 行 = name + NUL + state + code(4) = name_len + 6。
        assert_eq!(len, 2 + (3 + 6) + (7 + 6));
        let mut want = vec![ST_OK, 2];
        want.extend_from_slice(b"bsh\0");
        want.push(STATE_ALIVE);
        want.extend_from_slice(&0i32.to_le_bytes());
        want.extend_from_slice(b"appdemo\0");
        want.push(STATE_EXITED);
        want.extend_from_slice(&3i32.to_le_bytes());
        assert_eq!(&buf[..len], want.as_slice());
    }
}
