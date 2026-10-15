//! 首个 EL0 用户进程（2b-4 收官步 + G1.5 隔离关卡，`内存管理设计.md` §5.1 第 5 步）。
//!
//! 用户程序 = 内核 rodata 里的机器码模板（`global_asm!` 生成，boot 期整块拷进 VMO
//! 代码页后执行——程序内分支全为短距局部跳转/`movz` 绝对地址，无 PC 相对越界引用，
//! 整块搬迁不破坏寻址）。三个角色：
//! - `good`：节流循环——读私有数据页 canary（非零才继续）+ `NR_USER_PUTCHAR` 打 'u'
//!   + `NR_TEST_ECHO` 自检（坏即故意自陷 wild 读 → 被杀 → watchdog 见杀数>期望 →
//!   G1.5 FAIL 暴露）；
//! - `rogue_ptr`：读未映射 VA 0x4000_0000（D-abort from EL0 → 杀）；
//! - `rogue_svc`：调越界编号 0x99 的 SVC（v1 政策：EL0 畸形 syscall → 杀）。
//! 两个 rogue 应被杀且**只死自己**（`user_fault_exit`）；[`watchdog`] 内核任务断言
//! 杀数==2 + switches/putchar 金丝雀仍在涨 → 打 `G1.5 ok`（smoke 固化标记）后退出。

use crate::aspace::alloc_asid;
use crate::vmar::{Perm, Vmar};
use crate::vmo::Vmo;
use bos_mm::frame::FrameAllocator;
use bos_mm::va::PAGE_SIZE;

/// 用户 VA 布局（每个用户进程同构）：code @0x1000、data @0x2000、stack @
/// [0x3000, 0x4000)。全 UserRw、可执行（无 NX——v2 批次开，见安全设计）。
pub const USER_CODE_VA: usize = 0x1000;
pub const USER_DATA_VA: usize = 0x2000;
pub const USER_STACK_BASE: usize = 0x3000;
pub const USER_STACK_TOP: usize = 0x4000; // sp0（16 对齐）

/// 数据页 canary（非零——good 的「页内容正确」自检依据）。
pub const USER_MAGIC: u64 = 0x00C0_FFEe_E000_0E10;

// ---- 用户程序机器码模板（global_asm 生成；ARM64）----
//
// 约定：入口 x0=arg；good 的 arg = 打印节流迭代数。数字标签（0:/1:/3:）均为
// 函数内短距跳转，模板搬迁后依然成立。
core::arch::global_asm!(
    ".section .rodata",
    ".balign 16",
    ".globl __user_good",
    "__user_good:",
    "    mov  x9, x0",                 // x9 常驻：节流阈值（x0 会被 svc 返回值覆写）
    "0:  mov  x10, xzr",               // 迭代计数
    "1:  add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 1b",
    "    movz x3, #0x2000",            // 数据页 canary
    "    ldr  x4, [x3]",
    "    cbz  x4, 3f",                 // 页内容坏 → 自陷路径（被杀 = FAIL 暴露）
    "    mov  x0, #0x61",              // NR_TEST_ECHO
    "    mov  x1, #0x5a",
    "    svc  #0",                     // x0 = echo(0x5a)
    "    cmp  x0, #0x5a",
    "    b.ne 3f",                     // 往返坏 → 自陷路径
    "    mov  x0, #0x62",              // NR_USER_PUTCHAR
    "    mov  x1, #0x75",              // 'u'
    "    svc  #0",
    "    cbnz x0, 3f",                 // putchar 失败 → 自陷路径
    "    b    0b",
    "3:  movz x4, #0x4000, lsl #16",   // 0x4000_0000 未映射 → D-abort → 杀
    "    ldr  x5, [x4]",
    "    b    .",
    ".globl __user_good_end",
    "__user_good_end:",

    ".balign 16",
    ".globl __user_rogue_ptr",
    "__user_rogue_ptr:",
    "    movz x4, #0x4000, lsl #16",   // 野指针读 → D-abort(lower) → 杀
    "    ldr  x5, [x4]",
    "    b    .",                      // 不应到达（到达 = kill 路径坏）
    ".globl __user_rogue_ptr_end",
    "__user_rogue_ptr_end:",

    ".balign 16",
    ".globl __user_rogue_svc",
    "__user_rogue_svc:",
    "    mov  x0, #0x99",              // 越界 nr → v1 政策：EL0 畸形 syscall → 杀
    "    svc  #0",
    "    b    .",
    ".globl __user_rogue_svc_end",
    "__user_rogue_svc_end:",
);

extern "C" {
    static __user_good: u8;
    static __user_good_end: u8;
    static __user_rogue_ptr: u8;
    static __user_rogue_ptr_end: u8;
    static __user_rogue_svc: u8;
    static __user_rogue_svc_end: u8;
}

