# Agent Note: BerryControls D1 Rich Controls Demo

Status: implemented — 2026-11-12

## Problem

The GUI line had solid protocol and widget foundations but no user-facing showcase. UITEST is an acceptance harness, not a product-like controls gallery. Continuing only downward into protocol work risked building infrastructure that users could not see or evaluate.

## Decision

Add a separate `CONTROLS` BerryUI app instead of expanding UITEST. It runs as a normal desktop app through Berry Room, has a generated icon and Chinese desktop label, and exposes five product-like pages: Gallery, Buttons & States, Inputs, Lists & Panes, and Theme & Density. It uses only existing BerryUI SDK widgets and app-side composition. APP capacity was widened from 9 to 10, svcmgr emits two-digit app counts, and the launcher boot RPC readable cap was raised from 300s to 600s because the TCG boot batch now exceeds the old cap before appd reaches steady service.

## Alternatives considered

- **Expand UITEST**：would destabilize frozen semantic counts, focus order, AST bytes, and GUI8 anchors; rejected.
- **Use one of Browser/Settings/Files/Photos/Contacts**：each has a domain-specific role and data contract; a controls gallery would muddy their acceptance semantics.
- **Build a canvas demo**：would bypass retained widgets and recreate a second UI path; rejected.
- **Defer until more protocol knives land**：contrary to the requested product-direction correction; rejected.
- **Add new widget APIs in the same knife**：would couple a user-facing demo to SDK drift; deferred until the demo exposes concrete needs.

## Consequences

CONTROLS is a normal retained BerryUI app: no surface escape, no WM RPC parsing, no VMO mapping, no stride or pixel writes. It owns a constant demo IconSource for three 48×48 graphics. Desktop launch and GUI8 now prove the app as the seventh real BerryUI app. The icon residency budget has 7,896px left—less than one more 108×108 icon—so future app icons must recompute that cap before landing.

Adversarial audit corrections landed with this note: input captions are now emitted above each field, `CONTROLSFOCUS` reports the tapped field index, Clear/Restore force a same-page rebuild, and the known deepest-node drag-dispatch boundary is documented rather than silently ignored.

## Related

- [BerryControls-D1RichControlsDemo-v1.0.md](../../../docs/BerryControls-D1RichControlsDemo-v1.0.md)
- [GUI框架地基路线-v1.20.md](../../../docs/GUI框架地基路线-v1.20.md)
- [架构决策.md](../../../docs/架构决策.md) ADR #88
