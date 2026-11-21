//! berryassemble 四刀第三片·kernel 锚现算对账 oracle 对拍：Python 现役件
//! `measure_kernel_anchor` 三态（件缺=false / 件在且等=true / 件在不等=
//! loud 拒）双侧同构。语料包在临时目录合成（real fs 面），拒面文案逐字
//! 对拍（文案即契约）；Python 缺席/炸 = skip（full_parity.rs 同款守卫）。
//!
//! 语料节选自 Python selftest G7 段（:1761-1799）：空件 = sha256("") =
//! e3b0c442…（模板锚即空串摘要）；不等件 = `not-the-declared-kernel`
//! （selftest 同字节）；另补 5MiB 大件分块读腿（Python 1MiB chunk 循环
//! 的 Rust `fs::read` 单读等价面——确定性摘要双侧恒等即证）。

use berryassemble::anchor::measure_kernel_anchor;
use berryassemble::lite::load_lite_product;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// 结构面合法语料（lite 过检即可——文件面/度量面归本片独立触达；
/// G7 段同款：无 branding/override/source/trust 节 = 零文件对账面）。
const PRODUCT_TOML: &str = r#"schema = 1
[product]
name = "p"
vendor = "o"
board = "qemu-virt"
[product.apps]
preset = ["app/A.bap"]
[product.defaults]
launcher = "bin/sh"
[platform]
arch = "x86_64"
feature_level = "rich"
build_type = "eng"
sideload = false
[platform.boot]
kernel = "KERNEL.ELF@sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
[platform.storage]
image_size_mib = 96
rootfs_split_mib = 32
min_power_atomicity = "p0"
"#;

/// Python oracle：measure_kernel_anchor(load_product(p)) →
/// "MEAS:true" | "MEAS:false" | "REJ:<msg>"（load 面自带 lite=False 全量
/// 臂——语料无文件面节故零文件对账，与 selftest G7 段同形态）。
fn py_measure(pkg_dir: &Path) -> String {
    let code = format!(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\ntry:\n    prod = b.load_product({:?})\n    r = b.measure_kernel_anchor(prod)\n    print('MEAS:' + ('true' if r else 'false') + ':' + ('1' if prod.kernel_measured else '0'))\nexcept b.Reject as e:\n    print('REJ:' + str(e))\nexcept Exception as e:\n    print('EXC:' + repr(e))",
        pkg_dir.join("product.toml")
    );
    let o = Command::new("python3")
        .arg("-c")
        .arg(&code)
        .current_dir(repo_root())
        .output()
        .expect("spawn python3");
    let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
    assert!(o.status.success() && !out.starts_with("EXC"), "python oracle 崩：{out}");
    out
}

/// 双侧跑同一 pkg：Rust anchor vs Python measure → 比对。
fn both(pkg: &Path) -> (String, String) {
    let text = std::fs::read_to_string(pkg.join("product.toml")).unwrap();
    let mut p = load_lite_product(&text).unwrap();
    let rust = match measure_kernel_anchor(&mut p, &pkg.to_string_lossy()) {
        Ok(true) => format!("MEAS:true:{}", if p.kernel_measured { "1" } else { "0" }),
        Ok(false) => format!("MEAS:false:{}", if p.kernel_measured { "1" } else { "0" }),
        Err(e) => format!("REJ:{}", e.0),
    };
    let py = py_measure(pkg);
    (rust, py)
}

#[test]
fn parity_kernel_anchor_three_states() {
    let tmp = std::env::temp_dir().join(format!("berryassemble-anchor-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("product.toml"), PRODUCT_TOML).unwrap();

    // 态①：件缺 → 双侧 false:0（OEM 先声明后放件合法时序）
    let (r, p) = both(&tmp);
    assert_eq!(r, p, "件缺态双侧不同构");
    assert_eq!(r, "MEAS:false:0", "件缺应 false：{r}");

    // 态②：空件 → sha256("") = e3b0c442… 恰等模板锚 → true:1
    std::fs::write(tmp.join("KERNEL.ELF"), b"").unwrap();
    let (r, p) = both(&tmp);
    assert_eq!(r, p, "空件态双侧不同构");
    assert_eq!(r, "MEAS:true:1", "空件应对账过：{r}");

    // 态③：件在而不等 → loud 拒（selftest G7 同字节语料），文案逐字全等
    std::fs::write(tmp.join("KERNEL.ELF"), b"not-the-declared-kernel").unwrap();
    let (r, p) = both(&tmp);
    assert_eq!(r, p, "不等态拒面文案双侧不同构\n rust: {r}\n  py: {p}");
    assert!(r.starts_with("REJ:platform.boot.kernel: 锚对账不等——包内 KERNEL.ELF 现算 sha256:"), "拒面前缀应钉：{r}");
    assert!(r.ends_with("（件与锚不符，步⑧ 前置门——G7；改件须同步改锚）"));

    // 大件分块读腿：5MiB 确定性载荷（i*7&0xff）——先现算真摘要写入锚再双侧验
    let big: Vec<u8> = (0..5 * 1024 * 1024).map(|i| (i * 7 & 0xff) as u8).collect();
    std::fs::write(tmp.join("KERNEL.ELF"), &big).unwrap();
    let sha = sha256_of(&big);
    let text = PRODUCT_TOML.replace(
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        &sha,
    );
    std::fs::write(tmp.join("product.toml"), text).unwrap();
    let (r, p) = both(&tmp);
    assert_eq!(r, p, "大件态双侧不同构");
    assert_eq!(r, "MEAS:true:1", "5MiB 件应过：{r}");

    let _ = std::fs::remove_dir_all(&tmp);
}

/// 独立 SHA-256（测试侧自算真摘要——不与被测实现共源，防同错对拍）。
fn sha256_of(data: &[u8]) -> String {
    let o = Command::new("python3")
        .arg("-c")
        .arg("import hashlib,sys; sys.stdout.write(hashlib.sha256(sys.stdin.buffer.read()).hexdigest())")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut ch| {
            use std::io::Write;
            ch.stdin.as_mut().unwrap().write_all(data).unwrap();
            ch.wait_with_output()
        })
        .expect("spawn python3 hashlib");
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}
