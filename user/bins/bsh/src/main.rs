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
//! C-2b（uutils U2 管道面）：uu echo/true/false 三命令**执行面**自验
//! `U2 ok`（rootfs 相第 26 标记）——argv 多参（输出 `berry u2\n` 9B 恰长）/`-n`
//! 无尾换行/true-false 真退出码/`&&` 链真码传播/`echo | cat` 管道对拍六
//! 步，全在私有通道收齐逐字节比对（零串口 grep 面；U1 = fs 元数据链，
//! 本刀补执行面——互补不重叠）。
//!
//! C-4b（Ctrl-C kill 面，`INT ok` = rootfs 相第 34 标记）：0x03 经 console
//! 逐字节透传到 stdin（C-4 纯缓冲纪律不破——识别在 bsh，console 零改动）。
//! 两路径互斥不重入：**feed 循环期**（恒无前台子进程）→ editor `Interrupt`
//! 作废当前行（清行 + 不入历史 + "^C" 回显 + prompt 重显）；**前台等待期**
//! （run/pipe 的 collect 窗）→ [`collect_fg`] wait_many 三项
//! [out:READABLE, proc:TERMINATED, stdin:READABLE]，0x03 到达即
//! `process_terminate(130)`（POSIX 128+SIGINT 惯例）+ wait TERMINATED →
//! reap → `$?` = 130。**EOF 缺口（v1 边界）**：terminate 是内核杀不走
//! libc exit 族（EOF 先行在 exit/_exit/abort 内），被杀者 stdout 的收集方
//! **收不到零长 EOF**——收口只能靠 wait 的 TERMINATED 项（不赌 10s 活动
//! 帽），详见 [`collect_fg`] 头注。打字-ahead（等待期收走的非 0x03 字节）
//! 进 [`TYPEAHEAD`] 缓存，交互循环先于 stdin 消化——不丢键。
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

use std::io::{Read, Write};

use berry_libc::io::{close, fs_h, read, rootfs_h, stdin_h, write};
use berry_libc::{
    EEXIST, EIO, EISDIR, EINVAL, ENOENT, ENOSPC, ENOTDIR, ENOTEMPTY,
};
use berry_libc::fs::{
    chdir, closedir, getcwd, mkdir, open, opendir, readdir, stat, statfs, unlink, O_CREAT,
    O_RDONLY, O_WRONLY, Stat, Statfs,
};
use berry_sys::{
    berry_channel_recv, berry_clock_read, berry_handle_close, berry_handle_duplicate,
    berry_object_wait_many, berry_object_wait_one, berry_process_terminate, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BAD_STATE, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_RECV, RIGHT_SEND,
    SIGNAL_READABLE, SIGNAL_TERMINATED,
};
use berry_exec::{channel_pair, cpio_find, cpio_names, spawn_rootfs_elf, ChildHandles, SpawnSpec};
// C-1 刀 4：fsproto 只余帽常量（usage 预检 + 读面失控兜底）——协议面
// （encode/parse/ST_*）随自有 RPC 退役，全部走 berry-libc。
use berry_fsproto::{FILE_SIZE_MAX, NAME_MAX, PATH_DEPTH_MAX, PATH_MAX};

// C-4：行编辑器（console 退化 = 逐字节透传——echo/退格/历史/Tab 补全归
// bsh；编辑会话归属裁定兑现，shell设计 §2.4）。
mod editor;

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
    /// C-4b：因 Ctrl-C（process_terminate 130）收场——EOF 缺口面的合法
    /// 无 EOF（不是断链），调用方据此静默 no-EOF 警告。
    killed: bool,
}

/// C-4b 前台交互态：boot 自验链全程 false（自验子进程不容 stdin 误杀、
/// `collect` 路径零漂移——C-4r 确定性契约保持）；进交互循环前置 true——
/// run/pipe 的前台等待自此并监 stdin（[`collect_fg`]）。`static mut` +
/// 访问器同 [`TYPEAHEAD`] 单线程纪律（Cell 非 Sync / std::sync 锁走
/// pthread 桩深水区，均勿引）。
static mut INT_MODE: bool = false;

fn int_armed() -> bool {
    // SAFETY: bsh 单线程（main 独占）；唯一写点在交互循环前（先于任何读）。
    unsafe { core::ptr::addr_of_mut!(INT_MODE).read() }
}

fn int_arm() {
    // SAFETY: 同上。
    unsafe {
        *core::ptr::addr_of_mut!(INT_MODE) = true;
    }
}

/// C-4b 打字-ahead 暂存：前台等待期（collect_fg）收走的非 0x03 stdin 字节
/// （不收则 console 吸收环满丢新，C-4r）；交互循环**先于** stdin 消化
/// （FIFO）。std 单线程（v1 无 pthread 面——std::sync 锁走 futex/pthread
/// 桩深水区，勿引），`static mut` + `addr_of_mut` 单线程纪律同 berry-exec
/// `ELF_WIN`。
static mut TYPEAHEAD: Vec<u8> = Vec::new();

fn typeahead_push(b: u8) {
    // SAFETY: bsh 单线程（main 独占）；无并发写者。
    unsafe { (*core::ptr::addr_of_mut!(TYPEAHEAD)).push(b) }
}

fn typeahead_pop() -> Option<u8> {
    // SAFETY: 同上；FIFO——从队首取（push 追加尾）。
    unsafe {
        let v = &mut *core::ptr::addr_of_mut!(TYPEAHEAD);
        if v.is_empty() {
            None
        } else {
            Some(v.remove(0))
        }
    }
}

fn main() {
    // —— 自驱自验：内部 `run hexyl`，全链（cpio 查询 → 流式拉起 → feeder →
    //    私有 stdout 收齐）即交互 `run` 的同一台机器——自验过 = 交互面可信。
    //    输出绑 `hexyl_out` 留用：fs 自验的 2156B 多片写入样本（P6 A-1）。
    let hexyl_out: Vec<u8> = if let Some((o, _)) =
        run_capture("hexyl", Some(&HEXYL_STDIN), &["--color=never"])
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
    // —— C-2b：U2 必须排在 fat 族**之前**——EFI 路无盘时 fat_selftest
    //    ST_NOENT → park（`-> !` 永不返回），排其后的自验全部不可达
    //    （对抗审查实抓的链序契约；U2 零 fs 面，前移后 EFI 路亦打 U2 ok
    //    ——smoke-image ECO 同面）。
    u2_selftest(&mut vars, &mut status);
    // —— C-2c：U3 与 U2 同款链序契约——text 批全零 fs 盘面（纯 RAM/spawn），
    //    排 fat 族之前（EFI 路无盘亦打 U3——smoke-image ECO 同面）。
    u3_selftest();
    // —— B-1：EL0 基准自验（`B1 ok n=4` = rootfs 相第 28 标记）。链序契约
    //    同 U2/U3——bench1 零 fs 盘面（纯 RAM/spawn），排 fat 族之前（EFI
    //    路无盘亦打 B1——smoke-image ECO 同面）。
    bench1_selftest();
    // —— C-2c 挂账刀：U4 深坑六件执行面自验（`U4 ok` = rootfs 相第 30
    //    标记）。链序契约同 U2/U3/B1——tail/head/env/date 四步纯管道 +
    //    grep/which/find 三步走 RAM 树 fs gift（零盘面），排 fat 族之前
    //    （EFI 路无盘亦打 U4——smoke-image ECO canon 同面）。
    u4_selftest();
    // —— C-4：Tab 补全 + 行编辑接管自验（`TAB ok` = rootfs 相第 33 标记）。
    //    链序契约同 U2/U3/U4——零 fs 盘面（FakeSrc 状态机 + cpio 直读 +
    //    RAM 树 /etc），排 fat 族之前（EFI 路无盘亦打 TAB——smoke-image
    //    ECO canon 同面）。
    tab_selftest();
    // —— C-4b：Ctrl-C kill 面自验（`INT ok` = rootfs 相第 34 标记）。链序
    //    契约同 U2/U3/TAB——零 fs 盘面（editor 状态机 + bin/hello 纯 RAM
    //    spawn + 内核 terminate 原语），排 fat 族之前（EFI 路无盘亦打
    //    INT——smoke-image ECO canon 同面）。
    int_selftest();
    fat_selftest();
    fatw_selftest();
    // —— C-2 刀 2：df + statfs 真值自验（`DF ok` = rootfs 相第 29 标记；
    //    du 挂账裁出——uu_du 无条件 thread::spawn 平台缺口，见 df_selftest
    //    头注实录）。
    //    **排 fat 族后**（依赖挂载卷在位 + fatw 已写 RW.TXT——差分基线件）；
    //    EFI 路无盘 fat park（`-> !` 挡断其后自验）→ DF 结构性不打
    //    （smoke-image ECO canon 零变化）。
    df_selftest();
    dir_selftest();
    cd_selftest();

    // —— 交互循环（化妆面）：**C-4 起行编辑归 bsh**（console 逐字节透传，
    //    echo/退格/历史召回/Tab 补全全在本端——editor.rs）。逐字节 read +
    //    Editor::feed；行定稿走 run_line。空读（fd0 的 10s 帽超时 EOF 化）
    //    = 噪声 → 静默续读，prompt 不重刷（rootfs 相无人敲键盘也不刷屏；
    //    真交互者敲行即得响应——与 read_line 时代零漂移）。
    let stdin = std::io::stdin();
    let mut sin = stdin.lock();
    let mut out = std::io::stdout();
    let mut ed = editor::Editor::new();
    let mut src = RealSrc;
    let mut prompt_pending = true;
    // C-4b：交互态武装——Ctrl-C 杀链自此后效（boot 自验链全程未武装，
    // 自验子进程不容 stdin 误杀）。
    int_arm();
    loop {
        if prompt_pending {
            write!(out, "bsh> ").unwrap();
            out.flush().unwrap();
            prompt_pending = false;
        }
        // C-4b：前台等待期（collect_fg）收走的打字-ahead 先于 stdin 消化
        // （FIFO——时序保全，不丢键）。
        let c = match typeahead_pop() {
            Some(c) => c,
            None => {
                let mut b = [0u8; 1];
                if sin.read(&mut b).unwrap_or(0) == 0 {
                    continue; // 空读噪声：不重刷 prompt、不睡（下轮 read 再阻塞 10s）
                }
                b[0]
            }
        };
        match ed.feed(c, &mut src) {
            editor::Flow::Continue(echo) => {
                if !echo.is_empty() {
                    out.write_all(&echo).unwrap();
                    out.flush().unwrap();
                }
            }
            editor::Flow::Line(line) => {
                out.write_all(b"\n").unwrap(); // Enter 回显（归调用方契约）
                let s = String::from_utf8_lossy(&line).into_owned();
                run_line(s.trim_end_matches(['\n', '\r']), &mut vars, &mut status, &mut out);
                prompt_pending = true;
            }
            editor::Flow::Interrupt(echo) => {
                // C-4b：feed 循环期恒无前台子进程（前台等待走 collect_fg，
                // 其内的 0x03 在那边杀链处理——两路径互斥不重入）→ 作废
                // 当前行：编辑器已清行，此处 "^C\n" 回显 + prompt 重显。
                out.write_all(&echo).unwrap();
                out.flush().unwrap();
                prompt_pending = true;
            }
        }
    }
}

