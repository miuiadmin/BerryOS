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
    ext_vnc: Option<&str>,
    tls_creds: Option<&Path>,
) -> Result<Child> {
    let mut cmd = qemu.command();
    machine_args(&mut cmd, accel, memory_mib, serial, ram_path.map(|_| "mem"));
    if let Some(ram) = ram_path {
        cmd.arg("-object").arg(format!(
            "memory-backend-file,id=mem,size={memory_mib}M,mem-path={},share=on",
            ram.display()
        ));
    }
    // TLS/VeNCrypt (wave 2): x509 credentials object consumed by the external
    // endpoint's `tls-creds=vnc-tls` option (probe p3 syntax; the dir holds
    // ca-cert/server-cert/server-key pems). verify-peer=off: developer
    // servers do not authenticate clients — authentication stays VNC auth.
    if let Some(dir) = tls_creds {
        cmd.arg("-object").arg(format!(
            "tls-creds-x509,id=vnc-tls,dir={},endpoint=server,verify-peer=off",
            dir.display()
        ));
    }
    // External VNC endpoint first when present: QMP set_password only acts on
    // the first declared server (id=default, probe f3), so the public TCP
    // endpoint must precede the internal unix socket. The password itself is
    // never part of argv — it is injected at runtime via QMP.
    if let Some(ext) = ext_vnc {
        cmd.arg("-vnc").arg(ext);
    }
    cmd.arg("-vnc").arg(vnc.qemu_vnc()?);
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
    // virtio-mmio 默认 legacy；guest 驱动族只认 VERSION_1（inputd GUI-4
    // 判例；guifbd vgpu 扫描遇占用槽 ver≠2 即弃 gpu 落 ramfb——05 号 §5.4
    // 实测）。gpu 恒挂（见下）后此项不再随 tablet/数据盘条件化。
    cmd.arg("-global").arg("virtio-mmio.force-legacy=false");
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
    // virtio-gpu 恒挂：guifbd 双后端的 vgpu 相设备面（svcmgr 映第二窗成功
    // 发 'w' → guifbd 扫窗起 vgpu；无 gpu 相发 'n' 落 ramfb，缺省安全——
    // 09 号 §1-1）。只用全名 virtio-gpu-device（mmio 传输形）；`virtio-gpu`
    // 是 PCI 别名陷阱（05 号 §5.1）。force-legacy 不是该设备的属性（QEMU
    // 11.0.3 实测 `Property 'virtio-gpu-device.force-legacy' not found`），
    // 传输层 legacy 由上面的 -global 关掉（05 号 §1.2 定谳形态）。
    // 声明序 = console 绑定序：QEMU 的 VNC 面向先声明的图形设备，gpu 须
    // 先于 ramfb（09 号 §4 实证定谳：ramfb 在前则 VNC 面恒黑帧）；ramfb
    // 保留为后备（guifbd 分派零改）。
    cmd.arg("-device").arg("virtio-gpu-device");
    cmd.arg("-device").arg("ramfb");
    // M2 第 12 步：fw_cfg 具名键注入单源化——键集与条件只在 fwcfg_plan
    // 一处定义，spawn_image（真注入）与 doctor（静态推演）同源消费；全景
    // 登记在 tools/fwcfg-keys.toml（bash tools/fwcfg-check.sh 对拍）。
    for (name, value, _cond) in fwcfg_plan(display_mode, accel) {
        if let Some(v) = value {
            cmd.arg("-fw_cfg").arg(format!("name={name},string={v}"));
        }
    }
    log_stdio(&mut cmd, log)?;
    cmd.spawn().map_err(|e| {
        BerryError::device(format!("spawn qemu: {e}")).with_hint("check BERRY_QEMU / PATH")
    })
}

/// M2 第 12 步：spawn_image 将注入的 fw_cfg 具名键单源推演（doctor 静态
/// 消费不 spawn，守 no-side-effects）。三元组 =（键名, 值, 条件性）：值
/// None = 按当前环境不注入；conditional = true 的键缺席是条件性缺席（依
/// profile 档 / accel 类型 / 系统时间），不是该注未注。
///
/// 键语义（全景见 tools/fwcfg-keys.toml）：
/// - display：profile 几何（WxH@Hz；缺席 = 固件 GOP 800×600 零漂移）。
/// - accel：平台自识键（FP-FPSR 判例 2026-11-13）——QEMU HVF 后端不保存
///   /恢复 guest FPSR（vmentry 用 host 副本覆盖，实测 11.0.3；KVM 同族
///   风险），硬件加速档告知 guest 跳过对应 ktest 断言；TCG 完整仿真不注。
/// - noktest：GUI 家族恒注（体验大改造六波 owner 裁决 2026-11-15）——
///   生产 boot 跳过 ktest/bench；`berry run` / `just smoke` 冒烟路走
///   spawn_kernel 独立 spawn 点，不注此键（本函数只覆盖 spawn_image 面）。
/// - rtc：宿主当下 epoch 秒（波一刀 B 状态栏可信化；UTC 口径，读取失败
///   = 不注入，内核面降级 -ENOTSUP 静态占位钟）。
pub fn fwcfg_plan(
    display_mode: Option<&str>,
    accel: Accel,
) -> Vec<(&'static str, Option<String>, bool)> {
    let rtc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .ok();
    vec![
        ("opt/berry/display", display_mode.map(|m| m.to_string()), true),
        (
            "opt/berry/accel",
            (accel != Accel::Tcg).then(|| accel.as_arg().to_string()),
            true,
        ),
        ("opt/berry/noktest", Some("1".to_string()), false),
        ("opt/berry/rtc", rtc, true),
    ]
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

/// M2 第 5 步：vars.fd 可选持久（默认关）。blank 重造是「确定性干净启动」
/// 的设计前提（上方注释），跨 run 继承 UEFI 脏状态只作为特性开关：
/// `BERRY_VARS_PERSIST=<path>` 设了——文件在即复用，不在则造 blank 到该
/// 持久位（下次复用）；未设——维持 one-per-run blank（现状零漂移）。
/// 持久件须放 home() 层，勿放 run 目录内（Drop 会连带删）。
pub fn vars_store(run: &Path, mib: u64) -> Result<PathBuf> {
    if let Ok(persist) = std::env::var("BERRY_VARS_PERSIST") {
        let p = PathBuf::from(persist);
        if !p.is_file() {
            create_vars(&p, mib)?;
        }
        return Ok(p);
    }
    let run_vars = run.join("vars.fd");
    create_vars(&run_vars, mib)?;
    Ok(run_vars)
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
