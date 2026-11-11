//! berry-argv —— argv blob 组装律（A-2 SpawnSpec / d-3a env 块 v1）。
//!
//! 纯函数纪律（berry-winplan 同款）：零分配、零 unsafe、零 FFI；输入只
//! 借用，输出写调用方定长缓冲——**装不下就不装（整参或零参），绝不截
//! 半参**。host 单测全盖（tests/argv.rs）。
//!
//! 语义源 = berry-exec `spawn_rootfs_elf` 的 env 块组装段（迁出时逐字
//! 等价，勿漂移）：
//! - blob 192B（消息 inline 帽 256 内、argv0 + 数参的 cp/mv 形态留足
//!   余量）；
//! - argv0 = cpio 名：截 191B（NUL 终结占 1B——argv0 恒可达，永不因
//!   容量缺席）；
//! - 追加参逐个续装：`plen + len + 1 > 192` 即停（该参整体不装，其后
//!   参也不再装——单调截断，无中段装参洞）；
//! - 消费端（libc io.rs boot 的 argv 解析）按 NUL 分隔逐项读。
//!
//! v1 参数都是短 CLI 词，实测最长的 uu_ls 长参面也在 192 内——截断是
//! 防御面不是运行期常态。

// berry-virtio/berry-netproto 同款：非 test 构建无 std（workspace
// none-target 成员），host 测试构建放开 std（tests/ 独立 crate 本就带 std）。
#![cfg_attr(not(test), no_std)]

/// env 块 blob 容量（192B：消息 inline 帽 256 内、argv0 + 数参的 cp/mv
/// 形态留足余量）。
pub const ARGV_BLOB_SZ: usize = 192;

/// argv blob 组装：`argv0\0` + `args` 各项 `\0` 续装，返回有效长度。
///
/// - argv0 截 [`ARGV_BLOB_SZ`] − 1（NUL 终结占位——argv0 恒在场）；
/// - 追加参「整参或零参」：装不下（`plen + len + 1 > 192`）即停，其后
///   参不再尝试（单调截断——消费端不会读到中段洞）；
/// - `argv0` 空片合法（blob = 单 NUL，plen=1——spawn 面不出现，纯函数
///   面不拒绝）；
/// - out 由调用方提供（berry-exec 传栈上 `[0u8; 192]`；本函数只写
///   `[..plen]`，余部不动）。
pub fn argv_blob(out: &mut [u8], argv0: &[u8], args: &[&[u8]]) -> usize {
    let cap = out.len().min(ARGV_BLOB_SZ);
    let n0 = argv0.len().min(cap - 1);
    out[..n0].copy_from_slice(&argv0[..n0]);
    // NUL 分隔符自立写入（原 berry-exec 内嵌版靠调用方预零 [0u8; 192]
    // 缓冲——对预零输入两写法逐字节等价；纯函数面不依赖调用方清缓冲）。
    out[n0] = 0;
    let mut plen = n0 + 1;
    for arg in args.iter() {
        if plen + arg.len() + 1 > cap {
            break;
        }
        out[plen..plen + arg.len()].copy_from_slice(arg);
        out[plen + arg.len()] = 0;
        plen += arg.len() + 1;
    }
    plen
}

// 消费面单源引回说明见 berry-exec/src/lib.rs 的 use 处注释——本 crate
// 不长第二套：spawn_rootfs_elf 的 env 块组装调 argv_blob 原值。
