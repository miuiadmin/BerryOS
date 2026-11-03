# Device shell packaging

- **macOS**: `tools/berry/packaging/macos/build.sh` → `out/BerryOS.app`
  (bundles `berry`, `berryos.img`, `AAVMF_CODE.fd`; ad-hoc signed).
  Needs `just berry-build` + `just image` first. QEMU stays external
  (`brew install qemu`); it is GPLv2 and is never linked/bundled by this repo.
- **Linux**: use `linux/berry.desktop` with an AppImage/desktop entry that
  ships `berry` + `berryos.img`; QEMU from the distro (`qemu-system-arm`).
- **Windows**: see `windows/README.md`; a GUI-subsystem + `AttachConsole`
  tail is still open.

Double-click semantics: no args + no TTY → `berry shell`; the image/firmware
are resolved from `Contents/Resources` (macOS) or next to the executable.
