//! GUI-T1 TextField Foundation host tests：TextState/UTF-8 selection、
//! key route 优先级、组合控件几何/damage/focus 与 AST 零漂。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos};
use berry_gfx::font::{FontSize, GlyphAtlas};
use berry_guiproto::surface::{
    IME_PURPOSE_NUMBER, IME_PURPOSE_PASSWORD, IME_PURPOSE_TEXT,
};
use berry_guiproto::{
    KEY_ACTION_DOWN, KEY_ACTION_REPEAT, KEY_ACTION_UP, KEY_MOD_CTRL, KEY_MOD_SHIFT,
};
use berry_ui::desc::{FocusDir, Msg};
use berry_ui::sdk::{
    key_route, text_key_intent, KeyRoute, TextInputEdit, TextInputKeyOp, TextInputKind,
    TextInputState, TextField, TextFieldSlots, TextFieldSlotsSpec, TextFieldSpec, Theme,
    WidgetContext, FieldMessage, FieldMessageKind,
};
use berry_ui::{Ui, UiError};

fn ctx(w: u32, h: u32) -> WidgetContext {
    WidgetContext::for_viewport(Theme::light(), w, h)
}


struct StaticFontSource;
impl berry_ui::sdk::FontSource for StaticFontSource {
    fn atlas(&self, scale: FontSize) -> Option<&'static dyn GlyphAtlas> {
        Some(scale.atlas())
    }
}

fn field(ui: &mut Ui<96>, parent: berry_ui::NodeId, width: u32) -> TextField {
    TextField::new(
        ui,
        parent,
        &ctx(ui.viewport().0, ui.viewport().1),
        TextFieldSpec { kind: TextInputKind::Text, width },
    )
    .unwrap()
}

#[test]
fn empty_state_operations_are_total() {
    let mut state = TextInputState::new(b"").unwrap();
    assert!(state.is_empty());
    assert_eq!(state.caret_cp(), 0);
    assert!(!state.apply_edit(TextInputEdit::Backspace).changed);
    assert!(!state.apply_edit(TextInputEdit::Delete).changed);
    assert!(!state.apply_edit(TextInputEdit::Left { extend: true }).changed);
    assert!(!state.apply_edit(TextInputEdit::Home { extend: true }).changed);

    let out = state.apply_edit(TextInputEdit::Insert('a'));
    assert!(out.changed && out.selection_changed && !out.submitted);
    assert_eq!(state.value().as_bytes(), b"a");
    assert_eq!(state.caret_cp(), 1);
}

#[test]
fn insert_replaces_selection_and_preserves_utf8_boundaries() {
    let mut state = TextInputState::new("设a置".as_bytes()).unwrap();
    state.select_all();
    let out = state.apply_edit(TextInputEdit::Insert('莓'));
    assert!(out.changed && out.selection_changed);
    assert_eq!(state.value().as_bytes(), "莓".as_bytes());
    assert_eq!(state.len_cp(), 1);
    assert_eq!(state.selection().ordered(), (1, 1));

    state.apply_edit(TextInputEdit::Left { extend: false });
    let out = state.apply_edit(TextInputEdit::Insert('X'));
    assert_eq!(state.value().as_bytes(), "X莓".as_bytes());
    assert!(out.changed);
}

#[test]
fn caret_moves_and_extends_in_codepoint_units() {
    let mut state = TextInputState::new("a设b".as_bytes()).unwrap();
    state.apply_edit(TextInputEdit::Home { extend: false });
    assert_eq!(state.caret_cp(), 0);
    state.apply_edit(TextInputEdit::Right { extend: true });
    state.apply_edit(TextInputEdit::Right { extend: true });
    assert_eq!(state.selection().ordered(), (0, 2));
    assert_eq!(state.value().as_bytes(), "a设b".as_bytes());

    let out = state.apply_edit(TextInputEdit::Left { extend: false });
    assert!(out.selection_changed && !out.changed);
    assert!(state.selection().is_caret());
    assert_eq!(state.caret_cp(), 1);
}

#[test]
fn delete_backward_forward_and_capacity_are_loud() {
    let mut state = TextInputState::new("a设b".as_bytes()).unwrap();
    state.apply_edit(TextInputEdit::Home { extend: false });
    state.apply_edit(TextInputEdit::Delete);
    assert_eq!(state.value().as_bytes(), "设b".as_bytes());
    state.apply_edit(TextInputEdit::End { extend: false });
    state.apply_edit(TextInputEdit::Backspace);
    assert_eq!(state.value().as_bytes(), "设".as_bytes());
    assert_eq!(state.caret_cp(), 1);

    state.apply_edit(TextInputEdit::SelectAll);
    state.apply_edit(TextInputEdit::Delete);
    assert!(state.is_empty());

    let full = "x".repeat(64).into_bytes();
    let mut full = TextInputState::new(&full).unwrap();
    let out = full.apply_edit(TextInputEdit::Insert('x'));
    assert!(out.rejected && !out.changed);
    assert_eq!(full.value().len as usize, 64);
}

