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
//!     槽命中 → APP_SPAWN；pager.on_msg → 翻页 rehang）；键盘帧 →
//!     on_key（keynav 重组 ESC 序列——桌面态上/下键落 pager 相邻页）
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
//!
//! **状态栏双指示灯（塞班式，2026-11-22）**：右上角两枚 Text 空槽（建树
//! 零宽视觉缺席，scene::build_status_bar 预留组宽——右组成员位置恒稳）。
//! ①加载灯：点击 spawn 只置 `pending_spawn` + 点灯（"*"），本拍 render
//! 出帧后**下一拍 on_head 首位真发** APP_SPAWN——阻塞 RPC 期间屏上灯
//! 常亮（单线程泵被 RPC 楔住无法翻帧，闪烁不可达——最小改动形取常亮），
//! ack 归来即熄（`spawn_by_id_inner` 返回后清除）。②运行呼吸灯：spawn
//! 成功回执点火，此后 1Hz 表查（TABLE_GET——可见非自 APP 窗在场才续亮）
//! + 翻相位（"o"↔""——1fps 微脉动，每拍一醒一微 damage 帧）；表空/查败
//! 即熄。APP_CHILDREN 真值 launcher 无协议面不可见，以 wm 表可见窗近似
//!（=「app 画面在场」的用户可见真值）。

use berry_appshell as appshell;
use berry_appproto as ap;
use berry_guiproto as gp;
use berry_libc::io::{app_cli_h, self_vmar_h, wm_cli_h};
use berry_sys::{berry_clock_read, berry_power_off, Handle, CLOCK_REALTIME};
use berry_ui::desc::{IconProps, MsgBuf, Props};
use berry_ui::hit::PtrEvent;
use berry_ui::{Msg, NodeId, UiMain};

use appshell::rpc::{fail, rpc};

use crate::desktop::{self, Desktop, NavNodes, POWER_ARM_WINDOW_NS};
use crate::icons;
use crate::keynav;
use crate::settings_io;
use crate::syspages;

/// 整帧清屏底色（gfxdemo 同值——FULL 帧先铺底再回放 DisplayList）。
const CLEAR_BG: (u8, u8, u8) = (0x10, 0x18, 0x28);

/// 自测 swipe 收敛帽（超时 = eprintln 诊断 + 放弃——PAGE 缺失由 smoke
/// 兜底 loud；不 fail 挂死：桌面常驻义务优先于自测件）。
const SETTLE_CAP_NS: u64 = 5 * 1_000_000_000;

/// 双击幂等窗帽（`LoopHost::spawn_dedup` 兜底面，总审计 input-chain P2
/// 「双击闪现」2026-11-22）：覆盖 referee 250ms 双击窗之外的人手慢二击
/// + 应用冷启动建窗竞速（第二击先到、应用窗未上 wm 表——此形二次
/// APP_SPAWN 撞 svcmgr 同 id 回收 = 开→关→重开闪现）。窗内同格第二击
/// 视为重复吞掉；窗外视为蓄意重开。
const SPAWN_DEDUP_WINDOW_NS: u64 = 2 * 1_000_000_000;

/// 加载指示灯亮态字（塞班式状态栏右上角小图标——v1 最小改动形：Text
/// 翻字而非新 widget；空文本零宽 = 视觉缺席。ASCII 字形域）。
const BUSY_ON: &[u8] = b"*";
/// 运行呼吸灯亮态字（同上——1Hz 相位翻转面）。
const BREATH_ON: &[u8] = b"o";
/// 呼吸拍距（任务书「低帧率 1fps 脉动」：每拍一醒 + 一次表查 + 至多一
/// 微 damage 帧——角标字格级开销）。
const BREATH_BEAT_NS: u64 = 1_000_000_000;

/// 前台态表查拍距（R8 调查刀）：窗口化应用的退场/回收检测——wmd 僵尸窗
/// 对账把亡故属主的窗摘表，表空即离场。无窗应用（现行五个 [ui] app 皆
/// 以 surface 0 兼容档提交、从不 CREATE）表恒空，本拍永不见窗，前台态只
/// 由导航键解除（设计边界，见调查报告）。
const FG_BEAT_NS: u64 = 1_000_000_000;
/// 带内改文节点暂存帽（busy/breath/clock/power 四枚——一拍内至多四笔）。
const BAND_DIRTY_MAX: usize = 4;

/// 应用前台态（R8 调查刀）：自上次 APP_SPAWN 真拉起成功起算；
/// `win_seen` = 表查至少见过一枚可见非自 APP 窗（窗口化应用）——见过之后
/// 表空 = 应用退场或被回收，启动器回桌面；从未见过 = 慢启动或无窗应用，
/// 前台态保持到导航键。
#[derive(Clone, Copy)]
struct AppFg {
    win_seen: bool,
}

/// spawn 回执三态（flush_pending_spawn 消费面）：真拉起 / 已活聚焦（err=3
/// ——像素不在屏，前台态不置）/ 失败。
enum SpawnOut {
    Spawned,
    AliveFocus,
    Failed,
}

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

/// 双击幂等窗纯谓词（测试面——`LoopHost::spawn_dedup` 消费面）：窗内
/// （t_now 距 ok 回执 < [`SPAWN_DEDUP_WINDOW_NS`]）且命中格 id 与记账
/// id 同一 → true（吞第二击）；无记账/窗外/异格/命中非格面 = false
/// （点击落原面——非格命中不受窗影响）。
fn spawn_dedup_hit(dedup: Option<(&RecentsApp, u64)>, hit: Option<&[u8]>, t_now: u64) -> bool {
    let Some((app, ack_ns)) = dedup else {
        return false;
    };
    if t_now >= ack_ns.saturating_add(SPAWN_DEDUP_WINDOW_NS) {
        return false; // 窗外：第二击视为蓄意重开
    }
    match hit {
        Some(id) => id == &app.id[..app.id_len as usize],
        None => false, // 命中非格面（壁纸/容器白区/导航余白）
    }
}

/// 点击命中格 → 注册表 id（on_click 同映射：cell/label/icon 三节点皆归
/// 本格——最深叶命中面；桌面网格 + dock 同表）。幂等窗查询面：未命中
/// 格/占位格/注册表外 = None（该击不受窗影响，落原面）。
fn click_cell_app_id(node: NodeId, desktop: &Desktop) -> Option<&'static [u8]> {
    let slot = desktop
        .slots
        .iter()
        .flatten()
        .chain(desktop.dock.iter())
        .find(|s| s.cell == node || s.label_node == node || s.icon == Some(node))?;
    let idx = slot.app?;
    icons::registry().get(idx).map(|e| e.id_bytes())
}

