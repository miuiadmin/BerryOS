//! guifbd —— GUI-1（M-GUI 首刀）framebuffer 显示服务：V-6 刀 2 后段
//! 挂账收口（fbcon 自绘字形 + 蜂格渲染核的首个真机消费面）。
//!
//! **boot 授能链**（内核 main.rs fb 授能块，A-2 virtio MMIO 窗同模式）：
//! EFI-GOP 命中时 fb 窗（RamfbDxe AllocatePages 产物，普通 DRAM）包成
//! MMIO VMO 插 svcmgr 表第 5 插（`H_FB=0x0001_0005`）；几何五域
//! fb_pa/w/h/stride/fmt 落 40B 匿名 VMO（u64 LE）第 6 插
//! （`H_FBINFO=0x0001_0006`）。svcmgr 预拉段 spawn 本服务：fb 窗经
//! `SpawnSpec::mmio_vmo` 映到固定 [`FB_VA`]（berry-exec 单源），info
//! VMO 经 env 块 extras[0] 随行（libc `gift2_h()`——rootfs_h 同槽别名
//! 惯例）。非 EFI 路（-kernel 无 GOP）svcmgr 探测失败不 spawn——本服务
//! 恒只在 EFI 链存活。
//!
//! **自验三步**（像素读写对拍，非 grep 串口——smoke 断言在标记本身）：
//! ①clear 双相位差分（bg0→抽样==pack(bg0)→bg1→同点复验——防「写没生效
//! 读回恒旧」的 buddy 双写假绿）；②put_str("AB\nCD") 蜂格全对拍（字形
//! 位图逐像素，MSB=最左列——font 模块约定）；③scroll_up(1) 后行移对拍
//! （memmove 路径 + 滚出空行=bg）。全过才 `println!("FBW ok …")`（stdout
//! = console 发端，vblk `VBLK ok` 同款）→ park 挂起（标记缺失 =
//! smoke-image FAIL loud 兜底）。抽样读回走裸指针 `read_volatile`——
//! Fbcon 持有 fb slice 的 &mut 借用，读回须独立通道；ramfb 实为 DRAM，
//! volatile 是 Device 映射语义下的保守保险（零成本）。
//!
//! 段②（console fan-out 上屏）：park 换 **serve 循环**（fsmgr 形态）——
//! 收 console 转发的输出字节流 put_str 上屏（窗口与串口同内容；boot 早期
//! guifbd 未起时的输出丢 = console 侧 best-effort 既定语义）。首条消息到达
//! 一次性自打 `FBSERVE ok`（稳态矩阵打印持续不断 → 首条必达 = 状态谓词
//! 非竞速，4-5a 铁律）；空队 -EAGAIN → wait READABLE 10s 帽**重臂**续睡
//! （console 长静默合法空闲非断链——echo trap-on-timeout 反例见 fsmgr 头
//! 注）；非 -EAGAIN 错码 = 通道链断 trap（boot 定值接线，错即接线 bug，
//! 只死自己 + `FBSERVE ok` 缺失 = smoke FAIL loud 兜底）。
//!
//! 段③（GUI-2 消费侧首刀）：present/hdi 域**服务端真身**——serve 循环
//! 升格三路（present→hdi→console 确定性轮询，全 EAGAIN 才
//! `berry_object_wait_many` n=3 并等；deadline = min(下次软 vsync 点,
//! now+10s) **进调用本身**——parked 应答不靠醒后补查，最迟迟答 = 一个
//! 空闲帽；每轮 poll 前重置 nb/nh 容量——漏重置 = 恒 -EINVAL 死循环）。
//! present 六 op v1 = **blit-at-SUBMIT**（处理内同步 damage 裁剪 CPU
//! blit → px() 7 点抽样对拍 vmo_read → present_seq++ → 8B token；无
//! pending 帧状态机——squash/DISCARDED/SUBMIT_F_UNSQUASHABLE = 死路径
//! 留异步后端刀，永不判弃即合规）；credits = 在途未 RELEASE 别名数 ≤2
//! （满 → ST_FULL 1B 烘焙线）；「无 damage 不提交」两条 ST_BAD 立法
//! （K>0∧FULL 同设 / K=0∧无 FULL）；**一切拒收面 close 已随行迁入的
//! handle**（send 即 take，不 close 即泄漏）。hdi 五 op 退化形：
//! REG_VBLANK 存 event、VBLANK_WAIT park 到软 vsync 点（周期查
//! `mod panel` 档位表——刀1 率通路，旧 SOFT_VSYNC_NS 私常量废除
//! 合成 ts_ns；已 parked 再来 = ST_BUSY）、PREPARE 槽记账 + 空置 fence、
//! COMMIT 记账、PANEL_CAPS 应答档位表与当前档——deadline 到点 = 应答
//! parked + 信号 event（`1<<32` 用户
//! 半位——客户端同位等待契约，bsh/bench1 家族默认）。console 路**恒同步
//! 直写**不变（FULL 纯记账：console_full_n++ + 共享 present_seq 谱系，
//! 不占 credits 不发 token——误入 vsync 排队呈现 = 文本路 +16.67ms/
//! 队深打满/屏显冻结，立法禁入）。首个 gfx 帧呈现一次性 `GFXF ok n=`
//! （∧ 抽样全过 ∧ console_full_n≥1——FBW ok 回流自身 pipe 的结构保证）。
//! 全部服务状态**静态定容**（token 环/held 表/hdi 槽表/行弹舱——guifbd
//! 是 berry-libc musl 程序有堆，静态定容 = 工程选择非纪律）；**零唤醒
//! 不变量**：无未决请求 = 无主动 send（IDLE 面结构先行——ADR #53，能耗
//! 代理断言挂 GUI-6）。

use berry_exec::FB_VA;
use berry_fbcon::font::{self, GLYPH_H};
use berry_fbcon::render::{Fbcon, PixelFormat};
use berry_guiproto as gp;
use berry_libc::io::{gift2_h, gift3_h, hdi_srv_h, present_srv_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_event_create,
    berry_handle_close, berry_object_signal, berry_object_wait_many, berry_object_wait_one,
    berry_vmo_get_size, berry_vmo_read, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
};

// 链接种子（fsmgr 同款）：rlib/LTO 按需保留——无引用则 berry-libc 的 crt0
// （含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

// —— 段③ 常量（软 vsync / serve 帽 / 信号位 / 定容帽）——

/// boot 率域解析（活设备半件——刷新率体系规范 §13「按概念迁移不按文件
/// 形态」）：档位周期表单源已迁 `gp::softpanel::TIERS`（刀2，host 可测
/// 纯核），本侧只留**语义层校验落值 + loud 回退**：Tiered 表内直取，
/// 表外/0 回退 60 + 诊断行（规范 §4-1；旧内核 48B 零页域同路回退）；
/// FreeRange 钳 1..=480 后单点换算（0 同归 60——缺省语义不分模型）。
fn boot_panel_tier(cfg: &gp::softpanel::PanelCfg, boot_hz: u16) -> (u16, u64) {
    let hz0 = if boot_hz == 0 { 60 } else { boot_hz };
    match cfg.model {
        gp::RATE_MODEL_FREERANGE => {
            let q = gp::softpanel::freerange_quantize(hz0);
            (q, gp::softpanel::period_of(q))
        }
        _ => match gp::softpanel::resolve(hz0) {
            Some((hz, p)) => (hz, p),
            None => {
                eprintln!("guifbd: panel hz={} not in tiers, fallback 60", hz0);
                (60, gp::softpanel::period_of(60))
            }
        },
    }
}
/// serve 空闲并等帽：wait_many deadline 外层上界（合法空闲非死线——到点
/// 重臂续等；真断链面 = recv 非 -EAGAIN 错码，照 trap）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;
/// 应答 send -EBUSY 重臂帽（gui-inputd `SEND_REARM_NS` 同款）。
const SEND_REARM_NS: u64 = 10 * 1_000_000_000;
/// vblank event 信号位（用户半默认位——跨进程契约：客户端 REG_VBLANK 前
/// 自建 event 并在同位等待；bsh/bench1 家族先例，不登记内核
/// USER_SIGNAL_BITS——用户私有位无内核模板面）。
const VBLANK_SIG: u64 = 1 << 32;
/// token 环深（FRAME_CALLBACK 校验窗口——单客户端 v1 足够覆盖在途 8 帧）。
const TOKEN_RING: usize = 8;
/// 行弹舱帽（= ramfb stride 800×4；启动门执法——越帽说明几何非 ramfb
/// 基线，blit 行弹越界 = 未定义行为，宁可不起服务）。
// 行弹舱：≥ 1280×4（16:9 桌面）；1920×4 同帽可容。
const ROW_BOUNCE_MAX: usize = 16384;
/// SUBMIT 抽样点数（全落 damage 并集内——A5-b）。
const SAMPLE_N: usize = 7;
/// present 域 credits 帽（= [`gp::CREDITS_MAX`]，usize 面给 held 定容）。
const CREDITS: usize = gp::CREDITS_MAX as usize;
/// hdi 域槽帽（= [`gp::HDI_SLOTS_MAX`]，usize 面给槽表定容）。
const SLOTS: usize = gp::HDI_SLOTS_MAX as usize;
/// A8 内容节拍窗（呈现面折稳态内容 hz 的窗口——500ms 平衡响应与稳态判据）。
const CC_WINDOW_NS: u64 = 500 * 1_000_000;
/// A8 成窗最少帧数（<3 帧 = 窗太稀不折——防间歇抖动误报内容率）。
const CC_MIN_N: u32 = 3;
/// console 文本色（自验收的 bg1 黑/白字延续——段② 语义零变化）。
const CON_FG: (u8, u8, u8) = (0xFF, 0xFF, 0xFF);
const CON_BG: (u8, u8, u8) = (0x00, 0x00, 0x00);

/// 抽样读单像素（界内坐标）——裸指针 `read_volatile`，见模块头（避开
/// Fbcon 的 slice 借用）。
fn px(stride: usize, x: usize, y: usize) -> u32 {
    let off = y * stride + x * 4;
    // SAFETY: FB_VA 映射幅 = ceil_page(stride×h)（boot create_mmio 双页
    // 对齐执法）；调用点全部来自 boot info VMO 的几何界内坐标。
    unsafe { core::ptr::read_volatile((FB_VA as usize + off) as *const u32) }
}

