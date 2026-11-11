//! GUI-14 L3：transposed container flex + 最小 ListDetail 布局原语。
//!
//! 覆盖：Row 中 Col/Panel pane、Col 中 Row pane、同向容器拒面、props
//! 离开 eligible 形态摘除、ListDetail 1:2 几何、AST runtime-only、重建
//! 重登记、damage/hit 与 C1 focus 生命周期。

use berry_gfx::cmd::Color;
use berry_ui::desc::{
    ColProps, PanelProps, Props, RectProps, RowProps, ViewKind, ViewSpec,
};
use berry_ui::damage::RectI;
use berry_ui::layout::CrossAlign;
use berry_ui::sdk::{ListDetail, ListDetailSpec, Theme, WidgetContext};
use berry_ui::{NodeId, Ui, UiError};

fn rect(w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: Color { r: 10, g: 20, b: 30 },
            w,
            h,
        }),
    }
}

fn row_container(w: u32, h: u32, gap: u32) -> ViewSpec {
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

fn col_pane() -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Col,
        key: 0,
        props: Props::Col(ColProps {
            gap: 0,
            w: None,
            h: None,
            cross: CrossAlign::Stretch,
        }),
    }
}

fn add(ui: &mut Ui<64>, parent: NodeId, spec: ViewSpec) -> NodeId {
    ui.add(parent, spec).unwrap()
}

#[test]
fn row_transposed_col_panes_consume_flex_slots() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let row = add(&mut ui, root, row_container(600, 300, 12));
    let list = add(&mut ui, row, col_pane());
    let list_item = add(&mut ui, list, rect(None, Some(300)));
    let detail = add(&mut ui, row, col_pane());
    let detail_item = add(&mut ui, detail, rect(None, Some(300)));
    ui.set_flex(list, 1).unwrap();
    ui.set_flex(detail, 2).unwrap();
    ui.layout();

    assert_eq!(ui.bounds(list).unwrap(), RectI::new(0, 0, 196, 300));
    assert_eq!(ui.bounds(detail).unwrap(), RectI::new(208, 0, 392, 300));
    assert_eq!(ui.bounds(list_item).unwrap(), RectI::new(0, 0, 196, 300));
    assert_eq!(ui.bounds(detail_item).unwrap(), RectI::new(208, 0, 392, 300));
}

