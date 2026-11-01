//! 命令队列（六件套的回放半边）：[`Queue::submit`] = soft 光栅核的
//! 唯一执行入口——把 pass 当**屏障照跑**（语义不特化，六件套 ①纪律）：
//!
//! 1. **先验后写**：缓冲长度 / dirty 合法性 / blit 源界 / 图像位恰长
//!    四道拒面全过才动笔（出错零半执行面——目标缓冲不留半个 pass 的
//!    笔迹）；
//! 2. `load_op`（Clear 只铺 dirty ∩ 纹理界；Load 不动）；
//! 3. 逐命令回放（全部裁剪到 纹理界 ∩ dirty 区——dirty 区外结构性
//!    不可触碰）；
//! 4. `Present` no-op（soft = 整帧已直写；flip 留位）。
//!
//! v1 soft 单队列零内态（`Queue` 单元结构——API 形态锚点）；真后端
//! （virtio-gpu）双队列/异步提交留后续刀，接口面不变。

use crate::cmd::{Cmd, DisplayList, ImageSrc, Rect};
use crate::color::{DescError, TextureDesc};
use crate::pass::{LoadOp, PassDescriptor};
use crate::raster;

/// submit 拒面（全部先验后写——见模块头）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    /// 目标缓冲过小（`TextureDesc::check_buf` 第四拒面执法点；
    /// need/got 随错携带供诊断行直打）。
    BufferTooSmall { need: usize, got: usize },
    /// dirty 矩形负宽/高（零字节落笔即拒）。
    BadDirtyRect,
    /// Blit 源区越纹理界（`index` = 命令序号）。源区必须**整区**在界内
    /// ——半出界裁剪源 = 静默改语义，禁；零尺寸源恒放行（空操作）。
    BlitSrcOutOfBounds { index: usize },
    /// DrawImage 位图畸形（`index` = 命令序号）。两族恰长铁律：
    /// `ImageSrc::Bits` 字面构造绕过 `GlyphBits::new` 的，`rows.len() !=
    /// h * 行字节`（短行 = 越界位静默错位、长行 = 尾字节静默丢弃）；
    /// `ImageSrc::Argb` 的 `px.len() != w*h`（同族先验——短切片缺像素、
    /// 长切片尾随静默丢，两者都禁）。图标槽位面不在此门（实现契约 +
    /// `bit()`/`get()` 防御采样兜）。
    ImagePixelMismatch { index: usize },
}

/// 命令队列（soft 后端：submit = 同步整列表回放，无内态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Queue;

impl Queue {
    pub fn new() -> Queue {
        Queue
    }

