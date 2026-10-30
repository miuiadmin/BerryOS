//! # signal.rs —— POSIX 信号面 libc 真身（A7-4：段③a 纯语义面 + 段③b 投递循环）
//!
//! `docs/事件循环A7-4信号面实施设计草案.md` §4 拆步表：**段③a** = 不依赖
//! 内核两号的纯用户态语义面（sigset 五件 + sigprocmask 掩码状态机 +
//! sigaction/signal 表面）；**段③b（内核 0x86/0x87 落地后）** = 投递循环
//! （`take() | held_swap(0)` 归并分诊）+ kill/raise/pause 真身 + unblock
//! 补投 + 投递点接线（io/fs/sys/futex/pthread 返回后 may_dispatch——
//! §3.3；偏差清单 §3.4：协同式延迟档，compute 紧循环收不到、不中断已
//! 阻塞调用无 EINTR 面——pause 补显式等信号面）。
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

use berry_signal::{classify, ActionTable, Act, SigMask, SIGSET_BYTES};
use berry_sys::{berry_signal_send, berry_signal_take};

use crate::set_errno;

/// asm-generic SIGABRT（abort 改装消费面——io.rs；berry-signal 单源）。
pub use berry_signal::SIGABRT;
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
/// held 集（段③b）：blocked 窗内 take 到的位暂存——归并式分诊下同号
/// 跨 held/pending 只此一档（POSIX block 窗合并语义）；unblock 补投经
/// `take_total` 归并再出。
static mut SIG_HELD: SigMask = 0;
/// handler 投递计数（段③b）：单调递增——`pause` 用「纪元变动」判窗内
/// 已投递（usleep 尾 may_dispatch 投的 handler 不经 pause 的返回值面）。
static mut SIG_DELIVERED: u64 = 0;

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
                // 段③b（A7-4 §3.3）：unblock/SETMASK 收窄放出 held 位 →
                // 立即补投（held 再投证 = bsh sig_selftest 步②）。Block/
                // 展宽档 take 面不变，dispatch 内 Hold 分支自然接住。
                dispatch_pending();
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
                // POSIX：装 SIG_IGN 即丢弃本号全部 pending（含阻塞中的
                // held 位——「whether blocked or not」，修复轮补）。
                if v as u64 == berry_signal::SIG_IGN {
                    discard_pending_on_ign(sig);
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
            // POSIX（XSI 扩展到 ISO C signal 面，musl 同义）：装 SIG_IGN
            // 即丢弃本号全部 pending——与 sigaction 同一口径（修复轮补）。
            if handler as u64 == berry_signal::SIG_IGN {
                discard_pending_on_ign(sig);
            }
        }
        old as usize
    }
}

// ---- 投递循环 + kill/raise/pause 真身（段③b，A7-4 §3.3）----
//
// 内核 0x86/0x87 的用户态消费面：`take_total` 归并（内核 swap(0) 取走
// | held 清零）→ **逐位 `classify` 按步重查**（每步重读掩码与表——
// handler 中途改掩码/改表即时生效，POSIX delivery-time 口径；修复轮起
// 弃「轮首 plan_dispatch 冻结执行」形态——对抗审查 MAJOR finding）。
// 协同式延迟档（§3.4
// 偏差清单）：投递点 = read/write/clock_nanosleep/futex wait/pthread_join
// 返回后的 [`may_dispatch`] + sigprocmask unblock 补投 + raise 即时——
// compute 紧循环结构性收不到（无内核打断面），handler 只保证
// async-signal-safe（同 POSIX 契约：handler 内取 malloc 锁可自锁）。

/// 取待处置全集：内核 `signal_take`（swap(0) 语义取走）| held 集清零
/// 归并——同号跨 held/pending 只此一位（POSIX block 窗合并语义按构造
/// 成立，berry-signal 头注「归并式分诊」节）。内核 take 失败（ktest 期
/// 无 Process 语境等）→ 出参不动（初值 0）：held 位照常出，内核位未
/// 消费不丢，下轮再取。
fn take_total() -> SigMask {
    let mut from_kernel: u64 = 0;
    let _ = berry_signal_take(&mut from_kernel);
    // SAFETY: 单线程口径（头注），static mut 读清（lib.rs ABI 口径）。
    (unsafe {
        let held = SIG_HELD;
        SIG_HELD = 0;
        held
    }) | from_kernel
}

