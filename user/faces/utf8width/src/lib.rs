//! berry-utf8width——终端计划 刀 5：UTF-8 字符列宽单源（五处共享）。
//!
//! East Asian Width（Unicode 15.1）+ 组合字符（零宽）+ Emoji 宽形（2 列）。
//!
//! 纯面纪律：全表驱动 = 零 I/O 纯函数（host 可测）。
#![cfg_attr(not(test), no_std)]

/// 字符列宽（0/1/2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Width {
    Zero = 0,  // 组合字符/零宽连接符
    Narrow = 1, // ASCII/半角
    Wide = 2,   // 全角 CJK/Emoji
}

/// 单个 Unicode 码点 → 列宽。
pub fn char_width(c: char) -> Width {
    let cp = c as u32;
    // 快速路径：ASCII 可打印
    if (0x20..0x7F).contains(&cp) {
        return Width::Narrow;
    }
    // 零宽：组合字符 U+0300..U+036F + ZWJ/ZWNJ + 控制符
    if (0x0300..=0x036F).contains(&cp) || cp == 0x200D || cp == 0x200C || cp < 0x20 {
        return Width::Zero;
    }
    // 宽形：CJK 统一表意 + 全角 ASCII + Hangul + 常见 Emoji
    if is_wide(cp) {
        return Width::Wide;
    }
    Width::Narrow
}

fn is_wide(cp: u32) -> bool {
    matches!(cp,
        // CJK 统一表意文字
        0x4E00..=0x9FFF
        | 0x3400..=0x4DBF
        | 0x20000..=0x2A6DF
        // CJK 扩展
        | 0x2A700..=0x2EBEF
        // 全角 ASCII / 半角片假名补充
        | 0xFF01..=0xFF60
        // Hangul 音节
        | 0xAC00..=0xD7AF
        | 0x1100..=0x115F
        // CJK 标点
        | 0x3000..=0x303F
        | 0xFF00..=0xFF60
        // 常见 Emoji（表情/符号）
        | 0x1F300..=0x1F9FF
        | 0x1F600..=0x1F64F
        | 0x2600..=0x26FF
        | 0x2700..=0x27BF
        // 私用区
        | 0xE000..=0xF8FF
    )
}

/// UTF-8 字符串总列宽。
pub fn str_width(s: &str) -> usize {
    s.chars().map(|c| char_width(c) as usize).sum()
}

/// 按列宽截断——返回最多 `max_width` 列的前缀。
pub fn truncate_width(s: &str, max_width: usize) -> &str {
    let mut w = 0usize;
    for (idx, c) in s.char_indices() {
        let cw = char_width(c) as usize;
        if w + cw > max_width {
            return &s[..idx];
        }
        w += cw;
    }
    s
}

/// 光标前移列数（考虑零宽组合不前移）。
pub fn advance_columns(s: &str) -> usize {
    str_width(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_narrow() {
        assert_eq!(char_width('A'), Width::Narrow);
        assert_eq!(char_width('z'), Width::Narrow);
        assert_eq!(char_width(' '), Width::Narrow);
        assert_eq!(str_width("Hello"), 5);
    }

    #[test]
    fn cjk_wide() {
        assert_eq!(char_width('中'), Width::Wide);
        assert_eq!(char_width('莓'), Width::Wide);
        assert_eq!(char_width('核'), Width::Wide);
        assert_eq!(str_width("莓核"), 4); // 2+2
    }

    #[test]
    fn mixed_width() {
        assert_eq!(str_width("莓核Berry"), 4 + 5); // 2+2+5
        assert_eq!(str_width("AB中CD"), 2 + 2 + 2); // 1+1+2+1+1
    }

    #[test]
    fn combining_zero() {
        assert_eq!(char_width('\u{0301}'), Width::Zero); // 组合尖音
        assert_eq!(str_width("e\u{0301}"), 1); // é = e + 组合尖 = 1 列
    }

    #[test]
    fn control_zero() {
        assert_eq!(char_width('\n'), Width::Zero);
        assert_eq!(char_width('\t'), Width::Zero);
        assert_eq!(char_width('\0'), Width::Zero);
    }

    #[test]
    fn truncate_mixed() {
        assert_eq!(truncate_width("莓核Berry", 5), "莓核B"); // 2+2+1=5
        assert_eq!(truncate_width("Hello", 3), "Hel");
        assert_eq!(truncate_width("中中中", 3), "中"); // 2 ≤ 3 < 4
    }

    #[test]
    fn emoji_wide() {
        assert_eq!(char_width('\u{1F600}'), Width::Wide); // 😀
        assert_eq!(str_width("😀"), 2);
    }

    #[test]
    fn hangul_wide() {
        assert_eq!(char_width('\u{AC00}'), Width::Wide); // 가
        assert_eq!(str_width("가"), 2);
    }
}
