//! CLI 壳（W1-Q27 五刀·产链换轨 + 六刀·--check）：`mkapp <out.bap>
//! <app-dir>` 打包与 `mkapp --check <bap>` 五步复验实装。`--new`/`--json`/
//! `--sign-key`/`--verify-key`/`--selftest` = Python oracle 面保留（
//! selftest_guard 零豁免契约不动；Rust 件有疑时拿 Python 件复算对拍）。
//! 出口码同 Python：拒因 1、用法错 2、成功 0。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
    eprintln!("mkapp: 用法：mkapp <out.bap> <app-dir> | mkapp --check <bap>（其余形态走 python3 tools/mkapp.py——CLI oracle 面保留）");
    ExitCode::from(2)
}
