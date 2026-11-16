# 02 号：virtio-gpu 2D 档——GPU 加速路径兑现设计（规划第二档提前）

> 调研日期：2026-11-13。服务对象：显示/模拟器体验框架级重设计（用户令：1080P 初始不妥协 / GPU 加速 Mac 先行 / 性能要开发者可用），姊妹篇 = 01 号显示链审计（ramfb 三层静态钉板，同窗并行未落盘，本文按任务书转述其结论）。上位文档：`docs/GUI里程碑规划-2026-10-v1.0.md`（后端兑现序列 :187 / S1 spike 挂起段 :13 / HDI 三口 :249）、`docs/架构决策.md` ADR #50/#51/#62、`docs/berry设备壳设计草案-v1.0.md`。
>
> 方法：本地源码逐行（`tools/berry/src/` 壳显示链、`user/bins/guifbd/`、`drivers/berry-virtio/`、`mm/src/va.rs`、`kernel/src/main.rs`）+ **本机实证**（宿主 = Apple M3 Pro / 18GB / hw.pagesize=16384 / QEMU 11.0.3 Homebrew——display 后端清单、设备清单、`-display cocoa,gl=on` 拒答、宿主带宽 dd 粗测，全部当场跑过）+ 联网核验（QEMU 官方文档×2、QEMU master `virtio-gpu.c` 源码、Linux uapi 头两枚、Linux DRM 驱动源码）。确定度标注：**[高]** = 一手源码/文档/本机实证；**[中]** = 可信二手或时效敏感待 spike；**[低]** = 单源推测。推断一律显式标「推断」。

---

## §0 结论速览

- **架构选型：案 β 全取**（壳自持窗口保留 + QEMU 显示导出走显示后端线协议 + 宿主 wgpu/Metal 呈现），传输两步走：**β-0 = 现有 VNC 传输在 virtio-gpu 下自动获得增量矩形**（改造近零）；**β-1 = `-display dbus,p2p=on` + Listener/Unix.Map 共享图 + UpdateMap 纯 damage**（零编码零拷贝 + 动态分辨率 + 输入反走 D-Bus）为终态。案 α 结构性出局（本机实证：cocoa 无 GL、无 virgl、无 gtk/sdl）；案 γ 远期观察位（自研栈无 Mesa + virgl aarch64 史坑）。见 §2。
- **慢根因定量支持任务书假设①**：1080P 现链每变化帧 = guest 软光栅写 8.3MB + 宿主 pread 全帧 8.3MB + 207 万像素标量格式转换 + CPU 软件缩放 + softbuffer CPU 上屏，静止帧也要全帧 pread（签名校验在读取之后）；宿主带宽余量实测 59.6GB/s——**瓶颈不是带宽，是无 damage、标量转换与 CPU 合成的结构性浪费**。换 virtio-gpu 2D 后宿主 CPU 面只乘 damage 面积系数。见 §1/§4。
- **guest 驱动量 ≈ 5.5–8.5 人日**（协议 crate + 驱动后端 + guifbd 接线），壳宿主半 ≈ 4.5–9 人日（β-0 小、β-1 大），S1 spike 0.5–1 天，全链 ≈ 10–17 人日。见 §3。
- **1080P@60 帧预算口径**：RGB32 = 497.7MB/s（任务书 249MB/s 对应 RGB565@60 或 RGB32@30；本仓裁决 D 取 RGB32 起步——`docs/GUI里程碑规划-2026-10-v1.0.md` §3-D 行），满帧最坏情形总线占用不足宿主实测带宽 1%。TCG vs HVF 差距在 guest 软光栅半，HVF+virtio-gpu+GPU 合成 = 模拟器加速档的目标形态。见 §4。
- **分辨率动态顺势解**：01 号钉的「换分辨率需重启」在 virtio-gpu 下自然消解——SET_SCANOUT 携任意 16px 起步的矩形（QEMU 源码执法），scanout 几何 = 资源几何，guest 起资源即换分辨率；config `events_read`（VIRTIO_GPU_EVENT_DISPLAY）+ CONFIG_GEN 中断给宿主→guest 反向通知；D-Bus `SetUIInfo` 给窗口→EDID 反向通道。三层静态变三层动态。见 §5。
- **刀序**：S1 spike 原挂「GUI-2 刀前」（规划 :13），GUI-2 段③ 已于 2026-11-02 收官（`docs/GUI-2段③消费侧首刀实施设计草案-v1.0.md` :5）——**S1 现在就是到期未跑状态**，本文将其升格为「三验合一」（设备可达 / 传输可达 / HVF 兼容），随后 B1-a/b/c/d 四小刀。kill 线沿用 GUI-10 §九 :117，另立降档线（D-Bus Map 在 macOS 不可用 → 降 β-0，非 kill）。见 §6。

