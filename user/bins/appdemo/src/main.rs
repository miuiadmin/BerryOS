//! appdemo —— 控件画廊常驻演示（波 4 §2.8：42 行秒退自验件的根本翻新；
//! `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.8、worktree
//! 根 W4-RECON.md §四 APPDEMO 行）。
//!
//! **使命**：berry-ui pattern.rs 模式件 + 引擎核心 ViewKind 的活体验收
//! 场——`src/gallery.rs` 建树（pattern 26 格 + kind 22 格，格子清单口径
//! 对齐 `user/berry-ui/tests/gallery2.rs`），Scroll 拖滚翻览，交互件全走
//! 引擎内建面（widget_react / press_map+PressedVis），app 侧只接四件：
//! 页滚动 / Seg·TabBar 换档 / Sheet·Drawer 开合 / Refresh·SwipeItem·
//! DragReorder 手势器件。
//!
//! **常驻化（对秒退自验件的根本变更）**：`app APPDEMO up` 与
//! `app APPDEMO fs ok 23` 两行 console/fs 自证面**原序保留**（gui8 点击
//! spawn 断言载体 + fs 词消费侧自证——设计稿 §2.8 明令不动），随后进入
//! 常驻帧循环**永不退出**（退出形态照 settings 先例：fail = eprintln +
//! park；崩溃监督归 svcmgr app_supervise 域）。
//!
//! **链形态** = settings 垂直切片镜像（9035af061 先例；contacts/files
//! 同批同形）：`wm_present_h()` 门控（本 app 带 fs 词 → svcmgr GUI 授权
//! gift 布局 [rootfs, fs, wm, input]——wm 落 extras[2]，`wm_cli_h()` 的
//! launcher 镜像位不适用；W4-IMPL-FILES.md §三.2 同槽访问器先例）→
//! WM_GETINFO 几何 → present GETINFO 轻断言 → 帧缓冲 VMO 三连
//! （create/commit/map @ FRAME_VA）→ 三态停帧 + SUBMIT → FRAME_CALLBACK
//! → RELEASE 严格内联串行 + vote 票面变化才发。输入 = input_h()
//! INPUT_POLL 批拉 → push_pointer → pump_input（手势 referee +
//! widget_react 同拍）→ Msg/组件事件双面消费。
//!
//! **v1 降解面**（3b 未落——完整清单见 W4-IMPL-APPDEMO.md）：无
//! berry-appshell（RPC 半程手拼 + 自管帧循环）；Toast/Dialog/Menu 三
//! 演示位降级为应用内 Sheet/Drawer/Collapse 器件近似（Stack 叠层语）；
//! 无系统 back 链（画廊单页滚动形态无页栈——back 消费面天然缺席）。
//!
//! **输出面**：manifest console=true → stdout 走 console 服务（串口可
//! 见，两自证行构成既定断言面）；eprintln 族 = 诊断面。首帧呈现回执
//! 一行 `appdemo: first frame`（非标记形态）。

