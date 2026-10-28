# GUI-05 专用/微内核 OS 的 GUI 架构对照——服务摆位 / buffer 协议 / 帧调度 / 输入管线

> 调研日期 2026-10-28 · 调研者 = M-GUI 架构层深挖 workflow（run wf GUI-05）
> 对象谱系：Fuchsia（同宗微内核+能力模型）· Wayland 协议族（二十年协议教训）· Android SurfaceFlinger（移动延迟工程）· QNX Screen（车载实时）· OpenHarmony Rosen（本地源码走读）
> 证据纪律：联网论断标 URL；本地源码/文档标路径。确定度三级——【高】= 一手官方文档或源码直读、多源互证；【中】= 单一来源或二手转述、方向可信细节待核；【低】= 社区传闻/推断。
> 边界：本报告不做功能清单（`docs/GUI里程碑规划-2026-10.md` 附录 B 已有），只做**系统级架构**层：进程怎么摆、buffer 怎么过、帧怎么调度、输入怎么进。

---

## 0. 一页结论（先给答案）

五家谱系收敛出的共同形状，BerryOS 应当继承的四条骨架：

1. **显示/合成是特权服务，渲染在客户端，协议只有三件事：damage（哪些变了）、buffer（内容在哪）、frame callback（下一拍何时来）**。Wayland 二十年验证了这是最小完备面；Flatland 是同一语义在能力模型 OS 上的重述。
2. **帧调度是合成器主导的「未来时」协议**：客户端不猜 vsync，合成器下发未来 presentation 时间戳 + 提交流控（Fuchsia present credits / OH RequestNextVSync / Wayland frame callback 三家同构不同名）。
3. **buffer 跨进程 = 「协商分配 + 句柄授能 + fence 事件」三件套**：dmabuf（fd）、BufferCollection（token+约束交集）、BerryOS 对应物 = VMO handle + event fence，语义可完整映射【高】。
4. **输入命中必须归合成器/窗口服务**（只有它知道屏幕上真实是什么——防 clickjacking + 动画期一致性，Fuchsia RFC-0096 明文；QNX Screen window group、OH WindowInfo 推送同向）。

对 BerryOS 的摆位建议（§6 详证）：**维持规划稿「三薄服务 + 单 GUI 进程」，但把三服务的契约从「函数调用面」升级为「协议语义面」**——guifbd 的契约按 Wayland/Flatland 三件事（damage/buffer/frame）定形，gui-inputd 按「采集→归一→按窗口命中分发」定形，为多 GUI 进程与 GPU 后端留升级缝。

---

## 1. Fuchsia：同宗微内核+能力模型的 GUI 全栈（最直系参照）

Fuchsia 的图形栈全部住在用户态：`scenic`（合成器）· `sysmem`（buffer 分配协商）· `Magma`（GPU 驱动框架）· display-coordinator（显示控制器抽象）· input-pipeline（输入管线），服务间以 FIDL over Zircon channel 通信、以共享 VMO 过数据【高】（hdlfactory 深度报告：https://www.hdlfactory.com/post/2026/08/25/fuchsia-internals-vol-1-graphics-and-display ；注意该文作者自述为自动整理稿，细节以官方文档为准）。这与 BerryOS「内核零 GUI 知识 + channel IPC + VMO 授能」完全同构，是五家中唯一可直接平移语义的参照。

### 1.1 Scenic→Flatland：为什么放弃通用 3D 场景图

演进事实【高】（官方 roadmap：https://fuchsia.dev/fuchsia-src/contribute/roadmap/2021/flatland ；RFC-0166 One UI Stack：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0166_ui_stack ）：

- 旧 API `fuchsia.ui.gfx` 是**通用 3D 场景图**（按视频游戏建模：节点树、深度、光照）。但真实客户端（Flutter/Chromium/session 框架）**全是 2D 产品**——"our current customers are fundamentally 2D products"。客户端被迫在自己的 2D 输出与 Scenic 的 3D 表示之间架桥：2D 世界的 draw order 就是提交序、没有深度概念，而 3D 模型下「组不透明度（group opacity）」这类 2D 常用效果"impossible"。
- 性能面：现代显示控制器（VDC）硬件 overlay 平面**严格 2D 范式**——只懂 X/Y 摆位的矩形层。3D API"allows and encourages clients to submit content that doesn't fit into this paradigm and frustrates optimization attempts"（鼓励客户端提交硬件范式外内容，毁掉优化机会）。
- 解法 = Flatland：把「显示控制器能力」直接暴露给客户端——只准提交 X/Y 平移+缩放的矩形层，"exposing display-controller-like functionality to clients"。预期收益：真 2D API 匹配客户端预期 + "Less GPU usage by delegating work to the VDC where possible" + 更轻量的 Scenic。
- 迁移代价如实记录：所有客户端逐个转换、必须 per-product 原子切换（输入路由不能混跑两套场景图半切换状态）。

**教训（BerryOS 视角）**：
- 场景图的表达力要**贴着最终硬件（显示控制器/overlay）的范式**定，不是贴着「图形学上更通用」定——通用 3D 是负资产：客户端用不上、硬件用不上、合成器白白背复杂度。BerryOS berry-ui 视图树变换只做 2D 仿射（平移/缩放/裁剪）即是对齐，这不是将就而是 Flatland 用七年学费换来的结论。
- 「显示硬件能直接扫出的层」是一等公民：Fuchsia 把 VDC overlay 能力上翻到协议层。BerryOS 未来 virtio-gpu/真 overlay 时，guifbd 契约里的「层」应保持「矩形+平移+缩放」的可直扫形态。

### 1.2 Flatland 现行 API 形态（合成器侧语义）

现行协议【高】（概念文档：https://fuchsia.dev/fuchsia-src/concepts/ui/scenic/flatland ；FIDL 一手源码 `sdk/fidl/fuchsia.ui.composition/flatland.fidl`，fuchsia.googlesource.com main 分支实抓）：

- **命令式追加 + Present 提交**：客户端对场景图的操作是一条条单向 FIDL 命令（无命令 union——`Flatland does not offer a command union pattern. Present() call is the marker for processing the enqueued commands`，RFC-0162），Present() 是命令序列的终点标记，场景图保留模式（retained mode）、增量修改。
- **全局场景图、内容分布式**：每个客户端一个 Flatland 实例，实例间经 Viewport/View link（token 对）连成全局图；每个实例只能渲染到父实例划定的屏幕区域。Scenic 合并各实例子图，能直扫就送 VDC、否则 Vulkan 渲染成单层。
- **每 session 独立 dispatcher**：会话之间互不拖累提交节奏；渲染是"one deadline-scheduled render thread"——确定性 CPU 成本。
- 图片内容强制经 `fuchsia.ui.composition.Allocator` 分配（底层即 sysmem BufferCollection），"No copies are made when transferring Image resources"——传输零拷贝。

### 1.3 Present/帧调度协议：future 语义 + 流控 + fence

这是 Fuchsia 最值得整段吸收的部分。三层证据链【高】：