/// 蜂格全对拍：字形 GLYPH_H×8 位图逐像素（bit = glyph[y] & (0x80>>x)，
/// MSB=最左列）——期望值与 Fbcon::put_char 同一编码路径（pack(fg/bg)）。
fn cell_ok(stride: usize, fmt: PixelFormat, col: usize, row: usize, ch: u8,
           fg: (u8, u8, u8), bg: (u8, u8, u8)) -> bool {
    let g = match font::glyph(ch) {
        Some(g) => g,
        None => return false,
    };
    for y in 0..GLYPH_H {
        for x in 0..8 {
            let want = if g[y] & (0x80 >> x) != 0 {
                fmt.pack(fg.0, fg.1, fg.2)
            } else {
                fmt.pack(bg.0, bg.1, bg.2)
            };
            if px(stride, col * 8 + x, row * GLYPH_H + y) != want {
                return false;
            }
        }
    }
    true
}

/// 单调钟现值（失败 = clock 面断链，loud）。
fn now_ns() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}

/// 应答 send（无随行 handle 面；EBUSY → wait WRITABLE 重臂重验，绝不
/// 空转——gui-inputd `send_rep` 同款）。
fn send_rep(h: Handle, rep: &[u8]) {
    loop {
        let s = berry_channel_send(h, rep.as_ptr(), rep.len(), core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            println!("guifbd: send 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, now_ns() + SEND_REARM_NS, &mut obs);
        // 到点未通走下一轮 send 重验（obs 不作判据——重验循环兜底）
    }
}

/// 应答 send（随行 handle 面——RELEASE 归还别名 / PREPARE 递 fence：
/// handle 随应答迁回客户端表）。
fn send_rep_handles(h: Handle, rep: &[u8], hs: &[Handle]) {
    loop {
        let s = berry_channel_send(h, rep.as_ptr(), rep.len(), hs.as_ptr(), hs.len());
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            println!("guifbd: send 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, now_ns() + SEND_REARM_NS, &mut obs);
    }
}

/// 1B 状态应答（非 OK 烘焙线 + VOTE/POWER_MODE/REG_VBLANK/COMMIT 成功形）。
fn st_rep(h: Handle, st: u8) {
    let mut out = [0u8; 1];
    let n = gp::encode_status_rep(&mut out, st);
    send_rep(h, &out[..n]);
}

/// close 请求随行迁入的全部 handle（拒收公共出口——坏客户端不得杀服务，
/// close 失败忽略）。
fn close_all(hs: &[Handle]) {
    for &h in hs {
        let _ = berry_handle_close(h);
    }
}

/// 刀3 决策源证据行标（小写——串口可 grep 面）。
fn arb_src_label(s: gp::arbiter::Src) -> &'static str {
    match s {
        gp::arbiter::Src::Vote => "vote",
        gp::arbiter::Src::Dwell => "dwell",
        gp::arbiter::Src::Boost => "boost",
        gp::arbiter::Src::Dim => "dim",
        gp::arbiter::Src::Platform => "platform",
        gp::arbiter::Src::Fallback => "fallback",
        gp::arbiter::Src::Lfc => "lfc",
    }
}

/// hdi PREPARE 槽（记账面——v1 COMMIT 不触碰 VMO 内容，别名只存不读）。
#[derive(Clone, Copy)]
struct Slot {
    vmo: Handle, // 0 = 空槽
    w: u32,
    h: u32,
    stride: u32,
    /// 最近 COMMIT target（0 = 尽快）——纯记账，排队呈现留异步后端刀。
    commit_target: u64,
}

/// 刀6 P2 在途帧（深度 1 管线——SUBMIT 记帧、latch 拍呈现；spec §7-P2：
/// 帧呈现于 ≤ target_present_ns 的最大拍点，latch 已过 = 即时呈现的迟到
/// 诚实形）。`token == 0` = P5 COMMIT 面（无回执域——COMMIT wire 无 token
/// 位，呈现证据 = vblank 拍 + 一次性 `P5C ok` 行）。
struct PendingFrame {
    token: u32,
    /// SUBMIT 随行 buffer 别名（已入 held——latch 呈现后归 RELEASE 面）/
    /// P5 面 = 槽 VMO（服务端持有，不入 held；别名生命周期挂槽，blit
    /// 失败不 close）。
    vmo: Handle,
    full: bool,
    /// damage 快照（rects 从 SUBMIT 消息拷出——消息缓冲复用）。
    rects: [gp::Rect; gp::DAMAGE_MAX],
    rect_count: usize,
    /// latch 拍点（绝对 ns；serve deadline min 臂 + 到点即呈现）。
    latch_ns: u64,
    /// UNSQUASHABLE 位（新 P2 到席时不可压弃——ST_BUSY 挡新帧）。
    unsquashable: bool,
}

/// 段③ 服务状态（全静态定容——见模块头⑧注；无堆分配、无生长面）。
struct Srv<'a> {
    con: Fbcon<'a>,
    w: usize,
    h: usize,
    stride: usize,
    // —— present 域 ——
    /// 呈现谱系序（SUBMIT 与 console **共享抬高**——FRAME_CALLBACK 应答
    /// 禁断言精确值，A2-b）。
    present_seq: u32,
    /// token 单调源（1 起；0 = 环槽空哨兵）。
    submit_seq: u32,
    /// token 环（刀4 P3：呈现回执五元组覆盖最旧——FRAME_CALLBACK 与
    /// FRAME_TIMING 两查询面共源；`frame_token == 0` = 环槽空哨兵）。
    tokens: [gp::FrameTimingRep; TOKEN_RING],
    /// A10 预告锚：最近一次非零 `target_present_ns`（0 = 未见——间隔密
    /// 集判定只在连续非零目标间做）。
    last_hint_target_ns: u64,
    /// 在途别名表（credits = len；RELEASE 全量归还清零）。
    held: [(u32, Handle); CREDITS],
    held_n: usize,
    /// 刀3 仲裁器（§6：票簿 + A 规则裁决纯核——panel 是执行面，arb 是决
    /// 策面；VOTE/POWER_MODE 旧记账字段收编于此）。
    arb: gp::arbiter::Arbiter,
    // —— hdi 域（软 vsync）——
    /// 软面板拍点状态机（刀2 §5：FREE 零扰动 / DET 虚拟钟 / 故障旋钮；
    /// hz·period·模式·点序单源在此——旧 panel_hz/panel_period_ns/vb_seq
    /// 三字段收编）。
    panel: gp::softpanel::SoftPanel,
    /// 下次软 vsync 点（`vb_scheduled` 时有效；调度面值——DET 态上报
    /// ts 走虚拟钟，此值只管 serve 环何时醒）。
    vb_next_ns: u64,
    /// 有点在案（REG_VBLANK 使能 ∧/∨ parked wait——wait_many deadline
    /// 的 min 臂来源）。
    vb_scheduled: bool,
    vb_armed: bool,
    /// REG_VBLANK 存量 event（0 = 无）。
    vb_ev: Handle,
    vb_parked: bool,
    /// parked VBLANK_WAIT 的应答通道。
    vb_park_ch: Handle,
    /// parked 形态：false = VBLANK_WAIT 16B（GOLDEN 钉板面）/ true =
    /// OP_VBLANK_TIMING 24B 三元组（刀2 事件面——两 op 共用单车位）。
    vb_park_timing: bool,
    slots: [Slot; SLOTS],
    // —— 刀6 P2/P5/P6 呈现管线 ——
    /// 在途帧（深度 1；P1 即时呈现 = latch 已过的退化特例——同管线同码）。
    pending: Option<PendingFrame>,
    /// parked FRAME_CALLBACK（P2：应答须待 latch 呈现——单客户端单车位）。
    cb_park: Option<Handle>,
    cb_park_token: u32,
    /// parked RELEASE_WAIT（P2：在途帧持别名未呈现前不归还——防 latch 前
    /// 客户端覆写缓冲撕裂；呈现后随应答全量归还）。
    rel_park: Option<Handle>,
    /// P6 最近扫描墙钟（0 = 未扫描——自刷新重扫窗锚）。
    p6_last_scan_ns: u64,
    /// A8 内容节拍窗锚（0 = 窗未开；呈现面量稳态内容 hz）。
    cc_start_ns: u64,
    cc_n: u32,
    /// A8 上一已报 k（变更才打证据行；u32::MAX = 未见过）。
    cc_last_k: u32,
    /// P5 首呈现一次性证据行位。
    p5c_seen: bool,
    // —— 标记 / console 记账 ——
    served: bool,
    gfxf: bool,
    console_full_n: u64,
    /// GUI 模式：console 只记账/串口，不上屏（防测试/日志盖桌面）。
    gui: bool,
    /// SUBMIT 行弹舱（vmo_read 行读出 → 直写 fb；帽 = ROW_BOUNCE_MAX）。
    bounce: [u8; ROW_BOUNCE_MAX],
}

impl<'a> Srv<'a> {
    fn new(
        con: Fbcon<'a>,
        w: usize,
        h: usize,
        stride: usize,
        gui: bool,
        cfg: gp::softpanel::PanelCfg,
        boot_hz: u16,
    ) -> Self {
        // 语义层校验/回退先走（loud 诊断行在 boot_panel_tier），SoftPanel
        // 构造内自查表落周期——period 首值不外传；仲裁器同 boot 档起（A11
        // 无票缺省 = 平台表达档）。
        let (hz, _) = boot_panel_tier(&cfg, boot_hz);
        let arb = gp::arbiter::Arbiter::new(&cfg, hz);
        Srv {
            con,
            w,
            h,
            stride,
            present_seq: 0,
            submit_seq: 0,
            tokens: [gp::FrameTimingRep {
                frame_token: 0,
                presented_ns: 0,
                seq: 0,
                refresh_ns: 0,
                flags: 0,
            }; TOKEN_RING],
            last_hint_target_ns: 0,
            held: [(0, 0); CREDITS],
            held_n: 0,
            arb,
            panel: gp::softpanel::SoftPanel::new(cfg, hz),
            vb_next_ns: 0,
            vb_scheduled: false,
            vb_armed: false,
            vb_ev: 0,
            vb_parked: false,
            vb_park_ch: 0,
            vb_park_timing: false,
            slots: [Slot { vmo: 0, w: 0, h: 0, stride: 0, commit_target: 0 }; SLOTS],
            pending: None,
            cb_park: None,
            cb_park_token: 0,
            rel_park: None,
            p6_last_scan_ns: 0,
            cc_start_ns: 0,
            cc_n: 0,
            cc_last_k: u32::MAX,
            p5c_seen: false,
            served: false,
            gfxf: false,
            console_full_n: 0,
            gui,
            bounce: [0; ROW_BOUNCE_MAX],
        }
    }

    /// 软 vsync 排点（无点在案时；REG_VBLANK 使能 / parked wait 共用）。
    /// per_frame 形态 no-op：拍 = 扫描（latch/p6_rescan），周期点名不排
    /// ——vb_scheduled/vb_next_ns 仅周期形态有意义（误排不撤 = deadline
    /// 挂过去点自旋）。
    fn vsync_schedule(&mut self) {
        if self.panel.per_frame() {
            return;
        }
        if !self.vb_scheduled {
            self.vb_next_ns = now_ns() + self.panel.period_ns();
            self.vb_scheduled = true;
        }
    }

    /// 软 vsync 到点检查（每轮回环顶）：拍点过软面板裁决（刀2 §5 事件面）
    /// ——投递点应答 parked wait（16B/24B 按停车形态）+ 信号 REG_VBLANK
    /// event；丢帧 = 不投递（parked 留座、不信号——面板拍点在、seq 消耗，
    /// 客户端见跳号 = 诚实丢帧信号）。FREE 态名义点追赶跳过已过点（不
    /// 补偿积压 vsync，跳过点不耗 seq——现状不变量）；DET 态每醒恰一点
    ///（虚拟钟墙钟无关，追赶会引墙钟依赖破 T2 回放）。armed∧parked∧在途
    /// 帧三空后撤点（零唤醒不变量回位——P2 在途帧须拍驱动）。
    fn tick_vsync(&mut self) {
        if self.panel.per_frame() {
            self.p6_rescan();
            return;
        }
        if !self.vb_scheduled {
            return;
        }
        let t = now_ns();
        if t < self.vb_next_ns {
            return;
        }
        let point = self.vb_next_ns; // 名义点 = FREE 合成 ts（clock 口径）
        if let Some(tick) = self.panel.on_point(point) {
            self.deliver_tick(&tick);
        }
        let p = self.panel.period_ns();
        let mut next = if self.panel.catchup() {
            point + p
        } else {
            t + p
        };
        if self.panel.catchup() {
            while next <= t {
                next += p;
            }
        }
        self.vb_next_ns = next;
        // 刀3 拍后裁决（A5 streak 随拍积；armed/parked/在途全空的末拍 =
        // quiet=true → A7 停帧判拍；boost 窗内禁停——quiet 汇拢面）。
        let d = self.arb.on_tick(point, self.quiet_now(point));
        self.arb_exec(d);
        if !self.vb_armed && !self.vb_parked && self.pending.is_none() {
            self.vb_scheduled = false;
        }
    }

    /// 拍点投递共尾（周期拍 / P6 扫描拍 / 自刷新重扫共尾）：应答 parked
    /// wait（16B/24B 按停车形态）+ 信号 REG_VBLANK event。
    fn deliver_tick(&mut self, tick: &gp::VblankTimingRep) {
        if self.vb_parked {
            // 停车形态分应答：16B GOLDEN 面（VBLANK_WAIT）/ 24B 三元组
            //（OP_VBLANK_TIMING）——同一拍点同一载荷两形。
            let mut out = [0u8; 24];
            let n = if self.vb_park_timing {
                gp::encode_vblank_timing_rep(&mut out, tick)
            } else {
                gp::encode_vblank_rep(
                    &mut out,
                    &gp::VblankRep { seq: tick.seq as u32, ts_ns: tick.ts_ns },
                )
            };
            send_rep(self.vb_park_ch, &out[..n]);
            self.vb_parked = false;
        }
        if self.vb_armed {
            // 持有客户端 event 的迁移别名（SIGNAL 权随行）——脉冲形
            // clear+set 同位：周期信号每点重置可观察。
            let _ = berry_object_signal(self.vb_ev, VBLANK_SIG, VBLANK_SIG);
        }
    }

    /// P6 自刷新重扫：per_frame 形态下听众在席（armed/parked）而无一帧
    /// 内容时，max_interval 窗（1Hz）到点重扫维持 tick 流——真实自刷新
    /// 硬件不可观测，仿真面仅保 vblank 订阅者不断流；零听众不排（零唤醒
    /// 优先）。周期拍路径（tick_vsync 主体）在 per_frame 形态下不排点——
    /// 拍 = 扫描（latch_fire / 此处），节奏逐帧随内容。
    fn p6_rescan(&mut self) {
        if self.p6_last_scan_ns == 0 || (!self.vb_armed && !self.vb_parked) {
            return;
        }
        let t = now_ns();
        let rescan_at = self.p6_last_scan_ns.saturating_add(self.panel.max_interval_ns());
        if t < rescan_at {
            return;
        }
        // ts = 名义重扫点（自刷新硬件按自身节拍，宿主迟到醒只是观察晚——
        // 与周期拍名义点同语义）：墙钟直通会把 K1 唤醒尾（p99≈16.7ms）灌
        // 进 park 残差统计（k4t 2ms / k6h 高刷门在 per_frame 形态假红）。
        // p6_last_scan_ns 同步推进名义值 → 重扫流 = 恒 1Hz 网格。
        if let Some(tick) = self.panel.on_scan(rescan_at) {
            self.deliver_tick(&tick);
            let d = self.arb.on_tick(tick.ts_ns, self.quiet_now(tick.ts_ns));
            self.arb_exec(d);
        }
        self.p6_last_scan_ns = rescan_at;
    }

    /// P2 latch 拍点（§7-P2）：≤ target 的最大网格点（网格锚 = 下一点
    /// `vb_next_ns`、间距 = 当前面板周期）。latch ≤ now = 迟到（即时呈现
    /// 的退化特例）；target 跨多档取最远不外推网格。
    fn latch_point(&self, target_ns: u64) -> u64 {
        let p = self.panel.period_ns();
        let mut t = self.vb_next_ns;
        while t > target_ns {
            t -= p; // 网格单调下探（target ≥ 0 恒可止）
        }
        while t + p <= target_ns {
            t += p; // 远期 target 对齐最近网格（不外推不越界）
        }
        t
    }

    /// P2 在途帧到点呈现（每轮回环顶，serve deadline min 臂保证最迟本拍
    /// 醒）：per_frame 形态先扫描（拍 = 扫描，ts 走 DET 虚拟钟）再投递；
    /// 周期形态呈现拍序 = 下一将拍点（与 vblank 同域，T6 一号到底）。
    fn tick_latch(&mut self) {
        let Some(pf) = self.pending.as_ref() else { return };
        let now = now_ns();
        if now < pf.latch_ns {
            return;
        }
        self.latch_fire(now);
    }

    /// 在途帧 latch 拍（serve deadline 臂面——None = 无在途）。
    fn pending_ref(&self) -> Option<u64> {
        self.pending.as_ref().map(|p| p.latch_ns)
    }

    /// 呈现时戳裁决（latch 呈现 / 即时呈现共尾）：per_frame = 扫描即拍
    ///（ts = DET 虚拟钟 / FREE 直通；丢帧旋钮吞 tick 时呈现仍发生——seq
    /// 已耗，ts 退化 now）；周期形态 = 呈现拍序 = 下一将拍点（与 vblank
    /// 同域，T6 一号到底）。
    fn present_stamp(&mut self, now: u64) -> (u64, u64) {
        if self.panel.per_frame() {
            self.p6_last_scan_ns = now;
            match self.panel.on_scan(now) {
                Some(tick) => {
                    self.deliver_tick(&tick);
                    let d = self.arb.on_tick(tick.ts_ns, self.quiet_now(tick.ts_ns));
                    self.arb_exec(d);
                    (tick.ts_ns, tick.seq)
                }
                None => (now, self.panel.seq_now()),
            }
        } else {
            (now, self.panel.next_seq())
        }
    }

    /// 在途帧呈现本体：blit + 抽样 + 回执环 + parked 应答（callback /
    /// release）+ A8 内容节拍窗。blit 读失败 = loud + 弃帧（不 trap——
    /// 单客户端槽复用竞态是已知面，诚实丢弃好过杀服务）。
    fn latch_fire(&mut self, now: u64) {
        let pf = self.pending.take().unwrap();
        let (presented_ns, seq) = self.present_stamp(now);
        let fullrect = gp::Rect { x: 0, y: 0, w: self.w as u32, h: self.h as u32 };
        let targets: &[gp::Rect] = if pf.full {
            core::slice::from_ref(&fullrect)
        } else {
            &pf.rects[..pf.rect_count]
        };
        let (read_ok, all_pass, n_pass) = self.blit_and_sample(pf.vmo, targets);
        if !read_ok {
            // 读失败 = loud + 诚实弃帧（不 trap——单客户端槽复用竞态是已
            // 知面）：环记 DISCARDED、parked callback 答弃、parked release
            // 归还（别名留 held 由 release 面迁回），谱系不动。
            eprintln!("guifbd: latch blit fail drop token={}", pf.token);
            if pf.token != 0 {
                let rec = gp::FrameTimingRep {
                    frame_token: pf.token,
                    presented_ns: 0,
                    seq: 0,
                    refresh_ns: self.panel.period_ns() as u32,
                    flags: gp::FRAME_TIMING_F_DISCARDED
                        | if pf.full { gp::FRAME_TIMING_F_FULL } else { 0 },
                };
                self.tokens[pf.token as usize % TOKEN_RING] = rec;
                if self.cb_park.is_some() && self.cb_park_token == pf.token {
                    self.answer_cb_discarded();
                }
            }
            if self.rel_park.is_some() {
                self.answer_release();
            }
            return;
        }
        if pf.token != 0 {
            self.present_seq = self.present_seq.wrapping_add(1);
            let rec = gp::FrameTimingRep {
                frame_token: pf.token,
                presented_ns,
                seq,
                refresh_ns: self.panel.period_ns() as u32,
                flags: gp::FRAME_TIMING_F_PRESENTED
                    | if pf.full { gp::FRAME_TIMING_F_FULL } else { 0 },
            };
            self.tokens[pf.token as usize % TOKEN_RING] = rec;
            // parked FRAME_CALLBACK 同 token 即应答（token 异 park = 留座
            // 待其自帧——单客户端时序上不可达，防御面）。
            if self.cb_park.is_some() && self.cb_park_token == pf.token {
                self.answer_cb(presented_ns);
            }
        } else if !self.p5c_seen {
            self.p5c_seen = true;
            println!("P5C ok latch={}", presented_ns);
        }
        self.gfxf_check(all_pass, n_pass);
        // A8 内容节拍窗（per_frame 形态 LFC 天然内含——不走裁决）。
        self.cc_track(presented_ns);
        // parked RELEASE_WAIT：在途已清——全量归还。
        if self.rel_park.is_some() {
            self.answer_release();
        }
    }

    /// 在途帧压弃（新 P2 到席 / UNSQUASHABLE 除外）：回执环记 DISCARDED、
    /// parked callback 同 token 即答 DISCARDED；别名留 held 归 RELEASE 面。
    fn squash_pending(&mut self) {
        let Some(old) = self.pending.take() else { return };
        if old.token != 0 {
            let rec = gp::FrameTimingRep {
                frame_token: old.token,
                presented_ns: 0,
                seq: 0,
                refresh_ns: self.panel.period_ns() as u32,
                flags: gp::FRAME_TIMING_F_DISCARDED
                    | if old.full { gp::FRAME_TIMING_F_FULL } else { 0 },
            };
            self.tokens[old.token as usize % TOKEN_RING] = rec;
            if self.cb_park.is_some() && self.cb_park_token == old.token {
                self.answer_cb_discarded();
            }
        }
        println!("guifbd: p2 squash token={}", old.token);
    }

    /// parked FRAME_CALLBACK 应答（呈现形）。
    fn answer_cb(&mut self, presented_ns: u64) {
        if let Some(ch) = self.cb_park.take() {
            let rep = gp::FrameDoneRep {
                status: gp::FRAME_PRESENTED,
                credits: (CREDITS - self.held_n) as u8,
                present_seq: self.present_seq,
                presented_ns,
            };
            let mut out = [0u8; 16];
            let n = gp::encode_frame_done_rep(&mut out, &rep);
            send_rep(ch, &out[..n]);
        }
    }

    /// parked FRAME_CALLBACK 应答（压弃形——FRAME_DISCARDED）。
    fn answer_cb_discarded(&mut self) {
        if let Some(ch) = self.cb_park.take() {
            let rep = gp::FrameDoneRep {
                status: gp::FRAME_DISCARDED,
                credits: (CREDITS - self.held_n) as u8,
                present_seq: self.present_seq,
                presented_ns: 0,
            };
            let mut out = [0u8; 16];
            let n = gp::encode_frame_done_rep(&mut out, &rep);
            send_rep(ch, &out[..n]);
        }
    }

    /// parked RELEASE_WAIT 应答（在途已清——全量归还；与即时面同码共尾）。
    fn answer_release(&mut self) {
        if let Some(ch) = self.rel_park.take() {
            let k = self.held_n;
            let mut ids = [0u32; gp::RELEASE_MAX];
            let mut hs_out = [0u32; gp::RELEASE_MAX];
            for i in 0..k {
                ids[i] = self.held[i].0;
                hs_out[i] = self.held[i].1;
            }
            self.held_n = 0;
            let mut out = [0u8; 4 + 4 * gp::RELEASE_MAX];
            match gp::encode_release_rep(&mut out, &ids[..k]) {
                Some(n) => send_rep_handles(ch, &out[..n], &hs_out[..k]),
                None => trap(), // k ≤ 2 恒 ≤ 帽；防御面
            }
        }
    }

    /// A8 内容节拍窗（呈现面量稳态内容 hz——窗口 500ms 最少 3 帧成窗）：
    /// 窗满折 hz 入仲裁（on_content_cadence；per_frame 形态天然内含不走
    /// 裁决）+ k 变更/lfc-off 证据行（读即清）。
    fn cc_track(&mut self, presented_ns: u64) {
        if self.cc_start_ns == 0 {
            self.cc_start_ns = presented_ns;
            self.cc_n = 0;
        }
        self.cc_n += 1;
        let span = presented_ns.saturating_sub(self.cc_start_ns);
        if span < CC_WINDOW_NS || self.cc_n < CC_MIN_N {
            return;
        }
        // 帧距均值折 hz（n 帧间隔数 = n-1；u64 域防溢）。
        let hz = ((self.cc_n as u64 - 1) * 1_000_000_000 / span) as u16;
        if !self.panel.per_frame() {
            let d = self.arb.on_content_cadence(hz);
            self.arb_exec(d);
            if self.arb.take_warn_lfc_off() {
                println!("PANELARB lfc-off");
            }
            let k = self.arb.lfc_k();
            if k != self.cc_last_k {
                self.cc_last_k = k;
                println!("PANELARB lfc k={}", k);
            }
        }
        // 窗滚动（新窗自本帧起）。
        self.cc_start_ns = presented_ns;
        self.cc_n = 1;
    }

    /// parked wait 入座（VBLANK_WAIT / OP_VBLANK_TIMING 共用单车位）：
    /// 随行 handle 拒面、已停 ST_BUSY（A3-d③）、无即刻应答——tick_vsync
    /// 到点按 `timing` 分 16B/24B 应答（deadline min 臂保证最迟一个空闲
    /// 帽内到点）。
    fn park_vblank(&mut self, ch: Handle, hs: &[Handle], timing: bool) {
        if !hs.is_empty() {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        if self.vb_parked {
            st_rep(ch, gp::ST_BUSY);
            return;
        }
        // 刀3 A7：等拍即活动——停帧破除（Resume 证据）先于入座排点。
        let d = self.arb.on_activity(now_ns());
        self.arb_exec(d);
        self.vb_parked = true;
        self.vb_park_ch = ch;
        self.vb_park_timing = timing;
        // P6 首听众锚（boot 后无内容零扫描——latch 未发生过）：锚 now 使
        // 自刷新重扫窗（max_interval）起算，parked 应答 ≤ 1s 保底。
        if self.panel.per_frame() && self.p6_last_scan_ns == 0 {
            self.p6_last_scan_ns = now_ns();
        }
        self.vsync_schedule();
    }

    /// A7 静默判定（armed/parked/boost 窗/在途帧四汇拢；SUBMIT/pulse 的
    /// 活动破除走 on_activity 直达面，不入此布尔——P2 在途呈现 = 活动面）。
    fn quiet_now(&self, now: u64) -> bool {
        !self.vb_armed && !self.vb_parked && !self.arb.boost_active(now) && self.pending.is_none()
    }

    /// 决策执行面（刀3 A9）：Switch → 软面板切档受理（拒 → 回滚重裁）+
    /// 排点驱动在飞 2 帧窗；StopFrame/Resume → A7 证据行（调度语义即零
    /// 唤醒基线：撤点 = 不排点；Resume 不自排点——活动源（armed/park 排
    /// 点 / pulse 的 Switch 排点）各持排点权）。证据行无 ` ok` 形态（smoke
    /// 标记互撞纪律）。
    fn arb_exec(&mut self, d: gp::arbiter::Decision) {
        if self.arb.take_warn_no_g() {
            println!("PANELARB nog");
        }
        match d {
            gp::arbiter::Decision::Stay => {}
            gp::arbiter::Decision::Switch { hz, src } => {
                if self.panel.request_refresh(hz).is_none() {
                    self.arb.on_switch_rejected(self.panel.hz());
                } else {
                    self.vsync_schedule(); // 在飞 2 帧窗须拍驱动（armed/parked 可双空）
                    println!("PANELARB sw hz={} src={}", hz, arb_src_label(src));
                }
            }
            gp::arbiter::Decision::StopFrame => println!("PANELARB stop"),
            gp::arbiter::Decision::Resume => println!("PANELARB resume"),
        }
    }

    /// serve 时钟醒检查（回环顶）：仲裁 next_wake 到点 → on_time 无拍评
    /// 估（降档驻留 / boost 到期推进——停帧态恒 None 零醒）。
    fn arb_on_time(&mut self) {
        let now = now_ns();
        if let Some(w) = self.arb.next_wake(now) {
            if now >= w {
                let d = self.arb.on_time(now, self.quiet_now(now));
                self.arb_exec(d);
            }
        }
    }

    /// present 域轮询（一路至多一条：严格串行 RPC；返回是否处理过消息）。
    /// **每轮 poll 前重置 nb/nh 容量**（A1-b——recv 出参 in-out，漏重置 =
    /// 容量 0 恒 -EINVAL 死循环）；出参 8 槽（SUBMIT 随行 VMO 别名迁入）。
    fn poll_present(&mut self, ch: Handle) -> bool {
        let mut msg = [0u8; 256];
        let mut hout = [0u32; 8];
        let mut nb = msg.len();
        let mut nh = hout.len();
        let s = berry_channel_recv(
            ch,
            msg.as_mut_ptr(),
            &mut nb,
            hout.as_mut_ptr(),
            &mut nh,
        );
        if s == BERRY_ERR_AGAIN {
            return false;
        }
        if s != 0 {
            trap(); // 非 -EAGAIN 错码 = 通道链断（boot 定值接线，真死亡面）
        }
        self.handle_present(ch, &msg[..nb], &hout[..nh]);
        true
    }

    /// hdi 域轮询（同上；出参 8 槽——REG_VBLANK event / PREPARE VMO 迁入）。
    fn poll_hdi(&mut self, ch: Handle) -> bool {
        let mut msg = [0u8; 256];
        let mut hout = [0u32; 8];
        let mut nb = msg.len();
        let mut nh = hout.len();
        let s = berry_channel_recv(
            ch,
            msg.as_mut_ptr(),
            &mut nb,
            hout.as_mut_ptr(),
            &mut nh,
        );
        if s == BERRY_ERR_AGAIN {
            return false;
        }
        if s != 0 {
            trap();
        }
        self.handle_hdi(ch, &msg[..nb], &hout[..nh]);
        true
    }

    /// console 路轮询（段② 语义 + FULL 记账升格）。出参恒 null：svcmgr
    /// 转发端零随行 face（若有 handle 进来 = 容量不足保原样挂队，不丢——
    /// recv 语义，非静默丢弃面）。
    fn poll_console(&mut self, ch: Handle) -> bool {
        let mut msg = [0u8; 256];
        let mut nb = msg.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(ch, msg.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == BERRY_ERR_AGAIN {
            return false;
        }
        if s != 0 {
            trap();
        }
        if !self.served {
            self.served = true;
            // 首条消息标记（状态谓词非竞速：矩阵打印持续不断 → 首条必达；
            // 标记本身经 stdout 回 console → 串口 + 回扇 fb = 自证闭环单
            // 发，无风暴——put_str 上屏不再回发）。
            println!("FBSERVE ok");
        }
        // 上屏：字节流 put_str（自维护行缓冲/换行滚动/行尾回绕在 Fbcon——
        // '\n'→换行、col==cols 回绕、越底自动 scroll_up）。串口字节流非保证
        // UTF-8，lossy 容错（无效序列 U+FFFD——显示面语义，串口主路不受
        // 影响）。**恒同步直写**（A2-a 立法）：console 文本路禁入任何
        // vsync 排队呈现路径——误读形 = 文本路 +16.67ms/队深打满/屏显冻结。
        let s = String::from_utf8_lossy(&msg[..nb]);
        if !self.gui {
            self.con.put_str(&s, CON_FG, CON_BG);
        }
        // FULL 纯记账：每条消息语义 = 一次 submit(FULL) 走同一呈现谱系
        // （「无 damage 不提交」day-one 执法面）——console 不占 credits、
        // 不发 token、不入 held（R2 A2-a：同步直写与记账并行不悖）。
        self.console_full_n += 1;
        self.present_seq = self.present_seq.wrapping_add(1);
        true
    }

    /// present 域分发：七 op（刀4 起 + FRAME_TIMING）；一切拒面 close 随行
    /// handle 后 1B 应答（A3-b——send 即 take，不 close 即泄漏）。
    fn handle_present(&mut self, ch: Handle, msg: &[u8], hs: &[Handle]) {
        match gp::parse_present_req(msg) {
            Some(gp::PresentReq::Getinfo) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // ver 2/1/3（wire 域版本——非本服务覆盖面；present v2 =
                // 刀4 OP_FRAME_TIMING，hdi v3 = 刀1 OP_PANEL_CAPS——刷新率
                // 体系规范 §4-4）；格式面 v1
                // 单格式：32bpp RGBX 族（ramfb Bgrx=1，Rgbx 同族）→
                // FMT_XRGB8888 位——R/B 通道序 v1 wire 不建模（gfxdemo
                // 同源 Bgrx 构造，直拷零转换）；max_credits = 帽真值。
                let rep = gp::GetinfoRep {
                    version_present: gp::VER_PRESENT,
                    version_input: 1,
                    version_hdi: gp::VER_HDI,
                    format_bits: gp::FMT_XRGB8888,
                    display_w: self.w as u32,
                    display_h: self.h as u32,
                    max_credits: gp::CREDITS_MAX,
                };
                let mut out = [0u8; gp::PRESENT_REP_MAX];
                let n = gp::encode_getinfo_rep(&mut out, &rep);
                send_rep(ch, &out[..n]);
            }
            Some(gp::PresentReq::Submit {
                surface_id: _,
                buffer_id,
                flags,
                rate_hint,
                target_present_ns,
                rects,
                rect_count,
            }) => {
                self.submit(
                    ch,
                    hs,
                    buffer_id,
                    flags,
                    rate_hint,
                    target_present_ns,
                    &rects,
                    rect_count,
                );
            }
            Some(gp::PresentReq::FrameCallback { frame_token }) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // token 环校验（8 槽覆盖最旧；0 哨兵）；未命中 → ST_NOENT
                // 1B 烘焙线。
                let mut hit: Option<gp::FrameTimingRep> = None;
                for rec in &self.tokens {
                    if rec.frame_token == frame_token && rec.frame_token != 0 {
                        hit = Some(*rec);
                    }
                }
                match hit {
                    Some(rec) => {
                        // 环命中即已终态（PRESENTED / DISCARDED——P2 压弃
                        // 面）：按环旗分形应答，不 park。
                        // present_seq = 谱系现值——console 共享抬高，
                        // **禁断言精确值**（A2-b）。
                        let status = if rec.flags & gp::FRAME_TIMING_F_DISCARDED != 0 {
                            gp::FRAME_DISCARDED
                        } else {
                            gp::FRAME_PRESENTED
                        };
                        let rep = gp::FrameDoneRep {
                            status,
                            credits: (CREDITS - self.held_n) as u8,
                            present_seq: self.present_seq,
                            presented_ns: rec.presented_ns,
                        };
                        let mut out = [0u8; 16];
                        let n = gp::encode_frame_done_rep(&mut out, &rep);
                        send_rep(ch, &out[..n]);
                    }
                    None => {
                        // 环未命中 + 在途同 token = latch 未到——park 到呈现
                        // （latch_fire/squash 届时应答；单车位占用 → ST_BUSY
                        // ——客户端串行等一帧恒无碰撞）。
                        if self.pending.as_ref().map_or(false, |p| p.token == frame_token) {
                            if self.cb_park.is_some() {
                                st_rep(ch, gp::ST_BUSY);
                            } else {
                                self.cb_park = Some(ch);
                                self.cb_park_token = frame_token;
                            }
                        } else {
                            st_rep(ch, gp::ST_NOENT);
                        }
                    }
                }
            }
            Some(gp::PresentReq::FrameTiming { frame_token }) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // 刀4 P3：非阻塞时序真相查询——token 环共源（与 FRAME_
                // CALLBACK 同环同哨兵）；未命中 → ST_NOENT 1B（同烘焙线）。
                let mut hit: Option<gp::FrameTimingRep> = None;
                for rec in &self.tokens {
                    if rec.frame_token == frame_token && rec.frame_token != 0 {
                        hit = Some(*rec);
                    }
                }
                match hit {
                    None => st_rep(ch, gp::ST_NOENT),
                    Some(rec) => {
                        let mut out = [0u8; 28];
                        let n = gp::encode_frame_timing_rep(&mut out, &rec);
                        send_rep(ch, &out[..n]);
                    }
                }
            }
            Some(gp::PresentReq::ReleaseWait) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // P2：在途帧持别名未呈现——RELEASE 须待 latch（防 latch 前
                // buffer 重写撕帧）；park 到呈现（单车位占用 → ST_BUSY）。
                if self.pending.is_some() {
                    if self.rel_park.is_some() {
                        st_rep(ch, gp::ST_BUSY);
                    } else {
                        self.rel_park = Some(ch);
                    }
                    return;
                }
                // 帧外别名重挂：在途别名全量随应答迁回客户端（≤ CREDITS=2
                // ≤ RELEASE_MAX=4——encode 帽结构性不越）；空表 = count 0
                // 合法应答（幂等面）。
                let k = self.held_n;
                let mut ids = [0u32; gp::RELEASE_MAX];
                let mut hs_out = [0u32; gp::RELEASE_MAX];
                for i in 0..k {
                    ids[i] = self.held[i].0;
                    hs_out[i] = self.held[i].1;
                }
                self.held_n = 0;
                let mut out = [0u8; 4 + 4 * gp::RELEASE_MAX];
                match gp::encode_release_rep(&mut out, &ids[..k]) {
                    Some(n) => send_rep_handles(ch, &out[..n], &hs_out[..k]),
                    None => trap(), // k ≤ 2 恒 ≤ 帽；防御面
                }
            }
            Some(gp::PresentReq::Vote { sticky, scene, min_hz, max_hz, preferred_hz }) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // 刀3 §6：票入仲裁器（A1 入槽替换 + 即时裁决——事件不积
                // streak，升档随拍确认）。
                let now = now_ns();
                let d = self.arb.on_vote(
                    now,
                    self.quiet_now(now),
                    gp::arbiter::Ballot {
                        sticky: sticky != 0,
                        intent: scene,
                        min_hz,
                        max_hz,
                        preferred_hz,
                    },
                );
                self.arb_exec(d);
                st_rep(ch, gp::ST_OK);
            }
            Some(gp::PresentReq::PowerMode { mode }) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // 刀3 A5：POWER_DIM 置位 = 偏好降一档（仲裁器 dim 位）。
                let now = now_ns();
                let d = self.arb.on_power_mode(now, self.quiet_now(now), mode);
                self.arb_exec(d);
                st_rep(ch, gp::ST_OK);
            }
            Some(gp::PresentReq::InputPulse) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // 刀3 A6：脉冲即活动（停帧破除取证）→ boost 起臂立即升
                //（绕 streak）。任何持 present 端的观察者可发——v1 = 客户
                // 端输入环，inputd 直连挂 extras 抬帽刀。
                let now = now_ns();
                let d = self.arb.on_activity(now);
                self.arb_exec(d);
                let d = self.arb.on_input_pulse(now, false);
                self.arb_exec(d);
                st_rep(ch, gp::ST_OK);
            }
            None => {
                // 野帧（截断/尾随/未知 op）——坏客户端不得杀服务。
                close_all(hs);
                st_rep(ch, gp::ST_BAD);
            }
        }
    }

    /// SUBMIT：刀6 P2 统一管线——target_present_ns = 0 / latch 已过 =
    /// **即时呈现**（P1 blit-at-SUBMIT 降格为深度 1 特例，旧客户端零扰
    /// 动）；target 未来 = 记帧入途，latch 拍（≤ target 的最大网格点 /
    /// P6 扫描点）呈现。深度 1：新 P2 压弃旧 P2（UNSQUASHABLE 除外 →
    /// ST_BUSY）。
    fn submit(&mut self, ch: Handle, hs: &[Handle], buffer_id: u32, flags: u8,
              rate_hint: u8, target_present_ns: u64,
              rects: &[gp::Rect; gp::DAMAGE_MAX], rect_count: usize) {
        // 随行恰 1 buffer VMO 别名（rights ⊆ 源集——dup 只紧不松）。
        if hs.len() != 1 {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        let hvmo = hs[0];
        // 几何门：SUBMIT 无 stride 域 → v1 假定 buffer 与 fb 同几何同
        // stride（gfxdemo 同源构造；容量面 vmo_get_size 执法）。
        let mut sz = 0u64;
        if berry_vmo_get_size(hvmo, &mut sz) != 0 || sz < (self.stride * self.h) as u64 {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        let full = flags & gp::SUBMIT_F_DAMAGE_FULL != 0;
        // 「无 damage 不提交」两条 ST_BAD 立法（§2）：K>0∧FULL 同设 /
        // K=0∧无 FULL——wire 层不拦（合法帧），服务端拒面。
        if (rect_count > 0 && full) || (rect_count == 0 && !full) {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        // credits = 在途未 RELEASE 别名数（满 → ST_FULL 1B；拒面同样
        // close 随行别名——A3-b）。
        if self.held_n >= CREDITS {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_FULL);
            return;
        }
        // P2 压弃门：在途 UNSQUASHABLE 帧挡新帧（深度 1 不可排队——
        // ST_BUSY；拒面 close 随行）。
        if self.pending.as_ref().map_or(false, |p| p.unsquashable) {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_BUSY);
            return;
        }
        let now = now_ns();
        // latch 拍点裁决（P2/P6 分面）。
        let latch_ns = if target_present_ns == 0 {
            now // 无目标 = 尽快（P1 语义直通——latch 已过即即时）
        } else if self.panel.per_frame() {
            // P6 扫描点 = max(上一扫描 + min_interval, target)——帧就绪
            // 即上墙（首扫 p6_last_scan_ns=0 → floor= min_interval，target
            // 主导 = 逐帧锁相）。
            self.panel.scan_point(self.p6_last_scan_ns, target_present_ns)
        } else {
            self.vsync_schedule(); // latch 须拍驱动（armed/parked 可双空）
            self.latch_point(target_present_ns)
        };
        // 刀3 A7 + 刀4 A10（两域任一指示节拍快于现档 → dense——周期真值
        // 取 panel 在飞窗现行节拍）：SUBMIT 即活动 + 预告，与即时/入途
        // 分面无关。
        let d = self.arb.on_activity(now);
        self.arb_exec(d);
        let cur_hz = self.panel.hz();
        let dense = (rate_hint != 0 && (rate_hint as u16) > cur_hz)
            || (target_present_ns != 0
                && self.last_hint_target_ns != 0
                && target_present_ns > self.last_hint_target_ns
                && target_present_ns - self.last_hint_target_ns < self.panel.period_ns());
        if target_present_ns != 0 {
            self.last_hint_target_ns = target_present_ns;
        }
        let d = self.arb.on_present_hint(now, self.quiet_now(now), dense);
        self.arb_exec(d);
        self.submit_seq = self.submit_seq.wrapping_add(1);
        let token = self.submit_seq;
        if latch_ns <= now {
            // —— 即时呈现（P1 特例）：blit + 抽样 + 回执环 + 应答 ——
            let fullrect = gp::Rect { x: 0, y: 0, w: self.w as u32, h: self.h as u32 };
            let targets: &[gp::Rect] = if full {
                core::slice::from_ref(&fullrect)
            } else {
                &rects[..rect_count]
            };
            let (read_ok, all_pass, n_pass) = self.blit_and_sample(hvmo, targets);
            if !read_ok {
                let _ = berry_handle_close(hvmo);
                st_rep(ch, gp::ST_BAD);
                return;
            }
            self.present_seq = self.present_seq.wrapping_add(1);
            // 刀4 P3 五元组：呈现归属拍 = 下一将拍点全点序（panel.next_seq
            // ——与 vblank 事件 seq 同域，T6 一号到底）；refresh = 呈现时
            // 面板周期；flags = PRESENTED 恒置（即时面无弃帧）+ FULL 位。
            let rec = gp::FrameTimingRep {
                frame_token: token,
                presented_ns: now,
                seq: self.panel.next_seq(),
                refresh_ns: self.panel.period_ns() as u32,
                flags: gp::FRAME_TIMING_F_PRESENTED
                    | if full { gp::FRAME_TIMING_F_FULL } else { 0 },
            };
            self.tokens[token as usize % TOKEN_RING] = rec;
            self.held[self.held_n] = (buffer_id, hvmo);
            self.held_n += 1;
            self.gfxf_check(all_pass, n_pass);
            self.cc_track(now);
            // 8B token 应答（blit 已完成——应答即呈现，credits 剩余随
            // FRAME_CALLBACK 面）。
            let mut out = [0u8; 8];
            let n = gp::encode_submit_ack(&mut out, token);
            send_rep(ch, &out[..n]);
            return;
        }
        // —— P2 入途：压弃旧帧 + 记帧 + 入 held + 应答 token（呈现走
        // latch 面——FRAME_CALLBACK/RELEASE_WAIT 届时 parking）。
        self.squash_pending();
        let mut rcopy = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
        rcopy[..rect_count].copy_from_slice(&rects[..rect_count]);
        self.pending = Some(PendingFrame {
            token,
            vmo: hvmo,
            full,
            rects: rcopy,
            rect_count,
            latch_ns,
            unsquashable: flags & gp::SUBMIT_F_UNSQUASHABLE != 0,
        });
        self.held[self.held_n] = (buffer_id, hvmo);
        self.held_n += 1;
        let mut out = [0u8; 8];
        let n = gp::encode_submit_ack(&mut out, token);
        send_rep(ch, &out[..n]);
    }

    /// damage 裁剪 CPU blit + 7 点抽样对拍（P1 即时面与 P2 latch 呈现面
    /// 共尾）。返回 (读成败, 抽样全过, 过数)：读失败 = 致命（即时面拒帧/
    /// latch 面弃帧——调用方分面处置）；抽样失配 = 验证仪表 loud 不拦。
    fn blit_and_sample(&mut self, hvmo: Handle, targets: &[gp::Rect]) -> (bool, bool, usize) {
        for r in targets {
            let x0 = r.x as usize;
            let y0 = r.y as usize;
            let x1 = (r.x as usize + r.w as usize).min(self.w);
            let y1 = (r.y as usize + r.h as usize).min(self.h);
            if x1 <= x0 || y1 <= y0 || x0 >= self.w || y0 >= self.h {
                continue; // 界外/零面积 rect：裁剪空集跳过
            }
            let len = (x1 - x0) * 4;
            for y in y0..y1 {
                let off = (y * self.stride + x0 * 4) as u64;
                if berry_vmo_read(hvmo, self.bounce.as_mut_ptr(), off, len) != 0 {
                    eprintln!("guifbd: blit vmo_read fail @y={}", y);
                    return (false, false, 0);
                }
                // SAFETY: 目标界内（上取 min(w,h) 裁剪；映射幅 =
                // ceil_page(stride×h)——px() 同口径）；len ≤ ROW_BOUNCE_MAX
                // （x1-x0 ≤ w=800）。
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        self.bounce.as_ptr(),
                        (FB_VA as usize + off as usize) as *mut u8,
                        len,
                    );
                }
            }
        }
        // 抽样对拍（交叉核验强于复算期望——§7-⑤）：SAMPLE_N 点全落
        // damage 并集内（第 i 点取 targets[i%K]——A5-b），px() 读回 vs
        // vmo_read 同坐标。失配 = 服务端 bug → eprintln loud（标记缺失
        // = smoke FAIL），不拦应答（抽样 = 验证仪表非协议门）。
        let mut n_pass = 0usize;
        let mut all_pass = true;
        for i in 0..SAMPLE_N {
            let r = &targets[i % targets.len().max(1)];
            // 采样点 = rect 内 (i+1)/(SAMPLE_N+1) 分位（u32 乘加无溢出面：
            // rect ≤ 界内值域）；分母 = SAMPLE_N+1 ⇒ 分位 <1 恒严格内点
            //（A5-b 对任意合法几何封闭——分母勿改 SAMPLE_N 否则尾点取
            // rect 尾缘外假失配）；clamp 到 fb 界内（防御野 rect——px()
            // 界内前提）。
            let sx = ((r.x as u64 + r.w as u64 * (i as u64 + 1) / (SAMPLE_N as u64 + 1)) as usize)
                .min(self.w - 1);
            let sy = ((r.y as u64 + r.h as u64 * (i as u64 + 1) / (SAMPLE_N as u64 + 1)) as usize)
                .min(self.h - 1);
            let want = px(self.stride, sx, sy);
            let mut b4 = [0u8; 4];
            let off = (sy * self.stride + sx * 4) as u64;
            if berry_vmo_read(hvmo, b4.as_mut_ptr(), off, 4) != 0
                || u32::from_le_bytes(b4) != want
            {
                all_pass = false;
                eprintln!(
                    "guifbd: sample mismatch @{},{} got={:x} want={:x}",
                    sx,
                    sy,
                    u32::from_le_bytes(b4),
                    want
                );
            } else {
                n_pass += 1;
            }
        }
        (true, all_pass, n_pass)
    }

    /// 首帧标记共尾（∧ 抽样全过 ∧ console_full_n≥1——FBW ok 回流自身
    /// pipe 的结构保证——console 已有 FULL 记账才证明两路同屏）。
    fn gfxf_check(&mut self, all_pass: bool, n_pass: usize) {
        if !self.gfxf && all_pass && n_pass == SAMPLE_N && self.console_full_n >= 1 {
            self.gfxf = true;
            println!("GFXF ok n={}", n_pass);
        }
    }

    /// hdi 域分发：四 op 退化形（软 vsync；wire 零改动——§7-②）。
    fn handle_hdi(&mut self, ch: Handle, msg: &[u8], hs: &[Handle]) {
        match msg.first() {
            Some(&gp::OP_REG_VBLANK) => match gp::parse_reg_vblank_req(msg) {
                Some(1) => {
                    // 使能形：随行恰 1 event（客户端自建、SIGNAL 权随行）。
                    if hs.len() != 1 {
                        close_all(hs);
                        st_rep(ch, gp::ST_BAD);
                        return;
                    }
                    if self.vb_ev != 0 {
                        let _ = berry_handle_close(self.vb_ev); // 重注册覆盖
                    }
                    self.vb_ev = hs[0];
                    self.vb_armed = true;
                    // 刀3 A7：vsync 需求即活动——停帧破除取证先于排点。
                    let d = self.arb.on_activity(now_ns());
                    self.arb_exec(d);
                    // P6 首听众锚（与 park_vblank 同款）：重扫窗起算。
                    if self.panel.per_frame() && self.p6_last_scan_ns == 0 {
                        self.p6_last_scan_ns = now_ns();
                    }
                    self.vsync_schedule();
                    st_rep(ch, gp::ST_OK);
                }
                Some(0) => {
                    // 关闭形（A3-d①）：close 存量 + close 本次随行（客户端
                    // 无法重发已迁出的 handle——随行者一律收下即关；宽收
                    // nh∈{0,1}，多余按拒面关）。
                    if self.vb_ev != 0 {
                        let _ = berry_handle_close(self.vb_ev);
                        self.vb_ev = 0;
                    }
                    close_all(hs);
                    self.vb_armed = false;
                    if !self.vb_parked {
                        self.vb_scheduled = false; // 双空撤点（零唤醒回位）
                        // 刀3 A7：撤点即静默判拍（此后无拍——on_time 面评估；
                        // 停帧证据行在席）。
                        let now = now_ns();
                        let d = self.arb.on_time(now, self.quiet_now(now));
                        self.arb_exec(d);
                    }
                    st_rep(ch, gp::ST_OK);
                }
                _ => {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                }
            },
            Some(&gp::OP_VBLANK_WAIT) if gp::parse_vblank_wait_req(msg).is_some() => {
                self.park_vblank(ch, hs, false);
            }
            // 刀2 事件面（§4-4）：等拍点三元组——VBLANK_WAIT 同车位同纪律，
            // 应答 24B 三元组（16B GOLDEN 面不动）。
            Some(&gp::OP_VBLANK_TIMING) if gp::parse_vblank_timing_req(msg).is_some() => {
                self.park_vblank(ch, hs, true);
            }
            Some(&gp::OP_PREPARE) => match gp::parse_prepare_req(msg) {
                Some(req) => self.prepare(ch, hs, req),
                None => {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                }
            },
            Some(&gp::OP_COMMIT) => {
                match gp::parse_commit_req(msg) {
                    Some((slot, target)) => {
                        if !hs.is_empty() {
                            close_all(hs);
                            st_rep(ch, gp::ST_BAD);
                            return;
                        }
                        // 未 PREPARE 槽 = 引用不存在（parse 已帽 <2 拒掉
                        // 越帽野值——界内未 PREPARE 属 slot 无效非畸形；
                        // ST_NOENT wire 立法 guiproto :88-89，FRAME_CALLBACK
                        // token 空查同款对称）。
                        if self.slots[slot as usize].vmo == 0 {
                            st_rep(ch, gp::ST_NOENT);
                            return;
                        }
                        // P5 §7：在途帧在案 → ST_BUSY（深度 1 管线——
                        // SUBMIT/COMMIT 同队列互斥）。
                        if self.pending.is_some() {
                            st_rep(ch, gp::ST_BUSY);
                            return;
                        }
                        // 提交即活动（A7——呈现管线动，同 SUBMIT 面）。
                        let now = now_ns();
                        let d = self.arb.on_activity(now);
                        self.arb_exec(d);
                        let latch_ns = if target == 0 {
                            now // 无目标 = 尽快（即时呈现）
                        } else if self.panel.per_frame() {
                            self.panel.scan_point(self.p6_last_scan_ns, target)
                        } else {
                            self.vsync_schedule();
                            self.latch_point(target)
                        };
                        if latch_ns <= now {
                            // 即时呈现：槽 VMO 全幅 blit + 呈现时戳 +
                            // P5C 一次性证据 + 谱系抬高（无 token 域——
                            // 回执环不记，帧时序查询面不适用 P5）。
                            let fullrect =
                                gp::Rect { x: 0, y: 0, w: self.w as u32, h: self.h as u32 };
                            let (read_ok, all_pass, n_pass) =
                                self.blit_and_sample(self.slots[slot as usize].vmo,
                                                     core::slice::from_ref(&fullrect));
                            if read_ok {
                                let (presented_ns, _seq) = self.present_stamp(now);
                                self.present_seq = self.present_seq.wrapping_add(1);
                                self.slots[slot as usize].commit_target = target;
                                if !self.p5c_seen {
                                    self.p5c_seen = true;
                                    println!("P5C ok latch={}", presented_ns);
                                }
                                self.gfxf_check(all_pass, n_pass);
                                self.cc_track(presented_ns);
                                if self.rel_park.is_some() {
                                    self.answer_release();
                                }
                                st_rep(ch, gp::ST_OK);
                            } else {
                                // 读失败 = loud + 拒答（槽内容客户端可见可
                                // 重试；谱系不动）。
                                eprintln!("guifbd: commit blit fail slot={}", slot);
                                st_rep(ch, gp::ST_BAD);
                            }
                            return;
                        }
                        // 入途（token=0 = P5 面——无回执域；槽 VMO 借读不
                        // 迁权，槽复用竞态面 = latch 读失败 loud 弃）。
                        self.slots[slot as usize].commit_target = target;
                        self.pending = Some(PendingFrame {
                            token: 0,
                            vmo: self.slots[slot as usize].vmo,
                            full: true,
                            rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
                            rect_count: 0,
                            latch_ns,
                            unsquashable: false,
                        });
                        st_rep(ch, gp::ST_OK);
                    }
                    None => {
                        close_all(hs);
                        st_rep(ch, gp::ST_BAD);
                    }
                }
            }
            // 面板能力宣告（§4-4）：Tiered = 档位表全量（class + 首启阶段
            // 入行——v2 形；min/max 取全表极值，声明序非升序）/ FreeRange
            // = 空档位表 + 区间 {1..=480, gran 1, per_frame, vrr capable}
            //（§3.2 仿真宣告）。当前档两态同报。flags/vrr 取 PanelCfg 真值
            //（内核 fw_cfg 旋钮面——freerange:per_frame / tiered:vrr），
            // 不再服务端硬编码。无随行 handle、无副作用。
            Some(&gp::OP_PANEL_CAPS) if gp::parse_panel_caps_req(msg).is_some() => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                let cfg = self.panel.cfg();
                let (flags, vrr) = (
                    if cfg.per_frame { gp::PANEL_FLAG_PER_FRAME } else { 0 },
                    cfg.vrr,
                );
                let rep = match cfg.model {
                    gp::RATE_MODEL_FREERANGE => gp::PanelCapsRep {
                        version: 2,
                        rate_model: gp::RATE_MODEL_FREERANGE,
                        flags,
                        min_hz: gp::softpanel::FREERANGE_MIN_HZ,
                        max_hz: gp::softpanel::FREERANGE_MAX_HZ,
                        granularity_hz: gp::softpanel::FREERANGE_GRAN_HZ,
                        vrr,
                        current_hz: self.panel.hz(),
                        current_period_ns: self.panel.period_ns() as u32,
                        current_phase_ns: 0,
                        tiers: [gp::PanelTier {
                            hz: 0,
                            class: 0,
                            stage: 0,
                            period_ns: 0,
                        }; gp::PANEL_TIERS_MAX],
                        n_tiers: 0,
                    },
                    _ => {
                        let mut tiers = [gp::PanelTier {
                            hz: 0,
                            class: 0,
                            stage: 0,
                            period_ns: 0,
                        }; gp::PANEL_TIERS_MAX];
                        for (i, t) in gp::softpanel::TIERS.iter().enumerate() {
                            tiers[i] = gp::PanelTier {
                                hz: t.hz,
                                class: t.class,
                                stage: t.stage,
                                period_ns: t.period_ns as u32,
                            };
                        }
                        // min/max = 档位表首尾语义（§4-4）——TIERS 声明序
                        // 非升序（产品→测试→LTPO），取全表极值。
                        let (mut min_hz, mut max_hz) = (u16::MAX, 0u16);
                        for t in gp::softpanel::TIERS.iter() {
                            min_hz = min_hz.min(t.hz);
                            max_hz = max_hz.max(t.hz);
                        }
                        gp::PanelCapsRep {
                            version: 2,
                            rate_model: gp::RATE_MODEL_TIERED,
                            flags,
                            min_hz,
                            max_hz,
                            granularity_hz: 0,
                            vrr,
                            current_hz: self.panel.hz(),
                            current_period_ns: self.panel.period_ns() as u32,
                            current_phase_ns: 0,
                            tiers,
                            n_tiers: gp::softpanel::TIERS.len() as u8,
                        }
                    }
                };
                let mut out = [0u8; gp::PANEL_CAPS_REP_MAX];
                let n = gp::encode_panel_caps_rep(&mut out, &rep);
                send_rep(ch, &out[..n]);
            }
            _ => {
                close_all(hs);
                st_rep(ch, gp::ST_BAD);
            }
        }
    }

    /// PREPARE：槽记账 + 存 VMO 别名 + 应答 slot + 空置 fence（v1 永不
    /// 置位——异步后端刀接管面）。
    fn prepare(&mut self, ch: Handle, hs: &[Handle], req: gp::PrepareReq) {
        if hs.len() != 1 {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        let hvmo = hs[0];
        // format 是**值**非位域（§3.5-6）：v1 单格式面（FMT_XRGB8888——
        // GETINFO 注同源）；不在集 → ST_UNSUPPORTED（拒面 close 随行）。
        if req.format != gp::FMT_XRGB8888 {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_UNSUPPORTED);
            return;
        }
        // 槽策略：先空槽；无空槽且几何同 → 覆盖重用（close 旧别名——
        // A3-d② 槽复用生命周期）；否则 ST_FULL（拒面 close 随行）。
        let mut slot: Option<usize> = None;
        for i in 0..SLOTS {
            if self.slots[i].vmo == 0 {
                slot = Some(i);
                break;
            }
        }
        if slot.is_none() {
            for i in 0..SLOTS {
                if self.slots[i].w == req.width
                    && self.slots[i].h == req.height
                    && self.slots[i].stride == req.stride
                {
                    let _ = berry_handle_close(self.slots[i].vmo);
                    slot = Some(i);
                    break;
                }
            }
        }
        let Some(i) = slot else {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_FULL);
            return;
        };
        let mut fence: Handle = 0;
        if berry_event_create(0, &mut fence) != 0 {
            // 内核对象面枯竭 = 服务端资源死（非客户端错）——loud 自陷。
            eprintln!("guifbd: fence create failed");
            trap();
        }
        self.slots[i] = Slot {
            vmo: hvmo,
            w: req.width,
            h: req.height,
            stride: req.stride,
            commit_target: 0,
        };
        let mut out = [0u8; 4];
        let n = gp::encode_prepare_rep(&mut out, i as u8);
        // fence 随应答迁给客户端（出生信号全零 = 空置）。
        send_rep_handles(ch, &out[..n], &[fence]);
    }
}

