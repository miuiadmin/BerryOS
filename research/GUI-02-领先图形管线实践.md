# GUI-02 领先图形管线实践——iOS Core Animation / Metal / ProMotion 剖析与 DWM、Chromium 对照的普适原则提炼

> **调研定位**：M-GUI 立项后的第二份 GUI 专项调研。第一手问题 = 「iOS 与领先商用系统的图形管线『优秀』在哪」——为 BerryOS 的显示/合成/输入/帧调度服务形态提炼可迁移的普适原则。与 `docs/GUI里程碑规划-2026-10.md`（OpenHarmony 功能面参照、三薄服务 + 单 GUI 进程的档位裁决）互补：那份答「做什么」，本报告答「为什么这样分工会优秀」。
>
> **方法与证据等级**：官方一手资料（Apple Tech Talks / Apple Developer Docs / Microsoft Learn / Chrome for Developers / chromium.googlesource）＞ 权威社区逆向与实证（EthanArbuckle 逆向文档、objc.io、fabernovel、Jacob's Tech Tavern、MoltenVK issue）＞ 论坛级佐证。关键论断均标【确定度：高/中/低】；逆向与推断混合处如实标注。iOS 内部机制无官方文档，凡涉 backboardd/RenderLoop 内部均属逆向共识。

---

## 0. 结论速览（一段话）

iOS 图形管线的「优秀」不在于某个单点技术，而在于**把「描述变更」与「执行渲染」在进程边界上硬切开**：应用只提交图层树变更（CATransaction，一次 run loop pass 一批），渲染合成在独立系统进程（render server，backboardd 承载）按 vsync 节拍流水执行，GPU 执行从不阻塞应用主线程；动画与滚动这类「纯合成可算」的效果整体下沉到合成侧，应用进程再卡也不冻结屏幕上的运动；触摸事件走专用进程 mach 直递、作为帧循环第一阶段最先处理，采样率与投递率解耦（coalesced/predicted）；帧率本身是系统仲裁的资源而非应用的权利（ProMotion：应用只给 hint）。Windows DWM（flip 共享缓冲 + DirectFlip/MPO 直扫阶梯）与 Chromium（cc 四树 + commit/activate + compositor thread 滚动）从另两个起点收敛到了同构原则。这组原则在单核、软渲染、微内核服务形态下依然成立——**流水错相的本质是 deadline 结构，不是多核并行**。

---

## 1. iOS Core Animation：把「画」从应用进程里拿走

### 1.1 三方分工：应用只改树，render server 才画

Apple 官方对 iOS 渲染循环的定义是三方结构（原文表述）：

