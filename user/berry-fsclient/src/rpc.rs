//! fs RPC 会话机 + fd 面分片 op（C-1 刀 1 自 bsh 迁居，语义逐字保真）。
//!
//! 迁居面：`fs_rpc`（严格串行 RPC 单趟：EBUSY 阻塞重试 + 收侧 READABLE 帽
//! + **会话毒化位**——任一收侧超时即作废整个会话，v1 无请求序号防迟到
//! 应答错配）、`fs_rpc_status`（应答恰 1 字节 st 面）、双帽（常规
//! [`TIMEOUT_NS`] 10s / commit 面 [`FS_COMMIT_CAP_NS`] 90s——分片 PUT 与
//! TRUNC 都走 commit 帽：跨名 PUT 对保留 staging 槽隐式重跑整链 commit，
//! P6 A-6/A-7 对抗审查实抓）。bsh 刀 4 删自有面，改经 libc（刀 2 消费此处）
//! ——「不长第二套」的终点形态。
//!
//! fd 面 op（libc 刀 2 的原料，全部零分配——berry-libc 是纯 core crate）：
//! - 读：`get_chunk(path, off)` 单片 GET（≤CHUNK 短读合法，POSIX 短读面；
//!   调用方循环至 more=0 / EOF）。
//! - 写：`put_chunk(path, off, data)` 单片 PUT（data ≤CHUNK）+ close 时
//!   `commit(path, size)` = TRUNC——**写通道 commit 点 = close**（C-1 裁决
//! 2，bsh `>` 两段式同款）。
//! - 状态五 op：lookup / mkdir / unlink / readdir / stat。
//!
//! `cfg(not(test))` 整模块：berry-sys 的 syscall 符号在宿主不存在——host
//! 测试只盖 lib.rs 纯函数面；none target 编译由 workspace `cargo build`
//! （build-user pass2）覆盖、真执行由 QEMU 面（刀 2/3/4 标记）覆盖。

use berry_fsproto::{
    encode_get, encode_lookup, encode_mkdir, encode_put, encode_readdir, encode_stat,
    encode_trunc, encode_unlink, parse_reply_get, parse_reply_list, parse_reply_stat,
    ST_OK,
};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_object_wait_one, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
};

use crate::{st_to_err, FsErr};

/// 通道等待帽（活动帽：常规 op——RAM 面/单片 GET）；commit 面（PUT 分片/
/// TRUNC/LOOKUP/READDIR/STAT 的 FAT 回落链走查）走 [`FS_COMMIT_CAP_NS`]。
pub const TIMEOUT_NS: u64 = 10 * 1_000_000_000;

/// fs 写请求（分片 PUT + TRUNC——FAT 层 commit 载体）的收侧帽：commit 在
/// fsmgr 侧是一串**串行扇区 RPC**（16KiB 帽文件 worst ≈ 22 次），TCG 下
/// 每次往返 ~0.8s——10s 常规帽**结构性误杀**（P6 A-6 实踩：服务没死只是
/// 慢，晚到应答还会毒化会话）。跨名 PUT 也会先对保留的旧 staging 槽隐式
/// 重跑同款 commit——故 PUT 分片同帽（对抗审查实抓首版只给 TRUNC 扩帽漏
/// 了 PUT 面）。90s 覆盖帽文件 worst 仍有 4x 余量；真挂死由 smoke 侧标记
/// 帽兜底，交互面 v1 接受单命令阻塞（异步 commit/后台 flush 随 v2）。
pub const FS_COMMIT_CAP_NS: u64 = 90 * 1_000_000_000;

/// RPC 请求/应答缓冲（PUT worst = op+path+off+CHUNK ≤ 246B；bsh 同值）。
pub const RPC_BUF: usize = 256;

/// fs 会话健康位：任一收侧超时即作废（见 [`rpc`] 文档注释）——libc 单进程
/// 长驻，静态位即会话位。诊断访问器 [`session_ok`]。
static SESSION_OK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);

/// 会话健康查询（毒化后 fs 面确定性降级「fs unreachable」，恢复面随 v2）。
pub fn session_ok() -> bool {
    SESSION_OK.load(core::sync::atomic::Ordering::Relaxed)
}

