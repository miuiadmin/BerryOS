# BerryUI L1 Local Constraints & Adaptive Slot

Status: implemented — 2026-11-11

## Problem

`Constraints::{Tight,Loose,Infinite}` existed only as dead vocabulary. Non-Viewport values panicked in debug and silently fell back to Viewport in release guests. GUI-14 B10/L1 therefore had no honest local layout basis, and nested adaptive UI had no way to avoid reading the global screen.

## Decision

1. Keep v1 measure constraint-blind. A constraint may seed only the Root of one `relayout` pass.
2. Implement:
   - `Viewport`: existing viewport box, byte-for-byte unchanged;
   - `Tight { w, h }`: exact root box;
   - `Loose { w, h }`: root stack-child envelope clamped by the payload;
   - `Infinite`: root stack-child envelope without a payload cap.
3. Add stateless `Ui::layout_with(Constraints)`.
   - It does not remember the previous constraint.
   - A clean-tree call is a documented `{0,0}` no-op.
   - Constraint switching requires an explicit geometry dirty source.
   - `render_frame` remains Viewport-only.
4. Keep damage clipping tied to the real viewport for every constraint. Damage and paint remain clipped to the real viewport; hit uses the Root placed box (equal to viewport under Viewport). Tight/Loose/Infinite do not create a new clip authority; local clipping remains the existing `Clip` node.
5. Add SDK `slot_variant(basis_w, threshold)`.
   - `basis_w >= threshold` -> Expanded.
   - The basis is caller-provided and may be raw or usable width.
   - The function has no Ui/Room/context parameter, so implicit global-screen or device-class reads are impossible at compile time.
6. Keep adaptive selection outside the retained tree in v1. A variant flip uses the existing remove/build/layout/render path. A resident dual-subtree slot would leave hidden focusables/a11y entries in C1 order and double the arena.
7. UITEST proves an Expanded sample using local card width 1056 and threshold 600, plus a Compact decision at explicit basis 320.

## Alternatives considered

- **Implement flex/wrap now**: rejected; wrapping intrinsic size depends on incoming width and requires constraint-aware measure, scheduled as layout v2.
- **Resident AdaptiveSlot container**: rejected; breaks C1 focus/a11y/arena invariants and duplicates subtrees.
- **ListDetail now**: rejected; it decomposes into local slot selection plus selection/routing/back state and has no current gate consumer.
- **Expose Constraints without implementing them**: rejected; SDK admission requires a real host-tested contract, not debug-only vocabulary.
- **Clip damage to the constraint box**: rejected; paint already uses the real viewport (hit follows the Root placed box), and legitimate content may overflow the local box.

## Consequences

- Existing Viewport apps and tests remain byte-equivalent.
- Release guests no longer silently degrade Tight/Loose/Infinite to Viewport.
- Local adaptive decisions can be made from an explicit width without FormMetrics or device branching.
- Loose shrink-wrap has a documented wart: stretchy None/None children measure zero. Callers must use sized or intrinsic children until layout v2.
- Flex/wrap, ListDetail, Grid breakpoints, WindowSizeClass ladders, SafeArea, dp thresholds, subtree constraints, and automatic Room config pumping remain deferred.

## Related

- [BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md](../../../docs/BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md)
- [BerryUI-C1ComponentFocusNavigation-v1.0.md](../../../docs/BerryUI-C1ComponentFocusNavigation-v1.0.md)
