//! `berry vnc web`: a std-only HTTP/1.1 server for the BerryOS browser
//! desktop (wave 2), grown a TLS plane in wave 5, a dual-tree asset
//! layout in wave 9, and a LAN single-port form in wave 10: the default
//! page is the own Apache-2.0 shell (`index.html` + `berry.js` +
//! `berry.css` at the web root), while the vendored noVNC tree stays
//! byte-identical under `novnc/` — the raw upstream page remains
//! reachable at `/novnc/vnc.html` as the escape hatch. Zero
//! web-framework dependencies — one TcpListener, one thread per
//! connection under a `--max-conns` admission cap, GET/HEAD only,
//! plus a hard per-connection time budget (F18).
//!
//! Data plane: in the default (plaintext run) mode the browser connects
//! straight to QEMU's websocket port via the `path=` URL parameter and
//! this server never proxies any RFB bytes. TLS runs are the one
//! documented exception (wave 5): QEMU's gnutls sends a TLS
//! `CertificateRequest` on every TLS endpoint, and Chrome's wss channel
//! refuses to pick a client certificate, dying silently with
//! `ERR_SSL_CLIENT_AUTH_CERT_NEEDED` — a browser cannot dial QEMU's wss
//! directly no matter what the page origin is (probe p3, netlog). So for
//! TLS runs berry terminates TLS on both sides (rustls): the page is
//! served over https and a local wss proxy relays the bytes between the
//! browser and QEMU's websocket port. The proxy is a pure byte pipe —
//! no HTTP parsing, no ws framing, no RFB decoding; Upgrade/101/masks
//! stay between the two endpoints. Wave 10 adds the second proxied form
//! for phones on the same Wi-Fi: `--lan` binds one port on 0.0.0.0 and
//! routes by path — page assets as usual, `/ws` (websocket Upgrade
//! heads only) into a plaintext byte pipe toward QEMU's loopback ws
//! port (QEMU never touches the LAN; the classic VNC DES authentication
//! stays end-to-end between browser and QEMU).
//!
//! noVNC 1.7.0 URL-form quirk (wave 6, probe f2): on an https page the
//! vendored `app/ui.js` rewrites the scheme of `path=` URLs to wss
//! unconditionally — an absolute `ws://` URL included — while the
//! host/port/`encrypt` form derives the scheme from the flag and is
//! never rewritten. Pages therefore only ever reach a plaintext ws
//! endpoint via host/port/`encrypt=false` (Chrome's loopback
//! mixed-content exemption covers the dial); `vnc web --tls` on a
//! plaintext run builds exactly that form.
//!
//! Wave 10 adds the single-port LAN shape (`--lan`): berry binds
//! `0.0.0.0` as the one LAN entry — pages and the browser ws dial share
//! the port via path routing, `/ws` upgrading into a plaintext byte-pipe
//! proxy toward QEMU's loopback ws port (QEMU itself never binds the
//! LAN). The proxy rewrites only the request-line target to `/` (QEMU's
//! ws endpoint 404s anything else, probe f3) and relays every other
//! byte verbatim — VNC DES auth stays end-to-end between browser and
//! QEMU. The own shell's bottom deck also gets a `POST /api/power/...`
//! control endpoint (reboot/off via the run's control plane), gated by
//! the run's VNC password whenever one exists so the LAN surface never
//! widens the run's posture.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::{BerryError, Result};

/// Header budget per request (request line + headers); anything bigger is a
/// bad request. noVNC loads assets with plain browser requests, well below.
const MAX_HEAD: usize = 8 * 1024;

/// Whole-connection budget: hard cap over the entire head+body exchange.
/// Slow-drip clients (one byte every few seconds) can hold a slot forever
/// under per-read timeouts alone; 30s covers the slowest legitimate browser
/// asset load while starving that attack. Exceeded -> drop the connection,
/// the RAII slot recycles. No 408 is written — a slow-drip or dead client
/// cannot read it anyway.
pub const CONN_BUDGET: Duration = Duration::from_secs(30);

/// TLS handshake budget for the https page server and the wss proxy
/// (both terminations). Data-plane piping after the handshake follows OS
/// semantics — a desktop session may live for hours.
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Web-root discovery (precedent: qemu.rs resource_dirs, plus a cwd upward
// walk for the in-repo development layout)
// ---------------------------------------------------------------------------

/// Ordered candidate roots holding a `novnc/` asset directory: exe dir,
/// macOS .app Resources, Linux share layout (`<exe>/../share/berry/web`,
/// e.g. an AppDir's `usr/share/berry/web`), per-user pack dirs, then the
/// repo checkout found by walking cwd upwards (`tools/berry/web`).
pub fn web_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.join("web"));
            if let Some(parent) = d.parent() {
                v.push(parent.join("Resources").join("web"));
                v.push(parent.join("share").join("berry").join("web"));
            }
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        v.push(PathBuf::from(home).join("Library/Application Support/BerryOS/web"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(local).join("BerryOS").join("web"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut cur: Option<&Path> = Some(&cwd);
        while let Some(d) = cur {
            v.push(d.join("tools/berry/web"));
            cur = d.parent();
        }
    }
    v
}

/// First root that actually contains the web assets. Wave 9 dual
/// sentinel: both `novnc/vnc.html` (vendored escape hatch) and
/// `index.html` (own default page) must be present — the web/ tree
/// ships as one unit, and a root missing the new page would serve a
/// dead default entry. Not found is an E_DEVICE with the packaging
/// story in the hint.
pub fn find_web_root() -> Result<PathBuf> {
    let roots = web_roots();
    find_web_root_in(&roots).ok_or_else(web_assets_missing)
}

/// The E_DEVICE for a failed root walk (pure, unit-pinned: the hint
/// must name BOTH sentinels — the own default page and the vendored
/// escape hatch).
pub(crate) fn web_assets_missing() -> BerryError {
    BerryError::device("VNC web assets not found").with_hint(
        "expected <root>/index.html and <root>/novnc/vnc.html under the berry exe dir, its Resources/web or share/berry/web sibling, ~/Library/Application Support/BerryOS/web, or the repo's tools/berry/web; see tools/berry/web/NOTICE.md",
    )
}

/// Pure sentinel walk over candidate roots (the testable core of
/// `find_web_root`).
pub(crate) fn find_web_root_in(roots: &[PathBuf]) -> Option<PathBuf> {
    roots.iter().find(|root| {
        root.join("novnc").join("vnc.html").is_file() && root.join("index.html").is_file()
    }).cloned()
}

// ---------------------------------------------------------------------------
// Page URL construction (noVNC 1.7.0 semantics: host/port/encrypt are
// deprecated, `path=` carries the absolute ws/wss URL; the password lives in
// the fragment, which browsers never send to any server)
// ---------------------------------------------------------------------------

/// Percent-encode a URI component (RFC 3986 unreserved keep-list).
pub(crate) fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `http://127.0.0.1:<port>/?autoconnect=true&path=<ws uri>&resize=scale`
/// plus `#password=<pw>` when a password is known (fragment never leaves the
/// browser; without one noVNC prompts in-page — probe p6).
pub fn build_page_url(web_port: u16, ws_uri: &str, password: Option<&str>) -> String {
    let mut url = format!(
        "http://127.0.0.1:{web_port}/?autoconnect=true&path={}&resize=scale",
        pct_encode(ws_uri)
    );
    if let Some(pw) = password {
        url.push_str(&format!("#password={}", pct_encode(pw)));
    }
    url
}

/// https page over a plaintext ws endpoint (`vnc web --tls <dir>` on a
/// non-TLS run). noVNC 1.7.0's `app/ui.js` `connect()` unconditionally
/// rewrites the scheme of `path=` URLs to wss on https pages — even an
/// absolute `ws://` URL dies against QEMU's plaintext port (wave-6 probe
/// f2: Chrome then TLS-dials the plain port and QEMU never answers the
/// ClientHello) — while the host/port form derives the scheme from the
/// `encrypt` flag and is left alone. So the page passes
/// host/port/`encrypt=false` and the browser dials the plain ws endpoint
/// directly; Chrome exempts loopback origins from mixed-content blocking
/// (probe p3/f2). `path=` must be PRESENT and empty (an absent path
/// defaults to "websockify", which QEMU's root-only ws endpoint 404s).
/// `resize=scale` mirrors the http page semantics.
pub fn build_page_url_https(
    page_port: u16,
    host: &str,
    ws_port: u16,
    password: Option<&str>,
) -> String {
    let mut url = format!(
        "https://127.0.0.1:{page_port}/?autoconnect=true&host={}&port={ws_port}&encrypt=false&path=&resize=scale",
        pct_encode(host)
    );
    if let Some(pw) = password {
        url.push_str(&format!("#password={}", pct_encode(pw)));
    }
    url
}

/// TLS-run page URL (wave 5, noVNC 1.7.0 wire truth, probe p3): the page
/// is https and the browser connects to the local wss proxy via
/// host/port/`encrypt=true` — NOT via `path=<wss URL>`, which 1.7.0 does
/// not recognize and silently falls back to a plaintext connection (tap
/// evidence). `path=` must be PRESENT with an empty value: an absent path
/// defaults to "websockify" and QEMU's ws endpoint only accepts the root
/// path, so the default 404s. `resize=off` keeps noVNC from re-scaling
/// through the proxy. The password rides the fragment exactly like the
/// plaintext URL.
pub fn build_page_url_tls(
    page_port: u16,
    host: &str,
    proxy_port: u16,
    password: Option<&str>,
) -> String {
    let mut url = format!(
        "https://127.0.0.1:{page_port}/?autoconnect=true&host={}&port={proxy_port}&encrypt=true&path=&resize=off",
        pct_encode(host)
    );
    if let Some(pw) = password {
        url.push_str(&format!("#password={}", pct_encode(pw)));
    }
    url
}

/// LAN page URL (wave 10, `vnc web --lan`): plaintext http page on the
/// LAN address whose `path=` points at the SAME port's `/ws` route —
/// berry is the single LAN entrance, the browser dials the page and
/// the proxied ws through one port. The own shell consumes the absolute
/// `ws://` `path=` verbatim (`new URL().href` keeps the path), no
/// browser-side change. Bare IPv6 hosts get brackets via the shared
/// `addr_host` semantics; the fragment rides exactly like the other
/// builders (and `--qr` content is this URL with the fragment
/// stripped).
pub fn build_page_url_lan(lan_host: &str, web_port: u16, password: Option<&str>) -> String {
    let ws_uri = format!("ws://{}/ws", crate::vnc_cfg::addr_host(lan_host, web_port));
    let mut url = format!(
        "http://{}/?autoconnect=true&path={}&resize=scale",
        crate::vnc_cfg::addr_host(lan_host, web_port),
        pct_encode(&ws_uri)
    );
    if let Some(pw) = password {
        url.push_str(&format!("#password={}", pct_encode(pw)));
    }
    url
}

/// Single-port LAN page URL (wave 10): page and browser ws dial share
/// one `0.0.0.0` port; `path=` carries the same-origin `/ws` route that
/// the server proxies to QEMU's loopback ws port. berry.js consumes the
/// `path=` form verbatim (`new URL().href` keeps the `/ws` path), so
/// the browser side is unchanged. Password fragment semantics are
/// byte-identical to the other three builders.
pub fn build_page_url_lan(lan_host: &str, web_port: u16, password: Option<&str>) -> String {
    let hostport = crate::vnc_cfg::addr_host(lan_host, web_port);
    let ws_uri = format!("ws://{hostport}/ws");
    let mut url = format!(
        "http://{hostport}/?autoconnect=true&path={}&resize=scale",
        pct_encode(&ws_uri)
    );
    if let Some(pw) = password {
        url.push_str(&format!("#password={}", pct_encode(pw)));
    }
    url
}

// ---------------------------------------------------------------------------
// Fragment stripping (wave 9 `--open`: the one-shot `open` argv must not
// carry the password, same discipline as "no password in child argv")
// ---------------------------------------------------------------------------

/// Strip the URL fragment — everything from the first `#` on. A URL
/// without a fragment comes back unchanged (empty string included).
pub(crate) fn strip_fragment(url: &str) -> &str {
    match url.split_once('#') {
        Some((before, _)) => before,
        None => url,
    }
}

// ---------------------------------------------------------------------------
// wss termination proxy (wave 5, TLS runs only)
// ---------------------------------------------------------------------------

/// One rustls session of the proxy pair behind a mutex: rustls
/// connections are `Send` but not `Sync`, and both pump directions touch
/// both sessions. All critical sections are short — socket I/O happens
/// on duplicated handles outside the lock (see `ProxyEndpoint`).
enum ProxyConn {
    Server(rustls::ServerConnection),
    Client(rustls::ClientConnection),
}

impl ProxyConn {
    fn feed(&mut self, chunk: &[u8]) -> std::io::Result<()> {
        // Hand raw TLS bytes to the deframer; read_tls consumes one
        // record per call, so loop until the chunk is drained. An empty
        // chunk must NOT reach read_tls: reading Ok(0) from the cursor is
        // indistinguishable from a transport EOF and rustls latches
        // has_seen_eof, killing the session (the buffered-records drain
        // at pump start goes through process/flush only).
        if chunk.is_empty() {
            return Ok(());
        }
        let mut cur = std::io::Cursor::new(chunk);
        // Drive by remaining bytes, never by read_tls's Ok(0): a zero
        // return means "transport EOF" to rustls and latches has_seen_eof
        // — a chunk boundary here is not an EOF.
        while cur.position() < chunk.len() as u64 {
            let n = match self {
                ProxyConn::Server(c) => c.read_tls(&mut cur)?,
                ProxyConn::Client(c) => c.read_tls(&mut cur)?,
            };
            if n == 0 {
                break;
            }
        }
        Ok(())
    }

    fn process(&mut self) -> std::io::Result<()> {
        let r = match self {
            ProxyConn::Server(c) => c.process_new_packets(),
            ProxyConn::Client(c) => c.process_new_packets(),
        };
        r.map(|_| ()).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("tls: {e}"))
        })
    }

    /// Drain buffered plaintext. rustls's `Reader::read` reports
    /// `Err(WouldBlock)` once the buffer is empty while the session is
    /// still live (it is not a plain stream!) — that is the normal
    /// "nothing more" signal here, not an error. `Ok(0)` means the peer
    /// sent close_notify: the session is cleanly over.
    fn drain_plain(&mut self, out: &mut Vec<u8>) -> std::io::Result<()> {
        loop {
            let mut tmp = [0u8; 8192];
            let res = match self {
                ProxyConn::Server(c) => c.reader().read(&mut tmp),
                ProxyConn::Client(c) => c.reader().read(&mut tmp),
            };
            match res {
                Ok(0) => return Ok(()),
                Ok(n) => out.extend_from_slice(&tmp[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => return Err(e),
            }
            if out.len() > 4 << 20 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "tls: plaintext round overflow",
                ));
            }
        }
    }

    fn queue_plain(&mut self, plain: &[u8]) -> std::io::Result<()> {
        match self {
            ProxyConn::Server(c) => c.writer().write_all(plain),
            ProxyConn::Client(c) => c.writer().write_all(plain),
        }
    }

    fn wants_write(&self) -> bool {
        match self {
            ProxyConn::Server(c) => c.wants_write(),
            ProxyConn::Client(c) => c.wants_write(),
        }
    }

    fn write_tls(&mut self, w: &mut TcpStream) -> std::io::Result<usize> {
        match self {
            ProxyConn::Server(c) => c.write_tls(w),
            ProxyConn::Client(c) => c.write_tls(w),
        }
    }
}

