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
//! `berry_object_wait_many` 并等〔wm 面刀起第四路 + n=4——见下方批B 段
//! 复活注〕；deadline = min(下次软 vsync 点,
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
//!    标记）；没找到 → **Ramfb 零漂移**（b1bc 迁移期形态：现役路径逐字
//!    节不变、无设备相无新输出无新 syscall 面〔交接读除外，零可见差〕；
//!    波3 批B 起带裁剪/缓存/重放为两后端等价的新功能面〔ramfb 相同执
//!    法、BANDC/BANDR 新输出〕——红线现行语义 = vgpu 钩子面 None 恒短
//!    路零开销）。
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
//!    即答路径回执回显 fence_id 作批身份正向锚）。ramfb 红线零漂移：本段
//!    （B1-b/c vgpu 迁移块）全部改动在 vgpu 分支内；批B 起带缓存/带重放
//!    是两后端等价的新功能面（ramfb 相照跑缓存 seed 与重放——属设计内
//!    行为变更非漂移，见下方批B 段与笔记）。
//!
//! **波3 批B（guifbd 带合成 v1）**：带缓存/带重放 + surface 分类（lib
//! `classify_surface` 六档）驱动裁剪 / 带缓存双写（bounce 同拍抄录 + 补
//! 不透明键 0xFF——guifbd blit 是无透明语义的原样字节拷，已呈现即不透
//! 明；批C 真 ARGB 生产者落地时由该刀翻转补键规则）/ 带重放（CPU blit
//! 毕、vgpu 钩子前，SERVICE→SYSTEM 序，A==0 跳写）；带缓存 boot 一次性
//! 堆分配（`band_cache_bytes` 4MiB 门，失败禁用带面零回归——**绝不能入
//! Srv/栈：guifbd 栈仅 32KiB**）。标记 `BANDC ok` / `BANDR ok` 各恰一次
//! （one-shot latch）。wm 客户端面曾**收线**（批B 对抗审查 CONFIRMED：
//! dup(WMD_CLI_DUP) 与 launcher/gfxdemo/[ui] apps 同属一条 ChannelEnd，
//! wmd 应答无相关性 ID、单 FIFO 先 recv 者得，双消费者应答互截——交接
//! 已撤回，快照恒空 = 一切 wid 按 AppCompat 保守档、带集 = fallback 底
//! 带）；wm 订阅私有通道 RPC 面刀（2026-11-18）已**复活**该面：svcmgr
//! 代发 WM_SUBSCRIBE 后经交接对投 'm'+订阅者事件通道 cli 端，guifbd 即
//! 该私有通道唯一客户端（「单 wm 客户端」前提结构性恢复），Table 通知
//! 驱动 TABLE_GET 异步拉表、快照真值接入 classify/plan_bands（WinBand/
//! WinApp/Hidden 档随表激活）、`WMSUB ok` 首快照真值接入拍 latch——
//! 因果与合同见 plan .agents/plans/2026-11-17-wm订阅私有通道RPC面.md。

use berry_exec::{FB_VA, VIRTIO_MMIO_SLOTS, VIRTIO_MMIO_STRIDE, VIRTIO_MMIO_VA};
use berry_fbcon::font::{self, GLYPH_H};
use berry_fbcon::render::{Fbcon, PixelFormat};
use berry_guiproto as gp;
use berry_libc::io::{gift2_h, gift3_h, hdi_srv_h, present_srv_h, read, stdin_h};
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
    attach_entry, backend_from_handoff, band_cache_bytes, band_replay_plan, batch_resps_ok,
    chain_put, classify_surface, frame_flush_batch, plan_bands, rects_clip_into, replay_rows_u32,
    Backend, BandPlan, ChainEnt, SurfaceClass, BAND_H_CAP, BAND_SLOTS, CHAIN_MAX, ACMD_OFF,
    ACMD_SZ, AIND_OFF, ARESP_OFF, CMD_OFF, DMA_BUF_OFF, DMA_RING_OFF, FLUSH_RECT_MAX,
    FRAME_CMD_MAX, IND_OFF, RESP_CAP, RESP_OFF,
};

