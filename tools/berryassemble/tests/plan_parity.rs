//! berryassemble 六刀·`--dry-run` 计划面 oracle 对拍：Rust `build_plan`
//! ↔ Python 现役件 `build_plan`（tools/berry_assemble.py :890）双侧逐行
//! 全等。语料 = resolve_parity 同款合成现场（合成基座表 + 构建树 stub
//! ——real fs 面，零构建依赖）；Python 缺席/炸 = skip（full_parity 同款
//! 守卫）。
//!
//! 行面口径：两侧同 `(prod, items, n_base, pr, level, user_disp)` 输入——
//! Python 侧走 `load_product` + `resolve_entries` + `measure_kernel_anchor`
//! + `build_plan` 模块级直调（CLI 面 Python 只吃 DEFAULT_BASE 真源，合成
//! 现场够不着——validate_parity「真源基座拒腿」同族约束，故行面对拍走
//! 模块级）。`user_disp` 变腿直喂（relpath CLI 形在 main.rs 单测面钉板）。
//!
//! 腿面：① 基线全席（preset 2 + override 1 + trust 1 + params 2 键 +
//! locale 2 键——行面全分支）② sideload=True 形 + 零 params/locale
//! （开关行不发射钉板）③ 无 override/trust 最小面（n_ov/n_tr = 0 行
//! 省略钉板）④ user_disp `../user` 形 ⑤ params u32 裸数字 vs intent
//! 引号形 ⑥ source_instance 透传行 + mkimage `--source-instance` 注入位
//! ⑦ kernel_measured=True 态（件在 + 锚现算对账过行形）。

use std::path::{Path, PathBuf};
use std::process::Command;

use berryassemble::anchor::measure_kernel_anchor;
use berryassemble::full::check_full;
use berryassemble::plan::build_plan;
use berryassemble::resolve::resolve_entries;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

const BASE_TOML: &str = "version = 1\n\n[entries]\n\"app/A.bap\" = \"userbins/app/A.bap\"\n\"app/B.bap\" = \"userbins/app/B.bap\"\n\"bin/berry-launcher\" = \"cargo-target/release/berry-launcher\"\n\"bin/bsh\" = \"cargo-target/release/bsh\"\n\"selftest/T.bap\" = \"userbins/app/T.bap\"\n";

const KERNEL_SHA_EMPTY: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// gsi 两件套真产（full_parity 先例——`gensourceindex.py --out-dir
/// --source-pk` 直产；缺席/炸 = None 由调用方转 skip）。
fn make_source_pair(dir: &Path) -> Option<()> {
    let out = Command::new("python3")
        .arg(repo_root().join("tools/gensourceindex.py"))
        .arg("--out-dir")
        .arg(dir)
        .arg("--source-pk")
        .current_dir(repo_root())
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(())
}

