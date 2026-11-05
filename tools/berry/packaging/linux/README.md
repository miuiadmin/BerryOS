# Linux packaging

本目录把 host CLI、`berryos.img` 与 AAVMF 打成 AppDir；QEMU 不打包（GPLv2
进程边界，发行版 `qemu-system-arm` 提供）。

## 在 macOS 交叉构建 Linux CLI（aarch64/x86_64 musl）

```bash
rustup target add aarch64-unknown-linux-musl   # Apple Silicon
tools/berry/packaging/linux/build-linux-binary.sh
```

脚本用 `rust-lld` + 一个空的 `libdl` 桩（static musl 不需要 dl），产出
`tools/berry/cargo-target-linux/<target>/release/berry`，是静态 ELF。

## 打 AppDir

```bash
BERRY_BIN=tools/berry/cargo-target-linux/aarch64-unknown-linux-musl/release/berry \
BERRY_IMAGE=out/berryos.img \
AAVMF_CODE=/opt/homebrew/share/qemu/edk2-aarch64-code.fd \
tools/berry/packaging/linux/build-appdir.sh out/BerryOS.AppDir-linux

tools/berry/packaging/linux/check-appdir.sh --require-elf --file --sha256 \
  out/BerryOS.AppDir-linux
```

`check-appdir.sh` 应输出 `0 failure`。有 `appimagetool` 时：

```bash
APPIMAGETOOL=/path/to/appimagetool BERRY_BIN=... build-appdir.sh
```

## 限制

- AppDir 已验证为 **Linux ELF** 结构；尚未在 Linux 实机运行。
- AppImage/FUSE/桌面集成未验。
- QEMU 必须由目标机提供；`AAVMF_CODE.fd` 已随 AppDir。
