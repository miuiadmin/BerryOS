//! berry-blkproto —— 块设备服务 RPC 协议纯函数库（P6 A-4，`berry-fsproto`
//! 先例：「解析器即产品」，零分配、零 unsafe、零依赖，输入只借用）。
//!
//! 拓扑：vblk（常驻块设备驱动服务，`user/bins/vblk`）持通道一端（env 块
//! gift[2]，libc `gift2_h()`），svcmgr（A-4 起为消费自验方，A-5 起 FS 服务
//! 接棒）持对端——**单对双向 + 严格串行 RPC**（一请求一应答，队列深 1 用
//! 量，无乱序死锁面；fsmgr/bsh 同款约定）。本库只定字节布局与编解码，
//! 通道/等待语义在两端各自实现。
//!
//! **数据面走 VMO 不走 inline**：单扇区 512B > channel inline 帽 256B——
//! 请求随行 1 枚 VMO handle（client 侧 dup 出 READ|WRITE 别名随 send 迁移，
//! 本体留 client 表；driver bounce-buffer 后 close 别名），内容在 VMO 偏移
//! 0、恰 [`SECTOR_SZ`] 字节。写 = client 预填、driver 读走；读 = driver 回
//! 填、client 收割。**client 契约：随行 handle 必须是 dup 出的别名**——不
//! 得送已映射进自己地址空间的唯一 handle（内核对映射中 VMO 的销毁性
//! close 拒 -EBUSY，driver 回收别名即卡死；零拷贝写法须先 dup 再送）。
//! **扇区粒度定值 512**（virtio-blk 数据面单扇区语义；
//! 部分扇区/多扇区合并随 v2 扩 `len` 字段——解析面届时收紧，v1 恰长即无
//! 歧义）。
//!
//! 布局（v1，全定宽、LE）：
//! - 请求 `[op u8][sector u64]` = [`REQ_LEN`] 9 B（恰长，尾随字节 = 野值拒）
//! - 应答 `[st u8]` = 1 B
//!
//! 错误面（截断/野值/尾随/未知码）host 单测全盖——「AI 测试友好化」：协议
//! 正确性不依赖 QEMU 就能验证。编码器 None 拒绝契约：编不出就拒绝、不截
//! 断（含空 out 切片——`berry-fsproto` 对抗审查实抓的同族坑，day-one 盖）。

#![cfg_attr(not(test), no_std)]

/// 读扇区：driver 读盘 → 数据回填随行 VMO。
pub const OP_READ: u8 = 1;
/// 写扇区：随行 VMO 数据 → driver 落盘。
pub const OP_WRITE: u8 = 2;

/// 应答状态：成功。
pub const ST_OK: u8 = 0;
/// 应答状态：请求坏（解析失败 / 随行 VMO 缺或多 / VMO 访问失败）。
pub const ST_BAD: u8 = 1;
/// 应答状态：扇区越界（≥ 设备容量——driver 侧判，client 不必预知容量）。
pub const ST_RANGE: u8 = 2;

/// 扇区粒度（字节）：随行 VMO 的恰长内容（virtio-blk 单扇区）。
pub const SECTOR_SZ: usize = 512;

/// 请求线上长度。
pub const REQ_LEN: usize = 9;

/// 请求（解析产物）。
#[derive(Debug, PartialEq, Eq)]
pub enum Req {
    /// 读扇区。
    Read { sector: u64 },
    /// 写扇区。
    Write { sector: u64 },
}

/// 编码请求（恰 [`REQ_LEN`] 字节；坏 op / out 太短 → None，不截断）。
pub fn encode_request(out: &mut [u8], op: u8, sector: u64) -> Option<usize> {
    if op != OP_READ && op != OP_WRITE {
        return None;
    }
    if out.len() < REQ_LEN {
        return None;
    }
    out[0] = op;
    out[1..9].copy_from_slice(&sector.to_le_bytes());
    Some(REQ_LEN)
}

/// 解析请求（恰长；坏 op / 截断 / 尾随 → None）。
pub fn parse_request(buf: &[u8]) -> Option<Req> {
    if buf.len() != REQ_LEN {
        return None;
    }
    let sector = u64::from_le_bytes(buf[1..9].try_into().ok()?);
    match buf[0] {
        OP_READ => Some(Req::Read { sector }),
        OP_WRITE => Some(Req::Write { sector }),
        _ => None,
    }
}

/// 编码应答（1 字节；未知 st / 空 out → None）。
pub fn encode_reply(out: &mut [u8], st: u8) -> Option<usize> {
    if st != ST_OK && st != ST_BAD && st != ST_RANGE {
        return None;
    }
    if out.is_empty() {
        return None;
    }
    out[0] = st;
    Some(1)
}

