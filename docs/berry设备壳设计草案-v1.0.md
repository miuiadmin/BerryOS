# 开发者设备壳（berry shell）设计草案

> 版本：**1.0**（v1.0）｜2026-11-06

> 【状态】草案 · 2026-11-03 立稿；P0/P1/P2 已落地；2026-11-04 **平台化重规划**：CLI/控制面/快捷栏/刀序独立成文 → `Berry设备平台-CLI-first设计-v1.0.md`；本文降为显示/帧面/QEMU/客机侧子系统。
> 【定位】宿主侧设备壳**子系统**：显示/帧面/QEMU/客机侧；平台/CLI/控制面/快捷栏见
> `Berry设备平台-CLI-first设计-v1.0.md`（2026-11-04 起为单一事实源）。
> 【单一事实源】本文 = 设备壳架构/协议/刀序；ADR #62 = 一句话裁决；GUI 规划 = 上游需求。
> 【关系】补 `run-efi` 的 `-nographic` 窗口缺口；承接 GUI-9/SDK-1 的 screenshot/inject 宿主面；
> 客机 GUI 服务边界不动（guifbd/gui-wmd/launcher 零重构，v1 只加 ramfb 模式设置与 commit）。
> 【证据】本机 QEMU 11.0.3 实测：`-display help` 仅 none/curses/cocoa/dbus；`-vnc` 可抓 800×600
> 帧；QMP `screendump` 可用；RFB 3.8/None/Raw 全链通。仓库现状：`run-efi` = `-nographic`；
> `FBW ok w=800 h=600`；GUI-8 侦察已把「目验载体」记为 gap。

## §0 一句话结论

**`berry` = 宿主单二进制；窗口自持；QEMU 只是可换引擎；VNC 只是 v0 传输；客机像素是唯一真值；
ramfb commit 是 v1 的桥；virtio-gpu 是终态。**

这不是「给 QEMU 套一层皮」，而是把「设备」抽象成三个可替换面：引擎（QEMU/未来
Virtualization.framework/真机）、
控制面（QMP/未来自家协议）、数据面（VNC/未来共享内存）。窗口、输入、截图、注入、回放
只依赖这三面的**稳定契约**，不依赖任何一家实现。

### 0.1 平台方案（单一事实源）

产品形态（二进制 vs 平台）、CLI 命令面、session host/control.sock/QMP broker、窗口快捷栏、
C0–C4 刀序 → **`Berry设备平台-CLI-first设计-v1.0.md`**。本文只保留设备壳的显示/帧面/QEMU/客机侧
细节；§6/§8 的旧命令面以平台文档为准。

## §1 问题与现状证据

用户要的是：Windows/macOS/Linux 双击 → 手机比例窗口 → 启动设备 → 显示 GUI → 能点能滑。
今天仓库里缺的不是 GUI 本身，而是**宿主侧的设备壳**：

| 现状 | 证据 | 后果 |
| --- | --- | --- |
| `run-efi` 是 `-nographic` | justfile :808 | GUI 只画进显存，宿主人眼看不到 |
| 固件 GOP 固定 800×600 | `FBW ok w=800 h=600`；AAVMF RamfbDxe 默认 | 4:3，不是手机比例 |
| QEMU 窗口后端三平台不一致 | 本机仅 cocoa；cocoa 无 zoom-to-fit；Windows 常见 SDL | 无法做统一缩放/边框/注入 |
| QEMU 窗口按客机分辨率 1:1 开 | 1080×2400 直接超出屏幕 | 不能靠「把窗口拉大」解决 |
| 无宿主截图/注入入口 | GUI-8 侦察「目验载体 gap」 | GUI 无法进 CI，AI 无法自测 |

**关键拆分**：手机观感 = 两个独立问题，不能混为一谈。

1. **宿主窗口**：比例、缩放、黑边、边框、输入映射、截图、录屏。
2. **客机显示模式**：分辨率、刷新、像素格式、帧提交。

只做 1 = 手机形状窗口里塞 4:3 画面；只做 2 = 一个巨大的 1080×2400 QEMU 窗。两件必须都做，
但**可以分两刀**：v1 先让客机输出 480×1000（见 §7），宿主窗口永远按「等比缩放」显示。

## §2 目标与非目标

### 2.1 目标（按优先级）

| # | 目标 | 验收判据 |
| --- | --- | --- |
| G1 | 三平台双击可用 | `.app` / `.exe` / AppImage 打开即窗口 |
| G2 | 手机比例窗口 | 默认 19:9 级，等比缩放，无 QEMU 菜单栏 |
| G3 | 设备可启动 | EFI 镜像 → BerryBoot → 内核 → GUI 桌面 |
| G4 | 输入可用 | 鼠标/触摸点击、拖动；键盘可注入 |
| G5 | 截图/注入可编程 | `berry shot` / `berry input` 一条命令，`--json` |
| G6 | 无窗口 CI | `berry shot --ephemeral` 在 TCG 下可重复 |
| G7 | 传输可换 | 窗口代码不感知 VNC/共享内存 |
| G8 | 第三方可安装 | Runtime Pack + `berry doctor`，不要求源码树 |

### 2.2 非目标（v1 明确不做）

- 不做 Android 模拟器全家桶：传感器、摄像头、GPS、电话、多开群控都不在盒内。
- 不做 GPU 虚拟化：v1 软光栅，virtio-gpu 2D 是 v2，Vulkan 更后。
- 不做远程设备/云真机：P4 再议；v1 只绑本机 loopback。
- 不做常驻设备管理器 daemon：一窗口一设备进程，失败隔离优先。
- 不做真假刘海/侧键/听筒：假硬件 chrome 只会骗自己，客机画自己的状态栏。
- 不做 QEMU 源码 fork：引擎保持上游二进制，协议面自己定。
- 不做 Electron/Tauri/WebView：第二运行时 + 第二安全面，收益不成立。
- 不改默认 `run-efi`/smoke 的 800×600：手机模式是**运行时 profile**，CI 零漂移。

## §3 不可协商不变量（I1–I10）

