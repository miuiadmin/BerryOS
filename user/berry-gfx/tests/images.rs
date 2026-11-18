//! 图像/图标组（段② `draw_image` 面）：单色位图直接面（`ImageSrc::Bits`）
//! + 图标 LRU 图集面（`ImageSrc::Icon`）→ `mono_blit` 共核的执行/裁剪/
//! 拒面/敌意几何四张验纸。
//!
//! - 位法与字形同一条（MSB = 最左列、每行 `(w+7)/8` 字节、w 非 8 倍数
//!   的跨字节行也在场）；
//! - 恰长拒面 = submit 先验后写（`ImagePixelMismatch` 零半执行——index 0
//!   的合法命令也一笔不落）；
//! - 图标面免恰长门（实现契约）→ 敌意几何经 `bit()` 防御采样 + 可见窗
//!   有界迭代：**不 panic、缺位恒 false、成本 = 可见面不是位图面**；
//! - `Caps::max_texture_extent` = 8192 **承诺地板**（WebGPU 默认
//!   limits）——「承诺 vs soft 余量（i32 坐标域 = `DescError::TooLarge`
//!   硬界）」两层界钉板（承诺界内必可构造；硬界越一即拒）。

use berry_gfx::caps::Device;
use berry_gfx::cmd::{Cmd, Color, CommandEncoder, DisplayList, ImageSrc, Rect};
use berry_gfx::color::{DescError, TextureDesc, TextureFormat};
use berry_gfx::font::GlyphBits;
use berry_gfx::icon::{IconAtlas, IconId, IconLruCache};
use berry_gfx::pass::PassDescriptor;
use berry_gfx::queue::{Queue, SubmitError};

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

// —— 基础：fg/bg 两档 + 跨字节行位法（w=9 → 2B/行）——
#[test]
fn draw_image_bits_fg_bg_and_transparency() {
    let d = desc16();
    // w=10 h=1：on = {0, 9}（byte0 MSB + byte1 次高位）——w 非 8 倍数的
    // 行字节进位路径在场（位法跨字节面）。
    let bits = GlyphBits::new(10, 1, &[0b1000_0000, 0b0100_0000]).unwrap();
    assert_eq!(bits.row_bytes(), 2);

    // 不透明衬底：条带 10px 全落笔（off 位 = bg）。
    let mut fb = vec![0u8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Bits(bits), 2, 5, WHITE, Some(BLUE)).unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0)).unwrap();
    assert_eq!(px(&fb, 2, 5), 0x00FF_FFFF, "on x=0");
    assert_eq!(px(&fb, 11, 5), 0x00FF_FFFF, "on x=9（byte1 位法）");
    for x in 3..=10 {
        assert_eq!(px(&fb, x, 5), 0x0000_00FF, "off x={x} = bg");
    }
    assert_eq!(px(&fb, 1, 5), 0, "条带左外未触");
    assert_eq!(px(&fb, 12, 5), 0, "条带右外未触");
    assert_eq!(px(&fb, 2, 4), 0, "条带上外未触");

    // 透明底：off 位不写（哨兵穿透）。
    let mut fb2 = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Bits(bits), 2, 5, WHITE, None).unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb2, &d, &PassDescriptor::load()).unwrap();
    assert_eq!(px(&fb2, 2, 5), 0x00FF_FFFF, "on 照写");
    assert_eq!(px(&fb2, 3, 5), 0xEEEE_EEEE, "off 不写（穿透）");
    assert_eq!(px(&fb2, 11, 5), 0x00FF_FFFF, "on x=9 照写");
}

