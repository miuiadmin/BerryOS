//! 字形面：自绘 8×16 点阵（V-6 刀 2 前置段）。
//!
//! 数据在 [`font_data::FONT`]——由 `tools/genfbfont.py` 从 art（自绘
//! 原创设计，公有领域——非任何外部字体数据的拷贝）生成，改 art 改生成器
//! 后重跑，勿手改生成物。本文件只承载 API 与结构钉板测试。
//!
//! 习惯约定（生成器 rows_of 钉板 + 下方测试双执法）：
//! 行序自上而下；**字节内 MSB（bit7）= 最左列**（与 render blit 对偶）；
//! rows 0..=2 恒空；基线 row 12；下行部（g j p q y）伸 rows 13..=14；
//! row 15 恒空；col 7（bit0）右留白——95 形全数不用。

/// 字形宽（像素）。
pub const GLYPH_W: usize = 8;
/// 字形高（像素）。
pub const GLYPH_H: usize = 16;
/// 覆盖首字符（含）：空格。
pub const FIRST: u8 = 32;
/// 覆盖末字符（含）：'~'。
pub const LAST: u8 = 126;

#[path = "font_data.rs"]
mod font_data;

/// 取字形位图（16 行、每行一字节、MSB=最左列）。
///
/// 32..=126 返 `Some`；其余（控制符/DEL/非 ASCII）返 `None`——
/// 消费侧（render）按全 bg 空格处理，宁缺勿猜。
pub fn glyph(ch: u8) -> Option<&'static [u8; GLYPH_H]> {
    (FIRST..=LAST).contains(&ch).then(|| &font_data::FONT[(ch - FIRST) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink_pixels(g: &[u8; 16]) -> usize {
        g.iter().map(|r| r.count_ones() as usize).sum()
    }

    fn ink_cols(g: &[u8; 16]) -> usize {
        let mut cols = [false; 8];
        for &r in g {
            for (i, c) in cols.iter_mut().enumerate() {
                if r & (0x80 >> i) != 0 {
                    *c = true;
                }
            }
        }
        cols.iter().filter(|&&c| c).count()
    }

    /// 覆盖域常量与表长自洽。
    #[test]
    fn coverage_bounds() {
        assert_eq!(FIRST, 32);
        assert_eq!(LAST, 126);
        assert_eq!(font_data::FONT.len(), (LAST - FIRST + 1) as usize);
        assert_eq!(GLYPH_H, 16);
        assert_eq!(GLYPH_W, 8);
    }

    /// 域内取形：索引对位 + 返回位图非退化。
    #[test]
    fn glyph_in_range() {
        assert_eq!(glyph(b'A'), Some(&font_data::FONT[33]));
        assert_eq!(glyph(b'~'), Some(&font_data::FONT[94]));
        assert!(ink_pixels(glyph(b'#').unwrap()) >= 8);
    }

    /// 域外拒面：控制符 / DEL / 高位非 ASCII 全 None（宁缺勿猜）。
    #[test]
    fn glyph_out_of_range() {
        for ch in [0u8, 9, 10, 13, 27, 31, 127, 128, 255] {
            assert!(glyph(ch).is_none(), "ch={ch}");
        }
    }

    /// 空格 = 全零位图。
    #[test]
    fn space_blank() {
        assert_eq!(glyph(b' ').unwrap(), &[0u8; 16]);
    }

    /// 结构钉板（对生成物独立执法，非转述生成器断言）：rows 0..=2 与
    /// row 15 全数恒空——蜂格基线/下行部约定的硬界。
    #[test]
    fn structural_blank_rows() {
        for (i, g) in font_data::FONT.iter().enumerate() {
            assert_eq!(g[0], 0, "ch={} rows0", i + 32);
            assert_eq!(g[1], 0, "ch={} rows1", i + 32);
            assert_eq!(g[2], 0, "ch={} rows2", i + 32);
            assert_eq!(g[15], 0, "ch={} row15", i + 32);
        }
    }

    /// col 7（bit0）右留白：95 形全数不用——设计宽 7 列约定。
    #[test]
    fn col7_reserved_blank() {
        for (i, g) in font_data::FONT.iter().enumerate() {
            for (y, &r) in g.iter().enumerate() {
                assert_eq!(r & 0x01, 0, "ch={} row{y} 用了 col7", i + 32);
            }
        }
    }

    /// 对抗审查 F2：位序对偶的**字面**钉板——期望位图独立手算，不与
    /// 生成器同源重算（镜像错排两侧同错自洽面）。'L' = 竖茎 col1
    /// rows 3..=11 + 底横 cols1..=5 row12；MSB=最左列 ⇒ 茎 = 0x40、
    /// 底横 = 0b0111_1100 = 0x7C。'I' = 顶/底横 0x70 夹竖茎 0x20。
    #[test]
    fn msb_left_literal_l_and_i() {
        let l = glyph(b'L').unwrap();
        assert_eq!(
            l,
            &[
                0x00, 0x00, 0x00, // rows 0..=2 恒空
                0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, // 茎 rows 3..=11
                0x7C, // 底横 row 12（基线）
                0x00, 0x00, 0x00,
            ]
        );
        let i = glyph(b'I').unwrap();
        assert_eq!(
            i,
            &[
                0x00, 0x00, 0x00, 0x70, // 顶横 row 3（cols2..=4）
                0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, // 茎 rows 4..=11
                0x70, // 底横 row 12
                0x00, 0x00, 0x00,
            ]
        );
    }

    /// 基线锚：大写盒形字（L/M/E）在 row 12 有着墨——蜂格行距约定。
    #[test]
    fn baseline_row12_anchors() {
        for ch in [b'L', b'M', b'E', b'Z'] {
            let g = glyph(ch).unwrap();
            assert_ne!(g[12], 0, "{} 的基线 row 12 空", ch as char);
        }
    }

    /// 引号类上区钉板：'"' 着墨仅 rows 3..=5（上升部顶界以下、x 高以上）。
    #[test]
    fn quote_ink_upper_band_only() {
        let g = glyph(b'"').unwrap();
        for (y, &r) in g.iter().enumerate() {
            let expect = (3..=5).contains(&y) && r != 0 || !(3..=5).contains(&y) && r == 0;
            assert!(expect, "row{y}={r:#04x}");
        }
    }

    /// 下行部钉板：g/j/p/q/y 伸 rows 13..=14；对照组 'n'/'u' 不越 row 12。
    #[test]
    fn descenders_reach_rows13_14() {
        for ch in [b'g', b'j', b'p', b'q', b'y'] {
            let g = glyph(ch).unwrap();
            assert!(g[13] != 0 || g[14] != 0, "{} 无下行部", ch as char);
            assert_eq!(g[15], 0);
        }
        for ch in [b'n', b'u', b'o', b'A'] {
            let g = glyph(ch).unwrap();
            assert_eq!(&g[13..], &[0u8, 0, 0], "{} 越过基线下沿", ch as char);
        }
    }

    /// 密度下界：**字母数字**字形 ≥8 着墨像素——防丢形/空形静默入表
    /// （标点不在此列：句号 1 墨点/引号 6 墨点是合法稀疏面）。
    #[test]
    fn density_floor() {
        for ch in FIRST..=LAST {
            if !ch.is_ascii_alphanumeric() {
                continue;
            }
            let g = glyph(ch).unwrap();
            assert!(ink_pixels(g) >= 8, "'{}' 仅 {} 墨点", ch as char, ink_pixels(g));
        }
    }

    /// 宽度谱系：'M' 用 ≥6 着墨列（宽字母面）；'I' ≤3 列（窄字母面）。
    #[test]
    fn width_spectrum_m_wide_i_narrow() {
        assert!(ink_cols(glyph(b'M').unwrap()) >= 6, "M 应为最宽族");
        assert!(ink_cols(glyph(b'W').unwrap()) >= 5, "W 宽族");
        assert!(ink_cols(glyph(b'I').unwrap()) <= 3, "I 应为最窄族");
        assert!(ink_cols(glyph(b'l').unwrap()) <= 2, "l 窄");
        assert!(ink_cols(glyph(b'1').unwrap()) <= 3, "1 窄");
    }

    /// x 高谱系：小写无上升部字母（a/c/e/n/o）着墨不越 row 12 且不从
    /// row 6 之前起笔（rows 3..=5 空）；上升部 'b' 从 row 3 起笔。
    #[test]
    fn x_height_bands() {
        for ch in [b'a', b'c', b'e', b'n', b'o', b's', b'v', b'x', b'z'] {
            let g = glyph(ch).unwrap();
            assert_eq!(&g[3..6], &[0, 0, 0], "'{}' x 高顶越界", ch as char);
        }
        let b = glyph(b'b').unwrap();
        assert!(b[3] != 0, "'b' 上升部应自 row 3 起笔");
        let t = glyph(b't').unwrap();
        assert!(t[4] != 0, "'t' 半上升部应早于 x 高起笔");
    }
}
