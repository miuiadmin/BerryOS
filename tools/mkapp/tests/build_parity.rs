//! W1-Q27 五刀·产链换轨对拍：Rust build 面 ↔ Python `mkapp.py build()`
//! oracle 真跑对拍（mkfat/mkcpio/mkimage 换轨同款纪律）。
//!
//! Rust 侧走 lib 直调 `mkapp::build::build()`（hermetic——不依赖 bin 在席
//! 与外层 target-dir 环境）；Python 侧真跑 CLI。三组真实 staging
//! （无 UI / icon+views 全 UI / fs+net 能力）对拍：产物 sha256 恒等 +
//! 摘要行（剥路径段）全等 + Python `--check`（check_bap 五步）交叉复验
//! Rust 产物。拒面等价抽查：坏 staging 双方同拒同文案。Python 缺席打
//! skip（CI 净仓面——纯面钉板兜底在 src/lib.rs tests）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn run_py(args: &[&str], cwd: &Path) -> (bool, String, String) {
    let out = Command::new("python3")
        .arg(repo_root().join("tools/mkapp.py"))
        .args(args)
        .current_dir(cwd)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("spawn python3");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// PNG CRC-32（IEEE 反射多项式）——逐位法（测试件免表）。
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn png_chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc_in = tag.to_vec();
    crc_in.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
    out
}

/// 最小合法 8-bit RGBA PNG（stored zlib 块——零依赖可读）。
fn mk_png(w: u32, h: u32) -> Vec<u8> {
    let mut raw = Vec::new();
    for _ in 0..h {
        raw.push(0u8); // filter 0
        for x in 0..w {
            raw.extend_from_slice(&[(x * 40) as u8 & 0xFF, 0x80, 0xC0, 0xFF]);
        }
    }
    assert!(raw.len() < 65535);
    let mut z = vec![0x78u8, 0x01, 0x01];
    z.extend_from_slice(&(raw.len() as u16).to_le_bytes());
    z.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
    z.extend_from_slice(&raw);
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let ihdr = [
        w.to_be_bytes().to_vec(),
        h.to_be_bytes().to_vec(),
        vec![8, 6, 0, 0, 0],
    ].concat();
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &z));
    png.extend_from_slice(&png_chunk(b"IEND", b""));
    png
}

fn mk_stage(base: &Path, id: &str, manifest: &str, elf_pad: usize) -> PathBuf {
    let dir = base.join(id);
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("manifest.toml"), manifest).unwrap();
    let mut elf = b"\x7fELF".to_vec();
    elf.extend(std::iter::repeat(0xA5u8).take(elf_pad));
    fs::write(dir.join("bin").join(id.to_lowercase()), elf).unwrap();
    dir
}

fn manifest_plain(id: &str, ver: &str, extra_caps: &str) -> String {
    format!(
        "[app]\nid = \"{id}\"\nversion = \"{ver}\"\nentry = \"bin/{e}\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false{extra_caps}\n\n[runtime]\nstack_pages = 8\n",
        id = id,
        ver = ver,
        e = id.to_lowercase()
    )
}

