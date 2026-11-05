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
    let mut device = crate::device::Device::start(
        args.image,
        profile,
        &args.accel,
        args.memory,
        args.display_mode,
        args.keep,
        "host",
    )?;
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

pub fn events(run: Option<&str>, tail: usize, follow: bool, json: bool) -> Result<()> {
    let run = find_run(run)?;
    let path = run.join("events.jsonl");
    if follow {
        let mut t = Tail::open(&path);
        loop {
            let chunk = t.read_new();
            if !chunk.is_empty() {
                let text = String::from_utf8_lossy(&chunk);
                for line in text.lines() {
                    if json {
                        println!("{}", json!({"v":1,"t":"event","line":line}));
                    } else {
                        println!("{line}");
                    }
                }
                std::io::stdout().flush().ok();
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let lines: Vec<String> = if let Ok(v) = control::request(&run, "events", json!({"tail":tail})) {
        v.get("lines")
            .and_then(|l| l.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    } else {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
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
        if json {
            println!("{}", json!({"v":1,"t":"event","line":line}));
        } else {
            println!("{line}");
        }
    }
    Ok(())
}

pub fn device_start(args: DeviceStartArgs) -> Result<()> {
    if args.foreground {
        return host(HostArgs {
            image: args.image,
            profile: args.profile,
            accel: args.accel,
            memory: args.memory,
            display_mode: args.display_mode,
            keep: args.keep,
            restart_on_crash: args.restart_on_crash,
        });
    }
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
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| BerryError::device(format!("spawn host: {e}")))?;
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
    // Start the replacement first: if it fails, the old host is untouched.
    let (new_run, _) = device_start_run(DeviceStartArgs {
        image: Some(image),
        profile: profile.id.to_string(),
        accel: "tcg".to_string(),
        memory,
        display_mode,
        keep,
        foreground: false,
        restart_on_crash: false,
        json: false,
    })?;
    let _ = control::request(run, "power", json!({"action":"off"}));
    let deadline = Instant::now() + Duration::from_secs(15);
    while control::is_up(&run) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    if !keep {
        let _ = std::fs::remove_dir_all(run);
    }
    Ok(new_run)
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
                println!(
                    "{}: {} (profile {}, {}MiB, pid {})",
                    run.display(),
                    state,
                    profile,
                    mem,
                    pid
                );
            }
        }
        Err(_) => {
            let saved = run_dir::read_state(&run).unwrap_or_else(|| "unknown".to_string());
            let stale = saved == "running";
            let label = if stale { "stale" } else { saved.as_str() };
            if json {
                println!("{}", json!({"v":1,"t":"device.status","run":run,
                    "status":{"state":label,"stale":stale}}));
            } else {
                println!("{}: {}", run.display(), label);
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
