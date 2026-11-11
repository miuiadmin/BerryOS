//! gfxdemo —— GUI-5 消费侧（§3 #12）：wm 域首个真实消费者 + present/hdi
//! 改经 wmd 中继（同 op 换端，ADR #61 呈现/策略分离）。
//!
//! **B 形态三件事的接线半边**（ADR #54「协议先定形、无消费者不接线」）：
//! berry-gfx pass 形态 API soft 渲一帧（确定性 DisplayList：Clear 底色 +
//! fill rect + glyph 串；Bgrx 与 ramfb 同源免转换）→ 匿名 VMO commit+map
//! → present 域 SUBMIT（K=2 damage，随行 VMO READ 别名）→ guifbd
//! blit-at-SUBMIT 上屏 → FRAME_CALLBACK 回执 → RELEASE 归还即关 →
//! VOTE/POWER_MODE 1B 回执 → hdi 域软 vsync mini 序（REG_VBLANK /
//! VBLANK_WAIT / PREPARE / COMMIT），全断言过才打 `GFX ok`（smoke-image
//! EFI 相 ECO 标记）。服务端真值 `GFXF ok n=` 由 guifbd 打——两标记分立
//! = 故障定位面（GFXF 缺 = 服务端 blit/抽样/记账炸；GFXF 在而 GFX 缺 =
//! 客户端 RPC 链炸）。
//!
//! **端点拓扑**（GUI-5 §2/§7 Q12）：svcmgr 在 wmd 预拉段 channel_pair()
//! 接线，wm 客户端端随本程序 env extras 直递（5/8：console/stdin/
//! self_proc/self_vmar/wm_cli——libc `wm_cli_h()` 读 GIFT_H[0]）。三域
//! **同端混流**：wm 0x60..=0x65 wmd 本地权威处理；present 1..=6 与 hdi
//! 0x40..=0x43 经 wmd 分发表透传 guifbd（中继不改 op 编码——本程序断言
//! 面零变化，仅 rpc 首参换端；guifbd 零改动）。rootfs 相 wmd 随 fb 探测
//! 门不被拉起 → 本程序不被拉起；bsh 手跑 `wm_cli_h()==0` → 一行降级
//! 诊断 + exit(0)（netd 双门先例同款——非错误面，`gfx: wm absent` 永不
//! 进 MARKERS）。
//!
//! **WM ok 语义**（GUI-5 新标记，恰一次——结构性：svcmgr 单次预拉 +
//! 阻塞 reap + 单 println）：wm 四步全断言过（GETINFO 能力/版本 →
//! CREATE 全屏窗 id≠0 → SET(GEOM) 1B OK → TABLE_GET 快照对拍 id 回显
//! ∧ 有效几何 == 申请——v1 无系统层重叠恒等）才打；`GFX ok` 仍收口
//! 整链恰一次（wm + present + hdi + 渲染）。
//!
//! **D3 单 wm 客户端 = 时间轴单活**（GUI-8 段② 头注随动，0c 钉）：段②
//! 起 berry-launcher 常驻 = 第二 wm 客户端（wm 客户端端 dup 柄，
//! `wm_cli_h()` 同槽），但 svcmgr 装配序 = 本程序先活后死（阻塞 reap
//! 收毕）→ launcher 后活常驻——本程序整个生命周期内 wm 表恒「唯一客户
//! 端唯一窗」，TABLE_GET 断言零连带改；状态栏/壁纸 = launcher 窗口内容
//! 层（GUI-8 裁决四-e）非 wmd 系统窗，「有效几何 == 申请」恒等面不破
//! ——真系统窗/避让裁剪刀再随改。
//!
//! **RPC 纪律**（§9 A6-a 收侧 10s 帽，fsclient `fs_rpc` 同形）：严格串行
//! 一请求一应答；发送 -EBUSY 阻塞重试（WRITABLE 10s 重臂）；收侧空队 →
//! READABLE 10s 帽——超时/错码 eprintln + exit(1)（svcmgr 阻塞 reap 收码
//! 打 eco 行，FAIL loud 非挂死）。**send 即 take**（§9 A6-b）：随行 handle
//! 在 send 成功即迁出本表——SUBMIT/PREPARE 各需 fresh dup，RELEASE 应答
//! 随行归还的别名 handle 须即关。

