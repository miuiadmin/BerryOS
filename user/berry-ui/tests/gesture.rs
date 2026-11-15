use berry_ui::desc::{Axis, Msg};
use berry_ui::sdk::{LongPressGesture, LongPressOutcome, Ui, LONG_PRESS_NS};

#[test]
fn long_press_fires_once_and_suppresses_trailing_click() {
    let mut ui = Ui::<4>::new(100, 100);
    let node = ui.root();
    let mut gesture = LongPressGesture::new(LONG_PRESS_NS);
    assert_eq!(
        gesture.on_msg(&Msg::Press { node, x: 20, y: 30 }),
        LongPressOutcome::None
    );
    assert!(gesture.pending());
    assert_eq!(gesture.tick(1), LongPressOutcome::None);
    assert_eq!(
        gesture.tick(1 + LONG_PRESS_NS - 1),
        LongPressOutcome::None
    );
    assert_eq!(
        gesture.tick(1 + LONG_PRESS_NS),
        LongPressOutcome::Fired { node, x: 20, y: 30 }
    );
    assert_eq!(gesture.tick(1 + LONG_PRESS_NS + 1), LongPressOutcome::None);
    assert_eq!(
        gesture.on_msg(&Msg::Release { node, x: 20, y: 30 }),
        LongPressOutcome::None
    );
    assert_eq!(
        gesture.on_msg(&Msg::Click { node }),
        LongPressOutcome::ClickSuppressed { node }
    );
    assert_eq!(gesture.on_msg(&Msg::Click { node }), LongPressOutcome::None);
}

#[test]
fn long_press_first_tick_is_the_clock_anchor() {
    let mut ui = Ui::<4>::new(100, 100);
    let node = ui.root();
    let mut gesture = LongPressGesture::new(100);
    gesture.on_msg(&Msg::Press { node, x: 1, y: 2 });
    assert_eq!(gesture.tick(50), LongPressOutcome::None);
    assert_eq!(gesture.tick(149), LongPressOutcome::None);
    assert_eq!(
        gesture.tick(150),
        LongPressOutcome::Fired { node, x: 1, y: 2 }
    );
}

#[test]
fn movement_release_or_cancel_disarms_long_press() {
    let mut ui = Ui::<4>::new(100, 100);
    let node = ui.root();
    for msg in [
        Msg::DragStart { node, axis: Axis::X, x: 30, y: 2 },
        Msg::Release { node, x: 30, y: 2 },
        Msg::Cancel { node },
    ] {
        let mut gesture = LongPressGesture::new(100);
        gesture.on_msg(&Msg::Press { node, x: 1, y: 2 });
        gesture.on_msg(&msg);
        assert!(!gesture.pending());
        assert_eq!(gesture.tick(1_000), LongPressOutcome::None);
    }
}
