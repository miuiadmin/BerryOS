//! 内核内最小测试骨架（关卡 G0 判据辅助）：在 QEMU 串口打印 PASS/FAIL。
//!
//! 单核、无抢占（G0）下同步直跑；断言失败 → panic（panic handler 打消息 + file:line:col；
//! CPU 异常寄存器/PC/ESR/FAR dump 在 HAL `trap::exception_sync`）。SMP（G2）后需每核隔离。

use bos_mm::va::{phys_to_virt, PAGE_SHIFT};

pub fn run() {
    test_gic();
    test_frame();
    test_heap();
    bos_hal_aarch64::uart_puts(b"ktest done\n");
    // Step 6：异常向量 + TrapFrame save/restore/eret 全链路验证（放最后）。
    test_exc();
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
