//! desktop —— 桌面静态面（GUI-8 段③ W1，docs/GUI-8实施设计草案-v1.0.md §4
//! 段③「布局桩」；波3-件⑦ 起状态栏底座/scrim/文本叶抽入
//! appshell::scene——本模块只留桌面特化）。
//!
//! 责任：一次性建桌面保留树——OH 官方壁纸 + phone 状态栏（appshell::scene
//! 整条装配）+ SwipePager 四页 × Grid 5×4 槽板 + 常驻 Dock + a11y 骨架
//!（32/32）。数据源 = berry-layout 布局 ×
//! icons 注册表（注册表空 → 布局 0 cell → 全占位格 label "-"，W1 冻结
//! 相常态形态）。**帧循环/翻页/点击全无**——`frameloop`（W2）经本模块
//! 公开面驱动：`pager`（on_msg/tick）+ `slots` 槽表（命中映射/spawn）+
//! `rehang_a11y`（翻页重挂当前页）。
//!
//! 裁决出处（草案 §3）：
//!
//! - **裁决六（壁纸形态）**：OH 官方壁纸 Icon 全幅（`wallpaper.rs` 预采样
//!   静态源，SmoothArgb 放大——2026-11-06 ccc80954 起替旧三 Rect 带状
//!   形态）；Root 直接子插入序 = 自底向上 z 序（paint.rs 前序头先发 =
//!   先插者在底）：壁纸 → 状态栏 scrim 衬底 → 内容列。
//! - **裁决四-e（归属）**：状态栏/壁纸 = launcher 窗口内容层（非 wmd
//!   系统窗）——D3 单 wm 客户端不变量保持，gfxdemo `WM ok` 零连带。
//! - **裁决八（a11y OH phone 形 32/32）**：状态栏 4 件 ROLE_TEXT + 4 页
//!   Grid 容器 ROLE_LIST + 当前页 20 格 ROLE_BUTTON + 4 个 Dock 位
//!   ROLE_BUTTON = 32/32；翻页 take/set 重挂当前页（`rehang_a11y`）。
//!   press_map v1 不做（视觉反馈归 W2 spawn 回执标记行）。波一刀 B 的
//!   SIM/UTC 视觉角标**不占 a11y 槽**（32 帽满）——模拟态如实标注折进
//!   signal/wifi/battery 三件既有 label（in-place 改写）。
//!   **波一刀 D 修订**：页容器 ROLE_LIST 4 页全挂改**当前页单挂**
//!   （`rehang_a11y` 同批 take/set）——腾 3 槽给三键导航 ROLE_BUTTON，
//!   常态恰满 32（4 状态 + 1 页容器 + 20 格 + 4 dock + 3 键；非当前页
//!   本就非可见页，不出 a11y 树是三系常规形态）。
//! - **裁决 D（几何）**：屏幕几何全经 wm GETINFO 动态（`ui.viewport()`
//!   ——Root/带高/Grid 高全派生），禁硬编 800×600；字号档/图标 px 等
//!   字体面常量不属屏幕参数，不在此列。
//!
//! 波一刀 B（状态栏可信化，`research/桌面UX审计/02` §6.1 M1）：①状态栏
//! scrim 衬底（Rect 无 alpha——50% 黑预混壁纸顶色，§3.2 对比达标）；
//! ②真时钟——`clock: Option<[u8;5]>` 建树参数（main 经
//! `berry_clock_read(CLOCK_REALTIME)` 单次取值；None = 注入缺席回退
//! 12:00 静态占位），frameloop 每分钟 set_text 续摆 + UTC 角标；③模拟
//! 态标注——左组 SIM 小字（QEMU 无真值源：信号/电池恒满格为模拟值，
//! 如实注）。
//!
//! 波一刀 D（三键导航，`research/桌面UX审计/02` §6.1 M2）：底部三键
//! 返回/桌面/关机——返回 v1 = 翻页语义（上一页，page 0 无操作；应用内
//! 返回随 wmd 系统键语义刀 I2）；桌面 = 回首页 page 0（已在 = 无操作）；
//! 关机 = 二击 3s 窗确认（armed 态换暗红警示面，frameloop set_props
//! 换 id）→ 内核 `power_off`（0x88，PSCI SYSTEM_OFF——失败 eprintln
//! 诊断降级不 fail）。命中区 = 键 Icon 节点整体：big 132px（44vp×3）/
//! 紧凑 44px（应用规范 §3.2 硬指标下限）；视觉边界对比 = 键深底对壁纸
//! ≥3:1（底色 CLEAR_BG 同族）。三键只在桌面态有效（应用全屏独占时
//! 随 launcher 内容层整层被盖——02 号 §2.3 边界，SYSTEM 窗随 I1）。
//!
//! 树规模（最坏满图标）：1 Root + 壁纸 1 + scrim 1 + 内容列 1 + 状态栏
//! 13（+SIM/UTC 两文本）+ grid top 1 + pager 10 + 4×(Grid+20 Pad+20 内
//! Col+20 Icon+20 Text) + filler 1 + Dock 5 + 导航条 4（Row + 3 键 Icon）
//! = **361 节点** < MAX_NODES=512。

use berry_appshell as appshell;
use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_layout::{CELL_MAX, PAGE_MAX};
use berry_ui::desc::{
    ColProps, GridProps, IconProps, PadProps, RectProps, RowProps, StackAlign, StackProps,
};
use berry_ui::layout::CrossAlign;
use berry_ui::swipe::SwipePager;
use berry_ui::{NodeId, Props, TextBuf, UiError, UiMain, ViewKind, ViewSpec, ROLE_BUTTON};

use appshell::scene::{self, text_spec, OH_DENSITY};

use crate::icons;
use crate::icons::{AppEntry, NO_ICON};
use crate::layout_io::OwnedLayout;
use crate::settings_io::{BootMode, Settings};
use crate::syspages;

// —— 桌面视窗常量（默认布局公式同源消费——layout_io::default_layout）——

/// 桌面页数 = berry-layout `PAGE_MAX` 权威 4（裁决三定谳）。
pub(crate) const PAGES: usize = PAGE_MAX as usize;
/// 每页网格列数（OH phone 5×4 档）。
pub(crate) const GRID_COLS: usize = 4;
/// 每页网格行数。
pub(crate) const GRID_ROWS: usize = 5;
/// 每页槽数 = 4×5 = 20（OH 手机 5×4 档；CELL_MAX=48 覆盖前三页）。
pub(crate) const SLOTS_PER_PAGE: usize = GRID_COLS * GRID_ROWS;

