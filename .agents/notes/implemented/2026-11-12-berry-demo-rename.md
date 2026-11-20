# Agent Note: Berry Demo D1 rename and scope correction

Status: implemented — 2026-11-12

- Date: 2026-11-12
- Type: decision
- Supersedes: [2026-11-12-controls-rich-demo.md](2026-11-12-controls-rich-demo.md) for product naming and demo scope

## Problem

The first product-facing demo landed as `CONTROLS` / “BerryControls Rich Controls Demo”. The user rejected both the product name and the narrow positioning: it should be called Berry Demo directly, and it must demonstrate system features and the application development standard as well as UI controls.

## Decision

Rename the real app to `BERRYDEMO` / **Berry Demo** with entry `bin/berrydemo`, package `BERRYDEMO.bap`, and desktop label `Berry Demo`. Expand the demo from five UI pages to six by adding **System & Standards**, which explains the real APP_SPAWN/process/package/window/frame/input/destroy chain and the manifest/.bap/Berry Room/BerryUI/IconSource/no-surface-escape standard. The earlier BerryControls spec is superseded by [BerryDemo-D1-v1.0.md](../../../docs/BerryDemo-D1-v1.0.md).

## Alternatives considered

- **Keep CONTROLS as an internal technical name**: rejected by the user and confusing on the desktop.
- **Translate the label but keep the old ID**: breaks the one-name rule and leaves a second product identity.
- **Treat it as UI-only**: rejected; the demo's value includes the normal app lifecycle and development standard.
- **Add a separate system demo app**: doubles rootfs/app-slot cost and splits one obvious showcase into two products.

## Consequences

All product, package, tooling, marker, and governance names now use Berry Demo / BERRYDEMO. The app remains a normal retained BerryUI app: no WM RPC parsing, no VMO mapping, no buffer geometry ownership, no pixel writes, and no surface escape. System claims are visible in the new page and remain enforced by GUI8 through the real desktop launch and lifecycle chain.

## Related

- [BerryDemo-D1-v1.0.md](../../../docs/BerryDemo-D1-v1.0.md)
- [GUI框架地基路线-v1.20.md](../../../docs/GUI框架地基路线-v1.20.md)
- [架构决策.md](../../../docs/架构决策.md) ADR #89
