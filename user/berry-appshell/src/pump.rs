//! pump —— 帧循环骨架 embedder（波3-件⑦ 抽取自 berry-launcher
//! frameloop.rs 的通用半程；行为零变化——应用特化经 [`HostHooks`]
//! 回调注入，操作序逐字保真）。
//!
//! 帧循环体（launcher frameloop.rs 原序承袭）：
//!
//! ```text
//! present GETINFO 断言 → 跨代清账（RELEASE_WAIT）→ 帧缓冲 VMO 三连
//! （create/commit/map）→ glyph scratch 堆置 → set_vsync + wake 起振
//! → loop {
//!   hooks.on_head（时钟续摆/armed 复位等应用特化）
//!   → input 泵（上轮 INPUT_POLL 应答收 + push_pointer/键盘帧 on_key
//!     分流；再挂下一轮）
//!   → ui.pump_input → hooks.on_msgs（Click/翻页分发）
//!   → hooks.on_tick（snap 动画步进）
//!   → render_frame（跳过帧不提交——damage 空集零 SUBMIT 零 RPC）
//!   → [drew] 光栅进帧 VMO → SUBMIT（FULL xor K；fresh READ dup 别名
//!     ——send 即 take）→ FRAME_CALLBACK（断 PRESENTED）→
//!     note_presented（credits 真值重同步）→ RELEASE（归还别名即关）
//!     → hooks.on_presented（首帧标记位——LAUNCH ok 类面归应用）
//!   → vote（票面变化才发）
//!   → hooks.on_frame（自测状态机/click-ready 谓词等）
//!   → wait_many [wm READABLE (+input READABLE)] + deadline =
//!     min(next_wakeup, now+IDLE 帽, hooks.extra_deadline)
//! }
//! ```
//!
//! **稳态 = 非燃 CPU 阻塞形**：Idle 相 next_wakeup=None → deadline 落
//! now+10s IDLE 帽，单次 wait_many 睡满——10s 一轮回零渲染零 RPC。
//!
//! **背压 = 结构性**：Credits 无公开读面 → embedder 义务形——
//! SUBMIT→FRAME_CALLBACK→RELEASE 严格内联串行，在途恒 ≤1 <
//! CREDITS_MAX=2；note_presented(credits_remaining) 以服务端应答为真值
//! 重同步引擎簿记。
//!
//! **帧 VMO 一次三连**（gfxdemo raw 形同源）：create→commit→map @
//! [`FRAME_VA`]——libc mmap 禁用：SUBMIT 随行须本 VMO 的 dup 别名；
//! 每帧 SUBMIT fresh READ dup（send 即 take——原 vmo handle 恒留本表）。
//!
//! host 测试面：damage 包络/整帧判据/等待帽推算三纯函数——帧循环本体
//! `cfg(not(test))`（berry-sys 符号宿主不存在，berry-fsclient 先例）。

#[cfg(not(test))]
use alloc::format;
#[cfg(not(test))]
use alloc::vec;
use berry_gfx::cmd::Rect as GfxRect;
#[cfg(not(test))]
use berry_gfx::cmd::{CommandEncoder, GlyphPos};
#[cfg(not(test))]
use berry_gfx::color::{TextureDesc, TextureFormat};
#[cfg(not(test))]
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
#[cfg(not(test))]
use berry_gfx::queue::Queue;
// 无条件：HostHooks::on_panel_caps 签名引用 gp::PanelCapsRep（纯 wire
// 类型——host 测试面可见；guiproto 本 host 安全零 syscall 面）。
use berry_guiproto as gp;
use berry_sys::Handle;
#[cfg(not(test))]
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_handle_duplicate, berry_object_wait_many,
    berry_object_wait_one, berry_thread_sleep, berry_vmar_map, berry_vmo_commit, berry_vmo_create,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, BERRY_ERR_TIMEDOUT, RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_ui::damage::DamageSet;
use berry_ui::desc::MsgBuf;
#[cfg(not(test))]
use berry_ui::frame::VoteBallot;
#[cfg(not(test))]
use berry_ui::hit::PtrEvent;
use berry_ui::UiMain;

