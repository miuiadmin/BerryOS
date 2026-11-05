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
                    "code": code_name(e.code), "message": e.message}),
            };
            writeln!(writer, "{resp}")?;
            writer.flush()?;
        }
        Ok(())
    }
fn dispatch(
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
            "capabilities": ["hello","status","stats","power","stop","logs","display","shortcut","events","screenshot","key","tap","swipe","text"],
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
        _ => Err(BerryError::new(2, format!("unsupported op '{op}'"))),
    }
}

fn append_event(run: &Path, v: Value) {
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

fn code_name(code: i32) -> &'static str {
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
        let msg = v.get("message").and_then(Value::as_str).unwrap_or("control error");
        Err(BerryError::device(msg.to_string()))
    }
}
