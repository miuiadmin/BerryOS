//! 输出/stdio —— fd 0/1/2 → console channel（真实现档）
//!
//! fd 模型（C-1 刀 2 起）：0/1/2 = console/stdin 原路径**一字不动**（R6）；
//! fd≥3 = fs-backed（`fs.rs`——fd 表游标 + close 即 commit，C-1 两裁决）。
//! 1/2 = console 发端别名（写方向）、0 = env 块 stdin 端（d-3b 起真输入
//! ——channel recv，零长消息 = EOF 约定），都经**env 块**交付（d-3a，见
//! `berry_libc_boot`）；fd≥3 的 read/write/close/lseek 分流 `fs.rs`（不在
//! 册 fd 的 EBADF 面由那边统一收口）。
//!
//! 发送契约（雷 2 教训 + A-2 修订）：**阻塞重发**——`-EBUSY`（console 队列
//! 满）→ `clock_read` + `wait_one(console, WRITABLE, 10s)` 睡等重试（超时
//! 重臂）。纯自旋曾是 G-eco-d 的短命验收程序可容忍形态（自旋窗有界），
//! A-2 起常驻 std 服务（bsh）上线——自旋者永不出 runqueue，RR 环序冻结、
//! 排水窗口被占优者垄断 = 长跑饱和下结构性饿死（雷 2，`user.rs` 八站点
//! 同款修法）；阻塞者经 recv 侧 `wake_matching` 公平分槽。分片 ≤192B
//! （channel inline 帽 256）。

use core::ptr;

use berry_fsclient::AliasTable;
use berry_sys::{berry_channel_recv, berry_channel_send, berry_clock_read,
                berry_object_wait_one, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE,
                SIGNAL_WRITABLE};

use crate::{c_char, c_int, c_long, c_void, size_t, ssize_t, FILE};

static mut CONSOLE_H: u32 = 0;
static mut STDIN_H: u32 = 0; // d-3b 预留（env 块 handles[1]；read 面随后开）
// A-2/P6：env 块 extras 四槽（gift[4..] 位）——extras[0] = rootfs VMO
// READ dup（bsh）或 P6 A-1 第三/四位（fsmgr 得服务端端 `gift2_h()`、bsh
// 得 fs 客户端端 `fs_h()`——同槽两位经 spawn 侧的 gift 排布区分，见
// berry-exec 注）；A2-3 第三位 = netd 服务端端（bsh EVS 自验——
// `netd_h()`）；GUI-4 段② 第四位 = input 客户端端（bsh `inputpoll`——
// `input_h()`）；GUI-2 段③ 起 extras 首/次位再挂 gfxdemo 的 present/hdi
// 客户端端（`present_h()`/`hdi_h()`）、第三/四位再挂 guifbd 的 present/
// hdi 服务端端（`present_srv_h()`/`hdi_srv_h()`）；GUI-5 起首位再产两枚
// ——gfxdemo 的 wm 域客户端端（`wm_cli_h()`——端点换手：present/hdi 客
// 户端端改随 gui-wmd，`present_h()` 保留旧注）与 gui-wmd 的 wm 服务端
// 端（`wm_srv_h()`）、次位再产 inputd 的 wmd 推送客户端端
// （`wm_push_h()`）与 gui-wmd 的推送服务端端（`wm_push_srv_h()`）、
// 第三/四位再产 gui-wmd 的 present/hdi 客户端端（`wm_present_h()`/
// `wm_hdi_h()`——guifbd 客户端端换手件）——同槽多产面惯例
// （gift2/gift3 同款）；GUI-8 段② 起第三位再产 berry-launcher 的 app
// 域客户端端（`app_cli_h()`——app 域 serve 对的客户端端，dormant 至
// 段③ 消费）。
// T-1 自描述对占固定位 gift[2]/[3]（前缀紧致律——自描述对前移、extras
// 后挪，产侧 extra<2 不再中段留 0 洞）：GIFT_H[2] = self_proc MANAGE|
// DUPLICATE dup（`self_proc_h()`——pthread_create 的 thread_create 面）、
// GIFT_H[3] = self_vmar MAP|MANAGE|DUPLICATE dup（`self_vmar_h()`——
// 子线程栈映射/回收面）。
static mut GIFT_H: [u32; 6] = [0; 6];
static mut STACK_TOP: usize = 0;

// —— argv 面（d-3a env 块）：静态落位（名区 256B + 16 槽 + NULL 终结）——
static mut ARGV_STR: [u8; 256] = [0; 256];
static mut ARGV_PTR: [*const c_char; 17] = [ptr::null(); 17];

/// crt0 装配口：`_start` 汇编 `ldr x1, [BERRY_ARGV]` 取 argv 数组基址。
#[no_mangle]
pub static mut BERRY_ARGV: *const *const c_char = ptr::null();