#[cfg(not(test))]
use crate::host::diag;
#[cfg(not(test))]
use crate::rpc::{close_h, enc_req, expect_ok, fail, now, rpc};

/// 帧缓冲映射 VA（gfxdemo 同位：mmap VA 窗 [0x5000_0000,0x6000_0000) 内
/// 固定位 +16MiB——与假想 first-fit 分配零交叠的防御位；页对齐）。
pub const FRAME_VA: usize = 0x5100_0000;

/// IDLE 稳态等待帽（Idle 相撤定时器 → 10s 周期兜底醒，醒后跳过帧零提交
/// 再睡——非燃 CPU 阻塞形的定长节拍）。
pub const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// RELEASE_WAIT 1B 拒答（ST_BUSY 等）重试总帽：他客户端 parked RELEASE
/// 占用 guifbd `rel_park` 单槽时本端请求被拒——对方帧 latch 即让槽
///（vgpu reap 帽 10s 同值对齐）；到帽才 fail（真死链面）。
pub const RELEASE_RETRY_CAP_NS: u64 = 10 * 1_000_000_000;

/// RELEASE_WAIT 重试步进（100ms——步进睡让出 CPU，醒即重验）。
pub const RELEASE_RETRY_STEP_NS: u64 = 100 * 1_000_000;

/// glyph scratch 容量（堆置；~30 短 label 富余——paint 对超容 run 截断
/// 尾字形，视觉面非正确性面）。
pub const SCRATCH_GLYPHS: usize = 1024;

