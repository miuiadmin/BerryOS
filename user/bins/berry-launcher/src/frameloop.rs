//! frameloop —— launcher 帧循环特化（GUI-8 段③ W2，docs/GUI-8实施设计
//! 草案.md §4 段③「长活帧循环 embedder」；波3-件⑦ 起帧循环骨架抽入
//! appshell::pump——本模块经 [`LoopHost`]（appshell::pump::HostHooks 实现）
//! 注入 launcher 特化，操作序逐字保真）。
//!
//! 帧循环体（骨架在 appshell::pump，特化在此）：
//!
//! ```text
//! pump::run(起振) → loop {
//!   on_head：armed 过期复位 → 状态栏时钟续摆（分钟字变才动笔）
//!   [pump] input 收/挂 → pump_input → on_msgs（五键导航/tab 命中优先，
//!     槽命中 → APP_SPAWN；pager.on_msg → 翻页 rehang）
//!   on_tick：pager.tick（snap 动画步进）
//!   [pump] render → SUBMIT（surface_id=0xFFFF 系统面豁免——guifbd
//!     波3-件② 带裁剪）→ FRAME_CALLBACK → RELEASE → on_presented
//!     （`LAUNCH ok` 首帧恰一次）→ vote
//!   on_frame：selftest 状态机（`PAGE ok n=2`）→ click-ready 谓词
//!   [pump] wait_many（deadline 含 armed 窗——extra_deadline）
//! }
//! ```
//!
//! **稳态 = 非燃 CPU 阻塞形**：Idle 相 10s IDLE 帽单次睡满（骨架面）；
//! 时钟续摆每轮回读 CLOCK_REALTIME——分钟界最迟 10s 内换字。
//!
//! **自测**（裁决七：合成注入在 push_pointer = 逻辑真线）：首帧
//! FRAME_CALLBACK 过链 → `LAUNCH ok`；合成 swipe ×2（视口广义坐标，
//! 4 事件 1ms 间隔 → MOVE 滑窗 span=1ms < RESAMPLE_MIN_DELTA_MS=2ms
//! 恒 None 直通——原始坐标保真；位移 0.41·vp_w > 0.4·vp_w 翻页阈 +
//! 速度环远超 500px/s fling 阈，双判据同向）各落定 → `PAGE ok n=2`。
//!
//! **自测门**（I3 性能止血）：`smoke ∨ 壁钟注入缺席` 才跑——gui8 三靴
//!（A 双靴/B 常态对照/--normal-boot 常态隔离）与 smoke-image EFI 靴全在
//! 断言面且全无 `opt/berry/rtc` 注入（PAGE ok 照打）；宿主 berry 真实
//! 交互路恒注 rtc → WaitFirst 短路 Done，开机零 swipe——两轮 swipe 与
//! 真实输入同队列相撞的开机「点不动」窗消失（判据全账见 [`run`] 头注）。
//!
//! **段④ 点击面（真实输入 → APP_SPAWN）**：`Msg::Click` 槽命中 →
//! `app_cli_h()!=0` 时真发 APP_SPAWN；**首例 ok 回执恰一次**打 `CLICK
//! ok n=1`——**仅当 `smoke` 为真**（main 经 GUI8.SMK 精确载荷判定）。
//! 失败回执维持 eprintln 诊断、零标记——spawn 是旁路动作，不打断帧循环。
//! **正常交互 boot（smoke=false）点击照常 spawn，但 CLICK 行与 click
//! ready 行都不打**（冻结相 app_cli_h()==0 更早退：零 RPC 零打印）。
//! **覆盖态点击门（2026-11-20 桌面点击无反应事故刀）**：v1 输入单订阅
//! 者（指针事件恒全投 launcher）∧ 无合成器——应用窗开盖后桌面命中面仍
//! 活着，连点被误读成连环 spawn；svcmgr `APP_SPAWN_LIVE_MAX=4` 槽被同
//! app 烧尽后一切 APP_SPAWN 拒「app slots」（实抓：`eco: app MMS it 03`
//! ×4 后 `eco: app FILES app slots` 连环拒）。点击先查表（TABLE_GET），
//! 非自 APP 可见窗在场 = 收盖（DESTROY + wake FULL 重画）不落 spawn 面；
//! 失败 ack 一律 eprintln loud（原静默吞 = 用户零反馈的「无反应」面）。
//! `launcher: click ready` 非标记行 = 宿主注入的**状态谓词**门（规则/07
//! A32 禁睡等竞速）：`PAGE ok` 后 `snap_to(0)` 落位且其后已再呈一帧
//!（presented 增长——吸附完成帧已出，命中面回到 page 0 的 app 格）
//! 才打，且同样仅限 `smoke`；恰一次；`hi==0`（无 input gift）恒不打。
//!
//! 裁决四-c（输入拓扑）：真实输入订阅 = 段④（input extras 冻结相结构
//! 性缺席）；裁决四-e（归属）：状态栏/壁纸/桌面全在 launcher 进程内。

use berry_appshell as appshell;
use berry_appproto as ap;
use berry_guiproto as gp;
use berry_libc::io::{app_cli_h, self_vmar_h};
use berry_sys::{berry_clock_read, berry_power_off, Handle, CLOCK_REALTIME};
use berry_ui::desc::{IconProps, MsgBuf, Props};
use berry_ui::hit::PtrEvent;
use berry_ui::{Msg, NodeId, UiMain};

use appshell::rpc::{fail, rpc};

use crate::desktop::{self, Desktop, NavNodes, POWER_ARM_WINDOW_NS};
use crate::icons;
use crate::settings_io;
use crate::syspages;

/// 整帧清屏底色（gfxdemo 同值——FULL 帧先铺底再回放 DisplayList）。
const CLEAR_BG: (u8, u8, u8) = (0x10, 0x18, 0x28);

/// 自测 swipe 收敛帽（超时 = eprintln 诊断 + 放弃——PAGE 缺失由 smoke
/// 兜底 loud；不 fail 挂死：桌面常驻义务优先于自测件）。
const SETTLE_CAP_NS: u64 = 5 * 1_000_000_000;