/// crt0 落种口（`_start` 汇编直调）：x0 = **env 通道端**（bootstrap），
/// x1 = 初始栈顶（attr 谎报面数据源）。
///
/// env 块协议（BerryOS processargs v1，Q2 之答——Zircon 同形的最小面）：
/// svcmgr 在 `process_start` **前**把环境消息压进 env 端队列（确定性先行，
/// 无竞速）——payload = NUL 分隔 argv 字节串（首项 = 程序名）、随行
/// handles 按位语义：[0] = console 发端别名（stdout/stderr）、[1] = stdin
/// 端（d-3b）。解析落 static 后返回 argc（argv 基址经 `BERRY_ARGV` 交汇编）。
/// 协议破缺（recv 失败/无 handle）→ argc=0 + console=0：后续任何打印在
/// handle 0 上自旋死 → 标记缺失 = smoke FAIL 兜底（loud 不静默）。
#[no_mangle]
pub extern "C" fn berry_libc_boot(env: u32, sp0: usize) -> c_int {
    unsafe {
        STACK_TOP = sp0;
        stderr = ptr::addr_of_mut!(STDERR_FILE);
        stdout = ptr::addr_of_mut!(STDOUT_FILE);
        let mut blob = [0u8; 256];
        let mut nb = blob.len();
        // A-2：容量 2→3；P6 A-1：3→4；T-1：4→6；A2-3：6→7；GUI-4 段②：7→8。
        // 前缀紧致律布局
        // （与 berry-exec 产侧对偶）：[0]=console 发端别名、[1]=stdin 端、
        // [2]/[3]=T-1 自描述对（self_proc/self_vmar——spawn 侧条件装配，
        // 老调用面不送则位缺 0，`self_proc_h()==0` 即 pthread_create 的
        // EAGAIN 守门）、[4..]=extras（rootfs VMO READ dup 或 fsmgr 端 +
        // fs/blk 客户端端 + A2-3 netd 端 + GUI-4 段② input 端，按 spawn 侧
        // 排布区分，落 `GIFT_H` 四槽）。nh 取值域 = {1（无 stdin 老面）,
        // 4..=8（stdin → 自描述对必随行）}——nhandles 单调门控 presence，
        // 按实收落种向后兼容。
        let mut hs = [0u32; 8];
        let mut nh = hs.len();
        if berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh) != 0 {
            return 0;
        }
        if nh >= 1 {
            CONSOLE_H = hs[0];
        }
        if nh >= 2 {
            STDIN_H = hs[1];
        }
        if nh >= 3 {
            GIFT_H[2] = hs[2];
        }
        if nh >= 4 {
            GIFT_H[3] = hs[3];
        }
        if nh >= 5 {
            GIFT_H[0] = hs[4];
        }
        if nh >= 6 {
            GIFT_H[1] = hs[5];
        }
        if nh >= 7 {
            GIFT_H[4] = hs[6];
        }
        if nh >= 8 {
            GIFT_H[5] = hs[7]; // GUI-4 段②：input 端（svcmgr 预拉分种）
        }
        // argv 解析：NUL 分隔字节串逐个拷进名区、记指针。
        let (mut argc, mut i, mut o) = (0usize, 0usize, 0usize);
        while i < nb && argc < 16 {
            let start = o;
            while i < nb && blob[i] != 0 {
                if o < ARGV_STR.len() {
                    ARGV_STR[o] = blob[i];
                    o += 1;
                }
                i += 1;
            }
            if i >= nb {
                break; // 尾串无 NUL：不完整，丢弃（svcmgr 侧恒补 NUL）
            }
            if o < ARGV_STR.len() {
                ARGV_STR[o] = 0;
                o += 1;
            }
            i += 1;
            ARGV_PTR[argc] = ARGV_STR.as_ptr().add(start);
            argc += 1;
        }
        ARGV_PTR[argc] = ptr::null();
        BERRY_ARGV = ARGV_PTR.as_ptr();
        argc as c_int
    }
}

/// main 意外返回后的收口（crt0 跳入）。`bl main` 返回后 x0 仍持 main 返回码
/// （`bl` 不清 x0——零参桩读首参的合法面，见 lib.rs「ABI 口径」）；std 正路
/// 在 lang_start 内已 `libc::exit`，到这里的都是旁路，语义照走真退出。
#[no_mangle]
pub extern "C" fn berry_libc_park_after_main(code: c_int) -> ! {
    exit(code)
}

/// stdout 端零长 EOF（d-3c）：`park` 前一次性补发——d-3b「零长消息 = EOF」
/// 约定的**写侧**对称面。（hexyl 的 stdout 是 svcmgr 交接的私有通道端，
/// svcmgr 收到 EOF 才知输出终了（确定性验收，不赌超时）；对真 console 端
/// （stdhello 等）只是 0 字节消息，console 排水写 0 字节无扰。满队阻塞等
/// WRITABLE（`console_write` 同款，A-2 修订——自旋是雷 2 形态）；其余错误
/// 码放弃（协议破缺面自有更 loud 的死法）。
pub fn send_stdout_eof() {
    let h = unsafe { CONSOLE_H };
    if h == 0 {
        return;
    }
    let z = 0u8;
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, &z, 0, ptr::null(), 0);
        if s == 0 || s != BERRY_ERR_BUSY {
            return;
        }
        if deadline == 0 {
            let mut now = 0u64;
            if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                continue;
            }
            deadline = now + 10_000_000_000;
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs);
        deadline = 0;
    }
}

/// env 块 handles[1] 的 stdin 通道端（C-4b Ctrl-C 杀链消费：bsh 前台等待
/// 经 wait_many 并监 stdin READABLE，0x03 到达即 terminate(130)）；0 = 未
/// 交付（老 spawn 面），调用方退化为不监听 stdin。
pub fn stdin_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { STDIN_H }
}

/// env 块 handles[0] 的 console 发端别名（app-2：lifecycle 非阻塞诊断口
/// 消费——64-cap try-spin 面，与 [`console_write`] 的 EBUSY 阻塞重试形态
/// 分立〔草案 §2.2 诊断口纪律/§8.3 R3〕）；0 = env 块未解出（纯丢行）。
pub fn console_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { CONSOLE_H }
}

/// env 块 extras[0]（gift[4] 位）的 rootfs VMO handle（A-2：0 = 未交付——
/// svcmgr 系老程序；bsh 持 READ dup 经 berry-exec 流式取子程序 ELF）。
pub fn rootfs_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[0] }
}

