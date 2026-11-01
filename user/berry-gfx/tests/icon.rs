//! 图标组（段② `icon` 模块）：图标图集消费面的集成验收——**用户侧自实现
//! [`IconAtlas`]**（trait 在 crate 外可实现 = 集成价值第一面：静态表形态
//! 的图标源不经 LRU 也能进 `DrawImage`）+ 图标路绘制语义（跨字节行位法 /
//! 不透明衬底 / 边缘裁剪 / dirty 包络）+ **回放期查询即碰热**（Cell 内部
//! 可变性的设计动机端到端：`&dyn` 引用进命令、submit 内的 `icon()` 查询
//! 就推进 LRU 序——「常驻图集碰热无需 &mut」的唯一可观测证明）。
//!
//! 与 `tests/images.rs` 的分立价值：那边验 `IconLruCache` 缓存语义 +
//! 敌意几何免疫；这边验 **trait 抽象面**（自定义实现 + 静态表）与
//! **绘制语义的组合**（衬底/裁剪/dirty 在图标路上的对偶——`mono_blit`
//! 共核的两处消费必须同语义）。
//!
//! BYI1 线上格式（数据面）的解码验收独立成 `tests/icon_format.rs`
//! （全拒面矩阵 + wire 钉板）；本组专注图集面与绘制面的组合。

use berry_gfx::cmd::{Color, CommandEncoder, ImageSrc, Rect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::GlyphBits;
use berry_gfx::icon::{IconAtlas, IconId, IconLruCache};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;

const WHITE: Color = Color { r: 255, g: 255, b: 255 };
const BLUE: Color = Color { r: 0, g: 0, b: 255 };

fn desc16() -> TextureDesc {
    TextureDesc::new(16, 16, 64, TextureFormat::Rgbx).unwrap()
}

/// 读单像素（u32 小端）。
fn px(fb: &[u8], x: i32, y: i32) -> u32 {
    let o = y as usize * 64 + x as usize * 4;
    u32::from_le_bytes(fb[o..o + 4].try_into().unwrap())
}

/// 用户侧静态图标表（trait 的 crate 外实现面）：id 0 = 9×1 箭头条
/// （w 非 8 倍数——跨字节行位法在场：byte1 次高位落在声明宽 x=9 之外，
/// 结构性不可达——「声明宽是权威、字节内容让位」的活证）；id 1 = 4×3
/// 三线块（行 0/2 整行 on、行 1 全 off）。
struct MenuAtlas;
const ARROW: [u8; 2] = [0b1000_0000, 0b0100_0000];
const BRICK: [u8; 3] = [0xF0, 0x00, 0xF0];

impl IconAtlas for MenuAtlas {
    fn icon(&self, id: IconId) -> Option<GlyphBits<'static>> {
        match id.0 {
            0 => Some(GlyphBits { w: 9, h: 1, rows: &ARROW }),
            1 => Some(GlyphBits { w: 4, h: 3, rows: &BRICK }),
            _ => None,
        }
    }
}

// —— 静态表图标绘制：不透明衬底 + 跨字节行位法 + 缺图标 no-op ——
#[test]
fn custom_static_atlas_draws_with_opaque_bg() {
    let d = desc16();
    let atlas = MenuAtlas;
    let mut fb = vec![0u8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    // id 0（9×1）@ (2,3)：声明宽 9 → 条带 x=2..=10；byte1 次高位（x=9
    // 位）在声明宽之外 = 不可达——唯一 on = x=0（帧 x=2 白、x=3..=10 蓝）。
    enc.draw_image(ImageSrc::Icon { atlas: &atlas, id: IconId(0) }, 2, 3, WHITE, Some(BLUE))
        .unwrap();
    // id 99（缺图标）：整命令 no-op（稀疏表合法形态——icon.rs 头注语义）。
    enc.draw_image(ImageSrc::Icon { atlas: &atlas, id: IconId(99) }, 0, 0, WHITE, Some(BLUE))
        .unwrap();
    let dl = enc.finish();
    Queue::new()
        .submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0))
        .unwrap();
    assert_eq!(px(&fb, 2, 3), 0x00FF_FFFF, "on x=0（byte0 MSB）");
    for x in 3..=10 {
        assert_eq!(px(&fb, x, 3), 0x0000_00FF, "off x={x} = 衬底蓝");
    }
    assert_eq!(px(&fb, 1, 3), 0, "条带左外未触");
    assert_eq!(px(&fb, 11, 3), 0, "声明宽外的 byte1 置位不画（宽是权威）");
    assert_eq!(px(&fb, 2, 2), 0, "条带上外未触");
    // 缺图标命令零落笔的反证：全域除 9×1 条带外全为清屏黑。
    let mut touched = 0;
    for y in 0..16 {
        for x in 0..16 {
            if px(&fb, x as i32, y as i32) != 0 {
                touched += 1;
            }
        }
    }
    assert_eq!(touched, 9, "全域恰 9 像素落笔（9×1 条带本身）");
}

