//! UART 整数格式化族纯面（W3-Q83 单源 crate）。
//!
//! 收口对象：内核四处散落的「整数 → ASCII 字节串」装配律——
//! - HAL `uart.rs::uart_put_dec`（u64 十进制，无符号无前导）；
//! - HAL `uart.rs::uart_put_hex64`（u64 十六进制，固定 16 位小写，无 0x）；
//! - HAL `trap.rs::dec2`（两位十进制——寄存器下标 0..30）；
//! - HAL `psci.rs::put_hex32_upper`（u32 八位大写 hex——PSCI 诊断行专用）；
//! - HAL `trap.rs::put_kv`/`put_reg`（键值行/寄存器行装配序）。
//!
//! 本 crate 只做**字节装配**（写入调用方给的缓冲/逐字节回调），不触
//! UART——发射面（`uart_putc`/`uart_puts` MMIO 臂）与热路径直写消费面
//! （boot/panic/ktest）零扰动。判据与位序逐字节对齐原实现（freeze 锚
//! 在 host 测试钉板）。
//!
//! 形态纪律：同 berry-balface/berry-bandface——纯函数零依赖、
//! `#![cfg_attr(not(test), no_std)]` 双面、HAL 消费壳只留装配调用与
//! 发射循环。

#![cfg_attr(not(test), no_std)]

/// 十六进制小写数字表（`uart_put_hex64` 冻结镜像）。
const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";
/// 十六进制大写数字表（`put_hex32_upper` 冻结镜像）。
const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";
/// 十进制数字表（`dec2` 冻结镜像）。
const DEC: &[u8; 10] = b"0123456789";

/// u64 十进制最大位数（u64::MAX = 20 位）。
pub const DEC64_MAX_DIGITS: usize = 20;
/// u64 hex 固定位数（16 nibble）。
pub const HEX64_DIGITS: usize = 16;
/// u32 hex 固定位数（8 nibble）。
pub const HEX32_DIGITS: usize = 8;

/// u64 → 十进制 ASCII（无符号、无前导零；`0` → `"0"`）。
///
/// 返回写入 `out` 的字节数（`out` 至少 [`DEC64_MAX_DIGITS`] 长——短缓冲
/// 恒安全：只写 `min(need, out.len())`，need 溢出截高位前段由调用方
/// 缓冲合同排除）。装配序 = 低位先生再反转（原实现逐位余除律镜像）。
pub fn write_dec_u64(v: u64, out: &mut [u8]) -> usize {
    if v == 0 {
        if out.is_empty() {
            return 0;
        }
        out[0] = b'0';
        return 1;
    }
    let mut buf = [0u8; DEC64_MAX_DIGITS];
    let mut n = v;
    let mut i = 0usize;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    // 反转拷入 out（受 out 长度约束——合同下 i ≤ out.len() 恒真）。
    let take = i.min(out.len());
    for k in 0..take {
        out[k] = buf[i - 1 - k];
    }
    take
}

/// u64 → 固定 16 位小写 hex（无 `0x` 前缀；高位零填充——trap dump 形）。
///
/// 返回写入字节数（恒 [`HEX64_DIGITS`]，`out` 短于 16 时写 `out.len()`）。
pub fn write_hex64_lower(v: u64, out: &mut [u8]) -> usize {
    let take = HEX64_DIGITS.min(out.len());
    for i in 0..take {
        let shift = (15 - i) * 4;
        out[i] = HEX_LOWER[((v >> shift) & 0xf) as usize];
    }
    take
}

/// u32 → 固定 8 位大写 hex（PSCI 诊断行形态；无前缀）。
///
/// 返回写入字节数（恒 [`HEX32_DIGITS`]，短缓冲截尾同理）。
pub fn write_hex32_upper(v: u32, out: &mut [u8]) -> usize {
    let take = HEX32_DIGITS.min(out.len());
    for i in 0..take {
        let shift = (take - 1 - i) * 4;
        out[i] = HEX_UPPER[((v >> shift) & 0xf) as usize];
    }
    take
}

/// 两位十进制（寄存器下标 0..=99；n>99 只映低两位——原 `dec2` 恒两位
/// 形，无截断语义差异：消费面寄存器下标 ≤30）。
pub fn write_dec2(n: u8, out: &mut [u8]) -> usize {
    if out.is_empty() {
        return 0;
    }
    out[0] = DEC[(n / 10) as usize % 10];
    if out.len() == 1 {
        return 1;
    }
    out[1] = DEC[(n % 10) as usize];
    2
}

/// 键值行装配（trap dump `put_kv` 形）：`key + hex64(v) + '\n'` 单调序。
///
/// 返回总写入字节数；`out` 合同 ≥ `key.len() + HEX64_DIGITS + 1`。
pub fn write_kv_line(key: &[u8], v: u64, out: &mut [u8]) -> usize {
    let mut n = append(key, out, 0);
    n += write_hex64_lower(v, &mut out[n..]);
    n += append(b"\n", &mut out[n..], 0);
    n
}

/// 寄存器行装配（trap dump `put_reg` 形）：`"  x" + dec2(i) + "=0x" +
/// hex64(v) + '\n'`。
pub fn write_reg_line(i: usize, v: u64, out: &mut [u8]) -> usize {
    let mut n = append(b"  x", out, 0);
    n += write_dec2(i as u8, &mut out[n..]);
    n += append(b"=0x", &mut out[n..], 0);
    n += write_hex64_lower(v, &mut out[n..]);
    n += append(b"\n", &mut out[n..], 0);
    n
}

/// 区段拷贝辅助（src 截到 out 余量；返回实写数）。
fn append(src: &[u8], out: &mut [u8], _off: usize) -> usize {
    let take = src.len().min(out.len());
    out[..take].copy_from_slice(&src[..take]);
    take
}

// ---- 常量冻结锚（与 HAL 原字面逐字节一致——漂移即红）----

#[cfg(test)]
mod tests;