/// env 块 extras[0]（gift[4] 位）的同槽访问器（P6 A-1：fsmgr 的 fs 服务端端——svcmgr 建
/// fs 对后经 env 块 extras 首位交付；0 = 未交付 = park 挂起）。与 [`rootfs_h`]
/// 同槽不同名：spawn 侧同一 gift 位对两种程序承载不同对象。
pub fn gift2_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[0] }
}

/// env 块 extras[0]（gift[4] 位）的同槽访问器（GUI-2 段③：gfxdemo 的
/// present 域 channel 客户端端——svcmgr 预拉 guifbd 段建对后客户端端随
/// gfxdemo 预拉 extras 首位直递（fresh 端非 dup，fs_b/net_b 先例），
/// gfxdemo 经它发 berry-guiproto present RPC；0 = 未交付 = guifbd 未起
/// （rootfs 相 arm 静默跳过 / bsh 手跑恒降级），调用方走 present absent
/// 退出行）。与 [`rootfs_h`]/[`gift2_h`] 同槽不同名：spawn 侧同一 gift
/// 位对不同程序承载不同对象（gift3 同款惯例）。GUI-5 起 gfxdemo 改连
/// wm 域经 gui-wmd 中继（[`wm_cli_h`] 同槽顶替——ADR #61 端点换手）；
/// 本访问器保留（旧 spawn 面兼容注记）。
pub fn present_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[0] }
}

/// env 块 extras[0]（gift[4] 位）的同槽访问器（GUI-5：gfxdemo 的 wm 域
/// channel 客户端端——svcmgr wmd arm 建对后客户端端随 gfxdemo 预拉
/// extras 首位直递（fresh 端非 dup，fs_b/net_b 先例），gfxdemo 的 wm 六
/// op + present/hdi 中继经它发（ADR #61 端点换手——present/hdi 客户端
/// 端改随 gui-wmd）；0 = 未交付 = wmd 未起（rootfs 相 arm 静默跳过 /
/// bsh 手跑恒降级），调用方走 wm absent 退出行）。GUI-8 前置 wiring 刀起
/// berry-launcher 同槽消费：svcmgr launcher arm 交付 wmd 客户端端 dup 柄
/// （门控 + 一次 WM_GETINFO + 单行 LAUNCH 非标记日志后 park 永驻）。与
/// [`rootfs_h`]/[`gift2_h`]/[`present_h`] 同槽不同名：spawn 侧同一 gift
/// 位对不同程序承载不同对象（gift3 同款惯例）。
pub fn wm_cli_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[0] }
}

/// env 块 extras[0]（gift[4] 位）的同槽访问器（GUI-5：gui-wmd 的 wm 域
/// channel 服务端端——svcmgr wmd arm 建对后随 extras 首位 take 迁入，
/// serve 循环 wm 路经它收 wm 六 op + present/hdi 中继请求；0 = 未交付 =
/// 协议破缺面（EFI 链 wmd 恒有——0 值 eprintln+park 兜底）。与
/// [`rootfs_h`]/[`gift2_h`]/[`present_h`]/[`wm_cli_h`] 同槽不同名：spawn
/// 侧同一 gift 位对不同程序承载不同对象（netd_h/present_srv_h 同款
/// 多产面惯例）。
pub fn wm_srv_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[0] }
}

/// env 块 extras[0]（gift[4] 位）的同槽访问器（sim-S1 M2-b：simbattery 的
/// ACK 通道 SEND 端——boot 手搭 simbattery↔task_simcon ACK 对后随 svcmgr
/// 预拉 extras 首位交付，服务逐帧回 9B ACK（宿主 client 读回对拍面，
/// `docs/sim设备通道与契约设计-v1.1.md` §6）；0 = 未交付 = 未在 sim 门相
/// spawn（bsh 手跑/裸相），调用方单行诊断降级 park。与 [`rootfs_h`]/
/// [`gift2_h`]/[`present_h`]/[`wm_cli_h`]/[`wm_srv_h`] 同槽不同名：spawn
/// 侧同一 gift 位对不同程序承载不同对象（多产面惯例）。
pub fn sim_ack_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[0] }
}

/// env 块 extras[1]（gift[5] 位）的 fs 客户端端（P6 A-1：bsh 的 `>`/`cat`/`ls` 走它发
/// RPC；0 = 未交付 = fs 未拉起，调用方降级「fs not available」不死）。
pub fn fs_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[1] }
}

/// env 块 extras[0]（gift[4] 位）的同槽访问器（app-1：apptomb / 未来生命周期
/// app 的 lifecycle 通道端——svcmgr tomb_selftest 建对后端 B 随 extras 首位
/// 迁入，up/beat/resumed/saved/restored 上报与 PAUSESV1 下行都走它；
/// 0 = 未交付 = 未在生命周期 spawn 面。与 [`rootfs_h`] 族同槽不同名（多产面
/// 惯例）。v0 apptomb 自解析 env 块不经 libc——本访问器为规范 §4.1-5 回调
/// 三口/app-2 的 libc 路备。
pub fn lifecycle_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[0] }
}

/// env 块 extras[1]（gift[5] 位）的同槽访问器（app-1 段 b：墓碑 state VMO
/// ——appd 预建 64KiB 定尺寸（结构性帽）+ commit 后 dup 授子；relaunch 时
/// duplicate(READ|WRITE)（复活者再入 pause 循环写权——草案 v1.2 §4，
/// 不收紧只读）。与 [`fs_h`] 同槽不同名）。
pub fn state_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[1] }
}

