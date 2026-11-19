//! W1-Q27 二刀：`check_manifest_text` 门禁面（Python `tools/mkapp.py :375`
//! 镜像——彻底 Rust 化轨道 b 层 #4 刀序第四位第二刀）。manifest 文本 →
//! 过检视图：字节面前置（LF only / 拒 tab）→ TOML 子集解析（canonical 只
//! 收规范形子集，非规范形在解析层即拒，不甩锅给规范形比较）→ 结构/值域
//! 执法（[app] 五键 / [compat] 三值链 + min_level/abi_revision / caps 词表
//! / runtime 域 / security / ui / agent-actions / blob）→ Reject 拒因
//! 三元组（code/where/line——`--json` 信封消费面）。
//!
//! 与 Python 单源对齐义务：改任一侧必须同步另一侧并重钉黄金向量
//! （14 accept 形 + 64 拒面，LOG.md 本轮条目存再钉命令）。

use crate::{ManifestView, Sec, Ui, Blob, INTENTS, RISK_TIERS, BLOB_KEYS, SEC_KEYS,
            SDK_CURRENT, ID_MAX, VERSION_MAX, ENTRY_MAX, STACK_PAGES_MIN,
            STACK_PAGES_MAX, UI_PATH_MAX, LABEL_MAX, ACTION_NAME_MAX,
            AGENT_ACTIONS_MAX, BLOB_FIELD_MAX, ABI_REVISION_V1, ABI_REVISION_MAX, ver_str};

/// 门禁拒因（Python `Reject` 镜像——code/where/line 三元组 + 诊断载荷）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reject {
    pub message: String,
    /// 域.键.因 点分稳定名（消费端按舱分支渲染）。
    pub code: &'static str,
    /// 节.键路径。
    pub where_: &'static str,
    /// 仅语法层：TOML 解析位点（1 起行号）。
    pub line: Option<u32>,
    /// --json 拒因舱扩展（W1-Q27 七刀，Python `Reject` kwargs 镜像）：
    /// 三舱恒 `Option<String>`——`Some(原始 JSON 片段)`（串 = 含引号转义
    /// 形、int = 裸数字、list = JSON 数组），`None` = 字段缺席。仅钉板
    /// 类站点（Python selftest 八类对拍的拒因）填充；其余站点 None =
    /// Python 缺省 kwargs 同形。
    pub jx_where: Option<String>,
    pub jx_expected: Option<String>,
    pub jx_got: Option<String>,
    pub jx_fix: Option<String>,
}

impl Reject {
    fn new(message: impl Into<String>, code: &'static str, where_: &'static str) -> Self {
        Self { message: message.into(), code, where_, line: None, jx_where: None, jx_expected: None, jx_got: None, jx_fix: None }
    }

    /// --json 拒因舱三键装载（七刀：钉板类站点专用——Python kwargs 值的
    /// 原始 JSON 片段形）。
    pub fn jx(
        message: impl Into<String>,
        code: &'static str,
        where_: &'static str,
        jx_expected: Option<String>,
        jx_got: Option<String>,
        jx_fix: Option<String>,
    ) -> Self {
        Self { message: message.into(), code, where_, line: None, jx_where: None, jx_expected, jx_got, jx_fix }
    }
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Reject {}

/// TOML 子集解析中间值（canonical 规范形全集：双引号基础串 / bool /
/// 十进制 int / 单元素数组——hex/下划线/单引号/内联表/多行串在解析层拒）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Val {
    Str(String),
    Bool(bool),
    Int(i64),
    Arr(Vec<Val>),
}

/// 解析后的节（键序保序——canonical 同序重发射依据）。
pub type Doc = Vec<(String, Val)>;

/// 解析产物：键值表 + 节壳清单（空节也在场——tomllib doc.get(sec) = {} 同判据）。
pub struct Parsed {
    pub doc: Doc,
    pub section_headers: Vec<String>,
}

// ---- TOML 子集解析器（canonical 全集）----

/// 串外 `#` 定位：返回首个不在双引号串内的 `#` 字节位（None = 串内 # 或
/// 无 #）。canonical 值串无转义（`\` 解析层拒），故状态机只需双引号开关
/// ——无escaped quote 面。
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

fn parse_toml_subset(text: &str) -> Result<Parsed, Reject> {
    let mut doc: Doc = Vec::new();
    let mut section_headers: Vec<String> = Vec::new();
    let mut section: Option<String> = None;
    for (idx, raw) in text.split('\n').enumerate() {
        let lineno = idx as u32 + 1;
        let line = raw.trim_end_matches('\r');
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        // 注释：canonical 不发射注释——串外 `#` 在场即拒（canonical 形比较
        // 兜底前的显式拒因；行尾注释与整行注释同拒）。双引号串内 `#` 是
        // 值字符（Python tomllib 同判——串内 # 不构成注释），走串外定位
        // 豁免（#526 REJECT 修复：单源镜像对齐）。
        if outside_hash(line).is_some() {
            return Err(Reject {
                message: format!("manifest: TOML 解析失败： 行 {lineno} 注释非规范形（canonical 不发射注释）"),
                code: "manifest.syntax.toml", where_: "manifest.toml", line: Some(lineno),
                jx_where: None, jx_expected: None, jx_got: None, jx_fix: None,
            });
        }
        if t.starts_with('[') {
            if !t.ends_with(']') || t.len() < 3 {
                return Err(Reject {
                    message: format!("manifest: TOML 解析失败： 行 {lineno} 节头非 [name] 形"),
                    code: "manifest.syntax.toml", where_: "manifest.toml", line: Some(lineno),
                jx_where: None, jx_expected: None, jx_got: None, jx_fix: None,
                });
            }
            let name = t[1..t.len() - 1].to_string();
            if !section_headers.contains(&name) {
                section_headers.push(name.clone());
            }
            section = Some(name);
            continue;
        }
        let eq = match t.find('=') {
            Some(i) => i,
            None => return Err(Reject {
                message: format!("manifest: TOML 解析失败： 行 {lineno} 无「=」"),
                code: "manifest.syntax.toml", where_: "manifest.toml", line: Some(lineno),
                jx_where: None, jx_expected: None, jx_got: None, jx_fix: None,
            }),
        };
        let key = t[..eq].trim().to_string();
        let val_s = t[eq + 1..].trim();
        let val = parse_value(val_s, lineno)?;
        let full_key = match &section {
            Some(s) => format!("{s}.{key}"),
            None => key,
        };
        doc.push((full_key, val));
    }
    Ok(Parsed { doc, section_headers })
}

