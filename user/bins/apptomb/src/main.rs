//! app-1 段 a-1——freeze-first 长活自验 app（`docs/app-1实施设计草案-v1.0.md` §3，
//! hello 模型：no_std ELF 本体，env 块 x0 自解析）。
//!
//! 判据载体 = env 块 extras[4]（gift[4]）lifecycle 通道端 B（对抗审查 H1 修：
//! console=true 的 stdout 走 console 通道 SEND 别名、svcmgr 只发不读——串口
//! 行 appd 侧不可观测，故 up/beat/resumed 全走通道，console 行仅诊断辅证）。
//!
//! 生命周期（v0 段 a）：启动发 `UP` → 2s 心跳循环发 `BEAT c=N`（sleep 收绝对
//! deadline）；解冻自证 = 醒后 clock 差值 > 3s（1.5× 心跳）发 `RESUMED c=N`
//! **再**发当拍 BEAT（审修 L1：冻结中到期 sleep 者摘 RQ 挂起至 thaw 即续
//! 〔sched.rs:2323-2341〕，「计数骤停恢复」不可判——写死差值判）。收到
//! PAUSESV1（段 b）的收束不在本段——v0 段 a 由 svcmgr terminate+reap 收口。
//!
//! 契约三条（§5）：argv0 = entry 断言（`bin/apptomb`）/ EOF 先行退出（本段
//! 不读 stdin——feeder 零长 EOF 恒在，无阻塞面）/ 退出码 0 语义不变（本段
//! 不自退，终止权在 tomb_selftest）。
#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_thread_sleep, Handle,
    CLOCK_MONOTONIC,
};

/// 心跳周期（绝对 deadline 制：t0 + HEARTBEAT_NS）。
const HEARTBEAT_NS: u64 = 2 * 1_000_000_000;
/// resumed 判据阈：单拍实际耗时 > 1.5× 心跳 = 中途冻结过（TCG 频偏容忍取
/// 3s——差值判只作相对语义，绝对定标挂 HDR 实测，同 AM-2a 口径）。
const RESUME_GRACE_NS: u64 = 3 * 1_000_000_000;

/// lifecycle 消息（8B 定长：tag u8 + pad 3 + counter u32 LE——与 svcmgr
/// tomb_selftest 同一套常量；文本行是 console 诊断面，不进协议）。
const MSG_SZ: usize = 8;
const TAG_UP: u8 = 1;
const TAG_BEAT: u8 = 2;
const TAG_RESUMED: u8 = 3;

fn clock_now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        dead();
    }
    t
}

/// 非零即死（错码自陷纪律：echo/hello 同款——静默死 = svcmgr 侧标记缺失
/// + watchdog 断言暴露，loud 优于装活）。
fn dead() -> ! {
    loop {
        berry_thread_sleep(u64::MAX);
    }
}

fn send_msg(lc: Handle, tag: u8, c: u32) {
    let mut m = [0u8; MSG_SZ];
    m[0] = tag;
    m[4..8].copy_from_slice(&c.to_le_bytes());
    // 自旋重发（hello 同款契约：启动期饱和窗内阻塞等待会输掉竞速，自旋者
    // 由轮转兜底必达；-EBUSY 满队重试，其他错码死）。
    loop {
        let s = berry_channel_send(lc, m.as_ptr(), MSG_SZ, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != berry_sys::BERRY_ERR_BUSY {
            dead();
        }
    }
}

/// console 诊断行（`前缀` + 十进制 + `\n`——thr exit 降级行同款手排）。
fn print_line(con: Handle, prefix: &[u8], c: u32) {
    let mut line = [0u8; 40];
    let n = prefix.len().min(line.len() - 12);
    line[..n].copy_from_slice(&prefix[..n]);
    let mut i = n;
    let mut v = c as u64;
    let mut digits = [0u8; 10];
    let mut dn = 0;
    loop {
        digits[dn] = b'0' + (v % 10) as u8;
        dn += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    while dn > 0 {
        dn -= 1;
        line[i] = digits[dn];
        i += 1;
    }
    line[i] = b'\n';
    let total = i + 1;
    let mut tried = 0;
    while berry_channel_send(con, line.as_ptr(), total, core::ptr::null(), 0) != 0 {
        tried += 1;
        if tried > 64 {
            return; // 诊断面尽力而为——协议判据不依赖 console 行（H1 修）
        }
    }
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env: Handle) -> ! {
    // env 块（d-3a）：argv0 = `bin/apptomb`（SpawnSpec.name = manifest entry，
    // 契约断言面）+ handles 前缀紧致 [stdout 别名, stdin 端, self_proc,
    // self_vmar, extras…]——stdin_feed=Some(&[]) 在位（M1 修：tomb_selftest
    // 直传），extras[0] = lifecycle 端 B。收窗 8 槽全开（spawn 侧 nhandles=5）。
    let mut blob = [0u8; 64];
    let mut nb = blob.len();
    let mut hs = [0 as Handle; 8];
    let mut nh = hs.len();
    if berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh) != 0
        || nh < 5
    {
        dead();
    }
    // 契约①：argv0 = entry 断言。
    if !blob.starts_with(b"bin/apptomb") {
        dead();
    }
    let con = hs[0]; // stdout 别名（SEND）——诊断行
    let lc = hs[4]; // lifecycle 端 B（SEND|RECV fresh 端）

    send_msg(lc, TAG_UP, 0);
    print_line(con, b"app apptomb up c=", 0);

    let mut c: u32 = 0;
    let mut resumed = false;
    loop {
        let t0 = clock_now();
        if berry_thread_sleep(t0.wrapping_add(HEARTBEAT_NS)) != 0 {
            dead();
        }
        // resumed 差值判（审修 L1）：冻结中到期的 sleep 摘 RQ 挂起、thaw 即续
        // ——醒来返回时 dt 吞掉整个冻结窗。
        if !resumed && clock_now().wrapping_sub(t0) > RESUME_GRACE_NS {
            resumed = true;
            send_msg(lc, TAG_RESUMED, c);
            print_line(con, b"apptomb resumed c=", c);
        }
        c = c.wrapping_add(1);
        send_msg(lc, TAG_BEAT, c);
        print_line(con, b"apptomb beat c=", c);
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    dead()
}
