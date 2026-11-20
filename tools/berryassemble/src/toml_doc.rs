//! product.toml 面 TOML 子集解析器（berryassemble 二刀第一段）。
//!
//! 语面 = `tools/drill/acme/product.toml` 参照物 + berry_assemble.py
//! SELFTEST_PRODUCT 模板全集：嵌套点分节头 `[a.b.c]`、裸键/双引号键
//! （`"app/A.bap" = …`）、值四型（双引号串/bool/十进制 int/字符串数组——
//! 单行与多行形）、整行与行尾 `#` 注释（串外定位——串内 `#` 是值字符，
//! tomllib 同判）。非目标：内联表/多行串/转义/hex/日期——product.toml
//! 现役语面无此形，出现即拒（拒面 loud，tomllib 同咬由 parity 钉板）。
//!
//! 键序保序（Vec——tomllib dict 插入序镜像），`flatten` 输出 `k = repr`
//! 展平行序列供 oracle 对拍（Python repr 形：串单引号、数组逗号后空格）。
//! 空节不产出（现役语面无空节；对拍文本避让，注释在此钉界）。

use std::fmt::Write as _;

/// TOML 子集值四型（product.toml 现役语面全集）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Val {
    Int(i64),
    Str(String),
    Bool(bool),
    Arr(Vec<Val>),
}

/// 解析产物：展平键值表（`a.b.key` 全路径形，键序 = 节头出现序 × 节内
/// 键序——tomllib dict 插入序镜像）。
pub type Table = Vec<(String, Val)>;

/// 解析拒（一行理由带行号——tomllib 同咬拒面，文案不同源、分支标识
/// 对齐由 parity 钉板）。
#[derive(Debug)]
pub struct TomlError(pub String);

fn err(line: u32, msg: &str) -> TomlError {
    TomlError(format!("TOML 解析失败： 行 {line} {msg}"))
}

/// 串外 `#` 定位（双引号开关状态机——本子集无转义，串内 `#` 是值字符）。
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

/// 串内 `"`/`#`/`,`/`]` 字节计数辅助：判定值是否闭合/切分点均以「串外」
/// 为准。返回 (串外首个 ch 位, 串是否跨全行未闭合)。
fn outside_byte(s: &str, ch: u8) -> (Option<usize>, bool) {
    let mut in_str = false;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'"' => in_str = !in_str,
            x if x == ch && !in_str => return (Some(i), false),
            _ => {}
        }
    }
    (None, in_str)
}

/// 剥行尾注释（串外 `#` 起截断）。
fn strip_comment(line: &str) -> &str {
    match outside_hash(line) {
        Some(i) => &line[..i],
        None => line,
    }
}

/// 双引号串解析（可打印 ASCII、无转义——本子集串面全集；闭合引号后
/// 不得有残留）。
fn parse_str(s: &str, line: u32) -> Result<String, TomlError> {
    let body = s
        .strip_prefix('"')
        .ok_or_else(|| err(line, "字符串须以 \" 起始"))?;
    let close = body
        .find('"')
        .ok_or_else(|| err(line, "字符串未闭合"))?;
    let rest = &body[close + 1..];
    if !rest.trim().is_empty() {
        return Err(err(line, "字符串闭合引号后残留"));
    }
    let body = &body[..close];
    for ch in body.chars() {
        if !('\x20'..='\x7E').contains(&ch) || ch == '\\' {
            return Err(err(line, "字符串非法字符（子集仅可打印 ASCII 无转义）"));
        }
    }
    Ok(body.to_string())
}

/// 标量值解析（串/bool/int；数组归 parse_array_multiline）。
fn parse_scalar(s: &str, line: u32) -> Result<Val, TomlError> {
    let t = s.trim();
    if t == "true" {
        return Ok(Val::Bool(true));
    }
    if t == "false" {
        return Ok(Val::Bool(false));
    }
    if t.starts_with('"') {
        return Ok(Val::Str(parse_str(t, line)?));
    }
    // 十进制 int（含负；值域执法归上层——tomllib 同收）
    let body = t.strip_prefix('-').unwrap_or(t);
    if !body.is_empty() && body.bytes().all(|c| c.is_ascii_digit()) {
        if let Ok(n) = t.parse::<i64>() {
            return Ok(Val::Int(n));
        }
    }
    Err(err(line, &format!("无法解析值 {t:?}")))
}

