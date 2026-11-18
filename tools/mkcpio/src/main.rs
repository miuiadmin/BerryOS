//! CLI 壳——与 Python 版 main() 参数面/输出行对齐（首刀：仅 --selftest
//! 分发 + 基本打包路径；产链换轨归二刀，justfile 现役 Python 件不动）。
//!
//! 用法：mkcpio <out.cpio> name=path [name=path ...] [--split <MiB>]
//!             [--manifest <rootfs-manifest.toml>]
//! 自检：mkcpio --selftest（走 cargo test，tools/mkcpio——纯面全在 lib +
//! tests/selftest.rs 挂 test-host，bin 面只做分发）。

use std::process::ExitCode;

fn usage_fail(msg: &str) -> ExitCode {
    eprintln!("mkcpio: {msg}");
    eprintln!("用法：mkcpio <out.cpio> name=path [name=path ...] [--split <MiB>] [--manifest <toml>]");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--selftest") {
        // 钉板五步纪律：selftest 面挂 just test-host（cargo test），bin 入口
        // 保持与 Python CLI 同形（a-3 U4 门禁件自身必须有门——selftest_guard
        // 契约面：声明 --selftest 即须有接线行，本件接线 = cargo test 行）。
        eprintln!("mkcpio: --selftest 走 cargo test（tools/mkcpio，tests/selftest.rs）");
        return ExitCode::SUCCESS;
    }
    let mut split_mib: Option<usize> = None;
    let mut manifest_path: Option<String> = None;
    let mut pos: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" => {
                let Some(v) = args.get(i + 1) else {
                    return usage_fail("--manifest 需要 <toml> 参数");
                };
                manifest_path = Some(v.clone());
                i += 2;
            }
            "--split" => {
                let Some(v) = args.get(i + 1) else {
                    return usage_fail("--split 需要 <MiB> 参数");
                };
                match v.parse::<usize>() {
                    Ok(n) if n > 0 => split_mib = Some(n),
                    _ => return usage_fail(&format!("--split 须为正整数（MiB），得 {v:?}")),
                }
                i += 2;
            }
            _ => {
                pos.push(args[i].clone());
                i += 1;
            }
        }
    }
    if pos.len() < 2 && manifest_path.is_none() {
        return usage_fail("需要 <out.cpio> 与至少一条 name=path（或 --manifest 表驱动）");
    }
    let Some(out_path) = pos.first().cloned() else {
        return usage_fail("--manifest 路径仍需 <out.cpio> 位置参数");
    };
    let mut items = pos[1..].to_vec();
    if let Some(mp) = &manifest_path {
        let text = match std::fs::read_to_string(mp) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("mkcpio: FAIL: 读 {mp}：{e}");
                return ExitCode::FAILURE;
            }
        };
        let manifest = match mkcpio::load_manifest(&text) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
        items = match mkcpio::merge_items(&items, &manifest) {
            Ok(it) => it,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
    }
    items.extend(mkcpio::sig_companions(&items, |p| std::path::Path::new(p).exists()));
    // .berryver 生成式烙印名保留——外部注入同名 loud 拒（二刀换轨时本面
    // 随产链迁 Rust；首刀 Python 现役件不动，CLI 此处先钉拒面）。
    if items.iter().any(|it| it.split('=').next() == Some(".berryver")) {
        eprintln!("mkcpio: FAIL: 条目撞名 '.berryver'（生成式烙印名保留——外部不可注入同名条目）");
        return ExitCode::FAILURE;
    }
    // .berryver 烙印（名序点开头排前——cpio 惯例；首刀 CLI 面取数与 Python
    // 同构：git ident 降级链 + abi.toml/Cargo.toml 双单源 + uname 对拍）。
    let berryver = match berryver_bytes() {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("mkcpio: FAIL: {msg}");
            return ExitCode::FAILURE;
        }
    };
    // 排序语义单点：整条 `name=path` 字符串排序（Python main() 同款——
    // 烙印裸名 `.berryver` 同列同一次排序（'.' 排前）；伴生 .sig 条目因
    // `.sig=` 的 `.`(0x2E) < `=`(0x3D) 排在宿主 .bap 条目前——字节级
    // 对拍钉板）。
    let mut sorted_items = items.clone();
    sorted_items.push(".berryver".to_string());
    sorted_items.sort();
    let mut parsed: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(sorted_items.len());
    for item in &sorted_items {
        match item.split_once('=') {
            // 生成式烙印条目：无本地 path，内容上方进程内已取数。
            None if item == ".berryver" => parsed.push((b".berryver".to_vec(), berryver.clone())),
            Some((name, path)) => {
                if name.is_empty() || name.starts_with('/') {
                    return usage_fail(&format!("坏条目 {item:?}"));
                }
                let data = match std::fs::read(path) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("mkcpio: FAIL: 读 {path}：{e}");
                        return ExitCode::FAILURE;
                    }
                };
                parsed.push((name.as_bytes().to_vec(), data));
            }
            None => return usage_fail(&format!("坏条目 {item:?}")),
        }
    }
    let blob = mkcpio::build_archive(&parsed);
    // 陈卷清理先于写卷（--split 缺席 = 单档产链，编号卷全清）。
    let n_vols = split_mib.map(|m| mkcpio::split_volumes(&blob, m * 1024 * 1024).map(|v| v.len()).unwrap_or(0)).unwrap_or(0);
    let stale = mkcpio::clean_stale_volumes(&out_path, n_vols);
    if stale > 0 {
        println!("mkcpio: 清陈卷 {stale} 个（序号 ≥{n_vols}——旧构建残留）");
    }
    if let Err(e) = std::fs::write(&out_path, &blob) {
        eprintln!("mkcpio: FAIL: 写 {out_path}：{e}");
        return ExitCode::FAILURE;
    }
    println!("mkcpio: {out_path} {}B x{} 条目", blob.len(), parsed.len());
    println!("mkcpio: .berryver {}", String::from_utf8_lossy(&berryver));
    if let Some(mib) = split_mib {
        // P3 治理：产卷 Σ 超装载总帽时产链 loud（总帽 128MiB 单源在内核
        // berry_rootface vol_admit——双写纪律：改帽必两处同批）。
        const TOTAL_CAP_MIB: usize = 128;
        if blob.len() > TOTAL_CAP_MIB * 1024 * 1024 {
            let n_over = (blob.len() + mib * 1024 * 1024 - 1) / (mib * 1024 * 1024);
            eprintln!(
                "mkcpio: FAIL: 归档 {}B 超装载总帽 {TOTAL_CAP_MIB}MiB（将产 {n_over} 卷——内核 vol_admit 必拒、ROOTFS vmo=none；消费端帽单源见 berry_rootface）",
                blob.len()
            );
            return ExitCode::FAILURE;
        }
        let vol_bytes = mib * 1024 * 1024;
        let vols = mkcpio::split_volumes(&blob, vol_bytes).unwrap();
        let vpaths = mkcpio::volume_paths(&out_path, vols.len());
        for (path, vol) in vpaths.iter().zip(vols.iter()) {
            if let Err(e) = std::fs::write(path, vol) {
                eprintln!("mkcpio: FAIL: 写 {path}：{e}");
                return ExitCode::FAILURE;
            }
        }
        println!(
            "mkcpio: split {} 卷 ≤{mib}MiB（{}..{}，重装 {}B）",
            vols.len(),
            vpaths.first().map(String::as_str).unwrap_or(""),
            vpaths.last().map(String::as_str).unwrap_or(""),
            vols.iter().map(|v| v.len()).sum::<usize>()
        );
    }
    ExitCode::SUCCESS
}

