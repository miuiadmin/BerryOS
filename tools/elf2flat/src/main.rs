//! CLI 壳——与 Python 版 main() 参数面/输出行逐字对齐。
//!
//! 用法：elf2flat <in.elf> <out.bin> [--page 4096] [--max N] [--base 0x1000]
//! 自检：elf2flat --selftest（双段钉板 + 8 拒面，见 tests/selftest.rs——
//! bin 面只做分发，纯面全在 lib 挂 test-host）。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--selftest") {
        // 钉板五步纪律：selftest 面挂 just test-host（cargo test），bin 入口
        // 保持与 Python CLI 同形（a-3 U4 门禁件自身必须有门）。
        eprintln!("elf2flat: --selftest 走 cargo test（tools/elf2flat，tests/selftest.rs）");
        return ExitCode::SUCCESS;
    }
    let mut elf = None;
    let mut out = None;
    let mut page: u64 = 4096;
    let mut max_: u64 = 0;
    let mut base: u64 = 0x1000;
    let mut i = 0;
    loop {
        match args.get(i).map(String::as_str) {
        None => break,
        Some("--page") => {
            i += 1;
            match args.get(i).and_then(|v| parse_int(v)) {
                Some(v) => page = v,
                None => return usage_fail(&format!("--page 需数值（0x/十进制），得 {:?}", args.get(i))),
            }
        }
        Some("--max") => {
            i += 1;
            match args.get(i).and_then(|v| parse_int(v)) {
                Some(v) => max_ = v,
                None => return usage_fail(&format!("--max 需数值（0x/十进制），得 {:?}", args.get(i))),
            }
        }
        Some("--base") => {
            i += 1;
            match args.get(i).and_then(|v| parse_int(v)) {
                Some(v) => base = v,
                None => return usage_fail(&format!("--base 需数值（0x/十进制），得 {:?}", args.get(i))),
            }
        }
        Some(a) if a.starts_with("--") => {
            return usage_fail(&format!("未知参数 {a:?}"))
        }
        Some(a) if elf.is_none() => {
            elf = Some(a.to_string());
        }
        Some(a) if out.is_none() => {
            out = Some(a.to_string());
        }
        Some(a) => return usage_fail(&format!("多余位置参数 {a:?}")),
    }
    i += 1;
    }
    let (Some(elf), Some(out)) = (elf, out) else {
        return usage_fail("用法：elf2flat <in.elf> <out.bin> [--page 4096] [--max N] [--base 0x1000]");
    };
    let data = match std::fs::read(&elf) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("elf2flat: 读 {elf} 失败：{e}");
            return ExitCode::FAILURE;
        }
    };
    match elf2flat::build_flat(&data, base, page, max_) {
        Ok(f) => match std::fs::write(&out, &f.img) {
            Ok(()) => {
                println!(
                    "elf2flat: {out} base={:#x} file={:#x} mem={:#x} total={:#x}",
                    f.base, f.file_len, f.mem_len, f.total
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("elf2flat: 写 {out} 失败：{e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn usage_fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    eprintln!("用法：elf2flat <in.elf> <out.bin> [--page 4096] [--max N] [--base 0x1000]");
    ExitCode::FAILURE
}

/// Python `int(x, 0)` 对齐：0x 前缀十六进制，否则十进制。
fn parse_int(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<u64>().ok()
    }
}
