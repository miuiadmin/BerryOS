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
//!
//! **B1-b+c（显示与加速体系 02 号 §3.2/§6，双后端刀）**：本模块自
//! `src/main.rs` 零重排迁入（lib/bin 双面化——main.rs 薄壳 + 本件 guest
//! 面，gui-inputd 先例）。新增三件：
//!
//! 1. **boot 交接**（`main` 首步）：svcmgr 预拉本服务后把 virtio-mmio 窗
//!    映进子空间 `VIRTIO_MMIO_VA`（`ChildHandles::hv`——`mmio_vmo` 单窗
//!    槽恒归 fb 窗〔ramfb 红线的构造保证〕，第二窗走父侧后映射，02 号
//!    §7「mmio 扫描面 handle 需求」的解法），经 stdin 通道发单字节交接
//!    （'w' = 窗已映）。首 syscall = 读该字节——**读窗先于交接 = 序保证**
//!    （未映 VA 先读 = D-abort 杀服务）。旧 svcmgr（feeder EOF）/交接破缺
//!    → Ramfb 缺省安全。
//! 2. **后端分派**：交接 'w' → 扫 32 槽找 did=16（vprobe 形态，S1 实录
//!    §2.2）——找到 → vgpu 后端（初始化链 + 帧提交钩子 + `FB ok vgpu`
//!    标记）；没找到 → **Ramfb 零漂移**（现役路径逐字节不变——无设备相
//!    无新输出无新 syscall 面〔交接读除外，零可见差〕）。
//! 3. **vgpu 后端**（[`Vgpu`]）：初始化链 = INIT_SEQ 四命令（GET_DISPLAY_INFO
//!    → CREATE_2D → ATTACH_BACKING → SET_SCANOUT——纯面提交序钉板）；资源
//!    backing = **fb 窗同一物理段**（RamDxe AllocatePages 连续 DRAM——
//!    blit/蜂格/Fbcon 写路径零改动，ramfb 与 vgpu 双设备同屏一致）；
//!    传输 = berry-virtio split ring + INDIRECT 描述符（QEMU 侧 `indirect_desc`
//!    默认 on，S1 §2.1）——控制队列命令/响应同步对（QEMU 2D 全命令即答，
//!    used ring 收割即背压，02 号 §3.1）。
//! 4. **v2（per-rect flush + 异步发车——12 号 §4.3 排刀）**：帧提交从
//!    「全帧对 × cmd_sync 同步等」升格为 [`Vgpu::submit_frame`] 批形——
//!    SUBMIT damage 矩形集（wire 已在席，v1 收到即弃的接线收口）裁剪后
//!    逐矩形 TRANSFER_TO_HOST_2D（offset = y×stride+x×4）+ 单 FLUSH 包络
//!    （协议 FLUSH 恰一矩形、无 N-rect 数组形——Linux `virtgpu_plane.c`
//!    包络同构；合并挂 v3）；全帧形保留为回退路径（console 路 Fbcon 零
//!    脏区上报/init 首推/SUBMIT FULL）。发车/回执拆分：发车 = publish
//!    全批 + 单次门铃不等回执；回执 = [`Vgpu::reap`] 下帧发车前收（2-deep
//!    ——上帧设备处理与本帧 CPU 工作重叠；FLUSH 头置 FLAG_FENCE，QEMU 2D
//!    即答路径回执回显 fence_id 作批身份正向锚）。ramfb 红线零漂移：全部
//!    改动在 vgpu 分支内。

use berry_exec::{FB_VA, VIRTIO_MMIO_SLOTS, VIRTIO_MMIO_STRIDE, VIRTIO_MMIO_VA};
use berry_fbcon::font::{self, GLYPH_H};
use berry_fbcon::render::{Fbcon, PixelFormat};
use berry_guiproto as gp;
use berry_libc::io::{gift2_h, gift3_h, hdi_srv_h, present_srv_h, read};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_event_create,
    berry_handle_close, berry_object_signal, berry_object_wait_many, berry_object_wait_one,
    berry_thread_sleep, berry_vmo_commit, berry_vmo_create, berry_vmo_get_phys, berry_vmo_get_size,
    berry_vmo_read, berry_vmo_write, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
};
// B1-b/c：协议 crate（九命令 encode/parse——B1-a 冻结）+ 传输 crate
// （RingLayout/publish/used + mmio 表 + INDIRECT 位——vblk 先例）+ 本 crate
// lib 纯面（分派/链构造/提交序——host 钉板）。
use berry_gpuproto as vgp;
use berry_virtio as vio;
use guifbd::{
    attach_entry, backend_from_handoff, batch_resps_ok, chain_put, frame_flush_batch,
    rects_clip_into, Backend, ChainEnt, CHAIN_MAX, ACMD_OFF, ACMD_SZ, AIND_OFF, ARESP_OFF, CMD_OFF,
    DMA_BUF_OFF, DMA_RING_OFF, FLUSH_RECT_MAX, FRAME_CMD_MAX, IND_OFF, RESP_CAP, RESP_OFF,
};

