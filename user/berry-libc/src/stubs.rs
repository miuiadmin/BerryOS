//! ENOSYS/成功桩面 —— 附录 A「189 全定义」的机械档
//!
//! 三类宏（全部**零参**定义——AArch64 下被调方忽略多余寄存器实参合法，
//! 见 lib.rs ABI 口径；变参消费面走 vprintf 蹦床）：
//! - `neg!`：返 -1 + 置 errno（errno 取值跟 C 族语义：路径族 ENOENT、
//!   桩族 ENOSYS、fd 族 EBADF……）；
//! - `ok!`：成功谎报（无竞争锁族/特权空转/信号安装）；
//! - 少数签名敏感的（environ/posix_spawnp/getaddrinfo）手写——
//!   T-1 起 syscall/futex、sem、pthread key/生命周期族迁 `futex.rs`/
//!   `pthread.rs` 真身；A7-1 起 mmap/munmap 匿名档真身迁 `mmap.rs`
//!   （vmo/vmar_map 桥，docs/事件循环A7设计草案.md §3.1）——此处只剩
//!   attr getter 三桩。
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

// —— FS 路径面（C-1 刀 2 起 open/openat/opendir/stat/lstat/unlink/unlinkat/
//    rmdir/chdir/mkdir 迁 `fs.rs` 真实现；C-2a 起 rename 迁 fs.rs 组合真实现
//    ——copy+unlink 兑现 POSIX 面；余下无真面的照旧 ENOENT——std 侧
//    报「文件不存在」比「未实现」对最终用户更诚实，反正 v1 就是不存在）——
neg_int!(
    mkfifo => crate::ENOENT,
    symlink => crate::ENOENT,
    symlinkat => crate::ENOENT,
    linkat => crate::ENOENT,
    execvp => crate::ENOENT,
);

// —— xattr 族（C-2a rustix use-libc 后引入）：uu_cp/uu_ls 的属性保留/
//    显示路径引用（copy_xattrs、ls 的 security context 查询）。errno 取
//    **ENOTSUP(95) 非 ENOSYS(38)**（对抗审查实抓）：uucore fsxattr 的
//    is_xattr_unsupported 白名单只吞 ENOTSUP/EOPNOTSUPP——ENOSYS 会经
//    uu_cp -a（伪诊断）/--preserve=xattr（整单失败）传播；musl
//    ENOTSUP==EOPNOTSUPP==95，正是 Linux「文件系统不支持 xattr」的规范
//    档（GNU cp 对此静默跳过）。零参桩形态同上：AArch64 调用约定容忍
//    调用方多传寄存器参数，被调方忽略即可。读六族（get*/list*）POSIX
//    返回 ssize_t——同 readlink 档位纪律，见 neg_ssize! 注释。
neg_int!(
    setxattr => crate::ENOTSUP,
    lsetxattr => crate::ENOTSUP,
    fsetxattr => crate::ENOTSUP,
    removexattr => crate::ENOTSUP,
    lremovexattr => crate::ENOTSUP,
    fremovexattr => crate::ENOTSUP,
);

// —— FS 元数据/ENOSYS 面（fstat 刀 2 起迁 fs.rs——fd≥3 真/fd 0-2 保持
//    ENOSYS 旧面；utimensat/futimens C-2a 起改 no-op 成功桩见下方手写面；
//    fcntl C-2c 挂账刀起手写真面（console fd 克隆），移出本档）——
neg_int!(
    fchmod => crate::ENOSYS,
    fchmodat => crate::ENOSYS,
    fchown => crate::ENOSYS,
    chmod => crate::ENOSYS,
    chown => crate::ENOSYS,
    lchown => crate::ENOSYS,
    chroot => crate::ENOSYS,
    fsync => crate::ENOSYS,
    fdatasync => crate::ENOSYS,
    ftruncate => crate::ENOSYS,
    flock => crate::ENOSYS,
    ioctl => crate::ENOTTY,
    // mprotect 真身迁 mmap.rs（A7-1 修复刀：命中 EACCES/未命中 EINVAL
    // 分诊 + futex 锁）——桩位退役，避免与 #[no_mangle] 真身撞链。
);

