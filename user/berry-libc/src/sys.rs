//! clock/身份/信息面 —— 真实现与半真实现档
//!
//! 真实现：`__errno_location`、clock 族（A7-2 分档：单调族直读 berry
//! 单调钟、REALTIME 族 = EPOCH_BASE(0)+单调 纯合成——绝对值非真历法，
//! 诚实档）、time/gettimeofday/settimeofday/nanosleep/usleep（A7-2 补桩
//! 真身）、getrandom（xorshift，时钟播种——RandomState 可用）、
//! gethostname/uname/sysconf/getpagesize/sched_getaffinity（填真值或
//! 合理假值）。
//! 假值档：getpid/getppid/getuid（无进程 ID 概念的诚实常数）。
//! timer_create/settime/gettime/delete 族维持**零符号**（A7-0 #10 深水
//! 后置：内核对象或 futex 线程模拟，零在册消费者）。

use core::ptr;

use berry_sys::{berry_clock_read, berry_thread_sleep, CLOCK_MONOTONIC};

use crate::{c_char, c_int, c_long, c_uint, c_ulong, c_void, size_t, time_t};

#[no_mangle]
pub extern "C" fn __errno_location() -> *mut c_int {
    ptr::addr_of_mut!(crate::ERRNO)
}

#[repr(C)]
pub struct Timespec {
    pub tv_sec: time_t,
    pub tv_nsec: c_long,
}

// —— A7-2 clockid 双轨（`docs/事件循环A7-2实施设计草案.md` §3.1/§1 #8）——
// 调用侧事实源 = libc crate（crates.io）携 **Linux/musl 编号**；berry-sys
// `CLOCK_MONOTONIC`=0 是内核侧编号（仅作 berry_clock_read 实参）——两轨
// 勿混（REALTIME=0 与 berry MONOTONIC=0 数值撞位，分诊表按 Linux 值收参）。
const CLOCK_REALTIME: c_int = 0;
const CLOCK_MONOTONIC_LINUX: c_int = 1;
const CLOCK_MONOTONIC_RAW: c_int = 4;
const CLOCK_REALTIME_COARSE: c_int = 5;
const CLOCK_MONOTONIC_COARSE: c_int = 6;
const CLOCK_BOOTTIME: c_int = 7;

/// 壁钟基（ns，epoch 计）——v1 **静态零写者**：壁钟 = epoch 0 + 单调
/// 运行时（一致性满足：单调非减、跨调用一致；绝对值非真历法）。per-process
/// 静态 + 零写者 = 跨进程差异零可观测，无需全局钟服务；v2 随 RTC/时钟
/// 服务引入写者（届时 clock_settime 立场同步重裁，见下 clock_settime 注）。
static EPOCH_BASE_NS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// A7-2 分诊表：mono 族（1/4/6/7）直读（v1 无 NTP/挂起面——raw≡mono、
/// boottime≡mono、coarse=粒度承诺差异非值差异，三档诚实近似）；REALTIME
/// 族（0/5）= base+mono 合成（base=0 = 分档前行为逐 ns 同形，零漂移）；
/// 其它（CPUTIME/TAI/未知/负值）→ -1+EINVAL 诚实拒（首个真实 errno 路径
/// ——现册消费面 grep 实证仅 REALTIME/MONO，冷门 id 真消费者出现时按
/// 现探现补扩臂）。
#[no_mangle]
pub extern "C" fn clock_gettime(clk: c_int, ts: *mut Timespec) -> c_int {
    let mut now: berry_sys::Time = 0;
    if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
        return -1; // 结构性不可达（内核对单调钟恒成功）；保持无 errno 形态
    }
    let ns = match clk {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE => {
            now.saturating_add(EPOCH_BASE_NS.load(core::sync::atomic::Ordering::Relaxed))
        }
        CLOCK_MONOTONIC_LINUX | CLOCK_MONOTONIC_RAW | CLOCK_MONOTONIC_COARSE
        | CLOCK_BOOTTIME => now,
        _ => {
            crate::set_errno(crate::EINVAL);
            return -1;
        }
    };
    unsafe {
        (*ts).tv_sec = (ns / 1_000_000_000) as time_t;
        (*ts).tv_nsec = (ns % 1_000_000_000) as c_long;
    }
    0
}

