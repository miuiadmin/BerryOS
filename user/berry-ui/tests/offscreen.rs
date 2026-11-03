//! 验收③（GUI-3 §8）：berry-ui paint 产出的 DisplayList 交 berry-gfx
//! soft 光栅执行——小场景（根 + Col{Rect,Text,Rect}）逐 Cmd golden
//! 对拍 + 逐像素 golden 对拍（atlas 位共识 + 透明底零位不写）+
//! 增量 damage 恰发节点界（dirty pass 区外像素零扰动）+ 遮挡剔除
//! （被完全遮挡节点零 Cmd）+ overdraw ≤ 640 预算 + Present 尾恒在。
//!
//! 纪律：确定性、断言值级、只用公共 API（§6 公开面）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos, ImageSrc, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto::Rect as WireRect;
use berry_ui::damage::RectI;
use berry_ui::desc::{
    ColProps, GridProps, IconProps, ImageProps, RectProps, RowProps, TextProps, TextStyle,
};
use berry_ui::metrics::IconSource;
use berry_ui::{OVERDRAW_BUDGET_X256, Props, TextBuf, Ui, UiMain, ViewKind, ViewSpec};

const W: usize = 64;
const H: usize = 48;
const STRIDE: usize = 320; // > w*4：行尾留 16px padding 面（不写铁律探针）
const SENT: u32 = 0xEEEE_EEEE; // 未写哨兵字

const C1: Color = Color { r: 0xD0, g: 0x10, b: 0x20 };
const C2: Color = Color { r: 0x10, g: 0x40, b: 0x90 };
const C3: Color = Color { r: 0x30, g: 0x80, b: 0xA0 };
const FG: Color = Color { r: 0x20, g: 0xE0, b: 0x40 };

fn word(c: Color) -> u32 {
    TextureFormat::Rgbx.pack(c.r, c.g, c.b)
}

/// fb[y][x]（stride 口 + 小端 u32）。
fn px(fb: &[u8], x: usize, y: usize) -> u32 {
    let off = y * STRIDE + x * 4;
    u32::from_le_bytes([fb[off], fb[off + 1], fb[off + 2], fb[off + 3]])
}

fn rect_spec(bg: Color, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg, w, h }),
    }
}

// ---------- GUI-6 段C Image/Icon 叶发射钉板（§4.3 / §11-14） ----------

/// 双注册像素供件：Icon id=7 → 2×2 ARGB；Image res=9 → 3×2 ARGB。
/// px 值钉具体字面量（载荷值级对拍用）。
struct PaintSrc;
static ICON_PX: [u32; 4] = [0xFF_FF_00_00, 0xFF_00_FF_00, 0xFF_00_00_FF, 0xFF_FF_FF_00];
static IMG_PX: [u32; 6] =
    [0xFF_10_20_30, 0xFF_40_50_60, 0xFF_70_80_90, 0xFF_A0_B0_C0, 0xFF_D0_E0_F0, 0xFF_0F_1F_2F];
static PAINT_SRC: PaintSrc = PaintSrc;

impl IconSource for PaintSrc {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        match id {
            7 => Some((2, 2)),
            9 => Some((3, 2)),
            _ => None,
        }
    }
    fn pixels(&self, id: u16) -> Option<&[u32]> {
        match id {
            7 => Some(&ICON_PX),
            9 => Some(&IMG_PX),
            _ => None,
        }
    }
}

fn icon_spec(id: u16, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Icon,
        key: 0,
        props: Props::Icon(IconProps { id, fg: Color { r: 10, g: 20, b: 30 }, w, h }),
    }
}

fn image_spec(res: u16, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec { kind: ViewKind::Image, key: 0, props: Props::Image(ImageProps { res, w, h }) }
}

/// 场景：Root → Col(gap 0){ Rect C1 64×16, Text "AB" 8×16 fg 透明底,
/// Rect C2 64×16 }。返回 (ui, r1, text, r2)。
fn build_col_scene() -> (UiMain, berry_ui::NodeId, berry_ui::NodeId, berry_ui::NodeId) {
    let mut ui = UiMain::new(W as u32, H as u32);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }),
            },
        )
        .unwrap();
    let r1 = ui.add(col, rect_spec(C1, Some(64), Some(16))).unwrap();
    let text = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Text,
                key: 0,
                props: Props::Text(TextProps {
                    style: TextStyle { fg: FG, bg: None, scale: FontSize::S8x16 },
                    text: TextBuf::new(b"AB").unwrap(),
                }),
            },
        )
        .unwrap();
    let r2 = ui.add(col, rect_spec(C2, Some(64), Some(16))).unwrap();
    ui.layout();
    (ui, r1, text, r2)
}

