use berry_ui::sdk::{
    ContextMenu, ContextMenuOutcome, Menu, MenuItemSpec, Msg, RectI, Theme, Ui, UiError,
    WidgetContext,
};

fn ctx(w: u32, h: u32) -> WidgetContext {
    WidgetContext::for_viewport(Theme::light(), w, h)
}

#[test]
fn context_menu_is_clamped_selectable_and_dismissable() {
    let context = ctx(1080, 1920);
    let mut ui = Ui::<96>::new(1080, 1920);
    let _root = ui.root();
    let items = [
        MenuItemSpec { label: b"Open", dangerous: false },
        MenuItemSpec { label: b"Delete", dangerous: true },
    ];

    assert!(Menu::anchored_size(&context, 0).is_err());
    assert!(Menu::anchored_size(&context, 7).is_err());
    let (width, height) = Menu::anchored_size(&context, 2).unwrap();
    assert_eq!((width, height), (270, 108));

    let mut menu = ContextMenu::new(&mut ui, &context, -100, -100, &items).unwrap();
    ui.layout();
    let bounds = ui.bounds(menu.menu.root).unwrap();
    assert_eq!(bounds.x, 12);
    assert_eq!(bounds.y, 12);
    assert_eq!(bounds.w, 270);
    assert_eq!(bounds.h, 108);
    assert!(ui.bounds(menu.scrim).unwrap() == RectI::new(0, 0, 1080, 1920));

    let second = menu.menu.item_panel(1).unwrap();
    assert_eq!(
        menu.on_msg(
            &mut ui,
            &context,
            &Msg::Press { node: second, x: 100, y: 50 },
        )
        .unwrap(),
        ContextMenuOutcome::None
    );
    assert_eq!(
        menu.on_msg(
            &mut ui,
            &context,
            &Msg::Release { node: second, x: 100, y: 50 },
        )
        .unwrap(),
        ContextMenuOutcome::None
    );
    assert_eq!(
        menu.on_msg(&mut ui, &context, &Msg::Click { node: second })
            .unwrap(),
        ContextMenuOutcome::Selected(1)
    );
    assert!(ui.bounds(menu.scrim).is_err());

    let mut outside = ContextMenu::new(&mut ui, &context, 10_000, 10_000, &items).unwrap();
    ui.layout();
    let bounds = ui.bounds(outside.menu.root).unwrap();
    assert!(bounds.x + bounds.w <= 1080);
    assert!(bounds.y + bounds.h <= 1920);
    assert_eq!(
        outside
            .on_msg(&mut ui, &context, &Msg::Click { node: outside.scrim })
            .unwrap(),
        ContextMenuOutcome::Dismissed
    );
    assert!(ui.bounds(outside.scrim).is_err());
}

#[test]
fn context_menu_cancel_closes_without_selecting() {
    let context = ctx(800, 600);
    let mut ui = Ui::<64>::new(800, 600);
    let items = [MenuItemSpec { label: b"Rename", dangerous: false }];
    let mut menu = ContextMenu::new(&mut ui, &context, 20, 20, &items).unwrap();
    ui.layout();
    assert!(menu.menu.item_panel(0).is_ok());
    assert_eq!(
        menu.on_msg(&mut ui, &context, &Msg::Cancel { node: menu.scrim })
            .unwrap(),
        ContextMenuOutcome::Dismissed
    );
    assert_eq!(ui.bounds(menu.scrim).unwrap_err(), UiError::NodeBad);
}
