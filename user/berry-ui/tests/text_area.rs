//! TextArea v1 host tests：显式/自动换行、行编辑、selection、焦点与语义。

use berry_guiproto::{KEY_ACTION_DOWN, KEY_ACTION_REPEAT, KEY_MOD_SHIFT};
use berry_ui::desc::Msg;
use berry_ui::sdk::{
    key_route, KeyRoute, TextArea, TextAreaSpec, TextField, TextFieldSpec, TextInputEdit,
    TextInputKind, TextInputKeyOp, TextInputState, Theme, WidgetContext,
    SEM_ACTION_TEXT_EDIT, TEXT_AREA_LINES_MAX,
};
use berry_ui::{Ui, UiError};

fn ctx(w: u32, h: u32) -> WidgetContext {
    WidgetContext::for_viewport(Theme::light(), w, h)
}

fn area(ui: &mut Ui<96>, parent: berry_ui::NodeId, width: u32, lines: u8) -> TextArea {
    TextArea::new(
        ui,
        parent,
        &ctx(ui.viewport().0, ui.viewport().1),
        TextAreaSpec { width, lines },
    )
    .unwrap()
}

#[test]
fn multiline_enter_and_navigation_are_kind_gated() {
    let multiline = TextInputKind::Multiline;
    let single = TextInputKind::Text;
    assert_eq!(
        key_route(28, KEY_ACTION_DOWN, 0, multiline, true),
        KeyRoute::Text(TextInputKeyOp::Edit(TextInputEdit::Insert('\n')))
    );
    assert_eq!(
        key_route(28, KEY_ACTION_DOWN, KEY_MOD_SHIFT, multiline, true),
        KeyRoute::Text(TextInputKeyOp::Edit(TextInputEdit::Insert('\n')))
    );
    assert_eq!(
        key_route(28, KEY_ACTION_DOWN, 0, single, true),
        KeyRoute::Text(TextInputKeyOp::Submit)
    );
    assert_eq!(
        key_route(103, KEY_ACTION_DOWN, 0, multiline, true),
        KeyRoute::Text(TextInputKeyOp::Edit(TextInputEdit::LineUp { extend: false }))
    );
    assert_eq!(
        key_route(108, KEY_ACTION_REPEAT, KEY_MOD_SHIFT, multiline, true),
        KeyRoute::Text(TextInputKeyOp::Edit(TextInputEdit::LineDown { extend: true }))
    );
    assert_eq!(key_route(103, KEY_ACTION_DOWN, 0, single, true), KeyRoute::None);
    assert_eq!(
        key_route(102, KEY_ACTION_DOWN, 0, single, true),
        KeyRoute::Text(TextInputKeyOp::Edit(TextInputEdit::Home { extend: false }))
    );
}

#[test]
fn logical_line_helpers_and_sticky_column_are_total() {
    let bytes = b"ab\ncdef\ngh";
    let mut state = TextInputState::new(bytes).unwrap();
    assert_eq!(berry_ui::text_input::line_count(bytes), 3);
    assert_eq!(berry_ui::text_input::line_col_of(bytes, state.caret_cp()), (2, 2));
    assert_eq!(berry_ui::text_input::line_span(bytes, 1), (3, 7));

    assert!(state.apply_edit(TextInputEdit::LineDown { extend: false }).rejected);
    let out = state.apply_edit(TextInputEdit::LineUp { extend: false });
    assert!(out.selection_changed);
    assert_eq!(state.caret_cp(), 5);
    let _ = state.apply_edit(TextInputEdit::LineUp { extend: false });
    assert_eq!(state.caret_cp(), 2);
    let _ = state.apply_edit(TextInputEdit::LineDown { extend: false });
    assert_eq!(state.caret_cp(), 5, "short line clamps to its end");
    let out = state.apply_edit(TextInputEdit::LineDown { extend: false });
    assert_eq!(state.caret_cp(), 10, "sticky column restores 2");
    assert!(out.selection_changed);
    let _ = state.apply_edit(TextInputEdit::LineUp { extend: false });

    let mut state = TextInputState::new(b"ab\ncd").unwrap();
    let _ = state.apply_edit(TextInputEdit::LineHome { extend: false });
    assert_eq!(state.caret_cp(), 3);
    let _ = state.apply_edit(TextInputEdit::LineEnd { extend: false });
    assert_eq!(state.caret_cp(), 5);
    let _ = state.apply_edit(TextInputEdit::Home { extend: false });
    assert_eq!(state.caret_cp(), 0);
    let _ = state.apply_edit(TextInputEdit::End { extend: false });
    assert_eq!(state.caret_cp(), 5);
}

