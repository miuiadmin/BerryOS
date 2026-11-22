//! bctsrun 首刀：BCTS Tier 1 冻结件 schema 校验器（Rust 镜像——现役单源
//! `tools/bcts/t1_check.py`〔bcts-run.sh PYEOF①段机械抽出〕，彻底 Rust 化
//! 轨道 b 层 #7 刀序末档首刀）。
//!
//! 语面 = 冻结件 `tools/bcts/frozen/bcts@*.toml` 全集（bcts@0..6 + t2 两件
//! 的 T1 面不在此刀）：顶层八键 + `[[marker]]`/`[[host_gate]]` 两数组节；
//! 值域四型（串/bool/十进制 int）——冻结件语面无点分节/多行数组/内联表/
//! 转义/hex/日期，非目标形出现即拒（toml_doc 同界纪律，t1_check.py 同咬
//! 由 parity 钉板）。
//!
//! 拒面文案与 Python 逐字同源（`bcts: REJECT: <理由>` + 退 2——bcts-run.sh
//! 步①出口协议）；META/MARK/GATE TSV 发射序 = Python 同构（markers 后
//! gates、相内文件序）。
//!
//! 后续刀（依 §4 刀序）：PYEOF②（T2 冻结件校验器）/ PYEOF③（T2 静态对账）
//! / PYEOF④⑤（abi level 读器 / manifest PR 对账）/ 参数面与活树清单合成。

use std::collections::HashSet;

/// 拒面（文案与 t1_check.py 逐字同源；出口码 2 由 CLI 臂映射）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reject(pub String);

fn reject(msg: String) -> Reject {
    Reject(format!("bcts: REJECT: {msg}"))
}

/// TOML 子集值（冻结件现役语面全集）。
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Int(i64),
    Str(String),
    Bool(bool),
}

/// 解析拒（TOML 语法面——文案形与 tomllib 不同源，分支标识对齐由 parity 钉板）。
#[derive(Debug, PartialEq)]
pub struct TomlError(pub String);

/// 冻结件顶层键集（schema add-only——未知键硬拒）。
const TOP_KEYS: [&str; 7] = ["schema", "suite", "platform_revision", "frozen_at", "abi_snapshot", "abi_snapshot_hash", "tier1_floor"];
/// 数组节键（`[[…]]`——顶层展开单列一档）。
const ARRAY_KEYS: [&str; 2] = ["marker", "host_gate"];
const MARKER_KEYS: [&str; 5] = ["id", "expect", "phase", "tier", "source"];
const GATE_KEYS: [&str; 3] = ["id", "cmd", "tier"];
const PHASES: [&str; 3] = ["bare", "rootfs", "rx"];

/// 串外 `#` 定位（双引号开关状态机——子集无转义，串内 `#` 是值字符）。
fn outside_hash(line: &str) -> Option<usize> {
    let mut in_str = false;
    for (i, b) in line.bytes().enumerate() {
        match b {
            b'"' => in_str = !in_str,
            b'#' if !in_str => return Some(i),
            _ => {}
        }
    }
    None
}

/// 行清洗：去行尾注释（串外定位）+ CRLF/尾空白。
fn strip_comment(line: &str) -> &str {
    let base = line.trim_end_matches('\r');
    match outside_hash(base) {
        Some(i) => base[..i].trim_end(),
        None => base.trim_end(),
    }
}

/// 值解析（双引号串/bool/int——非目标形拒）。
fn parse_val(tok: &str, what: &str, line: u32) -> Result<Val, TomlError> {
    let t = tok.trim();
    if t == "true" || t == "false" {
        return Ok(Val::Bool(t == "true"));
    }
    if let Some(rest) = t.strip_prefix('"') {
        let rest = rest.strip_suffix('"').ok_or_else(|| TomlError(format!("TOML 解析失败： 行 {line} {what} 串未闭合")))?;
        if rest.is_empty() {
            return Ok(Val::Str(String::new()));
        }
        if rest.contains('"') {
            return Err(TomlError(format!("TOML 解析失败： 行 {line} {what} 串内双引号（子集无转义）")));
        }
        return Ok(Val::Str(rest.to_string()));
    }
    // int：可选负号 + 十进制数字（+ 前缀/hex/下划线 = 非目标形拒）
    let digits = t.strip_prefix('-').unwrap_or(t);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(v) = t.parse::<i64>() {
            return Ok(Val::Int(v));
        }
    }
    Err(TomlError(format!("TOML 解析失败： 行 {line} {what} 非目标值形：{t}")))
}

