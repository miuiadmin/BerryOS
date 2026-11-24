//! gfxdemo —— GUI-5 消费侧（§3 #12）：wm 域首个真实消费者 + present/hdi
//! 改经 wmd 中继（同 op 换端，ADR #61 呈现/策略分离）。
//!
//! **B 形态三件事的接线半边**（ADR #54「协议先定形、无消费者不接线」）：
//! berry-gfx pass 形态 API soft 渲一帧（确定性 DisplayList：Clear 底色 +
//! fill rect + glyph 串；Bgrx 与 ramfb 同源免转换）→ 匿名 VMO commit+map
//! → present 域 SUBMIT（K=2 damage——部分包络模式：下半屏包络 r≈500‰
//! 见 [`PARTIAL_ENVELOPE`]；随行 VMO READ 别名）→ guifbd
//! blit-at-SUBMIT 上屏 → FRAME_CALLBACK 回执 → RELEASE 归还即关 →
//! VOTE/POWER_MODE 1B 回执 → hdi 域软 vsync mini 序（REG_VBLANK /
//! VBLANK_WAIT / PREPARE / COMMIT），全断言过才打 `GFX ok`（smoke-image
//! EFI 相 ECO 标记）。服务端真值 `GFXF ok n=` 由 guifbd 打——两标记分立
//! = 故障定位面（GFXF 缺 = 服务端 blit/抽样/记账炸；GFXF 在而 GFX 缺 =
//! 客户端 RPC 链炸）。刀3 起序中插仲裁活证三段（INPUT_PULSE boost /
//! Maintenance 降档驻留 / Background 停帧恢复——`gfx: arb1/arb2` 客户端
//! 断言 + `PANELARB` 服务端串口证据行）。
//!
//! **端点拓扑**（GUI-5 §2/§7 Q12）：svcmgr 在 wmd 预拉段 channel_pair()
//! 接线，wm 客户端端随本程序 env extras 直递（5/8：console/stdin/
//! self_proc/self_vmar/wm_cli——libc `wm_cli_h()` 读 GIFT_H[0]）。三域
//! **同端混流**：wm 0x60..=0x65 wmd 本地权威处理；present 1..=7（0x07
//! INPUT_PULSE 刀3 入域）与 hdi 0x40..=0x45 经 wmd 分发表透传 guifbd（中
//! 继不改 op 编码——本程序断言
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
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::io::{self_vmar_h, wm_cli_h};
use berry_ui::step::FixedStep;
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_event_create,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_one, berry_vmar_map,
    berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    CLOCK_MONOTONIC, RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE, SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use std::process::exit;

// 链接种子（guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc 的 crt0
// （含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（fsclient `TIMEOUT_NS` 同值 10s——A6-a 形态：死法
/// = FAIL loud 非挂死，smoke 侧标记缺失兜底）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 部分包络模式（真 damage 帧源——18 号复核点名：现役链无 r<1000‰ 触发，
/// 16 号实测首个 per-rect 帧包络 = 全帧 r=1000‰，v2 r 缩放收益无活体证据；
/// 现役 damage 面定谳：damage 语义在 SUBMIT 侧（声明），绘制侧 berry-gfx
/// pass 本有 dirty 面〔`PassDescriptor::dirty`，queue 屏障裁剪执法〕而本程
/// 序此前未用——双整帧是消费者用法非协议限制）。true = 绘制面只 dirty 下
/// 半屏（pass `dirty` 约束：Clear 与全部命令裁剪到区内，dirty 区外结构性
/// 不可触碰）∧ SUBMIT rect 集同域（K=2 下半屏二分）——声明域 == 绘制域，
/// boot 后首个 per-rect 帧令 guifbd `VGPU2 rect` 观测行（per-rect 首用一次
/// 性）显 r≈500‰——β-1 增量粒度参考。false = 16 号基线形态（K=2 上下半
/// 帧分区，并集 = 整帧 r=1000‰）——A/B 回放翻此重建。
///
/// 为什么是编译分支（按现役参数面形态选）：env 变量 = berry-libc 空表桩
/// （environ NULL / getenv 恒空——子进程 env 传递 defer v2）；argv 面在席
/// 但 boot 链预拉恒空参（svcmgr spawn `args: &[]`）——运行期参数在本程序
/// 不可达（改 spawn 面出本轮范围）。缺省 true：VGPU2 rect 行是 guifbd 一次
/// 性立法，首 per-rect 帧即定读数——标准 boot 即产真部分包络帧源。
const PARTIAL_ENVELOPE: bool = true;

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

/// PANEL_CAPS 单趟——current_hz 现值（刀3 仲裁活证面复用：boost/降档后
/// 断言软面板已切）。
/// PANEL_CAPS 单趟——完整应答（刀6 k6t/k6l/k6f 资格门读 rate_model/
/// vrr/flags 面）。
fn panel_caps(hw: Handle) -> gp::PanelCapsRep {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_panel_caps_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_panel_caps_rep(&rep[..n]) {
        Some(c) => c,
        None => fail(&format!("panel caps bad st={}", rep[0])),
    }
}

/// 现行档便捷读（k4 既有口径）。
fn caps_cur_hz(hw: Handle) -> u16 {
    panel_caps(hw).current_hz
}

/// VBLANK_WAIT 单趟——等一个拍点（返回 seq；刀3 切档生效窗 = request 后
/// 拍点过 2 个：softpanel pending 2 帧递减、`left<=1` 提交）。
fn vb_point(hw: Handle) -> u64 {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_vblank_wait_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_vblank_rep(&rep[..n]) {
        Some(v) => v.seq as u64,
        None => fail(&format!("vblank rep bad st={}", rep[0])),
    }
}

/// OP_VBLANK_TIMING 单趟——拍点三元组（seq/ts_ns/refresh_ns；刀2 事件面
/// 24B 形）。k4t 拍点网格采样用（ts_ns = 标称点时刻——网格相位统计锚）。
fn vb_timing_point(hw: Handle) -> (u64, u64, u32) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_vblank_timing_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_vblank_timing_rep(&rep[..n]) {
        Some(v) => (v.seq, v.ts_ns, v.refresh_ns),
        None => fail(&format!("vblank timing rep bad st={}", rep[0])),
    }
}

/// 单调窗睡眠：永不置位的 event 上 wait 到点 = 纯超时睡眠（A5 驻留窗
/// 计时——输入通道不用，严格串行 RPC 面不沾通道信号语义）。
fn sleep_ns(ns: u64) {
    let mut ev: Handle = 0;
    if berry_event_create(0, &mut ev) != 0 || ev == 0 {
        fail("event create sleep");
    }
    let mut obs = 0u64;
    let _ = berry_object_wait_one(ev, SIGNAL_READABLE, now() + ns, &mut obs);
    let _ = berry_handle_close(ev);
}