/// clock_getres（uu_date 的 `--resolution`/周期探针链接面，C-2c 挂账刀）：
/// 谎报 1ns（tv_sec=0/tv_nsec=1）+ 成功——**必须成功**：上游（rustix→
/// chrono 链）对 Err 走 unwrap/expect 档，失败 = panic 死进程（ENOSYS
/// 桩会经此把 date 整体杀掉）；「分辨率 1ns」是 berry 单调钟 ns 粒度的
/// 诚实值，非任意假值。
/// A7-2 §3.4 维持决策：**不**随分诊表收紧——getres 查询面广于 gettime
/// （持冷门 id 的未知消费者走 getres 探针比 gettime 更常见），收紧 =
/// 无收益引入 panic 面。
#[no_mangle]
pub extern "C" fn clock_getres(_clk: c_int, ts: *mut Timespec) -> c_int {
    unsafe {
        (*ts).tv_sec = 0;
        (*ts).tv_nsec = 1;
    }
    0
}

/// clock_settime（uu_date `-s` 设置面）：无特权时钟写面，EPERM 拒——
/// date 对 -s 失败打诊断退非零（诚实档；`-s` 不在 u4 自验步，运行期
/// 不可达 = P5 探针实证，桩只守链接面）。
/// A7-2 §3.3 维持决策：**拒 per-process 假成功面**——REALTIME set 若做
/// 成写本进程 EPOCH_BASE = 「设置成功」文案 + 零全局效果（进程退出即
/// 蒸发），比 EPERM 恶劣；MONOTONIC set 同拒（musl/Linux 真值 EINVAL，
/// 差异零消费面不顺手改）。v2 随全局钟服务重裁。
#[no_mangle]
pub extern "C" fn clock_settime(_clk: c_int, _ts: *const Timespec) -> c_int {
    crate::set_errno(crate::EPERM);
    -1
}

/// std::thread::sleep 的落点；`flags & 1` = TIMER_ABSTIME（musl 值 1）。
/// A7-2 §3.5 补笔：`_clk` 维持忽略——REALTIME+TIMER_ABSTIME（壁钟绝对
/// 睡）在 base=0 下 REALTIME 值 ≡ mono 值，行为不变；v2 非零基才需
/// `deadline = d − base` 换算（零在册消费者用 REALTIME 绝对睡，届时
/// 随写者面一起裁）。
#[no_mangle]
pub extern "C" fn clock_nanosleep(
    _clk: c_int,
    flags: c_int,
    req: *const Timespec,
    _rem: *mut Timespec,
) -> c_int {
    unsafe {
        let d = (*req).tv_sec as u64 * 1_000_000_000 + (*req).tv_nsec as u64;
        let deadline = if flags & 1 != 0 {
            d
        } else {
            let mut now: berry_sys::Time = 0;
            if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                return -1; // 语义上应为 errno 正值，但时钟不可读无路可走
            }
            now.saturating_add(d)
        };
        let _ = berry_thread_sleep(deadline);
    }
    // A7-4 ③b：睡醒补投（§3.3 单点——nanosleep/usleep/pause 轮询档全经
    // 此；睡本身不可中断，信号处置移到醒来后）。
    crate::signal::may_dispatch();
    0
}

// —— A7-2 补桩真身五件（§3.6；time_t/suseconds_t = 8B 全宽出参——A7-0 #4
// 出参宽度纪律：非 handle 桶不收 4B 截写）——

/// musl struct timeval（aarch64：两 8B 字段；两字段同宽无互挤面，
/// 钉板从简——C-2a Stat 立法档注记）。
#[repr(C)]
pub struct Timeval {
    pub tv_sec: time_t,
    pub tv_usec: crate::suseconds_t,
}

