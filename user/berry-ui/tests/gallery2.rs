//! 波 2 画廊主树版（WAVE2-GALLERY——自 ../ui-wave2 波 2 验收工作树移植，
//! 依据 RECON-PORT.md 第 5 项）：41 ViewKind 全通电（3272bbdeb）的主树
//! 现实联络表——每 kind 至少一格 + pattern.rs 值面全件族 + Button
//! N/P/D 三态对比格。KNIFE3-GALLERY / 源 gallery2.rs 同位先例的通道：
//! `Ui::paint` → `CommandEncoder` → `Queue::submit` → RGBx 帧缓冲
//! （offscreen.rs soft 光栅先例），guest 面零改动。
//!
//! 与源画廊的差异（主树现实适配，非照抄）：
//! - 色板 = theme.rs light 档 token（`Tokens::pick`）+ paint.rs v1 固定
//!   色板（源为 dark 基线 `token()` API——主树无该面）；
//! - 七件裸锚格（Chip/Fab/Gauge/Qr/Card/Collapse/SwipeItem/DragReorder）
//!   在主树有真 paint 臂——源画廊「锚不可见」诚实标注不再需要；
//! - pattern 格 = 主树 pattern.rs 值面消费（几何常量 + ItemState 五态
//!   色表 + NavStack 纯数据；源 23 配方族建树 fn 挂路 2 未合，合入后
//!   验收轮重生成）；
//! - 现态诚实项：圆角（Chip/Card/ListItem 全直角——FillRectRounded
//!   挂路 1）、disabled 档（set_disabled 挂路 1——D 格现态同 N）、
//!   Scroll clip（溢出直出——挂路 1）、Progress Circular/Indeterminate
//!   （v1 统一线性）、Spacer flex（v1 0×0 占位）。上述任一面合入后
//!   须首跑回填 GOLDEN 表（验收轮重生成）。
//!
//! 本文件零第三方依赖：PPM（P6）dump 落 `WAVE2-GALLERY/ppm/`
//! （`WAVE2_GALLERY_DIR` env 可覆盖），`cells.tsv` 记格序/尺寸/像素
//! 哈希；每格 FNV-1a 64 位像素哈希 golden 钉板（渲染回归即红）；
//! PNG 联络表 = 宿主工具拼版（`tools/make_gallery.py`——仓内无 PNG
//! 编码器，转码归宿主）；像素级色证据探针 = `tools/probe_cells.py`。
//!
//! 建树律注记：随父填充件（Tabs/Scroll/Refresh/SwipeItem）Root 直挂取
//! 全幅（cell_col 锚的 Col 内只拿 meas 分配）；叠层面层（底色 Rect）走
//! Stack/`_` 臂全幅给子（每子全幅 avail）；发射序 = 父自身视觉先、子后
//! （子盖上父——Refresh 指示条格的内容经 set_offset 下移避让）。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto::PHASE_DOWN;
use berry_ui::desc::{
    BadgeProps, ButtonProps, ButtonStyle, CanvasProps, CardProps, CheckProps, ChipProps,
    CollapseProps, ColProps, DatePickProps, DividerProps, DragReorderProps, EditProps, FabProps,
    GaugeProps, GaugeStyle, GridProps, IconProps, ImageProps, InputKind, InputProps, ListProps,
    Orientation, PadProps, ProgressProps, ProgressStyle, QrProps, RadioProps, RectProps,
    RefreshProps, RowProps, SbarProps, ScrollProps, SegProps, SelectProps, SliderProps,
    SpacerProps, StackAlign, StackProps, StepProps, SwipeItemProps, TabsProps, TextBuf, TextProps,
    TextStyle, TimePickProps, TipPlacement, TipProps, ToggleProps,
};
use berry_ui::hit::PtrEvent;
use berry_ui::layout::CrossAlign;
use berry_ui::metrics::IconSource;
use berry_ui::pattern::{self as pat, ItemState, NavPage, NavStack};
use berry_ui::theme::{
    BG_COMP, BG_PAGE, BRAND, BRAND_PRESSED, DIVIDER, FONT_PRIMARY, FONT_SECONDARY, SUCCESS,
    WARNING,
};
use berry_ui::{NodeId, Props, UiMain, ViewKind, ViewSpec};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

// ---------- 色板单源（theme.rs light 档常量——零本地色值表） ----------

/// RGB u32 打包（`r<<16|g<<8|b`——lib.rs pack_color 同口径）→ Color。
/// ItemState::bg 值面消费用。
fn rgb(v: u32) -> Color {
    Color { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 }
}

// ---------- 供件：图标/图像像素（IconSource 缝，宿主预混） ----------

/// 测试图标 id（主树 pattern.rs 无 icons 面——本文件本地单源）。
mod icons {
    pub const BACK: u16 = 1;
    pub const CHECK: u16 = 2;
    pub const ADD: u16 = 3;
    pub const SETTINGS: u16 = 4;
    pub const HOME: u16 = 5;
    /// Image kind 的 res id（24×24 缩略图模式）。
    pub const THUMB: u16 = 9;
}

/// 16×16 行位图（MSB = 左）。on 像素 = 一级文本亮墨（ARGB 面不消费
/// IconProps.fg——像素即色；light 基线色板上可读——值源
/// theme::FONT_PRIMARY 同步维护）。
const INK: u32 = 0xFF_1A_1A_1A;

fn glyph_rows(id: u16) -> Option<[u16; 16]> {
    let rows: [u16; 16] = match id {
        icons::BACK => [
            0b0000000000000000, 0b0000010000000000, 0b0000110000000000, 0b0001110000000000,
            0b0011111111110000, 0b0111111111111000, 0b1111111111111100, 0b1111111111111100,
            0b0111111111111000, 0b0011111111110000, 0b0001110000000000, 0b0000110000000000,
            0b0000010000000000, 0b0000000000000000, 0b0000000000000000, 0b0000000000000000,
        ],
        icons::CHECK => [
            0b0000000000000000, 0b0000000000000000, 0b0000000000000010, 0b0000000000000110,
            0b0000000010000110, 0b0000000110001100, 0b0000000110011100, 0b0000000011111000,
            0b0000000011110000, 0b0000000111100000, 0b0000001111000000, 0b0000111110000000,
            0b0001111000000000, 0b0001110000000000, 0b0000000000000000, 0b0000000000000000,
        ],
        icons::ADD => [
            0b0000000000000000, 0b0000011000000000, 0b0000011000000000, 0b0000011000000000,
            0b0000011000000000, 0b0000011000000000, 0b0111111111111110, 0b1111111111111111,
            0b1111111111111111, 0b0111111111111110, 0b0000011000000000, 0b0000011000000000,
            0b0000011000000000, 0b0000011000000000, 0b0000011000000000, 0b0000000000000000,
        ],
        icons::SETTINGS => [
            0b0000000000000000, 0b0000110000110000, 0b0001111111111000, 0b0011111111111100,
            0b0111000000001110, 0b0110001100011110, 0b1110011110011111, 0b1110011110001111,
            0b1110011110001111, 0b1110011110011111, 0b0110001100011110, 0b0111100000001110,
            0b0011111111111100, 0b0001111111111000, 0b0000110000110000, 0b0000000000000000,
        ],
        icons::HOME => [
            0b0000000000000000, 0b0000001100000000, 0b0000011110000000, 0b0000111111000000,
            0b0001111111100000, 0b0011111111110000, 0b0111111111111000, 0b1111111111111100,
            0b1110011001111100, 0b1110011000111100, 0b1110000000111100, 0b1110000000111100,
            0b1110000000111100, 0b1111111111111100, 0b0000000000000000, 0b0000000000000000,
        ],
        _ => return None,
    };
    Some(rows)
}