/// fcntl（C-2c 挂账刀手写——旧 neg_int! EINVAL 桩退役）：真面仅
/// F_DUPFD(0)/F_DUPFD_CLOEXEC(1030) 的 fd 0/1/2 克隆（console 别名，
/// uu_head unix stdin 路径 `try_clone_to_owned` 的消费面——std
/// `sys::unix::fd::owned.rs` 同款调用，C-2c U3 步③ 挂账实录的解扣件）。
/// 分发表：fd 0-2 + dup cmd → 别名 alloc（满表 = EMFILE）；fd≥3 在册
/// fs fd → ENOSYS（loud 拒——客户端游标模型的表项是值语义，别名无独立
/// 游标可给，v1 无 fs fd 克隆面）；fd≥3 不在册 → EBADF；其余 cmd →
/// EINVAL（旧桩同值零漂移）。C 变参第 3 参按 int 收（F_DUPFD 的最低
/// 位号；AArch64 寄存器传参对变参/定参同形，lib.rs ABI 口径）。
#[no_mangle]
pub extern "C" fn fcntl(fd: c_int, cmd: c_int, arg: c_int) -> c_int {
    const F_DUPFD: c_int = 0;
    const F_DUPFD_CLOEXEC: c_int = 1030; // Linux 通用值（musl 同）
    if fd < 0 {
        crate::set_errno(crate::EBADF);
        return -1;
    }
    if (0..=2).contains(&fd) {
        if cmd == F_DUPFD || cmd == F_DUPFD_CLOEXEC {
            // POSIX F_DUPFD 边界（三镜审查口径修正）：arg 负或 ≥ 号段上限
            // （FD_BASE+FD_MAX+ALIAS_MAX = 15——本桩的 OPEN_MAX 档）→
            // EINVAL；段内无空位才 EMFILE。旧实现负 arg 钳 3、超窗归
            // EMFILE（现役唯一消费方 uu_head 传 0 不可达，纯口径面）。
            let ceiling = berry_fsclient::FD_BASE
                + (berry_fsclient::FD_MAX + berry_fsclient::ALIAS_MAX) as c_int;
            if arg < 0 || arg >= ceiling {
                crate::set_errno(crate::EINVAL);
                return -1;
            }
            return match crate::io::alias_alloc(fd, arg) {
                Some(n) => n,
                None => {
                    crate::set_errno(crate::EMFILE);
                    -1
                }
            };
        }
        crate::set_errno(crate::EINVAL);
        return -1;
    }
    if crate::fs::fd_valid(fd) {
        crate::set_errno(crate::ENOSYS);
        return -1;
    }
    // socket fd 腿（A3-1，net.rs）：fs miss 后查 socket 表（15..=22 段）。
    // F_GETFL(3)/F_SETFL(4) → 旗投影（O_RDWR | O_NONBLOCK——读写两用 +
    // 非阻塞旗，只认 O_NONBLOCK 一位写）；dup 族 → ENOSYS（**同 fs 在册
    // 口径**——dup socket v1 明弃，草案 §5）；其余 cmd → EINVAL。console/
    // fs 既有面零漂移（socket 段 fd 此前全走 EBADF；socket() 本身 A3-2
    // 才转真——本腿 = 表先立、执法面先行）。
    if crate::net::sock_valid(fd) {
        const F_GETFL: c_int = 3;
        const F_SETFL: c_int = 4;
        if cmd == F_DUPFD || cmd == F_DUPFD_CLOEXEC {
            crate::set_errno(crate::ENOSYS);
            return -1;
        }
        if cmd == F_GETFL {
            return match crate::net::sock_getfl(fd) {
                Some(fl) => fl,
                None => {
                    crate::set_errno(crate::EBADF);
                    -1
                }
            };
        }
        if cmd == F_SETFL {
            return match crate::net::sock_setfl(fd, arg) {
                Some(()) => 0,
                None => {
                    crate::set_errno(crate::EBADF);
                    -1
                }
            };
        }
        crate::set_errno(crate::EINVAL);
        return -1;
    }
    // epoll fd 腿（A4-1）：无旗面——epoll fd 唯一可做的是 ctl/wait/close，
    // 任何 fcntl cmd 恒 EINVAL（Linux 同档：epoll fd 无 FL 可读）。
    if crate::net::epoll_valid(fd) {
        crate::set_errno(crate::EINVAL);
        return -1;
    }
    crate::set_errno(crate::EBADF);
    -1
}