## §1 现状钉板：帧链数据面解剖（慢在哪，逐环节带证据）

现行链（desktop 档，render=panel=1920×1080 零 letterbox——`tools/berry/src/profile.rs` :31-37，owner 2026-11-13 裁定「1080P 不做低清妥协」）：

```
berry-gfx soft（guest CPU 光栅，TCG/HVF）
  → guifbd SUBMIT blit-at-SUBMIT（damage 裁剪 CPU blit → FB_VA；user/bins/guifbd/src/main.rs :33-56）
  → ramfb = 客机 DRAM（AAVMF RamDxe AllocatePages 普通内存；guifbd main.rs :4-12）
  → QEMU `-device ramfb` 扫客机 DRAM（tools/berry/src/qemu.rs :245）
  → 宿主文件：`-object memory-backend-file,share=on`（qemu.rs :196-201）+ `-display none`（:147）
  → berry shell RamSource 事件驱动轮询 pread 全帧（每 AboutToWait 一拍，display/ram.rs :136-179 + window.rs :481-489）
  → 逐像素标量 BGRX→u32 转换（ram.rs :164-177）
  → CPU 软件缩放 + CPU 工具栏/菜单绘制（window.rs :502-533，scale::render + draw_toolbar）
  → softbuffer CPU 上屏（window.rs :1 「winit + softbuffer」）
```

逐环节账（1080P RGB32 = 1920×1080×4 = 8,294,400 B ≈ 8.29MB；算术一手，与规划 :151 的 800×600≈1.9MB/帧同法互证）[高]：

| 环节 | 每变化帧成本 | 证据 |
| --- | --- | --- |
| guest 软光栅 | 写 8.29MB（TCG 下另乘翻译税） | `user/berry-gfx` soft 后端 |
| 宿主收帧 | **全帧 pread 8.29MB，先读后验签名**——静止帧也全读 | `ram.rs :136-158`（read_at 全量在前，签名采样在后） |
| 格式转换 | 207.36 万像素逐点标量循环 | `ram.rs :164-177` |
| 缩放合成 | 全帧 CPU `scale::render` + CPU 工具栏 | `window.rs :515-530` |
| 上屏 | softbuffer CPU 写窗口后备缓冲 | `window.rs :531` |
| 轮询形态 | winit 事件驱动 `try_next_frame` 非阻塞轮询 | `window.rs :481-489`、`frame.rs :77-88` |

三项结构性结论：

1. **每变化帧 ≥4 次全帧级 CPU 遍历**（guest 写 + pread 读 + 转换写 + 缩放/上屏写），静止帧仍付 1 次全帧读。宿主 memcpy 实测 59.6GB/s（`dd if=/dev/zero of=/dev/null`，M3 Pro 粗测——含内核零填页路径，作量级锚非精测）[高·实测]，497.7MB/s 全帧流量仅占 0.83%——**慢不在带宽，在遍历次数与标量转换**；这也解释「壳 accel 面完整（HVF 可用）但用户实测仍慢」：HVF 只消灭 guest 半的翻译税，宿主四遍搬运与 CPU 合成分毫未动（任务书假设①成立，定量版）。
2. **damage 信息在 ramfb 边界蒸发**：guifbd 段③ 已按 B 形态三件事带 damage K≤8（main.rs :38-43），但 ramfb 是裸 DRAM 扫描，无提交边界——QEMU 与壳只能全帧轮询比对（壳侧 `Caps.damage=false` 自报，`ram.rs :129`；壳草案 :184-185 亦钉「VNC 只有单指针、通常无精确 damage」）。
3. **三层静态烧死分辨率**（01 号钉板，本文补充链上证据）：boot fw_cfg 一次注入 `opt/berry/display`（qemu.rs :246-249 → kernel/src/main.rs :212-216 消费）+ RamDxe 静态几何 + `RamSource::open` 以静态 FbInfo 定容开档（ram.rs :74-99）——换分辨率需重启。

## §2 架构选型：α / β / γ 三案裁决（Mac 先行约束）

guest 半三案共享（virtio-gpu 2D：资源/scanout/flush，见 §3），分歧在**宿主呈现走哪**：

