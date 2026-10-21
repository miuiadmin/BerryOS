// P7-x0（ADR #41 x86_64 最小集第一刀）：PVH 引导 + 32→64 长模式 + COM1 串口。
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
// 非仅「串口能出字」）+ `X86 ok`。
//
// 范围盒子（ADR #41）：本刀只有串口；定时器（PIT+8259，无 APIC）/异常向量/
// 上下文切换/SVC 入口随后续切片。SMP/ACPI/PCIe 永不进 v1 盒。

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

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

extern "C" {
    static pvh_start_info: u32;
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
        outb(COM1 + 1, 0x00); // 关串口中断（本刀无 IRQ 面）
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
