//! GUI F4：组件 API 的布局、状态、a11y、命中与 AST round-trip 钉板。
//!
//! 只经 `berry_ui::sdk` 消费组件；组件不得暴露 WM/VMO/像素路径。所有
//! 几何断言来自 layout dump/bounds，所有交互断言来自 Msg/NodeId。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_ui::sdk::{
    column, divider, icon, row, text, Button, Card, EmptyState, InteractionState, ListItem,
    Msg, PanelProps, Props, RectProps, RectI, Scaffold, ScrollView, Switch, TextVariant, Theme,
    Ui, UiError, ViewKind, ViewSpec, WidgetContext,
};

#[test]
fn a11y_roles_forward_berry_session_vocabulary() {
    assert_eq!(berry_ui::ROLE_NONE, berry_guiproto::surface::A11Y_ROLE_NONE);
    assert_eq!(berry_ui::ROLE_BUTTON, berry_guiproto::surface::A11Y_ROLE_BUTTON);
    assert_eq!(berry_ui::ROLE_TEXT, berry_guiproto::surface::A11Y_ROLE_TEXT);
    assert_eq!(berry_ui::ROLE_IMAGE, berry_guiproto::surface::A11Y_ROLE_IMAGE);
    assert_eq!(berry_ui::ROLE_TOGGLE, berry_guiproto::surface::A11Y_ROLE_TOGGLE);
    assert_eq!(berry_ui::ROLE_LIST_ITEM, berry_guiproto::surface::A11Y_ROLE_LIST_ITEM);
}

fn ctx(w: u32, h: u32) -> WidgetContext {
    WidgetContext::for_viewport(Theme::light(), w, h)
}

fn rect(bg: Color, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg, w, h }),
    }
}

#[test]
fn panel_is_drawn_container_and_roundtrips_ast() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let panel = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Panel,
                key: 9,
                props: Props::Panel(PanelProps {
                    bg: Color { r: 20, g: 30, b: 40 },
                    w: Some(80),
                    h: Some(60),
                    padding: 4,
                    gap: 4,
                    radius: 0,
                }),
            },
        )
        .unwrap();
    ui.add(panel, rect(Color { r: 1, g: 2, b: 3 }, Some(20), Some(10))).unwrap();
    ui.add(panel, rect(Color { r: 4, g: 5, b: 6 }, Some(20), Some(10))).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(panel).unwrap(), RectI::new(0, 0, 80, 60));
    let mut dump = [berry_ui::sdk::LayoutDumpEntry {
        id: root, parent: None, first_child: None, next_sibling: None, kind: ViewKind::Root,
        key: 0, depth: 0, x: 0, y: 0, w: 0, h: 0, bounds: RectI::new(0, 0, 0, 0),
        measured_w: 0, measured_h: 0, layout_dirty: false, paint_dirty: false,
    }; 4];
    let n = ui.layout_dump(&mut dump).unwrap();
    assert_eq!(n, 4);
    assert_eq!(ui.bounds(dump[2].id).unwrap(), RectI::new(4, 4, 20, 10));
    assert_eq!(ui.bounds(dump[3].id).unwrap(), RectI::new(4, 18, 20, 10));

    ui.wake(0);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 4];
    let mut enc = CommandEncoder::new();
    let report = ui.paint(&mut scratch, &mut enc);
    assert_eq!(report.cmds, 4, "Panel + Rect + Rect + Present");
    let dl = enc.finish();
    let kinds: Vec<_> = dl.iter().map(|cmd| match cmd {
        berry_gfx::cmd::Cmd::FillRect { .. } => 0,
        berry_gfx::cmd::Cmd::Present { .. } => 1,
        _ => 2,
    }).collect();
    assert_eq!(kinds, vec![0, 0, 0, 1]);

    let mut stream = [0u8; 256];
    let len = ui.encode_desc(&mut stream).unwrap();
    let mut rebuilt = Ui::<16>::new(100, 100);
    rebuilt.apply_desc(&stream[..len]).unwrap();
    rebuilt.layout();
    let mut rebuilt_dump = [berry_ui::sdk::LayoutDumpEntry {
        id: rebuilt.root(), parent: None, first_child: None, next_sibling: None,
        kind: ViewKind::Root, key: 0, depth: 0, x: 0, y: 0, w: 0, h: 0,
        bounds: RectI::new(0, 0, 0, 0), measured_w: 0, measured_h: 0,
        layout_dirty: false, paint_dirty: false,
    }; 4];
    assert_eq!(rebuilt.layout_dump(&mut rebuilt_dump).unwrap(), 4);
    assert_eq!(rebuilt_dump[1].kind, ViewKind::Panel);
    let rebuilt_panel = rebuilt_dump[1].id;
    assert_eq!(rebuilt.bounds(rebuilt_panel).unwrap(), RectI::new(0, 0, 80, 60));
    let mut stream2 = [0u8; 256];
    let len2 = rebuilt.encode_desc(&mut stream2).unwrap();
    assert_eq!(&stream[..len], &stream2[..len2]);
}