#[test]
fn invalid_utf8_and_number_filter_reject() {
    assert_eq!(
        TextInputState::new(&[0xFF, 0xFE]).unwrap_err(),
        UiError::NodeBad
    );
    assert_eq!(
        text_key_intent(30, KEY_ACTION_DOWN, 0, TextInputKind::Number),
        None
    );
    assert_eq!(
        text_key_intent(8, KEY_ACTION_DOWN, 0, TextInputKind::Number),
        Some(TextInputKeyOp::Edit(TextInputEdit::Insert('7')))
    );
    assert_eq!(
        text_key_intent(11, KEY_ACTION_DOWN, 0, TextInputKind::Text),
        Some(TextInputKeyOp::Edit(TextInputEdit::Insert('0')))
    );
    assert_eq!(
        text_key_intent(52, KEY_ACTION_DOWN, KEY_MOD_SHIFT, TextInputKind::Text),
        Some(TextInputKeyOp::Edit(TextInputEdit::Insert('>')))
    );
}

#[test]
fn key_route_pins_text_before_c1_activation() {
    let text = true;
    let non_text = false;
    let kind = TextInputKind::Text;

    assert_eq!(
        key_route(28, KEY_ACTION_DOWN, 0, kind, text),
        KeyRoute::Text(TextInputKeyOp::Submit)
    );
    assert_eq!(
        key_route(57, KEY_ACTION_DOWN, 0, kind, text),
        KeyRoute::Text(TextInputKeyOp::Edit(TextInputEdit::Insert(' ')))
    );
    assert_eq!(
        key_route(15, KEY_ACTION_DOWN, KEY_MOD_SHIFT, kind, text),
        KeyRoute::Focus(berry_ui::sdk::FocusOp::Prev)
    );
    assert_eq!(
        key_route(15, KEY_ACTION_DOWN, 0, kind, text),
        KeyRoute::Focus(berry_ui::sdk::FocusOp::Next)
    );
    assert_eq!(
        key_route(28, KEY_ACTION_DOWN, 0, kind, non_text),
        KeyRoute::Focus(berry_ui::sdk::FocusOp::Activate)
    );
    assert_eq!(
        key_route(30, KEY_ACTION_DOWN, 0, kind, non_text),
        KeyRoute::None
    );
    assert_eq!(key_route(30, KEY_ACTION_UP, 0, kind, text), KeyRoute::None);
}

#[test]
fn purpose_and_ime_inset_are_add_only_mappings() {
    assert_eq!(TextInputKind::Text.ime_purpose(), IME_PURPOSE_TEXT);
    assert_eq!(TextInputKind::Number.ime_purpose(), IME_PURPOSE_NUMBER);
    assert_eq!(TextInputKind::Password.ime_purpose(), IME_PURPOSE_PASSWORD);
    assert_eq!(
        TextInputKind::IME_PURPOSE_MULTILINE,
        berry_guiproto::surface::IME_PURPOSE_MULTILINE
    );
    assert_eq!(berry_ui::sdk::ime_overlap_px(1900, 1920, 0), 0);
    assert_eq!(berry_ui::sdk::ime_overlap_px(1900, 1920, 240), 220);
    assert_eq!(berry_ui::sdk::ime_overlap_px(1600, 1920, 240), 0);
    assert_eq!(berry_ui::sdk::ime_overlap_px(2000, 1920, 240), 240);
}

#[test]
fn widget_layout_focus_damage_and_caret_visibility() {
    let mut ui = Ui::<96>::new(320, 240);
    let root = ui.root();
    let context = ctx(320, 240);
    let stack = berry_ui::sdk::column(&mut ui, root, &context).unwrap();
    let mut input = field(&mut ui, stack, 160);
    let mut state = TextInputState::new(b"ab").unwrap();
    input.sync(&mut ui, &context, &state).unwrap();
    ui.layout();

    let panel = ui.bounds(input.panel).unwrap();
    assert_eq!(panel.w, 160);
    assert!(panel.h >= 24);
    assert_eq!(ui.a11y_state(input.panel), Some(1));
    assert_eq!(ui.focus_node(), None);
    let caret = ui.bounds(input.caret).unwrap();
    assert_eq!(caret.w, 0);

    let events = input.request_focus(&mut ui, &context).unwrap();
    assert_eq!(
        events.iter().copied().collect::<Vec<_>>(),
        vec![Msg::FocusIn { node: input.panel }]
    );
    assert!(ui.damage().adds > 0);
    let caret = ui.bounds(input.caret).unwrap();
    assert!(caret.w > 0);
    assert!(caret.x >= panel.x);
    assert!(caret.x + caret.w <= panel.x + panel.w);
    assert_eq!(ui.a11y_state(input.panel), Some(3));

    state.apply_edit(TextInputEdit::End { extend: false });
    input.sync(&mut ui, &context, &state).unwrap();
    assert_eq!(state.value().as_bytes(), b"ab");
    assert!(ui.damage().adds > 0);
}