/// env 块 extras[1]（gift[5] 位）的同槽访问器（P6 A-5：fsmgr 的块设备服务端——svcmgr
/// 预拉 vblk 后把 blk_a 经 fsmgr 的 env 块 extras 次位交付，fsmgr 据此读 FAT
/// 卷；0 = 未交付 = 无 FAT 层，RAM-only 降级续走）。与 [`fs_h`] 同槽不同
/// 名：spawn 侧同一 gift 位对两种程序承载不同对象（bsh 拿 fs 端 / fsmgr
/// 拿 blk 端，每进程 env 块独立无冲突——extras[0] 的 rootfs/gift2 同款惯例）。
pub fn blk_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[1] }
}

/// env 块 extras[1]（gift[5] 位）的同槽访问器（GUI-1 段②：guifbd 的
/// console fan-out 上屏端 endB_fb——svcmgr 预拉 guifbd 时随 extras 次位
/// take 迁入，recv 循环消费 console 转发的输出字节流；0 = 未交付 = 非
/// EFI-GOP 链理论不可达，调用方 park 兜底）。与 [`fs_h`]/[`blk_h`] 同槽
/// 不同名：spawn 侧同一 gift 位对不同程序承载不同对象（extras[0] 的
/// rootfs/gift2 同款惯例）。
pub fn gift3_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[1] }
}

/// env 块 extras[1]（gift[5] 位）的同槽访问器（GUI-2 段③：gfxdemo 的
/// hdi 域 channel 客户端端——随 gfxdemo 预拉 extras 次位直递，gfxdemo 的
/// REG_VBLANK/VBLANK_WAIT/PREPARE/COMMIT 经它发；0 = 未交付 = 与
/// [`present_h`] 同门降级）。与 [`fs_h`]/[`blk_h`]/[`gift3_h`] 同槽不同
/// 名：spawn 侧同一 gift 位对不同程序承载不同对象。GUI-5 起 gfxdemo 的
/// hdi op 改经 wm 域中继（[`wm_cli_h`] 面），本访问器保留旧注。
pub fn hdi_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[1] }
}

/// env 块 extras[1]（gift[5] 位）的同槽访问器（GUI-5：gui-inputd 的 wmd
/// 推送 channel 客户端端——svcmgr inputd arm 随 extras 次位追加直递
/// （WMD_PUSH_B 非 0 追加形；0 = 未交付 = wmd 未起，inputd 退单通道现
/// 行为——`wm_push_h()==0` 门控））。与 [`fs_h`]/[`blk_h`]/[`gift3_h`]/
/// [`hdi_h`] 同槽不同名：spawn 侧同一 gift 位对不同程序承载不同对象
/// （多产面惯例）。
pub fn wm_push_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[1] }
}

/// env 块 extras[1]（gift[5] 位）的同槽访问器（GUI-5：gui-wmd 的
/// wmd↔inputd 推送对服务端端——inputd 的 WM_TABLE_WAIT 在此 park 应答
/// 窗口表快照；0 = 未交付 = [`wm_srv_h`] 同款 eprintln+park 兜底）。与
/// [`fs_h`]/[`blk_h`]/[`gift3_h`]/[`hdi_h`]/[`wm_push_h`] 同槽不同名：
/// spawn 侧同一 gift 位对不同程序承载不同对象（多产面惯例）。
pub fn wm_push_srv_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[1] }
}

/// env 块 extras[2]（gift[6] 位）访问器（A2-3：netd 服务端端 NET_A dup——
/// svcmgr 拉起 bsh 时随 extras 第三位交付，bsh 的 EVS 自验（echo server）
/// 经它发 berry-netdproto RPC；0 = 未交付 = 无 vnet 链（EFI 路/裸跑），
/// EVS 静默跳过）。GUI-2 段③ 起与 [`present_srv_h`] 同槽不同名（第三位
/// 对 bsh 承载 netd 端 / 对 guifbd 承载 present 服务端端——多产面惯例
/// 回填 extras[0]/[1]）。
pub fn netd_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[4] }
}

/// env 块 extras[2]（gift[6] 位）的同槽访问器（GUI-2 段③：guifbd 的
/// present 域 channel 服务端端——svcmgr 预拉 guifbd 时随 extras 第三位
/// take 迁入，serve 循环 present 路经它收 berry-guiproto present RPC；
/// 0 = 未交付 = 协议破缺面（EFI 链 guifbd 恒有——0 值 eprintln+park
/// 兜底）。与 [`netd_h`] 同槽不同名：spawn 侧同一 gift 位对不同程序
/// 承载不同对象（extras[0]/[1] 的多产面惯例同款）。
pub fn present_srv_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[4] }
}

/// env 块 extras[2]（gift[6] 位）的同槽访问器（GUI-5：gui-wmd 的 present
/// 域 channel 客户端端——**guifbd 客户端端换手件**：GUI-2 段③ 随 gfxdemo
/// 直递的客户端端，GUI-5 起改随 wmd extras 第三位直递，present 六 op 透
/// 传中继经它发（启动序②RELEASE_WAIT 清前世孤儿别名 / ③GETINFO 缓存
/// 几何 + 运行期逐调用中继——真值源恒 guifbd）；0 = 未交付 =
/// [`wm_srv_h`] 同款 eprintln+park 兜底）。与 [`netd_h`]/
/// [`present_srv_h`] 同槽不同名：spawn 侧同一 gift 位对不同程序承载
/// 不同对象（多产面惯例）。
pub fn wm_present_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[4] }
}

