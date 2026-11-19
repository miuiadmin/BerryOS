//! conf —— FAT canonical 单簇原子读写面（波3-件⑦ 抽取自 berry-launcher
//! layout_io.rs 的文件 I/O 半程——LAYOUT.JSN/BOOT.JSN 同款持久键）。
//!
//! 责任半程（三约束的 I/O 侧）：**单簇 ≤cap 近原子写**（`open(O_WRONLY|
//! O_CREAT)` → write 循环 → **close 即 commit**——TRUNC(游标) 两段式，
//! bsh `put_file` 同款；O_TRUNC 省略 = 覆写幂等）+ **全量读**（open →
//! read 循环至 EOF → close；超帽停读归 Oversize 自愈面）。三约束的策略
//! 半程（缺席/坏件回默认 + 重写自愈 + 写失败非致命）与键 schema（LAYOUT
//! cells / BOOT 模式）归消费侧——launcher 的 layout_io 是首消费者。
//!
//! **冻结相门归调用方**（原 `fs_h()==0` 早退语义不进本模块）：fs gift
//! 缺席（`Host::fs == None`）时本模块按 I/O 失败收口（`Failed(EIO)` /
//! `Err(EIO)`）——冻结相零 RPC 零打印的结构性保证由消费者先导门承担。
//!
//! fd 操作经 [`crate::Host`] 绑定表（无 berry-sys 直连面）——本模块恒
//! 可 host 编译；host 测试面 = 开局分类/超帽边界/零写破缺三谓词。

use alloc::vec::Vec;

use crate::host::host;

/// 读块（bsh `get_file` 同款 256B）。
const CHUNK: usize = 256;

/// POSIX open 旗（berry-libc fs.rs 立值镜像——值域钉板测试对拍；open 旗
/// 不进 Host 面：读写两径旗集固定，无消费者自由度）。
pub const O_RDONLY: i32 = 0;
pub const O_WRONLY: i32 = 1;
pub const O_CREAT: i32 = 0o100;

/// `ENOENT`（berry-libc 立值镜像——文件缺席与其余 I/O 错的分界码）。
pub const ENOENT: i32 = 2;
/// `EIO`（零写非 EOF = 协议破缺面的回填码）。
pub const EIO: i32 = 5;

/// 持久键（8.3 大写名 + NUL 结尾——单 FAT 卷根，CWD 默认根；`cap` =
/// 单簇帽，合法载荷 ≤ cap，超帽归 [`ReadOutcome::Oversize`] 自愈面）。
pub struct ConfKey {
    pub path: &'static [u8],
    pub cap: usize,
}

/// 读结局四态。
pub enum ReadOutcome {
    /// ENOENT——文件缺席（首 boot 常态）。
    Absent,
    /// 全文 ≤ cap。
    Bytes(Vec<u8>),
    /// 超单簇帽（异种/腐败文件面——重写自愈）。
    Oversize(usize),
    /// 其余 I/O 错（errno）。
    Failed(i32),
}

/// 读开局分类（纯函数，host 测试面）：open 失败码 → ENOENT 缺席 / 其余
/// I/O 错。
pub(crate) fn read_open_fail(errno: i32) -> ReadOutcome {
    if errno == ENOENT {
        ReadOutcome::Absent
    } else {
        ReadOutcome::Failed(errno)
    }
}

/// 超帽判定（纯函数，host 测试面）：读入后长度 > cap 即 Oversize（== cap
/// 合法——单簇帽是闭上界）。
pub(crate) fn chunk_oversize(len: usize, cap: usize) -> bool {
    len > cap
}

/// 零写判定（纯函数，host 测试面）：write 返回 0 非 EOF = 协议破缺面
///（回填 EIO）。
pub(crate) fn write_eof(n: isize) -> bool {
    n == 0
}

