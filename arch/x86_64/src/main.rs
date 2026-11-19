// P7-x0（ADR #41 x86_64 最小集第一刀）：PVH 引导 + 32→64 长模式 + COM1 串口。
// P7-x1（第二刀）：PIT 100Hz + 8259 PIC 重映射 + 最小 IDT——五原语之 2/5 定时器。
// P7-x2（第三刀）：异常向量 0..31 dump 面 + #UD 自证恢复——五原语之 3/5 异常向量。
// P7-x3（第四刀）：内核任务上下文切换 + A/B 乒乓自证——五原语之 4/5。
// P7-x4（第五刀 · 收官）：SYSCALL/SYSRET 入口 + TSS + CPL3 用户程序自证——五原语之 5/5。
//
// 引导协议（G-eco fw_cfg 同款方法论：四层实证收敛，QEMU 11.0.3 源码核实）：
//  - multiboot 路对 ELF64 明确拒收（hw/core/loader.c：EM_X86_64 直接报
//    "Cannot load x86-64 image"）——别试。
//  - PVH 路对 ELF64 成立：内核 ELF note `Xen`/type 18（XEN_ELFNOTE_PHYS32_ENTRY，
//    desc=32 位入口地址）声明 PVH；QEMU 主侧按 program header 装载 ELF 本体，
//    pvh.bin option ROM 建好 hvm_start_info + e820 后以
//    **32 位保护模式、分页关闭**（CR0=PE、CS=0x08 flat、数据段 0x10）跳入口，
//    **EBX = &hvm_start_info**（物理地址）。
//  - 入口职责：自建 4 级恒等页表（首 1GiB，512×2MiB 大页——qemu64 默认 CPU
//    不广播 1GiB 页，2MiB 是可移植面）→ PAE/EFER.LME/CR0.PG → 远跳 64 位代码。
//
// 验收标记（状态谓词，非竞速）：`BerryOS boot (x86_64)` + `PVH ok`（hvm_start_info
// magic == XEN_HVM_START_MAGIC_VALUE 且 version==1——引导协议握手的**内容活证**，
// 非仅「串口能出字」）+ `TIMER ok n=`（PIT→PIC→IDT→ sti 全链真中断收满 100 拍——
// 拍数只能来自 handler 递增，内容活证）+ `EXC ok vec=6`（ud2 触发 #UD → dump 落
// 串口 → rip+2 恢复续行 + rax 金丝雀往返不变——异常门/桩帧/全量保存恢复三环的
// 机器断言）+ `CTX ok n=`（A/B 两内核任务在独立 .bss 栈上经 context_switch
// 严格乒乓 128 轮——每轮双方互验对方计数，计数值只能来自真实交替切换而非
// 两个独立自增）+ `SYS ok n=`（CPL3 用户程序 128 轮 SYSCALL 往返对拍
// 3i+7 + DONE 哨兵经内核恢复路径归位——MSR/TSS/syscall 桩/用户段全链的
// 机器断言）+ `X86 ok`。
//
// 范围盒子（ADR #41）：五原语（串口 + 定时器 + 异常向量 + 上下文切换 +
// SVC 入口）全落地——IDT 装 0..31 异常门（#DF/#SS 挂 IST1）+ 0x20 定时器
// 门；意外向量（32..255 无门）→ #GP(13) → dump + `EXC panic` halt（smoke
// 快败 grep；三重故障路退役为有门可观测）。收官 gate = smoke-x86 挂 CI。
// SMP/ACPI/PCIe/APIC 永不进 v1 盒。

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// ---------- PVH note：ELF note `Xen`/type 18 = 32 位入口声明 ----------
// 布局（ELF64 note，4 字节对齐）：namesz=4（"Xen\0"）/descsz=4（入口地址）/
// type=18/name/desc。绝不能被链接脚本 DISCARD（见 link-x86.ld 头注）。
global_asm!(
    ".section .note.xen, \"a\"",
    ".balign 4",
    "    .long 4",              // namesz
    "    .long 4",              // descsz
    "    .long 18",             // XEN_ELFNOTE_PHYS32_ENTRY
    "    .asciz \"Xen\"",
    "    .long _start",         // 32 位物理入口（链接基址 0x100000 < 4GiB）
    // rustc 1.97 默认 Intel 语法（本仓 aarch64 全线 AT&T——显式钉死防漂）
    options(att_syntax)
);

// ---------- GDT + boot 页表（.rodata 文件面预零：PT_LOAD 装载即零，弹床免清）----------
// P7-x4：GDT 扩 7 quads——0x18 用户代码 / 0x20 用户数据（DPL3）/ 0x28+0x30 TSS
// 描述符（16B 系统描述符，运行期 gdt_tss_init 填）。恒等映射把 .rodata 页映
// RW（硬件写权限看 PTE 位不看 PT_LOAD 旗标，P7-x0 留档）——运行期写 0x28/0x30
// 不 fault。boot 32 位侧 lgdt 仍 limit=23（弹床只需前 3 段）；gdt_tss_init 以
// limit=55 重载。.balign 4096 的 pml4 吸收 32B 扩展，无搬迁。
global_asm!(
    ".section .rodata",
    ".balign 16",
    ".globl gdt64",
    "gdt64:",
    "    .quad 0",                    // 0x00：空描述符
    "    .quad 0x00AF9A000000FFFF",  // 0x08：64 位代码（L=1, D=0, P=1, exec）
    "    .quad 0x00CF92000000FFFF",  // 0x10：数据（RW, P=1）
    "    .quad 0x00AFFA000000FFFF",  // 0x18：用户代码（L=1, DPL3）
    "    .quad 0x00CFF2000000FFFF",  // 0x20：用户数据（RW, DPL3）
    "    .quad 0",                    // 0x28：TSS 描述符 low（运行期填）
    "    .quad 0",                    // 0x30：TSS 描述符 high（<4GiB 恒 0）
    ".balign 8",
    "gdtr64:",
    "    .word 23",                   // limit = 3 段 × 8 - 1（boot 32 位侧够用；gdt_tss_init 重载 55）
    "    .long gdt64",                // 32 位 lgdt 只取 6 字节（word limit + dword base）
    ".balign 4096",
    ".globl pml4",
    "pml4:",
    "    .zero 4096",
    ".globl pdpt",
    "pdpt:",
    "    .zero 4096",
    ".globl pd",
    "pd:",
    "    .zero 4096",
    options(att_syntax)
);

// ---------- .bss 静态槽（PVH start_info 留档 + 引导栈）----------
global_asm!(
    ".section .bss",
    ".balign 4",
    ".globl pvh_start_info",
    "pvh_start_info:",
    "    .long 0",
    ".balign 16",
    "boot_stack:",
    "    .space 16384",
    "boot_stack_top:",
    options(att_syntax)
);