/// Image res=9 的 24×24 缩略图（边框 + 棋盘纹——确定性生成）。
fn thumb_pixels() -> Vec<u32> {
    let d = 24usize;
    let mut v = Vec::with_capacity(d * d);
    for y in 0..d {
        for x in 0..d {
            let edge = x == 0 || y == 0 || x == d - 1 || y == d - 1;
            let px = if edge {
                0xFF_66_66_66
            } else if ((x / 4 + y / 4) & 1) == 0 {
                0xFF_E6_E6_E6
            } else {
                0xFF_F1_F3_F5
            };
            v.push(px);
        }
    }
    v
}

struct GalleryIcons;

impl IconSource for GalleryIcons {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        if glyph_rows(id).is_some() {
            Some((16, 16))
        } else if id == icons::THUMB {
            Some((24, 24))
        } else {
            None
        }
    }
    fn pixels(&self, id: u16) -> Option<&[u32]> {
        // 泄漏一次 = &'static（宿主供件常量语义；测试进程一次性）。
        static CACHE: OnceLock<HashMap<u16, &'static [u32]>> = OnceLock::new();
        let map = CACHE.get_or_init(|| {
            let mut m: HashMap<u16, &'static [u32]> = HashMap::new();
            for id in [icons::BACK, icons::CHECK, icons::ADD, icons::SETTINGS, icons::HOME] {
                if let Some(rows) = glyph_rows(id) {
                    let v: Box<[u32]> = rows
                        .iter()
                        .flat_map(|r| {
                            (0..16).map(move |b| if r & (1 << (15 - b)) != 0 { INK } else { 0 })
                        })
                        .collect();
                    m.insert(id, &*Box::leak(v));
                }
            }
            let t = thumb_pixels().into_boxed_slice();
            m.insert(icons::THUMB, &*Box::leak(t));
            m
        });
        map.get(&id).copied()
    }
}

static GALLERY_ICONS: GalleryIcons = GalleryIcons;

// ---------- 格机械（源 gallery2.rs / knife3 gallery.rs 同位先例） ----------

fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

fn rect(bg: Color, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    vspec(ViewKind::Rect, Props::Rect(RectProps { bg, w, h, radius: 0 }))
}

fn text(fg: Color, scale: FontSize, bytes: &[u8]) -> ViewSpec {
    vspec(
        ViewKind::Text,
        Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(bytes).unwrap(),
        }),
    )
}

fn col(gap: u32, cross: CrossAlign) -> ViewSpec {
    vspec(ViewKind::Col, Props::Col(ColProps { gap, w: None, h: None, cross }))
}

fn row(gap: u32, cross: CrossAlign) -> ViewSpec {
    vspec(ViewKind::Row, Props::Row(RowProps { gap, w: None, h: None, cross }))
}

fn pad(all: u32) -> ViewSpec {
    vspec(ViewKind::Pad, Props::Pad(PadProps { all }))
}

fn stack(align: StackAlign) -> ViewSpec {
    vspec(ViewKind::Stack, Props::Stack(StackProps { align }))
}

/// 格内说明字（S8x16 次级文本——52 字符预算内）。
fn caption(ui: &mut UiMain, c: NodeId, bytes: &[u8]) {
    ui.add(c, text(FONT_SECONDARY, FontSize::S8x16, bytes)).unwrap();
}

/// 格内容锚惯例：root → 满幅底 Rect（BG_PAGE）→ Pad(20) → Col(gap 16,
/// Stretch)。随父填充件（Tabs/Scroll/Refresh/SwipeItem）不走此锚——
/// Root 直挂取全幅（锚列空置不可见）。
fn cell_col(ui: &mut UiMain) -> NodeId {
    let root = ui.root();
    let (w, h) = ui.viewport();
    let _bg = ui.add(root, rect(BG_PAGE, Some(w), Some(h))).unwrap();
    let p = ui.add(root, pad(20)).unwrap();
    ui.add(p, col(16, CrossAlign::Stretch)).unwrap()
}

/// 按下不抬（PRESSED 态定格展示——widgets3 press 先例同款；
/// expected=0 惰性 pump 坐标透传）。
fn press_hold(ui: &mut UiMain, x: u32, y: u32) {
    let ev = PtrEvent::from_wire(PHASE_DOWN, 0, x, y, 1, 0, 0);
    ui.push_pointer(ev).unwrap();
    ui.pump_input(0);
}

/// 格：固定尺寸 + 建树闭包 → RGBx 帧缓冲（闭包内可自带 layout/press
/// 序——收尾统一再 layout + drain + wake(0) = FULL 发射）。
fn render(w: u32, h: u32, build: impl FnOnce(&mut UiMain, NodeId)) -> Vec<u8> {
    let mut ui = UiMain::new(w, h);
    ui.set_icon_source(&GALLERY_ICONS);
    let anchor = cell_col(&mut ui);
    build(&mut ui, anchor);
    ui.layout();
    ui.drain_damage();
    ui.wake(0);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 128];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();
    let stride = w as usize * 4;
    let desc = TextureDesc::new(w as usize, h as usize, stride, TextureFormat::Rgbx).unwrap();
    let mut fb = vec![0u8; stride * h as usize];
    let pass = PassDescriptor::new(LoadOp::Load, StoreOp::Store, None);
    let mut q = Queue::new();
    q.submit(&dl, &mut fb, &desc, &pass).unwrap();
    fb
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

#[derive(Clone, Copy)]
struct CellDef {
    name: &'static str,
    label: &'static str,
    w: u32,
    h: u32,
    build: fn(&mut UiMain, NodeId),
}

// ---------- kind 格（41 ViewKind 全覆盖：Root = 每格视口；结构件
// ---------- Col/Row/Pad 随每格锚 + kind_layout 专项格） ----------

