# Agent Note: BerryBoot Boot Progress P1

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [2026-11-13-boot-progress-p1.md](../../plans/2026-11-13-boot-progress-p1.md), [BerryBoot-BootProgress-P1-v1.0.md](../../../docs/BerryBoot-BootProgress-P1-v1.0.md)

## Problem

The Mac product booted at 1080×1920 but showed no system loading process. BerryBoot had truthful serial state predicates, while the visible product console stayed blank until vgpud and the launcher replaced it. A timer-driven splash or a demo-side animation would have been a fake UI and would not verify boot progress.

## Decision

Model progress as host-testable permille derived from real counters. BerryBoot maps KERNEL.ELF and INITRD bytes plus ELF segment and EBS state predicates to 0..1000. It renders the first native surface directly, and for the product request programs ramfb to 1080×1920 before drawing. When the requested surface exceeds firmware GOP, BerryBoot pins an EfiLoaderData frame and updates handoff GOP facts so the kernel reserves the same surface.

To cover the vgpu product console, the present protocol gained an add-only `BootProgress` control frame. svcmgr holds a private duplicate of the present client only before the first gui-wmd frame and sends service-state values; vgpud renders them to a private VMO and flushes through its real virtio-gpu resource. The first wmd SUBMIT naturally replaces the progress surface with the desktop.

## Alternatives considered

- **Timer animation in QEMU or an app**: rejected; it does not correspond to boot truth.
- **Draw only after the launcher starts**: rejected; most perceptible boot time is earlier.
- **Reuse SUBMIT for progress**: rejected; it would consume app credits and confuse surface ownership.
- **Give BerryUI apps a progress API**: rejected; boot progress is system chrome, not application chrome.
- **Keep only serial progress**: rejected; the user-facing Mac product needs a visible native process.
- **Assume EFI static/stack VA=PA for fw_cfg DMA**: rejected after the first implementation stalled; the working descriptor lives at the known framebuffer PA.

## Consequences

The product now has real 1080×1920 progress rendering in BerryBoot, a serial state predicate, vgpu progress from service bring-up through launcher assembly, and an explicit handoff to the desktop. P1 also records an honest boundary: QEMU still has two consoles, so the ramfb 0→100% first screen and vgpu desktop do not yet form one host-owned animated console; that handoff needs the P2 device-shell/QMP control design.
