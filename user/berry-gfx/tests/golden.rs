//! golden 组（GUI-2 段①）：期望像素两级钉死——
//!
//! 1. **小场景字面常量**：期望像素 = Rust 源内 `[u32; W*H]` 字面数组
//!    （手工生成一次入册，改 raster 语义/命令序/位序任一处即炸）——
//!    golden 不复用任何 src 函数，防「双绿 = 单实现自洽」的基准漂移雷
//!    （P6 A-5 fixture 共识雷的反面设计）；
//! 2. **较大场景独立参考算法**：测试内直写循环（clear/rect/glyph/blit
//!    全部手写，blit 用「先快照后落盘」的 as-if 语义——与 src 的
//!    `ptr::copy` 行序实现互证），不复用 src 光栅函数。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun, Rect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontAtlas8x16;
use berry_gfx::pass::PassDescriptor;
use berry_gfx::queue::Queue;

/// 字面常量 → 逐字节缓冲（每词 u32 小端 4 字节——LE 落盘纪律随 golden
/// 一并钉死：若 raster 改为大端写，此处必炸）。
fn words_to_frame(words: &[[u32; 16]]) -> Vec<u8> {
    let mut v = Vec::with_capacity(words.len() * 16 * 4);
    for row in words {
        for &w in row {
            v.extend_from_slice(&w.to_le_bytes());
        }
    }
    v
}

// —— 小场景字面常量（Rgbx，16×16）：
// 命令序 = Clear 红 → FillRect 蓝 (2,2,4,4) → glyph 'A' @像素(8,0)
//          fg 白 bg 黑 → FillRect 绿 (12,4,4,4)（最后画——绿色矩形与
//          'A' 右半交叠，命令序错位即炸）。'A' 位图行：
//          [00,00,00,10,28,44,44,44,FE,44,44,44,44,00,00,00]（MSB=最左列）。
// 期望词由 FONT['A'] 与 pack 公式手工展开钉死（与 src 零共享）。
#[test]
fn golden_small_literal_rgbx() {
    const EXPECT: [[u32; 16]; 16] = [
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x000000FF, 0x000000FF, 0x000000FF, 0x000000FF, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x000000FF, 0x000000FF, 0x000000FF, 0x000000FF, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00000000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x000000FF, 0x000000FF, 0x000000FF, 0x000000FF, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x0000FF00, 0x0000FF00, 0x0000FF00, 0x0000FF00],
        [0x00FF0000, 0x00FF0000, 0x000000FF, 0x000000FF, 0x000000FF, 0x000000FF, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000, 0x0000FF00, 0x0000FF00, 0x0000FF00, 0x0000FF00],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000, 0x0000FF00, 0x0000FF00, 0x0000FF00, 0x0000FF00],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000, 0x0000FF00, 0x0000FF00, 0x0000FF00, 0x0000FF00],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FFFFFF, 0x00FFFFFF, 0x00FFFFFF, 0x00FFFFFF, 0x00FFFFFF, 0x00FFFFFF, 0x00FFFFFF, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000, 0x00000000, 0x00FFFFFF, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
        [0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00FF0000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
    ];
    let d = TextureDesc::new(16, 16, 64, TextureFormat::Rgbx).unwrap();
    let mut fb = vec![0u8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(2, 2, 4, 4), Color { r: 0, g: 0, b: 255 }).unwrap();
    let run = GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: &[GlyphPos { ch: b'A', x: 8, y: 0 }],
        fg: Color { r: 255, g: 255, b: 255 },
        bg: Some(Color { r: 0, g: 0, b: 0 }),
    };
    enc.draw_glyph_run(run).unwrap();
    enc.fill_rect(Rect::new(12, 4, 4, 4), Color { r: 0, g: 255, b: 0 }).unwrap();
    let dl = enc.finish();
    Queue::new()
        .submit(&dl, &mut fb, &d, &PassDescriptor::clear(255, 0, 0))
        .unwrap();
    assert_eq!(fb, words_to_frame(&EXPECT), "小场景 golden 逐字节（命令序/pack/位序/LE 全钉）");
}

// —— Bgrx 通道序 + LE 落盘的字面钉板（8×16 最小帧）：
// Clear (0x12,0x34,0x56) → Bgrx pack = 0x0056_3412（b 在最高内容字节）；
// FillRect 白 2×2 @(3,7)；FillRect 单像素 (0xF0,0x0F,0x3C) @(6,2) →
// 0x003C_0FF0。Rgbx/Bgrx 混淆或大小端写反，此图必炸。
#[test]
fn golden_bgrx_channel_order_literal() {
    const BG: u32 = 0x0056_3412;
    const EXPECT: [[u32; 8]; 16] = [
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, 0x003C0FF0, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, 0x00FFFFFF, 0x00FFFFFF, BG, BG, BG],
        [BG, BG, BG, 0x00FFFFFF, 0x00FFFFFF, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
        [BG, BG, BG, BG, BG, BG, BG, BG],
    ];
    let d = TextureDesc::new(8, 16, 32, TextureFormat::Bgrx).unwrap();
    let mut fb = vec![0u8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(3, 7, 2, 2), Color { r: 255, g: 255, b: 255 }).unwrap();
    enc.fill_rect(Rect::new(6, 2, 1, 1), Color { r: 0xF0, g: 0x0F, b: 0x3C }).unwrap();
    let dl = enc.finish();
    Queue::new()
        .submit(&dl, &mut fb, &d, &PassDescriptor::clear(0x12, 0x34, 0x56))
        .unwrap();
    let mut expect = Vec::with_capacity(16 * 8 * 4);
    for row in EXPECT {
        for w in row {
            expect.extend_from_slice(&w.to_le_bytes());
        }
    }
    assert_eq!(fb, expect, "Bgrx 通道序 + 小端落盘 golden");
}

