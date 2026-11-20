# GUI R3 Config Event Architecture

Status: implemented — 2026-11-11

## Problem

R2 established raw bounds, visible bounds and host-derived occlusion, but clients had no per-window configuration event surface. `WM_TABLE_WAIT` is a single parking slot used by inputd, gui-wmd's waitset is already at the 16-handle cap, and Berry Room is a strict serial RPC client. A push channel would break `WM_CONNECT` and capacity assumptions. Meanwhile the R2 Room bootstrap used a whole-table `TABLE_GET`, and dynamic occluder changes had no consumption contract.

## Decision

1. Add only two WM ops:
   - `0x6A OP_WM_CONFIG_POLL`: nonblocking single-event poll on the existing session RPC;
   - `0x6B OP_WM_CONFIG_ACK`: exact serial acknowledgement.
2. Keep configuration events advisory and full-state. Each event carries:
   - table-version serial;
   - window id;
   - `CREATED/GEOMETRY/VISIBILITY/OCCLUSION` cause;
   - visible bool;
   - raw bounds;
   - visible bounds.
3. Store one pending event per `Win`. Newer events overwrite older unacknowledged events (last-one-wins). There is no event ring and no second geometry authority.
4. Use the post-mutation WmTable version as serial. Unrelated focus bumps do not renumber an existing pending event. Counter wrap advances the epoch; the extreme epoch terminal clears only already-serialized advisory pending state and preserves a fresh event from that same bump.
5. Generate events from state differences only:
   - CREATE -> `CREATED`;
   - raw change -> `GEOMETRY`;
   - visibility change -> `VISIBILITY`;
   - AVOID_SYS effective change -> `OCCLUSION`;
   - Z-only, no-op SET and focus-only changes generate nothing.
6. ACK validation is exact:
   - missing window -> `ST_NOENT`;
   - foreign owner -> `ST_DENIED`;
   - no pending/wrong/stale/consumed serial -> `ST_DENIED`;
   - exact current serial -> clear pending and `ST_OK`.
7. Add Wayland-style teeth: a window whose `CREATED` event has not been acknowledged, or which has a newer pending event, cannot enter `0x67` or SUBMIT. Geometry truth remains in WmTable; ACK never mutates geometry.
8. Berry Room now performs CREATE -> POLL -> validate CREATED/raw-visible bounds -> ACK. Dynamic `poll_config()` refreshes occlusion for visibility/occlusion changes and rejects raw-geometry changes that cannot be applied to its fixed VMO. `submit()` drains/acks pending events at the frame boundary and re-polls if a pending event is superseded between poll and ack; layout-aware apps should still poll before building a DisplayList.
9. GUI8 proves:
   - unconfigured SUBMIT and 0x67 denial;
   - foreign and destroyed-window ACK denial;
   - no-op SET and focus-only no-event proofs;
   - VISIBILITY hide/show events;
   - CREATED pull-once;
   - wrong-serial ACK denial;
   - empty queue after ACK;
   - SYSTEM occluder HIDE+SHOW coalescing to the latest OCCLUSION state;
   - stale ACK denial;
   - normal boot marker isolation.

## Alternatives considered

- **Per-session event channel/push**: rejected. It exceeds the full 16-handle waitset and changes WM_CONNECT.
- **Reuse TABLE_WAIT**: rejected. It would preempt inputd's single parking slot.
- **Batch all windows into one event list**: rejected for v1. One full-state event per window keeps the frame small and avoids cross-window consistency claims.
- **Ordered ACK comparison (`>=`)**: rejected. Epoch/version wrap makes ordering unsafe; exact serial is recoverable and stateless.
- **Add scale/density/rotation/IME causes now**: rejected. There are no producers; scale remains 0x67-only.
- **Let ACK mutate geometry**: rejected. WmTable is the sole geometry authority.

## Consequences

- Apps can obtain a consistent raw/visible snapshot before first surface use and consume dynamic occlusion changes at frame boundaries.
- Poll/ack serializes naturally with Room's SUBMIT -> FRAME_CALLBACK -> RELEASE loop.
- A superseded or stale ACK forces a re-poll of the latest full state.
- Dynamic raw resize still has no buffer lifecycle; Room rejects it loudly.
- BerryUI does not yet pump events automatically, and no push/wait op is claimed.

## Related

- [BerrySession-R3ConfigEventArchitecture-v1.0.md](../../../docs/BerrySession-R3ConfigEventArchitecture-v1.0.md)
- [BerrySession-R2OcclusionContract-v1.0.md](../../../docs/BerrySession-R2OcclusionContract-v1.0.md)
