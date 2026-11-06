//! desktop —— 桌面静态面（GUI-8 段③ W1，docs/GUI-8实施设计草案.md §4
//! 段③「布局桩」）。
//!
//! 责任：一次性建桌面保留树——壁纸三带 + 状态栏 + SwipePager 四页 ×
//! Grid 4×3 槽板 + a11y 骨架（18/32）。数据源 = berry-layout 布局 ×
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
//! - **裁决八（a11y 18/32）**：状态栏 2 件 ROLE_TEXT + 4 页 Grid 容器
//!   ROLE_LIST + 当前页 12 格容器 ROLE_BUTTON（label = app 名/幽灵 id/
//!   "-"）= 18/32；翻页 take/set 重挂当前页（`rehang_a11y`）。press_map
//!   v1 不做（砍单——视觉反馈归 W2 spawn 回执标记行）。
//! - **裁决 D（几何）**：屏幕几何全经 wm GETINFO 动态（`ui.viewport()`
//!   ——Root/带高/Grid 高全派生），禁硬编 800×600；字号档/图标 px 等
//!   字体面常量不属屏幕参数，不在此列。
//!
//! 树规模（裁决八实树数义务；对抗审查勘正——W1 初值漏算 48 个格
//! label Text）：1 Root + 3 壁纸 + 1 内容列 + 状态栏 5 + pager 10
//! （track+4 页+圆点行+4 圆点）+ 4×(Grid+12 格×3[cell+label+icon 位])
//! = **120 节点** < MAX_NODES=512（在册图标节点 +48 上限 = 168，
//! 裕量足）。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_layout::{CELL_MAX, PAGE_MAX};
use berry_ui::desc::{
    ColProps, GridProps, IconProps, PadProps, RectProps, RowProps, TextProps, TextStyle,
};
use berry_ui::layout::CrossAlign;
use berry_ui::swipe::SwipePager;
use berry_ui::{
    NodeId, Props, TextBuf, UiError, UiMain, ViewKind, ViewSpec, ROLE_BUTTON, ROLE_TEXT,
};

use crate::icons::{AppEntry, NO_ICON};
use crate::layout_io::OwnedLayout;

// —— 桌面视窗常量（默认布局公式同源消费——layout_io::default_layout）——

/// 桌面页数 = berry-layout `PAGE_MAX` 权威 4（裁决三定谳）。
pub(crate) const PAGES: usize = PAGE_MAX as usize;
/// 每页网格列数（草案 §4 段③「Grid 4×3」）。
pub(crate) const GRID_COLS: usize = 4;
/// 每页网格行数。
pub(crate) const GRID_ROWS: usize = 3;
/// 每页槽数 = 4×3 = 12。
pub(crate) const SLOTS_PER_PAGE: usize = GRID_COLS * GRID_ROWS;

/// 48 = 4 页 × 12 格恰满（裁决三容量公式）——编译期钉视窗与 berry-layout
/// 容量域一致性。
const _: () = assert!(CELL_MAX == PAGES * SLOTS_PER_PAGE, "CELL_MAX = 4页×12格 恰满");

// —— 派生几何（字体档单源派生，非屏幕参数硬编——裁决 D 注记）——

/// 状态栏高（800 宽 8x16 双行+6px pad；1280 宽 16x32 双行+8px pad）。
/// 圆点行高（swipe.rs DOT_SIZE=8 镜像——彼处私有常量，改彼处须同步此处）。
const DOT_H: u32 = 8;
/// 内容区底缘留白（800/1280 两档在 build 内派生）。
/// 格内图标显示边长（IconProps w/h 定值——v1 无缩放；72px 源图标直出）。
const ICON_PX: u32 = 96;
/// 状态栏下强调线高（800/1280 两档在 build 内派生）。
/// 壁纸竖向渐变色带数（Rect 无渐变原语——多带近似；奇高余量归末带）。
const WALLPAPER_BANDS: u32 = 48;