/// REALTIME 档秒；出参非空回填同值（POSIX 契约）。
#[no_mangle]
pub extern "C" fn time(t: *mut time_t) -> time_t {
    let mut ts = Timespec { tv_sec: 0, tv_nsec: 0 };
    clock_gettime(CLOCK_REALTIME, &mut ts);
    let s = ts.tv_sec;
    if !t.is_null() {
        unsafe { *t = s };
    }
    s
}

/// tv = REALTIME 档 μs；tz 非空写全零（POSIX obsolete 容忍档——musl
/// 同形，防调用方栈残留读回）。
#[no_mangle]
pub extern "C" fn gettimeofday(tv: *mut Timeval, tz: *mut c_void) -> c_int {
    let mut ts = Timespec { tv_sec: 0, tv_nsec: 0 };
    clock_gettime(CLOCK_REALTIME, &mut ts);
    unsafe {
        (*tv).tv_sec = ts.tv_sec;
        (*tv).tv_usec = (ts.tv_nsec / 1_000) as crate::suseconds_t;
        if !tz.is_null() {
            // struct timezone = 2 × c_int = 8B
            crate::strmem::memset_volatile(tz as *mut u8, 0, 8);
        }
    }
    0
}

/// §3.3 立场对称：per-process 假成功面拒（同 clock_settime EPERM）。
#[no_mangle]
pub extern "C" fn settimeofday(_tv: *const Timeval, _tz: *const c_void) -> c_int {
    crate::set_errno(crate::EPERM);
    -1
}

/// 包装 clock_nanosleep；rem 非空先清零（残余量恒 0 的诚实形态——v1 睡
/// 不可中断，A7-4 ③b 后窗内到达的信号由 nanosleep 链尾投递点补投）。
#[no_mangle]
pub extern "C" fn nanosleep(req: *const Timespec, rem: *mut Timespec) -> c_int {
    if !rem.is_null() {
        unsafe {
            (*rem).tv_sec = 0;
            (*rem).tv_nsec = 0;
        }
    }
    clock_nanosleep(CLOCK_MONOTONIC_LINUX, 0, req, rem)
}

/// μs 档（POSIX 无 errno 面，恒 0——同不可中断档；A7-4 ③b 起 pause
/// 轮询与 std sleep 面的投递载体）。
#[no_mangle]
pub extern "C" fn usleep(usec: c_uint) -> c_int {
    let req = Timespec {
        tv_sec: (usec / 1_000_000) as time_t,
        tv_nsec: ((usec % 1_000_000) as c_long) * 1_000,
    };
    nanosleep(&req, ptr::null_mut())
}

// pause 真身 = signal.rs（A7-4 ③b 迁出：假 EINTR 桩退役——100ms 轮询
// 档 + SIG_DELIVERED 纪元检测）。

/// getpid() 定值锚（「无进程 ID 概念的诚实常数」档）——提 pub const
/// 供 berry-signal `GETPID_MIRROR` 跨 crate const assert 钉板
/// （signal.rs，两值漂移编译期红）；kill 自杀判定锚同源（A7-4 §3.2）。
pub const SELF_PID: c_int = 42;

#[no_mangle]
pub extern "C" fn getpid() -> c_int {
    SELF_PID
}

#[no_mangle]
pub extern "C" fn getppid() -> c_int {
    1
}

#[no_mangle]
pub extern "C" fn getuid() -> u32 {
    100
}

#[no_mangle]
pub extern "C" fn getpagesize() -> c_int {
    4096
}

// musl _SC 值（bits/confname.h）里 std 真用到的两个
const _SC_PAGESIZE: c_int = 30;
const _SC_NPROCESSORS_ONLN: c_int = 84;

