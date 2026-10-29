//! 多字号共识组（段②）：genfbfont 放大档（2x/3x 静态图集）↔ 运行时
//! 从 1x 表独立重推的 NN 放大——**双实现共识**（生成器 Python 侧对拍
//! 之外的第二条独立执法线：Rust 测试内从 berry-fbcon 1x 表按位公式
//! 重推，与生成产物全 95 形 × 两档逐位比对；P6 A-5 fixture 共识的
//! 「双绿即独立收敛」纪律同源）。
//!
//! 位法公式（与 `tools/genfbfont.py` `scale_glyph` 同一式，两侧独立
//! 实现）：输出 `(c,r)` 位 = 源 `(c/N, r/N)` 位；源位 = 行字节
//! `& (0x80 >> gx)`（MSB = 最左列）。
//!
//! 渲染面另验：放大档像素级 NN 精确性（每个 2x/3x 像素 = 对应 1x 源
//! 位的着色）、放大档缺字形空格语义（bg=Some 铺整字格 / bg=None
//! no-op）、裁剪与 dirty 区包络。

use berry_fbcon::font as fbfont;
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun, Rect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;

const WHITE: Color = Color { r: 255, g: 255, b: 255 };
const BLACK: Color = Color { r: 0, g: 0, b: 0 };
const RED: Color = Color { r: 255, g: 0, b: 0 };

/// 读单像素（u32 小端）。
fn px(fb: &[u8], stride: usize, x: i32, y: i32) -> u32 {
    let o = y as usize * stride + x as usize * 4;
    u32::from_le_bytes(fb[o..o + 4].try_into().unwrap())
}

/// 运行时 NN 重推的独立位法实现（不调用 berry-gfx 任何位函数——共识
/// 测试的独立性面）：`(c,r)` → 源 1x 位。
fn nn_bit(src: &[u8; 16], n: usize, c: usize, r: usize) -> bool {
    src[r / n] & (0x80 >> (c / n)) != 0
}

// —— 全 95 形 × 两档逐位共识（双实现：生成产物 ↔ 运行时重推）——
#[test]
fn nn_upscale_cross_impl_consensus_all_glyphs() {
    for (n, size) in [(2usize, FontSize::S16x32), (3usize, FontSize::S24x48)] {
        let atlas = size.atlas();
        assert_eq!((atlas.cell_w(), atlas.cell_h()), (8 * n, 16 * n), "N={n} 几何");
        for ch in 32u8..=126 {
            let src = fbfont::glyph(ch).expect("1x 域内同域");
            let g = atlas.glyph(ch).expect("放大档域内");
            // 几何/恰长。
            assert_eq!((g.w, g.h), (8 * n, 16 * n), "N={n} ch={ch}");
            assert_eq!(g.rows.len(), 16 * n * n, "恰长 = 16N 行 × N 字节 ch={ch}");
            // 逐位重推（N²×8N 位全比对——零抽样，全量面）。
            for r in 0..16 * n {
                for c in 0..8 * n {
                    assert_eq!(
                        g.bit(c, r),
                        nn_bit(src, n, c, r),
                        "NN 位漂移 N={n} ch={ch} @({c},{r})"
                    );
                }
            }
        }
        // 空格全零位图（1x 域内共识的放大档对偶）。
        let sp = atlas.glyph(b' ').unwrap();
        assert!(sp.rows.iter().all(|&b| b == 0), "N={n} 空格 = 全零位图");
    }
}