/// env 块 extras[2]（gift[6] 位）的同槽访问器（GUI-8 段②：berry-launcher
/// 的 app 域 channel 客户端端——svcmgr launcher arm 门内 `channel_pair()`
/// 建对后客户端端随 extras 第三位直递，launcher 经它发 berry-appproto
/// APP_LIST/APP_SPAWN RPC（GUI-8 草案 §5；服务端 = svcmgr 稳态 serve
/// 常驻第二通道）；0 = 未交付 = app 域面缺，非 launcher 程序恒 0）。槽序
/// 固定注记：launcher extras 前缀紧致装配 wm→fs→app→input（[`wm_cli_h`]
/// /[`fs_h`]/[`app_cli_h`]/[`input_h`] 四访问器面），app 恒在 extras[2]
/// ——fs 缺席的 recipe 现不存在。与 [`netd_h`]/[`present_srv_h`]/
/// [`wm_present_h`] 同槽不同名：spawn 侧同一 gift 位对不同程序承载不同
/// 对象（多产面惯例）。dormant 访问器——段③ launcher 主体才消费（RPC
/// 面随段③ 接线），段② 先落编译面。
pub fn app_cli_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[4] }
}

/// env 块 extras[3]（gift[7] 位）访问器（GUI-4 段②：input 域 channel
/// 客户端端 INPUT_A dup——svcmgr 拉起 bsh 时随 extras 第四位交付，bsh 的
/// `inputpoll` 经它发 berry-guiproto INPUT_POLL RPC；0 = 未交付 = inputd
/// 未起（裸/B4/EFI 拓扑），调用方静默降级）。GUI-2 段③ 起与
/// [`hdi_srv_h`] 同槽不同名（第四位对 bsh 承载 input 端 / 对 guifbd
/// 承载 hdi 服务端端——netd_h/present_srv_h 同款）。
pub fn input_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[5] }
}

/// env 块 extras[3]（gift[7] 位）的同槽访问器（GUI-2 段③：guifbd 的
/// hdi 域 channel 服务端端——随 guifbd extras 第四位 take 迁入，serve
/// 循环 hdi 路经它收软 vsync 四 op（REG_VBLANK/VBLANK_WAIT/PREPARE/
/// COMMIT）；0 = 未交付 = [`present_srv_h`] 同款 eprintln+park 兜底）。
/// 与 [`input_h`] 同槽不同名：spawn 侧同一 gift 位对不同程序承载不同
/// 对象（netd_h/present_srv_h 同款）。
pub fn hdi_srv_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[5] }
}

/// env 块 extras[3]（gift[7] 位）的同槽访问器（GUI-5：gui-wmd 的 hdi 域
/// channel 客户端端——present 换手同款（guifbd 客户端端换手件），hdi 四
/// op 透传中继经它发；0 = 未交付 = [`wm_srv_h`] 同款 eprintln+park 兜
/// 底）。与 [`input_h`]/[`hdi_srv_h`] 同槽不同名：spawn 侧同一 gift 位对
/// 不同程序承载不同对象（多产面惯例）。
pub fn wm_hdi_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[5] }
}

pub fn stack_top() -> usize {
    unsafe { STACK_TOP }
}

// —— T-1 自描述对访问器（pthread.rs 消费）——

/// env 块 gift[2] 的**自身 Process** handle（MANAGE|DUPLICATE dup——
/// `pthread_create` 经它 `thread_create` 拉兄弟线程）。0 = 未交付（老
/// spawn 面）= `pthread_create` 返 EAGAIN（std Builder::spawn 走 Err）。
pub fn self_proc_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[2] }
}

/// env 块 gift[3] 的**自身 root Vmar** handle（MAP|MANAGE|DUPLICATE dup
/// ——子线程栈 `vmar_map`/join `vmar_unmap` 面）。0 = 未交付，同 EAGAIN 守门。
pub fn self_vmar_h() -> u32 {
    // SAFETY: 只读；唯一写点在 crt0 `berry_libc_boot`（先于 main）。
    unsafe { GIFT_H[3] }
}

/// console 发送：分片 + EBUSY 阻塞等 WRITABLE（见模块头契约，A-2 修订）。
pub fn console_write(bytes: &[u8]) {
    let mut off = 0usize;
    while off < bytes.len() {
        let n = core::cmp::min(192, bytes.len() - off);
        let p = unsafe { bytes.as_ptr().add(off) };
        let mut deadline = 0u64;
        loop {
            let s = berry_channel_send(unsafe { CONSOLE_H }, p, n, ptr::null(), 0);
            if s == 0 {
                break;
            }
            if s != BERRY_ERR_BUSY {
                // -EAGAIN 对 send 不发生；其余错误码（handle 失效等）阻塞
                // 等也无出路——原地重试（协议破缺面自有更 loud 的死法）。
                continue;
            }
            // -EBUSY（满）：阻塞等 WRITABLE（10s 帽，醒来/超时都重臂）——
            // 雷 2：自旋者永不出 runqueue，长跑饱和下结构性饿死。
            if deadline == 0 {
                let mut now = 0u64;
                if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                    continue; // 时钟不可用：退化为自旋（EL0 可抢占 = 有界）
                }
                deadline = now + 10_000_000_000;
            }
            let mut obs = 0u64;
            let _ = berry_object_wait_one(
                unsafe { CONSOLE_H },
                SIGNAL_WRITABLE,
                deadline,
                &mut obs,
            );
            deadline = 0; // 重臂（命中后若仍满则续等——公平排队）
        }
        off += n;
    }
}

#[inline]
fn fd_ok(fd: c_int) -> bool {
    (0..=2).contains(&fd)
}