#[test]
fn row_panel_pane_and_grid_pane_are_eligible() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let row = add(&mut ui, root, row_container(600, 120, 0));
    let panel = add(
        &mut ui,
        row,
        ViewSpec {
            kind: ViewKind::Panel,
            key: 0,
            props: Props::Panel(PanelProps {
                bg: Color { r: 1, g: 2, b: 3 },
                w: None,
                h: Some(120),
                padding: 0,
                gap: 0,
                radius: 0,
            }),
        },
    );
    let grid = add(
        &mut ui,
        row,
        ViewSpec {
            kind: ViewKind::Grid,
            key: 0,
            props: Props::Grid(berry_ui::desc::GridProps {
                cols: 1,
                rows: 1,
                gap: 0,
                w: None,
                h: Some(120),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    ui.set_flex(panel, 1).unwrap();
    ui.set_flex(grid, 3).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(panel).unwrap(), RectI::new(0, 0, 150, 120));
    assert_eq!(ui.bounds(grid).unwrap(), RectI::new(150, 0, 450, 120));
}

#[test]
fn col_transposed_row_panes_consume_vertical_slots() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let col = add(
        &mut ui,
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 12,
                w: Some(300),
                h: Some(600),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    let first = add(
        &mut ui,
        col,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(300),
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    );
    let second = add(
        &mut ui,
        col,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(300),
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    );
    ui.set_flex(first, 1).unwrap();
    ui.set_flex(second, 2).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(first).unwrap(), RectI::new(0, 0, 300, 196));
    assert_eq!(ui.bounds(second).unwrap(), RectI::new(0, 208, 300, 392));
}

#[test]
fn same_orientation_and_intrinsic_main_containers_reject() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let row = add(&mut ui, root, row_container(600, 120, 0));
    let row_child = add(
        &mut ui,
        row,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: None,
                h: Some(120),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    assert_eq!(ui.set_flex(row_child, 1), Err(UiError::NodeBad));

    let col = add(
        &mut ui,
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(300),
                h: Some(600),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    let col_child = add(&mut ui, col, col_pane());
    assert_eq!(ui.set_flex(col_child, 1), Err(UiError::NodeBad));
    let panel_child = add(
        &mut ui,
        col,
        ViewSpec {
            kind: ViewKind::Panel,
            key: 0,
            props: Props::Panel(PanelProps {
                bg: Color { r: 1, g: 2, b: 3 },
                w: Some(20),
                h: None,
                padding: 0,
                gap: 0,
                radius: 0,
            }),
        },
    );
    assert_eq!(ui.set_flex(panel_child, 1), Err(UiError::NodeBad));
}

#[test]
fn container_props_leaving_eligible_form_drop_flex() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let row = add(&mut ui, root, row_container(600, 120, 0));
    let pane = add(&mut ui, row, col_pane());
    ui.set_flex(pane, 1).unwrap();
    ui.set_props(
        pane,
        Props::Col(ColProps {
            gap: 0,
            w: Some(100),
            h: None,
            cross: CrossAlign::Stretch,
        }),
    )
    .unwrap();
    assert_eq!(ui.flex_iter().count(), 0);
    assert!(ui.set_flex(pane, 1).is_err());

    let col = add(
        &mut ui,
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(300),
                h: Some(600),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    let row_pane = add(
        &mut ui,
        col,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(300),
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    );
    ui.set_flex(row_pane, 1).unwrap();
    ui.set_props(
        row_pane,
        Props::Row(RowProps {
            gap: 0,
            w: None,
            h: Some(100),
            cross: CrossAlign::Stretch,
        }),
    )
    .unwrap();
    assert_eq!(ui.flex_iter().count(), 0);
}

#[test]
fn list_detail_primitive_layout_damage_and_hit() {
    let mut ui = Ui::<64>::new(800, 600);
    let ctx = WidgetContext::for_viewport(Theme::light(), 800, 600);
    let root = ui.root();
    let ld = ListDetail::new(
        &mut ui,
        root,
        &ctx,
        ListDetailSpec {
            width: 600,
            height: 300,
            gap: 12,
            list_weight: 1,
            detail_weight: 2,
        },
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(ld.row).unwrap(), RectI::new(0, 0, 600, 300));
    assert_eq!(ui.bounds(ld.list).unwrap(), RectI::new(0, 0, 196, 300));
    assert_eq!(ui.bounds(ld.detail).unwrap(), RectI::new(208, 0, 392, 300));
    assert_eq!(ui.node_at(50, 150), Some(ld.list));
    assert_eq!(ui.node_at(300, 150), Some(ld.detail));

    ui.drain_damage();
    ui.set_flex(ld.detail, 1).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(ld.list).unwrap(), RectI::new(0, 0, 294, 300));
    assert_eq!(ui.bounds(ld.detail).unwrap(), RectI::new(306, 0, 294, 300));
    assert!(!ui.damage().is_empty());
    assert!(!ui.damage().full);
}

#[test]
fn list_detail_ast_stays_runtime_only_and_rebuild_requires_reregister() {
    let mut ui = Ui::<64>::new(800, 600);
    let ctx = WidgetContext::for_viewport(Theme::light(), 800, 600);
    let root = ui.root();
    let _ld = ListDetail::new(
        &mut ui,
        root,
        &ctx,
        ListDetailSpec {
            width: 600,
            height: 300,
            gap: 12,
            list_weight: 1,
            detail_weight: 2,
        },
    )
    .unwrap();
    ui.layout();
    let mut stream = [0u8; 1024];
    let len = ui.encode_desc(&mut stream).unwrap();

    let mut rebuilt = Ui::<64>::new(800, 600);
    rebuilt.apply_desc(&stream[..len]).unwrap();
    let mut rebuilt_stream = [0u8; 1024];
    let rebuilt_len = rebuilt.encode_desc(&mut rebuilt_stream).unwrap();
    assert_eq!(len, rebuilt_len);
    assert_eq!(stream[..len], rebuilt_stream[..rebuilt_len]);
    rebuilt.layout();
    let seed = rebuilt.root();
    let mut dump = [
        berry_ui::sdk::LayoutDumpEntry {
            id: seed, parent: None, first_child: None, next_sibling: None,
            kind: ViewKind::Root, key: 0, depth: 0, x: 0, y: 0, w: 0, h: 0,
            measured_w: 0, measured_h: 0, layout_dirty: false, paint_dirty: false,
            bounds: RectI::new(0, 0, 0, 0),
        }; 4];
    assert_eq!(rebuilt.layout_dump(&mut dump).unwrap(), 4);
    let rebuilt_list = dump[2].id;
    let rebuilt_detail = dump[3].id;
    // 无 flex 重建：两个空 Panel 的内在宽各 = 2×padding(4)=8，
    // Row 以 12px gap 排列。
    assert_eq!(rebuilt.bounds(rebuilt_list).unwrap(), RectI::new(0, 0, 8, 300));
    assert_eq!(
        rebuilt.bounds(rebuilt_detail).unwrap(),
        RectI::new(20, 0, 8, 300)
    );
    rebuilt.set_flex(rebuilt_list, 1).unwrap();
    rebuilt.set_flex(rebuilt_detail, 2).unwrap();
    rebuilt.layout();
    assert_eq!(rebuilt.bounds(rebuilt_list).unwrap(), RectI::new(0, 0, 196, 300));
    assert_eq!(rebuilt.bounds(rebuilt_detail).unwrap(), RectI::new(208, 0, 392, 300));
}

#[test]
fn list_detail_content_keeps_c1_focus_lifecycle() {
    let mut ui = Ui::<64>::new(800, 600);
    let ctx = WidgetContext::for_viewport(Theme::light(), 800, 600);
    let root = ui.root();
    let ld = ListDetail::new(
        &mut ui,
        root,
        &ctx,
        ListDetailSpec {
            width: 600,
            height: 300,
            gap: 12,
            list_weight: 1,
            detail_weight: 2,
        },
    )
    .unwrap();
    let list_item = add(&mut ui, ld.list, rect(Some(20), Some(20)));
    let detail_item = add(&mut ui, ld.detail, rect(Some(20), Some(20)));
    ui.set_focusable(list_item, Some(list_item), true).unwrap();
    ui.set_focusable(detail_item, Some(detail_item), true).unwrap();
    ui.layout();

    let mut order = [list_item; 2];
    assert_eq!(ui.focus_order_into(&mut order), 2);
    assert_eq!(order, [list_item, detail_item]);
    let events = ui.focus_move(berry_ui::sdk::FocusDir::Next);
    assert_eq!(
        events.iter().copied().collect::<Vec<_>>(),
        vec![berry_ui::sdk::Msg::FocusIn { node: list_item }]
    );
    let events = ui.focus_move(berry_ui::sdk::FocusDir::Next);
    assert_eq!(
        events.iter().copied().collect::<Vec<_>>(),
        vec![
            berry_ui::sdk::Msg::FocusOut { node: list_item },
            berry_ui::sdk::Msg::FocusIn { node: detail_item }
        ]
    );
    assert_eq!(ui.focus_node(), Some(detail_item));

    ui.remove(ld.row).unwrap();
    assert_eq!(ui.flex_iter().count(), 0);
    assert_eq!(ui.focus_node(), None);
}

#[test]
fn collapsed_pane_subtree_remains_hittable_by_existing_semantics() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let row = add(&mut ui, root, row_container(0, 120, 12));
    let pane = add(&mut ui, row, col_pane());
    let child = add(&mut ui, pane, rect(Some(20), Some(20)));
    ui.set_flex(pane, 1).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(pane).unwrap(), RectI::new(0, 0, 0, 20));
    // Pane 自身零面积不可命中；但 hit_walk 不以普通父界裁剪子树，
    // 这是既有容器语义（Clip 才是局部裁剪权威），L3 文档化不改。
    assert_eq!(ui.node_at(5, 5), Some(child));
}

