//! CLI 壳——占位分发（首刀只有 lib 纯面，CLI 参数面后续刀随
//! check_manifest_text 门禁一起落）。mkimage 首刀同款形态：bin 面先立
//! 用法/出口码骨架，selftest 走 cargo test（tools/mkapp）。

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("mkapp: 首刀仅 lib 纯面（canonical 规范形重发射）——CLI 参数面随门禁刀落地");
    eprintln!("用法：mkapp <out.bap> <app-dir> [--json]（未实装；自检走 cargo test --manifest-path tools/mkapp/Cargo.toml）");
    ExitCode::from(2)
}
