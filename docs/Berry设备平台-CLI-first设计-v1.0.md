# Berry 设备平台（CLI-first）设计

> 版本：**1.0**（v1.0）｜2026-11-06

> 【状态】2026-11-04 立稿；ADR #62 追记。**本文 = 产品形态 / CLI / 控制面 / 窗口快捷栏 / 刀序的单一事实源。**
> 【关系】`berry设备壳设计草案-v1.0.md` 降为**显示/帧面/QEMU/客机侧子系统**；两文冲突时以本文为准。
> 【一句话】一个 `berry` 二进制，三种角色：CLI client / GUI client / session host；
> CLI 是主接口，GUI 是同一控制面的客户端，QEMU 只是引擎子进程。

---

## 1. 问题与结论

### 1.1 今天缺什么

- `berry shot` 每次都**起一台新设备**，不能对正在跑的窗口截图。
- host 独占 QMP；CLI 只走 control.sock/TCP，窗口/attach/input 都不抢 QMP。
- 窗口只有键盘快捷键，没有**可见快捷栏**；按钮即使做了，也没有和 CLI 共享动作表。
- 没有会话（session）概念：CLI 无法列设备、查状态、重启、关机、改分辨率、等一帧。
- AI/CI 只能「起新机 + 抓一帧」，不能做「看当前画面 → 点一下 → 再看 → 重启」的闭环。

### 1.2 结论

**分发单位 = 单二进制；产品形态 = 本地设备平台。**

- **CLI 是主接口**：截图、输入、电源、分辨率、快捷动作都有语义 verb。
- **GUI 是客户端**：窗口按钮/快捷键调用同一 `ControlClient`，没有 UI-only 路径。
- **session host**：同一二进制持有 QEMU/QMP/帧源，暴露本机 `control.sock`；CLI/GUI attach。
- **QMP 只由 host 持有并做 broker**；CLI 永不直连 QMP。
- **v1 不做 SaaS/云设备农场**；本地优先、零账号、零网络依赖。

---

## 2. 二进制还是平台？

| 维度 | 纯二进制 | 重平台/SaaS | 本方案：单二进制本地平台 |
| --- | --- | --- | --- |
| 分发 | 一个可执行文件 | 客户端 + 服务端 + 账号 | **一个 `berry`** |
| 会话 | 一次性，无状态 | 云端长活 | **本机 session host** |
| CLI | 有，但只能起新机 | 有 SDK，重 | **CLI 全覆盖 + attach** |
| GUI | 可选 | Web/Electron | **原生 client，与 CLI 同源** |
| 自动化 | 弱 | 强但依赖云 | **强，且离线可跑** |
| 失败隔离 | QEMU 进程 | 云端隔离 | **QEMU 进程 + host 状态机** |

**平台契约四件**：

1. **CLI verbs**：稳定命令名/参数/语义。
2. **JSON schema + 退出码**：机器可读、可断言。
3. **本机 control 协议**：`control.sock` 的请求/应答/事件。
4. **profile / runtime pack**：设备几何、QEMU/固件/镜像的版本化组合。

这四件稳定后，引擎可换（QEMU/Virtualization.framework/真机），数据面可换
（VNC/共享 RAM/virtio-gpu），窗口/截图/输入/回放不用改。

### 2.1 对标结论

| 参考 | 形态 | 借用 | 不照搬 |
| --- | --- | --- | --- |
| Android Emulator console + adb | 长活 emulator + `adb`/console；`adb exec-out screencap` | 会话/命令分离；截图/电源/网络都有 verb | 不做 adb 兼容层；不做第二套文本协议 |
| iOS `xcrun simctl` | 单 CLI 命名空间；`simctl io booted screenshot`、`shutdown` | 设备 id + 简单 verb + 脚本友好 | 不做 Xcode 工程耦合 |
| QEMU QMP | JSON 控制面；`screendump/system_reset/system_powerdown/quit` | 引擎后端白名单；事件驱动状态机 | QMP 单客户端 → host broker；CLI 不直连 |
| Docker CLI/daemon | client/daemon 分离；稳定 API/context | 本机 socket + 版本化请求；CLI 薄客户端 | 不做容器/仓库重生命周期 |
| Playwright / Maestro | CLI-first 测试；确定性命令/JSON | `--json`/退出码/`--wait`/可重放 | 不用 UI 坐标当 API |

---

## 3. 目标与非目标

### 3.1 目标（按优先级）