/// 布置合成现场（resolve_parity 同款布局）。返回 (tmp, pkg_dir)。
fn setup(tag: &str, product: &str) -> (PathBuf, PathBuf) {
    // tag 隔离（两测并行同进程 id 撞车——resolve_parity 首跑实咬先例）
    let tmp = std::env::temp_dir().join(format!(
        "berryassemble-plan-parity-{}-{}",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    let user = tmp.join("user");
    for rel in [
        "userbins/app/A.bap",
        "userbins/app/B.bap",
        "userbins/app/T.bap",
        "cargo-target/release/berry-launcher",
        "cargo-target/release/bsh",
    ] {
        let p = user.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"stub").unwrap();
    }
    std::fs::write(tmp.join("base.toml"), BASE_TOML).unwrap();
    let pkg = tmp.join("pkg");
    std::fs::create_dir_all(pkg.join("oem")).unwrap();
    std::fs::create_dir_all(pkg.join("trust")).unwrap();
    std::fs::create_dir_all(pkg.join("assets")).unwrap();
    std::fs::write(pkg.join("assets/wallpaper.byi"), b"y").unwrap();
    std::fs::write(pkg.join("oem/A.bap"), b"oem").unwrap();
    std::fs::write(pkg.join("trust/anchor.pk"), [0x5au8; 32]).unwrap();
    std::fs::write(pkg.join("product.toml"), product).unwrap();
    (tmp, pkg)
}

/// Python oracle 行面：同现场 build_plan 全行（成功时）或首拒行
/// （`REJ:`/`EXC:` 前缀——文案面由前四刀钉板，此处同构即证）。
fn py_plan(
    pkg_dir: &Path,
    base: &Path,
    user_dir: &str,
    pr: u32,
    level: u64,
    user_disp: &str,
) -> String {
    let code = format!(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\ntry:\n    prod = b.load_product({:?})\n    items, n_base = b.resolve_entries(prod, {:?}, {:?})\n    b.measure_kernel_anchor(prod)\n    for line in b.build_plan(prod, items, n_base, {pr}, {level}, {user_disp:?}):\n        print(line)\nexcept b.Reject as e:\n    print('REJ:' + str(e))\nexcept SystemExit as e:\n    print('REJ:' + str(e))\nexcept Exception as e:\n    print('EXC:' + repr(e))",
        pkg_dir.join("product.toml"),
        base,
        user_dir
    );
    let out = Command::new("python3")
        .arg("-c")
        .arg(&code)
        .current_dir(repo_root())
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("spawn python3");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Rust 侧行面：check_full → resolve_entries → measure_kernel_anchor →
/// build_plan（do_dry_run 组合序全镜像——validate_all 内序同构）。
fn rust_plan(
    pkg_dir: &Path,
    base: &str,
    user_dir: &str,
    pr: u32,
    level: u64,
    user_disp: &str,
) -> Result<String, String> {
    let text = std::fs::read_to_string(pkg_dir.join("product.toml")).unwrap();
    let src_dir = pkg_dir.display().to_string();
    let mut p = match check_full(&text, &src_dir) {
        Ok(p) => p,
        Err(e) => return Err(format!("REJ:{e}")),
    };
    let (items, n_base) = match resolve_entries(&p, &src_dir, base, user_dir) {
        Ok(v) => v,
        Err(e) => return Err(format!("REJ:{e}")),
    };
    if let Err(e) = measure_kernel_anchor(&mut p, &src_dir) {
        return Err(format!("REJ:{e}"));
    }
    Ok(build_plan(&p, &items, n_base, pr, level, user_disp).join("\n") + "\n")
}

/// 腿 ①-⑤ 语料基线（preset 2 + override 1 + trust 1 + params 2 键 +
/// locale 2 键——全分支）。
fn full_product() -> String {
    [
        "schema = 1\n",
        "[product]\n",
        "name = \"p\"\n",
        "vendor = \"o\"\n",
        "board = \"qemu-virt\"\n",
        "[product.apps]\n",
        "preset = [\"app/A.bap\", \"app/B.bap\"]\n",
        "[product.apps.override]\n",
        "A = \"oem/A.bap\"\n",
        "[product.trust]\n",
        "pk_anchor = \"trust/anchor.pk\"\n",
        "[product.branding]\n",
        "wallpaper = \"assets/wallpaper.byi\"\n",
        "[product.defaults]\n",
        "launcher = \"bin/berry-launcher\"\n",
        "[platform]\n",
        "arch = \"x86_64\"\n",
        "feature_level = \"rich\"\n",
        "build_type = \"eng\"\n",
        "sideload = false\n",
        "[platform.boot]\n",
        "kernel = \"K.ELF@sha256:",
    ]
    .concat()
        + KERNEL_SHA_EMPTY
        + "\"\n[platform.boot.params]\nsched_slice_hz = 250\nintent_default = \"interactive\"\n"
        + "[platform.storage]\nimage_size_mib = 96\nrootfs_split_mib = 32\nmin_power_atomicity = \"p0\"\n"
        + "[product.defaults.locale]\ntimezone = \"UTC\"\nlanguages = [\"zh-Hans-CN\", \"en-US\"]\n"
}

#[test]
fn plan_parity_full() {
    let legs: Vec<(&str, String, u32, u64, &str)> = vec![
        // 腿①：全席基线——行面全分支（产品行/rootfs 计数行/条目行×6/
        // override 行/trust 行/mkcpio 行/mkimage 行/platform_revision 行/
        // kernel 锚行/params 行/locale 行/计划完行）
        ("t1", full_product(), 1, 1, "user"),
        // 腿②：sideload=True 形 + 零 params/locale（开关行不发射钉板）
        (
            "t2",
            {
                let mut s = full_product();
                s = s
                    .replacen("sideload = false", "sideload = true", 1)
                    .replacen(
                        "[platform.boot.params]\nsched_slice_hz = 250\nintent_default = \"interactive\"\n",
                        "",
                        1,
                    )
                    .replacen(
                        "[product.defaults.locale]\ntimezone = \"UTC\"\nlanguages = [\"zh-Hans-CN\", \"en-US\"]\n",
                        "",
                        1,
                    );
                s
            },
            1,
            1,
            "user",
        ),
        // 腿③：无 override/trust 最小面（n_ov/n_tr = 0 行省略钉板）
        (
            "t3",
            {
                let mut s = full_product();
                s = s
                    .replacen("[product.apps.override]\nA = \"oem/A.bap\"\n", "", 1)
                    .replacen(
                        "preset = [\"app/A.bap\", \"app/B.bap\"]",
                        "preset = [\"app/B.bap\"]",
                        1,
                    )
                    .replacen("[product.trust]\npk_anchor = \"trust/anchor.pk\"\n", "", 1);
                s
            },
            1,
            1,
            "user",
        ),
        // 腿④：user_disp `../user` 形
        ("t4", full_product(), 1, 1, "../user"),
        // 腿⑤：params u32 裸数字 vs intent 引号形（timer_slack 替换）
        (
            "t5",
            {
                let mut s = full_product();
                s = s.replacen(
                    "intent_default = \"interactive\"\n",
                    "timer_slack = 500000\n",
                    1,
                );
                s
            },
            1,
            1,
            "user",
        ),
    ];
    for (tag, product, pr, level, user_disp) in legs {
        let (tmp, pkg) = setup(tag, &product);
        let base = tmp.join("base.toml");
        let user_dir = tmp.join("user").display().to_string();
        let py = py_plan(&pkg, &base, &user_dir, pr, level, user_disp);
        let r = rust_plan(
            &pkg,
            base.to_str().unwrap(),
            &user_dir,
            pr,
            level,
            user_disp,
        );
        match r {
            Ok(rust_lines) => {
                assert!(
                    !py.starts_with("REJ:") && !py.starts_with("EXC:"),
                    "{tag}: py 侧意外拒 {py}"
                );
                assert_eq!(
                    rust_lines, py,
                    "{tag}: 行面漂移\nrust:\n{rust_lines}\npy:\n{py}"
                );
            }
            Err(e) => {
                assert!(py.starts_with("REJ:"), "{tag}: py 应拒却过 {py}");
                assert_eq!(e.trim(), py.trim(), "{tag}: 拒面漂移\nrust: {e}\npy:   {py}");
            }
        }
        std::fs::remove_dir_all(&tmp).ok();
    }
}

#[test]
fn plan_parity_source_and_kernel() {
    // 腿⑥ source 两件套真产（gsi 不可得 = skip）
    let product_src = full_product().replacen(
        "[product.defaults]\n",
        "[product.source]\ninstance = \"src\"\n[product.defaults]\n",
        1,
    );
    let (tmp, pkg) = setup("src", &product_src);
    let src_dir = pkg.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    if make_source_pair(&src_dir).is_none() {
        std::fs::remove_dir_all(&tmp).ok();
        return; // gsi 缺席 skip
    }
    {
        let base = tmp.join("base.toml");
        let user_dir = tmp.join("user").display().to_string();
        let py = py_plan(&pkg, &base, &user_dir, 1, 1, "user");
        let r = rust_plan(&pkg, base.to_str().unwrap(), &user_dir, 1, 1, "user");
        let rust_lines = r.expect("source 腿应过检");
        assert!(!py.starts_with("REJ:"), "source 腿 py 意外拒 {py}");
        assert_eq!(rust_lines, py, "source 行面漂移\nrust:\n{rust_lines}\npy:\n{py}");
        assert!(rust_lines.contains(" --source-instance src"), "mkimage 注入位缺：{rust_lines}");
    }
    std::fs::remove_dir_all(&tmp).ok();

    // 腿⑦ kernel_measured=True：包内放真件（空文件 sha = KERNEL_SHA_EMPTY）
    let product_k = full_product().replacen(
        "kernel = \"K.ELF@sha256:",
        "kernel = \"KERNEL.ELF@sha256:",
        1,
    );
    let (tmp, pkg) = setup("kern", &product_k);
    std::fs::write(pkg.join("KERNEL.ELF"), b"").unwrap();
    {
        let base = tmp.join("baseline.toml");
        // 基座表名随意（内容已定）
        std::fs::write(&base, BASE_TOML).unwrap();
        let user_dir = tmp.join("user").display().to_string();
        let py = py_plan(&pkg, &base, &user_dir, 1, 1, "user");
        let r = rust_plan(&pkg, base.to_str().unwrap(), &user_dir, 1, 1, "user");
        let rust_lines = r.expect("kernel 腿应过检");
        assert!(!py.starts_with("REJ:"), "kernel 腿 py 意外拒 {py}");
        assert_eq!(rust_lines, py, "kernel 行面漂移\nrust:\n{rust_lines}\npy:\n{py}");
        assert!(
            rust_lines.contains("SHA-256 现算对账过"),
            "kernel_measured 行形缺：{rust_lines}"
        );
    }
    std::fs::remove_dir_all(&tmp).ok();
}
