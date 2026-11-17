//! mkimage Rust 件自检（挂 just test-host）：MBR 薄面钉板（CHS 向量 +
//! 两分区表项黄金字节）+ manifest v1 黄金字节 + 拒面 + 整盘确定性 +
//! CLI 对 Python 现役件 oracle 字节级对拍（Python 缺席 skip——纯面钉板
//! 兜底；CI 净仓先例同款）。

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn python_available() -> bool {
    repo_root().join("tools/mkimage.py").exists()
}

fn fixture_puts() -> Vec<(Vec<u8>, Vec<u8>)> {
    vec![
        (
            b"/EFI/BOOT/BOOTAA64.EFI".to_vec(),
            (0..3000).map(|i| ((i * 7 + 3) % 256) as u8).collect(),
        ),
        (b"/HELLO.TXT".to_vec(), b"berryboot efi hello\n".to_vec()),
        (
            b"/KERNEL.ELF".to_vec(),
            (0..5000).map(|i| ((i * 31 + 7) % 256) as u8).collect(),
        ),
        (b"/INITRD.BIN".to_vec(), b"berryos initrd stub\n".to_vec()),
    ]
}

/// CHS 编码向量（Python selftest 同四腿）。
#[test]
fn chs_vectors() {
    assert_eq!(mkimage::lba_to_chs(0), [0, 1, 0], "LBA0 → CHS(0,0,1)");
    assert_eq!(mkimage::lba_to_chs(2048), [32, 33, 0], "LBA2048 → CHS(0,32,33)");
    assert_eq!(mkimage::lba_to_chs(49151), [15, 12, 3], "LBA49151 → CHS(3,15,12)");
    assert_eq!(
        mkimage::lba_to_chs(1024 * (mkimage::HEADS * mkimage::SPT) as u64),
        [254, 63, 255],
        "超顶钳 (1023,254,63)"
    );
}

/// 表项 0/1 字段钉板 + 96MiB 派生黄金字节（Python selftest 同面）。
#[test]
fn mbr_table_entries() {
    let vol = mkimage::volume_sectors(mkimage::DEFAULT_SIZE_MIB);
    let m = mkimage::build_mbr(mkimage::PART_LBA, vol);
    assert_eq!(m[446], 0x00, "status 非 active");
    assert_eq!(m[450], mkimage::PART_TYPE_FAT16_LBA, "分区类型 0x0E");
    assert_eq!(
        u32::from_le_bytes(m[454..458].try_into().unwrap()),
        mkimage::PART_LBA as u32,
        "起始 LBA 域"
    );
    assert_eq!(
        u32::from_le_bytes(m[458..462].try_into().unwrap()),
        vol as u32,
        "扇区数域"
    );
    assert_eq!(&m[510..512], &[0x55, 0xAA], "签章");
    assert!(m[..446].iter().all(|&b| b == 0), "引导代码区全零");
    let meta_lba = mkimage::PART_LBA + vol;
    assert_eq!(m[462], 0x00, "meta status 非 active");
    assert_eq!(m[466], mkimage::PART_TYPE_KEYMETA, "meta 分区类型 0xDA");
    assert_eq!(
        u32::from_le_bytes(m[470..474].try_into().unwrap()),
        meta_lba as u32,
        "meta 起 LBA 域"
    );
    assert_eq!(
        u32::from_le_bytes(m[474..478].try_into().unwrap()),
        mkimage::META_SECTORS as u32,
        "meta 扇区数 = 16MiB"
    );
    assert_eq!(&m[463..466], &mkimage::lba_to_chs(meta_lba as u64), "meta 起 CHS 与 LBA 自洽");
    assert_eq!(
        &m[467..470],
        &mkimage::lba_to_chs((meta_lba + mkimage::META_SECTORS - 1) as u64),
        "meta 止 CHS"
    );
    assert_eq!(
        m[462..478],
        hex_to_bytes("003c310cda46380e0000030000800000"),
        "表项 1 黄金字节（96MiB 派生定值）"
    );
    assert_eq!(&m[478..510], &[0u8; 32], "表项 2-3 全零");
}

