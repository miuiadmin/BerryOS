//! CLI 壳——与 Python 版 main() 参数面/输出行对齐（首刀：--selftest 分发 +
//! 默认盘/fixture/put 主路径；产链现役 Python 件不动，换轨归二刀）。
//!
//! 用法：mkfat <out.img> [--fixture <path>] [--put <local>:<fat_path> ...]
//!             [--size-mib <N>] [--fat16]
//! 自检：mkfat --selftest（走 cargo test，tools/mkfat——纯面全在 lib +
//! tests/selftest.rs 挂 test-host，bin 面只做分发）。

use std::process::ExitCode;

fn usage_fail(msg: &str) -> ExitCode {
    eprintln!("mkfat: {msg}");
    eprintln!("用法：mkfat <out.img> [--fixture <path>] [--put <local>:<fat_path> ...] [--size-mib <N>] [--fat16]");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--selftest") {
        // 钉板五步纪律：selftest 面挂 just test-host（cargo test），bin 入口
        // 保持与 Python CLI 同形（selftest_guard 契约面：声明 --selftest 即
        // 须有接线行，本件接线 = cargo test 行）。
        eprintln!("mkfat: --selftest 走 cargo test（tools/mkfat，tests/selftest.rs）");
        return ExitCode::SUCCESS;
    }
    let mut put_specs: Vec<String> = Vec::new();
    let mut size_mib: Option<usize> = None;
    let mut fat16 = false;
    let mut pos: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--put" => {
                let Some(v) = args.get(i + 1) else {
                    return usage_fail("--put 需要一个 <local>:<fat_path> 参数");
                };
                put_specs.push(v.clone());
                i += 2;
            }
            "--size-mib" => {
                let Some(v) = args.get(i + 1) else {
                    return usage_fail("--size-mib 需要一个整数参数");
                };
                match v.parse::<usize>() {
                    Ok(n) => size_mib = Some(n),
                    Err(_) => return usage_fail(&format!("--size-mib 非整数（得 {v:?}）")),
                }
                i += 2;
            }
            "--fat16" => {
                fat16 = true;
                i += 1;
            }
            _ => {
                pos.push(args[i].clone());
                i += 1;
            }
        }
    }
    let Some(out_path) = pos.first().cloned() else {
        return usage_fail("需要 <out.img> 位置参数（裸调/fixture 模式）或 --put 面");
    };
    if !put_specs.is_empty() {
        let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        for spec in &put_specs {
            let Some(idx) = spec.find(':') else {
                return usage_fail(&format!("--put 形如 <local>:<fat_path>（得 {spec:?}）"));
            };
            let (local, fat_path) = (&spec[..idx], &spec[idx + 1..]);
            if local.is_empty() || fat_path.is_empty() {
                return usage_fail(&format!("--put 形如 <local>:<fat_path>（得 {spec:?}）"));
            }
            match std::fs::read(local) {
                Ok(d) => pairs.push((fat_path.as_bytes().to_vec(), d)),
                Err(e) => return usage_fail(&format!("读不了 {local}：{e}")),
            }
        }
        let result = if let Some(mib) = size_mib {
            mkfat::build_volume_puts(&pairs, mib * (1024 * 1024 / mkfat::SECTOR), fat16, 0)
        } else {
            mkfat::build_volume_puts(&pairs, mkfat::DISK_SECTORS, fat16, 0)
        };
        let (vol, geo) = match result {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
        if let Err(e) = std::fs::write(&out_path, &vol) {
            return usage_fail(&format!("写不了 {out_path}：{e}"));
        }
        let names = pairs.iter().map(|(p, _)| String::from_utf8_lossy(p).into_owned()).collect::<Vec<_>>().join(" ");
        let size_note = size_mib.map(|m| format!(", {m}MiB")).unwrap_or_default();
        let wide_note = if fat16 { ", fat16" } else { "" };
        println!(
            "mkfat: {out_path} {}B (efi boot 盘{size_note}{wide_note}) clusters_used={}/{} fat={}x{} puts=[{names}]",
            vol.len(),
            geo.next_clus - 2,
            geo.clusters,
            geo.fat_sz,
            mkfat::NUM_FATS,
        );
        return ExitCode::SUCCESS;
    }
    if size_mib.is_some() {
        return usage_fail("--size-mib 仅与 --put 连用（EFI 引导卷尺寸）");
    }
    if fat16 {
        return usage_fail("--fat16 仅与 --put 连用（FAT16 引导卷）");
    }
    // 默认盘 / fixture 模式（首刀 CLI 面：默认盘；--fixture 前缀写盘归
    // Python 现役件语义，此处对齐裸调主路径）。
    let (disk, geo) = match mkfat::build_disk() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = std::fs::write(&out_path, &disk) {
        return usage_fail(&format!("写不了 {out_path}：{e}"));
    }
    println!(
        "mkfat: {out_path} {}B vol@LBA{} clusters={} fat={}x{}",
        disk.len(),
        mkfat::RESERVED_ZONE,
        geo.clusters,
        geo.fat_sz,
        mkfat::NUM_FATS,
    );
    ExitCode::SUCCESS
}
