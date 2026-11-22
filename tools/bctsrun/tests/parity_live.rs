//! bctsrun 五刀·parity：活树清单合成对拍。oracle 两面：
//!
//! ①真源面：仓内 `tools/markers.sh` 真跑 heredoc 体（bash -c 子进程——
//! bcts-run.sh build_live_tsv 逐字同参）vs `build_live_tsv` 读同文件——
//! stdout 逐行全等（92 行 = 91 MARK + 1 GATE）；
//! ②拒面/边界族：合成语料 ×N（守卫空集/unbound 短路序/后写覆盖/%02d
//! 溢出/体内注释/续行）——绿腿 stdout 全等；拒腿「拒分支对齐 + 退 2」
//! （文案不同源面：unbound 行号随 bash 形浮动——归一后对拍；空集守卫
//! 文案逐字同源）。
//!
//! bash/python 缺席 = skip（t1 parity 同款守卫）。

use bctsrun::build_live_tsv;
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

/// heredoc 体真跑（bcts-run.sh build_live_tsv 内嵌 bash -c 逐字）。
fn heredoc_live(markers_sh: &str) -> (String, String, Option<i32>) {
    let script = format!(
        r#"set -u
src="{markers_sh}"
source "$src"
n=0; for m in "${{MARKERS[@]}}"; do n=$((n+1)); printf "MARK\tbare\tbare-%02d\t%s\n" "$n" "$m"; done
n=0; for m in "${{ROOTFS_MARKERS[@]}}"; do n=$((n+1)); printf "MARK\trootfs\trootfs-%02d\t%s\n" "$n" "$m"; done
n=0; for m in "${{RX_MARKERS[@]}}"; do n=$((n+1)); printf "MARK\trx\trx-%02d\t%s\n" "$n" "$m"; done
printf "GATE\tabi-check\tjust abi-check\n"
"#
    );
    let o = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .output()
        .expect("spawn bash");
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code(),
    )
}

/// bcts-run.sh 拒面包头（:249 逐字）。
const WRAPPER: &str =
    "bcts: REJECT: 活树清单合成失败（markers.sh 非空守卫拒或数组缺席）：";

fn write_case(dir: &std::path::Path, name: &str, text: &str) -> String {
    let p = dir.join(name);
    std::fs::write(&p, text).unwrap();
    p.to_str().unwrap().to_string()
}

#[test]
fn live_real_source_parity() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let root = repo_root();
    let src = root.join("tools/markers.sh");
    let src_str = src.to_str().unwrap();
    let text = std::fs::read_to_string(&src).unwrap();
    let (out, err, code) = heredoc_live(src_str);
    assert_eq!(code, Some(0), "heredoc 应过：{err}");
    assert_eq!(out.lines().count(), 92, "91 MARK + 1 GATE");
    let rust = build_live_tsv(&text, src_str).expect("rust 应过");
    assert_eq!(rust, out, "真源活树 TSV 不等");
    // 计数钉板（TIER1_FLOOR_LIVE 91 = 32+56+3 契约面）。
    let marks = rust.lines().filter(|l| l.starts_with("MARK\t")).count();
    assert_eq!(marks, 91);
    assert_eq!(rust.lines().filter(|l| l.starts_with("MARK\tbare\t")).count(), 32);
    assert_eq!(rust.lines().filter(|l| l.starts_with("MARK\trootfs\t")).count(), 56);
    assert_eq!(rust.lines().filter(|l| l.starts_with("MARK\trx\t")).count(), 3);
    assert_eq!(rust.lines().filter(|l| l.starts_with("GATE\t")).count(), 1);
    // id 序律（%02d 机械序）。
    assert!(rust.contains("MARK\tbare\tbare-01\tBerryOS boot\n"));
    assert!(rust.contains("MARK\trx\trx-03\tPWRKEY unowned n=\n"));
}

