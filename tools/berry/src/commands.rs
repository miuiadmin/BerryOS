//! Command handlers. All QEMU interaction is argv-based; no shell strings.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::control;
use crate::display::frame::ScreenSource;
use crate::display::vnc::{VncInput, VncSource};
use crate::error::{BerryError, Result};
use crate::net::Endpoint;
use crate::profile;
use crate::qemu::{self, Qemu};
use crate::qmp::QmpClient;
use crate::run_dir::{self, RunDir, Tail};
use crate::vnc_cfg;

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

pub struct HostArgs {
    pub image: Option<PathBuf>,
    pub profile: String,
    pub accel: String,
    pub memory: Option<u32>,
    pub display_mode: Option<String>,
    pub keep: bool,
    pub restart_on_crash: bool,
    pub vnc_spec: Option<String>,
    pub vnc_ws: Option<String>,
    pub vnc_pass: Option<String>,
    pub vnc_pass_file: Option<PathBuf>,
    pub vnc_tls: Option<String>,
}

pub struct DeviceStartArgs {
    pub image: Option<PathBuf>,
    pub profile: String,
    pub accel: String,
    pub memory: Option<u32>,
    pub display_mode: Option<String>,
    pub keep: bool,
    pub foreground: bool,
    pub restart_on_crash: bool,
    pub json: bool,
    pub vnc_spec: Option<String>,
    pub vnc_ws: Option<String>,
    pub vnc_pass: Option<String>,
    pub vnc_pass_file: Option<PathBuf>,
    pub vnc_tls: Option<String>,
}

