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
//! v1 内建：help / echo / env / run / exit / cat / ls / mkdir / rm / cd / pwd / inputpoll（GUI-4 段② input 域 POLL 消费端）
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

use berry_libc::io::{close, fs_h, input_h, netd_h, read, rootfs_h, stdin_h, write};
use berry_libc::{
    EACCES, EEXIST, EIO, EISDIR, EINVAL, ENOENT, ENOSPC, ENOTDIR, ENOTEMPTY, EPERM,
};
use berry_libc::fs::{
    chdir, closedir, getcwd, mkdir, open, opendir, readdir, stat, statfs, unlink, O_CREAT,
    O_RDONLY, O_WRONLY, Stat, Statfs,
};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_event_create,
    berry_handle_close, berry_handle_duplicate, berry_object_signal, berry_object_wait_many,
    berry_object_wait_one, berry_process_terminate, berry_signal_send, Handle, BERRY_ERR_AGAIN,
    BERRY_ERR_BAD_STATE, BERRY_ERR_BUSY, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_RECV, RIGHT_SEND,
    SIGNAL_READABLE, SIGNAL_TERMINATED, SIGNAL_WRITABLE,
};
// A2-3：netd 会话化协议（EVS echo server 自验的 RPC 编解码面）。
use berry_netdproto as ndp;
// GUI-4 段②：GUI 输入协议面（INPUT_POLL 请求 / 事件批应答编解码）——
// inputpoll 消费端的真值断言用 crate 单源 decode，不自造第二套解析。
use berry_guiproto as gp;
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
    // —— A2-1：多路等待聚合器 guest 冒烟（`POLL ok` = rootfs 相第 37 标记）。
    //    链序契约同 U2/U3/TAB/INT——零 fs 盘面（纯内核 Event 对象 + 真内核
    //    wait_many 批切分），排 fat 族之前（EFI 路无盘亦打 POLL——smoke-image
    //    ECO canon 同面）。
    poll_selftest();
    // —— A2-3：事件循环 echo server 自验（`EVS ok n=2` = rootfs 相第 39
    //    标记，A2 整卡验收线）。**串行状态谓词零竞速**（4-5a 铁律——宿主
    //    evsclient 逐连接对拍，设计 §3.4）；netd gift 缺席（EFI/裸/无盘
    //    拓扑无 netdev）→ netd_h()==0 静默跳过零扰动。排 fat 族前（U2
    //    家族链序契约照旧——EFI 路本就静默跳过，链序家族一致性不变）。
    evs_selftest();
    // —— A3-3：socket fd 皮自验（`SKT ok n=3` = rootfs 相第 40 标记，A3
    //    整卡验收线）：evs 七步链同语义换 POSIX fd 入口——libc socket 族
    //    （A3-1 表/旗面 + A3-2 RPC 真身）第一次全链活体：client 段
    //    connect/send/recv（echod 回显）+ 非阻塞 EAGAIN 节拍 + server 段
    //    bind/listen/accept/尸槽回收/回发（宿主 evsclient 18083 SKT 对拍）。
    //    拓扑降级同 EVS（netd_h()==0 / ENODEV 静默跳过——EFI canon 零变化、
    //    B4 无 netdev 相不设门）；排 fat 族前零盘面。
    skt_selftest();
    // —— A4-3：epoll + DNS 自验（`EPL ok n=8` = rootfs 相第 41 标记，A4
    //    整卡验收线）：epoll 三桩真身 + getaddrinfo DNS 档第一次全链活体
    //    （create1 槽满/ctl 拒面全表/wait LT 复报/短帽/非 socket 拒/
    //    localhost→127.0.0.1/非法名本地拒/close 释槽复用七步）。拓扑降级
    //    同 EVS/SKT（netd_h()==0 / connect ENODEV 静默跳过——EFI canon
    //    零变化、B4 无 netdev 相不设门）；排 fat 族前零盘面。
    epl_selftest();
    // —— A5-2：std::net spawn 面自验（`NETS ok n=2` = rootfs 相第 42 标记，
    //    A5 整卡验收线）：net_gifts 三元装配（A5-1）+ std::net 深水面
    //    （TcpStream connect→write→echo 恰长对拍）第一次经 spawn gift 链
    //    全链活体（正步 10.0.2.2:18080 echod 对拍 + localhost 负控——SLIRP
    //    guest 环回不绕宿主 loopback 的定谳形状，A5-1 净室 probe 实录）。
    //    拓扑降级双门同 EPL/SKT（netd_h()==0 / connect ENODEV(19) 静默
    //    跳过——EFI canon 零变化、B4 无 netdev 相同口径）；排 fat 族前
    //    零盘面。
    nets_selftest();
    // —— A7-1/A7-3：mmap 匿名档 + 文件档 staging 自验（`MMP ok n=9`，
    //    A7-1 验收线 `docs/事件循环A7设计草案.md` §3.1 + A7-3 扩步
    //    `docs/事件循环A7-3实施设计草案.md`）：mmap/mprotect 真身
    //    （region 表 + vmo_create/整段 commit/vmar_map(self_vmar_h())
    //    桥 + 文件档 get_chunk 直 RPC staging + dormant 五臂）guest 全链
    //    活体——⓪-④ 匿名档五步（首映射/写读对拍/munmap 三态/再映射
    //    复用/范围盒拒面）+ ⑥-⑨ A7-3 四步（文件档私有拷贝证/拒位四连/
    //    fd 面 EBADF·EACCES/dormant 保内容环）。链序契约同 U2/POLL——
    //    ⑥-⑧ 走 RAM 树 fs 面（非盘面），排 fat 族之前（EFI 路无盘亦打
    //    MMP）；无拓扑降级门（T-1 自描述对 gift[3] 是 spawn 通用装配，
    //    所有相恒在——失败即 park loud）。
    mmp_selftest();
    // —— A7-2：timer 壁钟分档自验（`CLK ok n=6` = rootfs 相第 44 标记，
    //    `docs/事件循环A7-2实施设计草案.md` §4——设计稿「五步/n=5」系
    //    ⑤ 增补后计数未同步的勘误，实现定案六步，台账随刀更正）：libc
    //    clock 族分诊（mono 族直读 / REALTIME 族 = EPOCH_BASE(0)+mono
    //    合成 / 冷门 id EINVAL 诚实拒）+ 补桩五真身（time/gettimeofday/
    //    settimeofday/nanosleep/usleep）第一次 guest 全链活体。链序契约
    //    同 U2/POLL/MMP——零 fs 盘面（纯 clock syscall + `date -u` spawn
    //    无文件参），排 mmp 后 fat 族前（EFI 路无盘亦打 CLK）；无拓扑
    //    降级门（失败即 park loud）。
    clk_selftest();
    // —— A7-4 段④：信号面 guest 自验（`SIG ok n=6` = rootfs 相第 45 标记，
    //    `docs/事件循环A7-4信号面实施设计草案.md` §5）——libc signal.rs 真身
    //    （sigaction/sigprocmask 表面 + 协同式投递循环 kill/raise/pause）+
    //    内核 0x86/0x87 跨进程位面的六步链（⓪raise 即时投递 ①表/掩码回读
    //    ②blocked 窗 held→unblock 补投 ③kill 分诊 self/ESRCH/EINVAL
    //    ④spawn bin/sigchild 跨进程 SIGTERM→DFL exit(143) EOF 先行
    //    ⑤修复轮回归钉板——嵌套补投恰一次/delivery-time 表变更/包络
    //    恢复/无残留双投）。
    //    链序契约同 U2/MMP/CLK——零 fs 盘面（纯用户态信号面 + sigchild 零
    //    gift spawn），排 fat 族前（EFI 路无盘亦打 SIG）；无拓扑降级门
    //    （失败即 park loud）。
    sig_selftest();
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
    // —— C-5：M-G3 里程碑演示相自验（`M3DEMO ok n=5` = rootfs 相第 35
    //    标记，`VM里程碑设计草案.md` §2）。**排 fat 族后**（①步 ls /
    //    断言 HELLO.TXT 依赖挂载卷在位；原案五件之 ps 名单由 PS ok 承接
    //    ——拆解决策见 m3demo_selftest 头注）；EFI 路无盘 fat park
    //    （`-> !` 挡断其后自验）→ M3DEMO 结构性不打（smoke-image ECO
    //    canon 零变化，DF 同款契约）。
    m3demo_selftest(&mut vars, &mut status);

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
        const BUILTINS: [&[u8]; 12] = [
            b"help", b"exit", b"env", b"echo", b"run", b"cat", b"ls", b"mkdir", b"rm",
            b"cd", b"pwd", b"inputpoll",
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
        mmio_va: 0,
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

/// A2-1 guest 绑定：berry-poll `WaitMany` trait 的真 syscall 实现（host 13
/// 测之上——假实现盖算法面，本面盖「绑定 + 单位换算 + 真内核行为」）。
/// 毫秒（库面）↔ 纳秒（内核 Time）换算收口在此：deadline=0 原样直传（纯
/// poll 语义零特判），now() = clock_read(ns)/1e6。
struct SysWaitMany;

impl berry_poll::WaitMany for SysWaitMany {
    fn wait_many(
        &mut self,
        handles: &[u32],
        signals: &[u64],
        deadline_ms: u64,
    ) -> Result<(usize, u64), i32> {
        let (mut idx, mut obs) = (usize::MAX, 0u64);
        let s = berry_object_wait_many(
            handles.as_ptr(),
            signals.as_ptr(),
            handles.len(),
            deadline_ms.saturating_mul(1_000_000),
            &mut idx,
            &mut obs,
        );
        if s == 0 {
            Ok((idx, obs))
        } else {
            Err(s)
        }
    }

    fn now(&mut self) -> u64 {
        now() / 1_000_000
    }
}

/// A2-1 guest 冒烟（`POLL ok` = rootfs 相第 37 标记）：berry-poll 聚合器
/// 最小消费面。四步全确定性状态谓词（信号先行置位、wait 只读不消费——
/// 零竞速，P5 A-2 纪律；单线程无外部置位者，阻塞唤醒面由 host 测试
/// arm_wake 盖，本面盖批切分/收割/摘槽/超时收口）：
/// ⓪ N=20 Event 全空纯 poll → 0 事件（20 > WAIT_MANY_MAX=16 → 真批切分
///    16+4——wait_many n≤16 上限的翻越面就是本刀的存在理由）
/// ① 置位第 20 个（批 1 成员）→ 非阻塞快照恰 1 事件、slot==19、observed
///    ==用户位（跨批发现 + 内核全满足集回写口径）
/// ② 置位其余 19 → wait(500ms) poll-first 立即收满 20（阻塞形态命中即返
///    ——非睡中唤醒）；随后快照再报同 20（LT 免费复报——信号未消费）
/// ③ remove 槽 0 → 快照 19（摘槽语义）
/// ④ 清位全部 → wait(30ms) → Ok(空)（阻塞段超时收口——真睡 30ms 墙钟，
///    berry-poll 段帽/now 基准换算经真 clock 走通）
/// 失败 eprintln + park（POLL ok 缺失 → smoke FAIL 兜底）。
fn poll_selftest() {
    let mut fails: Vec<&str> = Vec::new();
    let mask: u64 = 1 << 32; // 用户半位（object_signal 只收 ≥1<<32——4-2 裁定）
    const N: usize = 20; // 批切分 16+4（WAIT_MANY_MAX 之上第一档）
    let mut evs = [0u32; N];
    for (i, e) in evs.iter_mut().enumerate() {
        if berry_event_create(0, e) != 0 || *e == 0 {
            eprintln!("bsh: poll selftest: event create {}", i);
            berry_libc::park();
        }
    }
    let mut ps = berry_poll::PollSet::new(SysWaitMany);
    for &e in evs.iter() {
        if ps.add(e, mask).is_err() {
            fails.push("⓪ add");
        }
    }
    // ⓪ 全空纯 poll：非阻塞快照零事件（跨批 = 每批各一次 -EAGAIN）。
    match ps.wait(0, 0) {
        Ok(o) if o.is_empty() => {}
        _ => fails.push("⓪ 全空快照非零"),
    }
    // ① 批 1 成员就绪：跨批发现 + observed 全满足集（恰 mask——Event 新建
    //    无杂位）。
    if berry_object_signal(evs[19], 0, mask) != 0 {
        fails.push("① signal");
    }
    match ps.wait(0, 0) {
        Ok(o) if o.len() == 1
            && o[0].slot == 19
            && o[0].handle == evs[19]
            && o[0].observed == mask => {}
        _ => fails.push("① 跨批快照面"),
    }
    // ② 全量置位 → wait(500ms) 收满 20（poll-first 即返）；LT 免费复报：信号
    //    未消费 → 同轮快照再报同 20 路（host lt_semantics_repoll_reports_again
    //    的 guest 面）。
    for &e in evs.iter().take(19) {
        if berry_object_signal(e, 0, mask) != 0 {
            fails.push("② signal");
        }
    }
    match ps.wait(500, now() / 1_000_000) {
        Ok(o) if o.len() == N => {
            let sum: usize = o.iter().map(|e| e.slot).sum();
            if sum != N * (N - 1) / 2 {
                fails.push("② 槽全集破缺");
            }
        }
        _ => fails.push("② 收割非满 20"),
    }
    match ps.wait(0, 0) {
        Ok(o) if o.len() == N => {}
        _ => fails.push("② LT 复报破缺"),
    }
    // ③ 摘槽 0：19 路。
    if !ps.remove(evs[0]) {
        fails.push("③ remove");
    }
    match ps.wait(0, 0) {
        Ok(o) if o.len() == N - 1 => {}
        _ => fails.push("③ 摘槽后非 19"),
    }
    // ④ 清位全部 → 阻塞超时收口：Ok(空)（30ms 真睡——段帽路径经真 clock）。
    for &e in evs.iter() {
        if berry_object_signal(e, mask, 0) != 0 {
            fails.push("④ clear");
        }
    }
    match ps.wait(30, now() / 1_000_000) {
        Ok(o) if o.is_empty() => {}
        _ => fails.push("④ 超时非空收场"),
    }
    // 收口：Event handle 全关（bsh 长命进程——有界泄漏也不留）。
    for &e in evs.iter() {
        if berry_handle_close(e) != 0 {
            fails.push("close");
        }
    }
    if !fails.is_empty() {
        eprintln!("bsh: poll selftest bad: {:?}", fails);
        berry_libc::park();
    }
    println!("POLL ok");
}

/// EVS 对拍载荷（A2-3；与 `tools/evsclient.py` 同源定值——两连接异型
/// 64B/24B，本侧断言 DRAIN 收割逐字节、client 侧断言回读逐字节，两侧
/// 独立成立）。
const EVS_PAYLOAD_0: [u8; 64] = {
    let mut a = [0u8; 64];
    let mut i = 0;
    while i < 64 {
        a[i] = (i * 7 + 3) as u8;
        i += 1;
    }
    a
};
const EVS_PAYLOAD_1: [u8; 24] = {
    let mut a = [0u8; 24];
    let mut i = 0;
    while i < 24 {
        a[i] = (i * 11 + 5) as u8;
        i += 1;
    }
    a
};

/// A2-3 netd RPC 底座：严格串行（请求-应答一一对应、无请求号——P6 A-6
/// 纪律：**超时不重发**，晚到应答滞留队列只会毒化下一请求）。
/// encode → send（满队 -EBUSY 阻塞等 WRITABLE 重试——libc console 同款；
/// bsh 是 netd 队唯一发送者、串行 RPC 稳态至多 1 深在途）→ 应答阻塞帽
/// 等 READABLE → recv 原长（应答形随 op 三分 Sid/Poll/Tcp，恰长 parse 归
/// 调用方）。失败返 None——EVS 无降级续走（loud 死 = EVS ok 缺失 → smoke
/// FAIL 兜底）。cap 按服务端 worst 路径 + 余量定值（P6 A-6「服务慢 ≠
/// 服务死」：vnet RT/CLOSE 内帽 30s → client 侧 45s；快 op 10s）。
fn netd_rpc(netd: u32, req: ndp::TcpSReq<'_>, session: u32, cap_ns: u64) -> Option<Vec<u8>> {
    let mut buf = [0u8; ndp::REQ_S_LEN_MAX];
    let n = ndp::encode_tcp_s_request(&mut buf, req, session)?;
    let send_dl = now() + 10_000_000_000;
    loop {
        let s = berry_channel_send(netd, buf.as_ptr(), n, core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            return None; // 链断面（非满队）——非慢而活，loud 死
        }
        let mut obs = 0u64;
        if berry_object_wait_one(netd, SIGNAL_WRITABLE, send_dl, &mut obs) != 0 {
            return None;
        }
    }
    let reply_dl = now() + cap_ns;
    loop {
        let mut msg = [0u8; ndp::TCP_REP_MAX];
        let mut nb = msg.len();
        let mut nhd = 0usize;
        let s = berry_channel_recv(
            netd,
            msg.as_mut_ptr(),
            &mut nb,
            core::ptr::null_mut(),
            &mut nhd,
        );
        if s == 0 {
            return Some(msg[..nb].to_vec());
        }
        if s != BERRY_ERR_AGAIN {
            return None; // 坏 handle 类链断——立即死不空等
        }
        let mut obs = 0u64;
        if berry_object_wait_one(netd, SIGNAL_READABLE, reply_dl, &mut obs) != 0 {
            return None; // 帽到无应答 = 断链面（不重发——见头注）
        }
    }
}

/// netd_rpc 的 Tcp 形便捷面：parse 产物即刻拷贝成 owned（`TcpReply` 借用
/// 应答缓冲——闭包内借用无法穿透 and_then 返回）。
fn netd_rpc_tcp(
    netd: u32,
    req: ndp::TcpSReq<'_>,
    session: u32,
    cap_ns: u64,
) -> Option<(u8, Vec<u8>)> {
    netd_rpc(netd, req, session, cap_ns)
        .and_then(|b| ndp::parse_tcp_reply(&b).map(|r| (r.st, r.data.to_vec())))
}

/// GUI-4 段② input RPC 底座：netd_rpc 逐行镜像（严格串行：请求-应答一一
/// 对应、无请求号——超时不重发，晚到应答滞留队列只会毒化下一请求）。
/// encode（POLL 恰 4B——`encode_input_poll_req` 断言面在 guiproto crate 内
/// 已执，max_events 域 0 非法即调用方 bug）→ send（满队 -EBUSY 阻塞等
/// WRITABLE 10s 重臂）→ recv（空队 -EAGAIN 帽内等 READABLE 重臂——应答
/// 两形 1B 错误态 / 事件批，恰长 parse 归调用方）→ `Option<Vec<u8>>`。帽
/// 35s **> 服务端 POLL_BLOCK_CAP_NS 30s**（gui-inputd serve_poll 定值）——
/// 「client 帽必须大于服务帽」纪律（服务慢 ≠ 服务死，A71/A76 族）。失败
/// 返 None → inputpoll loud 面（标记缺失 → smoke FAIL 兜底）。
fn input_rpc(input: u32, max_events: u8, cap_ns: u64) -> Option<Vec<u8>> {
    let mut buf = [0u8; 4];
    let n = gp::encode_input_poll_req(&mut buf, max_events);
    let send_dl = now() + 10_000_000_000;
    loop {
        let s = berry_channel_send(input, buf.as_ptr(), n, core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            return None; // 链断面（非满队）——非慢而活，loud 死
        }
        let mut obs = 0u64;
        if berry_object_wait_one(input, SIGNAL_WRITABLE, send_dl, &mut obs) != 0 {
            return None;
        }
    }
    let reply_dl = now() + cap_ns;
    loop {
        let mut msg = [0u8; gp::INPUT_REP_MAX];
        let mut nb = msg.len();
        let mut nhd = 0usize;
        let s = berry_channel_recv(
            input,
            msg.as_mut_ptr(),
            &mut nb,
            core::ptr::null_mut(),
            &mut nhd,
        );
        if s == 0 {
            return Some(msg[..nb].to_vec());
        }
        if s != BERRY_ERR_AGAIN {
            return None; // 坏 handle 类链断——立即死不空等
        }
        let mut obs = 0u64;
        if berry_object_wait_one(input, SIGNAL_READABLE, reply_dl, &mut obs) != 0 {
            return None; // 帽到无应答 = 断链面（不重发——见头注）
        }
    }
}

/// GUI-4 段② §4.2 真值表单行（smoke RX 注入相三批 → 泵三事件的期值面；
/// 坐标字面量经 PointerPump `normalize_axis` 两 ABS 域（0..32767 /
/// 0..32768）推演同值——ScreenGeom 800×600 整除定值，首跑以
/// `gui-inputd: up … cal=` 实录核定，段②草案 §7-①）。
struct WantEv {
    name: &'static str,
    phase: u8,
    buttons: u8,
    /// 期望按位含的 flags 位（0 = 不检）。三行真值全 0：泵只对 CANCEL
    /// 置合成位（gui-inputd lib.rs:606/588 同型），零采样批合成的 UP 事件
    /// flags 恒 0——草案 §4.2 行③初稿误写 ∋SYNTHESIZED，审查轮纠正。
    flags_has: u8,
    x: u32,
    y: u32,
    history_len: usize,
    /// history[0] 期值 (x, y)——history_len ≥ 1 时才检。
    hist0: Option<(u32, u32)>,
}

/// 注入三批期值（段②草案 §4.2 真值表逐字段）：① DOWN@中点归一
/// (399,299) 无历史；② MOVE 末采样 (599,149) 带 1 历史 (199,449)（首
/// 采样对）；③ UP@最后已知位 (599,149) 零采样 → 泵合成（flags=0——泵仅
/// 对 CANCEL 置合成位）。pointer_id 恒 0 共面检（见 [`WantEv::diff`]）。
const INPUTP_WANT: [WantEv; 3] = [
    WantEv {
        name: "DOWN",
        phase: gp::PHASE_DOWN,
        buttons: gp::BUTTON_L,
        flags_has: 0,
        x: 399,
        y: 299,
        history_len: 0,
        hist0: None,
    },
    WantEv {
        name: "MOVE",
        phase: gp::PHASE_MOVE,
        buttons: gp::BUTTON_L,
        flags_has: 0,
        x: 599,
        y: 149,
        history_len: 1,
        hist0: Some((199, 449)),
    },
    WantEv {
        name: "UP",
        phase: gp::PHASE_UP,
        buttons: 0,
        flags_has: 0,
        x: 599,
        y: 149,
        history_len: 0,
        hist0: None,
    },
];

impl WantEv {
    /// 逐字段真值检（pointer_id 恒 0 三事件共面检——真值表同列）：不符
    /// 返回差异面首处（"bsh: inputpoll: ev<i>: <差异面>" loud 行的载荷源）。
    fn diff(&self, ev: &gp::PointerEvent) -> Option<String> {
        if ev.phase != self.phase {
            return Some(format!("phase={} want {}", ev.phase, self.phase));
        }
        if ev.buttons != self.buttons {
            return Some(format!("buttons={} want {}", ev.buttons, self.buttons));
        }
        if ev.pointer_id != 0 {
            return Some(format!("pointer_id={} want 0", ev.pointer_id));
        }
        if ev.x != self.x {
            return Some(format!("x={} want {}", ev.x, self.x));
        }
        if ev.y != self.y {
            return Some(format!("y={} want {}", ev.y, self.y));
        }
        if ev.history_len != self.history_len {
            return Some(format!(
                "history_len={} want {}",
                ev.history_len, self.history_len
            ));
        }
        if let Some((hx, hy)) = self.hist0 {
            if ev.history_len == 0 {
                return Some(format!("history 缺席 want ({},{})", hx, hy));
            }
            let s = &ev.history[0];
            if s.x != hx || s.y != hy {
                return Some(format!(
                    "history[0]=({},{}) want ({},{})",
                    s.x, s.y, hx, hy
                ));
            }
        }
        if ev.flags & self.flags_has != self.flags_has {
            return Some(format!("flags={:#x} 缺位 {:#x}", ev.flags, self.flags_has));
        }
        None
    }
}

/// GUI-4 段②：inputpoll——INPUT_POLL(max_events=3) RPC → 三事件真值
/// 断言 → `INPUTP ok n=3`。
/// - input_h()==0（inputd 未起：裸/B4/EFI）→ "input: absent" + status 1
/// - 应答 ST_EAGAIN（空态合法——手跑未注入时）→ "inputpoll: no events"
///   + status 0
/// - count==3 且段② §4.2 真值表逐字段全中 → 逐事件诊断行 +
///   `INPUTP ok n=3` + status 0
/// - 任何不符 → "bsh: inputpoll: <差异面>" + status 1（loud）
/// 输出纪律（A60）：writeln 单行（≤192B 原子消息）、标记行单发。
fn inputpoll_output(out: &mut std::io::Stdout) -> (Vec<u8>, i32) {
    let h = input_h();
    if h == 0 {
        writeln!(out, "input: absent").unwrap();
        return (Vec::new(), 1);
    }
    let rep = match input_rpc(h, gp::INPUT_EVENTS_MAX as u8, 35_000_000_000) {
        Some(r) => r,
        None => {
            writeln!(out, "bsh: inputpoll: rpc 无应答").unwrap();
            return (Vec::new(), 1);
        }
    };
    let view = match gp::parse_input_events_rep(&rep) {
        Some(Ok(v)) => v,
        Some(Err(gp::ST_EAGAIN)) => {
            // 空态合法（手跑未注入：serve_poll 30s 帽到点 ST_EAGAIN）。
            writeln!(out, "inputpoll: no events").unwrap();
            return (Vec::new(), 0);
        }
        Some(Err(st)) => {
            writeln!(out, "bsh: inputpoll: st={}", st).unwrap();
            return (Vec::new(), 1);
        }
        None => {
            writeln!(out, "bsh: inputpoll: 畸形应答").unwrap();
            return (Vec::new(), 1);
        }
    };
    if view.count() != INPUTP_WANT.len() {
        writeln!(
            out,
            "bsh: inputpoll: n={} want {}",
            view.count(),
            INPUTP_WANT.len()
        )
        .unwrap();
        return (Vec::new(), 1);
    }
    let mut evs: Vec<gp::PointerEvent> = Vec::with_capacity(INPUTP_WANT.len());
    for i in 0..INPUTP_WANT.len() {
        // parse 已整体校验事件流——event(i) None 结构性不可达（防御面）。
        let Some(ev) = view.event(i) else {
            writeln!(out, "bsh: inputpoll: ev{} 缺席", i).unwrap();
            return (Vec::new(), 1);
        };
        evs.push(ev);
    }
    for (i, w) in INPUTP_WANT.iter().enumerate() {
        if let Some(d) = w.diff(&evs[i]) {
            writeln!(out, "bsh: inputpoll: ev{}: {}", i, d).unwrap();
            return (Vec::new(), 1);
        }
    }
    // 全中：逐事件诊断行（坐标字面量只能来自协议应答的渲染——A79）+
    // 标记行单发（A60）。
    for (i, w) in INPUTP_WANT.iter().enumerate() {
        let ev = &evs[i];
        let mut line = format!(
            "inputpoll: #{} {} phase={} btn={} id={} x={} y={} hist={}",
            i, w.name, ev.phase, ev.buttons, ev.pointer_id, ev.x, ev.y, ev.history_len
        );
        if ev.history_len > 0 {
            line.push_str(&format!(" h0=({},{})", ev.history[0].x, ev.history[0].y));
        }
        if ev.flags != 0 {
            line.push_str(&format!(" flags={:#x}", ev.flags));
        }
        writeln!(out, "{}", line).unwrap();
    }
    writeln!(out, "INPUTP ok n={}", evs.len()).unwrap();
    (Vec::new(), 0)
}

/// EVS 事件循环轮询节拍的纯 sleep：永不置位 Event + 用户位 wait 帽（v1
/// 无 sleep syscall——wait 帽即定时器；SLEEP_OBJ 形态的用户态版）。
fn sleep_ms(ev: u32, mask: u64, ms: u64) {
    let mut obs = 0u64;
    let _ = berry_object_wait_one(ev, mask, now() + ms * 1_000_000, &mut obs);
}

/// A2-3 事件循环 echo server 自验（`EVS ok n=2` = rootfs 相第 39 标记，
/// A2 整卡验收线）：guest 内 netd LISTEN(18082) 会话 + 拉 POLL 事件循环
/// （推模型留 v2——设计 §A2-2「串行非竞速面轮询无伤」；1s 节拍），宿主
/// `tools/evsclient.py` 两连接 64B/24B 异型载荷**逐连接串行**对拍（4-5a
/// 铁律：串行 = 状态谓词零竞速）。生命周期：LISTEN → 每连接 {accept-wait
/// → data-wait（CLOSED 旗 loud）→ DRAIN 收割逐字节对拍 → RT 回发（ACK
/// 帽内 ST_OK）→ CLOSE(dsid)} ×2 → CLOSE(lsid) 撤表。netd gift 缺席
/// （EFI/裸/无盘拓扑无 netdev）→ netd_h()==0 静默跳过；gift 在位而
/// LISTEN 答 ST_NODEV（vnet deviceless serve——MMIO 窗在、netdev 缺席
/// 的 B4 掉电相拓扑）→ 同口径静默跳过（拓扑真值，与装配侧同源）。失败
/// eprintln + park（EVS ok 缺失 → smoke FAIL 兜底）。
fn evs_selftest() {
    let netd = netd_h();
    if netd == 0 {
        return;
    }
    // 轮询节拍专用 Event（永不置位——wait 帽即纯定时器；用户半位 4-2 裁定）。
    let mut tick = 0u32;
    if berry_event_create(0, &mut tick) != 0 || tick == 0 {
        berry_libc::park();
    }
    let sleep_mask: u64 = 1 << 32;
    macro_rules! bail {
        ($($a:tt)*) => {{
            eprintln!($($a)*);
            berry_libc::park();
        }};
    }
    // ⓪ LISTEN(18082) → lsid（hostfwd 专线 18082——LSN 自验 18081 零交集）。
    let rep = netd_rpc(netd, ndp::TcpSReq::Listen { port: 18082 }, 0, 10_000_000_000)
        .and_then(|b| ndp::parse_listen_reply(&b));
    let lsid = match rep {
        Some(r) if r.st == ndp::ST_OK && r.sid != 0 => r.sid,
        // 无 NIC 拓扑（A2-3：MMIO 窗在而 netdev 缺席——B4 掉电相；vnet
        // deviceless serve 应答）→ 静默跳过，与 netd_h()==0 同口径（EVS
        // ok 结构性缺席 = 拓扑真值）。
        Some(r) if r.st == ndp::ST_NODEV => return,
        other => bail!("bsh: evs: listen bad {:?}", other.map(|r| (r.st, r.sid))),
    };
    let mut n_ok = 0u32;
    for (ci, want) in [EVS_PAYLOAD_0.as_slice(), EVS_PAYLOAD_1.as_slice()]
        .iter()
        .enumerate()
    {
        let want: &[u8] = want;
        // 尸槽复活预算（A3-3 首跑实抓，④ 注释详案）：每次复活重入本槽
        // 整链（acc_dl 随之刷新）；帽 3 = client 重试环 ~94s 周期（S-2b §5.4
        // 帽 ×2 后）取 3 条仍未命中活连接 = 真异常 loud（无界复活吃穿相帽，
        // 更糟）。
        let mut revives = 0u32;
        'conn: loop {
            // ① accept-wait + ② data-wait 合并循环：EAGAIN = 暂无挂起连接
            //    （client 侧重试环在外侧）→ 1s 节拍重拉；帽 240s（client 总帽
            //    480s 内必达；S-2b §5.4 ×2——smp 线 client 总帽 960s 同序）。
            //
            // **尸槽回收**（A2-3 首跑实抓 `c0 echo bad Some(1)`）：client 重试环
            //    在 guest LISTEN 落位前后的失败尝试（SLIRP 的 SYN 重传才落进被
            //    动打开）留下「握手已成 + 载荷已到 + 对端 recv 帽尽已 FIN」的
            //    CloseWait 尸槽——accept 照样弹出（非 Closed 即弹）、drain 对拍
            //    照样相等（同公式载荷），但 ④ RT 回发被 berry-tcp 的
            //    Established-only `send()` 拒 → ST_BAD。修法 = data-wait 门收紧
            //    到「WRITABLE（=Established）且有数据」才进 echo；CLOSED 旗或
            //    「带据无 WRITABLE」= 尸槽 → CLOSE 释槽（帽到自释，结果不敏感
            //    ——死对端 RST 被 berry-tcp 忽略，恒走 60s 帽释槽路）→ 重入
            //    accept-wait 取下一条（accept 稳态 1s 内必达活连接）。
            let acc_dl = now() + 240_000_000_000;
            let dsid = 'acc: loop {
                let d = loop {
                    match netd_rpc(netd, ndp::TcpSReq::Accept, lsid, 10_000_000_000)
                        .and_then(|b| ndp::parse_accept_reply(&b))
                    {
                        Some(r) if r.st == ndp::ST_OK && r.sid != 0 => break r.sid,
                        Some(r) if r.st == ndp::ST_EAGAIN => {}
                        other => bail!(
                            "bsh: evs: accept c{} bad {:?}",
                            ci,
                            other.map(|r| (r.st, r.sid))
                        ),
                    }
                    if now() >= acc_dl {
                        bail!("bsh: evs: accept c{} timeout", ci);
                    }
                    sleep_ms(tick, sleep_mask, 1000);
                };
                // data-wait：WRITABLE + rbytes>0 才是活连接；尸槽释槽重入。
                let dat_dl = now() + 60_000_000_000;
                loop {
                    match netd_rpc(netd, ndp::TcpSReq::Poll, d, 10_000_000_000)
                        .and_then(|b| ndp::parse_poll_reply(&b))
                    {
                        Some(r) if r.st == ndp::ST_OK => {
                            if r.flags & ndp::POLL_FLAG_CLOSED != 0
                                || (r.rbytes > 0 && r.flags & ndp::POLL_FLAG_WRITABLE == 0)
                            {
                                // 尸槽（对端已关）：释槽换下一条，不是本自验的
                                // 失败——client 侧还有活尝试在途。
                                let _ = netd_rpc_tcp(
                                    netd,
                                    ndp::TcpSReq::Close,
                                    d,
                                    90_000_000_000,
                                );
                                continue 'acc;
                            }
                            if r.rbytes > 0 && r.flags & ndp::POLL_FLAG_WRITABLE != 0 {
                                break 'acc d;
                            }
                        }
                        other => bail!("bsh: evs: c{} poll bad {:?}", ci, other.map(|r| r.st)),
                    }
                    if now() >= dat_dl {
                        bail!("bsh: evs: c{} data timeout", ci);
                    }
                    sleep_ms(tick, sleep_mask, 1000);
                }
            };
            // ③ DRAIN 收割：st OK + 逐字节对拍（POLL 报过 rbytes 后串行收割
            //    必得全量——单连接独占、无并发写者）。
            let got = netd_rpc_tcp(netd, ndp::TcpSReq::Drain, dsid, 10_000_000_000);
            match got {
                Some((st, ref d)) if st == ndp::ST_OK && d.as_slice() == want => {}
                Some((st, ref d)) => bail!(
                    "bsh: evs: c{} drain mismatch len={} want={} st={}",
                    ci,
                    d.len(),
                    want.len(),
                    st
                ),
                None => bail!("bsh: evs: c{} drain bad", ci),
            }
            // ④ RT 回发（echo）：send + ACK 帽内 ST_OK（client 侧对拍回读——
            //    本侧只断言投递完成；应答 1B OK 形无数据面）。
            //
            // **尸槽迟到相位**（A3-3 首跑实抓 `c0 echo bad Some(1)`，vnet 拒面
            // 诊断 state=CloseWait infl=0 定谳）：hostfwd 下 host 侧帽尽 close
            // 与 SLIRP 建连同束——ACK+载荷+FIN µs 级连发进 RX 环；泵逐帧消化
            // 下 ② 门通过时（载荷已 pump）FIN 还在环里排队，drain→RT 的 RPC
            // 间隙才被 pump 进 → Established 已失 → `send()` 拒 → ST_BAD。
            // ② 的门只拦「FIN 先于载荷可见」相位（A2-3），本支拦「FIN 迟于
            // 载荷」相位：re-poll 复核非 WRITABLE/CLOSED = 尸槽坐实 → 释槽
            // 重入 accept（client 重试环的下一条活连接已在 Ready 队列——
            // A3-3 pcap 实证）；WRITABLE 仍在 = 真坏 loud。
            match netd_rpc_tcp(
                netd,
                ndp::TcpSReq::Roundtrip { data: want },
                dsid,
                90_000_000_000,
            ) {
                Some((st, _)) if st == ndp::ST_OK => {}
                Some((st, _)) if st == ndp::ST_BAD => {
                    match netd_rpc(netd, ndp::TcpSReq::Poll, dsid, 10_000_000_000)
                        .and_then(|b| ndp::parse_poll_reply(&b))
                    {
                        Some(r) if r.st == ndp::ST_OK
                            && (r.flags & ndp::POLL_FLAG_CLOSED != 0
                                || r.flags & ndp::POLL_FLAG_WRITABLE == 0) =>
                        {
                            revives += 1;
                            if revives > 3 {
                                bail!("bsh: evs: c{} corpse loop", ci);
                            }
                            let _ = netd_rpc_tcp(
                                netd,
                                ndp::TcpSReq::Close,
                                dsid,
                                90_000_000_000,
                            );
                            continue 'conn;
                        }
                        other => bail!(
                            "bsh: evs: c{} echo bad {:?} poll {:?}",
                            ci,
                            st,
                            other.map(|r| (r.st, r.flags))
                        ),
                    }
                }
                other => bail!("bsh: evs: c{} echo bad {:?}", ci, other.map(|(s, _)| s)),
            }
            // ⑤ CLOSE(dsid)：驱动到 Closed 释槽（ST_TIMEOUT = 帽到槽已释的
            //    边缘形——首跑实录若真出现再定标，验收打严格 OK）。
            match netd_rpc_tcp(netd, ndp::TcpSReq::Close, dsid, 90_000_000_000) {
                Some((st, _)) if st == ndp::ST_OK => {}
                other => bail!("bsh: evs: c{} close bad {:?}", ci, other.map(|(s, _)| s)),
            }
            n_ok += 1;
            break 'conn;
        }
    }
    // ⑥ CLOSE(lsid) 撤 listen 表（Listen 槽即时释放——恒 OK）。
    match netd_rpc_tcp(netd, ndp::TcpSReq::Close, lsid, 10_000_000_000) {
        Some((st, _)) if st == ndp::ST_OK => {}
        other => bail!("bsh: evs: close listen bad {:?}", other.map(|(s, _)| s)),
    }
    if berry_handle_close(tick) != 0 {
        berry_libc::park();
    }
    println!("EVS ok n={}", n_ok);
}

