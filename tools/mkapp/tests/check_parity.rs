//! W1-Q27 六刀·--check 对拍：Rust `check_bap` 五步 ↔ Python `mkapp.py
//! --check` oracle 真跑对拍（build_parity 同款纪律）。
//!
//! 正面 = 双侧同过 + OK 行（剥路径段）全等；拒面 = 双侧同拒同 code
//! （bap.size/bap.cpio.dup/bap.cpio.shape/bap.icons.name/bap.views.empty/
//! manifest.*/entry.mismatch/bap.elf/bap.ui.cross 全谱抽查）+ Python
//! stderr 文案关键词对照。敌意包走 mkcpio `build_archive` 真编码（非
//! 手搓字节——拒面来自真实走查路径）。Python 缺席打 skip（CI 净仓面
//! ——纯面钉板兜底在 src/check.rs 单测）。

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

// ---- 敌意包工厂（mkcpio 真编码；PNG/BYI 复用 build_parity 桩件思路）----

fn png_chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
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
    let mut out = Vec::new();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc_in = tag.to_vec();
    crc_in.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
    out
}

/// 最小合法 8-bit RGBA PNG（stored zlib——build_parity::mk_png 同款）。
fn mk_png(w: u32, h: u32) -> Vec<u8> {
    let mut raw = Vec::new();
    for _ in 0..h {
        raw.push(0u8);
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
    let mut png = b"\x89PNG\r\n\x1a\x10".to_vec();
    png[7] = 0x0A;
    let ihdr = [w.to_be_bytes().to_vec(), h.to_be_bytes().to_vec(), vec![8, 6, 0, 0, 0]].concat();
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &z));
    png.extend_from_slice(&png_chunk(b"IEND", b""));
    png
}

const MANIFEST: &str = "[app]\nid = \"chkdemo\"\nversion = \"0.1.0\"\nentry = \"bin/chkdemo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[runtime]\nstack_pages = 8\n";

/// UI 形 manifest（icon+views 声明在席——GUI-7 交叉门互指成立）。
const MANIFEST_UI: &str = "[app]\nid = \"chkdemo\"\nversion = \"0.1.0\"\nentry = \"bin/chkdemo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[runtime]\nstack_pages = 8\n\n[ui]\nicon = \"icons/app.byi\"\nlabel = \"Chk\"\nviews = \"views/main\"\n";

fn sums_for(items: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut sorted = items.to_vec();
    sorted.sort();
    let mut out = Vec::new();
    for (name, data) in &sorted {
        out.extend_from_slice(mkapp::cpio::sha256_hex(data).as_bytes());
        out.extend_from_slice(b"  ");
        out.extend_from_slice(name);
        out.push(b'\n');
    }
    out
}

/// 好包工厂：manifest + ELF +（可选 SHA256SUMS/icons/views）。
fn good_bap(with_sums: bool, with_ui: bool) -> Vec<u8> {
    let manifest = if with_ui { MANIFEST_UI } else { MANIFEST };
    let mut items: Vec<(Vec<u8>, Vec<u8>)> = vec![
        (b"manifest.toml".to_vec(), manifest.as_bytes().to_vec()),
        (b"bin/chkdemo".to_vec(), b"\x7fELF\xA5\xA5".to_vec()),
    ];
    if with_ui {
        let byi = mkapp::byi::png_to_byi(&mk_png(2, 2), "app.png").expect("png→byi");
        items.push((b"icons/app.byi".to_vec(), byi));
        items.push((b"views/main".to_vec(), b"# view\n".to_vec()));
    }
    if with_sums {
        let sums = sums_for(&items);
        items.push((b"SHA256SUMS".to_vec(), sums));
    }
    mkcpio::build_archive(&items)
}

