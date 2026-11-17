//! PullRefresh 值级钉板：状态机、几何揭示、阻尼、无时钟推进与全分发中立。

use berry_gfx::cmd::Color;
use berry_ui::desc::{Axis, Msg, NodeId, Props, RectProps, ViewKind, ViewSpec};
use berry_ui::sdk::{
    PullRefresh, PullRefreshEvent, PullRefreshPhase, PullRefreshSpec, RectI, Scaffold,
    ScrollView, Theme, Ui, UiError, WidgetContext,
};

fn ctx(w: u32, h: u32) -> WidgetContext {
    WidgetContext::for_viewport(Theme::light(), w, h)
}

fn rect(_parent: NodeId, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps {
            bg: Color { r: 20, g: 30, b: 40 },
            w: None,
            h: Some(h),
            radius: 0,
        }),
    }
}

fn fixture() -> (Ui<64>, ScrollView, PullRefresh, WidgetContext) {
    let context = ctx(500, 900);
    let mut ui = Ui::<64>::new(500, 900);
    let root = ui.root();
    let mut scroll = ScrollView::new(&mut ui, root, &context, 800).unwrap();
    ui.add(scroll.content, rect(scroll.content, 1000)).unwrap();
    scroll.sync_bounds(&mut ui).unwrap();
    let mut refresh =
        PullRefresh::attach(&mut ui, &scroll, &context, &PullRefreshSpec::default()).unwrap();
    refresh.sync(&mut ui, &scroll).unwrap();
    (ui, scroll, refresh, context)
}

#[test]
fn pull_refresh_parks_indicator_behind_clip_and_uses_intrinsic_trigger() {
    let (mut ui, scroll, refresh, _) = fixture();
    ui.layout();
    let port = ui.bounds(scroll.port).unwrap();
    let host = ui.bounds(refresh.host()).unwrap();
    let zone = host.h.max(0) as u32;
    assert!(zone > 0);
    assert_eq!(host.y, port.y - zone as i32);
    assert_eq!(refresh.phase(), PullRefreshPhase::Idle);
}

#[test]
fn pull_refresh_drag_is_geometric_damped_and_capped() {
    let (mut ui, scroll, mut refresh, context) = fixture();
    ui.layout();
    let port = ui.bounds(scroll.port).unwrap();
    let x = port.x + port.w / 2;
    let y0 = port.y + 80;

    // Ownership is geometric: node may be a list row, not one of ScrollView's
    // three structural nodes.
    let row = scroll.panel;
    assert_eq!(
        refresh.on_msg(
            &mut ui,
            &context,
            &scroll,
            &Msg::DragStart { node: row, axis: Axis::Y, x: x as u32, y: y0 as u32 },
        )
        .unwrap(),
        PullRefreshEvent::None
    );
    let zone = ui.bounds(refresh.host()).unwrap().h.max(0) as u32;
    let dy = zone.saturating_mul(4);
    let expected_pull = (dy / 2).min(zone * 2);
    assert_eq!(
        refresh.on_msg(
            &mut ui,
            &context,
            &scroll,
            &Msg::DragMove { node: row, x: x as u32, y: (y0 + expected_pull as i32 * 2) as u32, resampled: false },
        )
        .unwrap(),
        PullRefreshEvent::PullChanged {
            pull_px: expected_pull,
            armed: true,
        }
    );
    ui.layout();
    assert_eq!(ui.bounds(scroll.content).unwrap().y, expected_pull as i32);
    assert_eq!(
        refresh.phase(),
        PullRefreshPhase::Pulling {
            pull_px: expected_pull,
            armed: true,
        }
    );

    // Horizontal drag and off-port start never arm.
    let (mut ui, scroll, mut refresh, context) = fixture();
    ui.layout();
    let port = ui.bounds(scroll.port).unwrap();
    assert_eq!(
        refresh.on_msg(
            &mut ui,
            &context,
            &scroll,
            &Msg::DragStart { node: row, axis: Axis::X, x: 10, y: 20 },
        )
        .unwrap(),
        PullRefreshEvent::None
    );
    assert_eq!(
        refresh.on_msg(
            &mut ui,
            &context,
            &scroll,
            &Msg::DragStart { node: row, axis: Axis::Y, x: (port.x + port.w / 2) as u32, y: 2_000 },
        )
        .unwrap(),
        PullRefreshEvent::None
    );
}