/// A3-3 socket fd 皮自验 client 段载荷（32B）：echod 回显对拍源。公式族
/// 与 EVS 载荷分型（`(i*13+7)` vs `(i*7+3)`/`(i*11+5)`——server 段直接
/// 复用 EVS 载荷：宿主 evsclient.py 18083 参数化复用，同源对拍零漂移）。
const SKT_PAYLOAD_C: [u8; 32] = {
    let mut a = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        a[i] = (i * 13 + 7) as u8;
        i += 1;
    }
    a
};

/// A3-3 socket fd 皮自验（`SKT ok n=3` = rootfs 相第 40 标记，A3 整卡
/// 验收线）：evs 七步链（A2-3）同语义换 POSIX fd 入口——libc socket 族
/// （A3-1 表/旗面 + A3-2 RPC 真身）第一次全链活体。三段（每段独立
/// 验收点，失败 eprintln + park——SKT ok 缺失 → smoke FAIL 兜底）：
/// - ⓪ client 段（connect 10.0.2.2:18080 → 宿主 echod，T-c/TCPS 专线
///   串行复用——session-0 先后两用已证）：socket → connect（字节序四件
///   构造 sockaddr_in）→ send 32B（legacy RT 应答随行回显入 RESIDUE）→
///   recv 恰长逐字节对拍（RESIDUE 收割）→ close——Client 分形全链；
/// - ① 非阻塞 EAGAIN 节拍 + 旗/sockopt/send 错误面：bind 18084——**无
///   hostfwd 专线 = hostfwd 是唯一入站路径 → 结构性无连接**，accept
///   空队恒真（状态谓词非竞速，4-5a 铁律）：socket(SOCK_NONBLOCK 创建
///   旗) → bind → listen → fcntl F_GETFL 旗对拍（O_RDWR|O_NONBLOCK）→
///   accept4(空队) == -1 + EAGAIN → getsockopt SO_TYPE == SOCK_STREAM →
///   send-on-Listen == -1 + EPIPE → close；
/// - ② server 段（evs 七步链的 fd 皮）：bind 18083 + listen（hostfwd
///   专线——echod 18080/LSN 18081/EVS 18082 零交集；宿主
///   `evsclient.py 18083 SKT` 两连接 64B/24B 串行对拍）→ 每连接
///   {阻塞 accept（libc 内 1s 节拍；挂死兜底 = 相帽——client 总帽 480s
///   > DEADLINE_ROOTFS 420s，accept 落地前 client 恒活）→ accept 出参
///   钉板（哨兵 0xFFFF/0xAA 预填必被覆写：AF_INET/port 0/10.0.2.2/
///   addrlen 16——A3-2 出参契约活证）→ data-wait 闸门（poll 快照：
///   POLLHUP = 尸槽（CLOSED 旗——带据/空据同拒，evs 尸槽回收的 fd 皮
///   译法）→ close(dfd) 释槽重入 accept；POLLIN+POLLOUT = 活连接带据）
///   → recv 恰长逐字节对拍（POLL+DRAIN 在 libc 内）→ send 回发（命名
///   RT 纯发送臂，A3-1 案②）→ close(dfd)} ×2 → close(lfd)。
///
/// 拓扑降级同 EVS：netd gift 缺席（EFI/裸拓扑）→ netd_h()==0 静默跳过
/// （smoke-image canon 零变化）；gift 在位而 connect/listen 答 ENODEV
/// （vnet deviceless serve——B4 掉电相无 netdev）→ 同口径静默跳过
/// （拓扑真值）。
fn skt_selftest() {
    if netd_h() == 0 {
        return;
    }
    use berry_libc::net as skt;
    // 常量镜像（fsclient/std 单源 ABI 冻结值——bsh 不依赖 fsclient，双
    // 侧同步纪律：C-1 刀 4 usage 预检镜像同族；漂移 = 对拍断言即炸）。
    const AF_INET: i32 = 2;
    const SOCK_STREAM: i32 = 1;
    const F_GETFL: i32 = 3;
    const SOL_SOCKET: i32 = 1;
    const SO_TYPE: i32 = 3;
    const POLLIN: i16 = 0x001;
    const POLLOUT: i16 = 0x004;
    const POLLHUP: i16 = 0x010;
    // 轮询节拍专用 Event（永不置位——wait 帽即纯定时器；evs 同形态）。
    let mut tick = 0u32;
    if berry_event_create(0, &mut tick) != 0 || tick == 0 {
        berry_libc::park();
    }
    let sleep_mask: u64 = 1 << 32;
    macro_rules! bail {
        ($($a:tt)*) => {{
            eprintln!($($a)*);
            berry_libc::park();
        }};
    }
    macro_rules! errno {
        () => {
            berry_libc::get_errno()
        };
    }

    // —— ⓪ client 段：connect 10.0.2.2:18080（echod 回显）——
    let cfd = skt::socket(AF_INET, SOCK_STREAM, 0);
    if cfd < 0 {
        bail!("bsh: skt: socket errno={}", errno!());
    }
    let caddr = skt::SockAddrIn {
        sin_family: AF_INET as u16,
        sin_port: skt::htons(18080),
        sin_addr: skt::htonl(0x0A00_0202), // SLIRP 网关（connect 唯一可达对端）
        sin_zero: [0; 8],
    };
    // SAFETY: caddr/cbuf 均本帧局部，指针生命周期由调用方保证（shim 惯例）。
    if unsafe {
        skt::connect(
            cfd,
            &caddr as *const skt::SockAddrIn as *const skt::SockAddr,
            16,
        )
    } != 0
    {
        let e = errno!();
        if e == berry_libc::ENODEV {
            return; // 无 NIC 拓扑（B4 掉电相：vnet deviceless serve）——
            // 静默跳过，与 evs ST_NODEV 同口径（SKT ok 缺席 = 拓扑真值）。
        }
        bail!("bsh: skt: connect errno={}", e);
    }
    if unsafe { skt::send(cfd, SKT_PAYLOAD_C.as_ptr() as *const _, 32, 0) } != 32 {
        bail!("bsh: skt: client send errno={}", errno!());
    }
    let mut cbuf = [0u8; 32];
    let n = unsafe { skt::recv(cfd, cbuf.as_mut_ptr() as *mut _, 32, 0) };
    if n != 32 || cbuf != SKT_PAYLOAD_C {
        bail!(
            "bsh: skt: client echo n={} mismatch={}",
            n,
            cbuf != SKT_PAYLOAD_C
        );
    }
    if close(cfd) != 0 {
        bail!("bsh: skt: client close errno={}", errno!());
    }

    // —— ① 非阻塞 EAGAIN 节拍 + 旗/sockopt/send 错误面（18084 无
    //    hostfwd——hostfwd 是唯一入站路径，accept 空队恒真）——
    let nfd = skt::socket(AF_INET, SOCK_STREAM | berry_libc::fs::O_NONBLOCK, 0);
    if nfd < 0 {
        bail!("bsh: skt: nb socket errno={}", errno!());
    }
    let naddr = skt::SockAddrIn {
        sin_family: AF_INET as u16,
        sin_port: skt::htons(18084),
        sin_addr: 0, // INADDR_ANY（bind 纯本地 port 记账，地址域不校验）
        sin_zero: [0; 8],
    };
    if unsafe { skt::bind(nfd, &naddr as *const _ as *const skt::SockAddr, 16) } != 0 {
        bail!("bsh: skt: nb bind errno={}", errno!());
    }
    if skt::listen(nfd, 1) != 0 {
        bail!("bsh: skt: nb listen errno={}", errno!());
    }
    // F_GETFL 旗对拍：O_RDWR 恒底 | O_NONBLOCK（socket 创建旗入表 +
    // fcntl 投影——A3-1 表/旗面活证）。
    let fl = berry_libc::stubs::fcntl(nfd, F_GETFL, 0);
    if fl != berry_libc::fs::O_RDWR | berry_libc::fs::O_NONBLOCK {
        bail!("bsh: skt: F_GETFL fl={:#x}", fl);
    }
    // 非阻塞 accept 空队：-1 + EAGAIN 直返（不睡——A3-2 非阻塞语义）。
    if unsafe { skt::accept4(nfd, core::ptr::null_mut(), core::ptr::null_mut(), 0) } != -1
        || errno!() != berry_libc::EAGAIN
    {
        bail!("bsh: skt: nb accept not EAGAIN");
    }
    let mut sov = 0i32;
    let mut sol = 4u32;
    if unsafe {
        skt::getsockopt(
            nfd,
            SOL_SOCKET,
            SO_TYPE,
            &mut sov as *mut _ as *mut _,
            &mut sol,
        )
    } != 0
        || sov != SOCK_STREAM
        || sol != 4
    {
        bail!("bsh: skt: SO_TYPE sov={} sol={}", sov, sol);
    }
    // send-on-Listen：无会话可发 → -1 + EPIPE（SockOp::Send → ST_BAD →
    // EPIPE 分岔——A3-2 错码映射表活证）。
    if unsafe { skt::send(nfd, SKT_PAYLOAD_C.as_ptr() as *const _, 1, 0) } != -1
        || errno!() != berry_libc::EPIPE
    {
        bail!("bsh: skt: send-on-listen not EPIPE");
    }
    if close(nfd) != 0 {
        bail!("bsh: skt: nb close errno={}", errno!());
    }

    // —— ② server 段：evs 七步链的 fd 皮（hostfwd 18083 + 宿主
    //    evsclient.py 18083 SKT 两连接串行对拍）——
    let lfd = skt::socket(AF_INET, SOCK_STREAM, 0);
    if lfd < 0 {
        bail!("bsh: skt: srv socket errno={}", errno!());
    }
    let laddr = skt::SockAddrIn {
        sin_family: AF_INET as u16,
        sin_port: skt::htons(18083),
        sin_addr: 0,
        sin_zero: [0; 8],
    };
    if unsafe { skt::bind(lfd, &laddr as *const _ as *const skt::SockAddr, 16) } != 0 {
        bail!("bsh: skt: srv bind errno={}", errno!());
    }
    if skt::listen(lfd, 1) != 0 {
        let e = errno!();
        if e == berry_libc::ENODEV {
            return; // client 段已拦主路——防御位（B4 拓扑同口径）。
        }
        bail!("bsh: skt: srv listen errno={}", e);
    }
    let mut n_ok = 0u32;
    for (ci, want) in [EVS_PAYLOAD_0.as_slice(), EVS_PAYLOAD_1.as_slice()]
        .iter()
        .enumerate()
    {
        let want: &[u8] = want;
        // 尸槽复活预算（evs ④ 同根同款，详见其注释）：帽 3 loud。
        let mut revives = 0u32;
        'conn: loop {
            // 阻塞 accept（libc 内 1s 节拍重拉）+ accept 出参钉板 + data-wait
            // 闸门（poll 快照尸槽译法）。帽：accept 无显式帽——挂死兜底 =
            // 相帽（client 总帽 480s > DEADLINE_ROOTFS 420s：accept 落地前
            // client 恒活；client 死则相帽先 FAIL loud——与 evs 帽序同型）。
            let dfd = 'acc: loop {
                let mut sa = skt::SockAddr {
                    sa_family: 0xFFFF, // 哨兵：必须被覆写
                    sa_data: [0xAA; 14],
                };
                let mut alen = 0u32;
                // SAFETY: sa/alen 均本帧局部。
                let dfd = unsafe { skt::accept4(lfd, &mut sa, &mut alen, 0) };
                if dfd < 0 {
                    bail!("bsh: skt: accept c{} errno={}", ci, errno!());
                }
                // 出参钉板（A3-2 契约：AF_INET / port 0（wire 不携对端端口，
                // 诚实未知）/ 10.0.2.2 / *addrlen=16——整结构覆写即每字段活证）。
                if sa.sa_family != AF_INET as u16
                    || sa.sa_data != [0, 0, 10, 0, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0]
                    || alen != 16
                {
                    bail!(
                        "bsh: skt: accept out c{} fam={:#x} alen={}",
                        ci,
                        sa.sa_family,
                        alen
                    );
                }
                // data-wait 闸门：poll 快照（timeout 忽略恒立即返——快照语义）
                // POLLHUP = 尸槽（CLOSED 旗，带据/空据同拒——先于数据位判，
                // evs「CLOSED 先判」同序）；POLLIN 无 POLLOUT = 尸槽迟到相位
                // （CloseWait 带据——对端 FIN 已 pump 而未及 Closed；evs ② 门
                // 「带据无 WRITABLE」的 fd 皮译法，A3-3 首跑实抓补相位）→ 均
                // close 释槽重入 accept；POLLIN+POLLOUT = 活连接带据 → 进
                // recv。帽 60s（evs 同值）。
                let dat_dl = now() + 60_000_000_000;
                loop {
                    let mut p = [berry_libc::stubs::PollFd {
                        fd: dfd,
                        events: POLLIN | POLLOUT,
                        revents: 0,
                    }];
                    let r = berry_libc::stubs::poll(p.as_mut_ptr(), 1, 0);
                    let rev = p[0].revents;
                    if r >= 0
                        && (rev & POLLHUP != 0 || (rev & POLLIN != 0 && rev & POLLOUT == 0))
                    {
                        // 尸槽回收：释槽换下一条，不是失败——client 侧还有
                        // 活尝试在途（evs 首跑实抓同型 + A3-3 迟到相位）。
                        if close(dfd) != 0 {
                            bail!("bsh: skt: corpse close c{} errno={}", ci, errno!());
                        }
                        continue 'acc;
                    }
                    if r >= 0 && rev & POLLIN != 0 && rev & POLLOUT != 0 {
                        break;
                    }
                    if now() >= dat_dl {
                        bail!("bsh: skt: c{} data timeout rev={:#x}", ci, rev);
                    }
                    sleep_ms(tick, sleep_mask, 1000);
                }
                break dfd;
            };
            // recv 恰长逐字节对拍（POLL+DRAIN 在 libc 内；闸门已见 POLLIN
            // 必即收割——阻塞 recv 首拉即中）。
            let mut buf = [0u8; 64];
            let n = unsafe { skt::recv(dfd, buf.as_mut_ptr() as *mut _, want.len(), 0) };
            if n != want.len() as isize || &buf[..want.len()] != want {
                bail!(
                    "bsh: skt: c{} drain n={} want={} mismatch={}",
                    ci,
                    n,
                    want.len(),
                    &buf[..want.len()] != want
                );
            }
            // send 回发（命名 RT 纯发送臂——ACK 帽在 libc 内 CAP_SLOW=90s）。
            // 尸槽迟到相位（evs ④ 同根）：send 吃 EPIPE（SockOp::Send →
            // ST_BAD 分岔）时 re-poll 复核——非 POLLOUT/POLLHUP = 会话已离
            // Established（对端 FIN 在闸门后、send 前被 pump 进）→ close 释槽
            // 重入 accept；POLLOUT 仍在 = 真坏 loud。
            if unsafe { skt::send(dfd, buf.as_ptr() as *const _, want.len(), 0) }
                != want.len() as isize
            {
                let mut p = [berry_libc::stubs::PollFd {
                    fd: dfd,
                    events: POLLIN | POLLOUT,
                    revents: 0,
                }];
                let r = berry_libc::stubs::poll(p.as_mut_ptr(), 1, 0);
                let rev = p[0].revents;
                if r >= 0 && (rev & POLLHUP != 0 || rev & POLLOUT == 0) {
                    revives += 1;
                    if revives > 3 {
                        bail!("bsh: skt: c{} corpse loop", ci);
                    }
                    if close(dfd) != 0 {
                        bail!("bsh: skt: corpse close c{} errno={}", ci, errno!());
                    }
                    continue 'conn;
                }
                bail!("bsh: skt: c{} echo errno={} rev={:#x}", ci, errno!(), rev);
            }
            if close(dfd) != 0 {
                bail!("bsh: skt: c{} close errno={}", ci, errno!());
            }
            n_ok += 1;
            break 'conn;
        }
    }
    if close(lfd) != 0 {
        bail!("bsh: skt: listen close errno={}", errno!());
    }
    if berry_handle_close(tick) != 0 {
        berry_libc::park();
    }
    println!("SKT ok n={}", 1 + n_ok); // client 段 1 + server 段 2
}

