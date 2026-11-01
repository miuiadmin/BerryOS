//! # berry-fsclient —— fs 服务客户端会话机 + libc 文件面纯函数（C-1 刀 1）
//!
//! ## Overview (for crates.io users)
//!
//! Client side of the [BerryOS](https://github.com/miuiadmin/BerryOS)
//! filesystem-service RPC, in two layers: pure faces (fd-table, cwd
//! joining, stat shaping, dirent decoding) plus the syscall-touching
//! session core (`cfg(not(test))` — host tests never link the syscall
//! surface). It feeds the berry-libc file layer. v1 honest gaps (no
//! `.`/`..` normalization, write-open truncation semantics) are
//! documented in the zh-CN sections below.
//!
//! Status: experimental (0.1.0) — the API moves with the BerryOS v1
//! wire format. Repo docs are mostly zh-CN.
//!
//! 合约源 = `docs/VM里程碑设计草案.md` §C-1。两层：
//!
//! - **纯函数面**（本文件，host 测试）：fd 3+ 表项的客户端游标模型、cwd
//!   拼接、stat 整形、dirent 解码、ST_*→FsErr 映射——libc（刀 2）把这些
//!   组装成 open/close/read/write/lseek/stat/opendir/readdir/chdir/getcwd。
//! - **会话机**（`rpc` 模块，`cfg(not(test))`）：自 bsh 迁居的 fs_rpc 核心
//!   （严格串行 RPC、任一收侧超时即毒化会话、90s commit / 10s 常规双帽、
//!   EBUSY 阻塞重试形态）+ fd 面分片 op。协议面全按 berry-fsproto 单源
//!   消费：v1 四 op（PUT/TRUNC/GET/LIST——GET/PUT 自带 off，客户端游标
//!   = 服务端无会话）+ v2 五 op（LOOKUP/MKDIR/UNLINK/READDIR/STAT）原样
//!   零改动；C-2 刀 2 新增 STATFS（卷级几何/空闲真值，无路径参——df/
//!   statfs 桩的原料），协议 9→10 op。
//!
//! ## C-1 两裁决（随五刀拆解单批准）
//!
//! 1. **fd 表 = libc 侧客户端游标**：fd≥3 表项存 `{wire 路径, 偏移, 标志}`；
//!    read → `GET(path, off)`、write → `PUT(path, off)`。
//! 2. **写通道 commit 点 = close**：写 fd 走 PUT 累积 staging，`close()` 以
//!    最终游标发 `TRUNC(size)` 提交——与 bsh `>` 两段式同款，fsmgr 语义
//!    零改动（`take` 归还表项、调用方按 `writable` 决定 TRUNC 即此形状）。
//!
//! ## v1 诚实缺口（文档化，v2 再议）
//!
//! - 无 `.`/`..` 归一化（fsproto 段字符集放行它们、fsmgr 无点语义——与
//!   bsh 现行为同形零漂移；POSIX 归一化随 v2）。
//! - 写打开一律截断语义（close 时 TRUNC(游标)——`O_APPEND`/不截断打开 v2）。
//! - lseek 越过 `FILE_SIZE_MAX` 拒绝（协议 off 域帽；POSIX 允许越界 seek、
//!   帽内才可达）。
//! - dirent 无 d_type（READDIR blob 只给名——DT_UNKNOWN，需要类型走 STAT）。
//!
//! 帽常量与状态码自 berry-fsproto 单源引；`berry-fstree`/`berry-fatfs` 同
//! 先例：「不长第二套」。

#![cfg_attr(not(test), no_std)]

// 会话机 + fd 面 op（host 测试只盖纯函数面——berry-sys 符号宿主不存在，
// 不链接；none/musl target 编译由 workspace cargo build 覆盖）。
#[cfg(not(test))]
mod rpc;
#[cfg(not(test))]
pub use rpc::*;

// 单源再出口：libc（刀 2）只 dep 本 crate 即得帽常量；Statfs（C-2 刀 2
// statfs() 的应答结构）同源再出口——消费方不直 dep berry-fsproto。
pub use berry_fsproto::{
    CHUNK, FILE_SIZE_MAX, NAME_MAX, PATH_DEPTH_MAX, PATH_MAX, TYPE_DIR, TYPE_FILE, Statfs,
};

use berry_fsproto::{ST_BAD, ST_EXISTS, ST_FULL, ST_NOEMPTY, ST_NOENT};

/// fs RPC 错误面（自 bsh 迁居，text/code = shell 诊断/退出码面）。NoFs =
/// fs 服务未拉起（fs gift == 0——降级不死调用方）；Rpc = 通道/协议断链
/// （收侧超时（含毒化）/解析失败/发送失败——服务死或拓扑破缺）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsErr {
    NoFs,
    Noent,
    Full,
    Bad,
    Exists,
    NotEmpty,
    Rpc,
}

impl FsErr {
    pub fn text(&self) -> &'static str {
        match self {
            FsErr::NoFs => "fs not available",
            FsErr::Noent => "not found",
            FsErr::Full => "no space",
            FsErr::Bad => "bad path",
            FsErr::Exists => "exists",
            FsErr::NotEmpty => "dir not empty",
            FsErr::Rpc => "fs unreachable",
        }
    }
    /// shell 退出码面（bsh 现值迁移：Noent=127 not found，其余 1）。
    pub fn code(&self) -> i32 {
        match self {
            FsErr::NoFs => 1,
            FsErr::Noent => 127,
            FsErr::Full => 1,
            FsErr::Bad => 1,
            FsErr::Exists => 1,
            FsErr::NotEmpty => 1,
            FsErr::Rpc => 1,
        }
    }
}

/// 应答 st 码 → FsErr（未知码 = Rpc——协议面只认在册常量；bsh `st_err`
/// 迁居）。errno 整型映射归 libc（刀 2）——协议面错误与 errno 是两层。
pub fn st_to_err(st: u8) -> FsErr {
    match st {
        ST_NOENT => FsErr::Noent,
        ST_FULL => FsErr::Full,
        ST_BAD => FsErr::Bad,
        ST_EXISTS => FsErr::Exists,
        ST_NOEMPTY => FsErr::NotEmpty,
        _ => FsErr::Rpc,
    }
}

// —— fd 表（客户端游标模型，裁决 1）——

/// fd 表基址：0/1/2 = console/stdin（libc io.rs 现路径一字不动——R6），
/// fs-backed fd 自 3 起。
pub const FD_BASE: i32 = 3;
/// fd 表容量（8 槽 fd 3..=10：RAM 树用户可用文件帽 8 同量级
/// （FILE_COUNT_MAX=9 含 mtab 预建保留槽，C-2 刀 2 起）；满 = EMFILE
/// 面）。表项 ≈136B，libc 侧静态实例 ≈1.1KiB。
pub const FD_MAX: usize = 8;

/// fd≥3 表项：wire 路径快照 + 客户端游标 + 打开标志。open 时快照路径
/// （cwd 解析的产物——open 后 chdir 不影响已开 fd，POSIX 语义），read/
/// write 各自按游标发 GET/PUT。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FdEntry {
    path: [u8; PATH_MAX],
    path_len: usize,
    /// 客户端游标：read = 顺序读位、write = 已写总量（close 时 TRUNC 的
    /// size 即它）。
    pub offset: u32,
    pub readable: bool,
    pub writable: bool,
}

impl FdEntry {
    /// 新表项（游标 0 起）：wire 超长（>PATH_MAX）= None（调用方在 resolve
    /// 阶段已拦，此处防御性兜底）。
    pub fn new(wire: &[u8], readable: bool, writable: bool) -> Option<Self> {
        if wire.len() > PATH_MAX {
            return None;
        }
        let mut path = [0u8; PATH_MAX];
        path[..wire.len()].copy_from_slice(wire);
        Some(Self {
            path,
            path_len: wire.len(),
            offset: 0,
            readable,
            writable,
        })
    }
    /// wire 路径（NUL 分段，fsproto v2 形）。
    pub fn wire(&self) -> &[u8] {
        &self.path[..self.path_len]
    }
    /// 游标步进（read 返回量 / write 写入量；饱和加防回卷）。
    pub fn advance(&mut self, n: usize) {
        self.offset = self.offset.saturating_add(n as u32);
    }
}

/// fd 3+ 表：`None` 槽 = 空。libc 侧静态实例（单线程根基——berry-libc
/// `static mut` 直写合法，pthread 面永不出线程）。
pub struct FdTable {
    slots: [Option<FdEntry>; FD_MAX],
}

impl FdTable {
    pub const fn new() -> Self {
        Self {
            slots: [None; FD_MAX],
        }
    }

    /// POSIX 最低空位安装 → fd（3..=10）；满表 = None（EMFILE 面）。
    pub fn alloc(&mut self, e: FdEntry) -> Option<i32> {
        for (i, s) in self.slots.iter_mut().enumerate() {
            if s.is_none() {
                *s = Some(e);
                return Some(FD_BASE + i as i32);
            }
        }
        None
    }

    /// [`FdTable::alloc`] 的避位版（C-2c 挂账刀）：跳过 `occupied(fd)` 为
    /// 真的编号（libc open 面传 console 别名在册位——别名与 FDT 共段的
    /// 反向碰撞防线，正方向 = `AliasTable::alloc` 跳 FDT）。避位后满表 =
    /// None（EMFILE——被别名占号的空槽不给 fd，双语义同号静默错优先防）。
    pub fn alloc_skipping(
        &mut self,
        e: FdEntry,
        occupied: impl Fn(i32) -> bool,
    ) -> Option<i32> {
        for (i, s) in self.slots.iter_mut().enumerate() {
            let fd = FD_BASE + i as i32;
            if s.is_none() && !occupied(fd) {
                *s = Some(e);
                return Some(fd);
            }
        }
        None
    }

    pub fn get(&self, fd: i32) -> Option<&FdEntry> {
        self.index(fd).and_then(|i| self.slots[i].as_ref())
    }

