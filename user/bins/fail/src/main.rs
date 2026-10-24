//! fail —— P5 `&&` 自验的确定性非零退出程序（`docs/shell设计.md` v0）。
//!
//! 契约：`exit(42)`——std `process::exit` → libc `exit` → **EOF 先行**
//! （`send_stdout_eof`，收集方零长终点约定）→ `berry_process_exit(0x33)`
//! 码 42。父侧（bsh）`reap()` → 0x3A `process_get_exit_code` 收 42 =
//! 真退出码链的运行期验收面（ktest 只盖 -EAGAIN/-EBADF 错误面——Exited
//! 成功路径需 spawn，诚实注记同 F4b）。
//!
//! 刻意无输出：bsh 的 `run fail` collect 面只收一条零长 EOF（快路径，
//! 与 hexyl 的多片收集互为对照）；42 是唯一可断言产物。

use std::process::ExitCode;

// 链接种子（stdhello 同款）：无引用则 berry-libc 的 crt0（含 `_start`）
// 不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() -> ExitCode {
    ExitCode::from(42)
}
