//! CLI 壳——与 Python 版 main() 参数面/输出行对齐（首刀：--selftest 分发 +
//! 默认盘/fixture/put 主路径）。换轨二刀（b 层 #8 刀序第二位）：justfile
//! 四处产链位（run-rootfs 造盘 / test-host 双 fixture 守卫 / desktop 数据
//! 盘）已切本件；Python mkfat.py 保模块面（mkimage/pwrcls import 单编码
//! 纪律）+ CLI 对拍 oracle + fixture 重产面。
//!
//! 用法：mkfat <out.img> [--fixture <path>] [--put <local>:<fat_path> ...]
//!             [--size-mib <N>] [--fat16]
//! 守卫：mkfat --check-fixture <path> / --check-fixture16 <path>（不写盘，
//!       内存重产逐字节比对——test-host 前置基准漂移即 FAIL）
//! 重产：mkfat --fixture16 <path>（FAT16 共识基准单飞产盘）
//! 自检：mkfat --selftest（走 cargo test，tools/mkfat——纯面全在 lib +
//!       tests/selftest.rs 挂 test-host，bin 面只做分发）。

use std::process::ExitCode;

fn usage_fail(msg: &str) -> ExitCode {
    eprintln!("mkfat: {msg}");
    eprintln!("用法：mkfat <out.img> [--fixture <path>] [--put <local>:<fat_path> ...] [--size-mib <N>] [--fat16]");
    ExitCode::from(2)
}

/// FAT16 共识基准 puts（V-2，Python FIXTURE16_PUTS 同源同串）——内容字节与
/// FAT12 fixture 现役同源，仅卷几何翻宽（16384 扇区 = FIXTURE16_VOL_SECTORS）。
fn fixture16_puts() -> Vec<(Vec<u8>, Vec<u8>)> {
    vec![
        (b"/HELLO.TXT".to_vec(), b"berryos fat read-only hello\n".to_vec()),
        (b"/PATTERN.BIN".to_vec(), mkfat::pattern_bin(5000)),
        (b"/DATA/NOTE.TXT".to_vec(), b"berryos fat subdir note\n".to_vec()),
    ]
}