pub struct ShellArgs {
    pub attach: Option<String>,
    pub image: Option<PathBuf>,
    pub profile: String,
    pub scale: ScaleMode,
    pub accel: String,
    pub memory: Option<u32>,
    pub timeout: Duration,
    pub keep: bool,
    pub display_mode: Option<String>,
    pub json: bool,
    pub vnc_spec: Option<String>,
    pub vnc_ws: Option<String>,
    pub vnc_pass: Option<String>,
    pub vnc_tls: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum ScaleMode {
    Fit,
    Integer(u32),
}

pub enum InputAction {
    Key(String),
    Text(String),
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
        None,
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
    let out = args.out.unwrap_or_else(|| PathBuf::from("screenshot.png"));
    let out_abs = if out.is_absolute() {
        out.clone()
    } else {
        std::env::current_dir()
            .map(|d| d.join(&out))
            .unwrap_or_else(|_| out.clone())
    };
    // C1b: prefer the session host (works for any ScreenSource; no second VNC).
    if control::is_up(&run) {
        if let Ok(v) = control::request(
            &run,
            "screenshot",
            json!({"out": out_abs.display().to_string(), "settle_ms": args.settle.as_millis() as u64}),
        ) {
            if args.json {
                println!("{}", json!({"v":1,"t":"screenshot","run":run,"result":v}));
            } else {
                println!(
                    "screenshot: {} ({}x{})",
                    v.get("path").and_then(|p| p.as_str()).unwrap_or("-"),
                    v.get("width").and_then(|w| w.as_u64()).unwrap_or(0),
                    v.get("height").and_then(|h| h.as_u64()).unwrap_or(0),
                );
            }
            return Ok(());
        }
    }
    let endpoint = vnc_endpoint(&run)?;
    let mut source = VncSource::connect(&endpoint)?;
    if !args.settle.is_zero() {
        std::thread::sleep(args.settle);
    }
    let frame = source.next_frame(args.timeout).ok_or_else(|| {
        let err = source.error().unwrap_or_else(|| "no frame".to_string());
        BerryError::protocol(format!("VNC: {err}"))
            .with_hint("is the device still running? try `berry device list`")
    })?;
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
    // Fail fast with the real spec error before the window opens.
    vnc_cfg::validate_flags(args.vnc_spec.as_deref(), args.vnc_ws.as_deref(), args.vnc_tls.as_deref())?;
    // window.rs is frozen by a parallel session and calls Device::start with
    // positional args only; hand the VNC flags over via the process-level
    // launch override instead (BERRY_VNC env remains the fallback below it).
    vnc_cfg::set_launch(vnc_cfg::Launch {
        spec: args.vnc_spec.clone(),
        ws: args.vnc_ws.clone(),
        pass: args.vnc_pass.clone(),
        pass_file: None,
        tls: args.vnc_tls.clone(),
    });
    vnc_cfg::maybe_security_note(args.vnc_spec.as_deref());
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
        InputAction::Text(text) => {
            for ch in text.chars() {
                vnc.key(ch as u32, true)?;
                vnc.key(ch as u32, false)?;
            }
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
pub(crate) fn qcode_to_keysym(q: &str) -> Option<u32> {
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

/// Headless session host (hidden `berry host` role). Prints the run id, then
/// blocks until control asks to stop or QEMU exits.
pub fn host(args: HostArgs) -> Result<()> {
    let profile = profile::get(&args.profile)?;
    // Fail fast with the real spec error instead of a bare exit.
    vnc_cfg::validate_flags(args.vnc_spec.as_deref(), args.vnc_ws.as_deref(), args.vnc_tls.as_deref())?;
    vnc_cfg::set_launch(vnc_cfg::Launch {
        spec: args.vnc_spec.clone(),
        ws: args.vnc_ws.clone(),
        pass: args.vnc_pass.clone(),
        pass_file: args.vnc_pass_file.clone(),
        tls: args.vnc_tls.clone(),
    });
    let mut device = crate::device::Device::start(
        args.image,
        profile,
        &args.accel,
        args.memory,
        args.display_mode,
        args.keep,
        "host",
    )?;
    if let Some(ext) = device.vnc_exposed() {
        vnc_cfg::print_security_note(ext);
    }
    device.set_restart_on_crash(args.restart_on_crash);
    println!("RUN {}", device.run_path().display());
    std::io::stdout().flush().ok();
    device.wait_with_restart();
    device.stop();
    Ok(())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn expect_step(run: &Path, args: &serde_json::Value, step: usize) -> Result<()> {
    let mut checked = false;
    if let Some(want) = args.get("state").and_then(|v| v.as_str()) {
        checked = true;
        let v = control::request(run, "status", json!({}))?;
        let got = v.get("state").and_then(|s| s.as_str()).unwrap_or("?");
        if got != want {
            return Err(BerryError::device(format!(
                "expect step {step}: state {got:?} != {want:?}"
            )));
        }
    }
    if let Some(path) = args.get("file").and_then(|v| v.as_str()) {
        checked = true;
        let data = std::fs::read(path)
            .map_err(|e| BerryError::device(format!("expect step {step}: read {path}: {e}")))?;
        if let Some(min) = args.get("min_bytes").and_then(|v| v.as_u64()) {
            if data.len() < min as usize {
                return Err(BerryError::device(format!(
                    "expect step {step}: {path} {} bytes < {min}",
                    data.len()
                )));
            }
        }
        if let Some(want) = args.get("fnv1a64").and_then(|v| v.as_str()) {
            let want = want
                .trim_start_matches("0x")
                .parse::<u64>()
                .map_err(|_| BerryError::new(2, format!("expect step {step}: bad fnv1a64")))?;
            let got = fnv1a64(&data);
            if got != want {
                return Err(BerryError::device(format!(
                    "expect step {step}: fnv1a64 {got:016x} != {want:016x}"
                )));
            }
        }
    }
    if !checked {
        return Err(BerryError::new(
            2,
            format!("expect step {step}: needs state or file"),
        ));
    }
    Ok(())
}

pub fn play(script: &Path, run: Option<&str>, json: bool) -> Result<()> {
    let run = find_run(run)?;
    let raw = std::fs::read(script)
        .map_err(|e| BerryError::env(format!("read script {}: {e}", script.display())))?;
    let doc: serde_json::Value = serde_json::from_slice(&raw)
        .map_err(|e| BerryError::protocol(format!("script json: {e}")))?;
    let steps = doc
        .get("steps")
        .and_then(|s| s.as_array())
        .or_else(|| doc.as_array())
        .ok_or_else(|| BerryError::protocol("script needs {steps:[...]} or an array"))?;
    let mut done = 0usize;
    for (i, step) in steps.iter().enumerate() {
        let op = step.get("op").and_then(|s| s.as_str()).unwrap_or("");
        let args = step
            .get("args")
            .cloned()
            .unwrap_or_else(|| {
                let mut a = step.clone();
                if let Some(o) = a.as_object_mut() {
                    o.remove("op");
                }
                a
            });
        if op == "expect" {
            expect_step(&run, &args, i + 1)?;
        } else if op == "wait" {
            let ms = args.get("ms").and_then(|v| v.as_u64()).unwrap_or(100);
            std::thread::sleep(Duration::from_millis(ms));
        } else {
            control::request(&run, op, args).map_err(|e| {
                BerryError::new(e.code, format!("step {} ({op}): {}", i + 1, e.message))
            })?;
        }
        done += 1;
        if !json {
            println!("play: {}/{} {op}", i + 1, steps.len());
        }
    }
    if json {
        println!("{}", json!({"v":1,"t":"play","run":run,"steps":done}));
    } else {
        println!("play: {done} step(s) on {}", run.display());
    }
    Ok(())
}

pub fn record(
    run: Option<&str>,
    out_dir: &Path,
    frames: u32,
    interval_ms: u64,
    apng: Option<&Path>,
    json: bool,
) -> Result<()> {
    let run = find_run(run)?;
    if let Some(path) = apng {
        if frames == 0 {
            return Err(BerryError::env("record --frames must be > 0"));
        }
        let endpoint = vnc_endpoint(&run)?;
        let mut source = VncSource::connect(&endpoint)?;
        let mut raw_frames: Vec<Vec<u8>> = Vec::new();
        let mut last: Option<crate::display::frame::Frame> = None;
        let mut dims = (0u32, 0u32);
        for i in 0..frames {
            let wait = if i == 0 {
                Duration::from_secs(10)
            } else {
                Duration::from_millis(interval_ms.max(50))
            };
            if let Some(f) = source.next_frame(wait) {
                dims = (f.width, f.height);
                last = Some(f);
            }
            let Some(f) = last.as_ref() else {
                return Err(BerryError::device("record: no VNC frame"));
            };
            raw_frames.push(f.to_rgb());
            if i + 1 < frames {
                std::thread::sleep(Duration::from_millis(interval_ms));
            }
        }
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let delay = interval_ms.clamp(1, u16::MAX as u64) as u16;
        crate::png::write_apng(path, dims.0, dims.1, &raw_frames, delay)?;
        if json {
            println!(
                "{}",
                json!({"v":1,"t":"record","run":run,"frames":frames,"apng":path.display().to_string()})
            );
        } else {
            println!("record: {} frame(s) → {}", frames, path.display());
        }
        return Ok(());
    }
    std::fs::create_dir_all(out_dir)?;
    let mut paths = Vec::new();
    for i in 0..frames {
        let path = out_dir.join(format!("frame-{:04}.png", i + 1));
        let v = control::request(
            &run,
            "screenshot",
            json!({"out": path.display().to_string(), "settle_ms": 0}),
        )?;
        if !path.exists() {
            return Err(BerryError::device(format!(
                "record frame {} not written: {}",
                i + 1,
                v.get("path").and_then(|p| p.as_str()).unwrap_or("-")
            )));
        }
        paths.push(path.display().to_string());
        if i + 1 < frames {
            std::thread::sleep(Duration::from_millis(interval_ms));
        }
    }
    if json {
        println!(
            "{}",
            json!({"v":1,"t":"record","run":run,"frames":frames,"paths":paths})
        );
    } else {
        println!(
            "record: {} frame(s) → {}",
            frames,
            out_dir.display()
        );
    }
    Ok(())
}

pub fn display_list(json: bool) -> Result<()> {
    if json {
        let arr: Vec<_> = profile::PROFILES
            .iter()
            .map(|p| {
                json!({
                    "id":p.id,
                    "render":[p.render_w,p.render_h],
                    "panel":[p.panel_w,p.panel_h],
                    "memory_mib":p.memory_mib,
                })
            })
            .collect();
        println!("{}", json!({"v":1,"t":"display.list","profiles":arr}));
    } else {
        println!("{:<10} {:<12} {:<10}", "ID", "RENDER", "MEMORY");
        for p in profile::PROFILES {
            println!(
                "{:<10} {:<12} {} MiB",
                p.id,
                format!("{}x{}", p.render_w, p.render_h),
                p.memory_mib
            );
        }
    }
    Ok(())
}

fn parse_mode(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    let w: u32 = w.parse().ok()?;
    let h: u32 = h.parse().ok()?;
    (w > 0 && h > 0 && w <= 8192 && h <= 8192).then_some((w, h))
}

pub fn display_set(
    run: Option<&str>,
    value: &str,
    restart: bool,
    json: bool,
) -> Result<()> {
    let run = find_run(run)?;
    if let Some((w, h)) = parse_mode(value) {
        if !restart {
            return Err(BerryError::new(
                2,
                "custom resolution requires --restart (it starts a new host)",
            ));
        }
        let meta: serde_json::Value = std::fs::read(run.join("meta.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| json!({}));
        let pid = meta
            .get("profile")
            .and_then(|v| v.as_str())
            .unwrap_or("phone");
        let profile = profile::get(pid)?;
        let mode = format!("{w}x{h}@60");
        let new_run = restart_with(&run, &profile, Some(mode.clone()))?;
        if json {
            println!("{}", json!({"v":1,"t":"display.set","run":new_run,"old_run":run,
                "profile":profile.id,"display_mode":mode,"applied":true}));
        } else {
            println!(
                "display: {} → {} (new run: {})",
                run.display(),
                mode,
                new_run.display()
            );
        }
        return Ok(());
    }
    let profile = profile::get(value)?;
    if !restart {
        let v = control::request(
            &run,
            "display",
            json!({"action":"set","profile":profile.id}),
        )?;
        if json {
            println!("{}", json!({"v":1,"t":"display.set","run":run,"result":v}));
        } else {
            println!(
                "display: {} → {} (requires restart; add --restart to apply)",
                run.display(),
                v.get("profile").and_then(|s| s.as_str()).unwrap_or(profile.id)
            );
        }
        return Ok(());
    }
    let new_run = restart_with(&run, &profile, None)?;
    if json {
        println!("{}", json!({"v":1,"t":"display.set","run":new_run,"old_run":run,"profile":profile.id,"applied":true}));
    } else {
        println!("display: {} → {} (new run: {})", run.display(), profile.id, new_run.display());
    }
    Ok(())
}

pub fn shortcut(run: Option<&str>, action: &str, json: bool) -> Result<()> {
    let run = find_run(run)?;
    let v = control::request(&run, "shortcut", json!({"action":action}))?;
    if json {
        println!("{}", json!({"v":1,"t":"shortcut","run":run,"action":action,"result":v}));
    } else if action == "resolution" {
        println!(
            "shortcut resolution: profile {} {}x{}",
            v.get("profile").and_then(|s| s.as_str()).unwrap_or("-"),
            v.get("render_w").and_then(|n| n.as_u64()).unwrap_or(0),
            v.get("render_h").and_then(|n| n.as_u64()).unwrap_or(0),
        );
    } else {
        println!("shortcut {action}: {}", run.display());
    }
    Ok(())
}

pub fn events(
    run: Option<&str>,
    tail: usize,
    follow: bool,
    filter: Option<&str>,
    json: bool,
) -> Result<()> {
    let run = find_run(run)?;
    // Client-side substring filter; empty string = no filtering.
    let keep = |line: &str| filter.is_none_or(|f| f.is_empty() || line.contains(f));
    let emit = |line: &str| {
        if !keep(line) {
            return;
        }
        if json {
            println!("{}", json!({"v":1,"t":"event","line":line}));
        } else {
            println!("{line}");
        }
    };
    if follow {
        let mut t = Tail::open(&run.join("events.jsonl"));
        loop {
            let chunk = t.read_new();
            if !chunk.is_empty() {
                let text = String::from_utf8_lossy(&chunk);
                for line in text.lines() {
                    emit(line);
                }
                std::io::stdout().flush().ok();
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let lines: Vec<String> = if let Ok(v) =
        control::request(&run, "events", json!({"tail":tail}))
    {
        v.get("lines")
            .and_then(|l| l.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    } else {
        let text = std::fs::read_to_string(run.join("events.jsonl")).unwrap_or_default();
        text.lines()
            .rev()
            .take(tail)
            .map(str::to_string)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    };
    for line in lines {
        emit(&line);
    }
    Ok(())
}

pub fn device_start(args: DeviceStartArgs) -> Result<()> {
    // Pre-check before any spawn: an invalid spec must surface its own
    // E_USAGE + hint, not "host exited before announcing run".
    vnc_cfg::validate_flags(args.vnc_spec.as_deref(), args.vnc_ws.as_deref(), args.vnc_tls.as_deref())?;
    if args.foreground {
        return host(HostArgs {
            image: args.image,
            profile: args.profile,
            accel: args.accel,
            memory: args.memory,
            display_mode: args.display_mode,
            keep: args.keep,
            restart_on_crash: args.restart_on_crash,
            vnc_spec: args.vnc_spec,
            vnc_ws: args.vnc_ws,
            vnc_pass: args.vnc_pass,
            vnc_pass_file: args.vnc_pass_file,
            vnc_tls: args.vnc_tls,
        });
    }
    // The background host's stderr is discarded; surface the non-loopback
    // note from the CLI process itself, before the flags go anywhere.
    vnc_cfg::maybe_security_note(args.vnc_spec.as_deref());
    let json_out = args.json;
    let (run, pid) = device_start_run(args)?;
    if json_out {
        println!("{}", json!({"v":1,"t":"device.start","run":run,"pid":pid}));
    } else {
        println!("device: {}", run.display());
    }
    Ok(())
}

fn device_start_run(args: DeviceStartArgs) -> Result<(PathBuf, u32)> {
    let exe = std::env::current_exe()
        .map_err(|e| BerryError::device(format!("current_exe: {e}")))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("host")
        .arg("--profile")
        .arg(&args.profile)
        .arg("--accel")
        .arg(&args.accel);
    if let Some(img) = &args.image {
        cmd.arg("--image").arg(img);
    }
    if let Some(m) = args.memory {
        cmd.arg("--memory").arg(m.to_string());
    }
    if let Some(mode) = &args.display_mode {
        cmd.arg("--display-mode").arg(mode);
    }
    if args.keep {
        cmd.arg("--keep");
    }
    if args.restart_on_crash {
        cmd.arg("--restart-on-crash");
    }
    // The password never travels via child argv (audit finding: `ps` could
    // read it): an explicit --vnc-pass is staged as the hidden one-shot 0600
    // --vnc-pass-file — the same handoff restart_with uses for inheritance.
    // The child deletes the file as soon as it has read it (read_pass_file).
    if let Some(spec) = &args.vnc_spec {
        cmd.arg("--vnc").arg(spec);
    }
    if let Some(ws) = &args.vnc_ws {
        cmd.arg("--vnc-ws").arg(ws);
    }
    if let Some(tls) = &args.vnc_tls {
        cmd.arg("--vnc-tls").arg(tls);
    }
    let mut staged_pass: Option<std::path::PathBuf> = None;
    if let Some(pw) = &args.vnc_pass {
        let f = vnc_cfg::write_pass_temp(pw)?;
        cmd.arg("--vnc-pass-file").arg(&f);
        staged_pass = Some(f);
    } else if let Some(f) = &args.vnc_pass_file {
        cmd.arg("--vnc-pass-file").arg(f);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            // The staged pass file never reached its reader; drop it.
            if let Some(f) = &staged_pass {
                let _ = std::fs::remove_file(f);
            }
            return Err(BerryError::device(format!("spawn host: {e}")));
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BerryError::device("host: no stdout pipe"))?;
    let mut lines = BufReader::new(stdout).lines();
    let line = lines
        .next()
        .transpose()
        .map_err(|e| BerryError::device(format!("host read: {e}")))?
        .ok_or_else(|| {
            BerryError::device("host exited before announcing run")
                .with_hint("run `berry doctor`; try `berry device start --foreground`")
        })?;
    let run = line
        .strip_prefix("RUN ")
        .ok_or_else(|| BerryError::protocol(format!("host: bad announce '{line}'")))?;
    Ok((PathBuf::from(run), child.id()))
}

/// Stop an existing host and start a new host with another profile. Returns
/// the new run id (C2c/C2e). Used by `display set --restart` and attach RES.
pub fn restart_profile(run: &Path, profile_id: &str) -> Result<PathBuf> {
    let profile = profile::get(profile_id)?;
    restart_with(run, &profile, None)
}

fn restart_with(run: &Path, profile: &profile::Profile, display_mode: Option<String>) -> Result<PathBuf> {
    let meta: serde_json::Value = std::fs::read(run.join("meta.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| json!({}));
    let image = meta
        .get("image")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| BerryError::env("session has no image metadata; start with --image"))?;
    let memory = meta.get("memory_mib").and_then(|v| v.as_u64()).map(|v| v as u32);
    let keep = meta.get("keep").and_then(|v| v.as_bool()).unwrap_or(false);
    // Inherit the VNC exposure across the restart: same spec, same password
    // (v6/plan deviation 6). With a fixed port, two QEMUs cannot coexist, so
    // the old host stops first and the "new-first keeps the old on failure"
    // rollback is traded for the same-port guarantee.
    let vnc_spec = meta
        .get("vnc_spec")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| s != "off");
    let vnc_ws = meta
        .get("vnc_ws")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| s != "off");
    let vnc_tls = meta
        .get("vnc_tls")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty() && s != "off");
    let old_pass = vnc_cfg::read_pass(run);
    let fixed_port = vnc_spec.as_deref().is_some_and(|s| {
        matches!(
            vnc_cfg::parse_spec(s),
            Ok(vnc_cfg::VncSpec::Addr { port: vnc_cfg::PortSpec::Fixed(_), .. })
        )
    });
    // One-shot 0600 file keeps the inherited password out of the child argv;
    // the child deletes it as soon as it has read it (vnc_cfg::read_pass_file).
    let pass_file = match (&old_pass, vnc_spec.is_some()) {
        (Some(pw), true) => Some(vnc_cfg::write_pass_temp(pw)?),
        _ => None,
    };
    // Parent-side pre-check of the inherited triple: a spec/ws/tls that went
    // invalid since the original start (e.g. the TLS cert dir was deleted)
    // must fail here with its real E_USAGE — the replacement host would die
    // with stderr discarded, and on the fixed-port path the old session is
    // already stopped by then.
    vnc_cfg::validate_flags(vnc_spec.as_deref(), vnc_ws.as_deref(), vnc_tls.as_deref())?;
    let stop_old = |run: &Path| {
        let _ = control::request(run, "power", json!({"action":"off"}));
        let deadline = Instant::now() + Duration::from_secs(15);
        while control::is_up(run) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        if !keep {
            let _ = std::fs::remove_dir_all(run);
        }
    };
    let start_new = |pass_file: Option<PathBuf>| {
        device_start_run(DeviceStartArgs {
            image: Some(image.clone()),
            profile: profile.id.to_string(),
            accel: "tcg".to_string(),
            memory,
            display_mode: display_mode.clone(),
            keep,
            foreground: false,
            restart_on_crash: false,
            json: false,
            vnc_spec: vnc_spec.clone(),
            vnc_ws: vnc_ws.clone(),
            vnc_pass: None,
            vnc_pass_file: pass_file,
            vnc_tls: vnc_tls.clone(),
        })
    };
    let result = if fixed_port {
        stop_old(run);
        start_new(pass_file.clone())
    } else {
        // Auto/on ports: start the replacement first — if it fails, the old
        // host is untouched (the new run re-picks a free port).
        let r = start_new(pass_file.clone());
        stop_old(run);
        r
    };
    // The child normally consumes the file itself; clean up leftovers from a
    // failed start so no password file lingers in BERRY_HOME/tmp.
    if let Some(f) = &pass_file {
        let _ = std::fs::remove_file(f);
    }
    result.map(|(new_run, _)| new_run)
}
pub fn device_restart(run: Option<&str>, json: bool) -> Result<()> {
    let run = find_run(run)?;
    let v = control::request(&run, "power", json!({"action":"reboot"}))?;
    if json {
        println!("{}", json!({"v":1,"t":"device.restart","run":run,"result":v}));
    } else {
        println!("restart: {}", run.display());
    }
    Ok(())
}

pub fn device_cleanup(run: Option<&str>, all: bool, json: bool) -> Result<()> {
    if all {
        let root = run_dir::home().join("run");
        let mut cleaned = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                if control::is_up(&p) && control::request(&p, "status", json!({})).is_ok() {
                    continue; // running; skip
                }
                if cleanup_one(&p).is_ok() {
                    cleaned.push(p.display().to_string());
                }
            }
        }
        if json {
            println!("{}", json!({"v":1,"t":"device.cleanup","all":true,"runs":cleaned}));
        } else if cleaned.is_empty() {
            println!("cleanup: nothing stale/stopped");
        } else {
            for p in &cleaned {
                println!("cleanup: {p}");
            }
        }
        return Ok(());
    }
    let run = find_run(run)?;
    let pid = cleanup_one(&run)?;
    if json {
        println!("{}", json!({"v":1,"t":"device.cleanup","run":run,"qemu_pid":pid}));
    } else {
        println!("cleanup: {}", run.display());
    }
    Ok(())
}

fn cleanup_one(run: &Path) -> Result<u64> {
    if control::is_up(run) && control::request(run, "status", json!({})).is_ok() {
        return Err(BerryError::device(
            "device is still running; stop it before cleanup",
        ));
    }
    let meta: serde_json::Value = std::fs::read(run.join("meta.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| json!({}));
    let qemu_pid = meta.get("qemu_pid").and_then(|v| v.as_u64()).unwrap_or(0);
    if qemu_pid != 0 {
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("kill")
                .arg("-TERM")
                .arg(qemu_pid.to_string())
                .status();
            std::thread::sleep(Duration::from_millis(300));
            let _ = std::process::Command::new("kill")
                .arg("-KILL")
                .arg(qemu_pid.to_string())
                .status();
        }
        #[cfg(not(unix))]
        {
            let _ = std::process::Command::new("taskkill")
                .arg("/PID")
                .arg(qemu_pid.to_string())
                .arg("/F")
                .status();
        }
    }
    let _ = std::fs::remove_dir_all(run);
    Ok(qemu_pid)
}

pub fn stats(run: Option<&str>, json: bool) -> Result<()> {
    let run = find_run(run)?;
    let v = control::request(&run, "stats", json!({}))?;
    if json {
        println!("{}", json!({"v":1,"t":"stats","run":run,"stats":v}));
    } else {
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    }
    Ok(())
}

pub fn device_list(json: bool) -> Result<()> {
    let root = run_dir::home().join("run");
    let mut runs: Vec<(String, String, String, String)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&root) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let meta: serde_json::Value = std::fs::read(p.join("meta.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_else(|| json!({}));
            let label = meta.get("label").and_then(|v| v.as_str()).unwrap_or("-");
            let pid = meta.get("pid").and_then(|v| v.as_u64()).unwrap_or(0);
            let saved = run_dir::read_state(&p).unwrap_or_else(|| "no-control".to_string());
            let state = if control::is_up(&p) {
                match control::request(&p, "status", json!({})) {
                    Ok(v) => v.get("state").and_then(|s| s.as_str()).unwrap_or("?").to_string(),
                    Err(_) if saved == "running" => "stale".to_string(),
                    Err(_) => saved,
                }
            } else if saved == "running" {
                "stale".to_string()
            } else {
                saved
            };
            runs.push((
                p.file_name().unwrap_or_default().to_string_lossy().to_string(),
                label.to_string(),
                state,
                pid.to_string(),
            ));
        }
    }
    runs.sort_by(|a, b| b.0.cmp(&a.0));
    if json {
        let arr: Vec<_> = runs
            .iter()
            .map(|(id, label, state, pid)| {
                json!({"id":id,"label":label,"state":state,"pid":pid})
            })
            .collect();
        println!("{}", json!({"v":1,"t":"device.list","runs":arr}));
    } else if runs.is_empty() {
        println!("no devices");
    } else {
        println!("{:<44} {:<8} {:<12} {}", "ID", "LABEL", "STATE", "PID");
        for (id, label, state, pid) in runs {
            println!("{:<44} {:<8} {:<12} {}", id, label, state, pid);
        }
    }
    Ok(())
}

pub fn device_status(run: Option<&str>, json: bool) -> Result<()> {
    let run = find_run(run)?;
    match control::request(&run, "status", json!({})) {
        Ok(v) => {
            if json {
                println!("{}", json!({"v":1,"t":"device.status","run":run,"status":v}));
            } else {
                let state = v.get("state").and_then(|s| s.as_str()).unwrap_or("?");
                let profile = v.get("profile").and_then(|s| s.as_str()).unwrap_or("-");
                let mem = v.get("memory_mib").and_then(|n| n.as_u64()).unwrap_or(0);
                let pid = v.get("qemu_pid").and_then(|n| n.as_u64()).unwrap_or(0);
                let vnc = v
                    .get("vnc")
                    .and_then(|x| {
                        let h = x.get("host").and_then(|s| s.as_str())?;
                        let p = x.get("port").and_then(|n| n.as_u64())?;
                        Some(format!(", vnc {h}:{p}"))
                    })
                    .unwrap_or_default();
                println!(
                    "{}: {} (profile {}, {}MiB, pid {}{})",
                    run.display(),
                    state,
                    profile,
                    mem,
                    pid,
                    vnc
                );
            }
        }
        Err(_) => {
            let saved = run_dir::read_state(&run).unwrap_or_else(|| "unknown".to_string());
            let stale = saved == "running";
            let label = if stale { "stale" } else { saved.as_str() };
            let vnc = vnc_cfg::read_listen(&run)
                .map(|l| {
                    let dial = vnc_cfg::dial_host(&l.host);
                    format!(", vnc {}:{}", dial, l.port)
                })
                .unwrap_or_default();
            if json {
                println!("{}", json!({"v":1,"t":"device.status","run":run,
                    "status":{"state":label,"stale":stale},
                    "vnc":vnc_cfg::read_listen(&run)
                        .map(|l| {
                            let dial = vnc_cfg::dial_host(&l.host);
                            json!({"host":dial,"port":l.port,
                                "uri":vnc_cfg::uri_for(dial, l.port)})
                        })
                        .unwrap_or(serde_json::Value::Null)}));
            } else {
                println!("{}: {}{}", run.display(), label, vnc);
            }
        }
    }
    Ok(())
}

pub fn device_stop(run: Option<&str>, json: bool) -> Result<()> {
    let run = find_run(run)?;
    match control::request(&run, "stop", json!({})) {
        Ok(v) => {
            if json {
                println!("{}", json!({"v":1,"t":"device.stop","run":run,"result":v}));
            } else {
                println!("stop: {}", run.display());
            }
            Ok(())
        }
        Err(e) => {
            let saved = run_dir::read_state(&run).unwrap_or_default();
            if saved == "stopped" || saved == "failed" {
                if json {
                    println!("{}", json!({"v":1,"t":"device.stop","run":run,
                        "result":{"state":saved}}));
                } else {
                    println!("stop: {} (already {saved})", run.display());
                }
                Ok(())
            } else {
                Err(e.with_hint(
                    "session is stale (no control endpoint); start a new device or run `berry device cleanup`",
                ))
            }
        }
    }
}

/// `berry vnc status`: endpoint state via the control op, falling back to a
/// plain vnc.listen read (no clients) when the host is unreachable.
pub fn vnc_status(run: Option<&str>, json: bool) -> Result<()> {
    let run = find_run(run)?;
    let via_control = control::request(&run, "vnc", json!({})).ok();
    let state = match &via_control {
        Some(v) => Some(v.clone()),
        None => {
            // Degrade to run-dir artifacts (auth inferred from vnc.pass, tls
            // dir from meta when the endpoint is TLS).
            vnc_cfg::read_listen(&run).map(|l| {
                let dial = vnc_cfg::dial_host(&l.host);
                let tls_dir: serde_json::Value = std::fs::read(run.join("meta.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                    .and_then(|m| {
                        m.get("vnc_tls")
                            .and_then(|v| v.as_str())
                            .filter(|s| *s != "off")
                            .map(|s| json!(s))
                    })
                    .unwrap_or(serde_json::Value::Null);
                json!({
                    "enabled": true,
                    "listen": vnc_cfg::listen_value(&l.host, l.port, l.ws, l.tls),
                    "bind": vnc_cfg::addr_host(&l.host, l.port),
                    "uri": vnc_cfg::uri_for(dial, l.port),
                    "ws_uri": l.ws
                        .map(|p| json!(vnc_cfg::ws_uri_for(dial, p, l.tls)))
                        .unwrap_or(serde_json::Value::Null),
                    "auth": if vnc_cfg::read_pass(&run).is_some() { "vnc" } else { "none" },
                    "has_password": vnc_cfg::read_pass(&run).is_some(),
                    "clients": [],
                    "tls": if l.tls {
                        json!({"enabled": true, "dir": tls_dir, "vencrypt": serde_json::Value::Null})
                    } else {
                        json!({"enabled": false})
                    },
                })
            })
        }
    };
    match state {
        Some(v) if v.get("enabled").and_then(|x| x.as_bool()) == Some(true) => {
            if json {
                println!("{}", json!({"v":1,"t":"vnc.status","run":run,
                    "enabled":true,
                    "listen":v.get("listen").cloned().unwrap_or(serde_json::Value::Null),
                    "bind":v.get("bind").cloned().unwrap_or(serde_json::Value::Null),
                    "uri":v.get("uri").cloned().unwrap_or(serde_json::Value::Null),
                    "ws_uri":v.get("ws_uri").cloned().unwrap_or(serde_json::Value::Null),
                    "auth":v.get("auth").cloned().unwrap_or(serde_json::Value::Null),
                    "has_password":v.get("has_password").cloned().unwrap_or(serde_json::Value::Null),
                    "clients":v.get("clients").cloned().unwrap_or(json!([])),
                    "tls":v.get("tls").cloned().unwrap_or(json!({"enabled":false})),
                }));
            } else {
                let listen = v.get("listen").cloned().unwrap_or(json!({}));
                let host = listen.get("host").and_then(|s| s.as_str()).unwrap_or("?");
                let port = listen.get("port").and_then(|n| n.as_u64()).unwrap_or(0);
                let dial = vnc_cfg::dial_host(host);
                let clients = v
                    .get("clients")
                    .and_then(|c| c.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let bind = v
                    .get("bind")
                    .and_then(|b| b.as_str())
                    .map(|b| format!(" (bind {b})"))
                    .unwrap_or_default();
                println!(
                    "vnc: {}:{port}{} auth={} clients={}",
                    dial,
                    bind,
                    v.get("auth").and_then(|s| s.as_str()).unwrap_or("?"),
                    clients
                );
                if let Some(uri) = v.get("uri").and_then(|s| s.as_str()) {
                    println!("uri: {uri}");
                }
                if let Some(ws) = v.get("ws_uri").and_then(|s| s.as_str()) {
                    if !ws.is_empty() {
                        println!("ws:  {ws}");
                    }
                }
                let tls = v.get("tls").cloned().unwrap_or(json!({}));
                if tls.get("enabled").and_then(|b| b.as_bool()) == Some(true) {
                    let dir = tls.get("dir").and_then(|s| s.as_str()).unwrap_or("?");
                    match tls.get("vencrypt").and_then(|s| s.as_str()) {
                        Some(ve) => println!("tls: on ({dir}, vencrypt {ve})"),
                        None => println!("tls: on ({dir})"),
                    }
                }
                println!("run: {}", run.display());
            }
        }
        _ => {
            if json {
                println!("{}", json!({"v":1,"t":"vnc.status","run":run,"enabled":false}));
            } else {
                println!("vnc: not exposed ({})", run.display());
            }
        }
    }
    Ok(())
}

/// `berry vnc passwd [--set <pw>|--generate|--expire <seconds|never>]`
/// (generate is the default; the three forms are mutually exclusive).
pub fn vnc_passwd(
    run: Option<&str>,
    set: Option<&str>,
    expire: Option<&str>,
    json: bool,
) -> Result<()> {
    let run = find_run(run)?;
    if set.is_some() && expire.is_some() {
        return Err(BerryError::new(2, "--set and --expire are mutually exclusive"));
    }
    let (generate, args) = match (set, expire) {
        (Some(pw), _) => (false, json!({"password": pw})),
        (None, Some(t)) => {
            vnc_cfg::validate_expire(t)?;
            (false, json!({"expire": t}))
        }
        (None, None) => (true, json!({"generate": true})),
    };
    let v = control::request(&run, "vnc_passwd", args)?;
    if let Some(t) = expire {
        if json {
            println!(
                "{}",
                json!({"v":1,"t":"vnc.passwd_expire","run":run,"expire":t})
            );
        } else {
            println!("vnc: password expiry set to {t}");
        }
        return Ok(());
    }
    if json {
        println!(
            "{}",
            json!({"v":1,"t":"vnc.passwd","run":run,
                "generated":v.get("generated").cloned().unwrap_or(serde_json::Value::Null),
                "password":v.get("password").cloned().unwrap_or(serde_json::Value::Null)})
        );
    } else if generate {
        println!(
            "vnc: password rotated (generated): {}",
            v.get("password").and_then(|s| s.as_str()).unwrap_or("-")
        );
    } else {
        // Explicit --set: the caller already knows the value; no echo.
        println!("vnc: password rotated");
    }
    Ok(())
}

/// `berry vnc check`: full RFB self-test through the external TCP endpoint —
/// real auth handshake (password: --password > run's vnc.pass) and one frame.
/// TLS (VeNCrypt) endpoints walk the full state machine in
/// `VncSource::connect_tls` (wave 5): VeNCrypt 0.2 subtype negotiation,
/// rustls handshake, inner auth. The certificate directory resolves as
/// `--ca <dir>` > the run's `meta.json` `vnc_tls`; `--insecure` skips
/// verification behind an explicit warning. `--ca`/`--insecure` on a
/// plaintext endpoint are silently ignored (F17 precedent).
pub fn vnc_check(
    run: Option<&str>,
    password: Option<&str>,
    ca: Option<&Path>,
    insecure: bool,
    ws: bool,
    json: bool,
) -> Result<()> {
    if ca.is_some() && insecure {
        return Err(BerryError::new(2, "--ca and --insecure are mutually exclusive"));
    }
    let run = find_run(run)?;
    let listen = vnc_cfg::read_listen(&run).ok_or_else(|| {
        BerryError::new(3, format!("no external VNC endpoint in {}", run.display()))
            .with_hint("start the device with --vnc on (or a host:port spec)")
    })?;
    if ws {
        return vnc_check_ws(&run, &listen, password, ca, insecure, json);
    }
    let pw = password
        .map(str::to_string)
        .or_else(|| vnc_cfg::read_pass(&run));
    let dial = vnc_cfg::dial_host(&listen.host);
    let ep = Endpoint::Tcp(vnc_cfg::addr_host(dial, listen.port));
    let uri = vnc_cfg::uri_for(dial, listen.port);
    if !listen.tls {
        let mut source = VncSource::connect_auth(&ep, pw.as_deref())?;
        let frame = source.next_frame(Duration::from_secs(10)).ok_or_else(|| {
            let err = source.error().unwrap_or_else(|| "no frame".to_string());
            BerryError::protocol(format!("VNC: {err}"))
                .with_hint("is the device still running? try `berry device list`")
        })?;
        let auth = source.auth().as_str();
        if json {
            println!(
                "{}",
                json!({"v":1,"t":"vnc.check","run":run,"ok":true,"uri":uri,
                    "width":frame.width,"height":frame.height,"auth":auth})
            );
        } else {
            println!(
                "vnc check: ok {}x{} via {} (auth {})",
                frame.width, frame.height, uri, auth
            );
        }
        return Ok(());
    }
    // TLS path: certificate directory from --ca or the run's meta.
    let cfg = check_tls_client_config(&run, ca, insecure)?;
    let name = crate::tls::server_name(dial)?;
    let mut source = VncSource::connect_tls(&ep, pw.as_deref(), cfg, name)?;
    let frame = source.next_frame(Duration::from_secs(10)).ok_or_else(|| {
        let err = source.error().unwrap_or_else(|| "no frame".to_string());
        BerryError::protocol(format!("VNC: {err}"))
            .with_hint("is the device still running? try `berry device list`")
    })?;
    let auth = source.auth().as_str();
    let subtype = source.tls().unwrap_or("?");
    if json {
        println!(
            "{}",
            json!({"v":1,"t":"vnc.check","run":run,"ok":true,"uri":uri,
                "width":frame.width,"height":frame.height,"auth":auth,
                "tls":{"enabled":true,"subtype":subtype}})
        );
    } else {
        println!(
            "vnc check: ok {}x{} via {} (auth {}, tls {})",
            frame.width, frame.height, uri, auth, subtype
        );
    }
    Ok(())
}

/// TLS client config for `vnc check` — shared by the TCP/VeNCrypt and
/// ws/wss paths (wave 6) so the certificate discipline cannot drift:
/// `--ca` wins, else the run's meta `vnc_tls` directory; neither -> rc=2
/// with the `vnc certs` hint; `--insecure` swaps in the accept-all config
/// after the one-time stderr warning.
fn check_tls_client_config(
    run: &Path,
    ca: Option<&Path>,
    insecure: bool,
) -> Result<std::sync::Arc<rustls::ClientConfig>> {
    let dir = match ca.map(Path::to_path_buf).or_else(|| run_tls_dir(run)) {
        Some(d) => d,
        None => {
            return Err(BerryError::new(
                2,
                format!("TLS endpoint but {} carries no certificate directory", run.display()),
            )
            .with_hint("restart with --vnc-tls <dir> (generate one with `berry vnc certs <dir>`), or pass --ca <dir> explicitly"))
        }
    };
    if insecure {
        eprintln!(
            "berry: warning: --insecure skips TLS certificate verification; the connection is open to interception"
        );
        Ok(crate::tls::load_client_config_insecure())
    } else {
        crate::tls::load_client_config(&dir)
    }
}

/// `vnc check --ws` (wave 6): self-check the browser path — QEMU's
/// websocket endpoint — through the same RFB state machine as the TCP
/// check (`handshake_server_init` wraps `handshake_auth`), at ServerInit
/// depth: no encodings request, no frame pull. TLS runs wrap the ws
/// layer in rustls (wss); the RFB inside is plaintext 003.008 with no
/// VeNCrypt layer, unlike the TCP TLS form (probe f1). The ws judgement
/// is RFB semantics, never the `vnc.connected` event — QEMU emits it on
/// bare TCP accept, before any upgrade.
fn vnc_check_ws(
    run: &Path,
    listen: &vnc_cfg::ListenInfo,
    password: Option<&str>,
    ca: Option<&Path>,
    insecure: bool,
    json: bool,
) -> Result<()> {
    let Some(ws_port) = listen.ws else {
        return Err(BerryError::new(
            3,
            format!("no websocket listener in {}", run.display()),
        )
        .with_hint("start with --vnc on --vnc-ws on (see berry vnc status)"));
    };
    let pw = password
        .map(str::to_string)
        .or_else(|| vnc_cfg::read_pass(run));
    let dial = vnc_cfg::dial_host(&listen.host);
    let uri = vnc_cfg::ws_uri_for(dial, ws_port, listen.tls);
    let ep = Endpoint::Tcp(vnc_cfg::addr_host(dial, ws_port));
    // Same connect discipline as the TCP check (10s retry against the
    // QEMU listener racing the run dir), then the ws (or wss) upgrade.
    let base = crate::display::vnc::connect_retry(&ep, Duration::from_secs(10))?;
    base.set_read_timeout(Some(Duration::from_secs(10)))?;
    base.set_write_timeout(Some(Duration::from_secs(10)))?;
    let (w, h, auth) = if listen.tls {
        // wss: rustls client first (same certificate discipline as the
        // TCP TLS check), then the byte-identical ws layer inside.
        let cfg = check_tls_client_config(run, ca, insecure)?;
        let name = crate::tls::server_name(dial)?;
        let tcp = match base {
            crate::net::Stream::Tcp(s) => s,
            _ => return Err(BerryError::protocol("VNC: websocket endpoints are TCP only")),
        };
        let conn = rustls::ClientConnection::new(cfg, name)
            .map_err(|e| BerryError::protocol(format!("VNC: TLS client setup: {e}")))?;
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let tls_err = |e: std::io::Error| {
            BerryError::protocol(format!("VNC: TLS handshake failed: {e}")).with_hint(
                "--ca <dir> must point at the CA that issued the run's certificate (berry vnc certs), or pass --insecure to skip verification",
            )
        };
        while tls.conn.is_handshaking() {
            tls.conn.complete_io(&mut tls.sock).map_err(tls_err)?;
        }
        let mut ws = crate::ws::WsStream::handshake(tls, dial, ws_port)?;
        let (w, h, a) = crate::display::vnc::handshake_server_init(&mut ws, pw.as_deref())?;
        (w, h, a.as_str().to_string())
    } else {
        // Plaintext endpoint: --ca/--insecure are silently ignored (F17).
        let mut ws = crate::ws::WsStream::handshake(base, dial, ws_port)?;
        let (w, h, a) = crate::display::vnc::handshake_server_init(&mut ws, pw.as_deref())?;
        (w, h, a.as_str().to_string())
    };
    if json {
        // Symmetric with the TCP TLS path's tls:{enabled,subtype} field:
        // the ws face reports enabled from the listener (wss vs ws); the
        // RFB carried inside wss is always plaintext 003.008 (no VeNCrypt
        // layer — probe f1), so there is no subtype to report.
        println!(
            "{}",
            json!({"v":1,"t":"vnc.check","run":run,"ok":true,"uri":uri,
                "width":w,"height":h,"auth":auth,
                "tls":{"enabled":listen.tls}})
        );
    } else {
        println!("vnc check: ok {w}x{h} via {uri} (auth {auth})");
    }
    Ok(())
}

/// The run's `meta.json` `vnc_tls` directory (written at boot), filtered
/// like `control.rs` `vnc` does ("off"/empty -> None).
fn run_tls_dir(run: &Path) -> Option<PathBuf> {
    std::fs::read(run.join("meta.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|m| {
            m.get("vnc_tls")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
        })
        .filter(|p| {
            let s = p.to_string_lossy();
            !s.is_empty() && s != "off"
        })
}

/// `berry vnc open`: hand the endpoint to the platform viewer (macOS Screen
/// Sharing via `open vnc://…`; other platforms print the URI + guidance).
/// Failure is never fatal.
pub fn vnc_open(run: Option<&str>, json: bool) -> Result<()> {
    let run = find_run(run)?;
    let listen = vnc_cfg::read_listen(&run).ok_or_else(|| {
        BerryError::new(3, format!("no external VNC endpoint in {}", run.display()))
            .with_hint("start the device with --vnc on (or a host:port spec)")
    })?;
    let dial = vnc_cfg::dial_host(&listen.host);
    let uri = vnc_cfg::uri_for(dial, listen.port);
    let mut opened = false;
    if cfg!(target_os = "macos") {
        opened = std::process::Command::new("open")
            .arg(&uri)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    if json {
        println!("{}", json!({"v":1,"t":"vnc.open","run":run,"uri":uri,"opened":opened}));
    } else if opened {
        println!("vnc: opened {uri}");
    } else {
        println!("vnc: connect manually: {uri}");
        println!(
            "hint: TigerVNC/RealVNC: vncviewer {uri}; noVNC: use the ws endpoint from `berry vnc status`"
        );
    }
    Ok(())
}

/// `berry vnc web [--run <id>] [--port N] [--max-conns N] [--tls <dir>]
/// [--open]`: serve the web desktop over loopback and print a URL. The
/// default page is the own BerryOS shell (`index.html` + `berry.js` +
/// `berry.css`, wave 9); the vendored noVNC page stays reachable at
/// `/novnc/vnc.html` as the raw escape hatch. Default (plaintext run,
/// no --tls): plain http page whose `path=` points the browser straight
/// at QEMU's websocket port — berry never proxies RFB. TLS runs (or an
/// explicit `--tls <dir>`) are the documented exception (wave 5): the
/// page is served over https and, for TLS endpoints, a local wss proxy
/// terminates both TLS sides, because Chrome cannot dial QEMU's wss
/// directly (ERR_SSL_CLIENT_AUTH_CERT_NEEDED, probe p3). With `--open`
/// the printed URL is additionally handed to the platform browser on
/// macOS (`open`, fragment stripped — the password never enters the
/// one-shot argv); elsewhere or on failure a hint line points at the
/// URL instead, never an error. Foreground and blocking, like
/// `berry events --follow`.
pub fn vnc_web(
    run: Option<&str>,
    port: Option<u16>,
    max_conns: usize,
    tls: Option<&Path>,
    lan: bool,
    host: Option<&str>,
    qr: Option<bool>,
    open: bool,
    json: bool,
) -> Result<()> {
    let run = find_run(run)?;
    let listen = vnc_cfg::read_listen(&run).ok_or_else(|| {
        BerryError::new(3, format!("no external VNC endpoint in {}", run.display()))
            .with_hint("start with --vnc on --vnc-ws on (see berry vnc status)")
    })?;
    let Some(ws_port) = listen.ws else {
        return Err(BerryError::new(
            3,
            format!("no websocket listener in {}", run.display()),
        )
        .with_hint("start with --vnc on --vnc-ws on (see berry vnc status)"));
    };
    let root = crate::web::find_web_root()?;
    let port = port.unwrap_or_else(vnc_cfg::free_tcp_port);
    let dial = vnc_cfg::dial_host(&listen.host);
    let password = vnc_cfg::read_pass(&run);

    // Wave 10: `--lan` admission (the security matrix, contract 7) —
    // TLS runs and --tls are hard rc=2 refusals, and a passwordless
    // plaintext run never reaches the LAN binding.
    if let crate::lan::Admission::Reject(reason) = crate::lan::lan_admission(
        lan,
        listen.tls,
        tls.is_some(),
        password.is_some(),
        vnc_cfg::is_loopback(&listen.host),
    ) {
        return Err(crate::lan::admission_error(reason));
    }
    // The LAN host for URL/QR: explicit --host verbatim (validated at
    // arg parse), else the platform probe; the fallback keeps serving
    // on 0.0.0.0 with a one-time warning (--host remains the escape).
    let lan_host = if lan {
        match crate::lan::resolve_lan_host(host) {
            crate::lan::LanHost::Addr(a) => Some(a),
            crate::lan::LanHost::FallbackLoopback => {
                eprintln!(
                    "berry: warning: 无法解析本机 LAN 地址，回退 127.0.0.1（手机不可达）——用 --host <addr> 显式指定"
                );
                Some("127.0.0.1".to_string())
            }
        }
    } else {
        None
    };

    // Control API for the deck's power keys: every mode serves it on the
    // page origin; the run's VNC password gates it whenever one exists.
    let ctl = Some(std::sync::Arc::new(crate::web::CtlApi {
        run: run.clone(),
        pass: password.clone(),
    }));

    // `--open` (wave 9): hand the URL to the platform browser before the
    // blocking serve. macOS only (`open`, the `vnc open` precedent); the
    // fragment is stripped so the password never enters the one-shot
    // argv. Other platforms — or an `open` failure — degrade to a hint
    // line, never an error.
    let open_browser = |url: &str| -> bool {
        if !open || !cfg!(target_os = "macos") {
            return false;
        }
        std::process::Command::new("open")
            .arg(crate::web::strip_fragment(url))
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    let open_hint = |opened: bool| {
        if open && !opened {
            if cfg!(target_os = "macos") {
                println!("hint: 自动打开浏览器失败，请手动访问上面的 URL");
            } else {
                println!("hint: 此平台不支持自动打开浏览器，请手动访问上面的 URL");
            }
        }
    };

    // TLS plane: explicit --tls dir (validated), else the TLS run's meta
    // directory. A TLS run with neither is a usage error, not a warning.
    let tls_dir = match tls {
        Some(d) => Some(vnc_cfg::validate_tls_dir(&d.display().to_string())?),
        None => {
            if listen.tls {
                run_tls_dir(&run)
            } else {
                None
            }
        }
    };
    if listen.tls && tls_dir.is_none() {
        return Err(BerryError::new(
            2,
            format!("TLS run {} carries no certificate directory", run.display()),
        )
        .with_hint("restart with --vnc-tls <dir> (generate one with `berry vnc certs <dir>`), or pass --tls <dir> explicitly"));
    }

    match tls_dir {
        Some(dir) => {
            let server_cfg = crate::tls::load_server_config(&dir)?;
            let mut proxy_port = None;
            let url = if listen.tls {
                // TLS run: https page + local wss proxy (QEMU's wss is
                // unreachable from a browser — client-cert wall).
                let client_cfg = crate::tls::load_client_config(&dir)?;
                let name = crate::tls::server_name(dial)?;
                let proxy_l = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| {
                    BerryError::device(format!("bind wss proxy listener: {e}"))
                })?;
                let pp = proxy_l.local_addr()
                    .map_err(|e| BerryError::device(format!("wss proxy port: {e}")))?
                    .port();
                let upstream = vnc_cfg::addr_host(dial, ws_port);
                let max = max_conns;
                let proxy_cfg = server_cfg.clone();
                let diag_dir = dir.clone();
                std::thread::spawn(move || {
                    crate::web::serve_wss_proxy(
                        proxy_l,
                        upstream,
                        proxy_cfg,
                        client_cfg,
                        name,
                        max,
                        diag_dir,
                    );
                });
                proxy_port = Some(pp);
                crate::web::build_page_url_tls(port, "127.0.0.1", pp, password.as_deref())
            } else {
                // --tls on a plaintext run: https page, browser dials the
                // plain ws endpoint directly via host/port/encrypt=false
                // (wave 6: noVNC 1.7.0's ui.js rewrites `path=` schemes to
                // wss on https pages unconditionally — the host/port form
                // derives the scheme from `encrypt`, probe f2). Loopback
                // origins are exempt from mixed-content blocking (probe
                // p3); anything wider gets a one-time warning.
                if !vnc_cfg::is_loopback(dial) {
                    eprintln!(
                        "berry: warning: https page + plain ws to non-loopback {dial} may be blocked as mixed content by the browser"
                    );
                }
                crate::web::build_page_url_https(port, dial, ws_port, password.as_deref())
            };
            if json {
                let opened = open_browser(&url);
                let mut v = json!({"v":1,"t":"vnc.web","run":run,"url":url,"port":port,
                    "max_conns":max_conns,"tls":{"enabled":true},"opened":opened,
                    "root":root.display().to_string()});
                if let Some(pp) = proxy_port {
                    v["proxy_port"] = json!(pp);
                }
                println!("{v}");
            } else {
                println!("{url}");
                match proxy_port {
                    Some(pp) => println!(
                        "serving https {} on 127.0.0.1:{port} + wss proxy 127.0.0.1:{pp} (max {max_conns} connections each, Ctrl-C to stop)",
                        root.display()
                    ),
                    None => println!(
                        "serving https {} on 127.0.0.1:{port} (max {max_conns} connections, Ctrl-C to stop)",
                        root.display()
                    ),
                }
                open_hint(open_browser(&url));
            }
            std::io::stdout().flush().ok();
            crate::web::serve(&root, port, max_conns, Some(server_cfg), "127.0.0.1", None, ctl)
        }
        None => {
            // Wave 10 single-port LAN shape: berry is the only LAN entry
            // (pages + /ws proxy on one 0.0.0.0 port); admission already
            // rejected TLS/--tls/passwordless combinations above.
            if let Some(lan_host) = lan_host {
                let qr_on = qr.unwrap_or(true);
                let url = crate::web::build_page_url_lan(&lan_host, port, password.as_deref());
                let warn_extra = !vnc_cfg::is_loopback(&listen.host);
                if json {
                    let opened = open_browser(&url);
                    println!(
                        "{}",
                        json!({"v":1,"t":"vnc.web","run":run,"url":url,"port":port,
                            "max_conns":max_conns,"opened":opened,
                            "lan":{"bind":"0.0.0.0","host":lan_host,"qr":qr_on},
                            "root":root.display().to_string()})
                    );
                } else {
                    println!("{url}");
                    println!(
                        "serving {} on 0.0.0.0:{port} (LAN 模式：页面与 ws 代理同口, max {max_conns} connections, Ctrl-C to stop)",
                        root.display()
                    );
                    println!(
                        "note: LAN 暴露——明文 http 页 + 经典 VNC 认证(DES)是弱防线, 仅在可信 Wi-Fi 内使用; 建议重启时显式 --vnc-pass"
                    );
                    if warn_extra {
                        println!(
                            "warning: QEMU ws 端点自身已绑 {} 直接对外, berry 代理不是唯一 LAN 入口",
                            listen.host
                        );
                    }
                    if qr_on {
                        match crate::qr::render_qr(crate::web::strip_fragment(&url)) {
                            Some(block) => println!("{block}"),
                            None => eprintln!(
                                "berry: warning: QR 编码失败（URL 过长），已跳过渲染"
                            ),
                        }
                    }
                    println!(
                        "hint: 手机扫码打开页面, 口令在页面弹窗中手动输入（口令 = 上方 URL 的 #password= 段）"
                    );
                }
                std::io::stdout().flush().ok();
                let upstream = crate::web::LanUpstream { dial: vnc_cfg::addr_host(dial, ws_port) };
                return crate::web::serve(
                    &root,
                    port,
                    max_conns,
                    None,
                    "0.0.0.0",
                    Some(upstream),
                    ctl,
                );
            }
            // Plaintext run, no --tls: behavior identical to wave 3.
            let ws_uri = vnc_cfg::ws_uri_for(dial, ws_port, listen.tls);
            // The password rides in the URL fragment: it never leaves the
            // browser (no server sees it) and this stdout line is the
            // user-facing surface, the same trust level as
            // `vnc passwd --generate`'s echo.
            let url = crate::web::build_page_url(port, &ws_uri, password.as_deref());
            if json {
                let opened = open_browser(&url);
                println!(
                    "{}",
                    json!({"v":1,"t":"vnc.web","run":run,"url":url,"port":port,"max_conns":max_conns,
                        "opened":opened,"root":root.display().to_string()})
                );
            } else {
                println!("{url}");
                println!(
                    "serving {} on 127.0.0.1:{port} (max {max_conns} connections, Ctrl-C to stop)",
                    root.display()
                );
                // Wave 6: point at the browser-path self-check (the TLS
                // run mode does not add this — its browser path goes
                // through the berry proxy, not QEMU's ws port).
                println!("hint: self-test the browser path with berry vnc check --ws");
                open_hint(open_browser(&url));
            }
            std::io::stdout().flush().ok();
            crate::web::serve(&root, port, max_conns, None, "127.0.0.1", None, ctl)
        }
    }
}

/// `berry vnc certs <dir>`: generate a self-signed x509 server set for
/// `--vnc-tls` using openssl subprocesses (std has no x509 machinery — the
/// no-new-dependencies discipline is preserved by shelling out instead).
/// Each command is echoed to stderr as it runs; the output carries only the
/// two certificate SHA256 fingerprints — private keys are never printed.
pub fn vnc_certs(dir: &Path, json: bool) -> Result<()> {
    let openssl = if Path::new("/usr/bin/openssl").is_file() {
        "/usr/bin/openssl".to_string()
    } else {
        "openssl".to_string()
    };
    std::fs::create_dir_all(dir)?;
    let cert = dir.join("server-cert.pem");
    if cert.is_file() {
        return Err(BerryError::new(
            2,
            format!("{} already exists", cert.display()),
        )
        .with_hint("delete the directory (or pick another one) to regenerate — refusing to overwrite an existing certificate set"));
    }
    // LibreSSL 3.3.6 (probe p3): the CA needs critical basicConstraints and
    // keyUsage or QEMU rejects the dir; the server cert needs the SANs or
    // strict clients reject it.
    let ca_key = dir.join("ca-key.pem");
    let server_key = dir.join("server-key.pem");
    let csr = dir.join("server.csr");
    let ext = dir.join("server.ext");
    let run_step = |args: &[&str]| -> Result<String> {
        let mut cmd = std::process::Command::new(&openssl);
        cmd.args(args).current_dir(dir);
        eprintln!("berry: running: openssl {}", args.join(" "));
        let out = cmd
            .output()
            .map_err(|e| BerryError::device(format!("run openssl: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(BerryError::device(format!(
                "openssl {} failed: {stderr}",
                args[0]
            ))
            .with_hint(format!(
                "manual commands (from {}):\nopenssl req -nodes -new -x509 -newkey rsa:2048 -keyout ca-key.pem -out ca-cert.pem -days 3650 -subj /CN=berryos-vnc-ca -addext basicConstraints=critical,CA:TRUE -addext keyUsage=critical,keyCertSign,cRLSign -addext subjectKeyIdentifier=hash -addext authorityKeyIdentifier=keyid:always,issuer\nopenssl req -nodes -new -newkey rsa:2048 -keyout server-key.pem -out server.csr -subj /CN=localhost\necho 'subjectAltName=IP:127.0.0.1,DNS:localhost\\nauthorityKeyIdentifier=keyid,issuer' > server.ext\nopenssl x509 -req -in server.csr -CA ca-cert.pem -CAkey ca-key.pem -CAcreateserial -out server-cert.pem -days 825 -extfile server.ext\nopenssl verify -CAfile ca-cert.pem server-cert.pem",
                dir.display()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    };
    run_step(&[
        "req", "-nodes", "-new", "-x509", "-newkey", "rsa:2048",
        "-keyout", "ca-key.pem", "-out", "ca-cert.pem", "-days", "3650",
        "-subj", "/CN=berryos-vnc-ca",
        "-addext", "basicConstraints=critical,CA:TRUE",
        "-addext", "keyUsage=critical,keyCertSign,cRLSign",
        "-addext", "subjectKeyIdentifier=hash",
        "-addext", "authorityKeyIdentifier=keyid:always,issuer",
    ])?;
    run_step(&[
        "req", "-nodes", "-new", "-newkey", "rsa:2048",
        "-keyout", "server-key.pem", "-out", "server.csr",
        "-subj", "/CN=localhost",
    ])?;
    // SKID/AKID on both sides: strict TLS verifiers (python ssl = OpenSSL 3,
    // live-calibrated) reject chains whose CA or leaf lacks an AKID.
    std::fs::write(
        &ext,
        "subjectAltName=IP:127.0.0.1,DNS:localhost\nauthorityKeyIdentifier=keyid,issuer\n",
    )?;
    run_step(&[
        "x509", "-req", "-in", "server.csr",
        "-CA", "ca-cert.pem", "-CAkey", "ca-key.pem", "-CAcreateserial",
        "-out", "server-cert.pem", "-days", "825", "-extfile", "server.ext",
    ])?;
    let verify = run_step(&["verify", "-CAfile", "ca-cert.pem", "server-cert.pem"])?;
    if !verify.trim().ends_with("OK") {
        return Err(BerryError::device(format!(
            "openssl verify did not report OK: {}",
            verify.trim()
        )));
    }
    // Clean intermediates (csr/ext/srl); keep both keys 0600.
    let _ = std::fs::remove_file(&csr);
    let _ = std::fs::remove_file(&ext);
    let _ = std::fs::remove_file(dir.join("ca-cert.srl"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in [&ca_key, &server_key] {
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
        }
    }
    let fingerprint = |file: &str| -> Result<String> {
        let out = run_step(&["x509", "-noout", "-fingerprint", "-sha256", "-in", file])?;
        let line = out.lines().next().unwrap_or("").trim().to_string();
        let fp = line.split('=').nth(1).unwrap_or("").to_string();
        if fp.is_empty() {
            return Err(BerryError::device(format!("openssl fingerprint parse: {line}")));
        }
        Ok(fp)
    };
    let ca_fp = fingerprint("ca-cert.pem")?;
    let server_fp = fingerprint("server-cert.pem")?;
    if json {
        println!(
            "{}",
            json!({"v":1,"t":"vnc.certs","dir":dir.display().to_string(),
                "ca_fingerprint":ca_fp,"server_fingerprint":server_fp})
        );
    } else {
        println!("certs: {} (verify OK)", dir.display());
        println!("  ca-cert:     sha256 {ca_fp}");
        println!("  server-cert: sha256 {server_fp}");
    }
    Ok(())
}

pub fn power(run: Option<&str>, action: &str, force: bool, json: bool) -> Result<()> {
    let run = find_run(run)?;
    let v = control::request(&run, "power", json!({"action":action,"force":force}))?;
    if json {
        println!("{}", json!({"v":1,"t":"power","run":run,"action":action,"result":v}));
    } else {
        println!("power {action}: {}", run.display());
    }
    Ok(())
}

pub fn logs(run: Option<&str>, tail: usize, follow: bool, json: bool) -> Result<()> {
    let run = find_run(run)?;
    let path = run.join("serial.log");
    if follow {
        let mut t = Tail::open(&path);
        loop {
            let chunk = t.read_new();
            if !chunk.is_empty() {
                let text = String::from_utf8_lossy(&chunk);
                for line in text.lines() {
                    if json {
                        println!("{}", json!({"v":1,"t":"log","line":line}));
                    } else {
                        println!("{line}");
                    }
                }
                std::io::stdout().flush().ok();
            }
            std::thread::sleep(Duration::from_millis(100));
            let _ = tail;
        }
    }
    let lines: Vec<String> = if let Ok(v) = control::request(&run, "logs", json!({"tail":tail})) {
        v.get("lines")
            .and_then(|l| l.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(tail)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    };
    if json {
        println!("{}", json!({"v":1,"t":"logs","lines":lines}));
    } else {
        for line in lines {
            println!("{line}");
        }
    }
    Ok(())
}

pub(crate) fn find_run(id: Option<&str>) -> Result<PathBuf> {
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

pub(crate) fn vnc_endpoint(run: &Path) -> Result<Endpoint> {
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
    // Internal -vnc endpoints use display-number syntax on QEMU, so ports
    // below 5900 are not expressible (probe f2).
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
