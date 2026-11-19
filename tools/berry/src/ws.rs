//! Minimal RFC 6455 websocket client for `berry vnc check --ws` (wave 6):
//! the browser-facing endpoint QEMU exposes via `websocket=<port>`. That
//! endpoint is bare RFB over ws — binary frames carry the RFB byte stream
//! — so `vnc check` can self-check the exact path a browser takes with
//! the same RFB state machine as the TCP check.
//!
//! Zero new dependencies: SHA-1 and base64 are hand-written here (the
//! same discipline as `display/des.rs`); SHA-1 is used only to verify the
//! `Sec-WebSocket-Accept` integrity check of the local handshake, not as
//! a security primitive.
//!
//! Wire truth pinned against QEMU 11.0.3 (wave-6 probe f1):
//! - the `Date` response header may carry locale bytes (not RFC 1123
//!   ASCII) — header parsing is byte-tolerant and never forces UTF-8;
//! - server frames are always binary (op=2), FIN=1, unmasked; large
//!   payloads arrive as 8192-byte chunks that are byte-stream splits,
//!   NOT RFB message boundaries — the reader side is a stream;
//! - client frames MUST be masked: an unmasked frame is answered with
//!   close 1002 "client frames must be masked";
//! - no `Sec-WebSocket-Protocol` is offered (a no-sub protocol request
//!   still gets a 101 from QEMU, matching the browser form);
//! - QEMU's ws endpoint only accepts the root path, hence `GET /`.

