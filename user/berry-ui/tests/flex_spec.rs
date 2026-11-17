//! LAYOUT-L6a：runtime-only flex basis/grow/shrink 代数值级钉板。
//!
//! L2 `set_flex(weight)` 是 `FlexSpec { grow, shrink: 0, basis: Px(0) }`
//! 的冻结语法糖；本文件同时钉住非零 basis、Auto basis、capped shrink
//! freeze loop 与 runtime-only 生命周期。

use berry_gfx::cmd::Color;
use berry_ui::desc::{
    ColProps, Props, RectProps, RowProps, ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::sdk::{
    FlexBasis, FlexSpec, LayoutDumpEntry, Ui,
};
use berry_ui::damage::RectI;
use berry_ui::{NodeId, UiError};

fn rect() -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: Color { r: 10, g: 20, b: 30 },
            w: None,
            h: None,
            radius: 0,
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
            radius: 0,
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

fn add_pane(ui: &mut Ui<64>, parent: NodeId, content_w: u32) -> NodeId {
    let pane = ui
        .add(
            parent,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: None,
                    h: Some(100),
                    cross: CrossAlign::Start,
                }),
            },
        )
        .unwrap();
    ui.add(pane, fixed_rect(content_w, 20)).unwrap();
    pane
}

fn add(ui: &mut Ui<64>, parent: NodeId, spec: ViewSpec) -> NodeId {
    ui.add(parent, spec).unwrap()
}

fn add_keyed(ui: &mut Ui<64>, parent: NodeId, mut spec: ViewSpec, key: u64) -> NodeId {
    spec.key = key;
    ui.add(parent, spec).unwrap()
}

#[test]
fn set_flex_sugar_is_explicit_zero_basis_grow_only_spec() {
    let mut explicit = Ui::<64>::new(800, 600);
    let root = explicit.root();
    let container = add(&mut explicit, root, row(600, 120, 0));
    let first = add(&mut explicit, container, rect());
    let second = add(&mut explicit, container, rect());
    explicit
        .set_flex_spec(first, FlexSpec {
            grow: 1,
            shrink: 0,
            basis: FlexBasis::Px(0),
        })
        .unwrap();
    explicit
        .set_flex_spec(second, FlexSpec {
            grow: 2,
            shrink: 99, // Px(0) 下 shrink 不参与结果。
            basis: FlexBasis::Px(0),
        })
        .unwrap();
    explicit.layout();
    assert_eq!(explicit.bounds(first).unwrap(), RectI::new(0, 0, 200, 120));
    assert_eq!(explicit.bounds(second).unwrap(), RectI::new(200, 0, 400, 120));
    assert_eq!(explicit.flex_spec(first), Some(FlexSpec {
        grow: 1,
        shrink: 0,
        basis: FlexBasis::Px(0),
    }));
}

#[test]
fn px_basis_grow_distributes_only_positive_free_space() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 12));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.set_flex_spec(first, FlexSpec {
        grow: 1,
        shrink: 0,
        basis: FlexBasis::Px(100),
    })
    .unwrap();
    ui.set_flex_spec(second, FlexSpec {
        grow: 2,
        shrink: 0,
        basis: FlexBasis::Px(0),
    })
    .unwrap();
    ui.layout();

    // base 100 + 0，free = 600 - 12 - 100 = 488；1:2 分成 162/326。
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 262, 120));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(274, 0, 326, 120));
}

#[test]
fn auto_basis_reads_child_main_intrinsic() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 100, 0));
    let first = add_pane(&mut ui, container, 40);
    let second = add_pane(&mut ui, container, 20);
    ui.set_flex_spec(first, FlexSpec {
        grow: 1,
        shrink: 0,
        basis: FlexBasis::Auto,
    })
    .unwrap();
    ui.set_flex_spec(second, FlexSpec {
        grow: 1,
        shrink: 0,
        basis: FlexBasis::Auto,
    })
    .unwrap();
    ui.layout();

    // base 40 + 20，free 540 等分成 270/270；最终 310/290。
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 310, 100));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(310, 0, 290, 100));
}

