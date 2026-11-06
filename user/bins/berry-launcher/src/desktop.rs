//! desktop —— 桌面静态面（GUI-8 段③ W1，docs/GUI-8实施设计草案.md §4
//! 段③「布局桩」）。
//!
//! 责任：一次性建桌面保留树——OH 官方壁纸 + phone 状态栏 + SwipePager
//! 四页 × Grid 5×4 槽板 + 常驻 Dock + a11y 骨架（32/32）。数据源 = berry-layout 布局 ×
//! icons 注册表（注册表空 → 布局 0 cell → 全占位格 label "-"，W1 冻结
//! 相常态形态）。**帧循环/翻页/点击全无**——`frameloop`（W2）经本模块
//! 公开面驱动：`pager`（on_msg/tick）+ `slots` 槽表（命中映射/spawn）+
//! `rehang_a11y`（翻页重挂当前页）。
//!
//! 裁决出处（草案 §3）：
//!
//! - **裁决六（壁纸形态）**：树内 Rect 带状壁纸，Root 直接子满幅叠放
//!   （layout.rs `place_children` 尾臂：Root 子各自 (0,0) 起满幅 avail）。
//!   插入序 = 自底向上 z 序（paint.rs 前序头先发 = 先插者在底）：石板
//!   全幅底 → 深蓝灰盖上 2/3 → 状态栏黑带顶——三 Rect 零资产零缩放。
//! - **裁决四-e（归属）**：状态栏/壁纸 = launcher 窗口内容层（非 wmd
//!   系统窗）——D3 单 wm 客户端不变量保持，gfxdemo `WM ok` 零连带。
//! - **裁决八（a11y OH phone 形 32/32）**：状态栏 4 件 ROLE_TEXT + 4 页
//!   Grid 容器 ROLE_LIST + 当前页 20 格 ROLE_BUTTON + 4 个 Dock 位
//!   ROLE_BUTTON = 32/32；翻页 take/set 重挂当前页（`rehang_a11y`）。
//!   press_map v1 不做（视觉反馈归 W2 spawn 回执标记行）。
//! - **裁决 D（几何）**：屏幕几何全经 wm GETINFO 动态（`ui.viewport()`
//!   ——Root/带高/Grid 高全派生），禁硬编 800×600；字号档/图标 px 等
//!   字体面常量不属屏幕参数，不在此列。
//!
//! 树规模（最坏满图标）：1 Root + 壁纸 1 + 内容列 1 + 状态栏 10 +
//! grid top 1 + pager 10 + 4×(Grid+20 Pad+20 内Col+20 Icon+20 Text)
//! + filler 1 + Dock 5 = **354 节点** < MAX_NODES=512。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_layout::{CELL_MAX, PAGE_MAX};
use berry_ui::desc::{
    ColProps, GridProps, IconProps, PadProps, RowProps, TextProps, TextStyle,
};
use berry_ui::layout::CrossAlign;
use berry_ui::swipe::SwipePager;
use berry_ui::{
    NodeId, Props, TextBuf, UiError, UiMain, ViewKind, ViewSpec, ROLE_BUTTON, ROLE_TEXT,
};

use crate::icons;
use crate::icons::{AppEntry, NO_ICON};
use crate::layout_io::OwnedLayout;

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
/// 格内图标目标边长（OH phone icon 54vp×3；108px 源经 ScaledArgb 采样）。
const ICON_PX: u32 = 162;

// —— OpenHarmony phone launcher 几何（applications_launcher 源仓
//    PhonePresetStyleConstants / PhoneSmartDockStyleConfig；1080×1920
//    = 360×640vp @3x 折算）——
/// 密度（1080 物理宽 / 360vp 逻辑宽）。
const OH_DENSITY: u32 = 3;
/// 状态栏可见高 39vp（phone statusbar float；launcher 布局保留 44vp）。
const OH_STATUS_H: u32 = 39;
/// 状态栏左右 padding 24vp。
const OH_STATUS_PAD: u32 = 24;
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
/// Dock 下方保留区 45vp（sysUI bottom 36vp + dock margin 9vp）。
const OH_DOCK_BOTTOM: u32 = 45;
/// Dock 源资产密度（目标 3x；源段 167×156 → 目标段 ~250×234）。
const DOCK_SRC_DENSITY: u32 = 2;
/// Dock 目标分段宽（167×1.5 四舍分配，和 = 1002）。
const DOCK_TARGET_W: [u32; DOCK_SLOTS] = [251, 250, 250, 251];
/// Dock 常驻图标数（PhoneSmartDock max=4）。
const DOCK_SLOTS: usize = 4;

/// ROLE_LIST —— berry-ui 角色常量域（wire u8 0..=5 在册）之外的**开放
/// 值域 host 扩展**（lib.rs 角色常量族头注明文「host 可扩」）：6 =
/// launcher 扩 LIST（页 Grid 容器）。
const ROLE_LIST: u8 = 6;