/// now = clock_read；**失败按 0 处理 → deadline = 帽值绝对时刻、真 now 已
/// 过 → 立即超时**（fail-fast loud；bsh 版此处 park 冻结，libc 侧无 park
/// 依赖故降级为快速失败——两形态都把时钟不可用当死路，不静默自旋）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 严格串行 RPC 单趟（返回应答长度入 buf；None = 断链）。`cap` = 收侧
/// READABLE 帽（常规 [`TIMEOUT_NS`]；commit 链走 [`FS_COMMIT_CAP_NS`]）。
///
/// 发送侧：EBUSY → 阻塞等 WRITABLE（10s 帽重臂——console_write 同款契约，
/// 雷 2 纪律：不自旋）；发送侧超时**不毒化**（请求未发出、无迟到应答）。
///
/// **会话毒化**（P6 A-1 对抗审查实抓）：v1 协议无请求序号——任一**收侧**
/// 超时后，fsmgr 的迟到应答仍会入队，下一 RPC 的 poll-first 收侧会把它
/// 错配成自己的应答（GET 面 = 静默错数据，off-by-one 永不自愈）。故收侧
/// 超时即作废整个会话：后续 fs op 确定性降级「fs unreachable」（确定性
/// 失败优于无告警错数据；迟到应答无人消费、滞留队列无害）。
fn rpc(h: Handle, buf: &mut [u8; RPC_BUF], reqlen: usize, cap: u64) -> Option<usize> {
    if !session_ok() {
        return None;
    }
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, buf.as_ptr(), reqlen, core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            return None;
        }
        if deadline == 0 {
            deadline = now() + TIMEOUT_NS;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            return None;
        }
        deadline = 0; // 重臂（命中后若仍满则续等——公平排队）
    }
    // 应答：空队阻塞等 READABLE（cap 帽——严格串行下一请求一应答，应答
    // 必先于下一请求到队）。
    loop {
        let mut nb = buf.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(h, buf.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            return Some(nb);
        }
        if s != BERRY_ERR_AGAIN {
            return None;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + cap, &mut obs) != 0 {
            SESSION_OK.store(false, core::sync::atomic::Ordering::Relaxed); // 收侧超时 = 会话作废
            return None;
        }
    }
}

/// st 应答单趟（应答恰 1 字节 st；PUT/TRUNC 面）。长度破缺 = 协议面 Rpc。
fn rpc_status(h: Handle, buf: &mut [u8; RPC_BUF], reqlen: usize, cap: u64) -> Result<(), FsErr> {
    let n = rpc(h, buf, reqlen, cap).ok_or(FsErr::Rpc)?;
    if n == 1 {
        if buf[0] == ST_OK {
            return Ok(());
        }
        return Err(st_to_err(buf[0]));
    }
    Err(FsErr::Rpc)
}

/// GET 单片（fd read 面）：`GET(path, off)` → 数据搬进 `buf[0..len]`（≤
/// CHUNK，短读合法）；`(len, more)` ——EOF = `(0, false)`。服务端不变量
/// more=true ⇒ dlen≥1，破缺 = Rpc。帽 = 常规 [`TIMEOUT_NS`]（bsh fs_get
/// 同值零漂移）。
pub fn get_chunk(
    h: Handle,
    path: &[u8],
    off: u32,
    buf: &mut [u8; RPC_BUF],
) -> Result<(usize, bool), FsErr> {
    if h == 0 {
        return Err(FsErr::NoFs);
    }
    let l = encode_get(buf, path, off).ok_or(FsErr::Rpc)?;
    let n = rpc(h, buf, l, TIMEOUT_NS).ok_or(FsErr::Rpc)?;
    // 数据在应答帧中部——前移到 buf[0..]（copy_within：源/目的同缓冲重叠，
    // copy_from_slice 对自重叠是 memcpy 契约 UB——P6 A-7 实踩雷）。
    let (len, more, src) = match parse_reply_get(&buf[..n]).ok_or(FsErr::Rpc)? {
        Ok(g) => {
            if g.more && g.data.is_empty() {
                return Err(FsErr::Rpc);
            }
            let src = g.data.as_ptr() as usize - buf.as_ptr() as usize;
            (g.data.len(), g.more, src)
        }
        Err(st) => return Err(st_to_err(st)),
    };
    buf.copy_within(src..src + len, 0);
    Ok((len, more))
}

/// PUT 单片（fd write 面）：`PUT(path, off, data)`——off 超现长零填充扩容
/// （staging）；data ≤CHUNK 由调用方分片。帽 = [`FS_COMMIT_CAP_NS`]（跨名
/// PUT 对保留 staging 槽隐式重跑 commit——见模块头）。
pub fn put_chunk(h: Handle, path: &[u8], off: u32, data: &[u8]) -> Result<(), FsErr> {
    if h == 0 {
        return Err(FsErr::NoFs);
    }
    let mut buf = [0u8; RPC_BUF];
    let l = encode_put(&mut buf, path, off, data).ok_or(FsErr::Rpc)?;
    rpc_status(h, &mut buf, l, FS_COMMIT_CAP_NS)
}

