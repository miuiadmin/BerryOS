//! Transport-neutral screen model.
//!
//! A `Frame` is always host RGBA-in-u32 (`0x00RRGGBB`), regardless of whether
//! it came from VNC, shared memory, or a replay. This is the single pixel
//! truth the window and screenshot writers consume.

use std::time::{Duration, Instant};

use crate::error::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportKind {
    Vnc,
    Shm,
    Replay,
}

#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub version: u16,
    pub max_w: u32,
    pub max_h: u32,
    pub damage: bool,
    pub multi_touch: u8,
    pub cursor: bool,
    pub kind: TransportKind,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    /// `0x00RRGGBB`
    pub pixels: Vec<u32>,
}

impl Frame {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            seq: 0,
            width,
            height,
            pixels: vec![0; width as usize * height as usize],
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.pixels.resize(width as usize * height as usize, 0);
        }
    }

    pub fn to_rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 3);
        for &p in &self.pixels {
            out.push((p >> 16) as u8);
            out.push((p >> 8) as u8);
            out.push(p as u8);
        }
        out
    }
}

#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    Pointer { x: u16, y: u16, buttons: u8 },
    Key { keysym: u32, down: bool },
}

pub trait ScreenSource {
    fn caps(&self) -> Caps;
    fn try_next_frame(&mut self) -> Option<Frame>;

    fn next_frame(&mut self, timeout: Duration) -> Option<Frame> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(f) = self.try_next_frame() {
                return Some(f);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn send_input(&mut self, event: InputEvent) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— W1-Q25 六刀：frame.rs 零测面清偿。Frame 是 VNC/SHM/回放三传输的
    // 单一像素真相（截图/窗口/APNG 全消费它），几何律 + RGBA 装箱律 +
    // ScreenSource 默认超时循环三面钉板。

    #[test]
    fn frame_new_zeroed_geometry() {
        let f = Frame::new(4, 3);
        assert_eq!((f.width, f.height), (4, 3));
        assert_eq!(f.seq, 0, "fresh frame starts at seq 0");
        assert_eq!(f.pixels.len(), 12);
        assert!(f.pixels.iter().all(|&p| p == 0), "fresh frame is zeroed");
    }

    #[test]
    fn frame_new_zero_width_is_empty() {
        // 0×N / N×0 合法空帧：vec![0; 0] 不 panic——resize 判定与像素循环的
        // 空集前提。
        let f = Frame::new(0, 7);
        assert_eq!(f.pixels.len(), 0);
        assert_eq!(f.to_rgb().len(), 0);
    }

    #[test]
    fn to_rgb_bytes_per_pixel_and_channel_order() {
        // 0x00RRGGBB → RGB 三字节/像素：通道序与压缩长度同钉。
        let mut f = Frame::new(2, 1);
        f.pixels[0] = 0x00_ff_66_03; // R=0xff G=0x66 B=0x03
        f.pixels[1] = 0x112233;
        let rgb = f.to_rgb();
        assert_eq!(rgb, vec![0xff, 0x66, 0x03, 0x11, 0x22, 0x33]);
    }

    #[test]
    fn to_rgb_drops_alpha_byte() {
        // 高 8 位（0x00RRGGBB 的 alpha 槽）不得漏进输出——VNC BGRX 帧转
        // PNG 写入前的字节契约。
        let mut f = Frame::new(1, 1);
        f.pixels[0] = 0xAB_00_00_00;
        assert_eq!(f.to_rgb(), vec![0x00, 0x00, 0x00]);
    }

    #[test]
    fn resize_same_geometry_is_noop() {
        // 同几何 resize 不动缓冲：像素内容保持（同尺寸早退分支）。
        let mut f = Frame::new(2, 2);
        f.pixels.fill(0xabcdef);
        f.resize(2, 2);
        assert_eq!(f.pixels.len(), 4);
        assert!(f.pixels.iter().all(|&p| p == 0xabcdef));
    }

    #[test]
    fn resize_grow_zero_fills_and_shrink_truncates() {
        // 增长补 0、收缩截断：Vec::resize 语义钉板（增量 0 填充）。
        let mut f = Frame::new(2, 2);
        f.pixels.fill(0xff);
        f.resize(2, 3);
        assert_eq!(f.pixels.len(), 6);
        assert!(f.pixels[..4].iter().all(|&p| p == 0xff), "old rows keep content");
        assert!(f.pixels[4..].iter().all(|&p| p == 0), "new row is zero-filled");
        f.resize(1, 2);
        assert_eq!((f.width, f.height), (1, 2));
        assert_eq!(f.pixels.len(), 2);
        assert!(f.pixels.iter().all(|&p| p == 0xff), "truncation keeps head content");
    }

    /// ScreenSource 双腿测试件：可配「先空转 N 次 poll 再出帧」。
    struct FakeSource {
        frames: std::collections::VecDeque<Frame>,
        delay_polls: usize,
    }

    impl ScreenSource for FakeSource {
        fn caps(&self) -> Caps {
            Caps {
                version: 1,
                max_w: 640,
                max_h: 480,
                damage: true,
                multi_touch: 2,
                cursor: true,
                kind: TransportKind::Replay,
            }
        }

        fn try_next_frame(&mut self) -> Option<Frame> {
            if self.delay_polls > 0 {
                self.delay_polls -= 1;
                return None;
            }
            self.frames.pop_front()
        }

        fn send_input(&mut self, _event: InputEvent) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn next_frame_returns_immediately_when_ready() {
        let mut src = FakeSource { frames: vec![Frame::new(2, 2)].into(), delay_polls: 0 };
        let f = src
            .next_frame(Duration::from_secs(5))
            .expect("ready frame must come back without waiting");
        assert_eq!((f.width, f.height), (2, 2));
    }

    #[test]
    fn next_frame_polls_until_delayed_frame_appears() {
        // 默认循环轮询语义：空转若干 poll 后帧到达 → 返回该帧而非超时 None。
        let mut src = FakeSource { frames: vec![Frame::new(1, 1)].into(), delay_polls: 3 };
        let f = src.next_frame(Duration::from_secs(5)).expect("frame after polls");
        assert_eq!(f.width, 1);
    }

    #[test]
    fn next_frame_times_out_when_never_ready() {
        // 纯空源 + 有界超时 → None（非阻塞不 panic）。10ms 量级保证测试快。
        let mut src = FakeSource { frames: Default::default(), delay_polls: usize::MAX };
        let got = src.next_frame(Duration::from_millis(10));
        assert!(got.is_none(), "starved source must time out with None");
    }

    #[test]
    fn next_frame_exhausted_then_timeout() {
        // 帧耗尽后继续轮询直至超时：第二帧缺席不 panic。
        let mut src = FakeSource { frames: vec![Frame::new(1, 1)].into(), delay_polls: 0 };
        assert!(src.next_frame(Duration::from_secs(5)).is_some());
        assert!(src.next_frame(Duration::from_millis(5)).is_none());
    }

    #[test]
    fn transport_kind_copy_eq() {
        // Copy/Eq 派生面：三传输枚举可比较可复制（caps 面的消费前提）。
        assert_eq!(TransportKind::Vnc, TransportKind::Vnc);
        assert_ne!(TransportKind::Shm, TransportKind::Replay);
        let k = TransportKind::Vnc;
        let k2 = k;
        assert_eq!(k, k2);
    }
}
