//! bctsrun 二刀：BCTS Tier 2 冻结件 schema 校验器（Rust 镜像——现役单源
//! `tools/bcts/t2_check.py`〔bcts-run.sh PYEOF②段机械抽出〕，首刀 T1 面
//! 的续刀；彻底 Rust 化轨道 b 层 #7 刀序末档）。
//!
//! 语面 = 冻结件 `tools/bcts/frozen/bcts-t2@*.toml` 全集：顶层十键（含
//! `tier = 2`）+ `[[contract]]` 顶层数组节 + `[[contract.op]]` /
//! `[[contract.case]]` / `[[contract.semantics]]` 三嵌套数组节；值面在 T1
//! 子集（串/bool/十进制 int）上扩 hex int（`num = 0x40`）与字符串数组
//! （单行 + 多行：尾逗号/相内注释/串内 `,`·`#`）。点分普通节/内联表/
//! 浮点/嵌套数组/转义 = 非目标形拒（T1 同界纪律；tomllib 语法域文案不
//! 同源，分支标识由 parity 钉板）。
//!
//! 拒面文案与 Python 逐字同源（`bcts: REJECT: <理由>` + 退 2）；Python 语义
//! 面的三个坑显式镜像：bool 是 int 子类（`schema = true` 过 `!= 1` 检查、
//! `True` repr）；`sorted(集合)` list repr（`['a', 'b']`）；`{v!r}` 单引号
//! 串 repr。META/CONTRACT/OP/CASE/SEM TSV 发射序 = Python 同构（op→case→
//! semantics 相内文件序、组内条目文件序）。

use std::collections::HashSet;

use crate::{Reject, TomlError};

fn reject(msg: String) -> Reject {
    Reject(format!("bcts: REJECT: {msg}"))
}

/// T2 值树（tomllib 等价物：Table 保插入序——sorted()/发射序的语义基座）。
#[derive(Debug, Clone, PartialEq)]
pub enum T2Val {
    Int(i64),
    Str(String),
    Bool(bool),
    Arr(Vec<T2Val>),
    Table(Vec<(String, T2Val)>),
}

/// T2 冻结件解析产物（`[[contract]]` 及其三嵌套数组节——文件序）。
#[derive(Debug, Default)]
pub struct T2Doc {
    pub top: Vec<(String, T2Val)>,
    pub contracts: Vec<T2Contract>,
}

#[derive(Debug, Default)]
pub struct T2Contract {
    pub entries: Vec<(String, T2Val)>,
    pub ops: Vec<Vec<(String, T2Val)>>,
    pub cases: Vec<Vec<(String, T2Val)>>,
    pub sems: Vec<Vec<(String, T2Val)>>,
}

fn tbl_get<'a>(t: &'a [(String, T2Val)], k: &str) -> Option<&'a T2Val> {
    t.iter().find(|(key, _)| key == k).map(|(_, v)| v)
}

/// Python repr（T1 py_repr 的值域扩展：Arr list repr / Table dict repr；
/// 单引号串 + 控制字符 `\t \n \r \\` 转义；True/False；int 十进制——
/// hex 在 tomllib 侧解析即十进制化，本侧同构）。
fn py_repr(v: &T2Val) -> String {
    match v {
        T2Val::Str(s) => {
            let esc = s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "\\r");
            format!("'{esc}'")
        }
        T2Val::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        T2Val::Int(i) => i.to_string(),
        T2Val::Arr(xs) => {
            let parts: Vec<String> = xs.iter().map(py_repr).collect();
            format!("[{}]", parts.join(", "))
        }
        T2Val::Table(t) => {
            let parts: Vec<String> = t.iter().map(|(k, v)| format!("'{}': {}", k, py_repr(v))).collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// Python `sorted(set)` list repr（单引号逗号后空格）。
fn py_set_repr(keys: &[&str]) -> String {
    let mut sorted: Vec<&str> = keys.to_vec();
    sorted.sort_unstable();
    let quoted: Vec<String> = sorted.iter().map(|k| format!("'{k}'")).collect();
    format!("[{}]", quoted.join(", "))
}

/// Python 真值（空串/空表假、bool 本身、int 零假）。
fn py_truthy(v: &T2Val) -> bool {
    match v {
        T2Val::Bool(b) => *b,
        T2Val::Int(i) => *i != 0,
        T2Val::Str(s) => !s.is_empty(),
        T2Val::Arr(a) => !a.is_empty(),
        T2Val::Table(t) => !t.is_empty(),
    }
}

/// Python `v != n`（bool 是 int 子类：True == 1——schema/tier 检查面）。
fn py_ne_int(v: &T2Val, n: i64) -> bool {
    !matches!(v, T2Val::Int(i) if *i == n) && !matches!(v, T2Val::Bool(b) if (*b as i64) == n)
}

fn as_str(v: &T2Val) -> Option<&str> {
    match v {
        T2Val::Str(s) => Some(s),
        _ => None,
    }
}

// ---- TOML 子集解析（T2 语面） ----

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

fn strip_comment(line: &str) -> &str {
    let base = line.trim_end_matches('\r');
    match outside_hash(base) {
        Some(i) => base[..i].trim_end(),
        None => base.trim_end(),
    }
}

/// 值解析（T2 子集：双引号串/bool/十进制 int/hex int——非目标形拒）。
fn parse_val2(tok: &str, what: &str, line: u32) -> Result<T2Val, TomlError> {
    let t = tok.trim();
    if t == "true" || t == "false" {
        return Ok(T2Val::Bool(t == "true"));
    }
    if let Some(rest) = t.strip_prefix('"') {
        let rest = rest.strip_suffix('"').ok_or_else(|| TomlError(format!("TOML 解析失败： 行 {line} {what} 串未闭合")))?;
        if rest.is_empty() {
            return Ok(T2Val::Str(String::new()));
        }
        if rest.contains('"') {
            return Err(TomlError(format!("TOML 解析失败： 行 {line} {what} 串内双引号（子集无转义）")));
        }
        return Ok(T2Val::Str(rest.to_string()));
    }
    // hex int（冻结件 op 号形：0x40）
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            if let Ok(v) = i64::from_str_radix(hex, 16) {
                return Ok(T2Val::Int(v));
            }
        }
        return Err(TomlError(format!("TOML 解析失败： 行 {line} {what} 非目标值形：{t}")));
    }
    // 十进制 int（可选负号——u8/u32 域拒属 schema 面，解析层放行）
    let digits = t.strip_prefix('-').unwrap_or(t);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(v) = t.parse::<i64>() {
            return Ok(T2Val::Int(v));
        }
    }
    Err(TomlError(format!("TOML 解析失败： 行 {line} {what} 非目标值形：{t}")))
}

