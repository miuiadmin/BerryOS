//! SVC syscall 派发（plan Step 9；2b-4 起 EL0 用户态同走此表）。
//!
//! **G0 只证机制**：sync 解码 `EC==0x15` → 按编号查派发表 → 调平凡
//! handler → 返回值写回 `x0`、`eret` 续行。派发表形状对齐 v1 ABI 编号（`0..=0x85`），
//! 多数槽 unimplemented → [`ERR_NOT_SUPPORTED`]。错误为 C 风格负数、**绝不 panic**。
//! 能力门控 / 真实语义填充留 Phase 3（届时表随语义上语义层、handler 多迁 kernel）。
//!
//! 约定（ADR）：`x0`=号、`x1..x6`=参、`x0`=返回（0 成功 / 负错误）；C-ABI、`#[repr(C)]`。
//! **v1 政策（2b-4）**：EL0 调越界/未实现编号 = 畸形 syscall，`exception_sync_el0`
//! 见 `ERR_NOT_SUPPORTED` 即杀当前任务（G1.5）；EL1（ktest）仍只回错误码。

use crate::trap::TrapFrame;
use crate::uart_putc;
use core::sync::atomic::{AtomicU64, Ordering};

/// 未实现 syscall 的统一返回（C 风格负数，plan 定 -95）。
pub const ERR_NOT_SUPPORTED: i64 = -95;
/// G0 测试 echo syscall 编号（临时槽；Phase 3 让位真实 ABI 编号）。
pub const NR_TEST_ECHO: usize = 0x61;
/// EL0 用户 putchar 编号（临时槽；Phase 3 让位真实 ABI 编号）。内核代打 UART——
/// 用户页表无 MMIO 映射，直访不可行，须经 syscall（「内核代劳特权操作」最小样本）。
pub const NR_USER_PUTCHAR: usize = 0x62;
/// v1 ABI 编号上界（含）。
const NR_MAX: usize = 0x85;

/// EL0 putchar 计数（G1.5 watchdog 金丝雀：好用户活着的证据——仍在打 'u'）。
static USER_PUTCNT: AtomicU64 = AtomicU64::new(0);

/// 读 putchar 计数（watchdog 用）。
pub fn user_putcnt() -> u64 {
    USER_PUTCNT.load(Ordering::Relaxed)
}

type Handler = extern "C" fn(&mut TrapFrame) -> i64;

/// 回显第一参 `x1`，证 syscall 往返。
extern "C" fn sys_test_echo(tf: &mut TrapFrame) -> i64 {
    tf.regs[1] as i64
}

/// EL0 putchar：打 `x1` 低 8 位到 UART，返回 0。
extern "C" fn sys_user_putchar(tf: &mut TrapFrame) -> i64 {
    uart_putc(tf.regs[1] as u8);
    USER_PUTCNT.fetch_add(1, Ordering::Relaxed);
    0
}

/// 未实现槽统一 handler。
extern "C" fn sys_unimplemented(_tf: &mut TrapFrame) -> i64 {
    ERR_NOT_SUPPORTED
}

/// 派发表：编号 → handler（`0..=NR_MAX`）。
static SYSCALLS: [Handler; NR_MAX + 1] = {
    let mut t: [Handler; NR_MAX + 1] = [sys_unimplemented as Handler; NR_MAX + 1];
    t[NR_TEST_ECHO] = sys_test_echo as Handler;
    t[NR_USER_PUTCHAR] = sys_user_putchar as Handler;
    t
};

/// 派发入口（`exception_sync`/`exception_sync_el0` 在 `EC==0x15` 调）。nr/args 取自
/// TrapFrame，ret 写回 `x0`。**绝不 panic**：越界 / 未实现都返回负错误。
pub fn dispatch(tf: &mut TrapFrame) -> i64 {
    let nr = tf.regs[0] as usize;
    let ret = if nr <= NR_MAX {
        SYSCALLS[nr](tf)
    } else {
        ERR_NOT_SUPPORTED
    };
    tf.regs[0] = ret as u64;
    ret
}