#[test]
fn widget_selection_geometry_uses_monospace_codepoints() {
    let mut ui = Ui::<96>::new(320, 240);
    let root = ui.root();
    let context = ctx(320, 240);
    let stack = berry_ui::sdk::column(&mut ui, root, &context).unwrap();
    let mut input = field(&mut ui, stack, 240);
    let mut state = TextInputState::new("a设b".as_bytes()).unwrap();
    state.select_all();
    input.request_focus(&mut ui, &context).unwrap();
    input.sync(&mut ui, &context, &state).unwrap();

    let cell_w = context
        .metrics
        .typography(berry_ui::sdk::TypographyToken::Body)
        .cell_w() as i32;
    let selection = ui.bounds(input.selection).unwrap();
    assert_eq!(selection.w, state.len_cp() as i32 * cell_w);
    assert_eq!(selection.h, context.metrics.typography(berry_ui::sdk::TypographyToken::Body).cell_h() as i32);
}

#[test]
fn password_display_masks_each_codepoint_without_losing_value() {
    let state = TextInputState::new("设a".as_bytes()).unwrap();
    let display = state.display_value(TextInputKind::Password).unwrap();
    assert_eq!(display.as_bytes(), b"**");
    assert_eq!(state.value().as_bytes(), "设a".as_bytes());

    let mut ui = Ui::<96>::new(320, 240);
    let root = ui.root();
    let context = ctx(320, 240);
    let stack = berry_ui::sdk::column(&mut ui, root, &context).unwrap();
    let mut input = TextField::new(
        &mut ui,
        stack,
        &context,
        TextFieldSpec { kind: TextInputKind::Password, width: 160 },
    )
    .unwrap();
    input.sync(&mut ui, &context, &state).unwrap();
    assert_eq!(ui.kind_of(input.value_text).unwrap(), berry_ui::desc::ViewKind::Text);
}

#[test]
fn overflow_scrolls_to_keep_caret_visible() {
    let mut ui = Ui::<96>::new(320, 240);
    let root = ui.root();
    let context = ctx(320, 240);
    let stack = berry_ui::sdk::column(&mut ui, root, &context).unwrap();
    let mut input = field(&mut ui, stack, 96);
    let text = "x".repeat(64).into_bytes();
    let state = TextInputState::new(&text).unwrap();
    input.request_focus(&mut ui, &context).unwrap();
    input.sync(&mut ui, &context, &state).unwrap();

    let port = ui.bounds(input.port).unwrap();
    let caret = ui.bounds(input.caret).unwrap();
    assert!(caret.x >= port.x);
    assert!(caret.x + caret.w <= port.x + port.w);
    assert!(caret.x > port.x);
}

#[test]
fn textfield_composition_roundtrips_ast_byte_equal() {
    let mut ui = Ui::<96>::new(320, 240);
    let root = ui.root();
    let context = ctx(320, 240);
    let stack = berry_ui::sdk::column(&mut ui, root, &context).unwrap();
    let mut input = field(&mut ui, stack, 160);
    let state = TextInputState::new(b"abc").unwrap();
    input.sync(&mut ui, &context, &state).unwrap();
    input.request_focus(&mut ui, &context).unwrap();

    let mut before = [0u8; 4096];
    let n = ui.encode_desc(&mut before).unwrap();
    assert!(n > 0);
    ui.apply_desc(&before[..n]).unwrap();
    let mut after = [0u8; 4096];
    let n2 = ui.encode_desc(&mut after).unwrap();
    assert_eq!(n, n2);
    assert_eq!(before[..n], after[..n2]);
    assert_eq!(ui.focus_node(), None, "focus side table remains runtime-only");
}

