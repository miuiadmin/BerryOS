//! 文字三段解耦（六件套 ⑥）段② 多字号扩档：shape（v1 = codepoint
//! 直映射点阵索引，无整形引擎）/ 图集（[`GlyphAtlas`] trait 三档实现）/
//! pass 内绘制（`cmd::Cmd::DrawGlyphRun`）。矢量字体/HarfBuzz 面零涉及
//! ——规划只钦定点阵多字号（genfbfont.py 三档产链）。
//!
//! 字形位图契约与 berry-fbcon 逐位同：**MSB = 最左列**
//! （`bits & (0x80 >> gx)`）、行主序、每行 `(w+7)/8` 字节；缺字形返
//! `None`（消费侧按全零位图 = 空格语义处理，宁缺勿猜）。
//!
//! 段② 起 [`GlyphBits`] 从「8×16 专属」升格为**任意几何单色位图视图**
//! ——同时是图标/图像位（`cmd::ImageSrc::Bits`）与字形位的共同载体
//! （一条位法、两处消费），尺寸三档：
//!
//! | 档 | 字格 | 数据源 | 产链 |
//! |----|------|--------|------|
//! | 1x | 8×16 | berry-fbcon `font::FONT` | `tools/genfbfont.py`（fbcon 单一事实源） |
//! | 2x | 16×32 | [`font_data_s2::FONT_S2`] | 同生成器 NN 放大档 |
//! | 3x | 24×48 | [`font_data_s3::FONT_S3`] | 同生成器 NN 放大档 |
//!
//! 契约名注记：规划草案的 `GlyphBitmap` 落地为 [`GlyphBits`]（「位图
//! 视图」语义更准——同一位法三处消费：字形/图标/图像，不专属 glyph，
//! 名字不撒谎）；草案的 `cell()` 几何口以 [`GlyphCell`] 组合形保留
//! （`cell_w`/`cell_h` 拆分口为主面——单值消费点省结构解构）。
//!
//! 缓存裁剪注记：三档皆**静态表直索引**（O(1) 数组下标、全表常驻 ~
//! 1.5KB/档），无运行时字形缓存面——规划项「缓存」实指图标侧 blob
//! 解码产物的 [`crate::lru`]（静态字形表直索引零成本，缓存它没有价值
//! 面；「砍」的是不存在的成本，不是砍了功能）。

#[path = "font_data_s2.rs"]
mod font_data_s2;
#[path = "font_data_s3.rs"]
mod font_data_s3;
#[path = "font_data_hd.rs"]
mod font_data_hd;

/// 单色位图视图（字形/图标/图像共用——见模块头）。字段 pub 供命令
/// 构造直排，**权威构造口 = [`GlyphBits::new`]（恰长铁律 + 坐标域先验）**；
/// 字面构造绕过校验的，图像面由 `Queue::submit` 期 `ImagePixelMismatch`
/// 硬门兜底（见 queue.rs），字形面经 [`GlyphBits::bit`] 防御采样永不
/// panic（短行 = 越界位恒 false）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphBits<'a> {
    /// 像素宽。
    pub w: usize,
    /// 像素高。
    pub h: usize,
    /// 位图行主序（每行 [`GlyphBits::row_bytes`] 字节、MSB = 最左列）。
    pub rows: &'a [u8],
}

/// 单色位图拒面（构造口 loud——畸形位图绝不带病进光栅核）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitsError {
    /// `rows.len() != h * row_bytes`（恰长铁律——短行 = 越界位静默
    /// 错位、长行 = 尾字节静默丢弃，两者都禁）。
    PixelCountMismatch { expect: usize, got: usize },
    /// w/h 超 i32 坐标域上界（与 `color::DescError::TooLarge` 同口径——
    /// 光栅核 i32 域的先验执法点）。
    TooLarge,
}

