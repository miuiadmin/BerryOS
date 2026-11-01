//! crt0 —— std 程序的用户态自举（G-eco-d：musl crt1.o/`__libc_start_main` 全免）
//!
//! 职责（`_start`，经 `process_start` 进入，`x0` = bootstrap = **env 通道端**，
//! d-3a 起协议见 io.rs `berry_libc_boot` 头注）：
//! 1. 抢救 bootstrap handle 与初始栈顶；
//! 2. `berry_tls_setup`（T-1 §1 **拷贝式 TLS**）：经 `__ehdr_start` 自走 phdr
//!    表找 PT_TLS → 静态 arena 里 `tls_place` 放置 → **显式清零整窗 + 拷
//!    模板** → `msr tpidr_el0`（TP = data−0x10，EL0 可写寄存器，零内核
//!    seam）。原位 TP 方案已删——`.tbss` 零初始化契约三方共违约（lld 标准布局
//!    + berry-exec 零 PT_TLS 感知 + 原位直用），TLS 胞元初值 = 活
//!    `.data.rel.ro` 文件字节（vtable/函数指针），见
//!    `docs/法证-TLS原位方案崩因归因.md`；
//! 3. `berry_libc_boot`：recv env 块 → console/stdin handle 落种 + **真
//!    argv**（NUL 分隔字节串 → static 数组）→ 返 argc；
//! 4. 直调 `main(argc, argv)`（argv 从 `BERRY_ARGV` 全局交汇编取址）；
//!    不归则 park。
//!
//! 跳过项（诚实缺口，非疏漏）：
//! - `.init_array` 不走——Rust 静态量经 const 求值初始化，无 C++ 构造面。
//!
//! 纯函数面（`tls_place`/`find_ptls`/`tls_template`/`TLS_ARENA_CAP`/模板
//! 四全局）自 `berry-tls` 独立纯 crate 消费并再导出（T-1 §6 回退启用，
//! berry-fsclient 先例：本 crate 的 libc 同名符号在 host 测试二进制里对
//! libSystem write 发生符号插入——panic 钩子 stderr 被 write → console_write
//! → berry_channel_send 劫持自旋，首跑实测挂死；纯面独立 crate 后 host
//! 测试零 libc 符号面）。**python/rust 双份算术义务注记**随纯面迁
//! `user/berry-tls/src/lib.rs` 与 `tools/tlsscan.py` 两侧头注。

use core::arch::{asm, global_asm};

// T-1 裁决三：TLS 模板四全局 + 纯放置/phdr 走查面（单一事实源 = berry-tls；
// 再导出给第二层 pthread 经 `berry_libc::crt0::` 消费）。
pub use berry_tls::{
    find_ptls, tls_place, tls_template, BERRY_TLS_ALIGN, BERRY_TLS_FILESZ, BERRY_TLS_MEMSZ,
    BERRY_TLS_VADDR, TLS_ARENA_CAP,
};

/// 链接种子（rlib 按需提取的活门）：`_start` 是 global_asm——不产生任何
/// 引用面，无人引用本模块时 crt0 目标件整件不进链接（实踩：lld 警告
/// `cannot find entry symbol _start`）。消费者对它取一次址（`#[used]`
/// static，见 stdhello）即把本件拉进来。本体空操作、无 ABI 面。
#[no_mangle]
pub extern "C" fn berry_crt0_anchor() {}

/// TLS 拷贝式静态 arena：.bss NOBITS 零文件字节（每 std bin PT_LOAD
/// memsz +4KiB = +1 页/进程 AS，无 cpio 文件字节增长；VMO commit 走
/// zero_frame 保零——**但不信它**：`berry_tls_setup` 显式
/// `memset_volatile` 清零整窗，法证教训一级防御）。16 对齐给 tls_place
/// 的 base 前置（align ≤ 16 时 data−base 恒 0x10）——`#[repr(align)]`
/// 只认类型，包一层 newtype（对齐随类型走，零运行期开销）。
#[repr(C, align(16))]
struct TlsArena([u8; TLS_ARENA_CAP]);

static mut TLS_ARENA: TlsArena = TlsArena([0; TLS_ARENA_CAP]);

extern "C" {
    static __ehdr_start: u8;
}

/// 走自身 ELF 头装主线程 TLS（拷贝式，T-1 §1）。
///
/// 命中 PT_TLS → 写四模板全局 → `tls_place` 成功 → `memset_volatile`
/// 显式清零整窗（memsz，**不信 .bss 零初值**——法证教训一级防御）→
/// `memcpy_volatile` 拷模板（filesz）→ `msr tpidr_el0`（TP = data−0x10）；
/// placement None → `brk #0x7C1`（boot 期唯一 loud 形态：此刻 CONSOLE_H
/// 未落种、无打印 syscall 面；brk 走内核 EXCEPTION dump + user_fault_exit
/// 串口 loud；tlsscan --check F1 门先拦使其实际不可达）；无 PT_TLS →
/// 空操作返回（四全局保持 0 初值 = ALIGN 0 哨兵）。
///
/// 前置条件：`__ehdr_start`（链接器自供，附录 A 强面在册）；本函数在 TLS
/// 就绪前运行，体内不得触碰任何 `#[thread_local]`。
#[no_mangle]
pub unsafe extern "C" fn berry_tls_setup() {
    unsafe {
        let ehdr: *const u8 = core::ptr::addr_of!(__ehdr_start);
        let Some((vaddr, filesz, memsz, align)) = find_ptls(ehdr) else {
            return; // 无 PT_TLS：空操作（bin/hello 类）
        };
        BERRY_TLS_VADDR = vaddr;
        BERRY_TLS_FILESZ = filesz;
        BERRY_TLS_MEMSZ = memsz;
        BERRY_TLS_ALIGN = align;
        let base = core::ptr::addr_of_mut!(TLS_ARENA) as *mut u8 as usize;
        match tls_place(base, TLS_ARENA_CAP, filesz, memsz, align) {
            Some(data) => {
                crate::strmem::memset_volatile(data as *mut u8, 0, memsz);
                crate::strmem::memcpy_volatile(data as *mut u8, vaddr as *const u8, filesz);
                let tp = (data - 0x10) as u64;
                asm!("msr tpidr_el0, {}", in(reg) tp);
            }
            None => asm!("brk #0x7C1"), // arena 溢出：lint 门 F1 先拦 = 实际不可达
        }
    }
}

global_asm!(
    ".globl _start",
    "_start:",
    "    mov x19, x0",        // 抢救 bootstrap handle（env 通道端）
    "    mov x20, sp",        // 初始栈顶留档（attr 谎报面数据源）
    "    bl  berry_tls_setup",
    "    mov x0, x19",
    "    mov x1, x20",
    "    bl  berry_libc_boot", // recv env 块：console/argv 落种 → x0 = argc
    "    mov x19, x0",
    "    adrp x0, BERRY_ARGV",
    "    add  x0, x0, :lo12:BERRY_ARGV",
    "    ldr  x1, [x0]",      // argv 基址（io.rs 静态数组）
    "    mov  x0, x19",
    "    bl  main",           // std 生成的 C-ABI main（内部走 lang_start）
    "    bl  berry_libc_park_after_main", // main 不归；归了也 park
);