    pub fn get_mut(&mut self, fd: i32) -> Option<&mut FdEntry> {
        match self.index(fd) {
            Some(i) => self.slots[i].as_mut(),
            None => None,
        }
    }

    /// 摘除并归还表项（close 面）：调用方按 `writable` 决定是否以最终
    /// `offset` 发 TRUNC 提交（裁决 2：close 即 commit）。不在册（含
    /// fd<3 / 越界 / 空槽）= None（EBADF 面）。
    pub fn take(&mut self, fd: i32) -> Option<FdEntry> {
        match self.index(fd) {
            Some(i) => self.slots[i].take(),
            None => None,
        }
    }

    fn index(&self, fd: i32) -> Option<usize> {
        if fd < FD_BASE {
            return None;
        }
        let i = (fd - FD_BASE) as usize;
        (i < FD_MAX).then_some(i)
    }
}

// —— console 别名面（C-2c 挂账刀：fcntl F_DUPFD/F_DUPFD_CLOEXEC 的 fd
//    0/1/2 克隆——uu_head stdin 路径 try_clone_to_owned 的真身）——

/// 别名容量（4 槽）：单程序面 uu_head owned stdin 双克隆 ≤2，4 = 两倍
/// 余量；满表/越段帽 = None（EMFILE 面）。
pub const ALIAS_MAX: usize = 4;

/// console fd（0/1/2）别名表：槽 = `Some((alias_fd, base_fd))`。**别名与
/// FDT 共享同一段 fd 编号空间**（3 起）——alloc 跳过 FDT 在册位 + 已用
/// 别名位（双语义碰撞防线：同号既在 FDT 又是别名 = read/write 先撞别名
/// 翻译面，FDT 表项结构性不可达 = 静默错——两侧 alloc 互让，见
/// `FdTable::alloc_skipping` 反向防线）。FDT 满时别名越 10 上探至
/// `FD_BASE + FD_MAX + ALIAS_MAX`（排他）——克隆不被 FDT 挤死（POSIX
/// F_DUPFD 语义：新 fd ≥ start 的最低可用位，不限于某段）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AliasTable {
    slots: [Option<(i32, i32)>; ALIAS_MAX],
}

impl AliasTable {
    pub const fn new() -> Self {
        Self {
            slots: [None; ALIAS_MAX],
        }
    }

    /// console `base`（0/1/2）装一条 ≥`start` 的最低可用别名，返回新 fd。
    /// 占用判据 = FDT 在册 ∪ 已用别名位；None = 槽满或段帽尽（EMFILE）。
    /// 坏 base（非 0/1/2）= None（fcntl 分发层的 EINVAL/EBADF 前置已拦，
    /// 此处防御性兜底——纯函数不信任调用方）。
    pub fn alloc(&mut self, base: i32, start: i32, fdt: &FdTable) -> Option<i32> {
        if !(0..=2).contains(&base) {
            return None;
        }
        let lo = start.max(FD_BASE);
        let hi = FD_BASE + (FD_MAX + ALIAS_MAX) as i32;
        let mut fd = lo;
        while fd < hi {
            if fdt.get(fd).is_none() && self.base_of(fd).is_none() {
                let slot = self.slots.iter_mut().find(|s| s.is_none())?;
                *slot = Some((fd, base));
                return Some(fd);
            }
            fd += 1;
        }
        None
    }

    /// 别名 → 底 fd（0/1/2）；非别名 = None。
    pub fn base_of(&self, fd: i32) -> Option<i32> {
        self.slots.iter().find_map(|s| match *s {
            Some((a, b)) if a == fd => Some(b),
            _ => None,
        })
    }

    /// 摘除别名槽（close 面）：Some(base) / None（非别名 = EBADF 面）。
    pub fn take(&mut self, fd: i32) -> Option<i32> {
        for s in self.slots.iter_mut() {
            if matches!(*s, Some((a, _)) if a == fd) {
                let base = s.unwrap().1;
                *s = None;
                return Some(base);
            }
        }
        None
    }
}

// —— socket 表（A3-1：POSIX socket fd 面，docs/事件循环A3设计草案.md
//    §3.1——fd↔netd 会话映射的宿主表）——

/// socket fd 段基址：15 起。别名/fs 段扫描上界 = `FD_BASE + (FD_MAX +
/// ALIAS_MAX)` = 15（排他——`AliasTable::alloc` 的 hi 与 fcntl F_DUPFD
/// 的 ceiling 同源），socket 段与之结构性无碰撞（下方 const 断言钉板：
/// 两表独立演化时编译期执法，草案 §5 号段膨胀防线）。
pub const SOCK_FD_BASE: i32 = 15;
/// socket 表容量（8 槽 fd 15..=22）：与 netdproto `TCP_SESSIONS_MAX=8`
/// 同源单一定值（A3-2 接 RPC 面时补 libc→netdproto 依赖 + const 断言钉
/// 同源关系——依赖随该刀加，本刀注释注记，FD_MAX↔FILE_COUNT_MAX 注释
/// 先例）；满表 = EMFILE 面。
pub const SOCK_FD_MAX: usize = 8;

/// 段免疫钉板：fs/别名段扫描上界 ≤ socket 段基址（草案 §5）。
const _: () = assert!(FD_BASE + (FD_MAX + ALIAS_MAX) as i32 <= SOCK_FD_BASE);

/// AF_INET（`sys/socket.h` 定值——socket() 域执法消费面）。
pub const AF_INET: i32 = 2;
/// SOCK_STREAM（同上）。
pub const SOCK_STREAM: i32 = 1;
/// IPPROTO_TCP（`netinet/in.h` 定值；协议 0 = 默认 = STREAM 上即 TCP）。
pub const IPPROTO_TCP: i32 = 6;

/// socket() 域执法错误面（errno 整型映射归 libc——协议面错误与 errno
/// 是两层，FsErr/st_to_err 同先例）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SockDomErr {
    /// af 非 AF_INET → EAFNOSUPPORT。
    Af,
    /// type 非 SOCK_STREAM → ESOCKTNOSUPPORT。
    Type,
    /// proto ∉ {0, IPPROTO_TCP} → EPROTONOSUPPORT。
    Proto,
}

/// socket(af, ty, proto) 域执法：v1 只收 AF_INET + SOCK_STREAM + proto
/// ∈ {0, IPPROTO_TCP}（AF_INET+DGRAM/UDP、raw、UNIX 面 v1 不做——草案
/// §5「不做什么」；POSIX 语义 proto=0 = 默认协议）。域外 → 对应错误档。
pub fn sock_domain_err(af: i32, ty: i32, proto: i32) -> Result<(), SockDomErr> {
    if af != AF_INET {
        return Err(SockDomErr::Af);
    }
    if ty != SOCK_STREAM {
        return Err(SockDomErr::Type);
    }
    if proto != 0 && proto != IPPROTO_TCP {
        return Err(SockDomErr::Proto);
    }
    Ok(())
}

/// socket 表项生命周期（草案 §3.1 四态）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SockKind {
    /// socket() 已建未连（connect 前，sid=0）。
    Unconnected,
    /// connect 完成（TCP 客户端会话）。
    Client,
    /// bind+listen 完成（sid = listen 会话）。
    Listen,
    /// accept 产出（服务端数据会话）。
    Server,
}

/// socket fd 表项（值语义 Copy）：fd↔sid 映射 + F_SETFL 非阻塞旗 +
/// 本地端口记账（bind/listen 面——netdproto LISTEN 自带 port，bind 与
/// listen 拆 POSIX 面、合 RPC 面，草案 §3.2）。sid = netd 会话 id
/// （槽号+1，0 = 「无会话」哨兵——netdproto 同源口径）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SockEntry {
    pub kind: SockKind,
    pub port: u16,
    pub sid: u32,
    /// O_NONBLOCK 旗（fcntl F_SETFL 面投影，libc net.rs 消费）。
    pub nonblock: bool,
}

/// socket fd 表（libc 侧静态实例，单线程根基同 FdTable）：`None` 槽 =
/// 空。fd = `SOCK_FD_BASE + 槽号`（15..=22）。
pub struct SocketTable {
    slots: [Option<SockEntry>; SOCK_FD_MAX],
}

impl SocketTable {
    pub const fn new() -> Self {
        Self {
            slots: [None; SOCK_FD_MAX],
        }
    }

    /// 最低空位安装 → fd（15..=22）；满表 = None（EMFILE 面）。表项由
    /// 调用方按生命周期阶段构造（get_mut 状态迁移面）。
    pub fn alloc(&mut self, e: SockEntry) -> Option<i32> {
        for (i, s) in self.slots.iter_mut().enumerate() {
            if s.is_none() {
                *s = Some(e);
                return Some(SOCK_FD_BASE + i as i32);
            }
        }
        None
    }

    pub fn get(&self, fd: i32) -> Option<&SockEntry> {
        self.index(fd).and_then(|i| self.slots[i].as_ref())
    }

    pub fn get_mut(&mut self, fd: i32) -> Option<&mut SockEntry> {
        match self.index(fd) {
            Some(i) => self.slots[i].as_mut(),
            None => None,
        }
    }

    /// 摘除并归还表项（close 面）：None = 不在册（fd 越段/空槽——EBADF
    /// 面）。
    pub fn take(&mut self, fd: i32) -> Option<SockEntry> {
        match self.index(fd) {
            Some(i) => self.slots[i].take(),
            None => None,
        }
    }

    fn index(&self, fd: i32) -> Option<usize> {
        if fd < SOCK_FD_BASE {
            return None;
        }
        let i = (fd - SOCK_FD_BASE) as usize;
        (i < SOCK_FD_MAX).then_some(i)
    }
}

// —— epoll 表（A4-1：POSIX epoll fd 面，docs/事件循环A4设计草案.md
//    §3.1——interest 注册的宿主表；语义蓝本 = berry-poll LT/poll-first/
//    timeout 三档，对象面 = socket fd 快照）——

