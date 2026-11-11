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

use berry_ui::desc::{
    ColProps, GridProps, IconProps, ImageProps, PadProps, Props, RectProps, RowProps, TextProps,
    TextStyle, ViewKind, ViewSpec,
};
use berry_ui::damage::RectI;
use berry_ui::layout::LayoutStat;
use berry_ui::{NodeId, Ui, UiError};
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
        ViewKind::Col => Props::Col(ColProps { gap: v, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }),
        ViewKind::Row => Props::Row(RowProps { gap: v, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }),
        ViewKind::Pad => Props::Pad(PadProps { all: v }),
        _ => Props::Root,
    };
    ViewSpec { kind, key: 0, props }
}

/// 指定 cross 档的 Col/Row（GUI-6 §4.1 十字轴钉板用）。
fn cross_container(kind: ViewKind, gap: u32, cross: berry_ui::layout::CrossAlign) -> ViewSpec {
    let props = match kind {
        ViewKind::Col => Props::Col(ColProps { gap, w: None, h: None, cross }),
        ViewKind::Row => Props::Row(RowProps { gap, w: None, h: None, cross }),
        _ => Props::Root,
    };
    ViewSpec { kind, key: 0, props }
}

/// Grid spec（§4.2——w/h 定尺寸缺省 None）。
fn grid(cols: u8, rows: u8, gap: u32, cross: berry_ui::layout::CrossAlign) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Grid,
        key: 0,
        props: Props::Grid(GridProps { cols, rows, gap, cross, w: None, h: None }),
    }
}

/// 随父型拉伸子（§4.1 拉伸矩阵 + §11-9 轨格钉板用）：Pad(all=0) 包
/// 定值 rect——内在恰 = rect 定值，自身 w/h None → Stretch 档随轨格
/// 吃满双轴。直挂 rect 带定值 = HAS_SIZE 钉死拉不动（§4.1 定尺寸覆盖
/// 一切）、Text/Image/Icon = 内在型叶豁免不吃轨——Pad 是「有内在且
/// 双轴随父」的最小载体。
fn stretchy(ui: &mut Ui<16>, parent: NodeId, w: u32, h: u32) -> NodeId {
    let pad = ui.add(parent, container(ViewKind::Pad, 0)).unwrap();
    ui.add(pad, rect(Some(w), Some(h))).unwrap();
    pad
}

fn image(res: u16, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec { kind: ViewKind::Image, key: 0, props: Props::Image(ImageProps { res, w, h }) }
}

fn icon(id: u16, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Icon,
        key: 0,
        props: Props::Icon(IconProps { id, fg: bg(10, 20, 30), w, h }),
    }
}

/// 双注册供件（Icon id=7 → 12×6；Image res=9 → 4×4）——布局面只消费
/// dims；未注册面（§11-14）另走默认空供件（host 测试恒注册纪律）。
struct FixedSrc;
static FIXED_SRC: FixedSrc = FixedSrc;
impl berry_ui::metrics::IconSource for FixedSrc {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        match id {
            7 => Some((12, 6)),
            9 => Some((4, 4)),
            _ => None,
        }
    }
    fn pixels(&self, _id: u16) -> Option<&[u32]> {
        None
    }
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
fn a_face_same_text_still_full_relayout() {
    // GUI-6 §11.1-1 A 面保持锚：set_text 无旧值比较——同文（字节相等）
    // 换文也全量 relayout（现状浪费保留 = A/B 对照的诚实基线；段 A 基座
    // 不得误伤，B 面三态归 feature s7——段 E）。§11.1-7 B 侧面 = 同场景
    // cfg 分断言（双态组织 §11.1-17：A 断言逐字保持，B 断言零操作）。
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, container(ViewKind::Col, 4)).unwrap();
    let c = ui.add(col, text(b"hi", FontSize::S8x16)).unwrap();
    ui.layout();
    ui.drain_damage();

    ui.set_text(c, b"hi").unwrap(); // 同文
    let stat = ui.layout();
    #[cfg(not(feature = "s7"))]
    assert!(stat.measured >= 1, "A 面同文换文仍重测（measured≥1）");
    #[cfg(feature = "s7")]
    assert_eq!(stat.measured, 0, "B 面同文 = D3 三态之首：零操作直返零重测");
    assert_eq!(ui.bounds(c).unwrap(), RectI::new(0, 0, 16, 16), "摆位零漂");
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

// ---------- GUI-6 段C 钉板（§4.1 十字轴 / §4.2 Grid / §4.3 叶尺寸） ----------

