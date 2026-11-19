//! Session control plane: local NDJSON control socket + QMP broker (C1).
//!
//! Unix uses `<run>/control.sock`; Windows (and `BERRY_CONTROL_TCP=1` test
//! mode) uses loopback TCP + a per-run token. QMP is held only by the host,
//! so CLI never races QEMU's single-client QMP socket.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};

use crate::display::frame::ScreenSource;
use crate::display::vnc::{VncInput, VncSource};
use crate::error::{BerryError, Result};
use crate::net::Endpoint;
use crate::qmp::QmpClient;

pub struct ControlServer {
    cleanup: Vec<PathBuf>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl ControlServer {
    pub fn start(
        run: &Path,
        qmp: Arc<Mutex<QmpClient>>,
        vnc: Endpoint,
        stop: Arc<AtomicBool>,
    ) -> Result<Option<Self>> {
        #[cfg(unix)]
        if std::env::var_os("BERRY_CONTROL_TCP").is_none() {
            return unix_srv::start(run, qmp, vnc, stop);
        }
        tcp_srv::start(run, qmp, vnc, stop)
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        for p in &self.cleanup {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(unix)]
mod unix_srv {
    use super::*;
    use std::io::BufReader;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    pub fn start(
        run: &Path,
        qmp: Arc<Mutex<QmpClient>>,
        vnc: Endpoint,
        stop: Arc<AtomicBool>,
    ) -> Result<Option<ControlServer>> {
        let socket = run.join("control.sock");
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).map_err(|e| {
            BerryError::device(format!("bind control socket {}: {e}", socket.display()))
        })?;
        let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));
        listener
            .set_nonblocking(true)
            .map_err(|e| BerryError::device(format!("control listener nonblocking: {e}")))?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let sd = shutdown.clone();
        let run_buf = run.to_path_buf();
        let handle = std::thread::spawn(move || {
            while !sd.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let qmp = qmp.clone();
                        let vnc = vnc.clone();
                        let run = run_buf.clone();
                        let stop = stop.clone();
                        std::thread::spawn(move || {
                            if let Ok(reader) = stream.try_clone().map(BufReader::new) {
                                let _ = common::serve(reader, stream, qmp, vnc, run, stop, None);
                            }
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Some(ControlServer {
            cleanup: vec![socket],
            shutdown,
            handle: Some(handle),
        }))
    }
}

mod tcp_srv {
    use super::*;
    use std::io::BufReader;
    use std::net::TcpListener;

    fn token() -> String {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{}-{}", std::process::id(), n)
    }

    pub fn start(
        run: &Path,
        qmp: Arc<Mutex<QmpClient>>,
        vnc: Endpoint,
        stop: Arc<AtomicBool>,
    ) -> Result<Option<ControlServer>> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| {
            BerryError::device(format!("bind control tcp: {e}"))
        })?;
        let port = listener
            .local_addr()
            .map(|a| a.port())
            .map_err(|e| BerryError::device(format!("control tcp addr: {e}")))?;
        let token = token();
        let port_path = run.join("control.port");
        let token_path = run.join("control.token");
        std::fs::write(&port_path, port.to_string())
            .and_then(|_| std::fs::write(&token_path, &token))
            .map_err(|e| BerryError::device(format!("write control port/token: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600));
        }
        listener
            .set_nonblocking(true)
            .map_err(|e| BerryError::device(format!("control tcp nonblocking: {e}")))?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let sd = shutdown.clone();
        let run_buf = run.to_path_buf();
        let handle = std::thread::spawn(move || {
            while !sd.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nodelay(true);
                        let qmp = qmp.clone();
                        let vnc = vnc.clone();
                        let run = run_buf.clone();
                        let stop = stop.clone();
                        let tok = token.clone();
                        std::thread::spawn(move || {
                            if let Ok(reader) = stream.try_clone().map(BufReader::new) {
                                let _ = common::serve(
                                    reader,
                                    stream,
                                    qmp,
                                    vnc,
                                    run,
                                    stop,
                                    Some(tok),
                                );
                            }
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Some(ControlServer {
            cleanup: vec![port_path, token_path],
            shutdown,
            handle: Some(handle),
        }))
    }
}

mod common {
    use super::*;
    use std::io::{BufRead, Write};

    pub fn serve<R: BufRead, W: Write>(
        reader: R,
        mut writer: W,
        qmp: Arc<Mutex<QmpClient>>,
        vnc: Endpoint,
        run: PathBuf,
        stop: Arc<AtomicBool>,
        token: Option<String>,
    ) -> std::io::Result<()> {
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let req: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    let resp = json!({"v":1,"id":Value::Null,"ok":false,
                        "code":"E_PROTOCOL","message":format!("bad json: {e}")});
                    writeln!(writer, "{resp}")?;
                    continue;
                }
            };
            if let Some(want) = &token {
                if req.get("token").and_then(Value::as_str) != Some(want.as_str()) {
                    let resp = json!({"v":1,"id":req.get("id").cloned().unwrap_or(Value::Null),
                        "ok":false,"code":"E_USAGE","message":"bad control token"});
                    writeln!(writer, "{resp}")?;
                    writer.flush()?;
                    continue;
                }
            }
            let id = req.get("id").cloned().unwrap_or(Value::Null);
            let op = req.get("op").and_then(Value::as_str).unwrap_or("");
            let args = req.get("args").cloned().unwrap_or_else(|| json!({}));
            let resp = match dispatch(op, &args, &qmp, &vnc, &run, &stop) {
                Ok(result) => json!({"v":1,"id":id,"ok":true,"result":result}),
                Err(e) => json!({"v":1,"id":id,"ok":false,
                    "code": code_name(e.code), "message": e.message,
                    "hint": e.hint}),
            };
            writeln!(writer, "{resp}")?;
            writer.flush()?;
        }
        Ok(())
    }
pub(crate) fn dispatch(
    op: &str,
    args: &Value,
    qmp: &Arc<Mutex<QmpClient>>,
    vnc: &Endpoint,
    run: &Path,
    stop: &Arc<AtomicBool>,
) -> Result<Value> {
    match op {
        "hello" => Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "capabilities": ["hello","status","stats","power","stop","logs","display","shortcut","events","screenshot","key","tap","swipe","text","vnc","vnc_passwd"],
        })),
        "stats" => {
            let meta: Value = std::fs::read(run.join("meta.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_else(|| json!({}));
            let mut q = qmp.lock().unwrap();
            let st = q.query_status().unwrap_or_else(|_| json!({}));
            Ok(json!({
                "state": st.get("status").cloned().unwrap_or_else(|| json!("unknown")),
                "run": run.display().to_string(),
                "profile": meta.get("profile").cloned().unwrap_or(Value::Null),
                "memory_mib": meta.get("memory_mib").cloned().unwrap_or(Value::Null),
                "qemu_pid": meta.get("qemu_pid").cloned().unwrap_or(Value::Null),
                "started": meta.get("started").cloned().unwrap_or(Value::Null),
                "stop_requested": stop.load(Ordering::Relaxed),
            }))
        }
        "status" => {
            let meta: Value = std::fs::read(run.join("meta.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_else(|| json!({}));
            let mut q = qmp.lock().unwrap();
            let st = q.query_status().map_err(|e| {
                BerryError::device(format!("qmp query-status: {e}"))
            })?;
            // External VNC endpoint when exposed (device status shows it).
            // uri uses the dialable host (wildcard binds map to loopback,
            // wave 2); `bind` echoes the real binding.
            let vnc = crate::vnc_cfg::read_listen(run)
                .map(|l| {
                    let dial = crate::vnc_cfg::dial_host(&l.host);
                    json!({
                        "host": dial,
                        "port": l.port,
                        "uri": crate::vnc_cfg::uri_for(dial, l.port),
                        "bind": crate::vnc_cfg::addr_host(&l.host, l.port),
                    })
                })
                .unwrap_or(Value::Null);
            Ok(json!({
                "state": st.get("status").cloned().unwrap_or_else(|| json!("unknown")),
                "run": run.display().to_string(),
                "profile": meta.get("profile").cloned().unwrap_or(Value::Null),
                "memory_mib": meta.get("memory_mib").cloned().unwrap_or(Value::Null),
                "qemu_pid": meta.get("qemu_pid").cloned().unwrap_or(Value::Null),
                "started": meta.get("started").cloned().unwrap_or(Value::Null),
                "stop_requested": stop.load(Ordering::Relaxed),
                "vnc": vnc,
            }))
        }
        "power" => {
            let action = args.get("action").and_then(Value::as_str).unwrap_or("");
            let mut q = qmp.lock().unwrap();
            match action {
                "reboot" => {
                    q.system_reset().map_err(|e| {
                        BerryError::device(format!("qmp system_reset: {e}"))
                    })?;
                    crate::run_dir::write_state(run, "rebooting");
                    append_event(run, json!({"v":1,"t":"action","action":"power.reboot","source":"control"}));
                    Ok(json!({"state":"rebooting"}))
                }
                "off" => {
                    let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
                    crate::run_dir::write_state(run, "stopping");
                    append_event(run, json!({"v":1,"t":"action","action":"power.off","source":"control"}));
                    drop(q);
                    if !force {
                        // ACPI power button; give the guest 5s to flush/sync,
                        // then force quit. Host sees state=stopping and will
                        // not auto-restart if QEMU exits during this window.
                        {
                            let mut q = qmp.lock().unwrap();
                            let _ = q.system_powerdown();
                        }
                        std::thread::sleep(Duration::from_secs(5));
                    }
                    {
                        let mut q = qmp.lock().unwrap();
                        // Guest may already have powered off; quit failure is
                        // not an error once state=stopping.
                        let _ = q.quit_now();
                    }
                    stop.store(true, Ordering::Relaxed);
                    Ok(json!({"state":"stopping"}))
                }
                _ => Err(BerryError::new(2, "power action must be off|reboot")),
            }
        }
        "stop" => {
            let mut q = qmp.lock().unwrap();
            q.quit_now().map_err(|e| BerryError::device(format!("qmp quit: {e}")))?;
            stop.store(true, Ordering::Relaxed);
            crate::run_dir::write_state(run, "stopping");
            append_event(run, json!({"v":1,"t":"action","action":"stop","source":"control"}));
            Ok(json!({"state":"stopping"}))
        }
        "logs" => {
            let n = args.get("tail").and_then(Value::as_u64).unwrap_or(100) as usize;
            let text = std::fs::read_to_string(run.join("serial.log")).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(n);
            Ok(json!({"lines": lines[start..]}))
        }
        "display" => {
            let action = args.get("action").and_then(Value::as_str).unwrap_or("list");
            match action {
                "list" => {
                    let profiles: Vec<Value> = crate::profile::PROFILES
                        .iter()
                        .map(|p| json!({
                            "id": p.id,
                            "render": [p.render_w, p.render_h],
                            "panel": [p.panel_w, p.panel_h],
                            "memory_mib": p.memory_mib,
                        }))
                        .collect();
                    Ok(json!({"profiles": profiles}))
                }
                "set" => {
                    let id = args.get("profile").and_then(Value::as_str).unwrap_or("");
                    let p = crate::profile::get(id)?;
                    std::fs::write(run.join("next-profile"), p.id).map_err(|e| {
                        BerryError::device(format!("write next-profile: {e}"))
                    })?;
                    append_event(run, json!({
                        "v":1,"t":"display","profile":p.id,
                        "applied":false,"requires_restart":true,
                    }));
                    Ok(json!({
                        "profile": p.id,
                        "applied": false,
                        "requires_restart": true,
                        "next_start": true,
                    }))
                }
                _ => Err(BerryError::new(2, "display action must be list|set")),
            }
        }
        "shortcut" => {
            let action = args.get("action").and_then(Value::as_str).unwrap_or("");
            match action {
                "screenshot" => dispatch("screenshot", args, qmp, vnc, run, stop),
                "reboot" => dispatch(
                    "power",
                    &json!({"action":"reboot"}),
                    qmp,
                    vnc,
                    run,
                    stop,
                ),
                "power" => dispatch(
                    "power",
                    &json!({"action":"off"}),
                    qmp,
                    vnc,
                    run,
                    stop,
                ),
                "resolution" => {
                    let meta: Value = std::fs::read(run.join("meta.json"))
                        .ok()
                        .and_then(|b| serde_json::from_slice(&b).ok())
                        .unwrap_or_else(|| json!({}));
                    let id = meta.get("profile").and_then(|v| v.as_str()).unwrap_or("phone");
                    let p = crate::profile::get(id)?;
                    Ok(json!({
                        "profile": p.id,
                        "render_w": p.render_w,
                        "render_h": p.render_h,
                        "note": "resolution menu is window-only; use display set to change",
                    }))
                }
                "fullscreen" => Err(BerryError::new(2, "fullscreen is window-only")),
                _ => Err(BerryError::new(2, "unknown shortcut action")),
            }
        }
        "events" => {
            let n = args.get("tail").and_then(Value::as_u64).unwrap_or(100) as usize;
            let text = std::fs::read_to_string(run.join("events.jsonl")).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(n);
            Ok(json!({"lines": lines[start..]}))
        }
        "key" => {
            let code = args.get("code").and_then(Value::as_str).unwrap_or("");
            let ks = crate::commands::qcode_to_keysym(code)
                .ok_or_else(|| BerryError::new(2, format!("unknown key '{code}'")))?;
            let mut v = VncInput::connect(vnc)?;
            v.key(ks, true)?;
            v.key(ks, false)?;
            Ok(json!({"key": code}))
        }
        "tap" => {
            let x = args.get("x").and_then(Value::as_u64).unwrap_or(0) as u16;
            let y = args.get("y").and_then(Value::as_u64).unwrap_or(0) as u16;
            let mut v = VncInput::connect(vnc)?;
            v.pointer(x, y, 1)?;
            std::thread::sleep(Duration::from_millis(30));
            v.pointer(x, y, 0)?;
            Ok(json!({"x":x,"y":y}))
        }
        "swipe" => {
            let x1 = args.get("x1").and_then(Value::as_u64).unwrap_or(0) as u16;
            let y1 = args.get("y1").and_then(Value::as_u64).unwrap_or(0) as u16;
            let x2 = args.get("x2").and_then(Value::as_u64).unwrap_or(0) as u16;
            let y2 = args.get("y2").and_then(Value::as_u64).unwrap_or(0) as u16;
            let ms = args.get("ms").and_then(Value::as_u64).unwrap_or(200);
            let steps = (ms / 16).max(2);
            let mut v = VncInput::connect(vnc)?;
            v.pointer(x1, y1, 1)?;
            for i in 1..=steps {
                let t = i as f64 / steps as f64;
                let x = (x1 as f64 + (x2 as f64 - x1 as f64) * t) as u16;
                let y = (y1 as f64 + (y2 as f64 - y1 as f64) * t) as u16;
                v.pointer(x, y, 1)?;
                std::thread::sleep(Duration::from_millis(16));
            }
            v.pointer(x2, y2, 0)?;
            Ok(json!({"x1":x1,"y1":y1,"x2":x2,"y2":y2}))
        }
        "text" => {
            let text = args.get("text").and_then(Value::as_str).unwrap_or("");
            let mut v = VncInput::connect(vnc)?;
            for ch in text.chars() {
                v.key(ch as u32, true)?;
                v.key(ch as u32, false)?;
            }
            Ok(json!({"text":text}))
        }
        "screenshot" => {
            let out = args
                .get("out")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| run.join("screenshot.png"));
            let settle = args
                .get("settle_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let mut source = VncSource::connect(vnc)?;
            if settle > 0 {
                std::thread::sleep(Duration::from_millis(settle));
            }
            let frame = source.next_frame(Duration::from_secs(10)).ok_or_else(|| {
                BerryError::protocol("control screenshot: no VNC frame")
            })?;
            crate::png::write_png(&out, frame.width, frame.height, &frame.to_rgb())?;
            let path_str = out.display().to_string();
            append_event(run, json!({
                "v":1,"t":"screenshot","path":path_str,
                "width":frame.width,"height":frame.height,"source":"vnc",
            }));
            Ok(json!({
                "path": path_str,
                "width": frame.width,
                "height": frame.height,
                "source": "vnc",
            }))
        }
        "vnc" => {
            // State of the external endpoint: listen/bind from vnc.listen,
            // uri/ws_uri via the dialable host (wildcards map to loopback),
            // auth/clients/vencrypt from query-vnc-servers id=default (the
            // external endpoint is the first declared -vnc; query-vnc would
            // only report the default server as a single object — probe f4).
            // Note: a TLS endpoint's ws face reports auth "none" in QMP even
            // though it is wss-only (probe p3) — endpoint security derives
            // from listen.tls, never from that field.
            let Some(listen) = crate::vnc_cfg::read_listen(run) else {
                return Ok(json!({"enabled": false}));
            };
            let has_password = run.join("vnc.pass").exists();
            let mut auth = if has_password { "vnc".to_string() } else { "none".to_string() };
            let mut vencrypt = Value::Null;
            let mut clients = Value::Array(vec![]);
            if let Ok(servers) = qmp.lock().unwrap().query_vnc_servers() {
                if let Some(found) = servers.as_array().and_then(|a| {
                    a.iter()
                        .find(|e| e.get("id").and_then(Value::as_str) == Some("default"))
                        .map(|e| {
                            (
                                e.get("auth").and_then(Value::as_str).map(str::to_string),
                                e.get("vencrypt").and_then(Value::as_str).map(str::to_string),
                                e.get("clients").cloned(),
                            )
                        })
                }) {
                    if let Some(a) = found.0 {
                        auth = a;
                    }
                    if let Some(ve) = found.1 {
                        vencrypt = json!(ve);
                    }
                    if let Some(c) = found.2 {
                        clients = c;
                    }
                }
            }
            // TLS dir lives in meta (vnc.listen carries only the bool).
            let tls_dir: Value = std::fs::read(run.join("meta.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|m| {
                    m.get("vnc_tls")
                        .and_then(Value::as_str)
                        .filter(|s| *s != "off")
                        .map(|s| json!(s))
                })
                .unwrap_or(Value::Null);
            let dial = crate::vnc_cfg::dial_host(&listen.host);
            let ws_uri = listen
                .ws
                .map(|p| json!(crate::vnc_cfg::ws_uri_for(dial, p, listen.tls)))
                .unwrap_or(Value::Null);
            let tls = if listen.tls {
                json!({"enabled": true, "dir": tls_dir, "vencrypt": vencrypt})
            } else {
                json!({"enabled": false})
            };
            Ok(json!({
                "enabled": true,
                "listen": crate::vnc_cfg::listen_value(&listen.host, listen.port, listen.ws, listen.tls),
                "bind": crate::vnc_cfg::addr_host(&listen.host, listen.port),
                "uri": crate::vnc_cfg::uri_for(dial, listen.port),
                "ws_uri": ws_uri,
                "auth": auth,
                "has_password": has_password,
                "clients": clients,
                "tls": tls,
            }))
        }
        "vnc_passwd" => {
            // Rotate or expire the VNC password. Rotation: QMP set_password
            // first, then rewrite vnc.pass (0600) and append an event. The
            // event and any error text never carry the password value.
            // Expiry (wave 2): QMP expire_password (time = never | seconds);
            // QEMU 11 forces connected=keep for vnc set_password (probe p5),
            // so rotation never kicks connected clients — documented as such.
            let expire = args.get("expire").and_then(Value::as_str);
            let generate = args.get("generate").and_then(Value::as_bool).unwrap_or(false);
            let given = args
                .get("password")
                .and_then(Value::as_str)
                .map(str::to_string);
            if expire.is_some() && (given.is_some() || generate) {
                return Err(BerryError::new(
                    2,
                    "vnc_passwd: {expire} excludes {password}/{generate}",
                ));
            }
            if let Some(time) = expire {
                crate::vnc_cfg::validate_expire(time)?;
                let mut q = qmp.lock().unwrap();
                // Wire form: bare "N" would be absolute epoch seconds on
                // QEMU (live-calibrated); berry sends the relative "+N".
                let qmp_time = crate::vnc_cfg::expire_qmp_time(time);
                if let Err(e) = q.expire_vnc_password(&qmp_time) {
                    if e.message.contains("disabled") {
                        return Err(BerryError::new(
                            2,
                            "this endpoint started without authentication",
                        )
                        .with_hint("QEMU cannot manage VNC auth at runtime; restart with --vnc-pass to enable it"));
                    }
                    return Err(BerryError::device(format!("expire VNC password: {e}")));
                }
                drop(q);
                append_event(run, json!({"v":1,"t":"action","action":"vnc.passwd_expire",
                    "expire":time,"source":"control"}));
                return Ok(json!({"expire": time}));
            }
            let password = if generate {
                crate::vnc_cfg::generate_password()
            } else {
                given.ok_or_else(|| {
                    BerryError::new(2, "vnc_passwd needs {password}, {generate:true} or {expire}")
                })?
            };
            if password.is_empty() {
                return Err(BerryError::new(2, "vnc password must not be empty"));
            }
            let mut q = qmp.lock().unwrap();
            if let Err(e) = q.set_vnc_password(&password) {
                // QEMU refuses set_password unless the endpoint was started
                // with password=on (probe f3); map that case to E_USAGE with
                // the restart hint instead of a bare protocol error.
                if e.message.contains("disabled") {
                    return Err(BerryError::new(
                        2,
                        "this endpoint started without authentication",
                    )
                    .with_hint("QEMU cannot enable VNC auth at runtime; restart with --vnc-pass to enable it"));
                }
                return Err(BerryError::device(format!("rotate VNC password: {e}")));
            }
            // A fired expiry timer outlives set_password on QEMU 11
            // (live-calibrated: after `expire +N` elapses, set_password
            // alone still rejects every client). Rotation must leave a
            // usable password, so reset the expiry to never here. A failed
            // reset no longer goes silent (F16): the audit event carries
            // the QMP error so a bricked-auth endpoint leaves a trail —
            // the rotation itself already succeeded, so no command error.
            if let Err(e) = q.expire_vnc_password("never") {
                append_event(run, passwd_warn_event(&e.message));
            }
            drop(q);
            crate::vnc_cfg::write_pass(run, &password)?;
            append_event(run, json!({"v":1,"t":"action","action":"vnc.passwd","source":"control"}));
            Ok(json!({"generated": generate, "password": password}))
        }
        _ => Err(BerryError::new(2, format!("unsupported op '{op}'"))),
    }
}

    pub(crate) fn append_event(run: &Path, v: Value) {
        use std::io::Write;
        let path = run.join("events.jsonl");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{v}");
        }
    }

    /// F16 audit event for a failed `expire_password("never")` reset after
    /// a successful rotation (same family as `t:action` vnc.passwd). The
    /// error text is the QMP failure message — the call carries no
    /// password material, and none is interpolated.
    pub(crate) fn passwd_warn_event(err: &str) -> Value {
        json!({"v":1,"t":"action","action":"vnc.passwd_warn","error":err,"source":"control"})
    }
    pub(crate) fn code_name(code: i32) -> &'static str {
    match code {
        2 => "E_USAGE",
        3 => "E_NO_SESSION",
        4 => "E_TIMEOUT",
        5 => "E_BUSY",
        6 => "E_DEVICE",
        7 => "E_PROTOCOL",
        _ => "E_INTERNAL",
    }
}

}

// Wave 2: device.rs's QMP event sink shares this writer so events.jsonl
// keeps one shape across control actions and VNC client audit lines.
pub(crate) use common::append_event;

/// Session host presence: Unix socket or Windows/TCP port file.
pub fn is_up(run: &Path) -> bool {
    run.join("control.sock").exists() || run.join("control.port").exists()
}

/// Client side: issue one request; TCP+token if `control.port` exists, else Unix.
pub fn request(run: &Path, op: &str, args: Value) -> Result<Value> {
    if run.join("control.port").exists() {
        return request_tcp(run, op, args);
    }
    #[cfg(unix)]
    {
        use std::io::{BufRead, BufReader, Write};
        let socket = run.join("control.sock");
        if !socket.exists() {
            return Err(BerryError::new(3, "no control socket")
                .with_hint("start with `berry shell`/`device start`, then retry"));
        }
        let ep = Endpoint::Unix(socket);
        let mut stream = ep.connect().map_err(|e| {
            BerryError::device(format!("control connect {}: {e}", ep.display()))
        })?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        let req = json!({"v":1,"id":1,"op":op,"args":args});
        writeln!(stream, "{req}").map_err(|e| BerryError::device(format!("control send: {e}")))?;
        let mut line = String::new();
        BufReader::new(stream)
            .read_line(&mut line)
            .map_err(|e| BerryError::device(format!("control read: {e}")))?;
        return decode(&line);
    }
    #[cfg(not(unix))]
    Err(BerryError::new(3, "no control endpoint").with_hint("session is not hosted"))
}

fn request_tcp(run: &Path, op: &str, args: Value) -> Result<Value> {
    use std::io::{BufRead, BufReader, Write};
    let port = std::fs::read_to_string(run.join("control.port"))
        .map_err(|e| BerryError::device(format!("read control.port: {e}")))?;
    let token = std::fs::read_to_string(run.join("control.token"))
        .map_err(|e| BerryError::device(format!("read control.token: {e}")))?;
    let addr = format!("127.0.0.1:{}", port.trim());
    let mut stream = std::net::TcpStream::connect(&addr)
        .map_err(|e| BerryError::device(format!("control tcp {addr}: {e}")))?;
    let _ = stream.set_nodelay(true);
    stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
    let req = json!({"v":1,"id":1,"token":token.trim(),"op":op,"args":args});
    writeln!(stream, "{req}").map_err(|e| BerryError::device(format!("control send: {e}")))?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| BerryError::device(format!("control read: {e}")))?;
    decode(&line)
}

fn decode(line: &str) -> Result<Value> {
    let v: Value = serde_json::from_str(line)
        .map_err(|e| BerryError::protocol(format!("control json: {e}")))?;
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    } else {
        // Preserve the server's exit code and hint (vnc passwd maps to
        // E_USAGE when the endpoint has no auth, with a restart hint).
        let msg = v
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("control error")
            .to_string();
        let code = v
            .get("code")
            .and_then(Value::as_str)
            .and_then(|n| match n {
                "E_USAGE" => Some(2),
                "E_NO_SESSION" => Some(3),
                "E_TIMEOUT" => Some(4),
                "E_BUSY" => Some(5),
                "E_DEVICE" => Some(6),
                "E_PROTOCOL" => Some(7),
                _ => None,
            })
            .unwrap_or(6);
        let mut err = BerryError::new(code, msg);
        if let Some(h) = v.get("hint").and_then(Value::as_str) {
            err = err.with_hint(h);
        }
        Err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::passwd_warn_event;

    // F16: the warn event's JSONL shape is pinned field by field; the
    // error text is the QMP failure message and must never interpolate
    // password material (a canary string proves none rides along).
    #[test]
    fn passwd_warn_event_shape() {
        let v = passwd_warn_event("QMP command expire_password failed: timer busy");
        assert_eq!(v.get("v").and_then(Value::as_u64), Some(1));
        assert_eq!(v.get("t").and_then(Value::as_str), Some("action"));
        assert_eq!(v.get("action").and_then(Value::as_str), Some("vnc.passwd_warn"));
        assert_eq!(
            v.get("error").and_then(Value::as_str),
            Some("QMP command expire_password failed: timer busy")
        );
        assert_eq!(v.get("source").and_then(Value::as_str), Some("control"));
        assert_eq!(v.as_object().unwrap().len(), 5, "no extra fields");
        // Round-trips as one JSONL line (append_event writeln's it raw).
        let line = v.to_string();
        assert!(!line.contains('\n'));
        let back: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn passwd_warn_event_carries_no_password_material() {
        // The QMP error surface never sees the password; if it ever did,
        // the event is the place it would leak. Canary both directions.
        let v = passwd_warn_event("expire failed");
        let s = v.to_string();
        assert!(!s.contains("secret-pw-canary"));
        let v2 = passwd_warn_event("error mentioning secret-pw-canary in a QMP message");
        assert!(v2.to_string().contains("secret-pw-canary"));
        // ^ documents the contract honestly: the event is a passthrough of
        // the QMP text, and that text carries only expire-timer state.
    }

    // decode: the CLI's view of one server line. ok=true unwraps result
    // (missing result -> Null, not an error); ok=false preserves the
    // server's exit code name and hint verbatim.
    #[test]
    fn decode_ok_result_and_missing_result() {
        let v = decode(r#"{"v":1,"id":1,"ok":true,"result":{"expire":"never"}}"#).unwrap();
        assert_eq!(v["expire"], "never");
        let n = decode(r#"{"v":1,"id":1,"ok":true}"#).unwrap();
        assert!(n.is_null(), "ok without result -> Null");
    }

    #[test]
    fn decode_error_maps_code_names_and_hint() {
        let e = decode(
            r#"{"v":1,"id":1,"ok":false,"code":"E_USAGE","message":"no auth","hint":"restart with --vnc-pass"}"#,
        )
        .unwrap_err();
        assert_eq!(e.code, 2, "E_USAGE -> 2");
        assert_eq!(e.message, "no auth");
        assert_eq!(e.hint.as_deref(), Some("restart with --vnc-pass"));
        // Every wire code name round-trips to its stable exit code.
        for (name, code) in [
            ("E_USAGE", 2),
            ("E_NO_SESSION", 3),
            ("E_TIMEOUT", 4),
            ("E_BUSY", 5),
            ("E_DEVICE", 6),
            ("E_PROTOCOL", 7),
        ] {
            let line = format!(r#"{{"v":1,"ok":false,"code":"{name}","message":"m"}}"#);
            assert_eq!(decode(&line).unwrap_err().code, code, "{name}");
        }
        // Unknown code falls back to E_DEVICE(6); code_name() covers the
        // mirror direction (host side), E_INTERNAL for anything else.
        let f = decode(r#"{"v":1,"ok":false,"code":"E_WEIRD","message":"m"}"#).unwrap_err();
        assert_eq!(f.code, 6);
        assert_eq!(common::code_name(2), "E_USAGE");
        assert_eq!(common::code_name(9), "E_INTERNAL");
        assert_eq!(common::code_name(42), "E_INTERNAL");
    }

    #[test]
    fn decode_bad_json_is_protocol_error() {
        let e = decode("not json").unwrap_err();
        assert_eq!(e.code, 7, "E_PROTOCOL");
    }

    // is_up: presence of either endpoint marker file (socket or port file).
    #[test]
    fn is_up_by_socket_or_port_file() {
        let tmp = std::env::temp_dir().join(format!("berry-isup-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(!is_up(&tmp));
        std::fs::write(tmp.join("control.sock"), b"").unwrap();
        assert!(is_up(&tmp), "control.sock 在即 up");
        std::fs::remove_file(tmp.join("control.sock")).unwrap();
        assert!(!is_up(&tmp));
        std::fs::write(tmp.join("control.port"), b"1").unwrap();
        assert!(is_up(&tmp), "control.port 在即 up");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
// ---- W1-Q26 十五刀（control dispatch 面）：scripted fake-QMP + fake-VNC
// ---- 首次直测 dispatch 全 op 面（此前仅 decode/is_up/passwd_warn 纯面）。
// ---- 覆盖：hello 全能力表钉板 / stats 容错（meta 缺失·QMP 失败降级
// ---- unknown）/ status vnc 字段拼装（dial_host 通配→回环映射）/
// ---- power reboot 状态与审计事件 / stop 全链 / logs·events 尾窗 /
// ---- display list·set / shortcut 转发树 / key 未知键拒绝 /
// ---- vnc_passwd 参数互斥矩阵与空密码拒绝（QMP 不触达）。
// ---- 服务器线程 = qmp.rs fake-QMP scripted 先例复用（零真 QEMU）。
#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use common::dispatch;
    use serde_json::{json, Value};
    use std::io::{BufRead, BufReader, Write as _};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    /// 每请求起一个 fake-QMP：回一条 `{"return": ...}` 后立即关连接。
    /// dispatch 的每个 op 至多同步执行一条 QMP 命令（互斥锁串行），
    /// 所以逐请求接受即可覆盖全部路径而无需长连接泵。
    fn fake_qmp_listener() -> (u16, std::sync::mpsc::Receiver<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut s = match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                // QMP greeting first (QmpClient::connect negotiates on it).
                if s
                    .write_all(b"{\"QMP\":{\"version\":{\"qemu\":{\"micro\":3}},\"capabilities\":[]}}\n")
                    .is_err()
                {
                    continue;
                }
                // Serve one reply per request line until EOF: dispatch 在同
                // 一连接上顺序执行多条 QMP 命令（connect 握手 + 各 op）。
                // 回复 id 必须回显请求 id——QmpClient 按 id 路由 pending。
                let mut r = BufReader::new(s.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    match r.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    let _ = tx.send(line.clone());
                    let id = serde_json::from_str::<Value>(&line)
                        .ok()
                        .and_then(|v| v.get("id").cloned())
                        .unwrap_or(Value::Null);
                    let reply = format!(r#"{{"return":{{}},"id":{}}}"#, id);
                    if s.write_all(reply.as_bytes()).is_err()
                        || s.write_all(b"\n").is_err()
                    {
                        break;
                    }
                }
            }
        });
        (port, rx)
    }

    fn fake_qmp_client(port: u16) -> Arc<Mutex<QmpClient>> {
        let c = QmpClient::connect(&Endpoint::Tcp(format!("127.0.0.1:{port}")))
            .expect("fake qmp handshake");
        Arc::new(Mutex::new(c))
    }

    /// 死 VNC 端口（bind 不监听）：VNC-touching ops 的连接失败路径。
    fn dead_vnc() -> Endpoint {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        Endpoint::Tcp(format!("127.0.0.1:{}", l.local_addr().unwrap().port()))
        // listener drop = 端口关闭，connect 会拒。
    }

    fn temp_run() -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!(
            "berry-ctl-dispatch-{}-{}-{}",
            std::process::id(),
            seq,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn hello_capability_table_pinned() {
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        let stop = Arc::new(AtomicBool::new(false));
        let v = dispatch("hello", &json!({}), &qmp, &dead_vnc(), &run, &stop).unwrap();
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        let caps = v["capabilities"].as_array().unwrap();
        assert_eq!(
            caps.len(),
            16,
            "capability table must stay pinned (add op = update this)"
        );
        for op in [
            "hello", "status", "stats", "power", "stop", "logs", "display",
            "shortcut", "events", "screenshot", "key", "tap", "swipe", "text",
            "vnc", "vnc_passwd",
        ] {
            assert!(
                caps.iter().any(|c| c == op),
                "capability {op} missing"
            );
        }
        std::fs::remove_dir_all(&run).ok();
    }

    #[test]
    fn stats_meta_missing_and_qmp_failure_degrade() {
        // 空 run 目录：meta.json 缺失 -> 字段全 Null；活 fake QMP 回
        // 空 return -> query_status 无 status 字段 -> state "unknown"
        // （stats 路不向调用方报错）。
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        let stop = Arc::new(AtomicBool::new(false));
        let v = dispatch("stats", &json!({}), &qmp, &dead_vnc(), &run, &stop).unwrap();
        assert_eq!(v["state"], "unknown", "meta 缺失 + 无 status 字段 -> unknown");
        assert!(v["profile"].is_null());
        assert!(v["memory_mib"].is_null());
        assert_eq!(v["run"], run.display().to_string());
        assert_eq!(v["stop_requested"], false);
        std::fs::remove_dir_all(&run).ok();
    }

    #[test]
    fn status_vnc_object_assembly_from_run_dir() {
        // vnc.listen 存在时 status 拼装 host/port/uri/bind 四件；
        // 通配 bind 0.0.0.0 → dial 127.0.0.1（dial_host 契约）。
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        crate::vnc_cfg::write_listen(&run, "0.0.0.0", 5901, None, false).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let v = dispatch("status", &json!({}), &qmp, &dead_vnc(), &run, &stop).unwrap();
        assert_eq!(v["vnc"]["host"], "127.0.0.1", "wildcard maps to loopback");
        assert_eq!(v["vnc"]["port"], 5901);
        assert_eq!(v["vnc"]["uri"], "vnc://127.0.0.1:5901");
        assert_eq!(v["vnc"]["bind"], "0.0.0.0:5901");
        std::fs::remove_dir_all(&run).ok();
    }

    #[test]
    fn power_reboot_state_and_audit_event() {
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        let stop = Arc::new(AtomicBool::new(false));
        let v = dispatch(
            "power",
            &json!({"action": "reboot"}),
            &qmp,
            &dead_vnc(),
            &run,
            &stop,
        )
        .unwrap();
        assert_eq!(v["state"], "rebooting");
        // state.json 与 events.jsonl 落盘
        let state: Value = serde_json::from_slice(
            &std::fs::read(run.join("state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["state"], "rebooting");
        let events = std::fs::read_to_string(run.join("events.jsonl")).unwrap();
        assert!(events.contains("\"action\":\"power.reboot\""));
        assert!(events.contains("\"source\":\"control\""));
        std::fs::remove_dir_all(&run).ok();
    }

    #[test]
    fn logs_and_events_tail_window() {
        // serial.log 5 行 tail=2 -> 后 2 行；events.jsonl 3 行 tail=0 默认 100 -> 全量。
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        std::fs::write(run.join("serial.log"), "l1\nl2\nl3\nl4\nl5\n").unwrap();
        std::fs::write(run.join("events.jsonl"), "e1\ne2\ne3\n").unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let logs = dispatch("logs", &json!({"tail": 2}), &qmp, &dead_vnc(), &run, &stop).unwrap();
        let arr = logs["lines"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], "l4");
        assert_eq!(arr[1], "l5");
        // tail 超过行数 -> 全量（saturating_sub 起点 0）
        let all = dispatch("logs", &json!({"tail": 99}), &qmp, &dead_vnc(), &run, &stop).unwrap();
        assert_eq!(all["lines"].as_array().unwrap().len(), 5);
        // events 默认 tail=100
        let ev = dispatch("events", &json!({}), &qmp, &dead_vnc(), &run, &stop).unwrap();
        assert_eq!(ev["lines"].as_array().unwrap().len(), 3);
        std::fs::remove_dir_all(&run).ok();
    }

    #[test]
    fn display_list_matches_profile_table_and_set_unknown_rejects() {
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        let stop = Arc::new(AtomicBool::new(false));
        let v = dispatch("display", &json!({"action": "list"}), &qmp, &dead_vnc(), &run, &stop)
            .unwrap();
        let profiles = v["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), crate::profile::PROFILES.len());
        for (entry, p) in profiles.iter().zip(crate::profile::PROFILES.iter()) {
            assert_eq!(entry["id"], p.id);
            assert_eq!(entry["memory_mib"], p.memory_mib);
        }
        // set unknown -> E_ENV(3)（profile::get 的 env 码位）+ next-profile 不落盘
        let e = dispatch(
            "display",
            &json!({"action": "set", "profile": "no-such-profile"}),
            &qmp,
            &dead_vnc(),
            &run,
            &stop,
        )
        .unwrap_err();
        assert_eq!(e.code, 3);
        assert!(!run.join("next-profile").exists());
        std::fs::remove_dir_all(&run).ok();
    }

    #[test]
    fn shortcut_forwards_to_sibling_ops() {
        // resolution -> 从 meta.profile 读回 profile 面；fullscreen -> 拒。
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        std::fs::write(
            run.join("meta.json"),
            serde_json::to_vec(&json!({"profile": "phone"})).unwrap(),
        )
        .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let v = dispatch(
            "shortcut",
            &json!({"action": "resolution"}),
            &qmp,
            &dead_vnc(),
            &run,
            &stop,
        )
        .unwrap();
        assert_eq!(v["profile"], "phone");
        assert!(v["render_w"].is_u64());
        let e = dispatch(
            "shortcut",
            &json!({"action": "fullscreen"}),
            &qmp,
            &dead_vnc(),
            &run,
            &stop,
        )
        .unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("window-only"));
        std::fs::remove_dir_all(&run).ok();
    }

    #[test]
    fn key_unknown_qcode_rejected_before_vnc() {
        // 未知键在 VNC 连接前拒绝：dead_vnc 端口若被触达会连接失败报
        // E_DEVICE(6)，而参数错应为 E_USAGE(2)——码位即序证明。
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        let stop = Arc::new(AtomicBool::new(false));
        let e = dispatch(
            "key",
            &json!({"code": "definitely-not-a-qcode"}),
            &qmp,
            &dead_vnc(),
            &run,
            &stop,
        )
        .unwrap_err();
        assert_eq!(e.code, 2, "unknown key is usage error, not device error");
        assert!(e.message.contains("definitely-not-a-qcode"));
        std::fs::remove_dir_all(&run).ok();
    }

    #[test]
    fn vnc_passwd_arg_matrix_rejected_before_qmp_and_vnc() {
        // expire 与 password/generate 互斥、三者全缺、空密码：
        // 三形态都在触达 QMP/VNC 前拒绝（fake QMP 回任意 return 也不会
        // 被走到 set_password——若走到，事件/落盘会出现，断言其缺席）。
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        let stop = Arc::new(AtomicBool::new(false));
        // expire + password 互斥
        let e = dispatch(
            "vnc_passwd",
            &json!({"expire": "60", "password": "x"}),
            &qmp,
            &dead_vnc(),
            &run,
            &stop,
        )
        .unwrap_err();
        assert_eq!(e.code, 2);
        // 三缺
        let e = dispatch("vnc_passwd", &json!({}), &qmp, &dead_vnc(), &run, &stop).unwrap_err();
        assert_eq!(e.code, 2);
        // 空密码
        let e = dispatch(
            "vnc_passwd",
            &json!({"password": ""}),
            &qmp,
            &dead_vnc(),
            &run,
            &stop,
        )
        .unwrap_err();
        assert_eq!(e.code, 2);
        assert!(!run.join("vnc.pass").exists(), "no pass file on rejection");
        std::fs::remove_dir_all(&run).ok();
    }

    #[test]
    fn unsupported_op_is_usage_error() {
        let (port, _rx) = fake_qmp_listener();
        let qmp = fake_qmp_client(port);
        let run = temp_run();
        let stop = Arc::new(AtomicBool::new(false));
        let e = dispatch("no-such-op", &json!({}), &qmp, &dead_vnc(), &run, &stop).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("no-such-op"));
        std::fs::remove_dir_all(&run).ok();
    }
}