/// 批矩形帽互钉：lib 批形状常量 = wire damage 帽（两 crate 均冻结——值
/// 漂移即编译炸响，防线在编译期非运行期）。
const _: () = assert!(gp::DAMAGE_MAX == FLUSH_RECT_MAX);

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
/// wm 'm' 交接等待帽（AwaitHandle 态终界）：svcmgr wmsub 代发在 boot 走查
/// 窗同步完成（gfxdemo arm 前），帽到 = 代发失败/未达面——close stdin +
/// Disabled loud 一次降级（快照恒 fallback 真值，WMSUB 门缺失交 smoke
/// 兜底）。
const WM_AWAIT_CAP_NS: u64 = 10 * 1_000_000_000;
/// vblank event 信号位（用户半默认位——跨进程契约：客户端 REG_VBLANK 前
/// 自建 event 并在同位等待；bsh/bench1 家族先例，不登记内核
/// USER_SIGNAL_BITS——用户私有位无内核模板面）。
const VBLANK_SIG: u64 = 1 << 32;
/// token 环深（FRAME_CALLBACK 校验窗口——单客户端 v1 足够覆盖在途 8 帧）。
const TOKEN_RING: usize = 8;
// 波3-件② 系统面 surface_id marker（0xFFFF）单源已收 lib.rs `SYS_SURFACE_ID`
//（批B 接线：分类入 lib `classify_surface`，本模块不再持有本地副本）。

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

/// 兼容档带几何单源（AppCompat 裁剪 + [`Srv::band_replan`] fallback 底带
/// 共此——批B 前散在 blit_and_sample 内联）。几何镜像 launcher 实值单源：
/// big 档 band_top=117px = appshell scene.rs:34 OH_STATUS_H 39vp×
/// OH_DENSITY(:31)=3、band_bot=135px = desktop.rs:151 NAV_BAR_H_BIG；紧凑
/// 档 44/44px = scene.rs:130 紧凑 44 / desktop.rs:146 NAV_KEY_PX=44。互指
/// 防漂移——改值须与 launcher 真源双侧同步。满档判据同源 vp>=600×1800。
fn band_geometry(w: usize, h: usize) -> (usize, usize) {
    let big = w >= 600 && h >= 1800;
    if big { (117, 135) } else { (44, 44) }
}

/// 快照表项零值（WmFace 定容数组初始化——count 前缀外恒零，非语义面）。
fn wm_entry_zero() -> gp::WmEntry {
    gp::WmEntry {
        id: 0,
        parent: 0,
        group: 0,
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        layer: 0,
        z: 0,
        flags: 0,
    }
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

/// wm 私有通道三态（无 Subscribing 中间态——订阅由 svcmgr wmsub 代发
/// 〔wmd arm Ok 路静默窗，gui-wmd subscribe 应答的 cli 端经 guifbd 交接
/// 对随 'm' 投递〕，guifbd 只收不发 WM_SUBSCRIBE）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum WmPrivState {
    /// 等 'm' 交接（serve 循环非阻塞收 stdin；10s 帽到 → Disabled）。
    AwaitHandle,
    /// 已接订阅者事件通道：drain 通知/应答 + dirty 拉表。
    Active,
    /// 终态降级（帽到/畸形/防御面）——无臂；快照冻结在最后真值（从未
    /// 接入 = 恒空，classify 全归 AppCompat、带集 = fallback 底带）。
    Disabled,
}

/// wm 表跟随面（私有通道 RPC 版——批B 收线面复活）：svcmgr 代发
/// WM_SUBSCRIBE 后经 guifbd 交接对（stdin 通道）投 'm'+订阅者事件通道
/// cli 端，本服务即该通道唯一客户端——「单 wm 客户端」前提结构性成立，
/// 批B 共享队列双消费者互截面（dup(WMD_CLI_DUP) 应答互截）不复存在
/// （史与因果：plan .agents/plans/2026-11-17-wm订阅私有通道RPC面.md）。
/// Table 通知驱动 TABLE_GET 异步拉表（单飞 inflight 门）；快照真值接入
/// classify/plan_bands——WinBand/WinApp/Hidden 档随表激活；`WMSUB ok`
/// 首快照真值接入拍 latch 恰一次（count=0 空表也算面活，打点属主在
/// guifbd）。
struct WmFace {
    /// 快照表（前 count 项有效；首快照接入前恒空）。
    entries: [gp::WmEntry; gp::WM_WINDOWS_MAX],
    count: usize,
    /// 订阅者事件通道 cli 端（Active 起；随 'm' 交接迁入，持全四权直接
    /// 收发）。
    evt: Handle,
    state: WmPrivState,
    /// 已接入快照的表版本（Table(v) ≠ version → dirty；wmd 恒发 ≥1）。
    version: u32,
    /// TABLE_GET 已发未答（单飞门——至多一请求在途；应答全量真值幂等，
    /// 迟到应答照接）。
    get_inflight: bool,
    /// 表已脏待拉（'m' 接入拍 + Table 通知拍置位；应答拍/发送拍清）。
    dirty: bool,
    /// AwaitHandle 10s 帽（Srv::new 起算，绝对 ns）。
    await_deadline_ns: u64,
    /// `WMSUB ok` one-shot latch。
    wmsub_latched: bool,
    /// 畸形/错误应答 loud 一次位（不刷屏；下拍 Table 通知再臂自愈）。
    bad_rep_loud: bool,
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
    /// 波3 批B 带合成面：surface 分类（lib `classify_surface` 六档——SUBMIT
    /// 入途时按本地快照分类随行；P5 面 = InternalRefresh）——latch 呈现时
    /// 随行进 blit_and_sample 共尾。
    class: SurfaceClass,
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
    /// B1-b：vgpu 后端实例（None = ramfb 单后端——现役路径）。console/
    /// SUBMIT 写路径后的全帧 TRANSFER+FLUSH 钩子面（ramfb 相恒短路）。
    vgpu: Option<Vgpu>,
    /// SUBMIT 行弹舱（vmo_read 行读出 → 直写 fb；帽 = ROW_BOUNCE_MAX）。
    bounce: [u8; ROW_BOUNCE_MAX],
    // —— 波3 批B：wm 快照持有者 + 带缓存/带重放 ——
    /// wm 表跟随面（私有通道版——见 `WmFace` 注；Disabled/未接入 = 快照
    /// 恒空，wid 全归 AppCompat）。
    wm: WmFace,
    /// 当前带集（`plan_bands` 产物——双调用点：boot 拍 fallback 底带起步
    /// + 快照接入拍随真值重算）。
    bands: [Option<BandPlan>; BAND_SLOTS],
    /// 带缓存（boot 一次性堆分配——Box::leak 常驻段，绝不能入 Srv/栈
    /// 32KiB；None = 带面禁用：裁剪与现网行为不变，仅无缓存/重放）。
    band_cache: Option<&'static mut [u32]>,
    /// plan_bands warn loud 一次位（超带高/带数帽）。
    band_warn_loud: bool,
    /// `BANDC ok` one-shot latch（带缓存首 seed——任一槽 ≥1 行恰一次）。
    bandc_latched: bool,
    /// `BANDR ok n=` one-shot latch（首重放写数 ≥1 恰一次）。
    bandr_latched: bool,
}

