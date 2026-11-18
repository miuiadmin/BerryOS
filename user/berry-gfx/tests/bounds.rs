//! 边界组（GUI-2 段①）：零尺寸/负坐标/敌意巨参的裁剪与防溢、dirty
//! 区外哨兵不触、glyph 出缘裁剪、stride padding 铁律（glyph+blit 面）、
//! StoreOp::Discard soft no-op、重叠 blit 全方向（as-if 快照语义对拍）。
//! 哨兵执法贯穿：未写区必须是**原哨兵字节**（预埋值验证——不是「读到
//! 零」这种弱断言）。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun, Rect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontAtlas8x16;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;

const FMT: TextureFormat = TextureFormat::Rgbx;
const SENT: u8 = 0xEE;

fn desc16() -> TextureDesc {
    TextureDesc::new(16, 16, 64, FMT).unwrap()
}

fn px(fb: &[u8], stride: usize, x: i32, y: i32) -> u32 {
    let o = y as usize * stride + x as usize * 4;
    u32::from_le_bytes(fb[o..o + 4].try_into().unwrap())
}

const WHITE: Color = Color { r: 255, g: 255, b: 255 };
const RED: Color = Color { r: 255, g: 0, b: 0 };

// —— 零宽/零高/整矩形在帧外：全部 no-op（哨兵全保）；帧内对照生效 ——
#[test]
fn fill_zero_size_and_fully_outside_noop() {
    let d = desc16();
    // 零尺寸 = 合法空操作（显式语义，不是错误）。
    for rect in [
        Rect::new(0, 0, 0, 16),
        Rect::new(5, 5, 16, 0),
        Rect::new(3, 3, 0, 0),
    ] {
        let mut enc = CommandEncoder::new();
        enc.fill_rect(rect, WHITE).unwrap();
        let dl = enc.finish();
        let mut fb = vec![SENT; d.need_bytes()];
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
        assert_eq!(fb, vec![SENT; d.need_bytes()], "rect={rect:?} 应 no-op");
    }
    // 整矩形在帧外（四向）同样 no-op——负原点是合法记录，回放期裁掉。
    for rect in [
        Rect::new(-8, 0, 4, 16),  // 全左
        Rect::new(0, -8, 4, 4),   // 全上
        Rect::new(16, 0, 4, 16),  // 全右（恰贴右缘外）
        Rect::new(0, 16, 4, 16),  // 全下
    ] {
        let mut enc = CommandEncoder::new();
        enc.fill_rect(rect, WHITE).unwrap();
        let dl = enc.finish();
        let mut fb = vec![SENT; d.need_bytes()];
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
        assert_eq!(fb, vec![SENT; d.need_bytes()], "rect={rect:?} 帧外 no-op");
    }
    // 对照：帧内矩形确实写（防「全 no-op 假绿」）。
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(7, 7, 2, 2), WHITE).unwrap();
    let dl = enc.finish();
    let mut fb = vec![SENT; d.need_bytes()];
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    assert_eq!(px(&fb, 64, 7, 7), 0x00FF_FFFF);
}

