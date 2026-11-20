# Agent Note: LAYOUT-L6a Flex Spec Algebra

Status: implemented — 2026-11-12

## Problem

L2/L3 flex only had grow weight with an implicit zero basis. Real form and toolbar layouts need Px/Auto basis and the ability to shrink without letting fixed children move or the main axis stop filling.

## Decision

Keep flex runtime-only. Extend `FlexEntry` with `shrink` and `FlexBasis`, and expose `FlexSpec` through `Ui::set_flex_spec` / `Ui::flex_spec`. `set_flex(weight)` remains a frozen sugar for grow + `Px(0)` + shrink 0. Allocation stays in the existing place pass: positive free space is distributed by cumulative-prefix grow, negative free space by capped proportional shrink with a freeze loop. A freezing pass rolls back un-frozen provisional takes so capped base plus provisional take cannot exceed the deficit.

## Alternatives considered

- **Express flex in Props/AST**：would break the frozen AST/wire and GUI8 anchors before a real protocol consumer exists；rejected.
- **Run a second measure pass**：violates the one post-order measure contract and CountingMetrics budget；rejected.
- **Scale all shrink children in one linear pass**：over-takes when one child is capped and can produce incorrect total slot；rejected in favor of the freeze loop.
- **Wire a temporary UITEST flex probe now**：L6a has no production guest consumer and a temporary tree mutation adds real-chain noise；deferred to L6b.
- **Implement nested negotiation / Grid fr in the same knife**：expands scope before the basic algebra is pinned；owned by L6b.

## Consequences

L2 1:2 and L3 transposed geometries are unchanged. Flex specs survive keyed re-apply by NodeId, full rebuild clears them, and measured extents stay slot-independent. The slice is deliberately host-only: GUI8 proves zero drift of existing markers rather than adding UITESTL6. The first real guest consumer and wrap-to-flex-slot behavior belong to L6b.

Adversarial audit corrections landed with this note: a freezing pass now rolls back un-frozen provisional takes; place-time Col/Row walks use the same alive-chain break as the planning walk; the structurally unreachable over-cap flex arm has a debug assertion; and `FlexEntry` records its added-field breaking history with successor `FlexSpec`.

## Related

- [BerryUI-LAYOUTL6aFlexSpecAlgebra-v1.0.md](../../../docs/BerryUI-LAYOUTL6aFlexSpecAlgebra-v1.0.md)
- [GUI框架地基路线-v1.20.md](../../../docs/GUI框架地基路线-v1.20.md)
- [架构决策.md](../../../docs/架构决策.md) ADR #87