/// 待发 spawn（加载指示灯延迟一拍面——见 `LoopHost::pending_spawn` 注）。
#[derive(Clone, Copy)]
struct PendingSpawn {
    /// 目标 app（id 二进制拷贝——注册表条目生命周期不随帧循环）。
    app: RecentsApp,
    /// 多任务键重拉形（true = 失败打 recents respawn failed 诊断、成功
    /// 不记 recents/dedup——原 spawn_by_id 语义面）。
    from_recents: bool,
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
    /// 键盘序列重组态（on_key 覆写面——keynav 三态：0 基态 / 1 已收
    /// ESC / 2 已收 ESC `[`；inputd KeyPump 箭头出向 = ESC 序列三帧
    /// 单字节连发，消费侧按帧重组才恢复「一次箭头 = 一个动作」）。
    kseq: u8,
    /// 双击幂等窗（总审计 input-chain P2「双击闪现」2026-11-22 兜底面）：
    /// 最近一次点击路 spawn 成功的 app + 其 ok 回执时刻（flush 成功支路
    /// 落账；多任务键重拉不入账）。同格 Click 在 [`SPAWN_DEDUP_WINDOW_NS`]
    /// 窗内 → 吞第二击（不落 covered_tap 收盖〔开→关闪现〕、不二次
    /// APP_SPAWN 撞 svcmgr 同 id 回收〔开→关→重开闪现〕）。back/home
    /// 一击即撤窗（退回手势 = 蓄意离场声明，其后点图标 = 蓄意重开）。
    /// 正解 = already-alive 路承接活实例第二击（svcmgr 重开=聚焦刀），
    /// 本窗只兜其未接住的残余竞速（回执仍为 ok 形时）。
    spawn_dedup: Option<(RecentsApp, u64)>,
    /// 待发 spawn（加载指示灯刀）：点击/多任务键只置位 + 点灯，本拍
    /// render 出帧（灯已呈现）后**下一拍 on_head 首位真发** APP_SPAWN
    /// ——阻塞 RPC 期间屏上灯常亮，ack 归来即熄（flush_pending_spawn =
    /// `spawn_by_id_inner` 返回后清除）。单槽：同拍连击后到者胜（人手
    /// 双击同拍不同格不可达；同格由幂等窗先吞）。
    pending_spawn: Option<PendingSpawn>,
    /// 运行呼吸灯在席（塞班式）：spawn 成功回执点火；1Hz 表查（on_head
    /// 呼吸拍）无可见非自 APP 窗即熄（back/home/covered 收盖路径自然
    /// 反映进表查——≤1s 熄灯滞后可容）。APP_CHILDREN 真值 launcher 无
    /// 协议面不可见，以 wm 表可见窗近似 =「app 画面在场」的用户可见
    /// 真值（v1 共享 fb 残影窗亦如实亮——像素在场灯在场）。
    breath_on: bool,
    /// 呼吸灯当前相位（true = 亮拍）。
    breath_lit: bool,
    /// 下次呼吸拍时刻（extra_deadline 准时醒面）。
    breath_next_ns: u64,
    /// 应用前台态（R8 调查刀）：Some 期间 on_tick 丢弃一切 damage、只还回
    /// 带内改文节点矩形——状态栏/导航条照常刷新，应用内容区不被重绘覆盖
    /// （v1 无合成器、共享 fb，启动器整帧 = 盖掉应用画面）。
    app_fg: Option<AppFg>,
    /// 下次前台态表查拍（extra_deadline 准时醒面）。
    fg_next_ns: u64,
    /// 本拍带内改文节点：set_text/set_props 的 A 面路径（s7 未开）必
    /// mark_ancestors_dirty 到根，render_frame 内 relayout 的 place 把根的
    /// 整视口新旧界加进 damage（layout.rs）——前台态下 drain 后只 invalidate
    /// 这些节点。None 槽 = 空；非前台态不记账。复审 M1：登记成功的槽
    /// 保留到 on_presented 清账（泵节流拍不消费 damage，见 flush_band）。
    band_dirty: [Option<NodeId>; BAND_DIRTY_MAX],
    /// 带内帧在途（flush_band 登记成功置位，on_presented / leave_fg 清位）。
    band_pending: bool,
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
        kseq: keynav::KSEQ_BASE,
        spawn_dedup: None,
        pending_spawn: None,
        breath_on: false,
        breath_lit: false,
        breath_next_ns: 0,
        app_fg: None,
        fg_next_ns: 0,
        band_dirty: [None; BAND_DIRTY_MAX],
        band_pending: false,
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
        // —— 待发 spawn 冲刷（加载指示灯刀，首位于本拍）：点击拍置位 +
        //    点灯后本拍 render 已把灯呈现（SUBMIT/FRAME_CALLBACK 过链）
        //    ——此刻真发 APP_SPAWN，阻塞 RPC 期间屏上灯常亮，ack 归来
        //    即熄（spawn_by_id_inner 返回后清除——任务书语义）。
        self.flush_pending_spawn(ui, t_now);

        // —— 前台态表查拍（R8 调查刀，1Hz）：窗口化应用见过窗后表空 = 退场
        //    或被回收（wmd 僵尸窗对账摘表）→ 回桌面整帧重画；未见窗（慢
        //    启动/无窗应用）保持前台态。无窗应用本拍恒 false——前台态只由
        //    导航键解除（设计边界，见调查报告）。
        //    复审①：只有正常快照才参与退场裁定——RPC 查败 / 应答畸形
        //    （三态 None）不当作表空，win_seen 为真的窗口化应用不因一次坏
        //    应答被误清前台态（拍距已推进，下一拍重查）。
        if self.app_fg.is_some() && t_now >= self.fg_next_ns {
            self.fg_next_ns = t_now.saturating_add(FG_BEAT_NS);
            if let Some(covered) = self.app_cover_state() {
                let drop_fg = match self.app_fg.as_mut() {
                    Some(fg) => {
                        if covered {
                            fg.win_seen = true;
                            false
                        } else {
                            fg.win_seen
                        }
                    }
                    None => false,
                };
                if drop_fg {
                    self.leave_fg(ui, "table", t_now);
                }
            }
        }