use berry_gfx::cmd::{CommandEncoder, GlyphPos, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::io::{input_h, self_vmar_h, wm_present_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_many, berry_object_wait_one, berry_vmar_map,
    berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_ui::desc::{Axis, ScrollProps};
use berry_ui::pattern;
use berry_ui::{Msg, ProportionalTextMetrics, Props, UiMain};

mod gallery;

use gallery::{Gallery, EV_DRAWER_CLOSE, EV_DRAWER_OPEN, EV_SHEET_CLOSE, EV_SHEET_OPEN};

// 链接种子（fsmgr/guifbd/berry-launcher/settings 同款）：rlib/LTO 按需
// 保留——无引用则 berry-libc 的 crt0（含 `_start`）不进链接；`#[used]`
// 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（launcher/settings 同值 300s：app 拉起时点 =
/// 用户点击，首 RPC 距 boot 稳态可远）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 帧缓冲映射 VA（launcher frameloop 同位：mmap VA 窗内固定位 +16MiB）。
const FRAME_VA: usize = 0x5100_0000;

/// 默认 vsync 周期（60Hz；present GETINFO 无周期域——S2 食谱默认）。
const VSYNC_NS: u64 = 16_666_666;

/// presarb ST_BUSY 步进重试帽（wmd 呈现域单飞仲裁宽容面）：他客户端
/// 事务在途（属主租约窗内）时事务边界 op（SUBMIT/RELEASE_WAIT）得 1B
/// ST_BUSY 快答不转发——按 vsync 拍步进重试；属主挂死时租约（wmd
/// PRESENT_LEASE_NS 5s）到点后本端下一请求即抢占清场形放行，合法等待
/// 上界 = 租约 + 一趟清场（秒级）——本帽取 wmd RELAY_DRAIN_CAP_NS 同值
/// 35s 作一个量级余量；到点仍 BUSY = 呈现域结构性破相 fail loud。
const BUSY_RETRY_CAP_NS: u64 = 35 * 1_000_000_000;

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// glyph scratch 容量（堆置——paint 对超容 run 截断尾字形，视觉面非
/// 正确性面；画廊满页 ~50 文本域，视口内可见字形估 ~250——2× 富余）。
const SCRATCH_GLYPHS: usize = 512;

/// 整帧清屏底色 = 页面底 token 单源（light 档 BG_PAGE；theme.rs 唯一
/// 归属，本文件零清屏色值字面量）。
const CLEAR_BG: berry_gfx::cmd::Color = berry_ui::theme::BG_PAGE;

/// Refresh 演示驻留时长（越阈触发后指示器全露驻留——期满 settle 回
/// 弹；appshell Toast「2s 短驻留」同拍量级）。
const REFRESH_SPIN_NS: u64 = 1_500_000_000;

/// 诊断 + park 收口（settings `fail` 同形——app 常驻不被 reap；崩溃
/// 监督归 svcmgr app_supervise 域，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("appdemo: {what}");
    berry_libc::park()
}

/// now = clock_read；失败按 0 → deadline 即刻到期 = fail-fast。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 即关（RELEASE 归还别名——关失败 = handle 表泄漏，FAIL loud）。
fn close_h(h: Handle) {
    if berry_handle_close(h) != 0 {
        fail("handle close");
    }
}

/// 严格串行 RPC **发送半程**（launcher `rpc_send` 同形）：-EBUSY → 阻塞
/// 等 WRITABLE（帽重臂不自旋）；send 原子性——EBUSY/错码时随行 handle
/// 零迁移。**send 即 take**（§9 A6-b）。
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

/// 严格串行 RPC **收侧半程**（launcher `rpc_recv` 同形）：空队 -EAGAIN →
/// 阻塞等 READABLE（帽）；每轮重置容量（in-out 出参漏重置 = 死循环）。
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
            fail("rpc reply timeout");
        }
    }
}

/// 严格串行 RPC 单趟（一请求一应答）。
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

/// presarb ST_BUSY 步进等待（vsync 拍一步）：hw 通道无自发帧面（本端
/// 未订阅事件推送），READABLE 短等到帽恒超时 = 睡满一拍；提前醒（异常
/// 来帧）无害——重试侧 rpc 严格一问一答自对账。
fn busy_step(hw: Handle) {
    let mut obs = 0u64;
    berry_object_wait_one(hw, SIGNAL_READABLE, now() + VSYNC_NS, &mut obs);
}