/// 解析应答（恰 1 字节且 st 在册；截断 / 尾随 / 未知码 → None）。
pub fn parse_reply(buf: &[u8]) -> Option<u8> {
    if buf.len() != 1 {
        return None;
    }
    let st = buf[0];
    if st == ST_OK || st == ST_BAD || st == ST_RANGE {
        Some(st)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip() {
        // 边界扇区号三档：0 / 常规 / u64::MAX（协议不设盘容门——那是
        // driver 侧 ST_RANGE 的职责，线格式须能表达全域）。
        let mut b = [0u8; 16];
        for (op, sector) in [(OP_READ, 0u64), (OP_WRITE, 5), (OP_READ, u64::MAX)] {
            let n = encode_request(&mut b, op, sector).unwrap();
            assert_eq!(n, REQ_LEN);
            let expect = match op {
                OP_READ => Req::Read { sector },
                _ => Req::Write { sector },
            };
            assert_eq!(parse_request(&b[..n]), Some(expect));
        }
    }

    #[test]
    fn request_short_rejected() {
        // 8 字节（截断在 sector 高位）→ None。
        let m = [OP_READ, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(parse_request(&m), None);
    }

    #[test]
    fn request_trailing_rejected() {
        // 10 字节（尾随野值）→ None——恰长执法。
        let mut m = [0u8; 10];
        let n = encode_request(&mut m, OP_WRITE, 9).unwrap();
        assert_eq!(parse_request(&m[..n + 1]), None);
    }

    #[test]
    fn request_empty_rejected() {
        assert_eq!(parse_request(&[]), None);
    }

    #[test]
    fn request_bad_op_rejected() {
        // op ∈ {0, 3, 0xFF}（越界/保留值）→ None；解析与编码双面。
        for op in [0u8, 3, 0xFF] {
            let mut m = [op, 0, 0, 0, 0, 0, 0, 0, 0];
            assert_eq!(parse_request(&m), None);
            let mut b = [0u8; 16];
            assert_eq!(encode_request(&mut b, op, 0), None);
        }
    }

    #[test]
    fn encode_out_too_small() {
        // out 8 字节（< REQ_LEN）→ None 不截断。
        let mut b = [0u8; 8];
        assert_eq!(encode_request(&mut b, OP_READ, 1), None);
    }

    #[test]
    fn encode_empty_out() {
        // 空 out：全部编码器一律 None（None 拒绝契约 day-one 面）。
        assert_eq!(encode_request(&mut [], OP_READ, 1), None);
        assert_eq!(encode_reply(&mut [], ST_OK), None);
    }

    #[test]
    fn out_exact_fit() {
        // out 恰好等于所需容量：容量门回归成 <= 时本测显形（fsproto
        // out_exact_fit 同款——此前只测过小侧的盲区；encode_request 侧尤
        // 必要：svcmgr 消费面用 16B out，请求门回归无运行期守卫）。
        let mut b = [0u8; REQ_LEN]; // 恰 9
        assert_eq!(encode_request(&mut b, OP_READ, 1), Some(REQ_LEN));
        assert_eq!(parse_request(&b), Some(Req::Read { sector: 1 }));
        let mut r = [0u8; 1]; // 恰 1
        assert_eq!(encode_reply(&mut r, ST_OK), Some(1));
        assert_eq!(parse_reply(&r), Some(ST_OK));
    }

    #[test]
    fn wire_format_pinned() {
        // 冻结布局钉板（fsproto wire_name_exact_max 同款）：手搓线字节直断
        // 解析产物 + 直断编码输出内容——不经编码器的对称往返自证。编码/
        // 解析被一致改端序或字段序时 roundtrip 仍绿，此处炸响。
        let m = [OP_WRITE, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01];
        assert_eq!(
            parse_request(&m),
            Some(Req::Write { sector: 0x0102030405060708 })
        );
        let s = 0x1122334455667788u64;
        let mut b = [0u8; REQ_LEN];
        encode_request(&mut b, OP_READ, s).unwrap();
        assert_eq!(b[0], OP_READ);
        assert_eq!(b[1..9], s.to_le_bytes());
        let mut r = [0u8; 1];
        encode_reply(&mut r, ST_RANGE).unwrap();
        assert_eq!(r[0], ST_RANGE);
    }

    #[test]
    fn reply_roundtrip() {
        // 三个在册 st 全域往返。
        for st in [ST_OK, ST_BAD, ST_RANGE] {
            let mut b = [0u8; 4];
            let n = encode_reply(&mut b, st).unwrap();
            assert_eq!(n, 1);
            assert_eq!(parse_reply(&b[..n]), Some(st));
        }
    }

    #[test]
    fn reply_empty_rejected() {
        assert_eq!(parse_reply(&[]), None);
    }

    #[test]
    fn reply_trailing_rejected() {
        // 2 字节（应答后尾随）→ None。
        let m = [ST_OK, 0];
        assert_eq!(parse_reply(&m), None);
    }

    #[test]
    fn reply_unknown_st_rejected() {
        // 未知 st（3 起 / 0xFF）双面拒：解析 None + 编码 None。
        for st in [3u8, 0xFF] {
            assert_eq!(parse_reply(&[st]), None);
            let mut b = [0u8; 1];
            assert_eq!(encode_reply(&mut b, st), None);
        }
    }
}