/// 编译期钉视窗覆盖 berry-layout 容量域（4×20 ≥ CELL_MAX=48——第四页
/// 渲染面可空，持久化域不缩）。
const _: () = assert!(PAGES * SLOTS_PER_PAGE >= CELL_MAX, "OH 5×4 视窗覆盖布局容量");

// —— 派生几何（字体档单源派生，非屏幕参数硬编——裁决 D 注记）——

/// 页指示器视觉点行高（swipe.rs DOT_SIZE 镜像）。
const DOT_ROW_H: u32 = 16;
/// 紧凑档 BottomTabs 条高（pattern.rs BOTTOMTABS_H 紧凑 48px——网格高度
/// 扣减项，防 800×600 底条与末行格重叠）。
const TABS_H_COMPACT: u32 = 48;
/// 格内图标目标边长（OH phone icon 54vp×3；108px 源经 ScaledArgb 采样）。
const ICON_PX: u32 = 162;

// —— OpenHarmony phone launcher 几何（applications_launcher 源仓
//    PhonePresetStyleConstants / PhoneSmartDockStyleConfig；1080×1920
//    = 360×640vp @3x 折算；密度/状态栏几何单源 = appshell::scene）——
/// 桌面网格左右 margin 12vp。
const OH_GRID_MARGIN: u32 = 12;
/// 桌面 app item 80vp（网格 5×4 的行高）。
const OH_ITEM_H: u32 = 80;
/// 桌面网格 gap 5.33vp（LayoutViewModel 360vp 宽实算后 ×3）。
const OH_GRID_GAP: u32 = 16;
/// 桌面首行 app item 的绝对 top 53.83vp（含状态栏 44vp）。
const OH_DESKTOP_TOP: u32 = 162;
/// Dock 视觉高 78vp（不含外层 bottom margin）。
const OH_DOCK_H: u32 = 78;
/// Dock 下方保留区 45vp（sysUI bottom 36vp + dock margin 9vp——波3-件⑤
/// 起该区与 BottomTabs 条区共享，值并入 filler 公式注释，不再单列）。
/// Dock 源资产密度（目标 3x；源段 167×156 → 目标段 ~250×234）。
const DOCK_SRC_DENSITY: u32 = 2;
/// Dock 目标分段宽（167×1.5 四舍分配，和 = 1002）。
const DOCK_TARGET_W: [u32; DOCK_SLOTS] = [251, 250, 250, 251];
/// Dock 常驻图标数（PhoneSmartDock max=4）。
const DOCK_SLOTS: usize = 4;

/// ROLE_LIST —— berry-ui 角色常量域（wire u8 0..=5 在册）之外的**开放
/// 值域 host 扩展**（lib.rs 角色常量族头注明文「host 可扩」）：6 =
/// launcher 扩 LIST（页 Grid 容器）。
pub(crate) const ROLE_LIST: u8 = 6;

/// 桌面格按压面初始底色（刀 B）：OH 壁纸网格区深青黑近似（伪装透明）。
const CELL_FACE_BG: Color = Color { r: 0x04, g: 0x2E, b: 0x33 };
/// 导航键按压面初始底色（刀 B）：导航区深青黑近似（伪装透明）。
const NAV_FACE_BG: Color = Color { r: 0x03, g: 0x24, b: 0x28 };


/// 波3-件⑤：桌面页有主格的 a11y 挂载帽（预算 = A11Y_MAX 32 − 底座 16
///（状态栏 4 + dock 4 + 五键 5 + tab 三页位 3）− 页容器 1；满注册表
/// 20 格超帽时行主序截断——帽内诚实收口面）。
const A11Y_CELL_CAP: usize = 15;

// —— 五键导航条（owner 2026-11-13 裁定：返回/多任务/桌面/旋转/关机，
// 键加大）——
/// 导航键命中区边长：big = 160px（owner「大点」——原 132px 加大）；
/// 紧凑 = 44px（应用规范 §3.2 触控目标硬指标下限——视觉 = 键块整体，
/// 命中区不缩）。
const NAV_KEY_PX_BIG: u32 = 96;
const NAV_KEY_PX: u32 = 44;
/// 导航条高：big = 135（波3-件⑤ 起 = OH sysUI 45vp×3——BottomTabs 条
/// 进驻内容列后 1920 高内须整列收口，原 owner 加高 190px 挤出屏 55px
/// 的超额让位给条区；键面 160px 不变，条内居中上下溢 12px 视觉可容，
/// 桌面四件全收口进屏）；紧凑 = 44。
const NAV_BAR_H_BIG: u32 = 135;
/// 键间隔：big 48px；紧凑 12px。
const NAV_GAP_BIG: u32 = 176;
const NAV_GAP: u32 = 76;
/// 关机二击确认窗（防误触——两次点击 3s 内才生效；过期回常态）。
pub const POWER_ARM_WINDOW_NS: u64 = 3 * 1_000_000_000;
/// a11y 页容器 label。
const PAGE_LABELS: [&[u8]; PAGES] = [b"page 0", b"page 1", b"page 2", b"page 3"];
/// 空槽占位 label。
const PLACEHOLDER: &[u8] = b"";
/// 常驻 Dock app（Contacts/Messages/Browser/Camera；从网格去重）。
/// GUI 验收双 app（官方桌面常态隐藏，只保留后台/spawn 面）。
const TEST_IDS: [&[u8]; 2] = [b"APPDEMO", b"APPQUIET"];
/// OH PhonePresetDockItem 官方顺序。
const DOCK_IDS: [&[u8]; DOCK_SLOTS] = [b"CONTACTS", b"PHOTOS", b"SETTINGS", b"MMS"];

/// 格 label 配色（ICON_FG 同族弱一档；文本白 TEXT_FG 单源 =
/// appshell::scene）。
const LABEL_FG: Color = Color { r: 228, g: 228, b: 232 };
/// 图标 tint 色（Icon Argb 源不消费 fg——`paint.rs emit_image` 直写
/// 像素；仅 mono/Bits 源着墨。刀 D：frameloop `set_power_face` 复用）。
pub(crate) const ICON_FG: Color = Color { r: 230, g: 230, b: 235 };

