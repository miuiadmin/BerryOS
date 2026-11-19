# BerryUI L2 Flex Ratio Allocation

Status: implemented — 2026-11-11

## Problem

L1 proved an explicitly local adaptive basis, but the expanded pane widths were still app-authored constants. GUI-14 needed the first honest engine-owned ratio allocation without prematurely rewriting the entire layout engine.

## Decision

1. Keep `Node` frozen at 19 fields and keep AST/wire untouched.
2. Store flex weights in a runtime `Ui` side table:
   - `FlexEntry { node: NodeId, weight: u16 }`
   - capacity 32
   - overflow is `UiError::NodeFull`.
3. Keep measure constraint-blind. Flex is place-only and never writes `meas_w/meas_h`.
4. Support only fixed-main-axis `Row` and `Col` parents.
5. Restrict v1 eligible children to `Rect`/`Circle` with both dimensions `None`; their existing `final_size` consumes the allocated avail without a layout-engine rewrite.
6. Allocate remaining main-axis space with cumulative-prefix floor arithmetic:
   - subtract all gaps and inflexible child intrinsics;
   - clamp negative space to zero;
   - each flex slot is the difference between consecutive cumulative-weight boundaries;
   - all slots sum exactly to the available flex space;
   - the final flex child absorbs integer remainder.
7. Changing or removing a weight dirties the child and ancestor chain so all siblings re-place.
8. A same-value registration is a zero-op.
9. `remove()` clears subtree entries; `reset_arena()`/`apply_desc()` start with an empty flex table.
10. Flex is intentionally not serialized. It is a runtime layout concern like the C1 focus side table; an AST rebuild requires the app to re-register weights. Host tests pin byte-identical AST before and after registration.
11. UITEST upgrades its adaptive panes from fixed 360/684 to flex 1:2 over 1044 usable pixels, producing 348/696.

## Alternatives considered

- **Add a Node field**: rejected; Node is frozen at 144 bytes.
- **Extend every Props variant**: rejected; large constructor/test churn and risks Node size/golden drift.
- **Add a wire block now**: rejected for this slice; no remote flex consumer exists, and the runtime-only contract is explicitly pinned by AST byte equality.
- **Allocate in widgets**: rejected; duplicates ratio/damage/hit logic and forces apps to know available width outside the engine.
- **Rewrite constraint-aware measure**: rejected; fixed-main ratio does not need intrinsic-as-function-of-width. Wrap and intrinsic flex remain layout v2.
- **Support transposed/same-orientation containers now**: rejected; keep the first slice to leaves whose final size already consumes allocated avail.

## Consequences

- Existing unregistered trees retain byte-for-byte layout behavior.
- Fixed and flex children can mix; gaps and remainder are deterministic.
- Zero/negative free space collapses flex children without shrinking fixed children.
- `CrossAlign::Stretch` is the intended cross-axis mode.
- The motivating adaptive panes now demonstrate true engine-owned ratio allocation.
- Apply/desc rebuild still requires re-registering weights.
- Basis/grow-shrink split, min/max, wrap, nested negotiation, Grid `fr`, Panel flex, and ListDetail remain future work.

## Related

- [BerryUI-L2FlexRatioAllocation-v1.0.md](../../../docs/BerryUI-L2FlexRatioAllocation-v1.0.md)
- [BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md](../../../docs/BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md)