| # | 目标 | 判据 |
| --- | --- | --- |
| G1 | CLI 覆盖所有窗口动作 | 快捷栏每个按钮都有等价 CLI；CLI 可独立触发 |
| G2 | CLI 附着正在跑的设备 | `screenshot`/`key`/`tap`/`power` 不重启设备 |
| G3 | 会话可管理 | `list/status/stop/restart` 稳定；run dir 可审计 |
| G4 | 原生快捷栏 | 截图/分辨率/重启/关机/全屏五按钮；客机卡死仍可用 |
| G5 | 三平台双击 | macOS `.app`、Windows GUI 子系统、Linux AppImage |
| G6 | AI/CI 可编程 | `--json`、稳定退出码、`--wait`、事件/截图可回放 |

### 3.2 非目标（v1）

- 云设备农场、账号体系、多人共享设备。
- 把 QEMU 链接进主仓/单二进制（GPL 边界）。
- 用 UI 坐标（`click x y`）当平台 API；坐标只是 `tap` 的一个参数。
- 在客机里加「给 CLI 用的后门」；CLI 是宿主控制面。
- 追 Android 模拟器/浏览器 DevTools 的全量协议。

---

## 4. 用户与关键场景

### 4.1 AI agent 闭环

```bash
berry device start --profile phone --json   # 默认后台 headless；得到 session id
berry screenshot --run <id> --out before.png --json
berry input tap 240 500 --run <id> --json
berry input key ret --run <id> --json
berry screenshot --run <id> --out after.png --json
berry power reboot --run <id> --json
```

要求：每步有稳定 JSON/退出码；截图带 `run/profile/width/height/source`；失败带
`serial_tail`/`hint`；不需要窗口/AI 视觉坐标。

### 4.2 CI

```bash
berry shot --image out/berryos.img --profile phone \
  --wait-marker "LAUNCH" --out out/ci.png --json
```

`shot` 是**一次性**设备：起 QEMU → 等标记 → 抓帧 → 停；产物 + manifest 哈希可复现。

### 4.3 开发者双击

双击 `.app`/`.exe`/AppImage → 窗口（内部 session host）→ 快捷栏。CLI 可同时 attach；
关窗口的策略：询问「仅关窗口（session 继续）/ 关机（session 结束）」。

### 4.4 调试/回放

`berry logs -f`、`berry events`、截图序列 + 输入序列 = 最小复现包；C3 做 replay。

---

## 5. 架构

### 5.1 进程/角色

```
┌─────────────┐   ┌─────────────┐   ┌─────────────┐
│ CLI client  │   │ GUI client  │   │ AI/CI       │
│ berry ...   │   │ winit 窗口  │   │ berry ...   │
└──────┬──────┘   └──────┬──────┘   └──────┬──────┘
       │                 │                 │
       └────────── control.sock（NDJSON）───┘
                         │
                 ┌───────▼────────┐
                 │ session host   │  同一 berry 二进制（隐藏角色）
                 │ 状态机/事件     │
                 ├────────────────┤
                 │ QMP broker     │  唯一 QMP 客户端
                 │ FrameSource    │  VNC / shared RAM / future
                 │ InputSink      │  VNC / future
                 │ RunStore       │  run dir / state.json
                 └───────┬────────┘
                         │ spawn / signals
                 ┌───────▼────────┐
                 │ QEMU 子进程     │  -display none
                 └────────────────┘
```

- **单二进制**：`berry` 默认是 CLI；`berry shell` 起 host + GUI；`berry device serve`
  起 headless host。GUI 与 host 可同进程（v1），也可拆进程（C1 可选）。
- **QEMU 是唯一外部引擎**；GPL 进程边界不变。
- **数据面不经过 control.sock**：帧走 shared RAM/VNC；control 只传命令/事件/元数据。
- **control.sock 是本机唯一控制入口**；QMP/串口是 host 内部实现。

### 5.2 组件职责

| 组件 | 职责 | 不做什么 |
| --- | --- | --- |
| `cli` | 解析 argv、调用 ControlClient、渲染人读/JSON | 不直接碰 QMP/QEMU |
| `session host` | 生命周期、QMP broker、事件、run store | 不画窗口 |
| `ControlClient` | 连接/重连/请求关联/事件订阅 | 不解析业务 |
| `QmpBroker` | 串行化 QMP、query/event、超时 | 不暴露给 CLI |
| `FrameSource` | `ScreenSource` trait（VNC/shared RAM/virtio-gpu） | 不决定窗口布局 |
| `InputSink` | keysym/pointer 注入 | 不做快捷键语义 |
| `RunStore` | `<run>` 目录、state.json、日志、清理 | 不做设备策略 |
| `Profile` | 几何/内存/刷新/传输 | 不写死 QEMU argv |
| `GUI` | 窗口、缩放、快捷栏、确认框 | 不直接调 QMP |
| `RuntimePack` | QEMU/固件/镜像版本组合 | 不塞进主仓发行物 |

