//! cbox_io —— 黑匣子账本 init 落账面（cbox 刀 2，`.agents/plans/cbox/
//! design.md` 持久化节第三窗）。
//!
//! 责任：launcher init 期（layout 装载后、建树前）排液一次 + `CBOX.LOG`
//! 读-改-写——catch launcher 自身上一会话的崩溃（监督重启后 init 重入）
//! 与 battery → launcher 窗口内新入账的 fault。与 svcmgr 侧两窗
//! （battery / 最后活 app 死亡面）三写者同账本：`(boot, seq)` 去重 +
//! [`berry_cbox::boot_pick`] 内容匹配续号 = 幂等互恰，无跨写者协调面。
//!
//! 会话安全：launcher 自持 fs 会话（extras fs dup，libc 单 fd 严格串行
//! ——layout_io 同纪律）；init 期本进程尚无并发 RPC。冻结相
//! （`fs_h()==0`）结构性零 RPC 直退。
//!
//! 失败非致命（launcher 常驻纪律）：eprintln 诊断后照常进桌面——一次
//! 落账缺席 = 记录仍在内核 RAM 表，下个安全窗（app 死亡面/下 boot
//! battery）补落，`(boot, seq)` 去重保不重不漏。诊断一律小写
//! `launcher: cbox` 前缀——不产 `CBOX ok` 标记子串（标记 canon：
//! grep -qF 存在性断言）。

use berry_cbox as cbox;
use berry_libc::fs::{open, O_CREAT, O_RDONLY, O_WRONLY};
use berry_libc::io::{close, fs_h, read, write};
use berry_libc::{get_errno, EIO, ENOENT};

/// 卷根账本文件（8.3 大写名，`LAYOUT.JSN` 惯例同形——svcmgr 侧
/// `CBOX_PATH` 双写镜像，行格式单源在 berry-cbox）。
const PATH: &[u8] = b"CBOX.LOG\0";

/// 单次读块（layout_io 同款 256B）。
const CHUNK: usize = 256;

/// init 落账全链（main 单次调用，layout 装载后）。
pub fn init_flush() {
    if fs_h() == 0 {
        return; // 冻结相：extras 恰 [wm]——账本面结构性缺席，零 RPC 直退
    }
    let mut drain = vec![0u8; cbox::DRAIN_MAX];
    let mut n = 0u32;
    // 出参指针按 ABI 0x42 契约使用（drain 缓冲 844B 定长在册）。
    if berry_sys::berry_cbox_drain(drain.as_mut_ptr(), cbox::DRAIN_MAX as u32, &mut n) != 0 {
        eprintln!("launcher: cbox drain errno");
        return;
    }
    let Some(snap) = cbox::drain_decode(&drain[..n as usize]) else {
        eprintln!("launcher: cbox snapshot corrupt");
        return;
    };
    let old = match read_file() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("launcher: cbox read errno={e}");
            return; // 瞬态错误中止——重写 = 毁旧账，宁可不落
        }
    };
    let boot = cbox::boot_pick(&old, &snap);
    let mut out = vec![0u8; cbox::LEDGER_MAX];
    let st = cbox::merge(&old, &snap, boot, &mut out);
    if st.appended == 0 {
        return; // 幂等（battery 已落/无新记录）——零重写零打印
    }
    if let Err(e) = put_file(&out[..st.len]) {
        eprintln!("launcher: cbox write errno={e}");
        return;
    }
    // 读回对拍（close=TRUNC 已 commit——失配 = loud 诊断，账本以盘上
    // 真值为准）。
    match read_file() {
        Ok(back) if back == out[..st.len] => {}
        Ok(_) => eprintln!("launcher: cbox readback differs"),
        Err(e) => eprintln!("launcher: cbox readback errno={e}"),
    }
}

/// 全量读（layout_io `read_file` 形态）：open(O_RDONLY) → read 循环至
/// EOF → close。ENOENT = 空账本（`Ok(vec![])`——首 boot 常态）；超
/// `LEDGER_MAX` = 畸账 `Err`（调用方中止防毁旧账）。
fn read_file() -> Result<Vec<u8>, i32> {
    // SAFETY: PATH 是 NUL 结尾字面量；fd 按 libc 契约使用。
    let fd = unsafe { open(PATH.as_ptr() as *const _, O_RDONLY) };
    if fd < 0 {
        let e = get_errno();
        return if e == ENOENT { Ok(Vec::new()) } else { Err(e) };
    }
    let mut data: Vec<u8> = Vec::new();
    let mut buf = [0u8; CHUNK];
    loop {
        // read 安全 fn（io.rs——fd≥3 走 fs 面）；<0 错 / 0 EOF。
        let n = read(fd, buf.as_mut_ptr() as *mut _, buf.len());
        if n < 0 {
            let e = get_errno();
            close(fd);
            return Err(e);
        }
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n as usize]);
        if data.len() > cbox::LEDGER_MAX {
            close(fd);
            return Err(EIO); // 超帽畸账——停读（合法账本 ≤ LEDGER_MAX 单源）
        }
    }
    close(fd);
    Ok(data)
}

/// 全量写（layout_io `put_file` 形态）：open(O_WRONLY|O_CREAT) → write
/// 循环 → **close 即 commit**（TRUNC(游标)；中途写失败仍 close——部分
/// 数据照旧提交，POSIX close 恒消费，bsh 文档化取舍同款）。旧长于新时
/// 残留被 TRUNC 截断。
fn put_file(data: &[u8]) -> Result<(), i32> {
    // SAFETY: PATH NUL 结尾；fd 按 libc 契约使用。
    let fd = unsafe { open(PATH.as_ptr() as *const _, O_WRONLY | O_CREAT) };
    if fd < 0 {
        return Err(get_errno());
    }
    let mut off = 0usize;
    while off < data.len() {
        // write 安全 fn（io.rs——内部按 192B 分片 PUT）；部分写按返回量续写。
        let n = write(fd, data[off..].as_ptr() as *const _, data.len() - off);
        if n < 0 {
            let e = get_errno();
            close(fd); // 部分 commit（文档化取舍）
            return Err(e);
        }
        if n == 0 {
            close(fd);
            return Err(EIO); // 零写非 EOF = 协议破缺面
        }
        off += n as usize;
    }
    // close 安全 fn：出错 = commit（TRUNC）失败面——errno 透传。
    if close(fd) != 0 {
        return Err(get_errno());
    }
    Ok(())
}