        // —— 运行呼吸拍（塞班式呼吸灯，1Hz 微脉动）：呼吸窗内每拍先表查
        //    活窗（TABLE_GET 复用 covered_by_app——可见非自 APP 窗在场才
        //    续亮；表空/查败 = 熄灯——查败 fail-open 向熄灭侧保守），在场
        //    则翻相位（"o"↔""）+ wake 出一帧微 damage（角标字格）。拍距
        //    由 extra_deadline 保证准时醒（1Hz 一醒——Idle 10s 帽不吞拍）。
        //    R8：前台态下翻字只还回灯格矩形（mark_band）——A 面 set_text
        //    本身会把整视口带进 damage。
        if self.breath_on && t_now >= self.breath_next_ns {
            self.breath_next_ns = t_now.saturating_add(BREATH_BEAT_NS);
            if self.covered_by_app() {
                self.breath_lit = !self.breath_lit;
                set_breath_face(ui, self.desktop.breath, self.breath_lit);
                self.mark_band(self.desktop.breath);
                self.wake_unless_fg(ui, t_now);
            } else {
                self.breath_on = false;
                if self.breath_lit {
                    self.breath_lit = false;
                    set_breath_face(ui, self.desktop.breath, false);
                    self.mark_band(self.desktop.breath);
                }
            }
        }

        // —— 波一刀 D：armed 窗过期复位（视觉回常态面 + 清窗——wait
        //    deadline 已含本窗，3s 准时醒；复位 set_props 自标 damage
        //    出帧，无需另 wake——R8：前台态下记带内节点，on_tick 收口）。
        if let Some(dl) = self.power_armed {
            if t_now > dl {
                set_power_face(ui, &self.desktop.nav, false);
                self.mark_band(self.desktop.nav.power);
                self.power_armed = None;
            }
        }

