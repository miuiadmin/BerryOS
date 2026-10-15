//! SVC syscall 派发（plan Step 9；2b-4 起 EL0 用户态同走此表；3-1 起真 ABI 骨架）。
//!
//! **职责（HAL）**：nr 界检 + 派发表 + 统一错误面。约定（ADR #18）：`x0`=号、
//! `x1..x6`=参、`x0`=返回（0 成功 / 负错误）；C-ABI、`#[repr(C)]`。
//! **3-1 政策（撤 2b-4 临时杀政策，对齐 ABI §8.0/§8.4）**：未知/越界/未实现
//! 编号一律返 [`ERR_NOT_SUPPORTED`]、**绝不 panic、绝不杀调用者**——EL0/EL1
//! 同权，错误由用户态自理。真语义 handler 在内核（`内核/src/syscall.rs`）经
//! [`install`] 注册（同 IRQ 槽反转：HAL 不依赖内核 crate）；本表只留调试槽。
//!
//! 编号布局：ABI v1..v3 真号 ≤ `0x85`（`系统调用ABI设计.md` §4）；`0xFE/0xFF`
//! = **调试范围**（非 ABI、不冻结；P4 kernellog 落地后退役）。

use crate::trap::TrapFrame;
use crate::uart_putc;
use core::sync::atomic::{AtomicU64, Ordering};

/// 未实现/未知 syscall 的统一返回（C 风格负数 = -ENOTSUP，ABI §8.2）。
pub const ERR_NOT_SUPPORTED: i64 = -95;
/// 调试 echo 槽（非 ABI；ktest EL1 自检往返）。原 0x61 临时槽 3-1 让位 `clock_read`。
pub const NR_DEBUG_ECHO: usize = 0xFE;
/// 调试 putchar 槽（非 ABI；EL0 金丝雀 'u'/'S'——用户页表无 MMIO 映射，输出须经
/// syscall「内核代劳」）。原 0x62 临时槽 3-1 让位 `timer_create`。P4 退役。
pub const NR_DEBUG_PUTCHAR: usize = 0xFF;
/// 派发表上界（含）：真 ABI 号 ≤0x85，0x86..0xFD 预留，0xFE/0xFF 调试。
const NR_MAX: usize = 0xFF;

/// EL0 putchar 计数（G1.5 watchdog 金丝雀：好用户活着的证据——仍在打 'u'）。
static USER_PUTCNT: AtomicU64 = AtomicU64::new(0);

/// 读 putchar 计数（watchdog 用）。
pub fn user_putcnt() -> u64 {
    USER_PUTCNT.load(Ordering::Relaxed)
}

type Handler = extern "C" fn(&mut TrapFrame) -> i64;

/// 调试 echo：回显第一参 `x1`，证 syscall 往返。
extern "C" fn sys_debug_echo(tf: &mut TrapFrame) -> i64 {
    tf.regs[1] as i64
}

/// 调试 putchar：打 `x1` 低 8 位到 UART，返回 0。
extern "C" fn sys_debug_putchar(tf: &mut TrapFrame) -> i64 {
    uart_putc(tf.regs[1] as u8);
    USER_PUTCNT.fetch_add(1, Ordering::Relaxed);
    0
}

/// 未实现槽统一 handler。
extern "C" fn sys_unimplemented(_tf: &mut TrapFrame) -> i64 {
    ERR_NOT_SUPPORTED
}

/// 派发表：编号 → handler（`0..=NR_MAX`；调试槽常量初始化，真语义槽由内核
/// boot 期 [`install`] 填）。
static mut SYSCALLS: [Handler; NR_MAX + 1] = {
    let mut t: [Handler; NR_MAX + 1] = [sys_unimplemented as Handler; NR_MAX + 1];
    t[NR_DEBUG_ECHO] = sys_debug_echo as Handler;
    t[NR_DEBUG_PUTCHAR] = sys_debug_putchar as Handler;
    t
};

/// 注册语义 handler（内核 `rust_main` 在 `enable_irq` 前调用；v1 真号见
/// `内核/src/syscall.rs` 的 NR_* 常量组）。
///
/// # Panics
/// nr 越界（> [`NR_MAX`]）= 编码错误，boot 期 panic 停机优于带病前进。
pub fn install(nr: usize, h: Handler) {
    assert!(nr <= NR_MAX, "syscall install nr 越界: {nr:#x}");
    // SAFETY: boot 单线程（IRQ 未开）注册，此后全系统只读；单核无并发写。
    unsafe { (*(&raw mut SYSCALLS))[nr] = h };
}

/// 派发入口（`exception_sync`/`exception_sync_el0` 在 `EC==0x15` 调）。nr/args 取自
/// TrapFrame，ret 写回 `x0`（其余寄存器不动）。**绝不 panic、绝不杀调用者**：
/// 未知 / 未实现一律 [`ERR_NOT_SUPPORTED`]。
pub fn dispatch(tf: &mut TrapFrame) -> i64 {
    let nr = tf.regs[0] as usize;
    let table = &raw const SYSCALLS;
    let ret = if nr <= NR_MAX {
        // SAFETY: 只读快照单槽（fn 指针 Copy）；install 全在 boot 单线程期。
        unsafe { (*table)[nr](tf) }
    } else {
        ERR_NOT_SUPPORTED
    };
    tf.regs[0] = ret as u64;
    ret
}
