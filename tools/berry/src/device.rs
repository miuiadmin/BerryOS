//! One device = one QEMU child + one VNC source + one QMP control channel.
//! `Drop` is the last-resort reaper; `stop` is the graceful path.

use std::process::Child;
use std::time::{Duration, Instant};

use crate::display::vnc::VncSource;
use crate::error::Result;
use crate::net::Endpoint;
use crate::profile::Profile;
use crate::qemu::{self, Qemu};
use crate::qmp::QmpClient;
use crate::run_dir::{self, RunDir};

pub struct Device {
    child: Child,
    qmp: Endpoint,
    source: VncSource,
    run: RunDir,
    keep: bool,
    stopped: bool,
}

impl Device {
    pub fn start(
        image: Option<std::path::PathBuf>,
        profile: Profile,
        accel: &str,
        memory: Option<u32>,
        display_mode: Option<String>,
        keep: bool,
    ) -> Result<Self> {
        let qemu = Qemu::discover()?;
        let image = qemu::resolve_image(image)?;
        let firmware = qemu::firmware()?;
        let accel = qemu::choose_accel(accel, false)?;
        let mem = memory.unwrap_or(profile.memory_mib);
        let run = RunDir::create("shell", keep)?;
        let vars = run.file("vars.fd");
        qemu::create_vars(&vars, 64)?;
        let serial = run.file("serial.log");
        let log = run.file("qemu.log");
        let (vnc, qmp_ep) = endpoints(&run);
        let mut child = qemu::spawn_image(
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
            display_mode.as_deref(),
        )?;
        let result = (|| {
            wait_endpoint(&vnc, Duration::from_secs(10))?;
            VncSource::connect(&vnc)
        })();
        match result {
            Ok(source) => Ok(Self {
                child,
                qmp: qmp_ep,
                source,
                run,
                keep,
                stopped: false,
            }),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(e)
            }
        }
    }

    pub fn source_mut(&mut self) -> &mut VncSource {
        &mut self.source
    }

    pub fn reset(&mut self) -> Result<()> {
        let mut qmp = QmpClient::connect(&self.qmp)?;
        qmp.system_reset()
    }

    pub fn run_path(&self) -> &std::path::Path {
        self.run.path()
    }

    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        if let Ok(mut qmp) = QmpClient::connect(&self.qmp) {
            let _ = qmp.quit_now();
        }
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            if self.child.try_wait().ok().flatten().is_some() {
                if self.keep {
                    self.run.keep();
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
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
