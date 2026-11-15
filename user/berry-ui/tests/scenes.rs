//! UI 集成三场景（多组件协作钉板——证明 41 kind 不是各自孤立工作，
//! 而是能组合成真实 UI 页面）：
//!
//! - **场景 1 设置页**：标题 Text + Divider×2 + Row[Text+推位+Toggle]×2
//!   + Row[Text+推位+Slider+值 Text] + Check×3 + Button——布局（各组件
//!   不重叠、行距恒 gap）/渲染（每类组件都有 op）/交互（点 Toggle 翻转
//!   → damage → 重绘换色；Slider 拖动 → on_change → app `set_text` 值
//!   文本 → 纯 PAINT 帧换字——settings 真应用同链）。
//! - **场景 2 列表详情页**：AppBar（返回 Icon + 标题）+ Scroll[List 多行
//!   + Card[标题/正文/Chip×2] + Progress] + TabBar（Tabs 底部导航）。
//!   含两版：**spec 直译版**（Root Col 三段竖排——实测 Scroll 吃整
//!   content_height 把 TabBar 推出视口，记录为组合限制）与**可工作版**
//!   （Stack 叠层：Scroll 全幅 + AppBar/TabBar 浮层 + set_offset 钉位
//!   ——launcher 同形）。Tab 点击切页 / 滚动 offset / 内容命中全链。
//! - **场景 3 对话框**：Stack[底层内容 Col, 弹层 Card[文案 + 取消/删除
//!   双 Button]]——z 序（Card 盖底）、模态（全幅弹层容器吞命中）、
//!   remove(弹层) 关闭后底层恢复可达。
//!
//! 已知组件组合限制（本文件实测钉板，非猜测）：
//! 1. **Spacer 是 0×0 占位**（flex 权重分配挂后续布局刀）——Row 里
//!    Spacer 不推位，尾件贴着前件；真实 settings app 与本测试场景 1
//!    均用「定宽占位 Col」代位（见 `spacer_noop_in_row`）。
//! 2. **Scroll/List 是「随父填充」容器**：Col 主轴给子的可用高 = 子
//!    内在高——竖排 [AppBar, Scroll, TabBar] 时 Scroll 取 content_height
//!    整高（不吃「剩余高」），TabBar 被推出视口（见
//!    `scene2_naive_col_sandwich_breaks`）。
//! 3. ~~**List 直接挂多行子全叠 (0,0)**~~（已修——place List 臂子按
//!    y = i×item_height 排列、宽吃满，回归钉板见
//!    `list_direct_rows_tile_by_item_height`；内容 Col 承载仍是多行
//!    定宽 tile 的可工作形，gallery2/launcher 同形）。
//! 4. **StackAlign 摆位未实装**（place `_` 臂恒 (0,0) 全幅——消费面挂
//!    账）——弹层居中用 cross=Center 的全幅 Col + 定高顶占位实现
//!    （场景 3）。
//! 5. **文本按字节绘制**（GlyphPos.ch = u8）——CJK 走 UTF-8 字节流
//!    会乱码，真实 app（settings/launcher）全 ASCII 文案同证；本测试
//!    文案全 ASCII。
//! 6. **paint scratch 全帧共享且按发射序消耗**（Text run 截尾静默）：
//!    文本重的页面（场景 2 ≈ 300 可见字形）scratch 必须给足，否则
//!    发射序末段的 run（本例 AppBar 标题——Stack 浮层画在 Scroll
//!    内容之后）整 run 被截为空。settings 真应用 SCRATCH_GLYPHS=256
//!    对 ~40 字形富余；本测试两渲染助手用 2048。
//!
//! 通道 = gallery2/offscreen 同位先例：`Ui::paint` → `CommandEncoder` →
//! `Queue::submit` → RGBx 软光栅帧缓冲，逐像素探针断言（光栅 u32 落盘
//! 字节序 [b,g,r,x]——探针换序回 RGB）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto::{PHASE_DOWN, PHASE_MOVE, PHASE_UP};
use berry_ui::damage::RectI;
use berry_ui::desc::{
    ButtonProps, ButtonStyle, CardProps, CheckProps, ChipProps, ColProps, DividerProps, IconProps,
    ListProps, Msg, ProgressProps, ProgressStyle, Props, RectProps, RowProps, ScrollProps,
    SliderProps, SpacerProps, StackAlign, StackProps, TabsProps, TextBuf, TextProps, TextStyle,
    ToggleProps,
};
use berry_ui::hit::PtrEvent;
use berry_ui::layout::CrossAlign;
use berry_ui::metrics::IconSource;
use berry_ui::theme::{
    ACCENT, CARD_BG, CARD_SHADOW, KNOB, ON_ACCENT, OUTLINE, TABS_BG, TABS_INDICATOR, TRACK_DIM,
    TRACK_OFF,
};
use berry_ui::theme::{BG_PAGE, FONT_PRIMARY, FONT_SECONDARY};
use berry_ui::{NodeId, UiMain, ViewKind, ViewSpec};

// ---------- 事件 ID 面（props on_* 携载——take_events 消费锚） ----------

const EV_WIFI: u32 = 1;
const EV_BT: u32 = 2;
const EV_BRIGHT: u32 = 3;
const EV_SAVE: u32 = 4;
const EV_CHECK: u32 = 5;
const EV_TAB: u32 = 6;
const EV_CANCEL: u32 = 7;
const EV_DEL: u32 = 8;
const EV_CHIP: u32 = 9;

// ---------- 建树小件（settings app / gallery2 同形） ----------

fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

fn text_spec(bytes: &[u8], fg: Color, scale: FontSize) -> ViewSpec {
    vspec(
        ViewKind::Text,
        Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(bytes).unwrap(),
        }),
    )
}

fn col_spec(gap: u32, w: Option<u32>, h: Option<u32>, cross: CrossAlign) -> ViewSpec {
    vspec(ViewKind::Col, Props::Col(ColProps { gap, w, h, cross }))
}

fn row_spec(gap: u32, w: Option<u32>, h: Option<u32>, cross: CrossAlign) -> ViewSpec {
    vspec(ViewKind::Row, Props::Row(RowProps { gap, w, h, cross }))
}

fn box_spec(w: u32, h: u32) -> ViewSpec {
    // 定宽/定高占位 Col（launcher/settings 推位同形——Spacer v1 0×0）。
    col_spec(0, Some(w), Some(h), CrossAlign::Center)
}

fn divider_spec(thickness: u32) -> ViewSpec {
    vspec(
        ViewKind::Divider,
        Props::Divider(DividerProps { thickness, inset_start: 0, inset_end: 0 }),
    )
}

fn full_bg(ui: &mut UiMain, parent: NodeId, w: u32, h: u32) -> NodeId {
    ui.add(
        parent,
        vspec(ViewKind::Rect, Props::Rect(RectProps { bg: BG_PAGE, w: Some(w), h: Some(h) })),
    )
    .unwrap()
}

/// 带尾件行装配：Row(gap, w, Center)[标签 Text, 定宽推位, 尾件×n]——
/// 推位宽 = 行宽 − 标签宽 − 尾件总宽 − 全部缝（尾件右缘恒钉行右缘，
/// 尾件 x 不随文案宽抖动——settings app `row` 同形）。返回
/// (行, 标签, 推位, 尾件们)。
fn row_with_tail(
    ui: &mut UiMain,
    parent: NodeId,
    width: u32,
    label: &[u8],
    label_scale: FontSize,
    tails: &[ViewSpec],
    tail_ws: &[u32],
    gap: u32,
) -> (NodeId, NodeId, NodeId, Vec<NodeId>) {
    assert_eq!(tails.len(), tail_ws.len());
    let label_w = label.len() as u32 * label_scale.cell_w() as u32;
    let n = tails.len() as u32 + 2;
    let flex = width
        .saturating_sub(label_w)
        .saturating_sub(tail_ws.iter().sum::<u32>())
        .saturating_sub(gap * (n - 1));
    let r = ui.add(parent, row_spec(gap, Some(width), None, CrossAlign::Center)).unwrap();
    let lb = ui.add(r, text_spec(label, FONT_PRIMARY, label_scale)).unwrap();
    let sp = ui.add(r, box_spec(flex, 1)).unwrap();
    let mut out = Vec::new();
    for t in tails {
        out.push(ui.add(r, t.clone()).unwrap());
    }
    (r, lb, sp, out)
}

