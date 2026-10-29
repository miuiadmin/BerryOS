//! netprobe —— A5 std::net guest 对拍件（`docs/事件循环A5设计草案.md` §3.2）
//!
//! spawn gift 面（bsh `net_gifts` 三元 → env 块 gift[6] → libc `GIFT_H[4]`）
//! 下经**未修改的 Rust std::net** 打 SLIRP 可达回显服务：
//! `TcpStream::connect((host, port))` → write payload → read 回显逐字节
//! 对拍 → exit 0/1（失败 loud——stderr 诊断行 + stdout 失败标记行）。
//!
//! - 域名步：host="localhost" → ToSocketAddrs → getaddrinfo → A4-2 DNS 档
//!   （netd OP_DNS_QUERY → SLIRP DNS → 127.0.0.1 离线确定）
//! - 字面量步：host="127.0.0.1"/"10.0.2.2" → parse_ipv4 快路零 RPC（对照）
//! - 对端 = 宿主 echod（`tools/echod.py`，hostfwd 面外——SLIRP 网关别名
//!   10.0.2.2 直达宿主 loopback；T-c `TCP ok echo=64`/SKT ⓪/EPL 就绪探针
//!   同源专线 18080 串行复用）
//!
//! stdout 恒单行定值（A5-2 nets_selftest 逐字节对拍面；>192B 分片撕行
//! 纪律——诊断走 stderr 不进对拍缓冲）：
//! 成功 `NETPROBE ok n=<len>` / 失败 `NETPROBE fail: <stage> errno=<n>`
//! （echo 段无 errno——逻辑失配非系统错误）。**errno=N 是 bsh 侧拓扑门
//! 信号通道**：B4 掉电相无 netdev（vnet deviceless serve）connect 答
//! ENODEV(19)——nets_selftest 据此静默跳过（EPL/SKT 双门契约第二道），
//! 其余 connect 错误 loud（echod 死 = 真链断该 FAIL）。
//!
//! std::net 深水面（A5 草案 §3.4 首跑定谳项）本件即 probe 载体：撞桩
//! 判死面（bare abort 静默死）按 kill criteria 二裁——失败形态本身是
//! 定谳产物，如实入册。

use std::env;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::ExitCode;

// 链接种子（stdhello 同款）：无引用则 berry-libc 的 crt0（含 `_start`）
// 不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// 默认载荷（A5-2 自验不传参时的对拍基准——定值短串，远小于消息分片界）。
const DEFAULT_PAYLOAD: &str = "berry-netprobe";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("netprobe: usage: host port [payload]");
        return ExitCode::from(2);
    }
    let host = args[1].clone();
    let port: u16 = match args[2].parse() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("netprobe: bad port: {}", args[2]);
            return ExitCode::from(2);
        }
    };
    let payload = if args.len() > 3 {
        args[3].clone()
    } else {
        DEFAULT_PAYLOAD.to_string()
    };

    // connect 域内错误（refused/超时/ENODEV 无 netdev 拓扑）——失败 loud
    // exit 1；errno 数值进 stdout 标记行（bsh 拓扑门信号通道，确定性），
    // strerror 文案走 stderr 诊断行（不进对拍缓冲）。
    let mut stream = match TcpStream::connect((host.as_str(), port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("netprobe: connect {}:{}: {}", host, port, e);
            println!(
                "NETPROBE fail: connect errno={}",
                e.raw_os_error().unwrap_or(0)
            );
            return ExitCode::from(1);
        }
    };
    if let Err(e) = stream.write_all(payload.as_bytes()) {
        eprintln!("netprobe: write: {}", e);
        println!(
            "NETPROBE fail: write errno={}",
            e.raw_os_error().unwrap_or(0)
        );
        return ExitCode::from(1);
    }
    // 回显收取：恰好 payload.len() 字节（EOF 早到 = 短读失败面）。
    let mut echo = vec![0u8; payload.len()];
    let mut got = 0usize;
    while got < echo.len() {
        match stream.read(&mut echo[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) => {
                eprintln!("netprobe: read: {}", e);
                println!(
                    "NETPROBE fail: read errno={}",
                    e.raw_os_error().unwrap_or(0)
                );
                return ExitCode::from(1);
            }
        }
    }
    if got == echo.len() && echo == payload.as_bytes() {
        println!("NETPROBE ok n={}", got);
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "netprobe: echo mismatch got={} want={}",
            got,
            payload.len()
        );
        println!("NETPROBE fail: echo");
        ExitCode::from(1)
    }
}
