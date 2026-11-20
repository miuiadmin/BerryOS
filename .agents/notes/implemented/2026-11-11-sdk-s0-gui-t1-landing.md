# SDK-S0 and GUI-T1 landing

Status: implemented — 2026-11-11

## Problem

GUI-15 established the next SDK/UI foundation slices, but the repository still lacked the governing route rows, control-spec process, API ledger, and real text-editing primitive. Without those, future controls could drift by intuition and C1 keyboard semantics would collide with TextField editing.

## Decision

Land the two slices together:

```text
SDK-S0  Control Spec & API Governance Baseline
GUI-T1 TextField Foundation Before IME
```

SDK-S0 introduces:

```text
16-section BerryUIControl spec template
TextField as the first control spec
tools/sdk_api.toml as the 81-item facade ledger
tools/sdk_ledger.py --check as a just-check gate
generated availability matrix
no-successor deprecation ban
```

GUI-T1 introduces:

```text
TextInputState with valid UTF-8 and codepoint selection/caret
hardware edit commands and plain-data outcome flags
key_route arbitration: text Enter/Space before C1 Activate
TextField as a Panel/Clip/Rect/Text composition
P0 IME purpose add-only mapping
R2 insets::Ime overlap data face
```

No new ViewKind, Props, Msg, or wire format was added. Window focus remains in `gui-wmd`; component focus remains C1; text value remains app-owned.

## Alternatives considered

- **Implement the soft keyboard or IME Host first**: rejected; text state, selection, submit, and focus priority must be testable before an input method is layered on top.
- **Add a TextField ViewKind/Props payload**: rejected; it would break the frozen AST/wire contract and duplicate what the existing composition primitives already express.
- **Use callbacks/FormMetrics**: rejected; BerryUI remains plain-data and app-owned state with no callback framework.
- **Make the API ledger human-only Markdown**: rejected; facade drift would not be machine-detected.
- **Register all root re-exports instead of the facade**: rejected; the facade is the intended app contract and keeps the initial ledger bounded.

## Consequences

The SDK facade is now 100% inventoried and machine-checked. New interactive BerryUI controls require a per-control spec before export. TextField provides a real hardware-editing path while IME remains explicitly deferred.

GUI8 now proves:

```text
TextField joins tree-preorder focus order at index 4
A/Space/A/Backspace edit sequence reaches "a "
Left/Right restore caret to 2
Enter submits without toggling the focused Switch
IME inset producer remains zero
AST grows deliberately to 860 bytes with a11y=9
```

The rootfs remains within its total cap, with rootfs0 still exactly at the 32MiB single-volume boundary.

Final gates on the exact shipped image:

```text
just test-host                  PASS
just check                      PASS
berry-ui/uitest no_std           PASS
GUI8 self-test                  PASS
GUI8 normal-boot                PASS
GUI8 dual-boot layout self-heal PASS
smoke-image 138/138             PASS
cargo deny licenses             PASS
docs_guard / Curia strict       PASS
writable_globals                PASS
```

Final image SHA-256:

```text
66aa68fe21e930a338080bd2dc1d71776b27cff2dd5e669b6cd439f934f9bb41
```

Adversarial audit corrections were applied: the SDK parser now handles brace-less and multi-line re-exports, `RectI` brings the facade to 81 entries, module/deprecation rules are enforced, printable insertion is restricted to DOWN/REPEAT, and IME overlap clamps field bounds to the viewport.

## Related

- `docs/BerryUI-SDKS0ControlSpecAPIGovernance-v1.0.md`
- `docs/BerryUIControl-TextField-v1.0.md`
- `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md`
- `docs/架构决策.md` ADR #79/#80
- `research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md`
