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
//! - 稳态：节流 + ECHO_REQ 实证往返 + 'v'。
//!
//! 子程序镜像 `include_bytes!` 内嵌（两遍构建：leaf bins 先产平像再编本
//! crate，见 justfile `build-user`；改子程序须先重跑 pass 1）。入口 x0 =
//! console 发端（boot 首插定值 0x0001_0001）。任何链断 → 自陷被杀 →
//! 标记缺失 = smoke FAIL 兜底。

#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_one, berry_process_create,
    berry_process_get_exit_code, berry_process_start, berry_thread_create, berry_vmar_map,
    berry_vmar_unmap, berry_vmo_commit, berry_vmo_create, berry_vmo_get_size, berry_vmo_read,
    berry_vmo_write, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BAD_HANDLE, BERRY_ERR_BUSY,
    CLOCK_MONOTONIC, RIGHT_DUPLICATE,
    RIGHT_MANAGE, RIGHT_READ, RIGHT_RECV, RIGHT_SEND, RIGHT_WRITE, SIGNAL_READABLE,
    SIGNAL_TERMINATED, SIGNAL_WRITABLE,
};
// A-2：拉起链换轨 berry-exec（「不长第二套」——spawn_rootfs_elf/channel_pair
// 迁出，bsh 的 run 同链消费；行为逐字等价）。P6 A-1：cpio_find 供 fsmgr
// 预走查（bsh gift[3] 依赖 fsmgr 先起，而 mkcpio 名序 bsh 首位）。
use berry_exec::{
    channel_pair, cpio_find, spawn_rootfs_elf, SpawnSpec, FB_VA, VIRTIO_MMIO_VA,
};
// P6 A-4：vblk 块设备服务消费自验（BLKD ok）的协议面。
use berry_blkproto::{encode_request, parse_reply, OP_READ, OP_WRITE, SECTOR_SZ, ST_OK};
// P6 A-8c：vnet 网络服务消费自验（DNS ok）的协议面。
use berry_netdproto as ndp;
// app-0 a-2：.bap manifest v0 解析面（纯函数 crate，host 23 测同源）。
use berry_manifest::{
    APP_SLOTS, BAP_MAX, ENTRY_MAX, ID_MAX, MANIFEST_MAX, SDK_CURRENT,
};
// C-3 段①：ps 查询 RPC 协议面（ps_selftest 服务端：请求解析 + 账本编码）。
use berry_psproto as psp;
// GUI-4 段②：GUI wire 协议面（input 域 boot 自验——SUBSCRIBE 请求编码 +
// 1B 状态应答收口，no_std 纯函数 crate）。
use berry_guiproto as gp;
// A-8b：vnet 常驻服务 client 端保活槽（单线程 svcmgr，Relaxed 足够）。
use core::sync::atomic::{AtomicU32, Ordering};

/// echo 服务镜像（G-eco-b 编译产物；echo1/echo2 共用同一份字节——asm 同款）。
const ECHO_BIN: &[u8] = include_bytes!("../../../userbins/echo.bin");
/// talker 镜像。
const TALKER_BIN: &[u8] = include_bytes!("../../../userbins/talker.bin");
/// T-1 §2：thrchild 镜像（进程内双线程自验子——echo/talker 同产链 pass 1
/// 平像内嵌，零 rootfs 新条目 = 裸跑/EFI 路天然同面）。
const THRCHILD_BIN: &[u8] = include_bytes!("../../../userbins/thrchild.bin");
/// hexyl 终验基准（d-3c）：上游 0.17.0（`user/bins/hexyl` 逐字内嵌）在
/// 宿主以 `--color=never` 管道跑同样 256B 输入的**逐字节输出快照**（版本随
/// Cargo.toml 钉死——升版须重产此文件，比对不等即 FAIL 是故意的）。
const HEXYL_PLAIN: &[u8] = include_bytes!("hexyl-plain.bin");

/// ECHO_REQ（4-5b 验收载荷；回显比对取低 32 位 0xE000_0E30）。
const ECHO_REQ: u64 = 0x00C0_FFEE_E000_0E30;
/// smoke 第 21 标记（7 字节）。
const SVC_OK: &[u8; 7] = b"SVC ok\n";

