//! GUI-3 §8 layout 行：Col/Row/Pad 测量-放置 + Text 内在尺寸 + Measure
//! 缓存 + set_viewport 全量重排。
//!
//! - 摆位钉板（值级）：Col 游标+gap / Row 十字轴 / Pad 内缩 / 嵌套容器
//!   / Option（None）包裹 vs 定值两档。
//! - Text 内在尺寸 = len × cell 档位（FontSize::cell_w/cell_h 单源）。
//! - Measure 缓存：改色 0 重测（LayoutStat::measured == 0）、改文仅脏
//!   子树 + 聚合祖先链。
//! - set_viewport：全量 layout_dirty + FULL damage。
//!
//! 确定性：无随机无时钟；bounds 全值级断言。

use berry_ui::desc::{ColProps, PadProps, Props, RectProps, RowProps, TextProps, TextStyle, ViewKind, ViewSpec};
use berry_ui::damage::RectI;
use berry_ui::layout::LayoutStat;
use berry_ui::Ui;
use berry_gfx::font::FontSize;
use berry_gfx::cmd::Color;

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

fn rect(w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w, h }),
    }
}

fn text(bytes: &[u8], scale: FontSize) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: bg(9, 9, 9), bg: None, scale },
            text: berry_ui::TextBuf::new(bytes).unwrap(),
        }),
    }
}

fn container(kind: ViewKind, v: u32) -> ViewSpec {
    let props = match kind {
        ViewKind::Col => Props::Col(ColProps { gap: v }),
        ViewKind::Row => Props::Row(RowProps { gap: v }),
        ViewKind::Pad => Props::Pad(PadProps { all: v }),
        _ => Props::Root,
    };
    ViewSpec { kind, key: 0, props }
}

#[test]
fn col_cursor_gap_and_option_wrapping() {
    // Root(100x200) → Col{gap:4}{ A Rect(20,10) 定值, B Rect(None,None)
    // 随父, C Text "hi" S8x16 内在 (16,16) }
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, container(ViewKind::Col, 4)).unwrap();
    let a = ui.add(col, rect(Some(20), Some(10))).unwrap();
    let b = ui.add(col, rect(None, None)).unwrap();
    let c = ui.add(col, text(b"hi", FontSize::S8x16)).unwrap();

    let stat = ui.layout();
    assert_eq!(stat, LayoutStat { measured: 5, placed: 5 }, "首排全树测量+摆放（5 节点）");
    assert_eq!(ui.bounds(col).unwrap(), RectI::new(0, 0, 100, 34), "Col 十字轴吃满/主轴 = Σ子+gap×2 = 10+0+16+8");
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 20, 10));
    assert_eq!(ui.bounds(b).unwrap(), RectI::new(0, 14, 100, 0), "B 主轴 None 折内在 0（游标不推进主体）、十字轴吃满");
    assert_eq!(ui.bounds(c).unwrap(), RectI::new(0, 18, 16, 16), "C 落位 = 10+gap4+0+gap4 = 18");

    // 无脏位 → 全缓存命中
    assert_eq!(ui.layout(), LayoutStat { measured: 0, placed: 0 });
}

#[test]
fn row_cross_axis_and_pad_inset() {
    // Row：主轴 Σ、十字轴 max/吃满
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let row = ui.add(root, container(ViewKind::Row, 3)).unwrap();
    let a = ui.add(row, rect(Some(10), Some(20))).unwrap();
    let b = ui.add(row, rect(None, None)).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(row).unwrap(), RectI::new(0, 0, 13, 200), "Row 主轴 = 10+0+gap3 = 13、十字轴吃满 200");
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 10, 20));
    assert_eq!(ui.bounds(b).unwrap(), RectI::new(13, 0, 0, 200), "B 主轴 None → x 推进到 13、宽折 0、高吃满");

    // Pad：四向内缩 + None 子吃满缩余
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let pad = ui.add(root, container(ViewKind::Pad, 5)).unwrap();
    let d = ui.add(pad, rect(None, None)).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(pad).unwrap(), RectI::new(0, 0, 100, 200), "Pad 整体吃满分配");
    assert_eq!(ui.bounds(d).unwrap(), RectI::new(5, 5, 90, 190), "D = inset(5,5) + 分配 (100−10, 200−10)");
}

#[test]
fn nested_containers_preorder_placement() {
    // Root → Col{gap:0}{ A(20x10), Row{gap:2}{ B(10x10), C(10x10) } }
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, container(ViewKind::Col, 0)).unwrap();
    let a = ui.add(col, rect(Some(20), Some(10))).unwrap();
    let row = ui.add(col, container(ViewKind::Row, 2)).unwrap();
    let b = ui.add(row, rect(Some(10), Some(10))).unwrap();
    let c = ui.add(row, rect(Some(10), Some(10))).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 20, 10));
    assert_eq!(ui.bounds(row).unwrap(), RectI::new(0, 10, 22, 10), "Row 十字轴吃满其 Col 主轴分配 h=10");
    assert_eq!(ui.bounds(b).unwrap(), RectI::new(0, 10, 10, 10));
    assert_eq!(ui.bounds(c).unwrap(), RectI::new(12, 10, 10, 10), "C.x = B 宽 10 + gap 2");
    assert_eq!(ui.bounds(col).unwrap(), RectI::new(0, 0, 100, 20));
}