这十条是后续所有实现的评审门；违反任一条 = 架构回退，不是风格问题。

| # | 不变量 | 为什么 |
| --- | --- | --- |
| I1 | **一进程一设备**：一个 `berry shell` 只拥有一个 QEMU 子进程与一个 run dir | 故障隔离；无隐藏全局状态；崩溃即整窗死，可归因 |
| I2 | **控制面/数据面分离**：QMP 管生命周期，ScreenSource 管像素与输入 | 任一可换；截图/复位不被帧流阻塞 |
| I3 | **传输可换，窗口不感知**：窗口只认 `Caps/Frame/InputEvent` | VNC 是启动器不是架构；换 shm 不改 UI |
| I4 | **客机像素唯一真值**：宿主只做缩放/边框/光标叠层，不合成客机 UI | 截图/黄金图/像素对拍无双真相源 |
| I5 | **缩放只发生在最后一跳**：客机渲染 → 一次缩放 → 屏幕，禁止中间反复重采样 | 清晰度与性能；缩放链可测 |
| I6 | **输入坐标单源变换**：一个纯函数把窗口坐标映射到客机坐标，单测钉死 | 黑边/缩放/旋转/DPI 只改一处 |
| I7 | **默认零网络暴露**：只绑 loopback 或 Unix socket；无 `--listen` 不开端口 | 开发机不是攻击面 |
| I8 | **许可边界 = 进程边界**：QEMU 永不链接，只 spawn；GPL 包与主仓分离 | Apache-2.0 主仓干净；故障也隔离 |
| I9 | **可复现优先**：manifest 哈希 + 固定 QEMU 参数 + TCG 确定性通道 | CI/AI 同输入同输出 |
| I10 | **失败可归因**：结构化日志 + 串口尾 + 退出码 + 最后 N 帧元数据 | 用户/CI 能报「哪一步死」 |

## §4 总体架构

### 4.1 进程与分层

```
┌────────────────────────────── berry（宿主，Rust） ──────────────────────────────┐
│  cli ── DeviceSession ──┬── ControlPlane ── QMP ───────────────┐                │
│                         ├── ScreenSource ── VncSource ─────────┼── QEMU child   │
│                         ├── Window/Input（winit + softbuffer）  │   -display none │
│                         └── Trace/Shot/Stats                    │                │
└─────────────────────────────────┬───────────────────────────────┴────────────────┘
                                  │ QMP（生命周期/复位/截图）
                                  │ VNC（帧/输入）或未来的 shm ring
                       ┌──────────┴──────────┐
                       │ QEMU（引擎，GPLv2） │
                       └──────────┬──────────┘
                                  │ 客机
              BerryBoot → kernel → guifbd / gui-inputd / gui-wmd / launcher → .bap
```

### 4.2 宿主 crate 分层

| 模块 | 职责 | 不做什么 |
| --- | --- | --- |
| `cli` | 参数、退出码、JSON 输出 | 不拼 QEMU 命令行字符串 |
| `device` | 一设备的生命周期、run dir、子进程 | 不懂像素 |
| `qemu` | argv 构建、accel 探测、overlay/vars | 不解析帧 |
| `qmp` | QMP 客户端、事件、命令白名单 | 不碰 UI |
| `display` | `ScreenSource` trait、Frame pool、缩放 | 不碰 QEMU 进程 |
| `display::vnc` | RFB 3.8 子集、输入编码 | 不决定窗口大小 |
| `window` | winit 窗口、软呈现、缩放模式 | 不做业务 |
| `input` | 窗口事件 → `InputEvent`、坐标变换 | 不直接发 QMP |
| `control` | 本机 attach 服务器（`berry shot/input`） | 不暴露网络 |
| `trace` | JSONL 事件、回放 | 不写视频（v1） |
| `shot` | PNG/PPM 写出、原始帧/合成帧 | 不做图像编辑 |

### 4.3 核心契约（Rust 形态，语义冻结）

```rust
pub trait ScreenSource: Send {
    fn caps(&self) -> Caps;
    fn next_frame(&mut self, timeout: Duration) -> Result<Frame, SourceError>;
    fn send_input(&mut self, ev: InputEvent) -> Result<(), SourceError>;
    fn close(&mut self);
}

pub struct Caps {
    pub version: u16,        // 传输能力版本域；不匹配 = loud 拒
    pub max_w: u32,
    pub max_h: u32,
    pub formats: PixelFormats,
    pub damage: bool,         // 是否携带精确脏区
    pub multi_touch: u8,      // 0 = 单指针
    pub cursor: bool,         // 是否自带光标叠层
    pub kind: TransportKind,  // Vnc / Shm / Replay
}

pub struct Frame {
    pub seq: u64,
    pub w: u32, pub h: u32, pub stride: u32,
    pub format: PixelFormat,
    pub damage: DamageSet,    // K≤8 + FULL，沿用 berry-guiproto 语义
    pub captured_ns: u64,     // 宿主单调钟
    pub guest_ns: Option<u64>,// 客机提交时戳（shm 才有）
}
```

**输入事件与客机 `berry-guiproto` 的 `PointerEvent` 同构**（pointer_id/kind/x/y/pressure/时间戳），
这样「窗口输入」「脚本注入」「录屏回放」三种来源走同一条路径，不产生第二套语义。

## §5 数据面：帧从客机到窗口

### 5.1 v0 传输：VNC（RFB 3.8 子集）

**为什么是 VNC**：QEMU 三平台自带；反向输入通道现成；Unix socket/loopback 都行；能 headless
抓帧；本机已实测 RFB 3.8 → None → Raw 拿到 800×600 帧。**它不是终态**，只是 v0 传输。

实现子集（只做这些，别扩）：

