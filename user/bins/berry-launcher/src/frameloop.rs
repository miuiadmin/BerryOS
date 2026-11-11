//! frameloop —— 长活帧循环 embedder（GUI-8 段③ W2，docs/GUI-8实施设计
//! 草案.md §4 段③「长活帧循环 embedder」；蓝图 = 草案 + 侦察 S1 帧链
//! 拓扑 / S2 embedder 食谱）。
//!
//! 帧循环体：
//!
//! ```text
//! wake 起振 → loop {
//!   pump_input(上帧 expected) → Msg 分发（pager.on_msg→翻页 rehang /
//!     Click→槽命中映射→APP_SPAWN 门控桩）
//!   → pager.tick（snap 动画步进）→ render_frame（anim→relayout→paint
//!     →damage 出帧）
//!   → [drew] 光栅进帧 VMO → SUBMIT（FULL xor K；fresh READ dup 别名
//!     ——send 即 take；rate_hint/expected 直传）→ FRAME_CALLBACK（断
//!     PRESENTED）→ note_presented（credits 真值重同步）→ RELEASE（归还
//!     别名即关）→ vote（票面变化才发）
//!   → selftest 状态机（LAUNCH ok / PAGE ok n=2 标记——字节精确）
//!   → wait_many n=1 [wm READABLE] + deadline = min(next_wakeup, now+10s)
//! }
//! ```
//!
//! **稳态 = 非燃 CPU 阻塞形**（S1 判词）：Idle 相 next_wakeup=None →
//! deadline 落 now+10s IDLE 帽，单次 wait_many 睡满——10s 一轮回零渲染
//! 零 RPC（非 park 形：事件口（段④ input 订阅未来接此 wait）与定时面
//! 双活）。
//!
//! **背压 = 结构性**（S2 食谱）：Credits 无公开读面 → embedder 义务形
//! ——SUBMIT→FRAME_CALLBACK→RELEASE 严格内联串行，在途恒 ≤1 <
//! CREDITS_MAX=2；note_presented(credits_remaining) 以服务端应答为真值
//! 重同步引擎簿记。
//!
//! **帧 VMO 一次三连**（gfxdemo raw 形同源）：create→commit→map @
//! FRAME_VA——libc mmap 禁用：SUBMIT 随行须本 VMO 的 dup 别名，mmap
//! region 表藏 handle 取不出；每帧 SUBMIT fresh READ dup（A6-b send 即
//! take——原 vmo handle 恒留本表）。
//!
//! **栈面**（R5 仪器定谳，svcmgr launcher arm stack_pages=64 = 256KB——
//! 8 页下 `UiMain::new` 构造中间体 LTO 后仍部分落栈 ~29KB + pre-main std
//! 运行时，main+0x1c 序言即穿保护页 D-abort；gfxdemo 8 页先例无 berry-ui
//! 链不可平移，装配面注释为单源）：
//! CommandEncoder（DisplayList [Option<Cmd>; 256] ≈18KB）每帧栈构
//! （256KB 预算内）；glyph scratch（1024×12B）Box::leak 堆置；
//! Ui arena 堆置（main）。elf2flat 物化 .bss（单页墙判例）——零大 static
//! 缓冲。
//!
//! **自测**（裁决七：合成注入在 push_pointer = 逻辑真线）：首帧
//! FRAME_CALLBACK 过链 → `LAUNCH ok`；合成 swipe ×2（视口广义坐标，
//! 4 事件 1ms 间隔 → MOVE 滑窗 span=1ms < RESAMPLE_MIN_DELTA_MS=2ms
//! 恒 None 直通——原始坐标保真；位移 0.41·vp_w > 0.4·vp_w 翻页阈 +
//! 速度环远超 500px/s fling 阈，双判据同向）各落定 → `PAGE ok n=2`。
//!
//! **段④ 点击面（真实输入 → APP_SPAWN）**：`Msg::Click` 槽命中 →
//! `app_cli_h()!=0` 时真发 APP_SPAWN；**首例 ok 回执恰一次**打 `CLICK
//! ok n=1`——**仅当 `smoke` 为真**（main 经 GUI8.SMK 精确载荷判定）。
//! 失败回执维持 eprintln 诊断、零标记——spawn 是旁路动作，不打断帧循环。
//! **正常交互 boot（smoke=false）点击照常 spawn，但 CLICK 行与 click
//! ready 行都不打**（冻结相 app_cli_h()==0 更早退：零 RPC 零打印）。
//! `launcher: click ready` 非标记行 = 宿主注入的**状态谓词**门（规则/07
//! A32 禁睡等竞速）：`PAGE ok` 后 `snap_to(0)` 落位且其后已再呈一帧
//!（presented 增长——吸附完成帧已出，命中面回到 page 0 的 app 格）
//! 才打，且同样仅限 `smoke`；恰一次；`hi==0`（无 input gift）恒不打。
//!
//! 裁决四-c（输入拓扑）：真实输入订阅 = 段④（input extras 冻结相结构
//! 性缺席）；裁决四-e（归属）：状态栏/壁纸/桌面全在 launcher 进程内。

