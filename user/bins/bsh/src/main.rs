//! bsh —— P5 A-2 交互 shell（`docs/shell设计.md` v0 最小面 + v0.5 管道，
//! stdhello 同链：musl target + berry-libc shim + berry-exec 流式拉起）。
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
//! P5 v0.5 管道（`PIPE ok` = rootfs 相第 10 标记）：`A | B` 相邻段间 channel
//! 对当 pipe（前段 stdout = 写端 / 后段 stdin = 读端，`shell设计.md` §2.2
//! 「channel 即 pipe」）——自验三跑对拍：`hexyl | hexyl` 管道输出 == 同字节
//! 直喂 `hexyl` 参照**逐字节全等**（证明管道把 A 的 stdout 原样递到 B 的
//! stdin）+ 全链 EOF 到齐才打标。
//!
//! P6 A-1：fs 面（`FS ok` = rootfs 相第 11 标记）——`>` 重定向 + `cat`/
//! `ls` 内建。**C-1 刀 4 起全量走 berry-libc 文件面**（open/read/write/
//! close/opendir/readdir/stat/mkdir/unlink 直调——与 stdhello 的 std::fs
//! 同一条 libc 路，自有 RPC/协议会话机退役迁 berry-fsclient/libc，不再长
//! 第二套）。**操作符识别纪律不变**：`>` 检测在展开前的原始文本上（`|`/
//! 赋值前置，`>>` 被 `>` 计数 >1 自然排除）；**诊断/usage/not-found 打真
//! stdout（stderr 语义——不随 `>` 进文件）**，数据字节经 render_cmd 返
//! 调用方定向（直出 = 打 stdout，重定向 = 写入文件）。v1 面：简单命令重
//! 定向（`|`+`>` 组合、`>>` 追加随 v2）；空命令（展开后空）不建文件
//! （POSIX `> f` 建空文件——v2 再议）；无 fsmgr = `fs not available`
//! 降级不死 shell。
//!
//! C-0 刀 4（多段路径面，C-1 刀 4 继承）：cat/`>`/带参 ls 多段——libc
//! resolve 同规则同帽（段字符集白名单、NAME_MAX/DEPTH/PATH_MAX——usage
//! 预检在 bsh 侧镜像一份，保「坏路径 usage 2 vs 服务错误 1」的旧区分零
//! 漂移，双侧同步纪律第三镜）；内建 mkdir/rm 与带参 ls（READDIR——RAM
//! 目录树 + FAT 目录回落，服务端分层）；裸 `ls` = opendir("/")（fsmgr
//! READDIR 根 = LIST 同源 overlay 全景——RAM + FAT 并合，C-1 刀 4 服务
//! 侧对齐）。自验 `DIR ok` = 多段读写 RAM 全链（FAT 子目录 QEMU 活覆盖
//! 在 smoke FAT 子目录相注入段——cat DATA/NOTE.TXT / ls DATA）。
//!
//! C-1 刀 5（cd/pwd）：libc cwd 族（chdir/getcwd——进程级 CWD 静态）；
//! **相对路径语义自此成立**——cat/`>`/cd 的相对名经 libc resolve 挂 CWD
//! 解析。自验 `CD ok` = 沙箱全链（chdir → pwd → 相对名落子目录 → 绝对
//! 多段同文件读回 → 回根后相对名即失——CWD 真迁移非文本戏法）。
//!
//! v1 内建：help / echo / env / run / exit / cat / ls / mkdir / rm / cd / pwd
//!（Plan B v0.5：`exit` 经 std::process::exit
//! → libc `exit` → 0x33 真退出）。`run` 后 `reap()` 完整回收子进程（Plan B
//! 前「每次 run 泄漏一个 park 中的子进程」的旧常态退役）。
//!
//! P5 v0 收官（`shell设计.md` §2.3/§2.5）：**环境变量**（shell 局部变量表 +
//! `$NAME`/`$?` 展开 + `env` 内建；子进程 env 传递 defer v2——argv 块无 env
//! 面，POSIX `export` 语义随之）+ **`&&` 短路链**（`|` 优先级更高：先按 &&
//! 切行、各 part 再分派处理 `|`；每 part **执行时**才展开变量——
//! `A=1 && echo $A` 的求值序正确）。**操作符识别（&&/|/赋值形）一律在展开
//! 前的原始文本上**——展开结果只是数据、不当语法重解析（POSIX 分词先于
//! 展开；对抗审查实抓：`P=a|b` 后 `echo $P` 曾被展开文本里的 `|` 切成管道
//! 误报 usage、`P=hexyl|hexyl` 后裸 `$P` 会把变量值当语法执行）。状态码 sh
//! 惯例：0 成功 / 2 usage / 126 spawn failed / 127 not found / run|pipe =
//! 末段子进程真码（reap Err → 1——真码源 = 0x3A `process_get_exit_code`，
//! P5 扩号）。

use std::io::{BufRead, Write};

use berry_libc::io::{close, fs_h, read, rootfs_h, write};
use berry_libc::{
    EEXIST, EIO, EISDIR, EINVAL, ENOENT, ENOSPC, ENOTDIR, ENOTEMPTY,
};
use berry_libc::fs::{
    chdir, closedir, getcwd, mkdir, open, opendir, readdir, stat, unlink, O_CREAT, O_RDONLY,
    O_WRONLY, Stat,
};
use berry_sys::{
    berry_channel_recv, berry_clock_read, berry_handle_close, berry_object_wait_one, Handle,
    BERRY_ERR_AGAIN, CLOCK_MONOTONIC, SIGNAL_READABLE,
};
use berry_exec::{channel_pair, cpio_find, spawn_rootfs_elf, ChildHandles, SpawnSpec};
// C-1 刀 4：fsproto 只余帽常量（usage 预检 + 读面失控兜底）——协议面
// （encode/parse/ST_*）随自有 RPC 退役，全部走 berry-libc。
use berry_fsproto::{FILE_SIZE_MAX, NAME_MAX, PATH_DEPTH_MAX, PATH_MAX};

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
    //    输出绑 `hexyl_out` 留用：fs 自验的 2156B 多片写入样本（P6 A-1）。
    let hexyl_out: Vec<u8> = if let Some((o, _)) =
        run_capture("hexyl", Some(&HEXYL_STDIN), Some("--color=never"))
    {
        if !(o.eof && o.dropped == 0 && o.data.len() == HEXYL_PLAIN_LEN) {
            eprintln!(
                "bsh: selftest bad: eof={} len={} want={} dropped={}",
                o.eof,
                o.data.len(),
                HEXYL_PLAIN_LEN,
                o.dropped
            );
            berry_libc::park();
        }
        o.data
    } else {
        eprintln!("bsh: selftest: hexyl not found");
        berry_libc::park();
    };
    println!("BSH ok");
    // —— P5 v0：变量/状态壳（`&&` 自验与交互循环共享——`$?` 跨行语义）。
    let mut vars: Vec<(String, String)> = Vec::new();
    let mut status: i32 = 0;
    and_selftest(&mut vars, &mut status);
    pipe_selftest();
    fs_selftest(&hexyl_out);
    fat_selftest();
    fatw_selftest();
    dir_selftest();
    cd_selftest();

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
        run_line(line.trim_end_matches(['\n', '\r']), &mut vars, &mut status, &mut out);
        prompt_pending = true;
    }
}