| 维度 | 案 α：QEMU 自带窗口 | **案 β：显示导出 + 壳收帧（推荐）** | 案 γ：virgl 3D 全管线 |
| --- | --- | --- | --- |
| 窗口归属 | QEMU cocoa/gtk | **壳 winit 窗口（ADR #62 不动）** | 随 α/β |
| GPU 合成位置 | QEMU（virglrenderer） | **壳 wgpu/Metal 纹理合成** | guest Mesa→host virglrenderer |
| damage 到宿主 | 取决于前端 | **β-1 纯矩形 / β-0 增量矩形** | dmabuf/GL 路径 |
| 动态分辨率 | 前端窗口驱动 | **SET_SCANOUT + events 双向** | 同左 |
| 壳工具栏/截图/注入/--json CI | **全失**（ADR #62 已裁不可丢） | **全保** | 同左 |
| Mac 本机可达性 | **实证出局**：`-display help` 仅 none/curses/cocoa/dbus（无 gtk/sdl）；`-display cocoa,gl=on` 拒答「OpenGL support was not enabled in this build」；设备清单无 virtio-gpu-gl-device（virgl 未构建）[高·本机实测] | **实证在场**：`-display dbus` 后端在列 + `virtio-gpu-device`（mmio 形态）在列 [高·本机实测] | 同 α 双杀 + guest 需 Gallium 级栈（ADR #51 已裁自研不引）+ aarch64 史坑（QEMU #1141，GUI-03 :165 转录未复核）[中] |

**裁决：案 β。** α 的前提「GPU 合成在 QEMU/virglrenderer」在 Mac 构建上不存在（上表实证），且丢壳面违 ADR #62；γ 与自研栈路线冲突，留远期观察位（UTM/Mesa venus 支线同 GUI-03 :219 口径，不进产品路径）。β 的宿主链设计两步走：

- **β-0（先行兜底，改造近零）**：传输仍走 VNC，但设备换 `virtio-gpu-device` 后，QEMU console 收到的是 flush 矩形级 `qemu_console_update`（§4 引源码），VNC 增量更新协议天然按脏矩形发帧——现有 `vnc.rs`（Raw + DesktopSize + 输入，:2 注释）已按矩形收，仅需把矩形集透传到呈现层（`Frame` 加 damage 载荷——壳草案 §5.3 :156 已预留 `damage: DamageSet` 字段位）。输入继续 VNC loopback 不动。
- **β-1（终态）**：QEMU `-display dbus,p2p=on,addr=unix:<run_dir>/display.sock`（无需 dbus-daemon——RegisterListener 契约原文「a Unix socket FD, for peer-to-peer D-Bus communication」，p2p 直连 [高·官方文档]）。壳实现 `org.qemu.Display1.Listener`（+`Listener.Unix.Map`）收 **ScanoutMap（共享图 fd + 几何）/ UpdateMap（x,y,w,h 纯 damage 矩形）**；QEMU master 的 2D 资源已建为 shareable pixman（`virtio_gpu_resource_create_2d` 走 `qemu_pixman_image_new_shareable` [高·源码]）——共享图是设备原生能力非补丁。输入反走 `Mouse.SetAbsPosition`/`Keyboard`/`MultiTouch`（同一接口族 [高·官方文档]），VNC loopback 退役。macOS 无 memfd，QEMU 侧 qemu_memfd 走临时文件后备（M-share 源码级核实挂 S1，标 [中]；不可用则降 β-0，降档线非 kill）。
- **β 的 GPU 呈现半（「连 GPU 加速都用上」的宿主半）**：winit 窗口配 wgpu（macOS 原生 Metal 后端），`ScanoutMap` 的共享图 mmap 后按 `UpdateMap` 矩形 `write_texture`（GPU DMA 上传，CPU 零接触像素）→ 单 pass 采样缩放合成（letterbox/整数缩放/任意比例全变 GPU 采样）+ 工具栏层 GPU 合成；截图 = 纹理 readback（QMP screendump 退二线）；与 soft 后端的像素共识对拍按 ADR #51 双后端纪律保留。**Mac 上「GPU 直呈」的语义 = 共享图→MTLTexture 一次 DMA 上传 + Metal 合成**；Linux 上另有 ScanoutDMABUF 零拷贝直采位（`org.qemu.Display1.Listener.Unix.ScanoutDMABUF2` 接口在册 [高·官方文档]），是同构升级缝非新架构。
- **出局附注（vhost-user-gpu）**：壳进程自实现 GPU 设备（理论最优——渲染进壳、零 QEMU 显示栈）双杀出局：本机构建 vhost-user 设备**零命中**（`-device help | grep vhost` = 0，[高·本机实测]）+ QEMU 官方文档明言「vhost-user-gpu does not support MMIO」（PCI-only，BerryOS 是 virtio-mmio 路线，GUI-03 :169）[高]。Linux CI 远期可复活，观察位。