/// K 矩形并集包络（增量 pass 的 dirty 域；纯函数，host 测试面）。
/// engine damage 恒 viewport 裁剪——u32 界内值；i32 折 try_from 饱和
/// 防御（gfx Rect 值域）。
pub fn damage_envelope(dmg: &DamageSet) -> GfxRect {
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

/// 整帧判据（纯函数，host 测试面）：FULL 旗或 count==0 非空集视同整帧
///（K>0 xor FULL 立法兜底——服务端「无 damage 不提交」双拒面）。
pub fn damage_is_full(dmg: &DamageSet) -> bool {
    dmg.full || dmg.count == 0
}

/// 稳态等待帽推算（纯函数，host 测试面）：min(引擎 next_wakeup, IDLE
/// 帽, 应用额外帽（armed 窗类）)——三者取最早，None 让位。
pub fn wait_deadline(next_wakeup: Option<u64>, t_now: u64, extra: Option<u64>) -> u64 {
    let cap = t_now + IDLE_CAP_NS;
    let mut d = next_wakeup.map_or(cap, |t| t.min(cap));
    if let Some(e) = extra {
        d = d.min(e);
    }
    d
}

/// 应用特化回调面（帧循环骨架的消费侧注入点——launcher 形态：
/// on_head = 关机 armed 复位 + 时钟续摆；on_msgs = 五键导航/tab/点击
/// 分发；on_tick = SwipePager 步进；on_presented = 首帧标记；on_frame
/// = 自测状态机 + click-ready 谓词；extra_deadline = armed 窗）。
pub trait HostHooks {
    /// 循环头（每迭代首拍——时钟续摆/armed 过期复位类「读钟改树」面）。
    fn on_head(&mut self, ui: &mut UiMain, t_now: u64);

    /// 引擎 Msg 分发（pump_input 产物逐条——Click/翻页/手势裁定）。
    fn on_msgs(&mut self, ui: &mut UiMain, msgs: &MsgBuf, t_now: u64);

    /// 帧前步进（snap 动画 tick 类）。
    fn on_tick(&mut self, ui: &mut UiMain, t_now: u64);

    /// 帧呈现回执（`presented` = 本 pump 累计呈现数——首帧 == 1；标记
    /// 面归应用：LAUNCH ok 类恰一次语义在回调内自管）。
    fn on_presented(&mut self, ui: &mut UiMain, presented: u64);

    /// 帧尾回调（vote 后、wait 前——自测状态机/click-ready 谓词面）。
    fn on_frame(&mut self, ui: &mut UiMain, presented: u64, t_now: u64);

    /// 额外等待帽（Some(dl) = 准时醒面——关机 armed 窗过期复位类）。
    fn extra_deadline(&mut self) -> Option<u64>;

    /// hdi PANEL_CAPS 应答回调（refresh 刀1 率通路 §4-5——帧钟参数服务端
    /// 下发，服务端是率域唯一权威 R1 单源）。默认实现 = 有应答即注入
    /// `set_vsync`；None = 旧服务端无此面 → 不注入，留 berry-ui 回退缺省
    ///（[`UiMain`]( berry_ui::UiMain) 构造默认周期，lib.rs
    /// DEFAULT_VSYNC_PERIOD_NS 注释钉明）。证据行（`PANEL ok`）类应用面
    /// 策略归覆写侧。
    fn on_panel_caps(&mut self, ui: &mut UiMain, caps: Option<&gp::PanelCapsRep>) {
        if let Some(c) = caps {
            ui.set_vsync(c.current_period_ns as u64, c.current_phase_ns as u64);
        }
    }

    /// 键事件回调（inputd 键盘帧出向覆载面——INPUT_POLL 批内 PointerEvent
    /// 帧 device_id = DEV_KEYBOARD 分流至此：`code` = 翻译后出向字节、
    /// `ts_ns` = CNTVCT 时间戳，口径见 guiproto DEV_KEYBOARD 分注）。v1
    /// 单订阅者模型：键随 INPUT_POLL 到唯一轮询壳应用（本 pump 即唯一
    /// 消费者）；inputd 不查 wmd 焦点表——焦点门控分发是波 3 后续挂账，
    /// 非本批面。默认空实现：launcher 形态不收键零改零警；GUI 终端类应用
    /// 覆写收键。
    fn on_key(&mut self, _ui: &mut UiMain, _code: u8, _ts_ns: u64) {}

    /// 渲染让位策略位（默认 false = 不让位）：true = 本拍跳过 render/
    /// SUBMIT 半程（damage 留脏不消费，hold 解除拍自然重渲）。用途 =
    /// present 域**单飞让位**：guifbd 的 pending/cb_park/rel_park/held
    /// 全是单槽/单表（单 present 客户端时代几何），他客户端（如刚拉起
    /// 的 app）呈现窗内本端让道，防跨客户端踩踏（squash/DISCARDED/
    /// held 全量清账互吞）。hold 截止须并入 [`HostHooks::extra_deadline`]
    /// 保证到点醒（pump 等待面不感知 hold 本身）。
    fn render_hold(&mut self) -> bool {
        false
    }
}

/// pump 装配参数。
pub struct PumpCfg {
    /// wm/present 客户端端柄（同端混流，ADR #61 中继）。
    pub hw: Handle,
    /// input 客户端端柄（0 = 无 input gift——输入面静默）。
    pub hi: Handle,
    /// self VMAR gift（帧 VMO 映射；0 = fail loud "no self vmar gift"）。
    pub vmar: Handle,
    /// SUBMIT surface_id（应用窗 id 或系统面豁免标记——guifbd 波3-件②
    /// 带裁剪分档位）。
    pub surface_id: u32,
    /// 整帧清屏底色（FULL 帧先铺底再回放 DisplayList）。
    pub clear_bg: (u8, u8, u8),
}

/// 帧循环入口（永不返回）。启动半程 = present GETINFO 能力断言 + 跨代
/// 清账（RELEASE_WAIT）+ 帧缓冲 VMO 三连 + scratch 堆置 + vsync 起振；
/// 随后进 [`HostHooks`] 驱动的常驻循环。
#[cfg(not(test))]
pub fn run(cfg: PumpCfg, ui: &'static mut UiMain, hooks: &mut dyn HostHooks) -> ! {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];

    // —— present GETINFO（wmd 中继同端）：能力面轻断言（gfxdemo 同面）
    //    + 几何真值源（present 权威；与 wm 缓存一致性 = 内部不变量，
    //    失配 fail loud）。——
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo));
    let (n, _) = rpc(cfg.hw, &req[..m], &[], &mut rep, &mut rhs);
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

    // —— hdi PANEL_CAPS（refresh 刀1 率通路 §4-5）：帧钟参数服务端下发
    //    注入——服务端是率域唯一权威（R1 单源），客户端禁本地默认顶替
    //    真值。version_hdi < 2 = 旧 guifbd 无此面 → None 形回调（留
    //    berry-ui 回退缺省）；有面而应答畸形 = 协议违例 fail loud
    //    （GETINFO 同款）。hdi 域编码器 usize 直返（present 域 Option 形
    //    不同）——不过 enc_req。
    let mut caps: Option<gp::PanelCapsRep> = None;
    if pinfo.version_hdi >= 2 {
        let m = gp::encode_panel_caps_req(&mut req);
        let (n, _) = rpc(cfg.hw, &req[..m], &[], &mut rep, &mut rhs);
        match gp::parse_panel_caps_rep(&rep[..n]) {
            Some(c) if c.current_hz > 0 && c.current_period_ns > 0 => caps = Some(c),
            _ => fail(&format!("panel caps bad st={}", rep[0])),
        }
    }
    hooks.on_panel_caps(ui, caps.as_ref());

    // —— 跨代清账（对抗审查 R2 修）：前代实例若死于 SUBMIT ack→RELEASE
    //    close 窗，其 VMO 别名成孤儿永占 guifbd held 槽（RELEASE_WAIT 应答
    //    = held 全量排空，含他源/前代）；监督重启路纯 spawn 无 present 域
    //    清账——重启实例首帧 RELEASE 得 count=2 断言炸 → park 活死
    //    （TERMINATED 永不落，重启帽已耗）。boot 与重启两路都经本清账
    //    （wmd 启动序② relay::boot_release_all 同款）。宽容收口见
    //    [`release_round`]（1B 拒答重试 + count==0 合法）。
    let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
    release_round(cfg.hw, &req[..m], &mut rep, &mut rhs);

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
    if cfg.vmar == 0 {
        fail("no self vmar gift");
    }
    let mut va = FRAME_VA;
    if berry_vmar_map(cfg.vmar, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0
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
        alloc::boxed::Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());

    // —— vsync 起振（Idle→Run + 条件 FULL——首帧整帧 damage）。周期/相位
    //    已由上方 PANEL_CAPS 下发注入（on_panel_caps 默认实现）；旧服务端
    //    None 形 = berry-ui 回退缺省（本地 VSYNC_NS 常量废除——全仓周期
    //    单源在 guifbd 档位表，规范 §3.4）。
    ui.wake(now());

    let mut last_expected: u64 = 0; // pump 重采样 target（上帧 expected 轨）
    let mut last_vote: Option<VoteBallot> = None; // 票面变化才发
    let mut presented: u64 = 0; // FRAME_CALLBACK 过链计数（应用基线）
    let mut buf_id: u32 = 1; // SUBMIT buffer 代际（RELEASE echo 非断言面）
    // 段④形态：真实输入（inputd INPUT_POLL 阻塞批拉；hi==0 = 静默）。
    let mut hi = cfg.hi;
    let mut input_pending = false;
    let mut input_logged = false;
    let mut input_buf = [0u8; gp::INPUT_REP_MAX];
    let mut input_hs = [0 as Handle; 8];

    loop {
        let t_now = now();

        hooks.on_head(ui, t_now);

        // —— 真实输入：先收上一轮 INPUT_POLL 应答，再挂下一轮 poll。
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
                            diag(&format!("input events n={}", view.count()));
                            input_logged = true;
                        }
                        for i in 0..view.count() {
                            if let Some(ev) = view.event(i) {
                                // —— 键盘帧覆载分流（guiproto DEV_KEYBOARD
                                //    出向分注）：device_id = 键盘 = 键事件
                                //    （x = 翻译后字节），交 hooks.on_key 跳过
                                //    指针路径（否则读成 (x=keycode, y=0)
                                //    幻影按下）。
                                if ev.device_id == gp::DEV_KEYBOARD {
                                    hooks.on_key(ui, ev.x as u8, ev.timestamp_ns);
                                    continue;
                                }
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
                                    diag(&format!("input push err={e:?}"));
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {
                        if !input_logged {
                            diag("input poll EAGAIN");
                            input_logged = true;
                        }
                    }
                    Some(Err(st)) => diag(&format!("input poll st={st}")),
                    None => diag(&format!("input poll rep bad len={nb}")),
                }
            } else if s != BERRY_ERR_AGAIN {
                diag(&format!("input recv err={s}"));
                hi = 0;
                input_pending = false;
            }
        }
        if hi != 0 && !input_pending {
            if input_arm(hi) {
                input_pending = true;
                if !input_logged {
                    diag(&format!("input poll armed h={hi}"));
                    input_logged = true;
                }
            } else {
                diag("input poll arm failed -- disable");
                hi = 0;
            }
        }

        // —— 输入泵 + Msg 分发（合成注入与真实输入同口）——
        let msgs = ui.pump_input(last_expected);
        hooks.on_msgs(ui, &msgs, t_now);
        hooks.on_tick(ui, t_now);

        // —— 渲染（跳过帧不提交——damage 空集零 SUBMIT 零 RPC）——
        // render_hold（应用特化策略位）：true = 本拍跳过整个渲染半程
        //（damage 留脏不消费，hold 解除拍重渲——present 域单飞让位，
        // 见 HostHooks::render_hold 注）。
        if !hooks.render_hold() {
            let mut enc = CommandEncoder::new();
            let out = ui.render_frame(t_now, scratch, &mut enc);
            last_expected = out.expected_present_ns;
            if out.drew {
            let dmg = out.damage;
            // K>0 xor FULL 立法兜底：count==0 非空集视同整帧。
            let full = damage_is_full(&dmg);
            let pass = if full {
                PassDescriptor::clear(cfg.clear_bg.0, cfg.clear_bg.1, cfg.clear_bg.2)
            } else {
                // 增量帧：Load+Store + K 矩形并集包络（写侧约束——soft
                // 光栅只在 dirty 域动笔，域外像素保持上帧）。
                PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(damage_envelope(&dmg)))
            };
            let dl = enc.finish();
            if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
                fail(&format!("raster submit {e:?}"));
            }

            // SUBMIT：fresh READ dup 别名随行（send 即 take——原 vmo 留表；
            // TRANSFER 保位——随行预检执法，dup 只紧不松须显式保位）。
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
                buffer_id: buf_id,
                surface_id: cfg.surface_id,
                flags,
                rate_hint: out.rate_hint_hz,
                target_present_ns: out.expected_present_ns,
                rects,
                rect_count: k,
            };
            let m = enc_req(gp::encode_present_req(&mut req, &sreq));
            let (n, _) = rpc(cfg.hw, &req[..m], &[alias], &mut rep, &mut rhs);
            let token = match gp::parse_submit_ack(&rep[..n]) {
                Some(t) => t,
                None => fail(&format!("submit ack bad st={}", rep[0])),
            };
            ui.note_submitted();

            // FRAME_CALLBACK：blit-at-SUBMIT 形下帧已呈现（token 校验即
            // 答）——断 PRESENTED（DISCARDED = 呈现链破相 fail loud）。
            // status/credits 单源仍在本应答（P3 回执无 credits 域）。
            let m = enc_req(gp::encode_present_req(
                &mut req,
                &gp::PresentReq::FrameCallback { frame_token: token },
            ));
            let (n, _) = rpc(cfg.hw, &req[..m], &[], &mut rep, &mut rhs);
            let done = match gp::parse_frame_done_rep(&rep[..n]) {
                Some(d) => d,
                None => fail(&format!("frame done bad st={}", rep[0])),
            };
            if done.status != gp::FRAME_PRESENTED {
                fail(&format!("frame status={}", done.status));
            }

            // P3 呈现回执（refresh 刀4——遥测第一个读者）：presented_ns/
            // refresh_ns 服务端单源真相接入（actual 轨 + jank 周期基——
            // LTPO 切档后静态 estimate 过期即纠）；seq = 本帧引擎序
            //（render 后帧计数 -1 = last_seq）；credits 以 FRAME_CALLBACK
            // 应答为真值。
            let m = enc_req(gp::encode_present_req(
                &mut req,
                &gp::PresentReq::FrameTiming { frame_token: token },
            ));
            let (n, _) = rpc(cfg.hw, &req[..m], &[], &mut rep, &mut rhs);
            let rec = match gp::parse_frame_timing_rep(&rep[..n]) {
                Some(r) => r,
                None => fail(&format!("frame timing bad st={}", rep[0])),
            };
            ui.note_frame_timing(
                (ui.telemetry().frames() - 1) as u32,
                rec.presented_ns,
                rec.refresh_ns,
                done.status,
                done.credits,
            );

            // RELEASE（归还别名即关——在途 ≤1 结构性背压，见头注）。
            // RETURN-ALL 语义 + 跨客户端宽容收口（1B 拒答重试 + 随行全量
            // close）——语义全账见 [`release_round`]。本处刚 SUBMIT 过一帧
            // 在途，count==0 = 别名已被并发客户端提前排空（实抓形态）——
            // diag 一行宽容继续（早归还良性，见 release_round 注）。
            let m = enc_req(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
            let rel = release_round(cfg.hw, &req[..m], &mut rep, &mut rhs);
            if rel.count == 0 {
                diag("release count=0（别名已被并发客户端提前排空——宽容继续）");
            }

            presented += 1;
            buf_id = buf_id.wrapping_add(1);
            if buf_id == 0 {
                buf_id = 1;
            }
            hooks.on_presented(ui, presented);
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
            expect_ok(cfg.hw, &req[..m], &[], &mut rep, &mut rhs, "vote");
            last_vote = Some(ballot);
        }

        hooks.on_frame(ui, presented, t_now);

        // —— 稳态等待：wait_many [wm READABLE (+input READABLE)] +
        //    deadline = min(next_wakeup, now+IDLE 帽, extra)；TIMEDOUT =
        //    期限到正常醒——渲染下一拍 / Idle 空转睡回。
        let deadline = wait_deadline(ui.next_wakeup(), now(), hooks.extra_deadline());
        let mut hs = [cfg.hw, 0 as Handle];
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

