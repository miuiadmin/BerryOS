//! 拒面组（GUI-2 段①）：全 API 拒面 loud 执法——非法几何 / 容量满 /
//! 缓冲不符 / 坏 dirty / blit 源越界，每条拒面错误**值级**断言（含
//! need/got/index 携带载荷），且先验后写（出错零落笔——缓冲快照对拍
//! 执法）。

use berry_gfx::cmd::{
    Cmd, CmdError, Color, CommandEncoder, DisplayList, ImageSrc, PaintDesc, Rect,
};
use berry_gfx::color::{DescError, TextureDesc, TextureFormat};
use berry_gfx::font::GlyphBits;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::{Queue, SubmitError};

const FMT: TextureFormat = TextureFormat::Rgbx;

// —— TextureDesc 几何拒面（五条全验 + 恰达标边界放行）——
#[test]
fn desc_geometry_reject_faces() {
    // ① w < 8（h/stride 合法）。
    assert_eq!(
        TextureDesc::new(7, 16, 28, FMT).unwrap_err(),
        DescError::TooNarrow
    );
    // ② h < 16。
    assert_eq!(
        TextureDesc::new(8, 15, 32, FMT).unwrap_err(),
        DescError::TooShort
    );
    // ③ stride < w*4（恰差 1：w=8 → 需 32，给 31）。
    assert_eq!(
        TextureDesc::new(8, 16, 31, FMT).unwrap_err(),
        DescError::StrideTooSmall
    );
    // ④ w/h 超 i32 坐标域（敌意巨参——光栅核 i32 域的构造期执法点）。
    assert_eq!(
        TextureDesc::new(i32::MAX as usize + 1, 16, 8, FMT).unwrap_err(),
        DescError::TooLarge
    );
    assert_eq!(
        TextureDesc::new(8, i32::MAX as usize + 1, 32, FMT).unwrap_err(),
        DescError::TooLarge
    );
    // ⑤ 乘加溢出 usize：stride*(h-1) 溢出（stride 巨参，两形）。
    assert_eq!(
        TextureDesc::new(8, 16, usize::MAX, FMT).unwrap_err(),
        DescError::Overflow
    );
    assert_eq!(
        TextureDesc::new(8, 16, usize::MAX / 15 + 1, FMT).unwrap_err(),
        DescError::Overflow
    );
    // 注记：w*4 溢出路径对合法 w 结构性不可达（w ≤ i32::MAX ⇒ w*4 <
    // 2^63）——溢出面由 stride*(h-1) 主项独扛。
    // 边界正面：最小几何恰达标（w=8/h=16/stride=w*4）。
    assert!(TextureDesc::new(8, 16, 32, FMT).is_ok());
}

// —— check_buf 第四拒面：need/got 载荷 + 恰好装下合法 + need 公式 ——
#[test]
fn desc_buffer_check_face() {
    // w=16/h=16/stride=80：need = 80*15 + 64 = 1264（末行不含 padding）。
    let d = TextureDesc::new(16, 16, 80, FMT).unwrap();
    assert_eq!(d.need_bytes(), 1264);
    assert_eq!(
        d.check_buf(0),
        Err(DescError::BufferTooSmall { need: 1264, got: 0 })
    );
    assert_eq!(
        d.check_buf(1263),
        Err(DescError::BufferTooSmall { need: 1264, got: 1263 })
    );
    assert!(d.check_buf(1264).is_ok(), "== need 恰好装下合法");
    assert!(d.check_buf(4096).is_ok(), "> need（更长缓冲合法）");
}

// —— DisplayList 容量满 loud：恰 256 过、第 257 拒（encoder 面同执法）——
#[test]
fn display_list_capacity_full_loud() {
    let mut enc = CommandEncoder::new();
    for i in 0..256 {
        enc.fill_rect(Rect::new(i as i32 % 16, 0, 1, 1), Color { r: 1, g: 2, b: 3 })
            .unwrap();
    }
    let mut dl = enc.finish();
    assert_eq!(dl.len(), 256);
    // 直接 push 与 encoder.record 同拒面。
    assert_eq!(
        dl.push(Cmd::Present { rect: Rect::new(0, 0, 1, 1) }),
        Err(CmdError::Full)
    );
    assert_eq!(dl.len(), 256, "拒后长度不变（零静默丢）");
    // 满 + 坏矩形：Full 先判（容量是第一道先验）。
    assert_eq!(
        dl.push(Cmd::FillRect {
            rect: Rect::new(0, 0, -1, 1),
            paint: PaintDesc { color: Color { r: 0, g: 0, b: 0 } }
        }),
        Err(CmdError::Full)
    );
}