1. RFC-0162（Flatland API，https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0162_flatland ）：Present() 带严格节流——`Each client starts with one present allowance`，恶意客户端不能无限排队提交（**present credits 流控**）；`OnPresentProcessed()` 告诉客户端"何时开始下一帧工作是好时机"+ 理想下次 Present 时机提示；`OnFramePresented()` 告知上一帧实际到达屏幕的时刻（AV 同步用）。设计动机原话：此前客户端"rely on hardcoded offsets to avoid overlaps"——**靠硬编码偏移猜时机必然错过帧**，改为服务端显式反馈。
2. 现行 FIDL（flatland.fidl 实抓）将反馈升级为 `OnNextFrameBegin`：每次 Present 后服务端回包内含 **`FuturePresentationInfos`（最多 8 个未来 presentation 时刻窗口）+ `additional_present_credits`（补发的提交配额）**。客户端拿未来 vsync 预测表**倒排**自己的工作（"Using a requested_presentation_time in the future schedules the enqueued operations to"，字段注释原文）。
3. fence 语义【高】（flatland.fidl 字段注释）：`acquire_fences`——Flatland 等待全部 acquire fence 置位才执行本次提交的命令，不置位则**本次和后续 Present 全部阻塞**（顺序性执法）；`release_fences`——当本次提交后不再需要的旧资源可以安全复用时置位。另有 `release_counters`（计数式 release，事件式的补充）。fence 全部是 `zx.Handle:EVENT`——**Zircon event 即内核事件对象，正是 BerryOS 的 event 对象**。

**教训**：
- 帧回调的载荷不是「一拍脉冲」而是「**未来时间戳表**」——客户端能倒排（提前多少开始算），这比 Wayland frame callback 的纯脉冲信息量高一档，是 Android frame timeline（§3.2）与 Fuchsia 的共同收敛点。
- 提交流控（credits）是防客户端淹没合成器的标准件。BerryOS channel 有界队列天然有背压，但「每客户端在途帧数上限」的语义要显式写进 guifbd 契约。
- acquire/release fence 对在无 GPU 的 v1 也可用 event 表达（§7.2 详证），不必等 GPU。

### 1.4 BufferCollection：sysmem 跨进程 buffer 协商

【高】（官方 sysmem 概念文档，2026-09 更新：https://fuchsia.dev/fuchsia-src/development/graphics/sysmem/concepts/sysmem ）

存在理由：现代 SoC 是**硬件流水线**（GPU/视频编解码器/相机 DSP/显示 overlay），数据要在硬件块之间直传，驱动与应用必须对**格式与内存位置**达成一致，一个单元的输出才能直接喂下一个单元。sysmem = "a global service that can be fed constraints by applications and allocate buffers such that all the constraints are met"。

协商协议（生命周期逐步）：
1. 各参与方连 `fuchsia.sysmem2.Allocator`；
2. 发起方创建初始 **token**；
3. **token 复制并转交**给其他参与方（可递归转发，token 树=参与方树）；
4. 各方把自己的 token **bind** 成 BufferCollection 通道；
5. 各方**设置约束**（尺寸/格式/用途/堆类型…，通常给多组候选让 sysmem 有选择余地）；
6. **全部参与方都 bind 且 set 完约束后** sysmem 才做约束交集、选一个对所有方合法的格式（多解时按用途与系统架构选优）；
7. 一次分配整组 buffer；各方取回**每 buffer 一个 VMO handle** + SingleBufferSettings（缓存属性/图像格式）。
8. 后加入方走 `AttachToken`（约束必须容得下既有分配）；`SetDispensable` 允许初始参与方出「占位约束」提高后加入成功率；TokenGroup 提供「可选参与方/约束档位」的 OR 语义（失败隔离的子树）。
9. 销毁异步：通道关闭即失败扩散关闭各通道，但 **VMO 活到所有引用（handle/映射/子 VMO/硬件 pin）归零**。

关键点：
- **token 就是能力**（可复制/可转交/可收窄），协商纯走 handle 传递，无中心化权限检查——这是 buffer 协议在纯能力模型上的原生形态。
- **堆（heap）概念**：系统内存/独显 VRAM/特定硬件专用 carveout；有的堆 CPU 不可访问，VMO "is instead used as a key"（句柄只是把内存指给硬件驱动的钥匙）。
- sysmem 只管格式与分配，**不管数据流**——同步（fence）由参与方自行协调。分配与同步正交分离。

### 1.5 Magma：GPU 驱动的内核边界

【高】（RFC-0198 Magma API：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0198_magma_api_design ；porting guide：https://fuchsia.dev/fuchsia-src/development/graphics/magma/concepts/porting ）

- 双驱动切分：**ICD（client driver）** 装在每个应用地址空间的厂商库（Vulkan 实现），"cannot access the hardware"；**MSD（Magma System Driver）** 每个物理 GPU 一个、boot 期实例化，负责"调度并执行客户端请求到硬件上"。Magma 定位对标 Linux 的 DRM。
- MSD 职责：执行上下文调度到硬件；流控上限发布与事件回告；abort 长时间运行的 GPU 程序；**每连接独立硬件地址空间**做隔离（地址空间不够时排队或拒绝）。
- 内核接口面（MSD 消费的 Zircon 原语）：**VMO 承载一切共享 buffer；Zircon event 充当 semaphore；对象按 KOID 引用**；MapBufferGpu 隐含页表填充；设备发现走 `/dev/class/gpu`；perf counter 单独门控路径。即——**内核不需要任何 GPU 专有 syscall**，GPU 特有逻辑全在用户态 MSD 里，内核只提供通用原语（channel/event/VMO/中断/MMIO 窗）。
- 三协议分层：Device（同步、仅 setup 期查询）/ Primary（异步单向 client→server，错误即 epitaph+关通道：ImportObject/MapBufferGpu/CreateContext/ExecuteCommand…）/ Notifications（server→client，厂商差异隔离在独立通道，客户端用专用线程收——"primary-channel calls aren't proxied there for performance reasons"）。
- 安全模型：**客户端 GPU 程序不做验证**（"running unvalidated; isolation relies on hardware address spaces"——命令验证 impractical and slow），信任硬件地址空间隔离。这把「驱动安全性」从「检查每个命令」降为「配好 MMU」。
- 明确非目标：display 不归 Magma 管（显示走独立 display-coordinator）——GPU 命令与显示扫描out分离。

**教训**：BerryOS 未来 virtio-gpu/GPU 驱动应该照此切分——内核零新面（MMIO 窗 VMO + IRQ event + channel 即全部），用户态「设备服务」持硬件、「客户端库」（对 virtio-gpu 是 2D 命令封装，未来真 GPU 是 Vulkan ICD 类似物）进应用；GPU buffer 隔离靠设备侧地址空间（virtio-gpu 的 resource 模型天然如此）而非验证命令流。

### 1.6 输入管线：驱动 → input pipeline → Scenic → view 树

【高】（RFC-0096 User Input Architecture：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0096_user_input_arch ）

- 规范路径：**Driver → Input Pipeline → UI System Component → UI Framework → View**。
- input pipeline 是特权系统组件（Rust，InputHandler trait 链）：把无状态 InputReport 归一为有状态 InputEvent（相对位移→屏幕坐标、键位快照→down/up 对），再经 handler 链（shortcut/IME/键盘布局/触摸板手势/平滑/焦点/睡眠/媒体键…）增删改派发。它是**平台策略层**，刻意不放 session realm（要用平台不愿下放给会话的能力）。触摸优先与鼠键优先两个产品形态=同一套 handler 代码不同配置。
- **指针事件交给 Scenic 派发**，三条理由（RFC 原文）：①场景图隔离——只有 Scenic 能做 hit test，"no other component can query what's on screen"（任何组件都不能查询屏幕上有什么——防 clickjacking 的结构性保证）；②全局一致性——view 尺度/移动/消失 mid-gesture 时派发仍正确（动画期不竞态）；③手势歧义消解（parallel dispatch + gesture arena）。
- **焦点链（focus chain）归 Scenic 持有**：从根 view 走到焦点 view 的 ViewRef 向量；input pipeline 监视焦点链喂给 handler；快捷键管理器查整条链（任何祖先都可能有快捷键）。焦点变化本身也是 InputEvent、与其它输入严格定序。
- 输入事件流一致性：FINGER DOWN→MOVE→UP 必须按 view 连贯；handler 消费后有 handled 标记向下游传播；焦点切走/设备拔掉时给 cancel 而非悬挂的 down/up。
- **隐私红线**：除 recovery 模式外，input pipeline 是唯一允许直读驱动输入的组件（反键盘记录器）。合成输入走开发者构建专用 API，生产不可用。
- 性能结论（RFC 原话）：用户 ~10ms 起感知延迟、>100ms 明显劣化、>300ms 不可接受；缓解=最小化内核切换/上下文切换 + 关注延迟**分布**（不只均值）+ 手势 arena 响应的客户端 SLA。被否方案也有教益：「把窗口管理/输入策略焊进合成器」被明确否决（把产品策略烤进图形引擎，太重）。