| 环节 | 选择 | 备注 |
| --- | --- | --- |
| 版本 | RFB 003.008 | QEMU 实测 |
| 安全 | Unix socket = None；TCP = VNC auth 随机口令 | 口令经 QMP HMP 设置，**不进 argv** |
| 像素格式 | 服务端默认 32bpp/depth24/true-color（实测 shifts 16,8,0） | 只在必要时转换 |
| 编码 | Raw；**DesktopSize 必须处理**（客机改模式 → 重分配帧缓冲）；Cursor 伪编码按长度跳过 | 不做 Tight/ZRLE/Hextile |
| 更新 | 一次只挂一个 `FramebufferUpdateRequest(incremental=1)` | 处理完再请求，防积压 |
| 背压 | 帧池 + 「最新帧胜出」槽；UI 慢就丢旧帧 | 不阻塞 QEMU |
| 断线 | 失败即报错，不静默重连 | 状态可归因 |

**能力真相**：VNC 只有单指针、通常无精确 damage。窗口不得宣称「多点/捏合」；多点留 shm。
`Caps.damage = false`，UI 不显示「脏区优化」类指标。**分辨率会中途变**（v0 800×600 → v1
480×1000）：SetEncodings 必须带 DesktopSize，收到伪矩形就重分配帧缓冲；不能假设首帧尺寸恒定。

### 5.2 带宽算术（决定默认渲染分辨率）

| 客机渲染 | 每帧字节 | 60fps 全屏 | 结论 |
| --- | --- | --- | --- |
| 480×854（9:16） | 1.64 MB | 98 MB/s | 安全 |
| **480×1000（18.75:9）** | **1.92 MB** | **115 MB/s** | **默认** |
| 540×1200（9:20） | 2.59 MB | 155 MB/s | P2，需连续 fb |
| 800×600（4:3） | 1.92 MB | 115 MB/s | v0 兼容档 |
| 1080×2400 | 10.37 MB | 622 MB/s | 不可行 |

VNC 在 ramfb 下**可能只能整帧更新**（客机直接写显存，QEMU 看不见脏区）。所以默认渲染
必须小；`540×1200` 是 P2 的「更大但还可活」档，`1080×2400` 永不做 VNC 全帧。

### 5.3 v2 传输：共享内存环

当 `guifbd` 能给出 damage 时，换 shm：

```
[64B header][slot0][slot1][slot2]    header: magic/version/seq/ack/w/h/stride/format/damage
[input ring: 64B header + N * 32B PointerEvent]
```

- 客机 `guifbd` 写 slot + `commit(seq)`；宿主 mmap 同一文件/共享页，取最新 seq。
- `damage` 直接来自 `berry-gfx` 的脏区，不再整帧。
- 换传输不改 `ScreenSource`；`Caps.damage = true`、`multi_touch = N`。
- 与 virtio-gpu 的关系：virtio-gpu 负责「客机→宿主资源」；shm ring 是宿主侧呈现协议；
  两者可组合，也可先用 `ivshmem` 直通帧。**协议先定，实现后补**（ADR #50 同款纪律）。

### 5.4 传输能力协商

`Caps.version` 是传输版本域；宿主只接受 `1..=MAX`。新能力（damage/多点/光标/远端）
一律 add-only + 能力位；**禁止**让窗口代码出现 `if vnc { ... }` 分支。

## §6 控制面（子系统视角）

控制面已提升到平台文档 §5/§7：session host 持有 QMP 并做 broker，CLI/GUI 经
`control.sock` attach；CLI 永不直连 QMP。本节只保留设备壳关心的引擎事实：

- QMP 只绑 loopback/Unix socket；run dir 0700，socket 0600。
- QMP 白名单：`qmp_capabilities`、`query-status`、`stop/cont`、`system_reset`、
  `system_powerdown`、`quit`、`screendump`、`send-key`、`input-send-event`、事件。
- 生命周期/错误码/op 表见平台文档 §6/§7；本文不再重复。
- 当前落点：`berry screenshot` 已用 VNC 多客户端 attach；QMP broker/control.sock C1a 已落
  （shell-hosted）；C1b headless `device start`/restart/screenshot 走 host、C1c `shell --attach` 均已落；C1d 补 Windows/崩溃恢复。

## §7 客机侧：三档演进

### 7.1 v0：零客机改动（800×600 letterbox）

`berry shell` 用现有 EFI 镜像启动，QEMU `-display none -vnc`，宿主把 800×600 等比放进
19:9 窗口。**今天就能有窗口**，只是上下黑边、画面 4:3。这是 P1 的交付，不是终态。

窗口必须诚实显示「800×600 · 4:3 · letterbox」，不能拉伸变形。

### 7.2 v1：ramfb 模式设置 + 每帧 commit（默认手机档）

**目标**：不改默认 CI，只让 `berry shell --profile phone` 触发 480×1000。

1. `berry` 传 `-fw_cfg name=opt/berry/display,string=480x1000@60`。
2. 内核启动读取该字符串，校验 `render_w * render_h * 4 <= gop_fb_bytes`。
3. 复用固件 GOP 缓冲（不新分配），写 `etc/ramfb` 的 28B 配置：
   `addr = gop_fb_pa`、`width=480`、`height=1000`、`stride=1920`、`fourcc=XRGB8888`。
4. 更新 handoff 的 `gop_info`（w/h/stride），guifbd 按新几何映射，`FBW ok w=480 h=1000`。
5. guifbd 每次 present 后**重写同一份 28B 配置** = `commit`，QEMU 替换 surface 并通知
   VNC，宿主才看到新帧。

**为什么 480×1000**：`480×1000×4 = 1,920,000 = 800×600×4`，恰好用满固件 GOP 缓冲，
零新分配、零连续内存风险。这是 v1 最优雅的一步：**不碰分配器，只换几何与提交**。
`480×854`（9:16）同样安全；`540×1200` 需要 2.59MB，留给 P2。

**关键假设（S1 spike 必测）**：ramfb 只在 fw_cfg 配置写入时替换 surface；客机直接写显存
不会自动通知 QEMU。若假设成立，commit 是实时帧的唯一通道；若不成立，commit 仍是无害的
帧边界（未来 shm/virtio-gpu 也需要）。**无论结论如何，HDI 的 commit 语义都必须存在。**