// —— tail 桩面（C-2c 挂账刀：uu_tail 的 -f/监测链——mio/inotify 全家。
//    挂账刀纪律「现探现补」：链接探针实证 7 枚符号全归 tail（-f 尾随
//    面 v1 无实现，ENOSYS = 诚实语义——tail 无 -f 路径全绿，见 bsh
//    u4_selftest 头注）；eventfd/inotify 两族维持合册——epoll 三桩
//    A4-1 转真出册（net.rs 真身：EpollTable 23..=26 + LT/timeout 三档）——
neg_int!(
    eventfd => crate::ENOSYS,
    inotify_init1 => crate::ENOSYS,
    inotify_add_watch => crate::ENOSYS,
    inotify_rm_watch => crate::ENOSYS,
);

// A7-4 段③a：sigprocmask 真身迁 signal.rs（掩码状态机 + oldset 严出
// 回写——本桩「谎报旧掩码全空 + 成功」形态退役；uu_env block_signal
// 链行为升级为真位运算，返回值面不变）。

/// realtime 信号界（uu_env realtime_signal_bounds 的链接面）：musl
/// 默认 SIGRTMIN=34 / SIGRTMAX=64（Linux glibc 差值是预留动态库槽位，
/// musl 无此面）——值恒定，env 的界检查获确定区间。
#[no_mangle]
pub extern "C" fn __libc_current_sigrtmin() -> c_int {
    34
}

#[no_mangle]
pub extern "C" fn __libc_current_sigrtmax() -> c_int {
    64
}

/// `sync`（C 原型 `void sync(void)`——libc crate 同形，C-2 刀 2；uu_df
/// `--sync` 消费，探针实证 uu_df 全链缺符号恰 sync+statfs 两枚）：空桩
/// 不是谎——v1 写通道逐 op 同步 commit（PUT staging + close TRUNC 即真写
/// 扇区，双 FAT 随写随新），无进程内缓冲可刷；「刷盘」在零缓冲文件系统
/// 上语义恒真（utimensat no-op 桩同款口径）。
#[no_mangle]
pub extern "C" fn sync() {}

// —— fd/dir 族（closedir/dirfd 刀 2 起迁 fs.rs；readdir 同）——
// kill/killpg 已迁 signal.rs 真身（A7-4 ③b：自杀档 raise 等价 / ESRCH
// 诚拒——EPERM 桩退役）。
neg_int!(
    dup => crate::EBADF,
    dup2 => crate::EBADF,
    waitpid => crate::ECHILD,
    waitid => crate::ECHILD,
    setsid => crate::EPERM,
    fork => crate::ENOSYS,
);

// —— socket 族残桩（A3-2 后形态）：socket/connect/bind/listen/accept(+4)/
//    send/recv/setsockopt/getsockopt 已转真（net.rs——RPC 面）；本块只剩
//    v1 无语义面的边角（socketpair/shutdown/getpeername/getsockname/pipe2
//    ——草案 §5「不做什么」，ENOSYS = 诚实语义）。send/recv 六读写族
//    POSIX 返回 ssize_t，余部在 neg_ssize! 对档。——
neg_int!(
    socketpair => crate::ENOSYS,
    shutdown => crate::ENOSYS,
    getpeername => crate::ENOSYS,
    getsockname => crate::ENOSYS,
    pipe2 => crate::ENOSYS,
);