---

## 2. Wayland 协议族：二十年「协议最小面」的经验与教训

### 2.1 为什么 X11 被替代

【中-高】（综合 Daniel Stone "The real story behind Wayland and X"（LCA 演讲，讨论见 https://www.reddit.com/r/linux/comments/18065e/the_real_story_behind_wayland_and_x_daniel_stones ）+ wayland-book 第 1 章 https://wayland-book.com/ ）：X11 的结构病——协议中心化在 X server（合成、字体、剪贴板全家桶历史包袱），现代桌面上「everyone is a compositor」（WM 在 X 之上再合成一层），客户端走间接渲染路径，tearing/vsync 无法根治，且 X 的信任模型假定 client 可信（任何 client 可抓全局输入/截图）。Wayland 的翻转：**合成器就是显示服务器，客户端自渲染**，协议只管「buffer 交换 + 输入分发 + 表面状态」。社区保留的批评（reddit/Gist 汇总）也如实记录：Wayland 核心协议太小导致生态碎片化（每个扩展协议都要协商，X11 atoms 生态断裂）——**最小面是双刃剑：内核式正交性 vs 生态必需件的缺位**，后文 §2.5 wlroots 生态是对这刀的回应。

### 2.2 wl_surface 生命周期：attach/damage/commit/release

【高】（wayland-book 目录与第 8 章 frame-callbacks 实读：https://wayland-book.com/surfaces-in-depth/frame-callbacks.html ；commit 语义补丁讨论 https://lore.freedesktop.org/wayland-devel/1448056199-19061-2-git-send-send-derekf@osg.samsung.com ；damage 释疑 https://stackoverflow.com/questions/58471743/what-is-a-damage-request-in-wayland ）

- **双缓冲状态模型**：wl_surface 的全部状态（pending/current）双缓冲；客户端对 surface 的修改（attach 新 buffer、damage 区域、input/opaque region、变换…）先进 pending，**`commit` 一次性原子应用**到 current——「on commit, the pending wl_buffer is applied first, then all other pending state」。（对比 X11 的逐请求即时生效、竞态易碎。）
- **attach + damage + commit 三步提交**：attach 声明内容在哪个 wl_buffer；damage 声明「相对上次成功提交变了哪些区域」（合成器只重绘脏区——从 X11 全量重绘教训继承的核心优化）；commit 应用。
- **wl_buffer.release = 生命周期归还信号**：合成器用完 buffer（不再需要读它）时发 release，客户端才能复用/重画该 buffer。这就是**buffer 所有权协议化**：n 个 buffer 轮转（双/三缓冲）的节奏完全由 release 事件驱动，天然背压。
- 销毁语义显式：父 surface 销毁后子对象 inert（仅 destroy 合法）——对象生命周期进协议（2012 子表面讨论 https://lists.freedesktop.org/archives/wayland-devel/2012-December/006637.html ）。

### 2.3 wl_shm vs linux-dmabuf

【高】（wayland-book 第 6 章目录：6.2 shared memory / 6.3 Linux dmabuf；weston 2014 补丁讨论 "wl_dmabuf could be used as an alternative to shm... it may save the cost of one copy" https://lists.freedesktop.org/archives/wayland-devel/2014-January/012727.html ；内核 dma-buf 文档 https://www.kernel.org/doc/html//v5.12/driver-api/dma-buf.html ；linux-dmabuf feedback 扩展 https://forge.quantum5.ca/quantum/wayland-protocols/src/commit/b8ef271689ffc0a0f7a511e5ec469d88352e9f29/stable/linux-dmabuf/feedback.rst ）

- **wl_shm**：共享内存池（fd 传 anonymous memory），CPU 渲染路径——客户端写像素、合成器 mmap 读。普适、简单、必有；代价=无法表达「GPU/设备专用内存布局」。
- **linux-dmabuf**：buffer 以 dmabuf fd 提交——缓冲留在 GPU/设备可达内存，合成器直接 import（如 EGLImage）零拷贝；**feedback 扩展**让合成器广播自己支持的 format+modifier 组合、客户端选最优（能直扫出的 modifier 连合成器侧拷贝都免掉）。内核 dma-buf 子系统提供引用计数跨驱动共享 + 内建 fence 同步。
- 惯例：**客户端先试 dmabuf、失败回退 shm**；软渲染客户端也可用 dmabuf（让合成器直接 GPU import，省一次上传——Qt 论坛实践 https://forum.qt.io/topic/147503/using-dma-instead-of-shm-in-non-opengl-apps-wayland ）。

**BerryOS 映射**：这条二分在 BerryOS 上恰好=「普通 DRAM VMO（CPU 可写）」vs「设备窗 VMO/专用堆 VMO」。VMO 已同时能表达两者（sysmem 的 heap 概念——VMO 句柄甚至可以只是「把设备内存指给驱动的钥匙」）。

### 2.4 frame callback 语义

【高】（wayland-book frame-callbacks 章实读）

- 客户端 `wl_surface.frame` 请求注册回调；合成器在**自己准备好接收下一帧时**发 `done`（携带毫秒时间戳）——节奏由合成器/刷新周期主导，不是客户端请求时刻。
- 标准循环：done → 销毁旧回调 → 立即注册新回调 → 渲染 → attach+damage+commit。**让合成器告诉你它何时要下一帧**是「持续渲染的正确姿势」：渲染快过刷新率纯浪费（"all but the last will be discarded"）。
- **背压与节流**：合成器可对最小化/离屏/遮挡/缩略图 surface 降频发回调甚至停发——隐式省电机制（compositor 控制唤醒源）。
- 撕裂根治：原子 buffer 应用（commit 原子换 buffer）→ 无半帧画面；纳秒级呈现时刻走 presentation-time 扩展。

### 2.5 wlroots/Smithay：协议最小面之上还需要一层「公共合成器地基」

【中】（wlroots 立项报道 "modular Wayland compositor library" https://www.phoronix.com/news/Wayland-wlroots ；HN 讨论 "wlroots and smithay do a lot of the difficult work for you, that most compositors will do without much variation" https://news.ycombinator.com/item?id=46382610 ；Smithay https://github.com/Smithay/smithay ）

生态证明了：Wayland 核心协议小 → 每个合成器仍要重复实现同一批「与协议无关的苦活」（KMS/DRM 后端、输入设备热插拔、buffer 分配器、渲染器抽象、屏幕排列…）→ wlroots（C）/Smithay（Rust）/Louvre 以库形式沉淀公共部分，催生了 sway 等一批合成器。**教训是两面的**：协议面确实可以小（damage/buffer/frame/input 四件事够用二十年）；但「显示服务器内核」之下的**实用层**（后端/分配/渲染/输入栈）必须有一层可复用库，否则每个实现都从零爬。BerryOS 的对应物 = berry-gfx/berry-ui 做成 crate 库（规划稿已是此形态），GUI 服务只是这些 crate 的组装点。

