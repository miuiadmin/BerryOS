//! ENOSYS/成功桩面 —— 附录 A「189 全定义」的机械档
//!
//! 三类宏（全部**零参**定义——AArch64 下被调方忽略多余寄存器实参合法，
//! 见 lib.rs ABI 口径；变参消费面走 vprintf 蹦床）：
//! - `neg!`：返 -1 + 置 errno（errno 取值跟 C 族语义：路径族 ENOENT、
//!   桩族 ENOSYS、fd 族 EBADF……）；
//! - `ok!`：成功谎报（无竞争锁族/特权空转/信号安装）；
//! - 少数签名敏感的（mmap/environ/posix_spawnp/getaddrinfo/pthread key 族）
//!   手写。
//!
//! 返回宽度纪律：int 族 `-> c_int`、ssize 族 `-> isize`、指针族 `-> *mut T`。

use core::ptr;

use crate::{c_char, c_int, c_void, size_t};

macro_rules! neg_int {
    ($($f:ident => $e:expr),* $(,)?) => {$(
        #[no_mangle]
        pub extern "C" fn $f() -> c_int {
            crate::set_errno($e);
            -1
        }
    )*};
}

macro_rules! neg_ssize {
    ($($f:ident => $e:expr),* $(,)?) => {$(
        #[no_mangle]
        pub extern "C" fn $f() -> isize {
            crate::set_errno($e);
            -1
        }
    )*};
}

macro_rules! ok {
    ($($f:ident),* $(,)?) => {$(
        #[no_mangle]
        pub extern "C" fn $f() -> c_int {
            0
        }
    )*};
}

// —— FS 路径面（ENOENT：std 侧报「文件不存在」比「未实现」对最终用户更诚实，
//    反正 v1 就是不存在）——
neg_int!(
    open => crate::ENOENT,
    openat => crate::ENOENT,
    opendir => crate::ENOENT,
    stat => crate::ENOENT,
    lstat => crate::ENOENT,
    readlink => crate::ENOENT,
    unlink => crate::ENOENT,
    unlinkat => crate::ENOENT,
    rmdir => crate::ENOENT,
    rename => crate::ENOENT,
    chdir => crate::ENOENT,
    mkdir => crate::ENOENT,
    mkfifo => crate::ENOENT,
    symlink => crate::ENOENT,
    linkat => crate::ENOENT,
    execvp => crate::ENOENT,
);

// —— FS 元数据/杂（ENOSYS）——
neg_int!(
    fstat => crate::ENOSYS,
    fchmod => crate::ENOSYS,
    fchown => crate::ENOSYS,
    chmod => crate::ENOSYS,
    chown => crate::ENOSYS,
    lchown => crate::ENOSYS,
    chroot => crate::ENOSYS,
    fsync => crate::ENOSYS,
    fdatasync => crate::ENOSYS,
    ftruncate => crate::ENOSYS,
    futimens => crate::ENOSYS,
    utimensat => crate::ENOSYS,
    flock => crate::ENOSYS,
    fcntl => crate::EINVAL,
    ioctl => crate::ENOTTY,
    mprotect => crate::EINVAL,
);

// —— fd/dir 族 ——
neg_int!(
    dup => crate::EBADF,
    dup2 => crate::EBADF,
    closedir => crate::EBADF,
    dirfd => crate::EBADF,
    waitpid => crate::ECHILD,
    waitid => crate::ECHILD,
    kill => crate::EPERM,
    killpg => crate::EPERM,
    setsid => crate::EPERM,
    fork => crate::ENOSYS,
);

// —— socket 族（ENOSYS 一刀切）——
neg_int!(
    socket => crate::ENOSYS,
    socketpair => crate::ENOSYS,
    connect => crate::ENOSYS,
    bind => crate::ENOSYS,
    listen => crate::ENOSYS,
    accept4 => crate::ENOSYS,
    shutdown => crate::ENOSYS,
    getsockopt => crate::ENOSYS,
    setsockopt => crate::ENOSYS,
    getpeername => crate::ENOSYS,
    getsockname => crate::ENOSYS,
    send => crate::ENOSYS,
    recv => crate::ENOSYS,
    recvfrom => crate::ENOSYS,
    sendto => crate::ENOSYS,
    sendmsg => crate::ENOSYS,
    recvmsg => crate::ENOSYS,
    pipe2 => crate::ENOSYS,
);

