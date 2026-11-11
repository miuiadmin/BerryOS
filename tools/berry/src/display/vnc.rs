//! RFB 3.8 client, deliberately limited to what QEMU's VNC server emits:
//! Raw rectangles, DesktopSize, and the three input messages. This is the v0
//! `ScreenSource`; a shared-memory source will implement the same trait later.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::display::frame::{Caps, Frame, InputEvent, ScreenSource, TransportKind};
use crate::error::{BerryError, Result};
use crate::net::{Endpoint, Stream};

const MAX_DIM: u32 = 4096;
const MAX_PIXELS: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct PixelFormat {
    bpp: u8,
    big_endian: bool,
    true_color: bool,
    rmax: u16,
    gmax: u16,
    bmax: u16,
    rshift: u8,
    gshift: u8,
    bshift: u8,
}

impl PixelFormat {
    fn parse(b: &[u8]) -> Result<Self> {
        if b.len() < 16 {
            return Err(BerryError::protocol("VNC: short pixel format"));
        }
        let pf = Self {
            bpp: b[0],
            big_endian: b[2] != 0,
            true_color: b[3] != 0,
            rmax: u16::from_be_bytes([b[4], b[5]]),
            gmax: u16::from_be_bytes([b[6], b[7]]),
            bmax: u16::from_be_bytes([b[8], b[9]]),
            rshift: b[10],
            gshift: b[11],
            bshift: b[12],
        };
        if !pf.true_color {
            return Err(BerryError::protocol("VNC: colour-map formats unsupported"));
        }
        if !matches!(pf.bpp, 8 | 16 | 32) {
            return Err(BerryError::protocol(format!(
                "VNC: unsupported bpp {}",
                pf.bpp
            )));
        }
        Ok(pf)
    }

    fn bytes(&self) -> usize {
        self.bpp as usize / 8
    }

    fn pixel(&self, b: &[u8]) -> u32 {
        let v = match self.bpp {
            32 => {
                let a = [b[0], b[1], b[2], b[3]];
                if self.big_endian {
                    u32::from_be_bytes(a)
                } else {
                    u32::from_le_bytes(a)
                }
            }
            16 => {
                let a = [b[0], b[1]];
                let v = if self.big_endian {
                    u16::from_be_bytes(a)
                } else {
                    u16::from_le_bytes(a)
                };
                v as u32
            }
            _ => b[0] as u32,
        };
        let r = scale((v >> self.rshift) & self.rmax as u32, self.rmax as u32);
        let g = scale((v >> self.gshift) & self.gmax as u32, self.gmax as u32);
        let bl = scale((v >> self.bshift) & self.bmax as u32, self.bmax as u32);
        (r << 16) | (g << 8) | bl
    }
}

fn scale(v: u32, max: u32) -> u32 {
    if max == 0 || max == 255 {
        v.min(255)
    } else {
        (v * 255 + max / 2) / max
    }
}

struct Reader {
    slot: Arc<Mutex<Option<Frame>>>,
    writer: Arc<Mutex<Stream>>,
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    pf: PixelFormat,
    frame: Frame,
    seq: u64,
}

