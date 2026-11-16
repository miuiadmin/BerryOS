//! 钉板五步第 3/4 步：字节级对拍 + selftest 全绿（Python 版面逐条对齐）。
//!
//! 对拍纪律：同输入 → 产物逐字节恒等（现役「同输入同字节」确定性纪律）。
//! 双路：①Rust 纯面钉板（Python selftest_split 逐腿对齐——垫页几何走查/
//! 重装恒等/卷命名/陈卷清理/manifest 拒面）；②与 Python 现役件真跑对拍
//! （CLI 子进程：合成条目集 → 双方归档 sha256 恒等——`tools/mkcpio.py`
//! 在席时执行；缺席打 skip〔CI 环境净仓面〕，纯面钉板兜底）。

use mkcpio::*;

/// Python selftest_split 合成基准同款：4 条目（非 4 对齐长度让数据垫生效）。
fn synth_items() -> Vec<(Vec<u8>, Vec<u8>)> {
    vec![
        (b"bin/aa".to_vec(), vec![0x01u8; 37]),
        (b"bin/bb".to_vec(), vec![0x02u8; 1]),
        (b"etc/cc".to_vec(), vec![0x03u8; 4096]),
        (b"dd".to_vec(), vec![0x04u8; 123]),
    ]
}

#[test]
fn pad_geometry_walk() {
    // ⑦ X-3c 垫页几何钉：预测边界步进全档走查——每条头魔数落在预测起点、
    // data_off 恒页对齐、数据逐字节在位、步进终点 == 归档总长（与消费端
    // cpio_step/_cpio_entries 同口径，改 CPIO_PAD 此锚必炸）。
    let items = synth_items();
    let blob = build_archive(&items);
    assert!(blob.len() > 128, "合成归档体量不足以覆盖多边界形态");
    let mut off = 0usize;
    for (nm, dt) in items.iter().chain(std::iter::once(&(b"TRAILER!!!".to_vec(), Vec::new()))) {
        assert_eq!(&blob[off..off + 6], b"070701", "头边界漂 @ {off}");
        let data_off = (off + 110 + nm.len() + 1 + CPIO_PAD - 1) & !(CPIO_PAD - 1);
        assert_eq!(data_off % CPIO_PAD, 0, "data_off 非页对齐 @ {off}");
        if !dt.is_empty() {
            assert_eq!(&blob[data_off..data_off + dt.len()], &dt[..], "数据漂 @ {data_off}");
        }
        off = (data_off + dt.len() + CPIO_PAD - 1) & !(CPIO_PAD - 1);
    }
    assert_eq!(off, blob.len(), "步进终点 {off} != 归档总长 {}", blob.len());
}

#[test]
fn split_faces() {
    let blob = build_archive(&synth_items());
    // ① 重装恒等 + 卷帽 + 无空尾卷：任意卷宽（含 1B/全长-1/恰全长/超全长）。
    for vol in [1usize, 7, 64, blob.len() - 1, blob.len(), blob.len() + 100] {
        let vols = split_volumes(&blob, vol).unwrap();
        assert_eq!(vols.concat(), blob, "重装恒等破 @vol={vol}");
        assert!(vols.iter().all(|v| v.len() <= vol), "卷越界 @vol={vol}");
        assert!(!vols.is_empty() && !vols.last().unwrap().is_empty(), "空尾卷 @vol={vol}");
    }
    // ② 恰整数倍长度：卷数恰商、每卷满宽。
    let n = (blob.len() / 64) * 64;
    let vols = split_volumes(&blob[..n], 64).unwrap();
    assert_eq!(vols.len(), n / 64, "恰整数倍卷数");
    assert!(vols.iter().all(|v| v.len() == 64), "恰整数倍每卷满宽");
    // ③ 单卷形态。
    let vols = split_volumes(&blob, blob.len()).unwrap();
    assert_eq!(vols.len(), 1, "单卷卷数");
    assert_eq!(vols[0], blob, "单卷逐字节同");
    // ⑤ 非正卷宽拒。
    assert_eq!(split_volumes(&blob, 0), Err(MkcpioError::BadVolBytes));
}

