//! appdemo 画廊建树面（波 4 §2.8——`W4-RECON.md` §四 APPDEMO 行）。
//!
//! **使命**：berry-ui `pattern.rs` 模式件 + 引擎核心 ViewKind 的常驻控件
//! 画廊——每件一格（控件实体 + 名称 Text），Scroll 翻览。格子清单口径
//! 对齐 `user/berry-ui/tests/gallery2.rs` CELLS 表（波 2 画廊主树版）：
//! pattern 节 = pattern.rs 全部可渲染面位（gallery2 写作期配方族未移植、
//! 只能以 4 件值面格近似——配方移植 dd53bc039 后本画廊以真配方收编）；
//! kind 节 = gallery2 39 格 kind_* 的全集映射——14 件被 pattern 配方
//! 收编（divider/spacer/stack/step/badge/collapse/chip/fab/card/gauge/
//! qr/dragreorder/refresh/swipeitem 经配方格展览）、Scroll = 翻览容器
//! 本体、layout（Root/Col/Row/Pad）= 全页结构锚，余 23 件独立成格
//!（含 Seg kind 直载格——pattern::seg 配方走 Grid 组合，两形分立）。
//! Pad/Spacer 两结构 kind 无 v1 视觉面（Spacer flex 挂布局刀——
//! gallery2 kind_spacer 格同为 0×0 示意），如实注记不入格。
//!
//! **零手拼铁律**（W4-RECON §二）：卡片 = `pattern::card`、条目 =
//! `pattern::list_item`（Card kind 自绘卡面/阴影/圆角单源）、骨架 =
//! pattern 配方（app_bar/stack/spacer/divider）、按压反馈 = 引擎
//! press_map + paint::PressedVis 查表（Button 族）与 `widget_react`
//! （Toggle/Check/Radio/Seg/Tabs/Step/Select/Collapse/Chip/Slider）；
//! 本文件零手写 Rect 拼卡、零按压遮罩字面量；散见 Rect 仅两义——
//! theme token 色板展示格（kind_rect/stack/spacer 演示的视觉载体）与
//! pattern 叠层语（Stack 面层/bg 带——pattern.rs 自身同款习语）。文案
//! 全 ASCII（HD 档 95 字模域）。
//!
//! **v1 无 clip 诚实设计**（Scroll/List kind 已内嵌 clip，Stack/Col 无）：
//! Sheet 闭合面板下移 / Drawer 闭合面板左移 / Refresh 指示器上藏的
//! 「逃逸区」均以格内预留空带承载（spacer 巷道）——面板态位在相邻
//! 空白带内滑动，不越格覆盖邻格；SwipeItem 动作层右移出视口缘由
//! 光栅纹理界截断。
//!
//! **节点预算手账**（MAX_NODES=512 硬帽，NodeFull = loud Err）：页铬
//! ~13 + pattern 节 ~301（26 格）+ kind 节 ~156（23 格）+ 说明字 ~6
//! ≈ 476（宿主对拍实测 475，余 37——见 W4-IMPL-APPDEMO.md §四）。
//! a11y 表帽 32：pattern 配方内建尽力登记（表满静默跳过——pattern.rs
//! 契约），本文件不另登记。
//!
//! 本模块纯建树（berry-ui/berry-gfx 面，零 libc/sys 依赖）——与帧循环
//! （main.rs）分文件的理由：宿主对拍检查器可 include! 本文件整建树跑
//! NodeFull/文本域校验（bin 本体链 berry-libc 的 aarch64 ELF 专属 asm
//! 宿主不可跑，W4-IMPL-FILES.md §四同判）。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_ui::desc::{
    ButtonProps, ButtonStyle, CanvasProps, CheckProps, DatePickProps, EditProps, GaugeStyle,
    GridProps, IconProps, ImageProps, InputKind, InputProps, ListProps, Orientation, ProgressProps,
    ProgressStyle, RadioProps, RectProps, SbarProps, ScrollProps, SelectProps, SliderProps,
    TabsProps, TextBuf, TextProps, TextStyle, TimePickProps, TipPlacement, TipProps, ToggleProps,
};
use berry_ui::layout::CrossAlign;
use berry_ui::pattern::{self, DragReorder, NavHost, Refresh, Seg, Sheet, SwipeItem, TabBar, Drawer};
use berry_ui::theme::{self, Theme, Tokens};
use berry_ui::{IconSource, NodeId, Props, UiError, UiMain, ViewKind, ViewSpec};

// ---------------------------------------------------------------------------
// —— 几何常量 ——

/// 页缘水平留白（px）。
pub const PAGE_PAD: u32 = 24;

/// 格间距（Scroll 内容 Col gap——格与格的呼吸缝）。
const CELL_GAP: u32 = 24;

/// 格内缝（名称/内容/说明字之间）。
const CELL_GAP_INNER: u32 = 16;

/// 滚动内容尾部留白（尾格与内容底缘的呼吸位）。
const TAIL_PAD: u32 = 24;

/// QR 演示模块数（版本 1 域值 21——21+4×(v−1) 单源域内）。
const QR_MODULES: u32 = 21;

/// Sheet 面板高（px——120vp 档）。
const SHEET_H: u32 = 300;

/// Drawer 面板宽高（px——112vp × 100vp 档）。
const DRAWER_W: u32 = 336;
const DRAWER_H: u32 = 300;

// —— 组件事件 ID 面（props on_tap/on_change 携载——main.rs take_events
//    消费锚；1..=4 = 画廊无动作演示件（视觉按压即反馈），11.. = 动作件）——

/// 演示 Button/Chip/Fab 通用 on_tap（无动作消费——按压视觉即演示面）。
pub const EV_DEMO: u32 = 1;
/// Sheet 打开按钮。
pub const EV_SHEET_OPEN: u32 = 11;
/// Sheet 关闭按钮。
pub const EV_SHEET_CLOSE: u32 = 12;
/// Drawer 打开按钮。
pub const EV_DRAWER_OPEN: u32 = 13;
/// Drawer 关闭按钮。
pub const EV_DRAWER_CLOSE: u32 = 14;

// ---------------------------------------------------------------------------
// —— 应用内 glyph 供件（程序化绘制——设计稿 §3.3「v1 = berry-ui 程序化
//    绘制，不新增位图资产」；contacts crate 同款先例）——
//    Argb 1-bit 透明键（A==0 跳过），色值 theme token 单源打包；源
//    24px，pattern 配方请求 72px（24vp@3x）/48px 时由光栅 ScaledArgb
//    最近邻放大（硬边无走样）。

/// glyph 源边长（px）。
const GLYPH_N: usize = 24;

/// Image kind 演示的缩略图 res id（自域 0x02xx——pattern::icons 约定域外）。
const THUMB: u16 = 0x0200;

/// 强调底上的前景字色（pattern.rs 私有 const `ON_ACCENT` 按值镜像并
/// 注明单源——contacts crate 同款先例；theme OnPrimary 档收编后随迁）。
const ON_ACCENT: Color = Color { r: 255, g: 255, b: 255 };