// —— submit 缓冲拒面：desc 与 target 长度不符（过短 loud 带载荷 / 过长合法）——
#[test]
fn submit_buffer_mismatch_rejects() {
    let d = TextureDesc::new(16, 16, 64, FMT).unwrap(); // need = 64*15+64 = 1024
    let dl = DisplayList::new();
    let mut short = vec![0u8; 1023];
    assert_eq!(
        Queue::new().submit(&dl, &mut short, &d, &PassDescriptor::load()),
        Err(SubmitError::BufferTooSmall { need: 1024, got: 1023 })
    );
    // 过长缓冲合法（GOP 真机面：分配粒度常大于 need）——照常执行。
    let mut long = vec![0u8; 4096];
    assert!(
        Queue::new()
            .submit(&dl, &mut long, &d, &PassDescriptor::clear(7, 8, 9))
            .is_ok()
    );
    assert_eq!(&long[0..4], &(7u32 << 16 | 8 << 8 | 9).to_le_bytes());
}

// —— submit 坏 dirty 拒面：负宽/负高 loud + 零落笔 ——
#[test]
fn submit_bad_dirty_rect_rejects() {
    let d = TextureDesc::new(16, 16, 64, FMT).unwrap();
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(0, 0, 16, 16), Color { r: 255, g: 255, b: 255 })
        .unwrap();
    let dl = enc.finish();
    for dirty in [Rect::new(0, 0, -1, 4), Rect::new(0, 0, 4, -1), Rect::new(1, 1, -8, -8)] {
        let mut fb = vec![0x11u8; d.need_bytes()];
        let snap = fb.clone();
        assert_eq!(
            Queue::new().submit(
                &dl,
                &mut fb,
                &d,
                &PassDescriptor::new(LoadOp::Clear { r: 9, g: 9, b: 9 }, StoreOp::Store, Some(dirty))
            ),
            Err(SubmitError::BadDirtyRect),
            "dirty={dirty:?}"
        );
        assert_eq!(fb, snap, "拒面零落笔（dirty={dirty:?}）");
    }
}

// —— blit 源区拒面：四向越界各拒 + 零尺寸放行 + 恰满放行 ——
#[test]
fn submit_blit_src_oob_rejects() {
    let d = TextureDesc::new(16, 16, 64, FMT).unwrap();
    // 四向越界（其余维度合法）。
    for src in [
        Rect::new(-1, 0, 8, 8),   // 左越
        Rect::new(0, -1, 8, 8),   // 上越
        Rect::new(0, 0, 17, 4),   // 右越 1px
        Rect::new(0, 0, 4, 17),   // 下越 1px
        Rect::new(9, 9, 8, 8),    // 右下角内起点出界
        Rect::new(0, 0, i32::MAX, 4), // 巨宽（saturating_add 后仍越界）
    ] {
        let mut enc = CommandEncoder::new();
        enc.blit(src, 0, 0).unwrap();
        let dl = enc.finish();
        let mut fb = vec![0u8; d.need_bytes()];
        assert_eq!(
            Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()),
            Err(SubmitError::BlitSrcOutOfBounds { index: 0 }),
            "src={src:?}"
        );
    }
    // 零尺寸源恒放行（空操作）+ 负原点零尺寸同放行 + 恰满帧源放行。
    for src in [
        Rect::new(-5, -5, 0, 8),
        Rect::new(0, 0, 8, 0),
        Rect::new(0, 0, 16, 16),
    ] {
        let mut enc = CommandEncoder::new();
        enc.blit(src, 3, 3).unwrap();
        let dl = enc.finish();
        let mut fb = vec![0u8; d.need_bytes()];
        assert!(
            Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).is_ok(),
            "src={src:?} 应放行"
        );
    }
}

// —— 先验后写（零半执行）：三种拒面任一命中，此前已录的合法命令
//    一笔都不落——「出错零半执行面」是 submit 的结构性承诺 ——
#[test]
fn submit_no_partial_execution_on_any_reject() {
    let d = TextureDesc::new(16, 16, 64, FMT).unwrap();
    // 会写满帧的合法命令在前，各拒面在后。
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(0, 0, 16, 16), Color { r: 255, g: 255, b: 255 })
        .unwrap();
    enc.draw_line(0, 0, 15, 15, Color { r: 255, g: 0, b: 0 }).unwrap();
    let base = enc.finish();

    // 拒面 1：末尾追加越界 blit（index = 2）。
    let mut dl = DisplayList::new();
    for c in base.iter() {
        dl.push(*c).unwrap();
    }
    dl.push(Cmd::Blit { src: Rect::new(0, 0, 17, 4), dst_x: 0, dst_y: 0 }).unwrap();
    let mut fb = vec![0x22u8; d.need_bytes()];
    let snap = fb.clone();
    assert_eq!(
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0)),
        Err(SubmitError::BlitSrcOutOfBounds { index: 2 })
    );
    assert_eq!(fb, snap, "拒面 1：合法命令零落笔");

    // 拒面 2：缓冲过短（Clear 也一笔不铺）。
    let mut short = vec![0x22u8; d.need_bytes() - 1];
    let snap2 = short.clone();
    assert_eq!(
        Queue::new().submit(&base, &mut short, &d, &PassDescriptor::clear(0, 0, 0)),
        Err(SubmitError::BufferTooSmall { need: d.need_bytes(), got: d.need_bytes() - 1 })
    );
    assert_eq!(short, snap2, "拒面 2：Clear 零落笔");

    // 拒面 3：坏 dirty（同上零落笔——submit_bad_dirty_rect_rejects 已
    // 验单命令面，此处复验「多合法命令在前列」形态）。
    let mut fb3 = vec![0x22u8; d.need_bytes()];
    let snap3 = fb3.clone();
    assert_eq!(
        Queue::new().submit(
            &base,
            &mut fb3,
            &d,
            &PassDescriptor::new(LoadOp::Clear { r: 1, g: 2, b: 3 }, StoreOp::Store, Some(Rect::new(0, 0, 2, -2)))
        ),
        Err(SubmitError::BadDirtyRect)
    );
    assert_eq!(fb3, snap3, "拒面 3：零落笔");
}

