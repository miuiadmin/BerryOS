//! appquiet —— app-0 demo app B（`docs/app-0实施设计草案.md` §4）。
//!
//! manifest `console = false`：**能力缺席可运行**的活证——stdout = appd
//! `channel_pair` 私有对端（输出不进串口），main 静默；appd collect 面
//! 只收 libc exit（EOF 先行）发的零长消息（fail 同款快路径）。
//!
//! argv0 自检 = manifest `entry` 串 `bin/appquiet`（appdemo 同款断言面：
//! 错值 → abort(134) → reap != Ok(0) → `APP ok` 缺失 loud）。42/0 的
//! 退出码分工：fail 演非零源，本双 app 演 0——`APP ok n=` 谓词要全 0。

use std::process::ExitCode;

// 链接种子（stdhello 同款）：无引用则 berry-libc 的 crt0（含 `_start`）
// 不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() -> ExitCode {
    assert_eq!(
        std::env::args().next().as_deref(),
        Some("bin/appquiet"),
        "argv0 should be manifest entry"
    );
    ExitCode::from(0)
}
