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
}

pub fn collect() -> Report {
    let host = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    let qemu = Qemu::discover().ok().map(|q| (q.path, q.version));
    let accel = match qemu::choose_accel("auto", false) {
        Ok(a) => a.as_arg().to_string(),
        Err(_) => "unknown".to_string(),
    };
    let firmware = qemu::firmware().ok();
    let image = qemu::resolve_image(None).ok();
    let kernel = qemu::resolve_kernel(None).ok();
    Report {
        host,
        version: env!("CARGO_PKG_VERSION").to_string(),
        qemu,
        accel,
        firmware,
        image,
        kernel,
        home: crate::run_dir::home(),
    }
}

impl Report {
    pub fn ok(&self) -> bool {
        self.qemu.is_some() && self.firmware.is_some()
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