#[test]
fn col_cross_start_center_intrinsic_children() {
    // §4.1 矩阵 None 行：Start = 子内在宽、Center = 内在宽 + 居中摆位
    //（Text 内在型叶豁免恒 16；Row 随父型子在 Start 档保内在 22——非拉满）
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, cross_container(ViewKind::Col, 0, berry_ui::layout::CrossAlign::Start)).unwrap();
    let t = ui.add(col, text(b"hi", FontSize::S8x16)).unwrap();
    let row = ui.add(col, container(ViewKind::Row, 2)).unwrap();
    ui.add(row, rect(Some(10), Some(10))).unwrap();
    ui.add(row, rect(Some(10), Some(10))).unwrap();
    let z = ui.add(col, rect(None, None)).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 16, 16), "Text 内在宽（豁免行）");
    assert_eq!(ui.bounds(row).unwrap(), RectI::new(0, 16, 22, 10), "Start = Row 子内在宽 22");
    assert_eq!(ui.bounds(z).unwrap(), RectI::new(0, 26, 0, 0), "Rect(None) Start 档内在 0（vs Stretch 100）");

    // 换 Center：摆位 (可用-内在)/2，子宽仍内在
    ui.set_props(
        col,
        Props::Col(ColProps {
            gap: 0,
            w: None,
            h: None,
            cross: berry_ui::layout::CrossAlign::Center,
        }),
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(42, 0, 16, 16), "(100-16)/2 = 42");
    assert_eq!(ui.bounds(row).unwrap(), RectI::new(39, 16, 22, 10), "(100-22)/2 = 39");
    assert_eq!(ui.bounds(z).unwrap(), RectI::new(50, 26, 0, 0), "(100-0)/2 = 50");
}

#[test]
fn row_cross_start_center_intrinsic_children() {
    // Row 镜像：cross 作用于纵轴——Start = 子内在高、Center = 居中
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let row = ui.add(root, cross_container(ViewKind::Row, 0, berry_ui::layout::CrossAlign::Start)).unwrap();
    let t = ui.add(row, text(b"hi", FontSize::S8x16)).unwrap();
    let col = ui.add(row, container(ViewKind::Col, 0)).unwrap();
    ui.add(col, rect(Some(10), Some(10))).unwrap();
    ui.add(col, rect(Some(10), Some(10))).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(row).unwrap(), RectI::new(0, 0, 26, 200), "Row 主轴 Σ=26、十字吃满 200");
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 16, 16), "Start = Text 内在高 16");
    assert_eq!(ui.bounds(col).unwrap(), RectI::new(16, 0, 10, 20), "Start = Col 子内在高 20（非拉满 200）");

    ui.set_props(
        row,
        Props::Row(RowProps {
            gap: 0,
            w: None,
            h: None,
            cross: berry_ui::layout::CrossAlign::Center,
        }),
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 92, 16, 16), "(200-16)/2 = 92");
    assert_eq!(ui.bounds(col).unwrap(), RectI::new(16, 90, 10, 20), "(200-20)/2 = 90");
}

#[test]
fn has_size_fixed_cross_overrides_alignment() {
    // §4.1 矩阵 Some 行：定尺寸覆盖一切——cross 档换不动子宽（40 恒定）
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, cross_container(ViewKind::Col, 0, berry_ui::layout::CrossAlign::Start)).unwrap();
    let inner = ui.add(
        col,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(40),
                h: None,
                cross: berry_ui::layout::CrossAlign::Stretch,
            }),
        },
    )
    .unwrap();
    let r = ui.add(inner, rect(Some(6), Some(8))).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(inner).unwrap(), RectI::new(0, 0, 40, 8), "w=Some(40) 覆盖 Start 档");
    assert_eq!(ui.bounds(r).unwrap(), RectI::new(0, 0, 6, 8));

    ui.set_props(
        col,
        Props::Col(ColProps {
            gap: 0,
            w: None,
            h: None,
            cross: berry_ui::layout::CrossAlign::Center,
        }),
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(inner).unwrap(), RectI::new(30, 0, 40, 8), "Center 摆位随内在 40：(100-40)/2 = 30");
}

#[test]
fn has_size_main_axis_constant_slot() {
    // §4.1 主轴定尺寸：h=Some(30) 占位恒 30——内在 8 不改游标推进
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, container(ViewKind::Col, 4)).unwrap();
    let a = ui.add(
        col,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: None,
                h: Some(30),
                cross: berry_ui::layout::CrossAlign::Stretch,
            }),
        },
    )
    .unwrap();
    let r = ui.add(a, rect(Some(6), Some(8))).unwrap();
    let b = ui.add(col, rect(Some(10), Some(10))).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 100, 30), "定尺寸 h=30 + 十字 Stretch 吃满");
    assert_eq!(ui.bounds(r).unwrap(), RectI::new(0, 0, 6, 8), "内在测量仍跑（子摆位不受影响）");
    assert_eq!(ui.bounds(b).unwrap(), RectI::new(0, 34, 10, 10), "游标推进 30+gap4 = 34");
}

