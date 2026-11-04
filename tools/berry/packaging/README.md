# Device shell packaging

- **macOS**: `tools/berry/packaging/macos/build.sh` → `out/BerryOS.app`
  (bundles `berry`, `berryos.img`, `AAVMF_CODE.fd`; ad-hoc signed).
  Needs `just berry-build` + `just image` first. QEMU stays external
  (`brew install qemu`); it is GPLv2 and is never linked/bundled by this repo.
- **Linux**: `linux/build-appdir.sh` → `out/BerryOS.AppDir`（AppRun + berry + image +
  AAVMF + .desktop）；有 `appimagetool` 时顺带产 AppImage。QEMU 用发行版
  `qemu-system-arm`，外部依赖。
- **Windows**: `windows/build.ps1` → `out/BerryOS-win64`（berry.exe + image + AAVMF +
  README）；需先 cross-build `x86_64-pc-windows-gnu`，QEMU 外部。
- **Windows**: see `windows/README.md`; a GUI-subsystem + `AttachConsole`
  tail is still open.

Double-click semantics: no args + no TTY → `berry shell`; the image/firmware
are resolved from `Contents/Resources` (macOS) or next to the executable.
