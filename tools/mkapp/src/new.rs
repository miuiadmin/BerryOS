//! W1-Q27 八刀：`--new` 应用脚手架面（Python `do_new` mkapp.py :1514 镜像
//! ——SDK-1a §4.5 四件套）。模板四件 = include_str! 资产（零转录——Python
//! `SCAFFOLD_*` 同字节源；双侧同步义务：改任一侧模板须同步另一侧并重钉
//! new_parity 对拍）。
//!
//! 门序（Python 逐门同序）：basename(dir) == id.lower() → id `_check_str`
//! 镜像（length/charset，`key="new.id"`）→ ID_RE 字符集 → slug cargo 包名
//! 门（拒 '.'）→ isfile 拒 → 已存在非空拒（防覆写）→ 四件落盘 →
//! 自产自证（manifest 过全量门禁）。CLI 恒 str——`_check_str` 的 type 臂
//! 无面。files 摘要 = 写序落盘、`sorted()` 返回（Python 镜像）。

use std::fs;
use std::path::Path;

use crate::gate::{check_manifest_text, Reject};

pub const SCAFFOLD_MANIFEST: &str = include_str!("scaffold_manifest.toml.tpl");
pub const SCAFFOLD_MAIN: &str = include_str!("scaffold_main.rs.tpl");
pub const SCAFFOLD_CARGO: &str = include_str!("scaffold_cargo.toml.tpl");
pub const SCAFFOLD_README: &str = include_str!("scaffold_readme.md.tpl");

/// 脚手架产物摘要（`--json --new` result 舱 = Python `do_new` 返回 dict
/// 镜像：dir/id/files——files = `sorted()` 形）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSummary {
    pub dir: String,
    pub id: String,
    pub files: Vec<String>,
}

