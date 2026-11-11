# Berry 设备平台验收清单

> 版本：**1.0**（v1.0）｜2026-11-06

> 配套：`Berry设备平台-CLI-first设计-v1.0.md`（单一事实源）+ `berry设备壳设计草案-v1.0.md`（显示/帧面子系统）。
> 标记：✅ 已实测 / 🟡 有实现未实机 / ⬜ 未落。所有命令默认 `--json` 可用。

## 0. 构建与门禁

- [ ] `just berry-build` 成功（host target，独立 workspace）。
- [ ] `cd tools/berry && cargo test --lib --target "$(rustc -vV | sed -n 's/^host: //p')"` → 15 passed。
- [ ] `python3 tools/docs_guard.py` → PASS。
- [ ] `git diff --check` → 空。
- [ ] `sh -n tools/berry/packaging/linux/build-appdir.sh` → 0。

## 1. 会话生命周期

```bash
RUN=$(berry device start --profile phone --json | jq -r .run)
berry device list --json
berry device status --run "$RUN" --json
berry device restart --run "$RUN" --json
berry device stop --run "$RUN" --json
```

- [ ] `device start` 默认后台 headless；`--foreground` 前台阻塞。
- [ ] `list` 显示 label=`host`、state=`running`、pid。
- [ ] `status` 返回 state/profile/memory/qemu_pid。
- [ ] `restart` 走 QMP `system_reset`，run 不变。
- [ ] `stop` 后 state=`stopped`，QEMU 退出。
- [ ] `cleanup` 对 stopped/stale 可删 run + 杀 orphan QEMU；running 拒绝；`--all` 批量清理。
- [ ] `power off` 优雅关机：system_powerdown→5s→quit；`--force` 直接 quit；工具栏 OFF 同优雅路径。

## 2. 截图 / 录帧

```bash
berry screenshot --run "$RUN" --out out/a.png --settle-ms 500 --json
berry record --run "$RUN" --out out/rec --frames 5 --interval-ms 300 --json
```

- [ ] `screenshot` 不启动新设备，PNG 尺寸=profile render。
- [ ] `shot` 仍是一次性设备（起 QEMU → 抓帧 → 停）。
- [ ] `record` 产出 `frame-0001.png...`；`--json` 返回 paths。
- [ ] `record --apng out.apng` 产出 APNG（acTL/fcTL/fdAT 结构合法）。
- [ ] PNG 走 miniz_oxide deflate（480×1000 约 4KB 级）。

## 3. 输入 / 脚本

```bash
berry input key ret --run "$RUN"
berry input tap 240 500 --run "$RUN"
berry play script.json --run "$RUN"
```

- [ ] `input key/tap/swipe` 到达客机。
- [ ] `play` 支持 `screenshot/tap/key/text/swipe/wait/power/display/expect`。
- [ ] `play` 失败标明 step 序号；`--json` 返回 steps。
- [ ] 顶层 `berry tap/key/swipe/text` 与 `input text` 已落；实测 4 条均 ok。

## 4. 控制面

- [ ] Unix：`<run>/control.sock`（0600），CLI 不直连 QMP。
- [ ] TCP：`BERRY_CONTROL_TCP=1` → `control.port`/`control.token`（0600），
      `device status/logs/power` 通过。
- [ ] `control::is_up` 同时识别 socket/port（TCP 下 list/screenshot/cleanup 正常）。
- [ ] `stats` 返回 state/profile/memory/qemu_pid/started。
- [ ] `events` 可读 `display/screenshot/power.*` JSONL。
- [ ] QMP 只由 host broker 持有；CLI 直连 QMP 为 0。

## 5. 窗口 / 快捷栏

- [ ] `berry shell --profile phone` 打开 480×1000 窗口（256MiB）。
- [ ] `berry shell --attach "$RUN"` 开窗；关窗只 detach，host 仍 running。
- [ ] 快捷栏 5 键：`SHOT`/`RES`/`BOOT`/`OFF`/`FULL`；hover 高亮 + tooltip。
- [ ] `OFF` 3s 内二次点击才关机；`RES` 菜单当前 profile 高亮。
- [ ] host 窗口 `RES` 换 profile；attach 窗口换 profile 后自动重连新 run。
- [ ] 新 host 失败保留旧 host；VNC 重连失败按旧 profile 回滚。
- [ ] 键盘：`Cmd/Ctrl+S` 截图、`Cmd/Ctrl+R` 复位、`F` 全屏、`Cmd/Ctrl+Q` 退出。
- [ ] `Cmd/Ctrl+Shift+R/Q`、hover/tooltip 扩展快捷键：⬜ 未落/部分。

