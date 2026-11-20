//! W1-Q27 七刀·`--json` 信封 CLI 对拍：Rust bin `--json` ↔ Python
//! `mkapp.py --json` oracle 真跑对拍（build_parity/check_parity 同款纪律）。
//!
//! 覆盖：正例 build/check 两 op result 舱逐键全等 + 负例七类钉板族拒因
//! 信封七舱全等（code/where/line/expected/got/fix/message——纯 std 子串
//! 断言，零外部依赖）+ io 兜底 code 域对齐 + 单行纪律 + 双读一致（人读
//! stderr ↔ 机读 message）+ `--selftest` 不收 `--json`（用法错 exit 2）。
//! Python 缺席打 skip（CI 净仓面——src/json.rs 纯面钉板兜底）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn run(cmd: &str, args: &[String], cwd: &Path) -> (i32, String, String) {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("spawn");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn with_json(mut args: Vec<String>) -> Vec<String> {
    args.insert(0, "--json".to_string());
    args
}

fn run_py_json(args: &[String], cwd: &Path) -> (i32, String, String) {
    let mut a = vec![repo_root().join("tools/mkapp.py").to_string_lossy().into_owned()];
    a.extend_from_slice(&with_json(args.to_vec()));
    run("python3", &a, cwd)
}

fn run_rs_json(args: &[String], cwd: &Path) -> (i32, String, String) {
    let a = with_json(args.to_vec());
    run(env!("CARGO_BIN_EXE_mkapp"), &a, cwd)
}

/// `bin_name` 须与 manifest entry 对齐（`bin/jtest`）——否则先触发
/// entry.mismatch 门，到不了目标拒因（stage.bin.count 案 extra_bins 在
/// count 门先拒，不受影响）。
fn mkstage(base: &Path, name: &str, manifest: &str, bin_data: &[u8], extra_bins: &[&str]) -> PathBuf {
    let d = base.join(name);
    fs::create_dir_all(d.join("bin")).unwrap();
    fs::write(d.join("bin").join("jtest"), bin_data).unwrap();
    for b in extra_bins {
        fs::write(d.join("bin").join(b), b"\x7fELF").unwrap();
    }
    fs::write(d.join("manifest.toml"), manifest).unwrap();
    d
}

/// 负例拒因信封七舱断言（纯 std）：从单行信封文本剥 `"error":{...}` 段，
/// 逐舱子串断言。Rust 与 Python 信封剥路径段（tempdir 实例化）后比对。
fn err_field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\":");
    let i = text.find(&pat)? + pat.len();
    let rest = &text[i..];
    // 值段 = 到下一个 `"键":` 或 `}`（嵌套仅一层 list——intent 词表）
    let end = rest.find("\",\"").map(|e| e + 1).or_else(|| rest.find('}'))?;
    Some(&rest[..end])
}

const JM: &str = "[app]\nid = \"JTEST\"\nversion = \"0.1.0\"\nentry = \"bin/jtest\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = true\n";