**commit 的落点**：v1 让 guifbd 直接持 `fw_cfg` MMIO 窗 + 一个固定的 28B DMA 页
（PA 经 env 传值），present 后重写配置。**硬约束**：guifbd env 8/8 恰满（ADR #61）——
新增 fw_cfg 窗 = 第 9 插，必须抬 `CHAN_HND_MAX` 8→9（DMA 页若也走 handle 则 8→10）。
这是 v1 唯一的客机内核契约变更，必须单列评审；不改就会在 boot 装配处 loud 失败。
若拒绝抬帽，备选：① 内核加 `display_commit` syscall（add-only，一个号）；② 拆 `disp0`
服务（自己的 env，guifbd 用现有通道发 commit）；③ 内核 boot 显示驱动代提交。

**能力边界**：fw_cfg MMIO 窗是 capability；DMA 页 PA 是固定值，只有 guifbd 知道。
如果评审认为 PA 传值不可接受，备选是 `vmo_info` 增 `phys` 字段（add-only）或
`disp0` 服务；三条都列在 §16 待裁。

**不改默认**：没有 `opt/berry/display` 时，内核保持 800×600；`run-efi`/smoke 零漂移。
手机档只由 `berry shell` 显式传参。

**P2 实装选型（2026-11-03，取代本节原「每帧 commit」路径）**：ramfb 只在 boot
时用 fw_cfg DMA 写一次 `etc/ramfb`（切换 scanout）；**不做每帧 commit**。宿主用
QEMU `memory-backend-file,share=on` 把客机 RAM 映射成文件，`RamSource` 从
`FB ok … fb=0x…` 解析几何后直接读帧——这是真正的共享内存数据面，不依赖 VNC
更新语义，也不需要 `CHAN_HND_MAX` 抬帽/`disp0`。VNC 退为**输入专用**多客户端
通道（`VncInput`，不请求帧）+ 无共享 RAM 环境的回退。

### 7.3 v2：virtio-gpu 2D（终态）

- `-device virtio-gpu-pci,edid=on,xres=480,yres=1000`（或 profile 值）。
- 客机走 `RESOURCE_CREATE_2D` / `ATTACH_BACKING` / `TRANSFER_TO_HOST_2D` /
  `RESOURCE_FLUSH`；flush 天然带脏区，不再整帧。
- 宿主可用 `Caps.damage=true`，`ScreenSource` 换 shm，窗口零改动。
- 120Hz、tile、GPU 后端都在这条线上；ramfb 只留作「无 virtio 的降级档」。

### 7.4 输入链

- 现有 `gui-inputd` 已认 virtio-input；EFI 路加 `-device virtio-tablet-device`。
- 宿主窗口坐标 → 客机 `PointerEvent`：`window_to_guest()` 纯函数，单测覆盖
  letterbox/缩放/旋转/DPI。
- VNC 的 `PointerEvent` 是绝对坐标；QEMU 有 tablet 时直接映射到客机。
- 键盘：RFB `KeyEvent`（keysym）→ 客机；US 布局起步，IME 留后。
- 多点：VNC 不支持；`Caps.multi_touch=0`，UI 不提供捏合；shm 后补。

### 7.5 客机改动清单（v1）

| 件 | 改动 | 风险 |
| --- | --- | --- |
| boot handoff | 增 `fb_bytes`（或 `stride*h`）、可选 display mode | 低 |
| 内核 fw_cfg | 增 DMA 写路径 + 读 `opt/berry/display` + 模式设置 | 中 |
| 内核 handoff | 更新 `gop_info` 为新几何 | 低 |
| guifbd | 几何从 info 读；去掉 800×600 假设；present 后 commit | 中高 |
| guifbd env | `CHAN_HND_MAX` 8→9/10（fw_cfg 窗 + DMA 页） | 高（单列评审） |
| gui-inputd | EFI 路 tablet 认领 | 低（GUI-4 已有） |
| launcher/berry-ui | 零改动（只吃新几何） | 零 |

## §8 CLI/窗口 UX（子系统视角）

CLI 命令树、`shot` vs `screenshot`、原生快捷栏/键位、双击语义、AI/CI 契约 →
平台文档 §8/§9/§11。设备壳只保留：

- 窗口 = 客机帧区 + 原生快捷栏；C2a 已落 4 键（SHOT/BOOT/OFF/FULL，OFF 二次确认）；C2b 加 shortcut/events/display 记录；C2c 加 `display set --restart`（CLI）；C2d 加 RES 菜单/host 窗口内换 profile；C2e attach 窗口自动重附着；C2f hover/tooltip；C2g 新 host 先起/旧 run 清理；C1d state.json 崩溃可见；C1e device cleanup 杀 orphan/删 stale run；C1f TCP+token control（Windows 路径）；C2h attach 失败回滚。
- 双击/无 TTY = GUI；有 TTY = help；`shell` = `device start --gui` 简写。
- 快捷栏按钮一律映射到平台文档的 action 表；禁止 UI 直接调 QMP/QEMU。
- 打包细节见平台文档 §12 与本文件 §9。

## §9 构建、打包、分发

### 9.1 仓库位置与构建陷阱

```
tools/berry/
  Cargo.toml        # 独立 [workspace]，独立 Cargo.lock
  .cargo/config.toml
  src/...
  tests/...
  packaging/{macos,windows,linux}/
```

**陷阱**：根 `.cargo/config.toml` 把 `build.target` 锁成 `aarch64-unknown-none`，子目录会继承。
宿主工具必须显式 `cargo build --manifest-path tools/berry/Cargo.toml --target <host-triple>`；
`<host-triple>` 由 `rustc -vV` 解析。**禁止裸 `cargo build`**，否则会编到内核 target 上。
justfile 里包一层 `berry-build`，解析 triple 后调用；这是构建入口单一事实源。

### 9.2 依赖预算（host-only，不进客机/ABI）

| crate | 用途 | 许可 |
| --- | --- | --- |
| `winit` | 窗口/输入 | MIT/Apache-2.0 |
| `softbuffer` | CPU 帧呈现 | MIT/Apache-2.0 |
| `serde` + `serde_json` | QMP/profile/trace | MIT/Apache-2.0 |
| `png` | 截图 | MIT/Apache-2.0 |
| `miniz_oxide` | screenshot deflate | MIT/Zlib |
| `clap` | CLI | MIT/Apache-2.0 |

