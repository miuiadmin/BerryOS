# Berry 平台剩余缺口与验收矩阵

> 2026-11-04。配套：`Berry设备平台-CLI-first设计-v1.0.md`、`Berry设备平台-验收清单-v1.0.md`。
> 状态口径：✅ 已实测 / 🟡 有实现或脚本、未实机 / ⬜ 未落。

## P0（阻塞跨平台交付）

| 项 | 现状 | 为什么未做 | 验收证据 | 风险 |
| --- | --- | --- | --- | --- |
| Windows 实机 | 🟡 **已交叉构建 `out/berry.exe`**（PE32+ x64）+ Wine `--version`；未真机 | 本机无 Windows；Wine 只能验证启动 | Windows 上 `run-berry.cmd` → `berry shell`；`doctor` 绿；截图 | 高：QEMU/路径/GUI 行为 |
| Windows GUI subsystem | ✅ 双 exe：`berry.exe` console CLI + `berry-shell.exe` GUI subsystem | GUI 双击/窗口未在真实 Windows 验证 | zip 含两 exe；Wine console `--version` 通过 | 中：真实 Windows 交互 |
| Linux 实机 | ✅ arm64 Linux 容器 + QEMU TCG：`doctor`、480×1000、540×1200/256 GUI 截图通过 | 非桌面发行版/无窗口；AppImage 未产 | `out/linux-gui-540.png`；`smoke-docker.sh` | 中：GUI/QEMU 集成仍限容器 |
| Linux AppImage | ✅ arm64 20.7MB；extract + **FUSE 直接挂载** + `--version`/doctor 通过 | 桌面集成未验 | `smoke-appimage.sh`（`/dev/fuse`+SYS_ADMIN） | 中：只限容器 FUSE |

## P1（功能可见缺口）

| 项 | 现状 | 根因/前置 | 验收证据 | 风险 |
| --- | --- | --- | --- | --- |
| GUI-8 图标 dash | 🟡 540/256 全桌面可起，但图标占位 | `launcher_assemble` 在 `fs_gift==0` 时只给 `[wm]`，`app_cli_h()==0`；`icons::init` 空 registry | 串口 `LAUNCH v=1` 且桌面出现 APPDEMO/APPQUIET 图标 | 中：需 fsmgr/vblk 或改冻结相 [wm] |
| 状态栏时钟 | 🔴 bsh 12:00 硬编码占位（desktop.rs:102 TODO(W2) 分相号已闭卷） | berry_clock_read(0x28) 在册，frameloop 读时钟摆字即可 | 实施面小（~20 行） | 低 |
| data disk 图标实验 | ⬜ 失败已回退 | `vblk` 找到 slot 31，但 `fs_gift` 未成立；FAT/mkfat 有效 | `MMAP/ROOTFS/LAUNCH ok` + app registry | 中：可能污染 boot |
| 顶层 tap/key/swipe/text 别名 | ✅ 已落 | `input` 子命令 + 顶层 4 别名 | `berry tap 1 2` 等 | 低 |
| `display rotate` | ⬜ 仅 `display set <profile|WxH>` | 需客机/呈现面旋转 | `display rotate 90` 后截图方向正确 | 中 |
| `shortcut resolution/fullscreen` | 🟡 resolution 已做只读查询；fullscreen 窗口-only | CLI 无 profile 参数，菜单仍窗口侧 | `shortcut resolution` 返回 profile/render | 低 |
| `pack verify/install` | ⬜ C4 规划 | 无 runtime pack 格式 | 命令/JSON 契约 | 低 |
| attach 自动重连 GUI 实测 | 🟡 代码已接 `down_since`/`try_reconnect` | 无显示环境，GUI 进程级未跑通 | kill QEMU → attach 窗口恢复画面 | 中：VNC 重连时序 |

## P2（后续能力）

| 项 | 现状 | 说明 |
| --- | --- | --- |
| P3 virtio-gpu | ⬜ | 当前 ramfb + shared RAM；P3 再做 GPU/damage/120Hz |
| 视频编码/时间轴回放 | 🟡 APNG 帧序列；`play expect` | 不是视频容器；无音频/时间轴 |
| 窗口级截图/录屏 | ⬜ | 截图只含客机，不含宿主 toolbar |
| 剪贴板/IME/音频 | ⬜ | 窗口/客机协议未做 |
| 多设备编排 | ⬜ | 单 host/单 run；AI 批量需上层脚本 |
| 签名/公证 | ⬜ | macOS ad-hoc；无 notarization |

## 已实测基线（不要再回退）

- host **15 单测**；`docs_guard` PASS；`git diff --check` 空。
- `out/BerryOS.app` / `out/BerryOS-Desktop.app` 镜像哈希与 `out/*.img` 一致。
- `out/BerryOS.AppDir`：25 ok / 1 warning（macOS Mach-O，非 Linux ELF）/ 0 fail。
- 256MiB `MMAP ok`；540×1200/800×1280 `FBW`；host 自动重启；power off 5s；APNG；play expect。

## 下一步建议顺序

1. Windows CI/实机（P0）→ 2. GUI-8 icons（P1，需 GUI-8 分支）→ 3. Linux 实机/AppImage（P0）→ 4. P3。