fn hex_to_bytes(s: &str) -> [u8; 16] {
    let b: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect();
    b.try_into().unwrap()
}

/// manifest v1 黄金字节（格式冻结执法点——Python selftest 同载荷同基准；
/// PR 单源绊线：abi.toml level 与黄金 PR 列同笔冻结）。
#[test]
fn manifest_v1_golden() {
    let puts = fixture_puts();
    let golden = concat!(
        "berryos-manifest v1 format_frozen=2026-11-10 entries=04\n",
        "item path=/EFI/BOOT/BOOTAA64.EFI   ",
        "sha256=f541874101876255b4baf3a739778d04cb9cba25ffa38b30bc1fb8b0701f2a45 ",
        "size=000000003000 rollback_index=00000000000000000000 ",
        "platform_revision=0000000001\n",
        "item path=/HELLO.TXT               ",
        "sha256=785594f64f783cfaac356d9f75ba42d369e414e316ad1e8649b41090e6787526 ",
        "size=000000000020 rollback_index=00000000000000000000 ",
        "platform_revision=0000000001\n",
        "item path=/INITRD.BIN              ",
        "sha256=6c150fd292a1c404285bddf840b1a77cd9d479555b4e50c3dd98030779de29ba ",
        "size=000000000020 rollback_index=00000000000000000000 ",
        "platform_revision=0000000001\n",
        "item path=/KERNEL.ELF              ",
        "sha256=1e92fd98f113aba0a78e0830ca06e2775912370feab112dfc57bf3258b810595 ",
        "size=000000005000 rollback_index=00000000000000000000 ",
        "platform_revision=0000000001\n",
        "key_hash=none\n",
    );
    let pr = mkimage::platform_level(&repo_root()).expect("abi.toml level 可读");
    assert_eq!(
        format!("{pr:010}"),
        "0000000001",
        "abi.toml [platform].level = {pr} 与黄金 PR 列（0000000001）脱钩——升 level 须同笔连动黄金 PR 列（双侧）"
    );
    let got = mkimage::build_manifest(&puts, None, pr).unwrap();
    assert_eq!(got, golden.as_bytes(), "manifest v1 黄金字节漂移（破冻——格式改动须升 v2）");
    // 行序冻结面：传参次序无关（数据自定序 = 路径字节序）。
    let mut rev = puts.clone();
    rev.reverse();
    assert_eq!(
        mkimage::build_manifest(&rev, None, pr).unwrap(),
        got,
        "manifest 行序随传参漂移"
    );
    // PR 注入差集 = 恰列值（其余列/头/尾零扰动）。
    let got7 = mkimage::build_manifest(&puts, None, 7).unwrap();
    let l7 = String::from_utf8(got7.clone()).unwrap();
    for (a, b) in l7.lines().zip(String::from_utf8(got.clone()).unwrap().lines()) {
        let want = b.replace("platform_revision=0000000001", "platform_revision=0000000007");
        assert_eq!(a, want, "PR 注入差集非恰列值");
    }
    // 拒面：u32 域外 loud 拒。
    for bad in [1u64 << 32, 1u64 << 40] {
        assert!(
            mkimage::build_manifest(&puts, None, bad).is_err(),
            "坏 platform_revision 未被拒绝：{bad}"
        );
    }
    // path 列宽 loud 守卫：24 字符边界正例不误拒，25+ 拒。
    let p24 = format!("/EFI/BOOT/BOOTAA64.EFI{}", "XX"); // 22 + 2 = 恰上界
    assert_eq!(p24.len(), mkimage::MANIFEST_PATH_W);
    let m24 = mkimage::build_manifest(&[(p24.clone().into_bytes(), b"x".to_vec())], None, 1).unwrap();
    let m24s = String::from_utf8(m24).unwrap();
    assert!(
        m24s.contains(&format!("path={p24} sha256=")),
        "24 字符边界路径应原样成列（单空格分隔）"
    );
    let p25 = format!("/{}", "a".repeat(mkimage::MANIFEST_PATH_W));
    assert!(
        mkimage::build_manifest(&[(p25.into_bytes(), b"x".to_vec())], None, 1).is_err(),
        "超宽 manifest path 未被拒绝"
    );
    // 件 5：key_hash 注入差集恰尾行 + 坏值拒面。
    let kh = "0123456789abcdef".repeat(4);
    let with_kh = mkimage::build_manifest(&puts, Some(&kh), pr).unwrap();
    assert_eq!(
        with_kh,
        golden.replace("key_hash=none", &format!("key_hash={kh}")).into_bytes(),
        "key_hash 注入面差集非恰一行"
    );
    assert!(mkimage::parse_key_hash(&kh).is_ok());
    for bad in ["0x".to_string() + &"ab".repeat(32), "ab".repeat(31), "ab".repeat(33), "zz".repeat(32)] {
        assert!(mkimage::parse_key_hash(&bad).is_err(), "坏 key-hash 未被拒绝：{bad}");
    }
    // G8：manifest 名参数化面。
    assert_eq!(mkimage::manifest_file_name(None).unwrap(), "manifest.txt");
    assert_eq!(
        mkimage::manifest_file_name(Some("acme.manifest.txt")).unwrap(),
        "acme.manifest.txt"
    );
    for bad in ["", "a/b.txt", "..", ".", "a\\b.txt", "/x"] {
        assert!(
            mkimage::manifest_file_name(Some(bad)).is_err(),
            "坏 --manifest-name 未被拒绝：{bad}"
        );
    }
}

