//! 三刀第二片·生成面对拍：Rust `do_generate`（rootfs-manifest 路）↔
//! Python 现役件（tools/gensourceindex.py `do_generate`）oracle 真跑
//! 对拍。对拍强度 = **两件套字节恒等**（source-index.tlv + source-keys.txt
//! 双文件 sha256 全等——同输入同字节）+ **stdout 逐行全等**（剥路径段）
//! + **拒面同构**（exit 1 同文案）。
//!
//! 夹具 = python mkapp 真门禁产 .bap（两 app：一带 .sig 见证〔合成
//! 信封——pk 提取面真流〕一无）→ 双侧各产两件套。Python 缺席 = skip
//! （lib 纯面钉板在 src/gen.rs tests 兜底）。测试钥在临时目录，零入库。

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
    // 环境原样透传（BERRY_SIGN 等不删）——两侧同环境是对拍前提：berry-sign
    // 在席则双侧同走真 harden 验，缺席则双侧同打跳过注（None 分支另在
    // lib 测试钉板）。
    let is_script = bin.extension().map(|e| e == "py").unwrap_or(false);
    let mut c = if is_script {
        let mut c = Command::new("python3");
        c.arg(bin);
        c
    } else {
        Command::new(bin)
    };
    c.args(args).env_remove("CARGO_TARGET_DIR");
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
    let d = std::env::temp_dir().join(format!("gsi-genparity-{}-{tag}", std::process::id()));
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
    fs::write(stage.join("bin").join("x"), b"\x7fELFparity".to_vec()).unwrap();
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

/// 合成 .sig 信封（envelope v1 字段表——pk 见证提取面真流；非 berry-sign
/// 产物，覆盖域语义不在对拍面）。
fn synth_sig(path_base: &Path, lineage: &str, pk: &[u8; 32]) {
    let mut sig = Vec::new();
    sig.extend_from_slice(b"BSIG");
    sig.push(1);
    sig.push(0);
    sig.push(0);
    sig.push(lineage.len() as u8);
    sig.extend_from_slice(lineage.as_bytes());
    sig.extend_from_slice(&[0u8; 4]);
    sig.extend_from_slice(pk);
    let mut p = path_base.as_os_str().to_os_string();
    p.push(".sig");
    fs::write(p, sig).unwrap();
}