/// A4-3 epoll + DNS 自验（`EPL ok n=8` = rootfs 相第 41 标记，A4 整卡
/// 验收线）：epoll 三桩真身（A4-1 表+节拍循环）+ getaddrinfo DNS 档
/// （A4-2 接线）第一次全链活体。八步（每步独立验收点，失败 eprintln +
/// park——EPL ok 缺失 → smoke FAIL 兜底）：
/// - ⓪ create1 + 槽满面：flags 只认 0（EPOLL_CLOEXEC → EINVAL）；连开
///   4 槽满 → 第 5 个 EMFILE；
/// - ① ctl 拒面全表：epfd 域外 EBADF / op 坏 EINVAL / ADD event NULL
///   EFAULT / 掩码白名单外（EPOLLET）EINVAL / MOD·DEL 未在册 ENOENT；
///   段满·帽满形态 = **Exists**（A4-1 定谳：socket 段恰 8 fd ==
///   EPOLL_ITEMS_MAX ⇒ ENOSPC 结构性不可达）——7 socket + cfd 恰满
///   socket 段（第 8 个 socket() → EMFILE guest 活证），8/8 全 ADD 后
///   重 ADD → EEXIST；
/// - ② wait(timeout=0) 快照 LT 复报：ADD(cfd, IN|OUT, data 哨兵) →
///   两次连收恰 1 项 events==EPOLLOUT（Client 恒 WRITABLE 状态谓词——
///   echod 无数据在途非竞速）+ data 覆写对拍；
/// - ③ wait 短帽（timeout>0）空表到点返 0（POSIX 超时 = 0 事件成功返，
///   ~1s wall——1s 节拍 sleep_tick 首拍即帽）；
/// - ④ 非 socket fd（console fd 1）ADD → EINVAL（NotSock 检查先于
///   Exists/Full——interest 8/8 满时仍正确分档）；
/// - ⑤ getaddrinfo 双查："127.0.0.1" 字面量快路（零 RPC）+
///   "localhost" DNS 慢路（netd OP_DNS_QUERY → SLIRP 离线确定
///   127.0.0.1）——ai_family/ai_socktype/sin_addr 三对拍 +
///   freeaddrinfo；
/// - ⑥ 非法名（"my_host" 下划线）/ 超长名（65B > NAME_MAX=64）→
///   EAI_NONAME 纯本地拒（零发包——预检面活证）；
/// - ⑦ close 释槽复用：close(epfd3) → create1 同号回填（最低位回填
///   host 钉板的 guest 活证）→ close。
///
/// 拓扑降级同 EVS/SKT：netd gift 缺席（EFI/裸拓扑）→ netd_h()==0 静默
/// 跳过（smoke-image canon 零变化）；gift 在位而 connect 答 ENODEV
/// （vnet deviceless serve——B4 掉电相无 netdev）→ 同口径静默跳过
/// （拓扑真值）。排 fat 族前零盘面。
fn epl_selftest() {
    if netd_h() == 0 {
        return;
    }
    use berry_libc::net as epl;
    // 常量镜像（fsclient/std 单源 ABI 冻结值——skt 镜像同族；漂移 = 对拍
    // 断言即炸）。
    const AF_INET: i32 = 2;
    const SOCK_STREAM: i32 = 1;
    const EPOLLIN: u32 = 0x001;
    const EPOLLOUT: u32 = 0x004;
    const EPOLLET: u32 = 1 << 31;
    const EPOLL_CLOEXEC: i32 = 0x80000;
    const EPOLL_CTL_ADD: i32 = 1;
    const EPOLL_CTL_MOD: i32 = 3;
    const EPOLL_CTL_DEL: i32 = 2;
    let mut n_ok = 0u32;
    macro_rules! bail {
        ($($a:tt)*) => {{
            eprintln!($($a)*);
            berry_libc::park();
        }};
    }
    macro_rules! errno {
        () => {
            berry_libc::get_errno()
        };
    }

    // 拓扑门 + ② 的就绪源：connect echod 18080（skt ⓪ 同款）。
    let cfd = epl::socket(AF_INET, SOCK_STREAM, 0);
    if cfd < 0 {
        bail!("bsh: epl: socket errno={}", errno!());
    }
    let caddr = epl::SockAddrIn {
        sin_family: AF_INET as u16,
        sin_port: epl::htons(18080),
        sin_addr: epl::htonl(0x0A00_0202), // SLIRP 网关
        sin_zero: [0; 8],
    };
    // SAFETY: caddr 本帧局部。
    if unsafe {
        epl::connect(
            cfd,
            &caddr as *const epl::SockAddrIn as *const epl::SockAddr,
            16,
        )
    } != 0
    {
        let e = errno!();
        if e == berry_libc::ENODEV {
            return; // 无 NIC 拓扑（B4 掉电相）——静默跳过，skt 同口径。
        }
        bail!("bsh: epl: connect errno={}", e);
    }

    // —— ⓪ create1 + 槽满面（4 槽 23..=26）——
    // flags 执法：EPOLL_CLOEXEC v1 无 exec 面诚实拒。
    if unsafe { epl::epoll_create1(EPOLL_CLOEXEC) } != -1 || errno!() != berry_libc::EINVAL {
        bail!("bsh: epl: create1 flags not EINVAL");
    }
    let epfd = unsafe { epl::epoll_create1(0) };
    if epfd < 0 {
        bail!("bsh: epl: create1 errno={}", errno!());
    }
    let e2 = unsafe { epl::epoll_create1(0) };
    let e3 = unsafe { epl::epoll_create1(0) };
    let e4 = unsafe { epl::epoll_create1(0) };
    if e2 < 0 || e3 < 0 || e4 < 0 {
        bail!("bsh: epl: create1 fill errno={}", errno!());
    }
    // 槽满：4/4 后第 5 个 EMFILE；e4 验毕即释（后续主用 3 实例）。
    if unsafe { epl::epoll_create1(0) } != -1 || errno!() != berry_libc::EMFILE {
        bail!("bsh: epl: create1 full not EMFILE");
    }
    if close(e4) != 0 {
        bail!("bsh: epl: e4 close errno={}", errno!());
    }
    n_ok += 1;

    // —— ① ctl 拒面全表 + 段满·帽满形态（Exists——A4-1 定谳）——
    let ev_ok = epl::EpollEvent {
        events: EPOLLIN | EPOLLOUT,
        data: 0,
    };
    let ev_et = epl::EpollEvent {
        events: EPOLLIN | EPOLLET,
        data: 0,
    };
    // SAFETY: ev_ok/ev_et 均本帧局部（非 NULL event 路径）。
    unsafe {
        // epfd 域外 → EBADF（域检在 event 解引用前——EBADF 优先序）。
        if epl::epoll_ctl(99, EPOLL_CTL_ADD, cfd, &ev_ok) != -1
            || errno!() != berry_libc::EBADF
        {
            bail!("bsh: epl: ctl badfd not EBADF");
        }
        // op 坏 → EINVAL。
        if epl::epoll_ctl(epfd, 9, cfd, &ev_ok) != -1 || errno!() != berry_libc::EINVAL {
            bail!("bsh: epl: ctl op not EINVAL");
        }
        // ADD event NULL → EFAULT。
        if epl::epoll_ctl(epfd, EPOLL_CTL_ADD, cfd, core::ptr::null()) != -1
            || errno!() != berry_libc::EFAULT
        {
            bail!("bsh: epl: ctl null not EFAULT");
        }
        // 掩码白名单外（EPOLLET——ET v1 不做）→ EINVAL。
        if epl::epoll_ctl(epfd, EPOLL_CTL_ADD, cfd, &ev_et) != -1
            || errno!() != berry_libc::EINVAL
        {
            bail!("bsh: epl: ctl mask not EINVAL");
        }
        // MOD/DEL 未在册 → ENOENT。
        if epl::epoll_ctl(epfd, EPOLL_CTL_MOD, cfd, &ev_ok) != -1
            || errno!() != berry_libc::ENOENT
        {
            bail!("bsh: epl: ctl mod not ENOENT");
        }
        if epl::epoll_ctl(epfd, EPOLL_CTL_DEL, cfd, core::ptr::null()) != -1
            || errno!() != berry_libc::ENOENT
        {
            bail!("bsh: epl: ctl del not ENOENT");
        }
    }
    // 段满·帽满形态：7 socket + cfd 恰满 socket 段（15..=22）——第 8 个
    // socket() EMFILE；8/8 全 ADD 后重 ADD → EEXIST（ENOSPC 结构性
    // 不可达的可达形态——检查序 Exists 先于 Full 定谳）。
    let mut sfds = [0i32; 7];
    for (i, s) in sfds.iter_mut().enumerate() {
        *s = epl::socket(AF_INET, SOCK_STREAM, 0);
        if *s < 0 {
            bail!("bsh: epl: fill socket {} errno={}", i, errno!());
        }
    }
    if epl::socket(AF_INET, SOCK_STREAM, 0) != -1 || errno!() != berry_libc::EMFILE {
        bail!("bsh: epl: socket full not EMFILE");
    }
    // SAFETY: ev_ok 本帧局部。
    unsafe {
        for (i, &s) in sfds.iter().enumerate() {
            let ev = epl::EpollEvent {
                events: EPOLLIN | EPOLLOUT,
                data: i as u64,
            };
            if epl::epoll_ctl(epfd, EPOLL_CTL_ADD, s, &ev) != 0 {
                bail!("bsh: epl: add s{} errno={}", i, errno!());
            }
        }
        if epl::epoll_ctl(epfd, EPOLL_CTL_ADD, cfd, &ev_ok) != 0 {
            bail!("bsh: epl: add cfd errno={}", errno!());
        }
        // interest 8/8 帽满 → 重 ADD 在册 → EEXIST。
        if epl::epoll_ctl(epfd, EPOLL_CTL_ADD, sfds[0], &ev_ok) != -1
            || errno!() != berry_libc::EEXIST
        {
            bail!("bsh: epl: ctl full not EEXIST");
        }
    }
    n_ok += 1;

    // —— ② wait(timeout=0) 快照 LT 复报：两次连收同集（berry-poll LT
    //    同款免费——静态 interest 无消费即再报）——
    let epfd2 = e2; // ② 专用实例（interest 1/8——cfd）
    let ev_d = epl::EpollEvent {
        events: EPOLLIN | EPOLLOUT,
        data: 0xC0DE_0001,
    };
    // SAFETY: ev_d 本帧局部。
    if unsafe { epl::epoll_ctl(epfd2, EPOLL_CTL_ADD, cfd, &ev_d) } != 0 {
        bail!("bsh: epl: lt add errno={}", errno!());
    }
    for round in 0..2 {
        let mut evs = [epl::EpollEvent {
            events: 0,
            data: 0xDEAD_BEEF, // 哨兵：必须被覆写
        }; 4];
        // SAFETY: evs 本帧局部出参区。
        let r = unsafe { epl::epoll_wait(epfd2, evs.as_mut_ptr(), 4, 0) };
        // 恰 1 项（epfd2 interest 仅 cfd）；恰 OUT 无 IN（echod 无数据
        // 在途——不 send 它就不发，状态谓词非竞速）；data 覆写透传。
        // packed 字段先值拷贝再格式化（packed 引用禁——E0793）。
        let (ev, da) = (evs[0].events, evs[0].data);
        if r != 1 || ev != EPOLLOUT || da != 0xC0DE_0001 {
            bail!(
                "bsh: epl: lt round{} r={} ev={:#x} data={:#x}",
                round,
                r,
                ev,
                da
            );
        }
    }
    n_ok += 1;

    // —— ③ wait 短帽（timeout>0）空表到点返 0——
    let epfd3 = e3; // 空实例（零 interest）
    let mut evs = [epl::EpollEvent {
        events: 0,
        data: 0,
    }; 4];
    // SAFETY: evs 本帧局部出参区。
    if unsafe { epl::epoll_wait(epfd3, evs.as_mut_ptr(), 4, 1500) } != 0 {
        bail!("bsh: epl: wait cap not 0");
    }
    n_ok += 1;

    // —— ④ 非 socket fd ADD 拒（console fd 1——NotSock 先于 Exists/Full，
    //    epfd interest 8/8 满时仍正确分档 EINVAL 非 ENOSPC/EEXIST）——
    // SAFETY: ev_ok 本帧局部。
    if unsafe { epl::epoll_ctl(epfd, EPOLL_CTL_ADD, 1, &ev_ok) } != -1
        || errno!() != berry_libc::EINVAL
    {
        bail!("bsh: epl: ctl notsock not EINVAL");
    }
    n_ok += 1;

    // —— ⑤ getaddrinfo 双查（快路字面量 + 慢路 DNS）——
    let want_addr = epl::htonl(0x7F00_0001); // 127.0.0.1（wire 序对拍）
    for (label, name) in [("lit", &b"127.0.0.1\0"[..]), ("dns", &b"localhost\0"[..])] {
        let mut res: *mut epl::AddrInfo = core::ptr::null_mut();
        // SAFETY: res 本帧局部出参；name NUL 终结字面量。
        let r = unsafe {
            epl::getaddrinfo(
                name.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                &mut res,
            )
        };
        if r != 0 || res.is_null() {
            bail!("bsh: epl: gai {} r={} res={}", label, r, res.is_null());
        }
        // SAFETY: getaddrinfo 产物链（libc 契约）；ai_addr 指同链节点内
        // SockAddrIn（16B——A3-1 钉板）。
        let ok = unsafe {
            let ai = &*res;
            let sa = &*(ai.ai_addr as *const epl::SockAddrIn);
            ai.ai_family == AF_INET
                && ai.ai_socktype == SOCK_STREAM
                && sa.sin_family == AF_INET as u16
                && sa.sin_addr == want_addr
        };
        unsafe { epl::freeaddrinfo(res) };
        if !ok {
            bail!("bsh: epl: gai {} fields bad", label);
        }
    }
    n_ok += 1;

    // —— ⑥ 非法名/超长名本地拒（零发包——预检面）——
    let mut long_name = [b'a'; 65];
    long_name[65 - 1] = 0; // 64 字符 + NUL——名字长 65 > NAME_MAX=64
    for (label, name) in [
        ("badchar", &b"my_host\0"[..]), // 下划线出 [A-Za-z0-9.-]
        ("toolong", &long_name[..]),
    ] {
        let mut res: *mut epl::AddrInfo = core::ptr::null_mut();
        // SAFETY: res 本帧局部出参。
        let r = unsafe {
            epl::getaddrinfo(
                name.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                &mut res,
            )
        };
        if r != berry_libc::EAI_NONAME || !res.is_null() {
            bail!("bsh: epl: gai {} r={} res={}", label, r, res.is_null());
        }
    }
    n_ok += 1;

    // —— ⑦ close 释槽复用（最低位回填）+ 收尾 ——
    if close(epfd3) != 0 {
        bail!("bsh: epl: close epfd3 errno={}", errno!());
    }
    let again = unsafe { epl::epoll_create1(0) };
    if again != epfd3 {
        bail!("bsh: epl: refill {} != {}", again, epfd3);
    }
    if close(again) != 0 {
        bail!("bsh: epl: close again errno={}", errno!());
    }
    n_ok += 1;

    // 收尾：socket 8 全释（7 sfd + cfd）+ epoll 2 实例（epfd 8/8 interest
    // 随 close 全清；epfd2 cfd 项随之失义——先 close fd 后 close epoll 同
    // 序亦可，表持值快照无悬挂面）。
    for (i, &s) in sfds.iter().enumerate() {
        if close(s) != 0 {
            bail!("bsh: epl: s{} close errno={}", i, errno!());
        }
    }
    if close(cfd) != 0 {
        bail!("bsh: epl: cfd close errno={}", errno!());
    }
    if close(epfd2) != 0 || close(epfd) != 0 {
        bail!("bsh: epl: ep close errno={}", errno!());
    }
    println!("EPL ok n={}", n_ok);
}

