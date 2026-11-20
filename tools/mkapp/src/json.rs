//! `--json` 信封面（W1-Q27 七刀）：Python `_json_envelope`/`_json_main`
//! （mkapp.py :1569/:1610 段）的 Rust 镜像——SDK-2 §二 机读单行 JSON 信封。
//!
//! 范围 = CLI 面 build/check 两 op（产链现役形态）；`--new`/`--sign-key`/
//! `--verify-key` 仍归 Python oracle 面。拒因舱恒 7 键（code/where/line/
//! expected/got/fix/message——Python `Reject` kwargs 镜像，Rust 件
//! `expected`/`got`/`fix` 三舱由本刀入册的钉板类站点填充）；`io` 兜底 =
//! 各读/写站点 `io.read`/`io.write` code + `OSError` 语义的 message。
//! 单源对齐义务：改任一侧必须同步另一侧（五刀/六刀同款纪律）。
//!
//! 字段序（Python dict 插入序镜像，`json.dumps` 无空格分隔符同构）：
//! 成功 `{"tool":"mkapp","op":"build","ok":true,"result":{...}}`；
//! 拒因 `{"tool":"mkapp","op":"build","ok":false,"error":{...}}`。

use crate::build::BuildSummary;
use crate::check::CheckSummary;
use crate::gate::Reject;

/// JSON 字符串字面量转义（RFC 8259 双引号串子集——值域已过门禁可打印
/// ASCII，转义面只剩 `"` 与 `\`；控制字符走 `\u00XX` 防御面）。
pub(crate) fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// BuildSummary → result 舱（Python `build()` 返回 dict 插入序镜像：
/// out/bytes/id/version/console/stack_pages/intent 固定 7 键 + 条件键
/// compat→min_level→abi_revision→fs→net→icons/views→actions——无 UI 时
/// icons/views 不出现，与 Python `if m.ui:` 分支同构）。
fn build_result_json(s: &BuildSummary) -> String {
    let mut r = String::from("{\"out\":");
    r.push_str(&esc(&s.out));
    r.push_str(&format!(",\"bytes\":{},\"id\":", s.bytes));
    r.push_str(&esc(&s.id));
    r.push_str(",\"version\":");
    r.push_str(&esc(&s.version));
    r.push_str(&format!(
        ",\"console\":{},\"stack_pages\":{},\"intent\":",
        s.console, s.stack_pages
    ));
    r.push_str(&esc(&s.intent));
    if let Some(c) = &s.compat {
        r.push_str(",\"compat\":");
        r.push_str(&esc(c));
        if let Some(ml) = s.min_level {
            r.push_str(&format!(",\"min_level\":{ml}"));
        }
    }
    r.push_str(&format!(",\"abi_revision\":{}", s.abi_revision));
    if let Some(fs) = &s.fs {
        r.push_str(",\"fs\":");
        r.push_str(&esc(fs));
    }
    if let Some(net) = &s.net {
        r.push_str(",\"net\":");
        r.push_str(&esc(net));
    }
    if s.icons != 0 || s.views != 0 {
        r.push_str(&format!(",\"icons\":{},\"views\":{}", s.icons, s.views));
    }
    if s.actions != 0 {
        r.push_str(&format!(",\"actions\":{}", s.actions));
    }
    r.push('}');
    r
}

/// CheckSummary → result 舱（Python `check_bap` 返回 dict 镜像：path/
/// bytes/entries/digests——`--json --check` 首消费者即 `check` op 信封）。
fn check_result_json(s: &CheckSummary, path: &str) -> String {
    format!(
        "{{\"path\":{},\"bytes\":{},\"entries\":{},\"digests\":{}}}",
        esc(path),
        s.bytes,
        s.entries,
        esc(s.digests)
    )
}