// ---------- 输入/事件小件（widgets.rs pump_one 同形） ----------

fn pump_one(ui: &mut UiMain, phase: u8, x: u32, y: u32, ts: u64) -> Vec<Msg> {
    let ev = PtrEvent::from_wire(phase, 1, x, y, ts, 0, 0);
    ui.push_pointer(ev).unwrap();
    ui.pump_input(0).iter().copied().collect()
}

/// 单击（DOWN+UP）：返回两拍 Msg 并集（Press/Release/Click）。
fn tap(ui: &mut UiMain, x: u32, y: u32, ts: u64) -> Vec<Msg> {
    let mut out = pump_one(ui, PHASE_DOWN, x, y, ts);
    out.extend(pump_one(ui, PHASE_UP, x, y, ts + 1_000_000));
    out
}

/// 事件二元组抽取 (event, value)。
fn evs(ui: &mut UiMain) -> Vec<(u32, u32)> {
    ui.take_events().iter().map(|e| (e.event, e.value)).collect()
}

// ---------- 渲染/探针小件（gallery2 render 同形） ----------

/// FULL 帧 glyph scratch 容量（限制 6：全帧共享按发射序消耗——文本
/// 重页给足；场景 2 峰值 ≈ 310 可见字形）。
const SCRATCH_GLYPHS: usize = 2048;

/// FULL 帧软光栅：drain + wake(0) 强制全量 → CommandEncoder → Queue::
/// submit → RGBx 帧缓冲。返回 (fb, PaintReport)。
fn render_fb(ui: &mut UiMain, w: u32, h: u32) -> (Vec<u8>, berry_ui::paint::PaintReport) {
    ui.drain_damage();
    ui.wake(0);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();
    let stride = w as usize * 4;
    let desc = TextureDesc::new(w as usize, h as usize, stride, TextureFormat::Rgbx).unwrap();
    let mut fb = vec![0u8; stride * h as usize];
    let pass = PassDescriptor::new(LoadOp::Load, StoreOp::Store, None);
    let mut q = Queue::new();
    q.submit(&dl, &mut fb, &desc, &pass).unwrap();
    (fb, rep)
}

/// 帧缓冲像素探针（RGB 三元组——光栅 u32 落盘序 [b,g,r,x]，探针换序
/// 回 RGB）。
fn px(fb: &[u8], w: usize, x: usize, y: usize) -> (u8, u8, u8) {
    let i = (y * w + x) * 4;
    (fb[i + 2], fb[i + 1], fb[i])
}

fn rgb(c: Color) -> (u8, u8, u8) {
    (c.r, c.g, c.b)
}

/// FULL 帧 DisplayList 副本（op 分类断言面；scratch 泄漏一次 = 测试面
/// 豁免——widgets.rs paint_full 同律）。
fn dl_of(ui: &mut UiMain) -> Vec<Cmd<'static>> {
    ui.drain_damage();
    ui.wake(0);
    let scr: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());
    let mut enc = CommandEncoder::new();
    ui.paint(scr, &mut enc);
    enc.finish().iter().cloned().collect()
}

/// DisplayList 字形抽取：(ch, x, y)。
fn glyphs(dl: &[Cmd]) -> Vec<(u8, i32, i32)> {
    let mut out = Vec::new();
    for c in dl {
        if let Cmd::DrawGlyphRun { run } = c {
            for g in run.glyphs {
                out.push((g.ch, g.x, g.y));
            }
        }
    }
    out
}

/// DisplayList FillRect 抽取：(x, y, w, h, color)。
fn fills(dl: &[Cmd]) -> Vec<(i32, i32, i32, i32, Color)> {
    dl.iter()
        .filter_map(|c| match c {
            Cmd::FillRect { rect, paint } => Some((rect.x, rect.y, rect.w, rect.h, paint.color)),
            _ => None,
        })
        .collect()
}

/// 圆角填充计数（Chip/Card 走 FillRectRounded——真圆角档）。
fn rounded_count(dl: &[Cmd]) -> usize {
    dl.iter().filter(|c| matches!(c, Cmd::FillRectRounded { .. })).count()
}

// —— 真圆角档（XA opt-in）切换串行化：进程级开关 + 还原守卫（paint.rs
//    测试面同形——集成测试本地单源）。Cmd 流断言的三测全程序持锁（其
//    余测试的像素探针均取内点，两档等值——锁外安全）。 ——
static CORNER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct CornerRestore(bool);
impl Drop for CornerRestore {
    fn drop(&mut self) {
        berry_ui::paint::set_true_corners(self.0);
    }
}

/// Cmd 流断言测试的全程序串行守卫（保证期间档位恒默认直角）。
fn corner_guard() -> std::sync::MutexGuard<'static, ()> {
    CORNER_LOCK.lock().unwrap()
}

/// 真圆角档开档（须已持 corner_guard；Drop 还原）。
fn true_corners_on() -> CornerRestore {
    CornerRestore(berry_ui::paint::set_true_corners(true))
}

/// 两矩形是否重叠（半开区间）。
fn overlaps(a: &RectI, b: &RectI) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

// ---------- 图标供件（场景 2 AppBar 返回箭头——16×16 行位图） ----------

const ICON_BACK: u16 = 1;

struct SceneIcons;

const BACK_ROWS: [u16; 16] = [
    0b0000000000000000, 0b0000010000000000, 0b0000110000000000, 0b0001110000000000,
    0b0011111111110000, 0b0111111111111000, 0b1111111111111100, 0b1111111111111100,
    0b0111111111111000, 0b0011111111110000, 0b0001110000000000, 0b0000110000000000,
    0b0000010000000000, 0b0000000000000000, 0b0000000000000000, 0b0000000000000000,
];

impl IconSource for SceneIcons {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        if id == ICON_BACK {
            Some((16, 16))
        } else {
            None
        }
    }
    fn pixels(&self, id: u16) -> Option<&[u32]> {
        static CACHE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
        if id != ICON_BACK {
            return None;
        }
        Some(CACHE.get_or_init(|| {
            let mut v = [0u32; 256];
            for (y, r) in BACK_ROWS.iter().enumerate() {
                for b in 0..16 {
                    if r & (1 << (15 - b)) != 0 {
                        v[y * 16 + b] = 0xFF_1A_1A_1A;
                    }
                }
            }
            v
        }))
    }
}

static SCENE_ICONS: SceneIcons = SceneIcons;

// ============================================================
// 前置：组合限制钉板（场景构建里绕开的坑，先单独实测立此存照）
// ============================================================

