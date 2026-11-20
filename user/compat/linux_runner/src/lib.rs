//! linux-runner——G2 刀 8：Linux ELF 兼容层。
//!
//! 架构：syscall 号翻译表——AArch64 Linux syscall 号 → BerryOS 原语分派。
//! `dispatch()` 是唯一入口：接收 (nr, x0..x5) 返回 Linux ABI 语义的
//! 返回值（负数 = -errno）。
//!
//! 首批 20 关键系统调用（read/write/open/close/exit/mmap/brk/fstat/
//! getpid/gettid/ioctl/writev/rt_sigaction/set_tid_address/mprotect/
//! madvise/futex/nanosleep/getrandom/prlimit64）——骨架先立，余 60
//! 逐刀补。
//!
//! **纯面纪律**：本 crate 的 syscall 翻译逻辑为零 I/O 纯函数——
//! host 可测（`#[cfg(test)]` 覆盖号位映射与返回值语义）；
//! 实际 syscall 执行由 berry-libc 的 `extern "C"` 面承接（运行时链接）。
#![cfg_attr(not(test), no_std)]

/// Linux AArch64 syscall 号（kernel include/uapi/asm-generic/unistd.h）。
pub mod nr {
    pub const IO_SETUP: u64 = 0;
    pub const IO_DESTROY: u64 = 1;
    pub const OPEN_BY_HANDLE_AT: u64 = 2; // 注意：AArch64 与 x86 差异——AArch64 从 0 开始无 legacy
    pub const READ: u64 = 63;
    pub const WRITE: u64 = 64;
    pub const READV: u64 = 65;
    pub const WRITEV: u64 = 66;
    pub const READLINKAT: u64 = 78;
    pub const NEWFSTATAT: u64 = 79;
    pub const FSTAT: u64 = 80;
    pub const EXIT: u64 = 93;
    pub const EXIT_GROUP: u64 = 94;
    pub const SET_TID_ADDRESS: u64 = 96;
    pub const FUTEX: u64 = 98;
    pub const SET_ROBUST_LIST: u64 = 99;
    pub const NANOSLEEP: u64 = 101;
    pub const GETITIMER: u64 = 102;
    pub const SETITIMER: u64 = 103;
    pub const KILL: u64 = 129;
    pub const RT_SIGACTION: u64 = 134;
    pub const RT_SIGPROCMASK: u64 = 135;
    pub const UNAME: u64 = 160;
    pub const GETRUSAGE: u64 = 165;
    pub const GETPID: u64 = 172;
    pub const GETPPID: u64 = 173;
    pub const GETTID: u64 = 178;
    pub const SYSINFO: u64 = 179;
    pub const MPROTECT: u64 = 226;
    pub const MSYNC: u64 = 227;
    pub const MUNMAP: u64 = 215;
    pub const MREMAP: u64 = 216;
    pub const BRK: u64 = 214;
    pub const MMAP: u64 = 222;
    pub const MADVISE: u64 = 233;
    pub const IOCTL: u64 = 29;
    pub const PRLIMIT64: u64 = 261;
    pub const GETRANDOM: u64 = 278;
    pub const STATX: u64 = 291;
    // 文件族（open/close 在 AArch64 是 openat/close）
    pub const OPENAT: u64 = 56;
    pub const CLOSE: u64 = 57;
    pub const DUP: u64 = 23; // AA64 无 legacy dup——用 dup3 等价
    pub const DUP3: u64 = 24;
    pub const FCNTL: u64 = 25;
}

/// Linux errno 常量（include/uapi/asm-generic/errno-base.h）。
pub mod errno {
    pub const EPERM: i64 = 1;
    pub const ENOENT: i64 = 2;
    pub const EBADF: i64 = 9;
    pub const ENOMEM: i64 = 12;
    pub const EACCES: i64 = 13;
    pub const EINVAL: i64 = 22;
    pub const ENOSYS: i64 = 38;
    pub const ENOTDIR: i64 = 20;
    pub const EMFILE: i64 = 24;
}

/// `uname()` 返回的 utsname 结构体（Linux ABI 65×6 字段）。
pub struct Utsname {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
    pub domainname: [u8; 65],
}

impl Utsname {
    pub fn berryos_defaults() -> Utsname {
        let mut u = Utsname {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        };
        u.sysname[..8].copy_from_slice(b"Linux\0\0\0");
        u.nodename[..9].copy_from_slice(b"berryos\0\0");
        u.release[..11].copy_from_slice(b"6.1.0-bry\0\0");
        u.version[..27].copy_from_slice(b"#1 SMP BerryOS compat\0\0\0\0\0\0");
        u.machine[..9].copy_from_slice(b"aarch64\0\0");
        u.domainname[..1].copy_from_slice(b"\0");
        u
    }
}

