//! One device = one QEMU child + one ScreenSource + one QMP control channel.
//! Frames come from the shared guest-RAM mirror when the kernel prints `FB ok`;
//! otherwise we fall back to VNC. Input/control stay on QMP.
//! `Drop` is the last-resort reaper; `stop` is the graceful path.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::display::frame::ScreenSource;
use crate::display::ram::{FbInfo, RamSource};
use crate::control::ControlServer;
use crate::display::vnc::{VncInput, VncSource};
use crate::error::Result;
use crate::net::Endpoint;
use crate::profile::Profile;
use crate::qemu::{self, Qemu};
use crate::qmp::QmpClient;
use crate::run_dir::{self, RunDir, Tail};
use crate::vnc_cfg;

pub struct Device {
    qemu: Qemu,
    image: std::path::PathBuf,
    firmware: std::path::PathBuf,
    vars: std::path::PathBuf,
    serial: std::path::PathBuf,
    log: std::path::PathBuf,
    ram_path: std::path::PathBuf,
    vnc: Endpoint,
    qmp_ep: Endpoint,
    accel: qemu::Accel,
    memory_mib: u32,
    mode: Option<String>,
    restart_on_crash: bool,
    child: std::process::Child,
    qmp: Arc<Mutex<QmpClient>>,
    source: Option<Box<dyn ScreenSource>>,
    run: RunDir,
    keep: bool,
    stopped: bool,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    _control: Option<ControlServer>,
    /// External VNC exposure for this run (None = not exposed). Kept on the
    /// Device so `restart_in_place` reuses the same port and re-injects the
    /// same password after QEMU is reborn.
    vnc_ext: Option<vnc_cfg::Exposed>,
}
impl Device {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        image: Option<std::path::PathBuf>,
        profile: Profile,
        accel: &str,
        memory: Option<u32>,
        display_mode: Option<String>,
        keep: bool,
        label: &str,
        vnc: vnc_cfg::Launch,
    ) -> Result<Self> {
        let qemu = Qemu::discover()?;
        let image = qemu::resolve_image(image)?;
        let firmware = qemu::firmware()?;
        let accel = qemu::choose_accel(accel, false)?;
        let mem = memory.unwrap_or(profile.memory_mib);
        // VNC exposure resolution (passed-in flags > BERRY_VNC env > off,
        // wave 11: a direct parameter, formerly the process-level launch
        // override) happens before the run dir exists so artifacts land
        // with it; invalid specs fail fast here with E_USAGE instead of
        // after QEMU spawns.
        let resolved_vnc = vnc_cfg::resolve(vnc)?;
        let run = RunDir::create(label, keep)?;
        run_dir::write_state(run.path(), "starting");
        let vars = qemu::vars_store(run.path(), 64)?;
        let serial = run.file("serial.log");
        let log = run.file("qemu.log");
        let ram_path = run.file("ram.bin");
        let (vnc, qmp_ep) = endpoints(&run);
        let mut vnc_ext = resolved_vnc.exposed;
        if let Some(ext) = &vnc_ext {
            vnc_cfg::write_listen(
                run.path(),
                &ext.host,
                ext.port,
                ext.ws_port,
                ext.tls_dir.is_some(),
            )?;
            if let Some(pw) = &ext.password {
                vnc_cfg::write_pass(run.path(), pw)?;
            }
        }
        let mode = display_mode.or_else(|| {
            (profile.id != "default").then(|| {
                format!(
                    "{}x{}@{}",
                    profile.render_w, profile.render_h, profile.refresh
                )
            })
        });
        let meta = serde_json::json!({
            "v":1,
            "label":label,
            "pid":std::process::id(),
            "started":std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            "profile":profile.id,
            "image":image.display().to_string(),
            "memory_mib":mem,
            "display_mode":mode,
            "keep":keep,
            "vnc_spec":resolved_vnc.spec_str,
            "vnc_ws":resolved_vnc.ws_str,
            "vnc_tls":vnc_ext
                .as_ref()
                .and_then(|e| e.tls_dir.as_ref())
                .map(|d| d.display().to_string())
                .unwrap_or_else(|| "off".to_string()),
            "vnc_listen":vnc_ext.as_ref()
                .map(|e| {
                    vnc_cfg::listen_value(&e.host, e.port, e.ws_port, e.tls_dir.is_some())
                })
                .unwrap_or(serde_json::Value::Null),
        });
        std::fs::write(run.file("meta.json"), serde_json::to_vec_pretty(&meta)?)?;
        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (child, qmp, source, control) = boot_with_retry(
            &qemu,
            &image,
            &firmware,
            &vars,
            &serial,
            &log,
            &vnc,
            &qmp_ep,
            accel,
            mem,
            true,
            mode.as_deref(),
            Some(&ram_path),
            &run,
            stop_flag.clone(),
            &mut vnc_ext,
        )?;
        run_dir::write_state(run.path(), "running");
        Ok(Self {
            qemu,
            image,
            firmware,
            vars,
            serial,
            log,
            ram_path,
            vnc,
            qmp_ep,
            accel,
            memory_mib: mem,
            mode,
            restart_on_crash: false,
            child,
            qmp,
            source: Some(source),
            run,
            keep,
            stopped: false,
            stop_flag,
            _control: control,
            vnc_ext,
        })
    }

    /// External VNC endpoint for this run, when exposed.
    pub fn vnc_exposed(&self) -> Option<&vnc_cfg::Exposed> {
        self.vnc_ext.as_ref()
    }

    pub fn set_restart_on_crash(&mut self, on: bool) {
        self.restart_on_crash = on;
    }

    /// Restart QEMU in the same run/control path after an unexpected exit.
    /// The control/VNC socket is recreated; CLI/attach clients must reconnect.
    pub fn restart_in_place(&mut self) -> Result<()> {
        if self.stopped {
            return Err(crate::error::BerryError::device("device already stopped"));
        }
        run_dir::write_state(self.run.path(), "rebooting");
        self._control.take();
        self.source.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        #[cfg(unix)]
        if let Endpoint::Unix(p) = &self.vnc {
            let _ = std::fs::remove_file(p);
        }
        #[cfg(unix)]
        if let Endpoint::Unix(p) = &self.qmp_ep {
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::File::create(&self.serial);
        let _ = std::fs::File::create(&self.log);
        self.stop_flag
            .store(false, std::sync::atomic::Ordering::Relaxed);
        // Same run, same external endpoint: `self.vnc_ext` keeps the resolved
        // port and password so the reborn QEMU comes back identical (the
        // password must be re-injected over QMP — see boot_runtime). The
        // &mut handoff lets a bind-race retry update the auto ports in place.
        let runtime = boot_with_retry(
            &self.qemu,
            &self.image,
            &self.firmware,
            &self.vars,
            &self.serial,
            &self.log,
            &self.vnc,
            &self.qmp_ep,
            self.accel,
            self.memory_mib,
            true,
            self.mode.as_deref(),
            Some(&self.ram_path),
            &self.run,
            self.stop_flag.clone(),
            &mut self.vnc_ext,
        );
        match runtime {
            Ok((child, qmp, source, control)) => {
                self.child = child;
                self.qmp = qmp;
                self.source = Some(source);
                self._control = control;
                run_dir::write_state(self.run.path(), "running");
                Ok(())
            }
            Err(e) => {
                self.stopped = true;
                run_dir::write_state(self.run.path(), "failed");
                Err(e)
            }
        }
    }

    /// Host loop: block until stop is requested; if `restart_on_crash` is set,
    /// recreate QEMU in the same run after an unexpected exit.
    pub fn wait_with_restart(&mut self) {
        loop {
            self.wait();
            let saved = run_dir::read_state(self.run.path()).unwrap_or_default();
            if self.stop_requested()
                || !self.restart_on_crash
                || matches!(saved.as_str(), "stopping" | "stopped" | "failed")
            {
                break;
            }
            if let Err(e) = self.restart_in_place() {
                eprintln!("berry: in-place restart failed: {e}");
                break;
            }
        }
    }

    pub fn source_mut(&mut self) -> &mut dyn ScreenSource {
        self.source
            .as_mut()
            .expect("device source is present outside restart")
            .as_mut()
    }

    pub fn reset(&mut self) -> Result<()> {
        self.qmp.lock().unwrap().system_reset()
    }

    pub fn run_path(&self) -> &std::path::Path {
        self.run.path()
    }

    pub fn stop_requested(&self) -> bool {
        self.stop_flag.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Host loop: block until control asks to stop or QEMU exits.
    pub fn wait(&mut self) {
        while !self.stop_requested() {
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Graceful toolbar OFF: ACPI power button, 5s grace, then stop.
    pub fn power_off(&mut self) {
        if self.stopped {
            return;
        }
        let _ = self.qmp.lock().unwrap().system_powerdown();
        std::thread::sleep(Duration::from_secs(5));
        self.stop();
    }

    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        run_dir::write_state(self.run.path(), "stopping");
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.qmp.lock().unwrap().quit_now();
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            if self.child.try_wait().ok().flatten().is_some() {
                run_dir::write_state(self.run.path(), "stopped");
                if self.keep {
                    self.run.keep();
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        run_dir::write_state(self.run.path(), "stopped");
        if self.keep {
            self.run.keep();
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(clippy::too_many_arguments)]
fn boot_runtime(
    qemu: &Qemu,
    image: &std::path::Path,
    firmware: &std::path::Path,
    vars: &std::path::Path,
    serial: &std::path::Path,
    log: &std::path::Path,
    vnc: &Endpoint,
    qmp_ep: &Endpoint,
    accel: qemu::Accel,
    mem: u32,
    tablet: bool,
    mode: Option<&str>,
    ram_path: Option<&std::path::Path>,
    run: &RunDir,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    ext: Option<&vnc_cfg::Exposed>,
) -> Result<(
    std::process::Child,
    Arc<Mutex<QmpClient>>,
    Box<dyn ScreenSource>,
    Option<ControlServer>,
)> {
    let ext_arg = match ext {
        Some(e) => Some(e.qemu_arg()?),
        None => None,
    };
    let tls_creds = ext.and_then(|e| e.tls_dir.as_deref());
    let mut child = qemu::spawn_image(
        qemu, image, firmware, vars, serial, log, vnc, qmp_ep, accel, mem, tablet, mode, ram_path,
        ext_arg.as_deref(),
        tls_creds,
    )?;
    let meta_path = run.file("meta.json");
    let mut meta: serde_json::Value = std::fs::read(&meta_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    meta["qemu_pid"] = serde_json::json!(child.id());
    let _ = std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?);
    let mut tail = Tail::open(serial);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut fb = None;
    while Instant::now() < deadline {
        tail.read_new();
        if let Some(line) = tail.text().lines().find(|l| l.contains("FB ok ")) {
            if let Some(i) = FbInfo::parse(line) {
                fb = Some(i);
                break;
            }
        }
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let result = (|| {
        let mut qmp = connect_qmp(qmp_ep, Duration::from_secs(10))?;
        // Event pump (wave 2): map the three VNC client events onto the
        // run's events.jsonl (same writer as control actions). Non-VNC
        // events stay silent — wave-1 semantics for everything else.
        {
            let run_events = run.path().to_path_buf();
            qmp.set_event_sink(Arc::new(move |name, v| {
                if let Some(line) = vnc_event_line(name, v) {
                    crate::control::append_event(&run_events, line);
                }
            }));
        }
        // Re-inject the VNC password on every (re)spawn: QEMU restarts lose
        // it, and an auth-enabled endpoint without a password rejects every
        // client. The error text carries only the QMP desc, never the value.
        if let Some(pw) = ext.and_then(|e| e.password.as_deref()) {
            qmp.set_vnc_password(pw).map_err(|e| {
                crate::error::BerryError::device(format!("inject VNC password: {e}"))
            })?;
        }
        let qmp = Arc::new(Mutex::new(qmp));
        let source: Box<dyn ScreenSource> = if let Some(info) = fb {
            let input = VncInput::connect(vnc)?;
            Box::new(RamSource::open(ram_path.unwrap_or(std::path::Path::new("")), info, input)?)
        } else {
            wait_endpoint(vnc, Duration::from_secs(5))?;
            Box::new(VncSource::connect(vnc)?)
        };
        let control = ControlServer::start(run.path(), qmp.clone(), vnc.clone(), stop_flag)?;
        Ok::<_, crate::error::BerryError>((source, qmp, control))
    })();
    match result {
        Ok((source, qmp, control)) => Ok((child, qmp, source, control)),
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            run_dir::write_state(run.path(), "failed");
            // Surface QEMU's own words (bind races on pre-picked VNC ports,
            // bad fw_cfg args, …) — argv never contains a password, so the
            // log tail is safe to attach.
            let log_text = std::fs::read_to_string(log).unwrap_or_default();
            let tail: Vec<&str> = log_text
                .lines()
                .rev()
                .take(12)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if !tail.is_empty() {
                return Err(e.with_hint(format!("qemu.log tail:\n{}", tail.join("\n"))));
            }
            Err(e)
        }
    }
}

fn connect_qmp(ep: &Endpoint, timeout: Duration) -> Result<QmpClient> {
    let start = Instant::now();
    loop {
        match QmpClient::connect(ep) {
            Ok(q) => return Ok(q),
            Err(e) if start.elapsed() < timeout => {
                let _ = e;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e),
        }
    }
}

/// `boot_runtime` with auto-port bind-race recovery (wave 2): when the first
/// attempt died on a `-vnc` bind conflict and either side of the endpoint was
/// auto-picked, re-roll only the auto sides (host, fixed ports, password and
/// TLS dir untouched), rewrite `vnc.listen` + meta, and try exactly once
/// more. Fixed ports and unrelated failures fall through with the original
/// error (the qemu.log tail hint from `boot_runtime` stays attached).
#[allow(clippy::too_many_arguments)]
fn boot_with_retry(
    qemu: &Qemu,
    image: &std::path::Path,
    firmware: &std::path::Path,
    vars: &std::path::Path,
    serial: &std::path::Path,
    log: &std::path::Path,
    vnc: &Endpoint,
    qmp_ep: &Endpoint,
    accel: qemu::Accel,
    mem: u32,
    tablet: bool,
    mode: Option<&str>,
    ram_path: Option<&std::path::Path>,
    run: &RunDir,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    ext: &mut Option<vnc_cfg::Exposed>,
) -> Result<(
    std::process::Child,
    Arc<Mutex<QmpClient>>,
    Box<dyn ScreenSource>,
    Option<ControlServer>,
)> {
    let first = boot_runtime(
        qemu, image, firmware, vars, serial, log, vnc, qmp_ep, accel, mem, tablet, mode, ram_path,
        run, stop_flag.clone(), ext.as_ref(),
    );
    if first.is_ok() {
        return first;
    }
    let log_text = std::fs::read_to_string(log).unwrap_or_default();
    if !should_retry_bind_race(&log_text, ext.as_ref()) {
        return first;
    }
    let Some(e) = ext.as_mut() else {
        return first;
    };
    e.rebuild_auto_ports();
    vnc_cfg::write_listen(run.path(), &e.host, e.port, e.ws_port, e.tls_dir.is_some())?;
    let meta_path = run.file("meta.json");
    if let Ok(b) = std::fs::read(&meta_path) {
        if let Ok(mut meta) = serde_json::from_slice::<serde_json::Value>(&b) {
            meta["vnc_listen"] =
                vnc_cfg::listen_value(&e.host, e.port, e.ws_port, e.tls_dir.is_some());
            let _ = std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?);
        }
    }
    // Fresh log so a second failure shows the retry's own tail, not the
    // first attempt's.
    let _ = std::fs::File::create(log);
    boot_runtime(
        qemu, image, firmware, vars, serial, log, vnc, qmp_ep, accel, mem, tablet, mode, ram_path,
        run, stop_flag, ext.as_ref(),
    )
}

/// Map a QMP VNC event onto its events.jsonl line; None for non-VNC events.
/// Field shapes per probe p5: `data.client`/`data.server` pass through
/// verbatim (the client object is present on disconnects too — the client
/// port distinguishes which client left).
pub(crate) fn vnc_event_line(event: &str, v: &serde_json::Value) -> Option<serde_json::Value> {
    let t = match event {
        "VNC_CONNECTED" => "vnc.connected",
        "VNC_INITIALIZED" => "vnc.initialized",
        "VNC_DISCONNECTED" => "vnc.disconnected",
        _ => return None,
    };
    let data = v.get("data");
    Some(serde_json::json!({
        "v": 1,
        "t": t,
        "source": "qmp",
        "client": data.and_then(|d| d.get("client")).cloned().unwrap_or(serde_json::Value::Null),
        "server": data.and_then(|d| d.get("server")).cloned().unwrap_or(serde_json::Value::Null),
    }))
}

/// Does this qemu.log text look like a `-vnc` bind race? Calibrated against
/// QEMU 11.0.3 output (both the TCP display and the websocket side print
/// `… -vnc <spec>: Failed to find an available port: Address already in use`);
/// the older `Failed to bind` / `Failed to start VNC` spellings are kept as
/// secondary patterns. A bind failure elsewhere (QMP socket) must not match,
/// so each line has to be both VNC-related and bind-related.
pub(crate) fn is_vnc_bind_race(log: &str) -> bool {
    log.lines().any(|l| {
        let vncish = l.contains("-vnc") || l.to_ascii_lowercase().contains("vnc");
        let bindish = l.contains("Address already in use")
            || l.contains("Failed to bind")
            || l.contains("Failed to find an available port")
            || l.contains("Failed to start VNC");
        vncish && bindish
    })
}

/// Retry only when the endpoint has at least one auto-picked side; fixed
/// ports must fail loudly (nothing a silent re-pick could improve).
pub(crate) fn should_retry_bind_race(log: &str, ext: Option<&vnc_cfg::Exposed>) -> bool {
    is_vnc_bind_race(log) && ext.is_some_and(|e| e.port_auto || e.ws_auto)
}

fn endpoints(run: &RunDir) -> (Endpoint, Endpoint) {
    #[cfg(unix)]
    {
        let vnc = run.file("vnc.sock");
        let qmp = run.file("qmp.sock");
        let _ = std::fs::remove_file(&vnc);
        let _ = std::fs::remove_file(&qmp);
        (Endpoint::Unix(vnc), Endpoint::Unix(qmp))
    }
    #[cfg(not(unix))]
    {
        let vnc_port = free_port();
        let qmp_port = free_port();
        let _ = std::fs::write(run.file("vnc.port"), vnc_port.to_string());
        let _ = std::fs::write(run.file("qmp.port"), qmp_port.to_string());
        (
            Endpoint::Tcp(format!("127.0.0.1:{vnc_port}")),
            Endpoint::Tcp(format!("127.0.0.1:{qmp_port}")),
        )
    }
}

#[cfg(not(unix))]
fn free_port() -> u16 {
    // Internal -vnc endpoints also use display-number syntax on QEMU, so the
    // port must be >= 5900 to be expressible (probe f2).
    vnc_cfg::free_vnc_port()
}

fn wait_endpoint(ep: &Endpoint, timeout: Duration) -> Result<()> {
    #[cfg(unix)]
    {
        if let Endpoint::Unix(p) = ep {
            return run_dir::wait_for_file(p, timeout);
        }
    }
    let _ = (ep, timeout);
    std::thread::sleep(Duration::from_millis(100));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONNECTED: &str = r#"{"timestamp":{"seconds":1794538787,"microseconds":68437},"event":"VNC_CONNECTED","data":{"server":{"auth":"none","family":"ipv4","service":"11801","host":"127.0.0.1","websocket":false},"client":{"family":"ipv4","service":"61086","host":"127.0.0.1","websocket":false}}}"#;

    #[test]
    fn vnc_event_mapping() {
        // Three VNC events map to their t names with fields passed through.
        for (ev, t) in [
            ("VNC_CONNECTED", "vnc.connected"),
            ("VNC_INITIALIZED", "vnc.initialized"),
            ("VNC_DISCONNECTED", "vnc.disconnected"),
        ] {
            let raw = CONNECTED.replace("VNC_CONNECTED", ev);
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let line = vnc_event_line(ev, &v).unwrap();
            assert_eq!(line["t"], t);
            assert_eq!(line["v"], 1);
            assert_eq!(line["source"], "qmp");
            assert_eq!(line["client"]["service"], "61086");
            assert_eq!(line["client"]["host"], "127.0.0.1");
            assert_eq!(line["client"]["websocket"], false);
            assert_eq!(line["server"]["service"], "11801");
            assert_eq!(line["server"]["auth"], "none");
            // Round-trip through events.jsonl formatting stays one line.
            let s = serde_json::to_string(&line).unwrap();
            assert!(!s.contains('\n'));
        }
        // Disconnect keeps the client object (probe p5: identical shapes).
        let raw = r#"{"event":"VNC_DISCONNECTED","data":{"server":{"auth":"none","service":"11801"},"client":{"host":"127.0.0.1","service":"60628","websocket":true}}}"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let line = vnc_event_line("VNC_DISCONNECTED", &v).unwrap();
        assert_eq!(line["t"], "vnc.disconnected");
        assert_eq!(line["client"]["websocket"], true);
        // Non-VNC events map to None (stay silent).
        let v: serde_json::Value =
            serde_json::from_str(r#"{"event":"STOP","data":{}}"#).unwrap();
        assert!(vnc_event_line("STOP", &v).is_none());
        // Missing data still yields a line with null client/server.
        let v: serde_json::Value =
            serde_json::from_str(r#"{"event":"VNC_CONNECTED"}"#).unwrap();
        let line = vnc_event_line("VNC_CONNECTED", &v).unwrap();
        assert!(line["client"].is_null());
    }

    #[test]
    fn bind_race_detection() {
        // QEMU 11.0.3 TCP display and websocket failures (calibrated live).
        let tcp = "qemu-system-aarch64: -vnc 127.0.0.1:71: Failed to find an available port: Address already in use\n";
        let ws = "qemu-system-aarch64: -vnc 127.0.0.1:71,websocket=6001: Failed to find an available port: Address already in use\n";
        assert!(is_vnc_bind_race(tcp));
        assert!(is_vnc_bind_race(ws));
        // Unrelated failures must not match: no VNC mention, no bind mention.
        assert!(!is_vnc_bind_race("qemu: could not load kernel\n"));
        assert!(!is_vnc_bind_race("qemu-system-aarch64: -qmp unix:/run/qmp.sock: Failed to bind\n"));
        // Retry decision: bind race + at least one auto side.
        let mut auto = vnc_cfg::Exposed {
            host: "127.0.0.1".into(),
            port: 5971,
            ws_port: Some(6001),
            password: None,
            tls_dir: None,
            port_auto: true,
            ws_auto: false,
        };
        assert!(should_retry_bind_race(tcp, Some(&auto)));
        auto.port_auto = false;
        assert!(!should_retry_bind_race(tcp, Some(&auto)));
        auto.ws_auto = true;
        assert!(should_retry_bind_race(ws, Some(&auto)));
        // Fixed everything: never retry, fail loudly instead.
        let fixed = vnc_cfg::Exposed {
            host: "127.0.0.1".into(),
            port: 5971,
            ws_port: Some(6001),
            password: None,
            tls_dir: None,
            port_auto: false,
            ws_auto: false,
        };
        assert!(!should_retry_bind_race(tcp, Some(&fixed)));
        assert!(!should_retry_bind_race("unrelated boot failure", Some(&auto)));
        assert!(!should_retry_bind_race(tcp, None));
    }
}
