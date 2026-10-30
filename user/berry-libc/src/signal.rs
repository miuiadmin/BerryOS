//! # signal.rs —— POSIX 信号面 libc 真身（A7-4 段③a：纯语义面）
//!
//! `docs/事件循环A7-4信号面实施设计草案.md` §4 拆步表的段③ 拆两半：
//! **段③a（本刀）= 不依赖内核两号（0x86/0x87）的纯用户态语义面**——
//! sigset 五件 + sigprocmask 掩码状态机 + sigaction/signal 表面；投递
//! 循环（`take() | held_swap(0)` 归并分诊）与 kill/raise/pause 真身化
//! 随段③b（内核号落地后）一并接线。本刀后 unblock 补投语义暂缺（掩码
//! 面已真、投递点未挂）——A7-4 §3.4 偏差清单的过渡窗，随段③b 闭合。
//!
//! ## 语义核单源
//!
//! 全部信号语义（界检/掩码三 how/位图层/表编码）= `berry-signal` crate
//! （host 19 测背书），本文件只做 libc ABI 胶水：指针 ↔ musl 结构转换
//! + 全局状态存储。「不长第二套」。
//!
//! ## 布局钉板（C-2a Stat 案立法）
//!
//! `struct sigaction` 镜像对着 **libc crate 0.2.191 aarch64-musl** 逐
//! 字段抄（`src/unix/linux_like/linux/musl/mod.rs`）：sa_sigaction@0 /
//! sa_mask@8（128B）/ sa_flags@136 / sa_restorer@144，总 152B——
//! `offset_of!` 编译期钉板全字段（钉全不钉半，同尺寸异序 = 最阴的
//! 静默错）。`sigset_t` = 128B（`__val: [u64; 16]`——u64 数组 LE 与
//! berry-signal `[u8; 128]` 位图位序同构，拷贝零转换）。SIG_BLOCK/
//! UNBLOCK/SETMASK = 0/1/2（kernel asm-generic 钉死）与 berry-signal
//! `MASK_*` 同值直传。
//!
//! ## 全局态与单线程口径
//!
//! `SIG_MASK` / `SIG_ACTIONS` 按本 crate ABI 单线程口径 `static mut`
//! 直写（lib.rs ABI 口径、fs.rs RA 缓冲同款）——多线程并发
//! sigaction/sigprocmask 竞争 = v1 接受偏差；POSIX per-thread 掩码
//! 偏差同记 A7-4 §3.4（v1 进程级单掩码）。
//!
//! ## 谎报桶回收记录（本刀出册）
//!
//! stubs.rs ok! 桶五符号（sigaction/signal/sigaddset/sigemptyset/
//! sigfillset）+ sigprocmask 手写桩（谎报「旧掩码全空」）→ 全部迁本
//! 文件真身。行为升级对现有消费者兼容：uu_env block_signal 链
//! （emptyset→addset→sigprocmask）从「全谎报」变「真位运算 + 真掩码
//! 更新」，返回值面不变；std 启动面 sigaction(SIGSEGV) 从「谎报成功」
//! 变「真装表」——段③b 前无投递循环，装表无副作用。sigaltstack 留
//! ok! 桶（栈面无语义，谎报成功无害）；kill/killpg 留 EPERM 桶（真身
//! = kill_targets_self 分诊 + 内核 send，段③b）。

use core::ffi::c_int;
use core::ffi::c_void;
use core::ptr;

use berry_signal::{ActionTable, SigMask, SIGSET_BYTES};

use crate::set_errno;

// ---- musl aarch64 struct sigaction 镜像（头注「布局钉板」）----

/// `struct sigaction`（libc crate aarch64-musl 逐字段镜像——sa_sigaction
/// 单字段承载 handler/sa_sigaction union；sa_restorer musl 有此槽
/// （与 glibc 布局同序），v1 不消费只透传回读）。
#[repr(C)]
pub struct Sigaction {
    /// sighandler_t = size_t；三值编码与 berry-signal 对齐
    /// （0=SIG_DFL / 1=SIG_IGN / ≥2=fn 地址）。
    pub sa_sigaction: usize,
    /// sigset_t（128B；`__val: [u64; 16]` 与 `[u8; 128]` 同构）。
    pub sa_mask: [u8; SIGSET_BYTES],
    pub sa_flags: c_int,
    pub sa_restorer: usize,
}

// C-2a 钉板法：全字段 + 总宽（同尺寸异序互换静默错的编译期防线）。
const _: () = assert!(core::mem::size_of::<Sigaction>() == 152);
const _: () = assert!(core::mem::align_of::<Sigaction>() == 8);
const _: () = assert!(core::mem::offset_of!(Sigaction, sa_sigaction) == 0);
const _: () = assert!(core::mem::offset_of!(Sigaction, sa_mask) == 8);
const _: () = assert!(core::mem::offset_of!(Sigaction, sa_flags) == 136);
const _: () = assert!(core::mem::offset_of!(Sigaction, sa_restorer) == 144);