## §3 guest 半：virtio-gpu 2D 驱动量（协议面 + 落法 + 人日）

### 3.1 协议面（全部一手核验，BSD 许可头——协议洁净无碍）

- 命令集（Linux uapi `virtio_gpu.h` [高]）：2D 九命令 = GET_DISPLAY_INFO 0x0100 / RESOURCE_CREATE_2D / RESOURCE_UNREF / SET_SCANOUT / RESOURCE_FLUSH / TRANSFER_TO_HOST_2D / RESOURCE_ATTACH_BACKING / RESOURCE_DETACH_BACKING / GET_EDID（VIRTIO_GPU_F_EDID 特性位）；cursor 两命令（UPDATE_CURSOR/MOVE_CURSOR 0x0300，v1 可砍——鼠标在壳侧绘制+绝对坐标注入）。ctrl_hdr 24B；**格式直配**：VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM=2 = 现役 ramfb fmt=1 BGRX 同字节序（ram.rs :173），guest 半零像素转换。
- 设备身份：DEVICE_ID_GPU = 16（`virtio_ids.h` VIRTIO_ID_GPU [高]；berry-virtio `mmio` 表已有 INPUT=18/NET=1/BLK=2 先例，`lib.rs :230-237` 增一行）。**两队列**：controlq（qidx 0）+ cursorq（qidx 1）——QEMU `virtio_gpu_device_realize` 源码直取 `virtio_get_queue(vdev, 0/1)` [高·源码]；v1 只启用 controlq 亦合法（spec 允许不协商 cursor）[中·规范推断，S1 验]。
- 事件面：config 区 `events_read`/`events_clear`/`num_scanouts`（uapi `virtio_gpu_config` [高]）+ mmio CONFIG_GEN 翻转（berry-virtio `mmio::CONFIG_GEN` :223 已在表）——display 热插拔/EDID 变化的通知通道；ACK = 写 events_clear（QEMU `virtio_gpu_set_config` 源码语义 [高]）。
- 完成语义对 BerryOS 友好：**QEMU 2D 简单路径全命令即答**（`virtio_gpu_simple_process_cmd` 循环尾统一 `virtio_gpu_ctrl_response_nodata`，无 fence 挂起 [高·源码]）——used ring 收割即背压，与 guifbd credits≤2/release 语义直接对齐，比 ramfb 后端更简单。
- 参照系：Linux DRM 全驱动 18 文件约 194KB（含 virgl/venus/blob/prime 全家）；BerryOS 只取 2D 子集 + 参照 `virtgpu_plane.c` 的 per-rect「transfer+flush」同构流（见 3.2）。

### 3.2 落法（HDI 三口不动，换实现不换契约——ADR #50 原文）

- **新纯函数 crate `berry-virtio-gpu`**（协议编解码，`drivers/berry-virtio` 同款纪律：零分配零 unsafe、host 单测先行）：命令/响应结构体 + LE 编解码 + feature 位 + config/events 解析，估 0.6–1k 行。
- **guifbd 进程内后端模块**（不立独立服务——ADR #61 的 8/8 env 恰满教训：新守护 = 新插槽数抬帽连带内核刀；驱动即持证应用，guifbd 已持 mmio 授能链先例 main.rs :4-12）：初始化（mmio 扫描按 DEVICE_ID_GPU / 特性协商 VERSION_1(+EDID) / 建 controlq）→ GET_DISPLAY_INFO(/EDID) → RESOURCE_CREATE_2D(B8G8R8X8, W×H) → ATTACH_BACKING。
- **PREPARE/COMMIT 兑现形态**：hdi PREPARE 收 buffer VMO → `berry_vmo_get_phys`（`user/berry-sys/src/lib.rs` :240 已有）逐页出 mem_entry 表登记 backing；SUBMIT/COMMIT 的 damage K≤8 → 逐矩形 TRANSFER_TO_HOST_2D(offset,r) + RESOURCE_FLUSH(r)——与 Linux `virtgpu_plane.c` dumb bo 路径逐步同构（damage_merged → `virtio_gpu_update_dumb_bo`（per-rect transfer_to_host_2d）→ fb/尺寸变更时 `virtio_gpu_cmd_set_scanout` → `virtio_gpu_resource_flush` [高·源码]）。
- **已识别坑（估进人日）**：ATTACH_BACKING 的 mem_entry 表在请求 out 段——1080P 帧缓冲 4K 页 ≈ 507 页 × 16B ≈ 8.1KB，超 berry-virtio 单页 virtqueue 承诺（`lib.rs :14-17`「整 virtqueue 落一页」几何前提 + qsize=16 desc 链容量）；解法二选一：① 开 INDIRECT 描述符（`DESC_F_INDIRECT` 留 v2 位 `lib.rs :34-35`，协议在册）② `RingLayout` 放宽到 2 页连续（帧分配器无连续性保证——需 boot 期预留）。倾向 ①（INDIRECT 是 spec 正路，单页纪律不破）。帧缓冲 backing 本身 = 客机普通 RAM 页（GUI-03 :227「2D 模式资源天然驻 guest RAM」同判）。
- **ramfb 路降 fallback**：HDI 三口契约不变，guifbd 按「boot 探测到 virtio-gpu 设备与否」选后端（virtio-gpu 主 / ramfb 兜底），console 路（FULL 纯记账）与 `FBW/FBSERVE/GFXF` 标记面零演进污染（ADR #61 同款纪律）。