#[test]
fn focused_field_leaves_via_c1_without_losing_text_state() {
    let mut ui = Ui::<96>::new(320, 240);
    let root = ui.root();
    let context = ctx(320, 240);
    let stack = berry_ui::sdk::column(&mut ui, root, &context).unwrap();
    let mut button = berry_ui::sdk::Button::new(
        &mut ui,
        stack,
        &context,
        b"Go",
        berry_ui::sdk::InteractionState::Normal,
    )
    .unwrap();
    let mut input = field(&mut ui, stack, 160);
    let state = TextInputState::new(b"A").unwrap();
    input.sync(&mut ui, &context, &state).unwrap();
    input.request_focus(&mut ui, &context).unwrap();
    assert_eq!(ui.focus_node(), Some(input.panel));

    let messages = ui.focus_move(FocusDir::Next);
    assert_eq!(
        messages.iter().copied().collect::<Vec<_>>(),
        vec![
            Msg::FocusOut { node: input.panel },
            Msg::FocusIn { node: button.panel }
        ]
    );
    for message in messages.iter() {
        input.on_msg(&mut ui, &context, message).unwrap();
        button.on_msg(&mut ui, &context, message).unwrap();
    }
    assert_eq!(state.value().as_bytes(), b"A");
    assert_eq!(ui.focus_node(), Some(button.panel));
}

#[test]
fn constructor_rejects_zero_width_before_tree_mutation() {
    let mut ui = Ui::<96>::new(320, 240);
    let root = ui.root();
    let context = ctx(320, 240);
    let before = ast_len(&ui);
    let result = TextField::new(
        &mut ui,
        root,
        &context,
        TextFieldSpec { kind: TextInputKind::Text, width: 0 },
    );
    assert_eq!(result.unwrap_err(), UiError::NodeBad);
    assert_eq!(ast_len(&ui), before);
}

#[test]
fn keymap_repeat_and_shortcut_edges_are_pinned() {
    let kind = TextInputKind::Text;
    assert_eq!(
        text_key_intent(14, KEY_ACTION_REPEAT, 0, kind),
        Some(TextInputKeyOp::Edit(TextInputEdit::Backspace))
    );
    assert_eq!(
        text_key_intent(105, KEY_ACTION_DOWN, KEY_MOD_SHIFT, kind),
        Some(TextInputKeyOp::Edit(TextInputEdit::Left { extend: true }))
    );
    assert_eq!(
        text_key_intent(28, KEY_ACTION_REPEAT, 0, kind),
        Some(TextInputKeyOp::Submit)
    );
    assert_eq!(text_key_intent(15, KEY_ACTION_REPEAT, 0, kind), None);
    assert_eq!(
        text_key_intent(30, KEY_ACTION_DOWN, KEY_MOD_CTRL, kind),
        None
    );
}

fn ast_len(ui: &Ui<96>) -> usize {
    let mut bytes = [0u8; 4096];
    ui.encode_desc(&mut bytes).unwrap()
}

#[allow(dead_code)]
fn drain_damage(ui: &mut Ui<96>) {
    ui.wake(0);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    let mut encoder = CommandEncoder::new();
    let _ = ui.render_frame(0, &mut scratch, &mut encoder);
    let _ = encoder.finish();
}


#[test]
fn text_field_slots_add_label_and_error_without_new_semantic_anchors() {
    let context = ctx(1080, 1920);
    let mut ui = Ui::<96>::new(1080, 1920);
    let root = ui.root();
    let mut slots = TextFieldSlots::new(
        &mut ui,
        root,
        &context,
        TextFieldSlotsSpec {
            field: TextFieldSpec { kind: TextInputKind::Text, width: 480 },
            label: Some(b"User name"),
            supporting: Some(b"3-32 characters"),
        },
    )
    .unwrap();
    ui.layout();

    let label = ui.bounds(slots.label.unwrap()).unwrap();
    let field = ui.bounds(slots.field.panel).unwrap();
    let message = ui.bounds(slots.message).unwrap();
    assert_eq!(ui.bounds(slots.root).unwrap().w, 480);
    assert!(label.y + label.h <= field.y);
    assert!(message.y >= field.y + field.h);
    assert_eq!(slots.message_kind(), FieldMessageKind::Supporting);
    assert_eq!(ui.a11y_iter().filter(|entry| entry.node == slots.field.panel).count(), 1);
    assert_eq!(ui.sem_iter().count(), 1);

    slots
        .set_message(
            &mut ui,
            &context,
            Some(FieldMessage {
                kind: FieldMessageKind::Error,
                text: b"Name required",
            }),
        )
        .unwrap();
    ui.layout();
    assert_eq!(slots.message_kind(), FieldMessageKind::Error);

    ui.set_font_source(&StaticFontSource);
    ui.wake(0);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 96];
    let mut encoder = CommandEncoder::new();
    ui.paint(&mut scratch, &mut encoder);
    let list = encoder.finish();
    let danger = Color { r: 208, g: 47, b: 61 };
    assert!(list.iter().any(|cmd| match cmd {
        Cmd::DrawGlyphRun { run, .. } => run.fg == danger,
        _ => false,
    }));

    slots.set_message(&mut ui, &context, None).unwrap();
    assert_eq!(slots.message_kind(), FieldMessageKind::Supporting);
}
