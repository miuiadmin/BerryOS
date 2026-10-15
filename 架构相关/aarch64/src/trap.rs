//! ARM64 异常向量表（VBAR_EL1）+ 同步异常处理（plan Step 6）。
//!
//! 向量表 2048 字节、16 槽 × 128 字节；4 类（Sync/IRQ/FIQ/SError）× 4 来源
//! （current-EL-SP0 @0x000、current-EL-SPx @0x200、lower-EL-AArch64 @0x400、
//! lower-EL-AArch32 @0x600）。ARMv8 ARM D1.10.2 的精确偏移：**SPx Sync 在 0x200**
//! （非 0x000——那是 SP0）。
//!
//! **Phase 1 单核**：内核跑在 EL1h（`SP_EL1`，见 `boot.rs` 的 `spsr=0x3c5`），故
//! current-EL 异常落到 0x200（sync）/0x280（irq）、handler 用 SP_EL1（内核栈），不搞
//! 专用异常栈（那是 G2+/SMP 细化）。0x200/0x280 各装完整 save/restore；0x400/0x480
//! 自 2b-4 起装 lower-EL（EL0 用户态）sync/IRQ——帧宏 `*_el0` 差异仅在 `sp` 槽
//! 存取 **SP_EL0**（用户 SP）；其余 12 槽路由到 [`exception_unexpected`]（读 ESR/ELR
//! 打印后 halt）。
//!
//! **G0 判据**：异常/panic 必须能 dump 寄存器/PC/ESR/FAR。BRK（EC=0x3C）在本阶段
//! 做成**可恢复**：dump 后 `ELR += 4` 跳过 brk 指令、`eret` 返回——以此一次性验证
//! save→dump→改 ELR→restore→eret 全链路与 TrapFrame/asm 偏移一致（Step 9 SVC 同此路径）。

use core::arch::global_asm;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::{halt, uart_put_hex64, uart_puts};

/// 期望故障槽（2b-2 低半退役正向断言）：非 0 = 期望下一次 current-EL **D-abort**
/// 的 FAR。命中 → 恢复（ELR+=4，4 字节 ldr/str 假设）并清槽；未 arm 或 FAR 不符 →
/// 原 fatal 路径。调用方读回 [`fault_armed`] 判「确实 fault 了」（槽被消费 = 异常真
/// 发生过；没 fault 反而是断言失败——映射没退役干净）。
static EXPECTED_FAULT_FAR: AtomicUsize = AtomicUsize::new(0);

/// 武装期望故障（ktest 专用；单核、IRQ 屏蔽下 arm→触发之间无他人插队）。
pub fn arm_expected_fault(far: usize) {
    EXPECTED_FAULT_FAR.store(far, Ordering::Release);
}

/// 期望故障槽是否仍处于武装态（未武装 = 已被消费 = 异常确实发生过）。
pub fn fault_armed() -> bool {
    EXPECTED_FAULT_FAR.load(Ordering::Acquire) != 0
}

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
    "    b exc_sync_el0",       // 0x400  lower EL, AArch64, Sync  ← 2b-4 用户 SVC/fault
    "    .balign 128",
    "    b exc_irq_el0",        // 0x480  lower EL, AArch64, IRQ  ← timer 抢占用户态
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

    // ---- lower-EL（EL0 用户态）帧宏：sp 槽语义不同 ----
    // 异常自 EL0 入 EL1 时硬件强制 SPsel=1 → SP=SP_EL1（eret 进 EL0 前留在那的
    // 本任务内核栈），栈帧仍落在内核栈上；但帧的 `sp` 字段应存**用户 SP**（SP_EL0，
    // 唯一记录处——恢复时写回）。其余存取与 save/load_frame 逐字节同构。
    ".macro save_frame_el0",
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
    "    mrs x4, sp_el0",           // 用户 SP（非异常入口内核 SP）
    "    stp x0, x1, [sp, #248]",   // esr, far
    "    stp x2, x3, [sp, #264]",   // elr, spsr
    "    str x4,     [sp, #280]",   // sp = 用户 SP
    ".endm",

    ".macro load_frame_el0",
    "    ldr x0, [sp, #264]",
    "    msr elr_el1, x0",
    "    ldr x0, [sp, #272]",
    "    msr spsr_el1, x0",
    "    ldr x0, [sp, #280]",
    "    msr sp_el0, x0",           // 恢复用户 SP
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

    // ---- lower-EL（EL0）同步异常：SVC 派发 / 故障 → 杀当前任务（G1.5）----
    // handler 可能经 kill 槽 switch 走不归（帧所在栈被弃）——与 IRQ 路径 switch
    // 同一复合规则：switch 是纯叶子 asm。
    "exc_sync_el0:",
    "    save_frame_el0",
    "    mov x0, sp",
    "    bl exception_sync_el0",
    "    load_frame_el0",
    "    eret",

    // ---- lower-EL（EL0）IRQ：GIC 协议复用 exception_irq（含抢占 switch）----
    "exc_irq_el0:",
    "    save_frame_el0",
    "    mov x0, sp",
    "    bl exception_irq",
    "    load_frame_el0",
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
/// EL1、启动早期（MMU 已开、TTBR1 高半表就绪）；`__vectors` 链在高半 `.text`
/// （2b-1 起镜像布局，VMA = `KERNEL_VA_BASE` + LMA，经 TTBR1 内核表直映翻译——
/// 低半身份映射是已退役体制，向量表寻址与其无关）。
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
        0x25 => {
            // D-abort(current-EL)：先查期望故障槽（2b-2 低半退役正向断言）——
            // arm 且 FAR 命中 → 恢复（证「该地址确实无映射」）；否则原 fatal 路径。
            let want = EXPECTED_FAULT_FAR.swap(0, Ordering::AcqRel);
            if want != 0 && tf.far as usize == want {
                uart_puts(b"  -> expected D-abort, recover: ELR+=4\n");
                tf.elr = tf.elr.wrapping_add(4); // 假设 4 字节 ldr/str（ktest 用 ldr x0,[x1]）
                return;
            }
            dump(tf, ec);
            uart_puts(b"  -> fatal, halt\n");
            halt();
        }
        _ => {
            dump(tf, ec);
            uart_puts(b"  -> fatal, halt\n");
            halt();
        }
    }
}

