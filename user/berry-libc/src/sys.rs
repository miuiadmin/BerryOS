//! clock/身份/信息面 —— 真实现与半真实现档
//!
//! 真实现：`__errno_location`、clock_gettime/nanosleep（berry 单调钟）、
//! getrandom（xorshift，时钟播种——RandomState 可用）、gethostname/uname/
//! sysconf/getpagesize/sched_getaffinity（填真值或合理假值）。
//! 假值档：getpid/getppid/getuid（无进程 ID 概念的诚实常数）。

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

#[no_mangle]
pub extern "C" fn clock_gettime(_clk: c_int, ts: *mut Timespec) -> c_int {
    let mut now: berry_sys::Time = 0;
    if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
        return -1;
    }
    unsafe {
        (*ts).tv_sec = (now / 1_000_000_000) as time_t;
        (*ts).tv_nsec = (now % 1_000_000_000) as c_long;
    }
    0
}

/// std::thread::sleep 的落点；`flags & 1` = TIMER_ABSTIME（musl 值 1）。
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
    0
}

#[no_mangle]
pub extern "C" fn pause() -> c_int {
    let mut now: berry_sys::Time = 0;
    if berry_clock_read(CLOCK_MONOTONIC, &mut now) == 0 {
        let _ = berry_thread_sleep(now + 10_000_000_000);
    }
    crate::set_errno(crate::EINTR);
    -1
}

#[no_mangle]
pub extern "C" fn getpid() -> c_int {
    42
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

#[no_mangle]
pub extern "C" fn __cxa_thread_atexit_impl(
    _dtor: extern "C" fn(*mut c_void),
    _obj: *mut c_void,
    _dso: *mut c_void,
) -> c_int {
    0 // TLS 析构登记但不执行（进程 park 终态，无泄漏可观测面）
}

#[no_mangle]
pub static __dso_handle: c_uint = 0;
