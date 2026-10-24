//! fs —— fd 3+ 文件面真实现（C-1 刀 2，合约 `docs/VM里程碑设计草案.md` §C-1）
//!
//! 原料 = 刀 1 berry-fsclient（会话机 + 纯函数面），本模块只做**组装**：
//! fd 表静态化、CStr→wire 解析、errno 映射、musl 结构布局。fd 0/1/2 =
//! console/stdin（`io.rs` 现路径一字不动——R6）；fd≥3 = fs-backed。
//!
//! 两裁决（随五刀拆解单批准）：
//! 1. **fd 表 = 客户端游标**：read→`GET(path,off)`、write→`PUT(path,off)`
//!    ——协议零新增、fsmgr 零改动（服务端无会话）。
//! 2. **写通道 commit 点 = close**：写 fd 走 PUT 累积 staging，`close()` 发
//!    `TRUNC(offset)` 提交（两层实证：fstree trunc = create-or-resize 且
//!    size=0 兼 create；FAT stage_trunc 未 staged 且盘上无此名 = 建空文件）
//!    ——bsh `>` 两段式同款。
//!
//! 无 fs gift（`fs_h()==0`，hexyl 类 spawn 不带 gift[3]）的进程：一切文件
//! op 优雅降级 **ENOENT**——与刀前 ENOENT 桩同 errno 同 strerror，行为
//! 零漂移（NoFs→ENOENT 专为这条选的值）。
//!
//! 读通道 read-ahead（RA）：`GET` 无长度域（服务端恒返 ≤CHUNK 一片），小
//! read（std `read_to_string` 的 16B 探针）若直丢尾段 = 偏移跳空。单 RA
//! 静态缓冲（`RA_FD` 持有者）+ 三个失效点（fd 切换 / lseek / 同 fd write）
//! ——单线程根基，`static mut` 直写合法（lib.rs ABI 口径）。
//!
//! ## v1 诚实缺口（文档化，v2 再议；与刀 1 crate 头注同源）
//!
//! - 无 `.`/`..` 归一化；写打开一律截断语义（close 时 TRUNC(游标)，无
//!   O_APPEND/不截断打开）；lseek 越 `FILE_SIZE_MAX` 拒 EINVAL（协议 off
//!   域帽）；dirent `d_type = DT_UNKNOWN`（READDIR blob 只有名）。
//! - O_CREAT 的存在性/父目录错误**推迟到 close-commit** 才暴露（open 先
//!   行成功）——创建延后是裁决 2 的代价，POSIX 语义（open 时报 ENOENT）
//!   随 v2。
//! - SEEK_END 基准：只读 fd 走 STAT size；**可写 fd 用自身游标**（对未
//!   commit 的 FAT staging，盘上 STAT 是旧值——游标才是本 fd 的真视景）。
//! - `openat`/`unlinkat`/`mkdirat`/`fstatat` 忽略 dirfd（恒按 AT_FDCWD
//!   语义——std 的 `*at` 调用面全走 AT_FDCWD，真 dirfd 语义随 v2）。
//! - 沙箱内无并发写者（单 client 串行 RPC 会话）：无 POSIX 级竞态面。

use core::ptr;

use berry_fsclient::{
    commit, dirent_next, get_chunk, put_chunk, resolve_path, render_cwd, seek_offset, st_mode_of,
    readdir as rpc_readdir, stat as rpc_stat, mkdir as rpc_mkdir, unlink as rpc_unlink,
    FdEntry, FdTable, CHUNK, NAME_MAX, PATH_DEPTH_MAX, PATH_MAX, RPC_BUF, SEEK_END, TYPE_DIR,
    FsErr,
};

use crate::{c_char, c_int, c_void, off_t, size_t, ssize_t};

// —— open flags（Linux/musl 通用值；v1 消费面 = 访问位 + O_CREAT/O_EXCL）——