/// 数组体解析（元素 = 串/int/bool 标量；串外逗号切分；尾逗号容忍——
/// acme preset 多行形同款）。串内逗号免疫由 outside_byte 切分保证。
fn parse_array_body(inner: &str, line: u32) -> Result<Vec<Val>, TomlError> {
    let mut vals = Vec::new();
    let mut rest = inner.trim();
    while !rest.is_empty() {
        let (comma, _) = outside_byte(rest, b',');
        let (piece, tail) = match comma {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        };
        let piece = piece.trim();
        if !piece.is_empty() {
            vals.push(parse_scalar(piece, line)?);
        }
        rest = tail.trim_start();
    }
    Ok(vals)
}

/// 值解析（含数组臂）：`[` 起始 → 找串外闭合 `]`；本行未闭合返回 Ok(None)
/// （多行数组累积态由 parse 主循环驱动）。
fn parse_value(s: &str, line: u32) -> Result<Option<Val>, TomlError> {
    let t = s.trim();
    if t.starts_with('[') {
        let (close, _) = outside_byte(t, b']');
        return match close {
            Some(i) => {
                // 闭合后残留检查（] 之后只许空白/已剥的注释）
                if !t[i + 1..].trim().is_empty() {
                    return Err(err(line, "数组闭合 ] 后残留"));
                }
                Ok(Some(Val::Arr(parse_array_body(&t[1..i], line)?)))
            }
            None => Ok(None), // 多行数组开头
        };
    }
    Ok(Some(parse_scalar(t, line)?))
}

/// TOML 子集解析主入口。多行数组：值跨行累积至串外 `]`（累积行带原文
/// 注释面由 strip_comment 先行剥除）。
/// 段感知路径冲突（tomllib「标量/表互覆拒」镜像）：exact 命中 / 新路径
/// 段序列是既有段序列的严格前缀（新标量罩旧表）/ 既有段序列是新路径的
/// 严格前缀（新表嵌在旧标量下）。按 '.' 切段比较——引号键内 '.' 被切后
/// 与 tomllib 真树差一档，但现役语面（base.toml 引号键 vs product.toml
/// 裸键节）无同席形，偏差在此钉界。
fn path_conflicts(seen: &std::collections::HashSet<String>, path: &str) -> bool {
    let segs: Vec<&str> = path.split('.').collect();
    for e in seen {
        let esegs: Vec<&str> = e.split('.').collect();
        let n = segs.len().min(esegs.len());
        let common = segs[..n] == esegs[..n];
        if common && (segs.len() == esegs.len() || segs.len() > n || esegs.len() > n) {
            return true;
        }
    }
    false
}

pub fn parse(text: &str) -> Result<Table, TomlError> {
    parse_with_sections(text).map(|(t, _)| t)
}

