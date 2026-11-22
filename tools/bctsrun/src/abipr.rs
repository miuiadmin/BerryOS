//! bctsrun 四刀：abi level 读器 + 版本声称位对账镜像（现役单源 =
//! `tools/bcts/abi_level.py` + `tools/bcts/img_pr_check.py`〔bcts-run.sh
//! PYEOF④⑤段机械抽出〕，彻底 Rust 化轨道 b 层 #7 刀序末档四刀）。
//!
//! - `abi_level(path)`：读 abi.toml `[platform].level`——stdout = 十进制
//!   整数行；读失败/非整数 = `bcts: FAIL: …` stderr（py 退 1）。
//! - `img_pr_check(manifest, want, anchor)`：mkimage manifest v1 条目行
//!   platform_revision 声称位与回放期望对账——ok 行 stdout；拒 =
//!   `bcts: FAIL: …` stderr（py 退 1）。
//!
//! 文案与 Python 逐字同源（值域拒面 / ok 行 / OS Errno 面）。已知分歧
//! （拒分支对齐、文案不同源——tomllib 语法域同 T1/T2 刀先例）：①坏 TOML
//! ——py tomllib 异常文案 vs 本侧 io 文案；②非 UTF-8 源——py
//! UnicodeDecodeError vs io InvalidData。二者 parity 钉「双侧同拒退 1」。

use std::path::Path;

/// py `str(OSError)` 面（open() 异常带文件名）：`[Errno N] <msg>: '<path>'`
/// （Python 3.10+ repr 形路径单引号）。
fn py_os_str(e: &std::io::Error, path: &str) -> String {
    match e.raw_os_error() {
        Some(2) => format!("[Errno 2] No such file or directory: '{path}'"),
        Some(13) => format!("[Errno 13] Permission denied: '{path}'"),
        Some(21) => format!("[Errno 21] Is a directory: '{path}'"),
        Some(code) => format!("[Errno {code}] Unknown error {code}: '{path}'"),
        None => e.to_string(),
    }
}

/// `[platform].level` 值面（py `lv` 的 repr 需要的全部值域）。
#[derive(Debug, PartialEq)]
enum Lv {
    Int(i64),
    Str(String),
    Bool(bool),
    Float(f64),
    None,
}

/// py repr：int 十进制 / str 单引号 / True·False / float 最短往返（整值
/// 补 .0 对齐 repr(1.0)="1.0"）/ None。
fn lv_repr(v: &Lv) -> String {
    match v {
        Lv::Int(i) => i.to_string(),
        Lv::Str(s) => format!("'{s}'"),
        Lv::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Lv::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{f:.1}")
            } else {
                format!("{f}")
            }
        }
        Lv::None => "None".to_string(),
    }
}

/// 值解析（abi.toml `[platform]` 语面：int/双·单引号串/bool/float；其余
/// 形 = 坏 TOML 面 → None 外的 Err）。
fn parse_lv(tok: &str) -> Result<Lv, ()> {
    let t = tok.trim();
    if t == "true" || t == "false" {
        return Ok(Lv::Bool(t == "true"));
    }
    for q in ['"', '\''] {
        if let Some(rest) = t.strip_prefix(q) {
            return match rest.strip_suffix(q) {
                Some(inner) if !inner.contains(q) => Ok(Lv::Str(inner.to_string())),
                _ => Err(()),
            };
        }
    }
    let body = t.strip_prefix('-').unwrap_or(t);
    let ok_int = !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit());
    if ok_int {
        return t.parse::<i64>().map(Lv::Int).map_err(|_| ());
    }
    if let Some((a, b)) = body.split_once('.') {
        if !a.is_empty() && !b.is_empty() && a.bytes().all(|d| d.is_ascii_digit()) && b.bytes().all(|d| d.is_ascii_digit()) {
            return t.parse::<f64>().map(Lv::Float).map_err(|_| ());
        }
    }
    Err(())
}

/// `[platform]` 节 `level` 键扫描（文件序节头切换；行尾串外 `#` 注释；
/// 点分/数组节头 = 非 platform 节）。Err = 坏 TOML 面。
fn find_platform_level(text: &str) -> Result<Lv, ()> {
    let mut in_platform = false;
    let mut found = Lv::None;
    for raw in text.split('\n') {
        let base = raw.trim_end_matches('\r');
        let t = match base.find('#') {
            Some(i) if !base[..i].contains('"') => base[..i].trim_end(),
            _ => base.trim_end(),
        };
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix('[') {
            in_platform = rest.trim_end_matches(']') == "platform";
            continue;
        }
        if !in_platform {
            continue;
        }
        if let Some(eq) = t.find('=') {
            let key = t[..eq].trim();
            if key == "level" {
                found = parse_lv(&t[eq + 1..])?;
            }
        } else {
            return Err(()); // 节内非键值行 = 坏 TOML 面
        }
    }
    Ok(found)
}