#[test]
fn build_parity_oracle() {
    let root = repo_root();
    if !root.join("tools/mkapp.py").exists() {
        eprintln!("mkapp: tools/mkapp.py 缺席——CLI 对拍 skip（纯面钉板兜底）");
        return;
    }
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("mkapp: python3 不在席——CLI 对拍 skip（纯面钉板兜底）");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("mkapp-parity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    // 三组：plain / 全 UI（icon+views+label+compat 显式 abi）/ fs+net 能力
    let d1 = mk_stage(&tmp, "PARITYA", &manifest_plain("paritya", "0.1.0", ""), 64);
    let m2 = format!(
        "[app]\nid = \"parityb\"\nversion = \"0.2.0\"\nentry = \"bin/parityb\"\ntype = \"app\"\nintent = \"utility\"\n\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\nabi_revision = 7\n\n[capabilities]\nconsole = true\n\n[runtime]\nstack_pages = 16\n\n[ui]\nicon = \"icons/app.byi\"\nlabel = \"Parity B\"\nviews = \"views/main\"\n"
    );
    let d2 = mk_stage(&tmp, "PARITYB", &m2, 96);
    fs::create_dir_all(d2.join("icons")).unwrap();
    fs::write(d2.join("icons/app.png"), mk_png(3, 2)).unwrap();
    fs::create_dir_all(d2.join("views")).unwrap();
    fs::write(d2.join("views/main"), b"# placeholder view\n").unwrap();
    let d3 = mk_stage(
        &tmp,
        "PARITYC",
        &manifest_plain("parityc", "1.0.0", "\nfs = [\"rootfs\"]\nnet = [\"client\"]"),
        128,
    );

    for (dir, id) in [(d1, "PARITYA"), (d2, "PARITYB"), (d3, "PARITYC")] {
        // 输出名门 = <id>.bap——两侧各一个子目录分置产物
        let dir_r = tmp.join("out-r");
        let dir_p = tmp.join("out-p");
        fs::create_dir_all(&dir_r).unwrap();
        fs::create_dir_all(&dir_p).unwrap();
        let out_r = dir_r.join(format!("{}.bap", id.to_lowercase()));
        let out_p = dir_p.join(format!("{}.bap", id.to_lowercase()));
        // Rust：lib 直调（hermetic）
        let sum = mkapp::build::build(out_r.to_str().unwrap(), dir.to_str().unwrap())
            .unwrap_or_else(|e| panic!("Rust mkapp {id} 打包拒：{} ({})", e.message, e.code));
        // Python：oracle 真跑
        let (pok, pout, perr) = run_py(
            &[out_p.to_str().unwrap(), dir.to_str().unwrap()],
            &root,
        );
        assert!(pok, "Python mkapp {id} 打包失败：{perr}");
        // 产物字节恒等
        let hr = mkapp::cpio::sha256_hex(&fs::read(&out_r).unwrap());
        let hp = mkapp::cpio::sha256_hex(&fs::read(&out_p).unwrap());
        assert_eq!(hr, hp, "{id}: Rust 与 Python 产物 sha256 不等——单源漂移");
        // 摘要行（剥第二 token 路径段）全等——忠告 warn 行（<108 图标）
        // 与摘要行同流，取「含 NB B 尺寸段」的摘要行比对
        let pick_summary = |text: &str| -> String {
            text.lines()
                .rev()
                .find(|l| l.starts_with("mkapp: ") && l.contains("B "))
                .map(|l| {
                    let parts: Vec<&str> = l.splitn(3, ' ').collect();
                    format!("{} {}", parts[0], parts[2])
                })
                .unwrap_or_default()
        };
        assert_eq!(
            pick_summary(&sum.line()),
            pick_summary(pout.trim()),
            "{id}: 摘要行不等——Rust「{}」vs Python「{}」",
            sum.line(),
            pout.trim()
        );
        // Python --check 五步交叉复验 Rust 产物
        let (cok, _, cerr) = run_py(&["--check", out_r.to_str().unwrap()], &root);
        assert!(cok, "Python --check 拒 Rust 产物 {id}：{cerr}");
    }

    // 拒面等价抽查：未知顶层内容——双方同拒同文案
    let bad = tmp.join("BADX");
    fs::create_dir_all(bad.join("bin")).unwrap();
    fs::write(bad.join("manifest.toml"), manifest_plain("badx", "0.1.0", "")).unwrap();
    fs::write(bad.join("stray.txt"), b"x").unwrap();
    let out_b = tmp.join("BADX.bap");
    let rerr = match mkapp::build::build(out_b.to_str().unwrap(), bad.to_str().unwrap()) {
        Err(e) => e,
        Ok(_) => panic!("Rust 应拒未知内容"),
    };
    assert_eq!(rerr.code, "stage.unknown");
    let (pok, _, perr) = run_py(&[out_b.to_str().unwrap(), bad.to_str().unwrap()], &root);
    assert!(!pok, "Python 应拒未知内容");
    assert!(perr.contains("未知内容"), "Python 拒面文案：{perr}");

    let _ = fs::remove_dir_all(&tmp);
}