use berry_gfx::cmd::{CmdError, Color, CommandEncoder, GlyphPos, GlyphRun, Rect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontAtlas8x16;
use berry_gfx::pass::PassDescriptor;
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::io::{self_proc_h, self_vmar_h, wm_cli_h};
use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_clock_read, berry_event_create,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_one, berry_thread_sleep,
    berry_vmar_map,
    berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    CLOCK_MONOTONIC, RIGHT_DUPLICATE, RIGHT_READ, RIGHT_WRITE, SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use std::process::exit;

// 链接种子（guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc 的 crt0
// （含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（fsclient `TIMEOUT_NS` 同值 10s——A6-a 形态：死法
/// = FAIL loud 非挂死，smoke 侧标记缺失兜底）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 帧缓冲映射 VA：berry-mmap VA 窗 [0x5000_0000, 0x6000_0000) 内固定位
/// （窗基 +16MiB）。本程序不走 libc mmap（raw 三连直取 VMO handle 面——
/// SUBMIT 随行须是本 VMO 的 dup 别名，mmap 的 region 表藏 handle 取不出；
/// region 表不知情不影响）——头留段 = 与假想 first-fit 分配零交叠的防御
/// 位；页对齐（vmar_map 口径）。
const FRAME_VA: usize = 0x5100_0000;

/// FAIL loud 收口：诊断行（stderr = console 发端）+ exit(1)——svcmgr 阻塞
/// reap 收非零码打 eco 行；`GFX ok` 缺失 = smoke-image FAIL 兜底。
fn fail(what: &str) -> ! {
    eprintln!("gfxdemo: {what}");
    exit(1)
}

/// now = clock_read；失败按 0 → deadline 即刻到期 = fail-fast（fsclient
/// `now` 同口径——时钟不可用当死路，不静默自旋）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 即关（RELEASE 归还别名 / PREPARE 空置 fence——关失败 = 表泄漏，FAIL
/// loud）。
fn close_h(h: Handle) {
    if berry_handle_close(h) != 0 {
        fail("handle close");
    }
}

/// 编码收口（None = 本程序侧组帧 bug——定容 256B 缓冲恒够，结构性不可达）。
fn enc_req(m: Option<usize>) -> usize {
    match m {
        Some(n) => n,
        None => fail("req encode"),
    }
}

/// DisplayList 记录面收口（结构性不可达——界内 rect/ASCII glyph run 无拒
/// 面；真触发 = FAIL loud 而非静默丢命令）。
fn record(r: Result<(), CmdError>, what: &str) {
    if r.is_err() {
        fail(what);
    }
}

/// 严格串行 RPC 单趟（fsclient `fs_rpc` 同形；本程序单发单收、exit 即终，
/// 无会话毒化面）。发送侧：-EBUSY → 阻塞等 WRITABLE（10s 帽重臂，雷 2
/// 纪律不自旋；send 原子性——EBUSY/错码时随行 handle 零迁移）。收侧：空
/// 队 -EAGAIN → 阻塞等 READABLE（`RPC_CAP_NS` 帽）。返回 `(应答长, 随行
/// handle 数)`；`rhs` 出参 = 重挂本表的应答随行 handle（RELEASE 归还别名
/// / PREPARE fence）。
fn rpc(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (usize, usize) {
    let (hptr, hn) = if attach.is_empty() {
        (core::ptr::null(), 0)
    } else {
        (attach.as_ptr(), attach.len())
    };
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(ch, req.as_ptr(), req.len(), hptr, hn);
        if s == 0 {
            break;
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
    loop {
        // 每轮重置容量（in-out 出参——漏重置 = 恒 -EINVAL 死循环，A1-b）。
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
            fail("rpc reply timeout 10s");
        }
    }
}

/// 1B 状态应答单趟（VOTE/POWER_MODE/REG_VBLANK/COMMIT 成功形——烘焙线：
/// 一切非 OK 应答恒 1B 不带数据）。
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

/// Phase 1 WM_CONNECT：broker 只换 session；返回本进程保留的 session 端。
/// 随行迁移 [对端 session endpoint, Process]，wmd 用后者做死亡清理。
/// broker 生命周期由调用方管理（本测试进程要开两个 session 做跨 owner 负例）。
fn wm_connect_process(broker: Handle, process: Handle) -> (Handle, Handle) {
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
    let attach = [remote, input_remote, process];
    let m = enc_req(gp::encode_wm_connect_req(&mut req, gp::WM_CLIENT_APP));
    let (n, _) = rpc(broker, &req[..m], &attach, &mut rep, &mut rhs);
    let _ = gp::parse_wm_connect_rep(&rep[..n]).unwrap_or_else(|| fail("wm connect rep bad"));
    (local, input_local)
}

/// P1 反伪造负例：普通 app 自己 channel_pair、预载历史公开常量
/// "BSESCAPE"，再把它作为 WM_CONNECT 第 4 handle 附上。per-boot seed 接线
/// 后必须得到精确 1B ST_DENIED；若 wmd 接受，说明授权退化为自报。
fn wm_connect_forged_surface_denied(broker: Handle, process: Handle) {
    let mut local: Handle = 0;
    let mut remote: Handle = 0;
    let mut input_local: Handle = 0;
    let mut input_remote: Handle = 0;
    let mut token_sender: Handle = 0;
    let mut token_receiver: Handle = 0;
    let mut forge_process: Handle = 0;
    if process == 0
        || berry_channel_create(0, &mut local, &mut remote) != 0
        || berry_channel_create(0, &mut input_local, &mut input_remote) != 0
        || berry_channel_create(0, &mut token_sender, &mut token_receiver) != 0
        || berry_handle_duplicate(process, RIGHT_DUPLICATE, &mut forge_process) != 0
    {
        fail("forged surface channels");
    }
    let public = [0x42, 0x53, 0x45, 0x53, 0x43, 0x41, 0x50, 0x45];
    if berry_channel_send(
        token_sender,
        public.as_ptr(),
        public.len(),
        core::ptr::null(),
        0,
    ) != 0
        || berry_handle_close(token_sender) != 0
    {
        fail("forged surface preload");
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let attach = [remote, input_remote, forge_process, token_receiver];
    let m = enc_req(gp::encode_wm_connect_req(&mut req, gp::WM_CLIENT_APP));
    let (n, _) = rpc(broker, &req[..m], &attach, &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(gp::ST_DENIED) => {}
        other => fail(&format!("forged surface got {other:?}")),
    }
    let _ = berry_handle_close(local);
    let _ = berry_handle_close(input_local);
}

/// R1：APP session 不得在 0x67 自报 System role。
fn wm_app_system_role_denied(ch: Handle, window: u16) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let request = gp::surface::SurfaceModeReq {
        surface_id: window,
        role: gp::surface::SURFACE_ROLE_SYSTEM,
        mode: gp::surface::SURFACE_MODE_RETAINED,
        input_classes: gp::surface::INPUT_CLASS_TOUCH,
        ime_mode: gp::surface::IME_MODE_DISABLED,
        a11y_mode: gp::surface::A11Y_MODE_TREE,
    };
    let m = enc_req(gp::surface::encode_wm_surface_mode_req(&mut req, &request));
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    match gp::surface::parse_wm_surface_mode_rep(&rep[..n]) {
        Some(Err(gp::ST_DENIED)) => {}
        other => fail(&format!("wm role masquerade got {other:?}")),
    }
    println!("WMROLEMASQDENY ok");
}

/// R1：SYSTEM connect 带“存在但错误”的第 4 seed channel，也必须拒绝。
fn wm_connect_wrong_system_seed_denied(broker: Handle, process: Handle) {
    let mut local: Handle = 0;
    let mut remote: Handle = 0;
    let mut input_local: Handle = 0;
    let mut input_remote: Handle = 0;
    let mut token_sender: Handle = 0;
    let mut token_receiver: Handle = 0;
    let mut spoof_process: Handle = 0;
    if berry_channel_create(0, &mut local, &mut remote) != 0
        || berry_channel_create(0, &mut input_local, &mut input_remote) != 0
        || berry_channel_create(0, &mut token_sender, &mut token_receiver) != 0
        || berry_handle_duplicate(process, RIGHT_DUPLICATE, &mut spoof_process) != 0
    {
        fail("wm system seed spoof channels");
    }
    let wrong = [0xA5u8; 8];
    if berry_channel_send(
        token_sender,
        wrong.as_ptr(),
        wrong.len(),
        core::ptr::null(),
        0,
    ) != 0
        || berry_handle_close(token_sender) != 0
    {
        fail("wm system seed spoof preload");
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = enc_req(gp::encode_wm_connect_req(
        &mut req,
        gp::WM_CLIENT_SYSTEM,
    ));
    let attach = [remote, input_remote, spoof_process, token_receiver];
    let (n, _) = rpc(broker, &req[..m], &attach, &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(gp::ST_DENIED) => {}
        other => fail(&format!("wm system seed spoof got {other:?}")),
    }
    let _ = berry_handle_close(local);
    let _ = berry_handle_close(input_local);
    println!("WMSYSSEEDDENY ok");
}

/// R1：0x69 控制帧只属于 svcmgr 启动期；client 晚到/重复发送必须拒绝。
fn wm_system_seed_control_denied(broker: Handle) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let n = match gp::surface::encode_wm_system_seed_req(
        &mut req,
        &[0x5Au8; 8],
    ) {
        Some(n) => n,
        None => fail("wm system control encode"),
    };
    let (n, _) = rpc(broker, &req[..n], &[], &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(gp::ST_DENIED) => {}
        other => fail(&format!("wm system control got {other:?}")),
    }
    println!("WMSYSCTRLDENY ok");
}

/// P3：两个半屏窗都声明 Retained + Keyboard。Keyboard 不需要 P1 escape
/// seed；焦点由 wmd pointer DOWN 权威更新。
fn configure_retained_keyboard(ch: Handle, window: u16) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let request = gp::surface::SurfaceModeReq {
        surface_id: window,
        role: gp::surface::SURFACE_ROLE_APP,
        mode: gp::surface::SURFACE_MODE_RETAINED,
        input_classes: gp::surface::INPUT_CLASS_TOUCH | gp::surface::INPUT_CLASS_KEYBOARD,
        ime_mode: gp::surface::IME_MODE_DISABLED,
        a11y_mode: gp::surface::A11Y_MODE_TREE,
    };
    let m = enc_req(gp::surface::encode_wm_surface_mode_req(&mut req, &request));
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    let info = match gp::surface::parse_wm_surface_mode_rep(&rep[..n]) {
        Some(Ok(info)) => info,
        other => fail(&format!("retained keyboard grant got {other:?}")),
    };
    if info.mode != gp::surface::SURFACE_MODE_RETAINED
        || info.input_classes & gp::surface::INPUT_CLASS_KEYBOARD == 0
        || info.contract_flags & gp::surface::SURFACE_F_ESCAPE != 0
    {
        fail("retained keyboard grant mismatch");
    }
}

/// 非阻塞 pointer INPUT_POLL；空批返回 0。
fn poll_pointer(ch: Handle, out: &mut [gp::PointerEvent; gp::INPUT_EVENTS_MAX]) -> usize {
    let mut req = [0u8; 4];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_input_poll_req(&mut req, gp::INPUT_EVENTS_MAX as u8);
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_input_events_rep(&rep[..n]) {
        Some(Ok(events)) => {
            let count = events.count();
            for i in 0..count {
                out[i] = match events.event(i) {
                    Some(ev) => ev,
                    None => fail("pointer poll event"),
                };
            }
            count
        }
        Some(Err(_)) => 0,
        None => fail("pointer poll rep"),
    }
}

/// 非阻塞 keyboard INPUT_KEY_POLL；空批返回 0。
fn poll_keys(ch: Handle, out: &mut [gp::KeyEvent; gp::INPUT_KEY_EVENTS_MAX]) -> usize {
    let mut req = [0u8; 4];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_input_key_poll_req(&mut req, gp::INPUT_KEY_EVENTS_MAX as u8);
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_input_key_events_rep(&rep[..n]) {
        Some(Ok(events)) => {
            let count = events.count();
            for i in 0..count {
                out[i] = match events.event(i) {
                    Some(ev) => ev,
                    None => fail("key poll event"),
                };
            }
            count
        }
        Some(Err(_)) => 0,
        None => fail("key poll rep"),
    }
}

/// 等待目标通道出现 pointer DOWN；对侧若出现 DOWN 则 fail。
fn wait_pointer_down(target: Handle, other: Handle, side: &str) -> bool {
    let deadline = now() + 30_000_000_000;
    let mut pe = [gp::PointerEvent {
        phase: 0, device_id: 0, buttons: 0, flags: 0, pointer_id: 0,
        timestamp_ns: 0, x: 0, y: 0, pressure: 0, contact_major: 0,
        contact_minor: 0, history: [gp::Sample { timestamp_ns: 0, x: 0, y: 0 };
            gp::INPUT_SAMPLES_MAX - 1], history_len: 0,
    }; gp::INPUT_EVENTS_MAX];
    loop {
        let n = poll_pointer(other, &mut pe);
        for ev in pe.iter().take(n) {
            if ev.phase == gp::PHASE_DOWN {
                fail(&format!("focus pointer wrong side={side}"));
            }
        }
        let n = poll_pointer(target, &mut pe);
        for ev in pe.iter().take(n) {
            if ev.phase == gp::PHASE_DOWN {
                return true;
            }
        }
        if now() >= deadline {
            // Boot B/C 无 QMP 交互：不伪造焦点证明，显式 skip 后继续
            // 既有 gfxdemo 流；Boot A 由 GUI8 在 ready 后立即注入。
            return false;
        }
        if berry_thread_sleep(now() + 1_000_000) != 0 {
            fail("focus pointer sleep");
        }
    }
}

/// 等待目标通道 KEY_A down/up；对侧出现任意 key 则 fail。
fn wait_key_pair(target: Handle, other: Handle, side: &str) {
    let deadline = now() + 30_000_000_000;
    let mut ke = [gp::KeyEvent {
        timestamp_ns: 0, code: 0, action: 0, modifiers: 0, device_id: 0,
    }; gp::INPUT_KEY_EVENTS_MAX];
    let mut downs = 0u32;
    let mut ups = 0u32;
    loop {
        let n = poll_keys(other, &mut ke);
        if n != 0 {
            fail(&format!("focus key wrong side={side}"));
        }
        let n = poll_keys(target, &mut ke);
        for ev in ke.iter().take(n) {
            if ev.code != 30 {
                fail(&format!("focus key code={}", ev.code));
            }
            match ev.action {
                gp::KEY_ACTION_DOWN => downs += 1,
                gp::KEY_ACTION_UP => ups += 1,
                _ => {}
            }
        }
        if downs >= 1 && ups >= 1 {
            return;
        }
        if now() >= deadline {
            fail(&format!("focus key timeout side={side}"));
        }
        if berry_thread_sleep(now() + 1_000_000) != 0 {
            fail("focus key sleep");
        }
    }
}

/// TABLE_GET 焦点快照：返回版本与左右窗 focused 位。
fn table_focus_snapshot(
    ch: Handle,
    left: u16,
    right: u16,
) -> (u32, bool, bool) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_table_get_req(&mut req);
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    let snap = match gp::parse_wm_snapshot_rep(&rep[..n]) {
        Some(Ok(s)) => s,
        other => fail(&format!("focus table snap got {other:?}")),
    };
    let entry = snap
        .entries
        .iter()
        .take(snap.count)
        .find(|e| e.id == right)
        .unwrap_or_else(|| fail("focus table missing right"));
    let left_focused = snap
        .entries
        .iter()
        .take(snap.count)
        .find(|e| e.id == left)
        .map(|e| e.flags & gp::WM_WIN_F_FOCUSED != 0)
        .unwrap_or(false);
    let right_focused = entry.flags & gp::WM_WIN_F_FOCUSED != 0;
    (snap.version, left_focused, right_focused)
}