#[test]
fn text_intrinsic_len_times_cell_tiers() {
    // "berry" = 5 字节 × cell 档位（FontSize::cell_w/cell_h 单源对拍）
    let mut ui = Ui::<16>::new(200, 200);
    let root = ui.root();
    let t1 = ui.add(root, text(b"berry", FontSize::S8x16)).unwrap();
    let t2 = ui.add(root, text(b"berry", FontSize::S16x32)).unwrap();
    let t3 = ui.add(root, text(b"berry", FontSize::S24x48)).unwrap();
    ui.layout();
    // 内在尺寸 = len × cell_w（宽）/ cell_h（高）——FontSize 单源对拍
    let w = 5 * FontSize::S8x16.cell_w();
    let h = FontSize::S8x16.cell_h();
    assert_eq!(ui.bounds(t1).unwrap(), RectI::new(0, 0, w as i32, h as i32));
    assert_eq!(ui.bounds(t1).unwrap(), RectI::new(0, 0, 40, 16), "5×8x16");
    assert_eq!(ui.bounds(t2).unwrap(), RectI::new(0, 0, 80, 32), "5×16x32");
    assert_eq!(ui.bounds(t3).unwrap(), RectI::new(0, 0, 120, 48), "5×24x48");
    assert_eq!(ui.kind_of(t1).unwrap(), ViewKind::Text);
}

#[test]
fn measure_cache_color_change_zero_remeasure() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, container(ViewKind::Col, 4)).unwrap();
    let a = ui.add(col, rect(Some(20), Some(10))).unwrap();
    let _b = ui.add(col, rect(None, None)).unwrap();
    let _c = ui.add(col, text(b"hi", FontSize::S8x16)).unwrap();
    ui.layout();
    ui.drain_damage();

    // 纯着色变（w/h 同）：paint 脏、0 重测
    ui.set_props(a, Props::Rect(RectProps { bg: bg(7, 7, 7), w: Some(20), h: Some(10) }))
        .unwrap();
    assert_eq!(ui.layout(), LayoutStat { measured: 0, placed: 0 }, "改色面全缓存命中");
    let d = ui.drain_damage();
    assert_eq!(d.count, 1, "同界重绘恰一条脏区");
    assert_eq!(
        d.rects[0],
        berry_guiproto::Rect { x: 0, y: 0, w: 20, h: 10 },
        "脏区 = A 旧界（同界重绘）"
    );
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 20, 10), "摆位不动");
}

#[test]
fn measure_cache_text_change_remeasures_subtree_only() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, container(ViewKind::Col, 4)).unwrap();
    let a = ui.add(col, rect(Some(20), Some(10))).unwrap();
    let _b = ui.add(col, rect(None, None)).unwrap();
    let c = ui.add(col, text(b"hi", FontSize::S8x16)).unwrap();
    ui.layout();
    ui.drain_damage();

    // 改文 "hi"→"hey"：内在 (16,16)→(24,16)；重测 = C + Col + Root = 3
    ui.set_text(c, b"hey").unwrap();
    let stat = ui.layout();
    assert_eq!(stat.measured, 3, "仅脏子树 C + 聚合祖先链 Col/Root");
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 20, 10), "兄弟 A 不动");
    assert_eq!(ui.bounds(c).unwrap(), RectI::new(0, 18, 24, 16), "新内在 3×8=24 宽");
    // 再排全缓存
    assert_eq!(ui.layout(), LayoutStat { measured: 0, placed: 0 });
}

#[test]
fn set_viewport_full_relayout_and_full_damage() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, container(ViewKind::Col, 4)).unwrap();
    let a = ui.add(col, rect(Some(20), Some(10))).unwrap();
    let _b = ui.add(col, rect(None, None)).unwrap();
    let _c = ui.add(col, text(b"hi", FontSize::S8x16)).unwrap();
    ui.layout();
    ui.drain_damage();

    ui.set_viewport(200, 100);
    assert_eq!(ui.viewport(), (200, 100));
    let d = ui.damage();
    assert!(d.full, "set_viewport → FULL damage");
    assert!(!d.is_empty(), "FULL 语义 = 非空集");
    let stat = ui.layout();
    assert_eq!(stat, LayoutStat { measured: 5, placed: 5 }, "全量重排");
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 200, 100));
    assert_eq!(ui.bounds(col).unwrap(), RectI::new(0, 0, 200, 34), "十字轴吃满新宽");
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 20, 10));
}