/// Python `str.format` 子集渲染：模板值域实测只含 `{{`/`}}` 字面花括号
/// 与 `{app_id}`/`{slug}` 占位（四模板审计钉板，new_parity 逐字节对拍）。
fn render(tpl: &str, app_id: &str, slug: &str) -> String {
    let mut out = String::with_capacity(tpl.len());
    let mut rest = tpl;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let c = rest.as_bytes()[i];
        let after = &rest[i + 1..];
        if c == b'{' {
            if let Some(name_end) = after.find('}') {
                match &after[..name_end] {
                    // {{…}} 字面段：先查占位再看双花括号（模板域无冲突形）
                    "app_id" => {
                        out.push_str(app_id);
                        rest = &after[name_end + 1..];
                        continue;
                    }
                    "slug" => {
                        out.push_str(slug);
                        rest = &after[name_end + 1..];
                        continue;
                    }
                    _ => {}
                }
            }
            if after.starts_with('{') {
                out.push('{');
                rest = &after[1..];
            } else {
                out.push('{');
                rest = after;
            }
        } else {
            if after.starts_with('}') {
                out.push('}');
                rest = &after[1..];
            } else {
                out.push('}');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Python `repr()` 单引号形（ASCII 路径域——CLI 参数无引号/反斜杠字节，
/// 对拍面固定；exotic 输入两侧都进不了本函数）。
fn py_repr(s: &str) -> String {
    format!("'{s}'")
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn new_reject(msg: String, code: &'static str, where_: &str) -> Reject {
    Reject {
        message: msg,
        code,
        where_: leak(where_),
        line: None,
        jx_where: None,
        jx_expected: None,
        jx_got: None,
        jx_fix: None,
    }
}

/// id `_check_str` 镜像（`key="new.id"`：length/charset 两臂；CLI 恒 str
/// 故 type 臂无面）。长度按字符数（Python `len()` 语义）。
fn check_id(app_id: &str) -> Result<(), Reject> {
    let n = app_id.chars().count();
    if !(1..=crate::ID_MAX).contains(&n) {
        return Err(Reject::jx(
            format!("manifest: id 空/超长（1..={}）", crate::ID_MAX),
            "manifest.new.id.length",
            "new.id",
            Some(format!("\"1..={}\"", crate::ID_MAX)),
            Some(n.to_string()),
            None,
        ));
    }
    for ch in app_id.chars() {
        if !(('\u{20}'..='\u{7e}').contains(&ch)) || ch == '\\' || ch == '"' {
            return Err(Reject::jx(
                format!("manifest: id 非法字符 {}", py_repr(&ch.to_string())),
                "manifest.new.id.charset",
                "new.id",
                None,
                None,
                None,
            ));
        }
    }
    Ok(())
}

/// `--new` 本体（Python `do_new` 镜像）：人读行走 `NewSummary::line`（CLI
/// 层打），本面返回摘要供信封/人读双消费。
pub fn do_new(app_dir: &str, app_id: &str) -> Result<NewSummary, Reject> {
    // basename(normpath(dir))——Path::file_name 同归一（尾斜杠折掉）
    let slug = Path::new(app_dir)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if slug != app_id.to_lowercase() {
        return Err(Reject {
            message: format!(
                "--new: basename({}) = {} != --id {} 的小写形",
                py_repr(app_dir),
                py_repr(&slug),
                py_repr(app_id)
            ),
            code: "new.basename",
            where_: leak(app_dir),
            line: None,
            jx_where: None,
            jx_expected: Some(format!("\"{}\"", app_id.to_lowercase())),
            jx_got: Some(format!("\"{}\"", slug)),
            jx_fix: None,
        });
    }
    check_id(app_id)?;
    // ID_RE：首字符字母数字、余 [A-Za-z0-9._-]（code 无 manifest. 前缀——
    // Python 此门 code="new.id" 原样）
    let id_ok = app_id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && app_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !id_ok {
        return Err(Reject::jx(
            "manifest: id 字符集 [A-Za-z0-9._-]、首字符字母数字".to_string(),
            "new.id",
            "id",
            Some("\"[A-Za-z0-9._-]、首字符字母数字\"".to_string()),
            None,
            None,
        ));
    }
    // slug = Cargo.toml 包名——cargo 拒 '.'（ID_RE 宽松面在 --new 收紧）
    let slug_ok = slug.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'));
    if !slug_ok {
        return Err(Reject::jx(
            format!(
                "--new: slug {} 不是合法 cargo 包名（含点号等 cargo 拒收字符；id 勿含 '.'，请改用 '-' 或 '_'）",
                py_repr(&slug)
            ),
            "new.slug.cargo",
            leak(&slug),
            None,
            None,
            Some("\"id 勿含 '.'，改用 '-' 或 '_'\"".to_string()),
        ));
    }
    let p = Path::new(app_dir);
    if p.is_file() {
        return Err(new_reject(
            format!("--new: {app_dir} 是文件"),
            "new.isfile",
            app_dir,
        ));
    }
    if p.is_dir() && fs::read_dir(p).map_or(false, |mut d| d.next().is_some()) {
        return Err(Reject::jx(
            format!("--new: {app_dir} 已存在且非空（防覆写）"),
            "new.exists",
            leak(app_dir),
            None,
            None,
            Some("\"换新目录或先清空\"".to_string()),
        ));
    }
    // 写序 = Python dict 插入序（manifest → src/main.rs → Cargo → README）
    let manifest = render(SCAFFOLD_MANIFEST, app_id, &slug);
    let files: [(&str, String); 4] = [
        ("manifest.toml", manifest.clone()),
        ("src/main.rs", render(SCAFFOLD_MAIN, app_id, &slug)),
        ("Cargo.toml", render(SCAFFOLD_CARGO, app_id, &slug)),
        ("README.md", render(SCAFFOLD_README, app_id, &slug)),
    ];
    for (rel, text) in &files {
        let path = p.join(rel);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent); // makedirs(exist_ok=True) 镜像
        }
        fs::write(&path, text).map_err(|e| {
            new_reject(
                format!("--new: {} 写失败 {e}", path.display()),
                "io.write",
                &path.display().to_string(),
            )
        })?;
    }
    // 自产自证：脚手架 manifest 必须过全量门禁（§4.5 尾约——防模板漂移）
    check_manifest_text(&manifest)?;
    Ok(NewSummary {
        dir: app_dir.to_string(),
        id: app_id.to_string(),
        files: {
            let mut v: Vec<String> = files.iter().map(|(r, _)| r.to_string()).collect();
            v.sort();
            v
        },
    })
}

impl NewSummary {
    /// result 舱 JSON（Python `do_new` 返回 dict 插入序镜像：dir/id/files）。
    pub fn result_json(&self) -> String {
        let mut s = format!(
            "{{\"dir\":{},\"id\":{}",
            crate::json::esc(&self.dir),
            crate::json::esc(&self.id)
        );
        s.push_str(",\"files\":[");
        let mut first = true;
        for f in &self.files {
            if !first {
                s.push(',');
            }
            first = false;
            s.push_str(&crate::json::esc(f));
        }
        s.push_str("]}");
        s
    }

    /// 人读行（Python `emit=True` 打印镜像）。
    pub fn line(&self) -> String {
        format!(
            "mkapp: {} 脚手架四件就绪（{}；构建步骤见 {}/README.md）",
            self.dir, self.id, self.dir
        )
    }
}

#[cfg(test)]
mod new_tests {
    use super::*;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mkapp-new-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn render_brace_escape_and_placeholders() {
        // {{ → {、}} → }、占位替换（SCAFFOLD_MAIN 的 fn main() {{ … }} 段）
        let r = render(SCAFFOLD_MAIN, "MYAPP", "myapp");
        assert!(r.contains("fn main() -> ExitCode {"));
        assert!(r.contains("println!(\"app MYAPP up\");"));
        assert!(r.contains("Some(\"bin/myapp\"),"));
        assert!(!r.contains("{{"));
        assert!(!r.contains("{app_id}"));
        assert!(!r.contains("{slug}"));
        let m = render(SCAFFOLD_MANIFEST, "X9_", "x9_");
        assert!(m.contains("id = \"X9_\"\n"));
        assert!(m.contains("entry = \"bin/x9_\"\n"));
        let c = render(SCAFFOLD_CARGO, "X9_", "x9_");
        assert!(c.contains("name = \"x9_\"\n"));
        let rd = render(SCAFFOLD_README, "X9_", "x9_");
        assert!(rd.contains("# X9_ —— BerryOS 应用"));
        assert!(rd.contains("bins/x9_/Cargo.toml"));
        assert!(rd.contains("/tmp/X9_.bap"));
    }

    #[test]
    fn do_new_roundtrip_gate_and_files() {
        let td = tmpdir("rt");
        let d = td.join("myapp");
        let sum = do_new(d.to_str().unwrap(), "MYAPP").unwrap();
        assert_eq!(sum.id, "MYAPP");
        assert_eq!(sum.files, vec!["Cargo.toml", "README.md", "manifest.toml", "src/main.rs"]);
        for rel in &sum.files {
            assert!(d.join(rel).is_file(), "缺 {rel}");
        }
        let m = fs::read_to_string(d.join("manifest.toml")).unwrap();
        assert!(check_manifest_text(&m).is_ok(), "自产自证失败");
        assert!(sum.line().contains("MYAPP"));
        let j = sum.result_json();
        assert!(j.starts_with("{\"dir\":"));
        assert!(j.contains("\"files\":[\"Cargo.toml\",\"README.md\",\"manifest.toml\",\"src/main.rs\"]"));
        let _ = fs::remove_dir_all(&td);
    }

    #[test]
    fn do_new_rejects_full_ladder() {
        let td = tmpdir("rej");
        let s = |n: &str| td.join(n).to_string_lossy().into_owned();

        // ① basename 门
        let e = do_new(&s("other"), "MYAPP").unwrap_err();
        assert_eq!(e.code, "new.basename");
        assert!(e.message.contains("!= --id 'MYAPP' 的小写形"));
        assert_eq!(e.jx_expected.as_deref(), Some("\"myapp\""));
        assert_eq!(e.jx_got.as_deref(), Some("\"other\""));
        // ② id 空（length）
        let e = do_new("", "").unwrap_err();
        assert_eq!(e.code, "manifest.new.id.length");
        // ③ id 超长 25 字符
        let long_id = "A".repeat(25);
        let e = do_new(&s(&long_id.to_lowercase()), &long_id).unwrap_err();
        assert_eq!(e.code, "manifest.new.id.length");
        assert_eq!(e.jx_got.as_deref(), Some("25"));
        // ④ 非法首字符（ID_RE——code 无 manifest. 前缀）
        let e = do_new(&s("-bad"), "-BAD").unwrap_err();
        assert_eq!(e.code, "new.id");
        assert_eq!(e.where_, "id");
        // ⑤ slug cargo 门：id 含点号
        let e = do_new(&s("app.t2"), "APP.T2").unwrap_err();
        assert_eq!(e.code, "new.slug.cargo");
        assert!(e.message.contains("不是合法 cargo 包名"));
        assert_eq!(e.jx_fix.as_deref(), Some("\"id 勿含 '.'，改用 '-' 或 '_'\""));
        // ⑥ isfile 门
        let f = td.join("afile");
        fs::write(&f, b"x").unwrap();
        let e = do_new(f.to_str().unwrap(), "AFILE").unwrap_err();
        assert_eq!(e.code, "new.isfile");
        // ⑦ 已存在非空（防覆写——原文件不动）
        let d = td.join("busy");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("keep"), b"x").unwrap();
        let e = do_new(d.to_str().unwrap(), "BUSY").unwrap_err();
        assert_eq!(e.code, "new.exists");
        assert_eq!(e.jx_fix.as_deref(), Some("\"换新目录或先清空\""));
        assert_eq!(fs::read(d.join("keep")).unwrap(), b"x");
        // 已存在空目录 = 放行（makedirs exist_ok 语义）
        let d2 = td.join("empty-ok");
        fs::create_dir_all(&d2).unwrap();
        assert!(do_new(d2.to_str().unwrap(), "EMPTY-OK").is_ok());
        let _ = fs::remove_dir_all(&td);
    }

    #[test]
    fn do_new_second_run_rejected_no_overwrite() {
        let td = tmpdir("idem");
        let d = td.join("myapp");
        assert!(do_new(d.to_str().unwrap(), "MYAPP").is_ok());
        assert_eq!(do_new(d.to_str().unwrap(), "MYAPP").unwrap_err().code, "new.exists");
        let _ = fs::remove_dir_all(&td);
    }

    #[test]
    fn result_json_shape_mirror_python() {
        let sum = NewSummary {
            dir: "/tmp/x/myapp".to_string(),
            id: "MYAPP".to_string(),
            files: vec![
                "Cargo.toml".into(),
                "README.md".into(),
                "manifest.toml".into(),
                "src/main.rs".into(),
            ],
        };
        assert_eq!(
            sum.result_json(),
            "{\"dir\":\"/tmp/x/myapp\",\"id\":\"MYAPP\",\"files\":[\"Cargo.toml\",\"README.md\",\"manifest.toml\",\"src/main.rs\"]}"
        );
    }
}
