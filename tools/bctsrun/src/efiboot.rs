//! bctsrun 十刀：镜像腿 QEMU 轮询段纯函数面镜像（现役单源 =
//! `tools/bcts-run.sh` :655-660 `assert_marker` + :704-723 轮询谓词 +
//! :730-738 banner/ident 门——彻底 Rust 化轨道 b 层 #7 刀序末档十刀）。
//!
//! 纯面拆分原则（九刀同款）：QEMU 进程/管道/超时编排不在本刀——轮询段的
//! 判定面 =「日志文本里缺什么」：EFI 结构面（固定串 + ERE + 接续）与
//! bare 相清单合成的 missing 列表是**读入日志后的纯函数**；banner/ident
//! 门同理（到齐后 banner 行必在席，断言零竞态——真源头注）。进程面归
//! CLI 收官刀。
//!
//! 行为面逐字镜像：
//! - 轮询谓词每轮全查：`missing` 列表构造序 = IMG_EFI_FIXED 四串 →
//!   IMG_CONT_MARKERS 三串（grep -qF -- 命中即跳过；缺 = 空格连接追加，
//!   `${missing:+$missing }$m` 首项无前导）→ IMG_EFI_ERE 六条（缺 =
//!   `ERE:` 前缀形追加）→ entries.tsv bare 相 MARK 行逐条
//!   `assert_marker`（缺 = 追加 mid，无前缀）；
//! - ERE 六条手写判定器（零 regex 依赖——bctsrun 纪律）：`[0-9]+` 一次或
//!   多次（空串不匹配——`+` 语义）+ 字面尾巴；`elf:` 条十六进制尾巴
//!   `0x[0-9a-f]+`（小写锚）；
//! - missing 判空 = 轮询出口（`[ -z "$missing" ] && break`）；deadline 面
//!   与 QEMU 活性探不在本刀（IO/时钟面）；
//! - banner 门：`grep -m1 'BerryOS boot'` 取首行（无行 = 空串）→
//!   `grep -Eq` 形态正则断言 + `grep -q 'nogit'` ident 拒；两门文案
//!   逐字（含「（日志无 BerryOS boot 行）」回退臂）。
//!
//! 钉序发现：固定串缺项与 bare 相 mid 缺项在 missing 里无区分标记，仅
//! ERE 缺项带 `ERE:` 前缀——真源三段循环的字面结构。

use crate::load::LoadOutcome;
use crate::markers::assert_marker;

/// IMG_EFI_FIXED 四串（bcts-run.sh :326 逐字——smoke-image.sh 引导链
/// 结构面第三消费方，改引导链断言面须连改两处的纪律照旧）。
pub const IMG_EFI_FIXED: [&str; 4] = [
    "BerryBoot EFI skeleton",
    "fw: EDK II",
    "fs: HELLO.TXT 28B match",
    "BERRYBOOT ok",
];

/// IMG_EFI_ERE 六条（bcts-run.sh :327-334 逐字）。
pub const IMG_EFI_ERE: [&str; 6] = [
    "gop: [0-9]+x[0-9]+",
    "kfile: [0-9]+B ok",
    "init: [0-9]+B ok",
    "elf: segs=[0-9]+ entry=0x[0-9a-f]+ ok",
    "load: segs=[0-9]+ pages=[0-9]+ ok",
    "mmap: n=[0-9]+ ok",
];

/// IMG_CONT_MARKERS 三串（bcts-run.sh :338 逐字——handoff 源专属形态）。
pub const IMG_CONT_MARKERS: [&str; 3] =
    ["HANDOFF ok ver=", "MMAP ok segs=", "ROOTFS ok src=handoff"];

/// 追加缺项：`${missing:+$missing }$m`——首项无前导、空格连接。
fn push_missing(missing: &mut String, m: &str) {
    if !missing.is_empty() {
        missing.push(' ');
    }
    missing.push_str(m);
}

/// 单条 ERE 判定（零 regex 手写镜像——六条模式面恰好落在 `[0-9]+` +
/// 字面尾巴这个子集；`elf:` 条尾巴为 `0x[0-9a-f]+`）。
///
/// 语义 = grep -E 单行匹配：任一行命中即过。`[0-9]+` = 一次或多次（空串
/// 不匹配）；十六进制面小写 a-f 锚（大写不命中——ERE 字面钉板）。
fn ere_hit(pattern: &str, corpus: &str) -> bool {
    corpus.lines().any(|line| ere_line_hit(pattern, line))
}

