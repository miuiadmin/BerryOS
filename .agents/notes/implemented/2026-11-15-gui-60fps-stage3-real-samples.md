# GUI 60Hz stage three: preserve and dispatch real samples

Status: implemented

## Problem

Stage two proved that DMA and raster were no longer the controlling budgets, but a swipe still collapsed to about eleven presented frames. Instrumentation showed two remaining loss points: wmd coalesced MOVEs once four were queued even though those were real sampled positions, and inputd drained only three pointer events per polling heartbeat while the fair scheduler deferred the next drain. Frame-scale sleepers could also expire and be enqueued without an immediate scheduling re-evaluation.

## Decision

Preserve every real sampled position and make frame-scale deadlines dispatch promptly without granting a service permanent priority:

- wmd raises the discrete MOVE latency cap from four to the sixteen-entry queue bound. It merges older real samples into wire history only on true overflow; it never synthesizes a new position.
- inputd drains its PointerPump to empty in the turn that harvested it. `INPUT_ROUTE` remains at most three events per wire frame, but leftovers no longer wait for another polling heartbeat. EBUSY naturally waits for wmd backpressure.
- wmd flushes an already armed `INPUT_POLL` immediately after routing an input batch, before returning to its normal serve order.
- launcher dispatches a SUBMIT request cooperatively to wmd; wmd returns a SYSTEM surface SUBMIT ACK directly to launcher but leaves APP ACKs ordinary.
- vgpud starts the just queued flip immediately after sending SUBMIT ACK, before telemetry formatting, so a catch-up request stream cannot starve its DMA pump.
- On CNTV expiry, the scheduler immediately re-evaluates fairness for sleepers whose requested wait was at least 5ms. Sub-millisecond polling hearts still wake but do not cut an active frame RPC merely because they timed out.

## Alternatives considered

- Skipping the launcher wait whenever staged samples remain: rejected. It did not improve pacing and can submit multiple positions in one backend scanout period, causing overwrites.
- Yielding vgpud immediately after SUBMIT ACK instead of pumping: rejected. It handed CPU away before the flip ran and worsened completion latency.
- Treating every timeout as urgent: rejected. inputd's 500µs active heart could interrupt wmd in the middle of a reply syscall and cause hundred-millisecond deferrals.
- One pointer event per `INPUT_POLL` reply: rejected. Extra RPC overhead reduced pacing.
- Global equal-priority direct preempt and static GUI deadline boosts remain rejected from stages one and two.

## Consequences

- Clean five-run product calibration improved from stage two `frames median11 / gap median117.9ms / p95 312.1ms / snap240=3` to `frames median22 / min20 / gap median47.5ms / p95 125.8ms / snap240 median8`. DMA stayed fast (`median≈589µs`).
- The 60Hz gate remains unmet: median and p95 are above 16.7ms and 240ms snap is below fourteen frames. Do not call this smooth or 60fps.
- Product Recents/frame-cache e2e, GUI8 vgpu `--only-a`, `smoke-image` 147/147, BerryUI 339/339, berry-guiproto 73/73, gui-wmd host 8/8, berry host 18/18, and `just check` all pass.
- Remaining variance is the poll-model input wake/dispatch path. The next structural刀 is the documented virtio-input IRQ-to-waitable-event face, not more queue enlargement or fake interpolation.

## Related

- [GUI 60Hz stage two](2026-11-15-gui-60fps-stage2-handoff.md) added cooperative reply handoff and one-position staging.
- [GUI 60Hz stage one](2026-11-15-gui-60fps-stage1.md) established the pacing gate and removed DMA/raster overruns.
