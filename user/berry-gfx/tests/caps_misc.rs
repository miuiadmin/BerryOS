//! Caps/杂项组（GUI-2 段①）：能力面定值钉板 + Device→Queue 端到端、
//! from_raw 宁缺勿猜、缺字形→空格语义与 fbcon 一致（逐字节）、
//! 记录/回放分离形态（同一 DisplayList 对多纹理/多格式重放）。

use berry_fbcon::render::{Fbcon, PixelFormat};
use berry_gfx::caps::{Device, FormatCaps, Msaa};
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun, Rect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontAtlas8x16;
use berry_gfx::pass::PassDescriptor;
use berry_gfx::queue::Queue;

// —— Caps 定值 + Device→default_queue→submit 端到端 ——
#[test]
fn caps_pinned_and_device_queue_end_to_end() {
    let caps = Device::new().caps();
    assert!(caps.blit, "soft 支持纹理内拷贝");
    // transient：soft 定值钉板（防漂移；soft 语义下平凡成立、无可证伪
    // 面——见 caps.rs 字注，真后端接入才变实测位）。
    assert!(caps.transient_attachments, "soft = 复用调用方 CPU buffer");
    assert_eq!(caps.msaa, Msaa::None);
    assert_eq!(caps.texture_formats, FormatCaps { rgbx: true, bgrx: true });
    assert_eq!(caps.max_cmds, 256, "与 cmd::MAX_CMDS 单一事实源同值");
    assert!(caps.texture_formats.supports(TextureFormat::Rgbx));
    assert!(caps.texture_formats.supports(TextureFormat::Bgrx));
    // 端到端：能力面宣称的面真跑通（宣称谎报在此炸）。
    let mut dev = Device::new();
    let mut q = dev.default_queue();
    let d = TextureDesc::new(16, 16, 64, TextureFormat::Rgbx).unwrap();
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(0, 0, 16, 16), Color { r: 0, g: 255, b: 0 })
        .unwrap();
    let dl = enc.finish();
    let mut fb = vec![0u8; d.need_bytes()];
    q.submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0)).unwrap();
    assert_eq!(&fb[0..4], &0x0000_FF00u32.to_le_bytes());
}

// —— from_raw 宁缺勿猜：2=BitMask / 3=BltOnly / 敌意值全 None ——
//
// 咬住什么：把 2/3 猜成任一硬编码格式 = 整屏换色且静默（fbcon 同款
// 执法注记）；判别值 0/1 是 GOP 协议原值（自编序漂移在此炸）。
#[test]
fn format_from_raw_no_guessing() {
    assert_eq!(TextureFormat::from_raw(0), Some(TextureFormat::Rgbx));
    assert_eq!(TextureFormat::from_raw(1), Some(TextureFormat::Bgrx));
    assert_eq!(TextureFormat::from_raw(2), None, "PixelBitMask 不猜");
    assert_eq!(TextureFormat::from_raw(3), None, "PixelBltOnly 无线性 fb");
    for v in [4u32, 0xFF, 0x100, 0x7FFF_FFFF, 0xFFFF_FFFF, u32::MAX] {
        assert_eq!(TextureFormat::from_raw(v), None, "v={v:#x}");
    }
    // 判别值即协议值钉板（as-cast 不漂）。
    assert_eq!(TextureFormat::Rgbx as u32, 0);
    assert_eq!(TextureFormat::Bgrx as u32, 1);
}

