//! # berry-fsclient —— fs 服务客户端会话机 + libc 文件面纯函数（C-1 刀 1）
//!
//! 合约源 = `docs/VM里程碑设计草案.md` §C-1。两层：
//!
//! - **纯函数面**（本文件，host 测试）：fd 3+ 表项的客户端游标模型、cwd
//!   拼接、stat 整形、dirent 解码、ST_*→FsErr 映射——libc（刀 2）把这些
//!   组装成 open/close/read/write/lseek/stat/opendir/readdir/chdir/getcwd。
//! - **会话机**（`rpc` 模块，`cfg(not(test))`）：自 bsh 迁居的 fs_rpc 核心
//!   （严格串行 RPC、任一收侧超时即毒化会话、90s commit / 10s 常规双帽、
//!   EBUSY 阻塞重试形态）+ fd 面分片 op。协议零新增——fsproto v2 十 op
//!   原样（GET/PUT 自带 off，客户端游标 = 服务端无会话）。
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

// 单源再出口：libc（刀 2）只 dep 本 crate 即得帽常量。
pub use berry_fsproto::{CHUNK, FILE_SIZE_MAX, NAME_MAX, PATH_DEPTH_MAX, PATH_MAX, TYPE_DIR, TYPE_FILE};

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
/// fd 表容量（8 槽 fd 3..=10：RAM 树 FILE_COUNT_MAX=8 同量级；满 = EMFILE
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
}