fn parse_value(s: &str, lineno: u32) -> Result<Val, Reject> {
    let syntax = |msg: String| Reject {
        message: format!("manifest: TOML 解析失败： {msg}"),
        code: "manifest.syntax.toml", where_: "manifest.toml", line: Some(lineno),
                jx_where: None, jx_expected: None, jx_got: None, jx_fix: None,
    };
    if s == "true" { return Ok(Val::Bool(true)); }
    if s == "false" { return Ok(Val::Bool(false)); }
    if s.starts_with('"') {
        // 双引号基础串：canonical 全集 = 可打印 ASCII 0x20..=0x7E、无转义
        // （`\` 在 _check_str 拒——解析层同拒，canonical 不发射转义）。
        let close = match s[1..].find('"') {
            Some(i) => 1 + i,
            None => return Err(syntax(format!("行 {lineno} 字符串未闭合"))),
        };
        if close + 1 != s.len() {
            return Err(syntax(format!("行 {lineno} 字符串闭合引号后残留")));
        }
        let body = &s[1..close];
        for ch in body.chars() {
            if !('\x20'..='\x7E').contains(&ch) || ch == '\\' {
                return Err(syntax(format!("行 {lineno} 字符串非法字符（canonical 仅可打印 ASCII 无转义）")));
            }
        }
        return Ok(Val::Str(body.to_string()));
    }
    if s.starts_with('[') {
        // 单元素数组（canonical fs/net 形）：多元素/嵌套/空格混排拒。
        if !s.ends_with(']') {
            return Err(syntax(format!("行 {lineno} 数组未闭合")));
        }
        let inner = &s[1..s.len() - 1];
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        let mut vals = Vec::new();
        for part in parts {
            if part.is_empty() { continue; }
            vals.push(parse_value(part, lineno)?);
        }
        return Ok(Val::Arr(vals));
    }
    // 十进制 int（含负——值域执法在上层，tomllib 同收）：canonical 全集
    // 无前导零/hex/octal/下划线——非 -?[0-9]+ 拒。
    let int_body = s.strip_prefix('-').unwrap_or(s);
    if !int_body.is_empty() && int_body.chars().all(|c| c.is_ascii_digit()) {
        if int_body.len() > 1 && int_body.starts_with('0') {
            return Err(syntax(format!("行 {lineno} 整数前导零非规范形")));
        }
        if let Ok(v) = s.parse::<i64>() {
            return Ok(Val::Int(v));
        }
    }
    Err(syntax(format!("行 {lineno} 值非规范形（hex/下划线/单引号/内联表/多行串拒）")))
}

fn get<'a>(doc: &'a Doc, key: &str) -> Option<&'a Val> {
    doc.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn section_keys<'a>(doc: &'a Doc, sec: &str) -> Vec<&'a str> {
    let pfx = format!("{sec}.");
    doc.iter()
        .filter_map(|(k, _)| k.strip_prefix(&pfx))
        .collect()
}

/// 字符串值提取（非 Str → None；上层按 code 分支拒）。
fn as_str<'a>(v: &'a Val) -> Option<&'a str> {
    match v { Val::Str(s) => Some(s), _ => None }
}

/// `_check_str` 镜像：类型 → 长度 → 字符集（可打印 ASCII、无反斜杠/引号）。
fn check_str<'a>(v: &'a Val, what: &str, maxlen: usize, key: &'static str) -> Result<&'a str, Reject> {
    let code_ty: &'static str = Box::leak(format!("manifest.{}.type", key).into_boxed_str());
    let code_len: &'static str = Box::leak(format!("manifest.{}.length", key).into_boxed_str());
    let code_cs: &'static str = Box::leak(format!("manifest.{}.charset", key).into_boxed_str());
    let s = match v {
        Val::Str(s) => s,
        _ => return Err(Reject::new(format!("manifest: {what} 应为字符串"), code_ty, key)),
    };
    if s.is_empty() || s.len() > maxlen {
        // Python `_check_str` length 臂 kwargs 镜像（--json 钉板类②）：
        // expected="1..={maxlen}" 区间串 / got=len(v) int。
        return Err(Reject::jx(
            format!("manifest: {what} 空/超长（1..={maxlen}）"), code_len, key,
            Some(format!("\"1..={maxlen}\"")), Some(format!("{}", s.len())), None,
        ));
    }
    for ch in s.chars() {
        if !('\x20'..='\x7E').contains(&ch) || ch == '\\' || ch == '"' {
            return Err(Reject::new(format!("manifest: {what} 非法字符 {ch:?}"), code_cs, key));
    }
        }
    Ok(s)
}

/// VER_RE 镜像：每段 0 或 1-3 位无前导零。
fn parse_ver(s: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 { return None; }
    let mut out = [0u32; 3];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || p.len() > 3 { return None; }
        if p.len() > 1 && p.starts_with('0') { return None; }
        out[i] = p.parse().ok()?;
    }
    Some((out[0], out[1], out[2]))
}