// —— 缺字形 → 全 bg 空格蜂格：gfx 与 fbcon 逐字节一致 ——
//
// gfx：atlas glyph()==None → 全零位图；fbcon：font::glyph()==None →
// BLANK——两侧「空格语义」必须是同一张图（含真空格 ' '（有字形、全零
// 位图）同形——三种来源一个产物）。
#[test]
fn missing_glyph_blank_consensus() {
    let (w, h, stride) = (32usize, 32usize, 128usize);
    let fg = (255u8, 255, 255);
    let bg = (0xAAu8, 0x00, 0x99);
    for ch in [0x01u8, 0x7f, 0xC3, b' '] {
        // 控制符 / DEL / UTF-8 引导字节 / 真空格。
        let mut a = vec![0xEEu8; stride * (h - 1) + w * 4];
        {
            let mut c = Fbcon::new(&mut a, w, h, stride, PixelFormat::Rgbx).unwrap();
            c.clear(bg);
            c.put_char(1, 1, ch, fg, bg);
        }
        let d = TextureDesc::new(w, h, stride, TextureFormat::Rgbx).unwrap();
        let mut b = vec![0xEEu8; d.need_bytes()];
        let run = GlyphRun {
            atlas: &FontAtlas8x16,
            glyphs: &[GlyphPos { ch, x: 8, y: 16 }], // 蜂格 (1,1)
            fg: Color { r: fg.0, g: fg.1, b: fg.2 },
            bg: Some(Color { r: bg.0, g: bg.1, b: bg.2 }),
        };
        let mut enc = CommandEncoder::new();
        enc.draw_glyph_run(run).unwrap();
        let dl = enc.finish();
        Queue::new()
            .submit(&dl, &mut b, &d, &PassDescriptor::clear(bg.0, bg.1, bg.2))
            .unwrap();
        assert_eq!(a, b, "ch={ch:#x} 空格语义逐字节共识");
    }
}

// —— 记录/回放分离：同一 DisplayList 对多纹理重放（多目标 × 多格式）——
#[test]
fn display_list_replay_two_targets_and_formats() {
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(0, 0, 16, 16), Color { r: 0x12, g: 0x34, b: 0x56 })
        .unwrap();
    let run = GlyphRun {
        atlas: &FontAtlas8x16,
        glyphs: &[GlyphPos { ch: b'#', x: 0, y: 0 }],
        fg: Color { r: 255, g: 255, b: 255 },
        bg: Some(Color { r: 0x12, g: 0x34, b: 0x56 }),
    };
    enc.draw_glyph_run(run).unwrap();
    let dl = enc.finish(); // 记录一次，回放三次（形态面：列表与纹理解耦）。

    // 重放 ①②：同几何两帧 → 逐字节相同（确定性回放）。
    let d = TextureDesc::new(16, 16, 64, TextureFormat::Rgbx).unwrap();
    let mut f1 = vec![0u8; d.need_bytes()];
    let mut f2 = vec![0u8; d.need_bytes()];
    Queue::new().submit(&dl, &mut f1, &d, &PassDescriptor::clear(0, 0, 0)).unwrap();
    Queue::new().submit(&dl, &mut f2, &d, &PassDescriptor::clear(0, 0, 0)).unwrap();
    assert_eq!(f1, f2, "同一列表两次回放逐字节相同");

    // 重放 ③：同几何 Bgrx 帧 → 命令语义不变、pack 随格式走（背景/衬底
    // 通道序翻转、白前景两格式同值）。
    let db = TextureDesc::new(16, 16, 64, TextureFormat::Bgrx).unwrap();
    let mut f3 = vec![0u8; db.need_bytes()];
    Queue::new().submit(&dl, &mut f3, &db, &PassDescriptor::clear(0, 0, 0)).unwrap();
    let pxb = |fb: &[u8], x: usize, y: usize| u32::from_le_bytes(fb[y * 64 + x * 4..y * 64 + x * 4 + 4].try_into().unwrap());
    assert_eq!(pxb(&f3, 15, 15), 0x0056_3412, "Bgrx 背景（r/b 翻转）");
    assert_eq!(pxb(&f1, 15, 15), 0x0012_3456, "Rgbx 背景");
    // '#' 着墨位两格式同为白（row7=0x7C → gx1 着墨 @ (1,7)——gx0 属
    // 设计留白，MSB=最左列在此再钉一次）。
    assert_eq!(pxb(&f3, 1, 7), 0x00FF_FFFF);
    assert_eq!(pxb(&f1, 1, 7), 0x00FF_FFFF);
}