impl<'a> GlyphBits<'a> {
    /// 权威构造口：恰长 + 坐标域双先验。零宽/零高 = 合法空位图
    /// （`rows` 须为空切片——绘制 no-op）。
    pub fn new(w: usize, h: usize, rows: &'a [u8]) -> Result<GlyphBits<'a>, BitsError> {
        if w > i32::MAX as usize || h > i32::MAX as usize {
            return Err(BitsError::TooLarge);
        }
        let expect = row_bytes_of(w).checked_mul(h).ok_or(BitsError::TooLarge)?;
        if rows.len() != expect {
            return Err(BitsError::PixelCountMismatch { expect, got: rows.len() });
        }
        Ok(GlyphBits { w, h, rows })
    }

    /// 每行字节数（`(w+7)/8`——加法形态防 `w == usize::MAX` 回绕）。
    pub const fn row_bytes(&self) -> usize {
        row_bytes_of(self.w)
    }

    /// 防御采样：`(x,y)` 位是否着墨。越界（x≥w / y≥h / 行字节缺失/
    /// 寻址乘加溢出）恒 `false`——绝不 panic、绝不读越界（敌意几何的
    /// 安全网——Icon 面免构造口先验，巨 w 的 `y*row_bytes` 可越 usize
    /// 域；正常数据界内由构造口/生成器钉死）。
    pub fn bit(&self, x: usize, y: usize) -> bool {
        if x >= self.w || y >= self.h {
            return false;
        }
        let rb = self.row_bytes();
        // checked 乘加：域界巨参（w 近 usize::MAX）在此判 false 而非
        // 溢出 panic——防御采样的「绝不 panic」承诺覆盖这里。
        let Some(idx) = y.checked_mul(rb).and_then(|a| a.checked_add(x / 8)) else {
            return false;
        };
        match self.rows.get(idx) {
            Some(&b) => b & (0x80 >> (x % 8)) != 0, // MSB = 最左列（fbcon 对偶）
            None => false,
        }
    }
}

/// 每行字节数纯函数（`(w+7)/8` 的加法形态——`w/8 + 尾位进位`，防巨 w 回绕）。
const fn row_bytes_of(w: usize) -> usize {
    w / 8 + if w % 8 == 0 { 0 } else { 1 }
}

/// 字格几何对（契约名 `cell()` 的载体——`cell_w`/`cell_h` 拆分口之上
/// 的成对取用面：一次调用拿全几何，宽高失配（拿了 A 档的宽配 B 档的
/// 高）在类型面结构性不可发生）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphCell {
    /// 字格像素宽。
    pub w: usize,
    /// 字格像素高。
    pub h: usize,
}

/// 字形图集：codepoint → 单色位图（几何随实现档位——见模块头三档表）。
/// v1 三个静态实现（[`FontAtlas8x16`]/[`FontAtlas16x32`]/[`FontAtlas24x48`]）；
/// 图标 LRU 图集走 `icon::IconAtlas` → `cmd::ImageSrc::Icon`（不混入
/// 字形面——字形 = 等宽蜂格语义、图标 = 任意几何图像语义）。
pub trait GlyphAtlas {
    /// 字格像素宽。
    fn cell_w(&self) -> usize;
    /// 字格像素高。
    fn cell_h(&self) -> usize;
    /// 字格几何成对口（契约名保留——缺省实现组合两拆分口，三档静态
    /// 图集与自定义实现零改动获得）。
    fn cell(&self) -> GlyphCell {
        GlyphCell { w: self.cell_w(), h: self.cell_h() }
    }
    /// 单字形水平推进量（像素）——排布面逐字形累计的载荷源。缺省 =
    /// `cell_w()`：三档点阵（genfbfont 自绘等宽 art）的等宽语义**逐字节
    /// 零漂移**（现役消费点不覆写即行为不变）；比例字体档（HD）覆写读
    /// 烘焙 advance 表（`font_data_hd::ADVANCE_HD`，痛点⑥根修 03 号
    /// §1.3-2）。位图几何不受影响——字形仍居中在字格内，只改推进量。
    /// 域外字节（glyph 侧 `None`）同样返 `cell_w()`（等宽缺省——空格
    /// 语义的推进与三档一致）。
    fn advance(&self, _ch: u8) -> u32 {
        self.cell_w() as u32
    }
    /// 取字形位图。域外/缺字形返 `None`。
    fn glyph(&self, ch: u8) -> Option<GlyphBits<'_>>;
}