/// 拒因舱 → 7 键 JSON（Python `_json_envelope` `err` dict 插入序镜像：
/// code/where/line/expected/got/fix/message；line 仅语法层 int、其余域
/// None——本面用空串代 None 的字段缺席形。非字符串舱（expected/got int
/// 或 list）由 `Reject.jx_*` 装载面提供原始 JSON 片段）。
fn reject_error_json(r: &Reject) -> String {
    let mut e = String::from("{\"code\":");
    e.push_str(&esc(r.code));
    e.push_str(",\"where\":");
    match &r.jx_where {
        Some(w) => e.push_str(w),
        None => e.push_str(&esc(r.where_)),
    }
    e.push_str(",\"line\":");
    match r.line {
        Some(n) => e.push_str(&format!("{n}")),
        None => e.push_str("null"),
    }
    for (key, opt) in [("expected", &r.jx_expected), ("got", &r.jx_got), ("fix", &r.jx_fix)] {
        e.push(',');
        e.push_str(&format!("\"{key}\":"));
        match opt {
            Some(v) => e.push_str(v),
            None => e.push_str("null"),
        }
    }
    e.push_str(",\"message\":");
    e.push_str(&esc(&r.message));
    e.push('}');
    e
}

/// 信封执行器（Python `_json_envelope` 镜像）：fn() → Ok(result JSON 片段)
/// | Err(Reject) → (单行信封文本, 退出码 0|1)。
pub fn envelope(op: &str, run: impl FnOnce() -> Result<String, Reject>) -> (String, i32) {
    match run() {
        Ok(result) => (
            format!("{{\"tool\":\"mkapp\",\"op\":{},\"ok\":true,\"result\":{result}}}", esc(op)),
            0,
        ),
        Err(r) => (
            format!(
                "{{\"tool\":\"mkapp\",\"op\":{},\"ok\":false,\"error\":{}}}",
                esc(op),
                reject_error_json(&r)
            ),
            1,
        ),
    }
}

/// CLI `--json` 分派（Python `_json_main` 镜像，build/check/new 三 op）：
/// `--json <out.bap> <app-dir>` | `--json --check <path.bap>` |
/// `--json --new <dir> --id <ID>`。其余形态（含 `--selftest`/签名面旗标
/// 混排）= 用法错 → `None`（caller 打 stderr 用法 + exit 2——双模式同形）。
pub fn json_main_dispatch(args: &[String]) -> Option<(String, i32)> {
    if args.len() == 2 && args[0] == "--check" {
        let path = args[1].clone();
        return Some(envelope("check", || {
            crate::check::check_bap(&path).map(|s| check_result_json(&s, &path))
        }));
    }
    if args.len() == 4 && args[0] == "--new" && args[2] == "--id" {
        let dir = args[1].clone();
        let id = args[3].clone();
        return Some(envelope("new", || {
            crate::new::do_new(&dir, &id).map(|s| s.result_json())
        }));
    }
    if args.len() == 2 && !args[0].starts_with('-') {
        let out = args[0].clone();
        let dir = args[1].clone();
        return Some(envelope("build", || {
            crate::build::build(&out, &dir).map(|s| build_result_json(&s))
        }));
    }
    None
}

#[cfg(test)]
mod json_tests {
    use super::*;
    use crate::gate::Reject;

    fn base_reject() -> Reject {
        Reject {
            message: "x".to_string(),
            code: "manifest.app.id.length",
            where_: "app.id",
            line: None,
            jx_where: None,
            jx_expected: None,
            jx_got: None,
            jx_fix: None,
        }
    }

    #[test]
    fn envelope_ok_shape() {
        let (text, rc) = envelope("build", || Ok("{\"bytes\":123}".to_string()));
        assert_eq!(rc, 0);
        assert_eq!(
            text,
            "{\"tool\":\"mkapp\",\"op\":\"build\",\"ok\":true,\"result\":{\"bytes\":123}}"
        );
        assert!(!text.contains('\n'), "单行纪律");
    }

    #[test]
    fn envelope_reject_7keys_order() {
        let mut r = base_reject();
        r.jx_expected = Some("\"1..=24\"".to_string());
        r.jx_got = Some("0".to_string());
        let (text, rc) = envelope("build", || Err(r));
        assert_eq!(rc, 1);
        // 恒 7 键 + Python 插入序
        let want = concat!(
            "{\"tool\":\"mkapp\",\"op\":\"build\",\"ok\":false,",
            "\"error\":{\"code\":\"manifest.app.id.length\",\"where\":\"app.id\",\"line\":null,",
            "\"expected\":\"1..=24\",\"got\":0,\"fix\":null,\"message\":\"x\"}}"
        );
        assert_eq!(text, want);
    }