### 3.3 人日表（推断——以 vblk 541 行/vnet 1214 行/gui-inputd 1348+988 行既有刀距为标尺）

| 件 | 内容 | 估行 | 估人日 |
| --- | --- | --- | --- |
| berry-virtio-gpu crate | 协议编解码 + host 单测 | 0.6–1k | 1.5–2 |
| guifbd virtio-gpu 后端 | 初始化/资源/scanout/flush + INDIRECT + 标记 | 1.2–2k | 2.5–4 |
| 接线与 smoke | svcmgr 探测分支 + `VGPU ok` 标记 + 双 boot 关卡 + ramfb fallback 回归 | — | 1.5–2.5 |
| guest 半小计 | | | **5.5–8.5** |
| 壳 β-0 | Frame damage 载荷 + wgpu 呈现骨架 | — | 2–3.5 |
| 壳 β-1 | D-Bus p2p 客户端（手写线协议子集）+ Listener/Map + 输入切换 | 1–1.5k | 3–5 |
| S1 spike | 三验合一（§6） | — | 0.5–1 |
| 全链合计 | | | **10–17**（含评审，并行双会话可压到半墙钟） |

## §4 帧链性能预算（1080P@60）

### 4.1 口径表（数值两源：算术 + 归属文档/实测）

| 口径 | 每帧字节 | 60Hz 搬运量 | 二源 |
| --- | --- | --- | --- |
| 1920×1080 RGB32 | 8,294,400 B ≈ 8.29MB | **497.7MB/s** | 算术；规划 :151 同法（800×600≈1.9MB）互证 |
| 1920×1080 RGB565 | 4,147,200 B ≈ 4.15MB | **248.8MB/s** | 算术；任务书 249MB/s 与此口径相合（任务书数值出处即此或 RGB32@30，本文按 RGB32 预算——裁决 D RGB32 起步） |
| 480×1000 RGB32（phone 档现役） | 1.92MB | 115.2MB/s | 算术；ram.rs :8 实测几何互证 |

### 4.2 三加速档对比（推断——结构账一手，绝对数待 S1/B-2 icount 定标）

| 档 | guest 软光栅 | 宿主每变化帧 CPU 面 | 结构性判断 |
| --- | --- | --- | --- |
| TCG + soft + ramfb（现状） | 慢（TCG 翻译税 × 8.29MB 写/帧） | ≥4 次全帧遍历 + 207 万像素标量转换 + CPU 合成 | 1080P 滑动掉帧与用户实测一致；800×600 时代规划 §5 已备 kill 线 |
| HVF + soft + ramfb | 近原生（memcpy 实测 59.6GB/s 为量级锚） | **不变**（宿主半与 accel 无关） | 「HVF 开了仍慢」的定量解释——慢根因在渲染链 |
| **HVF + virtio-gpu 2D + β-1（目标）** | 近原生 | **damage 矩形级**：QEMU 一次 `iov_to_buf`（transfer_to_host_2d 行循环，源码 [高]）+ 共享图零编码 + GPU DMA 上传/合成 | 满帧最坏情形总线 ≈ 2×8.29MB×60 ≈ 1.0GB/s < 实测带宽 2%；典型 UI 帧 damage 5–20% → 宿主 CPU 面再降 5–20× |