/// 状态栏右件（时钟占位——TODO(W2)：真时钟经 berry_clock_read 摆字）。
const STATUS_CLOCK: &[u8] = b"12:00";
/// a11y 页容器 label。
const PAGE_LABELS: [&[u8]; PAGES] = [b"page 0", b"page 1", b"page 2", b"page 3"];
/// 空槽占位 label。
const PLACEHOLDER: &[u8] = b"";
/// 常驻 Dock app（Contacts/Messages/Browser/Camera；从网格去重）。
/// GUI 验收双 app（官方桌面常态隐藏，只保留后台/spawn 面）。
const TEST_IDS: [&[u8]; 2] = [b"APPDEMO", b"APPQUIET"];
/// OH PhonePresetDockItem 官方顺序。
const DOCK_IDS: [&[u8]; DOCK_SLOTS] = [b"CONTACTS", b"PHOTOS", b"SETTINGS", b"MMS"];

/// 文本配色（白色 = systemui TintStateManager 默认 contentColor）。
const TEXT_FG: Color = Color { r: 240, g: 240, b: 244 };
const LABEL_FG: Color = Color { r: 228, g: 228, b: 232 };
const ICON_FG: Color = Color { r: 230, g: 230, b: 235 };

/// 一格桌面的建树产物（W2 帧循环消费面）。
#[derive(Clone, Copy)]
pub struct Slot {
    /// 格容器 Col（a11y ROLE_BUTTON 锚 + hit 容器）。
    pub cell: NodeId,
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
    /// a11y 当前挂载页（`u8::MAX` 哨兵 = 未挂——build 尾经 rehang(0) 初挂）。
    a11y_page: u8,
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

/// S8x16 文本叶子 spec（label/状态栏共用字号档；超 `TEXT_INLINE`=64B
/// loud Err——id≤24/label≤64 域内不可达）。
fn text_spec(text: &[u8], fg: Color, scale: FontSize) -> Result<ViewSpec, UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(text)?,
        }),
    })
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

/// 状态栏 Icon spec（内建 id；宽高来自 OH 资产真值，不做运行时缩放）。
fn status_icon_spec(id: u16) -> Result<ViewSpec, UiError> {
    let (w, h) = match id {
        icons::STATUS_SIGNAL => icons::STATUS_SIGNAL_DIM,
        icons::STATUS_WIFI => icons::STATUS_WIFI_DIM,
        icons::STATUS_BATTERY => icons::STATUS_BATTERY_DIM,
        _ => return Err(UiError::NodeBad),
    };
    Ok(ViewSpec {
        kind: ViewKind::Icon,
        key: 0,
        props: Props::Icon(IconProps { id, fg: TEXT_FG, w: Some(w), h: Some(h) }),
    })
}

