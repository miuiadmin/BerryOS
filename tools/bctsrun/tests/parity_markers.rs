//! bctsrun 九刀·parity：步④ 标记对拍 + 对账循环 oracle 对拍。oracle =
//! bash 真跑——`bcts-run.sh` :655-660 `assert_marker` 函数体 + :756-776
//! 对账循环体逐字（`CORPUS` 入参合成语料；`IMAGE_FILE` 空/非空两态），
//! 驱动面 = 合成 entries.tsv（全过/缺件/镜像腿 bare 子面/DEMO·TAB 前邻特判
//! /短行）。
//!
//! 对拍面：`PASS_N` / `MISSING` / `FACE_N` / `VERDICT` 四变量全等（经尾行
//! `RC:pass|missing|face|verdict` 带出）。`assert_marker` 单函数另以逐条
//! 判定矩阵对拍（`AM:1`/`AM:0` 行）。
//!
//! bash 缺席 = skip（parity 先例同款守卫）。段体单源 = 文件行号提取，行号
//! 漂移即测试红（七/八刀先例：`sed -n` 从真文件取段，防双写）。

use bctsrun::load::LoadOutcome;
use bctsrun::markers::{assert_marker, reconcile};
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

/// oracle：`assert_marker` 函数体（:655-660 逐字，`$CORPUS` 入参）。
fn oracle_assert_script() -> String {
    let src = std::fs::read_to_string(repo_root().join("tools/bcts-run.sh")).unwrap();
    // 函数体定位：`assert_marker() {` 起至首个独立 `}` 行止（逐字提取——
    // 行号漂移免疫，段体即单源）。
    let lines: Vec<&str> = src.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with("assert_marker() {"))
        .expect("bcts-run.sh 缺 assert_marker 函数");
    let end = lines[start..]
        .iter()
        .position(|l| *l == "}")
        .expect("assert_marker 函数未闭合")
        + start;
    lines[start..=end].join("\n")
}

/// oracle：对账循环体（:756-776 逐字，`$WORK/entries.tsv` → 入参；`$IMAGE_FILE` 入态）。
fn oracle_reconcile_script() -> String {
    let src = std::fs::read_to_string(repo_root().join("tools/bcts-run.sh")).unwrap();
    let lines: Vec<&str> = src.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == "PASS_N=0")
        .expect("bcts-run.sh 缺对账循环 PASS_N=0");
    // 终点 = `FACE_N=$N_MARKERS` 行自身（其后的 `fi` 不属循环体——`FACE_N`
    // 赋值在 `if … fi` 块内，`fi` 后紧跟 `# ---- 步⑤` 注释行）。
    let end = lines[start..]
        .iter()
        .position(|l| *l == "  FACE_N=$N_MARKERS")
        .expect("bcts-run.sh 缺 FACE_N 行")
        + start;
    assert_eq!(lines[end + 1], "fi", "对账循环尾形漂移");
    lines[start..=end + 1].join("\n")
}

#[test]
fn extracted_segments_have_expected_shape() {
    // 段体提取形状钉板（防定位锚漂移后取到错段而静默假绿）。
    let am = oracle_assert_script();
    assert!(am.starts_with("assert_marker() {"));
    assert!(am.contains("\"DEMO ok\") grep -qE '(^|[^I])DEMO ok'"));
    assert!(am.contains("\"TAB ok\")  grep -qE '(^|[^G])TAB ok'"));
    assert!(am.trim_end().ends_with("}"));
    let rec = oracle_reconcile_script();
    assert!(rec.starts_with("PASS_N=0"));
    assert!(rec.contains("MISSING=\"${MISSING:+$MISSING }$mid\""));
    assert!(rec.contains("FACE_N=$N_BARE"));
    assert!(rec.contains("FACE_N=$N_MARKERS"));
    assert!(rec.trim_end().ends_with("fi"));
}

fn make_lo(rows: &[&str], n_markers: usize, n_bare: usize) -> LoadOutcome {
    LoadOutcome {
        entries: rows.iter().map(|s| s.to_string()).collect(),
        n_markers,
        n_gates: 0,
        floor: 91,
        n_bare,
        img_pr: None,
        img_pr_anchor: None,
        pre_stdout: vec![],
    }
}

