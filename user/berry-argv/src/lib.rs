//! berry-argv —— processargs wire 格式单源（bootstrap 通道两条消息的
//! 编码律：消息一 argv v1＋消息二 env v1.1；通道时序与收面解析律的家
//! 在 berry-exec `spawn_rootfs_elf` / berry-libc `berry_libc_boot` 头
//! 注，此处不双写）。
//!
//! 纯函数纪律（berry-winplan 同款）：零分配、零 unsafe、零 FFI；输入只
//! 借用，输出写调用方定长缓冲——**装不下就不装（整项或零项），绝不截
//! 半项**。host 单测全盖（tests/argv.rs）。
//!
//! 消息一（argv v1，恒形——env 有无不改本消息逐字节形态）：
//! - blob 192B（消息 inline 帽 256 内、argv0 + 数参的 cp/mv 形态留足
//!   余量）；
//! - argv0 = cpio 名：截 191B（NUL 终结占 1B——argv0 恒可达，永不因
//!   容量缺席）；
//! - 追加参逐个续装：`plen + len + 1 > 192` 即停（该参整体不装，其后
//!   参也不再装——单调截断，无中段装参洞）；
//! - 消费端（libc io.rs boot 的 argv 解析）按 NUL 分隔逐项读。语义源
//!   = berry-exec 旧内嵌组装段（外迁时逐字等价，勿漂移）。
//!
//! 消息二（env v1.1，通道级 add-only：产侧导出集非空才发送、0 句柄、
//! 在 argv send 之后 process_start 之前压队——不传 env 的调用方消息流
//! 逐字节零变化，自定入口结构性不可见）：
//! - payload = KEY=VALUE\0 续装串，帽 256B（＝通道消息帽恰满——env
//!   唯一收面是 libc boot 的 256B 收窗，无窄窗约束，满帽用满）；
//! - 每项为产侧预拼好的完整 KEY=VALUE 字节串（'=' 语义校验不在编码
//!   层——归产侧构造纪律与收侧解析防御）；
//! - 整项或零项单调截断（argv 同律）；空项跳过（编码面禁 \0\0 歧义）；
//!   含内嵌 NUL 项 debug_assert 拦。
//!
//! v1 参数都是短 CLI 词，实测最长的 uu_ls 长参面也在 192 内——截断是
//! 防御面不是运行期常态（env 项同律）。

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

/// env blob 容量（256B：＝通道消息帽恰满——env 唯一收面是 libc boot
/// 的 256B 收窗，无窄窗约束，满帽用满）。
pub const ENV_BLOB_SZ: usize = 256;

/// env blob 组装（消息二载荷）：`entries` 各项（产侧预拼好的完整
/// KEY=VALUE 字节串）按 `KEY=VALUE\0` 续装，返回有效长度。
///
/// - 空集（或全空项）返 0——空导出集零发送由调用方判，本函数纯组装；
/// - 空项跳过（空项若装即裸 NUL——编码面禁 \0\0 歧义）；
/// - 含内嵌 NUL 项 debug_assert 拦（KEY=VALUE 预拼面不含 NUL；预检
///   覆盖全部入参，含因截断不再装的项——release 构建编译出，防御面
///   非运行期常态）；
/// - 「整项或零项」：装不下（`plen + len + 1 > 256`）即停，其后项不
///   再尝试（单调截断——消费端不会读到中段洞；argv 同律）；
/// - out 由调用方提供（berry-exec 传栈上 `[0u8; 256]`；本函数只写
///   `[..plen]`，余部不动）。
pub fn env_blob(out: &mut [u8], entries: &[&[u8]]) -> usize {
    let cap = out.len().min(ENV_BLOB_SZ);
    // 输入面卫生预检：任何项含 NUL 即拦（"env_blob:" 前缀供 host 侧
    // should_panic 断言锚定）。
    for e in entries.iter() {
        debug_assert!(!e.contains(&0), "env_blob: 内嵌 NUL 项");
    }
    let mut plen = 0usize;
    for e in entries.iter() {
        if e.is_empty() {
            continue; // 空项跳过（编码面禁 \0\0 歧义）
        }
        if plen + e.len() + 1 > cap {
            break; // 装不下即停——其后项不再装（单调截断）
        }
        out[plen..plen + e.len()].copy_from_slice(e);
        out[plen + e.len()] = 0;
        plen += e.len() + 1;
    }
    plen
}

// 消费面单源引回说明见 berry-exec/src/lib.rs 的 use 处注释——本 crate
// 不长第二套：spawn_rootfs_elf 的消息一（argv）/消息二（env）组装调
// argv_blob/env_blob 原值。
