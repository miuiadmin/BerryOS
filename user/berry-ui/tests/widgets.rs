//! GUI F4：组件 API 的布局、状态、a11y、命中与 AST round-trip 钉板。
//!
//! 只经 `berry_ui::sdk` 消费组件；组件不得暴露 WM/VMO/像素路径。所有
//! 几何断言来自 layout dump/bounds，所有交互断言来自 Msg/NodeId。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos, ImageSrc};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_ui::sdk::{
    column, divider, icon, row, text, Button, Card, EmptyState, InteractionState, ListTile,
    ListTileSpec, ListItem, Msg, NavigationIconSource, NAV_ICON_BACK, NAV_ICON_BACK_ACTIVE,
    AlertDialog, Checkbox, DialogAction, Grid, LoadingIndicator, Menu, MenuItemSpec,
    ProgressIndicator, Radio, Section, Sheet, Slider, TabBarStyle, TabItemSpec, Tabs, Toast,
    PanelProps, Props, RectProps, RectI, Scaffold, ScrollView,
    Switch, TextVariant, Theme, Ui, UiError, ViewKind, ViewSpec, WidgetContext,
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

static NAVIGATION_PAINT_SOURCE: NavigationIconSource = NavigationIconSource;

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
    assert!(!item
        .on_msg(
            &mut ui,
            &context,
            &Msg::Press { node: item.title, x: 1, y: 1 },
        )
        .unwrap());
    assert!(item
        .on_msg(&mut ui, &context, &Msg::Click { node: item.panel })
        .unwrap());
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
fn app_bar_back_is_real_48dp_touch_target() {
    let mut ui = Ui::<32>::new(1080, 1920);
    let context = ctx(1080, 1920);
    let root = ui.root();
    ui.set_icon_source(&NAVIGATION_PAINT_SOURCE);
    let scaffold = Scaffold::with_back(
        &mut ui,
        root,
        &context,
        b"Buttons",
        NAV_ICON_BACK,
        NAV_ICON_BACK_ACTIVE,
    )
    .unwrap();
    ui.layout();

    assert_eq!(
        ui.bounds(scaffold.app_bar.panel).unwrap(),
        RectI::new(0, 0, 1080, 168),
        "Product 56dp top bar"
    );
    let mut back = scaffold.app_bar.back.expect("navigation back");
    assert_eq!(
        ui.bounds(back.panel).unwrap(),
        RectI::new(0, 12, 144, 144),
        "48dp target centered in 56dp bar"
    );
    assert_eq!(
        ui.bounds(scaffold.body).unwrap(),
        RectI::new(0, 168, 1080, 1752)
    );
    assert_eq!(ui.node_at(1, 13), Some(back.panel));
    assert!(ui.a11y_iter().any(|entry| {
        entry.role == berry_ui::ROLE_BUTTON && entry.label.as_bytes() == b"Back"
    }));

    assert!(!back
        .on_msg(
            &mut ui,
            &context,
            &Msg::Press { node: back.panel, x: 1, y: 13 },
        )
        .unwrap());
    assert!(back
        .on_msg(&mut ui, &context, &Msg::Click { node: back.panel })
        .unwrap());

    ui.drain_damage();
    ui.wake(0);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 128];
    let mut encoder = CommandEncoder::new();
    ui.paint(&mut scratch, &mut encoder);
    let commands = encoder.finish();
    assert!(
        commands.iter().any(|command| matches!(
            command,
            Cmd::DrawImage {
                src: ImageSrc::Argb { w, h, px },
                ..
            } if *w == 72 && *h == 72 && px.len() == 72 * 72
        )),
        "AppBar back must paint the shared 72×72 navigation glyph"
    );
}

