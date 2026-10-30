//! # berry-libc —— musl std 的 libc shim（G-eco-d d-1a）
//!
//! ## Overview (for crates.io users)
//!
//! Full-definition libc shim letting Rust std build for
//! `aarch64-unknown-linux-musl` on the
//! [BerryOS](https://github.com/berryos/berryos) microkernel:
//! malloc/write/pthread/futex/crt0 are real, the rest is stubbed
//! loudly — ENOSYS/ENOENT codes follow each caller's
//! error-classification whitelist. aarch64 only; pairs with the in-repo
//! `berry-ld` linker wrapper — see the repository's app guide.
//!
//! Status: experimental (0.1.0) — the surface moves with the BerryOS v1
//! userland. Repo docs are mostly zh-CN.
//!
//! 合约源 = `docs/G-eco编译侧设计草案.md` 附录 A（d-0 审计）：musl target 下
//! Rust std 的 unix 后端引用 189 个强未定义 libc 符号（hello 探针真实面 41）
//! ——本 crate **全定义**：shim 全定义 ⇒ 链接器永不从 musl libc.a 拉成员
//! （依赖 rlib 在 `-lc` 之前，归档按需拉入），产物零 musl 字节。
//!
//! 分档（附录 A）：
//! - **真实现**：malloc 族（静态 256KiB 堆）、write 族（fd 0/1/2 → console
//!   channel）、mem/str 族、`__errno_location`、clock 族、crt0（TLS/TPIDR_EL0
//!   用户态自设——零内核 seam）、snprintf/fprintf（asm 变参蹦床 + % 子集）、
//!   信号面（signal.rs，A7-4 ③a+③b：sigset/sigprocmask/sigaction 真身 +
//!   协同式投递循环 kill/raise/pause）；
//! - **成功桩**：无竞争锁族、`syscall` 走 futex.rs
//!   拦截（nr≠98 返 -ENOSYS——std 无竞争快路径不经此）；
//! - **ENOSYS/ENOENT 桩**：FS 面（fs.rs 真面外的残部）、socket 残部
//!   （A3-2 转真后剩 socketpair/shutdown/getpeername/getsockname——
//!   net.rs + stubs.rs 分界）、fork/exec/posix_spawn。
//!
//! ## ABI 口径（写新桩前必读）
//!
//! - **零参桩忽略多余寄存器实参**：AArch64 调用约定下变参/多参调用方把
//!   实参照常放 x0..x7，被调方（声明零参的 `extern "C" fn`）忽略多余寄存器
//!   完全合法——对象层无签名校验。变参面（`ioctl`/`fcntl`/`syscall`）只走
//!   这条（忽略变参）；需要真实变参的 `snprintf`/`fprintf` 用 asm 蹦床把
//!   x 寄存器实参摊到栈上再进 Rust（`vprintf.rs`）。
//! - **返回值宽度跟 C 类型走**：int 族返 `i32`（w0）、ssize 族返 `isize`
//!   （x0 全宽）、指针族返 `*mut T`——`mov w0` 会零扩展清高 32 位，
//!   声明错宽度 = 调用方读出 +4G 而非 -1（G-eco-c copy_out_u32 同族教训）。
//! - **errno 面**：返 -1 的桩一律 `set_errno(...)`（musl 惯例：调用方先看
//!   返回值再看 errno）。
//! - **多线程根基（T-1）**：pthread_create 真身（`pthread.rs`——栈 VMO +
//!   蹦床 TLS 装配 + thread_start）；跨线程共享面（malloc 堆 = futex 锁、
//!   key 登记表 = futex 锁；errno 仍进程级单实例——v1 接受的窄竞窗）。
//!   std::thread 同步原语全 futex 化（`futex.rs`——syscall(98) 拦截）。
//! - **死= 真退出（Plan B v0.5）不= trap**：`exit`/`_exit`/`abort` =
//!   stdout 零长 EOF 先行 + `berry_process_exit`（0x33 真路径：内核置
//!   Exited + 唤醒 TERMINATED 等待者 + drain + REAPQ 回收任务体）；trap
//!   仍会被 watchdog 计入错杀断言（G-eco 家族契约不变）。`park` 降格为
//!   协议破缺面的兜底（boot 破缺语境不敢假设表内 handle 可用）。

#![no_std]
#![allow(static_mut_refs)]
#![allow(non_camel_case_types)] // C 类型名与 libc 生态同形（c_int/size_t/…）

pub mod alloc;
pub mod crt0;
pub mod fs;
pub mod futex;
pub mod io;
pub mod mmap;
pub mod net;
pub mod pthread;
pub mod signal;
pub mod stubs;
pub mod strmem;
pub mod sys;
pub mod vprintf;

/// 根路径再导出（消费者 `#[used]` 种子引用更顺手；见 crt0.rs 文档）。
pub use crt0::berry_crt0_anchor;

// —— 基础 C 类型（musl aarch64 宽度）——

