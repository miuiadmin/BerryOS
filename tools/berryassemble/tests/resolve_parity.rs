//! berryassemble 四刀第二片·基座表对账 oracle 对拍：Python 现役件
//! `resolve_entries` 双侧同构（终表/基座计数/拒面文案逐字）。语料 =
//! selftest 同款合成基座表 + 构建树 stub（real fs 面，零构建依赖）；
//! Python 缺席/炸 = skip（full_parity.rs 同款守卫）。
//!
//! 布局（双侧同一文件系统事实）：
//!
//! ```text
//! <tmp>/base.toml              —— 合成基座表（SELFTEST_BASE_TOML 同形）
//! <tmp>/user/userbins/app/{A,B,T}.bap、user/cargo-target/release/{berry-launcher,bsh}
//! <tmp>/pkg/product.toml       —— 语料产品（preset/override/trust 变异腿）
//! <tmp>/pkg/oem/A.bap、<tmp>/pkg/trust/anchor.pk（32B）
//! ```
//!
//! 基座 TOML 坏形腿：tomllib 与 Rust 子集解析文案不同源（首刀先例）——
//! 拒径同构 + prefix 对拍（「基座条目表加载失败：」前缀逐字）。

use berryassemble::lite::load_lite_product;
use berryassemble::resolve::resolve_entries;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

const BASE_TOML: &str = "version = 1\n\n[entries]\n\"app/A.bap\" = \"userbins/app/A.bap\"\n\"app/B.bap\" = \"userbins/app/B.bap\"\n\"bin/berry-launcher\" = \"cargo-target/release/berry-launcher\"\n\"bin/bsh\" = \"cargo-target/release/bsh\"\n\"selftest/T.bap\" = \"userbins/app/T.bap\"\n";

/// 产品语料（preset + override + trust 全席基线——变异腿定点替换）。
fn product_toml(preset: &str, override_sec: &str, trust: &str, launcher: &str) -> String {
    format!(
        r#"schema = 1
[product]
name = "p"
vendor = "o"
board = "qemu-virt"
[product.apps]
preset = [{preset}]
{override_sec}[product.trust]
pk_anchor = "{trust}"
[product.branding]
wallpaper = "assets/wallpaper.byi"
[product.defaults]
launcher = "{launcher}"
[platform]
arch = "x86_64"
feature_level = "rich"
build_type = "eng"
sideload = false
[platform.boot]
kernel = "K.ELF@sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
[platform.storage]
image_size_mib = 96
rootfs_split_mib = 32
min_power_atomicity = "p0"
"#
    )
}

