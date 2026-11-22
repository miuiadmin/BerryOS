//! W1 gensourceindex 二刀·berry-sign 面对拍：Rust `sign_index`/
//! `verify_index_signature` ↔ Python 现役件（tools/gensourceindex.py
//! `sign_index`/`verify_index_signature`）oracle 真跑对拍。对拍强度 =
//! **.sig 64B 字节恒等**（berry-sign 确定性签名——同钥同覆盖域恒等）+
//! **验签三态双侧同构**（过/篡改拒/换钥拒）+ 诚实降级注文本全等。
//!
//! berry-sign 可得性：发现序任一入口在席（BERRY_SIGN env / 候选位）即跑
//! 全流；全部缺席 = 签面子进程面 skip（src/sign.rs 纯面钉板兜底——发现
//! 序/降级注/内部调用序错不依赖外部件）。测试钥在临时目录 keygen 生成，
//! 零秘密入库（tools/testkeys 面不触）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

struct Proc {
    rc: i32,
    #[allow(dead_code)]
    out: String,
    err: String,
}

fn run(bin: &Path, args: &[&str], env: &[(String, String)]) -> Proc {
    let is_script = bin.extension().map(|e| e == "py").unwrap_or(false);
    let mut c = if is_script {
        let mut c = Command::new("python3");
        c.arg(bin);
        c
    } else {
        Command::new(bin)
    };
    c.args(args)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("BERRY_SIGN");
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().expect("spawn");
    Proc {
        rc: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).into_owned(),
        err: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

/// berry-sign 可得性探测（发现序同款——env 优先，再候选位）。
fn find_host_berry_sign() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("BERRY_SIGN") {
        if !v.is_empty() && Path::new(&v).exists() {
            return Some(PathBuf::from(v));
        }
    }
    let root = repo_root();
    for tdir in ["target-cargo", "/tmp/target-p1", "/tmp/target-p2"] {
        for prof in ["debug", "release"] {
            let base = if Path::new(tdir).is_absolute() {
                PathBuf::from(tdir)
            } else {
                root.join(tdir)
            };
            let cand = base.join("aarch64-apple-darwin").join(prof).join("berry-sign");
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    None
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "gsi-signparity-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// 合成签名预置形（flags.signed=1 + 尾 64B 零槽）——build_index Some 形。
fn preset_form() -> Vec<u8> {
    use gensourceindex::*;
    let e = Entry {
        id: "P2APP".to_string(),
        ver: (0, 1, 0),
        size: 4096,
        digest: core::array::from_fn(|i| i as u8),
        lineage: "berryos".to_string(),
        pk: Some(const_pk(0xA0)),
        last_rev: 1,
        cert: "active".to_string(),
    };
    build_index(&[e], 9, &const_pk(0xA0), Some(&[0u8; SIG_LEN])).unwrap()
}

const fn const_pk(start: u8) -> [u8; 32] {
    let mut a = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        a[i] = start + i as u8;
        i += 1;
    }
    a
}

/// Python oracle：同覆盖域 sign_index 真签 → 64B hex（BERRY_SIGN 显式
/// 注入；sk 临时文件路径透传）。
fn py_sign_index(form_hex: &str, sk: &Path, bs: &Path) -> Result<String, String> {
    let code = format!(
        "import sys; sys.path.insert(0, 'tools')\nimport gensourceindex as g\nsig, note = g.sign_index(bytes.fromhex('{form_hex}'), {sk:?})\nprint(sig.hex())"
    );
    let o = Command::new("python3")
        .arg("-c")
        .arg(&code)
        .current_dir(repo_root())
        .env("BERRY_SIGN", bs)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("spawn python3");
    if o.status.success() {
        Ok(String::from_utf8(o.stdout).unwrap().trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).into_owned())
    }
}

/// Python oracle：verify_index_signature 三态。
fn py_verify_index(raw_hex: &str, pk_hex: &str, sig_hex: &str, bs: &Path) -> Result<String, String> {
    let code = format!(
        "import sys; sys.path.insert(0, 'tools')\nimport gensourceindex as g\nv = g.verify_index_signature(bytes.fromhex('{raw_hex}'), bytes.fromhex('{pk_hex}'), bytes.fromhex('{sig_hex}'))\nprint(v)"
    );
    let o = Command::new("python3")
        .arg("-c")
        .arg(&code)
        .current_dir(repo_root())
        .env("BERRY_SIGN", bs)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("spawn python3");
    if o.status.success() {
        Ok(String::from_utf8(o.stdout).unwrap().trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).into_owned())
    }
}