    /// 回放一个显示列表到目标纹理（pass 屏障语义——模块头四步）。
    pub fn submit<'a>(
        &mut self,
        dl: &DisplayList<'a>,
        target: &mut [u8],
        desc: &TextureDesc,
        pass: &PassDescriptor,
    ) -> Result<(), SubmitError> {
        // —— 先验后写：四道拒面（零半执行面）——
        // 第四拒面：check_buf 只可能返 BufferTooSmall（其余变体在
        // TextureDesc::new 构造期已拦，字段私有不可绕）。
        if let Err(DescError::BufferTooSmall { need, got }) = desc.check_buf(target.len()) {
            return Err(SubmitError::BufferTooSmall { need, got });
        }
        if let Some(d) = &pass.dirty {
            if !d.is_valid() {
                return Err(SubmitError::BadDirtyRect);
            }
        }
        for (i, cmd) in dl.iter().enumerate() {
            match cmd {
                Cmd::Blit { src, .. } => {
                    if !blit_src_in_frame(src, desc) {
                        return Err(SubmitError::BlitSrcOutOfBounds { index: i });
                    }
                }
                // 图像位恰长硬门：Bits 面 = 行字节公式（行字节公式单一
                // 事实源 = GlyphBits::row_bytes——与构造口同一算式，两处
                // 手写必漂，恰长门与权威门执法的必须是同一条律）；Argb
                // 面 = px.len() == w*h（u64 中转——u32×u32 恒可算，巨参
                // 手搓在乘法上无溢出假象）。图标槽位面不在此门（实现
                // 契约 + `bit()` 防御采样兜）。
                Cmd::DrawImage { src: ImageSrc::Bits(b), .. } => {
                    let expect = b.row_bytes().checked_mul(b.h);
                    if !matches!(expect, Some(n) if n == b.rows.len()) {
                        return Err(SubmitError::ImagePixelMismatch { index: i });
                    }
                }
                Cmd::DrawImage { src: ImageSrc::Argb { w, h, px }, .. } => {
                    if (*w as u64) * (*h as u64) != px.len() as u64 {
                        return Err(SubmitError::ImagePixelMismatch { index: i });
                    }
                }
                Cmd::DrawImage { .. } => {}
                _ => {}
            }
        }

        // —— 屏障照跑：clip = 纹理界 ∩ dirty 区（None = 整帧）——
        let frame = Rect::new(0, 0, desc.w() as i32, desc.h() as i32);
        let clip = match pass.dirty {
            Some(d) => d.intersect(&frame).unwrap_or(Rect::new(0, 0, 0, 0)),
            None => frame,
        };

        // load_op：Clear 只铺 clip（u32 pack 后走统一写口——padding 列
        // 结构性不可达）；Load 不动。
        if let LoadOp::Clear { r, g, b } = pass.load_op {
            let px = desc.format().pack(r, g, b);
            raster::fill_rect(target, desc, &clip, px, &clip);
        }

        // 逐命令回放（raster 内部逐像素裁到 clip）。
        for cmd in dl.iter() {
            raster::exec(target, desc, cmd, &clip);
        }

        // StoreOp：soft 路径 Store/Discard 都是 no-op（内容已在调用方
        // 缓冲——Discard 的「内容未定义」真后端语义见 pass.rs 头注）。
        Ok(())
    }
}

impl Default for Queue {
    fn default() -> Self {
        Queue::new()
    }
}

