# Agent Note: Mac emulator HVF1 and product fast desktop

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [BerryMacEmulator-HVF1-v1.0.md](../../../docs/BerryMacEmulator-HVF1-v1.0.md), [BerryDemo-D1-v1.0.md](../../../docs/BerryDemo-D1-v1.0.md), [架构决策.md](../../../docs/架构决策.md) ADR #90

## Problem

The interactive Mac emulator was unusable as a development surface: HVF panicked in the FP context test, PAN rejected kernel copy-to-user, ASID reads returned zero under HVF, QEMU HVF asserted on virtio-input config reads, every desktop boot ran the full serial verification batch, and pointer moves flooded the 16-deep WM queue so clicks were delayed or lost. The user explicitly rejected lowering 1080×1920 and required the emulator to be a first-class product experience.

## Decision

Keep 1080×1920@60 as the only default product resolution. Treat the four Apple HVF differences as runtime/access-shape variants in the same source tree: probe FPSR NZCV writability, enable `SCTLR_EL1.SPAN`, take the authoritative ASID from scheduler `Task` metadata, and read virtio-input config as aligned `u32` words.

Split the product image from the verification image by `probe.on`: `out/berryos-desktop.img` skips serial probes and launcher boot self-swipes but keeps real APP_LIST/APP_SPAWN/Berry Room/input routing; `out/berryos.img` and GUI8 remain unchanged. Make interaction latency a contract: inputd uses a 2ms active poll, gui-wmd coalesces only MOVE samples while always preserving DOWN/UP/CANCEL, and launcher gives immediate press feedback with a 500ms spawn debounce. Berry Room and gui-wmd now forward real partial damage instead of turning every local update into a full-screen present.

## Alternatives considered

- **Lower the default resolution**: rejected by the user; resolution may only be an explicit developer override.
- **Keep TCG for interactive use**: deterministic but too slow for app development and real pointer interaction.
- **Keep one image for product and verification**: makes every user boot a CI run and preserves the serial app batch delay.
- **Drop all MOVE samples beyond one**: improved click ordering but broke the swipe velocity/displacement evidence in GUI8; four discrete MOVE entries were retained before coalescing.
- **Report `apple-gfx-pci` or virtio-gpu as GPU acceleration**: rejected; EDK2 hands off no framebuffer for apple-gfx and BerryOS has no virtio-gpu guest driver yet.

## Consequences

Mac Apple Silicon `just desktop` opens the 1080p product desktop through QEMU Cocoa and HVF. Real QMP interaction measured 0.320s from a completed swipe to Files launch, 0.579s Berry Demo cold launch, 0.169s page transition, and 0.148s Switch response under a loaded host, with zero `WMIDROP bad`/`INPUTDROPS bad` and no kernel panic. GUI8 normal, dual-boot canonical persistence, smoke-image 146/146, host tests, checks, and license gates pass. The display is still guest CPU raster plus ramfb; a real GPU path requires a future virtio-gpu 2D driver or a Mac-side Metal/DisplayList presenter and must not be claimed by this knife.
