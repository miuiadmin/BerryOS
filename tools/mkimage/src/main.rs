//! CLI 壳——与 Python 版 main() 参数面/输出行对齐（首刀：--selftest 分发
//! + 造盘主路径）。换轨二刀（b 层 #8 刀序第三位）：justfile image 配方
//! 切本件后，Python mkimage.py 降格对拍 oracle。
//!
//! 用法：mkimage <out.img> --efi <EFI> --hello <hello.txt> --kernel <elf>
//!             --initrd <rootfs.cpio> [--size-mib N] [--key-hash <64hex>]
//!             [--source-instance <dir>] [--sign-key <sk> --trust-pk <pk>]
//!             [--manifest-name <name>]
//! 自检：mkimage --selftest（走 cargo test，tools/mkimage——纯面全在 lib
//!       + tests/selftest.rs 挂 test-host，bin 面只做分发）。

use std::process::ExitCode;

fn usage_fail(msg: &str) -> ExitCode {
    eprintln!("mkimage: {msg}");
    eprintln!("用法：mkimage <out.img> --efi <BERRYBOOT.EFI> --hello <hello.txt> --kernel <kernel.elf> --initrd <rootfs.cpio> [--size-mib N] [--key-hash <64hex>] [--source-instance <dir>] [--sign-key <sk> --trust-pk <pk>] [--manifest-name <name>]");
    ExitCode::from(2)
}