#[test]
fn sign_parity_sig_bytes_and_verify_states() {
    let Some(bs) = find_host_berry_sign() else {
        eprintln!("gensourceindex: berry-sign 不可得——签名面对拍 skip（纯面钉板兜底：src/sign.rs sign_tests）");
        return;
    };
    let t = tmpdir("full");
    // 测试钥对（临时目录 keygen；sk 0600 由 berry-sign 自理）。
    let (sk, pk_p) = (t.join("dev.sk"), t.join("dev.pk"));
    let kg = run(&bs, &["keygen", sk.to_str().unwrap(), pk_p.to_str().unwrap()], &[]);
    assert!(kg.rc == 0, "keygen 失败：{}", kg.err);
    let pk = fs::read(&pk_p).unwrap();
    assert_eq!(pk.len(), 32);

    // ---- 签名字节恒等（同 sk 同覆盖域——berry-sign 确定性）----
    let form = preset_form();
    let py_sig_hex = py_sign_index(&form.iter().map(|b| format!("{b:02x}")).collect::<String>(), &sk, &bs)
        .expect("python oracle sign_index");
    let (rs_sig, note) = gensourceindex::sign::sign_index(Some(&form), Some(sk.to_str().unwrap()))
        .expect("rust sign_index");
    assert!(note.contains("sign-raw") && note.contains("64B"), "如实注漂移：{note}");
    assert_eq!(
        py_sig_hex,
        rs_sig.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "签名字节双侧不等（同 sk 同覆盖域应恒等）"
    );

    // ---- 验签三态双侧同构 ----
    let mut raw = form.clone();
    let n = raw.len();
    raw[n - 64..].copy_from_slice(&rs_sig); // 换真签名 → 合法 signed 索引。
    let hexs = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let (raw_h, pk_h, sig_h) = (hexs(&raw), hexs(&pk), hexs(&rs_sig));
    // ① 过：双侧 true。
    let py_v = py_verify_index(&raw_h, &pk_h, &sig_h, &bs).unwrap();
    let rs_v = gensourceindex::sign::verify_index_signature(&raw, &pk, &rs_sig);
    assert_eq!(py_v, "True");
    assert_eq!(rs_v, Some(true));
    // ② 篡改拒：覆盖域首字节翻转 → 双侧 false。
    let mut bad = raw.clone();
    bad[0] ^= 0xFF;
    let py_v2 = py_verify_index(&hexs(&bad), &pk_h, &sig_h, &bs).unwrap();
    let rs_v2 = gensourceindex::sign::verify_index_signature(&bad, &pk, &rs_sig);
    assert_eq!(py_v2, "False");
    assert_eq!(rs_v2, Some(false));
    // ③ 换钥拒：异 pk → 双侧 false。
    let other_pk = const_pk(0x50);
    let py_v3 = py_verify_index(&raw_h, &hexs(&other_pk), &sig_h, &bs).unwrap();
    let rs_v3 = gensourceindex::sign::verify_index_signature(&raw, &other_pk, &rs_sig);
    assert_eq!(py_v3, "False");
    assert_eq!(rs_v3, Some(false));

    let _ = fs::remove_dir_all(&t);
}

#[test]
fn unsigned_honest_note_parity() {
    // sk 缺席 = 诚实降级：双侧注文本全等（零子进程——berry-sign 缺席也跑）。
    let (_, rs_note) = gensourceindex::sign::sign_index(None, None).unwrap();
    let code = "import sys; sys.path.insert(0, 'tools')\nimport gensourceindex as g\n_, note = g.sign_index(None, None)\nprint(note)";
    let o = Command::new("python3")
        .arg("-c")
        .arg(code)
        .current_dir(repo_root())
        .env_remove("BERRY_SIGN")
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("spawn python3");
    assert!(o.status.success(), "python oracle 失败：{}", String::from_utf8_lossy(&o.stderr));
    let py_note = String::from_utf8(o.stdout).unwrap().trim().to_string();
    assert_eq!(rs_note, py_note, "unsigned 如实注文本漂移");
}
