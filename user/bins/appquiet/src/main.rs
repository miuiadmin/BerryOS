//! appquiet —— GUI8 canonical 静默 app（no_std 轻量壳）。
//!
//! 行为契约与 std 版逐字等价：argv0 必须是 `bin/appquiet`，随后静默真退出。
//! console=false 的 stdout 是 appd 私有采集端；退出前补零长 EOF（appstub
//! 同款）。去掉 Rust std/fs 链后本 ELF 从约 312KiB 降到数 KiB，为全量 CJK
//! 字库页包回收 rootfs 预算。

#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_process_exit, Handle, BERRY_ERR_BUSY,
};

const ENTRY: &[u8] = b"bin/appquiet";

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env: Handle) -> ! {
    let mut blob = [0u8; 192];
    let mut nb = blob.len();
    let mut hs = [0 as Handle; 8];
    let mut nh = hs.len();
    let s = berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh);
    if s != 0 || nb <= ENTRY.len() || &blob[..ENTRY.len()] != ENTRY {
        berry_process_exit(134);
    }
    // appd 收口契约：console=false 的 stdout 需零长 EOF。通道瞬时满时
    // 自旋重试；其余错误交给进程退出兜底（appstub 同形）。
    if nh > 0 {
        loop {
            let s =
                berry_channel_send(hs[0], core::ptr::null(), 0, core::ptr::null(), 0);
            if s == 0 || s != BERRY_ERR_BUSY {
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