use std::io::{Read, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::{BerryError, Result};

/// RFC 6455 §1.3 magic GUID appended to the client key when computing
/// `Sec-WebSocket-Accept`.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Opcodes (RFC 6455 §5.2): continuation/text/binary are data, the rest
/// are control frames.
const OP_CONTINUATION: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

/// Cap on one server frame's payload. QEMU chunks at 8192 bytes (probe
/// f1); 16 MiB is far beyond anything legitimate while still bounding
/// allocation against a hostile length field.
const MAX_SERVER_FRAME: u64 = 16 << 20;

/// Cap on the upgrade response head: real 101 heads are a few hundred
/// bytes; anything near this is a broken or hostile endpoint.
const MAX_UPGRADE_HEAD: usize = 8192;

/// Whole-exchange budget for the client upgrade handshake (F32): the
/// caller's per-read timeout bounds one stalled read, this budget bounds
/// the entire head — a slow-drip server (every byte just inside the read
/// timeout) could otherwise stretch the byte-wise loop toward
/// `MAX_UPGRADE_HEAD` × per-read-timeout. Same shape as the page
/// server's CONN_BUDGET (F18): the deadline is checked between reads,
/// so one blocked read still waits for its own timeout or byte.
const UPGRADE_BUDGET: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// SHA-1 + base64 (hand-written: std has neither; used only for the
// handshake integrity check, not as a security primitive)
// ---------------------------------------------------------------------------

/// SHA-1 digest (FIPS 180-1). Enough for `Sec-WebSocket-Accept`; the
/// call sites never rely on it for security.
pub(crate) fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    // Padding: 0x80, zeros to 56 mod 64, big-endian bit length.
    let bits = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let sum = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = sum;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding (RFC 4648) — the form `Sec-WebSocket-Key`
/// and `Sec-WebSocket-Accept` use.
pub(crate) fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let mut triple = [0u8; 3];
        triple[..chunk.len()].copy_from_slice(chunk);
        let n = (u32::from(triple[0]) << 16) | (u32::from(triple[1]) << 8) | u32::from(triple[2]);
        out.push(B64_ALPHABET[(n >> 18) as usize & 0x3F] as char);
        out.push(B64_ALPHABET[(n >> 12) as usize & 0x3F] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[(n >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[n as usize & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

/// RFC 6455 §1.3: base64(SHA1(key + GUID)) — the value the server must
/// echo in `Sec-WebSocket-Accept`.
pub fn expected_accept(key: &str) -> String {
    let mut data = String::with_capacity(key.len() + WS_GUID.len());
    data.push_str(key);
    data.push_str(WS_GUID);
    b64_encode(&sha1(data.as_bytes()))
}

/// Entropy source for handshake keys and frame mask keys: nanos + pid +
/// a static counter through SHA-1. Only uniqueness matters here — the
/// key proves the 101 answers *this* connection on a local endpoint, it
/// is not a cryptographic secret. This is stated honestly rather than
/// pulling in a rand crate (zero new dependencies).
fn quasi_random(n: usize) -> Vec<u8> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut seed = Vec::with_capacity(24);
    seed.extend_from_slice(&nanos.to_be_bytes());
    seed.extend_from_slice(&(std::process::id() as u64).to_be_bytes());
    seed.extend_from_slice(&CTR.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    let digest = sha1(&seed);
    digest[..n].to_vec()
}

/// A fresh 16-byte `Sec-WebSocket-Key` (base64 of 16 bytes, RFC 6455
/// §4.1 requires exactly that size).
fn ws_key() -> String {
    b64_encode(&quasi_random(16))
}

// ---------------------------------------------------------------------------
// Upgrade handshake
// ---------------------------------------------------------------------------

/// The raw HTTP/1.1 upgrade request for QEMU's root-only ws endpoint.
/// Pure so tests can pin the exact bytes; no `Sec-WebSocket-Protocol`
/// (probe f1: QEMU answers 101 without it, matching the browser form).
pub(crate) fn upgrade_request(host: &str, port: u16, key: &str) -> String {
    format!(
        "GET / HTTP/1.1\r\n\
         Host: {host}:{port}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n"
    )
}

/// A parsed 101 head: the status line plus lower-cased header names with
/// byte-verbatim values (the `Date` header may be locale bytes, so values
/// are never forced through UTF-8).
struct UpgradeHead {
    status_line: Vec<u8>,
    headers: Vec<(Vec<u8>, Vec<u8>)>,
}

impl UpgradeHead {
    fn parse(raw: &[u8]) -> Option<Self> {
        let mut lines = raw.split(|&b| b == b'\n');
        let status_line = lines.next()?.to_vec();
        let status_line = status_line
            .strip_suffix(b"\r")
            .unwrap_or(&status_line)
            .to_vec();
        let mut headers = Vec::new();
        for line in lines {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            // `split_once` on slices is still unstable — find the colon.
            let Some(colon) = line.iter().position(|&b| b == b':') else {
                continue;
            };
            let (name, value) = line.split_at(colon);
            let value = &value[1..];
            let name = name
                .iter()
                .map(|b| b.to_ascii_lowercase())
                .collect::<Vec<_>>();
            let value = value
                .iter()
                .copied()
                .skip_while(|b| b == &b' ' || b == &b'\t')
                .collect::<Vec<_>>();
            headers.push((name, value));
        }
        Some(Self { status_line, headers })
    }

    fn value(&self, name: &[u8]) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_slice())
    }
}

/// Validate the 101 head against RFC 6455 §4.2.2 (status, Upgrade,
/// Connection, Sec-WebSocket-Accept). Byte-tolerant: only the compared
/// header values are inspected; everything else stays raw bytes.
fn validate_upgrade(raw: &[u8], key: &str) -> Result<()> {
    let head =
        UpgradeHead::parse(raw).ok_or_else(|| BerryError::protocol("ws: malformed upgrade response"))?;
    let bad = || {
        BerryError::protocol(format!(
            "ws: upgrade refused: {}",
            String::from_utf8_lossy(&head.status_line).trim()
        ))
    };
    let status = &head.status_line;
    if !status.starts_with(b"HTTP/1.") {
        return Err(bad());
    }
    let code = status
        .split(|&b| b == b' ')
        .nth(1)
        .ok_or_else(bad)?;
    if code != b"101" {
        return Err(bad());
    }
    let contains_ci = |name: &[u8], needle: &[u8]| {
        head.value(name).map(|v| {
            v.iter()
                .map(|b| b.to_ascii_lowercase())
                .collect::<Vec<u8>>()
                .windows(needle.len())
                .any(|w| w == needle)
        }) == Some(true)
    };
    if !contains_ci(b"upgrade", b"websocket") {
        return Err(BerryError::protocol(
            "ws: upgrade response missing `Upgrade: websocket`",
        ));
    }
    if !contains_ci(b"connection", b"upgrade") {
        return Err(BerryError::protocol(
            "ws: upgrade response missing `Connection: Upgrade`",
        ));
    }
    let want = expected_accept(key);
    match head.value(b"sec-websocket-accept") {
        Some(v) if v == want.as_bytes() => Ok(()),
        Some(_) => Err(BerryError::protocol(
            "ws: Sec-WebSocket-Accept mismatch (not a live 101 for this key)",
        )),
        None => Err(BerryError::protocol(
            "ws: upgrade response has no Sec-WebSocket-Accept",
        )),
    }
}

// ---------------------------------------------------------------------------
// Frame codec
// ---------------------------------------------------------------------------

/// XOR the payload with the 4-byte mask key (symmetric).
fn mask_in_place(key: &[u8; 4], data: &mut [u8]) {
    for (i, b) in data.iter_mut().enumerate() {
        *b ^= key[i % 4];
    }
}

/// One client frame: FIN=1, mask bit set, 7/16/64-bit length per size.
/// The mask key is injectable so tests can pin exact bytes; RFC 6455
/// §5.1 mandates client masking and QEMU hard-enforces it (close 1002
/// "client frames must be masked", probe f1).
pub(crate) fn encode_client_frame(opcode: u8, payload: &[u8], mask_key: &[u8; 4]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 14);
    frame.push(0x80 | opcode);
    let len = payload.len();
    if len < 126 {
        frame.push(0x80 | len as u8);
    } else if len <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(mask_key);
    let start = frame.len();
    frame.extend_from_slice(payload);
    mask_in_place(mask_key, &mut frame[start..]);
    frame
}

/// A decoded server frame. `fin`/`opcode` are exposed for tests; the
/// stream reader treats all data opcodes as one byte stream (QEMU chunks
/// large writes into 8192-byte frames that are stream splits, not RFB
/// message boundaries — probe f1).
#[derive(Debug)]
pub(crate) struct ServerFrame {
    /// FIN flag. Read by tests to pin frame forms; the production read
    /// path deliberately ignores it — QEMU always sets FIN=1 on data
    /// frames (probe f1) and tolerance for FIN=0 continuation is free.
    #[allow(dead_code)]
    pub fin: bool,
    pub opcode: u8,
    pub payload: Vec<u8>,
}

fn is_control(opcode: u8) -> bool {
    opcode >= 0x8
}

/// Read one server frame: FIN/opcode/length (7/16/64-bit forms per RFC
/// 6455 §5.2), mask bit rejected (§5.1: server frames are unmasked),
/// control frames must be FIN=1 and <=125 bytes (§5.5).
pub(crate) fn read_server_frame<R: Read>(src: &mut R) -> std::io::Result<ServerFrame> {
    let ws_err = |m: String| std::io::Error::new(std::io::ErrorKind::InvalidData, m);
    let mut hdr = [0u8; 2];
    src.read_exact(&mut hdr)?;
    let fin = hdr[0] & 0x80 != 0;
    if hdr[0] & 0x70 != 0 {
        return Err(ws_err("ws: reserved bits set in a server frame".into()));
    }
    let opcode = hdr[0] & 0x0F;
    if hdr[1] & 0x80 != 0 {
        return Err(ws_err("ws: server frame has the mask bit set".into()));
    }
    let len = match hdr[1] & 0x7F {
        n @ 0..=125 => n as u64,
        126 => {
            let mut ext = [0u8; 2];
            src.read_exact(&mut ext)?;
            u16::from_be_bytes(ext) as u64
        }
        _ => {
            let mut ext = [0u8; 8];
            src.read_exact(&mut ext)?;
            u64::from_be_bytes(ext)
        }
    };
    if is_control(opcode) {
        if !fin {
            return Err(ws_err("ws: fragmented control frame".into()));
        }
        if len > 125 {
            return Err(ws_err("ws: control frame longer than 125 bytes".into()));
        }
    }
    if len > MAX_SERVER_FRAME {
        return Err(ws_err(format!("ws: server frame too large ({len} bytes)")));
    }
    let mut payload = vec![0u8; len as usize];
    src.read_exact(&mut payload)?;
    Ok(ServerFrame { fin, opcode, payload })
}

// ---------------------------------------------------------------------------
// WsStream: framed Read/Write over any byte transport
// ---------------------------------------------------------------------------

/// Inner source for frame decoding: bytes read past the 101 head (the
/// first server frame may share the TCP segment) drain first, then the
/// transport.
#[derive(Debug)]
struct Prefixed<S> {
    pre: Vec<u8>,
    off: usize,
    inner: S,
}

impl<S: Read> Read for Prefixed<S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.off < self.pre.len() {
            let n = buf.len().min(self.pre.len() - self.off);
            buf[..n].copy_from_slice(&self.pre[self.off..self.off + n]);
            self.off += n;
            if self.off == self.pre.len() {
                self.off = 0;
                self.pre.clear();
            }
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

/// A ws-framed byte stream: `Read` yields the RFB byte stream carried by
/// binary frames (auto-answering pings, best-effort close reply on
/// close, EOF afterwards); `Write` sends each call as one masked binary
/// frame — RFB client messages are small (12B version, 1B type, 16B DES
/// response, 1B ClientInit), so per-call framing matches RFB message
/// boundaries naturally.
///
/// `S` is the byte transport: a plaintext `Stream`/`TcpStream`, or a
/// rustls `StreamOwned` for wss — the frame layer is identical (probe
/// f1: TLS wraps the same byte sequence).
#[derive(Debug)]
pub struct WsStream<S> {
    src: Prefixed<S>,
    /// Decoded payload bytes not yet handed out by `Read`.
    pending: Vec<u8>,
    pending_off: usize,
    /// A close frame was seen; further reads are EOF.
    closed: bool,
}

impl<S: Read + Write> WsStream<S> {
    /// Perform the RFC 6455 opening handshake over the connected
    /// transport (per-read timeouts armed by the caller, plus the
    /// whole-exchange budget above) and return the framed stream. Bytes
    /// the server sent past the 101 head are preserved — they are the
    /// start of the first server frame, not RFB payload.
    pub fn handshake(stream: S, host: &str, port: u16) -> Result<Self> {
        Self::handshake_with_budget(stream, host, port, UPGRADE_BUDGET)
    }

    /// `handshake` with an explicit budget (test seam for the deadline;
    /// production callers always get `UPGRADE_BUDGET`).
    fn handshake_with_budget(
        mut stream: S,
        host: &str,
        port: u16,
        budget: Duration,
    ) -> Result<Self> {
        let key = ws_key();
        let req = upgrade_request(host, port, &key);
        stream
            .write_all(req.as_bytes())
            .and_then(|_| stream.flush())
            .map_err(|e| BerryError::protocol(format!("ws: send upgrade request: {e}")))?;
        let deadline = Instant::now() + budget;
        let mut raw: Vec<u8> = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            if Instant::now() >= deadline {
                return Err(BerryError::protocol(format!(
                    "ws: upgrade response head not complete within {} s",
                    budget.as_secs()
                )));
            }
            match stream.read(&mut byte) {
                Ok(0) => {
                    return Err(BerryError::protocol(
                        "ws: EOF during the upgrade handshake",
                    ))
                }
                Ok(_) => {
                    raw.push(byte[0]);
                    if raw.ends_with(b"\r\n\r\n") {
                        break;
                    }
                    if raw.len() > MAX_UPGRADE_HEAD {
                        return Err(BerryError::protocol(
                            "ws: upgrade response head exceeds 8192 bytes",
                        ));
                    }
                }
                Err(e) => {
                    return Err(BerryError::protocol(format!(
                        "ws: read upgrade response: {e}"
                    )))
                }
            }
        }
        // The 4-byte terminator ends the head; with byte-wise reads the
        // loop stops exactly there, so nothing past it was consumed —
        // but compute the leftover from first principles anyway (any
        // bytes past the terminator are the first server frame's start).
        let head_end = raw.len() - 4;
        validate_upgrade(&raw[..head_end], &key)?;
        raw.drain(..head_end + 4);
        Ok(Self {
            src: Prefixed {
                pre: raw,
                off: 0,
                inner: stream,
            },
            pending: Vec::new(),
            pending_off: 0,
            closed: false,
        })
    }

    fn next_mask_key() -> [u8; 4] {
        let bytes = quasi_random(4);
        [bytes[0], bytes[1], bytes[2], bytes[3]]
    }

    /// Decode the next server frame, handling control frames inline:
    /// ping -> masked pong with the same payload; close -> masked close
    /// reply (best effort) and a flag; pong -> ignored. Data frames
    /// (binary/text/continuation — QEMU only emits binary, tolerance is
    /// free) return their payload.
    fn next_data(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        loop {
            let frame = read_server_frame(&mut self.src)?;
            match frame.opcode {
                OP_PING => {
                    let reply =
                        encode_client_frame(OP_PONG, &frame.payload, &Self::next_mask_key());
                    self.src.inner.write_all(&reply)?;
                    self.src.inner.flush()?;
                }
                OP_CLOSE => {
                    self.closed = true;
                    let body = &frame.payload[..frame.payload.len().min(125)];
                    let reply = encode_client_frame(OP_CLOSE, body, &Self::next_mask_key());
                    let _ = self.src.inner.write_all(&reply);
                    let _ = self.src.inner.flush();
                    return Ok(None);
                }
                OP_PONG => {}
                OP_TEXT | OP_BINARY | OP_CONTINUATION => return Ok(Some(frame.payload)),
                other => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("ws: unknown server opcode {other}"),
                    ))
                }
            }
        }
    }
}