/// 限制 1：Spacer 在 Row 里 0×0 不推位（flex 分配挂后续布局刀）。
#[test]
fn spacer_noop_in_row() {
    let mut ui = UiMain::new(320, 64);
    let root = ui.root();
    let r = ui.add(root, row_spec(8, Some(320), Some(32), CrossAlign::Center)).unwrap();
    let lb = ui.add(r, text_spec(b"Wi-Fi", FONT_PRIMARY, FontSize::S16x32)).unwrap();
    let sp = ui.add(r, vspec(ViewKind::Spacer, Props::Spacer(SpacerProps { flex: 1 }))).unwrap();
    let tog = ui
        .add(
            r,
            vspec(
                ViewKind::Toggle,
                Props::Toggle(ToggleProps { checked: false, on_change: EV_WIFI }),
            ),
        )
        .unwrap();
    ui.layout();
    // Text 5×16 = 80 宽；Spacer 0×0（占一个子位、吃两侧 gap 但不占宽）；
    // Toggle 紧贴 Text（x = 80 + 2×gap = 96），不在行尾 320−52 = 268。
    assert_eq!(ui.bounds(lb).unwrap(), RectI::new(0, 0, 80, 32));
    assert_eq!(ui.bounds(sp).unwrap(), RectI::new(88, 16, 0, 0), "Spacer 0×0 占位（Center 折 y=16）");
    assert_eq!(ui.bounds(tog).unwrap(), RectI::new(96, 0, 52, 32), "Toggle 贴 Text（Spacer 未推位）");
}

/// 限制 3 回归钉板（已修）：List 直接挂行子按 y = i×item_height 排列、
/// x=0 宽吃满（place List 臂——`_` 兜臂全叠 (0,0) 是批 A 半成品）。
#[test]
fn list_direct_rows_tile_by_item_height() {
    let mut ui = UiMain::new(320, 240);
    let root = ui.root();
    let ls = ui
        .add(root, vspec(ViewKind::List, Props::List(ListProps { item_count: 3, item_height: 40 })))
        .unwrap();
    let mut rows = Vec::new();
    for _ in 0..3 {
        rows.push(ui.add(ls, col_spec(0, Some(320), Some(40), CrossAlign::Stretch)).unwrap());
    }
    ui.layout();
    for (i, r) in rows.iter().enumerate() {
        let b = ui.bounds(*r).unwrap();
        assert_eq!((b.x, b.y), (0, i as i32 * 40), "List 直接子 {i} 行序竖排（List 臂）");
    }
    // 工作形：单一内容 Col 承载行。
    let ls2 = ui
        .add(root, vspec(ViewKind::List, Props::List(ListProps { item_count: 3, item_height: 40 })))
        .unwrap();
    let content = ui.add(ls2, col_spec(0, None, None, CrossAlign::Stretch)).unwrap();
    let mut tiled = Vec::new();
    for _ in 0..3 {
        tiled.push(ui.add(content, col_spec(0, Some(320), Some(40), CrossAlign::Stretch)).unwrap());
    }
    ui.layout();
    for (i, r) in tiled.iter().enumerate() {
        assert_eq!(ui.bounds(*r).unwrap().y, i as i32 * 40, "内容 Col 承载 → 行 {i} 竖排");
    }
}

// ============================================================
// 场景 1：设置页
// ============================================================

/// 场景 1 建树产物（句柄面——断言/交互锚）。
struct Scene1 {
    title: NodeId,
    div1: NodeId,
    wifi_row: NodeId,
    bt_row: NodeId,
    bright_row: NodeId,
    wifi_toggle: NodeId,
    bt_toggle: NodeId,
    bright_slider: NodeId,
    bright_value: NodeId,
    div2: NodeId,
    notif: NodeId,
    checks_col: NodeId,
    checks: Vec<NodeId>,
    save: NodeId,
}

/// 建设置页（spec 结构 + 定宽推位代位 Spacer——文件头限制 1）。
/// 视口 480×720，页 gap 12，行内 gap 8。
fn scene1_build() -> (UiMain, Scene1) {
    const W: u32 = 480;
    const GAP: u32 = 12;
    const RG: u32 = 8;
    let mut ui = UiMain::new(W, 720);
    let root = ui.root();
    full_bg(&mut ui, root, W, 720);
    let page = ui.add(root, col_spec(GAP, Some(W), Some(720), CrossAlign::Stretch)).unwrap();

    let title = ui.add(page, text_spec(b"Settings", FONT_PRIMARY, FontSize::S16x32)).unwrap();
    let div1 = ui.add(page, divider_spec(1)).unwrap();

    // Row [Text "Wi-Fi", 推位, Toggle]
    let (wifi_row, _lb, _sp, tails) = row_with_tail(
        &mut ui,
        page,
        W,
        b"Wi-Fi",
        FontSize::S16x32,
        &[vspec(
            ViewKind::Toggle,
            Props::Toggle(ToggleProps { checked: false, on_change: EV_WIFI }),
        )],
        &[52],
        RG,
    );
    let wifi_toggle = tails[0];

    // Row [Text "Bluetooth", 推位, Toggle]
    let (bt_row, _lb, _sp, tails) = row_with_tail(
        &mut ui,
        page,
        W,
        b"Bluetooth",
        FontSize::S16x32,
        &[vspec(
            ViewKind::Toggle,
            Props::Toggle(ToggleProps { checked: false, on_change: EV_BT }),
        )],
        &[52],
        RG,
    );
    let bt_toggle = tails[0];

    // Row [Text "Brightness", 推位, Slider, Text "75%"]
    let (bright_row, _lb, _sp, tails) = row_with_tail(
        &mut ui,
        page,
        W,
        b"Brightness",
        FontSize::S16x32,
        &[
            vspec(
                ViewKind::Slider,
                Props::Slider(SliderProps { value: 750, min: 0, max: 1000, on_change: EV_BRIGHT }),
            ),
            text_spec(b"75%", FONT_SECONDARY, FontSize::S16x32),
        ],
        &[200, 48],
        RG,
    );
    let (bright_slider, bright_value) = (tails[0], tails[1]);

    let div2 = ui.add(page, divider_spec(1)).unwrap();
    let notif = ui.add(page, text_spec(b"Notifications", FONT_PRIMARY, FontSize::S16x32)).unwrap();

    // Col [Check "Email", Check "SMS", Check "Push"]
    let checks_col = ui.add(page, col_spec(RG, None, None, CrossAlign::Stretch)).unwrap();
    let mut checks = Vec::new();
    for _lb in [b"Email" as &[u8], b"SMS", b"Push"] {
        checks.push(
            ui.add(
                checks_col,
                vspec(
                    ViewKind::Check,
                    Props::Check(CheckProps { checked: false, on_change: EV_CHECK }),
                ),
            )
            .unwrap(),
        );
    }
    let save = ui
        .add(
            page,
            vspec(
                ViewKind::Button,
                Props::Button(ButtonProps {
                    label: TextBuf::new(b"Save").unwrap(),
                    style: ButtonStyle::Filled,
                    on_tap: EV_SAVE,
                }),
            ),
        )
        .unwrap();
    ui.layout();
    (
        ui,
        Scene1 {
            title,
            div1,
            wifi_row,
            bt_row,
            bright_row,
            wifi_toggle,
            bt_toggle,
            bright_slider,
            bright_value,
            div2,
            notif,
            checks_col,
            checks,
            save,
        },
    )
}