#[test]
fn tri_platform_selection_progress_and_tab_controls() {
    let mut ui = Ui::<256>::new(1080, 1920);
    let context = ctx(1080, 1920);
    let root = ui.root();

    let mut checkbox = Checkbox::new(&mut ui, root, &context, false, false).unwrap();
    ui.layout();
    let checkbox_bounds = ui.bounds(checkbox.panel).unwrap();
    assert!(checkbox_bounds.w >= 48 && checkbox_bounds.w <= 72);
    assert!(!checkbox.on_msg(
        &mut ui,
        &context,
        &Msg::Press { node: checkbox.panel, x: 1, y: 1 }
    ).unwrap());
    assert!(!checkbox.on_msg(
        &mut ui,
        &context,
        &Msg::Release { node: checkbox.panel, x: 1, y: 1 }
    ).unwrap());
    assert!(checkbox.on_msg(
        &mut ui,
        &context,
        &Msg::Click { node: checkbox.panel }
    ).unwrap());
    assert!(checkbox.is_checked());

    let mut radio = Radio::new(&mut ui, root, &context, true, false, b"Berry").unwrap();
    ui.layout();
    assert!(radio.is_checked());
    assert!(ui.bounds(radio.dot).unwrap().w > 0);
    assert!(radio
        .on_msg(
            &mut ui,
            &context,
            &Msg::Press { node: radio.panel, x: 1, y: 1 },
        )
        .is_ok());

    let mut slider = Slider::new(&mut ui, root, &context, 0, false, b"Brightness").unwrap();
    ui.layout();
    assert_eq!(slider.permille(), 0);
    slider
        .set_permille(&mut ui, &context, 500)
        .unwrap();
    ui.layout();
    let track = ui.bounds(slider.track).unwrap();
    let thumb = ui.bounds(slider.thumb).unwrap();
    assert_eq!(track.w, 216);
    assert!(thumb.x >= track.x && thumb.x <= track.x + track.w);

    let mut progress = ProgressIndicator::new(&mut ui, root, &context, 300, 250).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(progress.fill).unwrap().w, 75);

    let loading = LoadingIndicator::new(&mut ui, root, &context).unwrap();
    loading.set_phase(&mut ui, &context, 1).unwrap();
    ui.layout();
    let loading_bounds = ui.bounds(loading.panel).unwrap();
    assert!(loading_bounds.w >= 24 && loading_bounds.w <= 72);

    let items = [
        TabItemSpec { label: b"Home", icon: None },
        TabItemSpec { label: b"Apps", icon: None },
        TabItemSpec { label: b"Settings", icon: None },
    ];
    let mut tabs = Tabs::new(
        &mut ui,
        root,
        &context,
        TabBarStyle::Bottom,
        &items,
        0,
    )
    .unwrap();
    ui.layout();
    assert_eq!(tabs.selected(), 0);
    for index in 0..items.len() {
        let bounds = ui.bounds(tabs.item_panel(index).unwrap()).unwrap();
        assert_eq!(bounds.w, 360, "tab {index} cell width");
        assert_eq!(bounds.x, 360 * index as i32, "tab {index} x");
    }
    let second_tab = tabs.item_panel(1).unwrap();
    assert!(!tabs
        .on_msg(
            &mut ui,
            &context,
            &Msg::Press { node: second_tab, x: 540, y: 1740 },
        )
        .unwrap());
    assert!(!tabs
        .on_msg(
            &mut ui,
            &context,
            &Msg::Release { node: second_tab, x: 540, y: 1740 },
        )
        .unwrap());
    assert!(tabs
        .on_msg(&mut ui, &context, &Msg::Click { node: second_tab })
        .unwrap());
    assert_eq!(tabs.selected(), 1);

    let section = Section::new(&mut ui, root, &context, b"Network").unwrap();
    ui.layout();
    assert!(ui.bounds(section.header).unwrap().h > 0);
    assert!(ui.bounds(section.content).unwrap().h >= 0);

    assert!(Grid::new(&mut ui, root, 0, 2, 4).is_err());
    let grid = Grid::new(&mut ui, root, 3, 2, 4).unwrap();
    assert!(ui.bounds(grid.content).unwrap().w >= 0);
}

