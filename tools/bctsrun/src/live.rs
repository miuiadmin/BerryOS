//! bctsrun 五刀：活树清单合成镜像（现役单源 = `bcts-run.sh build_live_tsv`
//! heredoc 体——source `tools/markers.sh` 三数组 + 固定 gate 行；彻底 Rust 化
//! 轨道 b 层 #7 刀序末档五刀）。
//!
//! - `build_live_tsv(markers_text, src_path)`：markers.sh 文本 → 步①出口
//!   协议 TSV（`MARK<TAB>phase<TAB>id<TAB>expect` 逐条 + 尾随恰一行
//!   `GATE<TAB>abi-check<TAB>just abi-check`）。id = 相内序号 `%02d` 机械
//!   生成（bash printf 同律：≥100 自然十进制不截断）。
//! - 拒面 = `bcts: REJECT: 活树清单合成失败（markers.sh 非空守卫拒或数组
//!   缺席）：` + 换行 + bash 原始 stderr 行（bcts-run.sh :246-250 逐字同源；
//!   退 2 由 CLI 臂映射）。
//!
//! 语面 = bash 数组赋量子集（markers.sh 现役形全集）：行首 `NAME=(` 顶层
//! 赋值 + `"…"`/`'…'` 引号元素 + `\` 续行 + 裸跨行体 + 全行/体内 `#` 注释
//! + 同键后写覆盖。守卫序 = 真文件 `[ ${#MARKERS[@]} … ECO_MARKERS[@] ]`
//! && 链逐项求值：缺席 → unbound 崩（`<src>: 行 N: NAME: 未绑定的变量`，
//! N = 守卫行——首个引用位）；任一空 → 空集守卫文案；短路序精确镜像
//! （前项空/false 即止，后项缺失不暴露）。
//!
//! 诚实分歧注记（reconcile 三刀「崩溃面诚实处理」同款先例，parity 钉板）：
//! ①非引号元素/`$` 展开/glob/行内多赋值/转义面——bash 侧合法（展开语义）
//! 而镜像 loud 拒：镜像误展开 = 静默错合成，诚实拒优于假等；②bash 语法级
//! 错误（数组面之外）镜像不建模（单源真跑仍是执法面，本件 = parity oracle
//! 镜像）；③unbound 文案 `行`/`line` 随 bash locale 浮动——镜像钉中文形，
//! parity 侧归一后对拍。

use crate::Reject;
use std::collections::HashMap;

/// 守卫序四数组（真文件守卫 && 链求值序；ECO 不进 TSV 但进守卫面）。
const GUARD_ORDER: [&str; 4] = ["MARKERS", "ROOTFS_MARKERS", "RX_MARKERS", "ECO_MARKERS"];
/// TSV 三数组（相名 = id 前缀）。
const TSV_ARRAYS: [(&str, &str); 3] = [
    ("MARKERS", "bare"),
    ("ROOTFS_MARKERS", "rootfs"),
    ("RX_MARKERS", "rx"),
];
/// 固定 gate 行（heredoc 尾随恰一行；活树态 [[host_gate]] 等价物）。
const GATE_LINE: &str = "GATE\tabi-check\tjust abi-check";
/// 拒面包头（bcts-run.sh :249 逐字）。
const WRAPPER: &str = "bcts: REJECT: 活树清单合成失败（markers.sh 非空守卫拒或数组缺席）：";

fn rej(detail: String) -> Reject {
    Reject(format!("{WRAPPER}\n{detail}"))
}