---

## 3. Android SurfaceFlinger：移动端延迟工程的十年

### 3.1 BufferQueue → BLASTBufferQueue

【高】（官方 BufferQueue/gralloc 文档：https://source.android.com/docs/core/graphics/arch-bq-gralloc ；AOSP 源码 BLASTBufferQueue.cpp：https://android.googlesource.com/platform/frameworks/native/+/master/libs/gui/BLASTBufferQueue.cpp ；动机叙述含二手中文深读【中】：https://juejin.cn/post/7527239817937174569 、https://zhuanlan.zhihu.com/p/1917988013490800529 ）

- 经典 BufferQueue = **生产者/消费者环形队列**：app（生产者）`dequeueBuffer`（给尺寸/格式/usage）→ 写入 → `queueBuffer`；SurfaceFlinger（消费者）`acquireBuffer` → 合成 → `releaseBuffer` 归还。buffer 永远按 handle 传递不拷贝（"buffers are always passed by a handle"）；特性（最大 buffer 数等）由生产消费双方共同决定。
- SurfaceFlinger **只在有活干时醒**（"wakes only when there's work to be done, not 60 times per second"，屏幕无更新时 vsync 分发关闭）——事件驱动而非帧驱动。
- gralloc：分配器接 usage 位（CPU/GPU 访问频率、纹理、编码器用途）——硬件布局自由（swizzle/重排）全由 usage 表达，软件只见 RGBA。**usage flags 就是「约束协商」的单边版本**。
- **BLAST（Android 12 起）**：app 自己 acquire buffer，把**每个 buffer 包成一个 SurfaceControl transaction（ASurfaceTransaction）**提交给 SF——buffer 提交与几何/属性变更原子化、app 侧自管理 buffer 队列（减 IPC 往返）、SF 从同步 acquire/release 路径中退出变纯异步。NDK Choreographer 文档同步注明现代 SF 按 **frame timeline**（desiredPresentTime）节流事务与 buffer latch【高】（https://developer.android.com/ndk/reference/group/choreographer ）。

### 3.2 vsync 分发：DispSync + Choreographer + 相位偏移（「安卓响应慢」的解剖）

【高】（官方 vsync 文档实读：https://source.android.com/docs/core/graphics/implement-vsync ）

- HWC 从 `HAL_PRIORITY_URGENT_DISPLAY` 优先级线程回调 vsync（典型 <0.5ms 延迟）；SF 锁定后即关硬件 vsync（`setVsyncEnabled` 门控），漂移才重开——**硬件 vsync 只用于校准，不用于分发**。
- **DispSync = 软件锁相环（PLL）**：以 retire fence 时间戳为反馈，建模硬件 vsync 的频率与相位，派生两个偏移信号——**VSYNC（给 app/Choreographer：读输入+渲染下一帧）与 SF_VSYNC（给 SF：开始合成下一帧）**。
- **相位偏移的延迟数学**（官方原文）：无偏移时 vsync N 显示第 N 帧、SF 合成 N+1、app 产 N+2——**"The latency is at least two frames"**（两帧起步的固有输入→显示延迟，这就是「安卓渲染响应慢」的结构性根源之一）；偏移让一帧的合成与下一帧的 app 渲染重叠，压掉一帧延迟，代价=时间预算收紧、掉帧概率升高（"offsets shrink the time budget and raise the odds of missed frames"）。偏移参数按重负载保守调，>1-2ms 延迟即可感知。
- Choreographer（app 侧，https://cs.android.com/android/platform/superproject/main/+/main:frameworks/base/core/java/android/view/Choreographer.java ）="coordinates the timing of animations, input and drawing"——**输入/动画/绘制三件事同拍调度**（一次 doFrame 内按序处理），这是 Android 对「输入与渲染错拍」的最终答案。

**教训（对应用户裁决③「吸取安卓渲染响应慢的教训」）**：
1. 两段式流水（app 渲→合成→显示）天然 ≥2 帧延迟；**合成唤醒与渲染唤醒错相**是标准解，BerryOS 软 vsync 分发要同构：给 UI 进程的回调早于给合成器的回调一个相位。
2. **输入处理必须与帧回调同拍合并**（Choreographer 形态），否则输入到画面永远多一拍。
3. 硬件 vsync 仅校准用、分发走软信号——BerryOS ramfb 无真 vsync，软 vsync 起步不只是妥协，恰是 Android/Fuchsia/OH 三家的真实运行形态（都是软件生成分发）。

### 3.3 HWC overlay 决策模型

【高】（官方 HWC 文档：https://source.android.com/docs/core/graphics/hwc ）

- SF 把**整层列表**交给 HWC，HWC 逐层标 device（overlay 直扫）/client（GPU 合成）；SF 只做 client 部分并交结果给 HWC。决策代码厂商定制——「能力难以用 API 表达」（overlay 数量/旋转/混合/重叠限制逐器件不同），所以干脆给全列表让 HWC 自己划。
- 典型 4 个 overlay plane，超出即 GPU 合成——**应用层数直接影响功耗**。
- **静态屏的省电反转**：静态+透明像素+多层重叠时 overlay 反而耗电，HWC 可请求 GPU 合成一次并**缓存合成结果**，同 buffer 组再来直接复用——空闲设备电池优化的关键件。

**教训**：overlay 决策是「设备专有策略」，协议只传「全层列表+询问」不传能力描述——BerryOS 未来 overlay（virtio-gpu 2D set scanout 同族）应同构：显示服务全知全列表，设备服务自己决策；静态帧缓存合成结果是低功耗的一等杠杆（移动优先裁决④的对应件）。

---

## 4. QNX Screen：车载实时低延迟 GUI

（既有基线：`research/06-QNX-Neutrino.md`——QNX 内核/IPC/调度已详查，本节只补 Screen 图形子系统；闭源，证据=官方文档树）

### 4.1 进程模型与关键路径

【高】（官方 Screen 文档：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.screen/topic/manual/cscreen_utilities_services.html ；windowing/window groups：https://qnx.com/developers/docs/7.1/com.qnx.doc.screen/topic//manual/cscreen_windowing-groups.html ；input：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.screen/topic/manual/cscreen_input.html ）

- **客户端/服务器模型**：`screen` 服务进程提供 "a composited windowing system"——**所有应用渲染都画在离屏 buffer**，之后由合成器更新显示；窗口=「组合来自多个渲染者的内容」，渲染者可同进程、可跨进程。这把「应用渲染」与「屏幕更新」解耦——应用的渲染速度与显示刷新互不拖死（车载多屏/多客户关键）。
- 显示侧走 DRM 家族驱动（`drm-*`）+ OpenWF Display 栈（`wfd-server` 资源管理器）——display 控制器独立于 GPU 栈。
- **context = 进程内 Screen 会话句柄**：事件与 context 绑定、在应用主循环里取；异步通知可用 sigevent（QNX 消息传递体系的事件原语）。输入事件由 Screen 分发回 client context。
- buffer：window/stream 可挂多个 buffer，pixmap 单 buffer；跨进程共享 buffer/资源是一等能力（Screen DCK/平台 API）。
- **Wayland-on-Screen**：Screen 附带核心 Wayland 通信库的 C 实现——车载供应商生态经 Wayland 协议接进 Screen 合成。**车载实时标杆最终也在吸收 Wayland 协议语义**，是 §6 谱系收敛的又一佐证【中】（该组件存在性为文档事实，动机为推断）。
- 低延迟来源【中】（docs + 架构推断）：合成常驻单点（无 X 式两级跳）、渲染/合成/显示三段 buffer 解耦、QNX 消息传递优先级继承（既有调研 §3.1/3.2）保证合成线程的实时档位。官方不公开内部时序数字（既有调研同结论）。

