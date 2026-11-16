//! W3-Q84 host 钉板：display 模式请求五门判定链全表。

use crate::{parse_display_mode, MAX_ITEM_BYTES};

#[test]
fn happy_path_full_chain() {
    // 全门全过：合法 WxH（含 @Hz 后缀截断 + NUL 填充 trim）。
    assert_eq!(parse_display_mode(b"1024x768"), Some((1024, 768)));
    assert_eq!(parse_display_mode(b"1280x720@60"), Some((1280, 720)));
    assert_eq!(parse_display_mode(b"640x480@59.94"), Some((640, 480)));
    assert_eq!(parse_display_mode(b"1920x1080\0"), Some((1920, 1080)));
    assert_eq!(parse_display_mode(b"\0 800x600 \0"), Some((800, 600)));
    // 空白族全收（ASCII whitespace）：space/tab/CR/LF。
    assert_eq!(parse_display_mode(b" \t1024x768\r\n"), Some((1024, 768)));
}

#[test]
fn size_gate_boundaries() {
    // 门 1：size 域 (0, 32]。
    assert_eq!(parse_display_mode(b""), None); // 空项
    // 恰 32B 过：W=7 位 + x + H=10 位 = 18 字符 + 14 NUL 填充 = 32。
    let mut buf = [0u8; MAX_ITEM_BYTES];
    buf[..18].copy_from_slice(b"1111111x1111111111");
    assert_eq!(parse_display_mode(&buf), Some((1111111, 1111111111)));
    // 33B 拒：同样式 + 15 NUL。
    let mut big = [0u8; MAX_ITEM_BYTES + 1];
    big[..18].copy_from_slice(b"1111111x1111111111");
    assert_eq!(parse_display_mode(&big), None);
    // kernel 侧 e.size 门与本地帽同值执法（单源锚）。
    assert_eq!(MAX_ITEM_BYTES, 32);
}

#[test]
fn utf8_gate() {
    // 门 2：畸形 UTF-8 拒。
    assert_eq!(parse_display_mode(&[0xff, b'x', b'1']), None);
    assert_eq!(parse_display_mode(b"102\xF0\x80x768"), None);
}

#[test]
fn at_suffix_and_missing_x() {
    // 门 4/5：@ 截断后无 x 拒；x 缺席拒。
    assert_eq!(parse_display_mode(b"1024"), None); // 无 x
    assert_eq!(parse_display_mode(b"1024@"), None); // @ 截断后无 x
    assert_eq!(parse_display_mode(b"x768"), None); // W 空
    assert_eq!(parse_display_mode(b"1024x"), None); // H 空
    assert_eq!(parse_display_mode(b"1024xx768"), None); // 双 x：H="x768" parse 拒
    // 多 @：split('@').next() 取首段。
    assert_eq!(parse_display_mode(b"800x600@60@60"), Some((800, 600)));
}

#[test]
fn decimal_domain() {
    // 门 5：u32 域——越界拒、零/前导零合法、非十进制拒。
    assert_eq!(parse_display_mode(b"99999999999x1"), None); // W > u32::MAX
    assert_eq!(parse_display_mode(b"1x4294967296"), None); // H = u32::MAX+1
    assert_eq!(parse_display_mode(b"4294967295x1"), Some((u32::MAX, 1))); // 恰界
    assert_eq!(parse_display_mode(b"0x0"), Some((0, 0))); // 零合法（kernel 消费侧自决）
    assert_eq!(parse_display_mode(b"01024x0768"), Some((1024, 768))); // 前导零
    assert_eq!(parse_display_mode(b"0x10x768"), None); // hex 前缀形 parse 拒
    assert_eq!(parse_display_mode(b"1_024x768"), None); // 下划线拒
    assert_eq!(parse_display_mode(b"+1024x768"), Some((1024, 768))); // 前导 + 合法（u32 parse 域）
    assert_eq!(parse_display_mode(b"-1024x768"), None); // 负号拒
    assert_eq!(parse_display_mode(b"1024X768"), None); // 大写 X 拒（大小写敏感）
}

#[test]
fn trim_only_then_reject() {
    // trim 后空串 → split_once 拒。
    assert_eq!(parse_display_mode(b"\0\0\0"), None);
    assert_eq!(parse_display_mode(b"   "), None);
    // 首段 trim 合法（内部空白不 trim——parse 拒）。
    assert_eq!(parse_display_mode(b"10 24x768"), None);
}