- 依赖只进 `tools/berry/Cargo.lock`；`deny.toml` 独立 allowlist；GPL/AGPL 零进入（ADR #37）。
- 不引 `wgpu`/`egui`：v1 只做「一个窗口 + 一张图」，softbuffer 足够；设备管理器面板 v2 再议。
- Linux 需要 X11/Wayland 运行库；AppImage 打包解决，源码开发者在 README 写清 `libxkbcommon` 等前置。

### 9.3 Runtime Pack 与 QEMU 边界

**主仓 Apache-2.0，QEMU GPLv2——只用进程边界，永不链接/不把 QEMU 放进主仓发行物。**

```
~/.berry/                       # macOS/Linux；Windows = %LOCALAPPDATA%\BerryOS
  runtime/<pack>/               # 可下载、可校验
    manifest.json               # 版本 + 每文件 SHA-256
    firmware/AAVMF_CODE.fd      # EDK2，BSD-2-Clause-Patent
    images/berryos.img
    images/kernel.elf
    images/rootfs.cpio
  profiles/phone.toml
  run/<id>/                     # 每次运行
```

- v1：QEMU 由用户安装（`brew install qemu` / 官方安装包 / `apt install qemu-system-arm`）；
  `berry doctor` 检测版本、路径、accel，给出可复制安装命令。
- v2：`berry pack install` 下载 Runtime Pack（**不含 QEMU**），或单独下载「QEMU Runtime Pack」
  并附 GPL 声明 + 源码获取方式；双击在干净机器上可用。
- 固件随 Pack 走，避免三平台路径漂移；`AVMF_CODE` 版本进 manifest。
- QEMU 版本策略：`>= 8.2`，CI 固定 11.x；doctor 报「未测版本」警告但不拒。

### 9.4 镜像与 overlay

- 基础镜像 `out/berryos.img` **只读**；每次运行用 `qemu-img create -f qcow2 -b base -F raw`
  生成 `run/<id>/disk.qcow2` 覆盖层。
- 好处：不污染仓库镜像；多设备并行；布局持久化落在覆盖层；`--fresh` 直接删覆盖层。
- `--ephemeral` = 临时覆盖层，退出即删；CI 默认。
- FAT/布局的跨 boot 持久化验收在覆盖层上做；基础镜像字节零漂移。

### 9.5 accel 与确定性

| 平台 | 交互默认 | 确定性通道 |
| --- | --- | --- |
| macOS arm64 | `hvf` + `-cpu host` | `tcg` + `cortex-a72` |
| Windows x86_64 | `tcg`（WHPX 实验） | `tcg` |
| Linux x86_64/arm64 | `kvm`（有 /dev/kvm） | `tcg` |

`--deterministic` 强制 TCG + 固定机型 + 固定内存；CI/黄金图只用这条。交互默认 auto，
doctor 打印实际选择；**不改 CI 的 TCG 基线**。

## §10 安全

| 面 | 威胁 | 对策 |
| --- | --- | --- |
| 网络 | 本机其他用户连 VNC/QMP | 默认 Unix socket 或 loopback；run dir 0700；TCP 用随机口令 |
| 秘密 | 口令出现在 `ps` | 口令经 QMP HMP 设置，不进 argv |
| 解析 | 恶意客机/损坏帧打爆宿主 | RFB 维度/矩形数/长度全上限；checked arithmetic；fuzz 语料 |
| 命令 | 路径注入 | argv 数组，永不 `sh -c`；路径 canonicalize |
| 文件 | 符号链接偷换 | run dir 0700；临时文件 `O_NOFOLLOW` |
| QMP | 任意命令 | 白名单；只绑 loopback；不暴露公网 |
| 包 | 篡改镜像 | manifest SHA-256；`berry pack verify` |
| .bap | 未签名应用 | v1 不做安装面；v2 走 manifest/签名门 |

RFB 硬上限：`w,h <= 4096`、`nrects <= 1024`、`bpp in {8,16,32}`、`x+w<=w` 全检查；
解析器不 panic、不无限分配。这是「客机像素不可信」的最小纪律。

## §11 可观测性与 AI 测试面

### 11.1 事件流（JSONL，带 schema 版本）

```json
{"v":1,"t":"run.start","run":"20261103-...","profile":"phone","qemu":"11.0.3"}
{"v":1,"t":"frame","seq":42,"w":480,"h":1000,"capture_us":1200,"present_us":800,"dropped":0}
{"v":1,"t":"input","kind":"tap","x":240,"y":500,"src":"window"}
{"v":1,"t":"marker","name":"FBW ok","line":"FBW ok w=480 h=1000"}
{"v":1,"t":"run.exit","code":0,"reason":"user"}
```

`--json` 时 stdout 只出事件；人读日志走 stderr。所有错误带 `code/message/hint` 三段，
AI 不需要解析散文。

### 11.2 指标

- `fps`、`frame_capture_us` p50/p95、`present_us` p50/p95、`dropped`、`bytes_in`。
- `input_to_present_us`（有客机时戳时）：GUI-9 的 `input_to_present` 宿主半边。
- `qemu_cpu_pct`、`host_cpu_pct`（可选，用系统接口；v1 可只报帧时）。
- `berry stats --json` 随时取；窗口悬浮层同源。

### 11.3 回放与黄金图

- `trace` 文件 = header（profile/manifest 哈希/QEMU 版本）+ 事件流 + 输入；
  可选关键帧 PNG。回放 = `ReplaySource` 重放帧 + 输入，不启 QEMU。
- 黄金图：`--raw` 取客机原始帧（480×1000），TCG + 固定 profile，逐字节 SHA-256；
  合成帧（缩放后）只做人工目验，不做黄金图。
- **时钟/动画**：状态栏时钟会让黄金图漂移；测试 profile 用 `time = "fixed"`
  （fw_cfg 传固定 epoch）或 `shot --region` 只比内容区。这是 v1 必须解决的一件小事。

