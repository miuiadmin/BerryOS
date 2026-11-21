//! berryassemble 四刀第一片·full 产物面对账 oracle 对拍：Python 现役件
//! `load_product(lite=False)` 文件面四臂（override/branding/source/trust）
//! 双侧同构。语料包在临时目录合成（real fs 面），拒面文案逐字对拍
//! （文案即契约）；Python 缺席/版本面炸 = skip（parity.rs 同款守卫）。
//!
//! 布局（Python 侧同一 pkg 目录复用——双侧同一文件系统事实）：
//!
//! ```text
//! <tmp>/pkg/product.toml     —— 全节在席语料（override+branding+source+trust）
//! <tmp>/pkg/baps/A.bap       —— override OEM 件
//! <tmp>/pkg/assets/wallpaper.byi
//! <tmp>/pkg/src/{source-index.tlv, source-keys.txt}   —— gsi 两件套
//! <tmp>/trust/anchor.pk      —— 32B / 16B 两态
//! ```
//!
//! gsi 两件套 = `gensourceindex.py --out-dir --source-pk` 真产（首跑时
//! subprocess 生成，无构建依赖——unsigned 诚实降级面）；拒腿逐条变异
//! （删件/截短/坏 TLV ver），双侧各自跑同一目录取文案比对。

use berryassemble::full::{check_full, GSI_KEYS_NAME};
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// 语料（结构面合法 + 四节全席——文件面逐臂变异）。
const PRODUCT_TOML: &str = r#"schema = 1
[product]
name = "p"
vendor = "o"
board = "qemu-virt"
[product.apps]
preset = ["app/A.bap"]
[product.apps.override]
A = "baps/A.bap"
[product.branding]
wallpaper = "assets/wallpaper.byi"
[product.source]
instance = "src"
[product.trust]
pk_anchor = "trust/anchor.pk"
[product.defaults]
launcher = "bin/sh"
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
"#;