/// `&&` 自验（P5 v0，确定性状态谓词——无串口噪声：三步全在变量表/状态码
/// 上断言，唯一子进程 `run fail` 零输出）。失败 eprintln + park（PIPE ok
/// 缺失 → smoke FAIL 兜底；置于 BSH ok 后、pipe_selftest 前）。
///
/// ① `X=1 && Y=2` 双置（&& 链上赋值连续求值）；② `run fail && Z=99` →
/// status=42（`fail` = exit 42 的 rootfs 程序，0x3A 真码链）**且 Z 未置**
/// ——若短路失效：Z 被置 99 且 status 被尾段赋值改写为 0，双证同时崩；
/// ③ `RC=$?` → RC=="42"（`$?` 展开 = 上一步真码）。
fn and_selftest(vars: &mut Vec<(String, String)>, status: &mut i32) {
    let mut out = std::io::stdout();
    run_line("X=1 && Y=2", vars, status, &mut out);
    run_line("run fail && Z=99", vars, status, &mut out);
    run_line("RC=$?", vars, status, &mut out);
    let get = |n: &str| get_var(vars, n).map(|s| s.to_string());
    if *status != 0
        || get("X").as_deref() != Some("1")
        || get("Y").as_deref() != Some("2")
        || get("Z").is_some()
        || get("RC").as_deref() != Some("42")
    {
        eprintln!(
            "bsh: && selftest bad: status={} X={:?} Y={:?} Z={:?} RC={:?}",
            status,
            get("X"),
            get("Y"),
            get("Z"),
            get("RC")
        );
        berry_libc::park();
    }
}

/// 变量表查询（后写覆盖：`rev` 找最新——线性表 + 覆盖写，shell 变量量级）。
fn get_var<'a>(vars: &'a [(String, String)], name: &str) -> Option<&'a str> {
    vars.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

/// `A=1` 赋值形检测（**原始** trimmed part 上——赋值优先于管道/命令分派；
/// 值域可含 `|` 等语法字符，落表时才展开）：`[A-Za-z_][A-Za-z0-9_]*=`
/// 前缀 → Some((name, value))。`echo X=1` 不匹配（名后非 `=`——扫描停在
/// 空格）。
fn assignment_of(s: &str) -> Option<(&str, &str)> {
    let b = s.as_bytes();
    if b.is_empty() || !(b[0].is_ascii_alphabetic() || b[0] == b'_') {
        return None;
    }
    let mut j = 1;
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
        j += 1;
    }
    if j < b.len() && b[j] == b'=' {
        Some((&s[..j], &s[j + 1..]))
    } else {
        None
    }
}