const TIMEOUT_NS: u64 = 10 * 1_000_000_000;
const THROTTLE: u64 = 0x30_0000;
/// 子进程布局：镜像首页 @0x1000 + 零栈页，映射 [0x1000,0x3000)，sp0=0x3000。
const CHILD_MAP_LEN: usize = 0x2000;
const CHILD_ENTRY: usize = 0x1000;
const CHILD_SP: usize = 0x3000;

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(console: Handle) -> ! {
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
    input_selftest(console); // GUI-4 段②：boot 期 input 域 SUBSCRIBE 自验（INPUT ok）

    // ---- 稳态：节流 + 实证往返 + 'v'（每个 'v' = 一次 ECHO_REQ 往返）----
    let v = [b'v'];
    loop {
        throttle();
        send_retry(end0, req.as_ptr(), 8);
        roundtrip(end0, &mut buf, &req);
        send_retry(console, v.as_ptr(), 1);
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
    // 消费）；客户端端存函数局部变量（fresh 端非 dup），交紧随本 arm 的
    // gfxdemo 预拉 arm 直递。0 = guifbd 未起 → gfxdemo arm 静默跳过。
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
    // ---- GUI-2 段③：gfxdemo（首个 gfx 消费者）预拉——紧随 guifbd arm ----
    // 门 = guifbd spawn Ok（present_cli != 0——两客户端端仅 Ok 路成对落值，
    // 兼作标志）∧ cpio_find(H_ROOTFS, b"bin/gfxdemo")。rootfs 相 guifbd 门
    // 已跳（`fb absent`）→ 标志 0 → **静默跳过零新行**（inputd INPUT_A==0
    // 静默跳过同款先例——§0「静默」的字面落实，不设 GFX absent 行）。
    // 拓扑：present/hdi 客户端端随 extras[0]/[1] **直递**（fresh 端非
    // dup——fs_b/net_b 先例；gfxdemo 侧 libc `present_h()`/`hdi_h()` 消费，
    // 0 = 自行降级退 exit(0)）；stdout=console（`GFX ok` 全链断言过自打）；
    // stdin Some(&[]) 占位（extras 非空须 stdin 槽在位——berry-exec
    // assert）。**阻塞 reap 收退出码**（stdhello 预拉同形；berry-exec
    // REAP_TIMEOUT 30s 在册）：零码零新行（GFX ok 归 gfxdemo 自打）；
    // 非零/Err 面 eco 行续走（失败隔离——标记缺失 = smoke FAIL 兜底）。
    // **不 ps_reg**（thr 先例注记——瞬态验收件不入账，PS ok n= 零漂移）。
    if present_cli != 0 {
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
                    extra_gifts: &[present_cli, hdi_cli],
                    mmio_vmo: None,
                    mmio_va: 0,
                },
            ) {
                Ok(child) => {
                    match child.reap() {
                        // 零码零新行——GFX ok 归 gfxdemo 自打（stdhello 的
                        // EXIT ok 同位，本 arm 无自证标记面）。
                        Ok(0) => {}
                        Ok(_) => eco_skip(b"gfx code"),
                        Err(()) => eco_skip(b"gfx reap"),
                    }
                }
                Err(()) => {
                    // Err 全在 env send 前——两 fresh 客户端端未被 take，
                    // 显式回收防表槽泄漏（vnet 同款；guifbd 服务端端互钉
                    // 常活，GFX/GFXF 标记已缺 = smoke FAIL 兜底）。
                    if berry_handle_close(present_cli) != 0 {
                        trap();
                    }
                    if berry_handle_close(hdi_cli) != 0 {
                        trap();
                    }
                    eco_skip(b"gfxdemo spawn");
                }
            }
        } else {
            // cpio 缺 gfxdemo（手工裁剪镜像面）：两 fresh 客户端端未被
            // take（guifbd 侧服务端端已迁入不受影响），显式回收防表槽
            // 泄漏（本 arm Err 路同款；常驻 svcmgr 表零终身滞留物）。
            if berry_handle_close(present_cli) != 0 {
                trap();
            }
            if berry_handle_close(hdi_cli) != 0 {
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
    // 失败面 = close(in_b) + close(in_a) + eco_skip(b"inputd spawn")——bsh
    // inputpoll 面静默降级（input_h()==0 → "input: absent"）。
    if berry_vmo_get_size(H_MMIO, &mut msz) != 0 {
        eco_skip(b"mmio absent");
    } else if let Some((data_off, data_len)) = cpio_find(H_ROOTFS, b"bin/gui-inputd") {
        let (in_a, in_b) = channel_pair();
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
                extra_gifts: &[in_b],
                mmio_vmo: Some(H_MMIO),
                mmio_va: VIRTIO_MMIO_VA,
            },
        ) {
            Ok(_child) => {
                ps_reg(b"inputd", psp::STATE_ALIVE, 0); // 拉起即常驻（boot 自验门成败都活——serve 常驻）
                // 常驻不 reap。`INPUT ok` 标记由 boot 期 input_selftest 打
                // （SUBSCRIBE RPC 走本端 in_a——net_a 转 client 同款）；
                // `gui-inputd: up slot=… cal=…` 诊断行（ABS 域真值源）由
                // inputd 自打。
                INPUT_A.store(in_a, Ordering::Relaxed);
            }
            Err(()) => {
                ps_reg(b"inputd", psp::STATE_SPAWNFAIL, 0);
                // Err 全在 env send 前——in_b 未被 take，两新鲜端显式回收
                // 防表槽泄漏（vnet 同款）。
                if berry_handle_close(in_b) != 0 {
                    trap();
                }
                if berry_handle_close(in_a) != 0 {
                    trap();
                }
                eco_skip(b"inputd spawn");
            }
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
                        },
                    ) {
                        // hello 常驻（10s 周期睡眠不回收）——入账。
                        Ok(_) => {
                            ps_reg(b"hello", psp::STATE_ALIVE, 0);
                            send_retry(console, ECO_OK.as_ptr(), ECO_OK.len())
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
    // P6 A-1 兜底：归档无 bin/bsh（fs 面无消费者）——fs_client/fs_a 留表
    // 无害但关掉更干净；fsmgr 常驻不受影响（fs_b 互钉 + 子表自持）。
    // A-5：归档无 bin/fsmgr 时 blk_cli 未被 take——同兜底关（vblk 经
    // pair 互钉常活，无 client 只是闲置）。
    // SDK-1a §1.2：bsh 延后拉起——bsh_geom.is_some() 时 fs 端点还要交给
    // app 批（fs 授能）与 bsh 拉起段（fs_client take），此处不得提前关
    // （旧序「bsh arm spawn Ok 已置 0」的隐式跳过条件在延后态失效，显式
    // 门取代；bsh spawn Err 面的 fs_a_h 留表 = 有界 1 槽，v1 常驻泄漏政策）。
    if bsh_geom.is_none() {
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
        for i in 0..app_n {
            if app_launch(
                console,
                fs_a_h,
                &apps[i].id[..apps[i].id_len],
                apps[i].bap_off,
                apps[i].bap_len,
                step,
                mf,
            ) {
                ok += 1;
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
    }
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
        // 落进 fs 槽 hs[5] 造成同槽不同名；fs 缺席而 vnet 在位的理论拓
        // 扑里 netd 一并让位，现无此 recipe）。
        let mut netd_dup = 0 as Handle;
        let net = NET_A.load(Ordering::Relaxed);
        if net != 0 {
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
            if fs_a_h != 0 && berry_handle_close(fs_a_h) != 0 {
                trap();
            }
        }
    }
}

// ---- C-3 段①：ps 查询 RPC——拉起账本（表驱动真值源）+ ps_selftest ----

/// ps 输出收集窗（16 行 × 最坏 ~29B 渲染 ≈ 464B；512B 帽）。
static mut PS_WIN: [u8; 512] = [0; 512];

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
    let cap: &mut [u8] =
        unsafe { core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(PS_WIN) as *mut u8, 512) };
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
    // ⑥ 标记：n = 窗内行数（≤ PS_MAX=16 → 恒 2 位十进制零填充）。
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
    let cap: &mut [u8] =
        unsafe { core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(PS_WIN) as *mut u8, 512) };
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
/// 期望退出码：五步链全过 = 7；任何步失败 = 9（thrchild src/main.rs 头注）。
const THR_OK_CODE: i32 = 7;
/// join 应答帽 60s：TCG 下 spawn→跑→收口「慢而活」不误杀（M3DEMO cp 步
/// ≈17s 定标同口径）；超帽 = 降级行 + 标记缺失 = smoke FAIL，不重发。
const THR_CAP_NS: u64 = 60 * 1_000_000_000;
/// T-1 §2 标记（7 字节；rootfs 相第 36 标记，EFI 路 ECO 同面）。
const THR_OK: &[u8; 7] = b"THR ok\n";

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
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + TIMEOUT_NS, &mut obs) != 0 {
            send_retry(console, b"hexyl: no EOF\n".as_ptr(), 14);
            return; // EOF 永不来 = 子进程断链/卡死（Plan B 起 exit 族必经 EOF——abort 亦先行）——loud 降级
        }
    }
    if got == HEXYL_PLAIN.len() && cap[..got] == HEXYL_PLAIN[..got] {
        send_retry(console, b"HEXYL ok\n".as_ptr(), 9);
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
fn wait(h: Handle, signals: u64) {
    let mut obs = 0u64;
    if berry_object_wait_one(h, signals, now() + TIMEOUT_NS, &mut obs) != 0 {
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
            send_retry(console, b"u1: no EOF\n".as_ptr(), 11);
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
/// 第三条目在场记账（SDK-1a——v1 只记不读不验，§2.4 三理由）。
struct BapScan {
    mf_len: usize,
    entry_len: usize,
    elf_off: u64,
    elf_len: u64,
    has_sums: bool,
}

/// .bap 内层 cpio 走查（mkapp 生产形状：恰 `manifest.toml` + 唯一 `bin/`
/// 条目 + 可选 `SHA256SUMS`（SDK-1a §2.4 起新产包第三条目常态在场，
/// 2 条目历史形照收）+ TRAILER 自终止）。rootfs 是外部输入面——消费侧
/// 独立执法（mkapp 门禁只约束本仓产物，拼装盘上的 .bap 不凭生产端
/// 承诺）：重复/未知条目一律 `bap shape` 拒。manifest 可达 1KiB >
/// 步进窗 512B——**专用 vmo_read 直读进 mf 窗**（步进窗只须盖头+名）；
/// ELF 只记几何（段泵送在 spawn_rootfs_elf，归档永不整档驻留）。
/// manifest 读窗预算（附 A）恒安全：APP_MF_WIN = MANIFEST_MAX = 1024B
/// ≥ 最坏 395B（v1 全量段）——SDK-1a 无需扩窗。
fn bap_scan(
    step: &mut [u8],
    mf: &mut [u8],
    entry: &mut [u8],
    bap_off: u64,
    bap_len: u64,
) -> Result<BapScan, &'static [u8]> {
    let mut pos = 0u64;
    let mut seen_mf = false;
    let mut seen_bin = false;
    let mut seen_sums = false;
    let mut out =
        BapScan { mf_len: 0, entry_len: 0, elf_off: 0, elf_len: 0, has_sums: false };
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

/// 单 app 拉起（ADR #43 能力面 + §4.3 生命周期 v0：拉起 → 收口 → reap）。
/// console=true → stdout 直打串口（子自证输出，无收集面）；false → 私有对
/// 端作 fd1/2（hexyl 同形：`channel_pair` 的 spawn 侧交付、收集端留本表
/// ——appquiet 静默活证）。**先 collect 后 reap**（PIPE 死锁序铁律——反序
/// = 子堵收集队 + 父等退出三方互等）。reap 真码 0 才算过：argv0 断言错 →
/// abort(134) → 非零 → 降级行 + `APP ok` 缺失 loud。任何失败 = `eco: app`
/// 行 + false，svcmgr 存活续走（失败隔离：单 app 坏不连坐整批）。
///
/// SDK-1a：`fs_a` = fsmgr 预拉对 a 侧（§1.2 新序下 app 批窗口 svcmgr 仍
/// 持 `fs_a_h`）；fs 授能不可达值 0 传参由 §5.2 `fs none` 降级路径消化。
fn app_launch(
    console: Handle,
    fs_a: Handle,
    id: &[u8],
    bap_off: u64,
    bap_len: u64,
    step: &mut [u8],
    mf: &mut [u8],
) -> bool {
    let mut entry = [0u8; ENTRY_MAX];
    let sc = match bap_scan(step, mf, &mut entry, bap_off, bap_len) {
        Ok(sc) => sc,
        Err(why) => {
            eco_app(console, id, why);
            return false;
        }
    };
    let man = match berry_manifest::parse(&mf[..sc.mf_len]) {
        Ok(m) => m,
        Err(e) => {
            eco_app(console, id, e.reason_str().as_bytes());
            return false;
        }
    };
    // 双守卫：manifest.id ↔ .bap 文件名（走查名 = 真理源）、manifest.entry
    // ↔ 包内条目名（mkapp 门禁 3 的消费侧镜像——rootfs 可被拼装）。
    if man.id.as_bytes() != id {
        eco_app(console, id, b"id mismatch");
        return false;
    }
    if man.entry.as_bytes() != &entry[..sc.entry_len] {
        eco_app(console, id, b"entry mismatch");
        return false;
    }
    // SDK-1a §5.2 步骤 1：compat min 装载期检查（ADR #55 工具链三值域的
    // 运行时执法点）。min > SDK_CURRENT = 本系统比包声称的最低线还老 →
    // loud 降级行拒拉（fsmgr diag("geom") 同式）；target/compile 是打包
    // 期面向（mkapp 已闸），装载期不查。此处 out_s 未建、dup 未造——
    // 零清理直退。
    if let Some(c) = man.compat {
        if c.min > SDK_CURRENT {
            eco_app(console, id, b"compat min"); // 10B ≤ 24B
            return false;
        }
    }
    // SDK-1a §5.2 步骤 2：net 词 loud 拒（档 (b)——schema/mkapp 收、appd
    // 拒：v1 无网络服务可安全授能，词表先行 ≠ 无服务硬授，挂账 §10 O3）。
    // 同样零清理直退（console 两态尚未分岔）。
    if man.net.is_some() {
        eco_app(console, id, b"net unserved"); // 11B ≤ 24B
        return false;
    }
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
            return false;
        }
        if berry_handle_duplicate(H_ROOTFS, RIGHT_READ, &mut rootfs_dup) != 0 {
            eco_app(console, id, b"fs dup"); // 6B ≤ 24B
            if out_s != 0
                && (berry_handle_close(out_s) != 0 || berry_handle_close(stdout) != 0)
            {
                trap();
            }
            return false;
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
            return false;
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
            ps_reg(id, psp::STATE_SPAWNFAIL, 0);
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
            ps_reg(id, psp::STATE_ALIVE, 0);
            if out_s != 0 {
                // console=false：子表持的是 stdout 的 dup 别名（spawn 内 dup
                // RIGHT_SEND 随 env 块迁移），原值 c 留本表——先收 EOF（d-3b
                // 写侧对称面——libc exit 族 EOF 先行，Plan B 起 abort 亦然）
                // 再关两端（hexyl 臂同款：收集端 + 本表原值副本）。帽内未到
                // = 断链/卡死。
                if !collect_eof(out_s) {
                    eco_app(console, id, b"no EOF");
                }
                if berry_handle_close(out_s) != 0 || berry_handle_close(stdout) != 0 {
                    trap();
                }
            }
            match child.reap() {
                Ok(0) => {
                    ps_exit(id, 0);
                    return true;
                }
                Ok(c) => {
                    ps_exit(id, c);
                    eco_app(console, id, b"exit code");
                }
                Err(()) => eco_app(console, id, b"app reap"),
            }
        }
    }
    false
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