/// epoll fd 段基址：23 起（socket 段 15..=22 之上；段界 const 断言
/// 钉板——fd 四段（fs/console 别名/socket/epoll）独立演化结构性免疫）。
pub const EPOLL_FD_BASE: i32 = 23;
/// epoll 表容量（4 槽 fd 23..=26）：v1 消费面 = 单进程事件循环 1-2 实例
/// （容量自定值，非协议同源）；满表 = EMFILE 面。
pub const EPOLL_FD_MAX: usize = 4;
/// 单 epoll 实例 interest 容量：与 socket 段容量**同源**（下方 const
/// 断言——distinct-target ADD（Exists 守卫）最多注册 SOCK_FD_MAX 项，
/// 恰满帽：**ENOSPC 结构性不可达、Full 是独立演化守卫**（两常量解耦
/// 演化时先于越界炸响——A4-1 首查实录，见草案 §7 追记）。
pub const EPOLL_ITEMS_MAX: usize = 8;

/// 段免疫钉板：socket 段上界 ≤ epoll 段基址（SocketTable 同法）。
const _: () = assert!(SOCK_FD_BASE + SOCK_FD_MAX as i32 <= EPOLL_FD_BASE);

/// 容量同源钉板：interest 帽 == socket 段容量（SOCK_FD_MAX↔
/// TCP_SESSIONS_MAX 先例）——「distinct target 全注册」恰满帽，Exists
/// 恒先于 Full 到达；两常量独立演化时此断言先于静默越界炸响。
const _: () = assert!(EPOLL_ITEMS_MAX == SOCK_FD_MAX);

/// EPOLL 事件位（Linux UAPI 通用值——**与 poll 位数值同源**（Linux 把
/// poll 位直接复用为 epoll 位），下方 const 对拍钉板零换算执法；域差
/// u32 vs i16 由对拍吸收）。v1 白名单 = 这四位，其余位（EPOLLET/
/// EPOLLPRI/EPOLLRDHUP…）拒 EINVAL——收下不执法是谎报，诚实拒优先。
pub const EPOLLIN: u32 = 0x001;
pub const EPOLLOUT: u32 = 0x004;
pub const EPOLLERR: u32 = 0x008;
pub const EPOLLHUP: u32 = 0x010;
/// interest 掩码白名单（ctl ADD/MOD 执法面）。
pub const EPOLL_MASK_OK: u32 = EPOLLIN | EPOLLOUT | EPOLLERR | EPOLLHUP;

/// EPOLL↔POLL 数值同源钉板（Linux UAPI 事实——epoll 位复用 poll 位；
/// libc wait 快照把 `sock_poll_one` 的 i16 revents 直转 u32 events，
/// 漂移即编译期炸响）。
const _: () = assert!(EPOLLIN as i64 == POLLIN as i64);
const _: () = assert!(EPOLLOUT as i64 == POLLOUT as i64);
const _: () = assert!(EPOLLERR as i64 == POLLERR as i64);
const _: () = assert!(EPOLLHUP as i64 == POLLHUP as i64);

/// epoll_ctl op 域（Linux 定值）。
pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;

/// epoll interest 注册项（值语义 Copy）：目标 socket fd + 订阅掩码
/// （白名单内）+ data 透传槽（epoll_event.data 的回拷源——wait 命中
/// 时原样归还，ptr/fd/u32 形态由消费方重释）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EpollItem {
    pub fd: i32,
    pub events: u32,
    pub data: u64,
}

/// epoll ctl 错误面（errno 整型映射归 libc——SockDomErr 同先例：协议/
/// 表面错误与 errno 是两层）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpollCtlErr {
    /// epfd 不在 epoll 段或空槽 → EBADF。
    BadFd,
    /// op 值非 ADD/DEL/MOD → EINVAL。
    OpBad,
    /// 目标 fd 非 socket 段（console/fs/epoll fd 不入 epoll——v1 白名
    /// 单 = socket 段；POSIX 近似：非法 fd 本应 EBADF/NFS 大同，统一
    /// EINVAL 诚实拒不静默收下）→ EINVAL。
    NotSock,
    /// ADD 撞已注册 fd → EEXIST。
    Exists,
    /// MOD/DEL 目标不在册 → ENOENT。
    Noent,
    /// interest 容量帽（EPOLL_ITEMS_MAX）→ ENOSPC。
    Full,
    /// 订阅掩码白名单外 → EINVAL。
    Mask,
}

/// epoll fd 表（libc 侧静态实例宿主）：`None` 槽 = 空；fd =
/// `EPOLL_FD_BASE + 槽号`；表项 = interest 数组（首空位安装、DEL 挖洞
/// 不压缩——快照方只做值拷贝，顺序稳定可测）。SocketTable 同形态。
pub struct EpollTable {
    slots: [Option<[Option<EpollItem>; EPOLL_ITEMS_MAX]>; EPOLL_FD_MAX],
}

impl EpollTable {
    pub const fn new() -> Self {
        EpollTable {
            slots: [None; EPOLL_FD_MAX],
        }
    }

    /// 最低空位安装 → fd；满表 None（EMFILE 面，libc 收口）。
    pub fn alloc(&mut self) -> Option<i32> {
        for i in 0..EPOLL_FD_MAX {
            if self.slots[i].is_none() {
                self.slots[i] = Some([None; EPOLL_ITEMS_MAX]);
                return Some(EPOLL_FD_BASE + i as i32);
            }
        }
        None
    }

    /// 释槽（close 面）：摘除整个实例。false = 不在册（EBADF 面）。
    pub fn free(&mut self, fd: i32) -> bool {
        match self.index(fd) {
            Some(i) => self.slots[i].take().is_some(),
            None => false,
        }
    }

    /// ctl 三 op（ADD/MOD/DEL）——epfd 域检 → op 域检 → 目标 fd 白名
    /// 单（socket 段 15..=22）→ 掩码白名单 → interest 面操作。检查序
    /// 即 POSIX errno 优先序（BadFd → OpBad → NotSock → Mask →
    /// Exists/Noent/Full）。
    pub fn ctl(
        &mut self,
        fd: i32,
        op: i32,
        target: i32,
        events: u32,
        data: u64,
    ) -> Result<(), EpollCtlErr> {
        let slot_idx = self.index(fd).ok_or(EpollCtlErr::BadFd)? as usize;
        let items = self
            .slots
            .get_mut(slot_idx)
            .ok_or(EpollCtlErr::BadFd)?
            .as_mut()
            .ok_or(EpollCtlErr::BadFd)?;
        if op != EPOLL_CTL_ADD && op != EPOLL_CTL_DEL && op != EPOLL_CTL_MOD {
            return Err(EpollCtlErr::OpBad);
        }
        // 目标 fd 白名单 = socket 段在册区间（表归属查 socket 表是 libc
        // 侧职责——本纯函数只执法号段；「段内但 socket 表空槽」由快照
        // 方的 sock_valid 前置收口，与此处不重叠）。
        if !(SOCK_FD_BASE..SOCK_FD_BASE + SOCK_FD_MAX as i32).contains(&target) {
            return Err(EpollCtlErr::NotSock);
        }
        match op {
            EPOLL_CTL_ADD => {
                if events & !EPOLL_MASK_OK != 0 {
                    return Err(EpollCtlErr::Mask);
                }
                // 撞槽判据 = 同一 target fd 已注册（两实例各注册同 fd 合
                // 法——epoll 语义 per-instance）。
                if items.iter().any(|s| s.is_some_and(|it| it.fd == target)) {
                    return Err(EpollCtlErr::Exists);
                }
                let slot = items
                    .iter_mut()
                    .find(|s| s.is_none())
                    .ok_or(EpollCtlErr::Full)?;
                *slot = Some(EpollItem { fd: target, events, data });
                Ok(())
            }
            EPOLL_CTL_MOD => {
                if events & !EPOLL_MASK_OK != 0 {
                    return Err(EpollCtlErr::Mask);
                }
                // 撞槽判据 = 同一 target fd 已注册（两实例各注册同 fd 合
                // 法——epoll 语义 per-instance）。
                let slot = items
                    .iter_mut()
                    .find(|s| s.is_some_and(|it| it.fd == target))
                    .ok_or(EpollCtlErr::Noent)?;
                let it = slot.as_mut().unwrap();
                it.events = events;
                it.data = data;
                Ok(())
            }
            _ => {
                // EPOLL_CTL_DEL：event 指针 POSIX 忽略（v1 同）。
                let slot = items
                    .iter_mut()
                    .find(|s| s.is_some_and(|it| it.fd == target))
                    .ok_or(EpollCtlErr::Noent)?;
                *slot = None;
                Ok(())
            }
        }
    }

    /// interest 值快照（wait 消费面）：拷贝整个数组——wait 循环持有
    /// 值副本，与表后续变更解耦（单线程下也无借冲突）。None = 不在册。
    pub fn snapshot(
        &self,
        fd: i32,
    ) -> Option<[Option<EpollItem>; EPOLL_ITEMS_MAX]> {
        self.index(fd)
            .and_then(|i| self.slots.get(i as usize))
            .and_then(|s| s.as_ref())
            .copied()
    }

    fn index(&self, fd: i32) -> Option<usize> {
        if fd < EPOLL_FD_BASE {
            return None;
        }
        let i = (fd - EPOLL_FD_BASE) as usize;
        (i < EPOLL_FD_MAX).then_some(i)
    }
}

