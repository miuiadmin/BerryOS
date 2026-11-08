//! F4 圆角矩形 DisplayList 命令测试：命令回放与直接 XRGB 表面路径
//! 逐字节一致，并证明 dirty clip 外像素零写。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, Rect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;

const W: u32 = 16;
const H: u32 = 16;
const STRIDE: usize = 64;
const SENT: u8 = 0xEE;

fn desc() -> TextureDesc {
    TextureDesc::new(W as usize, H as usize, STRIDE, TextureFormat::Rgbx).unwrap()
}

fn fb() -> Vec<u8> {
    vec![SENT; STRIDE * H as usize]
}

#[test]
fn round_rect_command_matches_direct_surface_bytes() {
    let mut enc = CommandEncoder::new();
    let color = Color { r: 12, g: 34, b: 56 };
    enc.fill_round_rect(Rect::new(2, 2, 12, 12), 4, color).unwrap();
    enc.record(Cmd::Present { rect: Rect::new(0, 0, 16, 16) }).unwrap();
    let dl = enc.finish();
    let mut got = fb();
    let d = desc();
    Queue::new()
        .submit(&dl, &mut got, &d, &PassDescriptor::new(LoadOp::Load, StoreOp::Store, None))
        .unwrap();

    let mut want = fb();
    berry_gfx::surface::fill_round_rect_xrgb(
        &mut want, STRIDE, 2, 2, 12, 12, 4, color,
    );
    assert_eq!(got, want, "FillRoundRect 命令路径与 direct surface coverage 必须同源");
}

#[test]
fn round_rect_command_respects_dirty_clip() {
    let mut enc = CommandEncoder::new();
    enc.fill_round_rect(
        Rect::new(2, 2, 12, 12),
        4,
        Color { r: 255, g: 255, b: 255 },
    )
    .unwrap();
    let dl = enc.finish();
    let mut got = fb();
    let d = desc();
    Queue::new()
        .submit(
            &dl,
            &mut got,
            &d,
            &PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(Rect::new(2, 2, 7, 12))),
        )
        .unwrap();

    // clip 内中心直写；角外与 clip 右侧均不得写。
    assert_eq!(&got[5 * STRIDE + 5 * 4..5 * STRIDE + 5 * 4 + 3], &[255, 255, 255]);
    // 名义角 (2,2) 有 AA coverage；真正 clip 外左邻 (1,2) 必须保持哨兵。
    assert_eq!(&got[2 * STRIDE + 1 * 4..2 * STRIDE + 1 * 4 + 3], &[SENT, SENT, SENT]);
    for y in 0..H as usize {
        for x in 9..W as usize {
            let off = y * STRIDE + x * 4;
            assert_eq!(&got[off..off + 3], &[SENT, SENT, SENT], "clip 外 ({x},{y}) 被写");
        }
    }
}