/// Shared half of one proxied TLS endpoint: the session (mutex) and the
/// write side of the socket (mutex — two threads may push TLS records:
/// this direction's reader flushing handshake byproducts, the other
/// direction delivering plaintext). The read side of the socket is owned
/// exclusively by this endpoint's pump thread (never under a lock).
struct ProxyEndpoint {
    conn: std::sync::Mutex<ProxyConn>,
    wr: std::sync::Mutex<TcpStream>,
}

impl ProxyEndpoint {
    /// Feed raw socket bytes in, get decrypted plaintext out. Also
    /// flushes anything the session wants to send (post-handshake
    /// records like NewSessionTicket or KeyUpdate acks). Short lock: the
    /// blocking socket read happened before this call.
    fn deframe(&self, chunk: &[u8]) -> std::io::Result<Vec<u8>> {
        let mut c = self.conn.lock().unwrap();
        c.feed(chunk)?;
        c.process()?;
        self.flush_locked(&mut c)?;
        let mut out = Vec::new();
        c.drain_plain(&mut out)?;
        Ok(out)
    }

    /// Deliver plaintext into the session and push the resulting TLS
    /// records to the socket.
    fn send(&self, plain: &[u8]) -> std::io::Result<()> {
        if plain.is_empty() {
            return Ok(());
        }
        let mut c = self.conn.lock().unwrap();
        c.queue_plain(plain)?;
        self.flush_locked(&mut c)
    }

    fn flush_locked(&self, c: &mut ProxyConn) -> std::io::Result<()> {
        while c.wants_write() {
            let mut w = self.wr.lock().unwrap();
            match c.write_tls(&mut w)? {
                0 => return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "tls: socket closed",
                )),
                _ => {}
            }
        }
        Ok(())
    }
}

/// One direction of the byte pipe: raw bytes from `rd` (never under a
/// lock) -> plaintext via `from` -> `to`. Any EOF or error — including
/// rustls reporting a missing close_notify as `UnexpectedEof`, the
/// documented 0.23 quirk — ends the direction and shuts both proxied
/// sockets down, which unblocks the sibling thread.
fn pump_direction(
    mut rd: TcpStream,
    from: Arc<ProxyEndpoint>,
    to: Arc<ProxyEndpoint>,
    shutdown: Arc<Vec<TcpStream>>,
) {
    // Records may already sit in the session buffers when the pump starts
    // (TLS 1.3 lets the peer pipeline its first app-data record with the
    // Finished); drain them before parking on the socket.
    loop {
        match from.deframe(&[]) {
            Ok(p) if p.is_empty() => break,
            Ok(p) => {
                if to.send(&p).is_err() {
                    break;
                }
            }
            Err(_) => {
                // Session dead before the first byte: still tear both
                // sides down so the sibling thread unblocks.
                for s in shutdown.iter() {
                    let _ = s.shutdown(std::net::Shutdown::Both);
                }
                return;
            }
        }
    }
    let mut raw = [0u8; 16 * 1024];
    loop {
        let n = match rd.read(&mut raw) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        let plain = match from.deframe(&raw[..n]) {
            Ok(p) => p,
            Err(_) => break,
        };
        if !plain.is_empty() && to.send(&plain).is_err() {
            break;
        }
    }
    for s in shutdown.iter() {
        let _ = s.shutdown(std::net::Shutdown::Both);
    }
}

/// Serve the wss proxy on an already-bound listener: per connection,
/// terminate the browser's TLS with `server_cfg`, dial the QEMU ws port
/// and establish a strict-verified client session (`client_cfg`,
/// `upstream_name`), then run the two-direction byte pipe. Admission
/// follows the same `max_conns` cap as the page server (shared cap
/// semantics: the proxy has its own live counter of the same size).
/// `tls_dir` is the `--tls` directory, carried only for the upstream
/// failure diagnostic: the page keeps loading while every wss
/// connection dies at the QEMU-side handshake (e.g. the directory's CA
/// did not issue the run's certificate), so a silent error would leave
/// the operator with zero clues.
pub fn serve_wss_proxy(
    listener: TcpListener,
    upstream_addr: String,
    server_cfg: Arc<rustls::ServerConfig>,
    client_cfg: Arc<rustls::ClientConfig>,
    upstream_name: rustls::pki_types::ServerName<'static>,
    max_conns: usize,
    tls_dir: std::path::PathBuf,
) {
    let live = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(sock) = stream else { continue };
        if !admit_connection(live.fetch_add(1, Ordering::AcqRel), max_conns) {
            live.fetch_sub(1, Ordering::AcqRel);
            eprintln!(
                "berry: vnc web: wss proxy connection limit {max_conns} reached; refusing a client"
            );
            let _ = sock.shutdown(std::net::Shutdown::Both);
            continue;
        }
        let slot = ConnSlot(Arc::clone(&live));
        let server_cfg = server_cfg.clone();
        let client_cfg = client_cfg.clone();
        let upstream = upstream_addr.clone();
        let name = upstream_name.clone();
        let tls_dir = tls_dir.clone();
        std::thread::spawn(move || {
            let _slot = slot;
            // Relay-phase errors stay silent (peer hangup is the normal
            // end of a session); establishment errors already reported
            // by `proxy_conn` itself.
            let _ = proxy_conn(sock, &upstream, &server_cfg, &client_cfg, name, &tls_dir);
        });
    }
}

