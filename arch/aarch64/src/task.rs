//! ARM64 任务上下文 + 上下文切换原语（plan Phase 2a Step 1）。
//!
//! 与异常帧 [`crate::trap::TrapFrame`]（异常入口保存的完整 x0..x30 + ESR/ELR/...）
//! 不同，本 [`TaskContext`] 存**协作者切换所需的 callee-saved**（x19..x30 + sp）+
//! **FPSIMD 全量**（q0..q31 + FPCR/FPSR，GUI-0 起）——因为切换发生在已知的抢占
//! 点（`yield` / IRQ 出口），其余整数寄存器由调用约定或 `TrapFrame` /
//! `exception_irq` 的 save/load 管；FP 面则靠 **内核 SIMD-free 合同**守住「异常/
//! 系统调用路径不碰 q 寄存器」——故 TrapFrame 不存 FP，全量保存在 switch 点即
//! 完备。合同三层机制（GUI-0 实抓 compiler-builtins NEON memset + 本仓 O1 块拷贝
//! 落 q 寄存器后立）：① `.cargo/config.toml` 目标限域 `-C target-feature=
//! -fp-armv8,-neon`（本仓 crate 全 GPR codegen）；② `kernel/src/mem.rs` 整数版
//! mem* + bcmp 覆写（挤掉 builtins 版——「哪些件今天恰好是 NEON」不进合同）；
//! ③ ktest FP canary 每次开机执法（copy/zero/比较路径链到 SIMD 即 boot 期
//! loud）。core fmt 链仍含 NEON 但仅 panic(=halt) 路径可达。
//! 注：AAPCS 下 v8..v15 也是 callee-saved，但 `switch` 是裸 asm 跳转不是调用
//! 边界——调用约定不约束它；即便按调用约定口径看，q0..q31 全量保存也已把
//! v8..v15 子集一并覆盖，无需分层。
//!
//! `switch` 是**纯叶子 asm stub**（无 prologue、不压自己的栈帧、只 `mov sp`）。
//! 这是从 IRQ handler 中途切换、之后 `eret` 重定向到新任务的关键不变式（见 plan
//! 「核心正确性 trace」：`TaskContext` 里存的 SP == `load_frame` 时所需的 SP）。
//! 承载不变式：**`switch` 永不在 `PSTATE.I=0` 时到达**（IRQ 路径硬件屏蔽；协作
//! `yield_now` 自行 `disable_irq`/`enable_irq` 包络——Bug 2）。

use core::arch::global_asm;

/// 任务切换上下文（callee-saved 子集 + TLS 基址，`#[repr(C)]`，与下方 asm 偏移严格对应）。
///
/// 布局：`x19..x29`（11×8=88）+ `lr`(x30)（8）+ `sp`（8）+ `tpidr`（8）+ FPSIMD 面
/// `fp`（512，q0..q31）+ `fpcr`（8）+ `fpsr`（8）= **640** 字节（GUI-0）。`fp` 用
/// `[u128; 32]` 使结构体自然 16 对齐——SCTLR.A=0（本仓形态）下 `stp q`/`ldp q`
/// 对 Normal 内存非对齐访问架构上**合法**，此处的 16 对齐是纪律而非硬性要求：
/// 消掉对齐边界的情形面 + 让「fp 数组偏移必 16 对齐」成为编译期可断言不变量
/// （Task 内嵌 ctx、Box 布局随结构体对齐，堆 size-class/页路径均满足）。
#[repr(C)]
pub struct TaskContext {
    /// x19..x29（callee-saved，11 个）。r[0]=x19、…、r[10]=x29。
    /// 新任务首切时把 `entry`/`arg` 塞进 r[0](x19)/r[1](x20) 当参向量（见
    /// [`TaskContext::new_kernel`]）。
    pub r: [u64; 11],
    /// x30（lr）。新任务首切时指向 [`task_entry_trampoline`]。
    pub lr: u64,
    /// sp_el1。新任务首切时指向其内核栈顶（16 对齐）。
    pub sp: u64,
    /// tpidr_el0——**每任务**用户 TLS 基址（P6 A-3 实踩补上）：crt0 在 EL0 自设，
    /// 但寄存器本身每 CPU 一份，不随任务切换保存 = 抢占后拿**别人**的 TLS 指针
    /// 裸奔（vblk 首次 TLS 访问在数秒设备 IO 之后，期间 hexyl 改写过 TPIDR_EL0
    /// → 醒来读 [hexyl_TP+0x20] 落未映射区 D-abort；stdhello/hexyl/bsh 幸存只因
    /// 首次 TLS 访问（首个 println 惰性初始化）距自设只有微秒级窗口）。内核任务
    /// 不用 TLS，随行携带的值无害；新任务出生 0（切断残留泄漏的根）。
    pub tpidr: u64,
    /// q0..q31（FPSIMD 全量，GUI-0 起）。**无条件全量保存**——S4 裁决（GUI-10：
    /// trap+lazy 的逃逸路径——异常嵌套窗口/内核 SIMD 借用/per-task CPACR 翻转——
    /// 复杂度超预算，按 kill criteria 直取全量；FPEN 维持 0b11 不陷）。
    /// 新任务出生全零 = 架构复位值（FPCR/FPSR 复位亦为 0）。
    pub fp: [u128; 32],
    /// FPCR（RMODE/FZ/DN/异常陷阱使能等控制位）——随任务保存恢复。
    pub fpcr: u64,
    /// FPSR（NZCV 标志 + 异常累积位）——随任务保存恢复。
    pub fpsr: u64,
}