/// berry-sign 二进制发现（Python _find_berry_sign 同款语义——同步义务：
/// env BERRY_SIGN → 构建产物候选两 CARGO_TARGET_DIR 惯用位 × debug/release）。
fn find_berry_sign(root: &std::path::Path) -> Option<std::path::PathBuf> {
    if let Ok(env) = std::env::var("BERRY_SIGN") {
        let p = std::path::Path::new(&env);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    for tdir in ["user/cargo-target", "target-cargo"] {
        for profile in ["debug", "release"] {
            let cand = root
                .join(tdir)
                .join("aarch64-apple-darwin")
                .join(profile)
                .join("berry-sign");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// 仓库根发现（exe 路径上溯 + CWD 兜底——mkcpio find_repo_root 同款）。
fn find_repo_root() -> Option<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        dirs.extend(exe.ancestors().map(|p| p.to_path_buf()));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.extend(cwd.ancestors().map(|p| p.to_path_buf()));
    }
    dirs.into_iter().find(|d| d.join(".git").exists())
}

/// 对镜像关键件签名（--sign-key 产线半边）：berry-sign sign-raw <sk>
/// <kernel>（stdout 64B 裸二进制——零临时文件）→ 几何门。berry-sign
/// 不可得 = loud 拒（签名面不静默）。
fn sign_kernel_raw(root: &std::path::Path, sk: &str, kernel: &str) -> Result<Vec<u8>, String> {
    let bs = find_berry_sign(root);
    let Some(bs) = bs else {
        return Err(
            "berry-sign 不可得（设 BERRY_SIGN 或先 CARGO_TARGET_DIR=<隔离目录> cargo build \
             --manifest-path user/Cargo.toml -p berry-crypto --bin berry-sign \
             --target aarch64-apple-darwin）"
                .to_string(),
        );
    };
    let out = std::process::Command::new(&bs)
        .args(["sign-raw", sk, kernel])
        .output()
        .map_err(|e| format!("执行 {} 失败：{e}", bs.display()))?;
    if !out.status.success() {
        let msg = if out.stderr.is_empty() { &out.stdout } else { &out.stderr };
        return Err(format!(
            "sign-raw 拒：{}",
            String::from_utf8_lossy(msg).trim()
        ));
    }
    mkimage::sign_face_puts(&out.stdout, &[0u8; mkimage::TRUSTPK_LEN])
        .map(|_| out.stdout)
        .map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--selftest") {
        // 钉板五步纪律：selftest 面挂 just test-host（cargo test），bin 入口
        // 与 Python CLI 同形（selftest_guard 契约面：声明 --selftest 即须有
        // 接线行，本件接线 = cargo test 行）。
        eprintln!("mkimage: --selftest 走 cargo test（tools/mkimage，tests/selftest.rs）");
        return ExitCode::SUCCESS;
    }
    let mut out: Option<&str> = None;
    let mut efi: Option<&str> = None;
    let mut hello: Option<&str> = None;
    let mut kernel: Option<&str> = None;
    let mut initrd: Option<&str> = None;
    let mut size_mib = mkimage::DEFAULT_SIZE_MIB;
    let mut key_hash: Option<String> = None;
    let mut source_instance: Option<&str> = None;
    let mut sign_key: Option<&str> = None;
    let mut trust_pk: Option<&str> = None;
    let mut manifest_name: Option<&str> = None;
    let mut i = 0;
    fn need<'a>(flag: &str, args: &'a [String], i: &mut usize) -> Result<&'a str, String> {
        *i += 1;
        args.get(*i)
            .map(String::as_str)
            .ok_or_else(|| format!("{flag} 需要一个参数"))
    }
    fn flag_of<'a>(v: &'a Option<&'a str>, name: &str) -> Result<&'a str, String> {
        v.ok_or_else(|| {
            format!("造盘需 out + --efi/--hello/--kernel/--initrd 四件套（缺 {name} 即 loud）")
        })
    }
    while i < args.len() {
        match args[i].as_str() {
            "--efi" => match need("--efi", &args, &mut i) {
                Ok(v) => efi = Some(v),
                Err(e) => return usage_fail(&e),
            },
            "--hello" => match need("--hello", &args, &mut i) {
                Ok(v) => hello = Some(v),
                Err(e) => return usage_fail(&e),
            },
            "--kernel" => match need("--kernel", &args, &mut i) {
                Ok(v) => kernel = Some(v),
                Err(e) => return usage_fail(&e),
            },
            "--initrd" => match need("--initrd", &args, &mut i) {
                Ok(v) => initrd = Some(v),
                Err(e) => return usage_fail(&e),
            },
            "--size-mib" => match need("--size-mib", &args, &mut i).map(|v| v.parse::<usize>()) {
                Err(e) => return usage_fail(&e),
                Ok(Err(_)) => return usage_fail("--size-mib 须正整数"),
                Ok(Ok(n)) => size_mib = n,
            },
            "--key-hash" => {
                let v = match need("--key-hash", &args, &mut i) {
                    Ok(v) => v,
                    Err(e) => return usage_fail(&e),
                };
                match mkimage::parse_key_hash(v) {
                    Ok(h) => key_hash = Some(h),
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::from(1);
                    }
                }
            }
            "--source-instance" => match need("--source-instance", &args, &mut i) {
                Ok(v) => source_instance = Some(v),
                Err(e) => return usage_fail(&e),
            },
            "--sign-key" => match need("--sign-key", &args, &mut i) {
                Ok(v) => sign_key = Some(v),
                Err(e) => return usage_fail(&e),
            },
            "--trust-pk" => match need("--trust-pk", &args, &mut i) {
                Ok(v) => trust_pk = Some(v),
                Err(e) => return usage_fail(&e),
            },
            "--manifest-name" => match need("--manifest-name", &args, &mut i) {
                Ok(v) => manifest_name = Some(v),
                Err(e) => return usage_fail(&e),
            },
            f if f.starts_with("--") => return usage_fail(&format!("未知旗标 {f}")),
            f if out.is_none() => out = Some(f),
            f => return usage_fail(&format!("多余位置参数 {f}")),
        }
        i += 1;
    }
    // 四件套必选面（缺一即 loud——Python ap.error 同形）。
    let out = match flag_of(&out, "out") {
        Ok(v) => v,
        Err(e) => return usage_fail(&e),
    };
    let efi = match flag_of(&efi, "--efi") {
        Ok(v) => v,
        Err(e) => return usage_fail(&e),
    };
    let hello = match flag_of(&hello, "--hello") {
        Ok(v) => v,
        Err(e) => return usage_fail(&e),
    };
    let kernel = match flag_of(&kernel, "--kernel") {
        Ok(v) => v,
        Err(e) => return usage_fail(&e),
    };
    let initrd = match flag_of(&initrd, "--initrd") {
        Ok(v) => v,
        Err(e) => return usage_fail(&e),
    };

    // 成对执法（半装签名镜像 = BerryBoot 验签面静默半装态，注入口炸好过
    // 设备侧兜底行）。
    if sign_key.is_some() != trust_pk.is_some() {
        return usage_fail("--sign-key 与 --trust-pk 须成对（半装签名镜像拒产——BerryBoot sig: half 降级的产线对应物）");
    }

    let Some(root) = find_repo_root() else {
        eprintln!("mkimage: FAIL: 找不到仓库根（exe/CWD 上溯无 .git——platform_revision 取值需仓库树）");
        return ExitCode::from(1);
    };
    // V9（PR ≡ level 刀）：platform_revision 无 CLI 覆盖位——单源读
    // tools/abi.toml [platform].level。u32 域守卫在 build_manifest。
    let pr = match mkimage::platform_level(&root) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };

    let mut puts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut nbytes = 0usize;
    for (local, fat_path) in [
        (efi, b"/EFI/BOOT/BOOTAA64.EFI".as_slice()),
        (hello, b"/HELLO.TXT".as_slice()),
        (kernel, b"/KERNEL.ELF".as_slice()),
        (initrd, b"/INITRD.BIN".as_slice()),
    ] {
        match std::fs::read(local) {
            Ok(data) => {
                nbytes += data.len();
                puts.push((fat_path.to_vec(), data));
            }
            Err(e) => {
                eprintln!("mkimage: FAIL: 读 {local}：{e}");
                return ExitCode::from(1);
            }
        }
    }
    // P2 挂接：源实例两件（缺省缺席——四件套字节零漂移）。
    if let Some(si) = source_instance {
        match mkimage::load_source_instance(si) {
            Ok(extra) => {
                for (p, d) in extra {
                    nbytes += d.len();
                    puts.push((p, d));
                }
            }
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        }
    }
    // P3 挂接：boot 期验签两件（缺省双缺席零漂移）。
    if let Some(sk) = sign_key {
        let pk_path = trust_pk.unwrap();
        let sig = match sign_kernel_raw(&root, sk, kernel) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("mkimage: FAIL: {e}");
                return ExitCode::from(1);
            }
        };
        let pk = match mkimage::read_trust_pk(pk_path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        };
        match mkimage::sign_face_puts(&sig, &pk) {
            Ok(extra) => {
                for (p, d) in extra {
                    nbytes += d.len();
                    puts.push((p, d));
                }
            }
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        }
    }

    let (img, geo) = match mkimage::build_image(&puts, size_mib) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = std::fs::write(out, &img) {
        eprintln!("mkimage: FAIL: 写 {out}：{e}");
        return ExitCode::from(1);
    }
    let manifest = match mkimage::build_manifest(&puts, key_hash.as_deref(), pr) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let mf_name = match mkimage::manifest_file_name(manifest_name) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let manifest_path = std::path::Path::new(out).with_file_name(mf_name);
    if let Err(e) = std::fs::write(&manifest_path, &manifest) {
        eprintln!("mkimage: FAIL: 写 {path}：{e}", path = manifest_path.display());
        return ExitCode::from(1);
    }
    let vol_sectors = mkimage::volume_sectors(size_mib);
    let meta_lba = mkimage::PART_LBA + vol_sectors;
    let si_note = source_instance.map(|s| format!(" source_instance={s}")).unwrap_or_default();
    println!(
        "mkimage: ok {out} {}B (mbr+fat16+keymeta, {size_mib}+16MiB) part=lba{}+{}s \
         meta=lba{}+{}s clusters={} used={} fat={}x{} spc={} files={} bytes={}{}",
        img.len(),
        mkimage::PART_LBA,
        vol_sectors,
        meta_lba,
        mkimage::META_SECTORS,
        geo.clusters,
        geo.next_clus - 2,
        geo.fat_sz,
        mkfat::NUM_FATS,
        geo.spc,
        puts.len(),
        nbytes,
        si_note,
    );
    println!(
        "mkimage: manifest {path} {}B entries={} key_hash={} platform_revision={} signed={}",
        manifest.len(),
        puts.len(),
        key_hash.as_deref().unwrap_or("none"),
        pr,
        if sign_key.is_some() { "on" } else { "off" },
        path = manifest_path.display(),
    );
    // 件 5：产链尾整盘哈希行——供 CI/审计对拍（黄金格式 img: sha256=<64hex>）。
    println!("img: sha256={}", mkimage::sha256_hex(&img));
    ExitCode::SUCCESS
}