neg_ssize!(
    // C-2a 档位纪律第一案（cp 毒路径实锤）：readlink 的 musl/POSIX ABI 返回
    // ssize_t（64 位）——std（musl 预编译）按 c_long 消费 x0 全宽。原 c_int
    // 桩只写 w0：x0 高 32 位为零 → 0x00000000ffffffff ≠ 64 位 -1 → std 判
    // 「成功」→ set_len(0xffffffff) 把桩根本没写过的堆残留当路径 →
    // Components 倒扫 D-abort（uucore locale 机 env::current_exe() 首踩，
    // lldb 回调断点 + 帧链 "/proc/self/exe" 14 字面量钉死）。errno 语义照旧
    // ENOENT：/proc v1 不存在 = 诚实失败，current_exe().ok() 自然走 None
    // 分支。**新桩入册先查 ABI 返回宽度：ssize_t/off_t/指针宽一律本档。**
    readlink => crate::ENOENT,
    // xattr 读六族：POSIX 返回 ssize_t（get*/list*）——uu_cp copy_xattrs 与
    // uu_ls security context 真实可达面，同毒机制防御性对档（写族 set*/
    // remove* 返回 int，留在 neg_int!）。errno 同写族取 ENOTSUP（白名单
    // 面要求，见上方注释）。
    getxattr => crate::ENOTSUP,
    lgetxattr => crate::ENOTSUP,
    fgetxattr => crate::ENOTSUP,
    listxattr => crate::ENOTSUP,
    llistxattr => crate::ENOTSUP,
    flistxattr => crate::ENOTSUP,
    // socket 读写余部（POSIX 返回 ssize_t）：send/recv 已转真（net.rs，
    // A3-2）；recvfrom/sendto/sendmsg/recvmsg v1 无数据报/散播面（草案
    // §5——TCP 流唯一），ENOSYS = 诚实语义。
    recvfrom => crate::ENOSYS,
    sendto => crate::ENOSYS,
    sendmsg => crate::ENOSYS,
    recvmsg => crate::ENOSYS,
    readv => crate::ENOSYS,
    pread => crate::ENOSYS,
    pwrite => crate::ENOSYS,
    preadv => crate::ENOSYS,
    pwritev => crate::ENOSYS,
    sendfile => crate::ENOSYS,
    splice => crate::ENOSYS,
    // C-2a：std io::copy 的 File→File 特化先试本调用——-1+ENOSYS 让它
    // 老实回落 read/write 循环（uu_cp 的 uucore 本地拷贝同款判据）。
    copy_file_range => crate::ENOSYS,
    // C-2a：uu_cp 设备文件拷贝路（nix::sys::stat::mknod，LTO 内联后真
    // 可达）。v1 无设备节点面，ENOSYS = 诚实语义：cp 打「未实现」诊断
    // 跳过，不谎言成功。
    mknod => crate::ENOSYS,
);

// —— 成功谎报档（无竞争锁族/特权空转——v1 无信号机制，
//    栈溢出 = EL0 fault 被杀可接受）——
// A7-4 段③a：信号族五符号（sigaction/signal/sigaddset/sigemptyset/
//    sigfillset）迁 signal.rs 真身出册；sigaltstack 留册（栈面无语义
//    谎报无害）。
ok!(
    madvise,
    prctl,
    setuid,
    setgid,
    setgroups,
    setpgid,
    setenv,
    unsetenv,
    sigaltstack,
    pthread_rwlock_rdlock,
    pthread_rwlock_wrlock,
    pthread_rwlock_unlock,
    pthread_setname_np,
    posix_spawn_file_actions_init,
    posix_spawn_file_actions_addchdir_np,
    posix_spawn_file_actions_adddup2,
    posix_spawn_file_actions_destroy,
    posix_spawnattr_init,
    posix_spawnattr_destroy,
    posix_spawnattr_setflags,
    posix_spawnattr_setpgroup,
    posix_spawnattr_setsigdefault,
    // A3-1：freeaddrinfo 摘出——真身迁 net.rs（链释放对偶 getaddrinfo 的
    // malloc 面；ok! 谎报成功版不再合法）。
    // C-2a：fadvise 是建议性接口（消费方对失败零处置义务）——谎报成功
    // 比 ENOSYS 更诚实于语义（「建议被忽略」本就是合法实现档）。
    posix_fadvise,
);

// —— 手写面（签名/返回语义敏感）——

// T-1：`syscall` 变参真号槽已迁 `futex.rs`（SYS_futex(98) 拦截 + 7 参定形；
// nr≠98 保持 set_errno(ENOSYS)+-1 零漂移）。

// A7-1：mmap/munmap 匿名档真身已迁 `mmap.rs`（ENOMEM 空桩/ok! 谎报桶
// 双退役——vmo_create+commit+vmar_map 桥 + region 表，头注见彼）。

// —— C-2a uutils core 批手写桩（12 bin 的链接/行为缺口；hexyl 面已够，
//    本批逐符号按上游源级降级路径定形）——

