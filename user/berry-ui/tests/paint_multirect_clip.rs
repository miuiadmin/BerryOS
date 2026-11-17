//! 多矩形 damage 帧的 per-rect PushClip/PopClip 括号钉板（raster 审计
//! C2：横滑帧每个 damage rect 各发射一段命令，submit 期只被 pass
//! dirty（= damage 包络）裁剪——跨全部矩形可见的叶（全屏壁纸）每段
//! 各画一遍整包络 = 矩形数 × 包络）。paint 现为每个 damage 矩形段加
//! PushClip{d}/PopClip → raster 只画 Σband。
//!
//! 钉三面：
//! ① **命令结构**：多矩形帧 PushClip/PopClip 恰一对 per damage 矩形
//!    （括号矩形值级 = damage 矩形）；单矩形/FULL 帧零括号（现状黄金
//!    流不扰动）。
//! ② **像素一致**：多矩形帧（submit dirty = 包络，镜像 berry-launcher
//!    frameloop）在 damage 并集内逐像素 == FULL 帧参考渲（同一场景、
//!    dirty=None）——含半透明 Image 的 blend 语义（每带恰混一次，不随
//!    矩形数叠混）。重复发射的全幅 Image 跨 rects 各被自己的括号裁剪
//!    ——这正是「重复 Image 去重」的像素面：效果等价于只画并集一遍。
//! ③ **Σband 有界**：包络内、并集外（带间隙）像素零扰动（哨兵原样）
//!    ——无括号时全幅 Image 会把整个包络刷掉，间隙必被写。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos, ImageSrc, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_ui::desc::{ImageProps, RectProps};
use berry_ui::metrics::IconSource;
use berry_ui::{Props, UiMain, ViewKind, ViewSpec};

const W: usize = 64;
const H: usize = 48;
const STRIDE: usize = W * 4;
const SENT: u32 = 0xEEEE_EEEE;
const SENT_B: u8 = 0xEE;

const C_A: Color = Color { r: 0xD0, g: 0x10, b: 0x20 };
const C_B: Color = Color { r: 0x10, g: 0x40, b: 0x90 };
const C_A2: Color = Color { r: 0x30, g: 0x80, b: 0xA0 };
const C_B2: Color = Color { r: 0x20, g: 0xE0, b: 0x40 };

/// 全幅 64×48 ARGB：偶行不透明（a=255）、奇行半透明（a=128）——
/// 像素面同时覆盖「末写者胜」与 blend 计数两面（括号错几何/错段序
/// 任一都会逐字节暴露）。
fn img_px() -> &'static [u32] {
    static PX: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    PX.get_or_init(|| {
        (0..W * H)
            .map(|i| {
                let (x, y) = (i % W, i / W);
                let a: u32 = if y % 2 == 0 { 255 } else { 128 };
                (a << 24)
                    | (((x * 4) as u32 & 0xFF) << 16)
                    | (((y * 5) as u32 & 0xFF) << 8)
                    | 0x66
            })
            .collect()
    })
}

struct FullVpSrc;
impl IconSource for FullVpSrc {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        (id == 1).then_some((W as u32, H as u32))
    }
    fn pixels(&self, id: u16) -> Option<&[u32]> {
        (id == 1).then_some(img_px())
    }
}

fn rect_spec(bg: Color, w: u32, h: u32) -> ViewSpec {
    ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg, w: Some(w), h: Some(h), radius: 0}) }
}

fn image_spec() -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Image,
        key: 0,
        props: Props::Image(ImageProps { res: 1, w: Some(W as u32), h: Some(H as u32) }),
    }
}

/// 场景：Root → A(24×12 @0,0) → B(24×12 @40,36) → 全幅 Image（末位 =
/// 最上层，半透明行在 A/B 之上可见——blend 面进并集）。Root 直子为
/// stack 语义（各落 (0,0)），B 经 set_offset 平移到右下角。返回
/// (ui, a, b)。
fn build_scene() -> (UiMain, berry_ui::NodeId, berry_ui::NodeId) {
    let mut ui = UiMain::new(W as u32, H as u32);
    ui.set_icon_source(&FullVpSrc);
    let root = ui.root();
    let a = ui.add(root, rect_spec(C_A, 24, 12)).unwrap();
    let b = ui.add(root, rect_spec(C_B, 24, 12)).unwrap();
    ui.add(root, image_spec()).unwrap();
    ui.layout();
    ui.set_offset(b, 40, 36).unwrap();
    ui.layout();
    (ui, a, b)
}

/// 同一改色施加（两路径共用——参考帧与被测帧渲染同一最终场景）。
fn recolor(ui: &mut UiMain, a: berry_ui::NodeId, b: berry_ui::NodeId) {
    ui.set_props(a, Props::Rect(RectProps { bg: C_A2, w: Some(24), h: Some(12), radius: 0})).unwrap();
    ui.set_props(b, Props::Rect(RectProps { bg: C_B2, w: Some(24), h: Some(12), radius: 0})).unwrap();
}

