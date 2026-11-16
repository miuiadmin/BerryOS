#![cfg_attr(all(windows, feature = "gui"), windows_subsystem = "windows")]

//! `berry` — one binary, two faces: no args on a desktop opens the device
//! shell; subcommands are the headless developer/CI surface.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use clap::{CommandFactory, Parser, Subcommand};

use berry::commands::{
    self, DeviceStartArgs, InputAction, InputArgs, RunArgs, ScaleMode,
    ScreenshotArgs, ShellArgs, ShotArgs,
};
use berry::error::{BerryError, Result};

#[cfg(all(windows, not(feature = "gui")))]
mod windows_console;

#[derive(Parser, Debug)]
#[command(
    name = "berry",
    version,
    about = "BerryOS developer device shell",
    disable_help_subcommand = true
)]
struct Cli {
    /// Emit JSONL events on stdout; human logs go to stderr.
    #[arg(long, global = true)]
    json: bool,
    /// Verbose diagnostics on stderr.
    #[arg(long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Check QEMU, firmware, image, kernel, accel.
    Doctor,
    /// Boot a kernel headlessly, optionally wait for a serial marker.
    Run(RunCmd),
    /// Boot the device image headlessly and capture one frame as PNG.
    Shot(ShotCmd),
    /// Capture one frame from a running device (does not boot a new one).
    Screenshot(ScreenshotCmd),
    /// Session management (start/list/status/restart/stop/cleanup).
    Device(DeviceCmd),
    /// Power actions on a running device.
    Power(PowerCmd),
    /// Read the session serial log.
    Logs(LogsCmd),
    /// Attach the host terminal to the guest serial console (bsh).
    Console(ConsoleCmd),
    /// Display/profile operations.
    Display(DisplayCmd),
    /// Trigger a window-style shortcut action.
    Shortcut(ShortcutCmd),
    /// Read session events.
    Events(EventsCmd),
    /// Show session state/profile/memory/QEMU pid.
    Stats(StatsCmd),
    /// Capture a PNG sequence or APNG from a running device.
    Record(RecordCmd),
    /// Run a JSON action script against a running device.
    Play(PlayCmd),
    /// Internal headless session host (used by `device start`).
    #[command(hide = true)]
    Host(HostCmd),
    /// Open the phone-shaped device window (the double-click entry).
    Shell(ShellCmd),
    /// Inject input into a running device via VNC.
    Input(InputCmd),
    /// Top-level tap alias for `input tap`.
    Tap(TapCmd),
    /// Top-level key alias for `input key`.
    Key(KeyCmd),
    /// Top-level swipe alias for `input swipe`.
    Swipe(SwipeCmd),
    /// Top-level text alias for `input text`.
    Text(TextCmd),
    /// External VNC endpoint operations (status/passwd/check/open/web/certs).
    Vnc(VncCmd),
    /// Print version.
    Version,
    /// 打印 home() 根路径（recipe/脚本消费——justfile 侧消除 home() 逻辑复制）。
    Home,
}

#[derive(Parser, Debug)]
struct RunCmd {
    #[arg(long)]
    kernel: Option<PathBuf>,
    #[arg(long, default_value = "BerryOS boot")]
    marker: String,
    #[arg(long, default_value_t = 30)]
    timeout: u64,
    #[arg(long, default_value = "auto")]
    accel: String,
    #[arg(long)]
    memory: Option<u32>,
    #[arg(long)]
    follow: bool,
    #[arg(long)]
    keep: bool,
}

#[derive(Parser, Debug)]
struct ShotCmd {
    #[arg(long)]
    image: Option<PathBuf>,
    #[arg(long, default_value = "BERRYBOOT ok")]
    wait_marker: String,
    #[arg(long, default_value_t = 1000)]
    delay_ms: u64,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long, default_value = "desktop")]
    profile: String,
    #[arg(long, default_value = "auto")]
    accel: String,
    #[arg(long)]
    memory: Option<u32>,
    #[arg(long, default_value_t = 120)]
    timeout: u64,
    #[arg(long)]
    keep: bool,
    #[arg(long)]
    display_mode: Option<String>,
    /// Boot screen mode: splash (default, iOS-style B/W) or verbose (Linux-style scrolling log).
    #[arg(long)]
    boot_mode: Option<String>,
}