/// One proxied session (both TLS terminations + the pipe). Errors are
/// returned to the spawner, which only drops the connection.
fn proxy_conn(
    browser: TcpStream,
    upstream_addr: &str,
    server_cfg: &Arc<rustls::ServerConfig>,
    client_cfg: &Arc<rustls::ClientConfig>,
    upstream_name: rustls::pki_types::ServerName<'static>,
    tls_dir: &std::path::Path,
) -> std::io::Result<()> {
    // Browser side: TLS server handshake.
    let b = accept_tls(browser, server_cfg)?;
    // QEMU side: dial + strict-verified TLS client handshake. QEMU sends
    // a CertificateRequest even with verify-peer=off; the empty-certificate
    // answer rustls gives under with_no_client_auth is accepted (probe p1).
    // Establishment failures surface on stderr (F30): without this the
    // page loads fine while every wss connection dies wordlessly.
    let upstream_fail = |e: std::io::Error| {
        eprintln!(
            "berry: vnc web: wss proxy upstream {upstream_addr} failed (cert dir {}): {e}",
            tls_dir.display()
        );
        e
    };
    let up = TcpStream::connect(upstream_addr).map_err(&upstream_fail)?;
    up.set_read_timeout(Some(TLS_HANDSHAKE_TIMEOUT)).map_err(&upstream_fail)?;
    up.set_write_timeout(Some(TLS_HANDSHAKE_TIMEOUT)).map_err(&upstream_fail)?;
    let conn = rustls::ClientConnection::new(Arc::clone(client_cfg), upstream_name)
        .map_err(|e| upstream_fail(std::io::Error::other(format!("tls client setup: {e}"))))?;
    let mut q = rustls::StreamOwned::new(conn, up);
    while q.conn.is_handshaking() {
        q.conn.complete_io(&mut q.sock).map_err(&upstream_fail)?;
    }
    // Data plane: OS semantics (blocking, no timeouts) — a desktop
    // session may stay up for hours.
    b.sock.set_read_timeout(None)?;
    b.sock.set_write_timeout(None)?;
    q.sock.set_read_timeout(None)?;
    q.sock.set_write_timeout(None)?;

    let (b_conn, b_sock) = b.into_parts();
    let (q_conn, q_sock) = q.into_parts();
    let b_rd = b_sock.try_clone()?;
    let b_wr = b_sock.try_clone()?;
    let q_rd = q_sock.try_clone()?;
    let q_wr = q_sock.try_clone()?;
    let shutdown: Arc<Vec<TcpStream>> = Arc::new(vec![b_sock, q_sock]);
    let ep_b = Arc::new(ProxyEndpoint {
        conn: std::sync::Mutex::new(ProxyConn::Server(b_conn)),
        wr: std::sync::Mutex::new(b_wr),
    });
    let ep_q = Arc::new(ProxyEndpoint {
        conn: std::sync::Mutex::new(ProxyConn::Client(q_conn)),
        wr: std::sync::Mutex::new(q_wr),
    });
    let t1 = {
        let sd = shutdown.clone();
        let (a, b) = (ep_b.clone(), ep_q.clone());
        std::thread::spawn(move || pump_direction(b_rd, a, b, sd))
    };
    let t2 = {
        let sd = shutdown.clone();
        let (a, b) = (ep_q.clone(), ep_b.clone());
        std::thread::spawn(move || pump_direction(q_rd, a, b, sd))
    };
    let _ = t1.join();
    let _ = t2.join();
    Ok(())
}

// ---------------------------------------------------------------------------
// LAN single-port ws proxy (wave 10, --lan): /ws Upgrade -> plaintext
// byte pipe toward QEMU's loopback ws port
// ---------------------------------------------------------------------------

/// Upstream dial target for the LAN `/ws` proxy: QEMU's loopback
/// websocket port as a `host:port` string (host from
/// `vnc_cfg::dial_host` of the run's bind — the proxy, not QEMU, is
/// what the LAN sees).
#[derive(Clone, Debug)]
pub struct LanUpstream {
    pub addr: String,
}

/// Connect budget for the upstream dial (the `vnc check` 10s discipline).
const LAN_UPSTREAM_CONNECT: Duration = Duration::from_secs(10);

/// Does the raw request head carry a websocket Upgrade? Header names
/// are case-insensitive; the `Upgrade` value must equal `websocket`
/// (case-insensitive token per RFC 6455 §4.1) and a non-empty
/// `Sec-WebSocket-Key` must be present — both together distinguish a
/// real browser ws dial from a plain GET of a path that happens to be
/// `/ws`. Pure, byte-oriented.
pub(crate) fn head_has_upgrade(raw: &[u8]) -> bool {
    let text = String::from_utf8_lossy(raw);
    let mut upgrade = false;
    let mut key = false;
    for line in text.lines().skip(1) {
        let line = line.trim_end_matches('\r');
        let Some((name, value)) = line.split_once(':') else { continue };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "upgrade" => {
                // Comma-separated token list is legal; browsers send one.
                upgrade = value
                    .split(',')
                    .any(|t| t.trim().eq_ignore_ascii_case("websocket"));
            }
            "sec-websocket-key" => key = !value.is_empty(),
            _ => {}
        }
        if upgrade && key {
            return true;
        }
    }
    false
}

/// Rewrite the ws upgrade request head for the LAN proxy: the request
/// line's target is spliced out and replaced by `/` — QEMU's ws
/// endpoint accepts only the root path (probe f3: `/ws` and `/foo`
/// both 404) — while every other byte (headers, CRLFs, the final blank
/// line, odd-but-legal spacing) passes through verbatim; the head is
/// never re-serialized. Malformed (no newline, no second space, empty
/// version) -> None, the caller drops the connection.
pub(crate) fn rewrite_ws_request_head(raw: &[u8]) -> Option<Vec<u8>> {
    let nl = raw.iter().position(|&b| b == b'\n')?;
    let mut line = &raw[..nl];
    if line.last() == Some(&b'\r') {
        line = &line[..line.len() - 1];
    }
    let sp1 = line.iter().position(|&b| b == b' ')?;
    let sp2 = sp1 + 1 + line[sp1 + 1..].iter().position(|&b| b == b' ')?;
    if sp2 + 1 >= line.len() {
        return None; // no version token after the target
    }
    let mut out = Vec::with_capacity(raw.len());
    out.extend_from_slice(&line[..=sp1]); // method + first space
    out.push(b'/'); // the replacement target
    out.extend_from_slice(&line[sp2..]); // second space + version
    out.extend_from_slice(&raw[nl..]); // CRLF + all remaining head bytes
    Some(out)
}

/// Dial the upstream within the connect budget. The address is always a
/// literal `host:port` built from `dial_host`, so it parses to one
/// socket address (multi-A names cannot occur).
fn dial_upstream(addr: &str) -> std::io::Result<TcpStream> {
    let sa: std::net::SocketAddr = addr
        .parse()
        .map_err(|_| std::io::Error::other(format!("lan proxy upstream address '{addr}'")))?;
    TcpStream::connect_timeout(&sa, LAN_UPSTREAM_CONNECT)
}

/// One direction of the plain byte pipe: raw bytes `rd` -> `wr`; any
/// EOF or error ends the direction and shuts BOTH proxied sockets
/// down, which unblocks the sibling thread (the `pump_direction`
/// teardown discipline without the TLS layer).
fn pump_plain(mut rd: TcpStream, mut wr: TcpStream, shutdown: Arc<Vec<TcpStream>>) {
    let mut buf = [0u8; 16 * 1024];
    loop {
        match rd.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if wr.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    for s in shutdown.iter() {
        let _ = s.shutdown(std::net::Shutdown::Both);
    }
}

/// The LAN ws byte pipe (wave 10): dial QEMU's loopback ws port, send
/// the rewritten upgrade head (target -> `/`; QEMU only accepts the
/// root path) plus any bytes the head reader had buffered past it,
/// then relay both directions until either side hangs up. The data
/// plane follows OS semantics — no timeouts, a desktop session may
/// live for hours (so this branch is exempt from CONN_BUDGET, which
/// exists to starve slow-drip attacks in the HTTP head phase).
/// Upstream establishment failures are voiced on stderr (the F30
/// discipline: the page keeps loading while every ws connection dies
/// wordlessly otherwise); after establishment, relay errors stay
/// silent — a peer hangup is the normal end of a session.
fn proxy_lan_ws(
    browser: TcpStream,
    raw_head: &[u8],
    leftover: Vec<u8>,
    upstream: &LanUpstream,
) -> std::io::Result<()> {
    let Some(head) = rewrite_ws_request_head(raw_head) else {
        return Ok(()); // malformed head: drop, the browser retries
    };
    let up = match dial_upstream(&upstream.addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "berry: vnc web: lan proxy upstream {} failed: {e}",
                upstream.addr
            );
            return Ok(());
        }
    };
    let _ = browser.set_read_timeout(None);
    let _ = browser.set_write_timeout(None);
    let _ = up.set_read_timeout(None);
    let _ = up.set_write_timeout(None);
    {
        let mut first = up.try_clone()?;
        first.write_all(&head)?;
        if !leftover.is_empty() {
            first.write_all(&leftover)?;
        }
        first.flush()?;
    }
    let b_rd = browser.try_clone()?;
    let b_wr = browser.try_clone()?;
    let q_rd = up.try_clone()?;
    let q_wr = up.try_clone()?;
    let shutdown: Arc<Vec<TcpStream>> = Arc::new(vec![browser, up]);
    let t1 = {
        let sd = shutdown.clone();
        std::thread::spawn(move || pump_plain(b_rd, q_wr, sd))
    };
    let t2 = {
        let sd = shutdown.clone();
        std::thread::spawn(move || pump_plain(q_rd, b_wr, sd))
    };
    let _ = t1.join();
    let _ = t2.join();
    Ok(())
}

// ---------------------------------------------------------------------------
// Connection admission (wave 3): one thread per connection needs a cap
// ---------------------------------------------------------------------------

/// Default concurrent-connection cap for the page server.
pub const DEFAULT_MAX_CONNS: usize = 16;

/// Hard ceiling accepted by `--max-conns` (a loopback dev page never needs
/// more; the bound keeps a typo from spinning up thousands of threads).
pub const MAX_CONNS_CEILING: usize = 1024;

/// `--max-conns` value parser: 1..=1024. clap renders the error and exits
/// with rc=2 (usage-error semantics).
pub fn parse_max_conns(raw: &str) -> std::result::Result<usize, String> {
    match raw.parse::<usize>() {
        Ok(n) if (1..=MAX_CONNS_CEILING).contains(&n) => Ok(n),
        Ok(n) => Err(format!(
            "must be between 1 and {MAX_CONNS_CEILING}, got {n}"
        )),
        Err(_) => Err(format!(
            "must be a number between 1 and {MAX_CONNS_CEILING}, got '{raw}'"
        )),
    }
}

/// Admission decision for one fresh connection given the live count and the
/// cap (pure, unit-pinned).
pub(crate) fn admit_connection(live: usize, max_conns: usize) -> bool {
    live < max_conns
}

