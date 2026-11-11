//! GUI-14 C1 component focus host tests：树前序、环绕、禁用、生命周期、
//! widget FocusIn/Out 视觉 damage 与 sdk keymap。

use berry_gfx::cmd::{CommandEncoder, GlyphPos};
use berry_guiproto::{KEY_ACTION_DOWN, KEY_ACTION_REPEAT, KEY_ACTION_UP, KEY_MOD_SHIFT};
use berry_ui::desc::{FocusDir, Msg, PanelProps, ViewKind, ViewSpec};
use berry_ui::sdk::{
    focus_key_intent, Button, FocusOp, InteractionState, WidgetContext,
};
use berry_ui::{NodeId, Ui, UiError};
use berry_gfx::cmd::Color as GfxColor;

fn panel(ui: &mut Ui<64>, parent: NodeId) -> NodeId {
    ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Panel,
            key: 0,
            props: berry_ui::Props::Panel(PanelProps {
                bg: GfxColor { r: 0xFF, g: 0xFF, b: 0xFF },
                w: Some(40),
                h: Some(20),
                padding: 2,
                gap: 0,
                radius: 0,
            }),
        },
    )
    .unwrap()
}

fn msgs(ui: &mut Ui<64>, dir: FocusDir) -> Vec<Msg> {
    ui.focus_move(dir).iter().copied().collect()
}

#[test]
fn focus_order_is_tree_preorder_not_registration_order() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let a = panel(&mut ui, root);
    let b = panel(&mut ui, root);
    let c = panel(&mut ui, root);

    // Registration order is deliberately scrambled: C, A, B-disabled.
    ui.set_focusable(c, Some(c), true).unwrap();
    ui.set_focusable(a, Some(a), true).unwrap();
    ui.set_focusable(b, Some(b), false).unwrap();

    assert_eq!(msgs(&mut ui, FocusDir::Next), vec![Msg::FocusIn { node: a }]);
    assert_eq!(
        msgs(&mut ui, FocusDir::Next),
        vec![
            Msg::FocusOut { node: a },
            Msg::FocusIn { node: c }
        ]
    );
    // Wrap is deliberate for 1D Tab; B-disabled is skipped.
    assert_eq!(
        msgs(&mut ui, FocusDir::Next),
        vec![
            Msg::FocusOut { node: c },
            Msg::FocusIn { node: a }
        ]
    );
    assert_eq!(
        msgs(&mut ui, FocusDir::Prev),
        vec![
            Msg::FocusOut { node: a },
            Msg::FocusIn { node: c }
        ]
    );
    assert_eq!(ui.focus_node(), Some(c));
    assert_eq!(ui.a11y_state(c), None);
}

#[test]
fn focus_lifecycle_and_rejects() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let a = panel(&mut ui, root);
    let b = panel(&mut ui, root);
    ui.set_focusable(a, Some(a), true).unwrap();
    ui.set_focusable(b, Some(b), true).unwrap();

    let out = ui.focus_set(a).unwrap();
    assert_eq!(out.iter().copied().collect::<Vec<_>>(), vec![Msg::FocusIn { node: a }]);
    assert_eq!(ui.focus_set(a).unwrap().len(), 0);
    assert!(ui.focus_set(root).unwrap().is_empty());

    ui.remove(b).unwrap();
    assert!(ui.set_focusable(b, Some(b), true).is_err());
    ui.remove(a).unwrap();
    assert_eq!(ui.focus_node(), None);
    assert!(ui.focus_iter().next().is_none());

    // A whole-tree remove/reset path clears the current component focus.
    let c = panel(&mut ui, root);
    ui.set_focusable(c, Some(c), true).unwrap();
    ui.focus_set(c).unwrap();
    ui.remove(root).unwrap();
    assert_eq!(ui.focus_node(), None);
    assert!(ui.set_focusable(c, Some(c), true).is_err());
}

#[test]
fn focus_table_capacity_is_loud() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let mut nodes = Vec::with_capacity(32);
    for _ in 0..32 {
        let node = panel(&mut ui, root);
        ui.set_focusable(node, Some(node), true).unwrap();
        nodes.push(node);
    }
    let extra = panel(&mut ui, root);
    assert_eq!(
        ui.set_focusable(extra, Some(extra), true),
        Err(UiError::NodeFull)
    );
    assert!(ui.take_focusable(nodes[0]));
    assert_eq!(ui.set_focusable(extra, Some(extra), true), Ok(()));
}

