//! SEM-A1 semantic overlay host tests：runtime-only、P0 投影、clip/merge、
//! focus 派生、生命周期与 AST 零漂。

use berry_guiproto::surface::{
    A11Y_ACTION_FOCUS, A11Y_STATE_CHECKED, A11Y_STATE_FOCUSED, A11Y_STATE_SELECTED,
    A11Y_STATE_VISIBLE,
};
use berry_ui::desc::{PanelProps, ViewKind, ViewSpec};
use berry_ui::sdk::{
    A11yRegion, Button, InteractionState, SemEntry, TextInputEdit, TextInputKind,
    SegmentedControl, SegmentedItemSpec, TextInputState, TextField, TextFieldSpec,
    WidgetContext, SEM_ACTION_ACTIVATE, SEM_ACTION_FOCUS, SEM_ACTION_TEXT_EDIT, SEM_MAX,
    SEM_STATE_CHECKED, SEM_STATE_DISABLED, SEM_STATE_SELECTED,
};
use berry_ui::{NodeId, Ui, UiError, ROLE_BUTTON, ROLE_LIST_ITEM, ROLE_TEXT};

fn panel(ui: &mut Ui<64>, parent: NodeId, w: u32, h: u32) -> NodeId {
    ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Panel,
            key: 0,
            props: berry_ui::Props::Panel(PanelProps {
                bg: berry_gfx::cmd::Color { r: 0, g: 0, b: 0 },
                w: Some(w),
                h: Some(h),
                padding: 0,
                gap: 0,
                radius: 0,
            }),
        },
    )
    .unwrap()
}

fn semantic(ui: &mut Ui<64>, node: NodeId, role: u8, label: &[u8]) -> SemEntry {
    ui.set_a11y(node, role, label).unwrap();
    SemEntry::new(node, SEM_ACTION_ACTIVATE, 0, b"", 0, 0, false).unwrap()
}

fn zero_region() -> A11yRegion {
    A11yRegion {
        node_id: 0,
        parent_id: 0,
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        role: 0,
        actions: 0,
        state: 0,
        label_len: 0,
        label: [0; 32],
    }
}

#[test]
fn overlay_lifecycle_capacity_and_validation_are_loud() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let node = panel(&mut ui, root, 40, 20);
    let mut first = semantic(&mut ui, node, ROLE_BUTTON, b"button");
    first.value = berry_ui::TextBuf::new(b"value").unwrap();
    ui.set_sem(first).unwrap();
    assert_eq!(ui.semantic_count(), 1);
    assert_eq!(ui.sem_of(node), Some(first));

    let mut second = first;
    second.state = SEM_STATE_CHECKED;
    ui.set_sem(second).unwrap();
    assert_eq!(ui.semantic_count(), 1);
    assert_eq!(ui.sem_of(node), Some(second));
    assert!(ui.take_sem(node));
    assert!(!ui.take_sem(node));
    ui.set_sem(first).unwrap();
    assert!(ui.take_a11y(node));
    assert_eq!(ui.semantic_count(), 0);

    let mut nodes = Vec::with_capacity(SEM_MAX + 1);
    for _ in 0..=SEM_MAX {
        let node = panel(&mut ui, root, 4, 4);
        let _ = semantic(&mut ui, node, ROLE_TEXT, b"x");
        nodes.push(node);
    }
    for node in nodes.iter().take(SEM_MAX) {
        let entry = SemEntry::new(*node, SEM_ACTION_ACTIVATE, 0, b"", 0, 0, false).unwrap();
        ui.set_sem(entry).unwrap();
    }
    assert_eq!(
        ui.set_sem(SemEntry::new(
            nodes[SEM_MAX], SEM_ACTION_ACTIVATE, 0, b"", 0, 0, false
        ).unwrap()).unwrap_err(),
        UiError::NodeFull
    );

    let mut direct = SemEntry::new(node, SEM_ACTION_ACTIVATE, 0, b"", 0, 0, false).unwrap();
    direct.actions = SEM_ACTION_FOCUS;
    assert_eq!(ui.set_sem(direct).unwrap_err(), UiError::NodeBad);
    let no_a11y = panel(&mut ui, root, 4, 4);
    assert_eq!(
        ui.set_sem(SemEntry::new(no_a11y, 0, 0, b"", 0, 0, false).unwrap()).unwrap_err(),
        UiError::NodeBad
    );
    assert_eq!(
        SemEntry::new(node, 0x80, 0, b"", 0, 0, false).unwrap_err(),
        UiError::NodeBad
    );
    assert_eq!(
        SemEntry::new(node, 0, A11Y_STATE_VISIBLE, b"", 0, 0, false).unwrap_err(),
        UiError::NodeBad
    );
    assert_eq!(
        SemEntry::new(node, 0, 0, &[b'v'; 65], 0, 0, false).unwrap_err(),
        UiError::TextTooLong { len: 65 }
    );
    assert_eq!(
        SemEntry::new(node, 0, 0, &[0xff], 0, 0, false).unwrap_err(),
        UiError::NodeBad
    );
    assert_eq!(
        SemEntry::new(node, 0, 0, b"abc", 2, 1, false).unwrap_err(),
        UiError::NodeBad
    );
    assert_eq!(
        SemEntry::new(node, 0, 0, b"abc", 0, 4, false).unwrap_err(),
        UiError::NodeBad
    );
}

