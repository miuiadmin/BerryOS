//! BerryGFX C1：DisplayList 嵌套裁剪栈语义。
//!
//! 覆盖：Push/Pop 精确裁剪、嵌套求交、Pop 后恢复外层、失衡/超深先验
//! 拒绝且零半执行、负矩形在录入期拒绝。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, Rect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::{Queue, CLIP_STACK_MAX};

const W: i32 = 16;
const H: i32 = 16;
const STRIDE: usize = 64;
const SENTINEL: u32 = 0xEEEE_EEEE;

const RED: Color = Color { r: 255, g: 0, b: 0 };
const GREEN: Color = Color { r: 0, g: 255, b: 0 };
const BLUE: Color = Color { r: 0, g: 0, b: 255 };

fn desc() -> TextureDesc {
    TextureDesc::new(W as usize, H as usize, STRIDE, TextureFormat::Rgbx).unwrap()
}

fn px(fb: &[u8], x: i32, y: i32) -> u32 {
    let off = y as usize * STRIDE + x as usize * 4;
    u32::from_le_bytes([fb[off], fb[off + 1], fb[off + 2], fb[off + 3]])
}

fn submit(enc: CommandEncoder<'_>, fb: &mut [u8]) {
    Queue::new()
        .submit(
            &enc.finish(),
            fb,
            &desc(),
            &PassDescriptor::new(LoadOp::Load, StoreOp::Store, None),
        )
        .unwrap();
}

#[test]
fn clip_stack_clips_and_restores() {
    let mut enc = CommandEncoder::new();
    enc.push_clip(Rect::new(0, 0, 8, 16)).unwrap();
    enc.fill_rect(Rect::new(0, 0, 16, 16), RED).unwrap();
    enc.pop_clip().unwrap();
    // 只补 clip 外区域；若 Pop 失效，clip 内也会被绿覆盖。
    enc.fill_rect(Rect::new(8, 0, 8, 16), GREEN).unwrap();
    let mut fb = vec![0xEEu8; STRIDE * H as usize];
    submit(enc, &mut fb);
    for y in 0..H {
        assert_eq!(px(&fb, 3, y), 0x00FF_0000, "clip 内红");
        assert_eq!(px(&fb, 11, y), 0x0000_FF00, "Pop 后恢复外层绿");
    }
}

#[test]
fn nested_clips_intersect() {
    let mut enc = CommandEncoder::new();
    enc.push_clip(Rect::new(0, 0, 16, 8)).unwrap();
    enc.push_clip(Rect::new(4, 0, 8, 16)).unwrap();
    enc.fill_rect(Rect::new(0, 0, 16, 16), BLUE).unwrap();
    enc.pop_clip().unwrap();
    // 只补内层 clip 外、外层 clip 内的左右边带；中间蓝保留证明嵌套交集。
    enc.fill_rect(Rect::new(0, 0, 4, 8), RED).unwrap();
    enc.fill_rect(Rect::new(12, 0, 4, 8), RED).unwrap();
    enc.pop_clip().unwrap();
    let mut fb = vec![0xEEu8; STRIDE * H as usize];
    submit(enc, &mut fb);
    for y in 0..H {
        for x in 0..W {
            let expected = if x >= 4 && x < 12 && y < 8 {
                0x0000_00FF
            } else if y < 8 {
                0x00FF_0000
            } else {
                SENTINEL
            };
            assert_eq!(px(&fb, x, y), expected, "@({x},{y})");
        }
    }
}

#[test]
fn unbalanced_clip_rejects_before_writes() {
    let mut enc = CommandEncoder::new();
    enc.fill_rect(Rect::new(0, 0, 16, 16), RED).unwrap();
    enc.pop_clip().unwrap();
    let mut fb = vec![0xEEu8; STRIDE * H as usize];
    let err = Queue::new()
        .submit(
            &enc.finish(),
            &mut fb,
            &desc(),
            &PassDescriptor::load(),
        )
        .unwrap_err();
    assert!(matches!(err, berry_gfx::queue::SubmitError::UnbalancedClip { index: 1 }));
    assert_eq!(px(&fb, 0, 0), SENTINEL, "先验拒：半执行不写红");

    let mut enc = CommandEncoder::new();
    enc.push_clip(Rect::new(0, 0, 8, 8)).unwrap();
    let mut fb = vec![0xEEu8; STRIDE * H as usize];
    let err = Queue::new()
        .submit(
            &enc.finish(),
            &mut fb,
            &desc(),
            &PassDescriptor::load(),
        )
        .unwrap_err();
    assert!(matches!(err, berry_gfx::queue::SubmitError::UnbalancedClip { index: 1 }));
    assert_eq!(px(&fb, 0, 0), SENTINEL, "未归零同样零写入");
}

#[test]
fn clip_stack_capacity_loud() {
    let mut enc = CommandEncoder::new();
    for _ in 0..CLIP_STACK_MAX {
        enc.push_clip(Rect::new(0, 0, 16, 16)).unwrap();
    }
    enc.push_clip(Rect::new(0, 0, 16, 16)).unwrap();
    let mut fb = vec![0xEEu8; STRIDE * H as usize];
    let err = Queue::new()
        .submit(
            &enc.finish(),
            &mut fb,
            &desc(),
            &PassDescriptor::load(),
        )
        .unwrap_err();
    assert!(matches!(err, berry_gfx::queue::SubmitError::ClipStackFull { index: CLIP_STACK_MAX }));
    assert_eq!(px(&fb, 0, 0), SENTINEL);
}

#[test]
fn push_clip_rejects_negative_rect_at_record() {
    let mut enc = CommandEncoder::new();
    assert_eq!(
        enc.push_clip(Rect::new(0, 0, -1, 8)),
        Err(berry_gfx::cmd::CmdError::BadRect)
    );
    assert_eq!(enc.record(Cmd::PopClip), Ok(()));
}

#[test]
fn command_full_missing_pop_rejects_whole_list() {
    let mut enc = CommandEncoder::new();
    enc.push_clip(Rect::new(0, 0, 8, 8)).unwrap();
    for _ in 0..berry_gfx::cmd::MAX_CMDS - 1 {
        enc.fill_rect(Rect::new(0, 0, 1, 1), RED).unwrap();
    }
    assert_eq!(enc.pop_clip(), Err(berry_gfx::cmd::CmdError::Full));
    let mut fb = vec![0xEEu8; STRIDE * H as usize];
    let err = Queue::new()
        .submit(
            &enc.finish(),
            &mut fb,
            &desc(),
            &PassDescriptor::load(),
        )
        .unwrap_err();
    assert!(matches!(err, berry_gfx::queue::SubmitError::UnbalancedClip { .. }));
    assert_eq!(px(&fb, 0, 0), SENTINEL, "满帽缺 Pop：整表拒绝零半帧");
}
