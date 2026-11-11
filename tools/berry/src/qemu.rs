//! QEMU discovery, argv construction, firmware/vars/disk plumbing.
//! Arguments are always an argv vector; never a shell string.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{BerryError, Result};
use crate::net::Endpoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accel {
    Tcg,
    Hvf,
    Kvm,
    Whpx,
}

impl Accel {
    pub fn as_arg(self) -> &'static str {
        match self {
            Accel::Tcg => "tcg",
            Accel::Hvf => "hvf",
            Accel::Kvm => "kvm",
            Accel::Whpx => "whpx",
        }
    }

    pub fn cpu(self) -> &'static str {
        match self {
            Accel::Tcg => "cortex-a72",
            _ => "host",
        }
    }
}

pub fn choose_accel(pref: &str, deterministic: bool) -> Result<Accel> {
    if deterministic {
        return Ok(Accel::Tcg);
    }
    let accel = match pref {
        "tcg" => Accel::Tcg,
        "hvf" => Accel::Hvf,
        "kvm" => Accel::Kvm,
        "whpx" => Accel::Whpx,
        "auto" => auto_accel(),
        other => {
            return Err(BerryError::env(format!("unknown accel '{other}'"))
                .with_hint("auto | tcg | hvf | kvm | whpx"))
        }
    };
    Ok(accel)
}

fn auto_accel() -> Accel {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Accel::Hvf
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        if Path::new("/dev/kvm").exists() {
            Accel::Kvm
        } else {
            Accel::Tcg
        }
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "aarch64")
    )))]
    {
        Accel::Tcg
    }
}

pub struct Qemu {
    pub path: PathBuf,
    pub version: String,
}

impl Qemu {
    pub fn discover() -> Result<Self> {
        let path = find_qemu().ok_or_else(|| {
            BerryError::env("qemu-system-aarch64 not found")
                .with_hint("macOS: brew install qemu · Linux: apt install qemu-system-arm")
        })?;
        let version = read_version(&path);
        Ok(Self { path, version })
    }

    pub fn command(&self) -> Command {
        Command::new(&self.path)
    }
}

fn find_qemu() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BERRY_QEMU") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let bin = if cfg!(windows) {
        "qemu-system-aarch64.exe"
    } else {
        "qemu-system-aarch64"
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/usr/local/sbin"] {
        dirs.push(PathBuf::from(d));
    }
    dirs.into_iter().map(|d| d.join(bin)).find(|p| p.is_file())
}

fn read_version(path: &Path) -> String {
    let out = Command::new(path).arg("--version").output();
    let text = match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return "unknown".to_string(),
    };
    text.lines()
        .next()
        .and_then(|l| l.split_whitespace().find(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit())))
        .unwrap_or("unknown")
        .to_string()
}

fn machine_args(
    cmd: &mut Command,
    accel: Accel,
    memory_mib: u32,
    serial: &Path,
    memory_backend: Option<&str>,
) {
    match memory_backend {
        Some(id) => cmd.arg("-machine").arg(format!("virt,memory-backend={id}")),
        None => cmd.arg("-machine").arg("virt"),
    };
    cmd.arg("-accel").arg(accel.as_arg())
        .arg("-cpu").arg(accel.cpu())
        .arg("-m").arg(format!("{memory_mib}M"))
        .arg("-display").arg("none")
        .arg("-monitor").arg("none")
        .arg("-serial").arg(format!("file:{}", serial.display()));
}

fn log_stdio(cmd: &mut Command, log: &Path) -> Result<()> {
    let f = File::create(log)?;
    let f2 = f.try_clone()?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(f))
        .stderr(Stdio::from(f2));
    Ok(())
}