// GETPID 镜像钉板（berry-signal 头注「GETPID 镜像」承诺的执法面）：
// sys.rs `SELF_PID` 与 berry-signal `GETPID_MIRROR` 双值对齐——任一
// 侧改动即编译期红（单边字面钉板检不出 sys.rs 改值）。
const _: () = assert!(berry_signal::GETPID_MIRROR == crate::sys::SELF_PID);

// ---- 全局态（头注「全局态与单线程口径」）----

/// 进程级信号掩码（v1 单掩码偏差档；berry-signal SigMask = u64 与内核
/// `pending_signals` 同宽同位序——段③b 投递循环拷贝零转换）。
static mut SIG_MASK: SigMask = 0;
/// sigaction 表（三值编码槽；const new 直落静态）。
static mut SIG_ACTIONS: ActionTable = ActionTable::new();

/// 界检失败 → errno + -1 的惯用收口（berry-signal 返负 errno）。
fn reject(e: i32) -> c_int {
    set_errno(-e);
    -1
}

// ---- sigset 五件（位图层转发；指针按 libc 契约指向 128B 对象）----

#[no_mangle]
pub extern "C" fn sigemptyset(set: *mut c_void) -> c_int {
    if set.is_null() {
        return reject(berry_signal::EINVAL);
    }
    // SAFETY: set 指向调用方 sigset_t（128B，libc 契约）。
    unsafe { ptr::copy_nonoverlapping(berry_signal::set_empty().as_ptr(), set as *mut u8, SIGSET_BYTES) };
    0
}

#[no_mangle]
pub extern "C" fn sigfillset(set: *mut c_void) -> c_int {
    if set.is_null() {
        return reject(berry_signal::EINVAL);
    }
    // SAFETY: 同上；musl fillset = 全 128B 填 0xFF（1024 位全 1——
    // 惯例用法必须活，berry-signal 头注「宽进严出」档）。
    unsafe { ptr::copy_nonoverlapping(berry_signal::set_full().as_ptr(), set as *mut u8, SIGSET_BYTES) };
    0
}

#[no_mangle]
pub extern "C" fn sigaddset(set: *mut c_void, sig: c_int) -> c_int {
    if set.is_null() {
        return reject(berry_signal::EINVAL);
    }
    let mut buf = [0u8; SIGSET_BYTES];
    // SAFETY: set 指向调用方 sigset_t（128B，libc 契约）。
    unsafe { ptr::copy_nonoverlapping(set as *const u8, buf.as_mut_ptr(), SIGSET_BYTES) };
    match berry_signal::set_add(&mut buf, sig) {
        Ok(()) => {
            // SAFETY: 同上回写。
            unsafe { ptr::copy_nonoverlapping(buf.as_ptr(), set as *mut u8, SIGSET_BYTES) };
            0
        }
        Err(e) => reject(e),
    }
}

#[no_mangle]
pub extern "C" fn sigdelset(set: *mut c_void, sig: c_int) -> c_int {
    if set.is_null() {
        return reject(berry_signal::EINVAL);
    }
    let mut buf = [0u8; SIGSET_BYTES];
    // SAFETY: set 指向调用方 sigset_t（128B，libc 契约）。
    unsafe { ptr::copy_nonoverlapping(set as *const u8, buf.as_mut_ptr(), SIGSET_BYTES) };
    match berry_signal::set_del(&mut buf, sig) {
        Ok(()) => {
            // SAFETY: 同上回写。
            unsafe { ptr::copy_nonoverlapping(buf.as_ptr(), set as *mut u8, SIGSET_BYTES) };
            0
        }
        Err(e) => reject(e),
    }
}

#[no_mangle]
pub extern "C" fn sigismember(set: *const c_void, sig: c_int) -> c_int {
    if set.is_null() {
        return reject(berry_signal::EINVAL);
    }
    let mut buf = [0u8; SIGSET_BYTES];
    // SAFETY: set 指向调用方 sigset_t（128B，libc 契约）。
    unsafe { ptr::copy_nonoverlapping(set as *const u8, buf.as_mut_ptr(), SIGSET_BYTES) };
    match berry_signal::set_ismember(&buf, sig) {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(e) => reject(e),
    }
}

// ---- sigprocmask（掩码状态机真身）----

