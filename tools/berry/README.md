# berry — BerryOS 开发者设备壳（host tool）

一个宿主二进制，三种角色：

- **CLI client**：`doctor / run / screenshot / input / ...` —— 可编程、可 CI/AI；
- **GUI client**：双击/无参数 —— 手机比例窗口 + 快捷栏，调同一套控制面；
- **session host**：同一二进制持有 QEMU/QMP/帧源，CLI/GUI attach（C1 收口）。

CLI 是主接口；窗口按钮只是命令的按钮化，任何 UI 动作都应有等价 CLI。

设计依据：`docs/Berry设备平台-CLI-first设计.md`（平台/CLI/控制面/快捷栏单一事实源）+
`docs/berry设备壳设计草案.md`（显示/帧面/QEMU 子系统）+ ADR #62 追记。**QEMU 只是引擎子进程**：
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
just berry screenshot --out out/shot.png --settle-ms 500   # 抓正在跑的 shell，不重启
just shell                                  # GUI 优先（out/berryos.img）/ console 回退
just desktop                                # GUI 桌面：out/berryos-portrait.img + out/gui-data.img + tablet + 1080×1920@60
just berry shell --profile phone --timeout 30
just berry device list
just berry device status --run <id>
just berry logs --run <id> --tail 50
just berry power reboot --run <id>
just berry power off --run <id>
just berry input key ret
just berry input tap 400 300
just berry input swipe 400 500 400 200 --ms 300
just berry input text hello
just berry tap 240 500
just berry key ret
just berry text hello
just berry-smoke                            # doctor + kernel marker + EFI shot
```

`berry shot` 起一台**临时**设备：QEMU（`-display none -vnc`）→ 等串口标记 → 从 VNC 抓一帧 →
写 PNG → 停设备。`berry screenshot` **不启动**新设备，直接 attach 正在跑的 shell（VNC 多客户端）
抓一帧；这是 CLI/AI 对「当前界面」截图的主路径。`berry shell` 用 `-display none` + `memory-backend-file` 起同一套设备：
串口 `FB ok 480x1000 … fb=0x…` 给出几何/PA，`RamSource` 直接读 `ram.bin`，窗口按
19:9 等比缩放显示；VNC 只作输入通道（不请求帧），`berry input` 可与 shell 并存。

## CLI-first 控制面（现状）

目标：会话（session host）持有一台设备，CLI/GUI 都 attach 到同一 `control.sock`；窗口按钮
与 CLI verb 一一对应，AI/CI 不依赖窗口。

| 动作 | CLI（当前/规划） | 窗口快捷栏 |
| --- | --- | --- |
| 截图 | **`berry screenshot`（已落）** / `shot`（临时设备） | 截图按钮 / `Cmd+S` |
| 输入 | `berry input key/tap/swipe/text`（VNC attach，已落） | 客机触摸/键盘 |
| 重启/关机 | **`berry power reboot/off`（C1a）** / `berry shortcut`（C2b） | BOOT/OFF 按钮（C2a） |
| 分辨率 | **`berry display set <profile> --restart`（C2c）** | **RES 菜单（C2d host / C2e attach）** |
| 全屏 | — | `F` / `FULL` 按钮 |
| 审计 | `berry events/logs`（C1/C2） | 同一事件流，`source=ui/cli` |

C1a/C1b/C1c 已落：`berry shell` 进程内或 headless `berry device start` 都会起
`control.sock` + QMP broker；CLI `device start/list/status/restart/stop`、`power off/reboot`、
`logs`、`screenshot`（优先走 host）已可用，CLI 不直连 QMP；`berry shell --attach <id>` 可给
headless host 开窗，关窗只 detach。C1d/C1e/C1f 已落（state/cleanup/TCP）；
C1g 的 host 自动重启已落：`device start --restart-on-crash`（QEMU 异常退出后同 run
原地重启；attach 窗口在 control 恢复后自动重连 VNC）；还剩 Windows 实机验证/打包。

## 环境变量

| 变量 | 作用 |
| --- | --- |
| `BERRY_HOME` | run dir 根（默认 `~/.berry`，Windows `%LOCALAPPDATA%\BerryOS`） |
| `BERRY_QEMU` | 指定 `qemu-system-aarch64` |
| `BERRY_KERNEL` / `BERRY_IMAGE` | 默认内核/镜像 |
| `BERRY_DATA_DISK` | GUI 桌面数据盘（FAT12 + `BERRYOSVBLK!`；launcher 需它拿 fs/app/input gift） |
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
| `desktop` | 540×960 | 1080×1920 | GUI 9:16 竖屏桌面档（`just desktop`） |
| `phone` | 1080×2250 | 480×1000 | 默认；恰好用满 800×600 GOP 缓冲 |
| `phone16` | 1080×1920 | 480×854 | 16:9 |
| `phone20` | 1080×2400 | 540×1200 | 已支持：buddy 4MB 连续 fb + ramfb 重编程 |
| `tablet` | 1600×2560 | 800×1280 | 已支持：buddy 4MB 连续块 + ramfb 重编程 |

`phone*`/`tablet` 默认内存 **256MiB**（`MMAP ok` + ROOTFS/GUI）；`default` 128MiB，
`--memory 128` 仍零回归。gui8 540 预览镜像也已换 256 内核。

## 已知限制（P2）

- QEMU 是外部依赖（GPLv2，进程边界）；主仓 Apache-2.0 不打包 QEMU。
- 帧面要求 QEMU `memory-backend-file,share=on`；不支持该后端的环境回退 VNC（P1 路径）。
- `berry input`/`berry screenshot` 走 VNC 多客户端 attach；QMP 由 host broker 独占，
  `power reboot/off`、`display set` 已落（C1a/C2c）；host 自动重启已落（--restart-on-crash）；
只剩 Windows 实机/打包。
- 窗口快捷栏 C2d 已落 5 键：`SHOT`/`RES`/`BOOT`/`OFF`/`FULL`（鼠标/触摸）；
  `OFF` 需 3s 内二次点击；`RES` 打开 profile 菜单，host 窗口内停旧/起新 profile。
- C2b 已落：`berry shortcut screenshot|reboot|power`、`berry display list/set`（记录
  next-profile）、`berry events`（JSONL）。
- C2c 已落 CLI：`berry display set phone16 --restart` 停旧 host、起新 profile host
  （新 run id，实测 480×854）。
- attach 窗口的 `RES` 已自动重附着：`restart_profile` 停旧 host/起新 profile host，
  窗口重连新 run VNC，无需手动 re-attach。按钮 hover 高亮 + tooltip 已落（C2f）；旧 run 清理/新 host 先起（失败不伤旧）= C2g。
`state.json` 已落（C1d）：正常 stop 显示 stopped；host 被杀后 list/status 显示 stale。
C1e：`berry device cleanup [--run|--all]` 对 stale/stopped run 杀 orphan QEMU 并删 run；
running 拒绝/跳过。示例：`berry device cleanup --all`。`power off` 走 ACPI 5s 优雅关机，
`--force` 立即 quit。
C1f：Unix 默认 `control.sock`；非 Unix 用 loopback TCP + `control.port`/`control.token`。
`BERRY_CONTROL_TCP=1` 可在 macOS 强制走 TCP 路径（已实测 status/logs/power）。
C2h：attach RES 新 host 失败保留旧 host；VNC 重连失败按旧 profile 回滚重连。
- 默认 `berry shot` 等 `BERRYBOOT ok`，当前测试镜像此时可能还在固件/测试面；要看 GUI
  请传 `--wait-marker "FBW ok"` 或 launcher 标记。`--profile phone` 时 guifbd 进入
  GUI 模式（console 只走串口，不盖桌面）；默认 800×600 仍保留 console 上屏。
- Windows/Linux 代码路径已写（TCP endpoint / 条件编译），但本轮只在 macOS 实机验证。
- P1 用 QEMU `snapshot=on` 临时覆盖层，基础镜像只读；跨 boot 持久化（GUI-8 布局）留后续。
- PNG 用 `miniz_oxide` deflate（zlib）；480×1000 干净桌面约 3.8KB，含图标/照片内容仍远小于 stored。

## 测试

```bash
cd tools/berry
cargo test --target "$(rustc -vV | sed -n 's/^host: //p')"
just berry-smoke     # 仓库根；需要 out/berryos.img
```