fn build_kind_button(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(16, CrossAlign::Center)).unwrap();
    for (lb, style) in [
        (&b"OK"[..], ButtonStyle::Filled),
        (b"Cancel", ButtonStyle::Outlined),
        (b"More", ButtonStyle::Text),
    ] {
        ui.add(
            r,
            vspec(
                ViewKind::Button,
                Props::Button(ButtonProps { label: TextBuf::new(lb).unwrap(), style, on_tap: 1 }),
            ),
        )
        .unwrap();
    }
    caption(ui, c, b"Button Filled / Outlined / Text");
}

/// 三态对比格：N 常态 / P 按住不抬（press_map + PHASE_DOWN 定格——
/// paint 面 PressedVis 查表换 ACCENT_PRESSED）/ D 档（set_disabled 挂
/// 路 1 未合——现态同 N 渲染，路 1 合入后验收轮重生成）。
fn build_kind_button_states(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(24, CrossAlign::Center)).unwrap();
    let make = |ui: &mut UiMain, lb: &[u8]| -> NodeId {
        ui.add(
            r,
            vspec(
                ViewKind::Button,
                Props::Button(ButtonProps {
                    label: TextBuf::new(lb).unwrap(),
                    style: ButtonStyle::Filled,
                    on_tap: 1,
                }),
            ),
        )
        .unwrap()
    };
    let _normal = make(ui, b"Normal");
    let pressed = make(ui, b"Pressed");
    let _disabled = make(ui, b"Disabled");
    ui.set_press_map(&[(pressed, pressed)]).unwrap();
    ui.layout();
    let b = ui.bounds(pressed).unwrap();
    press_hold(ui, (b.x + b.w / 2) as u32, (b.y + b.h / 2) as u32);
    caption(ui, c, b"Button N/P/D (P=press hold; D=path-1 pending)");
}

fn build_kind_toggle(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(48, CrossAlign::Center)).unwrap();
    ui.add(r, vspec(ViewKind::Toggle, Props::Toggle(ToggleProps { checked: false, on_change: 1 })))
        .unwrap();
    ui.add(r, vspec(ViewKind::Toggle, Props::Toggle(ToggleProps { checked: true, on_change: 1 })))
        .unwrap();
    caption(ui, c, b"Toggle off / on (capsule v1 square approx)");
}

fn build_kind_slider(ui: &mut UiMain, c: NodeId) {
    ui.add(
        c,
        vspec(ViewKind::Slider, Props::Slider(SliderProps { value: 600, min: 0, max: 1000, on_change: 1 })),
    )
    .unwrap();
    caption(ui, c, b"Slider 60% (permille 0..1000)");
}

fn build_kind_progress(ui: &mut UiMain, c: NodeId) {
    for v in [0u32, 400, 900] {
        ui.add(
            c,
            vspec(ViewKind::Progress, Props::Progress(ProgressProps { value: v, style: ProgressStyle::Linear })),
        )
        .unwrap();
    }
    ui.add(
        c,
        vspec(ViewKind::Progress, Props::Progress(ProgressProps { value: 650, style: ProgressStyle::Indeterminate })),
    )
    .unwrap();
    ui.add(
        c,
        vspec(ViewKind::Progress, Props::Progress(ProgressProps { value: 650, style: ProgressStyle::Circular })),
    )
    .unwrap();
    caption(ui, c, b"Progress 0/40/90% + Indet + Circular (v1 all linear)");
}

fn build_kind_tabs(ui: &mut UiMain, c: NodeId) {
    // Tabs v1 视觉律（widgets3 先例）：指示条在节点**底缘**、子面板
    // y=48 起铺到节点底——像素面必盖指示条（cmd golden 不可见的 z 序
    // 现实）。消费面正解 = 无面板子：条内在 48 高、宽经零高 helper 定
    // （Some(420)×Some(0)——meas 携宽、零面积空绘），内容区作兄弟
    // Stack 自管（页垫 + 标题，后序盖上）。
    let tabs = ui
        .add(c, vspec(ViewKind::Tabs, Props::Tabs(TabsProps { tab_count: 3, active: 1, on_switch: 1 })))
        .unwrap();
    ui.add(tabs, rect(BG_PAGE, Some(420), Some(0))).unwrap();
    let st = ui.add(c, stack(StackAlign::Start)).unwrap();
    ui.add(st, rect(BG_COMP, Some(420), Some(240))).unwrap();
    let p = ui.add(st, pad(16)).unwrap();
    ui.add(p, text(FONT_PRIMARY, FontSize::S24x48, b"Page 2")).unwrap();
    caption(ui, c, b"Tabs 3 pages active=1 (strip 48 + sibling content)");
}

fn build_kind_sbar(ui: &mut UiMain, c: NodeId) {
    ui.add(
        c,
        vspec(ViewKind::Sbar, Props::Sbar(SbarProps {
            orientation: Orientation::Horizontal,
            thumb_start: 200,
            thumb_end: 500,
        })),
    )
    .unwrap();
    let r = ui.add(c, row(32, CrossAlign::Center)).unwrap();
    ui.add(
        r,
        vspec(ViewKind::Sbar, Props::Sbar(SbarProps {
            orientation: Orientation::Vertical,
            thumb_start: 100,
            thumb_end: 600,
        })),
    )
    .unwrap();
    ui.add(r, text(FONT_SECONDARY, FontSize::S8x16, b"V 10-60%")).unwrap();
    caption(ui, c, b"Sbar H 20-50% + V 10-60%");
}

fn build_kind_scroll(ui: &mut UiMain, _c: NodeId) {
    // 随父填充件——Root 直挂（clip 挂路 1：溢出直出，视口缘由光栅
    // 纹理界截断）。子 = 单一内容 Col（Scroll 臂子同位——多子直挂
    // 会叠在同一偏移）。offset 200：带 1 整条上移出界、带 2 露头。
    let root = ui.root();
    let sc = ui
        .add(root, vspec(ViewKind::Scroll, Props::Scroll(ScrollProps { content_height: 720, offset_y: 200 })))
        .unwrap();
    let content = ui.add(sc, col(0, CrossAlign::Stretch)).unwrap();
    for bg in [BRAND, BG_COMP, DIVIDER, SUCCESS] {
        ui.add(content, rect(bg, None, Some(180))).unwrap();
    }
}

fn build_kind_list(ui: &mut UiMain, c: NodeId) {
    // List 行 = 子节点数（place `_` 臂全幅叠放）——行 tiling 经单一
    // 内容 Col 承载（launcher 消费面同形）。
    let list = ui
        .add(c, vspec(ViewKind::List, Props::List(ListProps { item_count: 4, item_height: 72 })))
        .unwrap();
    let content = ui.add(list, col(4, CrossAlign::Stretch)).unwrap();
    for bg in [BG_COMP, DIVIDER, BG_COMP, DIVIDER] {
        ui.add(content, rect(bg, None, Some(68))).unwrap();
    }
    caption(ui, c, b"List 4 rows h=72 (tiling via content Col)");
}

