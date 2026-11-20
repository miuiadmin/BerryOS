//! 五刀·生成面 CLI 换轨对拍：Rust bin 生成臂 ↔ Python 现役件
//! （tools/gensourceindex.py）oracle 真跑对拍。对拍强度 = **两件套
//! 字节恒等**（source-index.tlv + source-keys.txt sha256 全等）+
//! **stdout 逐行全等**（剥路径段）+ **拒因/用法错 exit 码与 stderr
//! 全等**（拒 1 / 用法错 2——argparse 判序镜像面）。
//!
//! 夹具 = python mkapp 真门禁产 .bap（lib 面 do_generate 对拍在
//! tests/gen_parity.rs；本件只钉 CLI 装配层：旗标解析/默认值/判序/
//! 出口码）。berry-sign 可得 = 加跑 --sign-key 真签腿（字节恒等——
//! 同 sk 同覆盖域确定性）；缺席 = unsigned 腿。测试钥临时目录生成，
//! 零秘密入库。

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
    out: String,
    err: String,
}

fn run(bin: &Path, args: &[&str]) -> Proc {
    run_env(bin, args, &[])
}

/// 带 env 注入的执行形（真签腿用：berry-sign 发现序首入口 = BERRY_SIGN
/// env 合同位——测试 CWD=crate 目录，Python 相对候选位探不到 repo
/// target-cargo，双侧同传 env 即同发现结果）。
fn run_env(bin: &Path, args: &[&str], envs: &[(&str, &str)]) -> Proc {
    let is_script = bin.extension().map(|e| e == "py").unwrap_or(false);
    let mut c = if is_script {
        let mut c = Command::new("python3");
        c.arg(bin);
        c
    } else {
        Command::new(bin)
    };
    c.args(args).env_remove("CARGO_TARGET_DIR").env_remove("BERRY_SIGN");
    for (k, v) in envs {
        c.env(k, v);
    }
    let o = c.output().expect("spawn");
    Proc {
        rc: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).into_owned(),
        err: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn py_oracle() -> Option<PathBuf> {
    let p = repo_root().join("tools/gensourceindex.py");
    p.exists().then_some(p)
}

fn rust_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gensourceindex"))
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gsi-gencli-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// python mkapp 真门禁产 .bap（[security] 三键在席——索引准入面）。
fn synth_bap(tmp: &Path, id: &str) -> PathBuf {
    let stage = tmp.join(format!("stage-{id}"));
    fs::create_dir_all(stage.join("bin")).unwrap();
    fs::write(
        stage.join("manifest.toml"),
        format!(
            "[app]\nid = \"{id}\"\nversion = \"0.1.0\"\nentry = \"bin/x\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[security]\nprofile = \"standard\"\nkey_lineage = \"berryos\"\nintegrator = \"oem-1\"\n"
        ),
    )
    .unwrap();
    fs::write(stage.join("bin").join("x"), b"\x7fELFgencli".to_vec()).unwrap();
    let out = tmp.join(format!("{id}.bap"));
    let o = Command::new("python3")
        .arg(repo_root().join("tools/mkapp.py"))
        .arg(out.to_str().unwrap())
        .arg(stage.to_str().unwrap())
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("mkapp");
    assert!(
        o.status.success(),
        "mkapp 产夹具失败：{}",
        String::from_utf8_lossy(&o.stderr)
    );
    out
}

fn sha256_hex(p: &Path) -> String {
    let o = Command::new("shasum").arg("-a").arg("256").arg(p).output().expect("shasum");
    String::from_utf8_lossy(&o.stdout)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

/// 剥 stdout 行内绝对路径段（tmp 两侧不同）：token 含 '/' → basename。
fn strip_path_line(l: &str) -> String {
    if !l.starts_with("gensourceindex: ") {
        return l.to_string();
    }
    let mut toks: Vec<String> = l.split_whitespace().map(str::to_string).collect();
    for t in toks.iter_mut().skip(1) {
        if t.contains('/') {
            *t = Path::new(t.as_str())
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| t.clone());
        }
    }
    toks.join(" ")
}

/// berry-sign 可得性探测（发现序同款——env 优先，再候选位；gen_parity
/// 同源）。
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

/// 双侧生成 → 两件套字节恒等 + stdout 逐行全等（剥路径）。真签腿经
/// `berry_sign_env` 注入 BERRY_SIGN（发现序首入口合同位——测试 CWD 探
/// 不到相对候选位，双侧同 env 即同发现结果）。
fn assert_gen_parity(
    tmp: &Path,
    manifest: &Path,
    pk: &Path,
    out_tag: &str,
    extra: &[&str],
    berry_sign_env: Option<&Path>,
) {
    let py = py_oracle().unwrap();
    let bin = rust_bin();
    let py_out = tmp.join(format!("out-py-{out_tag}"));
    let rs_out = tmp.join(format!("out-rs-{out_tag}"));
    let base_args = [
        "--rootfs-manifest",
        manifest.to_str().unwrap(),
        "--user-dir",
        tmp.to_str().unwrap(),
        "--source-pk",
        pk.to_str().unwrap(),
    ];
    let mut pa: Vec<&str> = base_args.to_vec();
    pa.push("--out-dir");
    pa.push(py_out.to_str().unwrap());
    pa.extend_from_slice(extra);
    let mut ra: Vec<&str> = base_args.to_vec();
    ra.push("--out-dir");
    ra.push(rs_out.to_str().unwrap());
    ra.extend_from_slice(extra);
    let envs: Vec<(&str, &str)> = match berry_sign_env {
        Some(p) => vec![("BERRY_SIGN", p.to_str().unwrap())],
        None => vec![],
    };
    let p = run_env(&py, &pa, &envs);
    let r = run_env(&bin, &ra, &envs);
    assert_eq!(p.rc, 0, "Python 生成失败：{}{}", p.out, p.err);
    assert_eq!(r.rc, 0, "Rust 生成失败：{}{}", r.out, r.err);
    for name in ["source-index.tlv", "source-keys.txt"] {
        let a = sha256_hex(&py_out.join(name));
        let b = sha256_hex(&rs_out.join(name));
        assert_eq!(a, b, "{name} 字节漂移：py {a} vs rs {b}");
    }
    let pl: Vec<String> = p.out.lines().map(strip_path_line).collect();
    let rl: Vec<String> = r.out.lines().map(strip_path_line).collect();
    assert_eq!(pl, rl, "stdout 行漂移：\npy: {pl:?}\nrs: {rl:?}");
}

#[test]
fn gen_cli_parity_unsigned_and_flags() {
    let Some(_py) = py_oracle() else {
        eprintln!("gensourceindex: tools/gensourceindex.py 缺席——CLI 生成对拍 skip");
        return;
    };
    let tmp = tmpdir("main");
    let bap = synth_bap(&tmp, "gc.a");
    fs::copy(&bap, tmp.join("ga.bap")).unwrap();
    let manifest = tmp.join("m.toml");
    fs::write(
        &manifest,
        "version = 1\n\n[entries]\n\"app/gc.a.bap\" = \"ga.bap\"\n",
    )
    .unwrap();
    let pk = tmp.join("pk.bin");
    fs::write(&pk, [0xA5u8; 32]).unwrap();

    // 面 1：unsigned 基线（两件套字节恒等 + stdout 全等）。
    assert_gen_parity(&tmp, &manifest, &pk, "base", &[], None);

    // 面 2：--source-id + --index-version 旗标面。
    assert_gen_parity(
        &tmp,
        &manifest,
        &pk,
        "flags",
        &["--source-id", "oem-9", "--index-version", "3"],
        None,
    );

    // 面 3：--prev 单调门（同水位幂等过——双侧 rc 0 同产物）。
    let prev = tmp.join("out-py-base/source-index.tlv");
    assert_gen_parity(&tmp, &manifest, &pk, "prev", &["--prev", prev.to_str().unwrap()], None);

    // 面 4：拒面同构——pk 非 32B（rc 1 同 stderr 文案）。
    let pk31 = tmp.join("pk31.bin");
    fs::write(&pk31, [0xA5u8; 31]).unwrap();
    let py = py_oracle().unwrap();
    let bin = rust_bin();
    let out_py = tmp.join("o-py");
    let out_rs = tmp.join("o-rs");
    let mut pa: Vec<&str> = vec![
        "--rootfs-manifest",
        manifest.to_str().unwrap(),
        "--user-dir",
        tmp.to_str().unwrap(),
        "--source-pk",
        pk31.to_str().unwrap(),
        "--out-dir",
        out_py.to_str().unwrap(),
    ];
    let mut ra = pa.clone();
    let p4 = run(&py, &pa);
    *ra.last_mut().unwrap() = out_rs.to_str().unwrap();
    let r4 = run(&bin, &ra);
    assert_eq!(p4.rc, 1);
    assert_eq!(r4.rc, 1);
    assert_eq!(p4.err.trim(), r4.err.trim(), "pk 长度门 stderr 不等");

    // 面 5：用法错族 exit 2 同构——缺 --out-dir / --sign-key 缺 --source-pk /
    // --verify 混排 / 未知旗标 / --index-version 非整。
    let cases: [(&[&str], &str); 5] = [
        (&["--source-pk", "x"], "缺 out-dir"),
        (&["--out-dir", "x", "--sign-key", "sk"], "sign-key 需 source-pk"),
        (&["--verify", "v.tlv", "--out-dir", "x"], "verify 混排"),
        (&["--out-dir", "x", "--source-pk", "y", "--wat"], "未知旗标"),
        (&["--out-dir", "x", "--source-pk", "y", "--index-version", "abc"], "非整"),
    ];
    for (args, tag) in cases {
        let p = run(&py, args);
        let r = run(&bin, args);
        assert_eq!(p.rc, 2, "{tag}: Python 应用法错 2（got {}）{}", p.rc, p.err);
        assert_eq!(r.rc, 2, "{tag}: Rust 应用法错 2（got {}）{}", r.rc, r.err);
    }

    // 面 6：--preset 让位（Rust 显式指回 python3——exit 2；Python 正常
    // 分派缺席文件拒 rc 1，两侧不同构是让位契约本身，只钉 Rust 侧形）。
    let r6 = run(&bin, &["--preset", "nope.toml", "--out-dir", "x", "--source-pk", "y"]);
    assert_eq!(r6.rc, 2);
    assert!(r6.err.contains("--preset 走 python3"), "{}", r6.err);

    // 面 7：默认 --source-id/--index-version 域（builtin 钉板——旗标面
    // 已过对拍，此处钉默认值真跑）。
    let r7 = run(
        &bin,
        &[
            "--rootfs-manifest",
            manifest.to_str().unwrap(),
            "--user-dir",
            tmp.to_str().unwrap(),
            "--source-pk",
            pk.to_str().unwrap(),
            "--out-dir",
            tmp.join("o-def").to_str().unwrap(),
        ],
    );
    assert_eq!(r7.rc, 0, "{}", r7.err);
    assert!(r7.out.contains("source_id=berryos"), "{}", r7.out);
    assert!(r7.out.contains("index_version=1"), "{}", r7.out);

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn gen_cli_parity_signed_stream() {
    let Some(_py) = py_oracle() else {
        eprintln!("gensourceindex: oracle 缺席——signed 腿 skip");
        return;
    };
    // berry-sign 在席才跑真签腿（确定性：同 sk 同覆盖域 → 同 64B 签名
    // → 双侧两件套字节恒等含签名槽）。
    let Some(bs) = find_host_berry_sign() else {
        eprintln!("gensourceindex: berry-sign 缺席——--sign-key 真流对拍 skip");
        return;
    };
    let tmp = tmpdir("signed");
    let bap = synth_bap(&tmp, "gc.s");
    fs::copy(&bap, tmp.join("gs.bap")).unwrap();
    let manifest = tmp.join("m.toml");
    fs::write(
        &manifest,
        "version = 1\n\n[entries]\n\"app/gc.s.bap\" = \"gs.bap\"\n",
    )
    .unwrap();
    let sk = tmp.join("dev.sk");
    let pk = tmp.join("dev.pk");
    let o = Command::new(&bs)
        .args(["keygen", sk.to_str().unwrap(), pk.to_str().unwrap()])
        .output()
        .expect("keygen");
    assert!(o.status.success(), "keygen 失败：{}", String::from_utf8_lossy(&o.stderr));
    assert_gen_parity(
        &tmp,
        &manifest,
        &pk,
        "signed",
        &["--sign-key", sk.to_str().unwrap()],
        Some(&bs),
    );
    let _ = fs::remove_dir_all(&tmp);
}