/// umask：返回进程 umask（v1 恒 0o022 = musl 默认档——mkdir 的最终 mode
/// 由 uu_mkdir 自己 0777&~umask 算，本桩只须值稳定）。
#[no_mangle]
pub extern "C" fn umask() -> u32 {
    0o022
}

// —— 身份族（v1 单 root 用户：uid=euid=gid=egid=0，无附属组）——
// getuid/getpid 在 sys.rs；此处补 e/g 面（uu_cp -p 走 geteuid 查保留面）。

#[no_mangle]
pub extern "C" fn geteuid() -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn getegid() -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn getgid() -> u32 {
    0
}

/// getgroups：返 0 = 无附属组（不写 list——musl 语义 0 组合法）。
#[no_mangle]
pub extern "C" fn getgroups() -> c_int {
    0
}

// —— passwd/group 数据库（无 /etc/passwd：NULL/ENOENT——上游 uucore
//    entries 对 None 回落数字 uid/gid 渲染，源级验证过的降级路）——

#[no_mangle]
pub extern "C" fn getpwuid() -> *mut c_void {
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn getpwnam() -> *mut c_void {
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn getgrgid() -> *mut c_void {
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn getgrnam() -> *mut c_void {
    ptr::null_mut()
}

/// _r 族：**全 5 参签名对真原型**（libc crate：uid/name、pwd/grp、buf、
/// buflen、result——result 是第 5 参落 x4；对抗审查实抓：旧单参形态绑
/// x0 = uid/name，真调用方的 result 指针无人写 = 调用即野读）。正返
/// ENOENT + *result 置 NULL（调用方两出口都看——只置一半是 UB 诱因）。
/// 零参宏不适用（出参必须写）。
#[no_mangle]
pub extern "C" fn getpwuid_r(
    _uid: u32,
    _pwd: *mut c_void,
    _buf: *mut c_void,
    _buflen: size_t,
    result: *mut *mut c_void,
) -> c_int {
    unsafe { *result = ptr::null_mut() };
    crate::ENOENT
}

#[no_mangle]
pub extern "C" fn getpwnam_r(
    _name: *const c_char,
    _pwd: *mut c_void,
    _buf: *mut c_void,
    _buflen: size_t,
    result: *mut *mut c_void,
) -> c_int {
    unsafe { *result = ptr::null_mut() };
    crate::ENOENT
}

#[no_mangle]
pub extern "C" fn getgrgid_r(
    _gid: u32,
    _grp: *mut c_void,
    _buf: *mut c_void,
    _buflen: size_t,
    result: *mut *mut c_void,
) -> c_int {
    unsafe { *result = ptr::null_mut() };
    crate::ENOENT
}

#[no_mangle]
pub extern "C" fn getgrnam_r(
    _name: *const c_char,
    _grp: *mut c_void,
    _buf: *mut c_void,
    _buflen: size_t,
    result: *mut *mut c_void,
) -> c_int {
    unsafe { *result = ptr::null_mut() };
    crate::ENOENT
}

// —— utimensat/futimens：no-op 成功桩——不是谎言：v1 FS 无时间戳面
// （fs.rs fill_stat 的 mtime 恒 0 = 「卷定值」，C-2a 诚实边界条款），
// 「设时」在时间戳恒定的文件系统上语义恒真。touch 因此全语义可用
// （建文件 + 更新时戳两半都真），违失败档（ENOSYS 桩）反把「已成功
// 建档」误报成错误。utimes 同理。

#[no_mangle]
pub extern "C" fn utimensat() -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn futimens() -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn utimes() -> c_int {
    0
}

/// musl 的 strerror_r 即 XPG int 版（glibc 才有双版）——libc crate 对
/// musl 恒引用本名，`__xpg_strerror_r` 单独在册不够。
#[no_mangle]
pub extern "C" fn strerror_r(errnum: c_int, buf: *mut c_char, buflen: size_t) -> c_int {
    __xpg_strerror_r(errnum, buf, buflen)
}

// pow —— C-2a 链接缺口三件之三：uu_cp 的 indicatif 进度条 ETA 平滑是
// llvm.pow.f64 libcall，LTO 全图 DCE 后真可达（非死码，必须给定义）。
// v1 无 libm 服务，落地纯算术实现：pow(x,y) = exp2(y·log2(x))——log2
// 用 atanh 级数（u=(m-1)/(m+1)，|u|≤1/3，16 项），exp2 用 e^{f·ln2}
// Taylor（15 项）+ 指数位合成 2^k；全程只用 f64 基本运算与位分解/位
// 截断（core 无 floor/trunc 家族，见 ftrunc/ffloor），零 libm 循环依赖。
// 相对精度 ~1e-12
// （ETA/显示面远超所需）；边角按 IEEE：y=0→1、负底数整指数可算非整
// NaN、上溢 INF、次正规区折返零（调用面不可达）。
#[no_mangle]
pub extern "C" fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 || x == 1.0 {
        return 1.0;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f64::INFINITY };
    }
    if x == f64::INFINITY {
        return if y > 0.0 { f64::INFINITY } else { 0.0 };
    }
    if x < 0.0 {
        if y != ftrunc(y) {
            return f64::NAN;
        }
        // 巨 |y| 饱和转换后奇偶判不真，但彼时结果已是 0/INF，符号无观感
        let odd = ((y as i64) & 1) == 1;
        let r = pow(-x, y);
        return if odd { -r } else { r };
    }
    exp2_impl(y * log2(x))
}

/// ftrunc —— 位运算截断（core 无 floor/trunc 家族）：尾数低于整数位的
/// 位清零；|v|<1 → ±0（保号）；exp=0x7ff（Inf/NaN）原样返回（本面
/// 不可达，防御性）。
fn ftrunc(v: f64) -> f64 {
    let b = v.to_bits();
    let exp = ((b >> 52) & 0x7ff) as i64;
    if exp < 1023 {
        return f64::from_bits(b & (1u64 << 63)); // ±0
    }
    let int_bits = exp - 1023; // 值的指数（≥52 即纯整数位形）
    if int_bits >= 52 {
        return v;
    }
    let mask = !((1u64 << (52 - int_bits as u32)) - 1);
    f64::from_bits(b & mask)
}

/// ffloor —— floor 的截断合成（有限值精确）：负非整截断后 -1。
fn ffloor(v: f64) -> f64 {
    let t = ftrunc(v);
    if v >= 0.0 || v == t {
        t
    } else {
        t - 1.0
    }
}

/// log2（x>0 有限）：m∈[1,2) 位分解 + atanh 级数。
fn log2(x: f64) -> f64 {
    let bits = x.to_bits();
    let e = (((bits >> 52) & 0x7ff) as i64 - 1023) as f64;
    let m = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1023u64 << 52));
    let u = (m - 1.0) / (m + 1.0);
    let u2 = u * u;
    let mut series = 1.0f64;
    let mut p = u2;
    let mut i = 3.0;
    while i <= 31.0 {
        series += p / i;
        p *= u2;
        i += 2.0;
    }
    e + 2.0 * u * series / core::f64::consts::LN_2
}