/// POSIX 钦定（修复轮补，对抗审查 finding）：把某号 disposition 装成
/// SIG_IGN 即**丢弃该号全部 pending——「whether blocked or not」**
/// （阻塞中的 held 位一并丢）。musl/Linux 经 rt_sigaction 内核面同义。
/// 实现：`take_total` 归并取走（held 清零 + 内核 swap(0)）→ 滤掉本号
/// 位 → 余量回 held → 补投一轮（余量中已 unblock 的位立即处置、仍
/// block 的位自然 Hold 回落——sigprocmask 收窄补投同纪律）。
/// 前提：`sig ∈ 1..=64` 且非 KILL/STOP（调用点在 `installable` 放行的
/// store==true 分支内——IGN 档对其结构性拒，界外不可达）。
fn discard_pending_on_ign(sig: c_int) {
    let bit = 1u64 << (sig - 1) as u32;
    let taken = take_total();
    let rest = taken & !bit;
    if rest != 0 {
        // SAFETY: SIG_HELD 单线程口径直访（头注；take_total 已清零，
        // 直写余量 = 恢复非本号 pending 的暂存）。
        unsafe { SIG_HELD = rest };
        dispatch_pending();
    }
}

/// 投递循环真身（段③b）。返回本轮 handler 投递数（[`pause`] 的纪元
/// 检测面；Terminate 档在此直接 exit 不返）。
///
/// **两遍走：取走即分诊（Hold 即刻落 held）+ 工作集按步重查**（修复轮
/// 二式，对抗审查两件）：原「轮首 plan_dispatch 冻结执行」把 handler
/// 中途的表变更挡在轮外（MAJOR）；修复轮首版的「全惰性落 held」又让
/// 「已被 take 走、尚未处置」的阻塞位悬在局部变量里**两头不见**——内核
/// 已 swap 空、held 未落位，handler 内 `sigprocmask(UNBLOCK)` 的嵌套
/// 补投 `take_total` 归并到空集，包络恢复再把 unblock 一并抹掉 = 位
/// 永久滞留 held（位号序敏感：仅「Hold 位号 > Call 位号」触发——修复
/// 轮对抗审查实抓回归）。
/// - 第一遍（可见性）：take 到的位按当下掩码/表逐位 [`classify`]——
///   Hold 位**即刻持久化进 `SIG_HELD`**，其余进本轮工作集（本遍
///   Call/Drop 分诊不携带，第二遍重查）。冻结 plan 时代「HELD 先持久化
///   再执行」的嵌套可见性性质由此保留。
/// - 第二遍（delivery-time）：工作集升序逐位 [`classify`] **每步重读**
///   `SIG_MASK`/`SIG_ACTIONS`——handler 内装的新 handler 同轮后续位即时
///   生效（表变更不回滚）；掩码变更只对**嵌套投递**生效——handler 返回
///   后全量快照恢复（下款），后续位按恢复后掩码分诊（Linux 同形：handler
///   block 某后续位，返回后该位照投——修复轮对抗审查勘正「掩码即时
///   生效」的过头注）。
///
/// **Call 包络 = 全量快照恢复**（修复轮，sigreturn 真口径）：进入
/// handler 前 `saved_mask = SIG_MASK`、执行期 `SIG_MASK |= 本号位`
/// （SA_NODEFER-off 自动 block 本号——同号递归投递挡进 held 集），
/// 返回后 `SIG_MASK = saved_mask` **全量恢复**——handler 内经
/// sigprocmask 的掩码改动不越 handler 存活（Linux sigreturn 同形；
/// 原「只摘包络位」差量保留档连同其「POSIX 同口径」错注一并翻正）。
///
/// 每轮 fresh [`take_total`]——不携带跨轮 total：handler 内再 raise 的
/// 信号进内核 pending，由嵌套 dispatch（sigprocmask 补投/raise 即时）
/// 或下一轮取走。**每实例至多投一次**：嵌套投的实例来自嵌套自己的
/// take，本轮位对应本轮已 take 的实例，一一对应；**已知偏差**（冻结
/// plan 时代既有，§3.4 偏差清单补记）：handler 执行期对同号的新 raise
/// 与外层快照原实例各投一次——POSIX 2.4.1 待决实例合并不跨嵌套窗口
/// 成立（Linux 内核 pending 位语义合而为一）。全 Hold 轮零进展即返
/// ——held 已持久化，无新 take 不会再有进展（progressed 防死循环）。
pub fn dispatch_pending() -> u32 {
    let mut delivered: u32 = 0;
    loop {
        let total = take_total();
        if total == 0 {
            return delivered;
        }
        // 第一遍（可见性，头注）：Hold 位即刻持久化进 held——嵌套投递
        // （handler 内 UNBLOCK 补投）take_total 归并 held 才能取到「已
        // take 未处置」的阻塞位；其余进工作集（第二遍重查，本遍分诊
        // 结果不携带）。
        let mut work: SigMask = 0;
        let mut bits = total;
        while bits != 0 {
            // 位号+1 ∈ 1..=64（total 非 0 保证 trailing_zeros ≤ 63——
            // classify 的 sig 界前置条件按构造成立，不再付费复检）。
            let sig = (bits.trailing_zeros() + 1) as i32;
            bits &= bits - 1;
            // SAFETY: SIG_MASK/SIG_ACTIONS/SIG_HELD 单线程口径直访（头注）。
            if let Act::Hold = unsafe { classify(sig, SIG_MASK, &SIG_ACTIONS) } {
                unsafe { SIG_HELD |= 1u64 << (sig - 1) as u32 };
            } else {
                work |= 1u64 << (sig - 1) as u32;
            }
        }
        // 第二遍（delivery-time，头注）：工作集升序逐位重查。
        let mut progressed = false;
        let mut bits = work;
        while bits != 0 {
            let sig = (bits.trailing_zeros() + 1) as i32;
            bits &= bits - 1;
            // 按步重查：每步重读掩码与表（头注——表变更即时生效、掩码
            // 变更经包络恢复只对嵌套面生效）。
            // SAFETY: 同第一遍。
            let act = unsafe { classify(sig, SIG_MASK, &SIG_ACTIONS) };
            match act {
                // handler 内 block 的后续位（mid-round Call→Hold 转移）：
                // 转落 held，unblock 后补投（第一遍之外的唯一 Hold 来源）。
                Act::Hold => {
                    // SAFETY: 同上。
                    unsafe { SIG_HELD |= 1u64 << (sig - 1) as u32 };
                }
                // IGN/DFL-Drop：消费即进展（POSIX ignored 信号不唤醒
                // pause 的语义面）。
                Act::Drop => progressed = true,
                Act::Call(f) => {
                    let bit = 1u64 << (sig - 1) as u32;
                    // Call 包络 = 全量快照恢复（sigreturn 真口径，头注）。
                    // SAFETY: 包络写读（单线程口径）。
                    let saved_mask = unsafe { SIG_MASK };
                    unsafe { SIG_MASK = saved_mask | bit };
                    // SAFETY: 表槽值 ≥2 = fn 地址（三值编码契约，
                    // berry-signal ActionTable 头注）。
                    let handler: extern "C" fn(c_int) =
                        unsafe { core::mem::transmute(f) };
                    // POSIX 钦定：handler 不见调用方 errno。
                    let saved_errno = crate::get_errno();
                    handler(sig);
                    crate::set_errno(saved_errno);
                    // 全量恢复：handler 内 sigprocmask 改动不越 handler
                    // 存活（Linux sigreturn 同形；修复轮翻正差量保留档）。
                    // SAFETY: 同上。
                    unsafe { SIG_MASK = saved_mask };
                    // SAFETY: 单调计数器（pause 纪元面）。
                    unsafe { SIG_DELIVERED += 1 };
                    delivered += 1;
                    progressed = true;
                }
                // DFL 终止档：`io::exit` = EOF 先行 + 0x33 真路径（Plan B
                // v0.5 契约；128+sig = shell `$?` 惯例码）。
                // SIGABRT 特例（A7-4 §3.2 abort 口径，修复轮补）：默认
                // 终止档同样汇入「console 诊断 + EOF + exit(134)」终止
                // 路径——`io::exit` 已 EOF 先行，此处只补 console 诊断
                // （对抗审查 finding：原直接 exit 无 `abort\n`——d-1c
                // 「bare abort 静默死」排障锚丢失，io.rs abort 尾兜只
                // 罩 Call-返回档）。
                Act::Terminate => {
                    if sig == SIGABRT {
                        crate::io::console_write(b"abort\n");
                    }
                    crate::io::exit(128 + sig)
                }
            }
        }
        if !progressed {
            return delivered; // 全 Hold：无新量不再轮
        }
    }
}

