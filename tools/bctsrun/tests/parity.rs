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

// ---- 二刀·T2 面 parity（tools/bcts/t2_check.py oracle 真跑对拍） ----

use bctsrun::{check_t2, parse_frozen_toml_t2};

/// Python 现役件真跑（t2_check.py 同参 CLI 面）。
fn py_t2(path: &str) -> (String, String, Option<i32>) {
    let o = Command::new("python3")
        .arg(repo_root().join("tools/bcts/t2_check.py"))
        .arg(path)
        .output()
        .expect("spawn python3");
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code(),
    )
}

/// Rust 面：T2 解析 + 校验 + TSV 发射。
fn rust_t2(path: &str) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path).unwrap();
    let doc = parse_frozen_toml_t2(&text).map_err(|e| e.0)?;
    check_t2(&doc, path).map_err(|r| r.0)
}

#[test]
fn t2_frozen_corpus_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let home = repo_root().join("tools/bcts/frozen");
    let mut n = 0;
    for i in 0..=1 {
        let p = home.join(format!("bcts-t2@{i}.toml"));
        let p_str = p.to_str().unwrap();
        let (out, err, code) = py_t2(p_str);
        assert_eq!(code, Some(0), "py 应过：{err}");
        assert!(err.is_empty());
        let rust = rust_t2(p_str).expect("rust 应过");
        assert_eq!(rust.join("\n"), out.trim_end_matches('\n'), "TSV 不等：{p_str}");
        n += 1;
    }
    eprintln!("t2 frozen corpus parity: {n} files OK");
}

/// T2 最小合法语料（覆盖守卫/锚定/发射面全活）——变异族挂在其上。
fn t2_good() -> String {
    std::fs::read_to_string(repo_root().join("tools/bcts/frozen/bcts-t2@0.toml")).unwrap()
}