### 5.3 模块落点（tools/berry）

```
src/
  main.rs          CLI 入口（clap）
  commands.rs      现有命令实现（逐步迁到 client/session）
  session/         host、生命周期、run store
  control/         protocol、client、server
  qmp.rs           QMP 客户端（host 内部）
  display/         ScreenSource: vnc.rs / ram.rs / scale.rs
  input.rs         winit/keysym
  window.rs        GUI client
  profile.rs       几何/内存
  png.rs           PNG（deflate）
  pack.rs          Runtime Pack（C4）
```

---

## 6. 会话模型

### 6.1 run 目录

```
~/.berry/run/<id>/
  state.json        # 状态、pid、updated（starting/running/rebooting/stopping/stopped/failed）
  meta.json         # 镜像/固件/内核哈希、QEMU 版本
  control.sock      # Unix socket（Windows: loopback TCP + token）
  qmp.sock          # host 独占
  vnc.sock          # 帧/输入（v0）
  ram.bin           # shared RAM（v1）
  serial.log
  qemu.log
  events.jsonl
  vars.fd
  data.img          # C2 数据盘（可选）
```

<id> = `<unix_ms>-<pid>-<label>`；`label` = `shell|shot|<custom>`。目录 0700；
socket 0600；token 0600，**不进 argv**。

### 6.2 状态机

```
Created → Starting → Running ⇄ Paused → Rebooting/Stopping → Exited
                         ↘ Failed
```

| 状态 | 进入 | 退出/事件 |
| --- | --- | --- |
| Starting | spawn QEMU + QMP 握手 | 首帧/首标记 → Running；超时 → Failed |
| Running | 正常 | stop/reboot/crash |
| Paused | `stop` | `cont` |
| Rebooting | `system_reset` | RESET/首帧 → Running |
| Stopping | `quit`/SIGTERM | 进程退出 → Exited |
| Failed | QEMU 崩溃/启动超时/协议错 | 保留 run dir；GUI 可重启 |

### 6.3 生命周期策略

- `device start`：默认后台（spawn 同二进制 `host` 角色，打印 run id）；`--foreground` 调试。
- `shell`：起 host + GUI；`--attach <id>` 只开窗口。
- `shot`：ephemeral；结束自动清理（`--keep` 保留）。
- `stop`：`quit` → 5s SIGTERM → 2s SIGKILL；回收子进程。
- 崩溃：`state.json` 标 `failed`；`list` 显示；`berry logs` 可看尾巴。

---

## 7. 控制协议

### 7.1 传输

- macOS/Linux：Unix domain socket `<run>/control.sock`，0600。
- Windows / `BERRY_CONTROL_TCP=1`：`127.0.0.1:<port>` + 随机 token；写
  `control.port`/`control.token`（token 0600）；每个请求带 token，服务端校验。
- 默认只接受本机；不开公网；C3 远程必须显式 `--listen` + token/证书。

### 7.2 帧格式

NDJSON：一行一个 JSON 对象。

请求：

```json
{"v":1,"id":7,"op":"screenshot","args":{"out":"shot.png","settle_ms":0}}
```

应答：

```json
{"v":1,"id":7,"ok":true,"result":{"path":"shot.png","width":480,"height":1000,"source":"ram"}}
{"v":1,"id":7,"ok":false,"code":"E_BUSY","message":"device is rebooting"}
```

事件（订阅后推送）：

```json
{"v":1,"t":"state","run":"...","from":"running","to":"rebooting"}
{"v":1,"t":"frame","run":"...","seq":42,"w":480,"h":1000}
{"v":1,"t":"log","run":"...","line":"LAUNCH v=1 ..."}
```

纪律：`v` 版本、`id` 请求关联、`ok` 布尔、稳定错误码；未知 op = `E_UNSUPPORTED`，
不静默；单连接串行，事件异步；请求超时由客户端 `--timeout` 控制。

### 7.3 op 表