#[test]
fn grid_track_arithmetic_row_major() {
    // §11-9 钉板：3×2 + gap=4 + viewport 100×∞（高无约束）→
    // 轨宽 (100-8)/3=30；行主序 cell 原点 (0,0)(34,0)(68,0)(0,14)(34,14)(68,14)
    let mut ui = Ui::<16>::new(100, 1000);
    let root = ui.root();
    let g = ui.add(root, grid(3, 2, 4, berry_ui::layout::CrossAlign::Stretch)).unwrap();
    let mut kids = [None; 6];
    for k in kids.iter_mut() {
        *k = Some(stretchy(&mut ui, g, 10, 10));
    }
    ui.layout();
    assert_eq!(ui.bounds(g).unwrap(), RectI::new(0, 0, 100, 24), "Grid 十字吃满 100、主轴内在 10+4+10 = 24");
    // Stretch 档：Pad 包 rect（内在 10×10、无定值）拉满格 30×10
    let want = [
        (0, 0, 30, 10),
        (34, 0, 30, 10),
        (68, 0, 30, 10),
        (0, 14, 30, 10),
        (34, 14, 30, 10),
        (68, 14, 30, 10),
    ];
    for (i, k) in kids.iter().enumerate() {
        let b = ui.bounds(k.unwrap()).unwrap();
        assert_eq!((b.x, b.y, b.w, b.h), want[i], "子 {i} 行主序坐标");
    }

    // 对照组：cross=Start + 定值子 → 原点同轨、子保内在 10×10
    let mut ui = Ui::<16>::new(100, 1000);
    let root = ui.root();
    let g = ui.add(root, grid(3, 2, 4, berry_ui::layout::CrossAlign::Start)).unwrap();
    let k0 = ui.add(g, rect(Some(10), Some(10))).unwrap();
    ui.add(g, rect(Some(10), Some(10))).unwrap();
    ui.add(g, rect(Some(10), Some(10))).unwrap();
    let k3 = ui.add(g, rect(Some(10), Some(10))).unwrap();
    ui.add(g, rect(Some(10), Some(10))).unwrap();
    ui.add(g, rect(Some(10), Some(10))).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(g).unwrap(), RectI::new(0, 0, 100, 24), "Grid 自身十字随 Root 拉满（Stretch 内在 24 同）");
    assert_eq!(ui.bounds(k0).unwrap(), RectI::new(0, 0, 10, 10), "Start 档子保定值");
    assert_eq!(ui.bounds(k3).unwrap(), RectI::new(0, 14, 10, 10), "行 1 起始 y = 10+4 = 14");
}

#[test]
fn grid_intrinsic_column_and_row_aggregation() {
    // §4.2 measure：列宽 = 各列 max 和 + gap、行高 = 各行 max 和 + gap
    //（2×2 gap=3：列 [20,5] 行 [10,4] → 内在 28×17）；Col cross=Start 下可见
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, cross_container(ViewKind::Col, 0, berry_ui::layout::CrossAlign::Start)).unwrap();
    let g = ui.add(col, grid(2, 2, 3, berry_ui::layout::CrossAlign::Stretch)).unwrap();
    let a = ui.add(g, rect(Some(20), Some(10))).unwrap();
    let _b = ui.add(g, rect(Some(5), Some(10))).unwrap();
    let _c = ui.add(g, rect(Some(20), Some(4))).unwrap();
    let _d = ui.add(g, rect(Some(5), Some(4))).unwrap();
    let after = ui.add(col, rect(Some(7), Some(7))).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(g).unwrap(), RectI::new(0, 0, 28, 17), "内在 = (20+3+5)×(10+3+4)");
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 20, 10), "定值子 (20,10) 恰满首格（格 20×10）");
    let _ = after;
}

#[test]
fn grid_partial_fill_uses_used_tracks() {
    // 未满格：轨数按子实占——rows_needed = ceil(4/3) = 2（非声明 3 行）、
    // 列向内在取已占 3 列 → 高 22（非 34）；格游标照声明 3 列行主序
    let mut ui = Ui::<16>::new(100, 1000);
    let root = ui.root();
    let g = ui.add(root, grid(3, 3, 2, berry_ui::layout::CrossAlign::Stretch)).unwrap();
    for _ in 0..3 {
        stretchy(&mut ui, g, 10, 10);
    }
    let k3 = stretchy(&mut ui, g, 10, 10);
    ui.layout();
    assert_eq!(ui.bounds(g).unwrap(), RectI::new(0, 0, 100, 22), "高 = 10+2+10（实占 2 行）");
    let track_w = (100 - 2 * 2) / 3; // 32
    assert_eq!(ui.bounds(k3).unwrap(), RectI::new(0, 12, track_w, 10), "第 4 子 = 行 1 列 0：y = 10+gap2");
}