// —— 边缘裁剪：部分出界可见半面正确 + 全出界/巨坐标 no-op 不炸 ——
#[test]
fn draw_image_clips_at_edges() {
    let d = desc16();
    // 4×3：行 0/2 整行 on、行 1 全 off。
    let bits = GlyphBits::new(4, 3, &[0xF0, 0x00, 0xF0]).unwrap();
    let mut fb = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Bits(bits), 14, 14, WHITE, None).unwrap();
    // 全出界（右下角外 + 巨坐标 i32 极值）——no-op 不炸。
    enc.draw_image(ImageSrc::Bits(bits), 100, 100, WHITE, Some(BLUE)).unwrap();
    enc.draw_image(ImageSrc::Bits(bits), i32::MIN, i32::MIN, WHITE, Some(BLUE)).unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    // 可见半面：源行 0 → 帧 (14..16,14) 两像素 on；源行 1 → 帧 y=15 off
    // （透明底 = 哨兵）；源行 2 → 帧 y=16 全出界。
    assert_eq!(px(&fb, 14, 14), 0x00FF_FFFF);
    assert_eq!(px(&fb, 15, 14), 0x00FF_FFFF);
    assert_eq!(px(&fb, 14, 15), 0xEEEE_EEEE, "off 行穿透");
    assert_eq!(px(&fb, 15, 15), 0xEEEE_EEEE);
    // 其余全域哨兵（后两条全出界命令零落笔的铁证）。
    let mut on_count = 0usize;
    for y in 0..16 {
        for x in 0..16 {
            if px(&fb, x as i32, y as i32) == 0x00FF_FFFF {
                on_count += 1;
            } else {
                assert_eq!(px(&fb, x as i32, y as i32), 0xEEEE_EEEE, "@({x},{y})");
            }
        }
    }
    assert_eq!(on_count, 2, "全域恰 2 着墨像素");
}

// —— 恰长拒面：字面构造绕过 GlyphBits::new 的由 submit 硬门拦（先验
//    后写——index 0 的合法命令也零落笔）——
#[test]
fn image_pixel_mismatch_loud_zero_partial() {
    let d = desc16();
    let good = GlyphBits::new(8, 16, &[0u8; 16]).unwrap();
    for bad in [
        GlyphBits { w: 8, h: 16, rows: &[0xFF] },           // 短行
        GlyphBits { w: 8, h: 16, rows: &[0xFF; 17] },       // 长行
        GlyphBits { w: usize::MAX, h: usize::MAX, rows: &[] }, // 巨参乘法溢出
    ] {
        let mut fb = vec![0xEEu8; d.need_bytes()];
        let snap = fb.clone();
        let mut enc = CommandEncoder::new();
        enc.fill_rect(Rect::new(0, 0, 16, 16), WHITE).unwrap(); // index 0 合法
        enc.draw_image(ImageSrc::Bits(bad), 0, 0, WHITE, Some(BLUE)).unwrap(); // index 1
        let dl = enc.finish();
        assert_eq!(
            Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()),
            Err(SubmitError::ImagePixelMismatch { index: 1 }),
            "bad rows.len()={}",
            bad.rows.len()
        );
        assert_eq!(fb, snap, "先验后写：合法前置命令也零落笔");
    }
    // 对照：同几何权威构造（new 过恰长门）→ submit 放行。
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Bits(good), 0, 0, WHITE, Some(BLUE)).unwrap();
    let dl = enc.finish();
    let mut fb = vec![0u8; d.need_bytes()];
    assert!(Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).is_ok());
    // 构造口拒面随错携带（与 submit 硬门同一恰长律）。
    assert_eq!(
        GlyphBits::new(8, 16, &[0xFF; 15]),
        Err(berry_gfx::font::BitsError::PixelCountMismatch { expect: 16, got: 15 })
    );
    assert_eq!(
        GlyphBits::new(1 << 31, 4, &[]),
        Err(berry_gfx::font::BitsError::TooLarge)
    );
}

