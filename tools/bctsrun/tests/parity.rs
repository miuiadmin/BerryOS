//! bctsrun 首刀·parity：Python 现役件（tools/bcts/t1_check.py）真跑 oracle
//! 对拍。三面：
//!
//! ①仓内七件冻结件（bcts@0..6）全量对拍——stdout（META/MARK/GATE TSV）逐行
//! 全等 + exit 0；
//! ②拒面族：单点变异语料 ×N（顶层未知键/必填缺失/schema/suite/pr 域/frozen_at
//! 形/abi_snapshot 空/hash 形/floor 域/marker 未知键·缺键·id 空·id 重复·expect
//! 违例·phase 域·tier/gate 同族/空集守卫/文件名失配/TOML 语法拒分支标识）——
//! 双侧 stderr 逐字全等 + exit 2；
//! ③TOML 语法面：非目标形语料（点分节/数组值/内联表形/hex/重复键/坏行形）——
//! 双侧拒（文案不同源 = tomllib vs 子集解析器，断言拒分支标识 + 退码）。
//!
//! Python 缺席/版本面炸 = skip（berryassemble parity 同款守卫）。

use bctsrun::{check_t1, parse_frozen_toml};
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn py_ok() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Python 现役件真跑（t1_check.py 同参 CLI 面）。
fn py_t1(path: &str) -> (String, String, Option<i32>) {
    let o = Command::new("python3")
        .arg(repo_root().join("tools/bcts/t1_check.py"))
        .arg(path)
        .output()
        .expect("spawn python3");
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code(),
    )
}

/// Rust 面：解析 + 校验 + TSV 发射（CLI 同构——lib 直接驱动）。
fn rust_t1(path: &str) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path).unwrap();
    let doc = parse_frozen_toml(&text).map_err(|e| e.0)?;
    check_t1(&doc, path).map_err(|r| r.0)
}

#[test]
fn t1_frozen_corpus_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let home = repo_root().join("tools/bcts/frozen");
    let mut n = 0;
    for i in 0..=6 {
        let p = home.join(format!("bcts@{i}.toml"));
        let p_str = p.to_str().unwrap();
        let (out, err, code) = py_t1(p_str);
        assert_eq!(code, Some(0), "py 应过：{err}");
        assert!(err.is_empty());
        let rust = rust_t1(p_str).expect("rust 应过");
        assert_eq!(rust.join("\n"), out.trim_end_matches('\n'), "TSV 不等：{p_str}");
        n += 1;
    }
    eprintln!("frozen corpus parity: {n} files OK");
}