/// 一格桌面的建树产物（W2 帧循环消费面）。
#[derive(Clone, Copy)]
pub struct Slot {
    /// 格容器 Col（a11y ROLE_BUTTON 锚 + hit 容器）。
    pub cell: NodeId,
    /// 按压视觉面（cell 下内衬 Rect——press_map 承载，刀 B）。
    pub press_face: Option<NodeId>,
    /// label Text 节点（Click 命中可能是它——最深叶命中面）。
    pub label_node: NodeId,
    /// 图标节点（占位/无图标格 = None；命中映射同上）。
    pub icon: Option<NodeId>,
    /// 格主注册表索引（占位/幽灵格 = None——不可 spawn）。
    pub app: Option<usize>,
    /// a11y label 字节（rehang 重挂单源——建树时定格）。
    label_buf: TextBuf,
}

/// 桌面静态面句柄（`build` 产物——W2 frameloop 消费）。
pub struct Desktop {
    /// 分页控制器（翻页状态机 + track offset 唯一写者）。
    pub pager: SwipePager,
    /// 槽表 `[页][行×GRID_COLS+列]`。
    pub slots: [[Slot; SLOTS_PER_PAGE]; PAGES],
    /// 常驻 dock 槽（PhoneSmartDock max=4；点击与网格同映射）。
    pub dock: [Slot; DOCK_SLOTS],
    /// 状态栏时钟文本节点（波一刀 B：REALTIME 摆字——frameloop 每分钟
    /// set_text 等长换文 = 纯 PAINT；a11y label 同步 in-place 覆写）。
    /// UTC 角标节点建树即定格（在场显字/缺席空文本），运行期无改笔——
    /// 不入句柄。
    pub clock: NodeId,
    /// 三键导航（波一刀 D）：返回/桌面/关机 Icon 节点（frameloop
    /// `Msg::Click` 命中面 + 关机 armed 态 set_props 换 id 的锚）。
    pub nav: NavNodes,
    /// 页容器节点（刀 D 修订裁决八：ROLE_LIST 单挂当前页——rehang 同批
    /// take/set，32 帽腾 3 槽给导航键）。
    page_nodes: [NodeId; PAGES],
    /// a11y 当前挂载页（`u8::MAX` 哨兵 = 未挂——build 尾经 rehang(0) 初挂）。
    a11y_page: u8,
    /// 波3-件⑤：系统页三件套（0=桌面 1=应用列表 2=设置）。
    pub tabs: SysTabs,
    /// 波3-件⑤：系统页框（pages 宿主 + 两 sys 页在树态 + 设置真值镜像）。
    pub sys: SysFrame,
    /// sys 页 a11y 挂载位（`u8::MAX` = 桌面态——sys 页 a11y 全摘）。
    a11y_sys: u8,
}

/// 波3-件⑤：BottomTabs 三页位句柄（pattern 冻结名消费——条实例整条
/// 重建换选，节点 id 随实例换，命中面读当帧值）。
pub struct SysTabs {
    /// 条宿主 Stack（内容列内恒定流槽——重建实例 z 叠其上，几何零漂）。
    pub bar_host: NodeId,
    /// 当前条实例（摘除锚——换选时整树 remove）。
    pub bar_inst: NodeId,
    /// 三页位节点（命中面——frameloop Click 路由）。
    pub tab_nodes: [NodeId; 3],
    /// 当前系统页（0=桌面 1=应用列表 2=设置）。
    pub cur: u8,
}

/// 波3-件⑤：系统页框（宿主几何 + 在树 sys 页 + 设置/过滤真值镜像——
/// frameloop 事件回写单源）。
pub struct SysFrame {
    /// pages 宿主 Stack（桌面页 + sys 页 z 叠——sys 页不透明底盖桌面）。
    pub host: NodeId,
    /// 页几何（w/h——sys 页建树参数）。
    pub w: u32,
    pub h: u32,
    /// big 档（建树档位——sys 页重建时沿用）。
    pub big: bool,
    /// 应用列表页在树态（桌面态/设置页态 = None——离树即摘）。
    pub apps: Option<syspages::AppPage>,
    /// 设置页在树态。
    pub setpg: Option<syspages::SettingsPage>,
    /// 应用列表过滤位（AppPage 离树重建间保留——FILTER_WORDS 域）。
    pub filter: u8,
    /// boot 模式真值镜像（设置页 Toggle 初值 + 事件回写）。
    pub boot: BootMode,
    /// 设置项真值镜像（控件初值 + 事件回写）。
    pub settings: Settings,
}

/// 五键导航节点句柄（owner 2026-11-13——frameloop 消费面）。
#[derive(Clone, Copy)]
pub struct NavNodes {
    /// 返回键（v1 语义 = 翻页上一页，如实注：应用内返回随 wmd 系统键
    /// 语义刀 I2）。
    pub back: NodeId,
    /// 多任务键（v1 语义 = 重新拉起最近 spawn 的应用——真重拉非死键）。
    pub recents: NodeId,
    /// 桌面键（回首页 page 0；已在 = 无操作）。
    pub home: NodeId,
    /// 旋转键（v1 语义 = 打 `ROTATE` 串口标记——宿主 berry 窗口监听
    /// 并换几何重启；屏幕方向键）。
    pub rotate: NodeId,
    /// 关机键（二击 3s 窗确认 → 内核 power_off 0x88 PSCI SYSTEM_OFF）。
    pub power: NodeId,
    /// 五键 (触发 Icon, 按压面 Rect) 对——press_map 刷新面（rehang 同步
    /// 重建表用；刀 B NodeFull 修复后与当前页格共表 ≤28 帽）。
    pub faces: [(NodeId, NodeId); 5],
    /// 键命中区边长（px，建树档位定格——frameloop `set_power_face`
    /// 换 id 时 set_props 须携同 w/h，防回落源内在尺寸）。
    pub key_px: u32,
}

/// 建树单源槽计划（布局格主三态折算：注册表在册 App/幽灵 Ghost——
/// 布局在册但注册表缺席，label 显示 id、不可 spawn/空占位；label 借自
/// `&OwnedLayout`/`&[AppEntry]`——建树即消费，不越 `build` 生命周期）。
#[derive(Clone, Copy)]
struct SlotPlan<'a> {
    label: &'a [u8],
    icon: Option<u16>,
    app: Option<usize>,
}

/// 条目图标三态：`NO_ICON` → None（不建节点）；域内 id → Some。
fn icon_of(e: &AppEntry) -> Option<u16> {
    if e.icon == NO_ICON {
        None
    } else {
        Some(e.icon)
    }
}