/// ① 布局：直系子不重叠、行距恒 12、尾件右缘钉 480、内在尺寸单源。
#[test]
fn scene1_settings_layout() {
    let (ui, s) = scene1_build();
    let ui = ui;

    // —— 逐件值级（gap 12 竖排；x=0 起左对齐）——
    assert_eq!(ui.bounds(s.title).unwrap(), RectI::new(0, 0, 128, 32), "标题 8×16");
    assert_eq!(ui.bounds(s.div1).unwrap(), RectI::new(0, 44, 480, 1), "分隔线宽随父");
    assert_eq!(ui.bounds(s.wifi_row).unwrap(), RectI::new(0, 57, 480, 32), "Wi-Fi 行 y=44+1+12");
    let tog_w = ui.bounds(s.wifi_toggle).unwrap();
    assert_eq!(tog_w, RectI::new(428, 57, 52, 32), "Wi-Fi Toggle 右缘钉 480（推位代位 Spacer）");
    let tog_b = ui.bounds(s.bt_toggle).unwrap();
    assert_eq!(tog_b, RectI::new(428, 101, 52, 32), "Bluetooth Toggle 同列对齐");
    assert!(!overlaps(&tog_w, &tog_b));
    assert_eq!(
        ui.bounds(s.bright_slider).unwrap(),
        RectI::new(224, 145, 200, 44),
        "Slider 200×44（160 标签 + 8 缝 + 48 推位 + 8 缝 = 224）"
    );
    let val = ui.bounds(s.bright_value).unwrap();
    assert_eq!(val, RectI::new(432, 151, 48, 32), "值文本 432..480");
    assert_eq!(val.x + val.w, 480, "行尾恒右缘");
    assert_eq!(ui.bounds(s.div2).unwrap(), RectI::new(0, 201, 480, 1));
    assert_eq!(ui.bounds(s.notif).unwrap(), RectI::new(0, 214, 208, 32), "13×16");
    assert_eq!(ui.bounds(s.checks_col).unwrap(), RectI::new(0, 258, 480, 88), "三 Check 24×24 + gap 8");
    for (i, c) in s.checks.iter().enumerate() {
        assert_eq!(ui.bounds(*c).unwrap(), RectI::new(0, 258 + i as i32 * 32, 24, 24), "Check[{i}]");
    }
    let save = ui.bounds(s.save).unwrap();
    assert_eq!(save, RectI::new(0, 358, 64, 48), "Save = max(4×8+32, 64) × 48");
    assert!(save.y + save.h <= 720, "页面底不溢出视口（底 = {}）", save.y + save.h);

    // —— 不重叠全扫 + 行距恒 gap ——
    let page_kids = [
        s.title, s.div1, s.wifi_row, s.bt_row, s.bright_row, s.div2, s.notif, s.checks_col,
        s.save,
    ];
    let rects: Vec<RectI> = page_kids.iter().map(|&n| ui.bounds(n).unwrap()).collect();
    for i in 0..rects.len() {
        for j in i + 1..rects.len() {
            assert!(
                !overlaps(&rects[i], &rects[j]),
                "页面子 {i}/{j} 重叠：{:?} {:?}",
                rects[i],
                rects[j]
            );
        }
    }
    for i in 1..rects.len() {
        assert_eq!(rects[i].y - (rects[i - 1].y + rects[i - 1].h), 12, "页面子 {i} 行距恒 gap 12");
    }
    // Wi-Fi 行内：标签与 Toggle 不重叠（标签 0..80、Toggle 428..480）。
    assert!(!overlaps(&RectI::new(0, 57, 80, 32), &tog_w));
}

/// ② 渲染：FULL 帧每类组件都有 op（Text 字形/Divider 线/Toggle 轨+滑块/
/// Slider 轨+填充+圆点/Check 框/Button 底+白字）+ 帧缓冲像素探针。
#[test]
fn scene1_settings_paint_ops_complete() {
    let (ui, _s) = scene1_build();
    let mut ui = ui;
    let _guard = corner_guard();

    let (fb, rep) = render_fb(&mut ui, 480, 720);
    assert!(rep.cmds > 0, "op 集非空");
    assert!(rep.drawn_px > 0);

    let dl = dl_of(&mut ui);
    let g = glyphs(&dl);
    // 字形面：标题 8 + Wi-Fi 5 + Bluetooth 9 + Brightness 10 + 75% 3 +
    // Notifications 13 + Save 4 = 52。
    assert_eq!(g.len(), 52, "七个文本面字形总数");
    assert_eq!(g.iter().filter(|&(_, _, y)| *y == 0).count(), 8, "标题 8 字形在 y=0");
    let save_g: Vec<u8> = g.iter().filter(|&(_, _, y)| *y == 374).map(|&(c, _, _)| c).collect();
    assert_eq!(save_g, b"Save".to_vec(), "按钮标签居中（y = 358+(48−16)/2 = 374）");

    let f = fills(&dl);
    // Toggle 轨中段（两行关轨灰）。
    assert!(f.contains(&(438, 57, 32, 32, TRACK_OFF)), "Wi-Fi Toggle 关轨中段");
    assert!(f.contains(&(438, 101, 32, 32, TRACK_OFF)), "Bluetooth Toggle 关轨中段");
    // Slider 轨 + 750‰ 填充（圆心 x = 234+135 = 369 → 填充宽 145）。
    assert!(f.contains(&(224, 165, 200, 4, TRACK_DIM)), "Slider 底轨");
    assert!(f.contains(&(224, 165, 145, 4, ACCENT)), "Slider 750‰ 填充宽 145");
    // Check 描边（未选中灰框顶边）。
    assert!(f.contains(&(0, 258, 24, 2, OUTLINE)), "Check[0] 框顶边");
    assert!(f.contains(&(0, 322, 24, 2, OUTLINE)), "Check[2] 框顶边");
    assert!(f.contains(&(0, 358, 64, 48, ACCENT)), "Save Filled 底");
    // 按钮白字前景（ON_ACCENT）在册。
    assert!(
        dl.iter().any(|c| matches!(c, Cmd::DrawGlyphRun { run } if run.fg == ON_ACCENT)),
        "按钮白字前景"
    );

    // —— 帧缓冲像素探针 ——
    assert_eq!(px(&fb, 480, 240, 44), rgb(OUTLINE), "分隔线 1px 线");
    assert_eq!(px(&fb, 480, 444, 73), rgb(KNOB), "Toggle 关位滑块白（圆心 428+16）");
    assert_eq!(px(&fb, 480, 350, 167), rgb(ACCENT), "Slider 750‰ 填充内");
    assert_eq!(px(&fb, 480, 12, 270), rgb(BG_PAGE), "未选中 Check 内透页底");
    assert_eq!(px(&fb, 480, 20, 366), rgb(ACCENT), "Save 底（文本带外）");
    assert_eq!(px(&fb, 480, 400, 700), rgb(BG_PAGE), "页底背景");
}