| op | args | result | 说明 |
| --- | --- | --- | --- |
| `hello` | `client` | `version/capabilities` | 握手/能力 |
| `status` | — | `state/profile/memory_mib/qemu_pid/started` | 查状态 |
| `screenshot` | `out,settle_ms` | `path,w,h,source` | 抓当前帧 |
| `key` | `code` | — | qcode/ASCII；按下+抬起 |
| `tap` | `x,y` | — | 客机像素 |
| `swipe` | `x1,y1,x2,y2,ms` | — | 插值 |
| `text` | `text` | — | 逐字符 keysym |
| `power` | `action:off|reboot,force?` | `state` | QMP |
| `display` | `profile,restart?` | `applied/requires_restart` | v1 重启；WxH 走 CLI 新 host，rotate 未落 |
| `shortcut` | `action:screenshot|resolution|reboot|power` | `result` | resolution 只读查询；fullscreen 窗口-only |
| `logs` | `tail?` | `lines` | 串口尾 |
| `stats` | — | `state/profile/memory_mib/qemu_pid/started` | 会话统计 |
| `events` | `tail?` | `lines` | events.jsonl 尾 |
| `stop` | — | — | 结束 session |

### 7.4 错误码

| code | 含义 | CLI 退出码 |
| --- | --- | --- |
| `E_USAGE` | 参数错 | 2 |
| `E_NO_SESSION` | 无设备/找不到 run | 3 |
| `E_TIMEOUT` | 等帧/等标记超时 | 4 |
| `E_BUSY` | 正在重启/停止 | 5 |
| `E_DEVICE` | QEMU/启动失败 | 6 |
| `E_PROTOCOL` | QMP/control 协议错 | 7 |
| `E_UNSUPPORTED` | 未实现 op | 2 |

`--json` 失败也输出 `{"ok":false,...}` 到 stdout；人读错误走 stderr；二者不混。

---

## 8. CLI 规格

### 8.1 命令树

```
berry [--json] [--verbose] <command>     # --run 是会话类子命令参数，非全局

device
  start [--image] [--profile] [--foreground] [--memory] [--accel] [--display-mode] [--keep] [--restart-on-crash] [--json]
  list [--json]
  status [--run] [--json]
  stop [--run] [--json]
  restart [--run] [--json]
  cleanup [--run] [--all] [--json]

shell [--attach <id>] [--profile] [--scale] [--timeout]
run [--kernel|--image] [--marker] [--timeout]

screenshot [--run] [--out] [--settle-ms] [--json]
shot [--image] [--profile] [--accel] [--memory] [--display-mode] [--wait-marker] [--delay-ms] [--timeout] [--out] [--keep] [--json]

input key|tap|swipe|text <...>
tap <x> <y> [--run]              # input tap 顶层别名
key <code> [--run]               # input key
swipe <x1> <y1> <x2> <y2> [--ms] [--run]
text <str> [--run]
power off|reboot [--run] [--force] [--json]
display list|set <profile|WxH> [--restart] [--run] [--json]
# WxH 自定义需 --restart（起新 host + display_mode）；rotate 未落

shortcut screenshot|resolution|reboot|power [--run] [--json]

logs [--run] [-f] [--tail N] [--json]
events [--run] [--json]
record [--run] --out <dir> --frames N --interval-ms MS [--apng file] [--json]
play <file.json> [--run] [--json]     # 步骤：screenshot/tap/key/text/swipe/wait/power/display/expect
stats [--run] [--json]
doctor [--json]
pack verify|install <path>       # C4 规划，未落
version
```

**C1a 已落（shell-hosted）**：`device list/status/stop`、`power off/reboot`、`logs`；
`control.sock` NDJSON + QMP broker 在 `berry shell` 进程内生效；CLI 不直连 QMP。
**C1b 已落**：headless `device start`（默认后台，`--foreground` 阻塞）、`device restart`、
`berry screenshot` 优先走 host control（VNC 兜底）。`device start` 的 host 进程标签 = `host`。
**C1c 已落**：`berry shell --attach <id>` 打开已有 session（VNC 帧/输入 + control 动作）；
关窗只 detach，host 继续。C1d/C1e/C1f 已落（state.json、cleanup、TCP+token）；
**C1g 已落 host 自动重启**：`device start --restart-on-crash`，QEMU 异常退出后在
同一 run/control 路径原地重启（rebooting→running；CLI 重连；attach 窗口在 control
恢复后自动重连 VNC；旧 control/VNC 短断）。C1g 剩余：Windows 实机验证/打包。

### 8.2 语义

- **`shot` vs `screenshot`**：`shot` 起临时设备并抓一帧；`screenshot` attach 现有设备，
  不启动、不停止。