/// FULL submit 单趟（刀6 k6 段共用——fresh dup 别名随行）：返回 frame
/// token（ST_BUSY/ST_FULL 等 1B 形 = fail loud）。
///
/// 随行规则：随消息发送的 VMO 别名必须以 RIGHT_READ 加 RIGHT_TRANSFER
/// 复制——缺 TRANSFER 会被内核 send 随行预检（逐柄 rights_of →
/// `hdlface::probe::probe_transfer_ok`）以 ERR_ACCESS_DENIED（-13）拒绝，
/// 本趟落 `rpc send err=-13`；下方 dup 请求即按此补 TRANSFER（77354854）。
fn submit_full(hw: Handle, vmo: Handle, target_ns: u64) -> u32 {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let mut alias: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0 || alias == 0 {
        fail("dup alias k6");
    }
    let sreq = gp::PresentReq::Submit {
        surface_id: 0,
        buffer_id: 1,
        flags: gp::SUBMIT_F_DAMAGE_FULL,
        rate_hint: 0,
        target_present_ns: target_ns,
        rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
        rect_count: 0,
    };
    let m = enc_req(gp::encode_present_req(&mut req, &sreq));
    let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
    match gp::parse_submit_ack(&rep[..n]) {
        Some(t) => t,
        None => fail(&format!("k6 submit bad st={}", rep[0])),
    }
}

/// FRAME_CALLBACK 单趟（P2 起阻塞语义：在途帧 park 至 latch 呈现/判弃
/// 才应答——park 即 pacing RPC；300s 收帽内 latch 必到）。
fn frame_callback(hw: Handle, token: u32) -> gp::FrameDoneRep {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::FrameCallback { frame_token: token },
    ));
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_frame_done_rep(&rep[..n]) {
        Some(d) => d,
        None => fail(&format!("k6 callback bad st={}", rep[0])),
    }
}

/// FRAME_TIMING 单趟（非阻塞真相面）：在途帧未入环 → None（ST_NOENT
/// ——P2 挂起态的结构性判别值）；环命中 → rec；其他错码 fail loud。
fn timing_probe(hw: Handle, token: u32) -> Option<gp::FrameTimingRep> {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::FrameTiming { frame_token: token },
    ));
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_frame_timing_rep(&rep[..n]) {
        Some(r) => Some(r),
        None if rep[0] == gp::ST_NOENT => None,
        None => fail(&format!("k6 timing bad st={}", rep[0])),
    }
}

/// RELEASE_WAIT 单趟 + 归还别名即关（k6 段共尾；断 count == expect）。
fn release_n(hw: Handle, expect: usize) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
    let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    if gp::parse_release_rep(&rep[..n]).map(|r| r.count) != Some(nh)
        || nh != expect
    {
        fail(&format!("k6 release nh={} expect={}", nh, expect));
    }
    for h in &rhs[..nh] {
        close_h(*h);
    }
}

/// VOTE Interactive 单趟（k6 段档位编排共用）。
fn vote_interactive(hw: Handle, min_hz: u16, max_hz: u16, preferred_hz: u16) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Interactive as u8,
            min_hz,
            max_hz,
            preferred_hz,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "k6 vote");
}

/// 相位残差 nearest-rank p99（k4t 插入排序同款——k6h 高刷网格统计复用；
/// 2 ≤ n ≤ 100——101 park = 100 delta 定容）。
fn resid_p99(dts: &[u64], period: u64) -> (u64, u64) {
    let n = dts.len();
    assert!(n >= 2 && n <= 100, "resid_p99 n 域 2..=100");
    let mut aerr = [0u64; 100];
    let mut miss = 0u64;
    for i in 0..n {
        let k = ((dts[i] + period / 2) / period).max(1);
        miss += k - 1;
        let resid = (dts[i] as i64) - (k as i64) * (period as i64);
        aerr[i] = resid.unsigned_abs();
    }
    for i in 1..n {
        let mut j = i;
        while j > 0 && aerr[j - 1] > aerr[j] {
            aerr.swap(j - 1, j);
            j -= 1;
        }
    }
    let rank = (99 * n + 99) / 100;
    (aerr[rank - 1], miss)
}

