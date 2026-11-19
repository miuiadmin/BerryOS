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
    /// Whether the reader sends incremental framebuffer requests after
    /// each update. False for TLS sessions, where the single rustls
    /// session is shared under one mutex and `vnc check` only needs the
    /// first full frame.
    request_more: bool,
}

/// Security type actually used for a session (RFB 3.8): 1 (None) or 2
/// (VNC auth). `vnc check` reports this so a password passed against an
/// open endpoint shows the truth (`auth none`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthKind {
    None,
    Vnc,
}

impl AuthKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthKind::None => "none",
            AuthKind::Vnc => "vnc",
        }
    }
}

// ---------------------------------------------------------------------------
// VeNCrypt (RFC-none; the de-facto spec + QEMU 11.0.3 wire truth, probe p2)
// ---------------------------------------------------------------------------

/// RFB security type carrying the VeNCrypt extension.
pub const VENCRYPT_TYPE: u8 = 19;
/// VeNCrypt version berry speaks (both directions on the wire: S->C offer,
/// C->S reply; ack byte 0x00 follows, then the u8 subtype count).
pub const VENCRYPT_VERSION: [u8; 2] = [0x00, 0x02];
/// VeNCrypt ack byte (server accepts the negotiated version).
pub const VENCRYPT_ACK_OK: u8 = 0x00;
/// VeNCrypt subtype accept status (server accepted the chosen subtype).
pub const VENCRYPT_ACCEPTED: u8 = 0x01;
/// X509None: TLS with no inner VNC auth.
pub const VENCRYPT_SUBTYPE_X509NONE: u32 = 260;
/// X509Vnc: TLS with the classic DES challenge inside.
pub const VENCRYPT_SUBTYPE_X509VNC: u32 = 261;

/// Highest common VeNCrypt version between the server's offer and berry's
/// 0.2: None when the server is strictly older (no common ground, the
/// client must abort).
pub fn vencrypt_common_version(server: [u8; 2]) -> Option<[u8; 2]> {
    let (maj, min) = (server[0], server[1]);
    if maj > 0 || (maj == 0 && min >= 2) {
        Some(VENCRYPT_VERSION)
    } else {
        None
    }
}

/// Subtype preference matrix (pure; unit-pinned): X509Vnc (261) whenever a
/// password is available, else X509None (260) as the unauthenticated
/// fallback; 261 without a password is a hard error with the password
/// hint; anything else means the server's subtype set is unsupported.
pub fn vencrypt_choose_subtype(offered: &[u32], password: Option<&str>) -> Result<(u32, AuthKind)> {
    if password.is_some() && offered.contains(&VENCRYPT_SUBTYPE_X509VNC) {
        return Ok((VENCRYPT_SUBTYPE_X509VNC, AuthKind::Vnc));
    }
    if offered.contains(&VENCRYPT_SUBTYPE_X509NONE) {
        return Ok((VENCRYPT_SUBTYPE_X509NONE, AuthKind::None));
    }
    if offered.contains(&VENCRYPT_SUBTYPE_X509VNC) {
        return Err(BerryError::protocol(
            "VeNCrypt: server offers x509-vnc auth but no password is available",
        )
        .with_hint("pass --password (the run's vnc.pass is read by default; set one via `berry vnc passwd`)"));
    }
    Err(BerryError::protocol(format!(
        "VeNCrypt: no supported subtype (need {VENCRYPT_SUBTYPE_X509VNC}/{VENCRYPT_SUBTYPE_X509NONE}, server offered {offered:?})"
    )))
}

/// RFB 3.8 handshake: version, security negotiation (None or VNC auth),
/// shared ClientInit, ServerInit. Returns framebuffer size, pixel format,
/// and the auth type used. With `password` set we prefer security type 2;
/// servers that only offer None continue unauthenticated so callers can
/// report the actual mode. Password material never appears in errors.
///
/// Generic over the byte transport (wave 6): the TCP path passes `Stream`,
/// `vnc check --ws` passes a `ws::WsStream` — one state machine, no
/// copies; `rfb_tail`/`vnc_auth_exchange` were already generic.
fn handshake_auth<S: Read + Write>(
    stream: &mut S,
    password: Option<&str>,
) -> Result<(u32, u32, PixelFormat, AuthKind)> {
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
    let auth = if types.contains(&2) && password.is_some() {
        // VNC auth (RFC 6143 §7.2.2): 16-byte challenge, DES-ECB response
        // under the bit-reversed password key, then SecurityResult u32.
        stream.write_all(&[2])?;
        stream.flush()?;
        vnc_auth_exchange(stream, password.unwrap_or(""))?;
        AuthKind::Vnc
    } else if types.contains(&1) {
        stream.write_all(&[1])?;
        stream.flush()?;
        let mut result = [0u8; 4];
        read_exact(stream, &mut result)?;
        if u32::from_be_bytes(result) != 0 {
            return Err(BerryError::protocol("VNC: security handshake failed"));
        }
        AuthKind::None
    } else if types.contains(&19) {
        // VeNCrypt (subtype 19) needs a STARTTLS upgrade. The TLS-capable
        // path lives in `connect_tls`; reaching this arm means the run
        // state claimed a plaintext endpoint but the server disagrees.
        return Err(BerryError::protocol(
            "VNC: server requires TLS (VeNCrypt, security type 19)",
        )
        .with_hint(
            "vnc check follows TLS automatically when the run's vnc.listen says tls=true — restart the run with --vnc-tls (see `berry vnc status`)",
        ));
    } else {
        return Err(BerryError::protocol("VNC: server requires authentication")
            .with_hint("pass --password / set one via `berry vnc passwd` (the run's vnc.pass works too)"));
    };

    let (w, h, pf) = rfb_tail(stream)?;
    Ok((w, h, pf, auth))
}

/// ServerInit-depth handshake over any byte transport: the entry point
/// for `vnc check --ws` (wave 6). The same `handshake_auth` state machine
/// as the TCP paths — version, auth, ClientInit, ServerInit — minus the
/// encodings request and reader startup: noVNC reports "connected" at the
/// same depth, so the browser-path self-check needs no frame pull (and
/// avoids QEMU's 8192-byte chunked bulk traffic).
pub fn handshake_server_init<S: Read + Write>(
    stream: &mut S,
    password: Option<&str>,
) -> Result<(u32, u32, AuthKind)> {
    let (w, h, _pf, auth) = handshake_auth(stream, password)?;
    Ok((w, h, auth))
}

