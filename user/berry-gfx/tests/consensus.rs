//! 共识组（GUI-2 段①核心）：同串同屏，gfx 路径（CommandEncoder 录
//! DrawGlyphRun + Queue::submit）↔ fbcon 路径（Fbcon 蜂格 put_str/
//! put_char），两块等尺寸缓冲**逐字节对拍**——两套独立代码、像素必同
//! （ADR #51 机器守卫先例；plan「像素共识契约」行）。
//!
//! 双路径共识的执法面：
//! - 双格式（Rgbx/Bgrx）× 多字符串 × 行进位（蜂格坐标换算 col*8/row*16）；
//! - stride padding 场景（两侧 padding 字节同为哨兵初值——padding 铁律
//!   双侧共识）；
//! - 回绕/换行/缺字形（UTF-8 引导字节 → 空格蜂格）；
//! - 守卫真伪自检（`consensus_guards_are_real`）：故意构造「pack 通道序
//!   翻转」「glyph 位序反转」两种破坏形态，断言它们与共识输出**必不相等
//!   **——证明本组 assert_eq 一旦 src 出现这两种回归形态必炸（守卫咬
//!   得动，不是恒真摆设）。

use berry_fbcon::render::{Fbcon, PixelFormat};
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontAtlas8x16;
use berry_gfx::pass::PassDescriptor;
use berry_gfx::queue::Queue;

type Rgb = (u8, u8, u8);

/// fbcon 路径渲染：哨兵初值缓冲 + 闭包驱动蜂格写入（put_str/put_char/
/// clear 全走 `Fbcon::put_px` 单口）。
fn fbcon_render(w: usize, h: usize, stride: usize, fmt: PixelFormat, f: impl FnOnce(&mut Fbcon)) -> Vec<u8> {
    let mut fb = vec![0xEEu8; stride * (h - 1) + w * 4];
    let mut c = Fbcon::new(&mut fb, w, h, stride, fmt).expect("几何合法");
    f(&mut c);
    fb
}

/// gfx 路径渲染：同一几何 + pass（Clear 走 load_op）+ run 序列。
///
/// 形态注记：`CommandEncoder<'a>` 的 `'a` 钉住 `GlyphRun` 的 atlas/glyphs
/// 借用——glyph 定位数据必须在 encoder 之前于调用方作用域备好、以切片
/// 传入（闭包内造临时 `Vec<GlyphPos>` 会被 'a 判死刑，E0597/E0716）。
fn gfx_render_runs(
    w: usize,
    h: usize,
    stride: usize,
    fmt: TextureFormat,
    clear: Rgb,
    runs: &[(&[GlyphPos], Rgb, Rgb)],
) -> Vec<u8> {
    let desc = TextureDesc::new(w, h, stride, fmt).unwrap();
    let mut fb = vec![0xEEu8; desc.need_bytes()];
    let mut enc = CommandEncoder::new();
    for &(glyphs, fg, bg) in runs {
        enc.draw_glyph_run(run_of(glyphs, fg, bg)).unwrap();
    }
    let dl = enc.finish();
    Queue::new()
        .submit(&dl, &mut fb, &desc, &PassDescriptor::clear(clear.0, clear.1, clear.2))
        .unwrap();
    fb
}

/// shape 段（文字三段第一段）测试侧实现：把 `&str` 按字节流复刻 fbcon
/// `put_str` 光标规则（'\n' → 换行；行尾回绕；缺字形字节照占一蜂格——
/// raster 侧按全零位图 = 空格语义），产出像素坐标 glyph 定位。蜂格
/// 坐标换算（行进位）在此钉死：x = col*8、y = row*16。
fn shape_str(s: &str, cols: usize) -> Vec<GlyphPos> {
    let mut out = Vec::new();
    let (mut col, mut row) = (0usize, 0usize);
    for &b in s.as_bytes() {
        if b == b'\n' {
            col = 0;
            row += 1;
            continue;
        }
        if col >= cols {
            col = 0;
            row += 1;
        }
        out.push(GlyphPos { ch: b, x: (col * 8) as i32, y: (row * 16) as i32 });
        col += 1;
    }
    out
}

fn run_of(positions: &[GlyphPos], fg: Rgb, bg: Rgb) -> GlyphRun<'_> {
    GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: positions,
        fg: Color { r: fg.0, g: fg.1, b: fg.2 },
        bg: Some(Color { r: bg.0, g: bg.1, b: bg.2 }),
    }
}