fn main() {
    // —— 降级门：wm 客户端端未交付（rootfs 相 wmd 随 fb 探测门不被拉起
    // → 本程序不被拉起；bsh 手跑无 extras）→ 一行诊断 + exit(0)（netd
    // 双门先例同款——非错误面，`gfx: wm absent` 永不进 MARKERS）。present/
    // hdi 客户端端已随 wmd 换手中继——本程序不再直持，单门即全链门（GUI-5
    // §7 Q12：extras 恰 wm_cli 一枚）。
    let hw = wm_cli_h();
    if hw == 0 {
        println!("gfx: wm absent");
        exit(0);
    }

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

    // ② CREATE：全屏 APP 层窗 + AVOID_SYS 旗标（v1 无系统层窗 = 避让空
    // 转，旗标面先接线）；id≠0 由 wmd 分配策略保证——建窗成功断言面。
    let creq = gp::WmCreateReq {
        layer: gp::WM_LAYER_APP,
        flags: gp::WM_CREATE_F_AVOID_SYS,
        parent: 0,
        group: 0,
        x: 0,
        y: 0,
        w: wminfo.disp_w,
        h: wminfo.disp_h,
    };
    let m = enc_req(gp::encode_wm_create_req(&mut req, &creq));
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let wid = match gp::parse_wm_create_ack(&rep[..n]) {
        Some(id) if id != 0 => id,
        _ => fail(&format!("wm create ack bad st={}", rep[0])),
    };

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
    if snap.count < 1 {
        fail(&format!("wm table count={}", snap.count));
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

    // —— 帧缓冲 VMO 三连（mmp_selftest/pthread 栈同链）：create → 整段
    // commit（eager VMO 面；demand G4-1 起另路）→ vmar_map(self_vmar_h())。GETINFO 无
    // stride 域：stride = w*4 tight packing（ramfb GOP 实测同值——服务端
    // blit 的源布局）；len 页取整（vmar_map 页倍执法；尾页零无害，need =
    // stride*(h-1)+w*4 ≤ len 由 submit 期第四拒面复检）。
    let (w, h) = (info.display_w as usize, info.display_h as usize);
    let stride = w.checked_mul(4).unwrap_or_else(|| fail("stride overflow"));
    let frame = stride.checked_mul(h).unwrap_or_else(|| fail("frame overflow"));
    let len = frame
        .checked_add(0xFFF)
        .map(|v| v & !0xFFF)
        .unwrap_or_else(|| fail("page round overflow"));
    let mut vmo: Handle = 0;
    if berry_vmo_create(len as u64, 0, &mut vmo) != 0 || vmo == 0 {
        fail("vmo create");
    }
    if berry_vmo_commit(vmo, 0, len as u64) != 0 {
        fail("vmo commit");
    }
    let hv = self_vmar_h();
    if hv == 0 {
        fail("no self vmar gift");
    }
    let mut va = FRAME_VA;
    if berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0
        || va != FRAME_VA
    {
        fail("vmar map");
    }

    // —— berry-gfx soft 渲一帧（确定性图案：Clear 深蓝灰底 + 顶部红带 +
    // 底部绿条 + 白字 "GFX" 深蓝衬底）。部分包络模式：pass dirty = 下半
    // 屏——Clear 与全部命令裁剪到区内（queue 屏障语义：dirty 区外结构性
    // 不可触碰），顶部红带/顶部字形整条裁空、底部绿条落区内；基线形态：
    // dirty 无 = 整帧。绘制域与下方 SUBMIT 声明域同域（真 damage：不产
    // 声明外笔迹）。
    let half = (h / 2) as u32;
    let pass = if PARTIAL_ENVELOPE {
        PassDescriptor::new(
            LoadOp::Clear { r: 0x10, g: 0x18, b: 0x28 },
            StoreOp::Store,
            Some(Rect::new(0, half as i32, w as i32, (h - h / 2) as i32)),
        )
    } else {
        PassDescriptor::clear(0x10, 0x18, 0x28)
    };
    let desc = match TextureDesc::new(w, h, stride, TextureFormat::Rgbx) {
        Ok(d) => d,
        Err(e) => fail(&format!("texture desc {e:?}")),
    };
    // SAFETY: [FRAME_VA, FRAME_VA+len) 刚由 vmar_map 落位且整段已 commit。
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };
    let atlas = FontAtlas8x16;
    let glyphs = [
        GlyphPos { ch: b'G', x: 16, y: 16 },
        GlyphPos { ch: b'F', x: 24, y: 16 },
        GlyphPos { ch: b'X', x: 32, y: 16 },
    ];
    let mut enc = CommandEncoder::new();
    record(
        enc.fill_rect(
            Rect::new(0, 0, w as i32, (h / 3) as i32),
            Color { r: 0x7F, g: 0x00, b: 0x00 },
        ),
        "fill band",
    );
    record(
        enc.fill_rect(
            Rect::new(0, (h - h / 16) as i32, w as i32, (h / 16) as i32),
            Color { r: 0x00, g: 0x60, b: 0x00 },
        ),
        "fill strip",
    );
    record(
        enc.draw_glyph_run(GlyphRun {
            atlas: &atlas,
            glyphs: &glyphs,
            fg: Color { r: 0xFF, g: 0xFF, b: 0xFF },
            bg: Some(Color { r: 0x10, g: 0x18, b: 0x28 }),
        }),
        "glyph run",
    );
    let dl = enc.finish();
    if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
        fail(&format!("render submit {e:?}"));
    }

    // —— SUBMIT（K=2 damage rect：部分包络模式 = 下半屏二分——K 面与基
    // 线同形（批形状/reap 可比），包络 = 下半屏 r≈500‰（1080×1920 档
    // env=1080x960+0+960）；基线形态 = 上下半帧分区，并集 = 整帧
    // r=1000‰。服务端 px() 抽样 7 点恒落并集内，§9 A5-b；flags=0 非
    // FULL =「K>0 或 FULL 二取一」立法的 K 侧）。随行 = 本 VMO 的 READ
    // dup 别名：send 即 take
    // （A6-b）——原 vmo handle 留表，PREPARE 时再 dup 新别名。
    //（A6-b）；TRANSFER 保位——别名随 SUBMIT channel_send 迁出（send 随行
    // 预检执法；dup 只紧不松须显式保位）。
    let mut alias: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0 || alias == 0 {
        fail("dup alias");
    }
    let mut rects = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
    if PARTIAL_ENVELOPE {
        let lower = h as u32 - half;
        let q = lower / 2;
        rects[0] = gp::Rect { x: 0, y: half, w: w as u32, h: q };
        rects[1] = gp::Rect { x: 0, y: half + q, w: w as u32, h: lower - q };
    } else {
        rects[0] = gp::Rect { x: 0, y: 0, w: w as u32, h: half };
        rects[1] = gp::Rect { x: 0, y: half, w: w as u32, h: h as u32 - half };
    }
    let sreq = gp::PresentReq::Submit {
        surface_id: 0,
        buffer_id: 1,
        flags: 0,
        rate_hint: 0,
        target_present_ns: 0,
        rects,
        rect_count: 2,
    };
    let m = enc_req(gp::encode_present_req(&mut req, &sreq));
    let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
    let token = match gp::parse_submit_ack(&rep[..n]) {
        Some(t) => t,
        None => fail(&format!("submit ack bad st={}", rep[0])),
    };

    // —— FRAME_CALLBACK：blit-at-SUBMIT 形下帧已呈现（token 校验即答）。
    // 断 PRESENTED；credits ≥0 对 u8 恒真、≤ CREDITS_MAX 由 parse 面执法。
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::FrameCallback { frame_token: token },
    ));
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let done = match gp::parse_frame_done_rep(&rep[..n]) {
        Some(d) => d,
        None => fail(&format!("frame done bad st={}", rep[0])),
    };
    if done.status != gp::FRAME_PRESENTED {
        fail(&format!("frame status={}", done.status));
    }

    // —— RELEASE：归还别名——应答随行 count 个 VMO handle 重挂本表，即关。
    // 断 count==1 恰归还本次 SUBMIT 的在途别名（credits 记账清零）；ids
    // 域 v1 单客户端 echo 面，不断言精确值免过度约束服务端。
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
    let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let rel = match gp::parse_release_rep(&rep[..n]) {
        Some(r) => r,
        None => fail(&format!("release rep bad st={}", rep[0])),
    };
    if rel.count != 1 || nh != 1 {
        fail(&format!("release count={} nh={}", rel.count, nh));
    }
    close_h(rhs[0]);

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

    // —— 刀2 消费面：PANEL_CAPS 断言（n 帽内 / min≤cur≤max / 当前周期 =
    // softpanel 周期单源换算——Tiered 查表值与 FreeRange 单点换算同路）
    // + VBLANK_TIMING 三元组（事件面：refresh 与 caps 当前档一致）。
    // `gfx:` 前缀小写诊断行——DET 相同 seed 双 boot 逐字节同（T2 guest 证面）。
    let m = gp::encode_panel_caps_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let caps = match gp::parse_panel_caps_rep(&rep[..n]) {
        Some(c) => c,
        None => fail(&format!("panel caps bad st={}", rep[0])),
    };
    if caps.version < 1
        || caps.n_tiers as usize > gp::PANEL_TIERS_MAX
        || !(caps.min_hz <= caps.current_hz && caps.current_hz <= caps.max_hz)
    {
        fail(&format!(
            "caps bad ver={} n={} {}..={} cur={}",
            caps.version, caps.n_tiers, caps.min_hz, caps.max_hz, caps.current_hz
        ));
    }
    if caps.current_hz == 0
        || caps.current_period_ns as u64 != gp::softpanel::period_of(caps.current_hz)
    {
        fail(&format!(
            "caps period hz={} ns={} expect={}",
            caps.current_hz,
            caps.current_period_ns,
            gp::softpanel::period_of(caps.current_hz)
        ));
    }
    println!(
        "gfx: caps model={} n={} cur={}",
        caps.rate_model, caps.n_tiers, caps.current_hz
    );

    let m = gp::encode_vblank_timing_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let vbt = match gp::parse_vblank_timing_rep(&rep[..n]) {
        Some(t) => t,
        None => fail(&format!("vblank timing bad st={}", rep[0])),
    };
    if vbt.seq < 1 || vbt.ts_ns == 0 {
        fail(&format!(
            "vbt seq={} ts={} refresh={}",
            vbt.seq, vbt.ts_ns, vbt.refresh_ns
        ));
    }
    // 暖机竞速容忍（run11 教训）：caps→vbt 两 RPC 间隔在 TCG 重载下拉到
    // 秒级，boot 期空闲驻留降档（PANELARB sw src=dwell）可恰落缝里——
    // refresh 诚实反映 30Hz 而 caps 是降档前旧读 60。此窗口无票、降档
    // 单调向下，重读一次对账，两读任一吻合即过（vbt 取值必居其一）。
    if vbt.refresh_ns != caps.current_period_ns
        && vbt.refresh_ns != panel_caps(hw).current_period_ns
    {
        fail(&format!(
            "vbt refresh={} (caps ns={} recheck mismatch)",
            vbt.refresh_ns, caps.current_period_ns
        ));
    }
    println!(
        "gfx: vb3 seq={} ts={} refresh={}",
        vbt.seq, vbt.ts_ns, vbt.refresh_ns
    );

    // —— 刀3 仲裁活证面三段：A6 boost → A5 降档驻留 → A7 停帧/恢复。
    // 前置清场（PowerMode/VOTE 已从「只记账」转仲裁输入——刀3 起是活语义）：
    // POWER_NORMAL 清前序 DIM 档 + VOTE Unspecified 换中性票（前序
    // Interactive max=60 票会把 boost 目标帽进 G=[30,60]——A2 全局区间
    // 票面 max 参与）。服务端证据行 = `PANELARB sw/stop/resume`（串口面，
    // guifbd 仲裁执行处打出）。
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::PowerMode { mode: gp::POWER_NORMAL },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "power normal");
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Unspecified as u8,
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 0,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote neutral");

    // (a) A6 触摸加速：脉冲 → 服务端即时 `sw hz=120 src=boost`；拍点过
    // 2 个 = 生效窗提交 → caps 断言 120。
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::InputPulse));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "input pulse");
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let hz1 = caps_cur_hz(hw);
    if hz1 != 120 {
        fail(&format!("arb1 hz={hz1} expect=120"));
    }
    println!("gfx: arb1 hz={hz1}");
    // boost 窗尾让净（500ms 帽 + 100ms 裕）——否则 (b) 的降档驻留锚点被
    // boost 目标期顶住（A5 同向换目标即重锚），时序测算失据。
    sleep_ns(600_000_000);

    // (b) A5 降档驻留：Maintenance 票 pref=30 → 服务端 1s 驻留（客户端睡
    // 1.5s 盖帽 TCG 抖动）→ 拍点过 3 个（驻留到期拍 + 2 帧生效窗）→ caps
    // 断言 30。
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Maintenance as u8,
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 30,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote maintenance");
    sleep_ns(1_500_000_000);
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let hz2 = caps_cur_hz(hw);
    if hz2 != 30 {
        fail(&format!("arb2 hz={hz2} expect=30"));
    }
    println!("gfx: arb2 hz={hz2}");

    // (c) A7 静止停帧/恢复：Background 资格票 → REG_VBLANK(0) 撤点
    //（armed/parked 双空 + 票面 BG = 停帧资格 → 服务端 `PANELARB stop`
    // 行；关闭形宽收 0/1 attach）→ 重臂（ev3 新事件——前序 ev 已迁服务
    // 端，本表值 stale）→ on_activity 恢复（`PANELARB resume` 行）。
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Background as u8,
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 0,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote background");
    let m = gp::encode_reg_vblank_req(&mut req, 0);
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "reg vblank arb off");
    let mut ev3: Handle = 0;
    if berry_event_create(0, &mut ev3) != 0 || ev3 == 0 {
        fail("event create 3");
    }
    let m = gp::encode_reg_vblank_req(&mut req, 1);
    expect_ok(hw, &req[..m], &[ev3], &mut rep, &mut rhs, "reg vblank arb on");

    // (d) A5 升档连击：Interactive 票 max=120 pref=120 → 上行 2 拍 streak
    //（升档无驻留——TCG 时序鲁棒）→ 生效窗 2 拍 → caps 断言 120（验收面
    // 「VOTE 120 票 → current_hz 120」）。收尾回中性票（Unspecified 停帧
    // 资格在列——尾序 REG_VBLANK(0) 撤点仍可停帧）。
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Interactive as u8,
            min_hz: 0,
            max_hz: 120,
            preferred_hz: 120,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote up");
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let hz3 = caps_cur_hz(hw);
    if hz3 != 120 {
        fail(&format!("arb3 hz={hz3} expect=120"));
    }
    println!("gfx: arb3 hz={hz3}");
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Unspecified as u8,
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 0,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote neutral 2");

    // —— 刀4 两段：A10 预告升档「不迟一帧」+ T3/T6 拍点误差与 seq 贯穿。
    // 前态：panel 120（(d) 升档到位）、票已回中性。
    // (e) A10 动机例（30Hz 降探后首帧压界）：Maintenance 票驻留降 30 →
    // 换 Interactive 120 票（Event 不积 streak——正常路须再等 2 拍）→
    // 单发 SUBMIT 携 rate_hint=120（密集预告）→ 服务端 hint 即 commit
    //（`PANELARB sw hz=120 src=vote`）→ **恰 2 拍**（生效窗）后 caps==120。
    // 判别性：若无预告快路，commit 落第 2 拍 streak → 生效须第 4 拍——
    // 2 拍即 120 = 快路独有（对照路 host 单测 arb_a10_hint_dense_upshift
    // 钉板）。
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Maintenance as u8,
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 30,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote maintenance 2");
    sleep_ns(1_500_000_000);
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let hz4 = caps_cur_hz(hw);
    if hz4 != 30 {
        fail(&format!("k4 pre hz={hz4} expect=30"));
    }
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Interactive as u8,
            min_hz: 0,
            max_hz: 120,
            preferred_hz: 120,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote up hint");
    // 预告帧：FULL damage + rate_hint=120（密集 = 快于现档 30）。vote 与
    // submit 之间零拍点（30Hz 拍距 33ms ≫ RPC 对时——streak 面零污染）。
    let mut alias_h: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias_h) != 0 || alias_h == 0 {
        fail("dup alias hint");
    }
    let hreq = gp::PresentReq::Submit {
        surface_id: 0,
        buffer_id: 1,
        flags: gp::SUBMIT_F_DAMAGE_FULL,
        rate_hint: 120,
        target_present_ns: 0,
        rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
        rect_count: 0,
    };
    let m = enc_req(gp::encode_present_req(&mut req, &hreq));
    let (n, _) = rpc(hw, &req[..m], &[alias_h], &mut rep, &mut rhs);
    if gp::parse_submit_ack(&rep[..n]).is_none() {
        fail(&format!("hint submit bad st={}", rep[0]));
    }
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
    let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    if gp::parse_release_rep(&rep[..n]).map(|r| r.count) != Some(nh) || nh < 1 {
        fail("hint release bad");
    }
    for h in &rhs[..nh] {
        close_h(*h);
    }
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let hz5 = caps_cur_hz(hw);
    if hz5 != 120 {
        fail(&format!("k4a hz={hz5} expect=120（2 拍生效 = 预告快路；streak 路此拍仍 30）"));
    }
    println!("gfx: k4a hz={hz5}");

    // (f) T3 拍点相位门 + T6 回执链门 @60Hz：Utility 票驻留降 60 后两段。
    // 设计实证（run3/4 取证改设计——原 101 帧 submit 驱动 p99 量的是 wmd
    // 中继 RPC 链节拍非拍点网格：P1 blit-at-SUBMIT 下 presented_ns = 提交
    // 瞬间，每帧 5 RPC × TCG+宿主争用 ≈ 0.2-1.4s/帧）：
    // - T3 改量拍点网格本身——连 park VBLANK_TIMING 取三元组，相位残差
    //   p99 = |Δts − round(Δts/period)·period|（miss 点不污染统计、只计数
    //   另报：粗钟漏拍证据行归刀5 hrtimer 对拍面，不入门）；FREE 追赶态
    //   名义网格在案（tick_vsync：迟到醒跳过已过点不耗相位）；
    // - T6 改括号对拍——帧回执 seq ∈ (本帧前拍, 下帧前拍]：cadence 无关
    //   的结构贯穿断言（一号到底），容 1 离群。
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Utility as u8,
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 60,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote utility 60");
    sleep_ns(1_500_000_000);
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let _ = vb_point(hw);
    let hz6 = caps_cur_hz(hw);
    if hz6 != 60 {
        fail(&format!("k4t pre hz={hz6} expect=60"));
    }
    // —— T3 网格段：101 连 park，相位残差 p99 ≤ 2ms（TCG 档——§10-T3）。
    const K4G: usize = 101;
    let mut gts = [0u64; K4G];
    let mut gseq = [0u64; K4G];
    let mut gperiod = 0u64;
    for i in 0..K4G {
        let (sq, ts, rf) = vb_timing_point(hw);
        if rf as u64 != 16_666_667 {
            fail(&format!("k4t grid refresh={rf} expect 60Hz"));
        }
        gperiod = rf as u64;
        gseq[i] = sq;
        gts[i] = ts;
    }
    if gseq[K4G - 1] <= gseq[0] {
        fail("k4t grid seq not advancing");
    }
    let mut aerr = [0u64; K4G - 1];
    let mut miss: u64 = 0;
    for i in 0..aerr.len() {
        let d = gts[i + 1] - gts[i];
        let k = ((d + gperiod / 2) / gperiod).max(1);
        miss += k - 1;
        let resid = (d as i64) - (k as i64) * (gperiod as i64);
        aerr[i] = resid.unsigned_abs();
    }
    for i in 1..aerr.len() {
        let mut j = i;
        while j > 0 && aerr[j - 1] > aerr[j] {
            aerr.swap(j - 1, j);
            j -= 1;
        }
    }
    let rank = (99 * aerr.len() + 99) / 100; // nearest-rank p99（n=100 → 99 位）
    let p99 = aerr[rank - 1];
    if p99 > 2_000_000 {
        fail(&format!("k4t p99={}ns > 2ms（TCG 档）", p99));
    }
    println!("gfx: k4t p99={}ns miss={} n={}", p99, miss, K4G - 1);
    // —— T6 回执链段：17 帧 submit→回执，括号 seq 贯穿 + flags/refresh 恒真。
    const K4M: usize = 17;
    let mut rec_seq = [0u64; K4M];
    let mut parks = [0u64; K4M + 1];
    let mut flags_bad = 0usize;
    for i in 0..K4M {
        parks[i] = vb_point(hw);
        let mut alias_i: Handle = 0;
        if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias_i) != 0 || alias_i == 0 {
            fail("dup alias loop");
        }
        let sreq = gp::PresentReq::Submit {
            surface_id: 0,
            buffer_id: 1,
            flags: gp::SUBMIT_F_DAMAGE_FULL,
            rate_hint: 0,
            target_present_ns: 0,
            rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
            rect_count: 0,
        };
        let m = enc_req(gp::encode_present_req(&mut req, &sreq));
        let (n, _) = rpc(hw, &req[..m], &[alias_i], &mut rep, &mut rhs);
        let token = match gp::parse_submit_ack(&rep[..n]) {
            Some(t) => t,
            None => fail(&format!("loop submit bad st={}", rep[0])),
        };
        // P3 回执查询（非阻塞真相面；在途 ≤1 ∧ 环 8 深 → 命中结构性保证）。
        let m = enc_req(gp::encode_present_req(
            &mut req,
            &gp::PresentReq::FrameTiming { frame_token: token },
        ));
        let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
        let rec = match gp::parse_frame_timing_rep(&rep[..n]) {
            Some(r) => r,
            None => fail(&format!("frame timing bad st={}", rep[0])),
        };
        if rec.frame_token != token {
            fail(&format!("timing token={} want={}", rec.frame_token, token));
        }
        if rec.flags != gp::FRAME_TIMING_F_PRESENTED | gp::FRAME_TIMING_F_FULL {
            flags_bad += 1;
        }
        if rec.refresh_ns != 16_666_667 {
            fail(&format!("k4s refresh={} expect 60Hz", rec.refresh_ns));
        }
        rec_seq[i] = rec.seq;
        let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
        let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
        if gp::parse_release_rep(&rep[..n]).map(|r| r.count) != Some(nh) || nh < 1 {
            fail("loop release bad");
        }
        for h in &rhs[..nh] {
            close_h(*h);
        }
    }
    parks[K4M] = vb_point(hw);
    let mut t6_bad = 0usize;
    for i in 0..K4M {
        if rec_seq[i] <= parks[i] || rec_seq[i] > parks[i + 1] {
            t6_bad += 1;
        }
    }
    if flags_bad != 0 {
        fail(&format!("k4s flags_bad={}", flags_bad));
    }
    if t6_bad > 1 {
        fail(&format!("t6 bad={} > 1（seq 括号贯穿破坏面超离群容忍）", t6_bad));
    }
    println!("gfx: k4s n={} bad={}", K4M, t6_bad);

    // —— 刀6 段（§9：P2 拍点门闩 / 240/480 产品档 / 165/180 测试档 +
    // T5 不变性 / A8 LFC / P6 逐帧锁相）。k6p/k6h 任何 model/mode 形都跑；
    // k6t/k6l/k6f 按面板宣告或行为自门（缺件跳段零行 = 非 boot 形态面，
    // 与 `gfx: wm absent` 降级同语义——smoke 侧按 boot 形各自断言）。
    let caps6 = panel_caps(hw);
    let p60 = 16_666_667u64;

    // (k6p) P2 四探：挂起（环外 NOENT）/迟到即呈/squash 判弃/P5 定点提交。
    // 挂起窗 = K6_DEFER_NS（3s）——须容「窗内 RPC 跳数 × 单趟 RTT」（TCG
    // 载荷实测单趟 0.2-1.4s——首版 8p=133ms 与二版 1.5s 均被 RTT 反超：
    // 探针晚到即撞已呈帧，假红；③ B 探针重排后在 hop 3，3s 容 r≤1.0s）；
    // latch = ≤ target 的最大拍点（网格域）→ presented ∈
    // [target−p−5ms, target+250ms]——下界结构性（fire 门 now ≥ latch ≥
    // target−p），上界 = K1 唤醒尾帽（BENCHWAKE p99≈16.7ms——TCG 档
    // 只证不丢不早，点位紧判归 DET 位级面 k6t）。
    const K6_DEFER_NS: u64 = 3_000_000_000;
    let _ = vb_point(hw); // 网格在案（park 即 vsync_schedule 锚）
    let target_a = now() + K6_DEFER_NS;
    let tok_a1 = submit_full(hw, vmo, target_a);
    if timing_probe(hw, tok_a1).is_some() {
        fail("k6p deferred probe: ring hit（未挂起）");
    }
    let done_a1 = frame_callback(hw, tok_a1);
    if done_a1.status != gp::FRAME_PRESENTED
        || done_a1.presented_ns + (p60 + 5_000_000) < target_a
        || done_a1.presented_ns > target_a + 250_000_000
    {
        fail(&format!(
            "k6p latch status={} presented={} target={}",
            done_a1.status, done_a1.presented_ns, target_a
        ));
    }
    match timing_probe(hw, tok_a1) {
        Some(r) if r.flags == gp::FRAME_TIMING_F_PRESENTED | gp::FRAME_TIMING_F_FULL
            && r.presented_ns == done_a1.presented_ns => {}
        other => fail(&format!("k6p rec bad {:?}", other.map(|r| r.flags))),
    }
    release_n(hw, 1);

    // ② 迟到即呈（P1 深度 1 特例保留面）：target 已过 → 即刻入环 + 即答。
    let t_late = now();
    let tok_late = submit_full(hw, vmo, t_late.wrapping_sub(1));
    let rec_late = match timing_probe(hw, tok_late) {
        Some(r) => r,
        None => fail("k6p late probe: 仍挂起（迟到未即呈）"),
    };
    if rec_late.presented_ns < t_late || rec_late.presented_ns > now() {
        fail(&format!(
            "k6p late presented={} 窗 [{},{}]",
            rec_late.presented_ns, t_late, now()
        ));
    }
    if frame_callback(hw, tok_late).status != gp::FRAME_PRESENTED {
        fail("k6p late callback 非 PRESENTED");
    }
    release_n(hw, 1);

    // ③ squash 判弃（深度 1）：后 P2 帧压前 → 前帧环 rec DISCARDED（ts=0）
    // + callback 即答 FRAME_DISCARDED；后帧正常 latch 呈现。B 挂起探针
    // 提到 s1 断言**之前**（hop 3——s1 的 DISCARDED 在 s2 submit 返回时即
    // 成定局、断言与到达时序无关；run9 教训：5 跳链的原序在 TCG 慢拍下
    // 反超窗宽，B 已 latch 假红 ring hit）。
    let target_sq = now() + K6_DEFER_NS;
    let tok_s1 = submit_full(hw, vmo, target_sq);
    let tok_s2 = submit_full(hw, vmo, target_sq);
    if timing_probe(hw, tok_s2).is_some() {
        fail("k6p squash B probe: ring hit（B 应挂起）");
    }
    match timing_probe(hw, tok_s1) {
        // 掩码式判 DISCARDED（squash 保 FULL 位——run3 实证 Some(6)=
        // DISCARDED|FULL，宿主行为正确、精确相等是我探针写错）。
        Some(r) if r.flags & gp::FRAME_TIMING_F_DISCARDED != 0 && r.presented_ns == 0 => {}
        other => fail(&format!("k6p squash rec bad {:?}", other.map(|r| r.flags))),
    }
    if frame_callback(hw, tok_s1).status != gp::FRAME_DISCARDED {
        fail("k6p squash callback 非 DISCARDED");
    }
    let done_s2 = frame_callback(hw, tok_s2);
    if done_s2.status != gp::FRAME_PRESENTED
        || done_s2.presented_ns + (p60 + 5_000_000) < target_sq
        || done_s2.presented_ns > target_sq + 250_000_000
    {
        fail(&format!(
            "k6p B status={} presented={} target={}",
            done_s2.status, done_s2.presented_ns, target_sq
        ));
    }
    release_n(hw, 2);

    // ④ P5 定点提交：COMMIT(slot, future) → ST_OK 挂起（token 0 面）→
    // 墙钟过拍呈现（服务端 `P5C ok latch=` 证据行——smoke canon）→ 管线
    // 排空后继 submit 不 ST_BUSY。target 走 K6_DEFER_NS 窗（RTT 反超 4p
    // 会退化即时呈现——定点面走真挂起）；sleep 盖 latch + 唤醒尾（首版
    // sleep 5p 撞唤醒尾 → slot 被 drain 帧压弃、P5C 永缺）。
    let mut alias_p5: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias_p5) != 0 || alias_p5 == 0 {
        fail("dup alias k6p5");
    }
    let preq6 = gp::PrepareReq {
        flags: 0,
        width: w as u32,
        height: h as u32,
        stride: stride as u32,
        format: gp::FMT_XRGB8888,
    };
    let m = gp::encode_prepare_req(&mut req, &preq6);
    let (n, nh) = rpc(hw, &req[..m], &[alias_p5], &mut rep, &mut rhs);
    let slot6 = match gp::parse_prepare_rep(&rep[..n]) {
        Some(s) => s,
        None => fail(&format!("k6p prepare bad st={}", rep[0])),
    };
    if nh != 1 {
        fail(&format!("k6p prepare fence nh={nh}"));
    }
    close_h(rhs[0]);
    let commit_target = now() + K6_DEFER_NS;
    let m = gp::encode_commit_req(&mut req, slot6, commit_target);
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "k6p commit deferred");
    sleep_ns(K6_DEFER_NS + 250_000_000); // 定点拍 + 唤醒尾 → 呈现毕
    let tok_d = submit_full(hw, vmo, 0); // 排空探针：挂起未fire = ST_BUSY loud
    let _ = frame_callback(hw, tok_d);
    release_n(hw, 1);
    println!("gfx: k6p n=4");

    // (k6t) 165/180 测试档 + T5 时长不变性（DET 专属——行为自门：TEST 档
    // admissible 仅 det_test，非 DET boot 探针落空（FREE 态 G=[160,190] 无
    // 产品档 → Fallback 60）→ 跳段零行）。DET 位级面：虚拟钟网格——拍距
    // 恒为档周期整倍数（每 park 间醒数拍——delta % p == 0 位级恒定，T2）。
    // 档序 165→180 纯升档入场（A5：降档目标 1s 驻留才生效——run6 实证
    // k6h 收尾 480 直投 180 被驻留拦成假跳段；升档 UP_CONFIRM_TICKS=2
    // 两拍即切）。
    if caps6.rate_model == gp::RATE_MODEL_TIERED {
        vote_interactive(hw, 160, 190, 165);
        let _ = vb_point(hw);
        let _ = vb_point(hw);
        if caps_cur_hz(hw) == 165 {
            const K6M: usize = 22;
            let p165 = 6_060_606u64;
            let mut prev2 = 0u64;
            for i in 0..K6M {
                let (_, ts, r) = vb_timing_point(hw);
                if r as u64 != p165 {
                    fail(&format!("k6t refresh={r} expect 165Hz"));
                }
                if i > 0 {
                    let d = ts - prev2;
                    if d == 0 || d % p165 != 0 {
                        fail(&format!("k6t det delta={} 非 {} 整倍", d, p165));
                    }
                }
                prev2 = ts;
            }
            vote_interactive(hw, 160, 190, 180);
            let _ = vb_point(hw);
            let _ = vb_point(hw);
            let hz180 = caps_cur_hz(hw);
            if hz180 != 180 {
                fail(&format!("k6t vote hz={hz180} expect=180"));
            }
            const K6N: usize = 24;
            let p180 = 5_555_556u64;
            let mut prev = 0u64;
            for i in 0..K6N {
                let (_, ts, r) = vb_timing_point(hw);
                if r as u64 != p180 {
                    fail(&format!("k6t refresh={r} expect 180Hz"));
                }
                if i > 0 {
                    let d = ts - prev;
                    if d == 0 || d % p180 != 0 {
                        fail(&format!("k6t det delta={} 非 {} 整倍", d, p180));
                    }
                }
                prev = ts;
            }
            // T5 时长不变性：实测档周期喂真身 FixedStep——A = 24×P180、
            // B = 22×P165（12:11 同总时长）、C = 交替混流。各 steps ==
            // floor(总时长/DT) ∧ 相互差 ≤ 1（整数周期取整的诚实容差——
            // 12×P180 与 11×P165 对 DT 均落在步界 ±18ns 内）∧ 零丢弃。
            let dt = berry_ui::step::STEP_DT_NS;
            let mut fa = FixedStep::new();
            for _ in 0..24 {
                if fa.advance(p180).dropped_steps != 0 {
                    fail("k6t A dropped");
                }
            }
            let mut fb = FixedStep::new();
            for _ in 0..22 {
                if fb.advance(p165).dropped_steps != 0 {
                    fail("k6t B dropped");
                }
            }
            let mut fc = FixedStep::new();
            for i in 0..24 {
                let d = if i % 2 == 0 { p180 } else { p165 };
                if fc.advance(d).dropped_steps != 0 {
                    fail("k6t C dropped");
                }
            }
            let (sa, sb, sc) = (fa.steps(), fb.steps(), fc.steps());
            let (ea, eb, ec) = (24 * p180 / dt, 22 * p165 / dt, (12 * p180 + 12 * p165) / dt);
            if sa != ea || sb != eb || sc != ec {
                fail(&format!("k6t steps a={sa}/{ea} b={sb}/{eb} c={sc}/{ec}"));
            }
            if sa.abs_diff(sb) > 1 || sa.abs_diff(sc) > 1 {
                fail(&format!("k6t invariance a={sa} b={sb} c={sc}"));
            }
            println!("gfx: k6t n=3 a={} b={} c={}", sa, sb, sc);
        }
    }

    // (k6l) A8 LFC 整数倍重发（资格门 vrr==CAPABLE ∧ Tiered——per_frame
    // LFC 天然内含（§6-A8），FreeRange boot 跳段）。编排三步防回弹：
    // ①预抬 240（k6t 收尾 180 升档 2 拍生效——降档入场会被 A5 驻留拦，
    // run6 实证 480→240 假红）——LFC 结果档 60/120
    // 皆 ≠ 240 使 sw 可观察；②换 Unspecified [30,60]——目标 60 走 A5
    // 驻留（≥1s 不即切，无升档面）；③8 帧内容连发——TCG RPC 节律即
    // < gmin 的内容 cadence，cc 窗 ≥3 帧跨 ≥500ms 即裁决（k = ceil(60/c)
    // ≥ 2，target = k×c < 60+c ≤ 120 → 档 60/120），cc 即时提交抢在
    // 驻留前。非整除 cadence 落 120、整除落 60、驻留先到落 60——三形
    // 皆合法：断言 hz ∈ {60,120}（engagement 证据 = 服务端 `PANELARB
    // lfc k=` 换行——smoke canon）。
    if caps6.vrr == gp::PANEL_VRR_CAPABLE && caps6.rate_model == gp::RATE_MODEL_TIERED {
        vote_interactive(hw, 60, 480, 240);
        let _ = vb_point(hw);
        let _ = vb_point(hw);
        let hz_pre = caps_cur_hz(hw);
        if hz_pre != 240 {
            fail(&format!("k6l pre hz={hz_pre} expect=240"));
        }
        // ②换 Unspecified [30,60]——目标 60（无偏好票走 no_vote 档过
        // G 钳到顶 60）→ 降档走 A5 驻留（≥T_DOWN_DWELL_NS 不即切，
        // 无升档面）——后续升档票面已撤，LFC 结果不被票面回弹覆盖。
        let m = enc_req(gp::encode_present_req(
            &mut req,
            &gp::PresentReq::Vote {
                sticky: 0,
                scene: gp::Intent::Unspecified as u8,
                min_hz: 30,
                max_hz: 60,
                preferred_hz: 0,
            },
        ));
        expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "k6l vote dwell");
        for _ in 0..8 {
            let tok = submit_full(hw, vmo, now() + 40_000_000);
            let d = frame_callback(hw, tok);
            if d.status != gp::FRAME_PRESENTED {
                fail(&format!("k6l frame status={}", d.status));
            }
            release_n(hw, 1);
        }
        let _ = vb_point(hw); // LFC 生效窗（受理 2 帧）
        let hz_lfc = caps_cur_hz(hw);
        if hz_lfc != 60 && hz_lfc != 120 {
            fail(&format!("k6l hz={hz_lfc} expect LFC 降档 60/120"));
        }
        println!("gfx: k6l hz={hz_lfc}");
    }

    // (k6h) 240/480 产品档宣告 + 高刷拍点门（§10-T3 高刷档收紧：240Hz
    // p99 ≤ 500µs / 480Hz ≤ 250µs——TCG 档，首跑后按实测校定；真机
    // 125/62.5µs 归刀7 硬件在环）。101 park（rank 99 容 1 离群——k4t
    // 同口径）；miss 计数透出（FREE 迟醒跳点 / P6 自刷新 1s 重扫的非门面）。
    // 段序在 k6t/k6l 后：k6l 收尾 60/120 → 240/480 纯升档入场（A5 降档
    // 驻留不挡升路）；主 boot k6t/k6l 双跳段，k6p 后直接本段（序不变）。
    // 预热 4 拍 = A5 升档 2 拍确认（Event 不直 commit——streak 由拍积，
    // 第 2 拍 commit 时该拍已应答）+ 受理后 2 帧生效窗：per_frame 空闲态
    // 拍 = 1Hz 自刷新重扫（vsync_schedule 对 per_frame no-op，无内容即
    // 无驱动——run8 教训：2 拍预热在 caps 前只走到 commit，读回 60）；
    // 周期形态受理后自排驱动拍（~33ms << RTT），4 拍仅多 ~64ms 无碍。
    vote_interactive(hw, 0, 480, 240);
    for _ in 0..4 {
        let _ = vb_point(hw);
    }
    let hz24 = caps_cur_hz(hw);
    if hz24 != 240 {
        fail(&format!("k6h vote hz={hz24} expect=240"));
    }
    const K6G: usize = 101;
    let mut gts24 = [0u64; K6G];
    for i in 0..K6G {
        let (_, ts, r) = vb_timing_point(hw);
        if r as u64 != 4_166_667 {
            fail(&format!("k6h refresh={r} expect 240Hz"));
        }
        gts24[i] = ts;
    }
    let mut d240 = [0u64; K6G - 1];
    for i in 0..d240.len() {
        if gts24[i + 1] <= gts24[i] {
            fail("k6h(240) ts not advancing");
        }
        d240[i] = gts24[i + 1] - gts24[i];
    }
    let (p99_240, miss240) = resid_p99(&d240, 4_166_667);
    if p99_240 > 500_000 {
        fail(&format!("k6h p99(240)={}ns > 500µs", p99_240));
    }
    println!("gfx: k6h hz=240 p99={}ns miss={}", p99_240, miss240);
    vote_interactive(hw, 0, 480, 480);
    for _ in 0..4 {
        let _ = vb_point(hw);
    }
    let hz48 = caps_cur_hz(hw);
    if hz48 != 480 {
        fail(&format!("k6h vote hz={hz48} expect=480"));
    }
    let mut gts48 = [0u64; K6G];
    for i in 0..K6G {
        let (_, ts, r) = vb_timing_point(hw);
        if r as u64 != 2_083_333 {
            fail(&format!("k6h refresh={r} expect 480Hz"));
        }
        gts48[i] = ts;
    }
    let mut d480 = [0u64; K6G - 1];
    for i in 0..d480.len() {
        if gts48[i + 1] <= gts48[i] {
            fail("k6h(480) ts not advancing");
        }
        d480[i] = gts48[i + 1] - gts48[i];
    }
    let (p99_480, miss480) = resid_p99(&d480, 2_083_333);
    if p99_480 > 250_000 {
        fail(&format!("k6h p99(480)={}ns > 250µs", p99_480));
    }
    println!("gfx: k6h hz=480 p99={}ns miss={}", p99_480, miss480);

    // (k6f) P6 逐帧锁相（FreeRange per_frame 专属——资格门 model==
    // FREERANGE ∧ flags PER_FRAME；Tiered boot 跳段）：latch = scan_point
    // = max(上一扫描+min_interval, target) → target 即拍点，无网格钳制
    //（网格面板会钳到 ≤target 的格点——残差最大 −p 均匀分布，本门即
    // 零 judder 判别面）。10 帧各 target = now+K6_DEFER_NS（裕过 TCG RPC
    // 往返）→ 阻塞 callback → presented−target = 唤醒过冲（FREE 态 on_scan
    // 墙钟直通）；判别面 = 永不提前（r ≥ −100µs 硬门）；上界帽 = TCG
    // 粗定时器唤醒尾（BENCHWAKE p99≈16.7ms 系空载测量——run10 实证宿主
    // 争用下 5/10 帧尾 >50ms，环境面非模型面，帽抬 300ms 容 1 离群）；
    // 真机收紧归刀7。
    if caps6.rate_model == gp::RATE_MODEL_FREERANGE
        && (caps6.flags & gp::PANEL_FLAG_PER_FRAME) != 0
    {
        const K6F: usize = 10;
        let mut resid_max = 0u64;
        let mut bad = 0usize;
        for _ in 0..K6F {
            // 挂起窗 = K6_DEFER_NS（> worst RTT 1.4s——1s 窗探针晚到即
            // 假红 ring hit，与 k6p ① 同病）。
            let target = now() + K6_DEFER_NS;
            let tok = submit_full(hw, vmo, target);
            if timing_probe(hw, tok).is_some() {
                fail("k6f probe: ring hit（逐帧 target 未挂起）");
            }
            let d = frame_callback(hw, tok);
            if d.status != gp::FRAME_PRESENTED {
                fail(&format!("k6f status={}", d.status));
            }
            let r = (d.presented_ns as i64).saturating_sub(target as i64);
            if r < -100_000 {
                fail(&format!("k6f resid={}ns 提前于拍点", r));
            }
            let ra = r.unsigned_abs();
            if ra > resid_max {
                resid_max = ra;
            }
            if ra > 300_000_000 {
                bad += 1;
            }
            release_n(hw, 1);
        }
        if bad > 1 {
            fail(&format!("k6f bad={} > 1（唤醒过冲帽 300ms）", bad));
        }
        println!("gfx: k6f p99={}ns bad={}", resid_max, bad);
    }

    // 收尾回中性票（Unspecified——尾部 PREPARE/COMMIT 仪式与后续 demo 语义
    // 与 (d) 收尾同款）。
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::Vote {
            sticky: 0,
            scene: gp::Intent::Unspecified as u8,
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 0,
        },
    ));
    expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote neutral 3");

    let mut alias2: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias2) != 0 || alias2 == 0 {
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

    // —— 波一刀 B（残帧清屏）：自测图案是 boot 期唯一上屏帧
    //     （blit-at-SUBMIT），退出后残留 fb 上直到 launcher 首帧——
    //     「测试图案当桌面」误读窗（research/桌面UX审计/02 §2.1/
    //     §3 修法 B1）。退出前提交一帧纯黑（FULL 形 K=0）：gfxdemo
    //     退场屏幕归黑，launcher 首帧（FULL 壁纸）前的间隔显示黑而非
    //     测试图案。credits 全周期与首帧同形（SUBMIT→FRAME_CALLBACK→
    //     RELEASE，在途 ≤1 不变量保持）。
    fb.fill(0);
    let mut alias3: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias3) != 0 || alias3 == 0 {
        fail("dup alias3");
    }
    let creq = gp::PresentReq::Submit {
        surface_id: 0,
        buffer_id: 2,
        flags: gp::SUBMIT_F_DAMAGE_FULL,
        rate_hint: 0,
        target_present_ns: 0,
        rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
        rect_count: 0,
    };
    let m = enc_req(gp::encode_present_req(&mut req, &creq));
    let (n, _) = rpc(hw, &req[..m], &[alias3], &mut rep, &mut rhs);
    let token = match gp::parse_submit_ack(&rep[..n]) {
        Some(t) => t,
        None => fail(&format!("clear submit ack bad st={}", rep[0])),
    };
    let m = enc_req(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::FrameCallback { frame_token: token },
    ));
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_frame_done_rep(&rep[..n]) {
        Some(d) if d.status == gp::FRAME_PRESENTED => {}
        Some(d) => fail(&format!("clear frame status={}", d.status)),
        None => fail(&format!("clear frame done bad st={}", rep[0])),
    }
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
    let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let rel = match gp::parse_release_rep(&rep[..n]) {
        Some(r) => r,
        None => fail(&format!("clear release rep bad st={}", rep[0])),
    };
    if rel.count != 1 || nh != 1 {
        fail(&format!("clear release count={} nh={}", rel.count, nh));
    }
    close_h(rhs[0]);

    // —— DESTROY（wm 域四步之尾：本程序建的全屏 APP 层窗随进程退场显式
    //    销毁——本程序走 wm_srv 非 apps 通道，svcmgr 无 `destroy_owned` 死亡
    //    对账兜底；不销 = 幽灵窗常驻窗口表，launcher `on_covered_tap` 把此后
    //    每一击都当「覆盖态收盖」吞掉（桌面点击无反应），且 recents/back 的
    //    APP 层判据全部被它污染）。失败不拦退场——打诊断行即可。
    let m = gp::encode_wm_destroy_req(&mut req, wid);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    if gp::parse_status_rep(&rep[..n]) != Some(gp::ST_OK) {
        eprintln!("gfx: wm destroy id={} st={}", wid, rep[0]);
    }

    // 客户端全链断言过（服务端真值 GFXF ok n= 由 guifbd 首呈现时打出）
    // ——结构性恰一次：svcmgr 单次预拉 + 阻塞 reap + 单 println。
    println!("GFX ok");
    exit(0);
}