/// VNC auth (security type 2) message exchange after the type choice:
/// 16-byte DES challenge -> response -> SecurityResult. On failure the
/// RFB 3.8 reason string is read best-effort for a usable message (the
/// reason never carries password material).
fn vnc_auth_exchange<S: Read + Write>(stream: &mut S, password: &str) -> Result<()> {
    let mut challenge = [0u8; 16];
    read_exact(stream, &mut challenge)?;
    let resp = crate::display::des::vnc_challenge_response(password, &challenge);
    stream.write_all(&resp)?;
    stream.flush()?;
    let mut result = [0u8; 4];
    read_exact(stream, &mut result)?;
    if u32::from_be_bytes(result) != 0 {
        let mut reason = String::new();
        let mut len = [0u8; 4];
        if read_exact(stream, &mut len).is_ok() {
            let size = u32::from_be_bytes(len) as usize;
            if size <= 4096 {
                let mut buf = vec![0u8; size];
                if read_exact(stream, &mut buf).is_ok() {
                    // QEMU's reason (e.g. "Authentication failed \0")
                    // is NUL-padded with a trailing space; trim both so
                    // the CLI error message reads clean.
                    reason = String::from_utf8_lossy(&buf)
                        .trim_matches(|c: char| c.is_whitespace() || c == '\0')
                        .to_string();
                }
            }
        }
        let detail = if reason.is_empty() {
            String::new()
        } else {
            format!(": {reason}")
        };
        return Err(BerryError::protocol(format!("VNC: auth failed{detail}"))
            .with_hint("check the VNC password (`berry vnc passwd` or the run's vnc.pass)"));
    }
    Ok(())
}

/// ClientInit (shared=1) -> ServerInit: framebuffer size, pixel format,
/// name. Shared by the plaintext handshake and the TLS (VeNCrypt) tail.
fn rfb_tail<S: Read + Write>(stream: &mut S) -> Result<(u32, u32, PixelFormat)> {
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

/// Advertise encodings (Raw + DesktopSize) and request one full
/// non-incremental framebuffer update.
fn send_encodings_request<S: Read + Write>(stream: &mut S, w: u32, h: u32) -> Result<()> {
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
    Ok(())
}

pub struct VncSource {
    slot: Arc<Mutex<Option<Frame>>>,
    writer: Arc<Mutex<Stream>>,
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    handle: Option<JoinHandle<()>>,
    caps: Caps,
    auth: AuthKind,
    /// VeNCrypt subtype actually used ("x509-vnc" | "x509-none") when the
    /// session was established over TLS; None on plaintext.
    tls: Option<&'static str>,
    /// Duplicate handle to the raw socket, TLS sessions only: `Drop`
    /// unblocks a reader parked on the socket without waiting for the
    /// stream mutex (the rustls session cannot be cloned).
    shutdown_sock: Option<std::net::TcpStream>,
}

impl VncSource {
    pub fn connect(endpoint: &Endpoint) -> Result<Self> {
        Self::connect_auth(endpoint, None)
    }

    /// Connect with an optional VNC-auth password (RFB security type 2).
    /// Used by `berry vnc check` against the external TCP endpoint.
    pub fn connect_auth(endpoint: &Endpoint, password: Option<&str>) -> Result<Self> {
        let mut stream = connect_retry(endpoint, Duration::from_secs(10))?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;

        let (w, h, pf, auth) = handshake_auth(&mut stream, password)?;
        send_encodings_request(&mut stream, w, h)?;

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
            request_more: true,
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
            auth,
            tls: None,
            shutdown_sock: None,
        })
    }

    /// Connect over VeNCrypt (wave 5): plain RFB version exchange ->
    /// security type 19 -> VeNCrypt 0.2 negotiation -> subtype (X509Vnc
    /// 261 preferred, X509None 260 fallback) -> rustls handshake (strict
    /// CA + SAN unless the caller passed the insecure config) -> the
    /// subtype's inner auth -> the shared ClientInit/ServerInit tail.
    /// Byte sequence pinned against QEMU 11.0.3 (probe p2).
    pub fn connect_tls(
        endpoint: &Endpoint,
        password: Option<&str>,
        client_cfg: std::sync::Arc<rustls::ClientConfig>,
        name: rustls::pki_types::ServerName<'static>,
    ) -> Result<Self> {
        let mut tcp = match endpoint.connect()? {
            Stream::Tcp(s) => s,
            _ => return Err(BerryError::protocol("VNC: TLS endpoints are TCP only")),
        };
        tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(10)))?;
        // Raw-socket duplicate so Drop can unblock the reader without the
        // session mutex (the rustls session itself is not cloneable).
        let shutdown_sock = tcp.try_clone()?;

        // --- plain RFB version exchange ---
        let mut ver = [0u8; 12];
        read_exact(&mut tcp, &mut ver)?;
        if !ver.starts_with(b"RFB 003.") {
            return Err(BerryError::protocol(format!(
                "VNC: bad version {:?}",
                String::from_utf8_lossy(&ver)
            )));
        }
        tcp.write_all(b"RFB 003.008\n")?;
        tcp.flush()?;

        // --- security types: must offer VeNCrypt (19) ---
        let mut n = [0u8; 1];
        read_exact(&mut tcp, &mut n)?;
        if n[0] == 0 {
            return Err(BerryError::protocol("VNC: TLS endpoint refused the connection"));
        }
        let mut types = vec![0u8; n[0] as usize];
        read_exact(&mut tcp, &mut types)?;
        if !types.contains(&VENCRYPT_TYPE) {
            return Err(BerryError::protocol(format!(
                "VNC: server offers no VeNCrypt (type {VENCRYPT_TYPE}) in {:?}",
                types
            ))
            .with_hint("this endpoint looks plaintext — check `berry vnc status` for the run's tls state"));
        }
        tcp.write_all(&[VENCRYPT_TYPE])?;
        tcp.flush()?;

        // --- VeNCrypt version negotiation (0.2) ---
        let mut srv_ver = [0u8; 2];
        read_exact(&mut tcp, &mut srv_ver)?;
        let Some(common) = vencrypt_common_version(srv_ver) else {
            return Err(BerryError::protocol(format!(
                "VeNCrypt: server version {}.{} has no common ground with 0.2",
                srv_ver[0], srv_ver[1]
            )));
        };
        tcp.write_all(&common)?;
        tcp.flush()?;
        let mut ack = [0u8; 1];
        read_exact(&mut tcp, &mut ack)?;
        if ack[0] != 0 {
            return Err(BerryError::protocol(
                "VeNCrypt: server rejected the version negotiation",
            ));
        }

        // --- subtype list and preference ---
        let mut count = [0u8; 1];
        read_exact(&mut tcp, &mut count)?;
        let mut offered = vec![0u32; count[0] as usize];
        for sub in &mut offered {
            let mut b = [0u8; 4];
            read_exact(&mut tcp, &mut b)?;
            *sub = u32::from_be_bytes(b);
        }
        let (subtype, auth) = vencrypt_choose_subtype(&offered, password)?;
        tcp.write_all(&subtype.to_be_bytes())?;
        tcp.flush()?;
        let mut status = [0u8; 1];
        read_exact(&mut tcp, &mut status)?;
        if status[0] != 1 {
            return Err(BerryError::protocol(format!(
                "VeNCrypt: server rejected subtype {subtype} (status {})",
                status[0]
            )));
        }

        // --- TLS handshake (rustls) ---
        let conn = rustls::ClientConnection::new(std::sync::Arc::clone(&client_cfg), name)
            .map_err(|e| BerryError::protocol(format!("VNC: TLS client setup: {e}")))?;
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let tls_err = |e: std::io::Error| {
            BerryError::protocol(format!("VNC: TLS handshake failed: {e}")).with_hint(
                "--ca <dir> must point at the CA that issued the run's certificate (berry vnc certs), or pass --insecure to skip verification",
            )
        };
        while tls.conn.is_handshaking() {
            tls.conn.complete_io(&mut tls.sock).map_err(tls_err)?;
        }

        // --- inner auth per subtype, then the shared tail ---
        match subtype {
            VENCRYPT_SUBTYPE_X509VNC => {
                vnc_auth_exchange(&mut tls, password.unwrap_or(""))?;
            }
            VENCRYPT_SUBTYPE_X509NONE => {}
            _ => return Err(BerryError::protocol("VeNCrypt: unhandled subtype")),
        }
        let (w, h, pf) = rfb_tail(&mut tls)?;
        send_encodings_request(&mut tls, w, h)?;
        let subtype_str = if subtype == VENCRYPT_SUBTYPE_X509VNC {
            "x509-vnc"
        } else {
            "x509-none"
        };

        // --- reader over the single shared session ---
        // The read timeout stays armed so a parked reader eventually
        // releases the stream mutex; request_more=false keeps the reader
        // from writing (one full frame for `vnc check` is enough).
        let writer = Arc::new(Mutex::new(Stream::Tls(Box::new(tls))));
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
            request_more: false,
        };
        let handle = std::thread::spawn(move || {
            if let Err(e) = reader.run_shared() {
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
            auth,
            tls: Some(subtype_str),
            shutdown_sock: Some(shutdown_sock),
        })
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    /// Security type the handshake actually used (None or VNC auth).
    pub fn auth(&self) -> AuthKind {
        self.auth
    }

    /// VeNCrypt subtype of a TLS session ("x509-vnc" | "x509-none");
    /// None for plaintext connections.
    pub fn tls(&self) -> Option<&'static str> {
        self.tls
    }
}

