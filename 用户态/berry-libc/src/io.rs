//! 输出/stdio —— fd 0/1/2 → console channel（真实现档）
//!
//! fd 模型（C-1 刀 2 起）：0/1/2 = console/stdin 原路径**一字不动**（R6）；
//! fd≥3 = fs-backed（`fs.rs`——fd 表游标 + close 即 commit，C-1 两裁决）。
//! 1/2 = console 发端别名（写方向）、0 = env 块 stdin 端（d-3b 起真输入
//! ——channel recv，零长消息 = EOF 约定），都经**env 块**交付（d-3a，见
//! `berry_libc_boot`）；fd≥3 的 read/write/close/lseek 分流 `fs.rs`（不在
//! 册 fd 的 EBADF 面由那边统一收口）。
//!
//! 发送契约（雷 2 教训 + A-2 修订）：**阻塞重发**——`-EBUSY`（console 队列
//! 满）→ `clock_read` + `wait_one(console, WRITABLE, 10s)` 睡等重试（超时
//! 重臂）。纯自旋曾是 G-eco-d 的短命验收程序可容忍形态（自旋窗有界），
//! A-2 起常驻 std 服务（bsh）上线——自旋者永不出 runqueue，RR 环序冻结、
//! 排水窗口被占优者垄断 = 长跑饱和下结构性饿死（雷 2，`user.rs` 八站点
//! 同款修法）；阻塞者经 recv 侧 `wake_matching` 公平分槽。分片 ≤192B
//! （channel inline 帽 256）。

use core::ptr;

use berry_sys::{berry_channel_recv, berry_channel_send, berry_clock_read,
                berry_object_wait_one, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE,
                SIGNAL_WRITABLE};

use crate::{c_char, c_int, c_long, c_void, size_t, ssize_t, FILE};

static mut CONSOLE_H: u32 = 0;
static mut STDIN_H: u32 = 0; // d-3b 预留（env 块 handles[1]；read 面随后开）
// A-2/P6：env 块 gift[2..] 双槽——[0] = rootfs VMO READ dup（bsh）、
// [1] = P6 A-1 第三/四位（fsmgr 得服务端端 `gift2_h()`、bsh 得 fs 客户
// 端端 `fs_h()`——同槽两位经 spawn 侧的 gift 排布区分，见 berry-exec 注）。
static mut GIFT_H: [u32; 2] = [0; 2];
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
        // A-2：容量 2→3；P6 A-1：3→4——gift 位语义 [0]=console 发端别名、
        // [1]=stdin 端、[2]=rootfs VMO READ dup 或 fsmgr 端、[3]=fs 客户端
        // 端（按 spawn 侧排布区分，落 `GIFT_H` 双槽；svcmgr 系老程序照送
        // 1-2 位，按实收 nhandles 落种，向后兼容）。
        let mut hs = [0u32; 4];
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
        if nh >= 3 {
            GIFT_H[0] = hs[2];
        }
        if nh >= 4 {
            GIFT_H[1] = hs[3];
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

/// main 意外返回后的收口（crt0 跳入）。`bl main` 返回后 x0 仍持 main 返回码
/// （`bl` 不清 x0——零参桩读首参的合法面，见 lib.rs「ABI 口径」）；std 正路
/// 在 lang_start 内已 `libc::exit`，到这里的都是旁路，语义照走真退出。
#[no_mangle]
pub extern "C" fn berry_libc_park_after_main(code: c_int) -> ! {
    exit(code)
}

/// stdout 端零长 EOF（d-3c）：`park` 前一次性补发——d-3b「零长消息 = EOF」
/// 约定的**写侧**对称面。（hexyl 的 stdout 是 svcmgr 交接的私有通道端，
/// svcmgr 收到 EOF 才知输出终了（确定性验收，不赌超时）；对真 console 端
/// （stdhello 等）只是 0 字节消息，console 排水写 0 字节无扰。满队阻塞等
/// WRITABLE（`console_write` 同款，A-2 修订——自旋是雷 2 形态）；其余错误
/// 码放弃（协议破缺面自有更 loud 的死法）。
pub fn send_stdout_eof() {
    let h = unsafe { CONSOLE_H };
    if h == 0 {
        return;
    }
    let z = 0u8;
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, &z, 0, ptr::null(), 0);
        if s == 0 || s != BERRY_ERR_BUSY {
            return;
        }
        if deadline == 0 {
            let mut now = 0u64;
            if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                continue;
            }
            deadline = now + 10_000_000_000;
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs);
        deadline = 0;
    }
}

/// env 块 gift[2] 的 rootfs VMO handle（A-2：0 = 无第三位——svcmgr 系老
/// 程序；bsh 持 READ dup 经 berry-exec 流式取子程序 ELF）。
pub fn rootfs_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[0] }
}

/// env 块 gift[2] 的同槽访问器（P6 A-1：fsmgr 的 fs 服务端端——svcmgr 建
/// fs 对后经 env 块第三位交付；0 = 未交付 = park 挂起）。与 [`rootfs_h`]
/// 同槽不同名：spawn 侧同一 gift 位对两种程序承载不同对象。
pub fn gift2_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[0] }
}