/// 全量读（原 layout_io `read_file` 形态精简）：open(O_RDONLY) → read
/// 循环至 EOF → close（读 fd 的 close 无 commit 面——幂等消费）。帽 =
/// `cap`——超即停读归 Oversize。fs 绑定缺席 = `Failed(EIO)`（冻结相门
/// 归调用方——见模块头注）。
pub fn read_key(key: &ConfKey) -> ReadOutcome {
    let Some(fs) = host().fs else {
        return ReadOutcome::Failed(EIO);
    };
    // SAFETY: key.path 是 NUL 结尾字面量；fd 按绑定表契约使用。
    let fd = unsafe { (fs.open)(key.path.as_ptr() as *const _, O_RDONLY) };
    if fd < 0 {
        return read_open_fail((fs.errno)());
    }
    let mut data: Vec<u8> = Vec::new();
    let mut buf = [0u8; CHUNK];
    loop {
        let n = (fs.read)(fd, buf.as_mut_ptr(), buf.len());
        if n < 0 {
            let e = (fs.errno)();
            (fs.close)(fd);
            return ReadOutcome::Failed(e);
        }
        if n == 0 {
            break; // EOF
        }
        data.extend_from_slice(&buf[..n as usize]);
        if chunk_oversize(data.len(), key.cap) {
            (fs.close)(fd);
            return ReadOutcome::Oversize(data.len());
        }
    }
    (fs.close)(fd);
    ReadOutcome::Bytes(data)
}

/// 全量写（原 layout_io `put_file` 形态）：open(O_WRONLY|O_CREAT) →
/// write 循环 → **close 即 commit**（TRUNC(游标)）。中途写失败仍 close
/// ——部分数据照旧提交（POSIX close 恒消费，bsh 文档化取舍同款）。
/// fs 绑定缺席 = `Err(EIO)`。
pub fn write_key(key: &ConfKey, data: &[u8]) -> Result<(), i32> {
    let Some(fs) = host().fs else {
        return Err(EIO);
    };
    // SAFETY: key.path 是 NUL 结尾字面量；fd 按绑定表契约使用。
    let fd = unsafe { (fs.open)(key.path.as_ptr() as *const _, O_WRONLY | O_CREAT) };
    if fd < 0 {
        return Err((fs.errno)());
    }
    let mut off = 0usize;
    while off < data.len() {
        let n = (fs.write)(fd, data[off..].as_ptr(), data.len() - off);
        if n < 0 {
            let e = (fs.errno)();
            (fs.close)(fd); // 部分 commit（文档化取舍）
            return Err(e);
        }
        if write_eof(n) {
            (fs.close)(fd);
            return Err(EIO); // 零写非 EOF = 协议破缺面
        }
        off += n as usize;
    }
    // close：出错 = commit（TRUNC）失败面——errno 透传。
    if (fs.close)(fd) != 0 {
        return Err((fs.errno)());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// open 失败分类：ENOENT = 缺席（首 boot 常态）；其余 errno 原值透传
    ///（EIO/EACCES/… 一律 Failed——重写自愈面）。
    #[test]
    fn read_open_fail_splits_enoent() {
        assert!(matches!(read_open_fail(ENOENT), ReadOutcome::Absent));
        assert!(matches!(read_open_fail(EIO), ReadOutcome::Failed(5)));
        assert!(matches!(read_open_fail(13), ReadOutcome::Failed(13)));
        assert!(matches!(read_open_fail(0), ReadOutcome::Failed(0)));
    }

    /// 单簇帽是闭上界：== cap 合法、cap+1 超帽（Oversize 自愈）。
    #[test]
    fn oversize_boundary_is_exclusive() {
        assert!(!chunk_oversize(2048, 2048));
        assert!(chunk_oversize(2049, 2048));
        assert!(!chunk_oversize(0, 2048));
    }

    /// 零写 = 协议破缺（EIO 回填）；正进展/负错码不是 EOF 面。
    #[test]
    fn write_eof_only_on_exact_zero() {
        assert!(write_eof(0));
        assert!(!write_eof(1));
        assert!(!write_eof(192));
        assert!(!write_eof(-5));
    }

    /// 旗值/errno 立值钉板（berry-libc fs.rs/lib.rs 同值——POSIX 值域）。
    #[test]
    fn posix_values_pinned() {
        assert_eq!((O_RDONLY, O_WRONLY, O_CREAT), (0, 1, 0o100));
        assert_eq!((ENOENT, EIO), (2, 5));
    }
}
