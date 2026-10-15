//! SVC syscall 派发（plan Step 9；2b-4 起 EL0 用户态同走此表；3-1 起真 ABI 骨架）。
//!
//! **职责（HAL）**：nr 界检 + 派发表 + 统一错误面。约定（ADR #18）：`x0`=号、
//! `x1..x6`=参、`x0`=返回（0 成功 / 负错误）；C-ABI、`#[repr(C)]`。
//! **3-1 政策（撤 2b-4 临时杀政策，对齐 ABI §8.0/§8.4）**：未知/越界/未实现
//! 编号一律返 [`ERR_NOT_SUPPORTED`]、**绝不 panic、绝不杀调用者**——EL0/EL1
//! 同权，错误由用户态自理。真语义 handler 在内核（`内核/src/syscall.rs`）经
//! [`install`] 注册（同 IRQ 槽反转：HAL 不依赖内核 crate）。
//!
//! 编号布局：ABI v1..v3 真号 ≤ `0x85`（`系统调用ABI设计.md` §4）。原调试槽
//! `0xFE/0xFF`（echo/putchar，非 ABI）**4-5a 退役**——EL0 输出改走 console
//! 服务通道（`channel_send` 真 IPC），两槽回落 [`sys_unimplemented`] 的
//! -ENOTSUP（ktest 退役面断言在册）。

use crate::trap::TrapFrame;

/// 未实现/未知 syscall 的统一返回（C 风格负数 = -ENOTSUP，ABI §8.2）。
pub const ERR_NOT_SUPPORTED: i64 = -95;
/// 派发表上界（含）：真 ABI 号 ≤0x85，0x86..0xFF 预留（0xFE/0xFF 调试槽已退役）。
const NR_MAX: usize = 0xFF;

type Handler = extern "C" fn(&mut TrapFrame) -> i64;

/// 未实现槽统一 handler。
extern "C" fn sys_unimplemented(_tf: &mut TrapFrame) -> i64 {
    ERR_NOT_SUPPORTED
}

/// 派发表：编号 → handler（`0..=NR_MAX`；全 [`sys_unimplemented`] 初始化，真
/// 语义槽由内核 boot 期 [`install`] 填）。
static mut SYSCALLS: [Handler; NR_MAX + 1] = [sys_unimplemented as Handler; NR_MAX + 1];

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
