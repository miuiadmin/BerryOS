# GUI-04：Rust 渲染/UI 生态盘点——哪些能进 BerryOS（许可证+架构双过关）、哪些只借思路

> 调研日期：2026-10-28。任务来源：M-GUI 立项（`docs/GUI里程碑规划-2026-10-v1.0.md`）+ 用户 2026-10-28 六条裁决（最先进渲染方案 / 移动设备基准 / 吸取安卓教训 / 低功耗且优秀 / 先进 API 兼容且原生适配 / 最先进开发套件）。
>
> 方法：联网调研（WebSearch/WebFetch/crates.io API/GitHub API，关键论断逐条带 URL）+ 本地文档源码对读（`docs/GUI里程碑规划-2026-10-v1.0.md`、`docs/架构决策.md` ADR #37、`reference/README.md`、`user/berry-fbcon/`）。独立信息源 > 25 个（联网 20+ / 本地 5）。
>
> 立场：每条判定带证据与确定度（高/中/低）；矛盾证据如实并列。

---

## §0 判定框架

**统一五维**：架构模型 / 渲染后端 / 性能特征 / 许可证 / 成熟度与嵌入式·no_std·OS 集成面。

**BerryOS 硬约束**（来自本地仓库，非假设）：

1. **许可证纪律**：仓库 Apache-2.0，**GPL/AGPL 代码零进入**，MPL-2.0 = 文件级弱传染「语义可借、文件不搬」（`docs/架构决策.md` ADR #37；`reference/README.md` 协议提醒节）。
2. **验证环境无 GPU**：QEMU aarch64 virt + AAVMF + ramfb/GOP，TCG 模拟（CLAUDE.md / GUI 规划 §5 风险表）。任何「必须有原生 GPU API」的库在 v1/v2 结构性不可跑——除非自建整套 GPU 用户态驱动栈（非 Rust，见 §1.4）。
3. **全 Rust 纪律**：内核 no_std；用户态服务 std（musl 形态 berry-libc）但 GUI 规划已定「三薄服务 + 单 GUI 进程 + 软渲染脏区起步」（`docs/GUI里程碑规划-2026-10-v1.0.md` §3 裁决 C、§4 GUI-2/3）。
4. **性能哲学**：软渲染 + 脏区刷新 day-one（GUI 规划 §2 结论引用附录 B「lite 最值钱的一件」）；低功耗是一等公民（用户裁决④）——与「每帧全量重绘」模型存在结构性张力。
5. **AI 原生调度**：GUI 输入/渲染优先级是 intent 机制的天然消费面（任务书背景）。

**判定档位**：

| 档位 | 含义 |
|---|---|
| **可进** | 许可证 + 架构双过关，可作为 BerryOS 依赖直接引入（仍需 spike 验证编译面） |
| **架缝候选** | 许可证过关，但依赖 GPU/桌面生态；留作 v2+ GPU 层选型，接口缝现在留位 |
| **只借思路** | 许可证不过关或集成面过深；学设计不搬代码 |
| **不进** | 双不合格或与架构哲学冲突 |

---

## §1 wgpu —— WebGPU 标准实现：API 面的长期锚点，v1/v2 不可达

### 五维评估