QEMU 侧单帧成本细节（源码一手 [高]）：transfer_to_host_2d 按矩形行循环 `iov_to_buf`（部分宽）或单发整块（整宽快路）；resource_flush 与 scanout 矩形求交后仅调 `qemu_console_update(交矩形)`——damage 语义端到端保真，无全帧放大。

### 4.3 TCG vs HVF 与 4K/16K 页兼容（任务书待证项②）

事实链：guest 内核 PAGE_SIZE=4096（`mm/src/va.rs` :14）；宿主 hw.pagesize=16384（本机实测 [高]）；ARM 舞台上 stage-1（guest 4K granule）与 stage-2（HVF 16K granule）粒度独立合法，且 **HVF 已实跑本仓 4K 页镜像**（任务书「用户实测仍慢」= 运行证据；壳 accel 面 macOS ARM 返 HVF，`qemu.rs :57-61`）[推断+运行证据]。残余风险类 = HVF 保存/恢复面缺陷，仓内已有同族判例先例（HVF 不存取 guest FPSR，`qemu.rs :250-253` fw_cfg 判别键 + ktest 跳过断言）——S1 双 boot 断言（TCG/HVF 各起 virtio-gpu 面）收口，不预设新判别键。

## §5 分辨率动态顺势解（01 号审计项闭账）

- 消解机制三层（全部一手 [高]）：① guest 主动换分辨率 = 新几何 RESOURCE_CREATE_2D + SET_SCANOUT（scanout 边界仅要求 ≥16×16，QEMU `virtio_gpu_check_scanout_bounds` 源码执法）——scanout 几何随资源，无重启；② 宿主→guest 通知 = config events_read（VIRTIO_GPU_EVENT_DISPLAY）+ CONFIG_GEN 翻转；③ 窗口→guest 反向 = D-Bus `Console.SetUIInfo`（官方接口，改物理毫米/像素几何 → QEMU 重生成 EDID）。
- 壳侧几何动态化：`Scanout/ScanoutMap` 契约原文自带 Resize 语义（「Resize and update the display content」）；VNC 路的 DesktopSize 伪编码 `vnc.rs` 已实现（:2 注释 + :472 resize）——两条传输都天然支持中途变几何。
- 连带收益：phone 档「panel 1080×2250 / render 480×1000」的 render≠panel 双几何（profile.rs :43-46）可升为真 1080×1920 渲染档动态切换（省电时降档渲染、充电/静止时全清），`vote` 票面（berry-guiproto OP_VOTE）已有协议位承载——「砍实现不砍协议位」纪律的自然兑现面。
- ramfb fallback 路维持静态几何语义不变（01 号钉板对其仍成立）——动态面是 virtio-gpu 档的增量承诺，写进 berry-guiproto 能力位（GETINFO 格式协商集已带 per-interface version，ADR #50 补充件）。

## §6 刀序与里程碑（挂 GUI 线，文件面互斥已排）

| 刀 | 内容 | 验收线 | 依赖/互斥 |
| --- | --- | --- | --- |
| **S1（三验合一 spike，升格）** | ① 设备验：`-device virtio-gpu-device`（mmio）+ AAVMF/ramfb 共存，vprobe 扫描见 DEVICE_ID 16；② 传输验：`-display dbus,p2p=on` 最小 Listener（或 dbus-send 手验）连上 + `ScanoutMap` 在 macOS 真给共享图 fd（memfd 文件后备核实）+ VNC 增量矩形实收；③ HVF 验：HVF+virtio-gpu 双 boot 断言（4K/16K 兼容收口）。GUI-09 :193 原估 30–60 分钟（设备验单件），三验扩为 0.5–1 天 | 三验各出读数（帧矩形序列/几何/中断标记）；kill 沿 GUI-10 :117（同脏区帧耗 > 软渲染 2× → virtio-gpu 降可选后端，soft 仍唯一 CI 基线）；**降档线**：D-Bus Map 不可用 → 锁 β-0（VNC 增量档），不是 kill | 到期未跑（GUI-2 段③ 已收官）；只读 spike 零代码或探针级 |
| **B1-a guest 半** | berry-virtio-gpu crate + guifbd virtio-gpu 后端（INDIRECT）+ `VGPU ok` 标记 + ramfb fallback 分支 | 双 boot smoke（TCG + HVF）`VGPU ok`×2 + 既有 FBW/FBSERVE/GFXF 全绿（零演进污染）+ host 协议测全绿 | S1 ①③ 过门；文件面 = drivers/ + guifbd（与 GUI-2 段④ 真字体刀 berry-gfx 面互斥，可并行排刀） |
| **B1-b 壳半（呈现 GPU 化）** | winit+wgpu 呈现骨架 + β-0 Frame damage 载荷 + GPU 缩放合成/工具栏 + 截图 readback | 1080P@60 墙钟达标（满帧行为目验）+ 截图像素对拍（soft 共识）+ softbuffer 路退役 | B1-a；文件面 = tools/berry/src/display + window（与 guest 半互斥） |
| **B1-c 动态分辨率** | SET_SCANOUT 动态几何 + events/EDID 消费 + 壳 ScanoutMap Resize 接窗 fit（+可选 SetUIInfo 反向） | 运行中切分辨率/切档（480×1000↔1080×1920）不重启，01 号钉板项闭账 | B1-a/b |
| **B1-d 收尾** | 输入反走 D-Bus（VNC loopback 退役）+ icount/墙钟双轨性能标记进遥测（B-2 通道）+ CI 矩阵（macOS 主，Linux gtk 无涉） | 注入/截图/`--json` 全绿；`berry shell` 单一形态对开发者 = 「性能能用不是看图」验收 | B1-c |