/// 点分四段 IPv4 字面量解析（getaddrinfo IP 字面量档的地基，草案 §3.3
/// ——musl 语义：字面量自消化不走 DNS）。严格面 = inet_pton 口径：恰四
/// 段、每段 1-3 位 ASCII 十进制、值 0-255；前导零按十进制（"01"=1）。
/// 返回**主机序** u32（a.b.c.d → a<<24|b<<16|c<<8|d；网络序转换 =
/// htonl/htons 族，libc net.rs 消费面）。None = 非字面量（调用方
/// EAI_NONAME 面）。
pub fn parse_ipv4(s: &[u8]) -> Option<u32> {
    let mut octets = [0u8; 4];
    let mut seg = 0usize;
    let mut cur: u16 = 0;
    let mut cur_len = 0usize;
    for &c in s {
        if c == b'.' {
            if seg == 3 || cur_len == 0 || cur > 255 {
                return None;
            }
            octets[seg] = cur as u8;
            seg += 1;
            cur = 0;
            cur_len = 0;
        } else if c.is_ascii_digit() {
            cur_len += 1;
            if cur_len > 3 {
                return None;
            }
            cur = cur * 10 + u16::from(c - b'0');
        } else {
            return None;
        }
    }
    if seg != 3 || cur_len == 0 || cur > 255 {
        return None;
    }
    octets[3] = cur as u8;
    Some(
        (u32::from(octets[0]) << 24)
            | (u32::from(octets[1]) << 16)
            | (u32::from(octets[2]) << 8)
            | u32::from(octets[3]),
    )
}

// —— 游标数学（lseek 面）——

pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

/// 纯游标数学：whence+pos → 新 offset（0..=FILE_SIZE_MAX）。SEEK_END 需
/// 调用方先 STAT 取 size（`end=None` = 未取 → None/EINVAL 面）。越
/// FILE_SIZE_MAX 拒（协议 off 域帽，v1 文档化缺口）；未知 whence → None。
pub fn seek_offset(cur: u32, whence: i32, pos: i64, end: Option<u32>) -> Option<u32> {
    let base = match whence {
        SEEK_SET => 0i64,
        SEEK_CUR => cur as i64,
        SEEK_END => end? as i64,
        _ => return None,
    };
    let target = base.checked_add(pos)?;
    (0..=FILE_SIZE_MAX as i64)
        .contains(&target)
        .then_some(target as u32)
}

// —— cwd 拼接（chdir/getcwd 面，libc 侧字符串状态）——

/// 用户空间路径（`/` 分段 C 字符串，调用方先按 NUL 截长）+ 当前 wire cwd
/// → 输出 wire 路径（NUL 分段，写入 `out`、返回长度）。绝对路径（首字节
/// `/`）替换 cwd；相对路径接在 cwd 后。段规则与 bsh `fs_wire_path` 同源
/// 同值（零漂移）：字符集 `[A-Za-z0-9._-]`、≤NAME_MAX、段数 ≤
/// PATH_DEPTH_MAX、总长 ≤PATH_MAX；空段（`a//b`、尾随 `/`）拒——根目录
/// = 空 wire（`/` 合法、len 0）。None = 坏段/超帽（EINVAL/ENAMETOOLONG
/// 面由调用方映射）。无 `.`/`..` 归一化（v1 缺口，见模块头）。
pub fn resolve_path(cwd: &[u8], user: &[u8], out: &mut [u8; PATH_MAX]) -> Option<usize> {
    if user.is_empty() {
        return None;
    }
    let (absolute, rest) = if user[0] == b'/' {
        (true, &user[1..])
    } else {
        (false, &user[..])
    };
    let mut len = 0usize;
    let mut segs = 0usize;
    if !absolute {
        if cwd.len() > PATH_MAX {
            return None;
        }
        out[..cwd.len()].copy_from_slice(cwd);
        len = cwd.len();
        segs = if cwd.is_empty() {
            0
        } else {
            1 + cwd.iter().filter(|&&c| c == 0).count()
        };
    }
    if rest.is_empty() {
        return Some(len); // 绝对 `/` → 根（空 wire）；相对空串已在入口拒
    }
    for seg in rest.split(|&c| c == b'/') {
        if seg.is_empty()
            || seg.len() > NAME_MAX
            || !seg
                .iter()
                .all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
        {
            return None;
        }
        segs += 1;
        if segs > PATH_DEPTH_MAX {
            return None;
        }
        // 分隔符显式写零：out 是调用方复用缓冲（libc 侧静态），残留字节
        // 不是分隔符——新鲜零缓冲掩盖过这个坑（脏缓冲回归测试钉死）。
        let need = seg.len() + usize::from(len > 0);
        if len + need > PATH_MAX {
            return None;
        }
        if len > 0 {
            out[len] = 0;
            len += 1;
        }
        out[len..len + seg.len()].copy_from_slice(seg);
        len += seg.len();
    }
    Some(len)
}

/// wire cwd → 用户空间渲染（getcwd 面）：根（空 wire）→ `"/"`；否则
/// `"/seg/seg"`（NUL 换 `/` + 前导 `/`，恰 wire.len()+1 字节——out 不足
/// = None/ERANGE 面）。
pub fn render_cwd(wire: &[u8], out: &mut [u8]) -> Option<usize> {
    if wire.is_empty() {
        if out.is_empty() {
            return None;
        }
        out[0] = b'/';
        return Some(1);
    }
    if out.len() < wire.len() + 1 {
        return None;
    }
    out[0] = b'/';
    for (i, &c) in wire.iter().enumerate() {
        out[i + 1] = if c == 0 { b'/' } else { c };
    }
    Some(wire.len() + 1)
}

// —— stat 整形 ——

pub const S_IFREG: u32 = 0o100000;
pub const S_IFDIR: u32 = 0o040000;

/// STAT 应答 ty → `st_mode`（v1 定死：文件 0644 / 目录 0755——fsmgr 无
/// 权限位，投影面）。未知 ty = None（协议破缺，EIO 面）。
pub fn st_mode_of(ty: u8) -> Option<u32> {
    match ty {
        TYPE_FILE => Some(S_IFREG | 0o644),
        TYPE_DIR => Some(S_IFDIR | 0o755),
        _ => None,
    }
}

// —— dirent 解码（readdir 面）——

/// READDIR 子名 blob（NUL 连接）→ 逐名迭代：`pos` 为游标（调用方 DIR 句柄
/// 持有），返回名切片或 None（尾）。容忍首/尾/连续 NUL（bsh split 同面）；
/// 名内容不校验（服务端已按 NAME_MAX 产名——解码是数据面不是协议面）。
pub fn dirent_next<'a>(blob: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    loop {
        if *pos >= blob.len() {
            return None;
        }
        let end = match blob[*pos..].iter().position(|&c| c == 0) {
            Some(i) => *pos + i,
            None => blob.len(),
        };
        let name = &blob[*pos..end];
        *pos = if end < blob.len() { end + 1 } else { end };
        if name.is_empty() {
            continue;
        }
        return Some(name);
    }
}

// —— A3-2：netd RPC 语义纯函数面（草案 §3.2 协议映射的 host 钉板件）——
// ST 码/会话容量/POLL 旗自 berry-netdproto 单源引（「不长第二套」）；
// POSIX 侧常量（errno/poll 位）在本 crate 立值、libc net.rs const assert
// 对拍（O_NONBLOCK 单源纪律的镜像面——两侧任一漂移编译期炸响）。

/// 容量同源钉板（草案 §3.1「socket 表容量与 TCP_SESSIONS_MAX 同源单一
/// 定值」的编译期执法）：socket 表槽数 ≠ netd 会话表容量 = fd 段与 sid
/// 段错位（alloc 满表先于 netd FULL 到达 = 结构性 EMFILE 谎报）。
const _: () = assert!(SOCK_FD_MAX == berry_netdproto::TCP_SESSIONS_MAX as usize);

/// POSIX poll 位（Linux 通用值；events/revents 都是 i16 位域）。
pub const POLLIN: i16 = 0x001;
pub const POLLPRI: i16 = 0x002;
pub const POLLOUT: i16 = 0x004;
pub const POLLERR: i16 = 0x008;
pub const POLLHUP: i16 = 0x010;
pub const POLLNVAL: i16 = 0x020;

/// netd POLL 快照（flags + rbytes）→ POSIX revents。掩码执法 POSIX 口径
/// 「revents ⊆ events ∪ {POLLERR, POLLHUP, POLLNVAL}」（恒报告位不受
/// 订阅约束）；CLOSED（对端收口）= POLLIN|POLLHUP——可读（余量收割/EOF）
/// 与挂断同报（Linux 对端关闭套接字的 poll 真值形态）。
pub fn poll_revents(netd_flags: u8, rbytes: u16, events: i16) -> i16 {
    let mut r: i16 = 0;
    if rbytes > 0 {
        r |= POLLIN;
    }
    if netd_flags & berry_netdproto::POLL_FLAG_WRITABLE != 0 {
        r |= POLLOUT;
    }
    if netd_flags & berry_netdproto::POLL_FLAG_CLOSED != 0 {
        r |= POLLIN | POLLHUP;
    }
    r & (events | POLLERR | POLLHUP | POLLNVAL)
}

/// netd ST 码 → errno 的 op 参数化域：ST_BAD/ST_NOSESSION 的 POSIX 语义
/// 随 op 分岔（listen 的 BAD = 端口已被 LISTEN → EADDRINUSE；send 的
/// BAD = 死会话/握手中 → EPIPE）——单一平表不成立（草案 §3.2 错码映射
/// 表逐行）。ST_OK → 0；未知码诚实 EIO（协议演化防御——新 ST 码不该被
/// 静默当成功）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SockOp {
    Connect,
    Listen,
    Accept,
    Send,
    Recv,
    Close,
}

// errno 数值面（Linux/musl 同值、ABI 冻结；libc lib.rs 同名常量的
// 对拍锚点——见 libc net.rs const assert）。
pub const EIO: i32 = 5;
pub const EAGAIN: i32 = 11;
pub const EINVAL: i32 = 22;
pub const EPIPE: i32 = 32;
pub const ENODEV: i32 = 19;
pub const EADDRINUSE: i32 = 98;
pub const ETIMEDOUT: i32 = 110;