/// ③ 交互：点 Toggle → 翻转事件 + damage + 重绘换色；Check/Save 事件；
/// Slider 拖动 → on_change → set_text("60%") 纯 PAINT 换字（settings
/// 真应用同链）。
#[test]
fn scene1_settings_interaction_flow() {
    let (ui, s) = scene1_build();
    let mut ui = ui;

    // 基线帧（并结清 damage）。
    let (fb0, _) = render_fb(&mut ui, 480, 720);

    // —— 点 Wi-Fi Toggle：checked 翻转 + on_change(1) ——
    let msgs = tap(&mut ui, 454, 73, 1_000_000);
    assert!(
        msgs.iter().any(|m| matches!(m, Msg::Click { node } if *node == s.wifi_toggle)),
        "Click 原样交 app：{msgs:?}"
    );
    assert_eq!(evs(&mut ui), vec![(EV_WIFI, 1)], "翻转 + on_change(checked=1)");
    assert!(!ui.damage().is_empty(), "翻转产生脏区");
    let (fb1, _) = render_fb(&mut ui, 480, 720);
    assert_eq!(px(&fb1, 480, 444, 73), rgb(ACCENT), "旧滑块位换 ACCENT 开轨");
    assert_eq!(px(&fb1, 480, 464, 73), rgb(KNOB), "滑块右移（圆心 428+36）");
    assert_ne!(px(&fb0, 480, 464, 73), rgb(KNOB), "基线该位非滑块");

    // 再击翻回（事件 value=0）。
    tap(&mut ui, 454, 73, 3_000_000);
    assert_eq!(evs(&mut ui), vec![(EV_WIFI, 0)], "再击翻回");

    // —— 点 Check[0]/Check[1]：on_change(1) ——
    tap(&mut ui, 12, 270, 5_000_000);
    tap(&mut ui, 12, 302, 6_000_000);
    assert_eq!(evs(&mut ui), vec![(EV_CHECK, 1), (EV_CHECK, 1)]);
    let (fb2, _) = render_fb(&mut ui, 480, 720);
    assert_eq!(px(&fb2, 480, 9, 267), rgb(ACCENT), "Check[0] 勾形 ACCENT");
    assert_eq!(px(&fb2, 480, 1, 270), rgb(ACCENT), "选中框描边 ACCENT");
    assert_eq!(px(&fb2, 480, 1, 334), rgb(OUTLINE), "Check[2] 未选中仍灰框");

    // —— 点 Save：on_tap(0) ——
    tap(&mut ui, 32, 382, 7_000_000);
    assert_eq!(evs(&mut ui), vec![(EV_SAVE, 0)]);

    // —— Slider 拖到 60%：DragStart 跳位 + DragMove 跟手 ——
    // 行程 [234, 414]：260 → 26×1000/180 = 144；342 → 108×1000/180 = 600。
    pump_one(&mut ui, PHASE_DOWN, 250, 167, 9_000_000);
    let msgs = pump_one(&mut ui, PHASE_MOVE, 260, 167, 9_001_000);
    assert!(matches!(msgs.as_slice(), [Msg::DragStart { .. }]), "越 slop 触发拖拽");
    let msgs = pump_one(&mut ui, PHASE_MOVE, 342, 167, 9_002_000);
    assert!(matches!(msgs.as_slice(), [Msg::DragMove { .. }]));
    pump_one(&mut ui, PHASE_UP, 342, 167, 9_003_000);
    assert_eq!(evs(&mut ui), vec![(EV_BRIGHT, 144), (EV_BRIGHT, 600)], "拖动两档值");

    // app 消费 on_change：亮度百分值换字 75% → 60%（等长 = 纯 PAINT）。
    ui.set_text(s.bright_value, b"60%").unwrap();
    assert_eq!(ui.bounds(s.bright_value).unwrap(), RectI::new(432, 151, 48, 32), "值文本几何不动");
    assert!(!ui.damage().is_empty());
    let dl = dl_of(&mut ui);
    let val_g: Vec<u8> = glyphs(&dl)
        .iter()
        .filter(|&&(_, x, y)| y == 151 && x >= 432)
        .map(|&(c, _, _)| c)
        .collect();
    assert_eq!(val_g, b"60%".to_vec(), "值文本换字为 60%（x≥432 排除同行 Brightness 标签）");
    let (fb3, _) = render_fb(&mut ui, 480, 720);
    assert_eq!(px(&fb3, 480, 360, 167), rgb(TRACK_DIM), "600‰ 圆点左移（360 出圆点 332..352）");
    assert_eq!(px(&fb0, 480, 360, 167), rgb(KNOB), "基线 750‰ 圆点盖 360（359..379）");
}

// ============================================================
// 场景 2：列表详情页
// ============================================================

/// 限制 2 钉板：spec 直译版（Root Col 三段竖排）——Scroll 吃整
/// content_height，TabBar 被推出视口（底部导航不可见）。
#[test]
fn scene2_naive_col_sandwich_breaks() {
    let mut ui = UiMain::new(360, 640);
    ui.set_icon_source(&SCENE_ICONS);
    let root = ui.root();
    full_bg(&mut ui, root, 360, 640);
    let page = ui.add(root, col_spec(0, Some(360), Some(640), CrossAlign::Stretch)).unwrap();

    // AppBar = Row[Icon BACK, Text 标题]。
    let bar = ui.add(page, row_spec(8, Some(360), None, CrossAlign::Center)).unwrap();
    ui.add(
        bar,
        vspec(ViewKind::Icon, Props::Icon(IconProps { id: ICON_BACK, fg: FONT_PRIMARY, w: None, h: None })),
    )
    .unwrap();
    ui.add(bar, text_spec(b"Item #42", FONT_PRIMARY, FontSize::S8x16)).unwrap();
    let div = ui.add(page, divider_spec(1)).unwrap();

    // Scroll 直挂 Col。
    let scr = ui
        .add(
            page,
            vspec(ViewKind::Scroll, Props::Scroll(ScrollProps { content_height: 800, offset_y: 0 })),
        )
        .unwrap();
    ui.add(scr, text_spec(b"row", FONT_PRIMARY, FontSize::S8x16)).unwrap();

    // TabBar = Tabs（宽经 0 高 Rect 子撑到 360）。
    let tabs = ui
        .add(
            page,
            vspec(ViewKind::Tabs, Props::Tabs(TabsProps { tab_count: 3, active: 0, on_switch: EV_TAB })),
        )
        .unwrap();
    ui.add(
        tabs,
        vspec(ViewKind::Rect, Props::Rect(RectProps { bg: BG_PAGE, w: Some(360), h: Some(0) })),
    )
    .unwrap();
    ui.layout();

    // AppBar/Divider 正常；Scroll 高 = content_height 800（非剩余 623）；
    // Tabs 被推到 y = 17+800 = 817——整件出视口（640）。
    assert_eq!(ui.bounds(bar).unwrap(), RectI::new(0, 0, 360, 16));
    assert_eq!(ui.bounds(div).unwrap(), RectI::new(0, 16, 360, 1));
    let sb = ui.bounds(scr).unwrap();
    assert_eq!(sb, RectI::new(0, 17, 360, 800), "Scroll 取 content_height 整高（不吃剩余高）");
    let tb = ui.bounds(tabs).unwrap();
    assert_eq!(tb, RectI::new(0, 817, 360, 48), "TabBar 被推出视口");
    assert!(tb.y >= 640, "底部导航整件在视口外（渲染被视口剔除）");
    let (fb, _) = render_fb(&mut ui, 360, 640);
    assert_eq!(px(&fb, 360, 10, 620), rgb(BG_PAGE), "视口底部无 TabBar 像素（不可见）");
}

/// 场景 2 可工作版建树产物。
struct Scene2 {
    scroll: NodeId,
    list: NodeId,
    rows: Vec<NodeId>,
    row_titles: Vec<NodeId>,
    card: NodeId,
    chip1: NodeId,
    chip2: NodeId,
    progress: NodeId,
    appbar: NodeId,
    app_icon: NodeId,
    app_div: NodeId,
    tabs: NodeId,
}

