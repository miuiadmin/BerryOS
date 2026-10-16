//! 输出/stdio —— fd 0/1/2 → console channel（真实现档）
//!
//! fd 模型（v1）：只有 0/1/2，全指向**同一个** console 发端别名（bootstrap
//! 传入、crt0 落种）；其余 fd 一律 -EBADF。读方向 v1 无（stdin 恒 EOF，
//! d-3 bootstrap 消息协议再开）。
//!
//! 发送契约（G-eco-b 教训的适用面）：**自旋重发**——`-EBUSY`（console 队列
//! 满）原地自旋重试，不睡不等（EL0 可抢占 = 自旋有界；asm svcmgr 本体
//! `send_spin` 同形）。分片 ≤192B（channel inline 帽 256）。

use core::ptr;

use berry_sys::berry_channel_send;

use crate::{c_char, c_int, c_long, c_void, size_t, ssize_t, FILE};

static mut CONSOLE_H: u32 = 0;
static mut STACK_TOP: usize = 0;

/// crt0 落种口（`_start` 汇编直调）：bootstrap handle = console 发端别名
/// + 初始栈顶（attr 谎报面数据源）。`stderr`/`stdout` 指针一并落位
/// （const 求值里取 static mut 地址不稳，运行期一次赋值最直白）。
#[no_mangle]
pub extern "C" fn berry_libc_boot(console: u32, sp0: usize) {
    unsafe {
        CONSOLE_H = console;
        STACK_TOP = sp0;
        stderr = ptr::addr_of_mut!(STDERR_FILE);
        stdout = ptr::addr_of_mut!(STDOUT_FILE);
    }
}

/// main 意外返回后的收口（crt0 跳入）。
#[no_mangle]
pub extern "C" fn berry_libc_park_after_main() -> ! {
    crate::park()
}

pub fn stack_top() -> usize {
    unsafe { STACK_TOP }
}

/// console 发送：分片 + EBUSY 自旋（见模块头契约）。
pub fn console_write(bytes: &[u8]) {
    let mut off = 0usize;
    while off < bytes.len() {
        let n = core::cmp::min(192, bytes.len() - off);
        let p = unsafe { bytes.as_ptr().add(off) };
        loop {
            let s = berry_channel_send(unsafe { CONSOLE_H }, p, n, ptr::null(), 0);
            if s == 0 {
                break;
            }
            // -EBUSY（满）自旋重试；-EAGAIN 对 send 不发生；其余错误码
            // （如 handle 失效）自旋也无害——park 前无更好出路。
        }
        off += n;
    }
}

#[inline]
fn fd_ok(fd: c_int) -> bool {
    (0..=2).contains(&fd)
}

#[no_mangle]
pub extern "C" fn write(fd: c_int, buf: *const c_void, n: size_t) -> ssize_t {
    if !fd_ok(fd) {
        crate::set_errno(crate::EBADF);
        return -1;
    }
    if n == 0 {
        return 0;
    }
    let bytes = unsafe { core::slice::from_raw_parts(buf as *const u8, n) };
    console_write(bytes);
    n as ssize_t
}

#[no_mangle]
pub extern "C" fn writev(fd: c_int, iov: *const Iovec, cnt: c_int) -> ssize_t {
    if !fd_ok(fd) {
        crate::set_errno(crate::EBADF);
        return -1;
    }
    if cnt <= 0 {
        return 0;
    }
    // 逐 iov 直写（不对齐组装缓冲——std 的 writev 用于拼接，逐段语义等价）
    let mut total: ssize_t = 0;
    for i in 0..cnt as usize {
        let v = unsafe { &*iov.add(i) };
        if v.len > 0 {
            let bytes = unsafe { core::slice::from_raw_parts(v.base as *const u8, v.len) };
            console_write(bytes);
            total += v.len as ssize_t;
        }
    }
    total
}

#[repr(C)]
pub struct Iovec {
    pub base: *mut c_void,
    pub len: size_t,
}

#[no_mangle]
pub extern "C" fn read(fd: c_int, _buf: *mut c_void, _n: size_t) -> ssize_t {
    if !fd_ok(fd) {
        crate::set_errno(crate::EBADF);
        return -1;
    }
    0 // stdin 恒 EOF（d-3 开输入方向）
}

#[no_mangle]
pub extern "C" fn close(fd: c_int) -> c_int {
    if fd_ok(fd) {
        return 0; // console handle 是进程级资产，close(1) 不真关
    }
    crate::set_errno(crate::EBADF);
    -1
}