### 11.4 CI 矩阵

| 平台 | 作业 | 判据 |
| --- | --- | --- |
| macOS arm64 | `berry doctor --json` + `berry shot --ephemeral` | 退出 0，PNG 非空，宽高匹配 |
| Ubuntu 24.04 | 同上 + `berry run --kernel --marker` | 串口标记恰一次 |
| Windows 2022 | `doctor` + `shot`（TCG） | 同上 |
| 三平台 | `cargo test`（RFB/QMP/transform/profile） | 全绿 |

CI 不建窗口；`shot --ephemeral` 内部 VNC/QMP 全 headless。GUI 冒烟留给人工 + 截图归档。

## §12 失败隔离与恢复

| 故障 | 行为 |
| --- | --- |
| QEMU 崩溃 | 窗口保留，显示 serial 尾 + qemu.log + 退出码；提供「重启」 |
| QEMU 挂死 | `query-status` 超时 → `quit` → SIGTERM → SIGKILL；标记 `hung` |
| 窗口崩溃 | 子进程随父退出（进程组/job object）；run dir 留档 |
| 端口冲突 | 先 bind :0 取空闲端口再传 QEMU；失败重试 3 次 |
| 陈旧 run dir | 启动时扫 `run/`，pid + 进程启动时刻双校验，防 PID 复用；`doctor --fix` 清 |
| 镜像被占用 | qcow2 overlay 每 run 独立，基础镜像只读；QEMU 文件锁兜底 |
| 多个实例 | 允许；`berry list` 列 run id/profile/pid；`berry stop <id>` |
| 磁盘满 | 启动前检查 run 目录可用空间；写失败 loud，不静默降级 |

**崩溃即整窗死**是 I1 的代价，也是可归因的来源；不做「窗口活着但设备半死」的模糊态。

## §13 验收与刀序

### P0：headless 设备壳（1–2 天）

- `berry doctor --json`：QEMU/固件/镜像/accel/版本。
- `berry run --kernel --marker "BerryOS boot" --timeout 30`。
- `berry shot --ephemeral --wait-marker "BERRYBOOT ok" --out shot.png`（QMP screendump）。
- `berry input key ret`（QMP）。
- 判据：三平台 CI 绿；PNG 800×600；退出码稳定。

### P1：窗口（3–5 天）

- `berry shell --image out/berryos.img`：winit 窗口 + VNC 帧 + letterbox + 缩放。
- 鼠标 → 触摸；键盘；`Cmd/Ctrl+S` 截图；`R` 复位。
- `.app`/`.exe`/AppImage 壳 + `doctor` 弹窗。
- 判据：三平台双击出窗；800×600 画面不变形；输入事件到达客机（GUI-4 标记）。

### P2：手机档（2–3 天，含 S1 spike）

- S1：ramfb commit 实时性、480×1000 尺寸、QEMU CPU/带宽。
- 内核 fw_cfg 写 + 模式设置；guifbd 几何/commit；EFI 路 tablet。
- `berry shell --profile phone` → 480×1000 实时画面；`shot --raw` 与 VNC 帧逐字节一致。
- 判据：TCG 下 ≥30fps（480×1000）；无撕裂黑屏；默认 smoke 零漂移。

### P3：传输升级（后续）

- shm ring / virtio-gpu 2D / damage；540×1200；120Hz；trace 回放；录屏。
- 判据：同场景 CPU 比 VNC 降 30%+；`Caps.damage=true` 下窗口零改动。

## §14 风险与 kill criteria

| 风险 | 证据需求 | kill/降级 |
| --- | --- | --- |
| VNC 整帧 480×1000 撑不住 30fps | P2 实测帧时/CPU | 降到 360×750 或提前 shm |
| ramfb commit 抖动/CPU 高 | S1 实测 QEMU CPU | 提前 virtio-gpu 或专用 QEMU 设备 |
| fw_cfg 写口/PA 授权不可行 | S1 实机写成功 | 改 `disp0` 服务或 `vmo_info.phys` add-only |
| 540×1200 连续 fb 分配失败 | P2 分配实测 | 480×1000 为 v1 终态，540 延后 |
| QEMU GPL 打包阻塞 | 法务/分发评审 | 只做外部安装，不做 Runtime Pack |
| macOS 公证/签名阻塞 | 打包实测 | 先发未签名 dev 包 + 文档，签名后补 |
| Windows 无 WHPX | 实机 | TCG 可用即可，交互慢接受 |
| Linux X11/Wayland 依赖 | AppImage 实测 | 只支持 AppImage，不支持裸二进制 |
| 客机 GUI 未就绪 | GUI-8/9 进度 | 窗口先显示 console/固件画面，GUI 随主线 |
| 范围膨胀成 Android 模拟器 | 需求评审 | 非目标清单直接拒 |

## §15 备选方案（为什么不是它们）

| 方案 | 优点 | 为什么拒 |
| --- | --- | --- |
| QEMU cocoa/gtk/sdl 直用 | 一天可用 | 三平台不一致；无缩放/边框/注入/截图；1080 窗超屏 |
| 浏览器 + noVNC | 零安装、可远程 | 第二运行时；非原生双击；延迟/画质；不是产品形态 |
| Electron/Tauri 壳 | UI 快 | 体积/内存/第二安全面；v1 不需要 |
| 链接 libqemu | 单进程 | GPL 链接；API 不稳；QEMU 崩溃拖死 UI |
| SPICE | 协议成熟 | 客户端重、Linux 中心、QEMU 配置复杂 |
| 直接 virtio-gpu 先行 | 终态正确 | 客机驱动未就位，窗口被阻塞数周 |
| 自定义 QEMU fork | 完全可控 | 维护上游补丁 = 无底洞；违背「引擎可换」 |
| 远程云真机 | 任何设备可看 | 网络/隐私/成本；v1 不需要 |

## §16 待裁与 ADR

### 16.1 本稿已拍板