/// 可工作版：Stack 叠层——[底 Rect, Scroll 全幅, AppBar, Divider,
/// Tabs(offset 钉底)]。Scroll 内容 = [顶占位(清 AppBar), List(内容
/// Col 承载 22 行), Card[标题/正文/Chip×2], Progress, 底占位(清
/// TabBar)]。内容高 1276 > 视口 640 → offset 域 0..636。
fn scene2_build() -> (UiMain, Scene2) {
    const W: u32 = 360;
    const H: u32 = 640;
    const N_ROWS: usize = 22;
    let mut ui = UiMain::new(W, H);
    ui.set_icon_source(&SCENE_ICONS);
    let root = ui.root();
    let stack = ui
        .add(root, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Center })))
        .unwrap();
    full_bg(&mut ui, stack, W, H);

    // —— 内容层：Scroll 全幅（Stack `_` 臂给全 avail）——
    let scroll = ui
        .add(
            stack,
            vspec(ViewKind::Scroll, Props::Scroll(ScrollProps { content_height: 1276, offset_y: 0 })),
        )
        .unwrap();
    let content = ui.add(scroll, col_spec(12, None, None, CrossAlign::Stretch)).unwrap();
    ui.add(content, box_spec(1, 20)).unwrap(); // 顶占位：清 20 高 AppBar 带

    // List：行 tiling 经单一内容 Col（限制 3 工作形）。
    let list = ui
        .add(
            content,
            vspec(ViewKind::List, Props::List(ListProps { item_count: N_ROWS as u32, item_height: 48 })),
        )
        .unwrap();
    let list_col = ui.add(list, col_spec(0, None, None, CrossAlign::Stretch)).unwrap();
    let mut rows = Vec::new();
    let mut row_titles = Vec::new();
    for i in 1..=N_ROWS {
        let row = ui.add(list_col, col_spec(4, Some(W), Some(48), CrossAlign::Stretch)).unwrap();
        let t = ui
            .add(row, text_spec(format!("Item {i}").as_bytes(), FONT_PRIMARY, FontSize::S8x16))
            .unwrap();
        ui.add(
            row,
            text_spec(format!("Detail for row {i}").as_bytes(), FONT_SECONDARY, FontSize::S8x16),
        )
        .unwrap();
        rows.push(row);
        row_titles.push(t);
    }

    // Card：详情卡 [标题, 正文, Row[Chip, Chip]]。
    let card = ui
        .add(
            content,
            vspec(ViewKind::Card, Props::Card(CardProps { elevation: 1, corner_radius: 8 })),
        )
        .unwrap();
    let card_col = ui.add(card, col_spec(8, None, None, CrossAlign::Stretch)).unwrap();
    ui.add(card_col, text_spec(b"Berry Phone X", FONT_PRIMARY, FontSize::S16x32)).unwrap();
    ui.add(card_col, text_spec(b"6.1 inch OLED 120Hz", FONT_SECONDARY, FontSize::S8x16)).unwrap();
    let chips = ui.add(card_col, row_spec(8, None, None, CrossAlign::Center)).unwrap();
    let chip1 = ui
        .add(
            chips,
            vspec(
                ViewKind::Chip,
                Props::Chip(ChipProps { label: TextBuf::new(b"tag1").unwrap(), selected: false, on_tap: EV_CHIP }),
            ),
        )
        .unwrap();
    let chip2 = ui
        .add(
            chips,
            vspec(
                ViewKind::Chip,
                Props::Chip(ChipProps { label: TextBuf::new(b"tag2").unwrap(), selected: false, on_tap: EV_CHIP }),
            ),
        )
        .unwrap();

    let progress = ui
        .add(
            content,
            vspec(ViewKind::Progress, Props::Progress(ProgressProps { value: 400, style: ProgressStyle::Linear })),
        )
        .unwrap();
    ui.add(content, box_spec(1, 48)).unwrap(); // 底占位：清 48 高 TabBar 带

    // —— 浮层：AppBar（内在尺寸钉 (0,0)）+ Divider + Tabs（offset 钉底）——
    let appbar = ui.add(stack, row_spec(8, Some(W), Some(20), CrossAlign::Center)).unwrap();
    let app_icon = ui
        .add(
            appbar,
            vspec(ViewKind::Icon, Props::Icon(IconProps { id: ICON_BACK, fg: FONT_PRIMARY, w: None, h: None })),
        )
        .unwrap();
    ui.add(appbar, text_spec(b"Item #42", FONT_PRIMARY, FontSize::S8x16)).unwrap();
    let app_div = ui.add(stack, divider_spec(1)).unwrap();
    ui.set_offset(app_div, 0, 20).unwrap();
    let tabs = ui
        .add(
            stack,
            vspec(ViewKind::Tabs, Props::Tabs(TabsProps { tab_count: 3, active: 0, on_switch: EV_TAB })),
        )
        .unwrap();
    ui.add(
        tabs,
        vspec(ViewKind::Rect, Props::Rect(RectProps { bg: BG_PAGE, w: Some(W), h: Some(0) })),
    )
    .unwrap();
    ui.set_offset(tabs, 0, (H - 48) as i32).unwrap();
    ui.layout();
    (
        ui,
        Scene2 { scroll, list, rows, row_titles, card, chip1, chip2, progress, appbar, app_icon, app_div, tabs },
    )
}

/// ① 布局（offset 0）：Scroll 全幅、AppBar/TabBar 浮层钉位、行竖排、
/// Card/Chip/Progress 内容位、互不重叠。
#[test]
fn scene2_detail_layout() {
    let (ui, s) = scene2_build();

    assert_eq!(ui.bounds(s.scroll).unwrap(), RectI::new(0, 0, 360, 640), "Scroll 全幅（Stack 给全 avail）");
    assert_eq!(ui.bounds(s.appbar).unwrap(), RectI::new(0, 0, 360, 20), "AppBar 浮层");
    assert_eq!(ui.bounds(s.app_icon).unwrap(), RectI::new(0, 2, 16, 16), "返回图标十字居中");
    assert_eq!(ui.bounds(s.app_div).unwrap(), RectI::new(0, 20, 360, 1), "AppBar 下沿分隔线（offset 钉位）");
    let tb = ui.bounds(s.tabs).unwrap();
    assert_eq!(tb, RectI::new(0, 592, 360, 48), "TabBar 钉底（set_offset）");

    // 行竖排：row i = y 32+48i；首行让开 AppBar 带（20+1）。
    for (i, r) in s.rows.iter().enumerate() {
        assert_eq!(ui.bounds(*r).unwrap(), RectI::new(0, 32 + i as i32 * 48, 360, 48), "行 {i}");
    }
    assert!(ui.bounds(s.rows[0]).unwrap().y >= 21, "首行不压 AppBar/分隔线");
    // 行内互不重叠（相邻行 y 半开区间相接）。
    for i in 1..s.rows.len() {
        assert!(!overlaps(&ui.bounds(s.rows[i - 1]).unwrap(), &ui.bounds(s.rows[i]).unwrap()));
    }

    // List/Card/Progress 内容位（offset 0，超出视口部分渲染期剔除）。
    assert_eq!(ui.bounds(s.list).unwrap(), RectI::new(0, 32, 360, 1056), "22×48");
    assert_eq!(ui.bounds(s.card).unwrap(), RectI::new(0, 1100, 360, 96), "Card 宽随父/高内在");
    assert_eq!(ui.bounds(s.chip1).unwrap(), RectI::new(0, 1164, 48, 32), "Chip1 4×8+16");
    assert_eq!(ui.bounds(s.chip2).unwrap(), RectI::new(56, 1164, 48, 32), "Chip2 间隔 8");
    assert!(!overlaps(&ui.bounds(s.chip1).unwrap(), &ui.bounds(s.chip2).unwrap()));
    assert_eq!(ui.bounds(s.progress).unwrap(), RectI::new(0, 1208, 200, 8), "Progress 200×8");
    // TabBar 与视口内内容不重叠（Card/Progress 在下方折叠区）。
    assert!(!overlaps(&tb, &ui.bounds(s.progress).unwrap()) || ui.bounds(s.progress).unwrap().y >= 640);
}