/// `check_manifest_text`（Python mkapp.py :375 镜像）：manifest 文本 →
/// 过检视图；门禁第 1+2 道（结构/值域 + 三值链 + 规范形）。
pub fn check_manifest_text(text: &str) -> Result<ManifestView<'static>, Reject> {
    // 字节面前置：LF only、拒 tab。
    if text.contains('\r') {
        return Err(Reject::new("manifest: CR（CRLF）拒收——LF only", "manifest.syntax.crlf", "manifest.toml"));
    }
    if text.contains('\t') {
        return Err(Reject::new("manifest: tab 拒收", "manifest.syntax.tab", "manifest.toml"));
    }

    let parsed = parse_toml_subset(text)?;
    let doc = &parsed.doc;

    // 节级结构：未知节/根级键 → section.unknown；缺节 → section.missing。
    let known = ["app", "capabilities", "runtime", "compat", "security", "ui", "agent-actions", "blob"];
    let mut sections: Vec<&str> = Vec::new();
    let mut root_keys: Vec<String> = Vec::new();
    for (k, _) in doc.iter() {
        if k.contains('.') {
            let sec = k.split('.').next().unwrap();
            if !sections.contains(&sec) { sections.push(sec); }
        } else {
            root_keys.push(k.clone());
        }
    }
    let mut unknown: Vec<&str> = sections.iter().copied().filter(|s| !known.contains(s)).collect();
    unknown.extend(root_keys.iter().map(|_| ""));
    if !unknown.is_empty() {
        return Err(Reject::new(
            format!("manifest: 未知节 {root_keys:?}"),
            "manifest.section.unknown", "manifest.toml"));
    }
    let has_app_section = parsed.section_headers.iter().any(|s| s == "app");
    if !has_app_section {
        return Err(Reject::new("manifest: 缺 [app]", "manifest.section.missing", "app"));
    }
    let has_caps_section = parsed.section_headers.iter().any(|s| s == "capabilities");
    if !has_caps_section {
        return Err(Reject::new("manifest: 缺 [capabilities]", "manifest.section.missing", "capabilities"));
    }

    // [app] 五键。
    let app_keys = section_keys(&doc, "app");
    let expected = ["id", "version", "entry", "type", "intent"];
    let mut got: Vec<&str> = app_keys.clone();
    got.sort_unstable();
    let mut exp_sorted = expected;
    exp_sorted.sort_unstable();
    if got != exp_sorted {
        return Err(Reject::new("manifest: [app] 键集应为五键", "manifest.app.keys", "app"));
    }
    let id: &'static str = Box::leak(check_str(get(&doc, "app.id").unwrap(), "id", ID_MAX, "app.id")?.to_string().into_boxed_str());
    // ID_RE：[A-Za-z0-9][A-Za-z0-9._-]*
    let mut id_chars = id.chars();
    match id_chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return Err(Reject::new("manifest: id 字符集 [A-Za-z0-9._-]、首字符字母数字", "manifest.id.charset", "app.id")),
    }
    for c in id_chars {
        if !(c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') {
            return Err(Reject::new("manifest: id 字符集 [A-Za-z0-9._-]、首字符字母数字", "manifest.id.charset", "app.id"));
        }
    }
    let version: &'static str = Box::leak(check_str(get(&doc, "app.version").unwrap(), "version", VERSION_MAX, "app.version")?.to_string().into_boxed_str());
    let entry: &'static str = Box::leak(check_str(get(&doc, "app.entry").unwrap(), "entry", ENTRY_MAX, "app.entry")?.to_string().into_boxed_str());
    if !entry.starts_with("bin/") || entry.len() <= 4 {
        return Err(Reject::new("manifest: entry 应为 bin/ 前缀完整路径", "manifest.entry.prefix", "app.entry"));
    }
    match get(&doc, "app.type").unwrap() {
        Val::Str(s) if s == "app" => {}
        other => return Err(Reject::new(format!("manifest: type 仅 \"app\"（got {other:?}）", ), "manifest.type.value", "app.type")),
    }
    let intent: &'static str = match as_str(get(&doc, "app.intent").unwrap()) {
        Some(s) if INTENTS.contains(&s) => Box::leak(s.to_string().into_boxed_str()),
        // Python kwargs 镜像（--json 钉板类③）：expected=六词表 list / got=原值串。
        _ => {
            let raw = get(&doc, "app.intent").unwrap();
            let got = match raw {
                Val::Str(s) => format!("\"{s}\""),
                Val::Bool(b) => format!("{b}"),
                Val::Int(i) => format!("{i}"),
                Val::Arr(a) => format!(
                    "[{}]",
                    a.iter().map(|v| match v {
                        Val::Str(s) => format!("\"{s}\""),
                        Val::Bool(b) => format!("{b}"),
                        Val::Int(i) => format!("{i}"),
                        Val::Arr(_) => "\"…\"".to_string(),
                    }).collect::<Vec<_>>().join(",")
                ),
            };
            return Err(Reject::jx(
                format!("manifest: intent 应为六词之一 ({})", INTENTS.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(", ")),
                "manifest.intent.vocab", "app.intent",
                Some(format!("[{}]", INTENTS.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(","))),
                Some(got), None,
            ));
        }
    };

    // [compat] 三值链 + min_level + abi_revision。
    let has_compat_section = parsed.section_headers.iter().any(|s| s == "compat");
    let mut compat = None;
    let mut min_level = None;
    let mut abi_revision = None;
    if has_compat_section {
        let keys = section_keys(&doc, "compat");
        let three = ["min", "target", "compile"];
        let n_three = keys.iter().filter(|k| three.contains(k)).count();
        let extra: Vec<&str> = keys.iter().copied().filter(|k| !three.contains(k) && k != &"min_level" && k != &"abi_revision").collect();
        if !extra.is_empty()
            || (n_three != 0 && n_three != 3)
            || (n_three == 0 && !(keys.len() == 1 && keys[0] == "abi_revision"))
        {
            return Err(Reject::new(
                "manifest: [compat] 应为 min/target/compile 三键齐备（+可选 min_level/abi_revision；仅 abi_revision 孤立在场 = mkapp 打戳形）",
                "manifest.compat.keys", "compat"));
        }
        if n_three == 3 {
            let mut triple = Vec::new();
            for k in three {
                let v = get(&doc, &format!("compat.{k}")).unwrap();
                let s = match as_str(v) { Some(s) => s, None =>
                    return Err(Reject::new("manifest: compat 值应为 x.y.z（每段 1-3 位十进制、无前导零）", "manifest.compat.value", "compat.min")) };
                match parse_ver(s) {
                    Some(t) => triple.push(t),
                    None => return Err(Reject::new("manifest: compat 值应为 x.y.z（每段 1-3 位十进制、无前导零）", "manifest.compat.value", "compat.min")),
                }
            }
            let (mn, tg, cp) = (triple[0], triple[1], triple[2]);
            if !(mn <= tg && tg <= cp) {
                return Err(Reject::new(
                    format!("manifest: compat 序不合（需 min ≤ target ≤ compile）：min={} target={} compile={}",
                        ver_str(mn), ver_str(tg), ver_str(cp)),
                    "manifest.compat.order", "compat"));
            }
            if cp > SDK_CURRENT {
                // Python kwargs 镜像（--json 钉板类④）：expected="≤{SDK}" / got=compile 串。
                return Err(Reject::jx(
                    format!("manifest: compile {} 超工具链当前级 {}", ver_str(cp), ver_str(SDK_CURRENT)),
                    "manifest.compat.ceiling", "compat.compile",
                    Some(format!("\"≤{}\"", ver_str(SDK_CURRENT))),
                    Some(format!("\"{}\"", ver_str(cp))), None,
                ));
            }
            compat = Some((mn, tg, cp));
            if let Some(v) = get(&doc, "compat.min_level") {
                match v {
                    Val::Int(i) if (0..=0xFFFFFFFFi64).contains(i) => min_level = Some(*i as u32),
                    Val::Int(_) => return Err(Reject::new("manifest: min_level 越界 [0,4294967295]", "manifest.compat.min_level.range", "compat.min_level")),
                    _ => return Err(Reject::new("manifest: min_level 应为整数", "manifest.compat.min_level.type", "compat.min_level")),
                }
            }
        }
        if let Some(v) = get(&doc, "compat.abi_revision") {
            match v {
                Val::Int(i) if (ABI_REVISION_V1 as i64..=ABI_REVISION_MAX as i64).contains(i) => abi_revision = Some(*i as u32),
                Val::Int(_) => return Err(Reject::new("manifest: abi_revision 越界 [1,65535]（u16，v1 起始 = 1）", "manifest.compat.abi_revision.range", "compat.abi_revision")),
                _ => return Err(Reject::new("manifest: abi_revision 应为整数", "manifest.compat.abi_revision.type", "compat.abi_revision")),
            }
        }
    }

    // [capabilities]：console 必在，fs/net 单元素数组词表。
    let caps_keys = section_keys(&doc, "capabilities");
    let extra: Vec<&str> = caps_keys.iter().copied().filter(|k| !["console", "fs", "net"].contains(k)).collect();
    if !extra.is_empty() || !caps_keys.contains(&"console") {
        return Err(Reject::new("manifest: [capabilities] 键集应为 console(+fs/net)", "manifest.capabilities.keys", "capabilities"));
    }
    let console = match get(&doc, "capabilities.console").unwrap() {
        Val::Bool(b) => *b,
        _ => return Err(Reject::new("manifest: console 应为 bool", "manifest.capabilities.console", "capabilities.console")),
    };
    let mut fs = None;
    let mut net = None;
    for (key, vocab, code) in [("fs", &["rootfs"][..], "manifest.fs.vocab"), ("net", &["client"][..], "manifest.net.vocab")] {
        if let Some(v) = get(&doc, &format!("capabilities.{key}")) {
            match v {
                Val::Arr(a) if a.len() == 1 => match as_str(&a[0]) {
                    Some(s) if vocab.contains(&s) => {
                        let l: &'static str = Box::leak(s.to_string().into_boxed_str());
                        if key == "fs" { fs = Some(l); } else { net = Some(l); }
                    }
                    _ => return Err(Reject::new(format!("manifest: {key} 应为单元素数组，词表 {vocab:?}"), code, "capabilities.fs")),
                },
                _ => return Err(Reject::new(format!("manifest: {key} 应为单元素数组，词表 {vocab:?}"), code, "capabilities.fs")),
            }
        }
    }

    // [runtime]：仅 stack_pages，域 [1,64]。
    let has_runtime = parsed.section_headers.iter().any(|s| s == "runtime");
    let mut stack_pages = 8u32;
    if has_runtime {
        let keys = section_keys(&doc, "runtime");
        if keys.len() != 1 || keys[0] != "stack_pages" {
            return Err(Reject::new("manifest: [runtime] 仅 stack_pages 一键", "manifest.runtime.keys", "runtime"));
        }
        match get(&doc, "runtime.stack_pages").unwrap() {
            Val::Int(i) if (STACK_PAGES_MIN as i64..=STACK_PAGES_MAX as i64).contains(i) => stack_pages = *i as u32,
            Val::Int(_) => return Err(Reject::new("manifest: stack_pages 越界 [1,64]", "manifest.runtime.range", "runtime.stack_pages")),
            _ => return Err(Reject::new("manifest: stack_pages 应为整数", "manifest.runtime.type", "runtime.stack_pages")),
        }
    }

    // [security]：三键齐备或齐缺，值 1..16B [a-z0-9-]。
    let mut sec = None;
    if parsed.section_headers.iter().any(|s| s == "security") {
        let keys = section_keys(&doc, "security");
        let mut got: Vec<&str> = keys.clone();
        got.sort_unstable();
        let mut want: Vec<&str> = SEC_KEYS.to_vec();
        want.sort_unstable();
        if got != want {
            return Err(Reject::new("manifest: [security] 应为 profile/key_lineage/integrator 三键齐备", "manifest.security.keys", "security"));
        }
        let mut vals: [&'static str; 3] = ["", "", ""];
        for (i, k) in SEC_KEYS.iter().enumerate() {
            let s = match as_str(get(&doc, &format!("security.{k}")).unwrap()) {
                Some(s) => s,
                None => return Err(Reject::new("manifest: security 值应为 1..16B 的 [a-z0-9-]", "manifest.security.vocab", "security.profile")),
            };
            let ok = (1..=16).contains(&s.len()) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !ok {
                return Err(Reject::new("manifest: security 值应为 1..16B 的 [a-z0-9-]", "manifest.security.vocab", "security.profile"));
            }
            vals[i] = Box::leak(s.to_string().into_boxed_str());
        }
        sec = Some(Sec { profile: vals[0], key_lineage: vals[1], integrator: vals[2] });
    }

    // [ui]：icon/label/views 独立可选 + 前缀/后缀/长度。
    let mut ui: Option<Ui<'static>> = None;
    if parsed.section_headers.iter().any(|s| s == "ui") {
        let keys = section_keys(&doc, "ui");
        let extra: Vec<&str> = keys.iter().copied().filter(|k| !["icon", "label", "views"].contains(k)).collect();
        if !extra.is_empty() {
            return Err(Reject::new("manifest: [ui] 键集应为 icon/label/views（独立可选）", "manifest.ui.keys", "ui"));
        }
        let icon = match get(&doc, "ui.icon") {
            Some(v) => {
                let s = check_str(v, "ui.icon", UI_PATH_MAX, "ui.icon")?;
                if !s.starts_with("icons/") || s.len() <= 6 || !s.ends_with(".byi") {
                    return Err(Reject::new("manifest: ui.icon 应为 icons/ 前缀 + `.byi` 后缀产品名", "manifest.ui.icon", "ui.icon"));
                }
                let l: &'static str = Box::leak(s.to_string().into_boxed_str());
                Some(l)
            }
            None => None,
        };
        let label = match get(&doc, "ui.label") {
            Some(v) => {
                let l: &'static str = Box::leak(check_str(v, "ui.label", LABEL_MAX, "ui.label")?.to_string().into_boxed_str());
                Some(l)
            }
            None => None,
        };
        let views = match get(&doc, "ui.views") {
            Some(v) => {
                let s = check_str(v, "ui.views", UI_PATH_MAX, "ui.views")?;
                if !s.starts_with("views/") || s.len() <= 6 {
                    return Err(Reject::new("manifest: ui.views 应为 views/ 前缀路径", "manifest.ui.views", "ui.views"));
                }
                let l: &'static str = Box::leak(s.to_string().into_boxed_str());
                Some(l)
            }
            None => None,
        };
        if icon.is_some() || label.is_some() || views.is_some() {
            ui = Some(Ui { icon, label, views });
        }
    }

    // [agent-actions]：kebab-case 键 ≤24B、三档词、≤16 条、文档序。
    let mut agent_actions: Vec<(&str, &str)> = Vec::new();
    let has_aa = parsed.section_headers.iter().any(|s| s == "agent-actions");
    if has_aa {
        let keys = section_keys(&doc, "agent-actions");
        if keys.len() > AGENT_ACTIONS_MAX {
            return Err(Reject::new(format!("manifest: [agent-actions] 条目越帽 {AGENT_ACTIONS_MAX}"), "manifest.agent-actions.cap", "agent-actions"));
        }
        for k in keys {
            let first_ok = k.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false);
            let rest_ok = k.chars().skip(1).all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !first_ok || !rest_ok || k.len() > ACTION_NAME_MAX || k.is_empty() {
                return Err(Reject::new("manifest: [agent-actions] 键应为 kebab-case 动作名", "manifest.agent-actions.key", "agent-actions"));
            }
            let v: &'static str = match as_str(get(&doc, &format!("agent-actions.{k}")).unwrap()) {
                Some(s) if RISK_TIERS.contains(&s) => Box::leak(s.to_string().into_boxed_str()),
                _ => return Err(Reject::new("manifest: [agent-actions] 值应为三档之一", "manifest.agent-actions.tier", "agent-actions.read")),
            };
            agent_actions.push((Box::leak(k.to_string().into_boxed_str()), v));
        }
    }

    // [blob]：五字段齐备，形态 1..=64 可打印 ASCII。
    let mut blob = None;
    if parsed.section_headers.iter().any(|s| s == "blob") {
        let keys = section_keys(&doc, "blob");
        let mut got: Vec<&str> = keys.clone();
        got.sort_unstable();
        let mut want: Vec<&str> = BLOB_KEYS.to_vec();
        want.sort_unstable();
        if got != want {
            return Err(Reject::new("manifest: [blob] 应为五字段齐备（缺一拒——声明完整性）", "manifest.blob.keys", "blob"));
        }
        let mut vals: [&'static str; 5] = [""; 5];
        for (i, k) in BLOB_KEYS.iter().enumerate() {
            vals[i] = Box::leak(check_str(get(&doc, &format!("blob.{k}")).unwrap(), "blob 字段", BLOB_FIELD_MAX, "blob.license")?.to_string().into_boxed_str());
        }
        blob = Some(Blob { license: vals[0], origin: vals[1], sha256: vals[2], size: vals[3], usage: vals[4] });
    }

    // 门禁 2：规范形比较（canonical 重发射 == 输入文本）。
    let view = ManifestView {
        id, version, entry, intent, console, fs, net,
        has_runtime: has_runtime, stack_pages,
        compat, min_level, abi_revision,
        sec, ui, agent_actions, blob,
    };
    if text != view.canonical() {
        // Python kwargs 镜像（--json 钉板类⑤）：fix 修法舱在场。
        return Err(Reject::jx(
            "manifest: 非规范形（hex/下划线整数、单引号、内联表、多行串、注释、多余空白等 tomllib 收而门禁不收）".to_string(),
            "manifest.canonical", "manifest.toml",
            None, None,
            Some("\"以门禁规范形重发射（canonical 键序/定序/双引号基础串/LF/无注释）\"".to_string()),
        ));
    }
    Ok(view)
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    /// accept 14 形（Python 现役件 check_manifest_text 实收 2026-11-19——
    /// canonical 全谱回喂：v0 旧形/词表/可选节组合/尾增节全谱/kitchen sink/
    /// 串内 # 值字符形）。再钉命令与生成器：LOG.md 本轮条目
    /// （scratch/w1-mkapp-golden.py 同源）。
    const ACCEPTS: [&str; 14] = [
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = true\nfs = [\"rootfs\"]\nnet = [\"client\"]\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = true\n\n[runtime]\nstack_pages = 16\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[compat]\nmin = \"0.0.1\"\ntarget = \"0.0.9\"\ncompile = \"0.1.0\"\nmin_level = 3\n\n[capabilities]\nconsole = false\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[compat]\nmin = \"0.0.1\"\ntarget = \"0.0.9\"\ncompile = \"0.1.0\"\n\n[capabilities]\nconsole = false\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[compat]\nabi_revision = 1\n\n[capabilities]\nconsole = false\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\nmin_level = 0\nabi_revision = 65535\n\n[capabilities]\nconsole = false\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[security]\nprofile = \"standard\"\nkey_lineage = \"lineage-a\"\nintegrator = \"oem-1\"\n",
        // #526 REJECT 修复新增形：串内 # = 值字符（Python tomllib 同判）。
        // 对拍锚：Python M.check_manifest_text 收且 canonical 回发射字节恒等。
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[ui]\nlabel = \"C# app\"\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[ui]\nicon = \"icons/app.byi\"\nlabel = \"Main View\"\nviews = \"views/main\"\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[ui]\nicon = \"icons/a.byi\"\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[agent-actions]\nread-inbox = \"routine\"\nsend-msg = \"sensitive\"\nwipe-all = \"destructive\"\n",
        "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[blob]\nlicense = \"Apache-2.0\"\norigin = \"https://example.com/src\"\nsha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\nsize = \"123456\"\nusage = \"demo usage text\"\n",
        "[app]\nid = \"x.y-z_1\"\nversion = \"9.9.9\"\nentry = \"bin/xyz\"\ntype = \"app\"\nintent = \"interactive\"\n\n[compat]\nmin = \"0.0.1\"\ntarget = \"0.0.9\"\ncompile = \"0.1.0\"\nmin_level = 7\nabi_revision = 1\n\n[capabilities]\nconsole = true\nfs = [\"rootfs\"]\nnet = [\"client\"]\n\n[runtime]\nstack_pages = 64\n\n[security]\nprofile = \"locked\"\nkey_lineage = \"k2\"\nintegrator = \"i2\"\n\n[ui]\nicon = \"icons/i.byi\"\nlabel = \"label\"\nviews = \"views/v\"\n\n[agent-actions]\na = \"routine\"\n\n[blob]\nlicense = \"MIT\"\norigin = \"o\"\nsha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\nsize = \"42\"\nusage = \"u\"\n",
    ];

    /// 拒面全谱（name|text|code|where|line——Python 拒因三元组钉板）。
    const BASE: &str = ACCEPTS[0];

    fn m(text: &str, pairs: &[(&str, &str)]) -> String {
        let mut t = text.to_string();
        for (old, new) in pairs {
            t = t.replacen(old, new, 1);
        }
        t
    }

    #[test]
    fn golden_accepts_14_forms() {
        for (i, text) in ACCEPTS.iter().enumerate() {
            match check_manifest_text(text) {
                Ok(v) => {
                    assert_eq!(v.canonical(), *text, "a{i} 视图回发射漂移");
                }
                Err(e) => panic!("a{i} 应收却拒：{e}"),
            }
        }
    }

    #[test]
    fn kitchen_sink_view_fields() {
        let v = check_manifest_text(ACCEPTS[13]).unwrap();
        assert_eq!(v.id, "x.y-z_1");
        assert!(v.console);
        assert_eq!(v.fs, Some("rootfs"));
        assert_eq!(v.net, Some("client"));
        assert_eq!(v.stack_pages, 64);
        assert_eq!(v.compat, Some(((0, 0, 1), (0, 0, 9), (0, 1, 0))));
        assert_eq!(v.min_level, Some(7));
        assert_eq!(v.abi_revision, Some(1));
        assert_eq!(v.sec.as_ref().unwrap().profile, "locked");
        assert_eq!(v.ui.as_ref().unwrap().label, Some("label"));
        assert_eq!(v.agent_actions, vec![("a", "routine")]);
        assert_eq!(v.blob.as_ref().unwrap().size, "42");
    }

    #[test]
    fn reject_spectrum_syntax_layer() {
        // 字节面 + TOML 语法层（line 透传钉板）。
        let cases: Vec<(&str, String, &str, &str, Option<u32>)> = vec![
            ("crlf", BASE.replace("\n[capabilities]", "\r\n[capabilities]"), "manifest.syntax.crlf", "manifest.toml", None),
            ("tab", m(BASE, &[("id = ", "id =\t")]), "manifest.syntax.tab", "manifest.toml", None),
            ("comment", m(BASE, &[("console = false", "console = false  # ok")]), "manifest.syntax.toml", "manifest.toml", Some(9)),
            ("unclosed", BASE.replacen("[app]", "[app", 1), "manifest.syntax.toml", "manifest.toml", Some(1)),
            ("bad_value", BASE.replacen("id = \"demo.app\"", "id = @", 1), "manifest.syntax.toml", "manifest.toml", Some(2)),
        ];
        for (name, text, code, where_, line) in cases {
            match check_manifest_text(&text) {
                Ok(_) => panic!("{name} 应拒却收"),
                Err(e) => {
                    assert_eq!(e.code, code, "{name} code 漂移：{}", e.code);
                    assert_eq!(e.where_, where_, "{name} where 漂移");
                    assert_eq!(e.line, line, "{name} line 漂移：{:?}", e.line);
                }
            }
        }
    }

    #[test]
    fn reject_spectrum_structure_and_values() {
        // (name, text, code)——结构/值域全谱；message 细节不钉（中英混排易碎）。
        let cases: Vec<(&str, String, &str)> = vec![
            ("root_key", format!("id = \"x\"\n\n{BASE}"), "manifest.section.unknown"),
            ("unknown_section", format!("{BASE}\n[extra]\nk = \"v\"\n"), "manifest.section.unknown"),
            ("missing_app", "[capabilities]\nconsole = false\n".into(), "manifest.section.missing"),
            ("missing_caps", "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n".into(), "manifest.section.missing"),
            ("app_keys_extra", m(BASE, &[("id = \"demo.app\"", "zzz = \"q\"\nid = \"demo.app\"")]), "manifest.app.keys"),
            ("app_keys_missing", m(BASE, &[("id = \"demo.app\"\n", "")]), "manifest.app.keys"),
            ("id_len", m(BASE, &[("\"demo.app\"", &format!("\"{}\"", "a".repeat(25)))]), "manifest.app.id.length"),
            ("id_charset", m(BASE, &[("\"demo.app\"", "\".abc\"")]), "manifest.id.charset"),
            ("id_type", m(BASE, &[("id = \"demo.app\"", "id = 5")]), "manifest.app.id.type"),
            ("version_len", m(BASE, &[("\"0.1.0\"", &format!("\"{}\"", "0".repeat(17)))]), "manifest.app.version.length"),
            ("entry_prefix", m(BASE, &[("\"bin/demo\"", "\"sbin/demo\"")]), "manifest.entry.prefix"),
            ("entry_bare", m(BASE, &[("\"bin/demo\"", "\"bin/\"")]), "manifest.entry.prefix"),
            ("type_value", m(BASE, &[("type = \"app\"", "type = \"driver\"")]), "manifest.type.value"),
            ("intent_vocab", m(BASE, &[("\"utility\"", "\"weird\"")]), "manifest.intent.vocab"),
            ("compat_order", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nmin = \"0.2.0\"\ntarget = \"0.1.0\"\ncompile = \"0.2.0\"\n")]), "manifest.compat.order"),
            ("compat_value_shape", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nmin = \"0.1\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\n")]), "manifest.compat.value"),
            ("compat_leading_zero", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nmin = \"0.01.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\n")]), "manifest.compat.value"),
            ("compat_ceiling", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.2.0\"\ncompile = \"0.2.0\"\n")]), "manifest.compat.ceiling"),
            ("compat_two_of_three", m(BASE, &[("\n[capabilities]\nconsole = false\n", "\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\n\n[capabilities]\nconsole = false\n")]), "manifest.compat.keys"),
            ("min_level_alone", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nmin_level = 3\n")]), "manifest.compat.keys"),
            ("min_level_bool", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\nmin_level = true\n")]), "manifest.compat.min_level.type"),
            ("min_level_neg", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\nmin_level = -1\n")]), "manifest.compat.min_level.range"),
            ("abi_rev_zero", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nabi_revision = 0\n")]), "manifest.compat.abi_revision.range"),
            ("abi_rev_over", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nabi_revision = 65536\n")]), "manifest.compat.abi_revision.range"),
            ("abi_rev_bool", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nabi_revision = true\n")]), "manifest.compat.abi_revision.type"),
            ("compat_not_dict", format!("compat = 5\n\n{BASE}"), "manifest.section.unknown"),
            ("caps_missing_console", m(BASE, &[("console = false\n", "")]), "manifest.capabilities.keys"),
            ("caps_console_str", m(BASE, &[("console = false", "console = \"yes\"")]), "manifest.capabilities.console"),
            ("fs_two_elems", m(BASE, &[("console = false\n", "console = false\nfs = [\"rootfs\", \"rootfs\"]\n")]), "manifest.fs.vocab"),
            ("fs_bad_vocab", m(BASE, &[("console = false\n", "console = false\nfs = [\"tmpfs\"]\n")]), "manifest.fs.vocab"),
            ("fs_not_list", m(BASE, &[("console = false\n", "console = false\nfs = \"rootfs\"\n")]), "manifest.fs.vocab"),
            ("net_bad_vocab", m(BASE, &[("console = false\n", "console = false\nnet = [\"server\"]\n")]), "manifest.net.vocab"),
            ("runtime_range_low", m(BASE, &[("console = false\n", "console = false\n\n[runtime]\nstack_pages = 0\n")]), "manifest.runtime.range"),
            ("runtime_range_high", m(BASE, &[("console = false\n", "console = false\n\n[runtime]\nstack_pages = 65\n")]), "manifest.runtime.range"),
            ("runtime_bool", m(BASE, &[("console = false\n", "console = false\n\n[runtime]\nstack_pages = true\n")]), "manifest.runtime.type"),
            ("runtime_extra_key", m(BASE, &[("console = false\n", "console = false\n\n[runtime]\nstack_pages = 8\nheap_pages = 2\n")]), "manifest.runtime.keys"),
            ("sec_two_keys", m(BASE, &[("console = false\n", "console = false\n\n[security]\nprofile = \"p\"\nintegrator = \"i\"\n")]), "manifest.security.keys"),
            ("sec_upper", m(BASE, &[("console = false\n", "console = false\n\n[security]\nprofile = \"P\"\nkey_lineage = \"k\"\nintegrator = \"i\"\n")]), "manifest.security.vocab"),
            ("sec_len", m(BASE, &[("console = false\n", &format!("console = false\n\n[security]\nprofile = \"{}\"\nkey_lineage = \"k\"\nintegrator = \"i\"\n", "a".repeat(17)))]), "manifest.security.vocab"),
            ("icon_no_byi", m(BASE, &[("console = false\n", "console = false\n\n[ui]\nicon = \"icons/a.png\"\n")]), "manifest.ui.icon"),
            ("icon_no_prefix", m(BASE, &[("console = false\n", "console = false\n\n[ui]\nicon = \"a.byi\"\n")]), "manifest.ui.icon"),
            ("views_no_prefix", m(BASE, &[("console = false\n", "console = false\n\n[ui]\nviews = \"vx\"\n")]), "manifest.ui.views"),
            ("ui_unknown_key", m(BASE, &[("console = false\n", "console = false\n\n[ui]\ntheme = \"dark\"\n")]), "manifest.ui.keys"),
            ("label_len", m(BASE, &[("console = false\n", &format!("console = false\n\n[ui]\nlabel = \"{}\"\n", "L".repeat(33)))]), "manifest.ui.label.length"),
            ("aa_cap", m(BASE, &[("console = false\n", &format!("console = false\n\n[agent-actions]\n{}\n", (0..17).map(|i| format!("k{i} = \"routine\"")).collect::<Vec<_>>().join("\n")))]), "manifest.agent-actions.cap"),
            ("aa_key_upper", m(BASE, &[("console = false\n", "console = false\n\n[agent-actions]\nBad-Key = \"routine\"\n")]), "manifest.agent-actions.key"),
            ("aa_key_len", m(BASE, &[("console = false\n", &format!("console = false\n\n[agent-actions]\n{} = \"routine\"\n", "k".repeat(25)))]), "manifest.agent-actions.key"),
            ("aa_tier", m(BASE, &[("console = false\n", "console = false\n\n[agent-actions]\nread = \"extreme\"\n")]), "manifest.agent-actions.tier"),
            ("blob_four", m(BASE, &[("console = false\n", &format!("console = false\n\n[blob]\nlicense = \"MIT\"\norigin = \"o\"\nsha256 = \"{}\"\nsize = \"42\"\n", "b".repeat(64)))]), "manifest.blob.keys"),
            ("blob_field_len", m(BASE, &[("console = false\n", &format!("console = false\n\n[blob]\nlicense = \"{}\"\norigin = \"o\"\nsha256 = \"{}\"\nsize = \"42\"\nusage = \"u\"\n", "L".repeat(65), "b".repeat(64)))]), "manifest.blob.license.length"),
        ];
        for (name, text, code) in cases {
            match check_manifest_text(&text) {
                Ok(_) => panic!("{name} 应拒却收"),
                Err(e) => assert_eq!(e.code, code, "{name} code 漂移：{}（msg：{}）", e.code, e.message),
            }
        }
    }

    #[test]
    fn reject_spectrum_canonical_forms() {
        // 规范形比较层：tomllib 宽容而门禁不收的全谱。
        let cases: Vec<(&str, String)> = vec![
            ("reorder", "[app]\ntype = \"app\"\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n".into()),
            ("single_quote", m(BASE, &[("id = \"demo.app\"", "id = 'demo.app'")])),
            ("hex_int", m(BASE, &[("console = false\n", "console = false\n\n[compat]\nabi_revision = 0x1\n")])),
            ("underscore_int", m(BASE, &[("console = false\n", "console = false\n\n[runtime]\nstack_pages = 1_6\n")])),
            ("inline_ws", m(BASE, &[("console = false", "console   =   false")])),
            ("no_trailing_nl", BASE.trim_end_matches('\n').to_string()),
        ];
        for (name, text) in cases {
            match check_manifest_text(&text) {
                Ok(_) => panic!("{name} 应拒却收"),
                Err(e) => {
                    // 本件解析器在解析层即拒非规范值形（hex/下划线/单引号），
                    // 其余到规范形比较层——两舱 code 都合法，钉 where 即可。
                    assert_eq!(e.where_, "manifest.toml", "{name} where 漂移：{}", e.where_);
                }
            }
        }
    }

    #[test]
    fn parser_rejects_leading_zero_int() {
        // 解析层钉板：前导零整数在 parse_value 即拒（canonical 全集不含）。
        let text = m(BASE, &[("console = false\n", "console = false\n\n[runtime]\nstack_pages = 08\n")]);
        let e = check_manifest_text(&text).unwrap_err();
        assert_eq!(e.code, "manifest.syntax.toml");
        assert!(e.line.is_some(), "语法层行号应透传");
    }

    #[test]
    fn hash_in_string_exempt_outside_rejected() {
        // #526 REJECT 修复钉板：串内 # = 值字符（Python tomllib 同判）。
        // 三收面：ui.label / blob.usage / 串内 # 行后续照常解析；两拒面：
        // 行尾真注释（串闭合后 #）与整行注释仍解析层拒。
        let in_str_label = m(BASE, &[("console = false\n", "console = false\n\n[ui]\nlabel = \"C# app\"\n")]);
        let v = check_manifest_text(&in_str_label).unwrap();
        assert_eq!(v.ui.as_ref().unwrap().label, Some("C# app"), "串内 # 应作值字符收");

        let in_str_blob = m(BASE, &[("console = false\n", &format!(
            "console = false\n\n[blob]\nlicense = \"MIT\"\norigin = \"o\"\nsha256 = \"{}\"\nsize = \"42\"\nusage = \"C# runtime helper\"\n", "b".repeat(64)))]);
        let v = check_manifest_text(&in_str_blob).unwrap();
        assert_eq!(v.blob.as_ref().unwrap().usage, "C# runtime helper");

        // 串内 # 不影响后续行解析（状态机逐行独立——跨行不漏）。
        let in_str_then_normal = m(BASE, &[("console = false\n", "console = false\n\n[ui]\nlabel = \"a#b\"\nviews = \"views/x\"\n")]);
        let v = check_manifest_text(&in_str_then_normal).unwrap();
        assert_eq!(v.ui.as_ref().unwrap().views, Some("views/x"));

        let trailing_comment = m(BASE, &[("console = false\n", "console = false  # real comment\n")]);
        let e = check_manifest_text(&trailing_comment).unwrap_err();
        assert_eq!(e.code, "manifest.syntax.toml", "行尾真注释应拒");
        assert_eq!(e.line, Some(9), "行号应透传");

        let whole_line = m(BASE, &[("console = false\n", "console = false\n\n[ui]\n# label = \"x\"\n")]);
        let e = check_manifest_text(&whole_line).unwrap_err();
        assert_eq!(e.code, "manifest.syntax.toml", "整行注释应拒");
    }

    #[test]
    fn empty_sections_rejected_by_canonical_compare() {
        // 空 [ui]/[agent-actions]/[blob] 节：canonical 不发射空节 → 规范形
        // 比较拒（Python 同款路径——「省略或显式」二选一）。
        for sec in ["ui", "agent-actions", "blob"] {
            let text = m(BASE, &[("console = false\n", &format!("console = false\n\n[{sec}]\n"))]);
            assert!(check_manifest_text(&text).is_err(), "空 [{sec}] 应拒");
        }
    }
}
