//! S-1 副核 bring-up 基建（`SMP设计草案.md` §3.2 十项 + §4 S-1 行）。
//!
//! 主核侧 [`smp_bringup`]：PSCI `CPU_ON`（S-0 钉板：HVC conduit、
//! fnid `0xC400_0003`、target MPIDR Aff0=1、entry PA、context_id）拉起副核
//! → CNTVCT 截止帽轮询心跳（状态谓词非竞速，4-5a 纪律）→ dump 副核自报
//! → release（store → dsb sy → sev 门铃序）→ 等 hb2 → `SMP ok ncores=2`。
//!
//! 副核路径 `secondary_entry`（global_asm 落 `.text.boot` 低半 VMA==PA——
//! entry PA 直填符号裸值），§3.2 十项：
//! ①入口 EL 分流：EL2 → 复刻 boot.rs 降级段（elr 落回本函数 EL1 续行）；
//!   EL1 直达（默认机型 PSCI 入口）；EL3 不预期 → 直入 park；
//! ②不清 .bss（主核 boot 已清一次——副核再清 = 扬主核状态）；
//! ③`bl enable_mmu`（表 = 主核所建只读消费；TTBR0/1/SCTLR/TCR/MAIR 皆
//!   per-CPU 寄存器，值全核同构）；
//! ④VBAR_EL1 ← 同一向量表（`__vectors` 高半 VMA − 基址 = PA；CPU1 身份
//!   TTBR0 常活，PA 向量开 MMU 前后皆有效）；
//! ⑤CPACR_EL1.FPEN=0b11（同 boot.rs——消 reset 依赖）；
//! ⑥GIC per-CPU：只动本核 GICC（banked）PMR=0xff + CTLR=1——**绝不碰
//!   GICD**（全局；副核重跑 gic::init 的 GICD_CTLR=0 会清光 CPU0 中断）；
//! ⑦SP = 低半 32KiB 早期栈（主核换高半主栈后已弃用——单写者复用）；
//! ⑧稳态 park = `wfi` 真睡（TCG 实证 wfe=立即返回的 hint 忙转——副核
//!   忙转会拖主核 ~40% 吞吐，S-0 B 相对照）；
//! ⑨CNTKCTL_EL1.EL0VCTEN 自开（per-CPU 复位 0——主核开过不影响副核）；
//! ⑩TPIDR_EL1 = 副核 per-CPU块地址（`.bss` 高半；S-3 per-CPU 区的种子）。
//!
//! 打印纪律（bring-up 窗口 UART 单写者）：副核只写内存 + 至多 1 个哨兵
//! 字符 `'s'` 进 UART（PA 直写，入口执行的第一证据）；主核从 CPU_ON 到
//! 心跳观测之间静默，之后才打印——probe 相曾见双写者字节交错撕行
//! （`'S1:s cpuS_1SEC:low'`），本纪律即其修复。
//!
//! 纪律：asm 只用 movz/movk/ldr=（无 ADRP——2b-1 双向坑）；高半取低半
//! 符号地址越 ±4GB，入口 PA 经链接脚本 `.rodata` 指针字 `__smp_sec_entry`
//! 间接递送（唯一已验证解法，S-0 probe A/B/C 三相实证）；报告区/心跳区
//! 全在高半 `.bss`（主核 boot 清一次；副核经 PA = VMA − KERNEL_VA_BASE 写）。
//!
//! S-1 盒子边界：无 KERN_LOCK（S-2）、无 per-CPU runqueue/IPI（S-3）、
//! console/RX 钉 CPU0（SPI_ROUTES target=0x01）、副核稳态零 UART 写。

use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicU64, Ordering};

use crate::{uart_put_dec, uart_put_hex64, uart_puts};

