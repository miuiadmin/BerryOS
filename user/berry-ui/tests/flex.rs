//! GUI-14 L2：固定主轴 Row/Col 的 flex ratio 分配值级钉板。
//!
//! 覆盖：1:2 剩余空间、gap、固定子混合、Col 镜像、零余量、登记序无
//! 关、权重变更 dirty/damage、eligible 拒面、容量与 remove 生命周期。

use berry_gfx::cmd::Color;
use berry_ui::desc::{CircleProps, Props, RectProps, RowProps, ViewKind, ViewSpec};
use berry_ui::damage::RectI;
use berry_ui::layout::CrossAlign;
use berry_ui::{NodeId, Ui, UiError};

fn rect() -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: Color { r: 10, g: 20, b: 30 },
            w: None,
            h: None,
        }),
    }
}

fn fixed_rect(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: Color { r: 1, g: 2, b: 3 },
            w: Some(w),
            h: Some(h),
        }),
    }
}

fn row(w: u32, h: u32, gap: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Row,
        key: 0,
        props: Props::Row(RowProps {
            gap,
            w: Some(w),
            h: Some(h),
            cross: CrossAlign::Stretch,
        }),
    }
}

fn add(ui: &mut Ui<64>, parent: NodeId, spec: ViewSpec) -> NodeId {
    ui.add(parent, spec).unwrap()
}

#[test]
fn row_distributes_remaining_space_by_weights() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    // 登记序故意逆序：分配只看兄弟链序。
    ui.set_flex(second, 2).unwrap();
    ui.set_flex(first, 1).unwrap();
    ui.layout();

    assert_eq!(ui.bounds(container).unwrap(), RectI::new(0, 0, 600, 120));
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 200, 120));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(200, 0, 400, 120));
}

#[test]
fn ratio_gap_and_fixed_child_are_exact() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 12));
    let fixed = add(&mut ui, container, fixed_rect(100, 120));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.set_flex(first, 1).unwrap();
    ui.set_flex(second, 2).unwrap();
    ui.layout();

    assert_eq!(ui.bounds(fixed).unwrap(), RectI::new(0, 0, 100, 120));
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(112, 0, 158, 120));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(282, 0, 318, 120));
    // 100 + 12 + 158 + 12 + 318 = 600。
}

#[test]
fn column_main_axis_mirrors_row() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(
        &mut ui,
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(berry_ui::desc::ColProps {
                gap: 0,
                w: Some(300),
                h: Some(300),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.set_flex(first, 1).unwrap();
    ui.set_flex(second, 2).unwrap();
    ui.layout();

    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 300, 100));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(0, 100, 300, 200));
}

#[test]
fn zero_and_undefined_remaining_space_collapse_safely() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let zero = add(&mut ui, root, row(0, 120, 12));
    let a = add(&mut ui, zero, rect());
    let b = add(&mut ui, zero, rect());
    ui.set_flex(a, 1).unwrap();
    ui.set_flex(b, 2).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 0, 120));
    assert_eq!(ui.bounds(b).unwrap(), RectI::new(12, 0, 0, 120));

    // 主轴未固定的 Row：flex 子内在 0，无剩余空间可分，不 panic。
    let mut intrinsic = Ui::<64>::new(800, 600);
    let root = intrinsic.root();
    let container = add(
        &mut intrinsic,
        root,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 4,
                w: None,
                h: Some(100),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    let child = add(&mut intrinsic, container, rect());
    intrinsic.set_flex(child, 1).unwrap();
    intrinsic.layout();
    assert_eq!(intrinsic.bounds(child).unwrap(), RectI::new(0, 0, 0, 100));
}

#[test]
fn weight_change_dirties_sibling_allocation_and_damage() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.layout();
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 0, 120));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(0, 0, 0, 120));
    ui.drain_damage();

    ui.set_flex(first, 1).unwrap();
    ui.set_flex(second, 2).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 200, 120));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(200, 0, 400, 120));
    assert!(!ui.damage().is_empty());
    ui.drain_damage();

    ui.set_flex(first, 3).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 360, 120));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(360, 0, 240, 120));
    assert!(!ui.damage().is_empty());
}