// ---------- ① 逐 Cmd golden：发射序 + 载荷值级 ----------

#[test]
fn golden_cmd_stream_col_scene() {
    let (ui, r1, text, r2) = build_col_scene();

    // 布局值级锚：Col 竖排三段 (0,0/16/32)
    assert_eq!(ui.bounds(r1).unwrap(), RectI::new(0, 0, 64, 16));
    assert_eq!(ui.bounds(text).unwrap(), RectI::new(0, 16, 16, 16));
    assert_eq!(ui.bounds(r2).unwrap(), RectI::new(0, 32, 64, 16));

    let mut ui = ui;
    ui.wake(0); // FULL → 全树发射
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();

    assert_eq!(rep.cmds, 4, "FillRect + GlyphRun + FillRect + Present");
    assert_eq!(dl.len(), 4);
    assert_eq!(rep.culled, 0);
    assert_eq!(rep.drawn_px, 64 * 16 + 16 * 16 + 64 * 16); // 2304
    assert_eq!(rep.overdraw_x256, 2304 * 256 / (64 * 48)); // 192
    assert!(rep.overdraw_x256 <= OVERDRAW_BUDGET_X256);

    let cmds: Vec<&Cmd> = dl.iter().collect();
    // 前序发射（画家算法背到前）：r1 → text → r2 → Present
    match cmds[0] {
        Cmd::FillRect { rect, paint } => {
            assert_eq!((rect.x, rect.y, rect.w, rect.h), (0, 0, 64, 16));
            assert_eq!(paint.color, C1);
        }
        other => panic!("cmd0 应为 FillRect，实为 {:?}", other),
    }
    match cmds[1] {
        Cmd::DrawGlyphRun { run } => {
            assert_eq!(run.glyphs.len(), 2);
            assert_eq!(run.glyphs[0].ch, b'A');
            assert_eq!((run.glyphs[0].x, run.glyphs[0].y), (0, 16));
            assert_eq!(run.glyphs[1].ch, b'B');
            assert_eq!((run.glyphs[1].x, run.glyphs[1].y), (8, 16));
            assert_eq!(run.fg, FG);
            assert_eq!(run.bg, None, "TextStyle.bg None 透传透明底");
            assert_eq!(run.atlas.cell_w(), 8);
            assert_eq!(run.atlas.cell_h(), 16);
        }
        other => panic!("cmd1 应为 DrawGlyphRun，实为 {:?}", other),
    }
    match cmds[2] {
        Cmd::FillRect { rect, paint } => {
            assert_eq!((rect.x, rect.y, rect.w, rect.h), (0, 32, 64, 16));
            assert_eq!(paint.color, C2);
        }
        other => panic!("cmd2 应为 FillRect，实为 {:?}", other),
    }
    match cmds[3] {
        Cmd::Present { rect } => assert_eq!((rect.x, rect.y, rect.w, rect.h), (0, 0, 64, 48)),
        other => panic!("cmd3 应为 Present，实为 {:?}", other),
    }
}

// ---------- ② 逐像素 golden：soft 光栅共识 ----------