// —— 图标端到端：IconLruCache 经 &dyn IconAtlas 进命令；缺图标 no-op；
//    LRU 逐出后同 id 变 no-op（缓存语义与绘制语义同帧验收）——
#[test]
fn icon_end_to_end_via_lru() {
    let d = desc16();
    let cache: IconLruCache<2> = IconLruCache::new();
    // 2×2 图标：on = {(0,0), (1,1)}。
    let bits = GlyphBits::new(2, 2, &[0b1000_0000, 0b0100_0000]).unwrap();
    cache.put(IconId(7), bits);

    let mut fb = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &cache, id: IconId(7) }, 3, 3, WHITE, None)
        .unwrap();
    // 缺图标（id 9 不在表）：整命令 no-op（稀疏表合法形态）。
    enc.draw_image(ImageSrc::Icon { atlas: &cache, id: IconId(9) }, 9, 9, WHITE, Some(BLUE))
        .unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    assert_eq!(px(&fb, 3, 3), 0x00FF_FFFF, "icon (0,0) on");
    assert_eq!(px(&fb, 4, 4), 0x00FF_FFFF, "icon (1,1) on");
    assert_eq!(px(&fb, 4, 3), 0xEEEE_EEEE, "off 穿透");
    assert_eq!(px(&fb, 3, 4), 0xEEEE_EEEE, "off 穿透");

    // LRU 逐出：draw 7 已碰热 → 装 1（空槽）→ 装 2（满表）逐出最久
    // 未用的 7 → 再画 7 = no-op；画 1 仍活（逐出不误伤）。
    let filler = GlyphBits::new(1, 1, &[0x80]).unwrap();
    assert_eq!(cache.put(IconId(1), filler), None, "尚有空槽（零逐出）");
    let evicted = cache.put(IconId(2), filler).unwrap();
    assert_eq!(evicted.0, IconId(7), "2 槽满 → 逐出最久未用 7");
    let mut fb2 = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &cache, id: IconId(7) }, 0, 0, WHITE, Some(BLUE))
        .unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb2, &d, &PassDescriptor::load()).unwrap();
    assert!(fb2.iter().all(|&b| b == 0xEE), "被逐出图标 = 缺图标 = no-op");
    let mut fb3 = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &cache, id: IconId(1) }, 5, 5, WHITE, None)
        .unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb3, &d, &PassDescriptor::load()).unwrap();
    assert_eq!(px(&fb3, 5, 5), 0x00FF_FFFF, "幸存图标照画");
}

/// 敌意图标图集：返回字面构造的畸形位图（免恰长门——实现契约面）。
struct HostileAtlas {
    /// usize 巨几何（w/h 域外——夹取防线）。
    giant: bool,
}
impl IconAtlas for HostileAtlas {
    fn icon(&self, _id: IconId) -> Option<GlyphBits<'static>> {
        if self.giant {
            // 巨几何 + 透明底：夹到 i32::MAX 后 ∩ 帧 = 整帧可见窗；
            // bit(0,0) 有值 → 恰 (0,0) 一像素 on（可见面迭代有界）。
            Some(GlyphBits { w: usize::MAX, h: usize::MAX, rows: &[0x80] })
        } else {
            // 短行（缺位恒 false——防御采样，不 panic）。
            Some(GlyphBits { w: 8, h: 4, rows: &[0xFF] })
        }
    }
}

// —— 敌意几何免疫（图标面免恰长门的安全网）：不 panic + 缺位恒 false
//    + 迭代有界（巨位图成本 = 可见面，不是位图面）——
#[test]
fn hostile_icon_geometry_no_panic() {
    let d = desc16();
    // 巨几何：整帧可见窗内仅 bit(0,0) 有数据 → (0,0) fg、其余缺位 false
    // （透明底穿透）。若按位图面迭代 = usize::MAX 宽循环（测试挂死），
    // 可见窗界使其与 16×16 帧同阶完成。
    let atlas = HostileAtlas { giant: true };
    let mut fb = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &atlas, id: IconId(0) }, 0, 0, WHITE, None)
        .unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    assert_eq!(px(&fb, 0, 0), 0x00FF_FFFF, "唯一有数据位");
    assert_eq!(px(&fb, 1, 0), 0xEEEE_EEEE, "缺位恒 false（穿透）");

    // 短行：行 0 整行 on、行 1..=3 缺数据恒 false（不透明底 = bg 铺）。
    let atlas = HostileAtlas { giant: false };
    let mut fb2 = vec![0u8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Icon { atlas: &atlas, id: IconId(0) }, 2, 2, WHITE, Some(BLUE))
        .unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb2, &d, &PassDescriptor::clear(0, 0, 0)).unwrap();
    for x in 2..10 {
        assert_eq!(px(&fb2, x, 2), 0x00FF_FFFF, "行 0 on x={x}");
        for y in 3..6 {
            assert_eq!(px(&fb2, x, y), 0x0000_00FF, "缺行 y={y} 恒 false = bg");
        }
    }
}

