//! LAYOUT-L5：per-node incoming BoxConstraints + Text wrap + TextField
//! shrink-to-width。wrap 关闭时 L1..L4 全部旧语义必须零漂。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos};
use berry_gfx::font::FontSize;
use berry_ui::damage::RectI;
use berry_ui::desc::{
    ColProps, GridProps, PadProps, Props, RectProps, TextBuf, TextProps, TextStyle, ViewKind,
    ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::metrics::MonospaceTextMetrics;
use berry_ui::sdk::{BoxConstraints, TextField, TextFieldSpec, TextInputKind, Theme, Ui, UiError, WidgetContext};
use berry_ui::text_wrap::{measure as measure_plan, plan};

fn text_spec(bytes: &[u8], scale: FontSize) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: Color { r: 1, g: 2, b: 3 }, bg: None, scale },
            text: TextBuf::new(bytes).unwrap(),
        }),
    }
}

#[test]
fn wrap_plan_is_greedy_space_cjk_and_emergency_total() {
    let mono = MonospaceTextMetrics;
    // S8x16：ASCII/CJK 都是 8px 宽（v1 等宽蜂格）。
    let ascii = plan(b"AAA BBB", 0, 24, &mono);
    assert_eq!(ascii.len(), 2);
    assert_eq!(ascii.line(0, b"AAA BBB"), b"AAA");
    assert_eq!(ascii.line(1, b"AAA BBB"), b"BBB");
    assert_eq!(measure_plan(b"AAA BBB", 0, &ascii, &mono), (24, 32));

    // CJK 每个码点后是软断点；无空格也不断成逐字行。
    let cjk_bytes = "设排列法".as_bytes();
    let cjk = plan(cjk_bytes, 0, 8, &mono);
    assert_eq!(cjk.len(), 4);
    for line in 0..cjk.len() {
        assert_eq!(cjk.line(line, cjk_bytes).len(), 3);
    }
    assert_eq!(measure_plan(cjk_bytes, 0, &cjk, &mono), (8, 64));

    // 连续 ASCII 无空格：24px 可容 3 字，第 4 字触发紧急断行。
    let emergency = plan(b"ABCD", 0, 24, &mono);
    assert_eq!(emergency.line(0, b"ABCD"), b"ABC");
    assert_eq!(emergency.line(1, b"ABCD"), b"D");

    // max_w=0 不丢字：首码点独立成行。
    let zero = plan(b"AB", 0, 0, &mono);
    assert_eq!(zero.len(), 2);
    assert_eq!(zero.line(0, b"AB"), b"A");
    assert_eq!(zero.line(1, b"AB"), b"B");
    assert_eq!(measure_plan(b"AB", 0, &zero, &mono), (8, 32));
}

#[test]
fn wrap_uses_incoming_parent_box_and_sizes_up() {
    let mut ui = Ui::<32>::new(200, 200);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: Some(32),
                    h: None,
                    cross: CrossAlign::Stretch,
                }),
            },
        )
        .unwrap();
    let text = ui.add(col, text_spec(b"AAA BBB", FontSize::S8x16)).unwrap();

    ui.set_text_wrap(text, true).unwrap();
    ui.layout_box(BoxConstraints::loose(200, 200))
        .unwrap();
    assert_eq!(ui.bounds(text).unwrap(), RectI::new(0, 0, 24, 32));
    assert_eq!(ui.text_wrap_iter().next().unwrap().last_max_w, 32);

    // 宽度变化：显式 dirty 后同 slot/generation 重测，焦点/语义身份不变。
    let before = text;
    let col_props = Props::Col(ColProps {
        gap: 0,
        w: Some(64),
        h: None,
        cross: CrossAlign::Stretch,
    });
    ui.set_props(col, col_props).unwrap();
    ui.layout_box(BoxConstraints::loose(200, 200))
        .unwrap();
    assert_eq!(before, text);
    assert_eq!(ui.bounds(text).unwrap(), RectI::new(0, 0, 56, 16));
    assert_eq!(ui.text_wrap_iter().next().unwrap().last_max_w, 64);
}