// ---------- 32 位入口 + 长模式弹床 ----------
global_asm!(
    ".section .text.boot, \"ax\"",
    ".code32",
    ".globl _start",
    ".type _start, @function",
    "_start:",
    "    cli",
    // .bss 清零——不信任 loader 对 NOBITS 的处理（aarch64 boot.rs 同款纪律）。
    // 顺序敏感：先清 .bss **再** stash EBX（槽自身在 .bss 里，倒序会被清零抹掉）。
    "    mov $__bss_start, %edi",
    "    mov $__bss_end, %esi",
    "    xor %eax, %eax",
    "1:  cmp %esi, %edi",
    "    jae 2f",
    "    mov %eax, (%edi)",
    "    add $4, %edi",
    "    jmp 1b",
    "2:",
    // PVH ABI：EBX = &hvm_start_info（物理地址 < 1GiB，恒等映射内可读）
    "    mov %ebx, pvh_start_info",
    "    mov $boot_stack_top, %esp",
    // 首级表项（表体 .rodata 预零，只填两级指针）：PML4[0]→PDPT、PDPT[0]→PD
    "    mov $pdpt, %eax",
    "    or $3, %eax",                // P|RW
    "    mov %eax, pml4",
    "    mov $pd, %eax",
    "    or $3, %eax",                // P|RW（非大页项：无 PS 位）
    "    mov %eax, pdpt",
    // PD：512 × 2MiB 大页恒等映射 [0, 1GiB)
    "    mov $pd, %edi",
    "    mov $0x83, %eax",            // PS(0x80)|RW|P
    "    mov $512, %ecx",
    "3:  mov %eax, (%edi)",
    "    add $8, %edi",
    "    add $0x200000, %eax",        // 2MiB 步进
    "    loop 3b",
    // 开长模式：PAE → CR3 → EFER.LME → CR0.PG（PG 置位后下一条取指即走新映射，
    // 恒等映射保证无缝）→ 换 GDT → 远跳 64 位代码段
    "    mov %cr4, %eax",
    "    or $0x20, %eax",             // PAE
    "    mov %eax, %cr4",
    "    mov $pml4, %eax",
    "    mov %eax, %cr3",
    "    mov $0xC0000080, %ecx",      // IA32_EFER
    "    rdmsr",
    "    or $0x100, %eax",            // LME
    "    wrmsr",
    "    mov %cr0, %eax",
    "    or $0x80000001, %eax",       // PG|PE
    "    mov %eax, %cr0",
    "    lgdt gdtr64",
    "    ljmp $0x08, $x86_entry64",
    // ---- 64 位侧 ----
    ".code64",
    "x86_entry64:",
    "    mov $0x10, %eax",
    "    mov %eax, %ss",
    "    mov %eax, %ds",
    "    mov %eax, %es",
    "    xor %eax, %eax",             // fs/gs 清零（ROM 残留基址无害但统一清）
    "    mov %eax, %fs",
    "    mov %eax, %gs",
    "    mov $boot_stack_top, %rsp",
    "    call x86_main",
    "4:  hlt",
    "    jmp 4b",
    ".size _start, . - _start",
    options(att_syntax)
);

// ---------- IRQ0 中断桩（P7-x1）----------
// 对齐两件事，缺一不可：
//  1. 被打断上下文的奇偶不可假设（park 循环/未来任意上下文都可能被打断）——
//     数 push 个数凑对齐只在单一被打断点成立；锚 rbp 后 and $-16 归一。
//  2. **call 边界奇偶（SysV ABI）**：call 指令执行前 RSP 须 ≡ 0 (mod 16)。
//     and $-16 只解决 1；下方恰压 9 个调用者保存 GP = 72B（奇数个 push），
//     call 现场 RSP ≡ 8 (mod 16)——今日 handler 纯整数路径零可观测影响，但
//     P7-x2/x4 克隆此桩后任何 handler 调用树出现 movaps（对齐 SSE 栈槽）
//     即 #GP。故 and 后 sub $8 凑偶（尾部 rbp 恢复兜底，无需对称 add）。
// 中断门入口 CPU 已清 IF，无重入。只压调用者保存寄存器（Rust ABI 允许
// callee 破坏的全部 GP 面；XMM 亦 caller-saved——builtin target 禁 SSE 使
// 其结构性安全，见 .cargo/config.toml 注）。iretq 恢复 RFLAGS（含 IF）。
global_asm!(
    ".section .text, \"ax\"",
    ".globl timer_isr",
    ".type timer_isr, @function",
    "timer_isr:",
    "    cld",
    "    push %rbp",
    "    mov %rsp, %rbp",
    "    and $-16, %rsp",
    "    sub $8, %rsp",              // 9 push 奇数——call 前 RSP 补 8 凑 16 对齐（SysV）
    "    push %rax",
    "    push %rdi",
    "    push %rsi",
    "    push %rdx",
    "    push %rcx",
    "    push %r8",
    "    push %r9",
    "    push %r10",
    "    push %r11",
    "    call x86_timer_irq",
    "    pop %r11",
    "    pop %r10",
    "    pop %r9",
    "    pop %r8",
    "    pop %rcx",
    "    pop %rdx",
    "    pop %rsi",
    "    pop %rdi",
    "    pop %rax",
    "    mov %rbp, %rsp",
    "    pop %rbp",
    "    iretq",
    ".size timer_isr, . - timer_isr",
    options(att_syntax)
);

// ---------- P7-x2：CPU 异常桩 0..31 + 公共入口 ----------
// 两类向量帧形不同：8/10/11/12/13/14/17/21/29/30 CPU 自动压错误码（Exception
// Error Code），其余不压——无错误码桩先 `push $0` 占位，统一成
// [regs..][vec][err][rip][cs][rflags] 形状（rsp/ss 仅 CPL3 来源由 CPU 追加，
// 本刀异常只来自 CPL0）。64 位 iretq 对**同 CPL 来源只弹 rip/cs/rflags**
// （timer_isr 100 拍往返已实证）。
macro_rules! exc_stub {
    ($name:ident, $vec:expr, err) => {
        global_asm!(
            concat!(
                ".section .text, \"ax\"\n",
                ".globl ", stringify!($name), "\n",
                ".type ", stringify!($name), ", @function\n",
                stringify!($name), ":\n",
                "    push $", stringify!($vec), "\n",
                "    jmp exc_common\n",
                ".size ", stringify!($name), ", . - ", stringify!($name), "\n",
            ),
            options(att_syntax)
        );
    };
    ($name:ident, $vec:expr, noerr) => {
        global_asm!(
            concat!(
                ".section .text, \"ax\"\n",
                ".globl ", stringify!($name), "\n",
                ".type ", stringify!($name), ", @function\n",
                stringify!($name), ":\n",
                "    push $0\n",
                "    push $", stringify!($vec), "\n",
                "    jmp exc_common\n",
                ".size ", stringify!($name), ", . - ", stringify!($name), "\n",
            ),
            options(att_syntax)
        );
    };
}

exc_stub!(exc_stub_0, 0, noerr);   // #DE
exc_stub!(exc_stub_1, 1, noerr);   // #DB
exc_stub!(exc_stub_2, 2, noerr);   // NMI
exc_stub!(exc_stub_3, 3, noerr);   // #BP
exc_stub!(exc_stub_4, 4, noerr);   // #OF
exc_stub!(exc_stub_5, 5, noerr);   // #BR
exc_stub!(exc_stub_6, 6, noerr);   // #UD（自证向量）
exc_stub!(exc_stub_7, 7, noerr);   // #NM
exc_stub!(exc_stub_8, 8, err);     // #DF
exc_stub!(exc_stub_9, 9, noerr);   // #CO（保留）
exc_stub!(exc_stub_10, 10, err);   // #TS
exc_stub!(exc_stub_11, 11, err);   // #NP
exc_stub!(exc_stub_12, 12, err);   // #SS
exc_stub!(exc_stub_13, 13, err);   // #GP
exc_stub!(exc_stub_14, 14, err);   // #PF
exc_stub!(exc_stub_15, 15, noerr); // 保留
exc_stub!(exc_stub_16, 16, noerr); // #MF
exc_stub!(exc_stub_17, 17, err);   // #AC
exc_stub!(exc_stub_18, 18, noerr); // #MC
exc_stub!(exc_stub_19, 19, noerr); // #XM
exc_stub!(exc_stub_20, 20, noerr); // #VE
exc_stub!(exc_stub_21, 21, err);   // #CP
exc_stub!(exc_stub_22, 22, noerr); // 保留
exc_stub!(exc_stub_23, 23, noerr);
exc_stub!(exc_stub_24, 24, noerr);
exc_stub!(exc_stub_25, 25, noerr);
exc_stub!(exc_stub_26, 26, noerr);
exc_stub!(exc_stub_27, 27, noerr);
exc_stub!(exc_stub_28, 28, noerr); // #HV
exc_stub!(exc_stub_29, 29, err);   // #VC
exc_stub!(exc_stub_30, 30, err);   // #SX
exc_stub!(exc_stub_31, 31, noerr); // 保留