// —— Caps.max_texture_extent：承诺地板 vs soft 实际余量 两层界 ——
//
// 承诺层（跨后端可移植面）：8192 = WebGPU spec 默认 limits
// `maxTextureDimension2D`（Dawn 最低特性集地板）——**Caps 报承诺不报
// 余量**，承诺界内构造必成（真后端也必须兑现）。余量层（soft 事实）：
// 光栅核 i32 坐标域才是构造期 `DescError::TooLarge` 执法的硬界——比
// 承诺宽得多；超承诺的纹理在 soft 上「能构造」，但不许当可移植面依赖
// （真后端可以拒、按承诺编程的消费侧也看不见）。
#[test]
fn caps_max_texture_extent_promise_vs_headroom() {
    let cap = Device::new().caps().max_texture_extent;
    assert_eq!(cap, 8192, "承诺地板 = WebGPU 默认 limits maxTextureDimension2D");
    // 承诺界上恰可构造（stride ≥ w*4 最小值——只算术不分配）。
    let c = cap as usize;
    assert!(TextureDesc::new(c, 16, c * 4, TextureFormat::Rgbx).is_ok(), "承诺界内必兑现");
    // 余量层存在性：超承诺一仍在 soft 可构造（余量是事实不是承诺）。
    let over = c + 1;
    assert!(TextureDesc::new(over, 16, over * 4, TextureFormat::Rgbx).is_ok());
    // 真正的硬界 = i32 坐标域（DescError::TooLarge）：域内极限仍可构造、
    // 越一即拒（构造期拒面执法的界与 Caps 承诺分层不打架）。
    let lim = i32::MAX as usize;
    assert!(
        TextureDesc::new(lim, 16, lim.checked_mul(4).unwrap(), TextureFormat::Rgbx).is_ok(),
        "i32 域内极限 = soft 实际余量上界（可构造）"
    );
    assert_eq!(
        TextureDesc::new(lim + 1, 16, 64, TextureFormat::Rgbx).unwrap_err(),
        DescError::TooLarge,
        "硬界越一即拒"
    );
    assert_eq!(
        TextureDesc::new(8, lim + 1, 64, TextureFormat::Rgbx).unwrap_err(),
        DescError::TooLarge
    );
}

// —— BitsError 形态面（拒面可判别可比较——诊断行直打的数据载体）——
#[test]
fn bits_error_forms() {
    assert_eq!(
        berry_gfx::font::BitsError::PixelCountMismatch { expect: 3, got: 4 },
        berry_gfx::font::BitsError::PixelCountMismatch { expect: 3, got: 4 }
    );
    assert_ne!(
        berry_gfx::font::BitsError::PixelCountMismatch { expect: 3, got: 4 },
        berry_gfx::font::BitsError::TooLarge
    );
    // ImageSrc 两种形态都在场（Debug 面可打）。
    let b = GlyphBits::new(1, 1, &[0x80]).unwrap();
    let s = ImageSrc::Bits(b);
    assert!(format!("{s:?}").contains("w: 1"));
    let cache: IconLruCache<1> = IconLruCache::new();
    let s = ImageSrc::Icon { atlas: &cache, id: IconId(3) };
    assert!(format!("{s:?}").contains("3"), "Icon Debug 打槽位");
}

