//! futex —— `SYS_futex`(98) 拦截（T-1 §3 候选 A）
//!
//! std 1.95 的全部同步原语（mutex/cond/rwlock/mpsc/park）走 futex：快路径
//! CAS（用户态，不经本文件）；慢路径 `libc::syscall(SYS_futex, …)`——本文件
//! 在此拦截，把等待落到内核 Event 池（object_wait_one 真睡，不烧片）。
//!
//! - `syscall()` **7 参定形**：AArch64 C-variadic 前 8 个 GP 实参恒走
//!   x0..x7，定参签名位对齐即兼容变参调用方（C-2a get*_r 教训正面应用）；
//!   nr≠98 保持 set_errno(ENOSYS)+-1（旧零参桩语义零漂移）。
//! - std 只用两形态（std futex.rs 实测）：op=137（FUTEX_WAIT_BITSET|PRIVATE
//!   ——timeout 为 CLOCK_MONOTONIC **绝对**时刻）与 op=129
//!   （FUTEX_WAKE|PRIVATE）；其余 op ENOSYS。
//! - **槽池**：16 槽静态数组（CAS 取槽），每槽惰性建一次 Event（handle
//!   永驻 = 有界）。取槽即清历史粘滞位（上一任 waiter 的迟到 wake）；
//!   **两段式防 lost-wakeup**：取槽 → 清粘滞 → 再读 word（变了 = 免睡放槽）；
//!   `futex_wake` 对同 addr 的全部 busy 槽 signal（**over-wake 有意**——
//!   关死 steal-hole，std 全部 wait 方言容忍伪唤醒）；池耗尽 → wait 即刻
//!   返 0（降级语义，正确性不变）。
//! - 槽 Event 用用户位 1<<32（与内核模板 ponger 哑 Event 同位值——Event
//!   信号位是 per-object 的，跨对象无碰撞；内核 `USER_SIGNAL_BITS` 表只收
//!   内核侧模板位，本文件不触）。
//! - **本文件零 malloc**（鸡芥闭合）：池是静态数组、Event 是内核 handle
//!   ——malloc 堆锁（alloc.rs）反过来调 `futex_wait/wake` 不得再入分配面。

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use berry_sys::{
    berry_event_create, berry_object_signal, berry_object_wait_one, BERRY_ERR_TIMEDOUT,
};

use crate::{c_int, c_long};

pub const SYS_FUTEX: c_long = 98;
/// FUTEX_WAKE|FUTEX_PRIVATE_FLAG。
const OP_WAKE: c_long = 1 | 128;
/// FUTEX_WAIT_BITSET|FUTEX_PRIVATE_FLAG（std 的 wait 方言——timeout 绝对）。
const OP_WAIT_BITSET: c_long = 9 | 128;
/// 槽 Event 的用户信号位（1<<32；见模块头）。
const EV_BIT: u64 = 1 << 32;
/// 槽池容量（对 ≤4 waiter 的 du/sort 形态结构性够；耗尽 = 降级非失败）。
const SLOTS: usize = 16;

struct Slot {
    /// 0 = 空闲、1 = 被某 waiter 持有（CAS 取/放）。
    taken: AtomicU32,
    /// futex word 地址（taken==1 时有效；Release 写 / Acquire 读配对）。
    addr: AtomicUsize,
    /// Event handle；0 = 未建（惰性一次，槽主独占写——taken CAS 取槽后无
    /// 并发写者，UnsafeCell 足矣）。
    ev: core::cell::UnsafeCell<u32>,
}

// SAFETY: ev 只被 CAS 取槽成功的槽主写（互斥由 taken 保证）。可见性协议
// （对抗审查⑦：写读两序都按此为准，此前实现双向违约）：
// - 写序（槽主）：CAS → ev 惰性建/清粘滞 → addr.store(Release)——Release
//   只发布**此前**的写，ev 必须先落再发 addr；
// - 读序（waker）：taken → addr（Acquire）→ ev——Acquire 读到 Release 写值
//   ⟹ 槽主发布前的全部写（含 ev）可见；**ev 读必须排在 addr Acquire 读之后**
//   （排在之前的普通读不受 Acquire 定序 = 可能读到旧 0 → 满配槽被跳过 =
//   lost wakeup）。
unsafe impl Sync for Slot {}

static POOL: [Slot; SLOTS] = [const {
    Slot {
        taken: AtomicU32::new(0),
        addr: AtomicUsize::new(0),
        ev: core::cell::UnsafeCell::new(0),
    }
}; SLOTS];

/// 取槽：CAS taken 0→1 成功后惰性建 Event、清历史粘滞，**最后才 Release 发
/// 布 addr**（写序见 `unsafe impl Sync` 注——addr 先行 = waker 见槽不见 ev）。
fn take_slot(addr: usize) -> Option<usize> {
    for (i, s) in POOL.iter().enumerate() {
        if s
            .taken
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            // SAFETY: ev 写点槽主独占（taken==1 由 CAS 互斥）。
            let evp = unsafe { &mut *s.ev.get() };
            if *evp == 0 {
                let mut h = 0u32;
                if berry_event_create(0, &mut h) == 0 && h != 0 {
                    *evp = h; // 永驻：不 close（有界 ≤SLOTS 枚）
                }
            }
            // 取槽即清粘滞（新建 Event 无粘滞、此调用无害；复用槽清掉上一
            // 任的迟到 wake——不清 = 下一次 wait 秒醒退化自旋）。
            if *evp != 0 {
                let _ = berry_object_signal(*evp, EV_BIT, 0);
            }
            // ev 已落，才发布 addr（Release 定序 = waker Acquire 读到 addr 即
            // 必见上两步的 ev 写——对抗审查⑦ 的写序修）。
            s.addr.store(addr, Ordering::Release);
            return Some(i);
        }
    }
    None
}

