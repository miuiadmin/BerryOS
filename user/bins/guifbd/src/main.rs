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
//! handle**（send 即 take，不 close 即泄漏）。hdi 四 op 退化形：
//! REG_VBLANK 存 event、VBLANK_WAIT park 到软 vsync 点（SOFT_VSYNC_NS
//! 合成 ts_ns；已 parked 再来 = ST_BUSY）、PREPARE 槽记账 + 空置 fence、
//! COMMIT 记账——deadline 到点 = 应答 parked + 信号 event（`1<<32` 用户
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

/// 软 vsync 周期（≈60Hz——ramfb 无真实 vsync，服务端合成时基）。
const SOFT_VSYNC_NS: u64 = 16_666_666;
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
    /// token 环：(frame_token, presented_ns)，覆盖最旧。
    tokens: [(u32, u64); TOKEN_RING],
    /// 在途别名表（credits = len；RELEASE 全量归还清零）。
    held: [(u32, Handle); CREDITS],
    held_n: usize,
    /// VOTE 票面（记账不仲裁——§0 不做面）。
    vote: (u8, u8, u16, u16, u16),
    /// POWER_MODE 最近值（同上）。
    power_mode: u8,
    // —— hdi 域（软 vsync）——
    vb_seq: u32,
    /// 下次软 vsync 点（`vb_scheduled` 时有效）。
    vb_next_ns: u64,
    /// 有点在案（REG_VBLANK 使能 ∧/∨ VBLANK_WAIT parked——wait_many
    /// deadline 的 min 臂来源）。
    vb_scheduled: bool,
    vb_armed: bool,
    /// REG_VBLANK 存量 event（0 = 无）。
    vb_ev: Handle,
    vb_parked: bool,
    /// parked VBLANK_WAIT 的应答通道。
    vb_park_ch: Handle,
    slots: [Slot; SLOTS],
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
    fn new(con: Fbcon<'a>, w: usize, h: usize, stride: usize, gui: bool) -> Self {
        Srv {
            con,
            w,
            h,
            stride,
            present_seq: 0,
            submit_seq: 0,
            tokens: [(0, 0); TOKEN_RING],
            held: [(0, 0); CREDITS],
            held_n: 0,
            vote: (0, 0, 0, 0, 0),
            power_mode: 0,
            vb_seq: 0,
            vb_next_ns: 0,
            vb_scheduled: false,
            vb_armed: false,
            vb_ev: 0,
            vb_parked: false,
            vb_park_ch: 0,
            slots: [Slot { vmo: 0, w: 0, h: 0, stride: 0, commit_target: 0 }; SLOTS],
            served: false,
            gfxf: false,
            console_full_n: 0,
            gui,
            bounce: [0; ROW_BOUNCE_MAX],
        }
    }

    /// 软 vsync 排点（无点在案时；REG_VBLANK 使能 / VBLANK_WAIT park 共用）。
    fn vsync_schedule(&mut self) {
        if !self.vb_scheduled {
            self.vb_next_ns = now_ns() + SOFT_VSYNC_NS;
            self.vb_scheduled = true;
        }
    }

    /// 软 vsync 到点检查（每轮回环顶）：到点 → 应答 parked VBLANK_WAIT +
    /// 信号 REG_VBLANK event，seq 抬高、点推进（滞后追赶跳过已过点——
    /// 不补偿积压 vsync）；armed∧parked 双空后撤点（零唤醒不变量回位）。
    fn tick_vsync(&mut self) {
        if !self.vb_scheduled {
            return;
        }
        let t = now_ns();
        if t < self.vb_next_ns {
            return;
        }
        let point = self.vb_next_ns; // 名义点 = 合成 ts_ns（clock 口径）
        if self.vb_parked {
            let mut out = [0u8; 16];
            let rep = gp::VblankRep { seq: self.vb_seq.wrapping_add(1), ts_ns: point };
            let n = gp::encode_vblank_rep(&mut out, &rep);
            send_rep(self.vb_park_ch, &out[..n]);
            self.vb_parked = false;
        }
        if self.vb_armed {
            // 持有客户端 event 的迁移别名（SIGNAL 权随行）——脉冲形
            // clear+set 同位：周期信号每点重置可观察。
            let _ = berry_object_signal(self.vb_ev, VBLANK_SIG, VBLANK_SIG);
        }
        self.vb_seq = self.vb_seq.wrapping_add(1);
        let mut next = point + SOFT_VSYNC_NS;
        while next <= t {
            next += SOFT_VSYNC_NS;
        }
        self.vb_next_ns = next;
        if !self.vb_armed && !self.vb_parked {
            self.vb_scheduled = false;
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

    /// present 域分发：六 op；一切拒面 close 随行 handle 后 1B 应答
    /// （A3-b——send 即 take，不 close 即泄漏）。
    fn handle_present(&mut self, ch: Handle, msg: &[u8], hs: &[Handle]) {
        match gp::parse_present_req(msg) {
            Some(gp::PresentReq::Getinfo) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // ver 1/1/1（wire 域版本——非本服务覆盖面）；格式面 v1
                // 单格式：32bpp RGBX 族（ramfb Bgrx=1，Rgbx 同族）→
                // FMT_XRGB8888 位——R/B 通道序 v1 wire 不建模（gfxdemo
                // 同源 Bgrx 构造，直拷零转换）；max_credits = 帽真值。
                let rep = gp::GetinfoRep {
                    version_present: 1,
                    version_input: 1,
                    version_hdi: 1,
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
                rate_hint: _,
                target_present_ns: _,
                rects,
                rect_count,
            }) => {
                self.submit(ch, hs, buffer_id, flags, &rects, rect_count);
            }
            Some(gp::PresentReq::FrameCallback { frame_token }) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // token 环校验（8 槽覆盖最旧；0 哨兵）；未命中 → ST_NOENT
                // 1B 烘焙线。
                let mut hit: Option<u64> = None;
                for &(t, ns) in &self.tokens {
                    if t == frame_token && t != 0 {
                        hit = Some(ns);
                    }
                }
                match hit {
                    None => st_rep(ch, gp::ST_NOENT),
                    Some(presented_ns) => {
                        // blit-at-SUBMIT：应答即 latch（v1 退化形）。
                        // present_seq = 谱系现值——console 共享抬高，
                        // **禁断言精确值**（A2-b）。
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
            }
            Some(gp::PresentReq::ReleaseWait) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
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
                // 票面收下即回执（仲裁策略挂 GUI-6+——§0 不做面）。
                self.vote = (sticky, scene, min_hz, max_hz, preferred_hz);
                st_rep(ch, gp::ST_OK);
            }
            Some(gp::PresentReq::PowerMode { mode }) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                self.power_mode = mode; // PM 脉冲真落地挂 GUI-6+——记账面
                st_rep(ch, gp::ST_OK);
            }
            None => {
                // 野帧（截断/尾随/未知 op）——坏客户端不得杀服务。
                close_all(hs);
                st_rep(ch, gp::ST_BAD);
            }
        }
    }

    /// SUBMIT：v1 = **blit-at-SUBMIT**（处理内同步 CPU blit——无 pending
    /// 帧状态机，squash/DISCARDED 死路径留异步后端刀）。
    fn submit(&mut self, ch: Handle, hs: &[Handle], buffer_id: u32, flags: u8,
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
        // SUBMIT_F_UNSQUASHABLE：v1 恒不判弃（blit-at-SUBMIT 钉板）——
        // 旗标收到即合规忽略。
        let fullrect = gp::Rect { x: 0, y: 0, w: self.w as u32, h: self.h as u32 };
        // ① damage 裁剪 CPU blit：逐 rect 行弹 vmo_read → 直写 fb（Bgrx
        // 同源直拷零转换）。越界裁剪（rect 与 fb 求交）；写路径走裸指针
        // ——Fbcon 持 fb slice 借用，px() 同理；ramfb 实为 DRAM，非
        // volatile 拷贝与 Fbcon 自身写路径同待遇。
        let targets: &[gp::Rect] = if full {
            core::slice::from_ref(&fullrect)
        } else {
            &rects[..rect_count]
        };
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
                    let _ = berry_handle_close(hvmo);
                    st_rep(ch, gp::ST_BAD);
                    return;
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
        // ② 抽样对拍（交叉核验强于复算期望——§7-⑤）：SAMPLE_N 点全落
        // damage 并集内（第 i 点取 rects[i%K]——A5-b），px() 读回 vs
        // vmo_read 同坐标。失配 = 服务端 bug → eprintln loud（标记缺失
        // = smoke FAIL），不拦应答（抽样 = 验证仪表非协议门）。
        let mut n_pass = 0usize;
        let mut all_pass = true;
        for i in 0..SAMPLE_N {
            let r = if full { &fullrect } else { &rects[i % rect_count] };
            // 采样点 = rect 内 (i+1)/(SAMPLE_N+1) 分位（u32 乘加无溢出面：
            // rect ≤ 界内值域）；分母 = SAMPLE_N+1 ⇒ 分位 <1 恒严格内点
            // （A5-b 对任意合法几何封闭——分母勿改 SAMPLE_N 否则尾点取
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
        // ③ 记账：谱系抬高 + token 入环 + 别名入 held。
        self.present_seq = self.present_seq.wrapping_add(1);
        self.submit_seq = self.submit_seq.wrapping_add(1);
        let token = self.submit_seq;
        self.tokens[token as usize % TOKEN_RING] = (token, now_ns());
        self.held[self.held_n] = (buffer_id, hvmo);
        self.held_n += 1;
        // ⑦ 首帧标记：∧ 抽样全过 ∧ console_full_n≥1（FBW ok 回流自身
        // pipe 的结构保证——console 已有 FULL 记账才证明两路同屏）。
        if !self.gfxf && all_pass && n_pass == SAMPLE_N && self.console_full_n >= 1 {
            self.gfxf = true;
            println!("GFXF ok n={}", n_pass);
        }
        // ④ 8B token 应答（blit 已完成——应答即呈现，credits 剩余随
        // FRAME_CALLBACK 面）。
        let mut out = [0u8; 8];
        let n = gp::encode_submit_ack(&mut out, token);
        send_rep(ch, &out[..n]);
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
                    }
                    st_rep(ch, gp::ST_OK);
                }
                _ => {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                }
            },
            Some(&gp::OP_VBLANK_WAIT) if gp::parse_vblank_wait_req(msg).is_some() => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // 单车位：已 parked 再来 = ST_BUSY（A3-d③——严格串行 RPC
                // 下属客户端违约面）。
                if self.vb_parked {
                    st_rep(ch, gp::ST_BUSY);
                    return;
                }
                // park 到软 vsync 点——**无即刻应答**（tick_vsync 到点
                // 应答；deadline min 臂保证最迟一个空闲帽内到点）。
                self.vb_parked = true;
                self.vb_park_ch = ch;
                self.vsync_schedule();
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
                        // v1 纯记账：target 不排队（blit-at-SUBMIT 形态下
                        // hdi 提交面无呈现动作——排队呈现挂异步后端刀）。
                        self.slots[slot as usize].commit_target = target;
                        st_rep(ch, gp::ST_OK);
                    }
                    None => {
                        close_all(hs);
                        st_rep(ch, gp::ST_BAD);
                    }
                }
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
    // 几何 info VMO（boot 48B 六域 u64 LE：fb_pa/w/h/stride/fmt/gui）。
    let h: Handle = gift2_h();
    if h == 0 {
        // svcmgr 探测门后恒随行 info——理论不可达；eprintln loud 兜底。
        eprintln!("guifbd: no fbinfo gift");
        berry_libc::park();
    }
    let mut info = [0u8; 48];
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
    let mut srv = Srv::new(con, w, h, stride, gui);
    // —— 三路 serve：present→hdi→console 确定性轮询；全 EAGAIN 才
    // wait_many n=3 并等。deadline = min(下次软 vsync 点, now+10s) **进
    // 调用本身**（A1-a——醒来后补查不够：parked 最长迟答 10s 且 smoke
    // 抓不住）；命中/到点皆回环顶三路重询（poll-first 无丢唤醒）。
    // 零唤醒不变量：无未决请求 = 无主动 send（IDLE 面结构先行）。
    loop {
        srv.tick_vsync();
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