fn build_kind_check(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(48, CrossAlign::Center)).unwrap();
    ui.add(r, vspec(ViewKind::Check, Props::Check(CheckProps { checked: false, on_change: 1 })))
        .unwrap();
    ui.add(r, vspec(ViewKind::Check, Props::Check(CheckProps { checked: true, on_change: 1 })))
        .unwrap();
    caption(ui, c, b"Check off / on");
}

fn build_kind_radio(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(48, CrossAlign::Center)).unwrap();
    ui.add(r, vspec(ViewKind::Radio, Props::Radio(RadioProps { selected: false, group: 7, on_change: 1 })))
        .unwrap();
    ui.add(r, vspec(ViewKind::Radio, Props::Radio(RadioProps { selected: true, group: 7, on_change: 1 })))
        .unwrap();
    caption(ui, c, b"Radio group 7 off / on");
}

fn build_kind_seg(ui: &mut UiMain, c: NodeId) {
    ui.add(c, vspec(ViewKind::Seg, Props::Seg(SegProps { segment_count: 3, active: 1, on_change: 1 })))
        .unwrap();
    caption(ui, c, b"Seg 3 active=1");
}

fn build_kind_badge(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(32, CrossAlign::Center)).unwrap();
    ui.add(r, vspec(ViewKind::Badge, Props::Badge(BadgeProps { count: 5, show_dot: false }))).unwrap();
    ui.add(r, vspec(ViewKind::Badge, Props::Badge(BadgeProps { count: 120, show_dot: false }))).unwrap();
    ui.add(r, vspec(ViewKind::Badge, Props::Badge(BadgeProps { count: 0, show_dot: true }))).unwrap();
    caption(ui, c, b"Badge 5 / 99+ / dot");
}

fn build_kind_divider(ui: &mut UiMain, c: NodeId) {
    let a = ui.add(c, col(12, CrossAlign::Start)).unwrap();
    ui.add(a, text(FONT_PRIMARY, FontSize::S16x32, b"Above")).unwrap();
    ui.add(
        a,
        vspec(ViewKind::Divider, Props::Divider(DividerProps { thickness: 4, inset_start: 24, inset_end: 24 })),
    )
    .unwrap();
    ui.add(a, text(FONT_PRIMARY, FontSize::S16x32, b"Below")).unwrap();
    caption(ui, c, b"Divider h + insets (v face pending)");
}

fn build_kind_spacer(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(0, CrossAlign::Center)).unwrap();
    ui.add(r, rect(BRAND, Some(96), Some(72))).unwrap();
    ui.add(r, vspec(ViewKind::Spacer, Props::Spacer(SpacerProps { flex: 1 }))).unwrap();
    ui.add(r, rect(WARNING, Some(96), Some(72))).unwrap();
    caption(ui, c, b"Spacer flex=1 (v1 0x0 -- split pending)");
}

fn build_kind_stack(ui: &mut UiMain, c: NodeId) {
    let st = ui
        .add(c, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Start })))
        .unwrap();
    ui.add(st, rect(DIVIDER, Some(360), Some(72))).unwrap();
    ui.add(st, rect(WARNING, Some(260), Some(72))).unwrap();
    ui.add(st, rect(BRAND, Some(140), Some(72))).unwrap();
    caption(ui, c, b"Stack z = child order (top = last)");
}

fn build_kind_step(ui: &mut UiMain, c: NodeId) {
    ui.add(
        c,
        vspec(ViewKind::Step, Props::Step(StepProps { value: 2, min: 0, max: 8, step: 1, on_change: 1 })),
    )
    .unwrap();
    caption(ui, c, b"Step 2/8 (- value +)");
}

fn build_kind_select(ui: &mut UiMain, c: NodeId) {
    ui.add(c, vspec(ViewKind::Select, Props::Select(SelectProps { option_count: 4, selected: 2, on_change: 1 })))
        .unwrap();
    caption(ui, c, b"Select collapsed (shows 1-based 3)");
}

fn build_kind_input(ui: &mut UiMain, c: NodeId) {
    ui.add(
        c,
        vspec(ViewKind::Input, Props::Input(InputProps {
            placeholder: TextBuf::new(b"search apps...").unwrap(),
            max_len: 32,
            input_kind: InputKind::Text,
        })),
    )
    .unwrap();
    let filled = ui
        .add(
            c,
            vspec(ViewKind::Input, Props::Input(InputProps {
                placeholder: TextBuf::new(b"type here").unwrap(),
                max_len: 32,
                input_kind: InputKind::Text,
            })),
        )
        .unwrap();
    ui.set_input_text(filled, b"hello").unwrap();
    ui.set_focus(Some(filled)).unwrap();
    caption(ui, c, b"Input placeholder + focused caret");
}

fn build_kind_edit(ui: &mut UiMain, c: NodeId) {
    let ed = ui
        .add(
            c,
            vspec(ViewKind::Edit, Props::Edit(EditProps {
                placeholder: TextBuf::new(b"notes").unwrap(),
                max_len: 64,
                max_lines: 3,
                readonly: false,
            })),
        )
        .unwrap();
    ui.set_input_text(ed, b"hello edit").unwrap();
    caption(ui, c, b"Edit 200x96 (v1 single run)");
}

fn build_kind_canvas(ui: &mut UiMain, c: NodeId) {
    ui.add(c, vspec(ViewKind::Canvas, Props::Canvas(CanvasProps { paint_ops: 7 }))).unwrap();
    caption(ui, c, b"Canvas 200x200 static face");
}

fn build_kind_tip(ui: &mut UiMain, c: NodeId) {
    let a = ui.add(c, col(16, CrossAlign::Start)).unwrap();
    ui.add(a, text(FONT_PRIMARY, FontSize::S16x32, b"WiFi")).unwrap();
    ui.add(
        a,
        vspec(ViewKind::Tip, Props::Tip(TipProps {
            text: TextBuf::new(b"WiFi is off").unwrap(),
            placement: TipPlacement::Bottom,
        })),
    )
    .unwrap();
    caption(ui, c, b"Tip bubble (notch face pending)");
}

fn build_kind_refresh(ui: &mut UiMain, _c: NodeId) {
    // 随父填充件——Root 直挂取全幅。主树 Refresh 子拒面（add 白名单
    // 未收编）+ 发射序父先子后（子会盖上父）——现态格 = 纯顶缘拉动
    // 指示条（offset 600 → min(h,32)*0.6 = 19px REFRESH_BAR）；内容
    // 包裹组合挂路 2 配方族。
    let root = ui.root();
    let _rf = ui
        .add(root, vspec(ViewKind::Refresh, Props::Refresh(RefreshProps { refreshing: false, offset: 600, on_refresh: 1 })))
        .unwrap();
}