/// A5-2 std::net spawn 面自验（`NETS ok n=2` = rootfs 相第 42 标记，A5
/// 整卡验收线）：[`net_gifts`] 三元装配（A5-1）+ std::net 深水面
/// （TcpStream connect→write→echo 恰长对拍）第一次经 spawn gift 链全链
/// 活体。两步——**A5-1 净室 probe 定谳后的修订步设计**（A5-0 草案
/// "localhost:18083" 原案证伪：SLIRP guest 环回不绕宿主 loopback
/// （ENETUNREACH）、evsclient 18083 本非 echo 对端；真 echo 对端 =
/// echod 18080，SLIRP 网关别名 10.0.2.2 直达宿主）：
/// - ① 正步：netprobe 10.0.2.2 18080（IP 字面量快路零 DNS RPC）→
///   rc=0 + 输出逐字节 `NETPROBE ok n=14\n`（14 = 默认载荷
///   "berry-netprobe"）；
/// - ② 负控：netprobe localhost 18080 → rc=1 + 输出**尾行**
///   `NETPROBE fail: connect errno=<n>`（**前缀锚**——errno 值不锚：
///   std ToSocketAddrs→getaddrinfo→A4-2 DNS 档经 std::net 活证——错误
///   落 connect 段非解析段；EPL ⓪ 同款「localhost 解析 127.0.0.1 后死于
///   环回不可达」定谳形状。errno 具体值（ENETUNREACH=101）是 SLIRP 行为
///   面、随 QEMU 版本可漂——锚值即跨版本脆弱面；strerror 文案同理不锚
///   ——stderr 诊断行先于 stdout 标记行同通道 FIFO，只锚尾行）。
///
/// 每步 fresh `net_gifts()`（berry-exec take 契约：spawn 成功 dup 全被
/// env send take——gift 值复用第二次 spawn 即 -EBADF）。netprobe 缺席
/// （cpio_find None）→ 诊断行 loud park（区别于拓扑静默门——标记缺失
/// smoke FAIL 兜底）。
///
/// 拓扑降级**双门**同 EPL/SKT：第一道 netd gift 缺席（EFI/裸拓扑）→
/// netd_h()==0 静默跳过（smoke-image canon 零变化）；第二道 **gift 在位
/// 而无 netdev**（B4 掉电相——vnet deviceless serve，connect 答
/// ENODEV(19)）→ 步① 按 netprobe stdout `fail: connect errno=19` 静默
/// 跳过——拓扑真值不是链断；其余 connect 错误 loud（echod 死 = 真链断
/// 该 FAIL）。A5 终验五相 smoke 首跑实抓：单门版在 B4 Boot A 把 ENODEV
/// 当链断 park → fat 族全断 → 240s FAT ok 未到。排 fat 族前零盘面
/// （A 系契约）。
fn nets_selftest() {
    if netd_h() == 0 {
        return;
    }
    // TCG 下 netprobe 全链（spawn + connect + echo 往返 + 净退）实测秒级、
    // ENETUNREACH 即返——60s 帽 = uu_cp/C-5 同口径 ×2（client 收侧帽按 worst
    // 路径成本定值，P6 A-6 纪律）。
    const NETS_CAP_NS: u64 = 60 * 1_000_000_000;
    let mut n_ok = 0u32;

    // —— ① 正步：10.0.2.2 字面量（echod 真对端）——
    let g = match net_gifts() {
        Some(g) => g,
        None => berry_libc::park(), // netd_h() 已过门而 dup 失败：链断 loud
    };
    let (o, rc) = match run_capture_gifts_cap(
        "netprobe",
        Some(&[] as &[u8]),
        &["10.0.2.2", "18080"],
        &[g.0, g.1, g.2],
        NETS_CAP_NS,
    ) {
        Some(r) => r,
        None => {
            eprintln!("bsh: nets: netprobe absent");
            berry_libc::park();
        }
    };
    if rc == 1 && o.eof && o.data.ends_with(b"NETPROBE fail: connect errno=19\n") {
        // 拓扑第二道门（EPL/SKT 双门契约）：B4 掉电相无 netdev——netd
        // gift 在位（第一道 netd_h()==0 不拦）而 vnet deviceless serve
        // 答 ENODEV(19)。拓扑真值静默跳过（无标记无 park，fat 族照走）；
        // 其余 connect 错误落下方 loud（echod 死 = 真链断）。
        return;
    }
    if rc != 0 || !o.eof || o.data != b"NETPROBE ok n=14\n" {
        eprintln!(
            "bsh: nets: lit rc={} eof={} data={:?}",
            rc,
            o.eof,
            String::from_utf8_lossy(&o.data)
        );
        berry_libc::park();
    }
    n_ok += 1;

    // —— ② 负控：localhost（std DNS 链 + 环回不可达定谳形状）——
    let g = match net_gifts() {
        Some(g) => g,
        None => berry_libc::park(),
    };
    let (o, rc) = match run_capture_gifts_cap(
        "netprobe",
        Some(&[] as &[u8]),
        &["localhost", "18080"],
        &[g.0, g.1, g.2],
        NETS_CAP_NS,
    ) {
        Some(r) => r,
        None => {
            eprintln!("bsh: nets: netprobe absent");
            berry_libc::park();
        }
    };
    // 尾行 = 剥尾 \n 后最后一个 \n 之后段（stderr 诊断行先于 stdout 标记
    // 行同通道 FIFO——诊断行数不定，只锚尾行；前缀锚：errno 值是 SLIRP
    // 行为面随 QEMU 版本可漂，值不锚）。
    let d = o.data.strip_suffix(b"\n").unwrap_or(&o.data);
    let tail = match d.iter().rposition(|&b| b == b'\n') {
        Some(i) => &d[i + 1..],
        None => d,
    };
    if rc != 1 || !o.eof || !tail.starts_with(b"NETPROBE fail: connect errno=") {
        eprintln!(
            "bsh: nets: dns rc={} eof={} data={:?}",
            rc,
            o.eof,
            String::from_utf8_lossy(&o.data)
        );
        berry_libc::park();
    }
    n_ok += 1;

    println!("NETS ok n={}", n_ok);
}

