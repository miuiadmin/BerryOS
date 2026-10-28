# GUI-03 先进渲染 API 全景与适配 —— 「兼容≠适配好」案例库 + BerryOS 后端序列裁决输入

> 调研日期：2026-10-28。服务对象：M-GUI 里程碑（`docs/GUI里程碑规划-2026-10.md`）+ 用户 2026-10-28 六条裁决（最先进渲染方案 / 移动设备基准 / 安卓响应慢教训 / 低功耗且优秀 / 先进 API 兼容但原生适配才算数 / 最先进开发套件）。
>
> 方法：联网调研（WebSearch/WebFetch，30+ 独立来源，关键论断随文标注 URL）+ 本地源码/文档（`docs/GUI里程碑规划-2026-10.md`、`user/berry-fbcon/`、`user/bins/guifbd/`、`drivers/berry-virtio/`、CLAUDE.md 工程事实）。每条关键论断标注确定度：**[高]** = 官方文档/一手 issue 实证；**[中]** = 可信二手/社区证据或时效敏感；**[低]** = 单源推测，需复核。

---

## §1 API 全景：五家速览

| API | 出身/年份 | 平台覆盖 | 移动端现状 | 一句话哲学 |
|---|---|---|---|---|
| **Vulkan** | Khronos，2016（1.3 2022-01，1.4 2024-12） | Android/Linux/Windows/Switch，macOS/iOS 经 MoltenVK | **Android 事实唯一低层 API**，GLES 退场中 [高] | 显式到牙齿：内存/同步/队列全手动，驱动只做校验与提交 |
| **Metal** | Apple，2014（Metal 4 = WWDC 2025-06） | 苹果独占 | iPhone/iPad 全系 GPU 的原生 API；TBDR 亲和设计 [高] | 显式但克制：render pass + load/store action day-one，无全局状态机 |
| **WebGPU** | W3C gpuweb，Chrome 113 首发桌面 2023-04 / Android 121（2024-01）；Firefox 141（2025）、Safari 26（2025-09）跟进；**2025-11 全主流浏览器到位** [高] | 所有浏览器 + 原生（wgpu / dawn / wgpu-native） | 移动浏览器已可用；原生侧 wgpu 支持 Vulkan/Metal/D3D12/GL | Vulkan/Metal/D3D12 的「安全子集」：显式管线/命令录制，但同步交给用法域隐式推导 |
| **DirectX 12** | Microsoft，2015 | Windows/Xbox | 与移动无关（引用对照用） | 首波显式 API 之一；根签名/PSO/栅栏 |
| **OpenGL ES** | Khronos，2003 起 | 全平台历史覆盖 | **退场中**：Android 侧被 ANGLE→Vulkan 翻译接管，新能力冻结 [高] | 全局状态机 + 运行时驱动魔法 = 移动端卡顿与碎片化的历史根源 |

### 1.1 Vulkan：Android 必备化时间线（确定度高，多源官方）