#[test]
fn nested_transposed_flex_composes_in_grown_pane() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let outer_row = add(&mut ui, root, row_container(600, 300, 0));
    let pane = add(
        &mut ui,
        outer_row,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: None,
                h: Some(300),
                cross: CrossAlign::Stretch,
            }),
        },
    );
    ui.set_flex(pane, 1).unwrap();
    let inner_top = add(
        &mut ui,
        pane,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(600),
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    );
    let inner_bottom = add(
        &mut ui,
        pane,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(600),
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    );
    ui.set_flex(inner_top, 1).unwrap();
    ui.set_flex(inner_bottom, 1).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(pane).unwrap(), RectI::new(0, 0, 600, 300));
    assert_eq!(ui.bounds(inner_top).unwrap(), RectI::new(0, 0, 600, 150));
    assert_eq!(
        ui.bounds(inner_bottom).unwrap(),
        RectI::new(0, 150, 600, 150)
    );
}

#[test]
fn cross_start_pane_keeps_nonzero_content_extent() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let row = add(
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
    let pane = add(&mut ui, row, col_pane());
    let child = add(&mut ui, pane, rect(Some(30), Some(50)));
    ui.set_flex(pane, 1).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(pane).unwrap(), RectI::new(0, 0, 600, 50));
    assert_eq!(ui.bounds(child).unwrap(), RectI::new(0, 0, 30, 50));
}

#[test]
fn flex_pane_measured_intrinsic_stays_slot_independent() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let row = add(&mut ui, root, row_container(600, 120, 0));
    let pane = add(&mut ui, row, col_pane());
    let _child = add(&mut ui, pane, rect(Some(40), Some(120)));
    ui.set_flex(pane, 1).unwrap();
    ui.layout();
    let seed = ui.root();
    let mut dump = [
        berry_ui::sdk::LayoutDumpEntry {
            id: seed, parent: None, first_child: None, next_sibling: None,
            kind: ViewKind::Root, key: 0, depth: 0, x: 0, y: 0, w: 0, h: 0,
            measured_w: 0, measured_h: 0, layout_dirty: false, paint_dirty: false,
            bounds: RectI::new(0, 0, 0, 0),
        }; 4];
    assert_eq!(ui.layout_dump(&mut dump).unwrap(), 4);
    assert_eq!(dump[2].measured_w, 40);
    assert_eq!(dump[2].bounds.w, 600);
    assert_eq!(dump[3].bounds.w, 40);
}