/// RELEASE_WAIT 一轮收口（boot 跨代清账与帧尾 RELEASE 共用）。
///
/// **宽容契约（2026-11-20 应用内全冻 + 导航三键零反应事故刀）**：
/// guifbd 的 held 是**进程级全局单表**（CREDITS=2，单 present 客户端
/// 时代几何），RELEASE_WAIT 应答 = 全量排空迁给请求者——K1 per-app
/// 通道后 present 域多客户端并存，任一客户端的 RELEASE（含新客户端
/// boot 跨代清账）都会把他人在途别名随应答迁来。两形处理：
/// - **count==0 合法**：本端别名已被并发客户端提前排空。别名只是本端
///   帧缓冲 VMO 的 READ|TRANSFER dup（原柄恒在本端表内），对方 close
///   = 纯引用计数下降；本帧 PRESENTED 已先断言过链（blit 读的 VMO 由
///   原柄保活）——早归还良性。旧 `count<1 → fail` 断言在此形把桌面
///   launcher 整体 park（实抓 serial `launcher: release count=0 nh=0`；
///   park 非 crash，监督一次重启永不触发——桌面/导航三键/输入泵全灭）。
///   boot 清账位 0 = 空表常态（诊断归帧尾调用点）。
/// - **1B 状态拒答重试**：ST_BUSY = 他客户端 parked RELEASE 占用
///   guifbd `rel_park` 单槽（本请求被拒未入账，可安全重发）——步进重试
///   至 [`RELEASE_RETRY_CAP_NS`]，到帽才 fail（真死链面）。
/// 应答随行 handle（0..N，可含他人别名）全量 close——他人别名 = 他人
/// VMO 的引用下降，其原柄在其进程，恒安全。
#[cfg(not(test))]
fn release_round(
    hw: Handle,
    req: &[u8],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> gp::ReleaseRep {
    let mut cap_dl = 0u64;
    loop {
        let (n, nh) = rpc(hw, req, &[], rep, rhs);
        if let Some(rel) = gp::parse_release_rep(&rep[..n]) {
            for h in &rhs[..nh] {
                close_h(*h);
            }
            return rel;
        }
        for h in &rhs[..nh] {
            close_h(*h); // 状态拒答零随行——防御面恒关
        }
        if cap_dl == 0 {
            cap_dl = now() + RELEASE_RETRY_CAP_NS;
        }
        if now() >= cap_dl {
            fail(&format!("release rep bad st={}", rep[0]));
        }
        diag(&format!("release busy st={} 步进重试", rep[0]));
        if berry_thread_sleep(now() + RELEASE_RETRY_STEP_NS) != 0 {
            fail("release retry sleep");
        }
    }
}

/// 挂一轮 INPUT_POLL：inputd 阻塞批拉 ≤3 事件；-EBUSY = 通道满（前一轮
/// 未收），等 WRITABLE 后重试；其余错 = 输入面失效，返回 false。
#[cfg(not(test))]
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
            diag(&format!("input send err={s}"));
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            diag("input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use berry_guiproto::Rect;

    fn dmg(rects: &[(u32, u32, u32, u32)], full: bool) -> DamageSet {
        let mut d = DamageSet {
            rects: [Rect { x: 0, y: 0, w: 0, h: 0 }; berry_guiproto::DAMAGE_MAX],
            count: rects.len() as u8,
            adds: 0,
            full,
        };
        for (i, &(x, y, w, h)) in rects.iter().enumerate() {
            d.rects[i] = Rect { x, y, w, h };
        }
        d
    }

    /// 包络 = K 矩形并集外接（含分离带）；饱和加法（u32 界右下角不回卷：
    /// w/h 在 u32 域饱和后折 i32）。
    #[test]
    fn damage_envelope_is_union_bounding_box() {
        let d = dmg(&[(10, 20, 5, 5), (100, 200, 10, 10)], false);
        let e = damage_envelope(&d);
        assert_eq!((e.x, e.y, e.w, e.h), (10, 20, 100, 190));
        // 单 rect 恒等。
        let d1 = dmg(&[(0, 0, 800, 600)], false);
        let e1 = damage_envelope(&d1);
        assert_eq!((e1.x, e1.y, e1.w, e1.h), (0, 0, 800, 600));
        // 饱和（w/h 在 u32 域先饱和再折 i32）。
        let ds = dmg(&[(u32::MAX - 3, u32::MAX - 3, 4, 4)], false);
        let es = damage_envelope(&ds);
        assert_eq!((es.w, es.h), (3, 3));
    }

    /// 整帧判据：FULL 旗或 count==0；K>0 且无 FULL = 增量帧。
    #[test]
    fn damage_full_predicate() {
        assert!(damage_is_full(&dmg(&[], true)));
        assert!(damage_is_full(&dmg(&[], false))); // 空集视同整帧（立法兜底）
        assert!(!damage_is_full(&dmg(&[(1, 2, 3, 4)], false)));
        assert!(damage_is_full(&dmg(&[(1, 2, 3, 4)], true)));
    }

    /// 等待帽：min(next_wakeup, now+IDLE, extra)；None 让位；next_wakeup
    /// 晚于 IDLE 帽时收帽；额外帽（armed 窗类 3s 准时醒）最早者胜。
    #[test]
    fn wait_deadline_takes_earliest() {
        let now = 1_000_000_000;
        // 无定时无额外 = IDLE 帽。
        assert_eq!(wait_deadline(None, now, None), now + IDLE_CAP_NS);
        // 定时早于帽 = 定时。
        assert_eq!(wait_deadline(Some(now + 1_000), now, None), now + 1_000);
        // 定时晚于帽 = 帽。
        assert_eq!(wait_deadline(Some(now + IDLE_CAP_NS * 2), now, None), now + IDLE_CAP_NS);
        // 额外帽最早。
        assert_eq!(wait_deadline(None, now, Some(now + 3_000_000_000)), now + 3_000_000_000);
        assert_eq!(
            wait_deadline(Some(now + 1_000), now, Some(now + 500)),
            now + 500
        );
    }
}
