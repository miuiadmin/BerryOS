//! SVC syscall 派发（plan Step 9）。
//!
//! **G0 只证机制**：current-EL sync 解码 `EC==0x15` → 按编号查派发表 → 调平凡
//! handler → 返回值写回 `x0`、`eret` 续行。派发表形状对齐 v1 ABI 编号（`0..=0x85`），
//! 多数槽 unimplemented → [`ERR_NOT_SUPPORTED`]。错误为 C 风格负数、**绝不 panic**。
//! 能力门控 / 真实语义填充留 Phase 3（届时表随语义上语义层、handler 多迁 kernel）。
//!
//! 约定（ADR）：`x0`=号、`x1..x6`=参、`x0`=返回（0 成功 / 负错误）；C-ABI、`#[repr(C)]`。

use crate::trap::TrapFrame;
use crate::{uart_put_hex64, uart_puts};

/// 未实现 syscall 的统一返回（C 风格负数，plan 定 -95）。
pub const ERR_NOT_SUPPORTED: i64 = -95;
/// G0 测试 echo syscall 编号（临时槽；Phase 3 让位真实 ABI 编号）。
pub const NR_TEST_ECHO: usize = 0x61;
/// v1 ABI 编号上界（含）。
const NR_MAX: usize = 0x85;

type Handler = extern "C" fn(&mut TrapFrame) -> i64;

/// 回显第一参 `x1`，证 syscall 往返。
extern "C" fn sys_test_echo(tf: &mut TrapFrame) -> i64 {
    tf.regs[1] as i64
}

/// 未实现槽统一 handler。
extern "C" fn sys_unimplemented(_tf: &mut TrapFrame) -> i64 {
    ERR_NOT_SUPPORTED
}

/// 派发表：编号 → handler（`0..=NR_MAX`）。G0 仅 [`NR_TEST_ECHO`] 接 echo，余 unimplemented。
static SYSCALLS: [Handler; NR_MAX + 1] = {
    let mut t: [Handler; NR_MAX + 1] = [sys_unimplemented as Handler; NR_MAX + 1];
    t[NR_TEST_ECHO] = sys_test_echo as Handler;
    t
};

/// 派发入口（`exception_sync` 在 `EC==0x15` 调）。nr/args 取自 TrapFrame，ret 写回 `x0`。
/// **绝不 panic**：越界 / 未实现都返回负错误。
pub fn dispatch(tf: &mut TrapFrame) -> i64 {
    let nr = tf.regs[0] as usize;
    let ret = if nr <= NR_MAX {
        SYSCALLS[nr](tf)
    } else {
        ERR_NOT_SUPPORTED
    };
    tf.regs[0] = ret as u64;

    // G0 诊断：每次 syscall 打一行（Phase 3 起由 tracing 取代）。
    uart_puts(b"syscall: nr=0x");
    uart_put_hex64(nr as u64);
    uart_puts(b" ret=0x");
    uart_put_hex64(ret as u64);
    uart_puts(b"\n");

    ret
}
