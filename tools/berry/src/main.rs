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
    /// Print version.
    Version,
}

#[derive(Parser, Debug)]
struct RunCmd {
    #[arg(long)]
    kernel: Option<PathBuf>,
    #[arg(long, default_value = "BerryOS boot")]
    marker: String,
    #[arg(long, default_value_t = 30)]
    timeout: u64,
    #[arg(long, default_value = "tcg")]
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
    #[arg(long, default_value = "default")]
    profile: String,
    #[arg(long, default_value = "tcg")]
    accel: String,
    #[arg(long)]
    memory: Option<u32>,
    #[arg(long, default_value_t = 120)]
    timeout: u64,
    #[arg(long)]
    keep: bool,
    #[arg(long)]
    display_mode: Option<String>,
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
struct StartCmd {
    #[arg(long)]
    image: Option<PathBuf>,
    #[arg(long, default_value = "phone")]
    profile: String,
    #[arg(long, default_value = "tcg")]
    accel: String,
    #[arg(long)]
    memory: Option<u32>,
    #[arg(long)]
    display_mode: Option<String>,
    #[arg(long)]
    keep: bool,
    #[arg(long)]
    foreground: bool,
    /// Restart QEMU in the same run after an unexpected exit.
    #[arg(long)]
    restart_on_crash: bool,
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
    #[arg(long, default_value = "phone")]
    profile: String,
    #[arg(long, default_value = "tcg")]
    accel: String,
    #[arg(long)]
    memory: Option<u32>,
    #[arg(long)]
    display_mode: Option<String>,
    #[arg(long)]
    keep: bool,
    /// Restart QEMU in the same run after an unexpected exit.
    #[arg(long)]
    restart_on_crash: bool,
}

#[derive(Parser, Debug)]
struct ShellCmd {
    /// Attach the window to an existing headless session (run id/dir).
    #[arg(long)]
    attach: Option<String>,
    #[arg(long)]
    image: Option<PathBuf>,
    #[arg(long, default_value = "phone")]
    profile: String,
    #[arg(long, default_value = "fit")]
    scale: String,
    #[arg(long, default_value = "tcg")]
    accel: String,
    #[arg(long)]
    memory: Option<u32>,
    #[arg(long, default_value_t = 0)]
    timeout: u64,
    #[arg(long)]
    keep: bool,
    #[arg(long)]
    display_mode: Option<String>,
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
            commands::events(c.run.as_deref(), c.tail, c.follow, cli.json)
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
            keep: c.keep,
            restart_on_crash: c.restart_on_crash,
        }),
        Some(Cmd::Device(c)) => match c.action {
            DeviceAction::Start(s) => commands::device_start(DeviceStartArgs {
                image: s.image,
                profile: s.profile,
                accel: s.accel,
                memory: s.memory,
                display_mode: s.display_mode,
                keep: s.keep,
                foreground: s.foreground,
                restart_on_crash: s.restart_on_crash,
                json: cli.json,
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
            json: cli.json,
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
        Some(Cmd::Version) => {
            commands::version(cli.json);
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
                profile: "phone".to_string(),
                scale: ScaleMode::Fit,
                accel: "tcg".to_string(),
                memory: None,
                timeout: Duration::from_secs(timeout),
                keep: std::env::var_os("BERRY_SHELL_KEEP").is_some(),
                display_mode: None,
                json: false,
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
