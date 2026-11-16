//! berry-launcher —— GUI-8 桌面 launcher（docs/GUI-8实施设计草案.md §4
//! 段③；前身 = gui3 前置 wiring 刀 v0 park 骨架）。
//!
//! **模块树（段③ W1 起）**：
//!
//! - [`icons`]：app 注册表 + berry-ui `IconSource` 供件桥（W1 空注册表
//!   骨架——W2 = APP_LIST 拉取装载）。
//! - [`layout_io`]：LAYOUT.JSN 读写三约束（缺席/坏件回默认 + 重写自愈
//!   + 写失败非致命；冻结相 `fs_h()==0` 默认直返零 RPC）。
//! - [`desktop`]：桌面静态面建树（OH 官方壁纸/phone 状态栏/SwipePager 四页
//!   Grid 5×4/常驻 Dock/a11y 32/32——裁决六/八；几何全 GETINFO 动态——
//!   裁决 D）。
//! - [`frameloop`]：长活帧循环（W2 填充——W1 静默 park）。
//!
//! 本文件保留面：wm 门控 + 严格串行 RPC 半程族（`rpc_send`/`rpc_recv`
//! 单趟拼装——W2 起 icons 流式会话与 frameloop attach 面共用；gfxdemo
//! `rpc` 形态镜像）+ `LAUNCH v=` 非标记行 + CRT0 链接种子 + `fail`/park
//! 常驻纪律（崩溃监督一次重启帽归 svcmgr 域——裁决四-d）。
//!
//! **门控面（wm_cli_h()）**：
//!
//! - wm 客户端端未交付（0——rootfs 相 wmd 随 fb 探测门不被拉起 → 本
//!   程序不被拉起；亦含理论不可达兜底 = 一切无预拉 spawn/bsh 手跑）→
//!   **单行降级输出 + park**（v0 骨架形态延续，hdi_srv_h 先例；勿改成
//!   gfxdemo 的 exit(0) 形——`LAUNCH wm absent` 永不进 MARKERS，零标记
//!   扰动）。
//! - 在场（EFI 路）→ 严格串行一次 WM_GETINFO RPC（gfxdemo `rpc` 形态
//!   镜像精简：send -EBUSY → WRITABLE 10s 帽重臂；recv -EAGAIN →
//!   READABLE 10s 帽；无 attach 面——GETINFO 无随行 handle）→ parse
//!   `WmGetinfoRep` → **单行非标记日志**（`LAUNCH` 前缀 canon 零碰撞
//!   ——2026-11-02 grep 全树实证；无 " ok" 形 = 非 smoke 标记语义）。
//!   **原值宁报勿猜**（fmt 原值先例）：不做 gfxdemo 式断言链（那是
//!   WM ok 标记语义面，launcher 非标记面）；parse 失败 = eprintln
//!   诊断 + park（不留半开态）。
//!
//! **输出面（W2 起）**：EFI 相 launcher 输出 = `LAUNCH v=…` 非标记行
//!（GETINFO 后）+ 帧循环两标记（`LAUNCH ok` 首帧呈现 / `PAGE ok n=2`
//! 合成 swipe 双翻页——字节精确，frameloop 自测产物）；icons/layout_io
//! 常态零打印（eprintln 仅失败诊断面）。
//!
//! **段④ 标记面（全部结构性门控——EFI 冻结相字节零扰动）**：
//! `LAYOUTW ok` / `LAYOUT ok n=<cells>`（数据盘根 `GUI8.SMK` **载荷恰为**
//! `BERRYOS-GUI8-SMOKE-V1\n` 才进 layout_io 冒烟往返；`fs_h()==0` 冻结相
//! 在哨兵探测之前直返）+ `CLICK ok n=1`（真实 `Msg::Click` → APP_SPAWN
//! ok 回执首例，且仅在冒烟模式）+ `launcher: click ready`（非标记状态谓词
//! 门，仅冒烟模式 + `input_h()==0` 恒不打）。哨兵在主 main **只读一次**
//! （[`layout_io::smoke_mode`]），bool 同时传入 layout 与 frameloop；缺失/
//! 错载荷 = false（fail closed）。CLICK/click ready 皆「每进程一次」——
//! 监督重启可合法重打，smoke 侧一律 grep -qF 存在性断言，禁升恰一次。
//! **正常交互 boot 仍可点击 spawn，但不打 CLICK / click ready 任一行。**

mod cbox_io;
mod desktop;
mod frameloop;
mod icons;
mod layout_io;

use berry_appshell::{self as appshell, Fs, Host};
use berry_gfx::font::FontSize;
use berry_guiproto as gp;
use berry_libc::io::{app_cli_h, input_h, self_proc_h, self_vmar_h, wm_cli_h};
use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_clock_read,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_one, berry_thread_sleep,
    berry_thread_yield, berry_vmar_map, berry_vmar_unmap, berry_vmo_commit, berry_vmo_create,
    Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC, RIGHT_DUPLICATE, RIGHT_READ, RIGHT_TRANSFER,
    RIGHT_WRITE, SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_libc::io::fs_h;
use berry_ui::theme::Density;
use berry_ui::UiMain;

// 链接种子（fsmgr/guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（gfxdemo 10s A6-a 形态；launcher 600s：首启要等
/// svcmgr 跑完 boot 自验进稳态 app serve。BERRYDEMO 把真实 GUI app 数抬到
/// 7 后，TCG 全 app boot 批可超过旧 300s 帽；真机仍远低于该值）。
const RPC_CAP_NS: u64 = 600 * 1_000_000_000;