#[test]
fn image_icon_leaf_sizing_never_stretched() {
    // §4.3：w/h None → IconSource::dims 内在；Some → 定值；内在型叶
    // 恒不拉十字轴（Col Stretch 宽 100 下 Image 仍 4、Icon 仍 12）
    let mut ui = Ui::<16>::new(100, 200);
    ui.set_icon_source(&FIXED_SRC);
    let root = ui.root();
    let col = ui.add(root, container(ViewKind::Col, 0)).unwrap();
    let img = ui.add(col, image(9, None, None)).unwrap();
    let ic = ui.add(col, icon(7, None, None)).unwrap();
    let fx = ui.add(col, image(9, Some(20), Some(8))).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(img).unwrap(), RectI::new(0, 0, 4, 4), "res=9 dims 4×4（不拉 100）");
    assert_eq!(ui.bounds(ic).unwrap(), RectI::new(0, 4, 12, 6), "id=7 dims 12×6");
    assert_eq!(ui.bounds(fx).unwrap(), RectI::new(0, 10, 20, 8), "w/h Some 定值");
}

#[test]
fn icon_dims_unregistered_zero_bounds_and_advance() {
    // §11-14 布局半：默认空供件 dims 缺 → (0,0) + 游标推进 0 + 不炸
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui.add(root, container(ViewKind::Col, 5)).unwrap();
    let ghost = ui.add(col, icon(99, None, None)).unwrap();
    let t = ui.add(col, text(b"hi", FontSize::S8x16)).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(ghost).unwrap(), RectI::new(0, 0, 0, 0), "未注册 → (0,0)");
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 5, 16, 16), "游标推进 0+gap5 = 5");
}

#[test]
fn grid_add_and_set_props_reject_faces() {
    // add 面（D24/§4.2）：spec 零维 → NodeBad；格满再 add → BadParent
    let mut ui = Ui::<32>::new(100, 200);
    let root = ui.root();
    assert_eq!(
        ui.add(root, grid(0, 2, 0, berry_ui::layout::CrossAlign::Stretch)),
        Err(UiError::NodeBad),
        "cols=0 spec 拒"
    );
    assert_eq!(
        ui.add(root, grid(3, 0, 0, berry_ui::layout::CrossAlign::Stretch)),
        Err(UiError::NodeBad),
        "rows=0 spec 拒"
    );
    let g = ui.add(root, grid(1, 1, 0, berry_ui::layout::CrossAlign::Stretch)).unwrap();
    assert!(ui.add(g, rect(Some(1), Some(1))).is_ok(), "首子过");
    assert_eq!(
        ui.add(g, rect(Some(1), Some(1))),
        Err(UiError::BadParent),
        "1×1 格第 2 子拒（UiError 族冻结——BadParent 兼任格满面）"
    );

    // set_props 面：零维 / 缩格致不容现有子 → NodeBad（先验后写——状态保持）
    let mut ui = Ui::<32>::new(100, 200);
    let root = ui.root();
    let g = ui.add(root, grid(2, 2, 0, berry_ui::layout::CrossAlign::Stretch)).unwrap();
    for _ in 0..3 {
        ui.add(g, rect(Some(1), Some(1))).unwrap();
    }
    assert_eq!(
        ui.set_props(g, Props::Grid(GridProps {
            cols: 0,
            rows: 2,
            gap: 0,
            cross: berry_ui::layout::CrossAlign::Stretch,
            w: None,
            h: None,
        })),
        Err(UiError::NodeBad),
        "set_props cols=0 拒"
    );
    assert_eq!(
        ui.set_props(g, Props::Grid(GridProps {
            cols: 1,
            rows: 2,
            gap: 0,
            cross: berry_ui::layout::CrossAlign::Stretch,
            w: None,
            h: None,
        })),
        Err(UiError::NodeBad),
        "1×2 = 2 < 3 现有子——缩格拒"
    );
    assert!(ui.set_props(g, Props::Grid(GridProps {
        cols: 3,
        rows: 1,
        gap: 0,
        cross: berry_ui::layout::CrossAlign::Stretch,
        w: None,
        h: None,
    }))
    .is_ok(), "3×1 = 3 恰容合法");
}