## 6. 显示档位

| profile | 渲染 | 验收 |
| --- | --- | --- |
| default | 800×600 | `FB ok 800x600` |
| phone | 480×1000 | ✅ `FB ok 480x1000 stride=1920` |
| phone16 | 480×854 | ✅ `FB ok 480x854` |
| phone20 | 540×1200 | ✅ `DISP defer`→`DISP ok`→`FB ok stride=2160` |
| tablet | 800×1280 | ✅ `FB ok 800x1280 stride=3200` |

- [ ] 256MiB：`MMAP ok segs=35` + `ROOTFS ok` + `FBW ok`；128MiB 仍 `MMAP ok segs=31`。
- [ ] `phone*/tablet` 默认 256MiB；`--memory 128` 可回退。
- [ ] 540/800 超出 GIF GOP 缓冲时走 buddy 连续块；失败回退并补 `FB ok`。

## 7. 崩溃 / 状态

- [ ] `kill -9 <host pid>` 后 `device list/status` 显示 `stale`（state.json + control 缺失）。
- [ ] `berry device cleanup` 杀掉 orphan QEMU 并删 run。
- [ ] `state.json` 状态集：starting/running/rebooting/stopping/stopped/failed。
- [ ] host 自动重启：`device start --restart-on-crash`，kill QEMU 后同一 run 从
      `rebooting` 回 `running`，qemu_pid 变化，截图恢复；✅ 已实测。attach 窗口
      control 恢复后应自动重连 VNC。

## 8. 打包

- [ ] macOS：`tools/berry/packaging/macos/build.sh` → `out/BerryOS.app`；
      ad-hoc 签名；无 TTY 启动资源自解析。
- [ ] macOS 桌面预览：`out/BerryOS-Desktop.app`（540/256 GUI-8 镜像）。
- [ ] Linux：`build-linux-binary.sh` + `build-appdir.sh` + `check-appdir --require-elf`
      → 真实 aarch64-musl ELF AppDir；`smoke-docker.sh` 在 arm64 Linux 容器里
      doctor/480/540 GUI 截图通过。✅ 容器实机；AppImage 20.7MB（extract 验证）；
      AppImage FUSE 直接挂载 + `--version`/doctor 通过；桌面发行版集成未验。
- [ ] Windows：`build-cross-docker.sh` → `out/berry.exe`（PE32+ x64）；
      `assemble.sh` → `out/BerryOS-win64.zip`；`smoke-wine-docker.sh` → `berry 0.1.0`。
      🟡 未真实 Windows；`berry.exe` console + `berry-shell.exe` GUI subsystem 双入口。
- [ ] QEMU 不打包；Runtime Pack 独立。✅ 进程边界。

## 9. 已知缺口

- GUI-8 桌面图标仍 dash（launcher app registry/app channel；归 GUI-8 app 服务）。
- Windows 实机/打包（C1g）；host 自动重启已落。
- P3：virtio-gpu / damage / 120Hz / 视频录屏 / 时间轴回放。
- 顶层 `berry tap/key/swipe/text` 别名、`input text`、`display set WxH` 已落 2026-11-05（12fab4a/e783303/f289d44）；实测注见 §3，剩余缺口表 ✅ 行互证。唯 `display rotate` 未落（`display set <profile|WxH>` 已可设，旋转归剩余缺口表「需客机/呈现面旋转」项）。

## 10. 最小 AI/CI 闭环

```bash
RUN=$(berry device start --profile phone --json | jq -r .run)
berry screenshot --run "$RUN" --out before.png --json
berry play script.json --run "$RUN" --json
berry screenshot --run "$RUN" --out after.png --json
berry power off --run "$RUN" --json
```

- [ ] 无窗口完成；每步 JSON/退出码稳定。
- [ ] 失败返回 serial tail/hint；`device cleanup` 可收尾。
