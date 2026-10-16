//! G-eco-b——svcmgr 服务管理器 Rust 化（替换 `内核/src/user.rs` 的 svcmgr
//! asm 模板，943-1663 行整块退役）。契约与 asm 逐字等价（4-5b + 4-6 A 半
//! 验收链，smoke 第 21/23 标记不许降级）：
//!
//! - B 半：拉起 echo 服务（process_create 双出参 → 2 页 VMO/commit/write
//!   内嵌 echo 镜像 → vmar_map@0x1000 → channel_create → thread_create →
//!   process_start(bootstrap=end1)——close 原值必 -EBADF 迁移自证）→
//!   ECHO_REQ 往返验回显（低 32 位）→ `SVC ok`；
//! - A 半：直线拉起 talker + echo2（同一份 echo 镜像第二实例），交接消息
//!   一次迁移双 handle（console 别名 + t2 对端——能力经消息授权，不经
//!   全局名）→ talker 打 `DEMO ok`；
//! - 稳态：节流 + ECHO_REQ 实证往返 + 'v'。
//!
//! 子程序镜像 `include_bytes!` 内嵌（两遍构建：leaf bins 先产平像再编本
//! crate，见 justfile `build-user`；改子程序须先重跑 pass 1）。入口 x0 =
//! console 发端（boot 首插定值 0x0001_0001）。任何链断 → 自陷被杀 →
//! 标记缺失 = smoke FAIL 兜底。

#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_clock_read,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_one, berry_process_create,
    berry_process_start, berry_thread_create, berry_vmar_map, berry_vmo_commit, berry_vmo_create,
    berry_vmo_write, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BAD_HANDLE, BERRY_ERR_BUSY,
    CLOCK_MONOTONIC, RIGHT_READ, RIGHT_SEND, RIGHT_WRITE, SIGNAL_READABLE, SIGNAL_WRITABLE,
};

/// echo 服务镜像（G-eco-b 编译产物；echo1/echo2 共用同一份字节——asm 同款）。
const ECHO_BIN: &[u8] = include_bytes!("../../../userbins/echo.bin");
/// talker 镜像。
const TALKER_BIN: &[u8] = include_bytes!("../../../userbins/talker.bin");

/// ECHO_REQ（4-5b 验收载荷；回显比对取低 32 位 0xE000_0E30）。
const ECHO_REQ: u64 = 0x00C0_FFEE_E000_0E30;
/// smoke 第 21 标记（7 字节）。
const SVC_OK: &[u8; 7] = b"SVC ok\n";

const TIMEOUT_NS: u64 = 10 * 1_000_000_000;
const THROTTLE: u64 = 0x30_0000;
/// 子进程布局：镜像首页 @0x1000 + 零栈页，映射 [0x1000,0x3000)，sp0=0x3000。
const CHILD_MAP_LEN: usize = 0x2000;
const CHILD_ENTRY: usize = 0x1000;
const CHILD_SP: usize = 0x3000;

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(console: Handle) -> ! {
    // ---- B 半：echo 服务 + ECHO_REQ 往返 + `SVC ok` ----
    let (end0, end1) = channel_pair();
    spawn_child(ECHO_BIN, end1);
    let mut buf = [0u8; 8];
    let req = ECHO_REQ.to_le_bytes();
    send_retry(end0, req.as_ptr(), 8);
    roundtrip(end0, &mut buf, &req);
    send_spin(console, SVC_OK.as_ptr(), 7);

    // ---- A 半：talker + echo2，交接双 handle ----
    let (s1, t1) = channel_pair();
    spawn_child(TALKER_BIN, t1);
    let (t2, e2) = channel_pair();
    spawn_child(ECHO_BIN, e2);
    let mut gift = [0 as Handle; 2];
    if berry_handle_duplicate(console, RIGHT_SEND, &mut gift[0]) != 0 {
        trap(); // 先复制再送：send 会 take 走句柄，原值须留给稳态
    }
    gift[1] = t2; // echo2 对端随消息交付 talker
    let payload = 0u64; // 纯载体，talker 不看内容
    if berry_channel_send(s1, payload.to_le_bytes().as_ptr(), 8, gift.as_ptr(), 2) != 0 {
        trap(); // 交接件未入队 = A 半链断
    }

    // ---- 稳态：节流 + 实证往返 + 'v'（每个 'v' = 一次 ECHO_REQ 往返）----
    let v = [b'v'];
    loop {
        throttle();
        send_retry(end0, req.as_ptr(), 8);
        roundtrip(end0, &mut buf, &req);
        send_spin(console, v.as_ptr(), 1);
    }
}

/// 建通道对（两端均授 SEND|RECV|DUPLICATE；返回 (本端, 交接端)）。
fn channel_pair() -> (Handle, Handle) {
    let mut a = 0 as Handle;
    let mut b = 0 as Handle;
    if berry_channel_create(0, &mut a, &mut b) != 0 {
        trap();
    }
    (a, b)
}

/// 拉起一个子进程：process_create 双出参 → 2 页 VMO（镜像首页 + 零栈页）
/// → vmar_map@0x1000（rights 恰 READ|WRITE）→ thread_create →
/// process_start(entry=0x1000, sp=0x3000, bootstrap)——bootstrap 随 start
/// 迁入子表（子入口 x0 = 子表新值），close 原值必 -EBADF（迁移自证）。
fn spawn_child(image: &[u8], bootstrap: Handle) {
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    if berry_process_create(0, 0, &mut hp, &mut hv) != 0 || hp == 0 || hv == 0 {
        trap();
    }
    let mut vmo = 0 as Handle;
    if berry_vmo_create(CHILD_MAP_LEN as u64, 0, &mut vmo) != 0 || vmo == 0 {
        trap();
    }
    if berry_vmo_commit(vmo, 0, CHILD_MAP_LEN as u64) != 0 {
        trap();
    }
    if berry_vmo_write(vmo, image.as_ptr(), 0, image.len()) != 0 {
        trap();
    }
    let mut vaddr: usize = CHILD_ENTRY; // 映射基址提示（in-out 槽预写）
    if berry_vmar_map(hv, 0, vmo, 0, &mut vaddr, CHILD_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0 {
        trap();
    }
    let mut ht = 0 as Handle;
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        trap();
    }
    if berry_process_start(hp, ht, CHILD_ENTRY, CHILD_SP, bootstrap) != 0 {
        trap();
    }
    if berry_handle_close(bootstrap) != BERRY_ERR_BAD_HANDLE {
        trap(); // 迁移自证：原值必须已作废
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
        trap(); // 回显低 32 位不符（期望 0xE000_0E30）
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

/// console 发送——**自旋重发**（asm 同形：`SVC ok`/稳态 'v' 的 EBUSY 环是
/// `b 6b`/`b 14b` 纯重发、无 wait）。启动期 console 队列饱和窗口内阻塞等
/// WRITABLE 会被自旋友商（abuser/spawner 等）持续抢槽——唤醒后须等一整圈
/// 轮转才拿到片，三场竞速即耗尽 10s 帽（G-eco-b 实踩：SVC ok 从未落串口、
/// echo1 连带饿死）；自旋者由轮转兜底必达。EL0 可抢占 = 自旋有界。
fn send_spin(h: Handle, buf: *const u8, len: usize) {
    loop {
        let s = berry_channel_send(h, buf, len, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            trap();
        }
    }
}

/// 阻塞等信号（10s 绝对帽；超时/错码自陷——对端断供即链断）。
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