// 公共入口：全量 16 GP 入栈构成 ExcFrame（push 序 = 结构体字段升序），rbp 锚定
// 帧基后 and $-16 归一——打断点奇偶不可假设（timer_isr 同款两层对齐纪律）；
// 此处 and 后零 push 即 call，天然满足 call 边界 RSP ≡ 0 (mod 16)。帧基经 rbx
// 转交（rbx 原值已入栈；SysV callee-saved，跨 Rust call 存活）。恢复路径改
// frame.rip 由 handler 完成；栈上其余字段原样 iretq 弹回。
global_asm!(
    ".section .text, \"ax\"",
    "exc_common:",
    "    cld",
    "    push %r15",
    "    push %r14",
    "    push %r13",
    "    push %r12",
    "    push %r11",
    "    push %r10",
    "    push %r9",
    "    push %r8",
    "    push %rdi",
    "    push %rsi",
    "    push %rbp",
    "    push %rbx",
    "    push %rdx",
    "    push %rcx",
    "    push %rax",
    "    mov %rsp, %rbx",
    "    mov %rsp, %rbp",
    "    and $-16, %rsp",
    "    mov %rbx, %rdi",
    "    call x86_exception",
    "    mov %rbp, %rsp",
    "    pop %rax",
    "    pop %rcx",
    "    pop %rdx",
    "    pop %rbx",
    "    pop %rbp",
    "    pop %rsi",
    "    pop %rdi",
    "    pop %r8",
    "    pop %r9",
    "    pop %r10",
    "    pop %r11",
    "    pop %r12",
    "    pop %r13",
    "    pop %r14",
    "    pop %r15",
    "    add $16, %rsp",           // 弹桩压的 vec + err（错误码向量随之一并弹走）
    "    iretq",
    options(att_syntax)
);

extern "C" {
    static pvh_start_info: u32;
    static gdt64: u64;   // P7-x4：GDT 基（u64 桩取地址，运行期写 0x28/0x30 描述符）
    static pml4: u64;    // boot 页表三级（用户页 U/S 授权的 RMW 对象）
    static pdpt: u64;
    static pd: u64;
    static timer_isr: u8; // global_asm 定义（u8 桩只取地址，永不读值）
    static exc_stub_0: u8;
    static exc_stub_1: u8;
    static exc_stub_2: u8;
    static exc_stub_3: u8;
    static exc_stub_4: u8;
    static exc_stub_5: u8;
    static exc_stub_6: u8;
    static exc_stub_7: u8;
    static exc_stub_8: u8;
    static exc_stub_9: u8;
    static exc_stub_10: u8;
    static exc_stub_11: u8;
    static exc_stub_12: u8;
    static exc_stub_13: u8;
    static exc_stub_14: u8;
    static exc_stub_15: u8;
    static exc_stub_16: u8;
    static exc_stub_17: u8;
    static exc_stub_18: u8;
    static exc_stub_19: u8;
    static exc_stub_20: u8;
    static exc_stub_21: u8;
    static exc_stub_22: u8;
    static exc_stub_23: u8;
    static exc_stub_24: u8;
    static exc_stub_25: u8;
    static exc_stub_26: u8;
    static exc_stub_27: u8;
    static exc_stub_28: u8;
    static exc_stub_29: u8;
    static exc_stub_30: u8;
    static exc_stub_31: u8;
}

// XEN_HVM_START_MAGIC_VALUE（"xEn\3" LE）
const HVM_START_MAGIC: u32 = 0x336e_c578;

// ---------- COM1 串口（16550，-nographic 重定向到 stdio）----------
const COM1: u16 = 0x3F8;

unsafe fn outb(port: u16, val: u8) {
    asm!("out %al, %dx", in("al") val, in("dx") port, options(att_syntax, nomem, nostack, preserves_flags));
}

unsafe fn inb(port: u16) -> u8 {
    let ret: u8;
    asm!("in %dx, %al", lateout("al") ret, in("dx") port, options(att_syntax, nomem, nostack, preserves_flags));
    ret
}

fn serial_init() {
    unsafe {
        outb(COM1 + 1, 0x00); // 关串口中断（串口无 IRQ 面：IRQ4 双重掩闭——设备级 IER=0 + PIC OCW1 主片 0xFE 的 bit4）
        outb(COM1 + 3, 0x80); // DLAB=1
        outb(COM1 + 0, 0x01); // divisor=1（115200；QEMU 不校验速率）
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03); // 8N1
        outb(COM1 + 2, 0xC7); // FIFO 使能 + 清
        outb(COM1 + 4, 0x0B); // RTS|DTR|OUT2
    }
}

fn serial_putc(c: u8) {
    unsafe {
        // LSR bit5 = THR 空；QEMU model 下首写即就绪，轮询为真机保真
        while inb(COM1 + 5) & 0x20 == 0 {}
        outb(COM1, c);
    }
}

fn serial_puts(s: &[u8]) {
    for &b in s {
        serial_putc(b);
    }
}

/// u32 → 小写十六进制（无前导零，调用方自带 0x 前缀）
fn put_hex32(v: u32) {
    let mut started = false;
    for i in (0..8).rev() {
        let nib = ((v >> (i * 4)) & 0xF) as u8;
        if nib != 0 || started || i == 0 {
            serial_putc(if nib < 10 { b'0' + nib } else { b'a' + nib - 10 });
            started = true;
        }
    }
}