/// 批矩形帽互钉：lib 批形状常量 = wire damage 帽（两 crate 均冻结——值
/// 漂移即编译炸响，防线在编译期非运行期）。
const _: () = assert!(gp::DAMAGE_MAX == FLUSH_RECT_MAX);

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
/// 波3-件② 系统面 surface_id marker（launcher 提交带系统带视觉不裁剪；
/// 0 = 应用面裁系统带。非协议号——SUBMIT 既有 u32 域的本地语义分档）。
const SYS_SURFACE_ID: u32 = 0xFFFF;

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
    /// B1-b：vgpu 后端实例（None = ramfb 单后端——现役路径）。console/
    /// SUBMIT 写路径后的全帧 TRANSFER+FLUSH 钩子面（ramfb 相恒短路）。
    vgpu: Option<Vgpu>,
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
        vgpu: Option<Vgpu>,
    ) -> Self {
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
            vgpu,
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
            // B1-b：vgpu 帧提交钩子（v2 全帧异步——console 路 Fbcon 零脏区
            // 上报保持全帧形；ramfb 相 None 恒短路零开销）。
            if let Some(v) = self.vgpu.as_mut() {
                v.submit_full();
            }
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
                surface_id,
                buffer_id,
                flags,
                rate_hint: _,
                target_present_ns: _,
                rects,
                rect_count,
            }) => {
                self.submit(ch, hs, buffer_id, flags, &rects, rect_count, surface_id);
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
              rects: &[gp::Rect; gp::DAMAGE_MAX], rect_count: usize, surface_id: u32) {
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
        // 波3-件② 带合成 v1 兼容档：surface_id=0（应用面）提交裁剪系统
        // 带——顶部状态栏带 + 底部导航条带（几何镜像 launcher desktop.rs
        // 常量：big 档 132/190px、紧凑档 44/44px；满档判据同源
        // vp>=600×1800）。应用全屏提交不再覆盖系统带 → 状态栏/导航条
        // 视觉常驻（owner 投诉「应用进去了状态栏没了」v1 根治）。
        // surface_id=0xFFFF = 系统面 marker（launcher 提交自带带视觉）
        // 不裁剪。裁剪只动 damage 面：blit 与抽样（②）同用裁后集，
        // 抽样点恒落裁后 rect 内 → 与 fb 对拍口径不变。
        let big = self.w >= 600 && self.h >= 1800;
        let (band_top, band_bot) = if big { (132usize, 190usize) } else { (44, 44) };
        let sys_surface = surface_id == SYS_SURFACE_ID;
        let mut clipped_storage = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX + 1];
        let mut clipped_n = 0usize;
        let final_targets: &[gp::Rect] = if sys_surface {
            targets
        } else {
            for r in targets {
                let mut y0 = r.y as usize;
                let mut y1 = (r.y as usize + r.h as usize).min(self.h);
                if y1 <= y0 || y0 >= self.h {
                    continue;
                }
                // 裁顶带（全宽带：与 rect 求交即弃 y<band_top 段）
                if y0 < band_top {
                    y0 = band_top;
                }
                // 裁底带
                if y1 > self.h - band_bot {
                    y1 = self.h - band_bot;
                }
                if y1 <= y0 {
                    continue; // rect 全在带内 = 空集跳过
                }
                if clipped_n < clipped_storage.len() {
                    clipped_storage[clipped_n] =
                        gp::Rect { x: r.x, y: y0 as u32, w: r.w, h: (y1 - y0) as u32 };
                    clipped_n += 1;
                }
            }
            &clipped_storage[..clipped_n]
        };
        for r in final_targets {
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
        // B1-b/v2：vgpu 帧提交钩子——blit 落 fb 后、抽样前（设备侧上传与
        // px() 抽样对拍独立；v2 per-rect：SUBMIT 矩形集直通设备上传——
        // v1 收到矩形但 flush_full 丢弃的接线收口。ramfb 相 None 恒短路
        // 零开销）。
        if let Some(v) = self.vgpu.as_mut() {
            v.submit_frame(full, rects, rect_count);
        }
        // ② 抽样对拍（交叉核验强于复算期望——§7-⑤）：SAMPLE_N 点全落
        // damage 并集内（第 i 点取 rects[i%K]——A5-b），px() 读回 vs
        // vmo_read 同坐标。失配 = 服务端 bug → eprintln loud（标记缺失
        // = smoke FAIL），不拦应答（抽样 = 验证仪表非协议门）。
        let mut n_pass = 0usize;
        let mut all_pass = true;
        // 抽样与 blit 同用裁后集（final_targets）——空集帧（全在带内）
        // 跳过抽样恒 pass（无 damage 面 = 无可对拍点）。
        if final_targets.is_empty() {
            n_pass = SAMPLE_N; // 空集合法帧：抽样面空转全过
        }
        for i in 0..SAMPLE_N {
            if final_targets.is_empty() {
                break;
            }
            let r = &final_targets[i % final_targets.len()];
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

// ==========================================================================
// B1-b+c：vgpu 后端 guest 面（virtio-gpu 2D 控制队列驱动）
// ==========================================================================

/// 控制队列 qsize（整环落一页——berry-virtio 单页承诺；QEMU QueueNumMax
/// 远大于此）。
const VIO_QSIZE: u16 = 16;
/// framebuffer 资源 id（v1 单资源单 scanout；多资源/动态分辨率挂 v2）。
const VIO_RES: u32 = 1;
/// scanout id（QEMU `max_outputs` 默认 1——单屏）。
const VIO_SCANOUT: u32 = 0;
/// 控制队列 qidx（QEMU `virtio_gpu_device_realize`：ctrl=0/cursor=1——v1
/// 只启用 ctrl，spec 允许不协商 cursor）。
const VIO_CTRLQ: u32 = 0;
/// DMA VMO 幅：page0 = virtqueue 整环、page1 = 命令/响应/间接表三缓冲。
const VIO_DMA_LEN: u64 = 2 * 4096;
/// used ring 轮询帽（QEMU 2D 全命令即答——常态首查即中；超帽 = 传输层
/// 断，停刷不杀服务）与 1ms 步进（vblk 同值同义）。
const VIO_POLL_CAP_NS: u64 = 10_000_000_000;
const VIO_POLL_STEP_NS: u64 = 1_000_000;
/// STATUS 复位回读帽（真机复位 ms 级；QEMU 同步即 0——vblk 同值）。
const VIO_RESET_CAP_NS: u64 = 1_000_000_000;

// SAFETY: 调用方单线程（main 顺序流 + serve poll-first 单线——fsmgr 惯例），
// 独占窗；static 免大栈帧（vblk 同款）。avail.idx 状态留 RING 暂存跨命令续用。
static mut VIO_RING: [u8; 4096] = [0; 4096];
/// 命令帧暂存（≤64B：最大 TRANSFER_TO_HOST_2D 56B；ATTACH_BACKING v1 单
/// entry 48B）。
static mut VIO_CMD: [u8; 64] = [0; 64];
/// 响应读回暂存（RESP_CAP = 408B）。
static mut VIO_RESP: [u8; RESP_CAP] = [0; RESP_CAP];
/// 间接描述符表暂存（CHAIN_MAX×16B）。
static mut VIO_IND: [u8; CHAIN_MAX * vio::DESC_SZ] = [0; CHAIN_MAX * vio::DESC_SZ];
/// v2 批命令暂存（FRAME_CMD_MAX×ACMD_SZ——编帧在 statics，落 DMA 前的
/// staging；与设备处理上帧重叠 = 2-deep 流水的 CPU 半）。
static mut VIO_ACMD: [u8; FRAME_CMD_MAX * ACMD_SZ] = [0; FRAME_CMD_MAX * ACMD_SZ];
/// v2 批间接表暂存（每命令 2 entry × 16B）。
static mut VIO_AIND: [u8; FRAME_CMD_MAX * 2 * vio::DESC_SZ] =
    [0; FRAME_CMD_MAX * 2 * vio::DESC_SZ];
/// v2 回执桶读回暂存（每命令 24B）。
static mut VIO_ARESP: [u8; FRAME_CMD_MAX * vgp::RESP_OK_NODATA_SZ] =
    [0; FRAME_CMD_MAX * vgp::RESP_OK_NODATA_SZ];

/// 占用槽寄存器基（u32 字指针——偏移一律 `/4` 进字单位，vblk 同款）。
fn vio_regs(slot: usize) -> *mut u32 {
    (VIRTIO_MMIO_VA + (slot * VIRTIO_MMIO_STRIDE) as u64) as *mut u32
}

fn vio_rd(regs: *mut u32, off: usize) -> u32 {
    // SAFETY: 窗口经交接序保证已映射（Device-nGnRnE）；对齐 u32 volatile 读。
    unsafe { core::ptr::read_volatile((regs as *const u32).add(off / 4)) }
}

fn vio_wr(regs: *mut u32, off: usize, v: u32) {
    // SAFETY: 同上；volatile 写（Device-nGnRnE，写不合并）。
    unsafe { core::ptr::write_volatile(regs.add(off / 4), v) }
}

/// 扫 32 槽找 virtio-gpu（did=16、占用槽 ver=2——vprobe 形态，S1 实录
/// §2.2；勿绑死槽号，设备落位随 -device 顺序漂移）。**无设备 → None 静默**
/// （ramfb 红线：无设备相零输出）；窗几何破缺/legacy gpu 槽 → 单行诊断 +
/// None（屏走 ramfb 保活，gpu 相 `FB ok vgpu` 缺席 = smoke FAIL loud）。
fn vio_scan() -> Option<usize> {
    let base = VIRTIO_MMIO_VA as *const u32;
    for slot in 0..VIRTIO_MMIO_SLOTS {
        // SAFETY: 交接序保证整窗已映射；只读 ID 头四字（RO 无副作用）。
        let regs = unsafe { base.add(slot * (VIRTIO_MMIO_STRIDE / 4)) };
        let magic = unsafe { core::ptr::read_volatile(regs) };
        if magic != vio::mmio::MAGIC_VAL {
            eprintln!("guifbd: vgpu slot {} bad magic={:#x}", slot, magic);
            return None;
        }
        let did = unsafe { core::ptr::read_volatile(regs.add(2)) };
        if did != vio::mmio::DEVICE_ID_GPU {
            continue; // 空槽/blk/net/input 合法同席（分类认领，vprobe 先例）
        }
        let ver = unsafe { core::ptr::read_volatile(regs.add(1)) };
        if ver != vio::mmio::VERSION_MODERN {
            eprintln!("guifbd: vgpu slot {} ver={}（须 force-legacy=false）", slot, ver);
            return None;
        }
        return Some(slot); // 首 gpu 槽（v1 单设备；多 gpu 挂 v2）
    }
    None
}

/// modern 协商（vblk `negotiate` 直改）：复位 → ACK|DRIVER → 特性两页读 →
/// 收 INDIRECT_DESC（页 0 bit28——QEMU `indirect_desc` 默认 on，S1 §2.1）
/// + VERSION_1（页 1 bit0）→ DRV_FEAT 两页写 → FEATURES_OK 回读验。
fn vio_negotiate(regs: *mut u32) -> bool {
    vio_wr(regs, vio::mmio::STATUS, 0);
    let mut now = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
        return false;
    }
    let reset_deadline = now + VIO_RESET_CAP_NS;
    while vio_rd(regs, vio::mmio::STATUS) != 0 {
        if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 || now >= reset_deadline {
            return false;
        }
        if berry_thread_sleep(now + VIO_POLL_STEP_NS) != 0 {
            return false;
        }
    }
    vio_wr(regs, vio::mmio::STATUS, vio::ST_ACKNOWLEDGE | vio::ST_DRIVER);
    vio_wr(regs, vio::mmio::DEV_FEAT_SEL, 0);
    let f0 = vio_rd(regs, vio::mmio::DEV_FEAT);
    vio_wr(regs, vio::mmio::DEV_FEAT_SEL, 1);
    let f1 = vio_rd(regs, vio::mmio::DEV_FEAT);
    if f1 & vio::FEAT_VERSION_1 == 0 || f0 & vio::FEAT_INDIRECT_DESC == 0 {
        eprintln!("guifbd: vgpu 特性缺席 f0={:#x} f1={:#x}", f0, f1);
        return false;
    }
    vio_wr(regs, vio::mmio::DRV_FEAT_SEL, 0);
    vio_wr(regs, vio::mmio::DRV_FEAT, vio::FEAT_INDIRECT_DESC);
    vio_wr(regs, vio::mmio::DRV_FEAT_SEL, 1);
    vio_wr(regs, vio::mmio::DRV_FEAT, vio::FEAT_VERSION_1);
    let want = vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK;
    vio_wr(regs, vio::mmio::STATUS, want);
    if vio_rd(regs, vio::mmio::STATUS) != want {
        return false; // 设备撤 FEATURES_OK = 协商拒绝
    }
    true
}

/// DMA 几何（ring/buf 两页后备 PA——描述符地址直用）。
struct VioDma {
    vmo: Handle,
    lay: vio::RingLayout,
    buf_pa: u64,
}

/// 建控制队列（vblk `setup_queue` 直改：Q_SEL/Q_NUM/三址 LOW+HIGH/
/// Q_READY=1 回读验——modern 序）。
fn vio_setup_queue(regs: *mut u32) -> Option<VioDma> {
    let lay = vio::RingLayout::new(VIO_QSIZE as usize)?;
    let mut vmo = 0 as Handle;
    if berry_vmo_create(VIO_DMA_LEN, 0, &mut vmo) != 0 || vmo == 0 {
        return None;
    }
    if berry_vmo_commit(vmo, 0, VIO_DMA_LEN) != 0 {
        return None;
    }
    let mut ring_pa = 0u64;
    let mut buf_pa = 0u64;
    if berry_vmo_get_phys(vmo, DMA_RING_OFF, &mut ring_pa) != 0
        || berry_vmo_get_phys(vmo, DMA_BUF_OFF, &mut buf_pa) != 0
    {
        return None;
    }
    vio_wr(regs, vio::mmio::Q_SEL, VIO_CTRLQ);
    if vio_rd(regs, vio::mmio::Q_NUM_MAX) < VIO_QSIZE as u32 {
        return None;
    }
    vio_wr(regs, vio::mmio::Q_NUM, VIO_QSIZE as u32);
    vio_wr(regs, vio::mmio::Q_READY, 0);
    for (lo, hi, pa) in [
        (vio::mmio::Q_DESC_LOW, vio::mmio::Q_DESC_HIGH, ring_pa),
        (vio::mmio::Q_AVAIL_LOW, vio::mmio::Q_AVAIL_HIGH, ring_pa + lay.avail_off as u64),
        (vio::mmio::Q_USED_LOW, vio::mmio::Q_USED_HIGH, ring_pa + lay.used_off as u64),
    ] {
        vio_wr(regs, lo, pa as u32);
        vio_wr(regs, hi, (pa >> 32) as u32);
    }
    vio_wr(regs, vio::mmio::Q_READY, 1);
    if vio_rd(regs, vio::mmio::Q_READY) != 1 {
        return None;
    }
    Some(VioDma { vmo, lay, buf_pa })
}

/// vgpu 后端实例（资源/队列/设备面就绪后的常驻句柄）。
struct Vgpu {
    regs: *mut u32,
    dma: VioDma,
    w: u32,
    h: u32,
    /// 停刷位：一次提交通道失败即置——fb 窗同段 ramfb 扫描保屏活（坏设备
    /// 不得杀呈现链），后续提交短路。
    dead: bool,
    /// v2 异步面：已发布命令总数（自由运行 u16，与 used.idx 同口径环绕
    /// 对齐——reap 的期望值源）。init 四命令经 cmd_sync 逐条抬入。
    published: u16,
    /// 在途批命令数（0 = 闲——reap 短路；发车前必先收上一批 = DMA 缓冲
    /// 复用的正确性序）。
    pending: u16,
    /// 在途批 fence id（FLUSH 回执回显对拍锚——0 = 无在途）。
    pending_fence: u64,
    /// fence 单调源（1 起）。
    fence_seq: u64,
    /// v2 一次性串口观测位：per-rect 首批发车 / async 首次回收（证据面零
    /// 常态噪声——per-frame 打点会经 console 管道自激成环，立法禁入）。
    log_rects: bool,
    log_reap: bool,
}

impl Vgpu {
    /// 同步命令车（命令/响应对——QEMU 2D 全命令即答，used ring 收割即
    /// 背压）：调用方已把命令帧编进 VIO_CMD（长 cmd_len）→ 落 DMA → 间接
    /// 链 [cmd|resp] 构表落 DMA → ring desc[0] = INDIRECT 头 → 整 ring 页
    /// 落 DMA → dmb ish → 门铃 → poll used.idx 前进 → 读回响应（24B 头
    /// 分流，DISPLAY_INFO 补整帧）。任一面失败 None（调用方定死法）。
    fn cmd_sync(&mut self, cmd_len: usize) -> Option<vgp::Resp> {
        // SAFETY: 单线程独占窗（模块头 statics 注）。
        let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(VIO_RING) };
        let ind: &mut [u8; CHAIN_MAX * vio::DESC_SZ] =
            unsafe { &mut *core::ptr::addr_of_mut!(VIO_IND) };
        let cmd: &[u8; 64] = unsafe { &*core::ptr::addr_of!(VIO_CMD) };
        let ents = [
            ChainEnt { addr: self.dma.buf_pa + CMD_OFF, len: cmd_len as u32, write: false },
            ChainEnt { addr: self.dma.buf_pa + RESP_OFF, len: RESP_CAP as u32, write: true },
        ];
        let n_ind = chain_put(ind, &ents)?;
        if berry_vmo_write(self.dma.vmo, cmd.as_ptr(), DMA_BUF_OFF + CMD_OFF, cmd_len) != 0
            || berry_vmo_write(self.dma.vmo, ind.as_ptr(), DMA_BUF_OFF + IND_OFF, n_ind) != 0
        {
            return None;
        }
        let _ = vio::put_desc(
            ring,
            &self.dma.lay,
            0,
            self.dma.buf_pa + IND_OFF,
            n_ind as u32,
            vio::DESC_F_INDIRECT,
            0,
        );
        let expect = {
            let old = vio::get_u16(ring, self.dma.lay.avail_off + 2)?;
            vio::publish(ring, &self.dma.lay, 0)?;
            old.wrapping_add(1)
        };
        // v2 记账：单命令发布入总数（reap 的 used.idx 对齐期望——cmd_sync
        // 自身仍同步等完，pending 恒不加）。
        self.published = self.published.wrapping_add(1);
        if berry_vmo_write(self.dma.vmo, ring.as_ptr(), DMA_RING_OFF, 4096) != 0 {
            return None;
        }
        // SAFETY: 纯屏障指令（vblk 同款——Normal 写 → Device 门铃写排序）。
        unsafe { core::arch::asm!("dmb ish") };
        vio_wr(self.regs, vio::mmio::Q_NOTIFY, VIO_CTRLQ);
        // poll used.idx 前进（10s 帽 1ms 步进）。
        let mut now = 0u64;
        if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
            return None;
        }
        let deadline = now + VIO_POLL_CAP_NS;
        loop {
            let mut u = [0u8; 2];
            if berry_vmo_read(
                self.dma.vmo,
                u.as_mut_ptr(),
                DMA_RING_OFF + self.dma.lay.used_off as u64 + 2,
                2,
            ) != 0
            {
                return None;
            }
            if u[0] as u16 | (u[1] as u16) << 8 == expect {
                break;
            }
            if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 || now >= deadline {
                return None;
            }
            if berry_thread_sleep(now + VIO_POLL_STEP_NS) != 0 {
                return None;
            }
        }
        // 整洁面：中断状态确认（poll 模型 ACK 清之——vblk 同款）。
        let isr = vio_rd(self.regs, vio::mmio::INT_STATUS);
        if isr != 0 {
            vio_wr(self.regs, vio::mmio::INT_ACK, isr);
        }
        // 读回响应：24B 头分流（DISPLAY_INFO 补整帧；NODATA/错误族恰 24B）。
        let resp: &mut [u8; RESP_CAP] = unsafe { &mut *core::ptr::addr_of_mut!(VIO_RESP) };
        if berry_vmo_read(self.dma.vmo, resp.as_mut_ptr(), DMA_BUF_OFF + RESP_OFF, 24) != 0 {
            return None;
        }
        let kind = u32::from_le_bytes(resp[0..4].try_into().ok()?);
        let full = if kind == vgp::RESP_OK_DISPLAY_INFO { RESP_CAP } else { 24 };
        if berry_vmo_read(self.dma.vmo, resp.as_mut_ptr(), DMA_BUF_OFF + RESP_OFF, full) != 0 {
            return None;
        }
        vgp::parse_resp(&resp[..full])
    }

    /// v2 停刷出口（通道死——单行诊断 + dead；fb 窗同段 ramfb 扫描保屏活，
    /// 服务不死，v1 `dead` 语义延承）。返回 false 供发车路径短路。
    fn die(&mut self, why: &str) -> bool {
        self.dead = true;
        self.pending = 0; // 停刷后不再回收（缓冲不再复用）
        eprintln!("guifbd: vgpu 提交通道停刷（{}）", why);
        false
    }

    /// v2 回收（发车/回执拆分的回执半——「响应桶延迟消费」）：等 used.idx
    /// 追平 published（10s 帽 1ms 步进，v1 cmd_sync 同参）→ 中断 ACK →
    /// 响应桶单次读回（pending×24B）→ 逐命令 OK + 末命令（FLUSH）fence
    /// 回显校验。QEMU 2D 控制队列 BH 单轮吃全批（`virtio_gpu_handle_ctrl`
    /// while-pop），发车到回执 = 一个 BH 往返——**正常态下帧间 CPU 工作
    /// （blit/编帧/应答）已把等待摊薄到零**（12 号 §4.3 爆点的拆法）。
    /// fence 机制定谳：QEMU 2D 即答路径不挂起（fence 位只回显 fence_id 进
    /// 回执——`virtio_gpu_ctrl_response`），即发即收、无第二时间轴。
    fn reap(&mut self) -> bool {
        if self.pending == 0 {
            return true; // 闲态短路（init 后首帧/停刷后）
        }
        let t0 = now_ns();
        let mut now = 0u64;
        if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
            return self.die("v2 clock");
        }
        let deadline = now + VIO_POLL_CAP_NS;
        loop {
            let mut u = [0u8; 2];
            if berry_vmo_read(
                self.dma.vmo,
                u.as_mut_ptr(),
                DMA_RING_OFF + self.dma.lay.used_off as u64 + 2,
                2,
            ) != 0
            {
                return self.die("v2 used 读回");
            }
            if u16::from_le_bytes(u) == self.published {
                break;
            }
            if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 || now >= deadline {
                return self.die("v2 reap 超时");
            }
            if berry_thread_sleep(now + VIO_POLL_STEP_NS) != 0 {
                return self.die("v2 reap sleep");
            }
        }
        // 整洁面：中断状态确认（poll 模型 ACK 清之——vblk/v1 同款；批单次）。
        let isr = vio_rd(self.regs, vio::mmio::INT_STATUS);
        if isr != 0 {
            vio_wr(self.regs, vio::mmio::INT_ACK, isr);
        }
        // 响应桶延迟消费：单次读回 + 逐命令校验（错误族/缺 fence 回显 → 停刷）。
        let n = self.pending as usize;
        // SAFETY: 单线程独占窗（模块头 statics 注）。
        let resp: &mut [u8; FRAME_CMD_MAX * vgp::RESP_OK_NODATA_SZ] =
            unsafe { &mut *core::ptr::addr_of_mut!(VIO_ARESP) };
        if berry_vmo_read(
            self.dma.vmo,
            resp.as_mut_ptr(),
            DMA_BUF_OFF + ARESP_OFF,
            n * vgp::RESP_OK_NODATA_SZ,
        ) != 0
        {
            return self.die("v2 回执桶读回");
        }
        if !batch_resps_ok(&resp[..], n, self.pending_fence) {
            return self.die("v2 回执校验");
        }
        // 一次性观测行：async 首次回收的实等时长（爆点拆除效果的直接读数
        // ——正常态 ≈0：设备在帧间已完成）。
        if !self.log_reap {
            self.log_reap = true;
            let wait = now_ns().wrapping_sub(t0);
            println!("VGPU2 reap ns={} k={}", wait, n);
        }
        self.pending = 0;
        true
    }

    /// v2 帧提交（per-rect + 异步发车——12 号 §4.3 模型）：
    ///
    /// - **矩形集**：full → 单全帧矩形（v1 等形回退路径——SUBMIT FULL/
    ///   console/init 首推共用；console 路 Fbcon 光标私有零脏区上报，v2
    ///   保持全帧形但获异步化收益——真 console per-rect 挂 v3，前置 =
    ///   berry-fbcon 脏区面）；K>0 → K×TRANSFER（逐矩形，offset =
    ///   y×stride+x×4）+ 1×FLUSH（包络——协议 FLUSH 单矩形无 N-rect 数组
    ///   形，Linux `virtgpu_plane.c` 包络同构；合并挂 v3）。
    /// - **发车不等回执**：编帧（statics）→ 回收上帧 → 构链落 DMA →
    ///   publish 全批 → **单次门铃** → 返回。CPU 编帧与设备处理上帧重叠
    ///   （2-deep：上一帧回执在本帧发车前收——爆点拆出单帧外）。
    /// - **背压定谳（如实注）**：单帧在途 ≤ FRAME_CMD_MAX=9 ≤ qsize=16，
    ///   发车前必先 reap——virtqueue 满**结构不可达**，无需同步退化档；
    ///   更深流水线（多帧在途+缓冲池+信用记账）挂 v3。
    /// - 全矩形界外（裁剪空集）= blit 零写入 → 零提交（「无 damage 不
    ///   提交」的设备侧同构执法）。
    fn submit_frame(&mut self, full: bool, rects: &[gp::Rect], rect_count: usize) {
        if self.dead {
            return;
        }
        // ① gp→vgp 矩形换面（full = 单全帧矩形）。
        let mut vr = [vgp::Rect::new(0, 0, 0, 0); FLUSH_RECT_MAX];
        let n_src = if full {
            vr[0] = vgp::Rect::new(0, 0, self.w, self.h);
            1
        } else {
            let k = rect_count.min(FLUSH_RECT_MAX);
            for i in 0..k {
                vr[i] = vgp::Rect::new(rects[i].x, rects[i].y, rects[i].w, rects[i].h);
            }
            k
        };
        // ② 裁剪 + 包络（guest blit 路同构交域语义）。
        let mut cl = [vgp::Rect::new(0, 0, 0, 0); FLUSH_RECT_MAX];
        let Some((kept, env)) = rects_clip_into(&mut cl, &vr[..n_src], self.w, self.h) else {
            self.die("v2 裁剪超容"); // 理论不可达（wire 帽 ≤ DAMAGE_MAX 互钉）
            return;
        };
        if kept == 0 {
            return; // 全界外：blit 零写入——无 damage 不提交
        }
        // ③ 编帧批（statics——与设备处理上帧重叠）。
        let fence = self.fence_seq.wrapping_add(1);
        // SAFETY: 单线程独占窗（模块头 statics 注）。
        let cmds: &mut [u8; FRAME_CMD_MAX * ACMD_SZ] =
            unsafe { &mut *core::ptr::addr_of_mut!(VIO_ACMD) };
        let Some(n) = frame_flush_batch(cmds, VIO_RES, self.w, self.h, &cl[..kept], &env, fence)
        else {
            self.die("v2 批编帧");
            return;
        };
        // ④ 回收上帧回执（发车前——DMA 缓冲复用的正确性序）。
        if !self.reap() {
            return;
        }
        // ⑤ 构链：每命令间接链 [cmd_i | resp_i]（resp 24B 设备写槽 = 桶位）。
        let ind: &mut [u8; FRAME_CMD_MAX * 2 * vio::DESC_SZ] =
            unsafe { &mut *core::ptr::addr_of_mut!(VIO_AIND) };
        for i in 0..n {
            // 槽 i < n-1 = TRANSFER（56B）；末槽 = FLUSH（48B）。
            let cmd_len = if i + 1 < n {
                vgp::CMD_TRANSFER_TO_HOST_2D_SZ
            } else {
                vgp::CMD_RESOURCE_FLUSH_SZ
            };
            let ents = [
                ChainEnt {
                    addr: self.dma.buf_pa + ACMD_OFF + (i as u64) * ACMD_SZ as u64,
                    len: cmd_len as u32,
                    write: false,
                },
                ChainEnt {
                    addr: self.dma.buf_pa + ARESP_OFF + (i as u64) * vgp::RESP_OK_NODATA_SZ as u64,
                    len: vgp::RESP_OK_NODATA_SZ as u32,
                    write: true,
                },
            ];
            if chain_put(&mut ind[i * 2 * vio::DESC_SZ..(i + 1) * 2 * vio::DESC_SZ], &ents)
                .is_none()
            {
                self.die("v2 构链");
                return;
            }
        }
        // ⑥ 落 DMA（两写——v1 每命令三写 × n 命令 → 批恒两写）。
        if berry_vmo_write(
            self.dma.vmo,
            cmds.as_ptr(),
            DMA_BUF_OFF + ACMD_OFF,
            n * ACMD_SZ,
        ) != 0
            || berry_vmo_write(
                self.dma.vmo,
                ind.as_ptr(),
                DMA_BUF_OFF + AIND_OFF,
                n * 2 * vio::DESC_SZ,
            ) != 0
        {
            self.die("v2 DMA 写");
            return;
        }
        // ⑦ ring：desc[i] = INDIRECT 头（指本命令表）→ publish 全批 → 整
        // ring 页单写 → dmb → **单次门铃**（QEMU ctrl BH 一轮吃全批）。
        let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(VIO_RING) };
        for i in 0..n {
            if vio::put_desc(
                ring,
                &self.dma.lay,
                i,
                self.dma.buf_pa + AIND_OFF + (i as u64) * (2 * vio::DESC_SZ) as u64,
                (2 * vio::DESC_SZ) as u32,
                vio::DESC_F_INDIRECT,
                0,
            )
            .is_none()
                || vio::publish(ring, &self.dma.lay, i as u16).is_none()
            {
                self.die("v2 ring 构表");
                return;
            }
        }
        self.published = self.published.wrapping_add(n as u16);
        if berry_vmo_write(self.dma.vmo, ring.as_ptr(), DMA_RING_OFF, 4096) != 0 {
            self.die("v2 ring 写");
            return;
        }
        // SAFETY: 纯屏障指令（vblk/v1 同款——Normal 写 → Device 门铃写排序）。
        unsafe { core::arch::asm!("dmb ish") };
        vio_wr(self.regs, vio::mmio::Q_NOTIFY, VIO_CTRLQ);
        self.pending = n as u16;
        self.pending_fence = fence;
        self.fence_seq = fence;
        // 一次性观测行：per-rect 首用（K + 包络脏区率 r‰——12 号 §4 r 模型
        // 的串口读数源；per-frame 打点自激成环，立法一次性）。
        if !full && !self.log_rects {
            self.log_rects = true;
            let env_area = env.width as u64 * env.height as u64;
            let full_area = self.w as u64 * self.h as u64;
            let permille = env_area * 1000 / full_area.max(1);
            println!(
                "VGPU2 rect k={} env={}x{}+{}+{} r={}\u{2030}",
                kept, env.width, env.height, env.x, env.y, permille
            );
        }
    }

    /// v2 全帧形入口（console 路 / init 首推 / SUBMIT FULL——v1 `flush_full`
    /// 的异步化等形）。
    fn submit_full(&mut self) {
        self.submit_frame(true, &[], 0);
    }
}

