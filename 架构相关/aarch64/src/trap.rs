//! ARM64 异常向量表（VBAR_EL1）+ 同步异常处理（plan Step 6）。
//!
//! 向量表 2048 字节、16 槽 × 128 字节；4 类（Sync/IRQ/FIQ/SError）× 4 来源
//! （current-EL-SP0 @0x000、current-EL-SPx @0x200、lower-EL-AArch64 @0x400、
//! lower-EL-AArch32 @0x600）。ARMv8 ARM D1.10.2 的精确偏移：**SPx Sync 在 0x200**
//! （非 0x000——那是 SP0）。
//!
//! **Phase 1 单核**：内核跑在 EL1h（`SP_EL1`，见 `boot.rs` 的 `spsr=0x3c5`），故
//! current-EL 异常落到 0x200（sync）/0x280（irq）、handler 用 SP_EL1（内核栈），不搞
//! 专用异常栈（那是 G2+/SMP 细化）。0x200/0x280 各装完整 save/restore；其余 14 槽
//! 路由到 [`exception_unexpected`]（读 ESR/ELR 打印后 halt）。
//!
//! **G0 判据**：异常/panic 必须能 dump 寄存器/PC/ESR/FAR。BRK（EC=0x3C）在本阶段
//! 做成**可恢复**：dump 后 `ELR += 4` 跳过 brk 指令、`eret` 返回——以此一次性验证
//! save→dump→改 ELR→restore→eret 全链路与 TrapFrame/asm 偏移一致（Step 9 SVC 同此路径）。

use core::arch::global_asm;
use crate::{halt, uart_put_hex64, uart_puts};

/// 异常帧（`#[repr(C)]`，与下方 asm 的存取偏移严格对应）。
///
/// 布局：`x0..x30`（31×8=248）+ esr/far/elr/spsr/sp（5×8=40）= **288** 字节（16 对齐）。
#[repr(C)]
pub struct TrapFrame {
    pub regs: [u64; 31],
    pub esr: u64,
    pub far: u64,
    pub elr: u64,
    pub spsr: u64,
    pub sp: u64,
}
// 编译期校 TrapFrame 与 asm 偏移一致（防 Step 6 风险「asm/#[repr(C)] 错位」）。
const _: () = assert!(core::mem::size_of::<TrapFrame>() == 288);
const _: () = assert!(core::mem::offset_of!(TrapFrame, regs) == 0);
const _: () = assert!(core::mem::offset_of!(TrapFrame, esr) == 248);
const _: () = assert!(core::mem::offset_of!(TrapFrame, far) == 256);
const _: () = assert!(core::mem::offset_of!(TrapFrame, elr) == 264);
const _: () = assert!(core::mem::offset_of!(TrapFrame, spsr) == 272);
const _: () = assert!(core::mem::offset_of!(TrapFrame, sp) == 280);