        // —— 波一刀 B：状态栏真时钟续摆——稳态每 10s（IDLE 帽）醒一回，
        //    读 CLOCK_REALTIME；分钟字变才动笔（set_text 等长换文 = 纯
        //    PAINT；a11y label 同步 in-place 覆写）+ wake（Idle→Run 保
        //    产出帧——inject_swipe 同款正统路径）。分钟界最迟 10s 内
        //    换字（分钟级摆字语境足够）。R8：前台态下只还回时钟字格矩形
        //    （mark_band + wake_unless_fg），不再整屏重画盖掉应用画面。
        let mut rt = 0u64;
        if berry_clock_read(CLOCK_REALTIME, &mut rt) == 0 {
            let hm = appshell::scene::hhmm_utc(rt);
            if self.last_clock != Some(hm) {
                if let Err(e) = ui.set_text(self.desktop.clock, &hm) {
                    fail(&format!("clock set_text err={e:?}"));
                }
                self.mark_band(self.desktop.clock);
                let mut label = *b"00:00 utc";
                label[..5].copy_from_slice(&hm);
                // a11y in-place 覆写（同节点不占新槽）；Err 非关键面容忍。
                let _ = ui.set_a11y(self.desktop.clock, berry_ui::ROLE_TEXT, &label);
                self.wake_unless_fg(ui, appshell::rpc::now());
                self.last_clock = Some(hm);
            }
        }
    }

    fn on_msgs(&mut self, ui: &mut UiMain, msgs: &MsgBuf, t_now: u64) {
        for m in msgs.iter() {
            match m {
                Msg::Click { node } => {
                    // 双击幂等窗撤销（退回手势）：back/home 命中一击即撤
                    // ——其后再点图标 = 蓄意重开，窗不得吞（撤窗与 DESTROY
                    // 成败无关：导航键命中本身即离场意图声明）。
                    if *node == self.desktop.nav.back || *node == self.desktop.nav.home {
                        self.spawn_dedup = None;
                    }
                    // 五键导航先判（含 BottomTabs 三页位——命中 = 消费，
                    // 不落槽 spawn 面）。导航键是覆盖态的逃生面，恒最先。
                    if !self.on_nav_click(ui, *node, t_now) {
                        // R8 调查刀：前台态下桌面命中面不可达——非导航键点击
                        // 整击吞掉（不落 spawn 面、不翻 sys 页；按压反馈与空集
                        // 唤醒的 damage 由 on_tick 的 flush_band 丢弃）。原
                        // on_covered_tap 收盖门只在冒烟态计数（spawn_ok 恒 0
                        // 短路）——正常 boot 此前会把内容区点击误读成 spawn
                        // 底层格应用。导航键是覆盖态的逃生面，已在上方先判。
                        if self.app_fg.is_some() {
                            continue;
                        }
                        // 双击幂等窗兜底（吞第二击，见 LoopHost::spawn_dedup
                        // 字段注）：同格命中且在上次 spawn ok 回执窗内 →
                        // 整击吞掉（先于 covered_tap——收盖恰是开→关闪现
                        // 的第一路）。正解 = already-alive 路，本窗只兜其
                        // 未接住的残余竞速。
                        let dedup_hit = spawn_dedup_hit(
                            self.spawn_dedup.as_ref().map(|(a, t)| (a, *t)),
                            click_cell_app_id(*node, &self.desktop),
                            t_now,
                        );
                        // 覆盖态点击门（2026-11-20 桌面点击无反应事故刀）：
                        // v1 无合成器 + 输入单订阅者（事件恒全投 launcher），
                        // 应用窗开盖后桌面命中面仍活着——连点被误读成连环
                        // spawn，svcmgr APP_SPAWN_LIVE_MAX=4 槽被同 app 烧尽
                        // 后一切 spawn 静默拒「app slots」=「点击无反应」。
                        // 覆盖态点击改收盖（back 键同款 DESTROY + wake 重画）。
                        if !dedup_hit
                            && (self.spawn_ok == 0
                                || !on_covered_tap(ui, self.hw, &self.self_wins, t_now))
                        {
                            if !self.on_sys_click(ui, *node, t_now) {
                                self.on_click(ui, *node, t_now);
                            }
                        }
                    }
                }
                Msg::LongPress { node } => {
                    // 导航键无长按语义：慢按（引擎按 wire 时戳判长按、不产
                    // Click）仍按一击处理。原落 `_` 臂静默无动作——真机按
                    // 返回稍久即无反应，前台态下其后图标点击又被整击吞掉。
                    // 幂等窗撤销与 Click 臂同律（仅 back/home）；on_nav_click
                    // 对非导航/非 tab 节点返回 false——图标格长按维持不拉起
                    //（长按语义留给将来的上下文菜单）。
                    if *node == self.desktop.nav.back || *node == self.desktop.nav.home {
                        self.spawn_dedup = None;
                    }
                    let _ = self.on_nav_click(ui, *node, t_now);
                }
                _ => {
                    // pager.on_msg true = 翻页目标变更 → a11y/press 重挂
                    //（幂等；Err = 容量违例 fail 收口）。sys 页在树时桌面
                    // 被整面盖住——隐藏 pager 不吃翻页手势（可见性真相面；
                    // Msg 不劫持，只是不喂）。R8：前台态同律不吃（应用画面
                    // 在上，翻页只会在其下换页）。
                    if self.app_fg.is_none()
                        && self.desktop.tabs.cur == 0
                        && self.desktop.pager.on_msg(ui, m)
                    {
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
        // R8 调查刀：前台态帧收口——render 前最后一位（pump 序 on_head →
        // pump_input_at → on_msgs → on_tick → render_frame），此时本拍全部
        // damage 已就位：带内改文的整视口、翻页跟手包络、按压反馈格、空集
        // 唤醒的 FULL——内容区一律丢弃，只还回带内改文节点。
        if self.app_fg.is_some() {
            self.flush_band(ui, t_now);
        }
    }

    fn on_presented(&mut self, ui: &mut UiMain, presented: u64) {
        // 复审 M1：带内帧已随本次呈现上屏——清账（flush_band 头注）。只认
        // 「登记过且尚未呈现」的账：band_pending 由 flush_band 置位，此处
        // 清位；leave_fg 另路同清。
        if self.band_pending {
            self.band_dirty = [None; BAND_DIRTY_MAX];
            self.band_pending = false;
        }
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
        // 刀 D armed 窗 + 呼吸拍 + 前台态表查拍纳入等待帽：三者取最早——
        // armed 3s 准时醒过期复位（不醒 = 红键滞留 IDLE 帽 10s，确认语义
        // 受损）；呼吸拍 1Hz 准时醒（不醒 = 灯滞留 Idle 帽，脉动语义受损）；
        // 前台态表查拍 1Hz 准时醒（R8：窗口化应用退场检测不拖到 10s 帽）。
        let breath = if self.breath_on { Some(self.breath_next_ns) } else { None };
        let fg = if self.app_fg.is_some() { Some(self.fg_next_ns) } else { None };
        [self.power_armed, breath, fg].into_iter().flatten().min()
    }

    /// 键盘消费面（输入链族批 2026-11-21，总审计 input-chain P1「键盘
    /// 全链死三连」第三连——635dc0e62 曾落、14a9b16fa G0G1 sweep 连坐
    /// 回退后本批重建；Router/焦点路由面不含——归 B-infra 刀自理）：
    /// 覆写 on_key 接 [`keynav::seq_step`] 重组 ESC[A/B 序列，上/下键
    /// 落 pager 相邻页。三重门（2026-11-20 键盘消费刀原设计）：
    /// ① 重组先行——序列态恒推进（门在动作后判：tab/覆盖起落不冻结
    /// 序列，跨界残帧不悬置成跨态死结）；
    /// ② 非桌面态吞——sys 页在树（`tabs.cur != 0`，on_msgs 喂 pager
    /// 同律：桌面被 sys 页不透明底整面盖住）或应用窗覆盖（翻页重画
    /// 会把桌面画到 app 像素上面——v1 无合成器共享 fb；查表
    /// fail-open）；
    /// ③ 边界页无操作——page 0 上键饱和停驻、末页下键越界拒（统一
    /// page_node 存在性预判，免空 rehang 空重画）。
    /// 翻页收尾 = snap_to + rehang_view（on_nav_click snap_page 同款）；
    /// wake 归 pump 键帧批后统一 wake（on_key continue 不 push_pointer，
    /// 该 wake 是键盘路径唯一起振面）——本覆写零 wake 笔零打印（翻页
    /// 非冒烟断言件，PAGE ok 是 swipe 自测标记）。
    fn on_key(&mut self, ui: &mut UiMain, code: u8, _ts_ns: u64) {
        let Some(nav) = keynav::seq_step(&mut self.kseq, code) else {
            return; // 吞/续态（keynav 吞面纪律——表外/失序/基态字节全静默）
        };
        if self.desktop.tabs.cur != 0 || self.app_fg.is_some() || self.covered_by_app() {
            return; // 非桌面态：sys 页整面盖桌面 / app 前台态或窗覆盖——键不达 pager
        }
        let p = self.desktop.pager.page();
        let target = match nav {
            keynav::KeyNav::PagePrev => p.saturating_sub(1),
            keynav::KeyNav::PageNext => p.saturating_add(1),
        };
        if target == p || self.desktop.pager.page_node(target).is_none() {
            return; // 边界页：饱和停驻/越界拒——无操作
        }
        self.desktop.pager.snap_to(ui, target);
        if let Err(e) = desktop::rehang_view(ui, &mut self.desktop) {
            fail(&format!("key rehang view err={e:?}"));
        }
    }
}

impl LoopHost {
    /// 覆盖态判据（on_key 门查询面——on_covered_tap 表查询的同款裁剪，
    /// 只判在场不收盖）：TABLE_GET 快照内可见非自 APP 窗在场 = 桌面被
    /// 应用画面覆盖。查表失败 fail-open（false——on_covered_tap 同律：
    /// 判据病态不连坐桌面常行为）。
    fn covered_by_app(&self) -> bool {
        match self.app_cover_state() {
            Some(covered) => covered,
            None => false, // 查败/畸形 fail-open（原两态语义不变）
        }
    }

    /// 覆盖态三态查询面（R8 前台态表查拍消费——复审①）：`Some(true)` =
    /// 快照正常且有可见非自 APP 窗；`Some(false)` = 快照正常且确无覆盖窗；
    /// `None` = RPC 查败或应答畸形（parse None/Err）——与「表空」严格
    /// 分离，前台态据此不清态。两态 `covered_by_app` 即本面 fail-open
    /// 到 false。
    fn app_cover_state(&self) -> Option<bool> {
        let mut req = [0u8; 256];
        let mut rep = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let m = gp::encode_wm_table_get_req(&mut req);
        let (n, _) = rpc(self.hw, &req[..m], &[], &mut rep, &mut rhs);
        match gp::parse_wm_snapshot_rep(&rep[..n]) {
            Some(Ok(snap)) => Some(covering_app_win(&snap, &self.self_wins).is_some()),
            _ => None,
        }
    }

    /// 带内改文记账（R8 调查刀）：仅前台态记账——on_tick 的 flush_band 只
    /// 还回这些节点的矩形。非前台态 A 面 set_text 的整视口 damage 由 render
    /// 自然消费（本就整屏重画），无需记账。同节点去重，帽满静默丢弃。
    fn mark_band(&mut self, node: NodeId) {
        if self.app_fg.is_none() {
            return;
        }
        for s in &self.band_dirty {
            if *s == Some(node) {
                return;
            }
        }
        if let Some(s) = self.band_dirty.iter_mut().find(|s| s.is_none()) {
            *s = Some(node);
        }
    }

    /// 非前台态才 wake：前台态的起振归 on_tick 的 flush_band（damage 须先
    /// 经 drain 收口，否则空集 wake 会置 FULL）。
    fn wake_unless_fg(&self, ui: &mut UiMain, t_now: u64) {
        if self.app_fg.is_none() {
            ui.wake(t_now);
        }
    }

    /// 前台态收口（导航键回桌面 / 窗口化应用退场）：清态 + 丢弃残 damage
    /// + 空集 wake 置 FULL——v1 无合成器、共享 fb，应用残留像素靠整帧覆盖
    ///（home 批/back 收窗同款正统重画路）。
    /// `reason` = 触发源一词（table / back / recents / home），随清态打
    /// 恰一行 `launcher: fg- <reason>`——与 flush_pending_spawn 的 `fg+`
    /// 成对，是宿主门可等的「前台态已清」握手行（每次收口恒一行，非逐拍）。
    fn leave_fg(&mut self, ui: &mut UiMain, reason: &str, t_now: u64) {
        println!("launcher: fg- {reason}");
        self.app_fg = None;
        self.band_dirty = [None; BAND_DIRTY_MAX];
        self.band_pending = false;
        let _ = ui.drain_damage();
        ui.wake(t_now);
    }

    /// 前台态帧收口（on_tick 末位、render 前最后一位）：A 面 set_text/
    /// set_props 经 mark_ancestors_dirty 把 layout_dirty 推到根，render_frame
    /// 内 relayout 的 place 会把根的整视口新旧界加进 damage（layout.rs place
    /// ——等价整屏重绘，盖掉应用内容）。此处先 `ui.layout()` 就地消费 dirty
    /// 位（render 内 relayout 即全缓存命中早退，不再加根界），再 drain 丢弃
    /// 一切累积 damage（整视口、翻页跟手包络、按压反馈格、空集唤醒的 FULL），
    /// 只 invalidate 带内改文节点；damage 非空 wake 不置 FULL（lib.rs wake
    /// 契约）。无带内改动不起振——damage 空集 render_frame 跳过帧零 SUBMIT
    ///（相位机 next_phase 信号全假即 Idle，无自旋）。复审②：invalidate
    /// 登记失败（坏节点 Err）的槽剔除——不计入「有改动」，否则 damage 仍
    /// 空、wake 置 FULL 整屏重画盖掉应用画面。复审 M1：登记成功的节点
    /// **不 take**，留在 band_dirty 逐拍重登记直到该帧真被呈现（on_presented
    /// 清账）——泵的 B2 节流拍（pump.rs pace_beat：距上次落画 <33.3 ms ∧
    /// 本拍无输入 ∧ 无动画 ∧ 视口 >1.5M px）整拍不进 render_frame，damage
    /// 不被消费；若此处已清空，下一拍 flush_band 的 drain 会把滞留的带内
    /// 矩形丢掉且不再 invalidate——熄灯字写进节点却永不上屏，直到 leave_fg
    /// 的 FULL（TCG 一帧 100-400 ms 不触发节流拍，真机 / 加速环境每次
    /// spawn 必中）。
    fn flush_band(&mut self, ui: &mut UiMain, t_now: u64) {
        let _ = ui.layout();
        let _ = ui.drain_damage();
        let mut any = false;
        for slot in self.band_dirty.iter_mut() {
            if let Some(node) = *slot {
                if ui.invalidate(node).is_err() {
                    *slot = None;
                } else {
                    any = true;
                }
            }
        }
        if any {
            self.band_pending = true;
            ui.wake(t_now);
        }
    }

    /// 点击路入队（桌面格/dock/应用列表行共用）：置 `pending_spawn` +
    /// 点亮加载灯 + wake——本拍 render 出帧后下一拍 on_head 真发（见
    /// `pending_spawn` 字段注）。
    fn queue_spawn(&mut self, ui: &mut UiMain, id: &[u8], t_now: u64) {
        let mut buf = [0u8; 24];
        let n = id.len().min(24);
        buf[..n].copy_from_slice(&id[..n]);
        self.queue_spawn_app(ui, RecentsApp { id: buf, id_len: n as u8 }, false, t_now);
    }

    /// 入队本体（多任务键重拉形直携 RecentsApp 走此口）。
    fn queue_spawn_app(
        &mut self,
        ui: &mut UiMain,
        app: RecentsApp,
        from_recents: bool,
        t_now: u64,
    ) {
        self.pending_spawn = Some(PendingSpawn { app, from_recents });
        set_busy_face(ui, self.desktop.busy, true);
        ui.wake(t_now);
    }

    /// 待发 spawn 冲刷（on_head 首位——灯已随上拍帧呈现）：真发
    /// APP_SPAWN（阻塞 RPC 期间屏上灯常亮），ack 归来即熄灯 + 记账 +
    /// 运行呼吸灯点火；多任务键重拉形失败打原 `spawn_by_id` 诊断、成功
    /// 不记 recents/dedup（take 已消费——原语义保持）。err=3 已活形返回
    /// true（聚焦切回 = 目标 app 已在前台）——记账成立、呼吸灯续亮。
    fn flush_pending_spawn(&mut self, ui: &mut UiMain, t_now: u64) {
        let Some(p) = self.pending_spawn.take() else { return };
        let out = spawn_by_id_inner(
            &p.app.id[..p.app.id_len as usize],
            self.smoke,
            &mut self.spawn_ok,
            self.self_wins,
        );
        // R8 调查刀：真拉起成功即置前台态（先于下方熄灯改文——mark_band 只
        // 在前台态记账，熄灯矩形才能以带内帧还回）。已活聚焦形不置：共享
        // fb 下 FOCUS 不重渲，应用像素并不在屏，桌面须保持可交互。
        if matches!(out, SpawnOut::Spawned) {
            self.app_fg = Some(AppFg { win_seen: false });
            // 前台态进入握手行（每次真拉起恰一行，非逐拍）：宿主门据此
            // 知道阻塞 spawn 已返回、前台态在席；与 leave_fg 的 `fg-` 成对。
            println!("launcher: fg+ spawned");
            self.fg_next_ns = t_now.saturating_add(FG_BEAT_NS);
            // 复审③：翻页手势收口——进入前台态后 pager 不再吃 Msg（on_msgs
            // 门），拖拽中的手势收不到 DragEnd 会把 track 悬在跟手偏移上
            //（snap 已被 DragStart 清空，无回弹）。同页 snap_to 直落：清
            // dragging / 中止在飞吸附 / track 落回当前页整位；非拖拽态
            // 调用为幂等重落（damage 由本拍 on_tick 的 flush_band 丢弃，
            // 桌面前台态期间零外溢）。SwipePager 无拖拽态公开读面，故无
            // 条件调用。
            self.desktop.pager.snap_to(ui, self.desktop.pager.page());
        }
        // 加载窗关闭（spawn_by_id_inner 已返回）。
        set_busy_face(ui, self.desktop.busy, false);
        self.mark_band(self.desktop.busy);
        if matches!(out, SpawnOut::Spawned | SpawnOut::AliveFocus) {
            // 运行呼吸灯点火（近似语义见 breath_on 字段注；首拍在一拍后）。
            self.breath_on = true;
            self.breath_next_ns = t_now.saturating_add(BREATH_BEAT_NS);
            if !p.from_recents {
                // 多任务键记忆 + 双击幂等窗起算（rpc 阻塞同步归——now()
                // 即 ok 回执时刻）。
                self.recents_app = Some(p.app);
                self.spawn_dedup = Some((p.app, appshell::rpc::now()));
            }
        } else if p.from_recents {
            eprintln!("launcher: recents respawn failed");
        }
        self.wake_unless_fg(ui, t_now);
    }
}

/// 灯熄灭态字（R8 调查刀）：单空格而非空文本——一格宽使节点矩形恒有
/// 面积，前台态下 `invalidate(灯)` 才能把亮态字形擦掉（空文本零宽 =
/// invalidate 零面积 no-op → damage 空 → wake 置 FULL 整屏重画）。空格
/// 字形空白、bg None，视觉与缺席等价；右组已为两灯各预留一字格
///（scene.rs lamp_reserve），不外溢组界、不移动 battery/clock/UTC 位置。
const LAMP_OFF: &[u8] = b" ";

/// 加载指示灯翻字（单空格 = 熄；单字 "*" = 亮）。Err 非关键面
/// eprintln 容忍（set_power_face 同律——指示灯不拖累 spawn 链）。
fn set_busy_face(ui: &mut UiMain, node: NodeId, on: bool) {
    if let Err(e) = ui.set_text(node, if on { BUSY_ON } else { LAMP_OFF }) {
        eprintln!("launcher: busy face set err={e:?}");
    }
}

/// 运行呼吸灯翻字（同上——1Hz 相位翻转面）。
fn set_breath_face(ui: &mut UiMain, node: NodeId, lit: bool) {
    if let Err(e) = ui.set_text(node, if lit { BREATH_ON } else { LAMP_OFF }) {
        eprintln!("launcher: breath face set err={e:?}");
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
/// 通道零新输入面；`LoopHost` 方法形——指示灯刀起直取宿主态）。返回
/// true = 已消费（不落槽 spawn）；false = 非导航键（调用方续走 app 槽
/// 命中）。
///
/// - **返回**：波3-件④——应用在焦先收应用（查表→焦点窗 DESTROY→
///   成收 wake 重画→wmd 表序回焦桌面）；焦点命中自窗（K2 排雷：wmd
///   refocus 回落后焦点常驻桌面窗自身）视同无应用在焦，走次级语义
///   ——桌面窗永不被 back 自毁；无应用在焦 = 桌面翻页语义（v1 原路）。
///   launcher/wmd 同步 RPC 无死锁对（guifbd 的 SUBMIT 内禁同步 RPC
///   wmd 才是死锁对——launcher 非 blit 路径）。
/// - **多任务**：重新拉起最近 spawn 的应用（真重拉非死键）；无历史 =
///   静默无操作。入队延迟一拍（加载指示灯刀——灯先行一帧）；失败诊断
///   （recents respawn failed）与不记 recents/dedup 语义归
///   flush_pending_spawn（原 spawn_by_id 面移入）。
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
impl LoopHost {
    fn on_nav_click(&mut self, ui: &mut UiMain, node: NodeId, t_now: u64) -> bool {
        let nav = self.desktop.nav;
        let wm_h = self.hw;
        let self_wins = self.self_wins;
        let snap_page = |ui: &mut UiMain, desktop: &mut Desktop, target: u8| {
            desktop.pager.snap_to(ui, target);
            if let Err(e) = desktop::rehang_view(ui, desktop) {
                fail(&format!("nav rehang view err={e:?}"));
            }
        };
        // 波3-件⑤：系统页 tab 命中（BottomTabs 三页位）——切页优先于
        // 五键导航消费。整树装配（离树/入树/换条/重挂）；Err = 容量违例
        // 非致命收口（桌面常驻义务优先）。
        let tab_nodes = self.desktop.tabs.tab_nodes;
        for (i, &tn) in tab_nodes.iter().enumerate() {
            if i < 3 && node == tn {
                if self.app_fg.is_some() {
                    return true; // R8：前台态下 tab 位在应用画面之下——整击吞掉不切页
                }
                if let Err(e) = desktop::switch_sys_page(ui, &mut self.desktop, i as u8) {
                    eprintln!("launcher: sys page switch err={e:?}");
                }
                return true;
            }
        }
        if node == nav.back {
            // R8 调查刀：前台态下 back 的第一义 = 回桌面——先收前台态并整帧
            // 重画（应用残留像素靠 FULL 覆盖）；其下原分支（焦点应用窗
            // DESTROY / sys 页回桌面 tab / 桌面翻页）照旧执行。
            if self.app_fg.is_some() {
                self.leave_fg(ui, "back", t_now);
            }
            // 波3-件④ + K2 排雷：应用在焦先收应用（查表→焦点窗 DESTROY→
            // 成收 wake 重画→wmd 表序回焦桌面）；焦点命中自窗（wmd
            // refocus 回落后即桌面窗自身）视同无应用在焦走次级语义——
            // 桌面窗永不被 back 自毁；无应用在焦 = 桌面翻页语义（v1 原路）。
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
                } else {
                    // 成收即 wake（home 批/on_covered_tap 同款）：v1 无合成
                    // 器、像素共享 fb，app 残留像素靠本帧 FULL 重画覆盖
                    // （wake 空脏集置 FULL——refocus 只是表态变更非重画，
                    // 不 wake 则 damage 空集零 SUBMIT，桌面残影不可见却
                    // 可交互）；零成收不 wake——表未变不重画，免覆盖仍
                    // 存活窗。
                    ui.wake(t_now);
                }
                let _ = n;
            } else if self.desktop.tabs.cur != 0 {
                // R1：非桌面 tab = 回桌面 tab（sys 页在树先摘——桌面整面回显
                //）；同拍不做页翻（一 back 一义）。
                if let Err(e) = desktop::switch_sys_page(ui, &mut self.desktop, 0) {
                    eprintln!("launcher: back to home tab err={e:?}");
                }
            } else {
                let p = self.desktop.pager.page();
                if p > 0 {
                    snap_page(ui, &mut self.desktop, p - 1);
                }
            }
            true
        } else if node == nav.recents {
            // 多任务 v1：重新拉起最近 spawn 的应用（真重拉非死键）；无历史
            // = 静默无操作（桌面常驻义务）。整体取出再入队——避免 spawn
            // RPC 窗内再借用 desktop。实例仍活 ∧ 窗在场时 svcmgr 回 err=3
            // 已活形——flush 路经 spawn_by_id_inner 走 wmd 焦点面真切换
            // （内容不丢，重开=聚焦刀）；实例亡/窗不在场才真重拉。加载灯
            // 先行一帧（queue_spawn_app），失败诊断归 flush。
            // R8：前台态下多任务键第一义 = 回桌面（收前台态 + 整帧重画），
            // 再入队重拉——加载灯点亮帧即全量帧（前台态已清）。
            if self.app_fg.is_some() {
                self.leave_fg(ui, "recents", t_now);
            }
            if let Some(app) = self.recents_app.take() {
                self.queue_spawn_app(ui, app, true, t_now);
            }
            true
        } else if node == nav.home {
            // R8 调查刀：前台态下 home 第一义 = 回桌面——先收前台态并整帧
            // 重画（无窗应用 DESTROY 批恒空，原路不会 wake；空集 wake 置
            // FULL 的正统重画在此），其下 DESTROY 批 + tab/page0 归位照旧。
            if self.app_fg.is_some() {
                self.leave_fg(ui, "home", t_now);
            }
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
            if self.desktop.tabs.cur != 0 {
                if let Err(e) = desktop::switch_sys_page(ui, &mut self.desktop, 0) {
                    eprintln!("launcher: home tab err={e:?}");
                }
            } else {
                let p = self.desktop.pager.page();
                if p != 0 {
                    snap_page(ui, &mut self.desktop, 0);
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
            match self.power_armed {
                Some(dl) if t_now <= dl => {
                    // 二击确认：真关机（PSCI SYSTEM_OFF——成功整机退出不归）。
                    let st = berry_power_off();
                    eprintln!("launcher: power off st={st} -- use host toolbar OFF");
                    set_power_face(ui, &self.desktop.nav, false);
                    self.mark_band(nav.power);
                    self.power_armed = None;
                }
                _ => {
                    // 首击：arm（3s 窗 + 警示面；窗起算点 = 本击时刻）。
                    set_power_face(ui, &self.desktop.nav, true);
                    self.mark_band(nav.power);
                    self.power_armed = Some(t_now + POWER_ARM_WINDOW_NS);
                }
            }
            true
        } else {
            false
        }
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
#[derive(Clone, Copy)]
pub struct RecentsApp {
    pub id: [u8; 24],
    pub id_len: u8,
}

/// 按 id spawn（on_click 与多任务键重拉共用）。回执记账返回 true 与否；
/// **err=3 已活形**（重开=聚焦刀②）：svcmgr 同 id 活 ∧ 窗在场时不回收
/// 不重拉——本侧改走 wmd 焦点面（`focus_alive_app`）后仍返回 true
/// （「目标 app 已活且已切回」——on_click/recents 记账语义成立）；
/// `note_spawn_ack(ack)` 对 (false,3) 不递增 spawn_ok：已活非拉起，
/// CLICK 标记面与覆盖态门零漂。
fn spawn_by_id_inner(id: &[u8], smoke: bool, spawn_ok: &mut u32, self_wins: [u16; 2]) -> SpawnOut {
    let h = app_cli_h();
    if h == 0 {
        return SpawnOut::Failed; // 冻结相：app 通道结构性缺席
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
            // eco: app 行）、2 = id 不在册、3 = 同 id 已活 ∧ 窗在场（重开
            // =聚焦形——本侧特判走焦点面，不打拒绝诊断；svcmgr 侧另有
            // eco `alive` 行）。失败面永不产标记（eprintln 非
            // canon 前缀，与 CLICK ok / click ready 互非子串）。
            let out = match ack {
                Some((true, _)) => SpawnOut::Spawned,
                Some((false, 3)) => {
                    // 已活回执（重开=聚焦刀① 的消费端）：不重 spawn——
                    // 查表定位应用窗发 OP_WM_FOCUS 切回（成功路径零打印；
                    // 查无窗/聚焦败 eprintln 诊断——TOCTOU 形窗已亡，下
                    // 一击走 svcmgr 回收再开路自愈）。
                    focus_alive_app(self_wins);
                    SpawnOut::AliveFocus
                }
                Some((false, st)) => {
                    eprintln!(
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
                    );
                    SpawnOut::Failed
                }
                None => {
                    eprintln!("launcher: spawn ack bad st={}", rep[0]);
                    SpawnOut::Failed
                }
            };
            if note_spawn_ack(smoke, spawn_ok, ack) {
                // 标记（字节精确）：真实 APP_SPAWN ok 回执恰一次，且仅
                // 冒烟模式（正常 boot 零 CLICK）。
                println!("CLICK ok n=1");
            }
            out
        }
        None => {
            eprintln!("launcher: spawn id bad");
            SpawnOut::Failed
        }
    }
}

/// 已活聚焦（重开=聚焦刀②的 wmd 面）：TABLE_GET 查表定位应用窗
/// （`covering_app_win`——可见 ∧ 非自 ∧ APP 层，焦点窗优先表序兜底；
/// 已活形态下应用窗即覆盖窗）→ OP_WM_FOCUS 聚焦该窗。像素面零动作：
/// v1 无合成器、共享 fb——已活窗的像素本就在屏，FOCUS 只授焦点态
/// （back 键路由面/输入路由面）；wm 通道缺席（冻结相）= 静默跳过
/// （app 通道在而 wm 通道缺的结构形不可达，防御面）。wm 通道取
/// `wm_cli_h()`（gifts [0] 槽——与本文件 on_covered_tap 的 wm_h 同源）。
fn focus_alive_app(self_wins: [u16; 2]) {
    let wm_h = wm_cli_h();
    if wm_h == 0 {
        return; // 冻结相：wm 通道结构性缺席——聚焦面静默跳过
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_table_get_req(&mut req);
    let (n, _) = rpc(wm_h, &req[..m], &[], &mut rep, &mut rhs);
    let target = match gp::parse_wm_snapshot_rep(&rep[..n]) {
        Some(Ok(snap)) => covering_app_win(&snap, &self_wins),
        _ => None,
    };
    let Some(wid) = target else {
        eprintln!("launcher: alive focus no win");
        return;
    };
    let m = gp::encode_wm_focus_req(&mut req, wid);
    let (n, _) = rpc(wm_h, &req[..m], &[], &mut rep, &mut rhs);
    if rep[0] != 0 {
        eprintln!("launcher: alive focus id={} st={}", wid, rep[0]);
    }
    let _ = n;
}

/// 多任务键重拉入口退役注记（加载指示灯刀）：原包装（spawn_by_id）的
/// 诊断面（recents respawn failed）与不记 recents/dedup 语义移入
/// `flush_pending_spawn`（from_recents 支路）；入队见 on_nav_click
/// recents 键分支（queue_spawn_app）。

/// sys 页点击路由（波3-件⑤——`tabs.cur != 0` 时；桌面态恒返 false
/// 落槽面）。返回 true = 已消费。`LoopHost` 方法形（指示灯刀起直取宿主
/// 态——spawn 入队延迟一拍）。
///
/// - 应用列表页：固定过滤词命中 → 换选（换底 + 重装行 + rehang）；
///   行命中（Card/Row/Pad/Text 四面）→ 与桌面格同一 spawn 路（注册表
///   真流——入队加载灯先行一帧，recents/幂等窗记账归 flush 成功支路；
///   空行/标题 = 不消费）。
/// - 设置页：Slider 纯点击步进（+100 permille wrap——Drag 路引擎已接
///   ；步进值落 SETTINGS.JSN）。其余控件 = 引擎组件反应面（事件经
///   [`on_widget_event`] 回写），此处零动作。
impl LoopHost {
    fn on_sys_click(&mut self, ui: &mut UiMain, node: NodeId, t_now: u64) -> bool {
        match self.desktop.tabs.cur {
            1 => {
                // 过滤词命中（id 拷出判定——避免跨 rehang 持 desktop 借）。
                let mut hit_filter = None;
                if let Some(page) = self.desktop.sys.apps.as_ref() {
                    for (i, &wn) in page.words.iter().enumerate() {
                        if node == wn {
                            hit_filter = Some(i as u8);
                            break;
                        }
                    }
                }
                if let Some(f) = hit_filter {
                    if self.desktop.sys.filter != f {
                        self.desktop.sys.filter = f;
                        let big = self.desktop.sys.big;
                        if let Some(page) = self.desktop.sys.apps.as_mut() {
                            if let Err(e) = syspages::set_filter(ui, page, f, big) {
                                eprintln!("launcher: filter err={e:?}");
                            }
                        }
                        if let Err(e) = desktop::rehang_view(ui, &mut self.desktop) {
                            eprintln!("launcher: rehang view err={e:?}");
                        }
                    }
                    return true;
                }
                // 行命中 → spawn（与桌面格同路；真流注册表条目）。
                let mut hit_app = None;
                if let Some(page) = self.desktop.sys.apps.as_ref() {
                    if let Some(ri) = syspages::row_index_of(page, node) {
                        hit_app = page.rows[ri].app;
                    }
                }
                match hit_app.and_then(|idx| icons::registry().get(idx)) {
                    Some(entry) => {
                        self.queue_spawn(ui, entry.id_bytes(), t_now);
                        true
                    }
                    None => false,
                }
            }
            2 => {
                let slider = self.desktop.sys.setpg.as_ref().map(|p| p.vol_slider);
                if slider == Some(node) {
                    let cur = self.desktop.sys.settings.volume;
                    let next = match self.desktop.sys.setpg.as_ref() {
                        Some(p) => syspages::step_slider(ui, p, cur),
                        None => cur,
                    };
                    self.desktop.sys.settings.volume = next;
                    settings_io::save_settings(&self.desktop.sys.settings);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
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

/// CLICK 面（修约 1 / 段④；`LoopHost` 方法形——指示灯刀起入队延迟一
/// 拍）：槽命中映射（cell/label/icon 三节点皆归本格——最深叶命中面）→
/// 注册表条目 → 入队 `pending_spawn` + 点亮加载灯（本拍出帧，下一拍
/// on_head 真发 APP_SPAWN——**正常 boot 与冒烟 boot 同路**；`app_cli_h
/// ()==0` 冻结相在 flush 侧静默跳过零 RPC 零打印）。回执记账（recents/
/// 双击幂等窗/`CLICK ok` 标记）全归 `flush_pending_spawn` 成功支路——
/// 标记语义零漂（首例 ok 回执恰一次，仅冒烟模式）。
impl LoopHost {
    fn on_click(&mut self, ui: &mut UiMain, node: NodeId, t_now: u64) {
        let slot = self
            .desktop
            .slots
            .iter()
            .flatten()
            .chain(self.desktop.dock.iter())
            .find(|s| s.cell == node || s.label_node == node || s.icon == Some(node));
        let Some(slot) = slot else { return }; // 壁纸/状态栏/圆点/容器白区 = 无格
        let Some(idx) = slot.app else { return }; // 占位/幽灵格不可 spawn
        let Some(entry) = icons::registry().get(idx) else { return };
        self.queue_spawn(ui, entry.id_bytes(), t_now);
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

    /// 双击幂等窗真值表（总审计 input-chain P2 兜底面）：窗内同 id 吞
    /// 第二击；无记账/窗外（帽边界即过期）/异 id/命中非格面 = 不吞
    /// （点击落原面）。
    #[test]
    fn spawn_dedup_hit_truth_table() {
        let app = |id: &[u8]| {
            let mut a = [0u8; 24];
            a[..id.len()].copy_from_slice(id);
            RecentsApp { id: a, id_len: id.len() as u8 }
        };
        let a = app(b"APPDEMO");
        let ack = 1_000_000_000u64;
        // 窗内同 id（referee 250ms 窗外的慢二击形）：吞。
        assert!(spawn_dedup_hit(Some((&a, ack)), Some(b"APPDEMO"), ack + 500_000_000));
        // 帽边界即过期（t_now >= ack + 帽 → 不吞——窗外蓄意重开）。
        assert!(!spawn_dedup_hit(
            Some((&a, ack)),
            Some(b"APPDEMO"),
            ack + SPAWN_DEDUP_WINDOW_NS
        ));
        // 窗外更远：不吞。
        assert!(!spawn_dedup_hit(
            Some((&a, ack)),
            Some(b"APPDEMO"),
            ack + SPAWN_DEDUP_WINDOW_NS + 1
        ));
        // 异 id（另一应用格）：不吞——正常 spawn 面。
        assert!(!spawn_dedup_hit(Some((&a, ack)), Some(b"FILES"), ack + 1));
        // 命中非格面（壁纸/容器白区）：不吞，落原面。
        assert!(!spawn_dedup_hit(Some((&a, ack)), None, ack + 1));
        // 无记账（首击 spawn 失败/boot 未点击）：不吞。
        assert!(!spawn_dedup_hit(None, Some(b"APPDEMO"), ack));
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