fn fb_word(fb: &[u8], x: usize, y: usize) -> u32 {
    let off = y * STRIDE + x * 4;
    u32::from_le_bytes([fb[off], fb[off + 1], fb[off + 2], fb[off + 3]])
}

fn desc() -> TextureDesc {
    TextureDesc::new(W, H, STRIDE, TextureFormat::Rgbx).unwrap()
}

/// 参考帧：FULL damage（wake 条件化置 FULL）→ paint → dirty=None 提交。
/// 与被测路径施加同一改色——两帧渲染同一最终场景。
fn reference_frame() -> Vec<u8> {
    let (mut ui, a, b) = build_scene();
    ui.drain_damage();
    recolor(&mut ui, a, b);
    ui.drain_damage();
    ui.wake(0);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();
    let mut fb = vec![SENT_B; desc().need_bytes()];
    Queue::new()
        .submit(&dl, &mut fb, &desc(), &PassDescriptor::new(LoadOp::Load, StoreOp::Store, None))
        .unwrap();
    fb
}

#[test]
fn multi_rect_damage_bracket_pixels_match_full_frame() {
    let (mut ui, a, b) = build_scene();
    ui.drain_damage();

    // 两处改色 → damage = {A 界, B 界}（包络 64×48 全幅、Σ=576 →
    // 浪费 8× 拒并——恰两矩形）。
    recolor(&mut ui, a, b);
    let dmg = ui.damage().clone();
    assert!(!dmg.full);
    assert_eq!(dmg.count, 2, "两带不可并（包络浪费 8×）");
    let env = dmg.envelope().expect("包络在");
    assert_eq!((env.w, env.h), (W as u32, H as u32), "包络 = 整幅（横滑病理形）");

    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();

    // ① 命令结构：每带一对 PushClip/PopClip；全幅 Image 跨两带各发射
    //    一次（DrawImage×2）但各被自己的括号裁剪。段内命令序零变化。
    let (mut push, mut pop, mut imgs, mut fills) = (0, 0, 0, 0);
    let mut push_rects: Vec<GfxRect> = Vec::new();
    for cmd in dl.iter() {
        match cmd {
            Cmd::PushClip { rect } => {
                push += 1;
                push_rects.push(*rect);
            }
            Cmd::PopClip => pop += 1,
            Cmd::DrawImage { src: ImageSrc::Argb { .. }, .. } => imgs += 1,
            Cmd::FillRect { .. } => fills += 1,
            _ => {}
        }
    }
    assert_eq!((push, pop), (2, 2), "每 damage 矩形恰一对括号");
    assert_eq!(imgs, 2, "全幅 Image 跨两带各一段（各被括号裁到本带）");
    assert_eq!(fills, 2, "A/B 各在本带段发射");
    // 括号矩形值级 = 两 damage 矩形（A 界 (0,0,24,12)、B 界 (40,36,24,12)）
    assert!(push_rects.contains(&GfxRect::new(0, 0, 24, 12)), "括号0 = A 界，实 {push_rects:?}");
    assert!(push_rects.contains(&GfxRect::new(40, 36, 24, 12)), "括号1 = B 界，实 {push_rects:?}");
    // cmds 记账：括号 4 + 叶 4（A+img+img+B）+ Present 1 = 9
    assert_eq!(rep.cmds, 9);
    assert_eq!(dl.len(), 9);

    // ② 像素一致：submit dirty = 包络（镜像 berry-launcher 增量帧）。
    let env_rect = GfxRect::new(
        env.x as i32, env.y as i32, env.w as i32, env.h as i32,
    );
    let mut fb = vec![SENT_B; desc().need_bytes()];
    Queue::new()
        .submit(&dl, &mut fb, &desc(), &PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(env_rect)))
        .unwrap();
    let ref_fb = reference_frame();
    let in_band = |x: usize, y: usize| {
        (x < 24 && y < 12) || (x >= 40 && x < 64 && y >= 36 && y < 48)
    };
    for y in 0..H {
        for x in 0..W {
            let got = fb_word(&fb, x, y);
            if in_band(x, y) {
                assert_eq!(got, fb_word(&ref_fb, x, y), "并集内像素 ({x},{y}) 与 FULL 参考失配");
            } else {
                assert_eq!(got, SENT, "包络内并集外 ({x},{y}) 零扰动（Σband 有界）");
            }
        }
    }
}

#[test]
fn single_rect_and_full_frames_have_no_bracket() {
    // FULL 帧：零括号（现状黄金流不扰动——n_dmgs==1 括号是纯开销）。
    let (mut ui, _, _) = build_scene();
    ui.drain_damage();
    ui.wake(0);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();
    assert!(!dl.iter().any(|c| matches!(c, Cmd::PushClip { .. } | Cmd::PopClip)));

    // 单矩形增量帧：同样零括号。
    let (mut ui, a, _) = build_scene();
    ui.drain_damage();
    ui.set_props(a, Props::Rect(RectProps { bg: C_A2, w: Some(24), h: Some(12), radius: 0})).unwrap();
    assert_eq!(ui.damage().count, 1);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();
    assert!(!dl.iter().any(|c| matches!(c, Cmd::PushClip { .. } | Cmd::PopClip)));
}
