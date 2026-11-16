//! berry-launcher —— GUI-8 桌面 launcher（docs/GUI-8实施设计草案.md §4
//! 段③；前身 = gui3 前置 wiring 刀 v0 park 骨架）。
//!
//! **模块树（波3-件⑦ 起——五模块通用面抽入 berry-appshell，本 crate 只
//! 留桌面特化）**：
//!
//! - [`icons`]：appshell::icons 再导出（app 注册表 + 图标供给桥——
//!   boot 期 APP_LIST 拉取/BYI 解码/OH 资产全在 appshell）。
//! - [`layout_io`]：LAYOUT.JSN 读写三约束（缺席/坏件回默认 + 重写自愈
//!   + 写失败非致命）——文件 I/O 半程走 appshell::conf，桌面布局
//!   schema（默认装填/冒烟变体/全等比对）留本 crate。
//! - [`desktop`]：桌面静态面建树特化（OH 官方壁纸/SwipePager 四页
//!   Grid 5×4/常驻 Dock/五键导航条/波3-件⑤ TabBar 三系统页）——状态
//!   栏整条/scrim/文本叶等底座走 appshell::scene。
//! - [`frameloop`]：launcher 帧循环特化（自测状态机/点击 spawn/五键
//!   导航/时钟续摆/关机 armed）——帧循环骨架（present 会话/帧 VMO/
//!   SUBMIT 链/输入泵/稳态等待）走 appshell::pump 的 HostHooks 回调。
//!
//! 本文件保留面：wm 门控 + appshell Host 绑定（诊断前缀 "launcher"——
//! 与抽取前字节一致）+ 严格串行 RPC 消费（appshell::rpc）+ `LAUNCH v=`
//! 非标记行 + CRT0 链接种子 + 降级 park 常驻纪律（崩溃监督一次重启帽
//! 归 svcmgr 域——裁决四-d）。
//!
//! **门控面（wm_cli_h()）**：
//!
//! - wm 客户端端未交付（0——rootfs 相 wmd 随 fb 探测门不被拉起 → 本
//!   程序不被拉起；亦含理论不可达兜底 = 一切无预拉 spawn/bsh 手跑）→
//!   **单行降级输出 + park**（v0 骨架形态延续，hdi_srv_h 先例；勿改成
//!   gfxdemo 的 exit(0) 形——`LAUNCH wm absent` 永不进 MARKERS，零标记
//!   扰动）。
//! - 在场（EFI 路）→ 严格串行一次 WM_GETINFO RPC（appshell::rpc 形态：
//!   send -EBUSY → WRITABLE 帽重臂；recv -EAGAIN → READABLE 帽；无
//!   attach 面——GETINFO 无随行 handle）→ parse `WmGetinfoRep` →
//!   **单行非标记日志**（`LAUNCH` 前缀 canon 零碰撞——2026-11-02 grep
//!   全树实证；无 " ok" 形 = 非 smoke 标记语义）。**原值宁报勿猜**
//!   （fmt 原值先例）：不做 gfxdemo 式断言链（那是 WM ok 标记语义面，
//!   launcher 非标记面）；parse 失败 = eprintln 诊断 + park（不留
//!   半开态）。
//!
//! **输出面**：EFI 相 launcher 输出 = `LAUNCH v=…` 非标记行（GETINFO
//! 后）+ 帧循环两标记（`LAUNCH ok` 首帧呈现 / `PAGE ok n=2` 合成
//! swipe 双翻页——字节精确，frameloop 自测产物）；icons/layout_io
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
mod layout_io;
mod settings_io;
mod syspages;

use berry_appshell as appshell;

pub use appshell::icons;

use appshell::rpc::{fail, rpc};
use berry_guiproto as gp;
use berry_libc::io::{app_cli_h, fs_h, input_h, wm_cli_h};
use berry_sys::{berry_clock_read, Handle, CLOCK_REALTIME};
use berry_ui::{ProportionalTextMetrics, UiMain};

// 链接种子（fsmgr/guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// appshell 宿主绑定（诊断前缀 "launcher" + park + FAT fd 面——libc
/// 符号经包装 fn 抹平 c_char/c_void 差；抽取前各诊断行字节不变）。
static SHELL_HOST: appshell::Host = appshell::Host {
    tag: "launcher",
    diag: |s| eprintln!("{s}"),
    park: berry_libc::park,
    fs: Some(appshell::Fs {
        open: libc_open,
        read: libc_read,
        write: libc_write,
        close: libc_close,
        errno: berry_libc::get_errno,
    }),
};

unsafe fn libc_open(path: *const u8, flags: i32) -> i32 {
    berry_libc::fs::open(path as *const _, flags)
}

fn libc_read(fd: i32, buf: *mut u8, len: usize) -> isize {
    berry_libc::io::read(fd, buf as *mut _, len)
}

fn libc_write(fd: i32, buf: *const u8, len: usize) -> isize {
    berry_libc::io::write(fd, buf as *const _, len)
}

fn libc_close(fd: i32) -> i32 {
    berry_libc::io::close(fd)
}