#[test]
fn json_parity_oracle() {
    let root = repo_root();
    if !root.join("tools/mkapp.py").exists() || Command::new("python3").arg("--version").output().is_err() {
        eprintln!("mkapp: tools/mkapp.py/python3 缺席——CLI 对拍 skip（纯面钉板兜底）");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("mkapp-json-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    // ---- 正例：build 成功信封（含 compat/fs 条件键序）----
    let jm_full = JM
        .replace("console = true\n", "console = true\nfs = [\"rootfs\"]\n")
        .replace("\n[capabilities]", "\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\n\n[capabilities]");
    let stage = mkstage(&tmp, "jtest", &jm_full, b"\x7fELF", &[]);
    let out_r = tmp.join("r").join("JTEST.bap");
    let out_p = tmp.join("p").join("JTEST.bap");
    fs::create_dir_all(out_r.parent().unwrap()).unwrap();
    fs::create_dir_all(out_p.parent().unwrap()).unwrap();
    let args_r = vec![out_r.to_string_lossy().into_owned(), stage.to_string_lossy().into_owned()];
    let args_p = vec![out_p.to_string_lossy().into_owned(), stage.to_string_lossy().into_owned()];
    let (rrc, rout, rerr) = run_rs_json(&args_r, &root);
    let (prc, pout, perr) = run_py_json(&args_p, &root);
    assert_eq!(rrc, 0, "Rust --json build 失败：{rerr}");
    assert_eq!(prc, 0, "Python --json build 失败：{perr}");
    // result 舱剥 out 段后逐键全等（out 含 tempdir 路径——两侧分实例）
    let strip_out = |t: &str, p: &str| t.replace(p, "<OUT>");
    let rn = strip_out(&rout, &out_r.to_string_lossy());
    let pn = strip_out(&pout, &out_p.to_string_lossy());
    assert_eq!(rn, pn, "build 信封（剥 out 段）不等：\nRust ={rn}\nPy   ={pn}");
    assert_eq!(rout.lines().count(), 1, "单行纪律：{rout}");
    // 产物恒等（--json build = 产链产物——顺带字节级复验）
    assert_eq!(fs::read(&out_r).unwrap(), fs::read(&out_p).unwrap(), "--json build 产物不等");

    // ---- 正例：check 成功信封 ----
    let cr = vec!["--check".to_string(), out_r.to_string_lossy().into_owned()];
    let cp = vec!["--check".to_string(), out_p.to_string_lossy().into_owned()];
    let (rrc, rout, _) = run_rs_json(&cr, &root);
    let (prc, pout, _) = run_py_json(&cp, &root);
    assert_eq!((rrc, prc), (0, 0));
    let rn = strip_out(&rout, &out_r.to_string_lossy());
    let pn = strip_out(&pout, &out_p.to_string_lossy());
    assert_eq!(rn, pn, "check 信封（剥 path 段）不等：\nRust ={rn}\nPy   ={pn}");

    // ---- 负例七类钉板族：拒因信封七舱全等 ----
    fs::create_dir_all(tmp.join("reject")).unwrap();
    let cases: Vec<(&str, String, Vec<&str>, &[u8])> = vec![
        ("manifest.syntax.toml", JM.replace("entry = \"bin/jtest\"", "entry == \"bin/jtest\""), vec![], b"\x7fELF"),
        ("manifest.app.id.length", JM.replace("id = \"JTEST\"", "id = \"\""), vec![], b"\x7fELF"),
        ("manifest.intent.vocab", JM.replace("intent = \"utility\"", "intent = \"urgent\""), vec![], b"\x7fELF"),
        ("stage.bin.count", JM.to_string(), vec!["second"], b"\x7fELF"),
        ("entry.mismatch", JM.replace("entry = \"bin/jtest\"", "entry = \"bin/other\""), vec![], b"\x7fELF"),
        ("bin.elf", JM.to_string(), vec![], b"NOEL"),
    ];
    for (i, (want_code, mf, extra, bin_data)) in cases.iter().enumerate() {
        let stage = mkstage(&tmp, &format!("neg{i}"), mf, bin_data, extra);
        let out1 = tmp.join("reject").join(format!("r{i}").as_str()).join("JTEST.bap");
        let out2 = tmp.join("reject").join(format!("p{i}").as_str()).join("JTEST.bap");
        fs::create_dir_all(out1.parent().unwrap()).unwrap();
        fs::create_dir_all(out2.parent().unwrap()).unwrap();
        let a1 = vec![out1.to_string_lossy().into_owned(), stage.to_string_lossy().into_owned()];
        let a2 = vec![out2.to_string_lossy().into_owned(), stage.to_string_lossy().into_owned()];
        let (rrc, rout, rerr) = run_rs_json(&a1, &root);
        let (prc, pout, _) = run_py_json(&a2, &root);
        assert_eq!((rrc, prc), (1, 1), "{want_code}：rc {rrc}≠{prc}；rerr={rerr}");
        // 结构舱断言（Python oracle 钉板值——err_field 通用解析在 message
        // 含逗号时截断，故七舱用逐类期望子串断言；Rust/Python 双侧同断）：
        // [code|where|line|expected|got|fix] 六结构舱 + 单行纪律。
        for t in [(&rout, "Rust"), (&pout, "Py")] {
            let (text, side) = t;
            assert!(text.contains(&format!("\"code\":\"{want_code}\"")), "{side} code 对号：{text}");
            assert_eq!(text.lines().count(), 1, "{side} 单行纪律");
            // 恒 7 键（error 舱）
            for key in ["code", "where", "line", "expected", "got", "fix", "message"] {
                assert!(text.contains(&format!("\"{key}\":")), "{side} 恒 7 键缺 {key}：{text}");
            }
        }
        // 逐类钉板（Python oracle 真跑结出的期望——两侧同构断言）
        let pin: &[(&str, &str)] = match *want_code {
            "manifest.syntax.toml" => &[("\"line\":4", "语法层 line int"), ("\"expected\":null", "expected null"), ("\"got\":null", "got null")],
            "manifest.app.id.length" => &[("\"expected\":\"1..=24\"", "区间串"), ("\"got\":0", "got int")],
            "manifest.intent.vocab" => &[("\"expected\":[\"unspecified\",\"latency_critical\",\"interactive\",\"utility\",\"background\",\"maintenance\"]", "词表 list"), ("\"got\":\"urgent\"", "got 串")],
            "stage.bin.count" => &[("\"expected\":1", "expected int"), ("\"got\":2", "got int"), ("\"where\":\"bin/\"", "where 目录")],
            "entry.mismatch" => &[("\"expected\":\"bin/jtest\"", "expected 条目"), ("\"got\":\"bin/other\"", "got entry")],
            "bin.elf" => &[("\"expected\":\"7f454c46\"", "expected hex"), ("\"got\":\"4e4f454c\"", "got hex")],
            _ => &[],
        };
        for t in [(&rout, "Rust"), (&pout, "Py")] {
            let (text, side) = t;
            for (frag, why) in pin {
                assert!(text.contains(frag), "{want_code} {side} {why}：\n{text}");
            }
        }
        // message 舱双读同构（syntax 类 tomllib 文案差异 / bin.elf Py 侧
        // 绝对路径 + bytes repr 形——跳过；结构舱 six-pin 已双侧钉死）
        if !matches!(*want_code, "manifest.syntax.toml" | "bin.elf") {
            let rv = err_field(&rout, "message").unwrap_or("null");
            let pv = err_field(&pout, "message").unwrap_or("null");
            assert_eq!(rv, pv, "{want_code} message 不等：\nRust  ={rout}\nPy    ={pout}");
        }
    }

    // ---- io 兜底：no-such-dir 输出位 ----
    let bad_out = tmp.join("no-such-dir").join("JTEST.bap");
    let stage = mkstage(&tmp, "iook", JM, b"\x7fELF", &[]);
    let a = vec![bad_out.to_string_lossy().into_owned(), stage.to_string_lossy().into_owned()];
    let (rrc, rout, _) = run_rs_json(&a, &root);
    let (prc, pout, _) = run_py_json(&a, &root);
    assert_eq!((rrc, prc), (1, 1));
    // Python OSError 兜底 code="io"；Rust io 站点 code="io.read"（前缀对齐）
    assert!(rout.contains("\"code\":\"io."), "Rust io code：{rout}");
    assert!(pout.contains("\"code\":\"io\""), "Python io code：{pout}");

    // ---- 单行纪律 + 双读一致 ----
    let stage_bad = mkstage(&tmp, "dual", &JM.replace("intent = \"utility\"", "intent = \"urgent\""), b"\x7fELF", &[]);
    fs::create_dir_all(tmp.join("dual")).unwrap();
    let out_d = tmp.join("dual").join("D.bap");
    let a = vec![out_d.to_string_lossy().into_owned(), stage_bad.to_string_lossy().into_owned()];
    let (jrc, jout, jerr) = run_rs_json(&a, &root);
    let hargs = a.clone();
    let (hrc, _, herr) = run(env!("CARGO_BIN_EXE_mkapp"), &hargs, &root);
    assert_eq!((jrc, hrc), (1, 1));
    assert!(jerr.is_empty(), "json 模式 stderr 空：{jerr}");
    assert_eq!(jout.lines().count(), 1);
    let msg = err_field(&jout, "message").unwrap().trim_matches('"');
    assert_eq!(herr, format!("mkapp: {msg}\n"), "双读一致：{herr}");

    // ---- --selftest 不收 --json（用法错 exit 2）----
    let (rc, out, _) = run_rs_json(&["--selftest".to_string()], &root);
    assert_eq!(rc, 2, "rc={rc} out={out}");
    assert!(out.is_empty(), "用法错 stdout 须空：{out}");

    let _ = fs::remove_dir_all(&tmp);
}