fn is_name(s: &str) -> bool {
    !s.is_empty()
        && (s.as_bytes()[0].is_ascii_alphabetic() || s.as_bytes()[0] == b'_')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// 单行体消费结果。
enum Body {
    /// 本行消费完，体未闭合（续行）。
    More,
    /// `)` 闭合。
    Closed,
}

/// 数组体单物理行消费：引号元素收录 / 注释跳过 / `\` 续行 / `)` 闭合判。
fn consume_body_line(line: &str, lineno: u32, elems: &mut Vec<String>) -> Result<Body, Reject> {
    let s = line;
    let mut p = 0;
    loop {
        while p < s.len() && (s.as_bytes()[p] == b' ' || s.as_bytes()[p] == b'\t') {
            p += 1;
        }
        if p >= s.len() {
            return Ok(Body::More);
        }
        match s.as_bytes()[p] {
            b'#' => return Ok(Body::More),
            b'\\' => {
                // 行尾反斜杠（其后仅空白）= 续行；中缀反斜杠 = 转义面拒。
                if s[p + 1..].trim().is_empty() {
                    return Ok(Body::More);
                }
                return Err(rej(format!(
                    "markers.sh: 行 {lineno} 反斜杠非行尾（子集无转义面）：{}",
                    s
                )));
            }
            b')' => {
                let after = s[p + 1..].trim_start();
                if after.is_empty() || after.starts_with('#') {
                    return Ok(Body::Closed);
                }
                return Err(rej(format!(
                    "markers.sh: 行 {lineno} 数组闭合后拖尾（非目标形）：{after}"
                )));
            }
            b'"' | b'\'' => {
                let q = s.as_bytes()[p] as char;
                match s[p + 1..].find(q) {
                    Some(off) => {
                        let close = p + 1 + off;
                        let inner = &s[p + 1..close];
                        if inner.contains('\\') {
                            return Err(rej(format!(
                                "markers.sh: 行 {lineno} 元素含反斜杠（子集无转义面）"
                            )));
                        }
                        // 紧贴判 = 闭合引号后首字符（不跳空白——`"a" "b"` 分隔
                        // 合法、`"a"b` 拼接形 = 非目标）。
                        let after_raw = &s[close + 1..];
                        let tight = after_raw.chars().next();
                        if let Some(c) = tight {
                            if !c.is_whitespace()
                                && c != ')'
                                && c != '#'
                                && c != '\\'
                            {
                                return Err(rej(format!(
                                    "markers.sh: 行 {lineno} 元素闭合引号后紧贴他字符（非目标形）：{after_raw}"
                                )));
                            }
                        }
                        elems.push(inner.to_string());
                        p = close + 1;
                    }
                    None => {
                        return Err(rej(format!(
                            "markers.sh: 行 {lineno} 元素引号未闭合（子集元素单物理行）"
                        )));
                    }
                }
            }
            _ => {
                let word: String =
                    s[p..].chars().take_while(|c| !c.is_whitespace()).collect();
                return Err(rej(format!(
                    "markers.sh: 行 {lineno} 非引号元素（非目标形，展开/glob 面诚实拒）：{word}"
                )));
            }
        }
    }
}

/// 顶层扫描：行首 `NAME=(` 起数组赋值（多行体聚合），行序返回。
fn scan_defs(text: &str) -> Result<Vec<(String, Vec<String>)>, Reject> {
    let lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let mut defs = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        let started = t.find("=(").filter(|&eq| is_name(&t[..eq]));
        if let Some(eq) = started {
            let name = t[..eq].to_string();
            let mut elems: Vec<String> = Vec::new();
            let mut rest = &t[eq + 2..];
            let mut lineno = (i + 1) as u32;
            let closed = loop {
                match consume_body_line(rest, lineno, &mut elems)? {
                    Body::Closed => break true,
                    Body::More => {}
                }
                i += 1;
                if i >= lines.len() {
                    return Err(rej(format!(
                        "markers.sh: 行 {lineno} 数组体未闭合（文件尾）"
                    )));
                }
                lineno = (i + 1) as u32;
                rest = lines[i];
            };
            if closed {
                defs.push((name, elems));
            }
        }
        i += 1;
    }
    Ok(defs)
}

/// 守卫行号（首个 `${#MARKERS[@]}` 引用位；守卫缺席 = 0）。
fn guard_line(text: &str) -> u32 {
    for (idx, l) in text.split('\n').enumerate() {
        if l.contains("${#MARKERS[@]}") {
            return (idx + 1) as u32;
        }
    }
    0
}

