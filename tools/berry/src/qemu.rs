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

/// 串口 chardev argv 单源（console 刀件1）：`-chardev socket,id=ser0,
/// path=<sock>,server=on,wait=off,logfile=<serial.log>` + `-serial
/// chardev:ser0`（-chardev 先声明）。语义（侦察实证，QEMU 11.0.3）：
/// - logfile 落盘完整含零客户端期的 boot 早期字节——serial.log 仍由
///   QEMU 写，logs/GUI tail 等读方零改动；
/// - server=on,wait=off：无客户端也照跑；chardev 单活客户端为常态，
///   第二连接只会成为不被服务的僵尸——console 子命令以 run 目录
///   console.lock flock 把「静默僵尸」升格为显式拒绝（commands::console）；
/// - socket backlog=1 存在偶发拒连窗（实证 5 连 4 即成）——客户端
///   0.5s 重试即过（commands::connect_serial）。
#[cfg(unix)]
pub fn serial_chardev_args(serial_sock: &Path, serial_log: &Path) -> Vec<String> {
    vec![
        "-chardev".to_string(),
        format!(
            "socket,id=ser0,path={},server=on,wait=off,logfile={}",
            serial_sock.display(),
            serial_log.display()
        ),
        "-serial".to_string(),
        "chardev:ser0".to_string(),
    ]
}

fn machine_args(
    cmd: &mut Command,
    accel: Accel,
    memory_mib: u32,
    serial: &Path,
    serial_sock: &Path,
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
        .arg("-monitor").arg("none");
    // 串口 chardev 双向化（console 刀件1）：serial.log 由 QEMU 写
    // （logfile= 选项），读方零改动；logfile 缺省 logappend=off——每次
    // spawn QEMU 自己截断 logfile，与 restart_in_place 侧
    // File::create 截断语义天然一致。
    #[cfg(unix)]
    for a in serial_chardev_args(serial_sock, serial) {
        cmd.arg(a);
    }
    // Windows 挂账（诚实）：chardev socket 的 unix path 形态需 TCP 化才
    // 可用，本刀 Unix only——非 Unix 维持 -serial file: 单向落盘
    // （serial.sock 天然缺席，console 子命令在非 Unix 报 not supported）。
    #[cfg(not(unix))]
    {
        let _ = serial_sock;
        cmd.arg("-serial").arg(format!("file:{}", serial.display()));
    }
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
    serial_sock: &Path,
    log: &Path,
    accel: Accel,
    memory_mib: u32,
) -> Result<Child> {
    let mut cmd = qemu.command();
    machine_args(&mut cmd, accel, memory_mib, serial, serial_sock, None);
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
    serial_sock: &Path,
    log: &Path,
    vnc: &Endpoint,
    qmp: &Endpoint,
    accel: Accel,
    memory_mib: u32,
    tablet: bool,
    display_mode: Option<&str>,
    profile_id: &str,
    ram_path: Option<&Path>,
    ext_vnc: Option<&str>,
    tls_creds: Option<&Path>,
) -> Result<Child> {
    let mut cmd = qemu.command();
    machine_args(&mut cmd, accel, memory_mib, serial, serial_sock, ram_path.map(|_| "mem"));
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
    // 注入专用 QMP：run/qmp.sock 被 broker 独占，LAN /ws 输入拦截器需要
    // 自己的通道（RFB PointerEvent → QMP input-send-event）。同目录
    // qmp-inject.sock，多客户端 unix 服务端，本机 0600 run 目录边界内。
    #[cfg(unix)]
    if let crate::net::Endpoint::Unix(p) = qmp {
        let inj = p.with_file_name("qmp-inject.sock");
        cmd.arg("-qmp")
            .arg(format!("unix:{},server=on,wait=off", inj.display()));
    }
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
    // 造。解析单源在 data_disk_plan（spawn 真挂载与 doctor 静态推演同源，
    // fwcfg_plan 先例）：显式 BERRY_DATA_DISK 路径 > "off" 显式关 >
    // home()/data/<profile>.img 存在即挂、缺席即无盘（挂账「持久化入口
    // 收敛」setter 侧：berry 不内嵌 mkfat，首次造盘归 just desktop，
    // berry CLI 只复用——双击态无 shell 环境也能拿到缺省盘）。
    let data_disk = data_disk_plan(profile_id);
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
        // 键盘与 tablet 同门（输入设备家族）：tablet 关则键盘亦关——headless
        // 形零漂移（B4 掉电相 park 语义不变）。设备名只用全名
        // virtio-keyboard-device（mmio 传输形）——`virtio-keyboard` 是 PCI
        // 别名陷阱，与 virtio-gpu-pci 同理（见下方 gpu 注）。force-legacy
        // 不是该设备的属性，传输层 legacy 已被上面的 -global 关掉（05 号
        // §1.2 定谳形态，gpu 同款）。
        cmd.arg("-device").arg("virtio-keyboard-device");
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
    // BERRY_NO_VGPU=1：跳过 virtio-gpu 仅挂 ramfb——ramfb 单后端客机构建
    // （如 knife6 系镜像）在此形态下 VNC 才面向 ramfb；gpu 双后端客机不
    // 要设（会失去加速相）。诊断/演示开关，缺省零漂移。
    if std::env::var_os("BERRY_NO_VGPU").is_none() {
        cmd.arg("-device").arg("virtio-gpu-device");
    }
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
    // demo-face 总门（GUI 家族恒注 off = 演示舰队关停；BERRY_DEMO_FACE=on
    // 逃生门保留舰队——对照/演示舰队本身的场景）。内核侧门
    // DEMO_FACE_PROBE_NAME（kernel/src/main.rs）、svcmgr 侧
    // USER_ENV_DEMO_OFF 由该键一并驱动。
    let demo_face = match std::env::var("BERRY_DEMO_FACE") {
        Ok(v) if v == "on" => None,
        _ => Some("off".to_string()),
    };
    vec![
        ("opt/berry/display", display_mode.map(|m| m.to_string()), true),
        (
            "opt/berry/accel",
            (accel != Accel::Tcg).then(|| accel.as_arg().to_string()),
            true,
        ),
        ("opt/berry/noktest", Some("1".to_string()), false),
        ("opt/berry/demo-face", demo_face, true),
        ("opt/berry/rtc", rtc, true),
    ]
}