/// `$NAME` / `$?` 展开（未定义变量 → 空串，sh 惯例；无 `${}`/引号/转义面
/// ——v1）。按字节扫描：多字节 UTF-8 连续字节 ≥0x80 不撞 ASCII 语法位。
fn expand(s: &str, vars: &[(String, String)], status: i32) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() {
            let c = b[i + 1];
            if c == b'?' {
                out.extend_from_slice(status.to_string().as_bytes());
                i += 2;
                continue;
            }
            if c.is_ascii_alphabetic() || c == b'_' {
                let (st, mut j) = (i + 1, i + 2);
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if let Some(v) = get_var(vars, &s[st..j]) {
                    out.extend_from_slice(v.as_bytes());
                }
                i = j;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 一行命令：按 `&&` 切段顺序执行，**非零短路**（sh 语义）。`|` 优先级更
/// 高——各 part 进 [`exec_one`] 后再分派处理管道；**每 part 执行时才展开
/// 变量**（`A=1 && echo $A` 的求值序）。`$?` 跨 part/跨行共享（`status`
/// 由调用方持有）。空 part = 无操作（status 0——v1 不做语法错误面）。
fn run_line(line: &str, vars: &mut Vec<(String, String)>, status: &mut i32, out: &mut std::io::Stdout) {
    for part in line.split("&&") {
        if part.trim().is_empty() {
            continue;
        }
        *status = exec_one(part, vars, status, out);
        if *status != 0 {
            return; // 短路：后续 part 不求值（含其中的赋值/展开）
        }
    }
}

/// 单段命令：trim → 赋值形检测（**原始文本**；值展开后落表——`RC=$?`）→
/// `|` 管道检测（**原始文本**——展开结果只是数据、永不产操作符）→ `>`
/// 重定向检测（原始文本，P6 A-1）→ 展开（用**进入时**的 `$?`）→ 分派
/// （render_cmd）。返回段状态码：0 成功 / 2 usage / 126 spawn failed /
/// 127 not found / run|pipe = 子进程真码（reap Err → 1）。
fn exec_one(part: &str, vars: &mut Vec<(String, String)>, status: &mut i32, out: &mut std::io::Stdout) -> i32 {
    let raw = part.trim();
    if raw.is_empty() {
        return 0;
    }
    // 赋值（原始文本上检测，优先于管道——`P=a|b` 的 | 进值域；值在落表时
    // 才展开——`RC=$?`）。覆盖写 = 同名尾置（get_var rev 找最新）。
    if let Some((name, value)) = assignment_of(raw) {
        let value = expand(value, vars, *status);
        vars.push((name.to_string(), value));
        return 0;
    }
    // 管道（原始文本上检测，优先于内建名匹配——`a | b` 整体不是内建名）。
    // 若在展开后文本上重解析：`P=a|b` 后 `echo $P` 被值里的 | 切成管道误报
    // usage、`P=hexyl|hexyl` 后裸 `$P` 会把变量值当语法执行（对抗审查实抓）。
    // `&&` 切行同在 run_line 的展开前做。
    if raw.contains('|') {
        return do_pipe(raw, vars, *status, out);
    }
    // 重定向（原始文本上检测，同纪律；`|`+`>` 组合先撞管道分支的段校验
    // ——`b > c` 段含空格 → usage，v1 面文档化）。`>>` 追加与多点 `>` 被
    // 计数 >1 自然排除为 usage（`a >> b`/`a > b > c`）。
    if raw.contains('>') {
        return do_redirect(raw, vars, *status, out);
    }
    let (data, code) = render_cmd(raw, vars, *status, out);
    out.write_all(&data).unwrap();
    out.flush().unwrap();
    code
}

/// 内建/外部命令统一渲染（P6 A-1 起 `>` 重定向与直出共道）：返回
/// (数据字节, 状态码)。**诊断/usage/not-found/警告打 `out`**（stderr 语义
/// ——不随 `>` 进文件），数据面字节交调用方定向（直出 = 打 stdout；
/// 重定向 = put_file 入文件）。`exit` 不重定向照常执行（v1：先于写文件——
/// POSIX 的 open-before-exec 面随 v2）。词边界纪律：`run `/`echo `/`cat `
/// 带空格才算命令 + 参数（`runfoo` = 未知命令 127）。
fn render_cmd(cmd_s: &str, vars: &[(String, String)], status: i32, out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    let cmd = expand(cmd_s, vars, status);
    if cmd.is_empty() {
        return (Vec::new(), 0);
    }
    if cmd == "help" {
        writeln!(
            out,
            "bsh 内建：help | echo <text> | env | run <name> | cat <file> | ls | ls <dir> | mkdir <dir> | rm <path> | cd <dir> | pwd | A | B（rootfs 管道）| A && B | A > file | exit"
        )
        .unwrap();
        (Vec::new(), 0)
    } else if cmd == "exit" {
        out.flush().unwrap();
        // → libc exit → EOF + berry_process_exit(0x33)
        std::process::exit(0)
    } else if cmd == "env" {
        // P5 v0：变量表回显（插入序；覆盖写的旧值也列——get 取最新，回显
        // 面向人不做去重）。
        let mut d = Vec::new();
        for (k, v) in vars.iter() {
            d.extend_from_slice(k.as_bytes());
            d.push(b'=');
            d.extend_from_slice(v.as_bytes());
            d.push(b'\n');
        }
        (d, 0)
    } else if let Some(text) = cmd.strip_prefix("echo ") {
        let mut d = text.trim().as_bytes().to_vec();
        d.push(b'\n');
        (d, 0)
    } else if cmd == "echo" {
        (b"\n".to_vec(), 0)
    } else if cmd == "run" {
        writeln!(out, "usage: run <name>").unwrap();
        (Vec::new(), 2)
    } else if let Some(rest) = cmd.strip_prefix("run ") {
        run_output(rest.trim(), out)
    } else if cmd == "cat" {
        writeln!(out, "usage: cat <file>").unwrap();
        (Vec::new(), 2)
    } else if let Some(rest) = cmd.strip_prefix("cat ") {
        cat_output(rest.trim(), out)
    } else if cmd == "ls" {
        ls_output(out)
    } else if let Some(rest) = cmd.strip_prefix("ls ") {
        // 刀 4：带参 ls = READDIR（RAM 目录树 + FAT 目录回落，同一条
        // libc 路——不长第二套）；坏路径 usage 2（「已知命令+坏参数」
        // 口径不变，P6 A-1 对抗审查纪律）。
        ls_path_output(rest.trim(), out)
    } else if cmd == "mkdir" {
        writeln!(out, "usage: mkdir <dir>").unwrap();
        (Vec::new(), 2)
    } else if let Some(rest) = cmd.strip_prefix("mkdir ") {
        mkdir_output(rest.trim(), out)
    } else if cmd == "rm" {
        writeln!(out, "usage: rm <path>").unwrap();
        (Vec::new(), 2)
    } else if let Some(rest) = cmd.strip_prefix("rm ") {
        rm_output(rest.trim(), out)
    } else if cmd == "cd" {
        // C-1 刀 5：裸 cd 无 HOME 概念（v1）——usage 面。
        writeln!(out, "usage: cd <dir>").unwrap();
        (Vec::new(), 2)
    } else if let Some(rest) = cmd.strip_prefix("cd ") {
        cd_output(rest.trim(), out)
    } else if cmd == "pwd" {
        // C-1 刀 5：cwd 渲染行（libc getcwd——进程级 CWD 静态，chdir 改）。
        let mut buf = [0u8; 160];
        // SAFETY: buf 可写、长度如参；返回值按 POSIX 检（null = 错）。
        let r = unsafe { getcwd(buf.as_mut_ptr() as *mut _, buf.len()) };
        if r.is_null() {
            // ERANGE 面 160B > PATH_MAX 渲染帽构造性不可达——到这即异常面。
            writeln!(out, "bsh: pwd: {}", errno_text(errno())).unwrap();
            (Vec::new(), 1)
        } else {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let mut d = buf[..len].to_vec();
            d.push(b'\n');
            (d, 0)
        }
    } else {
        writeln!(out, "bsh: {}: not found", cmd).unwrap();
        (Vec::new(), 127)
    }
}

/// `run <name>` 的渲染面（P6 A-1 拆 do_run——直出与 `>` 重定向共道）：
/// cpio 查 `bin/<name>` → 私有 stdout 对拉起（32 页栈 = musl 128KiB 口径，
/// 盖住 clap 级栈深）→ 收齐 EOF → **数据字节返回**交调用方定向；警告照打
/// `out`（stderr 语义）。子进程无 stdin（env 块单 gift，libc /dev/null 语义
/// ——read 即 EOF）。返回 (数据, 状态码)：127 not found / 126 spawn failed /
/// 2 usage / 其余 = 子真码（reap Err → 1——真码源 0x3A）。
fn run_output(name: &str, out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    if name.is_empty() || name.contains([' ', '/']) {
        writeln!(out, "usage: run <name>").unwrap();
        return (Vec::new(), 2);
    }
    let full = format!("bin/{}", name);
    let Some((data_off, data_len)) = cpio_find(rootfs_h(), full.as_bytes()) else {
        writeln!(out, "bsh: run: {}: not found", name).unwrap();
        return (Vec::new(), 127);
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
        extra_gifts: &[],
        mmio_vmo: None,
    };
    let (data, code) = if let Ok(child) = spawn_rootfs_elf(rootfs_h(), &spec) {
        let o = collect(out_s);
        if !o.eof {
            writeln!(out, "[bsh: run {}: no EOF (timeout)]", name).unwrap();
        }
        if o.dropped > 0 {
            writeln!(out, "[bsh: run {}: {}B dropped]", name, o.dropped).unwrap();
        }
        out.flush().unwrap();
        // Plan B：收完输出后回收子进程（挂死/帽尽 = 句柄留存有界泄漏，警告
        // 不死 shell——终验面在 svcmgr `EXIT ok`）。reap 返真退出码（P5）。
        let code = match child.reap() {
            Ok(c) => c,
            Err(()) => {
                writeln!(out, "[bsh: run {}: reap failed]", name).unwrap();
                out.flush().unwrap();
                1
            }
        };
        (o.data, code)
    } else {
        // Err = ELF 坏或资源耗尽（-ENOMEM 降级，PROCESS_MAX 出帽不杀 shell）。
        writeln!(out, "bsh: run: {}: spawn failed", name).unwrap();
        (Vec::new(), 126)
    };
    // out_c 原值：子表已持 SEND 别名（spawn 内 dup 迁移），本表副本使命已毕
    // （svcmgr verify_hexyl 同款）。
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        berry_libc::park(); // 关不掉 = 泄漏面，loud 死（不 trap：交互面不死 shell）
    }
    (data, code)
}

// —— fs 面（P6 A-1）：`>` 重定向 + cat/ls 内建。C-1 刀 4 起全量走
// berry-libc 文件面（fd 3+ 客户端游标 + close 即 commit 两裁决，RPC/
// 会话机在 berry-fsclient/libc——会话毒化/双帽语义在那侧逐字保真）——

/// errno 读（TLS 直读——失败返回后**立即**取，中间不夹其它可能写 errno
/// 的调用；诊断/状态码两用）。
fn errno() -> i32 {
    // SAFETY: __errno_location 返回本线程 TLS errno 槽（libc 契约）。
    unsafe { *berry_libc::sys::__errno_location() }
}

/// errno → 诊断文本（v1 `FsErr::text` 逐字同形——零漂移）：NoFs 面
/// （fs_h()==0 预检）不在此表（独立常量「fs not available」，见各内建）；
/// ENOENT 覆盖 Noent；EINVAL 覆盖 Bad（libc 把 ST_BAD 与客户端坏路径同报
/// EINVAL——usage 预检已把 bsh 侧坏路径先拦，到这里的 EINVAL 即服务面）；
/// EIO 覆盖 Rpc（断链族）；未见面按断链族显示。
fn errno_text(e: i32) -> &'static str {
    match e {
        ENOENT => "not found",
        EISDIR | ENOTDIR | EINVAL => "bad path",
        ENOSPC => "no space",
        EEXIST => "exists",
        ENOTEMPTY => "dir not empty",
        _ => "fs unreachable",
    }
}

/// errno → 状态码（v1 `FsErr::code` 同值：ENOENT=127 not found，其余 1）。
fn errno_code(e: i32) -> i32 {
    if e == ENOENT {
        127
    } else {
        1
    }
}

/// shell 路径合法面（usage 预检）：段字符集白名单 `[A-Za-z0-9._-]`（无
/// 空白/`$`/`/`——未引号 token 面，引号随 v2）+ 帽 NAME_MAX/段、
/// PATH_DEPTH_MAX 段数、PATH_MAX 总长。与 libc（berry-fsclient
/// `resolve_path`）/ fsproto `path_valid` 双侧同步纪律第三镜：bsh 侧镜像
/// 只为保「坏路径 usage 2 vs 服务错误 1」的旧区分零漂移（EINVAL 双义——
/// 客户端坏路径 vs 服务 ST_BAD——在显示层解歧）。
fn valid_path(s: &str) -> bool {
    let mut segs = 0usize;
    let mut total = 0usize;
    for seg in s.split('/') {
        segs += 1;
        if segs > PATH_DEPTH_MAX {
            return false;
        }
        if seg.is_empty()
            || seg.len() > NAME_MAX
            || !seg
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
        {
            return false;
        }
        total += seg.len();
    }
    total <= PATH_MAX
}

/// 路径 → NUL 结尾 C 串（`valid_path` 已过——无内嵌 NUL/超长；libc 面
/// 恒收 C 串）。
fn cpath(path: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(path.len() + 1);
    v.extend_from_slice(path.as_bytes());
    v.push(0);
    v
}

