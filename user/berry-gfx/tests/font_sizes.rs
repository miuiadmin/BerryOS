//! 多字号档位组（段② `font` 三档）：`FontSize`/三个静态图集的**档位几何
//! 钉板**（cell 几何 ↔ size() ↔ atlas() 三面互为单一事实源）+ **全域域检**
//! （0..=255 全字节面：32..=126 域内恰长、域外 None 跨档同律）+ **三档
//! 同串渲染共识**（同一字符串三档各渲染一次——1x 每像素在 2x/3x 帧上的
//! N×N 块同色：NN 放大的渲染面执法，比「偶数行/列抽验」强的全量块对拍）。
//!
//! 与 `tests/fonts.rs` 的分立价值：那边从 1x 表**位级重推**对拍放大档
//! 生成产物（双实现共识——生成器侧之外的第二条独立执法线）；这边验
//! **档位元数据一致性**与**渲染路径的三档共识**（位图正确 ≠ 档位映射
//! 正确 ≠ 渲染组合正确——三层各有一条会独立漂移的缝）。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::{FontAtlas8x16, FontAtlas16x32, FontAtlas24x48, FontSize, GlyphAtlas};
use berry_gfx::pass::PassDescriptor;
use berry_gfx::queue::Queue;

// —— 档位几何钉板：FontSize cell ↔ 静态实现 size() ↔ atlas() 三面一致 ——
#[test]
fn size_atlas_geometry_pinned() {
    let cases = [
        (FontSize::S8x16, 8usize, 16usize),
        (FontSize::S16x32, 16, 32),
        (FontSize::S24x48, 24, 48),
    ];
    for (size, w, h) in cases {
        assert_eq!((size.cell_w(), size.cell_h()), (w, h), "{size:?} cell 几何");
        // atlas() 直取的图集与档位几何一致（档位 ↔ 图集互为单一事实源）。
        let a = size.atlas();
        assert_eq!((a.cell_w(), a.cell_h()), (w, h), "{size:?} atlas() 几何");
        // 域内取样一形（整 95 形的恰长在下一测全量钉）。
        let g = a.glyph(b'#').expect("域内必有");
        assert_eq!((g.w, g.h), (w, h));
    }
    // 三个静态实现的 size() 自述档位正确（实现 ↔ 档位映射不串档）。
    assert_eq!(FontAtlas8x16.size(), FontSize::S8x16);
    assert_eq!(FontAtlas16x32.size(), FontSize::S16x32);
    assert_eq!(FontAtlas24x48.size(), FontSize::S24x48);
    // size() 的 cell 几何与图集实际 cell 几何一致（自述与行为对得上）。
    for (atlas_cell, size) in [
        ((FontAtlas8x16.cell_w(), FontAtlas8x16.cell_h()), FontAtlas8x16.size()),
        ((FontAtlas16x32.cell_w(), FontAtlas16x32.cell_h()), FontAtlas16x32.size()),
        ((FontAtlas24x48.cell_w(), FontAtlas24x48.cell_h()), FontAtlas24x48.size()),
    ] {
        assert_eq!(atlas_cell, (size.cell_w(), size.cell_h()), "size() 与 cell() 同几何");
    }
}

// —— 全域域检（0..=255 全字节面）：域内 95 形恰长、域外 None 跨档同律 ——
//
// 咬住什么：放大档图集的索引若偏一（ch-32 vs ch-31 类漂移），只有全域
// 扫描能同时抓「域内形状串位」与「域边界错收」两缝；恰长 = 行字节数
// （(cw+7)/8 = 1/2/3）× 行数（16/32/48）= 16/64/144 字节每形。
#[test]
fn atlas_domain_sweep_and_exact_row_bytes() {
    for size in [FontSize::S8x16, FontSize::S16x32, FontSize::S24x48] {
        let a = size.atlas();
        let (cw, chh) = (a.cell_w(), a.cell_h());
        let row_bytes = (cw + 7) / 8;
        for ch in 0u8..=255 {
            match a.glyph(ch) {
                Some(g) => {
                    assert!((32..=126).contains(&ch), "{size:?} 域外错收 ch={ch:#x}");
                    assert_eq!((g.w, g.h), (cw, chh), "{size:?} ch={ch} 几何 = 档位 cell");
                    assert_eq!(g.rows.len(), chh * row_bytes, "{size:?} ch={ch} 恰长");
                }
                None => {
                    assert!(!(32..=126).contains(&ch), "{size:?} 域内错拒 ch={ch:#x}");
                }
            }
        }
    }
}

// —— 三档同串渲染共识：1x 每像素的 N×N 块在 2x/3x 帧上同色 ——
//
// 同一串 "OK!" 三档各渲染一次（fg/bg 双色非灰——pack 通道序错可分辨）；
// 1x 帧 (x,y) 的颜色必须等于 N 档帧 (Nx..N(x+1), Ny..N(y+1)) 整块颜色
// （NN 放大的渲染面定义）。此断言全量覆盖 24×16 个 1x 像素位的 N×N 块，
// 同时覆盖「1x 着墨位的 N×N 块全 fg / 空白位全 bg」的字面契约。
#[test]
fn three_size_string_render_consensus() {
    let text = b"OK!";
    let fg = Color { r: 0xAA, g: 0xBB, b: 0xCC };
    let bg = Color { r: 0x11, g: 0x22, b: 0x33 };

    let render = |size: FontSize| -> (Vec<u8>, usize) {
        let (cw, chh) = (size.cell_w(), size.cell_h());
        let (w, stride) = (cw * text.len(), cw * text.len() * 4);
        let d = TextureDesc::new(w, chh, stride, TextureFormat::Rgbx).unwrap();
        let positions: Vec<GlyphPos> = text
            .iter()
            .enumerate()
            .map(|(i, &ch)| GlyphPos { ch, x: (i * cw) as i32, y: 0 })
            .collect();
        let run = GlyphRun { atlas: size.atlas(), glyphs: &positions, fg, bg: Some(bg) };
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        let mut fb = vec![0u8; d.need_bytes()];
        Queue::new()
            .submit(&dl, &mut fb, &d, &PassDescriptor::clear(bg.r, bg.g, bg.b))
            .unwrap();
        (fb, stride)
    };

    let (f1, s1) = render(FontSize::S8x16);
    for (n, size) in [(2usize, FontSize::S16x32), (3usize, FontSize::S24x48)] {
        let (fnn, sn) = render(size);
        for y in 0..16usize {
            for x in 0..(8 * text.len()) {
                let base = u32::from_le_bytes(f1[y * s1 + x * 4..][..4].try_into().unwrap());
                for dy in 0..n {
                    for dx in 0..n {
                        let (gx, gy) = (n * x + dx, n * y + dy);
                        let got =
                            u32::from_le_bytes(fnn[gy * sn + gx * 4..][..4].try_into().unwrap());
                        assert_eq!(
                            got, base,
                            "{size:?} 1x@({x},{y}) 的 {n}×{n} 块 @({gx},{gy}) 漂移"
                        );
                    }
                }
            }
        }
    }
}