// —— console 别名面（C-2c 挂账刀：fcntl F_DUPFD/F_DUPFD_CLOEXEC 的 fd
//    0/1/2 克隆——uu_head stdin 路径 try_clone_to_owned 的真身）——
// 别名槽与 fd 表（fd 3+）共享同一段编号空间、双侧 alloc 互让（正方向
// `AliasTable::alloc` 跳 FDT 在册位 / 反方向 `FdTable::alloc_skipping`
// 跳别名在册位——同号双语义 = read/write 先撞别名翻译、FDT 表项结构性
// 不可达的静默错，双侧防线在 berry-fsclient host 测试钉板）。别名无独立
// 状态：read/write/writev/isatty 先翻译回底 fd 走 console 原路径（共享
// PEND 残段——与 POSIX dup 共享 file description 同义）；close 摘槽即
// 成功（console 是进程级资产，同 close(1) 不真关口径）；lseek = ESPIPE
// （console 非 seekable——uu_head stream_position 失败即走缓冲回退路，
// 译成功 = 静默错位，fs.rs 闸）。
static mut ALIASES: AliasTable = AliasTable::new();

/// 别名 → 底 fd：非别名 = None。pub(crate)：fcntl 分发（stubs.rs）与
/// lseek 闸（fs.rs）/open 避位（fs.rs）用。
pub(crate) fn alias_base(fd: c_int) -> Option<c_int> {
    // SAFETY: 只读；唯一写点 = fcntl（stubs.rs，单线程 lib.rs ABI 口径）。
    unsafe { ALIASES.base_of(fd) }
}

/// fcntl F_DUPFD 面：console `base`（0/1/2）装一条 ≥`start` 的最低可用
/// 别名（满表/段帽尽 = None → 调用方 EMFILE）。
pub(crate) fn alias_alloc(base: c_int, start: c_int) -> Option<c_int> {
    // SAFETY: 单线程 static mut 直写（lib.rs ABI 口径）；FDT 只读借用。
    unsafe { ALIASES.alloc(base, start, &*core::ptr::addr_of!(crate::fs::FDT)) }
}

/// 摘别名槽（close 面）：Some(base) = 别名已摘 / None = 非别名。
pub(crate) fn alias_take(fd: c_int) -> Option<c_int> {
    // SAFETY: 单线程 static mut 直写（同上）。
    unsafe { ALIASES.take(fd) }
}

#[no_mangle]
pub extern "C" fn write(fd: c_int, buf: *const c_void, n: size_t) -> ssize_t {
    let r = write_inner(fd, buf, n);
    crate::signal::may_dispatch(); // A7-4 ③b：公共入口投递点（§3.3）
    r
}

fn write_inner(fd: c_int, buf: *const c_void, n: size_t) -> ssize_t {
    // 别名先翻译回底 fd（console 克隆——别名无独立状态，见别名面注释）。
    let fd = alias_base(fd).unwrap_or(fd);
    if !fd_ok(fd) {
        // socket 段（15..=22，A3-2）：分派先于 fs（两段无交集，先判 =
        // 归属谓词显式化）；sock_send 收口 EBADF/分片/部分写语义。
        if crate::net::sock_valid(fd) {
            return crate::net::sock_send(fd, buf as *const u8, n);
        }
        // epoll 段（23..=26，A4-1）：epoll fd 不可写（Linux 同款 EINVAL
        // ——interest 注册只走 epoll_ctl）。
        if crate::net::epoll_valid(fd) {
            crate::set_errno(crate::EINVAL);
            return -1;
        }
        // fd≥3 = fs-backed 写（fs.rs 收口 EBADF/游标/分片 PUT）；fd<0 同落
        // 那边的 EBADF 面。
        return crate::fs::write_fd(fd, buf, n);
    }
    if n == 0 {
        return 0;
    }
    let bytes = unsafe { core::slice::from_raw_parts(buf as *const u8, n) };
    console_write(bytes);
    n as ssize_t
}

#[no_mangle]
pub extern "C" fn writev(fd: c_int, iov: *const Iovec, cnt: c_int) -> ssize_t {
    let r = writev_inner(fd, iov, cnt);
    crate::signal::may_dispatch(); // A7-4 ③b：公共入口投递点（§3.3，修复轮补）
    r
}

fn writev_inner(fd: c_int, iov: *const Iovec, cnt: c_int) -> ssize_t {
    // 别名先翻译（同 write——console 克隆共底 fd）。
    let fd = alias_base(fd).unwrap_or(fd);
    if !fd_ok(fd) {
        // socket 段（A3-2）：逐 iov 走 sock_send（部分写返回已写量——
        // fs 路同语义）。
        if crate::net::sock_valid(fd) {
            if cnt <= 0 {
                return 0;
            }
            let mut total: ssize_t = 0;
            for i in 0..cnt as usize {
                let v = unsafe { &*iov.add(i) };
                if v.len > 0 {
                    let r = crate::net::sock_send(fd, v.base as *const u8, v.len);
                    if r < 0 {
                        return if total > 0 { total } else { -1 };
                    }
                    total += r;
                }
            }
            return total;
        }
        // epoll 段（A4-1）：不可写（write 同款 EINVAL）。
        if crate::net::epoll_valid(fd) {
            crate::set_errno(crate::EINVAL);
            return -1;
        }
        // fs-backed：逐 iov 走 fs 写（部分写返回已写量——write_fd 同语义）。
        if cnt <= 0 {
            return 0;
        }
        let mut total: ssize_t = 0;
        for i in 0..cnt as usize {
            let v = unsafe { &*iov.add(i) };
            if v.len > 0 {
                let r = crate::fs::write_fd(fd, v.base as *const c_void, v.len);
                if r < 0 {
                    return if total > 0 { total } else { -1 };
                }
                total += r;
            }
        }
        return total;
    }
    if cnt <= 0 {
        return 0;
    }
    // 逐 iov 直写（不对齐组装缓冲——std 的 writev 用于拼接，逐段语义等价）
    let mut total: ssize_t = 0;
    for i in 0..cnt as usize {
        let v = unsafe { &*iov.add(i) };
        if v.len > 0 {
            let bytes = unsafe { core::slice::from_raw_parts(v.base as *const u8, v.len) };
            console_write(bytes);
            total += v.len as ssize_t;
        }
    }
    total
}

