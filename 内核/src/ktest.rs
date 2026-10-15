//! 内核内最小测试骨架（关卡 G0 判据辅助）：在 QEMU 串口打印 PASS/FAIL。
//!
//! 单核、无抢占（G0）下同步直跑；断言失败 → panic（panic handler 打消息 + file:line:col；
//! CPU 异常寄存器/PC/ESR/FAR dump 在 HAL `trap::exception_sync`）。SMP（G2）后需每核隔离。

use bos_mm::va::{phys_to_virt, PAGE_SHIFT};

pub fn run() {
    test_gic();
    test_frame();
    test_heap();
    test_syscall();
    test_switch();
    test_aspace();
    bos_hal_aarch64::uart_puts(b"ktest done\n");
    // Step 6：异常向量 + TrapFrame save/restore/eret 全链路验证（放最后）。
    test_exc();
}

/// Step 9 信号 + G0 退出 #6：从 EL1 发 `svc #0`（EC=0x15，current-EL sync）→
/// dispatch 回写 x0=ret、eret 续行。断言 ret==arg 证 syscall 往返。
/// DAIF 屏蔽不影响同步异常；NZCV 经 SPSR 在 entry/eret 保存/恢复。
fn test_syscall() {
    let nr: u64 = bos_hal_aarch64::syscall::NR_TEST_ECHO as u64;
    let arg: u64 = 0x5a;
    let ret: u64;
    // SAFETY: svc #0 触发同步异常；dispatch 写回 x0、eret 回本指令之后。
    unsafe {
        core::arch::asm!(
            "svc #0",
            inout("x0") nr => ret,
            in("x1") arg,
            options(nostack, preserves_flags),
        );
    }
    assert_eq!(ret, arg, "syscall echo 往返");
    bos_hal_aarch64::uart_puts(b"SYSCALL ok\n");
}

/// Step 7 信号：GICv2 配置读回自检（IRQ27/CNTV：启用 + 优先级 + distributor/CPUif 开）。
fn test_gic() {
    assert!(bos_hal_aarch64::gic::verify(), "GIC IRQ27 配置读回不符");
    bos_hal_aarch64::uart_puts(b"GIC ok\n");
}

/// Step 6 信号：注入 `brk` → 同步异常（current-EL SPx @0x200）→ exception_sync
/// dump 寄存器/PC/ESR/FAR → BRK 可恢复（ELR+=4）→ eret 返回。
/// 见 `ktest done` 之后的 `=== EXCEPTION ... BRK recover ... [exc] brk recovered`。
fn test_exc() {
    bos_hal_aarch64::uart_puts(b"\n[exc] inject brk #0x42\n");
    // SAFETY: brk 触发同步异常（EC=0x3C）；exception_sync dump 后跳过本指令、eret 返回。
    // DAIF 屏蔽不影响同步异常（brk 非 IRQ）。
    unsafe { core::arch::asm!("brk #0x42", options(nostack, preserves_flags)) };
    bos_hal_aarch64::uart_puts(b"[exc] brk recovered (eret ok)\n");
}

/// 帧分配器往返 + 读写哨兵 + 回收 + 合并。
fn test_frame() {
    let f = crate::mm::frames();

    // 1) 分配一页，写哨兵再读回（身份映射 + Normal 映射下应一致）。
    let p1 = f.alloc().expect("FRAME alloc p1");
    let v1 = phys_to_virt(p1 << PAGE_SHIFT) as *mut u64;
    unsafe {
        v1.write_volatile(0xDEAD_BEEF_CAFE_F00D);
        assert_eq!(
            v1.read_volatile(),
            0xDEAD_BEEF_CAFE_F00D,
            "FRAME 哨兵读写不符"
        );
    }

    // 2) 再分配一页（应与 p1 不同）。
    let p2 = f.alloc().expect("FRAME alloc p2");
    assert_ne!(p2, p1, "FRAME 分配出重复页");

    // 3) 释放（free 不报错 = 页归还池）。buddy 的选页顺序是实现策略——「碎片优先」
    //    （先吃小空闲块、保留大连续块以减碎片），故 free 后不保证立即复用同一页，
    //    只保证「可继续分配 + 能切大块」（见 4）。这也正是 buddy 区别于 bump 分配之处。
    f.free(p1, 0).expect("FRAME free p1");
    f.free(p2, 0).expect("FRAME free p2");

    // 4) 合并：全释放后应能切出 32 页的大块——证明 free 真正归还 + 级联合并无碎片/泄漏。
    let big = f.alloc_order(5).expect("FRAME alloc_order(5) 合并失败");
    f.free(big, 5).expect("FRAME free big");

    bos_hal_aarch64::uart_puts(b"FRAME ok\n");
}

