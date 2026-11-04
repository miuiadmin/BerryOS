//! Command handlers. All QEMU interaction is argv-based; no shell strings.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::display::frame::ScreenSource;
use crate::display::vnc::{VncInput, VncSource};
use crate::error::{BerryError, Result};
use crate::net::Endpoint;
use crate::profile;
use crate::qemu::{self, Qemu};
use crate::qmp::QmpClient;
use crate::run_dir::{self, RunDir, Tail};

pub struct RunArgs {
    pub kernel: Option<PathBuf>,
    pub marker: String,
    pub timeout: Duration,
    pub accel: String,
    pub memory: Option<u32>,
    pub follow: bool,
    pub keep: bool,
    pub json: bool,
}

pub struct ShotArgs {
    pub image: Option<PathBuf>,
    pub marker: Option<String>,
    pub delay: Duration,
    pub out: Option<PathBuf>,
    pub profile: String,
    pub accel: String,
    pub memory: Option<u32>,
    pub timeout: Duration,
    pub keep: bool,
    pub display_mode: Option<String>,
    pub json: bool,
}

pub struct ScreenshotArgs {
    pub run: Option<String>,
    pub out: Option<PathBuf>,
    pub settle: Duration,
    pub timeout: Duration,
    pub json: bool,
}

pub struct ShellArgs {
    pub image: Option<PathBuf>,
    pub profile: String,
    pub scale: ScaleMode,
    pub accel: String,
    pub memory: Option<u32>,
    pub timeout: Duration,
    pub keep: bool,
    pub display_mode: Option<String>,
    pub json: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum ScaleMode {
    Fit,
    Integer(u32),
}

pub enum InputAction {
    Key(String),
    Tap(u16, u16),
    Swipe { x1: u16, y1: u16, x2: u16, y2: u16, ms: u64 },
}

pub struct InputArgs {
    pub run: Option<String>,
    pub action: InputAction,
    pub json: bool,
}

pub fn version(as_json: bool) {
    if as_json {
        println!("{}", json!({"v":1,"t":"version","version":env!("CARGO_PKG_VERSION")}));
    } else {
        println!("berry {}", env!("CARGO_PKG_VERSION"));
    }
}

pub fn run(args: RunArgs) -> Result<()> {
    let qemu = Qemu::discover()?;
    let kernel = qemu::resolve_kernel(args.kernel)?;
    let accel = qemu::choose_accel(&args.accel, false)?;
    let memory = args.memory.unwrap_or(profile::default_profile().memory_mib);
    let mut run = RunDir::create("run", args.keep)?;
    let serial = run.file("serial.log");
    let log = run.file("qemu.log");
    if args.json {
        println!(
            "{}",
            json!({"v":1,"t":"run.start","run":run.path(),"kernel":kernel,"accel":accel.as_arg(),"memory_mib":memory})
        );
    }
    let mut child = qemu::spawn_kernel(&qemu, &kernel, &serial, &log, accel, memory)?;
    let mut tail = Tail::open(&serial);
    let deadline = Instant::now() + args.timeout;
    let mut stdout = std::io::stdout();
    loop {
        let new = tail.read_new();
        if !new.is_empty() && args.follow && !args.json {
            stdout.write_all(&new).ok();
            stdout.flush().ok();
        }
        if !args.marker.is_empty() && tail.contains(&args.marker) {
            if args.json {
                println!(
                    "{}",
                    json!({"v":1,"t":"marker","name":args.marker,"run":run.path()})
                );
            } else {
                println!("marker ok: {}", args.marker);
            }
            stop_child(&mut child, None);
            if args.keep {
                run.keep();
            }
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            let text = tail.tail_text(4000);
            stop_child(&mut child, None);
            return Err(BerryError::device(format!(
                "qemu exited ({status}) before marker '{}'",
                args.marker
            ))
            .with_hint(format!("serial tail:\n{text}")));
        }
        if Instant::now() >= deadline {
            let text = tail.tail_text(4000);
            stop_child(&mut child, None);
            return Err(BerryError::timeout(format!(
                "timed out after {}s waiting for '{}'",
                args.timeout.as_secs(),
                args.marker
            ))
            .with_hint(format!("serial tail:\n{text}")));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn shot(args: ShotArgs) -> Result<()> {
    let qemu = Qemu::discover()?;
    let image = qemu::resolve_image(args.image)?;
    let firmware = qemu::firmware()?;
    let profile = profile::get(&args.profile)?;
    let accel = qemu::choose_accel(&args.accel, true)?;
    let memory = args.memory.unwrap_or(profile.memory_mib);
    let mode = args.display_mode.clone().or_else(|| {
        (profile.id != "default").then(|| {
            format!(
                "{}x{}@{}",
                profile.render_w, profile.render_h, profile.refresh
            )
        })
    });
    let mut run = RunDir::create("shot", args.keep)?;
    let vars = run.file("vars.fd");
    qemu::create_vars(&vars, 64)?;
    let serial = run.file("serial.log");
    let log = run.file("qemu.log");
    let (vnc, qmp_ep) = endpoints(&run);
    if args.json {
        println!(
            "{}",
            json!({"v":1,"t":"shot.start","run":run.path(),"image":image,"profile":profile.id})
        );
    }
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
        memory,
        true,
        mode.as_deref(),
        None,
    )?;
    wait_endpoint(&vnc, Duration::from_secs(10))?;
    let mut source = VncSource::connect(&vnc)?;
    let mut tail = Tail::open(&serial);
    let deadline = Instant::now() + args.timeout;
    if let Some(marker) = &args.marker {
        loop {
            tail.read_new();
            if tail.contains(marker) {
                break;
            }
            if let Some(status) = child.try_wait()? {
                stop_child(&mut child, None);
                return Err(BerryError::device(format!(
                    "qemu exited ({status}) before marker '{marker}'"
                ))
                .with_hint(format!("serial tail:\n{}", tail.tail_text(4000))));
            }
            if Instant::now() >= deadline {
                stop_child(&mut child, None);
                return Err(BerryError::timeout(format!(
                    "timed out waiting for marker '{marker}'"
                ))
                .with_hint(format!("serial tail:\n{}", tail.tail_text(4000))));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    if !args.delay.is_zero() {
        std::thread::sleep(args.delay);
    }
    let frame = source.next_frame(Duration::from_secs(10)).ok_or_else(|| {
        let err = source.error().unwrap_or_else(|| "no frame".to_string());
        BerryError::protocol(format!("VNC: {err}"))
            .with_hint(format!("serial tail:\n{}", tail.tail_text(4000)))
    })?;
    let out = args.out.unwrap_or_else(|| PathBuf::from("shot.png"));
    crate::png::write_png(&out, frame.width, frame.height, &frame.to_rgb())?;
    let mut qmp = QmpClient::connect(&qmp_ep).ok();
    stop_child(&mut child, qmp.as_mut());
    if args.keep {
        run.keep();
    }
    if args.json {
        println!(
            "{}",
            json!({"v":1,"t":"shot","path":out,"width":frame.width,"height":frame.height,"run":run.path()})
        );
    } else {
        println!("shot: {} ({}x{})", out.display(), frame.width, frame.height);
    }
    Ok(())
}

/// Capture one frame from an already-running device (CLI-first attach).
/// Does not spawn QEMU; VNC is multi-client so this coexists with `berry shell`.
pub fn screenshot(args: ScreenshotArgs) -> Result<()> {
    let run = find_run(args.run.as_deref())?;
    let endpoint = vnc_endpoint(&run)?;
    let mut source = VncSource::connect(&endpoint)?;
    if !args.settle.is_zero() {
        std::thread::sleep(args.settle);
    }
    let frame = source.next_frame(args.timeout).ok_or_else(|| {
        let err = source.error().unwrap_or_else(|| "no frame".to_string());
        BerryError::protocol(format!("VNC: {err}"))
            .with_hint("is the device still running? try `berry list`")
    })?;
    let out = args.out.unwrap_or_else(|| PathBuf::from("screenshot.png"));
    crate::png::write_png(&out, frame.width, frame.height, &frame.to_rgb())?;
    if args.json {
        println!(
            "{}",
            json!({"v":1,"t":"screenshot","path":out,"width":frame.width,"height":frame.height,"run":run})
        );
    } else {
        println!("screenshot: {} ({}x{})", out.display(), frame.width, frame.height);
    }
    Ok(())
}

pub fn shell(args: ShellArgs) -> Result<()> {
    crate::window::run(args)
}

pub fn input(args: InputArgs) -> Result<()> {
    let run = find_run(args.run.as_deref())?;
    let endpoint = vnc_endpoint(&run)?;
    let mut vnc = VncInput::connect(&endpoint)?;
    let (w, h) = vnc.size();
    let clamp = |x: u16, y: u16| -> (u16, u16) {
        (x.min(w.saturating_sub(1) as u16), y.min(h.saturating_sub(1) as u16))
    };
    match args.action {
        InputAction::Key(code) => {
            let ks = qcode_to_keysym(&code).ok_or_else(|| {
                BerryError::new(2, format!("unknown key '{code}'"))
                    .with_hint("qcode: ret/esc/tab/backspace/spc/left/up/right/down or one ASCII char")
            })?;
            vnc.key(ks, true)?;
            vnc.key(ks, false)?;
        }
        InputAction::Tap(x, y) => {
            let (x, y) = clamp(x, y);
            vnc.pointer(x, y, 1)?;
            std::thread::sleep(Duration::from_millis(40));
            vnc.pointer(x, y, 0)?;
        }
        InputAction::Swipe { x1, y1, x2, y2, ms } => {
            let (x1, y1) = clamp(x1, y1);
            let (x2, y2) = clamp(x2, y2);
            let steps = (ms / 16).max(2);
            vnc.pointer(x1, y1, 1)?;
            for i in 1..=steps {
                let t = i as f64 / steps as f64;
                let x = (x1 as f64 + (x2 as f64 - x1 as f64) * t) as u16;
                let y = (y1 as f64 + (y2 as f64 - y1 as f64) * t) as u16;
                vnc.pointer(x, y, 1)?;
                std::thread::sleep(Duration::from_millis(16));
            }
            vnc.pointer(x2, y2, 0)?;
        }
    }
    if args.json {
        println!(
            "{}",
            json!({"v":1,"t":"input","run":run,"ok":true,"guest":[w,h]})
        );
    } else {
        println!("input: ok ({}, guest {w}x{h})", run.display());
    }
    Ok(())
}

/// QEMU qcode -> X11 keysym (the VNC input channel speaks keysyms).
fn qcode_to_keysym(q: &str) -> Option<u32> {
    Some(match q {
        "ret" | "enter" => 0xff0d,
        "esc" => 0xff1b,
        "backspace" => 0xff08,
        "tab" => 0xff09,
        "spc" | "space" => 0x20,
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "delete" => 0xffff,
        "home" => 0xff50,
        "end" => 0xff57,
        "minus" => 0x2d,
        "equal" => 0x3d,
        "comma" => 0x2c,
        "dot" => 0x2e,
        "slash" => 0x2f,
        "semicolon" => 0x3b,
        "apostrophe" => 0x27,
        "bracket_left" => 0x5b,
        "bracket_right" => 0x5d,
        "backslash" => 0x5c,
        "grave_accent" => 0x60,
        _ if q.chars().count() == 1 => q.chars().next()? as u32,
        _ => return None,
    })
}

fn find_run(id: Option<&str>) -> Result<PathBuf> {
    if let Some(id) = id {
        let p = if Path::new(id).is_dir() {
            PathBuf::from(id)
        } else {
            run_dir::home().join("run").join(id)
        };
        if p.is_dir() {
            return Ok(p);
        }
        return Err(BerryError::env(format!("run not found: {}", p.display())));
    }
    let root = run_dir::home().join("run");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(rd) = std::fs::read_dir(&root) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() || !vnc_endpoint(&p).is_ok() {
                continue;
            }
            let t = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(bt, _)| t > *bt) {
                best = Some((t, p));
            }
        }
    }
    best.map(|(_, p)| p)
        .ok_or_else(|| BerryError::env("no running device").with_hint("start one with `berry shell`"))
}

fn vnc_endpoint(run: &Path) -> Result<Endpoint> {
    #[cfg(unix)]
    {
        let p = run.join("vnc.sock");
        if p.exists() {
            return Ok(Endpoint::Unix(p));
        }
        let port = std::fs::read_to_string(run.join("vnc.port")).ok();
        if let Some(port) = port {
            return Ok(Endpoint::Tcp(format!("127.0.0.1:{}", port.trim())));
        }
    }
    #[cfg(not(unix))]
    {
        let port = std::fs::read_to_string(run.join("vnc.port"))?;
        return Ok(Endpoint::Tcp(format!("127.0.0.1:{}", port.trim())));
    }
    Err(BerryError::env(format!(
        "no VNC endpoint in {}",
        run.display()
    )))
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

fn stop_child(child: &mut Child, qmp: Option<&mut QmpClient>) {
    if let Some(q) = qmp {
        let _ = q.quit_now();
    } else {
        let _ = child.kill();
        let _ = child.wait();
        return;
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}
