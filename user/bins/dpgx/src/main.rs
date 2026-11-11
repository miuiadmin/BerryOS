//! dpgx —— X-4 E 面 exec 差分子（std musl 单 k 特形，见 Cargo.toml 注）。
//!
//! 帧序契约（svcmgr `dpgx_foot` 配对收割，dpg_foot 同形）：
//!   1. 起动后**先**把单行 `DPGX ready\n` 打进 stdout 私有通道——父侧
//!      收齐该行 = 运行时预热页集已触 + 本程已到停泊点（确定性定泊
//!      信号，不赌 TCG 调度时序——判例 §A61 同源：时序不是契约）；
//!   2. 随即阻塞在 stdin `read`（父持对端不发不 EOF = 永停泊，映像
//!      页集在册停泊——footprint 记账面稳定，dpg_foot 同款停泊语义）；
//!   3. 终局由父 `process_terminate` 收割（read 中被杀 = 干净路径）。
//!
//! 双生配对：同一 bin 文件两次拉起，demand 胎（svcmgr 钩子在位）vs
//! eager 胎（berry-exec `EagerGuard` 摘钩）——唯一变量 = 装载分支。

use std::io::Read;

// crt0（含 `_start`）不进链接；`#[used]` 防死码消除——stdhello 同款。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    // 定泊信号：单行、行缓冲即冲（berry-libc stdout 行纪律同 bench1）。
    println!("DPGX ready");
    // 停泊：stdin 无馈恒阻塞（父持对端）。馈到（非 bench 形态）= 静默退。
    let mut b = [0u8; 1];
    let _ = std::io::stdin().read(&mut b);
}