/// RELEASE_WAIT 一趟（presarb 宽容面——步进重试）：他客户端呈现事务
/// 在途（wmd 单飞闸门租约窗内）时 RELEASE 边界 op 得 1B ST_BUSY 快答
/// 不转发——步进重试至放行（boot 跨代清账与帧尾收口共用本面）；其余
/// 畸形应答 = fail loud。成功形返回应答产物与随行 handle 数（count
/// 语义面归调用方执法）。
fn release_wait(
    hw: Handle,
    what: &str,
    req: &mut [u8; 256],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (gp::ReleaseRep, usize) {
    let mut waited = 0u64; // 步进累计（帽账）
    loop {
        let m = match gp::encode_present_req(req, &gp::PresentReq::ReleaseWait) {
            Some(m) => m,
            None => fail("release encode"),
        };
        let (n, nh) = rpc(hw, &req[..m], &[], rep, rhs);
        if let Some(rel) = gp::parse_release_rep(&rep[..n]) {
            return (rel, nh);
        }
        if gp::parse_status_rep(&rep[..n]) != Some(gp::ST_BUSY) {
            fail(&format!("{what} bad st={}", rep[0]));
        }
        if waited >= BUSY_RETRY_CAP_NS {
            fail(&format!("{what} busy cap"));
        }
        busy_step(hw);
        waited += VSYNC_NS;
    }
}

/// K 矩形并集包络（增量 pass 的 dirty 域；launcher/settings 同形——
/// engine damage 恒 viewport 裁剪，i32 折 try_from 饱和防御）。
fn damage_envelope(dmg: &berry_ui::damage::DamageSet) -> GfxRect {
    let mut x0 = u32::MAX;
    let mut y0 = u32::MAX;
    let mut x1 = 0u32;
    let mut y1 = 0u32;
    for r in &dmg.rects[..dmg.count as usize] {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x.saturating_add(r.w));
        y1 = y1.max(r.y.saturating_add(r.h));
    }
    let (ew, eh) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    GfxRect::new(
        i32::try_from(x0).unwrap_or(0),
        i32::try_from(y0).unwrap_or(0),
        i32::try_from(ew).unwrap_or(0).max(0),
        i32::try_from(eh).unwrap_or(0).max(0),
    )
}

/// 挂一轮 INPUT_POLL（launcher `input_arm` 同形）：inputd 阻塞批拉 ≤3
/// 事件；-EBUSY 等 WRITABLE 重试；其余错 = 输入面失效（调用方置 hi=0
/// 静态页续活不死）。
fn input_arm(h: Handle) -> bool {
    let mut req = [0u8; 4];
    let n = gp::encode_input_poll_req(&mut req, gp::INPUT_EVENTS_MAX as u8);
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, req.as_ptr(), n, core::ptr::null(), 0);
        if s == 0 {
            return true;
        }
        if s != BERRY_ERR_BUSY {
            eprintln!("appdemo: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("appdemo: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}

/// 滚动偏移累进（clamp 非负 + 不越尾——overscroll 无橡皮筋，v1 直钳；
/// contacts `scroll_by` 同形）。
fn scroll_by(ui: &mut UiMain, g: &mut Gallery, dy: i32) {
    let max_off = g.content_h.saturating_sub(g.vp_h);
    let next = (g.offset as i64 + dy as i64).clamp(0, max_off as i64) as u32;
    if next == g.offset {
        return;
    }
    g.offset = next;
    let props = Props::Scroll(ScrollProps { content_height: g.content_h, offset_y: g.offset });
    if let Err(e) = ui.set_props(g.scroll, props) {
        eprintln!("appdemo: scroll set err={e:?}");
    }
}

fn main() {
    // —— argv0 自检（manifest `entry` 串——appd 以 entry 作 SpawnSpec.
    //    name；错值 = 断言炸 → abort(134) → reap 非 0 → `APP ok` 缺失
    //    = smoke FAIL loud。秒退自验件时期同面原序保留）。
    assert_eq!(
        std::env::args().next().as_deref(),
        Some("bin/appdemo"),
        "argv0 should be manifest entry"
    );
    println!("app APPDEMO up");

    // —— fs 词自证（SDK-1a §6.2，原序保留）：libc open/read 经 fsclient
    //    → fs_h gift → fsmgr RPC 真往返；mtab = fsmgr RAM 树预建定值
    //    （盘相无关）。失败全 loud（expect/assert → abort → reap 非 0）。
    let mtab = std::fs::read("/etc/mtab").expect("appdemo: mtab read");
    assert_eq!(mtab, b"/dev/root / fat rw 0 0\n", "mtab content drift");
    println!("app APPDEMO fs ok 23");

    // —— 降级门：wm 客户端端未交付（GUI 通道授权缺席——本 app 带 fs
    //    词，gift 布局 [rootfs, fs, wm, input]，wm 落 extras[2] 经
    //    `wm_present_h()` 同槽访问器取；0 = 未授权/裸相）→ 一行诊断 +
    //    park 常驻（settings「wm absent」同形——两自证行已打，窗口
    //    义务语义不折衷成秒退）。
    let hw = wm_present_h();
    if hw == 0 {
        eprintln!("appdemo: wm absent (no gui gift)");
        berry_libc::park();
    }

    // —— WM_GETINFO 一次握手（几何真值源；parse 失败 = 诊断 + park）。
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_getinfo_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let info = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("wm getinfo rep bad st={}", rep[0])),
    };
    eprintln!(
        "appdemo: wm v={} {}x{} input={}",
        info.version_wm,
        info.disp_w,
        info.disp_h,
        input_h()
    );

    // —— UI arena 堆置（settings 同形：UiMain 构造中间体 LTO 后部分落
    //    栈 ~29KB——8 页栈穿先例，Box::leak = 常驻无回收语义）。比例
    //    文本供件 + glyph 真供件（画廊 pattern::icons 域 7 件 + 缩略图）。
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    static TEXT_METRICS: ProportionalTextMetrics = ProportionalTextMetrics;
    ui.set_text_metrics(&TEXT_METRICS);
    ui.set_icon_source(&gallery::ICONS);

    // —— 建树（Err = 容量/文本域违例 → fail 收口；NodeFull = 节点预
    //    算超帽的 loud 面）。
    let mut g = match gallery::build_all(ui) {
        Ok(g) => g,
        Err(e) => fail(&format!("gallery build err={e:?}")),
    };

    // —— 帧循环（常驻义务——永不返回）。
    run(hw, input_h(), ui, &mut g);
}