| 维度 | 事实 | 来源 |
|---|---|---|
| 架构模型 | 三层：`wgpu`（用户 API，WebGPU 规范实现）→ `wgpu-core`（资源跟踪/校验）→ `wgpu-hal`（后端 trait 抽象层）；WGSL 经 Naga 翻译到各后端 | [wgpu 官方架构文档](https://wgpu.rs/doc/wgpu/documentation/internals/architecture/index.html)、[docs.rs/wgpu-hal](https://docs.rs/wgpu-hal) |
| 渲染后端 | Vulkan / Metal / D3D12 一等公民；OpenGL(3.3+)/GLES(3.0+) best-effort（GL 走翻译层，开销更高）；wasm 走浏览器 WebGPU 或 WebGL2；macOS Vulkan 需 MoltenVK、GL 需 ANGLE | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) README 平台表 |
| 性能特征 | 抽象层本身薄：调 API 直达原生后端。实测数据两则并列：① WebGPU dispatch 开销 24–36μs（Vulkan 后端）/ 32–71μs（Metal 后端）——dispatch 密集型负载最痛；② A100 上 wgpu 计算负载比此前手写 Vulkan 实现还快 ~20%（对手实现非最优，说明 wgpu 不会拖后腿）。另有社区共识：wgpu 只暴露 WebGPU 子集（单队列、无 bindless、并行度受限），上限低于原生 Vulkan | [arXiv 2604.02344](https://arxiv.org/html/2604.02344v1)、[wgpu discussion #6688](https://github.com/gfx-rs/wgpu/discussions/6688)、[Reddit: What does wgpu actually do?](https://www.reddit.com/r/rust/comments/1dg8v7v/what_does_wgpu_actually_do) |
| 许可证 | **MIT OR Apache-2.0**（crates.io 30.0.1 实测元数据）——与 BerryOS 完全兼容 | [crates.io/crates/wgpu](https://crates.io/crates/wgpu) |
| 成熟度与集成面 | 极成熟：Firefox/Servo/Deno 的 WebGPU 集成核心（README 原文），crates.io 累计 4497 万下载、近 90 天 1538 万；v30.x，MSRV 1.87，约每 3 个月一个 breaking release。**但运行时必须有原生 GPU API 可用**（README 环境说明）——这是 OS 集成面的死结：要么有 Vulkan/Metal/D3D12/GL 驱动栈，要么没有 wgpu。无官方 no_std/裸机支持；嵌入式采用 = Android GLES/Vulkan 这类「有驱动栈的嵌入式」，非裸机 | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu)、[crates.io](https://crates.io/crates/wgpu) |

### 1.4 OS 集成面专项：BerryOS 要跑 wgpu 缺什么

wgpu 需要一条「原生 GPU API」腿。QEMU virtio-gpu 有三档能力（[kraxel.org（virtio-gpu 作者博客）](https://www.kraxel.org)、[Phoronix: Venus Vulkan 1.3](https://www.phoronix.com)、[QEMU virtio-gpu 文档](https://lira.no-ip.org)）：

| virtio-gpu 模式 | 能力 | guest 侧需要 | 对 BerryOS 可行性 |
|---|---|---|---|
| 2D 模式 | framebuffer + 2D blit/setstride，无 3D | virtio-gpu 驱动即可 | **可行**（drivers/berry-virtio 框架现成）——但 2D 模式跑不了 wgpu（无 3D API） |
| virgl | host OpenGL 加速（virglrenderer） | Mesa virgl GLES 驱动（C/C++ 巨型栈） | 不可行——违反全 Rust 纪律与体量现实 |
| Venus | host Vulkan 直通（Vulkan 1.3） | Mesa Venus Vulkan 驱动 + host blob/hostmem 支持 | 同上不可行；且 QEMU 11 对 host GPU/Mesa/virglrenderer 版本敏感 |

**判定：架缝候选（高确定度）**。v1/v2 不进（无驱动栈）；但 wgpu = WebGPU/WGSL 事实标准，是「先进渲染 API 要能兼容」（用户裁决⑤）的最自然落点——v2+ 若走 virtio-gpu + 自研最小 Vulkan/GLES 子集驱动（或等 Rust GPU 驱动生态成熟），wgpu 是 API 层首选。接口缝 = berry-gfx 的 trait 面按「后端中立」设计（wgpu-hal 的 trait 分层是现成范式）。

**借思路**：wgpu-hal 的后端 trait 抽象（能力协商、feature 追踪、资源状态机）；Naga 的 WGSL→多后端编译管线（若 BerryOS 未来 shader 面需要）。

---

## §2 vello 家族 —— 本次盘点最大发现：vello_cpu 双过关

### 2.1 谱系演化：从 compute shader 单路线到三 renderer 家族

vello 已从「compute shader 2D」单一叙事演化为共享 **Sparse Strips 架构**的三 renderer 家族（[github.com/linebender/vello](https://github.com/linebender/vello) README，2026-10 现行版）：

| 成员 | 路线 | 成熟度（README 原文口径） |
|---|---|---|
| **vello_cpu** | 纯 CPU，SIMD + 多线程优化 | 「**最成熟**的选择，适合无 GPU 或 GPU 弱的设备」 |
| **vello_gpu** | CPU 预处理路径 + GPU 光栅化/合成；**不依赖 compute shader**；支持 WebGL2 与 wgpu 双后端 | 生产 GPU 用途的成长线 |
| vello（compute renderer） | 原始 compute-centric 设计 | **已降级到 `research/` 目录**，实验性质 |

> 这个演化本身就是重要信号：Linebender（Raph Levien，Skia 前成员/Servo 时代 piet 作者）承认纯 compute 路线在兼容性与成熟度上不及「CPU 展平 + strips」路线。与 Skia/CPU 2D 库的竞争格局：2025-07 Linebender 月报口径「vello_cpu 有望成为 Rust 生态最快的纯 CPU 2D 渲染器，多数场景已胜 tiny-skia」（[Linebender 2025-07](https://linebender.org)、[vello cpu demo](https://linebender.org/vello/cpu)）。

### 2.2 vello_cpu 五维评估

| 维度 | 事实 | 来源 |
|---|---|---|
| 架构模型 | Sparse Strips：CPU 上路径展平 → 生成稀疏条带 → 光栅化/合成；`RenderContext`（固定场景区）+ `Resources` + `Pixmap` 输出；`RenderMode::OptimizeSpeed/OptimizeQuality` 两档；u8 定点管线 / f32 管线双轨（feature 切换） | [vello_cpu README/lib.rs](https://github.com/linebender/vello) |
| 渲染后端 | 纯 CPU（SIMD 经 fearless_simd 抽象，带标量 fallback）；多线程可选（rayon，可关）；**无 GPU 依赖** | [vello_cpu/Cargo.toml](https://github.com/linebender/vello) |
| 性能特征 | 定位「无 GPU 设备的高性能 2D」；Rust 生态最快纯 CPU 2D 渲染器（官方口径，见 2.1）；SIMD 加速 + 多线程伸缩 | 同上 |
| 许可证 | **Apache-2.0 OR MIT**——完全过关 | crates.io `vello_cpu` 0.3.0 元数据 |
| 成熟度与集成面 | 0.3.0（pre-1.0 但生态采纳强劲，见 2.3）；**`#![no_std]` + `extern crate alloc` + `#![forbid(unsafe_code)]`**（lib.rs 实测源码）；浮点可选 libm 替代 std（`libm` feature）；依赖树 no_std 友好（bytemuck/peniko/smallvec/guillotiere/thiserror-default-off，vello_common/Cargo.toml 实测） | [vello_cpu/src/lib.rs](https://github.com/linebender/vello)、[vello_common/Cargo.toml](https://github.com/linebender/vello) |

### 2.3 生产采纳（crates.io 反向依赖实测，2026-10-28）

- **epaint 0.37**（= egui 的绘制层）依赖 vello_cpu ^0.2，features = std/u8_pipeline/f32_pipeline，该依赖边承载 2868 万下载；egui CHANGELOG 明载「字体渲染后端从 ab_glyph 切到 skrifa + vello_cpu，启用字体 hinting」（[egui CHANGELOG #7694](https://github.com/emilk/egui)）。
- **Servo**（servo-canvas / servo-malloc-size-of 0.7.0）依赖 vello_cpu ^0.2 开 multithreading——Servo canvas 线生产在用。
- hayro（PDF/Typst 生态，依赖边 424 万下载）、anyrender_vello_cpu（Dioxus Blitz 的渲染抽象层 anyrender 的 vello_cpu 后端）。
- vello_cpu crate 累计 860 万下载、**近 90 天 670 万**（增速陡峭，2026 年起飞曲线）。

### 判定：**可进（需 spike 验证）——净收益候选**（确定度：中高）

许可证、no_std、纯 CPU、Apache 双许可、生产采纳五项全绿，是本次盘点唯一「双过关」的重型渲染库。保留项（故「中高」非「高」）：

1. pre-1.0，API 每 3 个月级变动（跟 0.x 版本锁步即可，uu_* 先例同款纪律）；
2. 依赖树含 alloc + hashbrown + fearless_simd——**能否在 berry-libc/aarch64 目标干净编译未验证**（spike 事项，见开放问题）；
3. Sparse Strips 是「矢量场景」渲染器：全场景重光栅模型 vs BerryOS 脏区模型的关系需要实测裁剪（矩形 blit 快速路径 vs 全 vello_cpu 两条路，B-2 icount 通道量帧成本后定）；
4. fearless_simd 在 aarch64 上是否走 NEON、fallback 性能档未查（TCG 下 SIMD 模拟成本存疑）。

**无论依赖进否，算法必借**：Sparse Strips（展平/条带化/扫描合成三段分离）就是 2026 年 CPU 2D 光栅的 state of the art，berry-gfx 六原语之上的路径/圆角/抗锯齿面照此设计。

---

## §3 Slint —— 技术形态最贴合，许可证三锁全不合

### 五维评估

| 维度 | 事实 | 来源 |
|---|---|---|
| 架构模型 | 自有 `.slint` 声明式 DSL（markup）+ 属性绑定响应式 + Rust/C++/JS/Python 绑定；编译期代码生成或解释器两模式；保留模式组件树 | [github.com/slint-ui/slint](https://github.com/slint-ui/slint) |
| 渲染后端 | femtovg（GPU）/ Skia（GPU）/ **软件渲染器**（纯 CPU，支持整帧 framebuffer 或**逐行渲染省内存**）三官方 renderer | [Slint 0.3 MCU platform API 博文](https://slint.dev/blog/slint-0.3-released-with-platform-api-for-mcu)、[lib.rs/slint/features](https://lib.rs/crates/slint/features) |
| 性能特征 | 嵌入式定位卖点：**MCU ~264-300KB RAM 可跑**（官方实测移植 RP2040 264K RAM；Slint 1.4 口径「运行时 < 300KB RAM」）；`#![no_std]` + 需全局分配器（官方 mcu.md） | [slint.dev/blog: Porting to a MCU with 264K RAM](https://slint.dev/blog/porting-slint-to-microcontrollers)、[docs.rs mcu.md](https://docs.rs/crate/slint/latest/source/mcu.md)、[DevClass Slint 1.4](https://www.devclass.com/development/2024/02/01/slint-14-released-lightweight-cross-platform-gui-toolkit-for-rust-c-and-javascript/1625115) |
| 许可证 | **三选一**：GPL-3.0-only **OR** LicenseRef-Slint-Royalty-free-2.0 **OR** LicenseRef-Slint-Software-3.0（商业）（crates.io 1.18.1 实测元数据）——**这是最大问题，见 §3.1** | [crates.io/crates/slint](https://crates.io/crates/slint)、[slint.dev/pricing](https://slint.dev/pricing) |
| 成熟度与集成面 | 1.18.1 稳定版线、24k stars、近 90 天 83 万下载（六家中生态热度最高之一）；no_std/MCU 面是全行业最深（ESP32-S3 智能表裸机移植演讲在册）；工具链 = Slint LSP + VS Code 插件 + Live Preview + 设计器 | [github.com/slint-ui/slint](https://github.com/slint-ui/slint)、[OxidizeConf: Slint on bare-metal ESP32-S3](https://oxidizeconf.com/sessions/no_std_no_problem_bringing_slint_to_a_bare-metal_esp32_s3_smartwatch) |

### 3.1 许可证判定：三锁全不合 BerryOS（确定度：高）

逐条对照 ADR #37（Apache-2.0 / GPL 零进入）：

1. **GPLv3 路**：Slint 以库形式链入 → 整个系统 UI/宿主应用被传染 GPL-3.0。BerryOS 仓库 Apache-2.0，**直接违禁**。GPLv3 只对「开源 GPLv3 兼容的应用」免费——BerryOS 作为 OS 平台不合。
2. **Royalty-free 2.0 路**（许可原文实测，[LICENSES/LicenseRef-Slint-Royalty-free-2.0.md](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md)）：
   - 仅限 Desktop/Mobile/Web **Application**（「设计运行于通用计算机/浏览器沙箱/通用移动设备的程序」）；
   - **明文排除 Embedded Systems**（「 larger mechanical or electrical system 内执行特定任务的计算机系统」）——BerryOS 手机/手表形态的系统 UI 是否算「mobile application」存在解释空间，但 OS 平台整体（尤其是手表/设备侧）踩在排除带边缘，法律灰区不可作为底座；
   - **「Applications may not expose the Software's APIs, in whole or in part」**——BerryOS 若把 Slint 当「第三方应用 UI SDK 底座」，SDK 向应用暴露 Slint API = 直接违约。这一条单独判死「SDK 底座」用途；
   - 禁止单独分发软件本体；强制 attribution（AboutSlint 部件或官网徽章）。
3. **商业路**：embedded 每设备 $1 起一次性版税 + 订阅制——与 BerryOS 开源免费分发模型冲突。

> 三条路没有一条允许「Apache-2.0 开源 OS 把 Slint 作为系统 UI + 应用 SDK 底座」。商业公司（SixtyFPS）刻意用此结构保护商业线——无可指责，但与本项目互斥。

### 判定：**不进（许可证一票否决）；思路高价值借用**（确定度：高）

**借思路清单**（价值极高，Slint 是「嵌入式 + 声明式 + 工具链」三合一的唯一成熟样板）：

- **DSL + 属性绑定**的 UI 表达形态（`.slint` 文件分离 UI 与逻辑）——直接对应「别人开发界面又容易渲染、又高效、又正确」（用户裁决⑥）的答案形态；
- **LSP + Live Preview** 工具链架构：改 markup 即见画面，无需重编译宿主——BerryOS 开发套件的参照蓝图；
- **逐行渲染**（line-by-line software rendering）省帧缓冲内存的档位设计——手表/低功耗端可用；
- 软渲染器「整帧/逐行」双档 + 嵌入式内存预算方法论（<300KB 实测路径）。

---

## §4 iced —— Elm 架构的生产样板

### 五维评估

| 维度 | 事实 | 来源 |
|---|---|---|
| 架构模型 | The Elm Architecture：State / Messages / View / Update 四分离；renderer-agnostic runtime + widget 库 + winit 窗口壳；纯函数 view(state) → 描述性 UI | [iced book: Architecture](https://book.iced.rs/architecture.html)、[github.com/iced-rs/iced](https://github.com/iced-rs/iced) |
| 渲染后端 | 双内建：`iced_wgpu`（Vulkan/Metal/D3D12）+ **`iced_tiny_skia`（纯 CPU 软渲染 fallback）** | [github.com/iced-rs/iced](https://github.com/iced-rs/iced) README |
| 性能特征 | 桌面级流畅；实测样点：2023 三家对比中启动 ~380ms（含 wgpu 初始化）；声明式 diff + 保留模式渲染，无全帧重绘问题 | [Lukas Kalbertodt 对比测试](http://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html) |
| 许可证 | **MIT**——过关 | [crates.io/crates/iced](https://crates.io/crates/iced) 0.14.0 元数据 |
| 成熟度与集成面 | 0.14.0（自述 experimental，但背书硬）：**System76 COSMIC 桌面（1.5 版线，Pop!_OS 24.04 起出厂）= libcosmic 包 iced 的生产实现**，是 Rust 自绘 GUI 最大规模生产案例；无 no_std 支持、深度绑定 winit/桌面窗口生态——OS 移植面深（窗口/事件/字体/剪贴板全套要自实现 shell） | [iced.rs](https://iced.rs)、[Phoronix: COSMIC uses iced](https://www.phoronix.com/forums/forum/software/distributions/1349435-system76-s-pop-_s-cosmic-desktop-to-make-use-of-iced-rust-toolkit-rather-than-gtk) |

### 判定：**不进（集成面深 + winit/wgpu 栈不可达）；Elm 范式 + 生产验证经验高价值借用**（确定度：高）

- COSMIC = 「用 iced/libcosmic 造系统级桌面 UI」的**现成全量先例**——其教训（widget 面缺口、theming、多显示器、无障碍补课）是 BerryOS launcher/系统 UI 的免费教材；
- Elm 三分离（message/update/view）是**应用 SDK 编程模型的候选范式**：纯函数 view 天然适配脏区（diff 前后描述）与确定性测试（AI 测试友好化：view(state) 无副作用可对拍）；
- `iced_tiny_skia` 软渲染 fallback 的存在本身佐证「桌面框架也需要 CPU 路线」。

---

## §5 egui —— 即时模式：上手最快，与低功耗/脏区哲学结构性冲突

### 五维评估

| 维度 | 事实 | 来源 |
|---|---|---|
| 架构模型 | 即时模式（immediate mode）：每帧全量重跑 UI 代码生成布局，无跨帧保留状态；状态外置在用户侧 | [github.com/emilk/egui](https://github.com/emilk/egui) |
| 渲染后端 | epaint 三角化 tessellation → 官方 wgpu / glow(GL) 后端；**0.37 起字形光栅内嵌 vello_cpu**（§2.3）；社区有 softbuffer 纯 CPU 路线示例 | [egui CHANGELOG](https://github.com/emilk/egui)、[docs.rs/egui](https://docs.rs/egui/latest/egui) |
| 性能特征 | README 原文承认：「即时模式每帧做全量布局，布局代码必须快，复杂 GUI 会吃满 CPU」；社区实测复杂桌面型 UI 全帧重绘 = CPU 大户；**缓解 = reactive 模式**（repaint on demand，输入/定时器触发才重绘），但静止界面仍非零成本。三角面 GPU 光栅模型在纯 CPU 环境成本高（TCG 下尤甚） | [egui README](https://github.com/emilk/egui)、[Tritium: Rust GUI observations](https://tritium.legal/blog/desktop)、[Reddit: How to reduce CPU usage in egui](https://www.reddit.com/r/rust/comments/16dcngv/how_to_reduce_cpu_usage_in_egui) |
| 许可证 | **MIT OR Apache-2.0**——过关 | crates.io 0.37.0 元数据 |
| 成熟度与集成面 | 生态最活跃（累计 2873 万下载、近 90 天 810 万，六家第一）；游戏内工具/调试面板的事实标准；no_std 不支持（字体/文件/线程面依赖 std）；集成需要自写后端（官方壳 eframe 绑 winit） | crates.io、GitHub |

### 判定：**不进系统底座（确定度高）；调试工具面挂账候选（确定度中）**

- 与 BerryOS 两根柱子冲突：①**低功耗**——「每帧全量重绘 + reactive 补丁」vs BerryOS「脏区 + 静止零帧」哲学；②**软渲染**——三角化模型为 GPU 设计，CPU 光栅全屏 UI 不划算。
- 但即时模式 API 的开发体验优势真实（上手 10 分钟、无状态同步 bug）——**内核 DFX 面板/系统监视器类内部工具**若未来需要快速搭 UI，egui + 自写 berry 后端（复用 vello_cpu 字形）是合理挂账项，不进 v1。

---

## §6 速览：makepad / xilem / libcosmic / Dioxus / Leptos / GPUI + 基础件

| 家伙 | 五维速览（合并） | 判定 |
|---|---|---|
| **makepad** | 自绘 + Live DSL（`.live` markup）+ 自研 GPU 抽象的完整方案；MIT（GitHub API 实测）；2025-05 宣布 1.0（HN）；7.2k stars。**坑**：crates.io 伞 crate 停在 0.1.0/2019——实际发布走 makepad_* 子 crate 群，生态接入成本高；平台目标 = 桌面/web/移动，无嵌入式 no_std 线 | **不进**（单仓巨型生态 + 无 OS 集成面）；**DSL + live-coding（改样式即时生效）思路高价值借**——用户裁决⑥「开发套件最先进」的直接参照 |
| **xilem** | Linebender 旗舰：声明式 reactive（SwiftUI/Elm 灵感）+ view diffing（Xilem 核心算法创新）；**Apache-2.0**（crates.io 0.4.0）；**官方自述 experimental，pre-1.0**（linebender.org 标签）；下载量小（1.2 万） | **不进**（自认实验）；**view diffing 算法思路借**——声明式树 → 保留模式树的最小 diff 映射，正是「声明式面 + 脏区底」的桥 |
| **libcosmic** | System76 COSMIC 生产工具包（1.5 版线出厂 Pop!_OS）；包 iced + cosmic 设计系统/widget 全家桶；**MPL-2.0**（GitHub API 实测）= 文件级弱传染——**语义可借、文件不搬**（hubris 同款纪律，`reference/README.md` 在案） | **不进**（Linux/COSMIC 生态深绑）；**设计系统组织 + 系统 UI 组件清单借**（状态栏/设置页/控制中心的 widget 分层是现成蓝图） |
| **Dioxus** | React 式 RSX 声明式全栈框架；**MIT OR Apache-2.0**；0.7.10，近 90 天 128 万下载；渲染器 = webview（桌面默认）/wgpu/skia(Freya) 实验线 + **Blitz**（自研原生渲染器，anyrender 抽象 + vello_cpu 后端） | **不进 v1**；**应用 SDK 的「React 心智 + 热重载 DX」形态参照**（rsx 宏 + 热重载是「容易开发」的生态答案） |
| **Leptos** | Web-first（WASM/SSR）细粒度响应式；**MIT**；0.8.22，近 90 天 152 万下载 | **不进**（浏览器生态）；**细粒度响应式（signals）思路供 v2 状态管理参考** |
| **GPUI** | Zed 编辑器的 UI 框架，已独立发 crates.io（**Apache-2.0**，0.2.2，近 90 天 23 万下载）；**混合即时/保留模式** + GPU 加速；官方口径「目前 tied to Zed」 | **不进**（绑定 Zed 仓库 + 桌面 GPU）；**「混合模式 + Tailwind 式布局 API」思路借**——证明高性能编辑器级 UI 不必二选一 |
| **tiny-skia** | Skia 的 Rust 子集移植（路径/光栅/图片 ops）；**BSD-3-Clause**；0.12.0，累计 6151 万下载（iced_tiny_skia 底座） | **架缝候选/思路**——许可证兼容（BSD-3 带 attribution 可与 Apache-2.0 共存）；是「保守稳妥 CPU 2D」备选，但路线图上被 vello_cpu 代际压制 |
| **softbuffer** | 纯 CPU framebuffer presentation 库（窗口系统无关）；MIT/Apache；累计 3058 万下载 | **思路借**——「最小显示面抽象」正是 BerryOS guifbd 的同构物，其 API 面（Buffer+present）值得对读 |
| **floem** | Lapce 编辑器的 UI 框架（xilem 思路 + 自绘）；MIT；0.2.0，下载量小 | 不进；编辑器级 UI 性能工程参照 |

---

## §7 Rust GUI 共识现状：无共识本身就是结论

[areweguiyet.com](https://areweguiyet.com)（活跃维护，新闻流更到 2026-03）的官方口径：

- 标语：「**The roots aren't deep but the seeds are planted**」（根不深，种子已种下）；
- 结论段原话要义：寻求「成熟、易用、全 Rust」的方案「**大概率会失望**」；WebRender 被定性为「GUI 框架的地基而非框架本身」；
- 收录约 50 个 crate，分三大阵营：**平台工具包绑定**（GTK/Qt/Win32/FLTK/lvgl）、**HTML/web 系**（Tauri/Yew/Leptos/Dioxus）、**纯 Rust 自绘**（iced/egui/Slint/Xilem/GPUI/Makepad…）。无任何 crate 获得「成熟」标记；唯一状态标注是某 crate 的「DEPRECATED」。

**两大路线之争现状**（社区讨论综合：[KAS: State of GUI 2022](https://kas-gui.github.io/blog/state-of-GUI-2022.html)、[users.rust-lang: Graphics why immediate mode](https://users.rust-lang.org/t/graphics-why-immediate-mode/93356)、[Lobsters: Future of rem 2024](https://lobste.rs/s/bt4enq/future_rem_cross_platform_gui_rust_2024)）：

1. **保留 vs 即时**：即时派（egui）赢在开发体验与无状态同步 bug；保留派赢在静止零开销/脏区/低功耗。**对 BerryOS（低功耗一等公民）结构性地站保留派**——但 GPUI 的混合模式与 egui 的 reactive 补丁说明边界在软化。
2. **自绘 vs 平台控件**：自绘派（iced/Slint/egui/GPUI/Makepad）在 Rust 生态占绝对主流（跨平台一致性 + 无 GTK/Qt FFI 负担）；平台绑定派（gtk-rs/Tauri webview）在桌面应用层务实流行。**BerryOS 无平台控件可绑（自己就是 OS）——自绘是唯一选项，Rust 生态主流恰好同向**。

> 矛盾证据如实并列：社区对「egui 是否适合复杂桌面 UI」分歧明显（官方 README 的 CPU 警告 vs 生态热度第一）；对 Tauri 的「最佳不理想选项」评价反映纯 Rust 自绘的成熟度缺口仍真实存在。**结论：没有任何现成 GUI 框架可以直接拿来当 BerryOS 系统底座——所有框架都预设了桌面 OS（winit/窗口/字体/剪贴板/事件循环）这套我们恰好要自建的地基。**

---

## §8 关键判定

### 8.1 作为「OS 系统级 UI 底座」（launcher/系统 UI）——哪些是净收益

| 引入 | 理由 |
|---|---|
| **vello_cpu（净收益第一候选）** | 双过关唯一重型件；berry-gfx 的矢量/文字/抗锯齿面直接复用其 Sparse Strips；生产采纳（egui/Servo）+ no_std + forbid(unsafe) —— 先 spike 后定依赖深度 |
| **fbcon 自研面保留** | 矩形 blit/字形/蜂格 = 图标桌面主负载，自研快速路径 + 脏区仍是骨架（GUI 规划既定）；vello_cpu 补「圆角/渐变/矢量图标/高质量文字」的先进性面（用户裁决①） |
| 其余全部不进 | wgpu（无 GPU 栈）、slint（许可证）、iced/egui/makepad/xilem（桌面生态绑定）、libcosmic（MPL+生态）——系统底座 = 自研 berry-gfx/berry-ui + vello_cpu 算法/依赖 |

### 8.2 作为「第三方应用 UI SDK 底座」——哪些形态可推

- **编程模型**：Elm/iced 三分离（message/update/view 纯函数）为基座——确定性测试友好（AI 测试友好化支柱）、脏区天然、声明式表达；xilem view-diffing 与 Dioxus RSX 热重载为进阶形态参照。
- **表达形态**：Slint 证明「DSL + LSP + Live Preview」是嵌入式端最优开发体验；BerryOS 可走「Rust 宏 DSL（rsx 风格）→ 后期独立 markup + LSP」两步——**兼容先进 API 形态但原生适配**（用户裁决⑤：兼容了不等于适配好）。
- **渲染面**：应用共享 berry-gfx/berry-ui；远期 GPU 线（virtio-gpu）到位后，应用面 API 若对齐 WebGPU（wgpu）即得生态兼容。
- **不可推**：把 egui/iced 整框架塞给第三方当 SDK（桌面栈绑定 + 版本节奏不受控）。

### 8.3 许可证纪律逐家判定表（对 Apache-2.0 仓库）

| 库 | 许可证（实测源） | 判定 |
|---|---|---|
| wgpu / egui / Dioxus / softbuffer | MIT OR Apache-2.0（crates.io） | **可进**（专利条款取 Apache 侧） |
| vello / vello_cpu | Apache-2.0 OR MIT（crates.io） | **可进** |
| iced / Leptos / floem | MIT（crates.io） | **可进**（MIT 无显式专利授权——按 ADR #37 取舍记录在案即可） |
| xilem / GPUI | Apache-2.0（crates.io） | 可进（成熟度另行判定） |
| makepad | MIT（GitHub API） | 可进（集成面另行判定） |
| tiny-skia | **BSD-3-Clause**（crates.io） | 可进（保留 attribution/notice 即与 Apache-2.0 共存） |
| libcosmic | **MPL-2.0**（GitHub API） | **只借语义、文件不搬**（文件级弱传染；hubris 先例同款纪律） |
| **Slint** | GPL-3.0-only OR Royalty-free-2.0 OR 商业（crates.io） | **不进**（§3.1 三锁全断；GPL 传染 / RF 禁 API 暴露+排除 embedded / 商业版税） |
| Skia（vello 对手参照） | BSD-3-Clause（C++） | 只看思路（语言纪律） |

---

## 关键发现

1. **vello_cpu 是本次盘点唯一「许可证 + 架构」双过关的重型渲染库**：Apache-2.0 OR MIT、`#![no_std]` + `extern crate alloc` + `#![forbid(unsafe_code)]`、纯 CPU Sparse Strips 渲染、libm 浮点替代——且已被 egui 0.37（epaint 字形光栅）与 Servo（canvas，multithreading）生产采纳（crates.io 反向依赖实测 + egui CHANGELOG）。
2. **vello 谱系已重组**：compute-shader 原始路线降级进 `research/` 实验目录，生产线转向共享 Sparse Strips 架构的 vello_cpu（最成熟）与 vello_gpu（CPU 预处理 + GPU 光栅化、不依赖 compute shader、支持 WebGL2/GLES 级）——对无 GPU 设备和弱 GPU 设备双双更友好（vello README 现行版）。
3. **wgpu 许可证过关（MIT/Apache）但 v1/v2 结构性不可达**：运行时必须有原生 GPU API；QEMU virtio-gpu 三模式中 virgl/Venus 都需要 guest 侧 Mesa C 驱动栈（违反全 Rust 纪律），2D 模式跑不了 wgpu——wgpu 的定位 = v2+ GPU 层 API 选型的架缝候选（wgpu README + kraxel.org + Phoronix）。
4. **Slint 许可证三锁全断**：GPLv3 传染（违 ADR #37）、Royalty-free 2.0 明文「Applications may not expose the Software's APIs」+ 排除 Embedded + 禁单独分发（判死 OS SDK 底座用途）、商业线按设备收费——但其 no_std/MCU（264KB RAM 实测）+ 软渲染逐行档 + DSL/LSP/Live-Preview 工具链是全行业最贴 BerryOS 需求的技术形态，思路价值极高（slint.dev licensing 原文 + pricing 页）。
5. **iced（MIT）拿到 Rust 自绘 GUI 唯一大规模生产背书**：System76 COSMIC 1.5 出厂 Pop!_OS（libcosmic MPL-2.0 包 iced）；Elm 四分离是 BerryOS 应用 SDK 编程模型的最优范式候选；但 winit/wgpu 桌面栈绑定使 OS 集成面不可行（iced.rs + Phoronix）。
6. **egui（MIT/Apache）生态热度第一（90 天 810 万下载）但即时模式全帧重绘与 BerryOS 低功耗 + 脏区哲学结构性冲突**——不进系统底座；0.37 起其字形光栅已内嵌 vello_cpu，侧面验证 vello_cpu 的工业可用性（egui README + CHANGELOG）。
7. **Rust GUI 无共识**：areweguiyet 官方口径「根不深种子已种」「成熟易用全 Rust 方案大概率失望」，约 50 crate 三阵营（平台绑定/HTML 系/纯 Rust 自绘）；自绘派占主流且与 BerryOS「自己就是 OS、无平台控件可绑」同向（areweguiyet.com）。
8. **所有现成框架都预设桌面 OS 地基**（winit 窗口/事件/字体/剪贴板）——这套地基恰好是 BerryOS 要自建的部分，故「整框架引入」路径全灭，「渲染核 + 编程范式 + 工具链思想」三层解耦引入是唯一可行姿势。
9. **基础件许可证全景干净**：主流 Rust 渲染生态几乎全 MIT/Apache-2.0/BSD-3/MPL-2.0；唯一 GPL 商业双锁大件 = Slint——逐家判定见 §8.3 表（crates.io + GitHub API 实测）。
10. **makepad 的 crates.io 伞 crate 停在 2019 年 0.1.0**（实际走子 crate 群发布）——生态健康度评估不能只看 GitHub stars，发布线实况要查 crates.io（crates.io API 实测）。

## 对 BerryOS 的架构启示

1. **GUI-2（berry-gfx）改为「双核」结构评估**：自研快速路径（矩形 fill/blit/字形——图标桌面主负载，fbcon 顺延）+ vello_cpu 依赖或算法移植（矢量路径/圆角/高质量文字/抗锯齿——用户裁决①「最先进」的落点）；先做 spike：vello_cpu `default-features = false` + `libm` 在 aarch64 no_std/berry-libc 目标能否干净编译、依赖树（peniko/smallvec/guillotiere/hashbrown）收敛度。
2. **帧成本走 B-2 icount 通道定裁剪**：Sparse Strips 全场景光栅 vs 矩形快速路径的 TCG 帧成本用确定性指令数通道量，再定 vello_cpu 的使用深度（库依赖 vs 算法参照）——不让「先进」变成「TCG 下跑不动」。
3. **GPU 架构缝按 vello_gpu + wgpu 双层留位**：berry-gfx trait 面向「软/GPU 双后端」设计（OpenHarmony BaseGfxEngine 同构，GUI 规划附录 B L0 已在案）；v2 virtio-gpu 线 = 2D 模式起步（零 host 3D 依赖）→ GPU 光栅化选 vello_gpu 形态（不依赖 compute shader，可落 GLES 级）→ API 面对齐 wgpu/WebGPU 兑现「先进 API 兼容」裁决。
4. **应用 SDK 编程模型定 Elm 范式**：message/update/view 纯函数三分离起步（iced 验证过的生产形态 + 确定性可对拍 = AI 测试友好化直接受益）；v2 再上声明式宏 DSL（Dioxus RSX 形态）+ view diffing（xilem 思路）。
5. **开发套件把 Slint 工具链思想列为对标蓝图**：markup/DSL + LSP + Live Preview + 逐行渲染低功耗档——「容易渲染、高效、正确」（用户裁决⑥）的生态答案是工具链而非框架本体；列入 M-GUI 后的 SDK 里程碑规划。
6. **许可证入仓门禁固化**：GUI 线新依赖一律先过「crates.io license 字段 + LICENSE 文件双源核对」；MPL-2.0 只借语义（libcosmic 同 hubris 纪律）；Slint 及一切 GPL/双锁商业件自动落「只看思路」档——建议随刀落 ADR。
7. **低功耗 UI 哲学写进 GUI 规划的对立面清单**：否决「每帧全量重绘」模型（egui 式）为系统底座；静止界面零帧 + 脏区 + vsync 驱动 = 与 tickless/no-idle-wake 电源纪律同构的 UI 版立法——这正是「又要低功耗又要优秀」（用户裁决④）的架构表达。
8. **吸取安卓教训的落点**（用户裁决③）：输入→渲染→合成链路的优先级仲裁从 day-one 进 intent 机制消费面——launcher 交互动画帧 = 高 intent、壁纸静止 = 零帧零唤醒；GPUI「编辑器级低延迟」与 COSMIC 的生产经验都是参照系。

## 开放问题

1. **vello_cpu spike 未做**：能否在 BerryOS 目标（aarch64 + berry-libc/no_std + libm）干净编译？fearless_simd 在 aarch64 走 NEON 还是标量 fallback？TCG 下 SIMD 模拟成本几何？——GUI-2 首刀前必须验证。
2. **vello_cpu 与脏区的组合形态**：整场景 RenderContext 重建每帧成本 vs 增量条带缓存——文档未覆盖增量使用模式，需读源码 + 实测定。
3. **virtio-gpu 2D 模式的 QEMU 实测**：AAVMF + virtio-mmio 挂 virtio-gpu-device 的 2D 面（setcursor/resource blit）实际可用性与 ramfb/GOP 双路关系——GUI 规划裁决 A 的姊妹问题，未在本次调研范围内实测。
4. **Slint Royalty-free 对「手机 OS 系统 UI」的定性**：mobile application 定义是否覆盖 OS 系统进程——法律解释问题，BerryOS 已按最严口径判死，若未来商务条件变化可复议（低优先）。
5. **字体线**：vello_cpu/glifo（text feature）与 BerryOS 点阵字形档（genfbfont.py 单一事实源）的关系——矢量字体（skrifa/parley 生态）何时引入、成本多少，另开字体专项调研。
6. **wgsl/WebGPU 应用面暴露的形态**：若 v2+ 引入 wgpu，第三方应用是直接拿 wgpu API 还是经 berry-gfx trait 中转——涉及 GPU 能力模型（VMO 授能 → GPU 上下文）如何进能力模型，未定。