/// 副核自报字段（asm 偏移 ×8 与此结构逐字段对应；编译期钉板见下方 const 断言）。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SmpReport {
    pub entry_el: u64,  // 入口 CurrentEL>>2（默认机型 1 / virtualization=on 2——实测面）
    pub mpidr: u64,     // 入口 MPIDR_EL1 原始值（期望 0x8000_0001：U 位 + Aff0=1）
    pub x0: u64,        // PSCI context_id 回显（S-0 钉板：x0 递送、x1..x3=0）
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub cntkctl0: u64,  // CNTKCTL 复位读（per-core 证据：主核已开 bit1，副核应为 0）
    pub cntkctl1: u64,  // 自开 EL0VCTEN 后读回（期望 bit1=1）
    pub cntv_ctl: u64,  // CNTV_CTL（期望 0=屏蔽——副核无定时器源）
    pub cntvct: u64,    // CNTVCT 直读（全局计数器）
    pub cntfrq: u64,    // CNTFRQ（期望 62.5MHz=0x3B9ACA0）
    pub gicc_ctlr: u64, // 本核 GICC CTLR 使能前复位读（期望 0）
    pub gicc_pmr: u64,  // PMR 写 0xff 后读回
    pub gicc_iar: u64,  // IAR（期望 0x3ff spurious——无 routed 源）
    pub itgt33: u64,    // GICD ITARGETSR[33]（期望 0x01=主核 SPI_ROUTES 所写路由证据）
    pub tpidr: u64,     // TPIDR_EL1 ← per-CPU 块地址写读
}

const fn zero_report() -> SmpReport {
    SmpReport {
        entry_el: 0, mpidr: 0, x0: 0, x1: 0, x2: 0, x3: 0,
        cntkctl0: 0, cntkctl1: 0, cntv_ctl: 0, cntvct: 0, cntfrq: 0,
        gicc_ctlr: 0, gicc_pmr: 0, gicc_iar: 0, itgt33: 0, tpidr: 0,
    }
}

// 编译期钉板：asm 字段偏移 ×8 与结构布局一致（C-2a Stat 布局案立法——同尺寸
// 不同字段序是最阴的静默错，钉全不钉零头）。
const _: () = assert!(core::mem::size_of::<SmpReport>() == 16 * 8);
const _: () = assert!(core::mem::offset_of!(SmpReport, entry_el) == 0);
const _: () = assert!(core::mem::offset_of!(SmpReport, mpidr) == 8);
const _: () = assert!(core::mem::offset_of!(SmpReport, x0) == 16);
const _: () = assert!(core::mem::offset_of!(SmpReport, x1) == 24);
const _: () = assert!(core::mem::offset_of!(SmpReport, x2) == 32);
const _: () = assert!(core::mem::offset_of!(SmpReport, x3) == 40);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntkctl0) == 48);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntkctl1) == 56);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntv_ctl) == 64);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntvct) == 72);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntfrq) == 80);
const _: () = assert!(core::mem::offset_of!(SmpReport, gicc_ctlr) == 88);
const _: () = assert!(core::mem::offset_of!(SmpReport, gicc_pmr) == 96);
const _: () = assert!(core::mem::offset_of!(SmpReport, gicc_iar) == 104);
const _: () = assert!(core::mem::offset_of!(SmpReport, itgt33) == 112);
const _: () = assert!(core::mem::offset_of!(SmpReport, tpidr) == 120);

/// 副核报告区（高半 `.bss`，主核 boot 清零一次；副核经 PA 写——超时也能收尸）。
#[no_mangle]
pub static mut SMP_SEC_REPORT: SmpReport = zero_report();

/// 心跳 1：副核十项初始化全走完、入 park 前（主核见此才解除静默开始打印）。
#[no_mangle]
pub static SMP_HB1: AtomicU64 = AtomicU64::new(0);
/// 心跳 2：副核收到 release、离开 wfe 邮箱后（握手闭环）。
#[no_mangle]
pub static SMP_HB2: AtomicU64 = AtomicU64::new(0);
/// release 邮箱（主核 store→dsb sy→sev 门铃序；副核查条件→wfe→醒来重查
/// 与之配套才构成丢唤醒免疫——store 全局可见性先于事件投递由 DSB 保证，
/// 见 `smp_bringup` release 段注）。
#[no_mangle]
pub static SMP_RELEASE: AtomicU64 = AtomicU64::new(0);
/// wfe 返回计数（诊断面：TCG wfe=hint 立返 → 此值高；真机应为小值）。
#[no_mangle]
pub static SMP_SEC_SPINS: AtomicU64 = AtomicU64::new(0);