/// 自测状态机（boot 一次性走完 → Done 常驻；门控见 [`run`] 头注——
/// 真实交互 boot 在 WaitFirst 即短路 Done，状态机不推进）。
#[derive(Clone, Copy)]
enum Selftest {
    /// 首帧在途——presented>0 时注入第一轮 swipe（LAUNCH ok 已由
    /// on_presented 打出——首帧 FRAME_CALLBACK 过链即打）；自测门
    /// 关（真实交互 boot）即短路 Done。
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

/// launcher 帧循环特化状态（appshell::pump::HostHooks 实现——全部字段
/// 即抽取前 frameloop::run 的局部态）。
struct LoopHost {
    /// wm/present 客户端端柄（back 键查表/收窗 RPC 面）。
    hw: Handle,
    /// 桌面静态面句柄。
    desktop: Desktop,
    /// 冒烟模式（段④ 两标记的唯一门）。
    smoke: bool,
    /// 自测门（I3）：`smoke ∨ 壁钟注入缺席`——run 构造期一次定格
    ///（判据全账见 [`run`] 头注；真实交互 boot = false → WaitFirst
    /// 短路 Done）。
    selftest: bool,
    /// input gift 在场（run 参数定格——hi 后续在 pump 内可因输入面失效
    /// 置 0，门仍真）。
    has_input: bool,
    /// APP_SPAWN ok 回执计数（CLICK ok 恰一次）。
    spawn_ok: u32,
    /// `LAUNCH ok` 恰一次。
    launched: bool,
    /// 多任务键记忆（最近 spawn 成功的 app）。
    recents_app: Option<RecentsApp>,
    /// 自窗记账（K2）：双 CREATE ack 窗 id——[0]=SYSTEM 状态栏窗、[1]=APP
    /// 桌面窗。back 键焦点命中自窗视同无应用（wmd refocus 回落桌面窗后
    /// 按 back = 自毁雷）；home 键 DESTROY 批只收非自窗。
    self_wins: [u16; 2],
    /// 自测状态机。
    st: Selftest,
    /// PAGE ok 后 snap_to(0) 的呈现基线。
    snap_frame: Option<u64>,
    /// `launcher: click ready` 恰一次。
    click_ready: bool,
    /// 关机二击确认窗（Some(deadline) = armed 态——3s 内再击 = 真关机；
    /// 过期回常态，deadline 纳入稳态等待帽保证准时醒复位）。
    power_armed: Option<u64>,
    /// 状态栏时钟续摆：上帧摆字（None = 尚未/永不摆：注入缺席时
    /// REALTIME 恒 -ENOTSUP，本面静默，桌面保持建树时的 12:00 占位）。
    last_clock: Option<[u8; 5]>,
}

/// 帧循环入口（W2 面 + K2 自窗账——`hw` = wm 客户端端柄（wm/present 同
/// 端混流，ADR #61 中继），`ui` = 泄堆 arena，`desktop` = 静态面句柄，
/// `smoke` = main 单次读取的 GUI8.SMK 精确载荷判定——门控段④两标记 +
/// 自测门两臂之一，不改点击/帧链行为；`self_wins` = main 双 CREATE ack
/// 窗 id 存档（[0]=SYSTEM 状态栏窗、[1]=APP 桌面窗）——back/home 键
/// 自窗排除面（桌面窗永不被导航键 DESTROY））。
///
/// **自测门判据（I3 前置核实定谳，2026-11-18 现场核树）**：`PAGE ok
/// n=2` 的全部断言面 = gui8 靴 A（SMK 在）+ 靴 B 与 `--normal-boot`
/// 常态隔离靴（无 SMK）+ smoke-image EFI 靴（零 fw_cfg 无数据盘）——
/// GUI8.SMK 旗**不覆盖**后三形态，`!smoke → Done` 一刀会炸两门。四形态
/// 拓扑结构性无壁钟注入（gui8 仅注 `opt/berry/display`；EFI 路零
/// fw_cfg → `CLOCK_REALTIME` 恒 -ENOTSUP），宿主 berry 真实交互路恒注
/// `opt/berry/rtc`（qemu.rs fwcfg 表，SystemTime 可读即有值）——门取
/// 两臂之或 `smoke ∨ 壁钟不可读`：冒烟形态照打，真实桌面短路。
pub fn run(
    hw: Handle,
    hi: Handle,
    ui: &'static mut UiMain,
    desktop: Desktop,
    smoke: bool,
    self_wins: [u16; 2],
) -> ! {
    let mut rt0 = 0u64;
    let selftest = smoke || berry_clock_read(CLOCK_REALTIME, &mut rt0) != 0;
    let mut host = LoopHost {
        hw,
        desktop,
        smoke,
        selftest,
        has_input: hi != 0,
        spawn_ok: 0,
        launched: false,
        recents_app: None,
        self_wins,
        st: Selftest::WaitFirst,
        snap_frame: None,
        click_ready: false,
        power_armed: None,
        last_clock: None,
    };
    let cfg = appshell::pump::PumpCfg {
        hw,
        hi,
        vmar: self_vmar_h(),
        // 波3-件②：系统面 marker（guifbd 带裁剪豁免——launcher 帧自带
        // 状态栏/导航条带视觉）。
        surface_id: 0xFFFF,
        clear_bg: CLEAR_BG,
    };
    appshell::pump::run(cfg, ui, &mut host)
}

impl appshell::pump::HostHooks for LoopHost {
    fn on_head(&mut self, ui: &mut UiMain, t_now: u64) {
        // —— 波一刀 D：armed 窗过期复位（视觉回常态面 + 清窗——wait
        //    deadline 已含本窗，3s 准时醒；复位 set_props 自标 damage
        //    出帧，无需另 wake）。
        if let Some(dl) = self.power_armed {
            if t_now > dl {
                set_power_face(ui, &self.desktop.nav, false);
                self.power_armed = None;
            }
        }

        // —— 波一刀 B：状态栏真时钟续摆——稳态每 10s（IDLE 帽）醒一回，
        //    读 CLOCK_REALTIME；分钟字变才动笔（set_text 等长换文 = 纯
        //    PAINT；a11y label 同步 in-place 覆写）+ wake（Idle→Run 保
        //    产出帧——inject_swipe 同款正统路径）。分钟界最迟 10s 内
        //    换字（分钟级摆字语境足够）。
        let mut rt = 0u64;
        if berry_clock_read(CLOCK_REALTIME, &mut rt) == 0 {
            let hm = appshell::scene::hhmm_utc(rt);
            if self.last_clock != Some(hm) {
                if let Err(e) = ui.set_text(self.desktop.clock, &hm) {
                    fail(&format!("clock set_text err={e:?}"));
                }
                let mut label = *b"00:00 utc";
                label[..5].copy_from_slice(&hm);
                // a11y in-place 覆写（同节点不占新槽）；Err 非关键面容忍。
                let _ = ui.set_a11y(self.desktop.clock, berry_ui::ROLE_TEXT, &label);
                ui.wake(appshell::rpc::now());
                self.last_clock = Some(hm);
            }
        }
    }