/// `[[marker]]`/`[[host_gate]]` 数组节头解析后的条目聚合结构。
#[derive(Debug, Default)]
pub struct FrozenDoc {
    /// 顶层键值（文件序）。
    pub top: Vec<(String, Val)>,
    /// `[[marker]]` 条目（文件序）——键值（文件序）。
    pub markers: Vec<Vec<(String, Val)>>,
    /// `[[host_gate]]` 条目（文件序）。
    pub gates: Vec<Vec<(String, Val)>>,
}

/// 冻结件 TOML 子集解析（单解释器——顶层键 + 两数组节；`[section]` 点分节/
/// 内联表/多行数组 = 非目标形拒）。
pub fn parse_frozen_toml(text: &str) -> Result<FrozenDoc, TomlError> {
    let mut doc = FrozenDoc::default();
    let mut seen_top: HashSet<String> = HashSet::new();
    let mut in_marker = false;
    let mut in_gate = false;

    for (idx, raw) in text.split('\n').enumerate() {
        let lineno = (idx + 1) as u32;
        let t = strip_comment(raw).trim();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("[[") {
            let name = rest.strip_suffix("]]").ok_or_else(|| TomlError(format!("TOML 解析失败： 行 {lineno} 数组节头非 [[name]] 形")))?.trim().to_string();
            if name.is_empty() || name.contains('"') || name.contains('.') {
                return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 数组节头非 [[name]] 形")));
            }
            match name.as_str() {
                "marker" => {
                    doc.markers.push(Vec::new());
                    in_marker = true;
                    in_gate = false;
                }
                "host_gate" => {
                    doc.gates.push(Vec::new());
                    in_gate = true;
                    in_marker = false;
                }
                _ => return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 数组节头 [[{name}]] 非冻结件目标形"))),
            }
            continue;
        }
        if t.starts_with('[') {
            return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 节头 [name] 非冻结件目标形（冻结件语面只有 [[marker]]/[[host_gate]] 数组节）")));
        }
        let eq = match t.find('=') {
            Some(i) => i,
            None => return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 非键值行形"))),
        };
        let key = t[..eq].trim();
        let key = key.strip_prefix('"').map(|k| k.strip_suffix('"').unwrap_or(k)).unwrap_or(key);
        let val = parse_val(&t[eq + 1..], key, lineno)?;
        match (in_marker, in_gate) {
            (true, _) => {
                let e = doc.markers.last_mut().unwrap();
                if e.iter().any(|(k, _)| k == key) {
                    return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 条目键 {key} 重复定义")));
                }
                e.push((key.to_string(), val));
            }
            (_, true) => {
                let e = doc.gates.last_mut().unwrap();
                if e.iter().any(|(k, _)| k == key) {
                    return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 条目键 {key} 重复定义")));
                }
                e.push((key.to_string(), val));
            }
            (false, false) => {
                if !seen_top.insert(key.to_string()) {
                    return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 顶层键 {key} 重复定义")));
                }
                doc.top.push((key.to_string(), val));
            }
        }
    }
    Ok(doc)
}

fn get<'a>(entries: &'a [(String, Val)], key: &str) -> Option<&'a Val> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn as_str(v: &Val) -> Option<&str> {
    match v {
        Val::Str(s) => Some(s),
        _ => None,
    }
}

/// Python repr 形（单引号串/True·False bool——文案 `实得 {x!r}` 段同源；
/// 控制字符 Python repr 转义面：\t \n \r \\）。
fn py_repr(v: &Val) -> String {
    match v {
        Val::Str(s) => {
            let esc = s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "\\r");
            format!("'{esc}'")
        }
        Val::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Val::Int(i) => i.to_string(),
    }
}

/// 未知键集报文段（Python `sorted(unknown)` 形——list repr 单引号逗号后空格）。
fn py_set_repr(keys: &[&str]) -> String {
    let mut sorted: Vec<&str> = keys.to_vec();
    sorted.sort_unstable();
    let quoted: Vec<String> = sorted.iter().map(|k| format!("'{k}'")).collect();
    format!("[{}]", quoted.join(", "))
}

