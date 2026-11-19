# GUI R2 Occlusion Contract

Status: implemented — 2026-11-11

## Problem

R1 left `AVOID_SYS` with three unaligned geometry truths. Clients allocate frame VMOs from requested window bounds, while gui-wmd derived source stride/height from avoidance-clipped effective bounds. A visible SYSTEM window therefore causes row shift, bottom-row loss or horizontal shear without an error. Input localization also used the effective origin, so pointer coordinates would not match the raw buffer rows once occlusion existed.

## Decision

1. Separate the three semantics:
   - raw window bounds = buffer allocation, tight stride, layout/input coordinate basis;
   - visible bounds = AVOID_SYS hit-test and composite visibility;
   - persistent occlusion = host-granted insets, not a second buffer geometry.
2. Add `SurfaceGeometry` as a host pure contract: validate `visible ⊆ raw`, derive `visible_local`, derive insets, and clip damage to raw then visible.
3. gui-wmd SUBMIT now uses `raw_w*4` as source stride, validates VMO size against raw bounds, maps screen rows through `visible_local`, and writes only the visible region. FULL damage is shrunk to visible before copying; partial damage is clipped to raw then visible.
4. Pointer localization uses the raw origin. Hit testing still uses visible bounds.
5. APP Surface Mode 0x67 fills existing `safe_*` fields from the raw/visible difference. Values are host-granted, raw-surface-local, and checked with `surface_mode_info_fits` against raw bounds.
6. Add host-side typed inset vocabulary (`berry-guiproto::insets`) with Flutter-style `effective_padding = max(0, viewPadding - viewInsets)`. Only SystemBars has a producer in R2; Ime/cutout/gestures/overlays are reserved without fake grants.
7. Berry Room keeps raw-sized buffers, creates its window with AVOID_SYS, and performs one `WM_TABLE_GET` after CREATE to snapshot create-time occlusion; the derivation has host unit coverage. A successful 0x67 refreshes that snapshot. This is a v1 stopgap; dynamic push is R3.
8. GUI8 now creates a real SYSTEM-layer occluder only in smoke mode, creates an AVOID_SYS APP route window, proves hide/show recomputation, proves safe_top=32, submits row-indexed pixels, and pixel-checks that screen row N reads raw row N. A separate oversized-VMO partial damage probe proves pixels cannot enter a neighbor window.
9. Negative proofs cover damage wholly inside the occlusion band (`ST_BAD`) and an old effective-sized VMO smaller than raw bounds (`ST_BAD`). Normal boot machine-asserts all smoke-only occluder markers at zero.
10. Adjacent hardening: reaping a dead session forgets a TABLE_WAIT parking slot owned by that RPC channel before closing it, preventing a later snapshot bump from trapping wmd on a stale handle.

## Alternatives considered

- **Allocate Room buffers at effective bounds**: rejected. Occluder appearance/movement makes the buffer stale; occluder removal leaves no resize path at fixed FRAME_VA; it fixes only Room, not raw clients.
- **Shrink bounds and also apply safe insets**: rejected as double-counting.
- **Add configure/resize events now**: rejected. R3 must first solve per-session event queues, serial/coalescing/ack semantics and the full wmd waitset.
- **Productize full typed insets**: rejected. IME, cutout, gesture, tappable and overlay producers do not exist; freezing zero-valued wire grants would create fake truth.
- **Treat `surface_id=0` aggregate as an occluder**: rejected. Only visible SYSTEM windows participate in AVOID_SYS; the global desktop surface remains a separate aggregate path.

## Consequences

- The B6 stride mismatch is structurally closed for top, bottom, left and right occlusion.
- Occluder hide/show can change visible output without reallocating or rewriting the client buffer.
- Apps that need layout avoidance consume host-granted safe insets; the host no longer silently reinterprets their rows.
- Dynamic inset changes remain unpushed; Room's snapshot can be stale until a later 0x67 or R3 event.
- General z-order/occlusion composition and multi-window buffer declaration remain future work.

## Related

- [BerrySession-R2OcclusionContract-v1.0.md](../../../docs/BerrySession-R2OcclusionContract-v1.0.md)
- [GUI-14 多形态 UI 二轮对抗深研](../../../research/GUI-14-多形态UI二轮对抗深研-2026-11.md)