/// per-CPU 块（副核 §3.2 ⑩ / 主核 S-2 前置件 2）：TPIDR_EL1 钉本核块地址
/// ——S-3 起扩为真 per-CPU 区（runqueue/current 指针等）。64 对齐（cache 行
/// 口径预留）。S-2 起首个真字段 = `last_vct`（A11：RR.tick 周期数记账基准
/// per-CPU 化——双 tick 结构性必须：两核共享全局时每核只对「自对方核上次
/// tick 以来的增量」扣减 = slice 约半速不确定燃烧，§3.4 红线；本核只写本核块）。
#[repr(C, align(64))]
pub struct SmpCpu {
    pub cpu: u64,      // 逻辑核号（主核=0 / 副核=1）
    pub mpidr: u64,    // 入口 MPIDR 原始值
    pub present: u64,  // 主核恒 1；副核 = bring-up 心跳闭环（hb2 后置）
    pub last_vct: u64, // RR.tick 记账基准（A11 per-CPU——本核独占写）
    pub pad: [u64; 4],
}

const fn zero_cpu() -> SmpCpu {
    SmpCpu { cpu: 0, mpidr: 0, present: 0, last_vct: 0, pad: [0; 4] }
}

// 编译期钉板：secondary_entry asm 硬编码偏移（cpu@[x24]、mpidr@[x24,#8]、
// present@[x24,#16]）与结构布局一致——SmpReport 同款纪律（钉全不钉零头）。
// 本块是 S-3 per-CPU 区的种子，重排/插字段要到 S-3 才有消费者显形，
// 编译期先拦（C-2a Stat 布局案：同尺寸不同字段序 = 最阴的静默错）。
const _: () = assert!(core::mem::size_of::<SmpCpu>() == 64);
const _: () = assert!(core::mem::offset_of!(SmpCpu, cpu) == 0);
const _: () = assert!(core::mem::offset_of!(SmpCpu, mpidr) == 8);
const _: () = assert!(core::mem::offset_of!(SmpCpu, present) == 16);
const _: () = assert!(core::mem::offset_of!(SmpCpu, last_vct) == 24);

/// 副核 per-CPU 块（TPIDR_EL1 由 secondary_entry asm ⑩ 自钉）。
#[no_mangle]
pub static mut SMP_CPU1: SmpCpu = zero_cpu();

/// 主核 per-CPU 块（前置件 2，S-2）：TPIDR_EL1 由 [`cpu0_init`] 自钉——
/// 单核机型同走（LAST_VCT per-CPU 化不分核数）。
#[no_mangle]
pub static mut SMP_CPU0: SmpCpu = zero_cpu();

/// 主核自钉 per-CPU 块：boot DAIF 屏蔽尾窗、`arm_slice`/`enable_irq` 前调用
///（首 tick 前必达）。副核同款动作在 secondary_entry asm ⑩——两核各自
/// TPIDR_EL1 钉各自块。
pub fn cpu0_init() {
    let mpidr: u64;
    // SAFETY: 只读系统寄存器；boot 单线程窗。
    unsafe {
        asm!("mrs {}, mpidr_el1", out(reg) mpidr);
        let p: *mut SmpCpu = &raw mut SMP_CPU0;
        (*p).cpu = 0;
        (*p).mpidr = mpidr;
        (*p).present = 1; // 主核恒在场（无 bring-up 握手）
        asm!("msr tpidr_el1, {0}", in(reg) p as u64);
        asm!("isb");
    }
}

