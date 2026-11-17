//! Card 按压反馈钉板：真实 token 换底 + 2px paint offset，同时证明
//! 宽高、兄弟布局与 AST 语义都不变。这里的反馈不是 width/height 缩放——
//! 那会触发 GEOM 组重排并移动整张卡内容。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos};
use berry_ui::desc::{Msg, Props, RectProps, ViewKind, ViewSpec};
use berry_ui::sdk::{
    Card, InteractionState, RectI, Theme, Ui, WidgetContext,
};

fn ctx(w: u32, h: u32) -> WidgetContext {
    WidgetContext::for_viewport(Theme::light(), w, h)
}

#[test]
fn card_press_changes_paint_and_offset_without_relayout() {
    let context = ctx(1080, 1920);
    let mut ui = Ui::<64>::new(1080, 1920);
    let root = ui.root();
    let stack = berry_ui::sdk::column(&mut ui, root, &context).unwrap();
    let mut card = Card::new(&mut ui, stack, &context).unwrap();
    berry_ui::sdk::text(
        &mut ui,
        card.panel,
        &context,
        berry_ui::sdk::TextVariant::Body,
        b"Card body",
    )
    .unwrap();
    let sibling = ui
        .add(
            stack,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: Color { r: 9, g: 19, b: 29 },
                    w: None,
                    h: Some(40),radius: 0}),
            },
        )
        .unwrap();
    ui.layout();
    let rest_card = ui.bounds(card.panel).unwrap();
    let rest_sibling = ui.bounds(sibling).unwrap();
    assert!(rest_card.w > 0 && rest_card.h > 0);

    let pressed_node = card.panel;
    card.on_msg(
        &mut ui,
        &context,
        &Msg::Press {
            node: pressed_node,
            x: 4,
            y: 4,
        },
    )
    .unwrap();
    assert!(card.is_pressed());
    let stat = ui.layout();
    assert_eq!(stat.measured, 0, "token bg + offset must not remeasure");
    let pressed_card = ui.bounds(card.panel).unwrap();
    let pressed_sibling = ui.bounds(sibling).unwrap();
    assert_eq!(pressed_card.w, rest_card.w);
    assert_eq!(pressed_card.h, rest_card.h);
    assert_eq!(pressed_card.y, rest_card.y + 6, "product 2dp = 6px nudge");
    assert_eq!(pressed_card.x, rest_card.x);
    assert_eq!(pressed_sibling, rest_sibling, "sibling never relayouts");
    assert!(!ui.damage().is_empty());

    ui.wake(1);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 128];
    let mut encoder = CommandEncoder::new();
    let outcome = ui.render_frame(2, &mut scratch, &mut encoder);
    assert!(outcome.drew);
    let list = encoder.finish();
    let pressed_bg = context
        .theme
        .state(InteractionState::Pressed)
        .background;
    assert!(
        list.iter().any(|command| match command {
            Cmd::FillRoundRect { rect, paint, .. } => {
                paint.color == pressed_bg
                    && rect.w == pressed_card.w
                    && rect.h == pressed_card.h
            }
            _ => false,
        }),
        "pressed Card uses the theme Pressed token"
    );

    card.on_msg(
        &mut ui,
        &context,
        &Msg::Release {
            node: pressed_node,
            x: 4,
            y: 4,
        },
    )
    .unwrap();
    assert!(!card.is_pressed());
    let released = ui.bounds(card.panel).unwrap();
    assert_eq!(
        released,
        RectI::new(
            rest_card.x,
            rest_card.y,
            rest_card.w,
            rest_card.h
        )
    );
}

#[test]
fn card_click_has_visual_only_and_foreign_messages_are_ignored() {
    let context = ctx(800, 600);
    let mut ui = Ui::<32>::new(800, 600);
    let root = ui.root();
    let mut card = Card::new(&mut ui, root, &context).unwrap();
    ui.layout();

    let other = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: Color { r: 1, g: 2, b: 3 },
                    w: Some(20),
                    h: Some(20),radius: 0}),
            },
        )
        .unwrap();
    ui.layout();

    assert!(!card
        .on_msg(
            &mut ui,
            &context,
            &Msg::Press {
                node: other,
                x: 1,
                y: 1
            }
        )
        .unwrap());
    assert!(!card.is_pressed());

    assert!(!card
        .on_msg(&mut ui, &context, &Msg::Click { node: card.panel })
        .unwrap());
    assert!(!card.is_pressed(), "Click restores surface but owns no action");
}
