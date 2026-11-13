# Agent Note: Mac product single window, AA progress digits, and system nav

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [BerryBoot-BootProgress-P1-v1.0.md](../../../docs/BerryBoot-BootProgress-P1-v1.0.md), [Berry设备平台-CLI-first设计-v1.0.md](../../../docs/Berry设备平台-CLI-first设计-v1.0.md), [BerryMacEmulator-GPU2-v1.0.md](../../../docs/BerryMacEmulator-GPU2-v1.0.md)

## Problem

The user saw three stale Berry shell windows stuck around 91%. They were old debug hosts using `/tmp/berryos-hvf-debug.img` (one had a kernel panic), not the current product recipe, but nothing prevented repeated product desktops from consuming multiple HVF/QEMU instances. Boot progress percentages were an 8×16 bitmap font scaled up, producing blocky digits. The native toolbar still had five small utility buttons and no system BACK/HOME/TASK controls.

## Decision

Close stale debug hosts and make the product entry single-instance with a `/tmp/berryos-desktop.lock` containing a live PID; a stale lock is reclaimed automatically. The Apple Silicon product entry now uses the native berry window, while its QEMU topology still has a real virtio-gpu primary console and ramfb fallback.

Replace percentage rendering with dedicated 28×54 geometric digits and a percent sign. Each output pixel is sampled 4×4, producing 16-level coverage that is alpha-blended with the background. The final state renders complete `100%`, and the resulting vgpu damage band is `1080×152@1305`.

Upgrade the native toolbar to 72px with 56px-high buttons and eight actions: BACK, HOME, TASK, SHOT, RES, BOOT, OFF, FULL. BACK/HOME/TASK use deterministic QMP key events (Linux F1/F2/F3) routed to gui-wmd. The WM authority implements BACK as hiding the focused app owner, HOME as hiding all APP-layer owners, and TASK as cycling to the next app owner. This preserves desktop surface 0 and does not ask individual apps to implement global navigation.

## Alternatives considered

- **Allow multiple desktop windows**: rejected; duplicate HVF/QEMU instances waste memory and create stale-boot confusion.
- **Keep scaled bitmap numerals**: rejected; magnifying an 8×16 bitmap cannot look like a modern OS boot face.
- **Fake navigation by changing screenshots**: rejected; HOME/TASK must change the real WM visibility/focus state.
- **Give every BerryUI app its own bottom system bar**: rejected; system navigation belongs to the WM/host authority, while apps own only in-app back stacks.
- **Put navigation in VNC-only input**: rejected after live testing; system keys use the same deterministic QMP event path as GUI8, with an 80ms down/up interval.

## Consequences

The stale windows were closed and the current product opens one native window with a large toolbar over a real 1080×1920 vgpu console. Live testing launched Settings, then exercised `WMNAV ok home owner=0`, `WMNAV ok task owner=2`, and `WMNAV ok back owner=2`. Host tests pin the eight hit targets and the anti-aliased digit coverage. A future visual recents overlay should consume the same WM task authority rather than adding a second truth source.
