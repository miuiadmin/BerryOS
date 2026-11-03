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