#[test]
fn armed_release_triggers_then_settles_into_refreshing() {
    let (mut ui, scroll, mut refresh, context) = fixture();
    ui.layout();
    let port = ui.bounds(scroll.port).unwrap();
    let zone = ui.bounds(refresh.host()).unwrap().h.max(0) as u32;
    let row = scroll.panel;
    let x = (port.x + port.w / 2) as u32;
    let y0 = (port.y + 40) as u32;
    refresh
        .on_msg(
            &mut ui,
            &context,
            &scroll,
            &Msg::DragStart { node: row, axis: Axis::Y, x, y: y0 },
        )
        .unwrap();
    refresh
        .on_msg(
            &mut ui,
            &context,
            &scroll,
            &Msg::DragMove { node: row, x, y: y0 + zone * 3, resampled: false },
        )
        .unwrap();
    assert_eq!(
        refresh.on_msg(&mut ui, &context, &scroll, &Msg::DragEnd { node: row, vx: 0, vy: 0 })
            .unwrap(),
        PullRefreshEvent::Triggered
    );
    assert!(matches!(refresh.phase(), PullRefreshPhase::Settling { .. }));
    refresh.tick(&mut ui, &context, 1).unwrap();
    refresh
        .tick(
            &mut ui,
            &context,
            1 + berry_ui::sdk::PULL_REFRESH_SETTLE_NS,
        )
        .unwrap();
    ui.layout();
    assert_eq!(refresh.phase(), PullRefreshPhase::Refreshing);
    assert_eq!(ui.bounds(scroll.content).unwrap().y, zone as i32);

    refresh.finish(&mut ui, &context).unwrap();
    refresh.tick(&mut ui, &context, 10_000_000_000).unwrap();
    refresh
        .tick(
            &mut ui,
            &context,
            10_000_000_000 + berry_ui::sdk::PULL_REFRESH_SETTLE_NS,
        )
        .unwrap();
    ui.layout();
    assert_eq!(refresh.phase(), PullRefreshPhase::Idle);
    assert_eq!(ui.bounds(scroll.content).unwrap().y, 0);
}

#[test]
fn below_threshold_cancel_and_full_dispatch_are_neutral() {
    let (mut ui, scroll, mut refresh, context) = fixture();
    ui.layout();
    let port = ui.bounds(scroll.port).unwrap();
    let row = scroll.panel;
    let x = (port.x + port.w / 2) as u32;
    let y = (port.y + 30) as u32;
    refresh
        .on_msg(
            &mut ui,
            &context,
            &scroll,
            &Msg::DragStart { node: row, axis: Axis::Y, x, y },
        )
        .unwrap();
    refresh
        .on_msg(
            &mut ui,
            &context,
            &scroll,
            &Msg::DragMove { node: row, x, y: y + 4, resampled: false },
        )
        .unwrap();
    assert_eq!(
        refresh.on_msg(&mut ui, &context, &scroll, &Msg::Cancel { node: row })
            .unwrap(),
        PullRefreshEvent::None
    );
    refresh.tick(&mut ui, &context, 1).unwrap();
    refresh
        .tick(&mut ui, &context, 1 + berry_ui::sdk::PULL_REFRESH_SETTLE_NS)
        .unwrap();
    assert_eq!(refresh.phase(), PullRefreshPhase::Idle);

    for msg in [
        Msg::Press { node: row, x, y },
        Msg::Release { node: row, x, y },
        Msg::Click { node: row },
        Msg::FocusIn { node: row },
        Msg::FocusOut { node: row },
    ] {
        assert_eq!(
            refresh.on_msg(&mut ui, &context, &scroll, &msg).unwrap(),
            PullRefreshEvent::None
        );
    }
    assert_eq!(refresh.phase(), PullRefreshPhase::Idle);
}

#[test]
fn pull_refresh_rejects_horizontal_and_stale_scrolls() {
    let context = ctx(500, 900);
    let mut ui = Ui::<64>::new(500, 900);
    let root = ui.root();
    let mut scroll = berry_ui::sdk::ScrollView::new_axis(
        &mut ui,
        root,
        &context,
        400,
        berry_ui::sdk::ScrollViewAxis::Horizontal,
    )
    .unwrap();
    scroll.sync_bounds(&mut ui).unwrap();
    assert_eq!(
        PullRefresh::attach(
            &mut ui,
            &scroll,
            &context,
            &PullRefreshSpec::default()
        )
        .unwrap_err(),
        UiError::NodeBad
    );

    // A stale tree must not panic in app loops.
    let (mut ui, scroll, mut refresh, context) = fixture();
    let root = ui.root();
    ui.remove(root).unwrap();
    assert_eq!(
        refresh.on_msg(
            &mut ui,
            &context,
            &scroll,
            &Msg::DragStart { node: scroll.panel, axis: Axis::Y, x: 10, y: 10 },
        )
        .unwrap_err(),
        UiError::NodeBad
    );
}

// Keep the product scaffold import exercised without constructing an extra UI.
#[test]
fn pull_refresh_test_context_matches_product_scaffold_geometry() {
    let context = ctx(500, 900);
    let mut ui = Ui::<32>::new(500, 900);
    let root = ui.root();
    let scaffold = Scaffold::new_light_safe(
        &mut ui,
        root,
        &context,
        b"Test",
        20,
        30,
    )
    .unwrap();
    ui.layout();
    assert_eq!(
        ui.bounds(scaffold.body).unwrap(),
        RectI::new(0, 132, 500, 738)
    );
}
