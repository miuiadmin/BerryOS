//! `berry doctor`: environment truth, human or JSON, no side effects.

use std::path::PathBuf;

use serde_json::json;

use crate::error::Result;
use crate::profile::PROFILES;
use crate::qemu::{self, Qemu};

pub struct Report {
    pub host: String,
    pub version: String,
    pub qemu: Option<(PathBuf, String)>,
    pub accel: String,
    pub firmware: Option<PathBuf>,
    pub image: Option<PathBuf>,
    pub kernel: Option<PathBuf>,
    pub home: PathBuf,
    /// M2 第 12 步：spawn_image 将注入的 fw_cfg 键静态推演（name /
    /// injected / conditional——conditional=true 的缺席是条件性缺席，不是
    /// 该注未注；推演面 default 档，profile 档下 display 会翻真）。
    pub fwcfg: Vec<(String, bool, bool)>,
    /// 挂账「持久化入口收敛」setter 侧：缺省数据盘真相——home()/data/
    /// 下盘清单与各 profile 将用的盘（data_disk_plan 单源：BERRY_DATA_DISK
    /// 显式路径/off > data/<profile>.img 存在即挂/缺席即无盘）。
    pub data_disk: DataDiskReport,
}

/// `berry doctor` 的数据盘推演块：dir 盘清单（.img 按名排序）+ 逐
/// profile 的缺省盘（None = 该 profile 无盘可挂，首造归 just desktop）。
pub struct DataDiskReport {
    /// BERRY_DATA_DISK 原值（None = 未设；Some("off") = 显式关）。
    pub env: Option<String>,
    /// 盘目录 home()/data（可能不存在——清单为空是常态）。
    pub dir: PathBuf,
    /// 目录下 .img 文件名清单（排序）。
    pub disks: Vec<String>,
    /// 逐 profile 将用的盘（data_disk_plan 同源推演）。
    pub profiles: Vec<(&'static str, Option<PathBuf>)>,
}

pub fn collect() -> Report {
    let host = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    let qemu = Qemu::discover().ok().map(|q| (q.path, q.version));
    let accel = match qemu::choose_accel("auto", false) {
        Ok(a) => a.as_arg().to_string(),
        Err(_) => "unknown".to_string(),
    };
    let fwcfg = qemu::choose_accel("auto", false)
        .map(|a| {
            qemu::fwcfg_plan(None, a)
                .into_iter()
                .map(|(name, value, cond)| (name.to_string(), value.is_some(), cond))
                .collect()
        })
        .unwrap_or_default();
    let firmware = qemu::firmware().ok();
    let image = qemu::resolve_image(None).ok();
    let kernel = qemu::resolve_kernel(None).ok();
    let data_disk = {
        let dir = crate::run_dir::home().join("data");
        let mut disks: Vec<String> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        e.path().is_file()
                            && e.path().extension().is_some_and(|x| x == "img")
                    })
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        disks.sort();
        DataDiskReport {
            env: std::env::var("BERRY_DATA_DISK").ok(),
            dir,
            disks,
            profiles: PROFILES
                .iter()
                .map(|p| (p.id, qemu::data_disk_plan(p.id)))
                .collect(),
        }
    };
    Report {
        host,
        version: env!("CARGO_PKG_VERSION").to_string(),
        qemu,
        accel,
        firmware,
        image,
        kernel,
        home: crate::run_dir::home(),
        fwcfg,
        data_disk,
    }
}

impl Report {
    pub fn ok(&self) -> bool {
        self.qemu.is_some() && self.firmware.is_some()
    }

    /// data_disk 块的 JSON 形（disks 借用转owned 以进 json!）。
    fn data_disk_json(&self) -> serde_json::Value {
        let dd = &self.data_disk;
        json!({
            "env": dd.env,
            "dir": dd.dir,
            "disks": dd.disks,
            "profiles": dd.profiles
                .iter()
                .map(|(id, disk)| json!({"id": id, "disk": disk}))
                .collect::<Vec<_>>(),
        })
    }

    pub fn print(&self, as_json: bool) {
        if as_json {
            let v = json!({
                "v": 1,
                "t": "doctor",
                "version": self.version,
                "host": self.host,
                "qemu": self.qemu.as_ref().map(|(p, v)| json!({"path": p, "version": v})),
                "accel": self.accel,
                "firmware": self.firmware,
                "image": self.image,
                "kernel": self.kernel,
                "home": self.home,
                "profiles": PROFILES.iter().map(|p| p.id).collect::<Vec<_>>(),
                "fwcfg": self.fwcfg.iter().map(|(name, injected, cond)| {
                    json!({"name": name, "injected": injected, "conditional": cond})
                }).collect::<Vec<_>>(),
                "data_disk": self.data_disk_json(),
                "ok": self.ok(),
            });
            println!("{v}");
        } else {
            println!("berry {} ({})", self.version, self.host);
            match &self.qemu {
                Some((p, v)) => println!("qemu      : {} {v}", p.display()),
                None => println!("qemu      : MISSING (brew install qemu)"),
            }
            println!("accel     : {}", self.accel);
            match &self.firmware {
                Some(p) => println!("firmware  : {}", p.display()),
                None => println!("firmware  : MISSING (AAVMF_CODE)"),
            }
            match &self.image {
                Some(p) => println!("image     : {} ({} MiB)", p.display(), size_mib(p)),
                None => println!("image     : MISSING (just image)"),
            }
            match &self.kernel {
                Some(p) => println!("kernel    : {}", p.display()),
                None => println!("kernel    : MISSING (just build)"),
            }
            println!("home      : {}", self.home.display());
            println!(
                "profiles  : {}",
                PROFILES.iter().map(|p| p.id).collect::<Vec<_>>().join(", ")
            );
            // 缺省数据盘（挂账收敛）：首行 = 盘目录 + .img 清单 + env 状态；
            // 逐 profile 一行 = 将用的盘（(none) = 无盘可挂）。
            let dd = &self.data_disk;
            let disks = if dd.disks.is_empty() {
                "-".to_string()
            } else {
                dd.disks.join(", ")
            };
            println!(
                "data disk : {} [{}] (BERRY_DATA_DISK {})",
                dd.dir.display(),
                disks,
                dd.env.as_deref().unwrap_or("unset")
            );
            for (id, disk) in &dd.profiles {
                match disk {
                    Some(p) => println!("  {id:<8} -> {}", p.display()),
                    None => println!("  {id:<8} -> (none)"),
                }
            }
            let fw = self
                .fwcfg
                .iter()
                .map(|(name, injected, cond)| {
                    format!("{name}{}", if *injected { "" } else if *cond { "(-)" } else { "(MISSING)" })
                })
                .collect::<Vec<_>>()
                .join(" ");
            println!("fwcfg     : {fw}  [(-)=条件性缺席]", );
        }
    }
}

fn size_mib(p: &std::path::Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len() / (1 << 20)).unwrap_or(0)
}

pub fn cmd(as_json: bool) -> Result<()> {
    let r = collect();
    r.print(as_json);
    if !r.ok() {
        return Err(crate::error::BerryError::env("environment incomplete")
            .with_hint("fix the MISSING lines above, then rerun `berry doctor`"));
    }
    Ok(())
}