// 编译期校 TaskContext 与 asm 偏移一致（仿 trap.rs 的 TrapFrame 断言，防「asm/
// #[repr(C)] 错位」风险 R1）。
const _: () = assert!(core::mem::size_of::<TaskContext>() == 640);
const _: () = assert!(core::mem::align_of::<TaskContext>() == 16);
const _: () = assert!(core::mem::offset_of!(TaskContext, r) == 0);
const _: () = assert!(core::mem::offset_of!(TaskContext, lr) == 88);
const _: () = assert!(core::mem::offset_of!(TaskContext, sp) == 96);
const _: () = assert!(core::mem::offset_of!(TaskContext, tpidr) == 104);
const _: () = assert!(core::mem::offset_of!(TaskContext, fp) == 112);
const _: () = assert!(core::mem::offset_of!(TaskContext, fpcr) == 624);
const _: () = assert!(core::mem::offset_of!(TaskContext, fpsr) == 632);

impl TaskContext {
    /// 零值（供静态未初始化上下文占位，如被首次 switch 覆盖的 idle / main）。
    pub const ZERO: TaskContext = TaskContext {
        r: [0; 11],
        lr: 0,
        sp: 0,
        tpidr: 0,
        fp: [0; 32],
        fpcr: 0,
        fpsr: 0,
    };

    /// 构造一个新内核线程的初始上下文。
    ///
    /// 首次 `switch` 进它时：`ret` 跳到 [`task_entry_trampoline`]，后者把 `arg` 放进
    /// `x0`、`blr` 调 `entry`。新任务**无需 TrapFrame**（未被中断过，不走
    /// `load_frame`/`eret`）——用 callee-saved 槽当参向量（标准 trick）：
    /// `x19=entry`、`x20=arg`、`x29=0(fp)`、`lr=trampoline`、`sp=kstack_top`。
    ///
    /// `kstack_top` 须 **16 字节对齐**（AArch64 ABI 硬要求，否则首切即 SP-align 异常
    /// EC=0x26——风险 R2）。
    pub fn new_kernel(
        entry: extern "C" fn(usize) -> !,
        arg: usize,
        kstack_top: usize,
    ) -> TaskContext {
        assert!(kstack_top & 0xF == 0, "kstack_top 必须 16 字节对齐");
        let mut ctx = TaskContext::ZERO;
        ctx.r[0] = entry as *const () as usize as u64; // x19 = entry
        ctx.r[1] = arg as u64; // x20 = arg
        // r[10]=x29(fp)=0：回溯干净终止（已由 ZERO 置零）。
        ctx.lr = task_entry_trampoline as *const () as usize as u64; // x30 = trampoline
        ctx.sp = kstack_top as u64;
        ctx
    }

    /// 构造一个新**用户**线程的初始上下文（2b-4 首个 EL0）。
    ///
    /// 首次 `switch` 进它时：`ret` 跳 [`user_entry_trampoline`]，后者设
    /// `ELR_EL1 = entry_va`、`SPSR_EL1 = EL0t`（0——DAIF 全解蔽，用户态可被 timer
    /// 抢占）、`SP_EL0 = sp0_va`、`x0 = arg`，`eret` 降到 EL0 跳用户入口。参向量
    /// 同 [`TaskContext::new_kernel`] trick：`x19=entry_va`、`x20=arg`、`x21=sp0_va`。
    ///
    /// EL0 运行期间 SP_EL1 恒留本任务内核栈（eret 前的栈位）——用户态异常/IRQ 入口
    /// （向量 0x400/0x480）硬件强制 SPsel=1，落到的正是该栈，`save_frame_el0` 直接可用。
    ///
    /// `kstack_top`/`sp0_va` 均须 **16 字节对齐**（SP-align EC=0x26）。
    pub fn new_user(
        entry_va: usize,
        arg: usize,
        sp0_va: usize,
        kstack_top: usize,
    ) -> TaskContext {
        assert!(kstack_top & 0xF == 0, "kstack_top 必须 16 字节对齐");
        assert!(sp0_va & 0xF == 0, "用户栈顶必须 16 字节对齐");
        let mut ctx = TaskContext::ZERO;
        ctx.r[0] = entry_va as u64; // x19 = 用户入口 VA
        ctx.r[1] = arg as u64; // x20 = arg
        ctx.r[2] = sp0_va as u64; // x21 = 用户 sp_el0
        ctx.lr = user_entry_trampoline as *const () as usize as u64; // x30 = trampoline
        ctx.sp = kstack_top as u64;
        ctx
    }
}

