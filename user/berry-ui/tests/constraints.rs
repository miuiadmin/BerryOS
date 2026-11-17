//! GUI-14 L1：根播种 Tight/Loose/Infinite 约束 + 显式本地 AdaptiveSlot
//! 判定。值级钉板：Viewport 零漂移、Tight 等价改 viewport、Loose
//! shrink-wrap/clamp、Infinite 子包络、净树 no-op、damage 恒裁真实
//! viewport、Clip 局部性。

use berry_gfx::cmd::Color;
use berry_ui::desc::{ClipProps, Props, RectProps, ViewKind, ViewSpec};
use berry_ui::layout::{Constraints, LayoutStat};
use berry_ui::sdk::{SlotVariant, slot_variant};
use berry_ui::damage::RectI;
use berry_ui::{NodeId, Ui};

fn rect(w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: Color { r: 1, g: 2, b: 3 },
            w,
            h,
            radius: 0,
        }),
    }
}

fn add_rect(ui: &mut Ui<16>, parent: NodeId, w: Option<u32>, h: Option<u32>) -> NodeId {
    ui.add(parent, rect(w, h)).unwrap()
}

#[test]
fn viewport_constraint_is_byte_equivalent_to_layout() {
    let mut a = Ui::<16>::new(100, 200);
    let mut b = Ui::<16>::new(100, 200);
    for ui in [&mut a, &mut b] {
        let root = ui.root();
        add_rect(ui, root, Some(20), Some(10));
        add_rect(ui, root, Some(40), Some(16));
    }
    let ra = a.root();
    let rb = b.root();
    let sa = a.layout();
    let sb = b.layout_with(Constraints::Viewport);
    assert_eq!(sa, sb);
    assert_eq!(a.bounds(ra).unwrap(), b.bounds(rb).unwrap());
    assert_eq!(a.damage(), b.damage());
}

#[test]
fn tight_constraint_is_equivalent_to_resized_viewport() {
    let mut a = Ui::<16>::new(100, 200);
    let ra = a.root();
    let ca = add_rect(&mut a, ra, None, None);
    let sa = a.layout_with(Constraints::Tight { w: 80, h: 60 });

    let mut b = Ui::<16>::new(80, 60);
    let rb = b.root();
    let cb = add_rect(&mut b, rb, None, None);
    let sb = b.layout();

    assert_eq!(sa, sb);
    assert_eq!(a.bounds(ra).unwrap(), b.bounds(rb).unwrap());
    assert_eq!(a.bounds(ca).unwrap(), b.bounds(cb).unwrap());
    assert_eq!(a.bounds(ra).unwrap(), RectI::new(0, 0, 80, 60));
}

#[test]
fn tight_zero_and_oversize_saturate_without_panic() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let text_like = add_rect(&mut ui, root, Some(20), Some(10));
    ui.layout_with(Constraints::Tight { w: 0, h: 0 });
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 0, 0));
    assert_eq!(ui.bounds(text_like).unwrap(), RectI::new(0, 0, 20, 10));

    let mut big = Ui::<16>::new(10, 10);
    let br = big.root();
    add_rect(&mut big, br, Some(5), Some(5));
    big.layout_with(Constraints::Tight {
        w: u32::MAX,
        h: u32::MAX,
    });
    assert_eq!(big.bounds(br).unwrap().w, i32::MAX as i32);
    for r in big.damage().rects[..big.damage().count as usize].iter() {
        assert!(r.x as u64 + r.w as u64 <= 10 || r.w == 0);
        assert!(r.y as u64 + r.h as u64 <= 10 || r.h == 0);
    }
}

#[test]
fn loose_shrink_wraps_and_clamps_child_extent() {
    let mut ui = Ui::<16>::new(200, 200);
    let root = ui.root();
    add_rect(&mut ui, root, Some(40), Some(16));
    add_rect(&mut ui, root, Some(22), Some(10));
    ui.layout_with(Constraints::Loose { w: 100, h: 100 });
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 40, 16));

    let mut clamped = Ui::<16>::new(200, 200);
    let cr = clamped.root();
    let big = add_rect(&mut clamped, cr, Some(200), Some(150));
    clamped.layout_with(Constraints::Loose { w: 100, h: 100 });
    assert_eq!(clamped.bounds(cr).unwrap(), RectI::new(0, 0, 100, 100));
    assert_eq!(clamped.bounds(big).unwrap(), RectI::new(0, 0, 200, 150));
}