impl<S: Read + Write> Read for WsStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.pending_off < self.pending.len() {
                let n = buf.len().min(self.pending.len() - self.pending_off);
                buf[..n].copy_from_slice(&self.pending[self.pending_off..self.pending_off + n]);
                self.pending_off += n;
                if self.pending_off == self.pending.len() {
                    self.pending_off = 0;
                    self.pending.clear();
                }
                return Ok(n);
            }
            if self.closed {
                return Ok(0);
            }
            match self.next_data()? {
                Some(payload) if payload.is_empty() => continue,
                Some(payload) => {
                    self.pending = payload;
                    self.pending_off = 0;
                }
                None => return Ok(0),
            }
        }
    }
}

impl<S: Read + Write> Write for WsStream<S> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let frame = encode_client_frame(OP_BINARY, buf, &Self::next_mask_key());
        self.src.inner.write_all(&frame)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.src.inner.flush()
    }
}

// ---------------------------------------------------------------------------
// Tests (U34-U38, wave-6 plan)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    // U34: SHA-1 / base64 / expected_accept against known vectors.

    #[test]
    fn sha1_known_vectors() {
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(b"")),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        // FIPS 180-1 suite: 448-bit message spanning 8 blocks total.
        assert_eq!(
            hex(&sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        // Block-boundary edges: 55 bytes pad inside one block, 56 force
        // a second (real digests, hashlib cross-checked).
        assert_eq!(
            hex(&sha1(&[b'x'; 55])),
            "cef734ba81a024479e09eb5a75b6ddae62e6abf1"
        );
        assert_eq!(
            hex(&sha1(&[b'x'; 56])),
            "901305367c259952f4e7af8323f480d59f81335b"
        );
    }

    #[test]
    fn b64_known_vectors() {
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(b64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(b64_encode(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn accept_matches_rfc6455_sample() {
        // RFC 6455 §1.3 worked example, byte-for-byte.
        assert_eq!(
            expected_accept("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    // U35: client frame encoding pinned with an injected mask key.

    #[test]
    fn client_frame_small_is_7bit_length() {
        let key = [0x11, 0x22, 0x33, 0x44];
        let frame = encode_client_frame(OP_BINARY, b"RFB 003.008\n", &key);
        // FIN=1 binary, mask bit, len=12, key, then XOR-masked payload.
        assert_eq!(frame[0], 0x82);
        assert_eq!(frame[1], 0x80 | 12);
        assert_eq!(&frame[2..6], &key);
        let mut payload = frame[6..].to_vec();
        mask_in_place(&key, &mut payload);
        assert_eq!(payload, b"RFB 003.008\n");
    }

    #[test]
    fn client_frame_length_boundaries() {
        let key = [0, 0, 0, 0];
        // 125 stays 7-bit; 126 flips to the 16-bit form.
        let f = encode_client_frame(OP_BINARY, &vec![b'a'; 125], &key);
        assert_eq!(&f[..2], &[0x82, 0x80 | 125]);
        let f = encode_client_frame(OP_BINARY, &vec![b'a'; 126], &key);
        assert_eq!(&f[..2], &[0x82, 0x80 | 126]);
        assert_eq!(&f[2..4], &126u16.to_be_bytes());
        // 65535 stays 16-bit; 65536 flips to the 64-bit form.
        let f = encode_client_frame(OP_BINARY, &vec![b'a'; 65535], &key);
        assert_eq!(&f[..2], &[0x82, 0x80 | 126]);
        assert_eq!(&f[2..4], &65535u16.to_be_bytes());
        let f = encode_client_frame(OP_BINARY, &vec![b'a'; 65536], &key);
        assert_eq!(&f[..2], &[0x82, 0x80 | 127]);
        assert_eq!(&f[2..10], &65536u64.to_be_bytes());
        // The mask is a plain XOR: re-masking restores the payload.
        let payload = vec![7u8; 300];
        let key = [0xde, 0xad, 0xbe, 0xef];
        let f = encode_client_frame(OP_BINARY, &payload, &key);
        let mut out = f[f.len() - 300..].to_vec();
        mask_in_place(&key, &mut out);
        assert_eq!(out, payload);
    }

    // U36: server frame decoding.

    #[test]
    fn server_frame_lengths_and_opcodes() {
        // 7-bit.
        let mut bytes = vec![0x82, 3, b'a', b'b', b'c'];
        let f = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap();
        assert!(f.fin);
        assert_eq!(f.opcode, OP_BINARY);
        assert_eq!(f.payload, b"abc");
        // 16-bit (126 marker).
        bytes = vec![0x82, 126];
        bytes.extend_from_slice(&300u16.to_be_bytes());
        bytes.extend_from_slice(&vec![b'z'; 300]);
        let f = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(f.payload.len(), 300);
        // 64-bit (127 marker).
        let mut bytes = vec![0x82, 127];
        bytes.extend_from_slice(&70000u64.to_be_bytes());
        bytes.extend_from_slice(&vec![b'q'; 70000]);
        let f = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(f.payload.len(), 70000);
        // Control frames: close/ping/pong decode with their opcodes.
        for (op, want) in [(OP_CLOSE, OP_CLOSE), (OP_PING, OP_PING), (OP_PONG, OP_PONG)] {
            let bytes = vec![0x80 | op, 2, 0x03, 0xe8];
            let f = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap();
            assert_eq!(f.opcode, want);
            assert_eq!(f.payload, vec![0x03, 0xe8]);
        }
        // FIN=0 continuation (never seen from QEMU, tolerated): decodes
        // as a data frame with fin=false.
        let bytes = vec![0x00, 2, 1, 2];
        let f = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap();
        assert!(!f.fin);
        assert_eq!(f.opcode, OP_CONTINUATION);
        assert_eq!(f.payload, vec![1, 2]);
    }

    #[test]
    fn server_frame_rejects_mask_and_bad_control() {
        // Mask bit set on a server frame: RFC 6455 §5.1 violation.
        let bytes = [0x82, 0x80 | 3, 1, 2, 3, 4, 9, 9, 9];
        let e = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap_err();
        assert!(e.to_string().contains("mask bit"), "{e}");
        // Control frame longer than 125 bytes.
        let mut bytes = vec![0x89, 126];
        bytes.extend_from_slice(&200u16.to_be_bytes());
        bytes.extend_from_slice(&vec![0u8; 200]);
        let e = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap_err();
        assert!(e.to_string().contains("125"), "{e}");
        // Fragmented control frame.
        let bytes = [0x09, 1, 0];
        let e = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap_err();
        assert!(e.to_string().contains("fragmented control"), "{e}");
    }

    // ------------------------------------------------------------------
    // U37/U38: loopback peers
    // ------------------------------------------------------------------

    /// Read one masked client frame and return its (unmasked) payload —
    /// the mirror of what QEMU's ws parser does (mask is mandatory).
    fn read_client_frame<S: Read>(s: &mut S) -> Vec<u8> {
        let mut hdr = [0u8; 2];
        s.read_exact(&mut hdr).unwrap();
        assert_eq!(hdr[1] & 0x80, 0x80, "client frames must be masked");
        let len = match hdr[1] & 0x7F {
            n @ 0..=125 => n as usize,
            126 => {
                let mut ext = [0u8; 2];
                s.read_exact(&mut ext).unwrap();
                u16::from_be_bytes(ext) as usize
            }
            _ => {
                let mut ext = [0u8; 8];
                s.read_exact(&mut ext).unwrap();
                u64::from_be_bytes(ext) as usize
            }
        };
        let mut key = [0u8; 4];
        s.read_exact(&mut key).unwrap();
        let mut payload = vec![0u8; len];
        s.read_exact(&mut payload).unwrap();
        mask_in_place(&key, &mut payload);
        payload
    }

    /// Wrap a payload in one unmasked binary server frame (QEMU's form).
    fn server_frame(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(payload.len() + 10);
        out.push(0x82);
        if payload.len() < 126 {
            out.push(payload.len() as u8);
        } else if payload.len() <= u16::MAX as usize {
            out.push(126);
            out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            out.push(127);
            out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
        out.extend_from_slice(payload);
        out
    }

    /// Read the client's upgrade request, return the 101 response bytes
    /// for it (correct Accept), optionally with extra raw headers.
    fn accept_101<S: Read + Write>(s: &mut S, extra: &[u8]) {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        let req = String::from_utf8(head.clone()).unwrap();
        let key = req
            .lines()
            .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
            .expect("upgrade request carries a key");
        assert!(req.starts_with("GET / HTTP/1.1\r\n"), "{req:?}");
        assert!(req.contains("Sec-WebSocket-Version: 13"));
        let resp = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Server: QEMU VNC\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n",
            expected_accept(key)
        );
        // `extra` carries additional header lines; the blank line ends
        // the head after them.
        s.write_all(resp.as_bytes()).unwrap();
        s.write_all(extra).unwrap();
        s.write_all(b"\r\n").unwrap();
        s.flush().unwrap();
    }

    fn spawn_peer<F>(script: F) -> u16
    where
        F: FnOnce(std::net::TcpStream) + Send + 'static,
    {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            script(s);
        });
        port
    }

    fn dial(port: u16) -> TcpStream {
        let s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        s.set_write_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        s
    }

    // U37: the upgrade handshake.

    #[test]
    fn handshake_accepts_101_with_locale_date_bytes() {
        // A Date header carrying non-UTF-8 locale bytes (zh systems,
        // probe f1) must not break the handshake.
        let port = spawn_peer(|mut s| {
            let date = b"Date: \xd0\xec\xd2\xbb\xd4\xc2 15 12:00:00 2026\r\n".to_vec();
            accept_101(&mut s, &date);
            // Then the first RFB banner rides a binary frame in the
            // SAME flush as the head tail is fine too; here separately.
            s.write_all(&server_frame(b"RFB 003.008\n")).unwrap();
            s.flush().unwrap();
        });
        let mut ws = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap();
        let mut banner = [0u8; 12];
        ws.read_exact(&mut banner).unwrap();
        assert_eq!(&banner, b"RFB 003.008\n");
    }

    #[test]
    fn handshake_rejects_bad_accept_and_non_101() {
        // Wrong Sec-WebSocket-Accept.
        let port = spawn_peer(|mut s| {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            s.write_all(
                b"HTTP/1.1 101 Switching Protocols\r\n\
                  Upgrade: websocket\r\n\
                  Connection: Upgrade\r\n\
                  Sec-WebSocket-Accept:AAAAAAAAAAAAAAAAAAAAAA==\r\n\r\n",
            )
            .unwrap();
            s.flush().unwrap();
        });
        let e = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("Accept mismatch"), "{e}");
        // Non-101 status: the status line rides the error.
        let port = spawn_peer(|mut s| {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
            s.flush().unwrap();
        });
        let e = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("404"), "{e}");
    }

    // U38: end-to-end RFB over ws (and wss) through the shared
    // handshake_auth state machine — the same despatch the TCP tests pin.

    /// Scripted ws RFB peer: 101, then banner/types/DES/ServerInit with
    /// all server bytes framed and all client bytes mask-checked.
    fn ws_rfb_script<S: Read + Write>(mut s: S, expect_pw: &str) {
        accept_101(&mut s, &[]);
        s.write_all(&server_frame(b"RFB 003.008\n")).unwrap();
        s.flush().unwrap();
        assert_eq!(&read_client_frame(&mut s)[..4], b"RFB ");
        // Offer [1, 2]; the client with a password picks 2.
        s.write_all(&server_frame(&[2, 1, 2])).unwrap();
        s.flush().unwrap();
        assert_eq!(read_client_frame(&mut s), vec![2]);
        let challenge = [7u8, 23, 99, 4, 200, 0, 255, 16, 42, 1, 2, 3, 4, 5, 250, 9];
        s.write_all(&server_frame(&challenge)).unwrap();
        s.flush().unwrap();
        let resp = read_client_frame(&mut s);
        assert_eq!(
            resp,
            crate::display::des::vnc_challenge_response(expect_pw, &challenge).to_vec(),
            "the DES response over ws must equal the TCP-path answer"
        );
        s.write_all(&server_frame(&[0, 0, 0, 0])).unwrap();
        s.flush().unwrap();
        assert_eq!(read_client_frame(&mut s), vec![1], "ClientInit shared=1");
        let mut init = Vec::new();
        init.extend_from_slice(&4u16.to_be_bytes());
        init.extend_from_slice(&2u16.to_be_bytes());
        init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
        init.extend_from_slice(&1u32.to_be_bytes());
        init.push(b't');
        s.write_all(&server_frame(&init)).unwrap();
        s.flush().unwrap();
    }

    #[test]
    fn rfb_handshake_over_ws_end_to_end() {
        let port = spawn_peer(|s| ws_rfb_script(s, "rightpw"));
        let mut ws = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap();
        let (w, h, auth) =
            crate::display::vnc::handshake_server_init(&mut ws, Some("rightpw")).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(auth.as_str(), "vnc");
    }

    #[test]
    fn rfb_handshake_over_wss_end_to_end() {
        // wss = TLS wraps the whole ws layer (probe f1): the same script
        // runs inside a real rustls server session.
        let port = spawn_peer(|raw| {
            let dir = crate::tls::test_support::certs_dir().to_path_buf();
            let scfg = crate::tls::load_server_config(&dir).unwrap();
            let conn = rustls::ServerConnection::new(scfg).unwrap();
            let mut tls = rustls::StreamOwned::new(conn, raw);
            while tls.conn.is_handshaking() {
                tls.conn.complete_io(&mut tls.sock).unwrap();
            }
            ws_rfb_script(&mut tls, "rightpw");
        });
        let cfg = crate::tls::load_client_config(crate::tls::test_support::certs_dir()).unwrap();
        let name = crate::tls::server_name("127.0.0.1").unwrap();
        let tcp = dial(port);
        let conn = rustls::ClientConnection::new(cfg, name).unwrap();
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        while tls.conn.is_handshaking() {
            tls.conn.complete_io(&mut tls.sock).unwrap();
        }
        let mut ws = WsStream::handshake(tls, "127.0.0.1", port).unwrap();
        let (w, h, auth) =
            crate::display::vnc::handshake_server_init(&mut ws, Some("rightpw")).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(auth.as_str(), "vnc");
    }

    #[test]
    fn wss_wrong_ca_is_rejected() {
        // F28: the strict client config must really reject a server
        // whose certificate chains to a different CA — the companion of
        // rfb_handshake_over_wss_end_to_end above (same peer, same
        // script; only the trusted CA differs). If load_client_config
        // ever degraded to trust-everything, the handshake below would
        // succeed and this test would fail.
        let port = spawn_peer(|raw| {
            let dir = crate::tls::test_support::certs_dir().to_path_buf();
            let scfg = crate::tls::load_server_config(&dir).unwrap();
            let conn = rustls::ServerConnection::new(scfg).unwrap();
            let mut tls = rustls::StreamOwned::new(conn, raw);
            while tls.conn.is_handshaking() {
                // The client aborts mid-handshake (alert / close); any
                // error ends the peer — no ws bytes ever flow.
                if tls.conn.complete_io(&mut tls.sock).is_err() {
                    break;
                }
            }
            // Session never carries ws bytes: the client is gone.
        });
        let cfg = crate::tls::load_client_config(crate::tls::test_support::other_ca_dir())
            .unwrap();
        let name = crate::tls::server_name("127.0.0.1").unwrap();
        let conn = rustls::ClientConnection::new(cfg, name).unwrap();
        let mut tls = rustls::StreamOwned::new(conn, dial(port));
        let mut io_err = None;
        while tls.conn.is_handshaking() {
            match tls.conn.complete_io(&mut tls.sock) {
                Ok(_) => {}
                Err(e) => {
                    io_err = Some(e);
                    break;
                }
            }
        }
        let e = io_err.expect("an unrelated CA must fail the strict handshake");
        assert!(
            e.to_string().contains("certificate"),
            "rustls must reject the unrelated issuer: {e}"
        );
    }

    #[test]
    fn handshake_has_a_whole_exchange_budget() {
        // F32: each byte-wise read can succeed just inside its timeout,
        // so only a whole-exchange deadline bounds the head phase. The
        // peer drips head bytes slower than the tiny test budget; the
        // deadline trips between reads even though no read ever fails.
        let port = spawn_peer(|mut s| {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            for b in b"HTTP/1.1 101 Switching Protocols\r\n" {
                std::thread::sleep(Duration::from_millis(60));
                s.write_all(&[*b]).unwrap();
                s.flush().unwrap();
            }
        });
        let e = WsStream::handshake_with_budget(
            dial(port),
            "127.0.0.1",
            port,
            Duration::from_millis(150),
        )
        .unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("within"), "{e}");
    }

    #[test]
    fn ping_is_answered_and_close_is_eof() {
        // The stream layer answers pings with masked pongs and turns a
        // server close into a clean EOF for the RFB reader.
        let port = spawn_peer(|mut s| {
            accept_101(&mut s, &[]);
            s.write_all(&[0x89, 3, b'p', b'i', b'!']).unwrap(); // ping
            s.flush().unwrap();
            let pong = read_client_frame(&mut s);
            assert_eq!(pong, b"pi!".to_vec(), "pong echoes the ping payload");
            s.write_all(&server_frame(b"RFB 003.008\n")).unwrap();
            s.flush().unwrap();
            let _ = read_client_frame(&mut s); // client version
            s.write_all(&[0x88, 2, 0x03, 0xe8]).unwrap(); // close 1000
            s.flush().unwrap();
            let close = read_client_frame(&mut s);
            assert_eq!(close, vec![0x03, 0xe8], "close reply mirrors the code");
        });
        let mut ws = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap();
        let mut banner = [0u8; 12];
        ws.read_exact(&mut banner).unwrap();
        assert_eq!(&banner, b"RFB 003.008\n");
        ws.write_all(b"RFB 003.008\n").unwrap();
        // After the close frame: EOF, not an error.
        let mut sink = [0u8; 4];
        assert_eq!(ws.read(&mut sink).unwrap(), 0);
        assert_eq!(ws.read(&mut sink).unwrap(), 0);
    }

    // ------------------------------------------------------------------
    // W1-Q26 二十二刀：ws.rs 拒面与底座增量（既有 14 测未覆盖面）。
    // ------------------------------------------------------------------

    /// read_server_frame 拒面增量：保留位（RSV1..3）非零必拒；长度域
    /// 超 MAX_SERVER_FRAME（16MiB）必拒且消息带实际长度；截断头/截断
    /// 扩展长度/截断载荷 = UnexpectedEof（区分于协议拒的 InvalidData）。
    #[test]
    fn server_frame_rejects_reserved_bits_oversize_and_truncation() {
        // RSV1 set (0x40) on a binary frame.
        let bytes = [0xC2, 1, 0];
        let e = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
        assert!(e.to_string().contains("reserved bits"), "{e}");
        // Oversize: 64-bit length 17MiB (> 16MiB cap), payload absent —
        // the length check fires before any allocation/read.
        let mut bytes = vec![0x82, 127];
        bytes.extend_from_slice(&(((16u64 << 20) + 1).to_be_bytes()));
        let e = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
        assert!(e.to_string().contains("too large"), "{e}");
        assert!(
            e.to_string().contains(&format!("{}", (16 << 20) + 1)),
            "实际长度入文案: {e}"
        );
        // 恰 16MiB 不拒长度门（载荷截断在读时咬——EOF 面）。
        let mut bytes = vec![0x82, 127];
        bytes.extend_from_slice(&(16u64 << 20).to_be_bytes());
        let e = read_server_frame(&mut std::io::Cursor::new(&bytes)).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof, "{e}");
        // Truncated mid-frame: header only, then EOF.
        let e = read_server_frame(&mut std::io::Cursor::new(&[0x82])).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof, "{e}");
        // Truncated 16-bit extended length.
        let e = read_server_frame(&mut std::io::Cursor::new(&[0x82, 126, 0x01]))
            .unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof, "{e}");
        // Truncated 64-bit extended length.
        let e =
            read_server_frame(&mut std::io::Cursor::new(&[0x82, 127, 0, 0, 0])).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof, "{e}");
    }

    /// MAX_SERVER_FRAME 常量契约：16MiB——RFB 矩形载荷的绝对上限，
    /// 超界即协议拒（不先吃内存）。数值漂移（手滑改 8/32MiB）即红。
    #[test]
    fn max_server_frame_is_16mib() {
        assert_eq!(MAX_SERVER_FRAME, 16 << 20);
        assert_eq!(MAX_UPGRADE_HEAD, 8192);
    }

    /// 流层拒面：未知服务端 opcode（3 = 保留）在 WsStream 读路径上报
    /// InvalidData 带 opcode 编号；空二元帧被跳过（零长载荷不产出
    /// 0 字节 read——循环续读下一帧）。
    #[test]
    fn stream_rejects_unknown_opcode_and_skips_empty_frames() {
        let port = spawn_peer(|mut s| {
            accept_101(&mut s, &[]);
            // Empty binary frame (0 bytes): skipped by the read loop.
            s.write_all(&[0x82, 0]).unwrap();
            // Unknown opcode 3 with a small payload: protocol error.
            s.write_all(&[0x83, 2, 0x11, 0x22]).unwrap();
            s.flush().unwrap();
        });
        let mut ws = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap();
        let mut buf = [0u8; 4];
        let e = ws.read(&mut buf).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
        assert!(e.to_string().contains("unknown server opcode 3"), "{e}");
    }

    /// 流层空帧跳过的对面证：空帧后跟数据帧，数据照常读出（跳过不
    /// 吞后续帧）。
    #[test]
    fn empty_frame_skip_still_yields_later_data() {
        let port = spawn_peer(|mut s| {
            accept_101(&mut s, &[]);
            s.write_all(&[0x82, 0]).unwrap(); // empty binary
            s.write_all(&server_frame(b"ok")).unwrap();
            s.flush().unwrap();
        });
        let mut ws = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap();
        let mut buf = [0u8; 2];
        ws.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ok");
    }

    /// quasi_random 底座：n 字节输出、连续两次调用不同（nanos+counter
    /// 混合源）、n 可到 20（SHA-1 摘要 20 字节内任取）。ws_key() 是
    /// 它的 base64 面：恒 24 字符（16 字节 → ceil(16/3)*4 = 24，含 '='）。
    #[test]
    fn quasi_random_shape_and_uniqueness() {
        let a = quasi_random(16);
        let b = quasi_random(16);
        assert_eq!(a.len(), 16);
        assert_eq!(b.len(), 16);
        assert_ne!(a, b, "计数器至少保证连续调用不同");
        let big = quasi_random(20);
        assert_eq!(big.len(), 20, "摘要全量 20 字节内可取");
        let key = ws_key();
        assert_eq!(key.len(), 24, "16 字节 base64 恒 24 字符（RFC 6455 §4.1）");
        assert!(key.ends_with('='), "16 字节非 3 的倍数——尾 padding 恒在");
        assert!(key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'/' || c == b'='));
    }

    /// upgrade_request 逐字节钉板：行序、CRLF 形、尾空行、Host 拼接、
    /// Version 13——QEMU ws 端点解析面的出向契约（无 Sec-WebSocket-
    /// Protocol，probe f1）。
    #[test]
    fn upgrade_request_exact_bytes() {
        let req = upgrade_request("127.0.0.1", 6001, "AKEY==");
        let want = "GET / HTTP/1.1\r\n\
                    Host: 127.0.0.1:6001\r\n\
                    Upgrade: websocket\r\n\
                    Connection: Upgrade\r\n\
                    Sec-WebSocket-Key: AKEY==\r\n\
                    Sec-WebSocket-Version: 13\r\n\
                    \r\n";
        assert_eq!(req, want);
    }

    /// 握手拒面增量：升级头中途 EOF（对端 101 前关连接）与超长头
    /// （>8192 无终止符）。两者都是 protocol 码 7。
    #[test]
    fn handshake_rejects_eof_and_oversize_head() {
        // EOF mid-head: peer half-closes after a partial status line
        // (shutdown(Write) = clean FIN; a raw drop-close on macOS can
        // surface as ECONNRESET instead of EOF — both are read-path
        // failures, but FIN pins the deterministic EOF face).
        let port = spawn_peer(|mut s| {
            let _ = s.write_all(b"HTTP/1.1 101 Switching Proto");
            let _ = s.flush();
            let _ = s.shutdown(std::net::Shutdown::Write);
            std::thread::sleep(Duration::from_secs(3));
        });
        let e = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("EOF during"), "{e}");
        // Oversize head: >8192 bytes of header lines, no terminator.
        let port = spawn_peer(|mut s| {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let filler = "X-Fill: 0123456789abcdef\r\n".repeat(512);
            s.write_all(filler.as_bytes()).unwrap();
            s.flush().unwrap();
            // Hold the connection open: the client must trip the size
            // limit on its own (no EOF shortcut).
            std::thread::sleep(Duration::from_secs(3));
        });
        let e = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("exceeds 8192"), "{e}");
    }

    /// 头解析底座（UpgradeHead::parse 经 validate_upgrade 间接）：
    /// 头名大小写不敏感（SEC-WEBSOCKET-ACCEPT 大写形照过）、值前导
    /// 空白剥、Upgrade: WebSocket 值大小写不敏感、缺 Connection 头拒。
    #[test]
    fn head_parsing_case_insensitive_names_and_value_trim() {
        // All-caps header names + padded values must still validate.
        let port = spawn_peer(|mut s| {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let req = String::from_utf8(head).unwrap();
            let key = req
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
                .unwrap()
                .to_string();
            let resp = format!(
                "HTTP/1.1 101 Switching Protocols\r\n\
                 UPGRADE: WebSocket\r\n\
                 CONNECTION:   Upgrade\r\n\
                 SEC-WEBSOCKET-ACCEPT:  {}\r\n\r\n",
                expected_accept(&key)
            );
            s.write_all(resp.as_bytes()).unwrap();
            s.flush().unwrap();
        });
        let ws = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap();
        drop(ws); // validated: parsed and admitted
        // Missing Connection header entirely -> protocol error naming it.
        let port = spawn_peer(|mut s| {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let req = String::from_utf8(head).unwrap();
            let key = req
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
                .unwrap()
                .to_string();
            let resp = format!(
                "HTTP/1.1 101 Switching Protocols\r\n\
                 Upgrade: websocket\r\n\
                 Sec-WebSocket-Accept: {}\r\n\r\n",
                expected_accept(&key)
            );
            s.write_all(resp.as_bytes()).unwrap();
            s.flush().unwrap();
        });
        let e = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains("Connection"), "{e}");
    }

    /// 握手后首帧同段捎带（Prefixed 面）：101 头与首帧载荷同一次
    /// write 到达——Prefixed 必须把头后字节喂给帧解码器，不丢不重。
    /// 既有 handshake_accepts_101_with_locale_date_bytes 分两次写；本
    /// 测钉同段捎带形态（TCP 不保消息边界，QEMU 单 flush 常态）。
    #[test]
    fn first_frame_piggybacked_on_head_segment() {
        let port = spawn_peer(|mut s| {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let req = String::from_utf8(head).unwrap();
            let key = req
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
                .unwrap()
                .to_string();
            let mut resp = format!(
                "HTTP/1.1 101 Switching Protocols\r\n\
                 Upgrade: websocket\r\n\
                 Connection: Upgrade\r\n\
                 Sec-WebSocket-Accept: {}\r\n\r\n",
                expected_accept(&key)
            )
            .into_bytes();
            resp.extend_from_slice(&server_frame(b"RFB 003.008\n"));
            s.write_all(&resp).unwrap();
            s.flush().unwrap();
        });
        let mut ws = WsStream::handshake(dial(port), "127.0.0.1", port).unwrap();
        let mut banner = [0u8; 12];
        ws.read_exact(&mut banner).unwrap();
        assert_eq!(&banner, b"RFB 003.008\n");
    }
}