/// 双跑对拍：bash 循环体 vs 镜像。返回镜像出口供断言。
fn parity(rows: &[&str], corpus: &str, image: bool, n_markers: usize, n_bare: usize, tag: &str) -> bctsrun::markers::ReconcileOutcome {
    let root = repo_root();
    let work = root.join(format!("target/bctsrun-parity-markers-{tag}"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let tsv = work.join("entries.tsv");
    std::fs::write(&tsv, rows.iter().map(|r| format!("{r}\n")).collect::<String>()).unwrap();
    let corpus_path = work.join("observed.log");
    std::fs::write(&corpus_path, corpus).unwrap();
    let tsv_s = tsv.to_str().unwrap();
    let corpus_s = corpus_path.to_str().unwrap();

    // VERDICT 两行（:801-802）逐字——对账循环 `fi` 后紧邻，不在循环体内，随刀
    // 取自真文件（定位 = `VERDICT=FAIL` 行起两行；漂移即形状断言红）。
    let src = std::fs::read_to_string(repo_root().join("tools/bcts-run.sh")).unwrap();
    let lines: Vec<&str> = src.lines().collect();
    let vi = lines.iter().position(|l| *l == "VERDICT=FAIL").expect("bcts-run.sh 缺 VERDICT=FAIL 行");
    assert_eq!(lines[vi + 1], "[ -z \"$MISSING\" ] && VERDICT=PASS", "VERDICT 判定行漂移");
    let verdict_block = format!("{}\n{}", lines[vi], lines[vi + 1]);

    // bash 循环体逐字前置变量：N_MARKERS/N_BARE/IMAGE_FILE 入态。
    let image_lit = if image { "\"/img/x.img\"" } else { "" };
    let script = format!(
        "N_MARKERS={n_markers}\nN_BARE={n_bare}\nIMAGE_FILE={image_lit}\nCORPUS=\"{corpus_s}\"\nWORK=\"{}\"\n{}\n{}\n{}\necho \"RC:$PASS_N|$MISSING|$FACE_N|$VERDICT\"\nexit 0\n",
        work.to_str().unwrap(),
        oracle_assert_script(),
        oracle_reconcile_script()
            .replace("done < \"$WORK/entries.tsv\"", &format!("done < \"{tsv_s}\"")),
        verdict_block,
    );
    let o = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .current_dir(&root)
        .output()
        .expect("spawn bash oracle");
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "bash oracle 非零退（{tag}）：rc={:?} stderr={err}", o.status.code());
    let rc_line = out.lines().rev().find(|l| l.starts_with("RC:")).expect("oracle 缺 RC: 行");
    let body = rc_line.trim_start_matches("RC:");
    let f: Vec<&str> = body.splitn(4, '|').collect();
    assert_eq!(f.len(), 4, "RC 行域数（{tag}）：{body}");
    let (b_pass, b_missing, b_face, b_verdict): (usize, &str, usize, &str) =
        (f[0].parse().unwrap(), f[1], f[2].parse().unwrap(), f[3]);

    let r = reconcile(&make_lo(rows, n_markers, n_bare), corpus, image);
    assert_eq!(r.pass_n, b_pass, "PASS_N 不等（{tag}）");
    assert_eq!(r.missing, b_missing, "MISSING 不等（{tag}）：bash={b_missing:?} rust={:?}", r.missing);
    assert_eq!(r.face_n, b_face, "FACE_N 不等（{tag}）");
    assert_eq!(r.verdict, b_verdict, "VERDICT 不等（{tag}）");
    let _ = std::fs::remove_dir_all(&work);
    r
}

#[test]
fn parity_assert_marker_matrix() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 逐条判定矩阵：语料 × expect → bash grep 真值 vs 镜像。
    let corpus_cases: Vec<&str> = vec![
        "", "\n", "DEMO ok n=1\n", "IDEMO ok n=1\n", "x DEMO ok\n", "中DEMO ok\n", "DEDEMO ok\n", "IIDEMO ok\n",
        "GTAB ok\n", "TAB ok\n", "ITAB ok\n", "ROOTFS ok vmo=1\nROOTFS ok/absent\n", "foo\nDEMO ok",
        "A ok\r\n", "A ok B ok\n", "B ok\nA ok\n",
    ];
    let expects: Vec<&str> = vec!["DEMO ok", "TAB ok", "ROOTFS ok", "A ok", "B ok", "absent", "vmo=1", "", "A ok\r", "DEMO okk"];
    let root = repo_root();
    let work = root.join("target/bctsrun-parity-markers-am");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let mut n_checked = 0;
    for (ci, c) in corpus_cases.iter().enumerate() {
        let p = work.join(format!("c{ci}.log"));
        std::fs::write(&p, c).unwrap();
        for e in &expects {
            let script = format!(
                "CORPUS=\"{}\"\n{}\nif assert_marker \"{}\"; then echo AM:1; else echo AM:0; fi\nexit 0\n",
                p.to_str().unwrap(),
                oracle_assert_script(),
                e.replace('"', "'\"'\"'")
            );
            let o = Command::new("bash").arg("-c").arg(&script).output().unwrap();
            let out = String::from_utf8_lossy(&o.stdout);
            let b = out.lines().find(|l| l.starts_with("AM:")).expect("oracle 缺 AM: 行");
            let bash_hit = b == "AM:1";
            let rust_hit = assert_marker(e, c);
            assert_eq!(rust_hit, bash_hit, "assert_marker 不等：expect={e:?} corpus={c:?} bash={bash_hit} rust={rust_hit}");
            n_checked += 1;
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    assert!(n_checked >= 100);
}

#[test]
fn parity_reconcile_all_pass_standard() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let rows = ["MARK\tbare\tb-01\tA ok", "MARK\trootfs\tr-01\tB ok", "MARK\trx\tx-01\tC ok"];
    let r = parity(&rows, "A ok\nB ok\nC ok\n", false, 3, 0, "allpass");
    assert_eq!(r.verdict, "PASS");
}

#[test]
fn parity_reconcile_missing_family() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let rows = ["MARK\tbare\tb-01\tA ok", "MARK\tbare\tb-02\tB ok", "MARK\trx\tx-01\tC ok", "MARK\trootfs\tr-01\tD ok"];
    // 缺首/中/尾 + 全缺。
    let r = parity(&rows, "B ok\n", false, 4, 0, "miss1");
    assert_eq!(r.missing, "b-01 x-01 r-01");
    let r = parity(&rows, "", false, 4, 0, "missall");
    assert_eq!(r.missing, "b-01 b-02 x-01 r-01");
    assert_eq!(r.pass_n, 0);
    // 多打不算 fail：语料多余标记照 PASS。
    let r = parity(&rows, "A ok B ok C ok D ok EXTRA ok\n", false, 4, 0, "extra");
    assert_eq!(r.verdict, "PASS");
}