里程碑挂点：本档 = 规划 :187 后端兑现序列第二档「virtio-gpu 2D（模拟器加速档）」、ADR #62「v2 virtio-gpu」承诺的兑现刀；不占 GUI-9 主线（依赖面已过），建议随 GUI-2 段④ 真字体刀并行排（双会话文件面互斥，smoke.sh 关卡归主线串行——规划 §4 并行纪律原文）。

## §7 风险与开放问题

- **D-Bus 客户端实现量**：壳需自带 D-Bus 线协议子集（p2p 免 Hello/SASL 简化），估 0.4–0.8k 行 + 测试；引 zbus = 拉 async 运行时依赖（壳现为同步线程模型）——倾向手写子集 [中]。
- **memfd on macOS 文件后备**：QEMU qemu_memfd 非 Linux 走临时文件 MAP_SHARED 后备——源码级核实挂 S1 ②；不可用则 β-0 降档线生效 [中]。
- **ATTACH_BACKING 多页 desc**：INDIRECT 未用过（berry-virtio :34 留 v2 位），首用即 8KB 表——协议自测先行 + QEMU 侧 `-d guest_errors` 对拍 [高·已识别，非隐患未知]。
- **guifbd 槽位预算**：env 8/8 恰满前科（ADR #61 理由②）——进程内后端方案已避开新守护，但 mmio 扫描/irq wait 面可能加 channel/handle 需求，开刀前先核 `CHAN_HND_MAX` 余量 [高·已识别]。
- **光栅核 1080P 帧成本**：damage 链修通后 guest 半成为新地板（berry-ui 脏区已就位、berry-gfx 消费侧 damage 面随 GUI-2 段③ 已落）——B-2 icount 定标（GUI-2 增件⑨）在 B1-d 回填读数，若 soft 核满帧行成本超帧预算则光栅核优化另立刀（不阻塞本档结构兑现）[推断]。
- **cursor/多 scanout**：v1 砍 cursor 队列与多输出（max_outputs=1 默认即单屏 [高·本机 `-device help` 实测默认值]）；UPDATE_CURSOR/多屏留 v2。
- **EDID 信任面**：GET_EDID 返回 1024B 原始块，BerryOS 不解析完整 EDID（只信 GET_DISPLAY_INFO pmodes 几何）——解析面留 v2 [推断，规范保守读法]。

## 来源索引