#[test]
fn wrap_paint_uses_the_same_line_plan() {
    let mut ui = Ui::<32>::new(100, 100);
    let root = ui.root();
    let text = ui.add(root, text_spec(b"AAA BBB", FontSize::S8x16)).unwrap();
    ui.set_text_wrap(text, true).unwrap();
    ui.layout_box(BoxConstraints::loose(24, 100))
        .unwrap();
    assert_eq!(ui.bounds(text).unwrap(), RectI::new(0, 0, 24, 32));

    ui.drain_damage();
    ui.set_viewport(100, 100);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 8];
    let mut encoder = CommandEncoder::new();
    ui.paint(&mut scratch, &mut encoder);
    let display = encoder.finish();
    let mut seen = None;
    for command in display.iter() {
        if let Cmd::DrawGlyphRun { run } = command {
            seen = Some(run.glyphs);
        }
    }
    let glyphs = seen.unwrap();
    let xs: Vec<_> = glyphs.iter().map(|g| g.x).collect();
    let ys: Vec<_> = glyphs.iter().map(|g| g.y).collect();
    assert_eq!(xs, vec![0, 8, 16, 0, 8, 16]);
    assert_eq!(ys, vec![0, 0, 0, 16, 16, 16]);
}

#[test]
fn wrap_registration_lifecycle_and_reject_faces_are_loud() {
    let mut ui = Ui::<32>::new(100, 100);
    let root = ui.root();
    let text = ui.add(root, text_spec(b"ABC", FontSize::S8x16)).unwrap();
    let rect = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(berry_ui::desc::RectProps {
                    bg: Color { r: 1, g: 2, b: 3 },
                    w: Some(10),
                    h: Some(10),radius: 0}),
            },
        )
        .unwrap();

    assert_eq!(ui.set_text_wrap(rect, true), Err(UiError::NodeBad));
    ui.set_text_wrap(text, true).unwrap();
    ui.set_text_wrap(text, true).unwrap(); // 幂等 no-op
    assert_eq!(ui.text_wrap_iter().count(), 1);
    ui.layout();
    assert!(ui.take_text_wrap(text));
    assert!(!ui.take_text_wrap(text));
    assert_eq!(ui.text_wrap_iter().count(), 0);

    // remove 子树清理，不悬挂 NodeId。
    ui.set_text_wrap(text, true).unwrap();
    ui.remove(text).unwrap();
    assert_eq!(ui.text_wrap_iter().count(), 0);
}

#[test]
fn textfield_shrink_has_floor_never_grows_and_uses_existing_damage_path() {
    let mut ui = Ui::<64>::new(400, 200);
    let root = ui.root();
    let ctx = WidgetContext::for_viewport(Theme::light(), 400, 200);
    let mut field = TextField::new(
        &mut ui,
        root,
        &ctx,
        TextFieldSpec { kind: TextInputKind::Text, width: 200 },
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(field.panel).unwrap().w, 200);

    // avail 小于 floor：停在可输入下限并返回实际宽。
    let floor = field.shrink_to_width(&mut ui, &ctx, 1).unwrap();
    assert!(floor < 200);
    ui.layout();
    assert_eq!(ui.bounds(field.panel).unwrap().w, floor as i32);

    // 可恢复到构造宽度以内的中间档，但永不超出 spec.width。
    assert_eq!(field.shrink_to_width(&mut ui, &ctx, 500).unwrap(), 200);
    ui.layout();
    assert_eq!(ui.bounds(field.panel).unwrap().w, 200);

    // 正常中间档走 PanelProps.w，AST 仍是既有 Props 域。
    let target = floor + 20;
    assert_eq!(field.shrink_to_width(&mut ui, &ctx, target).unwrap(), target);
    ui.layout();
    assert_eq!(ui.bounds(field.panel).unwrap().w, target as i32);
}

#[test]
fn wrap_off_tree_keeps_l4_default_measurement_semantics() {
    let mut ui = Ui::<32>::new(200, 200);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: Some(32),
                    h: None,
                    cross: CrossAlign::Stretch,
                }),
            },
        )
        .unwrap();
    let text = ui.add(col, text_spec(b"AAA BBB", FontSize::S8x16)).unwrap();
    ui.layout_box(BoxConstraints::loose(200, 200))
        .unwrap();
    // wrap 未登记：Text 不吃 incoming 32，仍单行 56×16。
    assert_eq!(ui.bounds(text).unwrap(), RectI::new(0, 0, 56, 16));
}

