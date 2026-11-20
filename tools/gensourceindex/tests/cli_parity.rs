//! 三刀·CLI 面对拍：Rust bin（--verify/--gen-keys）↔ Python 现役件
//! （tools/gensourceindex.py）oracle 真跑对拍。对拍强度 = **stdout 逐行
//! 全等**（剥路径段）+ 拒因 exit 码同构（拒 1 / 用法错 2）。生成面
//! （--out-dir）仍 Python 现役——两侧共享同一 Python 产出的真索引夹具，
//! 只对拍消费半边。
//!
//! berry-sign 可得性同 sign_parity：发现序任一入口在席即跑 --gen-keys
//! 真流腿；缺席 = 该腿 skip（纯面钉板兜底在 sign.rs tests）。测试钥在
//! 临时目录生成，零秘密入库。

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

/// 合成两件套夹具（Python 现役件产——两侧同源输入）：单条目 unsigned。
fn synth_pair(tmp: &Path) -> PathBuf {
    let outdir = tmp.join("src");
    let manifest = tmp.join("m.toml");
    fs::write(
        &manifest,
        "version = 1\n\n[entries]\n\"app/cli.duo.bap\" = \"cli.duo.bap\"\n",
    )
    .unwrap();
    let py = py_oracle().unwrap();
    // .bap 夹具 = python mkapp 真门禁产（工具面 oracle 同款——含
    // [security] 三键，无 .sig 伴生 = pk 无见证腿）。
    let stage = tmp.join("stage");
    fs::create_dir_all(stage.join("bin")).unwrap();
    fs::write(
        stage.join("manifest.toml"),
        "[app]\nid = \"cli.duo\"\nversion = \"0.1.0\"\nentry = \"bin/duo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[security]\nprofile = \"standard\"\nkey_lineage = \"berryos\"\nintegrator = \"oem-1\"\n",
    )
    .unwrap();
    fs::write(stage.join("bin").join("duo"), b"\x7fELFcli".to_vec()).unwrap();
    let o = Command::new("python3")
        .arg(repo_root().join("tools/mkapp.py"))
        .arg(tmp.join("cli.duo.bap").to_str().unwrap())
        .arg(stage.to_str().unwrap())
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("mkapp");
    assert!(o.status.success(), "mkapp 产夹具失败：{}", String::from_utf8_lossy(&o.stderr));
    let pk = tmp.join("pk.bin");
    fs::write(&pk, [0xA5u8; 32]).unwrap();
    let p = run(
        &py,
        &[
            "--rootfs-manifest",
            manifest.to_str().unwrap(),
            "--user-dir",
            tmp.to_str().unwrap(),
            "--out-dir",
            outdir.to_str().unwrap(),
            "--source-pk",
            pk.to_str().unwrap(),
        ],
    );
    assert_eq!(p.rc, 0, "Python 产夹具失败：{}{}", p.out, p.err);
    outdir
}