global_asm!(
    // `.arch` 局部重开 FP/SIMD 汇编解析：全局 -Ctarget-feature=-fp-armv8,-neon
    // （内核 SIMD-free 合同）会令汇编器拒收显式 q/FPCR 指令（"instruction
    // requires: fp-armv8"）——本 asm 单元单独开启，只影响汇编解析、不影响
    // 编译器 codegen（Linux 内核 FP asm 文件与 C 分旗编译同构）。
    ".arch armv8-a+fp+simd",
    ".text",
    ".globl switch",
    ".type switch,%function",
    "switch:",
    // x0 = prev（存当前 callee-saved + sp），x1 = next（载入）。纯叶子：无 prologue、
    // 不压自己的栈帧。x2 仅作 SP 中转（caller-saved，可自由 clobber）。
    "    stp x19, x20, [x0, #0]",
    "    stp x21, x22, [x0, #16]",
    "    stp x23, x24, [x0, #32]",
    "    stp x25, x26, [x0, #48]",
    "    stp x27, x28, [x0, #64]",
    "    str x29,      [x0, #80]",   // r[10]=x29（fp）
    "    str x30,      [x0, #88]",   // lr
    "    mov x2, sp",
    "    str x2,       [x0, #96]",   // sp
    "    mrs x2, tpidr_el0",
    "    str x2,       [x0, #104]",  // tpidr_el0（每任务 TLS 基址——先存 prev 再动寄存器）
    // ---- FPSIMD 全量保存（GUI-0）：q0..q31 + FPCR/FPSR，无条件。
    // ---- S4 裁决（GUI-10）：lazy 的逃逸路径复杂度超预算 → 全量；成本 ~70 指令/切换
    // ---- （FAST 对拍 ΔRT ≪ TCG 噪声界）。内核 SIMD-free 合同保证异常路径不碰 q。
    "    stp q0,  q1,  [x0, #112]",
    "    stp q2,  q3,  [x0, #144]",
    "    stp q4,  q5,  [x0, #176]",
    "    stp q6,  q7,  [x0, #208]",
    "    stp q8,  q9,  [x0, #240]",
    "    stp q10, q11, [x0, #272]",
    "    stp q12, q13, [x0, #304]",
    "    stp q14, q15, [x0, #336]",
    "    stp q16, q17, [x0, #368]",
    "    stp q18, q19, [x0, #400]",
    "    stp q20, q21, [x0, #432]",
    "    stp q22, q23, [x0, #464]",
    "    stp q24, q25, [x0, #496]",
    "    stp q26, q27, [x0, #528]",
    "    stp q28, q29, [x0, #560]",
    "    stp q30, q31, [x0, #592]",
    "    mrs x2, fpcr",
    "    str x2,       [x0, #624]",
    "    mrs x2, fpsr",
    "    str x2,       [x0, #632]",
    "    ldp x19, x20, [x1, #0]",
    "    ldp x21, x22, [x1, #16]",
    "    ldp x23, x24, [x1, #32]",
    "    ldp x25, x26, [x1, #48]",
    "    ldp x27, x28, [x1, #64]",
    "    ldr x29,      [x1, #80]",
    "    ldr x30,      [x1, #88]",   // 新 lr：首切时=trampoline
    "    ldr x2,       [x1, #96]",
    "    mov sp, x2",                // 切到新任务的内核栈
    "    ldr x2,       [x1, #104]",
    "    msr tpidr_el0, x2",         // 恢复新任务自己的 TLS 基址（新任务=0）
    // ---- FPSIMD 全量恢复（GUI-0）：载入新任务自己的 q0..q31 + FPCR/FPSR。
    "    ldp q0,  q1,  [x1, #112]",
    "    ldp q2,  q3,  [x1, #144]",
    "    ldp q4,  q5,  [x1, #176]",
    "    ldp q6,  q7,  [x1, #208]",
    "    ldp q8,  q9,  [x1, #240]",
    "    ldp q10, q11, [x1, #272]",
    "    ldp q12, q13, [x1, #304]",
    "    ldp q14, q15, [x1, #336]",
    "    ldp q16, q17, [x1, #368]",
    "    ldp q18, q19, [x1, #400]",
    "    ldp q20, q21, [x1, #432]",
    "    ldp q22, q23, [x1, #464]",
    "    ldp q24, q25, [x1, #496]",
    "    ldp q26, q27, [x1, #528]",
    "    ldp q28, q29, [x1, #560]",
    "    ldp q30, q31, [x1, #592]",
    "    ldr x2,       [x1, #624]",
    "    msr fpcr, x2",
    "    ldr x2,       [x1, #632]",
    "    msr fpsr, x2",
    "    ret",                       // 首切：跳 task_entry_trampoline；否则回新任务上次切出处
    ".size switch, . - switch",
);