/// fixture 面 puts → build_volume 所需 dirs 形（FAT12 侧 = 扁平子目录，
/// 不产 `.`/`..`——与 FAT16 走 build_volume_puts 的构造体不同形，两端约定）。
fn fixture_dirs() -> Vec<(&'static [u8], Vec<(Vec<u8>, Vec<u8>)>)> {
    mkfat::FIXTURE_DIRS
        .iter()
        .map(|(name, files)| {
            (
                *name,
                files.iter().map(|(n, d)| (n.to_vec(), d.to_vec())).collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// 卷前缀长度（至末链簇）——boot + FAT×2 + 根 + 已分配簇。
fn prefix_len(geo: &mkfat::Geo) -> usize {
    (geo.data_start + (geo.next_clus as usize - 2) * geo.spc) * mkfat::SECTOR
}

fn fixture_volume(fat16: bool) -> Result<(Vec<u8>, mkfat::Geo), mkfat::MkfatError> {
    if fat16 {
        mkfat::build_volume_puts(&fixture16_puts(), mkfat::FIXTURE16_VOL_SECTORS, true, 0)
    } else {
        let dirs = fixture_dirs();
        let refs: Vec<(&[u8], &[(Vec<u8>, Vec<u8>)])> =
            dirs.iter().map(|(n, fs)| (*n, fs.as_slice())).collect();
        mkfat::build_volume(None, &refs)
    }
}

/// 守卫模式（--check-fixture / --check-fixture16）：不写盘，内存重产 fixture
/// 字节与 <path> 逐字节比对——Python 同形输出行（重产提示指向 mkfat.py：Python
/// 件保留 fixture 重产面，产链/守卫两侧任一改动都会被本门咬住）。
fn check_fixture(fx: &str, fat16: bool) -> ExitCode {
    let (vol, geo) = match fixture_volume(fat16) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let want = &vol[..prefix_len(&geo)];
    let got = match std::fs::read(fx) {
        Ok(g) => g,
        Err(e) => {
            println!("mkfat: check-fixture{} FAIL: 读不了 {fx}：{e}", if fat16 { "16" } else { "" });
            return ExitCode::from(1);
        }
    };
    if got != want {
        let tag = if fat16 { "16" } else { "" };
        let regen = if fat16 {
            format!("mkfat.py --fixture16 {fx}")
        } else {
            format!("mkfat.py x.img --fixture {fx}")
        };
        println!(
            "mkfat: check-fixture{tag} FAIL: {fx} 与现行 mkfat 产物不一致（got {}B / want {}B）——改过 mkfat 就重产：{regen}",
            got.len(),
            want.len()
        );
        return ExitCode::from(1);
    }
    println!("mkfat: check-fixture{} ok: {fx} {}B", if fat16 { "16" } else { "" }, want.len());
    ExitCode::SUCCESS
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
    // FAT16 守卫模式（V-2，结构镜像 --check-fixture）：互斥面单飞。
    if args.iter().any(|a| a == "--check-fixture16") {
        let clash: Vec<&str> = ["--check-fixture", "--fixture", "--fixture16", "--put"]
            .into_iter()
            .filter(|f| args.iter().any(|a| a == f))
            .collect();
        if !clash.is_empty() {
            eprintln!("mkfat: --check-fixture16 与 {} 互斥", clash.join(" "));
            return ExitCode::from(2);
        }
        let Some(fx) = args.iter().position(|a| a == "--check-fixture16").and_then(|i| args.get(i + 1)) else {
            eprintln!("mkfat: --check-fixture16 需要一个路径参数");
            return ExitCode::from(2);
        };
        return check_fixture(fx, true);
    }
    // 守卫模式：不写盘，只比对 fixture（挂 test-host 前置——基准漂移即 FAIL）。
    if args.iter().any(|a| a == "--check-fixture") {
        if args.iter().any(|a| a == "--fixture16") {
            eprintln!("mkfat: --check-fixture 与 --fixture16 互斥");
            return ExitCode::from(2);
        }
        let Some(fx) = args.iter().position(|a| a == "--check-fixture").and_then(|i| args.get(i + 1)) else {
            eprintln!("mkfat: --check-fixture 需要一个路径参数");
            return ExitCode::from(2);
        };
        return check_fixture(fx, false);
    }
    // FAT16 基准产盘（V-2）：单飞——不产整盘只写卷前缀（boot + FAT16×2 +
    // 根 + 全部已分配簇），确定性字节进 git；与 --fixture/--put 互斥。
    // （Python 件保留同面重产责任：基准重产提示与 oracle 对拍仍指 mkfat.py，
    // 双件任一改动被 --check-fixture16 守卫咬合。）
    if args.iter().any(|a| a == "--fixture16") {
        let idx = args.iter().position(|a| a == "--fixture16").unwrap();
        let Some(fx) = args.get(idx + 1) else {
            eprintln!("mkfat: --fixture16 需要一个路径参数");
            return ExitCode::from(2);
        };
        let extra: Vec<&str> = args[..idx]
            .iter()
            .chain(args[idx + 2..].iter())
            .map(String::as_str)
            .collect();
        if !extra.is_empty() {
            eprintln!("mkfat: --fixture16 单飞（多余参数 {extra:?}）");
            return ExitCode::from(2);
        }
        let (vol, geo) = match fixture_volume(true) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
        let n = prefix_len(&geo);
        if let Err(e) = std::fs::write(fx, &vol[..n]) {
            return usage_fail(&format!("写不了 {fx}：{e}"));
        }
        println!(
            "mkfat: {fx} {n}B (fat16 fixture) clusters={} fat={}x{} spc={}",
            geo.clusters,
            geo.fat_sz,
            mkfat::NUM_FATS,
            geo.spc
        );
        return ExitCode::SUCCESS;
    }
    let mut put_specs: Vec<String> = Vec::new();
    let mut size_mib: Option<usize> = None;
    let mut fat16 = false;
    let mut fixture_path: Option<String> = None;
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
            "--fixture" => {
                let Some(v) = args.get(i + 1) else {
                    return usage_fail("--fixture 需要一个路径参数");
                };
                fixture_path = Some(v.clone());
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
        if fixture_path.is_some() {
            return usage_fail("--put 与 --fixture 互斥");
        }
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
    // 默认盘 / fixture 模式（二刀全量面）：--fixture = FIXTURE_DIRS 专列卷
    // 整盘 + 卷前缀至末链簇双写（smoke.sh C-0 刀 4 目录盘产线形）；裸调 =
    // 默认盘（QEMU 冒烟盘零漂移硬线——fixture 子目录不进默认卷）。
    let dirs = fixture_dirs();
    let dirs_ref: Vec<(&[u8], &[(Vec<u8>, Vec<u8>)])> = if fixture_path.is_some() {
        dirs.iter().map(|(n, fs)| (*n, fs.as_slice())).collect()
    } else {
        Vec::new()
    };
    let (vol, geo) = match mkfat::build_volume(None, &dirs_ref) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let mut disk = vec![0u8; mkfat::DISK_SECTORS * mkfat::SECTOR];
    let off = mkfat::RESERVED_ZONE * mkfat::SECTOR;
    disk[off..off + vol.len()].copy_from_slice(&vol);
    if let Err(e) = std::fs::write(&out_path, &disk) {
        return usage_fail(&format!("写不了 {out_path}：{e}"));
    }
    let mut line = format!(
        "mkfat: {out_path} {}B vol@LBA{} clusters={} fat={}x{}",
        disk.len(),
        mkfat::RESERVED_ZONE,
        geo.clusters,
        geo.fat_sz,
        mkfat::NUM_FATS,
    );
    if let Some(fx) = &fixture_path {
        let n = prefix_len(&geo);
        if let Err(e) = std::fs::write(fx, &vol[..n]) {
            return usage_fail(&format!("写不了 {fx}：{e}"));
        }
        line += &format!(" fixture={fx} {n}B");
    }
    println!("{line}");
    ExitCode::SUCCESS
}
