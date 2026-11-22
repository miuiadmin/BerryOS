//! berryassemble 七刀·`--emit` 产件面 oracle 对拍：Rust `emit_boot_params`
//! ↔ Python 现役件 `do_emit`（tools/berry_assemble.py :1046）双侧对拍。
//! 对拍强度 = **产件字节恒等（sha256）+ 尾行 stdout 全等（剥路径段）+
//! 空 params 零扰动（零产件零目录）+ IO 失败同构（out-dir 中途被文件
//! 占位——Python OSError 裸炸 traceback vs Rust 单行收口，分支标识
//! 对齐）**。合成现场 = plan_parity 同款布局（resolve/full 的文件面
//! 依赖全部 stub），行面前缀（build_plan 计划行）已由 plan_parity 钉板
//! ——本刀只对拍 emit 尾段。Python 缺席 = skip。
//!
//! 腿面：① 全席 params（u32 + intent 双形——三键词表 v1 全键）② 空
//! params（缺省零扰动合同——零产件零目录）③ 嵌套 out-dir 深位形
//! （makedirs 递归建 etc/）④ 拒面前置（坏 product.toml——emit 尾段
//! 不可达，计划面拒行前置钉板）⑤ IO 失败（out-dir 被普通文件占位
//! ——makedirs OSError 同构）。

use std::path::{Path, PathBuf};
use std::process::Command;

use berryassemble::full::check_full;
use berryassemble::plan::emit_boot_params;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