#[test]
fn newline_insert_and_backspace_merge_logical_lines() {
    let mut state = TextInputState::new(b"ab").unwrap();
    let _ = state.apply_edit(TextInputEdit::Insert('\n'));
    let _ = state.apply_edit(TextInputEdit::Insert('c'));
    assert_eq!(state.value().as_bytes(), b"ab\nc");
    assert_eq!(state.caret_cp(), 4);

    let _ = state.apply_edit(TextInputEdit::Left { extend: false });
    let _ = state.apply_edit(TextInputEdit::Backspace);
    assert_eq!(state.value().as_bytes(), b"abc");
    assert_eq!(state.caret_cp(), 2);
}

#[test]
fn text_area_renders_hard_lines_and_positions_caret() {
    let context = ctx(1080, 1920);
    let mut ui = Ui::<96>::new(1080, 1920);
    let root = ui.root();
    let mut area = area(&mut ui, root, 400, 3);
    let state = TextInputState::new(b"Berry notes\nline two").unwrap();
    area.sync(&mut ui, &context, &state).unwrap();

    let font = context.metrics.typography(berry_ui::sdk::TypographyToken::Body);
    let pad = context.metrics.spacing(berry_ui::sdk::SpacingToken::Xs);
    assert_eq!(
        ui.bounds(area.panel).unwrap().h,
        3 * font.cell_h() as i32 + 2 * pad as i32
    );
    assert_eq!(ui.bounds(area.value_text).unwrap().h, 2 * font.cell_h() as i32);

    let messages = area.request_focus(&mut ui, &context).unwrap();
    assert_eq!(
        messages.iter().copied().collect::<Vec<_>>(),
        vec![Msg::FocusIn { node: area.panel }]
    );
    area.sync(&mut ui, &context, &state).unwrap();
    let caret = ui.bounds(area.caret).unwrap();
    assert_eq!(
        caret.x,
        ui.bounds(area.panel).unwrap().x + pad as i32 + 8 * font.cell_w() as i32
    );
    assert_eq!(
        caret.y,
        ui.bounds(area.panel).unwrap().y + pad as i32 + font.cell_h() as i32
    );
    assert!(caret.w > 0);
}

#[test]
fn text_area_selection_spans_lines_with_three_rects() {
    let context = ctx(1080, 1920);
    let mut ui = Ui::<96>::new(1080, 1920);
    let root = ui.root();
    let mut area = area(&mut ui, root, 400, 3);
    let mut state = TextInputState::new(b"aa\nbbbb\ncc").unwrap();
    state.select_all();
    area.request_focus(&mut ui, &context).unwrap();
    area.sync(&mut ui, &context, &state).unwrap();

    let font = context.metrics.typography(berry_ui::sdk::TypographyToken::Body);
    let cell_w = font.cell_w() as i32;
    let head = ui.bounds(area.sel_head).unwrap();
    let band = ui.bounds(area.sel_band).unwrap();
    let tail = ui.bounds(area.sel_tail).unwrap();
    assert_eq!(head.w, 2 * cell_w);
    assert_eq!(tail.w, 2 * cell_w);
    assert_eq!(band.w, ui.bounds(area.port).unwrap().w);
    assert_eq!(band.h, font.cell_h() as i32);
    assert!(head.y < band.y && band.y < tail.y);
}