/// `parse` + 节头路径清单（文件序）。空节（无任何键）在 Table 中零条目
/// ——节在场性消费面（lite 结构执法）经此清单区分「节缺席」vs「节在场
/// 但空」（tomllib dict 语义：空节 = 空 table，在场）。
pub fn parse_with_sections(text: &str) -> Result<(Table, Vec<String>), TomlError> {
    let mut table: Table = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut section = String::new();
    let mut sections: Vec<String> = Vec::new();
    let mut acc: Option<(String, String)> = None; // (key, 累积数组体)
    for (idx, raw) in text.split('\n').enumerate() {
        let lineno = (idx + 1) as u32;
        let line = strip_comment(raw.trim_end_matches('\r'));
        let t = line.trim();
        if acc.is_some() {
            // 多行数组累积态：找串外 ] 闭合
            let (close, _) = outside_byte(t, b']');
            match close {
                Some(i) => {
                    let (key, mut body) = acc.take().unwrap();
                    body.push('\n');
                    body.push_str(&t[..i]);
                    let vals = parse_array_body(&body, lineno)?;
                    table.push((key, Val::Arr(vals)));
                }
                None => {
                    let (_, body) = acc.as_mut().unwrap();
                    body.push('\n');
                    body.push_str(t);
                }
            }
            continue;
        }
        if t.is_empty() {
            continue;
        }
        if t.starts_with('[') {
            if !t.ends_with(']') || t.len() < 3 {
                return Err(err(lineno, "节头非 [name] 形"));
            }
            section = t[1..t.len() - 1].trim().to_string();
            if section.is_empty() || section.contains('"') {
                return Err(err(lineno, "节头非 [name] 形"));
            }
            if !seen.insert(format!("[{section}]")) {
                return Err(err(lineno, &format!("节头 [{section}] 重复定义")));
            }
            sections.push(section.clone());
            continue;
        }
        let eq = t
            .find('=')
            .ok_or_else(|| err(lineno, "无「=」"))?;
        let key_raw = t[..eq].trim();
        let val_s = t[eq + 1..].trim();
        // 裸键 / 双引号键（base.toml "app/A.bap" 形）
        let key = if key_raw.starts_with('"') {
            parse_str(key_raw, lineno)?
        } else {
            if key_raw.is_empty()
                || !key_raw
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
            {
                return Err(err(lineno, &format!("键 {key_raw:?} 非裸键/引号键形")));
            }
            key_raw.to_string()
        };
        match parse_value(val_s, lineno)? {
            Some(v) => {
                let full = if section.is_empty() {
                    key
                } else {
                    format!("{section}.{key}")
                };
                if path_conflicts(&seen, &full) {
                    return Err(err(lineno, &format!("键 {full} 与既有键/表互覆")));
                }
                seen.insert(full.clone());
                table.push((full, v));
            }
            None => {
                // 多行数组开头：记 pending 键，数组体首段入累积
                let full = if section.is_empty() {
                    key
                } else {
                    format!("{section}.{key}")
                };
                if path_conflicts(&seen, &full) {
                    return Err(err(lineno, &format!("键 {full} 与既有键/表互覆")));
                }
                seen.insert(full.clone());
                let body = val_s.trim().strip_prefix('[').unwrap_or("").to_string();
                acc = Some((full, body));
            }
        }
    }
    if acc.is_some() {
        return Err(err(text.split('\n').count() as u32, "多行数组未闭合"));
    }
    Ok((table, sections))
}

/// 展平对拍形：`k = repr`（Python repr 镜像——串单引号、bool 大写
/// True/False、数组 `['a', 'b']` 逗号后空格）。遍历序 = 树深优先插入序
/// （tomllib dict 语义镜像：`[product.apps]` 归 `product` 子树——键序 =
/// 节首现序 × 节内键序，非文件行序）。tomllib flatten oracle 逐行全等面。
pub fn flatten(table: &Table) -> Vec<String> {
    #[derive(Default)]
    struct Node {
        leaf: Option<Val>,
        kids: Vec<(String, Node)>,
    }
    fn kid<'a>(kids: &'a mut Vec<(String, Node)>, name: &str) -> &'a mut Node {
        if let Some(i) = kids.iter().position(|(k, _)| k == name) {
            return &mut kids[i].1;
        }
        kids.push((name.to_string(), Node::default()));
        let last = kids.len() - 1;
        &mut kids[last].1
    }
    let mut root = Node::default();
    for (path, v) in table {
        let parts: Vec<&str> = path.split('.').collect();
        let mut node = &mut root;
        for p in &parts[..parts.len() - 1] {
            node = kid(&mut node.kids, p);
        }
        let last = kid(&mut node.kids, parts[parts.len() - 1]);
        last.leaf = Some(v.clone());
    }
    fn walk(node: &Node, prefix: &str, out: &mut Vec<String>) {
        for (k, kid) in &node.kids {
            let full = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            if let Some(v) = &kid.leaf {
                let mut s = String::new();
                write_repr(&mut s, v);
                out.push(format!("{full} = {s}"));
            }
            walk(kid, &full, out);
        }
    }
    let mut out = Vec::new();
    walk(&root, "", &mut out);
    out
}

fn write_repr(out: &mut String, v: &Val) {
    match v {
        Val::Int(n) => {
            write!(out, "{n}").unwrap();
        }
        Val::Bool(b) => {
            // Python repr 形（True/False 大写）——tomllib flatten 对拍镜像
            write!(out, "{}", if *b { "True" } else { "False" }).unwrap();
        }
        Val::Str(s) => {
            write!(out, "'{s}'").unwrap();
        }
        Val::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_repr(out, item);
            }
            out.push(']');
        }
    }
}