/// `abi_level` 镜像：Ok = level 值；Err = FAIL 文案（stderr 面）。
pub fn abi_level(path: &str) -> Result<i64, String> {
    let text = match std::fs::read_to_string(Path::new(path)) {
        Ok(t) => t,
        Err(e) => {
            return Err(format!(
                "bcts: FAIL: 读 {path} [platform].level 失败：{}",
                py_os_str(&e, path)
            ))
        }
    };
    let lv = match find_platform_level(&text) {
        Ok(v) => v,
        Err(()) => {
            return Err(format!(
                "bcts: FAIL: 读 {path} [platform].level 失败：TOML 解析失败（非目标值形/坏行——语法域文案与 tomllib 不同源，拒分支对齐）"
            ))
        }
    };
    match lv {
        Lv::Int(n) => Ok(n),
        other => Err(format!(
            "bcts: FAIL: {path} [platform].level 非整数：{}",
            lv_repr(&other)
        )),
    }
}

/// py universal newlines 面：`open(encoding=…)` 文本读把 `\r\n`/`\r` 归一为
/// `\n`——镜像同构（`replace` 归一后再行扫）。
fn normalize_newlines(s: &str) -> String {
    if s.contains('\r') {
        s.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        s.to_string()
    }
}

/// `(?m)^item .* platform_revision=(\d{10})$` 单行镜像：行以 `item ` 起 +
/// 「空格 + platform_revision= + 恰 10 位数字 + 行尾」。py `.*` 贪心回溯 =
/// 最右可行位取值（左位若可行则其后必有非行尾文本，矛盾——右优先即等价）；
/// `item ` 后紧贴无空格不匹配（needle 含前置空格——双空格形命中）。10 位后
/// 有余文（含 `\r`）= `$` 不达 → 该行不进 findall（py 同）。返回 None =
/// 不匹配。
fn line_pr(line: &str) -> Option<i64> {
    let after = line.strip_prefix("item ")?;
    let needle = " platform_revision=";
    let mut best: Option<i64> = None;
    let mut from = 0;
    while let Some(rel) = after[from..].find(needle) {
        let at = from + rel;
        let rest = &after[at + needle.len()..];
        if rest.len() == 10 && rest.bytes().all(|b| b.is_ascii_digit()) {
            best = rest.parse::<i64>().ok();
        }
        from = at + 1;
    }
    best
}

