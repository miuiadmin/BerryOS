//! LAYOUT-L4：四元组 min/max 约束代数、Root intrinsic clamp、L1 兼容
//! 糖与 L2/L3 slot 的纯函数表达。既有默认布局必须零漂。

use berry_gfx::cmd::Color;
use berry_ui::desc::{Props, RectProps, ViewKind, ViewSpec};
use berry_ui::layout::{BoxConstraints, Constraints, LayoutStat};
use berry_ui::damage::RectI;
use berry_ui::metrics::{MonospaceTextMetrics, TextMetrics};
use berry_ui::{NodeId, Ui};
use core::sync::atomic::{AtomicUsize, Ordering};

fn rect(w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: Color { r: 1, g: 2, b: 3 }, w, h }),
    }
}

fn add_rect(ui: &mut Ui<16>, parent: NodeId, w: Option<u32>, h: Option<u32>) -> NodeId {
    ui.add(parent, rect(w, h)).unwrap()
}

#[test]
fn box_constraint_algebra_is_loud_and_total() {
    assert!(BoxConstraints::new(80, 60, 20, 40).is_err());
    assert!(BoxConstraints::new(0, 100, 40, 20).is_err());
    let bc = BoxConstraints::new(20, 100, 10, 80).unwrap();
    assert_eq!(bc.min(), (20, 10));
    assert_eq!(bc.max(), (100, 80));
    assert_eq!(bc.fit(10, 200), (20, 80));
    assert_eq!(bc.fit(50, 40), (50, 40));

    let child = bc.deflate(10, 20);
    assert_eq!(child.min(), (10, 0));
    assert_eq!(child.max(), (90, 60));

    let other = BoxConstraints::new(30, 120, 0, 50).unwrap();
    assert_eq!(
        bc.intersect(other),
        Some(BoxConstraints::new(30, 100, 10, 50).unwrap())
    );
    assert!(bc
        .intersect(BoxConstraints::new(200, 300, 0, 300).unwrap())
        .is_none());
}

#[test]
fn constructor_sugar_and_infinite_domain_are_pinned() {
    let tight = BoxConstraints::tight(80, 60);
    assert_eq!(tight.min(), (80, 60));
    assert_eq!(tight.max(), (80, 60));
    assert!(tight.is_tight());

    let loose = BoxConstraints::loose(100, 80);
    assert_eq!(loose.min(), (0, 0));
    assert_eq!(loose.max(), (100, 80));
    assert!(!loose.is_tight());

    let infinite = BoxConstraints::infinite();
    assert_eq!(infinite.max(), (u32::MAX, u32::MAX));
    assert!(infinite.is_infinite());
    assert_eq!(infinite.fit(u32::MAX, u32::MAX), (u32::MAX, u32::MAX));
}

#[test]
fn root_intrinsic_clamps_to_min_and_max_independently() {
    let mut ui = Ui::<16>::new(300, 300);
    let root = ui.root();
    let stretch = add_rect(&mut ui, root, None, None);
    let bc = BoxConstraints::new(80, 200, 60, 180).unwrap();
    ui.layout_box(bc).unwrap();
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 80, 60));
    assert_eq!(ui.bounds(stretch).unwrap(), RectI::new(0, 0, 80, 60));

    let mut ui = Ui::<16>::new(300, 300);
    let root = ui.root();
    let big = add_rect(&mut ui, root, Some(400), Some(100));
    let bc = BoxConstraints::new(20, 240, 30, 80).unwrap();
    ui.layout_box(bc).unwrap();
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 240, 80));
    assert_eq!(ui.bounds(big).unwrap(), RectI::new(0, 0, 400, 100));
}