### 4.2 window group：安全域隔离的 GUI 形态

【高】（window groups 文档实读）

- group = 父窗 + ≥1 子窗，**对外呈现为单一窗口**、父内各自独立；任何窗口可标 `SCREEN_ROOT_WINDOW`——**root 负责消费 group 内全部窗口内容，Screen 只在 root 合成、不再下探**（root 的父看不到 root 的子）。
- 跨进程加入：父进程把窗口 ID（系统唯一串）外发，子进程任意位置 `screen_join_window_group(child, name)` 加入——加入后子窗口从父窗口继承显示属性（坐标系相对上一级窗口）。**跨进程内容组合是 API 一等公民**（「renderer 建窗、另一应用加入」的文档用例）。
- 这是仪表/ivi 域隔离的实用形态：安全域只暴露一个 root 窗口给全局合成，域内子窗口的布局/层级完全域内自决——**「域内自治 + 域外单窗」**。权限细则在独立的 Permissions/Security policies 章（未在本次抓取范围，标注待核）。

**教训**：多进程 GUI 的安全边界单位是「**窗口组**」而非「窗口」——BerryOS v2 多 GUI 进程/安全域（仪表风格）时的窗口模型参照；v1 单 GUI 进程用不上，但 wmd 的窗口表设计留「组」缝（window 有 parent，组根对上有单呈现位）成本极低。

---

## 5. OpenHarmony Rosen：本地源码走读（架构层）

基线：`~/Documents/harmony/openharmony/graphic_graphic_2d`（master 浅克隆 46a11e9，2025-09-13）。功能面见规划稿附录 B，这里只看三件事的**架构实现**。

### 5.1 双侧树 + RSCommand 录制回放

【高】（源码直读）

- UI 进程侧 `render_service_client/core/ui/rs_node.cpp`：RSNode 家族是**服务侧 RSRenderNode 的本地镜像代理**；一切属性操作被打成 `std::unique_ptr<RSCommand>`（如 `RSBaseNodeDestroy`/`RSSetNodeName`/`RSAnimationCancel`）投给 transaction（`transaction->AddCommand(command, IsRenderServiceNode())`）。节点是否服务侧节点（IsRenderServiceNode）决定命令走服务树还是自绘树——**同一命令通道服务两种消费方**。
- 命令序列化模板 `rosen/modules/render_service_base/include/command/rs_command_templates.h`（参数 tuple marshal 进 parcel）；绘制内容录成 DrawCmdList（`modules/2d_graphics/include/recording/draw_cmd_list.h`）跨进程回放。
- 服务侧按 pid 建专属连接（`render_service/core/pipeline/main_thread/rs_render_service.cpp:187`），命令到达后 unmarshal（`rs_unmarshal_task_manager.cpp`）回放到服务侧镜像树。

**取舍**：保留树双侧镜像 + 命令流 = 「UI 进程不碰像素、渲染全在服务进程」——获得 GPU 集中/动画服务端执行/统一合成；付出 parcel 级 IPC 高频流 + 双树一致性维护 + 调试黑盒（命令流难 introspect）。OH 自己在轻量档（ui_lite/单进程）不这么做——**这是多进程多 GPU 时代的形态，不是 GUI 的必要形态**。

### 5.2 HDI 显示契约三口（+ fence 口）

【高】（`rosen/modules/composer/hdi_backend/include/hdi_device.h` 直读）

```
class HdiDevice {
    virtual int32_t RegScreenVBlankCallback(uint32_t screenId, VBlankCallback callback, void *data) = 0;
    virtual int32_t PrepareScreenLayers(uint32_t screenId, bool &needFlushFb) = 0;
    virtual int32_t Commit(uint32_t screenId, sptr<SyncFence> &fence) = 0;
    virtual int32_t CommitAndGetReleaseFence(uint32_t screenId, ...) = 0;  // 带释放 fence 的提交
    ...
}
```

- 显示驱动 HDI 契约 = **vblank 回调注册 + 层准备（问询式：要不要回退 framebuffer）+ 提交（带 fence）**。层列表整表进出（HWC 同款「全列表+询问」模型，§3.3）。
- 提交线程独立：`core/pipeline/hardware_thread/`（RSHardwareThread：`CommitAndReleaseLayers(OutputPtr, layers)`、独立 PostTask 队列）——**显示提交与渲染线程解耦**，渲染慢不拖已就绪层的上屏【高】（rs_hardware_thread.h 直读）。
- RS 主线 vsync 驱动：`rs_main_thread.h:483 OnVsync(timestamp, frameCount, data)`——帧管线（收命令/更新树/渲染/交硬件线程）以 vsync 为拍。

### 5.3 vsync 专用通道

【高】（`rosen/modules/composer/vsync/src/vsync_distributor.cpp` 直读）

- 每客户端连接 = **一个 LocalSocketPair fd**（`GetReceiveDataFd` 给客户端 poll/epoll）；发信号 = 一次 24 字节写（`data[3]` = timestamp/period/vsyncCount）。**跨进程 vsync 递送只有一次 socketpair 写**——不走 IPC 框架（binder）、零 marshal、O(1)。
- **请求驱动**：客户端必须 `RequestNextVSync()` 才收下一拍（`vsync_distributor.h` 直读）——空闲客户端不收 vsync、不被唤醒（省电语义与 Wayland frame callback 的「注册才发」同构，比 Android 早期「谁醒着都发」干净）。
- 背压：`SendData` 返 EAGAIN（客户端没及时消费）→ **丢最早一拍再重发** + 警告日志（PostEvent 直读）——vsync 是最新值语义，允许丢拍。
- 生成侧：VSyncGenerator 进程内线程软生成（`vsync_generator.h` 的 std::thread + period/referenceTime），真硬件 vblank 到来时校准——**软 vsync 为主、硬件校准为辅**，与 DispSync 同思路。

### 5.4 Rosen 在谱系里的位置

Rosen = Fuchsia Flatland 的「双侧树+命令流」形态 + Android 的 BufferQueue/HWC/vsync 体系 + 独立显示提交线程，全部塞进一个「渲染服务进程」。它证明这套东西在工程上成立（商用出货），也演示了代价：服务进程巨大（render_service + composer + vsync + 动画 + 截图…同进程系）、IPC 消息密度高、v1 阶段的 BerryOS 没有任何一项收益是「图标+滑动+状态栏」需要的（规划稿 §3-C 裁决同向）。**值得整体平移的不是它的进程形态，而是它的三个协议件**：HDI 三口（显示驱动最小契约）、vsync 专用通道（request-driven + 最新值语义 + 独立提交线程）、BufferQueue 的 acquire/release fence 语义（`CommitAndGetReleaseFence` 名字里就写着）。

---

## 6. GUI 服务化摆位谱系汇总与 BerryOS 摆位建议

### 6.1 四象谱系（按「谁渲染、内容怎么过界」分）

