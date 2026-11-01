//! C-3 段②：berryctl v0——系统控制/查询 CLI 雏形（shell设计 §2 开发
//! 梯队；AI 友好面 `--json` 留 v2 随交互面）。
//!
//! 三子命令 → psproto 三 op（严格串行 RPC，ps bin 同链拓扑：svcmgr
//! 预建查询对，本进程持 ps_b——env 块 gift[2]，libc `gift2_h()`）：
//! - `ver`：`ver <串>`（svcmgr 编译期版本标识）；
//! - `ps`：拉起账本全表，行面与 ps bin 同（`{name} alive|exited(c)|spawnfail`）；
//! - `mem`：`mem vmos=<n> bytes=<t>`（svcmgr 经既有 vmo_get_size 计数
//!   自己可见的 VMO——内核遥测读 syscall 不新增）。
//!
//! 失败面（无 gift / RPC 帽尽 / 畸形应答 / 未知子命令）stderr 一行诊断
//! + exit 1——标记缺失 = smoke FAIL 兜底（链断 loud）。v0 边界：bsh
//! `run` 拉起档无 ps_b gift（gift 机制是 spawn 侧的）——「no svcmgr
//! gift」退 1 是诚实降级，服务发现式拉起留 v2。

use std::process::exit;

use berry_libc::io::gift2_h;
use berry_psproto::{
    encode_request_op, parse_mem_reply, parse_reply, parse_ver_reply, OP_MEM, OP_PS_LIST, OP_VER,
    STATE_ALIVE, STATE_EXITED,
};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_object_wait_one, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};

// 链接种子（stdhello 同款）：无引用则 berry-libc crt0（含 `_start`）不进
// 链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 全帽（发 + 收各一）：30s——svcmgr 单线程顺序走到自验才服务本请求
/// （慢而活着不误杀，ps 同档；真挂死交帽尽 loud）。
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
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(String::as_str).unwrap_or("");
    let op: u8 = match sub {
        "ver" => OP_VER,
        "ps" => OP_PS_LIST,
        "mem" => OP_MEM,
        _ => {
            eprintln!("usage: berryctl ver|ps|mem");
            exit(1);
        }
    };
    let h: Handle = gift2_h();
    if h == 0 {
        // 非本拓扑 spawn（bsh run 档/旧归档/手工拉起）——无查询端可用。
        eprintln!("berryctl: no svcmgr gift");
        exit(1);
    }
    // ① 发请求（1B 恰长；满 -EBUSY/空不足 -EAGAIN → 等 WRITABLE 重试
    // ——svcmgr send_retry 的 std 侧对称面，两码都续等）。
    let mut req = [0u8; 1];
    let rn = encode_request_op(&mut req, op);
    loop {
        let s = berry_channel_send(h, req.as_ptr(), rn, core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_AGAIN && s != BERRY_ERR_BUSY {
            eprintln!("berryctl: send err {}", s);
            exit(1);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            eprintln!("berryctl: send timeout");
            exit(1);
        }
    }
    // ② 收应答（单帧；-EAGAIN → 等 READABLE；随行 handle 不在协议内——
    // 收下即弃）。
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
            eprintln!("berryctl: recv err {}", s);
            exit(1);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            eprintln!("berryctl: reply timeout");
            exit(1);
        }
    }
    // ③ 按 op 解析 + 渲染（畸形帧整体拒——psproto 执法）。
    match op {
        OP_VER => match parse_ver_reply(&rep, nb) {
            Some(v) => println!("ver {}", String::from_utf8_lossy(v)),
            None => {
                eprintln!("berryctl: bad ver reply");
                exit(1);
            }
        },
        OP_MEM => match parse_mem_reply(&rep, nb) {
            Some((vmos, total)) => println!("mem vmos={} bytes={}", vmos, total),
            None => {
                eprintln!("berryctl: bad mem reply");
                exit(1);
            }
        },
        _ => {
            // OP_PS_LIST：行渲染（ps bin 同面）。
            let mut rows: Vec<(Vec<u8>, u8, i32)> = Vec::new();
            if parse_reply(&rep, nb, |name, st, code| {
                rows.push((name.to_vec(), st, code))
            })
            .is_none()
            {
                eprintln!("berryctl: bad ps reply");
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
        }
    }
    exit(0);
}