/// 独立参考算法的像素写口（越界静默跳过 = 裁剪语义的参考面）。
fn ref_put(buf: &mut [u8], w: usize, h: usize, stride: usize, x: i64, y: i64, px: u32) {
    if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
        return;
    }
    let off = (y as usize) * stride + (x as usize) * 4;
    buf[off..off + 4].copy_from_slice(&px.to_le_bytes());
}

// —— 较大场景（64×32）：clear + 矩形阵 + 字符串 + 双 blit（一个重叠
// 对角搬运 + 一个 dst 出界裁剪），与测试内独立参考算法全帧对拍。
// 参考实现不复用任何 src 光栅函数；blit 用「先快照源区、后落盘目的」
// 的 as-if 语义（与 src 的 ptr::copy + 行序实现互证——重叠场景两算法
// 结果必同，任一侧行序/方向错即炸）。
#[test]
fn golden_scene_reference_algorithm() {
    let (w, h, stride) = (64usize, 32usize, 256usize);
    let fmt = TextureFormat::Rgbx;
    let pack = |r: u8, g: u8, b: u8| (r as u32) << 16 | (g as u32) << 8 | b as u32;
    let bg = (0x08u8, 0x10, 0x30);
    let bg_px = pack(bg.0, bg.1, bg.2);

    // —— 被测：gfx 命令路径 ——
    let d = TextureDesc::new(w, h, stride, fmt).unwrap();
    let mut fb = vec![0u8; d.need_bytes()];
    let text = "A#e";
    let positions: Vec<GlyphPos> = text
        .bytes()
        .enumerate()
        .map(|(i, ch)| GlyphPos { ch, x: (i * 8) as i32, y: 0 })
        .collect();
    let run = GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: &positions,
        fg: Color { r: 255, g: 255, b: 255 },
        bg: Some(Color { r: bg.0, g: bg.1, b: bg.2 }),
    };
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(24, 4, 16, 8), Color { r: 255, g: 0, b: 0 }).unwrap();
    enc.draw_glyph_run(run).unwrap();
    enc.fill_rect(Rect::new(0, 16, 8, 16), Color { r: 0, g: 255, b: 0 }).unwrap();
    enc.blit(Rect::new(0, 16, 8, 16), 4, 12).unwrap(); // 对角重叠搬运
    enc.blit(Rect::new(0, 0, 8, 8), 56, 26).unwrap(); // dst 越右下缘裁剪
    let dl = enc.finish();
    Queue::new()
        .submit(&dl, &mut fb, &d, &PassDescriptor::clear(bg.0, bg.1, bg.2))
        .unwrap();

    // —— 参考：独立直写循环 ——
    let mut ex = vec![0u8; d.need_bytes()];
    for y in 0..h {
        for x in 0..w {
            ref_put(&mut ex, w, h, stride, x as i64, y as i64, bg_px); // Clear
        }
    }
    let fill = |ex: &mut Vec<u8>, x0: i64, y0: i64, rw: i64, rh: i64, px: u32| {
        for y in y0..y0 + rh {
            for x in x0..x0 + rw {
                ref_put(ex, w, h, stride, x, y, px);
            }
        }
    };
    fill(&mut ex, 24, 4, 16, 8, pack(255, 0, 0)); // 红条
    for &GlyphPos { ch, x, y } in &positions {
        let g = berry_fbcon::font::glyph(ch).unwrap();
        for (gy, &bits) in g.iter().enumerate() {
            for gx in 0..8usize {
                let on = bits & (0x80 >> gx) != 0;
                ref_put(
                    &mut ex,
                    w,
                    h,
                    stride,
                    x as i64 + gx as i64,
                    y as i64 + gy as i64,
                    if on { 0x00FF_FFFF } else { bg_px },
                );
            }
        }
    }
    fill(&mut ex, 0, 16, 8, 16, pack(0, 255, 0)); // 绿条（blit 源）
    // 参考 blit：快照-落盘（as-if 语义；越界由 ref_put 裁）。
    let ref_blit = |ex: &mut Vec<u8>, sx: i64, sy: i64, rw: i64, rh: i64, dx: i64, dy: i64| {
        let mut snap = vec![0u8; (rw * rh) as usize * 4];
        for j in 0..rh {
            for i in 0..rw {
                // 快照源（源区假定整区界内——与被测命令一致）。
                let off = ((sy + j) as usize) * stride + ((sx + i) as usize) * 4;
                let dst = ((j * rw + i) as usize) * 4;
                snap[dst..dst + 4].copy_from_slice(&ex[off..off + 4]);
            }
        }
        for j in 0..rh {
            for i in 0..rw {
                let v = u32::from_le_bytes(
                    snap[((j * rw + i) as usize) * 4..((j * rw + i) as usize) * 4 + 4]
                        .try_into()
                        .unwrap(),
                );
                ref_put(ex, w, h, stride, dx + i, dy + j, v);
            }
        }
    };
    ref_blit(&mut ex, 0, 16, 8, 16, 4, 12);
    ref_blit(&mut ex, 0, 0, 8, 8, 56, 26);

    assert_eq!(fb, ex, "64×32 场景与独立参考算法逐字节对拍");
}