> "The Render Loop is a continuous process by which touch events are handed to an app, and then changes to the UI are sent to the operating system where the frame is finalized."（[Apple Tech Talks 10855 "Explore UI animation hitches and the render loop"](https://developer.apple.com/videos/play/tech-talks/10855)）

三段为：① **应用进程**（处理事件、改 UI）；② **render server（独立进程）**——"This is where your UI is actually rendered"；③ **Display**（帧上屏）。【确定度：高，官方一手】

render server 的身份（逆向共识）：

- **iOS = `backboardd`，macOS = `WindowServer`**。EthanArbuckle 逆向文档："The Render Server (backboardd on iOS, WindowServer on macOS) is the ultimate authority on what appears on screen… leverages the GPU for compositing, transformations, and animations… manages the buffers that are ultimately scanned out to the display."（[EthanArbuckle/ios-rendering-docs](https://github.com/EthanArbuckle/ios-rendering-docs/blob/main/README.md)）
- 应用侧对应物是 **CAContext**："an independent rendering surface and layer tree managed by the system's Render Server"（通常一个 UIWindow 一个 context）；contextID 是 32 位整数令牌，"not itself a Mach port name, but can be mapped by Core Animation to the underlying Mach port"；FrontBoard 指示 render server 各 context 画什么、画在哪、Z 序与透明度（同上）
- 提交通道：**"Commits via Mach"**，像素共享走 **IOSurface（内核共享内存）**，同步用 fence（mach port / XPC 包装描述符）（同上）
- 演进注：早期 iOS 的渲染与事件路由在 SpringBoard 内，后拆出 backboardd；现行资料（2023 逆向文档、2026-05 的 Jacob's Tech Tavern）一致指认 backboardd。【确定度：中，逆向共识、无官方文档】

这个结构的本质：**应用从头到尾不触碰 framebuffer，也没有 present 调用**——应用唯一的输出是「树/属性变更描述」。GPU 合成模型（objc.io 经典描述）：每个 layer 是一张纹理，"The GPU has textures (bitmaps) that it composites together for each frame (i.e. 60 times a second)"；移动只是 "the CPU will simply tell the GPU what the new position is, so the GPU can reuse the existing texture"（[objc.io #3 "Getting Pixels onto the Screen"](https://www.objc.io/issues/3-views/moving-pixels-onto-the-screen/)）。

### 1.2 CATransaction：提交语义 = 变更描述，一次 run loop pass 合一批

- **隐式事务**：首个 layer 属性修改自动开启事务，run loop 即将休眠时提交。fabernovel 实证系列："CoreAnimation renders our app view hierarchy at each run loop pass and not at each view hierarchy modification… CoreAnimation finally commits this transaction just before the thread is put to sleep"，触发器是 **kCFRunLoopBeforeWaiting 观察者**（引 Rob Mayoff："CoreAnimation is an active observer of the run loop. It waits for the kCFRunLoopBeforeWaiting event"），崩栈证据显示 `layoutSubviews` 位于 `CA::Transaction::commit()` 之下、`__CFRUNLOOP_IS_CALLING_OUT_TO_AN_OBSERVER_CALLBACK_FUNCTION__` 之内（[fabernovel: UIKit rendering — The run loop](https://fabernovel.github.io/2021-01-04/uikit-rendering-part-3)）。SceneKit 的同类 API 文档亦确认「run loop 下次迭代时事务自动提交」语义（[SCNTransaction](https://developer.apple.com/documentation/scenekit/scntransaction)）。【确定度：高（官方语义）+ 中（触发细节属观察行为，作者自注 iOS 版本间有差异）】
- **显式事务**：`CATransaction.begin()/commit()` 分组、设时长、关隐式动画（[kCATransactionDisableActions](https://developer.apple.com/documentation/quartzcore/kcatransactiondisableactions)、[objc.io #12 "Animations Explained"](https://www.objc.io/issues/12-animations/animations-explained)）。
- **提交前四阶段**（WWDC 2014 session 419 口径，转引自 [Jacob's Tech Tavern](https://blog.jacobstechtavern.com/p/ui-pipeline-internals)）：**Layout**（父到子排布）→ **Display**（drawRect 软绘、文本渲染）→ **Prepare**（图像解码等杂务）→ **Commit**（整棵变更后的 layer 树发往 render server）。Tech Talks 10855 补充合批细节：多次 `setNeedsLayout`/`setNeedsDisplay` 会被系统 **coalesce** 到提交阶段一次做，"to reduce duplicate work"。
- **动画的提交物是参数不是帧**：timing curve（linear/spring/easeInOut/keyframe）+ 起止值随事务提交，逐帧插值发生在 render server——这是「应用卡顿不冻结动画」的机制根源（Tech Talks 10855 + objc.io #12）。Core Animation 最初即名为 "Layer Kit"，本质是 compositing engine 而非 drawing 库（[iOS Core Animation: Advanced Techniques](https://gist.github.com/rsattar/93e64ddde3d962922f91)）。
- 侧注：layer 树在应用侧有 model/presentation 两份视图、服务侧另有渲染树——presentation tree 是公开 API（`presentationLayer`），三树细节属公开常识但本调研未抓到一手深述，不作为关键论断。【确定度：中】

### 1.3 RenderLoop 五阶段、vsync 节拍与 hitch 度量

Tech Talks 10855 给出官方全貌（本节引文均出自该页）：

- 帧节拍："At the beginning of a frame, the hardware emits an event called a VSYNC… The Render Loop is timed to VSYNCs. It must hit checkpoints along the way in order to have a frame ready." 每阶段 deadline = 下一个 vsync。
- **五阶段**：① Event phase（app 处理触摸/定时器/网络回调，决定是否改 UI）→ ② Commit phase（Layout + Display，"the entire altered layer tree is collected and sent to the render server"）→ ③ Render prepare（render server 遍历 layer 树，"prepares a **linear pipeline** that the GPU can then execute"，父到子/兄弟到兄弟排成 back-to-front 线性序）→ ④ Render execute（"this linear pipeline is passed through the GPU, where each layer is **composited into a final texture**"）→ ⑤ Display（下一个 vsync 上屏）。
- **流水并行**："this entire process is actually happening in parallel every frame. This way, the pipeline becomes concurrent, and **our app can prepare a new frame while the system is rendering the previous one**."
- **双/三缓冲**："the frame is processed for two frames before display. We call this double buffering… the system may switch to triple buffering, where the render server is given one extra frame duration."
- **hitch 度量**：hitch = 帧晚于预期上屏；分 commit hitch（app 进程内）与 render hitch（server 侧）；指标 = **hitch time ratio**（hitch 毫秒/秒），"<5 认为良好基本无感，5–10 用户会察觉中断，>10 应立即优化"。
- **CADisplayLink**：应用侧的帧时钟——把回调注册进 run loop、与显示刷新对齐（[CADisplayLink 官方文档](https://developer.apple.com/documentation/quartzcore/cadisplaylink)）。注意 Tech Talks 10855 全程未提它——CADisplayLink 是「应用自愿对齐帧节拍」的口子，不是系统 render loop 本身。

### 1.4 为什么 UI 线程不等 GPU

结构性答案（综合 [Tech Talks 10855](https://developer.apple.com/videos/play/tech-talks/10855) + [objc.io #3](https://www.objc.io/issues/3-views/moving-pixels-onto-the-screen/) + [EthanArbuckle](https://github.com/EthanArbuckle/ios-rendering-docs/blob/main/README.md)）：

1. **提交即返回**：commit 只做 CPU 侧树序列化与 mach 投递，GPU 执行发生在另一进程、另一帧时段；应用主线程提交完立刻可处理下一个事件。
2. **渲染永不回压到应用逻辑**，唯一的背压点在缓冲池（见 §2.2 drawable 池）——池有空位应用就总能前进。
3. **内容与位置分离**：纹理上传一次，逐帧只改变换；"the fastest drawing is the drawing you don't do"（objc.io）。
4. 独立进程还有失败隔离红利：render server 与应用互不拖垮——与 BerryOS 微内核失败隔离原则（CLAUDE.md 微内核原则 5）同构。

### 1.5 合成直通 scanout：iOS 的证据边界

- render server "manages the buffers that are ultimately scanned out to the display"（[EthanArbuckle](https://github.com/EthanArbuckle/ios-rendering-docs/blob/main/README.md)）——合成结果进显示控制器扫描输出。【确定度：中】
- **iOS 没有公开的 per-layer overlay/直扫 API**（对照 Android HWC、Windows MPO）：苹果把「哪些层能直扫、哪些必须合成」的决策完全收进系统内部，应用面只有「提交树」。这是有意的 API 收窄——代价是灵活性、收益是系统对帧路径的完全仲裁（含 ProMotion）。【确定度：中，属「无公开文档」级论断】

---

## 2. Metal 与 Apple GPU（TBDR）：提交语义的硬件对偶

### 2.1 TBDR 设计哲学：带宽是功耗大头，把工作搬上片

- Apple GPU 为 TBDR（Tile-Based Deferred Rendering）架构：先把整个场景按 tile 分组，几何处理与片元着色都在片上 tile memory 进行，深度/混合不落显存（[WWDC20 session 10602 "Discover Metal on Apple GPUs"](https://developer.apple.com/videos/play/wwdc2020/10602)、[WWDC20 session 10632 "Optimize Metal Performance for Apple Silicon Macs"](https://developer.apple.com/videos/play/wwdc2020/10632)）。9to5Mac 对 10602 的转述："TBDR captures the entire scene before rendering, splitting it into small tiles processed on-chip"（[9to5mac](https://9to5mac.com/2020/07/08/arm-mac-graphics/)）。
- Metal API 与硬件协同的落点：render pass 的 **load/store action** 显式控制 tile memory 与显存间搬运（不清不存 = 零带宽）；**imageblock** 按 tile 显存分区（合法 tile 尺寸 32×32/32×16/16×16，[imageblockMemoryLength 文档](https://developer.apple.com/documentation/metal/mtlrenderpipelinestate/imageblockmemorylength(fordimensions:))）；memoryless 纹理（如 depth）只存在于片上（[WebGPU memoryless 讨论 #5396](https://github.com/gpuweb/gpuweb/issues/5396)）。
- 与 UI 管线的关系：**UI 合成恰好是「小纹理、多层叠加、每帧重复」的负载**——TBDR 的 tile 化合成天然省带宽省电；CA 的合成引擎即跑在 Metal 上（EthanArbuckle 架构图：server 侧 "Compositing Engine (Metal/GPU)"）。【确定度：高（架构官方）+ 中（对 UI 负载的适配判断为本报告推断）】

### 2.2 drawable 池 / present / vsync 对齐：背压内建于缓冲池

- `CAMetalLayer.maximumDrawableCount` 合法值 **2 或 3**（"the number of Metal drawables in the resource pool managed by Core Animation"，[maximumDrawableCount 文档](https://developer.apple.com/documentation/quartzcore/cametallayer/maximumdrawablecount)）；**池满时 `nextDrawable` 阻塞**——实证：MoltenVK issue "Metal often only supports 3 drawables total… drawable stalls occur at the nextDrawable retrieval stage — not at present time"（[MoltenVK #929](https://github.com/KhronosGroup/MoltenVK/issues/929)）；drawables 不归还则 `nextDrawable` 返回 nil（[CAMetalLayer 文档](https://developer.apple.com/documentation/quartzcore/cametallayer)）。
- 语义提炼：**「取缓冲阻塞」就是 vsync 节流的产品化形态**——三帧在飞（3-in-flight）即流水深度的硬件级定值，与 Tech Talks 10855 的「双缓冲，必要时切三缓冲」互为印证；`waitUntilScheduled` 提供提交与 GPU 排程之间的 CPU 侧同步点（[waitUntilScheduled 文档](https://developer.apple.com/documentation/metal/mtlcommandbuffer/waituntilscheduled())）。【确定度：高（池语义）+ 中（present 与 vsync 的精确相位表述）】

### 2.3 ProMotion：帧率是系统仲裁的资源，应用只给 hint

- iPhone 13 Pro 支持 **12 档刷新率**：120/80/60/48/40/30/24/20/16/15/12/10 Hz（8ms 至 100ms）；iPad Pro 仅 5 档（120/60/40/30/24）——手机档位更密、下探更低（10Hz），为功耗服务（[iClarified 转 Apple 开发者文档](https://www.iclarified.com/82663/apple-posts-developer-documentation-for-promotion-on-iphone-13-pro-12-refresh-rates-used)）。
- **仲裁机制**："Core Animation arbitrates all the animations it presents on the screen and determines the refresh rate at any particular time"；应用只能"provide hints to Core Animation about what refresh rates the app prefers"——即 `CADisplayLink.preferredFrameRateRange`（[API 文档](https://developer.apple.com/documentation/quartzcore/cadisplaylink/preferredframeraterange)），CAFrameRateRange 需满足 min ≤ preferred ≤ max（实证报错文案见 [jtrivedi/Wave #30](https://github.com/jtrivedi/Wave/issues/30)）。
- **默认保守**：iPhone 上应用必须声明 `CADisableMinimumFrameDurationOnPhone` Info.plist 键才能解锁 >60Hz——不开 plist，preferred 再高也被帽在 60（[iClarified](https://www.iclarified.com/82663/apple-posts-developer-documentation-for-promotion-on-iphone-13-pro-12-refresh-rates-used)）。这是功耗护栏：120 是要显式申请的特权，不是默认权利。
- 原则提炼：**帧率协商 = 应用报 hint + 系统统一仲裁 + 功耗护栏默认收紧**——硬件不驱动渲染节奏，渲染侧（CA）决定「本次需要几 Hz」，显示硬件跟随。

---

## 3. 事件优先：iOS 触摸为什么快

### 3.1 全路径：IOKit → backboardd → 目标应用主线程

综合 [Jacob's Tech Tavern "Touch to Pixels"](https://blog.jacobstechtavern.com/p/ui-pipeline-internals)、[The Apple Wiki: backboardd](https://theapplewiki.com/wiki/Dev:Backboardd)、[EthanArbuckle](https://github.com/EthanArbuckle/ios-rendering-docs/blob/main/README.md)：

1. 触控硬件（电容网格 + 微控制器扫描）→ xnu 内核 **IOKit 驱动**解码为 **IOHIDEvent**；
2. **backboardd**（BackBoardServices 桥接设备 IO 与用户态）生成 GSEvent，按 **CAContext contextID → mach port** 映射找到前台目标进程，**mach IPC 直递**（The Apple Wiki："BackBoard is the system component responsible for deciding which application should receive touch events arriving from IOKit"）；
3. 目标应用主线程 **run loop 被事件源唤醒**，UIKit 包装为 UIEvent → `sendEvent` → UIWindow **hit-test** → UIResponder `touchesBegan/Moved/...`；
4. 事件处理是 **Render Loop 的第一阶段**（Tech Talks 10855：Event phase 在 Commit phase 之前、有自己的 vsync deadline）——事件不是普通消息队列里的一个包，而是帧循环的时钟起点之一。

快的结构性原因：专用系统进程 + mach 直递（无全局总线轮询）+ 前台 context 路由表常驻 + 主线程唤醒即处理 + 事件阶段拥有帧预算最高优先级。backboardd 同时管触摸路由与渲染服务（§1.1）——**输入路由与显示合成天然同屋**，命中测试所需的窗口拓扑与渲染所需的 context 拓扑是同一份事实。【确定度：路径高（多源一致）；内部优先级数值细节中（逆向）】

### 3.2 coalesced 与 predicted touches：采样率/投递率/渲染率三解耦

- **coalesced**：硬件采样率（Apple Pencil / ProMotion 触控可达 240Hz）高于事件投递率（对齐显示刷新 60/120Hz）；UIKit 把一帧内的多个中间采样**合并进一个 UIEvent**，应用经 `event.coalescedTouches(for:)` 取回全部中间点——既保轨迹平滑又不迫使 UI 以 240Hz 重绘（[Apple: Implementing coalesced touch support in an app](https://developer.apple.com/documentation/uikit/implementing-coalesced-touch-support-in-an-app)）。
- **predicted**：`event.predictedTouches(for:)` 给出估计的未来触点，应用**提前一帧渲染**以压低感知时延（同上；Unity Apple Pencil 集成为实际消费者例证，[UnityApplePencil](https://github.com/all-iver/UnityApplePencil/blob/main/ApplePencil.cs)）。
- 原则提炼：**采样、投递、渲染三个频率各自独立**，聚合/预测在系统侧完成，展开与否的选择权交给应用。

---

## 4. 对照组一：Windows DWM——合成器统一 + 直扫优化阶梯

- **统一合成**：自 Vista 起 DWM 合成全部顶层窗口（背景常识）；现代形态的关键在呈现模型的阶梯。
- **DXGI flip 模型**：back buffer 与 DWM **共享**，"the DWM can compose straight from those back buffers without any additional copy operations"，而旧 bitblt 模型每次 Present 要先拷进 DWM 重定向表面再合成（省一次全窗读写）；窗口模式收益最大（[DXGI flip model, Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/dxgi-flip-model)）。
- **直扫阶梯**：composed flip → **DirectFlip**（缓冲尺寸在硬件缩放因子容差内即指定给硬件扫描平面，绕过 DWM 合成）→ **DirectFlip with MPO**（多平面叠加，窗口化甚至非全屏也能直扫）；PresentMon 类工具以 present mode "Hardware: Independent Flip" 观测之；窗口化 MPO 依赖驱动支持（[Microsoft Learn "For best performance, use DXGI flip model"](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/dxgi-flip-model) + [TechPowerUp/Guru3D MPO 讨论](https://www.techpowerup.com)）。【确定度：高（机制官方）+ 中（各代硬件支持矩阵）】
- **可观测可恢复的丢帧协议**：present statistics（`GetFrameStatistics` 给 PresentRefreshCount/SyncRefreshCount/SyncQPCTime）让应用对账「期望 vsync vs 实际 vsync」，错位即 glitch，恢复手段 = 跳帧 Present(0) ×N 或 `DXGI_PRESENT_RESTART` 清队列（[DXGI flip model](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/dxgi-flip-model)）——把丢帧从「感觉卡」变成机器可读事件。
- **官方承认的错相流水**：该文档的 present 时间线明确画的是——app 于 vsync N 醒→渲→present，GPU 渲 app 帧，DWM 于 vsync N+1 醒合成→present，DAC 于 vsync N+2 上屏——三级各错一帧，app 与 GPU 并发执行。【确定度：高，官方图示】
- **DirectComposition**（Windows 版 Core Animation 切面）：visual 树 + 2D/3D 变换 + 效果 + 动画，"operating **independently of the UI thread**"，位图内容可来自任意渲染库，原子 commit（[DirectComposition, Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/directcomp/directcomposition-portal)）。

---

## 5. 对照组二：Chromium cc/RenderingNG——线程级分离的完整教科书

- **12 阶段管线**：Animate → Style → Layout → Pre-paint → Scroll → Paint → **Commit** → Layerize → Raster/decode → **Activate** → Aggregate → Draw；其中 Animate 与 Scroll 可完全跑在合成器线程跳过主线程（[RenderingNG architecture, Chrome for Developers](https://developer.chrome.com/docs/chromium/renderingng-architecture)）。
- **Commit 的定义**："copy property trees and the display list to the compositor thread"——主线程产物是 **display list（绘制描述）+ property trees（变换/滚动/效果树）**，commit 是跨线程原子拷贝（同上）。cc 文档进一步："Commit is a method of getting data atomically from the main thread to the compositor thread… done by **blocking the main thread and copying data over**"，目的 = 一个 JS 调用栈内的多次变更对用户原子呈现（[how_cc_works.md](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/how_cc_works.md)）。
- **四树结构**：主线程 layer tree（cc::Layer）+ 合成器线程 pending tree（光栅中转站）+ active tree（绘制用）+ recycle tree（缓存）；**activation** = pending→active，条件是异步光栅全部完成（同上）。显示列表与光栅瓦片（软件 ~256×256、GPU ~视口宽×1/4 视口高）解耦，TileManager 依赖图调度（同上）。
- **为什么滚动不卡**：滚动手势由浏览器进程直接送 render 进程 **compositor thread**，后者直接改 active tree 的滚动属性——首事件问一次主线程（preventDefault 检查），"Later events skip the main-thread check entirely"（[RenderingNG](https://developer.chrome.com/docs/chromium/renderingng-architecture)）；cc 文档："allows for input and animations to be responsively handled on one thread while the other thread is busy"，pending tree 等光栅时 active tree 照样可滚（[how_cc_works.md](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/how_cc_works.md)）。主线程 JS 长任务再久，滚动/合成器动画不断——这正是 2012 年起 Off-Main-Thread Compositing 工程的目标（[OMTC tracking bug](https://issues.chromium.org/issues/40189611)）。
- **跨进程聚合**：cc 不再自己上屏，"Swap means submitting that frame to the display compositor via a CompositorFrameSink"，全部生产者的 compositor frame 交 **Viz 进程** SurfaceAggregator 聚合成全局帧再由 GPU 线程执行（[how_cc_works.md](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/how_cc_works.md) + [RenderingNG](https://developer.chrome.com/docs/chromium/renderingng-architecture)）——多渲染进程时代的「系统合成器」。

**三方对齐表**（本报告综合）：

| 原则角色 | iOS | Windows | Chromium |
|---|---|---|---|
| 变更描述载体 | layer 树属性（CATransaction） | DComp visual 树 / flip 缓冲句柄 | display list + property trees |
| 提交机制 | run loop BeforeWatching 自动 commit，经 mach | IDCompositionDevice::Commit 原子批 | commit：阻塞主线程原子拷贝到合成线程 |
| 渲染执行方 | render server（backboardd，独立进程） | DWM（独立进程）+ 驱动直扫 | compositor thread + Viz 独立进程 |
| 动画/滚动归属 | server 端插值（app 不逐帧参与） | DComp 动画独立于 UI 线程 | compositor thread tick，主线程缺席 |
| 帧时钟权威 | 系统（vsync/ProMotion 仲裁） | 系统（DWM/驱动） | 系统（beginFrame/Viz） |
| 丢帧可观测 | hitch time ratio（ms/s） | present statistics + glitch recovery | 帧轨迹/dropped frame 计数 |

---

## 6. 普适原则清单（领先管线收敛出的十条）

1. **P1 提交变更描述，不提交像素**：应用产出「树/属性/display list」级别的差异描述，像素只经共享缓冲零拷贝流动（CA 树提交；Chromium display list+property trees commit；DComp visual 树）。跨进程边界流动的是命令与句柄，不是逐像素位图。
2. **P2 逻辑线程与渲染执行分离，且有进程级/线程级两档强度**：iOS/DWM 用独立进程（隔离最强，跨应用统一合成），Chromium 用独立线程（单应用内，主线程忙不挡合成）。分离强度按「隔离对象」选档。
3. **P3 帧时钟权威单点在系统显示侧**：vsync/帧率由显示链路统一仲裁（ProMotion 的 CA 仲裁 + hint API + plist 功耗帽；DWM；Viz beginFrame）。应用对帧率只有建议权没有决定权。
4. **P4 帧管线各阶段流水错相，各领一帧预算**：输入→提交→合成→显示逐级错开一帧（Tech Talks 10855 "parallel every frame"；DXGI 官方时间线；double/triple buffering）。**错相的本质是 deadline 结构，单核分时同样受益**。
5. **P5 背压内建于缓冲池**：有限 drawable 池（2–3 帧 in-flight）自动节流生产者；缓冲不归还 = 生产者阻塞（nextDrawable 语义；DXGI BufferCount 2..16 防 DWM 等待）。流控不需要额外协议。
6. **P6 纯合成可算的效果下沉合成器执行**：动画插值、滚动偏移只需属性数学，放在合成侧逐帧算，内容生产者（app/主线程）零逐帧参与——app 卡顿不冻结屏幕运动。
7. **P7 输入是帧循环的第一阶段且享有通道特权**：专用进程直递 + 命中路由表 + 事件先于提交；采样率/投递率/渲染率三解耦（coalesced/predicted 在系统侧聚合与预测）。
8. **P8 直扫是优化阶梯不是二元开关**：composed → direct flip → MPO 逐级退化也逐级可用；iOS 干脆把整条阶梯收进系统黑箱。合成器永远可用，直扫是锦上添花——**软渲染路线与该原则天然兼容**。
9. **P9 丢帧必须可观测、可归因、可恢复**：hitch time ratio、present statistics 对账、glitch 跳帧恢复——帧管线自带度量协议是先进系统的公共特征（也是 AI 测试友好化的现成形态）。
10. **P10 流畅与功耗同权，且机制上成对出现**：TBDR 省带宽、ProMotion 降档省电、默认 60 帽 + 显式解锁——「又要低功耗又要优秀」不是折衷而是分层：每个帧都有恰当的频率与成本，而不是恒定高频。

---

## 7. 对 BerryOS 的映射：微内核服务形态下的落地

> 本地依据：`docs/GUI里程碑规划-2026-10.md`（三薄服务 guifbd/gui-inputd/gui-wmd + 单 GUI 进程 + berry-ui 内嵌的现行裁决；§3 服务摆位总图；「lite 实现 + 完整架构缝」档位判断）、`user/berry-fbcon/src/lib.rs` + `render.rs`（软渲染核现状）、`user/bins/guifbd/`（GUI-1 种子：boot 经 MMIO VMO 授能 fb 窗 + 几何 info VMO）、`CLAUDE.md`（VMO/Vmar 能力模型、channel/event IPC、intent 机制 ADR #42、PM 共享环、单核 v1、QEMU 无真 GPU/vsync）。

### 7.1 概念对位表

| 领先系统概念 | BerryOS 对应物 | 现状/缝 |
|---|---|---|
| render server（backboardd/DWM/Viz） | **guifbd 的演进方向**：从「fb 持有者」长成「合成器 + 帧调度权威」 | GUI-1 已起（fb VMO 授能）；合成语义是缝 |
| contextID ↔ mach port 路由 | channel handle + gui-wmd 的 WindowInfo 小表（规划 §2 闭包件 8） | 契约 crate 先例四家（fsproto 等） |
| IOSurface 共享内存 | **VMO 能力授予**（dupe 别名随消息递送——注意 v1「销毁性 close 拒映射中 VMO」约定，跨进程必须 dup 别名，CLAUDE.md P6 A-4 已立法） | 机制全在位 |
| commits via Mach | channel 消息（提交变更描述）+ event（帧时钟/完成信号） | 高频 vsync 类信号走专用通道缝（规划 L0 vsync 条同判） |
| drawable 池 / 3-in-flight | N 个帧 VMO 组成的池 + channel credit（取帧消息授权） | v1 单 GUI 进程可先做进程内双缓冲 |
| fence（mach port/XPC） | event handle 递送 + wait_many | 机制在位，语义契约待定（见开放问题） |
| CADisplayLink / preferredFrameRateRange | guifbd 对外「帧回调订阅 + 帧率 hint」协议 | 软 vsync 起步（规划已裁） |
| coalesced/predicted touches | gui-inputd 聚合（多采样点合并进单 PointerEvent）+ 可选预测 | 结构留位（规划闭包件 7 多点位留位） |
| hitch time ratio / present statistics | berry-ui FPS 统计扩为 hitch 统计；smoke 断言面 | GUI-9 DFX 三件的自然扩面 |

### 7.2 六条具体落点

**（a）guifbd = 帧调度权威 + 合成器（P3/P6 的归属裁决）**。vsync（软 vsync 起步）的生成与分发归 guifbd 单点——它同时是 fb 持有者与几何事实源（GUI-1 的 info VMO 已是这个方向的雏形）；应用/launcher 订阅帧回调（channel 信号），不自行定时。这对应 CA 仲裁 ProMotion 与 OpenHarmony vsync_distributor 两方先例的交集。**帧率 hint 协议 day-one 留位**：PointerEvent 协议同族加一个「hint」op（min/preferred/max 三元），仲裁权留 guifbd——真到 LTPO/变帧率硬件时协议面零改动。

**（b）提交语义：berry-ui 的「commit」要长成 CA 形态（P1）**。GUI-3 的视图树设计按「一 pass 一提交」组织：事件→动画→Measure→Render 全在缓冲的树/脏区上工作，pass 末尾一次性 flush 给显示侧（fbcon 的 `Fbcon` 已是「持有 fb slice 的渲染器」形态，包一层即得「显示列表 = 脏区矩形集 + 重绘回调」的最小版本）。跨进程（v2 多 GUI 进程）时，这个 commit 面 = channel 消息里的**矩形集/图层属性 diff**，像素留在共享 VMO——正是规划文档「v2 合成器拆分切面：RS 树/DrawCmdList 形态」预留的缝，本调研确认该缝值得留（三方系统全部收敛于此）。

**（c）单核 v1 下 P4 的正确姿势：错相 = deadline 结构**。BerryOS v1 单核，无法「并行每帧」，但流水错相的收益大头不在并行而在**隔离 deadline**：事件处理与提交拿到的是「本帧预算」，光栅/上屏吃的是「上一帧内容」——单核分时循环（输入→提交→光栅→flip）每个阶段有自己的帧 deadline，长光栅只损失自己阶段的下一拍，不阻塞输入采样（输入在 gui-inputd 独立服务里持续聚合，正是 P7 的通道特权形态）。GUI-3 的软 vsync 循环按此排相位，勿把全部阶段串在单一「无 deadline 大循环」里。
（注：TCG 下 CNTV tick 率 ≈ 标称一半——CLAUDE.md 已实录——软 vsync 周期定值按实测校准，勿按 16.67ms 想当然。）

**（d）背压用缓冲池不用协议（P5）**。v1 进程内：双缓冲 + 「后台缓冲忙则跳帧/合批」；v2 跨进程：guifbd 维护每窗口 2–3 帧池，取帧走 channel 请求-授权（credit），池空 = 申请阻塞（阻塞形态用 wait_many + 专用哑 Event 对照项，避开 donor 面——CLAUDE.md 4-5/4-6 纪律直接适用 GUI 常驻服务）。**GUI 服务一律 spawn_with_tag 挂私有表**：guifbd/gui-inputd 是典型常驻睡眠者，4-5 的错捐级联饿死教训在 GUI 场景同样致命。

**（e）动画器放 berry-ui（v1 进程内免费获得 P6）**。规划已裁「v1 单 GUI 进程内嵌 UI 库」——此时「动画在合成侧执行」自动成立（同一进程内的动画器 tick 直接改变换属性、跳过 Measure/Render）。真正的设计纪律是：**动画驱动只改「合成可算」属性**（位置/透明度/偏移），不触发布局重算——把这条写进 berry-ui 动画器的 API 约束（编译期类型分离 Transform 属性与 Layout 属性是可选的强形态）。v2 拆合成器时动画随 commit 下沉，应用面零改动。

**（f）输入三解耦（P7）与 AI 调度的接口（intent tie-in）**。gui-inputd 采样（virtio-input 中断率）≠ PointerEvent 投递率（帧对齐聚合）≠ 渲染率——事件结构按「一次投递携带本帧全部采样点」设计（coalesced 的 BerryOS 版：PointerEvent 带采样点数组，规划闭包件 7 的多点位留位正好承载）。命中路由表由 gui-wmd 推送（WindowInfo 契约），backboardd「输入路由与合成同屋」的启示：**gui-wmd 与 guifbd 的窗口拓扑事实必须同源**（一个事实源两个视图，避免输入命中与显示内容分叉）。
intent 联动：帧管线各阶段天然分档——输入事件处理 = 最高 intent 档、光栅 = 可降档、动画/合成 = 高档但预算短。ADR #42 的 intent 查表键（u8）给 GUI 服务的调度接口已在册，GUI-4/GUI-5 接线时按此申请档位，正是「AI 原生调度消费 GUI 输入/渲染优先级」的第一个真实消费面（用户裁决 ③「吸取安卓渲染响应慢的教训」在调度侧的落点：**输入路径永不被长光栅任务在 intent 档位上压死**）。

### 7.3 与现行规划的差异/修正建议

- 规划 §5 风险表把「ramfb 无 vsync/flip」列为「不设 kill（体验项）」——**维持**，但建议 GUI-3 帧循环把「软 vsync 周期 + 帧预算 + hitch 统计」做成一等结构而非散落的定时器回调（P4/P9 的可迁移部分不依赖真硬件）。
- 规划闭包件 6「帧循环 = timer/wait_many 软 vsync 驱动 Measure→Render 两遍」——**建议扩为四相位**：输入排空 → 动画/属性 → Measure/Render（脏区）→ 提交/flush，各相位独立计时统计（hitch 面向 GUI-9 DFX 铺路）。
- 「安卓教训」的管线对应物（用户裁决 ③）：安卓的历史问题是 UI 线程承担过多（绘制/布局/输入同线程串行）+ 帧内长任务阻塞输入。BerryOS 对策已隐含在架构里：输入独立服务（gui-inputd）、单 GUI 进程内动画/布局/绘制的相位化、intent 档位保输入——本调研建议把「输入→上屏预算」定为 smoke 可断言的指标（事件注入器 → 屏幕像素变化延迟，GUI-9 事件注入器正好可用）。

---

## 关键发现

1. iOS 渲染循环是官方定义的三段结构——应用处理事件并提交 UI 变更、独立进程 render server 实际渲染、display 上屏；五阶段（event→commit→prepare→execute→display）逐 vsync 流水。证据：Apple Tech Talks 10855（官方一手，URL 见参考 S2）。
2. render server 身份 = iOS backboardd / macOS WindowServer（逆向共识）；应用经 CAContext contextID→mach port 映射路由，提交走 Mach IPC、像素走 IOSurface 共享内存、同步走 fence。证据：EthanArbuckle/ios-rendering-docs（S4）。
3. CATransaction 的提交语义 = 「一次 run loop pass 攒一批变更、BeforeWaiting 观察者处自动 commit」；提交前 Layout/Display/Prepare 四阶段内联在应用进程，commit 只发送树变更。证据：fabernovel（S15）+ SCNTransaction 官方文档（S3）+ Tech Talks 10855（S2）。
4. UI 线程不等 GPU 的机制根源：提交即返回（树序列化 + IPC 投递）、GPU 执行在另一进程另一帧时段、唯一背压点是 drawable 池。证据：Tech Talks 10855（S2）+ objc.io #3（S5）+ MoltenVK #929（S16）。
5. 帧管线官方口径「每帧并行流水」：app 备下一帧的同时系统渲染上一帧；双缓冲起步、必要时三缓冲。证据：Tech Talks 10855（S2）；Windows 侧同构错相时间线见 Microsoft DXGI flip model 文档（S10）。
6. ProMotion 帧率协商 = 系统 CA 统一仲裁 + 应用 preferredFrameRateRange 只作 hint + iPhone 需 plist 键显式解锁 >60Hz（功耗默认收紧）；12 档刷新率下探 10Hz。证据：iClarified 转 Apple 开发者文档（S8）+ preferredFrameRateRange 文档（S7）。
7. Metal drawable 池 = 背压内建：maximumDrawableCount 2–3，池满 nextDrawable 阻塞（stall 在取缓冲不在 present）。证据：Apple 文档（S17）+ MoltenVK #929 实证（S16）。
8. Apple GPU TBDR 与 Metal load/store action/imageblock 协同把深度/混合留在片上 tile memory——带宽即功耗的架构级对策；UI 合成负载与其天然契合。证据：WWDC20 10602/10632（S6）。
9. 触摸路径 = IOKit IOHIDEvent → backboardd（contextID 路由）→ mach 直递前台进程主 run loop → hit-test → responder；事件阶段是 Render Loop 第一阶段、有独立帧 deadline。证据：Jacob's Tech Tavern（S1）+ The Apple Wiki（S14）+ Tech Talks 10855（S2）。
10. coalesced/predicted touches 实现「采样率(240Hz)/投递率(帧率)/渲染率」三解耦——聚合与预测在系统侧、展开放权给应用。证据：Apple 官方文章（S9）。
11. Windows 侧收敛同构：flip 模型缓冲与 DWM 共享零拷贝、DirectFlip/MPO 直扫阶梯（含窗口化直扫）、present statistics 使丢帧可对账可恢复；DirectComposition 的 visual 树动画「独立于 UI 线程」。证据：Microsoft Learn（S10/S11/S12）。
12. Chromium 侧收敛同构：display list + property trees 经 commit 原子拷贝到 compositor thread、pending/active 四树、光栅异步、滚动手势在合成线程直改属性树（首事件后跳过主线程）、Viz 进程跨源聚合——主线程忙不卡滚动。证据：RenderingNG（S13）+ how_cc_works.md（S18）。
13. 三系统对照提炼出十条普适原则（P1–P10，§6），其中「提交变更描述而非像素」「帧时钟权威单点」「背压内建缓冲池」「直扫是优化阶梯」四条在软渲染/无 GPU 环境依然完全可迁移。综合 S2/S10/S13/S18。
14. 本地对照：BerryOS 现行规划（三薄服务 + 单 GUI 进程 + lite 实现/完整架构缝）与本调研原则兼容——「lite 档」恰好对应 Chromium 的「分离强度降档」先例；关键缺口不在形态而在「提交语义/帧预算/hitch 度量」三个尚未成文的协议面。证据：docs/GUI里程碑规划-2026-10.md（本地）。

## 对 BerryOS 的架构启示

1. **把 guifbd 定位成「帧调度权威 + 合成器」而非 mere fb 持有者**：软 vsync 生成/分发、帧率 hint 仲裁、缓冲池管理三权归它单点（对应 P3）；GUI-1 已授 fb VMO，方向顺延即可（docs/GUI里程碑规划-2026-10.md §3/§4 + S2/S8）。
2. **berry-ui 的 commit 面按「变更描述」设计**：一 pass 一提交、提交物 = 脏区矩形集 +（v2）图层属性 diff，像素只经共享 VMO 流动（对应 P1）；GUI-3 视图树 API 从第一天就把「记录变更」与「执行渲染」分开，v2 拆合成器时协议面零翻修（S2/S13/S18 + 规划附录 B L0「命令录制回放」条）。
3. **帧循环四相位化 + 每相位帧预算**：输入排空→动画→Measure/Render→提交 flush，各相位独立计时；单核下错相的收益是 deadline 隔离而非并行（对应 P4）；软 vsync 周期按 TCG 实测校准（CLAUDE.md tick 率实录）。
4. **背压走缓冲池不走协议**：v1 进程内双缓冲；v2 每窗口 2–3 帧池 + channel credit 授权取帧，池空阻塞（对应 P5）；GUI 常驻服务挂私有 handle 表 + wait_many 哑项（CLAUDE.md 4-5/4-6 纪律直接适用）。
5. **动画器约束为「只改合成可算属性」**：位置/透明度/偏移不触发布局，API 面把 Transform 与 Layout 属性类型分离（对应 P6）；v1 进程内免费获得「应用卡不冻结动画」，v2 动画随 commit 下沉（S2/S13）。
6. **gui-inputd 三解耦 + 聚合在服务侧**：PointerEvent 携带本帧全部采样点（闭包件 7 多点位留位承载 coalesced 语义）；采样/投递/渲染率互不锁定（对应 P7，S9）；窗口拓扑事实 gui-wmd↔guifbd 同源单点。
7. **intent 档位把「输入永不被光栅压死」变成调度面承诺**：GUI 各阶段报不同 intent（输入最高/光栅可降/合成短高高预算），这是 ADR #42 intent 机制的首个真实消费面、也是「吸取安卓渲染响应慢教训」的系统级对策（用户裁决 ③；CLAUDE.md ADR #42）。
8. **hitch 度量进 DFX 三件**：FPS 统计升级为 hitch time ratio（hitch ms/s，阈值 5/10 两档可断言）+「事件注入→像素变化」输入延迟指标，smoke 相可机器断言（对应 P9 + AI 测试友好化支柱，S2）。
9. **帧率 hint 协议 day-one 留位（三元 min/preferred/max）**：仲裁权留 guifbd、应用只有建议权（对应 P3/P10，ProMotion 先例 S7/S8）——真 LTPO 硬件到位时协议零改动，QEMU 阶段软 vsync 可模拟变周期验证调度路径。
10. **直扫/真 vsync 相关全部做成「优化阶梯」形态的缝而非前提**：软渲染合成器永远可用（对应 P8 + 规划「RASTER 纯软回退是一等路线」判断），ramfb→virtio-gpu→真硬件 overlay 逐级替换，GUI-1..8 验收不依赖任何一级。

## 开放问题

1. **virtio-gpu 在 QEMU aarch64 virt + AAVMF 环境下的可用性与 2D 加速面**（virgl/venus 是否可用、cross-domain 不可用的替代路径）——本报告范围外，建议 GUI-03 专项核验；直接影响 P8 直扫阶梯在 BerryOS 模拟器链上的最高可达档。
2. **iOS 是否存在内部 per-layer overlay/直扫机制**：公开面只有「render server 扫描输出」，overlay 决策无文档；对标 Android HWC 的 in-kernel/in-server 决策机制属逆向空白（确定度低，不影响原则迁移）。
3. **跨进程 fence 的最小语义契约**：v1 channel/event 能否表达「该 VMO 帧内容已完成、可上屏」的读写时序承诺（对应 Metal fence/IOSurface 同步）——需要在 GUI-5/合成器拆分刀前定协议（event handle 随帧消息递送 + wait_many 是否足够，还是有竞速窗口）。
4. **软 vsync 下的 ProMotion 等价物验证深度**：QEMU 无真变刷新率，但「按内容需求变帧周期」的调度路径（hint→仲裁→周期切换）可以用软 timer 全链路演练——演练到什么深度（仅协议对拍 vs 含功耗代理指标）待裁。
5. **backboardd「输入路由与渲染合成同屋」是否值得 BerryOS 模仿**（gui-wmd 与 guifbd 合并为单一「显示与窗口服务」 vs 维持三薄服务分离）：合并减少拓扑事实同步、但增大单服务崩溃面（E 失败隔离裁决冲突点）；建议 GUI-5 前用一次 ADR 定夺。
6. **单核 TCG 下四相位帧循环的实测预算分配**：各相位实际耗时（尤其整屏 1.9MB flush 与脏区大小关系）需 GUI-3 刀内用 B-2 icount 通道定标——本报告只给了相位结构，没给数值预算。

---

## 参考来源

**网络来源（官方/权威一手在先）**

- S1. Jacob's Tech Tavern — Touch to Pixels: UI Pipeline Internals on iOS（2026-05）：https://blog.jacobstechtavern.com/p/ui-pipeline-internals （触摸全路径、CA 四阶段引 WWDC14-419、CAContext/FBSScene 路由）
- S2. Apple Tech Talks 10855 — Explore UI animation hitches and the render loop：https://developer.apple.com/videos/play/tech-talks/10855 （Render Loop 三段五阶段官方定义、双/三缓冲、hitch/hitch time ratio）
- S3. Apple Developer Documentation — SCNTransaction（隐式事务 run loop 自动提交语义）：https://developer.apple.com/documentation/scenekit/scntransaction ；kCATransactionDisableActions：https://developer.apple.com/documentation/quartzcore/kcatransactiondisableactions
- S4. EthanArbuckle/ios-rendering-docs（iOS 渲染架构逆向）：https://github.com/EthanArbuckle/ios-rendering-docs/blob/main/README.md （render server=backboardd/WindowServer、contextID→mach port、Commits via Mach、IOSurface、fence）
- S5. objc.io #3 — Getting Pixels onto the Screen：https://www.objc.io/issues/3-views/moving-pixels-onto-the-screen/ （GPU 合成模型、layer=texture、移动复用纹理）；#12 Animations Explained：https://www.objc.io/issues/12-animations/animations-explained
- S6. WWDC20 10602 — Discover Metal on Apple GPUs：https://developer.apple.com/videos/play/wwdc2020/10602 ；WWDC20 10632 — Optimize Metal Performance for Apple Silicon Macs：https://developer.apple.com/videos/play/wwdc2020/10632 ；imageblockMemoryLength：https://developer.apple.com/documentation/metal/mtlrenderpipelinestate/imageblockmemorylength(fordimensions:) ；9to5mac 转述：https://9to5mac.com/2020/07/08/arm-mac-graphics/
- S7. Apple — CADisplayLink.preferredFrameRateRange：https://developer.apple.com/documentation/quartzcore/cadisplaylink/preferredframeraterange ；CADisplayLink：https://developer.apple.com/documentation/quartzcore/cadisplaylink
- S8. iClarified — Apple Posts Developer Documentation for ProMotion（12 档刷新率/仲裁/plist 键）：https://www.iclarified.com/82663/apple-posts-developer-documentation-for-promotion-on-iphone-13-pro-12-refresh-rates-used ；CAFrameRateRange 校验实证：https://github.com/jtrivedi/Wave/issues/30
- S9. Apple — Implementing coalesced touch support in an app：https://developer.apple.com/documentation/uikit/implementing-coalesced-touch-support-in-an-app （coalesced/predicted touches）
- S10. Microsoft Learn — DXGI flip model：https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/dxgi-flip-model （共享缓冲零拷贝、present statistics、glitch 恢复、错相时间线）
- S11. Microsoft Learn — DirectComposition：https://learn.microsoft.com/en-us/windows/win32/directcomp/directcomposition-portal （visual 树、独立于 UI 线程）
- S12. Microsoft Learn — For best performance, use DXGI flip model（DirectFlip/MPO 直扫条件，经搜索摘要）+ MPO 实证讨论：https://www.techpowerup.com （"Hardware: Independent Flip" 观测）
- S13. Chrome for Developers — RenderingNG architecture：https://developer.chrome.com/docs/chromium/renderingng-architecture （12 阶段、commit、Viz、合成线程滚动）
- S14. The Apple Wiki — Dev:backboardd：https://theapplewiki.com/wiki/Dev:Backboardd （触摸路由职责）
- S15. fabernovel — UIKit rendering: The run loop：https://fabernovel.github.io/2021-01-04/uikit-rendering-part-3 （kCFRunLoopBeforeWaiting 提交、run loop 相位序、崩栈证据）
- S16. MoltenVK issue #929（3 drawable 池、nextDrawable 阻塞实证）：https://github.com/KhronosGroup/MoltenVK/issues/929
- S17. Apple — CAMetalLayer / maximumDrawableCount：https://developer.apple.com/documentation/quartzcore/cametallayer/maximumdrawablecount ；waitUntilScheduled：https://developer.apple.com/documentation/metal/mtlcommandbuffer/waituntilscheduled()
- S18. Chromium 源内文档 — how_cc_works.md：https://chromium.googlesource.com/chromium/src/+/HEAD/docs/how_cc_works.md （四树、commit/activation、合成线程滚动）；OMTC tracking bug：https://issues.chromium.org/issues/40189611

**本地来源**

- L1. `docs/GUI里程碑规划-2026-10.md` — M-GUI 立项规划、三薄服务摆位、15 件闭包、lite/完整档裁决、OpenHarmony 功能全景（附录 B）
- L2. `user/berry-fbcon/src/lib.rs` + `render.rs` — 软渲染核现状（蜂格/glyph blit/像素打包，28 host 测）
- L3. `user/bins/guifbd/Cargo.toml` + `src/main.rs` — GUI-1 种子：fb MMIO VMO 授能 + 几何 info VMO + FBW ok 自验形态
- L4. `CLAUDE.md` — VMO/Vmar 能力模型、channel/event/wait_many、intent 机制（ADR #42）、常驻服务私有表纪律（4-5/4-6）、TCG tick 率实录、QEMU/ramfb 环境事实、微内核失败隔离原则

（2026-10-28，GUI-02 专项调研）
