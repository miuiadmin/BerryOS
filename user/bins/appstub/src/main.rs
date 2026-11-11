//! appstub —— 桌面演示 app 的共用 no_std 静默壳（GUI-8 视觉补全）。
//!
//! 每个 `apps/<ID>/manifest.toml` 复用同一 ELF 为 `bin/<name>`；本壳
//! 消费 env 块后立即 `berry_process_exit(0)`。零 std/零输出 = 单包体
//! ~数 KB，8 个占位 app 不破 rootfs 32MiB 帽。真 app 内容后续替换。

#![no_std]
#![no_main]

use berry_sys::{berry_channel_recv, berry_channel_send, berry_process_exit, Handle};

/// env 窗：argv0 + base/extras 全收（只清队列，不消费语义）。
#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env: Handle) -> ! {
    let mut blob = [0u8; 192];
    let mut nb = blob.len();
    let mut hs = [0 as Handle; 8];
    let mut nh = hs.len();
    let _ = berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh);
    // appd 收口契约：console=false 的 stdout 需零长 EOF（libc exit 族同款）。
    // 启动期队列可能瞬时饱和——EBUSY 自旋重试，其余错留给进程退出兜底。
    if nh > 0 {
        loop {
            let s = berry_channel_send(hs[0], core::ptr::null(), 0, core::ptr::null(), 0);
            if s == 0 || s != berry_sys::BERRY_ERR_BUSY {
                break;
            }
        }
    }
    berry_process_exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_process_exit(134)
}