#[test]
fn remove_and_apply_desc_clear_runtime_semantics() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let parent = panel(&mut ui, root, 40, 40);
    let child = panel(&mut ui, parent, 10, 10);
    let parent_sem = semantic(&mut ui, parent, ROLE_BUTTON, b"parent");
    let child_sem = semantic(&mut ui, child, ROLE_TEXT, b"child");
    ui.set_sem(parent_sem).unwrap();
    ui.set_sem(child_sem).unwrap();

    ui.remove(parent).unwrap();
    assert_eq!(ui.semantic_count(), 0);
    assert_eq!(ui.set_sem(parent_sem).unwrap_err(), UiError::NodeBad);

    let mut bytes = [0u8; 2048];
    let len = ui.encode_desc(&mut bytes).unwrap();
    ui.apply_desc(&bytes[..len]).unwrap();
    assert_eq!(ui.semantic_count(), 0);
}

#[test]
fn snapshot_is_preorder_clipped_parent_linked_and_p0_roundtrip() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let outer = panel(&mut ui, root, 100, 100);
    let inner = panel(&mut ui, outer, 20, 20);
    let sibling = panel(&mut ui, root, 30, 30);
    let hidden = panel(&mut ui, root, 20, 20);
    ui.set_a11y(outer, ROLE_BUTTON, b"outer").unwrap();
    let inner_sem = semantic(&mut ui, inner, ROLE_TEXT, b"inner");
    let sibling_sem = semantic(&mut ui, sibling, ROLE_LIST_ITEM, b"sibling");
    let hidden_sem = semantic(&mut ui, hidden, ROLE_TEXT, b"hidden");
    ui.set_sem(inner_sem).unwrap();
    ui.set_sem(sibling_sem).unwrap();
    ui.set_sem(hidden_sem).unwrap();
    ui.layout();
    ui.set_offset(hidden, 0, 300).unwrap();

    let mut regions = [zero_region(); 8];
    let count = ui.sem_snapshot_into(&mut regions).unwrap();
    assert_eq!(count, 3);
    assert_eq!(&regions[0].label[..5], b"outer");
    assert_eq!(regions[0].parent_id, 0);
    assert_eq!(&regions[1].label[..5], b"inner");
    assert_eq!(regions[1].parent_id, regions[0].node_id);
    assert_eq!(&regions[2].label[..7], b"sibling");
    assert_eq!(regions[2].parent_id, 0);
    assert!(regions[..count].iter().all(|region| region.node_id != 0));
    assert!(regions[..count].iter().all(|region| region.state & A11Y_STATE_VISIBLE != 0));

    assert!(regions[..count]
        .iter()
        .all(berry_ui::sdk::a11y_region_roundtrip));
}

