//! 输出/stdio —— fd 0/1/2 → console channel（真实现档）
//!
//! fd 模型（v1）：只有 0/1/2；1/2 = console 发端别名（写方向）、0 = env
//! 块 stdin 端（d-3b 起真输入——channel recv，零长消息 = EOF 约定），都经
//! **env 块**交付（d-3a，见 `berry_libc_boot`）；其余 fd 一律 -EBADF。
//!
//! 发送契约（G-eco-b 教训的适用面）：**自旋重发**——`-EBUSY`（console 队列
//! 满）原地自旋重试，不睡不等（EL0 可抢占 = 自旋有界；asm svcmgr 本体
//! `send_spin` 同形）。分片 ≤192B（channel inline 帽 256）。

use core::ptr;

use berry_sys::{berry_channel_recv, berry_channel_send, berry_clock_read,
                berry_object_wait_one, CLOCK_MONOTONIC, SIGNAL_READABLE};

use crate::{c_char, c_int, c_long, c_void, size_t, ssize_t, FILE};

static mut CONSOLE_H: u32 = 0;
static mut STDIN_H: u32 = 0; // d-3b 预留（env 块 handles[1]；read 面随后开）
static mut STACK_TOP: usize = 0;

// —— argv 面（d-3a env 块）：静态落位（名区 256B + 16 槽 + NULL 终结）——
static mut ARGV_STR: [u8; 256] = [0; 256];
static mut ARGV_PTR: [*const c_char; 17] = [ptr::null(); 17];

/// crt0 装配口：`_start` 汇编 `ldr x1, [BERRY_ARGV]` 取 argv 数组基址。
#[no_mangle]
pub static mut BERRY_ARGV: *const *const c_char = ptr::null();

/// crt0 落种口（`_start` 汇编直调）：x0 = **env 通道端**（bootstrap），
/// x1 = 初始栈顶（attr 谎报面数据源）。
///
/// env 块协议（BerryOS processargs v1，Q2 之答——Zircon 同形的最小面）：
/// svcmgr 在 `process_start` **前**把环境消息压进 env 端队列（确定性先行，
/// 无竞速）——payload = NUL 分隔 argv 字节串（首项 = 程序名）、随行
/// handles 按位语义：[0] = console 发端别名（stdout/stderr）、[1] = stdin
/// 端（d-3b）。解析落 static 后返回 argc（argv 基址经 `BERRY_ARGV` 交汇编）。
/// 协议破缺（recv 失败/无 handle）→ argc=0 + console=0：后续任何打印在
/// handle 0 上自旋死 → 标记缺失 = smoke FAIL 兜底（loud 不静默）。
#[no_mangle]
pub extern "C" fn berry_libc_boot(env: u32, sp0: usize) -> c_int {
    unsafe {
        STACK_TOP = sp0;
        stderr = ptr::addr_of_mut!(STDERR_FILE);
        stdout = ptr::addr_of_mut!(STDOUT_FILE);
        let mut blob = [0u8; 256];
        let mut nb = blob.len();
        let mut hs = [0u32; 2];
        let mut nh = hs.len();
        if berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh) != 0 {
            return 0;
        }
        if nh >= 1 {
            CONSOLE_H = hs[0];
        }
        if nh >= 2 {
            STDIN_H = hs[1];
        }
        // argv 解析：NUL 分隔字节串逐个拷进名区、记指针。
        let (mut argc, mut i, mut o) = (0usize, 0usize, 0usize);
        while i < nb && argc < 16 {
            let start = o;
            while i < nb && blob[i] != 0 {
                if o < ARGV_STR.len() {
                    ARGV_STR[o] = blob[i];
                    o += 1;
                }
                i += 1;
            }
            if i >= nb {
                break; // 尾串无 NUL：不完整，丢弃（svcmgr 侧恒补 NUL）
            }
            if o < ARGV_STR.len() {
                ARGV_STR[o] = 0;
                o += 1;
            }
            i += 1;
            ARGV_PTR[argc] = ARGV_STR.as_ptr().add(start);
            argc += 1;
        }
        ARGV_PTR[argc] = ptr::null();
        BERRY_ARGV = ARGV_PTR.as_ptr();
        argc as c_int
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

// —— stdin 面（d-3b）：fd 0 → env 块 stdin 端（channel recv）——
// EOF 约定 = **零长消息**（feeder 发完数据压一条 0 字节消息；v1 无对端
// 关闭检测——3-4b 诚实缺口，消息层约定补）。消息粒度 ≠ read 粒度：一次
// read 要得少，残段留 PEND 静态缓冲下次先出。
static mut PEND: [u8; 256] = [0; 256];
static mut PEND_LEN: usize = 0;
static mut PEND_OFF: usize = 0;

#[no_mangle]
pub extern "C" fn read(fd: c_int, buf: *mut c_void, n: size_t) -> ssize_t {
    unsafe {
        if fd != 0 {
            crate::set_errno(crate::EBADF); // 1/2 只写：读方向诚拒
            return -1;
        }
        if n == 0 {
            return 0;
        }
        // 残段优先（上一条消息没读完的部分）。
        if PEND_LEN > PEND_OFF {
            let avail = PEND_LEN - PEND_OFF;
            let c = core::cmp::min(n, avail);
            ptr::copy_nonoverlapping(PEND.as_ptr().add(PEND_OFF), buf as *mut u8, c);
            PEND_OFF += c;
            return c as ssize_t;
        }
        let h = STDIN_H;
        if h == 0 {
            return 0; // env 块无 stdin 端 = /dev/null 语义
        }
        let mut msg = [0u8; 256];
        let mut nhd = 0usize;
        let mut deadline = 0u64;
        loop {
            let mut nb = msg.len();
            let s = berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, ptr::null_mut(), &mut nhd);
            if s == 0 {
                if nb == 0 {
                    return 0; // EOF 约定：零长消息
                }
                let c = core::cmp::min(n, nb);
                ptr::copy_nonoverlapping(msg.as_ptr(), buf as *mut u8, c);
                if nb > c {
                    let rest = nb - c; // ≤ 256 = PEND 容量
                    PEND[..rest].copy_from_slice(&msg[c..nb]);
                    PEND_LEN = rest;
                    PEND_OFF = 0;
                }
                return c as ssize_t;
            }
            // 空队：阻塞等 READABLE（10s 帽）。超时 = feeder 断供（协议破缺）
            // → EOF 化返 0：调用方读短 → 断言炸 → 标记缺失兜底（loud）。
            if deadline == 0 {
                let mut now = 0u64;
                if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                    crate::set_errno(crate::EINVAL);
                    return -1;
                }
                deadline = now + 10_000_000_000;
            }
            let mut obs = 0u64;
            if berry_object_wait_one(h, SIGNAL_READABLE, deadline, &mut obs) != 0 {
                return 0;
            }
        }
    }
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