pub fn sock_rpc_errno(op: SockOp, st: u8) -> i32 {
    use berry_netdproto as ndp;
    match st {
        ndp::ST_OK => 0,
        // accept 空队原生档（非阻塞快返语义）；他 op 不产此码
        ndp::ST_EAGAIN => EAGAIN,
        // 无 NIC 拓扑（vnet deviceless serve 的确定性信号）
        ndp::ST_NODEV => ENODEV,
        ndp::ST_TIMEOUT => ETIMEDOUT,
        // netd 会话表满：端口占用近似档（草案 §3.2）
        ndp::ST_FULL => EADDRINUSE,
        ndp::ST_NOSESSION => match op {
            // listen 会话已死 = 「非监听套接字」POSIX 档
            SockOp::Accept => EINVAL,
            _ => EPIPE,
        },
        ndp::ST_BAD => match op {
            // vnet ListenErr::Dup（端口已被 LISTEN）映射面
            SockOp::Listen => EADDRINUSE,
            // 死 sid / 握手中 / 已关（table.send_data None 三态）
            SockOp::Send => EPIPE,
            _ => EIO,
        },
        // session-0 独占冲突（connect 二连——他进程已持 legacy 客户端）
        ndp::ST_BUSY => EIO,
        _ => EIO,
    }
}

// —— DNS 名字预检 + 应答错码映射（A4-2；libc getaddrinfo DNS 档地基）——

/// EAI_* 数值面（getaddrinfo 直接返回码，非 errno；Linux/musl 同值——
/// libc lib.rs 同名常量的对拍锚点，errno 区同口径）。
pub const EAI_NONAME: i32 = -2;
/// DNS 查询超时档（ST_TIMEOUT 映射；musl 同值）。
pub const EAI_AGAIN: i32 = -3;

/// DNS 查询名合法性预检（libc 快路 miss 后、**发包前**的纯本地执法）：
/// 非空、len ≤ netdproto [`NAME_MAX`](berry_netdproto::NAME_MAX)（单源
/// 引用——协议帽即预检帽）、字符集 [A-Za-z0-9.-]（LDH 域减岗 + 点分
/// 分隔；v1 无下划线/IDN/尾部点特赦——诚实拒优于收名不执法）。非法/
/// 超长 → false（调用方 EAI_NONAME 本地拒，零 RPC）。
pub fn dns_name_ok(name: &[u8]) -> bool {
    use berry_netdproto as ndp;
    !name.is_empty()
        && name.len() <= ndp::NAME_MAX
        && name.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
}

