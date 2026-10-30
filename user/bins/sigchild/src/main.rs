//! A7-4 段④：信号终止靶（Cargo.toml 头注——全语义在彼）。
//!
//! 200ms 睡循环：`std::thread::sleep` → libc `nanosleep` →
//! `clock_nanosleep` 链尾投递点（§3.3）——SIGTERM 到达（bsh 侧
//! `berry_signal_send` 置内核 pending 位，到达时刻先于首睡也无竞：位
//! 持久保存在 Process 上，醒来必取走）。DFL 免装 → dispatch Terminate
//! 档 → `io::exit(143)` EOF 先行。零显式信号代码即靶面。

// 链接种子（bsh/stdhello 同款）：fat LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    loop {
        std::thread::sleep(core::time::Duration::from_millis(200));
    }
}
