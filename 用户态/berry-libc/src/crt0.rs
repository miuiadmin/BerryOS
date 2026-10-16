//! crt0 —— std 程序的用户态自举（G-eco-d：musl crt1.o/`__libc_start_main` 全免）
//!
//! 职责（`_start`，经 `process_start` 进入，`x0` = bootstrap handle = console 别名）：
//! 1. 抢救 bootstrap handle 与初始栈顶；
//! 2. `berry_tls_setup`：经 `__ehdr_start` 自走 phdr 表找 PT_TLS，
//!    **用户态自设 TPIDR_EL0**（EL0 可写寄存器，零内核 seam）；静态 ELF 的
//!    init image 本就加载在 `p_vaddr` 原位（.tbss 孔洞由装载端页铺平零页
//!    覆盖），**免拷贝**，一条 `msr` 完事；
//! 3. `berry_libc_boot`：console handle 落 fd 表种子；
//! 4. 直调 `main(argc=1, argv)`（argv = 静态合成，真 argv 是 d-3 bootstrap
//!    消息协议的事）；不归则 park。
//!
//! 跳过项（诚实缺口，非疏漏）：
//! - `.init_array` 不走——Rust 静态量经 const 求值初始化，无 C++ 构造面；
//! - argv[0] 固定 `berry-std`（d-3 换真名）。
//!
//! TP 偏移口径：LLD 的 AArch64 TLSLE 重定位把 `tprel` 解成「符号地址 − TP」；
//! `TP_ADJ` 是 PT_TLS `p_vaddr` 到 TP 的修正量，**值经链接产物反汇编实证**
//! （见 `berry_tls_probe`——build 侧 lldb 一眼定常数，不猜不赌）。

use core::arch::{asm, global_asm};

use crate::c_char;

/// 链接种子（rlib 按需提取的活门）：`_start` 是 global_asm——不产生任何
/// 引用面，无人引用本模块时 crt0 目标件整件不进链接（实踩：lld 警告
/// `cannot find entry symbol _start`）。消费者对它取一次址（`#[used]`
/// static，见 stdhello）即把本件拉进来。本体空操作、无 ABI 面。
#[no_mangle]
pub extern "C" fn berry_crt0_anchor() {}

// —— argv 静态合成面（d-1：argv0 固定；d-3 bootstrap 消息协议换真值）——

/// argv[0] 字面量（`_start` 在栈上拼 `[argv0, NULL]` 传 main——真 crt0 同形，
/// 免去裸指针进 static 的 Sync 面）。
#[no_mangle]
static BERRY_ARGV0: [c_char; 10] = *b"berry-std\0";

// TLS 探针不在 shim 侧：`#[thread_local]` 在 no_std crate 面有 feature gate，
// 且要测的本来就是 **std 自己** 的 TLS codegen 惯例——探针在 stdhello
// （`thread_local!` + `#[no_mangle]` 存取函数），build 侧反汇编该函数读
// `mrs tpidr_el0` 后的加法常数，对照探针符号地址与 PT_TLS p_vaddr 定
// `TP_ADJ`（见本文件头注「TP 偏移口径」）。

// TP = PT_TLS p_vaddr + TP_ADJ。**实证值 −0x10**（2026-10，stdhello audit
// 构建）：std_tls_probe 反汇编读 PROBE（.tdata st_value=0）得
// `mrs tpidr_el0; add #0x10` —— lld 的 AArch64 TLSLE 惯例是
// tprel = st_value + 0x10（TP 上方留 2 字 TCB 洞，musl/glibc aarch64 同
// 形）⟹ TP = 段首 − 0x10。
const TP_ADJ: i64 = -0x10;

/// 走自身 ELF 头找 PT_TLS 并设 TPIDR_EL0（无 TLS 段则空操作）。
///
/// 前置条件：`__ehdr_start`（链接器自供，附录 A 强面在册）；本函数在 TLS
/// 就绪前运行，体内不得触碰任何 `#[thread_local]`。
#[no_mangle]
pub unsafe extern "C" fn berry_tls_setup() {
    unsafe {
        let ehdr: *const u8 = core::ptr::addr_of!(__ehdr_start);
        let phoff = core::ptr::read_unaligned(ehdr.add(0x20) as *const u64) as usize;
        let phentsize = core::ptr::read_unaligned(ehdr.add(0x36) as *const u16) as usize;
        let phnum = core::ptr::read_unaligned(ehdr.add(0x38) as *const u16) as usize;
        for i in 0..phnum {
            let ph = ehdr.add(phoff + i * phentsize);
            // ELF64 Phdr: p_type@0 p_flags@4 p_offset@8 p_vaddr@0x10 p_filesz@0x20 p_memsz@0x28
            if core::ptr::read_unaligned(ph as *const u32) == 7 {
                // PT_TLS
                let vaddr = core::ptr::read_unaligned(ph.add(0x10) as *const u64) as i64;
                let tp = (vaddr + TP_ADJ) as u64;
                asm!("msr tpidr_el0, {}", in(reg) tp);
                return;
            }
        }
    }
}

extern "C" {
    static __ehdr_start: u8;
}

global_asm!(
    ".globl _start",
    "_start:",
    "    mov x19, x0",        // 抢救 bootstrap handle（console 别名）
    "    mov x20, sp",        // 初始栈顶留档（attr 谎报面数据源）
    "    bl  berry_tls_setup",
    "    mov x0, x19",
    "    mov x1, x20",
    "    bl  berry_libc_boot",
    "    adrp x0, BERRY_ARGV0",
    "    add  x0, x0, :lo12:BERRY_ARGV0",
    "    stp  x0, xzr, [sp, #-16]!", // 栈上拼 argv = [argv0, NULL]（真 crt0 同形）
    "    mov  x1, sp",
    "    mov  x0, #1",         // argc = 1
    "    bl  main",           // std 生成的 C-ABI main（内部走 lang_start）
    "    bl  berry_libc_park_after_main", // main 不归；归了也 park
);