/// 取 `[start,end)` 机器码模板。
///
/// # Safety
/// start/end 须来自同一 `global_asm!` 块的连续指令序列（模板整块拷贝语义成立）。
unsafe fn blob(start: *const u8, end: *const u8) -> &'static [u8] {
    core::slice::from_raw_parts(start, end as usize - start as usize)
}

/// good 用户程序模板。
pub fn user_good_code() -> &'static [u8] {
    // SAFETY: 同一 global_asm 块内连续。
    unsafe { blob(&__user_good, &__user_good_end) }
}

/// rogue_ptr 用户程序模板。
pub fn user_rogue_ptr_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_rogue_ptr, &__user_rogue_ptr_end) }
}

/// rogue_svc 用户程序模板。
pub fn user_rogue_svc_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_rogue_svc, &__user_rogue_svc_end) }
}

/// 一个用户进程的装配结果：表根 + ASID（交给 `spawn_user`）。
///
/// v1 对象常驻不回收（VMO/Vmar/栈帧均不退——Phase 3 能力层统一对象销毁）；
/// 拥有关系的 Vmo/Vmar 以泄漏代持。
pub struct UserProcess {
    pub ttbr0_pa: usize,
    pub asid: u16,
}

impl UserProcess {
    /// 建一个用户进程：root Vmar + code/data/stack 三页（全 `UserRw`）+ 拷代码 +
    /// 写数据页魔数。栈页全零（sp0 自顶向下）。
    pub fn build(fa: &mut FrameAllocator, code: &[u8], data_magic: u64) -> Self {
        assert!(code.len() <= PAGE_SIZE, "用户代码模板须单页");
        assert!(code.len() % 4 == 0, "用户代码模板须 4 字节对齐长度");
        let asid = alloc_asid();
        let mut vmar = Vmar::new_root(fa, asid);

        let mut code_vmo = Vmo::create(PAGE_SIZE);
        code_vmo.commit(fa, 0, PAGE_SIZE).expect("user: commit code");
        code_vmo.write(0, code).expect("user: 拷代码");

        let mut data_vmo = Vmo::create(PAGE_SIZE);
        data_vmo.commit(fa, 0, PAGE_SIZE).expect("user: commit data");
        data_vmo.write(0, &data_magic.to_le_bytes()).expect("user: 数据页魔数");

        let mut stack_vmo = Vmo::create(PAGE_SIZE);
        stack_vmo.commit(fa, 0, PAGE_SIZE).expect("user: commit stack");

        vmar.map(fa, &code_vmo, USER_CODE_VA, 0, PAGE_SIZE, Perm::UserRw)
            .expect("user: map code");
        vmar.map(fa, &data_vmo, USER_DATA_VA, 0, PAGE_SIZE, Perm::UserRw)
            .expect("user: map data");
        vmar.map(fa, &stack_vmo, USER_STACK_BASE, 0, PAGE_SIZE, Perm::UserRw)
            .expect("user: map stack");

        // vmar 与三个 vmo 泄漏持有（v1 不回收）；表树对页表走查可见（map 内 dsb ish）。
        Self { ttbr0_pa: vmar.ttbr0_pa(), asid }
    }
}

/// G1.5 验收看门狗（内核任务）：等两个 rogue 被杀、good 的 putchar 与 switches
/// 金丝雀仍在涨 → 打关卡标记 `G1.5 ok` 后 `exit_current` 退场（A/B/good 继续
/// 分时）。杀数超期望（好用户被误杀）或超时未达成 → panic（系统停 = 关卡失败）。
pub extern "C" fn watchdog(_arg: usize) -> ! {
    const TRIES: u32 = 2_000;
    let mut last_puts = bos_hal_aarch64::syscall::user_putcnt();
    let mut last_switches = bos_sched::switches();
    for _ in 0..TRIES {
        // 忙等一拍（无 sleep 原语——Phase 4 通知落地前的土法；轮到别的任务跑）。
        for _ in 0..200_000 {
            core::hint::spin_loop();
        }
        let kills = bos_sched::user_kills();
        let puts = bos_hal_aarch64::syscall::user_putcnt();
        let switches = bos_sched::switches();
        if kills > 2 {
            panic!("G1.5 FAIL: 杀数 {} > 2（好用户或内核任务被误杀）", kills);
        }
        if kills == 2 && puts > last_puts && switches > last_switches {
            bos_hal_aarch64::uart_puts(b"\nG1.5 ok (2 rogues dead, canaries alive)\n");
            bos_sched::exit_current()
        }
        last_puts = puts;
        last_switches = switches;
    }
    panic!("G1.5 FAIL: 超时（kills={} puts={}）", bos_sched::user_kills(), last_puts);
}