pub fn spawn_kernel(
    qemu: &Qemu,
    kernel: &Path,
    serial: &Path,
    log: &Path,
    accel: Accel,
    memory_mib: u32,
) -> Result<Child> {
    let mut cmd = qemu.command();
    machine_args(&mut cmd, accel, memory_mib, serial, None);
    cmd.arg("-kernel").arg(kernel);
    log_stdio(&mut cmd, log)?;
    cmd.spawn().map_err(|e| {
        BerryError::device(format!("spawn qemu: {e}")).with_hint("check BERRY_QEMU / PATH")
    })
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_image(
    qemu: &Qemu,
    image: &Path,
    firmware: &Path,
    vars: &Path,
    serial: &Path,
    log: &Path,
    vnc: &Endpoint,
    qmp: &Endpoint,
    accel: Accel,
    memory_mib: u32,
    tablet: bool,
    display_mode: Option<&str>,
    ram_path: Option<&Path>,
) -> Result<Child> {
    let mut cmd = qemu.command();
    machine_args(&mut cmd, accel, memory_mib, serial, ram_path.map(|_| "mem"));
    if let Some(ram) = ram_path {
        cmd.arg("-object").arg(format!(
            "memory-backend-file,id=mem,size={memory_mib}M,mem-path={},share=on",
            ram.display()
        ));
    }
    cmd.arg("-vnc").arg(vnc.qemu_vnc());
    cmd.arg("-qmp").arg(qmp.qemu_qmp());
    cmd.arg("-drive")
        .arg(format!(
            "if=pflash,format=raw,readonly=on,file={}",
            firmware.display()
        ))
        .arg("-drive")
        .arg(format!("if=pflash,format=raw,file={}", vars.display()));
    let fmt = if image.extension().is_some_and(|e| e == "qcow2") {
        "qcow2"
    } else {
        "raw"
    };
    cmd.arg("-drive")
        .arg(format!(
            "if=none,file={},format={},id=bb,snapshot=on",
            image.display(),
            fmt
        ))
        .arg("-device").arg("qemu-xhci")
        .arg("-device").arg("usb-storage,drive=bb");
    // GUI 桌面数据盘（可选）：launcher 需要 virtio-blk 数据盘才拿到
    // fs/app/input 三枚 gift（svcmgr BLK_OK 门）；FAT12 盘由 just desktop
    // 造。env 入口避免给 Device/CLI 加一整条参数链。
    let data_disk = std::env::var_os("BERRY_DATA_DISK")
        .map(std::path::PathBuf::from)
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p));
    if tablet || data_disk.is_some() {
        // virtio-mmio 默认 legacy；inputd 只认 VERSION_1（GUI-4 判例）。
        cmd.arg("-global").arg("virtio-mmio.force-legacy=false");
    }
    if let Some(path) = data_disk {
        cmd.arg("-drive")
            .arg(format!(
                "if=none,id=dd,format=raw,file={},cache.writeback=off",
                path.display()
            ))
            .arg("-device").arg("virtio-blk-device,drive=dd");
    }
    if tablet {
        cmd.arg("-device").arg("virtio-tablet-device");
    }
    cmd.arg("-device").arg("ramfb");
    if let Some(mode) = display_mode {
        cmd.arg("-fw_cfg")
            .arg(format!("name=opt/berry/display,string={mode}"));
    }
    log_stdio(&mut cmd, log)?;
    cmd.spawn().map_err(|e| {
        BerryError::device(format!("spawn qemu: {e}")).with_hint("check BERRY_QEMU / PATH")
    })
}

/// App-bundle / runtime-pack resource roots: executable dir, the macOS
/// `.app` Resources dir (`MacOS/../Resources`), and the per-user pack dir.
fn resource_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.to_path_buf());
            if let Some(parent) = d.parent() {
                v.push(parent.join("Resources"));
            }
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        v.push(PathBuf::from(home).join("Library/Application Support/BerryOS"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(local).join("BerryOS"));
    }
    v
}

/// AAVMF code pflash. Read-only: never opens the brew file for write.
pub fn firmware() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("AAVMF_CODE") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    let candidates = [
        "/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
        "/usr/local/share/qemu/edk2-aarch64-code.fd",
        "/usr/share/qemu/edk2-aarch64-code.fd",
        "/usr/share/AAVMF/AAVMF_CODE.fd",
        "out/AAVMF_CODE.fd",
        "tools/berry/firmware/AAVMF_CODE.fd",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.is_file() {
            return Ok(p);
        }
    }
    for d in resource_dirs() {
        let p = d.join("AAVMF_CODE.fd");
        if p.is_file() {
            return Ok(p);
        }
    }
    Err(BerryError::env("AAVMF firmware not found").with_hint(
        "brew install qemu, or set AAVMF_CODE=/path/to/edk2-aarch64-code.fd",
    ))
}

/// Blank 0xFF vars store, one per run: deterministic clean boot state.
pub fn create_vars(path: &Path, mib: u64) -> Result<()> {
    let mut f = File::create(path)?;
    let chunk = vec![0xffu8; 1 << 20];
    for _ in 0..mib {
        f.write_all(&chunk)?;
    }
    f.flush()?;
    Ok(())
}

pub fn resolve_kernel(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return require_file(p, "kernel");
    }
    if let Ok(p) = std::env::var("BERRY_KERNEL") {
        return require_file(PathBuf::from(p), "BERRY_KERNEL");
    }
    let local = PathBuf::from("out/kernel.elf");
    if local.is_file() {
        return Ok(local);
    }
    if Path::new("tools/paths.sh").is_file() {
        let out = Command::new("bash")
            .arg("tools/paths.sh")
            .arg("kernel")
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !s.is_empty() && Path::new(&s).is_file() {
                    return Ok(PathBuf::from(s));
                }
            }
        }
    }
    Err(BerryError::env("kernel ELF not found").with_hint(
        "pass --kernel <path>, set BERRY_KERNEL, or run `just build` (then `just berry ...`)",
    ))
}

pub fn resolve_image(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return require_file(p, "image");
    }
    if let Ok(p) = std::env::var("BERRY_IMAGE") {
        return require_file(PathBuf::from(p), "BERRY_IMAGE");
    }
    let p = PathBuf::from("out/berryos.img");
    if p.is_file() {
        return Ok(p);
    }
    for d in resource_dirs() {
        let p = d.join("berryos.img");
        if p.is_file() {
            return Ok(p);
        }
    }
    Err(BerryError::env("device image not found").with_hint(
        "pass --image <path>, set BERRY_IMAGE, or run `just image`",
    ))
}

fn require_file(p: PathBuf, what: &str) -> Result<PathBuf> {
    if p.is_file() {
        Ok(p)
    } else {
        Err(BerryError::env(format!("{what} not found: {}", p.display())))
    }
}

pub fn wait_for_path(path: &Path, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(BerryError::timeout(format!(
        "timed out waiting for {}",
        path.display()
    )))
}
