//! W1-Q27 九刀·签名面对拍：Rust CLI 签名面 ↔ Python `mkapp.py` oracle
//! 真跑对拍（build_parity 同款纪律）。对拍强度 = **拒面文案/出口码全等**
//! + 正流信封行剥路径段全等 + `.sig` 字节恒等（berry-sign 确定性签名实
//! 测 118B 稳定——同钥同包恒等）。
//!
//! berry-sign 可得性：发现序三个入口任一在席（BERRY_SIGN env / 候选位）
//! 即跑全流；全部缺席 = 签面子进程面 skip（src/sign.rs 纯面钉板兜底——
//! 发现序/拒因形态不依赖外部件）。keygen 的 .sk/.pk 在测试临时目录内
//! 生成，零秘密入库（tools/testkeys 面不触）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

struct Proc {
    rc: i32,
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

/// Python oracle（BERRY_SIGN 显式注入——发现序 env 分支）。
fn py(args: &[&str], bs: &Path) -> Proc {
    run(
        &repo_root().join("tools/mkapp.py"),
        args,
        &[("BERRY_SIGN".to_string(), bs.to_string_lossy().into_owned())],
    )
}

/// berry-sign 可得性探测（本机构建产物候选序——与 `find_berry_sign`
/// 同序；env 不设让两侧同走候选序发现，或显式注入同一路径）。
fn find_host_berry_sign() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("BERRY_SIGN") {
        if !v.is_empty() && Path::new(&v).is_file() {
            return Some(PathBuf::from(v));
        }
    }
    let root = repo_root();
    for tdir in ["target-cargo", "user/cargo-target"] {
        for prof in ["debug", "release"] {
            let c = root.join(tdir).join("aarch64-apple-darwin").join(prof).join("berry-sign");
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

/// 发现序钉板（无外部依赖）：候选位合成件验证调用序语义——不跑子进程
/// 面，只验 `find_berry_sign_at` 的返回路径（src/sign.rs 单元测已覆盖
/// env/顺序/X_OK 三面；此处补 CLI 级 exit-2 用法错族——纯 CLI 面，零
/// berry-sign 依赖）。
#[test]
fn sign_cli_usage_errors_match_python() {
    let py = PathBuf::from(repo_root().join("tools/mkapp.py"));
    if !py.exists() {
        eprintln!("mkapp: tools/mkapp.py 缺席——CLI 对拍 skip");
        return;
    }
    // 找 Rust bin：cargo test 的 bin 编译位（target 下 deps 不稳，直接
    // 走 CARGO_BIN_EXE_mkapp——integration test 标准入口）。
    let rs = PathBuf::from(env!("CARGO_BIN_EXE_mkapp"));
    // 用法错族：两侧同 rc=2 同 stderr（BERRY_SIGN 清空防发现序旁路）。
    // 例外：--json 混排形 Python 兜底打 __doc__ 全文，Rust bin 用法文案
    // 为单行（七刀既定形态差异，非签名面漂移）——只对拍出口码。
    let cases: &[&[&str]] = &[
        &["--sign-key"],
        &["--verify-key"],
        &["--sign-key", "sk", "--check", "x.bap"],
        &["--verify-key", "pk", "o.bap", "dir"],
        &["--selftest", "--sign-key", "sk"],
        &["--new", "d", "--id", "X", "--sign-key", "sk"],
    ];
    for args in cases {
        let p = run(&py, args, &[]);
        let r = run(&rs, args, &[]);
        assert_eq!(p.rc, 2, "Python {args:?} 应用法错");
        assert_eq!(r.rc, p.rc, "{args:?}: 出口码不等 R={} P={}", r.rc, p.rc);
        assert_eq!(r.err.trim(), p.err.trim(), "{args:?}: stderr 文案不等");
    }
    for args in [&["--json", "--sign-key", "sk", "o.bap", "d"][..]] {
        let p = run(&py, args, &[]);
        let r = run(&rs, args, &[]);
        assert_eq!(p.rc, 2);
        assert_eq!(r.rc, 2, "{args:?}: --json 混排应用法错（双模式同形）");
    }
}

#[test]
fn sign_parity_oracle_full_flow() {
    let root = repo_root();
    let pybin = root.join("tools/mkapp.py");
    if !pybin.exists() {
        eprintln!("mkapp: tools/mkapp.py 缺席——签名面对拍 skip");
        return;
    }
    let bs = match find_host_berry_sign() {
        Some(p) => p,
        None => {
            eprintln!("mkapp: berry-sign 不可得——签名子进程面 skip（纯面钉板兜底：src/sign.rs）");
            return;
        }
    };
    let rs = PathBuf::from(env!("CARGO_BIN_EXE_mkapp"));
    let tmp = std::env::temp_dir().join(format!("mkapp-signparity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let env_pair = |k: &str| (k.to_string(), bs.to_string_lossy().into_owned());

    // keygen（两侧共用同一对钥——签名确定性实测成立）
    let sk = tmp.join("dev.sk");
    let pk = tmp.join("dev.pk");
    let g = run(&bs, &["keygen", &sk.to_string_lossy(), &pk.to_string_lossy()], &[]);
    assert_eq!(g.rc, 0, "keygen 失败：{}", g.err);

    // staging：带 [security] 三键（签名面要求）与不带（berry-sign 拒面）
    let mk_stage = |id: &str, sec: bool| -> PathBuf {
        let d = tmp.join(id);
        fs::create_dir_all(d.join("bin")).unwrap();
        let mut m = format!(
            "[app]\nid = \"{id}\"\nversion = \"0.1.0\"\nentry = \"bin/{id}\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = true\n\n[runtime]\nstack_pages = 8\n"
        );
        if sec {
            m.push_str("\n[security]\nprofile = \"standard\"\nkey_lineage = \"lineage-p\"\nintegrator = \"oem-1\"\n");
        }
        fs::write(d.join("manifest.toml"), m).unwrap();
        let mut elf = b"\x7fELF".to_vec();
        elf.extend(std::iter::repeat(0xA5u8).take(64));
        fs::write(d.join("bin").join(id), elf).unwrap();
        d
    };
    let d_sec = mk_stage("sigsec", true);
    let d_plain = mk_stage("sigplain", false);

    // ── 正流 1：build + --sign-key（[security] 齐备）──
    let out_r = tmp.join("out-r").join("sigsec.bap");
    let out_p = tmp.join("out-p").join("sigsec.bap");
    fs::create_dir_all(out_r.parent().unwrap()).unwrap();
    fs::create_dir_all(out_p.parent().unwrap()).unwrap();
    let envr = [env_pair("BERRY_SIGN")];
    let pr = run(&rs, &["--sign-key", &sk.to_string_lossy(), &out_r.to_string_lossy(), &d_sec.to_string_lossy()], &envr);
    let pp = py(&["--sign-key", &sk.to_string_lossy(), &out_p.to_string_lossy(), &d_sec.to_string_lossy()], &bs);
    assert_eq!(pr.rc, 0, "Rust sign-build 失败：{}", pr.err);
    assert_eq!(pp.rc, 0, "Python sign-build 失败：{}", pp.err);
    // .sig 字节恒等（确定性签名）
    let (sr, sp) = (fs::read(out_r.with_extension("bap.sig")).unwrap(), fs::read(out_p.with_extension("bap.sig")).unwrap());
    assert_eq!(sr, sp, "两侧 .sig 字节不等（berry-sign 同钥同包应恒等）");
    // sign 行（剥路径段）全等
    let line = |t: &str| t.lines().map(str::trim).find(|l| l.starts_with("sign: ")).unwrap().to_string();
    assert_eq!(strip_path(&line(&pr.out)), strip_path(&line(&pp.out)), "sign 行不等");

    // ── 正流 2：--check + --verify-key ──
    let vr = run(&rs, &["--verify-key", &pk.to_string_lossy(), "--check", &out_r.to_string_lossy()], &envr);
    let vp = py(&["--verify-key", &pk.to_string_lossy(), "--check", &out_p.to_string_lossy()], &bs);
    assert_eq!(vr.rc, 0, "Rust verify 失败：{}", vr.err);
    assert_eq!(vp.rc, 0);
    let vline = |t: &str| t.lines().map(str::trim).find(|l| l.starts_with("verify: ")).unwrap().to_string();
    assert_eq!(strip_path(&vline(&vr.out)), strip_path(&vline(&vp.out)));

    // ── 拒面 1：无 [security] 三键（berry-sign sign 拒 → mkapp 透传）──
    let bad_r = tmp.join("bad-r.bap");
    let bad_p = tmp.join("bad-p.bap");
    let br = run(&rs, &["--sign-key", &sk.to_string_lossy(), &bad_r.to_string_lossy(), &d_plain.to_string_lossy()], &envr);
    let bp = py(&["--sign-key", &sk.to_string_lossy(), &bad_p.to_string_lossy(), &d_plain.to_string_lossy()], &bs);
    assert_eq!(br.rc, 1);
    assert_eq!(br.err.trim(), bp.err.trim(), "无 [security] 拒面文案不等");

    // ── 拒面 2：篡改 .bap（berry-sign verify 拒）──
    let mut blob = fs::read(&out_r).unwrap();
    let last = blob.len() - 1;
    blob[last] ^= 0xFF;
    fs::write(tmp.join("tamper.bap"), &blob).unwrap();
    fs::copy(out_r.with_extension("bap.sig"), tmp.join("tamper.bap.sig")).unwrap();
    let tr = run(&rs, &["--verify-key", &pk.to_string_lossy(), "--check", &tmp.join("tamper.bap").to_string_lossy()], &envr);
    let tp = py(&["--verify-key", &pk.to_string_lossy(), "--check", &tmp.join("tamper.bap").to_string_lossy()], &bs);
    assert_eq!(tr.rc, 1);
    assert_eq!(tr.err.trim(), tp.err.trim(), "篡改拒面文案不等");

    // ── 拒面 3：.sig 缺失（防剥离护栏）──
    fs::copy(&out_r, tmp.join("nosig.bap")).unwrap();
    let nr = run(&rs, &["--verify-key", &pk.to_string_lossy(), "--check", &tmp.join("nosig.bap").to_string_lossy()], &envr);
    let np_ = py(&["--verify-key", &pk.to_string_lossy(), "--check", &tmp.join("nosig.bap").to_string_lossy()], &bs);
    assert_eq!(nr.rc, 1);
    assert_eq!(nr.err.trim(), np_.err.trim(), ".sig 缺失拒面文案不等");

    // ── 拒面 4：换钥（无锚见证拒）──
    let sk2 = tmp.join("d2.sk");
    let pk2 = tmp.join("d2.pk");
    run(&bs, &["keygen", &sk2.to_string_lossy(), &pk2.to_string_lossy()], &[]);
    let wr = run(&rs, &["--verify-key", &pk2.to_string_lossy(), "--check", &out_r.to_string_lossy()], &envr);
    let wp = py(&["--verify-key", &pk2.to_string_lossy(), "--check", &out_p.to_string_lossy()], &bs);
    assert_eq!(wr.rc, 1);
    assert_eq!(wr.err.trim(), wp.err.trim(), "换钥拒面文案不等");

    // ── 拒面 5：berry-sign 不可得（发现序空 → tool-missing）──
    // BERRY_SIGN 指不存在件 + 候选位无可执行（仓根候选位合成环境不可
    // 控——改用发现序 env 值缺席 + 不存在路径形；两侧同走候选序兜底）。
    // 纯发现面已由 src/sign.rs 单元测钉板；此处 CLI 面 = 环境可达性
    // skip 守卫（候选位在席时本腿退化为正常拒，不跑）。
    if find_host_berry_sign().is_none() {
        let mr = run(&rs, &["--sign-key", &sk.to_string_lossy(), "o.bap", "d"], &[]);
        let mp = run(&pybin, &["--sign-key", &sk.to_string_lossy(), "o.bap", "d"], &[]);
        assert_eq!(mr.rc, 1);
        assert_eq!(mr.err.trim(), mp.err.trim(), "tool-missing 拒面文案不等");
    }

    let _ = fs::remove_dir_all(&tmp);
}

/// 剥行内绝对路径段（tmp 路径两侧不同）：`sign: /path/x.bap.sig 118B …` /
/// `verify: /path/x.bap OK …` → 路径 token 统一替换为 basename。
fn strip_path(l: &str) -> String {
    let mut toks = l.split_whitespace().map(str::to_string).collect::<Vec<_>>();
    if toks.len() >= 2 {
        let p = &toks[1];
        if p.contains('/') {
            toks[1] = Path::new(p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone());
        }
    }
    toks.join(" ")
}