    fn on_msgs(&mut self, ui: &mut UiMain, msgs: &MsgBuf, t_now: u64) {
        for m in msgs.iter() {
            match m {
                Msg::Click { node } => {
                    // 五键导航先判（含 BottomTabs 三页位——命中 = 消费，
                    // 不落槽 spawn 面）。导航键是覆盖态的逃生面，恒最先。
                    if !on_nav_click(
                        *node,
                        ui,
                        &mut self.desktop,
                        &mut self.power_armed,
                        t_now,
                        &mut self.recents_app,
                        self.hw,
                        self.self_wins,
                    ) {
                        // 覆盖态点击门（2026-11-20 桌面点击无反应事故刀）：
                        // v1 无合成器 + 输入单订阅者（事件恒全投 launcher），
                        // 应用窗开盖后桌面命中面仍活着——连点被误读成连环
                        // spawn，svcmgr APP_SPAWN_LIVE_MAX=4 槽被同 app 烧尽
                        // 后一切 spawn 静默拒「app slots」=「点击无反应」。
                        // 覆盖态点击改收盖（back 键同款 DESTROY + wake 重画）。
                        if self.spawn_ok == 0
                            || !on_covered_tap(ui, self.hw, &self.self_wins, t_now)
                        {
                            if !on_sys_click(
                                *node,
                                ui,
                                &mut self.desktop,
                                &mut self.spawn_ok,
                                self.smoke,
                                &mut self.recents_app,
                            ) {
                                on_click(
                                    *node,
                                    &self.desktop,
                                    &mut self.spawn_ok,
                                    self.smoke,
                                    &mut self.recents_app,
                                );
                            }
                        }
                    }
                }
                _ => {
                    // pager.on_msg true = 翻页目标变更 → a11y/press 重挂
                    //（幂等；Err = 容量违例 fail 收口）。sys 页在树时桌面
                    // 被整面盖住——隐藏 pager 不吃翻页手势（可见性真相面；
                    // Msg 不劫持，只是不喂）。
                    if self.desktop.tabs.cur == 0 && self.desktop.pager.on_msg(ui, m) {
                        if let Err(e) = desktop::rehang_view(ui, &mut self.desktop) {
                            fail(&format!("rehang view err={e:?}"));
                        }
                    }
                }
            }
        }
        // —— 波3-件⑤c：组件事件（widget_react 派发面）——设置控件真值
        //    回写 + 持久化（Toggle/Seg/Radio 翻转与 Slider 拖动均由引擎
        //    组件反应面即时改 props，此处只追真值镜像与落盘）。
        for ev in ui.take_events().iter() {
            on_widget_event(ui, &mut self.desktop, ev.event, ev.value);
        }
    }

    fn on_tick(&mut self, ui: &mut UiMain, t_now: u64) {
        // snap 动画步进（每迭代必调——snap 驱动面；返回值仅为「有吸附
        // 在推进」观察位，自测判据已不依赖）。
        self.desktop.pager.tick(ui, t_now);
    }

    fn on_presented(&mut self, ui: &mut UiMain, presented: u64) {
        if !self.launched && presented >= 1 {
            // 标记（字节精确）：首帧呈现回执过链恰一次。附带遥测快照行
            //（refresh 刀4 第一个读者到席证）：jank/deadline_miss/
            // spinup_late/wakes/frames 全零策略账本只读快照——判定单源
            // 在引擎循环。
            let t = ui.telemetry();
            let jank_n = t.last(16).filter(|r| r.jank).count();
            println!(
                "launcher: telem f={} w={} i={} d={} s={} j={}",
                t.frames(),
                t.wakes(),
                t.idle_exits(),
                t.deadline_miss(),
                t.spinup_late_frames(),
                jank_n
            );
            println!("LAUNCH ok");
            self.launched = true;
        }
    }

    /// refresh 刀1 §4-5：PANEL_CAPS 率通路串口证据行（规范 §11 验收面；
    /// 仅 v2 面打印——旧内核混跑缺席即旧面，非异常。标记互非子串已核
    ///（tools/ 零 PANEL））。注入面与 pump 默认实现同式（set_vsync——
    /// 覆写内直写，防 `Trait::method(self,…)` 解析回本覆写的自递归）。
    fn on_panel_caps(&mut self, ui: &mut UiMain, caps: Option<&gp::PanelCapsRep>) {
        if let Some(c) = caps {
            println!("PANEL ok hz={} ns={}", c.current_hz, c.current_period_ns);
            ui.set_vsync(c.current_period_ns as u64, c.current_phase_ns as u64);
        }
    }