neg_ssize!(
    lseek => crate::ESPIPE,
    readv => crate::ENOSYS,
    pread => crate::ENOSYS,
    pwrite => crate::ENOSYS,
    preadv => crate::ENOSYS,
    pwritev => crate::ENOSYS,
    sendfile => crate::ENOSYS,
    splice => crate::ENOSYS,
);

// —— 成功谎报档（无竞争锁族/特权空转/信号安装——v1 无信号机制，
//    栈溢出 = EL0 fault 被杀可接受）——
ok!(
    madvise,
    munmap,
    prctl,
    setuid,
    setgid,
    setgroups,
    setpgid,
    setenv,
    unsetenv,
    sigaction,
    signal,
    sigaddset,
    sigemptyset,
    sigaltstack,
    pthread_rwlock_rdlock,
    pthread_rwlock_wrlock,
    pthread_rwlock_unlock,
    pthread_detach,
    pthread_setname_np,
    pthread_attr_init,
    pthread_attr_destroy,
    pthread_attr_setstacksize,
    posix_spawn_file_actions_init,
    posix_spawn_file_actions_addchdir_np,
    posix_spawn_file_actions_adddup2,
    posix_spawn_file_actions_destroy,
    posix_spawnattr_init,
    posix_spawnattr_destroy,
    posix_spawnattr_setflags,
    posix_spawnattr_setpgroup,
    posix_spawnattr_setsigdefault,
    freeaddrinfo,
);

// —— 手写面（签名/返回语义敏感）——

/// 变参真号槽（std 无竞争快路径 CAS 不经此；is_terminal 的 ioctl 落此 → -ENOSYS）。
#[no_mangle]
pub extern "C" fn syscall() -> isize {
    crate::set_errno(crate::ENOSYS);
    -1
}