/// 整盘装配确定性（9MiB FAT16 最小量级卷）：双跑恒等 + FAT 段与 mkfat
/// 直产零漂移 + 预留区恒全零 + 总盘几何（件 4「卷面全不动」钉板）。
#[test]
fn image_assembly_determinism() {
    let puts = fixture_puts();
    let (img, _) = mkimage::build_image(&puts, 9).unwrap();
    assert_eq!(img, mkimage::build_image(&puts, 9).unwrap().0, "整盘装配不确定");
    let (vol_direct, _) = mkfat::build_volume_puts(
        &puts,
        mkimage::volume_sectors(9),
        true,
        mkimage::PART_LBA as u32,
    )
    .unwrap();
    let meta_off = (mkimage::PART_LBA + mkimage::volume_sectors(9)) * mkimage::SECTOR;
    assert_eq!(
        &img[mkimage::PART_LBA * mkimage::SECTOR..meta_off],
        &vol_direct[..],
        "FAT 段漂移"
    );
    assert!(
        img[meta_off..].iter().all(|&b| b == 0),
        "预留区非全零"
    );
    assert_eq!(img.len(), 9 * 1024 * 1024 + mkimage::META_SECTORS * mkimage::SECTOR, "总盘 = 9MiB + 16MiB");
    // 挂接面（源实例 + 验签两件）：整盘确定性 + FAT 段零漂移。
    let mut puts6 = puts.clone();
    puts6.push((
        mkimage::SRCINST_FAT_INDEX.to_vec(),
        (0..400).map(|i| ((i * 11 + 5) % 256) as u8).collect(),
    ));
    puts6.push((mkimage::SRCINST_FAT_KEYS.to_vec(), b"berryos-source-keys v1 stub\n".to_vec()));
    let (img6, _) = mkimage::build_image(&puts6, 9).unwrap();
    assert_eq!(img6, mkimage::build_image(&puts6, 9).unwrap().0, "挂接后整盘装配不确定");
    let (vol6, _) = mkfat::build_volume_puts(&puts6, mkimage::volume_sectors(9), true, mkimage::PART_LBA as u32).unwrap();
    assert_eq!(&img6[mkimage::PART_LBA * mkimage::SECTOR..meta_off], &vol6[..], "挂接后 FAT 段漂移（单编码器纪律）");
    // 源实例读取面：缺件/非目录拒 + 合成两件套成材。
    let td = std::env::temp_dir().join(format!("mkimage-st-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&td);
    std::fs::create_dir_all(&td).unwrap();
    assert!(mkimage::load_source_instance("not-a-dir").is_err(), "非目录未拒");
    assert!(mkimage::load_source_instance(td.to_str().unwrap()).is_err(), "空目录未拒");
    std::fs::write(td.join(mkimage::SRCINST_INDEX_NAME), [0u8; 400]).unwrap();
    assert!(mkimage::load_source_instance(td.to_str().unwrap()).is_err(), "缺 keys 未拒");
    std::fs::write(td.join(mkimage::SRCINST_KEYS_NAME), b"k\n").unwrap();
    let si = mkimage::load_source_instance(td.to_str().unwrap()).unwrap();
    assert_eq!(si.len(), 2, "两件套成材");
    assert_eq!(si[0].0, mkimage::SRCINST_FAT_INDEX);
    let _ = std::fs::remove_dir_all(&td);
}

/// P3 验签组装面：几何校验 + manifest 六件形态 + 尺寸拒面逐腿。
#[test]
fn sign_face() {
    let sig: Vec<u8> = (0..mkimage::SIGN_LEN).map(|i| ((i * 13 + 7) % 256) as u8).collect();
    let pk: Vec<u8> = (0..mkimage::TRUSTPK_LEN).map(|i| ((i * 5 + 1) % 256) as u8).collect();
    let sp = mkimage::sign_face_puts(&sig, &pk).unwrap();
    assert_eq!(sp[0].0, mkimage::SIGN_FAT_PATH);
    assert_eq!(sp[1].0, mkimage::TRUSTPK_FAT_PATH);
    for bad_len in [63, 65] {
        assert!(
            mkimage::sign_face_puts(&vec![b'x'; bad_len], &pk).is_err(),
            "坏签名尺寸未拒：{bad_len}B"
        );
    }
    for bad_len in [31, 33] {
        assert!(
            mkimage::sign_face_puts(&sig, &vec![b'x'; bad_len]).is_err(),
            "坏公钥尺寸未拒：{bad_len}B"
        );
    }
    // manifest 六件形态：entries=06 + 验签两件入列（行序 = 路径字节序）。
    let pr = mkimage::platform_level(&repo_root()).unwrap();
    let mut puts6 = fixture_puts();
    puts6.extend(sp);
    let m6 = String::from_utf8(mkimage::build_manifest(&puts6, None, pr).unwrap()).unwrap();
    let l6: Vec<&str> = m6.lines().collect();
    assert!(l6[0].ends_with("entries=06"), "头计数");
    assert_eq!(l6.len(), 8, "行数 = 1 头 + 6 条 + 1 尾");
    let paths: Vec<String> = l6[1..7]
        .iter()
        .map(|ln| ln.split("path=").nth(1).unwrap().split(' ').next().unwrap().to_string())
        .collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "行序 = 路径字节序");
    assert!(paths.iter().any(|p| p == "/KERNEL.SIG"), "验签两件入列");
    assert!(paths.iter().any(|p| p == "/TRUSTPK"), "验签两件入列");
}

/// CLI oracle 对拍：Rust bin 与 Python mkimage.py 同输入整盘 + manifest
/// 逐字节恒等（sha256 双侧）+ stdout 三行全等。Python 缺席 skip（CI 净仓
/// 先例：纯面钉板兜底）。
#[test]
fn oracle_byte_parity() {
    if !python_available() {
        eprintln!("mkimage: tools/mkimage.py 缺席——CLI 对拍 skip（纯面钉板兜底）");
        return;
    }
    let repo = repo_root();
    let td = std::env::temp_dir().join(format!("mkimage-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&td);
    std::fs::create_dir_all(&td).unwrap();
    let efi = td.join("BOOTAA64.EFI");
    std::fs::write(&efi, (0..3000).map(|i| ((i * 7 + 3) % 256) as u8).collect::<Vec<_>>()).unwrap();
    let hello = td.join("HELLO.TXT");
    std::fs::write(&hello, b"berryboot efi hello\n").unwrap();
    let kernel = td.join("KERNEL.ELF");
    std::fs::write(&kernel, (0..5000).map(|i| ((i * 31 + 7) % 256) as u8).collect::<Vec<_>>()).unwrap();
    let initrd = td.join("rootfs.cpio");
    std::fs::write(&initrd, b"berryos initrd stub\n").unwrap();

    let host = {
        let o = std::process::Command::new("rustc").arg("-vV").output().unwrap();
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
            .unwrap()
    };
    let cargo_target = repo.join("tools/mkimage/cargo-target");
    let build = std::process::Command::new("cargo")
        .args(["build", "--release", "--manifest-path", "tools/mkimage/Cargo.toml",
               "--target", &host, "--target-dir"])
        .arg(&cargo_target)
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(build.status.success(), "Rust mkimage build 失败：{}", String::from_utf8_lossy(&build.stderr));
    let rust_bin = cargo_target.join(&host).join("release").join("mkimage");

    let run = |bin: &std::path::Path, out: &std::path::Path, size: &str| {
        std::process::Command::new(bin)
            .arg(out)
            .args(["--efi", efi.to_str().unwrap(), "--hello", hello.to_str().unwrap(),
                   "--kernel", kernel.to_str().unwrap(), "--initrd", initrd.to_str().unwrap(),
                   "--size-mib", size])
            .current_dir(&repo)
            .output()
            .unwrap()
    };
    // 面 1：9MiB 合成四件套。
    let (py_out, rs_out) = (td.join("py.img"), td.join("rs.img"));
    let pyr = std::process::Command::new("python3")
        .arg("tools/mkimage.py").arg(&py_out)
        .args(["--efi", efi.to_str().unwrap(), "--hello", hello.to_str().unwrap(),
               "--kernel", kernel.to_str().unwrap(), "--initrd", initrd.to_str().unwrap(),
               "--size-mib", "9"])
        .current_dir(&repo).output().unwrap();
    assert!(pyr.status.success(), "Python mkimage 跑挂：{}", String::from_utf8_lossy(&pyr.stderr));
    let rsr = run(&rust_bin, &rs_out, "9");
    assert!(rsr.status.success(), "Rust mkimage 跑挂：{}", String::from_utf8_lossy(&rsr.stderr));
    let pyi = std::fs::read(&py_out).unwrap();
    let rsi = std::fs::read(&rs_out).unwrap();
    assert_eq!(pyi.len(), rsi.len(), "整盘长度不等");
    assert_eq!(
        mkimage::sha256_hex(&pyi),
        mkimage::sha256_hex(&rsi),
        "整盘字节级对拍破（sha256 不等）"
    );
    eprintln!("mkimage: oracle 整盘对拍 PASS（{}B sha256={}）", pyi.len(), mkimage::sha256_hex(&pyi));
    // manifest 逐字节恒等：同目录缺省名 manifest.txt 会互踩——两侧产物
    // 分居 td/py、td/rs 子目录。
    std::fs::create_dir_all(td.join("py")).unwrap();
    std::fs::create_dir_all(td.join("rs")).unwrap();
    let pyr2 = std::process::Command::new("python3")
        .arg("tools/mkimage.py").arg(td.join("py/a.img"))
        .args(["--efi", efi.to_str().unwrap(), "--hello", hello.to_str().unwrap(),
               "--kernel", kernel.to_str().unwrap(), "--initrd", initrd.to_str().unwrap(),
               "--size-mib", "9"])
        .current_dir(&repo).output().unwrap();
    assert!(pyr2.status.success(), "Python 重跑挂：{}", String::from_utf8_lossy(&pyr2.stderr));
    let rsr2 = run(&rust_bin, &td.join("rs/a.img"), "9");
    assert!(rsr2.status.success(), "Rust 重跑挂");
    let pym = std::fs::read(td.join("py/manifest.txt")).unwrap();
    let rsm = std::fs::read(td.join("rs/manifest.txt")).unwrap();
    assert_eq!(pym, rsm, "manifest 侧产物字节不等");
    eprintln!("mkimage: oracle manifest 对拍 PASS（{}B）", rsm.len());
    // 面 2：默认 96MiB 盘容（manifest 名分侧——同名互踩）。
    let (py96, rs96) = (td.join("py/96.img"), td.join("rs/96.img"));
    for (bin, argv0, out, mname) in [
        ("python3", "tools/mkimage.py", &py96, "m96-py.txt"),
        (rust_bin.to_str().unwrap(), "", &rs96, "m96-rs.txt"),
    ] {
        let mut c = std::process::Command::new(bin);
        if !argv0.is_empty() {
            c.arg(argv0);
        }
        let r = c.arg(out)
            .args(["--efi", efi.to_str().unwrap(), "--hello", hello.to_str().unwrap(),
                   "--kernel", kernel.to_str().unwrap(), "--initrd", initrd.to_str().unwrap(),
                   "--manifest-name", mname])
            .current_dir(&repo).output().unwrap();
        assert!(r.status.success(), "{bin} 96MiB 跑挂：{}", String::from_utf8_lossy(&r.stderr));
    }
    let p96 = std::fs::read(&py96).unwrap();
    let r96 = std::fs::read(&rs96).unwrap();
    assert_eq!(mkimage::sha256_hex(&p96), mkimage::sha256_hex(&r96), "96MiB 整盘对拍破");
    assert_eq!(std::fs::read(td.join("py/m96-py.txt")).unwrap(), std::fs::read(td.join("rs/m96-rs.txt")).unwrap());
    eprintln!("mkimage: oracle 96MiB 对拍 PASS（{}B）", r96.len());
    // 面 3：--key-hash 注入（盘字节零漂移面——key_hash 只进 manifest）。
    let kh = "0123456789abcdef".repeat(4);
    let (pykh, rskh) = (td.join("py/kh.img"), td.join("rs/kh.img"));
    for (bin, argv0, out, mname) in [
        ("python3", "tools/mkimage.py", &pykh, "kh-py.txt"),
        (rust_bin.to_str().unwrap(), "", &rskh, "kh-rs.txt"),
    ] {
        let mut c = std::process::Command::new(bin);
        if !argv0.is_empty() {
            c.arg(argv0);
        }
        let r = c.arg(out)
            .args(["--efi", efi.to_str().unwrap(), "--hello", hello.to_str().unwrap(),
                   "--kernel", kernel.to_str().unwrap(), "--initrd", initrd.to_str().unwrap(),
                   "--size-mib", "9", "--key-hash", &kh, "--manifest-name", mname])
            .current_dir(&repo).output().unwrap();
        assert!(r.status.success(), "{bin} key-hash 跑挂：{}", String::from_utf8_lossy(&r.stderr));
    }
    assert_eq!(
        std::fs::read(&pykh).unwrap(),
        std::fs::read(&rskh).unwrap(),
        "key-hash 面整盘不等"
    );
    let khm = String::from_utf8(std::fs::read(td.join("rs/kh-rs.txt")).unwrap()).unwrap();
    assert!(khm.ends_with(&format!("key_hash={kh}\n")), "key_hash 未落 manifest 尾行");
    eprintln!("mkimage: oracle key-hash 面 PASS");
    let _ = std::fs::remove_dir_all(&td);
}

/// sha256 实现自检（对抗 FIPS 180-4 标准向量——哈希实现自身的门禁；
/// golden manifest 的哈希值双侧（Python hashlib 与本实现）必须同值）。
#[test]
fn sha256_standard_vectors() {
    assert_eq!(
        mkimage::sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        mkimage::sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        mkimage::sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}
