//! G-eco-a 首个编译用户程序——替换 `内核/src/user.rs` 的 abuser asm 模板。
//!
//! 契约与原 `global_asm!` 块逐字对齐：
//! - 入口 x0 = console 发端（boot 首插 0x0001_0001，`spawn_user` 传入）；
//! - 节流（~3.1M 计数，别让 console 排队）→ `handle_close(0xDEAD_BEEF)`
//!   必须 -EBADF（真号 + 坏值 = 能力门控拒绝面 + 3-1「永不杀调用者」活证）；
//! - 'S' 经 `channel_send`（-EBUSY = console 消费慢，全参重试）；
//! - 任何返回值异常 → 自陷（读未映射地址被杀，watchdog 错杀断言拦下）。
//!
//! panic 同样走自陷：契约外路径要响亮地死，不静默挂起。

#![no_std]
#![no_main]

use berry_sys::{berry_channel_send, berry_handle_close, Handle, BERRY_ERR_BAD_HANDLE, BERRY_ERR_BUSY};

/// 节流阈值（原模板 `movz x9,#0x30,lsl16`，同量级）。
const THROTTLE: u64 = 0x30_0000;

/// 垃圾 handle（close 恒 -EBADF）。
const GARBAGE: Handle = 0xDEAD_BEEF;

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(console: Handle) -> ! {
    loop {
        throttle();
        if berry_handle_close(GARBAGE) != BERRY_ERR_BAD_HANDLE {
            trap(); // 返回值异常 = 能力门控链断，自杀暴露
        }
        let b = [b'S'];
        loop {
            let s = berry_channel_send(console, b.as_ptr(), 1, core::ptr::null(), 0);
            if s == 0 {
                break;
            }
            if s != BERRY_ERR_BUSY {
                trap(); // EBUSY = console 消费慢 → 重试；其余异常自杀
            }
        }
    }
}

#[inline(never)]
fn throttle() {
    let mut i: u64 = 0;
    while i < THROTTLE {
        core::hint::black_box(&mut i);
        i += 1;
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己。
#[cold]
fn trap() -> ! {
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    trap()
}
