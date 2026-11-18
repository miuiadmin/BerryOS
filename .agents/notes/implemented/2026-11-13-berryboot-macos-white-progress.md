# Agent Note: BerryBoot macOS white progress correction

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [BerryBoot-BootProgress-P1-v1.0.md](../../../docs/BerryBoot-BootProgress-P1-v1.0.md), [Berry设备平台-CLI-first设计-v1.9.md](../../../docs/Berry设备平台-CLI-first设计-v1.9.md), [2026-11-13-mac-single-window-system-nav.md](2026-11-13-mac-single-window-system-nav.md)

## Problem

The first native Boot Progress face had two defects visible in the product window. The intended blue `#0074E0` accent reached QEMU as orange because the renderer passed UEFI raw format 1 into `PixelFormat::Bgrx`, while the actual ramfb and virtio-gpu surfaces are little-endian XRGB8888. The temporary geometric percentage compounded the error: incomplete glyph segments looked like garbled characters, and supersampling a bad shape did not make it crisp.

## Decision

Treat the XRGB byte order as part of the boot display contract. BerryBoot and vgpud now instantiate the progress renderer with raw format 0, which maps to `PixelFormat::Rgbx.pack` and writes the correct B,G,R,X bytes for an intended RGB tuple; the renderer rejects any other raw format. A host test pins the pure-white fill bytes as 255,255,255 rather than merely asserting a nonzero pixel.

Change the visible boot face to the macOS model: pure black background, white BerryOS mark, dark track, and white progress fill. Remove both percentage and stage text from rendering. `Stage` and permille remain the protocol truth for state transitions, but they no longer become boot-screen text. The vgpu incremental damage is therefore only `1080×20@1305` for the product geometry and `800×12@432` for GUI8.

## Alternatives considered

- **Keep blue and swap channels only**: rejected by the product decision; the requested progress bar is macOS-style white.
- **Repair and keep geometric percentages**: rejected; even smooth coverage cannot fix the malformed glyph geometry, and the desired boot face has no percentage.
- **Use the scaled 8×16 bitmap percentage again**: rejected; that was the original blurry/garbled path.
- **Keep stage labels**: rejected for the boot face; they are diagnostics, not macOS-style system chrome.
- **Hide the defect behind host-side color correction**: rejected; two pixel truth sources would make soft/vgpu consensus invalid.

## Consequences

Host tests now assert the exact white XRGB fill and a black region below the bar. A live 1080×1920 HVF run captured the white 70% face on both ramfb and the real vgpu primary console; pixel analysis found only the white mark, white fill, dark track, and black background during progress, followed by the normal desktop. Serial truth still advances through 450, 520, 560, 700, and 1000 permille and ends with `LAUNCH ok` without a kernel panic.