pub const O_RDONLY: c_int = 0;
pub const O_WRONLY: c_int = 1;
pub const O_RDWR: c_int = 2;
pub const O_CREAT: c_int = 0o100;
pub const O_EXCL: c_int = 0o200;
pub const O_TRUNC: c_int = 0o1000;

/// `*at` 族的「相对 cwd」哨兵（std 的调用面恒传它）。
pub const AT_FDCWD: c_int = -100;

/// dirent d_type（v1 恒未知——需要类型走 STAT）。
pub const DT_UNKNOWN: u8 = 0;

// —— 静态态（单线程：static mut 直写，lib.rs ABI 口径）——

static mut FDT: FdTable = FdTable::new();
static mut CWD: [u8; PATH_MAX] = [0; PATH_MAX];
static mut CWD_LEN: usize = 0; // 根 = 空 wire
// 读 read-ahead：RA_BUF 兼 RPC 缓冲（get_chunk 把数据前移到 [0..len]，
// 恰是 RA 存区——零二次拷贝）。
static mut RA_FD: c_int = -1;
static mut RA_BUF: [u8; RPC_BUF] = [0; RPC_BUF];
static mut RA_LEN: usize = 0;
static mut RA_OFF: usize = 0;
/// stat 的假 inode 序号（从 1 起——0 在部分调用方语义里 = 已删项）。
static mut INO_SEQ: u64 = 0;

unsafe fn cwd_wire() -> &'static [u8] {
    // SAFETY: 只读；写点仅 chdir（本模块）。
    unsafe { &CWD[..CWD_LEN] }
}

unsafe fn ra_drop(fd: c_int) {
    if unsafe { RA_FD } == fd {
        unsafe { RA_FD = -1 };
    }
}

/// FsErr → errno（协议面错误与 errno 是两层——映射归 libc，刀 1 注）。
/// NoFs→ENOENT 是**行为零漂移**锚点：无 fs gift 的程序在刀前桩/刀后真
/// 实现下看到同 errno 同 strerror。
fn errno_of(e: FsErr) -> c_int {
    match e {
        FsErr::NoFs => crate::ENOENT,
        FsErr::Noent => crate::ENOENT,
        FsErr::Full => crate::ENOSPC,
        FsErr::Bad => crate::EINVAL,
        FsErr::Exists => crate::EEXIST,
        FsErr::NotEmpty => crate::ENOTEMPTY,
        FsErr::Rpc => crate::EIO,
    }
}

/// resolve_path 拒因分类（只选 errno，不重复执法——resolve_path 仍是
/// 权威）：长度/深度面 ENAMETOOLONG、其余（字符集/空段）EINVAL。
fn resolve_errno(user: &[u8]) -> c_int {
    let rest = if user.first() == Some(&b'/') {
        &user[1..]
    } else {
        user
    };
    let (mut segs, mut total) = (0usize, 0usize);
    for seg in rest.split(|&c| c == b'/') {
        segs += 1;
        if seg.len() > NAME_MAX {
            return crate::ENAMETOOLONG;
        }
        total += seg.len() + 1; // +分隔符
    }
    if segs > PATH_DEPTH_MAX || total > PATH_MAX + 1 {
        return crate::ENAMETOOLONG;
    }
    crate::EINVAL
}

/// CStr 路径 → wire（NUL 分段）解析到 out；Err = errno 已分类。
unsafe fn resolve_to(path: *const c_char, out: &mut [u8; PATH_MAX]) -> Result<usize, c_int> {
    if path.is_null() {
        return Err(crate::EFAULT);
    }
    // SAFETY: CStr 契约——首 NUL 前是路径（内嵌 NUL 天然截断）。
    let user = unsafe { core::slice::from_raw_parts(path as *const u8, crate::strmem::strlen(path)) };
    match resolve_path(unsafe { cwd_wire() }, user, out) {
        Some(l) => Ok(l),
        None => Err(resolve_errno(user)),
    }
}