global_asm!(
    ".text",
    ".globl task_entry_trampoline",
    ".type task_entry_trampoline,%function",
    "task_entry_trampoline:",
    // 首次 switch 进新任务：x19=entry、x20=arg（由 new_kernel 设）。
    // 把 arg 移到 x0（C-ABI 第一参），blr 调 entry。entry 须 `-> !`；
    // 若误返回，落到 task_exited_trap（halt）——Step 1 乒乓 fn 切回前不会返回。
    "    mov x0, x20",
    // 新任务起步即解蔽 IRQ：首次 switch 经 `ret` 进此 trampoline（非 eret），继承了
    // switch 时刻的 `PSTATE.I=1`（yield_now/IRQ 路径都先屏蔽）。不在此解蔽 → 新任务
    // 永跑在屏蔽态（如 Step 5 忙循环任务收不到 timer tick）。**不违 Bug 2 不变式**：
    // switch 本身仍在 `PSTATE.I=1` 完成；解蔽发生在 switch 之后的 trampoline 里。
    "    msr daifclr, #2",
    "    blr x19",
    "    bl task_exited_trap",
    ".size task_entry_trampoline, . - task_entry_trampoline",
);

global_asm!(
    ".text",
    ".globl user_entry_trampoline",
    ".type user_entry_trampoline,%function",
    "user_entry_trampoline:",
    // 首次 switch 进新用户任务：x19=entry_va、x20=arg、x21=sp0_va（new_user 设）。
    // 解蔽 IRQ（同内核 trampoline 的理由：switch 继承 PSTATE.I=1）；SPSR 另设 0
    //（EL0t、DAIF 解蔽）→ eret 后用户态可被 timer 抢占（向量 0x480）。
    "    msr daifclr, #2",
    "    mov x0, x20",           // 用户入口第一参（eret 不动 GP 寄存器）
    "    msr sp_el0, x21",       // 用户栈顶
    "    msr elr_el1, x19",      // 用户入口 PC
    "    msr spsr_el1, xzr",     // EL0t、DAIF 全解蔽
    "    eret",                  // 降 EL0，跳 entry(arg)；不归——任务死走 kill 路径
    ".size user_entry_trampoline, . - user_entry_trampoline",
);

extern "C" {
    fn switch(prev: *mut TaskContext, next: *const TaskContext);
    fn task_entry_trampoline();
    fn user_entry_trampoline();
}

/// 切换出去：把**当前** callee-saved+sp 存入 `*prev`，载入 `*next` 并 `ret` 进它。
///
/// # Safety
/// - 调用方保证 `PSTATE.I == 1`（IRQ 屏蔽）：IRQ 路径硬件自动屏蔽；协作路径须
///   `disable_irq`/`enable_irq` 包络（Bug 2）。否则 IRQ 打进 switch 中途会损坏状态。
/// - `prev`/`next` 须有效、可解引用、且不别名。
#[inline]
pub unsafe fn context_switch(prev: *mut TaskContext, next: *const TaskContext) {
    // SAFETY：调用方合同（PSTATE.I==1、指针有效不别名）见上；switch 为 extern asm 桩。
    unsafe { switch(prev, next) }
}

/// 任务入口误返回的兜底（asm `bl` 调）。正常任务 `-> !`（loop 或 exit 原语），
/// 到这里即 bug → 打印后 halt（Phase 2a 无可回的调度者，先停）。
#[no_mangle]
pub extern "C" fn task_exited_trap() -> ! {
    crate::uart_puts(b"\n!! task returned from entry (bug) -> halt\n");
    crate::halt();
}
