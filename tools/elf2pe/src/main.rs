//! CLI 壳——与 Python 版 main() 参数面/输出行逐字对齐。
//!
//! 用法：elf2pe <in.elf> <out.efi>
//! 自检：elf2pe --selftest（全链钉板走 cargo test，tests/selftest.rs——
//! bin 面只做分发，纯面全在 lib 挂 just test-host）。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--selftest") {
        // 钉板五步纪律：selftest 面挂 just test-host（cargo test），bin 入口
        // 保持与 Python CLI 同形（a-3 U4 门禁件自身必须有门）。
        eprintln!("elf2pe: --selftest 走 cargo test（tools/elf2pe，tests/selftest.rs）");
        return ExitCode::SUCCESS;
    }
    if args.len() < 2 || args[0].starts_with('-') {
        eprintln!("用法：elf2pe <in.elf> <out.efi>（--selftest 走 cargo test）");
        return ExitCode::from(2);
    }
    let data = match std::fs::read(&args[0]) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("elf2pe: 读 {} 失败：{e}", args[0]);
            return ExitCode::from(1);
        }
    };
    let relocs = match elf2pe::collect_relocs(&elf2pe::Elf::parse(&data).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    })) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let out = match elf2pe::convert(&data) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let line = match elf2pe::verify(&out.pe, relocs.len()) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = std::fs::write(&args[1], &out.pe) {
        eprintln!("elf2pe: 写 {} 失败：{e}", args[1]);
        return ExitCode::from(1);
    }
    println!("{line} -> {} {}B", args[1], out.pe.len());
    ExitCode::SUCCESS
}