/// 字号档位（段② genfbfont 三档；几何与档位一一对应——`atlas()` 直取
/// 静态实现，缺字形跨档同律 `None`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSize {
    /// 8×16 点阵（fbcon 单一事实源同款）。
    S8x16,
    /// 16×32（1x 整数最近邻放大档）。
    S16x32,
    /// 24×48（1x 整数最近邻放大档）。
    S24x48,
    /// 32×48 HD（Noto Sans OFL 原生光栅化档——非 8×16 放大）。
    S32x48,
}

impl FontSize {
    /// 字格像素宽。
    pub const fn cell_w(self) -> usize {
        match self {
            FontSize::S8x16 => 8,
            FontSize::S16x32 => 16,
            FontSize::S24x48 => 24,
            FontSize::S32x48 => 32,
        }
    }

    /// 字格像素高。
    pub const fn cell_h(self) -> usize {
        match self {
            FontSize::S8x16 => 16,
            FontSize::S16x32 => 32,
            FontSize::S24x48 => 48,
            FontSize::S32x48 => 48,
        }
    }

    /// 字格几何成对口（与 [`GlyphAtlas::cell`] 同形——档位面与图集面
    /// 的几何口径单一）。
    pub const fn cell(self) -> GlyphCell {
        GlyphCell { w: self.cell_w(), h: self.cell_h() }
    }

    /// 档位对应静态图集。
    pub fn atlas(self) -> &'static dyn GlyphAtlas {
        match self {
            FontSize::S8x16 => &FontAtlas8x16,
            FontSize::S16x32 => &FontAtlas16x32,
            FontSize::S24x48 => &FontAtlas24x48,
            FontSize::S32x48 => &FontAtlas32x48,
        }
    }
}

/// 1x 图集实现：包 berry-fbcon `font::FONT`（**单一事实源**——共识测试
/// 两侧同源字体；`tools/genfbfont.py` 生成勿手改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontAtlas8x16;

impl FontAtlas8x16 {
    /// 本图集字号档（恒 [`FontSize::S8x16`]）。
    pub fn size(&self) -> FontSize {
        FontSize::S8x16
    }
}

impl GlyphAtlas for FontAtlas8x16 {
    fn cell_w(&self) -> usize {
        8
    }
    fn cell_h(&self) -> usize {
        16
    }
    fn glyph(&self, ch: u8) -> Option<GlyphBits<'static>> {
        // fbcon 表本就是 16 行 × 1 字节/行——直借为 GlyphBits 视图。
        let g = berry_fbcon::font::glyph(ch)?;
        Some(GlyphBits { w: 8, h: 16, rows: g.as_slice() })
    }
}

/// 2x 图集实现：`font_data_s2::FONT_S2`（genfbfont NN 放大档，生成勿手改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontAtlas16x32;

impl FontAtlas16x32 {
    /// 本图集字号档（恒 [`FontSize::S16x32`]）。
    pub fn size(&self) -> FontSize {
        FontSize::S16x32
    }
}

impl GlyphAtlas for FontAtlas16x32 {
    fn cell_w(&self) -> usize {
        font_data_s2::CELL_W
    }
    fn cell_h(&self) -> usize {
        font_data_s2::CELL_H
    }
    fn glyph(&self, ch: u8) -> Option<GlyphBits<'static>> {
        let i = usize::from(ch).checked_sub(32)?;
        let g = font_data_s2::FONT_S2.get(i)?;
        Some(GlyphBits {
            w: font_data_s2::CELL_W,
            h: font_data_s2::CELL_H,
            rows: g.as_slice(),
        })
    }
}

/// 3x 图集实现：`font_data_s3::FONT_S3`（genfbfont NN 放大档，生成勿手改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontAtlas24x48;

impl FontAtlas24x48 {
    /// 本图集字号档（恒 [`FontSize::S24x48`]）。
    pub fn size(&self) -> FontSize {
        FontSize::S24x48
    }
}

impl GlyphAtlas for FontAtlas24x48 {
    fn cell_w(&self) -> usize {
        font_data_s3::CELL_W
    }
    fn cell_h(&self) -> usize {
        font_data_s3::CELL_H
    }
    fn glyph(&self, ch: u8) -> Option<GlyphBits<'static>> {
        let i = usize::from(ch).checked_sub(32)?;
        let g = font_data_s3::FONT_S3.get(i)?;
        Some(GlyphBits {
            w: font_data_s3::CELL_W,
            h: font_data_s3::CELL_H,
            rows: g.as_slice(),
        })
    }
}

