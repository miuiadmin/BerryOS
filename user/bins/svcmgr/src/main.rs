//! G-eco-b——svcmgr 服务管理器 Rust 化（替换 `kernel/src/user.rs` 的 svcmgr
//! asm 模板，943-1663 行整块退役）。契约与 asm 逐字等价（4-5b + 4-6 A 半
//! 验收链，smoke 第 21/23 标记不许降级）：
//!
//! - B 半：拉起 echo 服务（process_create 双出参 → 2 页 VMO/commit/write
//!   内嵌 echo 镜像 → vmar_map@0x1000 → channel_create → thread_create →
//!   process_start(bootstrap=end1)——close 原值必 -EBADF 迁移自证）→
//!   ECHO_REQ 往返验回显（低 32 位）→ `SVC ok`；
//! - A 半：直线拉起 talker + echo2（同一份 echo 镜像第二实例），交接消息
//!   一次迁移双 handle（console 别名 + t2 对端——能力经消息授权，不经
//!   全局名）→ talker 打 `DEMO ok`；
//! - 稳态：节流 + ECHO_REQ 实证往返 + 'v'（每个 'v' = 一次 ECHO_REQ 往返）；
//!   GUI-8 段② 起 GUI 形（APP_SVC≠0）加挂 app 域常驻 serve（wait_many
//!   双通道：app 请求 + launcher 终结信号——guifbd 先例）+ launcher 监督
//!   最小件（非阻塞探活 + 一次重启帽，裁决四-d）；rootfs/裸跑形原紧环
//!   逐字保留（47/47 零漂红线）。
//!
//! 子程序镜像 `include_bytes!` 内嵌（两遍构建：leaf bins 先产平像再编本
//! crate，见 justfile `build-user`；改子程序须先重跑 pass 1）。入口 x0 =
//! console 发端（boot 首插定值 0x0001_0001）。任何链断 → 自陷被杀 →
//! 标记缺失 = smoke FAIL 兜底。

#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_event_create,
    berry_handle_close, berry_handle_duplicate, berry_object_signal, berry_object_wait_many,
    berry_object_wait_one,
    berry_pager_supply_mapped, berry_pager_supply_mapped_table, berry_pager_supply_pages,
    berry_process_create, berry_process_get_exit_code, berry_process_start,
    berry_process_suspend,
    berry_process_terminate, berry_thread_create, berry_thread_sleep, berry_thread_start,
    berry_vmar_map, berry_vmar_unmap, berry_vmo_commit, berry_vmo_create, berry_vmo_get_size,
    berry_vmo_read, berry_vmo_write, Handle, PagerWindow, BERRY_ERR_ACCESS_DENIED,
    BERRY_ERR_AGAIN, BERRY_ERR_BAD_HANDLE, BERRY_ERR_BAD_STATE, BERRY_ERR_BUSY,
    BERRY_ERR_INVALID_ARG, BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, RIGHT_DUPLICATE, RIGHT_MANAGE,
    SIGNAL_WAKE_PENDING,
    BERRY_PLATFORM_LEVEL,
    RIGHT_READ, RIGHT_RECV, RIGHT_SEND, RIGHT_WRITE, SIGNAL_READABLE, SIGNAL_TERMINATED,
    SIGNAL_WRITABLE, VMO_OPT_DEMAND,
};
// A-2：拉起链换轨 berry-exec（「不长第二套」——spawn_rootfs_elf/channel_pair
// 迁出，bsh 的 run 同链消费；行为逐字等价）。P6 A-1：cpio_find 供 fsmgr
// 预走查（bsh gift[3] 依赖 fsmgr 先起，而 mkcpio 名序 bsh 首位）。
use berry_exec::{
    channel_pair, cpio_find, set_demand_hooks, spawn_rootfs_elf, ChildHandles, EagerGuard,
    SpawnSpec, FB_VA, VIRTIO_MMIO_VA,
};
// P6 A-4：vblk 块设备服务消费自验（BLKD ok）的协议面。
use berry_blkproto::{encode_request, parse_reply, OP_READ, OP_WRITE, SECTOR_SZ, ST_OK};
// P6 A-8c：vnet 网络服务消费自验（DNS ok）的协议面。
use berry_netdproto as ndp;
// app-0 a-2：.bap manifest v0 解析面（纯函数 crate，host 23 测同源）。
// GUI-7：+[ui] 面（ICON_PREFIX/VIEWS_PREFIX/UI_PATH_MAX——路径执法镜像）。
// ADR #82 V1：+AppType（type 词表 {app,driver}——driver 收格式不消费，
// 收集槽过滤执法面见 app_launch）。
use berry_manifest::{
    AppType, APP_SLOTS, BAP_MAX, ENTRY_MAX, ICON_PREFIX, ID_MAX, MANIFEST_MAX, SDK_CURRENT,
    UI_PATH_MAX, VIEWS_PREFIX,
};
// C-3 段①：ps 查询 RPC 协议面（ps_selftest 服务端：请求解析 + 账本编码）。
use berry_psproto as psp;
// GUI-4 段②：GUI wire 协议面（input 域 boot 自验——SUBSCRIBE 请求编码 +
// 1B 状态应答收口，no_std 纯函数 crate）。
use berry_guiproto as gp;
// GUI-8 段②：app 域 RPC 协议面（launcher ↔ svcmgr：APP_LIST/APP_SPAWN
// 编解码——段① 落库，本刀接 serve 消费侧）。
use berry_appproto as appp;
// A-8b：vnet 常驻服务 client 端保活槽（单线程 svcmgr，Relaxed 足够）。
// X-3c-3：pager 寄存表三宽原子（bool/usize/i64——槽 handshake 见 [`PagerReq`]）。
use core::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// echo 服务镜像（G-eco-b 编译产物；echo1/echo2 共用同一份字节——asm 同款）。
const ECHO_BIN: &[u8] = include_bytes!("../../../userbins/echo.bin");
/// talker 镜像。
const TALKER_BIN: &[u8] = include_bytes!("../../../userbins/talker.bin");
/// T-1 §2：thrchild 镜像（进程内双线程自验子——echo/talker 同产链 pass 1
/// 平像内嵌，零 rootfs 新条目 = 裸跑/EFI 路天然同面）。
const THRCHILD_BIN: &[u8] = include_bytes!("../../../userbins/thrchild.bin");
/// G4-2：dpgp 镜像（demand 装载受验子——跨进程供给形 EL0 侧，bootstrap=0
/// 零句柄；同产链 pass 1 平像内嵌，零 rootfs 新条目）。
const DPGP_BIN: &[u8] = include_bytes!("../../../userbins/dpgp.bin");
/// hexyl 终验基准（d-3c）：上游 0.17.0（`user/bins/hexyl` 逐字内嵌）在
/// 宿主以 `--color=never` 管道跑同样 256B 输入的**逐字节输出快照**（版本随
/// Cargo.toml 钉死——升版须重产此文件，比对不等即 FAIL 是故意的）。
const HEXYL_PLAIN: &[u8] = include_bytes!("hexyl-plain.bin");

/// ECHO_REQ（4-5b 验收载荷；回显比对取低 32 位 0xE000_0E30）。
const ECHO_REQ: u64 = 0x00C0_FFEE_E000_0E30;
/// smoke 第 21 标记（7 字节）。
const SVC_OK: &[u8; 7] = b"SVC ok\n";

const TIMEOUT_NS: u64 = 10 * 1_000_000_000;

/// hexyl 终验活动帽（verify_hexyl 专用——与 [`TIMEOUT_NS`]（wait() 链断自陷
/// 面）分立）：X-5 demand 化后 hexyl 工作集（clap/fmt 链 ~150-250 页）走
/// 逐页 fault→arm→0x23→醒往返，TCG 实测 ≈6 页/s（2026-11-08 净窗复跑
/// kick 行 sw 爬 12 圈/2s 定锚）→ 50-90s 级装载期；且 musl stdio 全缓冲
/// （HEXYL_PLAIN 2156B < 4096）= exit 单刷，装载期零包不刷帽——10s 帽
/// 把「慢」误判成「死」（kick dead + no EOF 双跑同脸定谳）。180s = 实测
/// 最坏 ×2 余量；eager 时代 10s 即足（spawn 全幅预拷）。
const HEXYL_EOF_CAP: u64 = 180 * 1_000_000_000;
/// S-6 ③ 临终诊断用 console 柄快照（_start 首行单写；wait() 自陷前一读）
/// ——console 逐层传参到不了 wait()（send_retry/roundtrip 均无此参），全局
/// 快照是最小侵入面。0 = 未初始化（裸测/异常早跑）→ 诊断行跳过。
static CONSOLE_H: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
const THROTTLE: u64 = 0x30_0000;
/// 子进程布局：镜像首页 @0x1000 + 零栈页，映射 [0x1000,0x3000)，sp0=0x3000。
const CHILD_MAP_LEN: usize = 0x2000;
const CHILD_ENTRY: usize = 0x1000;
const CHILD_SP: usize = 0x3000;

// —— X-3c-3 exec demand 装载 pager ——
//
// 内核 boot 直拉的第二裸任务（kernel/src/main.rs X-3c-1）与本任务共享
// ttbr0/asid/句柄表：寄存表即本 .bss 静态，零 IPC。berry-exec 经
// `set_demand_hooks`（_start 首段注入 [`pager_register`]/[`pager_unregister`]）
// 走 congruent→demand 路；bsh 链接副本无钩子恒 eager = 零漂移隔离。

/// 寄存表槽数（32 = 并发在册 demand 子进程帽；满 = reg false → berry-exec
/// eager 回退，不阻塞不等待）。
const PAGER_SLOTS: usize = 32;
/// 全槽空时的小睡帽（供给阻塞收货由内核 0x23 承担——本臂只防空转烧 CPU）。
const PAGER_IDLE_POLL_NS: u64 = 50 * 1_000_000;
/// 活跃轮间让步睡（X-3c 首轮 smoke 教训）：活槽在册即热转供给环，单核
/// 上会把同抢全局武装槽的他供给者（dpg 族 0x21）饿死在 pager 键竞速里
/// ——每轮 1ms 让步把竞速让成公平轮。供给延迟 +1ms/轮，TCG 噪声级。
const PAGER_ACTIVE_YIELD_NS: u64 = 1 * 1_000_000;

/// X-3c-3 pager 让位闸：dpg 自供族（selftest/storm/negative/bench/foot/
/// dpgf）运行期间 boot pager 挂起（零供给调用）。机制：0x21/0x23 供给
/// 共用「全局取货 + 身份验」复用面——双供给者并发时 pager 的 supply 调用
/// 会取到他槽武装请求（rvmo ≠ dv 拒 + 唤 faulter → 重武装 → 再取，热转
/// 环永远先赢键竞速），dpg 供给者永久饿死（首轮 smoke 实炸：DPGP/INPUT/
/// DPGS/DPGN 四标记 420s 连坐缺席）。hold 期间 registry 子供给暂停——
/// 非死锁（demand 缺页本就透明阻塞，闸放即续）；dpg 族皆短窗自包含测试。
static PAGER_HOLD: AtomicBool = AtomicBool::new(false);

/// 让位闸守卫：构造取闸、Drop 放闸（多早退路径免漏放；panic=abort 无
/// unwind 面，Drop 只覆盖正常/早退返回路径）。
struct PagerHold;

impl PagerHold {
    fn new() -> Self {
        PAGER_HOLD.store(true, Ordering::Release);
        PagerHold
    }
}

impl Drop for PagerHold {
    fn drop(&mut self) {
        PAGER_HOLD.store(false, Ordering::Release);
    }
}

/// 一条 demand 装载窗登记（X-5 逐窗化：一子进程 = N 窗 N 槽）：**单写者**
/// （本服务主任务）Relaxed 填三值 → `active` Release 发布；pager 侧
/// `active` Acquire 读后取值（无 CAS 面——写者唯一，空槽并发复用不存在）。
struct PagerReq {
    active: AtomicBool,
    /// 本窗 demand VMO（0 = 不该出现——active 真时恒非 0）。
    dv: AtomicUsize,
    /// 源 VMO（file 窗 = H_ROOTFS——boot 授能 READ 服务生命期不关；零窗
    /// = 0 → 内核 0x23 零页臂）。
    sv: AtomicUsize,
    /// 0x23 恒等供给常数（页对齐 i64，可负——本窗自己的 k_i 偏置；零窗
    /// 恒 0）。
    delta: AtomicI64,
}

/// 槽初值（const 数组Repeat 面用）。
const PAGER_REQ_INIT: PagerReq = PagerReq {
    active: AtomicBool::new(false),
    dv: AtomicUsize::new(0),
    sv: AtomicUsize::new(0),
    delta: AtomicI64::new(0),
};

/// 寄存表（32 槽 .bss 静态——flat 零垫覆盖，boot 即就位；X-5 起 musl 族
/// 一子 ≈ 5 槽（4 file + 1 零尾，hybrid 不占槽）→ 32 槽 ≈ 并发 6 子，满
/// = reg false → berry-exec 逐窗回退 eager，不阻塞不等待）。
static PAGER_REQS: [PagerReq; PAGER_SLOTS] = [PAGER_REQ_INIT; PAGER_SLOTS];

/// pager_main 扫供圈数（X-5 停摆分拣仪）：踢门时前后各快照一次。X-6 锚窗
/// 泊车起判读基线重标：锚序圈 + 池空时 0x23 常驻泊键（醒 = 有臂落地）→
/// sw 爬率大降，词义不变——**爬升仍 = pager 活着、冻结仍 = 用户态楔死**
/// （S2 实测未现；停摆根因词面见 stall_kick 分拣仪注）。停摆签名新形态
/// （post-72e5789 语义，非 X-6 引入）：stall_kick 自身 0x23 在池空时亦泊
/// 键——kick 瞬间池空 + 子死 + 无新 arm 时 kick 任务冻在内核，签名从
/// 「no EOF」变「全标记冻结」（loud fail 可接受，防未来误诊）。Relaxed
/// 纯取证计数，无同步语义。
static PAGER_SWEEPS: AtomicU64 = AtomicU64::new(0);

/// X-6 刀0 吞吐仪器：供给成功累计页数（回退臂 = 0x23 rc==0 逐窗 +1；
/// X-6 刀① 表形主路 = 0x24 rc>0 求和——**供页数口径不变**）；boot 验证成功点恰
/// 三处（hello/hexyl/boot 尾——bench 冻结词形，见 x6_mark）观测行
/// `X6 pg=N sw=M` 的 pg 取值处（sw = 上邻 PAGER_SWEEPS；行不带钟——页/s
/// 的钟 = 宿主侧 marker 到达时戳）。Relaxed 纯取证计数，无同步语义
/// （pager 任务写 / 主任务观测行读）。
static PAGER_SUPPLIED: AtomicU64 = AtomicU64::new(0);

/// berry-exec 注册钩子（X-5 逐窗）：找空槽 Relaxed 填三值 → `active`
/// Release 发布；返 false = 槽满（调用方回滚已注窗走 eager）。
fn pager_register(dv: Handle, sv: Handle, delta: i64) -> bool {
    for slot in PAGER_REQS.iter() {
        if slot.active.load(Ordering::Relaxed) {
            continue;
        }
        slot.dv.store(dv as usize, Ordering::Relaxed);
        slot.sv.store(sv as usize, Ordering::Relaxed);
        slot.delta.store(delta, Ordering::Relaxed);
        slot.active.store(true, Ordering::Release);
        return true;
    }
    false
}

/// berry-exec 注销钩子（逐窗；reap/回滚在 close dv **前**调）：按 dv 值
/// 扫描回收。句柄值在表内存活期唯一 ⇒ 活句柄扫描无假命中；先
/// `active=false`（Release）停供，reap 后续才 close——在途供给（已读旧
/// 值）碰 close 后句柄 = EBADF/EINVAL 单发误差，pager 推进即自愈，无害。
fn pager_unregister(dv: Handle) {
    for slot in PAGER_REQS.iter() {
        if !slot.active.load(Ordering::Acquire) {
            continue;
        }
        if slot.dv.load(Ordering::Relaxed) == dv as usize {
            slot.active.store(false, Ordering::Release);
        }
    }
}

/// pager 任务入口（link-user.ld 钉 `pager_entry` 段@0x1E000、栈顶 0x20000
/// ——kernel/src/main.rs X-3c-1 同值双改）。恒扫供给环：每活槽一供
/// 0x23(dv, sv, delta)——file 窗 = rootfs 恒等供给、零窗 = sv 0 内核零页
/// 臂（X-5 逐窗化；X-3c 的 dvf/dvz 双供对拆成两槽单供）。
/// X-6 锚窗扫圈：锚 = 最近供给命中槽（fn 内局部，boot 首供前无锚）——每圈
/// 锚槽最先、其余按锚+1 环绕原方向续扫；纯排序启发式，槽注销/复用无不变量
/// 要求，走查自愈；泊车全由内核池空 0x23 阻塞承担（本刀零新增用户态睡/
/// 唤醒——与被否决 NoneForVd-睡变体的本质区别：池空 = 无任何等供者，睡
/// 无饿他风险）。
/// 返值入账：rc==0 = 供上 1 页（PAGER_SUPPLIED 计数 + 锚更新本槽）；非 0
/// 忽略推进——EINVAL = 本窗无武装臂（X-5 过滤取刀 72e5789 起 `el0_take_for`
/// 只取属本 dv 的臂——他窗臂在池不烧不扰，-22 = NoneForVd 快跳推进；
/// 刀前旧语义「全局取货错配烧臂」= repro4 定谳的失配消耗舞，已灭）或
/// 陈旧槽（close 后句柄 EBADF），走查即自愈；无请求时 0x23 阻塞收货
/// （内核 EL0 pager 键，u64::MAX 帽 = 空闲 pager 零 CPU），全槽空才小睡。
/// **函数名 ≠ 段名**（dpgi_start/dpgi_entry 同款纪律）：dotless link_section
/// 与 no_mangle 符号同名 = rustc fat-LTO 双定义撞车（首版实踩：changed
/// binding to STB_GLOBAL）；内核按地址直跳（0x1E000），符号名无关紧要，
/// no_mangle 仅作防 DCE 根。
#[link_section = "pager_entry"]
#[no_mangle]
pub extern "C" fn pager_main() -> ! {
    // X-6 锚窗：最近供给命中槽（PAGER_REQS 索引）。Option 因 boot 首供前
    // 无锚；槽注销/复用不要求锚有效（miss 即走查自愈，下一 rc==0 归位）。
    let mut anchor: Option<usize> = None;
    // X-6 刀① boot 探测（一次性，圈外静序）：0x24(dv:0, n:1)——新核在册
    // 门败 -EBADF / 旧核未注册 -ENOTSUP。唯 -EBADF 立表形主路；其余任何
    // 返值（-95 / 异码）保守回退现行 0x23 单窗环（回退臂整保留——
    // stall_kick 钉死 0x23 同依据：kick 任务不得进 0x24 池空泊键面）。
    let probe = PagerWindow { dv: 0, sv: 0, delta: 0 };
    let table_mode = berry_pager_supply_mapped_table(&probe, 1) == BERRY_ERR_BAD_HANDLE;
    loop {
        PAGER_SWEEPS.fetch_add(1, Ordering::Relaxed);
        // 让位闸（X-3c-3）：dpg 自供族在跑 = 本 pager 静默（零供给调用，
        // 不入键竞速）；空转小睡同空闲臂。
        if PAGER_HOLD.load(Ordering::Acquire) {
            let mut t = 0u64;
            if berry_clock_read(CLOCK_MONOTONIC, &mut t) == 0 {
                let _ = berry_thread_sleep(t + PAGER_IDLE_POLL_NS);
            }
            continue;
        }
        let mut any = false;
        // 锚序扫圈（X-6）：圈首快照锚——圈中命中再锚只作下一圈起点（迭代
        // 内活读锚 = 新锚跳位与 i 递增双跳叠加，会跳槽/重复探测）。锚槽
        // 最先，其余按锚+1 环绕原方向续扫；无锚 = start 0，恰今日 0..N 序。
        let start = anchor.unwrap_or(0);
        if table_mode {
            // X-6 刀① 表形主路：圈首快照活槽按锚序纯旋转填表（512B 栈数组
            // ——设计 §2.3 落点），单发 0x24 多窗一次收货。rc 只报供页计数
            // 不报槽位 → 命中集不可知，锚不可精更新（None+rc>0 以圈首活槽
            // 近似「最近供给序」起点，rc==0 圈照旧）；rc<0 忽略推进（-22
            // 快跳 / -9 陈旧槽毒整批——unregister 先于 close 把毒界压到一
            // 圈，下一圈快照自愈；恒扫环口径与回退臂同义）。
            let mut tbl = [PagerWindow { dv: 0, sv: 0, delta: 0 }; PAGER_SLOTS];
            let mut k = 0usize;
            let mut first_active: Option<usize> = None;
            for i in 0..PAGER_SLOTS {
                let idx = (start + i) % PAGER_SLOTS;
                let slot = &PAGER_REQS[idx];
                if !slot.active.load(Ordering::Acquire) {
                    continue;
                }
                if first_active.is_none() {
                    first_active = Some(idx);
                }
                any = true;
                tbl[k] = PagerWindow {
                    dv: slot.dv.load(Ordering::Relaxed) as Handle,
                    sv: slot.sv.load(Ordering::Relaxed) as Handle,
                    delta: slot.delta.load(Ordering::Relaxed),
                };
                k += 1;
            }
            if k > 0 {
                let rc = berry_pager_supply_mapped_table(tbl.as_ptr(), k as u32);
                if rc > 0 {
                    // 供页数口径不变（X-6 刀0 仪器）：表批后按 rc 累加。
                    PAGER_SUPPLIED.fetch_add(rc as u64, Ordering::Relaxed);
                    if anchor.is_none() {
                        anchor = first_active;
                    }
                }
            }
        } else {
            // X-6 刀① 回退臂（现行 0x23 单窗环，整保留——旧核/探测未立形）：
            for i in 0..PAGER_SLOTS {
                let idx = (start + i) % PAGER_SLOTS;
                let slot = &PAGER_REQS[idx];
                if !slot.active.load(Ordering::Acquire) {
                    continue;
                }
                any = true;
                let dv = slot.dv.load(Ordering::Relaxed) as Handle;
                let sv = slot.sv.load(Ordering::Relaxed) as Handle;
                let delta = slot.delta.load(Ordering::Relaxed);
                // X-6 rc 入账：0 = 供上 1 页（吞吐计数 + 锚更新）；非 0 同今日
                // 语义忽略推进（-22 = NoneForVd 快跳 / -9 EBADF = 陈旧槽，自愈）。
                if berry_pager_supply_mapped(dv, sv, delta) == 0 {
                    PAGER_SUPPLIED.fetch_add(1, Ordering::Relaxed);
                    anchor = Some(idx);
                }
            }
        }
        if !any {
            let mut t = 0u64;
            if berry_clock_read(CLOCK_MONOTONIC, &mut t) == 0 {
                let _ = berry_thread_sleep(t + PAGER_IDLE_POLL_NS);
            }
            // clock 失败 = 契约面破（紧扫不睡）；供给正确性不依赖本臂。
        } else {
            // 活跃让步（见 PAGER_ACTIVE_YIELD_NS 注）。
            let mut t = 0u64;
            if berry_clock_read(CLOCK_MONOTONIC, &mut t) == 0 {
                let _ = berry_thread_sleep(t + PAGER_ACTIVE_YIELD_NS);
            }
        }
    }
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(console: Handle) -> ! {
    CONSOLE_H.store(console as usize, core::sync::atomic::Ordering::Relaxed);
    // X-3c-3：demand 装载钩子面开张（pager 任务已由内核 boot 直拉——
    // main.rs X-3c-1；此后本服务 spawn_rootfs_elf 走 congruent→demand
    // 路，bsh 链接副本无钩子恒 eager = exec 面零漂移隔离）。
    set_demand_hooks(pager_register, pager_unregister);
    // C-3 段①：拉起账本开卷——svcmgr 自身行（报告者恒 alive；裸跑也记，
    // 表零消费者无害）。
    ps_reg(b"svcmgr", psp::STATE_ALIVE, 0);
    // ---- B 半：echo 服务 + ECHO_REQ 往返 + `SVC ok` ----
    let (end0, end1) = channel_pair();
    spawn_child(ECHO_BIN, end1);
    ps_reg(b"echo", psp::STATE_ALIVE, 0); // 常驻（稳态 'v' 往返对端）
    let mut buf = [0u8; 8];
    let req = ECHO_REQ.to_le_bytes();
    send_retry(end0, req.as_ptr(), 8);
    roundtrip(end0, &mut buf, &req);
    send_retry(console, SVC_OK.as_ptr(), 7);

    // ---- A 半：talker + echo2，交接双 handle ----
    let (s1, t1) = channel_pair();
    spawn_child(TALKER_BIN, t1);
    ps_reg(b"talker", psp::STATE_ALIVE, 0); // 常驻（交接后持双 handle 等待）
    let (t2, e2) = channel_pair();
    spawn_child(ECHO_BIN, e2);
    ps_reg(b"echo2", psp::STATE_ALIVE, 0); // 常驻（talker 链对端）
    let mut gift = [0 as Handle; 2];
    if berry_handle_duplicate(console, RIGHT_SEND, &mut gift[0]) != 0 {
        trap(); // 先复制再送：send 会 take 走句柄，原值须留给稳态
    }
    gift[1] = t2; // echo2 对端随消息交付 talker
    let payload = 0u64; // 纯载体，talker 不看内容
    if berry_channel_send(s1, payload.to_le_bytes().as_ptr(), 8, gift.as_ptr(), 2) != 0 {
        trap(); // 交接件未入队 = A 半链断
    }

    // ---- G-eco-c/d：rootfs VMO 消费（流式步进 → ELF 泵送 → rootfs 程序）----
    // 裸跑（just run / smoke 相）无 rootfs：get_size -EBADF → 静默跳过，
    // 上述 4-5b/4-6 契约零扰动；带 rootfs（just run-rootfs / smoke rootfs
    // 相）→ hello + stdhello 各自拉起（`ECO ok` 第 24 标记 / 子自打
    // `STD ok`、`STD err ok`——G-eco-d d-1c）。
    run_rootfs_child(console);

    // ---- P6 A-8c：vnet 服务消费自验（`DNS ok addr=127.0.0.1`，rootfs 相
    // 第 18 标记）。此刻 vnet 预拉已久（即便自验未毕，请求入队深度 8，
    // serve() 起来即收）；裸跑 NET_A==0 静默跳过零扰动。----
    dns_selftest(console);

    // ---- T-d：TCP 服务化消费自验（`TCPS ok echo=64`，rootfs 相第 20 标记）
    // ——netdproto 三 op OPEN/ROUNDTRIP/CLOSE 经 vnet 常驻服务走一轮回显
    // （echod 由 recipe 起，与 vnet 自验 `TCP ok` 同一宿主服务）；裸跑
    // NET_A==0 静默跳过零扰动。----
    tcp_selftest(console);

    // ---- C-3 段①：ps 查询 RPC 自验（`PS ok n=`）——此刻常驻全员在位、
    // app 批/stdhello 已收口（账本状态稳定）。裸跑档缺静默跳过零扰动。----
    ps_selftest(console);

    // ---- C-3 段②：berryctl v0 自验（`BC ok`）——ver/ps/mem 三子命令
    // 全跑通（psproto 三 op 服务面 + 渲染行断言）。裸跑档缺静默跳过。----
    bc_selftest(console);

    // ---- T-1 §2：进程内多线程消费面自验（`THR ok`，rootfs 相第 36 标记）
    // ——thrchild 内嵌镜像，bootstrap 自句柄形态（dup(hp, MANAGE|DUPLICATE)
    // 经 process_start x0 递入子表），父侧等 Process TERMINATED + 真码 7。
    // 镜像内嵌零 rootfs 依赖 = 裸跑/EFI 路同打；链内坏面 = eco 降级行 +
    // 标记缺失（smoke FAIL 兜底）；账本不入册（hexyl 同例——PS ok n= 零
    // 漂移）。----
    thr_selftest(console);

    // ---- G4-2：pager 服务首形消费自验（`DPGP ok`，rootfs 相第 47 标记）
    // ——dpgp 内嵌镜像跨进程供给形（本服务即 pager：0x21 阻塞收货给子
    // demand 页），bootstrap=0 零句柄形态。镜像内嵌零 rootfs 依赖 = 裸跑/
    // EFI 路同打；链内坏面 = eco 降级行 + 标记缺失（smoke FAIL 兜底）。----
    dpg_selftest(console);
    input_selftest(console); // GUI-4 段②：boot 期 input 域 SUBSCRIBE 自验（INPUT ok）

    // ---- G4-3 C-lite：缺页风暴自愈 N=4（`DPGS ok n=4`，rootfs 相第 49
    // 标记）——共享三 VMO × 4 子并发首触：K=8 槽池各自认领 + 供给毕定向
    // 唤（B-1 键化后实况——C-lite 时代的单槽盲覆写/广播错醒面已退役，本
    // 测留作键化路径回归车）；4× join 7 within 帽。镜像内嵌零 rootfs 依赖
    // = 裸跑/EFI 路同打；链内坏面 = eco 降级行 + 标记缺失（smoke FAIL
    // 兜底）。----
    dpg_storm(console);

    // ---- G4-3 B 刀：0x21 四早退负序（`DPGN ok n=4`，rootfs 相第 50
    // 标记）——键化后拒绝面单点化：①门（无 SET_POLICY → -EACCES）
    // ②对齐（soff=1 → -EINVAL）③commit（未提交源 → -EBADSTATE + join 9
    // 供给先行实锚）④身份验（异 VMO → -EINVAL + 正调自愈 join 7）。
    // 镜像内嵌零 rootfs 依赖 = 裸跑/EFI 路同打；链内坏面 = eco 降级行 +
    // 标记缺失（smoke FAIL 兜底）。----
    dpgn_selftest(console);

    // ---- G4-3 C-full：缺页 soak 满配 N=15（`DPGF ok n=15` + C-4 回卷
    // `DPGF2 ok n=3`）——16 槽登记恰满 + K=8 溢出首压 + CONC-1 真跨核 +
    // MECH-1 convoy 轮供。**cpio 具名文件 `dpgf.on` 门控**：正装 rootfs/
    // run/smoke/EFI 缺席 = 静默跳过零扰动零新标记（rootfs 相 50 / 主
    // smoke 78 / EFI 36 计数零漂——变体卷 = tools/smoke-dpgsoak.sh 专属）；
    // 链内坏面 = eco 降级行 + 标记缺失（smoke FAIL 兜底）。----
    dpgf_selftest(console);
    // G4-3 D/E 刀（bench.py --dpg 通道）：dpgb.on = D 首触延迟 p50/p99 /
    // dpge.on = E footprint 差分（soakmon pages= 心跳）——cpio 具名门同
    // dpgf.on 族，正装缺席静默零扰动。
    dpg_bench(console);
    dpg_foot(console);
    // G4 X-4：E 面 exec 装载差分（dpg_foot 同形帧、真 exec 消费者版）——
    // `dpgx.on` 门（bench E 臂 rootfs 专递，正装/smoke 缺席整段静默零漂移）。
    dpgx_foot(console);
    // G4 X-5：同形帧、现役 musl 多段族版（hexyl/grep 双件两胎）——`x5.on`
    // 门（bench E2 臂 rootfs 专递；多段 k 表逐窗分解新路的首个现役差分面）。
    x5_foot(console);

    // ---- 稳态：节流 + 实证往返 + 'v'（每个 'v' = 一次 ECHO_REQ 往返）+
    // GUI-8 段② 双插面（裁决二/四-d，guifbd wait_many 多路先例）：app 域
    // serve 泵（APP_SVC READABLE——命中帧消化毕续泵至 -EAGAIN 静默）+
    // launcher 监督探活 + SPAWN 活子探活/drain（app_supervise，M-C 异步
    // 收口的稳态兑现点）。三路皆空（APP_SVC==0 且无 launcher 子且子表空
    // ——rootfs/裸跑/重启帽尽形）= **保现状纯 echo 紧环**（50 标记零漂，
    // n=1 字面落实）；GUI 形才并等（n≤2+2×APP_SPAWN_LIVE_MAX=10：svc
    // READABLE + launcher hp TERMINATED + 每活子 out_s READABLE（drain
    // 唤醒面——EOF 后停挂）+ hp TERMINATED）——deadline =
    // now+APP_IDLE_CAP_NS（guifbd min(vb_next, cap) 退化形：节流是忙 spin
    // 非时点，无可对齐的下轮点）；到点 = 合法空闲重臂（真断链面 recv 错码
    // 照 trap——app_serve_one/app_supervise 内执法）。----
    let v = [b'v'];
    loop {
        throttle();
        send_retry(end0, req.as_ptr(), 8);
        roundtrip(end0, &mut buf, &req);
        send_retry(console, v.as_ptr(), 1);
        // poll-first：先收干再并等（guifbd 三路同序）；SPAWN 帧异步形
        // （M-C）：拉起即 ACK、收口尾入 APP_CHILDREN——app_supervise 探
        // 活兑现，echo/监督零长停。
        while app_serve_one(console) {}
        launcher_supervise(console);
        app_supervise(console);
        // AM-2e：压力沿观测桥（机制→消费第一段真线；真梯度策略挂 v2 am，
        // 本刀只落消费沿观测，恢复沿不做）。载形 = 周期自检点 deadline=0
        // poll 探针（提案候选①消费原语 `object_wait_one` 的退化形），不进
        // 下方 wait_many 面——GUI-8 段② 多插面的槽序/分派/n==0「三路皆空
        // 保纯 echo 紧环」语义零触碰。门 = PRESSURE_ARMED（boot 装载断言
        // 臂起，仅 GOP+rootfs 相）：rootfs/裸跑相零调用零输出零行为漂移；
        // GUI 相无 memhog 注入内核恒不跨阈置位 → 现役 smoke 面恒静默
        // （观测行只在真压力沿出现——静默性论证）。位为粘滞边沿（内核
        // 跨阈置位、消费清位，ktest AM2B-K 钉翻卷语义）⇒ poll 不丢沿，
        // 观测迟延 ≤ 一圈（空闲并等帽 10s 内）。命中 1<<40（PRESSURE_P1
        // ——partial；1<<41 预留不清）→ eco 观测行 → 自清位（object_signal
        // clear 面 = am 边沿协议消费动作）。观测行走 eco_note 族（非标记
        // 诊断族）：'pressure p1' 11B ≤ 18B 截断窗、整行 17B ≤ 24B 行窗
        // ——长度由 why.len() 现算非手排定长（§A145 雷结构性规避）。
        // poll 非 0 非 -EAGAIN = 表/右漂移链断 → trap（A-3 口径；deadline=0
        // 恒无超时面，-EAGAIN = 空闲心跳）。
        if PRESSURE_ARMED.load(Ordering::Relaxed) != 0 {
            let mut pobs = 0u64;
            let ps = berry_object_wait_one(H_PRESSURE, PRESSURE_P1, 0, &mut pobs);
            if ps == 0 {
                eco_note(console, b"pressure p1");
                if berry_object_signal(H_PRESSURE, PRESSURE_P1, 0) != 0 {
                    trap(); // 清位面链断（boot 探针后才可能——表/右漂移）
                }
            } else if ps != BERRY_ERR_AGAIN {
                trap();
            }
        }
        let svc = APP_SVC.load(Ordering::Relaxed);
        // SAFETY: LAUNCHER_CHILD 单线程独占（boot arm / 本 loop 不交叠）。
        let lhp = match unsafe { &*core::ptr::addr_of!(LAUNCHER_CHILD) } {
            Some(c) => c.hp,
            None => 0,
        };
        // SAFETY: APP_CHILDREN 只读共享面（app_supervise 独占借用已释）。
        let children: &[Option<AppSpawnTail>] = unsafe {
            core::slice::from_raw_parts(
                core::ptr::addr_of!(APP_CHILDREN) as *const Option<AppSpawnTail>,
                APP_SPAWN_LIVE_MAX,
            )
        };
        let mut hs = [0 as Handle; 2 + 2 * APP_SPAWN_LIVE_MAX];
        let mut sigs = [0u64; 2 + 2 * APP_SPAWN_LIVE_MAX];
        let mut n = 0usize;
        if svc != 0 {
            hs[n] = svc;
            sigs[n] = SIGNAL_READABLE;
            n += 1;
        }
        if lhp != 0 {
            hs[n] = lhp;
            sigs[n] = SIGNAL_TERMINATED;
            n += 1;
        }
        for c in children.iter() {
            if let Some(t) = c {
                if t.out_s != 0 && !t.eof_seen {
                    hs[n] = t.out_s;
                    sigs[n] = SIGNAL_READABLE;
                    n += 1;
                }
                hs[n] = t.child.hp;
                sigs[n] = SIGNAL_TERMINATED;
                n += 1;
            }
        }
        if n != 0 {
            let mut idx = 0usize;
            let mut obs = 0u64;
            let s = berry_object_wait_many(
                hs.as_ptr(),
                sigs.as_ptr(),
                n,
                now() + APP_IDLE_CAP_NS,
                &mut idx,
                &mut obs,
            );
            if s != 0 && s != BERRY_ERR_TIMEDOUT {
                trap(); // 到点 = 空闲重臂；余错 = 死亡面（guifbd 同口径）
            }
        }
    }
}

/// 建通道对 = `berry_exec::channel_pair`（A-2 迁出；两端均授
/// SEND|RECV|DUPLICATE，返回 (本端, 交接端)）。

/// 拉起一个子进程：process_create 双出参 → 2 页 VMO（镜像首页 + 零栈页）
/// → vmar_map@0x1000（rights 恰 READ|WRITE）→ thread_create →
/// process_start(entry=0x1000, sp=0x3000, bootstrap)——bootstrap 随 start
/// 迁入子表（子入口 x0 = 子表新值），close 原值必 -EBADF（迁移自证）。
fn spawn_child(image: &[u8], bootstrap: Handle) {
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    if berry_process_create(0, 0, &mut hp, &mut hv) != 0 || hp == 0 || hv == 0 {
        trap();
    }
    let mut vmo = 0 as Handle;
    if berry_vmo_create(CHILD_MAP_LEN as u64, 0, &mut vmo) != 0 || vmo == 0 {
        trap();
    }
    if berry_vmo_commit(vmo, 0, CHILD_MAP_LEN as u64) != 0 {
        trap();
    }
    if berry_vmo_write(vmo, image.as_ptr(), 0, image.len()) != 0 {
        trap();
    }
    let mut vaddr: usize = CHILD_ENTRY; // 映射基址提示（in-out 槽预写）
    if berry_vmar_map(hv, 0, vmo, 0, &mut vaddr, CHILD_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0 {
        trap();
    }
    let mut ht = 0 as Handle;
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        trap();
    }
    if berry_process_start(hp, ht, CHILD_ENTRY, CHILD_SP, bootstrap) != 0 {
        trap();
    }
    if berry_handle_close(bootstrap) != BERRY_ERR_BAD_HANDLE {
        trap(); // 迁移自证：原值必须已作废
    }
}

// ---- G-eco-c/d：rootfs 消费（流式步进 + 段泵送——d-1b 纯函数面的消费端） ----

/// rootfs VMO 定值 = boot 第 2 插（main.rs `assert_eq!` 同源硬编——
/// console endU 首插 0x0001_0001 之后）。rights 仅 READ（vmo_read/get_size 面）。
const H_ROOTFS: Handle = 0x0001_0002;

/// bsh stdin 端（endB2）= boot 第 3 插定值（console 首插 / rootfs VMO 第 2
/// 插之后；main.rs `assert_eq` 同源硬编）。**仅 rootfs 相有效**——裸跑
/// rootfs 槽缺席顺延为 0x0001_0002，但本 arm 在无 rootfs 时早退永不走
/// （run_rootfs_child 开头 get_size 即返）。rights RECV 随 env 块迁移原样
/// 进 bsh 表（fd0）。A-2。
const H_BSH_STDIN: Handle = 0x0001_0003;

/// MMIO 窗口 VMO（virtio-mmio 整窗 32 槽 = 4 页）= boot 第 4 插定值（P6
/// A-2，rootfs_present 门内——main.rs `assert_eq!` 同源硬编；console 首插
/// / rootfs VMO 第 2 / endB2 第 3 之后）。rights R|W|MAP（vmar_map 的 vmo
/// 侧门 = MAP|READ|WRITE 恰集）。**map 不 take**——本表恒持原值（子进程
/// 只拿映射不拿 handle；子退出 reap unmap 后 mapcnt 归零，本表不关 = v1
/// 常驻泄漏政策，防销毁性 close -EBUSY 面纠缠）。A-2。
const H_MMIO: Handle = 0x0001_0004;

/// fb 窗口 VMO（GOP framebuffer → MMIO 映射窗）= boot 第 5 插定值（GUI-1；
/// **EFI-GOP 路才有**——`-kernel` 路零插表，探测即降级）。rights R|W|MAP
/// （H_MMIO 同集）；**map 不 take**——本表恒持原值（H_MMIO 同款 v1 常驻
/// 泄漏政策）。guifbd 预拉段探测门 = `vmo_get_size` 失败（-EBADF）= 非
/// EFI 路 → 一行降级不 spawn。
const H_FB: Handle = 0x0001_0005;

/// fb 几何 info VMO（五域 u64 LE 40B：fb_pa/w/h/stride/fmt）= boot 第 6
/// 插定值（GUI-1；同路门控）。rights 仅 READ（vmo_read 面）。随 env 块
/// extras[0] **take 迁入** guifbd 表（libc `gift2_h()`——rootfs_h 同槽
/// 别名惯例；用完即弃，svcmgr 表不再留）。
const H_FBINFO: Handle = 0x0001_0006;

/// console fan-out 上屏端 endB_fb = boot 第 7 插定值（GUI-1 段②；同路
/// 门控——fb VMO 5/info 6 之后）。rights 仅 RECV（guifbd 只收不回发 = 无
/// 回环风暴）。随 env 块 extras[1] **take 迁入** guifbd 表（libc
/// `gift3_h()`——blk_h 同槽别名惯例）；console 侧对端持 SEND best-effort
/// 转发（满丢弃计数，见内核 console 任务体）。
const H_FBPIPE: Handle = 0x0001_0007;

/// AM-2b 系统压力 Event = boot 第 8 插定值（endB_fb 7 之后；内核在 GOP
/// 相位内 dup 进本表——非 GOP 相槽缺席 -EBADF，fb/virtio 惯例）。rights
/// SIGNAL|DUPLICATE（`event_create` 出生授同口径；SIGNAL = 消费侧
/// `object_signal` 自清位面，DUPLICATE = 迁交侧再 dup 面）。恒持原值不
/// close（留表即留命，H_FB 同款 v1 政策）。**AM-2e 起最小消费**：稳态
/// loop 周期自检点 deadline=0 poll 观测 1<<40 沿 → eco 观测行 + 自清位
/// （边沿协议消费沿；恢复沿/真梯度策略挂 v2 am——am 成形经 env 块
/// extras take 迁交，契约 = `.agents/plans/am/design.md` v0.4.0
/// 「压力信号」节：自清位 + 窗口长 deadline 再等）。
const H_PRESSURE: Handle = 0x0001_0008;

/// 压力 Event 的 partial 跨阈用户位（内核 70ms 窗 θ_p 10% 停顿占比判据
/// 跨阈置位——提案候选①协议定值）。1<<41 complete 跨阈位**预留——登记
/// 不置位也不清**（full 判据需 waiter 深度跟踪，挂实测定标后再裁）。
/// 高 32 位用户位面（`object_signal` 只收高 32 位——低 32 位内核派生
/// 保留）。
const PRESSURE_P1: u64 = 1 << 40;

/// AM-2e 观测门：boot 装载断言探针过（GOP+rootfs 相——H_PRESSURE 在席
/// 且持 SIGNAL 右）臂起 1；rootfs/裸跑相装载块整段不执行 → 恒 0 = 稳态
/// poll 门恒闭（零调用零输出零行为漂移）。NET_A 族语义：0 = 该相无此
/// 源，静默跳过。
static PRESSURE_ARMED: AtomicU32 = AtomicU32::new(0);

/// VER 版本身份 VMO = boot 第 9 插定值（第 8 插 H_PRESSURE 之后；内核 fb
/// GOP 相位内 sysver VMO 进本表——非 GOP 相槽缺席 -EBADF，fb/virtio 惯例）。
/// rights 仅 READ（版本面禁写——vmo_write 放宽面拒；权限在 handle 不在
/// 对象，内核侧 boot 即写即交不留柄）。恒持原值不 close（H_FB 同款 v1
/// 常驻政策）。ver-hard 起对拍消费：[`ver_check`]（fb 相门内，kernel
/// sysver VMO × rootfs .berryver 烙印三元组对拍——eco: VER ok/mix
/// 观测行，非标记族）。
const H_VER: Handle = 0x0001_0009;

/// sim-S1 M2-b：boot 手搭 sim 礼物对之**帧通道 RECV 端**第 5 插定值
/// （`-kernel`+rootfs+fw_cfg `opt/berry/simdev` 门开相——task_simcon 转发
/// 的 UART1 字节流对端）。**与 [`H_FB`] 同值异相**：fw_cfg 具名键在 EFI 路
/// 不可见 → 门恒闭 → 两相结构性互斥，boot 侧 assert 绊网（并存即 boot 期
/// 炸响，设计 v1.1 §4.4）。随 env 块 `stdin_handle` **take 迁入**
/// simbattery 表（fd0，libc `stdin_h()`）。
const H_SIM_RX: Handle = 0x0001_0005;

/// sim 礼物对之 **ACK 通道 SEND 端**第 6 插定值（同上门相；与
/// [`H_FBINFO`] 同值异相互斥）。simbattery 逐帧回 9B ACK 的对端；随 env
/// 块 `extra_gifts[0]` **take 迁入**（libc `sim_ack_h()`——gift[0] 同槽
/// 别名惯例）。宿主注入 client 经 serial1 读回 ACK 对拍（§7）。
const H_SIM_ACK: Handle = 0x0001_0006;

/// cpio 步进窗：头 110B + 名（mkcpio 名 ≤ 56B 绰绰有余，盖不住 = 诚实拒）。
/// （ELF_WIN/PUMP_WIN/ELF_MAP_MAX 随 spawn_rootfs_elf 迁 berry-exec——A-2。）
static mut STEP_WIN: [u8; 512] = [0; 512];
/// hexyl 终验收齐窗：子 stdout 全量（基准 2156B + 版本漂移余量）。
static mut HEX_WIN: [u8; 4096] = [0; 4096];
/// C-2a U1 收集窗（uu 子私有 stdout；ls 两条目 + 上游格式余量，2KiB）。
static mut U1_WIN: [u8; 2048] = [0; 2048];
/// vnet（常驻网络服务）client 端（A-8b）：vnet 预拉 Ok 后本表留持的
/// net_a——A-8c 起作 DNS 自验的 client 端消费；本步只保活（唯一 client
/// 端 close = 服务端孤儿端，v1 无对端关闭检测，留表即留命）。
static NET_A: AtomicU32 = AtomicU32::new(0);
/// GUI-4 段②：input 域 channel 客户端端（预拉成功后非 0；boot 自验 +
/// bsh 面 dup 分种——input_h()）。0 = inputd 未起，静默降级。
static INPUT_A: AtomicU32 = AtomicU32::new(0);
/// GUI-8 段②：数据盘在位旗（vblk 预拉自验 Ok 路 store 1；0 = 无 virtio-blk
/// 数据盘/自验拒——EFI 冻结相 usb-storage 非本域即拒）。**旗非端点**
/// （NET_A/INPUT_A 皆句柄，此为布尔语义）——R1 判别器 v2 源：fs 让位 ⟺
/// 布局卷在位（fsmgr 挂同一 blk 盘，两信号同源同真假）。INPUT_A 判别实证
/// 废案：inputd 预拉只门 H_MMIO（virt 恒在）且 spawn Ok 即 store（tablet
/// 探测在 inputd 进程内、晚于 store，park 不回清）→ EFI 冻结相恒真误让位
/// （实测 bsh fs_h()==0 park，11 标记连坐）。
static BLK_OK: AtomicU32 = AtomicU32::new(0);
/// GUI-5：wmd↔inputd 推送对 inputd 端（wmd arm Ok 路存 push_cli；inputd
/// arm 读取追加 extras——0 = wmd 未起，inputd extras 仍单枚现行为逐字节
/// 保留）。**留表即留命**（NET_A 同形）：EFI 相 inputd 不拉起（H_MMIO 门
/// 先跳）→ 端恒滞 static = wmd push 服务端端的对端保活，属覆盖面非泄漏
/// （v1 无对端关闭检测）。
static WMD_PUSH_B: AtomicU32 = AtomicU32::new(0);
/// GUI-8 前置 wiring 刀（D2）→ 段② 升格常驻源：wm 客户端端 dup 源——
/// wmd arm Ok 路 dup 出 SEND|RECV 留表，launcher 装配（boot arm / 监督
/// 重启）每次从它**现 dup 新别名**作 gift（NET_A 常驻源先例：本体永留
/// 表、dup 生命周期独立——原柄 w_cli 由 gfxdemo arm 直递 take 离表且其
/// Err/cpio 缺路关原柄不置 0（死存弃判例），本 static 是 launcher 侧
/// 唯一可再生 wm 端）。**留表即留命**（NET_A/WMD_PUSH_B 同形）：留表期
/// 内它是 wmd 服务端端的活跃对端（v1 无对端关闭检测，覆盖面非泄漏）；
/// launcher 崩溃后其别名随子表销毁，本源在 = wm 客户端端点存活，监督
/// 一次重启（裁决四-d）可再 dup——重启可行性的结构前提（段② 现场裁：
/// 若首次赠出即耗尽，重启形退化为无 wm 空壳，裁决 E「监督重启」名存
/// 实亡）。唯一清零出口 = gfxdemo reap 挂死角（滞留原柄与 dup 同端点，
/// 续走 = 双读者应答互截违 D3——关门即装配门永闭，正确降级）。0 = wmd
/// 未起，launcher 装配静默跳过（rootfs 形恒 0）。
static WMD_CLI_DUP: AtomicU32 = AtomicU32::new(0);

// ---- GUI-8 段②：app 域常驻 serve + launcher 监督（裁决二/四-d）----

/// app 域服务端端：launcher 装配门内 `channel_pair()` 的本侧（稳态 loop
/// 挂第二通道 serve）；对端随 extras[2] take 迁入 launcher（libc
/// `app_cli_h()` 同槽惯例）。0 = app 域不在（rootfs/裸跑形门闭、或 fs
/// 缺席前缀截断降级）——稳态 loop 据此分形（n=1 紧环零漂）。launcher
/// 崩溃收口时置 0 + close（对端已死，serve 面撤）。
static APP_SVC: AtomicU32 = AtomicU32::new(0);

/// GUI 形 fs 常驻别名：launcher 装配 Ok 时存 fsmgr 预拉对的 fs_a_h（NET_A
/// 先例——**不随 bsh 关闭**）。两面消费：app 域 serve 的 fs 授能源
/// （`app_launch` 的 fs_a 参数——GUI spawn 的 fs 词 app 走 dup 授能，
/// 裁决四-b「授出端 dup 子自持 + 父收口后再用」）+ 监督重启的 fs gift
/// 现 dup 源。0 = rootfs 形（bsh Ok 路照旧关 fs_a_h，零漂）。
static FS_A: AtomicU32 = AtomicU32::new(0);

/// launcher 装配成立标志（Ok 置 1；崩溃收口置 0）——bsh arm 的 fs 让位
/// 路由判别器（裁决四-b：GUI 形 fs 端让位 launcher、bsh 退三元皮）+
/// 走查尾兜底/Ok 关口 fs_a_h 的让位门。与 FS_A 分立：FS_A 是端点真值
/// （可为 0 的降级形），本标志是相判别器。
static LAUNCHER_LIVE: AtomicU32 = AtomicU32::new(0);

/// 监督子柄（裁决四-d）：launcher spawn Ok 留存（`reap` 全链柄集），
/// 稳态 loop 非阻塞探活用；None = 无 launcher（rootfs 形/已收口）。
/// SAFETY: svcmgr 单线程独占（PS_TAB 同纪律）。
static mut LAUNCHER_CHILD: Option<ChildHandles> = None;

/// 一次重启帽（裁决四-d「一次重启」字面执法）：swap 惯取 1 = 帽已用。
/// 帽后再生崩溃 = eco 行 loud 终报不再试（循环重启风暴比不重启更伤稳态
/// ——kill criteria 5 的对面）。
static LAUNCHER_RESTARTED: AtomicU32 = AtomicU32::new(0);

/// SPAWN RPC 活子表帽（M-C）：GUI app 分钟级活期 × 4 槽 = 同时活 app 上
/// 限；满表拒新不拒旧（洪泛面：launcher 端重复点击防抖归 launcher，本帽
/// 兜服务侧结构性上限——防 APP_SPAWN 洪泛撑爆监督面）。帽值 = 桌面 4 页
/// × 每页 1 点击并发面的宽余量（v1 demo 拓扑实活 ≤2）。
const APP_SPAWN_LIVE_MAX: usize = 4;

/// SPAWN 收口尾子表（M-C γ-分·异步侧）：serve 期插入、稳态 `app_supervise`
/// 探活兑现后槽位回 None。`[const { None }; N]` 内联常量块 = 非 Copy 元素
/// 重复表达式的稳定形。SAFETY: 同 LAUNCHER_CHILD 单线程独占。
static mut APP_CHILDREN: [Option<AppSpawnTail>; APP_SPAWN_LIVE_MAX] =
    [const { None }; APP_SPAWN_LIVE_MAX];

/// 稳态空闲并等帽（guifbd `IDLE_CAP_NS` 同值同义——合法空闲非死线，到点
/// 重臂续等；真断链面 = recv 非 -EAGAIN 错码照 trap）。
const APP_IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// app 域 LIST 的 icon 声明长帽 = 64KiB（服务侧现场裁：BYI1 RLE 理论
/// 最坏全不可压 ~384KiB 不设整档帽——64KiB 覆盖现实 app 图标（65536px 帽
/// 下典型 RLE 产物 ≪ 4KiB）+ 大余量；越帽条目降级 blob_len=0（launcher
/// 回落默认图标，条目仍在册——id/label 面完整），v2 资产管道再议）。
/// 单页墙修复后纯声明值执法（零整读窗——blob 体经 ChunkStream 逐片直
/// 读，不再驻 .bss 静态窗）。
const APP_ICON_BLOB_MAX: usize = 65536;

/// 走查记账条目数的稳态镜像（`run_rootfs_child` 局部 app_n 持久面——
/// APP_LIST/APP_SPAWN 的数据源计数；walk 毕落值）。SAFETY: 同上单线程。
static mut APP_ACCOUNT_N: usize = 0;

// ---- app-0 a-2：.bap 消费面（appd 雏形——ADR #43/#44：应用 = 纯用户态
// 协议约定，svcmgr 兼任 appd；内核零「应用」概念）----

/// 走查收集的 app 槽：`app/<ID>.bap` 条目的包体几何（id = 文件名去
/// `app/` 前缀与 `.bap` 后缀）。主走查只收集不拉起——启动统一在走查毕
/// （STEP_WIN 闲置后复用作 .bap 内层 cpio 步进），槽序 = mkcpio 名序 =
/// 确定性启动序。
struct AppSlot {
    id: [u8; ID_MAX],
    id_len: usize,
    bap_off: u64,
    bap_len: u64,
}

const APP_SLOT_ZERO: AppSlot =
    AppSlot { id: [0; ID_MAX], id_len: 0, bap_off: 0, bap_len: 0 };

/// app 槽表（APP_SLOTS 帽）：超帽条目 `eco: app too many` 可见降级——
/// v1 单 boot 批 4 app 绰绰有余，扩容随 appd 正式化（v2）。
static mut APP_SLOTS_TBL: [AppSlot; APP_SLOTS] = [APP_SLOT_ZERO; APP_SLOTS];

/// manifest 读窗（MANIFEST_MAX）：.bss 静态——1KiB 级缓冲不进栈
/// （STEP_WIN/HEX_WIN 同款纪律；svcmgr 用户栈薄）。
static mut APP_MF_WIN: [u8; MANIFEST_MAX] = [0; MANIFEST_MAX];

// ---- GUI-7：[ui] 声明条目记账（appd 枚举面）----

/// 包内 ui 条目记账（icons/<stem>.byi / views/<名>——bap_scan 收集，
/// ui_face 按声明查表；名帽 UI_PATH_MAX=32，条目帽 UI_ENTRIES=4 双侧
/// 镜像 mkapp ICON_FILES_MAX/VIEWS_FILES_MAX）。
struct UiEntry {
    name: [u8; UI_PATH_MAX],
    name_len: usize,
    off: u64,
    len: u64,
}

const UI_ENTRY_ZERO: UiEntry =
    UiEntry { name: [0; UI_PATH_MAX], name_len: 0, off: 0, len: 0 };

/// icons//views/ 各自条目帽（mkapp 同值镜像——双侧同步义务见 mkapp 头注）。
const UI_ENTRIES: usize = 4;

static mut APP_ICON_WIN: [UiEntry; UI_ENTRIES] = [UI_ENTRY_ZERO; UI_ENTRIES];
static mut APP_VIEWS_WIN: [UiEntry; UI_ENTRIES] = [UI_ENTRY_ZERO; UI_ENTRIES];

/// BYI1 头产面帽（mkapp ICON_DIM_MAX/ICON_PIXELS_MAX 镜像——decode 面
/// 无单边帽，产面收紧；本侧只验头不展开 RLE）。
const APP_ICON_DIM_MAX: u16 = 1024;
const APP_ICON_PIXELS_MAX: u32 = 65536;

// ---- app-1 段 b-1：墓碑表（v1.2 §4——RAM 8 槽 {id, svmo, epoch}，三字段
// 对账立法原文：id↔app_id、svmo↔blob、epoch↔时间戳〔全局自增记入序——无
// 墙钟实现，仅 LRU 比大小，不兼作 boot 世代〕。跨 boot 复活挂 v2（表驻
// RAM = reboot 全墓碑——范围盒子）。id 定长 24B 零垫全比（v0 id 恒
// "APPTOMB" 7B）。SAFETY: boot 单线程独占（tomb_selftest 唯一写者），
// 顺序读写——APP_SLOTS_TBL 同款口径。----

/// 墓碑槽（§4：svmo = state VMO 本体 handle——spawn 迁走的是 dup，reap
/// 后本体入表；0 = 空槽）。
struct TombSlot {
    id: [u8; 24],
    svmo: Handle,
    epoch: u64,
}

const TOMB_SLOT_ZERO: TombSlot = TombSlot { id: [0; 24], svmo: 0, epoch: 0 };

/// 表帽（§7：v0 定值 8；真策略配额 v2）。
static mut TOMB_TBL: [TombSlot; 8] = [TOMB_SLOT_ZERO; 8];

/// 全局记入序号（LRU 比大小唯一用途）。
static mut TOMB_EPOCH: u64 = 0;

/// rootfs 全链（G-eco-d d-1c 起流式）：get_size（裸跑 -EBADF 静默跳过）→
/// `cpio_step` 步进遍历（头+名进窗、数据按偏移算术跳过——**归档永不整档
/// 驻留**，旧 16KiB 整档帽随 std 镜像 ~283KB 一并退役）→ 命中 `bin/hello`
/// / `bin/stdhello`/`bin/hexyl` 各自流式拉起。hello 后打 `ECO ok`（smoke
/// 第 24 标记）；stdhello 自打 `STD ok`/`STD err ok`（d-1c 接线）+ **Plan B
/// v0.5：main 返回真退出（0x33）→ svcmgr reap 全回收后打 `EXIT ok`（rootfs
/// 相第 9 标记）**；hexyl 走私有 stdout 终验后打 `HEXYL ok`（d-3c，见
/// `verify_hexyl`）+ best-effort reap。hello（10s 周期睡眠）/bsh（交互）
/// 常驻不回收（句柄留存 = 有界泄漏，v1 政策）。
/// 有 rootfs 而链断 = 打一行原因后跳过（可见降级，不静默吞）。
fn run_rootfs_child(console: Handle) {
    let mut cap = 0u64;
    if berry_vmo_get_size(H_ROOTFS, &mut cap) != 0 {
        return; // 裸跑：vmo=none → -EBADF，零扰动
    }
    let eco_skip = |why: &[u8]| {
        // 诊断行超长安全截断（no_std panic=halt，宁截断不炸机）——本闭包
        // 存在 >18B 调用点（`stdhello not pre-pulled`=23B），无帽即越界。
        let m = why.len().min(18);
        let mut line = [0u8; 24];
        line[..4].copy_from_slice(b"eco:");
        line[4] = b' ';
        let n = 5 + m;
        line[5..n].copy_from_slice(&why[..m]);
        line[n] = b'\n';
        send_retry(console, line.as_ptr(), n + 1);
    };
    // SAFETY: boot 后本进程独占步进窗；顺序写/读，单线程（G-eco-c 同款口径）。
    let step: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(STEP_WIN) as *mut u8, 512)
    };
    // SAFETY: app 槽表/manifest 窗同款——本进程独占、单线程顺序读写
    // （走查收集 + 启动两段共用；MANIFEST_MAX 同窗长）。
    let apps: &mut [AppSlot] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(APP_SLOTS_TBL) as *mut AppSlot,
            APP_SLOTS,
        )
    };
    let mf: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(APP_MF_WIN) as *mut u8,
            MANIFEST_MAX,
        )
    };
    // SAFETY: GUI-7 ui 条目双窗同款——本进程独占、单线程顺序读写
    // （bap_scan 收集 + ui_face 查表两段共用）。
    let icons: &mut [UiEntry] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(APP_ICON_WIN) as *mut UiEntry,
            UI_ENTRIES,
        )
    };
    let views: &mut [UiEntry] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(APP_VIEWS_WIN) as *mut UiEntry,
            UI_ENTRIES,
        )
    };
    let mut app_n = 0usize;
    let mut pos = 0u64;
    // SDK-1a §1.2：bsh 几何记账（延后拉起重排）。None = 归档无 bin/bsh
    // （走查尾兜底关 fs 端点按原序执行）；Some((data_off, data_len)) = 主
    // 走查只记几何，spawn 挪到走查毕 app 批之后（见尾段）——app 批的 fs
    // 授能要 fs_a_h 仍持，而原 arm 内「spawn Ok 后关 fs_a_h」把 bsh 定为
    // fs_a 队列唯一读者（防回复混流），app 批必须排它前面才拿得到安全
    // 端点（fsproto 严格串行无请求号，双读者 = 应答偷取——C-1 刀 3
    // stdhello 预拉同款机制性论证）。重复 `bin/bsh` 条目防重入 =
    // is_some() 判重（H_BSH_STDIN 单用 handle 的死值自陷面由「只记几何
    // 不 spawn」根除；重复条目 = 可见降级，较旧双 spawn 是小收紧）。
    let mut bsh_geom: Option<(u64, u64)> = None;
    // ---- P6 A-5：vblk（常驻块设备服务）预拉起——先于 fsmgr ----
    // A-4 时 vblk 在主走查 arm 拉起（mkcpio 名序靠后，仅 svcmgr 自验消费）；
    // A-5 起 fsmgr 的 FAT 层要 blk 服务端端（env gift[3]），而 fsmgr 预拉在
    // 主走查前 ⇒ vblk 必须更早。迁出主走查（arm 退役，条目自然跳过）。
    // 拓扑：channel_pair 得 (blk_a, blk_b)——blk_b 随 env 块 gift[2] 迁入
    // vblk（libc `gift2_h()`，每进程 env 块独立）；本表暂持 blk_a，自验门
    // （blk_selftest——BLKD ok 第 14 标记）过了才作为 `blk_cli` 交给下方
    // fsmgr 预拉段（take 迁移进 fsmgr 表 = env gift[3]，libc `blk_h()`）
    // ——此后 fsmgr 是 vblk 唯一 client，串行 RPC 无回复抢答面。
    // 缺席/失败 = blk_cli=0 → fsmgr gift 三元（RAM-only 降级，fat.rs init
    // 对 blk_h()==0 静默 None）。
    let mut blk_cli: Handle = 0;
    let mut msz = 0u64;
    if berry_vmo_get_size(H_MMIO, &mut msz) != 0 {
        eco_skip(b"mmio absent");
    } else if let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/vblk") {
        let (blk_a, blk_b) = channel_pair();
        // stdin 面：gift[2] 前要求 gift[1] 在位（berry-exec assert）——占位
        // dummy（空 feeder + 立即 EOF；vblk 服务永不读 stdin，EOF 留队无害）。
        match spawn_rootfs_elf(
            H_ROOTFS,
            &SpawnSpec {
                name: b"bin/vblk",
                data_off,
                data_len,
                stdout: console,
                stack_pages: 8,
                stdin_feed: Some(&[]),
                stdin_handle: None,
                args: &[],
                extra_gifts: &[blk_b],
                mmio_vmo: Some(H_MMIO),
                mmio_va: VIRTIO_MMIO_VA,
                intent: 0,
                force_eager: true,
            },
        ) {
            Ok(_child) => {
                ps_reg(b"vblk", psp::STATE_ALIVE, 0); // 拉起即常驻（自验门成败都活——pair 互钉）
                // 常驻服务：**不 reap**——reap 首步 wait TERMINATED 30s 绝对
                // 帽对「不退出的服务」必超时返 Err、回收链零执行（未确认退
                // 出绝不 unmap/close）；child 的 ht/hv/hp/vmo 留表即留命
                // （表槽有界 = 服务生命周期）。
                match blk_selftest(blk_a) {
                    Ok(()) => {
                        send_retry(console, BLKD_OK.as_ptr(), BLKD_OK.len());
                        blk_cli = blk_a; // 端点留给 fsmgr（下方预拉段交付）
                        // 数据盘在位旗（GUI-8 R1 判别器 v2）：真 virtio-blk
                        // 应答写读 LBA5 过才达此（EFI 冻结相 usb-storage
                        // 非 vblk 域，自验即拒 eco: blkd write → 恒 0）。
                        BLK_OK.store(1, Ordering::Relaxed);
                    }
                    Err(why) => {
                        eco_skip(why);
                        // 未交付即无人持有服务端——close 止损（vblk 进程经
                        // pair 互钉常活，BLKD 标记已缺 = smoke FAIL 兜底）。
                        if berry_handle_close(blk_a) != 0 {
                            trap();
                        }
                    }
                }
            }
            Err(()) => {
                // Err 全在 env send 前——blk_b 未被 take，两新鲜端显式
                // 回收防表槽泄漏。
                ps_reg(b"vblk", psp::STATE_SPAWNFAIL, 0);
                if berry_handle_close(blk_b) != 0 {
                    trap();
                }
                if berry_handle_close(blk_a) != 0 {
                    trap();
                }
                eco_skip(b"vblk spawn");
            }
        }
    }
    // ---- GUI-1：guifbd（framebuffer 显示服务）预拉起 ----
    // EFI-GOP 路（run-efi/smoke-image）：boot 把 fb 窗包 MMIO VMO 插第 5
    // 插（H_FB）+ 几何 info VMO 第 6 插（H_FBINFO）；`-kernel` 主路零插表
    // （H_FB get_size → -EBADF → `eco: fb absent` 一行降级——主 smoke
    // rootfs 相多一行诊断、零标记扰动）。拓扑：fb 窗经 `mmio_vmo` 映
    // `FB_VA`（map 不 take——本表恒持）；info VMO 随 env 块 extras[0]
    // **take 迁入** guifbd（libc `gift2_h()`，用完即弃）。子自验三步
    // （clear 差分/蜂格对拍/scroll 对拍）全过自打 `FBW ok`（stdout =
    // console，vblk 同路）；常驻 **不 reap**（vblk 同款政策）。
    // GUI-1 段②：console fan-out 端第 7 插（H_FBPIPE）随 extras[1] take
    // 迁入——guifbd 自验后转 serve 循环（recv → put_str 上屏），首条消息
    // 到达自打 `FBSERVE ok`（稳态矩阵打印持续不断 → 首条必达 = 状态谓词
    // 非竞速）。
    // GUI-2 段③（§7-① b-2）：present/hdi 域端点建对分发——探测门 else
    // 分支内、spawn 前 `channel_pair()`×2（**建点钉门内** A5-a：门外 =
    // rootfs 相每 boot 白建 4 对象 + eco_skip 路恒泄漏）。服务端端随
    // extras[2]/[3] **take 迁入** guifbd（extras 2→4 恰 8/8 满配——
    // berry-exec extras≤4 断言帽内；libc `present_srv_h()`/`hdi_srv_h()`
    // 消费）；客户端端存函数局部变量（fresh 端非 dup）——GUI-5 起交紧随
    // 本 arm 的 gui-wmd 预拉 arm 换手迁移（ADR #61 端点换手；原直递
    // gfxdemo 形已退役）。0 = guifbd 未起 → wmd/gfxdemo arm 静默跳过。
    let mut present_cli: Handle = 0;
    let mut hdi_cli: Handle = 0;
    if berry_vmo_get_size(H_FB, &mut msz) != 0 {
        eco_skip(b"fb absent");
    } else if let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/guifbd") {
        // 两对 (本端, 交接端)：客户端端本表留（下方 gfxdemo arm 消费），
        // 服务端端随 env 块迁移（fs_b/net_b 先例同款交接端形态）。
        let (p_cli, present_srv) = channel_pair();
        let (h_cli, hdi_srv) = channel_pair();
        match spawn_rootfs_elf(
            H_ROOTFS,
            &SpawnSpec {
                name: b"bin/guifbd",
                data_off,
                data_len,
                stdout: console,
                stack_pages: 8,
                stdin_feed: Some(&[]),
                stdin_handle: None,
                args: &[],
                extra_gifts: &[H_FBINFO, H_FBPIPE, present_srv, hdi_srv],
                mmio_vmo: Some(H_FB),
                mmio_va: FB_VA,
                intent: 0,
                force_eager: true,
            },
        ) {
            Ok(_child) => {
                ps_reg(b"guifbd", psp::STATE_ALIVE, 0); // 常驻（不 reap——vblk 同款）
                // 两客户端端仅此成对落值（兼作下方 gfxdemo arm 的 spawn Ok
                // 标志——NET_A/INPUT_A 的 0 值跳过同款）。
                present_cli = p_cli;
                hdi_cli = h_cli;
            }
            Err(()) => {
                // Err 全在 env send 前——H_FBINFO/H_FBPIPE 未被 take 仍在
                // 表（boot 插入原值，非本段新建）；与 H_FB 同留表（有界 2
                // 槽，v1 常驻泄漏政策——close 止损对 boot 插入值无回收价
                // 值）。段③ 四 fresh 端同样未被 take，显式回收防表槽泄漏
                // （vnet Err 路先例形态）。
                ps_reg(b"guifbd", psp::STATE_SPAWNFAIL, 0);
                if berry_handle_close(present_srv) != 0 {
                    trap();
                }
                if berry_handle_close(p_cli) != 0 {
                    trap();
                }
                if berry_handle_close(hdi_srv) != 0 {
                    trap();
                }
                if berry_handle_close(h_cli) != 0 {
                    trap();
                }
                eco_skip(b"guifbd spawn");
            }
        }
    }
    // ---- AM-2b：压力 Event 装载断言（机制面接线）+ AM-2e 观测臂起 ----
    // 门 = H_FB 在席（GOP 相——内核侧第 8 插与 fb VMO 5/fbinfo 6/endB_fb 7
    // 同分支插入，同相必在表）。探针 = `object_signal(H_PRESSURE, 0, 0)`
    // 零位面（get_raw 三门控在表/Event/SIGNAL 右；clear|set=0 → 位值不动
    // + wake 掩码 0 无命中——纯装载验证）。非 GOP 相 H_FB 先跳 = 静默零
    // 扰动（fb/virtio 惯例——PRESSURE_ARMED 恒 0，稳态观测门恒闭）；在席
    // 相探针非 0 = 接线漂移（槽缺席/缺 SIGNAL 右/类型错）→ trap 自陷
    // （「任何链断 → 自陷被杀」同口径，标记缺失 = smoke FAIL 兜底）。
    // 探针过即臂起稳态观测（AM-2e 消费沿门——消费点见 `_start` 稳态
    // loop 的 PRESSURE_ARMED poll 块）。
    if berry_vmo_get_size(H_FB, &mut msz) == 0 {
        if berry_object_signal(H_PRESSURE, 0, 0) != 0 {
            trap();
        }
        PRESSURE_ARMED.store(1, Ordering::Relaxed);
    }
    // ---- VER（ver-hard 乙侧）：版本身份对拍装载块 ----
    // 门 = H_FB 在席（GOP 相——内核侧第 9 插 sysver VMO 与第 8 插
    // H_PRESSURE **同分支**插入，同相必在表；fb 分支嵌 rootfs_present →
    // 相内 H_ROOTFS 必在，cpio 直查无裸相拓扑）。探针 = `berry_vmo_read
    // (H_VER, …, 0, 128)` 前 128B（sysver.rs 有效长全覆盖——字符串段帽
    // 128B 合同）；解析照 kernel/src/sysver.rs `parse` 同源口径（magic
    // 0x4256_4552/hdr=1/rsv 零/level/semver/两段 NUL）+ cpio 直查
    // .berryver（tomb D1 流式重扫先例——berry-exec STEP_WIN 独立窗）解
    // "<ident>|level <N>|v<semver>" 三元组对拍：全等 → `eco: VER ok
    // <ident> lvl <N>`（非标记观测族——smoke 门不 gate，QEMU 实证归主
    // 会话收口）；任一不等 → `eco: VER mix` loud 行含双值片段（混合构建
    // 〔kernel 与 rootfs 异窗〕即此脸）。非 GOP 相 H_FB 先跳 = 静默零
    // 扰动（上一块同款门形）；结构链断（读败/任一侧解析败/烙印缺席）
    // = trap（「任何链断 → 自陷被杀」口径——vmo_read 拒即第 9 插接线
    // 漂移，H_PRESSURE 探针同形）。
    if berry_vmo_get_size(H_FB, &mut msz) == 0 {
        ver_check(console);
    }
    // ---- GUI-5：gui-wmd（窗口管理守护）预拉——紧随 guifbd arm、gfxdemo
    // arm 之前（ADR #61 呈现/策略分离：权威窗口表 + present/hdi 双域透传
    // 中继）。门 = guifbd spawn Ok（present_cli != 0）∧
    // cpio_find(H_ROOTFS, b"bin/gui-wmd")。rootfs 相 guifbd 门已跳 →
    // **静默跳过零新行**（gfxdemo arm 同款先例）。
    // 拓扑：wm 对（wmd 服务端面 ↔ gfxdemo 客户端面）+ 推送对（wmd push
    // 服务端面 ↔ inputd 客户端面）本段新建；present/hdi 客户端端 =
    // guifbd arm 交接件（GUI-5 端点换手：不再随 gfxdemo，改随 wmd
    // extras[2]/[3] take 迁入）。**env 冻结 8/8**（GUI-5 设计稿 §7 Q12）：
    // extras 4 枚 = [wm_srv, push_srv, present_cli, hdi_cli] + base 4 恰
    // CHAN_HND_MAX 硬帽零余量——第 5 插 = 四处抬帽另刀。
    // intent 注记（inputd arm 同款）：两对 channel_pair() **options=0**
    // ——消息借入链有意保持激活（GUI 链路 = INTERACTIVE 借入面，ADR #42 /
    // 段① §9-6）；**建点钉门内**（A5-a：门外 = rootfs 相每 boot 白建对象
    // + eco_skip 路恒泄漏）。
    let mut wm_cli: Handle = 0;
    if present_cli != 0 {
        if let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/gui-wmd") {
            // 两对本段新建（fresh 端非 dup）：w_cli 本表留（下方 gfxdemo
            // arm 消费），push_cli 存 WMD_PUSH_B（下方 inputd arm 消费）；
            // 服务端端 + 交接件 present/hdi 客户端端随 env 块 take 迁入。
            let (w_cli, wm_srv) = channel_pair();
            let (push_cli, push_srv) = channel_pair();
            match spawn_rootfs_elf(
                H_ROOTFS,
                &SpawnSpec {
                    name: b"bin/gui-wmd",
                    data_off,
                    data_len,
                    stdout: console,
                    stack_pages: 8,
                    stdin_feed: Some(&[]),
                    stdin_handle: None,
                    args: &[],
                    extra_gifts: &[wm_srv, push_srv, present_cli, hdi_cli],
                    mmio_vmo: None,
                    mmio_va: 0,
                    intent: 0,
                    force_eager: true,
                },
            ) {
                Ok(_child) => {
                    ps_reg(b"gui-wmd", psp::STATE_ALIVE, 0); // 常驻（不 reap——vblk 同款）
                    // wmd 客户端端仅此落值（兼作下方 gfxdemo arm 的 spawn Ok
                    // 标志——present_cli 0 值跳过同款）；push_cli 交 inputd
                    // arm（WMD_PUSH_B 留表即留命——EFI 相 inputd 未拉起则
                    // 恒滞 static 保活）。
                    wm_cli = w_cli;
                    WMD_PUSH_B.store(push_cli, Ordering::Relaxed);
                    // GUI-8 段②：dup 一枚 wm 客户端端落 WMD_CLI_DUP 留表
                    // （SEND|RECV 双权利——bsh fs_a 同口径）——**常驻 dup 源**
                    // （NET_A 先例）：launcher 装配（下方新位 arm / 监督一次
                    // 重启）每次从它现 dup 新别名作 gift，本体永不消费。原柄
                    // w_cli 随 gfxdemo arm extras 直递 take 后离表（其 Err/
                    // cpio 缺路关原柄不置 0），本 static 是 launcher 侧唯一
                    // 可再生 wm 端（生命周期独立——gfxdemo 失败关原柄不伤
                    // dup）。失败 trap（fsmgr fs_dup 先例同形）。
                    let mut w_dup: Handle = 0;
                    if berry_handle_duplicate(w_cli, RIGHT_SEND | RIGHT_RECV, &mut w_dup) != 0 {
                        trap();
                    }
                    WMD_CLI_DUP.store(w_dup, Ordering::Relaxed);
                }
                Err(()) => {
                    ps_reg(b"gui-wmd", psp::STATE_SPAWNFAIL, 0);
                    // Err 全在 env send 前——六 fresh/交接端无一被 take：
                    // wm 对 ×2 + push 对 ×2 本段新建须回收；present/hdi 客户
                    // 端端虽 guifbd arm 所建，换手失败已无人交付（guifbd 侧
                    // 服务端端互钉常活），一并回收 + 置 0（防下方 arm 误
                    // 用）——显式回收防表槽泄漏（vnet Err 路先例形态）。
                    if berry_handle_close(wm_srv) != 0 {
                        trap();
                    }
                    if berry_handle_close(w_cli) != 0 {
                        trap();
                    }
                    if berry_handle_close(push_srv) != 0 {
                        trap();
                    }
                    if berry_handle_close(push_cli) != 0 {
                        trap();
                    }
                    if berry_handle_close(present_cli) != 0 {
                        trap();
                    }
                    if berry_handle_close(hdi_cli) != 0 {
                        trap();
                    }
                    eco_skip(b"gui-wmd spawn");
                }
            }
        } else {
            // cpio 缺 gui-wmd（手工裁剪镜像面）：present/hdi 客户端端未被
            // take（guifbd 侧服务端端已迁入不受影响），显式回收
            // （gfxdemo arm cpio 缺分支同款 close-only；零新行——置 0 死存
            // 弃：换手后无读者，编译器证死存即警告，GUI-2段③ 先例同形）。
            if berry_handle_close(present_cli) != 0 {
                trap();
            }
            if berry_handle_close(hdi_cli) != 0 {
                trap();
            }
        }
    }
    // ---- GUI-2 段③：gfxdemo（首个 gfx 消费者）预拉——紧随 wmd arm ----
    // 门 = wmd spawn Ok（wm_cli != 0——客户端端仅 Ok 路落值，兼作标志）
    // ∧ cpio_find(H_ROOTFS, b"bin/gfxdemo")。rootfs 相 guifbd 门已跳
    //（`fb absent`）→ 标志 0 → **静默跳过零新行**（inputd INPUT_A==0
    // 静默跳过同款先例——§0「静默」的字面落实，不设 GFX absent 行）。
    // 拓扑（GUI-5 端点换手）：wm 客户端端随 extras[0] **直递**（fresh 端
    // 非 dup——fs_b/net_b 先例；gfxdemo 侧 libc `wm_cli_h()` 消费，wm 六
    // op + present/hdi 中继同端，0 = 自行降级退 exit(0)）；present/hdi
    // 客户端端已随 wmd arm 换手迁移（本 arm 不再触碰）；stdout=console
    //（`GFX ok` 全链断言过自打）；stdin Some(&[]) 占位（extras 非空须
    // stdin 槽在位——berry-exec assert）。**阻塞 reap 收退出码**
    //（stdhello 预拉同形；berry-exec REAP_TIMEOUT 30s 在册）：零码零新行
    //（GFX ok 归 gfxdemo 自打）；非零/Err 面 eco 行续走（失败隔离——标记
    // 缺失 = smoke FAIL 兜底）。**不 ps_reg**（thr 先例注记——瞬态验收件
    // 不入账，PS ok n= 零漂移）。
    if wm_cli != 0 {
        if let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/gfxdemo") {
            match spawn_rootfs_elf(
                H_ROOTFS,
                &SpawnSpec {
                    name: b"bin/gfxdemo",
                    data_off,
                    data_len,
                    stdout: console,
                    stack_pages: 8,
                    stdin_feed: Some(&[]),
                    stdin_handle: None,
                    args: &[],
                    extra_gifts: &[wm_cli],
                    mmio_vmo: None,
                    mmio_va: 0,
                    intent: 0,
                    force_eager: false,
                },
            ) {
                Ok(child) => {
                    match child.reap() {
                        // 零码零新行——GFX ok 归 gfxdemo 自打（stdhello 的
                        // EXIT ok 同位，本 arm 无自证标记面）。
                        Ok(0) => {}
                        Ok(_) => eco_skip(b"gfx code"),
                        Err(()) => {
                            // 挂死/帽尽：berry-exec 不冒险回收（reap 超时
                            // 路）——原柄 w_cli 仍滞挂死进程表。此时若续走
                            // 接线 launcher = 同一 wm 端点双持有者（滞留原
                            // 柄 + dup 柄）并发 recv、应答互截，违 wmd「单
                            // wm 客户端串行」头注前提——dup 柄回收 + 清零，
                            // launcher arm 门落空静默跳过（此路径 GFX 标记
                            // 已缺 = smoke FAIL 兜底在前，无标记漂移面；
                            // GUI-8 wiring 刀审查修：gfxdemo arm 内本分支
                            // 时 WMD_CLI_DUP 恒非 0——门 wm_cli!=0 ⟸ wmd
                            // Ok 路 dup 已 store）。
                            if berry_handle_close(WMD_CLI_DUP.load(Ordering::Relaxed)) != 0 {
                                trap();
                            }
                            WMD_CLI_DUP.store(0, Ordering::Relaxed);
                            eco_skip(b"gfx reap");
                        }
                    }
                }
                Err(()) => {
                    // Err 全在 env send 前——wm 客户端端未被 take，显式回收
                    // 防表槽泄漏（vnet 同款；wmd 侧服务端端互钉常活，
                    // GFX/GFXF 标记已缺 = smoke FAIL 兜底）。
                    if berry_handle_close(wm_cli) != 0 {
                        trap();
                    }
                    eco_skip(b"gfxdemo spawn");
                }
            }
        } else {
            // cpio 缺 gfxdemo（手工裁剪镜像面）：wm 客户端端未被 take
            //（wmd 侧服务端端已迁入不受影响），显式回收防表槽泄漏（本
            // arm Err 路同款；常驻 svcmgr 表零终身滞留物）。
            if berry_handle_close(wm_cli) != 0 {
                trap();
            }
        }
    }
    // ---- P6 A-1：fsmgr（RAM-fs 服务）预拉起（A-5：+ gift[3] blk 端）----
    // mkcpio 名序 bsh 首位、fsmgr 第 3——但 bsh 的 gift[3]（fs 客户端端）
    // 依赖 fsmgr 先起：主走查前 cpio_find 直查先拉（主走查无 fsmgr arm，
    // 条目自然跳过）。拓扑：本表持 fs_a → dup 出 SEND|RECV 交 bsh（bsh
    // arm 内）；fs_b 随 env 块 gift[2] 迁入 fsmgr（libc `gift2_h()`）；
    // A-5：blk_cli（vblk 服务端端，自验门已过）随 gift[3] 一并 take 迁入
    // （libc `blk_h()`）——fsmgr 启动即 eager FAT init（vblk 已在服务，
    // 首读不候）。无 bin/fsmgr / spawn 失败 = fs_client=0 → bsh 退回三元
    // gift（libc `fs_h()==0` → 「fs not available」降级不死 shell，向后
    // 兼容）。
    let mut fs_a_h: Handle = 0;
    let mut fs_client: Handle = 0;
    if let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/fsmgr") {
        let (fs_a, fs_b) = channel_pair();
        // fs 客户端端双权利：SEND（发请求）+ RECV（收应答）——bsh 的
        // `>`/`cat`/`ls` 双向面。
        let mut fs_dup = 0 as Handle;
        if berry_handle_duplicate(fs_a, RIGHT_SEND | RIGHT_RECV, &mut fs_dup) != 0 {
            trap();
        }
        // A-5 gift 数组：blk_cli==0（vblk 缺席/未过自验）= 三元（RAM-only）。
        let mut fgifts = [0 as Handle; 2];
        fgifts[0] = fs_b;
        let fgifts: &[Handle] = if blk_cli != 0 {
            fgifts[1] = blk_cli;
            &fgifts
        } else {
            &fgifts[..1]
        };
        // stdin 面：gift[2] 前要求 gift[1] 在位（berry-exec assert）——占位
        // dummy（空 feeder + 立即 EOF；fsmgr 永不读 stdin，EOF 留队无害）。
        // 栈 8 页（stdhello 同款 std 小程序档；A-5 Fat 结构体 ≈3.6KB 在 main
        // 栈 + init 局部 ≈2.5KB，32KiB 余量足）。
        match spawn_rootfs_elf(
            H_ROOTFS,
            &SpawnSpec {
                name: b"bin/fsmgr",
                data_off,
                data_len,
                stdout: console,
                stack_pages: 8,
                stdin_feed: Some(&[]),
                stdin_handle: None,
                args: &[],
                extra_gifts: fgifts,
                mmio_vmo: None,
                mmio_va: 0,
                intent: 0,
                force_eager: true,
            },
        ) {
            Ok(_) => {
                ps_reg(b"fsmgr", psp::STATE_ALIVE, 0); // 拉起即常驻（FAT 降级不影响活性）
                fs_a_h = fs_a;
                fs_client = fs_dup;
                // blk_cli 已被 take 迁移（旧值必 -EBADF，勿关）——清零防
                // 尾部兜底误关。
                blk_cli = 0;
            }
            Err(()) => {
                ps_reg(b"fsmgr", psp::STATE_SPAWNFAIL, 0);
                // Err 路全在 env send 前——fs_b/blk_cli 均未被 take、fs_dup
                // 是 fresh dup 非 take 产物，显式回收防表槽泄漏。
                if berry_handle_close(fs_dup) != 0 {
                    trap();
                }
                if berry_handle_close(fs_b) != 0 {
                    trap();
                }
                if berry_handle_close(fs_a) != 0 {
                    trap();
                }
                if blk_cli != 0 && berry_handle_close(blk_cli) != 0 {
                    trap();
                }
                blk_cli = 0;
                eco_skip(b"fs spawn");
            }
        }
    }
    // ---- P6 A-8b：vnet（常驻网络服务）预拉起——fsmgr 之后、主走查前 ----
    // A-7 时 vnet 在主走查 arm 拉起（瞬态探针：`NET ok` 自打后 exit(0) +
    // svcmgr 阻塞 reap 回收）；A-8b 起自验毕转 serve() 常驻（berry-netdproto
    // DNS_QUERY RPC）⇒ 改预拉（主走查 arm 退役，条目自然跳过）。拓扑：
    // channel_pair 得 (net_a, net_b)——net_b 随 env 块 gift[2] 迁入 vnet
    // （libc `gift2_h()`）；net_a 留本表存 NET_A（A-8c 起作 DNS 自验
    // client）。窗口缺席 → vblk 同款可见降级不 spawn；常驻子不 reap
    // （vblk A-4 同款：wait TERMINATED 30s 帽必超时空烧）。
    if berry_vmo_get_size(H_MMIO, &mut msz) != 0 {
        eco_skip(b"mmio absent");
    } else if let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/vnet") {
        let (net_a, net_b) = channel_pair();
        // stdin 面：gift[2] 前要求 gift[1] 在位（berry-exec assert）——占位
        // dummy（空 feeder + 立即 EOF；vnet 服务永不读 stdin，vblk/fsmgr
        // 同款）。
        match spawn_rootfs_elf(
            H_ROOTFS,
            &SpawnSpec {
                name: b"bin/vnet",
                data_off,
                data_len,
                stdout: console,
                stack_pages: 8,
                stdin_feed: Some(&[]),
                stdin_handle: None,
                args: &[],
                extra_gifts: &[net_b],
                mmio_vmo: Some(H_MMIO),
                mmio_va: VIRTIO_MMIO_VA,
                intent: 0,
                force_eager: true,
            },
        ) {
            Ok(_child) => {
                ps_reg(b"vnet", psp::STATE_ALIVE, 0); // 拉起即常驻（自验门成败都活——serve 常驻）
                // 常驻不 reap。`NET ok` 标记仍由 vnet 自打（自验在 serve()
                // 前，rootfs 相第 17 标记零扰动）；DNS ok（A-8c）起本端
                // 转 client 消费。
                NET_A.store(net_a, Ordering::Relaxed);
            }
            Err(()) => {
                ps_reg(b"vnet", psp::STATE_SPAWNFAIL, 0);
                // Err 全在 env send 前——net_b 未被 take，两新鲜端显式回收
                // 防表槽泄漏（vblk 同款）。
                if berry_handle_close(net_b) != 0 {
                    trap();
                }
                if berry_handle_close(net_a) != 0 {
                    trap();
                }
                eco_skip(b"net spawn");
            }
        }
    }
    // ---- GUI-4 段②：gui-inputd 预拉（virtio-tablet 输入服务）。门控同 vnet
    // 两道：H_MMIO 在席（EFI/裸拓扑缺席 → eco_skip(b"mmio absent") 静默
    // 零扰动）+ cpio_find(H_ROOTFS, b"bin/gui-inputd")。
    //
    // intent 注记（台账 6，v1 可达面）：input 域 channel 由本表 channel_pair()
    // 创建、**options=0**——不设 CHAN_OPT_NO_INTENT，消息借入链（4-3c：收端按
    // 每消息重导 eff = min(declared, msg_intent)）有意保持激活 = GUI 规划 GUI-4
    // 行「intent INTERACTIVE」的 v1 唯一可达形态。UNSPECIFIED(0x00) 顶档语义下
    // 借入恒不抑制（4-3c 数值序）；svcmgr/bsh 自身 declared 不变——内核侧
    // berry_intent 自申报 = 另刀。ADR #42 / 段① §9-6。
    //
    // 常驻不 reap（A74：reap 首步 wait 30s 帽必超时）；stdin 哑喂 Some(&[])；
    // recv 容量按「协议定值 1 op + 富余」判面（段① serve() 已落，本刀零改）。
    // GUI-5：extras 追加 wmd 推送客户端端（WMD_PUSH_B——wmd arm Ok 路存
    // 值；0 = wmd 未起 → extras 仍单枚，inputd 现行为逐字节保留）。失败面
    // = close(in_b) + close(in_a)（+ push_b 在席时第三枚）+ eco_skip——bsh
    // inputpoll 面静默降级（input_h()==0 → "input: absent"）。
    if berry_vmo_get_size(H_MMIO, &mut msz) != 0 {
        eco_skip(b"mmio absent");
    } else if let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/gui-inputd") {
        let (in_a, in_b) = channel_pair();
        // GUI-5 推送端追加（fsmgr fgifts 条件切片同款）：WMD_PUSH_B 非 0 =
        // wmd 已起，extras 次位随行 push_b（inputd 侧 libc `wm_push_h()`）。
        let push_b = WMD_PUSH_B.load(Ordering::Relaxed);
        let mut igifts = [in_b, 0 as Handle];
        let igifts: &[Handle] = if push_b != 0 {
            igifts[1] = push_b;
            &igifts
        } else {
            &igifts[..1]
        };
        // stdin 面：gift[2] 前要求 gift[1] 在位（berry-exec assert）——占位
        // dummy（空 feeder + 立即 EOF；inputd 服务永不读 stdin，vnet 同款）。
        match spawn_rootfs_elf(
            H_ROOTFS,
            &SpawnSpec {
                name: b"bin/gui-inputd",
                data_off,
                data_len,
                stdout: console,
                stack_pages: 8,
                stdin_feed: Some(&[]),
                stdin_handle: None,
                args: &[],
                extra_gifts: igifts,
                mmio_vmo: Some(H_MMIO),
                mmio_va: VIRTIO_MMIO_VA,
                intent: 0,
                force_eager: true,
            },
        ) {
            Ok(_child) => {
                ps_reg(b"inputd", psp::STATE_ALIVE, 0); // 拉起即常驻（boot 自验门成败都活——serve 常驻）
                // 常驻不 reap。`INPUT ok` 标记由 boot 期 input_selftest 打
                // （SUBSCRIBE RPC 走本端 in_a——net_a 转 client 同款）；
                // `gui-inputd: up slot=… cal=…` 诊断行（ABS 域真值源）由
                // inputd 自打。
                INPUT_A.store(in_a, Ordering::Relaxed);
                // push_b 已被 take 迁入 inputd 表——static 清零防后续误用
                // （fsmgr blk_cli 同款；已迁值不可 close）。
                WMD_PUSH_B.store(0, Ordering::Relaxed);
            }
            Err(()) => {
                ps_reg(b"inputd", psp::STATE_SPAWNFAIL, 0);
                // Err 全在 env send 前——in_b/push_b 未被 take，新鲜端显式
                // 回收防表槽泄漏（vnet 同款；push_b = wmd push 服务端端的
                // 对端，close = 服务端端成孤儿——但 spawn 已败，wmd 推送链
                // 已断，留表无保活意义）。
                if berry_handle_close(in_b) != 0 {
                    trap();
                }
                if berry_handle_close(in_a) != 0 {
                    trap();
                }
                if push_b != 0 && berry_handle_close(push_b) != 0 {
                    trap();
                }
                WMD_PUSH_B.store(0, Ordering::Relaxed);
                eco_skip(b"inputd spawn");
            }
        }
    }
    // ---- GUI-8 段②：berry-launcher（桌面）装配 arm——inputd 后、stdhello
    // 前（草案 §2.3 序重排）。**D3 追认论证**（scout S3-F3）：launcher 仍
    // 在 gfxdemo 阻塞 reap 之后（单 wm 客户端序——gfxdemo 先活后死、
    // launcher 后活，wmd 头注前提保住）；fsmgr/vnet/inputd 三 arm 皆不触
    // wm 域，launcher 越其身后 = 重排安全。新位三得：fsmgr 后 = fs_client
    // 在位可让、inputd 后 = INPUT_A 在位可判、监督/serve 所需端点齐。
    // 门 = WMD_CLI_DUP != 0（= wmd Ok ∧ fb 在位；rootfs 形恒 0 → **静默
    // 跳过零新行**，48 标记零漂）。装配体 = `launcher_assemble`（boot 本
    // arm 与监督一次重启（裁决四-d）共用形）；装配表（env base 4 +
    // extras ≤4 = 8/8 恰满，bsh 满配先例，零内核抬帽）：
    //   extras[0] wm 客户端 fresh dup（源 WMD_CLI_DUP 常驻 dup 源——每次
    //            装配现 dup，本体留表；launcher 侧 libc `wm_cli_h()`）
    //   extras[1] fs 客户端端 = fs_client 让位迁移（**R1 判别器 v2**：仅
    //            BLK_OK≠0 交互相让位（数据盘在位）；libc `fs_h()`）
    //   extras[2] app 域客户端端（arm 门内 channel_pair() 对端，服务端端
    //            APP_SVC 留本表稳态 serve——裁决二；随 fs 让位同门截断；
    //            libc `app_cli_h()`）
    //   extras[3] input 客户端端 dup（源 INPUT_A，SEND|RECV；交互相缺席
    //            → 截断；libc `input_h()`）
    // **R1 判别器 v2（第二次修约，仪器实测定谳）**：fs 让位只随交互相，
    // 判据 = BLK_OK≠0（数据盘在位 = vblk 自验过；交互相结构性配置 tablet+
    // QMP+mkfat 盘三件同挂，fsmgr 挂同一盘——「launcher 需 LAYOUT.JSN 的
    // 卷在位」语义恰合）。废案存证（M-A fallback INPUT_A≠0）：实测 EFI
    // 冻结相恒真——inputd 预拉只门 H_MMIO（virt 恒在）、spawn Ok 即 store
    // INPUT_A、tablet 探测在 inputd 进程内晚于 store（park 不回清）→ 误
    // 让位 → bsh fs_h()==0 → fs_selftest PUT64 ENOENT park → fs 族 11 标记
    // 连坐（仪器实测 PUT64 not found h=0）。冻结十段相 BLK_OK=0 → fs 留
    // bsh（[rootfs,fs,net] 三元皮现状零漂——fs 族 11 标记 + fat 段 park 形
    // 全保；首轮 WMD_CLI_DUP 判别同病 = M-A 双报形态）。
    // app/input 随 fs 让位同门截断（前缀紧致律/netd 条款镜像：app 前移落
    // fs 槽 hs[5] = 同槽不同名）→ **冻结相实形 = 恰 [wm] 一枚**（e41ae92
    // 接线刀 EFI 形同构零漂）；交互相 = [wm,fs,app,input] 满四枚。
    // **入账**：Ok 路 ps_reg(b"launcher", ALIVE, 0)（psproto 帽 18 恰容
    // ——worker F 同刀抬 17→18；时序论证：不抬帽先入账 = 表恰满丢行、
    // berryctl 自验结构性 FAIL，两件必须同刀）；常驻不 reap（wmd 同款）
    // ——探活归稳态 loop `launcher_supervise`（裁决四-d）。家名单
    // （ps_selftest has_sub 链）不添（guifbd/wmd 不在册先例）。fs 串行
    // 纪律注记：让位后 launcher = fs 唯一桌面读者（布局写在桌面态、app
    // 活时结构性无布局编辑——裁决四-b 相位互斥）；冻结相 launcher 零 fs
    // 零 app（[wm] park 形，bsh 全自验照跑）。
    if WMD_CLI_DUP.load(Ordering::Relaxed) != 0 {
        // R1 判别器 v2：交互相（BLK_OK≠0 数据盘在位）才让位 fs；冻结相 0
        // 直传 = [wm] 截断形（assemble 前缀截断纪律自处）。rootfs 形
        // BLK_OK=1（disk.img 自验过）但 arm 门 WMD_CLI_DUP==0 恒跳过——
        // 判别器不消费零漂。
        let lg_fs: Handle = if BLK_OK.load(Ordering::Relaxed) != 0 {
            fs_client
        } else {
            0
        };
        let took = if launcher_assemble(console, lg_fs) {
            // GUI 形 fs 常驻别名（NET_A 先例——**不随 bsh 关**）：app 域
            // serve 的 fs 授能源（app_launch fs_a 参数）+ 监督重启 fs gift
            // 现 dup 源。fs 缺席形 fs_a_h==0 存 0 = 空占位无害。
            FS_A.store(fs_a_h, Ordering::Relaxed);
            true
        } else {
            false
        };
        // **让位 = 真实迁移点**（对抗审查 M0 裁定）：Ok 且 lg_fs≠0 = take
        // 成立（extras[1] 随 env 迁入子表）→ 局部归零防 bsh 复用；Ok 而
        // lg_fs==0 = 未涉让位；Err/cpio 缺 = assemble 契约原样归还（未
        // close）——两路 fs_client 原值流 bsh，cpio 手工裁剪角不饿死。
        if took && lg_fs != 0 {
            fs_client = 0;
        }
    }
    // ---- C-1 刀 3：stdhello 预拉起——vnet 之后、主走查（bsh spawn）前 ----
    // 原 arm 位置（名序 stdhello 在 bsh/hexyl 后）与 bsh 启动自验（fs 自验
    // PUT/GET + FAT/FATW 链 = 秒级多轮 RPC）并发共用 fs 通道 = 回复混流面
    // （fsproto 严格串行、无请求号——两客户交叠请求即跨客户错配，断言炸
    // = FAIL）。预拉到 bsh spawn 前 = fs_a 队列本窗口单活跃客户（机制性
    // 排除交叠，非时序赌注——「标记不得建立在竞速上」）。fs gift = fs_a
    // 的 SEND|RECV 新 dup，gifts [rootfs dup, fs 端]（gift[3] 位语义同
    // bsh，libc `fs_h()` 消费；rootfs 位占位兼 gift[1] 在位约束）；
    // fs_a_h==0（fsmgr 未起）→ 三元 gift，stdhello 自降级（FILE skip 行，
    // 标记缺失交 smoke）。stdin feeder 64B / EXIT ok 语义原 arm 原样迁居
    // （阻塞 reap 只延迟 walk，vprobe 同口径）。arm 退役为跳过。
    // 旗在此声明兼立（spawn Err 也不给 walk arm 双拉口——bsh_spawned
    // 同款纪律；恒真到 walk arm = 预拉必然先于 walk 执行）。
    let stdhello_spawned = true;
    if let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/stdhello") {
        // d-3b 同款：feeder 泵 64B 确定性 pattern + EOF（read_to_end 残段
        // 缓冲面的活证）。
        const STDIN_DATA: [u8; 64] =
            *b"berryos!berryos!berryos!berryos!berryos!berryos!berryos!berryos!";
        let stdin: &[u8] = &STDIN_DATA;
        let mut rootfs_dup = 0 as Handle;
        if berry_handle_duplicate(H_ROOTFS, RIGHT_READ, &mut rootfs_dup) != 0 {
            trap();
        }
        let mut fs_std = 0 as Handle;
        if fs_a_h != 0 && berry_handle_duplicate(fs_a_h, RIGHT_SEND | RIGHT_RECV, &mut fs_std) != 0
        {
            trap();
        }
        let gifts = [rootfs_dup, fs_std];
        let gifts: &[Handle] = if fs_std != 0 { &gifts } else { &gifts[..1] };
        match spawn_rootfs_elf(
            H_ROOTFS,
            &SpawnSpec {
                name: b"bin/stdhello",
                data_off,
                data_len,
                stdout: console,
                stack_pages: 8,
                stdin_feed: Some(stdin),
                stdin_handle: None,
                args: &[],
                extra_gifts: gifts,
                mmio_vmo: None,
                mmio_va: 0,
                intent: 0,
                force_eager: false,
            },
        ) {
            Ok(child) => {
                ps_reg(b"stdhello", psp::STATE_ALIVE, 0);
                // Plan B v0.5 原语义：reap 返真退出码，`EXIT ok` 只在
                // code==0 打（stdhello 恒 main 返 0）。reap 失败保 alive——
                // EXIT ok 已缺 = FAIL 已响，alive 是诚实下界。
                match child.reap() {
                    Err(()) => eco_skip(b"exit reap"),
                    Ok(0) => {
                        ps_exit(b"stdhello", 0);
                        send_retry(console, EXIT_OK.as_ptr(), EXIT_OK.len());
                    }
                    Ok(c) => {
                        ps_exit(b"stdhello", c);
                        eco_skip(b"exit code");
                    }
                }
            }
            Err(()) => {
                ps_reg(b"stdhello", psp::STATE_SPAWNFAIL, 0);
                // Err 三路全在 env send 前——gift dup 未被 take，显式回收防
                // 表槽泄漏（bsh arm 同款）。
                if berry_handle_close(rootfs_dup) != 0 {
                    trap();
                }
                if fs_std != 0 && berry_handle_close(fs_std) != 0 {
                    trap();
                }
                eco_skip(b"elf bad");
            }
        }
    }
    // ---- C-2a：uutils core 家族单子验收（`U1 ok` = rootfs 相第 25 标记）。
    // stdhello 预拉同窗口的 fs 串行纪律：每子 EOF + 阻塞 reap 后才起下一
    // 个（单活跃客户——fsproto 严格串行无请求号，交叠即混流；bsh arm 在
    // 本 arm 之后，其 fs 自验与本窗口零交叠）。fsmgr 未起（fs_a_h==0）→
    // 静默跳过（rootfs 相 fsmgr 链断本就该 FAIL——U1 缺失 loud）。
    if fs_a_h != 0 && u1_selftest(console, fs_a_h) {
        send_retry(console, U1_OK.as_ptr(), U1_OK.len());
    }
    while pos < cap {
        let want = ((cap - pos).min(step.len() as u64)) as usize;
        if berry_vmo_read(H_ROOTFS, step.as_mut_ptr(), pos, want) != 0 {
            eco_skip(b"vmo_read failed");
            return;
        }
        match berry_loader::cpio_step(&step[..want], pos, cap) {
            Ok(berry_loader::StepOutcome::End) => break,
            Ok(berry_loader::StepOutcome::Entry(s)) => {
                let (name, data_off, data_len, next) = (s.name, s.data_off, s.data_len, s.next_off);
                // 名字从步进窗借引用：下一轮 cpio_step 前用完（spawn 内的
                // env 发送即消费点，之后只碰 ELF_WIN/PUMP_WIN）。
                if name == "bin/hello" {
                    match spawn_rootfs_elf(
                        H_ROOTFS,
                        &SpawnSpec {
                            name: name.as_bytes(),
                            data_off,
                            data_len,
                            stdout: console,
                            stack_pages: 1,
                            stdin_feed: None,
                            stdin_handle: None,
                            args: &[],
                            extra_gifts: &[],
                            mmio_vmo: None,
                            mmio_va: 0,
                            intent: 0,
                            force_eager: false,
                        },
                    ) {
                        // hello 常驻（10s 周期睡眠不回收）——入账。
                        Ok(_) => {
                            ps_reg(b"hello", psp::STATE_ALIVE, 0);
                            send_retry(console, ECO_OK.as_ptr(), ECO_OK.len());
                            // X-6 观测行（hello = 最小 demand bin）。
                            x6_mark(console)
                        }
                        Err(()) => {
                            ps_reg(b"hello", psp::STATE_SPAWNFAIL, 0);
                            eco_skip(b"elf bad");
                        }
                    }
                } else if name == "bin/stdhello" {
                    // C-1 刀 3：预拉段（vnet 后、walk 前）已 spawn + 阻塞
                    // reap + EXIT ok——fs gift 单客户窗口须在 bsh spawn 前
                    // （fsproto 无请求号，与 bsh 启动自验并发 = 回复混流），
                    // 见预拉段注。本 arm 退役为跳过；旗未立 = 预拉段被跳过
                    // 的对抗面，可见降级行。
                    if !stdhello_spawned {
                        eco_skip(b"stdhello not pre-pulled");
                    }
                } else if name == "bin/hexyl" {
                    // d-3c：D4 路线①最终验证——上游 main.rs 逐字未动的生态
                    // 程序。argv 追加 `--color=never`（0.17 默认 Color=Always
                    // 会打 ANSI 码；plain 输出 = 宿主同版管道对照可逐字比）
                    // ——clap 真解析面，多 argv 首例。stdin = 0x00..=0xff
                    // 全字节 pattern（256B 恰一条消息 = inline 帽边界活证）。
                    // 栈 32 页：clap/Once/fmt 链实测深 ≈64KB——8 页栈用穿后
                    // **下行栈静默扎进自己映像 RW 页**，首中 0x30a000 页的
                    // `__aarch64_have_lse_atomics` 翻非零 → cortex-a72（无
                    // LSE）上 `casa` = UNDEFINED 自陷（d-3c 排障实录）。
                    // musl 默认 128KiB 栈同此口径。
                    const HEXYL_STDIN: [u8; 256] = {
                        let mut a = [0u8; 256];
                        let mut i = 0;
                        while i < 256 {
                            a[i] = i as u8;
                            i += 1;
                        }
                        a
                    };
                    let stdin: &[u8] = &HEXYL_STDIN;
                    // 私有 stdout 终验通道（d-3c）：fd1/2 交接本对 `out_c` 而
                    // 非共享 console——稳态矩阵的打印会在 192B 分片边界间插花
                    // （实跑：0xa0 行被 A/B 循环 + `DEMO ok` 撕开），对表行做
                    // grep 标记 = 时序竞速；svcmgr 收齐全量逐字节比对基准后打
                    // **单消息原子** `HEXYL ok`（每 boot 确定性成立）。
                    let (out_s, out_c) = channel_pair();
                    match spawn_rootfs_elf(
                        H_ROOTFS,
                        &SpawnSpec {
                            name: name.as_bytes(),
                            data_off,
                            data_len,
                            stdout: out_c,
                            stack_pages: 32,
                            stdin_feed: Some(stdin),
                            stdin_handle: None,
                            args: &[b"--color=never"],
                            extra_gifts: &[],
                            mmio_vmo: None,
                            mmio_va: 0,
                            intent: 0,
                            force_eager: false,
                        },
                    ) {
                        Err(()) => eco_skip(b"elf bad"),
                        Ok(child) => {
                            verify_hexyl(out_s, console);
                            // Plan B：终验后 best-effort 回收（Err 静默跳过——
                            // HEXYL ok 语义只锚定输出面，不叠回收成败；30s 帽
                            // 内 EOF 已收 = 子进程退出在即，常态瞬回）。
                            let _ = child.reap();
                        }
                    }
                    // out_c 原值：子表已持 SEND 别名（dup 迁移），本表副本使命已毕
                    if berry_handle_close(out_c) != 0 {
                        trap();
                    }
                    if berry_handle_close(out_s) != 0 {
                        trap();
                    }
                } else if name == "bin/bsh" {
                    // A-2：交互 shell bsh——**SDK-1a §1.2 起延后拉起**：本 arm
                    // 只记包内几何（data_off/data_len），spawn 整体挪到走查毕
                    // app 批之后的 `if let Some((off, len)) = bsh_geom` 段
                    // （原 arm 体逐字迁居，gift 拓扑注释随行）——app 批要给
                    // app 装 fs gift，结构性拿不到安全端点（原 arm spawn Ok
                    // 后关 fs_a_h = bsh 自此是 fs_a 队列唯一读者），裁「app
                    // 批前移、bsh 延后」。重复 `bin/bsh` 条目 = 可见降级。
                    if bsh_geom.is_some() {
                        eco_skip(b"bsh dup");
                    } else {
                        bsh_geom = Some((data_off, data_len));
                    }
                } else if name == "bin/simbattery" {
                    // sim-S1 M2-b：sim 电池快照注入服务（常驻 daemon，
                    // `docs/sim设备通道与契约设计-v1.1.md` §6）——fd0 = 帧
                    // 通道 RECV 端（task_simcon 转发的 UART1 字节流）、
                    // gift[0] = ACK SEND 端（libc `sim_ack_h()`）、fd1/2 =
                    // console 直打。门 = boot 插的 sim 礼物对**双双**在席
                    // （双 dup 探针：RX 探 RECV + ACK 探 SEND，任一 -EBADF
                    // = 门闭）：**门闭静默跳过**——裸相/smoke/EFI 每 boot
                    // 都走本 arm，eco_skip 会污染无门相串口（B4 零扰动
                    // 硬线）；门开 spawn 失败才 loud（SIMBAT 标记缺失 +
                    // 宿主 client 超时双兜底）。常驻不 reap（hello 同款
                    // ps_reg 入账）。
                    // 双探针的由来（2026-11-07 红 smoke 定谳，单探针假开
                    // 事故）：无 sim 相 slot5/6 被 _start 首 channel_pair
                    // 占用——end0 值恰=H_SIM_RX 且自留真 RECV 权（RX 单探
                    // 真通过=假开），而 end1 迁入 echo 子（spawn_child 迁移
                    // 自证）后 slot6 整 boot 反复换主 gen 翻，ACK 陈值
                    // 0x0001_0006 查表必 -EBADF——假开下场 = extra_gifts
                    // 带陈值进礼物 channel_send → -EBADF → berry-exec
                    // send!=0 自陷杀 svcmgr（死档 x0=-9 PC=0x8b44，walk
                    // 悬于本 arm，vprobe/BSH/INPUT 起全缺）。ACK 探针把
                    // 假开掐死在门前：slot6 陈值必败 → 门真闭静默跳过。
                    let mut probe: Handle = 0;
                    let mut probe_ack: Handle = 0;
                    if berry_handle_duplicate(H_SIM_RX, RIGHT_RECV, &mut probe) == 0
                        && berry_handle_duplicate(H_SIM_ACK, RIGHT_SEND, &mut probe_ack) == 0
                    {
                        berry_handle_close(probe);
                        berry_handle_close(probe_ack);
                        match spawn_rootfs_elf(
                            H_ROOTFS,
                            &SpawnSpec {
                                name: name.as_bytes(),
                                data_off,
                                data_len,
                                stdout: console,
                                stack_pages: 8,
                                stdin_feed: None,
                                stdin_handle: Some(H_SIM_RX),
                                args: &[],
                                extra_gifts: &[H_SIM_ACK],
                                mmio_vmo: None,
                                mmio_va: 0,
                                intent: 0,
                                force_eager: false,
                            },
                        ) {
                            Ok(_) => ps_reg(b"simbattery", psp::STATE_ALIVE, 0),
                            Err(()) => eco_skip(b"sim spawn"),
                        }
                    }
                } else if name == "bin/vprobe" {
                    // P6 A-2：virtio-mmio 探测（用户态驱动地基第一刀）——
                    // H_MMIO 经 `SpawnSpec::mmio_vmo` 映进子空间固定 VA
                    // （berry-exec `VIRTIO_MMIO_VA` 单源），子程序读规范头
                    // 四字断言 transport 在席自打 `VIRTIO ok`（rootfs 相第
                    // 12 标记，vprobe 自有输出面）。防御：窗口缺席（旧 boot
                    // 无 P6 拓扑）= get_size -EBADF → 可见降级不 spawn（本
                    // arm 在 rootfs 相必过——新内核下 handle 恒在）。reap
                    // Err → 降级行（标记锚 vprobe 输出不受 reap 成败影响，
                    // hexyl best-effort 同口径）；Ok(_) 码不判（vprobe 断言
                    // 失败走 trap——fault 杀码 0，码无区分度，诊断靠 vprobe
                    // 自打 bad regs 行）。mkcpio 名序 vprobe 末位（v > s）
                    // ——stdhello 预拉段的阻塞 reap 在 walk 前，只延迟不挡
                    // 本 arm。
                    let mut msz = 0u64;
                    if berry_vmo_get_size(H_MMIO, &mut msz) != 0 {
                        eco_skip(b"mmio absent");
                    } else {
                        match spawn_rootfs_elf(
                            H_ROOTFS,
                            &SpawnSpec {
                                name: name.as_bytes(),
                                data_off,
                                data_len,
                                stdout: console,
                                stack_pages: 8,
                                stdin_feed: None,
                                stdin_handle: None,
                                args: &[],
                                extra_gifts: &[],
                                mmio_vmo: Some(H_MMIO),
                                mmio_va: VIRTIO_MMIO_VA,
                                intent: 0,
                                force_eager: false,
                            },
                        ) {
                            Ok(child) => {
                                if child.reap().is_err() {
                                    eco_skip(b"vprobe reap");
                                }
                            }
                            Err(()) => eco_skip(b"vprobe spawn"),
                        }
                    }
                } else if name.starts_with("app/") && name.ends_with(".bap") {
                    // app-0 a-2：app 包整条目收集（'a'<'b' 排序在一切 bin/
                    // 前——收集完 bin/ arms 才轮到，启动批在主走查毕统一走）。
                    // mkapp 门禁已保形状，但 rootfs 是外部输入面——防御照旧：
                    // id 域/BAP_MAX/槽满 = 可见降级不炸 svcmgr（单 app 坏不
                    // 连坐）。BAP_MAX 双端执法（mkapp 生产侧 + 本消费侧同值
                    // ——berry-manifest 单源，a-3 对抗审查 U2）。
                    let id = &name[4..name.len() - 4]; // ASCII 前后缀剥除
                    if id.is_empty() {
                        eco_skip(b"app id empty");
                    } else if id.len() > ID_MAX {
                        eco_skip(b"app id long");
                    } else if data_len > BAP_MAX as u64 {
                        eco_skip(b"app bap big");
                    } else if app_n >= APP_SLOTS {
                        eco_skip(b"app too many");
                    } else {
                        apps[app_n].id[..id.len()].copy_from_slice(id.as_bytes());
                        apps[app_n].id_len = id.len();
                        apps[app_n].bap_off = data_off;
                        apps[app_n].bap_len = data_len;
                        app_n += 1;
                    }
                }
                pos = next;
            }
            Err(_) => {
                eco_skip(b"cpio bad");
                return;
            }
        }
    }
    // GUI-8 段②：走查记账面持久化——APP_LIST/APP_SPAWN 的数据源计数
    // （apps 槽表本体即 static APP_SLOTS_TBL，恒存活；此处镜像 app_n 供
    // 稳态 serve 分界。SAFETY: 单线程，walk 毕一次性落值）。
    unsafe { APP_ACCOUNT_N = app_n; }
    // P6 A-1 兜底：归档无 bin/bsh（fs 面无消费者）——fs_client/fs_a 留表
    // 无害但关掉更干净；fsmgr 常驻不受影响（fs_b 互钉 + 子表自持）。
    // A-5：归档无 bin/fsmgr 时 blk_cli 未被 take——同兜底关（vblk 经
    // pair 互钉常活，无 client 只是闲置）。
    // SDK-1a §1.2：bsh 延后拉起——bsh_geom.is_some() 时 fs 端点还要交给
    // app 批（fs 授能）与 bsh 拉起段（fs_client take），此处不得提前关
    // （旧序「bsh arm spawn Ok 已置 0」的隐式跳过条件在延后态失效，显式
    // 门取代；bsh spawn Err 面的 fs_a_h 留表 = 有界 1 槽，v1 常驻泄漏政策）。
    // GUI-8 段②：GUI 形（LAUNCHER_LIVE≠0）fs_a_h 是 FS_A 常驻别名本体
    // （app 域 serve fs 授能 + 监督重启源）——让位门同 bsh_geom 门并联；
    // fs_client 在 GUI 形已于 launcher arm 归零，闭包内自然 no-op。
    if bsh_geom.is_none() && LAUNCHER_LIVE.load(Ordering::Relaxed) == 0 {
        if fs_client != 0 {
            if berry_handle_close(fs_client) != 0 {
                trap();
            }
            fs_client = 0;
        }
        if fs_a_h != 0 {
            if berry_handle_close(fs_a_h) != 0 {
                trap();
            }
            // 关后置 0：app 批仍会把 fs_a_h 传进 app_launch——陈旧非零值
            // 会绕过其 `fs_a == 0` 守卫，在已关（或更糟：槽位被复用的错
            // 对象）句柄上 duplicate（对抗审查 minor：无 bsh+fs app 拓扑
            // v1 无 recipe，潜伏面堵死）。
            fs_a_h = 0;
        }
    }
    if blk_cli != 0 {
        if berry_handle_close(blk_cli) != 0 {
            trap();
        }
    }
    // ---- app-0 a-2：.bap 启动批（收集槽全量拉起；`APP ok n=` = rootfs
    // 相第 21 标记）——确定性状态谓词：每槽 bap_scan → manifest parse →
    // 双守卫 → 拉起 → EOF/reap 真码 0，全过才打（任一降级 = 标记缺失 =
    // smoke FAIL；裸跑 app_n==0 零扰动）。SDK-1a §1.2：批块先于 bsh 拉起
    // （appd fs 授能传 fs_a_h——此刻仍持，app 批窗口 app 为 fs_a 队列唯一
    // 活动客户端：app_launch 对 console=true 阻塞 reap、console=false
    // collect_eof+reap，任一时刻至多一个 app 存活发 fs RPC）。
    if app_n > 0 {
        let mut ok = 0usize;
        // GUI-7：ui 面独立谓词（与拉起成败解耦——失败隔离两方向：ui 元数据
        // 坏不拦拉起、拉起坏不抹 ui 验证；Absent 不计入 decl）。
        let mut ui_decl = 0usize;
        let mut ui_ok = 0usize;
        for i in 0..app_n {
            // app-1 D1：APPTOMB 改 selftest/ 前缀打包后不入走查收集——
            // 批面零特判（v1.0 白名单 skip 立法已退役）。
            let (launched, uface, tail) = app_launch(
                console,
                fs_a_h,
                &apps[i].id[..apps[i].id_len],
                apps[i].bap_off,
                apps[i].bap_len,
                step,
                mf,
                icons,
                views,
            );
            // M-C γ-分·boot 批：同步消费收口尾（行为零漂——批窗口仍单
            // app 串行收口，`APP ok n=` 谓词 = 拉起 ∧ 退出真码 0 不变）。
            let launched = match tail {
                Some(t) => app_reap_tail(console, t),
                None => launched,
            };
            if launched {
                ok += 1;
            }
            match uface {
                UiFace::Ok => {
                    ui_decl += 1;
                    ui_ok += 1;
                }
                UiFace::Bad => ui_decl += 1,
                UiFace::Absent => {}
            }
        }
        if ok == app_n {
            // 带参形态防子串混叠（`BLKD ok n=` 同款）；n ≤ APP_SLOTS → 单
            // 位数字——抬 APP_SLOTS ≥ 10 前先扩带参宽度（b'0'+ok 会溢出
            // ASCII 数字域；TCPS_PATTERN 定值钉板同款编译期拦）。
            const _: () = assert!(APP_SLOTS < 10, "APP ok n= 单数字位形态");
            let mut line = [0u8; 11];
            line[..9].copy_from_slice(b"APP ok n=");
            line[9] = b'0' + ok as u8;
            line[10] = b'\n';
            send_retry(console, line.as_ptr(), line.len());
        }
        if ui_decl > 0 && ui_ok == ui_decl {
            // GUI-7（rootfs 相 45→46）：`APPUI ok n=`——ui 面全验过才打
            // （任一 Bad = 标记缺失 = smoke FAIL，`APP ok` 同款确定性状态
            // 谓词）。13B 行、n 单数字——APP_SLOTS < 10 双钉板。
            const _: () = assert!(APP_SLOTS < 10, "APPUI ok n= 单数字位形态");
            let mut line = [0u8; 13];
            line[..11].copy_from_slice(b"APPUI ok n=");
            line[11] = b'0' + ui_ok as u8;
            line[12] = b'\n';
            send_retry(console, line.as_ptr(), line.len());
        }
    }
    // ---- app-1 段 a-1：freeze-first 自验（tomb_selftest——APPTOMB 走
    // selftest/ 前缀 cpio 直查后由此拉起：suspend → 8s 静默窗零收 → WAKE
    // 臂（AM-1.5 冻结窗 send PROBE → token 收 WAKE_PENDING 通知 → 解冻）
    // → RESUME/RESUMED+PROBED 握手，全过打 `FROZEN ok n=1` + `WAKE ok
    // n=1`（+ 段 b 尾 `TOMB ok n=1`）；判据载体 = lifecycle 通道〔审修
    // H1〕）----
    tomb_selftest(console, step, mf, icons, views);

    // ---- A-2：bsh 拉起（SDK-1a §1.2 延后段——原主走查 arm 体逐字迁居，
    // data_off/data_len 换绑 off/len，其余零改动）----
    // 交互 shell bsh——stdhello/hexyl 同链（musl target + berry-libc
    // shim）。gift 三~六元：[0] console 别名（spawn 内 dup）/ [1] **endB2
    // = H_BSH_STDIN（被 take 中转，svcmgr 自此不再持有——fd0 = console
    // 键盘行转发对端）** / [2] rootfs VMO READ dup（bsh 的 `run <name>`
    // 流式取 ELF，libc `rootfs_h()` 消费）/ [3] P6 A-1：fs 客户端端（fs_a
    // 的 SEND|RECV dup——`>`/`cat`/`ls` 的 RPC 端，libc `fs_h()`；
    // fs_client==0 = fsmgr 未起）/ [4] A2-3：netd 服务端端（NET_A 的
    // SEND|RECV dup——EVS echo server 自验的 RPC 面，libc `netd_h()`；
    // NET_A==0 = vnet 未起 = EFI/裸拓扑，静默不装）；[7] GUI-4 段②：
    // input 客户端端（INPUT_A dup——inputd 未起静默不装，只随 net 后
    // 入列守槽序）。栈 16 页（无 clap
    // 链）。`BSH ok` 由 bsh 自打（自驱自验：内部 `run hexyl` 收齐
    // 2156B——rootfs 相第 8 标记）；prompt/交互为化妆面。此刻拉起 =
    // app 批已收口，bsh 恢复 fs_a 队列唯一读者（下方 Ok 面关 fs_a_h）。
    if let Some((off, len)) = bsh_geom {
        let mut rootfs_dup = 0 as Handle;
        if berry_handle_duplicate(H_ROOTFS, RIGHT_READ, &mut rootfs_dup) != 0 {
            trap();
        }
        // A2-3 第五 extras：netd 服务端端。NET_A==0（vnet 未起 = EFI/裸
        // 拓扑）静默不装——bsh 侧 netd_h()==0 跳过同口径。槽序固定：netd
        // 只随 fs 之后入列（libc 按位落种 hs[6]→netd——前缀紧致会让 netd
        // 落进 fs 槽 hs[5] 造成同槽不同名；fs 缺席而 vnet 在位时 netd 一并
        // 让位——GUI-8 段② R1 判别器后此形 = **交互相专属**（BLK_OK≠0
        // 才让位 fs：冻结相 bsh 三元皮现状零漂；交互相 bsh 退 [rootfs] 单
        // 皮在 fs_selftest 客户端侧 park，新相自订契约）。
        let mut netd_dup = 0 as Handle;
        let net = NET_A.load(Ordering::Relaxed);
        if fs_client != 0 && net != 0 {
            if berry_handle_duplicate(net, RIGHT_SEND | RIGHT_RECV, &mut netd_dup) != 0 {
                trap();
            }
        }
        // GUI-4 段②第六 extras：input 客户端端。INPUT_A==0（inputd 未起 =
        // 裸/EFI/B4 拓扑）或 netd 让位（netd_dup==0）时静默不装——bsh 侧
        // input_h()==0 跳过同口径。槽序固定（netd 条款镜像）：input 只随
        // netd 之后入列（libc 按位落种 hs[7]→input——前缀紧致会让 input
        // 落进 netd 槽 hs[6] 造成同槽不同名；netd 缺席而 inputd 在位的
        // 理论拓扑里 input 一并让位，现无此 recipe）。
        let mut input_dup = 0 as Handle;
        let inp = INPUT_A.load(Ordering::Relaxed);
        if inp != 0 && netd_dup != 0 {
            if berry_handle_duplicate(inp, RIGHT_SEND | RIGHT_RECV, &mut input_dup) != 0 {
                trap();
            }
        }
        let mut gifts = [rootfs_dup, 0 as Handle, 0 as Handle, 0 as Handle];
        // GUI-8 段② 让位路由（R1 判别器 v2——BLK_OK）：交互相（数据盘在
        // 位）fs_client 已随 launcher extras take 迁入（arm 归零），此门走
        // else = [rootfs] 单 gift；冻结相/rootfs 形 fs 未让位 → 四元/三元
        // 满配现状零漂（同一门三相）。
        let gifts: &[Handle] = if fs_client != 0 {
            gifts[1] = fs_client;
            if netd_dup != 0 {
                gifts[2] = netd_dup;
                if input_dup != 0 {
                    gifts[3] = input_dup;
                    &gifts[..4]
                } else {
                    &gifts[..3]
                }
            } else {
                &gifts[..2]
            }
        } else {
            &gifts[..1]
        };
        if spawn_rootfs_elf(
            H_ROOTFS,
            &SpawnSpec {
                name: b"bin/bsh",
                data_off: off,
                data_len: len,
                stdout: console,
                stack_pages: 16,
                stdin_feed: None,
                stdin_handle: Some(H_BSH_STDIN),
                args: &[],
                extra_gifts: gifts,
                mmio_vmo: None,
                mmio_va: 0,
                intent: 0,
                force_eager: true,
            },
        )
        .is_err()
        {
            // Err 三路（首窗读/elf_head/几何前置）全在 env send 前——
            // rootfs_dup/fs_client/netd_dup/input_dup 均未被 take，显式回
            // 收防表槽泄漏（Ok 路径已被 take，关旧值必 -EBADF，勿在此关）。
            // fs_a_h 不在此关（走查尾兜底已被 bsh_geom 门挡，留表 = 有界 1 槽）。
            if berry_handle_close(rootfs_dup) != 0 {
                trap();
            }
            if fs_client != 0 && berry_handle_close(fs_client) != 0 {
                trap();
            }
            if netd_dup != 0 && berry_handle_close(netd_dup) != 0 {
                trap();
            }
            if input_dup != 0 && berry_handle_close(input_dup) != 0 {
                trap();
            }
            ps_reg(b"bsh", psp::STATE_SPAWNFAIL, 0);
            eco_skip(b"elf bad");
        } else {
            ps_reg(b"bsh", psp::STATE_ALIVE, 0);
            // Ok：fs_client/netd_dup/input_dup 已被 take 迁移（旧值必 -EBADF，勿关）；
            // 本表 fs_a 用毕关——bsh 的 dup 与本表别名共享同一 ObjRef 队列，
            // 关别名只减引用（队列由 bsh 侧撑着），bsh 自此是 fs_a 队列唯一
            // 读者（防回复混流）。NET_A 是本表常驻别名（vnet 复用，ps/bc 等
            // 后续消费面），不随 bsh 关闭。fs_client/fs_a_h 的旧「置 0 收尾」
            // 随 §1.2 迁居成为死写（本块即函数末语）——删。
            // GUI-8 段②：GUI 形（LAUNCHER_LIVE≠0）fs_a_h 让位 FS_A 常驻
            // 别名（NET_A 先例——app 域 serve fs 授能 + 监督重启源，不随
            // bsh 关）；rootfs 形照旧关。
            if fs_a_h != 0
                && LAUNCHER_LIVE.load(Ordering::Relaxed) == 0
                && berry_handle_close(fs_a_h) != 0
            {
                trap();
            }
        }
    }
    // X-6 观测行（boot 尾点）：走查 + app 批毕——尾窗（simbattery/vprobe/
    // app 批拉页）至此收口。恰 {hello, hexyl, 尾} 三点对齐 bench 冻结词形
    // 契约（stdhello/U1 在首行前 = 黑暗基线不发）；walk 早退降级路径不经
    // 此（该 boot 本已 FAIL，行数即证据）。
    x6_mark(console);
}

// ---- VER（ver-hard 乙侧）：版本身份对拍——kernel sysver VMO（第 9 插
// H_VER，fb GOP 相）× mkcpio .berryver 烙印（rootfs 生成式条目），三元组
// （ident/level/semver）双源对拍 → `eco: VER ok/mix` 观测行（非标记族）----

/// VER 探针窗长（= sysver.rs 字符串段帽 STRS_CAP=128：头 20B + 字符串段
/// 合同全覆盖；读 128B 是对拍面惯读宽度）。
const VER_WIN_LEN: usize = 128;
/// VER 探针窗（H_VER 前 128B——单页栈纪律：窗口走 static，STEP_WIN 同款）。
static mut VER_WIN: [u8; VER_WIN_LEN] = [0; VER_WIN_LEN];
/// .berryver 内容窗（"<ident>|level <N>|v<semver>"——ident 帽同 VMO 面
/// 字符串段 + 尾两段约 35B 绰余；盖不住 = 解析拒〔诚实拒，cpio 步进窗
/// 同口径〕）。
const VER_BVER_LEN: usize = 160;
static mut VER_BVER_WIN: [u8; VER_BVER_LEN] = [0; VER_BVER_LEN];
/// VER eco 行窗（mix 形最坏 = 双 ident + 双 lvl/semver 片段；256B 内现实
/// ident（describe ~25B）全宽容纳，理论极长 ident 截断——宁截断不炸机，
/// eco_note 同款）。
const VER_LINE_LEN: usize = 256;
static mut VER_LINE_WIN: [u8; VER_LINE_LEN] = [0; VER_LINE_LEN];

/// VMO 面 128B 解析——kernel/src/sysver.rs `parse` 同源口径逐字段镜像：
/// magic u32@0 = 0x4256_4552 / hdr u16@4 = 1 / rsv@6 零 / level u32@8 /
/// semver u16×3@12 / @20 起 ident、rustc 两段 NUL 连续（ident 段 UTF-8
/// 校验同镜像；rustc 段只验在席——对拍三元组不含它）。任一不符 → None。
/// 与 sysver::parse 的窗缘差异：那边缓冲恒页宽（r_off 必在窗内），这边
/// 窗 = 帽本身 → rustc 段 NUL 出窗显式拒（STRS_LEN ≤ 128 合同下不可达，
/// 窗缘防御——§A145 族「字面/边界即雷」面）。
fn ver_parse_vmo(buf: &[u8]) -> Option<(&[u8], u32, (u16, u16, u16))> {
    const HDR: usize = 20;
    if buf.len() < HDR {
        return None;
    }
    if u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) != 0x4256_4552 {
        return None;
    }
    if u16::from_le_bytes([buf[4], buf[5]]) != 1 {
        return None;
    }
    if buf[6] != 0 || buf[7] != 0 {
        return None; // rsv 须零
    }
    let level = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let semver = (
        u16::from_le_bytes([buf[12], buf[13]]),
        u16::from_le_bytes([buf[14], buf[15]]),
        u16::from_le_bytes([buf[16], buf[17]]),
    );
    let ident_end = HDR + buf[HDR..].iter().position(|&b| b == 0)?;
    let ident = core::str::from_utf8(&buf[HDR..ident_end]).ok()?;
    let r_off = ident_end + 1;
    if r_off >= buf.len() {
        return None; // rustc 段 NUL 出窗（128B 帽截断）
    }
    let r_end = r_off + buf[r_off..].iter().position(|&b| b == 0)?;
    if core::str::from_utf8(&buf[r_off..r_end]).is_err() {
        return None;
    }
    Some((ident.as_bytes(), level, semver))
}

/// 无符号十进制（u32 帽；空串/非数字/溢出 = None——.berryver 段面严格
/// 口径）。
fn ver_dec_u32(s: &[u8]) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let mut v: u32 = 0;
    for &b in s {
        if !b.is_ascii_digit() {
            return None;
        }
        v = v.checked_mul(10)?.checked_add((b - b'0') as u32)?;
    }
    Some(v)
}

/// .berryver 内容解析："<git-describe>|level <N>|v<maj>.<min>.<patch>"
/// 单行（mkcpio 烙印合同无尾随换行；尾随 \n 容忍 = 防御面，对冻结产链
/// 零观感差）→ (ident, level, semver)。ident 空串/段缺席/数字溢出/
/// semver 分量超 u16 = None。
fn ver_parse_berryver(buf: &[u8]) -> Option<(&[u8], u32, (u16, u16, u16))> {
    let mut s = buf;
    while s.last() == Some(&b'\n') {
        s = &s[..s.len() - 1];
    }
    let p1 = s.iter().position(|&b| b == b'|')?;
    let ident = &s[..p1];
    if ident.is_empty() {
        return None;
    }
    let s = s[p1 + 1..].strip_prefix(b"level ")?;
    let p2 = s.iter().position(|&b| b == b'|')?;
    let level = ver_dec_u32(&s[..p2])?;
    let s = s[p2 + 1..].strip_prefix(b"v")?;
    let d1 = s.iter().position(|&b| b == b'.')?;
    let maj = ver_dec_u32(&s[..d1])?;
    let s = &s[d1 + 1..];
    let d2 = s.iter().position(|&b| b == b'.')?;
    let min = ver_dec_u32(&s[..d2])?;
    let pat = ver_dec_u32(&s[d2 + 1..])?;
    if maj > u16::MAX as u32 || min > u16::MAX as u32 || pat > u16::MAX as u32 {
        return None;
    }
    Some((ident, level, (maj as u16, min as u16, pat as u16)))
}

/// 行内追加片段（容量执法：窗恒留 1B \n 槽，超宽截断——「宁截断不炸
/// 机」eco_note 同款）。§A145：cursor 派生长度、字面经 `b"…"` 整体入参
/// ——零手数定长界。
fn ver_push(line: &mut [u8], i: &mut usize, frag: &[u8]) {
    let n = frag.len().min(line.len() - 1 - *i);
    line[*i..*i + n].copy_from_slice(&frag[..n]);
    *i += n;
}

/// 行内追加无符号十进制（手排数字节——thr exit/tomb susp 同款；u32
/// ≤ 10 位，逐字节 ver_push 无独立界）。
fn ver_push_dec(line: &mut [u8], i: &mut usize, v: u32) {
    let mut dig = [0u8; 10];
    let mut dn = 0usize;
    let mut x = v;
    loop {
        dig[dn] = b'0' + (x % 10) as u8;
        dn += 1;
        x /= 10;
        if x == 0 {
            break;
        }
    }
    while dn > 0 {
        dn -= 1;
        ver_push(line, i, &dig[dn..dn + 1]);
    }
}

/// VER 对拍主体（fb GOP 相门内调用——run_rootfs_child 装载块；门形 =
/// H_PRESSURE 块同款）。结构链断全 trap：vmo_read 拒 = 第 9 插接线漂移
/// （H_PRESSURE 探针同形）；VMO 解析败 = 内核侧合同违约（ktest 同源
/// parse 面已拦）；.berryver 缺席/读败/解析败 = mkcpio 烙印合同违约
/// （生成式条目无条件在档）。零本段自有句柄/映射——H_VER/H_ROOTFS 皆
/// boot 插入原值恒持不关（H_FB 同款 v1 政策），错误清理 = 链断 trap
/// 口径即全部。
fn ver_check(console: Handle) {
    // SAFETY: boot 后本进程独占三窗；顺序读写，单线程（STEP_WIN 同口径）。
    let vwin: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(VER_WIN) as *mut u8, VER_WIN_LEN)
    };
    if berry_vmo_read(H_VER, vwin.as_mut_ptr(), 0, VER_WIN_LEN) != 0 {
        trap();
    }
    let Some((vident, vlevel, vsem)) = ver_parse_vmo(vwin) else {
        trap();
    };
    let Some((off, len)) = cpio_find(H_ROOTFS, b".berryver") else {
        trap();
    };
    let bwin: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(VER_BVER_WIN) as *mut u8,
            VER_BVER_LEN,
        )
    };
    let want = (len as usize).min(VER_BVER_LEN);
    if want == 0 || berry_vmo_read(H_ROOTFS, bwin.as_mut_ptr(), off, want) != 0 {
        trap();
    }
    let Some((bident, blevel, bsem)) = ver_parse_berryver(&bwin[..want]) else {
        trap();
    };
    let line: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(VER_LINE_WIN) as *mut u8,
            VER_LINE_LEN,
        )
    };
    let mut i = 0usize;
    if vident == bident && vlevel == blevel && vsem == bsem {
        // 全等：`eco: VER ok <ident> lvl <N>`（手排行——数字节末段）。
        ver_push(line, &mut i, b"eco: VER ok ");
        ver_push(line, &mut i, vident);
        ver_push(line, &mut i, b" lvl ");
        ver_push_dec(line, &mut i, vlevel);
    } else {
        // 任一不等：`eco: VER mix` loud 行含双值片段（vmo=/cpio= 两全宽
        // 三元组——肉眼可判哪段漂）。
        ver_push(line, &mut i, b"eco: VER mix vmo=");
        ver_push(line, &mut i, vident);
        ver_push(line, &mut i, b" lvl ");
        ver_push_dec(line, &mut i, vlevel);
        ver_push(line, &mut i, b" v");
        ver_push_dec(line, &mut i, vsem.0 as u32);
        ver_push(line, &mut i, b".");
        ver_push_dec(line, &mut i, vsem.1 as u32);
        ver_push(line, &mut i, b".");
        ver_push_dec(line, &mut i, vsem.2 as u32);
        ver_push(line, &mut i, b" cpio=");
        ver_push(line, &mut i, bident);
        ver_push(line, &mut i, b" lvl ");
        ver_push_dec(line, &mut i, blevel);
        ver_push(line, &mut i, b" v");
        ver_push_dec(line, &mut i, bsem.0 as u32);
        ver_push(line, &mut i, b".");
        ver_push_dec(line, &mut i, bsem.1 as u32);
        ver_push(line, &mut i, b".");
        ver_push_dec(line, &mut i, bsem.2 as u32);
    }
    line[i] = b'\n';
    send_retry(console, line.as_ptr(), i + 1);
}

// ---- C-3 段①：ps 查询 RPC——拉起账本（表驱动真值源）+ ps_selftest ----

/// ps 输出收集窗长（GUI-8 裁决五③ 随 18 帽抬：18 行 × 最坏 ~29B 渲染 ≈
/// 522B > 旧 512B——溢出即 ps 行截断；640B 帽留余。零可观测漂移：窗只
/// 是容量，行数由账本/协议决定）。
const PS_WIN_LEN: usize = 640;
/// ps 输出收集窗（18 行 × 最坏 ~29B 渲染 ≈ 522B；640B 帽）。
static mut PS_WIN: [u8; PS_WIN_LEN] = [0; PS_WIN_LEN];

/// 账本行（name 内联定宽——no_std 零分配；name_len=0 = 空槽）。
#[derive(Clone, Copy)]
struct PsRec {
    name: [u8; psp::NAME_MAX],
    name_len: u8,
    state: u8,
    code: i32,
}

impl PsRec {
    const EMPTY: PsRec = PsRec { name: [0; psp::NAME_MAX], name_len: 0, state: 0, code: 0 };
}

/// 表容 = psp::MAX_ENTRIES（协议 REPLY_MAX 帽的表侧对偶——扩容先抬协议）。
const PS_MAX: usize = psp::MAX_ENTRIES;
static mut PS_TAB: [PsRec; PS_MAX] = [PsRec::EMPTY; PS_MAX];
static mut PS_N: usize = 0;

/// 名定位（已注册 → 该槽；新名 → 追加；表满 → None）。幂等重名改写用。
fn ps_find_slot(name: &[u8]) -> Option<usize> {
    // SAFETY: svcmgr 单线程独占（PS_TAB/PS_N，HEX_WIN 同纪律）。
    let tab = unsafe { &mut *core::ptr::addr_of_mut!(PS_TAB) };
    let n = unsafe { &mut *core::ptr::addr_of_mut!(PS_N) };
    for i in 0..*n {
        if &tab[i].name[..tab[i].name_len as usize] == name {
            return Some(i);
        }
    }
    if *n < PS_MAX {
        *n += 1;
        Some(*n - 1)
    } else {
        None
    }
}

/// 账本登记（拉起/终态统一入口；重名幂等改写）。名域拒绝面与
/// psp::ReplyBuilder::push 同款（空/超宽/内嵌 NUL 静默丢行；表满同——
/// ps_selftest 名单断言缺失会 FAIL loud 兜底）。
fn ps_reg(name: &[u8], state: u8, code: i32) {
    if name.is_empty() || name.len() > psp::NAME_MAX || name.contains(&0) {
        return;
    }
    let Some(i) = ps_find_slot(name) else {
        return;
    };
    // SAFETY: 同 ps_find_slot；i < PS_N ≤ PS_MAX 由 ps_find_slot 保证。
    unsafe {
        let r = &mut PS_TAB[i];
        r.name = [0; psp::NAME_MAX];
        r.name[..name.len()].copy_from_slice(name);
        r.name_len = name.len() as u8;
        r.state = state;
        r.code = code;
    }
}

/// 终态改写（spawn 后退出——名不在表即登记被拒/被丢，重登记为 exited 同样
/// 无害：表满时再次丢弃，口径一致）。
fn ps_exit(name: &[u8], code: i32) {
    ps_reg(name, psp::STATE_EXITED, code);
}

/// C-3 段①自验（`PS ok n=` = rootfs 相新标记）：ps bin = svcmgr 拉起账本
/// 的渲染面——预建 (ps_a, ps_b) 查询对，ps_b 经 gift[2] 迁入（libc
/// `gift2_h()`，extra_gifts 单元素恰落位 2）→ spawn → 收 1B 请求 → 账本
/// 全表编码应答（畸形请求/带 handle → [ST_BAD,0]，ps 侧 bad reply 退 1
/// loud）→ 私有 stdout 收齐渲染行 → 家名单断言（常驻拓扑真值，非验收
/// 流水——U1/uu_run 瞬态子不入账）→ 打标记。`bin/ps` 缺席（裸跑档/旧
/// rootfs）静默跳过零扰动；链内坏面 = eco_skip 可见降级 + 标记缺失
/// （smoke FAIL 兜底）。
fn ps_selftest(console: Handle) {
    let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/ps") else {
        return;
    };
    // run_rootfs_child 的 eco_skip 同款（模块级函数看不到其局部闭包——
    // 复刻保持降级行形状零漂移：`eco: <why>\n`）。
    let eco_skip = |why: &[u8]| {
        // 诊断行超长安全截断（no_std panic=halt，宁截断不炸机）。
        let m = why.len().min(18);
        let mut line = [0u8; 24];
        line[..4].copy_from_slice(b"eco:");
        line[4] = b' ';
        let n = 5 + m;
        line[5..n].copy_from_slice(&why[..m]);
        line[n] = b'\n';
        send_retry(console, line.as_ptr(), n + 1);
    };
    let (ps_a, ps_b) = channel_pair();
    let (out_s, out_c) = channel_pair();
    // 自身入账（应答编码前——查询时点恒 alive 真值）。
    ps_reg(b"ps", psp::STATE_ALIVE, 0);
    let child = match spawn_rootfs_elf(
        H_ROOTFS,
        &SpawnSpec {
            name: b"bin/ps",
            data_off,
            data_len,
            stdout: out_c,
            stack_pages: 16, // bsh 同档 std 小程序（无 clap 链）
            stdin_feed: Some(&[]), // gift[1] 占位（立即 EOF；uu_run 同款）
            stdin_handle: None,
            args: &[],
            extra_gifts: &[ps_b], // 单元素 → env 块 extras[0]（gift[4] 位）
            mmio_vmo: None,
            mmio_va: 0,
            intent: 0,
            force_eager: false,
        },
    ) {
        Ok(c) => c,
        Err(()) => {
            // Err 全在 env send 前——ps_b 未被 take，显式回收（uu_run 同款）。
            if berry_handle_close(ps_b) != 0
                || berry_handle_close(ps_a) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            eco_skip(b"ps spawn");
            return;
        }
    };
    // ① 收请求（严格串行单请求；富余 handle 收口丢弃）。帽 60s——svcmgr
    // 顺序走到此处才服务，慢而活着不误杀（U1_EOF_CAP 同档；S-2b §5.4 ×2）。
    const PS_CAP_NS: u64 = 60 * 1_000_000_000;
    let mut msg = [0u8; 256];
    let mut hh = [0 as Handle; 6];
    let mut nb = msg.len();
    let mut nh = hh.len();
    let req_ok = loop {
        let s = berry_channel_recv(ps_a, msg.as_mut_ptr(), &mut nb, hh.as_mut_ptr(), &mut nh);
        if s == 0 {
            for i in 0..nh {
                let _ = berry_handle_close(hh[i]);
            }
            // 分派面：本自验拉起的是 `ps`——仅 PS_LIST 在册（berryctl 面
            // 走 bc_selftest；带 handle 随行 = 野值拒）。
            break psp::parse_request(&msg, nb) == Some(psp::OP_PS_LIST) && nh == 0;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ps_a, SIGNAL_READABLE, now() + PS_CAP_NS, &mut obs) != 0 {
            let m = b"ps: no req\n";
            send_retry(console, m.as_ptr(), m.len());
            let _ = child.reap();
            if berry_handle_close(ps_a) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            return;
        }
    };
    // ② 应答：请求合法 → 账本全表编码（`encode_table_reply`——push 拒 =
    // 内部超编，协议/表帽双端共识下理论不可达，仍 loud 降级）；否则
    // [ST_BAD, 0]。
    let mut rep = [0u8; 256];
    let rl = if req_ok {
        match encode_table_reply(&mut rep) {
            Some(l) => l,
            None => {
                eco_skip(b"ps table");
                bad_reply(&mut rep)
            }
        }
    } else {
        bad_reply(&mut rep)
    };
    send_retry(ps_a, rep.as_ptr(), rl);
    // ③ 渲染行收齐（EOF = libc exit 族零长消息，d-3b；帽同 60s）。
    // SAFETY: svcmgr 单线程独占（PS_WIN，HEX_WIN 同纪律）。
    let cap: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(PS_WIN) as *mut u8, PS_WIN_LEN)
    };
    let mut got = 0usize;
    let mut eof = false;
    while !eof {
        let mut m2 = [0u8; 256];
        let mut h2 = [0 as Handle; 6];
        let mut nb2 = m2.len();
        let mut nh2 = h2.len();
        let s = berry_channel_recv(out_s, m2.as_mut_ptr(), &mut nb2, h2.as_mut_ptr(), &mut nh2);
        if s == 0 {
            for i in 0..nh2 {
                let _ = berry_handle_close(h2[i]);
            }
            if nb2 == 0 {
                eof = true;
            } else {
                if got + nb2 > cap.len() {
                    let m = b"ps: overflow\n";
                    send_retry(console, m.as_ptr(), m.len());
                    let _ = child.reap();
                    if berry_handle_close(ps_a) != 0
                        || berry_handle_close(out_c) != 0
                        || berry_handle_close(out_s) != 0
                    {
                        trap();
                    }
                    return;
                }
                cap[got..got + nb2].copy_from_slice(&m2[..nb2]);
                got += nb2;
            }
            continue;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(out_s, SIGNAL_READABLE, now() + PS_CAP_NS, &mut obs) != 0 {
            let m = b"ps: no EOF\n";
            send_retry(console, m.as_ptr(), m.len());
            let _ = child.reap();
            if berry_handle_close(ps_a) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            return;
        }
    }
    // ④ 终验回收：真码必 0（ps 任何坏面都 eprintln+exit 1）。
    let code = match child.reap() {
        Ok(c) => c,
        Err(()) => {
            eco_skip(b"ps reap");
            if berry_handle_close(ps_a) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            return;
        }
    };
    if berry_handle_close(ps_a) != 0 || berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        trap();
    }
    if code != 0 {
        eco_skip(b"ps code");
        return;
    }
    // 账本真值：查询工具自身行迁终态（stdhello/app 批 ps_exit 同款；⑤
    // 断言读的是渲染窗快照，不受影响——后续 berryctl `ps` 子命令见到
    // 的即 exited(0)）。
    ps_exit(b"ps", 0);
    // ⑤ 家名单断言：常驻核心行无条件 + stdhello 退出行（全路径确定性——
    // 预拉段阻塞 reap 先于本函数）+ vblk/vnet 以 H_MMIO 探测门控
    // （EFI/smoke-image 路无 virtio、驱动不拉起——无条件断言会结构性
    // FAIL）。逐行子串（has_sub，相位稳健），非全表字节比对。
    let mut ok = has_sub(&cap[..got], b"svcmgr alive\n")
        && has_sub(&cap[..got], b"echo alive\n")
        && has_sub(&cap[..got], b"talker alive\n")
        && has_sub(&cap[..got], b"echo2 alive\n")
        && has_sub(&cap[..got], b"hello alive\n")
        && has_sub(&cap[..got], b"fsmgr alive\n")
        && has_sub(&cap[..got], b"bsh alive\n")
        && has_sub(&cap[..got], b"ps alive\n")
        && has_sub(&cap[..got], b"stdhello exited(0)\n");
    let mut msz = 0u64;
    if berry_vmo_get_size(H_MMIO, &mut msz) == 0 {
        ok = ok
            && has_sub(&cap[..got], b"vblk alive\n")
            && has_sub(&cap[..got], b"vnet alive\n")
            // GUI-4 段②：gui-inputd 预拉同 H_MMIO 门（在席即 alive 行必在）。
            && has_sub(&cap[..got], b"inputd alive\n");
    }
    if !ok {
        eco_skip(b"ps roster");
        return;
    }
    // ⑥ 标记：n = 窗内行数（≤ PS_MAX=18——GUI-8 裁决五抬帽随改 → 恒 2 位
    //    十进制零填充）。
    let n = cap[..got].iter().filter(|&&b| b == b'\n').count();
    const _: () = assert!(PS_MAX < 100, "PS ok n= 2 位形态");
    let mut line = [0u8; 11];
    line[..8].copy_from_slice(b"PS ok n=");
    line[8] = b'0' + (n / 10) as u8;
    line[9] = b'0' + (n % 10) as u8;
    line[10] = b'\n';
    send_retry(console, line.as_ptr(), line.len());
}

// ---- C-3 段②：berryctl v0——ver/ps/mem 三子命令自验（`BC ok`） ----

/// eco 降级行（run_rootfs_child/ps_selftest 的 `eco: <why>\n` 同形状
/// 零漂移——bc 族抽出模块级共用）。
fn eco_note(console: Handle, why: &[u8]) {
    // 诊断行超长安全截断（no_std panic=halt，宁截断不炸机）。
    let m = why.len().min(18);
    let mut line = [0u8; 24];
    line[..4].copy_from_slice(b"eco:");
    line[4] = b' ';
    let n = 5 + m;
    line[5..n].copy_from_slice(&why[..m]);
    line[n] = b'\n';
    send_retry(console, line.as_ptr(), n + 1);
}

/// bc ver 应答串（svcmgr 编译期标识——env! 取本 crate 版本；系统级版本
/// 源 v2 随发布面定）。≤ psp::VER_MAX 由 encode_ver_reply 拒面执法。
const BC_VER: &[u8] = concat!("BerryOS/svcmgr ", env!("CARGO_PKG_VERSION")).as_bytes();
const _: () = assert!(BC_VER.len() <= psp::VER_MAX, "BC_VER 超 psproto VER_MAX");

/// 畸形应答帧（2B [ST_BAD,0]——三 op 客户端解析面各自拒）。
fn bad_reply(rep: &mut [u8]) -> usize {
    rep[0] = psp::ST_BAD;
    rep[1] = 0;
    2
}

/// 账本全表 → 表应答（PS_LIST 服务面；ps_selftest 与 berryctl `ps` 子
/// 命令共用——「不长第二套」）。push 拒 = 内部超编（协议/表帽双端共识下
/// 理论不可达）→ None，调用方降级 ST_BAD。
fn encode_table_reply(buf: &mut [u8]) -> Option<usize> {
    let mut b = psp::ReplyBuilder::new(buf);
    // SAFETY: svcmgr 单线程独占（PS_TAB/PS_N，HEX_WIN 同纪律）。
    let n = unsafe { core::ptr::read(core::ptr::addr_of!(PS_N)) };
    for i in 0..n {
        // SAFETY: 同上（Copy 整行读出，不经引用跨 push 调用）。
        let r = unsafe { core::ptr::read(core::ptr::addr_of!(PS_TAB[i])) };
        if !b.push(&r.name[..r.name_len as usize], r.state, r.code) {
            return None;
        }
    }
    Some(b.finish())
}

/// mem 应答真值：svcmgr 经既有 vmo_get_size 计数自己可见的 VMO（rootfs
/// blob / MMIO 窗——H_MMIO 无 virtio 路缺席自然少一）。零新内核面
/// （C-3 钦定：内核遥测读不新增，telemetry_open 留 v3 挂账不动）。
fn mem_tally() -> (u8, u64) {
    let (mut n, mut tot) = (0u8, 0u64);
    let mut sz = 0u64;
    if berry_vmo_get_size(H_ROOTFS, &mut sz) == 0 {
        n += 1;
        tot += sz;
    }
    if berry_vmo_get_size(H_MMIO, &mut sz) == 0 {
        n += 1;
        tot += sz;
    }
    (n, tot)
}

/// berryctl 单子命令一轮：spawn（argv=子命令，gift[2]=查询端）→ 服务
/// 一笔查询 RPC（op 分派：VER/MEM/表）→ 渲染行收齐 → reap。返窗内字节
/// 数；任一坏面 eco 降级行 + None（标记缺失 = smoke FAIL 兜底）。渲染窗
/// 复用 PS_WIN（ps_selftest 已收口，单线程顺序复用零交叠）。
fn bc_run_sub(console: Handle, sub: &[u8], data_off: u64, data_len: u64) -> Option<usize> {
    let (q_a, q_b) = channel_pair();
    let (out_s, out_c) = channel_pair();
    // 自身入账（幂等——三子命令跑同槽改写，ps 同款待遇：注册于拉起点）。
    ps_reg(b"berryctl", psp::STATE_ALIVE, 0);
    let child = match spawn_rootfs_elf(
        H_ROOTFS,
        &SpawnSpec {
            name: b"bin/berryctl",
            data_off,
            data_len,
            stdout: out_c,
            stack_pages: 16, // std 小程序（ps 同档，无 clap 链）
            stdin_feed: Some(&[]), // gift[1] 占位（立即 EOF；uu_run 同款）
            stdin_handle: None,
            args: &[sub],
            extra_gifts: &[q_b], // 单元素 → env 块 extras[0]（gift[4] 位）
            mmio_vmo: None,
            mmio_va: 0,
            intent: 0,
            force_eager: false,
        },
    ) {
        Ok(c) => c,
        Err(()) => {
            // Err 全在 env send 前——q_b 未被 take，显式回收（uu_run 同款）。
            if berry_handle_close(q_b) != 0
                || berry_handle_close(q_a) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            eco_note(console, b"bc spawn");
            return None;
        }
    };
    // ① 收请求（严格串行单请求；富余 handle 收口丢弃）。帽 60s——svcmgr
    // 顺序走到此处才服务，慢而活着不误杀（PS_CAP_NS 同档）。
    const BC_CAP_NS: u64 = 60 * 1_000_000_000;
    let mut msg = [0u8; 256];
    let mut hh = [0 as Handle; 6];
    let mut nb = msg.len();
    let mut nh = hh.len();
    let op = loop {
        let s = berry_channel_recv(q_a, msg.as_mut_ptr(), &mut nb, hh.as_mut_ptr(), &mut nh);
        if s == 0 {
            for i in 0..nh {
                let _ = berry_handle_close(hh[i]);
            }
            break if nh == 0 { psp::parse_request(&msg, nb) } else { None };
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(q_a, SIGNAL_READABLE, now() + BC_CAP_NS, &mut obs) != 0 {
            let m = b"bc: no req\n";
            send_retry(console, m.as_ptr(), m.len());
            let _ = child.reap();
            if berry_handle_close(q_a) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            return None;
        }
    };
    // ② op 分派应答：VER → 编译期串；MEM → vmo_get_size 计数；LIST →
    // 账本全表；畸形/带 handle → [ST_BAD,0]（客户端各自 bad reply 退 1）。
    let mut rep = [0u8; 256];
    let rl = match op {
        Some(psp::OP_VER) => match psp::encode_ver_reply(&mut rep, BC_VER) {
            Some(l) => l,
            None => bad_reply(&mut rep),
        },
        Some(psp::OP_MEM) => {
            let (vmos, total) = mem_tally();
            psp::encode_mem_reply(&mut rep, vmos, total)
        }
        Some(_) => match encode_table_reply(&mut rep) {
            Some(l) => l,
            None => bad_reply(&mut rep),
        },
        None => bad_reply(&mut rep),
    };
    send_retry(q_a, rep.as_ptr(), rl);
    // ③ 渲染行收齐（EOF = libc exit 族零长消息，d-3b；帽同 60s）。
    // SAFETY: svcmgr 单线程独占（PS_WIN 顺序复用，HEX_WIN 同纪律）。
    let cap: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(PS_WIN) as *mut u8, PS_WIN_LEN)
    };
    let mut got = 0usize;
    let mut eof = false;
    while !eof {
        let mut m2 = [0u8; 256];
        let mut h2 = [0 as Handle; 6];
        let mut nb2 = m2.len();
        let mut nh2 = h2.len();
        let s = berry_channel_recv(out_s, m2.as_mut_ptr(), &mut nb2, h2.as_mut_ptr(), &mut nh2);
        if s == 0 {
            for i in 0..nh2 {
                let _ = berry_handle_close(h2[i]);
            }
            if nb2 == 0 {
                eof = true;
            } else {
                if got + nb2 > cap.len() {
                    let m = b"bc: overflow\n";
                    send_retry(console, m.as_ptr(), m.len());
                    let _ = child.reap();
                    if berry_handle_close(q_a) != 0
                        || berry_handle_close(out_c) != 0
                        || berry_handle_close(out_s) != 0
                    {
                        trap();
                    }
                    return None;
                }
                cap[got..got + nb2].copy_from_slice(&m2[..nb2]);
                got += nb2;
            }
            continue;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(out_s, SIGNAL_READABLE, now() + BC_CAP_NS, &mut obs) != 0 {
            let m = b"bc: no EOF\n";
            send_retry(console, m.as_ptr(), m.len());
            let _ = child.reap();
            if berry_handle_close(q_a) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            return None;
        }
    }
    // ④ 终验回收：真码必 0（berryctl 任何坏面都 eprintln+exit 1）。
    let code = match child.reap() {
        Ok(c) => c,
        Err(()) => {
            eco_note(console, b"bc reap");
            if berry_handle_close(q_a) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            return None;
        }
    };
    if berry_handle_close(q_a) != 0
        || berry_handle_close(out_c) != 0
        || berry_handle_close(out_s) != 0
    {
        trap();
    }
    if code != 0 {
        // 账本真值：终态改写（stdhello/app 批 ps_exit 同款）。
        ps_exit(b"berryctl", code);
        eco_note(console, b"bc code");
        return None;
    }
    ps_exit(b"berryctl", 0);
    Some(got)
}

/// C-3 段②自验（`BC ok` = rootfs 相新标记，注册面随段②收口刀挂
/// smoke.sh）：berryctl 三子命令全跑通——ver（编译期串逐字节）/ mem
/// （形状——bytes 值随 rootfs 内容漂，非定值断言）/ ps（家常驻锚行 +
/// 自身可见）。`bin/berryctl` 缺席（裸跑档/旧 rootfs）静默跳过零扰动；
/// 链内坏面 = eco 降级行 + 标记缺失（smoke FAIL 兜底）。
fn bc_selftest(console: Handle) {
    let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/berryctl") else {
        return;
    };
    // ⓪ ver：编译期期望串（BC_VER 同源拼——env! 编译期展开，双侧同 crate）。
    let want_ver: &[u8] = concat!("ver BerryOS/svcmgr ", env!("CARGO_PKG_VERSION"), "\n").as_bytes();
    let Some(got) = bc_run_sub(console, b"ver", data_off, data_len) else {
        return;
    };
    // SAFETY: 单线程独占（PS_WIN——bc_run_sub 收口后只读）。
    let win: &[u8] = unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(PS_WIN) as *const u8, got) };
    if !has_sub(win, want_ver) {
        eco_note(console, b"bc ver");
        return;
    }
    // ① mem：形状断言（`mem vmos=` 前缀——vmos 1/2 随 virtio 在位漂、
    // bytes 随 rootfs 内容漂；真码 0 + 前缀在即协议面通）。
    let Some(got) = bc_run_sub(console, b"mem", data_off, data_len) else {
        return;
    };
    // SAFETY: 同上。
    let win: &[u8] = unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(PS_WIN) as *const u8, got) };
    if !has_sub(win, b"mem vmos=") {
        eco_note(console, b"bc mem");
        return;
    }
    // ② ps：账本渲染锚行（家常驻 + ps 行已在 ps_selftest 收尾迁终态 +
    // 自身行——本轮查询时点 berryctl 刚 ps_reg 为 ALIVE，真值成立）。
    let Some(got) = bc_run_sub(console, b"ps", data_off, data_len) else {
        return;
    };
    // SAFETY: 同上。
    let win: &[u8] = unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(PS_WIN) as *const u8, got) };
    if !(has_sub(win, b"svcmgr alive\n")
        && has_sub(win, b"ps exited(0)\n")
        && has_sub(win, b"berryctl alive\n"))
    {
        eco_note(console, b"bc ps");
        return;
    }
    // ③ 标记。
    send_retry(console, b"BC ok\n".as_ptr(), 6);
}

// ---- T-1 §2：进程内多线程消费面（`THR ok`） ----

/// thrchild 布局：平像恰 1 页（代码/只读 + .bss 内嵌 2048B 次线程栈共页
/// ——8192 栈版平像 3 页会把 svcmgr 内嵌总量顶破 64KiB `USER_CODE_MAX`，
/// 见 thrchild link-user.ld 头注），映射加宽 [0x1000,0x7000)——余 5 页
/// 零页作主线程栈（spawn_child 的 2 页形态放大版；镜像越不过 0x5000 =
/// elf2flat --max 16384 执法，主栈区最劣仍余 2 页）。主 sp0 = 映射窗顶
/// （CHILD_SP=0x3000 同口径）。
const THR_MAP_LEN: usize = 0x6000;
const THR_ENTRY: usize = 0x1000;
const THR_SP: usize = 0x7000;
/// 期望退出码：T-1/S3-b 基础链 = 7；sim-S2 A2 两轮冻结全毕 = 8；失败 = 9。
const THR_OK_CODE: i32 = 7;
const THR_FRZ2_CODE: i32 = 8;
/// join 应答帽 60s：TCG 下 spawn→跑→收口「慢而活」不误杀（M3DEMO cp 步
/// ≈17s 定标同口径）；超帽 = 降级行 + 标记缺失 = smoke FAIL，不重发。
const THR_CAP_NS: u64 = 60 * 1_000_000_000;
/// T-1 §2 标记（7 字节；rootfs 相第 36 标记，EFI 路 ECO 同面）。
const THR_OK: &[u8; 7] = b"THR ok\n";
/// sim-S2 A2 标记：仅退出码 8（两轮 freeze/thaw 全毕）才打；码7 只打 THR。
const FRZ_OK: &[u8; 11] = b"FRZ ok n=2\n";

/// T-1 §2 自验：拉起 thrchild（进程内 thread_create/thread_start/join/
/// 复启拒面/末线程收口五步链，见其 src/main.rs 头注）——bootstrap 自句柄
/// 是关键形态：process_start 的 bootstrap 迁移面首次承载 **Process 本体**
/// （dup 裁权 MANAGE|DUPLICATE——子内 thread_create(self) 的门控来源），
/// x0 即子表新值。父侧等 TERMINATED → get_exit_code：7 打 `THR ok`，错码
/// 打 `eco: thr exit <n>`（数字形态超 eco_note 18B 帽，单建行）。
fn thr_selftest(console: Handle) {
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    if berry_process_create(0, 0, &mut hp, &mut hv) != 0 || hp == 0 || hv == 0 {
        eco_note(console, b"thr create");
        return;
    }
    let mut vmo = 0 as Handle;
    if berry_vmo_create(THR_MAP_LEN as u64, 0, &mut vmo) != 0 || vmo == 0 {
        eco_note(console, b"thr vmo");
        return;
    }
    if berry_vmo_commit(vmo, 0, THR_MAP_LEN as u64) != 0
        || berry_vmo_write(vmo, THRCHILD_BIN.as_ptr(), 0, THRCHILD_BIN.len()) != 0
    {
        eco_note(console, b"thr vw");
        return;
    }
    let mut vaddr: usize = THR_ENTRY; // 映射基址提示（in-out 槽预写）
    if berry_vmar_map(hv, 0, vmo, 0, &mut vaddr, THR_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0 {
        eco_note(console, b"thr map");
        return;
    }
    let mut ht = 0 as Handle;
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        eco_note(console, b"thr tc");
        return;
    }
    // bootstrap 自句柄：dup 只紧不松，子拿到的是裁权副本（本表 hp 本体不动
    // ——wait/get_exit_code 仍走全权原件）。
    let mut boot: Handle = 0;
    if berry_handle_duplicate(hp, RIGHT_MANAGE | RIGHT_DUPLICATE, &mut boot) != 0 || boot == 0 {
        eco_note(console, b"thr dup");
        return;
    }
    if berry_process_start(hp, ht, THR_ENTRY, THR_SP, boot) != 0 {
        eco_note(console, b"thr start");
        return;
    }
    if berry_handle_close(boot) != BERRY_ERR_BAD_HANDLE {
        trap(); // 迁移自证：原值必须已作废（spawn_child 同款铁证）
    }
    // join：wait 不需权利位；绝对帽 60s。
    let mut obs = 0u64;
    if berry_object_wait_one(hp, SIGNAL_TERMINATED, now() + THR_CAP_NS, &mut obs) != 0
        || obs != SIGNAL_TERMINATED
    {
        eco_note(console, b"thr join");
        return;
    }
    let mut code: i32 = 0;
    if berry_process_get_exit_code(hp, &mut code) != 0 {
        eco_note(console, b"thr code");
        return;
    }
    // 收口回收（berry-exec `ChildHandles::reap` 同序）：**先 vmar_unmap 再关
    // vmo**——映射不产生 refs 别名（vmar_map 只 `track_map` 计 mapcnt），不
    // unmap 就关 vmo = refs==1 且 any_mapped → 销毁性 close 拒 -EBUSY → 本表
    // trap 杀 svcmgr（THR ok 起全下家标记连坐缺席）。unmap 把 mapcnt 归零后
    // close(vmo) 才放行；hp 末引用销毁性 close 合法（子已 TERMINATED+drain）；
    // 顺序先 unmap/vmo/ht/hp 再 hv，Process 内部钉 root Vmar 的释放链自然
    // 收敛）。echo/talker 常驻不收，本子是短命验收件 = berryctl reap 同待遇。
    if berry_vmar_unmap(hv, THR_ENTRY, THR_MAP_LEN) != 0
        || berry_handle_close(vmo) != 0
        || berry_handle_close(ht) != 0
        || berry_handle_close(hp) != 0
        || berry_handle_close(hv) != 0
    {
        trap(); // 已 TERMINATED 的收口面，非 0 = 内核侧异常，loud
    }
    if code == THR_OK_CODE {
        send_retry(console, THR_OK.as_ptr(), THR_OK.len());
        return;
    }
    if code == THR_FRZ2_CODE {
        send_retry(console, THR_OK.as_ptr(), THR_OK.len());
        send_retry(console, FRZ_OK.as_ptr(), FRZ_OK.len());
        return;
    }
    // 错码降级行：`eco: thr exit <n>\n`（十进制带符号，i32 全域 ≤ 26B）。
    let mut line = [0u8; 32];
    line[..14].copy_from_slice(b"eco: thr exit ");
    let mut i = 14usize;
    if code < 0 {
        line[i] = b'-';
        i += 1;
    }
    let mut v = code.unsigned_abs() as u64;
    let mut dig = [0u8; 10];
    let mut dn = 0usize;
    loop {
        dig[dn] = b'0' + (v % 10) as u8;
        dn += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    while dn > 0 {
        dn -= 1;
        line[i] = dig[dn];
        i += 1;
    }
    line[i] = b'\n';
    send_retry(console, line.as_ptr(), i + 1);
}

// ---- G4-2：pager 服务首形消费面（`DPGP ok`，跨进程供给形） ----

/// dpgp 布局（dpgp src/main.rs 头注 / link-user.ld 单源对应，改须双改）：
/// 像窗 [0x1000,0x6000) eager（image 1 页 + 4 页零页，主 sp=0x6000 窗顶，
/// THR_MAP_LEN 同口径缩 1 页）；demand VMO 恰 1 页 @ 0x6000（窗界外——
/// 受验子读它必走缺页路径）。
const DPG_MAP_LEN: usize = 0x5000;
const DPG_ENTRY: usize = 0x1000;
const DPG_SP: usize = 0x6000;
/// demand 页 VA（dpgp 侧 `DPGP_VA` 同值——两文件各持 const，改须双改）。
const DPG_VA: usize = 0x6000;
/// G4-4 X-2：I-abort 腿 demand 代码页 VA（dpgp `DPGP_VA2` 同值双改）。
const DPG_VA2: usize = 0x7000;
/// G4-4 X-2：**I 腿第二入口 VA**（dpgp `dpgi_entry` 段钉 0x2000——
/// link-user.ld 同值双改）。`_start` 主径零漂移 = storm/negative/soak/
/// bench 消费面不变（共享嵌内 bin = 共享行为面——新腿走新入口）。
const DPGI_ENTRY: usize = 0x2000;
/// I 腿供给页字节：`movz w0,#0x5a; ret`（aarch64 手排两指——LE：
/// 0x52800B40 / 0xD65F03C0；余页零填，供给后取指即此函数体；期望返回
/// 0x5a 比对面在 dpgp `DPGP_IFN_RET`——父只认终态 exit 7）。
const DPG_IFN_CODE: [u8; 8] = [0x40, 0x0b, 0x80, 0x52, 0xc0, 0x03, 0x5f, 0xd6];
/// 供给页首 8 字节期望值（"DPGPmagi" LE——dpgp 侧 `DPGP_MAGIC` 同值）。
const DPG_MAGIC: u64 = 0x6967_616d_5047_5044;
/// 期望退出码（三态：两腿全过 → 7 / magic 腿错值 → 9 / I 腿错值 → 11，
/// thrchild 同码面扩一态）。
const DPG_OK_CODE: i32 = 7;
/// join 应答帽 60s（THR_CAP_NS 同口径；0x21 供给步在 wait 之前已收口，
/// 本帽只罩「子读值 → exit → TERMINATED」尾段）。
const DPG_CAP_NS: u64 = 60 * 1_000_000_000;
/// G4-2 标记（8 字节；rootfs 相第 47 标记〔THR(46) 后插随实发序〕，
/// 裸跑/EFI 路同面）。
const DPGP_OK: &[u8; 8] = b"DPGP ok\n";

/// G4-2 自验：**跨进程供给形**（§1.3 exec 装载 demand 化的最小缩影——
/// 装载器〔本服务〕即 pager）：本服务建三 VMO（源 eager 预写 magic /
/// 像 eager / demand `VMO_OPT_DEMAND`），像 + demand 映入子 root Vmar
///（demand 映射即内核 asid 反查登记），`process_start(..., bootstrap=0)`
/// 零句柄拉起 dpgp → **0x21 阻塞收货**（子首触 0x6000 → EL0 探针武装
/// 请求 → 本步收货：源页拷入供给页 → 唤子）→ 子读到 magic → exit 7。
/// 双序天然安全：子先 fault / 本步先睡两序由 poll-first + 位锁存兜底。
/// 供给步非 0（身份验/源读错）= 早退降级——子重 fault 再武装后无人收货
/// = 永久阻塞（§5-⑤ fail loud-on-hang，标记缺席 = smoke FAIL 执法）。
///
/// G4-4 X-2 **I-abort 腿**（§5-⑦ 留面兑现）：子改走**第二入口**
/// `dpgi_start`（0x2000，`_start` 主径零漂移 = storm/negative/soak/
/// bench 消费面不变——共享嵌内 bin 新腿走新入口判例）；再加一对源/
/// demand VMO（0x7000 代码页，源 = 手排 `movz w0,#0x5a; ret`），子读
/// magic（D 腿）后 call 0x7000——首取指走 EL0 I-abort(0x20)（trap.rs
/// 同臂并探针）→ 二供 → eret 重执行 → 返 0x5a。子触序与父供给序同向
///（magic 先/代码后，单子顺序 pin 无交错竞窗）；两腿全过 exit 7
///（`DPGP ok` 不变——0x20 面回归即 DPGP 缺席 FAIL 执法）。
fn dpg_selftest(console: Handle) {
    let _pager_hold = PagerHold::new(); // X-3c-3：本族自供 0x21，boot pager 让位（见 PAGER_HOLD 注）
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    if berry_process_create(0, 0, &mut hp, &mut hv) != 0 || hp == 0 || hv == 0 {
        eco_note(console, b"dpg create");
        return;
    }
    // 源 VMO（防线 4：pre-commit + 预写 magic——供给链零缺页）。
    let mut sv = 0 as Handle;
    if berry_vmo_create(4096, 0, &mut sv) != 0 || sv == 0 {
        eco_note(console, b"dpg sv");
        return;
    }
    if berry_vmo_commit(sv, 0, 4096) != 0
        || berry_vmo_write(sv, DPG_MAGIC.to_le_bytes().as_ptr(), 0, 8) != 0
    {
        eco_note(console, b"dpg svw");
        return;
    }
    // 受验像 VMO（eager 全提交）。
    let mut iv = 0 as Handle;
    if berry_vmo_create(DPG_MAP_LEN as u64, 0, &mut iv) != 0 || iv == 0 {
        eco_note(console, b"dpg iv");
        return;
    }
    if berry_vmo_commit(iv, 0, DPG_MAP_LEN as u64) != 0
        || berry_vmo_write(iv, DPGP_BIN.as_ptr(), 0, DPGP_BIN.len()) != 0
    {
        eco_note(console, b"dpg ivw");
        return;
    }
    // demand VMO（bit0 = VMO_OPT_DEMAND——内核授 SET_POLICY 附加权 = 0x21
    // 门权；首触缺页由 EL0 探针接管）。
    let mut dv = 0 as Handle;
    if berry_vmo_create(4096, VMO_OPT_DEMAND, &mut dv) != 0 || dv == 0 {
        eco_note(console, b"dpg dv");
        return;
    }
    // G4-4 X-2 I 腿：源 VMO（手排 `movz w0,#0x5a; ret` 函数体——防线 4
    // 同款 pre-commit）+ 第二 demand VMO（0x20 I-abort 首取指受验页）。
    let mut sv2 = 0 as Handle;
    if berry_vmo_create(4096, 0, &mut sv2) != 0 || sv2 == 0 {
        eco_note(console, b"dpg sv2");
        return;
    }
    if berry_vmo_commit(sv2, 0, 4096) != 0
        || berry_vmo_write(sv2, DPG_IFN_CODE.as_ptr(), 0, DPG_IFN_CODE.len()) != 0
    {
        eco_note(console, b"dpg sv2w");
        return;
    }
    let mut dv2 = 0 as Handle;
    if berry_vmo_create(4096, VMO_OPT_DEMAND, &mut dv2) != 0 || dv2 == 0 {
        eco_note(console, b"dpg dv2");
        return;
    }
    // 像窗 + demand 页映入子 root Vmar（demand 映射触发内核 asid 反查
    // 登记——探针第一段的供给可达性由此建立）。
    let mut vaddr = DPG_ENTRY;
    if berry_vmar_map(hv, 0, iv, 0, &mut vaddr, DPG_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0 {
        eco_note(console, b"dpg imap");
        return;
    }
    let mut dva = DPG_VA;
    if berry_vmar_map(hv, 0, dv, 0, &mut dva, 4096, RIGHT_READ | RIGHT_WRITE) != 0 {
        eco_note(console, b"dpg dmap");
        return;
    }
    let mut dva2 = DPG_VA2;
    if berry_vmar_map(hv, 0, dv2, 0, &mut dva2, 4096, RIGHT_READ | RIGHT_WRITE) != 0 {
        eco_note(console, b"dpg dmap2");
        return;
    }
    let mut ht = 0 as Handle;
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        eco_note(console, b"dpg tc");
        return;
    }
    // 零句柄形态：bootstrap=0（dpgp 不建线程不碰对象，x0=0）。**X-2 起
    // 本测走第二入口 dpgi_start（0x2000）**——_start 主径留给其余消费面。
    if berry_process_start(hp, ht, DPGI_ENTRY, DPG_SP, 0) != 0 {
        eco_note(console, b"dpg start");
        return;
    }
    // 0x21 阻塞收货：无武装请求即睡（子首触武装后唤醒本步）；供给毕恒
    // 唤子（失败亦唤——子重 fault 再武装，本步非 0 早退即 fail loud）。
    if berry_pager_supply_pages(dv, sv, 0) != 0 {
        eco_note(console, b"dpg sup");
        return;
    }
    // X-2 I 腿二供：子先读 magic（exit 前序）后 call 0x7000——父供给序
    // 与子触序同向（magic 页先、代码页后）；双序安全同上（poll-first +
    // 位锁存）。0x20 I-abort 首取指 → 探针武装 → 本供 → eret 重执行 call。
    if berry_pager_supply_pages(dv2, sv2, 0) != 0 {
        eco_note(console, b"dpg sup2");
        return;
    }
    // 子读到 magic → exit 7 → TERMINATED（0x21 返 0 = 供给页已就位，本等
    // 只罩尾段——60s 宽帽）。
    let mut obs = 0u64;
    if berry_object_wait_one(hp, SIGNAL_TERMINATED, now() + DPG_CAP_NS, &mut obs) != 0
        || obs != SIGNAL_TERMINATED
    {
        eco_note(console, b"dpg join");
        return;
    }
    let mut code: i32 = 0;
    if berry_process_get_exit_code(hp, &mut code) != 0 {
        eco_note(console, b"dpg code");
        return;
    }
    // 收口回收（thr_selftest 同序）：先 unmap 像窗 + demand 页（Demand 区间
    // unmap 同收割、mapcnt 归零）再关 vmo；dv 已供给页随 unmap 解除映射。
    if berry_vmar_unmap(hv, DPG_VA, 4096) != 0
        || berry_vmar_unmap(hv, DPG_VA2, 4096) != 0
        || berry_vmar_unmap(hv, DPG_ENTRY, DPG_MAP_LEN) != 0
        || berry_handle_close(dv) != 0
        || berry_handle_close(sv) != 0
        || berry_handle_close(dv2) != 0
        || berry_handle_close(sv2) != 0
        || berry_handle_close(iv) != 0
        || berry_handle_close(ht) != 0
        || berry_handle_close(hp) != 0
        || berry_handle_close(hv) != 0
    {
        trap(); // 已 TERMINATED 的收口面，非 0 = 内核侧异常，loud
    }
    if code == DPG_OK_CODE {
        send_retry(console, DPGP_OK.as_ptr(), DPGP_OK.len());
        return;
    }
    // 错码降级行：`eco: dpgp exit <n>\n`（thr_selftest 同款手排十进制）。
    let mut line = [0u8; 32];
    line[..15].copy_from_slice(b"eco: dpgp exit ");
    let mut i = 15usize;
    if code < 0 {
        line[i] = b'-';
        i += 1;
    }
    let mut v = code.unsigned_abs() as u64;
    let mut dig = [0u8; 10];
    let mut dn = 0usize;
    loop {
        dig[dn] = b'0' + (v % 10) as u8;
        dn += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    while dn > 0 {
        dn -= 1;
        line[i] = dig[dn];
        i += 1;
    }
    line[i] = b'\n';
    send_retry(console, line.as_ptr(), i + 1);
}

/// G4-3 C-lite 风暴子数（4 子并发首触同一 demand VMO）。
const DPGS_N: usize = 4;
/// G4-3 C-lite 标记（12 字节；rootfs 相第 49 标记〔INPUT(48) 后随实发
/// 序〕，裸跑/EFI 路同面）。带参形态防子串混叠（`BLKD ok n=` 同款惯例——
/// "DPGS ok n=4" 与 "DPGP ok"/"DPG ok" 位 4 S≠P 互非子串）。
const DPGS_OK: &[u8; 12] = b"DPGS ok n=4\n";
/// 标记 n= 与风暴子数同源（APP_SLOTS 单数字位同族——B 刀 #6 补）：改
/// N 忘改标记即编译拒。
const _: () = assert!(DPGS_OK[10] == b'0' + DPGS_N as u8 && DPGS_N < 10);

/// G4-3 C-lite 自验：**缺页风暴自愈 N=4**——共享三 VMO（源/像/demand 各
/// 建一次）× 4 子并发首触同一 demand 页。机制面现况（B-1 键化后）：EL0
/// 请求槽 = K=8 槽池各自认领 + 供给毕**定向**唤本槽（C-lite 时代的单槽
/// 盲覆写 + 全局广播错醒面已随 B 刀退役——本测留作键化路径回归车，4 子
/// < K=8 认领直路径零溢出）；4 轮 0x21 阻塞收货各取一槽恰喂一子（同 dv
/// ⇒ 身份验恒过；异子异表各填各叶，supply_page 双重幂等 = 首 commit 一次
/// + 4 填叶同 PA——共享库页真实形状）。不收敛/丢唤 = 某子永睡 → join
/// 帽到点 → 标记缺席 = smoke FAIL loud（§5-⑤ 执法）；收敛 = 4× join
/// exit 7 within 帽。
fn dpg_storm(console: Handle) {
    let _pager_hold = PagerHold::new(); // X-3c-3：同 dpg_selftest——自供期间 pager 让位
    // 共享源 VMO（防线 4：pre-commit + 预写 magic——供给链零缺页）。
    let mut sv = 0 as Handle;
    if berry_vmo_create(4096, 0, &mut sv) != 0 || sv == 0 {
        eco_note(console, b"dpgs sv");
        return;
    }
    if berry_vmo_commit(sv, 0, 4096) != 0
        || berry_vmo_write(sv, DPG_MAGIC.to_le_bytes().as_ptr(), 0, 8) != 0
    {
        eco_note(console, b"dpgs svw");
        return;
    }
    // 共享受验像 VMO（eager 全提交——4 子同像 dpgp）。
    let mut iv = 0 as Handle;
    if berry_vmo_create(DPG_MAP_LEN as u64, 0, &mut iv) != 0 || iv == 0 {
        eco_note(console, b"dpgs iv");
        return;
    }
    if berry_vmo_commit(iv, 0, DPG_MAP_LEN as u64) != 0
        || berry_vmo_write(iv, DPGP_BIN.as_ptr(), 0, DPGP_BIN.len()) != 0
    {
        eco_note(console, b"dpgs ivw");
        return;
    }
    // 共享 demand VMO（bit0 = VMO_OPT_DEMAND；4 子同映射 = 4 asid 反查
    // 登记 ≤ 16 槽）。
    let mut dv = 0 as Handle;
    if berry_vmo_create(4096, VMO_OPT_DEMAND, &mut dv) != 0 || dv == 0 {
        eco_note(console, b"dpgs dv");
        return;
    }
    // 4 子拉起：process_create + 双映射（像窗 + demand 页）+ 线程 + start
    //（零句柄 bootstrap=0，dpg_selftest 同形 ×4）。
    let mut hps = [0 as Handle; DPGS_N];
    let mut hvs = [0 as Handle; DPGS_N];
    let mut hts = [0 as Handle; DPGS_N];
    for i in 0..DPGS_N {
        if berry_process_create(0, 0, &mut hps[i], &mut hvs[i]) != 0 || hps[i] == 0 || hvs[i] == 0
        {
            eco_note(console, b"dpgs create");
            return;
        }
        let mut vaddr = DPG_ENTRY;
        if berry_vmar_map(hvs[i], 0, iv, 0, &mut vaddr, DPG_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0
        {
            eco_note(console, b"dpgs imap");
            return;
        }
        let mut dva = DPG_VA;
        if berry_vmar_map(hvs[i], 0, dv, 0, &mut dva, 4096, RIGHT_READ | RIGHT_WRITE) != 0 {
            eco_note(console, b"dpgs dmap");
            return;
        }
        if berry_thread_create(hps[i], 0, &mut hts[i]) != 0 || hts[i] == 0 {
            eco_note(console, b"dpgs tc");
            return;
        }
        if berry_process_start(hps[i], hts[i], DPG_ENTRY, DPG_SP, 0) != 0 {
            eco_note(console, b"dpgs start");
            return;
        }
    }
    // 4× 0x21 阻塞收货：poll-first + 睡 pager 键逐轮取一认领槽（键化后
    // 定向唤零错醒——每轮恰喂一子，4 轮收敛 4 叶；供给毕恒唤，失败亦唤
    // ——非 0 早退即 fail loud）。
    for _round in 0..DPGS_N {
        if berry_pager_supply_pages(dv, sv, 0) != 0 {
            eco_note(console, b"dpgs sup");
            return;
        }
    }
    // 4× join exit 7（DPG_CAP_NS 同口径 60s/子帽——供给步已毕，本等只罩
    // 「子读值 → exit → TERMINATED」尾段；丢唤子永睡 = 帽到点标记缺席）。
    for i in 0..DPGS_N {
        let mut obs = 0u64;
        if berry_object_wait_one(hps[i], SIGNAL_TERMINATED, now() + DPG_CAP_NS, &mut obs) != 0
            || obs != SIGNAL_TERMINATED
        {
            eco_note(console, b"dpgs join");
            return;
        }
        let mut code: i32 = 0;
        if berry_process_get_exit_code(hps[i], &mut code) != 0 {
            eco_note(console, b"dpgs code");
            return;
        }
        if code != DPG_OK_CODE {
            eco_note(console, b"dpgs exit");
            return;
        }
    }
    // 收口：每子先 unmap demand 页再 unmap 像窗（4 unmap 齐走 demand_mapped
    // 钉归零）→ 关 ht/hp/hv（Vmar Drop 注销 asid 槽）；最后关共享三 VMO
    //（dv 钉已释，关面不再 -EBUSY）。收口非 0 = 内核侧异常，loud。
    for i in 0..DPGS_N {
        if berry_vmar_unmap(hvs[i], DPG_VA, 4096) != 0
            || berry_vmar_unmap(hvs[i], DPG_ENTRY, DPG_MAP_LEN) != 0
            || berry_handle_close(hts[i]) != 0
            || berry_handle_close(hps[i]) != 0
            || berry_handle_close(hvs[i]) != 0
        {
            trap();
        }
    }
    if berry_handle_close(dv) != 0
        || berry_handle_close(sv) != 0
        || berry_handle_close(iv) != 0
    {
        trap();
    }
    send_retry(console, DPGS_OK.as_ptr(), DPGS_OK.len());
}

/// G4-3 B 刀负序子数（四早退各一断言——n= 与标记同源，const-assert 钉）。
const DPGN_N: usize = 4;
/// G4-3 B 刀负序标记（12 字节；rootfs 相第 50 标记〔DPGS(49) 后随实发
/// 序〕，裸跑/EFI 路同面）。"DPGN ok n=4" 与现役标记两两互非子串
///（"DPGS ok n=4"/"DPGP ok" 位 4 N≠S/P；"DPG ok" 位 3 后 N≠空格；
/// "B1 ok n=4" 前缀异）。
const DPGN_OK: &[u8; 12] = b"DPGN ok n=4\n";
/// 标记 n= 与子数同源（APP_SLOTS 单数字位同族）：改 N 忘改标记即编译拒。
const _: () = assert!(DPGN_OK[10] == b'0' + DPGN_N as u8 && DPGN_N < 10);

/// dpgn 子拉起小件（dpg_storm 单子同形：像 + demand 双映射 + 零句柄
/// start；败 = None——调用方降级〔已建柄不追清，fail-loud 先例：标记
/// 缺席 = smoke FAIL 执法〕）。
fn dpgn_spawn(iv: Handle, dv: Handle) -> Option<(Handle, Handle, Handle)> {
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    let mut ht = 0 as Handle;
    if berry_process_create(0, 0, &mut hp, &mut hv) != 0 || hp == 0 || hv == 0 {
        return None;
    }
    let mut vaddr = DPG_ENTRY;
    if berry_vmar_map(hv, 0, iv, 0, &mut vaddr, DPG_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0 {
        return None;
    }
    let mut dva = DPG_VA;
    if berry_vmar_map(hv, 0, dv, 0, &mut dva, 4096, RIGHT_READ | RIGHT_WRITE) != 0 {
        return None;
    }
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        return None;
    }
    if berry_process_start(hp, ht, DPG_ENTRY, DPG_SP, 0) != 0 {
        return None;
    }
    Some((hp, hv, ht))
}

/// dpgn 子 join（DPG_CAP_NS 同口径帽）；None = 超帽/坏面（丢唤子永睡
/// 帽到点即折——§5-⑤ 执法面）。
fn dpgn_join(hp: Handle) -> Option<i32> {
    let mut obs = 0u64;
    if berry_object_wait_one(hp, SIGNAL_TERMINATED, now() + DPG_CAP_NS, &mut obs) != 0
        || obs != SIGNAL_TERMINATED
    {
        return None;
    }
    let mut code: i32 = 0;
    if berry_process_get_exit_code(hp, &mut code) != 0 {
        return None;
    }
    Some(code)
}

/// dpgn 子收口（dpg_storm 同序：先 unmap demand 再 unmap 像 → 关三柄；
/// happy 路径内核侧异常 = trap 响）。
fn dpgn_reap(hv: Handle, ht: Handle, hp: Handle) {
    if berry_vmar_unmap(hv, DPG_VA, 4096) != 0
        || berry_vmar_unmap(hv, DPG_ENTRY, DPG_MAP_LEN) != 0
        || berry_handle_close(ht) != 0
        || berry_handle_close(hp) != 0
        || berry_handle_close(hv) != 0
    {
        trap();
    }
}

/// G4-3 B 刀自验：**0x21 四早退负序**（per-request 键化后的拒绝面单点化
/// ——键化后白唤唯一残源 = 失败供给唤起 faulter 再重 fault，四负序各断
/// 精确 errno）。镜像内嵌零 rootfs 依赖 = 裸跑/EFI 路同打；链内坏面 =
/// eco 降级行 + 标记缺失（smoke FAIL 兜底——降级路径子泄漏 = §5-⑤
/// fail loud-on-hang 先例同款，不掩盖失败）。
///
/// ①门：dup 降权别名（仅 RIGHT_READ，无 SET_POLICY）→ -EACCES——阻塞
///   收货前 fail fast，零子进程。
/// ②对齐：soff=1 → -EINVAL——入口首查，先于句柄门与入睡。
/// ③commit：在飞武装子 + 未提交 demand 源 → src.read 拒 -EBADSTATE；
///   供给窗 supply_page **先于**源拷贝 → 子已被零页唤醒 → 读 0 非 magic
///   → **join 9**（供给先行 + 恒唤本槽 + 探针返 true 无挂死，三合一
///   实锚）。
/// ④身份验：在飞武装子（区间属 dv）+ 冒充 vd（异 VMO，demand 面——过
///   SET_POLICY 门后落属主比对）→ -EINVAL——commit 前拒 → 子未得页重
///   fault 再武装；正调 0x21(vd=dv, sv=ss) 自愈 → **join 7**（重武装-
///   自愈闭环锚，§5-③ 恒唤语义活证）。
fn dpgn_selftest(console: Handle) {
    let _pager_hold = PagerHold::new(); // X-3c-3：同 dpg_selftest——自供期间 pager 让位
    // 公共 demand VMO（③④子区间属主；SET_POLICY 门内柄）。
    let mut dv = 0 as Handle;
    if berry_vmo_create(4096, VMO_OPT_DEMAND, &mut dv) != 0 || dv == 0 {
        eco_note(console, b"dpgn dv");
        return;
    }
    // ①门。
    let mut weak = 0 as Handle;
    if berry_handle_duplicate(dv, RIGHT_READ, &mut weak) != 0 || weak == 0 {
        eco_note(console, b"dpgn dup");
        return;
    }
    if berry_pager_supply_pages(weak, 0, 0) != BERRY_ERR_ACCESS_DENIED {
        eco_note(console, b"dpgn gate");
        return;
    }
    if berry_handle_close(weak) != 0 {
        trap();
    }
    // ②对齐（入口首查——soff 拒先于句柄门与阻塞收货）。
    if berry_pager_supply_pages(dv, 0, 1) != BERRY_ERR_INVALID_ARG {
        eco_note(console, b"dpgn align");
        return;
    }
    // 公共像 VMO（③④子同像 dpgp——dpg_storm 形）。
    let mut iv = 0 as Handle;
    if berry_vmo_create(DPG_MAP_LEN as u64, 0, &mut iv) != 0 || iv == 0 {
        eco_note(console, b"dpgn iv");
        return;
    }
    if berry_vmo_commit(iv, 0, DPG_MAP_LEN as u64) != 0
        || berry_vmo_write(iv, DPGP_BIN.as_ptr(), 0, DPGP_BIN.len()) != 0
    {
        eco_note(console, b"dpgn ivw");
        return;
    }
    // ③commit：未提交 demand 源（read 拒面——防线 4 源页零缺页）。
    let mut us = 0 as Handle;
    if berry_vmo_create(4096, VMO_OPT_DEMAND, &mut us) != 0 || us == 0 {
        eco_note(console, b"dpgn us");
        return;
    }
    let Some((hp3, hv3, ht3)) = dpgn_spawn(iv, dv) else {
        eco_note(console, b"dpgn sp3");
        return;
    };
    // 身份验同 VMO 过 → 供给 commit 零页 → src.read(us) NotCommitted 拒。
    if berry_pager_supply_pages(dv, us, 0) != BERRY_ERR_BAD_STATE {
        eco_note(console, b"dpgn commit");
        return;
    }
    match dpgn_join(hp3) {
        Some(9) => {}
        _ => {
            eco_note(console, b"dpgn j9");
            return;
        }
    }
    dpgn_reap(hv3, ht3, hp3);
    // ④身份验：冒充 vd（异 VMO）+ 自愈源 ss。dvb 须 **demand 面**——0x21
    // 门权 SET_POLICY 只授 demand 创建柄（syscall.rs vmo_create 权利集分
    // 流），普通 VMO 死在权利门 -EACCES 到不了属主比对（该面 ①门负序已
    // 覆盖）；demand 异 VMO 过门后落点 = 属主比对 -EINVAL。
    let mut dvb = 0 as Handle;
    if berry_vmo_create(4096, VMO_OPT_DEMAND, &mut dvb) != 0 || dvb == 0 {
        eco_note(console, b"dpgn dvb");
        return;
    }
    let mut ss = 0 as Handle;
    if berry_vmo_create(4096, 0, &mut ss) != 0 || ss == 0 {
        eco_note(console, b"dpgn ss");
        return;
    }
    if berry_vmo_commit(ss, 0, 4096) != 0
        || berry_vmo_write(ss, DPG_MAGIC.to_le_bytes().as_ptr(), 0, 8) != 0
    {
        eco_note(console, b"dpgn ssw");
        return;
    }
    let Some((hp4, hv4, ht4)) = dpgn_spawn(iv, dv) else {
        eco_note(console, b"dpgn sp4");
        return;
    };
    // 冒充：区间属 dv、vd=dvb → -EINVAL（commit 前——子未得页重武装）。
    if berry_pager_supply_pages(dvb, 0, 0) != BERRY_ERR_INVALID_ARG {
        eco_note(console, b"dpgn id");
        return;
    }
    // 正调自愈（dv 页已由 ③ commit——幂等门跳过 + ④子填叶 + 源拷贝）。
    if berry_pager_supply_pages(dv, ss, 0) != 0 {
        eco_note(console, b"dpgn heal");
        return;
    }
    match dpgn_join(hp4) {
        Some(7) => {}
        _ => {
            eco_note(console, b"dpgn j7");
            return;
        }
    }
    dpgn_reap(hv4, ht4, hp4);
    // 收口：本刀五 VMO（dv 钉已随两子 unmap 归零——关面不 -EBUSY）。
    if berry_handle_close(dvb) != 0
        || berry_handle_close(ss) != 0
        || berry_handle_close(us) != 0
        || berry_handle_close(iv) != 0
        || berry_handle_close(dv) != 0
    {
        trap();
    }
    send_retry(console, DPGN_OK.as_ptr(), DPGN_OK.len());
}

// ---- G4-3 C-full：缺页 soak 满配（`DPGF ok n=15` + C-4 回卷 `DPGF2 ok n=3`）----

/// C-full 满配子数：16 槽登记帽恰满（boot `dpg::init` 永占 1〔Box::leak
/// 不 Drop〕+ 15 子根 Vmar 各登记 1）；第 17 方 = ktest OwnerConflict 已
/// 单测，本测不打该面。
const DPGF_N: usize = 15;
/// C-4 槽回收二轮子数（declare「16 槽并发帽非终身帽」压测级定谳——全子
/// join + 收口后登记回落 1，再 spawn 再登记 Ok 的活证）。
const DPGF2_N: usize = 3;
/// spread 线程栈顶（像窗 [0x1000,0x6000) 对切：主栈顶 0x6000 用页 0x5000，
/// 本栈顶 0x4000 用页 0x3000——各 8KiB 无叠，像驻页 0x1000）。
const DPG_SP2: usize = 0x4000;
/// 波内停泊窗（spawn 环毕 → 轮供环前）：faulter 到齐窗（波相干性——
/// 无窗 = 流水线化，DPGF 终态行序面退化）。X-5 过滤取刀（EREQ_SLOTS
/// 8→32）前本窗兼把「池满溢出」升格结构面（15>K=8；实证档 ry8D79/
/// MdG8Xe〔lockdep banner=1 + ovfl=0 快红〕）；K 抬 32 后容量面恒不
/// 触、溢出机器覆盖迁 ktest 确定性单测（X-5 同刀），本窗仅存到齐职责
///（RCjWfp 484s 烧帽 = 变体越 ROOTFS_MAX 门 miss〔CRIT-CF1〕，非本面）。
/// 250ms = 15 任务 × 双核 TCG 调度余量（含 lockdep 开销 + 宿主争用劣化）。
const DPGF_SETTLE_NS: u64 = 250_000_000;
/// C-full 标记（13 字节）。**cpio 具名文件门控专属**：正装 rootfs/run/
/// smoke/EFI 无 `dpgf.on` = 恒不打——rootfs 相 50 / 主 smoke 78 / EFI 36
/// 计数零漂；变体卷 = `tools/smoke-dpgsoak.sh` 专属断言面。"DPGF ok n=15"
/// 与现役标记两两互非子串（"DPG ok"/"DPGP ok"/"DPGS ok"/"DPGN ok" 位 3
/// 后 F≠空格/P/S/N；"DPGF2 ok n=3" 位 4 后空格≠'2'，反向亦异）。
const DPGF_OK: &[u8; 13] = b"DPGF ok n=15\n";
/// 标记 n= 双位与子数同源（DPGN 单位同族）：改 N 忘改标记即编译拒。
const _: () =
    assert!(DPGF_OK[10] == b'0' + DPGF_N as u8 / 10 && DPGF_OK[11] == b'0' + DPGF_N as u8 % 10);
/// C-4 二轮标记（13 字节；同门控同专属面）。
const DPGF2_OK: &[u8; 13] = b"DPGF2 ok n=3\n";
const _: () = assert!(DPGF2_OK[11] == b'0' + DPGF2_N as u8 && DPGF2_N < 10);

// ---- G4-3 D/E 刀（bench.py --dpg 通道）----

/// D 刀 bench 区形状（dpgp `DPGP_BENCH_*` 同值双源——两文件各持 const 改
/// 须双改）：32 页 demand 区 + 1 页结果区。
const DPGB_PAGES: usize = 32;
const DPGB_VA: usize = 0x20_0000;
const DPGB_RES_VA: usize = 0x30_0000;
/// bench 子标签（dpgp `DPGP_BENCH_ALL/8` 同值双源；usize = thread_start
/// arg 形参同型）。
const DPGB_TAG_ALL: usize = 0xB301;
const DPGB_TAG_8: usize = 0xB302;
/// 结果页 done 哨槽位（dpgp `DPGP_RES_DONE` 同值双源；字节偏移 = ×8）。
const DPGB_RES_DONE: usize = 511;
/// D 相 join 帽：32 供给 × TCG 弹性 + 余量（供给步阻塞收货无重发语义——
/// 帽到点 = eco 降级行 + 标记缺席 = bench FAIL loud）。
const DPGB_CAP_NS: u64 = 120 * 1_000_000_000;
/// E 相预滚/窗宽：footprint 差分走内核 soakmon `pages=` 心跳（fw_cfg
/// opt/berry/soak 门，60s 拍、**首拍潜伏 5 拍** = 300s）。预滚 340s =
/// 越过首拍（本相调用点 ≈ 内核起 +45s，首拍 ≈ +345s；+余量）；窗
/// 130s = **标记前**垫 ≥2 拍（标记先于窗 = 上一窗拍不含本相位增量——
/// 帧序：A → 基线窗 → 起 demand 子 → B 窗毕打 B → 起 eager 子 → C 窗
/// 毕打 C → 尾窗 → E）。
const DPGE_PREROLL_NS: u64 = 340 * 1_000_000_000;
const DPGE_WINDOW_NS: u64 = 130 * 1_000_000_000;
/// E 相 done 哨轮询：250ms 步进读结果页哨位（子触面毕打 magic 哨后停泊
/// 保活）；30s 帽 = 8 页首触 TCG 弹性 ×5 余量。
const DPGE_POLL_NS: u64 = 250 * 1_000_000;
const DPGE_POLL_CAP_NS: u64 = 30 * 1_000_000_000;
/// E 相收尾 terminate 等 TERMINATED 帽（kill_all 收割停泊睡者——join 同
/// 语义尾部，宽帽不押时序）。
const DPGE_TERM_CAP_NS: u64 = 30 * 1_000_000_000;
/// E 相相位标记（6 字节；"DPGE " 与 "DPGF"/"DPG ok" 位 3 E≠F/位 4 E≠空格
/// 双距互斥——互非子串）。
const DPGE_A: &[u8; 7] = b"DPGE A\n";
const DPGE_B: &[u8; 7] = b"DPGE B\n";
const DPGE_C: &[u8; 7] = b"DPGE C\n";
const DPGE_E: &[u8; 7] = b"DPGE E\n";
// X-4 E 面 exec 差分（dpgx_foot）：帧标记 A/B/C/E + 子定泊信号行（子单行
// 打后阻塞 stdin read——§A61 时序不是契约，行到 = 停泊点确定性锚）。
const DPGX_A: &[u8; 7] = b"DPGX A\n";
const DPGX_B: &[u8; 7] = b"DPGX B\n";
const DPGX_C: &[u8; 7] = b"DPGX C\n";
const DPGX_E: &[u8; 7] = b"DPGX E\n";
const DPGX_READY: &[u8; 11] = b"DPGX ready\n";
const DPGX_READY_CAP_NS: u64 = 60 * 1_000_000_000;
// X-5 现役族差分帧（x5_foot；dpgx_foot 同形，bench.py E2 臂配对）。
const X5_A: &[u8; 5] = b"X5 A\n";
const X5_B: &[u8; 5] = b"X5 B\n";
const X5_C: &[u8; 5] = b"X5 C\n";
const X5_E: &[u8; 5] = b"X5 E\n";

/// u64 十进制追加（eco 降级行手排同款无 fmt 依赖；返回新尾位）。
fn append_u64_dec(buf: &mut [u8], at: usize, v: u64) -> usize {
    let mut dig = [0u8; 20];
    let mut dn = 0usize;
    let mut x = v;
    loop {
        dig[dn] = b'0' + (x % 10) as u8;
        dn += 1;
        x /= 10;
        if x == 0 {
            break;
        }
    }
    let mut i = at;
    while dn > 0 {
        dn -= 1;
        buf[i] = dig[dn];
        i += 1;
    }
    i
}

/// D 刀：缺页首触延迟 p50/p99——**faulter 侧 clock 环夹**（0x21 内 KL 窗
/// 不可测；用户态视角含唤醒延迟最诚实——breakdown 裁决）。单子 32 页
/// 顺序首触 × 一对一供给（单 faulter 顺序 fault = 请求序 = 供给序，源页
/// offset 递推零错配面）；样本经结果页 VMO 回传 → 插入排序 →
/// `DPGB p50=<ns> p99=<ns> min=<ns> max=<ns> n=32`（宿主 bench.py --dpg
/// 收割落档）。**门 = cpio 具名文件 `dpgb.on`**（dpgf.on 同族：正装面
/// 缺席 = 静默零扰动零新标记）；链内坏面 = eco 降级行 + 标记缺席。
fn dpg_bench(console: Handle) {
    let _pager_hold = PagerHold::new(); // X-3c-3：同 dpg_selftest——自供期间 pager 让位
    if cpio_find(H_ROOTFS, b"dpgb.on").is_none() {
        return;
    }
    // 源 VMO：32 页 eager，每页首 8B 预写 magic（供给页 = 源页逐字节拷贝
    // ——子逐页验值随测正确性）。
    let mut sv = 0 as Handle;
    if berry_vmo_create((DPGB_PAGES * 4096) as u64, 0, &mut sv) != 0 || sv == 0 {
        eco_note(console, b"dpgb sv");
        return;
    }
    if berry_vmo_commit(sv, 0, (DPGB_PAGES * 4096) as u64) != 0 {
        eco_note(console, b"dpgb svc");
        return;
    }
    for i in 0..DPGB_PAGES {
        if berry_vmo_write(sv, DPG_MAGIC.to_le_bytes().as_ptr(), (i * 4096) as u64, 8) != 0 {
            eco_note(console, b"dpgb svw");
            return;
        }
    }
    // 像 VMO（dpg_selftest 同形：DPGP_BIN eager 内嵌）。
    let mut iv = 0 as Handle;
    if berry_vmo_create(DPG_MAP_LEN as u64, 0, &mut iv) != 0 || iv == 0 {
        eco_note(console, b"dpgb iv");
        return;
    }
    if berry_vmo_commit(iv, 0, DPG_MAP_LEN as u64) != 0
        || berry_vmo_write(iv, DPGP_BIN.as_ptr(), 0, DPGP_BIN.len()) != 0
    {
        eco_note(console, b"dpgb ivw");
        return;
    }
    // demand 区 VMO（32 页 demand）+ 结果页 VMO（1 页 eager——样本回传面）。
    let mut dv = 0 as Handle;
    if berry_vmo_create((DPGB_PAGES * 4096) as u64, VMO_OPT_DEMAND, &mut dv) != 0 || dv == 0 {
        eco_note(console, b"dpgb dv");
        return;
    }
    let mut rv = 0 as Handle;
    if berry_vmo_create(4096, 0, &mut rv) != 0 || rv == 0 {
        eco_note(console, b"dpgb rv");
        return;
    }
    if berry_vmo_commit(rv, 0, 4096) != 0 {
        eco_note(console, b"dpgb rvc");
        return;
    }
    // 子 = dpgf_wave 奇数形（**映射先行**——park/stbench 代码皆在像窗内，
    // 映射后起跑零竞速）：main 停泊（bootstrap 事件句柄迁移）+ worker
    // thread_start 裸 arg = B3_ALL（process_start 第 5 参 = 迁移语义不可传
    // 标签，判例）。像窗@DPG_ENTRY + demand 区@DPGB_VA + 结果页@RES。
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    if berry_process_create(0, 0, &mut hp, &mut hv) != 0 || hp == 0 || hv == 0 {
        eco_note(console, b"dpgb pc");
        return;
    }
    let mut va = DPG_ENTRY;
    if berry_vmar_map(hv, 0, iv, 0, &mut va, DPG_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0 {
        eco_note(console, b"dpgb imap");
        return;
    }
    let mut dva = DPGB_VA;
    if berry_vmar_map(
        hv,
        0,
        dv,
        0,
        &mut dva,
        DPGB_PAGES * 4096,
        RIGHT_READ | RIGHT_WRITE,
    ) != 0 {
        eco_note(console, b"dpgb dmap");
        return;
    }
    let mut rva = DPGB_RES_VA;
    if berry_vmar_map(hv, 0, rv, 0, &mut rva, 4096, RIGHT_READ | RIGHT_WRITE) != 0 {
        eco_note(console, b"dpgb rmap");
        return;
    }
    let mut hev = 0 as Handle;
    if berry_event_create(0, &mut hev) != 0 || hev == 0 {
        eco_note(console, b"dpgb ev");
        return;
    }
    let mut ht = 0 as Handle;
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        eco_note(console, b"dpgb tc");
        return;
    }
    if berry_process_start(hp, ht, DPG_ENTRY, DPG_SP, hev) != 0 {
        eco_note(console, b"dpgb ps");
        return;
    }
    let mut htw = 0 as Handle;
    if berry_thread_create(hp, 0, &mut htw) != 0 || htw == 0 {
        eco_note(console, b"dpgb tw");
        return;
    }
    if berry_thread_start(htw, DPG_ENTRY, DPG_SP2, DPGB_TAG_ALL) != 0 {
        eco_note(console, b"dpgb ts");
        return;
    }
    // 32× 0x21 阻塞收货：源 offset 逐页递推（请求序 = 供给序）。
    for i in 0..DPGB_PAGES {
        if berry_pager_supply_pages(dv, sv, (i * 4096) as u64) != 0 {
            eco_note(console, b"dpgb sup");
            return;
        }
    }
    // join 7（供给步已毕只罩尾段；错码 = 供给链正确性面已随测）。
    let mut obs = 0u64;
    if berry_object_wait_one(hp, SIGNAL_TERMINATED, now() + DPGB_CAP_NS, &mut obs) != 0
        || obs != SIGNAL_TERMINATED
    {
        eco_note(console, b"dpgb join");
        return;
    }
    let mut code: i32 = 0;
    if berry_process_get_exit_code(hp, &mut code) != 0 || code != DPG_OK_CODE {
        eco_note(console, b"dpgb code");
        return;
    }
    // 样本收割（[u64;64] = 512B 恰页内样本区）+ 插入排序（no_std 无 sort
    // 面；n=32 小排序）。
    let mut lat = [0u64; 64];
    if berry_vmo_read(rv, lat.as_mut_ptr() as *mut u8, 0, 512) != 0 {
        eco_note(console, b"dpgb rd");
        return;
    }
    for i in 1..DPGB_PAGES {
        let mut j = i;
        while j > 0 && lat[j - 1] > lat[j] {
            lat.swap(j - 1, j);
            j -= 1;
        }
    }
    // `DPGB p50= p99= min= max= n=32`（n 恰 2 位定长——改 DPGB_PAGES 须
    // 同步行尾；带参形态防子串混叠）。标签宽 vs 字面长编译期钉板——首跑
    // 实踩：p99 切 6 配 5 字面 = copy_from_slice panic → rust_begin_unwind
    // → trap，svcmgr 死在收尾行组装（雷册 §A135）。
    const _: () = assert!(b"DPGB p50=".len() == 9);
    const _: () = assert!(b" p99=".len() == 5);
    const _: () = assert!(b" min=".len() == 5);
    const _: () = assert!(b" max=".len() == 5);
    const _: () = assert!(b" n=32".len() == 5);
    /// n= 字面与 DPGB_PAGES 数字同源（DPGF_OK :2931 同款 digit tie）：len
    /// 钉板拦不住「行印 n=32、实跑 16」——改 PAGES 忘改行尾即编译拒。
    const _: () = assert!(b" n=32"[3] == b'0' + DPGB_PAGES as u8 / 10
        && b" n=32"[4] == b'0' + DPGB_PAGES as u8 % 10);
    let mut line = [0u8; 160];
    let mut i = 0usize;
    line[..9].copy_from_slice(b"DPGB p50=");
    i = 9;
    i = append_u64_dec(&mut line, i, lat[DPGB_PAGES / 2]);
    line[i..i + 5].copy_from_slice(b" p99=");
    i += 5;
    i = append_u64_dec(&mut line, i, lat[DPGB_PAGES - 1]);
    line[i..i + 5].copy_from_slice(b" min=");
    i += 5;
    i = append_u64_dec(&mut line, i, lat[0]);
    line[i..i + 5].copy_from_slice(b" max=");
    i += 5;
    i = append_u64_dec(&mut line, i, lat[DPGB_PAGES - 1]);
    line[i..i + 5].copy_from_slice(b" n=32");
    i += 5;
    line[i] = b'\n';
    send_retry(console, line.as_ptr(), i + 1);
    // 收口（dpgf_wave 同序：先 unmap 三区再关全柄——Vmar Drop 注销登记
    // 槽）。非 0 = 内核侧异常，loud。
    if berry_vmar_unmap(hv, DPGB_RES_VA, 4096) != 0
        || berry_vmar_unmap(hv, DPGB_VA, DPGB_PAGES * 4096) != 0
        || berry_vmar_unmap(hv, DPG_ENTRY, DPG_MAP_LEN) != 0
        || berry_handle_close(htw) != 0
        || berry_handle_close(ht) != 0
        || berry_handle_close(hp) != 0
        || berry_handle_close(hv) != 0
        || berry_handle_close(rv) != 0
        || berry_handle_close(dv) != 0
        || berry_handle_close(iv) != 0
        || berry_handle_close(sv) != 0
    {
        trap();
    }
}

/// E 刀公共拉起：停泊子（dpgf_wave 奇数形，**映射先行**）+ worker
/// thread_start 裸 arg = B3_8（触前 8 页）。败 = None（调用方降级）。
/// 返回 (hp, hv, ht, htw)——收口面（hev 已迁移离表，勿关）。
fn dpg_foot_spawn(
    hp_hv: &mut (Handle, Handle),
    ht_htw: &mut (Handle, Handle),
    iv: Handle,
    region: Handle,
    rv: Handle,
) -> bool {
    if berry_process_create(0, 0, &mut hp_hv.0, &mut hp_hv.1) != 0
        || hp_hv.0 == 0
        || hp_hv.1 == 0
    {
        return false;
    }
    let mut va = DPG_ENTRY;
    if berry_vmar_map(hp_hv.1, 0, iv, 0, &mut va, DPG_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0 {
        return false;
    }
    let mut dva = DPGB_VA;
    if berry_vmar_map(
        hp_hv.1,
        0,
        region,
        0,
        &mut dva,
        DPGB_PAGES * 4096,
        RIGHT_READ | RIGHT_WRITE,
    ) != 0 {
        return false;
    }
    let mut rva = DPGB_RES_VA;
    if berry_vmar_map(hp_hv.1, 0, rv, 0, &mut rva, 4096, RIGHT_READ | RIGHT_WRITE) != 0 {
        return false;
    }
    let mut hev = 0 as Handle;
    if berry_event_create(0, &mut hev) != 0 || hev == 0 {
        return false;
    }
    if berry_thread_create(hp_hv.0, 0, &mut ht_htw.0) != 0 || ht_htw.0 == 0 {
        return false;
    }
    if berry_process_start(hp_hv.0, ht_htw.0, DPG_ENTRY, DPG_SP, hev) != 0 {
        return false;
    }
    if berry_thread_create(hp_hv.0, 0, &mut ht_htw.1) != 0 || ht_htw.1 == 0 {
        return false;
    }
    berry_thread_start(ht_htw.1, DPG_ENTRY, DPG_SP2, DPGB_TAG_8) == 0
}

/// E 相 done 哨轮询：结果页哨槽 == magic 即真；30s 帽尽/读败 = 假。
fn dpg_foot_done(rv: Handle) -> bool {
    let deadline = now() + DPGE_POLL_CAP_NS;
    loop {
        let mut v = [0u8; 8];
        if berry_vmo_read(rv, v.as_mut_ptr(), (DPGB_RES_DONE * 8) as u64, 8) != 0 {
            return false;
        }
        let mut got = 0u64;
        for b in v.iter().rev() {
            got = (got << 8) | *b as u64;
        }
        if got == DPG_MAGIC {
            return true;
        }
        if now() >= deadline {
            return false;
        }
        berry_thread_sleep(now() + DPGE_POLL_NS);
    }
}

/// 停泊收割 terminate 容差刀（X-5 E 臂定谳 2026-11-08：svcmgr E 臂「停摆」
/// 实为拆台序 terminate 撞已退出子 → 内核幂等面 -37 → 旧「非 0 = loud」
/// trap 自杀——dump 寄存器逐槽 = 0x314c 调用点原形）。停泊子可先退
///（d-3b libc exit EOF 先行面）→ terminate 返 -37 = 合法收割态非异常
///（内核侧「已终止：幂等面在 wait/get_exit_code」）；真死活由随后
/// wait TERMINATED / reap 收割面裁决。-37 打 eco 标记暴露先退频率
///（tag ≤18B 同 eco_note 截断纪律）。
fn park_terminate(console: Handle, hp: Handle, tag: &[u8]) -> bool {
    let rc = berry_process_terminate(hp, 9);
    if rc == 0 {
        return true;
    }
    if rc == BERRY_ERR_BAD_STATE {
        eco_note(console, tag);
        return true;
    }
    false
}

/// E 刀：demand 8 触 vs eager 同形 32 页 footprint 差分——**子在册停泊**
///（父持 rv/dv/ev 柄至 E 臂终局 = 记账面稳定——v1 无 Vmar 页表回收
///（vmar.rs:11），页属 VMO 对象；done 哨后停泊，本侧 terminate 收口）。
/// pages 口径 = 内核 soakmon 心跳（opt/berry/soak 门
/// fw_cfg——bench.py E 臂专递；60s 拍/首拍潜伏 5 拍）。帧序（**窗在标记
/// 前**——标记 = 上一窗的收割点）：预滚 340s → `A`（基线）→ 窗 → 起
/// demand 子（8 供给）→ 窗 → `B` → 起 eager 子（32 页全 commit）→ 窗
/// → `C` → 尾窗 → `E`。宿主配对「标记前最近拍」差分：d = pB−pA /
/// e = pC−pB，d<e 断言在宿主（eager 32 页全 commit vs demand 8 页实供，
/// 结构差 ≈24 页 + 映射元页）。**门 = `dpge.on`**（同族缺席静默）；
/// 链内坏面 = eco 降级行 + 相位标记缺席。
fn dpg_foot(console: Handle) {
    let _pager_hold = PagerHold::new(); // X-3c-3：同 dpg_selftest——自供期间 pager 让位
    if cpio_find(H_ROOTFS, b"dpge.on").is_none() {
        return;
    }
    // 共享件：像 VMO + 源 VMO（demand 供给源，8 页用量；eager 侧不消费）。
    let mut iv = 0 as Handle;
    if berry_vmo_create(DPG_MAP_LEN as u64, 0, &mut iv) != 0 || iv == 0 {
        eco_note(console, b"dpge iv");
        return;
    }
    if berry_vmo_commit(iv, 0, DPG_MAP_LEN as u64) != 0
        || berry_vmo_write(iv, DPGP_BIN.as_ptr(), 0, DPGP_BIN.len()) != 0
    {
        eco_note(console, b"dpge ivw");
        return;
    }
    let mut sv = 0 as Handle;
    if berry_vmo_create(4096, 0, &mut sv) != 0 || sv == 0 {
        eco_note(console, b"dpge sv");
        return;
    }
    if berry_vmo_commit(sv, 0, 4096) != 0
        || berry_vmo_write(sv, DPG_MAGIC.to_le_bytes().as_ptr(), 0, 8) != 0
    {
        eco_note(console, b"dpge svw");
        return;
    }
    // 预滚 340s（越过心跳首拍）→ 基线标记 A。
    let mut t0 = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t0) != 0 {
        eco_note(console, b"dpge clk");
        return;
    }
    berry_thread_sleep(t0 + DPGE_PREROLL_NS);
    send_retry(console, DPGE_A.as_ptr(), DPGE_A.len());
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    // demand 子：32 页 demand 区（**映射全宽、触面仅前 8**——d 增量 =
    // 8 实供页 + 子映射元），B3_8 触毕打哨停泊。
    let mut dv = 0 as Handle;
    if berry_vmo_create((DPGB_PAGES * 4096) as u64, VMO_OPT_DEMAND, &mut dv) != 0 || dv == 0 {
        eco_note(console, b"dpge dv");
        return;
    }
    let mut rv1 = 0 as Handle;
    if berry_vmo_create(4096, 0, &mut rv1) != 0 || rv1 == 0 {
        eco_note(console, b"dpge rv1");
        return;
    }
    if berry_vmo_commit(rv1, 0, 4096) != 0 {
        eco_note(console, b"dpge rv1c");
        return;
    }
    let mut c1 = ((0 as Handle, 0 as Handle), (0 as Handle, 0 as Handle));
    if !dpg_foot_spawn(&mut c1.0, &mut c1.1, iv, dv, rv1) {
        eco_note(console, b"dpge c1");
        return;
    }
    // 8× 供给（源 1 页 magic 复用——触面前 8 页每页首 8B 同 magic）。
    for _i in 0..8 {
        if berry_pager_supply_pages(dv, sv, 0) != 0 {
            eco_note(console, b"dpge sup");
            return;
        }
    }
    if !dpg_foot_done(rv1) {
        eco_note(console, b"dpge w1");
        return;
    }
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    send_retry(console, DPGE_B.as_ptr(), DPGE_B.len());
    // eager 子：32 页 eager 区（全 commit + 每页首 8B magic——e 增量含
    // ev 本体 32 页，触面同前 8 零 fault 对照）。
    let mut ev = 0 as Handle;
    if berry_vmo_create((DPGB_PAGES * 4096) as u64, 0, &mut ev) != 0 || ev == 0 {
        eco_note(console, b"dpge ev");
        return;
    }
    if berry_vmo_commit(ev, 0, (DPGB_PAGES * 4096) as u64) != 0 {
        eco_note(console, b"dpge evc");
        return;
    }
    for i in 0..DPGB_PAGES {
        if berry_vmo_write(ev, DPG_MAGIC.to_le_bytes().as_ptr(), (i * 4096) as u64, 8) != 0 {
            eco_note(console, b"dpge evw");
            return;
        }
    }
    let mut rv2 = 0 as Handle;
    if berry_vmo_create(4096, 0, &mut rv2) != 0 || rv2 == 0 {
        eco_note(console, b"dpge rv2");
        return;
    }
    if berry_vmo_commit(rv2, 0, 4096) != 0 {
        eco_note(console, b"dpge rv2c");
        return;
    }
    let mut c2 = ((0 as Handle, 0 as Handle), (0 as Handle, 0 as Handle));
    if !dpg_foot_spawn(&mut c2.0, &mut c2.1, iv, ev, rv2) {
        eco_note(console, b"dpge c2");
        return;
    }
    if !dpg_foot_done(rv2) {
        eco_note(console, b"dpge w2");
        return;
    }
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    send_retry(console, DPGE_C.as_ptr(), DPGE_C.len());
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    send_retry(console, DPGE_E.as_ptr(), DPGE_E.len());
    // 收尾：两子 terminate（停泊睡者 kill_all 收割；子可先退——d-3b 面，
    // -37 容差见 park_terminate）→ 等 TERMINATED →
    // unmap 三区 → 关全柄（hev 已迁移勿关）；其余非 0 = 内核侧异常，loud。
    if !park_terminate(console, c1.0.0, b"dpgf t-37")
        || !park_terminate(console, c2.0.0, b"dpgf t-37")
    {
        trap();
    }
    for (((hp, hv), (ht, htw)), rv, region) in [(c1, rv1, dv), (c2, rv2, ev)] {
        let mut obs = 0u64;
        if berry_object_wait_one(hp, SIGNAL_TERMINATED, now() + DPGE_TERM_CAP_NS, &mut obs) != 0
            || obs != SIGNAL_TERMINATED
        {
            trap();
        }
        if berry_vmar_unmap(hv, DPGB_RES_VA, 4096) != 0
            || berry_vmar_unmap(hv, DPGB_VA, DPGB_PAGES * 4096) != 0
            || berry_vmar_unmap(hv, DPG_ENTRY, DPG_MAP_LEN) != 0
            || berry_handle_close(htw) != 0
            || berry_handle_close(ht) != 0
            || berry_handle_close(hp) != 0
            || berry_handle_close(hv) != 0
            || berry_handle_close(rv) != 0
            || berry_handle_close(region) != 0
        {
            trap();
        }
    }
    if berry_handle_close(sv) != 0 || berry_handle_close(iv) != 0 {
        trap();
    }
}

/// dpgx 停泊子包：child + 本侧三端（out_s 收流 / out_c 交叠端自留 /
/// in_s 停泊锚——本侧永持不发不关 = 子 stdin read 永阻塞；in_c 已随 env
/// 块 take 迁入子表，子死随表释放）。
struct DpgxParked {
    child: ChildHandles,
    out_s: Handle,
    out_c: Handle,
    in_s: Handle,
}

/// dpgx 拉起（stdout 私有通道 + stdin held 端停泊形；SpawnSpec 全零 gift
/// 面——dpgx 无 fs/net 依赖）。败 = None（Err 全在 env send 前，四端显式
/// 回收防表槽泄漏——ps 同款）。
fn dpgx_spawn_parked(data_off: u64, data_len: u64, console: Handle) -> Option<DpgxParked> {
    let (out_s, out_c) = channel_pair();
    let (in_s, in_c) = channel_pair();
    let child = match spawn_rootfs_elf(
        H_ROOTFS,
        &SpawnSpec {
            name: b"bin/dpgx",
            data_off,
            data_len,
            stdout: out_c,
            stack_pages: 8, // std 小程序档（SpawnSpec 注）
            stdin_feed: None,
            stdin_handle: Some(in_c), // 停泊锚：held 不发不 EOF
            args: &[],
            extra_gifts: &[],
            mmio_vmo: None,
            mmio_va: 0,
            intent: 0,
            force_eager: false,
        },
    ) {
        Ok(c) => c,
        Err(()) => {
            if berry_handle_close(in_c) != 0
                || berry_handle_close(in_s) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            eco_note(console, b"dpgx sp");
            return None;
        }
    };
    Some(DpgxParked {
        child,
        out_s,
        out_c,
        in_s,
    })
}

/// 收定泊信号：out_s 流含 `DPGX ready\n` 即真（行缓冲单消息；EOF 先行
/// 〔子早死 libc exit 零长，d-3b〕= 假）。帽 60s——std musl TCG 冷起动慢
/// 而活着不误杀（PS_CAP_NS 同档）。杂行（理论无）保守续收。
fn dpgx_wait_ready(p: &DpgxParked) -> bool {
    let deadline = now() + DPGX_READY_CAP_NS;
    let mut m = [0u8; 64];
    loop {
        let mut nb = m.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(p.out_s, m.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            if nb == 0 {
                return false; // EOF = 子已退
            }
            if has_sub(&m[..nb], DPGX_READY) {
                return true;
            }
            continue;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(p.out_s, SIGNAL_READABLE, deadline, &mut obs) != 0 {
            return false; // 帽尽（挂死/未达停泊）
        }
    }
}

/// X-4 E 面 exec 装载差分（dpg_foot 同形帧，**真 exec 消费者版**）：同一
/// `bin/dpgx`（单 PT_LOAD 特形链接 = congruent 六查全过首例）两次拉起——
/// demand 胎（钩子在位 → 映像 = file 域 demand VMO + 零尾 zv + 恒等供给
/// 注册，启动足迹 ∝ 实触页）vs eager 胎（`EagerGuard` 摘钩 → 全幅 commit
/// 泵送，足迹 = len 全量）——pages 口径同 dpg_foot（内核 soakmon 心跳，
/// bench.py E 臂配对「标记前最近拍」：d = pB−pA / e = pC−pB，d<e 断言
/// 在宿主）。帧：预滚 → `A` → 窗 → demand 胎拉起 + `DPGX ready` 定泊 →
/// 窗 → `B` → eager 胎（guard 罩 spawn 窗）+ ready → 窗 → `C` → 窗 →
/// `E` → 双 terminate → 双 reap（demand 槽注销在 reap 内，先于 close
/// dv）→ 关本侧端。
///
/// **无 PagerHold**（与 dpg_foot 反）：0x23 恒等供给 = 内核侧表驱动，
/// 子映像 fault 由内核直供 rootfs 字节——svcmgr 本尊 pager 角色不动；
/// EagerGuard 只摘 spawn 决策面（boot 期注入的钩子读点），guard 出窗即
/// 恢复（demand 胎后续 fault 照常内核表供——表注册在 spawn 期已落）。
/// **门 = `dpgx.on`**（bench E 臂 rootfs 专递；正装/smoke/EFI 缺席 =
/// 整段静默零标记零漂移）。链内坏面 = eco 降级行 + 相位标记缺席（中段
/// 败的已停泊子有界留存至本服务终身——§5-⑤ fail loud 先例同款）。
fn dpgx_foot(console: Handle) {
    if cpio_find(H_ROOTFS, b"dpgx.on").is_none() {
        return;
    }
    let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/dpgx") else {
        eco_note(console, b"dpgx bin");
        return;
    };
    // 预滚 340s（越心跳首拍潜伏，DPGE 同源）→ 基线 A。
    let mut t0 = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t0) != 0 {
        eco_note(console, b"dpgx clk");
        return;
    }
    berry_thread_sleep(t0 + DPGE_PREROLL_NS);
    send_retry(console, DPGX_A.as_ptr(), DPGX_A.len());
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    // demand 胎：钩子在位（boot 注入未摘）→ congruent 命中 → demand 装载
    //（映像 fault 后续由内核表供——warm-up 页集 = println+stdin 触的页）。
    let p1 = match dpgx_spawn_parked(data_off, data_len, console) {
        Some(p) => p,
        None => return,
    };
    if !dpgx_wait_ready(&p1) {
        eco_note(console, b"dpgx w1");
        return;
    }
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    send_retry(console, DPGX_B.as_ptr(), DPGX_B.len());
    // eager 胎：EagerGuard 摘钩罩住 spawn 窗（装载分支决策点）——同 bin
    // 同形 → eager 全幅 commit；对照面 = 装载分支本身，非 ELF 形状。
    let p2 = {
        let _eg = EagerGuard::new();
        dpgx_spawn_parked(data_off, data_len, console)
    };
    let p2 = match p2 {
        Some(p) => p,
        None => return,
    };
    if !dpgx_wait_ready(&p2) {
        eco_note(console, b"dpgx w2");
        return;
    }
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    send_retry(console, DPGX_C.as_ptr(), DPGX_C.len());
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    send_retry(console, DPGX_E.as_ptr(), DPGX_E.len());
    // 收尾：双 terminate（停泊睡者 kill_all 收割；子可先退——d-3b 面，
    // -37 容差见 park_terminate）→ 双 reap（完整回收链，
    // 含 demand 槽注销）→ 关本侧端；其余非 0 = 内核侧异常，loud。
    if !park_terminate(console, p1.child.hp, b"dpgx t-37")
        || !park_terminate(console, p2.child.hp, b"dpgx t-37")
    {
        trap();
    }
    if p1.child.reap().is_err() || p2.child.reap().is_err() {
        trap();
    }
    for p in [(&p1, 0), (&p2, 1)] {
        let _ = p.1; // 数组同型占位（两包字段面一致）
        if berry_handle_close(p.0.out_s) != 0
            || berry_handle_close(p.0.out_c) != 0
            || berry_handle_close(p.0.in_s) != 0
        {
            trap();
        }
    }
}

/// X-5 E 面 exec 装载差分（dpgx_foot 同形帧，**现役 musl 多段族版**）：
/// 现役 bin 双件（hexyl 无参 stdin 回落 / grep 无 FILE stdin 回落——首读
/// 即泊）各两胎：demand 胎（钩子在位 → 多段 k 表逐窗分解，X-5 新装载路）
/// vs eager 胎（`EagerGuard` 摘钩 → 全幅 commit 泵送，足迹 = len 全量）。
/// 停泊锚 = `stdin_handle` 持端不发不 EOF（dpgx「父持对端」经 spawn 既有
/// 口落现役 bin）；**无 ready 行**——eager 胎 spawn 返回即全幅在册、
/// demand 胎泊于首读 warm 集只增不越，d<e 两侧皆无过采样风险（130s 窗
/// 兼 std musl TCG 冷起动）。帧：A → demand 双胎 + 窗 → B → EagerGuard
/// 罩 eager 双胎 + 窗 → C → 窗 → E → 四 terminate/reap（demand 槽注销
/// 在 reap 内）→ 关持有端。pages 口径 = soakmon 心跳（bench.py E2 臂配对
/// 「标记前最近拍」：d = pB−pA / e = pC−pB，d<e 断言在宿主）。门 =
/// rootfs `x5.on`（bench 专递；正装/smoke/EFI 缺席 = 整段静默零标记零漂
/// 移）。**无 PagerHold**（dpgx_foot 同款：0x23 恒等供给 = 内核侧表驱动）。
fn x5_foot(console: Handle) {
    if cpio_find(H_ROOTFS, b"x5.on").is_none() {
        return;
    }
    let Some((hx_off, hx_len)) = cpio_find(H_ROOTFS, b"bin/hexyl") else {
        eco_note(console, b"x5 hexyl");
        return;
    };
    let Some((gp_off, gp_len)) = cpio_find(H_ROOTFS, b"bin/grep") else {
        eco_note(console, b"x5 grep");
        return;
    };
    // 预滚（dpgx_foot 同源：越心跳首拍潜伏）→ 基线 A。
    let mut t0 = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t0) != 0 {
        eco_note(console, b"x5 clk");
        return;
    }
    berry_thread_sleep(t0 + DPGE_PREROLL_NS);
    send_retry(console, X5_A.as_ptr(), X5_A.len());
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    // demand 双胎（多段 k 表逐窗分解新路——X-5 验收面本体）。
    let p1 = match x5_spawn_parked(b"bin/hexyl", &[], 32, hx_off, hx_len, console) {
        Some(p) => p,
        None => return,
    };
    let p2 = match x5_spawn_parked(b"bin/grep", &[b"zzz"], 16, gp_off, gp_len, console) {
        Some(p) => p,
        None => {
            x5_park_drop(&[p1], console);
            return;
        }
    };
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    send_retry(console, X5_B.as_ptr(), X5_B.len());
    // eager 双胎：EagerGuard 摘钩罩 spawn 窗（装载分支决策点）——同 bin
    // 同形 → 全幅 commit；对照面 = 装载分支本身，非 ELF 形状。
    let (p3, p4) = {
        let _eg = EagerGuard::new();
        let p3 = x5_spawn_parked(b"bin/hexyl", &[], 32, hx_off, hx_len, console);
        let p4 = x5_spawn_parked(b"bin/grep", &[b"zzz"], 16, gp_off, gp_len, console);
        (p3, p4)
    };
    let p3 = match p3 {
        Some(p) => p,
        None => {
            x5_park_drop(&[p1, p2], console);
            return;
        }
    };
    let p4 = match p4 {
        Some(p) => p,
        None => {
            x5_park_drop(&[p1, p2, p3], console);
            return;
        }
    };
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    send_retry(console, X5_C.as_ptr(), X5_C.len());
    berry_thread_sleep(now() + DPGE_WINDOW_NS);
    send_retry(console, X5_E.as_ptr(), X5_E.len());
    // 收尾：四 terminate（停泊读中被杀 = 干净路径）→ 四 reap（完整回收
    // 链，含 demand 槽注销）→ 关持有端三件套。
    x5_park_drop(&[p1, p2, p3, p4], console);
}

/// x5_foot 停泊拉起（dpgx_spawn_parked 同形，name/args/栈档参数化面）：
/// 现役 bin + `stdin_handle` 持端停泊锚（不发不 EOF）。stdout = 私有通道
/// 对端（held 不读——停泊子零写；走写 = 队满自泊，零串口污染面）。
fn x5_spawn_parked(
    name: &'static [u8],
    args: &'static [&'static [u8]],
    stack_pages: u64,
    data_off: u64,
    data_len: u64,
    console: Handle,
) -> Option<DpgxParked> {
    let (out_s, out_c) = channel_pair();
    let (in_s, in_c) = channel_pair();
    let child = match spawn_rootfs_elf(
        H_ROOTFS,
        &SpawnSpec {
            name,
            data_off,
            data_len,
            stdout: out_c,
            stack_pages,
            stdin_feed: None,
            stdin_handle: Some(in_c), // 停泊锚：held 不发不 EOF
            args,
            extra_gifts: &[],
            mmio_vmo: None,
            mmio_va: 0,
            intent: 0,
            force_eager: false,
        },
    ) {
        Ok(c) => c,
        Err(()) => {
            if berry_handle_close(in_c) != 0
                || berry_handle_close(in_s) != 0
                || berry_handle_close(out_c) != 0
                || berry_handle_close(out_s) != 0
            {
                trap();
            }
            eco_note(console, b"x5 sp");
            return None;
        }
    };
    Some(DpgxParked {
        child,
        out_s,
        out_c,
        in_s,
    })
}

/// x5_foot 收尾（成功路径全四包；败路径已泊前缀）：先全员 terminate 再
/// 逐包 reap（demand 槽注销在 reap 内，先于 close dv）+ 关持有端三件套。
/// 子可先退（d-3b 面）→ -37 容差（park_terminate，eco 标记暴露频率）；
/// 其余非 0 = 内核侧异常，loud（trap 纪律）。
fn x5_park_drop(ps: &[DpgxParked], console: Handle) {
    for p in ps {
        if !park_terminate(console, p.child.hp, b"x5 t-37") {
            trap();
        }
    }
    for p in ps {
        if p.child.reap().is_err() {
            trap();
        }
        if berry_handle_close(p.out_s) != 0
            || berry_handle_close(p.out_c) != 0
            || berry_handle_close(p.in_s) != 0
        {
            trap();
        }
    }
}

/// dpgf 波（dpg_storm 满配放大 + spread 分流）：N 子并发首触 → N× 0x21
/// 轮供（MECH-1 convoy 服务序——单 pager 无单独超时；轮供相挂死 = 脚本
/// DPGF_CAP_S 帽兜底〔§5-⑤〕，DPG_CAP_NS join 帽只罩尾段——供环在
/// join 环前，挂死到不了 join）→ N× join 7 → N× 收口。偶数下标 = 现形（main 首触 DPG_VA，家核
/// CPU0）；奇数下标 = main 停泊（bootstrap 事件句柄迁移 → 子 x0 非 0 =
/// sleep 帽循环）+ 本服务
/// 侧 `thread_start`（spread=true——失衡 ≥2 翻核）线程同 DPG_ENTRY 首触：
/// 两核皆有 faulter = CONC-1 真跨核（CPU1 faulter ⇄ CPU0 pager 跨核供给
/// 对成立；偷取随迁 home 附加非确定，不作断言面）。fault 代码位置无关
/// ——读 magic → 进程 exit 7/9，非 main 线程同义；spread 线程
/// process_exit → kill_all 收割停泊 main。败 = Some(相位)（1 拉起 /
/// 2 轮供 / 3 join——降级路径子泄漏 = §5-⑤ fail loud 先例同款，标记缺席
/// = smoke FAIL 执法）。
fn dpgf_wave<const N: usize>(iv: Handle, sv: Handle, dv: Handle) -> Option<u8> {
    let mut hps = [0 as Handle; N];
    let mut hvs = [0 as Handle; N];
    let mut hts = [0 as Handle; N];
    let mut hts2 = [0 as Handle; N]; // 奇数子 spread 线程柄（偶 = 0）
    for i in 0..N {
        if berry_process_create(0, 0, &mut hps[i], &mut hvs[i]) != 0 || hps[i] == 0 || hvs[i] == 0
        {
            return Some(1);
        }
        let mut vaddr = DPG_ENTRY;
        if berry_vmar_map(hvs[i], 0, iv, 0, &mut vaddr, DPG_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0
        {
            return Some(1);
        }
        let mut dva = DPG_VA;
        if berry_vmar_map(hvs[i], 0, dv, 0, &mut dva, 4096, RIGHT_READ | RIGHT_WRITE) != 0 {
            return Some(1);
        }
        if berry_thread_create(hps[i], 0, &mut hts[i]) != 0 || hts[i] == 0 {
            return Some(1);
        }
        if i % 2 == 0 {
            if berry_process_start(hps[i], hts[i], DPG_ENTRY, DPG_SP, 0) != 0 {
                return Some(1);
            }
        } else {
            // main 停泊 + spread 首触。停泊令 = **bootstrap 事件句柄迁移**
            //（首光实证修正：process_start 第 5 参非裸 arg——非 0 = 调用者
            // 表 take 迁入子表、子 x0 = 子表新句柄值〔gen≥1 恒非 0，dpgp 侧
            // `x0 != 0` 即停泊判据〕；magic 值假 arg 被 take 当句柄拒
            // -EBADF）。take 即离本表——子 exit 收表释放，零泄漏零复用面。
            let mut hev = 0 as Handle;
            if berry_event_create(0, &mut hev) != 0 || hev == 0 {
                return Some(1);
            }
            if berry_process_start(hps[i], hts[i], DPG_ENTRY, DPG_SP, hev) != 0 {
                return Some(1);
            }
            if berry_thread_create(hps[i], 0, &mut hts2[i]) != 0 || hts2[i] == 0 {
                return Some(1);
            }
            if berry_thread_start(hts2[i], DPG_ENTRY, DPG_SP2, 0) != 0 {
                return Some(1);
            }
        }
    }
    // 停泊窗 250ms（C-3 lockdep 首跑实证补）：让 N faulter 全员先到齐再开
    // 轮供——否则「fault 到达 vs 0x21 消费」成流水线竞速（lockdep 面拖慢
    // 内核路径后 15 请求逐个到逐个消，从未 9 并发在飞 → ovfl 零计断言假
    // 败；实证档 = ry8D79/MdG8Xe 快红形）。停泊后**若全员在窗内到齐**则
    // 池满 = 结构必现（K=8 满 + N−K convoy），ovfl ≥ N−K；TCG 实测五跑
    // 恒 7 = 前提在实测成立（慢宿主可 <7——S-4 族；断言面 [1,40] 兜底）；
    // §5-⑤ 语义不变（pager 有界延迟必至）。波 2（N=3<K）同窗无害。
    let mut t0 = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t0) != 0 {
        return Some(2);
    }
    berry_thread_sleep(t0 + DPGF_SETTLE_NS);
    // N× 0x21 阻塞收货（poll-first 逐轮取一认领槽；15 请求 > K=8 → 溢出
    // convoy `DPG ovfl` 行首见——B 刀零计断言的镜像反转）。
    for _round in 0..N {
        if berry_pager_supply_pages(dv, sv, 0) != 0 {
            return Some(2);
        }
    }
    // N× join 7（帽同口径——供给步已毕，本等只罩「子读值 → exit →
    // TERMINATED」尾段；丢唤子永睡 = 帽到点标记缺席）。
    for i in 0..N {
        let mut obs = 0u64;
        if berry_object_wait_one(hps[i], SIGNAL_TERMINATED, now() + DPG_CAP_NS, &mut obs) != 0
            || obs != SIGNAL_TERMINATED
        {
            return Some(3);
        }
        let mut code: i32 = 0;
        if berry_process_get_exit_code(hps[i], &mut code) != 0 {
            return Some(3);
        }
        if code != DPG_OK_CODE {
            return Some(3);
        }
    }
    // 收口（dpg_storm 同序：先 unmap demand 再 unmap 像 → 关柄——奇数子
    // 多一 spread 柄；Vmar Drop 注销 asid 槽 = C-4 回落面）。收口非 0 =
    // 内核侧异常，loud。
    for i in 0..N {
        if berry_vmar_unmap(hvs[i], DPG_VA, 4096) != 0
            || berry_vmar_unmap(hvs[i], DPG_ENTRY, DPG_MAP_LEN) != 0
            || (hts2[i] != 0 && berry_handle_close(hts2[i]) != 0)
            || berry_handle_close(hts[i]) != 0
            || berry_handle_close(hps[i]) != 0
            || berry_handle_close(hvs[i]) != 0
        {
            trap();
        }
    }
    None
}

/// 波失败相位 → 降级行（wave 号 + 相位定位失败点）。
fn dpgf_note(console: Handle, wave: u8, phase: u8) {
    let tag: &[u8] = match (wave, phase) {
        (1, 1) => b"dpgf1 sp",
        (1, 2) => b"dpgf1 sup",
        (1, 3) => b"dpgf1 join",
        (2, 1) => b"dpgf2 sp",
        (2, 2) => b"dpgf2 sup",
        _ => b"dpgf2 join",
    };
    eco_note(console, tag);
}

/// G4-3 C-full 自验：**缺页 soak 满配 N=15**（`DPGF ok n=15`）+ C-4 槽
/// 回收二轮（`DPGF2 ok n=3`）——16 槽登记恰满（boot 1 + 15）+ K=8 池溢出
/// 首压 + CONC-1 真跨核（奇数子 spread 两核 faulter）+ MECH-1 convoy
/// 轮供 + MECH-3 满配时序（join 帽内收敛 + ovfl 计数有界——宿主侧断言，
/// 帽待首跑实测定）。**门 = cpio 具名文件 `dpgf.on`**（fw_cfg opt/berry/*
/// 仅内核可读、用户态无探测 syscall → cpio 直查；正装 rootfs/run/smoke/
/// EFI 缺席 = 静默跳过零扰动零新标记）。链内坏面 = eco 降级行 + 标记缺失
/// （smoke FAIL 兜底）。
fn dpgf_selftest(console: Handle) {
    let _pager_hold = PagerHold::new(); // X-3c-3：同 dpg_selftest——自供期间 pager 让位
    if cpio_find(H_ROOTFS, b"dpgf.on").is_none() {
        return;
    }
    // 共享三 VMO（dpg_storm 同形：源 eager 预写 magic / 像 eager 内嵌 /
    // demand 同页 ×N asid 反查登记——15 子同 dv 页 = 15 请求，K=8 池恰
    // 溢出 7 convoy）。
    let mut sv = 0 as Handle;
    if berry_vmo_create(4096, 0, &mut sv) != 0 || sv == 0 {
        eco_note(console, b"dpgf sv");
        return;
    }
    if berry_vmo_commit(sv, 0, 4096) != 0
        || berry_vmo_write(sv, DPG_MAGIC.to_le_bytes().as_ptr(), 0, 8) != 0
    {
        eco_note(console, b"dpgf svw");
        return;
    }
    let mut iv = 0 as Handle;
    if berry_vmo_create(DPG_MAP_LEN as u64, 0, &mut iv) != 0 || iv == 0 {
        eco_note(console, b"dpgf iv");
        return;
    }
    if berry_vmo_commit(iv, 0, DPG_MAP_LEN as u64) != 0
        || berry_vmo_write(iv, DPGP_BIN.as_ptr(), 0, DPGP_BIN.len()) != 0
    {
        eco_note(console, b"dpgf ivw");
        return;
    }
    let mut dv = 0 as Handle;
    if berry_vmo_create(4096, VMO_OPT_DEMAND, &mut dv) != 0 || dv == 0 {
        eco_note(console, b"dpgf dv");
        return;
    }
    // 波 1：满配 15（登记恰满 + 溢出 convoy 首压）。
    if let Some(p) = dpgf_wave::<DPGF_N>(iv, sv, dv) {
        dpgf_note(console, 1, p);
        return;
    }
    send_retry(console, DPGF_OK.as_ptr(), DPGF_OK.len());
    // C-4 波 2：登记回落 1（波 1 全子 Vmar Drop）→ 再登记 3 Ok——「并发
    // 帽非终身帽」定谳活证。
    if let Some(p) = dpgf_wave::<DPGF2_N>(iv, sv, dv) {
        dpgf_note(console, 2, p);
        return;
    }
    send_retry(console, DPGF2_OK.as_ptr(), DPGF2_OK.len());
    // 收口三 VMO（dv 钉已随两波子 unmap 归零——关面不 -EBUSY）。
    if berry_handle_close(dv) != 0
        || berry_handle_close(sv) != 0
        || berry_handle_close(iv) != 0
    {
        trap();
    }
}

/// smoke 第 24 标记（7 字节，尾插随合并序）。
const ECO_OK: &[u8; 7] = b"ECO ok\n";

/// rootfs 相第 9 标记（Plan B v0.5，8 字节）：stdhello 真退出（0x33）+
/// svcmgr reap 全回收链的确定性验收面（`ChildHandles::reap` 成功才打）。
const EXIT_OK: &[u8; 8] = b"EXIT ok\n";

/// rootfs 相第 14 标记（P6 A-4）：vblk 常驻块设备服务的消费自验——512B
/// 确定性 pattern 经服务写 LBA5 → 读回 → 逐字节对拍全等才打（带参形态
/// 防子串混叠——`FS ok n=` 同款惯例；**状态谓词，无竞速面**）。
const BLKD_OK: &[u8] = b"BLKD ok n=512\n";

/// BLKD 应答帽：60s（vblk 启动自验在服务循环**之前**——boot 期在途可达
/// 数秒；超帽 = 降级行 + 标记缺失 = smoke FAIL，不重发[请求可能仍在途，
/// 重发 = 重复写]）。
const BLKD_REPLY_CAP: u64 = 60 * 1_000_000_000;

/// BLKD 自验扇区（LBA5——避开 justfile/smoke 落盘印章 LBA0 与 vblk 启动
/// 自验 LBA1；确定性 pattern 跨相幂等重写，对拍恒过）。
const BLKD_TEST_LBA: u64 = 5;

/// commit 整页粒度（内核 `Vmo::commit` 的 Unaligned 执法——offset/len 都
/// 须页对齐；512B 扇区 → 取整到 4096，vmo_create 容量本就取整同值）。
const BLKD_COMMIT_LEN: u64 = ((SECTOR_SZ as u64 + 4095) / 4096) * 4096;

/// rootfs 相第 18 标记（P6 A-8c）：vnet 常驻网络服务的消费自验——
/// `DNS_QUERY("localhost")` RPC 往返 st==ST_OK 且 addr==[127,0,0,1]（SLIRP
/// 虚拟 DNS 10.0.2.3 → 宿主 /etc/hosts，离线确定无外网依赖）才打（带参
/// 形态防子串混叠——`BLKD ok n=` 同款惯例；状态谓词，无竞速面）。
const DNS_OK: &[u8] = b"DNS ok addr=127.0.0.1\n";

/// DNS 应答帽：120s。worst 路径 = vnet 启动自验尾巴（ARP+ICMP 在 serve()
/// **之前**，早到的请求入队等待——主走查的阻塞 reap 通常已覆盖这段，但
/// 帽不押时序）+ 服务端 DNS 整体帽 20s（vnet `DNS_CAP_NS`——SLIRP 失联
/// 时 ST_TIMEOUT 应答 20s 到点，慢 ≠ 死由本帽兜底）+ 余量。超帽 = 降级行
/// + 标记缺失 = smoke FAIL（请求可能仍在途，**不重发**——v1 无请求序号，
/// blk_rpc 同款纪律）。
const DNS_REPLY_CAP: u64 = 120 * 1_000_000_000;

/// rootfs 相第 20 标记（T-d，`docs/TCP栈设计草案.md` §7）：vnet TCP 三 op
/// 服务消费自验——OPEN(18080) → ROUNDTRIP 64B 逐字节对拍 → CLOSE 三步全
/// ST_OK 才打。**与 vnet 自验标记 `TCP ok echo=` 互非子串**（第 4 字符
/// 'S' vs ' '——自验走 `Sess` 直连、本标记走 netdproto RPC 服务链，「栈
/// 服务」口径的区分面）；状态谓词，无竞速面。
const TCPS_OK: &[u8] = b"TCPS ok echo=64\n";

/// TCP 应答帽：120s（DNS 同值。「client 帽 > 服务帽」纪律：vnet 侧三 op
/// 各相位帽 OPEN 20s / ROUNDTRIP 60s / CLOSE 60s，逐 op 各等各的应答——
/// 服务活着只是慢时 ST_TIMEOUT 应答最迟 ~60s 到，本帽只兜 vnet 整个挂死
/// 的面）。超帽 = 降级行 + 标记缺失 = smoke FAIL（不重发——串行 RPC 队列
/// 里可能滞留迟到应答错配下一请求，v1 无请求序号，blk_rpc 同款纪律）。
const TCPS_REPLY_CAP: u64 = 120 * 1_000_000_000;

/// 宿主回显服务端口（`tools/echod.py` bind 127.0.0.1:18080 定值——SLIRP
/// guest 主动出站 10.0.2.2 同端口转发；vnet tcp.rs `ECHO_PORT` 镜像值）。
const TCPS_ECHO_PORT: u16 = 18080;

/// TCP 回显 pattern（64B，与 vnet tcp.rs `PATTERN` 同串——两端各自内嵌，
/// 改任一侧须同步重验）。
const TCPS_PATTERN: &[u8; 64] =
    b"berryos-tcp-probe-0123456789012345678901234567890123456789abcdef";
const _: () = assert!(TCPS_PATTERN.len() == 64);

/// rootfs 相第 25 标记（C-2a，VM 草案 §C-2a）：uutils core 家族单子验收
/// ——上游 uu_* crate（0.12.0 锁步，MIT）12 命令薄 harness 接入后，
/// mkdir→touch→cp→mv→ls→cat→rm×2→rmdir 九步串行链全过才打（每步 = fs
/// gift 拉起 + 私有 stdout EOF 收齐 + 阻塞 reap 真码 0；ls 加子串断言、
/// cat 加恰空断言）。单消息原子、状态谓词无竞速面；空文件 roundtrip 盖
/// 元数据五 op RPC 全链，分片数据面经 C-2b shell 面验收。
const U1_OK: &[u8; 6] = b"U1 ok\n";

/// U1 单步 EOF 活动帽：60s（app 子同档——RPC 链慢而活着不误杀，每次收包
/// 刷新；真断链 = 帽尽降级 + 标记缺失 = smoke FAIL）。
const U1_EOF_CAP: u64 = 60 * 1_000_000_000;

// rootfs ELF 流式拉起 = `berry_exec::spawn_rootfs_elf`（A-2 迁出：ELF 首窗
// 解析 → 页铺平单 VMO → 段泵送 → 栈+保护页 → env 块交接；签名 SpawnSpec
// 化 + h_rootfs 参数化——语义纪律与契约文档见 berry-exec/src/lib.rs）。

/// hexyl 终验（d-3c，D4 路线①终验）：收齐子 stdout 全量 → 与宿主基准
/// `HEXYL_PLAIN` 逐字节比对 → 全等才打 `HEXYL ok`（smoke rootfs 相标记）。
/// **EOF = libc exit 族的 `send_stdout_eof` 零长消息**（d-3b 写侧约定；Plan B
/// v0.5 起 exit/_exit/abort 真退出 0x33 前必发——EOF 后子进程即退出）——确定性
/// 终点，不赌超时；10s **活动帽**（每次收包刷新）兜底断链/卡死路径
/// （无 EOF → 打可见降级行，标记缺失 = smoke FAIL）。
/// 比对通过后全表回放 console（人类可读面；与稳态矩阵插花无碍——已非
/// 标记）。
/// X-5 起 EOF 等待帽 = [`HEXYL_EOF_CAP`]（180s——demand 逐页往返装载期，
/// 见其注；10s 帽时代 = eager 全幅预拷即完）。
fn verify_hexyl(h: Handle, console: Handle) {
    // SAFETY: svcmgr 单线程独占（同 ELF_WIN/PUMP_WIN 纪律）。
    let cap: &mut [u8] =
        unsafe { core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(HEX_WIN) as *mut u8, 4096) };
    let mut got = 0usize;
    let mut msg = [0u8; 256];
    let mut eof = false;
    while !eof {
        let mut nb = msg.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            if nb == 0 {
                eof = true;
            } else {
                if got + nb > cap.len() {
                    send_retry(console, b"hexyl overflow\n".as_ptr(), 15);
                    return; // 超收 = 输出面已不等，比对无意义
                }
                cap[got..got + nb].copy_from_slice(&msg[..nb]);
                got += nb;
            }
            continue;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        // 空队：阻塞等 READABLE（10s 活动帽，收包刷新——慢而不僵不误杀）
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + HEXYL_EOF_CAP, &mut obs) != 0 {
            // X-5 踢门（u1 同款：复活续收——hexyl 需百页级供给，一轮踢门
            // ~5 页，多轮 wait→kick 循环缓爬；死寂才降级）。
            if stall_kick(console, b"hexyl", h) {
                continue;
            }
            send_retry(console, b"hexyl: no EOF\n".as_ptr(), 14);
            return; // EOF 永不来 = 子进程断链/卡死（Plan B 起 exit 族必经 EOF——abort 亦先行）——loud 降级
        }
    }
    if got == HEXYL_PLAIN.len() && cap[..got] == HEXYL_PLAIN[..got] {
        send_retry(console, b"HEXYL ok\n".as_ptr(), 9);
        // X-6 观测行（hexyl = demand 面主消费者——clap/fmt 链 ~150-250 页，
        // 装载期末 pg/sw 快照即吞吐判读锚）。
        x6_mark(console);
    } else {
        // 首差诊断行（hex4 两数：got/want 长度；逐字节定位交给离线复盘）
        let mut line = *b"hexyl mismatch got=0000 want=0000\n";
        put_hex4(&mut line[19..23], got);
        put_hex4(&mut line[30..34], HEXYL_PLAIN.len());
        send_retry(console, line.as_ptr(), line.len());
        return; // 不回放错表（错表 + 回放 = 两个混淆源）
    }
    // 全表回放（12×192B 分片，与稳态矩阵插花无碍——终验已过，纯展示）
    let mut off = 0usize;
    while off < got {
        let n = core::cmp::min(192, got - off);
        // SAFETY: cap 只读视图的分片指针。
        send_retry(console, unsafe { cap.as_ptr().add(off) }, n);
        off += n;
    }
}

/// 4 位十六进制落字（got/want 诊断行用；值 < 0x10000 由 4KiB 窗保证）。
fn put_hex4(out: &mut [u8], v: usize) {
    for i in 0..4 {
        out[i] = b"0123456789abcdef"[(v >> (4 * (3 - i))) & 0xf];
    }
}

/// 对端回显验收：wait READABLE → recv（-EAGAIN 重等）→ 低 32 位比对。
fn roundtrip(h: Handle, buf: &mut [u8; 8], req: &[u8; 8]) {
    wait(h, SIGNAL_READABLE);
    loop {
        let mut nb = 8usize;
        let mut nh = 0usize;
        let s = berry_channel_recv(h, buf.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        wait(h, SIGNAL_READABLE);
    }
    if buf[..4] != req[..4] {
        trap(); // 回显低 32 位不符（期望 0xE000_0E30）
    }
}

/// 发送（满 -EBUSY → wait WRITABLE 腾位重试；其余自陷）。
fn send_retry(h: Handle, buf: *const u8, len: usize) {
    loop {
        let s = berry_channel_send(h, buf, len, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            trap();
        }
        wait(h, SIGNAL_WRITABLE);
    }
}

/// console 发送 = [`send_retry`] 阻塞形态（P5 A-2 起——asm 时代的自旋豁免
/// 撤销）：82fe305 后全场生产者皆阻塞、WRITABLE 唤醒队列公平；自旋者在大半
/// 任务阻塞的稳态下反而被 RR 环相位锁死在填槽序末位（A-2 实踩：console
/// wait_many 恒满足项使其永不入睡、drain 相位钉死 → 自旋的 `SVC ok` 每片只
/// 见满队、结构性饿死）。G-eco-b 的「boot 竞速」前提（自旋友商抢槽）已随
/// 82fe305 全场阻塞化消失。

/// 阻塞等信号（10s 绝对帽；超时/错码自陷——对端断供即链断）。
/// S-6 ③ 临终诊断：自陷前把柄/信号/错码一行单发 console（非阻塞单发，
/// console 本身即断供源时发不出即罢——不递归 send_retry/wait）。定位
/// 「谁断供」用；定谳后此行退役或转常设 low-noise 诊断。
fn wait(h: Handle, signals: u64) {
    let mut obs = 0u64;
    let s = berry_object_wait_one(h, signals, now() + TIMEOUT_NS, &mut obs);
    if s != 0 {
        let c = CONSOLE_H.load(core::sync::atomic::Ordering::Relaxed);
        if c != 0 {
            let mut line = *b"WTDIE h=0x00000000 sig=0x0000 e=0x0000\n";
            put_hex4(&mut line[10..14], (h >> 16) as usize);
            put_hex4(&mut line[14..18], (h & 0xffff) as usize);
            put_hex4(&mut line[25..29], signals as usize);
            put_hex4(&mut line[34..38], s as usize);
            let _ = berry_channel_send(c as Handle, line.as_ptr(), line.len(), core::ptr::null(), 0);
        }
        trap();
    }
}

/// now = clock_read（失败自陷）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}

/// P6 A-4：vblk 服务消费自验——512B 确定性 pattern 经服务写 LBA5 → 读回
/// → 逐字节对拍（Err(why) = 降级行素材；Ok 才打标记）。数据面 = VMO 随行
/// 迁移：dup 别名随请求 send（take 走）、本体留本表供读回对拍，driver
/// bounce-buffer 后 close 别名——「能力经消息交付」在数据面的第一用。
fn blk_selftest(blk: Handle) -> Result<(), &'static [u8]> {
    let mut pat = [0u8; SECTOR_SZ];
    for (i, b) in pat.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(31).wrapping_add(7);
    }
    let mut vmo = 0 as Handle;
    if berry_vmo_create(SECTOR_SZ as u64, 0, &mut vmo) != 0 || vmo == 0 {
        return Err(b"blkd vmo");
    }
    let r = blk_stages(blk, vmo, &pat);
    let _ = berry_handle_close(vmo);
    r
}

/// 自验三段：预填 pattern → 写 LBA5 → **VMO 本体置反值 0xA5** → 读 LBA5 →
/// 读回对拍（反值刷本体 = 假绿防线的主位：服务 Read 臂若回 ST_OK 而未回
/// 填别名，读回的是写段残留的 pattern 旧值——先把本体刷成反值才能显形；
/// got 栈缓冲预置 0xA5 只是纵深，成功读回必然全量覆写它）。
fn blk_stages(blk: Handle, vmo: Handle, pat: &[u8; SECTOR_SZ]) -> Result<(), &'static [u8]> {
    if berry_vmo_commit(vmo, 0, BLKD_COMMIT_LEN) != 0
        || berry_vmo_write(vmo, pat.as_ptr(), 0, SECTOR_SZ) != 0
    {
        return Err(b"blkd stage");
    }
    match blk_rpc(blk, OP_WRITE, BLKD_TEST_LBA, vmo) {
        Ok(ST_OK) => {}
        _ => return Err(b"blkd write"),
    }
    // 假绿防线（对抗审查实抓：原防线错位在会被全量覆写的 got 栈缓冲上）
    // ——严格串行 RPC 下此刻写段已毕（driver 早已从别名读走 pattern），
    // 刷本体不扰落盘数据。
    let inv = [0xA5u8; SECTOR_SZ];
    if berry_vmo_write(vmo, inv.as_ptr(), 0, SECTOR_SZ) != 0 {
        return Err(b"blkd stage");
    }
    match blk_rpc(blk, OP_READ, BLKD_TEST_LBA, vmo) {
        Ok(ST_OK) => {}
        _ => return Err(b"blkd read"),
    }
    let mut got = [0xA5u8; SECTOR_SZ];
    if berry_vmo_read(vmo, got.as_mut_ptr(), 0, SECTOR_SZ) != 0 {
        return Err(b"blkd verify");
    }
    if got != *pat {
        return Err(b"blkd cmp");
    }
    Ok(())
}

/// 严格串行 RPC（一请求一应答，`berry-blkproto` 约定）：dup VMO 别名随请求
/// send（EBUSY → wait WRITABLE 重试；take 只发生在成功路径——EBUSY 时别名
/// 未被取走，可原值重发）→ 60s 应答帽 wait READABLE → recv 恰 1B 解析。
/// 应答帽超时/错码 → Err（请求可能仍在途，**不重发**——重发 = 重复写）。
fn blk_rpc(h: Handle, op: u8, sector: u64, vmo: Handle) -> Result<u8, ()> {
    let mut req = [0u8; 16];
    let n = match encode_request(&mut req, op, sector) {
        Some(n) => n,
        None => return Err(()),
    };
    let mut dup = 0 as Handle;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_WRITE, &mut dup) != 0 {
        return Err(());
    }
    let hs = [dup];
    loop {
        let s = berry_channel_send(h, req.as_ptr(), n, hs.as_ptr(), 1);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            // 非 EBUSY 失败：别名未被 take——显式回收防表槽泄漏。
            let _ = berry_handle_close(dup);
            return Err(());
        }
        wait(h, SIGNAL_WRITABLE);
    }
    // 应答帽 60s（vblk 启动自验在服务循环前——boot 在途可达数秒；普通
    // wait 是 10s，这里宽帽）。
    let mut obs = 0u64;
    if berry_object_wait_one(h, SIGNAL_READABLE, now() + BLKD_REPLY_CAP, &mut obs) != 0 {
        return Err(());
    }
    let mut rep = [0u8; 8];
    let mut nb = rep.len();
    let mut nh = 0usize;
    if berry_channel_recv(h, rep.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh) != 0 {
        return Err(());
    }
    parse_reply(&rep[..nb]).ok_or(())
}

/// eco 降级行（run_rootfs_child 内 `eco_skip` 闭包的独立 fn 版——A-8c 的
/// DNS 自验在 `_start` 语境（闭包作用域外）消费，形态逐字同源）。
fn eco_line(console: Handle, why: &[u8]) {
    let mut line = [0u8; 24];
    line[..4].copy_from_slice(b"eco:");
    line[4] = b' ';
    let n = 5 + why.len();
    line[5..n].copy_from_slice(why);
    line[n] = b'\n';
    send_retry(console, line.as_ptr(), n + 1);
}

/// app 降级行 `eco: app <id> <why>`——与 eco_skip/eco_line 分立：那两窗
/// 24B 装不下带 id 的形态，本行独立 64B 窗（最大 9+24+1+24+1 = 59）。
/// id ≤ ID_MAX=24（收集臂执法）、why ≤ 24B（现役理由全 ASCII 短语 +
/// `reason_str()` 截断契约 ≤24B）——min 双防御 = eco 面永不 panic。
fn eco_app(console: Handle, id: &[u8], why: &[u8]) {
    let mut line = [0u8; 64];
    line[..9].copy_from_slice(b"eco: app ");
    let mut n = 9;
    let id = &id[..id.len().min(24)];
    line[n..n + id.len()].copy_from_slice(id);
    n += id.len();
    line[n] = b' ';
    n += 1;
    let why = &why[..why.len().min(24)];
    line[n..n + why.len()].copy_from_slice(why);
    n += why.len();
    line[n] = b'\n';
    send_retry(console, line.as_ptr(), n + 1);
}

/// app EOF 收集帽：60s。worst 路径 = 子 main 全程 + EOF 排队（demo app
/// 毫秒级；30s = 断链/卡死兜底——超帽 `no EOF` 降级行，reap 兜底收口）。
const APP_EOF_CAP: u64 = 60 * 1_000_000_000;

/// 收集端等零长 EOF（`verify_hexyl` 的 drain 骨架去内容面）：s==0 且
/// nb==0 → true；有数据照收丢弃（appquiet 契约输出为零——防御面：万一
/// 多打不炸，EOF 终会来）；nh>0 = 坏 .bap 多带 handle——按数据面处置：
/// 令牌已入本表，全量 close 丢弃（P6 A-4 纪律：收口容量给「协议定值 +
/// 富余」——channel 随行上限 6（T-1：CHAN_HND_MAX 4→6），容量 6 = 任何消息
/// 不 -EINVAL；恰按 0 收
/// 时坏消息容量不足回滚返 -EINVAL，会误入链断 trap 杀 svcmgr——「常驻
/// 服务被单个坏 .bap 杀死」是最差死法）；非 -EAGAIN 错码 = 端点/表异常
/// → trap（链断 loud，verify_hexyl 同口径）；空队 wait READABLE 超帽返
/// false。
fn collect_eof(h: Handle) -> bool {
    let mut msg = [0u8; 256];
    let mut hh = [0 as Handle; 6]; // 随行收口容量 = channel 上限（富余非协议值）
    loop {
        let mut nb = msg.len();
        let mut nh = hh.len();
        let s = berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, hh.as_mut_ptr(), &mut nh);
        if s == 0 {
            for i in 0..nh {
                let _ = berry_handle_close(hh[i]); // 多带 handle：丢弃勿 trap
            }
            if nb == 0 {
                return true; // 零长消息 = EOF 约定（d-3b）
            }
            continue;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + APP_EOF_CAP, &mut obs) != 0 {
            return false;
        }
    }
}

// ---- C-2a：U1（uutils core 家族单子验收）四件套 ----

/// U1 步链（九步串行；任一步失败打可见诊断行返 false——`U1 ok` 缺失 =
/// smoke FAIL loud）。诊断行 step 数字 ↔ STEPS 下标钉死。ls = 输出含
/// `src.txt`+`mvd.txt` 双子串（上游列名/格式不钉板——「人有我强」买的是
/// GNU 兼容面，不是字节基准）；cat = 输出恰空（空文件零输出 + 真码 0）。
/// 全链只造/搬/删**空文件与目录**——12 命令无内容写路径（touch 空建、cp
/// 复制既有），分片数据面（PUT/GET chunk）留 C-2b。
fn u1_selftest(console: Handle, fs_a: Handle) -> bool {
    // (cpio 名, argv[1..], ls 子串断言, 输出恰空断言)。ls 显式
    // `--color=never`：uu_ls 0.12 默认 Color=Auto，非 tty 虽应回落 plain，
    // 显式关掉防上游行为漂移（hexyl 同款口径）。
    const STEPS: &[(&[u8], &[&[u8]], bool, bool)] = &[
        (b"bin/mkdir", &[b"u1d"], false, false),
        (b"bin/touch", &[b"u1d/src.txt"], false, false),
        (b"bin/cp", &[b"u1d/src.txt", b"u1d/dst.txt"], false, false),
        (b"bin/mv", &[b"u1d/dst.txt", b"u1d/mvd.txt"], false, false),
        (b"bin/ls", &[b"--color=never", b"u1d"], true, false),
        (b"bin/cat", &[b"u1d/src.txt"], false, true),
        (b"bin/rm", &[b"u1d/src.txt"], false, false),
        (b"bin/rm", &[b"u1d/mvd.txt"], false, false),
        (b"bin/rmdir", &[b"u1d"], false, false),
    ];
    const _: () = assert!(STEPS.len() < 10, "u1 diag step 位单数字");
    for (i, (name, args, want_subs, want_empty)) in STEPS.iter().enumerate() {
        let code = match uu_run(console, fs_a, name, args) {
            None => {
                u1_diag(console, i, 0xEE00);
                return false;
            }
            Some((c, len)) => {
                if c == 0 && *want_subs {
                    // SAFETY: svcmgr 单线程独占（HEX_WIN 同纪律）。
                    let out: &[u8] = unsafe {
                        core::slice::from_raw_parts(core::ptr::addr_of!(U1_WIN) as *const u8, len)
                    };
                    if !has_sub(out, b"src.txt") || !has_sub(out, b"mvd.txt") {
                        u1_diag(console, i, 0xEE01);
                        return false;
                    }
                }
                if c == 0 && *want_empty && len != 0 {
                    u1_diag(console, i, 0xEE02);
                    return false;
                }
                c
            }
        };
        if code != 0 {
            u1_diag(console, i, code as usize);
            return false;
        }
    }
    true
}

/// U1 单子拉起：fs gift（[rootfs dup, fs 客户端 dup]——gift[2]/[3] 槽位
/// 语义同 bsh，libc `rootfs_h()`/`fs_h()` 消费，stdhello 预拉同款）+ 私有
/// stdout（收进 [`U1_WIN`]）+ 阻塞 reap，返 Some((真码, 长度))。失败（档
/// 缺/spawn Err/无 EOF/reap Err）返 None——步链上层打诊断行（arm 不自
/// 陷：标记缺失已 loud，诊断留给 u1_diag 汇总）。
/// X-5 停滞踢门（no-EOF 超帽面专用，happy path 零面）：一次手动供给扫
/// （pager_main 内环同形——0x23 逐活槽，全 identity 验）+ 5s 二次 READABLE
/// 等。复活（true）= 供给链竞速丢服务（内核 el0 pager 唤醒面待修——踢门
/// 即自愈：arm 在池可服务，唯 pager_main 久未轮到；2026-11-07 三跑三形
/// 实证 + 踢门复现腿定谳），调用方续收不降级；死寂（false）= 子不在缺页
/// 等待（IPC 等待/更深处），降级路径走僵尸收口。行前缀参数化（u1/hexyl
/// 两用）；在册窗数一并打（注册表面取证——滞子窗恒在册，0 = 注册已丢）。
fn stall_kick(console: Handle, tag: &[u8], ch: Handle) -> bool {
    // 单发踢门：只打**首个活槽**一次 0x23。疗效不在供一页，而在 take→
    // 唤 faulter→其重武装→唤 el0_pager_key 这条链把「卡睡在旧 0x23 里
    // 的 pager_main」重新叫醒（丢唤醒后首个新 arm 即复活 pager，后续全
    // 速续供——hexyl 百页级也一轮即愈）。**单发纪律**：0x23 空池无限
    // 阻塞——多槽扫会在 arm 耗尽时把本任务吊死；单发只碰确知在池的
    // 那个 arm，空池窗 = 超帽→踢门间 μs 级（且该窗内 pager 已自愈，
    // 任意后续 arm 都会放行本调用）。窗数普查（纯原子读零调用）随行。
    let mut n = 0usize;
    let mut first = (0 as Handle, 0 as Handle, 0i64);
    for slot in PAGER_REQS.iter() {
        if !slot.active.load(Ordering::Acquire) {
            continue;
        }
        if n == 0 {
            first = (
                slot.dv.load(Ordering::Relaxed) as Handle,
                slot.sv.load(Ordering::Relaxed) as Handle,
                slot.delta.load(Ordering::Relaxed),
            );
        }
        n += 1;
    }
    // 分拣仪三件套：0x23 返码（0=供上 / 负 errno：EBADF 陈旧句柄；EINVAL
    // 刀前=全局取臂错配消耗（repro4 三发实锚）/ 过滤取刀 72e5789 起=本窗
    // 无武装臂 NoneForVd 快跳——舞已灭）+ 圈数快照（冻结=S2 pager 楔死；
    // 爬升=活着——语义历两刀不变）。
    let mut rc = 0i64;
    let sw0 = PAGER_SWEEPS.load(Ordering::Relaxed);
    if n > 0 {
        rc = berry_pager_supply_mapped(first.0, first.1, first.2) as i64;
    }
    // 2s 圈数采样窗（兼当早收哨）：pager 若活着，活跃让步 1ms/圈必爬升。
    let mut obs = 0u64;
    let _ = berry_object_wait_one(ch, SIGNAL_READABLE, now() + 2_000_000_000, &mut obs);
    let sw1 = PAGER_SWEEPS.load(Ordering::Relaxed);
    let ok = berry_object_wait_one(ch, SIGNAL_READABLE, now() + 5_000_000_000, &mut obs) == 0;
    let head: &[u8] = if ok {
        b": kick ok rc="
    } else {
        b": kick dead rc="
    };
    let mut line = [0u8; 72];
    let mut len = 0usize;
    line[..tag.len()].copy_from_slice(tag);
    len += tag.len();
    line[len..len + head.len()].copy_from_slice(head);
    len += head.len();
    if rc < 0 {
        line[len] = b'-';
        len += 1;
    }
    len = push_dec(&mut line, len, rc.unsigned_abs());
    line[len..len + 4].copy_from_slice(b" sw=");
    len += 4;
    len = push_dec(&mut line, len, sw0);
    line[len] = b'/';
    len += 1;
    len = push_dec(&mut line, len, sw1);
    line[len..len + 5].copy_from_slice(b" win=");
    len += 5;
    line[len] = b'0' + (n / 10) as u8;
    line[len + 1] = b'0' + (n % 10) as u8;
    len += 2;
    line[len] = b'\n';
    len += 1;
    send_retry(console, line.as_ptr(), len);
    ok
}

/// 十进制无符号追加写（stall_kick 分拣仪行组装用；返回新长度）。
fn push_dec(line: &mut [u8], len: usize, mut v: u64) -> usize {
    let mut tmp = [0u8; 20];
    let mut t = 0usize;
    loop {
        tmp[t] = b'0' + (v % 10) as u8;
        t += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for i in 0..t {
        line[len + i] = tmp[t - 1 - i];
    }
    len + t
}

/// X-6 刀0 观测行发射：单行原子 `\nX6 pg=<供给页数> sw=<扫圈数>`（stall_kick
/// 行组装同形——[0u8;72] 帽 + copy_from_slice 定宽前缀 + push_dec 多位十
/// 进制 + 显式换行单发）。恰三点（hello/hexyl/boot 尾各一——bench 冻结
/// 词形契约；stdhello/U1 在首行前 = 黑暗基线不发）；x5_foot/dpg 相位不经
/// boot 验证环，发射点天然零发射；行不带钟——页/s 的钟 = 宿主侧 marker
/// 到达时戳（设计 §5）。行首垫 \n：共享串口并发写者的字节会贴本行头
/// （smoke 实录 boot 尾 `pX6 pg=…`，bench `^X6` 行首锚失配 → 恰三点计数
/// 变 2 假 FAIL；垫 \n 恒使 X6 行起于行首，夹字节自成垃圾行，冻结正则
/// 一字不动）。垫在单发内不拆两写——夹字节只发生在写与写之间（实录
/// 行内容自身完好），拆开反把缝开在自家 \n 与 X6 之间。
fn x6_mark(console: Handle) {
    let mut line = [0u8; 72];
    line[0] = b'\n';
    line[1..7].copy_from_slice(b"X6 pg=");
    let mut len = 7usize;
    len = push_dec(&mut line, len, PAGER_SUPPLIED.load(Ordering::Relaxed));
    line[len..len + 4].copy_from_slice(b" sw=");
    len += 4;
    len = push_dec(&mut line, len, PAGER_SWEEPS.load(Ordering::Relaxed));
    line[len] = b'\n';
    len += 1;
    send_retry(console, line.as_ptr(), len);
}

fn uu_run(console: Handle, fs_a: Handle, name: &[u8], args: &[&[u8]]) -> Option<(i32, usize)> {
    let (data_off, data_len) = cpio_find(H_ROOTFS, name)?;
    let (out_s, out_c) = channel_pair();
    let mut rootfs_dup = 0 as Handle;
    if berry_handle_duplicate(H_ROOTFS, RIGHT_READ, &mut rootfs_dup) != 0 {
        trap();
    }
    let mut fs_dup = 0 as Handle;
    if berry_handle_duplicate(fs_a, RIGHT_SEND | RIGHT_RECV, &mut fs_dup) != 0 {
        trap();
    }
    let gifts = [rootfs_dup, fs_dup];
    let child = match spawn_rootfs_elf(
        H_ROOTFS,
        &SpawnSpec {
            name,
            data_off,
            data_len,
            stdout: out_c,
            stack_pages: 32, // clap 解析 + uu 主体栈深（hexyl 同口径，d-3c 实踩）
            stdin_feed: Some(&[]), // gift[1] 占位（立即 EOF；fsmgr/vnet 先例）
            stdin_handle: None,
            args,
            extra_gifts: &gifts,
            mmio_vmo: None,
            mmio_va: 0,
            intent: 0,
            force_eager: false,
        },
    ) {
        Ok(c) => c,
        Err(()) => {
            // Err 三路全在 env send 前——双 dup 未被 take，显式回收（stdhello
            // 预拉同款）；out 对同收（spawn 内 gift[0] 是 dup，原值仍在表）。
            if berry_handle_close(rootfs_dup) != 0 {
                trap();
            }
            if berry_handle_close(fs_dup) != 0 {
                trap();
            }
            if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
                trap();
            }
            return None;
        }
    };
    // 私有 stdout 收齐（EOF = libc exit 族零长消息，d-3b；活动帽 30s——
    // fs RPC 链慢而活着不误杀）。随行 handle 丢弃（collect_eof 同款纪律）。
    // SAFETY: svcmgr 单线程独占（HEX_WIN 同纪律）。
    let cap: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(U1_WIN) as *mut u8, 2048)
    };
    let mut got = 0usize;
    let mut eof = false;
    while !eof {
        let mut msg = [0u8; 256];
        let mut hh = [0 as Handle; 6]; // 随行收口容量 = channel 上限（富余非协议值）
        let mut nb = msg.len();
        let mut nh = hh.len();
        let s = berry_channel_recv(out_s, msg.as_mut_ptr(), &mut nb, hh.as_mut_ptr(), &mut nh);
        if s == 0 {
            for i in 0..nh {
                let _ = berry_handle_close(hh[i]); // 多带 handle：丢弃勿 trap
            }
            if nb == 0 {
                eof = true;
            } else {
                if got + nb > cap.len() {
                    send_retry(console, b"u1: overflow\n".as_ptr(), 13);
                    // 溢出 = 子活跃面：同款 terminate→reap 收口（免僵尸）。
                    let _ = berry_process_terminate(child.hp, 9);
                    let _ = child.reap();
                    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
                        trap();
                    }
                    return None;
                }
                cap[got..got + nb].copy_from_slice(&msg[..nb]);
                got += nb;
            }
            continue;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(out_s, SIGNAL_READABLE, now() + U1_EOF_CAP, &mut obs) != 0 {
            // X-5 踢门（诊断+自愈双面）：复活 = 续收不降级（竞速瞬滞 ~5s
            // 自愈；判决行保留取证）；死寂 = no-EOF 降级 + 僵尸收口。
            if stall_kick(console, b"u1", out_s) {
                continue;
            }
            send_retry(console, b"u1: no EOF\n".as_ptr(), 11);
            let _ = berry_process_terminate(child.hp, 9);
            let _ = child.reap();
            if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
                trap();
            }
            return None;
        }
    }
    let code = match child.reap() {
        Ok(c) => c,
        Err(()) => {
            if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
                trap();
            }
            return None;
        }
    };
    // out_c 本表副本使命已毕（子表持 SEND 别名，子退出 drain 已收）。
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        trap();
    }
    Some((code, got))
}

/// U1 诊断行（`u1: step N code XXXX\n`——step ↔ STEPS 下标；code = 子真
/// 码或 0xEExx 内部断言码：EE00 拉起/EOF/reap 面、EE01 ls 子串缺、EE02
/// cat 非空）。
fn u1_diag(console: Handle, step: usize, code: usize) {
    let mut line = [0u8; 21];
    line[..9].copy_from_slice(b"u1: step ");
    line[9] = b'0' + step as u8;
    line[10..16].copy_from_slice(b" code ");
    put_hex4(&mut line[16..20], code);
    line[20] = b'\n';
    send_retry(console, line.as_ptr(), 21);
}

/// U1 子串断言助手（hay 含 needle 与否；ls 输出面）。
fn has_sub(hay: &[u8], needle: &[u8]) -> bool {
    if needle.len() > hay.len() {
        return false;
    }
    (0..=hay.len() - needle.len()).any(|i| &hay[i..i + needle.len()] == needle)
}

/// .bap 内层走查产物：manifest 实长 + 包内 bin/ 条目名长 + ELF 包内几何
/// （`bap_off` 相对偏移——spawn 直用 `bap_off + elf_off`）+ SHA256SUMS
/// 第三条目在场记账（SDK-1a——v1 只记不读不验，§2.4 三理由）+ GUI-7
/// icons//views/ 条目记账（双窗表 + 计数；体不读——ui_face 按声明才读）。
struct BapScan {
    mf_len: usize,
    entry_len: usize,
    elf_off: u64,
    elf_len: u64,
    has_sums: bool,
    icon_n: usize,
    views_n: usize,
}

/// .bap 内层 cpio 走查（mkapp 生产形状：恰 `manifest.toml` + 唯一 `bin/`
/// 条目 + 可选 `SHA256SUMS`（SDK-1a §2.4 起新产包第三条目常态在场，
/// 2 条目历史形照收）+ GUI-7 `icons/*.byi`/`views/*` 各 ≤4 + TRAILER 自
/// 终止）。rootfs 是外部输入面——消费侧独立执法（mkapp 门禁只约束本仓
/// 产物，拼装盘上的 .bap 不凭生产端承诺）：重复/未知条目一律 `bap shape`
/// 拒。manifest 可达 1KiB > 步进窗 512B——**专用 vmo_read 直读进 mf 窗**
/// （步进窗只须盖头+名）；ELF 只记几何（段泵送在 spawn_rootfs_elf，归档
/// 永不整档驻留）。manifest 读窗预算（附 A）恒安全：APP_MF_WIN =
/// MANIFEST_MAX = 1024B ≥ 最坏 395B（v1 全量段 + GUI-7 [ui] 三行 ≈ +70B）
/// ——SDK-1a 无需扩窗。
fn bap_scan(
    step: &mut [u8],
    mf: &mut [u8],
    entry: &mut [u8],
    icons: &mut [UiEntry],
    views: &mut [UiEntry],
    bap_off: u64,
    bap_len: u64,
) -> Result<BapScan, &'static [u8]> {
    let mut pos = 0u64;
    let mut seen_mf = false;
    let mut seen_bin = false;
    let mut seen_sums = false;
    let mut out = BapScan {
        mf_len: 0,
        entry_len: 0,
        elf_off: 0,
        elf_len: 0,
        has_sums: false,
        icon_n: 0,
        views_n: 0,
    };
    while pos < bap_len {
        let want = ((bap_len - pos).min(step.len() as u64)) as usize;
        if berry_vmo_read(H_ROOTFS, step.as_mut_ptr(), bap_off + pos, want) != 0 {
            return Err(b"bap read");
        }
        match berry_loader::cpio_step(&step[..want], pos, bap_len) {
            Ok(berry_loader::StepOutcome::End) => break,
            Ok(berry_loader::StepOutcome::Entry(s)) => {
                let (name, data_off, data_len, next) =
                    (s.name, s.data_off, s.data_len, s.next_off);
                if name == "manifest.toml" {
                    if seen_mf {
                        return Err(b"bap shape");
                    }
                    if data_len > mf.len() as u64 {
                        return Err(b"bap mf big");
                    }
                    // manifest 直读进专用窗（内层偏移 + 包基址）。
                    if berry_vmo_read(
                        H_ROOTFS,
                        mf.as_mut_ptr(),
                        bap_off + data_off,
                        data_len as usize,
                    ) != 0 {
                        return Err(b"bap read");
                    }
                    out.mf_len = data_len as usize;
                    seen_mf = true;
                } else if name == "SHA256SUMS" {
                    // SDK-1a §2.4 第三条目：mkapp 产包新常态（恰一份；重复
                    // = bap shape 拒）。v1 只记账在场——不读内容不验摘要
                    // （零 no_std sha256 依赖 + 无签名的摘要非安全边界；真
                    // 执法点 = v2 安装期签名验证，挂账 O4）。2 条目形
                    // （SDK-1a 前历史包）照收——seen_sums 缺省即容。
                    if seen_sums {
                        return Err(b"bap shape");
                    }
                    seen_sums = true;
                    out.has_sums = true;
                } else if name.len() > 4 && name.starts_with("bin/") {
                    if seen_bin {
                        return Err(b"bap shape");
                    }
                    if name.len() > entry.len() {
                        return Err(b"bap entry long");
                    }
                    entry[..name.len()].copy_from_slice(name.as_bytes());
                    out.entry_len = name.len();
                    out.elf_off = data_off;
                    out.elf_len = data_len;
                    seen_bin = true;
                } else if name.starts_with(ICON_PREFIX) && name.ends_with(".byi") {
                    // GUI-7：图标条目（扁平 icons/<stem>.byi，stem 非空无
                    // '/'）——记几何不读体（10B 头按声明条目才读，ui_face；
                    // 非 .byi 的 icons/ 名落 else = bap shape 拒）。
                    let stem = &name[ICON_PREFIX.len()..name.len() - 4];
                    if stem.is_empty() || stem.contains('/') || name.len() > UI_PATH_MAX {
                        return Err(b"bap shape");
                    }
                    if out.icon_n >= icons.len() {
                        return Err(b"bap shape");
                    }
                    let e = &mut icons[out.icon_n];
                    e.name[..name.len()].copy_from_slice(name.as_bytes());
                    e.name_len = name.len();
                    e.off = data_off;
                    e.len = data_len;
                    out.icon_n += 1;
                } else if name.len() > VIEWS_PREFIX.len() && name.starts_with(VIEWS_PREFIX) {
                    // GUI-7：视图条目（扁平 views/<名>，非空非嵌套）——原样
                    // 记账，内容 v1 opaque（DSL 定形 SDK-2）。
                    let tail = &name[VIEWS_PREFIX.len()..];
                    if tail.contains('/') || name.len() > UI_PATH_MAX || data_len == 0 {
                        return Err(b"bap shape");
                    }
                    if out.views_n >= views.len() {
                        return Err(b"bap shape");
                    }
                    let e = &mut views[out.views_n];
                    e.name[..name.len()].copy_from_slice(name.as_bytes());
                    e.name_len = name.len();
                    e.off = data_off;
                    e.len = data_len;
                    out.views_n += 1;
                } else {
                    return Err(b"bap shape");
                }
                pos = next;
            }
            Err(_) => return Err(b"bap cpio"),
        }
    }
    if !seen_mf || !seen_bin {
        return Err(b"bap shape");
    }
    Ok(out)
}

// ---- GUI-7：[ui] 验证面（失败隔离——与拉起成败两独立谓词）----

/// 单 app 的 [ui] 验证结果：`Absent`（无声明且包内零 ui 条目）/ `Ok`
/// （有声明且全验过）/ `Bad`（有声明验败，或未声明而包内夹带——diag
/// 行已打）。拉起成败与本面正交（元数据坏不拦执行：`APP ok` 与
/// `APPUI ok` 各自独立计入）。
enum UiFace {
    Absent,
    Ok,
    Bad,
}

/// 按包内名查 ui 条目表（线性 ≤4——条目帽小，无谓散列）。
fn ui_find<'a>(tbl: &'a [UiEntry], want: &[u8]) -> Option<&'a UiEntry> {
    tbl.iter().find(|e| &e.name[..e.name_len] == want)
}

/// [ui] 声明验证：
/// - icon 声明 ⇒ 包内同名 icons/ 条目在席且 BYI1 头合法（magic/version/
///   flags + 单边 ≤1024 + w*h ≤65536 产面帽——只验头不展开 RLE，真解码
///   在 GUI-8 launcher 消费 berry-gfx）；
/// - views 声明 ⇒ 包内同名 views/ 条目在席；
/// - 未声明而条目在场 = mkapp 交叉门（`_ui_cross_check`）的消费侧镜像
///   ——拼装盘防御，`ui stray`。
/// 任何不符 = diag `eco: app <id> ui …` + `Bad`（不拦 spawn）。
fn ui_face(
    console: Handle,
    id: &[u8],
    ui: Option<berry_manifest::Ui<'_>>,
    icons: &[UiEntry],
    views: &[UiEntry],
    bap_off: u64,
) -> UiFace {
    let Some(u) = ui else {
        if icons.is_empty() && views.is_empty() {
            return UiFace::Absent;
        }
        eco_app(console, id, b"ui stray"); // 8B
        return UiFace::Bad;
    };
    let mut ok = true;
    match u.icon {
        Some(want) => match ui_find(icons, want.as_bytes()) {
            Some(e) => {
                // 10B 头直读（条目几何已知；len<10 = 截断头即坏）。
                let mut hdr = [0u8; 10];
                if e.len < 10
                    || berry_vmo_read(H_ROOTFS, hdr.as_mut_ptr(), bap_off + e.off, 10) != 0
                {
                    ok = false;
                    eco_app(console, id, b"ui bad icon"); // 11B
                } else {
                    let w = u16::from_le_bytes([hdr[6], hdr[7]]);
                    let h = u16::from_le_bytes([hdr[8], hdr[9]]);
                    if &hdr[..4] != b"BYI1"
                        || hdr[4] != 1
                        || hdr[5] != 0
                        || w < 1
                        || h < 1
                        || w > APP_ICON_DIM_MAX
                        || h > APP_ICON_DIM_MAX
                        || u32::from(w) * u32::from(h) > APP_ICON_PIXELS_MAX
                    {
                        ok = false;
                        eco_app(console, id, b"ui bad icon");
                    }
                }
            }
            None => {
                ok = false;
                eco_app(console, id, b"ui no icon"); // 10B
            }
        },
        None => {
            // 未声明 icon 而包内夹带 = stray（声明态额外条目 ≤ 帽 tolerated
            // ——mkapp 交叉门同律）。
            if !icons.is_empty() {
                ok = false;
                eco_app(console, id, b"ui stray");
            }
        }
    }
    match u.views {
        Some(want) => {
            if ui_find(views, want.as_bytes()).is_none() {
                ok = false;
                eco_app(console, id, b"ui no views"); // 11B
            }
        }
        None => {
            if !views.is_empty() {
                ok = false;
                eco_app(console, id, b"ui stray");
            }
        }
    }
    if ok {
        UiFace::Ok
    } else {
        UiFace::Bad
    }
}

/// 单 app 拉起（ADR #43 能力面 + §4.3 生命周期 v0：拉起 → 收口 → reap）。
/// console=true → stdout 直打串口（子自证输出，无收集面）；false → 私有对
/// 端作 fd1/2（hexyl 同形：`channel_pair` 的 spawn 侧交付、收集端留本表
/// ——appquiet 静默活证）。reap 真码 0 才算过：argv0 断言错 → abort(134)
/// → 非零 → 降级行 + `APP ok` 缺失 loud。任何失败 = `eco: app` 行 +
/// false，svcmgr 存活续走（失败隔离：单 app 坏不连坐整批）。
///
/// GUI-7：返回 `(拉起成败, UiFace)` 两独立谓词——[ui] 验证在 net 检后
/// spawn 前（纯读零句柄，无清理面）；验证坏不拦 spawn（失败隔离）。
///
/// SDK-1a：`fs_a` = fsmgr 预拉对 a 侧（§1.2 新序下 app 批窗口 svcmgr 仍
/// 持 `fs_a_h`）；fs 授能不可达值 0 传参由 §5.2 `fs none` 降级路径消化。
///
/// GUI-8 段②（对抗审查 M-C 裁定——§5「reap 异步收尾不阻塞回执」落地）：
/// 返回三元 `(拉起成败, UiFace, Option<AppSpawnTail>)`——spawn Ok 后
/// **收口尾外移**（collect_eof 60s + reap 30s 不再内联）：boot 批同步
/// 消费 `app_reap_tail`（行为零漂——批窗口仍单 app 串行收口）；SPAWN
/// RPC 形把尾入 `APP_CHILDREN` 子表，稳态 loop 探活后异步兑现（回执
/// 即刻——GUI app 分钟级活期不冻 echo/监督）。拉起成败 = spawn 面
/// 谓词（退出真码归收口尾，不进本值）。
fn app_launch(
    console: Handle,
    fs_a: Handle,
    id: &[u8],
    bap_off: u64,
    bap_len: u64,
    step: &mut [u8],
    mf: &mut [u8],
    icons: &mut [UiEntry],
    views: &mut [UiEntry],
) -> (bool, UiFace, Option<AppSpawnTail>) {
    let mut entry = [0u8; ENTRY_MAX];
    let sc = match bap_scan(step, mf, &mut entry, icons, views, bap_off, bap_len) {
        Ok(sc) => sc,
        Err(why) => {
            eco_app(console, id, why);
            return (false, UiFace::Absent, None);
        }
    };
    let man = match berry_manifest::parse(&mf[..sc.mf_len]) {
        Ok(m) => m,
        Err(e) => {
            eco_app(console, id, e.reason_str().as_bytes());
            return (false, UiFace::Absent, None);
        }
    };
    // ADR #82 V1：type 位收集槽过滤——berry-manifest 词表已收 "driver"
    // （收格式不消费），但 appd（雏形）只服务 app：driver 包的拉起语义
    // 随驱动模型 v2/devmgr（[hw] 节未立法，mkapp 产面不同步 = 现无合法
    // 产出，本滤 = 防拼装盘上的 driver 包开机被当应用拉起的消费侧闸——
    // parser 放行后 type 的唯一运行期闸门，11 号证伪修正）。boot 批与
    // APP_SPAWN RPC 同走本守卫（裁决二「守卫单源」——app_serve_spawn
    // 注）。loud 降级行（net unserved 同式：「词表 = 镜子」的诚实形态）。
    if man.ty != AppType::App {
        eco_app(console, id, b"driver unserved"); // 15B ≤ 24B
        return (false, UiFace::Absent, None);
    }
    // 双守卫：manifest.id ↔ .bap 文件名（走查名 = 真理源）、manifest.entry
    // ↔ 包内条目名（mkapp 门禁 3 的消费侧镜像——rootfs 可被拼装）。
    if man.id.as_bytes() != id {
        eco_app(console, id, b"id mismatch");
        return (false, UiFace::Absent, None);
    }
    if man.entry.as_bytes() != &entry[..sc.entry_len] {
        eco_app(console, id, b"entry mismatch");
        return (false, UiFace::Absent, None);
    }
    // PS 账本门（见 AppSpawnTail.ledger）：只登记 console=true 的可观测
    // app；静默占位 app 不占 PS_MAX/ps 渲染窗。
    let ledger = man.console;
    // SDK-1a §5.2 步骤 1：compat min 装载期检查（ADR #55 工具链三值域的
    // 运行时执法点）。min > SDK_CURRENT = 本系统比包声称的最低线还老 →
    // loud 降级行拒拉（fsmgr diag("geom") 同式）；target/compile 是打包
    // 期面向（mkapp 已闸），装载期不查。此处 out_s 未建、dup 未造——
    // 零清理直退。
    if let Some(c) = man.compat {
        if c.min > SDK_CURRENT {
            eco_app(console, id, b"compat min"); // 10B ≤ 24B
            return (false, UiFace::Absent, None);
        }
        // VER-01 提案第 4 条（平台 API level 单调化）：min_level 轨道的
        // 装载期检查——包声明的最低平台级高于本系统级 = loud 降级行拒拉
        //（"compat min" 同形短行）。基线 BERRY_PLATFORM_LEVEL 自 berry-sys
        // abi_gen（单源 tools/abi.toml [platform]——SDK 门槛位换轨单调整数
        // level，Android API level 形而更简）；与上方三值域检查正交并行、
        // 各自比较。同样零清理直退。
        if c.min_level > BERRY_PLATFORM_LEVEL {
            eco_app(console, id, b"lvl min"); // 7B ≤ 24B
            return (false, UiFace::Absent, None);
        }
    }
    // SDK-1a §5.2 步骤 2：net 词 loud 拒（档 (b)——schema/mkapp 收、appd
    // 拒：v1 无网络服务可安全授能，词表先行 ≠ 无服务硬授，挂账 §10 O3）。
    // 同样零清理直退（console 两态尚未分岔）。
    if man.net.is_some() {
        eco_app(console, id, b"net unserved"); // 11B ≤ 24B
        return (false, UiFace::Absent, None);
    }
    // GUI-7：[ui] 验证（net 检后、console 分岔前——纯读零句柄零清理；
    // 失败隔离：Bad 只打 diag 不拦 spawn，与下方拉起链独立计谓词）。
    let uface = ui_face(
        console,
        id,
        man.ui,
        &icons[..sc.icon_n],
        &views[..sc.views_n],
        bap_off,
    );
    // 能力面：console=false 才建私有对（spawn 内 dup RIGHT_SEND 交付 spawn
    // 侧端，原值留本表作收集端）。stack_pages 按 manifest；intent v1 解析
    // 面记录、调度零消费（消费点随 v2 EEVDF，ADR #42 两段式）。
    let mut out_s: Handle = 0;
    let stdout: Handle = if man.console {
        console
    } else {
        let (s, c) = channel_pair();
        out_s = s;
        c
    };
    // SDK-1a §5.2 步骤 4：fs 四元装配（uu_run/bsh fs_gifts 同款）——manifest
    // fs 词在场 = 真授能：rootfs RO dup（env hs[4]→GIFT_H[0]→rootfs_h()）+
    // fsmgr 对 SEND|RECV dup（env hs[5]→GIFT_H[1]→fs_h()）。半途失败两枚
    // 都试关（bsh fs_gifts 同款）；失败清理含 console=false 的 out 对（此
    // 时已分岔——spawn Err 面同款显式关两值）。wants_fs=false 两枚恒 0，
    // spawn Ok/Err 面的关闭分支不触。
    let wants_fs = man.fs.is_some();
    let mut rootfs_dup: Handle = 0;
    let mut fs_dup: Handle = 0;
    if wants_fs {
        if fs_a == 0 {
            // §1.2 新序下 app 批窗口 fs_a_h 必在席；0 = 老序残路/裸拓扑
            // 兜底——loud 降级不静默（fsmgr 未预拉的 fs 词授不了能）。
            eco_app(console, id, b"fs none"); // 7B ≤ 24B
            if out_s != 0
                && (berry_handle_close(out_s) != 0 || berry_handle_close(stdout) != 0)
            {
                trap();
            }
            return (false, uface, None);
        }
        if berry_handle_duplicate(H_ROOTFS, RIGHT_READ, &mut rootfs_dup) != 0 {
            eco_app(console, id, b"fs dup"); // 6B ≤ 24B
            if out_s != 0
                && (berry_handle_close(out_s) != 0 || berry_handle_close(stdout) != 0)
            {
                trap();
            }
            return (false, uface, None);
        }
        if berry_handle_duplicate(fs_a, RIGHT_SEND | RIGHT_RECV, &mut fs_dup) != 0 {
            // 半途失败：rootfs_dup 已成，先试关再走 out 对清理。
            let _ = berry_handle_close(rootfs_dup);
            eco_app(console, id, b"fs dup");
            if out_s != 0
                && (berry_handle_close(out_s) != 0 || berry_handle_close(stdout) != 0)
            {
                trap();
            }
            return (false, uface, None);
        }
    }
    // gift 数组前置绑定（E0716：字面数组临时值撑不过 SpawnSpec 借用窗）；
    // !wants_fs 时两枚恒 0、数组不被引用。
    let fs_gifts = [rootfs_dup, fs_dup];
    match spawn_rootfs_elf(
        H_ROOTFS,
        &SpawnSpec {
            name: man.entry.as_bytes(),
            data_off: bap_off + sc.elf_off,
            data_len: sc.elf_len,
            stdout,
            stack_pages: man.stack_pages as u64,
            // stdin 无数据面（hello/vprobe 同款——appdemo/appquiet 都不读
            // fd0）。SDK-1a：wants_fs 时改 `Some(&[])`——berry-exec 契约
            // extras 非空须 stdin 槽在位（:446-449 assert），零长 feed =
            // 纯槽位占位无数据。
            stdin_feed: if wants_fs { Some(&[]) } else { None },
            stdin_handle: None,
            args: &[],
            extra_gifts: if wants_fs { &fs_gifts } else { &[] },
            mmio_vmo: None,
            mmio_va: 0,
            intent: 0,
            force_eager: false,
        },
    ) {
        Err(()) => {
            // spawn Err 全在 env send 前（berry-exec 契约）：本表零 take，
            // fresh 私有对原值都在——关掉（vblk Err-path close 同款纪律）。
            // SDK-1a §5.2 步骤 6：fs dup 未 take 也在表——先关两枚再关
            // out 对（半途失败连带关的序一致化）。
            if wants_fs {
                if berry_handle_close(rootfs_dup) != 0 || berry_handle_close(fs_dup) != 0 {
                    trap();
                }
            }
            if ledger {
                ps_reg(id, psp::STATE_SPAWNFAIL, 0);
            }
            eco_app(console, id, b"elf bad");
            if out_s != 0
                && (berry_handle_close(out_s) != 0 || berry_handle_close(stdout) != 0)
            {
                trap();
            }
        }
        Ok(child) => {
            // SDK-1a §5.2 步骤 7：fs 两 dup 已被 take 随 env 块迁移（子表
            // 所有）——本表勿关（旧值已失效，关 = -EBADF；bsh arm 同款
            // 注释义务）。
            if ledger {
                ps_reg(id, psp::STATE_ALIVE, 0);
            }
            // M-C γ-分：收口尾不在此兑现——原值打包随三元返（boot 批同步
            // 消费 / SPAWN 形入 APP_CHILDREN 异步）。id 拷贝 owned（tail
            // 存活越出本帧，&id 借用撑不过——AppSlot 同款 [u8; ID_MAX]
            // 定宽）。
            let mut tid = [0u8; ID_MAX];
            tid[..id.len()].copy_from_slice(id);
            return (
                true,
                uface,
                Some(AppSpawnTail {
                    child,
                    out_s,
                    // console=true：子表持 console 原值，本表零收口面（
                    // 勿关 console）；console=false：原值 c 留尾。
                    stdout_c: if out_s != 0 { stdout } else { 0 },
                    ledger,
                    id: tid,
                    id_len: id.len(),
                    eof_seen: false,
                }),
            );
        }
    }
    (false, uface, None)
}

/// 墓碑表记入（§4：upsert——同 id 覆写旧槽并 close 旧 svmo，二次墓碑不
/// 双槽；满 8 淘汰 epoch 最小槽〔LRU〕并 close 其 svmo 真回收）。epoch =
/// 全局自增记入序。记入条件由调用方执法（reap 真码 0 ∧ blob BTMB 验）。
/// SAFETY: tomb_selftest 单线程 boot 路独占——本 fn 唯一调用方。
fn tomb_put(id: &[u8], svmo: Handle) {
    // id 零垫 24B（帽内截断——v0 id 恒 7B）。
    let mut key = [0u8; 24];
    let n = id.len().min(24);
    key[..n].copy_from_slice(&id[..n]);
    // SAFETY: 单线程独占（同 TOMB_TBL 头注）。
    let tbl: &mut [TombSlot] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(TOMB_TBL) as *mut TombSlot,
            8,
        )
    };
    // 单遍扫：同 id 查重 / 首空槽 / epoch 最小（LRU victim）。
    let mut same: Option<usize> = None;
    let mut free: Option<usize> = None;
    let mut min_epoch = u64::MAX;
    let mut lru = 0usize;
    for i in 0..8 {
        if tbl[i].svmo == 0 {
            if free.is_none() {
                free = Some(i);
            }
            continue;
        }
        if tbl[i].id == key {
            same = Some(i);
            break; // upsert 保证同 id 必唯一——早退
        }
        if tbl[i].epoch < min_epoch {
            min_epoch = tbl[i].epoch;
            lru = i;
        }
    }
    let (slot, old) = match same {
        Some(i) => (i, tbl[i].svmo),
        None => match free {
            Some(i) => (i, 0),
            None => (lru, tbl[lru].svmo),
        },
    };
    if old != 0 && old != svmo && berry_handle_close(old) != 0 {
        trap(); // 覆写/淘汰回收面破缺 = 表账异常，loud
    }
    // SAFETY: 同上单线程。
    unsafe { TOMB_EPOCH += 1 };
    let epoch = unsafe { core::ptr::addr_of!(TOMB_EPOCH).read() };
    tbl[slot] = TombSlot { id: key, svmo, epoch };
}

/// 墓碑表取档出表（§4 钉序「取档即出表」）：清槽 + close svmo（blob 随
/// 复活者末柄回收——弃档面同 op）。返 false = 槽不在（自陷级账错——put
/// 后必在）。SAFETY: 同上单线程独占。
fn tomb_take(id: &[u8]) -> bool {
    let mut key = [0u8; 24];
    let n = id.len().min(24);
    key[..n].copy_from_slice(&id[..n]);
    // SAFETY: 单线程独占（同 TOMB_TBL 头注）。
    let tbl: &mut [TombSlot] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(TOMB_TBL) as *mut TombSlot,
            8,
        )
    };
    for i in 0..8 {
        if tbl[i].svmo != 0 && tbl[i].id == key {
            let h = tbl[i].svmo;
            tbl[i] = TOMB_SLOT_ZERO;
            return berry_handle_close(h) == 0;
        }
    }
    false
}

/// SPAWN 收口尾载荷（M-C γ-分）：`app_launch` spawn Ok 后的异步收口包——
/// boot 批同步消费、SPAWN RPC 形入 `APP_CHILDREN` 子表由稳态探活兑现。
/// app-1 段 b-1：墓碑协议 v0 自验（`docs/app-1实施设计草案-v1.2.md` §4
/// ——b-1/b-2 唯一基准；段 a freeze-first 序列续接 pause(save)→记表→
/// relaunch 读档复活）。序列（确定性状态谓词，非竞速）：cpio 直查
/// selftest/ 位（D1：APPTOMB 不入 app/ 收集 = 零漂移；miss 静默 return）
/// → 预建 state VMO（64KiB 定尺寸 = blob 帽结构性执法）→ spawn（双
/// gift：extras[4]=lifecycle 端 B、extras[5]=state VMO dup〔R|W——v1.2 §4
/// 放弃只读收紧〕、stdin_feed 直传零长——M1）→ 通道收 UP → drain 在途
/// beat → `berry_process_suspend(hp,0)`（仅 -EBUSY 退避重试——D4）→ 8s
/// 窗通道零收（冻结进程不跑 = 无 BEAT；drain 后开窗，冻结前在途一拍不
/// 误报）→ WAKE 臂（AM-1.5：冻结窗 send PROBE 探针 → `object_wait_one
/// (token, SIGNAL_WAKE_PENDING)` 收通知 → close(token) 解冻〔Q1「唤醒即
/// 解冻」默认策略演示〕）→ 发 TAG_RESUME 帧收 TAG_RESUMED ∧ TAG_PROBED
/// 双旗（D2 消息化，帽 5s liveness 口径）→ `FROZEN ok n=1` →
/// `WAKE ok n=1` → PAUSESV1
/// （thaw-then-pause 前置不变式：此刻目标 freeze_depth=0）→ 10s deadline
/// 帽收 SAVED → reap 真码 0 + blob BTMB 验 → 墓碑表记入 → relaunch（新
/// lc 对 + 槽 svmo dup(R|W) 授子，复用首轮 bap_scan/manifest 果）→ 收
/// TAG_RESTORED（counter 续接 = saved+1）→ reap 真码 0 → 出表（取档即
/// 出表）→ `TOMB ok n=1`。三标记独立判（app-0 先例互不连坐——本流程串行
/// 共用一次生命周期，§3 取前者；WAKE 判据不含 RESUMED、FROZEN 判据不含
/// PROBED，任一缺不连坐他标）。任一步降级 = eco 行 + 同款收口（含
/// state VMO/槽回收）+ 标记缺席（smoke FAIL）。
fn tomb_selftest(console: Handle, step: &mut [u8], mf: &mut [u8], icons: &mut [UiEntry], views: &mut [UiEntry]) {
    // lifecycle 消息（TAG 族编码——app-2 刀2 双源镜像消亡：svcmgr 常量块
    // 升唯一权威在册，apptomb 侧 const 块退役、经 berry-libc 消费同值）：
    // 8B 定长 tag+pad+counter LE。
    const MSG: usize = 8;
    const TAG_UP: u8 = 1;
    // TAG_BEAT 在位声明（b-1 起 wait_frame 收帧臂需按 tag 跳过心跳噪声
    // ——不再只是「抽干不区分」；常量面同 TAG 族注：唯一权威在册）。
    const TAG_BEAT: u8 = 2;
    const TAG_RESUMED: u8 = 3;
    const TAG_RESUME: u8 = 4;
    // 段 b 面（v1.2 §4）：复活上报 / pause(save) 下行 / 保存回执。
    const TAG_RESTORED: u8 = 5;
    const TAG_PAUSESV1: u8 = 6;
    const TAG_SAVED: u8 = 7;
    // AM-1.5 WAKE 臂（Q1「唤醒即解冻」消费闭环）：冻结窗探针下行 tag 8 /
    // 回环回执 tag 9（tag 族 1-7 已占；同 TAG 族注口径——svcmgr 常量块
    // 唯一权威在册，apptomb 侧镜像已消亡）。
    const TAG_PROBE: u8 = 8;
    const TAG_PROBED: u8 = 9;
    const UP_CAP_NS: u64 = 10 * 1_000_000_000;
    const WIN_NS: u64 = 8 * 1_000_000_000; // TCG 放宽（≥3 心跳周期）
    // D2：RESUME 帽 5s 对齐 liveness 口径（app 拍周期 2s + 投递余量；仅
    // 活性兜底，非测量判据）。
    const RESUME_CAP_NS: u64 = 5 * 1_000_000_000;
    // §4 pause deadline：10s、起算 = appd 发出时刻（点对点 clock，含通道
    // 投递等待；app 侧预算 poll 型 ≥1 轮询周期 = 2s 心跳自留 ≥2s 余量）。
    // 值同规范 §4.1-1；v2 真策略按 intent 分档复核。
    const PAUSE_CAP_NS: u64 = 10 * 1_000_000_000;
    // relaunch 收 restored 帽（§6 谓词链；SAVED 同款量级——复活链 worst
    // = spawn + 启动判读 + RESTORED 即发 ≪ 10s）。
    const RESTORE_CAP_NS: u64 = 10 * 1_000_000_000;
    // AM-1.5 wake-pending 通知位（内核 `kernel/src/suspend.rs` 单源镜像：
    // 内核派生低 32 位域第 4 位，与 READABLE(1<<0)/WRITABLE(1<<1)/
    // TERMINATED(1<<2) 同域不重叠。abi.toml/berry-sys 归库挂 A 头注同名
    // 义务（消费臂收口刀再生——经 abi-gen 会再生产 kernel/ 产物，非本刀
    // 域），此处镜像 const 消费：值漂移由内核 WAKEP-K ktest + 本 WAKE 臂
    // 双兜底）。
        // WAKE 通知帽：send 收敛点在 berry_channel_send 返回前同步置位 +
    // 广播（内核 syscall.rs AM-1.5 注），wait 命中即回——2s 纯活性兜底
    // 非测量判据（RESUME_CAP 同口径）。
    const WAKE_CAP_NS: u64 = 2 * 1_000_000_000;
    // state blob 头（§4/§7）：12B 定长 {magic u32 LE, ver u8=0, hlen u8=12,
    // rsv u16=0, counter u32 LE}——svcmgr 常量块唯一权威在册（app-2 刀2
    // 双源镜像消亡，apptomb 侧手排退役）；12B/ver=0/hlen=12 为 v0 档，
    // v1 hlen=32 读者按 hlen 分派——app-2 §3.2。
    const BTMB_MAGIC: u32 = u32::from_le_bytes(*b"BTMB");
    // state VMO 定尺寸（§7 帽 = blob 帽：结构性执法——越写物理不可，比
    // 「appd 执法」更强）。
    const STATE_VMO_SZ: u64 = 64 * 1024;

    // ① cpio 直查 selftest/ 专用位（D1：APPTOMB 不入 app/ 前缀 = 不进走查
    // 收集 = APP_SLOTS/APP_LIST/boot 批全零漂移；复用主走查同款流式解析
    // 重扫 H_ROOTFS——berry-exec STEP_WIN 独立于本 fn 的 step 窗）。miss =
    // 裸相/无 selftest 位拓扑：静默 return 零扰动（标记缺席面归 smoke 门）。
    let Some((off, len)) = cpio_find(H_ROOTFS, b"selftest/APPTOMB.bap") else {
        return;
    };

    // ② lifecycle 对 + bap 解析 + 双守卫（app_launch 同款）。
    let (lc_a, lc_b) = channel_pair();
    let mut entry_name = [0u8; ENTRY_MAX];
    let sc = match bap_scan(step, mf, &mut entry_name, icons, views, off, len) {
        Ok(sc) => sc,
        Err(why) => {
            eco_note(console, why);
            if berry_handle_close(lc_a) != 0 || berry_handle_close(lc_b) != 0 {
                trap();
            }
            return;
        }
    };
    let man = match berry_manifest::parse(&mf[..sc.mf_len]) {
        Ok(m) => m,
        Err(e) => {
            eco_note(console, e.reason_str().as_bytes());
            if berry_handle_close(lc_a) != 0 || berry_handle_close(lc_b) != 0 {
                trap();
            }
            return;
        }
    };
    if man.id.as_bytes() != *b"APPTOMB" || man.entry.as_bytes() != &entry_name[..sc.entry_len] {
        eco_note(console, b"tomb guard");
        if berry_handle_close(lc_a) != 0 || berry_handle_close(lc_b) != 0 {
            trap();
        }
        return;
    }

    // ②' state VMO 预建（§4：appd 建本体、spawn 迁 dup、reap 后本体入表）
    // 。options=0 立法：权利非 create 入参——options 只认 VMO_OPT_DEMAND
    // 且 RIGHT_READ(bit0) 恰撞 DEMAND 位，照抄「READ|WRITE 入参」会静默
    // 建成 demand VMO（v1.2 §4 坑注）；创建者自动持 R|W|MAP|DUP。commit
    // 全幅（M2 修：零页建账，未 commit 首写/首读返 -EBADSTATE——thr/dpg
    // 自验 create+commit 同款序）。
    let mut svmo: Handle = 0;
    if berry_vmo_create(STATE_VMO_SZ, 0, &mut svmo) != 0
        || svmo == 0
        || berry_vmo_commit(svmo, 0, STATE_VMO_SZ) != 0
    {
        eco_note(console, b"tomb svmo");
        if berry_handle_close(lc_a) != 0 || berry_handle_close(lc_b) != 0 {
            trap();
        }
        return;
    }
    // 授子 dup（最小面 R|W：启动判读要 READ、pause(save) 要 WRITE；MAP/DUP
    // 不授。relaunch 轮同面——v1.2 §4 放弃只读收紧：复活者再入 pause(save)
    // 循环需写权，循环天然闭合）。
    let mut st_dup: Handle = 0;
    if berry_handle_duplicate(svmo, RIGHT_READ | RIGHT_WRITE, &mut st_dup) != 0 || st_dup == 0 {
        eco_note(console, b"tomb svmo");
        if berry_handle_close(lc_a) != 0
            || berry_handle_close(lc_b) != 0
            || berry_handle_close(svmo) != 0
        {
            trap();
        }
        return;
    }

    // ③ spawn（双 gift——v1.2 §4 装配面立法：**仅 tomb_selftest 显式
    // spawn 通道装配**，boot 批 app_launch 与 SPAWN RPC 恒不装（fs_gifts
    // 固占槽位、带 fs 词 app 结构性冲突）；gift[4] = lifecycle 端 B fresh
    // 迁入、gift[5] = state VMO dup 迁入；stdin_feed=Some(&[]) 直传——M1：
    // extras 在位断言要求 stdin 槽；console 诊断 = spawn stdout 面，不进
    // 判据〔H1〕）。
    let gifts = [lc_b, st_dup];
    let child = match spawn_rootfs_elf(
        H_ROOTFS,
        &SpawnSpec {
            name: man.entry.as_bytes(),
            data_off: off + sc.elf_off,
            data_len: sc.elf_len,
            stdout: console,
            stack_pages: man.stack_pages as u64,
            stdin_feed: Some(&[]),
            stdin_handle: None,
            args: &[],
            extra_gifts: &gifts,
            mmio_vmo: None,
            mmio_va: 0,
            intent: 0,
            force_eager: false,
        },
    ) {
        Ok(c) => c,
        Err(()) => {
            eco_note(console, b"tomb spawn");
            // Err 早退面 extra_gifts 未迁移——两端 + state dup 都关（svmo
            // 本体同收——尚未入表，无槽账联动）。
            if berry_handle_close(lc_a) != 0
                || berry_handle_close(lc_b) != 0
                || berry_handle_close(st_dup) != 0
                || berry_handle_close(svmo) != 0
            {
                trap();
            }
            return;
        }
    };
    // spawn Ok：lc_b + st_dup 已随 env 块迁入子进程——本侧持 lc_a + svmo
    // 本体（dup 与本体同 VMO 两柄：子写 = 本体可见，R2 首验点——vblk
    // 数据面同构先例）。

    // ④ 收 UP（通道判据起点；超时 = app 未活或通道面破）。嵌套 fn（非闭包
    // ——闭包捕 msg 借用会与 ⑤ drain 循环冲突）。
    fn recv_tag(lc: Handle, buf: &mut [u8; MSG], cap_ns: u64) -> Option<u8> {
        let mut obs = 0u64;
        if berry_object_wait_one(lc, SIGNAL_READABLE, now() + cap_ns, &mut obs) != 0 {
            return None;
        }
        let mut nb = buf.len();
        let mut nh = 0usize;
        if berry_channel_recv(lc, buf.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh) != 0
            || nb < 1
        {
            return None;
        }
        Some(buf[0])
    }
    // 段 b 收帧（带 counter 对账）：等 want tag（deadline 连续不重臂），在途
    // BEAT 心跳噪声跳过续等——D2 两拍循环后 app 醒拍先 BEAT 后处理下行帧，
    // SAVED/RESTORED 前必有残余 BEAT 在队，等首帧会假红；他 tag（协议破缺/
    // 降级面，如坏档弃档后全新启动的 UP）与超时返 None。
    fn wait_frame(lc: Handle, buf: &mut [u8; MSG], cap_ns: u64, want: u8) -> Option<u32> {
        let deadline = now() + cap_ns;
        loop {
            let now_ns = now();
            if now_ns >= deadline {
                return None;
            }
            match recv_tag(lc, buf, deadline - now_ns) {
                Some(t) if t == want => {
                    return Some(u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]));
                }
                Some(TAG_BEAT) => continue,
                Some(_) => return None,
                None => return None,
            }
        }
    }
    let mut msg = [0u8; MSG];
    match recv_tag(lc_a, &mut msg, UP_CAP_NS) {
        Some(TAG_UP) => {}
        _ => {
            eco_note(console, b"tomb up");
            let _ = berry_process_terminate(child.hp, 9);
            let _ = child.reap();
            if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
                trap();
            }
            return;
        }
    }

    // ⑤ 冻结（options=0 进程全冻；hp 全权——对抗审查 L2 预证 R1）。退避
    // （D4 钉死）：仅 -EBUSY 可重试——瞬态源 = console 背压/副核 Running
    // 窗/醒窗在飞（2s 睡窗占比 >99.9%，100ms 退避 40 试帽 4s 必落窗）；
    // 其余 errno（-EACCES/-EINVAL/-EBADSTATE）各对应一个明确拒绝点 = 非
    // 瞬态，立即出失败行 loud（致命错不被重试延迟）。
    let mut token: Handle = 0;
    let mut susp_fail: berry_sys::Status;
    let mut tries = 0u32;
    loop {
        susp_fail = berry_process_suspend(child.hp, 0, &mut token);
        if susp_fail == 0 && token != 0 {
            break;
        }
        if susp_fail != BERRY_ERR_BUSY || tries >= 40 {
            break;
        }
        tries += 1;
        berry_thread_sleep(now() + 100_000_000);
    }
    if susp_fail != 0 || token == 0 {
        // 取证行带 errno（thr exit 十进制同款）：-EBUSY/-EACCES/-EINVAL/
        // -EBADSTATE 各对应内核侧一个明确拒绝点。
        // 「eco: tomb susp 」前缀 = 15B（thr 先例 14B 是因 'thr'3B vs 'tomb'4B
        // ——首版 15 进 14 槽 panic 连灭四跑的根因，雷册候选）。
        let mut line = [0u8; 24];
        let pre = b"eco: tomb susp ";
        let mut i = pre.len();
        line[..i].copy_from_slice(pre);
        if susp_fail < 0 {
            line[i] = b'-';
            i += 1;
        }
        let mut v = (susp_fail as i64).unsigned_abs();
        let mut digits = [0u8; 10];
        let mut dn = 0;
        loop {
            digits[dn] = b'0' + (v % 10) as u8;
            dn += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        while dn > 0 {
            dn -= 1;
            line[i] = digits[dn];
            i += 1;
        }
        line[i] = b'\n';
        send_retry(console, line.as_ptr(), i + 1);
        let _ = berry_process_terminate(child.hp, 9);
        let _ = child.reap();
        if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
            trap();
        }
        return;
    }
    // drain：冻结生效后冻结前在途 beat 已全入队——抽干再开窗（竞态封口）。
    loop {
        let mut nb = msg.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(lc_a, msg.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == BERRY_ERR_AGAIN {
            break;
        }
        if s != 0 {
            trap();
        }
    }

    // ⑥ 静默窗：8s 内通道零收（可读 = 冻结失效/迟到 beat——K1 降级面）。
    let mut obs = 0u64;
    let w = berry_object_wait_one(lc_a, SIGNAL_READABLE, now() + WIN_NS, &mut obs);
    if w != BERRY_ERR_TIMEDOUT {
        eco_note(console, b"tomb win");
        let _ = berry_handle_close(token); // 解冻收口（无论窗判成败）
        let _ = berry_process_terminate(child.hp, 9);
        let _ = child.reap();
        if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
            trap();
        }
        return;
    }

    // ⑥' AM-1.5 WAKE 臂上半（冻结窗净判过、解冻前的最小插入——Q1「唤醒
    // 即解冻」消费闭环）：冻结窗内 send PROBE 探针（非阻塞有界队列天然
    // 排队——消息不丢弃，recv 解冻后照收）→ 内核 send 收敛点置
    // wake_pending + 以进程钉键广播 token 等待者 → wait_one(token,
    // WAKE_PENDING) 收通知（observed 须恰等位值——SuspendToken 信号面
    // 唯一可命中位，阻塞形即 am 推荐消费形）。任一败 = 通知面破缺：eco
    // 行 + 同款收口（token 解冻护送）+ WAKE/FROZEN/TOMB 全缺（smoke
    // FAIL 兜底）。
    let mut wm = [0u8; MSG];
    wm[0] = TAG_PROBE;
    if berry_channel_send(lc_a, wm.as_ptr(), MSG, core::ptr::null(), 0) != 0 {
        eco_note(console, b"tomb probe");
        let _ = berry_handle_close(token); // 解冻收口（无论本臂成败）
        let _ = berry_process_terminate(child.hp, 9);
        let _ = child.reap();
        if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
            trap();
        }
        return;
    }
    let mut wobs = 0u64;
    if berry_object_wait_one(token, SIGNAL_WAKE_PENDING, now() + WAKE_CAP_NS, &mut wobs) != 0
        || wobs != SIGNAL_WAKE_PENDING
    {
        eco_note(console, b"tomb wake");
        let _ = berry_handle_close(token); // 解冻收口（无论本臂成败）
        let _ = berry_process_terminate(child.hp, 9);
        let _ = child.reap();
        if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
            trap();
        }
        return;
    }

    // ⑦ 解冻（thaw = close(token)，1.1-S3 语义；此刻兼 Q1「唤醒即解冻」
    // 默认策略演示——am 形：token 持有者收 WAKE 通知即 close 解冻，裁决
    // 权留持有者）→ 发 TAG_RESUME 帧 → 收 TAG_RESUMED（D2 消息化：解冻由
    // 显式消息告知，app 侧差值启发式已废）。醒窗竞态封口：app 若在本侧
    // send 落队前已被调度，其拍一 recv 得 -EAGAIN 后先走真实 sleep，不先
    // 发帧——但 2s 拍界前后仍有在途 BEAT 可能，故收帧循环抽干至 RESUMED
    // ∧ PROBED 双旗（等的是两帧、不赌首帧与到达序；PROBE 先 RESUME 入队
    // ⇒ app 拍序抽干回帧 PROBED 恒先 RESUMED——双旗收集对此零依赖；帽 =
    // 总口径 5s）。
    if berry_handle_close(token) != 0 {
        trap();
    }
    let mut rm = [0u8; MSG];
    rm[0] = TAG_RESUME;
    if berry_channel_send(lc_a, rm.as_ptr(), MSG, core::ptr::null(), 0) != 0 {
        eco_note(console, b"tomb resume");
        let _ = berry_process_terminate(child.hp, 9);
        let _ = child.reap();
        if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
            trap();
        }
        return;
    }
    let deadline = now() + RESUME_CAP_NS;
    let mut resumed_ok = false;
    let mut probed_ok = false;
    loop {
        let now_ns = now();
        if now_ns >= deadline {
            break;
        }
        match recv_tag(lc_a, &mut msg, deadline - now_ns) {
            Some(TAG_RESUMED) => {
                resumed_ok = true;
                if probed_ok {
                    break;
                }
            }
            Some(TAG_PROBED) => {
                // WAKE 臂下半：PROBED 回环帧到（判据下半——见 ⑧'）。
                probed_ok = true;
                if resumed_ok {
                    break;
                }
            }
            Some(_) => {} // 在途 BEAT/迟帧抽干续等
            None => break, // 超时/收帧败 → 降级
        }
    }
    if !resumed_ok {
        eco_note(console, b"tomb res");
        let _ = berry_process_terminate(child.hp, 9);
        let _ = child.reap();
        if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
            trap();
        }
        return;
    }

    // ⑧ 段 a 标记（独立判——app-0 先例互不连坐，两行各自谓词；段 b-1 起
    // 不 terminate：§2「共用一次生命周期」，pause 收束续走）。
    send_retry(console, b"FROZEN ok n=1\n".as_ptr(), 14);

    // ⑧' AM-1.5 标记（独立判——与 FROZEN/TOMB 互不连坐）：WAKE 判据 =
    // ①冻结窗 send 后 token 通知到位（⑥' wait 命中且 observed 恰等）
    // ∧ ②解冻后 PROBED 回环（⑦ 窗内收）——「冻结进程收消息 → 令牌持有
    // 者收通知 → 解冻 → 进程收到该消息」全闭环实证（Q1 裁决消费面）。
    // PROBED 缺 = 通知面破缺：eco 行 + 收口 + WAKE/TOMB 缺（FROZEN 判据
    // 链不含 PROBED，不连坐——本行在 FROZEN 之后即此语义）。
    if !probed_ok {
        eco_note(console, b"tomb wake");
        let _ = berry_process_terminate(child.hp, 9);
        let _ = child.reap();
        if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
            trap();
        }
        return;
    }
    // `WAKE ok n=1\n` = 11 字面 + LF = 12B（行宽先数后写——雷册 §A145）。
    send_retry(console, b"WAKE ok n=1\n".as_ptr(), 12);

    // ⑨ pause(save)（§4）。thaw-then-pause 前置不变式已在 ⑦ 兑现（close
    // (token) 解冻 + RESUMED 回帧 = 目标 freeze_depth=0 且活转）——PAUSESV1
    // 仅可发往未冻结进程；跳过解冻直发 = 消息入队无人消费 → deadline 必超
    // → 监督杀 = 状态静默丢失。deadline 起算 = 本侧发出时刻（§4 立法；
    // wait_frame 帽紧随 send 返回，亚 ms 偏差）。
    let mut pm = [0u8; MSG];
    pm[0] = TAG_PAUSESV1;
    // counter 位恒 0：暂停令无参（app 回 SAVED 带己方 counter）。
    if berry_channel_send(lc_a, pm.as_ptr(), MSG, core::ptr::null(), 0) != 0 {
        // 发送面破缺（满队不可达——队深 8、此刻至多 2 帧在途）= pause 失败
        // 同 face：监督杀 + 不入表 + VMO 回收。
        eco_note(console, b"tomb ptmo");
        let _ = berry_process_terminate(child.hp, 9);
        let _ = child.reap();
        if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
            trap();
        }
        return;
    }
    let saved_c = match wait_frame(lc_a, &mut msg, PAUSE_CAP_NS, TAG_SAVED) {
        Some(c) => c,
        None => {
            // 超时未退 = 监督杀（§4：策略性墓碑超时 ≠ 崩溃监督——**不重启**
            // ，代号 +1 重启仅适用崩溃语义，v0 对规范 §4.1-1 显式偏差随 b-2
            // 回写）+ 不入墓碑表 + state VMO 直接 close 回收 64KiB。
            eco_note(console, b"tomb ptmo");
            let _ = berry_process_terminate(child.hp, 9);
            let _ = child.reap();
            if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
                trap();
            }
            return;
        }
    };
    // reap0：SAVED → app exit(0)（EOF 先行）→ wait 瞬回（30s 绝对帽兜底）。
    // reap Err/非零 = 句柄留存有界泄漏面（berry-exec 契约），标记缺席执法。
    match child.reap() {
        Ok(0) => {}
        _ => {
            eco_note(console, b"tomb rcode");
            if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
                trap();
            }
            return;
        }
    }
    // 记入条件（§4）：reap 真码 0 ∧ blob 首 4B = BTMB（仍零态 = 拒绝保存
    // 语义 → 不入表直接回收 64KiB；v0 自验件恒有态，此臂 = 守卫面）。
    let mut head = [0u8; 4];
    if berry_vmo_read(svmo, head.as_mut_ptr(), 0, 4) != 0
        || u32::from_le_bytes(head) != BTMB_MAGIC
    {
        eco_note(console, b"tomb nosave");
        if berry_handle_close(lc_a) != 0 || berry_handle_close(svmo) != 0 {
            trap();
        }
        return;
    }
    // 首轮 lifecycle 面毕：close lc_a（残余帧随端点消亡）→ 记表占槽（§4
    // 钉序：reap0 → 记表 → relaunch——槽自此接管 svmo 所有权）。
    if berry_handle_close(lc_a) != 0 {
        trap();
    }
    tomb_put(b"APPTOMB", svmo);

    // ⑩ relaunch（§4）：同 .bap 二次 spawn——复用首轮 bap_scan/manifest 解
    // 析果（走查表只供几何、不含 elf_off/stack_pages，表查单独不可成
    // spawn）；新 lifecycle 对作 gift[4]，gift[5] = 槽内 svmo 的 dup(R|W)
    // （v1.2「放弃只读收紧」——复活者再入 pause(save) 循环需写权）。
    let mut st_dup2: Handle = 0;
    if berry_handle_duplicate(svmo, RIGHT_READ | RIGHT_WRITE, &mut st_dup2) != 0 || st_dup2 == 0 {
        eco_note(console, b"tomb respawn");
        return; // §4 负⑤族：条目保留待重试——svmo 留槽
    }
    let (lc_a2, lc_b2) = channel_pair();
    let gifts2 = [lc_b2, st_dup2];
    let child2 = match spawn_rootfs_elf(
        H_ROOTFS,
        &SpawnSpec {
            name: man.entry.as_bytes(),
            data_off: off + sc.elf_off,
            data_len: sc.elf_len,
            stdout: console,
            stack_pages: man.stack_pages as u64,
            stdin_feed: Some(&[]),
            stdin_handle: None,
            args: &[],
            extra_gifts: &gifts2,
            mmio_vmo: None,
            mmio_va: 0,
            intent: 0,
            force_eager: false,
        },
    ) {
        Ok(c) => c,
        Err(()) => {
            // 二次 spawn 失败 = 条目保留待重试（§4 负⑤）——槽不动；未迁移
            // 双端 + dup 显式回收。
            eco_note(console, b"tomb respawn");
            if berry_handle_close(lc_a2) != 0
                || berry_handle_close(lc_b2) != 0
                || berry_handle_close(st_dup2) != 0
            {
                trap();
            }
            return;
        }
    };
    // spawn Ok：lc_b2 + st_dup2 已迁入复活者——本侧持 lc_a2（svmo 所有权
    // 在槽，本地值只作 dup 源已毕）。

    // ⑪ 收 RESTORED（§6 谓词链「restored c=N+1」——counter 续接 = 跨生死
    // 读档的确定性对账，非仅 tag 命中）→ reap1（restored → ≥1 心跳 → 自退
    // 0）→ 出表 → TOMB ok。
    match wait_frame(lc_a2, &mut msg, RESTORE_CAP_NS, TAG_RESTORED) {
        Some(c) if c == saved_c.wrapping_add(1) => {}
        Some(_) => {
            // counter 断链：读档未续接（blob 陈旧/写半档）——restore-fail 面。
            eco_note(console, b"tomb rctr");
            let _ = berry_process_terminate(child2.hp, 9);
            let _ = child2.reap();
            if berry_handle_close(lc_a2) != 0 || !tomb_take(b"APPTOMB") {
                trap();
            }
            return;
        }
        None => {
            // 超时/破缺（含坏档弃档按零态启动 = 收 UP 而非 RESTORED 的
            // badfmt 面）——§4 负④：close 槽 svmo 弃档 + 杀 + 不打标。
            eco_note(console, b"tomb rfail");
            let _ = berry_process_terminate(child2.hp, 9);
            let _ = child2.reap();
            if berry_handle_close(lc_a2) != 0 || !tomb_take(b"APPTOMB") {
                trap();
            }
            return;
        }
    }
    match child2.reap() {
        Ok(0) => {}
        _ => {
            // 复活即崩（二轮 reap 非零）——§4 负④同 face；reap Err 面 =
            // 句柄留存有界泄漏（reap 后不再 terminate，防半收链句柄复用）。
            eco_note(console, b"tomb rfail");
            if berry_handle_close(lc_a2) != 0 || !tomb_take(b"APPTOMB") {
                trap();
            }
            return;
        }
    }
    // 出表（取档即出表——清槽 + close svmo，blob 随复活者末柄回收；put
    // 后必在，take 失败 = 表账自陷面）+ 终关 lc_a2。
    if !tomb_take(b"APPTOMB") || berry_handle_close(lc_a2) != 0 {
        trap();
    }
    // 段 b 标记（行宽先数后写——雷册 15/14 off-by-one 教训）：
    // `TOMB ok n=1\n` = 11 字面 + LF = 12B。
    send_retry(console, b"TOMB ok n=1\n".as_ptr(), 12);
}

struct AppSpawnTail {
    child: ChildHandles,
    out_s: Handle,
    stdout_c: Handle, // 0 = console=true（子表即 console，本表零收口面）
    id: [u8; ID_MAX],
    /// PS 账本登记门：console=false 的静默 app（APPQUIET/appstub 视觉占位）
    /// 是瞬态装载验证，不入常驻/启动批账本——PS_MAX=18/REPLY_MAX=256 双帽
    /// 下 9 app 桌面不挤掉 ps/berryctl 自验行。
    ledger: bool,
    id_len: usize,
    /// 稳态 drain 已消费 EOF 帧（`app_supervise` 管堵防线吞掉的 0 长帧
    /// 记账——reap 侧 `collect_eof` 免死等已被消费的 EOF）。boot 批恒
    /// false（同步路径自收自证）。
    eof_seen: bool,
}

/// `AppSpawnTail` 兑现面（M-C γ-分下半）：collect_eof → 关对 → reap →
/// ps_exit——与段② 前 inline 形逐句同源（boot 批行为零漂义务）。返回 =
/// 「退出码 0」谓词（原 `(true, uface)` 首值语义：true 仅当 reap Ok(0)）。
fn app_reap_tail(console: Handle, t: AppSpawnTail) -> bool {
    let AppSpawnTail { child, out_s, stdout_c, id, id_len, ledger, eof_seen } = t;
    let id = &id[..id_len];
    if out_s != 0 && !eof_seen {
        // console=false：子表持的是 stdout 的 dup 别名（spawn 内 dup
        // RIGHT_SEND 随 env 块迁移），原值 c 留本表——先收 EOF（d-3b
        // 写侧对称面——libc exit 族 EOF 先行，Plan B 起 abort 亦然）
        // 再关两端（hexyl 臂同款：收集端 + 本表原值副本）。帽内未到
        // = 断链/卡死。
        if !collect_eof(out_s) {
            eco_app(console, id, b"no EOF");
        }
        if berry_handle_close(out_s) != 0 || berry_handle_close(stdout_c) != 0 {
            trap();
        }
    }
    match child.reap() {
        Ok(0) => {
            if ledger {
                ps_exit(id, 0);
            }
            true
        }
        Ok(c) => {
            if ledger {
                ps_exit(id, c);
            }
            eco_app(console, id, b"exit code");
            false
        }
        Err(()) => {
            eco_app(console, id, b"app reap");
            false
        }
    }
}

// ---- GUI-8 段②：app 域常驻 serve + launcher 装配/监督（裁决二/四-d）----

/// launcher 装配体（boot arm 与监督一次重启共用形）。**契约**（对抗审查
/// M0 裁定——让位 = 真实迁移点制）：`fs_gift` 由调用方持有；Ok 且
/// fs_gift≠0 = extras[1] take 迁入（不可 close）；**余路（门闭/cpio 缺/
/// Err）原样归还不动**（本函数只 close 自建端：wm fresh/app 对两端/input
/// dup——调用方局部零收尾义务两形同口不变）；返回 true = 装配成立
/// （LAUNCHER_CHILD/APP_SVC/LAUNCHER_LIVE 已落值）。三源：wm =
/// WMD_CLI_DUP 常驻源**现 dup**（NET_A 形——重启可行性的结构前提）；
/// app 对 = `channel_pair()` options=0（intent 借入链激活——input 域台账
/// 6 同款，GUI INTERACTIVE 面 v1 唯一可达形态，ADR #42）+ input =
/// INPUT_A 现 dup（SEND|RECV——bsh netd_dup 同口径）。前缀截断（bsh
/// netd 条款镜像）：fs 缺 → 恰 [wm]（**冻结十段相实形**——R1 判别器在
/// arm 门外定局）；input 缺 → [wm, fs, app]（交互相 tablet 缺席角）。
fn launcher_assemble(console: Handle, fs_gift: Handle) -> bool {
    let wmd_dup = WMD_CLI_DUP.load(Ordering::Relaxed);
    if wmd_dup == 0 {
        // 门闭（rootfs 形；boot/restart 调用点均已在门外——本支防御面，
        // fs_gift 归还不动）。
        return false;
    }
    let (data_off, data_len) = match cpio_find(H_ROOTFS, b"bin/berry-launcher") {
        Some(g) => g,
        None => {
            // cpio 缺 berry-launcher（手工裁剪镜像面）：零新行（gfxdemo
            // cpio 缺分支同款）——fs_gift 归还不动（bsh 照得 fs，M0 角）。
            return false;
        }
    };
    // wm fresh 别名（SEND|RECV——wmd arm w_dup 同口径）。失败 trap：dup 源
    // 在表却失败 = 内核资源面异常，此刻零清理面（fsmgr fs_dup 先例）。
    let mut wm_fresh: Handle = 0;
    if berry_handle_duplicate(wmd_dup, RIGHT_SEND | RIGHT_RECV, &mut wm_fresh) != 0 {
        trap();
    }
    // app 域通道对（fs 在位才建——前缀 [wm] ⊂ [wm,fs,app] 截断纪律；对端
    // extras[2] take 迁入，本端 spawn Ok 才定局落 APP_SVC）。
    let mut app_svc: Handle = 0;
    let mut app_cli: Handle = 0;
    if fs_gift != 0 {
        let (a, b) = channel_pair();
        app_svc = a;
        app_cli = b;
    }
    // input dup（[wm,fs,app] 全在位才装——同前缀纪律；INPUT_A==0 = inputd
    // 未起静默截断。冻结相 inputd 起而 tablet 缺席（INPUT_A≠0）也到不了
    // 此——fs_gift 已被 R1 v2 门截为 0，前缀纪律先拦）。
    let mut in_dup: Handle = 0;
    let inp = INPUT_A.load(Ordering::Relaxed);
    if fs_gift != 0 && inp != 0 {
        if berry_handle_duplicate(inp, RIGHT_SEND | RIGHT_RECV, &mut in_dup) != 0 {
            trap();
        }
    }
    let mut lgifts = [0 as Handle; 4];
    lgifts[0] = wm_fresh;
    let lgifts: &[Handle] = if fs_gift == 0 {
        &lgifts[..1]
    } else {
        lgifts[1] = fs_gift;
        lgifts[2] = app_cli;
        if in_dup != 0 {
            lgifts[3] = in_dup;
            &lgifts[..4]
        } else {
            &lgifts[..3]
        }
    };
    match spawn_rootfs_elf(
        H_ROOTFS,
        &SpawnSpec {
            name: b"bin/berry-launcher",
            data_off,
            data_len,
            stdout: console,
            // 64 页 = 256KB（R5 实测定值——段③ 首验仪器定谳：8 页 32KB 下
            // main+0x1c 序言即穿保护页 D-abort——`UiMain::new` 构造中间体
            // LTO 后仍部分落栈 ~29KB + pre-main std 运行时；帧循环另有
            // CommandEncoder 18KB/帧（gfxdemo 8 页先例无 berry-ui 链）。
            // bsh 32 页先例之上；manifest 域检 1..=64 顶格）。
            stack_pages: 64,
            // 占位 dummy（空 feeder 立即 EOF；berry-exec assert：extras 非
            // 空须 stdin 槽在位——fsmgr/vnet 同款）。
            stdin_feed: Some(&[]),
            stdin_handle: None,
            args: &[],
            extra_gifts: lgifts,
            mmio_vmo: None,
            mmio_va: 0,
            intent: 0,
            force_eager: false,
        },
    ) {
        Ok(child) => {
            ps_reg(b"launcher", psp::STATE_ALIVE, 0); // 常驻入账（帽 18 同刀，见 arm 注）
            // app 域服务端端定局：换新收旧（重启路旧值恒已被 supervise
            // 清零——swap-close 防御面，可达路径零触发）。
            let old = APP_SVC.swap(app_svc, Ordering::Relaxed);
            if old != 0 && berry_handle_close(old) != 0 {
                trap();
            }
            // SAFETY: svcmgr 单线程独占（boot arm / 稳态 loop 不交叠——
            // PS_TAB 同纪律）。
            unsafe { LAUNCHER_CHILD = Some(child); }
            LAUNCHER_LIVE.store(1, Ordering::Relaxed);
            true
        }
        Err(()) => {
            // Err 全在 env send 前——四类 fresh/交接端无一被 take，自建端
            // 全量回收防表槽泄漏（wmd arm Err 路同款枚举）；**fs_gift 归还
            // 不动**（M0 契约：调用方持有，只 close 本函数自建端）。
            if berry_handle_close(wm_fresh) != 0 {
                trap();
            }
            if app_svc != 0
                && (berry_handle_close(app_svc) != 0 || berry_handle_close(app_cli) != 0)
            {
                trap();
            }
            if in_dup != 0 && berry_handle_close(in_dup) != 0 {
                trap();
            }
            eco_line(console, b"launcher spawn");
            false
        }
    }
}

/// app 域 serve 单帧泵（稳态 loop 每轮先收干）：APP_SVC==0 = no-op 零开销
/// （rootfs/裸跑/收口形——rootfs 相零漂的结构保证）；空队 -EAGAIN →
/// false（poll-first 的 poll 半）；命中帧两段分派——LIST 走 1B 门
/// （`parse_request` 恰长执法只面 LIST 空），SPAWN 全帧走
/// `parse_spawn_request`（25B + NUL 补齐纪律），余者 1B `[ST_BAD]`
/// （appproto 烘焙线不变量：非 OK 应答恒 1B）。非 -EAGAIN 错码 trap
/// （断链/野柄 = 死亡面——guifbd serve 同口径，v1 无对端关闭检测）。
fn app_serve_one(console: Handle) -> bool {
    let svc = APP_SVC.load(Ordering::Relaxed);
    if svc == 0 {
        return false;
    }
    let mut buf = [0u8; appp::FRAME_MAX];
    let mut nbytes = buf.len();
    let mut hs = [0 as Handle; 8]; // 随行收口容量 = channel 上限 8（A2-3）
    let mut nh = hs.len();
    let s = berry_channel_recv(svc, buf.as_mut_ptr(), &mut nbytes, hs.as_mut_ptr(), &mut nh);
    if s == BERRY_ERR_AGAIN {
        return false;
    }
    if s != 0 {
        trap();
    }
    // 随行柄面零实施（v1 wire 无此面）：协议外令牌已入本表——全量丢弃
    // 防表槽泄漏（collect_eof 同款纪律），帧本体照常按字节分派。
    for i in 0..nh {
        let _ = berry_handle_close(hs[i]);
    }
    if nbytes == appp::REQUEST_LEN
        && appp::parse_request(&buf, nbytes) == Some(appp::OP_APP_LIST_REQ)
    {
        app_serve_list(console, svc);
    } else if buf[0] == appp::OP_APP_SPAWN_REQ {
        // **op 门先行**（对抗审查 M-B 裁定）：parse_spawn_request 只验帧长
        // + NUL 补齐、op 字节不参与（段① 冻结面）——无此门则任意 op 的
        // 25B 合法补齐帧（含 LIST 尾随形/APP_EVENT 留位/野 op）都被收作
        // SPAWN = 冻结拒面 [ST_BAD] 被反转成执行动作。
        match appp::parse_spawn_request(&buf, nbytes) {
            Some(id) => app_serve_spawn(console, svc, id),
            None => {
                let n = appp::encode_err_reply(&mut buf);
                send_retry(svc, buf.as_ptr(), n);
            }
        }
    } else {
        // 畸形帧（含 1B 裸 SPAWN 头——无 id 段 = 编不出合法请求）：拒面。
        let n = appp::encode_err_reply(&mut buf);
        send_retry(svc, buf.as_ptr(), n);
    }
    true
}

/// APP_LIST 应答流（裁决二 + §5）：N × ENTRY{id,label,w,h,blob_len} + 每
/// 条目 blob 192B 分片（`ChunkStream` 流式——与 `ChunkEncoder` 帧字节
/// 等价（host 测钉），idx 单源纪律同源；零整读窗 = 单页墙修复形）+ 终帧
/// END{n}。数据源 = boot 走查记账表（APP_SLOTS_TBL[..APP_ACCOUNT_N]——
/// 单一事实源；cpio_find 直查会是第二走查面）+ serve 期现查 bap_scan/
/// manifest（四静态窗复用——walk 期借用已随 run_rootfs_child 返回消亡，
/// 单线程零交叠）。**失败隔离逐条目**：scan/parse 失败 = eco 降级行 +
/// 跳过（END n 只计已发——「半张清单不进渲染面」由解码侧 idx/对账执法，
/// 服务侧诚实计数）；icon 面降级合法：头坏/越 64KiB 帽 → w/h=blob_len=0
///（launcher 回默认图标，id/label 面仍完整）。label 缺席回填 id 本串
///（appproto 头注哲学：缺席语义归数据源不进 wire）。
fn app_serve_list(console: Handle, svc: Handle) {
    // SAFETY: 稳态 serve 期单线程独占四窗 + 槽表（walk 已毕；run_rootfs_
    // child 的借用已消亡）——PS_WIN/HEX_WIN 同纪律。
    let step: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(STEP_WIN) as *mut u8, 512)
    };
    let mf: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(APP_MF_WIN) as *mut u8,
            MANIFEST_MAX,
        )
    };
    let icons: &mut [UiEntry] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(APP_ICON_WIN) as *mut UiEntry,
            UI_ENTRIES,
        )
    };
    let views: &mut [UiEntry] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(APP_VIEWS_WIN) as *mut UiEntry,
            UI_ENTRIES,
        )
    };
    let account = unsafe { core::ptr::addr_of!(APP_ACCOUNT_N).read() };
    let tbl: &[AppSlot] = unsafe {
        core::slice::from_raw_parts(core::ptr::addr_of!(APP_SLOTS_TBL) as *const AppSlot, APP_SLOTS)
    };
    let mut frame = [0u8; appp::FRAME_MAX];
    let mut sent = 0u32;
    for i in 0..account {
        let id = &tbl[i].id[..tbl[i].id_len];
        // app-1 D1：APPTOMB 不入走查表（selftest/ 前缀）——列表面零过滤。
        let mut entry_name = [0u8; ENTRY_MAX];
        let sc = match bap_scan(
            step,
            mf,
            &mut entry_name,
            icons,
            views,
            tbl[i].bap_off,
            tbl[i].bap_len,
        ) {
            Ok(sc) => sc,
            Err(_) => {
                eco_app(console, id, b"list skip"); // 服务期扫描败 = 走查后漂移面
                continue;
            }
        };
        let man = match berry_manifest::parse(&mf[..sc.mf_len]) {
            Ok(m) => m,
            Err(_) => {
                eco_app(console, id, b"list skip");
                continue;
            }
        };
        // label：manifest [ui].label（parse 执法非空 ≤32 ≤ wire 64）缺席回填
        // id 本串——桌显示名恒在。
        let label: &[u8] = match man.ui.and_then(|u| u.label) {
            Some(l) => l.as_bytes(),
            None => id,
        };
        // icon：声明 → 包内条目 + BYI1 头镜像 ui_face 执法——**零整读窗**
        //（单页墙修复：64KiB 静态整读窗会被 elf2flat 补零物化进平像（.bss
        // 文件侧兑现），143360B 顶破 131072 帽——改只验头 10B + `ChunkStream`
        // VMO 逐 ≤192B 直读进帧载荷域，blob 体零驻内存；帧字节与
        // `ChunkEncoder` 等价由 host 测钉）。任何不符 = 合法降级面（
        // 0/0/0——boot 期 APPUI 已验过健康路，此处降级只覆盖 serve 期漂移/
        // 越帽形）；流中途读败 = 该帧零填充 + eco 降级行（流长恒 = 声明
        // ——解码侧 idx/对账执法不破；中途败实际仅 H_ROOTFS 亡一途，
        // send_retry 同死 = 诊断行多数走不到，防御面在）。
        let mut icon_w = 0u32;
        let mut icon_h = 0u32;
        let mut icon_src: Option<(u64, u64)> = None; // (VMO 基址, 声明长)
        if let Some(want) = man.ui.and_then(|u| u.icon) {
            if let Some(e) = ui_find(&icons[..sc.icon_n], want.as_bytes()) {
                let mut hdr = [0u8; 10];
                let len_ok = e.len >= 10 && (e.len as usize) <= APP_ICON_BLOB_MAX;
                if len_ok
                    && berry_vmo_read(H_ROOTFS, hdr.as_mut_ptr(), tbl[i].bap_off + e.off, 10) == 0
                {
                    let w = u16::from_le_bytes([hdr[6], hdr[7]]);
                    let h = u16::from_le_bytes([hdr[8], hdr[9]]);
                    if &hdr[..4] == b"BYI1"
                        && hdr[4] == 1
                        && hdr[5] == 0
                        && w >= 1
                        && h >= 1
                        && w <= APP_ICON_DIM_MAX
                        && h <= APP_ICON_DIM_MAX
                        && u32::from(w) * u32::from(h) <= APP_ICON_PIXELS_MAX
                    {
                        icon_w = u32::from(w);
                        icon_h = u32::from(h);
                        icon_src = Some((tbl[i].bap_off + e.off, e.len));
                    }
                }
            }
        }
        let blob_len = match icon_src {
            Some((_, l)) => l as u32,
            None => 0,
        };
        // 条目头 + blob 分片 + 计数（encode None = id/label 野值——walk/parse
        // 双侧执法下理论不可达，防御跳过不炸 serve：单条目坏不连坐）。
        match appp::encode_entry(&mut frame, id, label, icon_w, icon_h, blob_len) {
            Some(n) => send_retry(svc, frame.as_ptr(), n),
            None => {
                eco_app(console, id, b"list skip");
                continue;
            }
        }
        if let Some((base, len)) = icon_src {
            let mut stream = appp::ChunkStream::new(len as usize);
            let mut read_ok = true;
            let mut fill = |dst: &mut [u8], off: usize| -> bool {
                if berry_vmo_read(
                    H_ROOTFS,
                    dst.as_mut_ptr(),
                    base + off as u64,
                    dst.len(),
                ) == 0
                {
                    true
                } else {
                    read_ok = false;
                    false
                }
            };
            while let Some(n) = stream.next(&mut frame, &mut fill) {
                send_retry(svc, frame.as_ptr(), n);
            }
            if !read_ok {
                // 13B ≤ 24B eco 帽；条目仍计 sent（头已发，降级可见化）。
                eco_app(console, id, b"icon partial");
            }
        }
        sent += 1;
    }
    let n = appp::encode_list_end(&mut frame, sent);
    send_retry(svc, frame.as_ptr(), n);
}

/// APP_SPAWN 处置（§5）：id → 记账表定位（boot 走查单源）→ **槽位预检** →
/// `app_launch` 全守卫复用（bap_scan→manifest→compat→ui→fs 授能→spawn
/// ——裁决二论据①「守卫单源」的字面落实）→ ACK 耦合律（ok=1 ⇒ err=0）。
/// **M-C 真异步形**（对抗审查后落地，替代 v1 同步现场裁）：app_launch 三元
/// 返尾 `Some(t)` = 拉起成立即刻入 `APP_CHILDREN`、ACK 先发（collect_eof
/// 60s + reap 30s 归稳态 `app_supervise` 异步兑现——回执不冻 echo/监督，
/// §5「reap 异步收尾不阻塞回执」冻结款字面兑现；同步形的「ACK ≥90s 毒化
/// 下轮请求」病灶随消）。ok=1 语义 = **拉起成立**（spawn 面），退出真码归
/// eco: app 行 + ps 账本异步可见。err 值域：1 = 守卫/拉起败/槽满（细节归
/// eco: app 行，槽满 = "app slots"）、2 = id 不在册。console 参数 = svcmgr
/// console（manifest console=true 的 app 诊断直打串口；false 走私有对收集
/// ——boot 批同语义，零新面）。
fn app_serve_spawn(console: Handle, svc: Handle, id: &[u8]) {
    let mut frame = [0u8; appp::FRAME_MAX];
    let account = unsafe { core::ptr::addr_of!(APP_ACCOUNT_N).read() };
    // SAFETY: 同 app_serve_list（只读面）。
    let tbl: &[AppSlot] = unsafe {
        core::slice::from_raw_parts(core::ptr::addr_of!(APP_SLOTS_TBL) as *const AppSlot, APP_SLOTS)
    };
    let mut hit: Option<(u64, u64)> = None;
    for i in 0..account {
        if &tbl[i].id[..tbl[i].id_len] == id {
            hit = Some((tbl[i].bap_off, tbl[i].bap_len));
            break;
        }
    }
    // 槽位预检在 spawn 前（M-C）：满表先拒——不拉起即无收口尾，防占位
    // 失败面；空槽序号此刻锁定（单线程独占，serve 期间无并发插槽）。
    let free: Option<usize> = {
        // SAFETY: 只读扫描面（同表读写皆单线程 serve/监督序）。
        let children: &[Option<AppSpawnTail>] = unsafe {
            core::slice::from_raw_parts(
                core::ptr::addr_of!(APP_CHILDREN) as *const Option<AppSpawnTail>,
                APP_SPAWN_LIVE_MAX,
            )
        };
        (0..APP_SPAWN_LIVE_MAX).find(|&i| children[i].is_none())
    };
    let (ok, err) = match (hit, free) {
        (None, _) => (false, 2), // id 不在册（launcher 注册表外的串号/伪造）
        (Some(_), None) => {
            // 槽满：拒新不拒旧（表内活 app 照常监督）。9B ≤ 24B eco 帽。
            eco_app(console, id, b"app slots");
            (false, 1)
        }
        (Some((off, len)), Some(slot)) => {
            // SAFETY: 同 app_serve_list——四窗 serve 期独占复用。
            let step: &mut [u8] = unsafe {
                core::slice::from_raw_parts_mut(
                    core::ptr::addr_of_mut!(STEP_WIN) as *mut u8,
                    512,
                )
            };
            let mf: &mut [u8] = unsafe {
                core::slice::from_raw_parts_mut(
                    core::ptr::addr_of_mut!(APP_MF_WIN) as *mut u8,
                    MANIFEST_MAX,
                )
            };
            let icons: &mut [UiEntry] = unsafe {
                core::slice::from_raw_parts_mut(
                    core::ptr::addr_of_mut!(APP_ICON_WIN) as *mut UiEntry,
                    UI_ENTRIES,
                )
            };
            let views: &mut [UiEntry] = unsafe {
                core::slice::from_raw_parts_mut(
                    core::ptr::addr_of_mut!(APP_VIEWS_WIN) as *mut UiEntry,
                    UI_ENTRIES,
                )
            };
            let (_, _, tail) = app_launch(
                console,
                FS_A.load(Ordering::Relaxed),
                id,
                off,
                len,
                step,
                mf,
                icons,
                views,
            );
            match tail {
                Some(t) => {
                    // 拉起成立：尾入子表（slot 由预检锁定），ACK 即刻——
                    // 收口归稳态 app_supervise（M-C 异步形）。
                    // SAFETY: 单线程独占写面（slot 此刻必空——预检后无
                    // 第二插入点）；tuple 首值谓词与 tail 同源（Ok 臂
                    // 恒 (true, _, Some)，Err 臂恒 (false, _, None)）。
                    let children: &mut [Option<AppSpawnTail>] = unsafe {
                        core::slice::from_raw_parts_mut(
                            core::ptr::addr_of_mut!(APP_CHILDREN)
                                as *mut Option<AppSpawnTail>,
                            APP_SPAWN_LIVE_MAX,
                        )
                    };
                    children[slot] = Some(t);
                    (true, 0)
                }
                // 拉起败（含守卫拒）：预检槽位自然空置，无需归还动作。
                None => (false, 1),
            }
        }
    };
    let n = match appp::encode_spawn_ack(&mut frame, ok, err) {
        Some(n) => n,
        None => trap(), // 耦合律执法面——本侧构造恒合法，不可达防御
    };
    send_retry(svc, frame.as_ptr(), n);
}

/// launcher 监督探活（裁决四-d 最小件）：`berry_object_wait_one(hp,
/// TERMINATED, deadline=0)` **poll 形**（berry-sys 文档形——非阻塞单点
/// 查询，选形依据：稳态 loop 每轮调用，阻塞形会卡死 echo 紧环；hp 恒由
/// LAUNCHER_CHILD 持有，与 guifbd 三路轮询同零成本量级）。命中 = 崩溃面：
/// reap（TERMINATED 已落 → 内部 wait 瞬回）→ ps_exit 账本收口 → eco 行
/// loud（**非 smoke 标记**——崩溃本是异常面，十段对拍不承crash行）→
/// APP_SVC 收口（对端已死，serve 面撤；已 close 值不可复用 → swap 清零）→
/// LAUNCHER_LIVE 落 0（bsh/fs 已在早前路径定局，此标志只作相真值）→
/// **一次重启帽**（LAUNCHER_RESTARTED swap 惯取——帽后再生崩溃 = 终报
/// 不再试：循环重启风暴比不重启更伤稳态，kill criteria 5 的对面）。重
/// 启 fs gift = FS_A 常驻别名现 dup（无 fs 装配形 FS_A==0 → 0 直传 =
/// [wm] 截断形，同 boot 契约）。
fn launcher_supervise(console: Handle) {
    let hp = match unsafe { &*core::ptr::addr_of!(LAUNCHER_CHILD) } {
        Some(c) => c.hp,
        None => return, // 无 launcher（rootfs 形/已收口）——no-op 零开销
    };
    let mut obs = 0u64;
    let s = berry_object_wait_one(hp, SIGNAL_TERMINATED, 0, &mut obs);
    if s == BERRY_ERR_AGAIN {
        return; // 存活常态（poll 未命中）
    }
    if s != 0 {
        trap(); // poll 野柄 = 表异常（死亡面——guifbd 同口径）
    }
    // 崩溃面：reap 全链收口（wait 已命中 → 内部 wait 瞬回；&self 借用形——
    // 柄集由其内部收链，Err = 句柄留存有界泄漏的既定政策，账本仍收口）。
    // SAFETY: 同上借用纪律（单线程独占）。
    let code = match unsafe { (&*core::ptr::addr_of!(LAUNCHER_CHILD)).as_ref() } {
        Some(c) => match c.reap() {
            Ok(c) => c,
            Err(()) => 1, // 读码失败防御面——非零标记（eco 行已响）
        },
        None => return, // 理论不可达（hp 借用自 Some 支）——防御直退
    };
    // SAFETY: 单线程独占（稳态 loop 内，装配/serve 不交叠）。
    unsafe { LAUNCHER_CHILD = None; }
    ps_exit(b"launcher", code);
    eco_line(console, b"launcher died");
    let old = APP_SVC.swap(0, Ordering::Relaxed);
    if old != 0 && berry_handle_close(old) != 0 {
        trap();
    }
    LAUNCHER_LIVE.store(0, Ordering::Relaxed);
    if LAUNCHER_RESTARTED.swap(1, Ordering::Relaxed) == 0 {
        let mut fs_dup: Handle = 0;
        // R1 判别器 v2 同门（boot arm 同款）：冻结相 BLK_OK=0 → 重启装配
        // 仍 [wm] 截断形——fs_a 别名 dup 交 launcher = park 骨架不读 fs 暂
        // 无害，段③ 布局读起即成 bsh↔launcher 双读者混流面（fsproto 串行
        // 无请求号——C-1 刀 3 同款回混危害类），形契约须一次立正。
        let fs_a = if BLK_OK.load(Ordering::Relaxed) != 0 {
            FS_A.load(Ordering::Relaxed)
        } else {
            0
        };
        if fs_a != 0 && berry_handle_duplicate(fs_a, RIGHT_SEND | RIGHT_RECV, &mut fs_dup) != 0 {
            eco_line(console, b"launcher fs");
            return;
        }
        if !launcher_assemble(console, fs_dup) {
            // 归还契约配套：败路 fs_dup 原样退回 = 本路自持（boot arm 调用
            // 点 fs_client 流 bsh，本路无下游）——显式 close 防表槽泄漏。
            if fs_dup != 0 && berry_handle_close(fs_dup) != 0 {
                trap();
            }
            eco_line(console, b"launcher restart");
        }
    }
}

/// SPAWN 活子探活（M-C γ-分·稳态侧）：逐槽两件事——① 机会性非阻塞
/// drain `out_s`（chatty-app 管堵防线：通道队列满则子写阻塞、永不达退出
/// = 槽位僵尸；EOF 帧（0 长）吞到即止并记 `eof_seen`——reap 侧
/// `collect_eof` 免死等已被本面消费的 EOF）；② poll `wait_one(hp,
/// TERMINATED, 0)` 命中 = 尾出表兑现 `app_reap_tail`（EOF/TERMINATED 先
/// 后序由退出路径保证——EOF 排队先于进程终结可观测，命中面 collect 帽内
/// 瞬收）+ 槽位回 None。poll 形选形同 `launcher_supervise`（稳态 loop
/// 每轮调用，阻塞形卡死 echo 紧环）；表扫 O(APP_SPAWN_LIVE_MAX)=4。
fn app_supervise(console: Handle) {
    // SAFETY: APP_CHILDREN 单线程独占（serve 插入/本探活/稳态并等皆顺序
    // 不交叠——PS_TAB 同纪律）。
    let children: &mut [Option<AppSpawnTail>] = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(APP_CHILDREN) as *mut Option<AppSpawnTail>,
            APP_SPAWN_LIVE_MAX,
        )
    };
    for i in 0..APP_SPAWN_LIVE_MAX {
        // take→回表形：drain 要 &mut t（eof_seen 记账），借用不撑过循环尾。
        let mut t = match children[i].take() {
            Some(t) => t,
            None => continue,
        };
        if t.out_s != 0 && !t.eof_seen {
            // drain（collect_eof 骨架的非阻塞形：数据帧丢弃、随行 handle
            // 全量 close 丢弃——P6 A-4 同款；非 -EAGAIN 错码 = 链断 trap）。
            let mut msg = [0u8; 256];
            let mut hh = [0 as Handle; 6];
            loop {
                let mut nb = msg.len();
                let mut nh = hh.len();
                let s = berry_channel_recv(
                    t.out_s,
                    msg.as_mut_ptr(),
                    &mut nb,
                    hh.as_mut_ptr(),
                    &mut nh,
                );
                if s == BERRY_ERR_AGAIN {
                    break;
                }
                if s != 0 {
                    trap();
                }
                for k in 0..nh {
                    let _ = berry_handle_close(hh[k]); // 多带 handle：丢弃勿 trap
                }
                if nb == 0 {
                    t.eof_seen = true; // EOF 已消费——reap 面免再等
                    break;
                }
            }
        }
        let mut obs = 0u64;
        let s = berry_object_wait_one(t.child.hp, SIGNAL_TERMINATED, 0, &mut obs);
        if s == BERRY_ERR_AGAIN {
            children[i] = Some(t); // 存活常态——回表续监督
            continue;
        }
        if s != 0 {
            trap(); // poll 野柄 = 表异常（launcher_supervise 同口径）
        }
        // 退出面：尾已在手（槽位 take 后即空）——收口兑现。
        app_reap_tail(console, t);
    }
}

/// P6 A-8c：vnet 服务消费自验——`DNS_QUERY("localhost")` 严格串行 RPC 往返
/// → st==ST_OK 且 addr==[127,0,0,1] 才打标记（blk_selftest 的 RPC 版样
/// 板：无 VMO 数据面，名字/地址全 inline）。NET_A==0（预拉失败/裸拓扑）
/// 静默跳过——降级行已由预拉段打过（`eco: net spawn`），勿重复。
fn dns_selftest(console: Handle) {
    let net = NET_A.load(Ordering::Relaxed);
    if net == 0 {
        return;
    }
    match dns_rpc(net, b"localhost") {
        Ok(rep) if rep.st == ndp::ST_OK && rep.addr == [127, 0, 0, 1] => {
            send_retry(console, DNS_OK.as_ptr(), DNS_OK.len());
        }
        // 应答到了但不是 127.0.0.1/非 OK——链路活着答案错，与链路断分诊。
        Ok(_) => eco_line(console, b"dns reply"),
        Err(()) => eco_line(console, b"dns rpc"),
    }
}

/// DNS 查询 RPC（严格串行，`berry-netdproto` 约定；blk_rpc 同款纪律）：
/// encode → send（EBUSY → wait WRITABLE 重试——**无随行 handle**，重发零
/// take 副作用）→ [`DNS_REPLY_CAP`] 应答帽 wait READABLE → recv 恰 5B
/// 解析。应答帽超时/错码 → Err（请求可能仍在途不重发；收口容量富余 +
/// 协议恰长由 parse_reply 把关——blk_rpc 8/1 同款）。
fn dns_rpc(h: Handle, name: &[u8]) -> Result<ndp::Reply, ()> {
    let mut req = [0u8; ndp::REQ_LEN_MAX];
    let n = match ndp::encode_request(&mut req, ndp::OP_DNS_QUERY, name) {
        Some(n) => n,
        None => return Err(()),
    };
    send_retry(h, req.as_ptr(), n);
    let mut obs = 0u64;
    if berry_object_wait_one(h, SIGNAL_READABLE, now() + DNS_REPLY_CAP, &mut obs) != 0 {
        return Err(());
    }
    let mut rep = [0u8; 16];
    let mut nb = rep.len();
    let mut nh = 0usize;
    if berry_channel_recv(h, rep.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh) != 0 {
        return Err(());
    }
    ndp::parse_reply(&rep[..nb]).ok_or(())
}

/// GUI-4 段②（rootfs 相第 46 标记）：input 域 boot 自验标记——预拉
/// gui-inputd 后 SUBSCRIBE(DELIVERY_BATCHED) RPC 往返 ST_OK 才打（状态
/// 谓词，无竞速面——A32）。与现役标记两两互非子串：`INT ok` 位 2 'P'≠'T'；
/// RX 注入相专用行 `INPUTP ok n=3` 位 5 ' '≠'P'（§3.7-④ 推演同源）。
const INPUT_OK: &[u8; 9] = b"INPUT ok\n";

/// SUBSCRIBE 应答帽：60s（GUI-4 段② §4.4 成本模型）：tablet 在席实测
/// ≈ 1-2 轮转（≤2s）→ 30x 余量；B4（tablet 缺席）= gui-inputd park 后
/// 应答永不到，帽到点即放弃（帽即烧时——B4 kill 全由 bsh 侧标记触发，
/// 墙钟零影响）。超帽 = 降级行 + 标记缺失（不重发——请求可能仍在途，
/// v1 无请求序号，dns_rpc 同款纪律）。
const INPUT_SUB_CAP_NS: u64 = 60 * 1_000_000_000;

/// GUI-4 段②：boot 期 input 域自验——SUBSCRIBE(DELIVERY_BATCHED) 严格
/// 串行 RPC，ST_OK 才打 INPUT ok（dns_selftest 样板：INPUT_A==0 静默
/// 跳过零扰动）。tablet 缺席（B4）= gui-inputd 已 spawn 但 find_input_device
/// None → park（段① :139-141），应答永不到 → 60s 帽到点 eco_line 诊断后
/// 放弃，**非致命**——B4「vnet 无 net 槽 trap 干净自死、标记自然缺席不设
/// 门」先例同款；墙钟零影响（B4 kill 全由 bsh 侧标记触发）。
fn input_selftest(console: Handle) {
    // GUI 形（wmd 在位）跳过 boot 自验：launcher 与 svcmgr 共享 INPUT_A
    // 客户端端；自验若与 launcher 的 INPUT_POLL 并发，会抢走事件应答，
    // 真实输入永久失效。桌面态把端点让给 launcher，INPUT ok 只在非 GUI 形打。
    if WMD_CLI_DUP.load(Ordering::Relaxed) != 0 {
        return;
    }
    let h = INPUT_A.load(Ordering::Relaxed);
    if h == 0 {
        return;
    }
    let mut buf = [0u8; 16];
    let n = gp::encode_input_sub_req(&mut buf, gp::DELIVERY_BATCHED);
    match input_rpc(h, &buf[..n]) {
        Some(st) if st == gp::ST_OK => {
            send_retry(console, INPUT_OK.as_ptr(), INPUT_OK.len());
        }
        // 应答到了但非 OK——链路活着答案错，与链路断分诊（dns 同款）。
        Some(_) => eco_line(console, b"input sub st"), // 诊断非致命
        None => eco_line(console, b"input rpc"), // 超时/链断——B4 形态，非致命
    }
}

/// input 域 RPC（GUI-4 段②，dns_rpc 同款纪律）：send（EBUSY → wait
/// WRITABLE 重试——无随行 handle，重发零 take 副作用）→ [`INPUT_SUB_CAP_NS`]
/// 应答帽 wait READABLE → recv 恰 1B 状态收口（`parse_status_rep`——
/// SUBSCRIBE 应答 = encode_status_rep 1B，段①烘焙线定形）。应答帽超时/
/// 错码 → None。no_std 无 alloc：应答不拷 Vec，rpc 层直收口状态字节。
fn input_rpc(h: Handle, req: &[u8]) -> Option<u8> {
    send_retry(h, req.as_ptr(), req.len());
    let mut obs = 0u64;
    if berry_object_wait_one(h, SIGNAL_READABLE, now() + INPUT_SUB_CAP_NS, &mut obs) != 0 {
        return None;
    }
    let mut rep = [0u8; 16];
    let mut nb = rep.len();
    let mut nh = 0usize;
    if berry_channel_recv(h, rep.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh) != 0 {
        return None;
    }
    gp::parse_status_rep(&rep[..nb])
}

/// TCP 面 RPC（T-d，dns_rpc 同款纪律）：encode → send（无随行 handle）→
/// [`TCPS_REPLY_CAP`] 应答帽 wait READABLE → recv 容量富余收口 →
/// parse_tcp_reply。**回显 data 借用 recv 缓冲——拷出 owned 返回**（返回
/// 后缓冲即被下一 op 复用；调用方拿 `(st, data, len)` 三件套）。应答帽
/// 超时/错码 → Err 不重发（串行 RPC 队列里可能滞留迟到应答，v1 无请求
/// 序号，blk_rpc 同款；自验序列即终，迟到应答滞留无害）。
fn tcp_rpc(h: Handle, req: ndp::TcpReq<'_>) -> Result<(u8, [u8; ndp::TCP_DATA_MAX], usize), ()> {
    let mut reqb = [0u8; ndp::REQ_LEN_MAX];
    let n = match ndp::encode_tcp_request(&mut reqb, req) {
        Some(n) => n,
        None => return Err(()),
    };
    send_retry(h, reqb.as_ptr(), n);
    let mut obs = 0u64;
    if berry_object_wait_one(h, SIGNAL_READABLE, now() + TCPS_REPLY_CAP, &mut obs) != 0 {
        return Err(());
    }
    let mut rep = [0u8; ndp::TCP_REP_MAX];
    let mut nb = rep.len();
    let mut nh = 0usize;
    if berry_channel_recv(h, rep.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh) != 0 {
        return Err(());
    }
    let pr = ndp::parse_tcp_reply(&rep[..nb]).ok_or(())?;
    let mut out = [0u8; ndp::TCP_DATA_MAX];
    if pr.data.len() > out.len() {
        return Err(());
    }
    out[..pr.data.len()].copy_from_slice(pr.data);
    Ok((pr.st, out, pr.data.len()))
}

/// T-d：vnet TCP 三 op 服务消费自验（`TCPS ok echo=64`，rootfs 相第 20
/// 标记）——OPEN(18080) → ROUNDTRIP(PATTERN) 逐字节对拍 → CLOSE 三步全
/// ST_OK 才打（dns_selftest 样板：NET_A==0 静默跳过裸跑零扰动；失败分诊
/// `tcps rpc` 链路断 / `tcps reply` 链路活答案错）。ROUNDTRIP 失败照走
/// CLOSE——vnet 侧 Timeout 不销毁会话，收口撤会话不留言体。
fn tcp_selftest(console: Handle) {
    let net = NET_A.load(Ordering::Relaxed);
    if net == 0 {
        return;
    }
    // ① OPEN：建会话（失败 = 链路断/宿主 echod 缺席，后续 op 无意义）。
    match tcp_rpc(net, ndp::TcpReq::Open { port: TCPS_ECHO_PORT }) {
        Ok((st, _, _)) if st == ndp::ST_OK => {}
        Ok(_) => {
            eco_line(console, b"tcps reply");
            return;
        }
        Err(()) => {
            eco_line(console, b"tcps rpc");
            return;
        }
    }
    // ② ROUNDTRIP 对拍（回显字节只能来自宿主 echod 真回程——状态谓词）。
    let rt_ok = match tcp_rpc(net, ndp::TcpReq::Roundtrip { data: &TCPS_PATTERN[..] }) {
        Ok((st, data, n)) if st == ndp::ST_OK && n == TCPS_PATTERN.len() && data[..n] == TCPS_PATTERN[..] => true,
        Ok(_) => {
            eco_line(console, b"tcps reply");
            false
        }
        Err(()) => {
            eco_line(console, b"tcps rpc");
            false
        }
    };
    // ③ CLOSE 无论 ② 成败都收口；三步全绿才打标记。
    match tcp_rpc(net, ndp::TcpReq::Close) {
        Ok((st, _, _)) if st == ndp::ST_OK => {
            if rt_ok {
                send_retry(console, TCPS_OK.as_ptr(), TCPS_OK.len());
            }
        }
        Ok(_) => eco_line(console, b"tcps reply"),
        Err(()) => eco_line(console, b"tcps rpc"),
    }
}

#[inline(never)]
fn throttle() {
    let mut i: u64 = 0;
    while i < THROTTLE {
        core::hint::black_box(&mut i);
        i += 1;
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己。
#[cold]
fn trap() -> ! {
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    trap()
}
