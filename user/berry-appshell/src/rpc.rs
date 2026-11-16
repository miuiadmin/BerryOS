//! rpc —— 严格串行 RPC 半程族 + fail/park 收口（波3-件⑦ 抽取自
//! berry-launcher main.rs——gfxdemo `rpc` 形态镜像，行为零变化）。
//!
//! 形态：**严格串行**（单通道一请求一应答——并发面归消费者结构排除）；
//! 发送侧 -EBUSY → 阻塞等 WRITABLE（帽重臂，雷 2 纪律不自旋）；收侧
//! 空队 -EAGAIN → 阻塞等 READABLE（帽）。**send 即 take**（A6-b）：
//! attach 随行 handle 在 send 成功即迁出本表——SUBMIT 每帧需 fresh dup
//! 别名（pump 消费面）。
//!
//! [`fail`] = 诊断一行 + [`Host::park`]（常驻不退出——崩溃监督/一次重启
//! 帽归 svcmgr 域；诊断前缀 = 绑定表 tag，launcher 绑 "launcher" 与抽取
//! 前字节一致）。
//!
//! host 测试面：send/recv 半程的**重试分类谓词**（0/BUSY/其余与
//! 0/空应答/AGAIN/其余的真值表）——syscall 面本体 `cfg(not(test))`
//! （berry-fsclient 先例：berry-sys 符号宿主不存在）。

use berry_sys::{BERRY_ERR_AGAIN, BERRY_ERR_BUSY};
#[cfg(not(test))]
use berry_sys::{berry_channel_recv, berry_channel_send, berry_clock_read,
                berry_handle_close, berry_object_wait_one, Handle, CLOCK_MONOTONIC,
                SIGNAL_READABLE, SIGNAL_WRITABLE};

use crate::host::{diag, host};

/// RPC 收侧 READABLE 帽（gfxdemo 10s A6-a 形态；launcher 300s：首启要等
/// svcmgr 跑完 boot 自验进稳态 app serve，10s 会把 APP_LIST 误判成超时）。
pub const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 诊断 + park 收口（原 launcher `fail` 的 park 形——常驻不被 reap，
/// exit 码无消费者；park = stdout EOF + 10s 周期睡眠，消费者经 Host 注入）。
pub fn fail(what: &str) -> ! {
    diag(what);
    (host().park)()
}

/// 编码收口（None = 组帧 bug——定容 256B 缓冲恒够，结构性不可达；
/// gfxdemo `enc_req` 同形）。
pub fn enc_req(m: Option<usize>) -> usize {
    match m {
        Some(n) => n,
        None => fail("req encode"),
    }
}

// —— 重试分类谓词（纯函数，host 测试面）——

/// send 半程分类。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SendStep {
    /// send 成功（随行 handle 已迁移）。
    Done,
    /// -EBUSY：通道满 → 等 WRITABLE 后重试（帽重臂）。
    Wait,
    /// 其余错码：致命（fail 收口）。
    Fatal(i32),
}

/// send 半程分类谓词（0 成功 / -BUSY 等待 / 其余致命）。
pub(crate) fn send_step(s: i32) -> SendStep {
    if s == 0 {
        SendStep::Done
    } else if s == BERRY_ERR_BUSY {
        SendStep::Wait
    } else {
        SendStep::Fatal(s)
    }
}

/// recv 半程分类。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RecvStep {
    /// 应答到账（nb = 字节数）。
    Frame(usize),
    /// -EAGAIN：空队 → 等 READABLE 后重试（帽）。
    Wait,
    /// 成功但零长应答（协议破缺——fail 收口）。
    Empty,
    /// 其余错码：致命。
    Fatal(i32),
}

/// recv 半程分类谓词（0 且非空 = 帧 / 0 且空 = 破缺 / -EAGAIN = 等待 /
/// 其余致命）。
pub(crate) fn recv_step(s: i32, nb: usize) -> RecvStep {
    if s == 0 {
        if nb == 0 {
            RecvStep::Empty
        } else {
            RecvStep::Frame(nb)
        }
    } else if s == BERRY_ERR_AGAIN {
        RecvStep::Wait
    } else {
        RecvStep::Fatal(s)
    }
}

// —— syscall 半程（cfg(not(test))：berry-sys 符号宿主不存在）——

/// now = clock_read；失败按 0 → deadline 即刻到期 = fail-fast（时钟不可用
/// 当死路，不静默自旋）。
#[cfg(not(test))]
pub fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 即关（RELEASE 归还别名——关失败 = handle 表泄漏，FAIL loud）。
#[cfg(not(test))]
pub fn close_h(h: Handle) {
    if berry_handle_close(h) != 0 {
        fail("handle close");
    }
}

