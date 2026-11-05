# Windows packaging

```powershell
# Requires a Windows cross-build first:
#   cargo build --release --target x86_64-pc-windows-gnu
.\tools\berry\packaging\windows\build.ps1
```

Outputs `out/BerryOS-win64`:

- `berry.exe`
- `berryos.img`
- `AAVMF_CODE.fd`
- `run-berry.cmd`（设置 `BERRY_IMAGE`/`AAVMF_CODE` 后启动 `berry shell`）
- `README.txt`

Overrides: `BERRY_EXE`, `BERRY_IMAGE`, `AAVMF_CODE`, `-Target`, `-TargetDir`.

## 限制

- QEMU 不打包（GPLv2 进程边界）；需 `qemu-system-aarch64` 在 PATH 或 `BERRY_QEMU`。
- `berry.exe` 当前是 console binary；Windows GUI subsystem / AttachConsole 仍是 C1g 尾项。
- 未在 Windows 实机验证；脚本仅做结构/路径静态检查。
