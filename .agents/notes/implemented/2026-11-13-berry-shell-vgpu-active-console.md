# Agent Note: Berry shell vgpu active-console source

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [Berry设备平台-CLI-first设计-v1.9.md](../../../docs/Berry设备平台-CLI-first设计-v1.9.md), [BerryMacEmulator-GPU2-v1.0.md](../../../docs/BerryMacEmulator-GPU2-v1.0.md), [berryboot-macos-white-progress.md](2026-11-13-berryboot-macos-white-progress.md)

## Problem

The live product window remained on a full Boot Progress bar even though the guest had completed the real handoff. Serial output in the same run showed `VGPUBOOT permille=1000`, a vgpu submit, and `LAUNCH ok`; a VNC screendump contained the desktop. The berry host had selected `RamSource` because the kernel printed `FB ok`: that framebuffer belongs to the firmware/GOP ramfb surface. Later vgpud and gui-wmd render into the virtio-gpu console, not back into that fixed guest-RAM address, so the host kept repainting the stale 100% boot frame.

## Decision

Make display-surface selection part of the product shell contract. When the requested display mode parses as `WxH`, QEMU instantiates `vgpu0`; the session host must therefore use `VncSource`, which follows QEMU's active console. `RamSource` remains only for non-vgpu/ramfb debug profiles where the guest framebuffer is the intended long-lived display surface.

The implementation exposes the display-mode geometry parser to the device runtime and adds `display_requests_vgpu`. A host test pins `1080x1920@60` and `800x600` to the VNC path, while malformed modes and `None` stay on the RamSource fallback. A live 1080×1920 HVF restart then showed white progress reach 100%, the first submit replace it with the desktop, and `LAUNCH ok`; the user-facing VNC frame was a 1.8MiB desktop PNG rather than the tiny stale boot frame.

## Alternatives considered

- **Keep `RamSource` and poll the vgpu resource in guest RAM**: rejected; virtio-gpu resources are host-side QEMU surfaces, not a stable guest framebuffer address.
- **Use `RamSource` until `VGPU ok` and then swap**: rejected as a race; the host can miss the first desktop submit and needs a second source-selection truth.
- **Make wmd also repaint ramfb**: rejected; it would create two pixel truth sources and violate GPU2 consensus.
- **Use a screenshot as the product display**: rejected; the window must present live frames and input on the same console.

## Consequences

The product shell now watches the same console that QEMU presents: Boot Progress occupies it before vgpud handoff and gui-wmd's first submit replaces it with the desktop. Attach sessions already use VNC. Non-vgpu debug profiles retain shared-RAM performance and behavior. Future shm/virtio-gpu host transports must be selected by the same display-mode authority, not by the presence of `FB ok`.