fn put_dec(v: u32) {
    let mut buf = [0u8; 10];
    let mut i = 0;
    let mut n = v;
    loop {
        buf[i] = b'0' + (n % 10) as u8;
        i += 1;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    while i > 0 {
        i -= 1;
        serial_putc(buf[i]);
    }
}

/// u64 → 16 位定宽十六进制（dump 面：前导零保留，寄存器行可对齐/可 grep）
fn put_hex64(v: u64) {
    for i in (0..16).rev() {
        let nib = ((v >> (i * 4)) & 0xF) as u8;
        serial_putc(if nib < 10 { b'0' + nib } else { b'a' + nib - 10 });
    }
}

pub fn hlt() {
    unsafe { asm!("hlt", options(att_syntax, nomem, nostack, preserves_flags)) };
}

fn park() -> ! {
    loop {
        hlt();
    }
}

// ---------- P7-x1：8259 PIC + PIT 100Hz + 最小 IDT（ADR #41 五原语之 2/5）----------
// 盒内明确无 APIC——legacy PIC 重映射到 0x20/0x28（避开 CPU 异常向量 0..31），
// 只放行 IRQ0（PIT ch0）。异常向量 0..31 dump 面已随 P7-x2 落地（见下节）。

static TIMER_TICKS: AtomicUsize = AtomicUsize::new(0);

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;
const PIT_CH0: u16 = 0x40;
const PIT_CMD: u16 = 0x43;
const PIT_HZ: u32 = 100;
const TIMER_TICKS_GOAL: usize = 100;

/// ICW 间真实硬件需要 settle 时间（QEMU 不需要）——读 0x80 调试口当 io_wait，
/// Linux 同款惯例。
fn io_wait() {
    unsafe { inb(0x80) };
}

fn pic_init() {
    unsafe {
        // ICW1：INIT + 级联 + ICW4。注意：init 序列期间 IMR 被清零（全部 IRQ
        // 短暂敞开）——必须在 IF=0 包络内做（boot cli 至此从未 sti），且 ICW4
        // 后立即 OCW1 掩回。
        outb(PIC1_CMD, 0x11);
        io_wait();
        outb(PIC1_DATA, 0x20); // ICW2：主片向量基 0x20（IRQ0 → 向量 0x20）
        io_wait();
        outb(PIC1_DATA, 0x04); // ICW3：主片 IRQ2 接从片
        io_wait();
        outb(PIC1_DATA, 0x01); // ICW4：8086 模式
        io_wait();
        outb(PIC2_CMD, 0x11);
        io_wait();
        outb(PIC2_DATA, 0x28); // ICW2：从片向量基 0x28
        io_wait();
        outb(PIC2_DATA, 0x02); // ICW3：从片级联 id 2
        io_wait();
        outb(PIC2_DATA, 0x01); // ICW4
        io_wait();
        outb(PIC1_DATA, 0xFE); // OCW1：只放行 IRQ0（bit0=0）
        outb(PIC2_DATA, 0xFF); // 从片全掩
    }
}

fn pic_eoi_master() {
    unsafe { outb(PIC1_CMD, 0x20) }; // 非特定 EOI
}

fn pit_init() {
    // 输入时钟 1193182Hz；mode 3 方波 + lobyte/hibyte（PC 定时器传统配置 0x36）
    let div = 1_193_182u32 / PIT_HZ;
    unsafe {
        outb(PIT_CMD, 0x36);
        outb(PIT_CH0, (div & 0xFF) as u8);
        outb(PIT_CH0, ((div >> 8) & 0xFF) as u8);
    }
}

/// IRQ0 上下文（timer_isr 桩调入）。IF 已被中断门清零，无重入。
#[no_mangle]
extern "C" fn x86_timer_irq() {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    pic_eoi_master();
}

#[derive(Clone, Copy)]
#[repr(C, align(16))] // IDT 基址 16 对齐（SDM 要求至少 8；16 全覆盖）
struct GateEntry {
    off_lo: u16,
    sel: u16,
    ist: u8,
    typ: u8, // P|DPL|S|type：0x8E = 在位 + DPL0 + 中断门（type 0xE）
    off_mid: u16,
    off_hi: u32,
    zero: u32,
}

#[repr(C, packed)] // lidt 读 10 字节：u16 limit + u64 base 紧排，无 Rust 对齐垫
struct Idtr {
    limit: u16,
    base: u64,
}

// wire 形状编译期钉板（TaskContext 偏移 const assert 同款惯例）：布局错 =
// 静默错硬件交互，单侧漂移须在编译期炸响。
const _: () = assert!(core::mem::size_of::<GateEntry>() == 16); // IDT 每门 16B 步进
const _: () = assert!(core::mem::size_of::<Idtr>() == 10); // lidt 读 6+8=10B（64 位）

static mut IDT: [GateEntry; 256] = [GateEntry {
    off_lo: 0,
    sel: 0,
    ist: 0,
    typ: 0,
    off_mid: 0,
    off_hi: 0,
    zero: 0,
}; 256];

/// 门构造：sel 0x08（GDT 64 位代码段）/ typ 0x8E（在位 + DPL0 中断门——进入
/// 即清 IF 防重入）。ist：0 = 用当前 RSP（red zone 已被 builtin target 禁）；
/// 1 = 换 TSS.IST[0] 专用栈（#DF/#SS——异常处理中栈再故障不在腐化栈上三重
/// 故障，P7-x2 refuted 留档「ist=0 加固项」的 P7-x4 兑现）。
fn make_gate(isr: usize, ist: u8) -> GateEntry {
    GateEntry {
        off_lo: (isr & 0xFFFF) as u16,
        sel: 0x08,
        ist,
        typ: 0x8E,
        off_mid: ((isr >> 16) & 0xFFFF) as u16,
        off_hi: (isr >> 32) as u32,
        zero: 0,
    }
}

fn idt_init() {
    // 0..31 CPU 异常门（P7-x2）+ 0x20 定时器门（P7-x1）。32..255 无门 →
    // 触发即 #GP(13) → dump + EXC panic halt（有门可观测，三重故障路退役）。
    let stubs: [usize; 32] = [
        core::ptr::addr_of!(exc_stub_0) as usize,
        core::ptr::addr_of!(exc_stub_1) as usize,
        core::ptr::addr_of!(exc_stub_2) as usize,
        core::ptr::addr_of!(exc_stub_3) as usize,
        core::ptr::addr_of!(exc_stub_4) as usize,
        core::ptr::addr_of!(exc_stub_5) as usize,
        core::ptr::addr_of!(exc_stub_6) as usize,
        core::ptr::addr_of!(exc_stub_7) as usize,
        core::ptr::addr_of!(exc_stub_8) as usize,
        core::ptr::addr_of!(exc_stub_9) as usize,
        core::ptr::addr_of!(exc_stub_10) as usize,
        core::ptr::addr_of!(exc_stub_11) as usize,
        core::ptr::addr_of!(exc_stub_12) as usize,
        core::ptr::addr_of!(exc_stub_13) as usize,
        core::ptr::addr_of!(exc_stub_14) as usize,
        core::ptr::addr_of!(exc_stub_15) as usize,
        core::ptr::addr_of!(exc_stub_16) as usize,
        core::ptr::addr_of!(exc_stub_17) as usize,
        core::ptr::addr_of!(exc_stub_18) as usize,
        core::ptr::addr_of!(exc_stub_19) as usize,
        core::ptr::addr_of!(exc_stub_20) as usize,
        core::ptr::addr_of!(exc_stub_21) as usize,
        core::ptr::addr_of!(exc_stub_22) as usize,
        core::ptr::addr_of!(exc_stub_23) as usize,
        core::ptr::addr_of!(exc_stub_24) as usize,
        core::ptr::addr_of!(exc_stub_25) as usize,
        core::ptr::addr_of!(exc_stub_26) as usize,
        core::ptr::addr_of!(exc_stub_27) as usize,
        core::ptr::addr_of!(exc_stub_28) as usize,
        core::ptr::addr_of!(exc_stub_29) as usize,
        core::ptr::addr_of!(exc_stub_30) as usize,
        core::ptr::addr_of!(exc_stub_31) as usize,
    ];
    unsafe {
        let idt: *mut GateEntry = core::ptr::addr_of_mut!(IDT).cast();
        for (v, s) in stubs.iter().enumerate() {
            idt.add(v).write(make_gate(*s, 0));
        }
        idt.add(0x20).write(make_gate(core::ptr::addr_of!(timer_isr) as usize, 0));
        // #DF(8)/#SS(12) 挂 IST1：栈类故障自身常因栈坏——ist=0 会把 handler
        // 落回腐化栈再故障 = 三重故障静默退场；IST1 专用栈保住 dump 面。
        // （lidt 起 ist=1 门就可能真读 TSS——gdt_tss_init 必须已先跑。）
        idt.add(8).write(make_gate(stubs[8], 1));
        idt.add(12).write(make_gate(stubs[12], 1));
        let idtr = Idtr {
            limit: (core::mem::size_of::<[GateEntry; 256]>() - 1) as u16,
            base: idt as u64,
        };
        asm!(
            "lidt (%rax)",
            in("rax") (&idtr as *const Idtr),
            options(att_syntax, nostack, preserves_flags)
        );
    }
}

fn sti() {
    // sti 置 IF（改 RFLAGS）——不得标 preserves_flags
    unsafe { asm!("sti", options(att_syntax, nomem, nostack)) };
}

// ---------- P7-x2：异常 dump 面 + #UD 自证恢复（ADR #41 五原语之 3/5）----------

/// 异常帧 = exc_common 入栈布局（字段序 = 内存升序）。rsp/ss 仅 CPL3 来源
/// 有效（CPU 才压）；CPL0 来源这两槽不存在——handler 只在 cs&3 != 0 时读。
#[repr(C)]
struct ExcFrame {
    rax: u64,
    rcx: u64,
    rdx: u64,
    rbx: u64,
    rbp: u64,
    rsi: u64,
    rdi: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    vec: u64, // 桩压的向量号（非 CPU 行为）
    err: u64, // CPU 错误码（无错误码向量 = 桩压 0 占位）
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64, // 以下两字段仅 CPL3 来源有效
    ss: u64,
}

// wire 形状编译期钉板（GateEntry/Idtr 同款惯例）：15 GP（rsp 除外——它就
// 是帧锚，被中断者 rsp = 帧基 + 176 处由 CPU/桩秩序隐含）+ vec + err +
// rip/cs/rfl + rsp/ss = 22 × 8 = 176B。
const _: () = assert!(core::mem::size_of::<ExcFrame>() == 176);

const EXC_NAMES: [&str; 32] = [
    "#DE", "#DB", "NMI", "#BP", "#OF", "#BR", "#UD", "#NM", "#DF", "#CO", "#TS", "#NP", "#SS",
    "#GP", "#PF", "rsvd", "#MF", "#AC", "#MC", "#XM", "#VE", "#CP", "rsvd", "rsvd", "rsvd", "rsvd",
    "rsvd", "rsvd", "#HV", "#VC", "#SX", "rsvd",
];

static EXC_SELFTEST: AtomicBool = AtomicBool::new(false);
static EXC_RECOVERED: AtomicBool = AtomicBool::new(false);
const EXC_CANARY: u64 = 0x600d_f00d_c0de_0006; // 低 4 位 = #UD 向量号 6

fn read_cr2() -> u64 {
    let v: u64;
    unsafe { asm!("mov %cr2, {}", out(reg) v, options(att_syntax, nomem, nostack, preserves_flags)) };
    v
}

/// dump 寄存器行助手：` name=0x…`（16 位定宽 hex，见 put_hex64 注）
fn dump_reg(name: &[u8], v: u64) {
    serial_putc(b' ');
    serial_puts(name);
    serial_puts(b"=0x");
    put_hex64(v);
}

fn exc_dump(f: &ExcFrame) {
    let vec = f.vec as usize;
    let name = if vec < 32 { EXC_NAMES[vec] } else { "???" };
    serial_puts(b"EXC dump vec=");
    put_dec(vec as u32);
    serial_putc(b' ');
    serial_puts(name.as_bytes());
    serial_puts(b" err=0x");
    put_hex64(f.err);
    dump_reg(b"rip", f.rip);
    dump_reg(b"cs", f.cs);
    dump_reg(b"rfl", f.rflags);
    if vec == 14 {
        dump_reg(b"cr2", read_cr2()); // #PF：故障线性地址（ARM64 FAR 对位面）
    }
    if f.cs & 3 != 0 {
        dump_reg(b"rsp", f.rsp); // CPL3 来源才读（两槽才存在）
        dump_reg(b"ss", f.ss);
    }
    serial_puts(b"\n rax=0x");
    put_hex64(f.rax);
    dump_reg(b"rcx", f.rcx);
    dump_reg(b"rdx", f.rdx);
    dump_reg(b"rbx", f.rbx);
    serial_puts(b"\n rbp=0x");
    put_hex64(f.rbp);
    dump_reg(b"rsi", f.rsi);
    dump_reg(b"rdi", f.rdi);
    dump_reg(b"r8", f.r8);
    serial_puts(b"\n r9=0x");
    put_hex64(f.r9);
    dump_reg(b"r10", f.r10);
    dump_reg(b"r11", f.r11);
    dump_reg(b"r12", f.r12);
    serial_puts(b"\n r13=0x");
    put_hex64(f.r13);
    dump_reg(b"r14", f.r14);
    dump_reg(b"r15", f.r15);
    serial_puts(b"\n");
}

/// 异常公共入口（exc_common 桩调入，frame 映射在栈上）。v1 政策：
/// dump 落串口后——#UD 且自证 armed 且 CPL0 来源 → rip+2 恢复续行（ud2 两
/// 字节，自证专用路径）；其余一律 `EXC panic` + cli halt（真异常 loud 死，
/// smoke 快败 grep EXC panic；-no-reboot 下 QEMU 停在原地不刷日志）。
/// 自证 arm 与 ud2 间是 ~3 条直代码无分支无 #UD 源——armed 窗口内不存在
/// 「别处 #UD 被误恢复」的可达路径（release 代码零 ud2/unreachable 指令）。
#[no_mangle]
extern "C" fn x86_exception(frame: &mut ExcFrame) {
    exc_dump(frame);
    if frame.vec == 6 && frame.cs & 3 == 0 && EXC_SELFTEST.swap(false, Ordering::SeqCst) {
        frame.rip += 2; // 恢复点 = ud2 后第一条（asm 块内/编译器代码皆续行）
        EXC_RECOVERED.store(true, Ordering::SeqCst);
        return;
    }
    serial_puts(b"EXC panic\n");
    unsafe { asm!("cli", options(att_syntax, nomem, nostack)) };
    park();
}

// ---------- P7-x3：内核任务上下文切换（ADR #41 五原语之 4/5）----------
// SysV 协作式切换的最小契约集：callee-saved GP + rsp。不保存 caller-saved
// （调用边界约定本就可破坏）、不保存 XMM（builtin target -sse 软浮点，二进制
// 零 xmm 指令——P7-x1 纪律）、不保存 rflags/IF（单核协作式切换，任务间共享
// 中断使能态；timer 中断落在当前栈上、iretq 原样返回，与切换 asm 无交织）。
// ARM64 task.rs TaskContext 的 x86 对位面。

/// 字段序 = 内存升序，与 context_switch asm 的偏移逐一对齐（0..=6 × 8B）。
#[repr(C)]
struct TaskContext {
    rbx: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rsp: u64, // 保存点 = call 压入的返回地址槽——切回时 ret 弹它在此续行
}

// wire 形状编译期钉板（GateEntry/Idtr/ExcFrame 同款惯例）：布局错 = 静默
// 错切换，单侧漂移须在编译期炸响。
const _: () = assert!(core::mem::size_of::<TaskContext>() == 56);

impl TaskContext {
    const ZERO: TaskContext =
        TaskContext { rbx: 0, rbp: 0, r12: 0, r13: 0, r14: 0, r15: 0, rsp: 0 };
}

// 保存当前任务的 callee-saved + rsp 进 *cur，载入 *next 的同名并 ret。对
// 「被切走的任务」这是一次正常的函数调用往返（SysV callee-saved 契约按逻辑
// 任务成立）；对新任务，ret 从其栈顶弹入口/续行点（call 帧仿真）。rdi/rsi
// 自身 caller-saved——先读后破坏安全（ret 前无再读）。
global_asm!(
    ".section .text, \"ax\"",
    ".globl context_switch",
    ".type context_switch, @function",
    "context_switch:",
    "    movq %rbx, 0(%rdi)",
    "    movq %rbp, 8(%rdi)",
    "    movq %r12, 16(%rdi)",
    "    movq %r13, 24(%rdi)",
    "    movq %r14, 32(%rdi)",
    "    movq %r15, 40(%rdi)",
    "    movq %rsp, 48(%rdi)",
    "    movq 0(%rsi), %rbx",
    "    movq 8(%rsi), %rbp",
    "    movq 16(%rsi), %r12",
    "    movq 24(%rsi), %r13",
    "    movq 32(%rsi), %r14",
    "    movq 40(%rsi), %r15",
    "    movq 48(%rsi), %rsp",
    "    ret",
    ".size context_switch, . - context_switch",
    options(att_syntax)
);

// 伪返回地址 gadget：entry 意外 ret 的落点。用 ud2 而非写 0（P7-x3 对抗
// 审查实抓）：地址 0 被自身恒等映射覆盖（PD[0] 2MiB supervisor RW、EFER
// 无 NXE），near-ret 到 0 无异常——执行的是物理 0 的 SeaBIOS IVT 残留字节
// （53 ff 00 f0… = push rbx; incl (%rax)（rax 落映射内时静默写穿 4B）→
// lock push → #UD(6) rip=0x3），向量/rip/机理全错。gadget 是 .text 内
// 符号：#UD(6) dump rip=ctx_bad_ret 精确直指，零栈操作零寄存器假设；
// EXC_SELFTEST 未 armed 时 #UD 走 EXC panic 分支（dump + halt loud），
// smoke-x86 的 "EXC panic" 快败 grep 兜底。
global_asm!(
    ".section .text, \"ax\"",
    ".globl ctx_bad_ret",
    ".type ctx_bad_ret, @function",
    "ctx_bad_ret:",
    "    ud2",
    ".size ctx_bad_ret, . - ctx_bad_ret",
    options(att_syntax)
);

// ---------- P7-x4：SYSCALL/SYSRET 入口 + TSS 栈槽（ADR #41 五原语之 5/5）----------
// syscall 主体栈（16 对齐——movabsq 载入后 RSP≡0，4 push 32B 保持，call 边界
// SysV 对齐成立）+ IST1 专用栈（#DF/#SS 换栈目标，4KiB——dump 定长输出够用）。
global_asm!(
    ".section .bss",
    ".balign 16",
    ".globl sys_kstack",
    "sys_kstack:",
    "    .space 16384",
    ".globl sys_kstack_top",
    "sys_kstack_top:",
    ".balign 16",
    ".globl ist_stack",
    "ist_stack:",
    "    .space 4096",
    ".globl ist_stack_top",
    "ist_stack_top:",
    ".balign 8",
    ".globl sys_user_rsp",
    "sys_user_rsp:",
    "    .quad 0",
    ".globl sysret_main_rsp",
    "sysret_main_rsp:",
    "    .quad 0",
    options(att_syntax)
);

// syscall 入口（LSTAR 目标）。FMASK 已清 IF ⇒ 首指令前无中断窗；syscall 不换
// 栈——rsp 仍是用户栈，先保存再切 sys_kstack。桩帧（内存升序，[rsp] 起）：
// [nr][arg][rcx][r11]——r11/rcx 是 sysret 的返回对（rip/rflags），随帧保全。
// dispatch 是普通 SysV 调用：callee-saved 过 call 存活，用户 rbx 计数器往返
// 不丢；caller-saved 由桩帧 + dispatch 自身 prologue 自管（本桩刻意不碰）。
// DONE 哨兵（SYS_RESUME = u64::MAX）走 sys_kern_return：恢复 x86_main 现场
// （enter_user 压的 callee-saved + 其调用帧）——`cmpq $-1` 的 imm8 符号扩展
// 恰为全 1，rdi/rsi/rax 等调用残值不影响（只看 rax）。
global_asm!(
    ".section .text, \"ax\"",
    ".globl syscall_entry",
    ".type syscall_entry, @function",
    "syscall_entry:",
    "    movq %rsp, sys_user_rsp(%rip)",
    "    movabsq $sys_kstack_top, %rsp",
    "    pushq %r11",
    "    pushq %rcx",
    "    pushq %rdi",
    "    pushq %rax",
    "    cld",
    "    movq %rax, %rdi",              // nr → 首参
    "    movq 8(%rsp), %rsi",           // [rsp]=nr、[rsp+8]=arg → 次参
    "    call x86_syscall_dispatch",
    "    addq $16, %rsp",               // 弹 nr/arg
    "    popq %rcx",
    "    popq %r11",
    "    cmpq $-1, %rax",               // u64::MAX = 内核恢复哨兵
    "    je sys_kern_return",
    "    movq sys_user_rsp(%rip), %rsp",
    "    sysretq",                      // rip←rcx、rflags←r11（掩前值，IF=1）
    "sys_kern_return:",
    "    movq sysret_main_rsp(%rip), %rsp",
    "    popq %r15",
    "    popq %r14",
    "    popq %r13",
    "    popq %r12",
    "    popq %rbp",
    "    popq %rbx",
    "    sti",                          // FMASK 清了 IF——不补 sti 则 park 的 hlt 永眠
    "    ret",
    ".size syscall_entry, . - syscall_entry",
    options(att_syntax)
);

// 首跳 CPL3：rdi=用户 rip、rsi=用户 rsp。压 6 个 callee-saved——用户面/桩都
// 不保它们，没有这层，用户寄存器状态会砸 x86_main 调用帧（DONE 哨兵的恢复
// 对称面）。iretq 帧五槽（CPL3 来源 CPU 需 ss/rsp）：rflags=0x202（IF=1 +
// bit1 恒 1——用户可被 timer 打断，TSS.RSP0 换栈回内核）。
global_asm!(
    ".section .text, \"ax\"",
    ".globl enter_user",
    ".type enter_user, @function",
    "enter_user:",
    "    pushq %rbx",
    "    pushq %rbp",
    "    pushq %r12",
    "    pushq %r13",
    "    pushq %r14",
    "    pushq %r15",
    "    movq %rsp, sysret_main_rsp(%rip)",
    "    pushq $0x23",                  // ss 用户数据
    "    pushq %rsi",                   // rsp
    "    pushq $0x202",                 // rflags：IF=1 + bit1
    "    pushq $0x1B",                  // cs 用户代码 RPL3
    "    pushq %rdi",                   // rip
    "    iretq",
    ".size enter_user, . - enter_user",
    options(att_syntax)
);

// CPL3 用户自证程序（真指令、位置无关——只用寄存器+立即数+数字局部标号）。
// 拷到 0x200000 执行：128 轮 syscall(nr=1) 对拍 3i+7（回显错配 → ud2 → CPL3
// #UD → dump + EXC panic loud）；末轮 syscall(nr=2, arg=128) 走哨兵不返回。
// 零栈使用（syscall 入口先存 rsp 再换 sys_kstack、程序无 push/call）——
// rsp=0x40_0000 是授权叶排他端点（叶外 supervisor 侧），安全系于零栈使用
// 而非映射合法（见 USER_STACK_TOP 注——对抗审查实抓口径修正）。
global_asm!(
    ".section .usrprog, \"ax\"",
    ".globl user_prog_start",
    "user_prog_start:",
    "    xorl %ebx, %ebx",
    "1:  movq %rbx, %rdi",
    "    movq $1, %rax",                // nr = SYS_PING
    "    syscall",
    "    leaq 7(%rbx,%rbx,2), %rsi",    // 期望回显 = 3i+7
    "    cmpq %rsi, %rax",
    "    jne 2f",
    "    incq %rbx",
    "    cmpq $128, %rbx",
    "    jb 1b",
    "    movq %rbx, %rdi",
    "    movq $2, %rax",                // nr = SYS_DONE（不返回——哨兵走内核恢复）
    "    syscall",
    "2:  ud2",
    ".globl user_prog_end",
    "user_prog_end:",
    options(att_syntax)
);

unsafe extern "C" {
    /// # Safety
    /// 切走当前执行流（换栈 + 换 callee-saved）：cur 须指向当前任务的
    /// TaskContext（现场将存入）；next 须指向已 ctx_init 或此前保存过的
    /// TaskContext，且其背后栈内存有效。
    fn context_switch(cur: *mut TaskContext, next: *const TaskContext);
    /// 永不返回（ud2 → #UD → dump + halt）——ctx_init 伪返回槽的唯一合法落点。
    fn ctx_bad_ret() -> !;
    // P7-x4：u8 桩只取地址
    static sys_kstack_top: u8;   // syscall 主体栈顶（RSP0/IST0 共用基）
    static ist_stack_top: u8;    // IST1 专用栈顶（#DF/#SS）
    static syscall_entry: u8;    // LSTAR 目标
    static user_prog_start: u8;  // 用户自证程序 [start, end)
    static user_prog_end: u8;
    /// # Safety
    /// 首跳 CPL3（iretq 换特权级）：rip 须指向已拷贝就绪的用户代码、rsp 为
    /// 用户栈顶；返回不走正常路径——用户经 DONE 哨兵 syscall 时从
    /// sys_kern_return 恢复到本函数调用点的下一条指令。
    fn enter_user(rip: usize, rsp: usize);
}

/// 任务栈（.bss 静态，16 对齐——ret 仿真 call 入口的 ABI 前提）。16KiB =
/// boot 栈同尺寸。v1 无 guard 页：栈溢出 = 越界写 .bss 相邻区（P7-x2 对抗
/// 审查 refuted 留档「ist=0 加固项」已随 P7-x4 兑现——#DF/#SS 挂 IST1 专用
/// 栈，异常处理中栈再故障不再三重故障；guard 页仍不设：静态栈恒等映射内，
/// 动态栈/用户内存触发面 v2 再议）。
#[repr(C, align(16))]
struct TaskStack([u8; 16384]);

static mut STACK_A: TaskStack = TaskStack([0; 16384]);
static mut STACK_B: TaskStack = TaskStack([0; 16384]);
static mut MAIN_CTX: TaskContext = TaskContext::ZERO;
static mut CTX_A: TaskContext = TaskContext::ZERO;
static mut CTX_B: TaskContext = TaskContext::ZERO;

static CTX_TICKS_A: AtomicUsize = AtomicUsize::new(0);
static CTX_TICKS_B: AtomicUsize = AtomicUsize::new(0);
static CTX_FAULT: AtomicBool = AtomicBool::new(false);
const CTX_PINGS: usize = 128;

/// 新任务上下文 + 栈排布。栈顶 16B = [entry][ctx_bad_ret]：entry 槽 16 对齐
/// （top 对齐 − 16 仍对齐），context_switch 的 ret 弹 entry 后
/// rsp ≡ 8 (mod 16)——与真 call 入口同形（编译器按 SysV 入口假设生成
/// prologue）。entry 槽下方 = ctx_bad_ret（ud2 gadget）伪返回地址：entry
/// 按契约永不返回（循环/切走），意外 ret → #UD(6) dump rip=ctx_bad_ret
/// 符号直指（loud 兜底；为何不用 0 见 gadget 定义处的审查实录）。
fn ctx_init(ctx: *mut TaskContext, stack: *const TaskStack, entry: extern "C" fn()) {
    unsafe {
        let top = stack as usize + core::mem::size_of::<TaskStack>();
        let sp = (top - 16) as *mut u64; // entry 槽（16 对齐）
        sp.write_volatile(entry as usize as u64);
        sp.add(1).write_volatile(ctx_bad_ret as *const () as usize as u64); // 伪返回地址
        (*ctx) = TaskContext::ZERO;
        (*ctx).rsp = sp as u64;
    }
}

/// 任务 A：第 k 轮断言 B == k−1（严格乒乓——计数值只能来自对方上一轮的
/// 真实切换推进，不是两个独立自增），置 A = k，切 B。N 轮毕切回 MAIN。
extern "C" fn ctx_task_a() {
    for k in 1..=CTX_PINGS {
        if CTX_TICKS_B.load(Ordering::Relaxed) != k - 1 {
            CTX_FAULT.store(true, Ordering::Relaxed);
        }
        CTX_TICKS_A.store(k, Ordering::Relaxed);
        unsafe {
            context_switch(core::ptr::addr_of_mut!(CTX_A), core::ptr::addr_of!(CTX_B));
        }
    }
    unsafe {
        // 收官切回 main——本任务自此悬停在该 context_switch 内（现场已存
        // CTX_A，无人再切回）。
        context_switch(core::ptr::addr_of_mut!(CTX_A), core::ptr::addr_of!(MAIN_CTX));
    }
}

/// 任务 B：第 k 轮断言 A == k 后置 B = k 切回 A。N 轮全由 A 的循环驱动；
/// 末次切走后永不再被切回——尾部 park 兜底防裸 ret（不可达路径）。
extern "C" fn ctx_task_b() {
    for k in 1..=CTX_PINGS {
        if CTX_TICKS_A.load(Ordering::Relaxed) != k {
            CTX_FAULT.store(true, Ordering::Relaxed);
        }
        CTX_TICKS_B.store(k, Ordering::Relaxed);
        unsafe {
            context_switch(core::ptr::addr_of_mut!(CTX_B), core::ptr::addr_of!(CTX_A));
        }
    }
    park();
}

// ---------- P7-x4：MSR + TSS + syscall 语义面（ADR #41 五原语之 5/5）----------

const MSR_EFER: u32 = 0xC000_0080;
const MSR_STAR: u32 = 0xC000_0081;
const MSR_LSTAR: u32 = 0xC000_0082;
const MSR_FMASK: u32 = 0xC000_0083;
/// syscall 进入即清的 rflags 位：TF|IF|DF|IOPL|NT|AC（0x47700——**非 0x44700**，
/// 少掩 IOPL = 用户态可带 IOPL=3 做 IO）。IF 在内 = 主体天然不可中断（免锁
/// 根基的 x86 版，对齐 ARM64 侧「SVC 入口硬件置 DAIF」）。SDM 序：R11 ←
/// RFLAGS 发生在掩位**之前** ⇒ r11 存掩前值（IF=1），sysret 自 r11 恢复——
/// 「用户态可被 timer 打断」成立的关键。
const SYSRET_FMASK: u64 = 0x47700;

unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(att_syntax, nomem, nostack, preserves_flags)
        );
    }
    (lo as u64) | ((hi as u64) << 32)
}