    fn on_frame(&mut self, ui: &mut UiMain, presented: u64, t_now: u64) {
        // —— 自测状态机（boot 一次性；标记字节精确）——
        match self.st {
            Selftest::WaitFirst => {
                if !self.selftest {
                    // 门控短路（I3）：真实交互 boot（非冒烟 ∧ 壁钟在场）
                    // 不自测——两轮 swipe 与真实输入同队列相撞的开机
                    // 「点不动」窗整体消失。LAUNCH ok/telem 不受影响
                    //（on_presented 面）；PAGE ok 在此形态无断言面。
                    self.st = Selftest::Done;
                } else if presented > 0 {
                    inject_swipe(ui, t_now);
                    self.st = Selftest::Settle1 { pc: presented, deadline: t_now + SETTLE_CAP_NS };
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
                if self.desktop.pager.page() == 1 && presented > pc {
                    inject_swipe(ui, t_now);
                    self.st = Selftest::Settle2 { pc: presented, deadline: t_now + SETTLE_CAP_NS };
                } else if t_now > deadline {
                    eprintln!(
                        "launcher: selftest page1 settle timeout (page={})",
                        self.desktop.pager.page()
                    );
                    self.st = Selftest::Done;
                }
            }
            Selftest::Settle2 { pc, deadline } => {
                // 同 Settle1 判据（裁定 + 帧流）——竞态面同理。
                if self.desktop.pager.page() == 2 && presented > pc {
                    // 标记（字节精确）：两轮翻页各落一帧呈现。
                    println!("PAGE ok n=2");
                    // 段④ 收尾：自测只验状态机，落回 page 0（默认布局的
                    // app 全在第 0 页；停在 page 2 = 用户看到空桌面）。
                    self.desktop.pager.snap_to(ui, 0);
                    if let Err(e) = desktop::rehang_view(ui, &mut self.desktop) {
                        fail(&format!("rehang view reset err={e:?}"));
                    }
                    // 命中面回 page 0 的呈现基线（click ready 状态谓词用）。
                    self.snap_frame = Some(presented);
                    self.st = Selftest::Done;
                } else if t_now > deadline {
                    eprintln!(
                        "launcher: selftest page2 settle timeout (page={})",
                        self.desktop.pager.page()
                    );
                    self.st = Selftest::Done;
                }
            }
            Selftest::Done => {}
        }

        // —— 段④ click ready 非标记门（规则/07 A32：状态谓词，禁睡等竞速）
        //    `snap_to(0)` 落位后须已再呈一帧（presented 增长 = 吸附完成帧
        //    出，命中面稳落 page 0）才宣布可注入；`has_input` 缺席或非冒烟
        //    模式恒不打（冻结相/正常 boot 字节零扰动）。恰一次。
        if click_ready_gate(
            self.smoke,
            self.has_input,
            self.click_ready,
            self.snap_frame,
            presented,
            self.desktop.pager.page(),
        ) {
            println!("launcher: click ready");
            self.click_ready = true;
        }
    }

    fn extra_deadline(&mut self) -> Option<u64> {
        // 刀 D：armed 窗纳入等待帽——3s 准时醒做过期复位（不醒 = 红键
        // 滞留至 IDLE 帽 10s，确认语义受损）。
        self.power_armed
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

/// 关机键视觉态切换（刀 D：armed ↔ 常态——set_props 换 Icon id，同
/// w/h 纯 PAINT 面；Err 非关键面 eprintln 容忍，关机链不受视觉件拖累）。
fn set_power_face(ui: &mut UiMain, nav: &NavNodes, armed: bool) {
    let id = if armed { icons::NAV_POWER_ARMED } else { icons::NAV_POWER };
    let props = Props::Icon(IconProps {
        id,
        fg: crate::desktop::ICON_FG,
        w: Some(nav.key_px),
        h: Some(nav.key_px),
    });
    if let Err(e) = ui.set_props(nav.power, props) {
        eprintln!("launcher: power face set err={e:?}");
    }
}

/// back 键焦点应用窗判据（纯谓词，测试面）：FOCUSED ∧ LAYER_APP ∧ 非自
/// 窗——自窗（APP 层桌面窗）在焦视同无应用（降次级语义，桌面窗永不被
/// back DESTROY）。wmd 单焦点槽下 FOCUSED 恒至多一枚，find 即取焦点。
fn focused_app_win(snap: &gp::WmSnapshot, self_wins: &[u16; 2]) -> Option<u16> {
    snap.entries
        .iter()
        .take(snap.count)
        .find(|e| {
            e.flags & gp::WM_WIN_F_FOCUSED != 0
                && e.layer == gp::WM_LAYER_APP
                && !self_wins.contains(&e.id)
        })
        .map(|e| e.id)
}

/// home 键 DESTROY 批目标筛（纯谓词，测试面）：LAYER_APP ∧ 可见 ∧ 非自
/// 窗——状态栏 SYSTEM 窗与桌面自窗永不在批内；隐藏应用窗不收（v1 语义
/// 只收可见窗）。快照表序出批（逐发逐收）。
fn home_destroy_ids<'a>(
    snap: &'a gp::WmSnapshot,
    self_wins: &'a [u16; 2],
) -> impl Iterator<Item = u16> + 'a {
    snap.entries
        .iter()
        .take(snap.count)
        .filter(|e| {
            e.layer == gp::WM_LAYER_APP
                && e.flags & gp::WM_WIN_F_VISIBLE != 0
                && !self_wins.contains(&e.id)
        })
        .map(|e| e.id)
}

/// 覆盖面判据（纯谓词，测试面）：非自 ∧ LAYER_APP 的可见窗在场 = 桌面被
/// 应用画面覆盖（v1 无合成器，像素共享 fb——应用帧盖在桌面像素之上，但
/// 输入单订阅者模型下指针事件仍全投 launcher，桌面命中面在覆盖态不可
/// 达）。焦点窗优先（wmd CREATE 接焦 = 覆盖者），无焦点回退表序首可见
/// 窗。仅自窗/隐藏窗/SYSTEM 层 = None（桌面未覆盖）。
fn covering_app_win(snap: &gp::WmSnapshot, self_wins: &[u16; 2]) -> Option<u16> {
    let visible = |e: &gp::WmEntry| {
        e.layer == gp::WM_LAYER_APP
            && e.flags & gp::WM_WIN_F_VISIBLE != 0
            && !self_wins.contains(&e.id)
    };
    let entries = &snap.entries[..snap.count];
    entries
        .iter()
        .find(|e| visible(e) && e.flags & gp::WM_WIN_F_FOCUSED != 0)
        .or_else(|| entries.iter().find(|e| visible(e)))
        .map(|e| e.id)
}

/// 覆盖态点击处置（back 键「先收应用」支路的点击面同款）：查表见非自
/// APP 窗在场 = 收盖——DESTROY 该窗 + wake（空脏集置 FULL 的正统重画路，
/// 覆盖残留像素）后消费本次点击；无覆盖/查表失败 = false（回落原点击
/// 面——fail-open，覆盖判据病态不连坐桌面常行为）。DESTROY 失败
/// eprintln 诊断（窗口表与像素面失联的异常面），仍 wake 仍消费（防同拍
/// 连击再落 spawn 面把失败面放大）。
fn on_covered_tap(ui: &mut UiMain, wm_h: Handle, self_wins: &[u16; 2], t_now: u64) -> bool {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_table_get_req(&mut req);
    let (n, _) = rpc(wm_h, &req[..m], &[], &mut rep, &mut rhs);
    let target = match gp::parse_wm_snapshot_rep(&rep[..n]) {
        Some(Ok(snap)) => covering_app_win(&snap, self_wins),
        _ => None,
    };
    let Some(wid) = target else { return false };
    let m = gp::encode_wm_destroy_req(&mut req, wid);
    let (n, _) = rpc(wm_h, &req[..m], &[], &mut rep, &mut rhs);
    if rep[0] != 0 {
        eprintln!("launcher: covered destroy id={} st={}", wid, rep[0]);
    }
    let _ = n;
    ui.wake(t_now);
    true
}

/// 导航五键命中 + 动作（刀 D，审计 02 号 §6.1 M2——走现役 `Msg::Click`
/// 通道零新输入面）。返回 true = 已消费（不落槽 spawn）；false = 非导航
/// 键（调用方续走 app 槽命中）。
///
/// - **返回**：波3-件④——应用在焦先收应用（查表→焦点窗 DESTROY→
///   wmd 表序回焦桌面）；焦点命中自窗（K2 排雷：wmd refocus 回落后
///   焦点常驻桌面窗自身）视同无应用在焦，走次级语义——桌面窗永不被
///   back 自毁；无应用在焦 = 桌面翻页语义（v1 原路）。
///   launcher/wmd 同步 RPC 无死锁对（guifbd 的 SUBMIT 内禁同步 RPC
///   wmd 才是死锁对——launcher 非 blit 路径）。
/// - **多任务**：重新拉起最近 spawn 的应用（真重拉非死键）；无历史 =
///   静默无操作。
/// - **桌面**：K2 补 DESTROY 批——回 tab+page0 之前先收快照内全部非自
///   窗 ∧ LAYER_APP 可见窗（逐发逐收），批中有成收即 wake 触发 FULL
///   重画（v1 无合成器、像素共享 fb，app 残留像素靠本帧覆盖）；再回
///   tab+page0（`page!=0` 吸附 page 0；已在 = 无操作）。
/// - **旋转**：打 `ROTATE` 串口标记——宿主 berry 窗口监听并换几何
///   重启（guest 无法自转 ramfb 几何——fw_cfg 是宿主注入面）。
/// - **关机**：二击 3s 窗确认——首击 armed（换警示面 + 窗起算）；窗内
///   再击 = 内核 `power_off`（0x88 PSCI SYSTEM_OFF，**成功不归**——整机
///   退出）；失败（conduit 死 / 固件拒）eprintln 诊断 + 回常态不 fail
///   （桌面常驻义务优先）；窗过期复位（on_head）。
fn on_nav_click(
    node: NodeId,
    ui: &mut UiMain,
    desktop: &mut Desktop,
    power_armed: &mut Option<u64>,
    t_now: u64,
    recents: &mut Option<RecentsApp>,
    wm_h: Handle,
    self_wins: [u16; 2],
) -> bool {
    let nav = desktop.nav;
    let snap_page = |ui: &mut UiMain, desktop: &mut Desktop, target: u8| {
        desktop.pager.snap_to(ui, target);
        if let Err(e) = desktop::rehang_view(ui, desktop) {
            fail(&format!("nav rehang view err={e:?}"));
        }
    };
    // 波3-件⑤：系统页 tab 命中（BottomTabs 三页位）——切页优先于
    // 五键导航消费。整树装配（离树/入树/换条/重挂）；Err = 容量违例
    // 非致命收口（桌面常驻义务优先）。
    for (i, &tn) in desktop.tabs.tab_nodes.iter().enumerate() {
        if i < 3 && node == tn {
            if let Err(e) = desktop::switch_sys_page(ui, desktop, i as u8) {
                eprintln!("launcher: sys page switch err={e:?}");
            }
            return true;
        }
    }
    if node == nav.back {
        // 波3-件④ + K2 排雷：应用在焦先收应用（查表→焦点窗 DESTROY→
        // wmd 表序回焦桌面）；焦点命中自窗（wmd refocus 回落后即桌面窗
        // 自身）视同无应用在焦走次级语义——桌面窗永不被 back 自毁；
        // 无应用在焦 = 桌面翻页语义（v1 原路）。
        let mut req = [0u8; 256];
        let mut rep = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let m = gp::encode_wm_table_get_req(&mut req);
        let (n, _) = rpc(wm_h, &req[..m], &[], &mut rep, &mut rhs);
        let focused_app = match gp::parse_wm_snapshot_rep(&rep[..n]) {
            Some(Ok(snap)) => focused_app_win(&snap, &self_wins),
            _ => None,
        };
        if let Some(wid) = focused_app {
            // DESTROY 焦点应用窗（owner/client 不明 v1——wmd 摘表+关窗,
            // 进程退出归其监督;refocus 表序回桌面已是 wmd 内建)。
            let m = gp::encode_wm_destroy_req(&mut req, wid);
            let (n, _) = rpc(wm_h, &req[..m], &[], &mut rep, &mut rhs);
            if rep[0] != 0 {
                eprintln!("launcher: back destroy st={}", rep[0]);
            }
            let _ = n;
        } else if desktop.tabs.cur != 0 {
            // R1：非桌面 tab = 回桌面 tab（sys 页在树先摘——桌面整面回显
            //）；同拍不做页翻（一 back 一义）。
            if let Err(e) = desktop::switch_sys_page(ui, desktop, 0) {
                eprintln!("launcher: back to home tab err={e:?}");
            }
        } else {
            let p = desktop.pager.page();
            if p > 0 {
                snap_page(ui, desktop, p - 1);
            }
        }
        true
    } else if node == nav.recents {
        // 多任务 v1：重新拉起最近 spawn 的应用（真重拉非死键）；无历史
        // = 静默无操作（桌面常驻义务）。整体取出再 spawn——避免 spawn
        // RPC 窗内再借用 desktop。
        if let Some(app) = recents.take() {
            spawn_by_id(&app);
        }
        true
    } else if node == nav.home {
        // 桌面键（K2 补 DESTROY 批）：回 tab+page0 之前，先收快照内全部
        // 非自窗 ∧ LAYER_APP 可见窗——逐发逐收（rpc 单飞天然串行；失败
        // eprintln 不阻批——批面按快照逐项独立）；wmd 摘表+回焦桌面为
        // 内建。批中有成收即 wake：v1 无合成器、像素共享 fb，app 残留
        // 像素靠本帧 FULL 重画覆盖（wake 空脏集置 FULL；订阅面 v1 不
        // 建；零成收不 wake——表未变不重画，免覆盖仍存活窗）。
        let mut req = [0u8; 256];
        let mut rep = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let m = gp::encode_wm_table_get_req(&mut req);
        let (n, _) = rpc(wm_h, &req[..m], &[], &mut rep, &mut rhs);
        let mut destroyed = false;
        if let Some(Ok(snap)) = gp::parse_wm_snapshot_rep(&rep[..n]) {
            for wid in home_destroy_ids(&snap, &self_wins) {
                let m = gp::encode_wm_destroy_req(&mut req, wid);
                rpc(wm_h, &req[..m], &[], &mut rep, &mut rhs);
                if rep[0] != 0 {
                    eprintln!("launcher: home destroy id={} st={}", wid, rep[0]);
                } else {
                    destroyed = true;
                }
            }
        }
        if destroyed {
            ui.wake(t_now);
        }
        // 原回桌面语义：sys 页在树先回桌面 tab（R1 同律）；再落翻页回
        // page 0。
        if desktop.tabs.cur != 0 {
            if let Err(e) = desktop::switch_sys_page(ui, desktop, 0) {
                eprintln!("launcher: home tab err={e:?}");
            }
        } else {
            let p = desktop.pager.page();
            if p != 0 {
                snap_page(ui, desktop, 0);
            }
        }
        true
    } else if node == nav.rotate {
        // 旋转 v1：打 `ROTATE` 串口标记——宿主 berry 窗口监听并换几何
        // 重启（guest 无法自转 ramfb 几何——fw_cfg 是宿主注入面）。标记
        // 字节精确（gui8-smoke 标记族同形）。
        println!("ROTATE");
        true
    } else if node == nav.power {
        match *power_armed {
            Some(dl) if t_now <= dl => {
                // 二击确认：真关机（PSCI SYSTEM_OFF——成功整机退出不归）。
                let st = berry_power_off();
                eprintln!("launcher: power off st={st} -- use host toolbar OFF");
                set_power_face(ui, &desktop.nav, false);
                *power_armed = None;
            }
            _ => {
                // 首击：arm（3s 窗 + 警示面；窗起算点 = 本击时刻）。
                set_power_face(ui, &desktop.nav, true);
                *power_armed = Some(t_now + POWER_ARM_WINDOW_NS);
            }
        }
        true
    } else {
        false
    }
}

/// CLICK 面（修约 1 / 段④）：槽命中映射（cell/label/icon 三节点皆归本
/// 格——最深叶命中面）→ 注册表条目 → `app_cli_h()==0`（冻结相 extras 恰
/// [wm]）静默跳过零 RPC 零打印；在场 = APP_SPAWN 真跑（**正常 boot 与
/// 冒烟 boot 同路**）。**首例 ok 回执**仅在 `smoke` 为真时打 `CLICK ok
/// n=1`（恰一次，`spawn_ok` 计数门——后续成功点击静默，避免重启/连点
/// 制造重复标记）；失败回执维持 eprintln 诊断（spawn 是旁路动作，不打
/// 断帧循环、不 fail、**不产标记**）。
/// 多任务键记忆体：最近一次 spawn 成功的应用 id（二进制拷贝——注册表
/// 条目生命周期不随帧循环，spawn 窗外安全）。
pub struct RecentsApp {
    pub id: [u8; 24],
    pub id_len: u8,
}

/// 按 id spawn（on_click 与多任务键重拉共用）。回执记账返回 ok 与否。
fn spawn_by_id_inner(id: &[u8], smoke: bool, spawn_ok: &mut u32) -> bool {
    let h = app_cli_h();
    if h == 0 {
        return false; // 冻结相：app 通道结构性缺席
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    match ap::encode_spawn_request(&mut req, id) {
        Some(m) => {
            let (n, _) = rpc(h, &req[..m], &[], &mut rep, &mut rhs);
            let ack = ap::parse_spawn_ack(&rep, n);
            // 失败回执 loud 诊断（2026-11-20 事故刀：正常 boot 的失败 ack
            // 原被静默吞——svcmgr 槽满（APP_SPAWN_LIVE_MAX=4）连环拒「app
            // slots」时用户零反馈 =「点击无反应」。err 值域 = svcmgr
            // app_serve_spawn 冻结款：1 = 守卫/拉起败/槽满（细节归 svcmgr
            // eco: app 行）、2 = id 不在册。失败面永不产标记（eprintln 非
            // canon 前缀，与 CLICK ok / click ready 互非子串）。
            match ack {
                Some((false, st)) => eprintln!(
                    "launcher: spawn {} 拒 st={} ({})",
                    core::str::from_utf8(id).unwrap_or("?"),
                    st,
                    if st == 1 {
                        "guard/slots"
                    } else if st == 2 {
                        "unknown id"
                    } else {
                        "?"
                    }
                ),
                None => eprintln!("launcher: spawn ack bad st={}", rep[0]),
                Some((true, _)) => {}
            }
            if note_spawn_ack(smoke, spawn_ok, ack) {
                // 标记（字节精确）：真实 APP_SPAWN ok 回执恰一次，且仅
                // 冒烟模式（正常 boot 零 CLICK）。
                println!("CLICK ok n=1");
            }
            matches!(ack, Some((true, _)))
        }
        None => {
            eprintln!("launcher: spawn id bad");
            false
        }
    }
}

/// 多任务键重拉入口（无 smoke 标记面——正常交互路径）。
fn spawn_by_id(id: &RecentsApp) {
    let mut n = 0u32;
    if !spawn_by_id_inner(&id.id[..id.id_len as usize], false, &mut n) {
        eprintln!("launcher: recents respawn failed");
    }
}

/// sys 页点击路由（波3-件⑤——`tabs.cur != 0` 时；桌面态恒返 false
/// 落槽面）。返回 true = 已消费。
///
/// - 应用列表页：固定过滤词命中 → 换选（换底 + 重装行 + rehang）；
///   行命中（Card/Row/Pad/Text 四面）→ 与桌面格同一 spawn 路（注册表
///   真流——成功记账 recents；空行/标题 = 不消费）。
/// - 设置页：Slider 纯点击步进（+100 permille wrap——Drag 路引擎已接
///   ；步进值落 SETTINGS.JSN）。其余控件 = 引擎组件反应面（事件经
///   [`on_widget_event`] 回写），此处零动作。
fn on_sys_click(
    node: NodeId,
    ui: &mut UiMain,
    desktop: &mut Desktop,
    spawn_ok: &mut u32,
    smoke: bool,
    recents: &mut Option<RecentsApp>,
) -> bool {
    match desktop.tabs.cur {
        1 => {
            // 过滤词命中（id 拷出判定——避免跨 rehang 持 desktop 借）。
            let mut hit_filter = None;
            if let Some(page) = desktop.sys.apps.as_ref() {
                for (i, &wn) in page.words.iter().enumerate() {
                    if node == wn {
                        hit_filter = Some(i as u8);
                        break;
                    }
                }
            }
            if let Some(f) = hit_filter {
                if desktop.sys.filter != f {
                    desktop.sys.filter = f;
                    let big = desktop.sys.big;
                    if let Some(page) = desktop.sys.apps.as_mut() {
                        if let Err(e) = syspages::set_filter(ui, page, f, big) {
                            eprintln!("launcher: filter err={e:?}");
                        }
                    }
                    if let Err(e) = desktop::rehang_view(ui, desktop) {
                        eprintln!("launcher: rehang view err={e:?}");
                    }
                }
                return true;
            }
            // 行命中 → spawn（与桌面格同路；真流注册表条目）。
            let mut hit_app = None;
            if let Some(page) = desktop.sys.apps.as_ref() {
                if let Some(ri) = syspages::row_index_of(page, node) {
                    hit_app = page.rows[ri].app;
                }
            }
            match hit_app.and_then(|idx| icons::registry().get(idx)) {
                Some(entry) => {
                    if spawn_by_id_inner(entry.id_bytes(), smoke, spawn_ok) {
                        let mut id = [0u8; 24];
                        let src = entry.id_bytes();
                        let n = src.len().min(24);
                        id[..n].copy_from_slice(&src[..n]);
                        *recents = Some(RecentsApp { id, id_len: n as u8 });
                    }
                    true
                }
                None => false,
            }
        }
        2 => {
            let slider = desktop.sys.setpg.as_ref().map(|p| p.vol_slider);
            if slider == Some(node) {
                let cur = desktop.sys.settings.volume;
                let next = match desktop.sys.setpg.as_ref() {
                    Some(p) => syspages::step_slider(ui, p, cur),
                    None => cur,
                };
                desktop.sys.settings.volume = next;
                settings_io::save_settings(&desktop.sys.settings);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// 组件事件回写（波3-件⑤c——`ui.take_events` 消费面）：设置控件真值
/// 镜像更新 + 持久化。Toggle/Seg/Radio 的 props 翻转由引擎组件反应面
/// 完成（本面零 props 笔）；Radio 组互斥清位 + 镜像去重归本面。
fn on_widget_event(ui: &mut UiMain, desktop: &mut Desktop, event: u32, value: u32) {
    match event {
        syspages::EV_BOOT => {
            desktop.sys.boot = if value == 1 {
                settings_io::BootMode::Verbose
            } else {
                settings_io::BootMode::Splash
            };
            settings_io::save_boot(desktop.sys.boot);
        }
        syspages::EV_THEME_LIGHT | syspages::EV_THEME_DARK => {
            let dark = event == syspages::EV_THEME_DARK;
            if desktop.sys.settings.dark != dark {
                desktop.sys.settings.dark = dark;
                if let Some(p) = desktop.sys.setpg.as_ref() {
                    syspages::enforce_theme_pair(ui, p, dark);
                }
                settings_io::save_settings(&desktop.sys.settings);
            }
        }
        syspages::EV_ANIM => {
            let anim = value == 1;
            if desktop.sys.settings.anim != anim {
                desktop.sys.settings.anim = anim;
                settings_io::save_settings(&desktop.sys.settings);
            }
        }
        syspages::EV_VOL => {
            desktop.sys.settings.volume = value.min(1000);
            settings_io::save_settings(&desktop.sys.settings);
        }
        _ => {}
    }
}

fn on_click(
    node: NodeId,
    desktop: &Desktop,
    spawn_ok: &mut u32,
    smoke: bool,
    recents: &mut Option<RecentsApp>,
) {
    let slot = desktop
        .slots
        .iter()
        .flatten()
        .chain(desktop.dock.iter())
        .find(|s| s.cell == node || s.label_node == node || s.icon == Some(node));
    let Some(slot) = slot else { return }; // 壁纸/状态栏/圆点/容器白区 = 无格
    let Some(idx) = slot.app else { return }; // 占位/幽灵格不可 spawn
    let Some(entry) = icons::registry().get(idx) else { return };
    if spawn_by_id_inner(entry.id_bytes(), smoke, spawn_ok) {
        // 多任务键记忆：spawn 成功才记账（失败不占 recents 槽）。
        let mut id = [0u8; 24];
        let src = entry.id_bytes();
        let n = src.len().min(24);
        id[..n].copy_from_slice(&src[..n]);
        *recents = Some(RecentsApp { id, id_len: n as u8 });
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

    /// K2 测试夹具：APP 层窗项（flags 可见/焦点自选）。
    fn app_entry(id: u16, flags: u8) -> gp::WmEntry {
        gp::WmEntry {
            id,
            parent: 0,
            group: 0,
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            layer: gp::WM_LAYER_APP,
            z: 0,
            flags,
        }
    }

    /// K2 测试夹具：自窗账 [状态栏 SYSTEM=1, 桌面 APP=2]（main 双 CREATE
    /// 存档序）。
    const SELF_WINS: [u16; 2] = [1, 2];

    /// back 焦点判据真值表：FOCUSED∧LAYER_APP∧非自窗三条件缺一不可——
    /// 桌面自窗在焦 = None（自毁雷排除，降次级语义）；SYSTEM 层窗与
    /// count 截断外的窗永不入选。
    #[test]
    fn back_focus_excludes_self_windows() {
        // app 窗在焦 = 收它。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(3, gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED);
        e[1] = app_entry(SELF_WINS[1], gp::WM_WIN_F_VISIBLE);
        e[2] = app_entry(SELF_WINS[0], gp::WM_WIN_F_VISIBLE);
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 800, disp_h: 1280, entries: e, count: 3 };
        assert_eq!(focused_app_win(&snap, &SELF_WINS), Some(3));

        // 桌面自窗（APP 层）在焦 = 视同无应用（wmd refocus 回落常态）。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(SELF_WINS[1], gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED);
        e[1] = app_entry(3, gp::WM_WIN_F_VISIBLE);
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 800, disp_h: 1280, entries: e, count: 2 };
        assert_eq!(focused_app_win(&snap, &SELF_WINS), None);

        // 状态栏自窗在焦但层不符（SYSTEM 层门拒——理论形钉板）。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = gp::WmEntry {
            layer: gp::WM_LAYER_SYSTEM,
            flags: gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED,
            ..app_entry(SELF_WINS[0], 0)
        };
        e[1] = app_entry(3, gp::WM_WIN_F_VISIBLE);
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 800, disp_h: 1280, entries: e, count: 2 };
        assert_eq!(focused_app_win(&snap, &SELF_WINS), None);

        // app 窗只可见不焦点 = 不收（FOCUSED 门）。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(3, gp::WM_WIN_F_VISIBLE);
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 800, disp_h: 1280, entries: e, count: 1 };
        assert_eq!(focused_app_win(&snap, &SELF_WINS), None);

        // 焦点窗在 count 截断外 = 不参选（表序前 count 项才有效）。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(3, gp::WM_WIN_F_VISIBLE);
        e[1] = app_entry(4, gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED);
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 800, disp_h: 1280, entries: e, count: 1 };
        assert_eq!(focused_app_win(&snap, &SELF_WINS), None);
    }

    /// home 批目标筛真值表：只收可见 ∧ 非自窗 ∧ LAYER_APP——桌面/状态
    /// 栏自窗、隐藏应用窗、SYSTEM 层窗皆不在批内；快照表序保序出批；
    /// 空表批空。
    #[test]
    fn home_batch_targets_visible_foreign_apps_only() {
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(SELF_WINS[1], gp::WM_WIN_F_VISIBLE); // 桌面自窗——排除
        e[1] = app_entry(3, gp::WM_WIN_F_VISIBLE); // 可见应用窗——收
        e[2] = app_entry(4, 0); // 隐藏应用窗——不收
        e[3] = gp::WmEntry {
            layer: gp::WM_LAYER_SYSTEM,
            flags: gp::WM_WIN_F_VISIBLE,
            ..app_entry(SELF_WINS[0], 0)
        }; // 状态栏窗（SYSTEM 层）——排除
        e[4] = app_entry(5, gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED); // 焦点应用窗——收
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 800, disp_h: 1280, entries: e, count: 5 };
        let got = home_destroy_ids(&snap, &SELF_WINS).collect::<Vec<u16>>();
        assert_eq!(got, vec![3, 5]);

        // 空表（count=0 = 合法空表）——批空（零成收零 wake 面）。
        let snap = gp::WmSnapshot {
            version: 1,
            disp_w: 800,
            disp_h: 1280,
            entries: [app_entry(0, 0); gp::WM_WINDOWS_MAX],
            count: 0,
        };
        assert_eq!(home_destroy_ids(&snap, &SELF_WINS).count(), 0);
    }