// —— 敌意巨参不炸（checked/饱和防御执法）：i32 域极值矩形与巨坐标线 ——
//
// 咬住什么：`x+w`/`y+h` 的 i64 中转、intersect 的 try_from 兜底、
// put_px 的 usize::try_from 负坐标挡板——任一处退回 i32 直算，
// i32::MIN→MAX 量级的和值/中转算术即溢出（debug 构建 panic = 测试炸；
// release 构建回绕成「覆盖全帧」类静默错 = 断言炸）。
//
// 线侧注记：`line()` 已带窗有界（审查 R2-F1——迭代上界 = 帧带宽+2，
// i32::MIN→MAX 全跨度三向专测在 `lines.rs`）；此处保留巨坐标短跨度
// 的 put_px 挡板路径（负巨原点整线全裁 no-op）。
#[test]
fn fill_hostile_coords_no_panic() {
    let d = desc16();
    // 横跨 ±2^31 的巨矩形 ∩ 帧 = 整帧（可裁剪的巨参照常工作）。
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(i32::MIN, i32::MIN, i32::MAX, i32::MAX), RED)
        .unwrap();
    // x+w「直觉溢出」形态：x=2e9、w=2e9（i32 直加回绕成负——i64 中转
    // 后与帧相离 = no-op，绝不因回绕变「覆盖全帧」）。
    enc.fill_rect(Rect::new(2_000_000_000, 0, 2_000_000_000, 8), WHITE)
        .unwrap();
    // 负巨原点 + 巨宽：与帧交叠 = 整帧。
    enc.fill_rect(Rect::new(-2_000_000_000, -2_000_000_000, i32::MAX, i32::MAX), RED)
        .unwrap();
    // 线端点巨坐标（负巨原点、短跨度）：整线在帧外（x 恒负巨）——
    // 带窗实现走「窗空早退」，巨坐标经挡板不 panic、全裁 no-op。
    enc.draw_line(-2_000_000_000, 5, -2_000_000_000 + 20, 5, WHITE)
        .unwrap();
    // 跨帧线（两端出界）：行 8 整行照画（39 次迭代，出界点逐点裁）。
    enc.draw_line(-8, 8, 30, 8, WHITE).unwrap();
    let dl = enc.finish();
    let mut fb = vec![SENT; d.need_bytes()];
    Queue::new()
        .submit(&dl, &mut fb, &d, &PassDescriptor::load())
        .unwrap();
    for y in 0..16 {
        for x in 0..16 {
            // 命令序：巨帧红（×2，中间的白巨矩形相离 no-op）→ 帧外巨线
            // no-op → 行 8 白线最后画。
            let expect = if y == 8 { 0x00FF_FFFF } else { 0x00FF_0000 };
            assert_eq!(px(&fb, 64, x, y), expect, "@({x},{y})");
        }
    }
}

// —— dirty 区外 = 别人的画面：Clear/命令/glyph 全部只进 dirty ——
#[test]
fn dirty_outside_untouched_sentinels() {
    let d = desc16();
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(0, 0, 16, 16), WHITE).unwrap(); // 全域命令
    let run = GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: &[GlyphPos { ch: b'A', x: 2, y: 2 }], // 蜂格横跨 dirty 边界
        fg: RED,
        bg: None, // 透明底：非着墨位不写（白底穿透）
    };
    enc.draw_glyph_run(run).unwrap();
    let dl = enc.finish();
    let mut fb = vec![SENT; d.need_bytes()];
    Queue::new()
        .submit(
            &dl,
            &mut fb,
            &d,
            &PassDescriptor::new(
                LoadOp::Clear { r: 0, g: 0, b: 255 },
                StoreOp::Store,
                Some(Rect::new(4, 4, 8, 8)),
            ),
        )
        .unwrap();
    // 期望：dirty [4,12)² 内 = 白底 + 'A' 着墨红（bg=None 零位穿透白）；
    // 区外 = 原哨兵 0xEE_EEEEEE（Clear 蓝也不铺区外）。
    let g = berry_fbcon::font::glyph(b'A').unwrap();
    for y in 0..16 {
        for x in 0..16 {
            let inside = (4..12).contains(&x) && (4..12).contains(&y);
            let expect = if !inside {
                0xEEEE_EEEE
            } else {
                let (cx, cy) = (x - 2, y - 2);
                let ink = (0..8).contains(&cx) && (0..16).contains(&cy) && g[cy as usize] & (0x80 >> cx) != 0;
                if ink { 0x00FF_0000 } else { 0x00FF_FFFF }
            };
            assert_eq!(px(&fb, 64, x, y), expect, "@({x},{y})");
        }
    }
}

