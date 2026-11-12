# DESIGN-D2 token pair contract landing

Status: implemented — 2026-11-12

## Problem

Berry Design F3 exposed eleven semantic colors and five state triples, but there was no structural requirement that a background travel with its foreground, no dark/high-contrast scheme matrix, no WCAG math, and no machine gate preventing unreadable pairs. `tools/gui8-smoke.py` also carried a second hand-copied palette for six apps' pixel anchors, so a theme value could drift from its test oracle.

## Decision

Land DESIGN-D2 as an additive token-pair contract:

```text
legacy F3 light palette + state table   frozen byte-for-byte
ColorPair                              mandatory background + foreground
ColorScheme                            light / dark / high-contrast
ColorSchemePreference                  forced or system-snapshot resolution
WCAG contrast ratio                    integer Q16 LUT, no std/no alloc
ColorPairToken                         container + status pairs
InteractionState pairs                 state background/content as pairs
TextScale                              discrete FontSize ladder, not multiplication
ThemeMetrics::effective_px             quarter-vp rounding single source
```

The design ledger at `tools/design_tokens.toml` contains all 36 scheme/token pairs. `tools/design_ledger.py --check` enforces:

```text
pair uniqueness and full scheme matrix
active text pairs >= 4.5:1
explicit WCAG inactive exemption for Disabled
source enum/token vocabulary parity
RGB values physically present in theme.rs
no Color RGB literal outside theme.rs
generated spec matrix byte parity
```

The gate is wired into `just check` as `design-ledger-check`.

F3 light bytes remain frozen. Dark and high-contrast schemes are additive and are not migrated into the six production apps in this knife.

## Non-goals

```text
system theme-setting service
new R3 theme event
five-app dark/high-contrast reskin
Material HCT or dynamic-color copy
continuous font-size multiplication
elevation tokens
continuous scale-permille density
OEM brand themes
migration of PRESSED_BG and swipe dot legacy constants
```

## Consequences

New visual backgrounds/status colors must enter the pair ledger with their foreground and contrast level. GUI8 now parses `UITESTTOKEN` from the guest and derives its BerryUI pixel anchors from that dump, eliminating the harness's second palette copy. The token marker appears exactly twice on normal boot (boot probe and runtime instance).

Final gates on the exact shipped image:

```text
just test-host                     PASS
just check                         PASS
berry-ui no_std / guest-font-vmo   PASS
uitest no_std                      PASS
GUI8 self-test                    PASS
GUI8 normal-boot                  PASS
GUI8 dual-boot canonical persistence PASS
smoke-image 138/138               PASS
cargo deny licenses               PASS
docs_guard / Curia strict         PASS
writable_globals                  PASS
```

Final budget:

```text
UITEST.bap 100,184B
rootfs.cpio 33,921,888B
rootfs0 33,554,432B
rootfs1 367,456B
total headroom 100,295,840B
```

Final image SHA-256:

```text
0dba6d08168cf496190276afc9c6a93cddb7cd4443f2a70c33bc62fe98a13226
```

## Related

- `docs/BerryDesign-D2TokenPairContract-v1.0.md`
- `docs/BerryDesign-F3主题与基元-v1.0.md`
- `docs/GUI框架地基路线-v1.18.md`
- `docs/架构决策.md` ADR #82
- `tools/design_tokens.toml`
- `tools/design_ledger.py`