- **`shell`**：打开窗口；headless 用 `device start`；`shell --attach` 附着已有 session；双击兼容。
- **`--run`**：默认最新 running session；多设备时必填；`--json` 结果含 `run`。
- **`--wait`**：阻塞到动作完成/超时；`--timeout` 秒。
- **`--settle-ms`**：截图前等待（默认 0）；用于桌面首帧/动画后。
- **幂等**：`stop` 已停 = 成功；`power reboot` 重启中 = `E_BUSY`。
- **不拼 shell**：QEMU argv 数组直传。

### 8.3 示例

```bash
# 起 headless 会话
RUN=$(berry device start --profile phone --json | jq -r .run)

# 看当前画面
berry screenshot --run "$RUN" --out before.png --settle-ms 500

# 操作
berry input tap 240 500 --run "$RUN"
berry input key ret --run "$RUN"

# UI 同款动作
berry shortcut screenshot --run "$RUN"

# 电源
berry power reboot --run "$RUN"
berry power off --run "$RUN"

# 观察/收尾
berry logs --run "$RUN" --tail 50
berry events --run "$RUN"
berry device stop --run "$RUN"
```

---

## 9. 窗口与快捷栏

### 9.1 布局

```
┌─────────────────────────────┐
│                             │
│      客机帧（480×1000）      │  ← fit/整数缩放，letterbox
│                             │
├─────────────────────────────┤
│ [SHOT] [RES] [BOOT] [OFF] [FULL] │  ← 48px 原生快捷栏（5 键）
└─────────────────────────────┘
```

- 快捷栏是 **winit 自绘宿主 UI**，不占客机分辨率，不画假刘海/假按键。
- **C2a 已落（4 键）**：`SHOT` 截图、`BOOT` 重启、`OFF` 关机、`FULL` 全屏。
- **C2b 已落**：`OFF` 二次确认（3s 内再点）、`berry shortcut screenshot|reboot|power`、
  `berry display list`、`berry display set <profile>`（记录 next-profile）、`berry events`。
- **C2c 已落（CLI）**：`berry display set <profile> --restart` 停旧 host → 以新 profile
  起新 host（新 run id）；实测 phone→phone16 = 480×854。
- **C2d 已落（host 窗口）**：`RES` 按钮打开 profile 菜单（5 项）；选新 profile 后
  **窗口内停旧 host、按新 profile 起新 host**（新 run），窗口继续。
- **C2e 已落（attach 窗口）**：`RES` 选新 profile → `restart_profile` 停旧 host/起新 host
  → 重新连新 run 的 VNC，窗口自动重附着；无需手动 `shell --attach`。
- **540×1200 预览件已落**：GUI-8 全桌面 + 540 内核 → `out/berryos-desktop.img` /
  `out/berry-desktop-540x1200.png`；桌面 app 已换该镜像，`RES → phone20` 可到 540×1200。
- **C2f 已落（交互）**：按钮 hover 高亮 + 顶部 tooltip（SCREENSHOT/PROFILE/REBOOT/POWER/FULLSCREEN）。
- **C2g 已落**：`restart_profile` 改「先起新 host 再停旧 host」——新 host 失败则旧 host
  原样保留（天然回滚）；成功后旧 run 按 `keep` 清理（`keep=false` 删目录）。
- **C2h 已落（attach 失败）**：attach RES 新 host 起不来 → 旧 host 原样保留；新 host 成功但
  VNC 重连失败 → 自动按旧 profile 再起一次并重连（rollback）；仍失败才退窗；attach 前
  control 缺失给 `device cleanup` 提示。
- **C1d 已落（state/崩溃可见）**：`state.json` 落 starting/running/rebooting/stopping/
  stopped/failed；`device list/status` 在 control 缺失时读 state，running 但无 control 标
  `stale`；`device stop` 对已停 run 幂等，对 stale 给诊断。
- **C1e 已落（崩溃清理）**：meta 记 `qemu_pid`；`berry device cleanup [--all]` 对 stale/stopped run
  先 TERM/KILL orphan QEMU，再删 run dir；running session 拒绝 cleanup。
- **C1f 已落（TCP+token）**：非 Unix 用 `127.0.0.1:<port>` + `control.port`/`control.token`
  （0600）NDJSON；Unix 默认仍 `control.sock`，`BERRY_CONTROL_TCP=1` 可强制 TCP 测试。
  已在 macOS 用 TCP 模式跑通 status/logs/power off；Windows 走同一 TCP 代码路径。