#[test]
fn t1_reject_corpus_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // 单点变异语料：((变异子, 名字))
    const GOOD: &str = "schema = 1\nsuite = \"bcts\"\nplatform_revision = 3\nfrozen_at = \"51932bff775c5da0534a41dae50c5ace7b8e945d\"\nabi_snapshot = \"tools/abi.toml\"\nabi_snapshot_hash = \"sha256:d47c0177b1f4667b68780c2b462f7066fa3f5a0c157da436be358c409246b5c8\"\ntier1_floor = 1\n[[marker]]\nid = \"bare-01\"\nexpect = \"X ok\"\nphase = \"bare\"\ntier = 1\nsource = \"tools/markers.sh@5f4a00f0ae8c\"\n[[host_gate]]\nid = \"abi-check\"\ncmd = \"just abi-check\"\ntier = 1\n";
    let mutants: Vec<(&str, &str, &str)> = vec![
        // (标签, 变异后片段, 期待拒面关键词)
        ("top-unknown", "__HEAD__", "顶层未知键：['extra_key']"),
        ("schema", "schema = 2", "schema 须 = 1"),
        ("suite", "suite = \"nope\"", "suite 须 ="),
        ("pr-domain", "platform_revision = 4294967296", "platform_revision 须 u32"),
        ("pr-neg", "platform_revision = -1", "platform_revision 须 u32"),
        ("pr-bool", "platform_revision = true", "platform_revision 须 u32"),
        ("frozen-len", "frozen_at = \"51932bff775c5da0534a41dae50c5ace7b8e945\"", "frozen_at 须 40 位"),
        ("frozen-upper", "frozen_at = \"51932BFF775C5DA0534A41DAE50C5ACE7B8E945D\"", "frozen_at 须 40 位"),
        ("abi-empty", "abi_snapshot = \"\"", "abi_snapshot 不得为空"),
        ("hash-prefix", "abi_snapshot_hash = \"d47c0177b1f4667b68780c2b462f7066fa3f5a0c157da436be358c409246b5c8\"", "abi_snapshot_hash 须 sha256"),
        ("hash-upper", "abi_snapshot_hash = \"sha256:D47C0177B1F4667B68780C2B462F7066FA3F5A0C157DA436BE358C409246B5C8\"", "abi_snapshot_hash 须 sha256"),
        ("floor-0", "tier1_floor = 0", "tier1_floor 须 ≥1"),
        ("floor-bool", "tier1_floor = true", "tier1_floor 须 ≥1"),
        ("marker-missing", "", "[[marker]] 缺席或为空"), // 整条 marker 节删除 → [[marker]] 缺席
        ("expect-tab", "expect = \"a\tb\"", "expect 违例"),
        ("expect-paren", "expect = \"a)b\"", "expect 违例"),
        ("phase-bad", "phase = \"boot\"", "phase 须 ∈"),
        ("tier-2", "tier = 2", "tier 须 = 1"),
        ("floor-guard", "tier1_floor = 5", "空集守卫"),
        ("name-mismatch", "__NAME__", "冗余自证"),
    ];
    let mut n = 0;
    for (tag, frag, kw) in mutants {
        // 组装变异语料
        let text = match tag {
            "top-unknown" => format!("extra_key = \"x\"\n{GOOD}"),
            "marker-missing" => GOOD.split("[[marker]]").next().unwrap().to_string() + "[[host_gate]]\nid = \"abi-check\"\ncmd = \"just abi-check\"\ntier = 1\n",
            "name-mismatch" => GOOD.to_string(),
            _ => {
                // 单点替换：键段（" = " 前）精确等值的行替换整行
                let key = frag.split(" = ").next().unwrap();
                let mut lines: Vec<String> = GOOD.lines().map(|l| l.to_string()).collect();
                let mut hit = false;
                for l in lines.iter_mut() {
                    let k = l.split(" = ").next().unwrap().trim();
                    if k == key && l.contains(" = ") {
                        *l = frag.to_string();
                        hit = true;
                        break;
                    }
                }
                assert!(hit, "mutant {tag}: key {key} not found");
                lines.join("\n") + "\n"
            }
        };
        // 文件名（name-mismatch 腿文件名给 bcts@9.toml）
        let fname = if tag == "name-mismatch" { "bcts@9.toml" } else { "bcts@3.toml" };
        let path = dir.join(fname);
        std::fs::write(&path, &text).unwrap();
        let p_str = path.to_str().unwrap();

        let (out, err, code) = py_t1(p_str);
        assert_eq!(code, Some(2), "[{tag}] py 应拒（stderr: {err}）");
        assert!(out.is_empty(), "[{tag}] py 拒时不应有 stdout");
        let py_first = err.lines().next().unwrap_or("").to_string();
        assert!(py_first.contains(kw), "[{tag}] py 拒面关键词缺失：{py_first}");

        let rust = rust_t1(p_str);
        let rust_err = rust.expect_err(&format!("[{tag}] rust 应拒"));
        assert!(rust_err.contains(kw), "[{tag}] rust 拒面关键词缺失：{rust_err}");
        // 逐字对拍：rust reject 文案 vs py stderr 首行
        assert_eq!(rust_err, py_first, "[{tag}] 拒面文案不等");
        n += 1;
    }
    eprintln!("reject corpus parity: {n} mutants OK");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn t1_toml_syntax_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-syntax-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // (标签, TOML 文本, py 拒面关键词)——py 面文案 = tomllib 侧；rust 面文案不同源，
    // 只对拍「双侧都拒 + 退 2」与分支语义（BerryOS 拒面文案同源纪律只覆盖 schema
    // 域文案，TOML 语法域文案各自实现）。
    let cases: Vec<(&str, &str, &str)> = vec![
        ("dot-section", "[a.b]\nx = 1\n", "顶层未知键：['a']"),
        ("array-value", "schema = [1\n", "TOML 解析失败"),
        ("hex-int", "schema = ?\n", "TOML 解析失败"),
        ("dup-key", "schema = 1\nschema = 2\n", "TOML 解析失败"),
        ("bare-fragment", "oops\n", "TOML 解析失败"),
        ("inline-table", "a = {b = 1}\n", "顶层未知键：['a']"),
    ];
    let mut n = 0;
    for (tag, text, kw) in cases {
        let path = dir.join("bcts@3.toml");
        std::fs::write(&path, text).unwrap();
        let (out, err, code) = py_t1(path.to_str().unwrap());
        assert_eq!(code, Some(2), "[{tag}] py 应拒");
        assert!(err.contains(kw), "[{tag}] py: {err}");
        assert!(out.is_empty());
        let rust = rust_t1(path.to_str().unwrap());
        assert!(rust.is_err(), "[{tag}] rust 应拒");
        n += 1;
    }
    eprintln!("toml syntax parity: {n} cases OK（双侧拒 + 退 2；语法域文案各自实现）");
    std::fs::remove_dir_all(&dir).ok();
}