/// 诊断 + park 收口（gfxdemo `fail` 的 park 形——launcher 常驻不被 reap，
/// exit 码无消费者；park = stdout EOF + 10s 周期睡眠，guifbd 自验后
/// park 同款；崩溃监督/一次重启帽归 svcmgr，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("launcher: {what}");
    berry_libc::park()
}

/// appshell 的 libc 注入面：诊断/park 与 FAT fd 半程都来自本进程的
/// berry-libc。未绑定时 `conf::read_key` 会把 Host::fs 视为缺席并回 EIO，
/// 使数据盘 LAYOUT.JSN 被误判坏件。
static SHELL_HOST: Host = Host {
    tag: "launcher",
    diag: shell_diag,
    park: berry_libc::park,
    fs: Some(Fs {
        open: shell_open,
        read: shell_read,
        write: shell_write,
        close: shell_close,
        errno: shell_errno,
    }),
};

fn shell_diag(line: &str) {
    eprintln!("{line}");
}

unsafe fn shell_open(path: *const u8, flags: i32) -> i32 {
    berry_libc::fs::open(path.cast(), flags)
}

fn shell_read(fd: i32, buf: *mut u8, len: usize) -> isize {
    berry_libc::io::read(fd, buf.cast(), len)
}

fn shell_write(fd: i32, buf: *const u8, len: usize) -> isize {
    berry_libc::io::write(fd, buf.cast(), len)
}

fn shell_close(fd: i32) -> i32 {
    berry_libc::io::close(fd)
}

fn shell_errno() -> i32 {
    berry_libc::get_errno()
}

/// now = clock_read；失败按 0 → deadline 即刻到期 = fail-fast（gfxdemo
/// `now` 同口径——时钟不可用当死路，不静默自旋）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 即关（RELEASE 归还别名——关失败 = handle 表泄漏，FAIL loud；gfxdemo
/// `close_h` 同形）。
fn close_h(h: Handle) {
    if berry_handle_close(h) != 0 {
        fail("handle close");
    }
}

/// 编码收口（None = 本程序侧组帧 bug——定容 256B 缓冲恒够，结构性不可
/// 达；gfxdemo `enc_req` 同形）。
fn enc_req(m: Option<usize>) -> usize {
    match m {
        Some(n) => n,
        None => fail("req encode"),
    }
}

/// 严格串行 RPC **发送半程**（gfxdemo `rpc` 发送侧同形拆出——icons 的
/// APP_LIST 是一请求多应答分片流，发送与接收须拆开复用）。-EBUSY →
/// 阻塞等 WRITABLE（10s 帽重臂，雷 2 纪律不自旋；send 原子性——EBUSY/
/// 错码时随行 handle 零迁移）。**send 即 take**（§9 A6-b）：attach 随行
/// handle 在 send 成功即迁出本表——SUBMIT 每帧需 fresh dup 别名
///（frameloop 消费面）。
fn rpc_send(ch: Handle, req: &[u8], attach: &[Handle]) {
    let (hptr, hn) = if attach.is_empty() {
        (core::ptr::null(), 0)
    } else {
        (attach.as_ptr(), attach.len())
    };
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(ch, req.as_ptr(), req.len(), hptr, hn);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            fail(&format!("rpc send err={s}"));
        }
        if deadline == 0 {
            deadline = now() + RPC_CAP_NS;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            fail("rpc send wait timeout");
        }
        deadline = 0; // 重臂（命中后仍满则续等——公平排队）
    }
}

/// 严格串行 RPC **收侧半程**（单条应答消息；流式会话逐消息复用）：
/// 空队 -EAGAIN → 阻塞等 READABLE（`RPC_CAP_NS` 帽）。返回 `(应答长,
/// 随行 handle 数)`；`rhs` 出参 = 应答随行 handle（RELEASE 归还别名——
/// 调用方即关）。每轮重置容量（in-out 出参——漏重置 = 恒 -EINVAL 死
/// 循环，A1-b）。
fn rpc_recv(ch: Handle, rep: &mut [u8; 256], rhs: &mut [Handle; 8]) -> (usize, usize) {
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(ch, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        if s == 0 {
            if nb == 0 {
                fail("rpc empty reply");
            }
            return (nb, nh);
        }
        if s != BERRY_ERR_AGAIN {
            fail(&format!("rpc recv err={s}"));
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            fail("rpc reply timeout 600s");
        }
    }
}