/// 本核 LAST_VCT 槽（A11/前置件 2）：TPIDR_EL1 → `SmpCpu.last_vct`。返回
/// 裸指针由调用方 volatile 读写（IRQ 热路径零包装）；None = TPIDR_EL1 未装
///（cpu0_init 前防御窗——RR.tick 实际不可达此态）——调用方回落自有全局。
pub fn last_vct_slot() -> Option<*mut u64> {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return None;
    }
    // SAFETY: 非零即本核钉入的 SmpCpu 地址（cpu0_init / secondary_entry ⑩）；
    // last_vct 字段本核独占（tick 锁窗内单写者）。
    Some(unsafe { core::ptr::addr_of_mut!((*(base as *mut SmpCpu)).last_vct) })
}

/// boot asm 钉存的**入口** EL（1/2/3）：`_start` 处检测、经 x20（callee-saved，
/// eret 不动 GPR）携带过 EL2→EL1 降级路径，`.Lbss_done`（`.bss` 清零之后）
/// 经 PA 写入——在 EL1 落点再读 CurrentEL 只会得到降级后的 1，必须入口即钉。
/// S-1 conduit 分型门：PSCI HVC 只在默认机型（boot EL1）发放——
/// `virtualization=on` 下 HVC 从 EL1 陷阱到 EL2 且 QEMU 不再拦截为 PSCI
/// （VBAR_EL2=0 → PC=0 死循环，probe C 相 + S-1 主树首跑双实据挂死），
/// SMC 变体留档不实现（S-1 盒子）。高半普通 `.bss`（boot asm 写
/// PA = VMA − 基址——与 .bss 清零循环同款口径）。
#[no_mangle]
pub static mut BOOT_EL: u64 = 0;

extern "C" {
    /// 链接脚本高半 `.rodata` 指针字 = 低半 `secondary_entry` 绝对地址
    /// （VMA==PA）。高半 Rust 直取低半符号地址越 ADRP ±4GB（2b-1 双向坑），
    /// 经指针字间接取——S-0 probe 三相验证的唯一解法。
    static __smp_sec_entry: u64;
}