#[test]
fn cli_verify_parity_oracle() {
    let Some(_py) = py_oracle() else {
        eprintln!("gensourceindex: tools/gensourceindex.py 缺席——CLI 对拍 skip");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("gsi-cli-{}-v", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    let outdir = synth_pair(&tmp);
    let tlv = outdir.join("source-index.tlv");
    let py = py_oracle().unwrap();
    let bin = rust_bin();

    // 面 1：基础 --verify（两件套在席 → 对拍行也在）。
    let pr = run(&py, &["--verify", tlv.to_str().unwrap()]);
    let rr = run(&bin, &["--verify", tlv.to_str().unwrap()]);
    assert_eq!(pr.rc, 0, "Python --verify 失败：{}", pr.err);
    assert_eq!(rr.rc, 0, "Rust --verify 失败：{}", rr.err);
    let norm = |t: &str| -> String {
        t.trim()
            .replace(outdir.to_str().unwrap(), "<OUT>")
            .replace(repo_root().to_str().unwrap(), "<R>")
    };
    let pn = norm(&pr.out);
    let rn = norm(&rr.out);
    let pl: Vec<&str> = pn.lines().collect();
    let rl: Vec<&str> = rn.lines().collect();
    // 逐行对拍（行序 = Python 输出序——Rust 按同序 println）。
    assert_eq!(pl, rl, "--verify stdout 不等：\nPY:\n{pl:#?}\nRS:\n{rl:#?}");

    // 面 2：--prev 单调门（同代等值 = 幂等过）。
    let pr2 = run(&py, &["--verify", tlv.to_str().unwrap(), "--prev", tlv.to_str().unwrap()]);
    let rr2 = run(&bin, &["--verify", tlv.to_str().unwrap(), "--prev", tlv.to_str().unwrap()]);
    assert_eq!(pr2.rc, 0);
    assert_eq!(rr2.rc, 0);
    assert_eq!(norm(&pr2.out), norm(&rr2.out));

    // 面 3：回降拒（伪造高水位 prev → 双侧同拒同文案）。
    let hi = outdir.join("hi.tlv");
    {
        let raw = fs::read(&tlv).unwrap();
        let mut forged = raw.clone();
        // index_ver 偏移 8..12：1 → 9（伪造高水位）。
        forged[8..12].copy_from_slice(&9u32.to_be_bytes());
        // 摘要/签名面不动（unsigned 索引零签名槽——parse 无签名耦合）。
        fs::write(&hi, forged).unwrap();
    }
    let pr3 = run(&py, &["--verify", tlv.to_str().unwrap(), "--prev", hi.to_str().unwrap()]);
    let rr3 = run(&bin, &["--verify", tlv.to_str().unwrap(), "--prev", hi.to_str().unwrap()]);
    assert_eq!(pr3.rc, 1, "Python 回降应拒：{}", pr3.out);
    assert_eq!(rr3.rc, 1, "Rust 回降应拒：{}", rr3.out);
    assert_eq!(pr3.err.trim(), rr3.err.trim(), "回降拒文案不等");

    // 面 4：两件套对拍拒（清单 pk 篡改 → 双侧同拒）。
    let kpath = outdir.join("source-keys.txt");
    let orig = fs::read_to_string(&kpath).unwrap();
    let tampered = orig.replace("pk=a5a5", "pk=5a5a");
    fs::write(&kpath, &tampered).unwrap();
    let pr4 = run(&py, &["--verify", tlv.to_str().unwrap()]);
    let rr4 = run(&bin, &["--verify", tlv.to_str().unwrap()]);
    assert_eq!(pr4.rc, 1);
    assert_eq!(rr4.rc, 1);
    assert_eq!(pr4.err.trim(), rr4.err.trim(), "两件套对拍拒文案不等");
    fs::write(&kpath, &orig).unwrap();

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn cli_usage_parity() {
    let bin = rust_bin();
    let py = match py_oracle() {
        Some(p) => p,
        None => {
            eprintln!("gensourceindex: oracle 缺席——skip");
            return;
        }
    };
    // 用法错族：零参 / 生成面形态（CLI 未装）→ 双侧 exit 2。
    let cases: [&[&str]; 3] = [&[], &["--out-dir", "x"], &["nonsense"]];
    for args in cases {
        let pr = run(&py, args);
        let rr = run(&bin, args);
        assert_eq!(pr.rc, 2, "Python 应用法错 {args:?}：{}", pr.err);
        assert_eq!(rr.rc, 2, "Rust 应用法错 {args:?}：{}", rr.err);
    }
}

#[test]
fn cli_gen_keys_parity_oracle() {
    let Some(py) = py_oracle() else {
        eprintln!("gensourceindex: oracle 缺席——skip");
        return;
    };
    // berry-sign 发现序（sign_parity 同款探测；env 已被 run() 剥——
    // 探测候选位）。
    let root = repo_root();
    let mut bs: Option<PathBuf> = None;
    for tdir in ["target-cargo", "/tmp/target-p1", "/tmp/target-p2"] {
        for prof in ["debug", "release"] {
            let c = if Path::new(tdir).is_absolute() {
                PathBuf::from(tdir)
            } else {
                root.join(tdir)
            }
            .join("aarch64-apple-darwin")
            .join(prof)
            .join("berry-sign");
            if c.is_file() {
                bs = Some(c);
                break;
            }
        }
        if bs.is_some() { break; }
    }
    let Some(bs) = bs else {
        eprintln!("gensourceindex: berry-sign 缺席——--gen-keys 真流对拍 skip（纯面钉板在 sign.rs tests）");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("gsi-cli-{}-g", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let bin = rust_bin();
    // 双侧独立目录 keygen（keygen 随机 → 只对拍 rc/stdout 行文本 + 产物
    // 存在性与长度，不对拍钥字节）。
    let (skr, pkr) = (tmp.join("r.sk"), tmp.join("r.pk"));
    let (skp, pkp) = (tmp.join("p.sk"), tmp.join("p.pk"));
    let rr = run(&bin, &["--gen-keys", skr.to_str().unwrap(), pkr.to_str().unwrap()]);
    let pr = run(&py, &["--gen-keys", skp.to_str().unwrap(), pkp.to_str().unwrap()]);
    assert_eq!(pr.rc, 0, "Python --gen-keys 失败：{}", pr.err);
    assert_eq!(rr.rc, 0, "Rust --gen-keys 失败：{}", rr.err);
    // stdout 全等（berry-sign 版本行 + 注行——零路径面）。
    assert_eq!(pr.out.trim(), rr.out.trim(), "--gen-keys stdout 不等");
    for (f, n) in [(&skr, 64usize), (&pkr, 32)] {
        let b = fs::read(f).unwrap_or_default();
        assert_eq!(b.len(), n, "{} 长度 {n}B 预期（got {}B）", f.display(), b.len());
    }
    // 用法错：单参 → 双侧 exit 2。
    let r2 = run(&bin, &["--gen-keys", "only-one"]);
    let p2 = run(&py, &["--gen-keys", "only-one"]);
    assert_eq!(r2.rc, 2);
    assert_eq!(p2.rc, 2);
    let _ = fs::remove_dir_all(&tmp);
    let _ = &bs;
}