/// TABLE_GET 单窗 focused 位：P4 第三锁定窗专用。
fn table_focus_one(ch: Handle, window: u16) -> (u32, bool) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_table_get_req(&mut req);
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    let snap = match gp::parse_wm_snapshot_rep(&rep[..n]) {
        Some(Ok(s)) => s,
        other => fail(&format!("p4 one snapshot got {other:?}")),
    };
    let focused = snap
        .entries
        .iter()
        .take(snap.count)
        .find(|e| e.id == window)
        .map(|e| e.flags & gp::WM_WIN_F_FOCUSED != 0)
        .unwrap_or(false);
    (snap.version, focused)
}

/// 排空一个 input channel 的残留 pointer 批，避免旧 DOWN 让后续
/// wait_pointer_down 立即假命中。
fn pointer_silent(ch: Handle) {
    let mut pe = [gp::PointerEvent {
        phase: 0, device_id: 0, buttons: 0, flags: 0, pointer_id: 0,
        timestamp_ns: 0, x: 0, y: 0, pressure: 0, contact_major: 0,
        contact_minor: 0, history: [gp::Sample { timestamp_ns: 0, x: 0, y: 0 };
            gp::INPUT_SAMPLES_MAX - 1], history_len: 0,
    }; gp::INPUT_EVENTS_MAX];
    if poll_pointer(ch, &mut pe) != 0 {
        fail("p4 pointer silence broken");
    }
}

fn drain_pointer(ch: Handle) {
    let mut pe = [gp::PointerEvent {
        phase: 0, device_id: 0, buttons: 0, flags: 0, pointer_id: 0,
        timestamp_ns: 0, x: 0, y: 0, pressure: 0, contact_major: 0,
        contact_minor: 0, history: [gp::Sample { timestamp_ns: 0, x: 0, y: 0 };
            gp::INPUT_SAMPLES_MAX - 1], history_len: 0,
    }; gp::INPUT_EVENTS_MAX];
    for _ in 0..16 {
        if poll_pointer(ch, &mut pe) == 0 {
            return;
        }
    }
    fail("focus pointer drain overflow");
}

/// 通用 WM_SET：P4 矩阵驱动 VIS/GEOM/Z 状态迁移。
fn wm_set(ch: Handle, req: gp::WmSetReq) {
    let mut wire = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = enc_req(gp::encode_wm_set_req(&mut wire, &req));
    expect_ok(ch, &wire[..m], &[], &mut rep, &mut rhs, "p4 wm set");
}

fn set_visibility(ch: Handle, window: u16, show: bool, geom: (u16,u16,u16,u16)) {
    wm_set(ch, gp::WmSetReq {
        mask: gp::WM_SET_F_VIS,
        window_id: window,
        x: geom.0, y: geom.1, w: geom.2, h: geom.3,
        vis: if show { gp::WM_VIS_SHOW } else { gp::WM_VIS_HIDE },
        z: 0,
    });
}

fn set_geometry_z(
    ch: Handle, window: u16, geom: (u16,u16,u16,u16), z: u8,
) {
    wm_set(ch, gp::WmSetReq {
        mask: gp::WM_SET_F_GEOM | gp::WM_SET_F_Z,
        window_id: window,
        x: geom.0, y: geom.1, w: geom.2, h: geom.3,
        vis: gp::WM_VIS_SHOW,
        z,
    });
}

fn set_z(ch: Handle, window: u16, z: u8, geom: (u16,u16,u16,u16)) {
    wm_set(ch, gp::WmSetReq {
        mask: gp::WM_SET_F_Z,
        window_id: window,
        x: geom.0, y: geom.1, w: geom.2, h: geom.3,
        vis: gp::WM_VIS_SHOW,
        z,
    });
}

fn destroy_window(ch: Handle, window: u16) {
    let mut wire = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = enc_req(Some(gp::encode_wm_destroy_req(&mut wire, window)));
    expect_ok(ch, &wire[..m], &[], &mut rep, &mut rhs, "p4 wm destroy");
}

/// R3：POLL 一条事件；None 仅代表 ST_EAGAIN。
fn poll_config_event(ch: Handle) -> Option<gp::WmConfigEvent> {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_config_poll_req(&mut req);
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_wm_config_event_rep(&rep[..n]) {
        Some(Ok(ev)) => Some(ev),
        Some(Err(gp::ST_EAGAIN)) => None,
        other => fail(&format!("config poll got {other:?}")),
    }
}

/// R3：ACK 原始状态面。
fn ack_config_event(ch: Handle, ev: &gp::WmConfigEvent) -> u8 {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let ack = gp::WmConfigAckReq {
        window_id: ev.window_id,
        serial: ev.serial,
    };
    let m = match gp::encode_wm_config_ack_req(&mut req, &ack) {
        Some(m) => m,
        None => fail("config ack encode"),
    };
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(st) => st,
        None => fail(&format!("config ack rep bad st={}", rep[0])),
    }
}

/// R3：排空并 ACK config events。gfxdemo 的 0x67/SUBMIT 均要求当前
/// config 已同步；这也证明 raw 客户端不依赖 Room 才能走新合约。
fn drain_config(ch: Handle) {
    while let Some(ev) = poll_config_event(ch) {
        if ack_config_event(ch, &ev) != gp::ST_OK {
            fail("config drain ack");
        }
    }
}

/// R3：必须为空的负例（no-op SET / focus-only bump 不产事件）。
fn assert_config_empty(ch: Handle, what: &str) {
    if poll_config_event(ch).is_some() {
        fail(&format!("config phantom event {what}"));
    }
}

fn create_lock_window(ch: Handle) -> (u16, u16, u16, u16, u16) {
    let req = gp::WmCreateReq {
        layer: gp::WM_LAYER_APP,
        flags: 0,
        parent: 0,
        group: 0,
        x: 24,
        y: 96,
        w: 160,
        h: 96,
    };
    let mut wire = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = enc_req(gp::encode_wm_create_req(&mut wire, &req));
    let (n, _) = rpc(ch, &wire[..m], &[], &mut rep, &mut rhs);
    let id = match gp::parse_wm_create_ack(&rep[..n]) {
        Some(id) if id != 0 => id,
        _ => fail(&format!("p4 lock create st={}", rep[0])),
    };
    drain_config(ch);
    (id, req.x, req.y, req.w, req.h)
}

/// 等待锁定窗被销毁后的 synthesized CANCEL。
fn wait_pointer_cancel(ch: Handle) -> gp::PointerEvent {
    let deadline = now() + 30_000_000_000;
    let mut pe = [gp::PointerEvent {
        phase: 0, device_id: 0, buttons: 0, flags: 0, pointer_id: 0,
        timestamp_ns: 0, x: 0, y: 0, pressure: 0, contact_major: 0,
        contact_minor: 0, history: [gp::Sample { timestamp_ns: 0, x: 0, y: 0 };
            gp::INPUT_SAMPLES_MAX - 1], history_len: 0,
    }; gp::INPUT_EVENTS_MAX];
    loop {
        let n = poll_pointer(ch, &mut pe);
        for ev in pe.iter().take(n) {
            if ev.phase == gp::PHASE_CANCEL
                && ev.flags & gp::PE_FL_SYNTHESIZED != 0
            {
                return *ev;
            }
        }
        if now() >= deadline {
            fail("p4 synthesized cancel timeout");
        }
        if berry_thread_sleep(now() + 1_000_000) != 0 {
            fail("p4 cancel sleep");
        }
    }
}

