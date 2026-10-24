# BerryOS

**A microkernel operating system written entirely in Rust — power-first, capability-secure, and AI-native by construction.**

[![CI](https://github.com/miuiadmin/BerryOS/actions/workflows/ci.yml/badge.svg)](https://github.com/miuiadmin/BerryOS/actions/workflows/ci.yml)

BerryOS targets battery-class devices first (watch-tier ARM64) and grows toward phones and PCs. The design borrows deliberately: Zircon's pragmatic kernel objects, seL4's capability discipline, and a scheduler built for tickless operation from day one.

## Design pillars

- **Minimal kernel, maximal isolation.** The `#![no_std]` kernel keeps only what cannot be delegated: scheduling, IPC, address spaces, and capability dispatch. File systems, networking, and drivers are user-space services that can crash and restart without taking the system down.
- **Capabilities, not permission bits.** Every resource access flows through kernel-enforced handles (Zircon-style). A service sees exactly what it was granted — nothing more.
- **Power-first.** Tickless (NO_HZ) idle from day one, per-task EPP hints, and a user-space power manager talking to the kernel through a shared ring — not message-copy IPC.
- **AI-native scheduling.** The kernel exposes first-class interfaces that let a user-space AI policy service influence placement, frequency, and priorities — within hard guardrails (envelope clamps, monotonic narrowing). Intent is a first-class scheduling input (u8, frozen encoding), layered on top of fairness, never replacing it.
- **AI-friendly ABI.** The syscall ABI is generated from a single source of truth (`tools/abi.toml` → kernel + user-space bindings + docs, byte-checked in CI). Structured tracing, semantic error codes, machine-readable documentation.
- **Testing built for automation.** Every smoke assertion is a deterministic state predicate — no timing races. An AI agent (or CI) can run the full gate, read the results, and trust them.

## Status — October 2026

Phase 0–6 shipped on ARM64 (QEMU `virt`), x86_64 minimal port in CI. Highlights of what runs today:

| Layer | What boots |
|---|---|
| Kernel | Preemptive scheduler with priority-inheritance donation, VMO/Vmar memory objects, capability handle tables, 30 registered syscalls (29 implemented), channel IPC with fastpath/slowpath, intent propagation |
| Services | console/svcmgr service framework, RAM filesystem, **user-space virtio-blk and virtio-net drivers**, FAT12 read/write with cross-boot persistence, UDP/DNS, minimal TCP client (full handshake → data → teardown) plus TCP RPC service |
| Apps | `bsh` shell (pipes, mini-readline, env), **unmodified `hexyl` 0.17.0** running via a musl-target libc shim, `.bap` application packages (manifest → capability grant → supervised lifecycle) |
| x86_64 | PVH boot, PIT/PIC timer, exception vectors, context switching, SYSCALL/SYSRET — 7 smoke markers gated in CI |

Current milestone **M-VM**: a self-hosting disk image (BerryBoot EFI loader → kernel handoff), then a virtualization bring-up window, then a usable CLI. Full plan (Chinese) in [`docs/VM里程碑设计草案.md`](docs/VM里程碑设计草案.md).

## Quick start

```bash
brew install qemu just                    # macOS; Linux: apt install qemu-system-arm qemu-system-x86 just
rustup target add aarch64-unknown-none x86_64-unknown-none

just run          # boot the kernel to a serial console in QEMU (ARM64 virt)
just run-rootfs   # full service stack: rootfs, FAT, DNS, TCP, .bap apps
just smoke        # unattended 5-phase gate: bare 23 + rootfs 24 + interactive RX 47 markers,
                  #   plus FAT subdirectory and cross-boot persistence phases
just test-host    # 315 host-side unit tests, no QEMU required
```

No nightly Rust, no custom toolchain — stable builtin targets only.

## Repository layout

```
kernel/       microkernel core (no_std)
mm/           physical frames / virtual memory / page tables
sched/        scheduler / tasks / context switching
ipc/          message passing (reserved)
arch/         aarch64 HAL (primary) + x86_64 minimal port
bootloader/   BerryBoot EFI loader
drivers/ fs/ net/    user-space service slots
user/         ABI bindings, protocol crates, programs (bsh, svcmgr, vnet, ...)
tools/        build recipes, smoke harness, deterministic packers, ABI generator
docs/         design documents (Chinese)
```

## Documentation

Design documents are written in Chinese and live in [`docs/`](docs/). Start from the index: [`docs/索引.md`](docs/索引.md). Key entries:

- Architecture decisions — 49 ADRs, the single source of truth: [`docs/架构决策.md`](docs/架构决策.md)
- Roadmap: [`docs/路线图.md`](docs/路线图.md)
- Scheduler design (mechanism/policy separation, capacity model, intent): [`docs/调度器设计.md`](docs/调度器设计.md)
- Syscall ABI (frozen v1, single-source generated): [`docs/系统调用ABI设计.md`](docs/系统调用ABI设计.md)

中文说明见 **[README.zh-CN.md](README.zh-CN.md)**。

## License

Apache-2.0. Third-party systems are studied as references only — no third-party code is vendored in this tree, and GPL-licensed implementations are excluded outright.