global_asm!(
    ".section .text.boot",
    ".globl secondary_entry",
    ".type secondary_entry,%function",
    "secondary_entry:",
    "    ldr x27, =0xFFFFFF8000000000",      // x27 = KERNEL_VA_BASE（全程钉住）
    "    ldr x26, =SMP_SEC_REPORT",
    "    sub x26, x26, x27",                 // x26 = 报告区 PA（MMU 前经 PA 写）
    "    stp x0, x1, [x26, #16]",            // 第一时间保存入口 x0..x3
    "    stp x2, x3, [x26, #32]",
    "    mrs x0, CurrentEL",
    "    lsr x0, x0, #2",
    "    str x0, [x26]",                     // entry_el
    "    mrs x1, mpidr_el1",
    "    str x1, [x26, #8]",                 // mpidr
    "    ldr x1, =0x09000000",               // 哨兵字符 's'（副核唯一 UART 写；
    "    mov x0, #0x73",                     //  PA 直写——入口执行的第一证据）
    "    str w0, [x1]",
    "    ldr x0, =__vectors",                // ④VBAR_EL1 ← 向量表 PA（同 CPU0
    "    sub x0, x0, x27",                   //  一张表；CPU1 身份 TTBR0 常活）
    "    msr vbar_el1, x0",
    "    isb",
    "    ldr x0, [x26]",                     // entry_el 重载（x0 刚被哨兵占用）
    "    cmp x0, #2",
    "    b.eq .Lsmp_from_el2",
    "    cmp x0, #3",
    "    b.eq .Lsmp_final",                  // EL3 不预期（virt 无）；直入 park
    "    b .Lsmp_el1",                       // EL1 直达（默认机型 PSCI 入口）

    // ---- ① EL2 → EL1 降级（复刻 boot.rs .Lfrom_el2；elr 落回本函数）----
    ".Lsmp_from_el2:",
    "    mrs x0, sctlr_el2",
    "    bic x0, x0, #1",                    // 清 M 位：关 EL2 MMU
    "    msr sctlr_el2, x0",
    "    isb",
    "    tlbi alle2",
    "    dsb sy",
    "    isb",
    "    movz x0, #0x8000, lsl #16",         // HCR_EL2.RW=1 → EL1 为 AArch64
    "    msr hcr_el2, x0",
    "    msr sctlr_el1, xzr",                // EL1 MMU 干净状态
    "    isb",
    "    ldr x0, =.Lsmp_el1",
    "    msr elr_el2, x0",
    "    mov x0, #0x3c5",                    // EL1h + DAIF 屏蔽
    "    msr spsr_el2, x0",
    "    eret",

    // ---- EL1 续行（直达 或 eret 后）----
    ".Lsmp_el1:",
    "    ldr x0, =__boot_stack_top",         // ⑦复用主核已弃用的低半 32KiB 早期栈
    "    mov sp, x0",
    "    movz x0, #0x30, lsl #16",           // ⑤CPACR_EL1.FPEN=0b11（同 boot.rs）
    "    msr cpacr_el1, x0",
    "    isb",
    "    mrs x0, cntkctl_el1",               // ⑨复位读（per-core 证据面）
    "    str x0, [x26, #48]",
    "    orr x0, x0, #2",                    // EL0VCTEN=1（B-1 同款门，自开）
    "    msr cntkctl_el1, x0",
    "    isb",
    "    mrs x0, cntkctl_el1",
    "    str x0, [x26, #56]",
    "    mrs x0, cntv_ctl_el0",
    "    str x0, [x26, #64]",
    "    mrs x0, cntvct_el0",
    "    str x0, [x26, #72]",
    "    mrs x0, cntfrq_el0",
    "    str x0, [x26, #80]",
    // ---- ⑥GIC per-CPU：只动本核 GICC（banked），绝不碰 GICD ----
    "    ldr x1, =0x08010000",               // GICC PA
    "    ldr w0, [x1]",                      // CTLR 复位读（期望 0）
    "    str x0, [x26, #88]",
    "    mov w0, #0xff",
    "    str w0, [x1, #4]",                  // PMR=0xff（全优先级放行）
    "    ldr w0, [x1, #4]",
    "    str x0, [x26, #96]",
    "    ldr w0, [x1, #0xC]",                // IAR spurious 读（无副作用）
    "    str x0, [x26, #104]",
    "    mov w0, #1",
    "    str w0, [x1]",                      // GICC CTLR=1（无 routed 源 → 无中断）
    "    ldr x1, =0x08000000",               // GICD PA（只读面）
    "    ldrb w0, [x1, #0x821]",             // ITARGETSR[33]（0x800+33）
    "    str x0, [x26, #112]",
    // ---- ③开 MMU（表 = 主核 boot 所建、只读消费；寄存器全 per-CPU）----
    "    bl enable_mmu",
    // ---- 高半续行：⑩TPIDR_EL1 = per-CPU 块 ----
    "    ldr x26, =SMP_SEC_REPORT",          // 换 VMA（TTBR1 生效）
    "    ldr x24, =SMP_CPU1",
    "    mov x0, #1",
    "    str x0, [x24]",                     // cpu = 1
    "    mrs x1, mpidr_el1",
    "    str x1, [x24, #8]",                 // mpidr
    "    msr tpidr_el1, x24",                // TPIDR_EL1 ← per-CPU 块地址
    "    mrs x1, tpidr_el1",
    "    str x1, [x26, #120]",               // 写读证据
    // ---- 心跳 1 → wfe 邮箱 park（先置后查再 wfe：丢失唤醒免疫序）----
    "    dmb ish",
    "    ldr x9, =SMP_HB1",
    "    mov x0, #1",
    "    str x0, [x9]",
    "    dmb ish",
    ".Lsmp_park:",
    "    ldr x9, =SMP_RELEASE",
    "    ldr x0, [x9]",
    "    cbnz x0, .Lsmp_released",
    "    wfe",
    "    ldr x9, =SMP_SEC_SPINS",
    "    ldr x0, [x9]",
    "    add x0, x0, #1",
    "    str x0, [x9]",
    "    b .Lsmp_park",
    ".Lsmp_released:",
    "    dmb ish",
    "    ldr x9, =SMP_HB2",
    "    mov x0, #1",
    "    str x0, [x9]",
    "    str x0, [x24, #16]",                // per-CPU 块 present=1
    "    dmb ish",
    ".Lsmp_final:",                          // ⑧稳态 park：wfi 真睡（零 UART 写）
    "    wfi",
    "    b .Lsmp_final",
    ".size secondary_entry, . - secondary_entry",
    ".ltorg",
);