// —— dirty 与帧界交叠为空 = 本 pass 全 no-op（合法——空脏区非错误）——
#[test]
fn dirty_empty_intersection_full_noop() {
    let d = desc16();
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(0, 0, 16, 16), WHITE).unwrap();
    enc.present(Rect::new(0, 0, 16, 16)).unwrap();
    let dl = enc.finish();
    for dirty in [
        Rect::new(20, 20, 8, 8),   // 右下角外
        Rect::new(-16, -16, 8, 8), // 左上角外
        Rect::new(16, 16, 100, 100), // 贴角外 + 巨参
    ] {
        let mut fb = vec![SENT; d.need_bytes()];
        Queue::new()
            .submit(
                &dl,
                &mut fb,
                &d,
                &PassDescriptor::new(LoadOp::Clear { r: 255, g: 0, b: 0 }, StoreOp::Store, Some(dirty)),
            )
            .unwrap(); // Err 才是错——空交叠是合法 no-op
        assert_eq!(fb, vec![SENT; d.need_bytes()], "dirty={dirty:?} 空交叠应全 no-op");
    }
}

// —— glyph 出缘裁剪：右缘只画可见列（不回绕）、下缘只画可见行、全外 no-op ——
#[test]
fn glyph_clipped_right_bottom_edges() {
    let d = desc16();
    let run = GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: &[
            GlyphPos { ch: b'A', x: 12, y: 0 },  // 右缘：仅 gx 0..4 可见
            GlyphPos { ch: b'A', x: 4, y: 14 },  // 下缘：仅 gy 0..2 可见
            GlyphPos { ch: b'A', x: 24, y: 0 },  // 全外（右）
            GlyphPos { ch: b'A', x: 0, y: 20 },  // 全外（下）
            GlyphPos { ch: b'A', x: -9, y: 0 },  // 全外（左：8 宽全负）
        ],
        fg: WHITE,
        bg: None, // 透明底：未写区必是哨兵
    };
    let mut enc = CommandEncoder::new();
    enc.draw_glyph_run(run).unwrap();
    let dl = enc.finish();
    let mut fb = vec![SENT; d.need_bytes()];
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    let g = berry_fbcon::font::glyph(b'A').unwrap();
    for y in 0..16 {
        for x in 0..16 {
            // 期望 = 哨兵，除非命中两可见字形片：
            // ① 右缘片 (12+gx, gy) gx<4（x≥16 的列被裁，不回绕到行首）；
            // ② 下缘片 (4+gx, 14+gy) gy<2。
            let ink1 = x >= 12 && y < 16 && g[y as usize] & (0x80 >> (x - 12)) != 0;
            let ink2 = y >= 14 && x >= 4 && x < 12 && g[(y - 14) as usize] & (0x80 >> (x - 4)) != 0;
            let expect = if ink1 || ink2 { 0x00FF_FFFF } else { 0xEEEE_EEEE };
            assert_eq!(px(&fb, 64, x, y), expect, "@({x},{y})");
        }
    }
}

// —— stride padding 铁律的 glyph/blit 面：padding 列绝不写 ——
#[test]
fn stride_padding_blit_and_glyph_never_written() {
    // w=16/h=32/stride=80：每行 64B 内容 + 16B padding（末行缓冲不含
    // padding——need = 80*31+64，y=31 无 padding 可检）。
    let d = TextureDesc::new(16, 32, 80, FMT).unwrap();
    let mut enc = CommandEncoder::new();
    let run = GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: &[GlyphPos { ch: b'H', x: 8, y: 16 }],
        fg: Color { r: 0x11, g: 0x22, b: 0x33 },
        bg: Some(Color { r: 0, g: 0, b: 0 }),
    };
    enc.draw_glyph_run(run).unwrap();
    enc.blit(Rect::new(0, 16, 16, 16), 0, 0).unwrap(); // 整行宽 blit
    let dl = enc.finish();
    let mut fb = vec![SENT; d.need_bytes()];
    Queue::new()
        .submit(&dl, &mut fb, &d, &PassDescriptor::clear(255, 255, 255))
        .unwrap();
    // padding 全保哨兵（blit 若按「整 stride 行」搬运会把 padding 也搬——
    // 16 行 × 16B 全炸）。
    for y in 0..31 {
        assert_eq!(&fb[y * 80 + 64..y * 80 + 80], &[SENT; 16], "padding y={y}");
    }
    // 内容面真写了：Clear 白 + H 着墨 + blit 复制（H 在 y=16 与 blit 后
    // 的 y=0 双位置出现——H row7=0x7C → gx1 着墨 @x=9（gx0 属设计留白））。
    assert_eq!(px(&fb, 80, 0, 0), 0x00FF_FFFF, "blit 目标行白底");
    assert_eq!(px(&fb, 80, 9, 7), 0x0011_2233, "H 着墨 blit 到 y=7");
    assert_eq!(px(&fb, 80, 9, 23), 0x0011_2233, "H 着墨原位 y=23");
}

