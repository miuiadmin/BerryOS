# Agent Note: GUI interaction quality phase 1

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [2026-11-13-gui-interaction-quality.md](../../plans/2026-11-13-gui-interaction-quality.md), [Berry设备平台-CLI-first设计-v1.0.md](../../../docs/Berry设备平台-CLI-first设计-v1.0.md), [BerryBoot-BootProgress-P1-v1.0.md](../../../docs/BerryBoot-BootProgress-P1-v1.0.md), [BerryMacEmulator-GPU2-v1.0.md](../../../docs/BerryMacEmulator-GPU2-v1.0.md)

## Problem

The product shell had correct low-level navigation events but no product behavior: `gui-wmd` changed only its authority table, leaving stale app pixels in the composition VMO; the host toolbar had no pressed state and blocked the winit thread for 80ms; app clicks rendered no state while a synchronous `APP_SPAWN` RPC cold-started a process; the first pointer event could wait up to 100ms in `gui-inputd`; and the visible vgpu boot progress had only five coarse truth points. Desktop scrolling also promoted a transparent page track to a near-full-port repaint.

## Decision

Implement interaction quality as system framework state, not per-app visuals. A new pointer flag, `PE_FL_SYSTEM_INVALIDATE`, lets `gui-wmd` wake the SYSTEM launcher after a real BACK/HOME/TASK table change. BerryUI exposes `invalidate_full`, and the launcher repaints the full desktop surface to erase hidden-app ghosts. The host toolbar draws an immediate pressed state and click ripple, and queues QMP key pulses on a worker thread.

Launcher clicks now enter a truthful Opening state before the blocking spawn RPC. BerryUI adds `hold_press`/`release_press` because DOWN and UP can arrive in one input batch and cancel before a frame. The first input harvest interval is reduced from 100ms to 16ms. Boot progress now has nine monotonic, producer-backed milestones from vgpu ready through system UI connection, first SYSTEM submit, and desktop handoff; no wall-clock interpolation was added.

For the display path, wmd uses a 64KiB static bounce to bulk-read source rows while preserving the destination full-screen row stride. The host VNC decoder has a 32bpp byte-channel fast path. BerryUI adds sparse scrolling damage aggregated by direct page child so transparent containers do not automatically become a full-port repaint.

## Alternatives considered

- **Let the next touch erase hidden app pixels**: rejected; HOME/BACK must take effect immediately.
- **Keep the launcher idle during navigation**: rejected; a table version bump without a composition update is not user-visible state.
- **Show a host-only success animation**: rejected; host feedback says “input received”, while guest state changes must be rendered by the guest system surface.
- **Use a timer-driven launch or boot animation**: rejected; it would fabricate progress.
- **Enable two vCPUs by default**: measured on the existing G2-capable kernel, but the GUI arm did not improve (launch 2.932s, page 2.604s, switch 1.739s versus the one-vCPU phase sample); deferred rather than changing the product topology.
- **Treat the first GUI8 gfxdemo submit as desktop handoff**: rejected after regression; progress milestones 900 and 1000 are restricted to SYSTEM surface 0.

## Consequences

A live 1080×1920 HVF product run now advances through all nine boot points, launches a real Berry Demo app, and emits exactly `WMNAV home/task/back` plus three `WMNAVINVAL` wakeups before returning to the desktop. Host tests pin immediate toolbar/touch feedback, persistent press state, full invalidation, sparse scrolling damage, and monotonic boot milestones. `smoke-image` remains 147/147. GUI8 vgpu exposed two integration defects during development (progress consumed by an early gfxdemo submit and a destination-stride error in bulk copy); both were fixed. A final GUI8 vgpu run was then externally killed by unrelated TCG shells on the host, so the complete GUI8 gate remains to be rerun after that host load clears. Longer-term swipe throughput still requires the plan's P2 pipeline/shm-ring work; phase-1 calibration does not claim a smooth 60fps desktop.