/// 2^t（t 有限）：k=floor 分解 + e^{f·ln2} Taylor + 指数位合成。
/// C-2c 起改名 exp2_impl 保私有——同名 C 符号 exp2 的导出面见下方
/// C-2c 段转发（libcall 面 uu_sort 可达）。
fn exp2_impl(t: f64) -> f64 {
    if t < -1022.0 {
        return 0.0; // 次正规区折返零（ETA 面不可达）
    }
    if t > 1023.0 {
        return f64::INFINITY;
    }
    let k = ffloor(t);
    let s = (t - k) * core::f64::consts::LN_2; // s∈[0,ln2)
    let mut term = 1.0f64;
    let mut sum = 1.0f64;
    for i in 1..=15 {
        term *= s / (i as f64);
        sum += term;
    }
    sum * f64::from_bits(((1023i64 + k as i64) as u64) << 52)
}

// —— C-2c 中批链接缺口四件（uu_sort 的 locale/数值面；2026-10-25
//    收敛树链接实证清单——text 批深坑件退出后按缺回补）——

// locale 双件（uu_sort -M 月份名面——uucore::features::i18n::datetime::
// get_locale_months 真消费；2026-10-25 berryos-99 剥桩重链实证归因）：
// v1 唯一 locale = "C"/ASCII。
// setlocale 恒答 "C"（musl 语义：返回现 locale 名——strerror 同款 rodata
// 指针形态，静态串免分配）；nl_langinfo 按 item 分诊（C-2c 挂账刀 U4 首跑
// 实测修订：原「问啥都答 UTF-8」把 uu_date 的 _DATE_FMT(0x2006c——uu_date
// locale.rs 手卷的 glibc nl_item，libc crate 未收录故上游自定) 查询也答成
// "UTF-8"——get_locale_format_string 拿它当日期格式串、ensure_timezone_
// in_format 再补 " %Z" → date 默认输出恒 "UTF-8 UTC"）：
//   CODESET(14——libc crate linux 面 glibc 序 nl_item) → "UTF-8"（i18n
//   codeset 查询面，语义保真）；
//   其余（_DATE_FMT/D_T_FMT/ABMON 族…）→ ""（空串）——C locale 之外的
//   locale 数据 v1 一律不支持。uu_date 的 !is_empty 过滤器吃空串后落内建
//   回退 "%a %b %e %X %Z %Y"（GNU date 同序输出）；uucore get_locale_months
//   的 ABMON 面有 should_use_icu_locale() 前置门（LC_TIME 缺席 → "und" →
//   false），运行期不可达——空串不会 poison 月份名。
#[no_mangle]
pub extern "C" fn setlocale(_category: c_int, _locale: *const c_char) -> *mut c_char {
    let s: &[u8] = b"C\0";
    s.as_ptr() as *mut c_char
}