fn build_kind_swipeitem(ui: &mut UiMain, _c: NodeId) {
    // 随父填充件——Root 直挂全幅：两端动作缘带（reveal 25%/50% →
    // min(w,96) 比例 24/48px；行面组合归消费面——SwipeItem 叶无子）。
    let root = ui.root();
    ui.add(
        root,
        vspec(ViewKind::SwipeItem, Props::SwipeItem(SwipeItemProps {
            action_count: 2,
            reveal_start: 250,
            reveal_end: 500,
            on_action: 1,
        })),
    )
    .unwrap();
}

fn build_kind_datepick(ui: &mut UiMain, c: NodeId) {
    ui.add(
        c,
        vspec(ViewKind::DatePick, Props::DatePick(DatePickProps {
            year: 2026,
            month: 11,
            day: 15,
            min_year: 2020,
            max_year: 2035,
            on_change: 1,
        })),
    )
    .unwrap();
    caption(ui, c, b"DatePick 2026-11-15");
}

fn build_kind_timepick(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(32, CrossAlign::Center)).unwrap();
    ui.add(r, vspec(ViewKind::TimePick, Props::TimePick(TimePickProps { hour: 9, minute: 5, use_24h: true, on_change: 1 })))
        .unwrap();
    ui.add(r, vspec(ViewKind::TimePick, Props::TimePick(TimePickProps { hour: 13, minute: 45, use_24h: false, on_change: 1 })))
        .unwrap();
    caption(ui, c, b"TimePick 09:05 / 13:45 (v1 raw HH:MM)");
}

fn build_kind_collapse(ui: &mut UiMain, c: NodeId) {
    let open = ui
        .add(c, vspec(ViewKind::Collapse, Props::Collapse(CollapseProps { expanded: true, on_toggle: 1 })))
        .unwrap();
    ui.add(open, text(FONT_PRIMARY, FontSize::S16x32, b"body line one")).unwrap();
    ui.add(open, text(FONT_SECONDARY, FontSize::S16x32, b"body line two")).unwrap();
    let _closed = ui
        .add(c, vspec(ViewKind::Collapse, Props::Collapse(CollapseProps { expanded: false, on_toggle: 2 })))
        .unwrap();
    caption(ui, c, b"Collapse open (arrow down) / closed (arrow right)");
}

fn build_kind_chip(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(24, CrossAlign::Center)).unwrap();
    ui.add(
        r,
        vspec(ViewKind::Chip, Props::Chip(ChipProps { label: TextBuf::new(b"Tag on").unwrap(), selected: true, on_tap: 1 })),
    )
    .unwrap();
    ui.add(
        r,
        vspec(ViewKind::Chip, Props::Chip(ChipProps { label: TextBuf::new(b"Tag").unwrap(), selected: false, on_tap: 2 })),
    )
    .unwrap();
    caption(ui, c, b"Chip on/off (capsule v1 square approx)");
}

fn build_kind_fab(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(32, CrossAlign::Center)).unwrap();
    ui.add(
        r,
        vspec(ViewKind::Fab, Props::Fab(FabProps {
            icon: icons::ADD as u32,
            extended_label: TextBuf::new(b"").unwrap(),
            on_tap: 1,
        })),
    )
    .unwrap();
    ui.add(
        r,
        vspec(ViewKind::Fab, Props::Fab(FabProps {
            icon: icons::ADD as u32,
            extended_label: TextBuf::new(b"Add").unwrap(),
            on_tap: 2,
        })),
    )
    .unwrap();
    caption(ui, c, b"Fab circle + extended (icon face pending)");
}

fn build_kind_card(ui: &mut UiMain, c: NodeId) {
    // corner_radius=24 数据携载（v1 直角——FillRectRounded 挂路 1，
    // 合入后验收轮重生成）；elevation 2 → 阴影条 4px。
    let card = ui
        .add(c, vspec(ViewKind::Card, Props::Card(CardProps { elevation: 2, corner_radius: 24 })))
        .unwrap();
    let inner = ui.add(card, pad(24)).unwrap();
    let lines = ui.add(inner, col(8, CrossAlign::Start)).unwrap();
    ui.add(lines, text(FONT_PRIMARY, FontSize::S24x48, b"Card title")).unwrap();
    ui.add(lines, text(FONT_SECONDARY, FontSize::S16x32, b"secondary line")).unwrap();
    caption(ui, c, b"Card elev=2 r=24 (v1 square, path-1 rounds)");
}

fn build_kind_gauge(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(48, CrossAlign::Center)).unwrap();
    ui.add(r, vspec(ViewKind::Gauge, Props::Gauge(GaugeProps { value: 750, style: GaugeStyle::Ring })))
        .unwrap();
    ui.add(r, vspec(ViewKind::Gauge, Props::Gauge(GaugeProps { value: 300, style: GaugeStyle::Bar })))
        .unwrap();
    caption(ui, c, b"Gauge 75% / 30% (v1 bar face all styles)");
}

fn build_kind_qr(ui: &mut UiMain, c: NodeId) {
    // 21 模块 = 84×84；暗/亮单色对比（cmds 预算紧——无格内说明字，
    // 联络表标签承载）。真 QR 编码挂后续刀（v1 确定性伪随机图案）。
    let r = ui.add(c, row(24, CrossAlign::Center)).unwrap();
    ui.add(
        r,
        vspec(ViewKind::Qr, Props::Qr(QrProps {
            module_count: 21,
            dark_color: 0x1A_1A_1A,
            light_color: 0xFF_FF_FF,
        })),
    )
    .unwrap();
}

fn build_kind_dragreorder(ui: &mut UiMain, c: NodeId) {
    let a = ui.add(c, col(6, CrossAlign::Start)).unwrap();
    for i in 0..3u8 {
        let r = ui.add(a, row(20, CrossAlign::Center)).unwrap();
        ui.add(
            r,
            vspec(ViewKind::DragReorder, Props::DragReorder(DragReorderProps {
                item_index: i as u32,
                total_items: 3,
                on_reorder: 1,
            })),
        )
        .unwrap();
        let mut lb = *b"Row a";
        lb[4] = b'a' + i;
        ui.add(r, text(FONT_PRIMARY, FontSize::S16x32, &lb)).unwrap();
    }
    caption(ui, c, b"DragReorder handles on 3 rows");
}

fn build_kind_image(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(32, CrossAlign::Center)).unwrap();
    ui.add(r, vspec(ViewKind::Image, Props::Image(ImageProps { res: icons::THUMB, w: None, h: None })))
        .unwrap();
    ui.add(r, vspec(ViewKind::Image, Props::Image(ImageProps { res: icons::THUMB, w: Some(48), h: Some(48) })))
        .unwrap();
    caption(ui, c, b"Image res=9 24x24 native + sized 48 (blit native)");
}