#[no_mangle]
pub extern "C" fn sysconf(name: c_int) -> c_long {
    match name {
        _SC_PAGESIZE => 4096,
        _SC_NPROCESSORS_ONLN => 1,
        _ => {
            crate::set_errno(crate::EINVAL);
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn sched_getaffinity(_pid: c_int, size: size_t, set: *mut c_void) -> c_int {
    unsafe {
        let n = size.min(128);
        crate::strmem::memset_volatile(set as *mut u8, 0, n);
        if n > 0 {
            ptr::write_volatile(set as *mut u8, 0x01); // cpu 0 在集
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn sched_yield() -> c_int {
    0 // 单核 prio-0 大家庭：真 yield 无 berry syscall 面，空转即可
}

static HOSTNAME: [c_char; 8] = *b"berryos\0";

#[no_mangle]
pub extern "C" fn gethostname(name: *mut c_char, len: size_t) -> c_int {
    if len < HOSTNAME.len() {
        crate::set_errno(crate::ERANGE);
        return -1;
    }
    unsafe {
        crate::strmem::memcpy_volatile(name as *mut u8, HOSTNAME.as_ptr(), HOSTNAME.len());
    }
    0
}

/// musl struct utsname = 6 × 65B 字段。
#[repr(C)]
pub struct Utsname {
    fields: [[c_char; 65]; 6],
}

static UNAME_SYSNAME: [c_char; 8] = *b"BerryOS\0";
static UNAME_NODENAME: [c_char; 6] = *b"berry\0";
static UNAME_RELEASE: [c_char; 4] = *b"0.1\0";
static UNAME_VERSION: [c_char; 18] = *b"berry-libc-shim-1\0";
static UNAME_MACHINE: [c_char; 8] = *b"aarch64\0";

#[no_mangle]
pub extern "C" fn uname(u: *mut Utsname) -> c_int {
    unsafe {
        let srcs: [&[c_char]; 6] = [
            &UNAME_SYSNAME,
            &UNAME_NODENAME,
            &UNAME_RELEASE,
            &UNAME_VERSION,
            &UNAME_MACHINE,
            &UNAME_VERSION, // domainname（未用字段，填无害值）
        ];
        for (i, s) in srcs.iter().enumerate() {
            let f = (*u).fields[i].as_mut_ptr();
            crate::strmem::memset_volatile(f, 0, 65);
            crate::strmem::memcpy_volatile(f, s.as_ptr(), s.len());
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn getauxval(_at: c_ulong) -> c_ulong {
    0
}

#[no_mangle]
pub extern "C" fn dl_iterate_phdr(
    _cb: extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    _data: *mut c_void,
) -> c_int {
    0 // backtrace 静默降级（零 phdr 帧）
}

#[no_mangle]
pub extern "C" fn dladdr(_addr: *const c_void, _info: *mut c_void) -> c_int {
    0
}

/// 弱面真实现（附录 A：弱缺省 0 合法，但真随机更好——HashMap 播种可用）。
#[no_mangle]
pub extern "C" fn getrandom(buf: *mut c_void, len: size_t, _flags: c_uint) -> isize {
    if len == 0 {
        return 0;
    }
    unsafe {
        let mut now: berry_sys::Time = 0;
        let _ = berry_clock_read(CLOCK_MONOTONIC, &mut now);
        // 时钟 + 自身地址扰动播种的 xorshift64*
        let mut x = now ^ (buf as u64).rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15;
        let bytes = core::slice::from_raw_parts_mut(buf as *mut u8, len);
        let mut i = 0;
        while i < len {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
            let n = core::cmp::min(8, len - i);
            for k in 0..n {
                bytes[i + k] = (r >> (k * 8)) as u8;
            }
            i += 8;
        }
    }
    len as isize
}

// —— 弱面顺手件（附录 A weak 24 里有真消费者的一对）——

// T-1：`__cxa_thread_atexit_impl` 桩已删（曾在此返 0 吞 TLS dtor 注册）。
// std 1.95 对它是**弱引用**（GOT null-check → cbz 分叉，rlib 反汇编实证）：
// 桩在场 = std 走桩不跑析构；桩缺席 = 落 std 自家 StaticList 注册路径
// （经 `mrs TPIDR_EL0` 走链表——与拷贝式 TLS 完全兼容），线程退出时
// 析构真实执行。删桩是严格改进，勿复活。

#[no_mangle]
pub static __dso_handle: c_uint = 0;
