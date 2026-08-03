//! 内核内最小测试骨架（关卡 G0 判据辅助）：在 QEMU 串口打印 PASS/FAIL。
//!
//! 单核、无抢占（G0）下同步直跑；断言失败 → panic（panic handler 现仅打 `PANIC`，
//! Step 6 升级为寄存器/PC/ESR/FAR dump + 回溯）。SMP（G2）后需每核隔离。

use bos_mm::va::{phys_to_virt, PAGE_SHIFT};

pub fn run() {
    test_frame();
    bos_hal_aarch64::uart_puts(b"ktest done\n");
}

/// 帧分配器往返 + 读写哨兵 + 复用 + 合并。
fn test_frame() {
    let f = crate::mm::frames();

    // 1) 分配一页，写哨兵再读回（身份映射：MMU 关时 VA==PA，直写物理内存）。
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

    // 2) 再分配一页（应不同于 p1）。
    let p2 = f.alloc().expect("FRAME alloc p2");
    assert_ne!(p2, p1, "FRAME 分配出重复页");

    // 3) 释放 p1 后再分配应复用 p1（最低空闲优先）。
    f.free(p1, 0).expect("FRAME free p1");
    let p3 = f.alloc().expect("FRAME realloc");
    assert_eq!(p3, p1, "FRAME realloc 未复用 p1");

    // 4) 合并：全释放后应能切出大块（证明无碎片/泄漏）。
    f.free(p2, 0).expect("FRAME free p2");
    f.free(p3, 0).expect("FRAME free p3");
    let big = f.alloc_order(5).expect("FRAME alloc_order(5) 合并失败");
    f.free(big, 5).expect("FRAME free big");

    bos_hal_aarch64::uart_puts(b"FRAME ok\n");
}