unsafe fn wrmsr(msr: u32, val: u64) {
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") (val & 0xFFFF_FFFF) as u32,
            in("edx") ((val >> 32) & 0xFFFF_FFFF) as u32,
            options(att_syntax, nomem, nostack, preserves_flags)
        );
    }
}

/// TSS（x86_64 = 104B——**非 96B**：io_bitmap_offset@100 + 2B 尾）。rsp0 =
/// CPL3→内核的换栈目标（timer 中断/异常走它，与 syscall 主体栈同基不叠层：
/// syscall 主体 IF=0 无嵌套，中断只从用户态 IF=1 进）；ist[0] = IST1 专用栈
/// （#DF/#SS 门）。
#[repr(C, packed)]
struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist: [u64; 7],
    reserved2: [u16; 4],
    io_bitmap_offset: u16, // 104 = 指向结构尾 = 空 IOPB（CPL3 零 IO 指令）
    tail: [u8; 2],
}
const _: () = assert!(core::mem::size_of::<Tss>() == 104); // wire 形状钉板

static mut TSS: Tss = Tss {
    reserved0: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    reserved1: 0,
    ist: [0; 7],
    reserved2: [0; 4],
    io_bitmap_offset: 104,
    tail: [0; 2],
};

/// GDT 扩段重载（limit 23→55）+ TSS 描述符填装 + ltr。**必须先于 idt_init**：
/// ist=1 门在 lidt 起就可能真读 TSS，TR 未载 → #TS → 三重故障静默退场。
/// 描述符打包在 Rust 侧（勿用 GAS 符号算术）：0x67 = limit 104−1；0x89 =
/// P|DPL0|S=0|type9（64 位 available TSS）；高 quad = base>>32（恒等映射
/// < 4GiB ⇒ 恒 0）。
fn gdt_tss_init() {
    unsafe {
        let tss = core::ptr::addr_of_mut!(TSS) as usize as u64;
        core::ptr::addr_of_mut!(TSS.rsp0)
            .write_volatile(core::ptr::addr_of!(sys_kstack_top) as usize as u64);
        core::ptr::addr_of_mut!(TSS.ist[0])
            .write_volatile(core::ptr::addr_of!(ist_stack_top) as usize as u64);
        let g = core::ptr::addr_of!(gdt64) as *mut u64;
        g.add(5).write_volatile(
            0x67 | ((tss & 0xFF_FFFF) << 16) | (0x89u64 << 40) | (((tss >> 24) & 0xFF) << 56),
        );
        g.add(6).write_volatile(tss >> 32);
        let gdtr = Idtr { limit: 55, base: g as u64 };
        asm!(
            "lgdt (%rax)",
            in("rax") (&gdtr as *const Idtr),
            options(att_syntax, nostack, preserves_flags)
        );
        // ltr 不标 nomem：LTR 架构上从 GDTR+0x28 真读 16B TSS 描述符（nomem
        // 字面失实）；当前 lgdt asm 居中构成双重屏障（volatile 描述符写不可
        // 下沉过它、ltr 不可上提过它——对抗审查 refuted 留档），去标注零成本。
        asm!("ltr %ax", in("ax") 0x28u16, options(att_syntax, nostack, preserves_flags));
    }
}

