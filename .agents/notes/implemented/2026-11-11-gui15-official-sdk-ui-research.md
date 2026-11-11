# GUI-15 official SDK/UI primary-source research

Status: implemented — 2026-11-11

## Problem

BerryUI and the future Berry SDK had enough implementation history to expose the next architectural seams, but decisions were still at risk of being based on memory or generic UI-framework intuition. The newly created `reference/sdk-ui/` cache made a primary-source study possible, but its implications had not yet been distilled into a BerryOS decision.

## Decision

Run a workflow-mode deep research wave against the pinned official sources, with the usual backend failure fallback to three read-only specialist agents:

```text
OpenHarmony / Android / AOSP / AndroidX
Apple / Swift / Microsoft / GNOME
Flutter / Fuchsia / libadwaita
```

Record the resulting evidence and recommendation in:

```text
research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md
```

The study keeps runtime code and protocol surfaces unchanged. Its proposed next slice is:

```text
GUI-T1 TextField Foundation Before IME
SDK-S0 Control Spec & API Governance Baseline
```

followed by semantic sidecar, paired design tokens, and constraint-aware layout.

These are research labels, not route milestones. Before implementation, `GUI框架地基路线-v1.16.md` and `docs/规则/14-GUI与UI框架.md` must be amended first. The prefixed names avoid collisions with existing scheduler/capacity/compatibility labels.

GUI-T1 must explicitly resolve its key-dispatch priority against BerryUI C1: focused text state consumes Enter/Space as editing semantics before component activation. Its IME and occlusion vocabulary must remain add-only mappings to the already frozen P0 composition purposes and R2 `insets::Ime`.

## Alternatives considered

- **Proceed directly to form/navigation features**: rejected; official sources consistently place text focus, semantics, lifecycle, and API governance before productized form/navigation.
- **Adopt an ArkUI/Compose-style reactive DSL immediately**: rejected; V1/V2 state-management migration costs, deep observation semantics, and frame iteration budgets show that reactive syntax must wait for explicit ownership and rebuild contracts.
- **Copy Flutter/Compose tree internals wholesale**: rejected; Berry's current retained tree plus runtime side tables is viable, while slot tables and link buffers are optimization details that do not address Berry's current text/a11y/API gaps.
- **Treat Apple HIG or Material 3 pages as local fact sources**: rejected; their licenses do not permit mirroring. The report uses link-only entries and separately evidenced Swift/WinUI/GNOME sources.
- **Immediately rewrite layout v2**: deferred; constraints/intrinsic work is necessary, but text intrinsic measurement should enter the model first to avoid another blind layout rewrite.

## Consequences

The research report now provides an official-source-backed decision matrix for state management, lifecycle, focus, text, accessibility, layout, embedding, frame feedback, tokens, API versioning, documentation, and tooling.

It confirms the existing authority split:

```text
gui-wmd / Room own window, surface, input, and frame authority
BerryUI owns in-tree focus, layout, text state, and semantics
apps own business state and selection/navigation policy
```

It identifies concrete future invariants and experiments, including stable keys, a lifecycle state machine, constraint ranges, single-pass measurement, focus side trees, semantic sidecars, paired color tokens, window capability records, and presented-frame feedback.

Two read-only adversarial audits found no blockers. The source-fact audit verified the pins and core platform claims while correcting two Fuchsia paths, the OpenHarmony keyboard-avoid enum, one BerryUI spec filename, and the WinUI spec-template provenance. The Berry-decision audit required the route/key/IME/AST/budget guards recorded above.

## Related

- `research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md`
- `reference/sdk-ui/README.md`
- `reference/sdk-ui/sources.toml`
- `reference/sdk-ui/LICENSE-NOTES.md`
- `docs/阶段沿革.md`