/// .berryver 烙印内容：单行 `<ident>|level <N>|v<semver>`（UTF-8 无尾随
/// 换行）。ident 降级链与 kernel/build.rs git_ident 同构；level 读
/// tools/abi.toml、semver 读根 Cargo.toml——与 Python berryver_bytes 同款。
fn berryver_bytes() -> Result<Vec<u8>, String> {
    // 取数 CWD 钉仓库根（本 bin 在 <root>/tools/mkcpio/ 下——cargo run 时
    // CWD 归调用方，从 exe 路径上溯；产链调用以仓库根为锚，此处双路兜底）。
    let root = find_repo_root().ok_or_else(|| ".berryver 找不到仓库根（exe 上溯无 .git）".to_string())?;
    let ident = git_ident(&root);
    let level = abi_level(&root)?;
    let semver = workspace_semver(&root)?;
    // P2-5 四面同值：uname 面（user/berry-libc/src/sys.rs UNAME_RELEASE_STR
    // 回退值）≠ semver 单源即 FAIL——Python _uname_release_face 同款。
    let uname = uname_release_face(&root)?;
    if uname != semver {
        return Err(format!(
            ".berryver uname 面漂移：sys.rs 回退 {uname:?} ≠ 根 Cargo.toml {semver:?}（规则 14 四面同值——升版须同笔更新 user/berry-libc/src/sys.rs 回退值）"
        ));
    }
    Ok(format!("{ident}|level {level}|v{semver}").into_bytes())
}