#[test]
fn loose_stretchy_child_collapse_wart_is_pinned() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    add_rect(&mut ui, root, None, None);
    ui.layout_with(Constraints::Loose { w: 80, h: 60 });
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 0, 0));
}

#[test]
fn infinite_uses_unbounded_child_extent_and_damage_stays_viewport_clipped() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let big = add_rect(&mut ui, root, Some(300), Some(200));
    ui.layout_with(Constraints::Infinite);
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 300, 200));
    assert_eq!(ui.bounds(big).unwrap(), RectI::new(0, 0, 300, 200));
    // hit 的根域是 Root 摆放界（此处 300×200），不是 paint/damage 的
    // 真实 viewport（100×100）——两条裁剪域在 L1 显式分野。
    assert_eq!(ui.node_at(250, 150), Some(big));
    assert!(!ui.damage().full);
    for r in ui.damage().rects[..ui.damage().count as usize].iter() {
        assert!(r.x as u64 + r.w as u64 <= 100 || r.w == 0);
        assert!(r.y as u64 + r.h as u64 <= 100 || r.h == 0);
    }
}

#[test]
fn clean_tree_constraint_call_is_documented_noop() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    add_rect(&mut ui, root, Some(20), Some(10));
    ui.layout();
    let before = ui.bounds(root).unwrap();
    ui.drain_damage();
    let stat = ui.layout_with(Constraints::Tight { w: 50, h: 40 });
    assert_eq!(stat, LayoutStat { measured: 0, placed: 0 });
    assert_eq!(ui.bounds(root).unwrap(), before);
    assert!(ui.damage().is_empty());
}

#[test]
fn tight_box_does_not_clip_and_clip_node_keeps_locality() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let clip = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Clip,
                key: 0,
                props: Props::Clip(ClipProps { w: Some(80), h: Some(60) }),
            },
        )
        .unwrap();
    let child = add_rect(&mut ui, clip, None, None);
    ui.layout_with(Constraints::Tight { w: 80, h: 60 });
    assert_eq!(ui.bounds(clip).unwrap(), RectI::new(0, 0, 80, 60));
    assert_eq!(ui.bounds(child).unwrap(), RectI::new(0, 0, 80, 60));
    assert_eq!(ui.node_at(70, 50), Some(child));
    assert_eq!(ui.node_at(90, 50), None);
}

#[test]
fn constraint_change_requires_explicit_dirty_and_full_union_damage() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    add_rect(&mut ui, root, Some(20), Some(10));
    ui.layout();
    ui.drain_damage();

    // 净树约束切换是 no-op；显式 set_viewport(same) 是全量 dirty/full
    // damage 语境， Tight 新旧包络由既有 FULL 面 cover。
    assert_eq!(
        ui.layout_with(Constraints::Tight { w: 50, h: 40 }),
        LayoutStat { measured: 0, placed: 0 }
    );
    ui.set_viewport(100, 200);
    let stat = ui.layout_with(Constraints::Tight { w: 50, h: 40 });
    assert_eq!(stat.measured, 2);
    assert_eq!(stat.placed, 2);
    assert!(ui.damage().full);
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 50, 40));

    ui.set_viewport(100, 200);
    ui.layout_with(Constraints::Viewport);
    assert_eq!(ui.bounds(root).unwrap(), RectI::new(0, 0, 100, 200));
}

#[test]
fn adaptive_slot_threshold_is_explicit_and_half_open() {
    assert_eq!(slot_variant(599, 600), SlotVariant::Compact);
    assert_eq!(slot_variant(600, 600), SlotVariant::Expanded);
    assert_eq!(slot_variant(0, 0), SlotVariant::Expanded);
    assert_eq!(slot_variant(u32::MAX, u32::MAX), SlotVariant::Expanded);
}