/// 全量读（cat/自验读面）：open(O_RDONLY) → read 循环至 EOF → close。
/// Err(errno)：open 失败（Noent→ENOENT / IsDir→EISDIR——libc open_core
/// 已检）或读失败；累计超 FILE_SIZE_MAX = 失控链兜底（服务端同帽，防御
/// 面——文本落「fs unreachable」族）。读 fd 的 close 无 commit 面（错误
/// 照 close 恒消费——fd 表槽回收优先）。
fn get_file(path: &str) -> Result<Vec<u8>, i32> {
    let p = cpath(path);
    // SAFETY: p 是 NUL 结尾路径串；fd 按 libc 契约使用。
    let fd = unsafe { open(p.as_ptr() as *const _, O_RDONLY) };
    if fd < 0 {
        return Err(errno());
    }
    let mut data: Vec<u8> = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        // read 是安全 fn（io.rs——fd≥3 走 fs::read_fd）；<0 错 / 0 EOF。
        let n = read(fd, buf.as_mut_ptr() as *mut _, buf.len());
        if n < 0 {
            let e = errno();
            close(fd);
            return Err(e);
        }
        if n == 0 {
            break; // EOF（libc read_fd：EOF 不前移游标——close 幂等）
        }
        data.extend_from_slice(&buf[..n as usize]);
        if data.len() > FILE_SIZE_MAX {
            close(fd);
            return Err(EIO);
        }
    }
    close(fd);
    Ok(data)
}

/// 全量写（`>`/自验写面）：open(O_WRONLY|O_CREAT) → write 循环（libc
/// write_fd 内部按 192B 分片 PUT）→ **close 即 commit**（TRUNC(游标)——
/// C-1 裁决 2，与 v1「分片 PUT + 尾随 TRUNC」同款落盘点）。中途写失败仍
/// close（POSIX close 恒消费——部分数据照旧提交；v1 失败面 TRUNC 不发、
/// 文件同为部分态，边缘同形）。空数据 = 零 write + close = 建空文件面
/// （`echo > f`）。O_TRUNC 省略 = 覆写幂等（v1 同形：PUT off 0 覆写 +
/// TRUNC(len) 截到新长——旧长于新时残留被 TRUNC 截断，语义同 v1）。
fn put_file(path: &str, data: &[u8]) -> Result<(), i32> {
    let p = cpath(path);
    // SAFETY: 同上。
    let fd = unsafe { open(p.as_ptr() as *const _, O_WRONLY | O_CREAT) };
    if fd < 0 {
        return Err(errno());
    }
    let mut off = 0usize;
    while off < data.len() {
        // write 安全 fn（io.rs——fd≥3 走 fs::write_fd 内部分片）；部分写按
        // 返回量续写。
        let n = write(fd, data[off..].as_ptr() as *const _, data.len() - off);
        if n < 0 {
            let e = errno();
            close(fd); // 部分 commit（文档化取舍）
            return Err(e);
        }
        if n == 0 {
            close(fd);
            return Err(EIO); // 零写非 EOF = 协议破缺面
        }
        off += n as usize;
    }
    // close 安全 fn（io.rs——fd≥3 走 fs::close_fd）：出错 = commit（TRUNC）
    // 失败面——errno 透传。
    if close(fd) != 0 {
        return Err(errno());
    }
    Ok(())
}

/// 目录列名（裸 `ls`/`ls <dir>`）：opendir → readdir 迭代 → 每名一行 →
/// closedir。Err(errno)：opendir 失败（Noent→ENOENT / 文件目标→ENOTDIR
/// /目录链断→EINVAL——libc opendir 契约）；**目录尾 null 无 errno 语义**
/// （libc 契约——直调 berry-libc 而非 std::fs::read_dir 的原因：std 尾检
/// 胶水的 stale-errno 面不可证，两个消费方共用同一条确定性的 libc 路）。
fn dir_lines(path: &str) -> Result<Vec<u8>, i32> {
    let p = cpath(path);
    // SAFETY: p 同上；DIR 指针按 libc 目录族契约使用。
    let d = unsafe { opendir(p.as_ptr() as *const _) };
    if d.is_null() {
        return Err(errno());
    }
    let mut out = Vec::new();
    loop {
        // SAFETY: 返回指向 DIR 内部 Dirent（下次调用前有效——即拷即用）；
        // null = 目录尾。
        let ent = unsafe { readdir(d) };
        if ent.is_null() {
            break;
        }
        // SAFETY: ent 有效；d_name 为 NUL 结尾名区，名长按 NUL 定界。
        let name = unsafe { &(*ent).d_name };
        let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        out.extend_from_slice(&name[..len]);
        out.push(b'\n');
    }
    unsafe { closedir(d) };
    Ok(out)
}

/// 单点 stat（自验用）：Ok((is_dir, size))——v1 fs_stat 的 (TYPE, n) 同义
/// （dir 的 size = 子项数，fstree 语义；libc fill_stat 同源）。S_IFMT/
/// S_IFDIR = aarch64 musl 同值（berry-fsclient `st_mode_of` 同源）。
fn stat_of(path: &str) -> Result<(bool, i64), i32> {
    const S_IFMT: u32 = 0o170000;
    const S_IFDIR: u32 = 0o040000;
    let p = cpath(path);
    // SAFETY: Stat 是纯数据 repr(C) 结构；stat 写 out、路径串只读。
    let mut st: Stat = unsafe { core::mem::zeroed() };
    if unsafe { stat(p.as_ptr() as *const _, &mut st) } != 0 {
        return Err(errno());
    }
    Ok((st.st_mode & S_IFMT == S_IFDIR, st.st_size))
}

/// mkdir 薄封装（内建/自验共用）：Err(errno)——Exists→EEXIST、帽满→
/// ENOSPC（fsmgr 纯 RAM 面，libc 同映射）。
fn mkdir_of(path: &str) -> Result<(), i32> {
    let p = cpath(path);
    // SAFETY: p 同上；mode 位被 fsmgr 忽略（0755 形参——POSIX 形面）。
    if unsafe { mkdir(p.as_ptr() as *const _, 0o755) } == 0 {
        Ok(())
    } else {
        Err(errno())
    }
}

/// unlink 薄封装（内建/自验共用）：文件或**空**目录（非空目录 →
/// ENOTEMPTY）。
fn unlink_of(path: &str) -> Result<(), i32> {
    let p = cpath(path);
    // SAFETY: p 同上。
    if unsafe { unlink(p.as_ptr() as *const _) } == 0 {
        Ok(())
    } else {
        Err(errno())
    }
}

/// `A > f` 切分：恰一个 `>`、两侧 trim、目标路径过 [`valid_path`]（刀 4
/// 起多段——`echo x > d1/Z.TXT`；单段与 v1 同形零漂移）。None = usage 面
/// （`>>`/多点 `>` 被计数排除；坏路径被段校验排除）。
fn split_redirect(raw: &str) -> Option<(&str, &str)> {
    if raw.matches('>').count() != 1 {
        return None;
    }
    let (a, b) = raw.split_once('>')?;
    let (cmd, file) = (a.trim(), b.trim());
    valid_path(file).then_some((cmd, file))
}

