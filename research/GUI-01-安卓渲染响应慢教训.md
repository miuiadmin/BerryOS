# GUI-01 安卓渲染响应慢的教训：根因考古与 BerryOS day-one 预防清单

> **调研任务**：安卓渲染响应慢的根因考古 + 新 OS 的 day-one 预防清单。
> **背景落点**：BerryOS M-GUI 里程碑（`docs/GUI里程碑规划-2026-10-v1.0.md`）已立项；用户 2026-10-28 裁决「用最先进的图形渲染方案、按移动设备设计、吸取安卓渲染响应慢的教训、又要低功耗又要优秀」——本报告即第 ③ 条裁决的证据底座。
> **方法**：联网调研（WebSearch/WebFetch，每条论断标 URL）+ 本地文档/源码（标仓库路径）。确定度分级：**高**（官方文档/一手演讲/源码）、**中**（知名工程师博客/权威媒体实测）、**低**（社区二手转述/未定案）。
> **调研日期**：2026-10-28。

---

## 1. 渲染管线演进史：八代考古，每代解决的病与残留的病

Android 图形栈十五年演进有一条清晰主线：**每一代都在把「随机时刻开始的同步工作」改造成「对齐显示节拍的流水线」，并把重工作从主线程上剥离**。这条主线本身就是 BerryOS 最重要的教科书。

### 1.1 G0：软件绘制时代（Android 1.0–2.3，2008–2010）