/// RFB 3.8 handshake (no encodings/request): version, security None, shared
/// ClientInit, ServerInit. Returns framebuffer size + pixel format.
fn handshake(stream: &mut Stream) -> Result<(u32, u32, PixelFormat)> {
    let mut ver = [0u8; 12];
    read_exact(stream, &mut ver)?;
    if !ver.starts_with(b"RFB 003.") {
        return Err(BerryError::protocol(format!(
            "VNC: bad version {:?}",
            String::from_utf8_lossy(&ver)
        )));
    }
    stream.write_all(b"RFB 003.008\n")?;
    stream.flush()?;

    let mut n = [0u8; 1];
    read_exact(stream, &mut n)?;
    if n[0] == 0 {
        let mut len = [0u8; 4];
        read_exact(stream, &mut len)?;
        let mut reason = vec![0u8; u32::from_be_bytes(len) as usize];
        read_exact(stream, &mut reason)?;
        return Err(BerryError::protocol(format!(
            "VNC: refused: {}",
            String::from_utf8_lossy(&reason)
        )));
    }
    let mut types = vec![0u8; n[0] as usize];
    read_exact(stream, &mut types)?;
    if !types.contains(&1) {
        return Err(BerryError::protocol("VNC: server requires authentication")
            .with_hint("use a Unix socket (no auth) or configure password=off on loopback"));
    }
    stream.write_all(&[1])?;
    stream.flush()?;
    let mut result = [0u8; 4];
    read_exact(stream, &mut result)?;
    if u32::from_be_bytes(result) != 0 {
        return Err(BerryError::protocol("VNC: security handshake failed"));
    }

    stream.write_all(&[1])?; // shared
    stream.flush()?;
    let mut init = [0u8; 24];
    read_exact(stream, &mut init)?;
    let w = u16::from_be_bytes([init[0], init[1]]) as u32;
    let h = u16::from_be_bytes([init[2], init[3]]) as u32;
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
        return Err(BerryError::protocol(format!("VNC: bad size {w}x{h}")));
    }
    let pf = PixelFormat::parse(&init[4..20])?;
    let name_len = u32::from_be_bytes([init[20], init[21], init[22], init[23]]) as usize;
    if name_len > 4096 {
        return Err(BerryError::protocol("VNC: bad name length"));
    }
    let mut name = vec![0u8; name_len];
    if name_len > 0 {
        read_exact(stream, &mut name)?;
    }

    Ok((w, h, pf))
}

pub struct VncSource {
    slot: Arc<Mutex<Option<Frame>>>,
    writer: Arc<Mutex<Stream>>,
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    handle: Option<JoinHandle<()>>,
    caps: Caps,
}