#[test]
fn l4_tight_is_l1_tight_and_default_viewport_is_zero_drift() {
    let mut a = Ui::<16>::new(100, 200);
    let ra = a.root();
    let ca = add_rect(&mut a, ra, None, None);

    let mut b = Ui::<16>::new(100, 200);
    let rb = b.root();
    let cb = add_rect(&mut b, rb, None, None);

    let stat_a = a.layout_box(BoxConstraints::tight(80, 60)).unwrap();
    let stat_b = b.layout_with(Constraints::Tight { w: 80, h: 60 });
    assert_eq!(stat_a, stat_b);
    assert_eq!(a.bounds(ra).unwrap(), b.bounds(rb).unwrap());
    assert_eq!(a.bounds(ca).unwrap(), b.bounds(cb).unwrap());
    assert_eq!(a.damage(), b.damage());

    let mut c = Ui::<16>::new(100, 200);
    let rc = c.root();
    let cc = add_rect(&mut c, rc, None, None);
    c.layout();

    let mut d = Ui::<16>::new(100, 200);
    let rd = d.root();
    let cd = add_rect(&mut d, rd, None, None);
    d.layout_box(BoxConstraints::tight(100, 200)).unwrap();
    assert_eq!(c.bounds(rc).unwrap(), d.bounds(rd).unwrap());
    assert_eq!(c.bounds(cc).unwrap(), d.bounds(cd).unwrap());
    assert_eq!(c.damage(), d.damage());
}

#[test]
fn l2_flex_and_l3_pane_slots_express_as_constraint_pure_functions() {
    let parent = BoxConstraints::loose(1056, 120);
    let flexible = parent.deflate(12, 0);
    let first = BoxConstraints::loose(348, 120);
    let second = BoxConstraints::loose(696, 120);
    assert_eq!(first.max().0 + second.max().0, flexible.max().0);
    assert_eq!(
        parent.intersect(second),
        Some(BoxConstraints::loose(696, 120))
    );

    let pane = BoxConstraints::new(100, 300, 40, 200).unwrap().deflate(24, 40);
    assert_eq!(pane.min(), (76, 0));
    assert_eq!(pane.max(), (276, 160));
}

#[test]
fn clean_tree_l4_call_and_measurement_cache_boundary_are_noop() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    add_rect(&mut ui, root, Some(30), Some(20));
    let first = ui.layout_box(BoxConstraints::loose(100, 100)).unwrap();
    assert_eq!(first.measured, 2);
    assert_eq!(first.placed, 2);
    ui.drain_damage();

    let clean = ui.layout_box(BoxConstraints::tight(40, 30)).unwrap();
    assert_eq!(clean, LayoutStat { measured: 0, placed: 0 });
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 30, 20));
    assert!(ui.damage().is_empty());

    ui.set_viewport(100, 200);
    let dirty = ui.layout_box(BoxConstraints::tight(40, 30)).unwrap();
    assert_eq!(dirty.measured, 2);
    assert_eq!(dirty.placed, 2);
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 40, 30));
}

struct CountingMetrics;

impl TextMetrics for CountingMetrics {
    fn measure(&self, text: &[u8], scale: u8) -> (u32, u32) {
        COUNTED_MEASURES.fetch_add(1, Ordering::SeqCst);
        MonospaceTextMetrics.measure(text, scale)
    }
}

static COUNTED_MEASURES: AtomicUsize = AtomicUsize::new(0);

#[test]
fn l4_keeps_single_text_measure_per_explicit_layout_pass() {
    let mut ui = Ui::<32>::new(200, 200);
    let root = ui.root();
    COUNTED_MEASURES.store(0, Ordering::SeqCst);
    ui.set_text_metrics(&CountingMetrics);
    for text in [b"alpha".as_slice(), b"beta".as_slice(), b"gamma".as_slice()] {
        let _ = ui
            .add(
                root,
                ViewSpec {
                    kind: ViewKind::Text,
                    key: 0,
                    props: Props::Text(berry_ui::desc::TextProps {
                        style: berry_ui::desc::TextStyle {
                            fg: Color { r: 0, g: 0, b: 0 },
                            bg: None,
                            scale: berry_gfx::font::FontSize::S16x32,
                        },
                        text: berry_ui::TextBuf::new(text).unwrap(),
                    }),
                },
            )
            .unwrap();
    }

    ui.layout_box(BoxConstraints::tight(200, 200)).unwrap();
    assert_eq!(COUNTED_MEASURES.load(Ordering::SeqCst), 3);

    ui.set_viewport(200, 200);
    ui.layout_box(BoxConstraints::loose(100, 100)).unwrap();
    assert_eq!(COUNTED_MEASURES.load(Ordering::SeqCst), 6);

    ui.layout_box(BoxConstraints::tight(100, 100)).unwrap();
    assert_eq!(COUNTED_MEASURES.load(Ordering::SeqCst), 6);
}