- **架构**：应用在主线程用 Skia 软件 Canvas 把 View 树直接绘制到窗口 bitmap，SurfaceFlinger 用 OpenGL ES（或早期 overlay）合成各窗口（[charleszblog 图形内幕系列](https://charleszblog.wordpress.com/2014/02/22/understanding-android-internals-surfaceflinger-i)、[source.android.com 图形架构](https://source.android.com/docs/core/graphics)）。
- **病**：
  - 每次绘制整棵子树重跑绘制代码——即使内容没变（Romain Guy 官方博文对旧模型的批判，见 1.2 的来源）；
  - 无 GPU、无 display list、无 vsync 对齐，CPU 绘制就是硬上限；
  - 绘制与主线程业务（输入、GC、Binder）串行在一个线程上，任何一件慢事都直接吃帧。
- **残留**：软件渲染并未消失——今天的 HWUI 仍有软件回退路径；BerryOS 的 QEMU 软渲染路线与之同族（本地 `docs/GUI里程碑规划-2026-10-v1.0.md` §5 风险表把 TCG 吞吐列为第一风险，并引 OpenHarmony RASTER 纯软回退为同路证据）。

### 1.2 Honeycomb / HWUI（Android 3.0，2011）：GPU 加速 + display list

Romain Guy 官方博文《Android 3.0 Hardware Acceleration》给出了完整一手叙述（[android-developers.googleblog.com, 2011-03](https://android-developers.googleblog.com/2011/03/android-30-hardware-acceleration.html)；发布背景另见 [SiliconRepublic 报道](https://www.siliconrepublic.com/gear/google-previews-android-3-0-honeycomb-sdk-for-tablet)）：

- **新模型三阶段**：invalidate → 记录/更新 **display list** → 重放 display list。display list 只含绘制操作的输出（不含逻辑），未失效的 View 直接重放旧列表，不再重跑绘制代码。
- **解决的病**：软件模型「按钮 invalidate 拖着重绘底下整张 MapView」的连带重绘；动画可以只改 display list 属性（alpha/x/y/scale/rotation/pivot——这些属性由合成处理，**无需 invalidate、无需重绘**）。
- **代价与残留的病**（官方博文同页列明）：
  - 一批 Canvas/Paint 操作不支持（clipPath/drawPosText/setMaskFilter 等），行为差异长期困扰开发者；
  - overdraw 成了 GPU 时代新病：官方经验法则「每帧绘制的像素不超过屏幕像素数 2.5 倍」；
  - draw 方法里创建 Paint/Path 对象会加重 GC 且绕过缓存——**分配 churn 第一次被官方点名与渲染性能挂钩**；
  - `setAlpha()` 强制离屏缓冲，「doubles the required fill-rate」——填充率翻倍（官方原话）。
- **Adam Powell（View 系统作者之一）在 Google I/O 2013《Writing Custom Views for Android》延续此叙事**：自定义 View 是性能调优的最后手段，measure/layout 传递成本随层级增长（[confpad.io 讲稿](https://confpad.io/conferences/google-i-o-2013/writing-custom-views-for-android)，确定度中）。

### 1.3 Project Butter / Jelly Bean（Android 4.1，2012）：vsync + Choreographer + 三缓冲

一手来源 = Chet Haase & Romain Guy 的 Google I/O 2012 演讲《For Butter or Worse: Smoothing Out Performance in Android UIs》。本调研采用了 changliu.cc 的详细笔记（[changliu.cc, 2020](https://changliu.cc/2020/07/15/For-Butter-or-Worse-Smoothing-Out-Performance-in-Android-UIs)）+ Android Police 的现场报道（[androidpolice.com, 2012-07](https://www.androidpolice.com/2012/07/12/getting-to-know-android-4-1-part-3-project-butter-how-it-works-and-what-it-added)）+ 官方 vsync 文档（[source.android.com VSync](https://source.android.com/docs/core/graphics?hl=zh-cn)）。

- **之前的病（演讲原话级细节）**：
  1. ICS 其实已有 vsync，但只在最低层（SurfaceFlinger post buffer 时防撕裂）；「系统中其他一切都不按 vsync 走」——绘制在帧内任意随机时刻开始。若 CPU 更新 display list 拖到帧尾，GPU 完成后已错过 vsync，同一 buffer 连显两帧 = 用户看到一次 hiccup。**即使你的 draw 只需 <1ms，跨过 vsync 边界就 jank，「你无能为力，所以我们替你修了」**。
  2. 双缓冲下 CPU 绘制、GPU 执行、显示合成三个环节抢两个 buffer，只能两者并行、第三个干等；某帧超时会连锁再 jank 一次。
  3. **输入管线病**：旧系统把输入事件异步攒批（batch）后打包给一个 dispatching 操作，事件 A 和 C 的结果要等第一次 draw 结束才可见；大量输入事件会让 dispatcher「越落越远」（雪崩）。ListView 滚动的坏案例大量时间耗在 `deliverInputEvent`（getView 被其触发）——4ms 的绘制不是瓶颈，输入分发才是。
- **三件套**：
  - **VSync 扩展到全系统**：动画、滚动、fling、触摸分发全部对齐显示节拍，目标是「在一帧内给自己尽可能多的时间」；
  - **Choreographer**（新 API）：保证所有人跑在同一 vsync 脉冲上（`postInvalidateOnAnimation`/`postOnAnimation`/`postFrameCallback`）；
  - **惰性三缓冲**：「需要时才在」（lazy），兼得双缓冲的低延迟与三缓冲的稳定性——首帧超时仍会 jank 一次，此后 CPU/GPU/显示三者并行恢复稳定。Romain 吐槽：「为了降输入延迟我们移除了一个 buffer，为了帧率稳定又加了一个」。
  - 输入侧配套：取消打包分发中间层（处理时刻直接取最新事件）+ **输入位置预测/外推**（目标时刻无事件则按轨迹外推）。
- **量化锚点（演讲内）**：60FPS=16ms 帧预算；当时最好设备触摸延迟「约 5 帧」；500ms 动画中一次 4ms 的 GC 停顿就可能把帧推过 16ms 界；display list properties 优化对 launcher 动画每帧省约 2ms（"Every millisecond counts"）。
- **残留的病**：vsync 对齐让「错过 vsync」从随机事件变成硬性 16ms 悬崖——官方话术变成「if you're missing frames, it's your fault」；三缓冲多一帧排队延迟；主线程仍然是单点（measure/layout/draw/输入全在一根线程上排队）。

### 1.4 KitKat（Android 4.4，2013）：低内存优化与减分配叙事

- 确定度**高**的部分：**Project Svelte**——把系统内存占用压到 512MB RAM 可跑，加入 `ActivityManager.isLowRamDevice()` 低内存配置（[Wikipedia: Android KitKat](https://en.wikipedia.org/wiki/Android_KitKat)、[GSMArena Flashback](https://www.gsmarena.com/flashback_android_44_kitkat_optimized_the_os_for_phones_with_just_512mb_of_ram-news-52805.php)、[Ars Technica Android 8.0 评审的回溯](https://arstechnica.com/gadgets/2017/09/android-8-0-oreo-thoroughly-reviewed)）。
- 确定度**中低**的部分：任务书所述「KitKat 去隐式分配」——本次调研**未找到 Google 官方一手文档**以此命名的工作项。可对位的事实是：`android.util.ArrayMap/SparseArray` 容器族在 KitKat 前后被官方大力推广为 HashMap 的低分配替代（官方 DevBytes 叙事），且框架侧确有「减少每帧隐式分配」的长期工程（见 1.5 Lollipop 的动画对象池）。**列为开放问题，不作为硬结论引用**。
- 残留的病：Dalvik 的 STW GC 仍是 jank 主因（见 §2.1），直到 5.0 换 ART 才结构性缓解。

### 1.5 Lollipop（Android 5.0，2014）：RenderThread + RenderNode

- **架构**：渲染工作从主线程剥离到专用 **RenderThread**；display list 升格为 **RenderNode** 树（保留模式、属性可独立更新）；动画中失效只标脏，重录与提交由 RenderThread 异步做。Lollipop SDK 官方公告确认 Material Design 动效体系建立在此能力上（[android-developers.googleblog.com, 2014-10](https://android-developers.googleblog.com/2014/10/android-50-lollipop-sdk-and-nexus.html)；ripple 波纹难以回移植到旧版正是 RenderThread 能力差异的旁证——[Stack Overflow](https://stackoverflow.com/questions/33872488/why-creating-ripple-effect-is-difficult-on-pre-lollipop-android-devices)）。
- **解决的病**：主线程在 UI 状态同步后即可返回继续处理输入，GPU 提交、display list 重放不再占用主线程帧预算；窗口动画（旋转、活动转场）由系统侧 RenderThread 驱动，**应用进程再忙也不拖累系统动画**。
- **残留的病**：主线程上的 measure/layout、资源解码、Binder 调用依旧在帧关键路径上；GC 停顿依旧（同年 ART 上位才缓解，见 §2.1）；RenderThread 自身的 syncFrameState 卡顿成为新瓶颈类型（Perfetto 分析系列的常规检查项，[androidperformance.com](https://androidperformance.com/en/2026/05/04/Android-Perfetto-18-Input-Response-Latency)）。

### 1.6 Nougat → HWUI Vulkan/Skia（Android 7.0–11，2016–2020）

- 确定度**高**：Android 7.0 Nougat 引入 Vulkan API 作为 OpenGL ES 之外的图形 API，给开发者更多 GPU 控制权（[XDA Developers, 2016-02](https://www.xda-developers.com/vulkan-api-means-more-control-but-not-outright-replacement-of-opengl/)；[Wccftech, 2015-08](https://wccftech.com/android-vulkan-api-for-developers-equals-more-gpu-power/)）。
- 确定度**中**：HWUI 自身其后迁到 Skia 后端（O/P 时代），Android 9/10 起实验性 Skia Vulkan 管线逐设备启用，但 OpenGL ES 长期保持默认——本次调研未拿到官方 rollout 文档，时间线按社区与 AOSP 线索综合（[Android 图形 API 内部 wiki](https://github.com/Gracker/android-internals-wiki/blob/master/src/part1-fundamentals/ch02-rendering/07-gpu-rendering-graphics-api.md)、[Stack Overflow: Android, Skia, and hardware accelerated graphics](https://stackoverflow.com/questions/21464693/android-skia-and-hardware-accelerated-graphics)）。
- **解决的病**：驱动模型更现代（显式同步、更可预测的提交）；Skia 统一 2D 栈降低维护双管线的成本。
- **残留的病**：UI 框架层（View 树遍历、主线程模型）不在本层——Vulkan 不治 jank 的框架根因；碎片化（驱动质量参差）成为新病（[OrganicMaps issue #2946](https://github.com/organicmaps/organicmaps/issues/2946) 等）。

### 1.7 可观测性代：FrameMetrics → JankStats → FrameTimeline（Android 7–12+）

- **FrameMetrics（API 24）**：逐帧分阶段耗时（含 CPU/GPU 拆分）；**JankStats（androidx.metrics）在其上加判定与状态标注**——默认判定「渲染时长达到当前刷新率帧时长 2 倍」才算 jank（缓冲一档防误报），`FrameDataApi24` 增加 CPU 侧耗时、`FrameDataApi31` 增加帧超时量 `frameOverrunNanos`（[developer.android.com JankStats 文档](https://developer.android.com/topic/performance/jankstats)，官方一手）。
- **Perfetto FrameTimeline（Android 11/12+）**：把「帧实际呈现时刻 vs 期望 vsync 槽位」做成系统级事实源——**一帧可以「不慢」但「晚到」（presented late），照样抬高输入延迟**；这是 jank 概念从「帧耗时」到「帧时序偏移」的关键深化（[perfetto.dev FrameTimeline](https://perfetto.dev/docs/data-sources/frametimeline)）。
- **意义**：Android 花了整整一代把「响应慢」从主观感受变成可量化指标（slow frames >16ms、frozen frames >700ms、jank率、input-to-present 延迟——阈值综述见 [Sentry 讨论与 Android vitals 语境](https://github.com/getsentry/sentry-react-native/issues/3291)）。**度量体系本身就是渲染架构的一部分**——这对 BerryOS 的「AI 测试友好化」支柱是直接背书。

### 1.8 Jetpack Compose（2021+）：重组模型 → Strong Skipping（2024）

- **模型**：声明式 + 状态驱动重组。运行时用 Slot Table（紧凑双数组存 UI 树）+ 位置记忆化（positional memoization）做「外科手术式」最小重组（Leland Richardson 本人在 Kotlin slack 的解释，[slack-chats.kotlinlang.org, 2020-02](https://slack-chats.kotlinlang.org/t/514716/)；技术综述见 [Stitch Fix 工程博客](https://multithreaded.stitchfix.com/blog/2022/08/05/jetpack-compose-recomposition/)）。
- **官方现状**：Compose 1.9.0 起 Pokedex hero benchmark 中滚动 jank 率与 View 体系持平（[developer.android.com Compose performance 官方页](https://developer.android.com/develop/ui/compose/performance)，一手）；**Strong Skipping 在 Compose Compiler 2.0+ 成为默认**——lambda 即使捕获不稳定值也会被记忆化（[developer.android.com Strong Skipping](https://developer.android.com/develop/ui/compose/performance/stability/strongskipping)，一手）。
- 详见 §5。

### 1.9 演进总表

| 代际 | 年份 | 解决的病 | 仍残留的病 |
|---|---|---|---|
| 软件绘制 1.0–2.3 | 2008–10 | —（起点） | 无 GPU、整树重绘、主线程单点、GC、无 vsync |
| HWUI 3.0 | 2011 | 整树重绘（display list）、GPU 化 | overdraw、每帧分配、主线程单点、无 vsync 对齐 |
| Butter 4.1 | 2012 | 随机绘制起点、缓冲饥饿、输入攒批雪崩 | 16ms 悬崖、主线程单点、GC 停顿、overdraw |
| KitKat 4.4 | 2013 | 内存占用（Svelte）、低配档 | Dalvik STW GC 仍在 |
| Lollipop 5.0 | 2014 | 渲染剥离主线程（RenderThread/RenderNode）、ART 并发 GC | measure/layout/资源/Binder 仍在主线程、双遍 measure |
| Nougat→Skia/Vulkan 7–11 | 2016–20 | 驱动模型、双管线统一 | 框架层根因不在此层；驱动碎片化 |
| 可观测性 7–12+ | 2016–22 | jank 从感受到指标 | 度量≠治愈 |
| Compose 1.0→2.0 | 2021–24 | 命令式 invalidate 模型、最小重组、lambda 记忆化 | 不稳定参数、backwards write、JIT 依赖、GC 仍在（JVM） |

---

## 2. 响应慢根因全清单（机制 + 量化影响 + day-one 预防）

### 2.1 Java GC 停顿与每帧分配 churn

- **机制**：Dalvik 时代 GC 是保守式 stop-the-world：分配打满堆即暂停全部 VM 线程（[proandroiddev GC 演化史](https://proandroiddev.com/collecting-the-garbage-a-brief-history-of-gc-over-android-versions-f7f5583e433c)、[GeeksforGeeks](https://www.geeksforgeeks.org/android/how-garbage-collector-works-in-android)）。ART（5.0+）改为并发分代 + sticky CMS 年轻代收集，压实（compaction）推迟到后台（[source.android.com Debug ART GC 官方文档](https://source.android.com/docs/core/runtime/gc-debug)，一手；[策略解析](https://sonique6784.medium.com/arts-garbage-collector-strategies-for-best-performance-1f5c79208851)）。
- **量化**：For Butter or Worse 实测案例——**500ms 动画中一次 4ms GC 停顿就能把帧推过 16ms 界**；Google 框架团队曾用 Allocation Tracker 在自家框架里抓到「每帧几十个异常对象」的 bug（[changliu.cc 笔记](https://changliu.cc/2020/07/15/For-Butter-or-Worse-Smoothing-Out-Performance-in-Android-UIs)）。Romain Guy 2011 官方博文同样点名「draw 方法里创建 Paint/Path 加重 GC」（[官方博客](https://android-developers.googleblog.com/2011/03/android-30-hardware-acceleration.html)）。Colt McAnlis 的官方《Android Performance Patterns》视频系列有整集讲 Memory Churn 与 GC 代价（[系列索引](https://billynyh.github.io/tech-talks/google-android-performance-1.html)、[要点文](https://medium.com/android-news/android-performance-patterns-rescue-tips-8c1e4c7cb1f0)）。
- **列表滚动的经典案例**：滚动 = 每秒几十次 getView/onBindViewHolder，每次产生临时对象 → 分配速率打满 → 年轻代 GC 频率抬升 → 停顿落在帧中间 → 掉帧（综述见 [HackerNoon GC explainer](https://hackernoon.com/android-gc-explained-a-short-read)、[LinkedIn 讨论汇](https://www.linkedin.com/posts/premjitdevelops_androiddev-garbagecollection-kotlin-activity-7324840056534245376-8oHY)）。
- **BerryOS day-one 免疫**：全 Rust、无 GC——GUI 热路径（帧循环/布局/渲染/输入处理）零运行时分配即可写成（栈 + 预分配池 + `&mut` 借用），分配只发生在非热路径（图标缓存条目装入等）。**但注意：Rust 只消灭了 GC 停顿这一种病，「每帧堆分配→malloc 抖动/缺页」仍需纪律约束**——把「帧循环内零分配」写成 GUI-3 的验收断言（可复用 BENCHCHURN 泄漏金丝雀的形式，本地 `CLAUDE.md` boot 标记段）。

### 2.2 主线程包办 measure/layout/draw/资源加载

- **机制**：Android 的主线程（UI 线程）串行承担输入分发 → 动画 → measure/layout（traversal）→ draw 录制 → 状态同步给 RenderThread。任何一段超时都吃掉整帧预算；资源解码（BitmapFactory）、磁盘读、JSON 解析如果也在这根线程上，就是直接掉帧。官方 ANR 文档把主线程阻塞 I/O 列为头号反模式（[developer.android.com: Find the unresponsive thread](https://developer.android.com/topic/performance/anrs/find-unresponsive-thread)，一手）。StrictMode 的推荐默认配置就是抓主线程磁盘/网络访问（[AOSP StrictMode 源码注释](https://android.googlesource.com/platform/frameworks/base/+/c3f21f3/core/java/android/os/StrictMode.java)）。
- **量化**：For Butter or Worst 案例里问题应用绘制只花 4ms，瓶颈是 `deliverInputEvent` 里 52ms 的 contacts content provider 查询（Traceview 抓到）（[changliu.cc](https://changliu.cc/2020/07/15/For-Butter-or-Worse-Smoothing-Out-Performance-in-Android-UIs)）；Perfetto 实战案例：主线程在 doFrame 前 Runnable 排队 21ms、处理 18ms、ACK 后到呈现还有 50ms——read-to-present 合计 76ms（[androidperformance.com Perfetto 18](https://androidperformance.com/en/2026/05/04/Android-Perfetto-18-Input-Response-Latency)）。
- **BerryOS day-one 免疫**：
  1. GUI 服务（launcher/未来各 GUI 进程）内部**帧循环线程与业务线程分离**（RenderThread 教训直接移植：Lollipop 用一代版本还的债，BerryOS 从第一刀就分）；
  2. 资源（图标/字体/壁纸）**启动期预解码 + LRU**（规划 §2 闭包件 3/4 已含——本地 `docs/GUI里程碑规划-2026-10-v1.0.md` §2）；
  3. **同步 IPC 不上 UI 线程**（见 2.5）；
  4. 单核 v1 下「分离线程」退化为「分离阶段」：帧循环内固定顺序 input→animation→layout→render，各阶段设预算断言（与 androidperformance.com 的 Perfetto 检查顺序同构）。

### 2.3 View 树双遍 measure 与深层级

- **机制**：View 系统 measure/layout 协商协议允许容器多次测量子树：**RelativeLayout 天生两遍**（先量一遍再按依赖关系重排）；**LinearLayout 带 layout_weight 时两遍**（第一遍得出剩余空间才能分权重）；FrameLayout 一遍（[android-cn issue #386 讨论](https://github.com/android-cn/android-discuss/issues/386)、[#PerfMatters 自定义 ViewGroup 系列](https://medium.com/android-news/perfmatters-introduction-to-custom-viewgroups-to-improve-performance-part-2-f14fbcd47c)）。层级越深，双遍的成本按指数放大。
- **量化**：嵌套 LinearLayout+weight 的深树在每次 layout pass 上花掉数毫秒是 2013–2016 年代 Google I/O 性能话题的常客（Adam Powell I/O 2013 的核心论点之一——自定义扁平容器省遍数，[confpad.io](https://confpad.io/conferences/google-i-o-2013/writing-custom-views-for-android)）。Compose 的解法是**单遍测量约束向下传**（子只能度量一次父给的约束——官方 phases 文档，[developer.android.com Compose phases](https://developer.android.com/develop/ui/compose/performance/phases)）。
- **BerryOS day-one 免疫**：berry-ui 的布局协议从第一天起**单遍约束传递**（父给约束→子一次返回尺寸），禁止「量两遍」协议；GridLayout/SwipeView 这类固定网格天然单遍（规划 §2 闭包件 11/12 的形态恰好都是单遍容器）。**这必须在 berry-ui 的 trait 设计里立法**（GUI-3 的 `Measure` 契约），而不是靠实现者自觉。

### 2.4 过度绘制 overdraw

- **机制**：每帧重复绘制被完全遮挡的像素（背景叠背景、列表项叠窗口底色）。GPU 时代成了填充率浪费与功耗黑洞。官方可视化工具按蓝/绿/粉/红标 1x–4x（开发者选项 Debug GPU overdraw，[dev.to 综述](https://dev.to/tohemt/overdraw-memory-leaks-anr-and-slow-rendering-in-android-apps-3njb)、[BrowserStack 指南](https://www.browserstack.com/docs/app-performance/app-performance-guides/android/ui-rendering-performance)）；官方经验法则「每帧绘制像素 ≤ 屏幕像素 ×2.5」（[Romain Guy 2011 官方博文](https://android-developers.googleblog.com/2011/03/android-30-hardware-acceleration.html)）；React Native 应用红区 4x overdraw 常见（[NativeScript issue](https://github.com/NativeScript/NativeScript/issues/3665)、[r/reactnative 讨论](https://www.reddit.com/r/reactnative/comments/1hi5te3/how_is_your_production_apps_gpu_overdraw/)）。
- **BerryOS day-one 免疫**：合成器视角天然免疫大部分 overdraw——脏区合成时按**不透明矩形遮挡剔除**（occlusion culling）：被不透明窗口/区域完全遮住的脏区不重绘。OpenHarmony ui_lite 的脏区合并/遮挡排除正是此路（本地 `docs/GUI里程碑规划-2026-10-v1.0.md` 附录 B L0「脏区刷新」条，源码证据 `frameworks/components/root_view.cpp`）；fbcon 蜂格渲染核已是矩形原语友好（本地 `user/berry-fbcon/src/render.rs`）。**建议在 berry-ui 的 Invalidate 合并算法里把「遮挡排除」列为与「矩形合并」同级的验收项。**

### 2.5 Binder IPC 阻塞

- **机制**：Android 的跨进程调用（拿窗口信息、资源、content provider、启动 Activity）默认同步阻塞直到对端返回；主线程上的同步 `Binder.transact()` 会挂起 UI（[Stack Overflow: Binder.transact() can block UI](https://stackoverflow.com/questions/15051569/android-os-binder-transact-can-block-ui)、[Medium: How Binder Decides Whether Your App Crashes](https://medium.com/@sivavishnu0705/the-hidden-toll-booth-how-binder-decides-whether-your-app-crashes-stutters-or-freezes-75cacb1246de)）。经典案例：For Butter or Worse 里 getView 中的 52ms contacts 查询就是主线程 Binder（content provider）调用。
- **BerryOS 对位**：BerryOS 的同步 channel 语义上与 Binder 同族——**同步等待 IPC 是同一类病，换了传输层不会自愈**。预防立法：
  1. GUI 进程的帧循环线程**永不发起同步 RPC**——所有跨服务取数（窗口信息、图标枚举、事件流）走订阅/推送或 wait_many 多路复用（本地 IPC 设计：channel 同步 + event 异步，`docs/IPC设计-v1.0.md` 在册）；
  2. gui-inputd→launcher 的事件流用「发送方不等待渲染方」的方向（输入服务永不阻塞在慢消费者上——OpenHarmony vsync 分发「高频小信号走专用通道」同理，规划附录 B L0 VSync 条）；
  3. smoke/DFX 面加一条「UI 线程 RPC 等待」遥测（对标 Perfetto 的 dispatch_latency_dur 字段思想）。

### 2.6 掉帧（jank）机制的三分类：input / animation / traversal

- Choreographer 把每帧的回调分类型排队：CALLBACK_INPUT → CALLBACK_ANIMATION → CALLBACK_TRAVERSAL（→ Lollipop 后加 COMMIT）。掉帧按此归因：**输入处理慢**（dispatch/handling 超预算）、**动画回调慢**（动画值计算/业务逻辑）、**traversal 慢**（measure/layout/draw）——Perfetto 的 doFrame 分析按此顺序检查（[androidperformance.com](https://androidperformance.com/en/2026/05/04/Android-Perfetto-18-Input-Response-Latency)、[codetheory.in 输入系统](https://codetheory.in/understanding-android-touch-event-system-framework/)）。
- **关键洞见（FrameTimeline 带来的）**：jank 不只是「帧耗时>16ms」——**「不慢但晚到」的帧同样伤输入延迟**（expected vs actual present 偏移，[perfetto.dev](https://perfetto.dev/docs/data-sources/frametimeline)）。JankStats 官方判定取「2 倍帧时长」也是承认时序噪声（[developer.android.com](https://developer.android.com/topic/performance/jankstats)）。
- **BerryOS day-one 预防**：软 vsync 帧循环从第一版就把回调按 input→animation→layout→render 固定顺序分级（规划 GUI-3「Measure→Render 两遍 + 软 vsync 帧循环」天然对位）；**遥测环记两级指标：帧耗时直方图 + 帧呈现时刻偏移（present offset）**——后者在 QEMU 无真 vsync 下以软 vsync 时钟为锚即可自测（复用 BENCHWAKE 直方图先例，本地 `CLAUDE.md` boot 速览段）。

### 2.7 触摸事件分发开销

- **机制（历史）**：pre-JB 攒批分发雪崩（§1.3）；JB 后 InputReader→InputDispatcher→app 输入通道→View 树 `dispatchTouchEvent` 逐层派发。View 层每次 onTouchEvent 遍历命中路径；深层级+复杂命中测试=每事件毫秒级开销（[utzcoz AOSP 输入架构分析](https://utzcoz.github.io/)、[aospinsider.com](https://aospinsider.com/)、[官方 InputDispatcher 源码（含触控延迟统计上报 statsd）](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/input/InputDispatcher.cpp)）。
- **量化**：For Butter or Worse 的案例应用 `deliverInputEvent` 吃掉帧预算大头（getView 由其触发）；Perfetto 示例 handling 18ms。
- **BerryOS day-one 预防**：
  1. gui-wmd 的命中测试 = **Z 序表坐标查找**（规划 §3 两层窗 + Z 序小表），不做逐 View 深度遍历；
  2. PointerEvent 单结构携全状态（OpenHarmony MMI 同构，规划附录 B L1），输入服务→GUI 进程一跳直达，无 system_server 式中心枢纽排队（Android 的 InputDispatcher 在 system_server 单点，是历史包袱）；
  3. MOVE 重采样/预测留缝（OpenHarmony event_resample 与 Android stylus motion prediction 两方先例——[developer.android.com Stylus Low Latency](https://medium.com/androiddevelopers/stylus-low-latency-d4a140a9c982)）。

### 2.8 输入-显示端到端延迟的组成（结构性根因汇总）

Perfetto 实战给出的分解框架（一手技术文档，[androidperformance.com](https://androidperformance.com/en/2026/05/04/Android-Perfetto-18-Input-Response-Latency)）：

```
触摸硬件/IC（不在 trace 内）
→ InputReader 读取（read_time 起点）
→ InputDispatcher 排队+发送（read_to_dispatch）
→ 应用接收（dispatch_latency）
→ 应用处理+ACK（handling/ack）
→ 请求下一帧 → Choreographer doFrame
→ RenderThread 提交 buffer
→ SurfaceFlinger 合成 → present（终点）
```

示例样本分解：read→dispatch 3ms、dispatch 4ms、handling 18ms、ack 1ms、ACK 后到呈现 50ms——**大头在应用侧帧循环排队与呈现等待，不在系统分发**。BerryOS 的启示：**先量再优化**；把这条链路做成 BerryOS GUI 的标准延迟分解遥测（每段一个时间戳），是「吸取教训」最实在的落法。

---

## 3. 触摸/响应延迟公开实测

| 年份 | 来源 | 数据 | 确定度 |
|---|---|---|---|
| 2013 | Agawi TouchMark（Min App Response Time，RoboTouch 机械指 + 高速摄像） | iPhone 5 ≈ **55ms**；最快 Android（Galaxy S4）≈ **85ms**；Lumia 928 117ms、Moto X ≈123ms——iPhone 比最快 Android 快 ~2.5x（[MacRumors 报道](https://www.macrumors.com/2013/09/21/iphone-5-touch-screen-twice-as-fast-as-android-touch-screen/)、[iMore](https://www.imore.com/iphone-5-touchscreen-latency-measured-found-be-25-times-faster-closest-android-device)、[TheNextWeb](https://thenextweb.com/news/touchscreen-response-test-finds-iphone-5-2x-faster-than-android-devices)、[Agawi 团队自述方法论](https://medium.com/@djplayerapp/behind-the-touchscreen-latency-benchmark-by-agawi-d8feca9b8a49)） | 中高（商业基准，方法公开） |
| 2012 | For Butter or Worse 演讲 | JB 时代最好设备触摸延迟「约 5 帧」（≈80ms@60Hz），并以「帧数」为延迟度量单位（[changliu.cc](https://changliu.cc/2020/07/15/For-Butter-or-Worse-Smoothing-Out-Performance-in-Android-UIs)） | 高（一手演讲） |
| ~2017 | Reddit r/Android 二手转述 | Android 7.0 输入延迟可达 ~48ms，图形栈重做后 Pixel 降到 ~28ms（[讨论串](https://www.reddit.com/r/Android/comments/7teiwe/lets_discuss_touch_latencyaccuracy_on_flagships/)） | 低（未见一手测量报告，仅方向性参考） |
| 2026 | Perfetto 实战（trace 分析样本，非基准） | 示例 read-to-present 76ms；各段分解见 §2.8（[androidperformance.com](https://androidperformance.com/en/2026/05/04/Android-Perfetto-18-Input-Response-Latency)） | 高（方法学，非横向基准） |

**架构归因（为何 iOS 长期快）**：多方综述指向三点——①iOS 触摸事件与 UI 渲染跑在**专用高（实时）优先级线程**，触摸发生时调度器优先服务 UI；②GPU 加速是第一原则（从未有软件渲染时代）；③硬件/固件校准（触控 IC 采样与合并策略）（[PhoneArena: myth busted](https://www.phonearena.com/news/Android-can-never-be-as-smooth-as-iOS-myth-busted_id26252)、[Zhihu 讨论汇](https://www.zhihu.com/question/24180748/answer/24746921)，确定度中——非官方文档，属合理共识）。

**对 BerryOS**：第 ① 条恰是 BerryOS 的机制本位优势——intent 第一类调度输入（`CLAUDE.md` 调度 intent 段，ADR #42）+ 调度器 class_rt/deadline 预留（`docs/调度器设计-v1.1.md`）让「GUI 帧循环线程申报高 intent/RT 档」成为 day-one 可用能力，不靠事后打补丁。

---

## 4. Google 官方总结与知名工程师结论

| 来源 | 角色 | 核心结论（一句） | 引用 |
|---|---|---|---|
| Romain Guy《Android 3.0 Hardware Acceleration》 | Android 框架工程师（View/图形） | display list 模型 + overdraw ≤2.5x 屏幕像素 + draw 里别建对象 + alpha 双倍填充率 | [官方博客 2011](https://android-developers.googleblog.com/2011/03/android-30-hardware-acceleration.html) |
| Chet Haase & Romain Guy《For Butter or Worse》I/O 2012 | UI toolkit / 图形负责人 | vsync 全系统对齐 + Choreographer 统一节拍 + 惰性三缓冲 + 输入不攒批 + 预测外推；「错过 vsync 你无能为力，我们替你修了」 | [changliu.cc 笔记](https://changliu.cc/2020/07/15/For-Butter-or-Worse-Smoothing-Out-Performance-in-Android-UIs)、[AndroidPolice](https://www.androidpolice.com/2012/07/12/getting-to-know-android-4-1-part-3-project-butter-how-it-works-and-what-it-added) |
| Adam Powell《Writing Custom Views for Android》I/O 2013 | View 系统作者之一 | 层级深度与测量遍数是性能主变量；扁平化与自定义容器是解 | [confpad.io](https://confpad.io/conferences/google-i-o-2013/writing-custom-views-for-android) |
| Colt McAnlis《Android Performance Patterns》官方视频系列 | Google 开发者关系（性能） | 16ms 预算教育 + memory churn/GC 专题 + overdraw/vsync 专题 | [系列索引](https://billynyh.github.io/tech-talks/google-android-performance-1.html)、[Medium 要点](https://medium.com/android-news/android-performance-patterns-rescue-tips-8c1e4c7cb1f0) |
| 官方 JankStats 文档 | AndroidX | jank = 2 倍帧时长判定 + FrameMetrics 三代分层 + 状态标注 | [developer.android.com](https://developer.android.com/topic/performance/jankstats) |
| 官方图形架构文档 | AOSP | 一切绘到 surface；SurfaceFlinger 唯一合成者；BufferQueue 三模式；HWC 省功耗 | [source.android.com](https://source.android.com/docs/core/graphics) |
| Perfetto FrameTimeline | AOSP/Perfetto 团队 | 「不慢但晚到」也算延迟；expected/actual present 双轨 | [perfetto.dev](https://perfetto.dev/docs/data-sources/frametimeline) |
| 官方 Stylus Low Latency / motion prediction | Android Developers | 输入预测是压感知延迟的官方正路 | [medium.com/androiddevelopers](https://medium.com/androiddevelopers/stylus-low-latency-d4a140a9c982) |
| What's new in Android Performance I/O 2023 | 官方 | 性能治理转向 Baseline Profiles/JIT 预热与持续基准 | [medium.com/androiddevelopers](https://medium.com/androiddevelopers/whats-new-in-android-performance-google-i-o-2023-edition-21a1832c9fa3) |

---

## 5. Jetpack Compose：解决了什么、没解决什么

### 5.1 解决了什么

1. **命令式 invalidate 漂移**：View 系统的失效靠开发者手动 `invalidate()`，漏调=显示旧内容（display list 时代还放大了此坑——Romain Guy 2011 官方博文明示）；Compose 由状态读取自动驱动重组，模型上消灭「忘失效」这一类 bug。
2. **整树重跑的默认代价**：slot table + 位置记忆化让重组最小化（[Leland Richardson slack 解释](https://slack-chats.kotlinlang.org/t/514716/)、[Stitch Fix](https://multithreaded.stitchfix.com/blog/2022/08/05/jetpack-compose-recomposition/)）；官方 hero benchmark：Compose 1.9.0 滚动 jank 率与 View 体系持平（[官方](https://developer.android.com/develop/ui/compose/performance)）。
3. **lambda 每次重组新分配**：Strong Skipping（Compose Compiler 2.0+ 默认）把捕获不稳定值的 lambda 也记忆化——此前这是重组放大与分配 churn 的双重来源（[官方 Strong Skipping 文档](https://developer.android.com/develop/ui/compose/performance/stability/strongskipping)）。
4. **测量双遍协议**：Compose 布局单遍约束传递（子只量一次），从协议上废除 RelativeLayout 式双遍（[官方 phases](https://developer.android.com/develop/ui/compose/performance/phases)）。
5. **基线性能**：默认配置即带 baseline profile（JIT 预热缓解，[官方 baseline profiles](https://developer.android.com/develop/ui/compose/performance/baseline-profiles)）。

### 5.2 没解决什么 / 新引入的病

1. **不稳定参数重组**：编译器无法证明稳定（集合、第三方类、lambda 捕获链）→ skip 失效 → 大范围重执行；实战案例：单个不稳定 lambda 拖垮整页（[Gojek 工程博客](https://medium.com/gojekengineering/the-silent-performance-killer-how-a-single-lambda-nearly-destroyed-my-compose-ui-8ffc76357766)、[r/androiddev](https://www.reddit.com/r/androiddev/comments/1an5emm/compose_unstable_lambda_parameters/)）。Strong Skipping 放宽了这一面（不稳定参数相等性比较后也可跳过），**但相等性比较本身有代价**（大集合 equals 不便宜），且**不保证免比较**——文档明示边界。
2. **backwards write**：composable 内写已读状态造成级联额外重组——官方专设一页讲这个反模式（[官方 backwards-write](https://developer.android.com/develop/ui/compose/performance/backwards-write)）。
3. **GC 并未消失**：Compose 跑在 ART/JVM 上，重组/记忆化/快照系统仍在分配（比 View 时代少得多，但 §2.1 的机制病根仍在——Stitch Fix 的 gotchas 与 slot tax 文均有量化讨论：[multithreaded.stitchfix.com](https://multithreaded.stitchfix.com/blog/2022/08/05/jetpack-compose-recomposition/)、[slot tax](https://medium.com/@sivavishnu0705/the-slot-tax-what-composes-remember-really-costs-and-when-to-leave-it-out-071af626291c)）。
4. **JIT/预热依赖**：冷启动与非预热路径的 JIT 编译开销仍是 Compose 性能叙事的一等公民（I/O 2023 整段都在讲 Baseline Profiles，[官方](https://medium.com/androiddevelopers/whats-new-in-android-performance-google-i-o-2023-edition-21a1832c9fa3)）。
5. **优化责任转移给开发者**：remember/derivedStateOf(1.13+ computedStateOf)/key/contentType/lambda 修饰符延迟读取……最优性能仍需逐条手工遵守（[官方 best practices](https://developer.android.com/develop/ui/compose/performance)）——声明式没有消灭性能工程，只是换了题型。

### 5.3 对 BerryOS 的直接启示

Compose 用五年时间把「响应式/声明式 UI」的性能坑填到与手工 View 持平——**贵在运行时的记忆化与跳过机制（编译器+运行时协同）**。BerryOS v1 用保留模式（Rust 手写视图树 + 手动 Invalidate，规划 §3 裁决 C）是正确的起步档；但应在 berry-ui 里**预留「状态驱动失效」的 API 形态**（状态变量绑定视图属性，变更自动落到 Invalidate 脏区），让未来声明式层（若有）不必改渲染核。**不需要 slot table**：无 GC 的 Rust + 保留模式视图树，本身就是「全部 skip」的极限形态——显示列表常驻 = Compose 记忆化想逼近的目标。

---

## 6. 汇总表：每条根因 → BerryOS day-one 免疫的架构决策

| # | Android 根因 | 机制一句话 | 量化锚点 | BerryOS day-one 对位（免疫决策） | 本地锚点 |
|---|---|---|---|---|---|
| 1 | GC 停顿 + 分配 churn | STW/并发 GC 停顿打断帧 | 4ms GC 停顿即毁 500ms 动画一帧（I/O 2012） | 全 Rust 无 GC；**帧循环内零分配立法**（预热 + 池 + 栈），smoke 断言 | `CLAUDE.md` 语言策略段 |
| 2 | 主线程包办 | 输入/动画/布局/绘制/资源一线串行 | 案例 52ms provider 查询吃帧；76ms read-to-present | 帧循环线程独立（v1 单核=固定阶段顺序）；资源启动期预解码 + LRU；UI 线程禁同步 IPC | 规划 §2 闭包 3/4、GUI-3 |
| 3 | 双遍 measure / 深层级 | RelativeLayout/weight 两遍测量，深度放大 | I/O 2013 扁平化叙事 | berry-ui **单遍约束协议立法**（trait 层钉死）；GridLayout/SwipeView 天然单遍 | 规划 GUI-3/GUI-6 |
| 4 | overdraw | 遮挡像素重复绘制 | 官方 ≤2.5x 屏幕像素 | 脏区合成 + 不透明遮挡剔除同列验收；矩形原语渲染核（fbcon 同族） | `user/berry-fbcon/src/render.rs` |
| 5 | Binder 同步阻塞 | 主线程同步跨进程调用挂起 | 52ms 案例；StrictMode 默认抓主线程 I/O | 同步 channel 同病同治：**UI 线程零同步 RPC**；输入/窗口信息走推送 + wait_many 多路 | `docs/IPC设计-v1.0.md` |
| 6 | 无 vsync 对齐（历史起点） | 绘制随机时刻开始，跨 vsync 即 jank | ICS→JB 叙事 | 软 vsync 帧循环 day-one（16ms 定时器起步），真 vsync 接缝留位 | 规划 GUI-3、风险表 ramfb 行 |
| 7 | 缓冲饥饿 | 双缓冲下三环节排队 | Butter 三缓冲叙事 | 双缓冲起步 + 「需要时第三缓冲」留缝（QEMU 撕裂容忍已裁决） | 规划 §5 风险表 |
| 8 | 输入攒批雪崩 | 事件打包延迟可见 + dispatcher 落后 | pre-JB 输入管线叙事 | gui-inputd 事件流直达消费进程、不攒批；处理时刻取最新态 | 规划 §3 服务图 |
| 9 | 触摸分发开销 | 逐层 View 遍历 + 中心枢纽排队 | deliverInputEvent 吃帧案例 | Z 序表坐标命中（无深度遍历）；无 system_server 单点 | 规划 GUI-5 |
| 10 | 「晚到帧」型延迟 | 帧不慢但呈现偏移 | FrameTimeline expected/actual | 遥测记双指标：帧耗时直方图 + 呈现偏移；BENCHWAKE 直方图先例 | `CLAUDE.md` boot 速览 |
| 11 | 度量后置 | jank 十年后才有官方指标体系 | JankStats/FrameTimeline 时间线 | FPS 统计 + 视图树 dump + 事件注入器 = GUI-9 第一梯队（AI 测试友好化支柱） | 规划 GUI-9、附录 B L2 DFX 条 |
| 12 | 优先级不保证 | UI 线程与后台公平竞争 | iOS 实时优先级线程对照 | intent 第一类调度输入 + class_rt/deadline 预留：GUI 帧线程申报高 intent；AI 调度面天然消费 | `docs/调度器设计-v1.1.md`、ADR #42 |
| 13 | 声明式层性能税（Compose 病） | 重组/不稳定参数/记忆化成本 | Strong Skipping 五年补课 | 保留模式 Rust 视图树 = 常驻显示列表（记忆化极限）；状态→Invalidate 绑定 API 留缝 | 规划裁决 C |
| 14 | JIT/预热依赖 | 冷路径解释执行 | Baseline Profiles 叙事 | AOT 编译（无 JIT 面）；spawn 预热（appd 先例） | `CLAUDE.md` G-eco 段 |
| 15 | 低功耗与流畅的对立 | 渲染全速=耗电 | Tickless vs 帧率治理（HGM/变帧率） | tickless day-one + GUI 空闲静默帧循环（无脏区不合成不唤醒）；PM 服务共管 | `CLAUDE.md` 调度/电源段 |

---

## 关键发现

1. **Android 渲染演进的主线是「时序对齐 + 剥离主线程 + 可量化」三件事**：vsync 全系统化（JB）、渲染剥离（Lollipop RenderThread）、度量体系化（JankStats/FrameTimeline）——三者都是 BerryOS 可以 day-one 预置的结构，不需要重走十年弯路。（来源：[For Butter or Worse 笔记](https://changliu.cc/2020/07/15/For-Butter-or-Worse-Smoothing-Out-Performance-in-Android-UIs)、[JankStats 文档](https://developer.android.com/topic/performance/jankstats)、[Perfetto FrameTimeline](https://perfetto.dev/docs/data-sources/frametimeline)）
2. **「帧耗时>16ms 才算 jank」是过时定义**：Android 11/12 的 FrameTimeline 确立「不慢但晚到（presented late）」同样抬高输入延迟——BerryOS 遥测从第一版就应记呈现偏移，不只记帧耗时。（来源：[perfetto.dev](https://perfetto.dev/docs/data-sources/frametimeline)，确定度高）
3. **端到端延迟的大头在应用帧循环排队与呈现等待，不在系统分发**：Perfetto 样本中 read→dispatch 仅 3ms，而 ACK 后到呈现 50ms。（来源：[androidperformance.com](https://androidperformance.com/en/2026/05/04/Android-Perfetto-18-Input-Response-Latency)，确定度高）
4. **2013 年公开实测 iPhone 5 触控延迟 ~55ms vs 最快 Android ~85ms（2.5x 差距）**，架构归因含「iOS 用实时优先级专用 UI 线程服务触摸」——BerryOS 的 intent/RT 调度机制恰可 day-one 对位。（来源：[MacRumors/Agawi](https://www.macrumors.com/2013/09/21/iphone-5-touch-screen-twice-as-fast-as-android-touch-screen/)，确定度中高；[PhoneArena](https://www.phonearena.com/news/Android-can-never-be-as-smooth-as-iOS-myth-busted_id26252)，确定度中）
5. **一次 4ms 的 GC 停顿就足以毁掉一帧（16ms 预算的 1/4）**，Google 框架团队曾抓到自家框架每帧几十个分配的 bug——无 GC 是 BerryOS 对 GUI 最大的单项结构红利。（来源：[For Butter or Worse 笔记](https://changliu.cc/2020/07/15/For-Butter-or-Worse-Smoothing-Out-Performance-in-Android-UIs)，一手演讲转述）
6. **RelativeLayout/weight 双遍测量是协议病不是实现病**：Compose 的解法是单遍约束协议立法——berry-ui 的 Measure trait 必须第一天就钉死单遍。（来源：[android-cn #386](https://github.com/android-cn/android-discuss/issues/386)、[官方 phases](https://developer.android.com/develop/ui/compose/performance/phases)，确定度高）
7. **同步 IPC 阻塞主线程是 Binder 与 BerryOS channel 共有的病**：换微内核不自动免疫——需要「UI 线程零同步 RPC」的 API 纪律与多路等待。（来源：[Stack Overflow](https://stackoverflow.com/questions/15051569/android-os-binder-transact-can-block-ui)、[官方 ANR 文档](https://developer.android.com/topic/performance/anrs/find-unresponsive-thread)）
8. **Compose 用 Strong Skipping（2024 默认）才把声明式性能追平手工 View**（官方 Pokedex 基准 1.9.0 持平）——声明式不是免费午餐；Rust 保留模式 + 常驻显示列表天然是「全 skip」形态。（来源：[官方 Strong Skipping](https://developer.android.com/develop/ui/compose/performance/stability/strongskipping)、[官方 performance 页](https://developer.android.com/develop/ui/compose/performance)，确定度高）
9. **输入攒批（batching）是 pre-JB 的结构性雪崩源**：处理时刻必须取最新事件态 + 预测外推留缝——gui-inputd 的事件流形态应直接照此设计。（来源：[For Butter or Worse](https://changliu.cc/2020/07/15/For-Butter-or-Worse-Smoothing-Out-Performance-in-Android-UIs)、[官方 stylus motion prediction](https://medium.com/androiddevelopers/stylus-low-latency-d4a140a9c982)）
10. **官方 overdraw 预算 = 每帧绘制像素 ≤ 屏幕像素 ×2.5**：脏区 + 不透明遮挡剔除应在 berry-ui 的 Invalidate 合并算法里同级验收。（来源：[Romain Guy 2011 官方博文](https://android-developers.googleblog.com/2011/03/android-30-hardware-acceleration.html)，确定度高）

## 对 BerryOS 的架构启示

1. **GUI-3 的 berry-ui 布局 trait 立法「单遍约束传递」**：Measure 契约禁止容器二次测量子树（RelativeLayout/weight 之戒）；GridLayout/SwipeView 均按单遍容器实现，host 测试断言「一次 invalidate 周期内每个 View 的 measure 恰被调用一次」。
2. **帧循环内零分配写成 smoke 断言**：复用 BENCHCHURN 泄漏金丝雀形式，GUI 帧线程堆分配计数跨相位恒定（Rust 无 GC 免除停顿，但 malloc/缺页抖动仍要防）。
3. **遥测双指标 day-one**：帧耗时直方图 + 帧呈现偏移（对软 vsync 锚点）——对齐 FrameTimeline 的「expected/actual」思想，落在 GUI-9 DFX 三件（FPS 统计升级为两级指标）；这也是 AI 测试友好化支柱的 GUI 展示位。
4. **UI 线程零同步 IPC 纪律**：guifbd/gui-inputd/gui-wmd 三薄服务的消费侧全部推送式 + wait_many 多路等待（本地 wait_many/wait_any 机制在册）；禁止帧循环内 channel 同步往返（「数据面推送、控制面异步」）。
5. **输入链路照 JB 教训设计**：gui-inputd 事件直达消费进程、不攒批；消费侧处理时刻取最新事件态；PointerEvent 结构留 MOVE 重采样与预测外推缝（OpenHarmony event_resample + Android motion prediction 双先例）。
6. **GUI 帧线程申报高 intent（0x01 INTERACTIVE 档）**：调度 intent 机制（ADR #42）从第一版就让 GUI 帧循环享受 iOS 式优先响应——这是「吸取安卓教训」最 BerryOS 原生的落法，也是 AI 原生调度对 GUI 的第一批真实消费面。
7. **脏区合成 + 不透明遮挡剔除同级实现**：berry-ui 的 Invalidate 合并算法把「被不透明区域完全遮挡的脏区不重绘」列为与矩形合并同级的验收项（overdraw ≤2.5x 预算的软渲染版）。
8. **保留模式视图树 = 常驻显示列表**：不做 Compose 式 slot table 记忆化（无 GC 的 Rust 保留模式天然全 skip）；但预留「状态变量→Invalidate」绑定 API 缝，供未来声明式层无痛接入。
9. **资源面启动期预解码 + LRU 限帽**（图标/字形/壁纸）：与 32MiB VMO 帽体系联动（规划 §5 内存风险行），避免运行期解码进入帧循环。
10. **输入-呈现延迟分解遥测**：read→dispatch→handle→present 各段一个时间戳进遥测环（对标 android_input_events 表字段设计），QEMU 环境即可自测；这是「吸取教训」的可验证形态，也是未来真机基准的锚点。

## 开放问题

1. **KitKat「去隐式分配」官方一手来源未找到**：本次调研只确认 Project Svelte（低内存优化）与 ArrayMap/SparseArray 容器推广；任务书所述「KitKat 去隐式分配」未检索到官方文档命名的工作项——按 Project Svelte 收录，隐式分配清理并入 Lollipop 前后的框架工程叙事（确定度中低）。
2. **HWUI Vulkan 后端默认启用的时间线未定案**：Android 7.0 引入 Vulkan API（确定），但 HWUI/Skia Vulkan 何时在哪些设备默认启用，未获官方 rollout 文档（社区线索指向 Android 9/10 实验性、逐设备启用、OpenGL ES 长期默认）。
3. **现代（120Hz/2020s）Android vs iOS 触控延迟缺乏严格公开基准**：Agawi 之后没有同等方法学的公开横向测量；Reddit 转述的「Pixel ~28ms」无一手出处。若需硬数据，可自建高速摄像测量（BerryOS 验收时可对模拟器窗口做同法测量）。
4. **QEMU ramfb 无真 vsync**：软 vsync 的呈现偏移指标以定时器为锚，与真机面板行为差距待真机阶段定标（规划 §5 已裁「撕裂容忍」，但延迟指标的可信度要注明环境）。
5. **TCG 软渲染吞吐是否足以支撑滑动动画**：800×600 RGB32 全屏 ≈1.9MB/帧，脏区 + RGB565 降载预案已在规划 kill criteria 内，首刀实测后回填本报告的量化表。
