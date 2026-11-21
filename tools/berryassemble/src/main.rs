//! berryassemble Rust CLI（三刀·CLI 消费半边：`--check-lite` 臂换轨——
//! b 层 #6 刀序第六位第三段）。出口码同 Python：拒因 1、用法错 2、
//! 成功 0。`--selftest`/`--sample`/`--check`/`--dry-run`/`--emit` 不镜像
//! （berry_assemble.py 现役单源——遇即让位指引 python3，gensourceindex
//! `--preset` 同款纪律；全量面〔基座对账/kernel 锚度量/产件〕与 import
//! 面仍归 Python 件，后续刀评估）。
//!
//! 判序逐行镜像 Python `main()`（:1846）：顺序旗标循环（--platform-
//! revision 值解析/互斥单模式/--out-dir 值对）→ 模式分派 → 无模式
//! 落用法。`--check-lite` = `do_check(lite=True)` 镜像：读文件 →
//! `lite::load_lite_product` 结构面执法 → OK 行/拒行逐字同构
//! （`tests/cli_parity.rs` oracle 真跑对拍钉板）。

use std::process::ExitCode;

use berryassemble::lite::{load_lite_product, APP_SLOTS};
use berryassemble::validate::{format_check_ok, validate_all};

const USAGE: &str = "用法：berryassemble --check | --check-lite <product.toml>（--dry-run/--emit/--selftest/--sample 走 python3 tools/berry_assemble.py——CLI oracle 面保留）";

/// 五刀·tools/ 目录发现（gensourceindex `repo_tools_dir` 同款先例——
/// CWD/exe 双上溯）。`--check` 默认基座表/user 基准 = Python `ROOT`/
/// `REPO`（__file__ 相对）的 Rust 等价物：exe 在
/// tools/berryassemble/cargo-target/<host>/debug 内，上溯三级即 tools/。
fn repo_tools_dir() -> std::path::PathBuf {
    for base in [
        std::env::current_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf())),
    ] {
        let Some(mut dir) = base else { continue };
        loop {
            if dir.file_name().map(|f| f == "tools").unwrap_or(false) {
                return dir.to_path_buf();
            }
            if dir.join("abi.toml").is_file() && dir.join("berry_assemble.py").is_file() {
                return dir.to_path_buf();
            }
            match dir.parent() {
                Some(p) => dir = p.to_path_buf(),
                None => break,
            }
        }
    }
    std::env::current_dir().unwrap_or_default().join("tools")
}

/// Python repr 串面（单引号 + 反斜杠/引号转义——f"{v!r}" 镜像）。
fn py_repr(s: &str) -> String {
    let inner = s
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!("'{inner}'")
}

/// Python OSError str 形镜像（open() 失败三常见态；余态落 io 原文）。
fn py_oserror_str(e: &std::io::Error, path: &str) -> String {
    match e.raw_os_error() {
        Some(2) => format!("[Errno 2] No such file or directory: '{path}'"),
        Some(13) => format!("[Errno 13] Permission denied: '{path}'"),
        Some(21) => format!("[Errno 21] Is a directory: '{path}'"),
        _ => e.to_string(),
    }
}

/// `do_check(lite=True)` 镜像：OK 行逐字同构（计数/开关位全对齐）。
fn cmd_check_lite(path: &str) -> ExitCode {
    let text = match read_product_text(path) {
        Ok(t) => t,
        Err(()) => return ExitCode::from(1),
    };
    match load_lite_product(&text) {
        Ok(p) => {
            println!(
                "berry-assemble: {path} OK[lite·结构面]（product={} board={} preset={}/{} override={} locale={} trust={}——产物面对账〔branding/override/trust/source 件、基座表、kernel 锚度量〕归 --check）",
                p.name,
                p.board,
                p.preset.len(),
                APP_SLOTS,
                p.apps_override.len(),
                if p.locale_on() { "on" } else { "off" },
                if p.trust_pk_anchor.is_empty() { "off" } else { "on" },
            );
            ExitCode::SUCCESS
        }
        // 解析拒面不带 {path}: 前缀（Python load_product 文件面入口带、
        // 结构面文案本体不带——首刀 parity 先例：分支标识对齐）。
        Err(e) => {
            eprintln!("berry-assemble: {e}");
            ExitCode::from(1)
        }
    }
}

/// product.toml 读面（--check/--check-lite 共用——Python `load_product`
/// 文件入口 :751-760 镜像：读失败 `读 {path} 失败：{OSError}`、坏 UTF-8
/// 落 TOML 解析面分支标识）。Err = 已印拒行（调用方直接返回 exit 1）。
fn read_product_text(path: &str) -> Result<String, ()> {
    match std::fs::read(path) {
        Ok(b) => match String::from_utf8(b) {
            Ok(t) => Ok(t),
            // tomllib 侧坏 UTF-8 同落 TOML 解析面（分支标识对齐——
            // 首刀先例：解析器文案不同源，逐字统一归后续刀）。
            Err(_) => {
                eprintln!(
                    "berry-assemble: {path}: TOML 解析失败： 行 1 无效 UTF-8 字节"
                );
                Err(())
            }
        },
        Err(e) => {
            eprintln!("berry-assemble: 读 {path} 失败：{}", py_oserror_str(&e, path));
            Err(())
        }
    }
}