#[test]
fn proportional_shrink_consumes_deficit_without_touching_fixed_children() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(300, 120, 0));
    let fixed = add(&mut ui, container, fixed_rect(200, 120));
    let flexible = add(&mut ui, container, rect());
    ui.set_flex_spec(flexible, FlexSpec {
        grow: 0,
        shrink: 1,
        basis: FlexBasis::Px(200),
    })
    .unwrap();
    ui.layout();

    assert_eq!(ui.bounds(fixed).unwrap(), RectI::new(0, 0, 200, 120));
    assert_eq!(ui.bounds(flexible).unwrap(), RectI::new(200, 0, 100, 120));
}

#[test]
fn shrink_freeze_loop_caps_overscaled_child_and_fills_deficit_exactly() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(50, 120, 0));
    let large = add(&mut ui, container, rect());
    let tiny = add(&mut ui, container, rect());
    ui.set_flex_spec(large, FlexSpec {
        grow: 0,
        shrink: 1,
        basis: FlexBasis::Px(100),
    })
    .unwrap();
    ui.set_flex_spec(tiny, FlexSpec {
        grow: 0,
        shrink: u16::MAX,
        basis: FlexBasis::Px(1),
    })
    .unwrap();
    ui.layout();

    // deficit 51：tiny 先按 65535 权重冻结到 base=1，large 收掉剩余 50。
    assert_eq!(ui.bounds(large).unwrap(), RectI::new(0, 0, 50, 120));
    assert_eq!(ui.bounds(tiny).unwrap(), RectI::new(50, 0, 0, 120));
}

#[test]
fn zero_shrink_pool_preserves_l2_overflow_semantics() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(50, 120, 4));
    let fixed = add(&mut ui, container, fixed_rect(60, 120));
    let flexible = add(&mut ui, container, rect());
    ui.set_flex(flexible, 1).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(fixed).unwrap(), RectI::new(0, 0, 60, 120));
    assert_eq!(ui.bounds(flexible).unwrap(), RectI::new(64, 0, 0, 120));
}

#[test]
fn shrink_only_spec_is_legal_and_never_grows() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let roomy = add(&mut ui, root, row(100, 120, 0));
    let roomy_child = add(&mut ui, roomy, rect());
    ui.set_flex_spec(roomy_child, FlexSpec {
        grow: 0,
        shrink: 1,
        basis: FlexBasis::Px(80),
    })
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(roomy_child).unwrap(), RectI::new(0, 0, 80, 120));

    let mut tight = Ui::<64>::new(800, 600);
    let root = tight.root();
    let container = add(&mut tight, root, row(60, 120, 0));
    let child = add(&mut tight, container, rect());
    tight
        .set_flex_spec(child, FlexSpec {
            grow: 0,
            shrink: 1,
            basis: FlexBasis::Px(80),
        })
        .unwrap();
    tight.layout();
    assert_eq!(tight.bounds(child).unwrap(), RectI::new(0, 0, 60, 120));
}

#[test]
fn dead_spec_and_ineligible_node_are_loud_rejected() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let child = add(&mut ui, container, rect());
    assert_eq!(
        ui.set_flex_spec(child, FlexSpec {
            grow: 0,
            shrink: 0,
            basis: FlexBasis::Auto,
        }),
        Err(UiError::NodeBad)
    );
    assert_eq!(ui.flex_spec(child), None);

    let fixed = add(&mut ui, container, fixed_rect(10, 10));
    assert_eq!(
        ui.set_flex_spec(fixed, FlexSpec {
            grow: 1,
            shrink: 0,
            basis: FlexBasis::Px(1),
        }),
        Err(UiError::NodeBad)
    );
}

#[test]
fn same_spec_overwrite_is_zero_op_and_change_dirties_allocation() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let child = add(&mut ui, container, rect());
    ui.set_flex_spec(child, FlexSpec {
        grow: 1,
        shrink: 2,
        basis: FlexBasis::Px(3),
    })
    .unwrap();
    ui.layout();
    ui.drain_damage();

    ui.set_flex_spec(child, FlexSpec {
        grow: 1,
        shrink: 2,
        basis: FlexBasis::Px(3),
    })
    .unwrap();
    assert!(ui.damage().is_empty());
    let stat = ui.layout();
    assert_eq!(stat.measured, 0);
    assert_eq!(stat.placed, 0);

    ui.set_flex_spec(child, FlexSpec {
        grow: 1,
        shrink: 2,
        basis: FlexBasis::Px(4),
    })
    .unwrap();
    ui.layout();
    assert!(!ui.damage().is_empty());
}