// —— 图标路裁剪与透明底：右下角半出界 + off 行穿透（mono_blit 共核的
//    图标消费对偶——bits 面在 images.rs，此处图标面同语义）——
#[test]
fn icon_clipping_and_transparent_bg() {
    let d = desc16();
    let atlas = MenuAtlas;
    // BRICK 4×3 画到 (14,14)：行 0 可见（x 14..16 两像素 on）、行 1 可见
    // （透明底 → 哨兵穿透）、行 2（y=16）全出界。
    let mut fb = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &atlas, id: IconId(1) }, 14, 14, WHITE, None)
        .unwrap();
    let dl = enc.finish();
    Queue::new()
        .submit(&dl, &mut fb, &d, &PassDescriptor::load())
        .unwrap();
    assert_eq!(px(&fb, 14, 14), 0x00FF_FFFF, "行 0 裁剪后可见 on");
    assert_eq!(px(&fb, 15, 14), 0x00FF_FFFF);
    assert_eq!(px(&fb, 14, 15), 0xEEEE_EEEE, "行 1 off 穿透");
    assert_eq!(px(&fb, 15, 15), 0xEEEE_EEEE);
    // 全出界（负坐标巨偏移）——no-op 不炸、零落笔。
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &atlas, id: IconId(1) }, -100, -100, WHITE, Some(BLUE))
        .unwrap();
    let dl = enc.finish();
    let mut fb2 = vec![0xEEu8; d.need_bytes()];
    Queue::new()
        .submit(&dl, &mut fb2, &d, &PassDescriptor::load())
        .unwrap();
    assert!(fb2.iter().all(|&b| b == 0xEE), "全出界 = 零落笔");
}

// —— 图标路 dirty 包络：区外 = 别人的画面（哨兵逐字节不动）——
#[test]
fn icon_respects_dirty_region() {
    let d = desc16();
    let atlas = MenuAtlas;
    // BRICK 4×3 @ (0,8)：行 y=8/10 整行 on、y=9 off；dirty = (0,10,16,6)
    // 只放行 y ∈ [10,16)——y=8 的整行 on 必须被 dirty 挡下。
    let mut fb = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &atlas, id: IconId(1) }, 0, 8, WHITE, Some(BLUE))
        .unwrap();
    let dl = enc.finish();
    Queue::new()
        .submit(
            &dl,
            &mut fb,
            &d,
            &PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(Rect::new(0, 10, 16, 6))),
        )
        .unwrap();
    for x in 0..4 {
        assert_eq!(px(&fb, x, 8), 0xEEEE_EEEE, "y=8 在 dirty 外——on 行不落笔 x={x}");
        assert_eq!(px(&fb, x, 9), 0xEEEE_EEEE, "y=9 在 dirty 外——off 行原样");
        assert_eq!(px(&fb, x, 10), 0x00FF_FFFF, "y=10 在 dirty 内——on 行照画 x={x}");
    }
    // y=10 行 off 侧（x=4 右邻）不写（衬底只在可见位图区内）。
    assert_eq!(px(&fb, 4, 10), 0xEEEE_EEEE);
}

// —— 回放期查询即碰热（Cell 内部可变性的端到端执法）：submit 内的
//    `icon()` 查询推进 LRU 序——被绘过的图标在满表插入后幸存 ——
#[test]
fn replay_heats_lru_during_submit() {
    let d = desc16();
    const ON: [u8; 1] = [0x80];
    let cache: IconLruCache<2> = IconLruCache::new();
    let single = GlyphBits::new(1, 1, &ON).unwrap();
    cache.put(IconId(1), single);
    cache.put(IconId(2), single);
    // 画 id 1：回放经 &dyn 查询 → 碰热（无任何 &mut——共享引用即够）。
    let mut fb = vec![0u8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &cache, id: IconId(1) }, 0, 0, WHITE, None)
        .unwrap();
    let dl = enc.finish();
    Queue::new()
        .submit(&dl, &mut fb, &d, &PassDescriptor::load())
        .unwrap();
    assert_eq!(px(&fb, 0, 0), 0x00FF_FFFF, "id 1 首画落笔");
    // 满表插入：若回放查询真的碰热 → 逐出 id 2（未画）；否则逐 id 1。
    let evicted = cache.put(IconId(3), single).unwrap();
    assert_eq!(evicted.0, IconId(2), "被绘过的 id 1 因碰热幸存");
    // 幸存者再画照常；被逐者变 no-op（缓存语义与绘制语义同帧闭环）。
    let mut fb2 = vec![0u8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &cache, id: IconId(1) }, 1, 1, WHITE, None)
        .unwrap();
    enc.draw_image(ImageSrc::Icon { atlas: &cache, id: IconId(2) }, 2, 2, WHITE, None)
        .unwrap();
    let dl = enc.finish();
    Queue::new()
        .submit(&dl, &mut fb2, &d, &PassDescriptor::load())
        .unwrap();
    assert_eq!(px(&fb2, 1, 1), 0x00FF_FFFF, "幸存 id 1 照画");
    assert_eq!(px(&fb2, 2, 2), 0, "被逐 id 2 = 缺图标 = no-op");
}

// —— IconId 新类型语义：值判别走内字段（id 空间的唯一入口）——
#[test]
fn icon_id_identity() {
    assert_eq!(IconId(7), IconId(7));
    assert_ne!(IconId(7), IconId(8));
    // 同值不同来源（字面量 vs 拷贝）同 id——id 空间是纯值域。
    let id = IconId(7);
    let copy = id;
    assert_eq!(id, copy);
}