/// 缺省数据盘位：home()/data/<profile>.img（profile 命名盘，一次造跨启动
/// 复用——just desktop 的造盘口径同此路径）。
pub fn default_data_disk(profile_id: &str) -> PathBuf {
    crate::run_dir::home().join("data").join(format!("{profile_id}.img"))
}

/// 数据盘解析单源（spawn_image 真挂载与 doctor 静态推演同源，fwcfg_plan
/// 先例）：
/// - `BERRY_DATA_DISK=<path>`：显式路径直用（canonicalize 防 QEMU cwd
///   相对路径失效；在席与否交 QEMU 自己报——显式选择不静默吞）。
/// - `BERRY_DATA_DISK=off`：显式关（缺省盘在席也不挂）。
/// - 未设：default_data_disk(profile_id) 存在即挂、缺席即无盘——berry
///   不内嵌 mkfat，只挂已有件；首次造盘归 just desktop / 手动 mkfat。
pub fn data_disk_plan(profile_id: &str) -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("BERRY_DATA_DISK") {
        if v.to_string_lossy() == "off" {
            return None;
        }
        let p = PathBuf::from(v);
        return Some(std::fs::canonicalize(&p).unwrap_or(p));
    }
    let p = default_data_disk(profile_id);
    p.is_file()
        .then(|| std::fs::canonicalize(&p).unwrap_or(p))
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

// ---- console 刀件1 护栏：serial chardev argv 形态与声明序（-chardev
// ---- 先于 -serial；logfile 指向 serial.log——读方零改动的契约面）。
#[cfg(unix)]
#[cfg(test)]
mod serial_chardev_tests {
    use super::*;