/// 写通道 commit（close 面，C-1 裁决 2）：`TRUNC(path, size)`——staging 落
/// 定；空数据文件 = size 0（`echo > f` 建空文件面）。帽同 [`put_chunk`]。
pub fn commit(h: Handle, path: &[u8], size: u32) -> Result<(), FsErr> {
    if h == 0 {
        return Err(FsErr::NoFs);
    }
    let mut buf = [0u8; RPC_BUF];
    let l = encode_trunc(&mut buf, path, size).ok_or(FsErr::Rpc)?;
    rpc_status(h, &mut buf, l, FS_COMMIT_CAP_NS)
}

/// LOOKUP `<path>`（存在性探针 = open 快速失败面；RAM 树 → FAT 回落，
/// 帽按链走查 worst = [`FS_COMMIT_CAP_NS`]）。
pub fn lookup(h: Handle, path: &[u8]) -> Result<(), FsErr> {
    if h == 0 {
        return Err(FsErr::NoFs);
    }
    let mut buf = [0u8; RPC_BUF];
    let l = encode_lookup(&mut buf, path).ok_or(FsErr::Rpc)?;
    rpc_status(h, &mut buf, l, FS_COMMIT_CAP_NS)
}

/// MKDIR `<path>`（RAM 目录树；已存在 → Exists）。纯 RAM 面（fsmgr 不回落
/// FAT）→ [`TIMEOUT_NS`]。
pub fn mkdir(h: Handle, path: &[u8]) -> Result<(), FsErr> {
    if h == 0 {
        return Err(FsErr::NoFs);
    }
    let mut buf = [0u8; RPC_BUF];
    let l = encode_mkdir(&mut buf, path).ok_or(FsErr::Rpc)?;
    rpc_status(h, &mut buf, l, TIMEOUT_NS)
}

/// UNLINK `<path>`：文件或**空**目录（非空目录 → NotEmpty）。→
/// [`TIMEOUT_NS`]。
pub fn unlink(h: Handle, path: &[u8]) -> Result<(), FsErr> {
    if h == 0 {
        return Err(FsErr::NoFs);
    }
    let mut buf = [0u8; RPC_BUF];
    let l = encode_unlink(&mut buf, path).ok_or(FsErr::Rpc)?;
    rpc_status(h, &mut buf, l, TIMEOUT_NS)
}

/// READDIR `<dir>`：子名 blob（NUL 连接）搬进 `buf[0..len]`（dirent_next
/// 消费）；文件目标 → Bad。FAT 目录回落 = 链走查 → [`FS_COMMIT_CAP_NS`]。
pub fn readdir(h: Handle, path: &[u8], buf: &mut [u8; RPC_BUF]) -> Result<usize, FsErr> {
    if h == 0 {
        return Err(FsErr::NoFs);
    }
    let l = encode_readdir(buf, path).ok_or(FsErr::Rpc)?;
    let n = rpc(h, buf, l, FS_COMMIT_CAP_NS).ok_or(FsErr::Rpc)?;
    let (len, src) = match parse_reply_list(&buf[..n]).ok_or(FsErr::Rpc)? {
        Ok(names) => {
            let src = names.as_ptr() as usize - buf.as_ptr() as usize;
            (names.len(), src)
        }
        Err(st) => return Err(st_to_err(st)),
    };
    buf.copy_within(src..src + len, 0);
    Ok(len)
}

/// STAT `<path>`：`(TYPE_FILE, 字节数)` / `(TYPE_DIR, 子项数)`——libc 的
/// fstat/stat 整形（`st_mode_of`）与 SEEK_END 的 size 都从这里取。帽同
/// READDIR（FAT 回落 = 同一链走查）。
pub fn stat(h: Handle, path: &[u8]) -> Result<(u8, u32), FsErr> {
    if h == 0 {
        return Err(FsErr::NoFs);
    }
    let mut buf = [0u8; RPC_BUF];
    let l = encode_stat(&mut buf, path).ok_or(FsErr::Rpc)?;
    let n = rpc(h, &mut buf, l, FS_COMMIT_CAP_NS).ok_or(FsErr::Rpc)?;
    match parse_reply_stat(&buf[..n]).ok_or(FsErr::Rpc)? {
        Ok(r) => Ok(r),
        Err(st) => Err(st_to_err(st)),
    }
}