/// 是否 dock 常驻 app（PhoneSmartDock max=4）。
fn is_dock_app(id: &[u8]) -> bool {
    DOCK_IDS.iter().any(|d| *d == id)
}

fn is_test_app(id: &[u8]) -> bool {
    TEST_IDS.iter().any(|d| *d == id)
}

/// 桌面建树（main 单次调用；几何全 `ui.viewport()` 动态——裁决 D）。
/// Err = 容量/文本域违例（NodeFull/NodeBad/TextTooLong 族）——调用方
/// `fail` 收口（eprintln + park；监督重启归 svcmgr 域，本进程不自救）。
/// `clock`（波一刀 B）= main 经 `berry_clock_read(CLOCK_REALTIME)` 单次
/// 取值换算的 "HH:MM"（appshell::scene::hhmm_utc 单源）——`None` = 注入
/// 缺席（冒烟拓扑/-kernel 相）回退 12:00 静态占位（scene::STATUS_CLOCK）。
pub fn build(
    ui: &mut UiMain,
    layout: &OwnedLayout,
    reg: &[AppEntry],
    clock: Option<[u8; 5]>,
    boot: BootMode,
    settings: Settings,
) -> Result<Desktop, UiError> {
    let (vp_w, vp_h) = ui.viewport();
    // OH phone 固定几何只给 1080×1920（360×640vp @3x）；横屏/调试档走
    // 紧凑分支，防 Dock/top 常量把 1280×720 挤出视口。
    let big = vp_w >= 600 && vp_h >= 1800;

    // OpenHarmony phone 档：1080×1920 = 360×640vp @3x，直接消费 OH
    // launcher 源仓 phone 常量。800×600 调试档保留紧凑布局。状态栏档位
    // 表（高/主字号/角标字号）单源 = appshell::scene::status_sizes。
    let sizes = scene::status_sizes(big);
    let status_h = sizes.h;
    let label_size = if big { FontSize::S32x48 } else { FontSize::S16x32 };
    // 800×600 调试档：56px 源采样 + 16px label 恰容 95px 轨高。
    let icon_px = if big { ICON_PX } else { 56 };
    let grid_gap = if big { OH_GRID_GAP } else { 12 };
    let cell_gap = if big { 12 } else { 2 };
    let root = ui.root();

    // OpenHarmony phone 默认壁纸：官方 JPEG 宿主预采样成 135×240
    // 静态源，SmoothArgb 双线性放大到 1080×1920；rodata 直供零堆。
    // 800×600 compact 调试档会拉伸该 9:16 源（非 OH phone 精确面）。
    ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Icon,
            key: 0,
            props: Props::Icon(IconProps {
                id: icons::WALLPAPER,
                fg: ICON_FG,
                w: Some(vp_w),
                h: Some(vp_h),
            }),
        },
    )?;

    // 波一刀 B → 刀 2（OH 形）：状态栏 scrim 衬底——壁纸之上、内容列之下的
    // 整宽暗化带，色值 = 壁纸顶区动态采样预混（50% 黑，随壁纸跟随）。
    // 插入序 = z 序（paint 前序先发 = 先插者底）：壁纸 → 本 Rect → 内容列
    // ——**必须在 content 之前插入**（后插者盖上：尾插会把状态内容整条
    // 压在 scrim 之下，boot 实证 2026-11-13）。
    // 沉浸（owner 2026-11-16）：scrim 撤除——壁纸直通顶部。（ux-replay 侧
    // 的 scrim Rect 不回填——owner 沉浸裁决晚于重放基线，以 HEAD 为准。）

    // 内容列满屏：cross=Center 让固定宽 Grid/Dock 在页内水平居中。
    let content = ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(vp_w),
                h: Some(vp_h),
                cross: CrossAlign::Center,
            }),
        },
    )?;

    // 状态栏整条装配（appshell::scene——OH systemui phone：左 = signal/
    // wifi（+SIM 模拟态角标），右 = battery/clock（+UTC 角标）；
    // StatusbarConfig LayoutGroups 1/3，clock rightmost）。首摆时钟文本
    // 对 = scene::clock_texts（真值或 12:00 占位回退）；返回时钟节点
    //（UTC 角标建树定格不入句柄）。
    let (clock_text, utc_text) = scene::clock_texts(&clock);
    let time_node = scene::build_status_bar(
        ui,
        content,
        &scene::StatusBarArgs {
            vp_w,
            status_h,
            big,
            status_size: sizes.main,
            tag_size: sizes.tag,
            clock_text,
            utc_text,
        },
    )?;

    // OH calculateDesktop：首 item top 绝对 162px；扣除状态栏 132px。
    let grid_top = if big { OH_DESKTOP_TOP.saturating_sub(status_h) } else { 8 };
    ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(1),
                h: Some(grid_top),
                cross: CrossAlign::Center,
            }),
        },
    )?;

    // —— 波3-件⑤：pages 宿主 Stack + 桌面页——内容列自「状态栏 + 桌面」
    //    改「状态栏 + pages 区 + BottomTabs 条 + 五键条」四段。桌面页 =
    //    原 SwipePager 四页 + filler + Dock **原样内嵌**（pager 状态机与
    //    建树面零改动，只换父位）；sys 页（应用列表/设置）按需建入本
    //    Stack 顶层——不透明底整面盖住桌面（v1 无 clip/隐藏原语，覆盖
    //    即隐藏——子树保活，pager 翻页态零损），离树 = remove 整页。
    let tabs_h = if big { berry_ui::pattern::BOTTOMTABS_H } else { TABS_H_COMPACT };
    let nav_h = if big { NAV_BAR_H_BIG } else { NAV_KEY_PX };
    let host_h = vp_h
        .saturating_sub(status_h + grid_top + tabs_h + nav_h)
        .saturating_sub(if big { 0 } else { 8 }); // 紧凑档 8px 底垫（原公式同值）
    let pages_host = ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Stack,
            key: 0,
            props: Props::Stack(StackProps { align: StackAlign::Start }),
        },
    )?;
    let desktop_page = ui.add(
        pages_host,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(vp_w),
                h: Some(host_h),
                cross: CrossAlign::Center,
            }),
        },
    )?;

    let mut pager = SwipePager::new(PAGES as u8);
    pager.attach(ui, desktop_page)?;

    // 布局 → 槽计划；dock 常驻 app 不重复进网格（OH SmartDock 语义）。
    // 剔除 dock 后非 dock app 按原相对序**紧致重排**——否则默认 9 app
    // 布局会在网格里留下 dock 取走的洞，不像 OH。
    let mut plan: [[SlotPlan<'_>; SLOTS_PER_PAGE]; PAGES] =
        [[SlotPlan { label: PLACEHOLDER, icon: None, app: None }; SLOTS_PER_PAGE]; PAGES];
    let mut cursor = 0usize;
    // 常态布局若含官方系统 app，则验收 APPDEMO/APPQUIET 不进官方桌面；
    // GUI8 canonical 双 app 布局仍全显示（冒烟/常态隔离点击面不变）。
    let official_present = layout
        .cells()
        .iter()
        .any(|c| !is_test_app(c.app_bytes()) && !is_dock_app(c.app_bytes()));
    for c in layout.cells() {
        if cursor >= PAGES * SLOTS_PER_PAGE
            || is_dock_app(c.app_bytes())
            || (official_present && is_test_app(c.app_bytes()))
        {
            continue;
        }
        let page = cursor / SLOTS_PER_PAGE;
        let slot = cursor % SLOTS_PER_PAGE;
        cursor += 1;
        plan[page][slot] = match reg.iter().position(|e| e.id_bytes() == c.app_bytes()) {
            Some(i) => SlotPlan {
                label: reg[i].label_bytes(),
                icon: icon_of(&reg[i]),
                app: Some(i),
            },
            None => SlotPlan { label: c.app_bytes(), icon: None, app: None },
        };
    }

    // OH 5×4：grid宽 1008px、item 240px、gap 16px（360vp 公式实算 ×3）。
    // 32px HD label 最长 256px，轨宽 240px 时向 16px gap 两侧各溢 8px，
    // 不压相邻图标；小屏回落“铺满可用高”。
    let grid_w = if big {
        vp_w.saturating_sub(OH_GRID_MARGIN * OH_DENSITY * 2)
    } else {
        vp_w
    };
    let grid_h = if big {
        OH_ITEM_H * OH_DENSITY * GRID_ROWS as u32
            + OH_GRID_GAP * (GRID_ROWS as u32 - 1)
    } else {
        // 小屏无 Dock：16px 视觉点行 + 8px 底垫 + BottomTabs 48px + 导航
        // 条 44px（刀 D——OH 96px 指示区预留仅 phone 档；波3-件⑤b 起底条
        // 占位并入扣减——末行格与底条不再重叠，cell 高收窄 48px 帽内）。
        vp_h.saturating_sub(status_h + grid_top + DOT_ROW_H + TABS_H_COMPACT + NAV_KEY_PX + 8)
    };
    let cell_h = if big {
        OH_ITEM_H * OH_DENSITY
    } else {
        (grid_h.saturating_sub((GRID_ROWS as u32 - 1) * grid_gap)) / GRID_ROWS as u32
    };
    let mut slots: [[Option<Slot>; SLOTS_PER_PAGE]; PAGES] = [[None; SLOTS_PER_PAGE]; PAGES];
    // 页容器收集（刀 D 修订裁决八：ROLE_LIST 单挂当前页——a11y 表尾由
    // rehang_a11y 统一挂/摘，建树期不挂）。
    let mut page_nodes: [Option<NodeId>; PAGES] = [None; PAGES];
    for p in 0..PAGES {
        let page = pager.page_node(p as u8).ok_or(UiError::NodeBad)?;
        page_nodes[p] = Some(page);
        let grid = ui.add(
            page,
            ViewSpec {
                kind: ViewKind::Grid,
                key: 0,
                props: Props::Grid(GridProps {
                    cols: GRID_COLS as u8,
                    rows: GRID_ROWS as u8,
                    gap: grid_gap,
                    cross: CrossAlign::Stretch,
                    w: Some(grid_w),
                    h: Some(grid_h),
                }),
            },
        )?;
        // 刀 D 修订裁决八：页容器 ROLE_LIST 改 rehang_a11y 单挂当前页
        //（原 4 页全挂 = 32 帽满员，导航三键无槽——非当前页本就非可见页）。
        for s in 0..SLOTS_PER_PAGE {
            let pn = plan[p][s];
            // OH app item 内边距 3vp×3：240 = 9 + icon162 + gap12 +
            // label48 + 9。Pad 直接作 Grid 子（内在恰满轨高，命中面整格）。
            let item_pad = if big { 9 } else { 2 };
            let cell = ui.add(
                grid,
                ViewSpec {
                    kind: ViewKind::Pad,
                    key: 0,
                    props: Props::Pad(PadProps { all: item_pad }),
                },
            )?;
            // 刀 B：按压视觉面——cell 下内衬 Rect（press_map 承载；
            // Press 换底 PRESSED_BG 即时反馈，Release 还原）。初始 bg =
            // 壁纸网格区近似色（青黑系——v1 Rect 无 alpha，近色伪装透明；
            // 样式系统落地后迁出）。
            // 刀 B 结构修正（BadParent 修复 2026-11-15）：Rect 是叶子不
            // 容子——原 item Col 挂 press_face 下必炸（boot 实证 BadParent
            // park）。改 cell(Pad) 双子叠加：Pad 对所有子同位 (inset,inset)
            // 全幅分配 = 叠加语义。Rect 先插 = 底层；Col 后插 = 盖上。
            let press_face = ui.add(
                cell,
                ViewSpec {
                    kind: ViewKind::Rect,
                    key: 0,
                    props: Props::Rect(RectProps { w: None, h: None, bg: CELL_FACE_BG, radius: 0 }),
                },
            )?;
            let item = ui.add(
                cell,
                ViewSpec {
                    kind: ViewKind::Col,
                    key: 0,
                    props: Props::Col(ColProps {
                        gap: cell_gap,
                        w: None,
                        h: Some(cell_h.saturating_sub(item_pad * 2)),
                        cross: CrossAlign::Center,
                    }),
                },
            )?;
            let icon_node = match pn.icon {
                Some(id) => Some(
                    ui.add(
                        item,
                        ViewSpec {
                            kind: ViewKind::Icon,
                            key: 0,
                            props: Props::Icon(IconProps {
                                id,
                                fg: ICON_FG,
                                w: Some(icon_px),
                                h: Some(icon_px),
                            }),
                        },
                    )?,
                ),
                None => None,
            };
            let label_node = ui.add(item, text_spec(pn.label, LABEL_FG, label_size)?)?;
            slots[p][s] = Some(Slot {
                cell,
                press_face: Some(press_face),
                label_node,
                icon: icon_node,
                app: pn.app,
                label_buf: TextBuf::new(pn.label)?,
            });
        }
    }
    let slots = slots.map(|page| page.map(|s| s.expect("slot 填满不变量破缺")));
    let page_nodes: [NodeId; PAGES] =
        page_nodes.map(|n| n.expect("页容器收集不变量破缺"));

    // OH SmartDock：官方 1002×234 条切成四段内建 Icon——每段是一个命中
    // 面，拼回仍是整条 radius66 圆角 dock（无容器背景原语也不失真）。
    let dock_w = icons::DOCK_DIMS
        .iter()
        .map(|&(w, _)| w)
        .sum::<u32>()
        .saturating_mul(OH_DENSITY)
        / DOCK_SRC_DENSITY;
    let dock_h = if big { OH_DOCK_H * OH_DENSITY } else { 0 };
    if big {
        // 波3-件⑤：filler = 页区余高扣 pager（grid+点行）与 dock——
        // BottomTabs 进驻后 dock 整体上移 144px（OH 45vp 底保留区
        //（sysUI 36vp + margin 9vp）与条区共享；桌面网格/dock 本体几何
        // 零改动，仅纵向让位）。
        let before_dock = grid_h + DOT_ROW_H;
        let filler = host_h.saturating_sub(before_dock + dock_h);
        ui.add(
            desktop_page,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: Some(1),
                    h: Some(filler),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
    }
    let dock_row = ui.add(
        desktop_page,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(if big { dock_w } else { 0 }),
                h: Some(dock_h),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let mut dock = Vec::with_capacity(DOCK_SLOTS);
    for (i, id) in DOCK_IDS.iter().enumerate() {
        let entry_idx = reg.iter().position(|e| e.id_bytes() == *id);
        let (seg_w, seg_h) = (
            DOCK_TARGET_W[i],
            icons::DOCK_DIMS[i].1 * OH_DENSITY / DOCK_SRC_DENSITY,
        );
        let icon_node = ui.add(
            dock_row,
            ViewSpec {
                kind: ViewKind::Icon,
                key: 0,
                props: Props::Icon(IconProps {
                    // 位图库优先（OH sysRes 单枚 66×66 面）；缺名回退
                    // DOCK_FACE_IDS 分段背板面（位图/内建同域渲染，几何不变）。
                    id: icons::bitmap_id(icons::DOCK_BITMAP_NAMES[i])
                        .unwrap_or(icons::DOCK_FACE_IDS[i]),
                    fg: ICON_FG,
                    w: Some(if big { seg_w } else { 0 }),
                    h: Some(if big { seg_h } else { 0 }),
                }),
            },
        )?;
        let label = entry_idx.map(|idx| reg[idx].label_bytes()).unwrap_or(b"");
        ui.set_a11y(icon_node, ROLE_BUTTON, label)?;
        dock.push(Slot {
            cell: icon_node,
            press_face: None,
            label_node: icon_node,
            icon: None,
            app: entry_idx,
            label_buf: TextBuf::new(label)?,
        });
    }
    let dock: [Slot; DOCK_SLOTS] = dock.try_into().ok().expect("dock 恰四槽");

    // —— 波3-件⑤：BottomTabs 条宿主（content 流槽：pages 区之下、五键
    //    条之上）——宿主 Stack 恒定占位，条实例整条重建换选（pattern
    //    选中态建树期烘焙——重建零漂移面；实例 z 叠宿主顶层，几何恒同）。
    //    条本体建树在 syspages::build_tab_bar（pattern 冻结名单源）。
    let bar_host = ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Stack,
            key: 0,
            props: Props::Stack(StackProps { align: StackAlign::Start }),
        },
    )?;
    let (bar_inst, tab_nodes) = syspages::build_tab_bar(ui, bar_host, 0)?;
    let tab_labels: [&[u8]; 3] = [b"home tab", b"apps tab", b"settings tab"];
    for (i, &tn) in tab_nodes.iter().enumerate() {
        ui.set_a11y(tn, ROLE_BUTTON, tab_labels[i])?;
    }

    // —— 五键导航条（owner 2026-11-13：返回/多任务/桌面/旋转/关机）：
    //    content 列尾行。键 = Icon 节点整体（Argb 深底圆角块 + 白符号，
    //    icons::nav_face——fg 不消费）即命中区 big 160px。
    let nav_key = if big { NAV_KEY_PX_BIG } else { NAV_KEY_PX };
    let nav_gap = if big { NAV_GAP_BIG } else { NAV_GAP };
    let nav_h = if big { NAV_BAR_H_BIG } else { NAV_KEY_PX };
    let nav_bar_w = nav_key * 3 + nav_gap * 2;
    let nav_bar = ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: nav_gap,
                w: Some(nav_bar_w),
                h: Some(nav_h),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    // 刀 B：nav 键按压面——Rect 承载（Icon 叠顶；Press 换底即时反馈）。
    // 返回 (icon 触发节点, Rect 视觉节点)——press_map 登记 (icon, rect)。
    // 刀 B 结构修正（BadParent 修复 2026-11-15）：Rect 是叶子（add 白名单
    // 只许容器为父）——原「Icon 叠 Rect」嵌套必炸 BadParent（boot 实证：
    // launcher: desktop build err=BadParent 后 park）。改 Pad 承载双子
    // 叠加：Pad(键位) → {Rect(底色块), Icon(符号)}——Pad 对所有子同位
    // 全幅分配 = 叠加语义；Rect 先插 = 底，Icon 后插 = 盖上。press_map
    // 登记 (icon, rect)——Rect 即按压换底面。
    let mut nav_icon = |id: u16, label: &[u8]| -> Result<(NodeId, NodeId), UiError> {
        let cell = ui.add(
            nav_bar,
            ViewSpec {
                kind: ViewKind::Pad,
                key: 0,
                props: Props::Pad(PadProps { all: 0 }),
            },
        )?;
        let rect = ui.add(
            cell,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    w: Some(nav_key),
                    h: Some(nav_key),
                    bg: NAV_FACE_BG,
                    radius: 0,
                }),
            },
        )?;
        let n = ui.add(
            cell,
            ViewSpec {
                kind: ViewKind::Icon,
                key: 0,
                props: Props::Icon(IconProps {
                    id,
                    fg: ICON_FG,
                    w: Some(nav_key),
                    h: Some(nav_key),
                }),
            },
        )?;
        ui.set_a11y(n, ROLE_BUTTON, label)?;
        Ok((n, rect))
    };
    let (back, back_face) = nav_icon(icons::NAV_BACK, b"back")?;
    let (recents, recents_face) = nav_icon(icons::NAV_RECENTS, b"recents")?;
    let (home, home_face) = nav_icon(icons::NAV_HOME, b"home")?;
    let (rotate, rotate_face) = nav_icon(icons::NAV_ROTATE, b"rotate screen")?;
    let (power, power_face) = nav_icon(icons::NAV_POWER, b"power off")?;
    drop(nav_icon);
    // 刀 B：press_map 登记——**当前页**（页 0——build 时 snap 基线）20 格
    // （cell → press_face）+ 5 nav 键（icon → rect face）= 25 对 <
    // PRESS_MAP_MAX=28 帽。非当前页格不预登（曾全 4 页登记 85 对炸
    // NodeFull——boot 实证 2026-11-15）；翻页重登记归 frameloop（pager
    // rehang 后按新当前页刷新表）。
    let mut pm: Vec<(NodeId, NodeId)> = Vec::new();
    for s in slots[0].iter() {
        if let Some(pf) = s.press_face {
            pm.push((s.cell, pf));
        }
    }
    pm.push((back, back_face));
    pm.push((recents, recents_face));
    pm.push((home, home_face));
    pm.push((rotate, rotate_face));
    pm.push((power, power_face));
    ui.set_press_map(&pm)?;

    let mut d = Desktop {
        pager,
        slots,
        dock,
        clock: time_node,
        nav: NavNodes {
            back,
            recents,
            home,
            rotate,
            power,
            faces: [
                (back, back_face),
                (recents, recents_face),
                (home, home_face),
                (rotate, rotate_face),
                (power, power_face),
            ],
            key_px: nav_key,
        },
        page_nodes,
        a11y_page: u8::MAX,
        tabs: SysTabs { bar_host, bar_inst, tab_nodes, cur: 0 },
        sys: SysFrame {
            host: pages_host,
            w: vp_w,
            h: host_h,
            big,
            apps: None,
            setpg: None,
            filter: syspages::FILTER_ALL,
            boot,
            settings,
        },
        a11y_sys: u8::MAX,
    };
    rehang_view(ui, &mut d)?;
    Ok(d)
}