// —— A7-1：mmap/munmap 符号经 extern 声明**链接期**解析到 berry-libc
//    mmap 真身（#[no_mangle] C-ABI）——与 libc crate aarch64-musl 声明
//    逐参对齐（registry 源 unix/mod.rs mmap 六参/munmap 两参实证）。extern
//    形态 = 零编译期耦合：真身落地前后 bsh 源恒可编译（接口契约只钉
//    符号名 + ABI，模块内部布局是 impl-libc 的实现自由——bsh 不 import
//    berry_libc::mmap 模块路径）。链接序：berry-libc rlib 先于 -lc 归档
//    （shim 全定义纪律），符号恒解析到真身。
extern "C" {
    fn mmap(
        addr: *mut core::ffi::c_void,
        length: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut core::ffi::c_void;
    fn munmap(addr: *mut core::ffi::c_void, length: usize) -> i32;
    fn mprotect(addr: *mut core::ffi::c_void, len: usize, prot: i32) -> i32;
}

/// A7-1/A7-3：mmap 匿名档 + 文件档 staging guest 自验（`MMP ok n=9` =
/// rootfs 相标记）——berry-libc mmap/mprotect 真身 guest 全链活体。九步
/// 全过才打；**状态谓词
/// 零竞速**（4-5a 铁律——纯串行 libc 直调，零子进程零对拍端，
/// poll_selftest 同款形态）：
/// ⓪ mmap(NULL,8192,PROT_READ|PROT_WRITE,MAP_ANONYMOUS|MAP_PRIVATE,-1,0)
///    双页首映射 → 非 MAP_FAILED、非 NULL、页对齐；两页各 volatile 写
///    u64 图样 0x4D4D5031/0x4D4D5032 读回对拍（跨页锚 = 整段 commit+
///    map 两页真活——单页写会漏「次页未 commit」形态）；
/// ① hint 忽略档：mmap(0x5500_0000,4096,…,-1,0)（非 NULL 无 MAP_FIXED
///    = hint，POSIX 允许忽略）→ 成功且落窗 [0x5000_0000,0x6000_0000)
///    + munmap 收口；
/// ② munmap 三态：内部地址（p1+4096,4096）→ -1 且 errno==EINVAL（交叠
///    非基址精确 = Partial 诚实拒，条目保留）；恰长（p1,8192）→ 0（真身
///    扛「先 vmar_unmap 再 close vmo」-EBUSY 序）；重复同址 → 0
///    （NotFound——POSIX 未映射区间非错误，A7-1 审查追认档）；
/// ③ 空洞复用钉板：p1 已出表 → 同长再 mmap → **== p1 首址**（first-fit
///    底起洞复用——A7-0 §3.1 钦定段内分配策略；bump 无复用形态在此必
///    破）+ 再写读一拍（图样 0x4D4D5033 区分首拍）+ munmap 收口；
/// ④ 拒面与 mprotect：范围盒三连（fd=0≠-1 / flags+MAP_SHARED / 单
///    PROT_READ——各断 MAP_FAILED 且 errno==EINVAL）+ PROT_EXEC 忽略档
///    （R|W|X 放行——用户页本可执行，NX 留 v2）→ mprotect 命中 → -1
///    且 errno==EACCES（v1 无改权档诚实拒）→ munmap 收口 → 同址再
///    mprotect → -1 且 errno==EINVAL（未命中档）；
/// ⑥ 文件档 staging 真身（A7-3）：RAM 造源 m/mmpf.bin（4396B = 4096 基界
///    + 300 尾段；目录名避开 M3DEMO 的 t/ 造源面零交叉——父目录在 RAM
///    树则 create 恒落 RAM，相位无关）→ open(O_RDONLY) → mmap(MAP_PRIVATE,
///    offset=0, len=4200)（跨 4096 页界 staging + want<文件长 → 收尾截断
///    臂 take<n 活证）→ 首尾字节/u64 对拍 + [4200,8192) 尾零区零值验
///    （整段 commit 含尾零页——A7-3 对设计稿「隐式 commit」缺口的执法面）
///    → 映射面改写一字节 → munmap → get_file 逐字节验源未变（急切私有
///    拷贝证——staging 与源解耦）→ 同 fd 复映射 offset=4096 len=300（单
///    PROT_READ 文件档放行 + file_off≠0 + EOF 恰边 4396）→ 读回对拍 →
///    收口；
/// ⑦ 文件档拒位四连：MAP_SHARED 叠加 / offset=8 非页对齐 / offset+length
///    超 EOF（4096+4096>4396，原始 length 判）/ prot 缺 READ（单 WRITE）
///    ——各 MAP_FAILED 且 errno==EINVAL；
/// ⑧ fd 面拒位：fd=1（console 段 <3）/ fd=99（fd 表查无）各 EBADF；
///    O_WRONLY fd → EACCES（v1 只收恰 O_RDONLY——共享档的面诚实拒，
///    POSIX 偏差清单 §3.3）；
/// ⑨ mprotect dormant 保内容环：写哨兵 → PROT_NONE 撤翻译 → dormant
///    幂等 → RW 复映复活 → 哨兵原值保内容（VMO 滞表未 close）→ 追写
///    读对拍 → dormant 条目 munmap 收口（close 无映射无 -EBUSY 面）；
///    dormant 期严格不触区间（无翻译读写 = EL0 D-abort 假死）。
/// 链序契约同 U2/POLL——⑥-⑧ 走 RAM 树 fs 面（非盘面；造源收尾还零残留），
/// 排 fat 族之前（EFI 路无盘亦打 MMP）；无拓扑降级门（T-1 自描述对
/// gift[3] 是 spawn 通用装配——失败即 park loud）。
/// 范围盒常量本地镜像（契约冻结值，已对 libc crate aarch64-musl 逐值
/// 核验：PROT 族 = 1/2/4、MAP_SHARED/PRIVATE/FIXED = 0x01/0x02/0x10、
/// MAP_ANONYMOUS = 0x20（musl MAP_ANON）、MAP_FAILED = !0——与
/// berry-libc pub 常量位置解耦，值由契约钉死不漂）。
fn mmp_selftest() {
    const PROT_NONE: i32 = 0;
    const PROT_READ: i32 = 1;
    const PROT_WRITE: i32 = 2;
    const PROT_EXEC: i32 = 4;
    const MAP_SHARED: i32 = 0x01;
    const MAP_PRIVATE: i32 = 0x02;
    const MAP_ANONYMOUS: i32 = 0x20;
    const MAP_FAILED: *mut core::ffi::c_void = usize::MAX as *mut core::ffi::c_void;
    const EBADF: i32 = 9; // musl aarch64（本函数 const 镜像纪律：值由契约钉死）
    const LEN: usize = 8192; // 双页（跨页验整段 commit+map 两页真活）
    const WIN_LO: usize = 0x5000_0000; // berry-mmap VA 窗（本地镜像钉板）
    const WIN_HI: usize = 0x6000_0000;

    let mut fails: Vec<&str> = Vec::new();

    // ⓪ 双页首映射：NULL 请求（first-fit 空表 → 窗基）→ 活址且页对齐。
    let p1 = unsafe {
        mmap(
            core::ptr::null_mut(),
            LEN,
            PROT_READ | PROT_WRITE,
            MAP_ANONYMOUS | MAP_PRIVATE,
            -1,
            0,
        )
    };
    let p1_ok = p1 != MAP_FAILED && !p1.is_null() && ((p1 as usize) & 0xfff) == 0;
    if !p1_ok {
        fails.push("⓪ 首映射非活址/非页对齐");
    }

    //    两页各写读（p1_ok 门控——失败即不触 p1：MAP_FAILED 解引用 = EL0
    //    D-abort 假死，诊断面只剩 park 前的 fails 清单；volatile 执法 =
    //    真映射页往返，防「写读被寄存器化」假绿；次页拍 = 「整段 commit
    //    两页真活」的跨页锚）。
    if p1_ok {
        const PAT1: u64 = 0x4D4D5031;
        const PAT1B: u64 = 0x4D4D5032;
        unsafe { core::ptr::write_volatile(p1 as *mut u64, PAT1) };
        unsafe { core::ptr::write_volatile(p1.add(4096) as *mut u64, PAT1B) };
        let got = unsafe { core::ptr::read_volatile(p1 as *const u64) };
        let got_b = unsafe { core::ptr::read_volatile(p1.add(4096) as *const u64) };
        if got.to_le_bytes() != PAT1.to_le_bytes() || got_b.to_le_bytes() != PAT1B.to_le_bytes() {
            fails.push("⓪ 双页写读对拍失配");
        }
    }

    // ① hint 忽略档：非 NULL 请求无 MAP_FIXED = hint（POSIX 允许忽略）
    //    → 成功且落窗 + 收口。
    let ph = unsafe {
        mmap(
            0x5500_0000 as *mut core::ffi::c_void,
            4096,
            PROT_READ | PROT_WRITE,
            MAP_ANONYMOUS | MAP_PRIVATE,
            -1,
            0,
        )
    };
    if ph == MAP_FAILED {
        fails.push("① hint 档未放行");
    } else {
        let a = ph as usize;
        if a < WIN_LO || a >= WIN_HI {
            fails.push("① hint 档落窗外");
        }
        if unsafe { munmap(ph, 4096) } != 0 {
            fails.push("① hint 档收口非 0");
        }
    }

    // ② munmap 三态：内部地址 → -1+EINVAL（交叠非基址 = Partial 诚实
    //    拒，条目保留）；恰长 → 0（「先 vmar_unmap 再 close vmo」-EBUSY
    //    序）；重复同址 → 0（NotFound——POSIX 未映射区间非错误，A7-1
    //    审查追认档）。
    if p1_ok {
        if unsafe { munmap(p1.add(4096), 4096) } != -1 || berry_libc::get_errno() != EINVAL {
            fails.push("② 内部地址未拒 EINVAL");
        }
        if unsafe { munmap(p1, LEN) } != 0 {
            fails.push("② 恰长收口非 0");
        }
        if unsafe { munmap(p1, LEN) } != 0 {
            fails.push("② 重复收口非 0（POSIX NotFound 档）");
        }
    }

    // ③ 空洞复用钉板：p1 已出表 → 同长再映射必回 p1 首址（first-fit
    //    底起、底洞恰容——A7-0 §3.1 钦定段内分配策略；bump 无复用形态
    //    在此必破）+ 再写读一拍（图样 +2 区分首拍）+ munmap 收口。
    let p3 = unsafe {
        mmap(
            core::ptr::null_mut(),
            LEN,
            PROT_READ | PROT_WRITE,
            MAP_ANONYMOUS | MAP_PRIVATE,
            -1,
            0,
        )
    };
    if p3 == MAP_FAILED || p3.is_null() || ((p3 as usize) & 0xfff) != 0 {
        fails.push("③ 复用档再映射非活址");
    } else {
        if p3 != p1 {
            fails.push("③ 空洞未复用首址（first-fit 钦定破缺）");
        }
        const PAT3: u64 = 0x4D4D5033;
        unsafe { core::ptr::write_volatile(p3 as *mut u64, PAT3) };
        let got = unsafe { core::ptr::read_volatile(p3 as *const u64) };
        if got.to_le_bytes() != PAT3.to_le_bytes() {
            fails.push("③ 复用档写读对拍失配");
        }
        if unsafe { munmap(p3, LEN) } != 0 {
            fails.push("③ 复用档收口非 0");
        }
    }

    // ④ 范围盒拒面三连：各断言返 MAP_FAILED 且 errno==EINVAL（get_errno
    //    = skt/epl 同源先例；拒因互异逐个独立断言）。
    let r = unsafe {
        mmap(
            core::ptr::null_mut(),
            LEN,
            PROT_READ | PROT_WRITE,
            MAP_ANONYMOUS | MAP_PRIVATE,
            0, // fd≠-1：匿名档 fd 必 -1
            0,
        )
    };
    if r != MAP_FAILED || berry_libc::get_errno() != EINVAL {
        fails.push("④ fd≠-1 未拒 EINVAL");
    }
    let r = unsafe {
        mmap(
            core::ptr::null_mut(),
            LEN,
            PROT_READ | PROT_WRITE,
            MAP_ANONYMOUS | MAP_PRIVATE | MAP_SHARED, // v1 无共享档
            -1,
            0,
        )
    };
    if r != MAP_FAILED || berry_libc::get_errno() != EINVAL {
        fails.push("④ MAP_SHARED 未拒 EINVAL");
    }
    let r = unsafe {
        mmap(
            core::ptr::null_mut(),
            LEN,
            PROT_READ, // prot 单 READ：vmar_map v1 只支持恰 RW=3
            MAP_ANONYMOUS | MAP_PRIVATE,
            -1,
            0,
        )
    };
    if r != MAP_FAILED || berry_libc::get_errno() != EINVAL {
        fails.push("④ prot 单 READ 未拒 EINVAL");
    }
    // PROT_EXEC 忽略档（R|W|X 放行——用户页本可执行，NX 留 v2）→
    // mprotect 命中 → -1+EACCES（v1 无改权档诚实拒）→ 收口 → 同址
    // 未命中 → -1+EINVAL。
    let pm = unsafe {
        mmap(
            core::ptr::null_mut(),
            4096,
            PROT_READ | PROT_WRITE | PROT_EXEC,
            MAP_ANONYMOUS | MAP_PRIVATE,
            -1,
            0,
        )
    };
    if pm == MAP_FAILED {
        fails.push("④ PROT_EXEC 忽略档未放行");
    } else {
        if unsafe { mprotect(pm, 4096, PROT_READ) } != -1 || berry_libc::get_errno() != EACCES {
            fails.push("④ mprotect 命中非 EACCES");
        }
        if unsafe { munmap(pm, 4096) } != 0 {
            fails.push("④ EXEC 档收口非 0");
        }
        if unsafe { mprotect(pm, 4096, PROT_READ) } != -1 || berry_libc::get_errno() != EINVAL {
            fails.push("④ mprotect 未命中非 EINVAL");
        }
    }

    // ⑥ 文件档 staging 真身（A7-3）：RAM 树 mkdir m + 造源 4396B（父目录
    //    在 RAM 树 → create 恒落 RAM 相位无关；避开 M3DEMO 的 t/ 零交叉）。
    //    fwant=4200 跨 4096 页界（staging 跨页写）且 < FLEN（收尾截断臂
    //    take<n 可达——4200 = 21×192+168，末 chunk take=168）。
    const FLEN: usize = 4396; // 4096 基界 + 300 尾段（两段 offset 档共用）
    const FWANT: usize = 4200;
    let mut f_ok = true;
    let mut made = false; // 造源已落（清理门与 f_ok 分离——open 失败仍要还零）
    if mkdir_of("m").is_err() {
        fails.push("⑥ mkdir m 失败");
        f_ok = false;
    } else {
        let fsrc: Vec<u8> = (0..FLEN).map(|i| (i ^ (i >> 3)) as u8).collect();
        if put_file("m/mmpf.bin", &fsrc).is_err() {
            fails.push("⑥ 造源失败");
            f_ok = false;
        } else {
            made = true;
        }
        // open O_RDONLY → fd 落 berry-libc fd 表（mmap 预检消费面）。
        let p = cpath("m/mmpf.bin");
        // SAFETY: p NUL 结尾路径串；fd 按 libc 契约使用。
        let ffd = if f_ok {
            unsafe { open(p.as_ptr() as *const _, O_RDONLY) }
        } else {
            -1
        };
        if f_ok && ffd < 3 {
            fails.push("⑥ open 失败");
            f_ok = false;
        }
        if f_ok {
            let q = unsafe {
                mmap(
                    core::ptr::null_mut(),
                    FWANT,
                    PROT_READ | PROT_WRITE,
                    MAP_PRIVATE,
                    ffd,
                    0,
                )
            };
            if q == MAP_FAILED {
                fails.push("⑥ 文件档首映射失败");
                f_ok = false;
            } else {
                let mut w8 = [0u8; 8];
                w8.copy_from_slice(&fsrc[0..8]);
                let g0 = unsafe { core::ptr::read_volatile(q as *const u64) };
                let gm = unsafe { core::ptr::read_volatile(q.add(150) as *const u8) };
                let gt = unsafe { core::ptr::read_volatile(q.add(FWANT - 1) as *const u8) };
                // 尾零区：[4200, 8192) 属 commit 清零面（staging 未触）——
                // 整段 commit 含尾零页的执法锚。
                let gz = unsafe { core::ptr::read_volatile(q.add(4352) as *const u64) };
                if g0.to_le_bytes() != w8 || gm != fsrc[150] || gt != fsrc[FWANT - 1] {
                    fails.push("⑥ staging 内容对拍失配");
                }
                if gz != 0 {
                    fails.push("⑥ 尾零区非零（整段 commit 破缺）");
                }
                // 改写字节 150（落私有 VMO——急切拷贝与源解耦）→ 收口。
                unsafe { core::ptr::write_volatile(q.add(150) as *mut u8, !fsrc[150]) };
                if unsafe { munmap(q, 8192) } != 0 {
                    fails.push("⑥ 文件档收口非 0");
                }
                // 私有拷贝证：源逐字节未变（改写不回写）。
                match get_file("m/mmpf.bin") {
                    Ok(back) if back.len() == FLEN && back.iter().zip(&fsrc).all(|(a, b)| a == b) => {}
                    _ => fails.push("⑥ 私有拷贝证失配（源被改写）"),
                }
                // 同 fd 复映射 offset=4096 len=300：单 PROT_READ 文件档
                // 放行 + file_off≠0 + EOF 恰边（4096+300 == 4396）。
                let q2 = unsafe {
                    mmap(
                        core::ptr::null_mut(),
                        300,
                        PROT_READ,
                        MAP_PRIVATE,
                        ffd,
                        4096,
                    )
                };
                if q2 == MAP_FAILED {
                    fails.push("⑥ offset 档复映射失败");
                } else {
                    let mut w8b = [0u8; 8];
                    w8b.copy_from_slice(&fsrc[4096..4104]);
                    let g0 = unsafe { core::ptr::read_volatile(q2 as *const u64) };
                    let gt = unsafe { core::ptr::read_volatile(q2.add(299) as *const u8) };
                    if g0.to_le_bytes() != w8b || gt != fsrc[FLEN - 1] {
                        fails.push("⑥ offset 档内容对拍失配");
                    }
                    if unsafe { munmap(q2, 4096) } != 0 {
                        fails.push("⑥ offset 档收口非 0");
                    }
                }
            }
            // ⑦ 文件档拒位四连（classify/预检执法面——逐个独立验 errno）。
            let r = unsafe {
                mmap(
                    core::ptr::null_mut(),
                    300,
                    PROT_READ,
                    MAP_PRIVATE | MAP_SHARED, // v1 无共享档
                    ffd,
                    0,
                )
            };
            if r != MAP_FAILED || berry_libc::get_errno() != EINVAL {
                fails.push("⑦ MAP_SHARED 未拒 EINVAL");
            }
            let r = unsafe {
                mmap(
                    core::ptr::null_mut(),
                    300,
                    PROT_READ,
                    MAP_PRIVATE,
                    ffd,
                    8, // 非页对齐
                )
            };
            if r != MAP_FAILED || berry_libc::get_errno() != EINVAL {
                fails.push("⑦ offset 非页对齐未拒 EINVAL");
            }
            let r = unsafe {
                mmap(
                    core::ptr::null_mut(),
                    4096,
                    PROT_READ,
                    MAP_PRIVATE,
                    ffd,
                    4096, // 4096+4096 > 4396：超 EOF（原始 length 判）
                )
            };
            if r != MAP_FAILED || berry_libc::get_errno() != EINVAL {
                fails.push("⑦ 超 EOF 未拒 EINVAL");
            }
            let r = unsafe {
                mmap(
                    core::ptr::null_mut(),
                    300,
                    PROT_WRITE, // 文件档缺 READ
                    MAP_PRIVATE,
                    ffd,
                    0,
                )
            };
            if r != MAP_FAILED || berry_libc::get_errno() != EINVAL {
                fails.push("⑦ prot 缺 READ 未拒 EINVAL");
            }
            if close(ffd) != 0 {
                fails.push("⑦ 源 fd 收口非 0");
            }
        }
    }
    // ⑧ fd 面拒位：fd=1（console 段 <3）/ fd=99（fd 表查无）各 EBADF；
    //    O_WRONLY fd → EACCES（v1 只收恰 O_RDONLY）。O_CREAT 对已存文件
    //    = 纯 open（put_file 覆写幂等档同款）；零写 close 的 TRUNC(0)
    //    清空造源无害（内容断言已在 ⑥ 完结）。
    {
        let r = unsafe {
            mmap(
                core::ptr::null_mut(),
                4096,
                PROT_READ,
                MAP_PRIVATE,
                1,
                0,
            )
        };
        if r != MAP_FAILED || berry_libc::get_errno() != EBADF {
            fails.push("⑧ fd=1 未拒 EBADF");
        }
        let r = unsafe {
            mmap(
                core::ptr::null_mut(),
                4096,
                PROT_READ,
                MAP_PRIVATE,
                99,
                0,
            )
        };
        if r != MAP_FAILED || berry_libc::get_errno() != EBADF {
            fails.push("⑧ fd=99 未拒 EBADF");
        }
        if f_ok {
            let p = cpath("m/mmpf.bin");
            // SAFETY: 同上。
            let wf = unsafe { open(p.as_ptr() as *const _, O_WRONLY | O_CREAT) };
            if wf < 3 {
                fails.push("⑧ O_WRONLY open 失败");
            } else {
                let r = unsafe {
                    mmap(
                        core::ptr::null_mut(),
                        300,
                        PROT_READ,
                        MAP_PRIVATE,
                        wf,
                        0,
                    )
                };
                if r != MAP_FAILED || berry_libc::get_errno() != EACCES {
                    fails.push("⑧ O_WRONLY fd 未拒 EACCES");
                }
                if close(wf) != 0 {
                    fails.push("⑧ O_WRONLY fd 收口非 0");
                }
            }
        }
    }
    // ⑨ mprotect dormant 保内容环（匿名档页验五臂机制——与文件档无关的
    //    纯 dormant 面防耦合）。dormant 期严格不触区间：无翻译读写 =
    //    EL0 D-abort 假死（验收只走 mprotect/munmap 调用面）。
    let p9 = unsafe {
        mmap(
            core::ptr::null_mut(),
            4096,
            PROT_READ | PROT_WRITE,
            MAP_ANONYMOUS | MAP_PRIVATE,
            -1,
            0,
        )
    };
    if p9 == MAP_FAILED {
        fails.push("⑨ dormant 环基映射失败");
    } else {
        const S9A: u64 = 0x4D4D5039;
        const S9B: u64 = 0x4D4D5041;
        unsafe { core::ptr::write_volatile(p9 as *mut u64, S9A) };
        if unsafe { mprotect(p9, 4096, PROT_NONE) } != 0 {
            fails.push("⑨ 撤翻译非 0");
        }
        if unsafe { mprotect(p9, 4096, PROT_NONE) } != 0 {
            fails.push("⑨ dormant 幂等非 0");
        }
        if unsafe { mprotect(p9, 4096, PROT_READ | PROT_WRITE) } != 0 {
            fails.push("⑨ 复映复活非 0");
        }
        let g = unsafe { core::ptr::read_volatile(p9 as *const u64) };
        if g.to_le_bytes() != S9A.to_le_bytes() {
            fails.push("⑨ dormant 往返内容失配（VMO 保内容破缺）");
        }
        unsafe { core::ptr::write_volatile(p9 as *mut u64, S9B) };
        let g2 = unsafe { core::ptr::read_volatile(p9 as *const u64) };
        if g2.to_le_bytes() != S9B.to_le_bytes() {
            fails.push("⑨ 复活后写读失配");
        }
        if unsafe { munmap(p9, 4096) } != 0 {
            fails.push("⑨ dormant 条目收口非 0");
        }
    }
    // 还零残留：造源文件 + m 目录出 RAM 树（后续 `ls /`/find 类断言
    // 相位无关恒真——contains 断言虽不锁全集，零残留不留面）。
    if made && unlink_of("m/mmpf.bin").is_err() {
        fails.push("⑥ 造源清理失败");
    }
    if made && unlink_of("m").is_err() {
        fails.push("⑥ m 目录清理失败");
    }

    if !fails.is_empty() {
        eprintln!("bsh: mmp selftest bad: {:?}", fails);
        berry_libc::park();
    }
    println!("MMP ok n=9");
}

// —— A7-2：clock 族符号同款 extern 链接期解析（A7-1 mmap 先例）——对
//    libc crate aarch64-musl 声明逐参对齐（clock_gettime 两参/time 单参
//    出参 gettimeofday 双参/nanosleep 双参）。Timespec/Timeval 布局 =
//    两 i64 字段（musl aarch64 同形），本地镜像钉板。
#[repr(C)]
struct ClkTs {
    tv_sec: i64,
    tv_nsec: i64,
}
#[repr(C)]
struct ClkTv {
    tv_sec: i64,
    tv_usec: i64,
}
extern "C" {
    fn clock_gettime(clk: i32, ts: *mut ClkTs) -> i32;
    fn clock_settime(clk: i32, ts: *const ClkTs) -> i32;
    fn time(t: *mut i64) -> i64;
    fn gettimeofday(tv: *mut ClkTv, tz: *mut core::ffi::c_void) -> i32;
    fn nanosleep(req: *const ClkTs, rem: *mut ClkTs) -> i32;
}

/// A7-2：timer 壁钟分档 guest 自验（`CLK ok n=6` = rootfs 相第 44 标记，
/// `docs/事件循环A7-2实施设计草案.md` §4）。六步全过才打；**状态谓词零
/// 竞速**（4-5a 铁律——纯串行 libc 直调 + 唯一子进程 `date -u`（漂移
/// 容忍判据非字面钉板），poll_selftest 同款形态）：
/// ⓪ mono 双读单调非减 + nsec 域合法（0 ≤ nsec < 1e9）；
/// ① REALTIME 合成一致：先 mono 后 rt 双读，rt ≥ mono 且 rt−mono ≤ 1s
///    （base=0 下差值 = 两读取样间隙，≤1s 容忍带）；
/// ② time()/gettimeofday() 互恰：同窗 |time − tv_sec| ≤ 1s + tv_usec
///    μs 域合法（0 ≤ usec < 1e6）+ time() 返回值 == 出参回填；
/// ③ nanosleep(50ms) 活性：前后 mono 差 ∈ [50ms, 5s]（TCG 宽带）+ 返 0；
/// ④ clock_settime 双档拒：REALTIME/MONOTONIC 各 -1 + errno==EPERM
///    （拒 per-process 假成功面——设计稿 §3.3）；
/// ⑤ 裸 `date -u` 漂移容忍钉板：前缀 `Thu Jan  1 00:` + MM:SS 数字域 +
///    尾 ` UTC 1970`（REALTIME 端到端活钟面——boot→clk 相位分钟级、
///    小时域 00 结构性稳定；U4 步③ `-d @0` 渲染面钉板的姊妹断言）。
/// Linux/musl clockid 编号本地镜像（调用侧事实源；berry-sys
/// CLOCK_MONOTONIC=0 是内核侧编号——本 fn 本地 const 遮蔽其 use 导入，
/// 双轨勿混，设计稿 §1 #8）。
fn clk_selftest() {
    const CLOCK_REALTIME: i32 = 0;
    const CLOCK_MONOTONIC: i32 = 1;

    let mut fails: Vec<&str> = Vec::new();
    let nsec_ok = |t: &ClkTs| t.tv_nsec >= 0 && t.tv_nsec < 1_000_000_000;
    let ns_of = |t: &ClkTs| t.tv_sec as i128 * 1_000_000_000 + t.tv_nsec as i128;
    let read_mono = |t: &mut ClkTs| unsafe { clock_gettime(CLOCK_MONOTONIC, t) };

    // ⓪ mono 双读单调非减 + nsec 域合法。
    let mut ma = ClkTs { tv_sec: 0, tv_nsec: 0 };
    let mut mb = ClkTs { tv_sec: 0, tv_nsec: 0 };
    let (ra, rb) = (read_mono(&mut ma), read_mono(&mut mb));
    if ra != 0 || rb != 0 {
        fails.push("⓪ mono 读非 0");
    }
    if !nsec_ok(&ma) || !nsec_ok(&mb) {
        fails.push("⓪ nsec 域非法");
    }
    if ns_of(&mb) < ns_of(&ma) {
        fails.push("⓪ mono 双读非单调非减");
    }

    // ① REALTIME 合成一致：先 mono 后 rt（base=0 → rt ≥ mono、差 = 间隙）。
    let mut m1 = ClkTs { tv_sec: 0, tv_nsec: 0 };
    let mut r1 = ClkTs { tv_sec: 0, tv_nsec: 0 };
    let (rm, rr) = (read_mono(&mut m1), unsafe { clock_gettime(CLOCK_REALTIME, &mut r1) });
    if rm != 0 || rr != 0 {
        fails.push("① 双钟读非 0");
    } else {
        let (mn, rn) = (ns_of(&m1), ns_of(&r1));
        if !(rn >= mn && rn - mn <= 1_000_000_000) {
            fails.push("① REALTIME 合成一致破缺");
        }
    }

    // ② time()/gettimeofday() 互恰（同窗 ≤1s）+ 出参契约。
    let mut tout: i64 = -1;
    let tret = unsafe { time(&mut tout) };
    let mut tv = ClkTv { tv_sec: 0, tv_usec: 0 };
    let gvd = unsafe { gettimeofday(&mut tv, core::ptr::null_mut()) };
    if gvd != 0 {
        fails.push("② gettimeofday 非 0");
    }
    if tret != tout {
        fails.push("② time 返回值 ≠ 出参回填");
    }
    if tret < 0 {
        fails.push("② time 返回负值");
    }
    if (tret - tv.tv_sec).abs() > 1 {
        fails.push("② time/gettimeofday 互恰破缺");
    }
    if !(tv.tv_usec >= 0 && tv.tv_usec < 1_000_000) {
        fails.push("② tv_usec 域非法");
    }

    // ③ nanosleep(50ms) 活性（TCG 宽带 [50ms, 5s]）。
    let mut pre = ClkTs { tv_sec: 0, tv_nsec: 0 };
    read_mono(&mut pre);
    let req = ClkTs { tv_sec: 0, tv_nsec: 50_000_000 };
    let nrc = unsafe { nanosleep(&req, core::ptr::null_mut()) };
    let mut post = ClkTs { tv_sec: 0, tv_nsec: 0 };
    read_mono(&mut post);
    if nrc != 0 {
        fails.push("③ nanosleep 非 0");
    }
    if !(ns_of(&post) - ns_of(&pre) >= 50_000_000
        && ns_of(&post) - ns_of(&pre) <= 5_000_000_000)
    {
        fails.push("③ nanosleep 50ms 活性破缺");
    }

    // ④ settime 双档拒（EPERM——拒 per-process 假成功面）。
    let zts = ClkTs { tv_sec: 0, tv_nsec: 0 };
    if unsafe { clock_settime(CLOCK_REALTIME, &zts) } != -1 || berry_libc::get_errno() != EPERM {
        fails.push("④ settime REALTIME 非 -1+EPERM");
    }
    if unsafe { clock_settime(CLOCK_MONOTONIC, &zts) } != -1 || berry_libc::get_errno() != EPERM {
        fails.push("④ settime MONOTONIC 非 -1+EPERM");
    }

    // ⑤ 裸 `date -u` 漂移容忍钉板（29B："Thu Jan  1 00:MM:SS UTC 1970\n"
    // ——小时域 00 结构性稳定；破界形态（>59min）放宽预案 = 中段 MM:SS
    // 任意数字，设计稿 §5 #2）。
    match run_capture("date", None, &["-u"]) {
        Some((o, c)) if c == 0 => {
            let d: &[u8] = &o.data;
            let dig = |x: u8| x.is_ascii_digit();
            let ok5 = d.len() == 29
                && d[0..13] == *b"Thu Jan  1 00"
                && d[13] == b':'
                && dig(d[14])
                && dig(d[15])
                && d[16] == b':'
                && dig(d[17])
                && dig(d[18])
                && d[19..28] == *b" UTC 1970"
                && d[28] == b'\n';
            if !ok5 {
                fails.push("⑤ 裸 date -u 漂移容忍钉板失配");
            }
        }
        _ => fails.push("⑤ 裸 date -u spawn 失败"),
    }

    if !fails.is_empty() {
        eprintln!("bsh: clk selftest bad: {:?}", fails);
        berry_libc::park();
    }
    println!("CLK ok n=6");
}

/// A7-4 段④：信号面 guest 自验（`SIG ok n=6`，调用点头注——六步）：
/// ⓪ sigaction 装 SIGUSR1 handler + raise → 计数恰 1（raise 尾同步
/// dispatch，不赌调度相位）；① 表回读（sigaction null 查询 == handler）
/// + sigprocmask BLOCK/查询双向（oldset 不含 → 含 SIGUSR2）；② blocked 窗
/// raise → handler 不跑（held 持久化）→ UNBLOCK 补投 → 计数恰 1（归并式
/// 分诊 + 收窄补投面 = ③b 投递循环核心环）；③ kill 分诊三连（self ≡
/// raise 计数 2 / 探活 sig==0 零内核 / 非 self ESRCH + 界外号 EINVAL）；
/// ④ spawn bin/sigchild → `berry_signal_send(hp, SIGTERM)`（libc kill 仅
/// self 档——跨进程直调 berry-sys 0x86 面）→ 子睡循环醒来投递 → DFL
/// Terminate → `io::exit(143)` **EOF 先行**（与 INT 案内核杀 no-EOF 恰成
/// 对照面——collect 预期 eof=true/killed=false）→ wait TERMINATED +
/// reap 真码 143；⑤ 修复轮回归钉板（位号序敏感三形分诊，对抗审查
/// mini-review S1/S3）：BLOCK{USR2} 后 raise 落 held + `berry_signal_
/// send(0, SIGCONT)` 内核置位**零投递** + raise(SIGHUP)——h3 内换 18
/// 的 handler（h4→h4b）+ UNBLOCK{USR2}，断言五件：嵌套补投恰一次
/// （「Hold 即刻落 held」可见性主钉板）/ delivery-time 表变更（冻结
/// plan 形态跑旧表 h4）/ 掩码包络全量恢复含 SIGUSR2 / 收尾 UNBLOCK
/// 计数不涨（冻结 plan 外层 Hold 步重落 held = 残留双投）。
///
/// 信号面经 bsh 本地 extern 声明链接期解析 berry-libc `#[no_mangle]` 真身
/// （clk ClkTs/mmp mmap 同款先例——extern 块签名与 libc crate
/// aarch64-musl 声明逐参对齐）。handler 计数靶 = AtomicU32（dispatch 同
/// 线程同步调用，Relaxed 足够——无跨线程可见性面）。
fn sig_selftest() {
    // asm-generic 三号（berry-signal 单源值；本地镜像——不引 berry-signal
    // 依赖面，与 clk 本地声明形态一致）。
    const SIGUSR1: i32 = 10;
    const SIGUSR2: i32 = 12;
    const SIGTERM: i32 = 15;
    const SIGHUP: i32 = 1; // ⑤ 位号序钉板用（Call 位 1 < Hold 位 12 < 表变位 18）
    const SIGCONT: i32 = 18;
    const SIG_BLOCK: i32 = 0; // 掩码三 how（kernel asm-generic 与 libc 同值）
    const SIG_UNBLOCK: i32 = 1;

    /// struct sigaction 最小镜像（sa_sigaction@0/sa_mask@8 128B/sa_flags@
    /// 136/sa_restorer@144——berry-libc signal.rs 钉板同源布局）。
    #[repr(C)]
    struct SigAct {
        sa_sigaction: usize,
        sa_mask: [u8; 128],
        sa_flags: i32,
        sa_restorer: usize,
    }
    extern "C" {
        fn sigaction(sig: i32, act: *const SigAct, oldact: *mut SigAct) -> i32;
        fn sigprocmask(how: i32, set: *const u8, oldset: *mut u8) -> i32;
        fn sigemptyset(set: *mut u8) -> i32;
        fn sigaddset(set: *mut u8, sig: i32) -> i32;
        fn sigismember(set: *const u8, sig: i32) -> i32;
        fn raise(sig: i32) -> i32;
        fn kill(pid: i32, sig: i32) -> i32;
        fn getpid() -> i32;
    }

    static CNT1: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    static CNT2: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    extern "C" fn h1(_sig: i32) {
        CNT1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    extern "C" fn h2(_sig: i32) {
        CNT2.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    static CNT3: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    static CNT4: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    static CNT4B: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    extern "C" fn h4(_sig: i32) {
        CNT4.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    extern "C" fn h4b(_sig: i32) {
        CNT4B.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    /// ⑤ 的嵌套司机：投递中途换 18 的 handler（h4→h4b）+ 解封
    /// SIGUSR2——两动作都在 handler 内完成才构成「中途表/掩码变更」。
    /// 换表在前：UNBLOCK 触发的嵌套补投只见 held 里的 12（18 早已被
    /// 外层轮首 take 进工作集），新表由外层 delivery-time 重查消费。
    extern "C" fn h3(_sig: i32) {
        CNT3.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut ab = SigAct {
            sa_sigaction: h4b as *const () as usize,
            sa_mask: [0; 128],
            sa_flags: 0,
            sa_restorer: 0,
        };
        let mut s = [0u8; 128];
        unsafe {
            sigemptyset(ab.sa_mask.as_mut_ptr());
            sigemptyset(s.as_mut_ptr());
            sigaddset(s.as_mut_ptr(), SIGUSR2);
            sigaction(SIGCONT, &ab, core::ptr::null_mut());
            sigprocmask(SIG_UNBLOCK, s.as_ptr(), core::ptr::null_mut());
        }
    }

    let c1 = || CNT1.load(std::sync::atomic::Ordering::Relaxed);
    let c2 = || CNT2.load(std::sync::atomic::Ordering::Relaxed);
    let c3 = || CNT3.load(std::sync::atomic::Ordering::Relaxed);
    let c4 = || CNT4.load(std::sync::atomic::Ordering::Relaxed);
    let c4b = || CNT4B.load(std::sync::atomic::Ordering::Relaxed);
    let mut fails: Vec<&str> = Vec::new();

    // ⓪ 装 SIGUSR1 handler + raise 即时投递：计数恰 1（handler 包络位
    //    事后已摘——掩码回到全开，①② 不受染）。
    let mut act = SigAct {
        sa_sigaction: h1 as *const () as usize,
        sa_mask: [0; 128],
        sa_flags: 0,
        sa_restorer: 0,
    };
    unsafe { sigemptyset(act.sa_mask.as_mut_ptr()) };
    if unsafe { sigaction(SIGUSR1, &act, core::ptr::null_mut()) } != 0 {
        fails.push("⓪ sigaction 装 handler 非 0");
    }
    if unsafe { raise(SIGUSR1) } != 0 || c1() != 1 {
        fails.push("⓪ raise 即时投递计数 != 1");
    }

    // ① 表回读 == handler；sigprocmask BLOCK 出 oldset（不含 SIGUSR2）+
    //    set=NULL 纯查询回读（含）——掩码状态机双向活证。
    let mut old = SigAct {
        sa_sigaction: 0,
        sa_mask: [0; 128],
        sa_flags: 0,
        sa_restorer: 0,
    };
    if unsafe { sigaction(SIGUSR1, core::ptr::null(), &mut old) } != 0
        || old.sa_sigaction != h1 as *const () as usize
    {
        fails.push("① sigaction 回读 != handler");
    }
    let mut set2 = [0u8; 128];
    let mut oldset = [0u8; 128];
    unsafe {
        sigemptyset(set2.as_mut_ptr());
        sigaddset(set2.as_mut_ptr(), SIGUSR2);
    }
    if unsafe { sigprocmask(SIG_BLOCK, set2.as_ptr(), oldset.as_mut_ptr()) } != 0
        || unsafe { sigismember(oldset.as_ptr(), SIGUSR2) } != 0
    {
        fails.push("① BLOCK oldset 不含 SIGUSR2 破缺");
    }
    if unsafe { sigprocmask(SIG_BLOCK, core::ptr::null(), oldset.as_mut_ptr()) } != 0
        || unsafe { sigismember(oldset.as_ptr(), SIGUSR2) } != 1
    {
        fails.push("① 查询回读不含 SIGUSR2 破缺");
    }

    // ② SIGUSR2 blocked 窗内 raise：handler 不跑（held 持久化）→ UNBLOCK
    //    收窄补投：计数恰 1。
    let mut act2 = SigAct {
        sa_sigaction: h2 as *const () as usize,
        sa_mask: [0; 128],
        sa_flags: 0,
        sa_restorer: 0,
    };
    unsafe { sigemptyset(act2.sa_mask.as_mut_ptr()) };
    if unsafe { sigaction(SIGUSR2, &act2, core::ptr::null_mut()) } != 0 {
        fails.push("② sigaction 装 h2 非 0");
    }
    if unsafe { raise(SIGUSR2) } != 0 || c2() != 0 {
        fails.push("② blocked 窗 raise 计数非 0（held 破缺）");
    }
    if unsafe { sigprocmask(SIG_UNBLOCK, set2.as_ptr(), core::ptr::null_mut()) } != 0 || c2() != 1 {
        fails.push("② UNBLOCK 补投计数 != 1");
    }

    // ③ kill 分诊：self ≡ raise（计数 2）；探活档 sig==0 零内核；非 self
    //    诚拒 ESRCH（v1 无 pid 面）；界外号 EINVAL。
    if unsafe { kill(getpid(), SIGUSR1) } != 0 || c1() != 2 {
        fails.push("③ kill self 投递计数 != 2");
    }
    if unsafe { kill(getpid(), 0) } != 0 {
        fails.push("③ kill(pid,0) 探活非 0");
    }
    if unsafe { kill(999, 0) } != -1 || berry_libc::get_errno() != 3 {
        fails.push("③ kill(999,0) 非 -1/ESRCH");
    }
    if unsafe { kill(999, SIGTERM) } != -1 || berry_libc::get_errno() != 3 {
        fails.push("③ kill(999,SIGTERM) 非 -1/ESRCH");
    }
    if unsafe { kill(getpid(), 99) } != -1 || berry_libc::get_errno() != 22 {
        fails.push("③ kill 界外号非 -1/EINVAL");
    }

    // ④ 跨进程终止链（bin/sigchild：DFL 免装靶——200ms 睡循环醒来即
    //    投递点；信号先于子首睡到达也无竞——内核 pending 位持久在
    //    Process 上）。EOF 先行对照面：collect 预期 eof=true/killed=false
    //    （io::exit 链 vs INT 案内核 terminate no-EOF）。
    let full = b"bin/sigchild";
    let Some((data_off, data_len)) = cpio_find(rootfs_h(), full) else {
        eprintln!("bsh: sig selftest: bin/sigchild not found");
        berry_libc::park();
    };
    let (out_s, out_c) = channel_pair();
    let spec = SpawnSpec {
        name: full,
        data_off,
        data_len,
        stdout: out_c,
        stack_pages: 32, // musl std 口径（run_capture 同款 d-3c）
        stdin_feed: None,
        stdin_handle: None,
        args: &[],
        extra_gifts: &[],
        mmio_vmo: None,
        mmio_va: 0,
    };
    let child = match spawn_rootfs_elf(rootfs_h(), &spec) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("bsh: sig selftest: sigchild spawn failed");
            berry_libc::park();
        }
    };
    if berry_signal_send(child.hp, SIGTERM as u32) != 0 {
        fails.push("④ signal_send(SIGTERM) != 0");
    }
    let o = collect_fg(out_s, child.hp, &[child.hp]);
    if o.killed || !o.eof {
        fails.push("④ collect 非 eof 收场（exit 链 EOF 面破缺）");
    }
    match child.reap() {
        Ok(143) => {}
        Ok(_) => fails.push("④ reap 真码 != 143"),
        Err(()) => fails.push("④ reap 失败"),
    }
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        berry_libc::park();
    }

    // ⑤ 修复轮回归钉板（位号序敏感：Call 位 SIGHUP=1 < Hold 位
    //    SIGUSR2=12 < 表变位 SIGCONT=18）：USR2 blocked-held（raise 落
    //    held）+ 18 经 berry_signal_send(0,·) 内核置位**零投递**（跨
    //    进程面走 0x86——raise 会自带 dispatch 拿不走「未投递挂起」形
    //    态）→ raise(SIGHUP) 一轮收三路位。h3 内换表 + 解封。五断言：
    let base2 = c2();
    let mut act3 = SigAct {
        sa_sigaction: h3 as *const () as usize,
        sa_mask: [0; 128],
        sa_flags: 0,
        sa_restorer: 0,
    };
    unsafe { sigemptyset(act3.sa_mask.as_mut_ptr()) };
    let mut act4 = SigAct {
        sa_sigaction: h4 as *const () as usize,
        sa_mask: [0; 128],
        sa_flags: 0,
        sa_restorer: 0,
    };
    unsafe { sigemptyset(act4.sa_mask.as_mut_ptr()) };
    if unsafe { sigprocmask(SIG_BLOCK, set2.as_ptr(), core::ptr::null_mut()) } != 0
        || unsafe { raise(SIGUSR2) } != 0
        || c2() != base2
    {
        fails.push("⑤ 前置 blocked 窗 raise 计数漂移");
    }
    if unsafe { sigaction(SIGHUP, &act3, core::ptr::null_mut()) } != 0
        || unsafe { sigaction(SIGCONT, &act4, core::ptr::null_mut()) } != 0
        || berry_signal_send(0, SIGCONT as u32) != 0
    {
        fails.push("⑤ 装 h3/h4/signal_send 非 0");
    }
    if unsafe { raise(SIGHUP) } != 0 || c3() != 1 {
        fails.push("⑤ raise(SIGHUP) 嵌套入口 != 1");
    }
    if c2() != base2 + 1 {
        fails.push("⑤ 嵌套补投计数 != base+1（Hold 即刻落 held 可见性破缺）");
    }
    if c4b() != 1 || c4() != 0 {
        fails.push("⑤ 表变位投递非 h4b 恰一次（delivery-time 表重查破缺）");
    }
    if unsafe { sigprocmask(SIG_BLOCK, core::ptr::null(), oldset.as_mut_ptr()) } != 0
        || unsafe { sigismember(oldset.as_ptr(), SIGUSR2) } != 1
    {
        fails.push("⑤ 包络恢复掩码缺 SIGUSR2");
    }
    if unsafe { sigprocmask(SIG_UNBLOCK, set2.as_ptr(), core::ptr::null_mut()) } != 0 || c2() != base2 + 1
    {
        fails.push("⑤ 收尾 UNBLOCK 残留双投");
    }

    if !fails.is_empty() {
        eprintln!("bsh: sig selftest bad: {:?}", fails);
        berry_libc::park();
    }
    println!("SIG ok n=6");
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
            "bsh 内建：help | echo <text> | env | run <name> | cat <file> | ls | ls <dir> | mkdir <dir> | rm <path> | cd <dir> | pwd | inputpoll | A | B（rootfs 管道）"
        )
        .unwrap();
        // A60：192B 控制台原子消息帽——单行装不下全清单，拆两条各 ≤192B。
        writeln!(
            out,
            "…续：| A && B | A > file | exit | 裸名 = bin/<name> 派发（C-2a uu 家族）"
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
    } else if cmd == "inputpoll" {
        // GUI-4 段②：input 域 POLL 消费端（无参内建——RPC + 真值断言面）。
        inputpoll_output(out)
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
        mmio_va: 0,
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
/// 确定性状态谓词 + 双向真值）。四步全收齐才打（单消息原子）：
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
/// - 步③ du 表观尺寸面（**T-1 回收 2026-10-28**）：`du -b PATTERN.BIN` →
///   `5000\tPATTERN.BIN\n`（原案步③ 原样回嵌——PATTERN.BIN 5000B 在全部
///   造盘模式在位 = 跨相恒定）。原裁因（首跑定标实录 2026-10-27）= uu_du
///   0.12.0 du.rs ~:1168 无条件 `thread::spawn`（打印线程）→ std 线程钩子
///   `run_spawn_hooks` 原子加野指针 → D-abort G1.5 kill；同日破案重归因 =
///   **crt0 原位 TLS 初值污染**（.tbss 窗 = 活 .data.rel.ro 文件字节，
///   docs/挂账-C2c深坑件.md 法证，uu_sort 同案同批），L1 TLS 四全局 + L2b
///   pthread/futex/gift 面落地后 du 家族复通——`-b` = --apparent-size
///   --block-size=1（消 st_blocks 512B 扇区取整口径，字节真值恰 5000）。
///   **run_capture_fs**（du 经 libc open/stat 消费 fs 服务，df 步同款
///   gift 四元装配）。
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
    // 步③（T-1 回收）：du -b 表观尺寸真值——PATTERN.BIN 5000B（造盘定值
    // 跨相恒定；`-b` 消 st_blocks 512B 扇区取整口径）。du 经 libc open/
    // stat 消费 fs 服务 = run_capture_fs gift 四元（步② df 同款）。
    match run_capture_fs("du", None, &["-b", "PATTERN.BIN"]) {
        Some((o, c)) if c == 0 && o.data == b"5000\tPATTERN.BIN\n" && o.eof && o.dropped == 0 => {}
        Some((o, c)) => df_bad(3, c, &o),
        None => {
            eprintln!("bsh: df selftest bad: step=3 du not found");
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

/// M-G3 里程碑演示相自验（C-5，`M3DEMO ok n=5` = rootfs 相第 35 标记，
/// `VM里程碑设计草案.md` §2 C-5）。演示叙事五步全状态谓词（4-5a 纪律
/// ——零竞速）：生态在册 → ls 列卷 → cp 复制 → 管道过滤 → 状态码跨
/// 命令传递落盘读回。
/// ⓪ 生态名单（cpio 真源面 = 「ls /bin」的等价真值——rootfs cpio **就是**
///    bin/ 目录，而 /bin 不在 fsmgr RAM/FAT 面、libc opendir 不可达）：
///    cpio_names 直数 ≥38（mkcpio x38 条目基线——新增 bin 只增不减，
///    下界断言防漂移）+ bsh/grep/hello/ls/cp/cat/ps 七锚；
/// ① ls 消费链（fs gift 面）：`ls /` 列根（RAM 层 ∪ FAT 层，盘在位
///    ——fat 族后）断言 HELLO.TXT + PATTERN.BIN 子串（mkfat 恒在件，
///    相位无关；行数/次序不断言——RAM 残留与交互落盘随相变化）；
/// ② cp 复制链：mkdir `t` + put `t/demo.txt`（字面三行）→ uu_cp 双
///    操作数（C-2a `SpawnSpec::args` 多参面；**collect 帽 30s**——首跑
///    定标实录：uu_cp TCG 下 spawn→exit ≈17s 慢而活，10s 帽结构性误判
///    eof=false，svcmgr `U1_EOF_CAP` 同口径）→ get `t/copy.txt` 逐字节
///    对拍；
/// ③ 管道：`cat /t/demo.txt | grep beta`——cat 文件参数段骑首段 fs
///    gift（v1 单 fs 消费段约束：grep 从 stdin 读零 fs），期望字面
///    `m3 beta\n`（自写 grep stdin 读口径无文件名前缀，U4 步⑤同款）；
/// ④ 状态码 + 落盘 + 读回：`grep -q zzz`（demo 无 zzz → rc 1，GNU -q
///    口径）→ `echo x$?y > T3.TXT`（run_line 真行执行器——`$?` 展开造
///    内容 `x1y\n` 消回声，段 d OUT.TXT 同款纪律；8.3 大写名经
///    do_redirect 落 FAT 根真盘）→ get `T3.TXT` 全等。T3.TXT **故意不
///    清理**：持久化相注入 `cat T3.TXT` 断言 x1y——但本相 boot 自验
///    会覆写它（RW.TXT 同款覆写幂等路径），故跨 boot 字节存活的正证
///    仍是 OUT.TXT f42t；T3.TXT 注入腿验证的是交互 cat 读 FAT 根的
///    活面（弱化面如实记载）。
/// ps 名单步（原案五件之末）由 `PS ok n=` 承接（C-3 svcmgr
/// ps_selftest 账本名单面）——bsh 无法自造 psproto gift 通道，不重拉
/// （拆解决策留档 VM里程碑 §2）。
/// 链序契约：**排 fat 族后**（①步依赖挂载卷在位）——EFI 路无盘 fat
/// park 挡断 → M3DEMO 结构性不打（smoke-image canon 零变化，DF 同款）。
/// fstree 容量窗：峰值在册 FILE 3+2=5 / DIR 1+1=2（MAX 9/17 内），
/// ④毕 cleanup 双文件 + 目录（容量纪律）。失败 eprintln 单串 + park
/// （标记缺失 → smoke FAIL 兜底）。
fn m3demo_selftest(vars: &mut Vec<(String, String)>, status: &mut i32) {
    const M3_DEMO: &[u8] = b"m3 alpha\nm3 beta\nm3 gamma\n";
    let m3_bad = |step: u8, c: i32, o: &RunOut| -> ! {
        let n = o.data.len().min(64);
        eprintln!(
            "bsh: m3demo bad: step={} code={} len={} eof={} dropped={} hex={}",
            step,
            c,
            o.data.len(),
            o.eof,
            o.dropped,
            u3_hex(&o.data[..n])
        );
        berry_libc::park()
    };
    let m3_fail = |step: u8, what: &str| -> ! {
        eprintln!("bsh: m3demo bad: step={} {}", step, what);
        berry_libc::park()
    };

    // ⓪ 生态名单：cpio 直数（纯 bin/ 视图——内建 11 不混入）。
    let mut names: Vec<Vec<u8>> = Vec::new();
    cpio_names(rootfs_h(), b"bin/", &mut |name| names.push(name.to_vec()));
    if names.len() < 38 {
        m3_fail(0, &format!("bin count {} < 38", names.len()));
    }
    for anchor in ["bsh", "grep", "hello", "ls", "cp", "cat", "ps"] {
        if !names.iter().any(|n| n == anchor.as_bytes()) {
            m3_fail(0, &format!("bin missing {}", anchor));
        }
    }

    // ① ls 消费链：列根断言 mkfat 恒在件（子串——uu_ls 短格式多列，
    // 名字完整成 token；行数相位相关不断言）。
    match run_capture_fs("ls", None, &["/"]) {
        Some((o, c)) if c == 0 && o.eof && o.dropped == 0
            && o.data.windows(9).any(|w| w == b"HELLO.TXT")
            && o.data.windows(11).any(|w| w == b"PATTERN.BIN") => {}
        Some((o, c)) => m3_bad(1, c, &o),
        None => m3_fail(1, "ls not found"),
    }

    // ② cp 复制链：RAM 树造源 → uu_cp 双操作数 → 读回对拍。**collect 帽
    //    30s**（首跑定标实录：uu_cp TCG 下 spawn→exit ≈17s——lstat/stat×2/
    //    读写/fchmod 整链串行 fs RPC 慢而活，10s 帽误判 eof=false；30s =
    //    svcmgr U1_EOF_CAP 同口径，U1 的 cp 先例五相在档）。
    if let Err(e) = mkdir_of("t") {
        m3_fail(2, &format!("mkdir t {}", errno_text(e)));
    }
    if let Err(e) = put_file("t/demo.txt", M3_DEMO) {
        m3_fail(2, &format!("put t/demo.txt {}", errno_text(e)));
    }
    const CP_EOF_CAP_NS: u64 = 60 * 1_000_000_000;
    match run_capture_fs_cap("cp", None, &["/t/demo.txt", "/t/copy.txt"], CP_EOF_CAP_NS) {
        Some((o, c)) if c == 0 && o.eof && o.dropped == 0 => {}
        Some((o, c)) => m3_bad(2, c, &o),
        None => m3_fail(2, "cp not found"),
    }
    match get_file("t/copy.txt") {
        Ok(d) if d == M3_DEMO => {}
        r => m3_fail(
            2,
            &format!(
                "copy mismatch {:?}",
                r.map(|v| v.len()).map_err(errno_text)
            ),
        ),
    }

    // ③ 管道：cat（首段 fs gift）| grep beta → 字面常量。
    let segs = [
        PipeSeg {
            name: "cat",
            args: &["/t/demo.txt"],
        },
        PipeSeg { name: "grep", args: &["beta"] },
    ];
    match pipe_capture(&segs, None) {
        Ok((o, c)) if c == 0 && o.data == b"m3 beta\n" && o.eof && o.dropped == 0 => {}
        Ok((o, c)) => m3_bad(3, c, &o),
        Err(PipeFail::NotFound(n)) => m3_fail(3, &format!("{} not found", n)),
        Err(PipeFail::SpawnFail(n)) => m3_fail(3, &format!("{} spawn failed", n)),
    }

    // ④ 状态码跨命令传递 + 落盘 + 读回（T3.TXT 留盘供持久化相注入腿）。
    match run_capture_fs("grep", None, &["-q", "zzz", "/t/demo.txt"]) {
        Some((o, c)) if c == 1 && o.data.is_empty() && o.eof && o.dropped == 0 => {}
        Some((o, c)) => m3_bad(4, c, &o),
        None => m3_fail(4, "grep not found"),
    }
    *status = 1; // $? ← grep -q 真码（rc 已上验，run_line 展开消费它）
    let mut out = std::io::stdout();
    run_line("echo x$?y > T3.TXT", vars, status, &mut out);
    match get_file("T3.TXT") {
        Ok(d) if d == b"x1y\n" => {}
        r => m3_fail(
            4,
            &format!(
                "T3.TXT {:?}",
                r.map(|v| v.len()).map_err(errno_text)
            ),
        ),
    }

    // cleanup：双文件 + 目录（T3.TXT 留盘——持久化相注入腿）。
    if let Err(e) = unlink_of("t/copy.txt") {
        m3_fail(9, &format!("unlink t/copy.txt {}", errno_text(e)));
    }
    if let Err(e) = unlink_of("t/demo.txt") {
        m3_fail(9, &format!("unlink t/demo.txt {}", errno_text(e)));
    }
    if let Err(e) = unlink_of("t") {
        m3_fail(9, &format!("rmdir t {}", errno_text(e)));
    }
    println!("M3DEMO ok n=5");
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
            mmio_va: 0,
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

/// C-2c U3：text 批执行验收（pwd/cat/wc/cut/tr/sort/uniq 七命令 + 四段管道
/// 面）。**七步门审原案两处挂账：一处回收、一处仍裁**：
/// - 步③ head 仍裁出：uu_head 0.12.0 unix stdin 路径 `try_clone_to_owned`
///   → fcntl(F_DUPFD) → berry-libc 桩 -EINVAL → uucore 译 "Invalid
///   input" 退 1（实测 code=1 + 该串）——fd 克隆面（dup/dup2/fcntl）libc
///   刀挂账，head 家族 v1 结构性不可验收（U4 步① 的 `head -c 8` 文件参
///   形态经 fcntl 实现另案收编，与本步 stdin 形态不同面）。
/// - 步④ sort|uniq 与步⑥ 四段链的 sort 段：**T-1 回收（2026-10-28）**——
///   原裁因 = uu_sort 启动期 ctrlc/signal-hook 后台线程踩 std 线程面
///   （`run_spawn_hooks` 解引用空注册表悬垂指针 → outline-atomics 原子加
///   D-abort → 内核 G1.5 kill（空输出无 EOF）；同日破案重归因 = **crt0
///   原位 TLS 初值污染**（.tbss 窗 = 活 .data.rel.ro 文件字节，
///   docs/挂账-C2c深坑件.md 法证），L1 TLS 四全局 + L2b pthread/futex/gift
///   面落地后 sort 家族复通——按原案字面常量回嵌，期望值零改动。
/// 步序映射：1=⓪pwd 2=①cat 3=②wc-c 4=④sort|uniq 5=⑤cut|tr 6=⑥四段链。
/// spawn 合计 = 11 ≤ 12（1+1+1+2+2+4）。feed 面 ≤1.5KB（三常量合计 96B）、
/// 期望面 <32KiB（COLLECT_MAX 口径，实测最大 32B）。断言期望值 = 定标实录
/// **字面常量**（绝不 Rust 镜像计算——P6 A-5 基准漂移雷）：pwd `/\n` /
/// cat 逐字节全等 / wc-c `32\n` / sort|uniq `alpha\nbravo\ncharlie\ndelta\n`
/// / cut|tr `BC\nZ\nEL\n` / 四段链 `4\n`。
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
    // 步 4（④，T-1 回收）：sort | uniq——排序 + 邻去重双段管道（七行 feed
    // → 字典序 → 四行；`alpha\nbravo\ncharlie\ndelta\n` 原案字面常量）。
    let segs4 = [
        PipeSeg { name: "sort", args: &[] },
        PipeSeg { name: "uniq", args: &[] },
    ];
    match pipe_capture(&segs4, Some(U3_SORT_FEED)) {
        Ok((o, c)) if c == 0 && o.data == b"alpha\nbravo\ncharlie\ndelta\n" && o.eof && o.dropped == 0 => {}
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
    // 步 5（⑤）：cut -c 2-3 | tr a-z A-Z——列裁剪 + 大小写映射双段管道
    // （abcd→BC / zz→Z / hello world→EL：短行越界取到即止）。
    let segs45 = [
        PipeSeg { name: "cut", args: &["-c", "2-3"] },
        PipeSeg { name: "tr", args: &["a-z", "A-Z"] },
    ];
    match pipe_capture(&segs45, Some(U3_CUT_FEED)) {
        Ok((o, c)) if c == 0 && o.data == b"BC\nZ\nEL\n" && o.eof && o.dropped == 0 => {}
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
    // 步 6（⑥，T-1 回复四段）：cat | sort | uniq | wc -l——首段开场 + 中段
    // 变换 + 末段收口的四段链原案形态（七行 feed → 4 行 → `4\n` 字面）。
    let segs6 = [
        PipeSeg { name: "cat", args: &[] },
        PipeSeg { name: "sort", args: &[] },
        PipeSeg { name: "uniq", args: &[] },
        PipeSeg { name: "wc", args: &["-l"] },
    ];
    match pipe_capture(&segs6, Some(U3_SORT_FEED)) {
        Ok((o, c)) if c == 0 && o.data == b"4\n" && o.eof && o.dropped == 0 => {}
        Ok((o, c)) => u3_bad(6, c, &o),
        Err(PipeFail::NotFound(n)) => {
            eprintln!("bsh: u3 selftest bad: step=6 {} not found", n);
            berry_libc::park();
        }
        Err(PipeFail::SpawnFail(n)) => {
            eprintln!("bsh: u3 selftest bad: step=6 {} spawn failed", n);
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

/// A5（`docs/事件循环A5设计草案.md` §3.1）：[`fs_gifts`] 的网络三元件——
/// rootfs/fs 双元同款 + netd 端 `RIGHT_SEND|RIGHT_RECV` dup 第三元
/// （env 块 extras 第 3 枚 → gift[6] → libc `GIFT_H[4]`——协议在册零
/// 扩容，berry-exec ≤3 断言恰满）。权利位对源 = svcmgr 给 bsh 的全量
/// 形态（fs 端 SEND|RECV 同形，A5-1 对源落定：svcmgr `net_b` 是 fresh
/// channel 端直递非 dup——bsh 场景必须 dup 既有端）。
/// `netd_h()==0`（EFI/裸拓扑无 netdev）→ None **静默跳过**（EVS/SKT/EPL
/// 拓扑门契约同款——canon 零变化按构造）；dup 半途失败双关防漏同
/// [`fs_gifts`]。**least privilege**：消费者 = nets_selftest（A5-2）——
/// 交互/管道/裸 spawn 面零改动（既有行为零漂移按构造）。
fn net_gifts() -> Option<(Handle, Handle, Handle)> {
    if netd_h() == 0 {
        return None;
    }
    let (rootfs_dup, fs_dup) = fs_gifts()?;
    let mut netd_dup = 0 as Handle;
    if berry_handle_duplicate(netd_h(), RIGHT_SEND | RIGHT_RECV, &mut netd_dup) != 0 {
        // 半途失败：前两枚已 dup 成功——三枚都试关（未成的 -EBADF 无害）。
        let _ = berry_handle_close(rootfs_dup);
        let _ = berry_handle_close(fs_dup);
        let _ = berry_handle_close(netd_dup);
        return None;
    }
    Some((rootfs_dup, fs_dup, netd_dup))
}

fn run_capture_fs(name: &str, stdin: Option<&[u8]>, args: &[&str]) -> Option<(RunOut, i32)> {
    run_capture_fs_cap(name, stdin, args, TIMEOUT_NS)
}

/// [`run_capture_fs`] 的宽帽版（C-5）：collect 活动帽按命令定值——uu_cp
/// 17s 实测（[`collect_cap`] 注），30s = svcmgr U1_EOF_CAP 同口径；余轻量
/// uu 命令维持 10s（u2/u3/u4 先例全在档）。
fn run_capture_fs_cap(
    name: &str,
    stdin: Option<&[u8]>,
    args: &[&str],
    cap_ns: u64,
) -> Option<(RunOut, i32)> {
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
    run_capture_gifts_cap(name, stdin, args, &[rootfs_dup, fs_dup], cap_ns)
}

fn run_capture_gifts(
    name: &str,
    stdin: Option<&[u8]>,
    args: &[&str],
    gifts: &[Handle],
) -> Option<(RunOut, i32)> {
    run_capture_gifts_cap(name, stdin, args, gifts, TIMEOUT_NS)
}

/// [`run_capture_gifts`] 的宽帽版（C-5）——collect 帽经参传入（见
/// [`collect_cap`]）。
fn run_capture_gifts_cap(
    name: &str,
    stdin: Option<&[u8]>,
    args: &[&str],
    gifts: &[Handle],
    cap_ns: u64,
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
        mmio_va: 0,
    };
    let child = match spawn_rootfs_elf(rootfs_h(), &spec) {
        Ok(c) => c,
        Err(_) => berry_libc::park(), // 自验面：链断 loud（标记缺失兜底）
    };
    let o = collect_cap(out_s, cap_ns);
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
/// 入队，空队 wait READABLE 活动帽，零长消息 = EOF 终点）——10s 常规帽
/// （交互管道/轻量命令）。uu 家族 fs 重命令走 [`collect_cap`] 拿宽帽。
fn collect(h: Handle) -> RunOut {
    collect_cap(h, TIMEOUT_NS)
}

/// [`collect`] 的宽帽版（C-5 首跑定标实录）：uu_cp 双操作数在 TCG 下
/// spawn→exit 实测 ≈17s（fs RPC 链慢而活——lstat/stat/读写整链 ~15 次串行
/// RPC，时间戳取证 CD ok→m3demo bad 25s 扣 ⓪① ≈8s），10s 帽结构性误判
/// eof=false（EOF 实际已发、collect 先弃）。「client 收侧帽按服务端 worst
/// 路径成本定值」——svcmgr `U1_EOF_CAP`=30s（U1 的 cp 同款先例，五相
/// smoke 在档），同口径 30s。
fn collect_cap(h: Handle, cap_ns: u64) -> RunOut {
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
        // 空队：阻塞等 READABLE（活动帽，收包刷新）
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + cap_ns, &mut obs) != 0 {
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
