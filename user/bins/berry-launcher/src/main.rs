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

mod desktop;
mod frameloop;
mod icons;
mod layout_io;

use berry_guiproto as gp;
use berry_libc::io::{app_cli_h, input_h, wm_cli_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_object_wait_one, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_ui::UiMain;

// 链接种子（fsmgr/guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（gfxdemo 10s A6-a 形态；launcher 120s：首启要等
/// svcmgr 跑完 boot 自验进稳态 app serve，10s 会把 APP_LIST 误判成超时）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 诊断 + park 收口（gfxdemo `fail` 的 park 形——launcher 常驻不被 reap，
/// exit 码无消费者；park = stdout EOF + 10s 周期睡眠，guifbd 自验后
/// park 同款；崩溃监督/一次重启帽归 svcmgr，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("launcher: {what}");
    berry_libc::park()
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
            fail("rpc reply timeout 10s");
        }
    }
}

/// 严格串行 RPC 单趟（一请求一应答；gfxdemo `rpc` 同形——wm/present 域
/// 消费面）。发送侧：-EBUSY → 阻塞等 WRITABLE（10s 帽重臂）。收侧：空
/// 队 -EAGAIN → 阻塞等 READABLE（`RPC_CAP_NS` 帽）。返回 `(应答长,
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

fn main() {
    // —— 降级门：wm 客户端端未交付（rootfs 相 wmd 随 fb 探测门不被拉起
    // → 本程序不被拉起；bsh 手跑无 extras）→ 一行诊断 + park（v0 骨架
    // 形态延续——理论不可达兜底，hdi_srv_h 先例；勿改成 gfxdemo 的
    // exit(0) 形）。
    let hw = wm_cli_h();
    if hw == 0 {
        eprintln!("LAUNCH wm absent (no wm client gift)");
        berry_libc::park();
    }

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
    // 相 launcher 全程唯一输出行（段③ W1 注：icons/layout_io 冻结相零
    // 打印，桌面链在此行后静默）。原值直报（v0 fmt 原值先例延续）。
    println!(
        "LAUNCH v={} wmax={} {}x{} input={}",
        info.version_wm, info.windows_max, info.disp_w, info.disp_h, input_h()
    );

    // —— 段③ 静态面接线（W1：树成帧不驱——frameloop 静默 park）——
    // 注册表初始化（W2 = app 域 APP_LIST 拉取装载；冻结相 app_cli_h()==0
    //（extras 恰 [wm]）→ 空注册表——草案 R1 修约 1/2）。此步也是 fs 挂载
    // 就绪的串行点：APP_LIST 应答前 appd/fsmgr 已进稳态。
    icons::init(app_cli_h());
    // 冒烟模式 = 数据盘根 GUI8.SMK 载荷精确匹配（main 单次读取；缺失/
    // 错载荷 fail closed → false）。**在 layout_io::load 之前**取一次，
    // bool 同时门控布局冒烟往返与 frameloop 的 CLICK/click ready 两行；
    // 正常 boot 的点击 spawn 行为不受影响（只是不打标记）。
    let smoke = layout_io::smoke_mode();
    // 布局装载（冻结相 fs_h()==0 → 默认布局直返零 RPC；三约束自愈面
    // 见 layout_io 头注）。
    let layout = layout_io::load(icons::registry(), smoke);
    // Ui<512> arena ≈ 75KB+（Node 144B×512 + 走查/序/快照缓冲侧表）——
    // guest 栈 stack_pages=64 = 256KB（R5 仪器定谳，svcmgr 装配注释为
    // 单源——8 页下 main+0x1c 序言即穿保护页）栈置仍不稳：Box::leak
    // 堆置（桌面进程终身持有——常驻面无回收语义，leak 即分配）。
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    ui.set_icon_source(icons::icon_source());
    // 桌面建树（几何全 GETINFO 动态——ui.viewport()；Err = 容量/文本域
    // 违例 → fail 收口：eprintln + park，监督重启归 svcmgr 域）。
    let desktop = match desktop::build(ui, &layout, icons::registry()) {
        Ok(d) => d,
        Err(e) => fail(&format!("desktop build err={e:?}")),
    };
    // 帧循环（W1 静默 park；LAUNCH ok/PAGE ok = W2 产物）。smoke = 段④
    // 两标记的唯一门（CLICK ok / launcher: click ready）；正常 boot 传
    // false = 点击照常 spawn、零新标记。
    frameloop::run(hw, input_h(), ui, desktop, smoke);
}