/// Tier 1 冻结件全量校验（t1_check.py 镜像——拒面文案逐字同源）。
/// 返回 TSV 发射行序列（META/MARK/GATE——bcts-run.sh 步①出口协议）。
pub fn check_t1(doc: &FrozenDoc, path: &str) -> Result<Vec<String>, Reject> {
    // ---- 顶层键集执法 ----
    let top_names: Vec<&str> = doc.top.iter().map(|(k, _)| k.as_str()).collect();
    let unknown: Vec<&str> = top_names.iter().copied().filter(|k| !TOP_KEYS.contains(k) && !ARRAY_KEYS.contains(k)).collect();
    if !unknown.is_empty() {
        return Err(reject(format!("顶层未知键：{}（schema add-only、未知键硬拒——设计稿 §4.1）", py_set_repr(&unknown))));
    }
    let missing: Vec<&str> = TOP_KEYS.iter().copied().filter(|k| !top_names.contains(k)).collect();
    if !missing.is_empty() {
        return Err(reject(format!("顶层必填缺失：{}", py_set_repr(&missing))));
    }
    let schema = get(&doc.top, "schema").unwrap();
    if *schema != Val::Int(1) {
        return Err(reject(format!("schema 须 = 1，实得 {}", py_repr(schema))));
    }
    let suite = get(&doc.top, "suite").unwrap();
    if *suite != Val::Str("bcts".into()) {
        return Err(reject(format!("suite 须 = \"bcts\"，实得 {}", py_repr(suite))));
    }
    let pr_v = get(&doc.top, "platform_revision").unwrap();
    let pr = match pr_v {
        Val::Int(i) if *i >= 0 && *i <= u32::MAX as i64 => *i,
        other => return Err(reject(format!("platform_revision 须 u32 域 [0, 4294967295]（V9 同源词表，mkimage 同款），实得 {}", py_repr(other)))),
    };
    let frozen_at = get(&doc.top, "frozen_at").unwrap();
    let frozen_ok = matches!(as_str(frozen_at), Some(s) if s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    if !frozen_ok {
        return Err(reject(format!("frozen_at 须 40 位小写 commit sha，实得 {}", py_repr(frozen_at))));
    }
    let abi_snapshot = get(&doc.top, "abi_snapshot").unwrap();
    if as_str(abi_snapshot).unwrap_or("").is_empty() {
        return Err(reject("abi_snapshot 不得为空".into()));
    }
    let hash_v = get(&doc.top, "abi_snapshot_hash").unwrap();
    let hash_ok = matches!(as_str(hash_v), Some(s) if s.len() == 71 && s.starts_with("sha256:") && s[7..].bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    if !hash_ok {
        return Err(reject(format!("abi_snapshot_hash 须 sha256:<64 小写 hex>，实得 {}", py_repr(hash_v))));
    }
    let floor_v = get(&doc.top, "tier1_floor").unwrap();
    let floor = match floor_v {
        Val::Int(i) if *i >= 1 => *i,
        other => return Err(reject(format!("tier1_floor 须 ≥1 整数，实得 {}", py_repr(other)))),
    };

    // ---- [[marker]] 执法 ----
    if doc.markers.is_empty() {
        return Err(reject("[[marker]] 缺席或为空（marker 条目 = 冻结件最小单元，删除 = 破冻——设计稿 §4.1）".into()));
    }
    let mut ids: HashSet<String> = HashSet::new();
    for (i, m) in doc.markers.iter().enumerate() {
        let n = i + 1;
        let names: Vec<&str> = m.iter().map(|(k, _)| k.as_str()).collect();
        let unknown: Vec<&str> = names.iter().copied().filter(|k| !MARKER_KEYS.contains(k)).collect();
        if !unknown.is_empty() {
            return Err(reject(format!("marker[{n}] 未知键：{}", py_set_repr(&unknown))));
        }
        let missing: Vec<&str> = MARKER_KEYS.iter().copied().filter(|k| !names.contains(k)).collect();
        if !missing.is_empty() {
            return Err(reject(format!("marker[{n}] 必填缺失：{}", py_set_repr(&missing))));
        }
        let id = get(m, "id").unwrap();
        let id_s = as_str(id).unwrap_or("");
        if id_s.is_empty() || ids.contains(id_s) {
            return Err(reject(format!("marker id 空或重复：{}", py_repr(id))));
        }
        ids.insert(id_s.to_string());
        let e = get(m, "expect").unwrap();
        let e_bad = match e {
            Val::Str(s) => s.is_empty() || s.bytes().any(|b| matches!(b, b')' | b'"' | b'\x27' | b'\t' | b'\n')),
            _ => true,
        };
        if e_bad {
            return Err(reject(format!("marker {id_s} expect 违例（空/含引号/右括号/制表换行——markers.sh 单源约束沿袭）：{}", py_repr(e))));
        }
        let phase = get(m, "phase").unwrap();
        let phase_s = as_str(phase).unwrap_or("");
        if !PHASES.contains(&phase_s) {
            return Err(reject(format!("marker {id_s} phase 须 ∈ {{bare, rootfs, rx}}，实得 {}", py_repr(phase))));
        }
        let tier = get(m, "tier").unwrap();
        if *tier != Val::Int(1) {
            return Err(reject(format!("marker {id_s} tier 须 = 1（v0 只开通 Tier 1——设计稿 §3.2），实得 {}", py_repr(tier))));
        }
        let source = get(m, "source").unwrap();
        if as_str(source).unwrap_or("").is_empty() {
            return Err(reject(format!("marker {id_s} source 不得为空")));
        }
    }

    // ---- [[host_gate]] 执法 ----
    if doc.gates.is_empty() {
        return Err(reject("[[host_gate]] 缺席或为空（Tier 1 host 门禁 = abi-check 起步——设计稿 §4.2-④）".into()));
    }
    let mut gids: HashSet<String> = HashSet::new();
    for (i, g) in doc.gates.iter().enumerate() {
        let n = i + 1;
        let names: Vec<&str> = g.iter().map(|(k, _)| k.as_str()).collect();
        let unknown: Vec<&str> = names.iter().copied().filter(|k| !GATE_KEYS.contains(k)).collect();
        if !unknown.is_empty() {
            return Err(reject(format!("host_gate[{n}] 未知键：{}", py_set_repr(&unknown))));
        }
        let missing: Vec<&str> = GATE_KEYS.iter().copied().filter(|k| !names.contains(k)).collect();
        if !missing.is_empty() {
            return Err(reject(format!("host_gate[{n}] 必填缺失：{}", py_set_repr(&missing))));
        }
        let id = get(g, "id").unwrap();
        let id_s = as_str(id).unwrap_or("");
        if id_s.is_empty() || gids.contains(id_s) {
            return Err(reject(format!("host_gate id 空或重复：{}", py_repr(id))));
        }
        gids.insert(id_s.to_string());
        if as_str(get(g, "cmd").unwrap()).unwrap_or("").is_empty() {
            return Err(reject(format!("host_gate {id_s} cmd 不得为空")));
        }
        let tier = get(g, "tier").unwrap();
        if *tier != Val::Int(1) {
            return Err(reject(format!("host_gate {id_s} tier 须 = 1，实得 {}", py_repr(tier))));
        }
    }

    // 步②空集守卫。
    if (doc.markers.len() as i64) < floor {
        return Err(reject(format!("空集守卫：marker {} 件 < tier1_floor {floor}——拒跑防假绿", doc.markers.len())));
    }

    // 文件名 ↔ 字段冗余自证。
    let base = path.rsplit('/').next().unwrap_or(path);
    if base != format!("bcts@{pr}.toml") {
        return Err(reject(format!("文件名 '{}' 与 platform_revision={pr} 不符（bcts@<platform_revision>.toml 冗余自证）", base)));
    }

    // TSV 发射（出口协议）。
    let s = |k: &str| -> &str { as_str(get(&doc.top, k).unwrap()).unwrap() };
    let mut out = vec![format!("META\t{pr}\t{floor}\t{}\t{}\t{}\t{}\t{}",
        doc.markers.len(), doc.gates.len(), s("frozen_at"), s("abi_snapshot"), s("abi_snapshot_hash"))];
    for m in &doc.markers {
        out.push(format!("MARK\t{}\t{}\t{}",
            as_str(get(m, "phase").unwrap()).unwrap(),
            as_str(get(m, "id").unwrap()).unwrap(),
            as_str(get(m, "expect").unwrap()).unwrap()));
    }
    for g in &doc.gates {
        out.push(format!("GATE\t{}\t{}",
            as_str(get(g, "id").unwrap()).unwrap(),
            as_str(get(g, "cmd").unwrap()).unwrap()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> FrozenDoc {
        parse_frozen_toml(text).unwrap()
    }

    #[test]
    fn parse_basics() {
        let d = doc("schema = 1\nsuite = \"bcts\"\n[[marker]]\nid = \"bare-01\"\nexpect = \"X ok\"\nphase = \"bare\"\ntier = 1\nsource = \"s\"\n");
        assert_eq!(d.top.len(), 2);
        assert_eq!(d.markers.len(), 1);
        assert_eq!(get(&d.markers[0], "id"), Some(&Val::Str("bare-01".into())));
        assert_eq!(parse_val("42", "k", 1), Ok(Val::Int(42)));
        assert_eq!(parse_val("-7", "k", 1), Ok(Val::Int(-7)));
        assert!(parse_val("0x10", "k", 1).is_err()); // hex 非目标形
    }

    #[test]
    fn parse_rejects() {
        assert!(parse_frozen_toml("[a.b]\nx = 1\n").is_err()); // 点分节
        assert!(parse_frozen_toml("x = [1, 2]\n").is_err()); // 数组值
        assert!(parse_frozen_toml("k = \"a\" = 1\n").is_err());
        assert!(parse_frozen_toml("schema = 1\nschema = 2\n").is_err()); // 重复键
        assert!(parse_frozen_toml("[[other]]\nid = \"x\"\n").is_err()); // 非目标数组节
        assert!(parse_frozen_toml("x = 1 # 注释\ny = \"a#b\"\n").is_ok()); // 串外 # 才是注释
    }

    #[test]
    fn t1_reject_messages() {
        // 未知键 + 缺失键报文形
        let text = "schema = 1\nsuite = \"bcts\"\nbogus = \"x\"\n";
        let r = check_t1(&doc(text), "bcts@0.toml");
        assert!(r.is_err());
        let msg = r.unwrap_err().0;
        assert!(msg.starts_with("bcts: REJECT: 顶层未知键：['bogus']"), "{msg}");
        // marker expect 拒字面
        let text = GOOD.replace("expect = \"X ok\"", "expect = \"a)b\"");
        let r = check_t1(&doc(&text), "bcts@0.toml");
        assert!(r.unwrap_err().0.contains("expect 违例"));
    }

    #[test]
    fn t1_ok_emission() {
        let out = check_t1(&doc(GOOD), "bcts@0.toml").unwrap();
        assert_eq!(out[0], "META\t0\t1\t1\t1\t51932bff775c5da0534a41dae50c5ace7b8e945d\ttools/abi.toml\tsha256:d47c0177b1f4667b68780c2b462f7066fa3f5a0c157da436be358c409246b5c8");
        assert_eq!(out[1], "MARK\tbare\tbare-01\tX ok");
        assert_eq!(out[2], "GATE\tabi-check\tjust abi-check");
    }

    #[test]
    fn t1_floor_and_name_guards() {
        // floor > marker 数 → 空集守卫
        let text = GOOD.replace("tier1_floor = 1", "tier1_floor = 2");
        assert!(check_t1(&doc(&text), "bcts@0.toml").unwrap_err().0.contains("空集守卫"));
        // 文件名失配
        assert!(check_t1(&doc(GOOD), "bcts@5.toml").unwrap_err().0.contains("冗余自证"));
    }

    const GOOD: &str = "schema = 1\nsuite = \"bcts\"\nplatform_revision = 0\nfrozen_at = \"51932bff775c5da0534a41dae50c5ace7b8e945d\"\nabi_snapshot = \"tools/abi.toml\"\nabi_snapshot_hash = \"sha256:d47c0177b1f4667b68780c2b462f7066fa3f5a0c157da436be358c409246b5c8\"\ntier1_floor = 1\n[[marker]]\nid = \"bare-01\"\nexpect = \"X ok\"\nphase = \"bare\"\ntier = 1\nsource = \"s\"\n[[host_gate]]\nid = \"abi-check\"\ncmd = \"just abi-check\"\ntier = 1\n";
}