#[test]
fn golden_pixel_raster_via_soft_queue() {
    let (mut ui, _, _, _) = build_col_scene();
    ui.wake(0);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();

    // 纹理几何 + 缓冲公式（need = stride×(h−1) + w×4）
    let desc = TextureDesc::new(W, H, STRIDE, TextureFormat::Rgbx).unwrap();
    assert_eq!(desc.need_bytes(), 320 * 47 + 64 * 4);
    let _ = desc.need_bytes();
    // 分配整 stride×H（≥ need 即合法）：末行 padding 面也进探针域
    let mut fb = vec![0xEEu8; STRIDE * H];

    let mut q = Queue::new();
    let pass = PassDescriptor::new(LoadOp::Load, StoreOp::Store, None);
    q.submit(&dl, &mut fb, &desc, &pass).unwrap();

    // 独立参考帧：band 0 → C1、band 1 → 字形（着墨 FG/零位哨兵）、
    // band 2 → C2；文字带 x≥16 与全部区外 = 哨兵（透明底零位不写）
    let atlas = FontSize::S8x16.atlas();
    let c1 = word(C1);
    let c2 = word(C2);
    let fg = word(FG);
    for y in 0..H {
        for x in 0..W {
            let expect = if y < 16 {
                c1
            } else if y < 32 {
                if x < 16 {
                    let ch = if x < 8 { b'A' } else { b'B' };
                    let on = atlas.glyph(ch).unwrap().bit(x % 8, y - 16);
                    if on { fg } else { SENT }
                } else {
                    SENT
                }
            } else {
                c2
            };
            assert_eq!(px(&fb, x, y), expect, "像素 ({x},{y}) 失配");
        }
    }
    // 行尾 padding 面（x ≥ w 的 16px/行）永不写——soft 铁律全行扫
    for y in 0..H {
        assert_eq!(px(&fb, W, y), SENT, "padding 像素 ({W},{y}) 被写");
    }
}

// ---------- ③ 增量 damage：恰发节点界 + dirty 区外零扰动 ----------

#[test]
fn incremental_damage_exact_node_bounds() {
    let (mut ui, r1, text, r2) = build_col_scene();

    // 首帧 FULL 发射后取出（paint(&self) 不排空——drain_damage 是唯一
    // 消费口）。wake 的 FULL 是**条件化**的（空集才置）：先排掉初摆脏
    // 区，wake 见空 → FULL——条件化语义下 FULL 首帧的标准造法
    ui.drain_damage();
    ui.wake(0);
    let mut scr1 = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc1 = CommandEncoder::new();
    ui.paint(&mut scr1, &mut enc1);
    let drained = ui.drain_damage();
    assert!(drained.full, "空集 wake 置 FULL 经 drain 原样取出");

    // 同几何改色：damage 恰 = 节点旧界（单矩形、非 FULL）
    ui.set_props(r1, Props::Rect(RectProps { bg: C3, w: Some(64), h: Some(16) }))
        .unwrap();
    let d = ui.damage();
    assert!(!d.full);
    assert_eq!(d.count, 1);
    assert_eq!(d.rects[0], WireRect { x: 0, y: 0, w: 64, h: 16 });

    // 增量发射：damage 外的 text/r2 全剔除（零 Cmd）
    let mut scr2 = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc2 = CommandEncoder::new();
    let rep = ui.paint(&mut scr2, &mut enc2);
    let dl = enc2.finish();
    assert_eq!(rep.culled, 2, "damage 外的 text/r2 各剔一次");
    assert_eq!(rep.cmds, 2);
    assert_eq!(dl.len(), 2);
    let cmds: Vec<&Cmd> = dl.iter().collect();
    match cmds[0] {
        Cmd::FillRect { rect, paint } => {
            assert_eq!((rect.x, rect.y, rect.w, rect.h), (0, 0, 64, 16));
            assert_eq!(paint.color, C3, "改色后的新色");
        }
        other => panic!("增量 cmd0 应为 FillRect，实为 {:?}", other),
    }
    match cmds[1] {
        // Present 载荷 = damage 包络（单矩形即自身）
        Cmd::Present { rect } => assert_eq!((rect.x, rect.y, rect.w, rect.h), (0, 0, 64, 16)),
        other => panic!("增量 cmd1 应为 Present，实为 {:?}", other),
    }

    // dirty pass 执行：区外像素零扰动（全帧 0xEE 底，仅 band 0 落 C3）
    let desc = TextureDesc::new(W, H, STRIDE, TextureFormat::Rgbx).unwrap();
    let need = desc.need_bytes();
    let mut fb = vec![0xEEu8; need];
    let mut q = Queue::new();
    let pass = PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(GfxRect::new(0, 0, 64, 16)));
    q.submit(&dl, &mut fb, &desc, &pass).unwrap();
    let c3 = word(C3);
    for y in 0..H {
        for x in 0..W {
            let expect = if y < 16 { c3 } else { SENT };
            assert_eq!(px(&fb, x, y), expect, "增量像素 ({x},{y}) 失配");
        }
    }
    let _ = (text, r2);
}

// ---------- ④ 遮挡剔除 + overdraw 预算 ----------

