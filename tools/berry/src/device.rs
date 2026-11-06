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
}
impl Device {
    pub fn start(
        image: Option<std::path::PathBuf>,
        profile: Profile,
        accel: &str,
        memory: Option<u32>,
        display_mode: Option<String>,
        keep: bool,
        label: &str,
    ) -> Result<Self> {
        let qemu = Qemu::discover()?;
        let image = qemu::resolve_image(image)?;
        let firmware = qemu::firmware()?;
        let accel = qemu::choose_accel(accel, false)?;
        let mem = memory.unwrap_or(profile.memory_mib);
        let run = RunDir::create(label, keep)?;
        run_dir::write_state(run.path(), "starting");
        let vars = run.file("vars.fd");
        qemu::create_vars(&vars, 64)?;
        let serial = run.file("serial.log");
        let log = run.file("qemu.log");
        let ram_path = run.file("ram.bin");
        let (vnc, qmp_ep) = endpoints(&run);
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
        });
        std::fs::write(run.file("meta.json"), serde_json::to_vec_pretty(&meta)?)?;
        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (child, qmp, source, control) = boot_runtime(
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
        })
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
        let runtime = boot_runtime(
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
) -> Result<(
    std::process::Child,
    Arc<Mutex<QmpClient>>,
    Box<dyn ScreenSource>,
    Option<ControlServer>,
)> {
    let mut child = qemu::spawn_image(
        qemu, image, firmware, vars, serial, log, vnc, qmp_ep, accel, mem, tablet, mode, ram_path,
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
        let qmp = connect_qmp(qmp_ep, Duration::from_secs(10))?;
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
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(5900)
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
