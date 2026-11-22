//! bctsrun 三刀：BCTS Tier 2 静态对账（reconcile_t2 镜像——现役单源
//! `tools/bcts/reconcile_t2.py`〔bcts-run.sh PYEOF③段机械抽出〕，彻底 Rust 化
//! 轨道 b 层 #7 刀序末档三刀）。
//!
//! 输入 = 二刀 `check_t2` 发射的 TSV（CONTRACT/OP/CASE 行；META/SEM 行忽略）；
//! 对每份合同读 interface_src（协议 crate 源文件）扫 `pub const OP_*: u8 = <n>;`
//! 常量表，与合同 op 表对账：缺席/错号 = 开放缺口 loud 报（不硬绿）；crate 侧
//! 多出 op = add-only 合法计行不拦。另报五类用例覆盖（缺口类如实记账不拦——
//! Tier 2 开通条件随 #83 落数，设计稿 §3.2 注 a）。
//!
//! 文案与 Python 逐字同源；出口协议由调用方映射：全绿 = 退 0 / 开放缺口 =
//! 退 1（`ReconcileReport::exit_code`）/ interface_src 读失败 = `Err`（CLI 臂退 2）。
//!
//! 已知分歧（Python traceback 崩溃面，镜像取诚实处理不复刻崩溃；TSV 由 check_t2
//! 发射、形状受校验，此面产线不可达）：①`u8 = 010;` 前导零十进制——Python
//! int(x, 0) ValueError 崩，本镜像按十进制读；②TSV 行引用无 CONTRACT 的 cid /
//! 列数不足——Python KeyError/IndexError 崩（OP 行静默丢），本镜像静默丢；
//!③`\d`/`\s` Unicode 面（非 ASCII 数字/空白）——Python re 匹配 str 全 Unicode，
//! 本镜像 ASCII 面（协议 crate 源文件无此形）。

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::Path;

const CLASS_KEYS: [&str; 5] = ["seq", "reject", "version", "error", "timeout"];
const CLASS_ZH: [&str; 5] = ["合法op序列", "非法op拒绝", "version协商", "错误域", "超时失败隔离"];

/// 合同记数形：fs 1..10 十进制 / hdi 0x40.. 十六进制（<16 十进制）。
fn opn(n: i64) -> String {
    if n < 16 {
        n.to_string()
    } else {
        format!("0x{n:02x}")
    }
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'
}

/// `\s*` 镜像（ASCII 空白面——见模块注③）。
fn skip_ws(b: &[u8], mut p: usize) -> usize {
    while p < b.len() && matches!(b[p], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        p += 1;
    }
    p
}

/// `pub\s+const\s+(OP_[A-Z0-9_]+)\s*:\s*u8\s*=\s*(0x[0-9A-Fa-f]+|\d+)\s*;` 镜像
/// （`re.finditer` 左起非重叠扫描：无前导词界——`xpub const` 亦命中；数字节
/// 回溯面「hex 不足位 / 尾非 `;`」= 该起点整体不命中，与正则一致）。
/// 返回文件序 (name, value)——同键后写覆盖前写由调用方 dict 语义承担。
fn scan_op_consts(text: &str) -> Vec<(String, i64)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = text[i..].find("pub") {
        let start = i + rel;
        let mut p = start + 3;
        macro_rules! miss {
            () => {{
                i = start + 1;
                continue;
            }};
        }
        let q = skip_ws(b, p);
        if q == p {
            miss!();
        }
        p = q;
        if !b[p..].starts_with(b"const") {
            miss!();
        }
        p += 5;
        let q = skip_ws(b, p);
        if q == p {
            miss!();
        }
        p = q;
        if !b[p..].starts_with(b"OP_") {
            miss!();
        }
        let name_start = p;
        let mut q = p;
        while q < b.len() && is_name_byte(b[q]) {
            q += 1;
        }
        if q == name_start {
            miss!();
        }
        let name = &text[name_start..q];
        p = skip_ws(b, q);
        if b.get(p) != Some(&b':') {
            miss!();
        }
        p = skip_ws(b, p + 1);
        if !b[p..].starts_with(b"u8") {
            miss!();
        }
        p = skip_ws(b, p + 2);
        if b.get(p) != Some(&b'=') {
            miss!();
        }
        p = skip_ws(b, p + 1);
        let num: i64;
        if b[p..].starts_with(b"0x") {
            let hs = p + 2;
            let mut q = hs;
            while q < b.len() && b[q].is_ascii_hexdigit() {
                q += 1;
            }
            if q == hs {
                miss!();
            }
            num = i64::from_str_radix(&text[hs..q], 16).unwrap_or(i64::MAX);
            p = q;
        } else {
            let ds = p;
            let mut q = ds;
            while q < b.len() && b[q].is_ascii_digit() {
                q += 1;
            }
            if q == ds {
                miss!();
            }
            num = text[ds..q].parse::<i64>().unwrap_or(i64::MAX);
            p = q;
        }
        p = skip_ws(b, p);
        if b.get(p) != Some(&b';') {
            miss!();
        }
        out.push((name.to_string(), num));
        i = p + 1;
    }
    out
}