// —— ARGB 彩色面：A 位 1-bit 透明键 + 出界裁剪 + fg/bg 不适用 ——
//
// 与 mono 面的分工：mono（Bits/Icon）= GlyphBits 一条位法 + fg/bg 着色；
// Argb = 解码产物同序直用（BYI1 → decode → 绘制零转换），像素自带色。
// A 位语义 = 1-bit 透明键（A==0 跳过 / A!=0 不透明覆盖）——**不是
// blend**：半透红画出来是纯红（A=0x80 与 A=0xFF 同色覆盖，无混合），
// v1 无 blend 铁律不破；透明键让真实图标的圆角/异形底可用。
#[test]
fn draw_image_argb_alpha_key_and_clip() {
    let d = desc16();
    // 3×2：row0 = [透明, 红, 蓝]、row1 = [绿, 半透红, 透明]（ARGB——
    // A 在最高字节：红 = 0xFFFF0000、半透红 = 0x80FF0000）。
    let img: [u32; 6] = [
        0x0000_0000, 0xFFFF_0000, 0xFF00_00FF, //
        0xFF00_FF00, 0x80FF_0000, 0x0000_0000,
    ];
    // 画在 (-1, 2)：源 col0 整列出界裁掉 → 可见面 = col1/col2 两列。
    // fg/bg 传白/蓝——Argb 面不适用（像素自带色），断言证明被忽略。
    let mut fb = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Argb { w: 3, h: 2, px: &img }, -1, 2, WHITE, Some(BLUE)).unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    // 可见窗 2×2 @ (0,2)：源 (1,0)=红 (2,0)=蓝 / (1,1)=半透红 (2,1)=透明。
    assert_eq!(px(&fb, 0, 2), 0x00FF_0000, "源 col1 红（col0 出界裁掉）");
    assert_eq!(px(&fb, 1, 2), 0x0000_00FF, "源 col2 蓝");
    assert_eq!(px(&fb, 0, 3), 0x00FF_0000, "半透红 → 不透明红（A 只做非零键）");
    assert_eq!(px(&fb, 1, 3), 0xEEEE_EEEE, "A=0 透明键：底色穿透");
    // 区外/窗外全未触（fg/bg 未铺 = 不适用的活证）。
    assert_eq!(px(&fb, 0, 0), 0xEEEE_EEEE);
    assert_eq!(px(&fb, 2, 2), 0xEEEE_EEEE, "窗外不写");
    assert_eq!(px(&fb, 2, 3), 0xEEEE_EEEE, "A=0 + 窗内：穿透");
}

// —— ScaledArgb：非整数 1.5x 与 2x 最近邻采样、透明键同律 ——
#[test]
fn draw_image_scaled_argb_nearest_and_guard() {
    let d = desc16();
    // 2×2 → 4×2：列采样中心取整为 0,1,1,1；A=0 仍透明键。
    let img: [u32; 4] = [
        0xFFFF_0000, 0x0000_0000, //
        0xFF00_FF00, 0xFF00_00FF,
    ];
    let mut fb = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(
        ImageSrc::ScaledArgb { sw: 2, sh: 2, dw: 4, dh: 2, px: &img },
        0,
        0,
        WHITE,
        None,
    )
    .unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    let want = [0x00FF_0000, 0xEEEE_EEEE, 0xEEEE_EEEE, 0xEEEE_EEEE];
    for (x, v) in want.iter().enumerate() {
        assert_eq!(px(&fb, x as i32, 0), *v, "row0 x={x}");
    }
    assert_eq!(px(&fb, 0, 1), 0x0000_FF00);
    assert_eq!(px(&fb, 1, 1), 0x0000_00FF);
    assert_eq!(px(&fb, 2, 1), 0x0000_00FF);
    assert_eq!(px(&fb, 3, 1), 0x0000_00FF);

    // 生产形 2→3 非整数放大：中心取整列映射 0,1,1。
    let img3: [u32; 2] = [0xFFFF_0000, 0xFF00_00FF];
    let mut enc = CommandEncoder::new();
    enc.draw_image(
        ImageSrc::ScaledArgb { sw: 2, sh: 1, dw: 3, dh: 1, px: &img3 },
        0,
        4,
        WHITE,
        None,
    )
    .unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    assert_eq!(px(&fb, 0, 4), 0x00FF_0000);
    assert_eq!(px(&fb, 1, 4), 0x0000_00FF);
    assert_eq!(px(&fb, 2, 4), 0x0000_00FF);

    // 零目标几何 / 源恰长破裂：submit 先验拒，帧面零落笔。
    let short: [u32; 3] = [0xFFFF_0000; 3];
    let mut enc = CommandEncoder::new();
    enc.draw_image(
        ImageSrc::ScaledArgb { sw: 2, sh: 2, dw: 4, dh: 2, px: &short },
        0,
        0,
        WHITE,
        None,
    )
    .unwrap();
    let before = fb.clone();
    let dl = enc.finish();
    assert_eq!(
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()),
        Err(SubmitError::ImagePixelMismatch { index: 0 })
    );
    assert_eq!(fb, before);
}