/// ROLE_LIST —— berry-ui 角色常量域（wire u8 0..=5 在册）之外的**开放
/// 值域 host 扩展**（lib.rs 角色常量族头注明文「host 可扩」）：6 =
/// launcher 扩 LIST（页 Grid 容器）。
const ROLE_LIST: u8 = 6;

/// 状态栏左件（系统名）。
const STATUS_NAME: &[u8] = b"BerryOS";
/// 状态栏右件（时钟占位——TODO(W2)：真时钟经 berry_clock_read 摆字）。
const STATUS_CLOCK: &[u8] = b"12:00";
/// a11y 页容器 label。
const PAGE_LABELS: [&[u8]; PAGES] = [b"page 0", b"page 1", b"page 2", b"page 3"];
/// 空槽占位 label。
const PLACEHOLDER: &[u8] = b"";
/// 桌面品牌件（page0 slot2 空位）：大字号 wordmark + 小字副标，填掉
/// 1280×720 顶行右侧的大片死白；非 app、不可 spawn（app=None）。
const WIDGET_TITLE: &[u8] = b"BerryOS";
const WIDGET_SUB: &[u8] = b"DESKTOP";

/// 壁纸/文本配色（裁决六三带 + 白系前景；stub 常量——位图通道 v2）。
const WALLPAPER_TOP: Color = Color { r: 24, g: 34, b: 58 }; // 夜空蓝
const WALLPAPER_BOTTOM: Color = Color { r: 86, g: 98, b: 124 }; // 冷灰蓝
const STATUS_BG: Color = Color { r: 10, g: 14, b: 24 }; // 状态栏深色带
const ACCENT: Color = Color { r: 82, g: 156, b: 255 }; // 状态栏下 2px 强调线
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

