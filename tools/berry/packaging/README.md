# Device shell packaging

- **macOS**: `tools/berry/packaging/macos/build.sh` → `out/BerryOS.app`
  (bundles `berry`, `berryos.img`, `AAVMF_CODE.fd`; ad-hoc signed).
  Needs `just berry-build` + `just image` first. QEMU stays external
  (`brew install qemu`); it is GPLv2 and is never linked/bundled by this repo.
- **Linux**: `linux/build-linux-binary.sh` 交叉构建 musl ELF →
  `linux/build-appdir.sh`（AppRun + berry + image + AAVMF + .desktop）→
  `linux/check-appdir.sh --require-elf`；`linux/build-tarball.sh` 产便携 tar.gz；
`linux/smoke-docker.sh` 用 arm64 Linux 容器 + QEMU TCG 实跑 doctor/480/540；
有 `appimagetool` 时顺带产 AppImage。
  QEMU 用发行版 `qemu-system-arm`，外部依赖。
- **Windows**: `windows/build-cross-docker.sh` → `out/berry.exe`；
  `windows/assemble.sh` → `out/BerryOS-win64` + zip；`smoke-wine-docker.sh` 做 Wine
  `--version` 验证。QEMU 外部；GUI subsystem 仍未落。
- **Windows**: `windows/README.md`；GUI-subsystem + `AttachConsole` 仍未落。

Double-click semantics: no args + no TTY → `berry shell`; the image/firmware
are resolved from `Contents/Resources` (macOS) or next to the executable.