#[test]
fn occlusion_culling_and_overdraw_budget() {
    let mut ui = UiMain::new(W as u32, H as u32);
    let root = ui.root();
    // 三个满屏 Rect 叠放（Root 直子各 (0,0) 满幅）——无剔除直发 3 张
    // 全屏填充 = overdraw×3 必爆预算
    let a = ui.add(root, rect_spec(C1, None, None)).unwrap();
    let b = ui.add(root, rect_spec(C2, None, None)).unwrap();
    let c = ui.add(root, rect_spec(C3, None, None)).unwrap();
    ui.layout();
    for n in [a, b, c] {
        assert_eq!(ui.bounds(n).unwrap(), RectI::new(0, 0, 64, 48));
    }

    // 无剔除参照：Σ节点界 = 3×全屏 → ×256/屏 = 768 > 640（非空洞守卫）
    let direct: u64 = [a, b, c]
        .iter()
        .map(|n| ui.bounds(*n).unwrap().area())
        .sum();
    assert_eq!(direct * 256 / (64 * 48), 768);
    assert!(direct * 256 / (64 * 48) > OVERDRAW_BUDGET_X256 as u64);

    ui.wake(0); // FULL
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();

    // 后加者（最后画 = 最上）独活；前两张被完全遮挡 → 零 Cmd
    assert_eq!(rep.culled, 2);
    assert_eq!(rep.cmds, 2);
    assert_eq!(dl.len(), 2);
    assert_eq!(rep.drawn_px, 64 * 48); // 只剩最顶一张的足迹
    assert_eq!(rep.overdraw_x256, 256);
    assert!(rep.overdraw_x256 <= OVERDRAW_BUDGET_X256);
    assert_ne!(rep.overdraw_x256 as u64, direct * 256 / (64 * 48));

    let cmds: Vec<&Cmd> = dl.iter().collect();
    match cmds[0] {
        Cmd::FillRect { rect, paint } => {
            assert_eq!((rect.x, rect.y, rect.w, rect.h), (0, 0, 64, 48));
            assert_eq!(paint.color, C3, "末位兄弟 = 最上层的 C3");
        }
        other => panic!("cmd0 应为 FillRect，实为 {:?}", other),
    }
    match cmds[1] {
        Cmd::Present { rect } => assert_eq!((rect.x, rect.y, rect.w, rect.h), (0, 0, 64, 48)),
        other => panic!("cmd1 应为 Present，实为 {:?}", other),
    }
}

// ---------- ⑤ Present 尾恒在（空 damage → 零矩形载荷） ----------

#[test]
fn present_tail_empty_damage_zero_rect() {
    let mut ui = UiMain::new(W as u32, H as u32);
    ui.root();
    ui.layout();
    // 初摆 damage：根界（= viewport）单矩形入集（值级）
    let d = ui.drain_damage();
    assert!(!d.full);
    assert_eq!(d.count, 1);
    assert_eq!(d.rects[0], WireRect { x: 0, y: 0, w: 64, h: 48 });

    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();

    assert_eq!(rep.cmds, 1);
    assert_eq!(dl.len(), 1);
    assert_eq!(rep.culled, 0);
    assert_eq!(rep.drawn_px, 0);
    assert_eq!(rep.overdraw_x256, 0);
    let cmds: Vec<&Cmd> = dl.iter().collect();
    match cmds[0] {
        Cmd::Present { rect } => assert_eq!((rect.x, rect.y, rect.w, rect.h), (0, 0, 0, 0)),
        other => panic!("唯一命令应为 Present，实为 {:?}", other),
    }
}

// ---------- ⑥ 病态巨树：cull 缓冲溢出 → 回退直发射（不丢画面） ----------

/// D10 容量帽联动：恰越 ORDER_CAP = 2*MAX_NODES 的可见叶数（顶层 const
/// 形态随 MAX_NODES 右移——非 generic_const_exprs 语法）。
const OVERFLOW_NODES: usize = 2 * berry_ui::MAX_NODES + 1;