| 形态 | 代表 | 内容过界方式 | 每帧 IPC 成本 | 端到端延迟 | 复杂度 | 失败隔离 |
|---|---|---|---|---|---|---|
| A. 全部应用内 | ui_lite、Carnelian 单体、传统嵌入式 GUI | 不过界（同进程） | 0 | 最低（渲染即上屏） | 最低 | 单进程崩=全无（监督重启） |
| B. 合成器服务 + 薄协议（buffer 过界） | **Wayland/wlroots**、**Flatland**、QNX Screen、BLAST 后的 Android | **buffer 句柄 + damage + frame callback** | 低（一帧一次提交，消息小、数据零拷贝） | 中（1 帧：渲染→合成→显示可错相重叠） | 中 | 客户端崩=丢一层，合成器崩=监督重启全屏；协议面小可测 |
| C. 保留树双侧镜像 + 命令流（命令过界） | **OH Rosen**、Qt RHI 场景图族 | 属性/绘制命令录制回放 | 中-高（高频小消息流 + 序列化） | 中（命令往返+服务端渲染） | 高（双树一致性、unmarshal 栈） | 服务端渲染集中（GPU 统一/动画服务端）；单服务巨石化 |
| D. 远程渲染命令流（像素不出本地合成器，命令跨网络/容器） | Android View 系远程历史形态、X11 网络透明、RDP/VNC 族 | 完整绘制命令跨界限 | 高（命令带宽 + 同步往返） | 高 | 高 | 安全（客户端只见命令产物） |

五家证据在谱系上的落点：**Fuchsia 从 C（Gfx 场景图时代命令式+3D）走向 B（Flatland：buffer 中心、命令只剩场景图增量）**；**Android 从 B 的重 IPC 变体（BufferQueue 深度参与）走向更纯粹的 B（BLAST：buffer+状态一次原子事务）**；**OH 在完整档选 C、在轻量档选 A**——同一公司两档并存，说明这不是对错问题而是规模问题；**QNX Screen 是 B + 域聚合（window group）**；**Wayland 二十年坚守 B 并把协议压到最小**。**没有人选 D 做本地 GUI**（D 只在网络/容器桥接场景活着）。

谱系结论【高】：**B 是移动/通用 GUI 的收敛解**——buffer 过界使「内容传输」零拷贝、协议小到可验证、客户端渲染自由（软/GPU 都行）、失败域清晰。C 的收益（GPU 集中/服务端动画）只有多客户端竞争单 GPU 时才兑现，且可以**后置**：Flatland 本身就是 C 转 B 的现场。

### 6.2 BerryOS 摆位建议

维持规划稿方向（三薄服务 + 单 GUI 进程），按谱系语言重新表述并收紧契约语义：

1. **guifbd = B 形态合成器 + 显示服务**（v1 里客户端只有一个、但契约按 B 定形）：
   - 契约三件事：`submit(buffer/dirty_rects)` + `frame_callback(未来时刻表, credits)` + buffer 归还事件（release）。**从第一刀就把 damage/release/帧回调写成协议（berry-guiproto），不是函数调用习惯**——这样 GUI-8 拆多 GUI 进程、未来 virtio-gpu 后端时协议不动。
   - present credits：每客户端在途帧 ≤2（双缓冲）；channel 有界队列已有背压，语义显式化。
   - 静态屏不重绘、不合成（HWC 缓存教训 + tickless：无更新=无唤醒——移动优先裁决④）。
2. **gui-inputd 按 RFC-0096 骨架**：采集（virtio-input）→ 归一（InputReport→PointerEvent）→ **按 wmd 窗口表命中分发**；hit test 归 wmd/guifbd 侧（窗口表由 wmd 推送，OH WindowInfo 契约同构）——「谁在屏上」只有合成侧知道。输入与帧回调在 UI 进程内**同拍**处理（Choreographer 教训）。
3. **gui-wmd v1 保持薄**（两层窗 + Z 表 + 避让），但窗口对象留 `parent/group` 字段（QNX window group 缝）。
4. **vsync 分发件独立于 console 通道**：request-driven event（客户端 RequestNext 才收下一拍）+ 24B 级载荷（timestamp/period/count）+ 丢旧保新语义——OH socketpair 的 BerryOS 版 = 每客户端一对 event handle 或专用小通道；**给 UI 的拍与给合成器的拍错相**（DispSync 教训），软 vsync 下只是两个不同的定时时刻。
5. **显示驱动契约按 HDI 三口定形**（ramfb 实现 = RegVBlank(软) / Prepare(恒 layer) / Commit(fence→v1 空)）——virtio-gpu 到来时换实现不换契约。

---

## 7. buffer 跨进程模型：dmabuf / BufferCollection / BerryOS VMO

### 7.1 三者对照

| 维度 | Linux dmabuf | Fuchsia BufferCollection (sysmem) | BerryOS VMO 能力（现状+最小扩展） |
|---|---|---|---|
| 本体 | 内核对象，fd 用户态句柄 | 服务仲裁，VMO 句柄为产物 | 内核对象，handle 即能力【已有】 |
| 分配 | 各驱动自分配（dma_heap/驱动私有） | 中心服务，**约束交集协商**后分配 | guifbd/未来 bufd 服务分配【服务层，零内核面】 |
| 协商 | 无内建（feedback 扩展在 Wayland 层补） | token 复制传递 + 全员约束 + 一次分配 | v1 单产者：**服务定格式客户端接受/拒**；多进程时升 token 语义 |
| 授能传递 | fd 经 UNIX fd 传递（SCM_RIGHTS） | token/VMO handle 经 FIDL 通道 | **VMO handle 经 channel 随行/表传递**（fs gift 四元组先例，C-2c 已有） |
| 零拷贝 | 设备间共享物理页 | 同左 + CPU 不可访问堆（VMO 仅作钥匙） | vmar_map 双方映射同一 VMO【已有】；设备窗 VMO=MMIO 授能（A-2/GUI-1 已有） |
| 同步 | 内建 fence（dma-fence） | sysmem 不管同步；参与者用 event fence | **event handle 即 fence**（acquire/release=fuchsia 同构，§7.2） |
| 生命周期 | 引用计数（fd/映射/pin） | VMO 活到所有引用归零，异步销毁 | VMO 引用计数+handle 关闭【已有】； destruction 拒面已有（映射中 -EBUSY 先例） |

来源：dma-buf 内核文档（https://www.kernel.org/doc/html//v5.12/driver-api/dma-buf.html ）、sysmem overview（§1.4）、BerryOS CLAUDE.md/Gotchas（VMO 32MiB 帽、-EBUSY 拒面、fs gift 装配先例）。

### 7.2 GPU buffer 授能与 fence 在纯能力模型上的表达

**fence**：Fuchsia 的 fence 就是 `zx.Handle:EVENT`（§1.3 FIDL 一手证据），acquire/release 语义在事件对象+等待原语上即可完整表达——BerryOS 的 event（object_signal/object_wait_one/wait_many）语义同构【高】。v1 无 GPU：acquire fence 恒空（CPU 写完 buffer 才 submit），release fence = guifbd 的「buffer 可复用」事件（wl_buffer.release 同构）。**不需要任何新内核对象**。

**GPU buffer 授能**（未来 virtio-gpu/真 GPU）：
- 客户端持 VMO → 提交给 GPU 设备服务 → 服务把物理页 pin/映射进设备地址空间（virtio-gpu = attach resource backing）→ 命令流引用 resource id。Magma 证明此切分下**内核零 GPU 专有 syscall**（§1.5）。
- 授权传递 = VMO handle 过 channel（能力收窄：只给 MAP_READ/TRANSFER 而不给 WRITE——Zircon rights narrowing 同款，`research/05-Zircon-Fuchsia.md` §3.1 已核）。
- 恶意客户端防线 = 设备侧地址空间隔离（Magma 安全模型），不是验证命令流。