impl Drop for VncSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // TLS: unblock the reader via the raw socket first so the stream
        // mutex is released promptly; then the generic shutdown path.
        if let Some(s) = &self.shutdown_sock {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
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
        Self::connect_auth(endpoint, None)
    }

    /// Input-only handshake with an optional VNC-auth password.
    pub fn connect_auth(endpoint: &Endpoint, password: Option<&str>) -> Result<Self> {
        let mut stream = connect_retry(endpoint, Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let (w, h, _, _) = handshake_auth(&mut stream, password)?;
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
                    if self.request_more {
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

    /// TLS variant: the single rustls session lives in the shared stream
    /// mutex; every read/write takes it for the duration of one exact
    /// read. The read timeout armed on the socket bounds each hold, and
    /// `Drop` unblocks a parked read via the raw-socket duplicate.
    fn run_shared(&mut self) -> Result<()> {
        let writer = self.writer.clone();
        let stop = self.stop.clone();
        while !stop.load(Ordering::Relaxed) {
            let mut guard = writer.lock().unwrap();
            let mut t = [0u8; 1];
            read_exact(&mut *guard, &mut t)?;
            match t[0] {
                0 => self.update(&mut guard)?,
                1 => {
                    let mut hdr = [0u8; 5];
                    read_exact(&mut *guard, &mut hdr)?;
                    let n = u16::from_be_bytes([hdr[3], hdr[4]]) as usize;
                    let mut data = vec![0u8; n * 6];
                    read_exact(&mut *guard, &mut data)?;
                }
                2 => {}
                3 => {
                    let mut hdr = [0u8; 7];
                    read_exact(&mut *guard, &mut hdr)?;
                    let n = u32::from_be_bytes([hdr[3], hdr[4], hdr[5], hdr[6]]) as usize;
                    if n > 1 << 20 {
                        return Err(BerryError::protocol("VNC: cut text too long"));
                    }
                    let mut data = vec![0u8; n];
                    read_exact(&mut *guard, &mut data)?;
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

fn read_exact<S: Read>(s: &mut S, buf: &mut [u8]) -> Result<()> {
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

/// Dial with retry until `timeout` (QEMU's listener races the run dir).
/// `pub(crate)` since wave 6: the ws path of `vnc check` reuses the same
/// 10s discipline for its websocket dial.
pub(crate) fn connect_retry(endpoint: &Endpoint, timeout: Duration) -> Result<Stream> {
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
pub(crate) mod tests {
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

    // ------------------------------------------------------------------
    // Loopback fake-RFB servers: the handshake_auth network path (vnc check)
    // pinned without QEMU. Shared scripting helpers first.
    // ------------------------------------------------------------------

    fn read_n(s: &mut std::net::TcpStream, n: usize) -> Vec<u8> {
        use std::io::Read;
        let mut b = vec![0u8; n];
        s.read_exact(&mut b).unwrap();
        b
    }

    pub(crate) fn server_write(s: &mut std::net::TcpStream, b: &[u8]) {
        use std::io::Write;
        s.write_all(b).unwrap();
        s.flush().unwrap();
    }

    /// Server side of the version exchange (send ServerVersion, read the
    /// client's 12-byte reply).
    fn rfb_exchange_version(s: &mut std::net::TcpStream) {
        server_write(s, b"RFB 003.008\n");
        let _client_ver = read_n(s, 12);
        assert_eq!(&_client_ver[..4], b"RFB ", "client must answer with a version");
    }

    /// Offer security types, return the client's 1-byte choice.
    fn rfb_offer_types(s: &mut std::net::TcpStream, types: &[u8]) -> Vec<u8> {
        server_write(s, &[types.len() as u8]);
        server_write(s, types);
        read_n(s, 1)
    }

    /// ClientInit -> ServerInit tail (4x2 BGRX 32bpp truecolor, name "t").
    fn rfb_server_init(s: &mut std::net::TcpStream) {
        let _shared = read_n(s, 1); // ClientInit (shared)
        let mut init = Vec::new();
        init.extend_from_slice(&4u16.to_be_bytes()); // width
        init.extend_from_slice(&2u16.to_be_bytes()); // height
        init.extend_from_slice(&[
            32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0,
        ]); // 32bpp BGRX LE
        init.extend_from_slice(&1u32.to_be_bytes()); // name length
        init.push(b't');
        server_write(s, &init);
    }

    fn connect_to(port: u16) -> Stream {
        let raw = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        raw.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        Stream::Tcp(raw)
    }

    /// Spawn a scripted RFB peer; returns the port to dial.
    // Shared with ram.rs's scanout tests (single fake-RFB source).
    pub(crate) fn spawn_rfb(script: impl FnOnce(std::net::TcpStream) + Send + 'static) -> u16 {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            script(s);
        });
        port
    }

    #[test]
    fn handshake_none_auth_with_and_without_password() {
        // Type 1 only: open endpoint. Without a password: plain ok.
        let port = spawn_rfb(|mut s| {
            rfb_exchange_version(&mut s);
            assert_eq!(rfb_offer_types(&mut s, &[1]), vec![1]);
            server_write(&mut s, &[0, 0, 0, 0]); // SecurityResult ok
            rfb_server_init(&mut s);
        });
        let mut c = connect_to(port);
        let (w, h, pf, auth) = handshake_auth(&mut c, None).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(pf.bpp, 32);
        assert_eq!(auth, AuthKind::None);
        // A password against an open endpoint does NOT force VNC auth — the
        // handshake continues unauthenticated so the caller can report the
        // actual mode (`auth none` truth).
        let port = spawn_rfb(|mut s| {
            rfb_exchange_version(&mut s);
            assert_eq!(rfb_offer_types(&mut s, &[1]), vec![1]);
            server_write(&mut s, &[0, 0, 0, 0]);
            rfb_server_init(&mut s);
        });
        let mut c = connect_to(port);
        let (_, _, _, auth) = handshake_auth(&mut c, Some("pw")).unwrap();
        assert_eq!(auth, AuthKind::None);
    }

    #[test]
    fn handshake_vnc_auth_challenge_response() {
        // Type 2 with the right password: the DES challenge response is
        // verified against the des module (the exact bytes QEMU expects).
        let port = spawn_rfb(|mut s| {
            rfb_exchange_version(&mut s);
            assert_eq!(rfb_offer_types(&mut s, &[1, 2]), vec![2], "client prefers type 2");
            let challenge = [
                7u8, 23, 99, 4, 200, 0, 255, 16, 42, 1, 2, 3, 4, 5, 250, 9,
            ];
            server_write(&mut s, &challenge);
            let resp = read_n(&mut s, 16);
            assert_eq!(
                resp,
                crate::display::des::vnc_challenge_response("rightpw", &challenge).to_vec(),
                "client response must be the DES-ECB challenge answer"
            );
            server_write(&mut s, &[0, 0, 0, 0]); // SecurityResult ok
            rfb_server_init(&mut s);
        });
        let mut c = connect_to(port);
        let (w, h, _, auth) = handshake_auth(&mut c, Some("rightpw")).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(auth, AuthKind::Vnc);
    }

    #[test]
    fn handshake_auth_failed_reason_is_cleaned() {
        // Wrong password: SecurityResult 1 + a QEMU-style reason padded with
        // a trailing space and NUL — the CLI error must read clean (wave-1
        // live fix, now regression-pinned) and carry the passwd hint.
        let port = spawn_rfb(|mut s| {
            rfb_exchange_version(&mut s);
            assert_eq!(rfb_offer_types(&mut s, &[2]), vec![2]);
            let challenge = [9u8; 16];
            server_write(&mut s, &challenge);
            let _resp = read_n(&mut s, 16);
            server_write(&mut s, &[0, 0, 0, 1]); // SecurityResult fail
            let reason = b"Authentication failed \0";
            let mut msg = (reason.len() as u32).to_be_bytes().to_vec();
            msg.extend_from_slice(reason);
            server_write(&mut s, &msg);
        });
        let mut c = connect_to(port);
        let e = handshake_auth(&mut c, Some("wrongpw")).unwrap_err();
        assert_eq!(e.code, 7, "auth failure is a protocol error (rc=7)");
        assert_eq!(e.message, "VNC: auth failed: Authentication failed");
        assert!(
            !e.message.ends_with(' ') && !e.message.contains('\0'),
            "trailing space/NUL must be trimmed: {:?}",
            e.message
        );
        assert!(e.hint.unwrap().contains("berry vnc passwd"));
    }

    // ------------------------------------------------------------------
    // VeNCrypt pure logic (U25/U26) + a full loopback TLS session
    // ------------------------------------------------------------------

    #[test]
    fn vencrypt_subtype_matrix() {
        use VENCRYPT_SUBTYPE_X509NONE as N;
        use VENCRYPT_SUBTYPE_X509VNC as V;
        // 261 with a password: the preferred encrypted-VNC-auth pair.
        assert_eq!(
            vencrypt_choose_subtype(&[V], Some("pw")).unwrap(),
            (V, AuthKind::Vnc)
        );
        // 261 without a password: hard error carrying the password hint.
        let e = vencrypt_choose_subtype(&[V], None).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.hint.as_deref().unwrap().contains("--password"));
        // 260 only: unauthenticated TLS fallback.
        assert_eq!(vencrypt_choose_subtype(&[N], None).unwrap(), (N, AuthKind::None));
        assert_eq!(
            vencrypt_choose_subtype(&[N], Some("pw")).unwrap(),
            (N, AuthKind::None),
            "no 261 offered -> 260 even with a password in hand"
        );
        // Both offered with a password: 261 wins.
        assert_eq!(
            vencrypt_choose_subtype(&[N, V], Some("pw")).unwrap(),
            (V, AuthKind::Vnc)
        );
        // Nothing supported: [] , [18] (plain VNC over TLS-less), [17].
        for offered in [&[][..], &[18u32][..], &[17u32][..], &[18u32, 17u32][..]] {
            let e = vencrypt_choose_subtype(offered, Some("pw")).unwrap_err();
            assert_eq!(e.code, 7, "offered={offered:?}");
            assert!(e.message.contains("no supported subtype"));
        }
    }

    #[test]
    fn vencrypt_version_negotiation_bytes() {
        // Wire constants pinned against the probe p2 byte dump.
        assert_eq!(VENCRYPT_TYPE, 19);
        assert_eq!(VENCRYPT_VERSION, [0x00, 0x02]);
        assert_eq!(VENCRYPT_ACK_OK, 0x00);
        assert_eq!(VENCRYPT_ACCEPTED, 0x01);
        assert_eq!(VENCRYPT_SUBTYPE_X509NONE, 260);
        assert_eq!(VENCRYPT_SUBTYPE_X509VNC, 261);
        // Common-version logic: server at 0.2 or newer -> reply 0.2;
        // server strictly older -> no common ground.
        assert_eq!(vencrypt_common_version([0x00, 0x02]), Some([0x00, 0x02]));
        assert_eq!(vencrypt_common_version([0x00, 0x03]), Some([0x00, 0x02]));
        assert_eq!(vencrypt_common_version([0x01, 0x00]), Some([0x00, 0x02]));
        assert_eq!(vencrypt_common_version([0x00, 0x01]), None);
        assert_eq!(vencrypt_common_version([0x00, 0x00]), None);
    }

    /// Scripted VeNCrypt+TLS server over loopback: the full connect_tls
    /// state machine against a real rustls server session (the same
    /// pairing QEMU's gnutls exercises, minus QEMU). `subtype` selects
    /// the offered list; 261 runs the DES challenge with `expect_pw`.
    fn spawn_vencrypt_tls(
        offered: &'static [u32],
        expect_pw: Option<&'static str>,
    ) -> u16 {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            s.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
            rfb_exchange_version(&mut s);
            let types = [VENCRYPT_TYPE];
            server_write(&mut s, &[types.len() as u8]);
            server_write(&mut s, &types);
            assert_eq!(read_n(&mut s, 1), vec![VENCRYPT_TYPE]);
            server_write(&mut s, &[0x00, 0x02]);
            assert_eq!(read_n(&mut s, 2), vec![0x00, 0x02]);
            server_write(&mut s, &[VENCRYPT_ACK_OK]);
            server_write(&mut s, &[offered.len() as u8]);
            for sub in offered {
                server_write(&mut s, &sub.to_be_bytes());
            }
            let chosen = read_n(&mut s, 4);
            let chosen = u32::from_be_bytes([chosen[0], chosen[1], chosen[2], chosen[3]]);
            if !offered.contains(&chosen) {
                server_write(&mut s, &[0x00]); // rejected
                return;
            }
            server_write(&mut s, &[VENCRYPT_ACCEPTED]);
            // TLS server handshake with the shared test certificate set.
            let dir = crate::tls::test_support::certs_dir().to_path_buf();
            let scfg = crate::tls::load_server_config(&dir).unwrap();
            let conn = rustls::ServerConnection::new(scfg).unwrap();
            let mut tls = rustls::StreamOwned::new(conn, s);
            while tls.conn.is_handshaking() {
                tls.conn.complete_io(&mut tls.sock).unwrap();
            }
            if chosen == VENCRYPT_SUBTYPE_X509VNC {
                let challenge = [7u8, 23, 99, 4, 200, 0, 255, 16, 42, 1, 2, 3, 4, 5, 250, 9];
                tls.write_all(&challenge).unwrap();
                tls.flush().unwrap();
                let mut resp = [0u8; 16];
                tls.read_exact(&mut resp).unwrap();
                let want = crate::display::des::vnc_challenge_response(
                    expect_pw.unwrap_or(""),
                    &challenge,
                );
                let ok = resp == want;
                tls.write_all(if ok { &[0, 0, 0, 0] } else { &[0, 0, 0, 1] })
                    .unwrap();
                tls.flush().unwrap();
                if !ok {
                    return;
                }
            }
            // ClientInit -> ServerInit (4x2 BGRX 32bpp, name "t").
            let _shared = {
                let mut b = [0u8; 1];
                tls.read_exact(&mut b).unwrap();
                b
            };
            let mut init = Vec::new();
            init.extend_from_slice(&4u16.to_be_bytes());
            init.extend_from_slice(&2u16.to_be_bytes());
            init.extend_from_slice(&[
                32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0,
            ]);
            init.extend_from_slice(&1u32.to_be_bytes());
            init.push(b't');
            tls.write_all(&init).unwrap();
            tls.flush().unwrap();
            // One framebuffer update (1x1 raw rect at 0,0) after the
            // client's SetEncodings (12B) + full request (10B) arrive.
            let mut hdr = [0u8; 22];
            tls.read_exact(&mut hdr).unwrap();
            let mut upd = vec![0u8, 0, 0, 1]; // type 0, pad, 1 rect
            upd.extend_from_slice(&[0, 0, 0, 0, 0, 1, 0, 1]); // x,y,w,h = 0,0,1,1
            upd.extend_from_slice(&0i32.to_be_bytes()); // Raw
            upd.extend_from_slice(&[0x11, 0x22, 0x33, 0x00]); // 1 BGRX pixel
            tls.write_all(&upd).unwrap();
            tls.flush().unwrap();
            // Hold the session open until the client drops the source.
            let mut sink = [0u8; 16];
            let _ = tls.read(&mut sink);
        });
        port
    }

    #[test]
    fn vencrypt_tls_end_to_end_x509vnc() {
        let port = spawn_vencrypt_tls(&[VENCRYPT_SUBTYPE_X509VNC], Some("rightpw"));
        let cfg = crate::tls::load_client_config(crate::tls::test_support::certs_dir()).unwrap();
        let name = crate::tls::server_name("127.0.0.1").unwrap();
        let ep = Endpoint::Tcp(format!("127.0.0.1:{port}"));
        let mut src =
            VncSource::connect_tls(&ep, Some("rightpw"), cfg, name).expect("TLS connect");
        assert_eq!(src.tls(), Some("x509-vnc"));
        assert_eq!(src.auth(), AuthKind::Vnc);
        let frame = src
            .next_frame(Duration::from_secs(5))
            .expect("first frame over TLS");
        assert_eq!((frame.width, frame.height), (4, 2));
    }

    #[test]
    fn vencrypt_tls_end_to_end_x509none() {
        let port = spawn_vencrypt_tls(&[VENCRYPT_SUBTYPE_X509NONE], None);
        let cfg = crate::tls::load_client_config(crate::tls::test_support::certs_dir()).unwrap();
        let name = crate::tls::server_name("127.0.0.1").unwrap();
        let ep = Endpoint::Tcp(format!("127.0.0.1:{port}"));
        let mut src = VncSource::connect_tls(&ep, None, cfg, name).expect("TLS connect");
        assert_eq!(src.tls(), Some("x509-none"));
        assert_eq!(src.auth(), AuthKind::None);
        let frame = src
            .next_frame(Duration::from_secs(5))
            .expect("first frame over TLS");
        assert_eq!((frame.width, frame.height), (4, 2));
    }

    #[test]
    fn vencrypt_tls_wrong_password_is_rc7() {
        let port = spawn_vencrypt_tls(&[VENCRYPT_SUBTYPE_X509VNC], Some("rightpw"));
        let cfg = crate::tls::load_client_config(crate::tls::test_support::certs_dir()).unwrap();
        let name = crate::tls::server_name("127.0.0.1").unwrap();
        let ep = Endpoint::Tcp(format!("127.0.0.1:{port}"));
        let e = match VncSource::connect_tls(&ep, Some("wrongpw"), cfg, name) {
            Err(e) => e,
            Ok(_) => panic!("wrong password must fail the inner DES auth"),
        };
        assert_eq!(e.code, 7, "inner DES failure over TLS is rc=7: {e}");
        assert!(e.message.contains("auth failed"), "{e}");
    }

    // ------------------------------------------------------------------
    // Pixel format / wire-format pure faces (W1-Q25 十刀).
    // ------------------------------------------------------------------

    #[test]
    fn pixel_format_parse_fields_and_rejects() {
        // Canonical 32bpp little-endian true-colour block (the ServerInit
        // layout rfb_tail hands in): every field lands where the wire says.
        let b = [
            32u8, 8, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0,
        ];
        let pf = PixelFormat::parse(&b).unwrap();
        assert_eq!(pf.bpp, 32);
        assert!(!pf.big_endian);
        assert!(pf.true_color);
        assert_eq!((pf.rmax, pf.gmax, pf.bmax), (255, 255, 255));
        assert_eq!((pf.rshift, pf.gshift, pf.bshift), (16, 8, 0));
        // Big-endian flag is b[2], true-colour flag is b[3]; maxes are BE u16.
        let b = [32u8, 8, 1, 1, 0, 31, 0, 63, 0, 31, 11, 5, 0, 0, 0, 0];
        let pf = PixelFormat::parse(&b).unwrap();
        assert!(pf.big_endian);
        assert_eq!((pf.rmax, pf.gmax, pf.bmax), (31, 63, 31));
        // Rejects: short buffer, colour-map, unsupported bpp (4/24).
        let short = BerryError::protocol("").code;
        let e = PixelFormat::parse(&b[..15]).unwrap_err();
        assert_eq!(e.code, short);
        assert!(e.message.contains("short pixel format"));
        let mut cm = b;
        cm[3] = 0;
        let e = PixelFormat::parse(&cm).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("colour-map"));
        for bad_bpp in [4u8, 24] {
            let mut bb = b;
            bb[0] = bad_bpp;
            let e = PixelFormat::parse(&bb).unwrap_err();
            assert_eq!(e.code, 7, "bpp={bad_bpp}");
            assert!(e.message.contains("unsupported bpp"));
        }
    }

    #[test]
    fn pixel_format_bytes_per_pixel() {
        let mk = |bpp: u8| PixelFormat {
            bpp,
            big_endian: false,
            true_color: true,
            rmax: 255,
            gmax: 255,
            bmax: 255,
            rshift: 16,
            gshift: 8,
            bshift: 0,
        };
        assert_eq!(mk(8).bytes(), 1);
        assert_eq!(mk(16).bytes(), 2);
        assert_eq!(mk(32).bytes(), 4);
    }

    #[test]
    fn pixel_32bpp_big_endian() {
        let pf = PixelFormat {
            bpp: 32,
            big_endian: true,
            true_color: true,
            rmax: 255,
            gmax: 255,
            bmax: 255,
            rshift: 16,
            gshift: 8,
            bshift: 0,
        };
        // BE wire bytes 00 11 22 33 -> value 0x00112233 -> r=0x11 g=0x22 b=0x33.
        assert_eq!(pf.pixel(&[0x00, 0x11, 0x22, 0x33]), 0x0011_2233);
    }

    #[test]
    fn pixel_8bpp_greyscale() {
        let pf = PixelFormat {
            bpp: 8,
            big_endian: false,
            true_color: true,
            rmax: 255,
            gmax: 255,
            bmax: 255,
            rshift: 0,
            gshift: 0,
            bshift: 0,
        };
        // All three channels read the same byte: greyscale passthrough.
        assert_eq!(pf.pixel(&[0x00]), 0x0000_0000);
        assert_eq!(pf.pixel(&[0x80]), 0x0080_8080);
        assert_eq!(pf.pixel(&[0xff]), 0x00ff_ffff);
    }

    #[test]
    fn channel_scale_math() {
        // Identity bands: max=255 and max=0 clamp straight through.
        assert_eq!(scale(200, 255), 200);
        assert_eq!(scale(255, 255), 255);
        assert_eq!(scale(300, 255), 300.min(255));
        assert_eq!(scale(7, 0), 7);
        // General max: round-half-up via (v*255 + max/2) / max.
        assert_eq!(scale(15, 15), (15 * 255 + 7) / 15); // == 255, full scale
        assert_eq!(scale(8, 15), (8 * 255 + 7) / 15); // == 136
        assert_eq!(scale(0, 63), 0);
        assert_eq!(scale(63, 63), 255);
        // 565 green max 63 midpoints land on the same rounding law.
        assert_eq!(scale(31, 63), (31 * 255 + 31) / 63);
        // Monotone across the band (no fold-back artefacts).
        let mut prev = 0u32;
        for v in 0..=31u32 {
            let s = scale(v, 31);
            assert!(s >= prev, "scale({v},31)={s} < prev {prev}");
            prev = s;
        }
    }

    #[test]
    fn framebuffer_request_wire_format() {
        // FramebufferUpdateRequest: type 3, incremental byte, then four
        // BE u16 fields (x, y, w, h) — 10 bytes total.
        let m = request(0, 0, 0, 1024, 768);
        assert_eq!(m.len(), 10);
        assert_eq!(&m[..2], &[3, 0]);
        assert_eq!(&m[2..], &[0, 0, 0, 0, 0x04, 0x00, 0x03, 0x00]);
        let m = request(1, 300, 40, 640, 480);
        assert_eq!(&m[..2], &[3, 1]);
        assert_eq!(&m[2..4], 300u16.to_be_bytes());
        assert_eq!(&m[4..6], 40u16.to_be_bytes());
        assert_eq!(&m[6..8], 640u16.to_be_bytes());
        assert_eq!(&m[8..10], 480u16.to_be_bytes());
        // u32 -> u16 field truncation is the wire contract (QEMU treats
        // the request as u16 domains); pin the cast so drift is visible.
        let m = request(0, 0x1_0002, 0, 0x1_0005, 0);
        assert_eq!(&m[2..4], 2u16.to_be_bytes());
        assert_eq!(&m[6..8], 5u16.to_be_bytes());
        assert_eq!(&m[8..10], 0u16.to_be_bytes());
    }

    #[test]
    fn read_exact_error_classification() {
        struct FailingRead(std::io::ErrorKind);
        impl std::io::Read for FailingRead {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(self.0))
            }
        }
        // WouldBlock/TimedOut classify as timeout (rc=4) …
        for kind in [
            std::io::ErrorKind::WouldBlock,
            std::io::ErrorKind::TimedOut,
        ] {
            let mut s = FailingRead(kind);
            let mut buf = [0u8; 4];
            let e = read_exact(&mut s, &mut buf).unwrap_err();
            assert_eq!(e.code, 4, "kind={kind:?}: {e}");
            assert!(e.message.contains("timeout"));
        }
        // … everything else (here: a hard IO error) as protocol (rc=7).
        let mut s = FailingRead(std::io::ErrorKind::ConnectionReset);
        let mut buf = [0u8; 4];
        let e = read_exact(&mut s, &mut buf).unwrap_err();
        assert_eq!(e.code, 7, "{e}");
        assert!(e.message.contains("VNC read"));
        // Early EOF on a real reader is protocol too, not timeout.
        let mut s = std::io::Cursor::new([0u8; 2]);
        let mut buf = [0u8; 4];
        let e = read_exact(&mut s, &mut buf).unwrap_err();
        assert_eq!(e.code, 7, "{e}");
    }

    #[test]
    fn auth_kind_display_names() {
        assert_eq!(AuthKind::None.as_str(), "none");
        assert_eq!(AuthKind::Vnc.as_str(), "vnc");
    }

    // ------------------------------------------------------------------
    // W1-Q26 二十刀：Reader 解码路径 + 线格式写面首获直测。私有 Reader
    // 状态机经 Unix socketpair 同步喂字节驱动（零 TCP 监听零 QEMU）——
    // update/raw 的矩形落点与跨行 stride、DesktopSize resize、cursor 伪
    // 编码消费、拒面族，run 的消息分派宽度，rfb_tail 的 ServerInit 域
    // 校验，send_encodings_request/VncInput 的出向字节。
    // ------------------------------------------------------------------

    #[cfg(unix)]
    fn unix_pair() -> (std::os::unix::net::UnixStream, Stream) {
        let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
        (a, Stream::Unix(b))
    }

    /// 32bpp BGRX LE reader over a fresh w×h frame（update/run 测试底座）。
    #[cfg(unix)]
    fn bgrx_reader(w: u32, h: u32) -> Reader {
        let (_, dummy) = unix_pair();
        Reader {
            slot: Arc::new(Mutex::new(None)),
            writer: Arc::new(Mutex::new(dummy)),
            stop: Arc::new(AtomicBool::new(false)),
            error: Arc::new(Mutex::new(None)),
            pf: PixelFormat {
                bpp: 32,
                big_endian: false,
                true_color: true,
                rmax: 255,
                gmax: 255,
                bmax: 255,
                rshift: 16,
                gshift: 8,
                bshift: 0,
            },
            frame: Frame::new(w, h),
            seq: 0,
            request_more: false,
        }
    }

    /// ServerInit 载荷（w、h、规范 BGRX pf、名字）——rfb_server_init 同形。
    #[cfg(unix)]
    fn init_body(w: u16, h: u16, name: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[32, 8, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
        b.extend_from_slice(&(name.len() as u32).to_be_bytes());
        b.extend_from_slice(name);
        b
    }

    /// FramebufferUpdate 头（type 0 已被 run 消费）+ n 个矩形头。
    #[cfg(unix)]
    fn upd(n: u16, rects: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8, (n >> 8) as u8, n as u8];
        b.extend_from_slice(rects);
        b
    }

    #[cfg(unix)]
    fn rect(x: u16, y: u16, w: u16, h: u16, enc: i32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&x.to_be_bytes());
        b.extend_from_slice(&y.to_be_bytes());
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&enc.to_be_bytes());
        b
    }

    #[cfg(unix)]
    #[test]
    fn reader_raw_rect_placement_stride_seq_publish() {
        // 双矩形一帧：2×1 @ (1,1) 钉行内偏移，1×2 @ (3,0) 钉跨行 stride；
        // 未触碰像素保持零，seq 滚 1，slot 发布携带矩形的帧副本。
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        // 线格式逐矩形交错：头→数据→头→数据（脚本首版头扎堆被首矩形
        // 当数据吃掉——真实行为就是交错流）。
        let mut msg = upd(2, &rect(1, 1, 2, 1, 0));
        msg.extend_from_slice(&[0x33, 0x22, 0x11, 0x00]); // BGRX -> 0x00112233
        msg.extend_from_slice(&[0x00, 0x00, 0xff, 0x00]); // -> 0x00ff0000
        msg.extend_from_slice(&rect(3, 0, 1, 2, 0));
        msg.extend_from_slice(&[0xff, 0x00, 0x00, 0x00]); // -> 0x000000ff ×2 行
        msg.extend_from_slice(&[0xff, 0x00, 0x00, 0x00]);
        peer.write_all(&msg).unwrap();
        r.update(&mut cli).unwrap();
        assert_eq!(r.frame.pixels.len(), 8);
        assert_eq!(r.frame.pixels[1 * 4 + 1], 0x0011_2233, "行内偏移 x=1");
        assert_eq!(r.frame.pixels[1 * 4 + 2], 0x00ff_0000);
        assert_eq!(r.frame.pixels[0 * 4 + 3], 0x0000_00ff, "竖矩形首行");
        assert_eq!(r.frame.pixels[1 * 4 + 3], 0x0000_00ff, "竖矩形次行（stride）");
        assert_eq!(r.frame.pixels[0], 0, "未触碰像素保持零");
        assert_eq!(r.seq, 1);
        let published = r.slot.lock().unwrap().take().expect("update 发布帧");
        assert_eq!((published.width, published.height, published.seq), (4, 2, 1));
        assert_eq!(published.pixels[5], 0x0011_2233, "副本携带矩形内容");
    }

    #[cfg(unix)]
    #[test]
    fn reader_desktop_size_resize_and_rejects() {
        // 合法 resize：几何滚新、缓冲重分配零填充。
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        r.frame.pixels.fill(0xff);
        peer.write_all(&upd(1, &rect(0, 0, 6, 2, -223))).unwrap();
        r.update(&mut cli).unwrap();
        assert_eq!((r.frame.width, r.frame.height), (6, 2));
        assert_eq!(r.frame.pixels.len(), 12);
        // Frame::resize 是 Vec::resize 扁平语义：前缀保内容、尾段零填充
        // （QEMU 的 DesktopSize 后随全量 raw 矩形重画——非 2D 重排）。
        assert!(r.frame.pixels[..8].iter().all(|&p| p == 0xff), "前缀保内容");
        assert!(r.frame.pixels[8..].iter().all(|&p| p == 0), "尾段零填充");
        // 退化/超 MAX_DIM 几何拒收。
        for (w, h) in [(0u16, 2u16), (2, 0), (5000, 2), (2, 5000)] {
            let (mut peer, mut cli) = unix_pair();
            let mut r = bgrx_reader(4, 2);
            peer.write_all(&upd(1, &rect(0, 0, w, h, -223))).unwrap();
            let e = r.update(&mut cli).unwrap_err();
            assert_eq!(e.code, 7, "w={w} h={h}: {e}");
            assert!(e.message.contains("bad DesktopSize"), "{e}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn reader_cursor_pseudo_encoding_consumed_and_bounded() {
        // -239 伪编码：像素+掩码载荷读后即弃，帧缓冲零触碰。
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        let mut msg = upd(1, &rect(0, 0, 2, 2, -239));
        msg.extend_from_slice(&[0u8; 18]); // 2*2*4 像素 + ((2+7)/8)*2 掩码
        peer.write_all(&msg).unwrap();
        r.update(&mut cli).unwrap();
        assert_eq!((r.frame.width, r.frame.height), (4, 2));
        assert!(r.frame.pixels.iter().all(|&p| p == 0));
        // 超界游标（4096×4096×4B + 掩码 > MAX_PIXELS*4）在读/分配前拒收。
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        peer.write_all(&upd(1, &rect(0, 0, 4096, 4096, -239))).unwrap();
        let e = r.update(&mut cli).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("cursor too large"), "{e}");
    }

    #[cfg(unix)]
    #[test]
    fn reader_update_reject_faces() {
        // 越界矩形：报文回显帧几何。
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        peer.write_all(&upd(1, &rect(3, 0, 2, 1, 0))).unwrap(); // x+w=5>4
        let e = r.update(&mut cli).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("outside"), "{e}");
        // 零面积矩形静默 noop（无载荷读取无报错）。
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        peer.write_all(&upd(1, &rect(0, 0, 0, 5, 0))).unwrap();
        r.update(&mut cli).unwrap();
        assert!(r.frame.pixels.iter().all(|&p| p == 0));
        // >1024 矩形先拒于逐矩形解析。
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        peer.write_all(&upd(1025, &[])).unwrap();
        let e = r.update(&mut cli).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("too many rects"), "{e}");
        // 未知编码带编号回显。
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        peer.write_all(&upd(1, &rect(0, 0, 1, 1, 5))).unwrap();
        let e = r.update(&mut cli).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("unsupported encoding 5"), "{e}");
    }

    #[cfg(unix)]
    #[test]
    fn reader_run_bell_cuttext_then_unknown() {
        // Bell（2）恰耗 1 字节、ServerCutText（3）恰耗 7 头 + 载荷——
        // 尾随未知类型 9 的报文即两宽度的逐字节证明（错位必先炸别处）。
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        let mut script = vec![2u8]; // Bell
        script.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0, 4]); // CutText len=4
        script.extend_from_slice(b"abcd");
        script.push(9);
        peer.write_all(&script).unwrap();
        let e = r.run(&mut cli).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("unknown message 9"), "{e}");
    }

    #[cfg(unix)]
    #[test]
    fn reader_run_cuttext_too_long() {
        let (mut peer, mut cli) = unix_pair();
        let mut r = bgrx_reader(4, 2);
        let mut script = vec![3u8, 0, 0, 0];
        script.extend_from_slice(&0x10_0001u32.to_be_bytes()); // > 1<<20
        peer.write_all(&script).unwrap();
        let e = r.run(&mut cli).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("cut text too long"), "{e}");
    }

    #[cfg(unix)]
    #[test]
    fn rfb_tail_valid_and_reject_faces() {
        // 合法：4x2 + 规范 BGRX pf + 单字节名。
        let (mut peer, mut cli) = unix_pair();
        peer.write_all(&init_body(4, 2, b"t")).unwrap();
        let (w, h, pf) = rfb_tail(&mut cli).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(pf.bpp, 32);
        assert_eq!((pf.rshift, pf.gshift, pf.bshift), (16, 8, 0));
        // 退化/超 MAX_DIM 几何拒收。
        for (gw, gh) in [(0u16, 2u16), (2, 0), (5000, 2), (2, 5000)] {
            let (mut peer, mut cli) = unix_pair();
            peer.write_all(&init_body(gw, gh, b"t")).unwrap();
            let e = rfb_tail(&mut cli).unwrap_err();
            assert_eq!(e.code, 7, "{gw}x{gh}: {e}");
            assert!(e.message.contains("bad size"), "{e}");
        }
        // 名长超 4096 在读取前拒收（覆盖 name-len 域，非追加）。
        let (mut peer, mut cli) = unix_pair();
        let mut b = init_body(4, 2, b"");
        b.truncate(20); // 头 20 字节（w/h/pf），name-len 域待写
        b.extend_from_slice(&5000u32.to_be_bytes());
        peer.write_all(&b).unwrap();
        let e = rfb_tail(&mut cli).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("bad name length"), "{e}");
    }

    #[cfg(unix)]
    #[test]
    fn send_encodings_request_exact_wire() {
        // SetEncodings（12B：type 2 + pad + count 2 + Raw 0 + DesktopSize
        // -223）紧跟全量非增量请求（10B）——出向字节逐字钉板。
        let (mut peer, mut cli) = unix_pair();
        send_encodings_request(&mut cli, 1024, 768).unwrap();
        let mut buf = [0u8; 22];
        peer.read_exact(&mut buf).unwrap();
        let mut want = vec![2u8, 0];
        want.extend_from_slice(&2u16.to_be_bytes());
        want.extend_from_slice(&0i32.to_be_bytes()); // Raw
        want.extend_from_slice(&(-223i32).to_be_bytes()); // DesktopSize
        want.extend_from_slice(&request(0, 0, 0, 1024, 768));
        assert_eq!(&buf[..], &want[..]);
    }

    #[cfg(unix)]
    #[test]
    fn handshake_bad_version_and_refusal() {
        // 非法版本横幅：不写任何后续即拒。
        let (mut peer, mut cli) = unix_pair();
        peer.write_all(b"XXX 003.008\n").unwrap();
        let e = handshake_auth(&mut cli, None).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("bad version"), "{e}");
        // n=0 + reason：拒绝文本逐字上浮（u32 长度 9 与 9 字节字面同长）。
        let (mut peer, mut cli) = unix_pair();
        peer.write_all(b"RFB 003.008\n").unwrap();
        peer.write_all(&[0]).unwrap();
        peer.write_all(&9u32.to_be_bytes()).unwrap();
        peer.write_all(b"go away!!").unwrap();
        let e = handshake_auth(&mut cli, None).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("refused: go away!!"), "{e}");
        assert!(e.message.ends_with("go away!!"), "无残缺无多余: {e}");
    }

    #[cfg(unix)]
    #[test]
    fn vnc_input_pointer_key_wire_and_size() {
        // 输入面三报文逐字节：Pointer 6B / Key 8B 双态 + size()。
        let (mut peer, cli) = unix_pair();
        let mut inp = VncInput { stream: cli, width: 4, height: 2 };
        assert_eq!(inp.size(), (4, 2));
        inp.pointer(0x0102, 0x0304, 7).unwrap();
        inp.key(0x41, true).unwrap();
        inp.key(0x1b, false).unwrap();
        let mut buf = [0u8; 22];
        peer.read_exact(&mut buf).unwrap();
        assert_eq!(&buf[..6], &[5, 7, 0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&buf[6..14], &[4, 1, 0, 0, 0, 0, 0, 0x41]);
        assert_eq!(&buf[14..], &[4, 0, 0, 0, 0, 0, 0, 0x1b]);
    }

    #[test]
    fn connect_retry_success_and_bounded_failure() {
        // 成功腿：活监听器首拨即中。
        let port = spawn_rfb(|_s| {});
        let ep = Endpoint::Tcp(format!("127.0.0.1:{port}"));
        assert!(connect_retry(&ep, Duration::from_secs(2)).is_ok());
        // 死端点：拒绝重试至预算耗尽，报文带端点显示形，耗时有界。
        // （Stream 无 Debug：unwrap_err 不可用，match 提取——QmpClient 先例。）
        let ep = Endpoint::Tcp("127.0.0.1:1".to_string());
        let start = Instant::now();
        let e = match connect_retry(&ep, Duration::from_millis(150)) {
            Err(e) => e,
            Ok(_) => panic!("dead port must not connect"),
        };
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "预算有界，实耗 {:?}",
            start.elapsed()
        );
        assert_eq!(e.code, 7);
        assert!(e.message.contains("VNC connect"), "{e}");
        assert!(e.message.contains("127.0.0.1:1"), "{e}");
    }
}
