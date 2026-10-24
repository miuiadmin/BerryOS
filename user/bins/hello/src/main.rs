//! G-eco-c——首个经 rootfs 全链拉起的程序（`docs/G-eco编译侧设计草案.md`
//! G-eco-c 行）。装载路径与 boot 手搭的 echo/talker 完全不同：本镜像以
//! **ELF 本体**躺 rootfs.cpio（fw_cfg 具名文件 → 内核 `ROOTFS` static →
//! 只读 VMO 第 2 插 0x0001_0002 → svcmgr cpio walk → berry-loader ELF
//! 解析 → 逐 PT_LOAD vmar_map → process_start）。
//!
//! 入口 x0 = bootstrap = **env 通道端**（d-3a processargs v1）：svcmgr 在
//! start 前压好环境消息（argv0 字节串 + [console 发端别名]），单次 recv
//! 确定性命中——console 是**自己表里**收到的 handle（能力随消息迁移）。
//! 全链活证 = 本进程自己把 `rootfs hello` 打上串口（smoke 与 `ECO ok`
//! 一并断言——两条都到才算链通）；此后 10s 滚动长睡静默（零稳态噪声，
//! 仅每 10s 一拍可抢占睡眠）。
//!
//! console 发送 = 自旋重发（svcmgr 同款契约：G-eco-b 实踩——启动期饱和
//! 窗口内阻塞等待会输掉三场竞速，自旋者由轮转兜底必达）。

#![no_std]
#![no_main]

use berry_sys::{berry_channel_recv, berry_channel_send, berry_clock_read, berry_thread_sleep,
                Handle, BERRY_ERR_BUSY, CLOCK_MONOTONIC};

const HELLO: &[u8; 13] = b"rootfs hello\n";
const SLEEP_NS: u64 = 10 * 1_000_000_000;

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env: Handle) -> ! {
    // d-3a env 块：argv0（本程序不消费）+ handles[0] = console 别名。
    let mut blob = [0u8; 64];
    let mut nb = blob.len();
    let mut hs = [0 as Handle; 2];
    let mut nh = hs.len();
    if berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh) != 0
        || nh < 1
    {
        trap();
    }
    let console = hs[0];
    // 自旋重发（EBUSY 环 = 重试；其余错码自陷——链断留给标记缺失兜底）。
    loop {
        let s = berry_channel_send(console, HELLO.as_ptr(), HELLO.len(),
                                   core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            trap();
        }
    }
    // 长睡静默：滚动绝对 deadline（thread_sleep 语义）。
    loop {
        let mut now = 0u64;
        if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
            trap();
        }
        berry_thread_sleep(now + SLEEP_NS);
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
