//! berryassemble 四刀第五片·`--check` 全量臂 CLI 对拍：Rust bin ↔
//! Python 现役件（tools/berry_assemble.py --check）oracle 真跑对拍。
//!
//! 对拍强度 = stdout 逐行全等（剥路径段）+ exit 码同构 + stderr 全等
//! （拒 1 / 用法错 2 / 成功 0——cli_parity 同款纪律）。合成现场 =
//! selftest/resolve_parity 同款（合成基座表 + 构建树 stub），双侧跑
//! **同一文件系统事实**；基座表/user 基准 = Python `--check` 只吃
//! DEFAULT_BASE/DEFAULT_USER_DIR（仓内真源），故对拍腿用「结构拒
//! 前置」（坏 schema/缺文件——不落基座面）+「真源基座拒腿」（基座表
//! 在席、构建树件缺/锚不等——双侧同咬仓内真基座表，拒面文案里路径
//! 段剥除后全等）。Python 缺席 = skip。
//!
//! OK 行正面腿：合成现场无法直喂（Python 侧基座表 = 仓内真源
//! rootfs-manifest.toml，条目 path 相对 user/ 的构建树件 host 上缺席
//! ——真源拒腿即由此咬）；正面 OK 行六段逐字对拍由 lib 面黄金形
//! （validate.rs check_ok_line_law）+ 本件锚三态腿覆盖（kernel=仅形
//! 的 OK 行在拒腿腿 ⑤ 的对照面里以 Rust 单侧钉板）。

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

/// 两腿同参直跑（--check <path>）。
fn both(path: &str) -> (Proc, Option<Proc>) {
    (
        run(&rust_bin(), &["--check", path]),
        py_oracle().map(|o| run(&o, &["--check", path])),
    )
}

fn assert_parity(tag: &str, rust: &Proc, py: &Proc) {
    assert_eq!(rust.rc, py.rc, "{tag}: exit 码漂移 rust={} py={}\n rust.err={}\n py.err={}", rust.rc, py.rc, rust.err, py.err);
    assert_eq!(rust.out, py.out, "{tag}: stdout 漂移\nrust: {}\npy:   {}", rust.out, py.out);
    assert_eq!(rust.err, py.err, "{tag}: stderr 漂移\nrust: {}\npy:   {}", rust.err, py.err);
}

fn good_product() -> String {
    [
        "schema = 1\n",
        "[product]\n",
        "name = \"p\"\n",
        "vendor = \"o\"\n",
        "board = \"qemu-virt\"\n",
        "[product.apps]\npreset = []\n",
        "[product.defaults]\nlauncher = \"bin/berry-launcher\"\n",
        "[platform]\n",
        "arch = \"x86_64\"\n",
        "feature_level = \"rich\"\n",
        "build_type = \"eng\"\n",
        "sideload = false\n",
        "[platform.boot]\n",
        "kernel = \"K.ELF@sha256:",
    ]
    .concat()
        + "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        + "\"\n[platform.storage]\nimage_size_mib = 96\nrootfs_split_mib = 32\nmin_power_atomicity = \"p0\"\n"
}

