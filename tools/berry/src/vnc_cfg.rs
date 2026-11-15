//! External VNC exposure: spec parsing, password policy, endpoint building.
//!
//! QEMU 11.0.3 facts this module encodes (probe f1–f5, 2026-11-13):
//! - `-vnc` with a single-colon number is a *display number*, not a literal
//!   port (real port = number + 5900), `tcp:` prefixes and `host::port` are
//!   rejected — so a literal port P must be written as `host:{P - 5900}` and
//!   P must be >= 5900 to be expressible at all.
//! - `password=on` is per-endpoint, and QMP `set_password` only ever targets
//!   the *first* declared `-vnc` (id=default) — the external endpoint must be
//!   declared before the internal unix socket.
//! - `websocket=on` derives its port from the display number (5700+display),
//!   which collides easily; we always pass an explicit `websocket=<port>`.
//!
//! Password policy (design doc §13): loopback defaults to no authentication,
//! non-loopback always requires a password (auto-generated when not given —
//! there is deliberately no "non-loopback without password" escape hatch).

use std::path::{Path, PathBuf};

use crate::error::{BerryError, Result};

/// Lowest literal TCP port expressible as a QEMU display number.
pub const MIN_VNC_PORT: u16 = 5900;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortSpec {
    Auto,
    Fixed(u16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VncSpec {
    Off,
    Addr { host: String, port: PortSpec },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WsSpec {
    Off,
    On,
    Port(u16),
}

fn bad_spec(s: &str) -> BerryError {
    BerryError::new(
        2,
        format!("bad --vnc spec '{s}'"),
    )
    .with_hint(
        "off | on | <port> | <host>:<port>; QEMU -vnc uses display numbers (real port = \
         number + 5900), so literal ports must be 5900..65535, e.g. --vnc 5971 or \
         --vnc 127.0.0.1:5971 (0/auto picks a free port)",
    )
}

/// Host part of an addr spec. IPv6 may be bracketed (`[::1]`) or bare
/// (`::`); brackets are kept as given since both QEMU and URIs accept them.
fn normalize_host(h: &str) -> Result<String> {
    if h.is_empty() {
        return Err(bad_spec(h));
    }
    if h.starts_with('[') {
        // "[::1]" -> kept verbatim; require at least "[:]" inside.
        if !h.ends_with(']') || h.len() < 4 {
            return Err(bad_spec(h));
        }
        return Ok(h.to_string());
    }
    if h.contains(':') {
        // Bare IPv6 (e.g. "::", "::1"): hex digits and colons only, and at
        // least two colons so "tcp:127.0.0.1" style mistakes are rejected.
        let ok = h.chars().all(|c| c.is_ascii_hexdigit() || c == ':')
            && h.matches(':').count() >= 2;
        if !ok {
            return Err(bad_spec(h));
        }
        return Ok(h.to_string());
    }
    Ok(h.to_string())
}

fn parse_display_port(s: &str) -> Result<PortSpec> {
    if s == "auto" {
        return Ok(PortSpec::Auto);
    }
    let n: u32 = s.parse().map_err(|_| bad_spec(s))?;
    if n == 0 {
        return Ok(PortSpec::Auto);
    }
    if n < MIN_VNC_PORT as u32 || n > 65535 {
        return Err(bad_spec(s));
    }
    Ok(PortSpec::Fixed(n as u16))
}

/// Parse `--vnc <spec>`: `off` | `on` | `<port>` | `<host>:<port>`.
/// Splitting is on the *last* colon so bracketed and bare IPv6 hosts work;
/// the wildcard shorthand `::5902` is normalized like `:::5902` (host `::`).
pub fn parse_spec(s: &str) -> Result<VncSpec> {
    let s = s.trim();
    match s {
        "off" => return Ok(VncSpec::Off),
        "on" => {
            return Ok(VncSpec::Addr {
                host: "127.0.0.1".to_string(),
                port: PortSpec::Auto,
            })
        }
        _ => {}
    }
    // "::<port>" (two leading colons + digits) is the wildcard host "::"
    // with a port; rsplit alone would split at the second colon.
    if let Some(port) = s.strip_prefix("::") {
        if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            let port = parse_display_port(port)?;
            return Ok(VncSpec::Addr {
                host: "::".to_string(),
                port,
            });
        }
    }
    let Some((host, port)) = s.rsplit_once(':') else {
        // Bare number (default host 127.0.0.1); anything else is an error.
        let port = parse_display_port(s)?;
        return Ok(VncSpec::Addr {
            host: "127.0.0.1".to_string(),
            port,
        });
    };
    let host = normalize_host(host)?;
    let port = parse_display_port(port)?;
    Ok(VncSpec::Addr { host, port })
}

/// Parse `--vnc-ws <on|off|port>` (port 1..65535, any value — the websocket
/// listener takes a literal port, not a display number).
pub fn parse_ws(s: &str) -> Result<WsSpec> {
    let s = s.trim();
    match s {
        "off" => Ok(WsSpec::Off),
        "on" => Ok(WsSpec::On),
        _ => {
            let n: u32 = s.parse().map_err(|_| bad_ws(s))?;
            if n == 0 || n > 65535 {
                return Err(bad_ws(s));
            }
            Ok(WsSpec::Port(n as u16))
        }
    }
}

fn bad_ws(s: &str) -> BerryError {
    BerryError::new(2, format!("bad --vnc-ws value '{s}'"))
        .with_hint("on | off | <port 1..65535>; requires --vnc (spec != off)")
}

/// TLS certificate directory must hold QEMU's x509 server-side trio (probe p3
/// layout). Checked both CLI-side (`validate_flags`) and host-side
/// (`resolve_from`) so a bad dir never reaches a background host whose stderr
/// is discarded.
pub fn validate_tls_dir(dir: &str) -> Result<std::path::PathBuf> {
    let p = std::path::PathBuf::from(dir);
    for name in ["ca-cert.pem", "server-cert.pem", "server-key.pem"] {
        if !p.join(name).is_file() {
            return Err(BerryError::new(
                2,
                format!("--vnc-tls dir {} is missing {}", p.display(), name),
            )
            .with_hint("generate a self-signed set with `berry vnc certs <dir>`"));
        }
    }
    Ok(p)
}

/// Literal port -> QEMU display number (5971 -> 71, 5900 -> 0). Ports below
/// 5900 cannot be expressed on a `-vnc` TCP endpoint at all.
pub fn port_to_display(port: u16) -> Result<u16> {
    if port < MIN_VNC_PORT {
        Err(BerryError::new(
            2,
            format!("VNC port {port} cannot be expressed as a QEMU display number"),
        )
        .with_hint("the -vnc TCP syntax addresses ports as display numbers (port = number + 5900); use a port >= 5900"))
    } else {
        Ok(port - MIN_VNC_PORT)
    }
}

/// Expire-time argument for `expire_password` / `berry vnc passwd --expire`:
/// `never` or a positive whole number of seconds. QMP-side calibration
/// (live test, QEMU 11.0.3): a bare "N" is *absolute* epoch seconds (expires
/// immediately), so berry always sends the relative "+N" form — the CLI
/// surface stays `<seconds|never>` per the design doc.
pub fn validate_expire(s: &str) -> Result<()> {
    let ok = s == "never" || s.parse::<u64>().is_ok_and(|n| n > 0);
    if ok {
        Ok(())
    } else {
        Err(BerryError::new(2, format!("bad --expire value '{s}'"))
            .with_hint("never | <positive seconds>, e.g. --expire 60"))
    }
}

/// User-form expire value -> QMP `expire_password` time string.
pub fn expire_qmp_time(user: &str) -> String {
    if user == "never" {
        "never".to_string()
    } else {
        format!("+{user}")
    }
}

/// Loopback set: 127.0.0.1, localhost, ::1 (bracketed or bare). Wildcards
/// (0.0.0.0, ::, [::]) and any other address are non-loopback.
pub fn is_loopback(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    matches!(h, "127.0.0.1" | "localhost" | "::1")
}

pub enum AuthDecision {
    None,
    Password(String),
}

/// Password policy matrix: explicit `--vnc-pass` wins everywhere; loopback
/// without one stays open; non-loopback without one gets a generated one.
pub fn resolve_auth(spec: &VncSpec, explicit_pass: Option<&str>) -> AuthDecision {
    match spec {
        VncSpec::Off => AuthDecision::None,
        VncSpec::Addr { host, .. } => {
            if let Some(p) = explicit_pass {
                AuthDecision::Password(p.to_string())
            } else if is_loopback(host) {
                AuthDecision::None
            } else {
                AuthDecision::Password(generate_password())
            }
        }
    }
}

/// 8 chars from [A-Za-z0-9]. DES keys are only 8 bytes, so this already
/// saturates the effective key material.
pub fn generate_password() -> String {
    const CS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = [0u8; 8];
    random_fill(&mut bytes);
    bytes
        .iter()
        .map(|b| CS[*b as usize % CS.len()] as char)
        .collect()
}

fn random_fill(buf: &mut [u8]) {
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            if f.read_exact(buf).is_ok() {
                return;
            }
        }
    }
    // Fallback (non-Unix or /dev/urandom unavailable): xorshift over a seed
    // mixed from the clock, the pid, and a process-wide counter.
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mut seed = nanos
        ^ ((std::process::id() as u64) << 32)
        ^ CTR.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);
    for b in buf.iter_mut() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        *b = seed as u8;
    }
}