#[test]
fn safe_navigation_shell_reserves_product_system_bars() {
    let mut ui = Ui::<64>::new(1080, 1920);
    let context = ctx(1080, 1920);
    let root = ui.root();
    let scaffold = Scaffold::with_back_safe(
        &mut ui,
        root,
        &context,
        b"Settings",
        103,
        104,
        117,
        108,
    )
    .unwrap();
    ui.layout();

    assert_eq!(
        ui.bounds(scaffold.app_bar.panel).unwrap(),
        RectI::new(0, 117, 1080, 168),
        "App bar begins below the 39vp status reserve"
    );
    assert_eq!(
        ui.bounds(scaffold.body).unwrap(),
        RectI::new(0, 285, 1080, 1527),
        "Body ends at the 36vp navigation reserve"
    );
    assert!(scaffold.app_bar.back.is_some());

    let mut light = Scaffold::new_light_safe(
        &mut ui,
        root,
        &context,
        b"Gallery",
        117,
        108,
    )
    .unwrap();
    ui.layout();
    assert_eq!(
        ui.bounds(light.app_bar.panel).unwrap(),
        RectI::new(0, 117, 1080, 168)
    );
    assert_eq!(
        ui.bounds(light.body).unwrap(),
        RectI::new(0, 285, 1080, 1527)
    );
    assert!(light.app_bar.back.is_none());
}

#[test]
fn overlay_components_have_real_z_order_and_actions() {
    let mut ui = Ui::<256>::new(1080, 1920);
    let context = ctx(1080, 1920);
    let root = ui.root();

    let mut dialog = AlertDialog::new(
        &mut ui,
        root,
        &context,
        b"Delete item",
        b"This action cannot be undone.",
        b"Delete",
        Some(b"Cancel"),
    )
    .unwrap();
    ui.layout();
    let bounds = ui.bounds(dialog.surface.content).unwrap();
    assert_eq!(bounds.x, (1080 - bounds.w) / 2);
    assert_eq!(bounds.y, (1920 - bounds.h) / 2);
    assert_eq!(
        dialog
            .on_msg(
                &mut ui,
                &context,
                &Msg::Click { node: dialog.confirm.panel },
            )
            .unwrap(),
        DialogAction::Confirm
    );
    dialog.dismiss(&mut ui).unwrap();
    ui.layout();

    let mut toast = Toast::new(&mut ui, root, &context, b"Saved", Some(b"Undo")).unwrap();
    ui.layout();
    let toast_bounds = ui.bounds(toast.root).unwrap();
    assert_eq!(toast_bounds.x, (1080 - toast_bounds.w) / 2);
    assert!(toast_bounds.y > 1500 && toast_bounds.y < 1920);
    assert!(toast
        .on_msg(
            &mut ui,
            &context,
            &Msg::Click { node: toast.action.unwrap().panel },
        )
        .unwrap());
    toast.dismiss(&mut ui).unwrap();
    ui.layout();

    let items = [
        MenuItemSpec { label: b"Rename", dangerous: false },
        MenuItemSpec { label: b"Delete", dangerous: true },
    ];
    let mut menu = Menu::new(&mut ui, root, &context, 120, 320, &items).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(menu.root).unwrap().x, 120);
    assert_eq!(
        menu.on_msg(
            &mut ui,
            &context,
            &Msg::Click { node: menu.root },
        )
        .unwrap(),
        None
    );
    let second_item = menu.item_panel(1).unwrap();
    assert_eq!(
        menu.on_msg(
            &mut ui,
            &context,
            &Msg::Press { node: second_item, x: 150, y: 350 },
        )
        .unwrap(),
        None
    );
    assert_eq!(
        menu.on_msg(
            &mut ui,
            &context,
            &Msg::Release { node: second_item, x: 150, y: 350 },
        )
        .unwrap(),
        None
    );
    assert_eq!(
        menu.on_msg(
            &mut ui,
            &context,
            &Msg::Click { node: second_item },
        )
        .unwrap(),
        Some(1)
    );
    menu.dismiss(&mut ui).unwrap();

    let sheet = Sheet::new(&mut ui, root, &context, 600).unwrap();
    ui.layout();
    let sheet_bounds = ui.bounds(sheet.root).unwrap();
    assert_eq!((sheet_bounds.x, sheet_bounds.y, sheet_bounds.h), (0, 1320, 600));
    sheet.dismiss(&mut ui).unwrap();
}

