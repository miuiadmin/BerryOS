# Agent Note: vgpud HDI foundation and backing pool

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [2026-11-13-virtio-gpu-hdi-pool-calibration.md](../../plans/2026-11-13-virtio-gpu-hdi-pool-calibration.md), [BerryMacEmulator-GPU2-v1.0.md](../../../docs/BerryMacEmulator-GPU2-v1.0.md)

## Problem

GPU1 gave the Mac product desktop a real virtio-gpu present backend, but vgpud answered every HDI request `ST_UNSUPPORTED`. Frame backing was a single resource keyed only by the first physical page, so a destroyed VMO whose pages were reused could cause a false pool hit. There was also no deterministic way to test the four HDI operations or compare soft and vgpu pixels without turning every product boot into a CI run.

## Decision

Implement the display contract inside vgpud instead of adding a client/demo workaround. REG_VBLANK and VBLANK_WAIT follow the soft-vsync timing model; PREPARE validates geometry and returns a real fence; COMMIT queues the slot resource and releases its fence only after full-slot TRANSFER and FLUSH. The service wait loop uses `min(next vblank, idle cap)` and preserves the zero-wake idle invariant.

Frame backings use three virtio-gpu resources: one for the wmd composition VMO and two HDI slots. Entries combine first/last physical-page fingerprints, LRU replacement, and a READ alias of the frame VMO that pins all committed pages. Empty resources are not detached; QEMU returns `0x1200` for that otherwise harmless-looking call and kills the service.

A diagnostic `gpuhdi.on` marker is deliberately excluded from official product and verification images. `just gpu2-hdi-image` builds a product diagnostic image, and `just gpu2-vgpu-image` builds the GUI8 consensus image. GUI8 `--vgpu` attaches a real 800×600 virtio-gpu, requires `VGPU ok` plus `VGPUHDI ok`, directs screendump to `device=vgpu0`, and keeps every existing pixel/interaction assertion. Mac dual-arm latency data is collected by `tools/desktop-calib.py`; `VGPUPERF` prints after SUBMIT ACK so serial telemetry is not counted as guest latency.

## Alternatives considered

- **Leave HDI unsupported until a normal app consumes it**: would leave ADR #50's reserved backend contract untestable and make async presentation impossible to validate.
- **Signal COMMIT fence immediately**: rejected; it destroys completion semantics and hides presentation failures.
- **Use only first PA as pool identity**: rejected; page allocator reuse can create a false hit.
- **Detach every selected resource before attach**: rejected after QEMU returned `0x1200` for never-attached resources 2/3.
- **Put the diagnostic marker in every product image**: rejected; product boot must not allocate a full-size test VMO or run CI probes.
- **Claim full GPU rasterization**: rejected; BerryGFX raster remains guest CPU. GPU2 defines and verifies the real display framework and measures its current bottleneck.

## Consequences

The 1080×1920 diagnostic image reaches `VGPU ok`, `VGPUHDI ok vblank=1 fence=1 presented=1`, `LAUNCH ok`, with no guifbd fallback or panic. GUI8 can now run the same pixel and interaction authority over soft ramfb and virtio-gpu topologies. Current Mac data still shows QEMU's full-window 8.3MiB 2D transfer as the dominant cost; that boundary is recorded by the GPU2 standard rather than hidden.