#[test]
fn snapshot_derives_focus_and_semantic_overlay_wins() {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let node = panel(&mut ui, root, 40, 20);
    let mut entry = semantic(&mut ui, node, ROLE_BUTTON, b"go");
    entry.state = SEM_STATE_CHECKED;
    ui.set_sem(entry).unwrap();
    ui.set_focusable(node, Some(node), true).unwrap();
    ui.focus_set(node).unwrap();
    let selected_node = panel(&mut ui, root, 30, 20);
    let mut selected_entry = semantic(&mut ui, selected_node, ROLE_BUTTON, b"current");
    selected_entry.state = SEM_STATE_SELECTED;
    ui.set_sem(selected_entry).unwrap();
    ui.layout();

    let mut regions = [zero_region(); 3];
    let count = ui.sem_snapshot_into(&mut regions).unwrap();
    assert_eq!(count, 2);
    assert_eq!(regions[0].role, ROLE_BUTTON);
    assert_eq!(regions[0].actions, SEM_ACTION_ACTIVATE | A11Y_ACTION_FOCUS);
    assert_eq!(
        regions[0].state,
        A11Y_STATE_VISIBLE | A11Y_STATE_FOCUSED | A11Y_STATE_CHECKED
    );
    assert_eq!(regions[1].label[..7], *b"current");
    assert_eq!(
        regions[1].state,
        A11Y_STATE_VISIBLE | A11Y_STATE_SELECTED
    );
    assert!(regions[..count].iter().all(berry_ui::sdk::a11y_region_roundtrip));
}

#[test]
fn merge_descendants_and_out_too_small_are_loud() {
    let mut ui = Ui::<64>::new(100, 100);
    let root = ui.root();
    let visible = panel(&mut ui, root, 30, 30);
    let merged = panel(&mut ui, root, 50, 50);
    let child = panel(&mut ui, merged, 10, 10);
    let visible_sem = semantic(&mut ui, visible, ROLE_BUTTON, b"visible");
    let mut merged_sem = semantic(&mut ui, merged, ROLE_BUTTON, b"merged");
    merged_sem.merge_descendants = true;
    let child_sem = semantic(&mut ui, child, ROLE_TEXT, b"child");
    ui.set_sem(visible_sem).unwrap();
    ui.set_sem(merged_sem).unwrap();
    ui.set_sem(child_sem).unwrap();
    ui.layout();

    let mut regions = [zero_region(); 4];
    let count = ui.sem_snapshot_into(&mut regions).unwrap();
    assert_eq!(count, 2);
    assert_eq!(&regions[0].label[..7], b"visible");
    assert_eq!(&regions[1].label[..6], b"merged");

    let mut regions = [zero_region(); 1];
    let a = panel(&mut ui, root, 10, 10);
    let b = panel(&mut ui, root, 10, 10);
    let a_sem = semantic(&mut ui, a, ROLE_BUTTON, b"a");
    let b_sem = semantic(&mut ui, b, ROLE_BUTTON, b"b");
    ui.set_sem(a_sem).unwrap();
    ui.set_sem(b_sem).unwrap();
    assert_eq!(ui.sem_snapshot_into(&mut regions).unwrap_err(), UiError::NodeFull);
}