/// 放槽：先清 addr 再 Release taken（waker 读到 taken==1 时 addr 可能已
/// 归零——不匹配即放过，无害）。
fn release_slot(i: usize) {
    let s = &POOL[i];
    s.addr.store(0, Ordering::Relaxed);
    s.taken.store(0, Ordering::Release);
}

/// `libc::syscall` 真身：futex(98) 拦截 + 其余号 ENOSYS。
///
/// C 约定：成功返 0（WAKE 返唤醒计数）、失败 -1+errno（ETIMEDOUT/ENOSYS）。
#[no_mangle]
pub extern "C" fn syscall(
    nr: c_long,
    a1: c_long,
    a2: c_long,
    a3: c_long,
    a4: c_long,
    _a5: c_long,
    _a6: c_long,
) -> c_long {
    if nr == SYS_FUTEX {
        let addr = a1 as usize;
        let op = a2 as u32;
        if op == OP_WAIT_BITSET as u32 {
            // a3 = expected 值；a4 = *timespec（FUTEX_WAIT_BITSET 口径 =
            // CLOCK_MONOTONIC 绝对时刻；null = 永等）。
            let deadline = ts_to_deadline(a4 as *const Timespec);
            let r = futex_wait(addr, a3 as u32, deadline);
            // A7-4 ③b：等醒补投（§3.3）——std 同步原语慢路径（cond/mutex
            // 慢等/park）是用户态最长的可中断等，信号在此面兑现。仅挂
            // syscall() 面：futex_wait/mutex_lock（malloc 堆锁）不挂——
            // malloc 路径入投递 = handler→malloc 自锁死锁面放大。
            crate::signal::may_dispatch();
            if r == crate::ETIMEDOUT {
                crate::set_errno(crate::ETIMEDOUT);
                return -1;
            }
            return 0; // 值变/被唤醒/池耗尽降级：调用方重验自旋循环
        }
        if op == OP_WAKE as u32 {
            return futex_wake(addr);
        }
    }
    crate::set_errno(crate::ENOSYS);
    -1
}

/// musl aarch64 `struct timespec`（FUTEX_WAIT_BITSET 的绝对时刻）。
#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

fn ts_to_deadline(p: *const Timespec) -> u64 {
    if p.is_null() {
        return u64::MAX;
    }
    unsafe {
        let s = (*p).tv_sec.max(0) as u64;
        let ns = (*p).tv_nsec.clamp(0, 999_999_999) as u64;
        s.saturating_mul(1_000_000_000).saturating_add(ns)
    }
}

/// 等待 `*addr == expected` 不再成立。返 0 = 过（值变/被唤醒/降级）、
/// ETIMEDOUT = 到点。**零 malloc**。
pub fn futex_wait(addr: usize, expected: u32, deadline: u64) -> c_int {
    // ① 快读：已变 → 无需等待。
    if unsafe { core::ptr::read_volatile(addr as *const u32) } != expected {
        return 0;
    }
    // ② 取槽（池耗尽 → 降级即刻返 0：调用方重验循环，正确性不变）。
    let Some(si) = take_slot(addr) else {
        return 0;
    };
    // ③ 两段式：取槽后再读——注册窗内值变则免睡（防 lost-wakeup）。
    if unsafe { core::ptr::read_volatile(addr as *const u32) } != expected {
        release_slot(si);
        return 0;
    }
    // ④ 真睡（poll-first 兜注册后睡前的 signal）。
    let mut obs = 0u64;
    // SAFETY: 只读（本线程即槽主）。
    let ev = unsafe { *POOL[si].ev.get() };
    let s = berry_object_wait_one(ev, EV_BIT, deadline, &mut obs);
    release_slot(si);
    if s == BERRY_ERR_TIMEDOUT {
        return crate::ETIMEDOUT;
    }
    0
}

/// 唤醒所有等在 `addr` 上的槽（over-wake 有意）。返 signal 成功数。
pub fn futex_wake(addr: usize) -> c_long {
    let mut n: c_long = 0;
    for s in POOL.iter() {
        if s.taken.load(Ordering::Acquire) == 1 && s.addr.load(Ordering::Acquire) == addr {
            // ev 读排在 addr Acquire 读**之后**（对抗审查⑦ 的读序修）：Acquire
            // 读到槽主的 Release 写值 ⟹ 此前的 ev 惰性建对此线程可见；先读
            // ev = 普通读不受 Acquire 定序 → 可能见旧 0 → 满配槽被跳过。
            // SAFETY: ev 只读；可见性论证见上（模块头 unsafe impl Sync 注）。
            let ev = unsafe { *s.ev.get() };
            if ev != 0 {
                if berry_object_signal(ev, 0, EV_BIT) == 0 {
                    n += 1;
                }
            }
        }
    }
    n
}

/// futex 化互斥对（malloc 堆锁/key 登记锁共用；零 malloc 闭合）：
/// CAS 0→1 失败 → 等 word 回到 1（即仍锁着）的真睡；解锁 store 0 + wake。
pub fn mutex_lock(l: &AtomicU32) {
    while l
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        let _ = futex_wait(l as *const AtomicU32 as usize, 1, u64::MAX);
    }
}

pub fn mutex_unlock(l: &AtomicU32) {
    l.store(0, Ordering::Release);
    let _ = futex_wake(l as *const AtomicU32 as usize);
}