/// 波3-件⑤：系统页切换（整树装配面）。
pub fn switch_sys_page(ui: &mut UiMain, d: &mut Desktop, target: u8) -> Result<u8, UiError> {
    if target > 2 || target == d.tabs.cur {
        return Ok(d.tabs.cur);
    }
    if let Some(p) = d.sys.apps.take() {
        ui.remove(p.node)?;
    }
    if let Some(p) = d.sys.setpg.take() {
        ui.remove(p.node)?;
    }
    match target {
        1 => {
            let page = syspages::build_apps(
                ui,
                d.sys.host,
                d.sys.w,
                d.sys.h,
                d.sys.big,
                d.sys.filter,
            )?;
            d.sys.apps = Some(page);
        }
        2 => {
            let page = syspages::build_settings(
                ui,
                d.sys.host,
                d.sys.w,
                d.sys.h,
                d.sys.big,
                d.sys.boot,
                &d.sys.settings,
            )?;
            d.sys.setpg = Some(page);
        }
        _ => {}
    }
    rebuild_tab_bar(ui, d, target)?;
    d.tabs.cur = target;
    rehang_view(ui, d)?;
    Ok(target)
}

/// BottomTabs 实例整条重建（换选面——旧实例摘除 + 新实例入宿主顶层 +
/// a11y 三页位重挂）。
fn rebuild_tab_bar(ui: &mut UiMain, d: &mut Desktop, selected: u8) -> Result<(), UiError> {
    ui.remove(d.tabs.bar_inst)?;
    let (inst, nodes) = syspages::build_tab_bar(ui, d.tabs.bar_host, selected)?;
    let labels: [&[u8]; 3] = [b"home tab", b"apps tab", b"settings tab"];
    for (i, &tn) in nodes.iter().enumerate() {
        ui.set_a11y(tn, ROLE_BUTTON, labels[i])?;
    }
    d.tabs.bar_inst = inst;
    d.tabs.tab_nodes = nodes;
    Ok(())
}