    #[test]
    fn chardev_args_shape_and_order() {
        let args = serial_chardev_args(
            Path::new("/run/42/serial.sock"),
            Path::new("/run/42/serial.log"),
        );
        // -chardev 声明在前（-serial chardev:ser0 引用其 id）。
        assert_eq!(args[0], "-chardev");
        assert!(args[1].starts_with("socket,id=ser0,path=/run/42/serial.sock,"));
        assert!(args[1].contains("server=on"));
        assert!(args[1].contains("wait=off"));
        assert!(args[1].ends_with(",logfile=/run/42/serial.log"));
        // -serial 绑定 chardev id，不再是 file: 单向落盘。
        assert_eq!(args[2], "-serial");
        assert_eq!(args[3], "chardev:ser0");
        assert_eq!(args.len(), 4);
    }
}

// ---- 四轮审计（D4）：fwcfg_plan 条件矩阵护栏——spawn_image（真注入）与
// ---- doctor（静态推演）双消费同源，键集改动两处同漂，此前零护栏。
#[cfg(test)]
mod fwcfg_plan_tests {
    use super::*;

    #[test]
    fn plan_conditional_matrix() {
        // TCG + 无 display 档：display/accel 双条件缺席（conditional=true），
        // noktest 恒注（conditional=false），rtc 依系统时间（只断言结构）。
        let v = fwcfg_plan(None, Accel::Tcg);
        assert_eq!(v[0].0, "opt/berry/display");
        assert_eq!(v[0].1, None);
        assert!(v[0].2);
        assert_eq!(v[1].0, "opt/berry/accel");
        assert_eq!(v[1].1, None);
        assert!(v[1].2);
        assert_eq!(v[2].0, "opt/berry/noktest");
        assert_eq!(v[2].1.as_deref(), Some("1"));
        assert!(!v[2].2);
        assert_eq!(v[3].0, "opt/berry/demo-face");
        assert_eq!(v[3].1.as_deref(), Some("off"));
        assert!(v[3].2);
        assert_eq!(v[4].0, "opt/berry/rtc");
        assert!(v[4].2);
        assert_eq!(v.len(), 5);
    }

    #[test]
    fn plan_accel_and_display_values() {
        // 硬件加速档：accel 注值。
        let v = fwcfg_plan(None, Accel::Hvf);
        assert_eq!(v[1].1.as_deref(), Some("hvf"));
        let v = fwcfg_plan(None, Accel::Kvm);
        assert_eq!(v[1].1.as_deref(), Some("kvm"));
        // display 给档：注 profile 几何串原样。
        let v = fwcfg_plan(Some("480x1000@60"), Accel::Tcg);
        assert_eq!(v[0].1.as_deref(), Some("480x1000@60"));
        // rtc 在时间可读环境下总有值（epoch 秒，数值域粗校验）——
        // 索引 4（demo-face 插入后 rtc 后移）。
        if let Some(rtc) = v[4].1.as_deref() {
            let secs: u64 = rtc.parse().unwrap();
            assert!(secs > 1_700_000_000);
        }
    }
}

// ---- 四轮审计（D4 序·测试续建）：vars_store 三分支与 data_disk_plan
// ---- 分支护栏。BERRY_VARS_PERSIST / BERRY_DATA_DISK / BERRY_HOME 是进程
// ---- 级 env——经 run_dir::ENV_LOCK 与其他 env 触碰测试串行。
#[cfg(test)]
mod vars_store_tests {
    use super::*;
    use crate::run_dir::ENV_LOCK;