/// 初始化链（lib `INIT_SEQ` 四命令——SET_SCANOUT 殿后：QEMU 对 scanout
/// 资源 require_backing，序反即 RESP_ERR 族）。每步失败单行诊断 + None
/// （调用方落 ramfb 保活；gpu 相 `FB ok vgpu` 缺席 = smoke FAIL loud）。
fn vio_init(w: usize, h: usize, stride: usize, fmt_raw: u32, fb_pa: u64) -> Option<Vgpu> {
    let Some(slot) = vio_scan() else {
        return None; // 无设备静默（ramfb 红线）；异常面 vio_scan 自打诊断
    };
    // 几何门两道（**scan 后**——无设备相零输出红线的构造保证）：v1 packed
    // stride（QEMU 侧资源 pixman stride = w×4，非 packed 几何挂 v2）+ BGRX
    // 直配单档（ramfb fmt=1 同字节序零转换）。
    if stride != w * 4 {
        eprintln!("guifbd: vgpu stride={} 非 packed（v1 只支持 w×4）", stride);
        return None;
    }
    let Some(gpu_fmt) = vgp::ramfb_fmt_to_gpu(fmt_raw) else {
        eprintln!("guifbd: vgpu fmt={} 非 BGRX 直配档", fmt_raw);
        return None;
    };
    let regs = vio_regs(slot);
    if !vio_negotiate(regs) {
        eprintln!("guifbd: vgpu init fail: 协商（VERSION_1+INDIRECT）");
        return None;
    }
    let Some(dma) = vio_setup_queue(regs) else {
        eprintln!("guifbd: vgpu init fail: 控制队列建制");
        return None;
    };
    // 规范序：队列建制后 DRIVER_OK（此后设备可消费 avail——vblk 同位）。
    vio_wr(
        regs,
        vio::mmio::STATUS,
        vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK | vio::ST_DRIVER_OK,
    );
    let mut v = Vgpu {
        regs,
        dma,
        w: w as u32,
        h: h as u32,
        dead: false,
        published: 0,
        pending: 0,
        pending_fence: 0,
        fence_seq: 0,
        log_rects: false,
        log_reap: false,
    };
    let cmd: &mut [u8; 64] = unsafe { &mut *core::ptr::addr_of_mut!(VIO_CMD) };
    // ① GET_DISPLAY_INFO（带载荷响应——传输往返首验；pmode[0] 只记日志
    // 不执法：v1 几何随 fbinfo（GOP 基线同款），分辨率动态面挂 v2）。
    let n = vgp::encode_get_display_info(cmd, &vgp::CtrlHdr::new(vgp::CMD_GET_DISPLAY_INFO))?;
    let (pw, ph) = match v.cmd_sync(n) {
        Some(vgp::Resp::OkDisplayInfo(_, info)) => {
            (info.pmodes[0].r.width, info.pmodes[0].r.height)
        }
        _ => {
            eprintln!("guifbd: vgpu init fail: GET_DISPLAY_INFO");
            return None;
        }
    };
    // ② RESOURCE_CREATE_2D（scanout 尺寸 BGRX）。
    let n = vgp::encode_resource_create_2d(
        cmd,
        &vgp::CtrlHdr::new(vgp::CMD_RESOURCE_CREATE_2D),
        &vgp::Create2d { resource_id: VIO_RES, format: gpu_fmt, width: w as u32, height: h as u32 },
    )?;
    if !matches!(v.cmd_sync(n), Some(vgp::Resp::OkNodata(_))) {
        eprintln!("guifbd: vgpu init fail: RESOURCE_CREATE_2D");
        return None;
    }
    // ③ ATTACH_BACKING（fb 窗连续物理段单 entry——lib `attach_entry` 段
    // 切面；per-page 表挂 v2 动态分辨率刀）。
    let Some((addr, len)) = attach_entry(fb_pa, (stride * h) as u64) else {
        eprintln!("guifbd: vgpu init fail: backing 段切");
        return None;
    };
    let n = vgp::encode_resource_attach_backing(
        cmd,
        &vgp::CtrlHdr::new(vgp::CMD_RESOURCE_ATTACH_BACKING),
        VIO_RES,
        &[vgp::MemEntry { addr, length: len }],
    )?;
    if !matches!(v.cmd_sync(n), Some(vgp::Resp::OkNodata(_))) {
        eprintln!("guifbd: vgpu init fail: ATTACH_BACKING");
        return None;
    }
    // ④ SET_SCANOUT（r ≥16×16——QEMU `virtio_gpu_do_set_scanout` 执法）。
    let n = vgp::encode_set_scanout(
        cmd,
        &vgp::CtrlHdr::new(vgp::CMD_SET_SCANOUT),
        &vgp::SetScanout {
            r: vgp::Rect::new(0, 0, w as u32, h as u32),
            scanout_id: VIO_SCANOUT,
            resource_id: VIO_RES,
        },
    )?;
    if !matches!(v.cmd_sync(n), Some(vgp::Resp::OkNodata(_))) {
        eprintln!("guifbd: vgpu init fail: SET_SCANOUT");
        return None;
    }
    println!("FB ok vgpu slot={} pmode={}x{}", slot, pw, ph);
    Some(v)
}

