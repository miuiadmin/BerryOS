//! bctsrun 八刀·parity：步③ host 门禁段对拍。oracle = bash 真跑——
//! bcts-run.sh :631-647 循环体逐字（`while IFS=$'\t' read -r tag gid cmd`
//! + `bash -c "$cmd" > log 2>&1` + PASS/FAIL 行 + `tail -5` + GATE_SUMMARY
//! `；` 连接），驱动面 = 合成 entries.tsv（拒门禁族 + 过门禁族 + 混排）。
//!
//! 对拍面：stdout/stderr 逐行全等 + summary 串全等 + 总判定全等。真源
//! GATE（`just abi-check`）不在此跑——步③在 --dry 之后、步④ QEMU 之前，
//! bcts-run.sh 无干跑停点（--dry 在步③前 exit 0），故 oracle = 循环体
//! 逐字提取（首刀 t1_check 同款「heredoc 体真跑」纪律——段体即单源）。
//!
//! bash 缺席 = skip（parity 先例同款守卫）。

use bctsrun::load::LoadOutcome;
use bctsrun::gates::{gate_log_name, host_gates};
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn sh_ok() -> bool {
    Command::new("bash")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// oracle：bcts-run.sh 步③循环体逐字（entries.tsv 路径入参）。
/// 返回 (stdout 逐行, stderr 逐行, summary, rc)。
fn oracle_gate_loop(tsv: &str, work: &str) -> (Vec<String>, Vec<String>, String, i32) {
    // :631-647 逐字（$WORK → 入参 work；entries.tsv 绝对形）。
    let script = format!(
        r#"GATE_SUMMARY=""
GATE_FAIL=0
while IFS=$'\t' read -r tag gid cmd; do
  [ "$tag" = GATE ] || continue
  echo "bcts: host 门禁 [$gid]：$cmd"
  if bash -c "$cmd" > "{work}/gate-$gid.log" 2>&1; then
    echo "bcts: host 门禁 [$gid] PASS"
    GATE_SUMMARY="${{GATE_SUMMARY}}${{GATE_SUMMARY:+；}}${{gid}}: PASS"
  else
    echo "bcts: FAIL: host 门禁 [$gid] 拒（${{cmd}}）；日志尾 5 行：" >&2
    tail -5 "{work}/gate-$gid.log" >&2
    GATE_SUMMARY="${{GATE_SUMMARY}}${{GATE_SUMMARY:+；}}${{gid}}: FAIL"
    GATE_FAIL=1
  fi
done < "{tsv}"
echo "SUMMARY:$GATE_SUMMARY"
[ "$GATE_FAIL" = 0 ] || exit 1
exit 0
"#
    );
    let o = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .current_dir(repo_root())
        .output()
        .expect("spawn bash oracle");
    let mut out: Vec<String> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect();
    let err: Vec<String> = String::from_utf8_lossy(&o.stderr)
        .lines()
        .map(|l| l.to_string())
        .collect();
    let mut summary = String::new();
    if let Some(last) = out.last().and_then(|l| l.strip_prefix("SUMMARY:")) {
        summary = last.to_string();
        out.pop();
    }
    (out, err, summary, o.status.code().unwrap_or(-1))
}

fn make_lo(gates: &[(&str, &str)]) -> LoadOutcome {
    LoadOutcome {
        entries: gates
            .iter()
            .map(|(g, c)| format!("GATE\t{g}\t{c}"))
            .collect(),
        n_markers: 0,
        n_gates: gates.len(),
        floor: 91,
        n_bare: 0,
        img_pr: None,
        img_pr_anchor: None,
        pre_stdout: vec![],
    }
}

/// 双跑对拍（合成门禁族 → oracle vs 镜像）。
fn parity(gates: &[(&str, &str)], tag: &str) -> bctsrun::gates::GateOutcome {
    let root = repo_root();
    let work = root.join(format!("target/bctsrun-parity-gates-{tag}"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let tsv = work.join("entries.tsv");
    let body: String = gates
        .iter()
        .map(|(g, c)| format!("GATE\t{g}\t{c}\n"))
        .collect();
    std::fs::write(&tsv, body).unwrap();
    let tsv_str = tsv.to_str().unwrap().to_string();
    let work_str = work.to_str().unwrap().to_string();

    let (bout, berr, bsum, brc) = oracle_gate_loop(&tsv_str, &work_str);
    let g = host_gates(&make_lo(gates), root.to_str().unwrap(), &work);

    assert_eq!(g.stdout, bout, "stdout 不等（{tag}）\nbash={bout:?}\nrust={:?}", g.stdout);
    assert_eq!(g.stderr, berr, "stderr 不等（{tag}）\nbash={berr:?}\nrust={:?}", g.stderr);
    assert_eq!(g.summary, bsum, "summary 不等（{tag}）");
    assert_eq!(g.ok, brc == 0, "总判定不等（{tag}）：bash rc={brc}");
    let _ = std::fs::remove_dir_all(&work);
    g
}

#[test]
fn parity_pass_family() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 单过门禁 + 真源 cmd 词面（不真跑 just——词面进 stdout 行即面）。
    parity(&[("abi-check", "true")], "pass1");
    // 多门禁全过：summary 连接律（含 cmd 内含分号——read -r 三域取行余量）。
    parity(
        &[("g1", "true"), ("g2", "echo ok; true"), ("g3", "exit 0")],
        "pass3",
    );
}

#[test]
fn parity_fail_family() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 拒门禁 + 6 行日志（tail -5 挤出首行）。
    parity(
        &[("f1", "echo a; echo b; echo c; echo d; echo e; echo f; exit 3")],
        "fail6",
    );
    // 短日志（不足 5 全发）+ 空日志（零尾行）。
    parity(&[("f2", "echo x; exit 1")], "failshort");
    parity(&[("f3", "exit 7")], "failempty");
    // 双流归日志（stderr 侧证据）。
    parity(&[("f4", "echo out1; echo err1 >&2; echo out2; false")], "failstreams");
}

#[test]
fn parity_mixed_no_shortcut() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 混排：拒后继续跑（非短路）+ summary 三段连接。
    parity(
        &[("p1", "true"), ("p2", "false"), ("p3", "true")],
        "mixed",
    );
}