fn ere_line_hit(pattern: &str, line: &str) -> bool {
    // 模式分段：`[0-9]+` 记 (digit, false)、`[0-9a-f]+` 记 (digit, true)、
    // 其余为字面段（含 `x`/`B ok` 等）。段序 = 模式字面序。
    let mut segs: Vec<(&str, bool)> = Vec::new();
    let mut rest = pattern;
    loop {
        match rest.find('[') {
            Some(at) => {
                if at > 0 {
                    segs.push((&rest[..at], false));
                }
                let close = rest.find(']').expect("模式含 [ 无 ]");
                let class = &rest[at + 1..close];
                let hex = class.contains('a');
                assert!(
                    class == "0-9" || class == "0-9a-f",
                    "未支持字符类：{class}"
                );
                assert_eq!(
                    &rest[close + 1..close + 2],
                    "+",
                    "字符类后须随 + 量词：{pattern}"
                );
                segs.push(("+", hex));
                rest = &rest[close + 2..];
            }
            None => {
                if !rest.is_empty() {
                    segs.push((rest, false));
                }
                break;
            }
        }
    }

    let lb = line.as_bytes();

    // 回溯匹配：plus 段贪心 + 回退；literal 段逐字节对齐。
    fn match_from(segs: &[(&str, bool)], si: usize, pos: usize, lb: &[u8]) -> bool {
        if si == segs.len() {
            return true;
        }
        let (seg, hex) = segs[si];
        if seg == "+" {
            let fits = |b: u8| if hex { b.is_ascii_hexdigit() && b.to_ascii_lowercase() == b } else { b.is_ascii_digit() };
            let mut take = 0;
            while pos + take < lb.len() && fits(lb[pos + take]) {
                take += 1;
            }
            // 贪心 → 最短回退（字面尾巴锚定，回溯必要——gop 条 WxH 双段）。
            while take > 0 {
                if match_from(segs, si + 1, pos + take, lb) {
                    return true;
                }
                take -= 1;
            }
            false
        } else {
            let sb = seg.as_bytes();
            if lb.len() >= pos + sb.len() && &lb[pos..pos + sb.len()] == sb {
                match_from(segs, si + 1, pos + sb.len(), lb)
            } else {
                false
            }
        }
    }

    (0..=lb.len()).any(|start| match_from(&segs, 0, start, lb))
}

/// 轮询谓词：构造本轮 missing 列表（空 = 到齐出口）。
///
/// `log` = img-serial.log 已读入文本；`o` = 装载出口（entries.tsv 面）。
/// 行为面 = bcts-run.sh :704-723 三段循环逐字：EFI 固定串/接续串（缺 =
/// 原样追加）、ERE（缺 = `ERE:` 前缀）、bare 相 MARK 行（缺 = mid 追加）。
pub fn poll_missing(o: &LoadOutcome, log: &str) -> String {
    let mut missing = String::new();
    for m in IMG_EFI_FIXED.iter().chain(IMG_CONT_MARKERS.iter()) {
        if !log.contains(m) {
            push_missing(&mut missing, m);
        }
    }
    for e in IMG_EFI_ERE.iter() {
        if !ere_hit(e, log) {
            push_missing(&mut missing, &format!("ERE:{e}"));
        }
    }
    for row in &o.entries {
        let f: Vec<&str> = row.split('\t').collect();
        if f.len() >= 4 && f[0] == "MARK" && f[1] == "bare" {
            if !assert_marker(f[3], log) {
                push_missing(&mut missing, f[2]);
            }
        }
    }
    missing
}

/// `grep -m1 'BerryOS boot'`：首条命中行（无 = None）。
fn first_boot_banner(log: &str) -> Option<&str> {
    log.lines().find(|l| l.contains("BerryOS boot"))
}

