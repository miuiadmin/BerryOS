# Agent Note: vgpud HDI foundation and backing pool

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [2026-11-13-virtio-gpu-hdi-pool-calibration.md](../../plans/2026-11-13-virtio-gpu-hdi-pool-calibration.md), [BerryMacEmulator-GPU1-v1.0.md](../../../docs/BerryMacEmulator-GPU1-v1.0.md)

## Problem

GPU1 gave the Mac product desktop a real virtio-gpu present backend, but vgpud answered every HDI request `ST_UNSUPPORTED`. Frame backing was a single resource keyed only by the first physical page, so a destroyed VMO whose pages were reused could cause a false pool hit. There was also no deterministic way to test the four HDI operations without making every product boot a CI run.

## Decision

Implement the system HDI semantics in vgpud rather than adding a demo/client workaround: REG_VBLANK and VBLANK_WAIT follow the guifbd soft-vsync timing model; PREPARE validates full 1080×1920 XRGB geometry and returns a real fence; COMMIT defers fence release to the next vblank or target-later tick. The service wait loop uses `min(next vblank, idle cap)` and preserves the zero-wake idle invariant.

Frame backings now use two virtio-gpu resources with first/last physical-page fingerprints and LRU replacement. Each pooled entry holds a READ alias of the frame VMO, pinning its committed pages and preventing physical-page reuse from creating a false identity. Scanout switches resources only on a pool miss/hit transition.

A diagnostic-only `gpuhdi.on` marker and `just gpu2-hdi-image` build a product image that runs REG_VBLANK→VBLANK_WAIT→PREPARE→COMMIT→fence wait→REG_VBLANK off and prints `VGPUHDI ok vblank=1 fence=1`. Official product and verification images do not contain the marker. Every 128 SUBMITs now emit `VGPUPERF` averages for submit time, reattach count, and vblank jitter.

## Alternatives considered

- **Leave HDI unsupported until a normal app consumes it**: would leave the second backend without the contract ADR #50 reserved for it.
- **Signal COMMIT fence immediately**: easier, but destroys presentation ordering and makes the fence meaningless.
- **Use only first PA as pool identity**: rejected; page allocator reuse can create a false hit.
- **Put the diagnostic marker in every product image**: turns a developer product into a boot test and adds an 8.3MiB transient VMO to every startup.

## Consequences

The diagnostic 1080×1920 HVF image reaches `VGPU ok`, `VGPUHDI ok`, `LAUNCH ok`, with no `GFXF` fallback and no panic. Official product images remain marker-free. Remaining GPU2 work is queued by the active plan: COMMIT-driven pixel presentation, soft↔vgpu pixel-consensus CI, Mac dual-arm calibration, and a shared guiproto signal-bit constant.