#[no_mangle]
pub extern "C" fn nl_langinfo(item: c_int) -> *const c_char {
    const NL_CODESET: c_int = 14; // libc crate linux 面 glibc 序（langinfo.h）
    static EMPTY: &[u8] = b"\0";
    static UTF8: &[u8] = b"UTF-8\0";
    match item {
        NL_CODESET => UTF8.as_ptr() as *const c_char,
        _ => EMPTY.as_ptr() as *const c_char,
    }
}

// exp2/log10 —— C-2c libcall 面（uu_sort 数值排序的 bigdecimal 链：
// exp2 ← BigDecimalRef::inverse_with_context、log10 ← pow_u64_with_context；
// 2026-10-25 剥桩重链实证归因——head/wc/cut/tr/uniq 五件零依赖）：导出 C 符号
// 转发 pow 同族纯算术实现（见上 C-2a 段）；log10(x) = log2(x)/log2(10)，
// 边角按 IEEE：log10(±0) = -INF、负数/NaN → NaN。
#[no_mangle]
pub extern "C" fn exp2(t: f64) -> f64 {
    exp2_impl(t)
}

#[no_mangle]
pub extern "C" fn log10(x: f64) -> f64 {
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x < 0.0 || x.is_nan() {
        return f64::NAN;
    }
    log2(x) / 3.3219280948873623
}

// —— C-2c 中批链接缺口（uu_sort 的 ctrlc/tempfile/rustix 链五件；cut/
//    tr/uniq 上游纯输入面零新缺口。2026-10-25 sort 首轮链接实证清单）——

/// sysinfo —— uu_sort buffer_hint::physical_memory_bytes 唯一消费面：只读
/// totalram 做外排缓冲启发（clamp_hint 钳 [512KiB, 1GiB]——纯性能参数非
/// 正确性）。musl aarch64 布局 totalram@32（uptime 8 + loads 24）；v1 无
/// RAM 探测面，定值 128MiB = QEMU 主 smoke 定值（-m 128M）；rustix 的
/// sysinfo() 非 Result 面（失败路不可达），返 0 + 全结构零填（364B 含
/// __reserved）。
#[no_mangle]
pub extern "C" fn sysinfo(info: *mut c_void) -> c_int {
    unsafe {
        if !info.is_null() {
            core::ptr::write_bytes(info as *mut u8, 0, 364);
            (info as *mut u64).add(4).write(128 * 1024 * 1024);
        }
    }
    0
}

/// getrlimit —— uu_sort fd_soft_limit（rustix::process::getrlimit 直映射）：
/// 读 rlim_cur 收敛合并段并发开档数。**必须写有限值**：RLIM_INFINITY
/// (u64::MAX) 经 rustix 映 None → 上游 `.current.ok_or(...)` 直接用法错
/// exit 2。1024 = musl soft limit 默认档；struct rlimit = {cur, max} u64×2。
#[no_mangle]
pub extern "C" fn getrlimit(_resource: c_int, rlim: *mut c_void) -> c_int {
    unsafe {
        if !rlim.is_null() {
            let r = rlim as *mut u64;
            r.add(0).write(1024);
            r.add(1).write(1024);
        }
    }
    0
}