/// `A > f` 重定向（P6 A-1）：渲染命令（数据字节**不进 out**）→ put_file
/// 全量入文件（libc open/write/close——close 即 commit）。诊断/警告照打
/// out（stderr 语义）；fs 错误 → 警告 + errno 码。v1 文档化面：空命令
/// （展开后空）不建文件（POSIX `> f` 建空文件——v2）；命令失败仍落已产出
/// 的部分数据（先渲染后写，非 open-before-exec）。刀 4：目标可多段（RAM
/// 树写；FAT 名仍只落根——fsmgr 分层路由）。
fn do_redirect(
    raw: &str,
    vars: &[(String, String)],
    status: i32,
    out: &mut std::io::Stdout,
) -> i32 {
    let Some((cmd_s, file_s)) = split_redirect(raw) else {
        writeln!(out, "usage: <command> > <file>").unwrap();
        return 2;
    };
    if expand(cmd_s, vars, status).is_empty() {
        return 0; // v1：空命令不建文件（`> f` 裸重定向——文档化面）
    }
    let (data, code) = render_cmd(cmd_s, vars, status, out);
    out.flush().unwrap(); // 诊断先落（stderr 语义——数据进文件）
    if fs_h() == 0 {
        // NoFs 预检（libc 无 gift 面会把 open 降级 ENOENT——文本漂移，故
        // bsh 侧预检保「fs not available」旧文案）。
        writeln!(out, "bsh: {}: fs not available", file_s).unwrap();
        out.flush().unwrap();
        return 1;
    }
    match put_file(file_s, &data) {
        Ok(()) => code,
        Err(e) => {
            writeln!(out, "bsh: {}: {}", file_s, errno_text(e)).unwrap();
            out.flush().unwrap();
            errno_code(e)
        }
    }
}

/// `cat <path>`：get_file 全量 → 数据字节（fs 错误 → 诊断打 out + errno
/// 码）。刀 4 起多段（RAM 树 → FAT 子目录链，fsmgr 分层）；目录目标 →
/// libc open EISDIR（「bad path」——目录不是读面，v1 同文案）。
fn cat_output(name: &str, out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    if !valid_path(name) {
        writeln!(out, "usage: cat <file>").unwrap();
        return (Vec::new(), 2);
    }
    if fs_h() == 0 {
        writeln!(out, "bsh: cat: {}: fs not available", name).unwrap();
        return (Vec::new(), 1);
    }
    match get_file(name) {
        Ok(d) => (d, 0),
        Err(e) => {
            writeln!(out, "bsh: cat: {}: {}", name, errno_text(e)).unwrap();
            (Vec::new(), errno_code(e))
        }
    }
}

/// `ls`：dir_lines("/") → 每名一行（空表 = 空输出 0）。裸 `ls` 的根列名
/// = opendir("/") → fsmgr READDIR 根并合（RAM 根 + FAT 未影子名——与
/// v1 LIST 同源同序，C-1 刀 4 服务侧对齐）。
fn ls_output(out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    if fs_h() == 0 {
        writeln!(out, "bsh: ls: fs not available").unwrap();
        return (Vec::new(), 1);
    }
    match dir_lines("/") {
        Ok(d) => (d, 0),
        Err(e) => {
            writeln!(out, "bsh: ls: {}", errno_text(e)).unwrap();
            (Vec::new(), errno_code(e))
        }
    }
}

/// `ls <dir>`（刀 4）：READDIR 子名 → 每名一行（空目录 = 空输出 0）。
/// 文件目标 → libc opendir ENOTDIR（「bad path」——v1 同文案）。
fn ls_path_output(name: &str, out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    if !valid_path(name) {
        writeln!(out, "usage: ls <dir>").unwrap();
        return (Vec::new(), 2);
    }
    if fs_h() == 0 {
        writeln!(out, "bsh: ls: {}: fs not available", name).unwrap();
        return (Vec::new(), 1);
    }
    match dir_lines(name) {
        Ok(d) => (d, 0),
        Err(e) => {
            writeln!(out, "bsh: ls: {}: {}", name, errno_text(e)).unwrap();
            (Vec::new(), errno_code(e))
        }
    }
}

/// `mkdir <path>`（刀 4）：RAM 目录树（fsmgr 纯 RAM 面）；已存在 →
/// EEXIST。静默成功（POSIX 口径；失败才诊断）。
fn mkdir_output(name: &str, out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    if !valid_path(name) {
        writeln!(out, "usage: mkdir <dir>").unwrap();
        return (Vec::new(), 2);
    }
    if fs_h() == 0 {
        writeln!(out, "bsh: mkdir: {}: fs not available", name).unwrap();
        return (Vec::new(), 1);
    }
    match mkdir_of(name) {
        Ok(()) => (Vec::new(), 0),
        Err(e) => {
            writeln!(out, "bsh: mkdir: {}: {}", name, errno_text(e)).unwrap();
            (Vec::new(), errno_code(e))
        }
    }
}

/// `rm <path>`（刀 4）：UNLINK——文件或**空**目录（非空目录 → ENOTEMPTY；
/// 目录链上文件先逐个 rm）。静默成功（POSIX 口径）。
fn rm_output(name: &str, out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    if !valid_path(name) {
        writeln!(out, "usage: rm <path>").unwrap();
        return (Vec::new(), 2);
    }
    if fs_h() == 0 {
        writeln!(out, "bsh: rm: {}: fs not available", name).unwrap();
        return (Vec::new(), 1);
    }
    match unlink_of(name) {
        Ok(()) => (Vec::new(), 0),
        Err(e) => {
            writeln!(out, "bsh: rm: {}: {}", name, errno_text(e)).unwrap();
            (Vec::new(), errno_code(e))
        }
    }
}

/// `cd <dir>`（C-1 刀 5）：libc chdir（resolve 相对路径 + STAT 目录型检
/// ——文件目标 ENOTDIR「bad path」）。静默成功（POSIX 口径；失败才诊断）；
/// cwd 是 libc 进程级静态——后续 cat/`>` 的相对名都经它解析。
fn cd_output(name: &str, out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    if !valid_path(name) {
        writeln!(out, "usage: cd <dir>").unwrap();
        return (Vec::new(), 2);
    }
    if fs_h() == 0 {
        writeln!(out, "bsh: cd: {}: fs not available", name).unwrap();
        return (Vec::new(), 1);
    }
    let p = cpath(name);
    // SAFETY: p 是 NUL 结尾路径串；chdir 按 libc 契约使用。
    if unsafe { chdir(p.as_ptr() as *const _) } == 0 {
        (Vec::new(), 0)
    } else {
        let e = errno();
        writeln!(out, "bsh: cd: {}: {}", name, errno_text(e)).unwrap();
        (Vec::new(), errno_code(e))
    }
}

/// fs 自验（P6 A-1，`FS ok` = rootfs 相第 11 标记——确定性状态谓词）：
/// put_file 64B pattern（HEXYL_STDIN 前缀——单片路径）→ get_file 全等；
/// put_file 2156B（hexyl 首验输出——多片 + close 即 commit 的多片路径）→
/// get_file 逐字节全等（分片链全盖）。数据是 hexyl 输出的纯函数快照，无
/// 竞速面；失败 eprintln + park（标记缺失 → smoke FAIL 兜底）。
fn fs_selftest(hexyl_out: &[u8]) {
    let a = &HEXYL_STDIN[..64];
    if let Err(e) = put_file("t64.bin", a) {
        eprintln!("bsh: fs selftest: rw64 {}", errno_text(e));
        berry_libc::park();
    }
    match get_file("t64.bin") {
        Ok(d) if d == a => {}
        Ok(d) => {
            eprintln!("bsh: fs selftest: get64 len={} want={}", d.len(), a.len());
            berry_libc::park();
        }
        Err(e) => {
            eprintln!("bsh: fs selftest: rw64 {}", errno_text(e));
            berry_libc::park();
        }
    }
    if let Err(e) = put_file("h.hex", hexyl_out) {
        eprintln!("bsh: fs selftest: rw2156 {}", errno_text(e));
        berry_libc::park();
    }
    match get_file("h.hex") {
        Ok(d) if d == hexyl_out => {}
        Ok(d) => {
            eprintln!(
                "bsh: fs selftest: get2156 len={} want={}",
                d.len(),
                hexyl_out.len()
            );
            berry_libc::park();
        }
        Err(e) => {
            eprintln!("bsh: fs selftest: rw2156 {}", errno_text(e));
            berry_libc::park();
        }
    }
    println!("FS ok n={}", hexyl_out.len());
}

