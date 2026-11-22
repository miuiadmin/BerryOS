//! bctsrun 四刀·parity：abi_level / img_pr_check 双件 Python 现役件
//! （tools/bcts/abi_level.py + tools/bcts/img_pr_check.py）真跑 oracle 对拍。
//!
//! 三面：
//! ①abi_level：仓内真源 tools/abi.toml 双侧 stdout 全等 + 变异族（值域拒面
//!   逐字 / OS Errno 面 / 坏 TOML 拒分支对齐）；
//! ②img_pr_check：ok 面（真 manifest 形 + 前导零归一 + 锚透传）+ 拒面族
//!   （混杂 sorted list repr / 无条目行 / 不等拒长文案 / OS Errno 面）——
//!   stderr 逐字全等 + 退码；
//! ③正则边界族：贪心最右位 / 紧贴形 / CRLF / 11 位回溯失败——stdout·exit
//!   双侧同构（py 不匹配 = 走「无条目行」拒，同文案）。
//!
//! Python 缺席 = skip（t1/t2/reconcile parity 同款守卫）。

use bctsrun::{abi_level, img_pr_check};
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

/// abi_level.py 真跑（同参 CLI 面：argv[1] = abi.toml 路径）。
fn py_abi_level(path: &str) -> (String, String, Option<i32>) {
    let o = Command::new("python3")
        .arg(repo_root().join("tools/bcts/abi_level.py"))
        .arg(path)
        .output()
        .expect("spawn python3");
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code(),
    )
}

/// img_pr_check.py 真跑（argv = manifest, want, anchor）。
fn py_img_pr(manifest: &str, want: i64, anchor: &str) -> (String, String, Option<i32>) {
    let o = Command::new("python3")
        .arg(repo_root().join("tools/bcts/img_pr_check.py"))
        .arg(manifest)
        .arg(want.to_string())
        .arg(anchor)
        .output()
        .expect("spawn python3");
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code(),
    )
}