#[test]
fn parity_reconcile_image_leg_bare_subset() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let rows = ["MARK\tbare\tb-01\tA ok", "MARK\tbare\tb-02\tDEMO ok", "MARK\trootfs\tr-01\tB ok", "MARK\trx\tx-01\tC ok"];
    // 镜像腿：FACE_N = N_BARE（入态 2），非 bare 行跳过。
    let r = parity(&rows, "A ok\nDEMO ok\n", true, 4, 2, "img-ok");
    assert_eq!((r.pass_n, r.face_n, r.verdict), (2, 2, "PASS"));
    // 镜像腿 IDEMO 假阳性面：DEMO ok 不算到齐。
    let r = parity(&rows, "A ok\nIDEMO ok\n", true, 4, 2, "img-demo");
    assert_eq!(r.missing, "b-02");
    // 标准流同语料：非 bare 也判 → 缺 r-01 x-01。
    let r = parity(&rows, "A ok\nDEMO ok\n", false, 4, 2, "std-same");
    assert_eq!(r.missing, "r-01 x-01");
    assert_eq!(r.face_n, 4);
}

#[test]
fn parity_reconcile_short_rows_and_gate_rows() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // GATE/META 行跳过；短行（域数不足）bash read 补空——expect 空 grep -qF "" 恒过
    // = bash 假绿面；镜像严拒记缺（分歧注记：见 parity_reconcile_empty_expect_divergence）。
    let rows = ["GATE\tabi-check\tjust abi-check", "MARK\tbare\tb-01\tA ok", "MARK\tbare\tb-02\tB ok"];
    let r = parity(&rows, "A ok\nB ok\n", false, 2, 0, "gaterows");
    assert_eq!(r.verdict, "PASS");
}