/// 读 boot asm 钉存的入口 EL（高半 `.bss` 普通静态量，主核单写点后只读）。
fn boot_el() -> u64 {
    // SAFETY: 读 boot asm 单写点钉存的静态量（此后只读）。
    unsafe { (&raw const BOOT_EL).read_volatile() }
}

fn cntvct() -> u64 {
    let v: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, cntvct_el0", out(reg) v) };
    v
}

fn cntfrq() -> u64 {
    let v: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, cntfrq_el0", out(reg) v) };
    v
}

/// 状态谓词轮询（CNTVCT 截止帽；非竞速——谓词满足即真，到点再验一次即假）。
fn wait_secs(secs: u64, mut pred: impl FnMut() -> bool) -> bool {
    let freq = cntfrq();
    let deadline = cntvct().saturating_add(secs.saturating_mul(freq));
    loop {
        if pred() {
            return true;
        }
        if cntvct() >= deadline {
            return pred();
        }
        for _ in 0..256 {
            core::hint::spin_loop();
        }
    }
}

/// 主核侧 bring-up：CPU_ON → 心跳 → dump → release → hb2 → `SMP ok ncores=2`。
///
/// 调用窗口契约：boot DAIF 屏蔽尾窗（全 spawn/回调注册后、`arm_slice`/
/// `enable_irq` 前）——主核轮询期间静默、无中断、无调度，与副核窗口
/// 结构性无重叠。
///
/// 门（S-1 盒子）：`efi_boot`（EFI/handoff 路的 PSCI conduit 归固件管，
/// 未实测——跳过）+ 入口 EL==1（默认机型；EL2 机型 HVC 挂死风险见
/// [`BOOT_EL`] 注）。单核（无 -smp）时 CPU_ON 返回 -2 → 一行降级，boot 继续。
pub fn smp_bringup(efi_boot: bool) {
    if efi_boot {
        uart_puts(b"SMP: skip (EFI handoff boot)\n");
        return;
    }
    let el = boot_el();
    if el != 1 {
        uart_puts(b"SMP: skip (boot el=");
        uart_put_dec(el);
        uart_puts(b"; HVC conduit = default machine only, S-1 box)\n");
        return;
    }
    // SAFETY: 读链接期定值指针字（高半 .rodata，无副作用）。
    let entry_pa = unsafe { (&raw const __smp_sec_entry).read_volatile() } as usize;
    let ctx_id: u64 = 0x5EC0_DEAD_BEEF_0001; // S-0 钉板口径（x0 回显实测面）
    let ret: u64;
    // CPU_ON 前打点（挂死可定位）；调用后静默直到心跳（UART 单写者纪律）。
    uart_puts(b"\nSMP: cpu_on entry=");
    uart_put_hex64(entry_pa as u64);
    uart_puts(b"\n");
    unsafe {
        asm!(
            "hvc #0",
            inout("x0") 0xC400_0003u64 => ret,   // PSCI CPU_ON（S-0 钉板 fnid）
            inout("x1") 0x1u64 => _,             // target MPIDR Aff0=1（S-0 钉板）
            inout("x2") entry_pa as u64 => _,    // entry PA（.text.boot VMA==PA）
            inout("x3") ctx_id => _,             // context_id
            // SMCCC：x4-x17 调用者保存 + x18 平台寄存器（规范允许固件
            // 不保留——真机 ATF 可改写；不赌实现保留，寄存器套路纪律）
            out("x4") _, out("x5") _, out("x6") _, out("x7") _,
            out("x8") _, out("x9") _, out("x10") _, out("x11") _,
            out("x12") _, out("x13") _, out("x14") _, out("x15") _,
            out("x16") _, out("x17") _, out("x18") _,
        );
    }
    if ret != 0 {
        // 单核（默认 -smp 1）常态：PSCI 返回 -2（INVALID_PARAMS——目标核不在）。
        uart_puts(b"SMP: cpu_on ret=");
        uart_put_hex64(ret);
        uart_puts(b" -> single-core, skip\n");
        return;
    }
    if !wait_secs(5, || SMP_HB1.load(Ordering::Acquire) >= 1) {
        // 超时也 dump：入口块（entry_el/mpidr/x0..x3）在 EL 分流前已写入——
        // 副核死在半路也能收尸定位死点。
        // SAFETY: 只读副核报告区（副核可能仍在写——值仅作诊断）。
        let r = unsafe { (&raw const SMP_SEC_REPORT).read_volatile() };
        uart_puts(b"SMP: FAIL hb timeout el=");
        uart_put_hex64(r.entry_el);
        uart_puts(b" mpidr=");
        uart_put_hex64(r.mpidr);
        uart_puts(b" x0=");
        uart_put_hex64(r.x0);
        uart_puts(b" spins=");
        uart_put_hex64(SMP_SEC_SPINS.load(Ordering::Relaxed));
        uart_puts(b"\n");
        return;
    }
    // 副核已入 wfe park（静默）——dump 报告 + 主控打印窗口。
    // SAFETY: 副核此后只写 SMP_RELEASE 观察位与 spins 计数，报告区冻结。
    let r = unsafe { (&raw const SMP_SEC_REPORT).read_volatile() };
    uart_puts(b"SMP: sec up el=");
    uart_put_hex64(r.entry_el);
    uart_puts(b" mpidr=");
    uart_put_hex64(r.mpidr);
    uart_puts(b" x0=");
    uart_put_hex64(r.x0);
    uart_puts(b" ck0=");
    uart_put_hex64(r.cntkctl0);
    uart_puts(b" ck1=");
    uart_put_hex64(r.cntkctl1);
    uart_puts(b" gctl=");
    uart_put_hex64(r.gicc_ctlr);
    uart_puts(b" gpmr=");
    uart_put_hex64(r.gicc_pmr);
    uart_puts(b" itgt33=");
    uart_put_hex64(r.itgt33);
    uart_puts(b" tpidr=");
    uart_put_hex64(r.tpidr);
    uart_puts(b"\n");
    // release：置邮箱 → dsb sy → sev（Arm ARM 门铃序——store 的全局可见性
    // 先于事件投递由 DSB 保证；副核侧查条件→wfe→醒来重查，与之配套才构成
    // 丢唤醒免疫）。缺 DSB 时真机存在按次随机的丢唤醒窗口：副核被 SEV 兑换
    // 的一次重查若仍读 0，回 wfe 后事件寄存器已清、全系统再无唤醒源 → 永久
    // 睡死（5s 帽收口 `SMP: FAIL hb2 timeout`）。TCG 的 wfe=hint 立返
    // （副核实际忙轮询 RELEASE）结构性掩蔽此窗——恰因此真机序必须在代码里
    // 修掉，不能靠 CI。
    SMP_RELEASE.store(1, Ordering::Release);
    // SAFETY: 特权指令；EL1。无 nomem/pure 选项 = 有内存副作用，编译器
    // 不将其与上方 store 重排；dsb sy 保证 store 先于 sev 对全系统可见。
    unsafe { asm!("dsb sy", "sev") };
    if !wait_secs(5, || SMP_HB2.load(Ordering::Acquire) >= 1) {
        uart_puts(b"SMP: FAIL hb2 timeout\n");
        return;
    }
    // 状态谓词全过才打标记（4-5a 纪律）：非竞速——两心跳都是内存状态断言。
    uart_puts(b"SMP ok ncores=2\n");
}