/// FAT 自验（P6 A-5，`FAT ok n=` = rootfs 相第 15 标记——确定性状态谓词）：
/// 走 libc 文件面（= 交互 `cat HELLO.TXT` 同一台机器）端到端读 mkfat 落盘
/// 卷——HELLO.TXT 28B 全等 + PATTERN.BIN 5000B 逐字节对拍（(i*31+7)%256，
/// mkfat/berry-fatfs fixture 同源 pattern）——FAT 兜底 → vblk 服务 RPC →
/// FAT12 簇链 + 分片链全盖。盘内容是 mkfat 确定性字节，无竞速面；失败
/// eprintln + park（标记缺失 → smoke FAIL 兜底）。
fn fat_selftest() {
    match get_file("HELLO.TXT") {
        Ok(d) if d == b"berryos fat read-only hello\n" => {}
        Ok(d) => {
            eprintln!("bsh: fat selftest: hello len={} want=28", d.len());
            berry_libc::park();
        }
        Err(e) => {
            eprintln!("bsh: fat selftest: hello {}", errno_text(e));
            berry_libc::park();
        }
    }
    match get_file("PATTERN.BIN") {
        Ok(d) => {
            if d.len() != 5000 {
                eprintln!("bsh: fat selftest: pattern len={} want=5000", d.len());
                berry_libc::park();
            }
            for (i, &b) in d.iter().enumerate() {
                if b as usize != (i * 31 + 7) % 256 {
                    eprintln!("bsh: fat selftest: pattern byte {i} mismatch");
                    berry_libc::park();
                }
            }
        }
        Err(e) => {
            eprintln!("bsh: fat selftest: pattern {}", errno_text(e));
            berry_libc::park();
        }
    }
    println!("FAT ok n=5000");
}

/// FAT 写自验（P6 A-6，`FATW ok n=` = rootfs 相第 16 标记——确定性状态
/// 谓词）：RW.TXT 3000B pattern（(i*17+3)%256，跨 2 簇）经 put_file 落盘
/// （fsmgr staging + close 即 commit 真写扇区）→ get_file 逐字节对拍才自打。
/// 每次 boot 重写 = 覆写路径幂等（旧链 collect → 新链分配 → 旧链释放）；
/// HELLO.TXT/PATTERN.BIN 是 mkfat 只读基准，本验永不触碰。失败 eprintln +
/// park（标记缺失 → smoke FAIL 兜底）。
fn fatw_selftest() {
    let data: Vec<u8> = (0..3000u32).map(|i| ((i * 17 + 3) % 256) as u8).collect();
    match put_file("RW.TXT", &data).and_then(|_| get_file("RW.TXT")) {
        Ok(d) => {
            if d.len() != 3000 {
                eprintln!("bsh: fatw selftest: len={} want=3000", d.len());
                berry_libc::park();
            }
            for (i, &b) in d.iter().enumerate() {
                if b as usize != (i * 17 + 3) % 256 {
                    eprintln!("bsh: fatw selftest: byte {i} mismatch");
                    berry_libc::park();
                }
            }
        }
        Err(e) => {
            eprintln!("bsh: fatw selftest: {}", errno_text(e));
            berry_libc::park();
        }
    }
    println!("FATW ok n=3000");
}

/// 目录自验（C-0 刀 4，`DIR ok n=` = rootfs 相第 22 标记——确定性状态
/// 谓词）：libc 目录/stat 面 + 多段读写全链 RAM 面——mkdir `bdir` →
/// 多段写（`bdir/n.bin` 64B pattern）→ 多段读逐字节全等 → READDIR 恰一
/// 名 → stat 双面（目录=true 子项数 1 / 文件=false 64B）→ unlink 文件 →
/// 空目录 READDIR → unlink 空目录 → stat ENOENT 收口。读面全 RAM 命中
/// （`bdir` 在树上——不触 FAT 链走查；FAT 子目录 QEMU 活覆盖在 smoke
/// FAT 子目录相注入段 `cat DATA/NOTE.TXT`/`ls DATA`）。失败 eprintln +
/// park（标记缺失 → smoke FAIL 兜底）。
fn dir_selftest() {
    let a = &HEXYL_STDIN[..64];
    if let Err(e) = mkdir_of("bdir") {
        eprintln!("bsh: dir selftest: mkdir {}", errno_text(e));
        berry_libc::park();
    }
    if let Err(e) = put_file("bdir/n.bin", a) {
        eprintln!("bsh: dir selftest: rw64 {}", errno_text(e));
        berry_libc::park();
    }
    match get_file("bdir/n.bin") {
        Ok(d) if d == a => {}
        Ok(d) => {
            eprintln!("bsh: dir selftest: get64 len={} want=64", d.len());
            berry_libc::park();
        }
        Err(e) => {
            eprintln!("bsh: dir selftest: rw64 {}", errno_text(e));
            berry_libc::park();
        }
    }
    match dir_lines("bdir") {
        Ok(d) if d == b"n.bin\n" => {}
        r => {
            eprintln!("bsh: dir selftest: readdir {:?}", r.map(|v| v.len()));
            berry_libc::park();
        }
    }
    match stat_of("bdir") {
        Ok((true, 1)) => {}
        r => {
            eprintln!("bsh: dir selftest: stat dir {:?}", r);
            berry_libc::park();
        }
    }
    match stat_of("bdir/n.bin") {
        Ok((false, 64)) => {}
        r => {
            eprintln!("bsh: dir selftest: stat file {:?}", r);
            berry_libc::park();
        }
    }
    // UNLINK 双面：先文件（目录转空——READDIR 空证）→ 空目录可删 → stat
    // ENOENT 收口（存在性终查与 v1 LOOKUP Noent 同义）。
    if let Err(e) = unlink_of("bdir/n.bin") {
        eprintln!("bsh: dir selftest: unlink file {}", errno_text(e));
        berry_libc::park();
    }
    match dir_lines("bdir") {
        Ok(d) if d.is_empty() => {}
        r => {
            eprintln!("bsh: dir selftest: readdir empty {:?}", r.map(|v| v.len()));
            berry_libc::park();
        }
    }
    if let Err(e) = unlink_of("bdir") {
        eprintln!("bsh: dir selftest: unlink dir {}", errno_text(e));
        berry_libc::park();
    }
    match stat_of("bdir") {
        Err(e) if e == ENOENT => {}
        r => {
            eprintln!("bsh: dir selftest: lookup gone {:?}", r);
            berry_libc::park();
        }
    }
    println!("DIR ok n=64");
}