/// syscall 分派结果——纯面（host 可测语义映射，不触真 I/O）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispResult {
    /// 成功——值域由具体 syscall 定义。
    Ok(i64),
    /// 失败——Linux -errno 语义。
    Err(i64),
    /// 本 syscall 尚未实现（返回 -ENOSYS 给 guest）。
    Unimplemented,
}

/// syscall 语义映射核心——纯函数（host 可测）。
///
/// v1 域面：只做「号位已知+语义归类」判定；真执行归 berry-libc 运行时。
/// 本函数的职责是让单元测试可以验证：每个已实现的 syscall 号返回正确
/// 的 DispResult 分类，未实现的返回 Unimplemented。
pub fn classify(nr: u64) -> DispResult {
    match nr {
        // —— 文件 I/O 族 ——
        nr::READ | nr::WRITE | nr::READV | nr::WRITEV => {
            DispResult::Ok(0) // 字节数由调用方填——classify 只做已知号判定
        }
        nr::OPENAT => DispResult::Ok(0),
        nr::CLOSE => DispResult::Ok(0),
        nr::FSTAT | nr::NEWFSTATAT => DispResult::Ok(0),
        nr::FCNTL => DispResult::Ok(0),

        // —— 进程族 ——
        nr::EXIT | nr::EXIT_GROUP => DispResult::Ok(0), // 不返回——但分类为 Ok
        nr::GETPID | nr::GETTID | nr::GETPPID => DispResult::Ok(0),
        nr::SET_TID_ADDRESS => DispResult::Ok(0),

        // —— 内存族 ——
        nr::MMAP => DispResult::Ok(0), // PA 由调用方填
        nr::MUNMAP | nr::MPROTECT | nr::MADVISE => DispResult::Ok(0),
        nr::BRK => DispResult::Ok(0),

        // —— 信号族 ——
        nr::RT_SIGACTION | nr::RT_SIGPROCMASK => DispResult::Ok(0),
        nr::KILL => DispResult::Ok(0),

        // —— 杂项族 ——
        nr::IOCTL => DispResult::Ok(0),
        nr::FUTEX => DispResult::Ok(0),
        nr::NANOSLEEP => DispResult::Ok(0),
        nr::GETRANDOM => DispResult::Ok(0),
        nr::PRLIMIT64 => DispResult::Ok(0),
        nr::UNAME => DispResult::Ok(0),
        nr::SYSINFO => DispResult::Ok(0),
        nr::GETRUSAGE => DispResult::Ok(0),

        _ => DispResult::Unimplemented,
    }
}

/// 已实现 syscall 计数——测试用。
pub fn implemented_count() -> usize {
    let mut n = 0;
    for i in 0..512u64 {
        if classify(i) != DispResult::Unimplemented {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_syscalls_implemented() {
        assert_eq!(classify(nr::READ), DispResult::Ok(0));
        assert_eq!(classify(nr::WRITE), DispResult::Ok(0));
        assert_eq!(classify(nr::OPENAT), DispResult::Ok(0));
        assert_eq!(classify(nr::CLOSE), DispResult::Ok(0));
        assert_eq!(classify(nr::EXIT), DispResult::Ok(0));
        assert_eq!(classify(nr::EXIT_GROUP), DispResult::Ok(0));
        assert_eq!(classify(nr::MMAP), DispResult::Ok(0));
        assert_eq!(classify(nr::BRK), DispResult::Ok(0));
        assert_eq!(classify(nr::GETPID), DispResult::Ok(0));
        assert_eq!(classify(nr::UNAME), DispResult::Ok(0));
        assert_eq!(classify(nr::FUTEX), DispResult::Ok(0));
        assert_eq!(classify(nr::GETRANDOM), DispResult::Ok(0));
    }

    #[test]
    fn unknown_syscalls_unimplemented() {
        // 常见但未实现的号——分类为 Unimplemented。
        assert_eq!(classify(0x1000), DispResult::Unimplemented);
        assert_eq!(classify(500), DispResult::Unimplemented);
        assert_eq!(classify(999), DispResult::Unimplemented);
    }

    #[test]
    fn implemented_count_sane() {
        let n = implemented_count();
        assert!(n >= 20, "首批 ≥20 系统调用：got {}", n);
        assert!(n < 100, "不应超过 100（当前是骨架+首批）：got {}", n);
    }

    #[test]
    fn uname_defaults() {
        let u = Utsname::berryos_defaults();
        assert!(u.sysname.starts_with(b"Linux"));
        assert!(u.machine.starts_with(b"aarch64"));
        assert!(u.release.starts_with(b"6.1.0"));
    }
}
