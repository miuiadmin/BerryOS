//! bsh —— P5 A-2 交互 shell（`文档/shell设计.md` v0 最小面，stdhello 同链：
//! musl target + berry-libc shim + berry-exec 流式拉起）。
//!
//! 通道拓扑（boot 手搭，见 main.rs A-2 段）：fd0 = endB2（console 转发键盘
//! 完整行，'\n' 规范化）、fd1/2 = console 别名（env 块 gift[0]）、rootfs VMO
//! READ dup = gift[2]（`rootfs_h()` 消费——`run <name>` 流式取 ELF）。
//!
//! 验收（smoke rootfs 相第 8 标记）：**自驱自验**——启动即内部 `run hexyl`
//! （256B pattern + `--color=never` + 私有 stdout 对 + 32 页栈），收齐 EOF 且
//! 字节数 == 2156（svcmgr `HEXYL_PLAIN` 同源基准）才打 `BSH ok`；失败 =
//! eprintln 诊断 + `park()`（标记缺失 loud 兜底，不赌交互输入——rootfs 相
//! stdin=/dev/null）。交互循环是化妆面：prompt `bsh> ` 只挂命令完成；fd0
//! 空读（libc 10s 帽超时 EOF 化）= 噪声，静默续读不重刷 prompt。
//!
//! v1 内建：help / echo / run / exit（Plan B v0.5：`exit` 经 std::process::exit
//! → libc `exit` → 0x33 真退出）。`run` 后 `reap()` 完整回收子进程（Plan B
//! 前「每次 run 泄漏一个 park 中的子进程」的旧常态退役）。

use std::io::{BufRead, Write};

use berry_libc::io::rootfs_h;
use berry_sys::{
    berry_channel_recv, berry_clock_read, berry_handle_close, berry_object_wait_one, Handle,
    BERRY_ERR_AGAIN, CLOCK_MONOTONIC, SIGNAL_READABLE,
};
use berry_exec::{channel_pair, cpio_find, spawn_rootfs_elf, SpawnSpec};

// 链接种子（stdhello 同款）：rlib/LTO 按需保留——无引用则 berry-libc 的
// crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// hexyl 0.17.0 plain 输出基准长度（= svcmgr `HEXYL_PLAIN.bin` 字节数；
/// 升版 hexyl 须重产基准并同步此常量——本自验只对长度，逐字节终验在
/// svcmgr `HEXYL ok`，双标记独立成立）。
const HEXYL_PLAIN_LEN: usize = 2156;
/// 自验输入：0x00..=0xff 全字节 pattern（与 svcmgr hexyl 面**同源同值**——
/// 256B 恰一条消息 = inline 帽边界）。
const HEXYL_STDIN: [u8; 256] = {
    let mut a = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        a[i] = i as u8;
        i += 1;
    }
    a
};
/// 收集帽：子进程 stdout 存储上限（堆 256KiB 内给收集留的份额；超出只记
/// 截断不存——EOF 照收，relay 打警告）。
const COLLECT_MAX: usize = 32 * 1024;
/// 通道等待帽（活动帽：每次收包刷新——慢而活着不误杀；超时无 EOF = 断链面）。
const TIMEOUT_NS: u64 = 10 * 1_000_000_000;

/// 一次 `run` 的产物。
struct RunOut {
    data: Vec<u8>,
    /// 收到零长 EOF（确定性终点）与否（false = 10s 活动帽耗尽）。
    eof: bool,
    /// 超收集帽被丢弃的字节数。
    dropped: usize,
}

fn main() {
    // —— 自驱自验：内部 `run hexyl`，全链（cpio 查询 → 流式拉起 → feeder →
    //    私有 stdout 收齐）即交互 `run` 的同一台机器——自验过 = 交互面可信。
    match run_capture("hexyl", Some(&HEXYL_STDIN), Some("--color=never")) {
        Some(o) if o.eof && o.dropped == 0 && o.data.len() == HEXYL_PLAIN_LEN => {}
        Some(o) => {
            eprintln!(
                "bsh: selftest bad: eof={} len={} want={} dropped={}",
                o.eof, o.data.len(), HEXYL_PLAIN_LEN, o.dropped
            );
            berry_libc::park();
        }
        None => {
            eprintln!("bsh: selftest: hexyl not found");
            berry_libc::park();
        }
    }
    println!("BSH ok");

    // —— 交互循环（化妆面）：prompt 只挂命令完成；空读（fd0 的 10s 帽超时
    //    EOF 化）= 噪声 → 静默续读，prompt 不重刷（rootfs 相无人敲键盘也
    //    不刷屏；真交互者敲行即得响应）。
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut prompt_pending = true;
    loop {
        if prompt_pending {
            write!(out, "bsh> ").unwrap();
            out.flush().unwrap();
            prompt_pending = false;
        }
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            continue; // 空读噪声：不重刷 prompt、不睡（下轮 read 再阻塞 10s）
        }
        dispatch(line.trim_end_matches(['\n', '\r']), &mut out);
        prompt_pending = true;
    }
}

