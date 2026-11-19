# Windows packaging

## Cross-build（macOS arm64 → Windows x86_64）

```bash
tools/berry/packaging/windows/build-cross-docker.sh
# out/berry.exe（PE32+ console x86-64）
```

内部用 `--platform linux/amd64` 的 `rust:1.97-slim` + mingw-w64 +
`x86_64-pc-windows-gnu`；QEMU 不参与链接。

## 组装发行目录

```bash
tools/berry/packaging/windows/assemble.sh
# out/BerryOS-win64/{berry.exe,berryos.img,AAVMF_CODE.fd,run-berry.cmd,README.txt}
# out/BerryOS-win64.zip
```

## Wine smoke（不是实机）

```bash
tools/berry/packaging/windows/smoke-wine-docker.sh
# berry 0.1.0
```

## 限制

- QEMU 不打包（GPLv2）；目标机需 `qemu-system-aarch64.exe` 或 `BERRY_QEMU`。
- 现在产 **两个 exe**：
  - `berry.exe`：console CLI（cmd/PowerShell 输出正常）
  - `berry-shell.exe`：`windows_subsystem=windows` 的 GUI 双击入口（无控制台）
- `assemble.sh` 两个都打包；`run-berry.cmd` 用 console 版。
- 未在真实 Windows 上跑；Wine 只验证 console `berry.exe --version`。
  GUI `berry-shell.exe` 的双击/窗口行为未在真实 Windows 验证。
- 未刷新图标/安装器/签名。