#[test]
fn order_cap_overflow_falls_back_no_lost_leaves() {
    // 1025 可见叶 > ORDER_CAP=2*MAX_NODES=1024：cull 缓冲满帽哨兵 → 整段
    // 回退前序直发射（无剔除无序缓冲）。发射面：1025 × fill_rect——cmds
    // 帽在 berry-gfx MAX_CMDS=256（Present 尾同被帽弃）；drawn_px 照实
    // 累计（发射足迹与录入成功无关）；culled 回卷到段前值后由回退重计
    // = 0（全叶皆 damage 内）。Ui<1100> 超布局 CAP=1024 走保守排布路径
    // ——结构性前提（>1024 节点非契约树），非本断言面。D10 容量翻倍：
    // Ui<600>→Ui<1100>、513 叶/513 宽随源翻 1025（§11.1-4 重钉项）。
    let mut ui = Ui::<1100>::new(OVERFLOW_NODES as u32, 8);
    let root = ui.root();
    let row = ui
        .add(
            root,
            ViewSpec { kind: ViewKind::Row, key: 0, props: Props::Row(RowProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }) },
        )
        .unwrap();
    for _ in 0..OVERFLOW_NODES {
        ui.add(row, rect_spec(C1, Some(1), Some(8))).unwrap();
    }
    ui.layout();
    ui.drain_damage();
    ui.wake(0); // damage 空 → FULL（单 damage 矩形 = viewport）

    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();

    assert_eq!(rep.culled, 0, "回退重计：全叶 damage 内（溢出误计已回卷）");
    assert_eq!(rep.drawn_px, (OVERFLOW_NODES * 8) as u64, "发射足迹全量——cmds 帽不减足迹");
    assert_eq!(rep.cmds, 256, "cmds 帽 = berry-gfx MAX_CMDS（Present 尾被帽弃）");
    assert_eq!(dl.len(), 256);
    assert_eq!(rep.overdraw_x256, 256, "1025×8 足迹恰满 1025×8 视口");
    assert!(rep.overdraw_x256 <= OVERDRAW_BUDGET_X256);
}

// ---------- ⑦ GUI-6 段C：Image/Icon 叶发射 + Grid 容器走查 ----------

#[test]
fn image_icon_paint_emits_draw_image_payload() {
    // §4.3 paint：注册供件 → DrawImage{Argb} 逐载荷对拍——Icon 带调色
    // fg、Image 中性 fg（Argb 忽略 fg/bg——值即文档）；blit 尺寸 = 源内在
    // 尺寸（v1 无缩放），发射序 = 加序（画家算法背到前）
    let mut ui = UiMain::new(W as u32, H as u32);
    ui.set_icon_source(&PAINT_SRC);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }),
            },
        )
        .unwrap();
    let ic = ui.add(col, icon_spec(7, None, None)).unwrap();
    let img = ui.add(col, image_spec(9, None, None)).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(ic).unwrap(), RectI::new(0, 0, 2, 2), "id=7 dims 2×2");
    assert_eq!(ui.bounds(img).unwrap(), RectI::new(0, 2, 3, 2), "res=9 dims 3×2 接续");

    ui.drain_damage();
    ui.wake(0); // FULL
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();

    assert_eq!(rep.cmds, 3, "DrawImage + DrawImage + Present");
    assert_eq!(dl.len(), 3);
    assert_eq!(rep.culled, 0);
    assert_eq!(rep.drawn_px, 2 * 2 + 3 * 2, "发射足迹 = 界 ∩ viewport");
    let cmds: Vec<&Cmd> = dl.iter().collect();
    match cmds[0] {
        Cmd::DrawImage { src: ImageSrc::Argb { w, h, px }, x, y, fg, bg } => {
            assert_eq!((*w, *h), (2, 2));
            assert_eq!(*px, ICON_PX, "Icon 像素载荷逐字面量对拍");
            assert_eq!((*x, *y), (0, 0));
            assert_eq!(*fg, Color { r: 10, g: 20, b: 30 }, "Icon fg = 调色前景（Argb 惰性）");
            assert_eq!(*bg, None);
        }
        other => panic!("cmd0 应为 DrawImage，实为 {:?}", other),
    }
    match cmds[1] {
        Cmd::DrawImage { src: ImageSrc::Argb { w, h, px }, x, y, fg, bg } => {
            assert_eq!((*w, *h), (3, 2));
            assert_eq!(*px, IMG_PX, "Image 像素载荷逐字面量对拍");
            assert_eq!((*x, *y), (0, 2));
            assert_eq!(*fg, Color { r: 0, g: 0, b: 0 }, "Image 无调色语义——中性黑（Argb 惰性）");
            assert_eq!(*bg, None);
        }
        other => panic!("cmd1 应为 DrawImage，实为 {:?}", other),
    }
    assert!(matches!(cmds[2], Cmd::Present { .. }), "尾 Present");
}