#[test]
fn pane_weight_change_does_not_emit_focus_messages() {
    let mut ui = Ui::<64>::new(800, 600);
    let ctx = WidgetContext::for_viewport(Theme::light(), 800, 600);
    let root = ui.root();
    let ld = ListDetail::new(
        &mut ui,
        root,
        &ctx,
        ListDetailSpec {
            width: 600,
            height: 300,
            gap: 12,
            list_weight: 1,
            detail_weight: 2,
        },
    )
    .unwrap();
    let item = add(&mut ui, ld.list, rect(Some(20), Some(20)));
    ui.set_focusable(item, Some(item), true).unwrap();
    ui.layout();
    assert_eq!(
        ui.focus_move(berry_ui::sdk::FocusDir::Next)
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![berry_ui::sdk::Msg::FocusIn { node: item }]
    );
    ui.set_flex(ld.detail, 1).unwrap();
    assert_eq!(ui.focus_node(), Some(item));
}

#[test]
fn pane_flex_registration_does_not_drift_ast() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let row = add(&mut ui, root, row_container(600, 120, 0));
    let pane = add(&mut ui, row, col_pane());
    let child = add(&mut ui, pane, rect(None, Some(120)));
    ui.layout();
    let mut before = [0u8; 512];
    let before_len = ui.encode_desc(&mut before).unwrap();
    ui.set_flex(pane, 1).unwrap();
    let mut after = [0u8; 512];
    let after_len = ui.encode_desc(&mut after).unwrap();
    assert_eq!(before_len, after_len);
    assert_eq!(before[..before_len], after[..after_len]);
    assert_eq!(ui.flex_iter().count(), 1);
    ui.layout();
    assert_eq!(ui.bounds(child).unwrap().w, 600);
}

#[test]
fn list_detail_zero_weight_rejects_before_node_creation() {
    let mut ui = Ui::<64>::new(800, 600);
    let ctx = WidgetContext::for_viewport(Theme::light(), 800, 600);
    let before = ui.flex_iter().count();
    let root = ui.root();
    let result = ListDetail::new(
        &mut ui,
        root,
        &ctx,
        ListDetailSpec {
            width: 600,
            height: 300,
            gap: 12,
            list_weight: 0,
            detail_weight: 2,
        },
    );
    assert_eq!(result, Err(UiError::NodeBad));
    assert_eq!(ui.flex_iter().count(), before);
}

#[test]
fn deferred_kinds_and_mismatched_kind_props_reject() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let row = add(&mut ui, root, row_container(600, 120, 0));

    let clip = add(
        &mut ui,
        row,
        ViewSpec {
            kind: ViewKind::Clip,
            key: 0,
            props: Props::Clip(berry_ui::desc::ClipProps { w: None, h: None }),
        },
    );
    assert_eq!(ui.set_flex(clip, 1), Err(UiError::NodeBad));

    let pad = add(
        &mut ui,
        row,
        ViewSpec {
            kind: ViewKind::Pad,
            key: 0,
            props: Props::Pad(berry_ui::desc::PadProps { all: 4 }),
        },
    );
    assert_eq!(ui.set_flex(pad, 1), Err(UiError::NodeBad));

    let image = add(
        &mut ui,
        row,
        ViewSpec {
            kind: ViewKind::Image,
            key: 0,
            props: Props::Image(berry_ui::desc::ImageProps { res: 1, w: None, h: None }),
        },
    );
    assert_eq!(ui.set_flex(image, 1), Err(UiError::NodeBad));

    let icon = add(
        &mut ui,
        row,
        ViewSpec {
            kind: ViewKind::Icon,
            key: 0,
            props: Props::Icon(berry_ui::desc::IconProps {
                id: 1,
                fg: Color { r: 1, g: 2, b: 3 },
                w: None,
                h: None,
            }),
        },
    );
    assert_eq!(ui.set_flex(icon, 1), Err(UiError::NodeBad));

    // builder 误配 kind/props 也不得进入 flex 读侧。
    let mismatch = ui
        .add(
            row,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: Color { r: 1, g: 2, b: 3 },
                    w: None,
                    h: None,
                }),
            },
        )
        .unwrap();
    assert_eq!(ui.set_flex(mismatch, 1), Err(UiError::NodeBad));
}
