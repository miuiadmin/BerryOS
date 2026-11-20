# Agent Note: Mac emulator GPU1 service backend

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [BerryMacEmulator-GPU1-v1.0.md](../../../docs/BerryMacEmulator-GPU1-v1.0.md), [BerryMacEmulator-HVF1-v1.0.md](../../../docs/BerryMacEmulator-HVF1-v1.0.md), [2026-11-13-virtio-gpu-2d-mac-emulator.md](../../plans/2026-11-13-virtio-gpu-2d-mac-emulator.md), [架构决策.md](../../../docs/架构决策.md) ADR #91

## Problem

HVF1 fixed the 1080×1920 Mac product experience and partial damage, but guifbd still moved every submitted frame through row-by-row VMO bounce copies into ramfb. QEMU exposed virtio-gpu, yet BerryOS had no guest driver and EDK2 could not hand off a usable virtio-gpu-only framebuffer. A display patch inside guifbd would make backend selection a client-facing branch and risk weakening the deterministic GUI8 baseline.

## Decision

Implement virtio-gpu as a second real display service. `vgpud` owns MMIO negotiation, the control queue, 1080×1920 resource creation, backing, scanout, transfer, flush, and the product present protocol. Backend selection happens in svcmgr: after `vgpud` sends a ready byte, svcmgr hands the existing present/hdi client endpoints to gui-wmd; otherwise guifbd and ramfb remain the verification/fallback path. The present wire, Berry Room, gui-wmd, and the eight-handle process cap do not change.

The first submitted frame VMO is mapped with 2,025 four-KiB `virtio_gpu_mem_entry` records supplied through a descriptor chain. Later aliases of the same VMO reuse that backing. Each FULL or damage rectangle is sent as `TRANSFER_TO_HOST_2D` plus `RESOURCE_FLUSH`; no row bounce copy runs in guifbd for the product path.

## Alternatives considered

- **Add an if/else vgpu branch inside guifbd**: couples ramfb verification and product backend lifecycle, and eventually drags queue state into the CI baseline service.
- **Widen `CHAN_HND_MAX` and give guifbd both MMIO windows**: a kernel ABI change to avoid a service boundary, contrary to ADR #61's separation precedent.
- **Use QEMU `apple-gfx-pci`**: it can be instantiated, but EDK2/BerryBoot receives no usable framebuffer and the guest would need Apple's proprietary ParavirtualizedGraphics protocol.
- **Claim full GPU rasterization**: rejected; BerryGFX raster remains guest CPU. GPU1 accelerates display transport/presentation through virtio-gpu and is honest about that boundary.

## Consequences

The Mac product boots with `VGPU ok w=1080 h=1920 scanout=1`, and explicit QMP `screendump(device=vgpu0)` changes across a real swipe. Under a loaded host, final measurements included 21.817s to `LAUNCH ok`, 0.731s swipe injection, 0.562s Files launch after swipe, and zero `WMIDROP bad`/`INPUTDROPS bad`. The verification image still boots without vgpud and uses guifbd/ramfb. Remaining GPU2 work is hdi vblank/fence/commit, a soft↔vgpu pixel-consensus CI gate, frame-VMO pooling, and Mac shell shm-ring/Metal composition.