#[test]
fn icon_unregistered_empty_paint_no_crash() {
    // §11-14 绘制半：默认空供件（未注册）→ 零 DrawImage、零面积叶入
    // culled、余树照发、不炸（guest 空绘安全面）
    let mut ui = UiMain::new(W as u32, H as u32);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }),
            },
        )
        .unwrap();
    let ghost = ui.add(col, icon_spec(99, None, None)).unwrap();
    let t = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Text,
                key: 0,
                props: Props::Text(TextProps {
                    style: TextStyle { fg: FG, bg: None, scale: FontSize::S8x16 },
                    text: TextBuf::new(b"hi").unwrap(),
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(ghost).unwrap(), RectI::new(0, 0, 0, 0), "未注册 → (0,0) 界");
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 16, 16), "游标推进 0——Text 落 0 位");

    ui.drain_damage();
    ui.wake(0);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();
    assert_eq!(rep.cmds, 2, "GlyphRun + Present——零 DrawImage");
    assert_eq!(dl.len(), 2);
    assert_eq!(rep.culled, 1, "零面积叶按可见区空剔除");
    assert!(matches!(dl.iter().next(), Some(Cmd::DrawGlyphRun { .. })));
}

#[test]
fn image_icon_never_occlusion_sources() {
    // §4.3 hit/遮挡面：Image/Icon 恒不计遮挡源——满幅 Icon 压不住先序
    // Text（对照：同形 Rect 会剔除 Text——occlusion_culling 测 ④）
    let mut ui = UiMain::new(16, 16);
    ui.set_icon_source(&PAINT_SRC);
    let root = ui.root();
    let t = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Text,
                key: 0,
                props: Props::Text(TextProps {
                    style: TextStyle { fg: FG, bg: None, scale: FontSize::S8x16 },
                    text: TextBuf::new(b"A").unwrap(),
                }),
            },
        )
        .unwrap();
    let ic = ui.add(root, icon_spec(7, Some(16), Some(16))).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(ic).unwrap(), RectI::new(0, 0, 16, 16), "定尺寸界全幅");
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 8, 16));

    ui.drain_damage();
    ui.wake(0);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    assert_eq!(rep.culled, 0, "Icon 不计遮挡源——Text 不被剔除");
    assert_eq!(rep.cmds, 3, "GlyphRun + DrawImage + Present");
    assert_eq!(rep.drawn_px, 8 * 16 + 16 * 16, "两叶足迹全计");
}

#[test]
fn grid_container_walk_emits_children_only() {
    // Grid = 无视觉内容容器（cull/emit 走查过而不发）；子按格发射。
    // 2×1 gap0：Rect(None) 拉满轨 32×16（行高 = 格内 max）+ Text 内在 16×16 落 (32,0)
    let mut ui = UiMain::new(W as u32, H as u32);
    let root = ui.root();
    let g = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Grid,
                key: 0,
                props: Props::Grid(GridProps {
                    cols: 2,
                    rows: 1,
                    gap: 0,
                    cross: berry_ui::layout::CrossAlign::Stretch,
                    w: None,
                    h: None,
                }),
            },
        )
        .unwrap();
    let r = ui.add(g, rect_spec(C1, None, None)).unwrap();
    let t = ui
        .add(
            g,
            ViewSpec {
                kind: ViewKind::Text,
                key: 0,
                props: Props::Text(TextProps {
                    style: TextStyle { fg: FG, bg: None, scale: FontSize::S8x16 },
                    text: TextBuf::new(b"hi").unwrap(),
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(r).unwrap(), RectI::new(0, 0, 32, 16), "轨宽 64/2 = 32、轨高 = 格内 max(0,16) = 16（Grid 主轴内在——§4.1 Col 同形）");
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(32, 0, 16, 16), "Text 豁免——内在 16×16");

    ui.drain_damage();
    ui.wake(0);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();
    assert_eq!(rep.cmds, 3, "FillRect + GlyphRun + Present");
    assert_eq!(rep.culled, 0);
    assert!(matches!(dl.iter().next(), Some(Cmd::FillRect { .. })));
}