    fn tmp_home(tag: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("berry-vars-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    #[test]
    fn persist_set_and_file_present_reuses() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tmp_home("reuse");
        let persist = tmp.join("persist-vars.fd");
        std::fs::write(&persist, b"PERSIST-MARK").unwrap();
        let mtime = std::fs::metadata(&persist).unwrap().modified().unwrap();
        std::env::set_var("BERRY_VARS_PERSIST", &persist);
        let run = tmp.join("run");
        std::fs::create_dir_all(&run).unwrap();
        let got = vars_store(&run, 64).unwrap();
        // 在席即复用：路径即持久件，mtime 与内容双不变（未重造）。
        assert_eq!(got, persist);
        assert_eq!(
            std::fs::metadata(&persist).unwrap().modified().unwrap(),
            mtime
        );
        assert_eq!(std::fs::read(&persist).unwrap(), b"PERSIST-MARK");
        // run 目录不落 blank（持久分支不写 run vars.fd）。
        assert!(!run.join("vars.fd").exists());
        std::env::remove_var("BERRY_VARS_PERSIST");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn persist_set_and_file_absent_creates_blank() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tmp_home("blank");
        let persist = tmp.join("persist-vars.fd");
        std::env::set_var("BERRY_VARS_PERSIST", &persist);
        let run = tmp.join("run");
        std::fs::create_dir_all(&run).unwrap();
        let got = vars_store(&run, 2).unwrap();
        // 不在则造 blank 到持久位：0xff × mib MiB。
        assert_eq!(got, persist);
        let data = std::fs::read(&persist).unwrap();
        assert_eq!(data.len(), 2 * 1024 * 1024);
        assert!(data.iter().all(|&b| b == 0xff));
        assert!(!run.join("vars.fd").exists());
        std::env::remove_var("BERRY_VARS_PERSIST");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unset_uses_per_run_blank() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("BERRY_VARS_PERSIST");
        let tmp = tmp_home("perrun");
        let run = tmp.join("run");
        std::fs::create_dir_all(&run).unwrap();
        let got = vars_store(&run, 1).unwrap();
        // 未设：one-per-run blank（现状零漂移）。
        assert_eq!(got, run.join("vars.fd"));
        assert!(got.is_file());
        let data = std::fs::read(&got).unwrap();
        assert_eq!(data.len(), 1 << 20);
        assert!(data.iter().all(|&b| b == 0xff));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod data_disk_tests {
    use super::*;
    use crate::run_dir::ENV_LOCK;

    fn tmp_home(tag: &str) -> (PathBuf, PathBuf) {
        let tmp = std::env::temp_dir().join(format!("berry-dd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let home = tmp.join("home");
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::env::set_var("BERRY_HOME", &home);
        (tmp, home)
    }

    #[test]
    fn default_mounts_existing_and_absent_is_none() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("BERRY_DATA_DISK");
        let (tmp, home) = tmp_home("default");
        let disk = home.join("data").join("desktop.img");
        std::fs::write(&disk, b"FAAT").unwrap();
        // 在席即挂（canonicalize 后的绝对位）。
        assert_eq!(
            data_disk_plan("desktop"),
            Some(std::fs::canonicalize(&disk).unwrap())
        );
        // 缺席即无盘——berry 只挂已有件，不内嵌 mkfat。
        assert_eq!(data_disk_plan("phone"), None);
        assert_eq!(default_data_disk("phone"), home.join("data").join("phone.img"));
        std::env::remove_var("BERRY_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn env_off_disables_and_env_path_wins() {
        let _g = ENV_LOCK.lock().unwrap();
        let (tmp, home) = tmp_home("env");
        let disk = home.join("data").join("desktop.img");
        std::fs::write(&disk, b"FAAT").unwrap();
        // off 显式关：缺省盘在席也不挂。
        std::env::set_var("BERRY_DATA_DISK", "off");
        assert_eq!(data_disk_plan("desktop"), None);
        // 显式路径直用（canonicalize 口径同 spawn argv）。
        let other = tmp.join("explicit.img");
        std::fs::write(&other, b"FAAT").unwrap();
        std::env::set_var("BERRY_DATA_DISK", &other);
        assert_eq!(
            data_disk_plan("desktop"),
            Some(std::fs::canonicalize(&other).unwrap())
        );
        std::env::remove_var("BERRY_DATA_DISK");
        std::env::remove_var("BERRY_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

// ---- W1-Q25 五刀：qemu.rs 弱面深测——choose_accel 词表/Accel 双映射/
// ---- create_vars blank 几何/resolve 双入口三分支/read_version 首数字
// ---- token 解析/wait_for_path 双出口/fwcfg demo-face 环境门。env 触碰
// ---- 件经 run_dir::ENV_LOCK 与既有 env 测试串行。
#[cfg(test)]
mod weak_face_tests {
    use super::*;
    use crate::run_dir::ENV_LOCK;

    fn tmp_dir(tag: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("berry-qemu5-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    #[test]
    fn choose_accel_matrix() {
        // deterministic 恒 Tcg（冒烟确定性档——pref 不再被消费）。
        assert_eq!(choose_accel("whatever", true).unwrap(), Accel::Tcg);
        for (pref, want) in [
            ("tcg", Accel::Tcg),
            ("hvf", Accel::Hvf),
            ("kvm", Accel::Kvm),
            ("whpx", Accel::Whpx),
        ] {
            assert_eq!(choose_accel(pref, false).unwrap(), want, "{pref}");
        }
        // auto 解析为某合法档即过（具体值平台相关，不钉）。
        choose_accel("auto", false).unwrap();
        // 未知档：env 码 3 + 词表 hint + 原词回显。
        let err = choose_accel("warp", false).unwrap_err();
        assert_eq!(err.code, 3);
        assert_eq!(err.hint.as_deref(), Some("auto | tcg | hvf | kvm | whpx"));
        assert!(err.message.contains("warp"));
    }

    #[test]
    fn accel_arg_and_cpu_mapping() {
        assert_eq!(Accel::Tcg.as_arg(), "tcg");
        assert_eq!(Accel::Hvf.as_arg(), "hvf");
        assert_eq!(Accel::Kvm.as_arg(), "kvm");
        assert_eq!(Accel::Whpx.as_arg(), "whpx");
        // TCG 全仿真用 modeled core；三硬件后端直通 host。
        assert_eq!(Accel::Tcg.cpu(), "cortex-a72");
        assert_eq!(Accel::Hvf.cpu(), "host");
        assert_eq!(Accel::Kvm.cpu(), "host");
        assert_eq!(Accel::Whpx.cpu(), "host");
    }

    #[test]
    fn create_vars_blank_geometry() {
        let tmp = tmp_dir("vars");
        let p = tmp.join("vars.fd");
        create_vars(&p, 2).unwrap();
        let data = std::fs::read(&p).unwrap();
        // blank = 0xff × mib MiB，确定性干净启动的设计前提。
        assert_eq!(data.len(), 2 * 1024 * 1024);
        assert!(data.iter().all(|&b| b == 0xff));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_kernel_explicit_and_env() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tmp_dir("kern");
        let elf = tmp.join("kernel.elf");
        std::fs::write(&elf, b"ELF").unwrap();
        // 显式参数在席即用。
        assert_eq!(resolve_kernel(Some(elf.clone())).unwrap(), elf);
        // 显式参数缺席：env 码 3 + what 回显。
        let missing = tmp.join("nope.elf");
        let err = resolve_kernel(Some(missing.clone())).unwrap_err();
        assert_eq!(err.code, 3);
        assert!(err.message.contains("kernel not found"));
        assert!(err.message.contains("nope.elf"));
        // env 入口同两分支（在席即用 / 缺席报错）。
        std::env::set_var("BERRY_KERNEL", &elf);
        assert_eq!(resolve_kernel(None).unwrap(), elf);
        std::env::set_var("BERRY_KERNEL", &missing);
        assert!(resolve_kernel(None).is_err());
        std::env::remove_var("BERRY_KERNEL");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_image_explicit_and_env() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tmp_dir("img");
        let img = tmp.join("berryos.img");
        std::fs::write(&img, b"IMG").unwrap();
        assert_eq!(resolve_image(Some(img.clone())).unwrap(), img);
        let missing = tmp.join("nope.img");
        let err = resolve_image(Some(missing.clone())).unwrap_err();
        assert_eq!(err.code, 3);
        assert!(err.message.contains("image not found"));
        assert!(err.message.contains("nope.img"));
        std::env::set_var("BERRY_IMAGE", &img);
        assert_eq!(resolve_image(None).unwrap(), img);
        std::env::set_var("BERRY_IMAGE", &missing);
        assert!(resolve_image(None).is_err());
        std::env::remove_var("BERRY_IMAGE");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(unix)]
    #[test]
    fn read_version_first_numeric_token() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tmp_dir("ver");
        let mk = |name: &str, line: &str| {
            let p = tmp.join(name);
            std::fs::write(&p, format!("#!/bin/sh\necho '{line}'\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        // 首行首个数字起始 token = 版本号（跳过 "QEMU"/"emulator"/"version"）。
        let sh = mk("fake-qemu", "QEMU emulator version 11.0.3 (v11.0.3-dirty)");
        assert_eq!(read_version(&sh), "11.0.3");
        // 首行无数字 token → unknown。
        let sh2 = mk("fake-qemu-nodigit", "qemu emulator (no version here)");
        assert_eq!(read_version(&sh2), "unknown");
        // 不可执行（spawn Err 分支）→ unknown，不 panic。
        let plain = tmp.join("not-executable");
        std::fs::write(&plain, b"QEMU emulator version 11.0.3\n").unwrap();
        assert_eq!(read_version(&plain), "unknown");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn wait_for_path_hit_and_timeout() {
        let tmp = tmp_dir("wait");
        let p = tmp.join("here");
        std::fs::write(&p, b"x").unwrap();
        assert!(wait_for_path(&p, Duration::from_millis(200)).is_ok());
        // 缺席超时：timeout 码 4 + 路径回显（有界等待，不挂死）。
        let absent = tmp.join("never");
        let err = wait_for_path(&absent, Duration::from_millis(120)).unwrap_err();
        assert_eq!(err.code, 4);
        assert!(err.message.contains("never"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn fwcfg_demo_face_env_gate() {
        let _g = ENV_LOCK.lock().unwrap();
        let idx = |v: &[(&'static str, Option<String>, bool)]| {
            v.iter().position(|(n, _, _)| *n == "opt/berry/demo-face").unwrap()
        };
        // BERRY_DEMO_FACE=on：逃生门保留舰队——键条件性缺席。
        std::env::set_var("BERRY_DEMO_FACE", "on");
        let v = fwcfg_plan(None, Accel::Tcg);
        let i = idx(&v);
        assert_eq!(v[i].1, None);
        assert!(v[i].2);
        // 未设 / 非 on 值：恒注 off（演示舰队关停缺省态）。
        std::env::remove_var("BERRY_DEMO_FACE");
        let v = fwcfg_plan(None, Accel::Tcg);
        assert_eq!(v[idx(&v)].1.as_deref(), Some("off"));
        std::env::set_var("BERRY_DEMO_FACE", "yes");
        let v = fwcfg_plan(None, Accel::Tcg);
        assert_eq!(v[idx(&v)].1.as_deref(), Some("off"));
        std::env::remove_var("BERRY_DEMO_FACE");
    }
}

// ---- W1-Q26 十九刀：spawn_image argv 拼装面钉板——真 spawn 路唯一入口，
// ---- 单源子面（fwcfg_plan/data_disk_plan/serial_chardev_args）各有护栏，
// ---- 但总装序契约零直测：外 VNC 先于内（QMP set_password 只作用于首个
// ---- 声明的 server）、gpu 先于 ramfb（VNC 面向先声明图形设备）、TLS
// ---- object、memory-backend 形态。经 fake-qemu 脚本捕获 argv
// ---- （read_version 测试同款先例），不跑真 QEMU。
#[cfg(unix)]
#[cfg(test)]
mod spawn_image_argv_tests {
    use super::*;
    use crate::run_dir::ENV_LOCK;
    use std::os::unix::fs::PermissionsExt;

    fn tmp_dir(tag: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("berry-spawnimg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    fn fake_qemu(tmp: &Path) -> Qemu {
        let sh = tmp.join("fake-qemu");
        // argv 捕获到 argv.txt（shell 分隔符安全：NUL 分隔无转义面），
        // exit 0 模拟 spawn 成功。
        std::fs::write(
            &sh,
            "#!/bin/sh\ncd \"$(dirname \"$0\")\"\nprintf '%s\\0' \"$@\" > argv.txt\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
        Qemu {
            path: sh,
            version: "fake".to_string(),
        }
    }

    fn read_argv(tmp: &Path) -> Vec<String> {
        let raw = std::fs::read(tmp.join("argv.txt")).unwrap();
        raw.split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect()
    }

    fn pos(args: &[String], needle: &str) -> usize {
        args.iter()
            .position(|a| a.contains(needle))
            .unwrap_or_else(|| panic!("argv missing {needle}: {args:?}"))
    }

    /// 基础形：全可选件缺席 + tablet 关 + 无数据盘（BERRY_DATA_DISK=off
    /// 钉死，防宿主缺省盘渗入）+ BERRY_NO_VGPU 未设。
    fn spawn_base(tmp: &Path, image_name: &str) -> Vec<String> {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("BERRY_DATA_DISK", "off");
        std::env::remove_var("BERRY_NO_VGPU");
        let r = spawn_image(
            &fake_qemu(tmp),
            &tmp.join(image_name),
            &tmp.join("fw.fd"),
            &tmp.join("vars.fd"),
            &tmp.join("serial.log"),
            &tmp.join("serial.sock"),
            &tmp.join("spawn.log"),
            &Endpoint::Unix(tmp.join("vnc.sock")),
            &Endpoint::Unix(tmp.join("qmp.sock")),
            Accel::Tcg,
            256,
            false,
            None,
            "testprof",
            None,
            None,
            None,
        );
        std::env::remove_var("BERRY_DATA_DISK");
        let mut child = r.unwrap();
        child.wait().unwrap();
        read_argv(tmp)
    }

    #[test]
    fn base_shape_and_conditional_absents() {
        let tmp = tmp_dir("base");
        let args = spawn_base(&tmp, "berryos.img");
        // 机器面：virt（无 memory-backend）、accel/cpu/m 三元组、headless。
        assert!(args.contains(&"-machine".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("virt,memory-backend")));
        let i = pos(&args, "-accel");
        assert_eq!(&args[i + 1], "tcg");
        assert_eq!(&args[i + 2], "-cpu");
        assert_eq!(&args[i + 1], "tcg");
        assert!(args.windows(2).any(|w| w == ["-cpu", "cortex-a72"]));
        assert!(args.windows(2).any(|w| w == ["-m", "256M"]));
        assert!(args.windows(2).any(|w| w == ["-display", "none"]));
        assert!(args.windows(2).any(|w| w == ["-monitor", "none"]));
        // 串口 chardev 双向化 + logfile 契约面（console 刀件1）。
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-chardev" && w[1].starts_with("socket,id=ser0,path=")));
        assert!(args.windows(2).any(|w| w == ["-serial", "chardev:ser0"]));
        // VNC/QMP 内端点形 + 注入 QMP 派生 sock（Unix qmp 端点面）。
        assert!(args.windows(2).any(|w| w[0] == "-vnc" && w[1] == format!("unix:{}", tmp.join("vnc.sock").display())));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-qmp" && w[1] == format!("unix:{},server=on,wait=off", tmp.join("qmp.sock").display())));
        assert!(args.windows(2).any(|w| w[0] == "-qmp" && w[1] == format!("unix:{},server=on,wait=off", tmp.join("qmp-inject.sock").display())));
        // 双 pflash：fw 只读、vars 可写。
        assert!(args.windows(2).any(|w| w[0] == "-drive" && w[1].contains("if=pflash,format=raw,readonly=on,file=")));
        assert!(args.windows(2).any(|w| w[0] == "-drive" && w[1] == format!("if=pflash,format=raw,file={}", tmp.join("vars.fd").display())));
        // raw/qcow2 回落：无扩展名 → raw。
        assert!(args.windows(2).any(|w| w[0] == "-drive" && w[1].contains(&format!("file={},format=raw,id=bb,snapshot=on", tmp.join("berryos.img").display()))));
        // USB 栈。
        assert!(args.windows(2).any(|w| w == ["-device", "qemu-xhci"]));
        assert!(args.windows(2).any(|w| w == ["-device", "usb-storage,drive=bb"]));
        // 数据盘显式关 + 无 ram_backend：无 id=dd 挂载、无 -object。
        assert!(!args.iter().any(|a| a.contains("id=dd")));
        assert!(!args.iter().any(|a| a.starts_with("-object")));
        // tablet 关：键盘/平板双缺。
        assert!(!args.iter().any(|a| a.contains("tablet")));
        assert!(!args.iter().any(|a| a.contains("keyboard")));
        // 图形设备序契约：gpu 先于 ramfb（VNC 面向先声明图形设备）。
        assert!(pos(&args, "virtio-gpu-device") < pos(&args, "ramfb"));
        // fwcfg：noktest 恒注、display 缺席（None）、accel 缺席（TCG）。
        assert!(args.windows(2).any(|w| w[0] == "-fw_cfg" && w[1] == "name=opt/berry/noktest,string=1"));
        assert!(!args.iter().any(|a| a.contains("opt/berry/display")));
        assert!(!args.iter().any(|a| a.contains("opt/berry/accel")));
        assert!(args.iter().any(|a| a.contains("opt/berry/rtc")));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(unix)]
    #[test]
    fn qcow2_and_fwcfg_display_value() {
        let tmp = tmp_dir("qcow2");
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("BERRY_DATA_DISK", "off");
        std::env::remove_var("BERRY_NO_VGPU");
        let r = spawn_image(
            &fake_qemu(&tmp),
            &tmp.join("disk.qcow2"),
            &tmp.join("fw.fd"),
            &tmp.join("vars.fd"),
            &tmp.join("serial.log"),
            &tmp.join("serial.sock"),
            &tmp.join("spawn.log"),
            &Endpoint::Unix(tmp.join("vnc.sock")),
            &Endpoint::Unix(tmp.join("qmp.sock")),
            Accel::Hvf,
            512,
            false,
            Some("480x1000@60"),
            "testprof",
            None,
            None,
            None,
        );
        std::env::remove_var("BERRY_DATA_DISK");
        let mut child = r.unwrap();
        child.wait().unwrap();
        let args = read_argv(&tmp);
        // qcow2 扩展名 → format=qcow2。
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-drive" && w[1].contains(&format!("file={},format=qcow2,id=bb,snapshot=on", tmp.join("disk.qcow2").display()))));
        // 硬件加速档：accel 键注值 + cpu host + accel hvf。
        assert!(args.windows(2).any(|w| w == ["-accel", "hvf"]));
        assert!(args.windows(2).any(|w| w == ["-cpu", "host"]));
        assert!(args.windows(2).any(|w| w[0] == "-fw_cfg" && w[1] == "name=opt/berry/accel,string=hvf"));
        // display 档注值原样。
        assert!(args.windows(2).any(|w| w[0] == "-fw_cfg" && w[1] == "name=opt/berry/display,string=480x1000@60"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(unix)]
    #[test]
    fn full_option_face() {
        let tmp = tmp_dir("full");
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("BERRY_DATA_DISK", "off");
        std::env::remove_var("BERRY_NO_VGPU");
        let ram = tmp.join("shared.ram");
        std::fs::write(&ram, b"RAM").unwrap();
        let tls = tmp.join("tlsdir");
        std::fs::create_dir_all(&tls).unwrap();
        let r = spawn_image(
            &fake_qemu(&tmp),
            &tmp.join("berryos.img"),
            &tmp.join("fw.fd"),
            &tmp.join("vars.fd"),
            &tmp.join("serial.log"),
            &tmp.join("serial.sock"),
            &tmp.join("spawn.log"),
            &Endpoint::Unix(tmp.join("vnc.sock")),
            &Endpoint::Unix(tmp.join("qmp.sock")),
            Accel::Tcg,
            128,
            true,
            None,
            "testprof",
            Some(&ram),
            Some("0.0.0.0:5901"),
            Some(&tls),
        );
        std::env::remove_var("BERRY_DATA_DISK");
        let mut child = r.unwrap();
        child.wait().unwrap();
        let args = read_argv(&tmp);
        // memory-backend：machine 绑定 + object 形态（size 同步、share=on）。
        assert!(args.windows(2).any(|w| w == ["-machine", "virt,memory-backend=mem"]));
        assert!(args.windows(2).any(|w| w[0] == "-object" && w[1] == format!("memory-backend-file,id=mem,size=128M,mem-path={},share=on", ram.display())));
        // TLS creds object（verify-peer=off 开发档）。
        assert!(args.windows(2).any(|w| w[0] == "-object" && w[1] == format!("tls-creds-x509,id=vnc-tls,dir={},endpoint=server,verify-peer=off", tls.display())));
        // 外 VNC 先于内 VNC（QMP set_password 只作用于首个声明 server）。
        assert!(pos(&args, "0.0.0.0:5901") < pos(&args, &format!("unix:{}", tmp.join("vnc.sock").display())));
        // tablet 开：平板 + 键盘同门双挂。
        assert!(args.windows(2).any(|w| w == ["-device", "virtio-tablet-device"]));
        assert!(args.windows(2).any(|w| w == ["-device", "virtio-keyboard-device"]));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
