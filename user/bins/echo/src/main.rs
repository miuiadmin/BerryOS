//! G-eco-b 编译子程序——echo 服务（原 svcmgr asm 模板内嵌的 echo 段
//! `kernel/src/user.rs` 标号 4，268B/67 指令，G-eco-b 起随 svcmgr 模板整块
//! 退役换编译产物）。
//!
//! 由 svcmgr 运行期拉起：2 页 VMO 映 [0x1000,0x3000)（本镜像占首页，
//! 次页全零 = 栈，sp0=0x3000）；入口 x0 = end1（bootstrap 迁入本表的
//! channel 端点）。服务循环（契约与 asm 逐字对齐）：
//! - recv(end1) → 原样回显 8 字节；
//! - 空 -EAGAIN → clock+wait READABLE（10s 帽）→ 重 recv；**帽到点重臂不自杀**
//!   （P6 A-3 改板：svcmgr rootfs 走查 ~15s+ 对本服务零往来，空闲自陷 = 常驻
//!   服务早夭，echo 一死稳态 'v' 往返连坐 svcmgr——安静是合法态，守门权交还
//!   发问方的 roundtrip 超时）；
//! - 回显 send -EBUSY（理论不至——对端每圈收一件）→ wait WRITABLE 重试；
//! - 错码自陷（echo 静默无 console，死 = svcmgr 侧标记缺失 + watchdog 错杀
//!   断言暴露）。
//!
//! 同镜像亦作 echo2（talker 的对端）——talker 稳态持续投喂，长空闲重臂无扰。

#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_object_wait_one, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, SIGNAL_READABLE,
    SIGNAL_WRITABLE,
};

/// 等待帽（原模板 0x2_540B_E400 = 1e10 ns）。
const TIMEOUT_NS: u64 = 10 * 1_000_000_000;

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(end1: Handle) -> ! {
    let mut buf = [0u8; 8];
    loop {
        recv_blocking(end1, &mut buf);
        send_retry(end1, buf.as_ptr(), 8); // 原样回显
    }
}

/// 收一件 8 字节消息（空 -EAGAIN → wait READABLE 重收；其余自陷）。
///
/// 空闲帽到点（-ETIMEDOUT）重臂续等：常驻服务的合法长空闲（svcmgr 走查期）；
/// 非 -EAGAIN 错码仍自陷（真链断 loud——svcmgr 侧 roundtrip 超时 + 标记缺失）。
fn recv_blocking(h: Handle, buf: &mut [u8; 8]) {
    loop {
        let mut nb = 8usize;
        let mut nh = 0usize;
        let s = berry_channel_recv(h, buf.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        let mut obs = 0u64;
        let w = berry_object_wait_one(h, SIGNAL_READABLE, now() + TIMEOUT_NS, &mut obs);
        if w != 0 && w != BERRY_ERR_TIMEDOUT {
            trap();
        }
        // 命中/超时一律回循环重 recv（醒来重验是 wait 面纪律；超时=继续空闲）。
    }
}

/// 发送（满 -EBUSY → wait WRITABLE 腾位重试；其余自陷）。
fn send_retry(h: Handle, buf: *const u8, len: usize) {
    loop {
        let s = berry_channel_send(h, buf, len, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            trap();
        }
        wait(h, SIGNAL_WRITABLE);
    }
}

/// 阻塞等信号（10s 绝对帽；超时/错码自陷——对端断供即链断）。
fn wait(h: Handle, signals: u64) {
    let mut obs = 0u64;
    if berry_object_wait_one(h, signals, now() + TIMEOUT_NS, &mut obs) != 0 {
        trap();
    }
}

/// now = clock_read（失败自陷——时钟链不在契约容错面）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
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