#[test]
fn flex_registration_rejects_ineligible_nodes_and_props() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let fixed = add(&mut ui, container, fixed_rect(10, 10));
    assert_eq!(ui.set_flex(fixed, 1), Err(UiError::NodeBad));
    let text = ui
        .add(
            container,
            ViewSpec {
                kind: ViewKind::Text,
                key: 0,
                props: Props::Text(berry_ui::desc::TextProps {
                    style: berry_ui::desc::TextStyle {
                        fg: Color { r: 1, g: 2, b: 3 },
                        bg: None,
                        scale: berry_gfx::font::FontSize::S8x16,
                    },
                    text: berry_ui::TextBuf::new(b"x").unwrap(),
                }),
            },
        )
        .unwrap();
    assert_eq!(ui.set_flex(text, 1), Err(UiError::NodeBad));

    let flexible = add(&mut ui, container, rect());
    assert_eq!(ui.set_flex(flexible, 0), Err(UiError::NodeBad));
    ui.set_flex(flexible, 1).unwrap();
    // props 离开双 None eligible 形态时连带摘除。
    ui.set_props(flexible, Props::Rect(RectProps {
        bg: Color { r: 1, g: 2, b: 3 },
        w: Some(10),
        h: Some(10),
    }))
    .unwrap();
    assert_eq!(ui.flex_iter().count(), 0);
    assert!(ui.set_flex(flexible, 1).is_err());

    // 父非 Row/Col：拒绝登记，不留下静默死权重。
    let orphan = add(&mut ui, root, rect());
    assert_eq!(ui.set_flex(orphan, 1), Err(UiError::NodeBad));
}

#[test]
fn flex_table_capacity_and_remove_lifecycle_are_loud() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let mut nodes = Vec::with_capacity(32);
    for _ in 0..32 {
        let node = add(&mut ui, container, rect());
        ui.set_flex(node, 1).unwrap();
        nodes.push(node);
    }
    let extra = add(&mut ui, container, rect());
    assert_eq!(ui.set_flex(extra, 1), Err(UiError::NodeFull));
    assert_eq!(ui.flex_iter().count(), 32);

    assert!(ui.take_flex(nodes[0]));
    assert_eq!(ui.set_flex(extra, 1), Ok(()));
    ui.remove(container).unwrap();
    assert_eq!(ui.flex_iter().count(), 0);
    assert!(ui.set_flex(extra, 1).is_err());
}

#[test]
fn flex_weights_are_runtime_only_and_do_not_drift_ast() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.layout();
    let mut before = [0u8; 512];
    let before_len = ui.encode_desc(&mut before).unwrap();

    ui.set_flex(first, 1).unwrap();
    ui.set_flex(second, 2).unwrap();
    let mut after = [0u8; 512];
    let after_len = ui.encode_desc(&mut after).unwrap();
    assert_eq!(before_len, after_len);
    assert_eq!(before[..before_len], after[..after_len]);
}

#[test]
fn same_weight_registration_is_zero_op() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let child = add(&mut ui, container, rect());
    ui.set_flex(child, 2).unwrap();
    ui.layout();
    ui.drain_damage();
    ui.set_flex(child, 2).unwrap();
    assert!(ui.damage().is_empty());
    let stat = ui.layout();
    assert_eq!(stat.measured, 0);
    assert_eq!(stat.placed, 0);
}