/// 可见面重挂单源（a11y + press_map 整表）——sys 页切换/桌面翻页/建树尾
/// 三拍共用。幂等（同态零动作）。
///
/// - **a11y（帽 32 的预算重立法）**：底座常驻 = 状态栏 4 + dock 4 +
///   五键 5 + tab 三页位 3 = 16；页集随可见真相挂/摘——桌面页 = 页容器
///   + **有主格**（占位格 label 空、无 spawn 面——不占 a11y 槽；满页
///   超预算时确定性截断，帽内诚实收口，记 W3A-IMPL-SC）；应用列表页 =
///   List 容器 + 有主行；设置页 = 控件五件。
/// - **press_map（帽 28）**：桌面页 = 当前页 20 格 (cell→label)（波 2
///   批 A 落地形原样——翻页重登记即本面）；应用列表页 = 有主行
///   (pad/text→label) 双触发面 + 过滤词自映；设置页 = 控件→行标签
///   （控件不在 visual 白名单——反馈落标签 Text 换底）。整表替换语义。
pub fn rehang_view(ui: &mut UiMain, d: &mut Desktop) -> Result<(), UiError> {
    // —— a11y：sys 页集换挂 ——
    if d.a11y_sys != d.tabs.cur {
        match d.a11y_sys {
            1 => {
                if let Some(p) = d.sys.apps.as_ref() {
                    ui.take_a11y(p.list);
                    for r in p.rows.iter().take(p.rows_n) {
                        ui.take_a11y(r.text);
                    }
                }
            }
            2 => {
                if let Some(p) = d.sys.setpg.as_ref() {
                    ui.take_a11y(p.boot_toggle);
                    ui.take_a11y(p.theme_light);
                    ui.take_a11y(p.theme_dark);
                    ui.take_a11y(p.anim_seg);
                    ui.take_a11y(p.vol_slider);
                }
            }
            _ => {}
        }
        match d.tabs.cur {
            1 => {
                if let Some(p) = d.sys.apps.as_ref() {
                    ui.set_a11y(p.list, ROLE_LIST, b"apps")?;
                    for r in p.rows.iter().take(p.rows_n) {
                        if let Some(idx) = r.app {
                            let label = icons::registry()
                                .get(idx)
                                .map(|e| e.label_bytes())
                                .unwrap_or(b"");
                            ui.set_a11y(r.text, berry_ui::ROLE_LIST_ITEM, label)?;
                        }
                    }
                }
            }
            2 => {
                if let Some(p) = d.sys.setpg.as_ref() {
                    ui.set_a11y(p.boot_toggle, berry_ui::ROLE_TOGGLE, b"verbose boot")?;
                    ui.set_a11y(p.theme_light, ROLE_BUTTON, b"theme light")?;
                    ui.set_a11y(p.theme_dark, ROLE_BUTTON, b"theme dark")?;
                    ui.set_a11y(p.anim_seg, ROLE_BUTTON, b"animation")?;
                    ui.set_a11y(p.vol_slider, ROLE_BUTTON, b"volume")?;
                }
            }
            _ => {}
        }
        d.a11y_sys = d.tabs.cur;
    }
    // —— a11y：桌面页集（sys!=0 时整集摘除——不可见即不挂）——
    let page = d.pager.page();
    if d.tabs.cur == 0 {
        rehang_a11y(ui, d, page)?;
    } else if (d.a11y_page as usize) < PAGES {
        ui.take_a11y(d.page_nodes[d.a11y_page as usize]);
        for s in &d.slots[d.a11y_page as usize] {
            ui.take_a11y(s.cell);
        }
        d.a11y_page = u8::MAX;
    }
    // —— press_map：整表替换 ——
    rehang_press(ui, d)?;
    Ok(())
}

