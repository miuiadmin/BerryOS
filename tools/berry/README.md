# berry — BerryOS 开发者设备壳（host tool）

一个宿主二进制，两种入口：

- 终端里 `berry doctor / run / shot / shell / input` —— 可编程、可 CI；
- 双击/无参数 —— 打开手机比例的设备窗口（`berry shell`）。

设计依据：`docs/berry设备壳设计草案.md` / ADR #62。**QEMU 只是引擎子进程**：
帧面走 QEMU `memory-backend-file,share=on` 的客机 RAM 镜（`RamSource`），输入走
VNC 多客户端（`VncInput`），生命周期/复位走 QMP。窗口、截图、缩放都在宿主侧。

## 构建

```bash
just berry-build          # host 独立 workspace，显式 --target <host>
just berry doctor         # QEMU / 固件 / 镜像 / accel
```

`tools/berry` 是独立 workspace；根 `.cargo/config.toml` 把 `build.target` 锁成
`aarch64-unknown-none`，所以必须走 `just berry-build`（或显式 `--target`）。宿主依赖
（winit/softbuffer/clap/serde_json）只在 `tools/berry/Cargo.lock`，不进客机/ABI。

## 双击包

```bash
tools/berry/packaging/macos/build.sh      # → out/BerryOS.app（自包含镜像/固件，ad-hoc 签名）
BERRY_IMAGE=out/berryos-desktop.img BERRY_BUNDLE_ID=dev.berryos.shell.desktop \
  tools/berry/packaging/macos/build.sh out/BerryOS-Desktop.app
```

macOS `.app` 会从 `Contents/Resources` 找 `berryos.img`/`AAVMF_CODE.fd`；QEMU 仍是外部
依赖（GPLv2，进程边界，不打包）。Linux desktop/Windows 草稿见
`tools/berry/packaging/`；Windows 的 GUI 子系统 + `AttachConsole` 仍待补。

## 命令

```bash
just berry doctor [--json]
just berry run --kernel <elf> --marker "BerryOS boot" --timeout 30 --follow
just berry shot --image out/berryos.img --out out/shot.png --timeout 120
just berry shot --image out/berryos.img --profile phone --out out/phone.png
just shell                                  # = berry shell --image out/berryos.img
just berry shell --profile phone --timeout 30
just berry input key ret
just berry input tap 400 300
just berry input swipe 400 500 400 200 --ms 300
just berry-smoke                            # doctor + kernel marker + EFI shot
```

`berry shot` 不建窗口：起 QEMU（`-display none -vnc`）→ 等串口标记 → 从 VNC 抓一帧 →
写 PNG → 停设备。`berry shell` 用 `-display none` + `memory-backend-file` 起同一套设备：
串口 `FB ok 480x1000 … fb=0x…` 给出几何/PA，`RamSource` 直接读 `ram.bin`，窗口按
19:9 等比缩放显示；VNC 只作输入通道（不请求帧），`berry input` 可与 shell 并存。

## 环境变量

| 变量 | 作用 |
| --- | --- |
| `BERRY_HOME` | run dir 根（默认 `~/.berry`，Windows `%LOCALAPPDATA%\BerryOS`） |
| `BERRY_QEMU` | 指定 `qemu-system-aarch64` |
| `BERRY_KERNEL` / `BERRY_IMAGE` | 默认内核/镜像 |
| `AAVMF_CODE` | AAVMF code pflash（默认 brew / 发行版路径） |

## 窗口快捷键

| 键 | 动作 |
| --- | --- |
| `Cmd/Ctrl+S` | 截图 `berry-shot-<ts>.png` |
| `Cmd/Ctrl+R` | 复位设备 |
| `Cmd/Ctrl+Q` | 退出 |
| `F` | 全屏切换 |
| `I` | 显示/隐藏传输信息 |
| 鼠标/触摸 | 点击、拖动 → 客机触摸 |
| 键盘 | 映射到 X11 keysym |

## Profile

| id | 面板 | 客机渲染 | 备注 |
| --- | --- | --- | --- |
| `default` | 800×600 | 800×600 | P1 兼容档 / CI |
| `phone` | 1080×2250 | 480×1000 | 默认；恰好用满 800×600 GOP 缓冲 |
| `phone16` | 1080×1920 | 480×854 | 16:9 |
| `phone20` | 1080×2400 | 540×1200 | 需 2.59MB fb；P2 暂被 1.92MB 容量拒 |
| `tablet` | 1600×2560 | 800×1280 | 后续 |

`phone*`/`tablet` 默认内存 128MiB（实测 256MiB 时 BerryBoot handoff 失效，另立刀）。

## 已知限制（P2）

- QEMU 是外部依赖（GPLv2，进程边界）；主仓 Apache-2.0 不打包 QEMU。
- 帧面要求 QEMU `memory-backend-file,share=on`；不支持该后端的环境回退 VNC（P1 路径）。
- `berry input` 走 VNC 输入专用连接（多客户端，不请求帧）；QMP 只留 reset/stop。
  独立 attach 控制服务仍未落，但跨进程注入已由 VNC 覆盖。
- 默认 `berry shot` 等 `BERRYBOOT ok`，当前测试镜像此时可能还在固件/测试面；要看 GUI
  请传 `--wait-marker "FBW ok"` 或 launcher 标记。`--profile phone` 时 guifbd 进入
  GUI 模式（console 只走串口，不盖桌面）；默认 800×600 仍保留 console 上屏。
- Windows/Linux 代码路径已写（TCP endpoint / 条件编译），但本轮只在 macOS 实机验证。
- P1 用 QEMU `snapshot=on` 临时覆盖层，基础镜像只读；跨 boot 持久化（GUI-8 布局）留后续。
- PNG 用 zlib stored block（无压缩），480×1000 约 1.4 MiB；后续换压缩编码。

## 测试

```bash
cd tools/berry
cargo test --target "$(rustc -vV | sed -n 's/^host: //p')"
just berry-smoke     # 仓库根；需要 out/berryos.img
```