#[test]
fn flex_spec_is_runtime_only_and_preserves_ast_bytes() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let child = add(&mut ui, container, rect());
    ui.layout();
    let mut before = [0u8; 512];
    let before_len = ui.encode_desc(&mut before).unwrap();

    ui.set_flex_spec(child, FlexSpec {
        grow: 1,
        shrink: 2,
        basis: FlexBasis::Px(30),
    })
    .unwrap();
    let mut after = [0u8; 512];
    let after_len = ui.encode_desc(&mut after).unwrap();
    assert_eq!(before_len, after_len);
    assert_eq!(before[..before_len], after[..after_len]);
}

#[test]
fn full_rebuild_drops_spec_and_keyed_reapply_preserves_it() {
    let mut source = Ui::<64>::new(800, 600);
    let root = source.root();
    let container = add_keyed(&mut source, root, row(600, 120, 0), 1);
    let first = add_keyed(&mut source, container, rect(), 2);
    let second = add_keyed(&mut source, container, rect(), 3);
    source.set_flex(first, 1).unwrap();
    source.set_flex(second, 2).unwrap();
    // key 身份 + 旁观众节点让同一 AST 走 matched 分支，钉住 side table。
    let _bystander = add_keyed(&mut source, root, fixed_rect(10, 20), 4);
    let mut bytes = [0u8; 512];
    let len = source.encode_desc(&mut bytes).unwrap();

    let mut rebuilt = Ui::<64>::new(800, 600);
    rebuilt.apply_desc(&bytes[..len]).unwrap();
    assert_eq!(rebuilt.flex_iter().count(), 0);
    rebuilt.layout();
    assert_eq!(rebuilt.flex_spec(first), None);

    // keyed re-apply：同 AST / key 身份匹配，side table 存活。
    let stat = source.apply_desc_keyed(&bytes[..len]).unwrap();
    assert!(!stat.fallback_full);
    assert!(stat.matched_clean >= 2);
    assert_eq!(source.flex_spec(first), Some(FlexSpec {
        grow: 1,
        shrink: 0,
        basis: FlexBasis::Px(0),
    }));
    source.layout();
    assert_eq!(source.bounds(first).unwrap(), RectI::new(0, 0, 200, 120));
    assert_eq!(source.bounds(second).unwrap(), RectI::new(200, 0, 400, 120));
}

#[test]
fn flex_allocation_never_writes_measured_extent() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(600, 120, 0));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.set_flex_spec(first, FlexSpec {
        grow: 1,
        shrink: 1,
        basis: FlexBasis::Px(100),
    })
    .unwrap();
    ui.set_flex_spec(second, FlexSpec {
        grow: 2,
        shrink: 1,
        basis: FlexBasis::Px(100),
    })
    .unwrap();
    ui.layout();

    let seed = ui.root();
    let mut dump = [LayoutDumpEntry {
        id: seed,
        parent: None,
        first_child: None,
        next_sibling: None,
        kind: ViewKind::Root,
        key: 0,
        depth: 0,
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        measured_w: 0,
        measured_h: 0,
        layout_dirty: false,
        paint_dirty: false,
        bounds: RectI::new(0, 0, 0, 0),
    }; 4];
    assert_eq!(ui.layout_dump(&mut dump).unwrap(), 4);
    assert_eq!(dump[2].measured_w, 0);
    assert_eq!(dump[2].measured_h, 0);
    assert_eq!(dump[3].measured_w, 0);
    assert_eq!(dump[3].measured_h, 0);
    assert_eq!(dump[2].bounds, RectI::new(0, 0, 233, 120));
    assert_eq!(dump[3].bounds, RectI::new(233, 0, 367, 120));
}

