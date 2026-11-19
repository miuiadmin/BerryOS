//! CLI 壳（W1-Q27 五刀·产链换轨 + 六刀·--check + 七刀·--json）：
//! `mkapp <out.bap> <app-dir>` 打包、`mkapp --check <bap>` 五步复验与
//! `mkapp --json <out.bap> <app-dir>` | `mkapp --json --check <bap>` 机读
//! 单行 JSON 信封（SDK-2 §二；build/check 两 op——拒因舱 7 键 + result
//! 舱摘要行镜像）实装。`--new`/`--sign-key`/`--verify-key`/`--selftest` =
//! Python oracle 面保留（selftest_guard 零豁免契约不动；Rust 件有疑时
//! 拿 Python 件复算对拍）。出口码同 Python：拒因 1、用法错 2、成功 0。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // --json 恒 argv[1] 前缀（Python main() `args[:1] == ["--json"]` 同判）
    // ——夹带（非 argv[1]）自然落用法错，与「--selftest 不收 --json」同执法。
    if args.first().map(|s| s.as_str()) == Some("--json") {
        match mkapp::json::json_main_dispatch(&args[1..]) {
            Some((text, code)) => {
                println!("{text}");
                return ExitCode::from(code as u8);
            }
            None => {
                eprintln!("mkapp: 用法：mkapp --json <out.bap> <app-dir> | mkapp --json --check <bap>（其余形态走 python3 tools/mkapp.py——CLI oracle 面保留）");
                return ExitCode::from(2);
            }
        }
    }
    // --check 恰一参（Python main() `args == ["--check", x]` 同判）
    if args.len() == 2 && args[0] == "--check" {
        return match mkapp::check::check_bap(&args[1]) {
            Ok(sum) => {
                println!("{}", sum.line(&args[1]));
                ExitCode::from(0)
            }
            Err(e) => {
                eprintln!("mkapp: {}", e.message);
                ExitCode::from(1)
            }
        };
    }
    // 产链形态：恰两位、首参非旗标（Python main() `len(args)==2 and
    // not args[0].startswith("-")` 同判）。
    if args.len() == 2 && !args[0].starts_with('-') {
        return match mkapp::build::build(&args[0], &args[1]) {
            Ok(sum) => {
                println!("{}", sum.line());
                ExitCode::from(0)
            }
            Err(e) => {
                eprintln!("mkapp: {}", e.message);
                ExitCode::from(1)
            }
        };
    }
    // --new 恰四参形（Python main() `len(args)==4 and args[0]=="--new"
    // and args[2]=="--id"` 同判；签名面旗标混排 = 用法错走兜底——Rust 件
    // 无签名面，混排天然落此处拒）。
    if args.len() == 4 && args[0] == "--new" && args[2] == "--id" {
        return match mkapp::new::do_new(&args[1], &args[3]) {
            Ok(sum) => {
                println!("{}", sum.line());
                ExitCode::from(0)
            }
            Err(e) => {
                eprintln!("mkapp: {}", e.message);
                ExitCode::from(1)
            }
        };
    }
    eprintln!("mkapp: 用法：mkapp <out.bap> <app-dir> | mkapp --check <bap> | mkapp --new <dir> --id <ID>（其余形态走 python3 tools/mkapp.py——CLI oracle 面保留）");
    ExitCode::from(2)
}