struct CountingBoxMetrics;
static BOX_MEASURES: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

impl berry_ui::metrics::TextMetrics for CountingBoxMetrics {
    fn measure(&self, _text: &[u8], _scale: u8) -> (u32, u32) {
        (0, 0)
    }

    fn measure_with_box(&self, _text: &[u8], _scale: u8, _max_w: u32) -> (u32, u32) {
        BOX_MEASURES.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        (16, 16)
    }
}

#[test]
fn wrapped_text_measures_once_per_explicit_layout_pass() {
    let mut ui = Ui::<32>::new(100, 100);
    let root = ui.root();
    let text = ui.add(root, text_spec(b"AAA BBB", FontSize::S8x16)).unwrap();
    ui.set_text_metrics(&CountingBoxMetrics);
    ui.set_text_wrap(text, true).unwrap();
    BOX_MEASURES.store(0, core::sync::atomic::Ordering::SeqCst);
    let stat = ui
        .layout_box(BoxConstraints::tight(24, 100))
        .unwrap();
    assert!(stat.measured > 0);
    assert_eq!(BOX_MEASURES.load(core::sync::atomic::Ordering::SeqCst), 1);

    ui.set_viewport(100, 100);
    ui.layout_box(BoxConstraints::tight(24, 100)).unwrap();
    assert_eq!(BOX_MEASURES.load(core::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn parent_geometry_change_remeasures_wrapped_descendant_without_full_dirty() {
    let mut ui = Ui::<32>::new(200, 200);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: Some(32),
                    h: None,
                    cross: CrossAlign::Stretch,
                }),
            },
        )
        .unwrap();
    let text = ui.add(col, text_spec(b"AAA BBB", FontSize::S8x16)).unwrap();
    ui.set_text_wrap(text, true).unwrap();
    ui.layout_box(BoxConstraints::loose(200, 200)).unwrap();
    assert_eq!(ui.bounds(text).unwrap(), RectI::new(0, 0, 24, 32));

    // 只改父容器几何，不做 set_viewport 全树 dirty。wrap 叶子仍必须用
    // 新 incoming max_w 重测，否则 last_max_w / bounds 保持旧 32。
    ui.set_props(
        col,
        Props::Col(ColProps {
            gap: 0,
            w: Some(64),
            h: None,
            cross: CrossAlign::Stretch,
        }),
    )
    .unwrap();
    ui.layout_box(BoxConstraints::loose(200, 200)).unwrap();
    assert_eq!(ui.bounds(text).unwrap(), RectI::new(0, 0, 56, 16));
    assert_eq!(ui.text_wrap_iter().next().unwrap().last_max_w, 64);
}


#[test]
fn pad_projection_subtracts_both_sides_and_matches_place_port() {
    let mut ui = Ui::<32>::new(200, 200);
    let root = ui.root();
    let pad = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Pad,
                key: 0,
                props: Props::Pad(PadProps { all: 88 }),
            },
        )
        .unwrap();
    let text = ui.add(pad, text_spec(b"AAA BBB", FontSize::S8x16)).unwrap();
    ui.set_text_wrap(text, true).unwrap();
    ui.layout();
    // 200 - 88*2 = 24：wrap 只看两侧后的真实 port，而不是只扣一侧的 112。
    assert_eq!(ui.bounds(text).unwrap(), RectI::new(88, 88, 24, 32));
    assert_eq!(ui.text_wrap_iter().next().unwrap().last_max_w, 24);
}

