//! 文字三段解耦（六件套 ⑥）v1 最小面：shape（v1 = codepoint 直映射
//! 点阵索引，无整形引擎）/ 图集（[`GlyphAtlas`] trait）/ pass 内绘制
//! （`cmd::Cmd::DrawGlyphRun`）。矢量字体/HarfBuzz 面零涉及——规划只
//! 钦定点阵多字号（段② genfbfont 扩档）。
//!
//! 字形位图契约与 berry-fbcon 逐位同：16 行 × 每行一字节、
//! **MSB = 最左列**（`bits & (0x80 >> gx)`）；缺字形返 `None`（消费侧
//! 按全零位图 = 空格语义处理，宁缺勿猜）。

/// 字形图集：codepoint → 8×16 位图（16 行 × 每行一字节、MSB = 最左列
/// ——与 berry-fbcon 对偶契约）。v1 唯一实现 = [`FontAtlas8x16`]；
/// 段② 多字号/图标 LRU 图集经本 trait 扩（不动命令面）。
pub trait GlyphAtlas {
    /// 取字形位图。域外/缺字形返 `None`。
    fn glyph(&self, ch: u8) -> Option<&[u8; 16]>;
}

/// 字号档位占位（v1 单一 8×16 档；段② genfbfont 多字号扩档时升格为
/// 图集属性/参数——此刻只是概念锚，不进命令面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSize {
    /// 8×16 点阵（fbcon 单一事实源同款）。
    S8x16,
}

/// v1 唯一图集实现：包 berry-fbcon `font::FONT`（**单一事实源**——
/// 共识测试两侧同源字体；`tools/genfbfont.py` 生成勿手改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontAtlas8x16;

impl FontAtlas8x16 {
    /// 本图集字号档（恒 [`FontSize::S8x16`]）。
    pub fn size(&self) -> FontSize {
        FontSize::S8x16
    }
}

impl GlyphAtlas for FontAtlas8x16 {
    fn glyph(&self, ch: u8) -> Option<&[u8; 16]> {
        berry_fbcon::font::glyph(ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— 单一事实源钉板：与 berry-fbcon font::glyph 逐字节同引用 ——
    #[test]
    fn atlas8x16_single_source() {
        let a = FontAtlas8x16;
        assert_eq!(a.size(), FontSize::S8x16);
        // 域内：同一张表（指针级同源——共识契约的字体面）。
        assert_eq!(a.glyph(b'#'), berry_fbcon::font::glyph(b'#'));
        assert_eq!(a.glyph(b' '), Some(&[0u8; 16]), "空格 = 全零位图");
        assert!(a.glyph(b'M').is_some());
        // 域外：控制符/DEL/高位字节全 None（宁缺勿猜）。
        for ch in [0u8, 9, 10, 27, 31, 127, 128, 0xFF] {
            assert_eq!(a.glyph(ch), None, "ch={ch}");
        }
    }
}