/// `img_pr_check` 镜像：Ok = stdout ok 行；Err = FAIL 文案（stderr 面）。
pub fn img_pr_check(manifest: &str, want: i64, anchor: &str) -> Result<String, String> {
    let text = match std::fs::read_to_string(Path::new(manifest)) {
        Ok(t) => t,
        Err(e) => {
            return Err(format!(
                "bcts: FAIL: 读 {manifest} 失败（mkimage 侧产物应与镜像同目录）：{}",
                py_os_str(&e, manifest)
            ))
        }
    };
    let text = normalize_newlines(&text);
    // py set 去重 → sorted 升序。
    let mut vals: Vec<i64> = Vec::new();
    for line in text.split('\n') {
        if let Some(v) = line_pr(line) {
            if !vals.contains(&v) {
                vals.push(v);
            }
        }
    }
    if vals.is_empty() {
        return Err(format!(
            "bcts: FAIL: {manifest} 无 platform_revision 条目行（manifest v1 产物面断裂——镜像非 mkimage 产链？）"
        ));
    }
    if vals.len() > 1 {
        vals.sort_unstable();
        return Err(format!(
            "bcts: FAIL: {manifest} platform_revision 条目混杂 {vals:?}（同盘同版断裂——mkimage 逐条目同源取值）"
        ));
    }
    let got = vals[0];
    if got != want {
        return Err(format!(
            "bcts: FAIL: 镜像 manifest platform_revision={got} != 回放清单 pr={want}（对账锚：{anchor}）——版本声称位对账断裂，不等 loud 拒（认证语义：镜像声称版本必须与回放冻结件同版）"
        ));
    }
    Ok(format!("bcts: 版本声称位对账 ok：manifest platform_revision={got} == {anchor}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_level_ok_real_source() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/abi.toml");
        let lv = abi_level(root).unwrap();
        assert!(lv >= 1, "level 须 ≥1（VER-01 单调轨道）");
    }

    #[test]
    fn abi_level_value_rejects() {
        let dir = std::env::temp_dir().join(format!("bctsrun-abil-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cases = [
            ("[platform]\nlevel = \"1\"\n", "非整数：'1'"),
            ("[platform]\nlevel = '1'\n", "非整数：'1'"),
            ("[platform]\nlevel = true\n", "非整数：True"),
            ("[meta]\nstage = \"v1\"\n", "非整数：None"),
            ("[platform]\nlevel = 1.5\n", "非整数：1.5"),
            ("[platform]\nlevel = 2.0\n", "非整数：2.0"),
            ("[platform]\nlevel = -3\n", "-3"), // 负 int：py isinstance(int) 收 → Ok(-3) 非拒（对照组见下）
        ];
        for (i, (text, kw)) in cases.iter().enumerate() {
            let p = dir.join(format!("m{i}.toml"));
            std::fs::write(&p, text).unwrap();
            let r = abi_level(p.to_str().unwrap());
            if i == 6 {
                assert_eq!(r.unwrap(), -3, "负 int py 收纳");
                continue;
            }
            let err = r.expect_err("应拒");
            assert!(err.starts_with("bcts: FAIL: "), "[{i}] 前缀：{err}");
            assert!(err.contains(kw), "[{i}] 期待含 {kw}：{err}");
        }
        // 坏 TOML 面（拒分支）
        let p = dir.join("bad.toml");
        std::fs::write(&p, "[platform]\nlevel = [1]\n").unwrap();
        assert!(abi_level(p.to_str().unwrap()).is_err());
        // 读失败面：缺席 + 目录
        assert!(abi_level(dir.join("nope.toml").to_str().unwrap()).unwrap_err().contains("失败：[Errno 2]"));
        assert!(abi_level(dir.to_str().unwrap()).unwrap_err().contains("Is a directory"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn img_pr_faces() {
        let dir = std::env::temp_dir().join(format!("bctsrun-imgpr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok_manifest = "berryos-manifest v1 format_frozen=2026-11-10 entries=04\n\
            item path=/EFI/BOOT/BOOTAA64.EFI   sha256=8387 platform_revision=0000000001\n\
            item path=/HELLO.TXT               sha256=e585 platform_revision=0000000001\n";
        let p = dir.join("ok.txt");
        std::fs::write(&p, ok_manifest).unwrap();
        // ok 面（前导零按值归一）
        let out = img_pr_check(p.to_str().unwrap(), 1, "锚Z").unwrap();
        assert_eq!(out, "bcts: 版本声称位对账 ok：manifest platform_revision=1 == 锚Z");
        // 不等拒（锚透传）
        let err = img_pr_check(p.to_str().unwrap(), 2, "锚A").unwrap_err();
        assert!(err.contains("platform_revision=1 != 回放清单 pr=2"));
        assert!(err.contains("对账锚：锚A"));
        // 混杂（set 去重 + sorted 升序 → [1, 10]）
        let mixed = ok_manifest.replace(
            "item path=/HELLO.TXT               sha256=e585 platform_revision=0000000001",
            "item path=/HELLO.TXT               sha256=e585 platform_revision=0000000010",
        );
        let p2 = dir.join("mixed.txt");
        std::fs::write(&p2, &mixed).unwrap();
        let err = img_pr_check(p2.to_str().unwrap(), 1, "锚").unwrap_err();
        assert!(err.contains("条目混杂 [1, 10]"), "{err}");
        // 同值前导零形不混杂（按值归一）
        let pad = ok_manifest.replace("platform_revision=0000000001\nitem", "platform_revision=1\nitem");
        let p2b = dir.join("pad.txt");
        std::fs::write(&p2b, &pad).unwrap();
        assert!(img_pr_check(p2b.to_str().unwrap(), 1, "锚").is_ok());
        // CRLF：py open() 文本读 universal newlines 归一 → 匹配 ok（parity 钉板）
        let p3 = dir.join("crlf.txt");
        std::fs::write(&p3, "berryos-manifest v1\r\nitem path=/X sha256=ab platform_revision=0000000001\r\n").unwrap();
        assert!(img_pr_check(p3.to_str().unwrap(), 1, "锚").is_ok());
        // 11 位数字不匹配（`\d{10}$` 回溯失败）→ 无条目拒
        let p4 = dir.join("bad.txt");
        std::fs::write(&p4, "item path=/X sha256=ab platform_revision=00000000011\n").unwrap();
        let err = img_pr_check(p4.to_str().unwrap(), 1, "锚").unwrap_err();
        assert!(err.contains("无 platform_revision 条目行"), "{err}");
        // 一行两处出现：贪心取最右可行位
        let p5 = dir.join("greedy.txt");
        std::fs::write(&p5, "item platform_revision=0000000002 platform_revision=0000000001\n").unwrap();
        assert!(img_pr_check(p5.to_str().unwrap(), 1, "锚").is_ok());
        assert!(img_pr_check(p5.to_str().unwrap(), 2, "锚").is_err());
        // 读失败面
        let err = img_pr_check(dir.join("nope.txt").to_str().unwrap(), 1, "锚").unwrap_err();
        assert!(err.contains("失败（mkimage 侧产物应与镜像同目录）：[Errno 2]"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