#[test]
fn text_area_vertical_scroll_keeps_caret_visible() {
    let context = ctx(1080, 1920);
    let mut ui = Ui::<96>::new(1080, 1920);
    let root = ui.root();
    let mut area = area(&mut ui, root, 400, 2);
    let mut state = TextInputState::new(b"A\nB\nC\nD\nE\nF").unwrap();
    let _ = state.apply_edit(TextInputEdit::End { extend: false });
    state.select_all();
    area.request_focus(&mut ui, &context).unwrap();
    area.sync(&mut ui, &context, &state).unwrap();

    let port = ui.bounds(area.port).unwrap();
    let caret = ui.bounds(area.caret).unwrap();
    let tail = ui.bounds(area.sel_tail).unwrap();
    assert!(area.scroll_offset() > 0);
    assert!(caret.y >= port.y);
    assert!(caret.y + caret.h <= port.y + port.h);
    assert!(tail.y >= port.y);
    assert!(tail.y + tail.h <= port.y + port.h);

    let _ = state.apply_edit(TextInputEdit::Home { extend: false });
    let _ = state.apply_edit(TextInputEdit::Home { extend: false });
    area.sync(&mut ui, &context, &state).unwrap();
    assert_eq!(area.scroll_offset(), 0);
}

#[test]
fn text_area_semantics_focus_and_ast_roundtrip() {
    let context = ctx(1080, 1920);
    let mut ui = Ui::<96>::new(1080, 1920);
    let root = ui.root();
    let mut area = area(&mut ui, root, 360, 3);
    let state = TextInputState::new(b"one\ntwo").unwrap();
    area.sync(&mut ui, &context, &state).unwrap();
    assert_eq!(ui.sem_iter().count(), 1);
    let semantic = ui.sem_of(area.panel).unwrap();
    assert_eq!(semantic.actions, SEM_ACTION_TEXT_EDIT);
    assert_eq!(semantic.value.as_bytes(), b"one\ntwo");
    assert_eq!(ui.a11y_state(area.panel), Some(1));

    let messages = area.request_focus(&mut ui, &context).unwrap();
    assert_eq!(ui.a11y_state(area.panel), Some(3));
    assert_eq!(
        messages.iter().copied().collect::<Vec<_>>(),
        vec![Msg::FocusIn { node: area.panel }]
    );
    let cleared = ui.focus_clear();
    assert_eq!(
        cleared.iter().copied().collect::<Vec<_>>(),
        vec![Msg::FocusOut { node: area.panel }]
    );
    assert!(area
        .on_msg(&mut ui, &context, &Msg::FocusOut { node: area.panel })
        .unwrap());
    assert_eq!(ui.a11y_state(area.panel), Some(1));

    let mut before = [0u8; 4096];
    let n = ui.encode_desc(&mut before).unwrap();
    ui.apply_desc(&before[..n]).unwrap();
    let mut after = [0u8; 4096];
    let n2 = ui.encode_desc(&mut after).unwrap();
    assert_eq!(before[..n], after[..n2]);
    assert_eq!(ui.a11y_state(area.panel), None);
    assert_eq!(ui.focus_node(), None);
}

#[test]
fn text_area_constructor_reject_faces_without_tree_mutation() {
    let mut ui = Ui::<32>::new(320, 240);
    let context = ctx(320, 240);
    let root = ui.root();
    let before = {
        let mut bytes = [0u8; 2048];
        ui.encode_desc(&mut bytes).unwrap()
    };
    for spec in [
        TextAreaSpec { width: 0, lines: 3 },
        TextAreaSpec { width: 300, lines: 0 },
        TextAreaSpec { width: 300, lines: TEXT_AREA_LINES_MAX as u8 + 1 },
    ] {
        assert_eq!(
            TextArea::new(&mut ui, root, &context, spec).unwrap_err(),
            UiError::NodeBad
        );
    }
    assert_eq!(
        TextField::new(
            &mut ui,
            root,
            &context,
            TextFieldSpec { kind: TextInputKind::Multiline, width: 200 },
        )
        .unwrap_err(),
        UiError::NodeBad
    );
    let mut bytes = [0u8; 2048];
    assert_eq!(ui.encode_desc(&mut bytes).unwrap(), before);
}