#[test]
fn widgets_sync_semantic_state_value_and_range() {
    let mut ui = Ui::<64>::new(320, 240);
    let ctx = WidgetContext::for_viewport(berry_ui::sdk::Theme::light(), 320, 240);
    let root = ui.root();

    let mut button = Button::new(&mut ui, root, &ctx, b"OK", InteractionState::Disabled).unwrap();
    assert_eq!(ui.sem_of(button.panel).unwrap().state, SEM_STATE_DISABLED);
    button.set_base(&mut ui, &ctx, InteractionState::Normal).unwrap();
    assert_eq!(ui.sem_of(button.panel).unwrap().state, 0);

    let mut item = berry_ui::sdk::ListItem::new(&mut ui, root, &ctx, b"Item", None, false).unwrap();
    assert_eq!(ui.sem_of(item.panel).unwrap().state, 0);
    item.set_selected(&mut ui, &ctx, true).unwrap();
    assert_eq!(ui.sem_of(item.panel).unwrap().state, SEM_STATE_SELECTED);

    let segment_items = [
        SegmentedItemSpec { label: b"One", disabled: false },
        SegmentedItemSpec { label: b"Two", disabled: false },
    ];
    let segments = SegmentedControl::new(&mut ui, root, &ctx, &segment_items, 0).unwrap();
    assert_eq!(
        ui.sem_of(segments.item_panel(0).unwrap()).unwrap().state,
        SEM_STATE_SELECTED
    );
    assert_eq!(
        ui.sem_of(segments.item_panel(1).unwrap()).unwrap().state,
        0
    );

    let mut switch = berry_ui::sdk::Switch::new(&mut ui, root, &ctx, false, false).unwrap();
    assert_eq!(ui.sem_of(switch.panel).unwrap().state, 0);
    switch.set_checked(&mut ui, &ctx, true).unwrap();
    let switch_sem = ui.sem_of(switch.panel).unwrap();
    assert_eq!(switch_sem.state, SEM_STATE_CHECKED);
    assert_eq!(switch_sem.value.as_bytes(), b"on");

    let mut input = TextField::new(
        &mut ui, root, &ctx, TextFieldSpec { kind: TextInputKind::Text, width: 120 },
    ).unwrap();
    let mut state = TextInputState::new(b"ab").unwrap();
    state.apply_edit(TextInputEdit::Left { extend: true });
    input.sync(&mut ui, &ctx, &state).unwrap();
    let input_sem = ui.sem_of(input.panel).unwrap();
    assert_eq!(input_sem.actions, SEM_ACTION_TEXT_EDIT);
    assert_eq!(input_sem.value.as_bytes(), b"ab");
    // 反向 selection（anchor=2,caret=1）投影为无向语义区间 (1,2)。
    assert_eq!((input_sem.text_range.anchor_cp, input_sem.text_range.caret_cp), (1, 2));
}

#[test]
fn password_semantic_value_and_range_stay_empty() {
    let mut ui = Ui::<64>::new(320, 240);
    let ctx = WidgetContext::for_viewport(berry_ui::sdk::Theme::light(), 320, 240);
    let root = ui.root();
    let mut input = TextField::new(
        &mut ui, root, &ctx, TextFieldSpec { kind: TextInputKind::Password, width: 120 },
    ).unwrap();
    let state = TextInputState::new("设密".as_bytes()).unwrap();
    input.sync(&mut ui, &ctx, &state).unwrap();
    let semantic = ui.sem_of(input.panel).unwrap();
    assert!(semantic.value.is_empty());
    assert_eq!(semantic.text_range().ordered(), (0, 0));
}

#[test]
fn semantic_hit_and_ast_have_single_sources() {
    let mut ui = Ui::<64>::new(320, 240);
    let ctx = WidgetContext::for_viewport(berry_ui::sdk::Theme::light(), 320, 240);
    let root = ui.root();
    let button = Button::new(&mut ui, root, &ctx, b"OK", InteractionState::Normal).unwrap();
    ui.layout();
    let mut before = [0u8; 2048];
    let len = ui.encode_desc(&mut before).unwrap();
    let entry = ui.sem_of(button.panel).unwrap();
    ui.set_sem(entry).unwrap();
    let mut after = [0u8; 2048];
    let len2 = ui.encode_desc(&mut after).unwrap();
    assert_eq!(len, len2);
    assert_eq!(before[..len], after[..len2]);
    assert_eq!(ui.sem_at(10, 10), Some(button.panel));
    assert!(ui.semantic_snapshot_id(button.panel).is_some());
}