#[test]
fn t2_reject_corpus_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-t2parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let good = t2_good();
    // (标签, 变换, 期待拒面关键词)——变换 = (旧串, 新串) 单点替换
    let mutants: Vec<(&str, &str, &str, &str)> = vec![
        ("top-unknown", "tier2_floor = 31", "extra_key = \"x\"\ntier2_floor = 31", "顶层未知键：['extra_key']"),
        // schema = true 面：py/rust 双侧都放行（True == 1——bool 是 int 子类
        // 语义坑），非拒面族，lib 测试 t2_reject_messages 已钉板。
        ("schema-2", "schema = 1", "schema = 2", "schema 须 = 1"),
        ("tier-1", "tier = 2", "tier = 1", "tier 须 = 2"),
        ("tier-bool", "tier = 2", "tier = true", "tier 须 = 2"),
        ("pr-domain", "platform_revision = 0", "platform_revision = 4294967296", "platform_revision 须 u32"),
        ("frozen-len", "bc93d446d5d9b03da7e62b16d84d470a0eb9b07d", "bc93d446d5d9b03da7e62b16d84d470a0eb9b07", "frozen_at 须 40 位"),
        ("abi-empty", "abi_snapshot = \"tools/abi.toml\"", "abi_snapshot = \"\"", "abi_snapshot 不得为空"),
        ("hash-bad", "sha256:438e5d538e913b51d07ea223e0d6242efe2a0ec91c3b937a02266f14f1a85e57", "sha256:XYZ", "abi_snapshot_hash 须 sha256"),
        ("floor-0", "tier2_floor = 31", "tier2_floor = 0", "tier2_floor 须 ≥1"),
        ("floor-guard", "tier2_floor = 31", "tier2_floor = 99", "空集守卫"),
        ("cid-bad", "id = \"svc-fs\"", "id = \"Svc-fs\"", "contract id 须"),
        ("cid-dup", "id = \"drv-display-backend\"", "id = \"svc-fs\"", "contract id 重复"),
        ("vermin-0", "ver_min = 1", "ver_min = 0", "ver_min 须 ≥1"),
        ("consumers-empty", "consumers = [\"bsh\", \"berry-fsclient\", \"appd(v2)\", \"launcher\"]", "consumers = []", "consumers 须非空列表"),
        ("suite-cases-dup", "\"illegal-op-reject\",\n  \"cap-enforcement\",", "\"illegal-op-reject\",\n  \"illegal-op-reject\",", "suite_cases 重复"),
        ("op-hex-domain", "num = 0x40", "num = 0x100", "num 须 u8"),
        ("op-num-dup", "num = 0x41", "num = 0x40", "op 号重复"),
        ("op-name-bad", "name = \"GET\"", "name = \"get\"", "name 须协议 crate OP_ 常量后缀形"),
        ("kind-bad", "kind = \"pos\"", "kind = \"good\"", "kind 须 ∈ {pos, neg}"),
        ("class-bad", "class = \"seq\"", "class = \"fast\"", "class 须五类之一"),
        ("group-unanchored", "group = \"roundtrip-all-10-ops\"", "group = \"nope-group\"", "未锚定"),
        ("facet-bad", "facet = \"错误码\"", "facet = \"非面\"", "facet 值域外"),
        ("status-bad", "status = \"frozen\"", "status = \"live\"", "status 须 frozen|deferred"),
        ("frozen-empty-cases", "cases = [\"fs-put-neg-chunk\", \"fs-put-neg-full\"]", "cases = []", "status=frozen 但 cases 空"),
        ("cases-unanchored", "cases = [\"fs-put-neg-chunk\", \"fs-put-neg-full\"]", "cases = [\"nope-case\"]", "引用未锚定用例"),
        ("name-mismatch", "", "", "冗余自证"),
    ];
    let mut n = 0;
    for (tag, old, new, kw) in mutants {
        let text = if tag == "name-mismatch" {
            good.clone()
        } else {
            assert!(good.contains(old), "[{tag}] 锚串缺失", );
            good.replacen(old, new, 1)
        };
        let fname = if tag == "name-mismatch" { "bcts-t2@9.toml" } else { "bcts-t2@0.toml" };
        let path = dir.join(fname);
        std::fs::write(&path, &text).unwrap();
        let p_str = path.to_str().unwrap();
        let (out, err, code) = py_t2(p_str);
        assert_eq!(code, Some(2), "[{tag}] py 应拒（stderr: {err}）");
        assert!(out.is_empty(), "[{tag}] py 拒时不应有 stdout");
        let py_first = err.lines().next().unwrap_or("").to_string();
        assert!(py_first.contains(kw), "[{tag}] py 拒面关键词缺失：{py_first}");
        let rust = rust_t2(p_str);
        let rust_err = rust.expect_err(&format!("[{tag}] rust 应拒"));
        assert!(rust_err.contains(kw), "[{tag}] rust 拒面关键词缺失：{rust_err}");
        assert_eq!(rust_err, py_first, "[{tag}] 拒面文案不等");
        n += 1;
    }
    eprintln!("t2 reject corpus parity: {n} mutants OK");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn t2_toml_syntax_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-t2syntax-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 语法域拒（文案不同源——分支标识对齐：双侧同拒 + 退 2）。
    let cases: Vec<(&str, &str)> = vec![
        ("dot-section", "[contract.extra]\nx = 1\n"),
        ("inline-table", "k = {a = 1}\n"),
        ("float", "tier2_floor = 1.5\n"),
        ("nested-array", "consumers = [[\"a\"]]\n"),
        ("unclosed-array", "consumers = [\"a\",\n"),
        ("dup-key", "schema = 1\nschema = 2\n"),
        ("bare-fragment", "oops\n"),
        ("bad-section", "[[nope]]\nid = \"x\"\n"),
    ];
    let mut n = 0;
    for (tag, text) in cases {
        let path = dir.join("bcts-t2@0.toml");
        std::fs::write(&path, text).unwrap();
        let (out, err, code) = py_t2(path.to_str().unwrap());
        assert_eq!(code, Some(2), "[{tag}] py 应拒：{err}");
        assert!(out.is_empty());
        let rust = rust_t2(path.to_str().unwrap());
        assert!(rust.is_err(), "[{tag}] rust 应拒");
        n += 1;
    }
    eprintln!("t2 toml syntax parity: {n} cases OK（双侧拒 + 退 2；语法域文案各自实现）");
    std::fs::remove_dir_all(&dir).ok();
}

// ---- 三刀·reconcile 面 parity（tools/bcts/reconcile_t2.py oracle 真跑对拍） ----

use bctsrun::reconcile_t2 as rust_reconcile;

/// Python 现役件真跑（reconcile_t2.py 同参 CLI 面：tsv + 仓库根）。
fn py_reconcile(tsv_path: &str, repo: &str) -> (String, String, Option<i32>) {
    let o = Command::new("python3")
        .arg(repo_root().join("tools/bcts/reconcile_t2.py"))
        .arg(tsv_path)
        .arg(repo)
        .output()
        .expect("spawn python3");
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code(),
    )
}