/// Color → RGB 打包（const 面——theme 常量进 const 求值）。
const fn pack_rgb(c: Color) -> u32 {
    ((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32
}

/// 点到线段垂距² ≤ r²（整数：cross² ≤ r²·len² 且投影段内；contacts
/// crate `near_seg` 同形——端点退化按点距判）。
const fn near_seg(px: i32, py: i32, sx: i32, sy: i32, ex: i32, ey: i32, r2: i32) -> bool {
    let dx = ex - sx;
    let dy = ey - sy;
    let len2 = dx * dx + dy * dy;
    if len2 == 0 {
        return (px - sx) * (px - sx) + (py - sy) * (py - sy) <= r2;
    }
    let t = (px - sx) * dx + (py - sy) * dy;
    if t < 0 || t > len2 {
        return false;
    }
    let cross = dx * (py - sy) - dy * (px - sx);
    cross * cross <= r2 * len2
}

/// 矩形域内判定（闭开区间 [x0,x1)×[y0,y1)）。
const fn in_rect(x: i32, y: i32, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    x >= x0 && x < x1 && y >= y0 && y < y1
}

/// 圆环域（r0 ≤ 距心 < r1——齿圈面）。
const fn in_ring(x: i32, y: i32, cx: i32, cy: i32, r0: i32, r1: i32) -> bool {
    let d2 = (x - cx) * (x - cx) + (y - cy) * (y - cy);
    d2 >= r0 * r0 && d2 < r1 * r1
}

/// 线段族位图（const 求值落 rodata——零堆零运行期构造）。
const fn seg_bitmap<const K: usize>(segs: [(i32, i32, i32, i32); K], r2: i32, rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    let mut px = [0u32; GLYPH_N * GLYPH_N];
    let mut i = 0usize;
    while i < GLYPH_N * GLYPH_N {
        let x = (i % GLYPH_N) as i32;
        let y = (i / GLYPH_N) as i32;
        let mut on = false;
        let mut s = 0usize;
        while s < K {
            let (sx, sy, ex, ey) = segs[s];
            if near_seg(x, y, sx, sy, ex, ey, r2) {
                on = true;
                break;
            }
            s += 1;
        }
        px[i] = if on { 0xFF00_0000 | rgb } else { 0 };
        i += 1;
    }
    px
}

/// 返回箭头「<」（两臂 r=3px——72px 目标下 9px 硬边臂宽；contacts
/// build_back 同形）。
const fn back_px(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    seg_bitmap([(17, 5, 8, 12), (8, 12, 17, 19)], 9, rgb)
}

/// 勾形（两臂 r=3px——Step 已完成步点字形）。
const fn check_px(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    seg_bitmap([(4, 13, 9, 18), (9, 18, 19, 6)], 9, rgb)
}

/// 加号（两臂 r=2px——Fab/icon_button 常用字形）。
const fn add_px(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    seg_bitmap([(12, 5, 12, 19), (5, 12, 19, 12)], 4, rgb)
}

/// 环形箭头（Refresh 指示）：圆心 (12,12) r=7 的 9 段折线弧（左上
/// 60° 缺口）+ 缺口终点的两笔箭头羽。
const fn refresh_px(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    seg_bitmap(
        [
            (6, 9, 5, 12),
            (5, 12, 6, 15),
            (6, 15, 9, 18),
            (9, 18, 12, 19),
            (12, 19, 15, 18),
            (15, 18, 18, 15),
            (18, 15, 19, 12),
            (19, 12, 18, 9),
            (18, 9, 16, 6),
            (16, 6, 12, 5),
            (16, 6, 19, 9),
        ],
        4,
        rgb,
    )
}

/// 房形（BottomTabs Home 席）：两笔屋顶 + 三笔墙体 U（r=2px）。
const fn home_px(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    seg_bitmap(
        [(3, 11, 12, 3), (12, 3, 21, 11), (7, 10, 7, 20), (7, 20, 17, 20), (17, 20, 17, 10)],
        4,
        rgb,
    )
}

/// 四宫格（Grid 席）：2×2 实心 7×7 块。
const fn grid_px(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    let mut px = [0u32; GLYPH_N * GLYPH_N];
    let mut i = 0usize;
    while i < GLYPH_N * GLYPH_N {
        let x = (i % GLYPH_N) as i32;
        let y = (i / GLYPH_N) as i32;
        let on = in_rect(x, y, 4, 4, 11, 11)
            || in_rect(x, y, 13, 4, 20, 11)
            || in_rect(x, y, 4, 13, 11, 20)
            || in_rect(x, y, 13, 13, 20, 20);
        px[i] = if on { 0xFF00_0000 | rgb } else { 0 };
        i += 1;
    }
    px
}

/// 齿轮（Settings 席）：r∈[5,7) 齿圈 + 东西南北四块 2×4 齿辐。
const fn settings_px(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    let mut px = [0u32; GLYPH_N * GLYPH_N];
    let mut i = 0usize;
    while i < GLYPH_N * GLYPH_N {
        let x = (i % GLYPH_N) as i32;
        let y = (i / GLYPH_N) as i32;
        let on = in_ring(x, y, 12, 12, 5, 7)
            || in_rect(x, y, 11, 3, 13, 6)
            || in_rect(x, y, 11, 18, 13, 21)
            || in_rect(x, y, 3, 11, 6, 13)
            || in_rect(x, y, 18, 11, 21, 13);
        px[i] = if on { 0xFF00_0000 | rgb } else { 0 };
        i += 1;
    }
    px
}

/// Image kind 演示缩略图（24×24 边框 + 棋盘纹——确定性生成；gallery2
/// `thumb_pixels` 同形，色值改 theme token 单源）。
const fn thumb_px() -> [u32; GLYPH_N * GLYPH_N] {
    let edge = 0xFF00_0000 | pack_rgb(theme::FONT_SECONDARY);
    let a = 0xFF00_0000 | pack_rgb(theme::BG_COMP);
    let b = 0xFF00_0000 | pack_rgb(theme::BG_PAGE);
    let mut px = [0u32; GLYPH_N * GLYPH_N];
    let mut i = 0usize;
    while i < GLYPH_N * GLYPH_N {
        let x = (i % GLYPH_N) as i32;
        let y = (i / GLYPH_N) as i32;
        let edge_on = x == 0 || y == 0 || x == GLYPH_N as i32 - 1 || y == GLYPH_N as i32 - 1;
        px[i] = if edge_on {
            edge
        } else if ((x / 4 + y / 4) & 1) == 0 {
            a
        } else {
            b
        };
        i += 1;
    }
    px
}

/// glyph 表（const 求值落 rodata——ink = theme token 打包单源）。
static BACK_PX: [u32; GLYPH_N * GLYPH_N] = back_px(pack_rgb(theme::FONT_PRIMARY));
static CHECK_PX: [u32; GLYPH_N * GLYPH_N] = check_px(pack_rgb(theme::BRAND));
static ADD_PX: [u32; GLYPH_N * GLYPH_N] = add_px(pack_rgb(theme::FONT_PRIMARY));
static REFRESH_PX: [u32; GLYPH_N * GLYPH_N] = refresh_px(pack_rgb(theme::BRAND));
static HOME_PX: [u32; GLYPH_N * GLYPH_N] = home_px(pack_rgb(theme::FONT_PRIMARY));
static GRID_PX: [u32; GLYPH_N * GLYPH_N] = grid_px(pack_rgb(theme::FONT_PRIMARY));
static SETTINGS_PX: [u32; GLYPH_N * GLYPH_N] = settings_px(pack_rgb(theme::FONT_PRIMARY));
static THUMB_PX: [u32; GLYPH_N * GLYPH_N] = thumb_px();

/// 本应用 IconSource 真供件（berry-ui D28 供给槽——pattern::icons 约定
/// 域 7 件 + Image 演示 1 件；域外 id 空绘不炸）。
pub struct GalleryIcons;

/// 供件单例（main.rs `set_icon_source` 注册）。
pub static ICONS: GalleryIcons = GalleryIcons;

impl IconSource for GalleryIcons {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        match id {
            pattern::icons::BACK
            | pattern::icons::CHECK
            | pattern::icons::ADD
            | pattern::icons::REFRESH
            | pattern::icons::HOME
            | pattern::icons::GRID
            | pattern::icons::SETTINGS
            | THUMB => Some((GLYPH_N as u32, GLYPH_N as u32)),
            _ => None,
        }
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        match id {
            pattern::icons::BACK => Some(&BACK_PX),
            pattern::icons::CHECK => Some(&CHECK_PX),
            pattern::icons::ADD => Some(&ADD_PX),
            pattern::icons::REFRESH => Some(&REFRESH_PX),
            pattern::icons::HOME => Some(&HOME_PX),
            pattern::icons::GRID => Some(&GRID_PX),
            pattern::icons::SETTINGS => Some(&SETTINGS_PX),
            THUMB => Some(&THUMB_PX),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// —— 建树小件 ——

/// 单节点 spec（key=0——调和键路径挂账 v1.5，pattern 同形）。
fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

/// Text 叶 spec（超 `TEXT_INLINE`=64B loud Err——本页文案 ≤32B 域内）。
fn text_spec(fg: Color, scale: FontSize, bytes: &[u8]) -> Result<ViewSpec, UiError> {
    Ok(vspec(
        ViewKind::Text,
        Props::Text(TextProps { style: TextStyle { fg, bg: None, scale }, text: TextBuf::new(bytes)? }),
    ))
}

/// 定尺寸/随父 Rect spec（theme token 色板展示与叠层语专用——零卡片
/// 拼装：Card 视觉一律 pattern::card/list_item）。
fn rect_spec(bg: Color, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    vspec(ViewKind::Rect, Props::Rect(RectProps { bg, w, h, radius: 0 }))
}

/// Row spec。
fn row_spec(gap: u32, w: Option<u32>, h: Option<u32>, cross: CrossAlign) -> ViewSpec {
    vspec(ViewKind::Row, Props::Row(berry_ui::desc::RowProps { gap, w, h, cross }))
}

/// Col spec。
fn col_spec(gap: u32, w: Option<u32>, h: Option<u32>, cross: CrossAlign) -> ViewSpec {
    vspec(ViewKind::Col, Props::Col(berry_ui::desc::ColProps { gap, w, h, cross }))
}

/// Button 叶 spec（on_tap 携事件 id——按压视觉走 press_map 登记面）。
fn button_spec(label: &[u8], style: ButtonStyle, on_tap: u32) -> Result<ViewSpec, UiError> {
    Ok(vspec(
        ViewKind::Button,
        Props::Button(ButtonProps { label: TextBuf::new(label)?, style, on_tap }),
    ))
}

// ---------------------------------------------------------------------------
// —— 画廊运行态（建树产物句柄——main.rs 交互接线面）——

/// 画廊页句柄（建树一次落定；帧循环只消费设备面 + 改滚动 offset）。
pub struct Gallery {
    /// 视口高（px——滚动 clamp 域）。
    pub vp_h: u32,
    /// 滚动容器（Scroll kind——offset 纯 props 域，app 自管）。
    pub scroll: NodeId,
    /// 滚动内容总高（ScrollProps.content_height 真值——建树期实测）。
    pub content_h: u32,
    /// 当前滚动偏移（app 自管域——clamp [0, content_h − vp_h]）。
    pub offset: u32,
    /// Seg 演示件（Click hit → set_active）。
    pub seg_cell: Seg,
    /// TabBar 演示件（Click hit → set_active）。
    pub tab_cell: TabBar,
    /// Sheet 演示件（按钮 set_open + tick 吸附）。
    pub sheet: Sheet,
    /// Drawer 演示件（按钮 set_open + tick 吸附）。
    pub drawer: Drawer,
    /// Refresh 演示件（纵拖 on_msg + settle 节拍）。
    pub refresh: Refresh,
    /// SwipeItem 演示件（横拖 on_msg + tick 吸附）。
    pub swipe: SwipeItem,
    /// DragReorder 演示件（纵拖 on_msg）。
    pub dragr: DragReorder,
}

/// 格头（Col w=cell_w cross=Stretch + 名称 Text S24x48 主墨——Stretch
/// 使 divider/骨架条/卡片横向吃满格宽；名称即「每件一格」的名牌）。
fn cell_head(ui: &mut UiMain, parent: NodeId, t: &Tokens, name: &[u8]) -> Result<NodeId, UiError> {
    let c = ui.add(
        parent,
        col_spec(CELL_GAP_INNER, Some(cell_w_of(ui)), None, CrossAlign::Stretch),
    )?;
    ui.add(c, text_spec(t.font_primary, FontSize::S24x48, name)?)?;
    Ok(c)
}

/// 格内说明字（S16x32 次墨——v1 诚实边界注记，仅关键格使用）。
fn caption(ui: &mut UiMain, c: NodeId, t: &Tokens, bytes: &[u8]) -> Result<(), UiError> {
    ui.add(c, text_spec(t.font_secondary, FontSize::S16x32, bytes)?)?;
    Ok(())
}
/// 分区头（标题 S32x48 + 分隔线——两节各一；包身 Col cross=Stretch：
/// 标题左贴格缘 + 分隔线吃满格宽——内容 Col 是 Center 档，Divider 内在
/// 宽 0 直挂会塌成零宽线）。
fn section_head(ui: &mut UiMain, parent: NodeId, t: &Tokens, title: &[u8]) -> Result<(), UiError> {
    let head = ui.add(
        parent,
        col_spec(CELL_GAP_INNER, Some(cell_w_of(ui)), None, CrossAlign::Stretch),
    )?;
    ui.add(head, text_spec(t.font_primary, FontSize::S32x48, title)?)?;
    pattern::divider(ui, head)?;
    Ok(())
}

/// cell_w 取值（视口 − 双侧页留白）。
fn cell_w_of(ui: &UiMain) -> u32 {
    let (vp_w, _) = ui.viewport();
    vp_w.saturating_sub(2 * PAGE_PAD)
}

// ---------------------------------------------------------------------------
// —— pattern 节建树（26 格——pattern.rs 全部可渲染面位）——

/// pattern 节（任务书「pattern 23 件」：刀 3 资产 23 配方经主树移植
/// （dd53bc039）= 20 pub fn 配方 + 7 attach 型器件 = 27 公开面位，其中
/// NavStack（纯数据页栈）/NavItem（入参件）无独立视觉面——NavStack 由
/// 消费面 NavHost 格承载、NavItem 由 bottom_tabs/scaffold 格入参承载，
/// 故可渲染格 = 26）。
fn build_pattern_section(
    ui: &mut UiMain,
    col: NodeId,
    t: &Tokens,
    out: &mut SectionOut,
) -> Result<(), UiError> {
    section_head(ui, col, t, b"pattern.rs recipes")?;

    // — spacer：弹性留白（Row 内推位——v1 Spacer kind 无 flex，配方以定
    //   宽 Col 承载；两色板间空带即 spacer 本体）—
    let c = cell_head(ui, col, t, b"spacer")?;
    let r = ui.add(c, row_spec(0, None, None, CrossAlign::Center))?;
    ui.add(r, rect_spec(t.brand, Some(216), Some(72)))?;
    pattern::spacer(ui, r, 216, 1)?;
    ui.add(r, rect_spec(t.warning, Some(216), Some(72)))?;
    pattern::divider(ui, c)?;

    // — divider：横向分隔线（Divider kind 直载，厚 1px 色单源 paint 面）—
    let c = cell_head(ui, col, t, b"divider")?;
    ui.add(c, text_spec(t.font_primary, FontSize::S16x32, b"Above")?)?;
    pattern::divider(ui, c)?;
    ui.add(c, text_spec(t.font_primary, FontSize::S16x32, b"Below")?)?;
    pattern::divider(ui, c)?;

    // — divider_v：竖向分隔线（w=1px Rect 承载——v1 无纵线 kind 面）—
    let c = cell_head(ui, col, t, b"divider_v")?;
    let r = ui.add(c, row_spec(48, None, Some(64), CrossAlign::Stretch))?;
    ui.add(r, text_spec(t.font_primary, FontSize::S16x32, b"Left")?)?;
    pattern::divider_v(ui, r, t.divider)?;
    ui.add(r, text_spec(t.font_primary, FontSize::S16x32, b"Right")?)?;
    pattern::divider(ui, c)?;

    // — stack：叠放容器（z = 子序，后子盖上——三色板级联）—
    let c = cell_head(ui, col, t, b"stack")?;
    let st = pattern::stack(ui, c)?;
    ui.add(st, rect_spec(t.divider, Some(648), Some(72)))?;
    ui.add(st, rect_spec(t.warning, Some(456), Some(72)))?;
    ui.add(st, rect_spec(t.brand, Some(264), Some(72)))?;
    pattern::divider(ui, c)?;

    // — app_bar：应用顶栏（56vp 高 + 返回 glyph + 标题）—
    let c = cell_head(ui, col, t, b"app_bar")?;
    pattern::app_bar(ui, c, t, b"App Bar", true)?;
    pattern::divider(ui, c)?;

    // — top_bar：顶栏变体（标题 + 尾随动作位 ×2——v1 无 flex 顺排）—
    let c = cell_head(ui, col, t, b"top_bar")?;
    pattern::top_bar(ui, c, t, b"Top Bar", &[pattern::icons::ADD, pattern::icons::SETTINGS])?;
    pattern::divider(ui, c)?;

    // — scaffold：Col 骨架（AppBar 槽 + 内容锚 + BottomTabs 槽）—
    let c = cell_head(ui, col, t, b"scaffold")?;
    let sc = pattern::scaffold(
        ui,
        c,
        t,
        Some((b"Title", true)),
        Some(&[
            pattern::NavItem { icon: pattern::icons::HOME, label: b"A" },
            pattern::NavItem { icon: pattern::icons::GRID, label: b"B" },
        ]),
    )?;
    ui.add(sc.content, text_spec(t.font_secondary, FontSize::S16x32, b"Scaffold content")?)?;
    pattern::divider(ui, c)?;

    // — NavHost：页面栈容器（Stack 叠放 + push/pop 300ms 推拉——画廊
    //   静态展示双层页级联；推拉动画归真应用消费面（contacts 详情页
    //   先例），无 clip 下跨格滑行不入演示）—
    let c = cell_head(ui, col, t, b"NavHost")?;
    let mut nav = NavHost::new();
    let nav_root = nav.attach(ui, c)?;
    let p1 = pattern::stack(ui, nav_root)?;
    ui.add(p1, rect_spec(t.bg_comp, None, Some(96)))?;
    ui.add(p1, text_spec(t.font_primary, FontSize::S24x48, b"Page 1")?)?;
    let p2 = pattern::stack(ui, nav_root)?;
    ui.add(p2, rect_spec(t.brand, None, Some(96)))?;
    ui.add(p2, text_spec(ON_ACCENT, FontSize::S24x48, b"Page 2")?)?;
    ui.set_offset(p2, 96, 120)?;
    caption(ui, c, t, b"page stack: push/pop + 300ms slide (apps)")?;
    pattern::divider(ui, c)?;

    // — tab_bar：页签栏（48vp Grid 等分 + active 换底换色；Click 命中
    //   经 hit() → set_active——main.rs 接线）—
    let c = cell_head(ui, col, t, b"tab_bar")?;
    out.tab_cell = Some(pattern::tab_bar(ui, c, t, &[b"One", b"Two", b"Three"], 0)?);
    pattern::divider(ui, c)?;

    // — seg：分段选择器（32vp Grid 等分 + active brand 换底）—
    let c = cell_head(ui, col, t, b"seg")?;
    out.seg_cell = Some(pattern::seg(ui, c, t, &[b"Red", b"Green", b"Blue"], 0)?);
    pattern::divider(ui, c)?;

    // — step：步进指示（静态配方——勾/当前/未达三态步点 + 连接线）—
    let c = cell_head(ui, col, t, b"step")?;
    pattern::step(ui, c, t, 4, 2)?;
    pattern::divider(ui, c)?;

    // — bottom_tabs：底部页签（48vp + icon + 小字 + active 换色）—
    let c = cell_head(ui, col, t, b"bottom_tabs")?;
    pattern::bottom_tabs(
        ui,
        c,
        t,
        &[
            pattern::NavItem { icon: pattern::icons::HOME, label: b"Home" },
            pattern::NavItem { icon: pattern::icons::GRID, label: b"Grid" },
            pattern::NavItem { icon: pattern::icons::SETTINGS, label: b"Set" },
        ],
        0,
    )?;
    pattern::divider(ui, c)?;

    // — card：卡面（elevation 2 阴影 + 8vp 圆角数据携载 + 前导图标——
    //   Card kind 自绘，零手拼）—
    let c = cell_head(ui, col, t, b"card")?;
    let card = pattern::card(ui, c, t, 2, Some(pattern::icons::ADD))?;
    ui.add(card.content, text_spec(t.font_primary, FontSize::S24x48, b"Card title")?)?;
    ui.add(card.content, text_spec(t.font_secondary, FontSize::S16x32, b"elevation 2 + icon")?)?;
    pattern::divider(ui, c)?;

    // — list_item：48vp 标准行（card(0, None) 薄别名——条目单源）—
    let c = cell_head(ui, col, t, b"list_item")?;
    let item = pattern::list_item(ui, c, t)?;
    ui.add(item.content, text_spec(t.font_primary, FontSize::S24x48, b"List item 48vp")?)?;
    pattern::divider(ui, c)?;

    // — Collapse：折叠面板（header 带 + 箭头自绘 + expanded 三面守卫；
    //   点击翻转走引擎 widget_react（kind Click 臂）——app 零接线）—
    let c = cell_head(ui, col, t, b"Collapse")?;
    let col5 = pattern::collapse(ui, c, true, EV_DEMO)?;
    ui.add(col5.body, text_spec(t.font_primary, FontSize::S16x32, b"body line one")?)?;
    ui.add(col5.body, text_spec(t.font_secondary, FontSize::S16x32, b"body line two")?)?;
    caption(ui, c, t, b"tap header to fold (engine widget_react)")?;
    pattern::divider(ui, c)?;

    // — SwipeItem：滑动展开行（跟手横拖 + QuadOut 吸附——on_msg/tick
    //   main.rs 接线；行面 = list_item 单源；行宽 = 格宽 + 右页留白 =
    //   通抵视口右缘（配方 width 语义「v1 取 vp_w」的格内对齐版——
    //   闭合态动作层恰藏于右缘外零露缝；edge-to-edge 行 = OH 列表
    //   习语）—
    let c = cell_head(ui, col, t, b"SwipeItem")?;
    let sw = SwipeItem::attach(ui, c, t, cell_w_of(ui) + PAGE_PAD, 2, EV_DEMO)?;
    let row_item = pattern::list_item(ui, sw.content, t)?;
    ui.add(
        row_item.content,
        text_spec(t.font_primary, FontSize::S24x48, b"Swipe left")?,
    )?;
    let acts = ui.add(sw.actions, row_spec(24, Some(pattern::SWIPE_REVEAL_W), Some(pattern::LISTITEM_H), CrossAlign::Center))?;
    ui.add(acts, text_spec(ON_ACCENT, FontSize::S16x32, b"Del")?)?;
    ui.add(acts, text_spec(ON_ACCENT, FontSize::S16x32, b"Info")?)?;
    out.swipe = Some(sw);
    pattern::divider(ui, c)?;

    // — DragReorder：拖拽重排（≤8 行——纵拖换槽；行 spec + 24×36 手柄
    //   kind 自绘；on_msg main.rs 接线）—
    let c = cell_head(ui, col, t, b"DragReorder")?;
    let mut dr = DragReorder::attach(ui, c, 96)?;
    for i in 0..3u8 {
        let spec = row_spec(24, Some(cell_w_of(ui)), Some(96), CrossAlign::Center);
        let row = dr.add_item(ui, spec, EV_DEMO)?;
        let mut lb = *b"Row 1";
        lb[4] = b'1' + i;
        ui.add(row, text_spec(t.font_primary, FontSize::S24x48, &lb)?)?;
    }
    dr.sync_totals(ui);
    caption(ui, c, t, b"drag a row vertically to reorder")?;
    out.dragr = Some(dr);
    pattern::divider(ui, c)?;

    // — chip：胶囊标签（选中色变——点击翻转走引擎 Chip 臂）—
    let c = cell_head(ui, col, t, b"chip")?;
    let r = ui.add(c, row_spec(24, None, None, CrossAlign::Center))?;
    pattern::chip(ui, r, b"On", true, EV_DEMO)?;
    pattern::chip(ui, r, b"Off", false, EV_DEMO)?;
    pattern::divider(ui, c)?;

    // — badge：角标（count=0 隐藏、>99 封「99+」——kind 自绘）—
    let c = cell_head(ui, col, t, b"badge")?;
    let r = ui.add(c, row_spec(32, None, None, CrossAlign::Center))?;
    pattern::badge(ui, r, 5)?;
    pattern::badge(ui, r, 120)?;
    pattern::badge(ui, r, 0)?;
    pattern::divider(ui, c)?;

    // — qr_show：二维码显示面（模块数 21 = 版本 1 域值；v1 确定性伪
    //   随机图案 + 定位图案 kind 自绘——真编码挂后续刀）—
    let c = cell_head(ui, col, t, b"qr_show")?;
    pattern::qr_show(ui, c, QR_MODULES, pack_rgb(t.font_primary), pack_rgb(t.bg_comp))?;
    pattern::divider(ui, c)?;

    // — fab：浮动动作钮（空标签 = 56 圆形 / 非空 = 扩展带）—
    let c = cell_head(ui, col, t, b"fab")?;
    let r = ui.add(c, row_spec(32, None, None, CrossAlign::Center))?;
    pattern::fab(ui, r, pattern::icons::ADD as u32, b"", EV_DEMO)?;
    pattern::fab(ui, r, pattern::icons::ADD as u32, b"Add", EV_DEMO)?;
    pattern::divider(ui, c)?;

    // — icon_button：图标钮（Fab 空扩展标签小档）—
    let c = cell_head(ui, col, t, b"icon_button")?;
    let r = ui.add(c, row_spec(32, None, None, CrossAlign::Center))?;
    pattern::icon_button(ui, r, pattern::icons::ADD as u32, EV_DEMO)?;
    pattern::icon_button(ui, r, pattern::icons::SETTINGS as u32, EV_DEMO)?;
    pattern::divider(ui, c)?;

    // — gauge：仪表（Ring/Bar/Arc 三档——v1 简化粗条视觉 kind 自绘）—
    let c = cell_head(ui, col, t, b"gauge")?;
    let r = ui.add(c, row_spec(48, None, None, CrossAlign::Center))?;
    pattern::gauge(ui, r, 750, GaugeStyle::Ring)?;
    pattern::gauge(ui, r, 300, GaugeStyle::Bar)?;
    pattern::gauge(ui, r, 550, GaugeStyle::Arc)?;
    pattern::divider(ui, c)?;

    // — Sheet：底部抽屉（跟手纵拖/按钮开合 + QuadOut 吸附——闭合态
    //   面板藏于格内预留空带「逃逸巷道」：v1 Stack 无 clip，预留带令
    //   滑动不越格；set_open/tick main.rs 接线）—
    let c = cell_head(ui, col, t, b"Sheet")?;
    let r = ui.add(c, row_spec(24, None, None, CrossAlign::Center))?;
    let ob = ui.add(r, button_spec(b"Open", ButtonStyle::Filled, EV_SHEET_OPEN)?)?;
    let cb = ui.add(r, button_spec(b"Close", ButtonStyle::Outlined, EV_SHEET_CLOSE)?)?;
    let sh = Sheet::attach(ui, c, t, cell_w_of(ui), SHEET_H)?;
    ui.add(sh.content, text_spec(t.font_primary, FontSize::S24x48, b"Sheet panel")?)?;
    ui.add(sh.content, text_spec(t.font_secondary, FontSize::S16x32, b"slides over the blank lane")?)?;
    pattern::spacer(ui, c, 1, SHEET_H)?;
    out.sheet = Some(sh);
    out.press.push(ob);
    out.press.push(cb);
    pattern::divider(ui, c)?;

    // — Drawer：侧滑抽屉（闭合态面板藏于格首预留巷道——同上逃逸语）—
    let c = cell_head(ui, col, t, b"Drawer")?;
    let r = ui.add(c, row_spec(24, None, None, CrossAlign::Center))?;
    let ob = ui.add(r, button_spec(b"Open", ButtonStyle::Filled, EV_DRAWER_OPEN)?)?;
    let cb = ui.add(r, button_spec(b"Close", ButtonStyle::Outlined, EV_DRAWER_CLOSE)?)?;
    let lane = ui.add(c, row_spec(0, Some(cell_w_of(ui)), None, CrossAlign::Start))?;
    pattern::spacer(ui, lane, DRAWER_W, 1)?;
    let drw = Drawer::attach(ui, lane, t, DRAWER_W, DRAWER_H)?;
    ui.add(drw.content, text_spec(t.font_secondary, FontSize::S16x32, b"Content")?)?;
    ui.add(drw.panel_content, text_spec(t.font_primary, FontSize::S24x48, b"Drawer")?)?;
    out.drawer = Some(drw);
    out.press.push(ob);
    out.press.push(cb);
    pattern::divider(ui, c)?;

    // — Refresh：下拉刷新（顶缘 48px 预留带藏指示器——指示 glyph 上移
    //   入带不越格；on_msg/settle main.rs 接线）—
    let c = cell_head(ui, col, t, b"Refresh")?;
    pattern::spacer(ui, c, 1, 48)?;
    let rf = Refresh::attach(ui, c, t, EV_DEMO)?;
    let row_item = pattern::list_item(ui, rf.content, t)?;
    ui.add(row_item.content, text_spec(t.font_primary, FontSize::S24x48, b"Pull down")?)?;
    caption(ui, c, t, b"drag down on the row to fire refresh")?;
    out.refresh = Some(rf);
    pattern::divider(ui, c)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// —— kind 节建树（23 格——gallery2 kind_* 全集映射中独立成格的核心
//    组件 kind；交互走引擎 widget_react/press_map，app 零接线）——

/// kind 节。
fn build_kind_section(
    ui: &mut UiMain,
    col: NodeId,
    t: &Tokens,
    out: &mut SectionOut,
) -> Result<(), UiError> {
    section_head(ui, col, t, b"ViewKind widgets")?;

    // — Button：三风格（Filled/Outlined/Text——按压视觉 press_map 登记）—
    let c = cell_head(ui, col, t, b"Button")?;
    let r = ui.add(c, row_spec(24, None, None, CrossAlign::Center))?;
    for (lb, style) in [
        (&b"OK"[..], ButtonStyle::Filled),
        (b"Cancel", ButtonStyle::Outlined),
        (b"More", ButtonStyle::Text),
    ] {
        let n = ui.add(r, button_spec(lb, style, EV_DEMO)?)?;
        out.press.push(n);
    }
    pattern::divider(ui, c)?;

    // — Button 三态对比（N 常态 / P 按住（press_map + PressedVis 查表
    //   换底）/ D disabled 档——v1 set_disabled 挂路 1 未合，渲染同 N，
    //   gallery2 kind_button_states 同一诚实注记）—
    let c = cell_head(ui, col, t, b"Button states")?;
    let r = ui.add(c, row_spec(24, None, None, CrossAlign::Center))?;
    for lb in [&b"Normal"[..], b"Pressed", b"Disabled"] {
        let n = ui.add(r, button_spec(lb, ButtonStyle::Filled, EV_DEMO)?)?;
        out.press.push(n);
    }
    caption(ui, c, t, b"N / P (hold) / D (path-1 pending)")?;
    pattern::divider(ui, c)?;

    // — Toggle：开关对（点击翻转走引擎）—
    let c = cell_head(ui, col, t, b"Toggle")?;
    let r = ui.add(c, row_spec(48, None, None, CrossAlign::Center))?;
    ui.add(r, vspec(ViewKind::Toggle, Props::Toggle(ToggleProps { checked: false, on_change: EV_DEMO })))?;
    ui.add(r, vspec(ViewKind::Toggle, Props::Toggle(ToggleProps { checked: true, on_change: EV_DEMO })))?;
    pattern::divider(ui, c)?;

    // — Slider：滑杆（60%——拖动跟值走引擎）—
    let c = cell_head(ui, col, t, b"Slider")?;
    ui.add(
        c,
        vspec(ViewKind::Slider, Props::Slider(SliderProps { value: 600, min: 0, max: 1000, on_change: EV_DEMO })),
    )?;
    pattern::divider(ui, c)?;

    // — Progress：进度（0/40/90% 线性 + Indeterminate + Circular——
    //   v1 三档统一线性视觉，gallery2 同注记）—
    let c = cell_head(ui, col, t, b"Progress")?;
    for v in [0u32, 400, 900] {
        ui.add(c, vspec(ViewKind::Progress, Props::Progress(ProgressProps { value: v, style: ProgressStyle::Linear })))?;
    }
    ui.add(c, vspec(ViewKind::Progress, Props::Progress(ProgressProps { value: 650, style: ProgressStyle::Indeterminate })))?;
    ui.add(c, vspec(ViewKind::Progress, Props::Progress(ProgressProps { value: 650, style: ProgressStyle::Circular })))?;
    pattern::divider(ui, c)?;

    // — Tabs：页签条（3 页 active=1——指示条 + 兄弟内容 Stack；点击
    //   切页走引擎 Tabs 臂）—
    let c = cell_head(ui, col, t, b"Tabs")?;
    let tabs = ui.add(
        c,
        vspec(ViewKind::Tabs, Props::Tabs(TabsProps { tab_count: 3, active: 1, on_switch: EV_DEMO })),
    )?;
    ui.add(tabs, rect_spec(t.bg_page, Some(cell_w_of(ui)), Some(0)))?;
    let st = pattern::stack(ui, c)?;
    ui.add(st, rect_spec(t.bg_comp, None, Some(240)))?;
    ui.add(st, text_spec(t.font_primary, FontSize::S24x48, b"Page 2")?)?;
    pattern::divider(ui, c)?;

    // — Seg：引擎分段选择器 kind（pattern::seg 配方 = Grid 组合——两形
    //   分立：本格展 kind 直载面，点击切段走引擎 Seg 臂 on_change）—
    let c = cell_head(ui, col, t, b"Seg")?;
    ui.add(
        c,
        vspec(ViewKind::Seg, Props::Seg(berry_ui::desc::SegProps { segment_count: 3, active: 1, on_change: EV_DEMO })),
    )?;
    pattern::divider(ui, c)?;

    // — Sbar：滚动条（H 20-50% + V 10-60% permille）—
    let c = cell_head(ui, col, t, b"Sbar")?;
    ui.add(
        c,
        vspec(ViewKind::Sbar, Props::Sbar(SbarProps { orientation: Orientation::Horizontal, thumb_start: 200, thumb_end: 500 })),
    )?;
    let r = ui.add(c, row_spec(32, None, None, CrossAlign::Center))?;
    ui.add(
        r,
        vspec(ViewKind::Sbar, Props::Sbar(SbarProps { orientation: Orientation::Vertical, thumb_start: 100, thumb_end: 600 })),
    )?;
    ui.add(r, text_spec(t.font_secondary, FontSize::S16x32, b"V 10-60%")?)?;
    pattern::divider(ui, c)?;

    // — List：列表（4 行 h=72——行 tiling 经单一内容 Col，gallery2 同形）—
    let c = cell_head(ui, col, t, b"List")?;
    let list = ui.add(
        c,
        vspec(ViewKind::List, Props::List(ListProps { item_count: 4, item_height: 72 })),
    )?;
    let rows = ui.add(list, col_spec(12, None, None, CrossAlign::Start))?;
    for i in 0..4u8 {
        let mut lb = *b"List row 1";
        lb[9] = b'1' + i;
        ui.add(rows, text_spec(t.font_primary, FontSize::S8x16, &lb)?)?;
    }
    pattern::divider(ui, c)?;

    // — Check：复选对 —
    let c = cell_head(ui, col, t, b"Check")?;
    let r = ui.add(c, row_spec(48, None, None, CrossAlign::Center))?;
    ui.add(r, vspec(ViewKind::Check, Props::Check(CheckProps { checked: false, on_change: EV_DEMO })))?;
    ui.add(r, vspec(ViewKind::Check, Props::Check(CheckProps { checked: true, on_change: EV_DEMO })))?;
    pattern::divider(ui, c)?;

    // — Radio：单选对（group 7——组互斥裁决在 app，画廊只展值面）—
    let c = cell_head(ui, col, t, b"Radio")?;
    let r = ui.add(c, row_spec(48, None, None, CrossAlign::Center))?;
    ui.add(r, vspec(ViewKind::Radio, Props::Radio(RadioProps { selected: false, group: 7, on_change: EV_DEMO })))?;
    ui.add(r, vspec(ViewKind::Radio, Props::Radio(RadioProps { selected: true, group: 7, on_change: EV_DEMO })))?;
    pattern::divider(ui, c)?;

    // — Select：选择器（折叠态显示 1-based 第 3 项——点击循环下一项
    //   走引擎 Select 臂）—
    let c = cell_head(ui, col, t, b"Select")?;
    ui.add(
        c,
        vspec(ViewKind::Select, Props::Select(SelectProps { option_count: 4, selected: 2, on_change: EV_DEMO })),
    )?;
    pattern::divider(ui, c)?;

    // — Input：单行输入（占位 + 已填焦点回显——v1 无 softime 敲键面，
    //   只展值面；点击置焦走引擎）—
    let c = cell_head(ui, col, t, b"Input")?;
    ui.add(
        c,
        vspec(ViewKind::Input, Props::Input(InputProps { placeholder: TextBuf::new(b"type here")?, max_len: 32, input_kind: InputKind::Text })),
    )?;
    let filled = ui.add(
        c,
        vspec(ViewKind::Input, Props::Input(InputProps { placeholder: TextBuf::new(b"search...")?, max_len: 32, input_kind: InputKind::Text })),
    )?;
    ui.set_input_text(filled, b"hello")?;
    ui.set_focus(Some(filled))?;
    pattern::divider(ui, c)?;

    // — Edit：多行编辑（v1 单 run 视觉——同上只展值面）—
    let c = cell_head(ui, col, t, b"Edit")?;
    let ed = ui.add(
        c,
        vspec(ViewKind::Edit, Props::Edit(EditProps { placeholder: TextBuf::new(b"notes")?, max_len: 64, max_lines: 3, readonly: false })),
    )?;
    ui.set_input_text(ed, b"hello edit")?;
    pattern::divider(ui, c)?;

    // — Canvas：画布（静态面——paint_ops 域 v1 自绘演示）—
    let c = cell_head(ui, col, t, b"Canvas")?;
    ui.add(c, vspec(ViewKind::Canvas, Props::Canvas(CanvasProps { paint_ops: 7 })))?;
    pattern::divider(ui, c)?;

    // — Tip：气泡提示（Bottom 档——notch 面挂后续刀，gallery2 同注记）—
    let c = cell_head(ui, col, t, b"Tip")?;
    let a = ui.add(c, col_spec(16, None, None, CrossAlign::Start))?;
    ui.add(a, text_spec(t.font_primary, FontSize::S24x48, b"WiFi")?)?;
    ui.add(
        a,
        vspec(ViewKind::Tip, Props::Tip(TipProps { text: TextBuf::new(b"WiFi is off")?, placement: TipPlacement::Bottom })),
    )?;
    pattern::divider(ui, c)?;

    // — DatePick：日期选择（2026-11-15——波 4 实施日钉板值）—
    let c = cell_head(ui, col, t, b"DatePick")?;
    ui.add(
        c,
        vspec(ViewKind::DatePick, Props::DatePick(DatePickProps { year: 2026, month: 11, day: 15, min_year: 2020, max_year: 2035, on_change: EV_DEMO })),
    )?;
    pattern::divider(ui, c)?;

    // — TimePick：时间对（24h / 12h 档——v1 raw HH:MM 视觉）—
    let c = cell_head(ui, col, t, b"TimePick")?;
    let r = ui.add(c, row_spec(32, None, None, CrossAlign::Center))?;
    ui.add(
        r,
        vspec(ViewKind::TimePick, Props::TimePick(TimePickProps { hour: 9, minute: 5, use_24h: true, on_change: EV_DEMO })),
    )?;
    ui.add(
        r,
        vspec(ViewKind::TimePick, Props::TimePick(TimePickProps { hour: 13, minute: 45, use_24h: false, on_change: EV_DEMO })),
    )?;
    pattern::divider(ui, c)?;

    // — Image：图像（res 缩略图——原生 24×24 + 定尺寸 48 双档）—
    let c = cell_head(ui, col, t, b"Image")?;
    let r = ui.add(c, row_spec(32, None, None, CrossAlign::Center))?;
    ui.add(r, vspec(ViewKind::Image, Props::Image(ImageProps { res: THUMB, w: None, h: None })))?;
    ui.add(r, vspec(ViewKind::Image, Props::Image(ImageProps { res: THUMB, w: Some(48), h: Some(48) })))?;
    pattern::divider(ui, c)?;

    // — Icon：字形（IconSource 真供件五面——48px 档显式定尺寸）—
    let c = cell_head(ui, col, t, b"Icon")?;
    let r = ui.add(c, row_spec(32, None, None, CrossAlign::Center))?;
    for id in [
        pattern::icons::BACK,
        pattern::icons::CHECK,
        pattern::icons::ADD,
        pattern::icons::SETTINGS,
        pattern::icons::HOME,
    ] {
        ui.add(
            r,
            vspec(ViewKind::Icon, Props::Icon(IconProps { id, fg: t.font_primary, w: Some(48), h: Some(48) })),
        )?;
    }
    pattern::divider(ui, c)?;

    // — Grid：网格（3×2 gap=8——theme 色板格）—
    let c = cell_head(ui, col, t, b"Grid")?;
    let g = ui.add(
        c,
        vspec(ViewKind::Grid, Props::Grid(GridProps { cols: 3, rows: 2, gap: 8, cross: CrossAlign::Stretch, w: None, h: None })),
    )?;
    for bg in [t.brand, t.bg_comp, t.divider, t.warning, theme::SUCCESS, t.brand_pressed] {
        ui.add(g, rect_spec(bg, Some(216), Some(96)))?;
    }
    pattern::divider(ui, c)?;

    // — Text：字阶（S8x16/S16x32/S24x48 三档 + 次墨）—
    let c = cell_head(ui, col, t, b"Text")?;
    let a = ui.add(c, col_spec(12, None, None, CrossAlign::Start))?;
    ui.add(a, text_spec(t.font_primary, FontSize::S8x16, b"S8x16 tier abc")?)?;
    ui.add(a, text_spec(t.font_primary, FontSize::S16x32, b"S16x32 tier abc")?)?;
    ui.add(a, text_spec(t.font_primary, FontSize::S24x48, b"S24x48 tier")?)?;
    ui.add(a, text_spec(t.font_secondary, FontSize::S16x32, b"secondary fg")?)?;
    pattern::divider(ui, c)?;

    // — Rect：色板（theme.rs light 档 token 六面——kind 底层面）—
    let c = cell_head(ui, col, t, b"Rect")?;
    let r = ui.add(c, row_spec(12, None, None, CrossAlign::Center))?;
    for bg in [t.brand, t.brand_pressed, t.warning, theme::SUCCESS, t.divider, t.bg_comp] {
        ui.add(r, rect_spec(bg, Some(96), Some(96)))?;
    }
    pattern::divider(ui, c)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// —— 分节产物（建树期收集——Gallery 总装面）——

/// 分节建树产物（设备 Option + press_map 登记表）。
struct SectionOut {
    seg_cell: Option<Seg>,
    tab_cell: Option<TabBar>,
    sheet: Option<Sheet>,
    drawer: Option<Drawer>,
    refresh: Option<Refresh>,
    swipe: Option<SwipeItem>,
    dragr: Option<DragReorder>,
    /// press_map 登记表（Button 族按压视觉 = paint::PressedVis 查表）。
    press: Vec<NodeId>,
}

/// 建树总装（main 一次调用）：页 Stack[滚动层, 铬层]（铬层后画 = 滚动
/// 溢出遮盖双保险——Scroll clip 已内嵌，此为无 clip 面的保守层）→ 两
/// 分节 → layout 实测滚动内容总高（手账免——bounds 尾锚真值）→
/// ScrollProps 收口 + press_map 登记。Err = 容量/文本域违例 → 调用方
/// fail 收口。
pub fn build_all(ui: &mut UiMain) -> Result<Gallery, UiError> {
    let (vp_w, vp_h) = ui.viewport();
    let t = Tokens::pick(Theme::Light);
    let root = ui.root();

    // 页 stack（滚动层 + 铬层叠放）。
    let page = pattern::stack(ui, root)?;
    // 滚动层（Scroll kind——Stack `_` 摆位吃满页 = 视口帽；内容 Col
    // 顶让位 chrome_h，格随 offset 上移滑入铬层之下）。
    let scroll = ui.add(
        page,
        vspec(ViewKind::Scroll, Props::Scroll(ScrollProps { content_height: 0, offset_y: 0 })),
    )?;
    let col = ui.add(
        scroll,
        col_spec(CELL_GAP, Some(vp_w), None, CrossAlign::Center),
    )?;
    pattern::spacer(ui, col, 1, pattern::TOPBAR_H)?;
    // 铬层（AppBar——后画盖上 = 保守遮盖）。
    let chrome = ui.add(page, col_spec(0, Some(vp_w), None, CrossAlign::Stretch))?;
    pattern::app_bar(ui, chrome, &t, b"Gallery", false)?;

    let mut out = SectionOut {
        seg_cell: None,
        tab_cell: None,
        sheet: None,
        drawer: None,
        refresh: None,
        swipe: None,
        dragr: None,
        press: Vec::new(),
    };
    build_pattern_section(ui, col, &t, &mut out)?;
    build_kind_section(ui, col, &t, &mut out)?;

    // 尾留白锚（content_h 实测基准——bounds 尾锚 y+h + TAIL_PAD）。
    let tail = pattern::spacer(ui, col, 1, TAIL_PAD)?;

    // press_map 登记（Button 族按压视觉——chip/fab kind 同表登记面，
    // 查表无命中零害）。
    let map: Vec<(NodeId, NodeId)> = out.press.iter().map(|&n| (n, n)).collect();
    ui.set_press_map(&map)?;

    // 滚动域收口：layout 一次后取尾锚绝对界（offset 0 期 = 内容坐标
    // 真值），content_height 实测入册（ScrollProps 单漏斗）。
    ui.layout();
    let b = ui.bounds(tail)?;
    let content_h = (b.y + b.h) as u32;
    ui.set_props(
        scroll,
        Props::Scroll(ScrollProps { content_height: content_h, offset_y: 0 }),
    )?;

    Ok(Gallery {
        vp_h,
        scroll,
        content_h,
        offset: 0,
        seg_cell: out.seg_cell.ok_or(UiError::NodeBad)?,
        tab_cell: out.tab_cell.ok_or(UiError::NodeBad)?,
        sheet: out.sheet.ok_or(UiError::NodeBad)?,
        drawer: out.drawer.ok_or(UiError::NodeBad)?,
        refresh: out.refresh.ok_or(UiError::NodeBad)?,
        swipe: out.swipe.ok_or(UiError::NodeBad)?,
        dragr: out.dragr.ok_or(UiError::NodeBad)?,
    })
}