/// 线性插值两色（`t` = 0..=den；Q16 不需要——整数通道足够，18 带近似）。
fn lerp_color(a: Color, b: Color, t: u32, den: u32) -> Color {
    if den == 0 {
        return b;
    }
    let f = |x: u8, y: u8| -> u8 {
        let x = x as u32;
        let y = y as u32;
        ((x * (den - t) + y * t) / den) as u8
    };
    Color {
        r: f(a.r, b.r),
        g: f(a.g, b.g),
        b: f(a.b, b.b),
    }
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

/// 桌面建树（main 单次调用；几何全 `ui.viewport()` 动态——裁决 D）。
/// Err = 容量/文本域违例（NodeFull/NodeBad/TextTooLong 族）——调用方
/// `fail` 收口（eprintln + park；监督重启归 svcmgr 域，本进程不自救）。
pub fn build(ui: &mut UiMain, layout: &OwnedLayout, reg: &[AppEntry]) -> Result<Desktop, UiError> {
    // 宽不消费：三带/Grid 宽全 None 随父拉满（Root 子 avail = viewport
    // 宽——裁决六满幅语义），仅带高/Grid 高需显式派生。
    let (vp_w, vp_h) = ui.viewport();
    // 16:9 桌面（1280×720）与旧 4:3（800×600）两档：字号/状态栏/格隙
    // 随视口走；图标源 72px 固定（v1 无运行时缩放），窄屏也不溢出格。
    let big = vp_w >= 1000 || vp_h >= 1000;
    let status_h = if big { 72 } else { 44 };
    let status_pad = if big { 12 } else { 6 };
    let accent_h = if big { 4 } else { 2 };
    let grid_gap = if big { 24 } else { 12 };
    let cell_gap = if big { 8 } else { 4 };
    let bottom_pad = if big { 16 } else { 8 };
    let status_size = if big { FontSize::S32x48 } else { FontSize::S16x32 };
    let label_size = if big { FontSize::S32x48 } else { FontSize::S16x32 };
    let root = ui.root();

    // —— 壁纸竖向多带渐变（Rect 无渐变原语；18 带在 TCG 下仍是一次
    //    整帧 fill，零资产/零缩放）。插入序 = 自底向上 z 序：先插者先绘。
    let wall = ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: None,
                h: Some(vp_h),
                cross: CrossAlign::Stretch,
            }),
        },
    )?;
    let band_h = vp_h / WALLPAPER_BANDS;
    for i in 0..WALLPAPER_BANDS {
        let h = if i + 1 == WALLPAPER_BANDS {
            vp_h.saturating_sub(band_h.saturating_mul(WALLPAPER_BANDS - 1))
        } else {
            band_h
        };
        ui.add(
            wall,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: lerp_color(WALLPAPER_TOP, WALLPAPER_BOTTOM, i, WALLPAPER_BANDS - 1),
                    w: None,
                    h: Some(h),
                }),
            },
        )?;
    }
    // 状态栏底带（Root 直子；内容列的状态栏 Col 叠在其上，同高）。
    ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps {
                bg: STATUS_BG,
                w: None,
                h: Some(status_h),
            }),
        },
    )?;

    // —— 内容列（第 4 个 Root 子 = z 序顶；透明底——三带透出）：
    //    状态栏 Col + pager(track + 圆点行) 纵向叠放 ——
    let content = ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: None,
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    )?;
    let status = ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: None,
                h: Some(status_h),
                cross: CrossAlign::Stretch,
            }),
        },
    )?;
    let pad = ui.add(
        status,
        ViewSpec {
            kind: ViewKind::Pad,
            key: 0,
            props: Props::Pad(PadProps { all: status_pad }),
        },
    )?;
    let srow = ui.add(
        pad,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                // 时间右对齐：宽 - 两侧 pad - 两串字宽；窄屏饱和 0。
                gap: vp_w.saturating_sub(
                    status_pad * 2
                        + (STATUS_NAME.len() + STATUS_CLOCK.len()) as u32
                            * status_size.cell_w() as u32,
                ),
                w: None,
                h: None,
                cross: CrossAlign::Start,
            }),
        },
    )?;
    // 状态栏 2 件 ROLE_TEXT（裁决八；时钟占位——W2 真时钟）。
    let t1 = ui.add(srow, text_spec(STATUS_NAME, TEXT_FG, status_size)?)?;
    ui.set_a11y(t1, ROLE_TEXT, STATUS_NAME)?;
    let t2 = ui.add(srow, text_spec(STATUS_CLOCK, TEXT_FG, status_size)?)?;
    ui.set_a11y(t2, ROLE_TEXT, STATUS_CLOCK)?;

    // 状态栏下 2px 强调线（内容列直子；色带逐帧只读）。
    ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps { bg: ACCENT, w: None, h: Some(accent_h) }),
        },
    )?;

    // —— 分页器建树（attach 自管：track Row(w=viewport) + 页 Col×4 +
    //    圆点行 + 圆点×4——swipe.rs；页宽 = viewport 动态）——
    let mut pager = SwipePager::new(PAGES as u8);
    pager.attach(ui, content)?;

    // —— 布局 × 注册表 → 槽计划折算（先算后建——建树单源）——
    let mut plan: [[SlotPlan<'_>; SLOTS_PER_PAGE]; PAGES] =
        [[SlotPlan { label: PLACEHOLDER, icon: None, app: None }; SLOTS_PER_PAGE]; PAGES];
    for c in layout.cells() {
        // 桌面视窗 = 4 页 × 行 0..3 × 列 0..4；布局域 8×8——窗外格
        // 不渲染（渲染面视窗裁剪，持久化数据保留——重装 app 后再现）。
        if c.page as usize >= PAGES
            || c.row as usize >= GRID_ROWS
            || c.col as usize >= GRID_COLS
        {
            continue;
        }
        let s = c.row as usize * GRID_COLS + c.col as usize;
        plan[c.page as usize][s] = match reg.iter().position(|e| e.id_bytes() == c.app_bytes()) {
            Some(i) => SlotPlan {
                label: reg[i].label_bytes(),
                icon: icon_of(&reg[i]),
                app: Some(i),
            },
            // 幽灵格：布局在册、注册表缺席——label 显示 id（诚实可见），
            // 不可 spawn。
            None => SlotPlan { label: c.app_bytes(), icon: None, app: None },
        };
    }

    // —— 每页 Grid 4×3 槽板（格 = Col[Icon?, Text]；Center 十字居中）——
    // Grid 高定值 = 视口派生（Grid 主轴内在不吃 avail——Col/Row/Grid
    // 主轴恒内在，定值才撑满；宽随页 Col 拉满 viewport）。
    let grid_h = vp_h.saturating_sub(status_h + accent_h + DOT_H + bottom_pad);
    // 格高 = Grid 轨高（与 berry-ui §4.2 place 的 track_h 同式：等分轨 −
    // 行间 gap）。**必须定高**：Cell 是 Col 容器，`h: None` 时引擎按子
    // 内在聚合（icon 48 + gap 4 + label 16 = 68）落界，格只占轨首 68px，
    // 轨余下空间命中落到 Grid 容器（Click 无槽可映射——smoke 实抓：
    // QMP 点轨心 → node=Grid → CLICK 永缺）。定高把命中面铺满槽（透明
    // 容器零视觉差），a11y ROLE_BUTTON 界同步为整槽。
    let cell_h = (grid_h.saturating_sub((GRID_ROWS as u32 - 1) * grid_gap))
        / GRID_ROWS as u32;
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
                    w: None,
                    h: Some(grid_h),
                }),
            },
        )?;
        // 4 页 Grid 容器 ROLE_LIST（翻页不变——恒挂；裁决八）。
        ui.set_a11y(grid, ROLE_LIST, PAGE_LABELS[p])?;
        for s in 0..SLOTS_PER_PAGE {
            let pn = plan[p][s];
            let cell = ui.add(
                grid,
                ViewSpec {
                    kind: ViewKind::Col,
                    key: 0,
                    props: Props::Col(ColProps {
                        gap: cell_gap,
                        w: None,
                        h: Some(cell_h),
                        cross: CrossAlign::Center,
                    }),
                },
            )?;
            // 仅稠密域内 id 建 Icon 节点——IconSource 真供件契约：域外
            // id 查 dims/pixels = debug_assert loud（注册 bug）；占位/
            // 无图标/幽灵格零节点零查表（icons.rs 头注契约面）。
            let icon_node = match pn.icon {
                Some(id) => Some(
                    ui.add(
                        cell,
                        ViewSpec {
                            kind: ViewKind::Icon,
                            key: 0,
                            props: Props::Icon(IconProps {
                                id,
                                fg: ICON_FG,
                                w: Some(ICON_PX),
                                h: Some(ICON_PX),
                            }),
                        },
                    )?,
                ),
                None => None,
            };
            let label_node = ui.add(cell, text_spec(pn.label, LABEL_FG, label_size)?)?;
            // page0 / slot2：品牌 wordmark 件（顶行右侧视觉锚点；不是 app）。
            if p == 0 && s == 2 && pn.app.is_none() {
                let title_size = if big { FontSize::S32x48 } else { FontSize::S16x32 };
                ui.add(cell, text_spec(WIDGET_TITLE, TEXT_FG, title_size)?)?;
                ui.add(cell, text_spec(WIDGET_SUB, ACCENT, FontSize::S16x32)?)?;
            }
            // page0 / slot5（第二行中间）：大时钟 widget，填中段空白。
            if p == 0 && s == 5 && pn.app.is_none() {
                let clock_size = if big { FontSize::S32x48 } else { FontSize::S16x32 };
                ui.add(cell, text_spec(b"12:00", TEXT_FG, clock_size)?)?;
                ui.add(cell, text_spec(WIDGET_SUB, ACCENT, FontSize::S16x32)?)?;
            }
            slots[p][s] = Some(Slot {
                cell,
                label_node,
                icon: icon_node,
                app: pn.app,
                label_buf: TextBuf::new(pn.label)?,
            });
        }
    }
    // 槽填满不变量（建槽循环恰覆盖 (p,s) 全域）——结构不可达防御面。
    let slots = slots.map(|page| page.map(|s| s.expect("slot 填满不变量破缺")));

    let mut d = Desktop { pager, slots, a11y_page: u8::MAX };
    // 初挂页 0（a11y 12 ROLE_BUTTON——合计 2+4+12 = 18/32，裁决八）。
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