/// libc crate 三参版（glibc/musl 用户面同形——kernel rt_sigprocmask
/// 的 size 参被 musl wrapper 吃掉）。
#[no_mangle]
pub extern "C" fn sigprocmask(
    how: c_int,
    set: *const c_void,
    oldset: *mut c_void,
) -> c_int {
    // 先读 set 后写 oldset（对抗审查 F1：`set == oldset` 别名时先写
    // oldset 会把入参砸成当前掩码——UNBLOCK 档静默清空整个掩码、
    // SETMASK 档变 no-op；musl/Linux 读先写后序，`sigprocmask(how,
    // &s, &s)` 惯例必须可用）。
    let mut buf = [0u8; SIGSET_BYTES];
    let has_set = !set.is_null();
    if has_set {
        // SAFETY: set 指向调用方 sigset_t（128B，libc 契约）。
        unsafe { ptr::copy_nonoverlapping(set as *const u8, buf.as_mut_ptr(), SIGSET_BYTES) };
    }
    // 旧掩码严出（set_of_mask：语义位外 120B 恒零——干净诚实档；
    // 即使后续 how 界检失败，POSIX 对失败时 oldset 内容未定义，先写
    // 无害且调用方拿到的是真值）。
    // SAFETY: oldset 指向调用方 sigset_t（128B，libc 契约）。
    unsafe {
        if !oldset.is_null() {
            let out = berry_signal::set_of_mask(SIG_MASK);
            ptr::copy_nonoverlapping(out.as_ptr(), oldset as *mut u8, SIGSET_BYTES);
        }
    }
    if !has_set {
        return 0; // 纯查询档（POSIX：set=NULL 只取 oldset）
    }
    // 宽进：低 8 字节 LE 取 u64、语义位外忽略（fillset 惯例必须活）。
    let arg = berry_signal::mask_of_set(&buf);
    // SAFETY: 单线程口径（头注），static mut 直写（lib.rs ABI 口径）。
    unsafe {
        match berry_signal::apply_mask(how, SIG_MASK, arg) {
            Ok(new) => {
                SIG_MASK = new;
                // 段③b 接线点：unblock 补投 dispatch 在此挂（A7-4 §3.3
                // ——held 位被放出后需再跑一轮投递循环；本刀前无循环）。
            }
            Err(e) => return reject(e),
        }
    }
    0
}

// ---- sigaction / signal（表面真身）----

#[no_mangle]
pub extern "C" fn sigaction(
    sig: c_int,
    act: *const Sigaction,
    oldact: *mut Sigaction,
) -> c_int {
    // 先读 act 后写 oldact（对抗审查 F1：`act == oldact` 别名时先写
    // oldact 会把新装值砸成旧值——musl/Linux 读先写后序）。
    // SAFETY: act 指向调用方 struct sigaction（libc 契约）。
    let new_val = if act.is_null() {
        None
    } else {
        Some(unsafe { (*act).sa_sigaction })
    };
    // KILL/STOP：纯查询与装 SIG_DFL 合法（POSIX EINVAL 只罩
    // catch/ignore——installable 分诊，对抗审查 F3）；界外号恒拒。
    let store = match new_val {
        Some(v) => match berry_signal::installable(sig, v as u64) {
            Ok(store) => store, // false = KILL/STOP 装 DFL no-op 免写
            Err(e) => return reject(e),
        },
        None => {
            if !berry_signal::valid_sig(sig) {
                return reject(berry_signal::EINVAL);
            }
            false
        }
    };
    // SAFETY: oldact 指向调用方 struct sigaction（libc 契约）；
    // SIG_ACTIONS 单线程口径直访。
    unsafe {
        if !oldact.is_null() {
            // v1 简化回读：槽值 + 空掩码 + 零 flags/restorer——POSIX 语义
            // = 上次装表快照（per-sig mask/flags），表只存 u64 槽，快照
            // 面留段③b 按需扩（A7-4 §3.4 偏差注）。
            let v = SIG_ACTIONS.get(sig).unwrap_or(berry_signal::SIG_DFL);
            (*oldact).sa_sigaction = v as usize;
            (*oldact).sa_mask = berry_signal::set_of_mask(0);
            (*oldact).sa_flags = 0;
            (*oldact).sa_restorer = 0;
        }
        if store {
            if let Some(v) = new_val {
                if let Err(e) = SIG_ACTIONS.set(sig, v as u64) {
                    return reject(e);
                }
            }
        }
    }
    0
}

/// ISO C `signal()`（libc crate：`fn signal(c_int, sighandler_t) ->
/// sighandler_t`；handler 三值同 sigaction 编码，SIG_ERR = -1 全 1）。
#[no_mangle]
pub extern "C" fn signal(sig: c_int, handler: usize) -> usize {
    // KILL/STOP 装 DFL = 合法 no-op（槽恒 DFL），catch/ignore 拒
    // （installable 分诊同 sigaction——对抗审查 F3）。
    let store = match berry_signal::installable(sig, handler as u64) {
        Ok(store) => store,
        Err(e) => {
            set_errno(-e);
            return usize::MAX; // SIG_ERR
        }
    };
    // SAFETY: 同 sigaction（单线程口径直访）。
    unsafe {
        let old = SIG_ACTIONS.get(sig).unwrap_or(berry_signal::SIG_DFL);
        if store {
            let _ = SIG_ACTIONS.set(sig, handler as u64);
        }
        old as usize
    }
}