#[test]
fn widget_focus_messages_paint_ring_and_a11y_state() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let ctx = WidgetContext::for_viewport(crate_theme(), 320, 240);
    let stack = berry_ui::sdk::column(&mut ui, root, &ctx).unwrap();
    let mut first = Button::new(&mut ui, stack, &ctx, b"One", InteractionState::Normal).unwrap();
    let mut second = Button::new(&mut ui, stack, &ctx, b"Two", InteractionState::Normal).unwrap();
    ui.layout();
    drain_damage(&mut ui);

    let events = ui.focus_set(first.panel).unwrap();
    assert_eq!(
        events.iter().copied().collect::<Vec<_>>(),
        vec![Msg::FocusIn { node: first.panel }]
    );
    for event in events.iter() {
        first.on_msg(&mut ui, &ctx, event).unwrap();
    }
    assert_eq!(ui.a11y_state(first.panel), Some(0b0000_0011));
    assert_eq!(ui.damage().count, 1);
    // Button::on_msg(FocusIn) 的 set_props 路径独立入账；这不是
    // Ui::set_focus_internal 的 engine invalidate 假绿。
    assert!(ui.damage().adds >= 2);

    drain_damage(&mut ui);
    assert_eq!(ui.damage().adds, 0);
    first.set_focused(&mut ui, &ctx, true).unwrap();
    assert!(ui.damage().adds >= 1);
    drain_damage(&mut ui);
    let events = ui.focus_move(FocusDir::Next);
    assert_eq!(
        events.iter().copied().collect::<Vec<_>>(),
        vec![
            Msg::FocusOut { node: first.panel },
            Msg::FocusIn { node: second.panel }
        ]
    );
    for event in events.iter() {
        first.on_msg(&mut ui, &ctx, event).unwrap();
        second.on_msg(&mut ui, &ctx, event).unwrap();
    }
    assert_eq!(ui.a11y_state(first.panel), Some(0b0000_0001));
    assert_eq!(ui.a11y_state(second.panel), Some(0b0000_0011));
    let first_bounds = ui.bounds(first.panel).unwrap();
    let second_bounds = ui.bounds(second.panel).unwrap();
    let envelope = first_bounds.envelope(&second_bounds).to_wire();
    assert_eq!(ui.damage().count, 1);
    assert_eq!(ui.damage().adds, 6);
    assert_eq!(ui.damage().rects[0], envelope);
}

#[test]
fn disabled_widget_drops_and_does_not_restore_focus() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let ctx = WidgetContext::for_viewport(crate_theme(), 320, 240);
    let stack = berry_ui::sdk::column(&mut ui, root, &ctx).unwrap();
    let mut button = Button::new(
        &mut ui, stack, &ctx, b"Go", InteractionState::Normal,
    )
    .unwrap();
    ui.layout();
    ui.focus_set(button.panel).unwrap();
    assert_eq!(ui.focus_node(), Some(button.panel));

    button
        .set_base(&mut ui, &ctx, InteractionState::Disabled)
        .unwrap();
    assert_eq!(ui.focus_node(), None);
    assert_eq!(ui.a11y_state(button.panel), Some(0b0000_0001));

    button
        .set_base(&mut ui, &ctx, InteractionState::Normal)
        .unwrap();
    assert_eq!(ui.focus_node(), None);
}

#[test]
fn pointer_click_does_not_move_component_focus() {
    use berry_guiproto::{PHASE_DOWN, PHASE_UP};
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let ctx = WidgetContext::for_viewport(crate_theme(), 320, 240);
    let stack = berry_ui::sdk::column(&mut ui, root, &ctx).unwrap();
    let mut button = Button::new(
        &mut ui, stack, &ctx, b"Go", InteractionState::Normal,
    )
    .unwrap();
    ui.layout();
    let bounds = ui.bounds(button.panel).unwrap();
    let x = (x_of(bounds) + 2) as u32;
    let y = (y_of(bounds) + 2) as u32;
    for (phase, ts) in [(PHASE_DOWN, 1u64), (PHASE_UP, 2u64)] {
        ui.push_pointer(berry_ui::hit::PtrEvent::from_wire(
            phase, 0, x, y, ts, 0, 0,
        ))
        .unwrap();
        let messages = ui.pump_input(0);
        for message in messages.iter() {
            button
                .on_msg(&mut ui, &ctx, message)
                .unwrap();
        }
    }
    assert_eq!(ui.focus_node(), None);
}

fn x_of(r: berry_ui::damage::RectI) -> i32 {
    r.x
}

fn y_of(r: berry_ui::damage::RectI) -> i32 {
    r.y
}

#[test]
fn previous_from_no_focus_targets_first_item() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let a = panel(&mut ui, root);
    let b = panel(&mut ui, root);
    ui.set_focusable(b, Some(b), true).unwrap();
    ui.set_focusable(a, Some(a), true).unwrap();
    assert_eq!(
        msgs(&mut ui, FocusDir::Prev),
        vec![Msg::FocusIn { node: a }]
    );
}

#[test]
fn focused_theme_state_is_pinned() {
    let theme = crate_theme();
    let state = theme.state(InteractionState::Focused);
    assert_eq!(
        (
            state.background.r,
            state.background.g,
            state.background.b,
            state.content.r,
            state.content.g,
            state.content.b,
            state.border.r,
            state.border.g,
            state.border.b,
        ),
        (0xE9, 0xF2, 0xFF, 0x0B, 0x25, 0x54, 0x00, 0x74, 0xE0)
    );
}

#[test]
fn focus_key_intent_is_edge_sensitive() {
    assert_eq!(focus_key_intent(15, KEY_ACTION_DOWN, 0), Some(FocusOp::Next));
    assert_eq!(
        focus_key_intent(15, KEY_ACTION_DOWN, KEY_MOD_SHIFT),
        Some(FocusOp::Prev)
    );
    assert_eq!(focus_key_intent(15, KEY_ACTION_UP, 0), None);
    assert_eq!(focus_key_intent(15, KEY_ACTION_REPEAT, 0), None);
    assert_eq!(focus_key_intent(28, KEY_ACTION_DOWN, 0), Some(FocusOp::Activate));
    assert_eq!(focus_key_intent(57, KEY_ACTION_REPEAT, 0), Some(FocusOp::Activate));
    assert_eq!(focus_key_intent(30, KEY_ACTION_DOWN, 0), None);
}

fn drain_damage(ui: &mut Ui<64>) {
    ui.wake(0);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut encoder = CommandEncoder::new();
    let _ = ui.render_frame(0, &mut scratch, &mut encoder);
    let _ = encoder.finish();
}

fn crate_theme() -> berry_ui::sdk::Theme {
    berry_ui::sdk::Theme::light()
}