#[test]
fn list_tile_is_card_row_with_leading_media() {
    let mut ui = Ui::<32>::new(1080, 1920);
    let context = ctx(1080, 1920);
    let root = ui.root();
    let spec = ListTileSpec {
        title: b"Buttons & states",
        detail: Some(b"Press, focus, disable, count"),
        leading_icon: 100,
        trailing: b">",
        selected: false,
        width: 1056,
    };
    let mut tile = ListTile::new(&mut ui, root, &context, &spec).unwrap();
    ui.layout();

    assert_eq!(ui.bounds(tile.panel).unwrap(), RectI::new(0, 0, 1056, 144));
    assert_eq!(ui.bounds(tile.leading).unwrap(), RectI::new(12, 48, 48, 48));
    assert_eq!(ui.bounds(tile.title).unwrap().x, 84);
    assert_eq!(ui.bounds(tile.trailing).unwrap().x, 1024);
    assert!(ui.a11y_iter().any(|entry| {
        entry.role == berry_ui::ROLE_LIST_ITEM
            && entry.label.as_bytes() == b"Buttons & states"
    }));
    assert!(!tile
        .on_msg(
            &mut ui,
            &context,
            &Msg::Press { node: tile.title, x: 100, y: 20 },
        )
        .unwrap());
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
fn scrollview_clips_overflow_paint_and_hit() {
    let mut ui = Ui::<32>::new(200, 100);
    let context = ctx(200, 100);
    let root = ui.root();
    let mut scroll = ScrollView::new(&mut ui, root, &context, 80).unwrap();
    let content = ui
        .add(scroll.content, rect(Color { r: 20, g: 40, b: 220 }, Some(180), Some(150)))
        .unwrap();
    scroll.sync_bounds(&mut ui).unwrap();
    scroll.scroll_by(&mut ui, 30).unwrap();
    assert_eq!(ui.bounds(content).unwrap(), RectI::new(0, -30, 180, 150));

    // 命中链：Clip 内命中内容；port 下方虽仍在内容未裁剪 bounds 内，
    // 但不可见，不得命中滚出项。
    assert_eq!(ui.node_at(50, 70), Some(content));
    assert_eq!(ui.node_at(50, 90), None);

    // 像素链：port 内蓝、port 外哨兵，证明 Push/Pop 裁剪真实执行。
    ui.wake(0);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let report = ui.paint(&mut scratch, &mut enc);
    let dl = enc.finish();
    assert_eq!(
        report.drawn_px,
        200 * 80 + 180 * 80,
        "Panel 背景 + 内容有效 clip 足迹"
    );
    let desc = TextureDesc::new(200, 100, 800, TextureFormat::Rgbx).unwrap();
    let mut fb = vec![0xEEu8; desc.need_bytes()];
    Queue::new()
        .submit(
            &dl,
            &mut fb,
            &desc,
            &PassDescriptor::new(LoadOp::Load, StoreOp::Store, None),
        )
        .unwrap();
    let blue = TextureFormat::Rgbx.pack(20, 40, 220);
    let at = |x: usize, y: usize| -> u32 {
        let off = y * 800 + x * 4;
        u32::from_le_bytes([fb[off], fb[off + 1], fb[off + 2], fb[off + 3]])
    };
    assert_eq!(at(50, 75), blue, "port 内内容可见");
    assert_eq!(at(50, 85), 0xEEEE_EEEE, "滚出 port 的内容不得泄漏");
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