/// access —— uu_sort check_inputs 的输入可读探测（rustix::fs::access →
/// libc::access；GNU sort 同款「先 access 后 open」防 FIFO 阻塞/档位膨
/// 胀）。v1 无权限位执法且进程恒 root：READ_OK ⟺ 路径存在——复用 fs.rs
/// stat 真实现（RPC 往返），0/-1 + errno 约定同 stat（ENOENT/EACCES…）。
#[no_mangle]
pub extern "C" fn access(path: *const c_char, _mode: c_int) -> c_int {
    let mut st: crate::fs::Stat = unsafe { core::mem::zeroed() };
    unsafe { crate::fs::stat(path, &mut st) }
}

// sem 三件 —— T-1 已迁 `pthread.rs` 真身（ctrlc 后台线程经 pthread_create
// 真跑后 sem_wait_forever **运行时可达**：计数 ≤0 → futex 真睡——v1 无信号
// 面 = 永睡无害；旧 EAGAIN 桩的热自旋（每轮醒-退-再 spawn）是其反面教训，
// 与 pthread 真身同刀落地是纪律）。

// getcwd（刀 2 起迁 fs.rs 真实现——cwd 字符串 + render_cwd/ERANGE 面）。

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

// readdir（刀 2 起迁 fs.rs 真实现——DIR blob 游标 + musl dirent 布局）。

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

/// poll（A3-2 改造）：socket 项（15..=22 段）→ `net::sock_poll_one`
/// 单 fd 快照（revents 掩码 POSIX 口径——fsclient `poll_revents` 钉板）；
/// console/fs 项 revents = 0（既有面零漂移——sanitize_standard_fds 的
/// fd 0/1/2 探测照旧返回 0）。返回值 = revents 非零项计数。timeout v1
/// 忽略（恒立即返——阻塞档留 A4 事件循环刀，届时配 sleep 节拍）。
#[no_mangle]
pub extern "C" fn poll(fds: *mut PollFd, nfds: u64, _timeout: c_int) -> c_int {
    let mut ready = 0;
    if !fds.is_null() {
        unsafe {
            for i in 0..nfds as usize {
                let p = fds.add(i);
                (*p).revents = if crate::net::sock_valid((*p).fd) {
                    crate::net::sock_poll_one((*p).fd, (*p).events)
                } else {
                    0
                };
                if (*p).revents != 0 {
                    ready += 1;
                }
            }
        }
    }
    ready
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
        95 => b"Operation not supported\0",
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

// —— getaddrinfo 族（A3-1：真身 = net.rs，IP 字面量档）——
// getaddrinfo/freeaddrinfo 旧桩已删：net.rs 双 #[no_mangle] 真实现
// （parse_ipv4 + hints 域检 + malloc 单节点链），此处保留符号指针即可。

#[no_mangle]
pub extern "C" fn gai_strerror(code: c_int) -> *const c_char {
    // 返回文案对齐 musl netdb.c 文案面（A3-1 扩三档；A4-2 扩 EAI_AGAIN）。
    let s: &[u8] = match code {
        crate::EAI_MEMORY => b"Out of memory\0",
        crate::EAI_FAMILY => b"Address family not supported\0",
        crate::EAI_SERVICE => b"Service not supported\0",
        crate::EAI_AGAIN => b"Temporary failure in name resolution\0",
        crate::EAI_NONAME => b"Name or service not known\0",
        _ => b"Name or service not known\0",
    };
    s.as_ptr() as *const c_char
}

// —— pthread key/specific + 身份/生命周期：T-1 已迁 `pthread.rs` 真身 ——
// （key 三件 per-thread 化（specific 入 struct pthread + PKEY_DTOR 进程级
// futex 锁）；self/create/join/detach/attr 三件 = 栈 VMO + 蹦床 TLS 装配 +
// join TERMINATED 信号面。旧「单线程真存取」PKEY_VALS 全局数组随之退役。）

// —— pthread 身份/attr（保留面）——

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