/// 布置合成现场（基线态）。返回 (tmp, pkg_dir)。
fn setup(tag: &str, mutations: &[(&str, &str)]) -> (PathBuf, PathBuf) {
    // tag 隔离（两测并行同进程 id 撞车——首跑实咬）
    let tmp = std::env::temp_dir().join(format!(
        "berryassemble-resolve-parity-{}-{}",
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
    let mut text = product_toml(
        "\"app/A.bap\"",
        "[product.apps.override]\nA = \"oem/A.bap\"\n",
        "trust/anchor.pk",
        "bin/berry-launcher",
    );
    for (from, to) in mutations {
        text = text.replace(from, to);
    }
    std::fs::write(pkg.join("product.toml"), text).unwrap();
    (tmp, pkg)
}

/// Python oracle：load_product + resolve_entries → ITEMS:<n>|<n_base> |
/// REJ:<msg> | EXC:...。
fn py_resolve(pkg_dir: &Path, base: &Path, user_dir: &Path) -> (String, bool) {
    let code = format!(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\ntry:\n    prod = b.load_product({:?})\n    items, n_base = b.resolve_entries(prod, {:?}, {:?})\n    print('ITEMS:%d|%d' % (len(items), n_base))\nexcept b.Reject as e:\n    print('REJ:' + str(e))\nexcept SystemExit as e:\n    print('REJ:' + str(e))\nexcept Exception as e:\n    print('EXC:' + repr(e))",
        pkg_dir.join("product.toml"),
        base,
        user_dir
    );
    let o = Command::new("python3")
        .arg("-c")
        .arg(&code)
        .current_dir(repo_root())
        .output()
        .expect("spawn python3");
    (
        String::from_utf8_lossy(&o.stdout).trim().to_string(),
        o.status.success(),
    )
}

/// 双侧跑同一现场。res 形 = ITEMS:<n>|<n_base> | REJ:<msg>。
fn both(tmp: &Path, pkg: &Path) -> (String, String) {
    let text = std::fs::read_to_string(pkg.join("product.toml")).unwrap();
    let rust = match load_lite_product(&text) {
        Ok(p) => {
            let base = tmp.join("base.toml");
            let user = tmp.join("user");
            match resolve_entries(&p, &pkg.to_string_lossy(), &base.to_string_lossy(), &user.to_string_lossy()) {
                Ok((items, n_base)) => format!("ITEMS:{}|{}", items.len(), n_base),
                Err(e) => format!("REJ:{}", e.0),
            }
        }
        Err(e) => format!("REJ:{}", e.0),
    };
    let (py, ok) = py_resolve(pkg, &tmp.join("base.toml"), &tmp.join("user"));
    assert!(ok, "python oracle 崩：{py}");
    (rust, py)
}

#[test]
fn parity_resolve_base_table() {
    // 正例：preset+override+trust 全席——双侧终表同构（计数 + 整条对拍）
    let (tmp, pkg) = setup("base", &[]);
    let (r, p) = both(&tmp, &pkg);
    assert_eq!(r, p, "正例双侧不同构：{r} vs {p}");
    assert!(r.starts_with("ITEMS:5|3"), "正例应过：{r}");
    // 终表内容逐条对拍（rust 侧再取一次明细）
    let text = std::fs::read_to_string(pkg.join("product.toml")).unwrap();
    let prod = load_lite_product(&text).unwrap();
    let (items, n_base) = resolve_entries(
        &prod,
        &pkg.to_string_lossy(),
        &tmp.join("base.toml").to_string_lossy(),
        &tmp.join("user").to_string_lossy(),
    )
    .unwrap();
    assert_eq!(n_base, 3);
    assert_eq!(
        items,
        vec![
            format!("app/A.bap={}/oem/A.bap", pkg.to_string_lossy()), // G1 override（包内绝对）
            "bin/berry-launcher=cargo-target/release/berry-launcher".to_string(),
            "bin/bsh=cargo-target/release/bsh".to_string(),
            format!("etc/trust-pk={}/trust/anchor.pk", pkg.to_string_lossy()), // G2 重锚
            "selftest/T.bap=userbins/app/T.bap".to_string(),
        ]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn parity_resolve_reject_legs() {
    // 腿①：preset 不在基座表（C 不在——纯新增拒）
    let (tmp, pkg) = setup("leg1", &[
        ("preset = [\"app/A.bap\"]", "preset = [\"app/C.bap\"]"),
        ("A = \"oem/A.bap\"", "C = \"oem/A.bap\""),
    ]);
    let (r, p) = both(&tmp, &pkg);
    assert!(p.starts_with("REJ:"), "腿① python 应拒：{p}");
    assert_eq!(r, p, "腿① 文案双侧不同构\n rust: {r}\n  py: {p}");
    let _ = std::fs::remove_dir_all(&tmp);

    // 腿②：基座件构建树缺席（selftest/T.bap 腾走——非 app 基座条目恒在终表）
    let (tmp, pkg) = setup("leg2", &[]);
    std::fs::remove_file(tmp.join("user/userbins/app/T.bap")).unwrap();
    let (r, p) = both(&tmp, &pkg);
    assert!(p.starts_with("REJ:"), "腿② python 应拒：{p}");
    assert_eq!(r, p, "腿② 文案双侧不同构\n rust: {r}\n  py: {p}");
    let _ = std::fs::remove_dir_all(&tmp);

    // 腿③：launcher 不在终表（bin/nolauncher 未注册）
    let (tmp, pkg) = setup("leg3", &[("launcher = \"bin/berry-launcher\"", "launcher = \"bin/nolauncher\"")]);
    let (r, p) = both(&tmp, &pkg);
    assert!(p.starts_with("REJ:"), "腿③ python 应拒：{p}");
    assert_eq!(r, p, "腿③ 文案双侧不同构\n rust: {r}\n  py: {p}");
    let _ = std::fs::remove_dir_all(&tmp);

    // 腿④：基座表读失败（文件缺席）
    let (tmp, pkg) = setup("leg4", &[]);
    std::fs::remove_file(tmp.join("base.toml")).unwrap();
    let (r, p) = both(&tmp, &pkg);
    assert!(p.starts_with("REJ:"), "腿④ python 应拒：{p}");
    assert_eq!(r, p, "腿④ 文案双侧不同构\n rust: {r}\n  py: {p}");
    let _ = std::fs::remove_dir_all(&tmp);

    // 腿⑤：基座 TOML 坏形（version=2——解析层拒；tomllib 与 Rust 子集
    // 文案不同源：拒径同构 + prefix 对拍）
    let (tmp, pkg) = setup("leg5", &[]);
    std::fs::write(tmp.join("base.toml"), "version = 2\n\n[entries]\n\"a\" = \"b\"\n").unwrap();
    let (r, p) = both(&tmp, &pkg);
    assert!(p.starts_with("REJ:基座条目表加载失败"), "腿⑤ python 应拒：{p}");
    assert!(r.starts_with("REJ:基座条目表加载失败"), "腿⑤ rust 拒径同构：{r}");
    let _ = std::fs::remove_dir_all(&tmp);

    // 腿⑥：launcher 指向 app/ 条目形（Q13 另一臂——正例腿）
    let (tmp, pkg) = setup("leg6", &[
        ("preset = [\"app/A.bap\"]", "preset = [\"app/A.bap\", \"app/B.bap\"]"),
        ("launcher = \"bin/berry-launcher\"", "launcher = \"app/B.bap\""),
    ]);
    let (r, p) = both(&tmp, &pkg);
    assert!(r.starts_with("ITEMS:6|3"), "腿⑥ 应过：{r}");
    assert_eq!(r, p, "腿⑥ 双侧不同构：{r} vs {p}");
    let _ = std::fs::remove_dir_all(&tmp);
}