global_asm!(
    ".text",
    ".balign 2048",
    ".globl __vectors",
    ".type __vectors,%function",
    "__vectors:",
    "    .balign 128",
    "    b exc_unexpected",      // 0x000  current EL, SP0, Sync
    "    .balign 128",
    "    b exc_unexpected",      // 0x080  IRQ
    "    .balign 128",
    "    b exc_unexpected",      // 0x100  FIQ
    "    .balign 128",
    "    b exc_unexpected",      // 0x180  SError
    "    .balign 128",
    "    b exc_sync_spx",        // 0x200  current EL, SPx, Sync   ← 真正的处理器
    "    .balign 128",
    "    b exc_irq_spx",         // 0x280  IRQ   ← Step 8 timer
    "    .balign 128",
    "    b exc_unexpected",      // 0x300  FIQ
    "    .balign 128",
    "    b exc_unexpected",      // 0x380  SError
    "    .balign 128",
    "    b exc_unexpected",      // 0x400  lower EL, AArch64, Sync (Phase 2 用户 SVC)
    "    .balign 128",
    "    b exc_unexpected",      // 0x480
    "    .balign 128",
    "    b exc_unexpected",      // 0x500
    "    .balign 128",
    "    b exc_unexpected",      // 0x580
    "    .balign 128",
    "    b exc_unexpected",      // 0x600  lower EL, AArch32
    "    .balign 128",
    "    b exc_unexpected",      // 0x680
    "    .balign 128",
    "    b exc_unexpected",      // 0x700
    "    .balign 128",
    "    b exc_unexpected",      // 0x780
    ".size __vectors, . - __vectors",

    // ---- 存/取异常帧（288 字节）宏 ----
    ".macro save_frame",
    "    sub sp, sp, #288",
    "    stp x0,  x1,  [sp, #0]",
    "    stp x2,  x3,  [sp, #16]",
    "    stp x4,  x5,  [sp, #32]",
    "    stp x6,  x7,  [sp, #48]",
    "    stp x8,  x9,  [sp, #64]",
    "    stp x10, x11, [sp, #80]",
    "    stp x12, x13, [sp, #96]",
    "    stp x14, x15, [sp, #112]",
    "    stp x16, x17, [sp, #128]",
    "    stp x18, x19, [sp, #144]",
    "    stp x20, x21, [sp, #160]",
    "    stp x22, x23, [sp, #176]",
    "    stp x24, x25, [sp, #192]",
    "    stp x26, x27, [sp, #208]",
    "    stp x28, x29, [sp, #224]",
    "    str x30,      [sp, #240]",
    "    mrs x0, esr_el1",
    "    mrs x1, far_el1",
    "    mrs x2, elr_el1",
    "    mrs x3, spsr_el1",
    "    add x4, sp, #288",          // 异常入口 SP（被 sub 之前的值）
    "    stp x0, x1, [sp, #248]",    // esr, far
    "    stp x2, x3, [sp, #264]",    // elr, spsr
    "    str x4,     [sp, #280]",    // sp
    ".endm",

    ".macro load_frame",
    "    ldr x0, [sp, #264]",
    "    msr elr_el1, x0",           // 让 exception_sync 对 ELR 的修改（brk 跳过）生效
    "    ldr x0, [sp, #272]",
    "    msr spsr_el1, x0",
    "    ldp x0,  x1,  [sp, #0]",
    "    ldp x2,  x3,  [sp, #16]",
    "    ldp x4,  x5,  [sp, #32]",
    "    ldp x6,  x7,  [sp, #48]",
    "    ldp x8,  x9,  [sp, #64]",
    "    ldp x10, x11, [sp, #80]",
    "    ldp x12, x13, [sp, #96]",
    "    ldp x14, x15, [sp, #112]",
    "    ldp x16, x17, [sp, #128]",
    "    ldp x18, x19, [sp, #144]",
    "    ldp x20, x21, [sp, #160]",
    "    ldp x22, x23, [sp, #176]",
    "    ldp x24, x25, [sp, #192]",
    "    ldp x26, x27, [sp, #208]",
    "    ldp x28, x29, [sp, #224]",
    "    ldr x30,      [sp, #240]",
    "    add sp, sp, #288",
    ".endm",

    // ---- current-EL SPx 同步异常：存帧 → 调 Rust → 取帧 → eret ----
    "exc_sync_spx:",
    "    save_frame",
    "    mov x0, sp",
    "    bl exception_sync",
    "    load_frame",
    "    eret",

    // ---- current-EL SPx IRQ：同 save/restore；调 exception_irq（ack/EOIR）----
    "exc_irq_spx:",
    "    save_frame",
    "    mov x0, sp",
    "    bl exception_irq",
    "    load_frame",
    "    eret",

    // ---- 其余向量：不应发生。直接调 Rust 读 ESR/ELR 打印 + halt（不存帧）。----
    "exc_unexpected:",
    "    bl exception_unexpected",
    "1:  wfe",
    "    b 1b",
);

/// 安装异常向量表。须在 EL1、任何 DAIF 解蔽**之前**调用。
///
/// # Safety
/// EL1、启动早期；`__vectors` 位于身份映射的内核镜像内。
pub unsafe fn init() {
    extern "C" {
        static __vectors: u8;
    }
    let vbar = core::ptr::addr_of!(__vectors) as u64;
    core::arch::asm!("msr vbar_el1, {0}", in(reg) vbar, options(nostack, preserves_flags));
    core::arch::asm!("isb", options(nostack, preserves_flags));
}