pub type c_char = u8;
pub type c_void = core::ffi::c_void;
pub type c_int = i32;
pub type c_uint = u32;
pub type c_long = i64;
pub type c_ulong = u64;
pub type size_t = usize;
pub type ssize_t = isize;
pub type off_t = i64;
pub type pid_t = i32;
pub type uid_t = u32;
pub type time_t = i64;
pub type suseconds_t = i64;
pub type clockid_t = c_int;

// —— errno（Linux 数值面；musl 同值）——

pub const EPERM: c_int = 1;
pub const ENOENT: c_int = 2;
pub const ESRCH: c_int = 3;
pub const EINTR: c_int = 4;
pub const EIO: c_int = 5;
pub const EBADF: c_int = 9;
pub const ECHILD: c_int = 10;
pub const EAGAIN: c_int = 11;
pub const ENOMEM: c_int = 12;
pub const EACCES: c_int = 13;
pub const EFAULT: c_int = 14;
pub const EBUSY: c_int = 16;
pub const EEXIST: c_int = 17;
pub const ENOTDIR: c_int = 20;
pub const EISDIR: c_int = 21;
pub const EINVAL: c_int = 22;
pub const EMFILE: c_int = 24;
pub const ENOTTY: c_int = 25;
pub const ENOSPC: c_int = 28;
pub const ESPIPE: c_int = 29;
pub const EPIPE: c_int = 32;
pub const ERANGE: c_int = 34;
pub const ENAMETOOLONG: c_int = 36;
pub const ENOSYS: c_int = 38;
pub const ENOTEMPTY: c_int = 39;
/// T-1：futex 面超时档（std futex_wait_timeout 的 -1+errno 路径）。
pub const ETIMEDOUT: c_int = 110;
/// musl：ENOTSUP == EOPNOTSUPP == 95（xattr 族桩档位——uucore
/// fsxattr 白名单只吞 95，38(ENOSYS) 会向 uu_cp -a/--preserve=xattr
/// 传播，对抗审查实抓）。
pub const ENOTSUP: c_int = 95;
/// socket 域执法三档（A3-1，musl/Linux 数值面； berry-fsclient
/// `SockDomErr` → errno 的映射面随 A3-2 socket() 转真刀接线）。
pub const EPROTONOSUPPORT: c_int = 93;
pub const ESOCKTNOSUPPORT: c_int = 94;
pub const EAFNOSUPPORT: c_int = 97;
/// A3-2：netd gift 缺席 / vnet deviceless（ST_NODEV——berry-fsclient
/// `sock_rpc_errno` 单源映射，const assert 对拍）。
pub const ENODEV: c_int = 19;
/// A3-2：listen 端口已被占用（ST_FULL/Dup——EADDRINUSE 近似档）。
pub const EADDRINUSE: c_int = 98;
/// A3-2：connect 目标非网关 10.0.2.2——v1 协议无地址域（OP_TCP_OPEN 只
/// 携 port、vnet 段构造 dst=GW_IP 钉死），非网关地址 = 本栈结构性不可
/// 达，诚实拒（不静默连到别处）。
pub const ENETUNREACH: c_int = 101;

// EAI_*（getaddrinfo 直接返回，非 errno）
pub const EAI_NONAME: c_int = -2;
/// A3-1：hints family 非 {AF_INET, 0}（musl 同值）。
pub const EAI_FAMILY: c_int = -5;
/// A3-1：service 非数字端口 / socktype 不支持（musl 对不支持 socktype
/// 用此码，musl 同值）。
pub const EAI_SERVICE: c_int = -8;
/// A3-1：malloc 失败档（musl 同值）。
pub const EAI_MEMORY: c_int = -10;
/// A4-2：DNS 查询超时档（netd ST_TIMEOUT 映射；musl 同值——「稍后重试
/// 可解」语义，gai_strerror 文案 "Temporary failure in name resolution"）。
pub const EAI_AGAIN: c_int = -3;

/// musl `FILE`（布局是 shim 私有：全部 FILE 消费者——fileno/fwrite/fprintf
/// ——都是本 crate 的定义，不与真 musl 互操作）。
#[repr(C)]
pub struct FILE {
    pub fd: c_int,
    pub _pad: c_int,
}

pub(crate) static mut ERRNO: c_int = 0;

/// `__errno_location` 的实现体（sys.rs 导出符号）。
#[inline]
pub fn set_errno(e: c_int) {
    unsafe { ERRNO = e };
}

#[inline]
pub fn get_errno() -> c_int {
    unsafe { ERRNO }
}

/// 死亡面统一收口：stdout 零长 EOF（d-3c，见 `io::send_stdout_eof`）+
/// 10s 周期睡眠（trap = watchdog 错杀断言违约，勿改）。
pub fn park() -> ! {
    crate::io::send_stdout_eof();
    loop {
        let mut now: berry_sys::Time = 0;
        if berry_sys::berry_clock_read(berry_sys::CLOCK_MONOTONIC, &mut now) == 0 {
            let _ = berry_sys::berry_thread_sleep(now + 10_000_000_000);
        }
    }
}