#[repr(C)]
pub struct Iovec {
    pub base: *mut c_void,
    pub len: size_t,
}

// —— stdin 面（d-3b）：fd 0 → env 块 stdin 端（channel recv）——
// EOF 约定 = **零长消息**（feeder 发完数据压一条 0 字节消息；v1 无对端
// 关闭检测——3-4b 诚实缺口，消息层约定补）。消息粒度 ≠ read 粒度：一次
// read 要得少，残段留 PEND 静态缓冲下次先出。
static mut PEND: [u8; 256] = [0; 256];
static mut PEND_LEN: usize = 0;
static mut PEND_OFF: usize = 0;

#[no_mangle]
pub extern "C" fn read(fd: c_int, buf: *mut c_void, n: size_t) -> ssize_t {
    let r = read_inner(fd, buf, n);
    crate::signal::may_dispatch(); // A7-4 ③b：公共入口投递点（§3.3）
    r
}

fn read_inner(fd: c_int, buf: *mut c_void, n: size_t) -> ssize_t {
    // 别名先翻译回底 fd（fd 0 克隆共享 PEND 残段——与 POSIX dup 共享
    // file description 同义：uu_head 经别名读到的与直读 fd 0 是同一条流）。
    let fd = alias_base(fd).unwrap_or(fd);
    unsafe {
        if fd != 0 {
            // socket 段（A3-2）：分派先于 fs（同 write）。n==0 由 sock_recv
            // 入口收口（POSIX recv 零长读 = 0）。
            if crate::net::sock_valid(fd) {
                return crate::net::sock_recv(fd, buf as *mut u8, n);
            }
            // epoll 段（A4-1）：epoll fd 不可读（Linux 同款 EINVAL——收割
            // 只走 epoll_wait）。
            if crate::net::epoll_valid(fd) {
                crate::set_errno(crate::EINVAL);
                return -1;
            }
            // fd≥3 = fs-backed 读（fs.rs 收口 EBADF/RA 预读）；fd 1/2 在那边
            // 落同一 EBADF 面（不在册）——旧「1/2 只写：读方向诚拒」零漂移。
            return crate::fs::read_fd(fd, buf, n);
        }
        if n == 0 {
            return 0;
        }
        // 残段优先（上一条消息没读完的部分）。
        if PEND_LEN > PEND_OFF {
            let avail = PEND_LEN - PEND_OFF;
            let c = core::cmp::min(n, avail);
            ptr::copy_nonoverlapping(PEND.as_ptr().add(PEND_OFF), buf as *mut u8, c);
            PEND_OFF += c;
            return c as ssize_t;
        }
        let h = STDIN_H;
        if h == 0 {
            return 0; // env 块无 stdin 端 = /dev/null 语义
        }
        let mut msg = [0u8; 256];
        let mut nhd = 0usize;
        let mut deadline = 0u64;
        loop {
            let mut nb = msg.len();
            let s = berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, ptr::null_mut(), &mut nhd);
            if s == 0 {
                if nb == 0 {
                    return 0; // EOF 约定：零长消息
                }
                let c = core::cmp::min(n, nb);
                ptr::copy_nonoverlapping(msg.as_ptr(), buf as *mut u8, c);
                if nb > c {
                    let rest = nb - c; // ≤ 256 = PEND 容量
                    PEND[..rest].copy_from_slice(&msg[c..nb]);
                    PEND_LEN = rest;
                    PEND_OFF = 0;
                }
                return c as ssize_t;
            }
            // 空队：阻塞等 READABLE（10s 帽）。超时 = feeder 断供（协议破缺）
            // → EOF 化返 0：调用方读短 → 断言炸 → 标记缺失兜底（loud）。
            if deadline == 0 {
                let mut now = 0u64;
                if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                    crate::set_errno(crate::EINVAL);
                    return -1;
                }
                deadline = now + 10_000_000_000;
            }
            let mut obs = 0u64;
            if berry_object_wait_one(h, SIGNAL_READABLE, deadline, &mut obs) != 0 {
                return 0;
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn close(fd: c_int) -> c_int {
    // 别名：摘槽即成功（console 是进程级资产——close(1) 不真关同口径）。
    if alias_take(fd).is_some() {
        return 0;
    }
    if fd_ok(fd) {
        return 0; // console handle 是进程级资产，close(1) 不真关
    }
    // socket 段（A3-2）：摘表 + best-effort CLOSE RPC（vnet 侧会话槽
    // 回收——不弃 = 会话表真容量位泄漏；net.rs 收口，恒返 0 面）。
    if crate::net::sock_valid(fd) {
        return crate::net::sock_close(fd);
    }
    // epoll 段（A4-1）：释槽 + interest 全清（net.rs 收口；段内空槽
    // EBADF）。
    if crate::net::epoll_valid(fd) {
        return crate::net::epoll_close(fd);
    }
    // fd≥3 = fs-backed：摘表 + 可写 fd 的 close 即 commit（裁决 2）。
    crate::fs::close_fd(fd)
}

#[no_mangle]
pub extern "C" fn isatty(fd: c_int) -> c_int {
    // 别名先翻译（console 克隆与底 fd 同判定——恒 ENOTTY，v1 诚实面）。
    let fd = alias_base(fd).unwrap_or(fd);
    if !fd_ok(fd) && !crate::fs::fd_valid(fd) && !crate::net::sock_valid(fd) {
        crate::set_errno(crate::EBADF);
        return 0;
    }
    crate::set_errno(crate::ENOTTY);
    0 // v1 诚实面：非 tty（console 同面；fs fd / socket fd 亦非 tty）
}

// —— FILE / stdio ——

static mut STDERR_FILE: FILE = FILE { fd: 2, _pad: 0 };
static mut STDOUT_FILE: FILE = FILE { fd: 1, _pad: 0 };

/// musl 形状：`stderr`/`stdout` 是值为 `FILE *` 的全局变量（boot 落种，
/// 见 `berry_libc_boot`）。
#[no_mangle]
pub static mut stderr: *mut FILE = ptr::null_mut();
#[no_mangle]
pub static mut stdout: *mut FILE = ptr::null_mut();

#[no_mangle]
pub extern "C" fn fileno(f: *mut FILE) -> c_int {
    if f.is_null() {
        return -1;
    }
    unsafe { (*f).fd }
}

#[no_mangle]
pub extern "C" fn fdopen(fd: c_int, _mode: *const c_char) -> *mut FILE {
    if !fd_ok(fd) {
        crate::set_errno(crate::EBADF);
        return ptr::null_mut();
    }
    let p = crate::alloc::malloc(core::mem::size_of::<FILE>());
    if p.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        *(p as *mut FILE) = FILE { fd, _pad: 0 };
    }
    p as *mut FILE
}

#[no_mangle]
pub extern "C" fn fwrite(ptr_: *const c_void, size: size_t, nmemb: size_t, f: *mut FILE) -> size_t {
    if f.is_null() || size == 0 || nmemb == 0 {
        return 0;
    }
    let fd = unsafe { (*f).fd };
    if !fd_ok(fd) {
        crate::set_errno(crate::EBADF);
        return 0;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr_ as *const u8, size * nmemb) };
    console_write(bytes);
    nmemb
}