/// 命令分派（v1 四内建 + not found）。
fn dispatch(cmd: &str, out: &mut std::io::Stdout) {
    if cmd.is_empty() {
        return;
    }
    if cmd == "help" {
        writeln!(
            out,
            "bsh 内建：help | echo <text> | run <name> | exit（rootfs bin/ 下程序）"
        )
        .unwrap();
    } else if cmd == "exit" {
        out.flush().unwrap();
        std::process::exit(0); // → libc exit → EOF + berry_process_exit(0x33)
    } else if let Some(text) = cmd.strip_prefix("echo ") {
        writeln!(out, "{}", text.trim()).unwrap();
    } else if let Some(rest) = cmd.strip_prefix("run") {
        if rest.is_empty() {
            writeln!(out, "usage: run <name>").unwrap();
        } else {
            do_run(rest.trim(), out);
        }
    } else {
        writeln!(out, "bsh: {}: not found", cmd).unwrap();
    }
    out.flush().unwrap();
}

/// `run <name>`：cpio 查 `bin/<name>` → 私有 stdout 对拉起（32 页栈 = musl
/// 128KiB 口径，盖住 clap 级栈深）→ 收齐 EOF → 回放 console。
/// 子进程无 stdin（env 块单 gift，libc /dev/null 语义——read 即 EOF）。
fn do_run(name: &str, out: &mut std::io::Stdout) {
    if name.is_empty() || name.contains([' ', '/']) {
        writeln!(out, "usage: run <name>").unwrap();
        return;
    }
    let full = format!("bin/{}", name);
    let Some((data_off, data_len)) = cpio_find(rootfs_h(), full.as_bytes()) else {
        writeln!(out, "bsh: run: {}: not found", name).unwrap();
        return;
    };
    let (out_s, out_c) = channel_pair();
    let spec = SpawnSpec {
        name: full.as_bytes(),
        data_off,
        data_len,
        stdout: out_c,
        stack_pages: 32,
        stdin_feed: None,
        stdin_handle: None,
        extra_arg: None,
        extra_gift: None,
    };
    if let Ok(child) = spawn_rootfs_elf(rootfs_h(), &spec) {
        let o = collect(out_s);
        out.write_all(&o.data).unwrap();
        if !o.eof {
            writeln!(out, "[bsh: run {}: no EOF (timeout)]", name).unwrap();
        }
        if o.dropped > 0 {
            writeln!(out, "[bsh: run {}: {}B dropped]", name, o.dropped).unwrap();
        }
        out.flush().unwrap();
        // Plan B：收完输出后回收子进程（挂死/帽尽 = 句柄留存有界泄漏，警告
        // 不死 shell——终验面在 svcmgr `EXIT ok`）。
        if child.reap().is_err() {
            writeln!(out, "[bsh: run {}: reap failed]", name).unwrap();
            out.flush().unwrap();
        }
    } else {
        writeln!(out, "bsh: run: {}: elf bad", name).unwrap();
    }
    // out_c 原值：子表已持 SEND 别名（spawn 内 dup 迁移），本表副本使命已毕
    // （svcmgr verify_hexyl 同款）。
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        berry_libc::park(); // 关不掉 = 泄漏面，loud 死（不 trap：交互面不死 shell）
    }
}

/// 拉起 + 收集（自验/交互共用）：查档 → spawn → collect → 回收 → 关对端。
/// None = 档未命中或 ELF 坏（调用方 not found / elf bad）。
fn run_capture(name: &str, stdin: Option<&[u8]>, extra_arg: Option<&str>) -> Option<RunOut> {
    let full = format!("bin/{}", name);
    let (data_off, data_len) = cpio_find(rootfs_h(), full.as_bytes())?;
    let (out_s, out_c) = channel_pair();
    let spec = SpawnSpec {
        name: full.as_bytes(),
        data_off,
        data_len,
        stdout: out_c,
        stack_pages: 32,
        stdin_feed: stdin,
        stdin_handle: None,
        extra_arg: extra_arg.map(|a| a.as_bytes()),
        extra_gift: None,
    };
    let child = match spawn_rootfs_elf(rootfs_h(), &spec) {
        Ok(c) => c,
        Err(_) => berry_libc::park(), // 自验面：链断 loud（标记缺失兜底）
    };
    let o = collect(out_s);
    // Plan B：reap 失败 ≠ run 链断（终验面在 svcmgr `EXIT ok`）——诊断可见
    // 即可，BSH ok 语义仍只锚定输出面。
    if child.reap().is_err() {
        eprintln!("bsh: selftest: reap failed");
    }
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        berry_libc::park();
    }
    Some(o)
}

/// 收子进程 stdout 至 EOF / 活动帽（verify_hexyl 同构：满 192B 分片逐条
/// 入队，空队 wait READABLE 10s 活动帽，零长消息 = EOF 终点）。
fn collect(h: Handle) -> RunOut {
    let mut data: Vec<u8> = Vec::new();
    let mut dropped = 0usize;
    let mut eof = false;
    let mut msg = [0u8; 256];
    while !eof {
        let mut nb = msg.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            if nb == 0 {
                eof = true;
            } else {
                let keep = nb.min(COLLECT_MAX.saturating_sub(data.len()));
                data.extend_from_slice(&msg[..keep]);
                dropped += nb - keep;
            }
            continue;
        }
        if s != BERRY_ERR_AGAIN {
            break; // 非 -EAGAIN 错误：对端异常，按无 EOF 收场（调用方打警告）
        }
        // 空队：阻塞等 READABLE（10s 活动帽，收包刷新）
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + TIMEOUT_NS, &mut obs) != 0 {
            break; // 活动帽尽仍无 EOF：断链面，交调用方警告
        }
    }
    RunOut { data, eof, dropped }
}

/// now = clock_read（失败按当前时刻 0 处理 → 立即超时，活证死路 loud）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        berry_libc::park();
    }
    t
}