fn find_repo_root() -> Option<std::path::PathBuf> {
    // exe 路径上溯（cargo-target/... 形态）+ CWD 兜底（产链以根为锚）。
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        dirs.extend(exe.ancestors().map(|p| p.to_path_buf()));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.extend(cwd.ancestors().map(|p| p.to_path_buf()));
    }
    dirs.into_iter().find(|d| d.join(".git").exists())
}

fn git_ident(repo: &std::path::Path) -> String {
    let run = |args: &[&str]| -> String {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let describe = run(&["describe", "--tags", "--long"]);
    let base = if describe.is_empty() {
        run(&["rev-parse", "--short", "HEAD"])
    } else {
        describe
    };
    if base.is_empty() {
        return "nogit".to_string();
    }
    if run(&["status", "--porcelain"]).is_empty() {
        base
    } else {
        format!("{base}-dirty")
    }
}

fn abi_level(root: &std::path::Path) -> Result<i64, String> {
    let text = std::fs::read_to_string(root.join("tools").join("abi.toml"))
        .map_err(|e| format!(".berryver 读 tools/abi.toml：{e}"))?;
    // 只取 [platform] 节内 `level = <int>`（行内 # 注释剥——现役文件
    // `level = 1             # 初值…` 形态）；节外 level 不认。
    let mut in_platform = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_platform = l == "[platform]";
            continue;
        }
        if in_platform {
            if let Some(rest) = l.strip_prefix("level") {
                if let Some(v) = rest.trim().strip_prefix('=') {
                    let v = v.split('#').next().unwrap_or("").trim();
                    if let Ok(n) = v.parse::<i64>() {
                        return Ok(n);
                    }
                    return Err(format!("tools/abi.toml [platform].level 非整数：{v:?}（.berryver level 单源）"));
                }
            }
        }
    }
    Err("tools/abi.toml [platform].level 缺席（.berryver level 单源）".to_string())
}

fn workspace_semver(root: &std::path::Path) -> Result<String, String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| format!(".berryver 读根 Cargo.toml：{e}"))?;
    let mut in_pkg = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_pkg = l == "[workspace.package]";
            continue;
        }
        if in_pkg {
            if let Some(v) = l.strip_prefix("version") {
                if let Some(v) = v.trim().strip_prefix('=').map(str::trim) {
                    let v = v.trim_matches('"');
                    if !v.is_empty() {
                        return Ok(v.to_string());
                    }
                }
            }
        }
    }
    Err("根 Cargo.toml [workspace.package].version 非字符串/缺席（.berryver semver 单源）".to_string())
}

fn uname_release_face(root: &std::path::Path) -> Result<String, String> {
    let path = root.join("user").join("berry-libc").join("src").join("sys.rs");
    let src = std::fs::read_to_string(&path)
        .map_err(|e| format!(".berryver uname 面读 {}: {e}", path.display()))?;
    // 与 Python 正则 `const UNAME_RELEASE_STR … None => "<semver>"` 同锚。
    let key = "const UNAME_RELEASE_STR";
    let anchor = src.find(key).ok_or_else(|| {
        format!(
            ".berryver uname 面锚点失配：{} 无 UNAME_RELEASE_STR 回退臂（sys.rs 改形——本守卫须随迁）",
            path.display()
        )
    })?;
    let tail = &src[anchor..];
    let arm = tail.find("None =>").ok_or_else(|| {
        format!(
            ".berryver uname 面锚点失配：{} 无 None => 回退臂（sys.rs 改形——本守卫须随迁）",
            path.display()
        )
    })?;
    let after = tail[arm + "None =>".len()..].trim_start();
    let q = after.find('"').ok_or("uname 面回退臂无字符串字面量")?;
    let end = after[q + 1..].find('"').ok_or("uname 面回退臂字符串未闭合")?;
    Ok(after[q + 1..q + 1 + end].to_string())
}
