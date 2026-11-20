//! berryassemble 首刀·oracle 对拍：Python 现役件（tools/berry_assemble.py）
//! 真跑（import + `python3 -c` 直驱函数面），四个纯面双侧同构：
//!
//! ①serialize_boot_params 字节恒等（SimpleNamespace 直驱——同清单同字节）；
//! ②validate_platform_revision 拒面文案全等（域界/反回滚两臂 + 过面值）；
//! ③kernel 锚 ANCHOR_RE fullmatch 对拍（accept 梯 + 拒梯形）；
//! ④platform level 真源单拍（tools/abi.toml 现值——Rust 读器 vs Python
//! `_platform_level()`，含坏源拒面文案）。
//!
//! Python 缺席/版本面炸 = skip（gensourceindex parity 同款守卫）。

use berryassemble::*;
use std::process::Command;

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Python 现役件直驱（berry_assemble import 面）。stdout 尾行 = 结果；
/// Reject 走 stderr（前置标记行 REJ:）。
fn py_run(code: &str) -> (String, String, Option<i32>) {
    let o = Command::new("python3")
        .arg("-c")
        .arg(code)
        .current_dir(repo_root())
        .output()
        .expect("spawn python3");
    (
        String::from_utf8_lossy(&o.stdout).trim().to_string(),
        String::from_utf8_lossy(&o.stderr).trim().to_string(),
        o.status.code(),
    )
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Python serialize_boot_params 直驱（params 字典字面量 → hex）。
fn py_serialize(params_expr: &str) -> (String, bool) {
    let (out, err, _) = py_run(&format!(
        "import sys; sys.path.insert(0, 'tools')\nfrom types import SimpleNamespace\nimport berry_assemble as b\nprint(b.serialize_boot_params(SimpleNamespace(params={params_expr})).hex())"
    ));
    (out, err.is_empty())
}

fn py_serialize_ok(params_expr: &str) -> String {
    let (out, ok) = py_serialize(params_expr);
    assert!(ok, "python oracle 失败：{out}");
    out
}

#[test]
fn parity_serialize_boot_params() {
    // 三键全席（acme 演练参照物同款清单）
    let mut p = check_boot_params(&map(&[
        ("sched_slice_hz", val_i(64)),
        ("intent_default", val_s("interactive")),
        ("timer_slack", val_i(1_250_000)),
    ]))
    .unwrap();
    assert_eq!(
        hex(&serialize_boot_params(&p)),
        py_serialize_ok(
            "{'sched_slice_hz': 64, 'intent_default': 'interactive', 'timer_slack': 1250000}"
        )
    );
    // 单键 u32 + 字典序（intent 先于 sched_）
    p.retain(|k, _| k != "timer_slack");
    assert_eq!(
        hex(&serialize_boot_params(&p)),
        py_serialize_ok("{'sched_slice_hz': 64, 'intent_default': 'interactive'}")
    );
    // 恰界值：1024 / 999_999_999
    let q = check_boot_params(&map(&[
        ("sched_slice_hz", val_i(1024)),
        ("timer_slack", val_i(999_999_999)),
    ]))
    .unwrap();
    assert_eq!(
        hex(&serialize_boot_params(&q)),
        py_serialize_ok("{'sched_slice_hz': 1024, 'timer_slack': 999999999}")
    );
    // 空 params = 纯 schema 头行（内核消费端空清单同字节）
    let empty = std::collections::BTreeMap::new();
    assert_eq!(hex(&serialize_boot_params(&empty)), py_serialize_ok("{}"));
}

fn map(kv: &[(&str, toml_val::Val)]) -> std::collections::BTreeMap<String, toml_val::Val> {
    kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn val_i(n: i64) -> toml_val::Val {
    toml_val::Val::Int(n)
}

fn val_s(s: &str) -> toml_val::Val {
    toml_val::Val::Str(s.to_string())
}

#[test]
fn parity_validate_platform_revision_rejects() {
    // Python 直驱两臂拒面（level 真源现值注入双侧同读）
    let cases: Vec<(String, i64)> = vec![
        ("域界上".into(), 4294967296),
        ("域界下".into(), -1),
        ("反回滚超前".into(), 1 << 40),
    ];
    let level = platform_level_of(&std::fs::read_to_string(repo_root().join("tools/abi.toml")).unwrap()).unwrap();
    for (name, pr) in cases {
        let rust = validate_platform_revision(pr, level).err().map(|r| r.0);
        let (out, err, _) = py_run(&format!(
            "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\ntry:\n    b.validate_platform_revision({pr})\n    print('OK')\nexcept b.Reject as e:\n    print('REJ:', e)"
        ));
        let py_rej = out.strip_prefix("REJ: ").map(|s| s.to_string());
        assert!(py_rej.is_some(), "{name}: python 未拒（{out} {err}）");
        match rust {
            Some(r) => assert_eq!(r, py_rej.unwrap(), "{name}: 拒面文案漂移"),
            None => panic!("{name}: rust 未拒（python 拒了）"),
        }
    }
    // 过面：0 与 level 等值（等值 = 合法旧产品装新平台）
    assert_eq!(validate_platform_revision(0, level).unwrap(), 0);
    let (out, _, _) = py_run(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(b.validate_platform_revision(0))",
    );
    assert_eq!(out, "0");
}

#[test]
fn parity_kernel_anchor_regex() {
    // Python ANCHOR_RE.fullmatch 对拍：accept 梯 + 拒梯
    let accepts = [
        format!("K@sha256:{}", "a".repeat(64)),
        format!("KERNEL.ELF@sha256:{}", "A19b".repeat(16)),
        format!("k-9._@sha256:{}", "0".repeat(64)),
    ];
    let rejects = [
        ".K@sha256:aaaa".to_string(),
        format!("K@sha256:{}", "g".repeat(64)),
        format!("K@sha256:{}", "a".repeat(63)),
        format!("K@sha256:{}", "a".repeat(65)),
        "K@md5:aaaa".to_string(),
        format!("a@b@sha256:{}", "a".repeat(64)),
        format!("K@sha256:{}/", "a".repeat(63)),
    ];
    for a in accepts {
        assert!(parse_kernel_anchor(&a).is_ok(), "rust 拒了 accept 形 {a}");
        let (out, _, _) = py_run(&format!(
            "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(bool(b.ANCHOR_RE.fullmatch({a:?})))"
        ));
        assert_eq!(out, "True", "python 拒了 {a}");
    }
    for r in rejects {
        assert!(parse_kernel_anchor(&r).is_err(), "rust 过了 reject 形 {r}");
        let (out, _, _) = py_run(&format!(
            "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(bool(b.ANCHOR_RE.fullmatch({r:?})))"
        ));
        assert_eq!(out, "False", "python 过了 {r}");
    }
}

#[test]
fn parity_platform_level_true_source() {
    // 真源单拍：tools/abi.toml 现值双侧同读
    let text = std::fs::read_to_string(repo_root().join("tools/abi.toml")).unwrap();
    let rust = platform_level_of(&text).unwrap();
    let (out, err, _) = py_run(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(b._platform_level())",
    );
    assert!(err.is_empty(), "python _platform_level 失败：{err}");
    assert_eq!(rust.to_string(), out);
    // 坏源拒面分支对拍（合法 TOML 非整型值 "7"——双侧同咬「非整数」臂；
    // 文案 repr 形不同源〔Rust raw reader 保留引号 vs Python tomllib 剥壳〕，
    // 分支标识全等 + 双侧 loud 拒即钉板，逐字文案归后续 CLI 刀统一）
    let rust_bad = platform_level_of("[platform]\nlevel = \"7\"\n").err().unwrap().0;
    assert!(rust_bad.contains("非整数"), "rust 分支漂移：{rust_bad}");
    let (out, _, _) = py_run(
        "import sys, tempfile, os; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nb.ROOT = tempfile.mkdtemp()\nopen(os.path.join(b.ROOT, 'abi.toml'), 'w').write('[platform]\\nlevel = \"7\"\\n')\ntry:\n    b._platform_level()\nexcept b.Reject as e:\n    print('REJ:', e)",
    );
    let py_bad = out.strip_prefix("REJ: ").unwrap().to_string();
    assert!(py_bad.contains("非整数"), "python 分支漂移：{py_bad}");
}