| # | 决策 | 选择 | 状态 |
| --- | --- | --- | --- |
| D1 | 默认手机档 | 480×1000 渲染 / 1080×2250 面板（19:9 级） | 拍板 |
| D2 | 16:9 档 | 480×854 渲染 / 1080×1920 面板 | 拍板 |
| D3 | 20:9 档 | 540×1200，P2，需连续 fb | 拍板（延后） |
| D4 | 窗口技术 | winit + softbuffer | 拍板 |
| D5 | v0 传输 | VNC + QMP，loopback/Unix socket | 拍板 |
| D6 | 客机 v1 | ramfb 模式设置 + 每帧 commit | 拍板，S1 验证 |
| D7 | 默认 CI | 800×600 不变；手机档运行时触发 | 拍板 |
| D8 | QEMU 分发 | v1 外部安装；v2 独立 Runtime Pack | 拍板 |

### 16.2 仍需评审的三件

1. **（已被 P2 实装取代）commit 的落点与 guifbd env 帽**：P2 选「boot 一次
   mode set + 宿主共享 RAM 读帧」，不逐帧 commit、不抬 `CHAN_HND_MAX`、不拆 `disp0`；
   原四选（fw_cfg 直写/`display_commit` syscall/`disp0`/`vmo_info.phys`）保留为无
   `memory-backend-file` 环境的备选。
2. **默认窗口比例**：19:9 级（480×1000）还是 16:9（480×854）作为 `phone` 默认。
   倾向 19:9，16:9 留 `phone16`；用户可改 profile。
3. **QEMU 最低版本**：`>= 8.2` 是否够；CI 固定 11.x，doctor 对 8.2–10 只警告。

### 16.3 ADR #62（全文）

见 `docs/架构决策.md` #62：**宿主自持窗口 + 双平面 + 传输可换 + 客机 ramfb 先行**。
一句话：`berry` 单二进制；QEMU 只作引擎子进程；控制面 QMP、数据面 v0 VNC/v2 shm；
窗口/输入/截图不依赖 QEMU GUI 后端；客机 v1 用 fw_cfg ramfb 模式设置 + 每帧 commit
实现 480×1000（恰好用满 800×600 GOP 缓冲）；QEMU GPL 用进程边界隔离。

### 16.4 落地注记（2026-11-03，P0/P1/P2）

**已落**：`tools/berry/`（lib + bin，host 独立 workspace）——`doctor` / `run` / `shot` /
`shell` / `input` / `version`；QMP 控制面；RFB 3.8（帧面 + `VncInput` 输入面）；
PNG；winit/softbuffer 窗口；`just berry-build` / `just berry ...` / `just shell` /
`just berry-smoke`。macOS + QEMU 11.0.3 实测：`doctor` 绿、`run` 命中 `BerryOS boot`、
`shot --profile phone` 出 **480×1000 PNG**、`shell` 起停、`input tap/key` 经 VNC
到达客机（与 shell 并存）；11 条 host 单测全绿。

- **双击包（P2.5）**：macOS `.app` 已落——`tools/berry/packaging/macos/build.sh`
  产 `out/BerryOS.app`（`Contents/Resources` 自带 `berryos.img` + `AAVMF_CODE.fd`，
  ad-hoc 签名；QEMU 仍外部）。实测从 `/tmp` 无 TTY 启动无参 `berry`，资源解析
  480×1000、退出 0。Linux `.desktop`/Windows 草稿在 `packaging/`；Windows 的
  GUI 子系统 + `AttachConsole` 仍待补。

**P2 实装（本刀核心）**：

- HAL `fwcfg` 增 fw_cfg DMA 写（16B `FWCfgDmaAccess` 描述符 + 8B 门铃；`SELECT|WRITE`），
  内核 boot 读 `opt/berry/display`，一次性把 GOP 缓冲切 480×1000 并更新 `gop_info`；
  `DISP ok` + `FB ok 480x1000 …` 为状态谓词，失败保持 800×600 + `DISP fail`。
- 宿主 QEMU 加 `memory-backend-file,share=on`；`RamSource` 按 `FB ok` 的
  `fb_pa/fmt/stride` 读 RAM 镜，BGRX→RGB，60Hz 轮询 + 采样签名去重；**无每帧 commit**。
- `VncInput`（handshake 后不请求帧）作输入专用通道；QEMU VNC 多客户端，shell 的
  QMP 不再被 `berry input` 抢占。QMP 只留 reset/stop 控制。
- 默认 `phone`/`phone16`/`phone20`/`tablet` 内存 **256MiB**：handoff `dram_top` 动态化
  + `efi_ram_top` 修正后 `-m 256M` 全链绿（`MMAP ok`/ROOTFS/GUI）；`default` 128MiB。

**与设计稿的差异（如实记）**：

- §7.2 原「每帧 `etc/ramfb` commit」未采用；真实选型 = boot 一次 mode set +
  共享 RAM 读帧（见上）。`etc/ramfb` 逐帧 commit 与 `disp0`/抬帽仅保留为
  无 `memory-backend-file` 环境的备选，不再是 P2 阻塞项。
- `berry input` 已从 QMP 改为 `VncInput`（多客户端），`guest 480x1000` 实测；
  平台文档 §7 的 control.sock/QMP broker **C1a/C1b 已落**（shell/headless host，CLI 可
  power/status/logs/screenshot）；C1c 补 attach/Windows。
- `shot` 仍以 VNC 抓帧，QMP `screendump` 兜底未接；测试镜像的 GUI 是 console/测试面，
  launcher 桌面尚未在该镜像验证——宿主数据面已就位。
- `shell --timeout N` 测试/演示自动退出；P1 的 `snapshot=on` 覆盖层未升级为
  qcow2 持久层；PNG 已换 deflate（`miniz_oxide`，480×1000 约 3.8KB）；540×1200 仍被 1.92MB
  GOP 缓冲已支持；540×1200 在 mm::init 后用 buddy 4MB 连续块。

