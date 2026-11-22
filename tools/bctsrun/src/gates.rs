//! bctsrun 八刀：步③ host 门禁段镜像（现役单源 = `tools/bcts-run.sh`
//! :631-647——Tier 1 前置门禁同跑循环，彻底 Rust 化轨道 b 层 #7 刀序末档
//! 八刀）。
//!
//! 输入 = 七刀 `LoadOutcome.entries`（GATE 行族），行为面逐字镜像：
//! - `while IFS=$'\t' read -r tag gid cmd`：第三域取行余量（内嵌制表符
//!   归 cmd——splitn(3) 同形）；非 GATE 行跳过；
//! - 逐门：stdout「bcts: host 门禁 [$gid]：$cmd」→ `bash -c "$cmd"`
//!   （stdout+stderr 双流重定向 `$WORK/gate-$gid.log`）→ 过「PASS」行 /
//!   拒 stderr「FAIL …日志尾 5 行：」+ `tail -5` 日志尾；
//! - 汇总串 `GATE_SUMMARY` = 逐门「$gid: PASS/FAIL」全角分号连接（首项
//!   无前导分隔——bash `${GATE_SUMMARY:+；}` 同形；零门禁 = 空串）——
//!   步⑤结果包 `host_gates = ` 消费面；
//! - 任一门禁拒 → 总退 1；逐门全跑非首门短路（汇总面完整）。
//!
//! cwd 语义：bcts-run.sh 不改目录——门禁 cmd 继承调用方 cwd（`just bcts`
//! = 仓根；`just abi-check` 相对路径面依赖此约定）。镜像以 `repo` 为
//! cwd 参数。
//!
//! 结构性守卫（t1_check 双侧执法 + live GATE 恒一行，前置不变量）：
//! GATE cmd 恒非空（`host_gate $id cmd 不得为空`）——空 cmd 面（bash
//! 空命令恒 0）对发射器不可达；gid 在席/唯一同源。

use crate::load::LoadOutcome;

/// 步③出口：stdout/stderr 逐行 + 步⑤消费汇总串 + 总判定。
#[derive(Debug, Clone, PartialEq)]
pub struct GateOutcome {
    /// stdout 逐行（门禁行 + PASS 行）。
    pub stdout: Vec<String>,
    /// 拒面 stderr（FAIL 行 + 日志尾 5 行；全过 = 空）。
    pub stderr: Vec<String>,
    /// 汇总串（`gid: PASS；gid: FAIL` 形；零门禁 = 空串）。
    pub summary: String,
    /// 总判定（任一拒 = false）。
    pub ok: bool,
}

/// 日志文件名（`gate-$gid.log` 词面）。
pub fn gate_log_name(gid: &str) -> String {
    format!("gate-{gid}.log")
}

/// 日志尾 N 行（bash `tail -5` 同形：不足 N 行全发；空文件零行）。
fn tail_lines(s: &str, n: usize) -> Vec<String> {
    let all: Vec<&str> = s.lines().collect();
    let start = all.len().saturating_sub(n);
    all[start..].iter().map(|l| l.to_string()).collect()
}

/// 单门禁发射（`bash -c "$cmd"`，stdout+stderr → 日志文件）。
/// 返回 (rc 成功?, 日志体)。
fn spawn_gate(cmd: &str, repo: &str, log_path: &std::path::Path) -> (bool, String) {
    let out = std::fs::File::create(log_path).expect("建门禁日志");
    let err = out.try_clone().expect("克隆日志柄");
    let status = std::process::Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .current_dir(repo)
        .stdout(std::process::Stdio::from(out))
        .stderr(std::process::Stdio::from(err))
        .status();
    let log = std::fs::read_to_string(log_path).unwrap_or_default();
    (matches!(status, Ok(s) if s.success()), log)
}