**BufferCollection 的 BerryOS 时机**：v1 单产者（合成器分配、客户端消费）不需要 token 协商——服务定约即可（wl_shm 世界里合成器也广播 format 表）；**触发升级的判据 = 出现第二个生产者或真 GPU 双写**（届时按 token 树 + 约束交集实现，语义可直接从 sysmem 抄，因为它本来就是能力模型的）。

---

## 8. 关键发现

（每条 = 断言 + 证据来源）

1. Fuchsia 放弃 3D 场景图转 2D 合成的根因：客户端全是 2D 产品 + 显示硬件 overlay 严格 2D 范式，3D 通用性是纯负资产——「表达力贴硬件范式」而非「图形学通用」。（fuchsia.dev roadmap/2021/flatland）
2. Flatland 帧协议的载荷是**未来 presentation 时间戳表（≤8 项）+ 提交配额（credits）**，客户端倒排工作；此前客户端靠硬编码偏移猜时机是官方点名的 miss 帧根因。（RFC-0162 + flatland.fidl 一手源码）
3. acquire/release fence 在 Fuchsia 上就是 Zircon event 句柄数组——fence 不需要专有内核对象，事件+等待原语即可完整表达。（flatland.fidl 字段注释）
4. sysmem BufferCollection = 「token 能力复制传递 + 全员约束 + 交集一次分配 + VMO 句柄分发」，且**只管分配不管同步**——分配与同步正交。（fuchsia.dev sysmem overview）
5. Magma 的内核面 = 通用原语（VMO/event/channel/KOID/MMIO 窗）零 GPU 专有 syscall；GPU 隔离靠设备侧硬件地址空间而非命令验证（验证被明确判定 impractical）。（RFC-0198）
6. Fuchsia 输入管线把 hit test 和焦点链都归 Scenic：任何其他组件不得查询屏幕内容（防 clickjacking 结构保证）；input pipeline 是唯一可直读驱动输入的组件（反键盘记录器）。（RFC-0096）
7. Wayland 的最小完备协议面 = surface 双缓冲状态 + attach/damage/commit 原子应用 + wl_buffer.release 归还 + frame callback（合成器主导背压、可对离屏面停发=隐式省电）。（wayland-book ch6/8）
8. wl_shm（CPU 共享内存、普适）与 linux-dmabuf（设备内存零拷贝 + format/modifier feedback 协商）的二分，在 BerryOS 上映射为「普通 DRAM VMO vs 设备窗/专用堆 VMO」——VMO 已同时可表达两者。（wayland-book ch6 + dma-buf 内核文档 + sysmem heap 概念）
9. Android 无偏移时输入→显示固有 ≥2 帧延迟（官方原文 "The latency is at least two frames"）；解法 = DispSync 软 PLL 派生错相的 app/SF 双 vsync + Choreographer 把输入/动画/绘制并成一拍——「安卓渲染响应慢」的结构性根源与标准解都在这。（source.android.com implement-vsync）
10. BLASTBufferQueue 把 buffer 提交从 SF 同步消费路径里拿出来，变成 app 侧每 buffer 一个原子事务（buffer+几何一起原子应用）——与 Wayland commit 原子性同向收敛。（source.android.com arch-bq-gralloc + AOSP BLASTBufferQueue.cpp + NDK Choreographer 文档）
11. HWC 决策模型 = 「整层列表给设备、设备逐层标 device/client、GPU 只补 client 部分」；静态屏时反向选 GPU 合成并缓存结果反而省电；硬件 vsync 只用于校准（锁定即关）。（source.android.com hwc/implement-vsync）
12. QNX Screen = 合成式窗口系统单服务进程 + 应用全部画离屏 buffer + window group「域内自治/域外单窗」安全聚合 + root window 合成下探截止；且附 Wayland-on-Screen 桥——车载实时标杆也在吸收 Wayland 语义。（qnx.com Screen 文档树）
13. OH vsync 专用通道实测形态：每客户端一个 socketpair fd、24B 载荷（ts/period/count）、request-driven（客户端显式请求下一拍）、EAGAIN 丢旧保新——vsync 递送刻意绕开 binder IPC 框架。（graphic_graphic_2d vsync_distributor.{h,cpp} 直读）
14. OH HDI 显示契约最小面 = RegScreenVBlankCallback + PrepareScreenLayers + Commit(+GetReleaseFence) 三口，提交在独立 hardware thread——渲染线程与显示提交解耦。（hdi_device.h / rs_hardware_thread.h 直读）
15. OH Rosen 的双侧树+RSCommand 命令流是「多进程+GPU 集中」时代的形态，其自身轻量档（ui_lite）不用；BerryOS v1 单 GUI 进程选它没有一项收益兑现——应对齐的是它的三个协议件（HDI 三口/vsync 通道/fence 语义）而非进程形态。（rs_node.cpp 直读 + 规划稿 §3-C）
16. 服务化摆位谱系收敛：**buffer 过界的 B 形态（Wayland/Flatland/BLAST/Screen）是移动 GUI 收敛解**，命令过界的 C 形态（Rosen/Gfx）收益只在多客户端竞争单 GPU 时兑现且可后置，像素命令跨界的 D 形态无人在本地 GUI 采用。（§6 五家证据汇总）
17. GUI-1 首刀（guifbd）已在进行中：fb MMIO 窗 VMO 授能 + 几何 40B VMO 随行 + FBW 自验三步——VMO 授能路线已开工，本报告的协议化建议可以无损叠加。（user/bins/guifbd/src/main.rs 直读）

## 9. 对 BerryOS 的架构启示

1. **guifbd 契约从第一刀就按「B 形态三件事」协议化**（submit(buffer,dirty_rects) / frame_callback(未来时刻表+credits) / release 事件），做成 berry-guiproto crate（fsproto 四家先例）——单客户端时不增加成本，多 GUI 进程与 GPU 后端到来时协议不动，是整个谱系给出的最大免费期权。
2. **damage day-one 不只是渲染优化，是协议字段**：submit 必须带脏区矩形；未来 overlay 直扫、脏区就是层更新粒度（Wayland damage/Flatland 层语义同源）。
3. **帧回调升级为「未来时」协议**：回调载荷 = 预测的未来 presentation 时刻（软 vsync 周期表）+ 在途帧配额；UI 进程倒排动画/布局/渲染。v1 软 vsync 下即可实现，这直接回应「吸取安卓教训」——比脉冲式回调高一个信息档。
4. **双 vsync 错相**：给 UI 进程的拍早于给 guifbd 合成的拍一个相位（如周期的 1/3），输入→渲染→合成重叠推进，把 2 帧固有延迟压向 1 帧——DispSync 数学在软 vsync 下照搬。
5. **输入与帧同拍**：berry-ui 帧循环把「取输入事件→跑动画→布局→渲染」并成一次 doFrame（Choreographer 形态）；输入命中由 wmd 窗口表在 gui-inputd/wmd 侧完成，「屏上有什么」的知识只存在于合成侧。
6. **vsync 分发件 = request-driven event 通道**：客户端显式 RequestNext 才收拍（空闲 GUI 进程零唤醒，tickless 同向）；24B 级载荷 + 丢旧保新；绝不复用 console/通用 RPC 通道（OH socketpair 先例 + PM 共享环纪律同构）。
7. **显示驱动契约三口定形**（reg_vblank/prepare/commit+fence），ramfb 是第一个实现、virtio-gpu 是第二个——契约不变换实现。
8. **静态屏零工作**：无 damage 提交 = 不合成不重绘 + 帧回调停发（Wayland 节流 + HWC 缓存双教训）——低功耗裁决的 GUI 侧执法点。
9. **buffer 授能零新内核面**：VMO handle 过 channel + rights 收窄（MAP 只读给合成器）即 GPU buffer 授能全解；fence = event handle（acquire 空/release 显式），Magma 拓扑（设备服务持硬件、客户端库进应用、内核只给通用原语）是 virtio-gpu 时代的直接蓝图。
10. **BufferCollection 分阶段**：v1 服务定格式（单产者），出现第二生产者/真 GPU 双写时再上 token 协商——sysmem 语义本身就是能力模型的，届时可直接平移，不用现在预建。
11. **窗口对象留 group/parent 缝**：v1 两层窗，但 WindowInfo 结构带 parent 字段（QNX window group 谱系缝），多安全域（车载/多用户形态）时免协议 break。
12. **berry-gfx/berry-ui 做 crate 库而非服务内置**（wlroots/Smithay 教训）：GUI 服务的可替换性来自「公共地基是库」；这同时保住「AI 测试友好」——库可 host 离屏测（fbcon 28 测先例）。