/// 投递点惯用入口（§3.3 五点：read/write/clock_nanosleep/futex wait/
/// pthread_join 返回后）。v1 无「pending 非空」用户态速查面——每点一次
/// take syscall（相对 console 通道 RPC 往返可忽略）。
#[inline]
pub fn may_dispatch() {
    dispatch_pending();
}

/// raise 真身（段③b）：self 发送（h=0 零 handle 面）+ 即时投递——
/// POSIX：raise ≡ kill(getpid(), sig)，终止档在 dispatch 内 exit 不返。
#[no_mangle]
pub extern "C" fn raise(sig: c_int) -> c_int {
    if !berry_signal::valid_sig(sig) {
        return reject(berry_signal::EINVAL);
    }
    let s = berry_signal_send(0, sig as u32);
    if s != 0 {
        // 界检已前置——残余失败 = 内核拒面（libc 语境不可达），诚实 EPERM。
        crate::set_errno(crate::EPERM);
        return -1;
    }
    dispatch_pending();
    0
}

/// abort 专属解锁（A7-4 §3.2，修复轮补——对抗审查 finding）：POSIX/C11
/// 钦定 abort 须**先解除 SIGABRT 阻塞**再 raise——原形态下 block 态的
/// raise 只落 held 集、handler 结构性永不执行（musl `__libc_sigaction`
/// 前的 `__restore` 同义处理）。只清掩码存储位；旧 pending（held 位 +
/// 内核位）由随后的 raise→`dispatch_pending` 经 [`take_total`] 归并
/// 一并送达（合并语义 = 至多投一次，POSIX 标准信号口径）。
pub fn abort_unblock_sigabrt() {
    let bit = 1u64 << (SIGABRT - 1) as u32;
    // SAFETY: SIG_MASK 单线程口径直访（头注）。
    unsafe { SIG_MASK &= !bit };
}

