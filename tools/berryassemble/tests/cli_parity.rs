//! 三刀·CLI 消费半边对拍：Rust bin（--check-lite）↔ Python 现役件
//! （tools/berry_assemble.py --check-lite）oracle 真跑对拍。对拍强度 =
//! **stdout 逐行全等（剥路径段）+ exit 码同构 + stderr 全等**（拒 1 /
//! 用法错 2——gensourceindex cli_parity 同款纪律）。非 --check-lite 臂
//! （--check/--dry-run/--emit/--selftest/--sample）= Rust 让位指引（exit
//! 2），只拍拒因文案与码，不拍 Python 执行面（现役单源不镜像）。
//!
//! 语料 = good_platform_min 单点变异族（lite 面全谱）+ 真实文件面
//! （不存在路径/目录/坏 UTF-8）。Python 缺席 = skip。

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
    c.args(args).env_remove("CARGO_TARGET_DIR").current_dir(repo_root());
    let o = c.output().expect("spawn");
    Proc {
        rc: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).into_owned(),
        err: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn py_oracle() -> Option<PathBuf> {
    let p = repo_root().join("tools/berry_assemble.py");
    p.exists().then_some(p)
}

fn rust_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_berryassemble"))
}

/// 临时文件面：写 product.toml 到临时目录跑双侧（路径段剥除后全等）。
fn run_both(tmp: &Path, text: &str, extra: &[&str]) -> (Proc, Option<Proc>) {
    let p = tmp.join("product.toml");
    fs::write(&p, text).unwrap();
    let ps = p.display().to_string();
    let mut rargs: Vec<&str> = vec!["--check-lite", &ps];
    rargs.extend_from_slice(extra);
    let rust = run(&rust_bin(), &rargs);
    let py = py_oracle().map(|o| {
        let mut pargs: Vec<&str> = vec!["--check-lite", &ps];
        pargs.extend_from_slice(extra);
        run(&o, &pargs)
    });
    (rust, py)
}

/// stdout 剥路径段（临时目录前缀 → <TMP>——两侧同基不等长）。
fn strip_tmp(s: &str, tmp: &Path) -> String {
    let t = tmp.display().to_string();
    s.replace(&t, "<TMP>")
}

/// 主对拍断言：stdout（剥路径段）逐行全等 + rc 全等 + stderr 全等。
fn assert_parity(tag: &str, rust: &Proc, py: &Proc, tmp: &Path) {
    assert_eq!(rust.rc, py.rc, "{tag}: exit 码漂移 rust={} py={}\n rust.err={}\n py.err={}", rust.rc, py.rc, rust.err, py.err);
    assert_eq!(
        strip_tmp(&rust.out, tmp),
        strip_tmp(&py.out, tmp),
        "{tag}: stdout 漂移\nrust: {}\npy:   {}",
        rust.out,
        py.out
    );
    // stderr 拒行带路径面（读失败）剥段；拒面文案本体不带路径。
    assert_eq!(
        strip_tmp(&rust.err, tmp).replace('\\', "/"),
        strip_tmp(&py.err, tmp).replace('\\', "/"),
        "{tag}: stderr 漂移\nrust: {}\npy:   {}",
        rust.err,
        py.err
    );
}

/// 第二片起的最小过面清单（parity.rs good_platform_min 同源复制——
/// CLI 件独立编译面，不跨 test 目标 import）。
fn good_platform_min() -> String {
    [
        "schema = 1\n",
        "[product]\n",
        "name = \"p\"\n",
        "vendor = \"o\"\n",
        "board = \"qemu-virt\"\n",
        "[product.apps]\npreset = []\n",
        "[product.defaults]\nlauncher = \"bin/sh\"\n",
        "[platform]\n",
        "arch = \"x86_64\"\n",
        "feature_level = \"rich\"\n",
        "build_type = \"eng\"\n",
        "sideload = false\n",
        "[platform.boot]\n",
        "kernel = \"krnl@sha256:",
    ]
    .concat()
        + &"a".repeat(64)
        + "\"\n[platform.storage]\nimage_size_mib = 96\nrootfs_split_mib = 32\nmin_power_atomicity = \"p0\"\n"
}

