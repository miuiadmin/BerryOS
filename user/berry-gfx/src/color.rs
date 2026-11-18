//! 纹理格式与几何（GUI-2 段①）：[`TextureFormat`] 判别值与 pack 语义
//! 与 berry-fbcon `render::PixelFormat` 逐位同源（GOP 原值口径——
//! handoff `gop_info` 透传零翻译层）；[`TextureDesc`] 是 soft 光栅核的
//! 全部几何事实（w/h/stride/格式，不持缓冲——缓冲由
//! `Queue::submit` 按借用递送，VMO-backed 留位后续刀）。
//!
//! 拒面四条照抄 fbcon `Fbcon::new`（w<8 / h<16 / stride<w*4 / 缓冲长度
//! 不足，乘加全 checked——「永不 panic」的库内版，敌意巨参溢出面同样
//! loud 拒）。
//!
//! 坐标域约定（全 crate）：位置与尺寸一律 i32；`desc.w/h > i32::MAX`
//! 构造期拒（[`DescError::TooLarge`]）——光栅核的裁剪/交叠算术因此恒
//! 在 i32/i64 安全界内，无需逐写点防御。

/// 纹理像素格式（UEFI `EFI_GRAPHICS_PIXEL_FORMAT` 原值对齐，判别值即
/// 协议值——同 fbcon `PixelFormat`；v1 soft 后端两格式都支持）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    /// 0 = PixelRedGreenBlueReserved8BitPerColor：R@bits16-23、G@8-15、B@0-7。
    Rgbx = 0,
    /// 1 = PixelBlueGreenRedReserved8BitPerColor：B@bits16-23、G@8-15、R@0-7。
    Bgrx = 1,
}

impl TextureFormat {
    /// handoff/协议原值 → 格式。2 = PixelBitMask / 3 = PixelBltOnly →
    /// `None`（宁缺勿猜：猜成任一硬编码格式 = 整屏换色且静默——
    /// fbcon 同款执法）。
    pub fn from_raw(v: u32) -> Option<TextureFormat> {
        match v {
            0 => Some(TextureFormat::Rgbx),
            1 => Some(TextureFormat::Bgrx),
            _ => None,
        }
    }

    /// RGB 三元组 → 32 位像素值（与 fbcon `PixelFormat::pack` 逐位同：
    /// 通道序自 MSB 起，RGBX 即 R 在最高内容字节）。
    pub fn pack(&self, r: u8, g: u8, b: u8) -> u32 {
        match self {
            TextureFormat::Rgbx => (r as u32) << 16 | (g as u32) << 8 | b as u32,
            TextureFormat::Bgrx => (b as u32) << 16 | (g as u32) << 8 | r as u32,
        }
    }
}

/// 纹理几何拒面错误（loud 拒——畸形几何绝不带病进光栅核）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescError {
    /// w < 8：窄于最小字形宽（fbcon 蜂格下界同款）。
    TooNarrow,
    /// h < 16：矮于最小字形高。
    TooShort,
    /// stride < w*4：一行像素都铺不下。
    StrideTooSmall,
    /// w/h 超 i32 坐标域上界（敌意巨参——光栅核 i32 域的构造期执法点）。
    TooLarge,
    /// 几何乘加溢出 usize（敌意巨参）。
    Overflow,
    /// 缓冲长度 < need（`need_bytes` 口径：最后像素 (w-1,h-1) 尾字节恰
    /// 触到缓冲末尾的下界）。携带 need/got 供 loud 诊断。
    BufferTooSmall { need: usize, got: usize },
}

/// 纹理几何描述：soft 光栅核的全部几何事实。
///
/// 只能经 [`TextureDesc::new`] 构造（字段私有——拒面不变量构造期钉死，
/// 后继 `check_buf`/光栅核直索引全部由此导出）。
#[derive(Debug, Clone, Copy)]
pub struct TextureDesc {
    w: usize,
    h: usize,
    /// 行字节数（GOP 口径，可 > w*4；行尾 padding 永不写）。
    stride: usize,
    format: TextureFormat,
    /// `stride*(h-1) + w*4`（构造期 checked 算好钉死——全 crate 按此执法）。
    need: usize,
}

impl TextureDesc {
    /// 几何验收 + 构造。拒面（前四条照抄 fbcon `Fbcon::new`）：
    /// - `w < 8 || h < 16`：容纳不下一个字形蜂格；
    /// - `w > i32::MAX || h > i32::MAX`：超坐标域（本 crate 独有第五条，
    ///   敌意巨参 loud 拒）；
    /// - `stride < w*4`：一行像素都铺不下；
    /// - 乘加溢出 usize：`Overflow`。
    /// 缓冲长度不足（第四条）与缓冲相关，构造后由 [`Self::check_buf`]
    /// 在拿到缓冲处执法。
    pub fn new(
        w: usize,
        h: usize,
        stride: usize,
        format: TextureFormat,
    ) -> Result<TextureDesc, DescError> {
        if w < 8 {
            return Err(DescError::TooNarrow);
        }
        if h < 16 {
            return Err(DescError::TooShort);
        }
        if w > i32::MAX as usize || h > i32::MAX as usize {
            return Err(DescError::TooLarge);
        }
        let row_bytes = w.checked_mul(4).ok_or(DescError::Overflow)?;
        if stride < row_bytes {
            return Err(DescError::StrideTooSmall);
        }
        let need = stride
            .checked_mul(h - 1)
            .ok_or(DescError::Overflow)?
            .checked_add(row_bytes)
            .ok_or(DescError::Overflow)?;
        Ok(TextureDesc { w, h, stride, format, need })
    }