/// lower-EL（EL0 用户态）同步异常入口（asm 调；2b-4 首个 EL0 + G1.5）。
///
/// SVC → 派发（ret 写回 x0、eret 续行于 svc 下一句）——**3-1 政策**（撤 2b-4
/// 临时杀政策，对齐 ABI §8.0/§8.4「永不 panic」）：未知/未实现编号返
/// [`ERR_NOT_SUPPORTED`](crate::syscall::ERR_NOT_SUPPORTED)，**一律存活**、错误
/// 由用户态自理；EL0/EL1 同权。其余一切 EC（D-abort 0x24 野指针 / I-abort
/// 0x20 / BRK 0x3C / 未知）→ dump 后杀当前任务——经 [`crate::user_fault_exit`]
/// 函数指针槽（HAL 不依赖 sched，同 IRQ 槽反转）。
/// 期望故障槽（ktest 专用 current-EL 通道）不参与：用户态故障一律真死。
///
/// # Safety（FFI）
/// 由异常向量 asm 以 `x0 = &mut TrapFrame` 调用；C-ABI。
#[no_mangle]
pub extern "C" fn exception_sync_el0(tf: &mut TrapFrame) {
    let ec = ((tf.esr >> 26) & 0x3f) as u32;
    match ec {
        0x15 => {
            // 未知/未实现号 → -ENOTSUP 存活（3-1 政策；滥用面如坏 handle → -EBADF）。
            crate::syscall::dispatch(tf);
            return; // 一切 SVC 正常往返（含错误码）
        }
        _ => {
            dump_el0(tf, ec);
            uart_puts(b"  -> user fault (G1.5: kill current task)\n");
        }
    }
    // 杀当前任务：切走永不回（被弃的是本故障帧所在的栈）。槽未注册（调度器没起，
    // 理论不可达——main 在 spawn 用户任务前注册）则 halt 兜底。
    // SAFETY: 槽由内核 boot 时注册；同步异常上下文（PSTATE.I=1）。
    if let Some(f) = unsafe { crate::user_fault_exit() } {
        // SAFETY: f 是 C-ABI `-> !` fn 指针。
        unsafe { f() }
    }
    halt();
}

/// dump 一个 TrapFrame（EC 名 + 寄存器全集）。
fn dump(tf: &TrapFrame, ec: u32) {
    dump_hdr(tf, ec, b"=== EXCEPTION: sync (current-EL SPx) ===");
}

/// 同 [`dump`]，lower-EL（用户态）帧抬头。
fn dump_el0(tf: &TrapFrame, ec: u32) {
    dump_hdr(tf, ec, b"=== EXCEPTION: sync (from EL0, user) ===");
}

fn dump_hdr(tf: &TrapFrame, ec: u32, hdr: &[u8]) {
    uart_puts(b"\n");
    uart_puts(hdr);
    uart_puts(b"\n");
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
///
/// **顺序（plan Bug 1）**：`ack → rearm(timer::handle) → EOIR → dispatch`。dispatch/switch
/// 必须在 EOIR **之后**——若 switch 跑在 ack 与 EOIR 之间，IRQ 在 GIC 里保持 active（优先级
/// 未降），电平触发的 CNTV 下次发不出来（EOIR 饿死）。dispatch 经 [`crate::irq_dispatch`]
/// 函数指针槽调调度器（HAL 不依赖 sched）。Step 5 dispatch 只打 tick + 已由 handle rearm，
/// 不 switch；Step 6 才在槽里调真 `tick` 抢占。
#[no_mangle]
pub extern "C" fn exception_irq(tf: &mut TrapFrame) {
    let id = crate::gic::acknowledge();
    if id == crate::gic::SPURIOUS {
        return;
    }
    if id as usize == crate::gic::IRQ_CNTV {
        // rearm（去活电平）在 EOIR 前：电平触发的 CNTV 须先重编程 CVAL（绝对截止
        // ——读 CNTVCT 现值 + SLICE，见 timer.rs）再 EOIR，否则 EOIR 后电平仍高立即重投。
        crate::timer::handle();
    }
    // 未知 IRQ：忽略（G0 单源）。
    crate::gic::end(id);
    // dispatch 在 EOIR 后（Bug 1）：经函数指针槽通知调度器。Step 5 只打 tick，不 switch。
    // SAFETY: IRQ 上下文（PSTATE.I=1）；槽由内核 boot 时注册（enable_irq 前）。
    if let Some(f) = unsafe { crate::irq_dispatch() } {
        // SAFETY: f 是 C-ABI fn 指针；tf 是当前 IRQ 栈上的异常帧。
        unsafe { f(tf) };
    }
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
        0x07 => b"FP/SIMD-trap",
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
