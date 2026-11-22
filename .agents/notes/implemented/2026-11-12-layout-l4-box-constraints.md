# LAYOUT-L4 box constraints and intrinsic boundary

Status: implemented — 2026-11-12

## Problem

L1 could seed only four coarse root modes, L2/L3 flex and pane slots were implemented as place-local arithmetic, and the GUI-15 E6 gap had no machine-checkable min/max algebra. A full layout-kernel negotiation rewrite would risk the frozen Node/AST contracts and the existing single-pass measure cache for no immediate consumer.

## Decision

Land the constraint algebra before any kernel rewrite:

```text
BoxConstraints(min_w, max_w, min_h, max_h)
fit(w,h)
deflate(w,h)
intersect(other)
Ui::layout_box(BoxConstraints)
```

L1 remains the public compatibility face:

```text
Viewport/Tight/Loose/Infinite
    -> BoxConstraints constructor sugar
```

The four-tuple affects only the Root intrinsic boundary:

```text
root intrinsic = box.fit(child_extent)
root final = root intrinsic
```

The existing post-order measure and pre-order place structure is unchanged. There is no per-node incoming box, no second measure pass, and no flex redistribution.

## Non-goals

```text
full layout-v2 negotiation kernel
per-node incoming constraints
box-sensitive measurement
Text wrap
TextField shrink-to-width
flex basis/grow/shrink
nested measure negotiation
Grid fr
constraint serialization into AST/wire
persistent constraint cache
```

## Consequences

Arbitrary root minimums and maximums are now expressible and testable. L2 and L3 slots can be represented as pure `deflate`/`loose`/`intersect` expressions without changing their existing allocation results. Clean-tree constraint calls continue to short-circuit as documented no-ops, and damage remains clipped to the real viewport.

Final gates on the exact shipped image:

```text
cargo test -p berry-ui             PASS
just test-host                     PASS
just check                         PASS
berry-ui no_std / guest-font-vmo   PASS
uitest no_std                      PASS
GUI8 self-test                    PASS
GUI8 normal-boot                  PASS
GUI8 dual-boot canonical persistence PASS
smoke-image 138/138                PASS
cargo deny licenses                PASS
docs_guard / Curia strict         PASS
writable_globals                  PASS
```

Final budget:

```text
UITEST.bap 101,656B
rootfs.cpio 33,922,872B
rootfs0 33,554,432B
rootfs1 368,440B
total headroom 100,294,856B
```

Final image SHA-256:

```text
6499b90692732fe015ba97dac4a49f2c2b2cb679707aad5d1fd7043f8006ccaf
```

## Related

- `docs/BerryUI-LAYOUTL4BoxConstraintsIntrinsicBoundary-v1.0.md`
- `docs/BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md`
- `docs/BerryUI-L2FlexRatioAllocation-v1.0.md`
- `docs/BerryUI-L3TransposedContainerFlexListDetail-v1.0.md`
- `docs/架构决策.md` ADR #83