    /// 覆盖面判据真值表（2026-11-20 事故刀）：焦点非自 APP 窗优先；
    /// 无焦点回退表序首可见非自 APP 窗；仅自窗（桌面常态）/隐藏应用窗/
    /// SYSTEM 层窗 = None（未覆盖，点击回落原 spawn 面）。
    #[test]
    fn covering_app_win_truth_table() {
        // 焦点应用窗在场（wmd CREATE 接焦的覆盖常态）= 收它。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(SELF_WINS[0], gp::WM_WIN_F_VISIBLE); // 状态栏自窗
        e[1] = app_entry(SELF_WINS[1], gp::WM_WIN_F_VISIBLE); // 桌面自窗
        e[2] = app_entry(3, gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED); // 覆盖者
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 480, disp_h: 1000, entries: e, count: 3 };
        assert_eq!(covering_app_win(&snap, &SELF_WINS), Some(3));

        // 无焦点（wmd refocus 已落回桌面自窗）：回退表序首可见非自 APP 窗。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(SELF_WINS[1], gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED);
        e[1] = app_entry(4, gp::WM_WIN_F_VISIBLE);
        e[2] = app_entry(5, gp::WM_WIN_F_VISIBLE);
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 480, disp_h: 1000, entries: e, count: 3 };
        assert_eq!(covering_app_win(&snap, &SELF_WINS), Some(4));