fn main() {
    // 几何 info VMO（boot 72B 九域 u64 LE：fb_pa/w/h/stride/fmt/gui/
    // refresh_hz + 软面板 cfg 两域——第 8/9 域 = 刀2 尾插）。
    let h: Handle = gift2_h();
    if h == 0 {
        // svcmgr 探测门后恒随行 info——理论不可达；eprintln loud 兜底。
        eprintln!("guifbd: no fbinfo gift");
        berry_libc::park();
    }
    let mut info = [0u8; 72];
    if berry_vmo_read(h, info.as_mut_ptr(), 0, info.len()) != 0 {
        eprintln!("guifbd: fbinfo read failed");
        berry_libc::park();
    }
    let rd = |i: usize| {
        u64::from_le_bytes(info[i * 8..i * 8 + 8].try_into().unwrap()) as usize
    };
    let (_fb_pa, w, h, stride) = (rd(0), rd(1), rd(2), rd(3));
    let fmt_raw = (rd(4) as u64 & 0xFFFF_FFFF) as u32;
    // 第 6 域 = GUI 模式（旧内核只写 40B → 该域读 0 = 兼容旧行为）。
    let gui = rd(5) != 0;
    // 第 7 域 = 刷新率（旧内核 48B → 零页域读 0；语义层校验/回退在
    // `boot_panel_tier`——0 与表外值同路 loud 回退 60，规范 §4-1/§4-2）。
    let boot_hz = rd(6) as u16;
    // 第 8/9 域 = 软面板 cfg（旧内核 56B → 零页域读 0 = 缺省 Tiered/
    // FREE/无故障——add-only 尾插兼容读；位布局单源 guiproto
    // `softpanel::PanelCfg::from_domains`，kernel 打包手镜像对拍钉板）。
    let rdu = |i: usize| u64::from_le_bytes(info[i * 8..i * 8 + 8].try_into().unwrap());
    let panel_cfg = gp::softpanel::PanelCfg::from_domains(rdu(7), rdu(8));
    if panel_cfg != gp::softpanel::PanelCfg::DEFAULT {
        // 非缺省旋钮到位的证据行（宿主探针取证面；kernel 侧另有
        // `PANEL cfg` 前导行）。
        eprintln!(
            "guifbd: panel cfg model={} mode={} seed={} drop={} jit={} sw={}",
            panel_cfg.model,
            panel_cfg.mode,
            panel_cfg.seed,
            panel_cfg.drop_permille,
            panel_cfg.jitter_ns,
            panel_cfg.sw_drop
        );
    }
    // QEMU ramfb fmt=1 = DRM_FORMAT_XRGB8888（LE 内存 B,G,R,X）。本仓
    // `PixelFormat::Rgbx.pack` 才落该字节序；历史 `Bgrx` 映射把 R/B 互换，
    // 桌面蓝灰显示成土棕（截图实证）。只改显示契约映射。
    let fmt = match fmt_raw {
        1 => PixelFormat::Rgbx,
        0 => PixelFormat::Bgrx,
        _ => {
            eprintln!("guifbd: unsupported fmt={}", fmt_raw);
            berry_libc::park();
        }
    };
    // fb 窗借用（映射幅 = ceil_page(stride*h) ≥ 本幅——boot 双页对齐执法）。
    // SAFETY: mmio_vmo 映射在 spawn 期由 berry-exec 落位 FB_VA；几何来自
    // boot info VMO（与映射幅同源构造），界内必然已映射。
    let fb: &mut [u8] =
        unsafe { core::slice::from_raw_parts_mut(FB_VA as *mut u8, stride * h) };
    let mut con = match Fbcon::new(fb, w, h, stride, fmt) {
        Some(c) => c,
        None => {
            eprintln!("guifbd: fbcon geometry rejected");
            berry_libc::park();
        }
    };

    // —— 自验三步（确定性抽样点：四角内缩 + 中心 + 两个四分位）——
    // 抽样面收敛**蜂格区**（cols×8 × rows×16）：fbcon 铁律——右 w%8 /
    // 底 h%16 sliver 不属 console「一律不触碰」（render.rs 模块头），
    // clear 只盖蜂格面；首跑实踩 @1,598（600 高 → 蜂格 592，底 sliver
    // 是固件残留字节）差分误炸——sliver 内容本服务无权假设。
    let bg0 = (0x00u8, 0x00u8, 0x80u8); // 深蓝
    let bg1 = (0x00u8, 0x00u8, 0x00u8); // 黑
    let fg = (0xFFu8, 0xFFu8, 0xFFu8); // 白
    let (cw, chh) = (con.cols() * 8, con.rows() * 16);
    let pts = [
        (1usize, 1usize), (cw - 2, 1), (1, chh - 2), (cw - 2, chh - 2),
        (cw / 2, chh / 2), (cw / 4, chh / 4), (3 * cw / 4, 3 * chh / 4),
    ];
    // ① clear 双相位差分。
    con.clear(bg0);
    for &(x, y) in &pts {
        if px(stride, x, y) != fmt.pack(bg0.0, bg0.1, bg0.2) {
            eprintln!("guifbd: fill phase0 mismatch @{},{}", x, y);
            berry_libc::park();
        }
    }
    con.clear(bg1);
    for &(x, y) in &pts {
        if px(stride, x, y) != fmt.pack(bg1.0, bg1.1, bg1.2) {
            eprintln!("guifbd: fill phase1 mismatch @{},{}", x, y);
            berry_libc::park();
        }
    }
    // ② put_str 蜂格全对拍（glyph blit 路径）：A(0,0) B(1,0) '\n'→C(0,1) D(1,1)。
    con.put_str("AB\nCD", fg, bg1);
    for (col, row, ch) in [(0usize, 0usize, b'A'), (1, 0, b'B'), (0, 1, b'C'), (1, 1, b'D')] {
        if !cell_ok(stride, fmt, col, row, ch, fg, bg1) {
            eprintln!("guifbd: glyph cell mismatch '{}' @{},{}", ch as char, col, row);
            berry_libc::park();
        }
    }
    // ③ scroll_up(1)：原次行上移对拍（滚出空行 = 最近 bg = bg1）。
    con.scroll_up(1);
    if !cell_ok(stride, fmt, 0, 0, b'C', fg, bg1) || !cell_ok(stride, fmt, 1, 0, b'D', fg, bg1) {
        eprintln!("guifbd: scroll mismatch");
        berry_libc::park();
    }
    println!("FBW ok w={} h={} cells={}x{}", w, h, con.cols(), con.rows());
    // —— 段③：present/hdi 域端点取用（extras[2]/[3]——S2 分发面随行；
    // 0 值 = 协议破缺面，eprintln+park 兜底同 :81-91 款——GFXF 缺失 =
    // smoke FAIL loud）——
    let hpresent = present_srv_h();
    if hpresent == 0 {
        eprintln!("guifbd: no present gift");
        berry_libc::park();
    }
    let hhdi = hdi_srv_h();
    if hhdi == 0 {
        eprintln!("guifbd: no hdi gift");
        berry_libc::park();
    }
    // 行弹舱启动门：几何非 ramfb 基线（stride 越帽）= blit 弹舱越界风险，
    // 宁可不起服务（FAIL loud）不留未定义行为面。
    if stride > ROW_BOUNCE_MAX {
        eprintln!("guifbd: stride {} over bounce cap", stride);
        berry_libc::park();
    }
    let hpipe = gift3_h();
    if hpipe == 0 {
        // svcmgr 探测门后 extras[1] 恒随行——理论不可达；eprintln loud +
        // park 兜底（FBSERVE ok 缺失 = smoke FAIL）。
        eprintln!("guifbd: no fb pipe gift");
        berry_libc::park();
    }
    let mut srv = Srv::new(con, w, h, stride, gui, panel_cfg, boot_hz);
    // —— 三路 serve：present→hdi→console 确定性轮询；全 EAGAIN 才
    // wait_many n=3 并等。deadline = min(下次软 vsync 点, now+10s) **进
    // 调用本身**（A1-a——醒来后补查不够：parked 最长迟答 10s 且 smoke
    // 抓不住）；命中/到点皆回环顶三路重询（poll-first 无丢唤醒）。
    // 零唤醒不变量：无未决请求 = 无主动 send（IDLE 面结构先行）。
    loop {
        srv.tick_vsync();
        srv.tick_latch(); // P2 在途帧到点呈现（deadline 臂保证最迟本拍醒）
        srv.arb_on_time(); // 仲裁时钟醒（降档驻留 / boost 到期——无拍推进面）
        if srv.poll_present(hpresent) {
            continue;
        }
        if srv.poll_hdi(hhdi) {
            continue;
        }
        if srv.poll_console(hpipe) {
            continue;
        }
        let t = now_ns();
        let mut deadline = t + IDLE_CAP_NS;
        if srv.vb_scheduled && srv.vb_next_ns < deadline {
            deadline = srv.vb_next_ns;
        }
        // P2 在途 latch 臂（呈现拍最迟醒——parked callback/release 不悬）。
        if let Some(p) = srv.pending_ref() {
            if p < deadline {
                deadline = p;
            }
        }
        // P6 自刷新重扫臂（armed/parked 在席 + max_interval 窗到点——
        // 零听众不排，零唤醒优先）。
        if srv.panel.per_frame()
            && srv.p6_last_scan_ns != 0
            && (srv.vb_armed || srv.vb_parked)
        {
            let r = srv.p6_last_scan_ns + srv.panel.max_interval_ns();
            if r < deadline {
                deadline = r;
            }
        }
        // 刀3 仲裁醒臂（A5 驻留到点 / A6 窗到期；停帧态 None 零醒）。
        if let Some(w) = srv.arb.next_wake(t) {
            if w < deadline {
                deadline = w;
            }
        }
        let hs = [hpresent, hhdi, hpipe];
        let sigs = [SIGNAL_READABLE, SIGNAL_READABLE, SIGNAL_READABLE];
        let mut idx = 0usize;
        let mut obs = 0u64;
        let s = berry_object_wait_many(
            hs.as_ptr(),
            sigs.as_ptr(),
            3,
            deadline,
            &mut idx,
            &mut obs,
        );
        if s != 0 && s != BERRY_ERR_TIMEDOUT {
            trap(); // 到点 = 空闲重臂/vsync fire（回环顶兜底）；余错 = 死亡面
        }
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （fsmgr 同款——链断 loud，`FBSERVE ok` 缺失 = smoke FAIL 兜底）。
#[cold]
fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（0x4000_0000 与用户布局无交集——家族惯例值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}