#[test]
fn flex_overflow_and_hit_are_safe() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(50, 120, 4));
    let fixed = add(&mut ui, container, fixed_rect(60, 120));
    let flexible = add(&mut ui, container, rect());
    ui.set_flex(flexible, 1).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(fixed).unwrap(), RectI::new(0, 0, 60, 120));
    assert_eq!(ui.bounds(flexible).unwrap(), RectI::new(64, 0, 0, 120));
    assert_eq!(ui.node_at(30, 60), Some(fixed));
    assert_eq!(ui.node_at(64, 60), None);

    let mut split = Ui::<64>::new(800, 600);
    let root = split.root();
    let container = add(&mut split, root, row(600, 120, 0));
    let first = add(&mut split, container, rect());
    let second = add(&mut split, container, rect());
    split.set_flex(first, 1).unwrap();
    split.set_flex(second, 2).unwrap();
    split.layout();
    assert_eq!(split.node_at(199, 60), Some(first));
    assert_eq!(split.node_at(200, 60), Some(second));
    assert_eq!(split.node_at(201, 60), Some(second));
}

#[test]
fn equal_triple_split_gives_remainder_to_last() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(100, 30, 0));
    let mut children = Vec::with_capacity(3);
    for _ in 0..3 {
        let child = add(&mut ui, container, rect());
        ui.set_flex(child, 1).unwrap();
        children.push(child);
    }
    ui.layout();
    assert_eq!(ui.bounds(children[0]).unwrap(), RectI::new(0, 0, 33, 30));
    assert_eq!(ui.bounds(children[1]).unwrap(), RectI::new(33, 0, 33, 30));
    assert_eq!(ui.bounds(children[2]).unwrap(), RectI::new(66, 0, 34, 30));
}

#[test]
fn apply_desc_rebuild_drops_weights_until_reregistered() {
    let mut source = Ui::<64>::new(800, 600);
    let root = source.root();
    let container = add(&mut source, root, row(600, 120, 0));
    let first = add(&mut source, container, rect());
    let second = add(&mut source, container, rect());
    source.set_flex(first, 1).unwrap();
    source.set_flex(second, 2).unwrap();
    source.layout();
    let mut bytes = [0u8; 512];
    let len = source.encode_desc(&mut bytes).unwrap();

    let mut rebuilt = Ui::<64>::new(800, 600);
    rebuilt.apply_desc(&bytes[..len]).unwrap();
    let mut rebuilt_bytes = [0u8; 512];
    let rebuilt_len = rebuilt.encode_desc(&mut rebuilt_bytes).unwrap();
    assert_eq!(len, rebuilt_len);
    assert_eq!(bytes[..len], rebuilt_bytes[..rebuilt_len]);
    rebuilt.layout();
    let dump_seed = rebuilt.root();
    let mut dump = [
        berry_ui::sdk::LayoutDumpEntry {
            id: dump_seed, parent: None, first_child: None, next_sibling: None,
            kind: ViewKind::Root, key: 0, depth: 0, x: 0, y: 0, w: 0, h: 0,
            measured_w: 0, measured_h: 0, layout_dirty: false, paint_dirty: false,
            bounds: RectI::new(0, 0, 0, 0),
        }; 4];

    let count = rebuilt.layout_dump(&mut dump).unwrap();
    assert_eq!(count, 4);
    let rebuilt_first = dump[2].id;
    let rebuilt_second = dump[3].id;
    assert_eq!(rebuilt.bounds(rebuilt_first).unwrap(), RectI::new(0, 0, 0, 120));
    assert_eq!(rebuilt.bounds(rebuilt_second).unwrap(), RectI::new(0, 0, 0, 120));
    rebuilt.set_flex(rebuilt_first, 1).unwrap();
    rebuilt.set_flex(rebuilt_second, 2).unwrap();
    rebuilt.layout();
    assert_eq!(rebuilt.bounds(rebuilt_first).unwrap(), RectI::new(0, 0, 200, 120));
    assert_eq!(rebuilt.bounds(rebuilt_second).unwrap(), RectI::new(200, 0, 400, 120));
}

#[test]
fn removed_flex_slot_reuse_has_no_stale_weight() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.set_flex(first, 1).unwrap();
    ui.set_flex(second, 2).unwrap();
    ui.remove(first).unwrap();
    assert_eq!(ui.flex_iter().count(), 1);
    assert_eq!(ui.set_flex(first, 1), Err(UiError::NodeBad));
    let replacement = add(&mut ui, container, rect());
    ui.set_flex(replacement, 1).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(replacement).unwrap(), RectI::new(400, 0, 200, 120));
}