/// 帧循环 + 输入 + 事件消费（settings 食谱 + 画廊四增量：拖滚 / 手势
/// 器件 Msg 转发（SwipeItem·Refresh·DragReorder 自判几何命中）/ 吸附
/// tick 族 / Sheet·Drawer 按钮 · Seg·TabBar 换档）。
fn run(hw: Handle, hi_in: Handle, ui: &'static mut UiMain, g: &mut Gallery) -> ! {
    // —— present GETINFO（wmd 中继同端）：能力面轻断言 + 几何一致性。
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = match gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo) {
        Some(m) => m,
        None => fail("present getinfo encode"),
    };
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let pinfo = match gp::parse_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("present getinfo bad st={}", rep[0])),
    };
    if pinfo.version_present != gp::VER_PRESENT
        || pinfo.format_bits & gp::FMT_XRGB8888 == 0
        || pinfo.max_credits != gp::CREDITS_MAX
    {
        fail(&format!(
            "present getinfo vp={} fmt={:#x} credits={}",
            pinfo.version_present, pinfo.format_bits, pinfo.max_credits
        ));
    }
    let (vp_w, vp_h) = ui.viewport();
    if pinfo.display_w != vp_w || pinfo.display_h != vp_h {
        fail(&format!(
            "disp mismatch wm {}x{} present {}x{}",
            vp_w, vp_h, pinfo.display_w, pinfo.display_h
        ));
    }

    // —— 跨代清账（前代实例孤儿别名全量排空；presarb 宽容面：拉起时点
    //    桌面常在属主租约窗内，他客户端事务在途 = ST_BUSY 步进重试——
    //    原 fail 形即「进应用即 park 永死」单发面之一）。
    let (_, nh) = release_wait(hw, "boot release", &mut req, &mut rep, &mut rhs);
    for h in &rhs[..nh] {
        close_h(*h);
    }

    // —— 帧缓冲 VMO 三连（stride = w*4 tight；len 页取整）。
    let (w, h) = (pinfo.display_w as usize, pinfo.display_h as usize);
    let stride = w.checked_mul(4).unwrap_or_else(|| fail("stride overflow"));
    let frame_bytes = stride.checked_mul(h).unwrap_or_else(|| fail("frame overflow"));
    let len = frame_bytes
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
    if berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0 || va != FRAME_VA
    {
        fail("vmar map");
    }
    let desc = match TextureDesc::new(w, h, stride, TextureFormat::Rgbx) {
        Ok(d) => d,
        Err(e) => fail(&format!("texture desc {e:?}")),
    };
    // SAFETY: [FRAME_VA, FRAME_VA+len) 刚由 vmar_map 落位且整段已
    // commit（settings/launcher 同形）；run 永不返回——fb 终身持有。
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };

    // —— glyph scratch 堆置（DisplayList 'a 钉调用方 scratch）。
    let scratch: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());

    // —— vsync 协商 + 起振（Idle→Run + 条件 FULL——首帧整帧 damage）。
    ui.set_vsync(VSYNC_NS, 0);
    ui.wake(now());

    let mut last_expected: u64 = 0; // pump 重采样 target（上帧 expected 轨）
    let mut last_vote: Option<berry_ui::frame::VoteBallot> = None; // 票面变化才发
    let mut presented: u64 = 0; // FRAME_CALLBACK 过链计数（首帧诊断门）
    let mut buf_id: u32 = 1; // SUBMIT buffer 代际
    let mut hi = hi_in; // input 端（失效面置 0——静态页续活）
    let mut input_pending = false; // INPUT_POLL 在途
    let mut input_buf = [0u8; gp::INPUT_REP_MAX];
    let mut input_hs = [0 as Handle; 8];
    let mut scroll_drag: Option<i32> = None; // 页拖滚纵坐标
    let mut refresh_until: Option<u64> = None; // Refresh 演示驻留期限

    loop {
        let t_now = now();

        // —— 真实输入（settings 同形）：先收上一轮 INPUT_POLL 应答，再
        //    挂下一轮 poll；事件逐条 push 进引擎（widget_react 在
        //    pump_input 期同拍消费——Toggle/Check/Radio/Seg/Tabs/Step/
        //    Select/Collapse/Chip/Slider/Input 焦点全引擎面）。
        if hi != 0 && input_pending {
            let mut nb = input_buf.len();
            let mut nh = input_hs.len();
            let s = berry_channel_recv(
                hi,
                input_buf.as_mut_ptr(),
                &mut nb,
                input_hs.as_mut_ptr(),
                &mut nh,
            );
            if s == 0 {
                input_pending = false;
                for h in &input_hs[..nh] {
                    close_h(*h);
                }
                match gp::parse_input_events_rep(&input_buf[..nb]) {
                    Some(Ok(view)) => {
                        for i in 0..view.count() {
                            if let Some(ev) = view.event(i) {
                                let pe = berry_ui::hit::PtrEvent::from_wire(
                                    ev.phase,
                                    ev.pointer_id,
                                    ev.x,
                                    ev.y,
                                    ev.timestamp_ns,
                                    ev.buttons,
                                    ev.pressure,
                                );
                                if let Err(e) = ui.push_pointer(pe) {
                                    eprintln!("appdemo: input push err={e:?}");
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {} // 无事件常态
                    Some(Err(st)) => eprintln!("appdemo: input poll st={st}"),
                    None => eprintln!("appdemo: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("appdemo: input recv err={s}");
                hi = 0;
                input_pending = false;
            }
        }
        if hi != 0 && !input_pending {
            if input_arm(hi) {
                input_pending = true;
            } else {
                // 输入面失效——置 0 永久降级静态页（页面常驻义务不受拖累）。
                hi = 0;
            }
        }

        // —— 输入泵（referee + 重采样 + widget_react/press_react 同拍）
        //    + Msg 面：页拖滚（Y 轴未被器件认领的拖）；SwipeItem（X 轴
        //    自判行界）/Refresh（Y 轴自判帧界）/DragReorder（Y 轴自判
        //    容器界）三器件几何自判转发；Click = Seg/TabBar hit 换档。
        let msgs = ui.pump_input(last_expected);
        for m in msgs.iter() {
            match *m {
                Msg::DragStart { axis, y, .. } => match axis {
                    Axis::X => {
                        if g.swipe.on_msg(ui, m) {
                            ui.wake(now());
                        }
                    }
                    Axis::Y => {
                        let mut claimed = g.refresh.on_msg(ui, m);
                        if !claimed {
                            claimed = g.dragr.on_msg(ui, m);
                        }
                        if !claimed && y >= pattern::TOPBAR_H {
                            scroll_drag = Some(y as i32); // 页拖滚占用
                            claimed = true;
                        }
                        if claimed {
                            ui.wake(now());
                        }
                    }
                },
                Msg::DragMove { y, .. } => {
                    let mut anim = g.swipe.on_msg(ui, m);
                    anim |= g.refresh.on_msg(ui, m);
                    anim |= g.dragr.on_msg(ui, m);
                    if let Some(ly) = scroll_drag {
                        let dy = ly - y as i32;
                        if dy != 0 {
                            scroll_by(ui, g, dy);
                            scroll_drag = Some(y as i32);
                            anim = true;
                        }
                    }
                    if anim {
                        ui.wake(now());
                    }
                }
                Msg::DragEnd { .. } => {
                    let mut anim = g.swipe.on_msg(ui, m);
                    anim |= g.dragr.on_msg(ui, m);
                    // Refresh 越阈回 true = 触发刷新（驻留节拍起拍）。
                    let fired = g.refresh.on_msg(ui, m);
                    if fired {
                        refresh_until = Some(now() + REFRESH_SPIN_NS);
                    }
                    anim |= fired;
                    scroll_drag = None;
                    if anim {
                        ui.wake(now());
                    }
                }
                Msg::Cancel { .. } => {
                    let mut anim = g.swipe.on_msg(ui, m);
                    anim |= g.refresh.on_msg(ui, m);
                    anim |= g.dragr.on_msg(ui, m);
                    scroll_drag = None;
                    if anim {
                        ui.wake(now());
                    }
                }
                Msg::Click { node } => {
                    // Seg/TabBar 换档（pattern Cell hit 面——bg/icon/label
                    // 任一锚命中即该席）。
                    if let Some(i) = g.seg_cell.hit(node) {
                        if g.seg_cell.set_active(ui, i) {
                            ui.wake(now());
                        }
                    }
                    if let Some(i) = g.tab_cell.hit(node) {
                        if g.tab_cell.set_active(ui, i) {
                            ui.wake(now());
                        }
                    }
                }
                Msg::Press { .. } | Msg::Release { .. } => {
                    // 按压视觉 = 引擎 press_map/PressedVis + widget_react
                    // 面——app 零接线（行级 ItemState 换底无画廊行，缺席）。
                }
            }
        }

        // —— 吸附/推拉动画步进族（无在途 → false 零开销；在途 → 每帧
        //    推进 + 保活）。
        let mut anim = g.sheet.tick(ui, t_now);
        anim |= g.drawer.tick(ui, t_now);
        anim |= g.swipe.tick(ui, t_now);
        if anim {
            ui.wake(t_now);
        }

        // —— Refresh 驻留期满 settle（指示器回弹 + kind refreshing 归零）。
        if let Some(dl) = refresh_until {
            if t_now >= dl {
                refresh_until = None;
                g.refresh.settle(ui);
                ui.wake(t_now);
            }
        }

        // —— 组件事件消费（引擎 widget_react 已发——纯 app 面）：
        //    Sheet/Drawer 开合按钮 → set_open（起吸附动画）。
        let evs = ui.take_events();
        for ev in evs.iter() {
            match ev.event {
                EV_SHEET_OPEN => {
                    g.sheet.set_open(ui, true);
                    ui.wake(now());
                }
                EV_SHEET_CLOSE => {
                    g.sheet.set_open(ui, false);
                    ui.wake(now());
                }
                EV_DRAWER_OPEN => {
                    g.drawer.set_open(ui, true);
                    ui.wake(now());
                }
                EV_DRAWER_CLOSE => {
                    g.drawer.set_open(ui, false);
                    ui.wake(now());
                }
                _ => {} // EV_DEMO 族 = 按压/翻转视觉即演示面（含 SwipeItem
                        // on_action 与 DragReorder on_reorder——落位动画
                        // 在器件内，无 app 动作）
            }
        }

        // —— 渲染（damage 空集零提交——跳过帧零 RPC）。
        let mut enc = CommandEncoder::new();
        let out = ui.render_frame(t_now, scratch, &mut enc);
        last_expected = out.expected_present_ns;
        if out.drew {
            let dmg = out.damage;
            // K>0 xor FULL 立法兜底：count==0 非空集视同整帧。
            let full = dmg.full || dmg.count == 0;
            let pass = if full {
                PassDescriptor::clear(CLEAR_BG.r, CLEAR_BG.g, CLEAR_BG.b)
            } else {
                // 增量帧：Load+Store + K 矩形并集包络。
                PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(damage_envelope(&dmg)))
            };
            let dl = enc.finish();
            if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
                fail(&format!("raster submit {e:?}"));
            }

            // SUBMIT：fresh READ dup 别名随行（send 即 take——原 vmo 留
            // 表；TRANSFER 保位——随行预检执法）。
            let mut alias: Handle = 0;
            if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0
                || alias == 0
            {
                fail("dup alias");
            }
            let (flags, rects, k) = if full {
                (
                    gp::SUBMIT_F_DAMAGE_FULL,
                    [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
                    0usize,
                )
            } else {
                (0u8, dmg.rects, dmg.count as usize)
            };
            let sreq = gp::PresentReq::Submit {
                surface_id: 0,
                buffer_id: buf_id,
                flags,
                rate_hint: out.rate_hint_hz,
                target_present_ns: out.expected_present_ns,
                rects,
                rect_count: k,
            };
            // SUBMIT + ST_BUSY 步进重试（presarb 宽容面）：他客户端呈现
            // 事务在途（属主租约窗内）= 1B ST_BUSY 快答不转发，随行别名
            // 已被 wmd 关收（send 即 take——零泄漏面）；帧缓冲是本端私有
            // VMO，raster 内容不腐——重 dup 别名原样重发（buf_id 不进：
            // 服务端未见本帧，代际零迁移；damage 已随 render_frame 排空，
            // 弃帧跳拍 = 屏面永滞，故重试而非跳帧）。其余畸形 = fail loud。
            let mut waited = 0u64; // 步进累计（帽账）
            let token = loop {
                let m = match gp::encode_present_req(&mut req, &sreq) {
                    Some(m) => m,
                    None => fail("submit encode"),
                };
                let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
                match gp::parse_submit_ack(&rep[..n]) {
                    Some(t) => break t,
                    None if gp::parse_status_rep(&rep[..n]) == Some(gp::ST_BUSY) => {
                        if waited >= BUSY_RETRY_CAP_NS {
                            fail("submit busy cap");
                        }
                        busy_step(hw);
                        waited += VSYNC_NS;
                        if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias)
                            != 0
                            || alias == 0
                        {
                            fail("dup alias");
                        }
                    }
                    None => fail(&format!("submit ack bad st={}", rep[0])),
                }
            };
            ui.note_submitted();

            // FRAME_CALLBACK：blit-at-SUBMIT 形下帧已呈现——断
            // PRESENTED（DISCARDED = 呈现链破相 fail loud）。
            let m = match gp::encode_present_req(
                &mut req,
                &gp::PresentReq::FrameCallback { frame_token: token },
            ) {
                Some(m) => m,
                None => fail("frame callback encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            let done = match gp::parse_frame_done_rep(&rep[..n]) {
                Some(d) => d,
                None => fail(&format!("frame done bad st={}", rep[0])),
            };
            if done.status != gp::FRAME_PRESENTED {
                fail(&format!("frame status={}", done.status));
            }
            // seq = 本帧引擎序（render 后 telemetry 帧计数 -1 = last_seq）；
            // credits 以服务端应答为真值重同步（S2 食谱）。
            ui.note_presented(
                (ui.telemetry().frames() - 1) as u32,
                now(),
                done.status,
                done.credits,
            );

            // RELEASE（归还别名即关——在途 ≤1 结构性背压；RETURN-ALL
            // 语义：count≥1 断言 + 随行全量 close；presarb 宽容面：他
            // 客户端事务在途（本端事务被租约帽外抢占后）= ST_BUSY 步进
            // 重试）。
            let (rel, nh) = release_wait(hw, "release rep", &mut req, &mut rep, &mut rhs);
            // 宽容 RELEASE（97a0edeb4 appshell pump 同义复原——appdemo
            // 自管帧循环漏随刀面，presarb 仲裁下单发帧 park 永死实抓）：
            // count==0 = 他客户端跨代清账已排空本端在途别名（别名 = 本端
            // VMO 的 READ|TRANSFER dup，原柄在本表，对方 close = 纯引用
            // 计数下降）；本帧 PRESENTED 已先断言——早归还良性。随行
            // handle 不论归属全量 close；count≥1 时保持 nh==count 契约。
            if rel.count != 0 && nh != rel.count {
                fail(&format!("release count={} nh={}", rel.count, nh));
            }
            for h in &rhs[..nh] {
                close_h(*h);
            }

            presented += 1;
            buf_id = buf_id.wrapping_add(1);
            if buf_id == 0 {
                buf_id = 1;
            }
            if presented == 1 {
                // 首帧呈现回执过链（诊断面——console=true 串口可见，非
                // 标记形态）。
                println!("appdemo: first frame");
            }
        }

        // —— vote（S2 食谱：票面变化才发——省流；幂等面在服务端）。
        let ballot = ui.vote_hint();
        if last_vote != Some(ballot) {
            let vreq = gp::PresentReq::Vote {
                sticky: ballot.sticky,
                scene: ballot.scene,
                min_hz: ballot.min_hz,
                max_hz: ballot.max_hz,
                preferred_hz: ballot.preferred_hz,
            };
            let m = match gp::encode_present_req(&mut req, &vreq) {
                Some(m) => m,
                None => fail("vote encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            match gp::parse_status_rep(&rep[..n]) {
                Some(gp::ST_OK) => last_vote = Some(ballot),
                Some(st) => fail(&format!("vote st={st}")),
                None => fail(&format!("vote rep bad len={n}")),
            }
        }

        // —— 稳态等待（S1 判词：wait_many n≤2 [wm READABLE, input
        //    READABLE] + deadline = min(next_wakeup, refresh 驻留期限,
        //    now+IDLE 帽)；吸附动画在途由 next_wakeup 载荷）。
        let cap = now() + IDLE_CAP_NS;
        let mut deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
        if let Some(dl) = refresh_until {
            deadline = deadline.min(dl);
        }
        let mut hs = [hw, 0 as Handle];
        let ss = [SIGNAL_READABLE, SIGNAL_READABLE];
        let mut n = 1usize;
        if hi != 0 && input_pending {
            hs[1] = hi;
            n = 2;
        }
        let mut idx = 0usize;
        let mut obs = 0u64;
        let wst = berry_object_wait_many(hs.as_ptr(), ss.as_ptr(), n, deadline, &mut idx, &mut obs);
        if wst != 0 && wst != BERRY_ERR_TIMEDOUT {
            fail(&format!("wait err={wst}"));
        }
    }
}