// —— 放大档渲染像素级 NN 精确性：每输出像素 = 对应 1x 源位着色 ——
#[test]
fn scaled_render_pixel_exact_nn() {
    // 2x：16×32 帧恰一字格，'#' 画在 (0,0)，fg 白 bg 黑。
    {
        let (w, h, stride) = (16usize, 32usize, 64usize);
        let d = TextureDesc::new(w, h, stride, TextureFormat::Rgbx).unwrap();
        let src = fbfont::glyph(b'#').unwrap();
        let positions = [GlyphPos { ch: b'#', x: 0, y: 0 }];
        let run = GlyphRun { atlas: FontSize::S16x32.atlas(), glyphs: &positions, fg: WHITE, bg: Some(BLACK) };
        let mut fb = vec![0u8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
        for r in 0..h {
            for c in 0..w {
                let expect = if nn_bit(src, 2, c, r) { 0x00FF_FFFF } else { 0 };
                assert_eq!(px(&fb, stride, c as i32, r as i32), expect, "s2 '#' @({c},{r})");
            }
        }
    }
    // 3x：24×48 帧恰一字格，非灰彩色（pack 错可分辨）+ Load 保旧对照。
    {
        let (w, h, stride) = (24usize, 48usize, 96usize);
        let d = TextureDesc::new(w, h, stride, TextureFormat::Rgbx).unwrap();
        let src = fbfont::glyph(b'A').unwrap();
        let positions = [GlyphPos { ch: b'A', x: 0, y: 0 }];
        let run = GlyphRun {
            atlas: FontSize::S24x48.atlas(),
            glyphs: &positions,
            fg: Color { r: 0x11, g: 0x22, b: 0x33 },
            bg: Some(Color { r: 0x44, g: 0x55, b: 0x66 }),
        };
        let mut fb = vec![0u8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
        for r in 0..h {
            for c in 0..w {
                let expect = if nn_bit(src, 3, c, r) { 0x0011_2233 } else { 0x0044_5566 };
                assert_eq!(px(&fb, stride, c as i32, r as i32), expect, "s3 'A' @({c},{r})");
            }
        }
    }
}

// —— 放大档缺字形空格语义：bg=Some 铺整字格 / bg=None no-op ——
#[test]
fn scaled_missing_glyph_blank_cell() {
    let (w, h, stride) = (32usize, 32usize, 128usize);
    let d = TextureDesc::new(w, h, stride, TextureFormat::Rgbx).unwrap();
    // 0x7F（DEL）/ 0x00（控制符）都域外（图集域 = 32..=126）。
    for ch in [0x7Fu8, 0x00] {
        let positions = [GlyphPos { ch, x: 8, y: 0 }];
        // bg=Some：整 16×32 字格铺 bg（红）——「全零位图」的等价语义。
        let run = GlyphRun { atlas: FontSize::S16x32.atlas(), glyphs: &positions, fg: WHITE, bg: Some(RED) };
        let mut fb = vec![0u8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0)).unwrap();
        for r in 0..32 {
            for c in 0..32 {
                let in_cell = (8..24).contains(&c);
                let expect = if in_cell { 0x00FF_0000 } else { 0 };
                assert_eq!(px(&fb, stride, c as i32, r as i32), expect, "ch={ch:#x} @({c},{r})");
            }
        }
        // bg=None：no-op（清屏黑保持——零落笔）。
        let run = GlyphRun { atlas: FontSize::S16x32.atlas(), glyphs: &positions, fg: WHITE, bg: None };
        let mut fb2 = vec![0u8; d.need_bytes()];
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        Queue::new().submit(&dl, &mut fb2, &d, &PassDescriptor::clear(0, 0, 0)).unwrap();
        assert!(fb2.iter().all(|&b| b == 0), "ch={ch:#x} 透明底缺字形 = 零落笔");
    }
}

// —— 放大档裁剪与 dirty 包络：负原点半格 + dirty 区外零落笔 ——
#[test]
fn scaled_clipping_and_dirty() {
    // 帧 16×32 = s3 字格可见半面（'#' 画在 (-8,-16)）：源 1x 位 (0,0) 落
    // 帧外、(1,1) 落帧 (0,0)……可见半格的每个像素仍按 NN 源位着色，帧
    // 恰被可见半面铺满（无「格外面」——断言全域即断言半面）。
    let (w, h, stride) = (16usize, 32usize, 64usize);
    let d = TextureDesc::new(w, h, stride, TextureFormat::Rgbx).unwrap();
    let src = fbfont::glyph(b'#').unwrap();
    let positions = [GlyphPos { ch: b'#', x: -8, y: -16 }];
    let run = GlyphRun { atlas: FontSize::S24x48.atlas(), glyphs: &positions, fg: WHITE, bg: Some(BLACK) };
    let mut fb = vec![0u8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_glyph_run(run).unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    for r in 0..h {
        for c in 0..w {
            // 字格原点 (-8,-16) ⇒ 源 1x 位 = ((c+8)/3, (r+16)/3)；帧内
            // 恒算出界内源位（c+8 ∈ [8,24) ⊂ [0,24) 同 r）。
            let on = nn_bit(src, 3, c + 8, r + 16);
            let expect = if on { 0x00FF_FFFF } else { 0 };
            assert_eq!(px(&fb, stride, c as i32, r as i32), expect, "裁剪后 @({c},{r})");
        }
    }
    // dirty 包络：同一命令 + dirty=(4,8,8,8)——区外哨兵逐字节不动。
    let mut fb2 = vec![0xEEu8; d.need_bytes()];
    Queue::new()
        .submit(
            &dl,
            &mut fb2,
            &d,
            &PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(Rect::new(4, 8, 8, 8))),
        )
        .unwrap();
    for r in 0..h {
        for c in 0..w {
            let inside = (4..12).contains(&c) && (8..16).contains(&r);
            let expect = if inside {
                if nn_bit(src, 3, c + 8, r + 16) { 0x00FF_FFFF } else { 0 }
            } else {
                0xEEEE_EEEE
            };
            assert_eq!(px(&fb2, stride, c as i32, r as i32), expect, "dirty @({c},{r})");
        }
    }
}