/// ② 渲染（offset 0）：AppBar 字形/图标、Tabs 条三件套、行文本、
/// 折叠区（Card/Chip/Progress）剔除；滚动到底后全部在册。
#[test]
fn scene2_detail_paint_ops_complete() {
    let (ui, s) = scene2_build();
    let mut ui = ui;
    let _guard = corner_guard();

    // —— offset 0：首屏 ——
    let (fb, rep) = render_fb(&mut ui, 360, 640);
    assert!(rep.cmds > 0 && rep.drawn_px > 0);
    let dl = dl_of(&mut ui);
    let g = glyphs(&dl);
    // AppBar 标题 8 字形 y=2；可见行（0..12）标题+副文 ≥ 12×6 字形。
    assert_eq!(g.iter().filter(|&(_, _, y)| *y == 2).count(), 8, "AppBar 标题 8 字形");
    assert!(g.len() >= 8 + 12 * 6, "可见 12+ 行文本在册（{} 字形）", g.len());
    assert!(
        !fills(&dl).iter().any(|&(_, _, _, _, c)| c == CARD_BG),
        "Card 在折叠区（y≥1100 > 640）→ 面底整件被 viewport 剔除"
    );
    let f = fills(&dl);
    assert!(f.contains(&(0, 592, 360, 48, TABS_BG)), "TabBar 条底");
    assert!(f.contains(&(0, 637, 120, 3, TABS_INDICATOR)), "active 0 段指示条");
    assert!(f.contains(&(120, 592, 2, 48, OUTLINE)), "段间分隔线");
    assert!(f.contains(&(240, 592, 2, 48, OUTLINE)), "段间分隔线 2");
    // 像素探针：AppBar 图标墨点 / TabBar 底 / 指示条。
    assert_eq!(px(&fb, 360, 5, 3), (0x1A, 0x1A, 0x1A), "返回箭头像素（Icon 供件）");
    assert_eq!(px(&fb, 360, 60, 600), rgb(TABS_BG));
    assert_eq!(px(&fb, 360, 60, 638), rgb(TABS_INDICATOR));
    assert_eq!(px(&fb, 360, 180, 20), rgb(OUTLINE), "AppBar 下沿分隔线（y=20）");

    // —— 滚到底（offset 636）：Card/Chip/Progress 进视口 ——
    ui.set_props(s.scroll, Props::Scroll(ScrollProps { content_height: 1276, offset_y: 636 }))
        .unwrap();
    ui.layout();
    assert!(!ui.damage().is_empty(), "滚动产生脏区");
    let (fb2, _) = render_fb(&mut ui, 360, 640);
    let f2 = fills(&dl_of(&mut ui));
    // 默认直角档：Card 阴影整界 + 面底右下内缩 2（elev 1 × step 2），
    // Chip 双底直角 FillRect。
    assert!(f2.contains(&(0, 464, 360, 96, CARD_SHADOW)), "Card 阴影（整界）");
    assert!(f2.contains(&(0, 464, 358, 94, CARD_BG)), "Card 面（内缩 2）");
    assert!(f2.contains(&(0, 528, 48, 32, TRACK_DIM)), "Chip1 未选中灰底");
    assert!(f2.contains(&(56, 528, 48, 32, TRACK_DIM)), "Chip2 未选中灰底");
    // 真圆角档（XA opt-in）：同面走 FillRectRounded。
    {
        let _restore = true_corners_on();
        let dl3 = dl_of(&mut ui);
        assert_eq!(rounded_count(&dl3), 4, "Card 阴影+面 + Chip×2 FillRectRounded");
    }
    // Card 细节文本在册（标题 y = 464）。
    let g2 = glyphs(&dl_of(&mut ui));
    assert_eq!(g2.iter().filter(|&(_, _, y)| *y == 464).count(), 13, "Card 标题 13 字形");
    assert_eq!(px(&fb2, 360, 250, 500), rgb(CARD_BG), "Card 面（标题带外 x>208）");
    assert_eq!(px(&fb2, 360, 4, 544), rgb(TRACK_DIM), "Chip1 未选中灰底");
    assert_eq!(px(&fb2, 360, 60, 544), rgb(TRACK_DIM), "Chip2 未选中灰底");
    assert_eq!(px(&fb2, 360, 40, 576), rgb(ACCENT), "Progress 400‰ 填充（80 宽内）");
    assert_eq!(px(&fb2, 360, 150, 576), rgb(TRACK_DIM), "Progress 尾轨");
    assert_eq!(px(&fb2, 360, 60, 600), rgb(TABS_BG), "TabBar 恒在（浮层不随滚动）");
}

/// ③ 交互：内容命中穿透浮层、Tab 点击切页、Chip 点选、app 驱动滚动。
#[test]
fn scene2_detail_interaction_flow() {
    let (ui, s) = scene2_build();
    let mut ui = ui;

    // —— offset 0：命中分层——图标/行文本/TabBar 各归其位 ——
    assert_eq!(ui.node_at(8, 10), Some(s.app_icon), "AppBar 返回图标 topmost");
    assert_eq!(ui.node_at(24, 136), Some(s.row_titles[2]), "行 2 标题（内容层穿透）");
    assert_eq!(ui.node_at(300, 616), Some(s.tabs), "TabBar 命中");
    let msgs = tap(&mut ui, 24, 136, 1_000_000);
    assert!(
        msgs.iter().any(|m| matches!(m, Msg::Click { node } if *node == s.row_titles[2])),
        "行点击 Click 交 app"
    );
    assert!(evs(&mut ui).is_empty(), "纯文本行无组件事件");

    // —— 点 Tab 3（x=300 ∈ 段 2）：active 切换 + on_switch(2) ——
    let (fb0, _) = render_fb(&mut ui, 360, 640);
    assert_eq!(px(&fb0, 360, 60, 638), rgb(TABS_INDICATOR), "基线指示条在段 0");
    tap(&mut ui, 300, 616, 2_000_000);
    assert_eq!(evs(&mut ui), vec![(EV_TAB, 2)], "切页事件");
    assert!(!ui.damage().is_empty());
    let (fb1, _) = render_fb(&mut ui, 360, 640);
    assert_eq!(px(&fb1, 360, 300, 638), rgb(TABS_INDICATOR), "指示条随 active 移到段 2");
    assert_eq!(px(&fb1, 360, 60, 638), rgb(TABS_BG), "段 0 指示条退场");

    // —— 滚到底（app 驱动 set_props）→ 行命中随 offset 走 ——
    ui.set_props(s.scroll, Props::Scroll(ScrollProps { content_height: 1276, offset_y: 636 }))
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(s.rows[0]).unwrap().y, 32 - 636, "首行滚出顶");
    assert_eq!(ui.bounds(s.card).unwrap(), RectI::new(0, 464, 360, 96), "Card 进视口");
    assert_eq!(ui.node_at(24, 124), Some(s.row_titles[15]), "行 15 标题命中（y = 32+720−636 = 116）");

    // —— 点 Chip1：selected 翻转 + on_tap(1) ——
    tap(&mut ui, 24, 544, 3_000_000);
    assert_eq!(evs(&mut ui), vec![(EV_CHIP, 1)], "Chip 点选");
    let (fb2, _) = render_fb(&mut ui, 360, 640);
    assert_eq!(px(&fb2, 360, 4, 544), rgb(ACCENT), "选中 Chip 换 ACCENT 底");
    assert_eq!(px(&fb2, 360, 60, 544), rgb(TRACK_DIM), "Chip2 不受牵连");
}

// ============================================================
// 场景 3：对话框
// ============================================================

/// 场景 3 建树产物。
struct Scene3 {
    base: NodeId,
    base_rows: Vec<NodeId>,
    overlay: NodeId,
    card: NodeId,
    cancel: NodeId,
    del: NodeId,
}