#[test]
fn parity_zero_gates() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 零 GATE 行（META/MARK 在场）：oracle while 循环零迭代。
    let g = parity(&[], "zero");
    assert_eq!(g.summary, "");
    assert!(g.ok);
}

#[test]
fn parity_gate_log_name_shape() {
    // 日志文件名词面（bash > "$WORK/gate-$gid.log" 同形）。
    assert_eq!(gate_log_name("abi-check"), "gate-abi-check.log");
}

#[test]
fn parity_real_frozen_gate_rows_parsed() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 真源装载链：bcts@6 冻结件 → load_entries → GATE 行进 host_gates 面
    // （真 cmd `just abi-check` 不真跑——以 echo 替身镜像 cmd 消费位，证
    // 装载链 GATE 行 → 步③入口的接管面）。
    let root = repo_root();
    let repo = root.to_string_lossy().into_owned();
    let argv = vec![
        "bcts-run.sh".to_string(),
        "--dry".to_string(),
    ];
    let r = bctsrun::resolve_args(&argv, &repo, Some("tools/bcts/frozen/bcts@6.toml"))
        .expect("resolve 应过");
    let o = bctsrun::load_entries(&r, &repo).expect("装载应过");
    assert_eq!(o.n_gates, 1, "bcts@6 host_gate 恰一门");
    let gate_rows: Vec<&String> = o
        .entries
        .iter()
        .filter(|l| l.starts_with("GATE\t"))
        .collect();
    assert_eq!(gate_rows.len(), 1);
    assert_eq!(gate_rows[0], "GATE\tabi-check\tjust abi-check");
    // 替身 cmd（真跑面 = just abi-check 全链，不在此复跑——四门已在别腿）。
    let work = root.join("target/bctsrun-parity-gates-real");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let mut o2 = o.clone();
    o2.entries = vec!["GATE\tabi-check\ttrue".into()];
    let g = host_gates(&o2, &repo, &work);
    assert_eq!(
        g.stdout,
        vec![
            "bcts: host 门禁 [abi-check]：true",
            "bcts: host 门禁 [abi-check] PASS"
        ]
    );
    assert_eq!(g.summary, "abi-check: PASS");
    assert!(g.ok);
    let _ = std::fs::remove_dir_all(&work);
}