**本地（file:line）**：`tools/berry/src/qemu.rs` :39-77（choose_accel/auto_accel macOS ARM→HVF、deterministic→TCG）、:133-150（machine_args `-display none`）、:196-201（memory-backend-file share=on）、:202（-vnc）、:242-249（tablet/ramfb/fw_cfg display）、:250-257（HVF FPSR 判例 + accel fw_cfg）；`tools/berry/src/display/ram.rs` :63-99（静态 FbInfo open）、:123-134（caps damage=false）、:136-179（全帧 pread + 后验签名 + 标量转换）、:181-186（输入走 VNC）；`tools/berry/src/display/frame.rs` :30-36/:77-88；`tools/berry/src/display/vnc.rs` :2/:255/:472（Raw+DesktopSize+resize）；`tools/berry/src/window.rs` :1（winit+softbuffer）、:481-489/:502-533（CPU 轮询/缩放/工具栏/上屏）；`tools/berry/src/profile.rs` :31-37（1080P 不妥协）/ :43-46（phone 双几何）；`user/bins/guifbd/src/main.rs` :4-12（boot 授能链/FB_VA）、:33-56（段③ blit-at-SUBMIT + damage + credits + hdi 四 op + 零唤醒）、:77-93（软 vsync/行弹舱 1920 容忍）；`user/berry-guiproto/src/lib.rs` :63-99（present 六 op/hdi 四 op/vote）；`user/berry-sys/src/lib.rs` :240（berry_vmo_get_phys）；`drivers/berry-virtio/src/lib.rs` :14-17（单页 virtqueue 前提）、:34-35（INDIRECT 留 v2）、:197-238（mmio 寄存器表 + 设备 ID 族）；`drivers/bins/`（vblk 541 行/vnet 1214+1229 行/gui-inputd 2336 行——人日标尺）；`mm/src/va.rs` :14（PAGE_SIZE=4096）；`kernel/src/main.rs` :212-216（opt/berry/display 消费）；`docs/GUI里程碑规划-2026-10-v1.0.md` :13（S1 挂 GUI-2 前）/:92（GUI-2 刀面）/:151（800×600≈1.9MB）/:187（后端兑现序列）/:249（HDI 三口）；`docs/架构决策.md` :59（ADR #50）/:60（ADR #51）/:71（ADR #62）；`docs/berry设备壳设计草案-v1.0.md` :17-18/:156/:184-185/:198-215（v0 VNC/v2 shm ring/传输可换）；`docs/GUI-2段③消费侧首刀实施设计草案-v1.0.md` :5（段③收官 2026-11-02）；`research/GUI-03-先进渲染API全景与适配.md` :164-169/:184-194/:212-227（virtio-gpu 三模式/mmio 事实/B1 推荐）；`research/GUI-09-综合裁决建议.md` :193（S1 30–60 分钟）；`research/GUI-10-移动优先渲染深研综合裁决.md` :117（S1 kill 线）/:181（虚拟环境地形）。

**本机实证（2026-11-13，Apple M3 Pro / 18GB / hw.pagesize 16384 / qemu-system-aarch64 11.0.3 Homebrew）**：`-display help` = none/curses/cocoa/dbus（无 gtk/sdl/spice）；`-device help` virtio-gpu-device + virtio-gpu-pci 在列、vhost-user 族 0 命中、无 *-gl 设备；`-display cocoa,gl=on` 拒答「OpenGL support was not enabled in this build」；`-device virtio-gpu-device,help` = edid 默认 on/xres 默认 1280/max_outputs 默认 1/blob 默认 off；`dd if=/dev/zero of=/dev/null bs=8m count=3000` ≈ 59.6GB/s（量级锚粗测）。

**外部（URL）**：QEMU virtio-gpu 官方文档（2D 默认后端/vhost-user 不支持 MMIO/后端族）：https://qemu.readthedocs.io/en/stable-10.1/system/devices/virtio-gpu.html ；QEMU D-Bus display 官方契约（RegisterListener p2p/Listener.Scanout/Update/Unix.Map.ScanoutMap/UpdateMap/ScanoutDMABUF2/Mouse/Keyboard/MultiTouch/SetUIInfo）：https://qemu.readthedocs.io/en/stable-10.1/interop/dbus-display.html ；QEMU master `hw/display/virtio-gpu.c`（shareable pixman 资源/transfer_to_host_2d 行循环/flush 与 scanout 求交/qemu_console_update/scanout 边界 ≥16/2D 即答/set_config events_clear/ctrl_vq=0+cursor_vq=1）：https://gitlab.com/qemu-project/qemu/-/blob/master/hw/display/virtio-gpu.c ；Linux uapi `virtio_gpu.h`（BSD 头，命令集/config events/格式枚举/ctrl_hdr）：https://github.com/torvalds/linux/blob/master/include/uapi/linux/virtio_gpu.h ；Linux uapi `virtio_ids.h`（VIRTIO_ID_GPU=16）：https://github.com/torvalds/linux/blob/master/include/uapi/linux/virtio_ids.h ；Linux DRM 驱动 `virtgpu_plane.c`（dumb bo per-rect transfer+flush 同构流）：https://github.com/torvalds/linux/blob/master/drivers/gpu/drm/virtio/virtgpu_plane.c ；驱动规模参照（18 文件）：https://github.com/torvalds/linux/tree/master/drivers/gpu/drm/virtio ；virgl aarch64 史坑（GUI-03 转录未复核）：https://gitlab.com/qemu-project/qemu/-/issues/1141 。