/// banner 形态正则（bcts-run.sh :731-733 逐字）：
/// `BerryOS boot [0-9]+\.[0-9]+\.[0-9]+ \([A-Za-z0-9.-]+( rustc[A-Za-z0-9.?]+ (dbg|rel))?\)`
///
/// 手写镜像（零 regex）：grep -Eq 无锚——模式起点可在行内任意位置（但模式
/// 以字面 `BerryOS boot ` 开头，候选起点即该串各出现处）。三段数字（无前导
/// 空）+ 空格 + `(`ident 可选组 +`)`。
fn banner_shape_ok(line: &str) -> bool {
    // 各出现处逐一试（首处失败不排除后处命中——grep 同形）。
    let mut from = 0;
    while let Some(rel) = line[from..].find("BerryOS boot ") {
        if banner_shape_at(&line[from + rel + "BerryOS boot ".len()..]) {
            return true;
        }
        from = from + rel + 1;
    }
    false
}

fn banner_shape_at(rest: &str) -> bool {
    // 三段 [0-9]+\.[0-9]+\.[0-9]+（版本面），逐段非空数字、点分。
    let b = rest.as_bytes();
    let mut i = 0;
    for seg in 0..3 {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
        if seg < 2 {
            if i >= b.len() || b[i] != b'.' {
                return false;
            }
            i += 1;
        }
    }
    if i >= b.len() || b[i] != b' ' {
        return false;
    }
    let tail = &rest[i + 1..];
    // ident 尾巴：`( [A-Za-z0-9.-]+ ( rustc…)? )`——闭括号后可有余文（grep
    // 行内命中）。
    let Some(open) = tail.find('(') else {
        return false;
    };
    let Some(close_rel) = tail[open + 1..].find(')') else {
        return false;
    };
    let close = open + 1 + close_rel;
    let ident = &tail[open + 1..close];
    // ident 主段 = [A-Za-z0-9.-]+；可选 ` rustc[...] (dbg|rel)` 组。
    let (main, opt) = match ident.find(" rustc") {
        Some(at) => (&ident[..at], Some(&ident[at + 1..])),
        None => (ident, None),
    };
    if main.is_empty() || !main.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-')) {
        return false;
    }
    // 闭括号后余文任意（grep 行内任意位置命中同形——九刀 assert_marker 注）。
    let _ = close;
    if let Some(o) = opt {
        let Some(rest2) = o.strip_prefix("rustc") else {
            return false;
        };
        let Some(sp) = rest2.rfind(' ') else {
            return false;
        };
        let (mid, last) = (&rest2[..sp], &rest2[sp + 1..]);
        if !matches!(last, "dbg" | "rel") {
            return false;
        }
        if mid.is_empty()
            || !mid
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'?' | b'-' | b'_'))
        {
            return false;
        }
    }
    true
}

