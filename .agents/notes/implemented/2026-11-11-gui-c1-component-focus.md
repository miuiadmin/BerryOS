# BerryUI C1 Component Focus & Navigation

Status: implemented — 2026-11-11

## Problem

The keyboard chain ended at the app. gui-wmd owns window focus and routes keys to the focused window, but BerryUI had only pointer messages and no in-tree focus target. Multi-form and eventual TV/keyboard-first interfaces require a component focus layer without creating a second window-focus authority.

## Decision

1. Keep the D7 layer split:
   - gui-wmd: window activation / seat focus / key routing;
   - BerryUI: component focus inside one retained tree;
   - future layer 3: exclusive/top arbitration.
2. Keep Node frozen at 19 fields. Component focus is a Ui side table plus one `focused: Option<NodeId>`.
3. Focusable entries contain:
   - semantic node;
   - optional visual node to invalidate;
   - enabled flag.
4. Capacity is 32. Overflow is `UiError::NodeFull`, never silent drop.
5. `Button`, `Switch`, and `ListItem` register their panel as focusable. Containers and passive primitives are not focusable.
6. Focus order is tree pre-order, not registration order. `Next` and `Prev` wrap, matching Compose 1D Tab semantics. Disabled and dangling nodes are skipped.
7. Msg is extended additively with:
   - `FocusIn { node }`;
   - `FocusOut { node }`.
   A move emits Out before In; a no-op emits nothing.
8. Activation reuses `Msg::Click { focused_node }`. There is no second activation message.
9. SDK key mapping is pure:
   - Tab -> Next (DOWN only, no REPEAT);
   - Shift+Tab -> Prev;
   - Enter/Space -> Activate;
   - arrows and letters -> None.
10. The app consumes `Room::poll_keys`, maps keys, moves focus, handles FocusIn/Out, synthesizes Click for activation, then renders and submits. Engine does not add a key buffer.
11. Focus visuals use a new `InteractionState::Focused`, applied through `set_props` and normal damage. The engine never paints a second focus ring.
12. Local a11y state exposes `VISIBLE | FOCUSED`; AST/wire and a11y batch transport stay unchanged.

## Alternatives considered

- **Store focus on Node**: rejected; Node is frozen at 144 bytes.
- **Let wmd or Room own component focus**: rejected; they have no UI tree and would become a second authority.
- **Derive focusability from a11y roles**: rejected; tab-stop, disabled state, and semantic role are different concerns.
- **Geometry/directional focus now**: rejected; it requires post-layout bounds, tie-breaking, and clip visibility. Tree order is layout-independent and testable first.
- **FocusGroup/FocusRestorer now**: rejected; no virtual list or Route stack exists yet, and page rebuild invalidates stored NodeIds.
- **Add an Activate message**: rejected; `Click` already defines activation for Button and Switch.

## Consequences

- Keyboard users can traverse Button/Switch/ListItem in authoring order and activate through the existing Click path.
- Component focus survives window focus loss as app memory; no window-loss notification is claimed.
- Pointer press does not implicitly move component focus in v1.
- UITEST proves two real Tab presses move Button -> Switch and two Enter presses toggle and restore the Switch.
- The first BerryUI focus slice is intentionally 1D. 2D focus, groups, restorers, text-field behavior, and modal capture remain future work.

## Related

- [BerryUI-C1ComponentFocusNavigation-v1.0.md](../../../docs/BerryUI-C1ComponentFocusNavigation-v1.0.md)
- [BerrySession-R3ConfigEventArchitecture-v1.0.md](../../../docs/BerrySession-R3ConfigEventArchitecture-v1.0.md)