// —— SmoothArgb：全不透明大面壁纸的 Q8 双线性路径 ——
#[test]
fn draw_image_smooth_argb_bilinear() {
    let d = desc16();
    let img: [u32; 2] = [0xFFFF_0000, 0xFF00_00FF];
    let mut fb = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(
        ImageSrc::SmoothArgb { sw: 2, sh: 1, dw: 3, dh: 1, px: &img },
        0,
        0,
        WHITE,
        None,
    )
    .unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
    assert_eq!(px(&fb, 0, 0), 0x00FF_0000);
    assert_eq!(px(&fb, 1, 0), 0x007F_007F);
    assert_eq!(px(&fb, 2, 0), 0x0000_00FF);
}

// —— ARGB 恰长拒面：短/长切片 + 手搓 Cmd 旁路 + 巨参（先验后写）——
#[test]
fn draw_image_argb_pixel_mismatch() {
    let d = desc16();
    let short: [u32; 5] = [0xFF00_0000; 5]; // 3×2 要 6 给 5
    let mut fb = vec![0x11u8; d.need_bytes()];
    let snap = fb.clone();
    let mut enc = CommandEncoder::new();
    enc.draw_image(ImageSrc::Argb { w: 3, h: 2, px: &short }, 0, 0, WHITE, None).unwrap();
    let dl = enc.finish();
    assert_eq!(
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()),
        Err(SubmitError::ImagePixelMismatch { index: 0 })
    );
    assert_eq!(fb, snap, "先验后写：拒面零落笔");
    // 手搓 Cmd 旁路（DisplayList::push 不查 ImageSrc 恰长——submit 期
    // 恰长门是唯一执法点）：长切片同样拒。
    let long: [u32; 7] = [0xFF00_0000; 7];
    let mut dl2 = DisplayList::new();
    dl2.push(Cmd::DrawImage {
        src: ImageSrc::Argb { w: 3, h: 2, px: &long },
        x: 0,
        y: 0,
        fg: WHITE,
        bg: None,
    })
    .unwrap();
    let mut fb2 = vec![0u8; d.need_bytes()];
    assert_eq!(
        Queue::new().submit(&dl2, &mut fb2, &d, &PassDescriptor::load()),
        Err(SubmitError::ImagePixelMismatch { index: 0 })
    );
    // 巨参手搓：w = h = u32::MAX（u64 中转乘法无溢出假象，恒拒——
    // usize 域乘法会把「必拒」错算成「溢出难判」）。
    let mut dl3 = DisplayList::new();
    dl3.push(Cmd::DrawImage {
        src: ImageSrc::Argb { w: u32::MAX, h: u32::MAX, px: &[] },
        x: 0,
        y: 0,
        fg: WHITE,
        bg: None,
    })
    .unwrap();
    assert_eq!(
        Queue::new().submit(&dl3, &mut fb2, &d, &PassDescriptor::load()),
        Err(SubmitError::ImagePixelMismatch { index: 0 })
    );
    // 零尺寸 + 空切片 = 恰长成立 → 合法空操作（与 Rect 零宽语义同族）。
    let mut dl4 = DisplayList::new();
    dl4.push(Cmd::DrawImage {
        src: ImageSrc::Argb { w: 0, h: 0, px: &[] },
        x: 0,
        y: 0,
        fg: WHITE,
        bg: None,
    })
    .unwrap();
    assert!(Queue::new().submit(&dl4, &mut fb2, &d, &PassDescriptor::load()).is_ok());
    // Debug 面可打（三变体齐——bits_error_forms 的 Argb 补位）。
    assert!(format!("{:?}", ImageSrc::Argb { w: 3, h: 2, px: &short }).contains("Argb"));
}