// —— StoreOp::Discard soft no-op：与 Store 逐字节同产物 ——
#[test]
fn store_discard_is_noop_equals_store() {
    let d = desc16();
    let render = |store: StoreOp| -> Vec<u8> {
        let mut enc = CommandEncoder::new();
        enc.fill_rect(Rect::new(2, 2, 8, 8), RED).unwrap();
        enc.draw_line(0, 15, 15, 0, WHITE).unwrap();
        let run = GlyphRun {
            atlas: &FontAtlas8x16,
            glyphs: &[GlyphPos { ch: b'Q', x: 8, y: 0 }],
            fg: WHITE,
            bg: Some(Color { r: 0, g: 0, b: 0 }),
        };
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        let mut fb = vec![SENT; d.need_bytes()];
        Queue::new()
            .submit(
                &dl,
                &mut fb,
                &d,
                &PassDescriptor::new(LoadOp::Clear { r: 0, g: 0, b: 255 }, store, None),
            )
            .unwrap();
        fb
    };
    let store = render(StoreOp::Store);
    let discard = render(StoreOp::Discard);
    assert_eq!(store, discard, "soft 后端 Discard = no-op（内容已在调用方缓冲）");
    // 非恒真证明：产物确实非哨兵（Clear 蓝在 (0,0)——避开 (2,2,8,8) 红块、
    // 对角线 x+y=15 与 'Q' 蜂格 x∈[8,16) 的三点未覆盖区）。
    assert_ne!(store, vec![SENT; d.need_bytes()]);
    assert_eq!(px(&store, 64, 0, 0), 0x0000_00FF, "Clear 蓝在");
}