#[test]
fn cli_parity_check_lite() {
    let Some(py) = py_oracle() else { return };
    let tmp = std::env::temp_dir().join(format!("basm-cli-{}", std::process::id()));
    fs::create_dir_all(&tmp).unwrap();

    // 面 1：正面过检（OK 行全字段对齐——preset 计数/locale=off/trust=off）
    let (r, p) = run_both(&tmp, &good_platform_min(), &[]);
    assert_parity("正面 min", &r, &p.expect("py"), &tmp);

    // 面 2：正面全席（locale 八键 on + trust on + override/priv_grants 计数）
    let full = good_platform_min().replacen(
        "[platform]\n",
        "[product.apps.override]\nA = \"oem/A.bap\"\n[product.trust]\npk_anchor = \"etc/trust-pk\"\n[product.defaults.locale]\nlanguages = [\"zh-Hans-CN\"]\ntimezone = \"UTC\"\n[platform]\n",
        1,
    ).replacen("preset = []", "preset = [\"app/A.bap\"]", 1);
    let (r, p) = run_both(&tmp, &full, &[]);
    assert_parity("正面全席", &r, &p.expect("py"), &tmp);

    // 面 3：结构拒面族（文案即契约——CLI 面拒行前缀 berry-assemble: 同构）
    for (name, text) in [
        // 坏 TOML 文案不同源（tomllib 自带英文 vs 本子集解析器中文——
        // 首刀先例）：只拍双侧拒 + exit 1，文案统一归后续刀。
        ("坏 TOML（分支标识对齐）", "[product\nname = \"p\"\n"),
        ("schema 非 1", "schema = 2\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("board 词表外", &good_platform_min().replacen("board = \"qemu-virt\"", "board = \"rpi4\"", 1)),
        ("preset 重复", &good_platform_min().replacen("preset = []", "preset = [\"app/A.bap\", \"app/A.bap\"]", 1)),
        ("locale 未知键", &good_platform_min().replacen("[platform]\n", "[product.defaults.locale]\nextra = 1\n[platform]\n", 1)),
    ] {
        let (r, p) = run_both(&tmp, text, &[]);
        let p = p.expect("py");
        if name == "坏 TOML（分支标识对齐）" {
            assert_eq!(r.rc, 1);
            assert_eq!(p.rc, 1);
            assert!(r.err.contains("TOML 解析失败"), "rust 分支标识漂移：{}", r.err);
            assert!(p.err.contains("TOML 解析失败"), "py 分支标识漂移：{}", p.err);
        } else {
            assert_parity(name, &r, &p, &tmp);
        }
    }

    // 面 4：文件面拒（读失败——OSError 文案镜像 + 路径段剥除）
    {
        let missing = tmp.join("nope.toml").display().to_string();
        let r = run(&rust_bin(), &["--check-lite", &missing]);
        let p = run(&py, &["--check-lite", &missing]);
        assert_parity("文件缺", &r, &p, &tmp);
        let dir = tmp.display().to_string();
        let r = run(&rust_bin(), &["--check-lite", &dir]);
        let p = run(&py, &["--check-lite", &dir]);
        assert_parity("目录入参", &r, &p, &tmp);
    }

    // 面 5：用法错族（exit 2 + stderr 全等）
    for (name, args) in [
        ("无参", vec![] as Vec<&str>),
        ("互斥双模式", vec!["--check-lite", "a", "--check", "b"]),
        ("未知旗标", vec!["--wat"]),
        ("check 缺值尾", vec!["--check-lite"]),
        ("pr 缺值尾", vec!["--platform-revision"]),
        ("pr 坏值", vec!["--platform-revision", "x", "--check-lite", "a"]),
        ("emit 缺 out-dir", vec!["--emit", "a"]),
        ("out-dir 单席", vec!["--out-dir", "o"]),
    ] {
        let r = run(&rust_bin(), &args);
        let p = run(&py, &args);
        // Python 无参/未知旗标落 __doc__（6.7KB 全文）vs Rust 一行让位
        // USAGE——用法面文案不同源（gensourceindex --preset 先例），只拍
        // exit 码；带具体文案的拒因面（互斥/pr 坏值/emit 缺值/out-dir
        // 单席）拍 stderr 全等。
        assert_eq!(r.rc, 2, "{name}: rust 用法错 2");
        assert_eq!(p.rc, 2, "{name}: py 用法错 2");
        match name {
            "无参" | "未知旗标" | "check 缺值尾" => {}
            _ => {
                assert_eq!(
                    r.err, p.err,
                    "{name}: stderr 漂移\nrust: {}\npy:   {}",
                    r.err, p.err
                );
            }
        }
    }

    // 面 6：让位臂（--check/--dry-run/--emit+out-dir/--selftest/--sample
    // = Rust exit 2 让位指引；Python 正常执行——单源不镜像只拍码面）
    for args in [
        vec!["--check", "x.toml"],
        vec!["--dry-run", "x.toml"],
        vec!["--emit", "x.toml", "--out-dir", "o"],
        vec!["--selftest"],
        vec!["--sample"],
    ] {
        let r = run(&rust_bin(), &args);
        assert_eq!(r.rc, 2, "让位臂应 exit 2：{args:?} → {}", r.err);
        assert!(r.err.contains("走 python3"), "让位文案缺指引：{}", r.err);
    }

    fs::remove_dir_all(&tmp).ok();
}
