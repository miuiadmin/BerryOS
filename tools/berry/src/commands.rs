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
    let serial_sock = run.file("serial.sock");
    let log = run.file("qemu.log");
    if args.json {
        println!(
            "{}",
            json!({"v":1,"t":"run.start","run":run.path(),"kernel":kernel,"accel":accel.as_arg(),"memory_mib":memory})
        );
    }
    // 取舍注记（三轮审计 C3）：spawn 失败的 ? 早退不 keep_fail——目录
    // 仅 meta.json 半成品；QEMU 起后的失败保留见下方 keep_fail 各臂。
    let mut child = qemu::spawn_kernel(&qemu, &kernel, &serial, &serial_sock, &log, accel, memory)?;
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
            // M1 失败现场保留：marker 面（CLI 有默认 marker「BerryOS boot」
            //——空 marker 仅 --marker '' 显式可达）qemu 先退 = 未达 marker
            // 必失败；显式空 marker 的交互 run 仅异常退出算失败——正常退
            // 出不保留。
            if !status.success() || !args.marker.is_empty() {
                run.keep_fail();
            }
            let marker_note = if args.marker.is_empty() {
                String::new()
            } else {
                format!(" before marker '{}'", args.marker)
            };
            return Err(BerryError::device(format!(
                "qemu exited ({status}){marker_note}"
            ))
            .with_hint(format!("serial tail:\n{text}")));
        }
        if Instant::now() >= deadline {
            let text = tail.tail_text(4000);
            stop_child(&mut child, None);
            run.keep_fail();
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
    let vars = qemu::vars_store(run.path(), 64)?;
    let serial = run.file("serial.log");
    let serial_sock = run.file("serial.sock");
    let log = run.file("qemu.log");
    let (vnc, qmp_ep) = endpoints(&run);
    if args.json {
        println!(
            "{}",
            json!({"v":1,"t":"shot.start","run":run.path(),"image":image,"profile":profile.id,
                   "data_disk": qemu::data_disk_plan(&profile.id).map(|p| p.display().to_string())})
        );
    }
    let mut child = qemu::spawn_image(
        &qemu,
        &image,
        &firmware,
        &vars,
        &serial,
        &serial_sock,
        &log,
        &vnc,
        &qmp_ep,
        accel,
        memory,
        true,
        mode.as_deref(),
        profile.id,
        None,
        None,
        None,
    )?;
    // M1 失败现场保留：shot 的成功判据是拿到帧——以下任一失败路径
    //（端点不起 / VNC 连不上 / qemu 先退 / 超时 / 取帧失败 / png 落盘
    // 失败）自动 keep run 目录，成功才默认删。
    if let Err(e) = wait_endpoint(&vnc, Duration::from_secs(10)) {
        run.keep_fail();
        return Err(e);
    }
    let mut source = match VncSource::connect(&vnc) {
        Ok(s) => s,
        Err(e) => {
            run.keep_fail();
            return Err(e);
        }
    };
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
                run.keep_fail();
                return Err(BerryError::device(format!(
                    "qemu exited ({status}) before marker '{marker}'"
                ))
                .with_hint(format!("serial tail:\n{}", tail.tail_text(4000))));
            }
            if Instant::now() >= deadline {
                stop_child(&mut child, None);
                run.keep_fail();
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
    let frame = match source.next_frame(Duration::from_secs(10)) {
        Some(f) => f,
        None => {
            let err = source.error().unwrap_or_else(|| "no frame".to_string());
            run.keep_fail();
            return Err(BerryError::protocol(format!("VNC: {err}"))
                .with_hint(format!("serial tail:\n{}", tail.tail_text(4000))));
        }
    };
    let out = args.out.unwrap_or_else(|| PathBuf::from("shot.png"));
    if let Err(e) = crate::png::write_png(&out, frame.width, frame.height, &frame.to_rgb()) {
        run.keep_fail();
        return Err(e);
    }
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
    // Wave 11: the VNC flags travel as a direct parameter into
    // window::run -> Device::start (the wave-1 process-level launch
    // override retired; BERRY_VNC env remains the fallback inside
    // vnc_cfg::resolve).
    let vnc = vnc_cfg::Launch {
        spec: args.vnc_spec.clone(),
        ws: args.vnc_ws.clone(),
        pass: args.vnc_pass.clone(),
        pass_file: None,
        tls: args.vnc_tls.clone(),
    };
    vnc_cfg::maybe_security_note(args.vnc_spec.as_deref());
    crate::window::run(args, vnc)
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
    let vnc = vnc_cfg::Launch {
        spec: args.vnc_spec.clone(),
        ws: args.vnc_ws.clone(),
        pass: args.vnc_pass.clone(),
        pass_file: args.vnc_pass_file.clone(),
        tls: args.vnc_tls.clone(),
    };
    let mut device = crate::device::Device::start(
        args.image,
        profile,
        &args.accel,
        args.memory,
        args.display_mode,
        args.keep,
        "host",
        vnc,
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
            // 锚值口径 = 十六进制（错误面与脚本样例一律 {:016x} 形态；
            // 旧 .parse::<u64>() 十进制口径把 hex 锚值整族拒成
            // "bad fnv1a64"——测试首跑实咬，随刀对齐显示面契约）。
            let want = u64::from_str_radix(want.trim_start_matches("0x"), 16)
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
    } else if !run.join("events.jsonl").exists() {
        // 同 logs()：run 不活且 events.jsonl 缺席——旧版空静默 exit 0。
        eprintln!(
            "berry: no events.jsonl in {} (run not live / file absent)",
            run.display()
        );
        Vec::new()
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
    let profile_id = args.profile.clone();
    let (run, pid) = device_start_run(args)?;
    if json_out {
        println!("{}", json!({"v":1,"t":"device.start","run":run,"pid":pid,
               "data_disk": qemu::data_disk_plan(&profile_id).map(|p| p.display().to_string())}));
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
        let mut cleaned: Vec<(String, Option<bool>)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                if control::is_up(&p) && control::request(&p, "status", json!({})).is_ok() {
                    continue; // running; skip
                }
                if let Ok((_, kept)) = cleanup_one(&p) {
                    cleaned.push((p.display().to_string(), kept));
                }
            }
        }
        if json {
            let runs: Vec<_> = cleaned
                .iter()
                .map(|(p, kept)| json!({"run": p, "kept": kept}))
                .collect();
            println!("{}", json!({"v":1,"t":"device.cleanup","all":true,"runs":runs}));
        } else if cleaned.is_empty() {
            println!("cleanup: nothing stale/stopped");
        } else {
            for (p, kept) in &cleaned {
                print_cleanup_line(p, *kept);
            }
        }
        return Ok(());
    }
    let run = find_run(run)?;
    let (pid, kept) = cleanup_one(&run)?;
    if json {
        println!(
            "{}",
            json!({"v":1,"t":"device.cleanup","run":run,"qemu_pid":pid,"kept":kept})
        );
    } else {
        print_cleanup_line(&run.display().to_string(), kept);
    }
    Ok(())
}

/// cleanup 人读行：kept=true 的目录是 keep_fail 落的失败现场保留，清理
/// 时标注来源（与 json 的 kept 字段同源，见 read_end_kept）。
fn print_cleanup_line(run: &str, kept: Option<bool>) {
    if kept == Some(true) {
        println!("cleanup: {run} (kept: failure artifact)");
    } else {
        println!("cleanup: {run}");
    }
}