#[derive(Parser, Debug)]
struct ScreenshotCmd {
    /// Run id/dir; default = newest running device.
    #[arg(long)]
    run: Option<String>,
    #[arg(long)]
    out: Option<PathBuf>,
    /// Wait before capturing (desktop/animation settle).
    #[arg(long, default_value_t = 0)]
    settle_ms: u64,
    #[arg(long, default_value_t = 10)]
    timeout: u64,
}

#[derive(Parser, Debug)]
struct DisplayCmd {
    #[command(subcommand)]
    action: DisplayAction,
}

#[derive(Subcommand, Debug)]
enum DisplayAction {
    /// List profiles.
    List,
    /// Select a profile for the next start (requires restart).
    Set(DisplaySetCmd),
}

#[derive(Parser, Debug)]
struct DisplaySetCmd {
    profile: String,
    #[arg(long)]
    run: Option<String>,
    /// Apply now: stop the current host and start a new one with this profile.
    #[arg(long)]
    restart: bool,
}

#[derive(Parser, Debug)]
struct ShortcutCmd {
    #[arg(value_parser = ["screenshot", "resolution", "reboot", "power"])]
    action: String,
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct PlayCmd {
    file: PathBuf,
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct RecordCmd {
    #[arg(long)]
    run: Option<String>,
    #[arg(long, default_value = "record")]
    out: PathBuf,
    #[arg(long, default_value_t = 10)]
    frames: u32,
    #[arg(long, default_value_t = 200)]
    interval_ms: u64,
    /// Write an animated PNG instead of a PNG sequence.
    #[arg(long)]
    apng: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct StatsCmd {
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct EventsCmd {
    #[arg(long)]
    run: Option<String>,
    #[arg(long, default_value_t = 100)]
    tail: usize,
    #[arg(long, short = 'f')]
    follow: bool,
    /// Only print lines containing this substring (empty = no filter).
    #[arg(long)]
    filter: Option<String>,
}

#[derive(Parser, Debug)]
struct DeviceCmd {
    #[command(subcommand)]
    action: DeviceAction,
}

#[derive(Subcommand, Debug)]
enum DeviceAction {
    /// Start a headless session (default background; --foreground blocks).
    Start(StartCmd),
    /// List known sessions.
    List,
    /// Query a running session.
    Status(StatusCmd),
    /// Restart the guest (same session).
    Restart(RestartCmd),
    /// Stop a running session.
    Stop(StopCmd),
    /// Remove a stale/stopped session and kill an orphan QEMU if needed.
    Cleanup(CleanupCmd),
}

#[derive(Parser, Debug)]
struct StatusCmd {
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct StopCmd {
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct CleanupCmd {
    #[arg(long)]
    run: Option<String>,
    /// Remove every stale/stopped session (skip running).
    #[arg(long)]
    all: bool,
}

#[derive(Parser, Debug)]
struct PowerCmd {
    #[arg(value_parser = ["off", "reboot"])]
    action: String,
    #[arg(long)]
    run: Option<String>,
    #[arg(long)]
    force: bool,
}

#[derive(Parser, Debug)]
struct LogsCmd {
    #[arg(long)]
    run: Option<String>,
    #[arg(long, default_value_t = 100)]
    tail: usize,
    #[arg(long, short = 'f')]
    follow: bool,
}

#[derive(Parser, Debug)]
struct ConsoleCmd {
    /// Run id/dir; default = newest running device.
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct StartCmd {
    #[arg(long)]
    image: Option<PathBuf>,
    #[arg(long, default_value = "desktop")]
    profile: String,
    #[arg(long, default_value = "auto")]
    accel: String,
    #[arg(long)]
    memory: Option<u32>,
    #[arg(long)]
    display_mode: Option<String>,
    /// Boot screen mode: splash (default, iOS-style B/W) or verbose (Linux-style scrolling log).
    #[arg(long)]
    boot_mode: Option<String>,
    #[arg(long)]
    keep: bool,
    #[arg(long)]
    foreground: bool,
    /// Restart QEMU in the same run after an unexpected exit.
    #[arg(long)]
    restart_on_crash: bool,
    /// Expose an external VNC endpoint: off | on | <port> | <host>:<port>.
    #[arg(long)]
    vnc: Option<String>,
    /// WebSocket listener for noVNC: off | on | <port> (requires --vnc).
    #[arg(long)]
    vnc_ws: Option<String>,
    /// VNC password; loopback defaults to none, non-loopback auto-generates.
    /// Ignored when no endpoint is exposed (no --vnc and no BERRY_VNC).
    #[arg(long)]
    vnc_pass: Option<String>,
    /// TLS (VeNCrypt) cert dir for the endpoint (requires --vnc).
    #[arg(long)]
    vnc_tls: Option<String>,
}

#[derive(Parser, Debug)]
struct RestartCmd {
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct HostCmd {
    #[arg(long)]
    image: Option<PathBuf>,
    #[arg(long, default_value = "desktop")]
    profile: String,
    #[arg(long, default_value = "auto")]
    accel: String,
    #[arg(long)]
    memory: Option<u32>,
    #[arg(long)]
    display_mode: Option<String>,
    /// Boot screen mode: splash (default, iOS-style B/W) or verbose (Linux-style scrolling log).
    #[arg(long)]
    boot_mode: Option<String>,
    #[arg(long)]
    keep: bool,
    /// Restart QEMU in the same run after an unexpected exit.
    #[arg(long)]
    restart_on_crash: bool,
    /// Expose an external VNC endpoint: off | on | <port> | <host>:<port>.
    #[arg(long)]
    vnc: Option<String>,
    /// WebSocket listener for noVNC: off | on | <port> (requires --vnc).
    #[arg(long)]
    vnc_ws: Option<String>,
    /// VNC password; loopback defaults to none, non-loopback auto-generates.
    /// Ignored when no endpoint is exposed (no --vnc and no BERRY_VNC).
    #[arg(long)]
    vnc_pass: Option<String>,
    /// TLS (VeNCrypt) cert dir for the endpoint (requires --vnc).
    #[arg(long)]
    vnc_tls: Option<String>,
    /// One-shot password file for cross-run inheritance (internal).
    #[arg(long, hide = true)]
    vnc_pass_file: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct ShellCmd {
    /// Attach the window to an existing headless session (run id/dir).
    #[arg(long)]
    attach: Option<String>,
    #[arg(long)]
    image: Option<PathBuf>,
    #[arg(long, default_value = "desktop")]
    profile: String,
    #[arg(long, default_value = "fit")]
    scale: String,
    #[arg(long, default_value = "auto")]
    accel: String,
    #[arg(long)]
    memory: Option<u32>,
    #[arg(long, default_value_t = 0)]
    timeout: u64,
    #[arg(long)]
    keep: bool,
    #[arg(long)]
    display_mode: Option<String>,
    /// Boot screen mode: splash (default, iOS-style B/W) or verbose (Linux-style scrolling log).
    #[arg(long)]
    boot_mode: Option<String>,
}

#[derive(Parser, Debug)]
struct TapCmd {
    x: u16,
    y: u16,
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct KeyCmd {
    code: String,
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct SwipeCmd {
    x1: u16,
    y1: u16,
    x2: u16,
    y2: u16,
    #[arg(long, default_value_t = 300)]
    ms: u64,
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct TextCmd {
    text: String,
    #[arg(long)]
    run: Option<String>,
}

#[derive(Parser, Debug)]
struct InputCmd {
    #[arg(long)]
    run: Option<String>,
    #[command(subcommand)]
    action: InputActionCmd,
}

#[derive(Subcommand, Debug)]
enum InputActionCmd {
    /// QEMU qcode, e.g. ret, esc, a.
    Key { code: String },
    /// Type a UTF-8 string as keysyms.
    Text { text: String },
    /// Tap at guest pixel coordinates.
    Tap { x: u16, y: u16 },
    /// Drag from one guest pixel to another.
    Swipe {
        x1: u16,
        y1: u16,
        x2: u16,
        y2: u16,
        #[arg(long, default_value_t = 300)]
        ms: u64,
    },
}

fn main() {
    #[cfg(all(windows, not(feature = "gui")))]
    if std::env::args_os().len() == 1 {
        windows_console::hide_console();
    }
    let cli = Cli::parse();
    let json = cli.json;
    let verbose = cli.verbose;
    let code = match dispatch(cli) {
        Ok(()) => 0,
        Err(e) => {
            if verbose && !json {
                eprintln!("debug: {e:?}");
            }
            e.print(json);
            e.code
        }
    };
    if code != 0 {
        std::process::exit(code);
    }
}

fn dispatch(cli: Cli) -> Result<()> {
    match cli.cmd {
        Some(Cmd::Doctor) => berry::doctor::cmd(cli.json),
        Some(Cmd::Run(c)) => commands::run(RunArgs {
            kernel: c.kernel,
            marker: c.marker,
            timeout: Duration::from_secs(c.timeout),
            accel: c.accel,
            memory: c.memory,
            follow: c.follow,
            keep: c.keep,
            json: cli.json,
        }),
        Some(Cmd::Shot(c)) => commands::shot(ShotArgs {
            image: c.image,
            marker: if c.wait_marker.is_empty() {
                None
            } else {
                Some(c.wait_marker)
            },
            delay: Duration::from_millis(c.delay_ms),
            out: c.out,
            profile: c.profile,
            accel: c.accel,
            memory: c.memory,
            timeout: Duration::from_secs(c.timeout),
            keep: c.keep,
            display_mode: c.display_mode,
            boot_mode: c.boot_mode,
            json: cli.json,
        }),
        Some(Cmd::Display(c)) => match c.action {
            DisplayAction::List => commands::display_list(cli.json),
            DisplayAction::Set(s) => {
                commands::display_set(s.run.as_deref(), &s.profile, s.restart, cli.json)
            }
        },
        Some(Cmd::Shortcut(c)) => {
            commands::shortcut(c.run.as_deref(), &c.action, cli.json)
        }
        Some(Cmd::Events(c)) => {
            commands::events(c.run.as_deref(), c.tail, c.follow, c.filter.as_deref(), cli.json)
        }
        Some(Cmd::Stats(c)) => commands::stats(c.run.as_deref(), cli.json),
        Some(Cmd::Record(c)) => {
            commands::record(
                c.run.as_deref(),
                &c.out,
                c.frames,
                c.interval_ms,
                c.apng.as_deref(),
                cli.json,
            )
        }
        Some(Cmd::Play(c)) => commands::play(&c.file, c.run.as_deref(), cli.json),
        Some(Cmd::Host(c)) => commands::host(commands::HostArgs {
            image: c.image,
            profile: c.profile,
            accel: c.accel,
            memory: c.memory,
            display_mode: c.display_mode,
            boot_mode: c.boot_mode,
            keep: c.keep,
            restart_on_crash: c.restart_on_crash,
            vnc_spec: c.vnc,
            vnc_ws: c.vnc_ws,
            vnc_pass: c.vnc_pass,
            vnc_pass_file: c.vnc_pass_file,
            vnc_tls: c.vnc_tls,
        }),
        Some(Cmd::Device(c)) => match c.action {
            DeviceAction::Start(s) => commands::device_start(DeviceStartArgs {
                image: s.image,
                profile: s.profile,
                accel: s.accel,
                memory: s.memory,
                display_mode: s.display_mode,
                boot_mode: s.boot_mode,
                keep: s.keep,
                foreground: s.foreground,
                restart_on_crash: s.restart_on_crash,
                json: cli.json,
                vnc_spec: s.vnc,
                vnc_ws: s.vnc_ws,
                vnc_pass: s.vnc_pass,
                vnc_pass_file: None,
                vnc_tls: s.vnc_tls,
            }),
            DeviceAction::List => commands::device_list(cli.json),
            DeviceAction::Status(s) => commands::device_status(s.run.as_deref(), cli.json),
            DeviceAction::Restart(s) => commands::device_restart(s.run.as_deref(), cli.json),
            DeviceAction::Stop(s) => commands::device_stop(s.run.as_deref(), cli.json),
            DeviceAction::Cleanup(s) => {
                commands::device_cleanup(s.run.as_deref(), s.all, cli.json)
            }
        },
        Some(Cmd::Power(c)) => {
            commands::power(c.run.as_deref(), &c.action, c.force, cli.json)
        }
        Some(Cmd::Logs(c)) => {
            commands::logs(c.run.as_deref(), c.tail, c.follow, cli.json)
        }
        Some(Cmd::Console(c)) => commands::console(c.run.as_deref()),
        Some(Cmd::Screenshot(c)) => commands::screenshot(ScreenshotArgs {
            run: c.run,
            out: c.out,
            settle: Duration::from_millis(c.settle_ms),
            timeout: Duration::from_secs(c.timeout),
            json: cli.json,
        }),
        Some(Cmd::Shell(c)) => commands::shell(ShellArgs {
            attach: c.attach,
            image: c.image,
            profile: c.profile,
            scale: parse_scale(&c.scale)?,
            accel: c.accel,
            memory: c.memory,
            timeout: Duration::from_secs(c.timeout),
            keep: c.keep,
            display_mode: c.display_mode,
            boot_mode: c.boot_mode,
            json: cli.json,
            vnc_spec: c.vnc,
            vnc_ws: c.vnc_ws,
            vnc_pass: c.vnc_pass,
            vnc_tls: c.vnc_tls,
        }),
        Some(Cmd::Input(c)) => commands::input(InputArgs {
            run: c.run,
            action: match c.action {
                InputActionCmd::Key { code } => InputAction::Key(code),
                InputActionCmd::Text { text } => InputAction::Text(text),
                InputActionCmd::Tap { x, y } => InputAction::Tap(x, y),
                InputActionCmd::Swipe { x1, y1, x2, y2, ms } => {
                    InputAction::Swipe { x1, y1, x2, y2, ms }
                }
            },
            json: cli.json,
        }),
        Some(Cmd::Tap(c)) => commands::input(InputArgs {
            run: c.run,
            action: InputAction::Tap(c.x, c.y),
            json: cli.json,
        }),
        Some(Cmd::Key(c)) => commands::input(InputArgs {
            run: c.run,
            action: InputAction::Key(c.code),
            json: cli.json,
        }),
        Some(Cmd::Swipe(c)) => commands::input(InputArgs {
            run: c.run,
            action: InputAction::Swipe {
                x1: c.x1,
                y1: c.y1,
                x2: c.x2,
                y2: c.y2,
                ms: c.ms,
            },
            json: cli.json,
        }),
        Some(Cmd::Text(c)) => commands::input(InputArgs {
            run: c.run,
            action: InputAction::Text(c.text),
            json: cli.json,
        }),
        Some(Cmd::Vnc(c)) => match c.action {
            VncAction::Status(s) => commands::vnc_status(s.run.as_deref(), cli.json),
            VncAction::Passwd(s) => {
                // Plan contract: --set/--generate/--expire are mutually
                // exclusive (commands::vnc_passwd only sees --set vs --expire,
                // so the --generate pairs are rejected here).
                if s.generate && (s.set.is_some() || s.expire.is_some()) {
                    return Err(BerryError::new(
                        2,
                        "--generate is mutually exclusive with --set/--expire",
                    ));
                }
                commands::vnc_passwd(
                    s.run.as_deref(),
                    // None (default, or explicit --generate) generates.
                    s.set.as_deref(),
                    s.expire.as_deref(),
                    cli.json,
                )
            }
            VncAction::Check(s) => {
                commands::vnc_check(
                    s.run.as_deref(),
                    s.password.as_deref(),
                    s.ca.as_deref(),
                    s.insecure,
                    s.ws,
                    cli.json,
                )
            }
            VncAction::Open(s) => commands::vnc_open(s.run.as_deref(), cli.json),
            VncAction::Web(s) => {
                // Wave-10 flag-family validation (rc=2 usage semantics):
                // --host/--qr/--no-qr only mean something with --lan (a
                // loopback QR is a dead end for phones — wave 9 f4), and
                // --qr × --no-qr is a contradiction.
                if !s.lan && s.host.is_some() {
                    return Err(berry::error::BerryError::new(
                        2,
                        "--host requires --lan".to_string(),
                    )
                    .with_hint("QR 内容与 URL 是 LAN 地址，只在 --lan 模式有意义"));
                }
                if !s.lan && (s.qr || s.no_qr) {
                    return Err(berry::error::BerryError::new(
                        2,
                        "--qr/--no-qr require --lan".to_string(),
                    )
                    .with_hint("QR 内容是 LAN 地址，只在 --lan 模式有意义"));
                }
                if s.qr && s.no_qr {
                    return Err(berry::error::BerryError::new(
                        2,
                        "--qr and --no-qr are mutually exclusive".to_string(),
                    )
                    .with_hint("pass exactly one (default: on with --lan)"));
                }
                // Wave 11 TLS×LAN trust flags: --ca/--insecure shape the
                // /ws proxy's upstream rustls client on TLS runs; they
                // only mean something under --lan (loopback TLS runs
                // serve their own https form), and naming both trust
                // sources at once is a contradiction.
                if s.ca.is_some() && s.insecure {
                    return Err(berry::error::BerryError::new(
                        2,
                        "--ca and --insecure are mutually exclusive".to_string(),
                    ));
                }
                if !s.lan && (s.ca.is_some() || s.insecure) {
                    return Err(berry::error::BerryError::new(
                        2,
                        "--ca/--insecure require --lan".to_string(),
                    )
                    .with_hint("上游 wss 信任源只在 --lan 模式有意义（环回 TLS run 走自带 https 形态）"));
                }
                if let Some(h) = s.host.as_deref() {
                    berry::lan::validate_host_override(h)?;
                }
                commands::vnc_web(
                    s.run.as_deref(),
                    s.port,
                    s.max_conns,
                    s.tls.as_deref(),
                    s.lan,
                    s.host.as_deref(),
                    match (s.qr, s.no_qr) {
                        (true, false) => Some(true),
                        (false, true) => Some(false),
                        _ => None,
                    },
                    s.open,
                    s.ca.as_deref(),
                    s.insecure,
                    cli.json,
                )
            }
            VncAction::Certs(s) => commands::vnc_certs(&s.dir, cli.json),
        },
        Some(Cmd::Version) => {
            commands::version(cli.json);
            Ok(())
        }
        Some(Cmd::Home) => {
            // 单源出口：run 目录/数据盘/vars 持久位的根（BERRY_HOME 覆写
            // 与 Windows LOCALAPPDATA 分支只活在这里——justfile 勿再复制）。
            println!("{}", berry::run_dir::home().display());
            Ok(())
        }
        None => {
            // Finder/double-click has no TTY; a terminal invocation without a
            // subcommand should teach instead of surprising.
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                let mut cmd = Cli::command();
                let _ = cmd.print_help();
                println!();
                return Ok(());
            }
            let timeout = std::env::var("BERRY_SHELL_TIMEOUT")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            commands::shell(ShellArgs {
                attach: None,
                image: None,
                // 与 `berry shell` 默认一致（2026-11-13 owner 裁定：双击/
                // 无参入口 = 横屏 1080P + 硬件加速，统一启动形态）。
                profile: "desktop".to_string(),
                scale: ScaleMode::Fit,
                accel: "auto".to_string(),
                memory: None,
                timeout: Duration::from_secs(timeout),
                keep: std::env::var_os("BERRY_SHELL_KEEP").is_some(),
                display_mode: None,
                boot_mode: None,
                json: false,
                vnc_spec: None,
                vnc_ws: None,
                vnc_pass: None,
                vnc_tls: None,
            })
        }
    }
}

fn parse_scale(s: &str) -> Result<ScaleMode> {
    match s {
        "fit" => Ok(ScaleMode::Fit),
        "1" => Ok(ScaleMode::Integer(1)),
        "2" => Ok(ScaleMode::Integer(2)),
        other => Err(BerryError::new(2, format!("unknown scale '{other}'"))
            .with_hint("fit | 1 | 2")),
    }
}