/// 严格串行 RPC 单趟（一请求一应答；gfxdemo `rpc` 同形——wm/present 域
/// 消费面）。发送侧：-EBUSY → 阻塞等 WRITABLE（RPC_CAP_NS 帽重臂）。收侧：
/// 空队 -EAGAIN → 阻塞等 READABLE（`RPC_CAP_NS` 帽）。返回 `(应答长,
/// 随行 handle 数)`。
fn rpc(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (usize, usize) {
    rpc_send(ch, req, attach);
    rpc_recv(ch, rep, rhs)
}

/// FRAME_CALLBACK request RPC with explicit reply handoff. The request itself
/// wakes the wmd relay; yielding after send dispatches that relay without a
/// fair-round delay. It is deliberately callback-only for the same pipeline
/// reason as wmd/vgpud: SUBMIT ACK must not force another producer enqueue.
fn rpc_callback(
    ch: Handle,
    req: &[u8],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (usize, usize) {
    rpc_send(ch, req, &[]);
    let _ = berry_thread_yield();
    rpc_recv(ch, rep, rhs)
}

/// SUBMIT request RPC with cooperative dispatch to wmd. The reply itself is
/// still governed by wmd (system ACK may hand back; APP ACK never does here).
fn rpc_submit(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (usize, usize) {
    rpc_send(ch, req, attach);
    let _ = berry_thread_yield();
    rpc_recv(ch, rep, rhs)
}

/// 1B 状态应答单趟收口（VOTE 烘焙线：一切非 OK 应答恒 1B 不带数据；
/// gfxdemo `expect_ok` 同形——fail 收口 = eprintln + park）。
fn expect_ok(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
    what: &str,
) {
    let (n, _) = rpc(ch, req, attach, rep, rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(gp::ST_OK) => {}
        Some(st) => fail(&format!("{what} st={st}")),
        None => fail(&format!("{what} rep bad len={n}")),
    }
}

/// Phase 1/3 WM_CONNECT：为调用方建立一对独立通道（WM/RPC + input）。
/// broker 生命周期由调用方管理——launcher 需再开一个 APP session 做真实
/// owner 输入路由验收。
fn wm_connect_client(
    broker: Handle,
    process: Handle,
    client_type: u8,
    system_seed: Option<u64>,
) -> (Handle, Handle) {
    let mut local: Handle = 0;
    let mut remote: Handle = 0;
    let mut input_local: Handle = 0;
    let mut input_remote: Handle = 0;
    if process == 0
        || berry_channel_create(0, &mut local, &mut remote) != 0
        || local == 0
        || remote == 0
        || berry_channel_create(0, &mut input_local, &mut input_remote) != 0
        || input_local == 0
        || input_remote == 0
    {
        fail("wm connect channel");
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let mut token_sender: Handle = 0;
    let mut token_receiver: Handle = 0;
    let attach: [Handle; 4] = if client_type == gp::WM_CLIENT_SYSTEM {
        let Some(seed) = system_seed else {
            fail("wm system seed absent");
        };
        if berry_channel_create(0, &mut token_sender, &mut token_receiver) != 0
            || token_sender == 0
            || token_receiver == 0
        {
            fail("wm system seed channel");
        }
        if berry_channel_send(
            token_sender,
            seed.to_le_bytes().as_ptr(),
            core::mem::size_of::<u64>(),
            core::ptr::null(),
            0,
        ) != 0
        {
            fail("wm system seed send");
        }
        [remote, input_remote, process, token_receiver]
    } else {
        [remote, input_remote, process, 0]
    };
    let attach_count = if client_type == gp::WM_CLIENT_SYSTEM { 4 } else { 3 };
    let m = enc_req(gp::encode_wm_connect_req(&mut req, client_type));
    let (n, _) = rpc(
        broker,
        &req[..m],
        &attach[..attach_count],
        &mut rep,
        &mut rhs,
    );
    let _ = gp::parse_wm_connect_rep(&rep[..n]).unwrap_or_else(|| fail("wm connect rep bad"));
    if token_sender != 0 && berry_handle_close(token_sender) != 0 {
        fail("wm system seed close");
    }
    (local, input_local)
}

/// argv[2] 可选 `product`：产品镜像跳过 boot 期合成 swipe 自测。
fn product_mode_from_argv() -> bool {
    std::env::args().nth(2).as_deref() == Some("product")
}

/// argv[1] 是 svcmgr 传入的 16 字符 hex SYSTEM seed。
fn system_seed_from_argv() -> Option<u64> {
    let arg = std::env::args().nth(1)?;
    let bytes = arg.as_bytes();
    if bytes.len() != 16 {
        return None;
    }
    let mut value = 0u64;
    for &b in bytes {
        let nibble = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            _ => return None,
        };
        value = (value << 4) | u64::from(nibble);
    }
    Some(value)
}

/// R3 非阻塞拉 config event；`None` 仅代表 ST_EAGAIN。
fn wm_config_poll(ch: Handle) -> Option<gp::WmConfigEvent> {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_config_poll_req(&mut req);
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_wm_config_event_rep(&rep[..n]) {
        Some(Ok(ev)) => Some(ev),
        Some(Err(gp::ST_EAGAIN)) => None,
        other => fail(&format!("wm config poll got {other:?}")),
    }
}

/// R3 ACK 原始状态面（错/旧 serial 必须可观察为 ST_DENIED）。
fn wm_config_ack(ch: Handle, window_id: u16, serial: u32) -> u8 {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let wire = gp::WmConfigAckReq { window_id, serial };
    let m = match gp::encode_wm_config_ack_req(&mut req, &wire) {
        Some(m) => m,
        None => fail("wm config ack encode"),
    };
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(st) => st,
        None => fail(&format!("wm config ack rep bad st={}", rep[0])),
    }
}

/// 产品系统壳保留面：状态栏与底部导航是 SYSTEM 层真实窗口。普通 APP 的
/// `AVOID_SYS` 窗因此得到权威 safe area；像素仍由 launcher surface0 供给。
fn create_system_reserve(
    hw: Handle, name: &[u8], x: u16, y: u16, w: u16, h: u16,
) -> u16 {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let create = gp::WmCreateReq {
        layer: gp::WM_LAYER_SYSTEM,
        flags: 0,
        parent: 0,
        group: 0,
        x,
        y,
        w,
        h,
    };
    let m = match gp::encode_wm_create_req(&mut req, &create) {
        Some(m) => m,
        None => fail("system reserve encode"),
    };
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let id = match gp::parse_wm_create_ack(&rep[..n]) {
        Some(id) if id != 0 => id,
        _ => fail(&format!("system reserve {} ack st={}", String::from_utf8_lossy(name), rep[0])),
    };
    while let Some(event) = wm_config_poll(hw) {
        if event.window_id != id || wm_config_ack(hw, id, event.serial) != gp::ST_OK {
            fail("system reserve config");
        }
    }
    println!(
        "WMSYSRESERVE ok id={} {}x{}+{}+{}",
        id, w, h, x, y
    );
    id
}

/// Configure and submit one launcher-owned transparent SYSTEM bar overlay.
///
/// The bar-local VMO carries straight ARGB8888. wmd caches its READ alias and
/// re-blends the bar after every later APP/full-desktop submission, so normal
/// edge-to-edge app content can extend beneath status/navigation.
fn submit_system_overlay(
    hw: Handle, id: u16, w: u16, h: u16, status: bool,
) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];

    let surface = gp::surface::SurfaceModeReq {
        surface_id: id,
        role: gp::surface::SURFACE_ROLE_SYSTEM,
        mode: gp::surface::SURFACE_MODE_RETAINED,
        input_classes: 1,
        ime_mode: 0,
        a11y_mode: 0,
    };
    let m = match gp::surface::encode_wm_surface_mode_req(&mut req, &surface) {
        Some(m) => m,
        None => fail("system overlay surface encode"),
    };
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let info = match gp::surface::parse_wm_surface_mode_rep(&rep[..n]) {
        Some(Ok(info)) => info,
        other => fail(&format!("system overlay surface id={id} got {other:?}")),
    };
    if !gp::surface::surface_mode_grant_matches(&surface, &info)
        || !gp::surface::surface_mode_info_fits(&info, u32::from(w), u32::from(h))
    {
        fail("system overlay surface grant mismatch");
    }

    let stride = u32::from(w).saturating_mul(4) as usize;
    let bytes = stride.saturating_mul(h as usize);
    let map_len = (bytes + 0xFFF) & !0xFFF;
    let mut vmo = 0;
    if berry_vmo_create(map_len as u64, 0, &mut vmo) != 0
        || vmo == 0
        || berry_vmo_commit(vmo, 0, map_len as u64) != 0
    {
        fail("system overlay vmo");
    }
    let filled = if status {
        icons::write_system_status_overlay(vmo, u32::from(w), u32::from(h))
    } else {
        icons::write_system_navigation_overlay(vmo, u32::from(w), u32::from(h))
    };
    if !filled {
        let _ = berry_handle_close(vmo);
        fail("system overlay pixels");
    }

    let mut alias = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0 || alias == 0 {
        let _ = berry_handle_close(vmo);
        fail("system overlay dup");
    }
    let submit = gp::PresentReq::Submit {
        surface_id: u32::from(id),
        buffer_id: 0x53595342,
        flags: gp::SUBMIT_F_DAMAGE_FULL,
        rate_hint: 0,
        target_present_ns: 0,
        rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
        rect_count: 0,
    };
    let m = enc_req(gp::encode_present_req(&mut req, &submit));
    let (n, _) = rpc_submit(hw, &req[..m], &[alias], &mut rep, &mut rhs);
    if gp::parse_submit_ack(&rep[..n]).is_none()
        && gp::parse_status_rep(&rep[..n]) != Some(gp::ST_OK)
    {
        fail(&format!("system overlay submit id={id} st={}", rep[0]));
    }
    if berry_handle_close(vmo) != 0 {
        fail("system overlay vmo close");
    }
    println!("WMSYSOVERLAY ok id={id} {}x{} status={}", w, h, status as u8);
}