    /// 像素宽。
    pub fn w(&self) -> usize {
        self.w
    }

    /// 像素高。
    pub fn h(&self) -> usize {
        self.h
    }

    /// 行字节数（≥ w*4，`new` 拒面执法；行尾 padding 不归本 crate）。
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// 像素格式。
    pub fn format(&self) -> TextureFormat {
        self.format
    }

    /// 缓冲长度下界：`stride*(h-1) + w*4`（最后像素尾字节恰触末尾的
    /// 精确下界，`==` 合法——拒面不含「恰好装下」）。
    pub fn need_bytes(&self) -> usize {
        self.need
    }

    /// 缓冲长度拒面（第四条执法点）：`len < need` → `BufferTooSmall`
    /// loud（need/got 随错携带，供诊断行直打）。
    pub fn check_buf(&self, len: usize) -> Result<(), DescError> {
        if len < self.need {
            return Err(DescError::BufferTooSmall { need: self.need, got: len });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— from_raw：0/1 → Some；2=BitMask/3=BltOnly/敌意值 → None ——
    #[test]
    fn from_raw_roundtrip() {
        assert_eq!(TextureFormat::from_raw(0), Some(TextureFormat::Rgbx));
        assert_eq!(TextureFormat::from_raw(1), Some(TextureFormat::Bgrx));
        // 判别值口径钉板（GOP 原值，非自编序）。
        assert_eq!(TextureFormat::Rgbx as u32, 0);
        assert_eq!(TextureFormat::Bgrx as u32, 1);
        for v in [2u32, 3, 4, 0xFFFF_FFFF] {
            assert_eq!(TextureFormat::from_raw(v), None, "v={v:#x}");
        }
    }

    // —— pack 不对称色位序（与 fbcon 逐位同）——
    #[test]
    fn pack_asymmetric_color() {
        let (r, g, b) = (0xAAu8, 0xBB, 0xCC);
        assert_eq!(TextureFormat::Rgbx.pack(r, g, b), 0x00AA_BB_CC);
        assert_eq!(TextureFormat::Bgrx.pack(r, g, b), 0x00CC_BB_AA);
        assert_ne!(
            TextureFormat::Rgbx.pack(r, g, b),
            TextureFormat::Bgrx.pack(r, g, b)
        );
        // 纯白/纯黑两格式同值（通道对称面恒等）。
        assert_eq!(TextureFormat::Rgbx.pack(255, 255, 255), 0x00FF_FFFF);
        assert_eq!(TextureFormat::Bgrx.pack(0, 0, 0), 0);
    }

    // —— 拒面前三条 + 敌意巨参（第四条缓冲面在 check_buf）——
    #[test]
    fn desc_rejects_degenerate_geometry() {
        // w < 8（其余合法）。
        assert_eq!(
            TextureDesc::new(7, 16, 28, TextureFormat::Rgbx).unwrap_err(),
            DescError::TooNarrow
        );
        // h < 16。
        assert_eq!(
            TextureDesc::new(8, 15, 32, TextureFormat::Rgbx).unwrap_err(),
            DescError::TooShort
        );
        // stride < w*4。
        assert_eq!(
            TextureDesc::new(64, 32, 255, TextureFormat::Rgbx).unwrap_err(),
            DescError::StrideTooSmall
        );
        // 敌意巨参：w 超 i32 域 / stride 乘加溢出。
        assert_eq!(
            TextureDesc::new(1 << 40, 16, 1 << 44, TextureFormat::Rgbx).unwrap_err(),
            DescError::TooLarge
        );
        assert_eq!(
            TextureDesc::new(8, 16, usize::MAX, TextureFormat::Rgbx).unwrap_err(),
            DescError::Overflow
        );
        // 边界正面：w=8/h=16/stride=w*4 恰达标。
        assert!(TextureDesc::new(8, 16, 32, TextureFormat::Bgrx).is_ok());
    }

    // —— check_buf 第四拒面 + 恰好装下合法 + need 公式（含 padding）——
    #[test]
    fn check_buf_boundary_and_need() {
        // w=16/h=16/stride=80（行内容 64B + 16B padding）：
        // need = 80*15 + 64 = 1264。
        let d = TextureDesc::new(16, 16, 80, TextureFormat::Rgbx).unwrap();
        assert_eq!(d.need_bytes(), 80 * 15 + 64);
        assert_eq!(d.check_buf(1263), Err(DescError::BufferTooSmall { need: 1264, got: 1263 }));
        assert!(d.check_buf(1264).is_ok(), "== need 恰好装下合法");
        assert!(d.check_buf(2000).is_ok());
        // getter 钉板。
        assert_eq!((d.w(), d.h(), d.stride()), (16, 16, 80));
        assert_eq!(d.format(), TextureFormat::Rgbx);
    }
}