// syscall 语义面 consts/statics
const SYS_PING: u64 = 1;
const SYS_DONE: u64 = 2;
const SYS_CALLS: u64 = 128;
/// DONE 返回哨兵 = u64::MAX（syscall_entry 的 cmpq $-1 据此走内核恢复）。
const SYS_RESUME: u64 = u64::MAX;
const USER_CODE_VA: usize = 0x20_0000; // [2MiB,4MiB) 2MiB 叶 = 授权用户页
/// 0x40_0000 = 授权叶 [0x200000,0x400000) 的**排他端点**——本身在叶外
/// （PD[2] supervisor 侧），**不是合法用户映射**（对抗审查实抓：勿按
/// 「只需合法映射」口径扩展本程序）。安全仅系于：user_prog 零栈使用（无
/// push/call/rsp 访问）+ syscall 入口先保存 rsp 再立即换 sys_kstack（不解
/// 引用用户 rsp）+ 栈访问必向下（即便加 push 也落 [rsp-8] 叶内）。若程序
/// 将用栈，取值须落入叶内。
const USER_STACK_TOP: usize = 0x40_0000;

static SYS_COUNT: AtomicUsize = AtomicUsize::new(0);
static SYS_USER_OK: AtomicBool = AtomicBool::new(false);

/// syscall 语义层（IF=0、sys_kstack 上；零打印——主流程恢复后统一落串口，
/// 与 ARM64 侧「syscall 面不 panic 不打印」同纪律）。
#[no_mangle]
extern "C" fn x86_syscall_dispatch(nr: u64, arg: u64) -> u64 {
    match nr {
        SYS_PING => {
            SYS_COUNT.fetch_add(1, Ordering::Relaxed);
            arg.wrapping_mul(3).wrapping_add(7)
        }
        SYS_DONE => {
            if arg == SYS_CALLS && SYS_COUNT.load(Ordering::Relaxed) == SYS_CALLS as usize {
                SYS_USER_OK.store(true, Ordering::Relaxed);
            }
            SYS_RESUME
        }
        _ => (-95i64) as u64, // -ENOTSUP：未知号永不杀调用者（ABI §8.0 最小集版）
    }
}

