//! {app_id} —— BerryOS 应用（SDK-1a 脚手架；契约见 docs/应用开发指南.md）。
use std::process::ExitCode;

// 链接种子（stdhello/appdemo 同款）：无引用则 berry-libc 的 crt0（含
// `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() -> ExitCode {{
    // argv0 契约：spawn name = manifest entry（app-0 §5）。
    assert_eq!(
        std::env::args().next().as_deref(),
        Some("bin/{slug}"),
        "argv0 should be manifest entry"
    );
    println!("app {app_id} up");
    ExitCode::from(0)
}}