- 按钮 disabled 态：重启/关机进行中；tooltip 显示快捷键 + 等价 CLI。
- 截图：闪一下 + 落 `berry-shot-<ts>.png` + 事件 `source=ui`。
- 关机/重启：确认框；进行中显示遮罩；失败显示 `code/message` + 日志入口。
- 分辨率：profile 列表；v1 选后提示「需重启」并调 `power reboot`（C2 定义）。
- 全屏：隐藏快捷栏/标题，`Esc` 退出。

### 9.2 键位

| 键 | 动作 | CLI |
| --- | --- | --- |
| `Cmd/Ctrl+S` | 截图 | `berry screenshot` |
| `Cmd/Ctrl+Shift+R` | 重启 | `berry power reboot` |
| `Cmd/Ctrl+Shift+Q` | 关机 | `berry power off` |
| `F` | 全屏 | —（窗口专属） |
| `+/-` | 缩放 | `berry shell --scale`（窗口内） |
| `Esc` | 退全屏 | — |
| `I` | 信息悬浮 | `berry stats` |
| `Cmd/Ctrl+Q` | 关窗口 | 询问（session 继续/关机） |

### 9.3 动作表 = 单一事实源

```rust
enum UiAction { Screenshot, Resolution, Reboot, PowerOff, Fullscreen }
```

- GUI 按钮 → `ControlClient::shortcut(action)`
- CLI `berry shortcut <action>` → 同一 op
- 事件记录 `source=ui|cli`、`run`、`action`、`ok`、`code`
- 任何新按钮先加 action + CLI，再画 UI；禁止 UI 直接调 QMP/QEMU。

---

## 10. 显示/分辨率/电源语义

### 10.1 Profile

| id | 客机渲染 | 状态 |
| --- | --- | --- |
| `default` | 800×600 | 兼容/CI |
| `phone` | 480×1000 | 已落（恰好用满 800×600 GOP 缓冲） |
| `phone16` | 480×854 | 可用 |
| `phone20` | 540×1200 | **可用**：buddy 4MB 连续 fb + ramfb 重编程 |
| `tablet` | 800×1280 | **可用**：buddy 4MB 连续块 + ramfb 重编程 |

### 10.2 `display set`

- **`display set`（C2b）**：把 profile 写成 `next-profile`，返回
  `{"applied":false,"requires_restart":true}`；**不假装热切换**。
- **`display set --restart`（C2c，已落）**：停当前 host → 以新 profile 起新 host
  （新 run id，旧 run 保留为 no-control）；CLI 可立即改 480×854/480×1000 等。
- **GUI `RES`（C2d host / C2e attach）**：菜单选 profile → 停旧 host、按新 profile 起新 host；
  host 窗口内换 Device，attach 窗口重连新 run VNC，窗口继续。
- **v2**：host/guest display 服务协商热切换（ramfb 重编程/virtio-gpu modeset）；
  需要客机侧协议，另立 ADR。
- `display rotate` 未落：需客机/呈现面支持，C2+ 评估；当前不要承诺 0/90/180/270。

### 10.3 电源

- `power off`：QMP `system_powerdown` → 等 5s → `quit`；`--force` 直接 `quit`。
- `power reboot`：`system_reset`；session id 不变，状态 `rebooting → running`。
- `stop`：结束 session（不是客机优雅关机；等价关窗口+关机）。
- 所有电源动作有确认（GUI）/`--yes`（CLI，避免脚本误触）。

---

## 11. AI/CI 契约

- **无窗口**：所有命令 headless 可用；GUI 只是客户端。
- **结构化**：`--json` 成功/失败都输出一个 JSON 对象；流式命令（logs/events）JSONL。
- **稳定**：字段名/错误码/退出码版本化；`v:1`；新增字段不破坏旧消费者。
- **可等待**：`--wait`/`--timeout`/`--settle-ms`；超时返回 `serial_tail`/`hint`。
- **可审计**：`events.jsonl` 记录 state/action/source；截图带 `run/profile/source`。
- **可回放（C3）**：截图序列 + 输入序列 + 事件 → 最小复现包；不依赖 AI 视觉。
- **安全**：不把 token 放 argv；不默认监听公网；输入注入只在本机 session。

---

## 12. 打包与分发

