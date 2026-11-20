//! CLI 壳（W1-Q27 五刀·产链换轨 + 六刀·--check + 七刀·--json + 九刀·
//! 签名面）：`mkapp <out.bap> <app-dir>` 打包、`mkapp --check <bap>` 五步
//! 复验、`mkapp --json …`（build/check/new 三 op）机读单行 JSON 信封、
//! `mkapp --sign-key <sk> …` 打包尾签名与 `mkapp --verify-key <pk>
//! --check …` 验签（berry-sign 子进程——发现序与拒因透传 = Python 镜像）。
//! `--selftest` = Python oracle 面保留（selftest_guard 零豁免契约不动；
//! Rust 件有疑时拿 Python 件复算对拍）。出口码同 Python：拒因 1、用法
//! 错 2、成功 0。

use std::process::ExitCode;

use mkapp::gate::Reject;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // --json 恒 argv[1] 前缀（Python main() `args[:1] == ["--json"]` 同判）
    // ——夹带（非 argv[1]）自然落用法错，与「--selftest 不收 --json」同
    // 执法。签名面旗标 + --json 组合 = 用法错（mkapp.py 头注 :102 未定义
    // 面：--json 分派先于旗标提取，混排形到不了签名面——双模式同构）。
    if argv.first().map(|s| s.as_str()) == Some("--json") {
        match mkapp::json::json_main_dispatch(&argv[1..]) {
            Some((text, code)) => {
                println!("{text}");
                return ExitCode::from(code as u8);
            }
            None => {
                eprintln!("mkapp: 用法：mkapp --json <out.bap> <app-dir> | mkapp --json --check <bap> | mkapp --json --new <dir> --id <ID>（其余形态走 python3 tools/mkapp.py——CLI oracle 面保留）");
                return ExitCode::from(2);
            }
        }
    }
    // P1 签名面旗标提取（Python :2556 同镜——--sign-key/--verify-key 可
    // 前缀可夹带；无旗标时 rest == 原 argv，行为与旧版逐字节同——签名
    // 面可选，向后兼容）。
    let mut sign_key: Option<String> = None;
    let mut verify_key: Option<String> = None;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        if a == "--sign-key" || a == "--verify-key" {
            if i + 1 >= argv.len() {
                eprintln!("mkapp: {a} 缺值");
                return ExitCode::from(2);
            }
            if a == "--sign-key" {
                sign_key = Some(argv[i + 1].clone());
            } else {
                verify_key = Some(argv[i + 1].clone());
            }
            i += 2;
            continue;
        }
        rest.push(argv[i].clone());
        i += 1;
    }
    let args = &rest;
    if args.len() == 1 && args[0] == "--selftest" {
        if sign_key.is_some() || verify_key.is_some() {
            eprintln!("mkapp: --selftest 不收签名面旗标");
            return ExitCode::from(2);
        }
        // Python selftest = 纯门禁自检面（零依赖纪律不破——签名面不进
        // selftest）；Rust bin 无 selftest，指回 oracle。
        eprintln!("mkapp: 用法：mkapp <out.bap> <app-dir> | mkapp --check <bap> | mkapp --new <dir> --id <ID> | mkapp --sign-key <sk> <out.bap> <app-dir> | mkapp --verify-key <pk> --check <bap>（--selftest 走 python3 tools/mkapp.py——CLI oracle 面保留）");
        return ExitCode::from(2);
    }
    // --check 恰一参（Python `args == ["--check", x]` 同判）
    if args.len() == 2 && args[0] == "--check" {
        if sign_key.is_some() {
            eprintln!("mkapp: --sign-key 只配打包形态（<out.bap> <app-dir>）");
            return ExitCode::from(2);
        }
        return match mkapp::check::check_bap(&args[1]) {
            Ok(sum) => {
                if let Some(pk) = &verify_key {
                    match mkapp::sign::verify_bap_sig(pk, &args[1]) {
                        Ok(line) => {
                            println!("{}", sum.line(&args[1]));
                            println!("{line}");
                            ExitCode::from(0)
                        }
                        Err(e) => emit_reject(&e),
                    }
                } else {
                    println!("{}", sum.line(&args[1]));
                    ExitCode::from(0)
                }
            }
            Err(e) => emit_reject(&e),
        };
    }
    // --new 恰四参形（Python `len(args)==4 and args[0]=="--new" and
    // args[2]=="--id"` 同判）。
    if args.len() == 4 && args[0] == "--new" && args[2] == "--id" {
        if sign_key.is_some() || verify_key.is_some() {
            eprintln!("mkapp: --new 不收签名面旗标");
            return ExitCode::from(2);
        }
        return match mkapp::new::do_new(&args[1], &args[3]) {
            Ok(sum) => {
                println!("{}", sum.line());
                ExitCode::from(0)
            }
            Err(e) => emit_reject(&e),
        };
    }
    // 产链形态：恰两位、首参非旗标（Python `len(args)==2 and
    // not args[0].startswith("-")` 同判）。
    if args.len() == 2 && !args[0].starts_with('-') {
        if verify_key.is_some() {
            eprintln!("mkapp: --verify-key 只配 --check 形态");
            return ExitCode::from(2);
        }
        return match mkapp::build::build(&args[0], &args[1]) {
            Ok(sum) => {
                println!("{}", sum.line());
                if let Some(sk) = &sign_key {
                    match mkapp::sign::sign_bap(sk, &args[0]) {
                        Ok(line) => {
                            println!("{line}");
                            ExitCode::from(0)
                        }
                        Err(e) => emit_reject(&e),
                    }
                } else {
                    ExitCode::from(0)
                }
            }
            Err(e) => emit_reject(&e),
        };
    }
    eprintln!("mkapp: 用法：mkapp <out.bap> <app-dir> | mkapp --check <bap> | mkapp --new <dir> --id <ID> | mkapp --sign-key <sk> <out.bap> <app-dir> | mkapp --verify-key <pk> --check <bap>（其余形态走 python3 tools/mkapp.py——CLI oracle 面保留）");
    ExitCode::from(2)
}

/// 拒因输出（Python `except Reject` 面：stderr `mkapp: {message}` + 1）。
fn emit_reject(e: &Reject) -> ExitCode {
    eprintln!("mkapp: {}", e.message);
    ExitCode::from(1)
}
