//! W3-Q83 host 钉板：UART 整数格式化族纯面（numface）。
//!
//! 对拍锚 = HAL 原实现的字面输出形态（uart.rs/trap.rs/psci.rs 冻结镜像）；
//! 敌意短缓冲安全律；全值域扫描对拍（与 u64 参考实现商余序对拍）。

use crate::*;

/// u64 十进制参考实现（format! 对照——host 测试专属）。
fn ref_dec(v: u64) -> String {
    format!("{}", v)
}

/// u64 hex16 小写参考。
fn ref_hex64(v: u64) -> String {
    format!("{:016x}", v)
}

/// u32 hex8 大写参考。
fn ref_hex32(v: u32) -> String {
    format!("{:08X}", v)
}

#[test]
fn dec_u64_full_domain_samples() {
    // 锚点族：0/1/9/10/99/100/界值/极值——与原 uart_put_dec 逐字节对拍。
    let anchors: &[u64] = &[
        0, 1, 9, 10, 99, 100, 255, 256, 999, 1000, 65535,
        0xFFFF_FFFF, 1_000_000_000, u64::MAX - 1, u64::MAX,
    ];
    for &v in anchors {
        let mut buf = [0u8; DEC64_MAX_DIGITS];
        let n = write_dec_u64(v, &mut buf);
        assert_eq!(&buf[..n], ref_dec(v).as_bytes(), "dec({v})");
    }
}

#[test]
fn dec_u64_random_1000_cross_check() {
    // 线性同余伪随机 1000 点对拍（确定性种子）。
    let mut s = 0x243F_6A88_85A3_08D3u64;
    let mut hit = 0;
    while hit < 1000 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        for v in [s, s >> 32, s % 10_000] {
            let mut buf = [0u8; DEC64_MAX_DIGITS];
            let n = write_dec_u64(v, &mut buf);
            assert_eq!(&buf[..n], ref_dec(v).as_bytes());
            hit += 1;
        }
    }
}

#[test]
fn hex64_lower_freeze() {
    // 固定 16 位小写、高位零填充、无 0x 前缀——trap dump 形冻结。
    let anchors: &[u64] = &[0, 1, 0xA, 0xDEAD_BEEF, u64::MAX, 0x0123_4567_89AB_CDEF];
    for &v in anchors {
        let mut buf = [0u8; HEX64_DIGITS];
        let n = write_hex64_lower(v, &mut buf);
        assert_eq!(n, HEX64_DIGITS);
        assert_eq!(&buf, ref_hex64(v).as_bytes(), "hex64({v:#x})");
    }
}

#[test]
fn hex32_upper_freeze() {
    // PSCI 诊断行：8 位大写。0x8000_0000 = SMCCC 调用号形锚。
    let anchors: &[u32] = &[0, 1, 0x8000_0000, 0xFFFF_FFFF, 0x8400_0000];
    for &v in anchors {
        let mut buf = [0u8; HEX32_DIGITS];
        let n = write_hex32_upper(v, &mut buf);
        assert_eq!(n, HEX32_DIGITS);
        assert_eq!(&buf, ref_hex32(v).as_bytes(), "hex32({v:#X})");
    }
}

#[test]
fn dec2_register_indices() {
    // 寄存器下标 0..=30（消费面域）+ 边界 99/100（低两位映射）。
    let mut buf = [0u8; 2];
    for i in 0u8..=30 {
        let n = write_dec2(i, &mut buf);
        assert_eq!(n, 2);
        assert_eq!(&buf, format!("{:02}", i).as_bytes());
    }
    assert_eq!(&buf_of(dec2h(99)), b"99");
    assert_eq!(&buf_of(dec2h(100)), b"00"); // 原形恒两位（n/10=10 → %10=0）
}
fn dec2h(n: u8) -> [u8; 2] {
    let mut b = [0u8; 2];
    write_dec2(n, &mut b);
    b
}
fn buf_of(b: [u8; 2]) -> [u8; 2] {
    b
}

#[test]
fn kv_line_assembly() {
    // put_kv 形：key + hex64 + \n。
    let mut out = [0u8; 64];
    let n = write_kv_line(b"esr=", 0x8400_0000, &mut out);
    assert_eq!(&out[..n], b"esr=0000000084000000\n");
    let n = write_kv_line(b"far=", 0, &mut out);
    assert_eq!(&out[..n], b"far=0000000000000000\n");
}

#[test]
fn reg_line_assembly() {
    // put_reg 形：两空格 + x + 两位下标 + =0x + 16 位 hex + \n。
    let mut out = [0u8; 64];
    let n = write_reg_line(0, 0xDEAD_BEEF, &mut out);
    assert_eq!(&out[..n], b"  x00=0x00000000deadbeef\n");
    let n = write_reg_line(29, u64::MAX, &mut out);
    assert_eq!(&out[..n], b"  x29=0xffffffffffffffff\n");
    let n = write_reg_line(7, 1, &mut out);
    assert_eq!(&out[..n], b"  x07=0x0000000000000001\n");
}

#[test]
fn short_buffer_safety() {
    // 敌意短缓冲：零长/一位/半长——零 panic、零越界、截断可预期。
    let mut empty: [u8; 0] = [];
    assert_eq!(write_dec_u64(12345, &mut empty), 0);
    assert_eq!(write_hex64_lower(u64::MAX, &mut empty), 0);
    assert_eq!(write_dec2(42, &mut empty), 0);

    let mut one = [0u8; 1];
    assert_eq!(write_dec_u64(0, &mut one), 1);
    assert_eq!(one[0], b'0');
    assert_eq!(write_dec2(7, &mut one), 1);
    assert_eq!(one[0], b'0'); // 十位先写

    let mut half = [0u8; 8];
    assert_eq!(write_hex64_lower(0xABCD, &mut half), 8);
    assert_eq!(&half, b"00000000"); // 高 8 位（截尾=低位段不写）

    let mut two = [0u8; 2];
    assert_eq!(write_dec_u64(98765, &mut two), 2);
    assert_eq!(&two, b"98"); // 高两位
}

#[test]
fn digit_tables_frozen() {
    // 数字表冻结：与 HAL 原字面逐字节一致（漂移即红）。
    let mut b = [0u8; 16];
    write_hex64_lower(0x0123_4567_89AB_CDEF, &mut b);
    assert_eq!(&b, b"0123456789abcdef");
    let mut b8 = [0u8; 8];
    write_hex32_upper(0x0123_4567, &mut b8);
    assert_eq!(&b8, b"01234567");
    write_hex32_upper(0x89AB_CDEF, &mut b8);
    assert_eq!(&b8, b"89ABCDEF");
}

#[test]
fn dec_power_of_ten_boundaries() {
    // 10^k 恰界族：k-1 位 → k 位换档点全扫。
    let mut p = 1u64;
    for _ in 0..19 {
        for v in [p - 1, p, p + 1] {
            if p == 1 && v == 0 {
                continue;
            }
            let mut buf = [0u8; DEC64_MAX_DIGITS];
            let n = write_dec_u64(v, &mut buf);
            assert_eq!(&buf[..n], ref_dec(v).as_bytes(), "10 界 {v}");
        }
        p *= 10;
    }
}
