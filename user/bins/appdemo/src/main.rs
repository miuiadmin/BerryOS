//! appdemo —— GUI8 canonical 真链测试 app（no_std 轻量壳）。
//!
//! 契约与 std 版逐字等价：
//! 1. argv0 必须是 `bin/appdemo`；
//! 2. console 输出 `app APPDEMO up`；
//! 3. 用 fs gift 直接走 fsclient RPC 读 `/etc/mtab`，内容恰 23B；
//! 4. 输出 `app APPDEMO fs ok 23`；
//! 5. 零长 EOF + `process_exit(0)`。
//!
//! Phase 0 去 Rust std/libc 链，直接消费 env 与 berry-fsclient，把约
//! 332KiB ELF 降到几十 KiB，为全量 CJK 分页字库回收 rootfs 预算。

#![no_std]
#![no_main]

use berry_fsclient::{get_chunk, resolve_path, PATH_MAX, RPC_BUF};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_object_wait_one,
    berry_process_exit, Handle, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_WRITABLE,
};

const ENTRY: &[u8] = b"bin/appdemo";
const MTAB: &[u8] = b"/dev/root / fat rw 0 0\n";
const UP: &[u8] = b"app APPDEMO up\n";
const FS_OK: &[u8] = b"app APPDEMO fs ok 23\n";

fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

fn send_console(h: Handle, data: &[u8]) -> bool {
    let deadline = now().saturating_add(10_000_000_000);
    loop {
        let s = berry_channel_send(h, data.as_ptr(), data.len(), core::ptr::null(), 0);
        if s == 0 {
            return true;
        }
        if s != BERRY_ERR_BUSY {
            return false;
        }
        let mut observed = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut observed) != 0 {
            return false;
        }
    }
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env: Handle) -> ! {
    let mut blob = [0u8; 192];
    let mut nb = blob.len();
    let mut hs = [0 as Handle; 8];
    let mut nh = hs.len();
    let s = berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh);
    if s != 0 || nb <= ENTRY.len() || &blob[..ENTRY.len()] != ENTRY || nh < 1 {
        berry_process_exit(134);
    }
    let console = hs[0];
    // appd 的 fs 四元装配：env gift[4]=rootfs、gift[5]=fsmgr client。
    // 本 app 只消费后者做真实 RPC；rootfs 只在 manifest 词面授权。
    if nh < 6 || hs[5] == 0 || !send_console(console, UP) {
        berry_process_exit(134);
    }

    let mut path = [0u8; PATH_MAX];
    let Some(plen) = resolve_path(&[], b"/etc/mtab", &mut path) else {
        berry_process_exit(134);
    };
    let mut buf = [0u8; RPC_BUF];
    let Ok((len, more)) = get_chunk(hs[5], &path[..plen], 0, &mut buf) else {
        berry_process_exit(134);
    };
    if len != MTAB.len() || more || buf[..len].iter().zip(MTAB).any(|(&a,&b)| a != b) {
        berry_process_exit(134);
    }

    if !send_console(console, FS_OK) {
        berry_process_exit(134);
    }
    // appd collect 契约：console=true 的 stdout 同样补零长 EOF。
    loop {
        let s = berry_channel_send(console, core::ptr::null(), 0, core::ptr::null(), 0);
        if s == 0 || s != BERRY_ERR_BUSY {
            break;
        }
    }
    berry_process_exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_process_exit(134)
}
