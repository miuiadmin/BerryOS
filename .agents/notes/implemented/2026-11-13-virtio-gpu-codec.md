# Agent Note: virtio-gpu Phase 1 codec foundation

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [2026-11-13-virtio-gpu-2d-mac-emulator.md](../../plans/2026-11-13-virtio-gpu-2d-mac-emulator.md), [BerryMacEmulator-HVF1-v1.0.md](../../../docs/BerryMacEmulator-HVF1-v1.0.md)

## Problem

The Mac emulator needs a real virtio-gpu display backend, but guest queue and DMA code cannot safely start from ad-hoc byte writes. The virtio-gpu control queue has many tightly packed little-endian structures, and a mistaken field offset would fail only inside QEMU after the driver service is already assembled.

## Decision

Land the wire codec first in `drivers/berry-virtio/src/gpu.rs`: exact 24-byte control headers, 16-byte rectangles, 2D create/set-scanout/flush/transfer/attach/detach commands, backing memory entries, 20-byte config decode, and the exact 408-byte display-info response decode. The format follows the Linux UAPI `virtio_gpu.h` layout, keeps the crate zero-allocation/zero-unsafe/zero-dependency, and rejects short or trailing buffers rather than truncating. Host golden and bounds tests pin every offset before any driver service exists.

## Alternatives considered

- **Write byte layouts directly in vgpud later**: mixes volatile MMIO and pure wire layout, and lets offset errors hide behind queue timing.
- **Add a third-party virtio crate**: violates the existing in-repo codec discipline and pulls an unnecessary dependency.
- **Start with 3D/virgl commands**: macOS QEMU does not provide a usable virgl path; 2D is the sanctioned and reachable backend.

## Consequences

`berry-virtio` now exposes `DEVICE_ID_GPU` and a complete tested 2D codec (16 host tests including prior input/layout suites). Phase 0 probing also established that virtio-gpu-only EDK2 GOP reports a 1080×1920 mode with framebuffer base zero, while virtio-gpu plus ramfb boots normally; therefore a guest driver is required and the codec is the foundation for `vgpud`.