#[test]
fn entry_geometry_single() {
    // 单条目几何：头 110B（6+13×8）+ 名 + NUL 垫到页；数据垫到页；
    // ino 递增字段、filesize、namesize 的 hex8 落位抽验。
    let e = entry(b"bin/aa", &[0xAA; 37], 3);
    assert_eq!(&e[..6], b"070701");
    assert_eq!(&e[6..14], b"00000003", "ino=3");
    assert_eq!(&e[14..22], b"000081ED", "mode 0755");
    assert_eq!(&e[54..62], b"00000025", "filesize=37");
    assert_eq!(&e[94..102], b"00000007", "namesize=7（含 NUL）");
    assert_eq!(e.len(), 4096 + 4096, "头页+数据页");
    // TRAILER：ino=0、空数据、名 10B 含 NUL=11 → 头页恰 4096。
    let t = entry(b"TRAILER!!!", b"", 0);
    assert_eq!(t.len(), CPIO_PAD, "TRAILER 单页");
}

#[test]
fn clean_stale_volumes_walk() {
    // ⑥ 陈卷清理：多卷→少卷收缩时 ≥keep 序号卷被清、<keep 存活；keep=0
    // 全清；幂等返 0。真实文件系统面（tempdir）。
    let td = std::env::temp_dir().join(format!("mkcpio-st-{}", std::process::id()));
    std::fs::create_dir_all(&td).unwrap();
    let vol = |i: usize| td.join(format!("rootfs{i}.cpio"));
    for i in 0..3 {
        std::fs::write(vol(i), [b'a' + i as u8]).unwrap();
    }
    let root = td.join("rootfs.cpio");
    assert_eq!(clean_stale_volumes(root.to_str().unwrap(), 2), 1, "收缩清 1 卷");
    assert!(vol(0).exists() && vol(1).exists(), "<keep 存活");
    assert!(!vol(2).exists(), "≥keep 已清");
    assert_eq!(clean_stale_volumes(root.to_str().unwrap(), 0), 2, "全清恰余量");
    assert_eq!(clean_stale_volumes(root.to_str().unwrap(), 0), 0, "空集幂等返 0");
    let _ = std::fs::remove_dir_all(&td);
}

#[test]
fn sig_faces() {
    // 伴生 .sig 收编四性质（Python selftest_sig_companion 逐腿对齐——
    // 纯函数面，exists 闭包注入在席性）。
    let items: Vec<String> = vec![
        "app/a.bap=td/a.bap".into(),
        "bin/zz=td/zz".into(),
        "app/b.bap=td/b.bap".into(),
        "app/b.bap.sig=td/explicit.sig".into(),
    ];
    // 直映：a.bap 伴生在席 → 恰追加一条直映。
    let out = sig_companions(&items, |p| p == "td/a.bap.sig");
    assert_eq!(out, vec!["app/a.bap.sig=td/a.bap.sig".to_string()]);
    // 非 .bap 不收：zz.sig 在席也不动（上腿已证——out 无 bin/zz.sig）。
    // 显式胜出：b.bap.sig 显式在表 → 不自动收编（out 仅 1 条）。
    // 缺席零漂移：全不在席 → 零追加。
    assert!(sig_companions(&items, |p| p == "td/zz.sig").is_empty(), "非 .bap stray 不收");
    assert!(sig_companions(&items, |_| false).is_empty(), "缺席零漂移");
}

#[test]
fn determinism() {
    // 确定性：同输入两次构建逐字节恒等（「同输入同字节」纪律）。
    let a = build_archive(&synth_items());
    let b = build_archive(&synth_items());
    assert_eq!(a, b);
}

// ─── 与 Python 现役件字节级对拍（钉板五步第 3 步） ─────────────────────
//
// Python `tools/mkcpio.py` 在席时：合成条目集（含 .bap 伴生面——CLI 子
// 进程真跑）→ 双方归档逐字节恒等。缺席打 skip（CI 净仓面）——纯面钉板
// 兜底。注意：CLI 面含 .berryver 烙印（git ident + 双单源现值）——双方
// 同窗取数，同窗内 ident 定值，恒等成立。

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn python_available() -> bool {
    repo_root().join("tools/mkcpio.py").exists()
}