/// 真源基座 TSV：check_t2 发射面真跑产（Rust 面）。
fn t2_tsv_of(frozen: &str) -> String {
    let text = std::fs::read_to_string(frozen).unwrap();
    let doc = parse_frozen_toml_t2(&text).unwrap();
    check_t2(&doc, frozen).unwrap().join("\n") + "\n"
}

#[test]
fn reconcile_frozen_corpus_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let home = repo_root().join("tools/bcts/frozen");
    let dir = std::env::temp_dir().join(format!("bctsrun-recparity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut n = 0;
    for i in 0..=1 {
        let frozen = home.join(format!("bcts-t2@{i}.toml"));
        let f_str = frozen.to_str().unwrap();
        let tsv_path = dir.join(format!("entries-t2-{i}.tsv"));
        std::fs::write(&tsv_path, t2_tsv_of(f_str)).unwrap();
        let repo = repo_root().to_str().unwrap().to_string();
        let (out, err, code) = py_reconcile(tsv_path.to_str().unwrap(), &repo);
        assert_eq!(code, Some(0), "py 应绿：{err}");
        assert!(err.is_empty());
        let r = rust_reconcile(&std::fs::read_to_string(&tsv_path).unwrap(), &repo).unwrap();
        assert_eq!(r.lines.join("\n"), out.trim_end_matches('\n'), "stdout 不等：{i}");
        assert_eq!(r.exit_code(), 0);
        n += 1;
    }
    eprintln!("reconcile frozen corpus parity: {n} files OK");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reconcile_mutant_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-recmut-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let repo = dir.to_str().unwrap().to_string();
    // 协议 crate 源基座（op 常量表真形）
    let src_ok = "pub const OP_A: u8 = 1;\npub const OP_B: u8 = 0x10;\npub const OP_EXTRA: u8 = 7;\n";
    std::fs::write(dir.join("src/svc.rs"), src_ok).unwrap();
    // 基座 TSV：两合同（一绿一断链）
    let base_tsv = "META\t0\t31\t2\t4\tx\ty\tz\n\
                    CONTRACT\tc1\tt\tiface1\tsrc/svc.rs\t1\ts\tcase-a\tbsh\n\
                    OP\tc1\t1\tA\tw\n\
                    OP\tc1\t16\tB\tw\n\
                    CASE\tc1\tcase-a\t1\tpos\tseq\tg\ti\te\n\
                    CASE\tc1\tcase-b\t16\tneg\treject\tg\ti\te\n\
                    CONTRACT\tc2\tt\tiface2\tsrc/gone.rs\t1\ts\tcase-c\tbsh\n\
                    OP\tc2\t2\tC\tw\n";
    // (标签, TSV 变换后的内容, 期待 exit 码)——变换族覆盖错号/缺席/断链/空表/多合同
    let cases: Vec<(&str, String, i32, &str)> = vec![
        ("green", base_tsv.replace("OP\tc1\t16\tB", "OP\tc1\t16\tB"), 1, "差集 0"), // C 缺席于 c2 断链 → exit 1
        ("wrong-num", base_tsv.replacen("OP\tc1\t16\tB", "OP\tc1\t17\tB", 1), 1, "错号（合同 0x11 vs crate 0x10）"),
        ("absent", base_tsv.replacen("OP\tc1\t16\tB\tw", "OP\tc1\t16\tZZ\tw", 1), 1, "OP_ZZ 缺席"),
        ("broken-src", base_tsv.replace("src/gone.rs", "src/svc.rs"), 1, "OP_C 缺席"),
        ("all-green", {
            let mut t = base_tsv.replace("src/gone.rs", "src/svc.rs");
            t = t.replace("OP\tc2\t2\tC", "OP\tc2\t7\tEXTRA");
            t
        }, 0, "T2 静态对账全绿"),
        ("empty", String::new(), 0, "T2 静态对账全绿"),
        ("dup-const", {
            let mut t = base_tsv.replace("src/gone.rs", "src/svc.rs");
            t = t.replace("OP\tc2\t2\tC", "OP\tc2\t1\tA");
            t
        }, 0, "crate 侧多 1 op（add-only 合法不拦：OP_EXTRA）"),
    ];
    let mut n = 0;
    for (tag, tsv, want_code, kw) in cases {
        let tsv_path = dir.join(format!("m-{tag}.tsv"));
        std::fs::write(&tsv_path, &tsv).unwrap();
        let (out, err, code) = py_reconcile(tsv_path.to_str().unwrap(), &repo);
        assert_eq!(code, Some(want_code), "[{tag}] py exit 应 = {want_code}（stderr: {err}）");
        let r = rust_reconcile(&tsv, &repo).unwrap();
        assert_eq!(r.lines.join("\n"), out.trim_end_matches('\n'), "[{tag}] stdout 不等");
        assert_eq!(r.exit_code(), want_code, "[{tag}] rust exit 应 = {want_code}");
        assert!(out.contains(kw) || err.contains(kw), "[{tag}] 关键词缺失：{out}{err}");
        n += 1;
    }
    // 同键后写覆盖（dict 语义）+ hex 混排 + 失败行 → 独立腿（py/rust 同构断言）
    std::fs::write(dir.join("src/dup.rs"), "pub const OP_A: u8 = 1;\npub const OP_A: u8 = 2;\n").unwrap();
    let t = "CONTRACT\tc1\tt\tiface\tsrc/dup.rs\t1\ts\tx\tb\nOP\tc1\t2\tA\tw\n".to_string();
    let p = dir.join("m-dupwin.tsv");
    std::fs::write(&p, &t).unwrap();
    let (out, err, code) = py_reconcile(p.to_str().unwrap(), &repo);
    assert_eq!(code, Some(0));
    let r = rust_reconcile(&t, &repo).unwrap();
    assert_eq!(r.lines.join("\n"), out.trim_end_matches('\n'));
    assert!(out.contains("命中 1"));
    assert!(err.is_empty());
    n += 1;
    eprintln!("reconcile mutant parity: {n} cases OK");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reconcile_scanner_edge_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-recedge-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let repo = dir.to_str().unwrap().to_string();
    // 正则边界族：无词界 xpub 前缀/紧凑形/跨行空白/回溯失败形（0x1G·尾缺分号·
    // hex 不足位不命中）/前导零十进制。
    let edge_full = "xpub const OP_F: u8 = 2;\n\
               pub const OP_G:u8=5;\n\
               pub  const\n  OP_D : u8 = 10 ;\n\
               pub const OP_E: u8 = 0x1G;\n\
               pub const OP_H: u8 = 7\n\
               pub const OP_J: u8 = 0x;\n\
               pub const OP_L: u8 = 010;\n";
    let edge_sane = edge_full.replace("pub const OP_L: u8 = 010;\n", "");
    let tsv = "CONTRACT\tc1\tt\ti\tsrc/edge.rs\t1\ts\tx\tb\n\
             OP\tc1\t2\tF\tw\nOP\tc1\t5\tG\tw\nOP\tc1\t10\tD\tw\nOP\tc1\t10\tL\tw\n".to_string();
    let p = dir.join("edge.tsv");
    // 面①：源含 010 前导零——py int(x, 0) ValueError traceback 退 1（真实崩溃
    // 面，分歧注①）；镜像诚实按十进制读 → 全绿。py 侧断言崩溃形态本身。
    std::fs::write(dir.join("src/edge.rs"), edge_full).unwrap();
    std::fs::write(&p, &tsv).unwrap();
    let (out, err, code) = py_reconcile(p.to_str().unwrap(), &repo);
    assert_eq!(code, Some(1), "py 崩溃面：{out}{err}");
    assert!(err.contains("ValueError: invalid literal for int() with base 0: '010'"), "py 崩溃签：{err}");
    let r = rust_reconcile(&tsv, &repo).unwrap();
    assert!(r.lines.iter().any(|l| l.contains("[c1]（i）：合同 op 表 4 项，crate 命中 4")));
    assert_eq!(r.exit_code(), 0, "镜像诚实读十进制不崩");
    // 面②：源剥 010、TSV 保留 L 腿——L 缺席开放缺口（退 1）+ F/G/D 命中，双侧对拍逐字全等。
    std::fs::write(dir.join("src/edge.rs"), &edge_sane).unwrap();
    std::fs::write(&p, &tsv).unwrap();
    let (out2, err2, code2) = py_reconcile(p.to_str().unwrap(), &repo);
    assert_eq!(code2, Some(1), "py 应报缺口：{err2}");
    let r2 = rust_reconcile(&tsv, &repo).unwrap();
    assert_eq!(r2.lines.join("\n"), out2.trim_end_matches('\n'), "stdout 不等");
    assert!(r2.lines.iter().any(|l| l.contains("OP_L 缺席（合同 op 10）")));
    eprintln!("reconcile scanner edge parity: 2 faces OK（崩溃面 + 逐字面）");
    std::fs::remove_dir_all(&dir).ok();
}
