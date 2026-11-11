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
use berry_libc::io::{self_vmar_h, wm_cli_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_event_create,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_one, berry_vmar_map,
    berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    CLOCK_MONOTONIC, RIGHT_READ, RIGHT_WRITE, SIGNAL_READABLE, SIGNAL_WRITABLE,
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
    // 底部绿条 + 白字 "GFX" 深蓝衬底；dirty 无 = 整帧）。
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
    if let Err(e) = Queue::new().submit(&dl, fb, &desc, &PassDescriptor::clear(0x10, 0x18, 0x28)) {
        fail(&format!("render submit {e:?}"));
    }

    // —— SUBMIT（K=2 damage rect：上下半帧分区，并集 = 整帧——服务端
    // px() 抽样 7 点恒落并集内，§9 A5-b；flags=0 非 FULL =「K>0 或 FULL
    // 二取一」立法的 K 侧）。随行 = 本 VMO 的 READ dup 别名：send 即 take
    // （A6-b）——原 vmo handle 留表，PREPARE 时再 dup 新别名。
    let mut alias: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
        fail("dup alias");
    }
    let half = (h / 2) as u32;
    let mut rects = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
    rects[0] = gp::Rect { x: 0, y: 0, w: w as u32, h: half };
    rects[1] = gp::Rect { x: 0, y: half, w: w as u32, h: h as u32 - half };
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

    // 客户端全链断言过（服务端真值 GFXF ok n= 由 guifbd 首呈现时打出）
    // ——结构性恰一次：svcmgr 单次预拉 + 阻塞 reap + 单 println。
    println!("GFX ok");
    exit(0);
}