const BASE_TOML: &str = "version = 1\n\n[entries]\n\"bin/berry-launcher\" = \"cargo-target/release/berry-launcher\"\n";
const KERNEL_SHA_EMPTY: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// 合成现场（plan_parity 同款最小面——emit 尾段只消费 params 与
/// out_dir，preset/override/trust 面不进腿）。
fn setup(tag: &str, product: &str) -> (PathBuf, PathBuf) {
    let tmp = std::env::temp_dir().join(format!(
        "berryassemble-emit-parity-{}-{}",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    let user = tmp.join("user");
    std::fs::create_dir_all(user.join("cargo-target/release")).unwrap();
    std::fs::write(user.join("cargo-target/release/berry-launcher"), b"stub").unwrap();
    std::fs::write(tmp.join("base.toml"), BASE_TOML).unwrap();
    let pkg = tmp.join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("product.toml"), product).unwrap();
    (tmp, pkg)
}

/// params 全席最小清单（三键词表 v1 全键——u32 双键 + intent）。
fn product_params_full() -> String {
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
        + KERNEL_SHA_EMPTY
        + "\"\n[platform.boot.params]\nsched_slice_hz = 250\nintent_default = \"interactive\"\ntimer_slack = 500000\n"
        + "[platform.storage]\nimage_size_mib = 96\nrootfs_split_mib = 32\nmin_power_atomicity = \"p0\"\n"
}

fn product_params_empty() -> String {
    product_params_full().replacen(
        "[platform.boot.params]\nsched_slice_hz = 250\nintent_default = \"interactive\"\ntimer_slack = 500000\n",
        "",
        1,
    )
}

/// Python oracle：同现场 do_emit 全行（成功 = 计划行 + 尾行；拒 =
/// `REJ:`/`EXC:` 前缀）。out_dir 相对 cwd（与 Rust 侧同基准）。
fn py_emit(pkg_dir: &Path, base: &Path, user_dir: &str, out_dir: &str, pr: u32) -> (String, String) {
    let code = format!(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\ntry:\n    b.do_emit({:?}, {:?}, {pr}, {:?}, {:?})\nexcept b.Reject as e:\n    print('REJ:' + str(e))\nexcept SystemExit as e:\n    print('REJ:' + str(e))\nexcept OSError as e:\n    print('EXC:' + repr(e))",
        pkg_dir.join("product.toml"),
        out_dir,
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
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Rust 侧 emit 尾行（计划行不重打——plan_parity 已钉板；此处走
/// check_full → emit_boot_params 组合，拒面经 check_full 前置）。
fn rust_emit_line(pkg_dir: &Path, out_dir: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(pkg_dir.join("product.toml")).unwrap();
    let src_dir = pkg_dir.display().to_string();
    let prod = match check_full(&text, &src_dir) {
        Ok(p) => p,
        Err(e) => return Err(format!("REJ:{e}")),
    };
    match emit_boot_params(&prod, out_dir) {
        Ok(line) => Ok(line),
        Err(e) => Err(format!("EXC:{e}")),
    }
}

fn sha256_hex(data: &[u8]) -> String {
    // mkapp cpio::sha256_hex 同款自持（FIPS 180-4）——依赖面借道
    // mkcpio path 依赖先例（gensourceindex 传递面显式直连）。
    // 此处独立小实现（测试件不引 path 依赖——emit 件自证）。
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let bitlen = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[test]
fn emit_parity_tail() {
    // Python oracle 缺席 = skip（族守卫）。
    if !repo_root().join("tools/berry_assemble.py").exists() {
        return;
    }

    // 腿①：全席 params（三键全——u32 双键 + intent）
    {
        let (tmp, pkg) = setup("t1", &product_params_full());
        let base = tmp.join("base.toml");
        let user_dir = tmp.join("user").display().to_string();
        let py_out = tmp.join("py-emit");
        let (py_stdout, _) = py_emit(&pkg, &base, &user_dir, py_out.to_str().unwrap(), 0);
        assert!(!py_stdout.starts_with("REJ:") && !py_stdout.contains("REJ:"), "py 意外拒：{py_stdout}");
        let py_tail = py_stdout.lines().last().unwrap().to_string();

        let r_out = tmp.join("rust-emit");
        let r_line = rust_emit_line(&pkg, r_out.to_str().unwrap()).expect("rust 应过");
        // 尾行剥路径段后全等（tmp 前缀 → <TMP>；out-dir 段名 rust-emit/
        // py-emit 也归一——同基不同名是本测布局形，非契约面）。
        let t = tmp.display().to_string();
        let norm = |s: &str| s.replace(&t, "<TMP>").replace("rust-emit", "OUT").replace("py-emit", "OUT");
        assert_eq!(
            norm(&r_line),
            norm(&py_tail),
            "尾行漂移\nrust: {r_line}\npy:   {py_tail}"
        );
        // 产件字节恒等（双侧独立产 → sha256 对拍）。
        let py_blob = std::fs::read(py_out.join("etc/boot-params.toml")).unwrap();
        let r_blob = std::fs::read(r_out.join("etc/boot-params.toml")).unwrap();
        assert_eq!(py_blob, r_blob, "产件字节漂移");
        assert_eq!(sha256_hex(&r_blob), sha256_hex(&py_blob));
        // 内容自证：schema 头 + 字典序 + u32 裸数字/intent 引号。
        let text = String::from_utf8_lossy(&r_blob).into_owned();
        assert_eq!(
            text,
            "schema = 1\nintent_default = \"interactive\"\nsched_slice_hz = 250\ntimer_slack = 500000\n"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    // 腿②：空 params（缺省零扰动——零产件零目录双侧同构）
    {
        let (tmp, pkg) = setup("t2", &product_params_empty());
        let base = tmp.join("base.toml");
        let user_dir = tmp.join("user").display().to_string();
        let py_out = tmp.join("py-emit");
        let (py_stdout, _) = py_emit(&pkg, &base, &user_dir, py_out.to_str().unwrap(), 0);
        let py_tail = py_stdout.lines().last().unwrap().to_string();
        assert!(py_tail.contains("emit 空 params 零产件"), "py 空扰动行缺：{py_tail}");

        let r_out = tmp.join("rust-emit");
        let r_line = rust_emit_line(&pkg, r_out.to_str().unwrap()).expect("rust 应过");
        assert_eq!(r_line, py_tail, "空 params 尾行漂移");
        assert!(!r_out.exists(), "空 params 零产件零目录被破");
        assert!(!py_out.exists(), "py 侧空 params 竟产目录");
        std::fs::remove_dir_all(&tmp).ok();
    }

    // 腿③：嵌套深位 out-dir（makedirs 递归建——a/b/c/etc/... 形）
    {
        let (tmp, pkg) = setup("t3", &product_params_full());
        let r_out = tmp.join("deep/a/b/c");
        let line = rust_emit_line(&pkg, r_out.to_str().unwrap()).expect("rust 应过");
        assert!(line.contains("产件"), "产件行缺：{line}");
        assert!(r_out.join("etc/boot-params.toml").is_file(), "深位产件未落");
        std::fs::remove_dir_all(&tmp).ok();
    }

    // 腿④：拒面前置（坏 product.toml——emit 尾段不可达，拒行前置）
    {
        let (tmp, pkg) = setup("t4", "schema = 2\n");
        let base = tmp.join("base.toml");
        let user_dir = tmp.join("user").display().to_string();
        let py_out = tmp.join("py-emit");
        let (py_stdout, _) = py_emit(&pkg, &base, &user_dir, py_out.to_str().unwrap(), 0);
        assert!(py_stdout.contains("REJ:"), "py 应拒：{py_stdout}");
        let r = rust_emit_line(&pkg, tmp.join("rust-emit").to_str().unwrap());
        assert!(r.unwrap_err().starts_with("REJ:"), "rust 应拒");
        assert!(!py_out.exists(), "拒路径不应产件");
        std::fs::remove_dir_all(&tmp).ok();
    }

    // 腿⑤：IO 失败同构（out-dir 中途被普通文件占位——makedirs OSError）
    {
        let (tmp, pkg) = setup("t5", &product_params_full());
        // 占位文件：out/another 路径段已有普通文件 → makedirs(深位) 炸。
        let blocker = tmp.join("blocked");
        std::fs::write(&blocker, b"not a dir").unwrap();
        let out_dir = blocker.join("etc").to_str().unwrap().to_string();
        // Python：OSError 裸炸（traceback exit 1——stderr 非空）。
        let base = tmp.join("base.toml");
        let user_dir = tmp.join("user").display().to_string();
        let (py_stdout, py_stderr) = py_emit(&pkg, &base, &user_dir, &out_dir, 0);
        assert!(py_stderr.contains("Traceback") || py_stdout.contains("EXC:"), "py 应炸：{py_stderr}");
        // Rust：单行收口（EXC: 前缀分支标识）。
        let r = rust_emit_line(&pkg, &out_dir);
        assert!(r.unwrap_err().starts_with("EXC:"), "rust 应炸");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