/// 数组元素切分（串感知逗号——串内 `,` 不是分隔符；尾逗号合法、
/// 中空元素拒；元素限标量——嵌套数组非目标形拒）。
fn parse_arr_elems(buf: &str, what: &str, line: u32) -> Result<Vec<T2Val>, TomlError> {
    let mut elems: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in buf.chars() {
        match ch {
            '"' => {
                in_str = !in_str;
                cur.push(ch);
            }
            ',' if !in_str => {
                elems.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    elems.push(cur);
    let n = elems.len();
    let mut out = Vec::new();
    for (i, e) in elems.iter().enumerate() {
        if e.trim().is_empty() {
            if i + 1 == n {
                continue; // 尾逗号 / 空数组
            }
            return Err(TomlError(format!("TOML 解析失败： 行 {line} {what} 数组中空元素（连续逗号）")));
        }
        if e.trim().starts_with('[') {
            return Err(TomlError(format!("TOML 解析失败： 行 {line} {what} 嵌套数组非目标形")));
        }
        out.push(parse_val2(e, what, line)?);
    }
    Ok(out)
}

/// 向数组累加器喂一行（串感知：`#` 相外起注释跳行尾、`]` 相外闭合、
/// 换行折空白）。返回 true = 闭合。
fn scan_into(buf: &mut String, s: &str) -> bool {
    let mut in_str = false;
    for ch in s.chars() {
        if in_str {
            buf.push(ch);
            if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_str = true;
                buf.push(ch);
            }
            '#' => return false,
            ']' => return true,
            '\n' | '\r' => buf.push(' '),
            _ => buf.push(ch),
        }
    }
    false
}

/// 键值落位（条目内重复键拒——TOML 单一定义律）。
fn push_kv(dst: &mut Vec<(String, T2Val)>, key: &str, val: T2Val, lineno: u32, top: bool) -> Result<(), TomlError> {
    if dst.iter().any(|(k, _)| k == key) {
        let what = if top { "顶层键" } else { "条目键" };
        return Err(TomlError(format!("TOML 解析失败： 行 {lineno} {what} {key} 重复定义")));
    }
    dst.push((key.to_string(), val));
    Ok(())
}

/// T2 冻结件 TOML 子集解析（单解释器——顶层键 + `[[contract]]` +
/// 三嵌套数组节；点分普通节/内联表/浮点/转义 = 非目标形拒）。
/// `[[contract.op]]` 先于 `[[contract]]` 出现 = 隐式开新合同（tomllib
/// 同构——后续 schema 面必填缺失拒兜底）。
pub fn parse_frozen_toml_t2(text: &str) -> Result<T2Doc, TomlError> {
    let mut doc = T2Doc::default();
    enum Cur {
        Top,
        C,
        Op,
        Case,
        Sem,
    }
    let mut cur = Cur::Top;
    let mut pending: Option<(String, String)> = None; // (key, buf) 多行数组

    macro_rules! assign {
        ($key:expr, $val:expr, $ln:expr) => {
            match cur {
                Cur::Top => push_kv(&mut doc.top, $key, $val, $ln, true)?,
                Cur::C => push_kv(&mut doc.contracts.last_mut().unwrap().entries, $key, $val, $ln, false)?,
                Cur::Op => push_kv(doc.contracts.last_mut().unwrap().ops.last_mut().unwrap(), $key, $val, $ln, false)?,
                Cur::Case => push_kv(doc.contracts.last_mut().unwrap().cases.last_mut().unwrap(), $key, $val, $ln, false)?,
                Cur::Sem => push_kv(doc.contracts.last_mut().unwrap().sems.last_mut().unwrap(), $key, $val, $ln, false)?,
            }
        };
    }

    for (idx, raw) in text.split('\n').enumerate() {
        let lineno = (idx + 1) as u32;
        if let Some((_key, buf)) = pending.as_mut() {
            let closed = scan_into(buf, raw);
            if closed {
                let (k, buf) = pending.take().unwrap();
                let elems = parse_arr_elems(&buf, &k, lineno)?;
                let key = k.as_str();
                assign!(key, T2Val::Arr(elems), lineno);
            }
            continue;
        }
        let t = strip_comment(raw).trim();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("[[") {
            let name = rest.strip_suffix("]]").ok_or_else(|| TomlError(format!("TOML 解析失败： 行 {lineno} 数组节头非 [[name]] 形")))?.trim().to_string();
            if name.is_empty() || name.contains('"') {
                return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 数组节头非 [[name]] 形")));
            }
            match name.as_str() {
                "contract" => {
                    doc.contracts.push(T2Contract::default());
                    cur = Cur::C;
                }
                "contract.op" => {
                    let last = doc.contracts.last_mut();
                    if last.is_none() {
                        doc.contracts.push(T2Contract::default());
                    }
                    doc.contracts.last_mut().unwrap().ops.push(Vec::new());
                    cur = Cur::Op;
                }
                "contract.case" => {
                    let last = doc.contracts.last_mut();
                    if last.is_none() {
                        doc.contracts.push(T2Contract::default());
                    }
                    doc.contracts.last_mut().unwrap().cases.push(Vec::new());
                    cur = Cur::Case;
                }
                "contract.semantics" => {
                    let last = doc.contracts.last_mut();
                    if last.is_none() {
                        doc.contracts.push(T2Contract::default());
                    }
                    doc.contracts.last_mut().unwrap().sems.push(Vec::new());
                    cur = Cur::Sem;
                }
                _ => return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 数组节头 [[{name}]] 非冻结件目标形"))),
            }
            continue;
        }
        if t.starts_with('[') {
            return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 节头 [name] 非冻结件目标形（T2 语面只有 [[contract]] 族数组节）")));
        }
        let eq = match t.find('=') {
            Some(i) => i,
            None => return Err(TomlError(format!("TOML 解析失败： 行 {lineno} 非键值行形"))),
        };
        let key = t[..eq].trim();
        let key = key.strip_prefix('"').map(|k| k.strip_suffix('"').unwrap_or(k)).unwrap_or(key);
        let v = t[eq + 1..].trim();
        if let Some(inner) = v.strip_prefix('[') {
            let mut buf = String::new();
            if scan_into(&mut buf, inner) {
                let elems = parse_arr_elems(&buf, key, lineno)?;
                assign!(key, T2Val::Arr(elems), lineno);
            } else {
                pending = Some((key.to_string(), buf));
            }
        } else {
            let val = parse_val2(v, key, lineno)?;
            assign!(key, val, lineno);
        }
    }
    if pending.is_some() {
        return Err(TomlError("TOML 解析失败： 文件尾数组未闭合".into()));
    }
    Ok(doc)
}

// ---- 校验执法（t2_check.py 镜像——拒面文案逐字同源） ----

const TOP_KEYS: [&str; 9] = ["schema", "suite", "tier", "platform_revision", "frozen_at", "abi_snapshot", "abi_snapshot_hash", "tier2_floor", "contract_doc"];
const ARRAY_KEYS: [&str; 1] = ["contract"];
const CONTRACT_KEYS: [&str; 8] = ["id", "title", "interface", "interface_src", "ver_min", "consumers", "suite_id", "suite_cases"];
const CONTRACT_ARRAY_KEYS: [&str; 3] = ["op", "case", "semantics"];
const OP_KEYS: [&str; 3] = ["num", "name", "wire"];
const CASE_KEYS: [&str; 7] = ["id", "op", "kind", "class", "input", "expect", "group"];
const SEM_KEYS: [&str; 6] = ["clause", "value", "facet", "cases", "status", "note"];
const KINDS: [&str; 2] = ["pos", "neg"];
const CLASSES: [&str; 5] = ["seq", "reject", "version", "error", "timeout"];
const FACETS: [&str; 11] = ["串行模型", "错误码", "帽执法", "挂载语义", "诚实义务", "档位声明", "失败隔离", "时延", "协商", "生命周期", "呈现原子"];
const STATUS: [&str; 2] = ["frozen", "deferred"];

fn is_lower_hex40(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn is_sha256(s: &str) -> bool {
    s.len() == 71 && s.starts_with("sha256:") && s[7..].bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn is_cid(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_op_name(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    cs.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// clean（t2_check.py 同咬）：str 非空且无制表/换行。
fn clean<'a>(v: &'a T2Val, what: &str) -> Result<&'a str, Reject> {
    match v {
        T2Val::Str(s) if !s.is_empty() && !s.contains(['\t', '\n', '\r']) => Ok(s),
        other => Err(reject(format!("{what} 违例（空/含制表换行）：{}", py_repr(other)))),
    }
}

/// u8 域（bool 显式拒——isinstance(v, bool) 先于 int 判）。
fn u8(v: &T2Val, what: &str) -> Result<i64, Reject> {
    match v {
        T2Val::Int(i) if (0..=255).contains(i) => Ok(*i),
        other => Err(reject(format!("{what} 须 u8 域 [0,255]，实得 {}", py_repr(other)))),
    }
}

/// Tier 2 冻结件全量校验（t2_check.py 镜像）。返回 TSV 发射行序列
/// （META/CONTRACT/OP/CASE/SEM——bcts-run.sh t2_load 出口协议）。
pub fn check_t2(doc: &T2Doc, path: &str) -> Result<Vec<String>, Reject> {
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
    let g = |k: &str| -> &T2Val { tbl_get(&doc.top, k).unwrap() };
    if py_ne_int(g("schema"), 1) {
        return Err(reject(format!("schema 须 = 1，实得 {}", py_repr(g("schema")))));
    }
    if g("suite") != &T2Val::Str("bcts".into()) {
        return Err(reject(format!("suite 须 = \"bcts\"，实得 {}", py_repr(g("suite")))));
    }
    if py_ne_int(g("tier"), 2) {
        return Err(reject(format!("tier 须 = 2（bcts-t2 工件），实得 {}", py_repr(g("tier")))));
    }
    let pr = match g("platform_revision") {
        T2Val::Int(i) if (0..=u32::MAX as i64).contains(i) => *i,
        other => return Err(reject(format!("platform_revision 须 u32 域 [0, 4294967295]（V9 同源词表），实得 {}", py_repr(other)))),
    };
    if !matches!(as_str(g("frozen_at")), Some(s) if is_lower_hex40(s)) {
        return Err(reject(format!("frozen_at 须 40 位小写 commit sha，实得 {}", py_repr(g("frozen_at")))));
    }
    if !py_truthy(g("abi_snapshot")) {
        return Err(reject("abi_snapshot 不得为空".into()));
    }
    if !matches!(as_str(g("abi_snapshot_hash")), Some(s) if is_sha256(s)) {
        return Err(reject(format!("abi_snapshot_hash 须 sha256:<64 小写 hex>，实得 {}", py_repr(g("abi_snapshot_hash")))));
    }
    let floor = match g("tier2_floor") {
        T2Val::Int(i) if *i >= 1 => *i,
        other => return Err(reject(format!("tier2_floor 须 ≥1 整数，实得 {}", py_repr(other)))),
    };
    clean(g("contract_doc"), "contract_doc")?;

    // ---- [[contract]] 族执法 ----
    if doc.contracts.is_empty() {
        return Err(reject("[[contract]] 缺席或为空（每开放槽位 ≥ 合同④件全集——设计稿 §3.2）".into()));
    }
    let mut cids: HashSet<String> = HashSet::new();
    let mut case_ids: HashSet<String> = HashSet::new();
    let mut n_cases: usize = 0;
    for (i, c) in doc.contracts.iter().enumerate() {
        let n = i + 1;
        let mut names: Vec<&str> = c.entries.iter().map(|(k, _)| k.as_str()).collect();
        if !c.ops.is_empty() {
            names.push("op");
        }
        if !c.cases.is_empty() {
            names.push("case");
        }
        if !c.sems.is_empty() {
            names.push("semantics");
        }
        let unknown: Vec<&str> = names.iter().copied().filter(|k| !CONTRACT_KEYS.contains(k) && !CONTRACT_ARRAY_KEYS.contains(k)).collect();
        if !unknown.is_empty() {
            return Err(reject(format!("contract[{n}] 未知键：{}", py_set_repr(&unknown))));
        }
        let missing: Vec<&str> = CONTRACT_KEYS.iter().copied().filter(|k| !names.contains(k)).collect();
        if !missing.is_empty() {
            return Err(reject(format!("contract[{n}] 必填缺失：{}", py_set_repr(&missing))));
        }
        let ce = |k: &str| -> &T2Val { tbl_get(&c.entries, k).unwrap() };
        let cid = clean(ce("id"), &format!("contract[{n}].id"))?.to_string();
        if cids.contains(&cid) {
            return Err(reject(format!("contract id 重复：'{cid}'")));
        }
        cids.insert(cid.clone());
        if !is_cid(&cid) {
            return Err(reject(format!("contract id 须 <层>-<名> 连字符串（合同 §2.6 命名规范），实得 '{cid}'")));
        }
        clean(ce("title"), &format!("{cid}.title"))?;
        clean(ce("interface"), &format!("{cid}.interface"))?;
        clean(ce("interface_src"), &format!("{cid}.interface_src"))?;
        match ce("ver_min") {
            T2Val::Int(v) if *v >= 1 => {}
            other => return Err(reject(format!("{cid} ver_min 须 ≥1 整数，实得 {}", py_repr(other)))),
        }
        let consumers = match ce("consumers") {
            T2Val::Arr(a) if !a.is_empty() => a,
            _ => return Err(reject(format!("{cid} consumers 须非空列表"))),
        };
        for x in consumers {
            clean(x, &format!("{cid}.consumers 元素"))?;
        }
        clean(ce("suite_id"), &format!("{cid}.suite_id"))?;
        let suite_cases = match ce("suite_cases") {
            T2Val::Arr(a) if !a.is_empty() => a,
            _ => return Err(reject(format!("{cid} suite_cases 须非空列表（合同④件骨架照录）"))),
        };
        let mut sc_ids: HashSet<&str> = HashSet::new();
        for x in suite_cases {
            let s = clean(x, &format!("{cid}.suite_cases 元素"))?;
            if !sc_ids.insert(s) {
                return Err(reject(format!("{cid} suite_cases 重复：'{s}'")));
            }
        }

        // [[contract.op]]
        if c.ops.is_empty() {
            return Err(reject(format!("{cid} [[contract.op]] 缺席或为空（②件 op 表 = 静态对账数据源）")));
        }
        let mut op_nums: Vec<(i64, String)> = Vec::new();
        for (j, o) in c.ops.iter().enumerate() {
            let m = j + 1;
            let names: Vec<&str> = o.iter().map(|(k, _)| k.as_str()).collect();
            let unknown: Vec<&str> = names.iter().copied().filter(|k| !OP_KEYS.contains(k)).collect();
            if !unknown.is_empty() {
                return Err(reject(format!("{cid}.op[{m}] 未知键：{}", py_set_repr(&unknown))));
            }
            let missing: Vec<&str> = OP_KEYS.iter().copied().filter(|k| !names.contains(k)).collect();
            if !missing.is_empty() {
                return Err(reject(format!("{cid}.op[{m}] 必填缺失：{}", py_set_repr(&missing))));
            }
            let oe = |k: &str| -> &T2Val { tbl_get(o, k).unwrap() };
            let num = u8(oe("num"), &format!("{cid}.op[{m}].num"))?;
            let name = clean(oe("name"), &format!("{cid}.op[{m}].name"))?.to_string();
            if !is_op_name(&name) {
                return Err(reject(format!("{cid}.op[{m}] name 须协议 crate OP_ 常量后缀形，实得 '{name}'")));
            }
            if let Some((_, prev)) = op_nums.iter().find(|(k, _)| *k == num) {
                return Err(reject(format!("{cid}.op[{m}] op 号重复：{num}（{prev} 已占）")));
            }
            op_nums.push((num, name));
            clean(oe("wire"), &format!("{cid}.op[{m}].wire"))?;
        }

        // [[contract.case]]（group 锚定域 = suite_cases ∪ ③条款键——预扫
        // semantics，锚定域在前映射表在后）。
        if c.cases.is_empty() {
            return Err(reject(format!("{cid} [[contract.case]] 缺席或为空（④件用例集 = 冻结最小单元）")));
        }
        let clause_keys: HashSet<&str> = c.sems.iter().filter_map(|s| tbl_get(s, "clause").and_then(as_str)).collect();
        let mut pos_ops: HashSet<i64> = HashSet::new();
        let mut neg_ops: HashSet<i64> = HashSet::new();
        for (j, x) in c.cases.iter().enumerate() {
            let m = j + 1;
            let names: Vec<&str> = x.iter().map(|(k, _)| k.as_str()).collect();
            let unknown: Vec<&str> = names.iter().copied().filter(|k| !CASE_KEYS.contains(k)).collect();
            if !unknown.is_empty() {
                return Err(reject(format!("{cid}.case[{m}] 未知键：{}", py_set_repr(&unknown))));
            }
            let missing: Vec<&str> = CASE_KEYS.iter().copied().filter(|k| *k != "group" && !names.contains(k)).collect();
            if !missing.is_empty() {
                return Err(reject(format!("{cid}.case[{m}] 必填缺失：{}", py_set_repr(&missing))));
            }
            let xe = |k: &str| -> &T2Val { tbl_get(x, k).unwrap() };
            let xid = clean(xe("id"), &format!("{cid}.case[{m}].id"))?.to_string();
            if case_ids.contains(&xid) {
                return Err(reject(format!("case id 全局重复：'{xid}'")));
            }
            case_ids.insert(xid.clone());
            let op = u8(xe("op"), &format!("{cid}.case[{m}] {xid} op"))?;
            if !KINDS.contains(&as_str(xe("kind")).unwrap_or("")) {
                return Err(reject(format!("{cid}.case[{m}] {xid} kind 须 ∈ {{pos, neg}}，实得 {}", py_repr(xe("kind")))));
            }
            if !CLASSES.contains(&as_str(xe("class")).unwrap_or("")) {
                return Err(reject(format!("{cid}.case[{m}] {xid} class 须五类之一（设计稿 §3.2 注 a），实得 {}", py_repr(xe("class")))));
            }
            clean(xe("input"), &format!("{cid}.case[{m}] {xid} input"))?;
            clean(xe("expect"), &format!("{cid}.case[{m}] {xid} expect"))?;
            if let Some(gv) = tbl_get(x, "group") {
                let gv2 = clean(gv, &format!("{cid}.case[{m}] {xid} group"))?;
                if !sc_ids.contains(gv2) && !clause_keys.contains(gv2) {
                    return Err(reject(format!("{cid}.case[{m}] {xid} group '{gv2}' 须 ∈ suite_cases ∪ semantics 条款键（未锚定）")));
                }
            }
            if as_str(xe("kind")) == Some("pos") {
                pos_ops.insert(op);
            } else {
                neg_ops.insert(op);
            }
        }
        n_cases += c.cases.len();
        // 逐 op 覆盖守卫（每 op ≥1 正例 ≥1 负例——num 升序）。
        let mut sorted_ops = op_nums.clone();
        sorted_ops.sort_by_key(|(num, _)| *num);
        for (num, name) in sorted_ops {
            if !pos_ops.contains(&num) {
                return Err(reject(format!("{cid} op {num}({name}) 无正例行（每 op ≥1 正例——空集守卫逐槽口径）")));
            }
            if !neg_ops.contains(&num) {
                return Err(reject(format!("{cid} op {num}({name}) 无负例行（每 op ≥1 负例——空集守卫逐槽口径）")));
            }
        }

        // [[contract.semantics]]
        if c.sems.is_empty() {
            return Err(reject(format!("{cid} [[contract.semantics]] 缺席或为空（③→④映射表 = 合同语义条款逐条）")));
        }
        let mut seen_clauses: HashSet<&str> = HashSet::new();
        let c_id_set: HashSet<&str> = c.cases.iter().filter_map(|x| tbl_get(x, "id").and_then(as_str)).collect();
        for (j, s) in c.sems.iter().enumerate() {
            let m = j + 1;
            let names: Vec<&str> = s.iter().map(|(k, _)| k.as_str()).collect();
            let unknown: Vec<&str> = names.iter().copied().filter(|k| !SEM_KEYS.contains(k)).collect();
            if !unknown.is_empty() {
                return Err(reject(format!("{cid}.semantics[{m}] 未知键：{}", py_set_repr(&unknown))));
            }
            let missing: Vec<&str> = SEM_KEYS.iter().copied().filter(|k| *k != "note" && !names.contains(k)).collect();
            if !missing.is_empty() {
                return Err(reject(format!("{cid}.semantics[{m}] 必填缺失：{}", py_set_repr(&missing))));
            }
            let se = |k: &str| -> &T2Val { tbl_get(s, k).unwrap() };
            let cl = clean(se("clause"), &format!("{cid}.semantics[{m}].clause"))?;
            if !seen_clauses.insert(cl) {
                return Err(reject(format!("{cid}.semantics clause 重复：'{cl}'")));
            }
            clean(se("value"), &format!("{cid}.semantics[{m}] {cl} value"))?;
            if !FACETS.contains(&as_str(se("facet")).unwrap_or("")) {
                return Err(reject(format!("{cid}.semantics[{m}] {cl} facet 值域外：{}", py_repr(se("facet")))));
            }
            if !STATUS.contains(&as_str(se("status")).unwrap_or("")) {
                return Err(reject(format!("{cid}.semantics[{m}] {cl} status 须 frozen|deferred，实得 {}", py_repr(se("status")))));
            }
            let sem_cases = match se("cases") {
                T2Val::Arr(a) => a.clone(),
                _ => return Err(reject(format!("{cid}.semantics[{m}] {cl} cases 须列表"))),
            };
            for x in &sem_cases {
                let xs = clean(x, &format!("{cid}.semantics[{m}] {cl} cases 元素"))?;
                if !c_id_set.contains(xs) && !sc_ids.contains(xs) {
                    return Err(reject(format!("{cid}.semantics[{m}] {cl} cases 引用未锚定用例：'{xs}'")));
                }
            }
            if as_str(se("status")) == Some("frozen") && sem_cases.is_empty() {
                return Err(reject(format!("{cid}.semantics[{m}] {cl} status=frozen 但 cases 空（frozen 须映射已冻结 wire 行）")));
            }
            if let Some(note) = tbl_get(s, "note") {
                clean(note, &format!("{cid}.semantics[{m}] {cl} note"))?;
            }
        }
    }

    // 空集守卫（回放侧；冻结侧同式执法——守卫两层，设计稿 §4.4）。
    if (n_cases as i64) < floor {
        return Err(reject(format!("空集守卫：case {n_cases} 行 < tier2_floor {floor}——拒跑防假绿")));
    }

    // 文件名 ↔ 字段冗余自证（版本绑定的机器表达，设计稿 §2.4-1）。
    let base = path.rsplit('/').next().unwrap_or(path);
    if base != format!("bcts-t2@{pr}.toml") {
        return Err(reject(format!("文件名 '{base}' 与 platform_revision={pr} 不符（bcts-t2@<platform_revision>.toml 冗余自证）")));
    }

    // ---- TSV 发射（出口协议） ----
    let top_s = |k: &str| -> &str { as_str(tbl_get(&doc.top, k).unwrap()).unwrap() };
    let mut out = vec![format!("META\t{pr}\t{floor}\t{}\t{n_cases}\t{}\t{}\t{}",
        doc.contracts.len(), top_s("frozen_at"), top_s("abi_snapshot"), top_s("abi_snapshot_hash"))];
    for c in &doc.contracts {
        let ce = |k: &str| -> &T2Val { tbl_get(&c.entries, k).unwrap() };
        let join = |a: &T2Val| -> String {
            match a {
                T2Val::Arr(xs) => xs.iter().filter_map(as_str).collect::<Vec<_>>().join(","),
                _ => String::new(),
            }
        };
        let cid = as_str(ce("id")).unwrap();
        out.push(format!("CONTRACT\t{cid}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            as_str(ce("title")).unwrap(), as_str(ce("interface")).unwrap(), as_str(ce("interface_src")).unwrap(),
            match ce("ver_min") { T2Val::Int(v) => v.to_string(), _ => String::new() },
            as_str(ce("suite_id")).unwrap(), join(ce("suite_cases")), join(ce("consumers"))));
        for o in &c.ops {
            out.push(format!("OP\t{cid}\t{}\t{}\t{}",
                match tbl_get(o, "num").unwrap() { T2Val::Int(v) => v.to_string(), _ => String::new() },
                as_str(tbl_get(o, "name").unwrap()).unwrap(), as_str(tbl_get(o, "wire").unwrap()).unwrap()));
        }
        for x in &c.cases {
            out.push(format!("CASE\t{cid}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                as_str(tbl_get(x, "id").unwrap()).unwrap(),
                match tbl_get(x, "op").unwrap() { T2Val::Int(v) => v.to_string(), _ => String::new() },
                as_str(tbl_get(x, "kind").unwrap()).unwrap(), as_str(tbl_get(x, "class").unwrap()).unwrap(),
                tbl_get(x, "group").and_then(as_str).unwrap_or("-"),
                as_str(tbl_get(x, "input").unwrap()).unwrap(), as_str(tbl_get(x, "expect").unwrap()).unwrap()));
        }
        for s in &c.sems {
            out.push(format!("SEM\t{cid}\t{}\t{}\t{}\t{}\t{}\t{}",
                as_str(tbl_get(s, "clause").unwrap()).unwrap(), as_str(tbl_get(s, "facet").unwrap()).unwrap(),
                as_str(tbl_get(s, "status").unwrap()).unwrap(), join(tbl_get(s, "cases").unwrap()),
                as_str(tbl_get(s, "value").unwrap()).unwrap(), tbl_get(s, "note").and_then(as_str).unwrap_or("")));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> T2Doc {
        parse_frozen_toml_t2(text).unwrap()
    }

    #[test]
    fn parse2_basics() {
        let d = doc("schema = 1\ntier = 2\n[[contract]]\nid = \"svc-x\"\n[[contract.op]]\nnum = 0x40\nname = \"GET\"\nwire = \"w\"\n");
        assert_eq!(d.top.len(), 2);
        assert_eq!(d.contracts.len(), 1);
        assert_eq!(d.contracts[0].ops.len(), 1);
        assert_eq!(tbl_get(&d.contracts[0].ops[0], "num"), Some(&T2Val::Int(0x40)));
        assert_eq!(parse_val2("0X1F", "k", 1), Ok(T2Val::Int(31)));
        assert_eq!(parse_val2("-7", "k", 1), Ok(T2Val::Int(-7)));
        // 多行数组：尾逗号 + 相内注释 + 串内逗号/井号
        let d2 = doc("suite_cases = [\n  \"a,b\",\n  \"c#d\", # 相内注释\n]\n");
        assert_eq!(tbl_get(&d2.top, "suite_cases"), Some(&T2Val::Arr(vec![T2Val::Str("a,b".into()), T2Val::Str("c#d".into())])));
        // 单行数组
        let d3 = doc("consumers = [\"bsh\", \"app\"]\n");
        assert_eq!(tbl_get(&d3.top, "consumers"), Some(&T2Val::Arr(vec![T2Val::Str("bsh".into()), T2Val::Str("app".into())])));
    }

    #[test]
    fn parse2_rejects() {
        assert!(parse_frozen_toml_t2("[a.b]\nx = 1\n").is_err()); // 点分普通节
        assert!(parse_frozen_toml_t2("a = {b = 1}\n").is_err()); // 内联表
        assert!(parse_frozen_toml_t2("x = 1.5\n").is_err()); // 浮点
        assert!(parse_frozen_toml_t2("x = [[1]]\n").is_err()); // 嵌套数组
        assert!(parse_frozen_toml_t2("x = [1,\n2\n").is_err()); // 文件尾未闭合
        assert!(parse_frozen_toml_t2("k = 1\nk = 2\n").is_err()); // 顶层重复键
        assert!(parse_frozen_toml_t2("[[bogus]]\nid = \"x\"\n").is_err()); // 非目标数组节
        assert!(parse_frozen_toml_t2("x = [1,, 2]\n").is_err()); // 中空元素
        assert!(parse_frozen_toml_t2("x = \"a\\\"b\"\n").is_err()); // 转义（子集无转义）
    }

    #[test]
    fn t2_ok_emission() {
        let out = check_t2(&doc(GOOD), "bcts-t2@0.toml").unwrap();
        assert_eq!(out[0], "META\t0\t1\t1\t4\t51932bff775c5da0534a41dae50c5ace7b8e945d\ttools/abi.toml\tsha256:d47c0177b1f4667b68780c2b462f7066fa3f5a0c157da436be358c409246b5c8");
        assert_eq!(out[1], "CONTRACT\tsvc-x\tT\tberry-x\tuser/berry-x/src/lib.rs\t1\tbcts-l1-x-v1\tsc-1\tbsh");
        assert_eq!(out[2], "OP\tsvc-x\t1\tPUT\treq 1B");
        assert_eq!(out[3], "OP\tsvc-x\t64\tGET\treq 1B"); // hex → 十进制发射
        assert_eq!(out[4], "CASE\tsvc-x\tx-put-pos\t1\tpos\tseq\tsc-1\ti\te");
        assert_eq!(out[5], "CASE\tsvc-x\tx-put-neg\t1\tneg\treject\t-\ti\te");
        assert_eq!(out[8], "SEM\tsvc-x\tc-1\t错误码\tfrozen\tx-put-pos\tv\t");
        assert_eq!(out.len(), 9);
    }

    #[test]
    fn t2_reject_messages() {
        // schema = true 过 py_ne（bool 是 int 子类：True == 1）→ 与 Python 同
        // 行为不拒（py 面同语料真跑核过：exit 0 全量通过）。
        let text = GOOD.replace("schema = 1", "schema = true");
        assert!(check_t2(&doc(&text), "bcts-t2@0.toml").is_ok(), "True==1 应放行");
        // group 未锚定
        let text = GOOD.replace("group = \"sc-1\"", "group = \"nope\"");
        assert!(check_t2(&doc(&text), "bcts-t2@0.toml").unwrap_err().0.contains("未锚定"));
        // 覆盖守卫：删 x-get-neg → op 64 无负例
        let text = GOOD.replace("[[contract.case]]\nid = \"x-get-neg\"\nop = 0x40\nkind = \"neg\"\nclass = \"error\"\ninput = \"i\"\nexpect = \"e\"\n", "");
        assert!(check_t2(&doc(&text), "bcts-t2@0.toml").unwrap_err().0.contains("op 64(GET) 无负例行"));
        // clause 重复
        let text = GOOD.to_string() + "[[contract.semantics]]\nclause = \"c-1\"\nvalue = \"v2\"\nfacet = \"错误码\"\nstatus = \"deferred\"\ncases = []\n";
        assert!(check_t2(&doc(&text), "bcts-t2@0.toml").unwrap_err().0.contains("clause 重复"));
        // frozen 空 cases
        let text = GOOD.replace("cases = [\"x-put-pos\"]", "cases = []");
        assert!(check_t2(&doc(&text), "bcts-t2@0.toml").unwrap_err().0.contains("status=frozen 但 cases 空"));
        // 空集守卫
        let text = GOOD.replace("tier2_floor = 1", "tier2_floor = 5");
        assert!(check_t2(&doc(&text), "bcts-t2@0.toml").unwrap_err().0.contains("空集守卫"));
        // 文件名失配
        assert!(check_t2(&doc(GOOD), "bcts-t2@9.toml").unwrap_err().0.contains("冗余自证"));
    }

    #[test]
    fn t2_py_repr_faces() {
        assert_eq!(py_repr(&T2Val::Arr(vec![T2Val::Str("a".into()), T2Val::Bool(true)])), "['a', True]");
        assert_eq!(py_repr(&T2Val::Table(vec![("k".into(), T2Val::Int(2))])), "{'k': 2}");
        assert_eq!(py_set_repr(&["b", "a"]), "['a', 'b']");
        assert!(!py_ne_int(&T2Val::Bool(true), 1)); // True == 1
        assert!(!py_ne_int(&T2Val::Int(2), 2));
    }

    const GOOD: &str = "schema = 1\nsuite = \"bcts\"\ntier = 2\nplatform_revision = 0\nfrozen_at = \"51932bff775c5da0534a41dae50c5ace7b8e945d\"\nabi_snapshot = \"tools/abi.toml\"\nabi_snapshot_hash = \"sha256:d47c0177b1f4667b68780c2b462f7066fa3f5a0c157da436be358c409246b5c8\"\ntier2_floor = 1\ncontract_doc = \"docs/x.md\"\n[[contract]]\nid = \"svc-x\"\ntitle = \"T\"\ninterface = \"berry-x\"\ninterface_src = \"user/berry-x/src/lib.rs\"\nver_min = 1\nconsumers = [\"bsh\"]\nsuite_id = \"bcts-l1-x-v1\"\nsuite_cases = [\"sc-1\"]\n[[contract.op]]\nnum = 1\nname = \"PUT\"\nwire = \"req 1B\"\n[[contract.op]]\nnum = 0x40\nname = \"GET\"\nwire = \"req 1B\"\n[[contract.case]]\nid = \"x-put-pos\"\nop = 1\nkind = \"pos\"\nclass = \"seq\"\ninput = \"i\"\nexpect = \"e\"\ngroup = \"sc-1\"\n[[contract.case]]\nid = \"x-put-neg\"\nop = 1\nkind = \"neg\"\nclass = \"reject\"\ninput = \"i\"\nexpect = \"e\"\n[[contract.case]]\nid = \"x-get-pos\"\nop = 0x40\nkind = \"pos\"\nclass = \"seq\"\ninput = \"i\"\nexpect = \"e\"\n[[contract.case]]\nid = \"x-get-neg\"\nop = 0x40\nkind = \"neg\"\nclass = \"error\"\ninput = \"i\"\nexpect = \"e\"\n[[contract.semantics]]\nclause = \"c-1\"\nvalue = \"v\"\nfacet = \"错误码\"\nstatus = \"frozen\"\ncases = [\"x-put-pos\"]\n";
}