/// cd 自验（C-1 刀 5，`CD ok` = rootfs 相第 24 标记——确定性状态谓词）：
/// libc cwd 族全链——mkdir 沙箱 `bdir2` → chdir 进去 → pwd=="/bdir2" →
/// **相对名**写 `n2.txt`（libc resolve 挂 CWD → 落进 bdir2）→ **CWD 内
/// 相对读回** `n2.txt` 全等（写落点铁证）→ chdir "/" → **回根后多段**
/// `bdir2/n2.txt` 全等（同一文件两路寻址）→ 回根后相对名 `n2.txt` 即失
/// （ENOENT——CWD 真迁移的铁证，非仅 pwd 文本）→ 清场回根。失败
/// eprintln + park（标记缺失 → smoke FAIL 兜底）。
fn cd_selftest() {
    if let Err(e) = mkdir_of("bdir2") {
        eprintln!("bsh: cd selftest: mkdir {}", errno_text(e));
        berry_libc::park();
    }
    let enter = |dir: &str| {
        let p = cpath(dir);
        // SAFETY: p 是 NUL 结尾路径串；chdir 按 libc 契约使用。
        if unsafe { chdir(p.as_ptr() as *const _) } != 0 {
            let e = errno();
            eprintln!("bsh: cd selftest: chdir {} {}", dir, errno_text(e));
            berry_libc::park();
        }
    };
    enter("bdir2");
    if !pwd_is("/bdir2") {
        eprintln!("bsh: cd selftest: pwd != /bdir2");
        berry_libc::park();
    }
    if let Err(e) = put_file("n2.txt", b"cd\n") {
        eprintln!("bsh: cd selftest: rw {}", errno_text(e));
        berry_libc::park();
    }
    // CWD 内相对读回（写落点铁证——此刻 CWD=/bdir2，相对名即 /bdir2/n2.txt）。
    match get_file("n2.txt") {
        Ok(d) if d == b"cd\n" => {}
        r => {
            eprintln!(
                "bsh: cd selftest: get-cwd {:?}",
                r.map(|v| v.len()).map_err(|e| errno_text(e))
            );
            berry_libc::park();
        }
    }
    enter("/");
    if !pwd_is("/") {
        eprintln!("bsh: cd selftest: pwd != /");
        berry_libc::park();
    }
    // 根 opendir（opendir("/") → READDIR 空 wire 根并合——刀 4 语义在
    // 刀 5 fsproto 立法后才真可达，此断言 = 首个 QEMU 活验）。
    match dir_lines("/") {
        Ok(d) if d.windows(5).any(|w| w == b"bdir2") => {}
        r => {
            eprintln!(
                "bsh: cd selftest: root-ls {:?}",
                r.map(|v| v.len()).map_err(|e| errno_text(e))
            );
            berry_libc::park();
        }
    }
    // 回根后多段寻址同一文件（两路寻址全等——CWD 回空 wire，`bdir2/n2.txt`
    // 从根解析；首版把多段名当绝对路径在 CWD=/bdir2 下查——解析成
    // /bdir2/bdir2/n2.txt 而 FAIL，实测教训：多段 ≠ 绝对）。
    match get_file("bdir2/n2.txt") {
        Ok(d) if d == b"cd\n" => {}
        r => {
            eprintln!(
                "bsh: cd selftest: get-root {:?}",
                r.map(|v| v.len()).map_err(|e| errno_text(e))
            );
            berry_libc::park();
        }
    }
    match get_file("n2.txt") {
        Err(e) if e == ENOENT => {}
        r => {
            eprintln!(
                "bsh: cd selftest: relative miss {:?}",
                r.map(|v| v.len()).map_err(|e| errno_text(e))
            );
            berry_libc::park();
        }
    }
    if let Err(e) = unlink_of("bdir2/n2.txt") {
        eprintln!("bsh: cd selftest: unlink file {}", errno_text(e));
        berry_libc::park();
    }
    if let Err(e) = unlink_of("bdir2") {
        eprintln!("bsh: cd selftest: unlink dir {}", errno_text(e));
        berry_libc::park();
    }
    println!("CD ok");
}

/// pwd 全等探针（自验用）：getcwd 渲染串 == want。
fn pwd_is(want: &str) -> bool {
    let mut buf = [0u8; 160];
    // SAFETY: buf 可写、长度如参；null = 错（本探针只论真伪不诊断）。
    let r = unsafe { getcwd(buf.as_mut_ptr() as *mut _, buf.len()) };
    if r.is_null() {
        return false;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    &buf[..len] == want.as_bytes()
}

/// `A | B | …` 交互面（v1：各段裸名——无参数无 `/`，参数面随 v2 argv 展开）。
/// 拉起/收集/回收全在 [`pipe_capture`]，此处只解析段 + 回放 + 警告。
/// **不死 shell 的边界（对抗审查 F3）**：PipeFail（not found / spawn failed
/// ——含 -ENOMEM 降级）与 EOF/dropped 警告面照打续走；**不退出段**（如
/// `hello | …`，hello 10s 周期睡眠永循环）= 10s collect 帽 + 30s reap 帽/段
/// 的可见冻结 + 僵尸留存（v1 无 kill syscall，PROCESS_MAX=64 出帽时新管
/// spawn failed 拒绝但 shell 活）——kill/监督随 v2。
fn do_pipe(cmd: &str, vars: &[(String, String)], status: i32, out: &mut std::io::Stdout) -> i32 {
    // 段名执行时展开（`$X | hexyl` 的 X 也是数据面）；展开注入的 `|`/空格
    // 只会撞下方校验或 not found——不当语法（切段已在 exec_one 的原始文本
    // 上完成）。
    let owned: Vec<String> = cmd.split('|').map(|p| expand(p.trim(), vars, status)).collect();
    let mut segs: Vec<PipeSeg> = Vec::new();
    for name in owned.iter() {
        let name = name.trim();
        if name.is_empty() || name.contains([' ', '/', '|']) {
            writeln!(out, "usage: <name> | <name>（rootfs bin/ 裸名）").unwrap();
            return 2;
        }
        segs.push(PipeSeg { name, arg: None });
    }
    let code = match pipe_capture(&segs, None) {
        Ok((o, c)) => {
            out.write_all(&o.data).unwrap();
            if !o.eof {
                writeln!(out, "[bsh: pipe: no EOF (timeout)]").unwrap();
            }
            if o.dropped > 0 {
                writeln!(out, "[bsh: pipe: {}B dropped]", o.dropped).unwrap();
            }
            c
        }
        Err(PipeFail::NotFound(n)) => {
            writeln!(out, "bsh: pipe: {}: not found", n).unwrap();
            127
        }
        Err(PipeFail::SpawnFail(n)) => {
            writeln!(out, "bsh: pipe: {}: spawn failed（先前段有界泄漏）", n).unwrap();
            126
        }
    };
    out.flush().unwrap();
    code
}

/// 管道段：程序名 + argv[1]（自验传 `--color=never`，交互 v1 裸名 None）。
struct PipeSeg<'a> {
    name: &'a str,
    arg: Option<&'a str>,
}

/// 管道失败面（调用方提示用）。
enum PipeFail {
    /// 某段 cpio 未命中（fail-fast：任何 spawn 之前，零泄漏）。
    NotFound(String),
    /// 某段拉起失败（ELF 坏 / **资源耗尽 -ENOMEM**——spawn 内 Err 降级，
    /// PROCESS_MAX 出帽反杀 shell 的旧路已断，P5 v0.5 对抗审查）。中链失败：
    /// 先前段有界泄漏，见 [`pipe_capture`]。
    SpawnFail(String),
}