#[test]
fn list_detail_px_basis_restores_l2_geometry() {
    let mut ui = Ui::<64>::new(1080, 1920);
    let root = ui.root();
    let ctx = berry_ui::sdk::WidgetContext::for_viewport(
        berry_ui::sdk::Theme::light(),
        1080,
        1920,
    );
    let adaptive = berry_ui::sdk::ListDetail::new(
        &mut ui,
        root,
        &ctx,
        berry_ui::sdk::ListDetailSpec {
            width: 1056,
            height: 120,
            gap: 12,
            list_weight: 1,
            detail_weight: 2,
        },
    )
    .unwrap();
    ui.set_flex_spec(adaptive.list, FlexSpec {
        grow: 1,
        shrink: 1,
        basis: FlexBasis::Px(100),
    })
    .unwrap();
    ui.set_flex_spec(adaptive.detail, FlexSpec {
        grow: 2,
        shrink: 1,
        basis: FlexBasis::Px(40),
    })
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(adaptive.list).unwrap(), RectI::new(0, 0, 401, 120));
    assert_eq!(ui.bounds(adaptive.detail).unwrap(), RectI::new(413, 0, 643, 120));

    ui.set_flex(adaptive.list, 1).unwrap();
    ui.set_flex(adaptive.detail, 2).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(adaptive.list).unwrap(), RectI::new(0, 0, 348, 120));
    assert_eq!(ui.bounds(adaptive.detail).unwrap(), RectI::new(360, 0, 696, 120));
}

#[test]
fn shrink_never_produces_negative_slots() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(&mut ui, root, row(0, 120, 0));
    let first = add(&mut ui, container, rect());
    let second = add(&mut ui, container, rect());
    ui.set_flex_spec(first, FlexSpec {
        grow: 0,
        shrink: 1,
        basis: FlexBasis::Px(40),
    })
    .unwrap();
    ui.set_flex_spec(second, FlexSpec {
        grow: 0,
        shrink: 1,
        basis: FlexBasis::Px(60),
    })
    .unwrap();
    ui.layout();
    let a = ui.bounds(first).unwrap();
    let b = ui.bounds(second).unwrap();
    assert_eq!(a, RectI::new(0, 0, 0, 120));
    assert_eq!(b, RectI::new(0, 0, 0, 120));
}

#[test]
fn deterministic_random_specs_keep_fill_and_nonnegative_invariants() {
    let mut seed = 0x4c4f_5455_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for round in 0..64 {
        let width = u32::try_from(80 + next() % 421).unwrap();
        let gap = u32::try_from(next() % 5).unwrap();
        let mut ui = Ui::<64>::new(800, 600);
        let root = ui.root();
        let container = add(&mut ui, root, row(width, 90, gap));
        let mut children = [None; 4];
        let mut bases = [0u32; 4];
        let mut shrinkable_base = 0i64;
        for i in 0..4 {
            let child = add(&mut ui, container, rect());
            children[i] = Some(child);
            bases[i] = u32::try_from(next() % 121).unwrap();
            let shrink = u16::try_from(next() % 5).unwrap();
            if shrink > 0 {
                shrinkable_base += i64::from(bases[i]);
            }
            ui.set_flex_spec(child, FlexSpec {
                grow: u16::try_from(1 + next() % 5).unwrap(),
                shrink,
                basis: FlexBasis::Px(bases[i]),
            })
            .unwrap();
        }
        ui.layout();

        let slots: i64 = children
            .iter()
            .flatten()
            .map(|&child| i64::from(ui.bounds(child).unwrap().w))
            .sum();
        let gaps = i64::from(gap) * 3;
        let base_sum: i64 = bases.iter().map(|v| i64::from(*v)).sum();
        let width_i = i64::from(width);
        assert!(children.iter().flatten().all(|&c| ui.bounds(c).unwrap().w >= 0));
        if base_sum + gaps <= width_i {
            assert_eq!(slots + gaps, width_i, "grow fill round {round}");
        } else if shrinkable_base >= base_sum + gaps - width_i {
            assert_eq!(slots + gaps, width_i, "shrink fill round {round}");
        } else {
            assert!(slots + gaps > width_i);
        }
    }
}

#[test]
fn column_auto_basis_reads_vertical_intrinsic_and_grows() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let container = add(
        &mut ui,
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(120),
                h: Some(300),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    let mut panes = [None; 2];
    for (i, content_h) in [40u32, 20].into_iter().enumerate() {
        let pane = add(&mut ui, container, ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(120),
                h: None,
                cross: CrossAlign::Start,
            }),
        });
        ui.add(pane, fixed_rect(20, content_h)).unwrap();
        ui.set_flex_spec(pane, FlexSpec {
            grow: 1,
            shrink: 0,
            basis: FlexBasis::Auto,
        })
        .unwrap();
        panes[i] = Some(pane);
    }
    ui.layout();
    let first = panes[0].unwrap();
    let second = panes[1].unwrap();
    // Auto base 40+20，free 240 等分：120+120 → 160+140。
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 120, 160));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(0, 160, 120, 140));
}
