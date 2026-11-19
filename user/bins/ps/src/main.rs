//! C-3 段①：ps——进程表查询客户端（svcmgr 拉起账本的渲染面）。
//!
//! 通道拓扑：svcmgr `ps_selftest` 预建 (ps_a, ps_b)，ps_b 随 env 块
//! gift[2] 迁入本进程（libc `gift2_h()`——extra_gifts 单元素恰落位 2，
//! 零 libc 新面）；严格串行 RPC（发 1B 请求 → 等 → 收帧 → 退）。
//!
//! 行面 `{name} {state}`（state = `alive` / `exited(code)` / `spawnfail`）
//! 到 stdout——本进程 stdout = spawn 侧私有捕获通道（uu_run 同款，非
//! console），svcmgr 收齐后按家常驻名单逐行断言打 `PS ok n=`。失败面
//! （无 gift / RPC 帽尽 / 畸形应答）stderr 一行诊断 + exit 1——标记缺失
//! = smoke FAIL 兜底（链断 loud）。

use std::process::exit;

use berry_libc::io::gift2_h;
use berry_psproto::{encode_request, parse_reply, STATE_ALIVE, STATE_EXITED};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_object_wait_one, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};

// 链接种子（stdhello 同款）：无引用则 berry-libc crt0（含 `_start`）不进
// 链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 全帽（发 + 收各一）：30s——svcmgr 单线程顺序走到 ps_selftest 才
/// 服务本请求（慢而活着不误杀，U1_EOF_CAP 同档；真挂死交帽尽 loud）。
const RPC_CAP_NS: u64 = 30 * 1_000_000_000;

/// now = clock_read（失败按 0 → deadline 0 = 立即超时，活证死路 loud）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

fn main() {
    let h: Handle = gift2_h();
    if h == 0 {
        // 非本拓扑 spawn（旧归档/手工拉起）——无查询端可用。
        eprintln!("ps: no svcmgr gift");
        exit(1);
    }
    // ① 发请求（1B 恰长；满 -EBUSY/空不足 -EAGAIN → 等 WRITABLE 重试
    // ——svcmgr send_retry 的 std 侧对称面，两码都续等）。
    let mut req = [0u8; 1];
    let rn = encode_request(&mut req);
    loop {
        let s = berry_channel_send(h, req.as_ptr(), rn, core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_AGAIN && s != BERRY_ERR_BUSY {
            eprintln!("ps: send err {}", s);
            exit(1);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            eprintln!("ps: send timeout");
            exit(1);
        }
    }
    // ② 收应答（单帧 ≤ REPLY_MAX；-EAGAIN → 等 READABLE；随行 handle 不
    // 在协议内——收下即弃）。
    let mut rep = [0u8; 256];
    let nb;
    loop {
        let mut n = rep.len();
        let mut hh = [0 as Handle; 6];
        let mut nh = hh.len();
        let s = berry_channel_recv(h, rep.as_mut_ptr(), &mut n, hh.as_mut_ptr(), &mut nh);
        if s == 0 {
            for i in 0..nh {
                // 多带 handle：丢弃勿用（P6 A-4 富余收口纪律的客户端面）。
                berry_handle_close(hh[i]);
            }
            nb = n;
            break;
        }
        if s != BERRY_ERR_AGAIN {
            eprintln!("ps: recv err {}", s);
            exit(1);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            eprintln!("ps: reply timeout");
            exit(1);
        }
    }
    // ③ 解析 + 渲染（畸形帧整体拒——半张表不进渲染面，psproto 执法）。
    let mut rows: Vec<(Vec<u8>, u8, i32)> = Vec::new();
    if parse_reply(&rep, nb, |name, st, code| {
        rows.push((name.to_vec(), st, code))
    })
    .is_none()
    {
        eprintln!("ps: bad reply");
        exit(1);
    }
    for (name, st, code) in &rows {
        let name = String::from_utf8_lossy(name);
        if *st == STATE_EXITED {
            println!("{} exited({})", name, code);
        } else if *st == STATE_ALIVE {
            println!("{} alive", name);
        } else {
            println!("{} spawnfail", name);
        }
    }
    exit(0);
}