fn build_kind_icon(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(32, CrossAlign::Center)).unwrap();
    for id in [icons::BACK, icons::CHECK, icons::ADD, icons::SETTINGS, icons::HOME] {
        ui.add(
            r,
            vspec(ViewKind::Icon, Props::Icon(IconProps {
                id,
                fg: FONT_PRIMARY,
                w: None,
                h: None,
            })),
        )
        .unwrap();
    }
    caption(ui, c, b"Icon 16x16 glyphs (host premix ink)");
}

fn build_kind_grid(ui: &mut UiMain, c: NodeId) {
    let g = ui
        .add(c, vspec(ViewKind::Grid, Props::Grid(GridProps {
            cols: 3,
            rows: 2,
            gap: 8,
            cross: CrossAlign::Stretch,
            w: None,
            h: None,
        })))
        .unwrap();
    for bg in [BRAND, BG_COMP, DIVIDER, WARNING, SUCCESS, BRAND_PRESSED] {
        ui.add(g, rect(bg, Some(120), Some(72))).unwrap();
    }
    caption(ui, c, b"Grid 3x2 gap=8 (theme swatch cells)");
}

fn build_kind_text(ui: &mut UiMain, c: NodeId) {
    let a = ui.add(c, col(12, CrossAlign::Start)).unwrap();
    ui.add(a, text(FONT_PRIMARY, FontSize::S8x16, b"S8x16 tier abc")).unwrap();
    ui.add(a, text(FONT_PRIMARY, FontSize::S16x32, b"S16x32 tier abc")).unwrap();
    ui.add(a, text(FONT_PRIMARY, FontSize::S24x48, b"S24x48 tier")).unwrap();
    ui.add(a, text(FONT_SECONDARY, FontSize::S16x32, b"secondary fg")).unwrap();
    caption(ui, c, b"Text scale ladder (S32x48 HD pending sheet)");
}

fn build_kind_rect(ui: &mut UiMain, c: NodeId) {
    let r = ui.add(c, row(12, CrossAlign::Center)).unwrap();
    for bg in [BRAND, BRAND_PRESSED, WARNING, SUCCESS, DIVIDER, BG_COMP] {
        ui.add(r, rect(bg, Some(56), Some(56))).unwrap();
    }
    caption(ui, c, b"Rect swatches = theme.rs light tokens");
}

fn build_kind_layout(ui: &mut UiMain, c: NodeId) {
    let p = ui.add(c, pad(16)).unwrap();
    let a = ui.add(p, col(12, CrossAlign::Stretch)).unwrap();
    ui.add(a, text(FONT_PRIMARY, FontSize::S16x32, b"Pad > Col")).unwrap();
    let r = ui.add(a, row(12, CrossAlign::Center)).unwrap();
    ui.add(r, rect(BRAND, Some(60), Some(40))).unwrap();
    ui.add(r, text(FONT_PRIMARY, FontSize::S16x32, b"Row")).unwrap();
    let np = ui.add(r, pad(12)).unwrap();
    ui.add(np, rect(WARNING, Some(60), Some(40))).unwrap();
    caption(ui, c, b"Root/Col/Row/Pad = anchor of every cell");
}

// ---------- pattern.rs 值面格（主树现实：几何常量 + ItemState 五态
// ---------- 色表 + NavStack 纯数据；23 配方族建树 fn 挂路 2） ----------

/// TopBar 56vp = 168px @3x（TOPBAR_H 单源）：返回位 + 标题 + 动作位的
/// 三段横条——主树 pattern.rs 无建树 fn，组合步骤本格单源示范。
fn build_pat_topbar(ui: &mut UiMain, c: NodeId) {
    let st = ui.add(c, stack(StackAlign::Start)).unwrap();
    ui.add(st, rect(BG_COMP, None, Some(pat::TOPBAR_H))).unwrap();
    let bar = ui.add(st, row(16, CrossAlign::Center)).unwrap();
    ui.add(
        bar,
        vspec(ViewKind::Button, Props::Button(ButtonProps {
            label: TextBuf::new(b"Back").unwrap(),
            style: ButtonStyle::Filled,
            on_tap: 1,
        })),
    )
    .unwrap();
    ui.add(bar, text(FONT_PRIMARY, FontSize::S24x48, b"Settings")).unwrap();
    ui.add(bar, rect(BG_COMP, Some(48), Some(8))).unwrap();
    ui.add(
        bar,
        vspec(ViewKind::Button, Props::Button(ButtonProps {
            label: TextBuf::new(b"+").unwrap(),
            style: ButtonStyle::Outlined,
            on_tap: 2,
        })),
    )
    .unwrap();
    caption(ui, c, b"pattern TopBar 56vp = TOPBAR_H 168px @3x");
}

/// BottomTabs 48vp = 144px @3x（BOTTOMTABS_H 单源）：图标 + 小字四席，
/// active 席 brand 着色（Tabs 件底座之上的结构位——pattern 头注）。
fn build_pat_bottomtabs(ui: &mut UiMain, c: NodeId) {
    let st = ui.add(c, stack(StackAlign::Start)).unwrap();
    ui.add(st, rect(BG_COMP, None, Some(pat::BOTTOMTABS_H))).unwrap();
    let bar = ui.add(st, row(32, CrossAlign::Center)).unwrap();
    let tabs: [(u16, &[u8]); 4] = [
        (icons::HOME, b"Home"),
        (icons::ADD, b"Add"),
        (icons::SETTINGS, b"Set"),
        (icons::CHECK, b"Ok"),
    ];
    for (i, (icon, lb)) in tabs.iter().enumerate() {
        let t = ui.add(bar, col(4, CrossAlign::Center)).unwrap();
        ui.add(
            t,
            vspec(ViewKind::Icon, Props::Icon(IconProps { id: *icon, fg: FONT_PRIMARY, w: None, h: None })),
        )
        .unwrap();
        let fg = if i == 0 { BRAND } else { FONT_SECONDARY };
        ui.add(t, text(fg, FontSize::S8x16, lb)).unwrap();
    }
    caption(ui, c, b"pattern BottomTabs 48vp = 144px active=0");
}

/// ListItem 48vp 五态（LISTITEM_H × ItemState::bg 单源）——主树五态
/// 色表纯数据零 paint 消费，本格把值面搬上屏（focus 档值面同 normal
/// ——pattern.rs 注记原样呈现）。
fn build_pat_listitem_5state(ui: &mut UiMain, c: NodeId) {
    let states: [(ItemState, &[u8]); 5] = [
        (ItemState::Normal, b"Normal"),
        (ItemState::Pressed, b"Pressed"),
        (ItemState::Selected, b"Selected"),
        (ItemState::Disabled, b"Disabled"),
        (ItemState::Focus, b"Focus (= Normal, v1)"),
    ];
    for (st_val, lb) in states {
        let st = ui.add(c, stack(StackAlign::Start)).unwrap();
        ui.add(st, rect(rgb(st_val.bg()), None, Some(pat::LISTITEM_H))).unwrap();
        let p = ui.add(st, pad(20)).unwrap();
        ui.add(p, text(FONT_PRIMARY, FontSize::S16x32, lb)).unwrap();
    }
    caption(ui, c, b"pattern ItemState 5-state bg, rows = LISTITEM_H 144px");
}