#[test]
fn grid_projection_uses_one_track_not_full_grid_content_width() {
    let mut ui = Ui::<64>::new(200, 200);
    let root = ui.root();
    let grid = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Grid,
                key: 0,
                props: Props::Grid(GridProps {
                    cols: 2,
                    rows: 1,
                    gap: 4,
                    w: Some(100),
                    h: Some(64),
                    cross: CrossAlign::Start,
                }),
            },
        )
        .unwrap();
    let text = ui.add(grid, text_spec(b"AAA BBB", FontSize::S8x16)).unwrap();
    let other = ui
        .add(
            grid,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: Color { r: 1, g: 2, b: 3 },
                    w: Some(48),
                    h: Some(16),radius: 0}),
            },
        )
        .unwrap();
    ui.set_text_wrap(text, true).unwrap();
    ui.layout();
    // track = (100 - 4) / 2 = 48；wrap Text 不得拿全格 96 后跨列溢出。
    assert_eq!(ui.bounds(text).unwrap(), RectI::new(0, 0, 24, 32));
    assert_eq!(ui.bounds(other).unwrap(), RectI::new(52, 0, 48, 16));
    assert_eq!(ui.text_wrap_iter().next().unwrap().last_max_w, 48);
}

#[test]
fn wrap_plan_treats_lf_as_explicit_line_boundary() {
    let mono = MonospaceTextMetrics;
    let hard = plan(b"AA\nBB", 0, u32::MAX, &mono);
    assert_eq!(hard.len(), 2);
    assert_eq!(hard.line(0, b"AA\nBB"), b"AA");
    assert_eq!(hard.line(1, b"AA\nBB"), b"BB");
    assert_eq!(measure_plan(b"AA\nBB", 0, &hard, &mono), (16, 32));

    let mixed = plan(b"AAAA\nB", 0, 16, &mono);
    assert_eq!(mixed.len(), 3);
    assert_eq!(mixed.line(0, b"AAAA\nB"), b"AA");
    assert_eq!(mixed.line(1, b"AAAA\nB"), b"AA");
    assert_eq!(mixed.line(2, b"AAAA\nB"), b"B");

    let empties = plan(b"A\n\nB", 0, u32::MAX, &mono);
    assert_eq!(empties.len(), 3);
    assert_eq!(empties.line(1, b"A\n\nB"), b"");
    assert_eq!(measure_plan(b"A\n\nB", 0, &empties, &mono), (8, 48));

    let trailing = plan(b"ABC\n", 0, u32::MAX, &mono);
    assert_eq!(trailing.len(), 1);
    assert_eq!(trailing.line(0, b"ABC\n"), b"ABC");
}

#[test]
fn wrap_plan_hard_newline_preserves_indent_and_capacity() {
    let mono = MonospaceTextMetrics;
    let indented = plan(b"AAA \n B", 0, u32::MAX, &mono);
    assert_eq!(indented.line(0, b"AAA \n B"), b"AAA");
    assert_eq!(indented.line(1, b"AAA \n B"), b" B");
    assert_eq!(measure_plan(b"AAA \n B", 0, &indented, &mono), (24, 32));

    let mut worst = [0u8; 64];
    worst.fill(b'\n');
    let worst_plan = plan(&worst, 0, u32::MAX, &mono);
    assert_eq!(worst_plan.len(), 64);
    assert_eq!(measure_plan(&worst, 0, &worst_plan, &mono), (0, 1024));
}

#[test]
fn wrapped_text_paint_skips_lf_glyph_and_advances_y() {
    let mut ui = Ui::<16>::new(64, 64);
    let root = ui.root();
    let node = ui.add(root, text_spec(b"AB\nCD", FontSize::S8x16)).unwrap();
    ui.set_text_wrap(node, true).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(node).unwrap(), RectI::new(0, 0, 16, 32));

    let mut glyphs = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut encoder = CommandEncoder::new();
    ui.paint(&mut glyphs, &mut encoder);
    let list = encoder.finish();
    let mut seen = Vec::new();
    for command in list.iter() {
        if let Cmd::DrawGlyphRun { run, .. } = command {
            seen.extend(run.glyphs.iter().map(|glyph| (glyph.ch, glyph.x, glyph.y)));
        }
    }
    assert_eq!(
        seen,
        vec![
            (u32::from(b'A'), 0, 0),
            (u32::from(b'B'), 8, 0),
            (u32::from(b'C'), 0, 16),
            (u32::from(b'D'), 8, 16),
        ]
    );
    assert!(!seen.iter().any(|(ch, _, _)| *ch == u32::from(b'\n')));
}