// —— 重叠 blit 全方向：as-if 快照语义对拍（行序/方向任一错即炸）——
//
// 预填逐像素异色图案（测试直写，不经 gfx），单一 blit 命令回放，与
// 「先快照源区、后落盘目的区」的参考语义全帧对拍。六个方向病理：
// 下移（dst 在 src 下，垂直重叠）、上移、右移（水平重叠）、左移、
// 对角（双向同时重叠）、dst 越缘（裁剪 × 重叠无组合面照验）。
#[test]
fn blit_overlaps_all_directions() {
    let d = desc16();
    let stride = 64usize;
    let pattern = |x: usize, y: usize| -> u32 {
        (x as u32 * 3 + 1) << 16 | (y as u32 * 5 + 2) << 8 | ((x ^ y) as u32 & 0xFF)
    };
    let cases = [
        (0i32, 0i32, 8i32, 8i32, 0i32, 4i32),   // 下移（垂直重叠）
        (0, 8, 8, 8, 0, 4),                      // 上移（垂直重叠）
        (0, 0, 8, 8, 4, 0),                      // 右移（水平重叠）
        (8, 0, 8, 8, 4, 0),                      // 左移（水平重叠）
        (2, 2, 8, 8, 6, 6),                      // 对角（双向重叠）
        (0, 0, 8, 8, 10, 10),                    // dst 越右下缘（6×6 可见）
    ];
    for (sx, sy, rw, rh, dx, dy) in cases {
        // 被测：预填图案 + 单 blit。
        let mut fb = vec![0u8; d.need_bytes()];
        for y in 0..16usize {
            for x in 0..16usize {
                fb[y * stride + x * 4..y * stride + x * 4 + 4]
                    .copy_from_slice(&pattern(x, y).to_le_bytes());
            }
        }
        let mut enc = CommandEncoder::new();
        enc.blit(Rect::new(sx, sy, rw, rh), dx, dy).unwrap();
        let dl = enc.finish();
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();

        // 参考：快照-落盘 as-if 语义（越界像素跳过 = 裁剪对偶）。
        let mut ex = vec![0u8; d.need_bytes()];
        for y in 0..16usize {
            for x in 0..16usize {
                ex[y * stride + x * 4..y * stride + x * 4 + 4]
                    .copy_from_slice(&pattern(x, y).to_le_bytes());
            }
        }
        for j in 0..rh {
            for i in 0..rw {
                let (sxi, syi) = (sx + i, sy + j);
                if !(0..16).contains(&sxi) || !(0..16).contains(&syi) {
                    continue;
                }
                let v = pattern(sxi as usize, syi as usize);
                let (dxi, dyi) = (dx + i, dy + j);
                if !(0..16).contains(&dxi) || !(0..16).contains(&dyi) {
                    continue;
                }
                let off = dyi as usize * stride + dxi as usize * 4;
                ex[off..off + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        assert_eq!(fb, ex, "blit ({sx},{sy},{rw}x{rh})→({dx},{dy})");
    }
}

// —— 重叠字形 = 后写胜（paint 序语义；审查 L5 收口）——
//
// v1 无合成/混合：后绘制的字形原样覆盖先写像素。两形态全帧断言：
// ① 双 run 同位（红 '#' → 绿 '#'）：着墨集全绿——若回放序倒转，
//    全红即炸；② 单 run 切片 [#, A] 同位、bg=Some：终帧 = 'A' 的
//    蜂格（A 后画，其墨位铺 fg、零位铺 bg，'#' 被整体盖掉）——若
//    切片序不作绘制序（'#' 后画），终帧 = '#' 蜂格即炸。'#' 与 'A'
//    位图不同是前提（font 常识位图，断言前先互证非同图防恒真）。
#[test]
fn glyph_overlap_later_wins() {
    let g_hash = berry_fbcon::font::glyph(b'#').unwrap();
    let g_a = berry_fbcon::font::glyph(b'A').unwrap();
    assert_ne!(g_hash, g_a, "两字形位图须不同（否则②恒真）");
    let d = desc16();

    // ① 双 run：红 '#' → 绿 '#'，同位 (0,0)，透明底（哨兵即未写面）。
    let mut enc = CommandEncoder::new();
    let run_red = GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: &[GlyphPos { ch: b'#', x: 0, y: 0 }],
        fg: RED,
        bg: None,
    };
    let run_green = GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: &[GlyphPos { ch: b'#', x: 0, y: 0 }],
        fg: Color { r: 0, g: 255, b: 0 },
        bg: None,
    };
    enc.draw_glyph_run(run_red).unwrap();
    enc.draw_glyph_run(run_green).unwrap();
    let dl = enc.finish();
    let mut fb = vec![SENT; d.need_bytes()];
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    for y in 0..16 {
        for x in 0..16 {
            let ink = x < 8 && g_hash[y as usize] & (0x80 >> x) != 0;
            let expect = if ink { 0x0000_FF00 } else { 0xEEEE_EEEE };
            assert_eq!(px(&fb, 64, x, y), expect, "双 run @({x},{y})（绿终帧）");
        }
    }

    // ② 单 run 切片序 [#, A] 同位、bg=Some 蓝：终帧 = 'A' 蜂格。
    let mut enc = CommandEncoder::new();
    let run = GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: &[
            GlyphPos { ch: b'#', x: 0, y: 0 },
            GlyphPos { ch: b'A', x: 0, y: 0 },
        ],
        fg: WHITE,
        bg: Some(Color { r: 0, g: 0, b: 255 }),
    };
    enc.draw_glyph_run(run).unwrap();
    let dl = enc.finish();
    let mut fb = vec![SENT; d.need_bytes()];
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    for y in 0..16 {
        for x in 0..16 {
            let expect = if x < 8 {
                if g_a[y as usize] & (0x80 >> x) != 0 {
                    0x00FF_FFFF
                } else {
                    0x0000_00FF
                }
            } else {
                0xEEEE_EEEE
            };
            assert_eq!(px(&fb, 64, x, y), expect, "单 run @({x},{y})（A 终帧）");
        }
    }
}