// —— musl aarch64 结构布局（消费面 = Rust std；字段偏移即 ABI）——

/// `struct stat`（musl aarch64：128B，st_mode@24 / st_size@48）。
#[repr(C)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub __pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_atime_nsec: i64,
    pub st_mtime: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime: i64,
    pub st_ctime_nsec: i64,
    pub __unused: [u32; 2],
}

/// `struct dirent`（musl aarch64：d_ino@0/d_off@8/d_reclen@16/d_type@18/
/// d_name@19，总 280B 对齐尾垫）。
#[repr(C)]
pub struct Dirent {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

/// `DIR`（shim 私有形状——FILE 同款先例：全部消费者都经本 crate 的
/// opendir/readdir/closedir，不与真 musl 互操作）。opendir 一次 READDIR
/// 拉全目录 blob（bsh ls 同形——服务端无游标面）。
#[repr(C)]
pub struct DIR {
    blob: [u8; RPC_BUF],
    len: usize,
    pos: usize,
    ent: Dirent,
    ino: u64,
}

unsafe fn fill_stat(out: *mut Stat, ty: u8, size: u32) -> c_int {
    let mode = match st_mode_of(ty) {
        Some(m) => m,
        None => return crate::EIO, // 未知 ty = 协议破缺
    };
    unsafe {
        INO_SEQ += 1;
        ptr::write(
            out,
            Stat {
                st_dev: 0,
                st_ino: INO_SEQ,
                st_nlink: 1,
                st_mode: mode,
                st_uid: 0,
                st_gid: 0,
                __pad0: 0,
                st_rdev: 0,
                st_size: size as i64,
                st_blksize: 512,
                st_blocks: ((size as u64 + 511) / 512) as i64,
                st_atime: 0,
                st_atime_nsec: 0,
                st_mtime: 0,
                st_mtime_nsec: 0,
                st_ctime: 0,
                st_ctime_nsec: 0,
                __unused: [0; 2],
            },
        );
    }
    0
}

// —— open 族（核心；openat = dirfd 忽略的同体）——

unsafe fn open_core(path: *const c_char, flags: c_int) -> c_int {
    unsafe {
        let mut wire = [0u8; PATH_MAX];
        let len = match resolve_to(path, &mut wire) {
            Ok(l) => l,
            Err(e) => {
                crate::set_errno(e);
                return -1;
            }
        };
        let w = &wire[..len];
        let acc = flags & 0x3;
        if acc > O_RDWR {
            crate::set_errno(crate::EINVAL);
            return -1;
        }
        let (creat, excl) = (flags & O_CREAT != 0, flags & O_EXCL != 0);
        let readable = acc == O_RDONLY || acc == O_RDWR;
        let writable = acc == O_WRONLY || acc == O_RDWR;
        // 存在性/类型探针（STAT 一次拿全；fs_h()==0 由 rpc_stat 返 NoFs）。
        match rpc_stat(crate::io::fs_h(), w) {
            Err(FsErr::NoFs) => {
                // 无 fs gift：降级 ENOENT 零漂移（O_CREAT 也建不了——无服务）。
                crate::set_errno(crate::ENOENT);
                return -1;
            }
            // 缺失 + O_CREAT = 建新（O_EXCL 不拦缺失——POSIX 原子建锁面）；
            // 创建延迟到 close-commit（模块头诚实缺口）。
            Err(FsErr::Noent) if creat => {}
            Err(FsErr::Noent) => {
                crate::set_errno(crate::ENOENT);
                return -1;
            }
            Err(e) => {
                crate::set_errno(errno_of(e));
                return -1;
            }
            // 已存在：O_EXCL 拒；目录拒（目录只走 opendir 面）。
            Ok((TYPE_DIR, _)) => {
                crate::set_errno(crate::EISDIR);
                return -1;
            }
            Ok((_, _)) if excl => {
                crate::set_errno(crate::EEXIST);
                return -1;
            }
            Ok((_, _)) => {}
        }
        let entry = match FdEntry::new(w, readable, writable) {
            Some(e) => e,
            None => {
                crate::set_errno(crate::ENAMETOOLONG);
                return -1;
            }
        };
        match FDT.alloc(entry) {
            Some(fd) => fd,
            None => {
                crate::set_errno(crate::EMFILE);
                -1
            }
        }
    }
}

/// 变参安全（mode 落 x2 被忽略——lib.rs ABI 口径零参面同源）。
#[no_mangle]
pub unsafe extern "C" fn open(path: *const c_char, flags: c_int) -> c_int {
    unsafe { open_core(path, flags) }
}

#[no_mangle]
pub unsafe extern "C" fn openat(_dirfd: c_int, path: *const c_char, flags: c_int) -> c_int {
    // std 的调用面恒 AT_FDCWD（模块头注）；dirfd 面 v2。
    unsafe { open_core(path, flags) }
}

// —— io.rs 的 fd≥3 分支原料 ——

pub(crate) fn read_fd(fd: c_int, buf: *mut c_void, n: size_t) -> ssize_t {
    unsafe {
        // fd<3（含 -1）先拒：io.rs 的 fd 1/2 读面旧桩即 EBADF（零漂移）；
        // 且 RA_FD 失效哨兵 = -1——不先拒，read(-1) 会撞「RA_FD==fd」把
        // 陈旧预读窗当数据返出（哨兵值与非法 fd 重合的坑）。
        if fd < 3 {
            crate::set_errno(crate::EBADF);
            return -1;
        }
        if n == 0 {
            // POSIX：0 字节读在坏 fd 上仍 EBADF（旧桩同面——零漂移）。
            return if FDT.get(fd).is_some_and(|e| e.readable) {
                0
            } else {
                crate::set_errno(crate::EBADF);
                -1
            };
        }
        if RA_FD != fd {
            RA_FD = -1; // 换 fd：旧 RA 作废（游标已越过，重取才对）
        }
        if RA_FD == fd && RA_OFF < RA_LEN {
            let c = core::cmp::min(n, RA_LEN - RA_OFF);
            ptr::copy_nonoverlapping(RA_BUF.as_ptr().add(RA_OFF), buf as *mut u8, c);
            RA_OFF += c;
            return c as ssize_t;
        }
        let entry = match FDT.get_mut(fd) {
            Some(e) if e.readable => e,
            _ => {
                crate::set_errno(crate::EBADF); // 不在册 / 只写 fd
                return -1;
            }
        };
        match get_chunk(crate::io::fs_h(), entry.wire(), entry.offset, &mut RA_BUF) {
            Ok((0, _)) => 0, // EOF（短读合法：单片即返，std 循环面消化）
            Ok((len, _)) => {
                entry.advance(len); // 游标进过 RA 窗（RA 是预读缓存非位置）
                RA_FD = fd;
                RA_LEN = len;
                let c = core::cmp::min(n, len);
                ptr::copy_nonoverlapping(RA_BUF.as_ptr(), buf as *mut u8, c);
                RA_OFF = c;
                c as ssize_t
            }
            Err(e) => {
                crate::set_errno(errno_of(e));
                -1
            }
        }
    }
}

pub(crate) fn write_fd(fd: c_int, buf: *const c_void, n: size_t) -> ssize_t {
    unsafe {
        if n == 0 {
            // POSIX：0 字节写在坏 fd 上仍 EBADF（write 桩旧面——零漂移）。
            return if FDT.get(fd).is_some_and(|e| e.writable) {
                0
            } else {
                crate::set_errno(crate::EBADF);
                -1
            };
        }
        ra_drop(fd); // O_RDWR 交错：write 后旧 RA 窗 = 陈旧数据
        let bytes = core::slice::from_raw_parts(buf as *const u8, n);
        let mut done: size_t = 0;
        while done < bytes.len() {
            let entry = match FDT.get_mut(fd) {
                Some(e) if e.writable => e,
                _ => {
                    crate::set_errno(crate::EBADF);
                    return -1;
                }
            };
            let take = core::cmp::min(CHUNK as usize, bytes.len() - done);
            match put_chunk(crate::io::fs_h(), entry.wire(), entry.offset, &bytes[done..done + take])
            {
                Ok(()) => {
                    entry.advance(take);
                    done += take;
                }
                Err(e) => {
                    crate::set_errno(errno_of(e));
                    // POSIX：部分写返回已写量（仅零写才 -1）。
                    return if done > 0 { done as ssize_t } else { -1 };
                }
            }
        }
        n as ssize_t
    }
}

/// close 即 commit（裁决 2）：fd 恒消费（POSIX——close 出错 fd 也已关）。
pub(crate) fn close_fd(fd: c_int) -> c_int {
    unsafe {
        ra_drop(fd);
        let entry = match FDT.take(fd) {
            Some(e) => e,
            None => {
                crate::set_errno(crate::EBADF);
                return -1;
            }
        };
        if entry.writable {
            if let Err(e) = commit(crate::io::fs_h(), entry.wire(), entry.offset) {
                crate::set_errno(errno_of(e));
                return -1;
            }
        }
        0
    }
}

/// isatty 的 fd≥3 面（io.rs 调）：在册 fs fd → ENOTTY，否则 EBADF。
pub(crate) fn fd_valid(fd: c_int) -> bool {
    // SAFETY: 只读 FDT。
    unsafe { FDT.get(fd).is_some() }
}

// —— lseek ——

#[no_mangle]
pub unsafe extern "C" fn lseek(fd: c_int, off: off_t, whence: c_int) -> off_t {
    unsafe {
        if fd < 3 {
            crate::set_errno(crate::ESPIPE); // fd 0/1/2 管道面（旧桩同值零漂移）
            return -1;
        }
        let entry = match FDT.get_mut(fd) {
            Some(e) => e,
            None => {
                crate::set_errno(crate::EBADF);
                return -1;
            }
        };
        // SEEK_END 基准：可写 fd 用自身游标（FAT staging 未 commit 时盘上
        // STAT 是旧值）；只读 fd 走 STAT。
        let end = if whence == SEEK_END && !entry.writable {
            match rpc_stat(crate::io::fs_h(), entry.wire()) {
                Ok((_, size)) => Some(size),
                Err(e) => {
                    crate::set_errno(errno_of(e));
                    return -1;
                }
            }
        } else if whence == SEEK_END {
            Some(entry.offset)
        } else {
            None
        };
        match seek_offset(entry.offset, whence, off, end) {
            Some(o) => {
                entry.offset = o;
                ra_drop(fd);
                o as off_t
            }
            None => {
                crate::set_errno(crate::EINVAL); // 越帽/未知 whence/END 无基准
                -1
            }
        }
    }
}

// —— stat 族 ——

unsafe fn stat_core(path: *const c_char, out: *mut Stat) -> c_int {
    unsafe {
        if out.is_null() {
            crate::set_errno(crate::EFAULT);
            return -1;
        }
        let mut wire = [0u8; PATH_MAX];
        let len = match resolve_to(path, &mut wire) {
            Ok(l) => l,
            Err(e) => {
                crate::set_errno(e);
                return -1;
            }
        };
        match rpc_stat(crate::io::fs_h(), &wire[..len]) {
            Ok((ty, size)) => {
                let e = fill_stat(out, ty, size);
                if e != 0 {
                    crate::set_errno(e);
                    return -1;
                }
                0
            }
            Err(err) => {
                crate::set_errno(errno_of(err));
                -1
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn stat(path: *const c_char, out: *mut Stat) -> c_int {
    unsafe { stat_core(path, out) }
}

/// 无符号链接：lstat ≡ stat。
#[no_mangle]
pub unsafe extern "C" fn lstat(path: *const c_char, out: *mut Stat) -> c_int {
    unsafe { stat_core(path, out) }
}

#[no_mangle]
pub unsafe extern "C" fn fstatat(
    _dirfd: c_int,
    path: *const c_char,
    out: *mut Stat,
    _flags: c_int,
) -> c_int {
    unsafe { stat_core(path, out) }
}

#[no_mangle]
pub unsafe extern "C" fn fstat(fd: c_int, out: *mut Stat) -> c_int {
    unsafe {
        if out.is_null() {
            crate::set_errno(crate::EFAULT);
            return -1;
        }
        // fd 0/1/2 = console/stdin：ENOSYS 旧桩面零漂移。
        let entry = match FDT.get(fd) {
            Some(e) => e,
            None => {
                crate::set_errno(if fd >= 3 { crate::EBADF } else { crate::ENOSYS });
                return -1;
            }
        };
        match rpc_stat(crate::io::fs_h(), entry.wire()) {
            Ok((ty, size)) => {
                let e = fill_stat(out, ty, size);
                if e != 0 {
                    crate::set_errno(e);
                    return -1;
                }
                0
            }
            Err(err) => {
                crate::set_errno(errno_of(err));
                -1
            }
        }
    }
}

// —— 目录族（std::fs::read_dir 的三件套）——

#[no_mangle]
pub unsafe extern "C" fn opendir(path: *const c_char) -> *mut DIR {
    unsafe {
        let mut wire = [0u8; PATH_MAX];
        let len = match resolve_to(path, &mut wire) {
            Ok(l) => l,
            Err(e) => {
                crate::set_errno(e);
                return ptr::null_mut();
            }
        };
        let p = crate::alloc::malloc(core::mem::size_of::<DIR>()) as *mut DIR;
        if p.is_null() {
            crate::set_errno(crate::ENOMEM);
            return ptr::null_mut();
        }
        // blob 兼 RPC 缓冲：readdir 应答数据前移到 [0..n] 恰是存区。
        let n = match rpc_readdir(crate::io::fs_h(), &wire[..len], &mut (*p).blob) {
            Ok(n) => n,
            Err(e) => {
                // 语义分类：Noent→ENOENT；Bad（目标是文件）→ENOTDIR；
                // NoFs→ENOENT（降级零漂移）。
                let errno = match e {
                    FsErr::Bad => crate::ENOTDIR,
                    other => errno_of(other),
                };
                crate::alloc::free(p as *mut c_void);
                crate::set_errno(errno);
                return ptr::null_mut();
            }
        };
        (*p).len = n;
        (*p).pos = 0;
        (*p).ino = 0;
        (*p).ent = Dirent {
            d_ino: 0,
            d_off: 0,
            d_reclen: 0,
            d_type: DT_UNKNOWN,
            d_name: [0; 256],
        };
        p
    }
}

#[no_mangle]
pub unsafe extern "C" fn readdir(d: *mut DIR) -> *mut Dirent {
    unsafe {
        if d.is_null() {
            return ptr::null_mut();
        }
        let d = &mut *d;
        match dirent_next(&d.blob[..d.len], &mut d.pos) {
            None => ptr::null_mut(), // 目录尾（无 errno 语义）
            Some(name) => {
                d.ino += 1;
                let nl = core::cmp::min(name.len(), 255);
                d.ent.d_ino = d.ino;
                d.ent.d_off = d.pos as i64;
                // 本项实长（尾按 8 对齐——musl getdents 惯例同形）。
                d.ent.d_reclen = ((19 + nl + 1 + 7) & !7) as u16;
                d.ent.d_type = DT_UNKNOWN;
                d.ent.d_name = [0; 256];
                d.ent.d_name[..nl].copy_from_slice(&name[..nl]);
                &mut d.ent
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn closedir(d: *mut DIR) -> c_int {
    if d.is_null() {
        crate::set_errno(crate::EBADF);
        return -1;
    }
    crate::alloc::free(d as *mut c_void);
    0
}

/// opendir 不占 fd（目录 blob 一次拉全）——诚实 EBADF 面。
#[no_mangle]
pub unsafe extern "C" fn dirfd(_d: *mut DIR) -> c_int {
    crate::set_errno(crate::EBADF);
    -1
}

// —— cwd 族（libc 侧字符串状态）——

#[no_mangle]
pub unsafe extern "C" fn getcwd(buf: *mut c_char, size: size_t) -> *mut c_char {
    unsafe {
        if buf.is_null() || size == 0 {
            crate::set_errno(crate::EINVAL);
            return ptr::null_mut();
        }
        // POSIX：size 需 ≥ 渲染串长 + NUL——render 帽传 size-1（串长），
        // 成功后补 NUL；size-1 < wire.len()+1 = ERANGE。
        let out = core::slice::from_raw_parts_mut(buf as *mut u8, size);
        match render_cwd(cwd_wire(), &mut out[..size - 1]) {
            Some(l) => {
                out[l] = 0;
                buf
            }
            None => {
                crate::set_errno(crate::ERANGE);
                ptr::null_mut()
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn chdir(path: *const c_char) -> c_int {
    unsafe {
        let mut wire = [0u8; PATH_MAX];
        let len = match resolve_to(path, &mut wire) {
            Ok(l) => l,
            Err(e) => {
                crate::set_errno(e);
                return -1;
            }
        };
        match rpc_stat(crate::io::fs_h(), &wire[..len]) {
            Ok((TYPE_DIR, _)) => {
                CWD[..len].copy_from_slice(&wire[..len]);
                CWD_LEN = len;
                0
            }
            Ok((_, _)) => {
                crate::set_errno(crate::ENOTDIR);
                -1
            }
            Err(e) => {
                crate::set_errno(errno_of(e));
                -1
            }
        }
    }
}

// —— 建删族 ——

unsafe fn mkdir_core(path: *const c_char) -> c_int {
    unsafe {
        let mut wire = [0u8; PATH_MAX];
        let len = match resolve_to(path, &mut wire) {
            Ok(l) => l,
            Err(e) => {
                crate::set_errno(e);
                return -1;
            }
        };
        match rpc_mkdir(crate::io::fs_h(), &wire[..len]) {
            Ok(()) => 0,
            Err(e) => {
                crate::set_errno(errno_of(e));
                -1
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn mkdir(path: *const c_char, _mode: u32) -> c_int {
    unsafe { mkdir_core(path) }
}

#[no_mangle]
pub unsafe extern "C" fn mkdirat(_dirfd: c_int, path: *const c_char, _mode: u32) -> c_int {
    unsafe { mkdir_core(path) }
}

unsafe fn unlink_core(path: *const c_char) -> c_int {
    unsafe {
        let mut wire = [0u8; PATH_MAX];
        let len = match resolve_to(path, &mut wire) {
            Ok(l) => l,
            Err(e) => {
                crate::set_errno(e);
                return -1;
            }
        };
        match rpc_unlink(crate::io::fs_h(), &wire[..len]) {
            Ok(()) => 0,
            Err(e) => {
                crate::set_errno(errno_of(e));
                -1
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn unlink(path: *const c_char) -> c_int {
    unsafe { unlink_core(path) }
}

#[no_mangle]
pub unsafe extern "C" fn unlinkat(_dirfd: c_int, path: *const c_char, _flags: c_int) -> c_int {
    unsafe { unlink_core(path) }
}

/// rmdir ≡ unlink（服务端 UNLINK 收文件与空目录——fstree 语义）。
#[no_mangle]
pub unsafe extern "C" fn rmdir(path: *const c_char) -> c_int {
    unsafe { unlink_core(path) }
}