#[test]
fn scaffold_core_widgets_layout_state_and_a11y() {
    let mut ui = Ui::<96>::new(800, 600);
    let context = ctx(800, 600);
    let root = ui.root();
    let scaffold = Scaffold::new(&mut ui, root, &context, b"Settings").unwrap();
    let bar = scaffold.app_bar.panel;
    ui.layout();
    assert_eq!(ui.bounds(bar).unwrap(), RectI::new(0, 0, 800, 56), "24x48 Title + 2*4 padding");
    assert_eq!(ui.bounds(scaffold.body).unwrap(), RectI::new(0, 56, 800, 544));

    let card = Card::new(&mut ui, scaffold.body, &context).unwrap();
    let col = column(&mut ui, card.panel, &context).unwrap();
    let title = text(&mut ui, col, &context, TextVariant::Title, b"Title").unwrap();
    let body = text(&mut ui, col, &context, TextVariant::Body, b"Body").unwrap();
    let mut button = Button::new(&mut ui, col, &context, b"Save", InteractionState::Normal).unwrap();
    let mut item = ListItem::new(&mut ui, col, &context, b"WLAN", Some(b"Connected"), false).unwrap();
    let mut switch = Switch::new(&mut ui, col, &context, false, false).unwrap();
    let line = divider(&mut ui, col, &context).unwrap();
    ui.layout();

    assert!(ui.bounds(title).unwrap().h > 0);
    assert!(ui.bounds(body).unwrap().h > 0);
    assert_eq!(ui.bounds(line).unwrap().h, 1, "Compact Thin stroke = 1px");
    assert_eq!(
        (ui.bounds(switch.panel).unwrap().w, ui.bounds(switch.panel).unwrap().h),
        (52, 28),
        "Compact switch = 48x24 track + 2px padding"
    );
    let off_thumb = ui.bounds(switch.thumb).unwrap();
    switch.set_checked(&mut ui, &context, true).unwrap();
    ui.layout();
    let on_thumb = ui.bounds(switch.thumb).unwrap();
    assert_eq!(on_thumb.x - off_thumb.x, 28, "Compact travel = 48-20");
    assert!(switch.is_checked());

    // a11y sidecar has AppBar title, Body/Title text, Button, ListItem and Switch.
    let entries: Vec<_> = ui.a11y_iter().collect();
    assert!(entries.len() >= 5);
    assert!(entries.iter().any(|e| e.role == berry_ui::ROLE_BUTTON));
    assert!(entries.iter().any(|e| e.role == berry_ui::ROLE_TOGGLE));
    assert!(entries.iter().any(|e| e.role == berry_ui::ROLE_LIST_ITEM));

    // 后兄弟 Text 是 Button 命中面；组件 on_msg 识别 label 并返回 Click。
    let clicked = button
        .on_msg(&mut ui, &context, &Msg::Click { node: button.label })
        .unwrap();
    assert!(clicked);
    let detail = item.detail.expect("ListItem detail");
    assert_eq!(
        ui.bounds(detail).unwrap().h,
        32,
        "ListItem detail = Body S16x32（guest S32 档同源）"
    );
    let click_layout = ui.layout();
    assert_eq!(click_layout.measured, 0, "Button 纯状态换色零重测");
    let mut disabled =
        Button::new(&mut ui, col, &context, b"Off", InteractionState::Disabled).unwrap();
    assert!(!disabled
        .on_msg(
            &mut ui,
            &context,
            &Msg::Press { node: disabled.label, x: 1, y: 1 },
        )
        .unwrap());
    assert!(!disabled
        .on_msg(&mut ui, &context, &Msg::Click { node: disabled.label })
        .unwrap());

    item.set_selected(&mut ui, &context, true).unwrap();
    assert!(item.is_selected());
}

