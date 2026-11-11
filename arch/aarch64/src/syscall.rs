//! SVC syscall 派发（plan Step 9；2b-4 起 EL0 用户态同走此表；3-1 起真 ABI 骨架）。
//!
//! **职责（HAL）**：nr 界检 + 派发表 + 统一错误面。约定（ADR #18）：`x0`=号、
//! `x1..x6`=参、`x0`=返回（0 成功 / 负错误）；C-ABI、`#[repr(C)]`。
//! **3-1 政策（撤 2b-4 临时杀政策，对齐 ABI §8.0/§8.4）**：未知/越界/未实现
//! 编号一律返 [`ERR_NOT_SUPPORTED`]、**绝不 panic、绝不杀调用者**——EL0/EL1
//! 同权，错误由用户态自理。真语义 handler 在内核（`kernel/src/syscall.rs`）经
//! [`install`] 注册（同 IRQ 槽反转：HAL 不依赖内核 crate）。
//!
//! 编号布局：ABI v1..v3 真号 ≤ `0x87`（`系统调用ABI设计.md` §4）。原调试槽
//! `0xFE/0xFF`（echo/putchar，非 ABI）**4-5a 退役**——EL0 输出改走 console
//! 服务通道（`channel_send` 真 IPC），两槽回落 [`sys_unimplemented`] 的
//! -ENOTSUP（ktest 退役面断言在册）。

use crate::trap::TrapFrame;

/// 未实现/未知 syscall 的统一返回（C 风格负数 = -ENOTSUP，ABI §8.2）。
pub const ERR_NOT_SUPPORTED: i64 = -95;
/// 派发表上界（含）：真 ABI 号 ≤0x87（0x86/0x87 信号面在册——A7-4），
/// 0x88..0xFF 预留（0xFE/0xFF 调试槽已退役）。
const NR_MAX: usize = 0xFF;

type Handler = extern "C" fn(&mut TrapFrame) -> i64;

/// 未实现槽统一 handler。
extern "C" fn sys_unimplemented(_tf: &mut TrapFrame) -> i64 {
    ERR_NOT_SUPPORTED
}

/// 派发表：编号 → (handler, 入口锁类)（`0..=NR_MAX`；全
/// (`sys_unimplemented`, NONE) 初始化，真语义槽由内核 boot 期 [`install`] 填）。
/// 类位集语义 → [`crate::kern_lock::svc_enter_class`]（slice 2b：NONE 号零锁
/// 直达；默认槽 NONE——未知/未实现号不取任何锁）。
static mut SYSCALLS: [(Handler, u8); NR_MAX + 1] =
    [(sys_unimplemented as Handler, crate::kern_lock::SVC_CLASS_NONE); NR_MAX + 1];

/// 注册语义 handler + 入口锁类（内核 `rust_main` 在 `enable_irq` 前调用；v1
/// 真号见 `kernel/src/syscall.rs` 的 NR_* 常量组；类常量 = `kern_lock::SVC_CLASS_*`）。
///
/// # Panics
/// nr 越界（> [`NR_MAX`]）= 编码错误，boot 期 panic 停机优于带病前进。
pub fn install(nr: usize, h: Handler, class: u8) {
    assert!(nr <= NR_MAX, "syscall install nr 越界: {nr:#x}");
    // SAFETY: boot 单线程（IRQ 未开）注册，此后全系统只读；单核无并发写。
    unsafe { (*(&raw mut SYSCALLS))[nr] = (h, class) };
}

/// 派发入口（`exception_sync`/`exception_sync_el0` 在 `EC==0x15` 调）。nr/args 取自
/// TrapFrame，ret 写回 `x0`（其余寄存器不动）。**绝不 panic、绝不杀调用者**：
/// 未知 / 未实现一律 [`ERR_NOT_SUPPORTED`]。
pub fn dispatch(tf: &mut TrapFrame) -> i64 {
    let nr = tf.regs[0] as usize;
    // S-3c c-2c slice 2b：SVC 入口按类分档（历史 = 全号无差别 svc_enter/exit
    // 大锁对）——类位集含 TABLE 位才取 KL（过渡态 KL-first，全局序 KL 最外
    // 不变），结构位集经 SVC_WINDOW 槽交 sched 预取；NONE 类零锁直达。
    // svc_exit_class 置于 x0 写回前（写回是 TrapFrame 栈写，不触共享结构）。
    // `-> !` handler（0x33/0x36）不归——放锁在 sched.rs switch_away_forever_with
    // 按位收口，本尾对其不可达。
    let (h, class) = if nr <= NR_MAX {
        // SAFETY: 只读快照单槽（元组 Copy）；install 全在 boot 单线程期。
        unsafe { *(&raw const SYSCALLS).cast::<(Handler, u8)>().add(nr) }
    } else {
        // 越界号：NONE 类零锁形态下无需入出口对，直接错误返回。
        (sys_unimplemented as Handler, crate::kern_lock::SVC_CLASS_NONE)
    };
    crate::kern_lock::svc_enter_class(class);
    let ret = h(tf);
    crate::kern_lock::svc_exit_class(class);
    // S-4 §3.2/§2 4-1②：SGI #2 回执的**包络外等待点**——此处 KL+结构锁已全
    // 放（`svc_exit_class` 毕）= 零锁持有；自旋读对核回执计数（他核 IRQ
    // handler 置位，不依赖本核 IRQ 开）。NONE 类号多付一次 Acquire 原子读
    //（单核闩恒 0 直返）。
    crate::tlbshoot::sched_exit_check();
    // S-4 4-3：ASID 回卷 quiesce 驱动（闩驱动——闩恒 false 时一次 Acquire
    // 读直返，单核闩恒不置）。零锁纪律同上（§3.2）。
    crate::tlbshoot::roll_quiesce_check();
    // G3-1b 落点①：syscall 返回边界检查点（草案 §8.C——svc_exit_class 毕 =
    // KL+结构锁全放；x0 写回是 TrapFrame 栈写不触共享结构，检查点在其前不
    // 违约）。`-> !` handler（0x33/0x36）不归，本尾对其不可达——不设检查点。
    // 槽空（注册前/未注册）= 平凡跳过。槽 static 在 lib.rs 根（跨 crate 槽
    // 惯例位）。
    // SAFETY: 只读快照函数指针；set_svc_exit_checkpoint 全在 boot 单线程期。
    if let Some(checkpoint) = unsafe { crate::SVC_EXIT_CHECKPOINT } {
        checkpoint();
    }
    tf.regs[0] = ret as u64;
    ret
}