| 平台 | 形态 | 备注 |
| --- | --- | --- |
| macOS | `.app`（`Contents/MacOS/berry` + Resources） | ad-hoc/Developer ID + 公证；QEMU 外部 |
| Windows | `berry.exe`（GUI 子系统） | 终端启动 `AttachConsole`；QEMU 外部 |
| Linux | AppImage + `.desktop` | `Terminal=false`；QEMU 由发行版/包提供 |

- **单二进制**；镜像/固件/QEMU 走 Runtime Pack 或外部安装。
- `BERRY_HOME`：`~/.berry` / `%LOCALAPPDATA%\BerryOS`；run dir 0700。
- 版本：`berry --version`、manifest 哈希、QEMU 版本进 `meta.json`。
- 不进主仓：QEMU 二进制、GPL 代码；Runtime Pack 单独分发。

---

## 13. 安全与威胁模型

| 威胁 | 对策 |
| --- | --- |
| 本机其他用户连 control/QMP | 0700 run dir、0600 socket、token 0600 |
| 恶意镜像/客机逃逸 | QEMU 进程边界；不把 QEMU 链进主进程；后续 sandbox |
| CLI 参数注入 | argv 数组直传；不拼 shell |
| token 泄漏 | 不进 argv/日志；文件 0600；可轮换 |
| 远程滥用 | v1 不监听公网；C3 显式 opt-in + TLS/token |
| 截图/输入越权 | 只操作本机 session；无全局设备发现 |

---

## 14. 刀序 C0–C4

| 刀 | 交付 | 验收 |
| --- | --- | --- |
| **C0（已落）** | `berry screenshot` attach（VNC 多客户端）；PNG deflate；`input` VNC；macOS `.app`；480×1000 干净桌面 | 运行中 shell + CLI 截图成功；3.8KB PNG；11 host 单测 |
| **C1a（已落）** | shell-hosted control.sock + QMP broker；`device list/status/stop`；`power off/reboot`；`logs` | shell 运行时 CLI 重启/关机/查状态/看日志；QMP 不再冲突 |
| **C1b（已落）** | headless `device start`（host 角色，`--foreground`）、`device restart`；`screenshot` 走 host（VNC 兜底） | 无窗口起会话；CLI 截图/重启/关机全通；host 标签 `host` |
| **C1c（已落）** | `shell --attach <id>`：VNC 帧/输入 + control 动作；关窗 detach，host 继续 | headless host 可开窗；关窗不杀 session；`device status` 仍 running |
| **C1d（已落 state）** | `state.json` 持久化（starting/running/rebooting/stopping/stopped/failed）；list/status 缺 control 时读 state，running→stale；stop 幂等 | 杀 host 后 list/status 显示 stale；正常 stop 显示 stopped |
| **C1e（已落清理）** | meta 记 qemu_pid；`device cleanup` 杀 orphan QEMU + 删 stale run；running 拒绝 | 杀 host 后 cleanup 移除 QEMU/run |
| **C1f（已落 TCP）** | 非 Unix TCP+token control；Unix 默认 socket，`BERRY_CONTROL_TCP=1` 测试；token 0600 | macOS TCP 模式 status/logs/power 全通；Windows 走同路径 |
| **C1g（部分已落）** | host 自动重启已落（同 run 原地重启）；剩 Windows 实机验证/打包 | `--restart-on-crash` + kill QEMU 后同 run 恢复 |
| **C2a（已落）** | 原生快捷栏 4 键（SHOT/BOOT/OFF/FULL）；截图/重启/关机/全屏；鼠标+触摸 | 窗口内可点；截图落盘；重启/关机走 control/QMP；全屏切换 |
| **C2b（已落）** | `OFF` 二次确认；`berry shortcut`；`display list/set`（next-profile）；`events` JSONL | 关机需两次点击；CLI 可触发截图/重启/关机；事件可读 |
| **C2c（已落 CLI）** | `display set <profile> --restart`：停旧 host、起新 profile host（新 run） | phone→phone16 实测 480×854；新 run 可控 |
| **C2d（已落 host）** | `RES` 按钮 + profile 菜单（5 项）；host 窗口内停旧/起新；`RES` 高亮当前 profile | 菜单命中/绘制测试；选 phone16 窗口继续 |
| **C2e（已落 attach）** | attach 窗口 RES → `restart_profile` → 新 run VNC 重连，自动重附着 | attach 窗口不重启，host 换 480×854 后窗口继续 |
| **C2f（已落交互）** | 按钮 hover 高亮 + tooltip；菜单当前项高亮；OFF 二次确认 | 悬停可见提示；15 单测绿 |
| **C2g（已落）** | 先起新 host 再停旧（失败旧 host 不动）；旧 run 按 keep 清理；tooltip 保持 | 新 host 失败不伤旧；keep=false 旧 run 删除 |
| **C2h（已落）** | attach 新 host 失败保留旧 host；VNC 重连失败按旧 profile 回滚重连；stale 提示 cleanup | 失败不丢旧会话；rollback 可恢复 |
| **C2i（下一刀）** | 快捷键提示扩展；Windows 实机验证 | 交互/平台收口 |
| **C3（部分已落）** | `record`/`play`/`stats` 已落；`--wait`、时间轴回放、多设备未落 | 一条脚本完成「起→看→点→看→重启→关」；回放可复现 |
| **C4** | Runtime Pack；远程（显式）；录屏；插件后端 | 无 QEMU 机器可一键装 pack；远端 TLS/token；录屏可回放 |