/// press_map 整表重挂（当前可见页的按压反馈面——帽 28 内）。
fn rehang_press(ui: &mut UiMain, d: &mut Desktop) -> Result<(), UiError> {
    // 占位对（NodeId 无公开构造——页容器 0 号作哑元，全部槽位在写入
    // 前不读）。
    let dummy = d.page_nodes[0];
    let mut entries = [(dummy, dummy); berry_ui::PRESS_MAP_MAX];
    let n = match d.tabs.cur {
        1 => {
            let mut n = 0usize;
            if let Some(p) = d.sys.apps.as_ref() {
                n = syspages::row_press_entries(p, &mut entries);
                for &wn in p.words.iter() {
                    if n >= entries.len() {
                        break;
                    }
                    entries[n] = (wn, wn);
                    n += 1;
                }
            }
            n
        }
        2 => {
            let mut n = 0usize;
            if let Some(p) = d.sys.setpg.as_ref() {
                let pairs = [
                    (p.boot_toggle, p.boot_label),
                    (p.theme_light, p.theme_label),
                    (p.theme_dark, p.theme_label),
                    (p.anim_seg, p.anim_label),
                    (p.vol_slider, p.vol_label),
                ];
                for (trig, vis) in pairs {
                    if n >= entries.len() {
                        break;
                    }
                    entries[n] = (trig, vis);
                    n += 1;
                }
            }
            n
        }
        _ => {
            // 桌面页：当前页 20 格 (cell→label)——波 2 批 A 建树尾登记形
            // 原样（trigger = 格容器 Pad；Icon/Text 直击不换底 = 原面）。
            let page = d.pager.page() as usize;
            let mut n = 0usize;
            for s in d.slots[page].iter() {
                if n >= entries.len() {
                    break;
                }
                entries[n] = (s.cell, s.label_node);
                n += 1;
            }
            n
        }
    };
    ui.set_press_map(&entries[..n])?;
    Ok(())
}