/// A fully resolved external VNC endpoint: concrete ports, auth decision.
/// `port_auto`/`ws_auto` record which sides were auto-picked (rebuild on a
/// bind race only re-rolls those); `tls_dir` is the x509 cert dir for
/// VeNCrypt (probe p3: `-object tls-creds-x509,id=vnc-tls,dir=…`).
#[derive(Clone, Debug)]
pub struct Exposed {
    pub host: String,
    pub port: u16,
    pub ws_port: Option<u16>,
    pub password: Option<String>,
    pub tls_dir: Option<std::path::PathBuf>,
    pub port_auto: bool,
    pub ws_auto: bool,
}

impl Exposed {
    pub fn build(
        spec: &VncSpec,
        ws: &WsSpec,
        explicit_pass: Option<&str>,
        tls_dir: Option<&str>,
    ) -> Result<Self> {
        let VncSpec::Addr { host, port } = spec else {
            return Err(BerryError::internal("Exposed::build on an Off spec"));
        };
        let port_auto = matches!(port, PortSpec::Auto);
        let port = match port {
            PortSpec::Auto => free_vnc_port(),
            PortSpec::Fixed(p) => *p,
        };
        let ws_port = match ws {
            WsSpec::Off => None,
            WsSpec::On => Some(free_tcp_port()),
            WsSpec::Port(p) => Some(*p),
        };
        let password = match resolve_auth(spec, explicit_pass) {
            AuthDecision::None => None,
            AuthDecision::Password(p) => Some(p),
        };
        let tls_dir = match tls_dir {
            Some(d) => Some(validate_tls_dir(d)?),
            None => None,
        };
        Ok(Self {
            host: host.clone(),
            port,
            ws_port,
            password,
            tls_dir,
            port_auto,
            ws_auto: matches!(ws, WsSpec::On),
        })
    }

    /// Bind-race recovery (plan: 端口竞态重试): re-pick fresh free ports for
    /// the auto sides only. Fixed ports, host, password, and the TLS dir are
    /// deliberately untouched.
    pub fn rebuild_auto_ports(&mut self) {
        if self.port_auto {
            self.port = free_vnc_port();
        }
        if self.ws_auto {
            self.ws_port = self.ws_port.map(|_| free_tcp_port());
        }
    }

