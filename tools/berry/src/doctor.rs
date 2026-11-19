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
    /// 逐 profile 的 BSHRC.CFG 在席布尔（C5 薄面：crashlog 读面缺席探测
    /// ——静态只读零副作用，fwcfg/data_disk 推演先例延伸）。
    pub bshrc_present: Vec<(&'static str, bool)>,
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
        let profiles: Vec<(&'static str, Option<PathBuf>)> = PROFILES
            .iter()
            .map(|p| (p.id, qemu::data_disk_plan(p.id)))
            .collect();
        DataDiskReport {
            env: std::env::var("BERRY_DATA_DISK").ok(),
            dir,
            disks,
            // BSHRC 在席推演与盘推演同序（Some 盘探测/None 无盘恒 false）。
            bshrc_present: profiles
                .iter()
                .map(|(id, disk)| {
                    (
                        *id,
                        disk.as_deref().is_some_and(crate::bshrc::bshrc_present),
                    )
                })
                .collect(),
            profiles,
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
            "profiles": dd
                .profiles
                .iter()
                .zip(dd.bshrc_present.iter())
                .map(|((id, disk), (_, bshrc))| {
                    json!({"id": id, "disk": disk, "bshrc_present": bshrc})
                })
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
                    Some(p) => {
                        let bshrc = dd
                            .bshrc_present
                            .iter()
                            .find(|(pid, _)| pid == id)
                            .map(|(_, b)| *b)
                            .unwrap_or(false);
                        println!(
                            "  {id:<8} -> {}{}",
                            p.display(),
                            if bshrc { "  [bshrc]" } else { "" }
                        );
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_dir::ENV_LOCK;
    use std::path::Path;

    // doctor 的推演面全在 collect() 依赖真机环境（QEMU/固件在席与否），
    // 可钉板的是纯输出层：data_disk JSON 形状与 ok() 判据；真机依赖面
    // （qemu/accel/fwcfg 推演）不在单测范围。

    #[test]
    fn ok_requires_qemu_and_firmware() {
        let mk = |qemu: bool, fw: bool| Report {
            host: "aarch64-macos".into(),
            version: "0.0.0-test".into(),
            qemu: qemu.then(|| (PathBuf::from("/usr/local/bin/qemu-system-aarch64"), "11.0.0".into())),
            accel: "hvf".into(),
            firmware: fw.then(|| PathBuf::from("/opt/AAVMF_CODE.fd")),
            image: None,
            kernel: None,
            home: PathBuf::from("/tmp/berry-home"),
            fwcfg: vec![],
            data_disk: DataDiskReport {
                env: None,
                dir: PathBuf::from("/tmp/berry-home/data"),
                disks: vec![],
                profiles: vec![],
                bshrc_present: vec![],
            },
        };
        assert!(!mk(false, false).ok(), "全缺 -> not ok");
        assert!(!mk(true, false).ok(), "有 qemu 缺 firmware -> not ok");
        assert!(!mk(false, true).ok(), "缺 qemu 有 firmware -> not ok");
        assert!(mk(true, true).ok(), "双在席 -> ok");
        // image/kernel 在席与否不影响 ok()（运行时才需要）。
        let mut r = mk(true, true);
        r.image = Some(PathBuf::from("/tmp/img"));
        r.kernel = Some(PathBuf::from("/tmp/kernel"));
        assert!(r.ok());
    }

    #[test]
    fn data_disk_json_shape() {
        let r = Report {
            host: "x".into(),
            version: "0".into(),
            qemu: None,
            accel: "unknown".into(),
            firmware: None,
            image: None,
            kernel: None,
            home: PathBuf::from("/h"),
            fwcfg: vec![],
            data_disk: DataDiskReport {
                env: Some("off".into()),
                dir: PathBuf::from("/h/data"),
                disks: vec!["desktop.img".into(), "phone.img".into()],
                profiles: vec![
                    ("default", None),
                    ("desktop", Some(PathBuf::from("/h/data/desktop.img"))),
                ],
                bshrc_present: vec![("default", false), ("desktop", true)],
            },
        };
        let v = r.data_disk_json();
        assert_eq!(v["env"], "off");
        assert_eq!(v["dir"], "/h/data");
        assert_eq!(v["disks"][0], "desktop.img");
        assert_eq!(v["disks"][1], "phone.img");
        assert_eq!(v["profiles"][0]["id"], "default");
        assert!(v["profiles"][0]["disk"].is_null(), "无盘 -> null");
        assert!(!v["profiles"][0]["bshrc_present"].as_bool().unwrap(), "无盘 -> 无 BSHRC");
        assert_eq!(v["profiles"][1]["id"], "desktop");
        assert_eq!(v["profiles"][1]["disk"], "/h/data/desktop.img");
        assert!(v["profiles"][1]["bshrc_present"].as_bool().unwrap(), "在席布尔透传");
        // env 未设时是 null 不是空串（JSON 形状钉板）。
        let mut r2 = r;
        r2.data_disk.env = None;
        assert!(r2.data_disk_json()["env"].is_null());
    }

    // collect() 在受控 BERRY_HOME 下的数据盘目录扫描：只收 .img、排序、
    // env 直读；BERRY_DATA_DISK 未设时逐 profile 的盘 = 在席即挂。
    // QEMU/固件/镜像推演面依赖宿主真环境，不在钉板范围（ok() 判据
    // 单测已覆盖）。
    #[test]
    fn collect_data_disk_scan_under_controlled_home() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("berry-doctor-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(tmp.join("data")).unwrap();
        std::env::set_var("BERRY_HOME", &tmp);
        std::env::remove_var("BERRY_DATA_DISK");
        std::fs::write(tmp.join("data/phone.img"), b"x").unwrap();
        std::fs::write(tmp.join("data/desktop.img"), b"x").unwrap();
        std::fs::write(tmp.join("data/readme.txt"), b"x").unwrap();
        std::fs::write(tmp.join("data/nested"), b"x").unwrap();
        // 目录而非文件：不收。
        std::fs::create_dir_all(tmp.join("data/dir.img")).unwrap();
        let r = collect();
        let dd = &r.data_disk;
        assert_eq!(dd.dir, tmp.join("data"));
        assert_eq!(dd.disks, vec!["desktop.img".to_string(), "phone.img".to_string()]);
        assert!(dd.env.is_none());
        // home() 尊重 BERRY_HOME（doctor 的 home 字段同源）。
        assert_eq!(r.home, tmp);
        // 逐 profile 推演与 data_disk_plan 同源：desktop.img 在席 -> Some，
        // 其余 profile 缺席 -> None；清单含全六 profile。
        assert_eq!(dd.profiles.len(), crate::profile::PROFILES.len());
        for (id, disk) in &dd.profiles {
            match *id {
                "desktop" => assert!(disk.is_some(), "desktop.img 在席即挂"),
                "default" => assert!(disk.is_none(), "default.img 缺席无盘"),
                _ => {}
            }
        }
        // off 显式关：全 profile 无盘。
        std::env::set_var("BERRY_DATA_DISK", "off");
        let r2 = collect();
        assert_eq!(r2.data_disk.env.as_deref(), Some("off"));
        assert!(r2.data_disk.profiles.iter().all(|(_, d)| d.is_none()));
        // C5 薄面：非卷盘件（b"x"）在席也不算 BSHRC 在——crashlog 读面探测。
        assert!(r2.data_disk.bshrc_present.iter().all(|(_, b)| !b));
        std::env::remove_var("BERRY_DATA_DISK");
        std::env::remove_var("BERRY_HOME");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn size_mib_floors_and_missing() {
        assert_eq!(size_mib(Path::new("/nonexistent-berry-test")), 0);
        let tmp = std::env::temp_dir().join(format!("berry-sizemib-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).unwrap();
        let f = tmp.join("img");
        std::fs::write(&f, vec![0u8; (3 << 20) + 1]).unwrap();
        assert_eq!(size_mib(&f), 3, "地板除法：3MiB+1B -> 3");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