## 10. 开放问题

1. **virtio-gpu 在 QEMU aarch64 virt + AAVMF 下的可用性**（virtio-mmio 总线挂 virtio-gpu-device 2D 路径、ramfb 与 virtio-gpu 并存的固件行为）——本报告未验证，属独立调研项；它是「HDI 三口 + overlay 语义」从纸面变实测的前提。
2. ramfb 无真 vsync/flip：帧回调的「未来时刻表」在无硬件校准时如何自我校正（帧实际呈现时刻不可测）——v1 可只发预测不回确认（OnFramePresented 缺席），but 长期帧节奏漂移需实测定案。
3. 软 vsync 的拍源：timer 精度（TCG 2x tick 效应）下 16ms 周期的实际抖动，以及 UI/合成错相两拍的 TCG 可行性——需 GUI-3/8 刀实测。
4. 多 GUI 进程拆分的触发判据与窗口 buffer 池容量核算（800×600×4≈1.9MB/帧在 32MiB VMO 帽内宽裕；1080p×3 层双缓冲 ≈25MB 逼近帽——届时帽值或分层池需 A1 容量工程联动）。
5. release 语义与 v1 无 GPU 现实的最小化边界：v1 是否需要 acquire fence 字段占位（协议留位 vs YAGNI）——建议 FIDL 式 table 字段号留位（add-only），不实现。
6. OH DVSync（vsync_distributor.h 中 `RS_ENABLE_DVSYNC` 条件编译的延迟补偿机制）工作原理与价值——浅克隆可见其存在，未深挖；与 BerryOS 软 vsync 的帧延迟补偿可能相关，值一读（graphic_graphic_2d rosen/modules/composer/vsync/）。
7. 输入的「手势 arena」跨进程语义（parallel dispatch 的胜负裁决时机）在 BerryOS 单 GUI 进程 v1 内是纯库问题，多进程时协议怎么表达——Fuchsia gesture arena 的跨进程部分（RFC-0096 提及）未拿到一手细节。

---

## 参考资料清单

**Fuchsia（官方一手）**
- Flatland 2D Composition roadmap：https://fuchsia.dev/fuchsia-src/contribute/roadmap/2021/flatland
- RFC-0162 Flatland API：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0162_flatland
- Flatland 概念页：https://fuchsia.dev/fuchsia-src/concepts/ui/scenic/flatland
- flatland.fidl 一手源码（main 分支实抓）：https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.ui.composition/flatland.fidl
- RFC-0198 Magma API：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0198_magma_api_design ；porting：https://fuchsia.dev/fuchsia-src/development/graphics/magma/concepts/porting
- sysmem overview：https://fuchsia.dev/fuchsia-src/development/graphics/sysmem/concepts/sysmem
- RFC-0096 User Input Architecture：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0096_user_input_arch
- RFC-0166 One UI Stack：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0166_ui_stack
- Focus chain：https://fuchsia.dev/fuchsia-src/development/graphics/scenic/concepts/focus_chain
- Fuchsia Internals Vol.I（第三方深度报告，作者自注自动整理需核对）：https://www.hdlfactory.com/post/2026/08/25/fuchsia-internals-vol-1-graphics-and-display

**Wayland**
- The Wayland Book（frame callbacks 章实读）：https://wayland-book.com/surfaces-in-depth/frame-callbacks.html
- commit 语义补丁讨论：https://lore.freedesktop.org/wayland-devel/1448056199-19061-2-git-send-send-derekf@osg.samsung.com
- damage 释疑：https://stackoverflow.com/questions/58471743/what-is-a-damage-request-in-wayland
- wl_dmabuf weston 补丁（2014）：https://lists.freedesktop.org/archives/wayland-devel/2014-January/012727.html
- linux-dmabuf feedback：https://forge.quantum5.ca/quantum/wayland-protocols/src/commit/b8ef271689ffc0a0f7a511e5ec469d88352e9f29/stable/linux-dmabuf/feedback.rst
- 内核 dma-buf：https://www.kernel.org/doc/html//v5.12/driver-api/dma-buf.html
- wlroots 立项：https://www.phoronix.com/news/Wayland-wlroots ；Smithay：https://github.com/Smithay/smithay ；HN 公共地基讨论：https://news.ycombinator.com/item?id=46382610
- Daniel Stone 演讲讨论：https://www.reddit.com/r/linux/comments/18065e/the_real_story_behind_wayland_and_x_daniel_stones

**Android（官方）**
- BufferQueue/gralloc：https://source.android.com/docs/core/graphics/arch-bq-gralloc
- VSync/DispSync：https://source.android.com/docs/core/graphics/implement-vsync
- HWC：https://source.android.com/docs/core/graphics/hwc
- Choreographer NDK：https://developer.android.com/ndk/reference/group/choreographer
- BLASTBufferQueue.cpp：https://android.googlesource.com/platform/frameworks/native/+/master/libs/gui/BLASTBufferQueue.cpp
- 中文深读【中】：https://juejin.cn/post/7527239817937174569 、https://zhuanlan.zhihu.com/p/1917988013490800529

**QNX Screen（官方文档）**
- Graphics services：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.screen/topic/manual/cscreen_utilities_services.html
- Window groups：https://qnx.com/developers/docs/7.1/com.qnx.doc.screen/topic//manual/cscreen_windowing-groups.html
- Input：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.screen/topic/manual/cscreen_input.html

**OpenHarmony（本地源码直读，Apache-2.0 只看思路）**
- `~/Documents/harmony/openharmony/graphic_graphic_2d/rosen/modules/composer/vsync/{include,src}/vsync_distributor.{h,cpp}`、`vsync_generator.h`
- `~/Documents/harmony/openharmony/graphic_graphic_2d/rosen/modules/composer/hdi_backend/include/hdi_device.h`
- `~/Documents/harmony/openharmony/graphic_graphic_2d/rosen/modules/render_service/core/pipeline/{main_thread/rs_main_thread.h, hardware_thread/rs_hardware_thread.h}`
- `~/Documents/harmony/openharmony/graphic_graphic_2d/rosen/modules/render_service_client/core/ui/rs_node.cpp`

**BerryOS 内部**
- `docs/GUI里程碑规划-2026-10.md`（附录 B 功能清单与 §3 裁决候选）
- `research/05-Zircon-Fuchsia.md`（handle/rights/VMO/能力模型既有调研）
- `research/06-QNX-Neutrino.md`（QNX 内核/IPC/调度既有调研）
- `user/bins/guifbd/src/main.rs`（GUI-1 首刀在途：fb VMO 授能链 + FBW 自验）
- `CLAUDE.md`（VMO 32MiB 帽/-EBUSY 拒面/fs gift 先例/PM 共享环纪律）
