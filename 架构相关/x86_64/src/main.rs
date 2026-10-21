// P7-x0（ADR #41 x86_64 最小集第一刀）：PVH 引导 + 32→64 长模式 + COM1 串口。
// P7-x1（第二刀）：PIT 100Hz + 8259 PIC 重映射 + 最小 IDT——五原语之 2/5 定时器。
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
// 拍数只能来自 handler 递增，内容活证）+ `X86 ok`。
//
// 范围盒子（ADR #41）：本刀串口 + 定时器；异常向量（0..31 dump 面）/上下文切换/
// SVC 入口随后续切片——本刀 IDT 只装 0x20 一门，意外向量无门 → #GP → 三重故障
// → -no-reboot 直接退出（loud）。SMP/ACPI/PCIe/APIC 永不进 v1 盒。

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicUsize, Ordering};

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
global_asm!(
    ".section .rodata",
    ".balign 16",
    "gdt64:",
    "    .quad 0",                    // 0x00：空描述符
    "    .quad 0x00AF9A000000FFFF",  // 0x08：64 位代码（L=1, D=0, P=1, exec）
    "    .quad 0x00CF92000000FFFF",  // 0x10：数据（RW, P=1）
    ".balign 8",
    "gdtr64:",
    "    .word 23",                   // limit = 3 段 × 8 - 1
    "    .long gdt64",                // 32 位 lgdt 只取 6 字节（word limit + dword base）
    ".balign 4096",
    "pml4:",
    "    .zero 4096",
    "pdpt:",
    "    .zero 4096",
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

extern "C" {
    static pvh_start_info: u32;
    static timer_isr: u8; // global_asm 定义（u8 桩只取地址，永不读值）
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
// 只放行 IRQ0（PIT ch0）。异常向量全表（0..31 dump 面）是下一原语，本刀只装
// 0x20 一门——意外向量无门 → #GP → 三重故障 → -no-reboot 直接退出（loud）。

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

fn idt_init() {
    let isr = core::ptr::addr_of!(timer_isr) as usize;
    unsafe {
        let idt: *mut GateEntry = core::ptr::addr_of_mut!(IDT).cast();
        idt.add(0x20).write(GateEntry {
            off_lo: (isr & 0xFFFF) as u16,
            sel: 0x08, // GDT 0x08 = 64 位代码段（boot 弹床 lgdt 的 gdt64）
            ist: 0,    // 用当前 RSP（内核同栈；red zone 已被 builtin target 禁——见 .cargo/config.toml 实证注）
            typ: 0x8E,
            off_mid: ((isr >> 16) & 0xFFFF) as u16,
            off_hi: (isr >> 32) as u32,
            zero: 0,
        });
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