        // 桌面常态（仅双自窗）= None——正常点击面零扰动（boot A 冒烟单击
        // APPDEMO 前的表态即此形）。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(SELF_WINS[0], gp::WM_WIN_F_VISIBLE);
        e[1] = app_entry(SELF_WINS[1], gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED);
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 480, disp_h: 1000, entries: e, count: 2 };
        assert_eq!(covering_app_win(&snap, &SELF_WINS), None);

        // 隐藏应用窗不算覆盖（不可见即不挡命中语义）。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(SELF_WINS[1], gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED);
        e[1] = app_entry(3, 0);
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 480, disp_h: 1000, entries: e, count: 2 };
        assert_eq!(covering_app_win(&snap, &SELF_WINS), None);

        // SYSTEM 层窗不参与覆盖判定；count 截断外不读。
        let mut e = [app_entry(0, 0); gp::WM_WINDOWS_MAX];
        e[0] = app_entry(SELF_WINS[1], gp::WM_WIN_F_VISIBLE);
        e[1] = gp::WmEntry {
            layer: gp::WM_LAYER_SYSTEM,
            flags: gp::WM_WIN_F_VISIBLE | gp::WM_WIN_F_FOCUSED,
            ..app_entry(9, 0)
        };
        e[2] = app_entry(7, gp::WM_WIN_F_VISIBLE); // count 截断外
        let snap =
            gp::WmSnapshot { version: 1, disp_w: 480, disp_h: 1000, entries: e, count: 2 };
        assert_eq!(covering_app_win(&snap, &SELF_WINS), None);
    }
}