/// `A | B | …` 拉起 + 末段 stdout 收集（`shell设计.md` §2.2 channel 即 pipe）：
/// 相邻段间 [`channel_pair`]——前段 stdout = 写端（spawn 内 dup SEND 别名作
/// gift[0]）、后段 stdin = 读端（`stdin_handle`，被 env send take 迁进子表）；
/// 末段 stdout = 采集对。EOF 链 = 消息层约定（d-3b）：前段 exit → libc
/// `send_stdout_eof` → 后段 read 收 0 → 逐级传导 → 末段 EOF 上采集端。
///
/// **顺序纪律：先 collect 末段再 reap 全部**——反序死锁：前段堵满管（无
/// 读者时 WRITABLE 永不来）、末段堵满采集队（父未收）时，父先 reap 前段
/// = 三方互等（v1 无对端关闭检测，30s 帽白耗 × 段数）。
///
/// 全段 `cpio_find` fail-fast 在任何 spawn 之前（常见错误 = 名字拼错，不该
/// 拉起半条链才失败）。中链 spawn Err：失败段零副作用（Err 早退点 = 解析/
/// 几何前置检 + process_create -ENOMEM，均无已建对象、未 take 任何
/// handle），先前段写无读者管道 = 永堵（libc 写侧 10s 重臂）——跳过 30s
/// reap 白等，关本表副本，有界泄漏交调用方警告。
fn pipe_capture(
    segs: &[PipeSeg],
    first_feed: Option<&[u8]>,
) -> Result<(RunOut, i32), PipeFail> {
    // fail-fast：全段查档（拼错名 = 零 spawn 零泄漏）。
    let mut found: Vec<(String, (u64, u64))> = Vec::with_capacity(segs.len());
    for s in segs {
        let full = format!("bin/{}", s.name);
        match cpio_find(rootfs_h(), full.as_bytes()) {
            Some(x) => found.push((full, x)),
            None => return Err(PipeFail::NotFound(s.name.to_string())),
        }
    }
    let last = segs.len() - 1;
    let (out_s, out_c) = channel_pair();
    // pipes[i] = 段 i 与段 i+1 之间的通道对（(写端, 读端)）。
    let mut pipes: Vec<(Handle, Handle)> = Vec::with_capacity(last);
    for _ in 0..last {
        pipes.push(channel_pair());
    }
    let mut children: Vec<ChildHandles> = Vec::with_capacity(segs.len());
    for (i, (full, (data_off, data_len))) in found.iter().enumerate() {
        let spec = SpawnSpec {
            name: full.as_bytes(),
            data_off: *data_off,
            data_len: *data_len,
            stdout: if i == last { out_c } else { pipes[i].0 },
            stack_pages: 32, // musl 128KiB 口径（hexyl 级栈深，d-3c）
            stdin_feed: if i == 0 { first_feed } else { None },
            stdin_handle: if i > 0 { Some(pipes[i - 1].1) } else { None },
            extra_arg: segs[i].arg.map(|a| a.as_bytes()),
            extra_gifts: &[],
            mmio_vmo: None,
        };
        match spawn_rootfs_elf(rootfs_h(), &spec) {
            Ok(c) => children.push(c),
            Err(_) => {
                // 失败段无子进程；读端中只有本段待用的 pipes[i-1].1 未被
                // take（其余已被后段 spawn 迁走，close 返 -EBADF 忽略之）。
                let _ = berry_handle_close(out_c);
                let _ = berry_handle_close(out_s);
                for (w, r) in &pipes {
                    let _ = berry_handle_close(*w);
                    let _ = berry_handle_close(*r);
                }
                return Err(PipeFail::SpawnFail(segs[i].name.to_string()));
            }
        }
    }
    // 本表写端副本清场：子持各自的 SEND 别名，本表原值使命已毕（EOF 是
    // 消息层约定，不依赖关闭）；读端已被 take（原值作废，勿关）。
    for (w, _) in &pipes {
        if berry_handle_close(*w) != 0 {
            berry_libc::park(); // 关不掉 = 表序错乱，loud（标记缺失兜底）
        }
    }
    // 死锁序铁律：先收末段 stdout（EOF 链逐级传到采集端）……
    let o = collect(out_s);
    // ……再逐段 reap（此时全员应已退出，30s 帽只兜挂死面；失败 = 句柄
    // 留存有界泄漏 + 子进程僵尸（v1 无 kill，见 do_pipe 边界注），警告不死
    // shell——终验面在 svcmgr `EXIT ok`）。末段 reap Ok = 管道状态码（P5
    // `&&`/`$?` 真码源；reap Err → 1）。
    let mut code = 1;
    for (i, c) in children.iter().enumerate() {
        match c.reap() {
            Ok(cc) => {
                if i == last {
                    code = cc;
                }
            }
            Err(()) => {
                eprintln!("bsh: pipe: seg {} reap failed (leak)", i);
                if i == last {
                    code = 1;
                }
            }
        }
    }
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        berry_libc::park();
    }
    Ok((o, code))
}

/// 管道自验（三跑对拍，`PIPE ok` = rootfs 相第 10 标记）：
/// 输入 = 64B pattern（HEXYL_STDIN 前缀切片）；跑 1 = 直喂 hexyl 得 a1
/// （管道 B 段的理论输入，~659B）；跑 2 = `hexyl | hexyl` 管道采集；跑 3 =
/// 同字节直喂参照 `run_capture(a1)`。管道把 A 的 stdout 原样递到 B 的
/// stdin ⟺ 跑 2 == 跑 3 **逐字节全等**（hexyl 输出是输入字节的纯函数 +
/// isatty 恒 false——确定性，无竞速面）。
///
/// **输入刻意取 64B（对抗审查 F2）**：spawn 内联 feeder 与调用方 collect
/// 不并发——参照腿直喂的字节数逼近互锁门限（hexyl 8KB BufWriter 下
/// L≥2689B 确定性死锁，见 berry-exec feeder 注释）。64B → a1≈659B =
/// 4 片 + EOF < 8 队深，**结构性不进 feeder 阻塞分支**（不靠 hexyl 消费
/// 速率的巧合余量）；管道 A→B 段仍走多片背压（659B = 4 条消息）。
/// 双 eof + 零 dropped + 非空 + 全等才打标（单消息原子标记）；失败
/// eprintln 诊断 + park（标记缺失 loud 兜底）。
fn pipe_selftest() {
    let pat = &HEXYL_STDIN[..64];
    let a1 = match run_capture("hexyl", Some(pat), Some("--color=never")) {
        Some((o, _)) if o.eof && o.dropped == 0 && !o.data.is_empty() => o.data,
        Some((o, _)) => {
            eprintln!(
                "bsh: pipe selftest: ref input bad: eof={} len={} dropped={}",
                o.eof,
                o.data.len(),
                o.dropped
            );
            berry_libc::park();
        }
        None => {
            eprintln!("bsh: pipe selftest: hexyl not found");
            berry_libc::park();
        }
    };
    let segs = [
        PipeSeg { name: "hexyl", arg: Some("--color=never") },
        PipeSeg { name: "hexyl", arg: Some("--color=never") },
    ];
    let piped = match pipe_capture(&segs, Some(pat)) {
        Ok((o, _)) => o,
        Err(PipeFail::NotFound(n)) => {
            eprintln!("bsh: pipe selftest: {} not found", n);
            berry_libc::park();
        }
        Err(PipeFail::SpawnFail(n)) => {
            eprintln!("bsh: pipe selftest: {} spawn failed", n);
            berry_libc::park();
        }
    };
    let out3 = match run_capture("hexyl", Some(&a1), Some("--color=never")) {
        Some((o, _)) => o,
        None => {
            eprintln!("bsh: pipe selftest: ref run failed");
            berry_libc::park();
        }
    };
    let ok = piped.eof
        && out3.eof
        && piped.dropped == 0
        && out3.dropped == 0
        && !piped.data.is_empty()
        && piped.data == out3.data;
    if !ok {
        eprintln!(
            "bsh: pipe selftest bad: eof={}/{} dropped={}/{} len={}/{}",
            piped.eof,
            out3.eof,
            piped.dropped,
            out3.dropped,
            piped.data.len(),
            out3.data.len()
        );
        berry_libc::park();
    }
    println!("PIPE ok n={}", piped.data.len());
}

/// 拉起 + 收集（自验/交互共用）：查档 → spawn → collect → 回收 → 关对端。
/// None = 档未命中（调用方 not found）；Some((RunOut, code)) 的 code = 子
/// 真退出码（reap Err → 1——P5 `&&`/`$?` 真码源；spawn Err 仍 park = 自验
/// 面 loud 死，交互 not-found 面在 do_run 不经此）。
fn run_capture(name: &str, stdin: Option<&[u8]>, extra_arg: Option<&str>) -> Option<(RunOut, i32)> {
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
        extra_gifts: &[],
        mmio_vmo: None,
    };
    let child = match spawn_rootfs_elf(rootfs_h(), &spec) {
        Ok(c) => c,
        Err(_) => berry_libc::park(), // 自验面：链断 loud（标记缺失兜底）
    };
    let o = collect(out_s);
    // Plan B：reap 失败 ≠ run 链断（终验面在 svcmgr `EXIT ok`）——诊断可见
    // 即可，BSH ok 语义仍只锚定输出面；真码面 Err → 1（P5）。
    let code = match child.reap() {
        Ok(c) => c,
        Err(()) => {
            eprintln!("bsh: selftest: reap failed");
            1
        }
    };
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        berry_libc::park();
    }
    Some((o, code))
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