/// The 503 response sent to over-limit clients (pure renderer, shared by
/// serve(); pinned by unit test).
pub(crate) fn overload_response(max_conns: usize) -> String {
    let body = format!("connection limit reached ({max_conns}); close a tab and retry\n");
    format!(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// RAII decrement for the live-connection count: Drop runs on normal return,
/// error paths, and panic unwind, so an admitted slot cannot leak silently.
struct ConnSlot(Arc<AtomicUsize>);

impl Drop for ConnSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Upstream dial address (`127.0.0.1:<ws port>`) for the single-port LAN
/// proxy (`--lan`): browser ws upgrade → plaintext byte pipe → QEMU's
/// loopback ws port. QEMU never binds the LAN; berry is the only entry.
#[derive(Clone, Debug)]
pub struct LanUpstream {
    pub dial: String,
}

/// Control-API state for the deck's power keys (`POST /api/power/<action>`):
/// the run to steer plus its VNC password. When the run carries a password
/// (mandatory on `--lan`) the endpoint demands it in `X-Berry-Pass`, so the
/// control surface is exactly as strong as the run's own auth; passwordless
/// (loopback-only) runs skip the gate.
pub struct CtlApi {
    pub run: PathBuf,
    pub pass: Option<String>,
}

/// Per-server context threaded through every connection: the dual web-tree
/// root, the optional LAN `/ws` proxy upstream, and the optional control
/// API.
pub(crate) struct ServeCtx {
    pub root: PathBuf,
    pub lan: Option<LanUpstream>,
    pub ctl: Option<Arc<CtlApi>>,
}

/// Static-only context (tests; also the pre-wave-10 shape).
pub(crate) fn serve_ctx(root: &Path) -> ServeCtx {
    ServeCtx { root: root.to_path_buf(), lan: None, ctl: None }
}

/// Bind `bind_host` ("127.0.0.1", or "0.0.0.0" in `--lan` mode) and serve
/// the dual web tree forever (own assets at the root, vendored `novnc/`
/// under its prefix; foreground, like `berry events --follow`). Returns
/// only on bind failure. With `tls` the same asset pipeline runs inside a
/// rustls server session (https). Connections beyond `max_conns` get one
/// 503 and one stderr line; the listener keeps accepting and never
/// crashes.
pub fn serve(
    root: &Path,
    port: u16,
    max_conns: usize,
    tls: Option<Arc<rustls::ServerConfig>>,
    bind_host: &str,
    lan: Option<LanUpstream>,
    ctl: Option<Arc<CtlApi>>,
) -> Result<()> {
    let listener = TcpListener::bind((bind_host, port)).map_err(|e| {
        BerryError::device(format!("bind web server {bind_host}:{port}: {e}"))
    })?;
    let ctx = Arc::new(ServeCtx { root: root.to_path_buf(), lan, ctl });
    serve_listener(listener, max_conns, tls, ctx);
    Ok(())
}

/// Accept loop over an already-bound listener: admission gate, 503 line,
/// per-connection thread, RAII slot recycling. Split out of `serve` so the
/// service-level behavior is unit-testable against an ephemeral listener.
pub(crate) fn serve_listener(
    listener: TcpListener,
    max_conns: usize,
    tls: Option<Arc<rustls::ServerConfig>>,
    ctx: Arc<ServeCtx>,
) {
    let live = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if !admit_connection(live.fetch_add(1, Ordering::AcqRel), max_conns) {
            live.fetch_sub(1, Ordering::AcqRel);
            eprintln!(
                "berry: vnc web: connection limit {max_conns} reached; refusing a client with 503"
            );
            let _ = stream.write_all(overload_response(max_conns).as_bytes());
            let _ = stream.flush();
            continue;
        }
        let slot = ConnSlot(Arc::clone(&live));
        let tls = tls.clone();
        let ctx = Arc::clone(&ctx);
        std::thread::spawn(move || {
            let _slot = slot; // moved in: Drop decrements on every exit path
            match tls {
                Some(cfg) => match accept_tls(stream, &cfg) {
                    Ok(mut tls_stream) => {
                        let _ = handle_conn(&mut tls_stream, &ctx);
                    }
                    Err(_) => {} // failed handshake: drop quietly
                },
                None => {
                    // Per-read timeout (pre-wave-5 behavior): wakes
                    // read_head so the F18 deadline is actually enforced
                    // against a client that drips without newlines.
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                    let _ = handle_conn_plain(&mut stream, &ctx);
                }
            }
        });
    }
}

/// Run one rustls server handshake over a fresh socket within the
/// handshake budget. The socket timeouts armed here also bound the
/// request head phase afterwards (a full request exchange stays well
/// under 10s; the per-connection budget caps the rest).
fn accept_tls(
    sock: TcpStream,
    cfg: &Arc<rustls::ServerConfig>,
) -> std::io::Result<rustls::StreamOwned<rustls::ServerConnection, TcpStream>> {
    sock.set_read_timeout(Some(TLS_HANDSHAKE_TIMEOUT))?;
    sock.set_write_timeout(Some(TLS_HANDSHAKE_TIMEOUT))?;
    let conn = rustls::ServerConnection::new(Arc::clone(cfg))
        .map_err(|e| std::io::Error::other(format!("tls server setup: {e}")))?;
    let mut s = rustls::StreamOwned::new(conn, sock);
    while s.conn.is_handshaking() {
        s.conn.complete_io(&mut s.sock)?;
    }
    Ok(s)
}

fn handle_conn<S: Read + Write>(stream: &mut S, ctx: &ServeCtx) -> std::io::Result<()> {
    handle_conn_budget(stream, ctx, CONN_BUDGET)
}

/// Plain-TCP request handling: `handle_conn_budget` plus the single-port
/// LAN `/ws` proxy branch (needs the raw socket for the byte pipe; the
/// TLS plane never coexists with `--lan`, contract 6).
fn handle_conn_plain(stream: &mut TcpStream, ctx: &ServeCtx) -> std::io::Result<()> {
    let deadline = Instant::now() + CONN_BUDGET;
    let head = {
        let mut reader = BufReader::new(&mut *stream);
        match read_head(&mut reader, deadline) {
            Ok(h) => h,
            Err(HeadFail::Code(code)) => {
                return write_simple(stream, code, code == 404, &[]);
            }
            Err(HeadFail::Budget) => {
                return Ok(()); // silent drop: slot recycles via Drop
            }
        }
    };
    if Instant::now() >= deadline {
        return Ok(());
    }
    let (method, target, raw) = head;
    // LAN `/ws` route: GET + sanitized target "ws" + a websocket upgrade
    // head → hand the raw socket to the byte-pipe proxy (which is exempt
    // from the connection budget — a desktop session may live for hours,
    // the same data-plane semantics as the wss proxy).
    if let Some(up) = ctx.lan.as_ref() {
        if method == "GET"
            && sanitize_path(&target).as_deref() == Some("ws")
            && head_has_ws_upgrade(&raw)
        {
            return proxy_ws_plain(stream, raw, &up.dial);
        }
    }
    respond_request(stream, &method, &target, &raw, ctx)
}

/// One request-response exchange under a whole-connection time budget
/// (F18): the deadline is checked on every head line and again before the
/// response is written; exceeded means a silent close (a slow-drip client
/// cannot read an error page). Budget is a parameter so tests can shrink
/// it; production always passes `CONN_BUDGET`.
fn handle_conn_budget<S: Read + Write>(
    stream: &mut S,
    ctx: &ServeCtx,
    budget: Duration,
) -> std::io::Result<()> {
    let deadline = Instant::now() + budget;
    let head = {
        let mut reader = BufReader::new(&mut *stream);
        match read_head(&mut reader, deadline) {
            Ok(h) => h,
            Err(HeadFail::Code(code)) => {
                return write_simple(stream, code, code == 404, &[]);
            }
            Err(HeadFail::Budget) => {
                return Ok(()); // silent drop: slot recycles via Drop
            }
        }
    };
    if Instant::now() >= deadline {
        return Ok(());
    }
    let (method, target, raw) = head;
    respond_request(stream, &method, &target, &raw, ctx)
}

/// Shared request dispatcher (generic over the transport): the control
/// API first, then the GET/HEAD static dual tree.
fn respond_request<S: Read + Write>(
    stream: &mut S,
    method: &str,
    target: &str,
    raw: &[u8],
    ctx: &ServeCtx,
) -> std::io::Result<()> {
    if sanitize_path(target).as_deref() == Some("api/power") {
        return api_power(stream, method, raw, "", ctx);
    }
    if let Some(action) = sanitize_path(target)
        .as_deref()
        .and_then(|r| r.strip_prefix("api/power/"))
    {
        return api_power(stream, method, raw, action, ctx);
    }
    if method != "GET" && method != "HEAD" {
        return write_simple(stream, 405, false, b"method not allowed\n");
    }
    let rel = match sanitize_path(&target.to_string()) {
        Some(r) => r,
        None => return write_simple(stream, 400, false, b"bad path\n"),
    };
    // Dual-tree resolution (wave 9): `novnc/<rel>` stays in the vendored
    // tree (raw escape hatch), everything else resolves against the web
    // root itself (own assets: index.html/berry.js/berry.css/NOTICE.md).
    let (base, rel) = match rel.strip_prefix("novnc/") {
        Some(sub) => (ctx.root.join("novnc"), sub.to_string()),
        None => (ctx.root.to_path_buf(), rel),
    };
    let path = base.join(&rel);
    // Defense in depth: the resolved file must stay under its base tree.
    let canon = match std::fs::canonicalize(&path) {
        Ok(p) => p,
        Err(_) => return write_simple(stream, 404, true, b"not found\n"),
    };
    let base_canon = std::fs::canonicalize(&base).unwrap_or(base);
    if !canon.starts_with(&base_canon) {
        return write_simple(stream, 400, false, b"bad path\n");
    }
    let body = match std::fs::read(&canon) {
        Ok(b) => b,
        Err(_) => return write_simple(stream, 404, true, b"not found\n"),
    };
    let mime = mime_for(&rel);
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(resp.as_bytes())?;
    if method == "GET" {
        stream.write_all(&body)?;
    }
    stream.flush()
}

// ---------------------------------------------------------------------------
// Control API (deck power keys): POST /api/power/<reboot|off>
// ---------------------------------------------------------------------------

/// Handle one control-API request. Empty-body POST design on purpose: the
/// head phase is the only read phase, so no buffered-body edge exists.
fn api_power<S: Read + Write>(
    stream: &mut S,
    method: &str,
    raw: &[u8],
    action: &str,
    ctx: &ServeCtx,
) -> std::io::Result<()> {
    use serde_json::json;
    if method != "POST" {
        return write_json(stream, 405, &json!({"ok": false, "error": "method not allowed"}));
    }
    let Some(ctl) = ctx.ctl.as_ref() else {
        return write_json(stream, 404, &json!({"ok": false, "error": "not found"}));
    };
    // DNS-rebinding guard: the deck's pages always carry an IP-literal or
    // localhost Host; a rebound hostname is not us.
    if !host_header_ok(raw) {
        return write_json(stream, 403, &json!({"ok": false, "error": "bad host"}));
    }
    // Password gate: exactly as strong as the run's own VNC auth.
    if let Some(expect) = &ctl.pass {
        let ok = header_value(raw, "x-berry-pass")
            .map(|g| ct_eq(g.as_bytes(), expect.as_bytes()))
            .unwrap_or(false);
        if !ok {
            return write_json(stream, 403, &json!({"ok": false, "error": "unauthorized"}));
        }
    }
    let action = match action {
        "reboot" | "off" => action,
        "" => {
            return write_json(stream, 400, &json!({"ok": false, "error": "missing action (reboot|off)"}))
        }
        _ => {
            return write_json(stream, 400, &json!({"ok": false, "error": "bad action (reboot|off)"}))
        }
    };
    match crate::control::request(&ctl.run, "power", json!({"action": action, "force": false})) {
        Ok(v) => write_json(stream, 200, &json!({"ok": true, "result": v})),
        Err(e) => write_json(stream, 502, &json!({"ok": false, "error": e.to_string()})),
    }
}

/// First value of a (case-insensitively named) header from the raw head
/// bytes, or None.
fn header_value(raw: &[u8], name: &str) -> Option<String> {
    for line in head_lines(raw).skip(1) {
        if let Some((n, v)) = split_header(line) {
            if n.eq_ignore_ascii_case(name.as_bytes()) {
                return Some(String::from_utf8_lossy(v).trim().to_string());
            }
        }
    }
    None
}

fn split_header(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let pos = line.iter().position(|b| *b == b':')?;
    Some((&line[..pos], &line[pos + 1..]))
}

fn head_lines(raw: &[u8]) -> impl Iterator<Item = &[u8]> {
    raw.split(|b| *b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
}

/// Host header must be absent, empty, `localhost`-shaped, or an IP literal
/// (port stripped, IPv6 brackets tolerated) — anything else is a rebind.
fn host_header_ok(raw: &[u8]) -> bool {
    let Some(host) = header_value(raw, "host") else { return true };
    let h = host.split(':').next().unwrap_or("").trim();
    if h.is_empty() {
        return true;
    }
    let h = h.trim_start_matches('[').trim_end_matches(']');
    h == "localhost" || h.ends_with(".localhost") || h.parse::<std::net::IpAddr>().is_ok()
}

/// Constant-time-ish equality (length check aside — acceptable for a LAN
/// dev tool; the digest avoids early-exit byte leaks).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

fn write_json<S: Write>(w: &mut S, code: u16, body: &serde_json::Value) -> std::io::Result<()> {
    let body = body.to_string();
    let resp = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        reason_phrase(code),
        body.len()
    );
    w.write_all(resp.as_bytes())?;
    w.flush()
}

fn reason_phrase(code: u16) -> &'static str {
    match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        502 => "Bad Gateway",
        _ => "Error",
    }
}