/// 32×48 HD 图集：Noto Sans OFL 原生光栅化（`font_data_hd::FONT_HD`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontAtlas32x48;

impl FontAtlas32x48 {
    /// 本图集字号档（恒 [`FontSize::S32x48`]）。
    pub fn size(&self) -> FontSize {
        FontSize::S32x48
    }
}

impl GlyphAtlas for FontAtlas32x48 {
    fn cell_w(&self) -> usize {
        font_data_hd::CELL_W
    }
    fn cell_h(&self) -> usize {
        font_data_hd::CELL_H
    }
    /// 比例推进：读 `ADVANCE_HD` 烘焙表（Noto Sans CJK 真实 advance；
    /// 「i」9px vs 字格 32px——等宽占格观感劣化的根修，03 号 §1.1）。
    /// 域外回退 `cell_w()`（trait 缺省同语义——与 glyph None 空格语义
    /// 成对：位图缺位画不出，推进不放大）。
    fn advance(&self, ch: u8) -> u32 {
        usize::from(ch)
            .checked_sub(32)
            .and_then(|i| font_data_hd::ADVANCE_HD.get(i))
            .map_or(font_data_hd::CELL_W as u32, |&a| u32::from(a))
    }
    fn glyph(&self, ch: u8) -> Option<GlyphBits<'static>> {
        let i = usize::from(ch).checked_sub(32)?;
        let g = font_data_hd::FONT_HD.get(i)?;
        Some(GlyphBits {
            w: font_data_hd::CELL_W,
            h: font_data_hd::CELL_H,
            rows: g.as_slice(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— 单一事实源钉板：与 berry-fbcon font::glyph 同表同几何 ——
    #[test]
    fn atlas8x16_single_source() {
        let a = FontAtlas8x16;
        assert_eq!(a.size(), FontSize::S8x16);
        assert_eq!((a.cell_w(), a.cell_h()), (8, 16));
        // 域内：同一张表（行字节逐字节同源 + 几何钉板——共识契约的字体面）。
        let g = a.glyph(b'#').unwrap();
        assert_eq!((g.w, g.h), (8, 16));
        assert_eq!(g.rows, berry_fbcon::font::glyph(b'#').unwrap().as_slice());
        let sp = a.glyph(b' ').unwrap();
        assert_eq!((sp.w, sp.h), (8, 16));
        assert_eq!(sp.rows, &[0u8; 16], "空格 = 全零位图");
        assert!(a.glyph(b'M').is_some());
        // 域外：控制符/DEL/高位字节全 None（宁缺勿猜）。
        for ch in [0u8, 9, 10, 27, 31, 127, 128, 0xFF] {
            assert!(a.glyph(ch).is_none(), "ch={ch}");
        }
    }

    // —— 放大档图集：几何/恰长/域内域外三面钉板 ——
    #[test]
    fn scaled_atlases_geometry_and_domain() {
        let a2 = FontAtlas16x32;
        assert_eq!(a2.size(), FontSize::S16x32);
        assert_eq!((a2.cell_w(), a2.cell_h()), (16, 32));
        let a3 = FontAtlas24x48;
        assert_eq!(a3.size(), FontSize::S24x48);
        let ah = FontAtlas32x48;
        assert_eq!(ah.size(), FontSize::S32x48);
        assert_eq!((ah.cell_w(), ah.cell_h()), (32, 48));
        assert!(ah.glyph(b'B').is_some());
        assert_eq!((a3.cell_w(), a3.cell_h()), (24, 48));
        // 全 95 形恰长（32 行 × 2B / 48 行 × 3B）+ bit() 行末越界恒 false。
        for ch in 32u8..=126 {
            let g2 = a2.glyph(ch).unwrap();
            assert_eq!(g2.rows.len(), 32 * 2, "s2 ch={ch}");
            assert!(!g2.bit(16, 0) && !g2.bit(0, 32), "s2 域外恒 false ch={ch}");
            let g3 = a3.glyph(ch).unwrap();
            assert_eq!(g3.rows.len(), 48 * 3, "s3 ch={ch}");
            assert!(!g3.bit(24, 0) && !g3.bit(0, 48), "s3 域外恒 false ch={ch}");
        }
        // 域外与 1x 同律（跨档同域）。
        for ch in [0u8, 31, 127, 0x80, 0xFF] {
            assert!(a2.glyph(ch).is_none(), "s2 ch={ch:#x}");
            assert!(a3.glyph(ch).is_none(), "s3 ch={ch:#x}");
        }
        // 字面手算锚：'#'（1x row3 = "..#..#.."）→ 2x 行 6 两字节
        // 0x0C/0x30（源列 2/5 → 输出列对 4-5 / 10-11）。
        let g = a2.glyph(b'#').unwrap();
        assert_eq!((g.rows[12], g.rows[13]), (0x0C, 0x30), "s2 '#' 行 6 NN 手算锚");
        // FontSize 档位几何/图集直取（三档同源）。
        for (size, w, h) in [
            (FontSize::S8x16, 8, 16),
            (FontSize::S16x32, 16, 32),
            (FontSize::S24x48, 24, 48),
            (FontSize::S32x48, 32, 48),
        ] {
            assert_eq!((size.cell_w(), size.cell_h()), (w, h));
            let a = size.atlas();
            assert_eq!((a.cell_w(), a.cell_h()), (w, h), "atlas() 档位一致");
            assert!(a.glyph(b'#').is_some());
        }
    }

    // —— advance 载荷（痛点⑥根修 03 号 §1.3-2）：三档缺省零漂移 + HD 读表 ——
    #[test]
    fn advance_default_and_hd_table() {
        // 三档点阵：缺省实现 = cell_w()——全 256 字节面零例外（现役消费
        // 点不覆写行为不变的钉板；含域外字节同样回 cell_w）。
        for (atlas, cw) in [
            (&FontAtlas8x16 as &dyn GlyphAtlas, 8u32),
            (&FontAtlas16x32 as &dyn GlyphAtlas, 16),
            (&FontAtlas24x48 as &dyn GlyphAtlas, 24),
        ] {
            for ch in 0u8..=255 {
                assert_eq!(atlas.advance(ch), cw, "等宽档 advance 须恒 cell_w ch={ch}");
            }
        }
        // HD 档：真实比例 advance（03 号 §1.1 表 = genhdfont 黄金锚同源）。
        let hd = FontAtlas32x48;
        for (ch, want) in [('i', 9u32), ('I', 10), ('r', 13), ('s', 16), ('e', 19),
                           ('B', 22), ('W', 30), ('m', 31), (' ', 8)] {
            assert_eq!(hd.advance(ch as u8), want, "{ch} advance");
        }
        // 全表值域 1..=CELL_W + 索引对齐（advance 表与 FONT_HD 同序：抽验
        // '@'=32 整格宽、'%'=31——值域极值在席）。
        for i in 0usize..95 {
            let a = hd.advance((32 + i) as u8);
            assert!((1..=32).contains(&a), "advance 越域 i={i} a={a}");
        }
        assert_eq!(hd.advance(b'@'), 32);
        assert_eq!(hd.advance(b'%'), 31);
        // 域外：回 cell_w（与 glyph None 空格语义成对）。
        for ch in [0u8, 9, 31, 127, 0x80, 0xFF] {
            assert_eq!(hd.advance(ch), 32, "HD 域外回 cell_w ch={ch:#x}");
        }
        // 'Browser Settings' 累计对拍：等宽 16×32=512px vs 比例 269px
        //（03 号 §1.1 实测重现实证——排布面对拍的事实源）。
        let w: u32 = "Browser Settings".bytes().map(|b| hd.advance(b)).sum();
        assert_eq!(w, 269, "'Browser Settings' 比例累计宽");
        // trait 缺省口经 dyn 面可达（FontSize::atlas() 消费同路）。
        assert_eq!(FontSize::S32x48.atlas().advance(b'i'), 9);
        assert_eq!(FontSize::S8x16.atlas().advance(b'i'), 8);
    }

    // —— GlyphCell 契约口：trait 缺省实现与 FontSize 档位面同几何 ——
    #[test]
    fn glyph_cell_contract_face() {
        // trait 缺省 cell()：三档静态图集零改动获得（组合两拆分口）。
        for (atlas, size) in [
            (&FontAtlas8x16 as &dyn GlyphAtlas, FontSize::S8x16),
            (&FontAtlas16x32 as &dyn GlyphAtlas, FontSize::S16x32),
            (&FontAtlas24x48 as &dyn GlyphAtlas, FontSize::S24x48),
            (&FontAtlas32x48 as &dyn GlyphAtlas, FontSize::S32x48),
        ] {
            assert_eq!(atlas.cell(), size.cell(), "trait 缺省口与档位面同几何");
            assert_eq!(atlas.cell(), GlyphCell { w: size.cell_w(), h: size.cell_h() });
        }
        assert_eq!(FontSize::S8x16.cell(), GlyphCell { w: 8, h: 16 });
        assert_eq!(FontSize::S16x32.cell(), GlyphCell { w: 16, h: 32 });
        assert_eq!(FontSize::S24x48.cell(), GlyphCell { w: 24, h: 48 });
        assert_eq!(FontSize::S32x48.cell(), GlyphCell { w: 32, h: 48 });
    }

    // —— GlyphBits 构造口拒面 + 防御采样 ——
    #[test]
    fn glyph_bits_ctor_rejects_and_samples() {
        // 恰长合法（含零尺寸空位图）。
        assert!(GlyphBits::new(8, 16, &[0u8; 16]).is_ok());
        assert!(GlyphBits::new(0, 0, &[]).is_ok());
        assert!(GlyphBits::new(9, 3, &[0u8; 6]).is_ok(), "w=9 → 2B/行");
        // 短行/长行都拒（恰长铁律——payload 随错携带）。
        assert_eq!(
            GlyphBits::new(8, 16, &[0u8; 15]),
            Err(BitsError::PixelCountMismatch { expect: 16, got: 15 })
        );
        assert_eq!(
            GlyphBits::new(8, 16, &[0u8; 17]),
            Err(BitsError::PixelCountMismatch { expect: 16, got: 17 })
        );
        // 坐标域先验（w/h 超 i32 域）。
        assert_eq!(GlyphBits::new(1 << 31, 4, &[]), Err(BitsError::TooLarge));
        assert_eq!(GlyphBits::new(8, 1 << 31, &[]), Err(BitsError::TooLarge));
        // i32 域内最大几何 × 空 rows：64 位下乘法不溢出 → 恰长面拒
        // （checked_mul 是 32 位 usize 的防御臂，64 位上不可达）。
        assert_eq!(
            GlyphBits::new(i32::MAX as usize, i32::MAX as usize, &[]),
            Err(BitsError::PixelCountMismatch {
                expect: (i32::MAX as usize / 8 + 1) * i32::MAX as usize,
                got: 0
            })
        );
        // 采样位法：w=10 跨两字节——x0 = byte0 MSB、x8 = byte1 MSB、
        // x9 = byte1 次高位（非对称位型：位序反转可分辨）。
        let b = GlyphBits { w: 10, h: 1, rows: &[0b1000_0000, 0b0100_0000] };
        assert!(b.bit(0, 0), "x0 = byte0 MSB");
        assert!(!b.bit(8, 0) && b.bit(9, 0), "x8 = byte1 MSB（off）/ x9 = 次高（on）");
        assert!(!b.bit(1, 0) && !b.bit(7, 0) && !b.bit(10, 0), "域内 off + 域外恒 false");
        // 防御采样：字面构造的短行（绕过构造口）不 panic、缺位恒 false
        //（行 0 有数据在位、行 1.. 缺数据恒 false）。
        let bad = GlyphBits { w: 8, h: 4, rows: &[0xFF] };
        assert!(bad.bit(0, 0), "行 0 有数据（在位）");
        for y in 1..4 {
            assert!(!bad.bit(0, y), "缺行 y={y} 恒 false（不 panic）");
        }
        assert!(!bad.bit(8, 0) && !bad.bit(0, 4), "域外恒 false");
    }
}