/// index.jsonl 的第一个读方（四轮审计挂账收敛）：从尾向头找该 run id
/// 最近一条 run.end 行取其 kept 值；文件缺席/无匹配行/字段缺席容错
/// None（索引容忍不全——宿主崩溃必漏 end 行，枚举以扫目录兜底）。
/// berry 进程探活（cleanup 守卫内核，五轮后抽纯函数可测）：`/bin/kill -0`
/// 同 uid 下进程在 = exit 0（macOS 无 /proc 的可移植形态）。pid 0 一律死
/// （meta 缺/坏的残目录语义）。
pub(crate) fn berry_pid_alive(berry_pid: u64) -> bool {
    #[cfg(unix)]
    {
        berry_pid != 0
            && std::process::Command::new("kill")
                .arg("-0")
                .arg(berry_pid.to_string())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn read_end_kept(run_id: &str) -> Option<bool> {
    let idx = run_dir::home().join("run").join("index.jsonl");
    let text = std::fs::read_to_string(idx).ok()?;
    for line in text.lines().rev() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("t").and_then(|t| t.as_str()) != Some("run.end") {
            continue;
        }
        if v.get("run").and_then(|r| r.as_str()) != Some(run_id) {
            continue;
        }
        return v.get("kept").and_then(|k| k.as_bool());
    }
    None
}

/// 清一个 run 目录，返回（meta 的 qemu_pid, index 最近 run.end 的 kept）。
/// kept=Some(true) 的目录是失败现场保留（keep_fail），输出侧据此标注。
fn cleanup_one(run: &Path) -> Result<(u64, Option<bool>)> {
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
    // 四轮审计（D2/D4）升级：探活对全部目录形态生效（移出 qemu_pid==0
    // 分支）——meta 的 pid 是 berry 进程 pid（RunDir::create 恒写）：
    // a) 活会话拒删：run/shot 类其 QEMU 无端点杀不到（直删 = 抽走活会话
    //    的 serial.log 并孤儿化 QEMU）；device 类防 restart_in_place 窗口
    //    （_control 已关而 sock 残留 → control 检查失守）误杀 pid 回收
    //    复用的无辜进程并抽走活会话目录。
    // b) pid=0（meta 缺/坏的残目录）不拒——残目录本就是要清的；旧版
    //    「pid 0 也拒」是误导文案（四轮 D4 实抓）。
    // c) 非 unix 无 kill -0 探活面，pid≠0 保守拒删。
    let berry_pid = meta.get("pid").and_then(|v| v.as_u64()).unwrap_or(0);
    #[cfg(unix)]
    {
        if berry_pid_alive(berry_pid) {
            return Err(BerryError::device(format!(
                "run {} belongs to a live berry session (pid {berry_pid}); stop it before cleanup",
                run.display()
            )));
        }
    }
    #[cfg(not(unix))]
    {
        if berry_pid != 0 {
            return Err(BerryError::device(format!(
                "run {} pid liveness cannot be probed on this platform; verify manually",
                run.display()
            )));
        }
    }
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
    // 删目录前读 index：目录一删，run.id 的最近 end 行就是唯一存活证词。
    let run_id = run
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string);
    let kept = run_id.as_deref().and_then(read_end_kept);
    let _ = std::fs::remove_dir_all(run);
    Ok((qemu_pid, kept))
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

/// `berry vnc web [--run <id>] [--port N] [--max-conns N]
/// [--lan [--host <addr>] [--qr | --no-qr] [--ca <dir> | --insecure]]
/// [--open] [--json] [--tls <dir>]`: serve the web desktop over loopback
/// and print a URL. The default page is the own BerryOS shell
/// (`index.html` + `berry.js` + `berry.css`, wave 9); the vendored noVNC
/// page stays reachable at `/novnc/vnc.html` as the raw escape hatch.
/// Default (plaintext run, no --tls): plain http page whose `path=`
/// points the browser straight at QEMU's websocket port — berry never
/// proxies RFB. TLS runs (or an explicit `--tls <dir>`) are the
/// documented exception (wave 5): the page is served over https and,
/// for TLS endpoints, a local wss proxy terminates both TLS sides,
/// because Chrome cannot dial QEMU's wss directly
/// (ERR_SSL_CLIENT_AUTH_CERT_NEEDED, probe p3). `--lan` on a TLS run
/// (wave 11) is the second exception: the page stays plain http on
/// 0.0.0.0 and only the `/ws` proxy's upstream leg gains a
/// strict-verified rustls client to QEMU's loopback wss (`--ca`
/// overrides the trust dir, `--insecure` skips verification after a
/// warning). With `--open` the printed URL is additionally handed to
/// the platform browser on macOS (`open`, fragment stripped — the
/// password never enters the one-shot argv); elsewhere or on failure a
/// hint line points at the URL instead, never an error. Foreground and
/// blocking, like `berry events --follow`.
pub fn vnc_web(
    run: Option<&str>,
    port: Option<u16>,
    max_conns: usize,
    tls: Option<&Path>,
    lan: bool,
    host: Option<&str>,
    qr: Option<bool>,
    open: bool,
    ca: Option<&Path>,
    insecure: bool,
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

    // Wave 10 admission (the security matrix, contract 7) — a
    // passwordless run never reaches the LAN binding and `--tls` stays
    // mutually exclusive with `--lan`; TLS runs are admitted since wave
    // 11 (the upstream leg gains rustls below).
    if let crate::lan::LanAdmission::Reject { msg, hint } = crate::lan::lan_admission(
        lan,
        &listen,
        tls.is_some(),
        password.is_some(),
    ) {
        return Err(BerryError::new(2, msg).with_hint(&hint));
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
    // page origin. F44: a passwordless (loopback-only) run must not skip
    // the credential gate — ctl.pass=None skips it entirely and a bare
    // POST (no custom header) is a CORS simple request any website can
    // fire at loopback. So a passwordless serve mints a random API token
    // (the same /dev/urandom generator as VNC passwords) and the gate
    // demands it in X-Berry-Pass; the page URL delivers it in #apikey=,
    // the same read-once fragment channel as #password= (QR and --open
    // strip the fragment, so neither surface carries it). Runs with a
    // password keep today's behavior byte for byte: password == token.
    let api_token = password.is_none().then(vnc_cfg::generate_password);
    let ctl = Some(std::sync::Arc::new(crate::web::CtlApi {
        run: run.clone(),
        pass: password.clone().or(api_token.clone()),
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

    // Wave 11 TLS×LAN: the single-port LAN shape now also serves TLS
    // runs — the page stays plain http on 0.0.0.0 and only the `/ws`
    // proxy's upstream leg gains a strict-verified rustls client to
    // QEMU's loopback wss (the wave-10 restart condition: "rustls
    // client terminates the upstream + http page"). The `--lan`
    // decision diverges before the tls_dir plane below; explicit `--tls`
    // stays refused by admission above. URL/QR and the text output are
    // byte-identical to a plaintext `--lan`; only the JSON `lan` object
    // grows `upstream` (ws vs wss).
    if lan {
        let upstream_tls = if listen.tls {
            let cfg = check_tls_client_config(&run, ca, insecure)?;
            let name = crate::tls::server_name(dial)?;
            let dir = ca
                .map(Path::to_path_buf)
                .or_else(|| run_tls_dir(&run))
                .unwrap_or_default();
            Some(crate::web::LanTlsUpstream { cfg, name, dir })
        } else {
            // Plaintext upstream: --ca/--insecure are silently ignored
            // (F17) — there is no TLS leg to configure.
            None
        };
        if let Some(lan_host) = lan_host {
            let qr_on = qr.unwrap_or(true);
            let url = crate::web::build_page_url_lan(&lan_host, port, password.as_deref(), api_token.as_deref());
            let warn_extra = !vnc_cfg::is_loopback(&listen.host);
            if json {
                let opened = open_browser(&url);
                println!(
                    "{}",
                    json!({"v":1,"t":"vnc.web","run":run,"url":url,"port":port,
                        "max_conns":max_conns,"opened":opened,
                        "lan":{"bind":"0.0.0.0","host":lan_host,"qr":qr_on,
                            "upstream":crate::web::lan_upstream_kind(upstream_tls.is_some())},
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
                // F52: --lan text mode honors --open like every other
                // form (JSON branch above, both loopback branches below)
                // — browser or hint line, never a silent drop.
                open_hint(open_browser(&url));
            }
            std::io::stdout().flush().ok();
            // 输入拦截器装配（2026-11-16）：run 有 qmp.sock 即启用；QMP abs
            // 值域 0..32767 需要缩放分母——**display_mode 实几何优先**
            // （--display-mode 覆盖档位下 profile 的渲染尺寸已失真：
            // 2026-11-20 1080x1920 场实锤——按 480x1000 缩放把中/下半屏
            // 点击全钳到右/底边缘，「点了很多没反应」主凶；ram.rs 原生路
            // 「缩放分母用 fb 实几何」同款单源律），'WxH@hz' 形取 @ 前
            // WxH；缺场才回退 profile 渲染尺寸。
            let qmp_inject = run.join("qmp-inject.sock");
            let qmp_sock = if qmp_inject.exists() { qmp_inject } else { run.join("qmp.sock") };
            let (gw, gh) = std::fs::read(run.join("meta.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .and_then(|m| {
                    // display_mode 实几何优先——解析复用 berry-dmface 单源
                    // （对抗审查 P1：宿主手写解串缺 trim+32B 门，与客机判定链
                    // 分叉可致同串两侧判定相反、静默回退错分母）；宽度同款
                    // 32 对齐（P2：客机 scanout 宽 (w0+31)&!31——1080→1088，
                    // 分母用请求宽则 web 路系统性右偏且与原生路 fbinfo 不一致）。
                    // 解析失败回退 profile 是有意降级链（回退打一行可见）。
                    m.get("display_mode")
                        .and_then(|s| s.as_str())
                        .and_then(|s| {
                            berry_dmface::parse_display_mode(s.as_bytes())
                                .filter(|(w, h)| *w > 0 && *h > 0)
                                .map(|(w, h)| ((w + 31) & !31, h))
                        })
                        .or_else(|| {
                            let fallback = m
                                .get("profile")
                                .and_then(|p| p.as_str())
                                .and_then(|id| crate::profile::get(id).ok())
                                .map(|pr| (pr.render_w, pr.render_h));
                            let raw = m
                                .get("display_mode")
                                .and_then(|s| s.as_str())
                                .unwrap_or("(缺席)");
                            if let Some((fw, fh)) = fallback {
                                eprintln!(
                                    "berry: 拦截器几何回退 profile {fw}x{fh}（display_mode={raw} 解析失败）"
                                );
                            }
                            fallback
                        })
                })
                .unwrap_or((1, 1));
            let upstream = crate::web::LanUpstream {
                dial: vnc_cfg::addr_host(dial, ws_port),
                tls: upstream_tls,
                qmp_sock: qmp_sock.exists().then_some(qmp_sock),
                guest_w: gw,
                guest_h: gh,
            };
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
    }

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
                crate::web::build_page_url_tls(port, "127.0.0.1", pp, password.as_deref(), api_token.as_deref())
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
                crate::web::build_page_url_https(port, dial, ws_port, password.as_deref(), api_token.as_deref())
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
            // --lan already returned above (wave 11 hoisted its branch
            // before this plane); what remains is the plaintext run,
            // no --tls: behavior identical to wave 3.
            let ws_uri = vnc_cfg::ws_uri_for(dial, ws_port, listen.tls);
            // The password rides in the URL fragment: it never leaves the
            // browser (no server sees it) and this stdout line is the
            // user-facing surface, the same trust level as
            // `vnc passwd --generate`'s echo.
            let url = crate::web::build_page_url(port, &ws_uri, password.as_deref(), api_token.as_deref());
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
    } else if !path.exists() {
        // 四轮审计挂账（回落双行提示）：run 不活且 serial.log 缺席——旧版
        // 空静默 exit 0（人读零输出、json 空 lines 数组），比报错更误导。
        // stderr 一行说明后退；stdout 行为零漂移。文件在席路径不变。
        eprintln!(
            "berry: no serial.log in {} (run not live / file absent)",
            run.display()
        );
        Vec::new()
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

/// `berry console [--run <id>]`——宿主终端接进客机串口控制台（bsh）。
/// 不走 control.sock：直连 run 的 serial.sock 数据面（README 控制面节
/// 同注；screenshot 的 VNC 兜底同族——attach 数据端点，不经控制面）。
pub fn console(run: Option<&str>) -> Result<()> {
    // 非 Unix 诚实拒（先于 find_run——拒绝不需要解析 run）：件1 的串口
    // chardev socket 是 unix path 形态，非 Unix 平台维持 -serial file:
    // 单向落盘（qemu.rs machine_args 注记）——serial.sock 永不出现，
    // chardev TCP 化跟进前无从接入。
    #[cfg(not(unix))]
    let _ = run;
    #[cfg(not(unix))]
    return Err(BerryError::env(
        "console is not supported yet on this platform (serial chardev TCP pending)",
    )
    .with_hint("非 Unix 维持 -serial file: 单向落盘；chardev TCP 化后跟进"));
    #[cfg(unix)]
    return console_attach(&find_run(run)?);
}

// ---- 件2 主链（unix）：前置检查 → 独占锁 → 连接（拒连重试）→ TTY
// ---- raw → 双向泵 → join + 恢复终端 + 一行收尾。
#[cfg(unix)]
fn console_attach(run: &Path) -> Result<()> {
    use std::io::Read as _;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let sock = run.join("serial.sock");
    if !sock.exists() {
        // 件1 前起的老 run：QEMU 起于 -serial file:，从无 serial.sock。
        return Err(BerryError::env(format!(
            "run {} 起于旧 -serial file: 形态，无 console 端点",
            run.display()
        ))
        .with_hint(format!(
            "重启该 run（berry device restart --run {}）即以 chardev 形态重生出 serial.sock",
            run.display()
        )));
    }
    // 锁先于连接：防两个 console 赛跑抢连（后者只会成为不被服务的僵尸）。
    let _lock = lock_console(run)?;
    let stream = connect_serial(&sock)?;
    // try_clone 先于 raw：clone 失败的 ? 早退不带终端副作用（raw 之后
    // 的路径再无 ? ——恢复收敛在主链收尾处）。
    let wsock = stream.try_clone()?;
    let saved_tty = stdin_raw();

    let exit = Arc::new(AtomicBool::new(false));
    // 写泵：stdin 逐字节读 → socket 写。读到 0x04（Ctrl-D，bsh 语义的
    // 退出键）或 EOF → 置退出旗标并 shutdown socket 读端——读泵的阻塞
    // read 立即返 0 收尾（QEMU 不会因我们停写而关连接，不主动断读端
    // 读泵会挂到 QEMU 死）。
    let wexit = Arc::clone(&exit);
    let writer = std::thread::spawn(move || {
        let mut wsock = wsock;
        let mut stdin = std::io::stdin();
        let mut b = [0u8; 1];
        loop {
            if wexit.load(Ordering::SeqCst) {
                break;
            }
            match stdin.read(&mut b) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if b[0] == 0x04 {
                        break;
                    }
                    if wsock.write_all(&b).is_err() {
                        break;
                    }
                }
            }
        }
        wexit.store(true, Ordering::SeqCst);
        let _ = wsock.shutdown(std::net::Shutdown::Read);
    });
    // 读泵：socket 块读 → stdout 写 + flush。Ok(0)/Err（对端断：QEMU
    // 退出或原地重启）→ 置退出旗标。
    let rexit = Arc::clone(&exit);
    let reader = std::thread::spawn(move || {
        let mut stream = stream;
        let mut out = std::io::stdout();
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out.write_all(&buf[..n]).is_err() || out.flush().is_err() {
                        break;
                    }
                }
            }
        }
        rexit.store(true, Ordering::SeqCst);
    });
    // 主线程收尾：join 双泵 → 恢复终端 → 一行退场注记。余量（如实）：
    // QEMU 先死而 stdin 空闲时 writer 卡在 tty 阻塞读——最后一键任何
    // 字节即收尾（旗标短路/写死 socket 得错退）或 Ctrl-D 直退；std 无
    // 带超时 join，此边界接受。
    let _ = reader.join();
    let _ = writer.join();
    restore_tty(&saved_tty);
    eprintln!("console: disconnected（run 继续运行；berry logs 可查历史）");
    Ok(())
}

// ---- 件2 第 2 步：run 目录 console.lock 独占锁。chardev 单活客户端，
// ---- 第二连接只是不被服务的僵尸（qemu.rs serial_chardev_args 注记）
// ---- ——flock 把静默僵尸升格为显式拒绝。锁文件生命周期 = 命令进程持
// ---- fd 到退出（drop 关 fd 即释放；0 长文件留 run 目录无害，cleanup
// ---- 随 run 删）。
#[cfg(unix)]
fn lock_console(run: &Path) -> Result<std::fs::File> {
    use std::os::fd::AsRawFd;
    let lock = run.join("console.lock");
    let f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&lock)?;
    // 拿不到先复探再定性（3×50ms）：close 后的锁释放可有 vnode 级滞后
    // （macOS 并行负载下实测偶发）——前会话刚退、紧接重进时瞬时
    // EWOULDBLOCK 会误报「已有会话」。复探仍拒才是真持有（真持有方
    // 多等 100ms 才收拒，换报错可信度）。
    for attempt in 0..3 {
        // SAFETY: flock 只对我们自己 open 的 fd 施加 LOCK_EX|LOCK_NB
        // ——不阻塞、不写文件、不碰他进程的锁（拿不到返
        // -1/EWOULDBLOCK）。
        let rc = unsafe {
            console_ffi::flock(f.as_raw_fd(), console_ffi::LOCK_EX | console_ffi::LOCK_NB)
        };
        if rc == 0 {
            return Ok(f);
        }
        if attempt < 2 {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    Err(BerryError::device(format!(
        "console session already active for {} (console.lock held)",
        run.display()
    ))
    .with_hint("另一个终端正持该 run 的 console；在那边 Ctrl-D 退出后再来"))
}

// ---- 件2 第 3 步：连接 serial.sock。chardev socket backlog=1 有偶发
// ---- 拒连窗（件1 侦察实证 5 连 4 即成）——0.5s sleep 重试 ≤8 次仍败
// ---- 才报错。重试参数可注入是单测面（生产 8×500ms 不可入测）。
#[cfg(unix)]
fn connect_serial(sock: &Path) -> Result<std::os::unix::net::UnixStream> {
    connect_with_retry(sock, 8, Duration::from_millis(500))
}

#[cfg(unix)]
fn connect_with_retry(
    sock: &Path,
    retries: u32,
    delay: Duration,
) -> Result<std::os::unix::net::UnixStream> {
    let mut last = String::new();
    for attempt in 0..=retries {
        if attempt > 0 {
            std::thread::sleep(delay);
        }
        match std::os::unix::net::UnixStream::connect(sock) {
            Ok(s) => return Ok(s),
            Err(e) => last = e.to_string(),
        }
    }
    Err(BerryError::device(format!(
        "cannot connect to console endpoint {} ({last})",
        sock.display()
    ))
    .with_hint(
        "serial.sock 在但拒连——run 可能已死（berry device status 复核）或 backlog 偶发窗（已 8×0.5s 重试）",
    ))
}

// ---- 件2 第 4 步：TTY raw（仅 stdin 是 TTY 时——std::io::IsTerminal；
// ---- 管道驱动面跳过：EOF 即退出，无行编辑可扰）。Ctrl-C 注记：raw
// ---- 模式下 0x03 是字节转发——ISIG 清位后宿主 tty 不再把它变 SIGINT
// ---- 截杀本进程，字节直达客机 bsh 行编辑（bsh Ctrl-C 杀链真语义）；
// ---- 本命令自己的退出键是 Ctrl-D（0x04，写泵识别即断开）。保存原
// ---- termios 退出恢复。写法贴 windows_console.rs 房内 FFI 先例。
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn stdin_raw() -> Option<termios_ffi::Termios> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return None;
    }
    let mut t = termios_ffi::zeroed();
    // SAFETY: tcgetattr 只往我们的 Termios 镜像里写（fd 0 读取语义，
    // 无副作用）。
    if unsafe { termios_ffi::tcgetattr(0, &mut t) } != 0 {
        return None;
    }
    let saved = t;
    termios_ffi::set_raw(&mut t);
    // SAFETY: tcsetattr 把改后的镜像写回 fd 0（TCSANOW 立即生效）。
    // 失败不致命——退化为行缓冲，console 仍可用。
    if unsafe { termios_ffi::tcsetattr(0, termios_ffi::TCSANOW, &t) } != 0 {
        return None;
    }
    Some(saved)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn restore_tty(saved: &Option<termios_ffi::Termios>) {
    let Some(saved) = saved else { return };
    // SAFETY: 写回 tcgetattr 存下的原值（尽力而为——进程随后即退，
    // 极端残留 raw 态的坏终端可 `reset` 修）。
    unsafe { let _ = termios_ffi::tcsetattr(0, termios_ffi::TCSANOW, saved); }
}

// 其他 Unix 宿主（非 macOS/Linux）：termios 镜像未手写，不开 raw——
// console 仍通（行缓冲+回显的代价），诚实降级。
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn stdin_raw() -> Option<std::convert::Infallible> {
    None
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn restore_tty(_saved: &Option<std::convert::Infallible>) {}

// ---- 件2 的 FFI 面（写法贴 windows_console.rs:13-24 房内先例：显式
// ---- #[link] + extern 块、调用点逐处 SAFETY 注记）。std 无 flock 与
// ---- termios——run 独占锁与 TTY raw 直链 libc。
#[cfg(unix)]
mod console_ffi {
    // flock(2) 选项位（macOS/Linux 同值）：LOCK_EX 独占、LOCK_NB 拿不
    // 到即返 -1（EWOULDBLOCK）不挂起。
    pub const LOCK_EX: i32 = 2;
    pub const LOCK_NB: i32 = 4;

    #[link(name = "c")]
    extern "C" {
        pub fn flock(fd: i32, operation: i32) -> i32;
    }
}

// termios 最小镜像（件2 第 4 步）：按宿主布局手写——macOS aarch64 与
// Linux glibc 两形。safe 面只动 c_lflag 与 c_cc[VMIN/VTIME]，其余字段
// 由 tcgetattr/tcsetattr 经裸指针原样往返（编译器看不见该访问面，整
// 结构 allow(dead_code)）。
#[cfg(all(unix, any(target_os = "macos", target_os = "linux")))]
mod termios_ffi {
    // tcsetattr 动作位：立即生效（两平台同值 0）。
    pub const TCSANOW: i32 = 0;

    // macOS 布局：tcflag_t=u64 ×4 + c_cc[20]（NCCS=20）+ speed ×2。
    // c_cc 下标：VMIN=16 VTIME=17；c_lflag 位：ICANON=0x100 ECHO=0x8
    // ISIG=0x40。
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Termios {
        pub c_iflag: u64,
        pub c_oflag: u64,
        pub c_cflag: u64,
        pub c_lflag: u64,
        pub c_cc: [u8; 20],
        pub c_ispeed: u64,
        pub c_ospeed: u64,
    }

    // Linux glibc 布局：tcflag_t=u32 ×4 + c_line + c_cc[32]（NCCS=32）
    // + speed ×2。c_cc 下标：VMIN=6 VTIME=5；c_lflag 位：ICANON=0x2
    // ECHO=0x8 ISIG=0x1。
    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Termios {
        pub c_iflag: u32,
        pub c_oflag: u32,
        pub c_cflag: u32,
        pub c_lflag: u32,
        pub c_line: u8,
        pub c_cc: [u8; 32],
        pub c_ispeed: u32,
        pub c_ospeed: u32,
    }

    #[link(name = "c")]
    extern "C" {
        pub fn tcgetattr(fd: i32, t: *mut Termios) -> i32;
        pub fn tcsetattr(fd: i32, action: i32, t: *const Termios) -> i32;
    }

    pub fn zeroed() -> Termios {
        // SAFETY: 全零位模式对纯整型/字节数组结构有效（tcgetattr 随后
        // 整体覆写）。
        unsafe { std::mem::zeroed() }
    }

    /// raw 语义（件2 规格）：c_lflag 清 ICANON|ECHO，c_cc[VMIN]=1
    /// c_cc[VTIME]=0（逐字节即时读）。ISIG 一并清——Ctrl-C（0x03）在
    /// raw 模式是字节转发，宿主 tty 不得把它变 SIGINT 截杀本进程
    /// （语义注记见 stdin_raw）。
    pub fn set_raw(t: &mut Termios) {
        #[cfg(target_os = "macos")]
        {
            t.c_lflag &= !(0x0000_0100 | 0x0000_0008 | 0x0000_0040);
            t.c_cc[16] = 1; // VMIN
            t.c_cc[17] = 0; // VTIME
        }
        #[cfg(target_os = "linux")]
        {
            t.c_lflag &= !(0x2 | 0x8 | 0x1);
            t.c_cc[6] = 1; // VMIN
            t.c_cc[5] = 0; // VTIME
        }
    }
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
    let mut best_vnc: Option<(std::time::SystemTime, PathBuf)> = None;
    let mut best_any: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(rd) = std::fs::read_dir(&root) {
        for e in rd.flatten() {
            let p = e.path();
            // 合法 run 目录 = meta.json 在（create 即写）。M2 第 11 步前过滤
            // 是 vnc 端点在席——run/shot 类 run 目录（无 VNC 端点文件）永远
            // 进不了候选，`berry logs` 无参必报 no running device。
            if !p.is_dir() || !p.join("meta.json").is_file() {
                continue;
            }
            let t = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            // 五轮挂账收敛：残留 sock 探活——QEMU 退出后 unix sock 文件
            // 残留但 connect 拒，死 run 不再进 best_vnc 候选（TCP 端口形态
            // 不预探——会占连接；维持端点文件判定）。
            let has_vnc = vnc_endpoint(&p).is_ok() && vnc_reachable(&p);
            if has_vnc && best_vnc.as_ref().is_none_or(|(bt, _)| t > *bt) {
                best_vnc = Some((t, p.clone()));
            }
            if best_any.as_ref().is_none_or(|(bt, _)| t > *bt) {
                best_any = Some((t, p));
            }
        }
    }
    // 活设备（VNC 端点）优先；无 VNC run 时回落最新 run 目录——logs 等读
    // 面直接可用（文件在即读），VNC 类命令对回落 run 在 vnc_endpoint 报
    // 明确的 no VNC endpoint。
    // 审计补刀：回落命中时 stderr 提示——无活设备时 berry logs 无参回放
    // 的是死 run 旧 serial.log（旧版报 no running device），静默成功比报
    // 错更误导。
    if best_vnc.is_none() {
        if let Some((_, p)) = &best_any {
            eprintln!("berry: no live device; fell back to latest run dir {}", p.display());
        }
    }
    best_vnc
        .or(best_any)
        .map(|(_, p)| p)
        .ok_or_else(|| {
            BerryError::env("no run directory found")
                .with_hint("start one with `berry shell` / `berry run`")
        })
}

/// unix sock 探活：connect 预探（QEMU 在 = 握手成；死后残留文件 = 拒连）。
pub(crate) fn vnc_reachable(run: &Path) -> bool {
    #[cfg(unix)]
    {
        let sock = run.join("vnc.sock");
        if sock.exists() {
            // 六轮 F1 副作用缓解：QEMU VNC 监听 backlog=1——探活 connect+close
            // 占 accept 队列的 <0.5ms 窗内本连接自身可能吃 ECONNREFUSED
            //（背靠背实测 29/40）——refused 时延时 2ms 重试一次再判死，防
            // 活 run 被瞬时误滤。副作用注记：探活会让 QEMU 发一对
            // vnc.connected/disconnected 事件进 events.jsonl（accept 即发，
            // 无真会话对应）——消费侧知悉即可。
            return std::os::unix::net::UnixStream::connect(&sock).is_ok()
                || {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    std::os::unix::net::UnixStream::connect(&sock).is_ok()
                };
        }
    }
    true
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

// ---- 四轮审计（D4 序·测试续建）：find_run 两级回落护栏——vnc 在席目录
// ---- 优先于更新的无 vnc 目录 / 全无 vnc 回落最新 meta.json / 无
// ---- meta.json 被滤 / 空 root 报错。BERRY_HOME 是进程级 env，经
// ---- run_dir::ENV_LOCK 与其他 env 触碰测试串行；mtime 用 set_modified
// ---- 钉死序（目录自身 mtime 是 find_run 的排序键）。
#[cfg(unix)]
#[cfg(test)]
mod find_run_tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    /// 造一个 run 候选目录：meta.json 在 = 合法候选；vnc 桌 = 真监听 unix
    /// socket（六轮审计 F1 实抓：普通文件假 sock 被 vnc_reachable 的
    /// connect 预探判 ENOTSOCK → vnc 级候选永远选不出、断言必败且 panic
    /// 泄漏 BERRY_HOME 连环污染 env 测试）；mtime 钉到 base+off 秒（目录
    /// mtime 是排序键，set_modified 免真实时间流逝的偶发）。返回 (目录,
    /// listener)——listener 须持有到断言后（drop 即 socket 消失）。
    fn mk_run(
        root: &Path,
        name: &str,
        off: u64,
        meta: bool,
        vnc: bool,
    ) -> (PathBuf, Option<std::os::unix::net::UnixListener>) {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        if meta {
            std::fs::write(d.join("meta.json"), "{}").unwrap();
        }
        let mut listener = None;
        if vnc {
            let l = std::os::unix::net::UnixListener::bind(d.join("vnc.sock")).unwrap();
            listener = Some(l);
        }
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + off);
        std::fs::File::open(&d).unwrap().set_modified(t).unwrap();
        (d, listener)
    }

    fn tmp_home(tag: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("berry-findrun-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("run")).unwrap();
        std::env::set_var("BERRY_HOME", &tmp);
        tmp
    }

    #[test]
    fn vnc_dir_wins_over_newer_non_vnc() {
        let _g = crate::run_dir::ENV_LOCK.lock().unwrap();
        let tmp = tmp_home("vnc-wins");
        let root = tmp.join("run");
        let (older_vnc, _l) = mk_run(&root, "100-a", 100, true, true);
        let (_newer, _nl) = mk_run(&root, "200-b", 200, true, false);
        assert_eq!(find_run(None).unwrap(), older_vnc);
        std::env::remove_var("BERRY_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn all_no_vnc_falls_back_to_newest_meta() {
        let _g = crate::run_dir::ENV_LOCK.lock().unwrap();
        let tmp = tmp_home("fallback");
        let root = tmp.join("run");
        let (_older, _ol) = mk_run(&root, "100-a", 100, true, false);
        let (newer, _nl) = mk_run(&root, "200-b", 200, true, false);
        assert_eq!(find_run(None).unwrap(), newer);
        std::env::remove_var("BERRY_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn dirs_without_meta_are_filtered() {
        let _g = crate::run_dir::ENV_LOCK.lock().unwrap();
        let tmp = tmp_home("no-meta");
        let root = tmp.join("run");
        let (older, _ol) = mk_run(&root, "100-a", 100, true, false);
        // 更新的目录无 meta.json（残目录）：被滤，不夺回落位。
        let (_newest_nometa, _nnl) = mk_run(&root, "300-c", 300, false, false);
        assert_eq!(find_run(None).unwrap(), older);
        std::env::remove_var("BERRY_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn empty_root_errors() {
        let _g = crate::run_dir::ENV_LOCK.lock().unwrap();
        let tmp = tmp_home("empty");
        assert!(find_run(None).is_err());
        std::env::remove_var("BERRY_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

// ---- 五轮后收敛批：cleanup 探活内核的纯函数护栏。----
#[cfg(test)]
mod berry_alive_tests {
    use super::berry_pid_alive;

    #[test]
    fn pid_zero_is_dead() {
        // meta 缺/坏的残目录语义：pid 0 一律死（可清）。
        assert!(!berry_pid_alive(0));
    }

    #[test]
    fn live_and_dead_process() {
        // unix：起 sleep 子进程当活 pid；杀掉收尸后当死 pid。
        #[cfg(unix)]
        {
            use std::process::{Command, Stdio};
            let mut child = Command::new("sleep")
                .arg("30")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn sleep");
            let pid = child.id() as u64;
            assert!(berry_pid_alive(pid), "活子进程应判活");
            child.kill().expect("kill");
            child.wait().expect("wait");
            assert!(!berry_pid_alive(pid), "已收尸子进程应判死");
        }
    }
}

// ---- 六轮审计 F1 收敛：vnc_reachable 探活的纯文件系统三例。----
#[cfg(test)]
mod vnc_reachable_tests {
    use super::*;

    #[test]
    fn live_socket_true_dead_file_false_absent_true() {
        let tmp = std::env::temp_dir().join(format!("berry-vncreach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // 无 vnc.sock：TCP 形态语义——不预探，恒 true。
        assert!(vnc_reachable(&tmp));
        // 活 listener：connect 握手成 → true。
        let _l = std::os::unix::net::UnixListener::bind(tmp.join("vnc.sock")).unwrap();
        assert!(vnc_reachable(&tmp));
        // 死残留（普通文件冒充）：connect 得 ENOTSOCK → false。
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("vnc.sock"), "").unwrap();
        assert!(!vnc_reachable(&tmp));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

// ---- 件2 护栏：可单测面 = 锁语义（flock 锁在 open file description
// ---- 上，同进程两个独立 open 的 fd 即互斥——无需跨进程 mock）与
// ---- 前置/连接参数面（旧形态报错、重试注入）。socket 泵与 TTY raw
// ---- 需真 QEMU/真终端，不可单测——如实记，验证走实机交互冒烟。
#[cfg(all(unix, test))]
mod console_tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn console_lock_excludes_second_holder_until_drop() {
        let tmp = std::env::temp_dir().join(format!("berry-console-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let l1 = lock_console(&tmp).unwrap();
        // 第二持有者（独立 open 的 fd）：LOCK_EX|LOCK_NB 必拒。
        let err = lock_console(&tmp).unwrap_err();
        assert!(err.message.contains("already active"), "{}", err.message);
        assert!(err.hint.is_some_and(|h| h.contains("console")));
        // drop 关 fd 即释放：可重取（console.lock 文件留存无害）。close
        // 后的释放可有 vnode 级滞后（macOS 并行负载下实测偶发）——复探
        // ≤2s，语义断言「最终可重取」而非瞬时。
        drop(l1);
        let mut l2 = None;
        for _ in 0..200 {
            match lock_console(&tmp) {
                Ok(f) => {
                    l2 = Some(f);
                    break;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        let _l2 = l2.expect("drop 释放后 2s 内应可重取 console.lock");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn old_style_run_without_sock_errors_with_restart_hint() {
        let tmp = std::env::temp_dir().join(format!("berry-console-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("meta.json"), "{}").unwrap();
        let err = console_attach(&tmp).unwrap_err();
        assert!(err.message.contains("无 console 端点"), "{}", err.message);
        let hint = err.hint.unwrap();
        assert!(hint.contains("restart"), "{hint}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn connect_retry_dead_sock_fails_after_injected_retries() {
        let tmp = std::env::temp_dir().join(format!("berry-console-dead-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // 普通文件冒充 sock（run 死后残留形态）：connect 必拒——重试
        // 2×1ms 快速失败（生产 8×0.5s 的参数面不可入测，形态面等价）。
        std::fs::write(tmp.join("serial.sock"), "").unwrap();
        let err =
            connect_with_retry(&tmp.join("serial.sock"), 2, Duration::from_millis(1)).unwrap_err();
        assert!(err.message.contains("cannot connect"), "{}", err.message);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn connect_retry_live_listener_first_attempt() {
        let tmp = std::env::temp_dir().join(format!("berry-console-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let _l = UnixListener::bind(tmp.join("serial.sock")).unwrap();
        // 活 listener：0 次重试参数下首连即成（不 sleep）。
        connect_with_retry(&tmp.join("serial.sock"), 0, Duration::from_millis(0)).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

// ---- W1-Q25 八刀：commands.rs 纯函数面钉板（fnv1a64 / qcode 表 /
// parse_mode / read_end_kept / expect_step 文件面）。----
#[cfg(test)]
mod fnv1a64_tests {
    use super::fnv1a64;

    /// FNV-1a 64 官方向量（空串 = offset basis 常数自证——脚本侧 expect
    /// 步的 fnv1a64 锚值全部由本函数产出，表错即静默错判）。
    #[test]
    fn official_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325, "空串 = 位移基");
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
        assert_eq!(fnv1a64(b"hello world"), 0x779a_65e7_023c_d2e7);
    }

    /// 字节敏感性与顺序律：单字节差必改哈希；同字节集不同序必不同；
    /// 末字节翻转必改（expect 锚值防碰撞最小面）。
    #[test]
    fn byte_sensitivity_and_order() {
        assert_ne!(fnv1a64(b"a"), fnv1a64(b"b"));
        assert_ne!(fnv1a64(b"ab"), fnv1a64(b"ba"));
        assert_ne!(fnv1a64(b"\x00\x01\x02"), fnv1a64(b"\x00\x01\x03"));
        assert_ne!(fnv1a64(&[0xfe, 0xff]), fnv1a64(&[0xff, 0xfe]));
    }
}

#[cfg(test)]
mod qcode_table_tests {
    use super::qcode_to_keysym;

    /// 全表钉板（含别名对 ret=enter、spc=space）——错值/漏键在运行期
    /// 表现为 VNC 侧按键错乱，无编译期信号。
    #[test]
    fn full_named_table() {
        let cases: &[(&str, u32)] = &[
            ("ret", 0xff0d),
            ("enter", 0xff0d),
            ("esc", 0xff1b),
            ("backspace", 0xff08),
            ("tab", 0xff09),
            ("spc", 0x20),
            ("space", 0x20),
            ("left", 0xff51),
            ("up", 0xff52),
            ("right", 0xff53),
            ("down", 0xff54),
            ("delete", 0xffff),
            ("home", 0xff50),
            ("end", 0xff57),
            ("minus", 0x2d),
            ("equal", 0x3d),
            ("comma", 0x2c),
            ("dot", 0x2e),
            ("slash", 0x2f),
            ("semicolon", 0x3b),
            ("apostrophe", 0x27),
            ("bracket_left", 0x5b),
            ("bracket_right", 0x5d),
            ("backslash", 0x5c),
            ("grave_accent", 0x60),
        ];
        for (q, want) in cases {
            assert_eq!(qcode_to_keysym(q), Some(*want), "qcode {q}");
        }
    }

    /// 单字符面：可打印 ASCII 恒等；多字符未知词 / 空串拒绝。
    #[test]
    fn char_face_and_rejects() {
        assert_eq!(qcode_to_keysym("a"), Some(0x61));
        assert_eq!(qcode_to_keysym("Q"), Some(0x51));
        assert_eq!(qcode_to_keysym("5"), Some(0x35));
        assert_eq!(qcode_to_keysym("é"), Some(0xe9));
        assert_eq!(qcode_to_keysym(""), None);
        assert_eq!(qcode_to_keysym("ab"), None);
        assert_eq!(qcode_to_keysym("notakey"), None);
        assert_eq!(qcode_to_keysym("Return"), None, "大小写敏感");
    }
}

#[cfg(test)]
mod parse_mode_tests {
    use super::parse_mode;

    /// 接受梯：常规 / 最小 1x1 / 上界恰 8192。
    #[test]
    fn accept_ladder() {
        assert_eq!(parse_mode("800x600"), Some((800, 600)));
        assert_eq!(parse_mode("1x1"), Some((1, 1)));
        assert_eq!(parse_mode("8192x8192"), Some((8192, 8192)));
    }

    /// 拒面全家桶：零维 / 越上界 / 非数字 / 缺分隔 / 多余段 / 大写 X /
    /// 负数 / 空白混入。
    #[test]
    fn reject_family() {
        assert_eq!(parse_mode("0x600"), None, "零宽");
        assert_eq!(parse_mode("800x0"), None, "零高");
        assert_eq!(parse_mode("8193x600"), None, "越上界");
        assert_eq!(parse_mode("800x8193"), None);
        assert_eq!(parse_mode("800.5x600"), None, "非整数");
        assert_eq!(parse_mode("800"), None, "缺分隔");
        assert_eq!(parse_mode("x600"), None);
        assert_eq!(parse_mode("800x"), None);
        assert_eq!(parse_mode("800x600x24"), None, "多余段");
        assert_eq!(parse_mode("800X600"), None, "大写 X 不认");
        assert_eq!(parse_mode("-1x600"), None, "负数");
        assert_eq!(parse_mode("800 x600"), None, "空白混入");
        assert_eq!(parse_mode(""), None);
    }
}

#[cfg(test)]
mod read_end_kept_tests {
    use super::read_end_kept;
    use crate::run_dir::ENV_LOCK;
    use std::io::Write as _;

    fn sandbox(tag: &str) -> std::path::PathBuf {
        let tmp = std::env::temp_dir().join(format!("berry-endkept-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("run")).unwrap();
        std::env::set_var("BERRY_HOME", &tmp);
        tmp
    }

    /// 尾向扫描契约：同 id 多条 run.end 取最后一条；他 id 行跳过；
    /// 坏 JSON 行容错不炸。
    #[test]
    fn latest_end_line_wins_and_tolerates_corrupt() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = sandbox("latest");
        let idx = tmp.join("run").join("index.jsonl");
        let mut f = std::fs::File::create(&idx).unwrap();
        writeln!(
            f,
            r#"{{"v":1,"t":"run.start","run":"r1","label":"t"}}"#
        )
        .unwrap();
        writeln!(f, r#"{{"v":1,"t":"run.end","run":"r0","kept":false}}"#).unwrap();
        writeln!(f, "{{oops").unwrap();
        writeln!(f, r#"{{"v":1,"t":"run.end","run":"r1","kept":false}}"#).unwrap();
        writeln!(f, r#"{{"v":1,"t":"run.start","run":"r2"}}"#).unwrap();
        writeln!(f, r#"{{"v":1,"t":"run.end","run":"r1","kept":true}}"#).unwrap();
        drop(f);
        assert_eq!(read_end_kept("r1"), Some(true), "同 id 取最后一条");
        assert_eq!(read_end_kept("r0"), Some(false), "他 id 正常取值");
        assert_eq!(read_end_kept("r2"), None, "只有 start 无 end");
        assert_eq!(read_end_kept("rX"), None, "无匹配 id");
        std::env::remove_var("BERRY_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 缺席面：kept 字段缺 → None；index 文件整缺 → None（宿主崩溃漏
    /// 写形态，枚举以扫目录兜底——本函数只管证词缺席）。
    #[test]
    fn kept_field_or_index_absent_is_none() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = sandbox("absent");
        let idx = tmp.join("run").join("index.jsonl");
        std::fs::write(&idx, r#"{"v":1,"t":"run.end","run":"r9"}"#).unwrap();
        assert_eq!(read_end_kept("r9"), None, "kept 字段缺");
        std::env::remove_var("BERRY_HOME");
        // index 缺席：换一个空 home 再探。
        let tmp2 = sandbox("absent2");
        std::fs::remove_file(tmp2.join("run").join("index.jsonl")).ok();
        assert_eq!(read_end_kept("r9"), None, "index 文件缺");
        std::env::remove_var("BERRY_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&tmp2);
    }
}

#[cfg(test)]
mod expect_step_file_tests {
    use super::expect_step;
    use serde_json::json;
    use std::io::Write as _;

    /// 文件面绿路：file + min_bytes + fnv1a64（0x 前缀与裸十六进制双形
    /// 态）全过。run 路径在纯 file 步不触（state 才走 control）。
    #[test]
    fn file_face_green_both_hex_forms() {
        let tmp = std::env::temp_dir().join(format!("berry-expect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let f = tmp.join("shot.ppm");
        let mut fh = std::fs::File::create(&f).unwrap();
        fh.write_all(b"foobar").unwrap();
        drop(fh);
        let run = tmp.join("no-such-run");
        let args = json!({"file": f.to_str().unwrap(), "min_bytes": 3,
                          "fnv1a64": "0x85944171f73967e8"});
        expect_step(&run, &args, 1).unwrap();
        let args = json!({"file": f.to_str().unwrap(),
                          "fnv1a64": "85944171f73967e8"});
        expect_step(&run, &args, 2).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 拒面：min_bytes 不足与 fnv 不匹配均 device 码 6 且消息带回显；
    /// 坏 fnv 字面量与空 args 均 usage 码 2。
    #[test]
    fn reject_faces_and_codes() {
        let tmp = std::env::temp_dir().join(format!("berry-expect2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let f = tmp.join("data.bin");
        std::fs::write(&f, b"foobar").unwrap();
        let run = tmp.join("no-such-run");
        // min_bytes 不足：device 码，消息含实际字节数与门槛。
        let args = json!({"file": f.to_str().unwrap(), "min_bytes": 100});
        let e = expect_step(&run, &args, 3).unwrap_err();
        assert_eq!(e.code, 6, "{}", e.message);
        assert!(e.message.contains("bytes < 100"), "{}", e.message);
        // fnv 不匹配：device 码，消息两侧十六进制都在（定位用）。
        let args = json!({"file": f.to_str().unwrap(),
                          "fnv1a64": "0x0000000000000001"});
        let e = expect_step(&run, &args, 4).unwrap_err();
        assert_eq!(e.code, 6, "{}", e.message);
        assert!(e.message.contains("85944171f73967e8"), "{}", e.message);
        // 坏 fnv 字面量：usage 码 2。
        let args = json!({"file": f.to_str().unwrap(), "fnv1a64": "zz"});
        let e = expect_step(&run, &args, 5).unwrap_err();
        assert_eq!(e.code, 2, "{}", e.message);
        assert!(e.message.contains("bad fnv1a64"), "{}", e.message);
        // 既无 state 又无 file：usage 码 2。
        let e = expect_step(&run, &json!({}), 6).unwrap_err();
        assert_eq!(e.code, 2, "{}", e.message);
        assert!(e.message.contains("needs state or file"), "{}", e.message);
        // 文件缺席（state 未给、file 给但读不到）：device 码。
        let args = json!({"file": tmp.join("missing.bin").to_str().unwrap()});
        let e = expect_step(&run, &args, 7).unwrap_err();
        assert_eq!(e.code, 6, "{}", e.message);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

// ---- W1-Q26 十八刀（run 定位决策面增量）：显式 id 三态（目录 id 命中/
// ---- 绝对路径直用/缺席 env 码 + 拼路径文案）+ 死 sock 集成面（占位普通
// ---- 文件 vnc.sock 探活拒——不进 best_vnc、回落最新死 run）+
// ---- vnc_endpoint 三态（sock 优先/port 次之/双缺 env 码带路径）+
// ---- vnc_reachable 三态（listen 真/占位文件假/无 sock 恒真）。
// ---- 既有 find_run_tests 已覆盖枚举回落四面（vnc 优先/回落最新/
// ---- no-meta 滤/空 root 拒），本模块只补零测增量面。
#[cfg(test)]
mod run_locate_tests {
    use super::{find_run, vnc_endpoint, vnc_reachable};
    use crate::run_dir::ENV_LOCK;
    use std::io::Write as _;
    use std::os::unix::net::UnixListener;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    fn tmp_home(tag: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!(
            "berry-runloc-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(tmp.join("run")).unwrap();
        std::env::set_var("BERRY_HOME", &tmp);
        tmp
    }

    fn cleanup(tmp: &Path) {
        std::env::remove_var("BERRY_HOME");
        let _ = std::fs::remove_dir_all(tmp);
    }

    /// run 目录（meta.json 在）+ mtime 钉 base+off 秒（set_modified 免
    /// 真实时间流逝偶发——find_run_tests 同款纪律）。
    fn mk_run(root: &Path, name: &str, off: u64) -> PathBuf {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("meta.json"), b"{}").unwrap();
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + off);
        std::fs::File::open(&d).unwrap().set_modified(t).unwrap();
        d
    }

    /// 显式 id 三态：run 根下 id 直命中；绝对路径形态直用（is_dir 门）；
    /// 缺席 env 码 3 且消息带拼出的完整路径。
    #[test]
    fn explicit_id_dir_path_and_missing() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tmp_home("explicit");
        let r1 = mk_run(&tmp.join("run"), "r1", 100);
        assert_eq!(find_run(Some("r1")).unwrap(), r1, "id 形态拼 run 根");
        assert_eq!(
            find_run(Some(r1.to_str().unwrap())).unwrap(),
            r1,
            "目录路径直用"
        );
        let e = find_run(Some("nope")).unwrap_err();
        assert_eq!(e.code, 3, "{}", e.message);
        assert!(e.message.contains("run not found"), "{}", e.message);
        assert!(
            e.message.contains(tmp.join("run").join("nope").to_str().unwrap()),
            "拼路径入文案: {}",
            e.message
        );
        cleanup(&tmp);
    }

    /// 死 sock 集成面：QEMU 崩溃现场形态——vnc.sock 残留为普通文件，
    /// 探活 connect 拒 → 不进 best_vnc 候选，回落最新死 run。
    #[test]
    #[cfg(unix)]
    fn stale_socket_file_falls_back_to_newest() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tmp_home("stale");
        let root = tmp.join("run");
        let r1 = mk_run(&root, "100-a", 100);
        let r2 = mk_run(&root, "200-b", 200);
        // r1 造残留死 sock（占位普通文件——connect ENOTSOCK/ECONNREFUSED）。
        // 写 sock 后 r1 目录 mtime 滚到当下（最新）——探活拒后它仍该走
        // best_any 回落路径胜出，正钉「死 sock 不夺回落位」的字面行为。
        std::fs::write(r1.join("vnc.sock"), b"stale").unwrap();
        assert!(!vnc_reachable(&r1), "占位文件 sock 探活必拒");
        assert_eq!(find_run(None).unwrap(), r1, "死 sock 回落最新死 run（写后最新）");
        cleanup(&tmp);
    }

    /// vnc_endpoint 三态：unix sock 文件优先；vnc.port 次之（trim 生效）；
    /// 双缺 env 码且消息带 run 路径。
    #[test]
    #[cfg(unix)]
    fn vnc_endpoint_sock_port_absent() {
        let tmp = std::env::temp_dir().join(format!("berry-vncep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let e = vnc_endpoint(&tmp).unwrap_err();
        assert_eq!(e.code, 3, "{}", e.message);
        assert!(e.message.contains("no VNC endpoint"), "{}", e.message);
        assert!(e.message.contains(tmp.to_str().unwrap()), "{}", e.message);
        // vnc.port 形态：TCP 端点（尾换行 trim 后拼 127.0.0.1:5944）。
        let mut f = std::fs::File::create(tmp.join("vnc.port")).unwrap();
        writeln!(f, "5944").unwrap();
        drop(f);
        match vnc_endpoint(&tmp) {
            Ok(crate::net::Endpoint::Tcp(a)) => assert_eq!(a, "127.0.0.1:5944"),
            other => panic!("port 形态应产 TCP 端点: {other:?}"),
        }
        // vnc.sock 在席优先于 port（判序 sock → port）。
        std::fs::write(tmp.join("vnc.sock"), b"").unwrap();
        match vnc_endpoint(&tmp) {
            Ok(crate::net::Endpoint::Unix(p)) => assert_eq!(p, tmp.join("vnc.sock")),
            other => panic!("sock 形态应产 Unix 端点: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// vnc_reachable 三态：listen 中的 sock 探活命中真；占位普通文件假；
    /// 无 sock 文件恒真（TCP/无端点形态不预探）。
    #[test]
    #[cfg(unix)]
    fn vnc_reachable_listen_placeholder_absent() {
        let tmp = std::env::temp_dir().join(format!("berry-vncre-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(vnc_reachable(&tmp), "无 sock 文件不预探恒真");
        std::fs::write(tmp.join("vnc.sock"), b"stale").unwrap();
        assert!(!vnc_reachable(&tmp), "普通文件 sock 必拒");
        std::fs::remove_file(tmp.join("vnc.sock")).unwrap();
        let _l = UnixListener::bind(tmp.join("vnc.sock")).unwrap();
        assert!(vnc_reachable(&tmp), "listen 中的 sock 探活命中");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