// ---------------------------------------------------------------------------
// LAN /ws byte-pipe proxy (wave 10, plaintext runs only)
// ---------------------------------------------------------------------------

/// Route activation: `Upgrade: websocket` (case-insensitive name, value
/// containing the websocket token) plus a `Sec-WebSocket-Key:` header.
fn head_has_ws_upgrade(raw: &[u8]) -> bool {
    let mut up = false;
    let mut key = false;
    for line in head_lines(raw).skip(1) {
        if let Some((n, v)) = split_header(line) {
            if n.eq_ignore_ascii_case(b"upgrade")
                && String::from_utf8_lossy(v).to_lowercase().contains("websocket")
            {
                up = true;
            }
            if n.eq_ignore_ascii_case(b"sec-websocket-key") {
                key = true;
            }
        }
    }
    up && key
}

/// Rewrite the request line's target to `/` (QEMU's ws endpoint only
/// accepts the root path, probe f3) leaving every other head byte
/// verbatim — Host, Sec-WebSocket-Key, version line, extension headers,
/// and the terminating blank line included. No re-serialization. None on
/// a malformed request line.
pub(crate) fn rewrite_ws_request_head(raw: &[u8]) -> Option<Vec<u8>> {
    let pos = raw.windows(2).position(|w| w == b"\r\n")?;
    let line = &raw[..pos];
    let sp1 = line.iter().position(|b| *b == b' ')?;
    let sp2 = sp1 + 1 + line[sp1 + 1..].iter().position(|b| *b == b' ')?;
    let method = &line[..sp1];
    let version = &line[sp2 + 1..];
    if method.is_empty() || sp2 + 1 >= line.len() || !version.starts_with(b"HTTP/") {
        return None;
    }
    let mut out = Vec::with_capacity(raw.len());
    out.extend_from_slice(method);
    out.push(b' ');
    out.extend_from_slice(b"/");
    out.push(b' ');
    out.extend_from_slice(version);
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&raw[pos + 2..]);
    Some(out)
}

/// The plaintext byte pipe: dial QEMU's loopback ws port (10s connect
/// budget, `vnc check` discipline; failures are audible — F30 precedent),
/// send the rewritten head, then pump both directions with OS semantics
/// until either side EOFs, and tear both down. Exempt from CONN_BUDGET
/// structurally: this function never returns to a deadline check.
fn proxy_ws_plain(browser: &mut TcpStream, raw: Vec<u8>, dial: &str) -> std::io::Result<()> {
    use std::net::ToSocketAddrs;
    let fail = |msg: std::string::String| {
        eprintln!("berry: vnc web: lan /ws proxy upstream {dial} failed: {msg}");
    };
    let addr = match dial.to_socket_addrs() {
        Ok(mut it) => it.next(),
        Err(_) => None,
    };
    let Some(addr) = addr else {
        fail("cannot resolve upstream".to_string());
        return Ok(());
    };
    let mut up = match TcpStream::connect_timeout(&addr, Duration::from_secs(10)) {
        Ok(s) => s,
        Err(e) => {
            fail(e.to_string());
            return Ok(());
        }
    };
    let Some(head) = rewrite_ws_request_head(&raw) else {
        return Ok(()); // malformed request line: drop the connection
    };
    if let Err(e) = up.write_all(&head).and_then(|_| up.flush()) {
        fail(e.to_string());
        return Ok(());
    }
    // Data plane: OS semantics — a desktop session may stay up for hours.
    browser.set_read_timeout(None).ok();
    browser.set_write_timeout(None).ok();
    up.set_read_timeout(None).ok();
    up.set_write_timeout(None).ok();
    let mut b_rd = browser.try_clone()?;
    let mut u_wr = up.try_clone()?;
    let pump_in = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match b_rd.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if u_wr.write_all(&buf[..n]).is_err() || u_wr.flush().is_err() {
                        break;
                    }
                }
            }
        }
    });
    let mut buf = [0u8; 8192];
    loop {
        match up.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if browser.write_all(&buf[..n]).is_err() || browser.flush().is_err() {
                    break;
                }
            }
        }
    }
    let _ = pump_in.join();
    let _ = browser.shutdown(Shutdown::Both);
    let _ = up.shutdown(Shutdown::Both);
    Ok(())
}

/// Why a head read failed: an HTTP status to write back, or the F18
/// budget expiring (silent close).
#[derive(Debug)]
enum HeadFail {
    Code(u16),
    Budget,
}

/// Read the request head (request line + headers) within the size budget
/// and the whole-connection deadline. Returns (method, target, raw head
/// bytes) on success — the raw block feeds the `/ws` route detection and
/// the proxy's request-line rewrite (wave 10); a protocol problem maps to
/// an HTTP status, a budget overrun to the silent-drop marker.
///
/// A read timeout on the underlying socket (armed by the callers — the
/// plain path with 5s, the TLS path inherits the 10s handshake budget)
/// wakes the blocked `read_until` so the deadline is re-checked instead
/// of blocking past the budget; partial bytes consumed before the
/// timeout stay in `buf` (std keeps them on error), so the retry resumes
/// mid-line.
fn read_head(
    reader: &mut impl BufRead,
    deadline: Instant,
) -> std::result::Result<(String, String, Vec<u8>), HeadFail> {
    let mut buf = Vec::new();
    let mut line_start = 0usize;
    loop {
        if Instant::now() >= deadline {
            return Err(HeadFail::Budget);
        }
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => return Err(HeadFail::Code(400)), // EOF mid-head
            Ok(_) => {
                let line = &buf[line_start..];
                line_start = buf.len();
                // End of head: the empty line after the headers.
                if line == b"\r\n" || line == b"\n" {
                    break;
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                continue; // partial bytes stay in buf; re-check the deadline
            }
            Err(_) => return Err(HeadFail::Code(400)),
        }
        if buf.len() > MAX_HEAD {
            return Err(HeadFail::Code(400));
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let req_line = text.lines().next().unwrap_or("").to_string();
    let (method, target) = parse_request_line(&req_line).map_err(HeadFail::Code)?;
    Ok((method, target, buf))
}

/// `GET /path HTTP/1.1` -> (GET, /path). Anything else is a 400.
pub(crate) fn parse_request_line(line: &str) -> std::result::Result<(String, String), u16> {
    let mut it = line.split_whitespace();
    let (Some(method), Some(target), Some(version)) = (it.next(), it.next(), it.next()) else {
        return Err(400);
    };
    if it.next().is_some() {
        return Err(400);
    }
    if !version.starts_with("HTTP/") {
        return Err(400);
    }
    // origin-form only: absolute targets are proxies, not us.
    if !target.starts_with('/') {
        return Err(400);
    }
    Ok((method.to_string(), target.to_string()))
}

/// Percent-decode a URI path (query string stripped first). Invalid escapes
/// and NUL bytes are rejected (None -> 400).
pub(crate) fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return None;
                }
                let h = (bytes[i + 1] as char).to_digit(16)?;
                let l = (bytes[i + 2] as char).to_digit(16)?;
                let b = (h * 16 + l) as u8;
                if b == 0 {
                    return None;
                }
                out.push(b);
                i += 3;
            }
            b if b < 0x20 || b == 0x7f => return None,
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// URL path -> relative file path. `/` (and an empty collapse) maps to the
/// own default page `index.html`; a `novnc/` prefix keeps the vendored
/// tree (escape hatch). Rejects `..` segments (checked after
/// percent-decoding so `%2e%2e` cannot sneak through), backslashes, and
/// drive-letter colons.
pub(crate) fn sanitize_path(target: &str) -> Option<String> {
    let path = target.split('?').next().unwrap_or("");
    let decoded = percent_decode(path)?;
    let trimmed = decoded.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some("index.html".to_string());
    }
    let mut parts = Vec::new();
    for seg in trimmed.split('/') {
        match seg {
            "" | "." => continue,
            ".." => return None,
            s if s.contains('\\') || s.contains(':') || s.contains('\0') => return None,
            s => parts.push(s),
        }
    }
    if parts.is_empty() {
        return Some("index.html".to_string());
    }
    Some(parts.join("/"))
}