/// env 块 gift[3] 的 fs 客户端端（P6 A-1：bsh 的 `>`/`cat`/`ls` 走它发
/// RPC；0 = 无第四位 = fs 未拉起，调用方降级「fs not available」不死）。
pub fn fs_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[1] }
}

/// env 块 gift[3] 的同槽访问器（P6 A-5：fsmgr 的块设备服务端——svcmgr
/// 预拉 vblk 后把 blk_a 经 fsmgr 的 env 块第四位交付，fsmgr 据此读 FAT
/// 卷；0 = 未交付 = 无 FAT 层，RAM-only 降级续走）。与 [`fs_h`] 同槽不同
/// 名：spawn 侧同一 gift 位对两种程序承载不同对象（bsh 拿 fs 端 / fsmgr
/// 拿 blk 端，每进程 env 块独立无冲突——gift[2] 的 rootfs/gift2 同款惯例）。
pub fn blk_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[1] }
}

pub fn stack_top() -> usize {
    unsafe { STACK_TOP }
}

/// console 发送：分片 + EBUSY 阻塞等 WRITABLE（见模块头契约，A-2 修订）。
pub fn console_write(bytes: &[u8]) {
    let mut off = 0usize;
    while off < bytes.len() {
        let n = core::cmp::min(192, bytes.len() - off);
        let p = unsafe { bytes.as_ptr().add(off) };
        let mut deadline = 0u64;
        loop {
            let s = berry_channel_send(unsafe { CONSOLE_H }, p, n, ptr::null(), 0);
            if s == 0 {
                break;
            }
            if s != BERRY_ERR_BUSY {
                // -EAGAIN 对 send 不发生；其余错误码（handle 失效等）阻塞
                // 等也无出路——原地重试（协议破缺面自有更 loud 的死法）。
                continue;
            }
            // -EBUSY（满）：阻塞等 WRITABLE（10s 帽，醒来/超时都重臂）——
            // 雷 2：自旋者永不出 runqueue，长跑饱和下结构性饿死。
            if deadline == 0 {
                let mut now = 0u64;
                if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                    continue; // 时钟不可用：退化为自旋（EL0 可抢占 = 有界）
                }
                deadline = now + 10_000_000_000;
            }
            let mut obs = 0u64;
            let _ = berry_object_wait_one(
                unsafe { CONSOLE_H },
                SIGNAL_WRITABLE,
                deadline,
                &mut obs,
            );
            deadline = 0; // 重臂（命中后若仍满则续等——公平排队）
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
        // fd≥3 = fs-backed 写（fs.rs 收口 EBADF/游标/分片 PUT）；fd<0 同落
        // 那边的 EBADF 面。
        return crate::fs::write_fd(fd, buf, n);
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
        // fs-backed：逐 iov 走 fs 写（部分写返回已写量——write_fd 同语义）。
        if cnt <= 0 {
            return 0;
        }
        let mut total: ssize_t = 0;
        for i in 0..cnt as usize {
            let v = unsafe { &*iov.add(i) };
            if v.len > 0 {
                let r = crate::fs::write_fd(fd, v.base as *const c_void, v.len);
                if r < 0 {
                    return if total > 0 { total } else { -1 };
                }
                total += r;
            }
        }
        return total;
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
            // fd≥3 = fs-backed 读（fs.rs 收口 EBADF/RA 预读）；fd 1/2 在那边
            // 落同一 EBADF 面（不在册）——旧「1/2 只写：读方向诚拒」零漂移。
            return crate::fs::read_fd(fd, buf, n);
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
    // fd≥3 = fs-backed：摘表 + 可写 fd 的 close 即 commit（裁决 2）。
    crate::fs::close_fd(fd)
}

#[no_mangle]
pub extern "C" fn isatty(fd: c_int) -> c_int {
    if !fd_ok(fd) && !crate::fs::fd_valid(fd) {
        crate::set_errno(crate::EBADF);
        return 0;
    }
    crate::set_errno(crate::ENOTTY);
    0 // v1 诚实面：非 tty（console 同面；fs fd 亦非 tty）
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

// —— 死亡面（Plan B v0.5：exit 族接真 0x33——EOF 先行 + `berry_process_exit`。
// EOF 在退出**前**补发（d-3c 收端约定不变：收端收齐零长消息即知输出终了，
// 不赌进程死序）；`park` 保留给「协议破缺面」的兜底（argc=0/console=0 类
// boot 破缺——退出路径依赖表内 handle，破缺语境不敢假设可用）——

#[no_mangle]
pub extern "C" fn exit(code: c_int) -> ! {
    send_stdout_eof();
    berry_sys::berry_process_exit(code)
}

#[no_mangle]
pub extern "C" fn _exit(code: c_int) -> ! {
    send_stdout_eof();
    berry_sys::berry_process_exit(code)
}

#[no_mangle]
pub extern "C" fn abort() -> ! {
    console_write(b"abort\n");
    send_stdout_eof();
    berry_sys::berry_process_exit(134) // 128+SIGABRT 惯例码
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
    exit(134) // assert 失败 = abort 同码（glibc 语义）
}