#[test]
fn cross_start_keeps_main_slot_but_collapses_cross_extent() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(
        &mut ui,
        root,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(600),
                h: Some(120),
                cross: CrossAlign::Start,
            }),
        },
    );
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.set_flex(first, 1).unwrap();
    ui.set_flex(second, 2).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 200, 0));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(200, 0, 400, 0));
}

#[test]
fn non_row_col_container_parents_are_rejected() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let pad = add(
        &mut ui,
        root,
        ViewSpec {
            kind: ViewKind::Pad,
            key: 0,
            props: Props::Pad(berry_ui::desc::PadProps { all: 4 }),
        },
    );
    let child = add(&mut ui, pad, rect());
    assert_eq!(ui.set_flex(child, 1), Err(UiError::NodeBad));
    let grid = add(
        &mut ui,
        root,
        ViewSpec {
            kind: ViewKind::Grid,
            key: 0,
            props: Props::Grid(berry_ui::desc::GridProps {
                cols: 1,
                rows: 1,
                gap: 0,
                w: Some(100),
                h: Some(100),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    let grid_child = add(&mut ui, grid, rect());
    assert_eq!(ui.set_flex(grid_child, 1), Err(UiError::NodeBad));
}


#[test]
fn circle_child_is_flex_eligible() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let circle = ui
        .add(
            container,
            ViewSpec {
                kind: ViewKind::Circle,
                key: 0,
                props: Props::Circle(CircleProps {
                    bg: Color { r: 4, g: 5, b: 6 },
                    w: None,
                    h: None,
                }),
            },
        )
        .unwrap();
    let rect_child = add(&mut ui, container, rect());
    ui.set_flex(circle, 1).unwrap();
    ui.set_flex(rect_child, 1).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(circle).unwrap(), RectI::new(0, 0, 300, 120));
    assert_eq!(ui.bounds(rect_child).unwrap(), RectI::new(300, 0, 300, 120));
}

#[test]
fn taking_flex_reflows_surviving_sibling() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.set_flex(first, 1).unwrap();
    ui.set_flex(second, 2).unwrap();
    ui.layout();
    ui.drain_damage();
    assert!(ui.take_flex(second));
    let stat = ui.layout();
    assert!(stat.placed > 0);
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 600, 120));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(600, 0, 0, 120));
    assert!(!ui.damage().is_empty());
}

#[test]
fn maximum_weight_domain_stays_deterministic() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let tiny = add(&mut ui, container, rect());
    let huge = add(&mut ui, container, rect());
    ui.set_flex(tiny, 1).unwrap();
    ui.set_flex(huge, u16::MAX).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(tiny).unwrap(), RectI::new(0, 0, 0, 120));
    assert_eq!(ui.bounds(huge).unwrap(), RectI::new(0, 0, 600, 120));
}

#[test]
fn flex_allocation_never_writes_measured_extent() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.set_flex(first, 1).unwrap();
    ui.set_flex(second, 2).unwrap();
    ui.layout();
    let dump_seed = ui.root();
    let mut dump = [
        berry_ui::sdk::LayoutDumpEntry {
            id: dump_seed, parent: None, first_child: None, next_sibling: None,
            kind: ViewKind::Root, key: 0, depth: 0, x: 0, y: 0, w: 0, h: 0,
            measured_w: 0, measured_h: 0, layout_dirty: false, paint_dirty: false,
            bounds: RectI::new(0, 0, 0, 0),
        }; 4];
    assert_eq!(ui.layout_dump(&mut dump).unwrap(), 4);
    assert_eq!(dump[2].measured_w, 0);
    assert_eq!(dump[2].measured_h, 0);
    assert_eq!(dump[3].measured_w, 0);
    assert_eq!(dump[3].measured_h, 0);
    assert_eq!(dump[2].bounds.w, 200);
    assert_eq!(dump[3].bounds.w, 400);
}