/// C-4 真源（Tab 候选）：命令位 = 内建 ∪ rootfs bin/（cpio 直读——libc
/// fs 视图无 /bin，bin/ 只存在于 rootfs VMO，`run` 同源；双源同名由
/// berry-complete 内部去重）；路径位 = libc opendir（RAM overlay + FAT
/// 回落——与 `ls` 同一条路）。"." = 编辑器的 cwd 相对名形态 → getcwd
/// 绝对化（cwd 值来自同一条 fs，白名单必过）。
struct RealSrc;

impl editor::TabSource for RealSrc {
    fn commands(&mut self, out: &mut Vec<Vec<u8>>) {
        const BUILTINS: [&[u8]; 11] = [
            b"help", b"exit", b"env", b"echo", b"run", b"cat", b"ls", b"mkdir", b"rm",
            b"cd", b"pwd",
        ];
        for b in BUILTINS {
            out.push(b.to_vec());
        }
        let h = rootfs_h();
        if h != 0 {
            cpio_names(h, b"bin/", &mut |name| out.push(name.to_vec()));
        }
    }

    fn entries(&mut self, dir: &[u8]) -> Result<Vec<Vec<u8>>, ()> {
        let d: Vec<u8> = if dir == b"." {
            let mut buf = [0u8; 160];
            // SAFETY: buf 可写、长度如参；null = 错（cwd 面 Err 静默）。
            let r = unsafe { getcwd(buf.as_mut_ptr() as *mut _, buf.len()) };
            if r.is_null() {
                return Err(());
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            buf[..len].to_vec()
        } else {
            dir.to_vec()
        };
        // 尾 '/' 剥到只剩根：fsproto path_valid 空段拒（前导/尾随/连续
        // NUL）——编辑器 dir 形态恒含尾 '/'，直接送 LOOKUP = ST_BAD
        // （C-4 真机首跑实抓）；根 "/" 保持原样（根寻址立法 nlen 段面）。
        let mut end = d.len();
        while end > 1 && d[end - 1] == b'/' {
            end -= 1;
        }
        let mut p = d[..end].to_vec();
        p.push(0);
        // SAFETY: p NUL 结尾；DIR 指针按 libc 目录族契约使用。
        let dp = unsafe { opendir(p.as_ptr() as *const _) };
        if dp.is_null() {
            return Err(());
        }
        let mut names = Vec::new();
        loop {
            // SAFETY: 返回指向 DIR 内部 Dirent（即拷即用）；null = 目录尾。
            let ent = unsafe { readdir(dp) };
            if ent.is_null() {
                break;
            }
            // SAFETY: ent 有效；d_name NUL 结尾名区。
            let name = unsafe { &(*ent).d_name };
            let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            names.push(name[..len].to_vec());
        }
        unsafe { closedir(dp) };
        Ok(names)
    }
}

/// Tab 补全自验（C-4，`TAB ok` = rootfs 相第 33 标记）。九步全确定性：
/// ⓪-⑥ FakeSrc 喂键断言编辑器状态机（唯一补全/多候选列示/历史召回环/
/// 退格三连与空行无操作/Ctrl-U/ESC 非 final 复位落下/路径位补全——内核
/// 版语义对齐回归面）；⑦⑧ 真源两钉（cpio bin/ 含 ps、libc /etc 含
/// mtab——「补全的源是真的」两面各一钉）。失败 eprintln + park（TAB ok
/// 缺失 → smoke FAIL 兜底）。
fn tab_selftest() {
    use editor::{Editor, Flow, TabSource};

    /// 确定性假源：命令位三候选（echo/env/exit——唯一命中与公共前缀零
    /// 增量两面可造）；路径位只识 /etc/。
    struct FakeSrc;
    impl TabSource for FakeSrc {
        fn commands(&mut self, out: &mut Vec<Vec<u8>>) {
            for c in [b"echo" as &[u8], b"env", b"exit"] {
                out.push(c.to_vec());
            }
        }
        fn entries(&mut self, dir: &[u8]) -> Result<Vec<Vec<u8>>, ()> {
            if dir == b"/etc/" {
                Ok(vec![b"mtab".to_vec()])
            } else {
                Err(())
            }
        }
    }

    /// 喂键串：收 (echo, lines)——echo = Continue/Interrupt 累计回显字节、
    /// lines = Line 定稿集（Enter 事件按序入列；C-4b Interrupt 计回显）。
    fn feed_all(ed: &mut Editor, src: &mut dyn TabSource, keys: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let (mut echo, mut lines) = (Vec::new(), Vec::new());
        for &k in keys {
            match ed.feed(k, src) {
                Flow::Continue(e) => echo.extend_from_slice(&e),
                Flow::Line(l) => lines.push(l),
                Flow::Interrupt(e) => echo.extend_from_slice(&e),
            }
        }
        (echo, lines)
    }

    let mut src = FakeSrc;
    let mut fails: Vec<&str> = Vec::new();

    // ⓪ 命令位唯一补全：`ec`+Tab → 行与回显双面 `echo `（后缀 ho + 尾空格）。
    let mut ed = Editor::new();
    let (e, lines) = feed_all(&mut ed, &mut src, b"ec\t");
    if ed.line() != b"echo " || e.as_slice() != b"echo " || !lines.is_empty() {
        fails.push("⓪ 唯一补全");
    }

    // ① 多候选：`e`+Tab → 公共增量空（lcp(echo,env,exit)=prefix）——列示
    // 三名 + prompt/当前行重显，行不动。
    let mut ed = Editor::new();
    let (e, lines) = feed_all(&mut ed, &mut src, b"e\t");
    if ed.line() != b"e" || e.as_slice() != b"e\necho env exit\nbsh> e" || !lines.is_empty() {
        fails.push("① 多候选列示");
    }

    // ② 历史召回环：两行入史（cat=0 新 / ls=1 旧）→ 首翻 newest → 次翻
    // ls → 下翻回 cat → 下翻到 live 清行（内核版对抗审查面全保持）。
    let mut ed = Editor::new();
    let (e, lines) = feed_all(&mut ed, &mut src, b"ls\ncat\n");
    if lines != vec![b"ls".to_vec(), b"cat".to_vec()] || e.as_slice() != b"lscat" {
        fails.push("② 历史入环");
    }
    let (e, _) = feed_all(&mut ed, &mut src, b"\x1b[A");
    if ed.line() != b"cat" || e.as_slice() != b"cat" {
        fails.push("② 上翻 newest");
    }
    let (e, _) = feed_all(&mut ed, &mut src, b"\x1b[A");
    if ed.line() != b"ls" || e.as_slice() != b"\x08 \x08\x08 \x08\x08 \x08ls" {
        fails.push("② 上翻次新");
    }
    let (e, _) = feed_all(&mut ed, &mut src, b"\x1b[B");
    if ed.line() != b"cat" || e.as_slice() != b"\x08 \x08\x08 \x08cat" {
        fails.push("② 下翻回 newest");
    }
    let (e, _) = feed_all(&mut ed, &mut src, b"\x1b[B");
    if ed.line() != b"" || e.as_slice() != b"\x08 \x08\x08 \x08\x08 \x08" {
        fails.push("② 下翻到 live 清行");
    }

    // ③ 退格三连 + 空行无操作：ab 两退（b、a 依次落）+ 第三退空行零回显。
    let mut ed = Editor::new();
    let (e, _) = feed_all(&mut ed, &mut src, b"ab\x7f\x7f\x7f");
    if ed.line() != b"" || e.as_slice() != b"ab\x08 \x08\x08 \x08" {
        fails.push("③ 退格/空行无操作");
    }

    // ④ Ctrl-U 擦全行：abcd 四格回擦。
    let mut ed = Editor::new();
    let (e, _) = feed_all(&mut ed, &mut src, b"abcd\x15");
    if ed.line() != b"" || e.as_slice() != b"abcd\x08 \x08\x08 \x08\x08 \x08\x08 \x08" {
        fails.push("④ Ctrl-U 擦全行");
    }

    // ⑤ ESC 非 final 复位落下回归：ESC '[' 后 0x7f（非参数非 final）→
    // 复位按普通字节落下 = 退格（内核版对抗审查实抓面）。
    let mut ed = Editor::new();
    let (e, _) = feed_all(&mut ed, &mut src, b"xy\x1b[\x7f");
    if ed.line() != b"x" || e.as_slice() != b"xy\x08 \x08" {
        fails.push("⑤ ESC 非 final 复位落下");
    }

    // ⑥ 路径位补全：`cat /etc/m`+Tab → `/etc/mtab`（无尾空格——路径位
    // add_space=false）。
    let mut ed = Editor::new();
    let (e, _) = feed_all(&mut ed, &mut src, b"cat /etc/m\t");
    if ed.line() != b"cat /etc/mtab" || e.as_slice() != b"cat /etc/mtab" {
        fails.push("⑥ 路径位补全");
    }

    // ⑦ 真源钉 1：命令位含内建 echo 与 rootfs bin/ps（C-3 双源真值）。
    let mut rs = RealSrc;
    let mut cmds = Vec::new();
    rs.commands(&mut cmds);
    if !cmds.iter().any(|c| c.as_slice() == b"echo") || !cmds.iter().any(|c| c.as_slice() == b"ps") {
        fails.push("⑦ 真源命令位");
    }

    // ⑧ 真源钉 2：路径位 /etc/ 含 mtab（fsmgr 预建 RAM 树——EFI 路同在）。
    match rs.entries(b"/etc/") {
        Ok(names) => {
            if !names.iter().any(|n| n.as_slice() == b"mtab") {
                fails.push("⑧ 真源路径位");
            }
        }
        Err(()) => fails.push("⑧ 真源路径面不可用"),
    }

    if !fails.is_empty() {
        eprintln!("bsh: tab selftest bad: {:?}", fails);
        berry_libc::park();
    }
    println!("TAB ok");
}

/// Ctrl-C kill 链自验（C-4b，`INT ok` = rootfs 相第 34 标记）。四步全确定性、
/// 零 fs 盘面（editor 状态机 + bin/hello 纯 RAM spawn + 内核 0x3B terminate
/// 原语）——链序契约同 U2/U3/TAB：排 fat 族之前（EFI 路无盘亦打 INT——
/// smoke-image ECO canon 同面）。失败 eprintln + park（INT ok 缺失 → smoke
/// FAIL 兜底）。
///
/// ⓪ editor 常态 0x03：`cat f` 进行中 0x03 → Flow::Interrupt + 行清空 +
///   "^C\n" 回显 + 作废草稿**不入历史**（ESC[A 上翻在空历史上无操作）；
/// ① ESC 序列共存（0x03 与状态机中间态冲突面逐一核对）：裸 ESC 后 0x03
///   （esc=1 非 '[' 复位落下）与 ESC '[' 后 0x03（esc=2——0x03 非参数段
///   0x30..=0x3f、非 final 段 0x40..=0x7e，复位落下）都直达 Interrupt
///   ——CSI 序列被中断、行作废（与真终端语义一致）；
/// ② Interrupt 后编辑器续用：清行后 `ls`+Enter 正常定稿（状态机无残留）；
/// ③ 前台杀链真机面：spawn bin/hello（发 13B 后 10s 滚动长睡——结构性
///   不自退，svcmgr ECO 同款常驻形态）→ `process_terminate(130)` == 0 →
///   重复 terminate == -EBADSTATE（已终止面，退出码首因不覆写）→
///   collect_fg 返回 killed=true + eof=false（**EOF 缺口活证**：内核杀不
///   走 libc exit 族，stdout 收不到零长 EOF——收口靠 wait_many 的
///   TERMINATED 项，不赌 10s 活动帽）→ reap() == Ok(130)（真码面：
///   run_output 的 `$?` 即此值）。
fn int_selftest() {
    use editor::{Editor, Flow, TabSource};

    /// 确定性哑源：0x03 路径永不问 Tab（src 仅 Tab 键消费）。
    struct NoSrc;
    impl TabSource for NoSrc {
        fn commands(&mut self, _out: &mut Vec<Vec<u8>>) {}
        fn entries(&mut self, _dir: &[u8]) -> Result<Vec<Vec<u8>>, ()> {
            Err(())
        }
    }

    let mut src = NoSrc;
    let mut fails: Vec<&str> = Vec::new();

    // ⓪ 常态 0x03：作废当前行——Interrupt("^C\n") + 行清空。
    let mut ed = Editor::new();
    for &k in b"cat f" {
        if !matches!(ed.feed(k, &mut src), Flow::Continue(_)) {
            fails.push("⓪ 前导键非 Continue");
        }
    }
    match ed.feed(0x03, &mut src) {
        Flow::Interrupt(e) if e.as_slice() == b"^C\n" && ed.line().is_empty() => {}
        _ => fails.push("⓪ 常态 0x03 非 Interrupt/清行/回显"),
    }
    // 作废草稿不入历史：上翻在空历史上无操作（行恒空、无 Line 产出）。
    for k in [0x1b, b'[', b'A'] {
        if !matches!(ed.feed(k, &mut src), Flow::Continue(_)) || !ed.line().is_empty() {
            fails.push("⓪ 作废行不入历史");
        }
    }

    // ① ESC 序列共存两态：esc=1 / esc=2 下的 0x03 都复位落下直达 Interrupt。
    let mut ed = Editor::new();
    for k in b"ab\x1b" {
        if !matches!(ed.feed(*k, &mut src), Flow::Continue(_)) {
            fails.push("① 前导键非 Continue");
        }
    }
    match ed.feed(0x03, &mut src) {
        Flow::Interrupt(_) if ed.line().is_empty() => {}
        _ => fails.push("① ESC 态 0x03 非 Interrupt"),
    }
    let mut ed = Editor::new();
    for k in b"ab\x1b[" {
        if !matches!(ed.feed(*k, &mut src), Flow::Continue(_)) {
            fails.push("① 前导键非 Continue");
        }
    }
    match ed.feed(0x03, &mut src) {
        Flow::Interrupt(_) if ed.line().is_empty() => {}
        _ => fails.push("① CSI 态 0x03 非 Interrupt"),
    }

    // ② Interrupt 后编辑器续用：清行后 `ls`+Enter 正常定稿。
    match ed.feed(b'l', &mut src) {
        Flow::Continue(_) => {}
        _ => fails.push("② 续用 l 非 Continue"),
    }
    match ed.feed(b's', &mut src) {
        Flow::Continue(_) => {}
        _ => fails.push("② 续用 s 非 Continue"),
    }
    match ed.feed(b'\n', &mut src) {
        Flow::Line(l) if l.as_slice() == b"ls" => {}
        _ => fails.push("② 续用定稿 != ls"),
    }

    // ③ 前台杀链真机面（bin/hello：svcmgr ECO 同款常驻形态，stack_pages
    //    同款 1 页）。
    let full = b"bin/hello";
    let Some((data_off, data_len)) = cpio_find(rootfs_h(), full) else {
        eprintln!("bsh: int selftest: bin/hello not found");
        berry_libc::park();
    };
    let (out_s, out_c) = channel_pair();
    let spec = SpawnSpec {
        name: full,
        data_off,
        data_len,
        stdout: out_c,
        stack_pages: 1,
        stdin_feed: None,
        stdin_handle: None,
        args: &[],
        extra_gifts: &[],
        mmio_vmo: None,
    };
    let child = match spawn_rootfs_elf(rootfs_h(), &spec) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("bsh: int selftest: hello spawn failed");
            berry_libc::park();
        }
    };
    if berry_process_terminate(child.hp, 130) != 0 {
        fails.push("③ terminate(130) != 0");
    }
    if berry_process_terminate(child.hp, 130) != BERRY_ERR_BAD_STATE {
        fails.push("③ 重复 terminate != -EBADSTATE");
    }
    let o = collect_fg(out_s, child.hp, &[child.hp]);
    if !o.killed || o.eof {
        fails.push("③ collect_fg 非 killed 收场（EOF 缺口面破缺）");
    }
    match child.reap() {
        Ok(130) => {}
        Ok(_) => fails.push("③ reap 真码 != 130"),
        Err(()) => fails.push("③ reap 失败"),
    }
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        berry_libc::park();
    }

    if !fails.is_empty() {
        eprintln!("bsh: int selftest bad: {:?}", fails);
        berry_libc::park();
    }
    println!("INT ok");
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
            "bsh 内建：help | echo <text> | env | run <name> | cat <file> | ls | ls <dir> | mkdir <dir> | rm <path> | cd <dir> | pwd | A | B（rootfs 管道）| A && B | A > file | exit | 裸名 = bin/<name> 派发（C-2a uu 家族）"
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
        run_output(rest.trim(), &[], out)
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
        // C-2a：裸名 PATH 派发（内建优先零漂移——到这 = 非内建）：首词查
        // bin/<name>，命中即拉起、余词作 argv[1..]（uu 家族非内建成员
        // cp/mv/touch/rmdir 等经这条进 shell；裸名 ls/rm/mkdir/cd/pwd/cat
        // 被内建分支先吃，GNU 全量面经 `run ls` 取）；未命中保持 not found
        // 127（文案/码不变——RX 相 b/c 断言面）。含 `/` 的词不走派发
        // （v1 PATH 唯一 = bin/，路径形仍 not found）。
        let mut words = cmd.split_whitespace();
        let (name, rest): (&str, Vec<&str>) = match words.next() {
            Some(w) if !w.contains('/') => (w, words.collect()),
            _ => {
                writeln!(out, "bsh: {}: not found", cmd).unwrap();
                return (Vec::new(), 127);
            }
        };
        if cpio_find(rootfs_h(), format!("bin/{}", name).as_bytes()).is_some() {
            run_output(name, &rest, out)
        } else {
            writeln!(out, "bsh: {}: not found", cmd).unwrap();
            (Vec::new(), 127)
        }
    }
}

