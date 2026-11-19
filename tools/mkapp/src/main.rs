//! CLI 壳（W1-Q27 五刀·产链换轨）：`mkapp <out.bap> <app-dir>` 打包形态
//! 实装——产链现役件（justfile 四处调用位）。`--check`/`--new`/`--json`/
//! `--sign-key`/`--verify-key`/`--selftest` = Python oracle 面保留（
//! selftest_guard 零豁免契约不动；Rust 件有疑时拿 Python 件复算对拍）。
//! 出口码同 Python：拒因 1、用法错 2、成功 0。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // 产链形态唯一：恰两位、首参非旗标（Python main() `len(args)==2 and
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
    eprintln!("mkapp: 用法：mkapp <out.bap> <app-dir>（其余形态走 python3 tools/mkapp.py——CLI oracle 面保留）");
    ExitCode::from(2)
}