fn main() {
    appshell::bind(&SHELL_HOST);

    // —— 降级门：wm 客户端端未交付（rootfs 相 wmd 随 fb 探测门不被拉起
    // → 本程序不被拉起；bsh 手跑无 extras）→ 一行诊断 + park（v0 骨架
    // 形态延续——理论不可达兜底，hdi_srv_h 先例；勿改成 gfxdemo 的
    // exit(0) 形）。
    let broker = wm_cli_h();
    if broker == 0 {
        eprintln!("LAUNCH wm absent (no wm client gift)");
        berry_libc::park();
    }
    let process = self_proc_h();
    if process == 0 {
        fail("wm self process");
    }
    let mut app_process: Handle = 0;
    if berry_handle_duplicate(
        process,
        RIGHT_DUPLICATE | RIGHT_TRANSFER,
        &mut app_process,
    ) != 0
        || app_process == 0
    {
        fail("wm app process dup");
    }
    let system_seed = system_seed_from_argv();
    if system_seed.is_none() {
        fail("wm system seed argv");
    }
    let (hw, session_input) = wm_connect_client(
        broker,
        process,
        gp::WM_CLIENT_SYSTEM,
        system_seed,
    );

    // —— WM_GETINFO 一次握手（4B 无参请求 / 12B 应答——wmd 权威能力面，
    // disp 域 = wmd 启动期 present GETINFO 缓存同源）。原值宁报勿猜——
    // 不做 gfxdemo 式断言链（WM ok 标记语义面归 gfxdemo，launcher 非标记
    // 面）；parse 失败 = eprintln 诊断 + park（不留半开态）。
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_getinfo_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let info = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("wm getinfo rep bad st={}", rep[0])),
    };
    // 单行非标记日志（LAUNCH 前缀 canon 零碰撞；无 " ok" 形）——EFI 冻结
    // 相 launcher 常态启动唯一输出行（GUI8 哨兵相另有 WMRTWIN/交互标记；
    // icons/layout_io 仍保持冻结相静默）。原值直报（v0 fmt 先例延续）。
    println!(
        "LAUNCH v={} wmax={} {}x{} input={}",
        info.version_wm, info.windows_max, info.disp_w, info.disp_h, input_h()
    );

    // Product topology owns persistent system bars. GUI8's compact validation
    // topology intentionally stays unchanged; its occlusion tests create their
    // own SYSTEM window.
    let product_boot = product_mode_from_argv();
    let mut system_reserves = None;
    if product_boot {
        let product_density =
            Density::for_viewport(u32::from(info.disp_w), u32::from(info.disp_h));
        let status_h = match product_density {
            Density::Product => 117,
            Density::Compact => 44,
        };
        let nav_h = match product_density {
            Density::Product => 108,
            Density::Compact => 48,
        };
        let status = create_system_reserve(hw, b"status", 0, 0, info.disp_w, status_h);
        let nav_y = info.disp_h.saturating_sub(nav_h);
        let navigation =
            create_system_reserve(hw, b"navigation", 0, nav_y, info.disp_w, nav_h);
        submit_system_overlay(hw, status, info.disp_w, status_h, true);
        submit_system_overlay(hw, navigation, info.disp_w, nav_h, false);
        system_reserves = Some((status, navigation));
    }

    // Phase 4 全量 CJK A2 面 boot 实探：两个高频字都不在旧 16 字标签
    // 子集中，必须在客机完成 zlib 页解压 + A2 nibble 解包后才可能命中。
    for cp in [u32::from('中'), u32::from('国')] {
        if FontSize::S32x48
            .atlas()
            .coverage_cp(cp)
            .is_none()
        {
            fail(&format!("CJK A2 probe cp={cp:#x}"));
        }
    }
    println!("CJKPROBE ok n=2");

    // —— 段③ 静态面接线（W1：树成帧不驱——frameloop 静默 park）——
    // 注册表初始化（W2 = app 域 APP_LIST 拉取装载；冻结相 app_cli_h()==0
    //（extras 恰 [wm]）→ 空注册表——草案 R1 修约 1/2）。此步也是 fs 挂载
    // 就绪的串行点：APP_LIST 应答前 appd/fsmgr 已进稳态。
    icons::init(app_cli_h());
    // —— Phase 3 真实 owner 输入路由验收面（仅 GUI8 哨兵模式）：launcher
    // 再开一个 APP session，在右下角建可见窗。该窗口不参与桌面 UI；QMP
    // 点入它时，wmd 必须把 window-local 事件投到 APP input 通道。常态
    // boot 不建，避免产品桌面右下角出现不可见输入占位。
    let fs_ready = fs_h() != 0;
    let smoke = layout_io::smoke_mode(fs_ready);
    let mut app_input: Handle = 0;
    if smoke {
        let (app_hw, input) =
            wm_connect_client(broker, app_process, gp::WM_CLIENT_APP, None);
        app_input = input;
        let aw = (info.disp_w / 8).max(64);
        let ah = info.disp_h;
        let ax = info.disp_w.saturating_sub(aw);
        let ay = 0;
        let occluder_h = 32u16;

        // R2 真链前置：SYSTEM session 造真实 SYSTEM-layer 遮挡窗。它只在
        // GUI8 哨兵相存在，不迁移 GUI-8 裁决四-e 的常态 launcher 状态栏。
        let sreq = gp::WmCreateReq {
            layer: gp::WM_LAYER_SYSTEM,
            flags: 0,
            parent: 0,
            group: 0,
            x: ax,
            y: 0,
            w: aw,
            h: occluder_h,
        };
        let m = enc_req(gp::encode_wm_create_req(&mut req, &sreq));
        let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
        let sys_win = match gp::parse_wm_create_ack(&rep[..n]) {
            Some(id) if id != 0 => id,
            _ => fail(&format!("wm system occluder ack bad st={}", rep[0])),
        };
        println!(
            "WMSYSOCC ok id={sys_win} x={ax} y=0 w={aw} h={occluder_h}"
        );

        // APP route 窗改为整右带 + AVOID_SYS：raw bounds 保持 aw×disp_h，
        // visible bounds 顶部让出 32px。后续 QMP 路由仍点 raw 中心。
        let areq = gp::WmCreateReq {
            layer: gp::WM_LAYER_APP,
            flags: gp::WM_CREATE_F_AVOID_SYS,
            parent: 0,
            group: 0,
            x: ax,
            y: ay,
            w: aw,
            h: ah,
        };
        let m = enc_req(gp::encode_wm_create_req(&mut req, &areq));
        let (n, _) = rpc(app_hw, &req[..m], &[], &mut rep, &mut rhs);
        let app_win = match gp::parse_wm_create_ack(&rep[..n]) {
            Some(id) if id != 0 => id,
            _ => fail(&format!("wm route window ack bad st={}", rep[0])),
        };
        let m = enc_req(Some(gp::encode_wm_table_get_req(&mut req)));
        let (n, _) = rpc(app_hw, &req[..m], &[], &mut rep, &mut rhs);
        let snapshot = match gp::parse_wm_snapshot_rep(&rep[..n]) {
            Some(Ok(s)) => s,
            _ => fail(&format!("wm avoid table bad st={}", rep[0])),
        };
        let entry = snapshot
            .entries
            .iter()
            .take(snapshot.count)
            .find(|e| e.id == app_win)
            .unwrap_or_else(|| fail("wm avoid table entry"));
        let expected = (
            ax,
            occluder_h,
            aw,
            ah.saturating_sub(occluder_h),
        );
        if (entry.x, entry.y, entry.w, entry.h) != expected {
            fail(&format!(
                "wm avoid geometry got {}x{}+{}+{} want {}x{}+{}+{}",
                entry.w, entry.h, entry.x, entry.y,
                expected.2, expected.3, expected.0, expected.1,
            ));
        }
        println!(
            "WMAVOIDWIN ok id={app_win} raw={ax},{ay},{aw},{ah} \
             visible={},{},{},{} safe=0,{occluder_h},0,0",
            entry.x, entry.y, entry.w, entry.h
        );

        // R3 initial config pull-once：CREATE 后必须拿到 CREATED event，且
        // raw/visible 双几何与权威表一致。错 serial ACK 先负例（ST_DENIED），再精确 ACK。
        let created = wm_config_poll(app_hw)
            .unwrap_or_else(|| fail("wm config created absent"));
        if created.window_id != app_win
            || created.cause != gp::WM_CONFIG_CAUSE_CREATED
            || !created.visible
            || (created.raw_x, created.raw_y, created.raw_w, created.raw_h)
                != (ax, ay, aw, ah)
            || (
                created.visible_x, created.visible_y,
                created.visible_w, created.visible_h,
            ) != expected
        {
            fail(&format!(
                "wm config created got id={} cause={} visible={} raw={},{},{},{} \
                 visible={},{},{},{}",
                created.window_id, created.cause, created.visible,
                created.raw_x, created.raw_y, created.raw_w, created.raw_h,
                created.visible_x, created.visible_y, created.visible_w,
                created.visible_h,
            ));
        }
        let wrong_serial = created.serial.rotate_left(1);
        if wm_config_ack(app_hw, app_win, wrong_serial) != gp::ST_DENIED {
            fail("wm config wrong serial accepted");
        }
        println!("WMCONFIGACKDENY ok serial={wrong_serial}");
        if wm_config_ack(app_hw, app_win, created.serial) != gp::ST_OK {
            fail("wm config created ack failed");
        }
        println!(
            "WMCONFIGCREATED ok id={app_win} serial={}",
            created.serial
        );
        if wm_config_poll(app_hw).is_some() {
            fail("wm config queue not empty after ack");
        }
        println!("WMCONFIGEMPTY ok id={app_win}");

        // 隐藏遮挡窗必须立刻恢复 raw bounds；再显示必须回到同一 visible
        // band。raw buffer 全程不重排、不重分配。
        let hide = gp::WmSetReq {
            mask: gp::WM_SET_F_VIS,
            window_id: sys_win,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            vis: gp::WM_VIS_HIDE,
            z: 0,
        };
        let m = enc_req(gp::encode_wm_set_req(&mut req, &hide));
        let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
        match gp::parse_status_rep(&rep[..n]) {
            Some(gp::ST_OK) => {}
            other => fail(&format!("wm avoid hide got {other:?}")),
        }
        let m = enc_req(Some(gp::encode_wm_table_get_req(&mut req)));
        let (n, _) = rpc(app_hw, &req[..m], &[], &mut rep, &mut rhs);
        let hidden = match gp::parse_wm_snapshot_rep(&rep[..n]) {
            Some(Ok(snapshot)) => {
                let entry = snapshot
                    .entries
                    .iter()
                    .take(snapshot.count)
                    .find(|e| e.id == app_win)
                    .unwrap_or_else(|| fail("wm avoid hidden entry"));
                (entry.x, entry.y, entry.w, entry.h)
            }
            _ => fail("wm avoid hidden table"),
        };
        if hidden != (ax, ay, aw, ah) {
            fail(&format!(
                "wm avoid hidden geometry got {}x{}+{}+{}",
                hidden.2, hidden.3, hidden.0, hidden.1
            ));
        }
        println!("WMAVOIDHIDE ok id={app_win} w={aw} h={ah}");

        let show = gp::WmSetReq { vis: gp::WM_VIS_SHOW, ..hide };
        let m = enc_req(gp::encode_wm_set_req(&mut req, &show));
        let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
        match gp::parse_status_rep(&rep[..n]) {
            Some(gp::ST_OK) => {}
            other => fail(&format!("wm avoid show got {other:?}")),
        }
        let m = enc_req(Some(gp::encode_wm_table_get_req(&mut req)));
        let (n, _) = rpc(app_hw, &req[..m], &[], &mut rep, &mut rhs);
        let reshown = match gp::parse_wm_snapshot_rep(&rep[..n]) {
            Some(Ok(snapshot)) => {
                let entry = snapshot
                    .entries
                    .iter()
                    .take(snapshot.count)
                    .find(|e| e.id == app_win)
                    .unwrap_or_else(|| fail("wm avoid reshown entry"));
                (entry.x, entry.y, entry.w, entry.h)
            }
            _ => fail("wm avoid reshown table"),
        };
        if reshown != expected {
            fail(&format!(
                "wm avoid reshown geometry got {}x{}+{}+{}",
                reshown.2, reshown.3, reshown.0, reshown.1
            ));
        }
        println!(
            "WMAVOIDSHOW ok id={app_win} y={occluder_h} h={}",
            ah.saturating_sub(occluder_h)
        );

        // R3 last-one-wins：HIDE 与 SHOW 两次表变之间不 POLL，最终只能取到
        // SHOW 后的最新 OCCLUSION event；被覆盖的 HIDE serial 不进队列。
        let coalesced = wm_config_poll(app_hw)
            .unwrap_or_else(|| fail("wm config coalesced absent"));
        if coalesced.window_id != app_win
            || coalesced.cause != gp::WM_CONFIG_CAUSE_OCCLUSION
            || !coalesced.visible
            || coalesced.serial == created.serial
            || (
                coalesced.visible_x, coalesced.visible_y,
                coalesced.visible_w, coalesced.visible_h,
            ) != expected
        {
            fail(&format!(
                "wm config coalesced got id={} cause={} visible={} serial={} \
                 visible={},{},{},{}",
                coalesced.window_id, coalesced.cause, coalesced.visible,
                coalesced.serial, coalesced.visible_x, coalesced.visible_y,
                coalesced.visible_w, coalesced.visible_h,
            ));
        }
        if wm_config_ack(app_hw, app_win, coalesced.serial) != gp::ST_OK {
            fail("wm config coalesced ack failed");
        }
        println!(
            "WMCONFIGCOALESCE ok id={app_win} serial={} old={}",
            coalesced.serial, created.serial
        );
        // 已 ACK serial 重发 = stale ACK，精确 ST_DENIED；客户端应重新 POLL。
        if wm_config_ack(app_hw, app_win, coalesced.serial) != gp::ST_DENIED {
            fail("wm config stale serial accepted");
        }
        println!("WMCONFIGSTALE ok serial={}", coalesced.serial);

        // SYSTEM occluder 自身也保持 config 清洁，避免测试窗积累未 ACK
        // pending；未来系统层窗口若开始提交，不会被旧 gate 卡住。
        while let Some(ev) = wm_config_poll(hw) {
            if wm_config_ack(hw, ev.window_id, ev.serial) != gp::ST_OK {
                fail("wm system config ack");
            }
        }

        // 0x67 常态化 R2 persistent occlusion：safe_* 是 host 推导值，
        // 以 raw surface-local 为基准，不由 APP 自报。
        let surface_req = gp::surface::SurfaceModeReq {
            surface_id: app_win,
            role: gp::surface::SURFACE_ROLE_APP,
            mode: gp::surface::SURFACE_MODE_RETAINED,
            input_classes: gp::surface::INPUT_CLASS_TOUCH,
            ime_mode: gp::surface::IME_MODE_DISABLED,
            a11y_mode: gp::surface::A11Y_MODE_TREE,
        };
        let m = enc_req(gp::surface::encode_wm_surface_mode_req(
            &mut req, &surface_req,
        ));
        let (n, _) = rpc(app_hw, &req[..m], &[], &mut rep, &mut rhs);
        let grant = match gp::surface::parse_wm_surface_mode_rep(&rep[..n]) {
            Some(Ok(info)) => info,
            other => fail(&format!("wm avoid grant got {other:?}")),
        };
        if (grant.safe_left, grant.safe_top, grant.safe_right, grant.safe_bottom)
            != (0, occluder_h, 0, 0)
        {
            fail(&format!(
                "wm avoid insets got l={} t={} r={} b={}",
                grant.safe_left, grant.safe_top, grant.safe_right, grant.safe_bottom
            ));
        }
        println!(
            "WMAVOIDGRANT ok id={app_win} l=0 t={occluder_h} r=0 b=0"
        );

        // R2 像素反 mock：raw buffer 每行 R=y、G=64、B=128。正确合约下
        // 屏幕 y=40 读 raw row 40；旧 eff-stride 合约会从 row 8 读出 R=8。
        let stride = usize::from(aw)
            .checked_mul(4)
            .unwrap_or_else(|| fail("wm avoid stride"));
        let bytes = stride
            .checked_mul(usize::from(ah))
            .unwrap_or_else(|| fail("wm avoid frame"));
        let map_len = (bytes + 0xFFF) & !0xFFF;
        let mut frame_vmo: Handle = 0;
        if berry_vmo_create(map_len as u64, 0, &mut frame_vmo) != 0
            || frame_vmo == 0
            || berry_vmo_commit(frame_vmo, 0, map_len as u64) != 0
        {
            fail("wm avoid vmo");
        }
        const AVOID_VA: usize = 0x5400_0000;
        let mut va = AVOID_VA;
        if berry_vmar_map(
            self_vmar_h(),
            0,
            frame_vmo,
            0,
            &mut va,
            map_len,
            RIGHT_READ | RIGHT_WRITE,
        ) != 0 || va != AVOID_VA
        {
            fail("wm avoid map");
        }
        let frame = unsafe {
            core::slice::from_raw_parts_mut(AVOID_VA as *mut u8, bytes)
        };
        for y in 0..usize::from(ah) {
            let px = ((y as u32 & 0xFF) << 16) | (64 << 8) | 128;
            let row = &mut frame[y * stride..(y + 1) * stride];
            for pixel in row.chunks_exact_mut(4) {
                pixel.copy_from_slice(&px.to_le_bytes());
            }
        }
        let mut alias: Handle = 0;
        if berry_handle_duplicate(frame_vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0
            || alias == 0
        {
            fail("wm avoid dup");
        }
        let submit = gp::PresentReq::Submit {
            surface_id: u32::from(app_win),
            buffer_id: 0xA0CC,
            flags: gp::SUBMIT_F_DAMAGE_FULL,
            rate_hint: 0,
            target_present_ns: 0,
            rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
            rect_count: 0,
        };
        let m = enc_req(gp::encode_present_req(&mut req, &submit));
        let (n, _) = rpc(app_hw, &req[..m], &[alias], &mut rep, &mut rhs);
        let token = match gp::parse_submit_ack(&rep[..n]) {
            Some(t) => t,
            None => fail(&format!("wm avoid submit bad st={}", rep[0])),
        };
        let m = enc_req(gp::encode_present_req(
            &mut req,
            &gp::PresentReq::FrameCallback { frame_token: token },
        ));
        let (n, _) = rpc(app_hw, &req[..m], &[], &mut rep, &mut rhs);
        match gp::parse_frame_done_rep(&rep[..n]) {
            Some(done) if done.status == gp::FRAME_PRESENTED => {}
            other => fail(&format!("wm avoid frame got {other:?}")),
        }
        let m = enc_req(gp::encode_present_req(
            &mut req,
            &gp::PresentReq::ReleaseWait,
        ));
        let (n, nh) = rpc(app_hw, &req[..m], &[], &mut rep, &mut rhs);
        let release = match gp::parse_release_rep(&rep[..n]) {
            Some(r) => r,
            None => fail(&format!("wm avoid release bad st={}", rep[0])),
        };
        if usize::from(release.count) != nh {
            fail("wm avoid release handles");
        }
        for handle in rhs.iter().take(nh) {
            close_h(*handle);
        }
        println!(
            "WMAVOIDFRAME ok id={app_win} x={ax} y={occluder_h} w={aw} h={}",
            ah.saturating_sub(occluder_h)
        );

        // R2 负例：damage 完全落在持久遮挡带内时，整批有效 damage 为空，
        // 必须显式 ST_BAD，不得变成零 damage 提交或偷写 overlay 区。
        let mut occluded_alias: Handle = 0;
        if berry_handle_duplicate(frame_vmo, RIGHT_READ | RIGHT_TRANSFER, &mut occluded_alias) != 0
            || occluded_alias == 0
        {
            fail("wm occluded damage dup");
        }
        let mut rects = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
        rects[0] = gp::Rect {
            x: 0,
            y: 0,
            w: u32::from(aw),
            h: u32::from(occluder_h),
        };
        let bad_submit = gp::PresentReq::Submit {
            surface_id: u32::from(app_win),
            buffer_id: 0xA0CD,
            flags: 0,
            rate_hint: 0,
            target_present_ns: 0,
            rects,
            rect_count: 1,
        };
        let m = enc_req(gp::encode_present_req(&mut req, &bad_submit));
        let (n, _) = rpc(
            app_hw,
            &req[..m],
            &[occluded_alias],
            &mut rep,
            &mut rhs,
        );
        match gp::parse_status_rep(&rep[..n]) {
            Some(gp::ST_BAD) => {}
            other => fail(&format!("wm occluded damage got {other:?}")),
        }
        println!("WMOCCDAMAGEBAD ok h={occluder_h}");
        if berry_thread_sleep(now() + 2_000_000_000) != 0 {
            fail("wm avoid sleep");
        }
        if berry_vmar_unmap(self_vmar_h(), AVOID_VA, map_len) != 0
            || berry_handle_close(frame_vmo) != 0
        {
            fail("wm avoid unmap");
        }
        println!("WMRTWIN ok id={app_win} x={ax} y={ay} w={aw} h={ah}");
    } else {
        close_h(app_process);
    }
    // broker 的连接使命完成；session 均已换到专通道（常态仅 SYSTEM）。
    close_h(broker);

    // 布局装载（冻结相 fs_h()==0 → 默认布局直返零 RPC；三约束自愈面
    // 见 layout_io 头注）。
    let layout = layout_io::load(icons::registry(), smoke, fs_ready);
    // cbox 刀 2：init 落账（第三窗——排液一次 + CBOX.LOG 读-改-写；catch
    // launcher 上一会话崩溃与 battery 后新入账 fault，与 svcmgr 两窗
    // (boot,seq) 去重幂等互恰；失败非致命照进桌面，见 cbox_io 头注）。
    // 冒烟相（gui8 fixture）冻结落账：ktest 每 boot 必记一条 rogue kill
    //（on_fault 钩，故意杀非真崩溃）——boot B 排液必有新账必重写，撞
    // gui8「跨 boot 读路径零重写」盘哈希不变量（2026-11-16 实证：CBOX.LOG
    // 2 条 = boot 1..2 各一条 user_rogue_ptr）。fixture 世界冻结语义与
    // layout_io 冒烟支路同族；真机/常态 boot 落账面不变。
    if !smoke {
        cbox_io::init_flush();
    }
    // Ui<512> arena ≈ 75KB+（Node 144B×512 + 走查/序/快照缓冲侧表）——
    // guest 栈 stack_pages=64 = 256KB（R5 仪器定谳，svcmgr 装配注释为
    // 单源——8 页下 main+0x1c 序言即穿保护页）栈置仍不稳：Box::leak
    // 堆置（桌面进程终身持有——常驻面无回收语义，leak 即分配）。
    icons::prime_wallpaper(info.disp_w as u32, info.disp_h as u32);
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    ui.set_icon_source(icons::icon_source());
    // 桌面建树（几何全 GETINFO 动态——ui.viewport()；Err = 容量/文本域
    // 违例 → fail 收口：eprintln + park，监督重启归 svcmgr 域）。
    let desktop = match desktop::build(ui, &layout, icons::registry()) {
        Ok(d) => d,
        Err(e) => fail(&format!("desktop build err={e:?}")),
    };
    if let Err(e) = desktop::install_press_feedback(ui, &desktop) {
        fail(&format!("desktop press feedback err={e:?}"));
    }
    // 帧循环（W1 静默 park；LAUNCH ok/PAGE ok = W2 产物）。smoke = 段④
    // 两标记的唯一门（CLICK ok / launcher: click ready）；正常 boot 传
    // false = 点击照常 spawn、零新标记。
    frameloop::run(
        hw,
        session_input,
        (app_input != 0).then_some(app_input),
        ui,
        desktop,
        smoke,
        product_mode_from_argv(),
        system_reserves,
    );
}