fn sha256_hex(data: &[u8]) -> String {
    // 零依赖 sha256（FIPS 180-4）——elf2pe oracle 对拍同款自包含面。
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bitlen = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

fn run_cmd(cmd: &[&str], cwd: &std::path::Path) -> (bool, String, String) {
    match std::process::Command::new(cmd[0]).args(&cmd[1..]).current_dir(cwd).output() {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        ),
        Err(_) => (false, String::new(), String::new()),
    }
}

#[test]
fn oracle_byte_parity() {
    let _ = run_oracle_byte_parity();
}

fn run_oracle_byte_parity() -> Option<()> {
    if !python_available() {
        eprintln!("mkcpio: tools/mkcpio.py 缺席——CLI 对拍 skip（纯面钉板兜底）");
        return None;
    }
    let td = std::env::temp_dir().join(format!("mkcpio-parity-{}", std::process::id()));
    std::fs::create_dir_all(&td).unwrap();
    let _ = clean_stale_volumes(td.join("o.cpio").to_str().unwrap(), 0);
    // 布景：多形态条目集（非对齐长度/恰页长/.bap 伴生/名序深浅）。
    let f1 = td.join("a.bin");
    std::fs::write(&f1, [0x11u8; 33]).unwrap();
    let f2 = td.join("big.bin");
    std::fs::write(&f2, [0x22u8; 4096]).unwrap();
    let bap = td.join("app.bap");
    std::fs::write(&bap, [0x33u8; 9]).unwrap();
    std::fs::write(td.join("app.bap.sig"), [0x44u8; 21]).unwrap();
    // Rust CLI：先 build 再跑（cargo-target 形态与产链一致）。
    let repo = repo_root();
    let host = {
        let (_, o, _) = run_cmd(&["rustc", "-vV"], &repo);
        o.lines().find_map(|l| l.strip_prefix("host: ").map(str::to_string)).unwrap_or_default()
    };
    let (ok, _, err) = run_cmd(
        &["cargo", "build", "--release", "--manifest-path", "tools/mkcpio/Cargo.toml", "--target", &host],
        &repo,
    );
    if !ok {
        panic!("Rust mkcpio build 失败：{err}");
    }
    let rust_bin = repo.join(format!("tools/mkcpio/cargo-target/{host}/release/mkcpio"));
    let args_spec = |out: &str| vec![
        out.to_string(),
        format!("bin/aa={}", f1.display()),
        format!("etc/big={}", f2.display()),
        format!("app/app.bap={}", bap.display()),
    ];
    // Python CLI。
    let py_out = td.join("py.cpio");
    let (ok, _, err) = run_cmd(
        &["python3", "tools/mkcpio.py", py_out.to_str().unwrap(), &args_spec("py")[1], &args_spec("py")[2], &args_spec("py")[3]],
        &repo,
    );
    assert!(ok, "Python CLI 跑挂：{err}");
    // Rust CLI。
    let rs_out = td.join("rs.cpio");
    let a = args_spec("rs");
    let (ok, _, err) = run_cmd(
        &[
            rust_bin.to_str().unwrap(),
            rs_out.to_str().unwrap(),
            &a[1].clone(),
            &a[2].clone(),
            &a[3].clone(),
        ],
        &repo,
    );
    assert!(ok, "Rust CLI 跑挂：{err}");
    let py = std::fs::read(&py_out).unwrap();
    let rs = std::fs::read(&rs_out).unwrap();
    assert_eq!(sha256_hex(&py), sha256_hex(&rs), "CLI 归档字节级对拍破（sha256 不等）");
    eprintln!("mkcpio: oracle 对拍 PASS（{}B sha256={}）", py.len(), sha256_hex(&py));
    let _ = std::fs::remove_dir_all(&td);
    Some(())
}