// —— 基础共识：put_str 单串 + Rgbx + 黑底白字 ——
#[test]
fn consensus_put_str_rgbx() {
    // h=48（3 蜂格行）：22 字节 = 8+8+5 → 两轮回绕、第三行不触底。
    // （fbcon put_str 越底会滚屏——滚屏属 fbcon 光标层策略，gfx 文字
    // 三段制的 shape 段是调用方职责，共识测试几何上避开滚屏面。）
    let (w, h, stride) = (64usize, 48usize, 256usize);
    let text = "Hello, BerryOS GUI-2!";
    let a = fbcon_render(w, h, stride, PixelFormat::Rgbx, |c| {
        c.clear((0, 0, 0));
        c.put_str(text, (255, 255, 255), (0, 0, 0));
    });
    let positions = shape_str(text, w / 8);
    let b = gfx_render_runs(w, h, stride, TextureFormat::Rgbx, (0, 0, 0), &[(&positions, (255, 255, 255), (0, 0, 0))]);
    assert_eq!(a, b, "gfx glyph 路径与 fbcon put_str 逐字节共识（Rgbx）");
}

// —— Bgrx 双格式共识 + 多串多色（put_char 显式蜂格坐标）——
#[test]
fn consensus_put_char_bgrx_multicolor() {
    let (w, h, stride) = (32usize, 32usize, 128usize); // 4×2 蜂格
    let cells = [
        (0usize, 0usize, b'A', (0x11u8, 0x22, 0x33), (0x44u8, 0x55, 0x66)),
        (1, 0, b'#', (255, 0, 0), (0, 255, 0)),
        (3, 0, b'z', (0, 0, 255), (255, 255, 0)),
        (0, 1, b'~', (0xAA, 0xBB, 0xCC), (0x01, 0x02, 0x03)),
        (2, 1, 0x7f, (255, 255, 255), (0x77, 0x00, 0x99)), // DEL = 缺字形 → 空格
    ];
    let a = fbcon_render(w, h, stride, PixelFormat::Bgrx, |c| {
        c.clear((9, 8, 7));
        for &(col, row, ch, fg, bg) in &cells {
            assert!(c.put_char(col, row, ch, fg, bg), "蜂格 ({col},{row}) 在界内");
        }
    });
    // 逐字形独立 run（每格自带 fg/bg——put_char 逐调用配色的对偶；蜂格
    // 坐标换算内联：x = col*8、y = row*16）。定位数据先备好再进 encoder。
    let per_cell: Vec<[GlyphPos; 1]> = cells
        .iter()
        .map(|&(col, row, ch, _, _)| [GlyphPos { ch, x: (col * 8) as i32, y: (row * 16) as i32 }])
        .collect();
    let runs: Vec<(&[GlyphPos], Rgb, Rgb)> = per_cell
        .iter()
        .zip(cells.iter())
        .map(|(one, &(_, _, _, fg, bg))| (&one[..], fg, bg))
        .collect();
    let b = gfx_render_runs(w, h, stride, TextureFormat::Bgrx, (9, 8, 7), &runs);
    assert_eq!(a, b, "gfx 与 fbcon put_char 逐字节共识（Bgrx/多色/缺字形）");
}

// —— stride padding 场景：内容列共识 + padding 字节双侧同哨兵不触 ——
#[test]
fn consensus_with_stride_padding() {
    // w=32（cols=4）h=32（rows=2）stride=160：行内容 128B + 32B padding。
    let (w, h, stride) = (32usize, 32usize, 160usize);
    let text = "Hi";
    let a = fbcon_render(w, h, stride, PixelFormat::Rgbx, |c| {
        c.clear((0x10, 0x20, 0x30));
        c.put_str(text, (255, 255, 255), (0x10, 0x20, 0x30));
    });
    let positions = shape_str(text, w / 8);
    let b = gfx_render_runs(w, h, stride, TextureFormat::Rgbx, (0x10, 0x20, 0x30), &[(&positions, (255, 255, 255), (0x10, 0x20, 0x30))]);
    assert_eq!(a, b, "含 padding 逐字节共识（padding 铁律双侧同执法）");
    // padding 共识的面证：整块缓冲以 0xEE 起步，双方都未写任何 padding
    // 字节——若任一侧踩 padding，上方 vec 级对拍即炸（非只查内容列）。
    for y in 0..h - 1 {
        assert_eq!(&a[y * stride + w * 4..(y + 1) * stride], &[0xEE; 32], "fbcon padding y={y}");
    }
}