#[test]
fn check_parity_oracle() {
    let root = repo_root();
    if !root.join("tools/mkapp.py").exists() || Command::new("python3").arg("--version").output().is_err() {
        eprintln!("mkapp: tools/mkapp.py/python3 缺席——check 对拍 skip（纯面钉板兜底）");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("mkapp-check-parity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    // ---- 正面三组：无 sums 无 UI / sums+UI / sums 无 UI ----
    for (i, (sums, ui)) in [(0, (true, true)), (1, (true, false)), (2, (false, false))] {
        let bap = tmp.join(format!("chk{i}.bap"));
        fs::write(&bap, good_bap(sums, ui)).unwrap();
        // Rust lib 直调
        let rs = mkapp::check::check_bap(bap.to_str().unwrap())
            .unwrap_or_else(|e| panic!("Rust check 拒好包 chk{i}：{} ({})", e.message, e.code));
        // Python oracle
        let (pok, pout, perr) = run_py(&["--check", bap.to_str().unwrap()], &root);
        assert!(pok, "Python --check 拒好包 chk{i}：{perr}");
        // OK 行（剥路径段）全等
        let p_line = pout.lines().next().unwrap_or_default();
        let r_line = rs.line(bap.to_str().unwrap());
        let strip = |l: &str| -> String {
            let parts: Vec<&str> = l.splitn(3, ' ').collect();
            format!("{} {}", parts[0], parts[2])
        };
        assert_eq!(strip(&r_line), strip(p_line), "chk{i}: OK 行不等——R「{r_line}」vs P「{p_line}」");
        // digests 舱
        let want_dig = if sums { "ok" } else { "n/a" };
        assert_eq!(rs.digests, want_dig, "chk{i}: digests 舱不符");
        assert!(p_line.contains(&format!("digests={want_dig}")), "chk{i}: Python digests 舱不符：{p_line}");
    }

    // ---- 拒面全谱：双侧同拒同 code ----
    let mut bad_items: Vec<(Vec<u8>, Vec<u8>)> = vec![
        (b"manifest.toml".to_vec(), MANIFEST.as_bytes().to_vec()),
        (b"bin/chkdemo".to_vec(), b"\x7fELF\xA5\xA5".to_vec()),
    ];
    let mut sums = sums_for(&bad_items);
    bad_items.push((b"SHA256SUMS".to_vec(), sums.clone()));

    let mk_case = |name: &str, items: Vec<(Vec<u8>, Vec<u8>)>| -> PathBuf {
        let p = tmp.join(name);
        fs::write(&p, mkcpio::build_archive(&items)).unwrap();
        p
    };
    // 基线好包（拒面变体都从它派生）
    let base = mk_case("base.bap", bad_items.clone());
    let (ok, _, err) = run_py(&["--check", base.to_str().unwrap()], &root);
    assert!(ok, "基线包应过：{err}");
    assert!(mkapp::check::check_bap(base.to_str().unwrap()).is_ok());

    // 1. bap.cpio.dup：同名双条目
    let mut dup = bad_items.clone();
    dup.push(bad_items[1].clone());
    let p = mk_case("dup.bap", dup);
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "bap.cpio.dup", "dup code：{}", e.code);
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok && perr.contains("重名"), "Python dup 拒面：ok={ok} {perr}");

    // 2. bap.cpio.shape：stray 条目
    let mut stray = bad_items.clone();
    stray.push((b"stray.txt".to_vec(), b"x".to_vec()));
    let p = mk_case("stray.bap", stray);
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "bap.cpio.shape", "stray code：{}", e.code);
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok && perr.contains("条目形不符"), "Python stray 拒面：ok={ok} {perr}");

    // 3. bap.cpio.shape：双 bin/
    let mut twobin = bad_items.clone();
    twobin.push((b"bin/other".to_vec(), b"\x7fELF".to_vec()));
    let p = mk_case("twobin.bap", twobin);
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "bap.cpio.shape");
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok && perr.contains("条目形不符"), "Python twobin：ok={ok} {perr}");

    // 4. bap.icons.name：icons/foo.txt 非 .byi
    let mut badicon = bad_items.clone();
    badicon.push((b"icons/foo.txt".to_vec(), b"x".to_vec()));
    let p = mk_case("badicon.bap", badicon);
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "bap.icons.name", "badicon code：{}", e.code);
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok && perr.contains("icons/"), "Python badicon：ok={ok} {perr}");

    // 5. bap.views.empty
    let mut emptyview = bad_items.clone();
    emptyview.push((b"views/e".to_vec(), Vec::new()));
    let p = mk_case("emptyview.bap", emptyview);
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "bap.views.empty");
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok && perr.contains("为空"), "Python emptyview：ok={ok} {perr}");

    // 6. manifest 域拒（bom 拒在门禁层）：entry 与包内不符
    let mut mismatch = bad_items.clone();
    mismatch[0] = (
        b"manifest.toml".to_vec(),
        MANIFEST.replace("bin/chkdemo", "bin/other").into_bytes(),
    );
    let p = mk_case("mismatch.bap", mismatch);
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "entry.mismatch", "mismatch code：{}", e.code);
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok && perr.contains("!="), "Python mismatch：ok={ok} {perr}");

    // 7. bap.elf：bin 内容非 ELF
    let mut notelf = bad_items.clone();
    notelf[1] = (b"bin/chkdemo".to_vec(), b"MZ\x90\x00".to_vec());
    let p = mk_case("notelf.bap", notelf);
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "bap.elf");
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok && perr.contains("非 ELF"), "Python notelf：ok={ok} {perr}");

    // 8. bap.sums：摘要被篡
    let mut badsum = bad_items.clone();
    let mut tampered = sums.clone();
    tampered[0] = if tampered[0] == b'0' { b'1' } else { b'0' };
    badsum[2] = (b"SHA256SUMS".to_vec(), tampered);
    let p = mk_case("badsum.bap", badsum);
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "bap.sums", "badsum code：{}", e.code);
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok && perr.contains("SHA256SUMS"), "Python badsum：ok={ok} {perr}");

    // 9. bap.ui.cross：manifest 声明 ui.icon 但包内无 icons/
    let m_ui = format!(
        "{MANIFEST}\n[ui]\nicon = \"icons/app.byi\"\nlabel = \"x\"\nviews = \"views/main\"\n"
    );
    let mut cross = vec![
        (b"manifest.toml".to_vec(), m_ui.into_bytes()),
        (b"bin/chkdemo".to_vec(), b"\x7fELF\xA5\xA5".to_vec()),
    ];
    let s = sums_for(&cross);
    cross.push((b"SHA256SUMS".to_vec(), s));
    let p = mk_case("cross.bap", cross);
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "bap.ui.cross", "cross code：{}", e.code);
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok && perr.contains("ui.icon"), "Python cross：ok={ok} {perr}");

    // 10. bap.cpio.header：坏魔数（截断/垃圾头）
    let mut garbage = mkcpio::build_archive(&bad_items);
    garbage[6] = b'X';
    let p = tmp.join("garbage.bap");
    fs::write(&p, garbage).unwrap();
    let e = mkapp::check::check_bap(p.to_str().unwrap()).unwrap_err();
    assert_eq!(e.code, "bap.cpio.header", "garbage code：{}", e.code);
    let (ok, _, perr) = run_py(&["--check", p.to_str().unwrap()], &root);
    assert!(!ok, "Python garbage 应拒");
    assert!(perr.contains("cpio"), "Python garbage 拒面：{perr}");

    let _ = fs::remove_dir_all(&tmp);
}