#[test]
fn row_icon_empty_state_and_hit_truth() {
    let mut ui = Ui::<64>::new(800, 600);
    let context = ctx(800, 600);
    let root = ui.root();
    let stack = column(&mut ui, root, &context).unwrap();
    let line = row(&mut ui, stack, &context).unwrap();
    let icon_a = icon(&mut ui, line, &context, 7).unwrap();
    let icon_b = icon(&mut ui, line, &context, 8).unwrap();
    let caption = text(&mut ui, stack, &context, TextVariant::Caption, b"Caption").unwrap();
    let empty = EmptyState::new(
        &mut ui, stack, &context, 7, b"No items", b"Create something first",
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(icon_a).unwrap().w, 16, "Compact L token = 16px");
    assert_eq!(ui.bounds(icon_b).unwrap().x - ui.bounds(icon_a).unwrap().x, 20, "gap 4 + icon 16");
    assert!(ui.bounds(empty.panel).unwrap().h > 0);
    assert_eq!(ui.bounds(caption).unwrap().h, 16, "Compact Caption = S8x16");
    assert!(ui.bounds(empty.title).unwrap().h > 0);
    assert!(ui.bounds(empty.message).unwrap().h > 0);
    let hit = ui.node_at(8, 8).unwrap();
    assert_eq!(hit, icon_a, "Icon topmost hit");
}

#[test]
fn scrollview_clamps_drag_and_updates_bounds() {
    let mut ui = Ui::<32>::new(200, 100);
    let context = ctx(200, 100);
    let root = ui.root();
    let mut scroll = ScrollView::new(&mut ui, root, &context, 80).unwrap();
    ui.add(scroll.content, rect(Color { r: 1, g: 2, b: 3 }, Some(180), Some(150))).unwrap();
    scroll.sync_bounds(&mut ui).unwrap();
    assert_eq!(scroll.offset(), 0);
    assert!(scroll
        .on_msg(&mut ui, &Msg::DragStart { node: scroll.content, axis: berry_ui::sdk::Axis::Y, x: 20, y: 100 })
        .unwrap());
    assert!(scroll
        .on_msg(&mut ui, &Msg::DragMove { node: scroll.content, x: 20, y: 80, resampled: false })
        .unwrap());
    assert_eq!(scroll.offset(), 20);
    assert_eq!(ui.bounds(scroll.content).unwrap().y, -20);
    assert!(scroll
        .on_msg(&mut ui, &Msg::DragMove { node: scroll.content, x: 20, y: 0, resampled: false })
        .unwrap());
    assert_eq!(scroll.offset(), 70, "content 150 - viewport 80");
    assert_eq!(ui.bounds(scroll.content).unwrap().y, -70);
    assert!(scroll
        .on_msg(&mut ui, &Msg::DragEnd { node: scroll.content, vx: 0, vy: 0 })
        .unwrap());
}

#[test]
fn widget_text_over_cap_is_loud() {
    let mut ui = Ui::<12>::new(200, 200);
    let root = ui.root();
    let context = ctx(200, 200);
    let long = [b'x'; 65];
    assert_eq!(
        text(&mut ui, root, &context, TextVariant::Body, &long),
        Err(UiError::TextTooLong { len: 65 })
    );
}

#[test]
fn widget_geometry_scales_from_compact_to_product() {
    let mut compact_ui = Ui::<32>::new(800, 600);
    let mut product_ui = Ui::<32>::new(1080, 1920);
    let compact_ctx = ctx(800, 600);
    let product_ctx = ctx(1080, 1920);
    let compact_root = compact_ui.root();
    let product_root = product_ui.root();
    let compact = Scaffold::new(&mut compact_ui, compact_root, &compact_ctx, b"Settings").unwrap();
    let product = Scaffold::new(&mut product_ui, product_root, &product_ctx, b"Settings").unwrap();
    let compact_switch = Switch::new(&mut compact_ui, compact.body, &compact_ctx, false, false).unwrap();
    let product_switch = Switch::new(&mut product_ui, product.body, &product_ctx, false, false).unwrap();
    compact_ui.layout();
    product_ui.layout();

    assert_eq!(compact_ui.bounds(compact.app_bar.panel).unwrap(), RectI::new(0, 0, 800, 56));
    assert_eq!(product_ui.bounds(product.app_bar.panel).unwrap(), RectI::new(0, 0, 1080, 60));
    assert_eq!(compact_ui.bounds(compact.body).unwrap(), RectI::new(0, 56, 800, 544));
    assert_eq!(product_ui.bounds(product.body).unwrap(), RectI::new(0, 60, 1080, 1860));
    assert_eq!(
        (compact_ui.bounds(compact_switch.panel).unwrap().w, compact_ui.bounds(compact_switch.panel).unwrap().h),
        (52, 28)
    );
    assert_eq!(
        (product_ui.bounds(product_switch.panel).unwrap().w, product_ui.bounds(product_switch.panel).unwrap().h),
        (78, 42)
    );
}