/// NavStack 纯数据页栈（push 三页 → top 渲染）——子树操作归消费面
/// （pattern.rs 头注）：本格以 set_offset 叠三页 Rect + 顶页标题自
/// `nav.top()` 值面取文（key/word 单源演示）。
fn build_pat_navstack(ui: &mut UiMain, c: NodeId) {
    fn page(key: u32, name: &[u8]) -> NavPage {
        let mut title = [0u8; 16];
        let n = name.len().min(16);
        title[..n].copy_from_slice(&name[..n]);
        NavPage { key, title, title_len: n as u8 }
    }
    let mut nav = NavStack::new();
    nav.push(page(1, b"Root")).unwrap();
    nav.push(page(2, b"List")).unwrap();
    nav.push(page(3, b"Detail")).unwrap();
    let st = ui.add(c, stack(StackAlign::Start)).unwrap();
    let p1 = ui.add(st, rect(BG_COMP, None, Some(200))).unwrap();
    let p2 = ui.add(st, rect(SUCCESS, None, Some(200))).unwrap();
    let p3 = ui.add(st, rect(BRAND, None, Some(200))).unwrap();
    ui.set_offset(p1, 0, 0).unwrap();
    ui.set_offset(p2, 16, 20).unwrap();
    ui.set_offset(p3, 32, 40).unwrap();
    let top = nav.top().unwrap();
    let title = &top.title[..top.title_len as usize];
    let tx = ui.add(st, text(Color { r: 0xFF, g: 0xFF, b: 0xFF }, FontSize::S24x48, title)).unwrap();
    ui.set_offset(tx, 64, 120).unwrap();
    caption(ui, c, b"pattern NavStack push x3, top = key 3 (offset stack)");
}

// ---------- 格表 ----------

const CELLS: &[CellDef] = &[
    // kind 格（41 ViewKind 全覆盖）
    CellDef { name: "kind_button", label: "Button Filled/Outlined/Text", w: 460, h: 170, build: build_kind_button },
    CellDef { name: "kind_button_states", label: "Button N/P/D (P=press hold; D=path-1 pending)", w: 460, h: 200, build: build_kind_button_states },
    CellDef { name: "kind_toggle", label: "Toggle off/on", w: 460, h: 150, build: build_kind_toggle },
    CellDef { name: "kind_slider", label: "Slider 60%", w: 460, h: 140, build: build_kind_slider },
    CellDef { name: "kind_progress", label: "Progress 0/40/90 + Indet + Circular (v1 linear)", w: 460, h: 260, build: build_kind_progress },
    CellDef { name: "kind_tabs", label: "Tabs 3 pages active=1 (strip + sibling content)", w: 460, h: 410, build: build_kind_tabs },
    CellDef { name: "kind_sbar", label: "Sbar H 20-50% + V 10-60%", w: 460, h: 300, build: build_kind_sbar },
    CellDef { name: "kind_scroll", label: "Scroll offset=200 (clip=path-1, root-mounted)", w: 460, h: 300, build: build_kind_scroll },
    CellDef { name: "kind_list", label: "List 4 rows h=72", w: 460, h: 380, build: build_kind_list },
    CellDef { name: "kind_check", label: "Check off/on", w: 460, h: 150, build: build_kind_check },
    CellDef { name: "kind_radio", label: "Radio group 7 off/on", w: 460, h: 150, build: build_kind_radio },
    CellDef { name: "kind_seg", label: "Seg 3 active=1", w: 460, h: 140, build: build_kind_seg },
    CellDef { name: "kind_badge", label: "Badge 5 / 99+ / dot", w: 460, h: 150, build: build_kind_badge },
    CellDef { name: "kind_divider", label: "Divider h + insets", w: 460, h: 190, build: build_kind_divider },
    CellDef { name: "kind_spacer", label: "Spacer flex=1 (v1 0x0)", w: 460, h: 160, build: build_kind_spacer },
    CellDef { name: "kind_stack", label: "Stack z = child order", w: 460, h: 210, build: build_kind_stack },
    CellDef { name: "kind_step", label: "Step 2/8 (- value +)", w: 460, h: 140, build: build_kind_step },
    CellDef { name: "kind_select", label: "Select collapsed (3/4)", w: 460, h: 150, build: build_kind_select },
    CellDef { name: "kind_input", label: "Input placeholder + focused caret", w: 460, h: 220, build: build_kind_input },
    CellDef { name: "kind_edit", label: "Edit 200x96 (v1 single run)", w: 460, h: 240, build: build_kind_edit },
    CellDef { name: "kind_canvas", label: "Canvas 200x200 static face", w: 460, h: 250, build: build_kind_canvas },
    CellDef { name: "kind_tip", label: "Tip bubble", w: 460, h: 180, build: build_kind_tip },
    CellDef { name: "kind_refresh", label: "Refresh pull bar 60% (no-child face, root-mounted)", w: 460, h: 120, build: build_kind_refresh },
    CellDef { name: "kind_swipeitem", label: "SwipeItem reveal 25%/50% (root-mounted)", w: 460, h: 200, build: build_kind_swipeitem },
    CellDef { name: "kind_datepick", label: "DatePick 2026-11-15", w: 460, h: 150, build: build_kind_datepick },
    CellDef { name: "kind_timepick", label: "TimePick 09:05 / 13:45 (v1 raw)", w: 460, h: 150, build: build_kind_timepick },
    CellDef { name: "kind_collapse", label: "Collapse open/closed", w: 460, h: 300, build: build_kind_collapse },
    CellDef { name: "kind_chip", label: "Chip on/off (v1 square)", w: 460, h: 150, build: build_kind_chip },
    CellDef { name: "kind_fab", label: "Fab circle + extended", w: 460, h: 160, build: build_kind_fab },
    CellDef { name: "kind_card", label: "Card elev=2 r=24 (v1 square)", w: 460, h: 290, build: build_kind_card },
    CellDef { name: "kind_gauge", label: "Gauge 75% / 30% (v1 bar face)", w: 460, h: 220, build: build_kind_gauge },
    CellDef { name: "kind_qr", label: "Qr module_count=21 (v1 pattern)", w: 460, h: 190, build: build_kind_qr },
    CellDef { name: "kind_dragreorder", label: "DragReorder handles 3 rows", w: 460, h: 250, build: build_kind_dragreorder },
    CellDef { name: "kind_image", label: "Image res=9 (ARGB host source)", w: 460, h: 170, build: build_kind_image },
    CellDef { name: "kind_icon", label: "Icon 16x16 glyphs (host premix)", w: 460, h: 170, build: build_kind_icon },
    CellDef { name: "kind_grid", label: "Grid 3x2 gap=8", w: 460, h: 240, build: build_kind_grid },
    CellDef { name: "kind_text", label: "Text scale ladder", w: 460, h: 260, build: build_kind_text },
    CellDef { name: "kind_rect", label: "Rect swatches = theme light tokens", w: 460, h: 160, build: build_kind_rect },
    CellDef { name: "kind_layout", label: "Root/Col/Row/Pad anchor", w: 460, h: 240, build: build_kind_layout },
    // pattern.rs 值面格（主树现实四件全件族）
    CellDef { name: "pat_topbar", label: "pattern TopBar 56vp = 168px @3x", w: 460, h: 280, build: build_pat_topbar },
    CellDef { name: "pat_bottomtabs", label: "pattern BottomTabs 48vp = 144px active=0", w: 460, h: 280, build: build_pat_bottomtabs },
    CellDef { name: "pat_listitem_5state", label: "pattern ItemState 5-state rows LISTITEM_H", w: 460, h: 900, build: build_pat_listitem_5state },
    CellDef { name: "pat_navstack", label: "pattern NavStack push x3 (top key 3)", w: 460, h: 340, build: build_pat_navstack },
];