/// banner 门结果。`Ok(())` = 过；`Err(msg)` = img_fail 完整 stderr 行
/// （`bcts: FAIL: ` 前缀含——abipr.rs img_pr_check 先例同款口径）。
pub fn banner_gate(log: &str) -> Result<(), String> {
    let line = first_boot_banner(log);
    // grep -m1 无命中 = 空串喂下游 grep（两 grep 均不命中）→ 形态门先拒，
    // 文案回退臂「（日志无 BerryOS boot 行）」。
    let l = line.unwrap_or("");
    if !banner_shape_ok(l) {
        return Err(format!(
            "bcts: FAIL: boot banner 形态缺（G-ver-2；正则 = smoke-image.sh 同款）：{}",
            if l.is_empty() { "（日志无 BerryOS boot 行）" } else { l }
        ));
    }
    if l.contains("nogit") {
        return Err(format!("bcts: FAIL: boot banner ident 为 nogit——nogit 标识产物不得进发布链/过认证门（版本管理标准 §6）：{l}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成满配日志：EFI 十段 + 接续 + banner（真源形状样本）。
    fn full_log() -> String {
        let mut s = String::new();
        for m in IMG_EFI_FIXED {
            s.push_str(m);
            s.push('\n');
        }
        s.push_str("gop: 1280x800\n");
        s.push_str("kfile: 2048B ok\n");
        s.push_str("init: 4096B ok\n");
        s.push_str("elf: segs=4 entry=0x48020000 ok\n");
        s.push_str("load: segs=4 pages=512 ok\n");
        s.push_str("mmap: n=7 ok\n");
        s.push_str("KVA ok\n");
        for m in IMG_CONT_MARKERS {
            s.push_str(m);
            s.push_str("5\n");
        }
        s.push_str("BerryOS boot 0.9.7 (abc1234 rustc1.99.0 rel)\n");
        s
    }

    fn lo(entries: &[(&str, &str, &str, &str)]) -> LoadOutcome {
        LoadOutcome {
            entries: entries
                .iter()
                .map(|(t, p, m, e)| format!("{t}\t{p}\t{m}\t{e}"))
                .collect(),
            n_markers: 32,
            n_gates: 0,
            floor: 91,
            n_bare: 32,
            img_pr: None,
            img_pr_anchor: None,
            pre_stdout: vec![],
        }
    }

    #[test]
    fn poll_full_log_empty_missing() {
        let o = lo(&[
            ("MARK", "bare", "bare-01", "BerryOS boot"),
            ("MARK", "bare", "bare-02", "KVA ok"),
        ]);
        assert_eq!(poll_missing(&o, &full_log()), "");
    }

    #[test]
    fn poll_single_absence_pinned() {
        // 单缺 = 日志抹去 KVA ok 行 → 恰缺 bare-99（其余全在席）。
        let o = lo(&[("MARK", "bare", "bare-99", "KVA ok")]);
        let log: String = full_log()
            .lines()
            .filter(|l| *l != "KVA ok")
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(poll_missing(&o, &log), "bare-99");
    }

    #[test]
    fn poll_missing_join_order_fixed_then_ere_then_bare() {
        // 三段序钉板：固定串缺 → ERE 缺（ERE: 前缀）→ bare mid 缺。
        let o = lo(&[("MARK", "bare", "bare-77", "KVA ok")]);
        let mut log = String::new();
        // 全缺（空日志）→ 首项 = 第一固定串。
        let m = poll_missing(&o, &log);
        assert!(m.starts_with("BerryBoot EFI skeleton fw: EDK II"), "首段序：{m}");
        // 全缺时 ERE 前缀形在固定串后。
        assert!(m.contains("ERE:gop: [0-9]+x[0-9]+"), "ERE 前缀形：{m}");
        // bare mid 缺在尾。
        assert!(m.ends_with("bare-77"), "尾段 bare：{m}");
        // 首项无前导空格。
        assert!(!m.starts_with(' '), "首项无前导");
        // 空日志下 QEMU 活性不在此刀面——但轮询谓词本身空日志全缺。
        log.push('\n');
        assert_eq!(poll_missing(&o, &log), m, "空行日志与空日志同缺集");
    }

    #[test]
    fn poll_rootfs_phase_rows_ignored() {
        // rootfs/RX 相 MARK 行不进轮询面（EFI 路结构性缺席——真源 continue）。
        let o = lo(&[
            ("MARK", "rootfs", "rf-01", "ZZZ never ok"),
            ("MARK", "bare", "bare-01", "BerryOS boot"),
        ]);
        assert_eq!(poll_missing(&o, &full_log()), "");
    }

    #[test]
    fn poll_gate_and_meta_rows_ignored() {
        let o = lo(&[
            ("GATE", "abi-check", "just abi-check", ""),
            ("META", "live", "91", "0"),
        ]);
        assert_eq!(poll_missing(&o, &full_log()), "");
    }

    #[test]
    fn poll_bare_marker_demo_prefix_guard_respected() {
        // 轮询面走 assert_marker 同语义：IDEMO ok 前邻 I 不算 DEMO ok 命中。
        let mut log = full_log();
        log.push_str("IDEMO ok\n");
        let o = lo(&[("MARK", "bare", "bare-d", "DEMO ok")]);
        assert_eq!(poll_missing(&o, &log), "bare-d");
    }

    #[test]
    fn ere_matcher_positives_and_negatives() {
        let ok = [
            ("gop: [0-9]+x[0-9]+", "gop: 1280x800"),
            ("gop: [0-9]+x[0-9]+", "x gop: 1x2 y"),
            ("kfile: [0-9]+B ok", "kfile: 2048B ok"),
            ("elf: segs=[0-9]+ entry=0x[0-9a-f]+ ok", "elf: segs=4 entry=0x48020000 ok"),
            ("elf: segs=[0-9]+ entry=0x[0-9a-f]+ ok", "elf: segs=1 entry=0xabc ok"),
            ("mmap: n=[0-9]+ ok", "mmap: n=7 ok"),
        ];
        for (p, l) in ok {
            assert!(ere_line_hit(p, l), "应命中：{p} × {l}");
        }
        let bad = [
            ("gop: [0-9]+x[0-9]+", "gop: x800"),            // 空数字段
            ("gop: [0-9]+x[0-9]+", "gop: 1280x"),           // 尾段空
            ("kfile: [0-9]+B ok", "kfile: B ok"),           // + 不匹配空
            ("elf: segs=[0-9]+ entry=0x[0-9a-f]+ ok", "elf: segs=4 entry=0xABCDEF ok"), // 大写不锚
            ("elf: segs=[0-9]+ entry=0x[0-9a-f]+ ok", "elf: segs=4 entry=0x ok"),       // hex 段空
            ("mmap: n=[0-9]+ ok", "mmap: n= ok"),
        ];
        for (p, l) in bad {
            assert!(!ere_line_hit(p, l), "应不命中：{p} × {l}");
        }
    }

    #[test]
    fn ere_hex_class_lowercase_only() {
        assert!(ere_line_hit("elf: segs=[0-9]+ entry=0x[0-9a-f]+ ok", "elf: segs=4 entry=0xdeadbeef ok"));
        assert!(!ere_line_hit("elf: segs=[0-9]+ entry=0x[0-9a-f]+ ok", "elf: segs=4 entry=0xDead ok"));
    }

    #[test]
    fn banner_gate_full_form_passes() {
        assert!(banner_gate(&full_log()).is_ok());
    }

    #[test]
    fn banner_gate_minimal_form_passes() {
        let log = "BerryOS boot 1.2.3 (abc)\n";
        assert!(banner_gate(&full_log_with(log)).is_ok());
    }

    fn full_log_with(extra: &str) -> String {
        let mut s = full_log();
        s.push_str(extra);
        s
    }

    #[test]
    fn banner_gate_rejects_shape_and_nogit() {
        let bad_shapes = [
            "BerryOS boot 1.2 (abc)\n",             // 两段版本
            "BerryOS boot 1.2.x (abc)\n",           // 非数字段
            "BerryOS boot 1.2.3 abc\n",             // 缺括号
            "BerryOS boot 1.2.3 ()\n",              // 空 ident
            "BerryOS boot 1.2.3 (rustc1.0 rel)\n",  // 缺主段（纯可选组）
        ];
        for s in bad_shapes {
            let log = replace_banner(&full_log(), s);
            let err = banner_gate(&log).unwrap_err();
            assert!(err.starts_with("bcts: FAIL: boot banner 形态缺"), "{s} → {err}");
        }
        let log = replace_banner(&full_log(), "BerryOS boot 1.2.3 (nogit)\n");
        let err = banner_gate(&log).unwrap_err();
        assert!(err.starts_with("bcts: FAIL: boot banner ident 为 nogit"), "{err}");
    }

    fn replace_banner(log: &str, new: &str) -> String {
        let mut out = String::new();
        let mut done = false;
        for l in log.lines() {
            if !done && l.contains("BerryOS boot") {
                out.push_str(new.trim_end_matches('\n'));
                out.push('\n');
                done = true;
            } else {
                out.push_str(l);
                out.push('\n');
            }
        }
        out
    }

    #[test]
    fn banner_gate_empty_log_fallback_message() {
        let err = banner_gate("").unwrap_err();
        assert!(err.contains("（日志无 BerryOS boot 行）"), "{err}");
        assert!(err.starts_with("bcts: FAIL: boot banner 形态缺"), "{err}");
    }

    #[test]
    fn banner_gate_nogit_inside_ident_vs_shape_priority() {
        // 形态门先于 nogit 门（真源序）：`nogit` 在闭括号后余文 = 形态过、
        // ident 门照拒（`grep -q 'nogit'` 整行扫）。
        let log = replace_banner(&full_log(), "BerryOS boot 1.2.3 (abc) extra nogit stuff\n");
        let err = banner_gate(&log).unwrap_err();
        assert!(err.starts_with("bcts: FAIL: boot banner ident 为 nogit"), "{err}");
    }
}