pub(crate) fn main() {
    // —— B1-b+c：boot 交接（模块头注①）——svcmgr 预拉本服务后把 virtio
    // 窗映进本空间 VIRTIO_MMIO_VA，经 stdin 通道发 'w'/'n' 单字节。**首
    // syscall = 读该字节**：窗未映先碰 = D-abort 杀服务，读字节是序保证
    // 非数据面。旧 svcmgr（feeder EOF）/交接破缺 → None → Ramfb（缺省
    // 安全：交接面任何破绽落现役行为，版本混布防御）。
    let backend = {
        let mut b = [0u8; 1];
        // SAFETY: libc read 桩（fd 0 → stdin 通道 recv，阻塞 10s 帽内置
        // ——bsh readline 同链）；1 字节读无越界面。
        let n = read(0, b.as_mut_ptr() as *mut core::ffi::c_void, 1);
        backend_from_handoff(if n == 1 { Some(b[0]) } else { None })
    };
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
    let (fb_pa, w, h, stride) = (rd(0), rd(1), rd(2), rd(3));
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
    // —— B1-b：vgpu 后端起链（交接 'w' ∧ 扫窗见设备才走——Ramfb 相此段
    // 整体短路 = 现役零漂移）。资源 backing = fb 窗**同一物理段**：blit/
    // 蜂格/px() 读回路径全体零改动（ramfb 与 vgpu 双设备同屏一致）。起链
    // 即首推一帧（自验三步的屏面上 gpu console）。
    let vgpu = match backend {
        Backend::Vgpu => vio_init(w, h, stride, fmt_raw, fb_pa as u64).map(|mut v| {
            v.submit_full();
            v
        }),
        Backend::Ramfb => None,
    };
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
    let mut srv = Srv::new(con, w, h, stride, gui, vgpu);
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