    /// The value for the external `-vnc` argv entry, e.g.
    /// `127.0.0.1:71,password=on,websocket=6001,tls-creds=vnc-tls`. The
    /// password itself is never part of argv — it is injected at runtime via
    /// QMP `set_password`.
    pub fn qemu_arg(&self) -> Result<String> {
        let display = port_to_display(self.port)?;
        let mut a = format!("{}:{}", self.host, display);
        if self.password.is_some() {
            a.push_str(",password=on");
        }
        if let Some(p) = self.ws_port {
            a.push_str(&format!(",websocket={p}"));
        }
        if self.tls_dir.is_some() {
            a.push_str(",tls-creds=vnc-tls");
        }
        Ok(a)
    }

    pub fn uri(&self) -> String {
        uri_for(&self.host, self.port)
    }
}

/// `host:port` in URI/socket form: bare IPv6 hosts get brackets.
pub fn addr_host(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

pub fn uri_for(host: &str, port: u16) -> String {
    format!("vnc://{}", addr_host(host, port))
}

/// ws/wss URI for the websocket listener. With TLS enabled the QEMU `websocket=`
/// port speaks wss only (probe p3: plain Upgrade gets RST), so the scheme
/// follows the endpoint's TLS state, not QMP's (mis)reported ws auth field.
pub fn ws_uri_for(host: &str, port: u16, tls: bool) -> String {
    format!("{}://{}", if tls { "wss" } else { "ws" }, addr_host(host, port))
}

/// Dialable host for a bind host: wildcard binds cannot be dialed as-is, so
/// map them to their loopback counterpart. Everything else (loopback, host
/// names, concrete addresses) passes through unchanged. `vnc status`/`open`/
/// `check` dial this while a separate `bind` field echoes the real bind.
pub fn dial_host(bind: &str) -> &str {
    match bind {
        "0.0.0.0" => "127.0.0.1",
        "::" | "[::]" => "[::1]",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Run-dir artifacts: vnc.listen / vnc.pass
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListenInfo {
    pub host: String,
    pub port: u16,
    pub ws: Option<u16>,
    /// VeNCrypt enabled on the endpoint (wave 2). Wave-1 files have no such
    /// field and read as false.
    pub tls: bool,
}

pub fn listen_value(host: &str, port: u16, ws: Option<u16>, tls: bool) -> serde_json::Value {
    serde_json::json!({"host": host, "port": port, "ws": ws, "tls": tls})
}

pub fn write_listen(run: &Path, host: &str, port: u16, ws: Option<u16>, tls: bool) -> Result<()> {
    std::fs::write(
        run.join("vnc.listen"),
        serde_json::to_vec(&listen_value(host, port, ws, tls))?,
    )?;
    Ok(())
}

pub fn read_listen(run: &Path) -> Option<ListenInfo> {
    let b = std::fs::read(run.join("vnc.listen")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&b).ok()?;
    Some(ListenInfo {
        host: v.get("host")?.as_str()?.to_string(),
        port: v.get("port")?.as_u64()? as u16,
        ws: v.get("ws").and_then(|w| w.as_u64()).map(|p| p as u16),
        tls: v.get("tls").and_then(|t| t.as_bool()).unwrap_or(false),
    })
}

/// Write `vnc.pass` with 0600 permissions (same policy as control.token).
pub fn write_pass(run: &Path, pw: &str) -> Result<()> {
    let p = run.join("vnc.pass");
    std::fs::write(&p, pw)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn read_pass(run: &Path) -> Option<String> {
    let s = std::fs::read_to_string(run.join("vnc.pass")).ok()?;
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Age at which a staged `vncpass-*` file counts as an orphan (F20): the
/// handoff window is a child-process spawn away (milliseconds), so one
/// hour covers any sane host under load while recycling files from
/// crashed/aborted launches. Fresh files (any live handoff) are never
/// touched — their mtime is the current moment.
pub const PASS_ORPHAN_AGE: std::time::Duration = std::time::Duration::from_secs(3600);

/// Pure staleness predicate: the name must be a staged pass file
/// (`vncpass-` prefix) and older than `PASS_ORPHAN_AGE` at `now`.
pub fn is_stale_vncpass(
    file_name: &str,
    mtime: std::time::SystemTime,
    now: std::time::SystemTime,
) -> bool {
    if !file_name.starts_with("vncpass-") {
        return false;
    }
    match now.duration_since(mtime) {
        Ok(age) => age > PASS_ORPHAN_AGE,
        Err(_) => false, // mtime in the future (clock skew): never stale
    }
}

/// Best-effort sweep of orphaned staged pass files in `dir` (F20). Every
/// failure mode — unreadable directory, unparsable metadata, undeletable
/// file — is ignored: this is hygiene around the handoff path, never a
/// gate in front of it.
pub fn sweep_orphan_passes(dir: &Path, now: std::time::SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        if let Ok(mtime) = meta.modified() {
            if is_stale_vncpass(name, mtime, now) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Stage a password in `BERRY_HOME/tmp/` (0600) for handoff to a child host
/// process via the hidden `--vnc-pass-file` flag, keeping it out of argv.
/// The child deletes the file after reading it (see `read_pass_file`);
/// when a child dies before reading, the staged file becomes an orphan —
/// swept here by age (F20) before a new one is written.
pub fn write_pass_temp(pw: &str) -> Result<PathBuf> {
    let dir = crate::run_dir::home().join("tmp");
    std::fs::create_dir_all(&dir)?;
    sweep_orphan_passes(&dir, std::time::SystemTime::now());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let p = dir.join(format!("vncpass-{}-{nanos}", std::process::id()));
    std::fs::write(&p, pw)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(p)
}

fn read_pass_file(p: &Path) -> Option<String> {
    let s = std::fs::read_to_string(p).ok()?;
    // One-shot: the file carried a secret, remove it as soon as it is read.
    let _ = std::fs::remove_file(p);
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

// ---------------------------------------------------------------------------
// Launch flags (direct parameter, wave 11)
// ---------------------------------------------------------------------------

/// VNC flags captured at the CLI entry points. Wave 1 routed these
/// through a process-level global because `window.rs` was frozen and
/// called `Device::start` with positional args only; wave 11 (the file
/// unfrozen) made `Launch` a direct `Device::start` parameter — shell
/// hands it to `window::run`, which keeps it on the `App` so the
/// in-place restarts (RES menu, ROTATE) re-pass the same flags
/// explicitly instead of inheriting a sticky global.
#[derive(Clone, Debug, Default)]
pub struct Launch {
    pub spec: Option<String>,
    pub ws: Option<String>,
    pub pass: Option<String>,
    pub pass_file: Option<PathBuf>,
    pub tls: Option<String>,
}

/// What `Device::start` resolved: the raw spec string (for meta.json) and the
/// concrete endpoint (None when not exposed).
pub struct Resolved {
    pub spec_str: String,
    pub ws_str: String,
    pub exposed: Option<Exposed>,
}

/// Resolution order: the passed-in launch flags (wave 11: a direct
/// `Device::start` parameter) > `BERRY_VNC` env (spec only) > off. The
/// env only fills the spec — ws/pass flags still come from the explicit
/// launch; `BERRY_VNC_TLS` fills the TLS dir the same way (usable even
/// when the spec itself came from a flag).
pub fn resolve(vnc: Launch) -> Result<Resolved> {
    let mut launch = Some(vnc);
    if launch.as_ref().and_then(|l| l.spec.as_deref()).is_none() {
        if let Ok(spec) = std::env::var("BERRY_VNC") {
            launch
                .get_or_insert_with(Launch::default)
                .spec = Some(spec);
        }
    }
    if launch.as_ref().and_then(|l| l.tls.as_deref()).is_none() {
        if let Ok(tls) = std::env::var("BERRY_VNC_TLS") {
            launch
                .get_or_insert_with(Launch::default)
                .tls = Some(tls);
        }
    }
    match launch {
        None => Ok(Resolved {
            spec_str: "off".to_string(),
            ws_str: "off".to_string(),
            exposed: None,
        }),
        Some(l) => resolve_from(l),
    }
}

/// Flag-or-env precedence (pure half of the effective-value merge): the CLI
/// flag wins, the env only fills a missing flag — exactly the order
/// `resolve` applies at launch time.
fn merge_flag_env<'a>(flag: Option<&'a str>, env: Option<&'a str>) -> Option<&'a str> {
    flag.or(env)
}

/// CLI-side pre-check so an invalid spec fails with its real E_USAGE error
/// before a host child is spawned (whose stderr is discarded in background
/// mode and would otherwise surface as "host exited before announcing run").
/// Inputs are the *effective* values: a missing flag falls back to
/// `BERRY_VNC`/`BERRY_VNC_TLS` like `resolve`, so an env typo fails
/// here too, and `--vnc-ws`/`--vnc-tls` are judged against an env-provided
/// endpoint instead of being wrongly rejected.
pub fn validate_flags(spec: Option<&str>, ws: Option<&str>, tls: Option<&str>) -> Result<()> {
    let env_spec = std::env::var("BERRY_VNC").ok();
    let env_tls = std::env::var("BERRY_VNC_TLS").ok();
    let spec = merge_flag_env(spec, env_spec.as_deref());
    let tls = merge_flag_env(tls, env_tls.as_deref());
    validate_effective(spec, ws, tls)
}

/// Pure core of `validate_flags` over already-merged effective values
/// (split out so the matrix stays unit-testable without mutating the
/// process env, which would race other tests).
fn validate_effective(spec: Option<&str>, ws: Option<&str>, tls: Option<&str>) -> Result<()> {
    let parsed = match spec {
        Some(s) => Some(parse_spec(s)?),
        None => None,
    };
    let vnc_off = parsed.as_ref().is_none_or(|p| matches!(p, VncSpec::Off));
    if let Some(w) = ws {
        let ws_spec = parse_ws(w)?;
        if vnc_off && !matches!(ws_spec, WsSpec::Off) {
            return Err(BerryError::new(
                2,
                "--vnc-ws requires an exposed endpoint",
            )
            .with_hint("pass --vnc on (or a spec) alongside --vnc-ws, or set BERRY_VNC"));
        }
    }
    if let Some(t) = tls {
        if vnc_off {
            return Err(BerryError::new(
                2,
                "--vnc-tls requires an exposed endpoint",
            )
            .with_hint("pass --vnc on (or a spec) alongside --vnc-tls, or set BERRY_VNC"));
        }
        validate_tls_dir(t)?;
    }
    Ok(())
}

fn resolve_from(l: Launch) -> Result<Resolved> {
    let spec_str = l.spec.clone().unwrap_or_else(|| "off".to_string());
    let ws_str = l.ws.clone().unwrap_or_else(|| "off".to_string());
    let spec = parse_spec(&spec_str)?;
    let ws = parse_ws(&ws_str)?;
    if matches!(spec, VncSpec::Off) {
        if !matches!(ws, WsSpec::Off) {
            return Err(BerryError::new(
                2,
                "--vnc-ws requires an exposed endpoint",
            )
            .with_hint("pass --vnc on (or a spec) alongside --vnc-ws"));
        }
        if l.tls.is_some() {
            return Err(BerryError::new(
                2,
                "--vnc-tls requires an exposed endpoint",
            )
            .with_hint("pass --vnc on (or a spec) alongside --vnc-tls"));
        }
        return Ok(Resolved { spec_str, ws_str, exposed: None });
    }
    // Password: explicit value > one-shot file. Neither reaches argv/logs.
    let pass = l
        .pass
        .as_deref()
        .map(str::to_string)
        .or_else(|| l.pass_file.as_ref().and_then(|p| read_pass_file(p)));
    let exposed = Exposed::build(&spec, &ws, pass.as_deref(), l.tls.as_deref())?;
    Ok(Resolved {
        spec_str,
        ws_str,
        exposed: Some(exposed),
    })
}

// ---------------------------------------------------------------------------
// Port picking (TOCTOU note): we pre-probe a free port and hand it to QEMU.
// If another process grabs it in between, QEMU fails to bind and exits —
// the boot failure path surfaces qemu.log, so the race is detected, not lost.
// ---------------------------------------------------------------------------

pub fn free_tcp_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or_else(|_| {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            MIN_VNC_PORT + (n % 500) as u16
        })
}

/// Free port that is expressible as a display number (>= 5900), used by both
/// the external endpoint and the non-Unix internal `-vnc` fallback.
pub fn free_vnc_port() -> u16 {
    loop {
        let p = free_tcp_port();
        if p >= MIN_VNC_PORT {
            return p;
        }
    }
}

/// One-time note when exposing on non-loopback: classic VNC auth (DES) is a
/// developer/LAN gate, not TLS-grade security.
pub fn print_security_note(ext: &Exposed) {
    if !is_loopback(&ext.host) {
        eprintln!(
            "berry: note: VNC exposed on {}: {} — classic VNC auth (DES) is weak; keep it on trusted networks",
            ext.host,
            ext.port
        );
    }
}

/// Entry-point variant for paths that do not have a running Device yet
/// (background `device start`, `shell`): warn from the raw spec alone.
/// The spec is the effective one (flag ‖ `BERRY_VNC` env): a background
/// host's own `print_security_note` goes to discarded stderr, so this
/// CLI-side print is the only warning an env-driven exposure ever gets.
pub fn maybe_security_note(spec: Option<&str>) {
    let env_spec = std::env::var("BERRY_VNC").ok();
    let spec = merge_flag_env(spec, env_spec.as_deref());
    let Some(s) = spec else { return };
    match parse_spec(s) {
        Ok(VncSpec::Addr { host, .. }) if !is_loopback(&host) => {
            eprintln!(
                "berry: note: VNC exposed on non-loopback {host} — classic VNC auth (DES) is weak; keep it on trusted networks"
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_of(s: &str) -> VncSpec {
        parse_spec(s).unwrap_or_else(|e| panic!("spec '{s}': {e}"))
    }

    fn ws_of(s: &str) -> WsSpec {
        parse_ws(s).unwrap_or_else(|e| panic!("ws '{s}': {e}"))
    }

    #[test]
    fn spec_valid_samples() {
        assert_eq!(spec_of("off"), VncSpec::Off);
        assert_eq!(
            spec_of("on"),
            VncSpec::Addr { host: "127.0.0.1".into(), port: PortSpec::Auto }
        );
        assert_eq!(
            spec_of("5971"),
            VncSpec::Addr { host: "127.0.0.1".into(), port: PortSpec::Fixed(5971) }
        );
        assert_eq!(
            spec_of("127.0.0.1:5971"),
            VncSpec::Addr { host: "127.0.0.1".into(), port: PortSpec::Fixed(5971) }
        );
        assert_eq!(
            spec_of("localhost:0"),
            VncSpec::Addr { host: "localhost".into(), port: PortSpec::Auto }
        );
        assert_eq!(
            spec_of("0.0.0.0:5902"),
            VncSpec::Addr { host: "0.0.0.0".into(), port: PortSpec::Fixed(5902) }
        );
        assert_eq!(
            spec_of("[::1]:6000"),
            VncSpec::Addr { host: "[::1]".into(), port: PortSpec::Fixed(6000) }
        );
        assert_eq!(
            spec_of("::5902"),
            VncSpec::Addr { host: "::".into(), port: PortSpec::Fixed(5902) }
        );
    }

    #[test]
    fn spec_invalid_samples() {
        for s in [
            "tcp:127.0.0.1:5971",
            "5899",
            "70000",
            "abc",
            ":5901",
            "::",
            "127.0.0.1",
            "[::1]",
            "",
        ] {
            let err = parse_spec(s).expect_err(s);
            assert_eq!(err.code, 2, "{s}");
            assert!(err.hint.is_some(), "{s} must carry a hint");
        }
    }

    #[test]
    fn port_display_mapping() {
        assert_eq!(port_to_display(5971).unwrap(), 71);
        assert_eq!(port_to_display(5900).unwrap(), 0);
        assert!(port_to_display(5899).is_err());
    }

    #[test]
    fn auth_policy_matrix() {
        let loop_spec = VncSpec::Addr { host: "127.0.0.1".into(), port: PortSpec::Fixed(5901) };
        let wide_spec = VncSpec::Addr { host: "0.0.0.0".into(), port: PortSpec::Fixed(5901) };
        // loopback, no explicit pass -> none
        assert!(matches!(resolve_auth(&loop_spec, None), AuthDecision::None));
        // loopback, explicit pass -> that pass
        match resolve_auth(&loop_spec, Some("pw")) {
            AuthDecision::Password(p) => assert_eq!(p, "pw"),
            _ => panic!("expected password"),
        }
        // non-loopback, no pass -> generated
        match resolve_auth(&wide_spec, None) {
            AuthDecision::Password(p) => {
                assert_eq!(p.len(), 8);
                assert!(p.chars().all(|c| c.is_ascii_alphanumeric()));
            }
            _ => panic!("expected generated password"),
        }
        // non-loopback, explicit pass -> that pass
        match resolve_auth(&wide_spec, Some("pw2")) {
            AuthDecision::Password(p) => assert_eq!(p, "pw2"),
            _ => panic!("expected password"),
        }
        // off -> none regardless
        assert!(matches!(resolve_auth(&VncSpec::Off, Some("pw")), AuthDecision::None));
    }

    #[test]
    fn uri_building() {
        assert_eq!(uri_for("127.0.0.1", 5971), "vnc://127.0.0.1:5971");
        assert_eq!(uri_for("[::1]", 5971), "vnc://[::1]:5971");
        assert_eq!(uri_for("::", 5971), "vnc://[::]:5971");
        assert_eq!(addr_host("localhost", 0), "localhost:0");
    }

    #[test]
    fn ws_scheme_follows_tls() {
        assert_eq!(ws_uri_for("127.0.0.1", 6001, false), "ws://127.0.0.1:6001");
        assert_eq!(ws_uri_for("127.0.0.1", 6001, true), "wss://127.0.0.1:6001");
        assert_eq!(ws_uri_for("::", 6001, true), "wss://[::]:6001");
    }

    #[test]
    fn dial_host_mapping() {
        assert_eq!(dial_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(dial_host("::"), "[::1]");
        assert_eq!(dial_host("[::]"), "[::1]");
        assert_eq!(dial_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(dial_host("localhost"), "localhost");
        assert_eq!(dial_host("192.168.1.4"), "192.168.1.4");
        assert_eq!(dial_host("[::1]"), "[::1]");
    }

    #[test]
    fn password_generator() {
        let a = generate_password();
        let b = generate_password();
        assert_eq!(a.len(), 8);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(a, b);
    }

    #[test]
    fn ws_parsing() {
        assert_eq!(ws_of("off"), WsSpec::Off);
        assert_eq!(ws_of("on"), WsSpec::On);
        assert_eq!(ws_of("6001"), WsSpec::Port(6001));
        assert!(parse_ws("0").is_err());
        assert!(parse_ws("70000").is_err());
        assert!(parse_ws("abc").is_err());
    }

    #[test]
    fn qemu_arg_building() {
        let ext = Exposed {
            host: "127.0.0.1".into(),
            port: 5971,
            ws_port: Some(6001),
            password: Some("x".into()),
            tls_dir: None,
            port_auto: false,
            ws_auto: false,
        };
        assert_eq!(ext.qemu_arg().unwrap(), "127.0.0.1:71,password=on,websocket=6001");
        let ext = Exposed {
            host: "127.0.0.1".into(),
            port: 5971,
            ws_port: None,
            password: None,
            tls_dir: None,
            port_auto: false,
            ws_auto: false,
        };
        assert_eq!(ext.qemu_arg().unwrap(), "127.0.0.1:71");
        let ext = Exposed {
            host: "[::1]".into(),
            port: 5900,
            ws_port: None,
            password: None,
            tls_dir: None,
            port_auto: false,
            ws_auto: false,
        };
        assert_eq!(ext.qemu_arg().unwrap(), "[::1]:0");
    }

    #[test]
    fn qemu_arg_with_tls_and_auto_flags() {
        let tmp = std::env::temp_dir().join(format!("berry-vnccfg-tls-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        for name in ["ca-cert.pem", "server-cert.pem", "server-key.pem"] {
            std::fs::write(tmp.join(name), b"x").unwrap();
        }
        // Full combination: password + ws + tls.
        let spec = spec_of("127.0.0.1:5971");
        let ext = Exposed::build(&spec, &ws_of("6001"), Some("pw"), Some(tmp.to_str().unwrap()))
            .unwrap();
        assert_eq!(
            ext.qemu_arg().unwrap(),
            "127.0.0.1:71,password=on,websocket=6001,tls-creds=vnc-tls"
        );
        // Auto flags: "on"/"on" is auto on both sides.
        let spec = spec_of("on");
        let ext = Exposed::build(&spec, &ws_of("on"), None, Some(tmp.to_str().unwrap()))
            .unwrap();
        assert!(ext.port_auto);
        assert!(ext.ws_auto);
        assert!(ext.port >= MIN_VNC_PORT);
        assert!(ext.ws_port.is_some());
        // rebuild_auto_ports only re-rolls the auto sides: fixed port + fixed
        // ws + password + tls dir stay identical.
        let spec = spec_of("127.0.0.1:5971");
        let mut ext = Exposed::build(&spec, &ws_of("6001"), Some("pw"), Some(tmp.to_str().unwrap()))
            .unwrap();
        ext.rebuild_auto_ports();
        assert_eq!(ext.port, 5971);
        assert_eq!(ext.ws_port, Some(6001));
        assert_eq!(ext.password.as_deref(), Some("pw"));
        assert_eq!(ext.tls_dir.as_deref(), Some(tmp.as_path()));
        // Auto sides: fixed values must move away from the originals.
        let spec = spec_of("on");
        let mut ext = Exposed::build(&spec, &ws_of("on"), None, None).unwrap();
        let (p0, w0) = (ext.port, ext.ws_port.unwrap());
        let mut guard = 0;
        loop {
            assert!(guard < 200, "rebuild never produced fresh ports");
            if ext.port != p0 && ext.ws_port != Some(w0) {
                break;
            }
            ext.rebuild_auto_ports();
            guard += 1;
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn tls_dir_validation() {
        let tmp = std::env::temp_dir().join(format!("berry-vnccfg-tlsval-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // Empty dir: first missing pem is named, hint points at the helper.
        let e = validate_tls_dir(tmp.to_str().unwrap()).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("ca-cert.pem"), "{}", e.message);
        assert!(e.hint.unwrap().contains("berry vnc certs"));
        for name in ["ca-cert.pem", "server-cert.pem", "server-key.pem"] {
            std::fs::write(tmp.join(name), b"x").unwrap();
        }
        assert!(validate_tls_dir(tmp.to_str().unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn listen_and_pass_roundtrip() {
        let dir = std::env::temp_dir().join(format!("berry-vnccfg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_listen(&dir, "127.0.0.1", 5971, Some(6001), true).unwrap();
        assert_eq!(
            read_listen(&dir),
            Some(ListenInfo {
                host: "127.0.0.1".into(),
                port: 5971,
                ws: Some(6001),
                tls: true,
            })
        );
        write_listen(&dir, "127.0.0.1", 5971, None, false).unwrap();
        assert_eq!(read_listen(&dir).unwrap().ws, None);
        assert!(!read_listen(&dir).unwrap().tls);
        // Wave-1 artifact (no tls field) still reads, as false.
        std::fs::write(
            dir.join("vnc.listen"),
            br#"{"host":"0.0.0.0","port":15944,"ws":null}"#,
        )
        .unwrap();
        assert_eq!(
            read_listen(&dir),
            Some(ListenInfo {
                host: "0.0.0.0".into(),
                port: 15944,
                ws: None,
                tls: false,
            })
        );
        write_pass(&dir, "secret123").unwrap();
        assert_eq!(read_pass(&dir).as_deref(), Some("secret123"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("vnc.pass"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expire_value_validation() {
        assert!(validate_expire("never").is_ok());
        assert!(validate_expire("60").is_ok());
        assert!(validate_expire("86400").is_ok());
        for bad in ["abc", "0", "-1", "", "1.5", "12s"] {
            let e = validate_expire(bad).unwrap_err();
            assert_eq!(e.code, 2, "'{bad}' must be E_USAGE");
            assert!(e.hint.is_some());
        }
        // User form -> QMP wire form: relative "+N" (bare "N" would be
        // absolute epoch seconds and expire immediately — live-calibrated).
        assert_eq!(expire_qmp_time("never"), "never");
        assert_eq!(expire_qmp_time("60"), "+60");
        assert_eq!(expire_qmp_time("2"), "+2");
    }

    #[test]
    fn loopback_detection() {
        for h in ["127.0.0.1", "localhost", "::1", "[::1]"] {
            assert!(is_loopback(h), "{h}");
        }
        for h in ["0.0.0.0", "::", "[::]", "192.168.1.4", "example.com"] {
            assert!(!is_loopback(h), "{h}");
        }
    }

    #[test]
    fn flag_precheck() {
        // The matrix runs against the pure effective-value core so ambient
        // BERRY_VNC/BERRY_VNC_TLS on a dev machine cannot flip expectations;
        // the flag‖env merge itself is pinned by effective_value_merge below
        // and live-checked by the berry-vnc-smoke recipe.
        let vf = validate_effective;
        assert!(vf(None, None, None).is_ok());
        assert!(vf(Some("off"), None, None).is_ok());
        assert!(vf(Some("on"), Some("on"), None).is_ok());
        assert!(vf(Some("127.0.0.1:5971"), Some("6001"), None).is_ok());
        assert!(vf(None, Some("off"), None).is_ok());
        // ws without an exposed endpoint is E_USAGE.
        let e = vf(Some("off"), Some("on"), None).unwrap_err();
        assert_eq!(e.code, 2);
        let e = vf(None, Some("6001"), None).unwrap_err();
        assert_eq!(e.code, 2);
        // tls without an exposed endpoint is E_USAGE (wave 2).
        let e = vf(Some("off"), None, Some("/tmp/any")).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("--vnc-tls"));
        let e = vf(None, None, Some("/tmp/any")).unwrap_err();
        assert_eq!(e.code, 2);
        // tls dir missing its pems is E_USAGE with the certs helper hint.
        let tmp = std::env::temp_dir().join(format!("berry-vnccfg-pre-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let e = vf(Some("on"), None, Some(tmp.to_str().unwrap())).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.hint.unwrap().contains("berry vnc certs"));
        for name in ["ca-cert.pem", "server-cert.pem", "server-key.pem"] {
            std::fs::write(tmp.join(name), b"x").unwrap();
        }
        assert!(vf(Some("on"), Some("on"), Some(tmp.to_str().unwrap())).is_ok());
        let _ = std::fs::remove_dir_all(&tmp);
        // invalid specs surface their own error before any spawn.
        assert_eq!(vf(Some("tcp:127.0.0.1:5971"), None, None).unwrap_err().code, 2);
        assert_eq!(vf(Some("5899"), None, None).unwrap_err().code, 2);
        assert_eq!(vf(Some("on"), Some("70000"), None).unwrap_err().code, 2);
        // An env-provided spec counts as an exposed endpoint for ws/tls
        // (F01: the merge mirrors resolve, so `BERRY_VNC=on … --vnc-ws
        // on` must pre-check clean, not be wrongly rejected).
        assert!(vf(Some("on"), Some("on"), None).is_ok());
        let e = vf(Some("off"), Some("on"), None).unwrap_err();
        assert!(e.hint.unwrap().contains("BERRY_VNC"));
    }

    #[test]
    fn effective_value_merge() {
        // Flag wins over env; env only fills a missing flag — the exact
        // precedence resolve applies (validate_flags reuses this).
        assert_eq!(merge_flag_env(Some("on"), Some("zzz")), Some("on"));
        assert_eq!(merge_flag_env(Some("off"), Some("on")), Some("off"));
        assert_eq!(merge_flag_env(None, Some("zzz")), Some("zzz"));
        assert_eq!(merge_flag_env(None, None), None);
        // An env-only endpoint makes ws/tls pre-checks pass, and an env-only
        // bad spec fails with its own parse error (F01 both directions).
        assert!(validate_effective(Some("on"), Some("6001"), None).is_ok());
        assert_eq!(
            validate_effective(Some("zzz-not-a-spec"), None, None)
                .unwrap_err()
                .message,
            "bad --vnc spec 'zzz-not-a-spec'"
        );
    }

    /// U52 (wave 11): the direct-parameter `resolve` over an explicit
    /// `Launch` — the pure field-by-field half. The env-merge half stays
    /// pinned by `effective_value_merge`; the `Launch::default()` row is
    /// guarded against a dev machine exporting BERRY_VNC/BERRY_VNC_TLS
    /// (the same discipline as `flag_precheck` above).
    #[test]
    fn resolve_direct_parameter_matrix() {
        if std::env::var("BERRY_VNC").is_ok() || std::env::var("BERRY_VNC_TLS").is_ok() {
            return; // ambient env would fill the defaults below
        }
        // Default launch = off triple.
        let r = resolve(Launch::default()).unwrap();
        assert_eq!(r.spec_str, "off");
        assert_eq!(r.ws_str, "off");
        assert!(r.exposed.is_none());
        // Field-by-field: spec/ws/tls strings land in Resolved; the pass
        // file is read one-shot (gone afterwards); tls needs a valid dir.
        let tmp = std::env::temp_dir().join(format!("berry-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        for name in ["ca-cert.pem", "server-cert.pem", "server-key.pem"] {
            std::fs::write(tmp.join(name), b"x").unwrap();
        }
        let pf = tmp.join("vncpass-resolve");
        std::fs::write(&pf, b"secret123\n").unwrap();
        let r = resolve(Launch {
            spec: Some("127.0.0.1:5971".into()),
            ws: Some("6001".into()),
            pass: None,
            pass_file: Some(pf.clone()),
            tls: Some(tmp.to_str().unwrap().into()),
        })
        .unwrap();
        assert_eq!(r.spec_str, "127.0.0.1:5971");
        assert_eq!(r.ws_str, "6001");
        let ext = r.exposed.expect("exposed endpoint");
        assert_eq!(ext.host, "127.0.0.1");
        assert_eq!(ext.port, 5971);
        assert_eq!(ext.ws_port, Some(6001));
        assert_eq!(ext.password.as_deref(), Some("secret123"));
        assert_eq!(ext.tls_dir.as_deref(), Some(tmp.as_path()));
        assert!(!pf.exists(), "one-shot pass file consumed on read");
        // Explicit pass wins over the file; no ws/tls flags -> none.
        let pf2 = tmp.join("vncpass-resolve2");
        std::fs::write(&pf2, b"from-file\n").unwrap();
        let r = resolve(Launch {
            spec: Some("on".into()),
            ws: None,
            pass: Some("explicit".into()),
            pass_file: Some(pf2),
            tls: None,
        })
        .unwrap();
        let ext = r.exposed.expect("exposed endpoint");
        assert_eq!(ext.password.as_deref(), Some("explicit"));
        assert!(ext.ws_port.is_none(), "ws off unless flagged");
        assert!(ext.tls_dir.is_none(), "tls off unless flagged");
        // Same error surface as the sticky era: ws/tls without an
        // exposed endpoint stays E_USAGE.
        let e = match resolve(Launch { ws: Some("on".into()), ..Launch::default() }) {
            Err(e) => e,
            Ok(_) => panic!("ws without an exposed endpoint must be E_USAGE"),
        };
        assert_eq!(e.code, 2);
        assert!(e.message.contains("--vnc-ws"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ------------------------------------------------------------------
    // F20: staged-pass orphan sweep (pure threshold + directory sweep)
    // ------------------------------------------------------------------

    #[test]
    fn stale_vncpass_threshold_matrix() {
        use std::time::Duration;
        let now = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let mk = |age_secs: u64| now - Duration::from_secs(age_secs);
        // Boundary: strictly older than 3600s is stale, 3599s is not.
        assert!(!is_stale_vncpass("vncpass-123-456", mk(3599), now));
        assert!(is_stale_vncpass("vncpass-123-456", mk(3601), now));
        assert!(!is_stale_vncpass("vncpass-123-456", mk(3600), now));
        // Non-staged names are never touched, however old.
        assert!(!is_stale_vncpass("other-123", mk(9_999_999), now));
        assert!(!is_stale_vncpass("", mk(9_999_999), now));
        // mtime in the future (clock skew): never stale.
        assert!(!is_stale_vncpass(
            "vncpass-123-456",
            now + Duration::from_secs(5),
            now
        ));
    }

    #[test]
    fn sweep_orphan_passes_directory_behavior() {
        use std::time::Duration;
        let dir = std::env::temp_dir().join(format!("berry-sweep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vncpass-1-1"), b"old").unwrap();
        std::fs::write(dir.join("vncpass-2-2"), b"fresh").unwrap();
        std::fs::write(dir.join("unrelated-stale"), b"keep").unwrap();
        let now = std::time::SystemTime::now();
        // Aged 2h: both vncpass files look stale by name, but only the
        // threshold decides (here both are fresh on disk; assert no-op).
        sweep_orphan_passes(&dir, now);
        assert!(dir.join("vncpass-1-1").is_file(), "fresh files survive");
        // now = +2h: every staged file is older than the 1h threshold and
        // gets swept; unrelated names stay even when old.
        let future = now + Duration::from_secs(2 * 3600);
        sweep_orphan_passes(&dir, future);
        assert!(!dir.join("vncpass-1-1").exists(), "aged orphan swept");
        assert!(!dir.join("vncpass-2-2").exists(), "aged orphan swept");
        assert!(dir.join("unrelated-stale").is_file(), "non-vncpass kept");
        // A missing directory must not panic.
        sweep_orphan_passes(&dir.join("nope"), future);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