/// 步③主面：逐 GATE 行同跑。`work_dir` = 日志落点（bash `$WORK` 语义）；
/// `repo` = 门禁 cmd 工作目录。
pub fn host_gates(o: &LoadOutcome, repo: &str, work_dir: &std::path::Path) -> GateOutcome {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut parts: Vec<String> = Vec::new();
    let mut ok = true;
    for e in &o.entries {
        let f: Vec<&str> = e.splitn(3, '\t').collect();
        if f.len() < 3 || f[0] != "GATE" {
            continue;
        }
        let (gid, cmd) = (f[1], f[2]);
        stdout.push(format!("bcts: host 门禁 [{gid}]：{cmd}"));
        let (pass, log) = spawn_gate(cmd, repo, &work_dir.join(gate_log_name(gid)));
        if pass {
            stdout.push(format!("bcts: host 门禁 [{gid}] PASS"));
            parts.push(format!("{gid}: PASS"));
        } else {
            stderr.push(format!(
                "bcts: FAIL: host 门禁 [{gid}] 拒（{cmd}）；日志尾 5 行："
            ));
            stderr.extend(tail_lines(&log, 5));
            parts.push(format!("{gid}: FAIL"));
            ok = false;
        }
    }
    GateOutcome {
        stdout,
        stderr,
        summary: parts.join("；"),
        ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成 LoadOutcome（GATE 行族）。
    fn lo(gates: &[(&str, &str)]) -> LoadOutcome {
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

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bctsrun-gate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn gate_log_name_law() {
        assert_eq!(gate_log_name("abi-check"), "gate-abi-check.log");
    }

    #[test]
    fn tail_five_law() {
        let s = "l1\nl2\nl3\nl4\nl5\nl6\nl7";
        assert_eq!(tail_lines(s, 5), vec!["l3", "l4", "l5", "l6", "l7"]);
        assert_eq!(tail_lines("only", 5), vec!["only"]);
        assert_eq!(tail_lines("", 5), Vec::<String>::new());
        // 无尾换行形：lines() 同吞。
        assert_eq!(tail_lines("a\nb", 5), vec!["a", "b"]);
    }

    #[test]
    fn pass_gate_lines() {
        let tmp = tmpdir("p");
        let o = lo(&[("g1", "true")]);
        let g = host_gates(&o, ".", &tmp);
        assert_eq!(
            g.stdout,
            vec!["bcts: host 门禁 [g1]：true", "bcts: host 门禁 [g1] PASS"]
        );
        assert!(g.stderr.is_empty());
        assert_eq!(g.summary, "g1: PASS");
        assert!(g.ok);
        // 日志文件在席（重定向面实证）。
        assert!(tmp.join("gate-g1.log").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn fail_gate_lines_and_tail() {
        let tmp = tmpdir("f");
        let cmd = "echo a; echo b; echo c; echo d; echo e; echo f; exit 3";
        let o = lo(&[("g2", cmd)]);
        let g = host_gates(&o, ".", &tmp);
        assert_eq!(g.stdout, vec![format!("bcts: host 门禁 [g2]：{cmd}")]);
        assert_eq!(
            g.stderr[0],
            format!("bcts: FAIL: host 门禁 [g2] 拒（{cmd}）；日志尾 5 行：")
        );
        // 尾 5 = b c d e f（a 挤出——tail -5 同形）。
        assert_eq!(g.stderr[1..], vec!["b", "c", "d", "e", "f"]);
        assert_eq!(g.summary, "g2: FAIL");
        assert!(!g.ok);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn fail_gate_short_log_all_lines() {
        let tmp = tmpdir("fs");
        let cmd = "echo x; echo y; exit 1";
        let o = lo(&[("g3", cmd)]);
        let g = host_gates(&o, ".", &tmp);
        // 不足 5 行全发。
        assert_eq!(g.stderr[1..], vec!["x", "y"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn fail_gate_empty_log_zero_tail() {
        let tmp = tmpdir("fe");
        let o = lo(&[("g4", "exit 7")]);
        let g = host_gates(&o, ".", &tmp);
        assert_eq!(g.stderr.len(), 1, "空日志零尾行：{g:?}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn gate_stderr_captured_into_log() {
        let tmp = tmpdir("se");
        let o = lo(&[("g5", "echo out; echo err >&2; exit 5")]);
        let g = host_gates(&o, ".", &tmp);
        // 双流归日志（> log 2>&1）：尾行含两侧输出。
        assert!(g.stderr[1..].contains(&"out".to_string()));
        assert!(g.stderr[1..].contains(&"err".to_string()));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn multi_gate_summary_join_no_shortcut() {
        let tmp = tmpdir("m");
        let o = lo(&[("p1", "true"), ("p2", "false"), ("p3", "true")]);
        let g = host_gates(&o, ".", &tmp);
        assert_eq!(g.summary, "p1: PASS；p2: FAIL；p3: PASS");
        assert!(!g.ok);
        // 逐门全跑非短路：p3 PASS 行在场。
        assert!(g.stdout.contains(&"bcts: host 门禁 [p3] PASS".to_string()));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn zero_gate_empty_summary() {
        let tmp = tmpdir("z");
        let o = lo(&[]);
        let g = host_gates(&o, ".", &tmp);
        assert_eq!(g.summary, "");
        assert!(g.ok);
        assert!(g.stdout.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn non_gate_rows_skipped() {
        let tmp = tmpdir("s");
        let mut o = lo(&[("g", "true")]);
        o.entries.insert(0, "META\t1\t91\t0\t1\tx\ty\tz".into());
        o.entries.insert(1, "MARK\tbare\tbare-01\tX".into());
        let g = host_gates(&o, ".", &tmp);
        assert_eq!(g.summary, "g: PASS");
        assert_eq!(g.stdout.len(), 2);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cmd_keeps_embedded_tabs() {
        let tmp = tmpdir("t");
        // read -r 三域：cmd 取行余量（内嵌制表符保留）。
        let o = LoadOutcome {
            entries: vec!["GATE\tgid\techo a\techo b".into()],
            n_markers: 0,
            n_gates: 1,
            floor: 91,
            n_bare: 0,
            img_pr: None,
            img_pr_anchor: None,
            pre_stdout: vec![],
        };
        let g = host_gates(&o, ".", &tmp);
        assert_eq!(g.stdout[0], "bcts: host 门禁 [gid]：echo a\techo b");
        assert!(g.ok);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