- **Android 10（2019）CDD：64 位设备必须支持 Vulkan 1.1**（[Android 11 CDD PDF](https://source.android.com/static/docs/compatibility/11/android-11-cdd.pdf) 沿袭该条款；[Android 官方 Vulkan 架构页](https://source.android.com/docs/core/graphics/arch-vulkan)）[高]
- **Android 15：chipset 起步要求抬到 Vulkan 1.3**（Khronos 官方 profile：[VP_ANDROID_15_requirements.json](https://github.com/KhronosGroup/Vulkan-Profiles/blob/main/profiles/Android/VP_ANDROID_15_requirements.json)）[高]；Mishaal Rahman 报道 Android 15 要求 UI 渲染默认走 Vulkan（[X 帖](https://x.com/MishaalRahman/status/1752063453227798926)）[中，转述口径]
- **Android 16 minimums 已在讨论 Vulkan 1.4 scope**（[VP_ANDROID_16_minimums.json](https://github.com/KhronosGroup/Vulkan-Profiles/blob/main/profiles/Android/VP_ANDROID_16_minimums.json)、Google 工程师 Ian Elliott 主导）[高]
- Google 在 Vulkanised 2025 的口径：「Vulkan is the future——更低 CPU 开销与功耗」，Android Baseline Profile 2021/2022 逐年抬底（[Vulkan on Android 讲稿 PDF](https://www.vulkan.org/user/pages/09.events/vulkanised-2025/T15-Ian-Elliott-Google-Vulkan-on-Android.pdf)）[高；百分比数字未能取回 PDF 正文，不下具体数]
- **GLES 的退场路径 = ANGLE**：Android 14 Beta 3 起 Pixel 有「以 ANGLE 作 GLES 驱动」开关（[Mishaal Rahman](https://x.com/MishaalRahman/status/1667221544173547520)），Android 15 继续推进（[Krita 社区对 Android 15 OpenGL 退场的讨论](https://krita-artists.org/t/opengl-dropped-from-android-15/93447/11)）；ANGLE 内存占用仍在向原生 GLES 驱动对齐（[ANGLE issue 42262141](https://issues.angleproject.org/issues/42262141)）[中]

### 1.2 Metal：苹果独占但设计领先

- 2014 年首发即带 **render pass + loadAction/storeAction** 语义——比 Vulkan 的 render pass 早两年、比 Vulkan「好用」整整一代（[Wikipedia: Metal (API)](https://en.wikipedia.org/wiki/Metal_(API))）[高]
- **Metal 4（WWDC 2025-06）**：统一命令编码器体系、mesh shaders、模块化**着色器编译 API**（编译时机/场所可控——正是 Impeller 式 AOT 的平台化）、神经渲染、快速资源加载（[Apple 官方文档](https://developer.apple.com/documentation/metal/using-the-metal-4-compilation-api)、[MoltenVK #2560 讨论](https://github.com/KhronosGroup/MoltenVK/issues/2560)）[高]
- 对 BerryOS 的意义：Metal 证明「显式 API 可以不折磨人」；其编译时机可控化与 BerryOS「杜绝运行时编译抖动」直接同向。

### 1.3 WebGPU：标准化状态与落地

- **2025-11 起全主流浏览器**：Chrome/Edge 113+、Firefox 141+（2025 年中默认开启）、Safari 26（2025-09，macOS Tahoe 26/iOS 26 默认）——[web.dev 官方博客](https://web.dev/blog/webgpu-supported-major-browsers)、[gpuweb Implementation Status wiki](https://github.com/gpuweb/gpuweb/wiki/Implementation-Status)、[Wikipedia: WebGPU](https://en.wikipedia.org/wiki/WebGPU)、[caniuse](https://caniuse.com/webgpu)、[zircon.tech 对 Firefox 141/Safari 的报道](https://zircon.tech/blog/webgpu-just-got-real-what-firefox-141-and-upcoming-safari-mean-for-ai-in-the-browser) [高]
- 三个**独立实现**：Blink/dawn（Google）、Gecko/**wgpu**（Rust，Firefox 与 wgpu-native）、WebKit 自研（Safari）[高]
- 着色语言 **WGSL** 三引擎全部落地；规范 + 一致性测试套件（CTS）由 gpuweb 维护 [高]
- 原生使用（InfoQ「WebGPU is Not Just about the Web」及社区讨论）：dawn 追规范更快、wgpu 刻意保守贴合 Web 安全语义（详见 §4.4）

### 1.4 D3D12 / OpenGL ES（简评）

- D3D12 与 BerryOS 无交集，仅作「显式 API 第一波」对照；值得一提：D3D12 后来也补了 optional 的 render pass 特性（`BeginRenderPass`/preserve-clear-discard 语义）服务 tiled GPU——render pass 语义是全行业收敛点这一判断的又一旁证 [中]
- GLES 是「兼容 ≠ 适配好」的历史主角：运行时着色器编译（首帧卡顿）、驱动端全局状态机、各厂商实现质量离散——它的退场本身就是行业判决。

---

## §2 「最先进」判断标准与排序

四个标准逐一打分（★=强，☆=弱；确定度基于规范文本与官方文档 [高]，个别项为社区共识 [中]）：

| 标准 | Vulkan 1.4 | Metal 4 | WebGPU | D3D12 | GLES 3.2 |
|---|---|---|---|---|---|
| 显式控制（内存/同步/队列） | ★★★ 最极端：分配器/描述符集/屏障全手动，多队列+timeline semaphore | ★★ 显式但托管：内存经 heap/heapless 抽象、同步靠 pass 边界与 fence，队列少而清 | ★★ 管线/绑定组/命令录制显式；**同步隐式**（用法域推导），v1 单默认队列 | ★★★ 与 Vulkan 同级（根签名/PSO/栅栏） | ☆ 全局状态机，驱动猜意图 |
| 多线程录制 | ★★ 命令缓冲无线程归属，但描述符/同步手工成本高 | ★★ encoder 天然线程安全，Apple 官方鼓励多 encoder | ★★ 原生实现对象可跨线程（wgpu `Send + Sync`；浏览器侧 Worker 分治） | ★★ command list 录制可并行 | ☆ 单上下文 + 共享上下文补丁 |
| render pass 感知 | ★★ 1.0 起 render pass + load/store op + subpass + transient（LAZILY_ALLOCATED）；1.3 dynamic rendering 把它变可选但语义保留 | ★★★ pass 是 API 骨架，memoryless（瞬态附件）一等公民 | ★★ pass + loadOp/storeOp 是核心；瞬态附件未进 v1 核心（提案中）[中] | ★（后补 optional） | ☆ 无概念，靠驱动 tile 化 |
| GPU 架构中立 | ★★（对 IMR/TBDR 都可表达，但 render pass 用错就性能塌方——中立性靠开发者自律） | ★★（为 TBDR 而生，但在 AMD/Intel IMR Mac 上也跑——证明 pass 模型跨架构可行） | ★★（pass 语义 + limits 能力协商，对 TBDR/IMR 双友好且**防呆**） | ★（为 IMR 桌面而生） | ★（驱动代劳，代价是失控） |

**排序结论（按「先进性 = 表达力上限 × 语义健康度」综合）**：

1. **Vulkan**——能力天花板最高（计算/光线追踪/多队列/显式同步全都在），且是唯一在移动端大规模强制的低层 API；但它的「先进」要求使用者付出同步与内存管理学费。
2. **Metal 4**——语义设计最优雅的显式 API（pass 语义 day-one、编译时机可控化），代价是苹果独占。
3. **WebGPU**——**语义健康度第一**：继承了三家显式 API 的形态（管线、绑定组、命令缓冲、pass/load-store），砍掉了易错面（手动屏障、裸内存），且是唯一带 CTS 一致性套件与三独立实现的规范。表达力上限低于 Vulkan（无手动同步、单队列、能力经 limits 协商），但**每一条砍掉的都是 Android 卡顿/驱动碎片化的病灶**。
4. D3D12 与 Vulkan 同代同档，平台无交集。
5. GLES——历史 API，正在被其自家生态（ANGLE）翻译到 Vulkan 之上。

> **判断**：产业收敛点不是「谁最底层」而是「**pass 形状 + 显式管线 + 能力协商 + 无运行时魔法**」——这四件 Metal 有、Vulkan 1.3+ 补齐、WebGPU 规范化。BerryOS 的 API 面应该长这个形状（§6 展开）。[推断，基于上述证据]

---

## §3 移动 GPU 架构如何塑造现代 API

### 3.1 IMR vs TBDR 分型

- **IMR（立即模式渲染）**：桌面 NVIDIA/AMD 传统路线——逐图元直接写 framebuffer，带宽换简单性。（旁注：NVIDIA Maxwell/Pascal 实际用了 tile-based immediate 的缓冲技巧，界限在模糊（[Level1Techs 讨论](https://forum.level1techs.com/t/nvidia-maxwell-pascal-borrowing-tricks-from-mobile-tile-based-rendering/105037)，[中/投机级]））
- **TBDR（瓦片延迟渲染）**：PowerVR 与 Apple GPU 的正统——先 binning 全场景，**片上隐藏面剔除（HSR）后才着色**，混合在 tile memory 里完成、末了才回写主存（[Imagination 官方架构文档](https://docs.imgtec.com/starter-guides/powervr-architecture/html/index.html)、[Rys Sommefeldt: PowerVR TBDR](https://rys.sommefeldt.com/post/powervr-tbdr-deferred-rendering)）[高]
- **TBR/瓦片化（binning）**：Mali 与 Adreno——逐 tile 光栅化（省带宽），但**不**做完整 HSR（overdraw 仍在，靠 early-Z/Forward Pixel Kill 类机制缓解）；Adreno 俗称「灵活渲染」，同为 tile 家族。[中，厂商口径综合]
- 四家移动 GPU（Adreno/Mali/PowerVR/Apple）**全部**是 tile 家族——移动端唯一主流形态；这是省电约束（外部 DRAM 访问是移动 GPU 第一功耗项）的直接产物。[高]

### 3.2 tile 架构对 API 语义的塑造（为什么现代 API 长这样）

- **render pass 边界 = tile 边界**：pass 内 attachment 留在片上 tile memory；loadOp（CLEAR/DONT_CARE/LOAD）与 storeOp（STORE/DONT_CARE）决定是否产生主存往返——用错 = 白白多两次全屏带宽（PowerVR 文档 + [Vulkan 官方移动优化教程](https://docs.vulkan.org/tutorial/latest/Advanced_Vulkan_Compute/12_Mobile_and_Embedded_Compute/03_mobile_optimization.html)）[高]
- **transient attachment / tile memory**：Metal `memoryless`、Vulkan `LAZILY_ALLOCATED`/transient attachment——深度/中间色从不落主存；WebGPU v1 尚未暴露（提案中）[中]
- **破坏 pass = 冲刷 tile**：pass 中段的 barrier/回读/别名切换强制 tile 落主存再重载——移动端性能悬崖。Meta 官方文档把「假设立即模式语义的算法」（帧中回读、频繁切 pass）列为移动 GPU 反模式（[Mobile GPUs and impaired algorithms](https://developers.meta.com/horizon/documentation/native/android/gpu-impaired-algorithms)）[高]
- **subpass**（Vulkan）= 显式声明「这两个 pass 级操作可共 tile」，GPGPU/后处理链在移动端的正确写法。

### 3.3 「Vulkan 越来越像 Metal」的证据链

1. **render pass + load/store op**：Vulkan 1.0（2016）即引入——比 Metal 晚两年，且当时的对象模型（VkRenderPass/VkFramebuffer 显式创建）更重；这是 tile 架构倒逼的直系产物。
2. **VK_KHR_dynamic_rendering（Vulkan 1.3 核心）**：干掉显式 render pass/framebuffer 对象，`vkCmdBeginRendering` 直接开画——官方动机就是「让 API 更 dynamic、更灵活、少样板」（[Khronos 博客 Streamlining Render Passes](https://www.khronos.org/blog/streamlining-render-passes)、[扩展提案文档](https://docs.vulkan.org/features/latest/features/proposals/VK_KHR_dynamic_rendering.html)）[高]。但注意社区共识：**tile 架构上 load/store 语义仍然必要**，dynamic rendering 只是换了个表达位置（[r/vulkan 讨论](https://www.reddit.com/r/vulkan/comments/1labb1o/should_i_keep_using_render_passes_or_move_to)）[中]
3. **synchronization2 + timeline semaphore**：把 Vulkan 最臭名昭著的手写屏障域收敛成 Metal 级的可读形态。
4. **Metal 4 的「统一命令编码器 + 编译时机可控」**继续把「显式而人体工学」推向新位——Vulkan 在追、WebGPU 生而如此。

> **结论**：Tile 架构是现代显式 API 语义的共同塑造者；「pass + loadOp/storeOp + 瞬态资源」不是可选优化，而是移动 GPU 上的**正确性-功耗基线**。任何以移动设备为设计基准的渲染抽象，第一天就得是 pass 形状。[高，多源收敛]

---

## §4 「兼容≠适配好」案例库（本报告核心）

> 用户原话关切：先进渲染 API 要能兼容，但「兼容了不等于适配好」，原生适配才算数。以下六类案例是业界用真金白银换来的证据。

### 4.1 Vulkan on Android 驱动碎片化：「API 在场 ≠ 驱动可用」

| 案例 | 症状 | 来源 |
|---|---|---|
| Filament（Google 自家 3D 库）Pixel 3/Adreno 法线贴图渲染错误 | 同一份代码 Adreno 崩、Mali 正常——厂商 shader 编译差异 | [filament #1102](https://github.com/google/filament/issues/1102) [高] |
| Mali-G76 `VK_DEVICE_LOST` | 帧缓冲丢失类崩溃，需按 Mali tiler 特性重排顶点缓冲规避 | [Arm 社区论坛](https://community.arm.com/forums/f/mobile-graphics-and-gaming-forum/49770/do-we-need-to-repack-our-vertex-buffers-for-mali-g76-to-avoid-vk_device_lost) [高] |
| wgpu 特定硬件渲染伪影 | 2024 年仍在新硬件上出现的驱动级画质劣化 | [wgpu #6340](https://github.com/gfx-rs/wgpu/issues/6340) [高] |
| Vulkan 不必然更快：Adreno 750/730 上 Vulkan 的 CPU/内存开销**高于** GLES | 引擎被迫做 API 双轨 | [facebook/igl #333](https://github.com/facebook/igl/issues/333) [高] |
| Organic Maps 运行时切换 Vulkan↔OpenGL 规避驱动差异 | 生产应用维护双渲染路径 | [organicmaps #2946](https://github.com/organicmaps/issues/2946) [高] |
| ShaderTest：同族 shader 在 ARM(Mali) vs Qualcomm(Adreno) 上检测渲染不匹配 | 驱动 bug 猎手工具的存在本身即证明 | [A tale of two Samsungs](https://medium.com/hackernoon/a-tale-of-two-samsungs-arm-vs-qualcomm-in-android-graphics-c1c6f1eef828) [中] |

**教训**：即使 Vulkan 是「单一标准 API」，Android 上的真实形态 = **N 家驱动 × 版本矩阵 × 各自的 bug 面**。业界的应对是：能力协商（profiles/limits）+ per-vendor quirk 库 + 双 API 回退 + 崩溃遥测。Khronos 用 Android Baseline Profile（2021/2022）逐年抬底、Google 用 GMS 认证强制，正是制度化的「兼容 ≠ 适配好」治理（[Vulkanised 2025 Google 讲稿](https://www.vulkan.org/user/pages/09.events/vulkanised-2025/T15-Ian-Elliott-Google-Vulkan-on-Android.pdf)、[VP_ANDROID_15_requirements.json](https://github.com/KhronosGroup/Vulkan-Profiles/blob/main/profiles/Android/VP_ANDROID_15_requirements.json)）[高]。

### 4.2 MoltenVK：翻译层继承目标的基因缺陷

- **版本天花板**：上报 Vulkan 1.2.275，不支持 1.3（[MoltenVK #2292](https://github.com/KhronosGroup/MoltenVK/issues/2292)）[高；时效敏感]
- **特性名存实亡**：`robustBufferAccess` 标记支持但社区注释「(not really)」；bindless texture 在 1.2.10 有 bug 需 shader 规避（[#1215](https://github.com/KhronosGroup/MoltenVK/issues/1215)、[#2300](https://github.com/KhronosGroup/MoltenVK/issues/2300)）[高]
- **结构性缺口**：Metal 无 geometry shader，MoltenVK 不仿真——引擎用 compute 改写（[Stride3D 论坛](https://forums.stride3d.net/t/geometry-shaders-on-moltenvk-opengl-es/1920)）[高]；Metal heap 内存上限、LLVM/驱动级崩溃亦见诸生产报告（Elder Scrolls Online Mac 版曾约每日一崩 + 文字模糊，[官方论坛帖](https://forums.elderscrollsonline.com/en/discussion/574037/mac-moltenvk-problem-report-and-hope-for-the-future)）[中]
- **结论**：MoltenVK 让 Vulkan 代码「能跑」，但 robustness/bindless/新版本特性三处都要当心——**跨 API 翻译层的能力下界由目标 API 的短板决定**，性能与稳定性还要再折一层。[高]

### 4.3 ANGLE（ES→Vulkan 翻译）：一次成功，但代价是 Google 级投入

- 成功面：Chrome 的 ES→D3D11 后端十年稳定；Android 上 Google 把 ANGLE 当**系统级 GLES 实现**推进（Android 14 起开关、Android 15+ 持续，见 §1.1）——在受控矩阵上，翻译层可以升格为系统层 [高]
- 代价面：内存占用高于原生 GLES 驱动、仍在调优（[ANGLE 42262141](https://issues.angleproject.org/issues/42262141)）；Android 专属工程（preRotation、EGLConfig 格式补全）常年占用 issue 列表 [中]
- **结论**：翻译层能赢的前提 = 单一强力维护者 + CTS 级验证纪律 + 对目标后端的完全掌控。「借 ANGLE 思路兼容 GLES」对 BerryOS 无必要——我们没有 GLES 包袱，这是后发优势。

### 4.4 WebGPU 三实现差异：有 CTS 也有行为漂移

- **dawn vs wgpu**：dawn（Chrome）追规范与新特性更快；wgpu 刻意保守、贴 Web 安全语义——原生使用时被批评「lowest common denominator」（Web 做不到的原生也做不到）（[Reddit r/gameenginedevs 讨论](https://www.reddit.com/r/gameenginedevs/comments/1hp44q8/any_thoughts_on_native_webgpu_like_wgpu_or_dawn)、InfoQ 报道）[中]
- **资源销毁/回收语义**在两实现间存在差异（InfoQ 报道指出的关键不同点之一）[中]
- **Safari 26 = 第三家独立实现**（非 dawn/wgpu）——三实现意味着真实世界差异面，CTS 收窄但不归零 [高]
- **软后端缺口**：wgpu 官方曾因 lavapipe（Mesa 软 Vulkan）**合格性不足**而将其排除在支持列表外、后经讨论恢复（[wgpu #3622 Reconsider lavapipe](https://github.com/gfx-rs/wgpu/issues/3622)）[高]——连 Rust 生态自己的参考实现在软后端上都栽过「兼容≠适配好」。

### 4.5 Flutter Impeller / Skia：多后端矩阵成本与「运行时编译 = 卡顿」的世纪教训

- **病根**：Skia(Ganesh) 在 GL 后端**运行时生成着色器变体** → 首帧/滚动首条卡顿（early-onset jank），iOS 上尤甚；这是「Android 渲染响应慢」教训里可归因到渲染 API 层的一块（[Flutter 官方 Impeller 文档](https://docs.flutter.dev/perf/impeller)、[flutter #183495 设计文档原文](https://github.com/flutter/flutter/issues/183495)）[高]
- **Impeller 解法**：AOT 着色器预编译（构建期全量）+ 直接 Metal/Vulkan 后端（不经 Skia 的后端抽象）——Flutter 3.4x 起 Android 现代设备上 Skia 已被移除、Impeller Vulkan 唯一（[Level Up Coding 报道](https://levelup.gitconnected.com/flutter-just-removed-skia-from-every-modern-android-device-impeller-vulkan-is-now-mandatory-b52de5038587)）[中]
- **代价与残留**：旧设备保留 Impeller-GLES 回退路径，**回退路径 bug 长期无人踩**（渐变渲染错只在该路径触发，[flutter #179268](https://github.com/flutter/flutter/issues/179268)）[高]；Windows 上启动变慢（启动期编译着色器换运行时零卡顿，[flutter #191860](https://github.com/flutter/flutter/issues/191860)）[高]——AOT 的成本只是搬家，不是消失。
- **Skia Graphite / Chrome 的同类裁决**：Skia 新后端 Graphite **砍掉自家 native Metal 后端、只留 Dawn 单一抽象**（Chromium issue 原文：「Since support for the Graphite native Metal backend was removed, the only supported backend is Dawn」，[#40063538](https://issues.chromium.org/40063538)）；Android 上 Graphite 走 Dawn-Vulkan 且**只在 Ganesh-Vulkan 已验证稳定的设备群放开**（[#40945084](https://issues.chromium.org/issues/40945084)）[高]
- **结论**：Google 自己从「一个引擎维护 N 个全量后端（Ganesh：GL/GLES/Vulkan/Metal/D3D）」撤退到「一个便携层（Dawn）+ 少数原生后端」——**多后端全量自研不是产业方向，单一语义层 + 逐后端原生落地才是**。[高]

### 4.6 「一个抽象层摊 N 个后端」工程教训汇总

| # | 教训 | 案例锚点 |
|---|---|---|
| 1 | 抽象层会漏：每后端 bug 面在抽象层「平均化」，用户表现为随机画质/崩溃 | wgpu #6340、filament #1102 |
| 2 | 最低公分母风险：为全后端一致而砍能力，原生优势发挥不出来 | wgpu「lowest common denominator」批评 |
| 3 | 回退路径腐烂：不常跑的 fallback 后端 bug 只在被用户踩到时发现 | flutter #179268（GLES 回退渐变 bug） |
| 4 | 翻译层继承目标短板：能力上界由底层决定，不是由你的抽象决定 | MoltenVK #1215/#2300、wgpu-lavapipe #3622 |
| 5 | 「同一标准」≠「同一行为」：即使有 CTS，实现差异仍在（销毁语义/进度/软后端合格性） | WebGPU dawn vs wgpu vs WebKit |
| 6 | 运行时编译是移动端卡顿的第一源头：AOT 是产业共识解 | Impeller 全案、Metal 4 编译 API、ANGLE 预热 |
| 7 | 后端矩阵 = 测试矩阵：没有 per-backend 的自动验证纪律，多后端就是多倍的静默错 | Skia Graphite 按设备白名单灰度（Google 也要这样做） |

---

## §5 虚拟 / 无 GPU 环境真实现状（BerryOS 验证地形）

### 5.1 virtio-gpu 三模式 + QEMU 官方口径

[QEMU 10.x 官方文档](https://qemu.readthedocs.io/en/stable-10.1/system/devices/virtio-gpu.html)（直接抓取）[高]：

| 模式 | 机制 | 关键要求/限制 |
|---|---|---|
| **2D（默认）** | 纯 paravirt 2D（resource create/attach、set scanout、flush）；guest 3D 靠**guest 内软光栅**（llvmpipe/lavapipe/SwiftShader） | 开箱即用；资源驻 guest 内存，flush 脏区送 host 合成 |
| **virgl（`-gl` 变体）** | guest GL → virglrenderer（host）转 host GL | host 需 virglrenderer 库；aarch64 有历史黑屏 issue（[QEMU #1141](https://gitlab.com/qemu-project/qemu/-/issues/1141)，时效未复核 [中]） |
| **venus（Vulkan）** | Vulkan 命令序列化协议，guest Mesa venus 驱动 ↔ host virglrenderer≥1.0 | 需 `hostmem`（256M–8G）+ `blob=true` + `venus=true`；guest 必须有完整 Mesa venus 栈；QEMU 上游支持 2024-10 合入（[Phoronix](https://www.phoronix.com/news/VirtIO-GPU-Vulkan-QEMU)、[Collabora 博客](https://www.collabora.com/news-and-blog/blog/2021/11/26/venus-on-qemu-enabling-new-virtual-vulkan-driver)、[Mesa venus 文档](https://docs.mesa3d.org/drivers/venus.html)）[高] |
| **rutabaga/gfxstream** | host 端 rutabaga 组件渲染（gfxstream-vulkan/cross-domain；Android 用 x-gfxstream-gles/composer） | 需 hostmem blob；构建链在 crosvm 生态（QEMU 文档明言不覆盖） |

**传输层事实（对 BerryOS 关键）**：非 VGA 变体（`virtio-gpu-device`）**支持 virtio-mmio**，与 BerryOS 现有 blk/net 同总线；VGA 变体与 vhost-user-gpu 不支持 MMIO——BerryOS aarch64 virt 路线 = `virtio-gpu-device`（mmio）2D 模式起步。[高]

### 5.2 软光栅三家现状

- **llvmpipe**（Mesa 软 GL）：Mesa 官方称「最快软光栅器」，LLVM JIT；实践报告对 SwiftShader 约 4 倍吞吐（[Mesa 文档](https://docs.mesa3d.org)、[Microlink 实测博客](https://microlink.io/blog/webgl-without-a-gpu)）[高]
- **lavapipe**（Mesa 软 Vulkan）：2025-01 已暴露 **Vulkan 1.4**；自报「非合格实现」警告（[Phoronix](https://www.phoronix.com)、[Mesa 文档](https://docs.mesa3d.org)）[高]；wgpu 曾因合格性问题拒收后恢复（[wgpu #3622](https://github.com/gfx-rs/wgpu/issues/3622)）[高]
- **SwiftShader**：**Chrome 已弃用**（GPU 进程内 JIT 的安全风险 + 软渲染体验差），WebGL 上下文创建将直接失败；Edge 144 起需 `EnableUnsafeSwiftShader` 策略（[Chromium swiftshader.md](https://chromium.googlesource.com/chromium/src/+/main/docs/gpu/swiftshader.md)、[chromestatus](https://chromestatus.com/feature/5166674414927872)、[Edge 策略文档](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-policies/enableunsafeswiftshader)）[高]
- **对 BerryOS 的含义**：① 软渲染是产业一等路线而非降级（OpenHarmony RASTER 后端同判，见 `docs/GUI里程碑规划-2026-10.md` 附录 B L0「渲染后端三分支 + 纯软回退」）；② Mesa 移植（llvmpipe/lavapipe）对 BerryOS 不现实——LLVM 依赖巨重且是 C 栈，自研软光栅（fbcon 路线）是对的；③ SwiftShader 的退场说明「软后端也要有安全与质量纪律」。

### 5.3 UTM / macOS Apple 虚拟化

- 旧况：Apple Virtualization.framework 的 GPU 加速长期仅限 macOS 12+ ARM Linux guest，且稳定性问题不断（[UTM #4871 v4.1.4 修 GPU 崩溃](https://github.com/utmapp/UTM/discussions/4871)、[#3076 无加速显示](https://github.com/utmapp/UTM/issues/3076)）[中]
- **新况（关键增量）**：**macOS 26 Tahoe + UTM 5.0：Linux guest 经 Mesa Venus 驱动获得 Vulkan 1.3 加速**（host 侧走 Apple 虚拟化 + Metal 底座）（[UTM 5.0.0 Beta 发布说明 #7578](https://github.com/utmapp/UTM/discussions/7578)、[farzon.org 技术分析 2026-04](https://blog.farzon.org/2026/04/enabling-gpu-api-virtualization-on-utm.html)）[高]
- 即：**virtio-gpu + Venus 已在 Apple 平台商用化**——BerryOS 若未来跑在 UTM 且实现 Venus 协议端，可获得真 Metal 加速；但 Venus guest 侧 = 完整 Mesa Vulkan 驱动栈，对 BerryOS v1/v2 是重工程（见 §6.4 裁决）。

### 5.4 QEMU aarch64 virt 地形小结（BerryOS 实际跑道）

| 选项 | 可用性 | 备注 |
|---|---|---|
| GOP/ramfb 直写 fb（现役，V-6a 已实测 800x600） | ✅ 在用 | 无 flip/vsync、整屏刷新、分辨率固定——GUI-1 已在收口（`user/bins/guifbd/src/main.rs`） |
| **virtio-gpu-device（mmio）2D** | ✅ 可用 | scanout/flush/脏区、支持动态分辨率与 display event；与 `drivers/berry-virtio` 框架同构——**推荐的下一站** |
| virgl（GL 加速） | ⚠️ 历史坑 | host 需 virglrenderer；aarch64 有黑屏历史 issue；guest 侧需 Gallium 协议栈——性价比低 |
| venus（Vulkan） | ⚠️ 重 | blob+hostmem 前提 + guest Mesa venus 全栈；TCG 下收益存疑；留观察 |
| guest 内 llvmpipe 级软渲染 | ✅ 已在走 | fbcon 自研路线（`user/berry-fbcon`，host 28 测） |

**Cuttlefish 先例**（Google 官方 Android 虚拟设备）：走 crosvm + virtio-gpu + gfxstream/rutabaga 路线做 GPU 虚拟化（[Cuttlefish 官方仓库](https://android.googlesource.com/device/google/cuttlefish/+/refs/tags/android-15.0.0_r7/README.md)、[GPU.md](https://gitee.com/Fishand/android-cuttlefish/blob/master/GPU.md)）[中]——**移动 OS 在无真机环境下用 virtio-gpu 家族验证图形栈，是 Google 自己验证过的路线**，BerryOS 选 virtio 总线做图形外设与产业同构。

---

## §6 BerryOS 裁决输入

### 6.1 渲染后端抽象层：自研薄 HAL（WebGPU 语义子集）vs 直接绑 wgpu

**推荐：自研薄 HAL，语义对齐 WebGPU 子集；不把 wgpu 当 HAL。** 理由四条：

1. **语义选 WebGPU 子集 = 站在产业收敛点上**（§2/§3 结论：pass + loadOp/storeOp + 显式管线 + 命令录制 + limits 能力协商）。这套语义 Metal 验证了 12 年、Vulkan 1.3 收敛、三浏览器 2025 全落地——「最先进且被验证」，且天然 TBDR-friendly（移动设计基准）。
2. **wgpu 当 HAL 的三处不适配**（都是「兼容≠适配好」的 BerryOS 版）：
   - 资源模型表达不了 VMO：BerryOS 纹理/缓冲的第一需求是 **VMO 能力授予、跨进程零拷贝**（微内核纪律），wgpu 的 `Device/Texture` 没有这个口；
   - 后端矩阵错位：wgpu 的后端是 Vulkan/Metal/D3D12/GL，BerryOS 需要的后端是 **软光栅 / virtio-gpu-2D / (远期)Vulkan**——套 wgpu 等于在 wgpu 的 N 后端下再垫自研后端，双层抽象、最低公分母双倍生效（§4.6 教训 2/4）；
   - 依赖面：wgpu 带来的完整依赖树与 BerryOS 服务形态（no_std 起步、自有 libc）暂不匹配。
3. **但语义兼容 wgpu/WebGPU 本身**：API 命名/概念/limits 对齐后，未来「把 wgpu 移植到 BerryOS HAL 之上」（用户态 std 面已具备——T-1 线程/TLS 已落地，CLAUDE.md）或「应用直接用 WebGPU 形状写渲染代码」都是顺路而非翻新。这正是用户「先进 API 要能兼容，但原生适配才算数」的落地形态：**兼容在语义层，适配在后端层**。
4. 产业平行案例：OpenHarmony Rosen 的「后端三分支 + RASTER 软回退」、Google 的「Dawn 单抽象 + 原生后端」、Impeller 的「直 Metal/Vulkan」——全部是「一薄层 + 逐后端原生」，无一例外（`docs/GUI里程碑规划-2026-10.md` 附录 B L0；§4.5）。

### 6.2 推荐后端序列（软 → virtio-gpu 2D → 真机 GPU）

| 阶段 | 后端 | 增益 | 成本 | 裁决建议 |
|---|---|---|---|---|
| **B0（现役，GUI-1..3）** | fbcon 软光栅 + 脏区（GOP/ramfb） | 可验、可控、host 可测 | TCG 吞吐有限 | **永久保留**：作为一致性基准后端 + 所有高层（视图树/手势/动画）的宿主——软后端不是过渡品，是验收基线（§4.6 教训 3 的制度化解法） |
| **B1（M-GUI 后段或 v2 首刀）** | **virtio-gpu 2D（`virtio-gpu-device`，virtio-mmio）** | 真 flip/scanout/脏区 flush、动态分辨率、display event（可做准 vsync）、host 合成减 guest 搬运 | 一个新设备类驱动（`drivers/berry-virtio` 扩展，blk/net 先例） | **推荐主攻**：QEMU/UTM 双环境同栈（§5.3/5.4）；2D 模式开箱即用零 host 依赖 [高] |
| **B2（真机线）** | 真机 GPU 原生 Vulkan（Mali/Adreno） | 真 3D/合成加速、真功耗账本 | 用户态 Vulkan 驱动 = 持证应用（驱动模型在册），TBDR 纪律全量生效 | 远期主线；HAL 的 pass/loadStore 语义就是为这一天预付的（§3） |
| **支线（观察）** | venus/UTM-Metal、gfxstream | macOS 26 UTM 已商用 Vulkan 1.3（§5.3） | guest 需完整 Mesa venus 栈 | 不进产品路径，作「虚拟化真加速」技术观察位；若走 = 认可多一层翻译（§4.6 教训 4） |

**不推荐**：virgl（aarch64 历史坑 + Gallium 协议栈重）、移植 llvmpipe/lavapipe（LLVM 巨依赖）、SwiftShader 思路（连 Chrome 都在退役，§5.2）。

### 6.3 API 面留缝清单（「兼容但不牺牲原生适配」的具体针脚）

1. **Pass 形状 day-one**：`RenderPass { attachments, load_op, store_op }` 进 berry-gfx trait（GUI-2 六原语 trait 化时就把 2D 绘制包成 pass 语义——软后端「pass」退化为立即执行，零成本；GPU 后端直接映射）。**不要**把 API 长成 GL 形状（全局状态 + bind texture）——真机 TBDR 上那是带宽/功耗的自我伤害（§3.2）。
2. **命令录制面**：`CommandList` 概念保留（软后端 = 直接执行；记录/回放形态同时是未来多进程合成器「UI 进程录制、显示服务回放」的天然接口——OpenHarmony RSCommand/DrawCmdList 同构，规划文档已留此缝）。
3. **资源 = VMO**：`Texture/Buffer` 由 VMO 背书，跨进程授予走能力拷贝——软后端 = 借片直写 fb（`RootfsBuf::Borrowed` 同款纪律）；virtio-gpu 后端 = guest 内存资源（2D 模式资源天然驻 guest RAM，flush 脏区即 DMA——与脏区引擎严丝合缝）。
4. **AOT 管线描述**：v1 无着色器，但「绘制描述可序列化 + 可离线预编译」从第一天作为 API 不变量（Impeller 教训 §4.5）；将来 WGSL 子集 → 自有字节码，构建期编译、运行时零编译。
5. **能力表 + quirk 注册表**：`Caps { max_texture_size, formats, flip 支持, vsync 源… }` per-backend 上报 + 运行期 quirk 表可热更 + 遥测上报（Android Vulkan 碎片化的治理形态，§4.1；同时是「AI 测试友好化」支柱的渲染版）。
6. **limits 协商语义**对齐 WebGPU（`Limits` 结构 + `request device with limits` 的降级路径）——应用层代码未来平移 WebGPU 零重写。

### 6.4 验证纪律（防「兼容≠适配好」内化到 CI）

- **双后端共识对拍**：同一场景在软后端与 virtio-gpu 后端的像素级 golden 基准（host 离屏断言，GUI-2/3 既定路线的扩面）——基准重产义务必须机器守卫（mkfat `--check-fixture` 先例，CLAUDE.md P6 A-5 教训直接迁移）。
- **回退路径常跑**：软后端永远在 smoke 主相位（flutter #179268 教训——回退路径不常跑必腐烂）。
- **per-backend caps 差异测试**：能力表声明什么就测什么，声明没有的特性编译期拒绝（不静默模拟）。
- **帧路径确定性**：脏区合并/两遍布局/手势状态机全部 host 喂事件断言（规划文档 GUI-3/6 既定，与 bsh FakeSrc 同法）。

### 6.5 与「安卓渲染响应慢教训」「低功耗」的对线（用户裁决 ③④）

- **响应慢的三个渲染层病根与 BerryOS 对策**：① 运行时着色器编译（Impeller 案）→ AOT 管线描述（§6.3-4）；② 帧路径里的驱动/锁停顿 → 软后端零驱动调用 + 单 GUI 进程内无跨进程帧内 RPC（规划文档裁决 C）；③ 输入-渲染-提交无截止时间概念 → **帧 = 有 deadline 的事务**：软 vsync 驱动「输入→动画→布局→录制→提交」链，超 deadline 丢弃中间帧保最新（Choreographer 形态）；输入事件与帧首的耦合点即 **intent 消费面**——gui-inputd/launcher 声明交互 intent，AI 调度接口（在册 ADR #42）拿遥测做优先级/频率决策，这是「AI 原生调度 + GUI」的第一等结合点。
- **低功耗**：tile 纪律（pass/loadStore）= 真机功耗第一杠杆（§3.2）；模拟器侧 = 脏区驱动 + 无帧不醒（tickless 同族）+ vsync 对齐不空转；软后端在 TCG 下的降载预案（RGB565/降分辨率）已在规划文档 §5 预置。

---

## 关键发现

1. **Android Vulkan 必备化是十年单向门**：Android 10 起 64 位设备强制 Vulkan 1.1、Android 15 抬至 1.3（Khronos 官方 profile）、Android 16 在议 1.4，GLES 由 ANGLE→Vulkan 翻译接管退场（Khronos VP_ANDROID_15 requirements.json；Vulkanised 2025 Google 讲稿；Mishaal Rahman X）。移动低层 API 的答案唯一：Vulkan。
2. **WebGPU 2025-11 起全主流浏览器，三独立实现 + CTS**（web.dev；gpuweb wiki；Wikipedia）——「pass + 显式管线 + limits 协商 + 安全默认」成为唯一带一致性套件的现代渲染语义，是 API 语义设计的产业收敛点。
3. **移动 GPU 全员 tile 家族（Adreno/Mali/PowerVR/Apple），render pass/load-store/transient 不是优化而是功耗-正确性基线**（Imagination 官方文档；Meta 开发者文档；Vulkan 官方移动教程）；Vulkan 的演进（dynamic rendering/sync2）是在向 Metal 的人体工学收敛（Khronos 官方博客）。
4. **「兼容≠适配好」六类实证齐全**：Vulkan-on-Android 驱动碎片化（filament #1102、Mali-G76 VK_DEVICE_LOST、igl #333、organicmaps #2946）；MoltenVK 继承 Metal 短板（1.2 天花板、robustness 名存实亡、无 geometry shader）；WebGPU 三实现行为漂移 + wgpu 曾拒收 lavapipe 软后端（wgpu #3622）；Flutter 回退路径腐烂（#179268）；Skia Graphite 砍自研 Metal 后端只留 Dawn（Chromium #40063538）——Google 自己也从「N 全量后端」撤到「单薄抽象 + 原生后端」。
5. **运行时着色器编译 = 移动卡顿第一病灶，AOT 是产业共识解**（Impeller 全案；Metal 4 把编译时机可控化做进 API；flutter #191860 显示 AOT 只是把成本搬到启动期）。
6. **虚拟环境地形**：QEMU virtio-gpu 2D 模式（mmio 可用、开箱即用）> venus（需 blob+hostmem+Mesa venus 全栈，重）> virgl（aarch64 历史坑）；软件光栅产业线 = llvmpipe（Mesa 官方最快）+ lavapipe（软 Vulkan 1.4，自报非合格），SwiftShader 被 Chrome 退役（安全+体验）（QEMU 官方文档；Chromium swiftshader.md；Phoronix）。
7. **UTM/macOS 26 Tahoe 已商用「Linux guest Vulkan 1.3 via Mesa Venus + host Metal」**（UTM 5.0.0 Beta #7578）——virtio-gpu 家族在 Apple 平台的真加速通道已开，但 guest 侧代价 = 完整 Mesa venus 栈。
8. **Cuttlefish（Google 官方 Android 虚拟设备）用 crosvm + virtio-gpu + gfxstream 验证图形**——移动 OS 在模拟器里跑图形栈走 virtio-gpu 家族是被 Google 验证过的路线（Cuttlefish 仓库/GPU.md）。
9. BerryOS 现有资产（`user/berry-fbcon` 软光栅 28 测、`drivers/berry-virtio` mmio 框架、GOP 携面实测、VMO 能力模型、`user/bins/guifbd` GUI-1 雏形）与推荐路线（软 → virtio-gpu-2D → 真机 Vulkan）逐一对得上号，净新增 = virtio-gpu 设备类驱动一个。

## 对 BerryOS 的架构启示

1. **渲染 HAL = 自研薄层、语义对齐 WebGPU 子集（pass/loadOp/storeOp/命令录制/limits），不绑 wgpu 当 HAL**——兼容在语义层做，适配在后端层做；wgpu 未来作为应用层 SDK 移植目标保留（用户态 std 面已具备）。
2. **后端序列定为：软光栅（永久基准后端，CI 常跑防腐烂）→ virtio-gpu 2D（`virtio-gpu-device` @ virtio-mmio，flip/scanout/脏区 flush/display event）→ 真机 Vulkan（远期主线，pass 语义的兑现日）**；venus/UTM-Metal 作技术观察支线不进产品路径；不碰 virgl/llvmpipe 移植/SwiftShader 思路。
3. **berry-gfx（GUI-2）六原语 trait 化时就把 API 面做成 pass 形状**——软后端 pass 退化为立即执行零成本，真机 TBDR 上是带宽/功耗生死线；绝不做 GL 形状（全局状态机）抽象。
4. **绘制/管线描述可序列化 + 离线预编译从第一天作为不变量**（Impeller 教训）——v1 无着色器也把「运行时零编译」写进 API 契约。
5. **纹理/缓冲 = VMO 背书的能力对象**，跨进程零拷贝授予；virtio-gpu 2D 资源天然驻 guest RAM，与脏区引擎直接对接。
6. **每后端能力表（Caps/Limits）+ quirk 注册表 + 遥测**进 berry-gfx 设计（Android 驱动碎片化的治理形态 + AI 测试友好化的渲染版）；声明没有的能力编译期拒绝、不静默模拟。
7. **双后端像素级 golden 共识测试 + 基准重产机器守卫**（mkfat --check-fixture 先例迁移）；软后端永久留在 smoke 主相位。
8. **帧 = 带 deadline 的事务**：软 vsync 驱动「输入→动画→布局→录制→提交」，超时丢帧保最新；gui-inputd/launcher 的交互 intent 声明 = AI 原生调度（ADR #42）与 GUI 的第一个结合点。
9. **低功耗双线**：模拟器侧脏区驱动 + 无帧不醒（tickless 同族）；真机侧 pass/tile 纪律 + EPP/intent 联动改频。
10. **驱动即持证应用（驱动模型在册）**：virtio-gpu 驱动与未来 Vulkan 用户态驱动都走 `drivers/` 用户态服务形态，内核零新增面——微内核纪律不变。

## 开放问题

1. **QEMU venus 在 aarch64 virt + TCG 的实际可用性与收益**（blob/hostmem 支持时序、TCG 下 Vulkan 指令流模拟的帧率量级）——需实验定标；短期不阻塞主线。
2. **virgl 在 aarch64 新版 QEMU 是否已修复**（历史 issue #1141 未复核）——若 B1 后想加 GL 加速线再验。
3. **WebGPU 瞬态附件（memoryless/transient）未进 v1 核心**——berry-gfx 的 attachment 描述是否预留 transient 位，待 WebGPU 规范动向再裁。
4. **wgpu 作为应用层 SDK 的移植时机与成本**（std/线程面已具备，但 Vulkan 后端依赖整条驱动链）——M-GUI 后或真机线启动时评估。
5. **真机 GPU 基线选型**（Mali vs Adreno；以 ABP2022 还是 VPA15 作最低能力基线）——真机启动时裁。
6. **virtio-gpu display event 作准 vsync 源的质量**（分辨率变化通知/刷新事件粒度），以及无信号时软 vsync 帧率定标（TCG 实测）——GUI-1/B1 刀内定标。
7. **多进程窗口合成（gui-wmd 拆分）时录制/回放命令流的协议形态**（OpenHarmony DrawCmdList 同构）——v2 议，API 面已留缝。

---

### 附：来源索引（独立信息源计数）

联网（26）：web.dev WebGPU 博客 / gpuweb wiki / Wikipedia WebGPU / caniuse / zircon.tech Firefox141 / Android arch-vulkan 页 / Android 11 CDD PDF / Khronos VP_ANDROID_15 与 VP_ANDROID_16 profile / Mishaal Rahman X×2 / Vulkanised 2025 T15 PDF / Krita 论坛 / ANGLE issues×2 / filament #1102 / ARM 社区 / wgpu #6340 / igl #333 / organicmaps #2946 / Medium 两三星 / MoltenVK issues×4 + Stride3D 论坛 + ESO 论坛 / Khronos 博客 + VK_KHR_dynamic_rendering 提案 / r/vulkan / PowerVR 文档 + Rys Sommefeldt + Meta 文档 + Vulkan 教程 / Flutter 官方文档 + flutter issues×3 + Level Up 报道 / Chromium Graphite issues×2 + Chrome 博客镜像 / QEMU virtio-gpu 官方文档 + QEMU #1141 + virt 平台页 / Mesa venus + llvmpipe 文档 + Phoronix×2 + Collabora / Chromium swiftshader.md + chromestatus + Edge 策略 + Microlink / UTM discussions×3 + farzon.org / Cuttlefish 仓库 + GPU.md 镜像 / Wikipedia Metal + Apple Metal 4 文档 + MoltenVK #2560。

本地（5）：`docs/GUI里程碑规划-2026-10.md`（功能全景与架构裁决默认值）/ `user/berry-fbcon/src/lib.rs`（软光栅种子）/ `user/bins/guifbd/src/main.rs`（GUI-1 显示服务雏形与 VMO 授能链）/ `drivers/berry-virtio/`（mmio 驱动框架先例）/ CLAUDE.md（工程事实：V-6a GOP、VMO 32MiB 帽、T-1 std 面、P6 A-5 基准守卫先例）。