/// netdproto DNS 应答 st → EAI 码（sock_rpc_errno 全表先例——「映射臂
/// 先宣后现」执法：netdproto 应答域 {ST_OK, ST_BAD, ST_TIMEOUT} 三值
/// 全接线，不按「预期跑不到」裁剪）。域外值经 parse_reply 烘焙线
/// （未知 st → None）结构性不可达——防御面同 EAI_NONAME。
pub fn dns_rpc_eai(st: u8) -> i32 {
    use berry_netdproto as ndp;
    match st {
        ndp::ST_OK => 0,
        // DNS 往返帽到（服务端 DnsRes::Timeout 真实在册可达——域内码，
        // 非结构性缺席）；「稍后重试可解」语义
        ndp::ST_TIMEOUT => EAI_AGAIN,
        // 名字不存在/坏请求（服务端 DnsRes::Bad / parse 拒）
        ndp::ST_BAD => EAI_NONAME,
        _ => EAI_NONAME,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— FdTable 状态机 ——
    fn entry(wire: &[u8], readable: bool, writable: bool) -> FdEntry {
        FdEntry::new(wire, readable, writable).unwrap()
    }

    #[test]
    fn fd_alloc_lowest_first() {
        let mut t = FdTable::new();
        assert_eq!(t.alloc(entry(b"A.TXT", true, false)), Some(3));
        assert_eq!(t.alloc(entry(b"B.TXT", true, false)), Some(4));
        assert_eq!(t.alloc(entry(b"C.TXT", false, true)), Some(5));
    }

    #[test]
    fn fd_close_reuses_lowest_free() {
        let mut t = FdTable::new();
        assert_eq!(t.alloc(entry(b"A", true, false)), Some(3));
        assert_eq!(t.alloc(entry(b"B", true, false)), Some(4));
        assert_eq!(t.alloc(entry(b"C", true, false)), Some(5));
        assert!(t.take(4).is_some());
        // POSIX 最低空位：4 先于 6 回填。
        assert_eq!(t.alloc(entry(b"D", true, false)), Some(4));
    }

    #[test]
    fn fd_alloc_full_is_emfile_face() {
        let mut t = FdTable::new();
        for i in 0..FD_MAX {
            assert_eq!(t.alloc(entry(b"x", true, false)), Some(FD_BASE + i as i32));
        }
        assert_eq!(t.alloc(entry(b"x", true, false)), None);
    }

    #[test]
    fn fd_take_bad_fd_is_none() {
        let mut t = FdTable::new();
        for fd in [-1, 0, 1, 2, 3, FD_BASE + FD_MAX as i32, 99] {
            assert_eq!(t.take(fd), None, "fd={}", fd);
        }
    }

    #[test]
    fn fd_entry_roundtrip_and_advance() {
        let mut t = FdTable::new();
        let fd = t.alloc(entry(b"d1\0Z.TXT", false, true)).unwrap();
        {
            let e = t.get_mut(fd).unwrap();
            assert_eq!(e.wire(), b"d1\0Z.TXT");
            assert!(!e.readable && e.writable);
            e.advance(192);
            e.advance(100);
        }
        assert_eq!(t.get(fd).unwrap().offset, 292);
        // close 即 commit 的依据：take 归还的表项保游标与写标志。
        let closed = t.take(fd).unwrap();
        assert_eq!(closed.offset, 292);
        assert!(closed.writable);
        assert!(t.take(fd).is_none()); // 二次 close = EBADF 面
    }

    #[test]
    fn fd_entry_new_caps_and_saturating_advance() {
        assert!(FdEntry::new(&[b'a'; PATH_MAX], true, false).is_some());
        assert!(FdEntry::new(&[b'a'; PATH_MAX + 1], true, false).is_none());
        let mut e = entry(b"f", true, false);
        e.offset = u32::MAX - 1;
        e.advance(10);
        assert_eq!(e.offset, u32::MAX); // 饱和不回卷
    }

    // —— AliasTable（console fd 克隆面，C-2c 挂账刀）——

    #[test]
    fn alias_alloc_lowest_first_and_start_floor() {
        let mut a = AliasTable::new();
        let t = FdTable::new();
        // 空 FDT：自 FD_BASE 最低位起（F_DUPFD 的 start ≤3 抬到 3）。
        assert_eq!(a.alloc(0, 0, &t), Some(3));
        assert_eq!(a.alloc(1, 0, &t), Some(4));
        // start 抬位：显式要 ≥5 → 跳过已用 3/4 落 5。
        assert_eq!(a.alloc(2, 5, &t), Some(5));
        assert_eq!(a.base_of(3), Some(0));
        assert_eq!(a.base_of(4), Some(1));
        assert_eq!(a.base_of(5), Some(2));
        // 坏 base（非 0/1/2）= None（防御面）。
        assert_eq!(a.alloc(3, 3, &t), None);
    }

    #[test]
    fn alias_alloc_skips_fdt_and_alias_occupied() {
        let mut a = AliasTable::new();
        let mut t = FdTable::new();
        assert_eq!(t.alloc(entry(b"A", true, false)), Some(3));
        assert_eq!(t.alloc(entry(b"B", true, false)), Some(4));
        // FDT 占 3/4：别名从 5 起（跳 FDT 在册位）。
        assert_eq!(a.alloc(0, 0, &t), Some(5));
        // 已用别名位 5 也跳：第二条落 6。
        assert_eq!(a.alloc(1, 0, &t), Some(6));
        // take 摘 5 后可回填（close 复用面）。
        assert_eq!(a.take(5), Some(0));
        assert_eq!(a.take(5), None); // 二次 close = 非别名
        assert_eq!(a.alloc(2, 0, &t), Some(5));
    }

    #[test]
    fn alias_alloc_full_is_emfile_face() {
        let mut a = AliasTable::new();
        let t = FdTable::new();
        for i in 0..ALIAS_MAX {
            assert_eq!(a.alloc(0, 0, &t), Some(FD_BASE + i as i32));
        }
        // 槽满（EMFILE）——空号仍在也不给。
        assert_eq!(a.alloc(0, 0, &t), None);
    }

    #[test]
    fn alias_alloc_crosses_above_ten_when_fdt_full() {
        let mut a = AliasTable::new();
        let mut t = FdTable::new();
        for i in 0..FD_MAX {
            assert_eq!(t.alloc(entry(b"x", true, false)), Some(FD_BASE + i as i32));
        }
        assert_eq!(t.alloc(entry(b"x", true, false)), None); // FDT 满
        // FDT 满（3..=10 全占）：别名越 10 上探——11/12/13/14 恰 ALIAS_MAX
        // 枚，段帽 15 排他。
        for i in 0..ALIAS_MAX {
            assert_eq!(a.alloc(1, 3, &t), Some(11 + i as i32));
        }
        assert_eq!(a.alloc(1, 3, &t), None); // 段帽尽 = EMFILE
        assert_eq!(a.base_of(14), Some(1));
    }

    // —— FdTable::alloc_skipping（共段反向碰撞防线）——

    #[test]
    fn fd_alloc_skipping_avoids_occupied_numbers() {
        // 别名占 3（FDT 槽 0 空着也不给同号）→ 落 4；占 3/4 → 落 5；
        // 全段占 → None（EMFILE——不给双语义同号）。
        let mut t2 = FdTable::new();
        assert_eq!(t2.alloc_skipping(entry(b"A", true, false), |fd| fd == 3), Some(4));
        let mut t3 = FdTable::new();
        assert_eq!(
            t3.alloc_skipping(entry(b"A", true, false), |fd| fd == 3 || fd == 4),
            Some(5)
        );
        let mut t4 = FdTable::new();
        assert_eq!(
            t4.alloc_skipping(entry(b"A", true, false), |fd| (3..=10).contains(&fd)),
            None
        );
        // 空占用谓词 = 与裸 alloc 同值（最低位 3）。
        let mut t5 = FdTable::new();
        assert_eq!(t5.alloc_skipping(entry(b"A", true, false), |_| false), Some(3));
    }

    // —— resolve_path（cwd 拼接）——
    fn r(cwd: &[u8], user: &[u8]) -> Option<Vec<u8>> {
        let mut out = [0u8; PATH_MAX];
        resolve_path(cwd, user, &mut out).map(|l| out[..l].to_vec())
    }

    #[test]
    fn resolve_root_cwd_relative() {
        assert_eq!(r(b"", b"A.TXT").unwrap(), b"A.TXT");
        assert_eq!(r(b"d1", b"Z.TXT").unwrap(), b"d1\0Z.TXT");
        assert_eq!(r(b"d1\0d2", b"Z.TXT").unwrap(), b"d1\0d2\0Z.TXT");
    }

    #[test]
    fn resolve_absolute_replaces_cwd() {
        assert_eq!(r(b"d1", b"/d2/X").unwrap(), b"d2\0X");
        assert_eq!(r(b"d1\0d2", b"/A").unwrap(), b"A");
    }

    #[test]
    fn resolve_root_is_empty_wire() {
        assert_eq!(r(b"", b"/").unwrap(), b"");
        assert_eq!(r(b"d1\0d2", b"/").unwrap(), b"");
        assert_eq!(r(b"", b"/a").unwrap(), b"a");
    }

    #[test]
    fn resolve_rejects_bad_paths() {
        assert_eq!(r(b"", b""), None); // 空路径
        assert_eq!(r(b"", b"a//b"), None); // 空段
        assert_eq!(r(b"", b"a/"), None); // 尾随 /
        assert_eq!(r(b"", b"/a/"), None);
        assert_eq!(r(b"", b"A B"), None); // 空白（未引号 token 面）
        assert_eq!(r(b"", b"A$B"), None); // 展开字符
        assert_eq!(r(b"", b"A\0B"), None); // 内嵌 NUL（charset 拒）
    }

    #[test]
    fn resolve_caps() {
        let name48 = [b'a'; NAME_MAX];
        assert_eq!(r(b"", &name48).unwrap(), &name48[..]); // 恰 NAME_MAX 过
        let name49 = [b'a'; NAME_MAX + 1];
        assert_eq!(r(b"", &name49), None);
        // 段数帽：8 段过、9 段拒。
        let d8 = b"a/b/c/d/e/f/g/h";
        assert!(r(b"", d8).is_some());
        let d9 = b"a/b/c/d/e/f/g/h/i";
        assert_eq!(r(b"", d9), None);
        // 总长帽：2×48+1 = 97 过、3×48+2 = 146 > PATH_MAX 拒。
        let two = [name48.as_slice(), b"/".as_slice(), name48.as_slice()].concat();
        assert!(r(b"", &two).is_some());
        let three = [
            name48.as_slice(),
            b"/".as_slice(),
            name48.as_slice(),
            b"/".as_slice(),
            name48.as_slice(),
        ]
        .concat();
        assert_eq!(r(b"", &three), None);
    }

    #[test]
    fn resolve_writes_separator_on_dirty_buffer() {
        // libc 侧 out 是复用静态缓冲：分隔符必须显式写零，不靠残留。
        let mut out = [0xAAu8; PATH_MAX];
        let l = resolve_path(b"d1", b"Z.TXT", &mut out).unwrap();
        assert_eq!(&out[..l], b"d1\0Z.TXT");
        let mut out = [0xAAu8; PATH_MAX];
        let l = resolve_path(b"d1\0d2", b"X", &mut out).unwrap();
        assert_eq!(&out[..l], b"d1\0d2\0X");
        // cwd 恰满 PATH_MAX 时相对拼接在长度帽拒绝（无越界写）。
        let full = [b'a'; PATH_MAX];
        let mut out = [0u8; PATH_MAX];
        assert_eq!(resolve_path(&full, b"b", &mut out), None);
    }

    // —— render_cwd（getcwd 面）——
    #[test]
    fn render_cwd_shapes() {
        let mut out = [0u8; PATH_MAX];
        assert_eq!(render_cwd(b"", &mut out).unwrap(), 1);
        assert_eq!(&out[..1], b"/");
        assert_eq!(render_cwd(b"d1", &mut out).unwrap(), 3);
        assert_eq!(&out[..3], b"/d1");
        assert_eq!(render_cwd(b"d1\0d2", &mut out).unwrap(), 6);
        assert_eq!(&out[..6], b"/d1/d2");
    }

    #[test]
    fn render_cwd_undersized_is_none() {
        let mut tiny = [0u8; 1];
        assert_eq!(render_cwd(b"d1", &mut tiny), None); // 需 wire.len()+1
        let mut zero = [0u8; 0];
        assert_eq!(render_cwd(b"", &mut zero), None);
    }

    // —— seek_offset（lseek 面）——
    #[test]
    fn seek_set_faces() {
        assert_eq!(seek_offset(50, SEEK_SET, 0, None), Some(0));
        assert_eq!(seek_offset(50, SEEK_SET, -1, None), None);
        assert_eq!(
            seek_offset(0, SEEK_SET, FILE_SIZE_MAX as i64, None),
            Some(FILE_SIZE_MAX as u32)
        );
        assert_eq!(
            seek_offset(0, SEEK_SET, FILE_SIZE_MAX as i64 + 1, None),
            None
        );
    }

    #[test]
    fn seek_cur_faces() {
        assert_eq!(seek_offset(100, SEEK_CUR, 28, None), Some(128));
        assert_eq!(seek_offset(100, SEEK_CUR, -100, None), Some(0));
        assert_eq!(seek_offset(100, SEEK_CUR, -101, None), None); // 下溢
    }

    #[test]
    fn seek_end_faces() {
        assert_eq!(seek_offset(0, SEEK_END, -4, Some(10)), Some(6));
        assert_eq!(seek_offset(0, SEEK_END, 1, Some(10)), Some(11));
        assert_eq!(seek_offset(0, SEEK_END, 0, None), None); // 未取 size = EINVAL 面
        assert_eq!(seek_offset(0, 3, 0, Some(10)), None); // 未知 whence
    }

    // —— stat 整形 ——
    #[test]
    fn st_mode_shapes() {
        assert_eq!(st_mode_of(TYPE_FILE), Some(S_IFREG | 0o644));
        assert_eq!(st_mode_of(TYPE_DIR), Some(S_IFDIR | 0o755));
        assert_eq!(st_mode_of(2), None);
        assert_eq!(st_mode_of(0xff), None);
    }

    // —— dirent 解码 ——
    #[test]
    fn dirent_empty_blob() {
        let blob = b"";
        let mut pos = 0usize;
        assert!(dirent_next(blob, &mut pos).is_none());
    }

    #[test]
    fn dirent_iteration() {
        let blob = b"a\0b\0";
        let mut pos = 0usize;
        assert_eq!(dirent_next(blob, &mut pos), Some(&b"a"[..]));
        assert_eq!(dirent_next(blob, &mut pos), Some(&b"b"[..]));
        assert!(dirent_next(blob, &mut pos).is_none());
        // 无尾随 NUL（末名直抵 blob 尾）同形。
        let blob = b"a\0b";
        let mut pos = 0usize;
        assert_eq!(dirent_next(blob, &mut pos), Some(&b"a"[..]));
        assert_eq!(dirent_next(blob, &mut pos), Some(&b"b"[..]));
        assert!(dirent_next(blob, &mut pos).is_none());
    }

    #[test]
    fn dirent_skips_empty_names() {
        let blob = b"\0a\0\0b\0";
        let mut pos = 0usize;
        assert_eq!(dirent_next(blob, &mut pos), Some(&b"a"[..]));
        assert_eq!(dirent_next(blob, &mut pos), Some(&b"b"[..]));
        assert!(dirent_next(blob, &mut pos).is_none());
    }

    // —— ST→FsErr 映射 ——
    #[test]
    fn st_mapping_in_book() {
        assert_eq!(st_to_err(ST_NOENT), FsErr::Noent);
        assert_eq!(st_to_err(ST_FULL), FsErr::Full);
        assert_eq!(st_to_err(ST_BAD), FsErr::Bad);
        assert_eq!(st_to_err(ST_EXISTS), FsErr::Exists);
        assert_eq!(st_to_err(ST_NOEMPTY), FsErr::NotEmpty);
    }

    #[test]
    fn st_mapping_unknown_is_rpc() {
        assert_eq!(st_to_err(0), FsErr::Rpc); // ST_OK 不走此面（调用方先检）
        assert_eq!(st_to_err(6), FsErr::Rpc);
        assert_eq!(st_to_err(0xff), FsErr::Rpc);
    }

    // —— SocketTable + 域执法 + parse_ipv4（A3-1）——

    fn sock() -> SockEntry {
        SockEntry {
            kind: SockKind::Unconnected,
            port: 0,
            sid: 0,
            nonblock: false,
        }
    }

    #[test]
    fn sock_segment_immune_to_alias_span() {
        // 模块级 const 断言的运行期镜像（数值关系可读钉板）：socket 段
        // 基 ≥ fs/别名段扫描上界（AliasTable::alloc 的 hi / fcntl F_DUPFD
        // ceiling 同源）；段形 = 15..=22 排他上界 23。
        assert!(SOCK_FD_BASE >= FD_BASE + (FD_MAX + ALIAS_MAX) as i32);
        assert_eq!(SOCK_FD_BASE, 15);
        assert_eq!(SOCK_FD_BASE + SOCK_FD_MAX as i32, 23);
    }

    #[test]
    fn sock_domain_gate() {
        // proto 0（默认）与 IPPROTO_TCP 双过档。
        assert_eq!(sock_domain_err(AF_INET, SOCK_STREAM, 0), Ok(()));
        assert_eq!(sock_domain_err(AF_INET, SOCK_STREAM, IPPROTO_TCP), Ok(()));
        // AF_INET6(10)/AF_UNIX(1) → Af；SOCK_DGRAM(2) → Type；
        // IPPROTO_UDP(17) → Proto（执法序：af 先于 ty 先于 proto）。
        assert_eq!(sock_domain_err(10, SOCK_STREAM, 0), Err(SockDomErr::Af));
        assert_eq!(sock_domain_err(1, SOCK_STREAM, 0), Err(SockDomErr::Af));
        assert_eq!(sock_domain_err(AF_INET, 2, 0), Err(SockDomErr::Type));
        assert_eq!(
            sock_domain_err(AF_INET, SOCK_STREAM, 17),
            Err(SockDomErr::Proto)
        );
        // af+ty 双错：af 先报。
        assert_eq!(sock_domain_err(1, 2, 17), Err(SockDomErr::Af));
    }

    #[test]
    fn sock_alloc_lowest_first_and_reuse() {
        let mut t = SocketTable::new();
        assert_eq!(t.alloc(sock()), Some(15));
        assert_eq!(t.alloc(sock()), Some(16));
        assert!(t.take(15).is_some());
        // POSIX 最低空位：15 先于 17 回填。
        assert_eq!(t.alloc(sock()), Some(15));
    }

    #[test]
    fn sock_alloc_full_is_emfile_face() {
        let mut t = SocketTable::new();
        for i in 0..SOCK_FD_MAX {
            assert_eq!(t.alloc(sock()), Some(SOCK_FD_BASE + i as i32));
        }
        assert_eq!(t.alloc(sock()), None);
    }

    #[test]
    fn sock_index_segment_boundaries() {
        let mut t = SocketTable::new();
        assert_eq!(t.alloc(sock()), Some(15));
        // 14 = 别名段顶（socket 段前一）不在册；15 在册；22 空槽（在段
        // 但 None）；23 越段。
        for fd in [14, 0, 3, 10, 11, 22, 23, -1, 99] {
            assert_eq!(t.get(fd), None, "fd={}", fd);
        }
        assert!(t.get(15).is_some());
        assert_eq!(t.take(14), None);
        assert_eq!(t.take(23), None);
    }

    #[test]
    fn sock_lifecycle_state_transitions() {
        // 生命周期模拟：socket(Unconnected,sid=0) → connect → Client(sid=3)
        // → nonblock 翻转 → close take 归还。Listen 形态另验。
        let mut t = SocketTable::new();
        let fd = t.alloc(sock()).unwrap();
        {
            let e = t.get_mut(fd).unwrap();
            assert_eq!((e.kind, e.sid), (SockKind::Unconnected, 0));
            e.kind = SockKind::Client;
            e.sid = 3;
            e.nonblock = true;
        }
        let e = t.get(fd).unwrap();
        assert_eq!(
            (*e, e.port),
            (
                SockEntry {
                    kind: SockKind::Client,
                    port: 0,
                    sid: 3,
                    nonblock: true,
                },
                0
            )
        );
        let closed = t.take(fd).unwrap();
        assert_eq!((closed.kind, closed.sid), (SockKind::Client, 3));
        assert!(closed.nonblock);
    }

    #[test]
    fn sock_listen_entry_shape() {
        let mut t = SocketTable::new();
        let fd = t
            .alloc(SockEntry {
                kind: SockKind::Listen,
                port: 18082,
                sid: 1,
                nonblock: false,
            })
            .unwrap();
        assert_eq!(
            t.get(fd),
            Some(&SockEntry {
                kind: SockKind::Listen,
                port: 18082,
                sid: 1,
                nonblock: false,
            })
        );
    }

    #[test]
    fn sock_take_twice_is_none() {
        let mut t = SocketTable::new();
        let fd = t.alloc(sock()).unwrap();
        assert!(t.take(fd).is_some());
        assert_eq!(t.take(fd), None); // 二次 close = EBADF 面
    }

    #[test]
    fn parse_ipv4_canonical() {
        assert_eq!(parse_ipv4(b"127.0.0.1"), Some(0x7f00_0001));
        assert_eq!(parse_ipv4(b"0.0.0.0"), Some(0));
        assert_eq!(parse_ipv4(b"255.255.255.255"), Some(0xffff_ffff));
        assert_eq!(parse_ipv4(b"255.0.255.0"), Some(0xff00_ff00));
        assert_eq!(parse_ipv4(b"10.0.2.2"), Some(0x0a00_0202)); // SLIRP 网关
        assert_eq!(parse_ipv4(b"192.168.1.1"), Some(0xc0a8_0101));
    }

    #[test]
    fn parse_ipv4_leading_zeros_decimal() {
        // inet_pton/strtoul 十进制口径："01"=1 非八进制。
        assert_eq!(parse_ipv4(b"01.2.3.04"), Some(0x0102_0304));
        assert_eq!(parse_ipv4(b"00.0.0.0"), Some(0));
    }

    #[test]
    fn parse_ipv4_rejects_malformed() {
        for s in [
            &b""[..],
            b"1.2.3",
            b"1.2.3.4.5",
            b"1..2.3",
            b"1.2.3.",
            b".1.2.3",
            b".",
            b"256.1.1.1",
            b"1.2.3.999",
            b"1234.1.1.1",
            b"1.2.3.a",
            b"0x1.2.3.4",
            b"1.2.3.4\n",
            b" 1.2.3.4",
            b"1.2.3.4.",
        ] {
            assert_eq!(parse_ipv4(s), None, "s={:?}", core::str::from_utf8(s));
        }
    }

    // —— A3-2：poll_revents / sock_rpc_errno 钉板 ——

    use berry_netdproto as ndp;

    #[test]
    fn poll_revents_data_and_writable() {
        // rbytes>0 → POLLIN（订阅才报）；WRITABLE → POLLOUT。
        assert_eq!(poll_revents(0, 64, POLLIN), POLLIN);
        assert_eq!(poll_revents(ndp::POLL_FLAG_WRITABLE, 0, POLLIN), 0);
        assert_eq!(
            poll_revents(ndp::POLL_FLAG_WRITABLE, 0, POLLIN | POLLOUT),
            POLLOUT
        );
        assert_eq!(
            poll_revents(ndp::POLL_FLAG_WRITABLE, 64, POLLIN | POLLOUT),
            POLLIN | POLLOUT
        );
    }

    #[test]
    fn poll_revents_closed_and_mask() {
        // CLOSED = POLLIN|POLLHUP；HUP 恒报（未订阅 POLLIN 时只余 HUP）。
        assert_eq!(
            poll_revents(ndp::POLL_FLAG_CLOSED, 0, POLLIN),
            POLLIN | POLLHUP
        );
        assert_eq!(poll_revents(ndp::POLL_FLAG_CLOSED, 0, 0), POLLHUP);
        // 空订阅不掩 ERR/HUP/NVAL 恒报告位。
        assert_eq!(poll_revents(0, 64, 0), 0);
        // 空闲会话（零 flags 零 rbytes）= 无就绪。
        assert_eq!(poll_revents(0, 0, POLLIN | POLLOUT), 0);
    }

    #[test]
    fn sock_rpc_errno_table() {
        // 通用档：OK/EAGAIN/NODEV/TIMEOUT/FULL 全 op 一致。
        for op in [SockOp::Connect, SockOp::Listen, SockOp::Accept, SockOp::Send, SockOp::Recv, SockOp::Close] {
            assert_eq!(sock_rpc_errno(op, ndp::ST_OK), 0);
            assert_eq!(sock_rpc_errno(op, ndp::ST_EAGAIN), EAGAIN);
            assert_eq!(sock_rpc_errno(op, ndp::ST_NODEV), ENODEV);
            assert_eq!(sock_rpc_errno(op, ndp::ST_TIMEOUT), ETIMEDOUT);
            assert_eq!(sock_rpc_errno(op, ndp::ST_FULL), EADDRINUSE);
            assert_eq!(sock_rpc_errno(op, ndp::ST_BUSY), EIO);
            assert_eq!(sock_rpc_errno(op, 0x7F), EIO, "未知码诚实 EIO");
        }
        // 分岔档：NOSESSION（accept=EINVAL 其余 EPIPE）与 BAD
        //（listen=EADDRINUSE、send=EPIPE、其余 EIO）。
        for (op, nosess, bad) in [
            (SockOp::Connect, EPIPE, EIO),
            (SockOp::Listen, EPIPE, EADDRINUSE),
            (SockOp::Accept, EINVAL, EIO),
            (SockOp::Send, EPIPE, EPIPE),
            (SockOp::Recv, EPIPE, EIO),
            (SockOp::Close, EPIPE, EIO),
        ] {
            assert_eq!(sock_rpc_errno(op, ndp::ST_NOSESSION), nosess, "{:?}", op);
            assert_eq!(sock_rpc_errno(op, ndp::ST_BAD), bad, "{:?}", op);
        }
    }

    #[test]
    fn sock_capacity_matches_netd() {
        // const 断言的运行时镜像（失败信息更可读）。
        assert_eq!(SOCK_FD_MAX, ndp::TCP_SESSIONS_MAX as usize);
    }

    // —— DNS 名字预检 + 错码映射（A4-2）——

    #[test]
    fn dns_name_ok_boundary() {
        // 合法形：LDH 域 + 点分（大小写均收——DNS 大小写不敏感）。
        assert!(dns_name_ok(b"localhost"));
        assert!(dns_name_ok(b"a.b.c.d.example.com"));
        assert!(dns_name_ok(b"LocalHost"));
        assert!(dns_name_ok(b"x-y.z-w"));
        assert!(dns_name_ok(b"a")); // 单字符——字符集面合法（语义留给服务端）
        // 空名 = 拒（encode_request 同面——预检先于 RPC 零发包）。
        assert!(!dns_name_ok(b""));
        // 越帽 = 恒拒（协议 NAME_MAX=64 单源——65B 全 'a' 恒拒）。
        assert!(!dns_name_ok(&[b'a'; ndp::NAME_MAX + 1]));
        assert!(dns_name_ok(&[b'a'; ndp::NAME_MAX])); // 恰帽收
        // 非法字符：下划线（v1 无 _ 特赦）、空格、斜杠、冒号、非 ASCII。
        assert!(!dns_name_ok(b"my_host"));
        assert!(!dns_name_ok(b"a b"));
        assert!(!dns_name_ok(b"a/b"));
        assert!(!dns_name_ok(b"a:b"));
        assert!(!dns_name_ok(b"a\x80b"));
        // 边界字符逐一切面：'-' 与 '.' 合法、'.' 首尾字符集面不拒
        //（「.」/「a..b」语义合法性归服务端/解析器——预检只管字符集与长度）。
        assert!(dns_name_ok(b"-"));
        assert!(dns_name_ok(b"."));
    }

    #[test]
    fn dns_rpc_eai_table() {
        // netdproto DNS 应答域全表（「映射臂先宣后现」——三值全接线）。
        assert_eq!(dns_rpc_eai(ndp::ST_OK), 0);
        assert_eq!(dns_rpc_eai(ndp::ST_TIMEOUT), EAI_AGAIN);
        assert_eq!(dns_rpc_eai(ndp::ST_BAD), EAI_NONAME);
        // 域外防御面（经 parse_reply 烘焙线结构性不可达——同值拒绝）。
        assert_eq!(dns_rpc_eai(0x7F), EAI_NONAME);
        assert_eq!(dns_rpc_eai(ndp::ST_BUSY), EAI_NONAME, "DNS 面无会话语义");
        // 数值面锚（musl 同值：EAI_NONAME=-2 / EAI_AGAIN=-3）。
        assert_eq!(EAI_NONAME, -2);
        assert_eq!(EAI_AGAIN, -3);
    }

    // —— epoll 表（A4-1）——

    /// ctl 桩：ADD 合法面（EPOLLIN|EPOLLOUT，socket 段内 target）。
    fn ectl_add(
        t: &mut EpollTable,
        epfd: i32,
        target: i32,
        events: u32,
    ) -> Result<(), EpollCtlErr> {
        t.ctl(epfd, EPOLL_CTL_ADD, target, events, target as u64)
    }

    #[test]
    fn epoll_alloc_lowest_first_and_reuse() {
        let mut t = EpollTable::new();
        assert_eq!(t.alloc(), Some(23));
        assert_eq!(t.alloc(), Some(24));
        assert!(t.free(23));
        // POSIX 最低空位：23 先于 25 回填。
        assert_eq!(t.alloc(), Some(23));
        for fd in 25..=26 {
            assert_eq!(t.alloc(), Some(fd));
        }
        // 满表 = EMFILE 面（4 槽 23..=26）。
        assert_eq!(t.alloc(), None);
        assert!(t.free(26));
        assert_eq!(t.alloc(), Some(26));
    }

    #[test]
    fn epoll_ctl_add_mod_del_happy() {
        let mut t = EpollTable::new();
        let ep = t.alloc().unwrap();
        // ADD：interest 首空位安装，data 透传原样入表。
        assert!(ectl_add(&mut t, ep, 15, EPOLLIN).is_ok());
        assert!(ectl_add(&mut t, ep, 17, EPOLLOUT | EPOLLERR).is_ok());
        let snap = t.snapshot(ep).unwrap();
        assert_eq!(
            snap[0],
            Some(EpollItem { fd: 15, events: EPOLLIN, data: 15 })
        );
        assert_eq!(
            snap[1],
            Some(EpollItem {
                fd: 17,
                events: EPOLLOUT | EPOLLERR,
                data: 17
            })
        );
        assert_eq!(snap[2..], [None; EPOLL_ITEMS_MAX - 2]);
        // MOD：换掩码 + 换 data（槽位不动——顺序稳定）。
        assert!(t
            .ctl(ep, EPOLL_CTL_MOD, 15, EPOLLHUP, 0xdead_beef)
            .is_ok());
        let snap2 = t.snapshot(ep).unwrap();
        assert_eq!(
            snap2[0],
            Some(EpollItem { fd: 15, events: EPOLLHUP, data: 0xdead_beef })
        );
        // DEL：挖洞不压缩（后续项槽位不动——顺序稳定可测）。
        assert!(t.ctl(ep, EPOLL_CTL_DEL, 15, 0, 0).is_ok());
        let snap3 = t.snapshot(ep).unwrap();
        assert_eq!(snap3[0], None);
        assert_eq!(snap3[1], snap2[1]);
    }

    #[test]
    fn epoll_ctl_rejection_surface() {
        let mut t = EpollTable::new();
        let ep = t.alloc().unwrap();
        // BadFd：epfd 未安装（空槽）/段外/负值。
        assert_eq!(ectl_add(&mut t, 24, 15, EPOLLIN), Err(EpollCtlErr::BadFd));
        assert_eq!(ectl_add(&mut t, 22, 15, EPOLLIN), Err(EpollCtlErr::BadFd));
        assert_eq!(ectl_add(&mut t, 27, 15, EPOLLIN), Err(EpollCtlErr::BadFd));
        // OpBad：op 值域外。
        assert_eq!(
            t.ctl(ep, 0, 15, EPOLLIN, 0),
            Err(EpollCtlErr::OpBad)
        );
        assert_eq!(
            t.ctl(ep, 4, 15, EPOLLIN, 0),
            Err(EpollCtlErr::OpBad)
        );
        // NotSock：target 非 socket 段（console/fs/别名/epoll 段自身）。
        for bad in [0, 2, 3, 10, 14, 22 + 1, 26, 99, -1] {
            assert_eq!(
                ectl_add(&mut t, ep, bad, EPOLLIN),
                Err(EpollCtlErr::NotSock),
                "target={}",
                bad
            );
        }
        // Mask：白名单外（EPOLLET 高位/POLLPRI/POLLNVAL 位）。
        for bad_mask in [
            1u32 << 31,          // EPOLLET
            0x002,               // EPOLLPRI（= POLLPRI 位）
            0x020,               // EPOLLNVAL 位域（非白名单）
            EPOLLIN | 0x040,     // 白名单内混非法位
        ] {
            assert_eq!(
                ectl_add(&mut t, ep, 15, bad_mask),
                Err(EpollCtlErr::Mask),
                "mask={:#x}",
                bad_mask
            );
        }
        // Exists：重复 ADD 同一 target。
        assert!(ectl_add(&mut t, ep, 15, EPOLLIN).is_ok());
        assert_eq!(
            ectl_add(&mut t, ep, 15, EPOLLOUT),
            Err(EpollCtlErr::Exists)
        );
        // Noent：MOD/DEL 未注册 target。
        assert_eq!(
            t.ctl(ep, EPOLL_CTL_MOD, 16, EPOLLIN, 0),
            Err(EpollCtlErr::Noent)
        );
        assert_eq!(
            t.ctl(ep, EPOLL_CTL_DEL, 16, 0, 0),
            Err(EpollCtlErr::Noent)
        );
        // 两实例各注册同一 target 合法（epoll 语义 per-instance）。
        let ep2 = t.alloc().unwrap();
        assert!(ectl_add(&mut t, ep2, 15, EPOLLOUT).is_ok());
    }

    #[test]
    fn epoll_ctl_capacity_invariant() {
        // 容量同源不变量的运行时镜像：socket 段全部 distinct target 注
        // 册恰好填满——后续段内 ADD 恒 Exists（帽满的**可达**形态；
        // ENOSPC 结构性不可达——见 EPOLL_ITEMS_MAX 注）。
        let mut t = EpollTable::new();
        let ep = t.alloc().unwrap();
        for fd in SOCK_FD_BASE..SOCK_FD_BASE + SOCK_FD_MAX as i32 {
            assert!(ectl_add(&mut t, ep, fd, EPOLLIN).is_ok(), "fd={}", fd);
        }
        assert_eq!(
            ectl_add(&mut t, ep, 15, EPOLLIN),
            Err(EpollCtlErr::Exists)
        );
        // 挖洞回填：DEL 后新 ADD 复用空位。
        assert!(t.ctl(ep, EPOLL_CTL_DEL, 17, 0, 0).is_ok());
        assert!(ectl_add(&mut t, ep, 17, EPOLLOUT).is_ok());
        // errno 优先序镜像：BadFd 先于 OpBad/NotSock/Mask（全错参时）。
        assert_eq!(t.ctl(24, 0, 0, 0xdead, 0), Err(EpollCtlErr::BadFd));
    }

    #[test]
    fn epoll_snapshot_free_semantics() {
        let mut t = EpollTable::new();
        // 空槽/段外 snapshot → None。
        assert_eq!(t.snapshot(24), None);
        assert_eq!(t.snapshot(22), None);
        assert_eq!(t.snapshot(27), None);
        let ep = t.alloc().unwrap();
        assert!(ectl_add(&mut t, ep, 15, EPOLLIN | EPOLLOUT).is_ok());
        // 快照 = 值拷贝：取后改表，副本不动。
        let snap = t.snapshot(ep).unwrap();
        assert!(t.ctl(ep, EPOLL_CTL_DEL, 15, 0, 0).is_ok());
        assert_eq!(
            snap[0],
            Some(EpollItem {
                fd: 15,
                events: EPOLLIN | EPOLLOUT,
                data: 15
            })
        );
        // free：释槽后 snapshot → None；重复 free false；未安装槽 false。
        assert!(t.free(ep));
        assert_eq!(t.snapshot(ep), None);
        assert!(!t.free(ep));
        assert!(!t.free(26));
    }
}