#[test]
fn parity_reconcile_empty_expect_divergence() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 分歧钉板：expect 空 → bash `grep -qF ""` 有行即过（假绿）；镜像判缺。
    // 发射器不可达（t1_check `expect 违例（空/…）` + markers.sh 非空断言双门前置），
    // 纯函数面诚实严拒——此测不断言全等，只钉 bash 侧行为 + 镜像侧拒。
    let rows = ["MARK\tbare\tb-01\t"];
    let root = repo_root();
    let work = root.join("target/bctsrun-parity-markers-emptyexp");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let tsv = work.join("entries.tsv");
    std::fs::write(&tsv, "MARK\tbare\tb-01\t\n").unwrap();
    std::fs::write(work.join("observed.log"), "zzz\n").unwrap();
    // VERDICT 两行随刀（parity 同款提取）。
    let src = std::fs::read_to_string(repo_root().join("tools/bcts-run.sh")).unwrap();
    let lines: Vec<&str> = src.lines().collect();
    let vi = lines.iter().position(|l| *l == "VERDICT=FAIL").expect("bcts-run.sh 缺 VERDICT=FAIL 行");
    let verdict_block = format!("{}\n{}", lines[vi], lines[vi + 1]);
    let script = format!(
        "N_MARKERS=1\nN_BARE=0\nIMAGE_FILE=\nCORPUS=\"{}\"\nWORK=\"{}\"\n{}\n{}\n{}\necho \"RC:$PASS_N|$MISSING|$FACE_N|$VERDICT\"\nexit 0\n",
        work.join("observed.log").to_str().unwrap(),
        work.to_str().unwrap(),
        oracle_assert_script(),
        oracle_reconcile_script().replace("done < \"$WORK/entries.tsv\"", &format!("done < \"{}\"", tsv.to_str().unwrap())),
        verdict_block,
    );
    let o = Command::new("bash").arg("-c").arg(&script).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    let rc = out.lines().rev().find(|l| l.starts_with("RC:")).unwrap();
    assert!(rc.contains("|PASS"), "bash 空 expect 假绿面钉板漂移：{rc}");
    let r = reconcile(&make_lo(&rows, 1, 0), "zzz\n", false);
    assert_eq!((r.pass_n, r.verdict), (0, "FAIL"));
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn parity_reconcile_real_live_manifest_against_real_markers_sh() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 真源面：活树清单（markers.sh 经 live 镜像合成）× 合成语料 = 全部 expect 行
    // 原样拼成语料 → 双侧必 PASS 且计数 91/91（DEMO/TAB 前邻特判在行首不误拒）。
    let markers_sh = repo_root().join("tools/markers.sh");
    let text = std::fs::read_to_string(&markers_sh).unwrap();
    let tsv = bctsrun::build_live_tsv(&text, "tools/markers.sh").unwrap();
    let rows: Vec<String> = tsv.lines().filter(|l| l.starts_with("MARK\t")).map(|l| l.to_string()).collect();
    let refs: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
    let corpus: String = rows
        .iter()
        .filter_map(|r| r.split('\t').nth(3))
        .map(|e| format!("{e}\n"))
        .collect();
    let r = parity(&refs, &corpus, false, refs.len(), 0, "reallive");
    assert_eq!(r.verdict, "PASS");
    assert_eq!(r.pass_n, 91);
    assert_eq!(r.face_n, 91);
    // 单标记缺失：去掉一条 expect 行 → 恰缺该 id（双侧同）——取真源首条
    // （`BerryOS boot`——固定串面，不在 DEMO/TAB 特判表）。
    let victim = rows.first().expect("清单非空");
    let vid = victim.split('\t').nth(2).unwrap();
    let corpus2: String = rows
        .iter()
        .filter(|r| *r != victim)
        .filter_map(|r| r.split('\t').nth(3))
        .map(|e| format!("{e}\n"))
        .collect();
    let r2 = parity(&refs, &corpus2, false, refs.len(), 0, "reallive-miss1");
    assert_eq!(r2.missing, vid);
    assert_eq!(r2.pass_n, 90);
}