/// P3 双窗焦点证明：QMP 先点左窗+KEY_A，再点右窗+KEY_A。本函数只在
/// 每侧收到 pointer DOWN 和同侧 key pair 后打印 payload marker。
fn focus_keyboard_proof(
    hw: Handle, rogue: Handle, input1: Handle, input2: Handle,
    left: u16, right: u16, left_w: u16, right_x: u16, right_w: u16, h: u16,
) -> bool {
    configure_retained_keyboard(hw, left);
    configure_retained_keyboard(rogue, right);
    println!(
        "WMFOCUSREADY ok left={left} right={right} left_rect=0,0,{left_w},{h} right_rect={right_x},0,{right_w},{h}"
    );
    if !wait_pointer_down(input1, input2, "left") {
        println!("WMFOCUSSKIP ok reason=no-pointer");
        return false;
    }
    wait_key_pair(input1, input2, "left");
    println!("WMKEYFOCUS ok side=left window={left} pointer=1 keys=2");
    wait_pointer_down(input2, input1, "right");
    wait_key_pair(input2, input1, "right");
    println!("WMKEYFOCUS ok side=right window={right} pointer=1 keys=2");
    println!("WMFOCUSKEY ok left={left} right={right} proofs=2");
    // 同窗重复点击：wmd 不得再次 bump / 打 WMFOCUS。这里等待真实第二次
    // pointer DOWN，并用 TABLE_GET 前后版本与 focused bit 钉板 no-op。
    let (before, _, focused_before) = table_focus_snapshot(hw, left, right);
    if !focused_before {
        fail("focus snapshot right not focused");
    }
    if !wait_pointer_down(input2, input1, "right-noop") {
        fail("focus no-op pointer absent");
    }
    let (after, _, focused_after) = table_focus_snapshot(hw, left, right);
    if before != after || !focused_after {
        fail(&format!("focus no-op snapshot {before}->{after} focused={focused_after}"));
    }
    println!("WMSNAPFOCUS ok v={after} right=1");
    println!("WMFOCUSNOOP ok window={right} v={before}");
    true
}

/// P4 窗口策略矩阵：desktop miss 焦点保持、HIDE/SHOW refocus、
/// z-order 命中焦点、锁定窗销毁后的 synthesized CANCEL。所有状态迁移仍
/// 走 WM_SET/DESTROY 与 wmd 权威表，不在 guest 侧造第二真相。
#[allow(clippy::too_many_arguments)]
fn window_policy_proof(
    hw: Handle,
    rogue: Handle,
    input1: Handle,
    input2: Handle,
    left: u16,
    right: u16,
    left_w: u16,
    right_x: u16,
    right_w: u16,
    h: u16,
) {
    let right_geom = (right_x, 0, right_w, h);

    // ① HIDE：焦点右窗隐藏后必须归还给唯一可见左窗。
    let (before, _, right_before) = table_focus_snapshot(hw, left, right);
    if !right_before {
        fail("p4 hide precondition right not focused");
    }
    set_visibility(rogue, right, false, right_geom);
    let (hidden, left_hidden, right_hidden) = table_focus_snapshot(hw, left, right);
    if hidden <= before || !left_hidden || right_hidden {
        fail(&format!("p4 hide snapshot v={hidden} l={left_hidden} r={right_hidden}"));
    }
    println!("WMHIDEFOCUS ok hidden={right} focused={left} v={hidden}");
    let miss_x = right_x + right_w / 2;
    let miss_y = h / 2;
    drain_pointer(input1);
    drain_pointer(input2);
    println!("WMMISSREADY ok x={miss_x} y={miss_y}");

    // ② desktop miss：右半屏已无窗，点击不 bump、不清焦；KEY_A 仍进左窗。
    wait_key_pair(input1, input2, "miss");
    pointer_silent(input1);
    pointer_silent(input2);
    let (miss, left_miss, right_miss) = table_focus_snapshot(hw, left, right);
    if miss != hidden || !left_miss || right_miss {
        fail(&format!("p4 miss snapshot v={miss} l={left_miss} r={right_miss}"));
    }
    println!("WMMISSFOCUS ok focused={left} pointer=0 keys=2 v={miss}");

    // ③ SHOW：显窗接焦，恢复右窗 focus。
    set_visibility(rogue, right, true, right_geom);
    let (shown, left_shown, right_shown) = table_focus_snapshot(hw, left, right);
    if shown <= miss || left_shown || !right_shown {
        fail(&format!("p4 show snapshot v={shown} l={left_shown} r={right_shown}"));
    }
    println!("WMSHOWFOCUS ok shown={right} v={shown}");

    // ④ z-order：右窗移入重叠带。z 平局按表序，左窗先建者胜；
    //    右窗 z=1 后同点命中右窗。焦点必须跟随 hit-test winner。
    let overlap_x = left_w / 2;
    let overlap_geom = (overlap_x, 0, right_w, h);
    let overlap_center = overlap_x + left_w / 4;
    let overlap_y = h / 2;
    set_geometry_z(rogue, right, overlap_geom, 0);
    drain_pointer(input1);
    drain_pointer(input2);
    println!("WMZREADY ok x={overlap_center} y={overlap_y} z=0");
    wait_pointer_down(input1, input2, "z-tie");
    let (tie, left_tie, right_tie) = table_focus_snapshot(hw, left, right);
    if !left_tie || right_tie {
        fail(&format!("p4 z tie l={left_tie} r={right_tie}"));
    }
    println!("WMZFOCUS ok winner={left} z=0 v={tie}");
    set_z(rogue, right, 1, overlap_geom);
    drain_pointer(input1);
    drain_pointer(input2);
    println!("WMZRAISE ok window={right} z=1");
    wait_pointer_down(input2, input1, "z-top");
    let (top, left_top, right_top) = table_focus_snapshot(hw, left, right);
    if left_top || !right_top {
        fail(&format!("p4 z top l={left_top} r={right_top}"));
    }
    println!("WMZFOCUS ok winner={right} z=1 v={top}");
    set_geometry_z(rogue, right, right_geom, 0);

    // ⑤ stale input lock：真实 DOWN 锁定第三窗，guest 收到 DOWN 后销毁
    //    该窗，再注入 UP；wmd 必须合成 CANCEL+SYNTHESIZED 并清锁。
    let (lock, lock_x, lock_y, lock_w, lock_h) = create_lock_window(rogue);
    // 锁定窗叠在左窗上；z=1 确保 DOWN 命中锁定窗而非表序更早的左窗。
    set_z(
        rogue,
        lock,
        1,
        (lock_x, lock_y, lock_w, lock_h),
    );
    let (lock_created, lock_focused) = table_focus_one(hw, lock);
    if !lock_focused {
        fail("p4 lock window not focused");
    }
    drain_pointer(input1);
    drain_pointer(input2);
    println!(
        "WMLOCKREADY ok window={lock} x={lock_x} y={lock_y} w={lock_w} h={lock_h} v={lock_created}"
    );
    wait_pointer_down(input2, input1, "lock");
    destroy_window(rogue, lock);
    println!("WMLOCKDOWN ok window={lock} destroyed=1");
    let cancel = wait_pointer_cancel(input2);
    if cancel.phase != gp::PHASE_CANCEL
        || cancel.flags & gp::PE_FL_SYNTHESIZED == 0
    {
        fail(&format!("p4 cancel phase={} flags={}", cancel.phase, cancel.flags));
    }
    let (after_lock, left_after, right_after) = table_focus_snapshot(hw, left, right);
    if !right_after || left_after {
        fail(&format!("p4 post-lock l={left_after} r={right_after}"));
    }
    println!(
        "WMCANCEL ok window={lock} phase={} flags={} v={after_lock}",
        cancel.phase, cancel.flags
    );
}

/// 页取整帧 VMO（窗口局部 buffer；Phase 2 不再要求 app 分配全屏）。
fn frame_vmo(w: usize, h: usize) -> (Handle, usize, usize) {
    let stride = w.checked_mul(4).unwrap_or_else(|| fail("tile stride overflow"));
    let frame = stride.checked_mul(h).unwrap_or_else(|| fail("tile frame overflow"));
    let len = frame.checked_add(0xFFF).map(|v| v & !0xFFF)
        .unwrap_or_else(|| fail("tile page round overflow"));
    let mut vmo: Handle = 0;
    if berry_vmo_create(len as u64, 0, &mut vmo) != 0
        || vmo == 0
        || berry_vmo_commit(vmo, 0, len as u64) != 0
    {
        fail("tile vmo");
    }
    (vmo, stride, len)
}

/// 固定 VA 映射（caller 指定互不重叠窗口）。
fn map_frame(vmo: Handle, len: usize, va: usize) {
    let hv = self_vmar_h();
    if hv == 0 {
        fail("no self vmar gift");
    }
    let mut got = va;
    if berry_vmar_map(hv, 0, vmo, 0, &mut got, len, RIGHT_READ | RIGHT_WRITE) != 0
        || got != va
    {
        fail("tile vmar map");
    }
}