/// `run <name>` / 裸名 PATH 派发共用的渲染面（P6 A-1 拆 do_run——直出与
/// `>` 重定向共道）：cpio 查 `bin/<name>` → 私有 stdout 对拉起（32 页栈 =
/// musl 128KiB 口径，盖住 clap 级栈深）→ 收齐 EOF → **数据字节返回**交调用
/// 方定向；警告照打 `out`（stderr 语义）。子进程无 stdin（env 块单 gift，
/// libc /dev/null 语义——read 即 EOF）。`args` = argv[1..]（C-2a 起：
/// `run` 恒空片、PATH 派发带余词——cp/mv 双操作数走这条）。返回 (数据,
/// 状态码)：127 not found / 126 spawn failed / 2 usage / 其余 = 子真码
/// （reap Err → 1——真码源 0x3A）。
fn run_output(name: &str, args: &[&str], out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    if name.is_empty() || name.contains([' ', '/']) {
        writeln!(out, "usage: run <name>").unwrap();
        return (Vec::new(), 2);
    }
    let full = format!("bin/{}", name);
    let Some((data_off, data_len)) = cpio_find(rootfs_h(), full.as_bytes()) else {
        writeln!(out, "bsh: run: {}: not found", name).unwrap();
        return (Vec::new(), 127);
    };
    // C-2c 挂账刀：fs gift 装配（挂账文档 §② 根因修法——裸 spawn 的子
    // 进程 fs_h()==0，libc 文件 op 全体降级 ENOENT（与文件缺失不可区分）
    //；uu 家族文件参数面（cat <f>/grep <f>/df …）自此真可达）。fs 缺席
    // = 裸形态降级（无 fsmgr 不死 shell——零漂移面）。
    let mut gift_buf = [0 as Handle; 2];
    let gifts: Option<&[Handle]> = match fs_gifts() {
        Some((r, f)) => {
            gift_buf = [r, f];
            Some(&gift_buf[..])
        }
        None => None,
    };
    let (out_s, out_c) = channel_pair();
    let argv: Vec<&[u8]> = args.iter().map(|a| a.as_bytes()).collect();
    let spec = SpawnSpec {
        name: full.as_bytes(),
        data_off,
        data_len,
        stdout: out_c,
        stack_pages: 32,
        // gift 在位时按 berry-exec 断言补空 feeder 占位（EOF 即达——子
        // 进程读 stdin 返 0，与裸形态 /dev/null 语义同形零漂移）。
        stdin_feed: if gifts.is_some() { Some(&[]) } else { None },
        stdin_handle: None,
        args: &argv,
        extra_gifts: gifts.unwrap_or(&[]),
        mmio_vmo: None,
    };
    let (data, code) = if let Ok(child) = spawn_rootfs_elf(rootfs_h(), &spec) {
        // C-4b：交互态 = 前台 Ctrl-C 武装（collect_fg 三项等待）；boot 自验
        // 链（INT_MODE=false）走纯 collect——C-4r 确定性契约零漂移。
        let o = if int_armed() {
            collect_fg(out_s, child.hp, &[child.hp])
        } else {
            collect(out_s)
        };
        if !o.eof && !o.killed {
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
        // gifts 未被 take（早退点全在 env send 前）——显式补关防泄漏
        //（未装配/未成的 0 值关返 -EBADF 无害，照关省一分支）。
        if gifts.is_some() {
            let _ = berry_handle_close(gift_buf[0]);
            let _ = berry_handle_close(gift_buf[1]);
        }
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

/// df + statfs 真值自验（C-2 刀 2，`DF ok` = rootfs 相第 29 标记——
/// 确定性状态谓词 + 双向真值）。三步全收齐才打（单消息原子）：
/// - 步⓪ 几何定值：直调 statfs（libc 真实现 → OP_STATFS RPC → fsmgr BPB
///   几何 + FAT 扫描）断言 `f_blocks==506 && f_bsize==2048 && f_frsize==2048`
///   ——mkfat 默认卷与 fixture 盘同几何（VOL_SECTORS=2032/SPC=4/簇 2048B/
///   clusters=506），造盘定值跨相恒定。
/// - 步① 差分真值（**相位无关化**——绝对 free 跨相漂移：rootfs 相每 boot
///   重造盘 free=500 / 持久化相脏盘 499，绝不能当字面断言）。原案
///   DFTEST.BIN「写 2048B → free 差==1 → 删除」不可行：FAT 文件删除留 v2
///   （UNLINK 纯 RAM 树面、fat.rs 无 stage_unlink），残名跨 boot 让第二跑
///   覆写净差 0 ≠ 1 相位化必 FAIL——改用 fatw 刚写的 RW.TXT 重定尺寸双向
///   对拍：基线 statfs → 覆写 2048B（1 簇，TRUNC commit 时旧 2 簇链
///   collect、新链短一节）→ free 恰 +1 → 还原 3000B 原 pattern（fatw 同源
///   (i*17+3)%256）→ free 恰回基线。缩/放两向都过 = 空闲计数既不撒谎也
///   不漏失效；双写都在本 boot 内闭合、终态 = fatw 终态（幂等零残留）。
/// - 步② df 表格面：spawn `bin/df --output=source,fstype` 逐字节对拍
///   （期望值定标实录字面常量；列宽 = Source min_width 14 左对齐 + 单空
///   格分隔、末列无尾随空格——uu_df table.rs 渲染源钉板）。活证全链：
///   /etc/mtab 回落行（fsmgr 启动预建）+ mount_dev_id 分组（stat 根
///   st_dev 非零定值）+ fstype=fat 过 is_dummy/is_remote 过滤 + statfs
///   真值装配（blocks>0 才出行）+ 表头 translate! 面（df 薄 harness 手动
///   setup_localization——缺失时表头变消息 ID、列宽漂移）。
///
/// **du 裁出挂账（首跑定标实录 2026-10-27，C-2c uu_sort 同型同批）**：
/// 原案步③ `du -b PATTERN.BIN` → `5000\tPATTERN.BIN`（表观尺寸真值）
/// 实跑必死——uu_du 0.12.0 du.rs ~:1168 无条件 `thread::spawn`（打印线
/// 程，无 flag 可关）→ std 线程钩子 `run_spawn_hooks` 对未注册平台的
/// 全局注册表做原子加 → outline-atomics LSE 探测回退 `ldxr` 路解引用
/// 垃圾指针（实抓 dump：D-abort(lower) ISS=0x21 PC=0x306918
/// FAR=0x26c68c——PC 落 `__aarch64_cas` 家族 ldxr/stxr 循环、x1==FAR
/// 即原子目标）→ G1.5 kill（code=0 len=0 eof=false 经典死状）。std 线
/// 程面（pthread/TLS）平台缺口挂账，随 C-2c 深坑件族另刀；du 表观尺
/// 寸面（st_blocks=512B 扇区取整、-b 消块口径）届时回收——bin/du 仍
/// 随 rootfs 发行（uu_sort 同形：交互可用面缺席、自验不硬凑假绿）。
/// 期望值全字面常量（绝不 Rust 镜像计算——P6 A-5 基准漂移雷）；失败诊断
/// 单串 format! 一次 eprintln（稳态矩阵撕行防护，u3_bad 同款）+ park
/// （标记缺失 → smoke FAIL loud 兜底）。
fn df_selftest() {
    // 步⓪：几何定值（statfs 直调——恒定面先验，步① 差分基线才有意义）。
    let mut sf: Statfs = unsafe { core::mem::zeroed() };
    // SAFETY: cpath("/") 恒 NUL 结尾 C 串；sf 按 libc 契约出参写入。
    if unsafe { statfs(cpath("/").as_ptr() as *const _, &mut sf) } != 0 {
        eprintln!("bsh: df selftest bad: step=0 statfs errno={}", errno());
        berry_libc::park();
    }
    if sf.f_blocks != 506 || sf.f_bsize != 2048 || sf.f_frsize != 2048 {
        eprintln!(
            "bsh: df selftest bad: step=0 geom blocks={} bsize={} frsize={}",
            sf.f_blocks, sf.f_bsize, sf.f_frsize
        );
        berry_libc::park();
    }
    // 步①：RW.TXT 重定尺寸差分（3000B=2 簇 → 2048B=1 簇 → 3000B）。2048B
    // 覆写面 = 前 2048 字节恰是原 pattern 前缀（per-index 公式），用
    // full[..2048] 切片即 fatw 同源内容。
    let base = sf.f_bfree;
    let full: Vec<u8> = (0..3000u32).map(|i| ((i * 17 + 3) % 256) as u8).collect();
    if let Err(e) = put_file("RW.TXT", &full[..2048]) {
        eprintln!("bsh: df selftest bad: step=1 shrink {}", errno_text(e));
        berry_libc::park();
    }
    let mid = statfs_free();
    if let Err(e) = put_file("RW.TXT", &full) {
        eprintln!("bsh: df selftest bad: step=1 restore {}", errno_text(e));
        berry_libc::park();
    }
    let end = statfs_free();
    if mid != base + 1 || end != base {
        eprintln!(
            "bsh: df selftest bad: step=1 diff base={} mid={} end={}",
            base, mid, end
        );
        berry_libc::park();
    }
    // 步②：df 执行面（挂载表 + statfs 装配全链活证；期望 = uu_df 渲染源
    // 钉板字面：Filesystem(10) pad 至 14 + 单空格 + Type / 末列无尾随空格）
    // ——**run_capture_fs**：df 子进程要经 libc open/statfs 消费 fs 服务
    //（U 家族先例零 fs 面，run_capture 裸 spawn 的子进程 fs_h()==0 → open
    // 降级 ENOENT，首跑实抓——gift 四元装配见该函数头注）。
    match run_capture_fs("df", None, &["--output=source,fstype"]) {
        Some((o, c)) if c == 0
            && o.data == b"Filesystem     Type\n/dev/root      fat\n"
            && o.eof
            && o.dropped == 0 => {}
        Some((o, c)) => df_bad(2, c, &o),
        None => {
            eprintln!("bsh: df selftest bad: step=2 df not found");
            berry_libc::park();
        }
    }
    println!("DF ok");
}

/// statfs 空闲簇真值（df 自验差分面专用）：statfs 失败 = 链断级——errno
/// 打点后直接 park（与调用方诊断同形，不自造 Err 往返）。
fn statfs_free() -> u64 {
    let mut sf: Statfs = unsafe { core::mem::zeroed() };
    // SAFETY: 同上——cpath("/") 恒 NUL 结尾。
    if unsafe { statfs(cpath("/").as_ptr() as *const _, &mut sf) } != 0 {
        eprintln!("bsh: df selftest bad: statfs errno={}", errno());
        berry_libc::park();
    }
    sf.f_bfree
}

/// DF 断言失败诊断（u3_bad 同款）：四面（真码/长度/EOF/丢弃）+ 实际字节
/// hex（截 64B）**单串一次打**后 park。
fn df_bad(step: u8, c: i32, o: &RunOut) -> ! {
    let n = o.data.len().min(64);
    let s = format!(
        "bsh: df selftest bad: step={} code={} len={} eof={} dropped={} hex={}",
        step,
        c,
        o.data.len(),
        o.eof,
        o.dropped,
        u3_hex(&o.data[..n])
    );
    eprintln!("{}", s);
    berry_libc::park()
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
        segs.push(PipeSeg { name, args: &[] });
    }
    let code = match pipe_capture(&segs, None) {
        Ok((o, c)) => {
            out.write_all(&o.data).unwrap();
            if !o.eof && !o.killed {
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

/// 管道段：程序名 + argv[1..]（自验传 `--color=never`，交互 v1 裸名空片）。
struct PipeSeg<'a> {
    name: &'a str,
    args: &'a [&'a str],
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
    // C-2c 挂账刀：首段 fs gift 装配（挂账文档 §②；v1 约束「单管道至多
    // 一个 fs 消费段」的结构化落法——gifts 只骑首段，fs 缺席 = 裸形态
    // 降级零漂移）。fail-fast 已过 = 名字全命中，dup 失败不影响 spawn 面。
    let mut gift_buf = [0 as Handle; 2];
    let gifts: Option<&[Handle]> = match fs_gifts() {
        Some((r, f)) => {
            gift_buf = [r, f];
            Some(&gift_buf[..])
        }
        None => None,
    };
    // pipes[i] = 段 i 与段 i+1 之间的通道对（(写端, 读端)）。
    let mut pipes: Vec<(Handle, Handle)> = Vec::with_capacity(last);
    for _ in 0..last {
        pipes.push(channel_pair());
    }
    let mut children: Vec<ChildHandles> = Vec::with_capacity(segs.len());
    for (i, (full, (data_off, data_len))) in found.iter().enumerate() {
        let argv: Vec<&[u8]> = segs[i].args.iter().map(|a| a.as_bytes()).collect();
        let spec = SpawnSpec {
            name: full.as_bytes(),
            data_off: *data_off,
            data_len: *data_len,
            stdout: if i == last { out_c } else { pipes[i].0 },
            stack_pages: 32, // musl 128KiB 口径（hexyl 级栈深，d-3c）
            stdin_feed: if i == 0 {
                // gift 在位时按 berry-exec 断言补空 feeder 占位（EOF 即达
                //——首段读 stdin 恒返 0，与裸形态同形零漂移）。
                first_feed.or(gifts.is_some().then_some(&[] as &[u8]))
            } else {
                None
            },
            stdin_handle: if i > 0 { Some(pipes[i - 1].1) } else { None },
            args: &argv,
            extra_gifts: if i == 0 { gifts.unwrap_or(&[]) } else { &[] },
            mmio_vmo: None,
        };
        match spawn_rootfs_elf(rootfs_h(), &spec) {
            Ok(c) => children.push(c),
            Err(_) => {
                // 失败段无子进程；读端中只有本段待用的 pipes[i-1].1 未被
                // take（其余已被后段 spawn 迁走，close 返 -EBADF 忽略之）。
                // fs gifts 同面：只在首段 spawn 失败时未 take（i>0 时已被
                // 首段 env send 迁走，照关返 -EBADF 无害）。
                if i == 0 && gifts.is_some() {
                    let _ = berry_handle_close(gift_buf[0]);
                    let _ = berry_handle_close(gift_buf[1]);
                }
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
    // 死锁序铁律：先收末段 stdout（EOF 链逐级传到采集端）……C-4b：交互态
    // 前台等待集 = [out:READABLE, **末段** proc:TERMINATED, stdin:READABLE]
    // ——Ctrl-C 杀**全段**（terminate 逐段 130），但 TERMINATED 只盯末段：
    // 盯中段会在「中段正常先退、末段仍在产出」的常态 pipe 上提前断收 +
    // 尾段丢失（EOF 缺口面只对被杀者成立——正常退出者 EOF 先行，idx 序
    // 保证 READABLE 恒先命中）。boot 自验（INT_MODE=false）走纯 collect
    // 零漂移。
    let o = if int_armed() && !children.is_empty() {
        let kills: Vec<Handle> = children.iter().map(|c| c.hp).collect();
        collect_fg(out_s, children[last].hp, &kills)
    } else {
        collect(out_s)
    };
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
    let a1 = match run_capture("hexyl", Some(pat), &["--color=never"]) {
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
        PipeSeg { name: "hexyl", args: &["--color=never"] },
        PipeSeg { name: "hexyl", args: &["--color=never"] },
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
    let out3 = match run_capture("hexyl", Some(&a1), &["--color=never"]) {
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

/// u2 自验（C-2b，`U2 ok` = rootfs 相第 26 标记——确定性状态谓词）：C-2a
/// 接入的 uu echo/true/false 三命令**执行面**验收（U1 = svcmgr 宿主九步 fs
/// 元数据链，本刀补 argv 多参/`-n`/真退出码/`&&` 链/管道对拍——U1 未盖
/// 面，互补不重叠；uu echo 带参仅自验面可达——交互面内建优先原样）。
/// 六步全在 bsh 进程内私有通道收齐后逐字节比对（零串口 grep 面，PIPE ok
/// 同款）；全过才打 `U2 ok`（单消息原子）；任一步败 eprintln 诊断 + park
/// （标记缺失 → smoke FAIL loud 兜底）。调用位 = main 链 fat 族**之前**
/// （EFI 路无盘 fat park 挡断其后——可达性链序契约，见调用点注）。
///
/// ① `echo berry u2` 多参 → 9B 恰长 `berry u2\n` + EOF 干净（该腿兼作
/// 步 ⑥ 管道参照，零额外 spawn）；② `echo -n u2x` → 3B 无尾换行（`-n`
/// flush 面闭环）；③ `true` 真码 0 + 零输出；④ `false` 真码 1 + 零输出；
/// ⑤ `&&` 链真码传播（and_selftest 同款零串口噪声）：`run true && A=1`
/// 置 A、`run false && B=1` 短路 → status==1（false 残留）+ A=="1" +
/// B 未置；⑥ `echo | cat` 管道对拍（PIPE ok 同型；cat 恒无参末段——
/// extra_gifts 无 fs handle 契约不破）与步 ① 参照逐字节全等。
fn u2_selftest(vars: &mut Vec<(String, String)>, status: &mut i32) {
    // 步 1：echo 多参（argv[1..] = berry/u2——C-2a SpawnSpec::args 面）。
    let echo_ref: Vec<u8> = match run_capture("echo", Some(&[]), &["berry", "u2"]) {
        Some((o, c)) if c == 0 && o.data == b"berry u2\n" && o.eof && o.dropped == 0 => o.data,
        Some((o, c)) => {
            eprintln!(
                "bsh: u2 selftest bad: step=1 code={} len={} eof={} dropped={}",
                c,
                o.data.len(),
                o.eof,
                o.dropped
            );
            berry_libc::park();
        }
        None => {
            eprintln!("bsh: u2 selftest bad: step=1 echo not found");
            berry_libc::park();
        }
    };
    // 步 2：`-n` 无尾换行（3B 恰长——沿革「-n 零消费路径」的验收面闭环）。
    match run_capture("echo", Some(&[]), &["-n", "u2x"]) {
        Some((o, c)) if c == 0 && o.data == b"u2x" && o.eof && o.dropped == 0 => {}
        Some((o, c)) => {
            eprintln!(
                "bsh: u2 selftest bad: step=2 code={} len={} eof={} dropped={}",
                c,
                o.data.len(),
                o.eof,
                o.dropped
            );
            berry_libc::park();
        }
        None => {
            eprintln!("bsh: u2 selftest bad: step=2 echo not found");
            berry_libc::park();
        }
    }
    // 步 3/4：true/false 真退出码（0x3A 真码链）+ 零输出。
    match run_capture("true", Some(&[]), &[]) {
        Some((o, c)) if c == 0 && o.data.is_empty() && o.eof && o.dropped == 0 => {}
        Some((o, c)) => {
            eprintln!(
                "bsh: u2 selftest bad: step=3 code={} len={} eof={} dropped={}",
                c,
                o.data.len(),
                o.eof,
                o.dropped
            );
            berry_libc::park();
        }
        None => {
            eprintln!("bsh: u2 selftest bad: step=3 true not found");
            berry_libc::park();
        }
    }
    match run_capture("false", Some(&[]), &[]) {
        Some((o, c)) if c == 1 && o.data.is_empty() && o.eof && o.dropped == 0 => {}
        Some((o, c)) => {
            eprintln!(
                "bsh: u2 selftest bad: step=4 code={} len={} eof={} dropped={}",
                c,
                o.data.len(),
                o.eof,
                o.dropped
            );
            berry_libc::park();
        }
        None => {
            eprintln!("bsh: u2 selftest bad: step=4 false not found");
            berry_libc::park();
        }
    }
    // 步 5：&& 链真码传播（and_selftest 同款——run_line 内部驱动 + 变量表
    // 断言；true/false 零输出零警告 = 零串口噪声）。
    let mut out = std::io::stdout();
    run_line("run true && A=1", vars, status, &mut out);
    run_line("run false && B=1", vars, status, &mut out);
    let get = |n: &str| get_var(vars, n).map(|s| s.to_string());
    if *status != 1 || get("A").as_deref() != Some("1") || get("B").is_some() {
        eprintln!(
            "bsh: u2 selftest bad: step=5 status={} A={:?} B={:?}",
            status,
            get("A"),
            get("B")
        );
        berry_libc::park();
    }
    // 步 6：管道对拍（PIPE ok 同型死锁序铁律在 pipe_capture 内）。
    let segs = [
        PipeSeg { name: "echo", args: &["berry", "u2"] },
        PipeSeg { name: "cat", args: &[] },
    ];
    match pipe_capture(&segs, Some(&[])) {
        Ok((o, c)) if c == 0 && o.data == echo_ref && o.eof && o.dropped == 0 => {}
        Ok((o, c)) => {
            eprintln!(
                "bsh: u2 selftest bad: step=6 code={} len={} eof={} dropped={}",
                c,
                o.data.len(),
                o.eof,
                o.dropped
            );
            berry_libc::park();
        }
        Err(PipeFail::NotFound(n)) => {
            eprintln!("bsh: u2 selftest bad: step=6 {} not found", n);
            berry_libc::park();
        }
        Err(PipeFail::SpawnFail(n)) => {
            eprintln!("bsh: u2 selftest bad: step=6 {} spawn failed", n);
            berry_libc::park();
        }
    }
    println!("U2 ok");
}

/// C-2c U3：text 批执行验收（pwd/cat/wc/cut/tr/uniq 六命令 + 三段管道
/// 面）。**七步门审原案两处挂账裁形**（首跑定标实录，不硬凑假绿）：
/// - 步③ head 裁出：uu_head 0.12.0 unix stdin 路径 `try_clone_to_owned`
///   → fcntl(F_DUPFD) → berry-libc 桩 -EINVAL → uucore 译 "Invalid
///   input" 退 1（实测 code=1 + 该串）——fd 克隆面（dup/dup2/fcntl）libc
///   刀挂账，head 家族 v1 结构性不可验收。
/// - 步④ sort|uniq 与步⑥ 四段链的 sort 段：uu_sort 启动期 tmp_dir.rs:67
///   `ctrlc::set_handler` → ctrlc/signal-hook 后台线程 → std
///   `thread::spawnhook::run_spawn_hooks` 解引用空注册表悬垂指针 4
///   （static @0x432078——链接文件面原值 0400…已核，非 berry-ld 重定位
///   错）→ outline-atomics `__aarch64_ldadd8_relax+0x18` 原子加 D-abort
///   → 内核 G1.5 kill（空输出无 EOF）——std 线程面（pthread/TLS）平台
///   缺口挂账；步⑥ 按门审预案授权裁三段（cat,uniq,wc -l）。
/// 步序映射：1=⓪pwd 2=①cat 3=②wc-c 4=⑤cut|tr 5=⑥'三段链。spawn 合计
/// = 8 ≤ 12（1+1+1+2+3）。feed 面 ≤1.5KB（三常量合计 96B）、期望面
/// <32KiB（COLLECT_MAX 口径，实测最大 32B）。断言期望值 = 定标实录
/// **字面常量**（绝不 Rust 镜像计算——P6 A-5 基准漂移雷）：pwd `/\n` /
/// cat 逐字节全等 / wc-c `32\n` / cut|tr `BC\nZ\nEL\n` / 三段链 `7\n`。
/// 链序契约同 U2：全步零 fs 盘面（纯 RAM/spawn），排 fat 族之前——EFI
/// 路无盘亦打 U3（smoke-image ECO 同面）。诊断行单串 format! 再打（多
/// 段 eprintln 逐段 write 会被稳态矩阵撕行——定标 v1 实踩）。
fn u3_selftest() {
    // 步 1（⓪）：pwd——boot 期 bsh cwd 恒根，render_cwd 空 wire 渲染
    // "/"（getcwd 根 wart C-1 已修；`/\n` musl 同形——无参 pwd 恒退 0）。
    match run_capture("pwd", None, &[]) {
        Some((o, c)) if c == 0 && o.data == b"/\n" && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u3_bad(1, c, &o),
        None => {
            eprintln!("bsh: u3 selftest bad: step=1 pwd not found");
            berry_libc::park();
        }
    }
    // 步 2（①）：cat 32B 双行 feed 全量回显（逐字节全等——含尾换行）。
    match run_capture("cat", Some(U3_FEED), &[]) {
        Some((o, c)) if c == 0 && o.data == U3_FEED && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u3_bad(2, c, &o),
        None => {
            eprintln!("bsh: u3 selftest bad: step=2 cat not found");
            berry_libc::park();
        }
    }
    // 步 3（②）：wc -c——字节计数真值 32（feed 常量面 32B 的字面钉板，
    // 宿主 wc -c 同源 =0.12.0）。
    match run_capture("wc", Some(U3_FEED), &["-c"]) {
        Some((o, c)) if c == 0 && o.data == b"32\n" && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u3_bad(3, c, &o),
        None => {
            eprintln!("bsh: u3 selftest bad: step=3 wc not found");
            berry_libc::park();
        }
    }
    // 步 4（⑤）：cut -c 2-3 | tr a-z A-Z——列裁剪 + 大小写映射双段管道
    // （abcd→BC / zz→Z / hello world→EL：短行越界取到即止）。
    let segs45 = [
        PipeSeg { name: "cut", args: &["-c", "2-3"] },
        PipeSeg { name: "tr", args: &["a-z", "A-Z"] },
    ];
    match pipe_capture(&segs45, Some(U3_CUT_FEED)) {
        Ok((o, c)) if c == 0 && o.data == b"BC\nZ\nEL\n" && o.eof && o.dropped == 0 => {}
        Ok((o, c)) => u3_bad(4, c, &o),
        Err(PipeFail::NotFound(n)) => {
            eprintln!("bsh: u3 selftest bad: step=4 {} not found", n);
            berry_libc::park();
        }
        Err(PipeFail::SpawnFail(n)) => {
            eprintln!("bsh: u3 selftest bad: step=4 {} spawn failed", n);
            berry_libc::park();
        }
    }
    // 步 5（⑥'）：cat | uniq | wc -l 三段链——首段开场 + 末段收口的三段
    // 结构面（门审原案四段链裁 sort 段，见头注挂账）；uniq 面喂七行无
    // 相邻重复（sort 缺席下 uniq 语义 = 逐行直通）→ wc -l 计 7。
    let segs5 = [
        PipeSeg { name: "cat", args: &[] },
        PipeSeg { name: "uniq", args: &[] },
        PipeSeg { name: "wc", args: &["-l"] },
    ];
    match pipe_capture(&segs5, Some(U3_SORT_FEED)) {
        Ok((o, c)) if c == 0 && o.data == b"7\n" && o.eof && o.dropped == 0 => {}
        Ok((o, c)) => u3_bad(5, c, &o),
        Err(PipeFail::NotFound(n)) => {
            eprintln!("bsh: u3 selftest bad: step=5 {} not found", n);
            berry_libc::park();
        }
        Err(PipeFail::SpawnFail(n)) => {
            eprintln!("bsh: u3 selftest bad: step=5 {} spawn failed", n);
            berry_libc::park();
        }
    }
    println!("U3 ok");
}

/// U3 feed 面（定标冻结；步③ head 的五行 feed 随挂账裁出）。
const U3_FEED: &[u8] = b"u3 feed: abcdefghij\nsecond line\n";
const U3_SORT_FEED: &[u8] = b"delta\nbravo\nalpha\ndelta\ncharlie\nbravo\nalpha\n";
const U3_CUT_FEED: &[u8] = b"abcd\nzz\nhello world\n";

/// U3 断言失败诊断：四面（真码/长度/EOF/丢弃）+ 实际字节 hex（截 64B）
/// **单串一次打**后 park——撕行防护（稳态矩阵并发写串口）+ 标记缺失 =
/// smoke FAIL 的 loud 兜底。
fn u3_bad(step: u8, c: i32, o: &RunOut) -> ! {
    let n = o.data.len().min(64);
    let s = format!(
        "bsh: u3 selftest bad: step={} code={} len={} eof={} dropped={} hex={}",
        step,
        c,
        o.data.len(),
        o.eof,
        o.dropped,
        u3_hex(&o.data[..n])
    );
    eprintln!("{}", s);
    berry_libc::park()
}

/// hex 渲染（小写两位——诊断/串口双可读；仅失败路径走）。
fn u3_hex(d: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(d.len() * 2);
    for b in d {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

/// B-1 自验（`B1 ok n=4` = rootfs 相第 28 标记——状态谓词 + 「统计归统计」
/// 的执法点）：spawn `bin/bench1`（EL0 四项微基准 sys_null/sys_vmo_rw4k/
/// ipc_enqueue8/evt_signal）经私有通道收齐 BENCH1 行 → **格式断言**（行数
/// 恰 4 + 名字-iters 对字面常量逐项对拍 + 行体 ≤191B（含 println 尾 \n 共
/// ≤192B 单消息原子）+ cyc 可解析非零 + eof/
/// dropped/code 干净）→ 全过才 relay 四行（每行单次 println，~35B 单消息
/// 原子——HEXYL ok 的化妆面模式；bench.py --el0 从串口收割的就是 relay 面；
/// **禁止拼一串打**：>192B 经 console 分片被稳态矩阵撕行，d-3c 雷）→ 打
/// 原子标记。**cyc 数值零断言**（公约 2——统计量只进 bench.py L1 宽松因子；
/// bsh 侧只验格式/形状，防「行在但缺项/多项」的空心绿）。失败诊断单串
/// format! 一次 eprintln（撕行防护，u3_bad 同款）+ park（标记缺失 → smoke
/// FAIL loud 兜底）。
///
/// 栈取舍：设计 §8 原文「8 页」——落地复用 run_capture（stack_pages=32，
/// uu 家族口径）：零新代码 + d-3c std 栈机器 32 页实踩背书（std format!
/// 栈深无虞），取舍注记见 docs/阶段沿革.md B-1 条。
fn bench1_selftest() {
    let (o, c) = match run_capture("bench1", None, &[]) {
        Some(r) => r,
        None => {
            eprintln!("bsh: b1 selftest bad: bench1 not found");
            berry_libc::park();
        }
    };
    // 整体形状：真码 0 + EOF 干净 + 无截断 + UTF-8 可解析。
    let text = match std::str::from_utf8(&o.data) {
        Ok(t) if c == 0 && o.eof && o.dropped == 0 => t,
        _ => b1_bad(&format!(
            "code={} eof={} dropped={} len={}",
            c,
            o.eof,
            o.dropped,
            o.data.len()
        )),
    };
    // 逐行：`BENCH1,<name>,<iters>,<cyc>`——name/iters 对字面常量钉板
    // （与 bench1 定值镜像；顺序 = bench1 打印序，确定性形状）。
    const WANT: [(&str, &str); 4] = [
        ("sys_null", "4096"),
        ("sys_vmo_rw4k", "128"),
        ("ipc_enqueue8", "128"),
        ("evt_signal", "1024"),
    ];
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() != WANT.len() {
        b1_bad(&format!("lines={} want={}", lines.len(), WANT.len()));
    }
    for (i, line) in lines.iter().enumerate() {
        let parts: Vec<&str> = line.split(',').collect();
        let (name, iters) = WANT[i];
        let ok = parts.len() == 4
            && parts[0] == "BENCH1"
            && parts[1] == name
            && parts[2] == iters
            && parts[3].parse::<u64>().map_or(false, |v| v > 0)
            // 公约 4：单行单消息原子——口径含 println 追加的 \n（console_write
            // 按 min(192, len) 分片）：行体 ≤191 ⇒ 行体+\n ≤192 恰一整条；
            // 行体恰 192 时 193B 被切 192+1 两条 = 分片撕行窗口（d-3c 雷）。
            && line.len() <= 191;
        if !ok {
            b1_bad(&format!("line={} want=BENCH1,{},{},<cyc>0", line, name, iters));
        }
    }
    // 断言全过才 relay + 打标（序 = BENCH1 行在前、标记收口——BENCH ok 同款）。
    for line in &lines {
        println!("{}", line);
    }
    println!("B1 ok n=4");
}

/// B1 断言失败：单串一次 eprintln + park（u3_bad 同款撕行防护）。
fn b1_bad(diag: &str) -> ! {
    eprintln!("bsh: b1 selftest bad: {}", diag);
    berry_libc::park()
}

/// U4 feed 面（定标冻结——期望值全字面常量，宿主 =0.12.0 同源钉板）。
const U4_LINES: &[u8] = b"l1\nl2\nl3\nl4\nl5\n";
const U4_ABC: &[u8] = b"abcdefghijkl";
const U4_GREP_FILE: &[u8] = b"alpha\nbravo\ncharlie\nbravo\n";

/// U4 自验（C-2c 挂账刀，`U4 ok` = rootfs 相第 30 标记）：深坑六件
/// tail/env/date/grep/find/which 的**执行面**验收——八步全字面常量逐字节
/// 对拍（私有通道收齐，零串口 grep 面；spawn 合计 12：tail/head/env/date
/// 各 1 + grep 4（-n 主体 + -v 双态 + -q 优先级真码回归钉板——三镜
/// 审查修复）+ 管道 cat|grep 2 + which/find 各 1）。链序契约同
/// U2/U3/B1——排 fat 族之前（EFI 路无盘亦打 U4，smoke-image ECO 同面；
/// fs 步走 RAM 树 + run_capture_fs gift 四元，/etc/mtab 由 fsmgr 启动
/// 预建 EFI 路同在）。
/// 步② 弱断言锚 environ=NULL（berry-libc 空环境档）：uu_env 无命令面
/// print_all_env_vars 遍历 environ——NULL 表零行 + setenv ok! 桩不写真
/// 状态 → 恒空出（setenv/environ 落真时本步常量改 "U4V=zz\n" 收编设值
/// 打印面）。步① head -c 是 fd 克隆面的端到端活证（stdin
/// try_clone_to_owned → fcntl F_DUPFD_CLOEXEC → 别名槽 → 同一条流读回
/// ；stream_position 撞 ESPIPE 走缓冲回退路——Segment 2 设计闭环）。
/// 步④ 期望值按自写 grep 实测口径（FILE 参数恒带名前缀——GNU 单文件
/// 无前缀是其差异面，grep 头注语义子集口径内；不按 GNU 字面硬凑）。
/// fstree 容量窗：fs_selftest 残留 t64.bin/h.hex + etc/mtab 在册，U4 峰值
/// 再 +u4.txt +bin +bin/wc（FILE_COUNT_MAX=9 / DIR_COUNT_MAX=17 内），
/// 步④⑥ 用毕即清（容量纪律——残留会挤压后续 dir/cd 自验窗口）。
fn u4_selftest() {
    // 步 ⓪：tail -n 2——末两行（-f 尾随链 v1 无实现，运行期不可达）。
    match run_capture("tail", Some(U4_LINES), &["-n", "2"]) {
        Some((o, c)) if c == 0 && o.data == b"l4\nl5\n" && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(0, c, &o),
        None => u4_fail(0, "tail not found"),
    }
    // 步 ①：head -c 8——fd 克隆活证（别名槽读回与直读 fd 0 同一条流）。
    match run_capture("head", Some(U4_ABC), &["-c", "8"]) {
        Some((o, c)) if c == 0 && o.data == b"abcdefgh" && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(1, c, &o),
        None => u4_fail(1, "head not found"),
    }
    // 步 ②：env U4V=zz——弱断言（空出 + 真码 0；environ=NULL 锚，见头注）。
    match run_capture("env", None, &["U4V=zz"]) {
        Some((o, c)) if c == 0 && o.data.is_empty() && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(2, c, &o),
        None => u4_fail(2, "env not found"),
    }
    // 步 ③：date -u -d @0——epoch 渲染字面钉板（clock_getres 谎报 1ns 成功
    // 桩的消费面；--resolution/-s 运行期不可达）。
    match run_capture("date", None, &["-u", "-d", "@0"]) {
        Some((o, c)) if c == 0 && o.data == b"Thu Jan  1 00:00:00 UTC 1970\n"
            && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(3, c, &o),
        None => u4_fail(3, "date not found"),
    }
    // 步 ④：grep -n bravo /u4.txt——fs gift 面（文件参数段：open/read 经
    // run_capture_fs 四元装配；见头注名前缀口径）。
    if let Err(e) = put_file("u4.txt", U4_GREP_FILE) {
        u4_fail(4, &format!("put u4.txt {}", errno_text(e)));
    }
    match run_capture_fs("grep", None, &["-n", "bravo", "/u4.txt"]) {
        Some((o, c)) if c == 0 && o.data == b"/u4.txt:2:bravo\n/u4.txt:4:bravo\n"
            && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(4, c, &o),
        None => u4_fail(4, "grep not found"),
    }
    // 步 ④（续）：grep 真码面回归钉板（三镜审查修复的防回归断言）——
    // -v 双态（GNU：选中行存在 → 0 / 零选中 → 1；旧 `any != inv` 恰两态
    // 反转）+ -q 优先级（-q 命中 → 0 压过文件错 2；stderr 诊断同通道进
    // 比对面——/nope 打开错文案即期望字节的一部分）。
    match run_capture_fs("grep", None, &["-n", "-v", "bravo", "/u4.txt"]) {
        Some((o, c)) if c == 0 && o.data == b"/u4.txt:1:alpha\n/u4.txt:3:charlie\n"
            && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(4, c, &o),
        None => u4_fail(4, "grep not found"),
    }
    match run_capture_fs("grep", None, &["-v", "a", "/u4.txt"]) {
        Some((o, c)) if c == 1 && o.data.is_empty() && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(4, c, &o),
        None => u4_fail(4, "grep not found"),
    }
    match run_capture_fs("grep", None, &["-q", "bravo", "/nope", "/u4.txt"]) {
        Some((o, c)) if c == 0 && o.data == b"grep: /nope: not readable\n"
            && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(4, c, &o),
        None => u4_fail(4, "grep not found"),
    }
    if let Err(e) = unlink_of("u4.txt") {
        u4_fail(4, &format!("unlink u4.txt {}", errno_text(e)));
    }
    // 步 ⑤：cat | grep -c bravo——管道面（首段纯转发 + 末段 stdin 回落 +
    // 计数旗标；fs gift 骑首段结构约束与 cat 零 fs 消费不冲突）。
    let segs5 = [
        PipeSeg { name: "cat", args: &[] },
        PipeSeg { name: "grep", args: &["-c", "bravo"] },
    ];
    match pipe_capture(&segs5, Some(U4_GREP_FILE)) {
        Ok((o, c)) if c == 0 && o.data == b"2\n" && o.eof && o.dropped == 0 => {}
        Ok((o, c)) => u4_bad(5, c, &o),
        Err(PipeFail::NotFound(n)) => u4_fail(5, &format!("{} not found", n)),
        Err(PipeFail::SpawnFail(n)) => u4_fail(5, &format!("{} spawn failed", n)),
    }
    // 步 ⑥：which wc——PATH 空 → 默认 /bin（存在性判据，v1 mode 恒 0644
    // 无 x 位的头注偏差）；自建 /bin/wc 验证定位，用毕即清。
    if let Err(e) = mkdir_of("bin") {
        u4_fail(6, &format!("mkdir bin {}", errno_text(e)));
    }
    if let Err(e) = put_file("bin/wc", b"x\n") {
        u4_fail(6, &format!("put bin/wc {}", errno_text(e)));
    }
    match run_capture_fs("which", None, &["wc"]) {
        Some((o, c)) if c == 0 && o.data == b"/bin/wc\n" && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(6, c, &o),
        None => u4_fail(6, "which not found"),
    }
    if let Err(e) = unlink_of("bin/wc") {
        u4_fail(6, &format!("unlink bin/wc {}", errno_text(e)));
    }
    if let Err(e) = unlink_of("bin") {
        u4_fail(6, &format!("rmdir bin {}", errno_text(e)));
    }
    // 步 ⑦：find /etc -name mt*——递归走查 + GLOB 谓词（opendir/readdir/
    // stat 全 fs 面；/etc/mtab fsmgr 启动预建——EFI 路同在）。
    match run_capture_fs("find", None, &["/etc", "-name", "mt*"]) {
        Some((o, c)) if c == 0 && o.data == b"/etc/mtab\n" && o.eof && o.dropped == 0 => {}
        Some((o, c)) => u4_bad(7, c, &o),
        None => u4_fail(7, "find not found"),
    }
    println!("U4 ok");
}

/// U4 断言失败诊断（u3_bad 同款：四面 + 实际字节 hex 截 64B 单串一次打
/// + park——撕行防护 + 标记缺失 = smoke FAIL loud 兜底）。
fn u4_bad(step: u8, c: i32, o: &RunOut) -> ! {
    let n = o.data.len().min(64);
    let s = format!(
        "bsh: u4 selftest bad: step={} code={} len={} eof={} dropped={} hex={}",
        step,
        c,
        o.data.len(),
        o.eof,
        o.dropped,
        u3_hex(&o.data[..n])
    );
    eprintln!("{}", s);
    berry_libc::park()
}

/// U4 档未命中 / IO 面失败（put/unlink/mkdir/段拉起）：单串 + park。
fn u4_fail(step: u8, what: &str) -> ! {
    eprintln!("bsh: u4 selftest bad: step={} {}", step, what);
    berry_libc::park()
}

/// 拉起 + 收集（自验/交互共用）：查档 → spawn → collect → 回收 → 关对端。
/// None = 档未命中（调用方 not found）；Some((RunOut, code)) 的 code = 子
/// 真退出码（reap Err → 1——P5 `&&`/`$?` 真码源；spawn Err 仍 park = 自验
/// 面 loud 死，交互 not-found 面在 do_run 不经此）。
fn run_capture(name: &str, stdin: Option<&[u8]>, args: &[&str]) -> Option<(RunOut, i32)> {
    run_capture_gifts(name, stdin, args, &[])
}

/// run_capture 的 fs 能力版（C-2 刀 2）：df/du 子进程经 libc `statfs`/
/// `open`/`stat` 消费 fs 服务——U 家族先例全零 fs 面（stdin/stdout 纯管道
/// ），`extra_gifts: &[]` 装配下子进程 `fs_h()==0` → libc open 降级
/// NoFs→ENOENT（首跑实抓：df 挂载表两级回落全 ENOENT，形态与「文件缺失
/// 」不可区分——open_core 的 NoFs 降级锚）。gift 排布与 bsh 自身同款
/// （svcmgr 给 bsh 的四元）：gift[2] = rootfs VMO READ dup、gift[3] = fs
/// 通道 SEND|RECV dup——libc `fs_h()` 恰读 env hs[3]。dup 别名随 env send
/// **take 迁移**（成功路径零泄漏；失败 park 不回收）。严格串行 RPC 会话
/// 纪律不破：df/du 生命周期内它是唯一活动 fs client（bsh 阻在 collect/
/// reap，不发 fs 请求）。
/// fs gift 四元装配（C-2c 挂账刀——挂账文档 §② 修法落地：三个存量
/// spawn 点 run_output/pipe_capture 接线，样板 = svcmgr `uu_run` 与下方
/// `run_capture_fs`）：gift[2] = rootfs VMO READ dup、gift[3] = fs 通道
/// SEND|RECV dup。**berry-exec take 契约**：spawn 成功则两 dup 随 env
/// send 全被 take（本表副本使命即毕，成功路径零泄漏）；spawn Err（全部
/// 早退点在 env send 前）= 未 take 留本表，调用方须显式补关双 dup
///（Err 面清理见各调用点）。None = fs 服务缺席或 dup 失败——交互面调用
/// 方降级裸 spawn（零漂移，无 fsmgr 不死 shell）；自验面走 run_capture_fs
/// 的 park。**v1 约束：单管道至多一个 fs 消费段**——共享单会话 fs 通道
/// 上双消费者 = 应答偷取/会话错位，故 pipe_capture 只给首段装配（末段
/// fs 消费 = 文件参数段的组合随 v2）。
fn fs_gifts() -> Option<(Handle, Handle)> {
    let mut rootfs_dup = 0 as Handle;
    let mut fs_dup = 0 as Handle;
    if berry_handle_duplicate(rootfs_h(), RIGHT_READ, &mut rootfs_dup) != 0
        || berry_handle_duplicate(fs_h(), RIGHT_SEND | RIGHT_RECV, &mut fs_dup) != 0
    {
        // 半途失败：可能已成一枚——两枚都试关（未成的关返 -EBADF 无害）。
        let _ = berry_handle_close(rootfs_dup);
        let _ = berry_handle_close(fs_dup);
        return None;
    }
    Some((rootfs_dup, fs_dup))
}

fn run_capture_fs(name: &str, stdin: Option<&[u8]>, args: &[&str]) -> Option<(RunOut, i32)> {
    let (rootfs_dup, fs_dup) = match fs_gifts() {
        Some(g) => g,
        None => {
            // fs/rootfs gift 缺席或 dup 失败 = 链断级（fs_selftest 已过则
            // 结构性不可达；EFI 路 fat park 挡在前——自验面 loud 不静默）。
            eprintln!("bsh: run capture: fs gift dup failed");
            berry_libc::park();
        }
    };
    // gift[2] 前要求 stdin 在位（berry-exec 断言）——空 feeder 占位（EOF
    // 留队无害：df/du 不读 stdin，fsmgr dummy 同款）。
    let stdin = stdin.or(Some(&[] as &[u8]));
    run_capture_gifts(name, stdin, args, &[rootfs_dup, fs_dup])
}

fn run_capture_gifts(
    name: &str,
    stdin: Option<&[u8]>,
    args: &[&str],
    gifts: &[Handle],
) -> Option<(RunOut, i32)> {
    let full = format!("bin/{}", name);
    let (data_off, data_len) = cpio_find(rootfs_h(), full.as_bytes())?;
    let (out_s, out_c) = channel_pair();
    let argv: Vec<&[u8]> = args.iter().map(|a| a.as_bytes()).collect();
    let spec = SpawnSpec {
        name: full.as_bytes(),
        data_off,
        data_len,
        stdout: out_c,
        stack_pages: 32,
        stdin_feed: stdin,
        stdin_handle: None,
        args: &argv,
        extra_gifts: gifts,
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
    RunOut { data, eof, dropped, killed: false }
}

/// C-4b 前台收集（Ctrl-C 杀链）：[`collect`] 的交互态扩展——空队等待从
/// `wait_one(out:READABLE)` 扩为 **wait_many 三项** `[out:READABLE,
/// proc:TERMINATED, stdin:READABLE]`（入参序 = 命中序：wait_many 返回
/// 入参序最前命中项，READABLE 恒排 TERMINATED 前——正常退出面 EOF 先行
/// 零漂移）。三分支：
/// - **idx 0（out 可读）**：数据到——回外层 recv（与 collect 同形）；
/// - **idx 1（TERMINATED 先于 EOF）**：子已死而无零长 EOF——**EOF 缺口面
///   （v1 边界）**：`process_terminate` 是内核杀**不走 libc exit 族**
///   （`send_stdout_eof` 在 exit/_exit/abort 内），被杀者 stdout 的收集方
///   **收不到 EOF**——收口只能靠本 wait 的 TERMINATED 项（不赌 10s 活动帽；
///   v1 channel 无对端关闭检测，EOF 语义补全挂 v2）。排空残留队列
///   后按 killed 收场；
/// - **idx 2（stdin 可读）**：排空 stdin 找 0x03——命中即
///   `process_terminate(×kill 集, 130)`（POSIX 128+SIGINT 惯例）+ 回显
///   `^C`；非 0x03 字节进 [`TYPEAHEAD`]（打字-ahead 不丢——交互循环先于
///   stdin 消化）。
/// 已死者的重复 terminate 返 -EBADSTATE，忽略（退出码首因不覆写）。
/// kill 集调用方给全（单段 run = 1 / 管道 = 全段）；hp = TERMINATED 盯
/// 哨（run = 同一 handle / 管道 = 末段——盯中段会在常态 pipe 上误断收）。
fn collect_fg(h: Handle, hp: Handle, kill: &[Handle]) -> RunOut {
    let sin = stdin_h();
    let mut data: Vec<u8> = Vec::new();
    let mut dropped = 0usize;
    let mut eof = false;
    let mut killed = false;
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
            break; // 非 -EAGAIN 错误：对端异常，按无 EOF 收场（collect 同款）
        }
        // 空队：三项等待（10s 活动帽，收包刷新）。stdin 缺席（老 spawn 面）
        // 退化两项（out + TERMINATED）——Ctrl-C 面不达但 EOF/TERMINATED
        // 收口仍完整。
        let hs = [h, hp, sin];
        let sigs = [SIGNAL_READABLE, SIGNAL_TERMINATED, SIGNAL_READABLE];
        let n = if sin != 0 { 3 } else { 2 };
        let (mut idx, mut obs) = (0usize, 0u64);
        if berry_object_wait_many(
            hs.as_ptr(),
            sigs.as_ptr(),
            n,
            now() + TIMEOUT_NS,
            &mut idx,
            &mut obs,
        ) != 0
        {
            break; // 活动帽尽：与 collect 同收场（eof=false 交调用方警告）
        }
        if idx == 2 {
            // stdin：排空找 0x03（console 逐字节 1B 消息、队列深 8——循环
            // 收到 -EAGAIN；超容量非 -EAGAIN 返码同面出循环）。
            let mut sb = [0u8; 64];
            loop {
                let mut snb = sb.len();
                let mut snh = 0usize;
                if berry_channel_recv(
                    sin,
                    sb.as_mut_ptr(),
                    &mut snb,
                    core::ptr::null_mut(),
                    &mut snh,
                ) != 0
                {
                    break;
                }
                for i in 0..snb {
                    if sb[i] == 0x03 {
                        println!("^C"); // Ctrl-C 回显（杀链起点；换行随 println）
                        for &p in kill {
                            // 已死者 -EBADSTATE 忽略——退出码首因不覆写。
                            let _ = berry_process_terminate(p, 130);
                        }
                        killed = true;
                    } else {
                        typeahead_push(sb[i]);
                    }
                }
            }
            continue; // 回外层：杀后 wait_many 撞 TERMINATED（idx 1）收口
        }
        if idx == 1 {
            // TERMINATED 先于 EOF：EOF 缺口面（头注）——排空残留后收场。
            // 残留里恰有零长 EOF（窄竞速窗内正常退出）也照收——eof 置真。
            killed = true;
            loop {
                let mut nb = msg.len();
                let mut nh = 0usize;
                let s = berry_channel_recv(
                    h,
                    msg.as_mut_ptr(),
                    &mut nb,
                    core::ptr::null_mut(),
                    &mut nh,
                );
                if s != 0 {
                    break;
                }
                if nb == 0 {
                    eof = true;
                    break;
                }
                let keep = nb.min(COLLECT_MAX.saturating_sub(data.len()));
                data.extend_from_slice(&msg[..keep]);
                dropped += nb - keep;
            }
            break;
        }
        // idx == 0：out 数据可读——外层 recv
    }
    RunOut { data, eof, dropped, killed }
}

/// now = clock_read（失败按当前时刻 0 处理 → 立即超时，活证死路 loud）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        berry_libc::park();
    }
    t
}