/// SYSCALL/SYSRET MSR 四件 + 用户页 U/S 授权。
fn sysret_init() {
    unsafe {
        // EFER：boot 只设过 LME——运行期增量置 SCE，不整写覆写。
        wrmsr(MSR_EFER, rdmsr(MSR_EFER) | 1);
        // STAR：syscall 载 CS=0x08/SS=0x10（内核）；sysret 载 CS=0x1B/SS=0x23（用户）。
        wrmsr(MSR_STAR, (0x1Bu64 << 48) | (0x08u64 << 32));
        wrmsr(MSR_LSTAR, core::ptr::addr_of!(syscall_entry) as usize as u64);
        wrmsr(MSR_FMASK, SYSRET_FMASK);
        // 用户页授权：U/S 位三级全开才可见——PML4[0]/PDPT[0]（中间级）+ PD[1]
        // （[2MiB,4MiB) 2MiB 叶：0x200083 → 0x200087）。RMW + volatile。
        let pml4_p: *mut u64 = core::ptr::addr_of!(pml4) as *mut u64;
        let pdpt_p: *mut u64 = core::ptr::addr_of!(pdpt) as *mut u64;
        let pd_p: *mut u64 = core::ptr::addr_of!(pd) as *mut u64;
        pml4_p.add(0).write_volatile(pml4_p.add(0).read_volatile() | 4);
        pdpt_p.add(0).write_volatile(pdpt_p.add(0).read_volatile() | 4);
        pd_p.add(1).write_volatile(pd_p.add(1).read_volatile() | 4);
        // 分页结构缓存可能持 supervisor-only 陈旧中间项——显式失效。
        // **勿标 nomem**（对抗审查实抓）：invlpg 与上面三个 RMW 零寄存器依赖，
        // nomem 授予编译器跨 RMW 上提许可 → 失效落空（SDM §4.10：直线取指会
        // 以旧 supervisor 项重填分页结构缓存，store 只改内存不刷缓存——CPL3
        // 取指 0x200000 即 #PF；TCG 不建模该缓存，冒烟恒绿真机才爆）。
        // aarch64 侧 mmu.rs 的 tlbi 同不标 nomem（页表写→失效同构面）。
        asm!(
            "invlpg (%rax)",
            in("rax") 0x200000usize,
            options(att_syntax, nostack, preserves_flags)
        );
    }
}