#[test]
fn validate_parity_check() {
    let Some(py) = py_oracle() else { return };
    let tmp = std::env::temp_dir().join(format!("basm-val-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    // 腿 ①：读失败（OSError 文案镜像 + exit 1）
    {
        let missing = tmp.join("nope.toml").display().to_string();
        let (r, p) = both(&missing);
        assert_parity("文件缺", &r, &p.expect("py"));
    }
    // 腿 ②：坏 TOML（拒径同构 + 分支标识——文案不同源首刀先例）
    {
        fs::write(tmp.join("bad.toml"), "[product\nname = \"p\"\n").unwrap();
        let (r, p) = both(&tmp.join("bad.toml").display().to_string());
        let p = p.expect("py");
        assert_eq!(r.rc, 1);
        assert_eq!(p.rc, 1);
        assert!(r.err.contains("TOML 解析失败"), "rust 分支标识：{}", r.err);
        assert!(p.err.contains("TOML 解析失败"), "py 分支标识：{}", p.err);
    }
    // 腿 ③：结构拒（schema=0 拒面文案逐字全等——避开 kernel 值行的
    // 双侧解析器文案不同源面，单点变异只动 schema 值）
    {
        let bad = good_product().replacen("schema = 1", "schema = 0", 1);
        fs::write(tmp.join("s0.toml"), &bad).unwrap();
        let (r, p) = both(&tmp.join("s0.toml").display().to_string());
        let p = p.expect("py");
        assert_eq!(r.rc, 1);
        assert_eq!(p.rc, 1);
        // Python 拒面前缀带 {path}:（文件面入口），Rust 同构带（三刀
        // 首例面 3 已钉）——剥 tmp 段后逐字全等
        let t = tmp.display().to_string();
        assert_eq!(
            r.err.replace(&t, "<TMP>"),
            p.err.replace(&t, "<TMP>"),
            "schema=0 拒面漂移"
        );
    }
    // 腿 ④：产物面拒（override OEM 件缺——lite 过检后文件面首咬）
    {
        let bad = good_product().replacen(
            "[platform]\n",
            "[product.apps.override]\nA = \"oem/A.bap\"\n[platform]\n",
            1,
        );
        fs::write(tmp.join("ovr.toml"), &bad).unwrap();
        let (r, p) = both(&tmp.join("ovr.toml").display().to_string());
        let p = p.expect("py");
        assert_eq!(r.rc, 1);
        assert_eq!(p.rc, 1);
        // 拒面带临时目录段——剥除后全等
        let t = tmp.display().to_string();
        assert_eq!(
            r.err.replace(&t, "<TMP>"),
            p.err.replace(&t, "<TMP>"),
            "override 拒面漂移"
        );
    }
    // 腿 ⑤：真源基座拒（结构+产物全过后落 resolve_entries——双侧同吃
    // 仓内 rootfs-manifest.toml 真基座表；构建树 bin/* 件 host 缺席 →
    // 「构建树缺席」拒面，路径段含仓根（剥除）+ user/（保留）逐字对拍）
    {
        let pkg = tmp.join("pkg5");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("product.toml"), good_product()).unwrap();
        let (r, p) = both(&pkg.join("product.toml").display().to_string());
        let p = p.expect("py");
        assert_eq!(r.rc, 1, "rust 基座拒：{}", r.err);
        assert_eq!(p.rc, 1, "py 基座拒：{}", p.err);
        let root = repo_root().display().to_string();
        // 双侧拒面剥仓根段后逐字全等（基座表首条非 app 条目同咬）
        let re = r.err.replace(&root, "<REPO>");
        let pe = p.err.replace(&root, "<REPO>");
        assert!(!re.is_empty() && re == pe, "基座拒面漂移\nrust: {re}\npy:   {pe}");
        assert!(re.contains("构建树缺席"), "拒面应为构建树缺席族：{re}");
    }
    // 腿 ⑥：用法错族（--check 缺值尾 exit 2 双侧同构）
    {
        let r = run(&rust_bin(), &["--check"]);
        let p = run(&py, &["--check"]);
        assert_eq!(r.rc, 2);
        assert_eq!(p.rc, 2);
    }
    // 腿 ⑦（六刀）：--dry-run 真源基座拒腿（行面前置的基座面对拍——
    // 双侧同咬仓内真基座表构建树缺席，拒面剥仓根段逐字全等；行面全
    // 分支对拍归 plan_parity.rs 模块级）
    {
        let pkg = tmp.join("pkg7");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("product.toml"), good_product()).unwrap();
        let path = pkg.join("product.toml").display().to_string();
        let r = run(&rust_bin(), &["--dry-run", &path, "--platform-revision", "1"]);
        let p = run(&py, &["--dry-run", &path, "--platform-revision", "1"]);
        assert_eq!(r.rc, 1, "rust dry-run 基座拒：{}", r.err);
        assert_eq!(p.rc, 1, "py dry-run 基座拒：{}", p.err);
        let root = repo_root().display().to_string();
        let re = r.err.replace(&root, "<REPO>");
        let pe = p.err.replace(&root, "<REPO>");
        assert!(!re.is_empty() && re == pe, "dry-run 基座拒面漂移\nrust: {re}\npy:   {pe}");
    }
    // 腿 ⑧（六刀）：--dry-run 装配门两臂（pr 越域拒前置——不落读面；
    // 域内但超前 level 拒——文案逐字全等）
    {
        let r = run(&rust_bin(), &["--dry-run", "nope.toml", "--platform-revision", "4294967296"]);
        let p = run(&py, &["--dry-run", "nope.toml", "--platform-revision", "4294967296"]);
        assert_eq!(r.rc, 1);
        assert_eq!(p.rc, 1);
        assert_eq!(r.err, p.err, "域界拒面漂移");
        assert!(r.err.contains("u32 域"), "域界族拒面：{}", r.err);

        let r = run(&rust_bin(), &["--dry-run", "nope.toml", "--platform-revision", "2"]);
        let p = run(&py, &["--dry-run", "nope.toml", "--platform-revision", "2"]);
        assert_eq!(r.rc, 1);
        assert_eq!(p.rc, 1);
        assert_eq!(r.err, p.err, "反回滚拒面漂移");
        assert!(r.err.contains("超前平台"), "反回滚族拒面：{}", r.err);
    }
    // 腿 ⑨（六刀）：--dry-run 用法错（缺值尾 exit 2）+ 坏 pr 值（int()
    // 拒形——exit 2 文案全等）
    {
        let r = run(&rust_bin(), &["--dry-run"]);
        let p = run(&py, &["--dry-run"]);
        assert_eq!(r.rc, 2);
        assert_eq!(p.rc, 2);

        let r = run(&rust_bin(), &["--dry-run", "x.toml", "--platform-revision", "abc"]);
        let p = run(&py, &["--dry-run", "x.toml", "--platform-revision", "abc"]);
        assert_eq!(r.rc, 2);
        assert_eq!(p.rc, 2);
        assert_eq!(r.err, p.err, "坏 pr 值拒面漂移");
    }

    fs::remove_dir_all(&tmp).ok();
}