use berry_appproto as ap;
use berry_gfx::cmd::{CommandEncoder, GlyphPos, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::io::{app_cli_h, self_vmar_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_handle_duplicate, berry_object_wait_many,
    berry_object_wait_one, berry_vmar_map, berry_vmo_commit, berry_vmo_create, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, BERRY_ERR_TIMEDOUT, RIGHT_READ, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_ui::damage::DamageSet;
use berry_ui::frame::VoteBallot;
use berry_ui::hit::PtrEvent;
use berry_ui::{Msg, NodeId, UiMain};

use crate::desktop::{self, Desktop};
use crate::icons;
use crate::{close_h, enc_req, expect_ok, fail, now, rpc};

/// 帧缓冲映射 VA（gfxdemo 同位：mmap VA 窗 [0x5000_0000,0x6000_0000) 内
/// 固定位 +16MiB——与假想 first-fit 分配零交叠的防御位；页对齐）。
const FRAME_VA: usize = 0x5100_0000;

/// 默认 vsync 周期（60Hz；present GETINFO 无周期域——S2 食谱默认
/// 16_666_666 phase 0）。
const VSYNC_NS: u64 = 16_666_666;

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒，醒后
/// 跳过帧零提交再睡——非燃 CPU 阻塞形的定长节拍）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// glyph scratch 容量（堆置；桌面 ~30 短 label 富余——paint 对超容 run
/// 截断尾字形，视觉面非正确性面）。
const SCRATCH_GLYPHS: usize = 1024;

/// 自测 swipe 收敛帽（超时 = eprintln 诊断 + 放弃——PAGE 缺失由 smoke
/// 兜底 loud；不 fail 挂死：桌面常驻义务优先于自测件）。
const SETTLE_CAP_NS: u64 = 5 * 1_000_000_000;

/// 整帧清屏底色（gfxdemo 同值——FULL 帧先铺底再回放 DisplayList）。
const CLEAR_BG: (u8, u8, u8) = (0x10, 0x18, 0x28);

/// 自测状态机（boot 一次性走完 → Done 常驻）。
#[derive(Clone, Copy)]
enum Selftest {
    /// 首帧在途——presented>0 时注入第一轮 swipe（LAUNCH ok 已由提交块
    /// 打出——首帧 FRAME_CALLBACK 过链即打）。
    WaitFirst,
    /// 第一轮 swipe 收敛中（pc = 注入时已呈现数基线；page()==1 且 snap
    /// 停（tick false）且再有一帧呈现 = 落定）。
    Settle1 { pc: u64, deadline: u64 },
    /// 第二轮 swipe 收敛中（同判据，page()==2 → PAGE ok n=2）。
    Settle2 { pc: u64, deadline: u64 },
    /// 终态（标记已打或已诊断放弃）。
    Done,
}

/// `launcher: click ready` 标记的纯谓词（单元测试面）：**冒烟模式** +
/// input gift 在场 + 本进程未打过 + `snap_to(0)` 的呈现基线存在且其后
/// 已再呈帧（presented > pc）+ 命中面在 page 0。任一不满足 = 不打。
fn click_ready_gate(
    smoke: bool,
    has_input: bool,
    already: bool,
    snap_frame: Option<u64>,
    presented: u64,
    page: u8,
) -> bool {
    smoke
        && has_input
        && !already
        && page == 0
        && matches!(snap_frame, Some(pc) if presented > pc)
}

/// `CLICK ok n=1` 标记的纯谓词（单元测试面）：**冒烟模式** + 恰为第 1 个
/// 成功 APP_SPAWN 回执（count 0 = 尚无成功回执；count≥2 = 非首例）。
/// 调用方只在 `parse_spawn_ack == Some((true, _))` 分支递增——失败回执
/// 永不置 count，故永不产标记。
fn click_marker_gate(smoke: bool, spawn_ok: u32) -> bool {
    smoke && spawn_ok == 1
}

/// APP_SPAWN ack 记账（纯函数，测试面）：仅 `Some((true, _))` 递增计数；
/// 失败 ack（`Some((false, _))`）/ 坏 ack（`None`）不递增、不打标记。
/// 返回值 = 本次是否该打 `CLICK ok n=1`——冒烟模式且恰为第 1 个成功回执。
fn note_spawn_ack(smoke: bool, spawn_ok: &mut u32, ack: Option<(bool, u32)>) -> bool {
    match ack {
        Some((true, _)) => {
            *spawn_ok += 1;
            click_marker_gate(smoke, *spawn_ok)
        }
        _ => false,
    }
}

/// 帧循环入口（签名 = W2 冻结面——`hw` = wm 客户端端柄（wm/present 同
/// 端混流，ADR #61 中继），`ui` = 泄堆 arena，`desktop` = 静态面句柄，
/// `smoke` = main 单次读取的 GUI8.SMK 精确载荷判定——只门控段④两标记，
/// 不改点击/帧链行为）。
pub fn run(
    hw: Handle,
    hi: Handle,
    ui: &'static mut UiMain,
    mut desktop: Desktop,
    smoke: bool,
) -> ! {
    // —— present GETINFO（wmd 中继同端）：能力面轻断言（gfxdemo 同面）
    //    + 几何真值源（present 权威；与 wm 缓存一致性 = 内部不变量，
    //    失配 fail loud）。——
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo));
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

    // —— 跨代清账（对抗审查 R2 修）：前代实例若死于 SUBMIT ack→RELEASE
    //    close 窗，其 VMO 别名成孤儿永占 guifbd held 槽（RELEASE_WAIT 应答
    //    = held 全量排空，含他源/前代）；监督重启路纯 spawn 无 present 域
    //    清账——重启实例首帧 RELEASE 得 count=2 断言炸 → park 活死
    //    （TERMINATED 永不落，重启帽已耗）。boot 与重启两路都经本清账
    //    （wmd 启动序② relay::boot_release_all 同款）。
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
    let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    if gp::parse_release_rep(&rep[..n]).is_none() {
        fail(&format!("boot release bad st={}", rep[0]));
    }
    for h in &rhs[..nh] {
        close_h(*h);
    }

    // —— 帧缓冲 VMO 三连（gfxdemo raw 形同源）：stride = w*4 tight
    //    packing；len 页取整（vmar_map 页倍执法；尾页零无害）。
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
    if berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0
        || va != FRAME_VA
    {
        fail("vmar map");
    }
    let desc = match TextureDesc::new(w, h, stride, TextureFormat::Rgbx) {
        Ok(d) => d,
        Err(e) => fail(&format!("texture desc {e:?}")),
    };
    // SAFETY: [FRAME_VA, FRAME_VA+len) 刚由 vmar_map 落位且整段已 commit
    //（gfxdemo 同形）；run 永不返回——fb 终身持有。
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };

    // —— glyph scratch 堆置（DisplayList 'a 钉调用方 scratch + 'static
    //    图集/像素面——paint.rs 头注契约；堆置免栈穿）。
    let scratch: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());

    // —— vsync 协商 + 起振（Idle→Run + 条件 FULL——首帧整帧 damage）。
    ui.set_vsync(VSYNC_NS, 0);
    ui.wake(now());

    let mut last_expected: u64 = 0; // pump 重采样 target（上帧 expected 轨）
    let mut last_vote: Option<VoteBallot> = None; // 票面变化才发
    let mut presented: u64 = 0; // FRAME_CALLBACK 过链计数（自测基线）
    let mut launched = false; // LAUNCH ok 恰一次
    let mut spawn_ok: u32 = 0; // 真实 APP_SPAWN ok 回执计数（CLICK ok 恰一次）
    let mut buf_id: u32 = 1; // SUBMIT buffer 代际（RELEASE echo 非断言面）
    let mut st = Selftest::WaitFirst;
    // 段④：真实输入（inputd INPUT_POLL 阻塞批拉；hi==0 = 冻结相静默）。
    let mut hi = hi;
    let has_input = hi != 0; // 礼物在场（hi 后续可因输入面失效置 0——门仍真）
    // PAGE ok 后 snap_to(0) 的呈现基线：Some(pc) 且 presented>pc 才 click ready。
    let mut snap_frame: Option<u64> = None;
    let mut click_ready = false;
    let mut input_pending = false;
    let mut input_logged = false;
    let mut input_buf = [0u8; gp::INPUT_REP_MAX];
    let mut input_hs = [0 as Handle; 8];

    loop {
        let t_now = now();

        // —— 段④ 真实输入：先收上一轮 INPUT_POLL 应答，再挂下一轮 poll。
        //    非阻塞排空（-EAGAIN = 事件尚未到，poll 仍在服务端阻塞）；事件
        //    逐条 push 进引擎，与合成注入共用 push_pointer 口。
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
                        if !input_logged {
                            eprintln!("launcher: input events n={}", view.count());
                            input_logged = true;
                        }
                        for i in 0..view.count() {
                            if let Some(ev) = view.event(i) {
                                let pe = PtrEvent::from_wire(
                                    ev.phase,
                                    ev.pointer_id,
                                    ev.x,
                                    ev.y,
                                    ev.timestamp_ns,
                                    ev.buttons,
                                    ev.pressure,
                                );
                                if let Err(e) = ui.push_pointer(pe) {
                                    eprintln!("launcher: input push err={e:?}");
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {
                        if !input_logged {
                            eprintln!("launcher: input poll EAGAIN");
                            input_logged = true;
                        }
                    }
                    Some(Err(st)) => eprintln!("launcher: input poll st={st}"),
                    None => eprintln!("launcher: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("launcher: input recv err={s}");
                hi = 0;
                input_pending = false;
            }
        }
        if hi != 0 && !input_pending {
            if input_arm(hi) {
                input_pending = true;
                if !input_logged {
                    eprintln!("launcher: input poll armed h={hi}");
                    input_logged = true;
                }
            } else {
                eprintln!("launcher: input poll arm failed -- disable");
                hi = 0;
            }
        }

        // —— 输入泵 + Msg 分发（合成注入与真实输入同口）——
        let msgs = ui.pump_input(last_expected);
        for m in msgs.iter() {
            match m {
                Msg::Click { node } => on_click(*node, &desktop, &mut spawn_ok, smoke),
                _ => {
                    // pager.on_msg true = 翻页目标变更 → a11y 重挂当前页
                    //（幂等；Err = 容量违例 fail 收口）。
                    if desktop.pager.on_msg(ui, m) {
                        let page = desktop.pager.page();
                        if let Err(e) = desktop::rehang_a11y(ui, &mut desktop, page) {
                            fail(&format!("rehang a11y err={e:?}"));
                        }
                    }
                }
            }
        }
        // snap 动画步进（每迭代必调——snap 驱动面；返回值仅为「有吸附
        // 在推进」观察位，自测判据已不依赖——见 Settle1 注）。
        desktop.pager.tick(ui, t_now);

        // —— 渲染（跳过帧不提交——damage 空集零 SUBMIT 零 RPC）——
        let mut enc = CommandEncoder::new();
        let out = ui.render_frame(t_now, scratch, &mut enc);
        last_expected = out.expected_present_ns;
        if out.drew {
            let dmg = out.damage;
            // K>0 xor FULL 立法兜底：count==0 非空集视同整帧。
            let full = dmg.full || dmg.count == 0;
            let pass = if full {
                PassDescriptor::clear(CLEAR_BG.0, CLEAR_BG.1, CLEAR_BG.2)
            } else {
                // 增量帧：Load+Store + K 矩形并集包络（写侧约束——soft
                // 光栅只在 dirty 域动笔，域外像素保持上帧）。
                PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(damage_envelope(&dmg)))
            };
            let dl = enc.finish();
            if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
                fail(&format!("raster submit {e:?}"));
            }

            // SUBMIT：fresh READ dup 别名随行（send 即 take——原 vmo 留表）。
            let mut alias: Handle = 0;
            if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
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
            let m = enc_req(gp::encode_present_req(&mut req, &sreq));
            let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
            let token = match gp::parse_submit_ack(&rep[..n]) {
                Some(t) => t,
                None => fail(&format!("submit ack bad st={}", rep[0])),
            };
            ui.note_submitted();

            // FRAME_CALLBACK：blit-at-SUBMIT 形下帧已呈现（token 校验即
            // 答）——断 PRESENTED（DISCARDED = 呈现链破相 fail loud）。
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
            // seq = 本帧引擎序（render 后 telemetry 帧计数 -1 = last_seq）；
            // credits 以服务端应答为真值重同步（S2 食谱）。
            ui.note_presented(
                (ui.telemetry().frames() - 1) as u32,
                now(),
                done.status,
                done.credits,
            );

            // RELEASE（归还别名即关——在途 ≤1 结构性背压，见头注）。
            // RETURN-ALL 语义（对抗审查 R2 修）：应答 = held 全量排空——
            // 稳态恒 count==1（单 present 客户端 + 在途≤1），但可含前代
            // 孤儿别名（boot 清账同刀兜底）；count≥1 时随行 handle 全量
            // close 归还。
            let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
            let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            let rel = match gp::parse_release_rep(&rep[..n]) {
                Some(r) => r,
                None => fail(&format!("release rep bad st={}", rep[0])),
            };
            if rel.count < 1 || nh != rel.count {
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
            if !launched {
                // 标记（字节精确）：首帧呈现回执过链恰一次。
                println!("LAUNCH ok");
                launched = true;
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
            let m = enc_req(gp::encode_present_req(&mut req, &vreq));
            expect_ok(hw, &req[..m], &[], &mut rep, &mut rhs, "vote");
            last_vote = Some(ballot);
        }

        // —— 自测状态机（boot 一次性；标记字节精确）——
        match st {
            Selftest::WaitFirst => {
                if presented > 0 {
                    inject_swipe(ui, t_now);
                    st = Selftest::Settle1 { pc: presented, deadline: t_now + SETTLE_CAP_NS };
                }
            }
            Selftest::Settle1 { pc, deadline } => {
                // 落定判据 = 裁定 + 帧流：page() 即时反映翻页裁定
                //（DragEnd 当帧）；presented>pc = 注入后已有帧呈现——
                // tick/动画活性由同循环帧流背书。终验竞态定谳：曾以
                // `!pager_anim`（吸附动画完成帧）为判据——完成帧本身
                // tick=true，TCG 争用宿主下每帧 render+blit+3 RPC 可
                // 100-400ms，240ms 动画全程竞速 5s 帽：首个过帽迭代恰
                // 逢完成帧 → timeout 吞掉本已成立的落定（rig 实证：超时
                // 时 page=1、桌面存活续渲）。
                if desktop.pager.page() == 1 && presented > pc {
                    inject_swipe(ui, t_now);
                    st = Selftest::Settle2 { pc: presented, deadline: t_now + SETTLE_CAP_NS };
                } else if t_now > deadline {
                    eprintln!(
                        "launcher: selftest page1 settle timeout (page={})",
                        desktop.pager.page()
                    );
                    st = Selftest::Done;
                }
            }
            Selftest::Settle2 { pc, deadline } => {
                // 同 Settle1 判据（裁定 + 帧流）——竞态面同理。
                if desktop.pager.page() == 2 && presented > pc {
                    // 标记（字节精确）：两轮翻页各落一帧呈现。
                    println!("PAGE ok n=2");
                    // 段④ 收尾：自测只验状态机，落回 page 0（默认布局的
                    // app 全在第 0 页；停在 page 2 = 用户看到空桌面）。
                    desktop.pager.snap_to(ui, 0);
                    if let Err(e) = desktop::rehang_a11y(ui, &mut desktop, 0) {
                        fail(&format!("rehang a11y reset err={e:?}"));
                    }
                    // 命中面回 page 0 的呈现基线（click ready 状态谓词用）。
                    snap_frame = Some(presented);
                    st = Selftest::Done;
                } else if t_now > deadline {
                    eprintln!(
                        "launcher: selftest page2 settle timeout (page={})",
                        desktop.pager.page()
                    );
                    st = Selftest::Done;
                }
            }
            Selftest::Done => {}
        }

        // —— 段④ click ready 非标记门（规则/07 A32：状态谓词，禁睡等竞速）
        //    `snap_to(0)` 落位后须已再呈一帧（presented 增长 = 吸附完成帧
        //    出，命中面稳落 page 0）才宣布可注入；`has_input` 缺席或非冒烟
        //    模式恒不打（冻结相/正常 boot 字节零扰动）。恰一次。
        if click_ready_gate(
            smoke,
            has_input,
            click_ready,
            snap_frame,
            presented,
            desktop.pager.page(),
        ) {
            println!("launcher: click ready");
            click_ready = true;
        }

        // —— 稳态等待（S1 判词：wait_many n=1 [wm READABLE] + deadline =
        //    min(next_wakeup, now+IDLE 帽)；Idle 相 = 睡满 10s 零渲染零
        //    RPC 非燃 CPU；wm READABLE = 段④ 输入事件未来口。TIMEDOUT =
        //    期限到正常醒——渲染下一拍 / Idle 空转睡回。）
        let nw = now();
        let cap = nw + IDLE_CAP_NS;
        let deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
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

/// K 矩形并集包络（增量 pass 的 dirty 域）。engine damage 恒 viewport
/// 裁剪——u32 界内值；i32 折 try_from 饱和防御（gfx Rect 值域）。
fn damage_envelope(dmg: &DamageSet) -> GfxRect {
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

/// 挂一轮 INPUT_POLL（段④）：inputd 阻塞批拉 ≤3 事件；-EBUSY = 通道满
///（前一轮未收），等 WRITABLE 后重试；其余错 = 输入面失效，返回 false。
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
            eprintln!("launcher: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("launcher: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}

/// 合成 swipe 注入（裁决七：push_pointer 逻辑真线——视口广义坐标）。
///
/// 序列（4 事件形）：DOWN(x0) → MOVE(x0−step) → MOVE(xe) → UP(xe)，
/// 1ms 间隔（dt 单位终验定谳：曾裸加 ns——注释宣称的 ms 语义从未成立，
/// 窗 span=1ns 恒拒直通、行为侥幸同构；现如实 ×1e6）。resample 拒面 =
/// MOVE 滑窗（DOWN 清窗）span 严格 < `RESAMPLE_MIN_DELTA_MS`=2ms——4 事件
/// 每窗至多 2 样本 span=1ms 恒 None 直通（原始坐标保真）。
///
/// 判据双同向（ts 基 = 注入 base）：速度环末两样本 dx=xe−(x0−step)=
/// −0.41·vp_w、dt=1ms → vx 钳 −32768 远超 500px/s fling 阈（速度向负 =
/// next_page）；位移 drag_last_x−drag_x0 = −0.41·vp_w（x0 基准 = slop
/// 触发样本 x0−step）> 0.4·vp_w 翻页阈——800 宽下 328 > 320 余 8px。
/// 命中面：(vp_w/2, vp_h/2) 恒落 Grid 容器（非 Root 可命中——格间隙落
/// Grid 本身）。4 事件 < INPUT_BUF_MAX=8——单泵全序消费（速度环在 UP
/// 分派前完备）。
fn inject_swipe(ui: &mut UiMain, base: u64) {
    let (vp_w, vp_h) = ui.viewport();
    let x0 = vp_w / 2;
    let y = vp_h / 2;
    let step = (vp_w / 20).max(9); // slop 严格 >8px——极窄屏兜底
    let xf = (vp_w / 25).max(1); // 终点 x（左翻——下一页方向）
    let m1 = x0.saturating_sub(step); // 首 MOVE（= slop 触发样本 = 位移基准）
    let xe = xf.min(m1); // 终点 ≤ 首 MOVE（左向单调防御）
    let ev = |phase: u8, x: u32, dt: u64| PtrEvent {
        phase,
        pointer_id: 1,
        x,
        y,
        ts_ns: base + dt * 1_000_000, // dt = 毫秒（终验单位定谳——见头注）
        buttons: gp::BUTTON_L,
        pressure_mi: 500,
    };
    let seq: [(u8, u32, u64); 4] = [
        (gp::PHASE_DOWN, x0, 0),
        (gp::PHASE_MOVE, m1, 1),
        (gp::PHASE_MOVE, xe, 2),
        (gp::PHASE_UP, xe, 3),
    ];
    for (phase, x, dt) in seq {
        if let Err(e) = ui.push_pointer(ev(phase, x, dt)) {
            fail(&format!("selftest inject err={e:?}"));
        }
    }
    // 注入即 wake（首验定谳：snap 落定帧引擎已回 Idle——next_wakeup=None
    // 着 10s 帽睡，队列事件干等醒；swipe2 醒时已超 SETTLE_CAP →
    // settle timeout(page 却已到 2)。wake = Idle→Run 起振 + damage 空集
    // 置 FULL 保产出——与 boot 首轮起振同一正统路径；snap 收敛后引擎
    // 自然回 Idle，稳态不燃 CPU 不变）。
    ui.wake(base);
}

/// CLICK 面（修约 1 / 段④）：槽命中映射（cell/label/icon 三节点皆归本
/// 格——最深叶命中面）→ 注册表条目 → `app_cli_h()==0`（冻结相 extras 恰
/// [wm]）静默跳过零 RPC 零打印；在场 = APP_SPAWN 真跑（**正常 boot 与
/// 冒烟 boot 同路**）。**首例 ok 回执**仅在 `smoke` 为真时打 `CLICK ok
/// n=1`（恰一次，`spawn_ok` 计数门——后续成功点击静默，避免重启/连点
/// 制造重复标记）；失败回执维持 eprintln 诊断（spawn 是旁路动作，不打
/// 断帧循环、不 fail、**不产标记**）。
fn on_click(node: NodeId, desktop: &Desktop, spawn_ok: &mut u32, smoke: bool) {
    let slot = desktop
        .slots
        .iter()
        .flatten()
        .chain(desktop.dock.iter())
        .find(|s| s.cell == node || s.label_node == node || s.icon == Some(node));
    let Some(slot) = slot else { return }; // 壁纸/状态栏/圆点/容器白区 = 无格
    let Some(idx) = slot.app else { return }; // 占位/幽灵格不可 spawn
    let Some(entry) = icons::registry().get(idx) else { return };
    let h = app_cli_h();
    if h == 0 {
        return; // 冻结相：app 通道结构性缺席
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    match ap::encode_spawn_request(&mut req, entry.id_bytes()) {
        Some(m) => {
            let (n, _) = rpc(h, &req[..m], &[], &mut rep, &mut rhs);
            let ack = ap::parse_spawn_ack(&rep, n);
            if note_spawn_ack(smoke, spawn_ok, ack) {
                // 标记（字节精确）：真实 APP_SPAWN ok 回执恰一次，且仅
                // 冒烟模式（正常 boot 零 CLICK）。
                println!("CLICK ok n=1");
            }
            match ack {
                Some((true, _)) => {}
                Some((false, err)) => eprintln!(
                    "launcher: spawn {} err={err}",
                    String::from_utf8_lossy(entry.id_bytes())
                ),
                None => eprintln!("launcher: spawn ack bad"),
            }
        }
        None => eprintln!("launcher: spawn id bad"),
    }
}

// ---- 标记门纯谓词测试（同 layout_io：aarch64-musl 目标可编译，宿主
// ---- 无 qemu-user 不可执行；用例零 RPC/零 fd，只验门控真值表）----
#[cfg(test)]
mod tests {
    use super::*;

    /// click ready 必须同满足冒烟 + input + 未打 + 吸附后已再呈帧 + page 0；
    /// 任一不满足即 false（正常 boot 的 smoke=false 是首要负例）。
    #[test]
    fn click_ready_gate_truth_table() {
        // 正例：冒烟、input 在场、未打、基线 pc=7、presented=8、page 0。
        assert!(click_ready_gate(true, true, false, Some(7), 8, 0));
        // smoke=false（正常交互 boot）：其余全真也不打。
        assert!(!click_ready_gate(false, true, false, Some(7), 8, 0));
        // 无 input gift（冻结相）。
        assert!(!click_ready_gate(true, false, false, Some(7), 8, 0));
        // 已打过（恰一次）。
        assert!(!click_ready_gate(true, true, true, Some(7), 8, 0));
        // 吸附基线未建立。
        assert!(!click_ready_gate(true, true, false, None, 8, 0));
        // 基线后尚未再呈帧（presented == pc）。
        assert!(!click_ready_gate(true, true, false, Some(8), 8, 0));
        // 命中面不在 page 0。
        assert!(!click_ready_gate(true, true, false, Some(7), 8, 1));
    }

    /// CLICK 标记只在冒烟模式且恰为第 1 个成功 ack 时打；0（失败回执未
    /// 递增）与 ≥2（非首例）皆 false。
    #[test]
    fn click_marker_gate_truth_table() {
        assert!(click_marker_gate(true, 1));
        assert!(!click_marker_gate(false, 1)); // 正常 boot 零 CLICK
        assert!(!click_marker_gate(true, 0)); // 失败 ack 不产标记
        assert!(!click_marker_gate(true, 2)); // 后续成功静默
        assert!(!click_marker_gate(false, 0));
    }

    /// ack 记账：失败/坏 ack 不递增即不产标记；首个成功 ack 恰打一次；
    /// 后续成功静默；正常 boot（smoke=false）即使成功也零标记。
    #[test]
    fn spawn_ack_marker_only_after_first_success() {
        let mut n = 0u32;
        assert!(!note_spawn_ack(true, &mut n, Some((false, 7))));
        assert_eq!(n, 0);
        assert!(!note_spawn_ack(true, &mut n, None));
        assert_eq!(n, 0);
        assert!(note_spawn_ack(true, &mut n, Some((true, 0))));
        assert_eq!(n, 1);
        assert!(!note_spawn_ack(true, &mut n, Some((true, 0))));
        assert_eq!(n, 2);
        // 正常 boot：成功 spawn 仍记账，但永不产 CLICK。
        let mut quiet = 0u32;
        assert!(!note_spawn_ack(false, &mut quiet, Some((true, 0))));
        assert_eq!(quiet, 1);
        assert!(!note_spawn_ack(false, &mut quiet, Some((true, 0))));
        assert_eq!(quiet, 2);
    }
}