#[no_mangle]
pub extern "C" fn isatty(fd: c_int) -> c_int {
    if !fd_ok(fd) {
        crate::set_errno(crate::EBADF);
        return 0;
    }
    crate::set_errno(crate::ENOTTY);
    0 // v1 诚实面：非 tty（hexyl 因之关配色，输出更稳）
}

// —— FILE / stdio ——

static mut STDERR_FILE: FILE = FILE { fd: 2, _pad: 0 };
static mut STDOUT_FILE: FILE = FILE { fd: 1, _pad: 0 };

/// musl 形状：`stderr`/`stdout` 是值为 `FILE *` 的全局变量（boot 落种，
/// 见 `berry_libc_boot`）。
#[no_mangle]
pub static mut stderr: *mut FILE = ptr::null_mut();
#[no_mangle]
pub static mut stdout: *mut FILE = ptr::null_mut();

#[no_mangle]
pub extern "C" fn fileno(f: *mut FILE) -> c_int {
    if f.is_null() {
        return -1;
    }
    unsafe { (*f).fd }
}

#[no_mangle]
pub extern "C" fn fdopen(fd: c_int, _mode: *const c_char) -> *mut FILE {
    if !fd_ok(fd) {
        crate::set_errno(crate::EBADF);
        return ptr::null_mut();
    }
    let p = crate::alloc::malloc(core::mem::size_of::<FILE>());
    if p.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        *(p as *mut FILE) = FILE { fd, _pad: 0 };
    }
    p as *mut FILE
}

#[no_mangle]
pub extern "C" fn fwrite(ptr_: *const c_void, size: size_t, nmemb: size_t, f: *mut FILE) -> size_t {
    if f.is_null() || size == 0 || nmemb == 0 {
        return 0;
    }
    let fd = unsafe { (*f).fd };
    if !fd_ok(fd) {
        crate::set_errno(crate::EBADF);
        return 0;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr_ as *const u8, size * nmemb) };
    console_write(bytes);
    nmemb
}

#[no_mangle]
pub extern "C" fn fflush(_f: *mut FILE) -> c_int {
    0 // 直通写，无缓冲可冲
}

#[no_mangle]
pub extern "C" fn fclose(_f: *mut FILE) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn feof(_f: *mut FILE) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn fread(_p: *mut c_void, _size: size_t, _nmemb: size_t, _f: *mut FILE) -> size_t {
    0
}

#[no_mangle]
pub extern "C" fn fseek(_f: *mut FILE, _off: c_long, _whence: c_int) -> c_int {
    crate::set_errno(crate::ESPIPE);
    -1
}

#[no_mangle]
pub extern "C" fn ftell(_f: *mut FILE) -> c_long {
    crate::set_errno(crate::ESPIPE);
    -1
}

// —— 死亡面（exit 族：park 不 trap——watchdog 错杀断言契约）——

#[no_mangle]
pub extern "C" fn exit(_code: c_int) -> ! {
    crate::park()
}

#[no_mangle]
pub extern "C" fn _exit(_code: c_int) -> ! {
    crate::park()
}

#[no_mangle]
pub extern "C" fn abort() -> ! {
    console_write(b"abort\n");
    crate::park()
}

/// atexit 注册表（32 槽，登记不执行——std 的 flush 在 lang_start 收尾侧，
/// 不经 atexit；留表是为了「谎报成功」的同时不砸未来真调用者）。
const ATEXIT_MAX: usize = 32;
static mut ATEXIT_TAB: [usize; ATEXIT_MAX] = [0; ATEXIT_MAX];
static mut ATEXIT_N: usize = 0;

#[no_mangle]
pub extern "C" fn atexit(f: extern "C" fn()) -> c_int {
    unsafe {
        if ATEXIT_N < ATEXIT_MAX {
            ATEXIT_TAB[ATEXIT_N] = f as usize;
            ATEXIT_N += 1;
            return 0;
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn __assert_fail(
    expr: *const c_char,
    file: *const c_char,
    line: c_int,
    _func: *const c_char,
) -> ! {
    console_write(b"assert failed: ");
    unsafe {
        let e = core::slice::from_raw_parts(expr as *const u8, crate::strmem::strlen(expr));
        console_write(e);
        console_write(b" at ");
        let f = core::slice::from_raw_parts(file as *const u8, crate::strmem::strlen(file));
        console_write(f);
    }
    console_write(b":");
    let mut buf = [0u8; 24];
    let s = crate::vprintf::utoa_dec(line as u64, &mut buf);
    console_write(s);
    console_write(b"\n");
    crate::park()
}
