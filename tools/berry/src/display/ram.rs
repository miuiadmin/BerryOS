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

/// Pure geometry half of the `open` ladder (no filesystem): zero
/// extents, stride < width*4 (u64 math — the old `width*4` u32 multiply
/// overflowed, panicking in debug on a corrupt boot line), PA below the
/// RAM base. Returns `(offset_into_file, bytes_needed)`.
pub(crate) fn fb_geometry(info: &FbInfo) -> Result<(u64, u64)> {
    if info.width == 0 || info.height == 0 {
        return Err(BerryError::protocol("RAM: bad fb geometry"));
    }
    if (info.stride as u64) < info.width as u64 * 4 {
        return Err(BerryError::protocol("RAM: bad fb geometry"));
    }
    if info.fb_pa < RAM_BASE {
        return Err(BerryError::protocol("RAM: fb PA below RAM base"));
    }
    Ok((info.fb_pa - RAM_BASE, info.stride as u64 * info.height as u64))
}

/// The full window bound: geometry plus the EOF check. `checked_add`: a
/// corrupt boot line can carry fb_pa near u64::MAX where offset+need
/// wraps and would sneak past the EOF gate.
pub(crate) fn fb_window(offset: u64, need: u64, file_len: u64) -> Result<()> {
    if offset.checked_add(need).is_none_or(|end| end > file_len) {
        return Err(BerryError::protocol(format!(
            "RAM: fb window {offset}+{need} outside {file_len}"
        )));
    }
    Ok(())
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
        // Geometry ladder first (pure, no filesystem), so a corrupt boot
        // line never produces an io error or an arithmetic panic.
        let (offset, need) = fb_geometry(&info)?;
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        fb_window(offset, need, len)?;
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

    #[test]
    fn parse_fb_line_rejections_and_fragments() {
        // Missing any required token -> None (the `?` on each Option).
        for bad in [
            "FB ok fmt=1 stride=1920 fb=0x43d00000",              // no WxH
            "FB ok 480x1000 stride=1920 fb=0x43d00000",           // no fmt
            "FB ok 480x1000 fmt=1 fb=0x43d00000",                 // no stride
            "FB ok 480x1000 fmt=1 stride=1920",                   // no fb
            "FB ok axb fmt=1 stride=1920 fb=0x43d00000",          // bad WxH
            "FB ok 480x1000 fmt=x stride=1920 fb=0x43d00000",     // bad fmt
            "FB ok 480x1000 fmt=1 stride=x fb=0x43d00000",        // bad stride
            "FB ok 480x1000 fmt=1 stride=1920 fb=zz",             // bad fb hex
            "",
        ] {
            assert!(FbInfo::parse(bad).is_none(), "must reject: {bad:?}");
        }
        // Tokens survive arbitrary order and surrounding junk (the loop is
        // token-oriented, not position-oriented).
        let i = FbInfo::parse(
            "junk fb=0000000043d00000 fmt=1 noise 480x1000 stride=1920 tail",
        )
        .unwrap();
        assert_eq!((i.width, i.height, i.stride, i.format), (480, 1000, 1920, 1));
        assert_eq!(i.fb_pa, 0x43d0_0000);
        // fb= without the 0x prefix still parses as hex; leading zeros ok.
        let i = FbInfo::parse("FB ok 2x2 fmt=1 stride=8 fb=43d00000").unwrap();
        assert_eq!(i.fb_pa, 0x43d0_0000);
        // A second WxH-looking token does not clobber: first wins? No — the
        // loop overwrites, so LAST wins; pin that either way is stable under
        // the real kernel line (single WxH token). Here: pin current
        // behavior (last wins) so a future refactor is a conscious change.
        let i = FbInfo::parse("FB ok 480x1000 800x600 fmt=1 stride=1920 fb=0x1").unwrap();
        assert_eq!((i.width, i.height), (800, 600));
        // u32::MAX geometry tokens parse (rejection happens later, in
        // fb_window) — parse itself must not panic on extremes.
        let i = FbInfo::parse("FB ok 4294967295x4294967295 fmt=1 stride=4294967295 fb=0xffffffffffffffff").unwrap();
        assert_eq!(i.width, u32::MAX);
        assert_eq!(i.fb_pa, u64::MAX);
    }

    /// Minimal scripted RFB peer backing a live VncInput (handshake only;
    /// further client messages relayed to the channel so send_input's
    /// wire bytes are assertable). Reuses vnc.rs's fake-RFB helpers —
    /// single source for the scripting discipline.
    fn vnc_input_stub() -> (VncInput, std::sync::mpsc::Receiver<Vec<u8>>) {
        use std::io::Read;
        use crate::net::Endpoint;
        let (tx, rx) = std::sync::mpsc::channel();
        let port = crate::display::vnc::tests::spawn_rfb(move |mut s| {
            let sw = crate::display::vnc::tests::server_write;
            sw(&mut s, b"RFB 003.008\n");
            let mut b = [0u8; 12];
            s.read_exact(&mut b).unwrap();
            sw(&mut s, &[1, 1]); // one security type: none
            let mut c = [0u8; 1];
            s.read_exact(&mut c).unwrap();
            sw(&mut s, &[0, 0, 0, 0]); // SecurityResult ok
            let mut ci = [0u8; 1]; // ClientInit
            s.read_exact(&mut ci).unwrap();
            // ServerInit: 4x2, 32bpp BGRX, name "t"
            let mut init = Vec::new();
            init.extend_from_slice(&4u16.to_be_bytes());
            init.extend_from_slice(&2u16.to_be_bytes());
            init.extend_from_slice(&[
                32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0,
            ]);
            init.extend_from_slice(&1u32.to_be_bytes());
            init.push(b't');
            sw(&mut s, &init);
            // Relay pointer/key messages from the client.
            let mut buf = [0u8; 64];
            loop {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let vnc = VncInput::connect_auth(
            &Endpoint::Tcp(format!("127.0.0.1:{port}")),
            None,
        )
        .unwrap();
        (vnc, rx)
    }

    fn geo(w: u32, h: u32, stride: u32, fb_pa: u64) -> FbInfo {
        FbInfo { fb_pa, width: w, height: h, stride, format: 1 }
    }

    #[test]
    fn fb_geometry_ladder() {
        // Happy path: offset = fb_pa - 0x4000_0000, need = stride*height.
        let (off, need) = fb_geometry(&geo(480, 1000, 1920, 0x43d0_0000)).unwrap();
        assert_eq!(off, 0x3d0_0000);
        assert_eq!(need, 1_920_000);
        // Stride may exceed width*4 (row padding is legal).
        let (off, need) = fb_geometry(&geo(4, 2, 32, 0x4000_0000)).unwrap();
        assert_eq!((off, need), (0, 64));
        // Rejections, in ladder order.
        for (bad, why) in [
            (geo(0, 100, 400, 0x4000_0000), "zero width"),
            (geo(480, 0, 1920, 0x4000_0000), "zero height"),
            (geo(4, 2, 15, 0x4000_0000), "stride < width*4"),
            // width*4 = 2^32 overflows the OLD u32 multiply — debug panic
            // before this fix; u64 math here must cleanly reject.
            (geo(0xFFFF_FFFF, 1, 8, 0x4000_0000), "width*4 u32 overflow"),
            (geo(4, 2, 16, 0x3FFF_FFFF), "PA below RAM base"),
            (geo(4, 2, 16, 0), "PA zero"),
        ] {
            let e = fb_geometry(&bad).unwrap_err();
            assert_eq!(e.code, 7, "{why}: {bad:?}");
        }
        // Exact boundary: stride == width*4 is legal; PA == RAM_BASE ok.
        let (off, _) = fb_geometry(&geo(4, 2, 16, 0x4000_0000)).unwrap();
        assert_eq!(off, 0);
    }

    #[test]
    fn fb_window_eof_and_overflow() {
        // Exact fit.
        fb_window(0, 32, 32).unwrap();
        // One byte short -> rc=7 with both numbers in the message.
        let e = fb_window(0, 32, 31).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("outside"), "msg: {}", e.message);
        // Offset pushed to the middle: 32+32 == 64 fits, len 63 does not.
        fb_window(32, 32, 64).unwrap();
        assert!(fb_window(32, 32, 63).is_err());
        // u64 add overflow: offset near u64::MAX plus a huge need must be
        // rejected (the old unchecked `offset + need` wrapped and sneaked
        // past the gate). offset = u64::MAX - 2^30, need = (2^32-1)^2.
        let offset = 0xFFFF_FFFF_FFFF_FFFFu64 - 0x4000_0000;
        let need = 0xFFFF_FFFFu64 * 0xFFFF_FFFFu64;
        let e = fb_window(offset, need, u64::MAX).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("outside"), "msg: {}", e.message);
    }

    #[test]
    fn ram_source_open_ladder_and_io() {
        // unwrap_err needs T: Debug and RamSource is not Debug; extract
        // the error side with expect_err_msg instead.
        let (vnc, _rx) = vnc_input_stub();
        fn open_err(
            path: &std::path::Path,
            info: FbInfo,
            vnc: VncInput,
        ) -> BerryError {
            match RamSource::open(path, info, vnc) {
                Err(e) => e,
                Ok(_) => panic!("open must fail: {info:?} {path:?}"),
            }
        }
        let missing = std::path::Path::new("/nonexistent/berry-ram-test/guest.ram");
        // Bad geometry is rejected BEFORE the file is touched: with a
        // nonexistent path the error must be the geometry one, not io.
        let e = open_err(missing, geo(0, 2, 8, 0x4000_0000), vnc_input_stub().0);
        assert!(e.message.contains("bad fb geometry"), "got: {}", e.message);
        let e = open_err(missing, geo(4, 2, 16, 0x3FFF_FFFF), vnc_input_stub().0);
        assert!(e.message.contains("below RAM base"), "got: {}", e.message);
        let _ = vnc;
        // Good geometry + missing file -> io error (code 6, device class).
        let e = open_err(missing, geo(4, 2, 16, 0x4000_0000), vnc_input_stub().0);
        assert_eq!(e.code, 6, "got: {} ({})", e.message, e.code);
        // Real file, window past EOF -> the "outside" rejection.
        let tmp = std::env::temp_dir().join(format!("berry-ram-open-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let small = tmp.join("small.ram");
        std::fs::write(&small, vec![0u8; 8]).unwrap();
        let e = open_err(&small, geo(4, 2, 16, 0x4000_0000), vnc_input_stub().0);
        assert!(e.message.contains("outside"), "got: {}", e.message);
        std::fs::remove_file(&small).unwrap();
        std::fs::remove_dir(&tmp).unwrap();
    }

    #[test]
    fn ram_source_scanout_data_plane() {
        // 4x2 BGRX framebuffer inside a fake guest-RAM file: window read,
        // sampled-signature change detection, per-format channel packing,
        // and the send_input routing — all without QEMU.
        let (vnc, rx) = vnc_input_stub();
        let tmp = std::env::temp_dir().join(format!("berry-ram-scanout-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let ram_path = tmp.join("guest.ram");
        let w = 4u32;
        let h = 2u32;
        let stride = 16u32;
        let mut ram = vec![0u8; 1024];
        let base = 64usize; // fb window offset inside the file
        let px = |ram: &mut Vec<u8>, o: usize, b: u8, g: u8, r: u8| {
            ram[base + o] = b;
            ram[base + o + 1] = g;
            ram[base + o + 2] = r;
            ram[base + o + 3] = 0;
        };
        // Row 0: red, green; row 1: blue, white (BGRX byte order).
        px(&mut ram, 0, 0x00, 0x00, 0xff);
        px(&mut ram, 4, 0x00, 0xff, 0x00);
        px(&mut ram, stride as usize, 0xff, 0x00, 0x00);
        px(&mut ram, stride as usize + 4, 0xff, 0xff, 0xff);
        std::fs::write(&ram_path, &ram).unwrap();
        let info = FbInfo { fb_pa: 0x4000_0000 + 64, width: w, height: h, stride, format: 1 };

        let mut src = RamSource::open(&ram_path, info, vnc).unwrap();
        assert_eq!(src.caps().kind, TransportKind::Shm);
        assert_eq!(src.caps().max_w, 4096);
        let f = src.try_next_frame().unwrap();
        assert_eq!((f.width, f.height), (4, 2));
        assert_eq!(f.seq, 1);
        assert_eq!(f.pixels[0], 0xff_0000); // red
        assert_eq!(f.pixels[1], 0x00_ff00); // green
        assert_eq!(f.pixels[4], 0x00_00ff); // blue (row 1, x=0)
        assert_eq!(f.pixels[5], 0xff_ffff); // white
        // Stride padding bytes (x=2..4 both rows) are skipped, not read.
        // Static frame: sampled signature unchanged -> None, seq frozen.
        assert!(src.try_next_frame().is_none());

        // Change one byte of pixel (0,0): blue channel 0x00 -> 0x11.
        ram[base + 0] = 0x11;
        std::fs::write(&ram_path, &ram).unwrap();
        let f = src.try_next_frame().unwrap();
        assert_eq!(f.seq, 2);
        assert_eq!(f.pixels[0], 0xff_0011); // R intact, B changed

        // Input routing: Pointer -> RFB message 5, Key -> message 4. TCP
        // may coalesce both into one relay read; split by message type.
        src.send_input(InputEvent::Pointer { x: 1, y: 2, buttons: 3 }).unwrap();
        src.send_input(InputEvent::Key { keysym: 0x61, down: true }).unwrap();
        let mut wire = Vec::new();
        while wire.len() < 14 {
            wire.extend(rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap());
        }
        assert_eq!(&wire[..6], &[5, 3, 0, 1, 0, 2]);
        assert_eq!(&wire[6..], &[4, 1, 0, 0, 0, 0, 0, 0x61]);

        // RGBX (format 0): R = byte0. Same file, fresh source.
        let info0 = FbInfo { format: 0, ..info };
        let mut src0 = RamSource::open(&ram_path, info0, vnc_input_stub().0).unwrap();
        let f0 = src0.try_next_frame().unwrap();
        // pixel(0,0) bytes are now b0=0x11,b1=0,b2=0xff -> (0x11<<16)|0xff.
        assert_eq!(f0.pixels[0], 0x11_00ff);
        // Unknown format code -> all-black pixels (pinned).
        let info7 = FbInfo { format: 7, ..info };
        let mut src7 = RamSource::open(&ram_path, info7, vnc_input_stub().0).unwrap();
        let f7 = src7.try_next_frame().unwrap();
        assert!(f7.pixels.iter().all(|&p| p == 0));

        std::fs::remove_file(&ram_path).unwrap();
        std::fs::remove_dir(&tmp).unwrap();
    }
}
