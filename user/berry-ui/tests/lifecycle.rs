//! RUN-K1 lifecycle properties：keyed re-apply 不只追求几何相同，还证明
//! removed-subtree 侧表清理、焦点消息交替、非法转换零半执行与确定性
//! random op/full-rebuild oracle。

use berry_gfx::cmd::Color;
use berry_ui::desc::{
    ColProps, DescBuilder, Msg, Props, RectProps, ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::ROLE_BUTTON;
use berry_ui::sdk::{
    RectI, SemEntry, Ui, UiError, SEM_ACTION_ACTIVATE,
};

fn rect_spec(key: u64, w: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key,
        props: Props::Rect(RectProps {
            bg: Color { r: 1, g: 2, b: 3 },
            w: Some(w),
            h: Some(20),
        }),
    }
}

fn flex_rect_spec(key: u64) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key,
        props: Props::Rect(RectProps {
            bg: Color { r: 4, g: 5, b: 6 },
            w: None,
            h: None,
        }),
    }
}

fn flex_stream(width: u32) -> Vec<u8> {
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Col,
        100,
        &Props::Col(ColProps {
            gap: 0,
            w: Some(width),
            h: None,
            cross: CrossAlign::Stretch,
        }),
    )
    .unwrap();
    b.size(Some(width), None).unwrap();
    b.begin(
        ViewKind::Rect,
        101,
        &Props::Rect(RectProps {
            bg: Color { r: 4, g: 5, b: 6 },
            w: None,
            h: None,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.end().unwrap();
    b.end().unwrap();
    b.bytes().to_vec()
}

fn keyed_stream(children: &[(u64, u32)]) -> Vec<u8> {
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    for &(key, width) in children {
        b.begin(
            ViewKind::Rect,
            key,
            &Props::Rect(RectProps {
                bg: Color { r: 7, g: 8, b: 9 },
                w: Some(width),
                h: Some(20),
            }),
        )
        .unwrap();
        b.end().unwrap();
    }
    b.end().unwrap();
    b.bytes().to_vec()
}

fn id_by_key<const N: usize>(ui: &mut Ui<N>, key: u64) -> berry_ui::NodeId {
    ui.root();
    let mut scratch = vec![
        berry_ui::diagnostic::LayoutDumpEntry {
            id: ui.root(),
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
            bounds: RectI::new(0, 0, 0, 0),
            measured_w: 0,
            measured_h: 0,
            layout_dirty: false,
            paint_dirty: false,
        };
        N + 8
    ];
    let n = ui.layout_dump(&mut scratch).unwrap();
    scratch.truncate(n);
    scratch.into_iter().find(|e| e.key == key).unwrap().id
}

fn encoded<const N: usize>(ui: &mut Ui<N>) -> Vec<u8> {
    let mut out = vec![0; 8192];
    let n = ui.encode_desc(&mut out).unwrap();
    out.truncate(n);
    out
}

#[test]
fn keyed_reapply_preserves_runtime_sem_focus_and_flex_identity() {
    let mut ui = Ui::<24>::new(200, 200);
    ui.apply_desc(&flex_stream(30)).unwrap();
    let col = id_by_key(&mut ui, 100);
    let rect = id_by_key(&mut ui, 101);

    ui.set_a11y(rect, ROLE_BUTTON, b"Run").unwrap();
    ui.set_sem(SemEntry::new(rect, SEM_ACTION_ACTIVATE, 0, b"On", 0, 2, false).unwrap())
        .unwrap();
    ui.set_focusable(rect, Some(rect), true).unwrap();
    ui.set_flex(rect, 2).unwrap();
    let messages = ui.focus_set(rect).unwrap();
    assert_eq!(messages.iter().copied().collect::<Vec<_>>(), vec![Msg::FocusIn { node: rect }]);

    let stat = ui.apply_desc_keyed(&flex_stream(42)).unwrap();
    assert_eq!(stat.updated, 1);
    assert_eq!(stat.matched_clean, 2);
    assert_eq!(id_by_key(&mut ui, 101), rect);
    assert_eq!(id_by_key(&mut ui, 100), col);
    assert_eq!(ui.focus_node(), Some(rect));
    assert_eq!(ui.sem_of(rect).unwrap().node, rect);
    assert!(ui.flex_iter().any(|entry| entry.node == rect && entry.weight == 2));
    assert!(ui.a11y_iter().any(|entry| entry.node == rect && entry.label.as_bytes() == b"Run"));
}

#[test]
fn removed_subtree_clears_runtime_side_tables_without_synthetic_messages() {
    let mut ui = Ui::<24>::new(200, 200);
    let root = ui.root();
    let col = ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 200,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(100),
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    )
    .unwrap();
    let rect = ui.add(col, flex_rect_spec(201)).unwrap();

    ui.set_a11y(rect, ROLE_BUTTON, b"Remove").unwrap();
    ui.set_sem(SemEntry::new(rect, SEM_ACTION_ACTIVATE, 0, b"Off", 0, 3, false).unwrap())
        .unwrap();
    ui.set_focusable(rect, Some(rect), true).unwrap();
    ui.set_flex(rect, 1).unwrap();
    let _ = ui.focus_set(rect).unwrap();

    ui.remove(col).unwrap();
    assert_eq!(ui.focus_node(), None);
    assert_eq!(ui.sem_of(rect), None);
    assert!(!ui.a11y_iter().any(|entry| entry.node == rect));
    assert!(!ui.focus_iter().any(|entry| entry.node == rect));
    assert!(!ui.flex_iter().any(|entry| entry.node == rect));
    // remove 是结构生命周期，不产输入消息，也不合成 FocusOut/Click。
    assert!(ui.pump_input(0).is_empty());
}

#[test]
fn focus_messages_alternate_and_repeated_focus_is_noop() {
    let mut ui = Ui::<24>::new(300, 100);
    let root = ui.root();
    let a = ui.add(root, rect_spec(1, 30)).unwrap();
    let b = ui.add(root, rect_spec(2, 30)).unwrap();
    ui.set_focusable(a, None, true).unwrap();
    ui.set_focusable(b, None, true).unwrap();

    let first = ui.focus_set(a).unwrap();
    assert_eq!(first.iter().copied().collect::<Vec<_>>(), vec![Msg::FocusIn { node: a }]);
    let second = ui.focus_set(b).unwrap();
    assert_eq!(
        second.iter().copied().collect::<Vec<_>>(),
        vec![Msg::FocusOut { node: a }, Msg::FocusIn { node: b }]
    );
    assert_eq!(ui.focus_set(b).unwrap().is_empty(), true);
    let clear = ui.focus_clear();
    assert_eq!(clear.iter().copied().collect::<Vec<_>>(), vec![Msg::FocusOut { node: b }]);
    assert_eq!(ui.focus_node(), None);
}

#[test]
fn illegal_transitions_fail_loud_and_preserve_state() {
    let mut ui = Ui::<16>::new(200, 200);
    let root = ui.root();
    let node = ui.add(root, rect_spec(7, 30)).unwrap();
    let stale = ui.add(root, rect_spec(8, 30)).unwrap();
    ui.remove(stale).unwrap();
    let before = encoded(&mut ui);

    assert_eq!(
        ui.add(root, rect_spec(7, 40)),
        Err(UiError::NodeBad)
    );
    assert_eq!(
        ui.set_props(
            node,
            Props::Text(berry_ui::desc::TextProps {
                style: berry_ui::desc::TextStyle {
                    fg: Color { r: 1, g: 2, b: 3 },
                    bg: None,
                    scale: berry_gfx::font::FontSize::S16x32,
                },
                text: berry_ui::desc::TextBuf::new(b"bad").unwrap(),
            })
        ),
        Err(UiError::NodeBad)
    );
    assert_eq!(ui.set_focusable(stale, None, true), Err(UiError::NodeBad));
    assert_eq!(encoded(&mut ui), before);
}

#[test]
fn deterministic_random_updates_match_full_rebuild_oracle_and_settle() {
    // LCG：固定种子、无时钟/HashMap/迭代序，失败可复现。
    let mut seed = 0x1357_2468_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    let keys = [11_u64, 22, 33, 44];
    let widths = [20_u32, 31, 42, 53];
    let mut keyed = Ui::<24>::new(300, 200);
    let mut oracle = Ui::<24>::new(300, 200);
    let initial = keyed_stream(&[
        (keys[0], widths[0]),
        (keys[1], widths[1]),
        (keys[2], widths[2]),
        (keys[3], widths[3]),
    ]);
    keyed.apply_desc(&initial).unwrap();
    let stable = id_by_key(&mut keyed, 22);
    keyed.set_focusable(stable, Some(stable), true).unwrap();
    let _ = keyed.focus_set(stable).unwrap();

    for round in 0..64 {
        let mut children: Vec<(u64, u32)> = keys
            .iter()
            .copied()
            .zip(widths.iter().copied().map(|width| width + (next() % 7) as u32))
            .collect();
        // 前 32 轮逐步换序；后 32 轮保持序，覆盖 moved 与 clean 两态。
        if round < 32 {
            children.swap((next() % 4) as usize, (next() % 4) as usize);
        }
        let source = keyed_stream(&children);

        keyed.apply_desc_keyed(&source).unwrap();
        oracle.apply_desc(&source).unwrap();
        assert_eq!(encoded(&mut keyed), encoded(&mut oracle));
        assert_eq!(id_by_key(&mut keyed, 22), stable);
        assert_eq!(keyed.focus_node(), Some(stable));

        let stat = keyed.apply_desc_keyed(&source).unwrap();
        assert_eq!(stat.matched_clean, 5);
        assert_eq!(stat.updated + stat.moved + stat.added + stat.removed, 0);
    }
}

#[test]
fn widget_context_for_ui_uses_the_ui_viewport_single_source() {
    let ui = Ui::<8>::new(123, 456);
    let theme = berry_ui::sdk::Theme::light();
    assert_eq!(
        berry_ui::sdk::WidgetContext::for_ui(theme, &ui),
        berry_ui::sdk::WidgetContext::for_viewport(theme, 123, 456)
    );
}
