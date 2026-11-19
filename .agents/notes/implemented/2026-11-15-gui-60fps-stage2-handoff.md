# GUI 60Hz stage two: cooperative reply handoff

Status: implemented

## Problem

Stage one proved that the remaining roughly 293ms launcher frame gap was not virtio-gpu DMA and not BerryUI raster. Host input was already near 60Hz, vgpud completed callback requests in the same millisecond, but equal-priority launcher/wmd/vgpud fair rounds and three-position input batches collapsed a real swipe into five presented frames. A global same-priority direct preempt had already broken intent ktests, and statically placing GUI services in the deadline band broke app click chains.

## Decision

Implement a restricted, explicit reply-handoff path instead of changing IPC fairness globally:

- Channel endpoints remember the Process tag of their most recent receiver. A send records either the sole fast-path waiter or that receiver process as a per-CPU handoff hint.
- `thread_yield` may select that exact ready task only when no higher-priority eligible task exists. No send implicitly preempts an equal-priority task; intent semantics and ordinary fair scheduling remain unchanged.
- vgpud and wmd use the hint only for completed `FRAME_CALLBACK` replies, not SUBMIT ACK. launcher does the same after sending a callback request. wmd also hands a nonempty `INPUT_POLL` reply to the waiting launcher.
- inputd's active-window harvest tick drops from 2ms to 500µs for the existing 200ms activity window; idle remains 16ms.
- The scheduler slice changes from 100ms to 50ms (`SLICE_HZ` 10→20). This keeps boot/ktests green while reducing fair-round quantization.
- launcher stages pointer events from a wire batch and pushes one real sampled position per frame, preserving order and timestamps. It does not interpolate or synthesize positions.

## Alternatives considered

- Global equal-priority direct switch on every channel send: rejected; intent two-hop/slow-path ktests fail.
- Statically boosting inputd/wmd/vgpud or launcher into SCHED_DEADLINE: rejected; app-internal click/page chains regress.
- Yielding after every service reply, including SUBMIT ACK: rejected; it lets producers enqueue work before wmd drains pending frames.
- Yielding inputd to wmd after every routed batch, or having wmd yield back through request-side sender affinity: rejected; measured frame pacing became worse and occasionally hung before HOME.
- Returning only one input event per INPUT_POLL reply: rejected; RPC overhead reduced pacing quality.
- 100ms/33ms/20ms slices: 100ms was the original quantizer, 33ms occasionally exceeded the 60s boot gate, and 20ms failed intent timing. 50ms was the fastest slice that passed the full boot/smoke gates in this stage.

## Consequences

- Clean three-run product calibration improved from stage one `frames=5 / median=292.8ms / p95=487.4ms / snap240=2` to `frames median=11 / min=9 / gap median=117.9ms / p95=312.1ms / snap240=3`. DMA remained fast (`median≈133µs`).
- The 60Hz gate is still not met: median and p95 exceed 16.7ms, 240ms snap is far below 14 frames, and `fps_target_met=false`. Do not describe this stage as smooth or 60fps.
- Product Recents and owner frame-cache e2e, GUI8 vgpu `--only-a`, `smoke-image` 147/147, BerryUI 339/339, berry-guiproto 73/73, gui-wmd host 8/8, berry host 18/18, and `just check` all pass.
- Remaining work is the inputd→wmd→launcher catch-up path: routed positions still arrive in fair-round-sized groups, so a queued real position can wait behind service scheduling before presentation.

## Related

- [GUI 60Hz stage one](2026-11-15-gui-60fps-stage1.md) established the 60Hz gate and removed deterministic DMA/raster overruns.
- [GUI swipe precise damage and completion reclaim](2026-11-15-gui-swipe-precise-damage.md) supplies the per-row damage and completion-reclaim base.