/// kill 真身（段③b）：v1 仅自杀档（pid == getpid() 定值锚 42）。
/// sig==0 探活档零内核调用（POSIX kill(pid,0)）；非 self 诚拒 ESRCH
/// （无 pid 面 = 确无此进程，语义真于旧 EPERM 桩）。stubs.rs 桩退役。
#[no_mangle]
pub extern "C" fn kill(pid: crate::pid_t, sig: c_int) -> c_int {
    if sig == 0 {
        return match berry_signal::kill_probe(pid) {
            Ok(()) => 0,
            Err(e) => reject(e),
        };
    }
    if !berry_signal::valid_sig(sig) {
        return reject(berry_signal::EINVAL);
    }
    if !berry_signal::kill_targets_self(pid) {
        return reject(berry_signal::ESRCH);
    }
    raise(sig) // 自杀档 ≡ raise（POSIX 等价式）
}

/// killpg 真身（段③b）：v1 无进程组面——诚实 ESRCH（「无此进程组」：
/// 不是权限不够，是组根本不存在）。stubs.rs 桩退役。
#[no_mangle]
pub extern "C" fn killpg(_pgrp: crate::pid_t, _sig: c_int) -> c_int {
    reject(berry_signal::ESRCH)
}

/// pause 真身（段③b，sys.rs 假 EINTR 桩迁此）：v1 睡不可中断——补偿
/// 形态 = 100ms 轮询档：usleep 尾的 may_dispatch 投掉窗内到达的信号，
/// `SIG_DELIVERED` 纪元变动（= 有 handler 真跑过）即返 EINTR。只有
/// handler 投递算唤醒（POSIX：ignored 信号不醒 pause；blocked 信号
/// 恒 Hold 不动计数；DFL 终止档在 dispatch 内 exit 不返）。
#[no_mangle]
pub extern "C" fn pause() -> c_int {
    dispatch_pending(); // 入口先排一轮（睡前已 pending 的信号即投）
    // SAFETY: 单线程口径（头注）。
    let before = unsafe { SIG_DELIVERED };
    loop {
        crate::sys::usleep(100_000);
        // SAFETY: 同上。
        if unsafe { SIG_DELIVERED } != before {
            break;
        }
    }
    crate::set_errno(crate::EINTR);
    -1
}