// —— 回绕/换行/UTF-8 缺字形字节：put_str 光标机 ↔ shape 复刻 ——
#[test]
fn consensus_wrap_newline_and_missing_glyphs() {
    let (w, h, stride) = (64usize, 32usize, 256usize); // 8×2 蜂格
    // "AB\nCD\xC3E"：显式换行 + 缺字形字节（0xC3 = UTF-8 引导字节，
    // fbcon put_str 逐字节占一空格蜂格；gfx 图集 glyph(0xC3)=None →
    // 全零位图 = 同一空格语义）。
    for text in ["AB\nCD\u{00C3}E", "WXYZ1234z"] {
        // 后者 9 字节：恰满一行后回绕（z 落行 1 首）。
        let a = fbcon_render(w, h, stride, PixelFormat::Rgbx, |c| {
            c.clear((0, 0, 0));
            c.put_str(text, (255, 255, 255), (0, 0, 0));
        });
        let positions = shape_str(text, w / 8);
        let b = gfx_render_runs(w, h, stride, TextureFormat::Rgbx, (0, 0, 0), &[(&positions, (255, 255, 255), (0, 0, 0))]);
        assert_eq!(a, b, "text={text:?}");
    }
}

// —— 守卫真伪自检：两种典型回归形态必被共识对拍咬住 ——
//
// 本组的执法者是 `assert_eq!(fbcon_out, gfx_out)`——守卫要「真」，必须
// 证明:若 src 出现下述回归，两输出必不相等（否则对拍恒绿 = 摆设）。
// 这里在测试内**手工构造**两种破坏产物，断言它们与真共识输出不等：
//
// 1. pack 通道序翻转（Rgbx 的 r/b 高低位互换——`pack` 写反或 `from_raw`
//    误猜格式的产出形态）：把 gfx 输出每像素 LE 字节 [b,g,r,0] 的
//    byte0↔byte2 互换 = 恰好变成 Bgrx pack 值。用非灰彩色（红/白），
//    翻转后必不同——若 raster 的 pack 回归成此形态，本组对拍必炸。
// 2. glyph 位序反转（`0x80 >> gx` ↔ `1 << gx`）：产出 = 每蜂格行 8 像素
//    水平镜像。选 'A' 与 '#'（位图含不对称行 0x28/0xFE/0x7C——镜像 ≠
//    原像），镜像产物必不同于真输出——若 raster 的 gx 位序回归，对拍
//    必炸。
#[test]
fn consensus_guards_are_real() {
    let (w, h, stride) = (16usize, 16usize, 64usize); // 2×1 蜂格
    let fg = (255u8, 0, 0); // 非灰彩色：pack 翻转可区分
    let bg = (0x10u8, 0x20, 0x30);
    let positions = [
        GlyphPos { ch: b'A', x: 0, y: 0 },
        GlyphPos { ch: b'#', x: 8, y: 0 },
    ];
    let a = fbcon_render(w, h, stride, PixelFormat::Rgbx, |c| {
        c.clear(bg);
        c.put_char(0, 0, b'A', fg, bg);
        c.put_char(1, 0, b'#', fg, bg);
    });
    let b = gfx_render_runs(w, h, stride, TextureFormat::Rgbx, bg, &[(&positions, fg, bg)]);
    assert_eq!(a, b, "真共识先行（此行绿 = 双路径像素一致）");

    // 破坏形态 1：pack 通道序翻转。
    let mut pack_flipped = b.clone();
    for px_bytes in pack_flipped.chunks_exact_mut(4) {
        px_bytes.swap(0, 2);
    }
    assert_ne!(
        pack_flipped, a,
        "pack 翻转产物 ≠ fbcon 基准——共识对拍对 pack 回归有咬合力"
    );

    // 破坏形态 2：glyph 位序反转（每蜂格行 8 像素水平镜像）。
    let mut mirrored = a.clone();
    for y in 0..h {
        for x0 in (0..w).step_by(8) {
            for i in 0..4 {
                let (l, r) = (y * stride + (x0 + i) * 4, y * stride + (x0 + 7 - i) * 4);
                for k in 0..4 {
                    mirrored.swap(l + k, r + k);
                }
            }
        }
    }
    assert_ne!(
        mirrored, b,
        "位序镜像产物 ≠ gfx 输出——共识对拍对 gx 位序回归有咬合力"
    );
    // 顺手证两破坏形态互不相同（不是殊途同归的同一张错图）。
    assert_ne!(pack_flipped, mirrored);
}