/// 内核堆（`alloc` crate / slab）：slab 路径（Vec 增长 + realloc）+ 整页路径（> 2KiB）。
fn test_heap() {
    use alloc::{boxed::Box, vec::Vec};

    // slab 路径：连续 push 触发多次 realloc（小→大，最终跨过 MAX_CLASS 走整页）。
    let mut v: Vec<u64> = Vec::new();
    for i in 0..1024u64 {
        v.push(i);
    }
    assert_eq!(v.iter().sum::<u64>(), (0..1024u64).sum::<u64>(), "Vec 内容损坏");

    // 整页路径：4KiB（> MAX_CLASS=2048）→ 1 页；8KiB → 2 页。
    let big: Box<[u8; 4096]> = Box::new([0u8; 4096]);
    assert_eq!(big.len(), 4096);
    let big2: Box<[u64; 1024]> = Box::new([0u64; 1024]);
    assert_eq!(big2.iter().filter(|&&x| x == 0).count(), 1024, "8KiB 块内容不符");

    // 作用域结束：big/big2/v 释放（slab 回收槽、整页归还 buddy）。
    drop(v);
    drop(big);
    drop(big2);

    bos_hal_aarch64::uart_puts(b"HEAP ok\n");
}

// ---- Phase 2a Step 1：上下文切换 asm 乒乓（无调度器、无 IRQ）----

use bos_hal_aarch64::task::TaskContext;

/// 16 对齐的 4KiB 内核栈（`new_kernel` 要求栈顶 16 对齐——风险 R2）。
#[repr(C, align(16))]
struct TestStack4096([u8; 4096]);

/// MAIN 的上下文：首切出时由 `context_switch` 把当前 callee-saved+sp 存进来。
/// 初始 ZERO（内容无所谓，被首次 switch 覆盖）。
static mut SWITCH_CTX_MAIN: TaskContext = TaskContext::ZERO;
/// OTHER 的上下文：`new_kernel` 预设指向 `switch_ping_fn`。
static mut SWITCH_CTX_OTHER: TaskContext = TaskContext::ZERO;
static mut SWITCH_OTHER_STACK: TestStack4096 = TestStack4096([0; 4096]);

/// 乒乓对端：打 `PING ` 后切回 MAIN。Step 1 不会被切回（MAIN 不再切它），故以
/// `halt()` 收尾满足 `-> !`。
extern "C" fn switch_ping_fn(_arg: usize) -> ! {
    // SAFETY: 当前 DAIF 屏蔽（boot 起 IRQ 未解蔽，ktest 在 enable_irq 之前），
    // 满足 context_switch 的 `PSTATE.I==1` 不变式。两 ctx 不别名。
    unsafe {
        bos_hal_aarch64::uart_puts(b"PING ");
        bos_hal_aarch64::task::context_switch(
            &raw mut SWITCH_CTX_OTHER,
            &raw const SWITCH_CTX_MAIN,
        );
    }
    bos_hal_aarch64::halt();
}