**规则**：C1 之前 CLI 不直连 QMP；C2 之前 GUI 不直接调 QMP；所有动作先有 ControlClient
方法，再有 CLI/GUI 两个入口。

---

## 15. 现状 → 目标映射

| 现状 | 目标 | 差距/动作 |
| --- | --- | --- |
| `shot` 起临时设备 | `shot` 保留；`screenshot` attach | **screenshot 已落**（VNC） |
| `shell` 自起 + 持 QMP | host 持 QMP + control.sock | **C1a 已落**：Device 内嵌 ControlServer/QMP broker；C1b 加 headless host；C1c 加 attach |
| `input` VNC attach | control `input` op | C1 路由；VNC 保留兜底 |
| 无 `list/status/stop` | RunStore + control | **C1a/C1b 已落**（shell/headless） |
| 无 `power/display` | QMP broker + profile 重启 | **power C1a 已落**；display set --restart C2c 已落；RES C2d |
| 只有键盘快捷键 | 原生快捷栏 + action 表 | **C2a 已落**（4 键）；**C2b 已落**（确认/shortcut/events/display 记录）；C2c 补 RES/应用 |
| 无事件流 | `events.jsonl` + 订阅 | C1/C2 |
| macOS `.app` | 三平台包 | Windows/Linux 后续 |

---

## 16. 风险与 kill criteria

| 风险 | 证据 | 降级 |
| --- | --- | --- |
| QMP broker 复杂/时序 bug | C1 集成测试 | 先单请求串行 + 超时；不做并发 op |
| QEMU 事件/退出竞态 | 重启/关机 100 次 | 状态机保守；`E_BUSY` 明确 |
| Windows socket/token | 实机 | named pipe 或 loopback；只支持单用户 |
| 快捷栏拖慢窗口/缩放 | C2 帧时 | 独立 winit 区域/叠加层；不做复杂动画 |
| `display` 热切换不可行 | C2 spike | v1 只重启应用；不承诺热切 |
| 范围膨胀成模拟器 | 需求评审 | 非目标清单直接拒 |
| AI 误触关机 | 用户投诉 | 默认确认/`--yes`；事件审计 |

---

## 17. 待决/默认选择

| 问题 | 默认 | 备注 |
| --- | --- | --- |
| 单二进制 vs 独立 `berryd` | **单二进制隐藏角色** | 分发简单；C1 可拆 |
| NDJSON vs HTTP | **NDJSON** | 流式/无 HTTP 依赖；C4 可加 HTTP |
| 快捷栏 native vs guest UI | **native** | 客机卡死仍可用；不占分辨率 |
| display v1 热切 vs 重启 | **重启应用** | 诚实返回 `requires_restart` |
| 远程 v1 | **不做** | C4 显式 opt-in |
| `shot` 是否废弃 | **保留** | CI 一次性设备有独立价值 |

---

## 18. 参考

- Android Emulator console：<https://developer.android.com/studio/run/emulator-console>
- adb（`exec-out screencap`）：<https://developer.android.com/tools/adb>
- `xcrun simctl`：<https://developer.apple.com/documentation/xcode/simctl>
- QEMU QMP：<https://www.qemu.org/docs/master/interop/qmp-spec.html>
- Docker Engine API：<https://docs.docker.com/engine/api/>
- Playwright CLI：<https://playwright.dev/docs/test-cli>
- Maestro：<https://maestro.mobile.dev/>

> 说明：以上为公开官方文档/产品形态对标；本项目不复制其协议，只借用
> 「会话与命令分离 / 设备 id + verb / JSON 控制面 / client-host 契约 / CLI-first 测试」
> 这些结构结论。