/// 附录 A「引用点待实证」：v1 ENOMEM 拒绝——若首跑证明在启动关键路径再调档。
#[no_mangle]
pub extern "C" fn mmap() -> *mut c_void {
    crate::set_errno(crate::ENOMEM);
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn getcwd() -> *mut c_char {
    crate::set_errno(crate::ENOENT);
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn realpath(_path: *const c_char, resolved: *mut c_char) -> *mut c_char {
    crate::set_errno(crate::ENOENT);
    if !resolved.is_null() {
        unsafe { *resolved = 0 };
    }
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn fdopendir() -> *mut c_void {
    crate::set_errno(crate::EBADF);
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn readdir() -> *mut c_void {
    ptr::null_mut() // 目录尾（无 errno 语义）
}

/// posix_spawn 族错误码**直接正返**（无 -1 约定）。
#[no_mangle]
pub extern "C" fn posix_spawnp() -> c_int {
    crate::ENOSYS
}

// —— poll：std 启动关键路径，必须真实现（d-1c 实踩）——
// `sys::init` → `sanitize_standard_fds` 先 poll(3 fds) 验 0/1/2 有效性：
// 失败且非 EINTR → **`libc::abort()` 直接静默死**（不走 panic 机制、零
// 输出——首个 std 程序 bare `abort` 实跑抓到，errno=38 内联写入即此桩）。
// console 模型 fd 恒有效：返 0（无就绪事件、无 POLLNVAL），后续的
// /dev/null 开补循环天然跳过。

/// pollfd（AArch64 musl：fd/events/revents = int/short/short）。
#[repr(C)]
pub struct PollFd {
    pub fd: c_int,
    pub events: i16,
    pub revents: i16,
}

#[no_mangle]
pub extern "C" fn poll(fds: *mut PollFd, nfds: u64, _timeout: c_int) -> c_int {
    if !fds.is_null() {
        unsafe {
            for i in 0..nfds as usize {
                (*fds.add(i)).revents = 0;
            }
        }
    }
    0
}

// —— environ/getenv（空环境：environ = NULL 表）——

#[no_mangle]
pub static mut environ: *mut *mut c_char = ptr::null_mut();

#[no_mangle]
pub extern "C" fn getenv(_name: *const c_char) -> *mut c_char {
    ptr::null_mut()
}

// —— strerror 面 ——

#[no_mangle]
pub extern "C" fn strerror(errnum: c_int) -> *const c_char {
    let s: &[u8] = match errnum {
        1 => b"Operation not permitted\0",
        2 => b"No such file or directory\0",
        9 => b"Bad file descriptor\0",
        11 => b"Resource temporarily unavailable\0",
        12 => b"Out of memory\0",
        13 => b"Permission denied\0",
        22 => b"Invalid argument\0",
        25 => b"Inappropriate ioctl for device\0",
        38 => b"Function not implemented\0",
        110 => b"Connection timed out\0",
        _ => b"Unknown error\0",
    };
    s.as_ptr() as *const c_char
}

/// XPG 版（返 int 非 glibc 指针版）。
#[no_mangle]
pub extern "C" fn __xpg_strerror_r(_errnum: c_int, buf: *mut c_char, buflen: size_t) -> c_int {
    let s = b"Unknown error\0";
    unsafe {
        let n = core::cmp::min(s.len(), if buflen == 0 { 0 } else { buflen - 1 });
        crate::strmem::memcpy_volatile(buf as *mut u8, s.as_ptr(), n);
        if buflen > 0 {
            *buf.add(n) = 0;
        }
    }
    0
}

// —— getaddrinfo 族 ——

#[no_mangle]
pub extern "C" fn getaddrinfo(
    _node: *const c_char,
    _service: *const c_char,
    _hints: *const c_void,
    res: *mut *mut c_void,
) -> c_int {
    unsafe { *res = ptr::null_mut() };
    crate::EAI_NONAME
}

#[no_mangle]
pub extern "C" fn gai_strerror(_code: c_int) -> *const c_char {
    let s: &[u8] = b"Name or service not known\0";
    s.as_ptr() as *const c_char
}

// —— pthread key/specific（单线程真存取——std 的 TLS 析构注册面）——

const PKEYS: usize = 64;
static mut PKEY_N: u32 = 0;
static mut PKEY_VALS: [u64; PKEYS] = [0; PKEYS];

#[no_mangle]
pub extern "C" fn pthread_key_create(
    key: *mut u32,
    _dtor: extern "C" fn(*mut c_void),
) -> c_int {
    unsafe {
        if PKEY_N as usize >= PKEYS {
            return crate::EAGAIN;
        }
        PKEY_N += 1;
        *key = PKEY_N; // 从 1 起
    }
    0
}

#[no_mangle]
pub extern "C" fn pthread_key_delete(_key: u32) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn pthread_setspecific(key: u32, val: *const c_void) -> c_int {
    unsafe {
        if (key as usize) < PKEYS {
            PKEY_VALS[key as usize] = val as u64;
        }
    }
    0
}

/// 弱面顺手件（附录 A weak 24 有真消费者）。
#[no_mangle]
pub extern "C" fn pthread_getspecific(key: u32) -> *mut c_void {
    unsafe {
        if (key as usize) < PKEYS {
            PKEY_VALS[key as usize] as *mut c_void
        } else {
            ptr::null_mut()
        }
    }
}

// —— pthread 身份/attr ——

#[no_mangle]
pub extern "C" fn pthread_self() -> u64 {
    1 // 单线程：恒定非零伪 id（join/attr 面不真消费）
}

#[no_mangle]
pub extern "C" fn pthread_create() -> c_int {
    crate::EAGAIN // 谎报成功 = 调用方 join 幻影线程；EAGAIN 让 std 报错
}

#[no_mangle]
pub extern "C" fn pthread_join() -> c_int {
    crate::ESRCH
}

#[no_mangle]
pub extern "C" fn pthread_getattr_np() -> c_int {
    0 // attr 是不透明块，getter 桩不读它
}

#[no_mangle]
pub extern "C" fn pthread_attr_getguardsize(guardsize: *mut size_t) -> c_int {
    unsafe { *guardsize = 4096 };
    0
}

#[no_mangle]
pub extern "C" fn pthread_attr_getstack(
    stackaddr: *mut *mut c_void,
    stacksize: *mut size_t,
) -> c_int {
    unsafe {
        *stackaddr = (crate::io::stack_top() - 64 * 1024) as *mut c_void;
        *stacksize = 64 * 1024; // 谎报界（真实映射幅装载端才知道）
    }
    0
}

// —— 惰性 profile 惰性件（rlib 上界引用，程序面永不拉入；定义使「全定义」成立）——

#[no_mangle]
pub static __llvm_profile_filename: usize = 0; // 指针位形的 null（usize 免裸指针 Sync 面）
#[no_mangle]
pub static __llvm_profile_raw_version: u64 = 0;
