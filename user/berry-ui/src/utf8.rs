//! UTF-8 等宽消费面（桌面中文标签首刀）。
//!
//! berry-ui 的 wire 文本仍是原始字节（`TextBuf` 64B 帽不变）；本模块只把
//! 有效字节解码为 Unicode 码点供字形 run / intrinsic 计数使用。非法序列
//! 折成 U+FFFD 且前进 1 字节——渲染为缺字形空格，测量与绘制保持同一
//! 计数，不 panic、不猜测成其他 glyph。

/// U+FFFD（replacement character）：非法 UTF-8 的统一缺字形码点。
pub const REPLACEMENT: u32 = 0xFFFD;

/// 解码 `src` 首个码点，返回 `(码点, 消耗字节数)`；空切片返回 `None`。
pub fn next(src: &[u8]) -> Option<(u32, usize)> {
    let &b0 = src.first()?;
    let cont = |i: usize| src.get(i).copied().filter(|b| b & 0xC0 == 0x80);
    let cp = match b0 {
        0x00..=0x7F => return Some((u32::from(b0), 1)),
        0xC2..=0xDF => {
            let Some(b1) = cont(1) else {
                return Some((REPLACEMENT, 1));
            };
            return Some(((u32::from(b0 & 0x1F) << 6) | u32::from(b1 & 0x3F), 2));
        }
        0xE0..=0xEF => {
            let Some(b1) = cont(1) else {
                return Some((REPLACEMENT, 1));
            };
            let Some(b2) = cont(2) else {
                return Some((REPLACEMENT, 1));
            };
            let cp = (u32::from(b0 & 0x0F) << 12)
                | (u32::from(b1 & 0x3F) << 6)
                | u32::from(b2 & 0x3F);
            // 拒绝过短 / 代理区（UTF-8 标准编码面）。
            if cp < 0x800 || (0xD800..=0xDFFF).contains(&cp) {
                return Some((REPLACEMENT, 1));
            }
            return Some((cp, 3));
        }
        0xF0..=0xF4 => {
            let Some(b1) = cont(1) else {
                return Some((REPLACEMENT, 1));
            };
            let Some(b2) = cont(2) else {
                return Some((REPLACEMENT, 1));
            };
            let Some(b3) = cont(3) else {
                return Some((REPLACEMENT, 1));
            };
            let cp = (u32::from(b0 & 0x07) << 18)
                | (u32::from(b1 & 0x3F) << 12)
                | (u32::from(b2 & 0x3F) << 6)
                | u32::from(b3 & 0x3F);
            if !(0x10000..=0x10FFFF).contains(&cp) {
                return Some((REPLACEMENT, 1));
            }
            cp
        }
        _ => return Some((REPLACEMENT, 1)),
    };
    Some((cp, 4))
}

/// 码点计数（非法单字节也计 1——与 [`next`] 的测量/绘制语义同源）。
pub fn count(src: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < src.len() {
        let step = next(&src[i..]).map(|(_, n)| n).unwrap_or(1);
        i += step.max(1);
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_cjk_and_boundaries() {
        assert_eq!(next(b"A"), Some((0x41, 1)));
        assert_eq!(next("设".as_bytes()), Some((0x8BBE, 3)));
        assert_eq!(next("浏览器".as_bytes()), Some((0x6D4F, 3)));
        assert_eq!(next("\u{10FFFF}".as_bytes()).map(|(c, n)| (c, n)), Some((0x10FFFF, 4)));
        assert_eq!(count("Browser设置".as_bytes()), 9);
    }

    #[test]
    fn malformed_input_is_one_replacement_step() {
        for src in [&[0xFF][..], &[0xE0, 0x80, 0x80][..], &[0xED, 0xA0, 0x80][..], &[0xF4, 0x90][..]] {
            assert_eq!(next(src), Some((REPLACEMENT, 1)));
        }
        assert_eq!(count(&[0xC3, 0x28]), 2);
    }
}