/// Step 1 信号：构造 OTHER(`new_kernel` 指向 `switch_ping_fn`) → 从 MAIN 切到 OTHER
/// → OTHER 打 `PING ` 切回 → 这里续行打 `TASK ok`。证 switch asm 存/取 callee-saved
/// + sp、新任务首切经 trampoline 接线正确（风险 R1）。
fn test_switch() {
    // SAFETY: DAIF 屏蔽（在 enable_irq 之前）；两 ctx 静态、不别名；栈顶 16 对齐。
    unsafe {
        let stack_top =
            (&raw const SWITCH_OTHER_STACK) as *const u8 as usize + core::mem::size_of::<TestStack4096>();
        SWITCH_CTX_OTHER = TaskContext::new_kernel(switch_ping_fn, 0, stack_top);
        // 从当前（MAIN）切到 OTHER：当前 callee-saved+sp 落 SWITCH_CTX_MAIN，
        // 载 SWITCH_CTX_OTHER → 经 trampoline 进 switch_ping_fn。
        bos_hal_aarch64::task::context_switch(
            &raw mut SWITCH_CTX_MAIN,
            &raw const SWITCH_CTX_OTHER,
        );
        // <- switch_ping_fn 切回 SWITCH_CTX_MAIN 时续行于此。
    }
    bos_hal_aarch64::uart_puts(b"TASK ok\n");
}

/// 2b-2：每进程页表 + ASID 互切 + 低半退役正向断言（`内存管理设计.md` §5.1 第 3 步）。
/// 低半退役本身在 `rust_main` 早期已完成（`retire_low_half`），本用例验证其效果。
fn test_aspace() {
    use crate::aspace::MiniAspace;
    use bos_hal_aarch64 as hal;

    const MAGIC_A: u64 = 0x00A5_C0DE_0000_0001;
    const MAGIC_B: u64 = 0x00B6_C0DE_0000_0002;

    // 关卡记录（§5.2）：ASID 实测宽度（QEMU virt/cortex-a72）。
    let width = hal::asid_bits();

    let a = MiniAspace::new(crate::mm::frames(), crate::aspace::alloc_asid(), MAGIC_A);
    let b = MiniAspace::new(crate::mm::frames(), crate::aspace::alloc_asid(), MAGIC_B);

    // 激活 A：读到自己的 magic；写入改值、读回。
    a.activate();
    assert_eq!(a.read_va(), MAGIC_A, "ASPACE：A 初值不符");
    a.write_va(0x00A5_DEAD_0000_0011);
    assert_eq!(a.read_va(), 0x00A5_DEAD_0000_0011, "ASPACE：A 写读不符");

    // 切 B：**同一 VA** 读到 B 的 magic（≠ A 刚写的值）→ 两表隔离；B 写自己的值。
    b.activate();
    assert_eq!(b.read_va(), MAGIC_B, "ASPACE：B 下同 VA 应是 B 的内容（隔离）");
    b.write_va(0x00B6_DEAD_0000_0022);

    // 切回 A：A 的写入保留 → 切换不破坏对端表。
    a.activate();
    assert_eq!(a.read_va(), 0x00A5_DEAD_0000_0011, "ASPACE：切回 A 后 A 的写入须保留");

    // 低半退役**正向断言**：读已退役的身份地址必须 fault 且可恢复。
    // arm 期望故障 → ldr（若低半仍有映射则不 fault → 槽仍武装 → 断言失败）。
    hal::trap::arm_expected_fault(bos_hal_aarch64::RAM_BASE);
    // SAFETY: 预期同步异常（期望故障槽已 arm，FAR 命中即恢复）；值本身无意义。
    let _ = unsafe { core::ptr::read_volatile(bos_hal_aarch64::RAM_BASE as *const u64) };
    assert!(
        !hal::trap::fault_armed(),
        "ASPACE：低半读居然没 fault——身份映射没退役干净"
    );

    // 收尾：回内核空表（与 idle/无 AS 任务的 ttbr0=0 状态对齐）。
    hal::activate_aspace(0, 0);

    bos_hal_aarch64::uart_puts(b"ASPACE ok (asid=");
    bos_hal_aarch64::uart_put_dec(width as u64);
    bos_hal_aarch64::uart_puts(b"b, low-half faulted)\n");
}