#[no_mangle]
pub extern "C" fn x86_main() -> ! {
    serial_init();
    serial_puts(b"\nBerryOS boot (x86_64)\n");

    // PVH 握手内容活证：hvm_start_info { magic u32 @0, version u32 @4 }
    let si = unsafe { core::ptr::addr_of!(pvh_start_info).read_volatile() } as usize;
    let magic = unsafe { core::ptr::read_volatile(si as *const u32) };
    let ver = unsafe { core::ptr::read_volatile((si + 4) as *const u32) };
    serial_puts(b"PVH start_info=0x");
    put_hex32(si as u32);
    serial_puts(b" magic=0x");
    put_hex32(magic);
    serial_puts(b" ver=");
    put_dec(ver);
    serial_puts(b"\n");
    if magic != HVM_START_MAGIC {
        serial_puts(b"X86 FAIL: hvm_start_info magic mismatch\n");
        park();
    }
    if ver != 1 {
        serial_puts(b"X86 FAIL: hvm_start_info version != 1\n");
        park();
    }
    serial_puts(b"PVH ok\n");

    // P7-x1：PIT 100Hz → 8259 IRQ0 → IDT 0x20 → sti 全链。收满 100 拍打标记
    // （状态谓词非竞速：拍数只能来自 handler 递增，PIT/PIC/IDT/sti 任一环断
    // 则标记缺失；恰一次由 break 保证——100 拍 ≈ 1s 后打印）。标记后中断
    // 保持敞开进 park（拍数继续涨，无害——系统持续可中断的活证）。
    pic_init();
    pit_init();
    // gdt_tss_init 必须先于 idt_init：ist=1 门（#DF/#SS）在 lidt 起就会真读
    // TSS，TR 未载 → #TS → 三重故障静默退场（P7-x4 头注「顺序约束」）。
    gdt_tss_init();
    idt_init();
    sti();
    loop {
        let n = TIMER_TICKS.load(Ordering::Relaxed);
        if n >= TIMER_TICKS_GOAL {
            serial_puts(b"TIMER ok n=");
            put_dec(n as u32);
            serial_puts(b"\n");
            break;
        }
        hlt();
    }

    // P7-x2：异常面自证——ud2 触发 #UD(6)，dump 落串口后 rip+2 恢复续行。
    // rax 金丝雀随行（exc_common 16 GP 全量入栈/弹回的机器断言面——桩漏存
    // 任一 caller-saved 且 handler 破坏它，往返即变值）；恢复判定 =
    // EXC_RECOVERED 状态谓词非竞速。asm 不标 nomem：handler 写 statics 须
    // 经编译器视野（不标 = 假定可触碰内存，后续 load 不缓存到 asm 前）。
    EXC_SELFTEST.store(true, Ordering::SeqCst);
    let mut canary = EXC_CANARY;
    unsafe { asm!("ud2", inlateout("rax") canary, options(att_syntax)) };
    if EXC_RECOVERED.load(Ordering::SeqCst) && canary == EXC_CANARY {
        serial_puts(b"EXC ok vec=6\n");
    } else {
        serial_puts(b"X86 FAIL: exc selftest (recovered/canary mismatch)\n");
        park();
    }

    // P7-x3：上下文切换自证——A/B 两内核任务在独立 .bss 栈上严格乒乓 128
    // 轮（每轮双方互验对方计数——计数只能来自真实交替切换）；毕后 A 切回
    // MAIN_CTX，此处续行（main 的 callee-saved/rsp 在首次切走时保存）。
    // 状态谓词非竞速；恰一次由单次直线驱动保证。全程中断敞开（timer ISR 落
    // 当前栈 iretq 原样返回，与切换无交织——切走时被打断的现场含在 callee-
    // saved/rsp 里，ISR 帧在其下方不触达）。
    unsafe {
        ctx_init(core::ptr::addr_of_mut!(CTX_A), core::ptr::addr_of!(STACK_A), ctx_task_a);
        ctx_init(core::ptr::addr_of_mut!(CTX_B), core::ptr::addr_of!(STACK_B), ctx_task_b);
        context_switch(core::ptr::addr_of_mut!(MAIN_CTX), core::ptr::addr_of!(CTX_A));
    }
    let na = CTX_TICKS_A.load(Ordering::Relaxed);
    let nb = CTX_TICKS_B.load(Ordering::Relaxed);
    if !CTX_FAULT.load(Ordering::Relaxed) && na == CTX_PINGS && nb == CTX_PINGS {
        serial_puts(b"CTX ok n=");
        put_dec(na as u32);
        serial_puts(b"\n");
    } else {
        serial_puts(b"X86 FAIL: ctx selftest (alternation/count mismatch)\n");
        park();
    }

    // P7-x4：SYSCALL/SYSRET + CPL3 用户程序自证——MSR 四件 + 用户页授权 →
    // user_prog 逐字节拷到 0x200000 → iretq 首跳 CPL3。返回点 = DONE 哨兵经
    // sys_kern_return 恢复到 enter_user 调用点的下一条（此处续行）。
    // 状态谓词非竞速：SYS_COUNT/SYS_USER_OK 只能来自 128 轮真实 syscall 对拍
    // （回显错配 → CPL3 #UD → dump + EXC panic loud；smoke 快败兜底）。
    sysret_init();
    unsafe {
        let src = core::ptr::addr_of!(user_prog_start) as *const u8;
        let end = core::ptr::addr_of!(user_prog_end) as *const u8;
        let dst = USER_CODE_VA as *mut u8;
        let mut p = src;
        while p < end {
            dst.add(p.offset_from(src) as usize).write_volatile(p.read_volatile());
            p = p.add(1);
        }
        enter_user(USER_CODE_VA, USER_STACK_TOP);
    }
    if SYS_USER_OK.load(Ordering::Relaxed) && SYS_COUNT.load(Ordering::Relaxed) == SYS_CALLS as usize
    {
        serial_puts(b"SYS ok n=");
        put_dec(SYS_COUNT.load(Ordering::Relaxed) as u32);
        serial_puts(b"\n");
    } else {
        serial_puts(b"X86 FAIL: sys selftest (count/flag mismatch)\n");
        park();
    }
    serial_puts(b"X86 ok\n");
    park();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_puts(b"\nx86 panic: ");
    if let Some(p) = info.location() {
        serial_puts(b"@");
        serial_puts(p.file().as_bytes());
        serial_puts(b":");
        put_dec(p.line());
    }
    serial_puts(b"\n");
    park()
}