struct Contract {
    cid: String,
    interface: String,
    src: String,
    ops: Vec<(i64, String)>,
}

#[derive(Default)]
struct CaseCount {
    kinds: BTreeMap<String, usize>,
    class: BTreeMap<String, usize>,
}

/// 对账报告：stdout 行序列（Python print 逐行同源）+ 开放缺口总数（FAIL 行尾段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileReport {
    pub lines: Vec<String>,
    pub total_gap: usize,
}

impl ReconcileReport {
    /// 出口码：开放缺口 > 0 → 1（不硬绿）；全绿 → 0。
    pub fn exit_code(&self) -> i32 {
        if self.total_gap > 0 {
            1
        } else {
            0
        }
    }
}

/// TSV 面解析（`line.rstrip("\n").split("\t")`——CONTRACT/OP/CASE 三行型消费，
/// 余行型与短行静默丢，见模块注②）。
fn parse_tsv(tsv: &str) -> (Vec<Contract>, BTreeMap<String, CaseCount>) {
    let mut contracts: Vec<Contract> = Vec::new();
    let mut cases: BTreeMap<String, CaseCount> = BTreeMap::new();
    for raw in tsv.split('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let p: Vec<&str> = line.split('\t').collect();
        match p.first().copied().unwrap_or("") {
            "CONTRACT" if p.len() >= 5 => {
                contracts.push(Contract {
                    cid: p[1].to_string(),
                    interface: p[3].to_string(),
                    src: p[4].to_string(),
                    ops: Vec::new(),
                });
                cases.entry(p[1].to_string()).or_default();
            }
            "OP" if p.len() >= 4 => {
                if let Some(c) = contracts.iter_mut().find(|c| c.cid == p[1]) {
                    c.ops.push((p[2].parse::<i64>().unwrap_or(0), p[3].to_string()));
                }
            }
            "CASE" if p.len() >= 6 => {
                if let Some(cv) = cases.get_mut(p[1]) {
                    *cv.kinds.entry(p[4].to_string()).or_insert(0) += 1;
                    *cv.class.entry(p[5].to_string()).or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }
    (contracts, cases)
}

/// Tier 2 静态对账（reconcile_t2.py `reconcile()` 镜像——文案逐字同源）。
/// `repo` = 仓库根（interface_src 相对路径锚）；interface_src 在席但读失败 → `Err`。
pub fn reconcile_t2(tsv: &str, repo: &str) -> io::Result<ReconcileReport> {
    let (contracts, cases) = parse_tsv(tsv);
    let mut out: Vec<String> = Vec::new();
    let mut fail = false;
    let mut total_gap = 0usize;
    for c in &contracts {
        let path = Path::new(repo).join(&c.src);
        if !path.is_file() {
            out.push(format!(
                "bcts: T2 对账 [{}]：interface_src 不可读：{}——开放缺口（合同②件单源指向断链）",
                c.cid, c.src
            ));
            fail = true;
            total_gap += c.ops.len();
            continue;
        }
        let text = std::fs::read_to_string(&path)?;
        let mut found: BTreeMap<String, i64> = BTreeMap::new();
        for (k, v) in scan_op_consts(&text) {
            found.insert(k, v);
        }
        let mut gaps: Vec<String> = Vec::new();
        for (num, name) in &c.ops {
            let key = format!("OP_{name}");
            match found.get(&key) {
                None => gaps.push(format!("{key} 缺席（合同 op {}）", opn(*num))),
                Some(&v) if v != *num => gaps.push(format!("{key} 错号（合同 {} vs crate {}）", opn(*num), opn(v))),
                _ => {}
            }
        }
        let want: HashSet<String> = c.ops.iter().map(|(_, n)| format!("OP_{n}")).collect();
        let mut extras: Vec<&str> = found.keys().filter(|k| !want.contains(*k)).map(|k| k.as_str()).collect();
        extras.sort_unstable();
        let hit = c.ops.len() - gaps.len();
        let tail = format!(
            "；crate 侧多 {} op（add-only 合法不拦：{}）",
            extras.len(),
            if extras.is_empty() { "无".to_string() } else { extras.join(",") }
        );
        if !gaps.is_empty() {
            for g in &gaps {
                out.push(format!("bcts: T2 对账 [{}]（{}）：开放缺口——{}", c.cid, c.interface, g));
            }
            out.push(format!(
                "bcts: T2 对账 [{}]：合同 op 表 {} 项，crate 命中 {}，差集 {}{tail}",
                c.cid,
                c.ops.len(),
                hit,
                gaps.len()
            ));
            fail = true;
        } else {
            out.push(format!(
                "bcts: T2 对账 [{}]（{}）：合同 op 表 {} 项，crate 命中 {}，差集 0（开放缺口 = 0）{tail}",
                c.cid,
                c.interface,
                c.ops.len(),
                hit
            ));
        }
        total_gap += gaps.len();
        let empty = CaseCount::default();
        let cv = cases.get(&c.cid).unwrap_or(&empty);
        let pos = cv.kinds.get("pos").copied().unwrap_or(0);
        let neg = cv.kinds.get("neg").copied().unwrap_or(0);
        let cls = CLASS_KEYS
            .iter()
            .map(|k| format!("{k}={}", cv.class.get(*k).copied().unwrap_or(0)))
            .collect::<Vec<_>>()
            .join(" ");
        let miss_cls: Vec<&str> = CLASS_KEYS
            .iter()
            .enumerate()
            .filter(|(i, _)| cv.class.get(CLASS_KEYS[*i]).copied().unwrap_or(0) == 0)
            .map(|(i, _)| CLASS_ZH[i])
            .collect();
        let miss = if miss_cls.is_empty() {
            String::new()
        } else {
            format!("——缺口类：{}（如实记账不拦：Tier 2 开通条件随 #83 落数）", miss_cls.join("、"))
        };
        out.push(format!(
            "bcts: T2 覆盖 [{}]：wire 用例 {} 行（pos {pos}/neg {neg}）；五类 {cls}{miss}",
            c.cid,
            pos + neg
        ));
    }
    if fail {
        out.push(format!(
            "bcts: FAIL: T2 静态对账开放缺口 {total_gap} 项（crate op 全集 ⊉ 合同表——如实报，不硬绿）"
        ));
    } else {
        out.push("bcts: T2 静态对账全绿（实现侧协议 crate op 全集 ⊇ 合同②件 op 表）".to_string());
    }
    Ok(ReconcileReport { lines: out, total_gap })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_law() {
        let src = "pub const OP_A: u8 = 0x40;\n\
                   pub const OP_B: u8 = 1;\n\
                   pub const OP_C: u16 = 0x50;\n\
                   pub  const\n  OP_D : u8 = 10 ;\n\
                   pub const OP_E: u8 = 0x1G;\n\
                   xpub const OP_F: u8 = 2;\n\
                   pub const OP_G:u8=5;\n\
                   pub const OP_H_M: u8 = 0x2a;\n\
                   pub const OP_I: u8 = 7\n\
                   pub const OP_J: u8 = 0x;\n\
                   pub constOP_K: u8 = 1;\n\
                   pub const OP_L: u8 = 010;\n";
        let got: Vec<(String, i64)> = scan_op_consts(src);
        assert_eq!(
            got,
            vec![
                ("OP_A".into(), 0x40),
                ("OP_B".into(), 1),
                ("OP_D".into(), 10),
                ("OP_F".into(), 2), // 无前导词界（Python 同）
                ("OP_G".into(), 5),
                ("OP_H_M".into(), 42),
                ("OP_L".into(), 10), // 前导零十进制（Python 崩溃面分歧注①）
            ]
        );
        assert_eq!(opn(1), "1");
        assert_eq!(opn(15), "15");
        assert_eq!(opn(16), "0x10");
        assert_eq!(opn(0x40), "0x40");
        assert_eq!(opn(0x100), "0x100");
    }

    fn tsv(rows: &[&str]) -> String {
        rows.iter().map(|r| format!("{r}\n")).collect()
    }

    /// 合成现场：仓库根目录 + interface_src 文件。
    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bctsrun-reconcile-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        d
    }

    #[test]
    fn gap_and_unreadable_lines() {
        let repo = scratch("gap");
        std::fs::write(repo.join("src/svc.rs"), "pub const OP_A: u8 = 1;\npub const OP_B: u8 = 0x11;\npub const OP_EXTRA: u8 = 9;\npub const OP_Z2: u8 = 0;\n").unwrap();
        let t = tsv(&[
            "META\t0\t31\t2\t31\tx\ty\tz",
            "CONTRACT\tsvc-x\tt\tiface\tsrc/svc.rs\t1\tsuite-x\tcase-a,case-b\tbsh",
            "OP\tsvc-x\t1\tA\tw",
            "OP\tsvc-x\t16\tB\tw",
            "OP\tsvc-x\t3\tZZZ\tw",
            "CASE\tsvc-x\tcase-a\t1\tpos\tseq\tg\ti\te",
            "CASE\tsvc-x\tcase-b\t16\tneg\treject\tg\ti\te",
            "CONTRACT\tsvc-y\tt\tiface2\tsrc/missing.rs\t1\tsuite-y\tcase-c\tbsh",
            "OP\tsvc-y\t2\tC\tw",
            "SEM\tsvc-x\tc\tf\tfrozen\tcase-a\tv\tn",
        ]);
        let r = reconcile_t2(&t, repo.to_str().unwrap()).unwrap();
        assert_eq!(r.total_gap, 3);
        assert_eq!(r.exit_code(), 1);
        assert_eq!(
            r.lines,
            vec![
                "bcts: T2 对账 [svc-x]（iface）：开放缺口——OP_B 错号（合同 0x10 vs crate 0x11）",
                "bcts: T2 对账 [svc-x]（iface）：开放缺口——OP_ZZZ 缺席（合同 op 3）",
                "bcts: T2 对账 [svc-x]：合同 op 表 3 项，crate 命中 1，差集 2；crate 侧多 2 op（add-only 合法不拦：OP_EXTRA,OP_Z2）",
                "bcts: T2 覆盖 [svc-x]：wire 用例 2 行（pos 1/neg 1）；五类 seq=1 reject=1 version=0 error=0 timeout=0——缺口类：version协商、错误域、超时失败隔离（如实记账不拦：Tier 2 开通条件随 #83 落数）",
                "bcts: T2 对账 [svc-y]：interface_src 不可读：src/missing.rs——开放缺口（合同②件单源指向断链）",
                "bcts: FAIL: T2 静态对账开放缺口 3 项（crate op 全集 ⊉ 合同表——如实报，不硬绿）",
            ]
        );
        std::fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn green_all_classes_and_dup_const() {
        let repo = scratch("green");
        // 同键后写覆盖前写（dict 赋值语义）：合同 A=2 命中。
        std::fs::write(repo.join("src/svc.rs"), "pub const OP_A: u8 = 1;\npub const OP_A: u8 = 2;\n").unwrap();
        let t = tsv(&[
            "CONTRACT\tsvc-x\tt\tiface\tsrc/svc.rs\t1\ts\tc1\tbsh",
            "OP\tsvc-x\t2\tA\tw",
            "CASE\tsvc-x\tc1\t2\tpos\tseq\tg\ti\te",
            "CASE\tsvc-x\tc2\t2\tneg\treject\tg\ti\te",
            "CASE\tsvc-x\tc3\t2\tpos\tversion\tg\ti\te",
            "CASE\tsvc-x\tc4\t2\tneg\terror\tg\ti\te",
            "CASE\tsvc-x\tc5\t2\tpos\ttimeout\tg\ti\te",
            "CASE\tsvc-x\tc6\t2\tneg\ttimeout\tg\ti\te",
        ]);
        let r = reconcile_t2(&t, repo.to_str().unwrap()).unwrap();
        assert_eq!(r.total_gap, 0);
        assert_eq!(r.exit_code(), 0);
        assert_eq!(
            r.lines,
            vec![
                "bcts: T2 对账 [svc-x]（iface）：合同 op 表 1 项，crate 命中 1，差集 0（开放缺口 = 0）；crate 侧多 0 op（add-only 合法不拦：无）",
                "bcts: T2 覆盖 [svc-x]：wire 用例 6 行（pos 3/neg 3）；五类 seq=1 reject=1 version=1 error=1 timeout=2",
                "bcts: T2 静态对账全绿（实现侧协议 crate op 全集 ⊇ 合同②件 op 表）",
            ]
        );
        // 空合同 TSV = 零合同全绿（Python 同）；目录占位 = is_file false 走
        // 断链开放缺口（Python os.path.isfile 同判序）；无效 UTF-8 文件 =
        // 读失败 Err（Python UnicodeDecodeError 崩溃面的诚实处理）。
        let r0 = reconcile_t2("META\t0\t31\t0\t0\tx\ty\tz\n", repo.to_str().unwrap()).unwrap();
        assert_eq!((r0.total_gap, r0.lines.len()), (0, 1));
        std::fs::create_dir_all(repo.join("src/dir.rs")).unwrap();
        let rd = reconcile_t2("CONTRACT\tc\tt\ti\tsrc/dir.rs\t1\ts\tx\tb\nOP\tc\t1\tA\tw\n", repo.to_str().unwrap()).unwrap();
        assert_eq!((rd.total_gap, rd.exit_code()), (1, 1));
        assert!(rd.lines[0].contains("interface_src 不可读"));
        std::fs::write(repo.join("src/bad.rs"), b"pub const OP_A: u8 = 1;\n\xff\xfe").unwrap();
        assert!(reconcile_t2("CONTRACT\tc\tt\ti\tsrc/bad.rs\t1\ts\tx\tb\n", repo.to_str().unwrap()).is_err());
        std::fs::remove_dir_all(&repo).ok();
    }
}