/// current-EL SPx 同步异常入口（asm 调）。
///
/// # Safety（FFI）
/// 由异常向量 asm 以 `x0 = &mut TrapFrame` 调用；C-ABI。
#[no_mangle]
pub extern "C" fn exception_sync(tf: &mut TrapFrame) {
    let ec = ((tf.esr >> 26) & 0x3f) as u32;
    match ec {
        0x15 => {
            // SVC：派发，ret 写回 x0，eret 续行（无 dump）。
            crate::syscall::dispatch(tf);
            return;
        }
        0x3c => {
            dump(tf, ec);
            uart_puts(b"  -> BRK recover: ELR+=4, eret\n");
            tf.elr = tf.elr.wrapping_add(4);
            return;
        }
        _ => {
            dump(tf, ec);
            uart_puts(b"  -> fatal, halt\n");
            halt();
        }
    }
}

/// dump 一个 TrapFrame（EC 名 + 寄存器全集）。
fn dump(tf: &TrapFrame, ec: u32) {
    uart_puts(b"\n=== EXCEPTION: sync (current-EL SPx) ===\n");
    uart_puts(b"  EC=0x");
    uart_put_hex64(ec as u64);
    uart_puts(b" (");
    uart_puts(ec_name(ec));
    uart_puts(b")  ISS=0x");
    uart_put_hex64(tf.esr & 0x1ff_ffff);
    uart_puts(b"\n");
    put_kv(b"  PC  =0x", tf.elr);
    put_kv(b"  FAR=0x", tf.far);
    put_kv(b"  SP =0x", tf.sp);
    put_kv(b"  SPSR=0x", tf.spsr);
    for (i, r) in tf.regs.iter().enumerate() {
        put_reg(i, *r);
    }
}

/// 不应触发的向量（asm 调）。读 ESR/ELR 打印后 halt。
#[no_mangle]
pub extern "C" fn exception_unexpected() {
    let (esr, elr) = unsafe { read_esr_elr() };
    uart_puts(b"\n=== EXCEPTION: unexpected vector ===\n");
    put_kv(b"  ESR=0x", esr);
    put_kv(b"  ELR=0x", elr);
    uart_puts(b"  -> fatal, halt\n");
    halt();
}

/// current-EL SPx IRQ 入口（asm 调）。
///
/// **GIC 协议**：ack（IAR 取 ID）→ 分发 → end（EOIR 同 ID）。spurious（1023）不写 EOIR。
/// CNTV（IRQ 27）电平触发：[`timer::handle`] 内先 rearm（去活电平）再回这里 EOIR。
#[no_mangle]
pub extern "C" fn exception_irq(_tf: &mut TrapFrame) {
    let id = crate::gic::acknowledge();
    if id == crate::gic::SPURIOUS {
        return;
    }
    if id as usize == crate::gic::IRQ_CNTV {
        // rearm（去活电平）在 EOIR 前：电平触发的 CNTV 须先重写 TVAL 再 EOIR，
        // 否则 EOIR 后电平仍高立即重投。dispatch/switch 接在 EOIR 后（Step 5）。
        crate::timer::handle();
    }
    // 未知 IRQ：忽略（G0 单源）。
    crate::gic::end(id);
}

unsafe fn read_esr_elr() -> (u64, u64) {
    let esr: u64;
    let elr: u64;
    core::arch::asm!("mrs {0}, esr_el1", out(reg) esr, options(nostack, preserves_flags));
    core::arch::asm!("mrs {0}, elr_el1", out(reg) elr, options(nostack, preserves_flags));
    (esr, elr)
}

fn ec_name(ec: u32) -> &'static [u8] {
    match ec {
        0x00 => b"Unknown",
        0x0d => b"FP/trap",
        0x15 => b"SVC",
        0x20 => b"I-abort(lower)",
        0x21 => b"I-abort(current)",
        0x22 => b"PC-align",
        0x24 => b"D-abort(lower)",
        0x25 => b"D-abort(current)",
        0x26 => b"SP-align",
        0x2f => b"SError",
        0x3c => b"BRK",
        _ => b"other",
    }
}

fn put_kv(k: &[u8], v: u64) {
    uart_puts(k);
    uart_put_hex64(v);
    uart_puts(b"\n");
}

fn put_reg(i: usize, v: u64) {
    uart_puts(b"  x");
    dec2(i as u8);
    uart_puts(b"=0x");
    uart_put_hex64(v);
    uart_puts(b"\n");
}

/// 两位十进制（寄存器下标 0..30）。
fn dec2(n: u8) {
    const D: &[u8; 10] = b"0123456789";
    let buf = [D[(n / 10) as usize], D[(n % 10) as usize]];
    uart_puts(&buf);
}