#[test]
fn live_green_corpus_parity() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-live-par-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 绿语料：后写覆盖 + 续行 + 体内注释 + 空串元素 + 单引号 + # 值字符。
    let c1 = "MARKERS=(\"old\" \"o2\")\nMARKERS=(\"a1\" \"a2 (p) x\")\nROOTFS_MARKERS=(\"r1 ok src=\" \\\n  # 相内注释行\n  \"r2\" \\\n  \"\")\nRX_MARKERS=('sq #x 中文')\nECO_MARKERS=(\"e\")\n[ \"${#MARKERS[@]}\" -gt 0 ] && echo ok\n";
    for (name, text) in [("c1.sh", c1)] {
        let p = write_case(&dir, name, text);
        let (out, err, code) = heredoc_live(&p);
        assert_eq!(code, Some(0), "{name} heredoc 应过：{err}");
        // source 副作用行（守卫 echo ok）不进清单——剥非 MARK/GATE 行后对拍。
        let clean: String = out
            .lines()
            .filter(|l| l.starts_with("MARK\t") || l.starts_with("GATE\t"))
            .map(|l| format!("{l}\n"))
            .collect();
        let rust = build_live_tsv(text, &p).expect("{name} rust 应过");
        assert_eq!(rust, clean, "{name} TSV 不等");
    }
    // %02d ≥100（printf 自然十进制）。
    let m100 = format!(
        "MARKERS=({})\nROOTFS_MARKERS=(\"r\")\nRX_MARKERS=(\"x\")\nECO_MARKERS=(\"e\")\n",
        vec!["\"m\""; 100].join(" ")
    );
    let p = write_case(&dir, "m100.sh", &m100);
    let (out, err, code) = heredoc_live(&p);
    assert_eq!(code, Some(0), "{err}");
    let rust = build_live_tsv(&m100, &p).unwrap();
    assert_eq!(rust, out);
    assert!(rust.contains("MARK\tbare\tbare-100\tm\n"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn live_reject_corpus_parity() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("bctsrun-live-rej-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 空集守卫（MARKERS=()）：heredoc set -u 下 source 即守卫拒（exit 1 杀壳
    // → heredoc 非零），镜像同拒分支——文案逐字同源。
    let empty = "MARKERS=()\nROOTFS_MARKERS=(\"r\")\nRX_MARKERS=(\"x\")\nECO_MARKERS=(\"e\")\n[ \"${#MARKERS[@]}\" -gt 0 ] && [ \"${#ROOTFS_MARKERS[@]}\" -gt 0 ] && [ \"${#RX_MARKERS[@]}\" -gt 0 ] && [ \"${#ECO_MARKERS[@]}\" -gt 0 ] || {\n  echo \"markers.sh: 标记数组为空（单源被误清？）——拒跑防假绿\" >&2\n  exit 1\n}\n";
    let p = write_case(&dir, "empty.sh", empty);
    let (out, err, code) = heredoc_live(&p);
    assert_eq!(code, Some(1), "空集守卫应杀壳：{out}");
    assert!(err.contains("markers.sh: 标记数组为空（单源被误清？）——拒跑防假绿"));
    let r = build_live_tsv(empty, &p).unwrap_err();
    assert_eq!(
        r.0,
        format!("{WRAPPER}\nmarkers.sh: 标记数组为空（单源被误清？）——拒跑防假绿")
    );
    // 短路序：前项空 → && 链止（`|| true` 兜底零退出码面在 bash 合法，但
    // 镜像守卫面拒 = 拒分支对齐分歧注记——真源 heredoc（bcts-run.sh 实体）
    // 无 || true 兜底，source 即杀壳非零；本腿钉「空集先于 unbound」序）。
    let shortcircuit = "MARKERS=()\n[ \"${#MARKERS[@]}\" -gt 0 ] && [ \"${#ROOTFS_MARKERS[@]}\" -gt 0 ] && echo ok || true\n";
    let p2 = write_case(&dir, "sc.sh", shortcircuit);
    let r2 = build_live_tsv(shortcircuit, &p2).unwrap_err();
    assert!(r2.0.ends_with("标记数组为空（单源被误清？）——拒跑防假绿"), "{}", r2.0);
    // unbound（守卫在场形——真 markers.sh 语义）：bash 实测双重静默降级——
    // ①守卫行 `${#X[@]}` 长度展开 unbound 报错但处于 && 链内（POSIX 展开
    // 错不杀非交互壳、整条 AND-OR 弃走）；②数组消费循环 `"${X[@]}"` 在
    // bash 4.4+ 对 unset 数组静默空扩 → RX 相零行、GATE 照发、rc=0（假绿
    // 面！真 bcts-run.sh 靠步② floor 91 兜底）。镜像同形严拒（unbound 拒
    // 分支对齐 + 早于清单发射——比 bash 更严的诚实面，分歧注记钉板）。
    let unbound = "MARKERS=(\"a\")\nROOTFS_MARKERS=(\"r\")\n[ \"${#MARKERS[@]}\" -gt 0 ] && [ \"${#ROOTFS_MARKERS[@]}\" -gt 0 ] && [ \"${#RX_MARKERS[@]}\" -gt 0 ] && [ \"${#ECO_MARKERS[@]}\" -gt 0 ] || {\n  echo \"markers.sh: 标记数组为空（单源被误清？）——拒跑防假绿\" >&2\n  exit 1\n}\n";
    let p3 = write_case(&dir, "unb.sh", unbound);
    let (out3, err3, code3) = heredoc_live(&p3);
    // bash 文案随 locale：C/English 壳 = "unbound variable"、UTF-8 中文壳 =
    // 「未绑定的变量」——归一断言（关键词 RX_MARKERS + unbound）。
    let err3n = if err3.contains("未绑定的变量") {
        err3.replace("未绑定的变量", "unbound variable")
    } else {
        err3.clone()
    };
    assert_eq!(code3, Some(0), "bash 静默降级面（钉板）：{err3}");
    assert!(err3n.contains("RX_MARKERS: unbound variable"), "{err3}");
    assert!(!out3.contains("MARK\trx\t"), "RX 相应静默空：{out3}");
    assert!(out3.ends_with("GATE\tabi-check\tjust abi-check\n"));
    let r3 = build_live_tsv(unbound, &p3).unwrap_err();
    assert!(r3.0.starts_with(WRAPPER));
    assert!(r3.0.contains("RX_MARKERS: 未绑定的变量"), "{}", r3.0);
    std::fs::remove_dir_all(&dir).ok();
}