fn sha256_hex(p: &Path) -> String {
    let o = Command::new("shasum")
        .arg("-a")
        .arg("256")
        .arg(p)
        .output()
        .expect("shasum");
    String::from_utf8_lossy(&o.stdout).split_whitespace().next().unwrap_or("").to_string()
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

#[test]
fn gen_parity_oracle_two_files_and_stdout() {
    let Some(py) = py_oracle() else {
        eprintln!("gensourceindex: tools/gensourceindex.py 缺席——生成面对拍 skip");
        return;
    };
    let tmp = tmpdir("main");

    // 夹具：双 app（PA.BAP 带 .sig 见证，PB 无）+ rootfs-manifest 表。
    // .sig 须伴生在表解析到的最终路径（user-dir/pa.bap）旁——见证提取
    // 腿真进对拍流。berry-sign 在席 = keygen+sign 产真 .sig（harden 验
    // 真过）；缺席 = 合成信封（双侧 harden 跳过注对拍——None|Err 分支
    // 钉板）。
    let bap_a = synth_bap(&tmp, "par.a");
    let bap_b = synth_bap(&tmp, "par.b");
    fs::copy(&bap_a, tmp.join("pa.bap")).unwrap();
    fs::copy(&bap_b, tmp.join("pb.bap")).unwrap();
    let mut expect_witnessed = 1;
    match find_host_berry_sign() {
        Some(bs) => {
            let sk = tmp.join("dev.sk");
            let pkf = tmp.join("dev.pk");
            let o = Command::new(&bs)
                .args(["keygen", sk.to_str().unwrap(), pkf.to_str().unwrap()])
                .output()
                .expect("keygen");
            assert!(o.status.success(), "keygen 失败：{}", String::from_utf8_lossy(&o.stderr));
            let o = Command::new(&bs)
                .args(["sign", sk.to_str().unwrap(), tmp.join("pa.bap").to_str().unwrap()])
                .output()
                .expect("sign");
            assert!(o.status.success(), "sign 失败：{}", String::from_utf8_lossy(&o.stderr));
        }
        None => {
            synth_sig(&tmp.join("pa.bap"), "berryos", &[0xB0u8; 32]);
            expect_witnessed = 1; // 合成信封也带 pk 见证（witnessed 计数面）
        }
    }
    // 根目录另放非 app 条目（筛面——不进索引）。
    fs::write(tmp.join("bin.sh"), b"SH").unwrap();
    let manifest = tmp.join("m.toml");
    fs::write(
        &manifest,
        "version = 1\n\n[entries]\n\"app/par.a.bap\" = \"pa.bap\"\n\"app/par.b.bap\" = \"pb.bap\"\n\"bin/other\" = \"bin.sh\"\n",
    )
    .unwrap();
    let pk = tmp.join("pk.bin");
    fs::write(&pk, [0xA5u8; 32]).unwrap();

    // Python oracle 产两件套。
    let py_out = tmp.join("out-py");
    let p = run(
        &py,
        &[
            "--rootfs-manifest", manifest.to_str().unwrap(),
            "--user-dir", tmp.to_str().unwrap(),
            "--out-dir", py_out.to_str().unwrap(),
            "--source-pk", pk.to_str().unwrap(),
        ],
    );
    assert_eq!(p.rc, 0, "Python oracle 失败：{}{}", p.out, p.err);

    // Rust do_generate 产两件套。
    let rs_out = tmp.join("out-rs");
    let entries = gensourceindex::gen::load_app_entries(
        &fs::read_to_string(&manifest).unwrap(),
        "m.toml",
        &tmp,
    )
    .unwrap();
    let lines = gensourceindex::gen::do_generate(&gensourceindex::gen::GenParams {
        entries,
        source_pk: &pk,
        sign_key: None,
        source_id: "berryos",
        index_version: 1,
        prev: None,
        out_dir: &rs_out,
    })
    .unwrap();

    // 面 1：两件套字节恒等。
    for name in ["source-index.tlv", "source-keys.txt"] {
        let a = sha256_hex(&py_out.join(name));
        let b = sha256_hex(&rs_out.join(name));
        assert_eq!(a, b, "{name} 字节漂移：py {a} vs rs {b}");
    }

    // 面 2：stdout 逐行全等（剥路径段——Python 打印、剥后比对）+ witnessed
    // 计数钉板。
    let py_lines: Vec<String> = p.out.lines().map(strip_path_line).collect();
    let rs_lines: Vec<String> = lines.iter().map(|l| strip_path_line(l)).collect();
    assert_eq!(py_lines, rs_lines, "stdout 行漂移：\npy: {py_lines:?}\nrs: {rs_lines:?}");
    // 汇总行按谓词定位（不钉 [0]）：.sig 在席 + berry-sign 中途不可得
    // （如 BERRY_SIGN 指向的并行构建树被清）时，加固验跳过注先于汇总行
    // 出现——双侧同序（stdout 逐行全等已过），witnessed 断言只看汇总行。
    let summary = rs_lines
        .iter()
        .find(|l| l.contains("entries="))
        .expect("汇总行缺席");
    assert!(
        summary.contains(&format!("witnessed={expect_witnessed}")),
        "witnessed 计数漂移：{summary}"
    );

    // 面 3：--prev 单调门拒面同构（index_version 回降）。
    let prev = py_out.join("source-index.tlv");
    let p2 = run(
        &py,
        &[
            "--rootfs-manifest", manifest.to_str().unwrap(),
            "--user-dir", tmp.to_str().unwrap(),
            "--out-dir", tmp.join("out-py2").to_str().unwrap(),
            "--source-pk", pk.to_str().unwrap(),
            "--index-version", "0",
            "--prev", prev.to_str().unwrap(),
        ],
    );
    assert_eq!(p2.rc, 1);
    assert!(p2.err.contains("回降拒"), "{}", p2.err);
    let entries2 = gensourceindex::gen::load_app_entries(
        &fs::read_to_string(&manifest).unwrap(),
        "m.toml",
        &tmp,
    )
    .unwrap();
    let r2 = gensourceindex::gen::do_generate(&gensourceindex::gen::GenParams {
        entries: entries2,
        source_pk: &pk,
        sign_key: None,
        source_id: "berryos",
        index_version: 0,
        prev: Some(&prev),
        out_dir: &tmp.join("out-rs2"),
    })
    .unwrap_err();
    assert!(r2.to_string().contains("回降拒"), "{}", r2);

    // 面 4：manifest 缺 [security] 三键拒（mkapp 产无 sec 面 → 双侧同拒文案）。
    let stage = tmp.join("stage-nosec");
    fs::create_dir_all(stage.join("bin")).unwrap();
    fs::write(
        stage.join("manifest.toml"),
        "[app]\nid = \"nosec\"\nversion = \"0.1.0\"\nentry = \"bin/x\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n",
    )
    .unwrap();
    fs::write(stage.join("bin").join("x"), b"\x7fELFnosec".to_vec()).unwrap();
    let nosec_bap = tmp.join("nosec.bap");
    let o = Command::new("python3")
        .arg(repo_root().join("tools/mkapp.py"))
        .arg(nosec_bap.to_str().unwrap())
        .arg(stage.to_str().unwrap())
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("mkapp");
    assert!(o.status.success());
    let m2 = tmp.join("m2.toml");
    fs::write(&m2, "version = 1\n\n[entries]\n\"app/nosec.bap\" = \"nosec.bap\"\n").unwrap();
    let p3 = run(
        &py,
        &[
            "--rootfs-manifest", m2.to_str().unwrap(),
            "--user-dir", tmp.to_str().unwrap(),
            "--out-dir", tmp.join("out-py3").to_str().unwrap(),
            "--source-pk", pk.to_str().unwrap(),
        ],
    );
    assert_eq!(p3.rc, 1, "{}{}", p3.out, p3.err);
    assert!(p3.err.contains("无 [security] 三键"), "{}", p3.err);
    let r3 = gensourceindex::gen::do_generate(&gensourceindex::gen::GenParams {
        entries: gensourceindex::gen::load_app_entries(
            &fs::read_to_string(&m2).unwrap(),
            "m2.toml",
            &tmp,
        )
        .unwrap(),
        source_pk: &pk,
        sign_key: None,
        source_id: "berryos",
        index_version: 1,
        prev: None,
        out_dir: &tmp.join("out-rs3"),
    })
    .unwrap_err();
    assert!(r3.to_string().contains("无 [security] 三键"), "{}", r3);

    // 面 5：pk 长度门同构。
    let pk31 = tmp.join("pk31.bin");
    fs::write(&pk31, [0xA5u8; 31]).unwrap();
    let p4 = run(
        &py,
        &[
            "--rootfs-manifest", manifest.to_str().unwrap(),
            "--user-dir", tmp.to_str().unwrap(),
            "--out-dir", tmp.join("out-py4").to_str().unwrap(),
            "--source-pk", pk31.to_str().unwrap(),
        ],
    );
    assert_eq!(p4.rc, 1);
    assert!(p4.err.contains("--source-pk 非 32B"), "{}", p4.err);

    let _ = fs::remove_dir_all(&tmp);
}