/// 渲一个确定性窗口 tile：底色 + 顶带 + 底条 + 三字符标签。
fn render_tile(va: usize, w: usize, h: usize, stride: usize,
               bg: (u8,u8,u8), band: (u8,u8,u8), strip: (u8,u8,u8), label: [u8;3]) {
    let desc = match TextureDesc::new(w, h, stride, TextureFormat::Rgbx) {
        Ok(d) => d,
        Err(e) => fail(&format!("tile texture {e:?}")),
    };
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8,
        stride.saturating_mul(h)) };
    let atlas = FontAtlas8x16;
    let glyphs = [
        GlyphPos { ch: u32::from(label[0]), x: 12, y: 12 },
        GlyphPos { ch: u32::from(label[1]), x: 20, y: 12 },
        GlyphPos { ch: u32::from(label[2]), x: 28, y: 12 },
    ];
    let mut enc = CommandEncoder::new();
    record(enc.fill_rect(
        Rect::new(0, 0, w as i32, (h / 3) as i32),
        Color { r: band.0, g: band.1, b: band.2 },
    ), "tile band");
    record(enc.fill_rect(
        Rect::new(0, (h - h / 12) as i32, w as i32, (h / 12) as i32),
        Color { r: strip.0, g: strip.1, b: strip.2 },
    ), "tile strip");
    record(enc.draw_glyph_run(GlyphRun {
        atlas: &atlas,
        glyphs: &glyphs,
        fg: Color { r: 255, g: 255, b: 255 },
        bg: Some(Color { r: bg.0, g: bg.1, b: bg.2 }),
    }), "tile glyph");
    let dl = enc.finish();
    if let Err(e) = Queue::new().submit(&dl, fb, &desc,
        &PassDescriptor::clear(bg.0, bg.1, bg.2)) {
        fail(&format!("tile render {e:?}"));
    }
}

/// 经 wmd 合成链提交一个窗口 tile：SUBMIT(surface=window) → FRAME_CALLBACK
/// → RELEASE。返回 guifbd 真实 token；RELEASE 归还的是 wmd composition
/// alias，客户端收下即关（本窗口 VMO 原值保留给 hdi PREPARE 复用）。
fn submit_tile(ch: Handle, vmo: Handle, _w: usize, _h: usize,
               surface_id: u32, buffer_id: u32) -> u32 {
    let mut alias: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
        fail("tile dup");
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let sreq = gp::PresentReq::Submit {
        surface_id,
        buffer_id,
        flags: gp::SUBMIT_F_DAMAGE_FULL,
        rate_hint: 0,
        target_present_ns: 0,
        rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
        rect_count: 0,
    };
    let m = enc_req(gp::encode_present_req(&mut req, &sreq));
    let (n, _) = rpc(ch, &req[..m], &[alias], &mut rep, &mut rhs);
    let token = match gp::parse_submit_ack(&rep[..n]) {
        Some(t) => t,
        None => fail(&format!("tile submit ack bad st={}", rep[0])),
    };
    let m = enc_req(gp::encode_present_req(&mut req,
        &gp::PresentReq::FrameCallback { frame_token: token }));
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    let done = match gp::parse_frame_done_rep(&rep[..n]) {
        Some(d) => d,
        None => fail(&format!("tile frame bad st={}", rep[0])),
    };
    if done.status != gp::FRAME_PRESENTED {
        fail(&format!("tile frame status={}", done.status));
    }
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
    let (n, nh) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    let rel = match gp::parse_release_rep(&rep[..n]) {
        Some(r) => r,
        None => fail(&format!("tile release bad st={}", rep[0])),
    };
    if rel.count != 1 || nh != 1 {
        fail(&format!("tile release count={} nh={}", rel.count, nh));
    }
    close_h(rhs[0]);
    token
}

/// R2 partial-damage 探针：提交一个横跨 raw 右边界的 16px 窄条。wmd 必须
/// 只拷贝窗内 8px；oversized VMO 的窗外 8px 不得进入右邻窗。
fn submit_partial_damage(
    ch: Handle,
    vmo: Handle,
    surface_id: u32,
    buffer_id: u32,
    rect: gp::Rect,
) -> u32 {
    let mut alias: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
        fail("partial damage dup");
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let mut rects = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
    rects[0] = rect;
    let sreq = gp::PresentReq::Submit {
        surface_id,
        buffer_id,
        flags: 0,
        rate_hint: 0,
        target_present_ns: 0,
        rects,
        rect_count: 1,
    };
    let m = enc_req(gp::encode_present_req(&mut req, &sreq));
    let (n, _) = rpc(ch, &req[..m], &[alias], &mut rep, &mut rhs);
    let token = match gp::parse_submit_ack(&rep[..n]) {
        Some(t) => t,
        None => fail(&format!("partial damage ack bad st={}", rep[0])),
    };
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::FrameCallback { frame_token: token },
    ));
    let (n, _) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_frame_done_rep(&rep[..n]) {
        Some(done) if done.status == gp::FRAME_PRESENTED => {}
        other => fail(&format!("partial damage frame got {other:?}")),
    }
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
    let (n, nh) = rpc(ch, &req[..m], &[], &mut rep, &mut rhs);
    let release = match gp::parse_release_rep(&rep[..n]) {
        Some(r) => r,
        None => fail(&format!("partial damage release bad st={}", rep[0])),
    };
    if usize::from(release.count) != nh {
        fail("partial damage release handles");
    }
    for handle in rhs.iter().take(nh) {
        close_h(*handle);
    }
    token
}