/// 严格串行 RPC **发送半程**（一请求多应答分片流的发送与接收拆开复用——
/// icons 的 APP_LIST 面）。-EBUSY → 阻塞等 WRITABLE（帽重臂，雷 2 纪律
/// 不自旋；send 原子性——EBUSY/错码时随行 handle 零迁移）。
#[cfg(not(test))]
pub fn rpc_send(ch: Handle, req: &[u8], attach: &[Handle]) {
    let (hptr, hn) = if attach.is_empty() {
        (core::ptr::null(), 0)
    } else {
        (attach.as_ptr(), attach.len())
    };
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(ch, req.as_ptr(), req.len(), hptr, hn);
        match send_step(s) {
            SendStep::Done => return,
            SendStep::Wait => {}
            SendStep::Fatal(e) => fail(&alloc::format!("rpc send err={e}")),
        }
        if deadline == 0 {
            deadline = now() + RPC_CAP_NS;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            fail("rpc send wait timeout");
        }
        deadline = 0; // 重臂（命中后仍满则续等——公平排队）
    }
}

/// 严格串行 RPC **收侧半程**（单条应答消息；流式会话逐消息复用）：空队
/// -EAGAIN → 阻塞等 READABLE（`RPC_CAP_NS` 帽）。返回 `(应答长, 随行
/// handle 数)`；`rhs` 出参 = 应答随行 handle（RELEASE 归还别名——调用方
/// 即关）。每轮重置容量（in-out 出参——漏重置 = 恒 -EINVAL 死循环，A1-b）。
#[cfg(not(test))]
pub fn rpc_recv(ch: Handle, rep: &mut [u8; 256], rhs: &mut [Handle; 8]) -> (usize, usize) {
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(ch, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        match recv_step(s, nb) {
            RecvStep::Frame(_) => return (nb, nh),
            RecvStep::Empty => fail("rpc empty reply"),
            RecvStep::Wait => {}
            RecvStep::Fatal(e) => fail(&alloc::format!("rpc recv err={e}")),
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            fail("rpc reply timeout 10s");
        }
    }
}

/// 严格串行 RPC 单趟（一请求一应答；wm/present 域消费面）。返回
/// `(应答长, 随行 handle 数)`。
#[cfg(not(test))]
pub fn rpc(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (usize, usize) {
    rpc_send(ch, req, attach);
    rpc_recv(ch, rep, rhs)
}

/// 1B 状态应答单趟收口（一切非 OK 应答恒 1B 不带数据；fail 收口 =
/// 诊断 + park）。
#[cfg(not(test))]
pub fn expect_ok(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
    what: &str,
) {
    let (n, _) = rpc(ch, req, attach, rep, rhs);
    match berry_guiproto::parse_status_rep(&rep[..n]) {
        Some(berry_guiproto::ST_OK) => {}
        Some(st) => fail(&alloc::format!("{what} st={st}")),
        None => fail(&alloc::format!("{what} rep bad len={n}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// send 分类真值表：0 成功 / -BUSY 等待 / 其余（含正数与异负数）致命。
    #[test]
    fn send_step_truth_table() {
        assert_eq!(send_step(0), SendStep::Done);
        assert_eq!(send_step(BERRY_ERR_BUSY), SendStep::Wait);
        assert_eq!(send_step(-5), SendStep::Fatal(-5));
        assert_eq!(send_step(-22), SendStep::Fatal(-22));
    }

    /// recv 分类真值表：成功非空 = 帧；成功零长 = 协议破缺（fatal 面）；
    /// -EAGAIN = 等待（含收长非零——分类只看状态码）；其余致命。
    #[test]
    fn recv_step_truth_table() {
        assert_eq!(recv_step(0, 12), RecvStep::Frame(12));
        assert_eq!(recv_step(0, 1), RecvStep::Frame(1));
        assert_eq!(recv_step(0, 0), RecvStep::Empty);
        assert_eq!(recv_step(BERRY_ERR_AGAIN, 0), RecvStep::Wait);
        assert_eq!(recv_step(BERRY_ERR_AGAIN, 8), RecvStep::Wait);
        assert_eq!(recv_step(-9, 0), RecvStep::Fatal(-9));
        assert_eq!(recv_step(-22, 0), RecvStep::Fatal(-22));
    }

    /// 帽常量钉板：300s（launcher 首启等 svcmgr 稳态的口径，注释单源）。
    #[test]
    fn rpc_cap_is_300s() {
        assert_eq!(RPC_CAP_NS, 300 * 1_000_000_000);
    }
}