// —— 图像恰长拒面（段② `ImagePixelMismatch`）：字面 `GlyphBits` 绕过
//    `GlyphBits::new` 权威构造口、且经 `DisplayList::push` 手搭 `Cmd`
//    绕过编码器——**两道前置都绕开，硬门仍在 submit 期拦住**（拒面驻
//    留点 = 回放入口，与命令怎么录进来的无关）；index 随命令序 loud、
//    先验后写零落笔；合法路径（权威构造 + 零维空位图 no-op）对照放行 ——
#[test]
fn submit_image_pixel_mismatch_handbuilt_cmd() {
    let d = TextureDesc::new(16, 16, 64, FMT).unwrap();
    let white = Color { r: 255, g: 255, b: 255 };
    let blue = Color { r: 0, g: 0, b: 255 };
    // 手搭命令：8×16 应恰 16 行字节——短/长/空三形全拒。
    let hand = |rows: &'static [u8]| Cmd::DrawImage {
        src: ImageSrc::Bits(GlyphBits { w: 8, h: 16, rows }),
        x: 0,
        y: 0,
        fg: white,
        bg: Some(blue),
    };
    for rows in [&[0xFFu8; 15][..], &[0xFFu8; 17][..], &[][..]] {
        let mut dl = DisplayList::new();
        dl.push(hand(rows)).unwrap();
        let mut fb = vec![0x33u8; d.need_bytes()];
        let snap = fb.clone();
        assert_eq!(
            Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()),
            Err(SubmitError::ImagePixelMismatch { index: 0 }),
            "rows.len()={} 手搭 index 0 也 loud",
            rows.len()
        );
        assert_eq!(fb, snap, "先验后写：零落笔");
    }
    // index 随命令序：合法命令在 index 0、坏命令在 index 1——载荷必须
    // 指认后者（拒面诊断行可定位到具体命令）。
    let mut dl = DisplayList::new();
    dl.push(Cmd::FillRect { rect: Rect::new(0, 0, 16, 16), paint: PaintDesc { color: white } })
        .unwrap();
    dl.push(hand(&[0xFF; 15])).unwrap();
    let mut fb = vec![0x33u8; d.need_bytes()];
    let snap = fb.clone();
    assert_eq!(
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()),
        Err(SubmitError::ImagePixelMismatch { index: 1 })
    );
    assert_eq!(fb, snap, "index 0 的合法 FillRect 也零落笔");

    // 对照放行：权威构造（new 过恰长门）→ submit ok 且像素语义正确。
    let bits = GlyphBits::new(8, 1, &[0b1000_0000]).unwrap();
    let mut dl = DisplayList::new();
    dl.push(Cmd::DrawImage { src: ImageSrc::Bits(bits), x: 2, y: 3, fg: white, bg: Some(blue) })
        .unwrap();
    // 零维空位图（(0,0) 合法空操作）随后——不拒、不落笔。
    dl.push(Cmd::DrawImage {
        src: ImageSrc::Bits(GlyphBits::new(0, 0, &[]).unwrap()),
        x: 0,
        y: 0,
        fg: white,
        bg: Some(blue),
    })
    .unwrap();
    let mut fb = vec![0u8; d.need_bytes()];
    assert!(Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::clear(0, 0, 0)).is_ok());
    let px = |fb: &[u8], x: usize, y: usize| u32::from_le_bytes(fb[y * 64 + x * 4..][..4].try_into().unwrap());
    assert_eq!(px(&fb, 2, 3), 0x00FF_FFFF, "on = 白");
    assert_eq!(px(&fb, 3, 3), 0x0000_00FF, "off = 衬底蓝");
}