#[test]
fn abilvl_real_source_and_mutants_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-abilpar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // ①真源：双侧 stdout 全等
    let root = repo_root().join("tools/abi.toml");
    let root_str = root.to_str().unwrap();
    let (out, err, code) = py_abi_level(root_str);
    assert_eq!(code, Some(0), "py 真源应绿：{err}");
    let lv = abi_level(root_str).unwrap();
    assert_eq!(out, format!("{lv}\n"));
    // ②值域变异族：文案逐字 + 退码
    let mutants: Vec<(&str, &str)> = vec![
        ("[platform]\nlevel = \"1\"\n", "非整数：'1'"),
        ("[platform]\nlevel = '1'\n", "非整数：'1'"),
        ("[platform]\nlevel = true\n", "非整数：True"),
        ("[platform]\nlevel = false\n", "非整数：False"),
        ("[platform]\nlevel = 1.5\n", "非整数：1.5"),
        ("[platform]\nlevel = 2.0\n", "非整数：2.0"),
        ("[meta]\nstage = \"v1\"\n", "非整数：None"),
        ("[platform]\ndesc = \"x\"\n", "非整数：None"),
    ];
    let mut n = 0;
    for (i, (text, _kw)) in mutants.iter().enumerate() {
        let p = dir.join(format!("m{i}.toml"));
        std::fs::write(&p, text).unwrap();
        let p_str = p.to_str().unwrap();
        let (out, err, code) = py_abi_level(p_str);
        assert_eq!(code, Some(1), "[{i}] py 应拒：{err}");
        assert!(out.is_empty());
        let py_first = err.lines().next().unwrap_or("").to_string();
        let rust_err = abi_level(p_str).expect_err("rust 应拒");
        assert_eq!(rust_err, py_first, "[{i}] 拒面文案不等");
        n += 1;
    }
    // ③负 int 对照组：py isinstance(int) 收纳（stdout 同形）
    let p = dir.join("neg.toml");
    std::fs::write(&p, "[platform]\nlevel = -3\n").unwrap();
    let p_str = p.to_str().unwrap();
    let (out, _err, code) = py_abi_level(p_str);
    assert_eq!(code, Some(0));
    assert_eq!(out, "-3\n");
    assert_eq!(abi_level(p_str).unwrap(), -3);
    // ④OS Errno 面：缺席 / 目录
    let absent = dir.join("nope.toml").to_str().unwrap().to_string();
    let (out, err, code) = py_abi_level(&absent);
    assert_eq!(code, Some(1));
    assert!(out.is_empty());
    let py_first = err.lines().next().unwrap_or("").to_string();
    let rust_err = abi_level(&absent).unwrap_err();
    assert_eq!(rust_err, py_first, "Errno 文案不等");
    let dir_str = dir.to_str().unwrap().to_string();
    let (out, err, code) = py_abi_level(&dir_str);
    assert_eq!(code, Some(1));
    assert!(out.is_empty());
    let py_first = err.lines().next().unwrap_or("").to_string();
    let rust_err = abi_level(&dir_str).unwrap_err();
    assert_eq!(rust_err, py_first, "Errno 文案不等（目录）");
    // ⑤坏 TOML：双侧同拒退 1（语法域文案不同源——分支对齐）
    let p = dir.join("bad.toml");
    std::fs::write(&p, "[platform]\nlevel = [1]\n").unwrap();
    let p_str = p.to_str().unwrap();
    let (_out, err, code) = py_abi_level(p_str);
    assert_eq!(code, Some(1), "py 坏 TOML 应拒：{err}");
    assert!(abi_level(p_str).is_err());
    eprintln!("abi_level parity: 1 real + {n} mutants + neg/errno/badtoml OK");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn imgpr_ok_and_rejects_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-imgprpar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ok_manifest = "berryos-manifest v1 format_frozen=2026-11-10 entries=04\n\
        item path=/EFI/BOOT/BOOTAA64.EFI   sha256=8387 platform_revision=0000000001\n\
        item path=/HELLO.TXT               sha256=e585 platform_revision=0000000001\n";
    let p = dir.join("ok.txt");
    std::fs::write(&p, ok_manifest).unwrap();
    let p_str = p.to_str().unwrap();
    // ①ok 面：stdout 逐字 + 退 0（前导零按值归一 + 锚透传）
    let (out, err, code) = py_img_pr(p_str, 1, "冻结件 platform_revision（bcts@0.toml）");
    assert_eq!(code, Some(0), "py 应绿：{err}");
    let rust = img_pr_check(p_str, 1, "冻结件 platform_revision（bcts@0.toml）").unwrap();
    assert_eq!(rust, out.trim_end_matches('\n'));
    // ②不等拒（长文案逐字）
    let (out, err, code) = py_img_pr(p_str, 2, "锚A");
    assert_eq!(code, Some(1));
    assert!(out.is_empty());
    let py_first = err.lines().next().unwrap_or("").to_string();
    let rust_err = img_pr_check(p_str, 2, "锚A").unwrap_err();
    assert_eq!(rust_err, py_first, "不等拒文案不等");
    // ③混杂（set 去重 → sorted 升序 [1, 10]）
    let mixed = ok_manifest.replace(
        "item path=/HELLO.TXT               sha256=e585 platform_revision=0000000001",
        "item path=/HELLO.TXT               sha256=e585 platform_revision=0000000010",
    );
    let p2 = dir.join("mixed.txt");
    std::fs::write(&p2, &mixed).unwrap();
    let p2_str = p2.to_str().unwrap();
    let (out, err, code) = py_img_pr(p2_str, 1, "锚");
    assert_eq!(code, Some(1));
    assert!(out.is_empty());
    let py_first = err.lines().next().unwrap_or("").to_string();
    let rust_err = img_pr_check(p2_str, 1, "锚").unwrap_err();
    assert_eq!(rust_err, py_first, "混杂拒文案不等");
    // ④无条目行（含 11 位回溯失败腿：py 正则不匹配 → 同拒）
    let p3 = dir.join("empty.txt");
    std::fs::write(&p3, "berryos-manifest v1\nno item rows\n").unwrap();
    let p3_str = p3.to_str().unwrap();
    let (_out, err, code) = py_img_pr(p3_str, 1, "锚");
    assert_eq!(code, Some(1));
    let py_first = err.lines().next().unwrap_or("").to_string();
    let rust_err = img_pr_check(p3_str, 1, "锚").unwrap_err();
    assert_eq!(rust_err, py_first, "无条目拒文案不等");
    let p4 = dir.join("eleven.txt");
    std::fs::write(&p4, "item path=/X sha256=ab platform_revision=00000000011\n").unwrap();
    let p4_str = p4.to_str().unwrap();
    let (_out, err, code) = py_img_pr(p4_str, 1, "锚");
    assert_eq!(code, Some(1), "py 11 位回溯失败应走无条目拒：{err}");
    let py_first = err.lines().next().unwrap_or("").to_string();
    let rust_err = img_pr_check(p4_str, 1, "锚").unwrap_err();
    assert_eq!(rust_err, py_first, "11 位腿文案不等");
    // ⑤读失败面（Errno 逐字）
    let absent = dir.join("nope.txt").to_str().unwrap().to_string();
    let (out, err, code) = py_img_pr(&absent, 1, "锚");
    assert_eq!(code, Some(1));
    assert!(out.is_empty());
    let py_first = err.lines().next().unwrap_or("").to_string();
    let rust_err = img_pr_check(&absent, 1, "锚").unwrap_err();
    assert_eq!(rust_err, py_first, "Errno 文案不等");
    eprintln!("img_pr_check parity: ok/不等/混杂/无条目/11位/Errno OK");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn imgpr_regex_edge_parity() {
    if !py_ok() {
        eprintln!("skip: python3 absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-imgpredge-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 正则边界族（每腿 = 独立 manifest 文件，py/rust 同参对拍 stdout+exit+stderr）
    let faces: Vec<(&str, &str, i64)> = vec![
        // 贪心最右位（左位右有余文不可行）
        ("item platform_revision=0000000002 platform_revision=0000000001\n", "greedy", 1),
        // item 后紧贴无空格：needle 含前置空格 → 不匹配 → 无条目拒
        ("item platform_revision=0000000001\n", "tight", 1),
        // 双空格形命中
        ("item  platform_revision=0000000001\n", "dblsp", 1),
        // CRLF：py open() 文本读 universal newlines 归一 → 匹配 ok
        ("item path=/X sha256=ab platform_revision=0000000001\r\n", "crlf", 1),
        // 多行混合：一行命中一行不匹配
        ("item path=/A platform_revision=0000000003\nnot-item platform_revision=0000000009\n", "multi", 3),
    ];
    let mut n = 0;
    for (text, tag, want) in &faces {
        let p = dir.join(format!("{tag}.txt"));
        std::fs::write(&p, text).unwrap();
        let p_str = p.to_str().unwrap();
        let (out, err, code) = py_img_pr(p_str, *want, "锚");
        let py_first = err.lines().next().unwrap_or("").to_string();
        match code {
            Some(0) => {
                let rust = img_pr_check(p_str, *want, "锚").unwrap();
                assert_eq!(rust, out.trim_end_matches('\n'), "[{tag}] ok 行不等");
                n += 1;
            }
            Some(1) => {
                let rust_err = img_pr_check(p_str, *want, "锚").unwrap_err();
                assert_eq!(rust_err, py_first, "[{tag}] 拒面不等");
                n += 1;
            }
            other => panic!("[{tag}] py 意外退码 {other:?}：{out}{err}"),
        }
    }
    eprintln!("img_pr regex edge parity: {n} faces OK");
    std::fs::remove_dir_all(&dir).ok();
}
