# BerryUI L3 Transposed Container Flex & ListDetail

Status: implemented — 2026-11-11

## Problem

L2 flex weights were limited to `Rect`/`Circle` double-`None` leaves. That proved deterministic ratio allocation but could not host content inside each ratio slot, so a real list/detail pane still required app-authored fixed widths.

## Decision

1. Keep the L2 allocation arithmetic, measure cache, damage, hit, Node, AST, and wire surfaces unchanged.
2. Add one orientation-aware eligibility predicate, shared by:
   - `Ui::set_flex` write-side rejection;
   - `Ui::set_props` drift teardown;
   - layout read-side stale-entry filtering.
3. Allow these transposed pane forms:
   - under a `Row`: `Col`, `Grid`, or `Panel` with width `None`;
   - under a `Col`: `Row` with height `None`.
4. Preserve the L2 leaf contract: `Rect`/`Circle` must still have both dimensions `None`.
5. Reject same-orientation containers, `Clip`, `Pad`, intrinsic leaves, zero weights, fixed share-axis sizes, dangling ids, and non-`Row`/`Col` parents.
6. Add a stateless `ListDetail` primitive:
   - `ListDetailSpec` is plain data;
   - construction creates one fixed-main `Row` and two content-bearing `Panel` panes;
   - runtime flex weights are registered on those panes;
   - the app owns content, selection, compact/expanded branching, and navigation.
7. Keep pane focus/a11y behavior delegated to content. The panes themselves are not focusable and carry no a11y entries.
8. Keep flex runtime-only. AST rebuild drops weights and requires reconstruction or re-registration.

## Alternatives considered

- **Use Rect panes only**: rejected; leaves cannot host child content.
- **Make ListDetail stateful**: rejected; selection and back/route state cross the established app-owned-state boundary.
- **Allow same-orientation container flex**: rejected; the allocated share would be overwritten by the container's intrinsic main-axis size, creating a silent dead weight.
- **Allow Clip/Pad now**: deferred; they mechanically follow avail but expand the semantics and asymmetric same-orientation surface without a current consumer.
- **Serialize flex weights**: deferred; no remote flex consumer exists and runtime-only behavior is byte-pinned.
- **Rewrite constraint-aware measure**: deferred; nested fixed-main flex already composes without changing intrinsic measurement.

## Consequences

- A pane can consume a ratio slot and propagate the resulting geometry into its subtree.
- ListDetail can host list and detail content while remaining stateless.
- Existing 19 L2 flex tests remain unchanged and green.
- A collapsed non-Clip pane may have a hittable non-zero child subtree; this is existing hit semantics and is now documented and tested.
- Same-orientation flex, Grid `fr`, basis/grow/shrink, min/max, wrap, intrinsic flex, navigation state, and wire serialization remain future work.

## Related

- [BerryUI-L3TransposedContainerFlexListDetail-v1.0.md](../../../docs/BerryUI-L3TransposedContainerFlexListDetail-v1.0.md)
- [BerryUI-L2FlexRatioAllocation-v1.0.md](../../../docs/BerryUI-L2FlexRatioAllocation-v1.0.md)