/// 活树清单合成（bcts-run.sh build_live_tsv 镜像）。`src_path` = markers.sh
/// 路径（unbound 拒面前缀用——bash 报错带 source 路径）。
pub fn build_live_tsv(markers_text: &str, src_path: &str) -> Result<String, Reject> {
    let defs = scan_defs(markers_text)?;
    // 同键后写覆盖（bash 赋值语义）。
    let mut latest: HashMap<&str, &Vec<String>> = HashMap::new();
    for (name, elems) in &defs {
        latest.insert(name.as_str(), elems);
    }
    // 守卫 && 链逐项求值：缺席 → unbound（求值即崩）；空 → 空集守卫；短路序精确。
    for name in GUARD_ORDER {
        match latest.get(name) {
            None => {
                return Err(rej(format!(
                    "{src_path}: 行 {}: {name}: 未绑定的变量",
                    guard_line(markers_text)
                )));
            }
            Some(v) if v.is_empty() => {
                return Err(rej(
                    "markers.sh: 标记数组为空（单源被误清？）——拒跑防假绿".to_string(),
                ));
            }
            _ => {}
        }
    }
    let mut out = String::new();
    for (name, phase) in TSV_ARRAYS {
        for (i, m) in latest[name].iter().enumerate() {
            out.push_str(&format!("MARK\t{phase}\t{phase}-{:02}\t{m}\n", i + 1));
        }
    }
    out.push_str(GATE_LINE);
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "MARKERS=(\"a1\" \"a2\")\nROOTFS_MARKERS=(\"r1 ok src=\" \\\n  # 相内注释\n  \"r2 (p) x\" \\\n  \"\")\nRX_MARKERS=(\"x#y 中文\")\nECO_MARKERS=(\"e\")\n[ \"${#MARKERS[@]}\" -gt 0 ] && echo ok\n";

    #[test]
    fn live_basics() {
        let tsv = build_live_tsv(GOOD, "/m.sh").unwrap();
        assert_eq!(
            tsv,
            "MARK\tbare\tbare-01\ta1\nMARK\tbare\tbare-02\ta2\nMARK\trootfs\trootfs-01\tr1 ok src=\nMARK\trootfs\trootfs-02\tr2 (p) x\nMARK\trootfs\trootfs-03\t\nMARK\trx\trx-01\tx#y 中文\nGATE\tabi-check\tjust abi-check\n"
        );
    }

    #[test]
    fn live_latest_wins_and_pad100() {
        // 后写覆盖 + %02d ≥100 自然十进制（printf 同律）。
        let text = "MARKERS=(\"old\")\nMARKERS=("
            .to_string()
            + &vec!["\"m\""; 100].join(" ")
            + ")\nROOTFS_MARKERS=(\"r\")\nRX_MARKERS=(\"x\")\nECO_MARKERS=(\"e\")\n";
        let tsv = build_live_tsv(&text, "/m.sh").unwrap();
        assert!(tsv.contains("MARK\tbare\tbare-100\tm\n"));
        assert!(!tsv.contains("old"));
    }

    #[test]
    fn live_guard_missing_unbound() {
        // 缺席 → unbound 崩面（守卫行号 = 首个引用位）。
        let r = build_live_tsv("MARKERS=(\"a\")\nROOTFS_MARKERS=(\"r\")\n[ \"${#MARKERS[@]}\" -gt 0 ] && echo ok\n", "/m.sh").unwrap_err();
        assert_eq!(
            r.0,
            "bcts: REJECT: 活树清单合成失败（markers.sh 非空守卫拒或数组缺席）：\n/m.sh: 行 3: RX_MARKERS: 未绑定的变量"
        );
        // 短路序：前项空先于后项缺失（bash && 链求值序）。
        let empty_first = build_live_tsv(
            "MARKERS=()\nROOTFS_MARKERS=(\"r\")\n[ \"${#MARKERS[@]}\" -gt 0 ] && echo ok\n",
            "/m.sh",
        )
        .unwrap_err();
        assert!(empty_first.0.ends_with("markers.sh: 标记数组为空（单源被误清？）——拒跑防假绿"));
    }

    #[test]
    fn live_guard_empty() {
        let r = build_live_tsv(
            "MARKERS=(\"a\")\nROOTFS_MARKERS=(\"r\")\nRX_MARKERS=()\nECO_MARKERS=(\"e\")\n[ \"${#MARKERS[@]}\" -gt 0 ] && echo ok\n",
            "/m.sh",
        )
        .unwrap_err();
        assert_eq!(
            r.0,
            "bcts: REJECT: 活树清单合成失败（markers.sh 非空守卫拒或数组缺席）：\nmarkers.sh: 标记数组为空（单源被误清？）——拒跑防假绿"
        );
    }

    #[test]
    fn live_non_target_rejects() {
        // 非引号元素（bash 合法展开面——镜像诚实拒，分歧注记①）。
        let r = build_live_tsv("MARKERS=(abc def)\nROOTFS_MARKERS=(\"r\")\nRX_MARKERS=(\"x\")\nECO_MARKERS=(\"e\")\n", "/m.sh").unwrap_err();
        assert!(r.0.contains("非引号元素"), "{}", r.0);
        // 未闭合引号。
        let r = build_live_tsv("MARKERS=(\"a\nclosing\")\nROOTFS_MARKERS=(\"r\")\nRX_MARKERS=(\"x\")\nECO_MARKERS=(\"e\")\n", "/m.sh").unwrap_err();
        assert!(r.0.contains("引号未闭合"), "{}", r.0);
        // 闭合后拖尾（行内双赋值面）。
        let r = build_live_tsv("MARKERS=(\"a\") OTHER=(\"b\")\nROOTFS_MARKERS=(\"r\")\nRX_MARKERS=(\"x\")\nECO_MARKERS=(\"e\")\n", "/m.sh").unwrap_err();
        assert!(r.0.contains("闭合后拖尾"), "{}", r.0);
        // 体未闭合到文件尾。
        let r = build_live_tsv("MARKERS=(\"a\"\n", "/m.sh").unwrap_err();
        assert!(r.0.contains("未闭合"), "{}", r.0);
        // 中缀反斜杠。
        let r = build_live_tsv("MARKERS=(\"a\" \\x \"b\")\nROOTFS_MARKERS=(\"r\")\nRX_MARKERS=(\"x\")\nECO_MARKERS=(\"e\")\n", "/m.sh").unwrap_err();
        assert!(r.0.contains("反斜杠"), "{}", r.0);
    }
}