/// `do_check` 全量臂（Python :1009-1019 镜像——五刀收口）。默认基座表/
/// user 基准 = tools/ 发现（`DEFAULT_BASE`/`DEFAULT_USER_DIR` 等价物）。
fn cmd_check(path: &str) -> ExitCode {
    let text = match read_product_text(path) {
        Ok(t) => t,
        Err(()) => return ExitCode::from(1),
    };
    // Python src_dir = dirname(abspath(path))；基座表 path 相对 user/。
    let tools = repo_tools_dir();
    let base = tools.join("rootfs-manifest.toml").display().to_string();
    let user_dir = tools
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("user")
        .display()
        .to_string();
    let src_dir = std::path::Path::new(path)
        .parent()
        .map(|p| {
            let s = p.display().to_string();
            if s.is_empty() { ".".to_string() } else { s }
        })
        .unwrap_or_else(|| ".".to_string());
    match validate_all(&text, &src_dir, &base, &user_dir) {
        Ok((p, _, _)) => {
            println!("{}", format_check_ok(path, &p));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("berry-assemble: {e}");
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // Python main() 顺序循环逐行镜像（:1852-1886）。
    let mut mode: Option<&str> = None;
    let mut path: Option<&str> = None;
    let mut out_dir: Option<&str> = None;
    let mut i = 0usize;
    while i < argv.len() {
        if argv[i] == "--platform-revision" {
            if i + 1 >= argv.len() {
                eprintln!("berry-assemble: --platform-revision 需要 <N> 参数");
                return ExitCode::from(2);
            }
            // Python int() 容忍前后空白与 '+' 号（trim+parse 同域）。
            if argv[i + 1].trim().parse::<i64>().is_err() {
                eprintln!(
                    "berry-assemble: 坏 --platform-revision 值 {}",
                    py_repr(&argv[i + 1])
                );
                return ExitCode::from(2);
            }
            i += 2;
            continue;
        }
        if argv[i] == "--out-dir" && i + 1 < argv.len() {
            out_dir = Some(argv[i + 1].as_str());
            i += 2;
            continue;
        }
        if matches!(argv[i].as_str(), "--check" | "--check-lite" | "--dry-run" | "--emit")
            && i + 1 < argv.len()
        {
            if mode.is_some() {
                eprintln!("berry-assemble: --check/--check-lite/--dry-run/--emit 互斥（单模式）");
                return ExitCode::from(2);
            }
            mode = Some(argv[i].as_str());
            path = Some(argv[i + 1].as_str());
            i += 2;
            continue;
        }
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match mode {
        Some("--check-lite") => {
            // out_dir 静默忽略（Python 判序同构：模式分派先于 out_dir 单裁）。
            cmd_check_lite(path.expect("mode 带值"))
        }
        Some("--emit") => {
            // Python :1900 判序先于执行让位——缺 --out-dir 精确文案。
            if out_dir.is_none() {
                eprintln!(
                    "berry-assemble: --emit 需 --out-dir <dir>（产件落点——etc/boot-params.toml 所在）"
                );
                return ExitCode::from(2);
            }
            eprintln!(
                "berry-assemble: --emit 走 python3 tools/berry_assemble.py（产件面现役单源——Rust 件不镜像，让位指引）"
            );
            ExitCode::from(2)
        }
        Some("--check") => {
            // out_dir 静默忽略（Python :1887 判序同构——模式分派先于执行）。
            cmd_check(path.expect("mode 带值"))
        }
        Some("--dry-run") => {
            eprintln!(
                "berry-assemble: --dry-run 走 python3 tools/berry_assemble.py（计划面 build_plot 现役单源——让位指引）"
            );
            ExitCode::from(2)
        }
        _ => {
            if out_dir.is_some() {
                eprintln!("berry-assemble: --out-dir 只随 --emit（计划面模式不产件）");
                return ExitCode::from(2);
            }
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use berryassemble::lite::LiteProduct;

    #[test]
    fn py_repr_law() {
        assert_eq!(py_repr("abc"), "'abc'");
        assert_eq!(py_repr("a'b"), "'a\\'b'");
        assert_eq!(py_repr("a\\b"), "'a\\\\b'");
    }

    #[test]
    fn oserror_str_law() {
        let e = std::io::Error::from_raw_os_error(2);
        assert_eq!(
            py_oserror_str(&e, "x.toml"),
            "[Errno 2] No such file or directory: 'x.toml'"
        );
    }

    #[test]
    fn locale_on_law() {
        let mut p = LiteProduct::default();
        assert!(!p.locale_on());
        p.locale_timezone = "UTC".into();
        assert!(p.locale_on());
    }
}
