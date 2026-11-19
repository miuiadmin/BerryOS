# Device shell packaging

- **THIRD-PARTY-NOTICES（单源）**: `tools/berry/packaging/THIRD-PARTY-NOTICES.txt`
  是静态链入 berry 二进制的第三方组件许可通知的唯一事实源（rustls 系 6 项
  + subtle BSD-3-Clause 点名 + noVNC 指向随包 `web/NOTICE.md` 的指引）。
  三平台脚本各自从单源拷贝，改内容只改单源：Linux AppDir
  `usr/share/berry/THIRD-PARTY-NOTICES.txt`（`linux/build-appdir.sh`，
  tarball 整目录打包自动随行）、macOS `.app`
  `Contents/Resources/THIRD-PARTY-NOTICES.txt`（`macos/build.sh`）、
  Windows 包根 `THIRD-PARTY-NOTICES.txt`（`windows/assemble.sh` 与
  `windows/build.ps1`，zip 整目录打包自动随行）。三平台产物与单源的
  sha256 同一性由打包冒烟对拍验收；`linux/check-appdir.sh` 查在场与关键行。
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