/// a11y 翻页重挂（裁决八：当前页 12 格 ROLE_BUTTON take/set 重挂）。
/// W2 帧循环在 `pager.on_msg` 返 true 且页值变化时调用；幂等（同页直返）。
/// 刀 D 修订：页容器 ROLE_LIST 同批 take/set（4 页全挂 → 当前页单挂——
/// 32 帽腾 3 槽给三键导航，常态恰满：4 状态 + 1 页容器 + 20 格 +
/// 4 dock + 3 键）。
/// a11y 桌面页集重挂（裁决八形——`rehang_view` 的桌面页半程）。
/// W2 帧循环在 `pager.on_msg` 返 true 且页值变化时经 `rehang_view` 调
/// 用；幂等（同页直返）。
/// 刀 D 修订 + **波3-件⑤ 预算重立法**：页容器 ROLE_LIST + **有主格**
/// ROLE_BUTTON（占位格无 spawn 面、label 空——不占槽）；有主格超
/// [`A11Y_CELL_CAP`] 时确定性截断（底座 16 + 容器 1 + 帽 32 的差值
/// ——满注册表下帽内诚实收口，截断面记 W3A-IMPL-SC）。
pub fn rehang_a11y(ui: &mut UiMain, d: &mut Desktop, page: u8) -> Result<(), UiError> {
    if page as usize >= PAGES || page == d.a11y_page {
        return Ok(());
    }
    if (d.a11y_page as usize) < PAGES {
        ui.take_a11y(d.page_nodes[d.a11y_page as usize]);
        for s in &d.slots[d.a11y_page as usize] {
            ui.take_a11y(s.cell);
        }
    }
    ui.set_a11y(d.page_nodes[page as usize], ROLE_LIST, PAGE_LABELS[page as usize])?;
    let mut mounted = 0usize;
    for s in &d.slots[page as usize] {
        if s.app.is_none() {
            continue; // 占位格：无 spawn 面、label 空——不占 a11y 槽
        }
        if mounted >= A11Y_CELL_CAP {
            break; // 帽内截断（确定性：行主序前 N 格）
        }
        ui.set_a11y(s.cell, ROLE_BUTTON, s.label_buf.as_bytes())?;
        mounted += 1;
    }
    // 刀 B 补面（NodeFull 修复 2026-11-15）：press_map 同步刷新——旧页格
    // 出表、新页 20 格 + 5 nav 键入表（恒 25 对 < PRESS_MAP_MAX=28 帽）。
    // nav 键对子在建树时由 build 收集进 d.nav_faces——表刷新保持完整。
    let mut pm: Vec<(NodeId, NodeId)> = Vec::new();
    for s in &d.slots[page as usize] {
        if let Some(pf) = s.press_face {
            pm.push((s.cell, pf));
        }
    }
    for (trig, vis) in d.nav.faces.iter() {
        pm.push((*trig, *vis));
    }
    ui.set_press_map(&pm)?;
    d.a11y_page = page;
    Ok(())
}