/// blit 源区整区在纹理界内谓词（零尺寸恒真——空操作无界可越）。
/// saturating_add 防巨参和值溢出（w/h ≥ 0 已在录入期执法）。
fn blit_src_in_frame(src: &Rect, desc: &TextureDesc) -> bool {
    src.w == 0
        || src.h == 0
        || (src.x >= 0
            && src.y >= 0
            && src.x.saturating_add(src.w) <= desc.w() as i32
            && src.y.saturating_add(src.h) <= desc.h() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun};
    use crate::color::TextureFormat;
    use crate::font::{FontAtlas8x16, GlyphAtlas, GlyphBits};
    use crate::pass::StoreOp;

    /// 测试常备几何：16×16、stride=64（无 padding 的最小帧）。
    fn desc16() -> TextureDesc {
        TextureDesc::new(16, 16, 64, TextureFormat::Rgbx).unwrap()
    }

    /// 读单像素（u32 小端对偶解码）。
    fn px(fb: &[u8], stride: usize, x: i32, y: i32) -> u32 {
        let o = y as usize * stride + x as usize * 4;
        u32::from_le_bytes(fb[o..o + 4].try_into().unwrap())
    }

    const WHITE: Color = Color { r: 255, g: 255, b: 255 };
    const RED: Color = Color { r: 255, g: 0, b: 0 };
    const GREEN: Color = Color { r: 0, g: 255, b: 0 };
    const BLUE: Color = Color { r: 0, g: 0, b: 255 };

    // —— fill 整屏 + Present no-op + LoadOp::Load 不动旧内容 ——
    #[test]
    fn fill_full_screen_and_present_noop() {
        let d = desc16();
        let mut fb = vec![0xEEu8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.fill_rect(Rect::new(0, 0, 16, 16), RED).unwrap();
        enc.present(Rect::new(0, 0, 16, 16)).unwrap(); // soft no-op
        let dl = enc.finish();
        // Load（不动）+ Present：全部像素变红只能来自 FillRect。
        Queue::new()
            .submit(&dl, &mut fb, &d, &PassDescriptor::load())
            .unwrap();
        for y in 0..16 {
            for x in 0..16 {
                assert_eq!(px(&fb, 64, x, y), 0x00FF_0000, "@({x},{y})");
            }
        }
    }

    // —— fill 单像素 ——
    #[test]
    fn fill_single_pixel() {
        let d = desc16();
        let mut fb = vec![0u8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.fill_rect(Rect::new(5, 9, 1, 1), GREEN).unwrap();
        let dl = enc.finish();
        Queue::new()
            .submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0))
            .unwrap();
        assert_eq!(px(&fb, 64, 5, 9), 0x0000_FF00);
        assert_eq!(px(&fb, 64, 4, 9), 0, "左邻未写");
        assert_eq!(px(&fb, 64, 5, 8), 0, "上邻未写");
        assert_eq!(px(&fb, 64, 6, 9), 0, "右邻未写");
    }

    // —— fill 越界裁剪：负原点/越右下界都裁到帧内，帧外不炸不写 ——
    #[test]
    fn fill_clips_out_of_bounds() {
        let d = desc16();
        let mut fb = vec![0u8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.fill_rect(Rect::new(-4, -4, 24, 24), BLUE).unwrap(); // 覆盖全域
        enc.fill_rect(Rect::new(12, 12, 8, 8), RED).unwrap(); // 只右下角可见
        let dl = enc.finish();
        Queue::new()
            .submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0))
            .unwrap();
        // 全帧蓝（第一条 FillRect 裁剪后恰整帧）；右下角红（第二条只剩
        // 4×4 可见——且证明裁剪后仍执行）。
        for y in 0..16 {
            for x in 0..16 {
                let expect = if x >= 12 && y >= 12 { 0x00FF_0000 } else { 0x0000_00FF };
                assert_eq!(px(&fb, 64, x, y), expect, "@({x},{y})");
            }
        }
    }

    // —— stride padding 铁律：行尾 padding 字节绝不写 ——
    #[test]
    fn stride_padding_never_touched() {
        // 16×16、stride=80（行内容 64B + 16B padding）。
        let d = TextureDesc::new(16, 16, 80, TextureFormat::Rgbx).unwrap();
        let mut fb = vec![0xEEu8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.fill_rect(Rect::new(0, 0, 16, 16), WHITE).unwrap();
        let dl = enc.finish();
        Queue::new()
            .submit(&dl, &mut fb, &d, &PassDescriptor::clear(255, 255, 255))
            .unwrap();
        // 每行 16B padding 原样（末行缓冲只含内容 64B——need 公式不含
        // 末行 padding，y=15 无 padding 可检）。
        for y in 0..15 {
            assert_eq!(fb[y * 80 + 64..y * 80 + 80], [0xEE; 16], "padding y={y}");
        }
        // 内容列确实写了（不是全帧 0xEE 假绿）。
        assert_eq!(px(&fb, 80, 15, 15), 0x00FF_FFFF);
    }

    // —— dirty 区外 = 别人的画面：Clear 与命令都只进 dirty ——
    #[test]
    fn dirty_region_containment() {
        let d = desc16();
        let mut fb = vec![0xEEu8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.fill_rect(Rect::new(0, 0, 16, 16), WHITE).unwrap(); // 全域命令
        let dl = enc.finish();
        // dirty = (4,4,8,8)：Clear 蓝 + 全域白 FillRect 都只落区内。
        Queue::new()
            .submit(
                &dl,
                &mut fb,
                &d,
                &PassDescriptor::new(LoadOp::Clear { r: 0, g: 0, b: 255 }, StoreOp::Store, Some(Rect::new(4, 4, 8, 8))),
            )
            .unwrap();
        for y in 0..16 {
            for x in 0..16 {
                let inside = (4..12).contains(&x) && (4..12).contains(&y);
                let expect = if inside { 0x00FF_FFFF } else { 0xEEEE_EEEE };
                assert_eq!(px(&fb, 64, x, y), expect, "@({x},{y})");
            }
        }
        // dirty 与帧界交叠为空（区外）= 全 no-op 合态。
        let mut fb2 = vec![0xEEu8; d.need_bytes()];
        Queue::new()
            .submit(&dl, &mut fb2, &d, &PassDescriptor::clear(255, 0, 0))
            .unwrap(); // 无 dirty 对照：整帧可写
        assert_eq!(px(&fb2, 64, 0, 0), 0x00FF_FFFF);
    }

    // —— LoadOp 两档对照：Clear 整帧 / Load 保旧 ——
    #[test]
    fn load_variants() {
        let d = desc16();
        let dl = DisplayList::new(); // 空命令表——纯 load_op 面
        let mut a = vec![0x11u8; d.need_bytes()];
        Queue::new().submit(&dl, &mut a, &d, &PassDescriptor::clear(1, 2, 3)).unwrap();
        assert_eq!(px(&a, 64, 7, 7), 0x0001_0203, "Clear 铺满");
        let mut b = vec![0x11u8; d.need_bytes()];
        Queue::new().submit(&dl, &mut b, &d, &PassDescriptor::load()).unwrap();
        assert_eq!(b, vec![0x11u8; d.need_bytes()], "Load 逐字节不动");
    }

    // —— Bresenham：水平/垂直/45° 对角 + 出界端点不炸 ——
    #[test]
    fn bresenham_axes_and_diagonal() {
        let d = desc16();
        let mut fb = vec![0u8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.draw_line(0, 2, 15, 2, WHITE).unwrap(); // 水平
        enc.draw_line(5, 0, 5, 15, WHITE).unwrap(); // 垂直
        enc.draw_line(0, 0, 15, 15, RED).unwrap(); // 45° 对角
        enc.draw_line(-8, 9, 30, 9, GREEN).unwrap(); // 两端出界（y=9 整行）
        let dl = enc.finish();
        Queue::new()
            .submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0))
            .unwrap();
        // 全 256 像素按「绘制序优先」闭环：绿行最后画 > 对角 > 白轴 >
        // 清屏黑——交点颜色按命令序收口（不是只查线上孤立点）。
        for y in 0..16 {
            for x in 0..16 {
                let expect = if y == 9 {
                    0x0000_FF00 // 绿线最后画（含交点 (5,9)/(9,9)）
                } else if x == y {
                    0x00FF_0000 // 45° 对角（穿白轴交点 (2,2)/(5,5)）
                } else if y == 2 || x == 5 {
                    0x00FF_FFFF // 水平/垂直白轴
                } else {
                    0
                };
                assert_eq!(px(&fb, 64, x, y), expect, "@({x},{y})");
            }
        }
    }

    // —— blit：水平重叠 memmove + 垂直重叠行序安全（双病理可分）——
    #[test]
    fn blit_overlap_memmove_and_row_order() {
        let d = desc16();
        // 病理 1（行内水平重叠）：左半 cols[0,8) 红右半绿，右移 4px。
        let mut fb = vec![0u8; d.need_bytes()];
        {
            let mut enc = CommandEncoder::new();
            enc.fill_rect(Rect::new(0, 0, 4, 16), RED).unwrap();
            enc.fill_rect(Rect::new(4, 0, 4, 16), GREEN).unwrap();
            enc.blit(Rect::new(0, 0, 8, 16), 4, 0).unwrap(); // dst 与 src 同行交叠 [4,8)
            let dl = enc.finish();
            Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0)).unwrap();
        }
        for y in 0..16 {
            // cols[0,4) 原红；cols[4,8) ← 源红；cols[8,12) ← 源绿；
            // cols[12,16) 未触（清屏黑）。正向字节拷贝会把 col8 也染红。
            assert_eq!(px(&fb, 64, 0, y), 0x00FF_0000, "y={y}");
            assert_eq!(px(&fb, 64, 4, y), 0x00FF_0000);
            assert_eq!(px(&fb, 64, 8, y), 0x0000_FF00, "col8 应绿（memmove 语义）");
            assert_eq!(px(&fb, 64, 11, y), 0x0000_FF00);
            assert_eq!(px(&fb, 64, 12, y), 0);
        }
        // 病理 2（垂直重叠行序）：上半红下半绿，下移 4px——错误行序
        // （自顶向下）会把已覆盖行读回去：rows[8,12) 将错染红。
        let mut fb = vec![0u8; d.need_bytes()];
        {
            let mut enc = CommandEncoder::new();
            enc.fill_rect(Rect::new(0, 0, 16, 4), RED).unwrap();
            enc.fill_rect(Rect::new(0, 4, 16, 4), GREEN).unwrap();
            enc.blit(Rect::new(0, 0, 16, 8), 0, 4).unwrap();
            let dl = enc.finish();
            Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0)).unwrap();
        }
        for x in 0..16 {
            assert_eq!(px(&fb, 64, x, 4), 0x00FF_0000, "rows[4,8)←源红");
            assert_eq!(px(&fb, 64, x, 8), 0x0000_FF00, "rows[8,12)←源绿（行序安全）");
            assert_eq!(px(&fb, 64, x, 12), 0, "rows[12,16) 未触");
        }
    }

    // —— blit 拒面：源区越纹理界 loud（零尺寸放行）——
    #[test]
    fn blit_src_bounds_loud() {
        let d = desc16();
        let mut fb = vec![0u8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.fill_rect(Rect::new(0, 0, 1, 1), WHITE).unwrap(); // index 0
        enc.blit(Rect::new(0, 0, 17, 4), 0, 0).unwrap(); // index 1：宽越 1px
        let dl = enc.finish();
        let mut fb_snapshot = fb.clone();
        let e = Queue::new()
            .submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0))
            .unwrap_err();
        assert_eq!(e, SubmitError::BlitSrcOutOfBounds { index: 1 });
        assert_eq!(fb, fb_snapshot, "先验后写：拒面零落笔");
        fb_snapshot = fb.clone();
        // 零尺寸源恒放行（空操作）+ 负原点零尺寸同样空操作。
        let mut enc = CommandEncoder::new();
        enc.blit(Rect::new(-5, -5, 0, 8), 0, 0).unwrap();
        enc.blit(Rect::new(0, 0, 8, 0), 3, 3).unwrap();
        let dl = enc.finish();
        assert!(Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).is_ok());
        assert_eq!(fb, fb_snapshot, "零尺寸 = no-op");
    }

    // —— submit 拒面：缓冲过小 loud（need/got 随错携带）——
    #[test]
    fn submit_buffer_too_small() {
        let d = desc16();
        let dl = DisplayList::new();
        let mut short = vec![0u8; d.need_bytes() - 1];
        assert_eq!(
            Queue::new().submit(&dl, &mut short, &d, &PassDescriptor::load()),
            Err(SubmitError::BufferTooSmall { need: d.need_bytes(), got: d.need_bytes() - 1 })
        );
        let mut bad_dirty = vec![0u8; d.need_bytes()];
        assert_eq!(
            Queue::new().submit(
                &dl,
                &mut bad_dirty,
                &d,
                &PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(Rect::new(0, 0, 4, -1)))
            ),
            Err(SubmitError::BadDirtyRect)
        );
    }

    /// 手工期望图集：row0=0x80（仅最左列）、row1=0x01（仅最右列）、
    /// row2=0xFF（整行）——位序断言的字面基准，不与任何生成器同源。
    struct FakeAtlas {
        ch: u8,
    }
    impl GlyphAtlas for FakeAtlas {
        fn cell_w(&self) -> usize {
            8
        }
        fn cell_h(&self) -> usize {
            16
        }
        fn glyph(&self, ch: u8) -> Option<GlyphBits<'_>> {
            const G: [u8; 16] = [
                0x80, 0x01, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ];
            (ch == self.ch).then_some(GlyphBits { w: 8, h: 16, rows: &G })
        }
    }

    // —— glyph 位序（手工期望位图）：MSB=最左列 + 透明底两档 ——
    #[test]
    fn glyph_bit_order_handmade() {
        let d = desc16();
        let atlas = FakeAtlas { ch: b'K' };
        // 不透明衬底：bg 铺零位。
        let mut fb = vec![0u8; d.need_bytes()];
        let run = GlyphRun {
            atlas: &atlas,
            glyphs: &[GlyphPos { ch: b'K', x: 2, y: 3 }],
            fg: WHITE,
            bg: Some(BLUE),
        };
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        Queue::new()
            .submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0))
            .unwrap();
        assert_eq!(px(&fb, 64, 2, 3), 0x00FF_FFFF, "row0 bit7=最左列");
        assert_eq!(px(&fb, 64, 3, 3), 0x0000_00FF, "row0 其余位 = bg");
        assert_eq!(px(&fb, 64, 9, 4), 0x00FF_FFFF, "row1 bit0=最右列");
        assert_eq!(px(&fb, 64, 2, 4), 0x0000_00FF, "row1 其余位 = bg");
        assert_eq!(px(&fb, 64, 5, 5), 0x00FF_FFFF, "row2 整行着墨");
        assert_eq!(px(&fb, 64, 2, 6), 0x0000_00FF, "row3+ = bg");
        assert_eq!(px(&fb, 64, 1, 3), 0, "字形区外未触（清屏黑）");
        // 透明底：零位不写（衬底色穿透）。
        let mut fb2 = vec![0xEEu8; d.need_bytes()];
        let run = GlyphRun { atlas: &atlas, glyphs: &[GlyphPos { ch: b'K', x: 2, y: 3 }], fg: WHITE, bg: None };
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        Queue::new().submit(&dl, &mut fb2, &d, &PassDescriptor::load()).unwrap();
        assert_eq!(px(&fb2, 64, 2, 3), 0x00FF_FFFF, "着墨位照写");
        assert_eq!(px(&fb2, 64, 3, 3), 0xEEEE_EEEE, "零位不写（穿透）");
        // 缺字形（'Z' 不在 FakeAtlas）：bg=Some → 全 bg 蜂格。
        let run = GlyphRun {
            atlas: &atlas,
            glyphs: &[GlyphPos { ch: b'Z', x: 0, y: 0 }],
            fg: WHITE,
            bg: Some(RED),
        };
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        Queue::new()
            .submit(&dl, &mut fb2, &d, &PassDescriptor::load())
            .unwrap();
        for gy in 0..16 {
            for gx in 0..8 {
                assert_eq!(px(&fb2, 64, gx, gy), 0x00FF_0000, "空格语义 @({gx},{gy})");
            }
        }
    }

    // —— 与 berry-fbcon 蜂格路径逐字节共识（两套独立代码、像素必同）——
    #[test]
    fn glyph_consensus_with_fbcon() {
        let (w, h, stride) = (64usize, 32usize, 256usize);
        let d = TextureDesc::new(w, h, stride, TextureFormat::Rgbx).unwrap();
        let mut a = vec![0u8; d.need_bytes()];
        let mut b = vec![0u8; d.need_bytes()];
        {
            let mut c = berry_fbcon::render::Fbcon::new(
                &mut a,
                w,
                h,
                stride,
                berry_fbcon::render::PixelFormat::Rgbx,
            )
            .unwrap();
            c.put_char(2, 1, b'#', (0x11, 0x22, 0x33), (0x44, 0x55, 0x66));
        }
        let atlas = FontAtlas8x16;
        let run = GlyphRun {
            atlas: &atlas,
            glyphs: &[GlyphPos { ch: b'#', x: 16, y: 16 }], // 蜂格 (2,1) = 像素 (16,16)
            fg: Color { r: 0x11, g: 0x22, b: 0x33 },
            bg: Some(Color { r: 0x44, g: 0x55, b: 0x66 }),
        };
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        Queue::new()
            .submit(&dl, &mut b, &d, &PassDescriptor::load())
            .unwrap();
        assert_eq!(a, b, "soft glyph 路径与 fbcon 蜂格路径逐字节共识");
    }
}