fn render_all() -> Vec<(CellDef, Vec<u8>, u64)> {
    CELLS
        .iter()
        .map(|c| {
            let fb = render(c.w, c.h, |ui, anchor| (c.build)(ui, anchor));
            let h = fnv1a(&fb);
            (*c, fb, h)
        })
        .collect()
}

fn gallery_dir() -> PathBuf {
    std::env::var_os("WAVE2_GALLERY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../WAVE2-GALLERY"))
}

#[test]
fn gallery2_dump_cells() {
    let dir = gallery_dir();
    let ppm = dir.join("ppm");
    std::fs::create_dir_all(&ppm).unwrap();
    let mut tsv = String::from("name\tlabel\tw\th\tfnv1a64\n");
    for (c, fb, h) in render_all() {
        let stride = c.w as usize * 4;
        let mut ppm_body = format!("P6\n{} {}\n255\n", c.w, c.h).into_bytes();
        for y in 0..c.h as usize {
            let row = &fb[y * stride..y * stride + stride];
            for px in row.chunks_exact(4) {
                // Rgbx.pack = R<<16|G<<8|B，小端内存字节序 [B,G,R,X]——
                // PPM P6 通道序 RGB，反取。
                ppm_body.push(px[2]);
                ppm_body.push(px[1]);
                ppm_body.push(px[0]);
            }
        }
        std::fs::write(ppm.join(format!("{}.ppm", c.name)), &ppm_body).unwrap();
        tsv.push_str(&format!("{}\t{}\t{}\t{}\t{:016x}\n", c.name, c.label, c.w, c.h, h));
    }
    std::fs::write(dir.join("cells.tsv"), tsv).unwrap();
}

#[test]
fn gallery2_golden_hashes() {
    // 像素哈希 golden（首建回填——改视觉/引擎渲染即红，源画廊双轨
    // 同律；主树 v1 现态（直角/无 disabled/无 clip）钉板——路 1/路 2
    // 合入后须首跑回填（dump 拿新值再钉），验收轮重生成）。
    const GOLDEN: &[(&str, u64)] = &[
        ("kind_button", 0x7da323b93cb7317a),
        ("kind_button_states", 0x64db856aed3b07a1),
        ("kind_toggle", 0xdb31e96be233ba2a),
        ("kind_slider", 0x1a74f0304f0c6dbd),
        ("kind_progress", 0x99bcfc2ea43538d2),
        ("kind_tabs", 0xa6c827c5f37db73e),
        ("kind_sbar", 0xd7d5e42f6dc6f36d),
        ("kind_scroll", 0x2ec3b91393b9c125),
        ("kind_list", 0x3c0ef7c879957de5),
        ("kind_check", 0x278b5642bc9812be),
        ("kind_radio", 0xc694be6ee8cea5b2),
        ("kind_seg", 0x38a188adb438ed65),
        ("kind_badge", 0x9271b63f948a2396),
        ("kind_divider", 0x402098e8c612870a),
        ("kind_spacer", 0x945e2ba34a5b9c8a),
        ("kind_stack", 0xde3c446edc32b455),
        ("kind_step", 0x3804f6dcb9c826e6),
        ("kind_select", 0xf5988cfc5db90688),
        // kind_input：B9 caret 居中同式（+14 → (h−16)/2 = +16）重钉。
        ("kind_input", 0xa9de573fc08899a1),
        ("kind_edit", 0x95a720941ffbc112),
        ("kind_canvas", 0xd06ef53af02c0b25),
        ("kind_tip", 0xcde97a00751db9ac),
        ("kind_refresh", 0xa35fda372c5a80bd),
        ("kind_swipeitem", 0x70daa50f501a34a5),
        ("kind_datepick", 0xc6335a8da789e470),
        ("kind_timepick", 0x18c1e721f6ba276b),
        ("kind_collapse", 0xce0b3704d9e819aa),
        ("kind_chip", 0xa4abeec93830b25d),
        ("kind_fab", 0xdd9faa3c5a36d0a5),
        ("kind_card", 0xdff78874d47d35ad),
        ("kind_gauge", 0x9db32e13b00969f5),
        ("kind_qr", 0xcca026b494e06be5),
        ("kind_dragreorder", 0xe7857b9f66eadd15),
        ("kind_image", 0x90d2a30474935a32),
        ("kind_icon", 0xfcf9c80302fd4391),
        ("kind_grid", 0xf8aec51d796d3f52),
        ("kind_text", 0x2ab86b2d5c70dce9),
        ("kind_rect", 0xe1bd11d439fe16e2),
        ("kind_layout", 0x7ebae4bd3fa1c155),
        ("pat_topbar", 0xaa9fb801f49dce37),
        ("pat_bottomtabs", 0x516ad0dd5d6ce1c4),
        ("pat_listitem_5state", 0x64d6e985951d9d95),
        ("pat_navstack", 0x99cd64f3abedebb8),
    ];
    let have: HashMap<&str, u64> = render_all().into_iter().map(|(c, _, h)| (c.name, h)).collect();
    for (name, want) in GOLDEN {
        match have.get(name) {
            Some(got) => assert_eq!(got, want, "格 {name} 像素漂移"),
            None => panic!("格 {name} 不在格表"),
        }
    }
    // GOLDEN 表必须覆盖格表全量（防格增补后 golden 面静默缺位）。
    assert_eq!(GOLDEN.len(), CELLS.len(), "GOLDEN 覆盖数 = 格表数（首建回填后钉板）");
}