    #[test]
    fn envelope_reject_line_int_passthrough() {
        let mut r = base_reject();
        r.code = "manifest.syntax.toml";
        r.line = Some(3);
        let (text, _) = envelope("build", || Err(r));
        assert!(text.contains("\"line\":3,"), "语法层 line 恒 int：{text}");
    }

    #[test]
    fn escape_quotes_backslash_control() {
        assert_eq!(esc("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(esc("\u{1}x"), "\"\\u0001x\"");
        assert_eq!(esc("中文"), "\"中文\"");
        // 非门禁面防御：控制字符不裸出
        assert!(!esc("a\u{7}b").contains('\u{7}'));
    }

    #[test]
    fn build_result_field_order_no_ui() {
        let s = BuildSummary {
            out: "/tmp/X.bap".to_string(),
            bytes: 100,
            id: "X".to_string(),
            version: "0.1.0".to_string(),
            console: true,
            stack_pages: 8,
            intent: "utility".to_string(),
            compat: Some("0.1.0..0.1.0".to_string()),
            min_level: Some(2),
            abi_revision: 1,
            fs: Some("rootfs".to_string()),
            net: None,
            icons: 0,
            views: 0,
            actions: 0,
        };
        let j = build_result_json(&s);
        // Python dict 插入序：out..intent + compat→min_level→abi_revision→fs
        let want = concat!(
            "{\"out\":\"/tmp/X.bap\",\"bytes\":100,\"id\":\"X\",\"version\":\"0.1.0\",",
            "\"console\":true,\"stack_pages\":8,\"intent\":\"utility\",",
            "\"compat\":\"0.1.0..0.1.0\",\"min_level\":2,\"abi_revision\":1,\"fs\":\"rootfs\"}"
        );
        assert_eq!(j, want);
    }

    #[test]
    fn build_result_conditional_keys() {
        // 无 UI/无 actions：icons/views/actions 键不出现（Python `if m.ui:`
        // `if m.agent_actions:` 分支镜像）
        let s = BuildSummary {
            out: "o".to_string(),
            bytes: 1,
            id: "I".to_string(),
            version: "0.1.0".to_string(),
            console: false,
            stack_pages: 4,
            intent: "background".to_string(),
            compat: None,
            min_level: None,
            abi_revision: 5,
            fs: None,
            net: Some("client".to_string()),
            icons: 0,
            views: 0,
            actions: 0,
        };
        let j = build_result_json(&s);
        assert!(!j.contains("\"icons\""));
        assert!(!j.contains("\"actions\""));
        assert!(j.contains("\"net\":\"client\""));
        // 有 UI：icons+views 成对出现（Python 恒 `icons=n views=n` 对）
        let s2 = BuildSummary { icons: 2, views: 1, actions: 3, ..s };
        let j2 = build_result_json(&s2);
        assert!(j2.contains("\"icons\":2,\"views\":1"));
        assert!(j2.contains("\"actions\":3"));
    }

    #[test]
    fn check_result_shape() {
        let s = CheckSummary { bytes: 42, entries: 3, digests: "ok" };
        let j = check_result_json(&s, "/tmp/a.bap");
        assert_eq!(
            j,
            "{\"path\":\"/tmp/a.bap\",\"bytes\":42,\"entries\":3,\"digests\":\"ok\"}"
        );
    }

    #[test]
    fn dispatch_forms() {
        // build/check 两形 Some；--selftest/--new/空/签名面混排 None（用法错）
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(json_main_dispatch(&a(&["--check", "x.bap"])).is_some());
        assert!(json_main_dispatch(&a(&["out.bap", "dir"])).is_some());
        assert!(json_main_dispatch(&a(&["--selftest"])).is_none());
        assert!(json_main_dispatch(&a(&["--new", "d", "--id", "X"])).is_some());
        // 变形：缺 --id / --id 不在位 = 用法错
        assert!(json_main_dispatch(&a(&["--new", "d"])).is_none());
        assert!(json_main_dispatch(&a(&["--new", "d", "X", "--id"])).is_none());
        assert!(json_main_dispatch(&a(&["--sign-key", "k", "o.bap", "d"])).is_none());
        assert!(json_main_dispatch(&[]).is_none());
    }
}