/// 桌面建树（main 单次调用；几何全 `ui.viewport()` 动态——裁决 D）。
/// Err = 容量/文本域违例（NodeFull/NodeBad/TextTooLong 族）——调用方
/// `fail` 收口（eprintln + park；监督重启归 svcmgr 域，本进程不自救）。
pub fn build(ui: &mut UiMain, layout: &OwnedLayout, reg: &[AppEntry]) -> Result<Desktop, UiError> {
    let (vp_w, vp_h) = ui.viewport();
    // OH phone 固定几何只给 1080×1920（360×640vp @3x）；横屏/调试档走
    // 紧凑分支，防 Dock/top 常量把 1280×720 挤出视口。
    let big = vp_w >= 600 && vp_h >= 1800;

    // OpenHarmony phone 档：1080×1920 = 360×640vp @3x，直接消费 OH
    // launcher 源仓 phone 常量。800×600 调试档保留紧凑布局。
    let status_h = if big { OH_STATUS_H * OH_DENSITY } else { 44 };
    let status_size = if big { FontSize::S32x48 } else { FontSize::S16x32 };
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

    // 状态栏：左时间、右 signal/wifi/battery（OH systemui phone 资产）。
    let status = ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(vp_w),
                h: Some(status_h),
                cross: CrossAlign::Stretch,
            }),
        },
    )?;
    // OH systemui phone：左右 24vp；左 = signal/wifi，右 = battery/clock
    // （StatusbarConfig LayoutGroups 1/3；clock rightmost）。
    let status_pad = if big { OH_STATUS_PAD * OH_DENSITY } else { 6 };
    let time_w = STATUS_CLOCK.len() as u32 * status_size.cell_w() as u32;
    let status_icon_gap = if big { 15 } else { 6 };
    let left_w = icons::STATUS_SIGNAL_DIM.0
        + status_icon_gap
        + icons::STATUS_WIFI_DIM.0;
    let right_w = icons::STATUS_BATTERY_DIM.0 + status_icon_gap + time_w;
    let status_flex = vp_w.saturating_sub(status_pad * 2 + left_w + right_w);
    let srow = ui.add(
        status,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(vp_w),
                h: Some(status_h),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let add_status_spacer = |ui: &mut UiMain, w: u32| -> Result<NodeId, UiError> {
        ui.add(
            srow,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: Some(w),
                    h: Some(1),
                    cross: CrossAlign::Center,
                }),
            },
        )
    };
    add_status_spacer(ui, status_pad)?;
    let left_group = ui.add(
        srow,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: status_icon_gap,
                w: Some(left_w),
                h: None,
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let sig = ui.add(left_group, status_icon_spec(icons::STATUS_SIGNAL)?)?;
    ui.set_a11y(sig, ROLE_TEXT, b"signal full")?;
    let wifi = ui.add(left_group, status_icon_spec(icons::STATUS_WIFI)?)?;
    ui.set_a11y(wifi, ROLE_TEXT, b"wifi full")?;
    add_status_spacer(ui, status_flex)?;
    let right_group = ui.add(
        srow,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: status_icon_gap,
                w: Some(right_w),
                h: None,
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let battery = ui.add(right_group, status_icon_spec(icons::STATUS_BATTERY)?)?;
    ui.set_a11y(battery, ROLE_TEXT, b"battery 100")?;
    let time_node = ui.add(right_group, text_spec(STATUS_CLOCK, TEXT_FG, status_size)?)?;
    ui.set_a11y(time_node, ROLE_TEXT, STATUS_CLOCK)?;
    add_status_spacer(ui, status_pad)?;

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

    let mut pager = SwipePager::new(PAGES as u8);
    pager.attach(ui, content)?;

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
        // 小屏无 Dock：只让 16px 视觉点行，OH 96px 指示区预留仅 phone 档。
        vp_h.saturating_sub(status_h + grid_top + DOT_ROW_H + 8)
    };
    let cell_h = if big {
        OH_ITEM_H * OH_DENSITY
    } else {
        (grid_h.saturating_sub((GRID_ROWS as u32 - 1) * grid_gap)) / GRID_ROWS as u32
    };
    let mut slots: [[Option<Slot>; SLOTS_PER_PAGE]; PAGES] = [[None; SLOTS_PER_PAGE]; PAGES];
    for p in 0..PAGES {
        let page = pager.page_node(p as u8).ok_or(UiError::NodeBad)?;
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
        ui.set_a11y(grid, ROLE_LIST, PAGE_LABELS[p])?;
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
                label_node,
                icon: icon_node,
                app: pn.app,
                label_buf: TextBuf::new(pn.label)?,
            });
        }
    }
    let slots = slots.map(|page| page.map(|s| s.expect("slot 填满不变量破缺")));

    // OH SmartDock：官方 1002×234 条切成四段内建 Icon——每段是一个命中
    // 面，拼回仍是整条 radius66 圆角 dock（无容器背景原语也不失真）。
    let dock_w = icons::DOCK_DIMS
        .iter()
        .map(|&(w, _)| w)
        .sum::<u32>()
        .saturating_mul(OH_DENSITY)
        / DOCK_SRC_DENSITY;
    if big {
        // SwipePager 点行是 16px 视觉件；OH 32vp 指示区差额并入 filler，
        // 保证 dock top = 1920 − 45vp×3 − 78vp×3 = 1551px。
        let before_dock = status_h + grid_top + grid_h + DOT_ROW_H;
        let target = vp_h.saturating_sub((OH_DOCK_BOTTOM + OH_DOCK_H) * OH_DENSITY);
        let filler = target.saturating_sub(before_dock);
        ui.add(
            content,
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
        content,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(if big { dock_w } else { 0 }),
                h: Some(if big { OH_DOCK_H * OH_DENSITY } else { 0 }),
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
                    id: icons::DOCK_FACE_IDS[i],
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
            label_node: icon_node,
            icon: None,
            app: entry_idx,
            label_buf: TextBuf::new(label)?,
        });
    }
    let dock: [Slot; DOCK_SLOTS] = dock.try_into().ok().expect("dock 恰四槽");

    let mut d = Desktop { pager, slots, dock, a11y_page: u8::MAX };
    rehang_a11y(ui, &mut d, 0)?;
    Ok(d)
}

/// a11y 翻页重挂（裁决八：当前页 12 格 ROLE_BUTTON take/set 重挂）。
/// W2 帧循环在 `pager.on_msg` 返 true 且页值变化时调用；幂等（同页直返）。
pub fn rehang_a11y(ui: &mut UiMain, d: &mut Desktop, page: u8) -> Result<(), UiError> {
    if page as usize >= PAGES || page == d.a11y_page {
        return Ok(());
    }
    if (d.a11y_page as usize) < PAGES {
        for s in &d.slots[d.a11y_page as usize] {
            ui.take_a11y(s.cell);
        }
    }
    for s in &d.slots[page as usize] {
        ui.set_a11y(s.cell, ROLE_BUTTON, s.label_buf.as_bytes())?;
    }
    d.a11y_page = page;
    Ok(())
}