fn main() {
    // —— appshell 绑定（boot 期单次）：诊断/park/fs 三面注入——此后
    //    appshell 各模块的诊断行与抽取前字节一致（"launcher: …"）。
    appshell::bind(&SHELL_HOST);

    // —— 降级门：wm 客户端端未交付（rootfs 相 wmd 随 fb 探测门不被拉起
    //    → 本程序不被拉起；bsh 手跑无 extras）→ 一行诊断 + park（v0
    //    骨架形态延续——理论不可达兜底，hdi_srv_h 先例；勿改成
    //    gfxdemo 的 exit(0) 形）。
    let hw = wm_cli_h();
    if hw == 0 {
        eprintln!("LAUNCH wm absent (no wm client gift)");
        berry_libc::park();
    }

    // —— WM_GETINFO 一次握手（4B 无参请求 / 12B 应答——wmd 权威能力面，
    //    disp 域 = wmd 启动期 present GETINFO 缓存同源）。原值宁报勿猜——
    //    不做 gfxdemo 式断言链（WM ok 标记语义面归 gfxdemo，launcher
    //    非标记面）；parse 失败 = eprintln 诊断 + park（不留半开态）。
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
    // 相 launcher 全程唯一输出行（icons/layout_io 冻结相零打印，桌面链
    // 在此行后静默）。原值直报（v0 fmt 原值先例延续）。
    println!(
        "LAUNCH v={} wmax={} {}x{} input={}",
        info.version_wm, info.windows_max, info.disp_w, info.disp_h, input_h()
    );

    // —— 段③ 静态面接线（W1：树成帧不驱——frameloop 静默 park）——
    //    注册表初始化（W2 = app 域 APP_LIST 拉取装载；冻结相
    //    app_cli_h()==0（extras 恰 [wm]）→ 空注册表——草案 R1 修约 1/2）。
    //    此步也是 fs 挂载就绪的串行点：APP_LIST 应答前 appd/fsmgr 已进
    //    稳态。
    icons::init(app_cli_h());
    // fs gift 就绪性（冻结相 extras 恰 [wm] → 布局持久化面结构性缺席
    // ——layout_io/conf 先导门单次取值，零 RPC）。
    let fs_ready = fs_h() != 0;
    // 冒烟模式 = 数据盘根 GUI8.SMK 载荷精确匹配（main 单次读取；缺失/
    // 错载荷 fail closed → false）。**在 layout_io::load 之前**取一次，
    // bool 同时门控布局冒烟往返与 frameloop 的 CLICK/click ready 两行；
    // 正常 boot 的点击 spawn 行为不受影响（只是不打标记）。
    let smoke = layout_io::smoke_mode(fs_ready);
    // 布局装载（冻结相 → 默认布局直返零 RPC；三约束自愈面见 layout_io
    // 头注）。
    let layout = layout_io::load(icons::registry(), smoke, fs_ready);
    // Ui<512> arena ≈ 75KB+（Node 144B×512 + 走查/序/快照缓冲侧表）——
    // guest 栈 stack_pages=64 = 256KB（R5 仪器定谳，svcmgr 装配注释为
    // 单源——8 页下 main+0x1c 序言即穿保护页）栈置仍不稳：Box::leak
    // 堆置（桌面进程终身持有——常驻面无回收语义，leak 即分配）。
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    ui.set_icon_source(icons::icon_source());
    // 比例文本供件（痛点⑥根修·消费点切换，03 号 §1.3-3）：HD 档
    //（big 模式状态栏/格 label S32x48）内在测量与渲染同走真实 advance
    //（「12:00」等串宽从 len×32 等宽值 → Σadvance 比例值）；三档点阵
    // 下与 Monospace 逐值恒等（advance 缺省 = cell_w）——零漂移面。
    static TEXT_METRICS: ProportionalTextMetrics = ProportionalTextMetrics;
    ui.set_text_metrics(&TEXT_METRICS);
    // —— 波一刀 B（状态栏可信化）：真时钟首摆——宿主 berry 经 fw_cfg
    //    `opt/berry/rtc` 注入 epoch（内核 rtc.rs 消费），此处单取
    //    CLOCK_REALTIME 换算 "HH:MM"（UTC 口径）。注入缺席（-ENOTSUP，
    //    如 -kernel/自建拓扑冒烟）= None → 建树回退 12:00 静态占位，
    //    零标记零漂移；在场打一行串口证据（"launcher:" 非 canon 前缀）。
    let mut rt = 0u64;
    let clock = if berry_clock_read(CLOCK_REALTIME, &mut rt) == 0 {
        let hm = appshell::scene::hhmm_utc(rt);
        eprintln!(
            "launcher: clock {} utc (realtime)",
            core::str::from_utf8(&hm).unwrap_or("?")
        );
        Some(hm)
    } else {
        None
    };
    // 桌面建树（几何全 GETINFO 动态——ui.viewport()；Err = 容量/文本域
    // 违例 → fail 收口：eprintln + park，监督重启归 svcmgr 域）。
    // —— 波3-件⑤c：设置真值首读（BOOT.JSN / SETTINGS.JSN——I/O 走
    //    appshell::conf 单簇原子写面；冻结相 fs gift 缺席默认直返零
    //    RPC；canonical 字节锚点见 settings_io 头注）。
    let boot = settings_io::load_boot();
    let settings = settings_io::load_settings();
    let desktop = match desktop::build(ui, &layout, icons::registry(), clock, boot, settings) {
        Ok(d) => d,
        Err(e) => fail(&format!("desktop build err={e:?}")),
    };
    // 帧循环（W1 静默 park；LAUNCH ok/PAGE ok = W2 产物）。smoke = 段④
    // 两标记的唯一门（CLICK ok / launcher: click ready）；正常 boot 传
    // false = 点击照常 spawn、零新标记。
    frameloop::run(hw, input_h(), ui, desktop, smoke);
}
