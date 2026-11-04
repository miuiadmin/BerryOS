//! `berry` — one binary, two faces: no args on a desktop opens the device
//! shell; subcommands are the headless developer/CI surface.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use clap::{CommandFactory, Parser, Subcommand};

use berry::commands::{
    self, InputAction, InputArgs, RunArgs, ScaleMode, ScreenshotArgs, ShellArgs, ShotArgs,
};
use berry::error::{BerryError, Result};

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
    /// Open the phone-shaped device window (the double-click entry).
    Shell(ShellCmd),
    /// Inject input into a running device via QMP.
    Input(InputCmd),
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
struct ShellCmd {
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
        Some(Cmd::Screenshot(c)) => commands::screenshot(ScreenshotArgs {
            run: c.run,
            out: c.out,
            settle: Duration::from_millis(c.settle_ms),
            timeout: Duration::from_secs(c.timeout),
            json: cli.json,
        }),
        Some(Cmd::Shell(c)) => commands::shell(ShellArgs {
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
                InputActionCmd::Tap { x, y } => InputAction::Tap(x, y),
                InputActionCmd::Swipe { x1, y1, x2, y2, ms } => {
                    InputAction::Swipe { x1, y1, x2, y2, ms }
                }
            },
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
