# SEM-A1 semantic tree sidecar landing

Status: implemented — 2026-11-12

## Problem

BerryUI's legacy a11y side table carried only node/role/label. Switch state was smuggled into `on`/`off` labels, Button Disabled was invisible, the P0 action bits were unused, and there was no tree-ordered semantic snapshot, parent linkage, clipped bounds export, text value/range face, or accessibility-group rule. A future screen reader or automation client needed a derived semantic tree without weakening the frozen Node/AST/P0 contracts or creating a second focus/hit/geometric authority.

## Decision

Land SEM-A1 as a runtime-only semantic overlay, not a second semantic tree authority:

```text
legacy a11y  role + label single source
SEM overlay actions + static state + value + text range + merge flag
layout      bounds and Clip visibility
C1          focusable and focused
tree walk   preorder and nearest emitted semantic parent
P0 codec    52B A11yRegion output
```

The overlay capacity is deliberately 8 entries, not another 32-entry resident table. The first implementation used a 32-entry rich side table and reproduced a real launcher stack overflow in GUI8; the final design stores only fields absent from existing authorities and keeps role/label/geometric data derived. This is a budget/architecture invariant, not an implementation convenience.

Widget synchronization:

```text
Button     ACTIVATE + DISABLED
Switch     ACTIVATE + CHECKED/DISABLED + on/off value
ListItem   ACTIVATE + CHECKED for selected
TextField  TEXT_EDIT + value + ordered GUI-T1 selection
Password   semantic value and range stay empty
Backward Shift-selection projects to an ordered semantic range
```

Snapshot rules:

```text
tree preorder
SEM overlay merges onto its legacy a11y node
bounds = absolute bounds ∩ Clip chain; empty intersection is omitted
node_id = slot + 1
parent_id = nearest emitted semantic ancestor, root = 0
FOCUS action derived from enabled C1 entry
VISIBLE/FOCUSED state derived at snapshot time
merge_descendants skips descendants only
out too small is NodeFull, never silent truncation
```

Lifecycle matches other runtime-only state:

```text
set_sem requires a live legacy a11y entry
take_a11y clears its overlay
remove(subtree) clears overlays
reset/apply_desc clears all overlays
encode_desc never serializes semantics
```

## Non-goals

```text
screen reader service
TTS or read-screen navigation
batch semantic transport
subscription or push channel
cross-process a11y service
semantic action dispatch
P0 wire change
value/text-range wire serialization
stable cross-frame semantic ids
full clearAndSetSemantics replacement policy
minimum touch target
password a11y policy beyond hiding value/range
```

## Consequences

BerryUI can now export deterministic P0-compatible semantic records while preserving the frozen AST and P0 wire. The same face can later serve automation and AI/read-screen clients without those clients parsing visual tree internals.

Final gates on the exact shipped image:

```text
just test-host                     PASS
just check                         PASS
berry-ui no_std / guest-font-vmo   PASS
uitest no_std                      PASS
GUI8 self-test                     PASS
GUI8 normal-boot                   PASS
GUI8 dual-boot canonical persistence PASS
smoke-image 138/138                PASS
cargo deny licenses                PASS
docs_guard / Curia strict          PASS
writable_globals                   PASS
```

Final budget:

```text
UITEST.bap 98,328B (+9,128B)
rootfs.cpio 33,918,464B
rootfs0 33,554,432B
rootfs1 364,032B
total headroom 100,299,264B
```

Final image SHA-256:

```text
27693390dc49e49ecc66709be9ea099e9e257bcc3490a8a879f354b5b7022135
```

## Related

- `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md`
- `docs/BerrySession-P0SurfaceMode协议-v1.0.md`
- `docs/BerryUI-C1ComponentFocusNavigation-v1.0.md`
- `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md`
- `docs/架构决策.md` ADR #81
