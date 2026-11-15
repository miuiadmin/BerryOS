use berry_ui::sdk::{NavEvent, NavStack, UiError, NAV_STACK_MAX};

#[test]
fn nav_stack_is_a_pure_nonempty_route_state_machine() {
    let mut nav = NavStack::new(10).unwrap();
    assert_eq!(nav.depth(), 1);
    assert_eq!(nav.top(), 10);
    assert!(!nav.can_pop());
    assert_eq!(nav.pop(), NavEvent::None);
    assert_eq!(nav.depth(), 1);

    assert_eq!(
        nav.push(11).unwrap(),
        NavEvent::Pushed { route: 11, depth: 1 }
    );
    assert_eq!(
        nav.push(12).unwrap(),
        NavEvent::Pushed { route: 12, depth: 2 }
    );
    assert_eq!(nav.route_at(0), Some(10));
    assert_eq!(nav.route_at(2), Some(12));
    assert_eq!(nav.route_at(3), None);
    assert!(nav.can_pop());
    assert_eq!(
        nav.pop(),
        NavEvent::Popped { route: 12, depth: 1 }
    );
    assert_eq!(nav.top(), 11);

    assert_eq!(
        nav.replace(13).unwrap(),
        NavEvent::Replaced { route: 13, depth: 1 }
    );
    assert_eq!(nav.top(), 13);
    assert_eq!(nav.pop_to_root(), NavEvent::Reset);
    assert_eq!(nav.depth(), 1);
    assert_eq!(nav.top(), 10);
}

#[test]
fn nav_stack_rejects_bad_routes_and_overflow_loudly() {
    assert_eq!(NavStack::new(0).unwrap_err(), UiError::NodeBad);
    let mut nav = NavStack::new(1).unwrap();
    assert_eq!(nav.push(0).unwrap_err(), UiError::NodeBad);
    for route in 2u16..=8 {
        nav.push(route).unwrap();
    }
    assert_eq!(nav.depth(), NAV_STACK_MAX);
    assert_eq!(nav.push(9).unwrap_err(), UiError::NodeFull);
    assert_eq!(nav.replace(0).unwrap_err(), UiError::NodeBad);
    assert_eq!(nav.depth(), NAV_STACK_MAX);
    assert_eq!(nav.top(), 8);
}
