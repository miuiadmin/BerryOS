//! G-eco-b 编译子程序——talker（4-6 A 半 svcmgr 拉起的第二服务，原 svcmgr
//! asm 模板标号 8/9，936B/232 指令 + `DEMO ok\n` banner，随 G-eco-b 退役）。
//!
//! 入口 x0 = t1（svcmgr↔talker 通道本端，bootstrap 迁入本表）。先收交接件
//! （随行 handle ×2：[console 别名（RIGHT_SEND）, t2（echo2 对端）]——能力
//! 经消息进本表），随后与 echo2 直接往返 DEMO_REQ，首圈验回显后凭**收到的**
//! console handle 打 `DEMO ok`（smoke 第 23 标记——「服务经 channel 全链
//! 互联 + 能力经消息授权」的出口演示），稳态节流往返打 'd'。
//! 断链自陷被杀（标记缺失 = smoke FAIL 兜底）。

#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_object_wait_one, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
};

/// 等待帽（原模板 0x2_540B_E400 = 1e10 ns）。
const TIMEOUT_NS: u64 = 10 * 1_000_000_000;
/// 节流阈值（原模板 `movz x9,#0x30,lsl16`）。
const THROTTLE: u64 = 0x30_0000;
/// DEMO_REQ：低 32 位 0xE000_0D30（区别 echo 链的 0xE000_0E30）。
const DEMO_REQ: u64 = 0x0000_0000_E000_0D30;
/// smoke 第 23 标记（8 字节，恰 4 对齐）。
const DEMO_OK: &[u8; 8] = b"DEMO ok\n";

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(t1: Handle) -> ! {
    // ---- 交接件：console 别名 + t2（随行 handle 迁入本表）----
    let mut buf = [0u8; 8];
    let mut handles = [0 as Handle; 2];
    let nh = recv_handles(t1, &mut buf, &mut handles);
    if nh != 2 {
        trap(); // 交接件必须恰双 handle
    }
    let console = handles[0];
    let t2 = handles[1];
    if console == 0 || t2 == 0 {
        trap();
    }

    // ---- 首圈 round-trip 验收 + `DEMO ok` ----
    let mut req = DEMO_REQ.to_le_bytes();
    send_retry(t2, req.as_ptr(), 8);
    roundtrip(t2, &mut buf, &req);
    send_retry(console, DEMO_OK.as_ptr(), 8);

    // ---- 稳态：节流 + 往返 + 'd' ----
    let d = [b'd'];
    loop {
        throttle();
        req = DEMO_REQ.to_le_bytes(); // recv 覆写后重写（asm 同款保险）
        send_retry(t2, req.as_ptr(), 8);
        roundtrip(t2, &mut buf, &req);
        send_retry(console, d.as_ptr(), 1);
    }
}

/// 收一件消息（可带随行 handle；空 -EAGAIN → wait READABLE 重收）。
fn recv_handles(h: Handle, buf: &mut [u8; 8], handles: &mut [Handle; 2]) -> usize {
    loop {
        let mut nb = 8usize;
        let mut nh = 2usize;
        let s = berry_channel_recv(h, buf.as_mut_ptr(), &mut nb, handles.as_mut_ptr(), &mut nh);
        if s == 0 {
            return nh;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        wait(h, SIGNAL_READABLE);
    }
}

/// 对端回显验收：wait READABLE → recv（-EAGAIN 重等）→ 低 32 位比对。
fn roundtrip(h: Handle, buf: &mut [u8; 8], req: &[u8; 8]) {
    wait(h, SIGNAL_READABLE);
    loop {
        let mut nb = 8usize;
        let mut nh = 0usize;
        let s = berry_channel_recv(h, buf.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        wait(h, SIGNAL_READABLE);
    }
    if buf[..4] != req[..4] {
        trap(); // 回显低 32 位不符
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

/// 阻塞等信号（10s 绝对帽；超时/错码自陷）。
fn wait(h: Handle, signals: u64) {
    let mut obs = 0u64;
    if berry_object_wait_one(h, signals, now() + TIMEOUT_NS, &mut obs) != 0 {
        trap();
    }
}

/// now = clock_read（失败自陷）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
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