#[no_mangle]
pub extern "C" fn fflush(_f: *mut FILE) -> c_int {
    0 // 直通写，无缓冲可冲
}

#[no_mangle]
pub extern "C" fn fclose(_f: *mut FILE) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn feof(_f: *mut FILE) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn fread(_p: *mut c_void, _size: size_t, _nmemb: size_t, _f: *mut FILE) -> size_t {
    0
}

#[no_mangle]
pub extern "C" fn fseek(_f: *mut FILE, _off: c_long, _whence: c_int) -> c_int {
    crate::set_errno(crate::ESPIPE);
    -1
}

#[no_mangle]
pub extern "C" fn ftell(_f: *mut FILE) -> c_long {
    crate::set_errno(crate::ESPIPE);
    -1
}

// —— 死亡面（Plan B v0.5：exit 族接真 0x33——EOF 先行 + `berry_process_exit`。
// EOF 在退出**前**补发（d-3c 收端约定不变：收端收齐零长消息即知输出终了，
// 不赌进程死序）；`park` 保留给「协议破缺面」的兜底（argc=0/console=0 类
// boot 破缺——退出路径依赖表内 handle，破缺语境不敢假设可用）——

#[no_mangle]
pub extern "C" fn exit(code: c_int) -> ! {
    send_stdout_eof();
    berry_sys::berry_process_exit(code)
}

#[no_mangle]
pub extern "C" fn _exit(code: c_int) -> ! {
    send_stdout_eof();
    berry_sys::berry_process_exit(code)
}

#[no_mangle]
pub extern "C" fn abort() -> ! {
    // A7-4 ③b：abort 语义 = raise(SIGABRT)——被 catch 则进 handler，
    // handler 返回后仍终止（musl 口径：abort 流程不因 handler 返回而
    // 续命）；IGN/DFL 汇入终止路径（dispatch 内 DFL 终止档自带
    // `abort\n` 诊断 + exit(134) 不返，此后的兜底只罩 Call-返回档）。
    // POSIX/C11 钦定（修复轮补，对抗审查 finding）：raise 前先解除
    // SIGABRT 阻塞——block 态下 raise 只落 held、handler 永不执行；
    // 旧 pending 位随 take_total 归并一并送达。
    crate::signal::abort_unblock_sigabrt();
    let _ = crate::signal::raise(crate::signal::SIGABRT);
    console_write(b"abort\n");
    send_stdout_eof();
    berry_sys::berry_process_exit(134) // 128+SIGABRT 惯例码
}

/// atexit 注册表（32 槽，登记不执行——std 的 flush 在 lang_start 收尾侧，
/// 不经 atexit；留表是为了「谎报成功」的同时不砸未来真调用者）。
const ATEXIT_MAX: usize = 32;
static mut ATEXIT_TAB: [usize; ATEXIT_MAX] = [0; ATEXIT_MAX];
static mut ATEXIT_N: usize = 0;

#[no_mangle]
pub extern "C" fn atexit(f: extern "C" fn()) -> c_int {
    unsafe {
        if ATEXIT_N < ATEXIT_MAX {
            ATEXIT_TAB[ATEXIT_N] = f as usize;
            ATEXIT_N += 1;
            return 0;
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn __assert_fail(
    expr: *const c_char,
    file: *const c_char,
    line: c_int,
    _func: *const c_char,
) -> ! {
    console_write(b"assert failed: ");
    unsafe {
        let e = core::slice::from_raw_parts(expr as *const u8, crate::strmem::strlen(expr));
        console_write(e);
        console_write(b" at ");
        let f = core::slice::from_raw_parts(file as *const u8, crate::strmem::strlen(file));
        console_write(f);
    }
    console_write(b":");
    let mut buf = [0u8; 24];
    let s = crate::vprintf::utoa_dec(line as u64, &mut buf);
    console_write(s);
    console_write(b"\n");
    exit(134) // assert 失败 = abort 同码（glibc 语义）
}
