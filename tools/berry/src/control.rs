//! Session control plane: local NDJSON socket + QMP broker (C1).
//!
//! The GUI/session host binds `<run>/control.sock`; CLI clients connect and
//! send one JSON object per line. QMP is held only by the host: the broker
//! serializes it, so CLI never races QEMU's single-client QMP socket.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::display::frame::ScreenSource;
use crate::display::vnc::VncSource;
use crate::error::{BerryError, Result};
use crate::net::Endpoint;
use crate::qmp::QmpClient;

#[cfg(unix)]
pub use imp::ControlServer;

#[cfg(not(unix))]
pub use stub::ControlServer;

#[cfg(not(unix))]
mod stub {
    use super::*;

    /// Non-Unix C1 is not wired yet (Windows named pipe / TCP+token).
    pub struct ControlServer;

    impl ControlServer {
        pub fn start(
            _run: &Path,
            _qmp: Arc<Mutex<QmpClient>>,
            _vnc: Endpoint,
            _stop: Arc<AtomicBool>,
        ) -> Result<Option<Self>> {
            Ok(None)
        }
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::thread::{self, JoinHandle};

    pub struct ControlServer {
        socket: PathBuf,
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
            let socket = run.join("control.sock");
            let _ = std::fs::remove_file(&socket);
            let listener = UnixListener::bind(&socket).map_err(|e| {
                BerryError::device(format!(
                    "bind control socket {}: {e}",
                    socket.display()
                ))
            })?;
            let _ = std::fs::set_permissions(
                &socket,
                std::fs::Permissions::from_mode(0o600),
            );
            listener.set_nonblocking(true).map_err(|e| {
                BerryError::device(format!("control listener nonblocking: {e}"))
            })?;

            let shutdown = Arc::new(AtomicBool::new(false));
            let shutdown_thread = shutdown.clone();
            let run = run.to_path_buf();
            let handle = thread::spawn(move || {
                while !shutdown_thread.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let qmp = qmp.clone();
                            let vnc = vnc.clone();
                            let run = run.clone();
                            let stop = stop.clone();
                            thread::spawn(move || {
                                let _ = serve(stream, qmp, vnc, run, stop);
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(50));
                        }
                        Err(_) => break,
                    }
                }
            });

            Ok(Some(Self {
                socket,
                shutdown,
                handle: Some(handle),
            }))
        }
    }

    impl Drop for ControlServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
            let _ = std::fs::remove_file(&self.socket);
        }
    }

    fn serve(
        stream: UnixStream,
        qmp: Arc<Mutex<QmpClient>>,
        vnc: Endpoint,
        run: PathBuf,
        stop: Arc<AtomicBool>,
    ) -> std::io::Result<()> {
        let reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;
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
                "capabilities": ["hello","status","power","stop","logs","screenshot"],
            })),
            "status" => {
                let mut q = qmp.lock().unwrap();
                let st = q.query_status().map_err(|e| {
                    BerryError::device(format!("qmp query-status: {e}"))
                })?;
                Ok(json!({
                    "state": st.get("status").cloned().unwrap_or_else(|| json!("unknown")),
                    "run": run.display().to_string(),
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
                        q.quit_now().map_err(|e| {
                            BerryError::device(format!("qmp quit: {e}"))
                        })?;
                        stop.store(true, Ordering::Relaxed);
                        crate::run_dir::write_state(run, "stopping");
                        append_event(run, json!({"v":1,"t":"action","action":"power.off","source":"control"}));
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
                    "resolution" => dispatch("display", args, qmp, vnc, run, stop),
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

/// Client side: connect to a session's control socket and issue one request.
#[cfg(unix)]
pub fn request(run: &Path, op: &str, args: Value) -> Result<Value> {
    use std::io::{BufRead, BufReader, Write};
    let socket = run.join("control.sock");
    if !socket.exists() {
        return Err(BerryError::new(
            3,
            "no control socket (device not hosted by this shell?)",
        )
        .with_hint("start with `berry shell`, then retry; `berry shot` is one-shot"));
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
    let v: Value = serde_json::from_str(&line)
        .map_err(|e| BerryError::protocol(format!("control json: {e}")))?;
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    } else {
        let msg = v
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("control error");
        Err(BerryError::device(msg.to_string()))
    }
}

#[cfg(not(unix))]
pub fn request(_run: &Path, _op: &str, _args: Value) -> Result<Value> {
    Err(BerryError::new(2, "control plane is not wired on this platform yet"))
}