impl VncSource {
    pub fn connect(endpoint: &Endpoint) -> Result<Self> {
        let mut stream = connect_retry(endpoint, Duration::from_secs(10))?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;

        let (w, h, pf) = handshake(&mut stream)?;

        // Encodings: Raw (0) + DesktopSize (-223). Cursor is deliberately not
        // requested; the host draws its own touch indicator.
        let mut enc = vec![2u8, 0];
        enc.extend_from_slice(&2u16.to_be_bytes());
        enc.extend_from_slice(&0i32.to_be_bytes());
        enc.extend_from_slice(&(-223i32).to_be_bytes());
        stream.write_all(&enc)?;
        stream.flush()?;
        stream.write_all(&request(0, 0, 0, w, h))?;
        stream.flush()?;

        stream.set_read_timeout(None)?;
        let writer = Arc::new(Mutex::new(stream.try_clone()?));
        let slot = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let mut reader = Reader {
            slot: slot.clone(),
            writer: writer.clone(),
            stop: stop.clone(),
            error: error.clone(),
            pf,
            frame: Frame::new(w, h),
            seq: 0,
        };
        let handle = std::thread::spawn(move || {
            if let Err(e) = reader.run(&mut stream) {
                if !reader.stop.load(Ordering::Relaxed) {
                    *reader.error.lock().unwrap() = Some(e.message);
                }
            }
        });

        Ok(Self {
            slot,
            writer,
            stop,
            error,
            handle: Some(handle),
            caps: Caps {
                version: 1,
                max_w: MAX_DIM,
                max_h: MAX_DIM,
                damage: false,
                multi_touch: 0,
                cursor: false,
                kind: TransportKind::Vnc,
            },
        })
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
}

impl Drop for VncSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(w) = self.writer.lock() {
            w.shutdown();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Input-only RFB client: performs the handshake but never requests a frame
/// and never advertises encodings. QEMU supports multiple VNC clients, so the
/// shell and `berry input` can coexist while frames come from shared RAM.
pub struct VncInput {
    stream: Stream,
    width: u32,
    height: u32,
}

impl VncInput {
    pub fn connect(endpoint: &Endpoint) -> Result<Self> {
        let mut stream = connect_retry(endpoint, Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let (w, h, _) = handshake(&mut stream)?;
        stream.set_read_timeout(None)?;
        Ok(Self { stream, width: w, height: h })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn pointer(&mut self, x: u16, y: u16, buttons: u8) -> Result<()> {
        let mut m = [5u8, buttons, 0, 0, 0, 0];
        m[2..4].copy_from_slice(&x.to_be_bytes());
        m[4..6].copy_from_slice(&y.to_be_bytes());
        self.stream.write_all(&m)?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn key(&mut self, keysym: u32, down: bool) -> Result<()> {
        let mut m = vec![4u8, u8::from(down), 0, 0];
        m.extend_from_slice(&keysym.to_be_bytes());
        self.stream.write_all(&m)?;
        self.stream.flush()?;
        Ok(())
    }
}

impl ScreenSource for VncSource {
    fn caps(&self) -> Caps {
        self.caps
    }

    fn try_next_frame(&mut self) -> Option<Frame> {
        self.slot.lock().unwrap().take()
    }

    fn send_input(&mut self, event: InputEvent) -> Result<()> {
        let mut w = self.writer.lock().unwrap();
        let msg = match event {
            InputEvent::Pointer { x, y, buttons } => {
                let mut m = [5u8, buttons, 0, 0, 0, 0];
                m[2..4].copy_from_slice(&x.to_be_bytes());
                m[4..6].copy_from_slice(&y.to_be_bytes());
                m.to_vec()
            }
            InputEvent::Key { keysym, down } => {
                let mut m = vec![4u8, u8::from(down), 0, 0];
                m.extend_from_slice(&keysym.to_be_bytes());
                m
            }
        };
        w.write_all(&msg)?;
        w.flush()?;
        Ok(())
    }
}

impl Reader {
    fn run(&mut self, stream: &mut Stream) -> Result<()> {
        while !self.stop.load(Ordering::Relaxed) {
            let mut t = [0u8; 1];
            read_exact(stream, &mut t)?;
            match t[0] {
                0 => {
                    self.update(stream)?;
                    let mut w = self.writer.lock().unwrap();
                    let req = request(
                        1,
                        0,
                        0,
                        self.frame.width,
                        self.frame.height,
                    );
                    w.write_all(&req)?;
                    w.flush()?;
                }
                1 => {
                    let mut hdr = [0u8; 5];
                    read_exact(stream, &mut hdr)?;
                    let n = u16::from_be_bytes([hdr[3], hdr[4]]) as usize;
                    let mut data = vec![0u8; n * 6];
                    read_exact(stream, &mut data)?;
                }
                2 => {}
                3 => {
                    let mut hdr = [0u8; 7];
                    read_exact(stream, &mut hdr)?;
                    let n = u32::from_be_bytes([hdr[3], hdr[4], hdr[5], hdr[6]]) as usize;
                    if n > 1 << 20 {
                        return Err(BerryError::protocol("VNC: cut text too long"));
                    }
                    let mut data = vec![0u8; n];
                    read_exact(stream, &mut data)?;
                }
                other => {
                    return Err(BerryError::protocol(format!(
                        "VNC: unknown message {other}"
                    )))
                }
            }
        }
        Ok(())
    }

    fn update(&mut self, stream: &mut Stream) -> Result<()> {
        let mut hdr = [0u8; 3];
        read_exact(stream, &mut hdr)?;
        let n = u16::from_be_bytes([hdr[1], hdr[2]]) as usize;
        if n > 1024 {
            return Err(BerryError::protocol("VNC: too many rects"));
        }
        for _ in 0..n {
            let mut r = [0u8; 12];
            read_exact(stream, &mut r)?;
            let x = u16::from_be_bytes([r[0], r[1]]) as u32;
            let y = u16::from_be_bytes([r[2], r[3]]) as u32;
            let w = u16::from_be_bytes([r[4], r[5]]) as u32;
            let h = u16::from_be_bytes([r[6], r[7]]) as u32;
            let enc = i32::from_be_bytes([r[8], r[9], r[10], r[11]]);
            match enc {
                0 => self.raw(stream, x, y, w, h)?,
                -223 => {
                    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
                        return Err(BerryError::protocol("VNC: bad DesktopSize"));
                    }
                    self.frame.resize(w, h);
                }
                -239 => {
                    let px = w as u64 * h as u64 * self.pf.bytes() as u64;
                    let mask = ((w as u64 + 7) / 8) * h as u64;
                    if px + mask > MAX_PIXELS * 4 {
                        return Err(BerryError::protocol("VNC: cursor too large"));
                    }
                    let mut data = vec![0u8; (px + mask) as usize];
                    read_exact(stream, &mut data)?;
                }
                other => {
                    return Err(BerryError::protocol(format!(
                        "VNC: unsupported encoding {other}"
                    )))
                }
            }
        }
        self.seq += 1;
        self.frame.seq = self.seq;
        *self.slot.lock().unwrap() = Some(self.frame.clone());
        Ok(())
    }

    fn raw(&mut self, stream: &mut Stream, x: u32, y: u32, w: u32, h: u32) -> Result<()> {
        if w == 0 || h == 0 {
            return Ok(());
        }
        if x + w > self.frame.width || y + h > self.frame.height {
            return Err(BerryError::protocol(format!(
                "VNC: rect {x},{y} {w}x{h} outside {}x{}",
                self.frame.width, self.frame.height
            )));
        }
        let px = w as u64 * h as u64;
        if px > MAX_PIXELS {
            return Err(BerryError::protocol("VNC: rect too large"));
        }
        let bytes = px as usize * self.pf.bytes();
        let mut data = vec![0u8; bytes];
        read_exact(stream, &mut data)?;
        let step = self.pf.bytes();
        let stride = self.frame.width as usize;
        for row in 0..h as usize {
            let src = row * w as usize * step;
            let dst = (y as usize + row) * stride + x as usize;
            for col in 0..w as usize {
                let off = src + col * step;
                self.frame.pixels[dst + col] = self.pf.pixel(&data[off..off + step]);
            }
        }
        Ok(())
    }
}

fn request(incremental: u8, x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
    let mut m = vec![3u8, incremental];
    m.extend_from_slice(&(x as u16).to_be_bytes());
    m.extend_from_slice(&(y as u16).to_be_bytes());
    m.extend_from_slice(&(w as u16).to_be_bytes());
    m.extend_from_slice(&(h as u16).to_be_bytes());
    m
}

fn read_exact(s: &mut Stream, buf: &mut [u8]) -> Result<()> {
    s.read_exact(buf).map_err(|e| {
        if matches!(
            e.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ) {
            BerryError::timeout("VNC: read timeout")
        } else {
            BerryError::protocol(format!("VNC read: {e}"))
        }
    })
}

fn connect_retry(endpoint: &Endpoint, timeout: Duration) -> Result<Stream> {
    let start = Instant::now();
    loop {
        match endpoint.connect() {
            Ok(s) => return Ok(s),
            Err(e) if start.elapsed() < timeout => {
                let _ = e;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                return Err(BerryError::protocol(format!(
                    "VNC connect {}: {e}",
                    endpoint.display()
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_bgrx() {
        let pf = PixelFormat {
            bpp: 32,
            big_endian: false,
            true_color: true,
            rmax: 255,
            gmax: 255,
            bmax: 255,
            rshift: 16,
            gshift: 8,
            bshift: 0,
        };
        assert_eq!(pf.pixel(&[0x11, 0x22, 0x33, 0x00]), 0x0033_2211);
    }

    #[test]
    fn pixel_565() {
        let pf = PixelFormat {
            bpp: 16,
            big_endian: false,
            true_color: true,
            rmax: 31,
            gmax: 63,
            bmax: 31,
            rshift: 11,
            gshift: 5,
            bshift: 0,
        };
        // r=31, g=63, b=31 -> 0xffff -> white
        assert_eq!(pf.pixel(&[0xff, 0xff]), 0x00ff_ffff);
    }
}