/// 建对话框树（viewport 320×480）：Stack[底 Rect, 底层内容 Col, 弹层
/// 全幅 Col[顶占位 180, Card[文案, Row[Cancel, Delete]]]]。弹层居中 =
/// cross=Center 的 Col（限制 4：StackAlign 未实装）。
fn scene3_build() -> (UiMain, Scene3) {
    const W: u32 = 320;
    const H: u32 = 480;
    let mut ui = UiMain::new(W, H);
    let root = ui.root();
    let stack = ui
        .add(root, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Center })))
        .unwrap();
    full_bg(&mut ui, stack, W, H);

    // —— 底层内容：文件列表页 ——
    let base = ui.add(stack, col_spec(16, Some(W), Some(H), CrossAlign::Stretch)).unwrap();
    ui.add(base, text_spec(b"Files", FONT_PRIMARY, FontSize::S16x32)).unwrap();
    ui.add(base, divider_spec(1)).unwrap();
    let mut base_rows = Vec::new();
    // 四行文案各恰 16 字节（字形计数断言单源）。
    for lb in [
        b"doc.pdf   1.2 MB",
        b"img.png   340 KB",
        b"notes.md    3 KB",
        b"song.mp3    5 MB",
    ] {
        assert_eq!(lb.len(), 16);
        base_rows.push(ui.add(base, text_spec(lb, FONT_PRIMARY, FontSize::S8x16)).unwrap());
    }

    // —— 弹层：全幅 Col（cross=Center）承载 Card ——
    let overlay = ui.add(stack, col_spec(0, Some(W), Some(H), CrossAlign::Center)).unwrap();
    ui.add(overlay, box_spec(1, 120)).unwrap(); // 顶占位：Card 纵向落位（盖住底行 2/3——z 序）
    let card = ui
        .add(
            overlay,
            vspec(ViewKind::Card, Props::Card(CardProps { elevation: 2, corner_radius: 8 })),
        )
        .unwrap();
    let dlg = ui.add(card, col_spec(12, None, None, CrossAlign::Stretch)).unwrap();
    ui.add(dlg, text_spec(b"Delete this item?", FONT_PRIMARY, FontSize::S8x16)).unwrap();
    let btns = ui.add(dlg, row_spec(12, None, None, CrossAlign::Center)).unwrap();
    let cancel = ui
        .add(
            btns,
            vspec(
                ViewKind::Button,
                Props::Button(ButtonProps {
                    label: TextBuf::new(b"Cancel").unwrap(),
                    style: ButtonStyle::Outlined,
                    on_tap: EV_CANCEL,
                }),
            ),
        )
        .unwrap();
    let del = ui
        .add(
            btns,
            vspec(
                ViewKind::Button,
                Props::Button(ButtonProps {
                    label: TextBuf::new(b"Delete").unwrap(),
                    style: ButtonStyle::Filled,
                    on_tap: EV_DEL,
                }),
            ),
        )
        .unwrap();
    ui.layout();
    (ui, Scene3 { base, base_rows, overlay, card, cancel, del })
}

/// ①+② 布局与渲染：Card 居中盖底（z 序）、双按钮并排、阴影/面/字形
/// op 完整、像素探针。
#[test]
fn scene3_dialog_layout_and_paint() {
    let (ui, s) = scene3_build();

    // 底层文件行竖排（gap 16）。
    for (i, r) in s.base_rows.iter().enumerate() {
        assert_eq!(ui.bounds(*r).unwrap().y, 65 + i as i32 * 32, "底行 {i}");
    }
    // Card 居中：内在 172×76（标题 136 / 按钮行 172 取 max），十字
    // Center → x = (320−172)/2 = 74；顶占位 120 → y = 120（盖底行 2/3）。
    let cb = ui.bounds(s.card).unwrap();
    assert_eq!(cb, RectI::new(74, 120, 172, 76), "Card 居中（cross=Center + 顶占位）");
    let (b1, b2) = (ui.bounds(s.cancel).unwrap(), ui.bounds(s.del).unwrap());
    assert_eq!(b1, RectI::new(74, 148, 80, 48), "Cancel 左");
    assert_eq!(b2, RectI::new(166, 148, 80, 48), "Delete 右（80+12 间隔）");
    assert!(!overlaps(&b1, &b2), "双按钮并排不重叠");
    assert!(overlaps(&cb, &ui.bounds(s.base_rows[2]).unwrap()), "Card 与底层内容叠放（z 序盖底）");
    // 按钮都在 Card 面内。
    assert!(cb.x <= b1.x && b2.x + b2.w <= cb.x + cb.w && cb.y <= b1.y && b2.y + b2.h <= cb.y + cb.h);

    let mut ui = ui;
    let _guard = corner_guard();
    let (fb, rep) = render_fb(&mut ui, 320, 480);
    assert!(rep.cmds > 0 && rep.drawn_px > 0);
    // 默认直角档：Card 阴影整界 + 面底右下内缩 4（elev 2 × step 2）。
    let f = fills(&dl_of(&mut ui));
    assert!(f.contains(&(74, 120, 172, 76, CARD_SHADOW)), "Card 阴影（整界）");
    assert!(f.contains(&(74, 120, 168, 72, CARD_BG)), "Card 面（内缩 4）");
    // 真圆角档（XA opt-in）：同一双底走 FillRectRounded。
    {
        let _restore = true_corners_on();
        let dl_r = dl_of(&mut ui);
        assert_eq!(rounded_count(&dl_r), 2, "Card 阴影 + 面 FillRectRounded");
    }
    let g = glyphs(&dl_of(&mut ui));
    // 字形面：Files 5 + 底行 4×16 + 弹层文案 17 + Cancel 6 + Delete 6 = 98。
    assert_eq!(g.len(), 98, "全树字形总数");
    let dlg_g: Vec<u8> = g.iter().filter(|&(_, _, y)| *y == 120).map(|&(c, _, _)| c).collect();
    assert_eq!(dlg_g, b"Delete this item?".to_vec(), "弹层文案在 Card 顶（y=120）");
    let f = fills(&dl_of(&mut ui));
    assert!(f.contains(&(166, 148, 80, 48, ACCENT)), "Delete Filled 底");
    assert!(f.contains(&(74, 148, 80, 2, OUTLINE)), "Cancel 描边顶边");
    // 像素探针：Card 面白 / 阴影带（elev 2 → off 4，右缘按钮带上段）/
    // 底层带外。
    assert_eq!(px(&fb, 320, 160, 158), rgb(CARD_BG), "Card 面中心");
    assert_eq!(px(&fb, 320, 244, 130), rgb(CARD_SHADOW), "Card 右缘阴影带（按钮带上段）");
    assert_eq!(px(&fb, 320, 170, 184), rgb(ACCENT), "Delete 底（文本带下）");
    assert_eq!(px(&fb, 320, 300, 40), rgb(BG_PAGE), "Card 外仍是底层页底（标题右空白）");
}

/// ③ 交互：双按钮事件、模态遮罩（全幅弹层吞命中）、remove 关闭后
/// 底层恢复可达 + 重绘退场。
#[test]
fn scene3_dialog_interaction_flow() {
    let (ui, s) = scene3_build();
    let mut ui = ui;

    // 弹层开着：按钮命中（deepest），点按各派事件。
    assert_eq!(ui.node_at(206, 172), Some(s.del), "Delete 命中");
    assert_eq!(ui.node_at(114, 172), Some(s.cancel), "Cancel 命中");
    tap(&mut ui, 206, 172, 1_000_000);
    assert_eq!(evs(&mut ui), vec![(EV_DEL, 0)], "删除按钮 on_tap");
    tap(&mut ui, 114, 172, 2_000_000);
    assert_eq!(evs(&mut ui), vec![(EV_CANCEL, 0)], "取消按钮 on_tap");

    // 模态遮罩：Card 外的底层区命中被全幅弹层容器吞掉（v1 无透传面——
    // 全幅容器即天然 scrim）。
    assert_eq!(ui.node_at(20, 300), Some(s.overlay), "Card 外命中落弹层容器（模态）");
    assert_ne!(ui.node_at(20, 300), Some(s.base_rows[3]));
    let msgs = tap(&mut ui, 20, 300, 3_000_000);
    assert!(
        msgs.iter().any(|m| matches!(m, Msg::Click { node } if *node == s.overlay)),
        "scrim 点击 Click 交 app（可接「点外关闭」）"
    );
    assert!(evs(&mut ui).is_empty(), "容器无组件事件");

    // 关闭：remove 弹层 → 脏区 + 底层恢复可达 + 像素退场。
    ui.remove(s.overlay).unwrap();
    assert!(!ui.damage().is_empty(), "remove 产生脏区");
    ui.layout();
    assert_eq!(ui.node_at(40, 169), Some(s.base_rows[3]), "底层行 4 恢复命中");
    assert_eq!(ui.node_at(20, 300), Some(s.base), "弹层位命中回落底层容器");
    let (fb, _) = render_fb(&mut ui, 320, 480);
    assert_eq!(px(&fb, 320, 160, 158), rgb(BG_PAGE), "Card 位像素退场（回页底）");
}
