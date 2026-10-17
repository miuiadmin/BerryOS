//! bsh —— P5 A-2 交互 shell（`文档/shell设计.md` v0 最小面 + v0.5 管道，
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
//! stdin）+ 全链 EOF 到齐才打标。`>` 重定向依赖 FS（/svc/fs），随 P6。
//!
//! v1 内建：help / echo / env / run / exit（Plan B v0.5：`exit` 经 std::process::exit
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

use berry_libc::io::rootfs_h;
use berry_sys::{
    berry_channel_recv, berry_clock_read, berry_handle_close, berry_object_wait_one, Handle,
    BERRY_ERR_AGAIN, CLOCK_MONOTONIC, SIGNAL_READABLE,
};
use berry_exec::{channel_pair, cpio_find, spawn_rootfs_elf, ChildHandles, SpawnSpec};

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
    if let Some((o, _)) = run_capture("hexyl", Some(&HEXYL_STDIN), Some("--color=never")) {
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
    } else {
        eprintln!("bsh: selftest: hexyl not found");
        berry_libc::park();
    }
    println!("BSH ok");
    // —— P5 v0：变量/状态壳（`&&` 自验与交互循环共享——`$?` 跨行语义）。
    let mut vars: Vec<(String, String)> = Vec::new();
    let mut status: i32 = 0;
    and_selftest(&mut vars, &mut status);
    pipe_selftest();

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
/// `|` 管道检测（**原始文本**——展开结果只是数据、永不产操作符）→ 展开
/// （用**进入时**的 `$?`）→ 分派。返回段状态码：0 成功 / 2 usage / 126
/// spawn failed / 127 not found / run|pipe = 子进程真码（reap Err → 1）。
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
    let cmd = expand(raw, vars, *status);
    if cmd.is_empty() {
        return 0;
    }
    let code = if cmd == "help" {
        writeln!(
            out,
            "bsh 内建：help | echo <text> | env | run <name> | A | B（rootfs 管道）| A && B | exit"
        )
        .unwrap();
        0
    } else if cmd == "exit" {
        out.flush().unwrap();
        std::process::exit(0); // → libc exit → EOF + berry_process_exit(0x33)
    } else if cmd == "env" {
        // P5 v0：变量表回显（插入序；覆盖写的旧值也列——get 取最新，回显
        // 面向人不做去重）。
        for (k, v) in vars.iter() {
            writeln!(out, "{}={}", k, v).unwrap();
        }
        0
    } else if let Some(text) = cmd.strip_prefix("echo ") {
        writeln!(out, "{}", text.trim()).unwrap();
        0
    } else if cmd == "echo" {
        writeln!(out).unwrap();
        0
    } else if cmd == "run" {
        writeln!(out, "usage: run <name>").unwrap();
        2
    } else if let Some(rest) = cmd.strip_prefix("run ") {
        // 词边界：`runfoo` 是未知命令（127），不是 `run foo`（对抗审查实抓
        // ——echo 分支同款双边界：`run ` 带空格才算 run + 参数）。
        do_run(rest.trim(), out)
    } else {
        writeln!(out, "bsh: {}: not found", cmd).unwrap();
        127
    };
    out.flush().unwrap();
    code
}

/// `run <name>`：cpio 查 `bin/<name>` → 私有 stdout 对拉起（32 页栈 = musl
/// 128KiB 口径，盖住 clap 级栈深）→ 收齐 EOF → 回放 console。
/// 子进程无 stdin（env 块单 gift，libc /dev/null 语义——read 即 EOF）。
/// 返回状态码：127 not found / 126 spawn failed / 2 usage / 其余 = 子真码
/// （reap Err → 1——真码源 0x3A）。
fn do_run(name: &str, out: &mut std::io::Stdout) -> i32 {
    if name.is_empty() || name.contains([' ', '/']) {
        writeln!(out, "usage: run <name>").unwrap();
        return 2;
    }
    let full = format!("bin/{}", name);
    let Some((data_off, data_len)) = cpio_find(rootfs_h(), full.as_bytes()) else {
        writeln!(out, "bsh: run: {}: not found", name).unwrap();
        return 127;
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
    let code = if let Ok(child) = spawn_rootfs_elf(rootfs_h(), &spec) {
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
        // 不死 shell——终验面在 svcmgr `EXIT ok`）。reap 返真退出码（P5）。
        match child.reap() {
            Ok(c) => c,
            Err(()) => {
                writeln!(out, "[bsh: run {}: reap failed]", name).unwrap();
                out.flush().unwrap();
                1
            }
        }
    } else {
        // Err = ELF 坏或资源耗尽（-ENOMEM 降级，PROCESS_MAX 出帽不杀 shell）。
        writeln!(out, "bsh: run: {}: spawn failed", name).unwrap();
        126
    };
    // out_c 原值：子表已持 SEND 别名（spawn 内 dup 迁移），本表副本使命已毕
    // （svcmgr verify_hexyl 同款）。
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        berry_libc::park(); // 关不掉 = 泄漏面，loud 死（不 trap：交互面不死 shell）
    }
    code
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
            extra_gift: None,
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
        extra_gift: None,
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