fn main() {
    // —— 降级门：wm 客户端端未交付（rootfs 相 wmd 随 fb 探测门不被拉起
    // → 本程序不被拉起；bsh 手跑无 extras）→ 一行诊断 + exit(0)（netd
    // 双门先例同款——非错误面，`gfx: wm absent` 永不进 MARKERS）。present/
    // hdi 客户端端已随 wmd 换手中继——本程序不再直持，单门即全链门（GUI-5
    // §7 Q12：extras 恰 wm_cli 一枚）。
    let broker = wm_cli_h();
    if broker == 0 {
        println!("gfx: wm absent");
        exit(0);
    }
    let process = self_proc_h();
    if process == 0 {
        fail("wm self process");
    }
    let mut process2: Handle = 0;
    let mut process3: Handle = 0;
    let mut process4: Handle = 0;
    if berry_handle_duplicate(process, RIGHT_DUPLICATE, &mut process2) != 0
        || process2 == 0
        || berry_handle_duplicate(process, RIGHT_DUPLICATE, &mut process3) != 0
        || process3 == 0
        || berry_handle_duplicate(process, RIGHT_DUPLICATE, &mut process4) != 0
        || process4 == 0
    {
        fail("wm self process dup");
    }
    // R1 SYSTEM 自报负例：APP 测试进程只持 broker endpoint，不持 svcmgr
    // per-boot SYSTEM seed channel；WM_CLIENT_SYSTEM 必须 ST_DENIED。
    {
        let mut local: Handle = 0;
        let mut remote: Handle = 0;
        let mut input_local: Handle = 0;
        let mut input_remote: Handle = 0;
        let mut spoof_process: Handle = 0;
        if berry_channel_create(0, &mut local, &mut remote) != 0
            || berry_channel_create(0, &mut input_local, &mut input_remote) != 0
            || berry_handle_duplicate(process, RIGHT_DUPLICATE, &mut spoof_process) != 0
            || spoof_process == 0
        {
            fail("wm system spoof channel");
        }
        let mut req = [0u8; 256];
        let mut rep = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let m = enc_req(gp::encode_wm_connect_req(
            &mut req,
            gp::WM_CLIENT_SYSTEM,
        ));
        let attach = [remote, input_remote, spoof_process];
        let (n, _) = rpc(broker, &req[..m], &attach, &mut rep, &mut rhs);
        match gp::parse_status_rep(&rep[..n]) {
            Some(gp::ST_DENIED) => {}
            other => fail(&format!("wm system spoof got {other:?}")),
        }
        println!("WMSYSCLIENTDENY ok");
    }
    wm_connect_wrong_system_seed_denied(broker, process);
    let (hw, input1) = wm_connect_process(broker, process);
    let (rogue, input2) = wm_connect_process(broker, process2);
    // P1：公开常量自铸 seed channel 必须在 broker 退役前被拒绝。
    wm_connect_forged_surface_denied(broker, process3);
    wm_system_seed_control_denied(broker);
    // 两个 session 建立后 broker 才退役；后续业务全部走 owner 通道。
    close_h(broker);

    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let mut req = [0u8; 256];

    // —— wm 域四步（GUI-5 §3 #12——wmd 本地权威面，先于 present 链：
    // 窗口策略先立，呈现消费随后）。全断言过才打 `WM ok`——恰一次。
    // ① GETINFO：能力/版本协商断言（version=1 ∧ windows_max=15 满配 ∧
    // 几何 >0——disp 域 = wmd 启动期 present GETINFO 缓存同源，Q15）。
    let m = gp::encode_wm_getinfo_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let wminfo = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("wm getinfo rep bad st={}", rep[0])),
    };
    if wminfo.version_wm != gp::VER_WM
        || wminfo.windows_max != gp::WM_WINDOWS_MAX as u8
        || wminfo.disp_w == 0
        || wminfo.disp_h == 0
    {
        fail(&format!(
            "wm getinfo vw={} wmax={} disp={}x{}",
            wminfo.version_wm, wminfo.windows_max, wminfo.disp_w, wminfo.disp_h
        ));
    }

    // ② CREATE：全屏 APP 层窗 + AVOID_SYS 旗标（gfxdemo 生命周期早于
    // GUI8 哨兵 SYSTEM occluder，因此 raw==visible；旗标面先接线）。
    // Phase 2：两个真实非重叠窗口（左右半屏），各自用窗口局部 buffer。
    let tile_w = wminfo.disp_w / 2;
    let tile_h = wminfo.disp_h;
    if tile_w == 0 || tile_h == 0 {
        fail("wm tile geometry zero");
    }
    let creq = gp::WmCreateReq {
        layer: gp::WM_LAYER_APP,
        flags: gp::WM_CREATE_F_AVOID_SYS,
        parent: 0,
        group: 0,
        x: 0,
        y: 0,
        w: tile_w,
        h: tile_h,
    };
    let m = enc_req(gp::encode_wm_create_req(&mut req, &creq));
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let wid = match gp::parse_wm_create_ack(&rep[..n]) {
        Some(id) if id != 0 => id,
        _ => fail(&format!("wm create ack bad st={}", rep[0])),
    };

    // R3 teeth：CREATED config 未 poll/ack 前，surface 绑定必须拒绝；
    // 房客不得带着未配置几何进入 0x67/SUBMIT。
    {
        let (deny_vmo, _, _) = frame_vmo(
            usize::from(creq.w),
            usize::from(creq.h),
        );
        let mut alias: Handle = 0;
        if berry_handle_duplicate(deny_vmo, RIGHT_READ, &mut alias) != 0
            || alias == 0
        {
            fail("unconfigured submit dup");
        }
        let submit = gp::PresentReq::Submit {
            surface_id: u32::from(wid),
            buffer_id: 0xD6,
            flags: gp::SUBMIT_F_DAMAGE_FULL,
            rate_hint: 0,
            target_present_ns: 0,
            rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
            rect_count: 0,
        };
        let m = enc_req(gp::encode_present_req(&mut req, &submit));
        let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
        match gp::parse_status_rep(&rep[..n]) {
            Some(gp::ST_DENIED) => {}
            other => fail(&format!("unconfigured submit got {other:?}")),
        }
        if berry_handle_close(deny_vmo) != 0 {
            fail("unconfigured submit close");
        }
        println!("WMCONFIGSUBMITDENY ok id={wid}");

        // 同一状态的 0x67 负例：Retained + Touch 在老 wmd 会成功；在
        // CREATED 未 ACK 前必须因 config gate 精确 ST_DENIED。
        let unconfigured_surface = gp::surface::SurfaceModeReq {
            surface_id: wid,
            role: gp::surface::SURFACE_ROLE_APP,
            mode: gp::surface::SURFACE_MODE_RETAINED,
            input_classes: gp::surface::INPUT_CLASS_TOUCH,
            ime_mode: gp::surface::IME_MODE_DISABLED,
            a11y_mode: gp::surface::A11Y_MODE_TREE,
        };
        let m = enc_req(gp::surface::encode_wm_surface_mode_req(
            &mut req, &unconfigured_surface,
        ));
        let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
        match gp::surface::parse_wm_surface_mode_rep(&rep[..n]) {
            Some(Err(gp::ST_DENIED)) => {}
            other => fail(&format!("unconfigured surface got {other:?}")),
        }
        println!("WMCONFIGSURFACEDENY ok id={wid}");
        let created = poll_config_event(hw)
            .unwrap_or_else(|| fail("config created absent"));
        if created.window_id != wid
            || created.cause != gp::WM_CONFIG_CAUSE_CREATED
            || !created.visible
        {
            fail(&format!(
                "config created got id={} cause={} visible={}",
                created.window_id, created.cause, created.visible
            ));
        }
        if ack_config_event(rogue, &created) != gp::ST_DENIED {
            fail("config foreign ack accepted");
        }
        println!("WMCONFIGFOREIGNACK ok id={wid}");
        if ack_config_event(hw, &created) != gp::ST_OK {
            fail("config owner ack failed");
        }
    }

    // ③ SET(GEOM)：同几何回写（20B 掩码式定长——masked-out 槽位 vis/z
    // 仍编码合法值：「编什么收什么」，Q17）；成功形 1B OK。
    let sreq = gp::WmSetReq {
        mask: gp::WM_SET_F_GEOM,
        window_id: wid,
        x: creq.x,
        y: creq.y,
        w: creq.w,
        h: creq.h,
        vis: gp::WM_VIS_SHOW,
        z: 0,
    };
    let m = enc_req(gp::encode_wm_set_req(&mut req, &sreq));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "wm set");
    assert_config_empty(hw, "same-geometry set");
    println!("WMCONFIGNOOPEMPTY ok id={wid}");

    // P1 Surface escape 负例：普通 APP session 未携带 per-boot seed，
    // 即使点名自己拥有的可见窗口，Canvas 也不得被授予；必须精确 1B
    // ST_DENIED，绝不静默降级 Retained 或返回 20B 授予帧。
    {
        let surface_req = gp::surface::SurfaceModeReq {
            surface_id: wid,
            role: gp::surface::SURFACE_ROLE_APP,
            mode: gp::surface::SURFACE_MODE_CANVAS,
            input_classes: gp::surface::INPUT_CLASS_TOUCH | gp::surface::INPUT_CLASS_MOUSE,
            ime_mode: gp::surface::IME_MODE_DISABLED,
            a11y_mode: gp::surface::A11Y_MODE_HYBRID,
        };
        let m = enc_req(gp::surface::encode_wm_surface_mode_req(
            &mut req, &surface_req,
        ));
        let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
        match gp::surface::parse_wm_surface_mode_rep(&rep[..n]) {
            Some(Err(gp::ST_DENIED)) => {}
            other => fail(&format!("wm surface deny got {other:?} st={}", rep[0])),
        }
    }

    // P1 服务侧拒面补充：foreign session 命名他人窗口必须 ST_NOENT；
    // owner 隐藏后 0x67 也必须 ST_NOENT；0x67 附带 handle 是帧面错误。
    {
        let surface_req = gp::surface::SurfaceModeReq {
            surface_id: wid,
            role: gp::surface::SURFACE_ROLE_APP,
            mode: gp::surface::SURFACE_MODE_CANVAS,
            input_classes: gp::surface::INPUT_CLASS_TOUCH | gp::surface::INPUT_CLASS_MOUSE,
            ime_mode: gp::surface::IME_MODE_DISABLED,
            a11y_mode: gp::surface::A11Y_MODE_HYBRID,
        };
        let m = enc_req(gp::surface::encode_wm_surface_mode_req(
            &mut req, &surface_req,
        ));
        let (n, _) = rpc(rogue, &req[..m], &[], &mut rep, &mut rhs);
        match gp::surface::parse_wm_surface_mode_rep(&rep[..n]) {
            Some(Err(gp::ST_NOENT)) => {}
            other => fail(&format!("wm surface foreign got {other:?}")),
        }

        let hide = gp::WmSetReq {
            mask: gp::WM_SET_F_VIS,
            window_id: wid,
            x: creq.x,
            y: creq.y,
            w: creq.w,
            h: creq.h,
            vis: gp::WM_VIS_HIDE,
            z: 0,
        };
        let m = enc_req(gp::encode_wm_set_req(&mut req, &hide));
        expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "wm hide");
        let hidden_ev = poll_config_event(hw)
            .unwrap_or_else(|| fail("config visibility hide absent"));
        if hidden_ev.window_id != wid
            || hidden_ev.cause != gp::WM_CONFIG_CAUSE_VISIBILITY
            || hidden_ev.visible
        {
            fail(&format!(
                "config hide got id={} cause={} visible={}",
                hidden_ev.window_id, hidden_ev.cause, hidden_ev.visible
            ));
        }
        if ack_config_event(hw, &hidden_ev) != gp::ST_OK {
            fail("config hide ack failed");
        }
        println!("WMCONFIGHIDE ok id={wid}");
        let m = enc_req(gp::surface::encode_wm_surface_mode_req(
            &mut req, &surface_req,
        ));
        let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
        match gp::surface::parse_wm_surface_mode_rep(&rep[..n]) {
            Some(Err(gp::ST_NOENT)) => {}
            other => fail(&format!("wm surface hidden got {other:?}")),
        }
        let show = gp::WmSetReq { vis: gp::WM_VIS_SHOW, ..hide };
        let m = enc_req(gp::encode_wm_set_req(&mut req, &show));
        expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "wm show");
        let shown_ev = poll_config_event(hw)
            .unwrap_or_else(|| fail("config visibility show absent"));
        if shown_ev.window_id != wid
            || shown_ev.cause != gp::WM_CONFIG_CAUSE_VISIBILITY
            || !shown_ev.visible
        {
            fail(&format!(
                "config show got id={} cause={} visible={}",
                shown_ev.window_id, shown_ev.cause, shown_ev.visible
            ));
        }
        if ack_config_event(hw, &shown_ev) != gp::ST_OK {
            fail("config show ack failed");
        }
        println!("WMCONFIGSHOW ok id={wid}");

        let m = enc_req(gp::surface::encode_wm_surface_mode_req(
            &mut req, &surface_req,
        ));
        let (n, _) = rpc(hw, &req[..m], &[process4], &mut rep, &mut rhs);
        match gp::surface::parse_wm_surface_mode_rep(&rep[..n]) {
            Some(Err(gp::ST_BAD)) => {}
            other => fail(&format!("wm surface handle got {other:?}")),
        }
    }
    drain_config(hw);

    // Phase 1 反 owner 负例：第二个 session 不得修改第一个 session 的窗。
    // 静默成功 / ST_BAD 都不合格，必须精确 ST_DENIED。
    let m = enc_req(gp::encode_wm_set_req(&mut req, &sreq));
    let (n, _) = rpc(rogue, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(gp::ST_DENIED) => {}
        other => fail(&format!("wm cross-owner set got {other:?}")),
    }
    println!("WMDENY ok");

    // R1 层权限负例：APP session 不得自报 SYSTEM layer。精确 ST_DENIED，
    // 不得回 ack/window id 或降级成 APP 层。
    let sysreq = gp::WmCreateReq {
        layer: gp::WM_LAYER_SYSTEM,
        flags: 0,
        parent: 0,
        group: 0,
        x: 0,
        y: 0,
        w: tile_w,
        h: 24,
    };
    let m = enc_req(gp::encode_wm_create_req(&mut req, &sysreq));
    let (n, _) = rpc(rogue, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(gp::ST_DENIED) => {}
        other => fail(&format!("wm system layer got {other:?}")),
    }
    println!("WMLAYERDENY ok");

    // R1 parent 权限负例：rogue 不得把窗口挂到 hw session 的窗口下。
    let childreq = gp::WmCreateReq {
        layer: gp::WM_LAYER_APP,
        flags: 0,
        parent: wid,
        group: 0,
        x: 0,
        y: 0,
        w: 32,
        h: 32,
    };
    let m = enc_req(gp::encode_wm_create_req(&mut req, &childreq));
    let (n, _) = rpc(rogue, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(gp::ST_DENIED) => {}
        other => fail(&format!("wm cross-owner parent got {other:?}")),
    }
    println!("WMPARENTDENY ok");

    // Phase 2 第二真实窗口：rogue session 拥有右半屏，不 destroy，
    // 退出时由 wmd Process-terminated 清理（unclean death 正例）。
    let creq2 = gp::WmCreateReq {
        layer: gp::WM_LAYER_APP,
        flags: 0,
        parent: 0,
        group: 0,
        x: tile_w,
        y: 0,
        w: wminfo.disp_w - tile_w,
        h: tile_h,
    };
    let m = enc_req(gp::encode_wm_create_req(&mut req, &creq2));
    let (n, _) = rpc(rogue, &req[..m], &[], &mut rep, &mut rhs);
    let wid2 = match gp::parse_wm_create_ack(&rep[..n]) {
        Some(id) if id != 0 => id,
        _ => fail(&format!("wm create2 ack bad st={}", rep[0])),
    };
    drain_config(rogue);
    println!("WMORPHAN ok id={wid2}");

    // R3 destroyed-window ACK：CREATE→POLL 取 serial 后销毁，旧 ACK 必须
    // ST_NOENT（不是 DENIED/OK），防止已亡窗口再消费配置。
    {
        let tmp_req = gp::WmCreateReq {
            layer: gp::WM_LAYER_APP,
            flags: 0,
            parent: 0,
            group: 0,
            x: 8,
            y: 8,
            w: 48,
            h: 48,
        };
        let m = enc_req(gp::encode_wm_create_req(&mut req, &tmp_req));
        let (n, _) = rpc(rogue, &req[..m], &[], &mut rep, &mut rhs);
        let tmp_win = match gp::parse_wm_create_ack(&rep[..n]) {
            Some(id) if id != 0 => id,
            _ => fail(&format!("config temp create st={}", rep[0])),
        };
        let tmp_ev = poll_config_event(rogue)
            .unwrap_or_else(|| fail("config temp event absent"));
        if tmp_ev.window_id != tmp_win
            || tmp_ev.cause != gp::WM_CONFIG_CAUSE_CREATED
        {
            fail(&format!(
                "config temp got id={} cause={}",
                tmp_ev.window_id, tmp_ev.cause
            ));
        }
        destroy_window(rogue, tmp_win);
        if ack_config_event(rogue, &tmp_ev) != gp::ST_NOENT {
            fail("config destroyed ack accepted");
        }
        println!("WMCONFIGDESTROYEDACK ok id={tmp_win}");
    }

    // P3：在 TABLE/Present 流之前完成 click-to-focus + Keyboard 真链。
    // GUI8 看到 WMFOCUSREADY 后注入左右点击与 KEY_A；本进程阻塞等待，
    // 证明完成才继续既有 gfxdemo 流。
    let focus_interactive = focus_keyboard_proof(
        hw, rogue, input1, input2, wid, wid2,
        creq.w, creq2.x, creq2.w, creq2.h,
    );
    if focus_interactive {
        assert_config_empty(hw, "focus move");
        assert_config_empty(rogue, "focus move");
        println!("WMCONFIGFOCUSEMPTY ok left={wid} right={wid2}");
        window_policy_proof(
            hw, rogue, input1, input2, wid, wid2,
            creq.w, creq2.x, creq2.w, creq2.h,
        );
        drain_config(hw);
        drain_config(rogue);
    }

    // ④ TABLE_GET：全表快照对拍——count≥1 ∧ window_id 回显 ∧ 有效几何
    // == 申请几何（v1 无系统层重叠 = 恒等；GUI-8 裁决四-e 定谳：状态栏/
    // 壁纸 = launcher 窗口内容层非 wmd 系统窗，恒等不破——真系统窗/避让
    // 裁剪刀再随改，guiproto `WmEntry` doc 注同口径）。
    let m = gp::encode_wm_table_get_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let snap = match gp::parse_wm_snapshot_rep(&rep[..n]) {
        Some(Ok(s)) => s,
        Some(Err(st)) => fail(&format!("wm table st={st}")),
        None => fail(&format!("wm table rep bad len={n}")),
    };
    if snap.count != 2 {
        fail(&format!("wm table count={}", snap.count));
    }
    match snap.entries[..snap.count].iter().find(|e| e.id == wid2) {
        Some(e) => {
            if e.x != creq2.x || e.y != creq2.y || e.w != creq2.w || e.h != creq2.h {
                fail(&format!(
                    "wm geom2 echo {}x{}+{}+{} vs req {}x{}+{}+{}",
                    e.w, e.h, e.x, e.y, creq2.w, creq2.h, creq2.x, creq2.y
                ));
            }
        }
        None => fail(&format!("wm table no win2 id={wid2}")),
    }
    match snap.entries[..snap.count].iter().find(|e| e.id == wid) {
        Some(e) => {
            if e.x != creq.x || e.y != creq.y || e.w != creq.w || e.h != creq.h {
                fail(&format!(
                    "wm geom echo {}x{}+{}+{} vs req {}x{}+{}+{}",
                    e.w, e.h, e.x, e.y, creq.w, creq.h, creq.x, creq.y
                ));
            }
        }
        None => fail(&format!("wm table no win id={wid}")),
    }
    println!("WM ok");
    wm_app_system_role_denied(hw, wid);

    // —— GETINFO：能力/版本/格式协商断言（ver=1 ∧ XRGB8888 位在册 ∧
    // credits=2——Bgrx 与 ramfb 同源的格式面在位证明）。
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo));
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let info = match gp::parse_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("getinfo rep bad st={}", rep[0])),
    };
    if info.version_present != gp::VER_PRESENT
        || info.format_bits & gp::FMT_XRGB8888 == 0
        || info.max_credits != gp::CREDITS_MAX
    {
        fail(&format!(
            "getinfo vp={} fmt={:#x} credits={}",
            info.version_present, info.format_bits, info.max_credits
        ));
    }

    // —— Phase 2 两窗口局部帧：session 1 左半屏，session 2 右半屏。
    // wmd 是 guifbd 唯一 present client：两个 SUBMIT 都先拷入 wmd
    // composition VMO，再聚合提交 guifbd。
    let (w, h) = (tile_w as usize, tile_h as usize);
    let (vmo, stride, len) = frame_vmo(w, h);
    map_frame(vmo, len, FRAME_VA);
    render_tile(FRAME_VA, w, h, stride,
        (0x10,0x18,0x28),(0x7F,0,0),(0,0x60,0),[b'W',b'I',b'N']);
    // Phase 2 surface-owner 负例：rogue session 不得以 session 1 的窗口 id
    // 提交 buffer；wmd 必须在拷贝前拒绝 ST_DENIED。
    {
        let mut alias: Handle = 0;
        if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
            fail("wm surface deny dup");
        }
        let sreq = gp::PresentReq::Submit {
            surface_id: u32::from(wid),
            buffer_id: 0xD1,
            flags: gp::SUBMIT_F_DAMAGE_FULL,
            rate_hint: 0,
            target_present_ns: 0,
            rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
            rect_count: 0,
        };
        let m = enc_req(gp::encode_present_req(&mut req, &sreq));
        let (n, _) = rpc(rogue, &req[..m], &[alias], &mut rep, &mut rhs);
        match gp::parse_status_rep(&rep[..n]) {
            Some(gp::ST_DENIED) => {}
            other => fail(&format!("wm surface deny got {other:?}")),
        }
        println!("WMSDENY ok");
    }

    // R1 全局面负例：APP session 的 surface_id=0 是未授权全屏涂刷，
    // 必须在拷贝前 ST_DENIED。
    {
        let mut alias: Handle = 0;
        if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
            fail("wm global deny dup");
        }
        let sreq = gp::PresentReq::Submit {
            surface_id: 0,
            buffer_id: 0xD2,
            flags: gp::SUBMIT_F_DAMAGE_FULL,
            rate_hint: 0,
            target_present_ns: 0,
            rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
            rect_count: 0,
        };
        let m = enc_req(gp::encode_present_req(&mut req, &sreq));
        let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
        match gp::parse_status_rep(&rep[..n]) {
            Some(gp::ST_DENIED) => {}
            other => fail(&format!("wm global surface got {other:?}")),
        }
        println!("WMGLOBALDENY ok");
    }

    // R1 damage 越窗负例：owner 自己也只能提交与本窗相交的 damage。
    // 完全越界批不得被全屏 clamp 成涂刷，必须 ST_BAD。
    {
        let mut alias: Handle = 0;
        if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
            fail("wm damage deny dup");
        }
        let mut rects = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
        rects[0] = gp::Rect {
            x: w as u32 + 16,
            y: h as u32 + 16,
            w: 32,
            h: 32,
        };
        let sreq = gp::PresentReq::Submit {
            surface_id: u32::from(wid),
            buffer_id: 0xD3,
            flags: 0,
            rate_hint: 0,
            target_present_ns: 0,
            rects,
            rect_count: 1,
        };
        let m = enc_req(gp::encode_present_req(&mut req, &sreq));
        let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
        match gp::parse_status_rep(&rep[..n]) {
            Some(gp::ST_BAD) => {}
            other => fail(&format!("wm damage clip got {other:?}")),
        }
        println!("WMDAMAGEBAD ok");
    }
    let token1 = submit_tile(hw, vmo, w, h, u32::from(wid), 1);

    let w2 = (wminfo.disp_w - tile_w) as usize;
    let (vmo2, stride2, len2) = frame_vmo(w2, h);
    map_frame(vmo2, len2, FRAME_VA + 0x0100_0000);
    render_tile(FRAME_VA + 0x0100_0000, w2, h, stride2,
        (0x18,0x10,0x28),(0,0x7F,0xFF),(0xBF,0x5A,0),[b'2',b'N',b'D']);
    let token2 = submit_tile(rogue, vmo2, w2, h, u32::from(wid2), 2);
    if token1 == 0 || token2 == 0 || token1 == token2 {
        fail(&format!("wm2 tokens {token1}/{token2}"));
    }
    println!("WM2FRAME ok n=2");

    // R2 partial-damage 像素面：左窗 raw 宽 w，故意交 (w+16) 宽 VMO 并
    // 提交跨界 16px。窗内 8px 变品红；窗外 8px 若被拷贝会污染右邻窗。
    {
        let (probe_vmo, probe_stride, probe_len) = frame_vmo(w + 16, h);
        const PROBE_VA: usize = 0x5300_0000;
        map_frame(probe_vmo, probe_len, PROBE_VA);
        let probe = unsafe {
            core::slice::from_raw_parts_mut(PROBE_VA as *mut u8, probe_stride * h)
        };
        for row in probe.chunks_exact_mut(probe_stride) {
            for px in row.chunks_exact_mut(4) {
                // Rgbx 内存序 B,G,R,X；PPM 观察值为 (255,0,255)。
                px.copy_from_slice(&[255, 0, 255, 255]);
            }
        }
        let rect = gp::Rect {
            x: (w - 8) as u32,
            y: (h / 2) as u32,
            w: 16,
            h: 1,
        };
        let token = submit_partial_damage(
            hw,
            probe_vmo,
            u32::from(wid),
            0xD4,
            rect,
        );
        if token == 0 {
            fail("partial damage token");
        }
        println!(
            "WMDAMAGECLIP ok x={} y={} w={} h=1",
            rect.x, rect.y, rect.w
        );

        // R2 负例：旧 eff-size buffer（小于 raw_w*raw_h）不能再通过
        // size gate；FULL 提交必须 ST_BAD，防行错位静默。
        let (bad_vmo, _bad_stride, _bad_len) = frame_vmo(w - 8, h);
        let mut bad_alias: Handle = 0;
        if berry_handle_duplicate(bad_vmo, RIGHT_READ, &mut bad_alias) != 0
            || bad_alias == 0
        {
            fail("raw buffer deny dup");
        }
        let bad_submit = gp::PresentReq::Submit {
            surface_id: u32::from(wid),
            buffer_id: 0xD5,
            flags: gp::SUBMIT_F_DAMAGE_FULL,
            rate_hint: 0,
            target_present_ns: 0,
            rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
            rect_count: 0,
        };
        let m = enc_req(gp::encode_present_req(&mut req, &bad_submit));
        let (n, _) = rpc(hw, &req[..m], &[bad_alias], &mut rep, &mut rhs);
        match gp::parse_status_rep(&rep[..n]) {
            Some(gp::ST_BAD) => {}
            other => fail(&format!("raw buffer deny got {other:?}")),
        }
        if berry_handle_close(bad_vmo) != 0 {
            fail("raw buffer deny close");
        }
        println!("WMRAWBUFBAD ok w={}", w - 8);
    }
    // 留 2s 呈现窗：QEMU/serial 观察者可在此窗口抓屏（机器标记之外
    // 的像素证据面）；不影响协议，只推迟本一次性测试客户端退出。
    if berry_thread_sleep(now() + 2_000_000_000) != 0 {
        fail("wm2 present sleep");
    }

    // —— VOTE/POWER_MODE：1B 烘焙线成功形（§0「只记账不仲裁」——票收下
    // 即回执；POWER_DIM = §3#6 钉的 DIM 档）。VOTE scene 位携 berry_intent
    // 六档值（GUI-8 §4 段①/ADR #58 收官——常态票 = INTERACTIVE，SCENE
    // 词表退役 deprecated）。
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Interactive as u8,
            min_hz: 0,
            max_hz: 60,
            preferred_hz: 60,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote");
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::PowerMode { mode: gp::POWER_DIM },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "power mode");

    // —— hdi 域软 vsync mini 序：REG_VBLANK(1)+event（send 即 take——event
    // 归服务端）→ VBLANK_WAIT 断 seq≥1 ∧ ts_ns>0（ts = clock 合成软 vsync
    // 点）→ PREPARE **再 dup 新别名**随行（A6-b：SUBMIT 旧别名已归服务端，
    // 本表原值 -EBADF）断 slot<2（parse 面执法；应答随行 1 fence Event =
    // 空置档，收下即关）→ COMMIT(slot) → REG_VBLANK(0) 关闭形（也随行 1
    // Event：服务端 close 存量 + close 本次，§3#4⑤）。
    let mut ev: Handle = 0;
    if berry_event_create(0, &mut ev) != 0 || ev == 0 {
        fail("event create");
    }
    let m = gp::encode_reg_vblank_req(&mut req, 1);
    expect_ok(hw, &req[..m], &[ev], &mut rep, &mut rhs, "reg vblank on");

    let m = gp::encode_vblank_wait_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let vb = match gp::parse_vblank_rep(&rep[..n]) {
        Some(v) => v,
        None => fail(&format!("vblank rep bad st={}", rep[0])),
    };
    if vb.seq < 1 || vb.ts_ns == 0 {
        fail(&format!("vblank seq={} ts={}", vb.seq, vb.ts_ns));
    }

    let mut alias2: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias2) != 0 || alias2 == 0 {
        fail("dup alias2");
    }
    let preq = gp::PrepareReq {
        flags: 0,
        width: w as u32,
        height: h as u32,
        stride: stride as u32,
        format: gp::FMT_XRGB8888,
    };
    let m = gp::encode_prepare_req(&mut req, &preq);
    let (n, nh) = rpc(hw, &req[..m], &[alias2], &mut rep, &mut rhs);
    let slot = match gp::parse_prepare_rep(&rep[..n]) {
        Some(s) => s,
        None => fail(&format!("prepare rep bad st={}", rep[0])),
    };
    if nh != 1 {
        fail(&format!("prepare fence nh={nh}"));
    }
    close_h(rhs[0]);

    let m = gp::encode_commit_req(&mut req, slot, 0);
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "commit");

    let mut ev2: Handle = 0;
    if berry_event_create(0, &mut ev2) != 0 || ev2 == 0 {
        fail("event create 2");
    }
    let m = gp::encode_reg_vblank_req(&mut req, 0);
    expect_ok(hw, &req[..m], &[ev2], &mut rep, &mut rhs, "reg vblank off");

    // 生命周期收口：全链成功后显式销毁测试窗，并回读 WM 表验证本窗无
    // 残留。单客户端时代这是“CREATE/SET/TABLE_GET 的反向下限”；后续多
    // session owner 刀沿用同一验收语义。
    let m = gp::encode_wm_destroy_req(&mut req, wid);
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "wm destroy");
    let m = gp::encode_wm_table_get_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let after = match gp::parse_wm_snapshot_rep(&rep[..n]) {
        Some(Ok(s)) => s,
        Some(Err(st)) => fail(&format!("wm after destroy st={st}")),
        None => fail(&format!("wm after destroy rep bad st={}", rep[0])),
    };
    if after.entries[..after.count].iter().any(|e| e.id == wid) || after.count != 1 {
        fail(&format!("wm destroy leaves count={}", after.count));
    }
    if !after.entries[..after.count].iter().any(|e| e.id == wid2) {
        fail("wm destroy dropped unowned second window");
    }
    println!("WMDTY ok n=1");

    // 客户端全链断言过（服务端真值 GFXF ok n= 由 guifbd 首呈现时打出）
    // ——结构性恰一次：svcmgr 单次预拉 + 阻塞 reap + 单 println。
    println!("GFX ok");
    exit(0);
}