impl<'a> Srv<'a> {
    fn new(
        con: Fbcon<'a>,
        w: usize,
        h: usize,
        stride: usize,
        gui: bool,
        vgpu: Option<Vgpu>,
        cfg: gp::softpanel::PanelCfg,
        boot_hz: u16,
        band_cache: Option<&'static mut [u32]>,
    ) -> Self {
        // 语义层校验/回退先走（loud 诊断行在 boot_panel_tier），SoftPanel
        // 构造内自查表落周期——period 首值不外传；仲裁器同 boot 档起（A11
        // 无票缺省 = 平台表达档）。
        let (hz, _) = boot_panel_tier(&cfg, boot_hz);
        let arb = gp::arbiter::Arbiter::new(&cfg, hz);
        let mut srv = Srv {
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
            vgpu,
            bounce: [0; ROW_BOUNCE_MAX],
            // wm 面：AwaitHandle 起步（'m'+订阅者 cli 端由 svcmgr boot 走查
            // 窗投递交接——poll_wm 臂接收），10s 帽同拍起算；带集 = fallback
            // 底带（band_replan boot 拍——快照接入拍重算是双调用点之二）；
            // 缓存 None 时双写/重放全短路（带面禁用零回归）。
            wm: WmFace {
                entries: [wm_entry_zero(); gp::WM_WINDOWS_MAX],
                count: 0,
                evt: 0,
                state: WmPrivState::AwaitHandle,
                version: 0,
                get_inflight: false,
                dirty: false,
                await_deadline_ns: now_ns() + WM_AWAIT_CAP_NS,
                wmsub_latched: false,
                bad_rep_loud: false,
            },
            bands: [None; BAND_SLOTS],
            band_cache,
            band_warn_loud: false,
            bandc_latched: false,
            bandr_latched: false,
        };
        // boot 拍落带集（fallback 底带起步——快照真值接入拍〔wm_drain〕
        // 重算是 band_replan 双调用点之二；窗口期 WinApp 重放可能读到旧
        // 窗带像素，v1 接受——下次 SysFace/WinBand 提交 A 位恒写覆盖自愈）。
        srv.band_replan();
        srv
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
        // 带裁剪/vgpu 钩子/带缓存双写/带重放统一在 blit_and_sample 共尾
        //——surface 分类随行（P2 latch 呈现与 SUBMIT 即时面同口径）。
        let (read_ok, all_pass, n_pass) =
            self.blit_and_sample(pf.vmo, pf.full, &pf.rects, pf.rect_count, pf.class);
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

    /// wm 私有通道轮询（serve 第四路；**死锁红线**：SUBMIT/COMMIT 处理
    /// 路径内零 wm 通道 send/recv/wait——本臂与下属全部 wm 面只在 serve
    /// 循环）。AwaitHandle = 'm' 交接接收 + 帽检查；Active = drain 通知/
    /// 应答 + dirty 拉表；Disabled 无臂。返回本拍是否处理过（处理过 =
    /// 回环顶重询，poll-first 无丢唤醒）。
    fn poll_wm(&mut self) -> bool {
        let acted = match self.wm.state {
            WmPrivState::AwaitHandle => self.wm_await(),
            WmPrivState::Active => self.wm_drain(),
            WmPrivState::Disabled => false,
        };
        self.wm_pull(); // Active 拉表臂（内部三重门自守；新转入同拍首拉）
        acted
    }

    /// wm 'm' 交接接收（AwaitHandle 态——poll_wm 臂内）：非阻塞
    /// `berry_channel_recv(stdin_h(), …双出参)`（read(0) 桩无 handle 出参
    /// 窗，交接字节 'w'/'n' 已被 boot 首读吃掉——'m' 是交接通道内第二条
    /// 消息恰序在后）。收到 'm'+恰 1 handle（svcmgr wmsub 代发合同）→
    /// evt 落值 + close stdin + 转 Active + dirty 即刻首拉；10s 帽到/畸形
    /// → close stdin + Disabled loud 一次。
    fn wm_await(&mut self) -> bool {
        if now_ns() >= self.wm.await_deadline_ns {
            self.wm_disable("await cap");
            return true;
        }
        let hstdin = stdin_h();
        if hstdin == 0 {
            // 老 spawn 面（env 块无 stdin 通道端——handle 0 恒非法值）：
            // 'm' 不可达，本臂零 recv（-EBADF trap 面）；10s 帽到自落
            // Disabled（版本混布防御：Ramfb + fallback 带照跑）。
            return false;
        }
        let mut msg = [0u8; 256];
        let mut hout = [0u32; 8];
        let mut nb = msg.len();
        let mut nh = hout.len();
        let s = berry_channel_recv(
            hstdin,
            msg.as_mut_ptr(),
            &mut nb,
            hout.as_mut_ptr(),
            &mut nh,
        );
        if s == BERRY_ERR_AGAIN {
            return false;
        }
        if s != 0 {
            trap(); // 非 -EAGAIN 错码 = 交接通道链断（boot 定值接线，真死亡面）
        }
        if nb == 1 && msg[0] == b'm' && nh == 1 {
            // 随行 handle 持全四权（内核 channel_create 两端恒授、权随消息
            // 令牌迁移）——直接可收发，无需先 dup。
            self.wm.evt = hout[0];
            self.wm.state = WmPrivState::Active;
            self.wm.dirty = true;
            let _ = berry_handle_close(stdin_h()); // 交接通道使命毕（无第二读者面）
            return true;
        }
        // 畸形（非 'm' tag / 零/多 handle——svcmgr 合同下双重不可达角）：
        // 随行 handle 收口丢弃 + Disabled。
        close_all(&hout[..nh]);
        self.wm_disable("handoff bad");
        true
    }

    /// wm 事件通道 drain（Active 态——通知+应答同通道同 FIFO）。帧判别先
    /// `parse_wm_evt` 后 `parse_wm_snapshot_rep`（执法域不相交：事件帧首
    /// 字节 0x90..=0x92、快照应答首字节 ST_OK=0，两解析器互斥拒对方帧）。
    /// Table(v) ≠ version → dirty（同版本 = 已是真值，幂等无操作）；
    /// Back/BackAck 防御忽略（guifbd 非返回键消费面）；快照 Ok → 全量真值
    /// 落值（迟到应答接——幂等无害）+ band_replan + `WMSUB ok` latch 恰
    /// 一次；Err/None → loud 一次不重拉（下拍 Table 通知再臂自愈）。返回
    /// 是否处理过帧。
    fn wm_drain(&mut self) -> bool {
        let evt = self.wm.evt;
        let mut acted = false;
        loop {
            let mut msg = [0u8; 256];
            let mut hout = [0u32; 8];
            let mut nb = msg.len();
            let mut nh = hout.len();
            let s = berry_channel_recv(evt, msg.as_mut_ptr(), &mut nb, hout.as_mut_ptr(), &mut nh);
            if s == BERRY_ERR_AGAIN {
                break;
            }
            if s != 0 {
                trap(); // 非 -EAGAIN 错码 = evt 通道链断（boot 定值接线）
            }
            if nh > 0 {
                // 本通道合同零随行 handle（wmd 只发 5B 事件/快照帧）——防御
                // 收口（拒收面 close 随行纪律）。
                close_all(&hout[..nh]);
            }
            acted = true;
            match gp::parse_wm_evt(&msg[..nb]) {
                Some(gp::WmEvt::Table(v)) => {
                    if v != self.wm.version {
                        self.wm.dirty = true;
                    }
                }
                Some(_) => {} // Back/BackAck 防御忽略
                None => match gp::parse_wm_snapshot_rep(&msg[..nb]) {
                    Some(Ok(snap)) => {
                        self.wm.entries = snap.entries;
                        self.wm.count = snap.count;
                        self.wm.version = snap.version;
                        self.wm.get_inflight = false;
                        self.band_replan();
                        if !self.wm.wmsub_latched {
                            self.wm.wmsub_latched = true;
                            println!("WMSUB ok");
                        }
                    }
                    _ => {
                        // Err（1B 错误应答——本通道不可能形）/ None（畸形）：
                        // inflight 清位 + loud 一次 + 不重拉。
                        self.wm.get_inflight = false;
                        if !self.wm.bad_rep_loud {
                            self.wm.bad_rep_loud = true;
                            eprintln!("guifbd: wm rep bad");
                        }
                    }
                },
            }
        }
        acted
    }

    /// wm 拉表臂（Active 态）：dirty ∧ !inflight → TABLE_GET 发 evt 通道。
    /// BUSY → 留拍重试（poll-first 自愈，不等 WRITABLE——通知+应答同队列，
    /// wmd serve 循环每轮 sweep drain 本通道即腾位）；他错码 = 防御 Disabled
    /// 降级（v1 通道无对端死面，理论不可达）。
    fn wm_pull(&mut self) {
        if self.wm.state != WmPrivState::Active || !self.wm.dirty || self.wm.get_inflight {
            return;
        }
        let mut req = [0u8; 4];
        let n = gp::encode_wm_table_get_req(&mut req);
        let s = berry_channel_send(self.wm.evt, req.as_ptr(), n, core::ptr::null(), 0);
        if s == 0 {
            self.wm.dirty = false;
            self.wm.get_inflight = true;
        } else if s != BERRY_ERR_BUSY {
            self.wm_disable("send bad");
        }
        // EBUSY：留拍重试（dirty 保持，下轮 poll_wm 再臂）
    }

    /// wm 面终态降级（Disabled loud 一次，幂等）：AwaitHandle 相 close
    /// stdin、Active 相 close evt。快照冻结在最后真值（从未接入 = 恒空）
    /// ——classify/带集回退该真值（fallback 底带起步形态）。
    fn wm_disable(&mut self, why: &str) {
        if self.wm.state == WmPrivState::Disabled {
            return;
        }
        match self.wm.state {
            WmPrivState::AwaitHandle => {
                let _ = berry_handle_close(stdin_h());
            }
            WmPrivState::Active => {
                let _ = berry_handle_close(self.wm.evt);
                self.wm.evt = 0;
            }
            WmPrivState::Disabled => {}
        }
        self.wm.state = WmPrivState::Disabled;
        self.wm.dirty = false;
        self.wm.get_inflight = false;
        eprintln!("guifbd: wm face off ({})", why);
    }

    /// 带集重算（双调用点：Srv::new boot 拍 fallback 底带起步 + wm 快照
    /// 接入拍〔wm_drain〕随真值重算；lib `plan_bands` 纯函数；warn 任一
    /// 真 = 超带高/带数帽 loud 一次）。快照重排带槽后旧缓存内容不失效
    /// ——下次 SysFace/WinBand 提交 A 位恒写覆盖自愈（窗口期 WinApp 重放
    /// 可能读到旧窗带像素，v1 接受）。
    fn band_replan(&mut self) {
        let (_, band_bot) = band_geometry(self.w, self.h);
        let (bands, warn, _total) = plan_bands(
            &self.wm.entries,
            self.wm.count,
            self.w as u32,
            self.h as u32,
            band_bot as u32,
        );
        if warn.iter().any(|&w| w) && !self.band_warn_loud {
            self.band_warn_loud = true;
            eprintln!("guifbd: band plan warn (over cap)");
        }
        self.bands = bands;
    }

    /// 带缓存行抄录（SysFace/WinBand 路——blit 行循环内同拍调用）：行区间
    /// [x0,x1) ∩ 各带槽 rect 非空 → bounce 像素 u32 入缓存 + **补不透明键
    /// 0xFF**（guifbd blit 是无透明语义的原样字节拷——凡落 fb 行的像素已
    /// 呈现即不透明；berry-gfx pack 已丢 X 位且现网生产者全族 X 恒 0，不补
    /// 键则缓存全 A==0、重放恒零写。补键后重放像素集与原 fb 像素集严格一
    /// 致（零漂移）；批C 真 ARGB 生产者落地时由该刀翻转补键规则）。布局
    /// = slot×BAND_H_CAP×fb_w + 带内行×fb_w + 列——列取 fb 绝对 x。
    /// 任一槽 ≥1 行首写打 `BANDC ok`（恰一次）。
    fn band_cache_row(&mut self, x0: usize, x1: usize, y: usize) {
        let Some(cache) = self.band_cache.as_deref_mut() else { return };
        let w = self.w;
        let bands = self.bands; // Copy 数组——避开与 cache 借用冲突
        let mut wrote = false;
        let mut wrote_rects =
            [(0u32, 0u32, 0u32, 0u32); BAND_SLOTS]; // (x, y, w, h)——本拍 seed 域
        let mut wrote_n = 0usize;
        for (slot, b) in bands.iter().enumerate() {
            let Some(b) = b else { continue };
            let (bx, by, bw, bh) =
                (b.rect.x as usize, b.rect.y as usize, b.rect.w as usize, b.rect.h as usize);
            // 行命中带行域（带高 ≤ BAND_H_CAP 由 plan_bands 形态帽保证）。
            if y < by || y >= by + bh || y - by >= BAND_H_CAP as usize {
                continue;
            }
            let cx0 = x0.max(bx);
            let cx1 = x1.min(bx + bw);
            if cx1 <= cx0 {
                continue;
            }
            let row = &mut cache[slot * BAND_H_CAP as usize * w + (y - by) * w..][..w];
            for cx in cx0..cx1 {
                let p = &self.bounce[(cx - x0) * 4..(cx - x0) * 4 + 4];
                row[cx] = 0xFF00_0000 | u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                wrote = true;
            }
            if wrote_n < wrote_rects.len() {
                wrote_rects[wrote_n] =
                    (cx0 as u32, y as u32, (cx1 - cx0) as u32, 1);
                wrote_n += 1;
            }
        }
        if wrote && !self.bandc_latched {
            self.bandc_latched = true;
            println!("BANDC ok");
            // seed 自验证拍（BANDR 触发源）：对刚 seed 的行域跑一次真实
            // band_replay（幂等——同像素回写零视觉漂移），每靴确定性验证
            // plan/rows/写全链。应用触发的重放不在此证：launcher 与 spawn
            // 应用共享 wm 队列应答互截仍在（urgent 缺陷另刀根治——本刀只
            // 治 guifbd 接线），app 首帧链随该刀解锁后再证。
            let mut rects = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
            let n = wrote_n.min(gp::DAMAGE_MAX);
            for (i, &(x, ry, rw, rh)) in wrote_rects[..n].iter().enumerate() {
                rects[i] = gp::Rect { x, y: ry, w: rw, h: rh };
            }
            self.band_replay(&rects, false);
        }
    }

    /// 带重放（AppCompat/WinApp 路——CPU blit 毕、vgpu submit_frame 前；
    /// 另 BANDC 首拍 seed 自验证同走此路）：lib `band_replay_plan` 对原始
    /// targets 求交（FULL 恒触发），逐带行 cache→fb（A==0 跳写在
    /// `replay_rows_u32`）。首写数 ≥1 打 `BANDR ok n=`（恰一次——现树
    /// 首触发 = seed 自验证拍；app 提交触发随 launcher×apps 共享队列互截
    /// 根治另刀解锁）。
    fn band_replay(&mut self, targets: &[gp::Rect], full: bool) {
        let Some(cache) = self.band_cache.as_deref_mut() else { return };
        let bands = self.bands; // Copy 数组——避开与 cache 借用冲突
        let mut plan =
            [(0usize, gp::Rect { x: 0, y: 0, w: 0, h: 0 }); BAND_SLOTS * gp::DAMAGE_MAX + 2];
        let n = band_replay_plan(
            targets,
            targets.len(),
            full,
            self.w as u32,
            self.h as u32,
            &bands,
            &mut plan,
        );
        let mut writes = 0usize;
        for &(slot, r) in &plan[..n] {
            let Some(b) = bands[slot].as_ref() else { continue };
            let band_y = b.rect.y as usize;
            let (rx, ry) = (r.x as usize, r.y as usize);
            for y in ry..ry + r.h as usize {
                let off = slot * BAND_H_CAP as usize * self.w + (y - band_y) * self.w + rx;
                let src = &cache[off..off + r.w as usize];
                // SAFETY: dst = fb 行切片（y<self.h、rx+r.w≤w 由 plan 界内
                // 保证；映射幅 = ceil_page(stride×h)——blit 行循环同口径；
                // 逐像素覆写无别名：cache 是独立堆段非 fb 映射）。
                let dst = unsafe {
                    core::slice::from_raw_parts_mut(
                        (FB_VA as usize + y * self.stride + rx * 4) as *mut u32,
                        r.w as usize,
                    )
                };
                writes += replay_rows_u32(dst, src);
            }
        }
        if writes > 0 && !self.bandr_latched {
            self.bandr_latched = true;
            println!("BANDR ok n={}", writes);
        }
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
                surface_id,
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
                    surface_id,
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
            // （带裁剪/vgpu 钩子/带缓存/带重放统一在 blit_and_sample 共尾
            //——分类先于可变借用落局部。）
            let class = classify_surface(surface_id, &self.wm.entries, self.wm.count);
            let (read_ok, all_pass, n_pass) =
                self.blit_and_sample(hvmo, full, &rects, rect_count, class);
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
        // 批B：入途时按本地快照分类（latch 呈现时快照可能已推进——分类
        // 冻结在入途拍 = 与应答同拍语义；错分一拍自愈）。
        let class = classify_surface(surface_id, &self.wm.entries, self.wm.count);
        self.pending = Some(PendingFrame {
            token,
            vmo: hvmo,
            full,
            rects: rcopy,
            rect_count,
            class,
            latch_ns,
            unsquashable: flags & gp::SUBMIT_F_UNSQUASHABLE != 0,
        });
        self.held[self.held_n] = (buffer_id, hvmo);
        self.held_n += 1;
        let mut out = [0u8; 8];
        let n = gp::encode_submit_ack(&mut out, token);
        send_rep(ch, &out[..n]);
    }

    /// damage 裁剪 CPU blit + 带缓存双写 + 带重放 + 7 点抽样对拍（P1 即时
    /// 面与 P2 latch 呈现面共尾；波3 批B 分类口径在共尾内统一执法——lib
    /// `classify_surface` 六档）。返回 (读成败, 抽样全过, 过数)：读失败 =
    /// 致命（即时面拒帧/latch 面弃帧——调用方分面处置）；抽样失配 = 验证
    /// 仪表 loud 不拦。
    fn blit_and_sample(
        &mut self,
        hvmo: Handle,
        full: bool,
        rects: &[gp::Rect; gp::DAMAGE_MAX],
        rect_count: usize,
        class: SurfaceClass,
    ) -> (bool, bool, usize) {
        let fullrect = gp::Rect { x: 0, y: 0, w: self.w as u32, h: self.h as u32 };
        let targets: &[gp::Rect] = if full {
            core::slice::from_ref(&fullrect)
        } else {
            &rects[..rect_count]
        };
        // 波3 批B 分类分档（SPEC §1 语义表）：AppCompat（surface_id=0 或
        // wid 不在快照）保守保护带——Y 裁带逐字保留（几何单源
        // `band_geometry`，撤裁剪归批C；应用全屏提交不再覆盖系统带 → 状态
        // 栏/导航条视觉常驻）；Hidden 空集呈现（blit 面=空集、记账照常）；
        // 余档原样不裁。裁剪只动 damage 面：blit 与抽样（②）同用裁后集，
        // 抽样点恒落裁后 rect 内 → 与 fb 对拍口径不变。
        let (band_top, band_bot) = band_geometry(self.w, self.h);
        let mut clipped_storage = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX + 1];
        let mut clipped_n = 0usize;
        let final_targets: &[gp::Rect] = match class {
            SurfaceClass::AppCompat => {
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
            }
            SurfaceClass::Hidden => &[], // 空集呈现：blit/缓存/重放/上传全空
            _ => targets,
        };
        // 批B 带缓存双写面（SysFace/WinBand 路）：行落 fb 后同拍从 bounce
        // 抄录（保 A 位——fb pack 已丢 alpha 禁从 fb 抓；缓存像素 = 客户端
        // 原始 ARGB u32）。
        let cache_write = matches!(class, SurfaceClass::SysFace | SurfaceClass::WinBand);
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
                if cache_write {
                    self.band_cache_row(x0, x1, y);
                }
            }
        }
        // 批B 带重放（AppCompat/WinApp 路）：CPU blit 毕、vgpu submit_frame
        // 前——重放区 ⊆ 原始 damage 界（band_replay_plan 对**原始 targets**
        // 求交、FULL 帧恒触发保 marker 确定性）→ vgpu 上传集（下方传原始
        // rects）自然覆盖。SERVICE→SYSTEM 定序与 A==0 跳写在 lib 纯函数。
        if matches!(class, SurfaceClass::AppCompat | SurfaceClass::WinApp) {
            self.band_replay(targets, full);
        }
        // B1-b/v2：vgpu 帧提交钩子——blit 落 fb 后、抽样前（设备侧上传与
        // px() 抽样对拍独立；v2 per-rect：SUBMIT 矩形集直通设备上传——
        // v1 收到矩形但 flush_full 丢弃的接线收口。ramfb 相 None 恒短路
        // 零开销）。批B Hidden 档跳过——上传原始 rects 会复活已隐藏窗像素。
        if !matches!(class, SurfaceClass::Hidden) {
            if let Some(v) = self.vgpu.as_mut() {
                v.submit_frame(full, rects, rect_count);
            }
        }
        // ② 抽样对拍（交叉核验强于复算期望——§7-⑤）：SAMPLE_N 点全落
        // damage 并集内（第 i 点取 final_targets[i%K]——A5-b），px() 读回 vs
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
                            // P5 面（hdi 槽呈现）无 surface 域——refresh
                            // 语义逐字保真：不裁带（带裁剪是 present 域
                            // surface 语义）。批B 起显式传 InternalRefresh
                            //（不再冒用 0xFFFF 值域）：槽内容属任意应用
                            // ——不写带缓存（防 P5 污染）、不重放。
                            let (read_ok, all_pass, n_pass) = self.blit_and_sample(
                                self.slots[slot as usize].vmo,
                                true,
                                &[gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
                                0,
                                SurfaceClass::InternalRefresh,
                            );
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
                            // P5 面（hdi 槽呈现）无 surface 域——不裁带
                            //（refresh 语义逐字保真；InternalRefresh 口径
                            // 同即时面：不缓存不重放）。
                            class: SurfaceClass::InternalRefresh,
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
    // 几何 info VMO（boot 72B 九域 u64 LE：fb_pa/w/h/stride/fmt/gui/
    // refresh_hz + 软面板 cfg 两域——第 8/9 域 = 刀2 尾插；refresh 刀2
    // 尾插域经薄壳交接面透传，ours 48B 六域旧注由本注取代）。
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
    let (fb_pa, w, h, stride) = (rd(0), rd(1), rd(2), rd(3));
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
    // —— 批B：带缓存 boot 一次性堆分配（SPEC §3——**绝不能入 Srv/栈**：
    // guifbd 栈仅 32KiB〔svcmgr stack_pages:8〕bounce 已占 16KiB。Box::leak
    // 常驻段先例 launcher main.rs；超 4MiB 门或分配失败 → 带面禁用 loud，
    // 裁剪与现网行为不变〔仅无缓存/重放〕）——
    let band_cache: Option<&'static mut [u32]> = match band_cache_bytes(w as u32) {
        Some(bytes) => {
            let mut v: Vec<u32> = Vec::new();
            match v.try_reserve_exact(bytes / 4) {
                Ok(()) => {
                    v.resize(bytes / 4, 0);
                    Some(Box::leak(v.into_boxed_slice()))
                }
                Err(_) => {
                    eprintln!("guifbd: band cache alloc fail {}B", bytes);
                    None
                }
            }
        }
        None => {
            eprintln!("guifbd: band cache over gate, disabled");
            None
        }
    };
    let mut srv = Srv::new(con, w, h, stride, gui, vgpu, panel_cfg, boot_hz, band_cache);
    // —— 四路 serve：present→hdi→console→wm 确定性轮询；全 EAGAIN 才
    // wait_many n=4 并等。deadline = min(下次软 vsync 点, now+10s,
    // wm 'm' 交接帽) **进调用本身**（A1-a——醒来后补查不够：parked 最长
    // 迟答 10s 且 smoke 抓不住）；命中/到点皆回环顶四路重询（poll-first
    // 无丢唤醒）。wm 槽：AwaitHandle 态 = stdin（'m' 到达可读）、Active
    // 态 = 订阅者事件通道（通知+应答同队列恒 READABLE——BUSY 留拍重试
    // 自愈，不做 WRITABLE 臂）、Disabled 态无槽（n=3 回落）。
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
        if srv.poll_wm() {
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
        // wm 'm' 交接帽臂（AwaitHandle 态——10s 帽到点自醒落 Disabled；
        // 他态无此臂）。
        if srv.wm.state == WmPrivState::AwaitHandle && srv.wm.await_deadline_ns < deadline {
            deadline = srv.wm.await_deadline_ns;
        }
        // wm 等待槽（Disabled = 无活柄——关柄入 wait 集 = 整体 -EBADF
        // trap，禁入；n 随态回落，Disabled 相恰 = 批B 前三路形态）。
        let hwm = match srv.wm.state {
            WmPrivState::AwaitHandle => stdin_h(),
            WmPrivState::Active => srv.wm.evt,
            WmPrivState::Disabled => 0,
        };
        let hs = [hpresent, hhdi, hpipe, hwm];
        let sigs = [SIGNAL_READABLE, SIGNAL_READABLE, SIGNAL_READABLE, SIGNAL_READABLE];
        let wn = if hwm != 0 { 4 } else { 3 };
        let mut idx = 0usize;
        let mut obs = 0u64;
        let s = berry_object_wait_many(
            hs.as_ptr(),
            sigs.as_ptr(),
            wn,
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