/// Python oracle：load_product(path, lite=False) → OK | REJ:<msg>。
fn py_full(pkg_dir: &Path) -> (String, bool) {
    let code = format!(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\ntry:\n    b.load_product({:?}, lite=False)\n    print('OK')\nexcept b.Reject as e:\n    print('REJ:' + str(e))\nexcept Exception as e:\n    print('EXC:' + repr(e))",
        pkg_dir.join("product.toml")
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

/// gsi 两件套真产（unsigned 面零构建依赖——pk = 32B 全零占位）。
fn gen_gsi_pair(out_dir: &Path) {
    let pk = out_dir.with_file_name("gsi-pk.bin");
    std::fs::write(&pk, [0u8; 32]).unwrap();
    let o = Command::new("python3")
        .arg("tools/gensourceindex.py")
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--source-pk")
        .arg(&pk)
        .current_dir(repo_root())
        .output()
        .expect("spawn gensourceindex");
    assert!(
        o.status.success(),
        "gsi 两件套生成失败：{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}

/// 布置完整包（全件在席——正例基线）。
fn setup_pkg(tmp: &Path) {
    let pkg = tmp.join("pkg");
    std::fs::create_dir_all(pkg.join("baps")).unwrap();
    std::fs::create_dir_all(pkg.join("assets")).unwrap();
    std::fs::create_dir_all(pkg.join("trust")).unwrap();
    std::fs::write(pkg.join("product.toml"), PRODUCT_TOML).unwrap();
    std::fs::write(pkg.join("baps/A.bap"), b"x").unwrap();
    std::fs::write(pkg.join("assets/wallpaper.byi"), b"y").unwrap();
    std::fs::write(pkg.join("trust/anchor.pk"), [0x5au8; 32]).unwrap();
    let src = pkg.join("src");
    if !src.join(GSI_KEYS_NAME).is_file() {
        gen_gsi_pair(&src);
    }
}

/// 双侧跑同一 pkg：Rust full vs Python lite=False → (rust_res, py_res)。
/// res 形 = "OK" | "REJ:<msg>"。
fn both(pkg: &Path) -> (String, String) {
    let text = std::fs::read_to_string(pkg.join("product.toml")).unwrap();
    let rust = match check_full(&text, &pkg.to_string_lossy()) {
        Ok(_) => "OK".to_string(),
        Err(e) => format!("REJ:{}", e.0),
    };
    let (py, ok) = py_full(pkg);
    assert!(ok, "python oracle 崩：{py}");
    (rust, py)
}

#[test]
fn parity_full_files_four_arms() {
    let tmp = std::env::temp_dir().join(format!("berryassemble-full-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    setup_pkg(&tmp);
    let pkg = tmp.join("pkg");

    // 正例：全件在席——双侧 OK
    let (r, p) = both(&pkg);
    assert_eq!(r, p, "正例双侧不同构");
    assert_eq!(r, "OK", "正例应过：{r}");

    // 腿族：逐臂变异（删件/截短/坏 TLV），每腿后恢复
    let legs: Vec<(&str, Box<dyn Fn(&Path)>, Box<dyn Fn(&Path)>)> = vec![
        // ① override OEM 件缺席
        (
            "override 件缺席",
            Box::new(|pkg: &Path| std::fs::remove_file(pkg.join("baps/A.bap")).unwrap()),
            Box::new(|pkg: &Path| std::fs::write(pkg.join("baps/A.bap"), b"x").unwrap()),
        ),
        // ② branding 资产缺席
        (
            "branding 件缺席",
            Box::new(|pkg: &Path| std::fs::remove_file(pkg.join("assets/wallpaper.byi")).unwrap()),
            Box::new(|pkg: &Path| std::fs::write(pkg.join("assets/wallpaper.byi"), b"y").unwrap()),
        ),
        // ③ source 目录缺席（两件套整目录删）
        (
            "source 目录缺席",
            Box::new(|pkg: &Path| {
                let src = pkg.join("src");
                // 两件套先搬到 tmp 保底，恢复时搬回
                std::fs::rename(&src, src.with_file_name("src.bak")).unwrap();
            }),
            Box::new(|pkg: &Path| {
                let src = pkg.join("src");
                std::fs::rename(src.with_file_name("src.bak"), &src).unwrap();
            }),
        ),
        // ④ source 缺 keys 清单
        (
            "source 缺 keys 件",
            Box::new(|pkg: &Path| {
                std::fs::rename(pkg.join("src").join(GSI_KEYS_NAME), pkg.join("src").join("keys.bak")).unwrap();
            }),
            Box::new(|pkg: &Path| {
                let src = pkg.join("src");
                std::fs::rename(src.join("keys.bak"), src.join(GSI_KEYS_NAME)).unwrap();
            }),
        ),
        // ⑤ source 索引 TLV 坏（ver 字段 9——单解释器拒）
        (
            "source 坏 TLV",
            Box::new(|pkg: &Path| {
                let p = pkg.join("src").join("source-index.tlv");
                let mut d = std::fs::read(&p).unwrap();
                d[4] = 9;
                std::fs::write(&p, d).unwrap();
            }),
            Box::new(|pkg: &Path| {
                let src = pkg.join("src");
                std::fs::remove_file(src.join("source-index.tlv")).unwrap();
                gen_gsi_pair(&src);
            }),
        ),
        // ⑥ trust 锚件缺席
        (
            "trust 件缺席",
            Box::new(|pkg: &Path| std::fs::remove_file(pkg.join("trust/anchor.pk")).unwrap()),
            Box::new(|pkg: &Path| std::fs::write(pkg.join("trust/anchor.pk"), [0x5au8; 32]).unwrap()),
        ),
        // ⑦ trust 16B 短件
        (
            "trust 16B 短",
            Box::new(|pkg: &Path| std::fs::write(pkg.join("trust/anchor.pk"), [0x5au8; 16]).unwrap()),
            Box::new(|pkg: &Path| std::fs::write(pkg.join("trust/anchor.pk"), [0x5au8; 32]).unwrap()),
        ),
    ];

    for (label, mutate, restore) in legs {
        mutate(&pkg);
        let (r, p) = both(&pkg);
        assert!(
            p.starts_with("REJ:"),
            "{label}: python 应拒却得 {p}"
        );
        assert_eq!(
            r, p,
            "{label}: 拒面文案双侧不同构\n rust: {r}\n  py: {p}"
        );
        restore(&pkg);
        // 恢复后复跑双侧 OK（腿间独立）
        let (r, p) = both(&pkg);
        assert_eq!((r.as_str(), p.as_str()), ("OK", "OK"), "{label}: 恢复腿未回到 OK");
    }

    let _ = std::fs::remove_dir_all(&tmp);
}

/// 结构拒面前置（full = lite 先行——坏 TOML 在文件面前先咬）。
#[test]
fn parity_full_struct_reject_first() {
    let tmp = std::env::temp_dir().join(format!("berryassemble-full-parity-s-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    // 坏 TOML（lite 面拒——文件面零触及）
    std::fs::write(tmp.join("product.toml"), "schema = 2\n[product]\nname = \"p\"\n").unwrap();
    let (r, p) = both(&tmp);
    assert!(p.starts_with("REJ:") && r.starts_with("REJ:"), "结构拒面双侧应均拒：{r} / {p}");
    let _ = std::fs::remove_dir_all(&tmp);
}