/// Content-Type by extension; unknown extensions fall back to a download.
pub fn mime_for(path: &str) -> &'static str {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("oga" | "ogg") => "audio/ogg",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("txt") | Some("md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn write_simple<S: Write>(w: &mut S, code: u16, head_only: bool, body: &[u8]) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let resp = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    w.write_all(resp.as_bytes())?;
    if !head_only {
        w.write_all(body)?;
    }
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_url_construction() {
        // No password: no fragment. Wave 9: only the page path segment
        // changed (/vnc.html -> /); query params and ordering are frozen.
        let url = build_page_url(8123, "ws://127.0.0.1:6001", None);
        assert_eq!(
            url,
            "http://127.0.0.1:8123/?autoconnect=true&path=ws%3A%2F%2F127.0.0.1%3A6001&resize=scale"
        );
        // Byte-identity with the pre-wave-9 form except the path segment.
        assert_eq!(
            url.replace("/?autoconnect", "/vnc.html?autoconnect"),
            "http://127.0.0.1:8123/vnc.html?autoconnect=true&path=ws%3A%2F%2F127.0.0.1%3A6001&resize=scale"
        );
        // With a password: fragment carries it (never sent to a server).
        let url = build_page_url(8123, "wss://[::1]:6001", Some("ab12CDef"));
        assert!(url.starts_with("http://127.0.0.1:8123/?autoconnect=true&path="));
        assert!(url.contains("path=wss%3A%2F%2F%5B%3A%3A1%5D%3A6001"));
        assert!(url.ends_with("#password=ab12CDef"));
        // Odd characters in a user password get percent-encoded.
        let url = build_page_url(8123, "ws://127.0.0.1:6001", Some("a b&c#d"));
        assert!(url.ends_with("#password=a%20b%26c%23d"));
    }

    #[test]
    fn path_sanitization() {
        // Wave 9 dual tree: `/` is the own default page; own assets
        // resolve at the root; the novnc/ prefix keeps the vendored tree.
        assert_eq!(sanitize_path("/"), Some("index.html".into()));
        assert_eq!(sanitize_path("/index.html"), Some("index.html".into()));
        assert_eq!(sanitize_path("/berry.js"), Some("berry.js".into()));
        assert_eq!(sanitize_path("/berry.css"), Some("berry.css".into()));
        assert_eq!(sanitize_path("/NOTICE.md"), Some("NOTICE.md".into()));
        assert_eq!(sanitize_path("/novnc/vnc.html"), Some("novnc/vnc.html".into()));
        assert_eq!(sanitize_path("/novnc/core/rfb.js"), Some("novnc/core/rfb.js".into()));
        // Bare legacy /vnc.html is the own-tree literal now — no file
        // there means a plain 404 (fs-layer semantics, no redirect).
        assert_eq!(sanitize_path("/vnc.html"), Some("vnc.html".into()));
        assert_eq!(
            sanitize_path("/novnc/app/styles/base.css?x=1"),
            Some("novnc/app/styles/base.css".into())
        );
        assert_eq!(sanitize_path("//double//slash"), Some("double/slash".into()));
        // Dot-dot in plain and encoded form is rejected after decoding —
        // including through the novnc prefix.
        assert_eq!(sanitize_path("/../etc/passwd"), None);
        assert_eq!(sanitize_path("/core/../../etc/passwd"), None);
        assert_eq!(sanitize_path("/novnc/../../etc/passwd"), None);
        assert_eq!(sanitize_path("/novnc/%2e%2e/x"), None);
        assert_eq!(sanitize_path("/%2e%2e/etc/passwd"), None);
        assert_eq!(sanitize_path("/%2E%2E/etc/passwd"), None);
        assert_eq!(sanitize_path("/a/%2e%2e/b"), None);
        // Backslash tricks and drive letters are rejected.
        assert_eq!(sanitize_path("/a\\b"), None);
        assert_eq!(sanitize_path("/C:/x"), None);
        // Bad escapes are rejected.
        assert_eq!(sanitize_path("/%zz"), None);
        assert_eq!(sanitize_path("/%2"), None);
        assert_eq!(sanitize_path("/%00"), None);
        // "." segments collapse; a path of only dots lands on the
        // default page.
        assert_eq!(sanitize_path("/./x"), Some("x".into()));
        assert_eq!(sanitize_path("/././"), Some("index.html".into()));
    }

    #[test]
    fn request_line_parsing() {
        assert_eq!(
            parse_request_line("GET /vnc.html HTTP/1.1").unwrap(),
            ("GET".into(), "/vnc.html".into())
        );
        assert_eq!(
            parse_request_line("HEAD / HTTP/1.0").unwrap(),
            ("HEAD".into(), "/".into())
        );
        // Garbage, missing parts, extra parts, non-HTTP version, absolute
        // target: all 400.
        assert_eq!(parse_request_line("").unwrap_err(), 400);
        assert_eq!(parse_request_line("GET").unwrap_err(), 400);
        assert_eq!(parse_request_line("GET /").unwrap_err(), 400);
        assert_eq!(parse_request_line("GET / HTTP/1.1 junk").unwrap_err(), 400);
        assert_eq!(parse_request_line("GET / FTP/1.1").unwrap_err(), 400);
        assert_eq!(parse_request_line("GET http://x/y HTTP/1.1").unwrap_err(), 400);
    }

    #[test]
    fn mime_table() {
        assert_eq!(mime_for("a.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("a.HTML"), "text/html; charset=utf-8");
        assert_eq!(mime_for("a.css"), "text/css; charset=utf-8");
        assert_eq!(mime_for("a.json"), "application/json");
        assert_eq!(mime_for("a.png"), "image/png");
        assert_eq!(mime_for("a.svg"), "image/svg+xml");
        assert_eq!(mime_for("a.ico"), "image/x-icon");
        assert_eq!(mime_for("LICENSE"), "application/octet-stream");
        assert_eq!(mime_for("Makefile"), "application/octet-stream");
        assert_eq!(mime_for("noext"), "application/octet-stream");
    }

    #[test]
    fn max_conns_parsing() {
        // Boundaries of the accepted range.
        assert_eq!(parse_max_conns("1"), Ok(1));
        assert_eq!(parse_max_conns("16"), Ok(DEFAULT_MAX_CONNS));
        assert_eq!(
            parse_max_conns("1024"),
            Ok(MAX_CONNS_CEILING)
        );
        // Out-of-range and non-numeric values are rejected with the range
        // spelled out (clap turns this into rc=2).
        for raw in ["0", "1025", "99999", "-1", "abc", "", "1.5", " 8"] {
            let err = parse_max_conns(raw).unwrap_err();
            assert!(err.contains("1"), "raw={raw:?} err={err}");
            assert!(err.contains("1024"), "raw={raw:?} err={err}");
        }
    }

    #[test]
    fn admission_boundaries() {
        // The cap is inclusive-exclusive: at most max_conns live at once.
        assert!(admit_connection(0, 1));
        assert!(!admit_connection(1, 1));
        assert!(admit_connection(15, 16));
        assert!(!admit_connection(16, 16));
        assert!(!admit_connection(17, 16));
        assert!(admit_connection(1023, 1024));
    }

    #[test]
    fn overload_response_rendering() {
        let resp = overload_response(16);
        assert!(resp.starts_with("HTTP/1.1 503 Service Unavailable\r\n"));
        assert!(resp.contains("Connection: close\r\n"));
        assert!(resp.contains("Cache-Control: no-store\r\n"));
        assert!(resp.ends_with("connection limit reached (16); close a tab and retry\n"));
        // Content-Length must match the body bytes exactly.
        let (head, body) = resp.split_once("\r\n\r\n").unwrap();
        let len: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(len, body.len());
        // The cap value rides in the message (operator-facing).
        assert!(overload_response(7).contains("(7)"));
    }

    #[test]
    fn head_parsing_over_limit() {
        let budget = Instant::now() + CONN_BUDGET;
        // A header block over 8KB must yield 400 (via read_head).
        let mut req = String::from("GET / HTTP/1.1\r\nX-Big: ");
        req.push_str(&"a".repeat(MAX_HEAD + 16));
        req.push_str("\r\n\r\n");
        let mut cursor = std::io::Cursor::new(req.into_bytes());
        assert!(matches!(
            read_head(&mut cursor, budget).unwrap_err(),
            HeadFail::Code(400)
        ));
        // Well-formed head parses.
        let mut cursor = std::io::Cursor::new(b"GET /vnc.html HTTP/1.1\r\nHost: x\r\n\r\n".to_vec());
        let (m, t, raw) = read_head(&mut cursor, budget).unwrap();
        assert_eq!((m.as_str(), t.as_str()), ("GET", "/vnc.html"));
        assert_eq!(raw, b"GET /vnc.html HTTP/1.1\r\nHost: x\r\n\r\n".to_vec());
        // Header exactly at the budget still parses (limit is > MAX_HEAD).
        let mut req = String::from("GET / HTTP/1.1\r\nX-Big: ");
        req.push_str(&"a".repeat(MAX_HEAD - 64));
        req.push_str("\r\n\r\n");
        let mut cursor = std::io::Cursor::new(req.into_bytes());
        assert!(read_head(&mut cursor, budget).is_ok());
        // Deadline already in the past: the budget marker, not a status.
        let mut cursor = std::io::Cursor::new(b"GET / HTTP/1.1\r\n\r\n".to_vec());
        assert!(matches!(
            read_head(&mut cursor, Instant::now()).unwrap_err(),
            HeadFail::Budget
        ));
    }

    #[test]
    fn web_root_found_from_repo() {
        // The repo checkout contains both sentinels; running tests from
        // tools/berry (cargo test) must find it via the cwd upward walk.
        let root = find_web_root();
        // Not asserting Ok: a foreign build env may legitimately lack the
        // assets; when found, it must actually hold both files.
        if let Ok(r) = root {
            assert!(r.join("novnc").join("vnc.html").is_file());
            assert!(r.join("index.html").is_file());
        }
    }

    /// U44: the wave-9 dual sentinel — a root with only the vendored page
    /// is not a web root (the default entry would be dead); adding the
    /// own index.html completes it.
    #[test]
    fn find_web_root_dual_sentinel() {
        let tmp = std::env::temp_dir().join(format!("berry-web-sentinel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("novnc")).unwrap();
        std::fs::write(tmp.join("novnc").join("vnc.html"), b"<html>noVNC</html>").unwrap();
        assert!(
            find_web_root_in(&[tmp.clone()]).is_none(),
            "novnc-only root must fail the dual sentinel"
        );
        std::fs::write(tmp.join("index.html"), b"<html>BerryOS</html>").unwrap();
        assert_eq!(find_web_root_in(&[tmp.clone()]), Some(tmp.clone()));
        // The not-found error names both sentinels in its hint.
        let err = web_assets_missing();
        let hint = err.hint.expect("hint must be set");
        assert!(hint.contains("index.html"), "hint: {hint}");
        assert!(hint.contains("novnc/vnc.html"), "hint: {hint}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// U45: `--open` strips the password fragment from the one-shot URL.
    #[test]
    fn strip_fragment_cases() {
        assert_eq!(
            strip_fragment("http://127.0.0.1:1/?autoconnect=true&path=ws%3A%2F%2Fx#password=ab12"),
            "http://127.0.0.1:1/?autoconnect=true&path=ws%3A%2F%2Fx"
        );
        assert_eq!(
            strip_fragment("https://127.0.0.1:2/?autoconnect=true#password=a%20b"),
            "https://127.0.0.1:2/?autoconnect=true"
        );
        // No fragment: unchanged (same slice, no allocation needed).
        let url = "https://127.0.0.1:2/?autoconnect=true";
        assert_eq!(strip_fragment(url), url);
        // Empty string edge.
        assert_eq!(strip_fragment(""), "");
        // A bare '#' with nothing after it still strips to before it.
        assert_eq!(strip_fragment("http://x/#"), "http://x/");
    }

    /// U46: the single-port LAN URL — page and same-origin `/ws` route on
    /// one port; the percent-encoding and fragment rules mirror the other
    /// builders byte for byte.
    #[test]
    fn lan_page_url_construction() {
        assert_eq!(
            build_page_url_lan("192.168.1.105", 8123, None),
            "http://192.168.1.105:8123/?autoconnect=true&path=ws%3A%2F%2F192.168.1.105%3A8123%2Fws&resize=scale"
        );
        assert_eq!(
            build_page_url_lan("192.168.1.105", 8123, Some("ab12")),
            "http://192.168.1.105:8123/?autoconnect=true&path=ws%3A%2F%2F192.168.1.105%3A8123%2Fws&resize=scale#password=ab12"
        );
        // Existing builders regress-pinned (wave-10 contract 4).
        assert_eq!(
            build_page_url(8123, "ws://127.0.0.1:54005", None),
            "http://127.0.0.1:8123/?autoconnect=true&path=ws%3A%2F%2F127.0.0.1%3A54005&resize=scale"
        );
    }

    /// U50: `/ws` route detection and the request-line rewrite.
    #[test]
    fn ws_route_detection_and_head_rewrite() {
        let up = b"GET /ws?x=1 HTTP/1.1\r\nHost: 192.168.1.105:8123\r\nUpgrade: WebSocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
        assert!(head_has_ws_upgrade(up));
        // No upgrade headers: not the proxy route.
        assert!(!head_has_ws_upgrade(b"GET /ws HTTP/1.1\r\nHost: x\r\n\r\n"));
        // Key without Upgrade, and vice versa.
        assert!(!head_has_ws_upgrade(
            b"GET /ws HTTP/1.1\r\nHost: x\r\nSec-WebSocket-Key: k\r\n\r\n"
        ));
        assert!(!head_has_ws_upgrade(
            b"GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\n\r\n"
        ));
        // Rewrite: only the request-line target changes (to "/"), every
        // other head byte verbatim — CRLFs, odd headers, blank line.
        let raw = b"GET /ws HTTP/1.1\r\nHost: h\r\nX-Odd: va\x01lue\r\nUpgrade: websocket\r\nSec-WebSocket-Key: k\r\n\r\n";
        let out = rewrite_ws_request_head(raw).unwrap();
        assert_eq!(
            &out[..raw.len() - 2],
            b"GET / HTTP/1.1\r\nHost: h\r\nX-Odd: va\x01lue\r\nUpgrade: websocket\r\nSec-WebSocket-Key: k\r\n"
        );
        assert_eq!(&out[raw.len() - 2..], b"\r\n");
        // Query variants rewrite away with the target.
        let q = rewrite_ws_request_head(b"GET /ws?a=b HTTP/1.1\r\n\r\n").unwrap();
        assert!(q.starts_with(b"GET / HTTP/1.1\r\n\r\n"));
        // Malformed request lines yield None.
        assert!(rewrite_ws_request_head(b"GET\r\n").is_none());
        assert!(rewrite_ws_request_head(b"GET /x FTP/1.1\r\n\r\n").is_none());
        assert!(rewrite_ws_request_head(b"").is_none());
    }

    /// U51: the plaintext `/ws` proxy end-to-end against a synthetic
    /// upstream — the upstream only ever sees "/" as the target, the 101
    /// and everything after relays byte-exact both ways, and a browser-side
    /// drop tears both directions down.
    #[test]
    fn lan_ws_proxy_end_to_end() {
        use std::io::Read;
        // Upstream "QEMU ws": read the head, pin the rewritten request
        // line, answer a 101-shaped line, then echo forever.
        let up_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let up_port = up_l.local_addr().unwrap().port();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_t = Arc::clone(&seen);
        std::thread::spawn(move || {
            for mut sock in up_l.incoming().flatten() {
                let seen_t = Arc::clone(&seen_t);
                std::thread::spawn(move || {
                    let mut buf = [0u8; 8192];
                    let mut head = Vec::new();
                    // Head ends at the blank line.
                    loop {
                        match sock.read(&mut buf) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => {
                                head.extend_from_slice(&buf[..n]);
                                if head.windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }
                        }
                    }
                    let first = head
                        .split(|b| *b == b'\n')
                        .next()
                        .unwrap_or(b"")
                        .to_vec();
                    *seen_t.lock().unwrap() = first;
                    sock.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\r\nRFB 003.008\n")
                        .unwrap();
                    sock.flush().unwrap();
                    let mut echo = [0u8; 8192];
                    loop {
                        match sock.read(&mut echo) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if sock.write_all(&echo[..n]).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
            }
        });
        // The berry side: static-only root + LAN upstream.
        let tmp = std::env::temp_dir().join(format!("berry-web-lan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("novnc")).unwrap();
        std::fs::write(tmp.join("index.html"), b"<html>BerryOS</html>").unwrap();
        std::fs::write(tmp.join("novnc").join("vnc.html"), b"<html>noVNC</html>").unwrap();
        let ctx = Arc::new(ServeCtx {
            root: tmp.clone(),
            lan: Some(LanUpstream { dial: format!("127.0.0.1:{up_port}") }),
            ctl: None,
        });
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || serve_listener(listener, 4, None, ctx));

        // Browser side: upgrade through the proxy, read the 101 + banner,
        // echo round-trips, drop tears down.
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(
            b"GET /ws HTTP/1.1\r\nHost: 192.168.1.105\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
        )
        .unwrap();
        let mut buf = [0u8; 512];
        let n = s.read(&mut buf).unwrap();
        let got = &buf[..n];
        assert!(got.starts_with(b"HTTP/1.1 101"));
        assert!(got.ends_with(b"RFB 003.008\n"), "banner relayed: {got:?}");
        for payload in ["client-frame-1", "x"] {
            let p = payload.as_bytes();
            s.write_all(p).unwrap();
            let mut back = vec![0u8; p.len()];
            s.read_exact(&mut back).unwrap();
            assert_eq!(back, p);
        }
        // A non-upgrade GET /ws stays static (404: no ws file in the root).
        let mut s2 = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s2.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s2.write_all(b"GET /ws HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut buf = String::new();
        let _ = s2.read_to_string(&mut buf);
        assert!(buf.starts_with("HTTP/1.1 404"));

        drop(s);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            let first = seen.lock().unwrap().clone();
            if !first.is_empty() {
                assert_eq!(first, b"GET / HTTP/1.1\r\n".to_vec());
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let first = seen.lock().unwrap().clone();
        assert_eq!(first, b"GET / HTTP/1.1\r\n".to_vec(), "upstream saw the rewritten target");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Deck control API: password gate, host guard, method gate, action
    /// validation, and 502 when no control plane answers. (The 200 path
    /// needs a live session host — covered by the wave-10 e2e.)
    #[test]
    fn api_power_gate_and_dispatch() {
        let tmp = std::env::temp_dir().join(format!("berry-web-api-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let ctl = Arc::new(CtlApi { run: tmp.clone(), pass: Some("s3cret".to_string()) });
        let ctx = Arc::new(ServeCtx { root: tmp.clone(), lan: None, ctl: Some(ctl) });
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || serve_listener(listener, 4, None, ctx));

        let post = |target: &str, pass: Option<&str>, host: &str| -> String {
            use std::io::Read;
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut req = format!("POST {target} HTTP/1.1\r\nHost: {host}\r\n");
            if let Some(p) = pass {
                req.push_str(&format!("X-Berry-Pass: {p}\r\n"));
            }
            req.push_str("Content-Length: 0\r\n\r\n");
            s.write_all(req.as_bytes()).unwrap();
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        };
        // No password header / wrong password: 403, no control dial.
        assert!(post("/api/power/reboot", None, "127.0.0.1").starts_with("HTTP/1.1 403"));
        assert!(post("/api/power/reboot", Some("wrong"), "127.0.0.1").starts_with("HTTP/1.1 403"));
        // Rebound hostname: 403 before anything else.
        assert!(post("/api/power/reboot", Some("s3cret"), "evil.example").starts_with("HTTP/1.1 403"));
        // Wrong method and bad action shapes.
        let get = |target: &str| -> String {
            use std::io::Read;
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            s.write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
                .unwrap();
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        };
        assert!(get("/api/power/reboot").starts_with("HTTP/1.1 405"));
        assert!(post("/api/power/nonsense", Some("s3cret"), "127.0.0.1").starts_with("HTTP/1.1 400"));
        assert!(post("/api/power", Some("s3cret"), "127.0.0.1").starts_with("HTTP/1.1 400"));
        // Right password, no control socket in the run dir: 502 with the
        // JSON error body.
        let r = post("/api/power/reboot", Some("s3cret"), "127.0.0.1");
        assert!(r.starts_with("HTTP/1.1 502"), "{r}");
        assert!(r.contains("\"ok\":false"));
        // localhost-shaped hosts pass the guard.
        assert!(post("/api/power/off", Some("s3cret"), "localhost:8000").starts_with("HTTP/1.1 502"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Live request round-trip against an ephemeral server thread, over a
    /// synthetic dual-tree root (U43): own assets at the root, the
    /// vendored page under novnc/ — including the escape-hatch pin.
    #[test]
    fn http_round_trip() {
        let tmp = std::env::temp_dir().join(format!("berry-web-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("novnc").join("core")).unwrap();
        std::fs::write(tmp.join("index.html"), b"<html>BerryOS shell</html>").unwrap();
        std::fs::write(tmp.join("NOTICE.md"), b"notices: noVNC + own assets").unwrap();
        std::fs::write(tmp.join("novnc").join("vnc.html"), b"<html>noVNC</html>").unwrap();
        std::fs::write(tmp.join("novnc").join("core").join("rfb.js"), b"// rfb").unwrap();
        let root = tmp.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let ctx = std::sync::Arc::new(serve_ctx(&root));
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let _ = handle_conn(&mut stream, &ctx);
            }
        });
        let get = |path: &str, method: &str| -> (String, Vec<u8>) {
            use std::io::Read;
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            let mut parts = buf.splitn(2, "\r\n\r\n");
            let head = parts.next().unwrap_or("").to_string();
            let body = parts.next().unwrap_or("").as_bytes().to_vec();
            (head, body)
        };
        // Default page: own shell at /.
        let (head, body) = get("/", "GET");
        assert!(head.starts_with("HTTP/1.1 200 OK"));
        assert!(head.contains("Content-Type: text/html; charset=utf-8"));
        assert!(head.contains("Cache-Control: no-store"));
        assert!(String::from_utf8_lossy(&body).contains("BerryOS"));
        // Own assets serve from the root tree.
        let (head, _) = get("/NOTICE.md", "GET");
        assert!(head.starts_with("HTTP/1.1 200 OK"));
        assert!(head.contains("Content-Type: text/plain; charset=utf-8"));
        // Escape hatch: the vendored page stays reachable verbatim.
        let (head, body) = get("/novnc/vnc.html", "GET");
        assert!(head.starts_with("HTTP/1.1 200 OK"));
        assert!(String::from_utf8_lossy(&body).contains("noVNC"));
        // The RFB core module serves with a JS content type.
        let (head, _) = get("/novnc/core/rfb.js", "GET");
        assert!(head.contains("Content-Type: text/javascript; charset=utf-8"));
        // Bare legacy /vnc.html: own-tree literal, no file there -> 404.
        let (head, _) = get("/vnc.html", "GET");
        assert!(head.starts_with("HTTP/1.1 404"));
        // Method and traversal semantics are unchanged.
        let (head, body) = get("/", "HEAD");
        assert!(head.starts_with("HTTP/1.1 200 OK"));
        assert!(body.is_empty());
        let (head, _) = get("/nope.js", "GET");
        assert!(head.starts_with("HTTP/1.1 404"));
        let (head, _) = get("/../etc/passwd", "GET");
        assert!(head.starts_with("HTTP/1.1 400"));
        let (head, _) = get("/novnc/../../etc/passwd", "GET");
        assert!(head.starts_with("HTTP/1.1 400"));
        let (head, _) = get("/novnc/vnc.html", "POST");
        assert!(head.starts_with("HTTP/1.1 405"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Service-level admission through the real accept loop (`serve_listener`,
    /// the same path `serve` delegates to after binding): idle connections
    /// fill the cap, the next client gets the 503 line, and a released slot
    /// is recycled — the ConnSlot decrement regression net (a double-decrement
    /// or a leak would flip either assertion).
    #[test]
    fn serve_admission_gate_and_slot_recycling() {
        use std::time::Instant;
        let tmp = std::env::temp_dir().join(format!("berry-web-serve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("novnc")).unwrap();
        std::fs::write(tmp.join("novnc").join("vnc.html"), b"<html>noVNC</html>").unwrap();
        std::fs::write(tmp.join("index.html"), b"<html>BerryOS</html>").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let root = tmp.clone();
        std::thread::spawn(move || {
            serve_listener(listener, 2, None, std::sync::Arc::new(serve_ctx(&root)))
        });

        // A probe request: connect, send one GET, read the head back.
        let probe = |path: &str| -> String {
            use std::io::Read;
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        };
        // Wait until the server has accepted both idle connections (slots
        // register at accept time, not connect time) — bounded polling.
        let idle1 = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let idle2 = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut saw_503 = false;
        while Instant::now() < deadline {
            if probe("/").starts_with("HTTP/1.1 503") {
                saw_503 = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(saw_503, "third client must be refused with 503 while both slots are held");

        // Dropping an idle connection releases its slot (its read_head hits
        // EOF and the ConnSlot drops); the next request is admitted again.
        drop(idle1);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut admitted = false;
        while Instant::now() < deadline {
            let r = probe("/");
            if r.starts_with("HTTP/1.1 200") && r.contains("BerryOS") {
                admitted = true;
                break;
            }
            // Still full only transiently: retry.
            assert!(
                r.starts_with("HTTP/1.1 503") || r.starts_with("HTTP/1.1 200"),
                "unexpected response: {r:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(admitted, "released slot must be recycled for a fresh client");
        drop(idle2);
        let _ = std::fs::remove_dir_all(&tmp);
    }
    // ------------------------------------------------------------------
    // Wave 5: TLS page URL (U32), F18 budget (U30), https serving,
    // wss proxy end-to-end (U33)
    // ------------------------------------------------------------------

    #[test]
    fn https_page_url_construction() {
        // Wave-6 fix: `--tls` on a plaintext run must use the
        // host/port/encrypt=false form — noVNC 1.7.0 rewrites `path=`
        // schemes to wss on https pages unconditionally (probe f2).
        // `path=` PRESENT and empty, `resize=scale` like the http page.
        let url = build_page_url_https(8443, "127.0.0.1", 6001, None);
        assert_eq!(
            url,
            "https://127.0.0.1:8443/?autoconnect=true&host=127.0.0.1&port=6001&encrypt=false&path=&resize=scale"
        );
        // Password rides the fragment, percent-encoded like the others.
        let url = build_page_url_https(8443, "127.0.0.1", 6001, Some("ab12CDef"));
        assert!(url.ends_with("#password=ab12CDef"));
        let url = build_page_url_https(8443, "127.0.0.1", 6001, Some("a b&c#d"));
        assert!(url.ends_with("#password=a%20b%26c%23d"));
        // The host parameter is percent-encoded (bracketed IPv6).
        let url = build_page_url_https(8443, "[::1]", 6001, None);
        assert!(url.contains("host=%5B%3A%3A1%5D&port=6001&encrypt=false"));
        // No `path=ws://` form may come back (the f2 failure mode).
        assert!(!url.contains("path=ws"));
    }

    #[test]
    fn tls_page_url_construction() {
        // No password: exact query string, no fragment. `path=` must be
        // PRESENT and empty: noVNC 1.7.0 defaults an absent path to
        // "websockify", which QEMU's root-only ws endpoint 404s (probe p3).
        let url = build_page_url_tls(8443, "127.0.0.1", 9100, None);
        assert_eq!(
            url,
            "https://127.0.0.1:8443/?autoconnect=true&host=127.0.0.1&port=9100&encrypt=true&path=&resize=off"
        );
        // Password rides the fragment, percent-encoded like the plain URL.
        let url = build_page_url_tls(8443, "127.0.0.1", 9100, Some("ab12CDef"));
        assert!(url.ends_with("#password=ab12CDef"));
        let url = build_page_url_tls(8443, "127.0.0.1", 9100, Some("a b&c#d"));
        assert!(url.ends_with("#password=a%20b%26c%23d"));
        // The host parameter is percent-encoded too (bracketed IPv6 etc).
        let url = build_page_url_tls(8443, "[::1]", 9100, None);
        assert!(url.contains("host=%5B%3A%3A1%5D"));
        // The plaintext URL form is unchanged (no regression, U32 second
        // half pinned by page_url_construction above).
        assert_eq!(
            build_page_url(8123, "ws://127.0.0.1:6001", None),
            "http://127.0.0.1:8123/?autoconnect=true&path=ws%3A%2F%2F127.0.0.1%3A6001&resize=scale"
        );
    }

    /// F18 live: a 1s budget against a slow-drip client (0.2s per byte).
    /// The server must close the connection around the budget mark while
    /// the head is still dripping, and the released slot serves the next
    /// client with 200.
    #[test]
    fn connection_budget_drops_slow_drip_client() {
        let tmp = std::env::temp_dir().join(format!("berry-web-budget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("novnc")).unwrap();
        std::fs::write(tmp.join("novnc").join("vnc.html"), b"<html>noVNC</html>").unwrap();
        std::fs::write(tmp.join("index.html"), b"<html>BerryOS</html>").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let root = tmp.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let root = root.clone();
                std::thread::spawn(move || {
                    let mut s = stream;
                    // Short per-read timeout so the budget deadline is
                    // checked while the head is dripping (mirrors the
                    // 5s arming the real serve loop does).
                    let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
                    let _ = handle_conn_budget(&mut s, &root, Duration::from_secs(1));
                });
            }
        });

        // Slow-drip: send the request head one byte every 0.2s; the full
        // head needs ~30 bytes (6s) — far past the 1s budget.
        let mut drip = TcpStream::connect(("127.0.0.1", port)).unwrap();
        drip.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        let head = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n";
        let start = Instant::now();
        let mut got_eof = false;
        'send: for b in head.iter() {
            if drip.write_all(&[*b]).is_err() || drip.flush().is_err() {
                got_eof = true;
                break 'send;
            }
            // Poll for the server-side close while dripping.
            let mut probe = [0u8; 1];
            match drip.read(&mut probe) {
                Ok(0) => {
                    got_eof = true;
                    break 'send;
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => {
                    got_eof = true;
                    break 'send;
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let elapsed = start.elapsed();
        assert!(got_eof, "server must close the slow-drip connection");
        assert!(
            elapsed >= Duration::from_millis(700) && elapsed <= Duration::from_secs(3),
            "close must happen at the ~1s budget, took {elapsed:?}"
        );
        // The head never completed: no HTTP response bytes may leak.
        drop(drip);

        // The slot is free again: a well-behaved client gets 200.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut ok = false;
        while Instant::now() < deadline {
            let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            };
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            if s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").is_err() {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            let mut buf = String::new();
            use std::io::Read;
            let _ = s.read_to_string(&mut buf);
            if buf.starts_with("HTTP/1.1 200") && buf.contains("BerryOS") {
                ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(ok, "a fresh client must be served after the budget drop");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The https plane: serve_listener with a rustls config serves the
    /// same assets, reached through a rustls client session.
    #[test]
    fn https_serving_over_rustls() {
        let dir = crate::tls::test_support::certs_dir().to_path_buf();
        let cfg = match crate::tls::load_server_config(&dir) {
            Ok(c) => c,
            Err(_) => return, // no openssl in this environment
        };
        let tmp = std::env::temp_dir().join(format!("berry-web-tls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("novnc")).unwrap();
        std::fs::write(tmp.join("novnc").join("vnc.html"), b"<html>noVNC</html>").unwrap();
        std::fs::write(tmp.join("index.html"), b"<html>BerryOS</html>").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let root = tmp.clone();
        std::thread::spawn(move || {
            serve_listener(listener, 4, Some(cfg), std::sync::Arc::new(serve_ctx(&root)))
        });

        let client_cfg = crate::tls::load_client_config(&dir).unwrap();
        let name = crate::tls::server_name("127.0.0.1").unwrap();
        let conn = rustls::ClientConnection::new(client_cfg, name).unwrap();
        let sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut tls = rustls::StreamOwned::new(conn, sock);
        tls.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        tls.flush().unwrap();
        let mut buf = Vec::new();
        let _ = tls.read_to_end(&mut buf);
        let text = String::from_utf8_lossy(&buf);
        assert!(text.starts_with("HTTP/1.1 200 OK"), "{text}");
        assert!(text.contains("BerryOS"));
        // 404 still works inside TLS.
        let conn = rustls::ClientConnection::new(
            crate::tls::load_client_config(&dir).unwrap(),
            crate::tls::server_name("127.0.0.1").unwrap(),
        )
        .unwrap();
        let sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut tls = rustls::StreamOwned::new(conn, sock);
        tls.write_all(b"GET /nope HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        tls.flush().unwrap();
        let mut buf = Vec::new();
        let _ = tls.read_to_end(&mut buf);
        assert!(String::from_utf8_lossy(&buf).starts_with("HTTP/1.1 404"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The wss proxy end-to-end (U33): browser TLS -> proxy -> upstream
    /// TLS, pure byte relay in both directions through a loopback echo
    /// upstream; closing one side tears the whole chain down.
    #[test]
    fn wss_proxy_end_to_end_relay() {
        use std::io::Read;
        let dir = crate::tls::test_support::certs_dir().to_path_buf();
        let up_cfg = match crate::tls::load_server_config(&dir) {
            Ok(c) => c,
            Err(_) => return, // no openssl in this environment
        };
        // Upstream "QEMU ws": TLS echo of whatever plaintext arrives.
        let up_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let up_port = up_l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for sock in up_l.incoming().flatten() {
                let cfg = up_cfg.clone();
                std::thread::spawn(move || {
                    let Ok(mut s) = accept_tls(sock, &cfg) else { return };
                    s.sock.set_read_timeout(None).ok();
                    s.sock.set_write_timeout(None).ok();
                    let mut buf = [0u8; 8192];
                    loop {
                        match s.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if s.write_all(&buf[..n]).is_err() || s.flush().is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
            }
        });
        // The proxy itself.
        let px_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let px_port = px_l.local_addr().unwrap().port();
        let server_cfg = crate::tls::load_server_config(&dir).unwrap();
        let client_cfg = crate::tls::load_client_config(&dir).unwrap();
        let name = crate::tls::server_name("127.0.0.1").unwrap();
        let diag_dir = dir.clone();
        std::thread::spawn(move || {
            serve_wss_proxy(
                px_l,
                format!("127.0.0.1:{up_port}"),
                server_cfg,
                client_cfg,
                name,
                4,
                diag_dir,
            );
        });

        // Browser side: TLS client to the proxy, echo round-trips.
        let b_cfg = crate::tls::load_client_config(&dir).unwrap();
        let conn = rustls::ClientConnection::new(b_cfg, crate::tls::server_name("127.0.0.1").unwrap())
            .unwrap();
        let sock = TcpStream::connect(("127.0.0.1", px_port)).unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut tls = rustls::StreamOwned::new(conn, sock);
        for payload in ["hello wss", "second chunk with more bytes", "x"] {
            let payload = payload.as_bytes();
            tls.write_all(payload).unwrap();
            tls.flush().unwrap();
            let mut back = vec![0u8; payload.len()];
            tls.read_exact(&mut back).unwrap();
            assert_eq!(&back, payload, "echo through the proxy must be byte-exact");
        }
        // Dropping the browser side tears both proxy directions down
        // (EOF shutdown propagation); the threads wind up on their own.
        drop(tls);
        std::thread::sleep(Duration::from_millis(200));
    }
}