- **GUI 模式 console 静音**：fbinfo 扩 48B 第 6 域 `gui=1`（仅 `DISP ok` 时置位）；
  guifbd 在该模式仍记账 `console_full_n`/`present_seq`，但跳过 `put_str`——测试/日志
  只走串口，不再盖桌面。默认 800×600（无 display 项）零漂移；已用 GUI-8 干净镜像复验。
- **P2 × GUI-8 联调（预览，未并）**：`gui8-draft` 2e57502 + 本 P2 内核 + `gui=1` 静音，
  在 480×1000 出**干净桌面**：状态栏/页指示器/4 列 8 图标位，无控制台覆盖；图标仍是
  dash = GUI-8 资产在途面。产物 `out/berryos-desktop.img` / `out/berry-desktop-480x1000.png`
  （gitignored 预览件）与 `out/BerryOS-Desktop.app`。主分支 launcher 仍是 skeleton；
  GUI-8 合并后 `just image` 即得桌面。

**未落（下一刀）**：P3 shm ring/virtio-gpu/damage/120Hz/回放；540×1200 已落；
256MiB handoff 已完整可用（MMAP ok）；C1b headless/Windows control。P2 未动 `smoke.sh` 默认 800×600
与内核 ABI；`run-efi`/smoke 不带 `opt/berry/display` 时零漂移。

## 附录 A：RAMFBCfg（28B，大端）

```c
struct RAMFBCfg {
    uint64_t addr;     // 客机物理地址（GOP 缓冲基址）
    uint32_t fourcc;   // XRGB8888 = 'XR24'（0x34325258）
    uint32_t flags;    // 0
    uint32_t width;    // 480
    uint32_t height;   // 1000
    uint32_t stride;   // 480*4 = 1920
} __attribute__((packed));   // 28 bytes
```

- 写入项名：`etc/ramfb`；走 fw_cfg DMA 的 `WRITE` 控制位。
- 客机必须在写之前确保 `addr..addr+stride*height` 是已映射、未被复用的 RAM。
- `fourcc` 由 HDI 像素格式映射；首版只支持 XRGB8888/BGRX。

## 附录 B：QEMU argv（v1 手机档）

```bash
qemu-system-aarch64 \
  -machine virt -cpu cortex-a72 -m 256M \
  -display none \
  -vnc unix:/run/berry/vnc.sock \
  -qmp unix:/run/berry/qmp.sock,server=on,wait=off \
  -serial file:/run/berry/serial.log \
  -drive if=pflash,format=raw,readonly=on,file=runtime/firmware/AAVMF_CODE.fd \
  -drive if=pflash,format=raw,file=/run/berry/vars.fd \
  -drive if=none,file=/run/berry/disk.qcow2,format=qcow2,id=bb \
  -device qemu-xhci -device usb-storage,drive=bb \
  -device virtio-tablet-device \
  -device ramfb \
  -fw_cfg name=opt/berry/display,string=480x1000@60
```

- TCG 确定性档：`-accel tcg -cpu cortex-a72` + 固定 `-m`。
- HVF 交互档：`-accel hvf -cpu host`；不用于 CI。
- Windows 的 VNC/QMP 改 `tcp:127.0.0.1:<port>`；口令经 QMP HMP 设置。

## 附录 C：Profile

```toml
schema = 1
id = "phone"
panel = { w = 1080, h = 2250 }
render = { w = 480, h = 1000 }
refresh = 60
memory_mib = 256
accel = "auto"          # auto | tcg | hvf | whpx | kvm
input = "virtio-tablet"
display = "ramfb"
time = "host"           # host | fixed（黄金图用 fixed）
```

校验：`render.w * render.h * 4 <= gop_fb_bytes`；`refresh <= 60`（v1）；
`render` 宽高 4 对齐；未知键 loud 拒（与 manifest 同款纪律）。

## 附录 D：run dir

```
~/.berry/run/<id>/
  meta.json        # pid, start_time, profile, manifest hash, argv, qemu version
  qmp.sock         # 或 qmp.port + token
  vnc.sock         # 或 vnc.port
  control.json     # attach 端口 + token + pid
  serial.log
  qemu.log
  shell.log.jsonl
  disk.qcow2
  vars.fd
```

`meta.json` 的 `pid + start_time` 用于防 PID 复用的陈旧回收。

## 附录 E：退出码

| 码 | 含义 |
| --- | --- |
| 0 | 成功 |
| 2 | 用法/参数错误 |
| 3 | 环境缺失（QEMU/固件/镜像） |
| 4 | 启动超时 |
| 5 | 标记不匹配 |
| 6 | 设备崩溃/异常退出 |
| 7 | 协议错误（RFB/QMP/帧） |
| 8 | 用户取消/信号退出 |
| 9 | 内部错误（bug） |

## 附录 F：RFB 消息子集

| 方向 | 消息 | 处理 |
| --- | --- | --- |
| S→C | ProtocolVersion | 只接受 3.8 |
| S→C | SecurityTypes | None / VNC auth |
| S→C | ServerInit | 记录 w/h/pixel format/name |
| C→S | SetEncodings | Raw + DesktopSize + Cursor |
| C→S | FramebufferUpdateRequest | 一次一个，增量 |
| S→C | FramebufferUpdate | rects；Raw 拷贝；**DesktopSize 重设 w/h**；Cursor 跳过 |
| C→S | KeyEvent / PointerEvent | 键盘/绝对指针 |
| C→S | ClientCutText | v1 不发 |

## 附录 G：与既有文档的钩子

- `docs/GUI里程碑规划-2026-10-v1.0.md`：目验载体（本文承接 `run-efi` 窗口缺口）。
- `docs/GUI-8实施侦察-2026-11.md`：§3.2「目验载体 gap」、§3.5「UTM 手动点验」。
- `docs/开发体验.md`：L3 运行/安装阶梯、hvf/TCG 双模式。
- `docs/GUI-2段③消费侧首刀实施设计草案-v1.0.md`：HDI 三口 / present 契约。
- `docs/架构决策.md` #50/#53/#61：B 形态、低功耗、呈现/策略分离。
- `docs/应用开发指南.md`：SDK 安装、`berryctl` 后续 verb 面。
