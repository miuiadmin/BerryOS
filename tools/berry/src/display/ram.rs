//! Shared-RAM scanout: QEMU `memory-backend-file,share=on` exposes guest RAM
//! as a host file; the guest framebuffer is just an offset inside it. This is
//! the v0.5 data plane — no VNC frame encoding, no per-frame guest commit,
//! and it sees exactly what the guest wrote (the same bytes the display
//! controller scans out).
//!
//! Geometry/PA come from the kernel's boot line:
//!   `FB ok 480x1000 fmt=1 stride=1920 fb=0000000043d00000`
//! The line is printed once at boot; `berry shell` waits for it.

use std::fs::File;
use std::io;
use std::path::Path;

use crate::display::frame::{Caps, Frame, InputEvent, ScreenSource, TransportKind};
use crate::display::vnc::VncInput;
use crate::error::{BerryError, Result};

const RAM_BASE: u64 = 0x4000_0000;

#[derive(Clone, Copy, Debug)]
pub struct FbInfo {
    pub fb_pa: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
}

impl FbInfo {
    /// Parse the kernel's `FB ok <w>x<h> fmt=<f> stride=<s> fb=0x...` line.
    pub fn parse(line: &str) -> Option<Self> {
        let mut w = None;
        let mut h = None;
        let mut fmt = None;
        let mut stride = None;
        let mut fb = None;
        for tok in line.split_whitespace() {
            if let Some(v) = tok.strip_prefix("fmt=") {
                fmt = v.parse().ok();
            } else if let Some(v) = tok.strip_prefix("stride=") {
                stride = v.parse().ok();
            } else if let Some(v) = tok.strip_prefix("fb=") {
                let v = v.strip_prefix("0x").unwrap_or(v);
                fb = u64::from_str_radix(v, 16).ok();
            } else if let Some((a, b)) = tok.split_once('x') {
                if let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) {
                    w = Some(a);
                    h = Some(b);
                }
            }
        }
        Some(Self {
            fb_pa: fb?,
            width: w?,
            height: h?,
            stride: stride?,
            format: fmt?,
        })
    }
}

pub struct RamSource {
    file: File,
    offset: u64,
    info: FbInfo,
    vnc: VncInput,
    raw: Vec<u8>,
    seq: u64,
    last_sig: u64,
}

impl RamSource {
    pub fn open(path: &Path, info: FbInfo, vnc: VncInput) -> Result<Self> {
        if info.width == 0 || info.height == 0 || info.stride < info.width * 4 {
            return Err(BerryError::protocol("RAM: bad fb geometry"));
        }
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        if info.fb_pa < RAM_BASE {
            return Err(BerryError::protocol("RAM: fb PA below RAM base"));
        }
        let offset = info.fb_pa - RAM_BASE;
        let need = info.stride as u64 * info.height as u64;
        if offset + need > len {
            return Err(BerryError::protocol(format!(
                "RAM: fb window {offset}+{need} outside {len}"
            )));
        }
        Ok(Self {
            file,
            offset,
            info,
            vnc,
            raw: vec![0; need as usize],
            seq: 0,
            last_sig: 0,
        })
    }

    fn read_at(file: &File, buf: &mut [u8], off: u64) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            file.read_exact_at(buf, off)
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileExt;
            let mut done = 0;
            while done < buf.len() {
                let n = file.seek_read(&mut buf[done..], off + done as u64)?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "ram eof"));
                }
                done += n;
            }
            Ok(())
        }
    }
}

impl ScreenSource for RamSource {
    fn caps(&self) -> Caps {
        Caps {
            version: 1,
            max_w: 4096,
            max_h: 4096,
            damage: false,
            multi_touch: 0,
            cursor: false,
            kind: TransportKind::Shm,
        }
    }

    fn try_next_frame(&mut self) -> Option<Frame> {
        Self::read_at(&self.file, &mut self.raw, self.offset).ok()?;
        // Skip static frames with a cheap sampled signature; the console/frame
        // is otherwise re-rendered 60x/s for no reason.
        let mut sig = 0xcbf2_9ce4_8422_2325u64;
        let step = (self.raw.len() / 4 / 512).max(1);
        let mut i = 0;
        while i + 3 < self.raw.len() {
            let v = u32::from_le_bytes([
                self.raw[i],
                self.raw[i + 1],
                self.raw[i + 2],
                self.raw[i + 3],
            ]);
            sig ^= v as u64;
            sig = sig.wrapping_mul(0x100_0000_01b3);
            i += step * 4;
        }
        if sig == self.last_sig {
            return None;
        }
        self.last_sig = sig;
        self.seq += 1;
        let mut frame = Frame::new(self.info.width, self.info.height);
        frame.seq = self.seq;
        let w = self.info.width as usize;
        let h = self.info.height as usize;
        let stride = self.info.stride as usize;
        for y in 0..h {
            let row = y * stride;
            for x in 0..w {
                let o = row + x * 4;
                let b0 = self.raw[o] as u32;
                let b1 = self.raw[o + 1] as u32;
                let b2 = self.raw[o + 2] as u32;
                frame.pixels[y * w + x] = match self.info.format {
                    0 => (b0 << 16) | (b1 << 8) | b2,       // RGBX
                    1 => (b2 << 16) | (b1 << 8) | b0,       // BGRX
                    _ => 0,
                };
            }
        }
        Some(frame)
    }

    fn send_input(&mut self, event: InputEvent) -> Result<()> {
        match event {
            InputEvent::Pointer { x, y, buttons } => self.vnc.pointer(x, y, buttons),
            InputEvent::Key { keysym, down } => self.vnc.key(keysym, down),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fb_line() {
        let i = FbInfo::parse(
            "FB ok 480x1000 fmt=1 stride=1920 fb=0000000043d00000",
        )
        .unwrap();
        assert_eq!(i.width, 480);
        assert_eq!(i.height, 1000);
        assert_eq!(i.stride, 1920);
        assert_eq!(i.fb_pa, 0x43d0_0000);
    }
}
