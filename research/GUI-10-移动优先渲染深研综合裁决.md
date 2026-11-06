# GUI-10 移动优先渲染终局深研——综合裁决建议（2026-10）

> **定位**：M-GUI 线第二 wave 深度调研——在 2026-10-29 八维调研（`research/GUI-01..09`）+ ADR #50–#54 + BerryUI 全面自研追裁之上的**补刀式深研**，不推翻既有裁决；与既有裁决冲突处单列冲突项（C1/C2）供用户拍板；**同日晚已拍板（均路线 A，用户授权按「最优秀最优雅」原则代拍）——细目见 `docs/GUI里程碑规划-2026-10-v1.0.md` 修订记录与 ADR #52/#53**。
> **触发**：用户重申六原则——①最先进图形渲染方案；②手机单线走到底、手表=能力非锚点（ADR #39 现行口径重申）；③吸取安卓渲染响应慢教训；④又低功耗又优秀；⑤渲染 API 兼容但「兼容≠适配好」；⑥SDK 要「容易渲染、高效、正确」。
> **生成方式**：workflow 两跑——主跑 `berryui-mobile-first-research-mvstcxas-nyrurg`（9 agents：仓库基线×2 + 六角度 web 深研 + 综合员，1.9M tok）；w-sdk 角度首跑产出残缺（误报「已存 store」只回速览），补跑 `berryui-sdk-angle-rerun-mvsukavk-ucager` 得全文——正文 §7 综合时消费的是速览+基线 GUI-07，附录 F 补跑全文为权威版，两者结论一致、补跑证据更全。
> **配套维护**：`research/GUI-07` / `GUI-09` 已加「2026-10-29 全面自研追裁」追记（防误导）；现行口径单一事实源 = `docs/GUI里程碑规划-2026-10-v1.0.md` 修订记录 + `docs/架构决策.md` ADR #50–#54。

---

## 正文：综合裁决建议（综合员产出原貌）

# BerryUI 深度调研综合裁决建议（synthesis · 2026-10-29）

> 证据标注：【证】=一手来源可查（URL/仓库路径）；【推】=本人推断。【冲突项】单列两条路线供用户拍板，不改既有裁决。

## 一、执行摘要（12 条硬结论）

1. B 形态协议三件事与安卓十年修法（Choreographer→BLAST→FrameTimeline）及 Wayland fifo/commit-timing 逐一同构，方向被双体系独立验证——**#50 确认，无需修订**。【证】
2. 「兼容≠适配好」的结构实锤：掉性能的五层（驱动接口/分配/合成/DPU/一致性）没有一层住在 API 签名里——Fuchsia 押注 Dawn 七年仍 "Work in progress"（dawn support.md），Asahi 适配良好≈四年+个位数顶级专家（rosenzweig.io）。**语义边界自研定形是唯一正解，#51 成立。**
3. vello 官方已把全 compute 路线降级 `research/`、CPU/GPU 双后端共享 Sparse Strips 场景记录（linebender/vello ARCHITECTURE.md）——「场景记录一次、软硬两执行」有先例背书；自研光栅核内联 sparse strips 思想是有据主路。
4. tile GPU 的成本单位是 pass：附件应瞬时、禁冗余 store（ARM perfdoc）；dirty-rect 碎片化是功耗反模式——K≤8 大矩形+transient 语义 day-one 是功耗-正确性基线，不只是优化。【证】
5. 运行时着色器编译是移动卡顿第一病灶（Impeller 全案）；berry-gfx 必须补「着色器=构建产物、无运行时编译入口」纪律（#51 补充）。
6. 安卓核心教训=**一切省电默认都在向延迟收租**（相位保守取值/输入 batching/buffer stuffing）；低功耗六件必须配「快速起振路径+起振延迟可观测」。【证】
7. idle 功耗正解=0 帧管线 0 唤醒而非低帧率照跑（Weston 按需重绘十年实证）；「无 damage 不提交」应成为 guifbd 服务端不变量+smoke 断言（#53 补充）。
8. 高频信号三先例（Android BitTube fd 通道、Wayland frame callback 推送、QNX 有界队列 pull）一致表明：高频走专用 fd/共享环推送、低频控制走请求-响应——与本仓 IPC 设计 §7 同构。【证】
9. 声明式四家收敛于「属性级依赖记录+按依赖短路+编译期前移」（strong skipping 首屏重组 −20%，I/O'24 官方博客）；#52 的视图级→属性级路线成立，**属性级依赖记录骨架应 day-one 留缝**。
10. 载体证据：宏的致命短板=展开后 AI 不可读（xilem 因此弃宏）；文本 DSL 赢在独立预览器+AI 生成产物=数据（ArkTS/Slint）；解药=「声明即数据」AST 导出——**无论宏/文本，AST 导出应为硬要求**。
11. 软渲染定位对标 WARP：一致性/可测/CI 兜底，非性能叙事（learn.microsoft.com）——「软渲染是路线」的误读必须钉死。【证】
12. GUI-1 已落但协议面顺延；定形锚点（GUI-2/4 先到先定）前，berry-guiproto version 字段与 fence 完成语义留位应随首个消费者一次落位，避免 FB_DAMAGE_CLIPS 式「2017 年才补、后补≠自动生效」。【证】

## 二、端定位与路线确认：手机单线、手表=能力非锚点

【证】ADR #39–#41（docs/架构决策.md）：v1=手机 rich 档单线；#40 排 MCU。落到渲染面四条具体含义：

1. **协议数值按 phone 取，结构按通用取**：dirty K≤8、credits≤2、800×600 RGB32 起步皆为 phone 档值；vote 票面{min,max,preferred,scene,sticky}是区间结构——手表低帧率=区间取值不同，不是协议分支（Android setFrameRate=per-surface 意图+系统聚合仲裁先例，developer.android.com/reference/android/view/Surface【证】）。
2. **AOD/DIM/OFF 是手机一等场景**：AOD 在手机上即主流功能；power·mode 四档直接服务手机，手表「能力符合」=同一协议的子集消费。Wear OS 的 on-pixel≤15%、1 分钟一拍等只是场景预设值，不写进协议常量（developer.android.com/training/wearables/always-on【证】）。
3. **光栅核按 rich 档设计**：FPU/FPSIMD 可用（GUI-0 正是前置）、内存可分配、多核并行留位；Slint 264KB MCU 经验仅作逐行渲染思路，不背 KB 级包袱（research/GUI-04）。
4. **TCG 软渲染=验收手段非产品定位**：产品终态是 TBDR GPU——所以 pass/loadOp·storeOp/Caps/transient 必须 day-one（GUI-02、w-pipeline【证】），否则软后端特化语义会堵死真机路。

SDK 面同理：不带 embedded 排除项（Slint 对 embedded 收费/或 GPL 反证自研正确，slint LICENSE.md【证】）；验收锚全为 phone 场景（滑动翻页/launcher/AOD），smoke 不加穿戴分支。

## 三、安卓教训→BerryUI 结构性反清单

| # | 安卓教训（证据） | BerryUI 对策 | 挂点 |
| --- | --- | --- | --- |
| 1 | 主线程单线程模型，动画到 5.0 才离主线程（RenderThread.cpp@5.0） | UI/帧/输入三线分离，帧线程 intent 申报 | #52/GUI-3 确认 |
| 2 | vsync 相位按最坏负载保守取值=延迟稳定互收租（implement-vsync） | frame_callback 未来时刻表+相位参数可调 | #50 确认+相位进协议版本域【补】 |
| 3 | Buffer stuffing：错帧后所有帧至少晚一 vsync（frametimeline.md/BufferStuffing.md） | credits≤2 背压+错帧沿动画时间轴对齐 | #50 已裁，BLAST 同构实证 |
| 4 | 主线程同步 Binder 52ms 案例 | UI 线程零同步 RPC；帧信号走共享环不走 channel | IPC §7+GUI-4【补：显式入刀面】 |
| 5 | RelativeLayout 双遍测量协议病 | 布局自研单遍约束（Compose 立法同构） | #52 确认 |
| 6 | 4ms GC 停顿毁一帧；ART 花八年治 | Rust 无 GC=结构性免疫 | 全仓既有 |
| 7 | overdraw 官方预算 ≤2.5× 屏幕像素 | 遮挡剔除+矩形合并与 damage 同级验收 | GUI-3/GUI-9【补：断言】 |
| 8 | 输入攒批雪崩→resampling 修法（InputEventReceiver） | PointerEvent 带时间戳序列+重采样留位+「即到即派」低延迟模式留位 | GUI-4 确认+【补：即到即派】 |
| 9 | 运行时 shader 编译卡顿（Impeller 全案） | 着色器离线预编译 archive，无运行时编译入口 | #51【补】 |
| 10 | 掉帧不可归因→FrameTimeline 类型化 | 遥测双轨打点带 deadline-miss 分类 | GUI-3/GUI-9【补】 |
| 11 | touch boost 证明输入提频要走系统通道 | 交互脉冲→intent/EPP，GUI 只投票不设频 | #42/#53/GUI-4 确认 |
| 12 | 输入 batching 为省电晚半拍 | 三态停帧+快速起振+起振 1–2 帧延迟进遥测 | #53/GUI-3【补】 |

## 四、最先进渲染方案：ADR #50–#54 逐条确认/补充

**#50——确认。** 三件事各有十年先例：damage=DRM FB_DAMAGE_CLIPS；时刻表=wp_commit_timing_v1（"presented as closely as possible to, but not before"【证】）；credits=BLAST/BufferStuffing 修法。三项补充：① latch/squash 语义进 guifbd——Fuchsia frame_scheduling 的 latch（过点锁定、迟到顺延）+squashing（积压帧折叠、保底一帧）【证】与 credits≤2 折叠上限同构（ credits×折叠=推断映射）；② 服务端不变量「无 damage 不提交」+稳态零唤醒断言；③ per-interface version 字段随定形刀必落（Wayland global/bind 先例）。

**#51——确认+三补充。** ①着色器=构建产物：Impeller impellerc 全离线编译+blobcat archive、启动即建全部 PSO【证】；②transient attachment 语义 day-one 留位（TBDR tile memory 生命周期=pass，ARM perfdoc【证】）；③「同一场景记录、双后端执行」写进 berry-gfx 内部纪律（vello 共享 Sparse Strips 结构【证】），软硬切换不换语义。

**#52——确认+一补充。** 属性级依赖记录骨架 day-one 入框架核心（SwiftUI @Observable access 钩子/ArkUI @Trace/Compose 相位读取追踪，四家收敛【证】）；v1 失效粒度仍视图级，接口留缝给 v1.5 对拍；「声明即数据」AST 导出为硬要求（§7）。

**#53——确认+三补充。** ①稳态零唤醒 smoke 断言（Weston idle 零唤醒实证【证】）；②停帧→交互起振 1–2 帧额外延迟写进遥测（Weston repaint-scheduling 实证【证】）；③每图层帧率意图=呈现元数据位，与 vote 票面并存（**冲突项 C1**，见 §9）。

**#54——确认+两补充。** ①headless+黄金图逐字节对拍 day-one（iced 0.14 风向标【证】；QEMU ramfb 确定性天然接 G 关卡纪律）；②SDK-3 增 input_to_present 一等指标+deadline-miss 归因（NVIDIA 实测：几十 ms 系统延迟差→任务完成时间差 182ms【证】）。

**终态图景一句话**：`berry-ui（声明式+属性级依赖）→ berry-gfx（pass 形态记录，软硬双后端）→ berry-guiproto（三件事+latch/squash+version）→ guifbd（合成+无 damage 不提交+hit-test）→ HDI（ramfb/virtio-gpu/native）`。

## 五、兼容 vs 适配：三层归位

判据【推，据三样本归纳】：①与自家合成器/显示服务咬合的→自研定形；②语义已收敛且有上游维护的→直接吃；③只有实机厂商付得起的→留缝不实现。

- **自研定形层**：berry-gfx 语义边界、berry-guiproto 咬合面、合成器。先例：Google 级预算仍被「新内核×标准 API」接口面拖住（Dawn Fuchsia=WIP【证】）；Asahi 原文「strided linear 只配给 modifier-unaware 的窗口系统，性能会掉」——**适配的最后一段长在窗口系统里**（docs.mesa3d.org/drivers/asahi.html【证】）。
- **直接吃层**：WebGPU/Vulkan 对象语义（不发明第四种语义）；shader 编译器（naga/Tint 思想）；conformance 思想当验收关卡；WARP 式软栈定位。复用红利：Honeykrisp 复用 NVK 骨架约一年达成 Vulkan 1.3（rosenzweig.io【证】）。
- **留缝不实现层**：真机厂商 GPU 适配（Caps/required-features 地板=Dawn 最小特性清单范本）；GL 翻译层不进 v1（ANGLE 七后端各付一轮 conformance【证】）；DPU overlay 直扫留能力位（AOSP HWC：「静止+透明时 overlay 可能反而更耗」【证】——软件遮挡感知先行）；GPU-driven 全 compute 不进主线（vello 降 research 实证）。
- 翻译层税定量：MoltenVK vs 原生帧时间差 4–22%、方差差近一个数量级（gfx-rs Dolphin 对拍【证】）——勿为「以后跑 Linux 图形程序」在 v1 欠 ANGLE 类维护面。

## 六、低功耗：day-one 不可逆 vs 可后补

**不可逆五件**（跨信任边界/要求对端配合）：①damage 进 present 首版（FB_DAMAGE_CLIPS 2017 才补=后补≠自动生效，kernel.org drm-uapi【证】）；②帧调度模型=回调制+零回调（fifo/commit-timing 十年补课实证【证】）；③时序反馈位（presentation-time presented/discarded 终态）；④buffer 分配协商（格式/scanout 兼容）；⑤per-surface 帧率意图位（C1）。①–④ #50/#53 已裁或在册。

**可后补**（服务端内部可改）：仲裁启发式、合成缓存、背光/热策略、AOD 场景内容、LTPO 档位表、vote 数值——#53「API 语义 day-one、机制分刀」与此表一致，本表即其判定标准。

**联动**：IDLE 帧用绝对时刻一次 timer sleep 提交（commit-timing 同构，与本仓 tickless 同构）；AOD→intent maintenance 0x05=#42 首个真实消费者；PM 共享环{target,actual}禁通用 IPC 拷贝（AGENTS.md 既有）。缺口：AOD 面板逐档功耗定量仍缺一手实测（300–400mW 引用降级待再验）；on-pixel≤15%+1min 一拍可先用作断言锚【证】。

## 七、SDK/开发者体验

**载体裁决建议（冲突项 C2——已拍板 2026-10-29 晚：路线 A）**：现有裁决=berryui 自家体系、宏 vs 文本延至 SDK-1 后（2026-10-29 追裁）。证据：①有编译器者内建（Swift result builder/Kotlin Compose 插件）——Rust 无此前提；②宏代价=编译慢/报错差/展开后 AI 不可读（xilem 弃宏走 builder【证】）；③文本 DSL 赢在语法面小+独立预览器+AI 生成产物=数据（ArkTS/Slint【证】）；④裁决 F 已定「AI 生成 UI 产物=数据文件非 Rust 代码」。**路线 A（推荐）**：SDK-1 后拍板文本 DSL，宏仅作语法糖；**路线 B**：宏为主+强制 AST 导出补 AI 可读。共同硬要求：**声明即数据**——AST/场景树导出 day-one，机器可断言、可预览、可作 AI 生成目标；可选辅助 DSL 与代码共享同一 AST，非第二事实源（w-sdk §6）。

其余确认：热重载=SDK-2 FAT 热重载在册；AI 闭环=改→渲→验带机器可断言面（Design2Code 实证纯视觉生成有系统性召回缺口，arxiv 2403.03163【证】；SDK-4 四 verb 已裁）；验证器=headless 黄金图+重组/skip 计数+逐相耗时进结构化 tracing；框架层低功耗缝=不可见子树解除订阅联动三态停帧（ArkUI 组件冻结同构【证】，GUI-3 补充）。

## 八、GUI-0..9 刀序修订清单

| 刀 | 裁决 | 理由 |
| --- | --- | --- |
| GUI-0 FPU | 确认（可并入 GUI-2 前段） | rich 档光栅核前置（§2.3） |
| GUI-1 显示链 | 已落；协议面顺延确认 | 定形锚点 GUI-2/4 先到先定（规划 §4 注记） |
| GUI-2 berry-gfx | **调整：加五件** | ①着色器离线编译纪律；②transient 留位；③文字三段解耦（shape/图集/pass 内绘制，glyphon 复用 pass 模式【证】）；④自研光栅核性能对拍基准形态（S2 退役后空缺，基线已点名）；⑤Caps 地板参照 Dawn 最小特性清单 |
| GUI-3 berry-ui | 确认+补三件 | 属性级依赖记录骨架留缝；三态起振延迟遥测；overdraw 预算断言 |
| GUI-4 gui-inputd | 确认+补两件 | 输入服务调度铁律显式入刀（wait_many 阻塞形态/私有 handle 表/EOIR 纪律——基线点名未写）；「即到即派」模式留位 |
| GUI-5 gui-wmd | 确认；合并 vs 分离刀前 ADR 维持 | latch/squash 语义随此刀落 guifbd |
| GUI-6 交互三件 | 确认 | 布局单遍约束在 #52 内 |
| GUI-7 appd 扩面 | 确认 | views/ 资源位与 AST 导出对齐 |
| GUI-8 launcher | 确认（主验收） | — |
| GUI-9 验收+SDK-1 | **调整：加两件** | FrameTimeline 四桶归因（app/合成 deadline miss、stuffing、起振延迟）；稳态零唤醒断言 |

维护项：research/GUI-07、GUI-09 正文仍是追裁前口径（宏 DSL/Taffy 优先/vello_cpu 候选），须加追记防误导（以 docs/GUI里程碑规划-2026-10-v1.0.md 修订记录为准）。

## 九、开放问题与 spike 清单（含 kill criteria）

- **C1（已拍板 2026-10-29 晚：路线 A）**：per-surface 帧率意图位 day-one 留位 vs 仅 vote 票面。路线 A（推荐）：留协议版本域零成本位（Android/ARR 均为系统聚合仲裁先例）；路线 B：v2 再加（风险=FB_DAMAGE_CLIPS 同型协议破裂）。
- **C2（已拍板 2026-10-29 晚：路线 A——文本 DSL 为主）**：§7 DSL 载体路线 A/B。
- S1 virtio-gpu 2D spike（GUI-2 前）：量脏区 flush 往返。kill：同脏区帧耗 > 软渲染 2× → virtio-gpu 降可选后端，soft 仍唯一 CI 基线（#51 顺序不推翻）。
- S2' 光栅核定标（B-2 icount）：sparse strips 内联子集帧成本。kill：800×600 AA 全屏超软 vsync 60Hz 帧预算 → 降 AA 档或 480p（裁决 D 降档路径，规划 §5）。
- S3 virtio-input spike：kill：三源全不可达=结构性风险上报重议（规划 §5 原文）。
- S4 FPU 刀 ADR：trap+lazy-save vs 全量。kill：lazy 逃逸路径复杂度超预算 → 全量保存。
- S5 CJK 字库体积账：5–20MB vs 48MiB 盘。kill：装不下 → 子集化/分段加载 ADR。
- S6 credits 折叠实证：kill：单进程 guifbd 下丢交互帧 → credits 升 3 或引入 unsquashable 位（Fuchsia 保底语义）。
- S7（可选）属性级失效小原型：kill：属性级收益 <20% 帧耗时 → 维持视图级至 v2。

## 十、参考文献（去重，分组）

- **AOSP**：source.android.com/docs/core/graphics/{architecture, implement-vsync, hwc, multiple-refresh-rate, arr, arch-bq-gralloc, sync}；source.android.com/docs/core/runtime/improvements
- **Android API/博客**：developer.android.com/reference/android/view/{Choreographer, Surface}；/topic/performance/vitals/anr；/training/wearables/always-on；/develop/ui/compose/performance/stability/strongskipping；android-developers.googleblog.com/2024/05/whats-new-in-jetpack-compose-at-io-24.html
- **Android 源码文档**：android.googlesource.com → platform/external/perfetto docs/frametimeline.md；frameworks/native/libs/gui/BufferStuffing.md、DisplayEventReceiver.cpp、BitTube.h；frameworks/base@5.0 RenderThread.cpp；InputEventReceiver.cpp
- **Fuchsia/Dawn**：fuchsia.dev → flatland / frame_scheduling / life_of_a_flatland_image / escher / RFC-0161·0162·0198 / magma porting；dawn.googlesource.com/dawn docs/support.md；github.com/google/dawn
- **Wayland**：wayland.freedesktop.org/architecture.html；wayland.app/protocols/{wayland, presentation-time, fifo-v1, commit-timing-v1}；ppaalanen.blogspot.com/2015/02/weston-repaint-scheduling.html
- **QNX**：qnx.com/developers/docs/8.0 → cscreen_event-handling.html / cscreen_windowing.html
- **内核**：torvalds/linux drivers/gpu/drm/i915/display/intel_psr.c、include/uapi/drm/drm_mode.h；kernel.org/doc/html/latest/gpu/drm-uapi.html；docs.kernel.org/driver-api/dma-buf.html
- **GPU 适配**：rosenzweig.io/blog/{asahi-gpu-part-2, first-conformant-m1-gpu-driver, vk13-on-the-m1-in-1-month, vulkan-14-sur-asahi-linux}.html；docs.mesa3d.org/drivers/{asahi, freedreno, zink}.html；gfx-rs.github.io/2019/03/22/dolphin-macos-performance.html；chromium.googlesource.com/angle/angle README；github.com/ARM-software/perfdoc
- **渲染栈**：docs.flutter.dev/perf/impeller（+3.10/3.27 blog）；flutter/flutter impeller README；github.com/linebender/vello（ARCHITECTURE.md）；github.com/gfx-rs/wgpu；developer.apple.com Metal TBDR 文档；learn.microsoft.com/windows/win32/direct3darticles/directx-warp
- **Rust UI/AI**：github.com/linebender/xilem；iced 0.14 release；Dioxus；github.com/slint-ui/slint LICENSE.md；makepad；arxiv.org/abs/2403.03163；swiftlang/swift-evolution 0289；developer.apple.com/documentation/Observation；openharmony docs arkts-new-observedV2-and-trace
- **仓库**：docs/架构决策.md（ADR #39–#42、#50–#54）；docs/GUI里程碑规划-2026-10-v1.0.md §3–§5；docs/GUI渲染调研-2026-10-v1.0.md；research/GUI-01..09；AGENTS.md

---

## 附录 A：仓库基线

### A.1 渲染侧（GUI-01..04 摘要）

【基线·渲染侧】

# 渲染侧仓库基线：GUI-01..04 综合摘要

## 一、核心结论清单

**GUI-01（安卓渲染响应慢教训）**

1. 十五年演进主线＝时序对齐（JB vsync 全系统化）＋剥离主线程（5.0 RenderThread）＋度量体系化（JankStats/FrameTimeline）。
2. 一次 4ms GC 停顿即可毁一帧（16ms 预算 ¼）；draw 内建对象＝分配 churn，2011 官方博文点名。
3. FrameTimeline：帧「不慢但晚到」同样伤输入延迟——jank 从帧耗时深化为呈现时序偏移（perfetto.dev）。
4. Perfetto 样本：read→dispatch 3ms vs ACK→present 50ms，延迟大头在应用帧循环排队与呈现等待。
5. 2013 Agawi 实测：iPhone5 ≈55ms vs 最快 Android ≈85ms；归因含 iOS 实时优先级 UI 线程（MacRumors）。
6. RelativeLayout/weight 双遍测量是协议病，Compose 单遍约束立法废除。
7. 主线程同步 Binder 阻塞（52ms contacts 案例）——同步 channel 同病，需「UI 线程零同步 RPC」纪律。
8. overdraw 官方预算 ≤2.5× 屏幕像素；遮挡剔除应与矩形合并同级验收。
9. Compose 靠 Strong Skipping（2024 默认）五年才追平 View（官方 Pokedex 基准 1.9 持平）；Rust 无 GC 保留树＝「全 skip」极限形态。
10. pre-JB 输入攒批雪崩→处理时刻取最新＋预测外推，仍是输入链设计正解。

**GUI-02（领先图形管线）**

1. iOS＝应用提交图层树变更（CATransaction 一次 run loop pass 一批）、render server（backboardd，逆向共识）独立进程按 vsync 渲染（Tech Talks 10855）。
2. UI 线程不等 GPU：提交即返回；唯一背压＝drawable 池 2–3 帧，池满 nextDrawable 阻塞（CAMetalLayer/MoltenVK #929）。
3. ProMotion：12 档 120..10Hz，CA 统一仲裁、应用仅 hint、>60Hz 需 plist 显式解锁（功耗默认收紧）。
4. 触摸＝IOKit→backboardd mach 直递，事件是帧循环第一阶段；coalesced/predicted 实现 240Hz 采样/投递/渲染三解耦。
5. Windows DWM：flip 共享缓冲零拷贝＋DirectFlip/MPO 直扫阶梯＋present statistics 丢帧可对账可恢复。
6. Chromium：display list＋property trees 原子 commit 到合成线程、四树、滚动绕过主线程、Viz 聚合（RenderingNG）。
7. 提炼 P1–P10 普适原则；流水错相本质＝deadline 结构，单核分时同享其利。
8. TBDR：带宽＝功耗大头，render pass/loadOp·storeOp/transient 是 tile GPU 功耗-正确性基线。

**GUI-03（先进渲染 API 全景）**

1. Android 十年单向门：10 强制 Vulkan 1.1→15 抬 1.3（Khronos profile）→16 议 1.4；GLES 经 ANGLE 退场。
2. WebGPU 2025-11 全主流浏览器＋三独立实现＋CTS＝语义收敛点（web.dev）。
3. 「兼容≠适配好」实证库：Vulkan 驱动碎片化（filament #1102、Mali VK_DEVICE_LOST、igl #333 Vulkan 反而更慢）、MoltenVK 天花板 1.2、wgpu 拒过 lavapipe（#3622）、Flutter GLES 回退腐烂（#179268）、Skia Graphite 撤到 Dawn 单抽象。
4. 运行时着色器编译＝移动卡顿第一病灶（Impeller 全案）；AOT 只是把成本搬到启动（flutter #191860）。
5. 虚拟环境地形：virtio-gpu 2D（mmio 开箱即用）＞venus（blob+hostmem+Mesa 全栈）＞virgl（aarch64 黑屏史）；SwiftShader 被 Chrome 退役；Cuttlefish 同走 virtio-gpu。
6. UTM5/macOS26 已商用 Venus→Metal Vulkan 1.3。

**GUI-04（Rust 渲染生态）**

1. vello_cpu 原判唯一双过关件（Apache/MIT、no_std、forbid(unsafe)、纯 CPU Sparse Strips；egui0.37/Servo 生产采纳、90 天 670 万下载）——后经用户追裁降思想参考。
2. wgpu 许可过关但结构性不可达（需原生 GPU API；virgl/venus 要 Mesa C 栈违全 Rust 纪律）。
3. Slint 三锁全断（GPLv3 传染／RF-2.0 禁暴露 API＋排除 embedded／商业版税）违 ADR #37；其 no_std/MCU 264KB、逐行渲染、DSL+LSP+Live Preview 思路价值最高。
4. iced＝唯一大规模生产背书（COSMIC 出厂 Pop!_OS）但绑 winit/wgpu；Elm 三分离＝SDK 编程模型候选。
5. egui 热度第一（90 天 810 万）但即时模式全帧重绘与低功耗/脏区结构性冲突。
6. Rust GUI 无共识（areweguiyet「根不深」）；所有框架预设 winit 桌面地基——恰是 BerryOS 要自建的，整框架引入全灭，只能「渲染核＋范式＋工具链」三层解耦引入。

## 二、已进裁决 vs 开放项

**已进 ADR #50–#54（2026-10-29 落册）**：B 形态协议三件事（#50：submit+damage K≤8+FULL／frame_callback 时刻表+credits≤2／release）；HDI 三口显示驱动契约＋hit-test 归合成侧＋软 vsync request-driven（#50）；pass 形态 WebGPU 语义子集薄 HAL＋soft→virtio-gpu2D→native Vulkan 后端序列（#51）；光栅核全面自研、vello_cpu 降思想参考/像素对拍基准（#51 追裁）；berry-ui 自研声明式＋布局自研不引 Taffy＋DSL berryui 自家体系（#52）；低功耗六件 damage/三态停帧 RUN·SETTLE·IDLE/vote 票面/power·mode/PM 环 intent·EPP/能耗代理断言（#53）；SDK-1..5（#54，吸收 Slint 工具链思想与 P9 可观测原则）。GUI 帧线程 intent 申报＝#42 消费面、AOD→maintenance 0x05（#53）。

**仍开放**：sparse strips 自研子集帧成本定标（B-2 icount，GUI-2）；跨进程 fence 最小语义（event handle 竞速窗）；wmd/guifbd 合并 vs 分离（需 ADR）；virtio-gpu S1 spike 暂缓至 GUI-2 前；信号核 vs 键路径 diff（v1.5 原型对拍）；transient attachment 留位；真机 GPU 基线（Mali/Adreno）；矢量字体线；考据类（KitKat 去隐式分配一手来源、HWUI Vulkan rollout 时间线、120Hz 触控延迟无严格基准）。

## 三、覆盖不足（web 深研补刀清单）

1. TCG 软渲染吞吐定量参照系：脏区/RGB565 降载后帧耗能量级无外部基准可对（两报告均挂「实测定标」）。
2. sparse strips 算法一手细读：Linebender 源码/论文级（展平-条带-合成三段复杂度与内存形态），现仅 README 级——自研内联子集直接依赖它。
3. 声明式+保留树增量失效算法深料：SwiftUI AttributeGraph、Compose Slot Table 内部机制、ArkUI 状态管理——#52 v1.5「信号核 vs 键路径 diff」对拍的方法论基础。
4. AOD/LTPO 面板逐档功耗定量数据：#53 vote 票面与 power·mode 四档的数值依据目前缺一手实测。
5. B 形态 credits 协议先行者一手文献：BLASTBufferQueue、Wayland frame_callback 语义、Fuchsia scheduling hints（GUI-05 在姐妹报告，本四报告未展开）。
6. 输入预测算法层资料：Android stylus prediction 模型、Apple predicted touches 机制细节。
7. 合成侧 hit-test 与 clickjacking 防御文献：#50 已裁但报告内无论证细节。
8. virtio-gpu 2D 驱动侧一手：Linux 驱动/协议 spec（资源/dmabuf/脏区 flush/display event 频率），现仅 QEMU 文档级。
9. 微内核 GPU 栈先例深挖：Fuchsia Magma、seL4 GPU 路线——「内核只给通用原语」的佐证与陷阱。
10. 帧内零分配的嵌入式 GUI 生态实践：embedded-graphics/lvgl-rs 的池化与分配策略，供自研光栅核借鉴。

---
**核对于 2026-10-29 裁决现状**：已对照 `docs/架构决策.md` ADR #50–#54 与 `docs/GUI里程碑规划-2026-10-v1.0.md` 修订记录——注意 GUI-03/04 报告中的「vello_cpu spike 过门候选」「S1 spike」两项已被 2026-10-29 用户追裁改写（前者退役降思想参考、后者暂缓），上表已按现行口径标注。全程只读，未改任何仓库文件。

### A.2 架构侧（GUI-05..09 + 规划 + ADR 摘要）

【基线·架构侧】
全部材料已通读（GUI-05..09 五报告、规划全文、ADR #39–#41/#50–#54、姊妹综合）。以下为产出纪要。

---

# 仓库基线·架构侧通读纪要（ground-arch）

依据：`research/GUI-05..09` + `docs/GUI里程碑规划-2026-10-v1.0.md` + `docs/架构决策.md` ADR #39–#41/#50–#54 + 姊妹综合 `docs/GUI渲染调研-2026-10-v1.0.md`。只读，未改任何文件。

## 一、现行裁决全清单

**端定位底座（ADR #39–#41）**：#39 v1 主战场 = 移动设备（手机）rich 档单线走到底，手表 = 同族低功耗端非锚（2026-10-28 修订）——GUI 按 phone 形态设计、模拟器验收、手表靠协议缝自然受益；#40 MCU 档排除；#41 x86_64 最小集提前（范围盒子 + kill criteria）。

**裁决 A–I（规划 §3）**：

| # | 定了什么 | 为什么 |
| --- | --- | --- |
| A | 输入源 virtio-input→USB 平板→串口鼠标三层递降 | 框架现成；兜底保验收不塌 |
| B | 图标 = 自定义紧凑格式（宿主 PNG→ARGB/RLE） | 宿主转换一次成型，设备端零解码依赖 |
| C | 单 GUI 进程内嵌 berry-ui + 三薄服务，不搬 Rosen 重 IPC | GUI-05 五家谱系：B 形态（buffer 过界）终态的单客户端退化形 |
| D | RGB32 800×600 起步；协商进协议版本域；不足降 RGB565/480p | 像素格式烧常量 = 移植性死刑（R1） |
| E | guifbd/launcher 崩 → appd 监督重启 | 微内核失败隔离原则 5 |
| F | manifest 扩 icon/label/views/（add-only） | AI 生成 UI 产物 = 数据文件非 Rust 代码 |
| G | 低功耗缝 day-one 六件（见 §三） | 砍实现不砍协议位 |
| H | SDK-1..5 演进序（见 §三） | 差异化在验证不在生成 |
| I | berry-guiproto 提前定形（GUI-1/4） | 协议窗口正开着，后补 = ABI 破裂 |

**ADR #50–#54**：**#50** 架构面按终态、实现面渐进——B 形态三件事（submit(buffer, dirty_rects K≤8+FULL) / frame_callback(未来时刻表+credits≤2) / release）定形 berry-guiproto；显示驱动 HDI 三口（ramfb 首实现、virtio-gpu 次实现，换实现不换契约）；buffer = VMO handle、fence = event handle，零新内核对象；hit test 归合成侧防 clickjacking。**#51** 薄 HAL（WebGPU 语义子集：pass/loadOp·storeOp/命令录制/Caps day-one 在场）+ 后端序列 soft（永久 CI 基线）→ virtio-gpu 2D → native Vulkan；光栅核全面自研（追裁后 vello_cpu 降思想参考）。**#52** 自研 berry-ui 声明式：API 面声明式、引擎内核保留树+脏区；布局自研不引 Taffy；DSL = berryui 自家体系；数据驱动纪律 day-one；失效粒度 v1 视图级 → v1.5 属性级键路径 diff。**#53** 低功耗六件 + R1–R10 反面清单入评审。**#54** SDK-1..5 锁定；dump day-one round-trip 可注入。

**2026-10-29 追裁**：**BerryUI 全面自研**——UI 框架/布局/光栅核全自己写（参考 SwiftUI/Compose/ArkUI/Flutter 思想，协议洁净照旧）；vello_cpu 从「S2 spike 过门候选」降思想参考/对拍基准（spike 过门机制退役），sparse strips 思想内联子集转正主路。

## 二、GUI-0..9 刀序状态

| 刀 | 内容 | 状态 |
| --- | --- | --- |
| GUI-0 | FPU/FPSIMD 上下文（硬门：TaskContext 仅整数，FP 跨任务串味） | 未落，可并入 GUI-2 前段 |
| GUI-1 | 显示链收口 | **已落**（2026-10-29 两段提交） |
| GUI-2 | berry-gfx（pass 形态 API + 自研光栅核 + 图标/字体档 + 像素共识测试） | 待做 |
| GUI-3 | berry-ui（保留树脏区 + 三态停帧验收 + 双轨打点 + 数据驱动 API） | 待做 |
| GUI-4 | gui-inputd（PointerEvent 带时间戳序列 + 重采样留位 + intent/PM 脉冲） | 待做，可与 GUI-2/3 并行 |
| GUI-5 | gui-wmd（两层窗 + hit test 立法 + parent/group；刀前 ADR 是否并入 guifbd） | 待做 |
| GUI-6 | 交互三件（自研布局 + 手势 referee + 缓动 + SwipeView） | 待做 |
| GUI-7 | appd 扩面（icon/label/views/ + 增删事件） | 待做 |
| GUI-8 | berry-launcher（**M-GUI 主验收**：桌面+翻页+点击启动+跨 boot 保布局） | 待做 |
| GUI-9 | +SDK-1 并轨（dump 可注入 + 能耗断言 + 双通道注入 + 截图对拍） | 待做 |

**GUI-1 已落**：fb MMIO 窗 VMO 授能 + guifbd 常驻服务（`FBW ok` 像素自验）+ console fan-out 双路上屏（`FBSERVE ok`），smoke-image 生态 canon 21→23；V-6 刀 2 挂账清账。**顺延（与原案差异）**：serve 循环暂为 fsmgr 形态纯文本流，**B 形态三件事与 berry-guiproto 雏形未随刀落**——定形锚点改挂 GUI-4 或 GUI-2 消费侧首刀（先到先定）；HDI 三口随 GUI-2 后端面落。

## 三、低功耗缝六件（G）与 SDK-1..5（H）现状

| # | 六件（ADR #53） | 落点 | 状态 |
| --- | --- | --- | --- |
| ① | damage 矩形集 K≤8+FULL | berry-guiproto（顺延→GUI-2/4 定形） | 未落 |
| ② | 三态停帧 RUN/SETTLE/IDLE（IDLE 撤定时器睡事件） | GUI-3 验收项 | 未落 |
| ③ | vote 区间票面{min,max,preferred,scene,sticky} | berry-guiproto 留位 | 未落 |
| ④ | power/mode op NORMAL/DIM/AOD/OFF | 协议留位 | 未落 |
| ⑤ | PM 共享环{target,actual} + 帧态翻 intent·EPP（AOD→maintenance 0x05 = #42 机制预留首个真实消费者） | GUI-4/9 接线 | 未落 |
| ⑥ | 能耗代理断言（IDLE 唤醒 0 + icount≈0 进 smoke） | GUI-9 | 未落 |

六件全部在途：GUI-1 未带协议面，首个协议消费者出现时定形；R1–R10 反面清单待挂评审流程。**SDK 线**：SDK-0（CLI 循环 + smoke 断言 + FIFO 注入）已在库；SDK-1 挂 GUI-8/9 未落；SDK-2（DSL+解释器+FAT 热重载，载体宏 vs 文本 SDK-1 后裁）、SDK-3（帧时间线+p50/p90）、SDK-4（AI 生成闭环+MCP 四 verb）、SDK-5（设计器+Rust 热重载等 v2 动态加载）均未启动；SDK-2 起与 GUI 线文件面互斥可并行。

## 四、覆盖不足 / 未回答 / 需刷新

**需刷新（追裁后未回写）**：①`research/GUI-07`、`research/GUI-09` 正文仍是追裁前口径（GUI-07 推宏 DSL；GUI-09 §2.b/§3 推「Taffy 移植优先」「vello_cpu 候选 S2」），与 #51/#52 相反且报告头无追记（姊妹综合已加注），直读报告会被误导；②DSL 载体三案（外部 DSL/宏 DSL/双面）报告间分歧仍在册——追裁只定「berryui 自家体系」，宏 vs 文本显式延迟到 SDK-1 后。

**未回答（按紧急度）**：①协议定形前强制项——S3 virtio-input spike 未跑、S1 virtio-gpu 拍板「留 GUI-2 前」、S2 退役后自研光栅核的性能对拍基准形态未定义；②gui-wmd/guifbd 合并 ADR（GUI-5 前）+ FPU 刀 ADR（trap+lazy-save vs 全量）；③CJK 文本栈未入刀序（姊妹综合拆三刀 + 字库 5–20MB vs 48MiB 盘体积账「先算账再选型」，规划 GUI-2 仅写「字体档多字号」）；④intent 档位边界（复用 0x01/0x02+EPP vs 开新档，ADR #42 冻结段扩表）；⑤多 GUI 进程拆分判据 + 32MiB 帽 buffer 池核算、fence v2 ADR、JSON 动态 UI 与 .bap 能力模型边界、TCG 时基口径、预测渲染默认开关（GUI-6 后 A/B）。

**覆盖不足（姊妹综合对抗校验已点名、未挂刀）**：色彩管理/色深概念位（零成本）、IME/preedit 缝、多显示/热插拔/DPI 谱系、无障碍消费端 + RTL 镜像、GUI 资产字节级预算表（32MiB 帽 + 大块 buddy 陷阱）、damage 申报信任/截屏隐私面、输入服务调度铁律（wait_many 阻塞/私有 handle 表/EOIR 纪律）未显式写进 GUI-4 刀面。

---

## 附录 B：w-android——安卓渲染响应慢：因果谱系与修法时间线

# 安卓“慢”的完整病根谱系与修法时间线（w-android）

## 一、结构性病根逐个拆

1. **主线程单线程 UI 模型**：View 树 measure/layout/draw、输入回调、binder 分发、消息队列同住主线程；输入派发 5s 无响应即 ANR，官方把“主线程慢代码”列为首要原因（[ANR](https://developer.android.com/topic/performance/vitals/anr)）。5.0 起才把 display list 录制/执行拆给 RenderThread，动画得以脱离主线程（[RenderThread.cpp@5.0](https://android.googlesource.com/platform/frameworks/base/+/android-5.0.0_r1/libs/hwui/renderthread/RenderThread.cpp)）。
2. **Choreographer/vsync 对齐的固有代价**：4.1 起绘制被 vsync 门控（[Choreographer，API16](https://developer.android.com/reference/android/view/Choreographer)）。AOSP 明说 app/SF 相位偏移按“最坏负载”保守取值，等效把输入→显示拉长，调小则更易错帧——延迟与稳定互为租金（[implement-vsync](https://source.android.com/docs/core/graphics/implement-vsync)）。
3. **BufferQueue 与 buffer stuffing**：每 Surface 是三缓冲生产者-消费者环（[graphics architecture](https://source.android.com/docs/core/graphics/architecture)）。SF 错过 deadline 不还 buffer，客户端堆积；FrameTimeline 定义 "Buffer Stuffing" 掉帧类型：*不管 app 多快，所有帧至少晚一 vsync 上屏*（[frametimeline](https://android.googlesource.com/platform/external/perfetto/+/refs/heads/main/docs/data-sources/frametimeline.md)）。12 起 BLASTBufferQueue 改事务式提交，且错帧后自动把客户端动画时间轴后移一帧恢复（[BufferStuffing.md](https://android.googlesource.com/platform/frameworks/native/+/refs/heads/main/libs/gui/BufferStuffing.md)）。
4. **SurfaceFlinger 合成时机**：SF 自身有 CPU/GPU deadline（FrameTimeline 的 SFCpuDeadlineMissed / SF Deadline Missed），HWC 回退 GPU 合成再吃一段（同上两源）。
5. **Binder**：一切跨进程（WMS/SF/AMS）走 binder（[binder-ipc](https://source.android.com/docs/core/architecture/hidl/binder-ipc)）；主线程同步 binder 是掉帧常客，最坏被 5s ANR 上限放大（[ANR](https://developer.android.com/topic/performance/vitals/anr)）。
6. **GC 停顿史**：Dalvik 停顿式 GC 是历史包袱；8.0 ART 并发压缩 GC 仅剩 thread roots 一次短停顿，H2 基准停顿较 7.0 小 85%（[ART improvements](https://source.android.com/docs/core/runtime/improvements)）——但 GC/渲染同进程的结构耦合仍在。
7. **输入管线**：InputReader→InputDispatcher→socket→app，事件按 vsync 节奏 batching+resampling（[InputEventReceiver "Batched consumption"](https://android.googlesource.com/platform/frameworks/base/+/master/core/jni/android_view_InputEventReceiver.cpp)）——为省电/稳定天然晚半拍；Butter 补 touch boost（触摸即提频）（[AP](https://www.androidpolice.com/2012/07/12/getting-to-know-android-4-1-part-3-project-butter-how-it-works-and-what-it-means/)）。
8. **动画掉帧机理**：全链任一段超 deadline 即整体晚一 vsync；FrameTimeline 类型化归因（App CPU/GPU、Buffer/SF Stuffing、Prediction Error、Display HAL）（[frametimeline](https://android.googlesource.com/platform/external/perfetto/+/refs/heads/main/docs/data-sources/frametimeline.md)）。

> 检索说明：brief 的 "ADSF" 未检索到主线缩写用法（推断=ARR/ADFR 帧率自适应族）；"preemptive input" 亦非公开特性名（推断=ADPF/预测式返回一类输入优先化动向）。

## 二、修法时间线（含实测口径）

| 年/版 | 修法 | 效果/证据 |
| --- | --- | --- |
| 2012·4.1 | Project Butter：全链 vsync+Choreographer+三缓冲+touch boost（[I/O 2012](https://developers.google.com/events/io/2012/sessions)、[AP](https://www.androidpolice.com/2012/07/12/getting-to-know-android-4-1-part-3-project-butter-how-it-works-and-what-it-means/)） | 掉帧系统性缓解 |
| 2014·5.0 | RenderThread：动画离主线程 | 动画抗主线程抖动 |
| 2017·8.0 | ART CC GC | GC 停顿 −85%（H2）（[同病根6源](https://source.android.com/docs/core/runtime/improvements)） |
| 7→10 | HWUI 录制-回放 + Skia/Vulkan 后端（[renderer 配置](https://source.android.com/docs/core/graphics/renderer)） | skiavk 可切；默认面随机型，未核到统一口径（推断） |
| 2020·11 | Frame Rate API/MRR（[multiple-refresh-rate](https://source.android.com/docs/core/graphics/multiple-refresh-rate)） | 帧率切换 |
| 2021·12 | FrameTimeline 归因 + BLAST 治 stuffing | 掉帧可观测、可恢复 |
| 2024·15 | ARR 自适应刷新率，vsync 离散步进（[arr](https://source.android.com/docs/core/graphics/arr)） | 省电为主 |
| 2025·16 | hasArrSupport()/getSuggestedFrameRate()（[A16](https://developer.android.com/about/versions/16/summary)） | 应用侧可感知 |

## 三、玻璃到像素量级

- NVIDIA 把系统延迟拆 输入→帧开始→present→显示 四段并实测：12ms vs 20ms 的系统延迟差使瞄准任务完成时间差 182ms（≈22 倍）——几十 ms 差异可感（[latency blog](https://developer.nvidia.com/blog/understanding-and-measuring-pc-latency/)、[Reflex](https://www.nvidia.com/en-us/geforce/news/reflex-low-latency-platform/)）。
- Android 可测 input_to_present_ms，但官方口径**不含**触摸 IC、驱动上报、面板扫描/响应——真“玻璃到像素”更大（[Perfetto 实战 18](https://androidperformance.com/en/2026/05/04/Android-Perfetto-18-Input-Response-Latency/)）。
- 算术项：60Hz 触摸采样=最多 16.7ms 采样等待（纯算术，推断）。
- 主机：RTINGS 按整链实测输入延迟（game mode 面板普遍 10–30ms 档），60fps 一帧 16.7ms 起步，整链常 50–100ms（推断=面板实测口径+帧等待算术）（[RTINGS](https://www.rtings.com/monitor/tests/inputs/input-lag)）。
- Apple Pencil 在 120Hz iPad 上 9ms 为厂商宣称口径，本次仅核到二手转述（[r/ipad](https://www.reddit.com/r/ipad/comments/c0aq4a/ipados_9mm_latency_apple_pencil/)）。

## 四、新 OS day-one 应内建的“快”结构性清单

1. 输入线程独立于 UI 线程且最高优先级（病根1）。
2. 事件时间戳贯穿 input→present，全链可归因（病根8）。
3. 合成相位偏移按设备实测可调，并提供“低延迟档”而非钉死保守值（病根2）。
4. 提交=事务+dirty rects+显式 release，缓冲配额与帧预算挂钩，错帧自动沿动画时间轴对齐（病根3）。
5. 合成器 deadline 一等公民：CPU/GPU 分账、GPU 回退有预算（病根4）。
6. 显示路径 IPC 走零拷贝共享环+无锁快路径，微秒级，永不排队主线程（病根5）。
7. 无 stop-the-world GC（Rust no_std）（病根6）。
8. vsync 对齐为默认，但保留“即到即派”低延迟输入模式（可放弃 batching 省电收益）（病根7）。
9. touch boost 类提频 hint 做成内核第一类接口（病根7）。
10. 类型化掉帧归因（deadline-miss 分类）内建于遥测（病根8）。
11. 帧率/刷新率自适应归合成器，API 暴露期望 present 时刻（病根2/4，ARR 思路）。
12. input_to_present 为一等公民指标，机器可断言（病根1–8 的验收面）。

## 五、对 BerryUI 的可迁移启示

- frame_callback(未来时刻表+credits) ≈ Choreographer+BLAST 修法的形态化，方向已被安卓验证，坚持 B 形态协议；
- dirty_rects/release 与 ARR 的 notifyExpectedPresent 同构，保留；
- 安卓核心教训=**一切省电默认都在向延迟收租**：BerryUI 低功耗缝（vote/DIM/AOD/三态停帧）必须留“即到即响应”逃生门；
- 把 FrameTimeline 式类型化掉帧归因写进 B 形态遥测面，供 AI 原生调度与 QEMU 测试断言复用。

---
*产出已同步至共享 store（key=`w-android`）。字数：中文约 1005 字 + ASCII 词 314（URL 为引用元数据另计）；共 20 处来源，其中 AOSP/Google 官方文档 11 处、NVIDIA 2 处、社区/工程博客 4 处，未核验处均已显式标注“推断/厂商宣称”。*

---

## 附录 C：w-pipeline——最先进 2D/UI 渲染管线全景（2024–2026）

# 2024–2026 最先进 2D/UI 渲染管线全景（w-pipeline 深研）

## 1. Flutter Impeller：为什么换掉 Skia

**机理（有证据）**：Skia 时代着色器在设备上**首次绘制时**才编译（SkSL→后端着色器），首帧/新特效必卡；Impeller 的对策是把全部着色器与反射**离线**在引擎构建期完成（`impellerc`：GLSL→MSL 等 + 生成 C++ 绑定，无运行时反射），**启动即建全部 PSO**，GLES/Vulkan 这类无 shader 库的后端用 `blobcat` 打包 shader archive 随引擎分发（<https://github.com/flutter/flutter/blob/master/engine/src/flutter/impeller/README.md> ， <https://docs.flutter.dev/perf/impeller> ）。

**落地（有证据）**：3.10（2023.5）Impeller 成 iOS 默认，「复杂屏幕内存占用几乎减半」（更少 render pass/中间 target + 有损纹理压缩）；3.27（2024.12）成为 Android（API 29+，Vulkan）默认；如今 iOS 上 **Skia 已彻底移除、无切换项**，3.27 还修了 Metal compositor backpressure、可稳定打满 120Hz（<https://docs.flutter.dev/blog/whats-new-in-flutter-3-10> ， /whats-new-in-flutter-3-27 ， /perf/impeller ）。

**架构要点**：`entity` 层做 **pass 优化/pass 重写**（合并甚至省略 pass）；`typographer` 只画字形（glyph 图集），不做排版；DisplayList dispatcher 与框架解耦——前端记录、后端执行分离（engine README，同上）。

## 2. Skia Graphite/Dawn、Fuchsia Escher

- **Graphite**（推断+证据）：Skia 源码树同时含 `src/gpu/graphite/` 与老后端 `ganesh/`（<https://github.com/google/skia> ），Graphite 为面向现代 API（配合 Dawn/WebGPU 语义）的新 GPU 路径；公开文档已大幅收缩，只能从源码结构与 Chromium 关联验证：**Dawn** 是 Chromium 的 WebGPU 实现（`webgpu.h` 一对一，后端 D3D12/Metal/Vulkan/GL，BSD-3），且提供**沙箱应用的 client-server WebGPU**（<https://github.com/google/dawn> ）——对微内核 OS 把 GPU 做成用户态服务是直接先例。
- **Fuchsia Escher（教训）**：官方自述为「Scenic 用的 C++/Vulkan 库，**不进 SDK**」，提供 pipeline 复用、省带宽的色域校正、比裸 Vulkan 好写的 sub-pass（<https://fuchsia.dev/fuchsia-src/concepts/graphics/escher> ）。教训：渲染器与窗口系统深度耦合、不对外暴露，最终生态价值归零。

## 3. wgpu 生态（Rust 侧现状，有证据）

- **wgpu**：WebGPU 语义的纯 Rust API，跑 Vulkan/Metal/D3D12/OpenGL/WebGL2/WebGPU，是 Firefox/Servo/Deno 的 WebGPU 核心，Apache/MIT（<https://github.com/gfx-rs/wgpu> ）。
- **Vello 转向（关键！）**：仓库现在明确「三个渲染器」——**vello_cpu**（纯 CPU，SIMD+多线程，官方称**最成熟**选择）、**vello_gpu**（CPU 预处理后 GPU 光栅/合成，wgpu+原生 WebGL2，**不要求 compute shader**，定位为未来生产主力），而原来的**全 compute 路线 `vello` 已降级为 `research/` 实验品**（兼容面与内存代价不同）。CPU/GPU 两路共享 **Sparse Strips** 架构（路径展平→tile 分箱→稀疏条带，只保留边界 coverage）与 `vello_common`/`glifo`（文字 glyph run）（<https://github.com/linebender/vello> ，ARCHITECTURE.md）。
- **文字栈**：**parley**（linebender 富文本排版：Fontique 枚举/回退 + HarfRust shaping + Skrifa + ICU4X，Apache/MIT）；**cosmic-text**（COSMIC 桌面在用：HarfRust shaping、双向文本、借用 Chromium/Firefox 回退表，Apache/MIT）；**glyphon**（wgpu 2D 文字：cosmic-text 排版 + etagere 图集 + 绘制，**复用已有 render pass** 以免加 pass——中间件模式，Apache/MIT/zlib 三许可）（各 README；<https://github.com/ARM-software/perfdoc> 无关，见下）。

## 4. 移动 GPU 架构对 2D 合成/部分刷新的影响

- **Apple TBDR（有证据）**：GPU 把目标切成 tile，**凑齐一个 pass 的全部几何再着色**；tile memory 是片上快速临时存储，省时间/能耗/带宽，pass 结束才写回设备内存；A11 起有 imageblock/tile shading（<https://developer.apple.com/documentation/metal/tailor-your-apps-for-apple-gpus-and-tile-based-deferred-rendering> ）。
- **Adreno（有证据）**：「mostly tile-mode renderer」，可绕过 tiling（gmem）直写系统内存（sysmem）（Mesa freedreno 文档，<https://docs.mesa3d.org/drivers/freedreno.html> ）。
- **Mali（有证据）**：Arm 官方最佳实践被固化为 PerfDoc 校验项——附件应**瞬时/懒分配**（驻留 tile memory）、**禁止冗余 render-pass store**、避免 tile readback、clear-attachments-after-load（<https://github.com/ARM-software/perfdoc> ，已捐 Khronos validation layers）。
- **对 UI 的含义（推断）**：pass 数量是 tile GPU 的成本单位；dirty-rect 越碎→pass 越碎→store/load 反复发生。K≤8 的大矩形远优于大量小矩形；「停帧三态/部分刷新」在 TBDR 上天然省电（不渲染即不 store）。**GPU-driven（全 compute）UI 不宜作主线**：vello compute 降级 research 是实证；compute pass 放弃 tile 局部性（推断）。

## 5. 「无 GPU 起步 → 有 GPU 平滑升级」的设计要点

**成功先例（有证据）**：Vello Sparse Strips——**同一份场景记录，CPU/GPU 两后端共享几何/分箱/paint/strip 数据结构**，软硬切换不换语义；Flutter 的 DisplayList（记录）与 Impeller/Skia（执行）分离同理。

**必须 day-one 在 API 里**（Impeller 教训+tile GPU 语义）：① pass 为唯一同步/合成边界：loadOp/storeOp/clear 色、dirty rect；② 能力位（Caps）：blit、瞬时附件、MSAA、格式查询——Impeller 有 texture→texture blit 进 device caps 的先例（3.10 release notes）；③ **「着色器=构建产物」**：禁运行时编译，离线预编译+打包 archive；④ 记录/回放分离（display list）；⑤ 中间资源显式声明 transient（tile memory 生命周期=pass）；⑥ 文字三段解耦：shape/layout / 图集 / pass 内绘制。**可以后补**：高级 AA（Impeller 单独迭代 AA）、宽色域、compute 特效、3D（Flutter GPU 实验性）。

## 对 BerryUI 的可迁移启示

1. berry-gfx 的 pass 形态定对了：**pass=成本单位**，loadOp/storeOp/Caps day-one 冻结，软后端把 pass 当 no-op 屏障照跑，语义不特化。
2. **同一场景记录双后端**（软 raster↔GPU）：抄 Vello Sparse Strips 的「共享几何/paint 结构、后端只换执行」。
3. 着色器策略学 Impeller：全离线预编译打包，GPU 后端启动即建全部 PSO；berry-gfx 不给任何「运行时编译」入口。
4. 中间纹理加 `transient` 语义（day-one），TCG 软路径=一块复用 CPU buffer，真机 TBDR=tile memory。
5. 文字栈三段解耦；glyph 绘制做**中间件**复用当前 pass（glyphon 模式），控 pass 数。
6. B 协议 K≤8 dirty rect 与 frame_callback/credits 与 TBDR 天然契合；把「合并成大矩形」写进合成器策略（推断）。
7. GPU-driven 全 compute 不进 v1 主线（vello 降级实证）；Dawn 的 client-server WebGPU 是「GPU=用户态服务」的现成参照。
8. 参考许可全部安全：vello/parley/wgpu Apache-2.0 OR MIT、glyphon Apache/MIT/zlib、cosmic-text Apache/MIT、Dawn BSD-3——只看结构不搬代码（OpenHarmony/AOSP 同 Apache-2.0 铁律照旧）。

---

## 附录 D：w-apicompat——先进渲染 API「兼容 vs 适配」经济学

# 兼容了不等于适配好：先进渲染 API 在新 OS 落地的经济学

> 调研说明：无独立 web_search 通道（DDG/Bing 均被验证码拦截），全部改用一手来源直抓原文（Dawn/Fuchsia/Mesa 官方文档与源码树、Alyssa Rosenzweig 博客、gfx-rs 基准、Microsoft Learn、AOSP 文档）。全文 1666 汉字（≤2500）。证据/推断逐处标注。

## 一、三个一手样本的成本结构

**Fuchsia：押注标准 API，至今仍是 "Work in progress"。** Fuchsia 是新 OS + 现代图形最有钱的试验场，路径是自研微内核图形栈：Magma（Vulkan 驱动接口，RFC-0198）、Scenic Allocator（共享缓冲分配，RFC-0161）、Flatland（自研合成器，RFC-0162）、Escher（Vulkan 渲染器），并计划把 Dawn/WebGPU 作为一等前端。但 Dawn 官方支持矩阵在 Vulkan 后端一栏至今写着 **"Fuchsia: Work in progress"**；Fuchsia 第三方源码树里有 `third_party/mesa` 而没有 dawn，实际可用路径是 Mesa Vulkan（含 lavapipe 软件 Vulkan）+ Android 模拟器 goldfish GPU 直通（src/graphics/lib 下二者并列在册）。结论：即便 Google 级预算，把标准 API 移植到新内核也是以年计的工程——Magma 移植指南显示，一块 Vulkan 驱动进 Fuchsia 要重做驱动↔内核的全部接口面（地址空间、提交、内存）；加上合成器与分配器，「API 兼容」在时间轴上是最后一个里程碑，而不是第一个。
来源：<https://dawn.googlesource.com/dawn/+/main/docs/support.md> ；<https://fuchsia.googlesource.com/> ；<https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/graphics/lib> ；fuchsia.dev RFC-0161/0162/0198_magma_api_design ；<https://fuchsia.dev/fuchsia-src/development/graphics/magma/concepts/porting>

**翻译层税：可测、且不均匀。** gfx-rs 2019 年用 Dolphin（Metroid Prime 2 定点场景，20s 帧时间采样）对拍 MoltenVK 与自家 Vulkan-on-Metal 层：同为翻译层，平均帧时间 Intel 机型差 4%、AMD 机型差 22%（14.6ms vs 18.3ms），帧时间方差差近一个数量级。ANGLE README 展示翻译层的维护面：7 个后端（D3D9/11、GL、GLES、Vulkan、Metal、WebGPU 进行中），每个后端各自要过一轮一致性认证（D3D9 后端 2011、Vulkan 后端 2019–2020）；Chrome 在 Windows 上全部图形渲染压在它身上。zink 文档一句话点破分层语义：特性级别 = min(zink 实现，底层 Vulkan 驱动)。税种有三：CPU 侧语义错配开销、特性集交集缩水、双份一致性/回归维护。
来源：<https://gfx-rs.github.io/2019/03/22/dolphin-macos-performance.html> ；<https://chromium.googlesource.com/angle/angle/+/main/README.md> ；<https://docs.mesa3d.org/drivers/zink.html>

**Asahi：真机适配的价码表。** 一家 GPU、一个逆向小团队（以 Alyssa Rosenzweig 为核心，公开署名核心开发者个位数——推断）：2021-01 起逆向（part-2 系列日志 2021-01-22），2023-08-22 拿到全球唯一 M1/M2 一致性 OpenGL ES 3.1（原文：“通过数万项测试”），2024-06-05 Honeykrisp 成为 Apple 硬件上首个无 portability 豁免的 Vulkan 1.3 一致性驱动——明确不是从零写，而是复用 Faith Ekstrand 的 NVK 骨架；2024-12-02 同日达成 Vulkan 1.4（另有 GL 4.6 / ES 3.2 / OpenCL 3.0 一致性）。合计约四年。成本结构值得拆开：逆向 ISA 是明面成本；暗面成本是逐特性啃硬件限制——Mesa asahi 文档逐条列出 tiling 布局、稀疏页表、压缩格式等限制，每一条都对应 API 表面之下的适配工作；最后一条原文尤为关键："strided linear 图像只配给 modifier-unaware 的窗口系统用，性能会掉”。这印证：适配的最后一段永远长在窗口系统里（引文为证，后半句为推论）。

**样本对比的小结**：三家代表三种预算形态——Google 级预算仍被「新内核 × 标准 API」的接口面拖住；翻译层路径以 4–22% 帧时间税换入场券；从裸硬件到「真机适配良好」的公开价码 ≈ 四年 + 个位数顶级专家。
来源：<https://rosenzweig.io/blog/asahi-gpu-part-2.html> ；…/first-conformant-m1-gpu-driver.html ；…/vk13-on-the-m1-in-1-month.html ；…/vulkan-14-sur-asahi-linux.html ；<https://docs.mesa3d.org/drivers/asahi.html>

## 二、WARP：软渲染兜底的正确定位

微软 WARP 的诞生动机本身就有教育意义：其文档把第一收益点列为 **“消除各 OEM 自写软件光栅器的碎片化”**——历史上每家显卡厂商各养一个软渲染器，Windows 用一个一致性实现收编了这笔重复税。WARP 是“完全一致性”的 D3D10/11/12 软光栅器：无 GPU 机器无缝回退、进 DXGI 枚举、供 CI/诊断/无卡场景。官方量化：某些基准可胜低端核显，但低端独显快 4–5×；CPU 带宽 ~10GB/s vs GPU 20–100GB/s；原文 "WARP is not a replacement for graphics hardware"。它的价值=正确性/可测性路径，不是性能故事——这个定位边界值得所有自研 OS 抄走。
来源：<https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp>

## 三、「能跑但不优」的五层解剖

| 层 | 缺什么就掉性能/功耗 | 证据 |
| --- | --- | --- |
| 驱动接口 | 无类 Magma 层，真驱动进不来 | RFC-0198 + magma porting 指南 |
| 内存分配/共享 | 无 sysmem 类零拷贝分配，纹理 import/export 退化为拷贝 | RFC-0161；Asahi 修饰符一节 |
| 合成器协同 | GPU 合成 vs 显示硬件 overlay：后者"可显著更高效”，且逐机型协商 device/client composition | Android HWC 文档 |
| DPU/格式协商 | 修饰符不通 → 退 linear，性能掉 | Mesa asahi 文档 |
| 一致性/回归 | 翻译层每个后端各付一轮 conformance | ANGLE README |

这五层的共同点是：**没有任何一层住在 API 签名里**。API 兼容测试（画一个三角形）在四层全缺时也能通过；掉性能/功耗的地方全在提交、分配、合成、扫描输出的握手面上。所以“兼容了不等于适配好”不是修辞性抱怨，而是结构性的：兼容是点语义，适配是链路语义。

Android HWC：<https://source.android.com/docs/core/graphics/hwc>

## 四、对自研 OS 的三层决策框架

1. **语义边界自研、一次定形**：pass 形态 API、Caps 位、命令录制语义。这层自研不是 NIH——它是唯一必须与自家合成器/显示服务咬合的层；Fuchsia 的教训是连 Dawn 这种成熟实现都逃不掉为宿主 OS 重做 WSI/内存路径（"Fuchsia: WIP" 七年未落地）。
2. **现成标准直接吃**：Vulkan/WebGPU 对象模型与语义（不发明第四种语义）；shader 编译器（Tint/naga）；软件栈（lavapipe 类）做 CI 与兜底；conformance 思想当验收关卡。复用的省钱效应被 Honeykrisp 实证：复用 NVK 骨架使 Vulkan 1.3 一致性驱动大幅提速。
3. **留缝不实现**：真机厂商 GPU 适配（v1 无实机目标即不欠此债，但以 Caps/required-features 式能力协商预留差异——Dawn 的最小特性清单是现成“地板”范本）；GL 翻译层不进 v1（ANGLE 七后端维护面是大厂游戏）；高性能 DPU overlay 路径留缝（协议留 dirty_rects/vote 位），v1 以软渲染对标 WARP 定位兜底正确性。

**决策依据**可以收拢成三条判据：① 与自家合成器/显示服务咬合的 → 自研定形（咬合面外泄 = Asahi/Fuchsia 的 modifier 之疼）；② 语义已收敛且有持续上游维护的 → 直接吃（shader 编译器、软栈、conformance 工具链）；③ 只有实机厂商才付得起的适配面 → 留缝不实现，用能力协商封住变异性。三条判据的好处是可执行：每项新功能问一句“住在哪一层”即可自动归位。

## 对 BerryUI 的可迁移启示

- 「兼容 ≠ 适配好」的量化形态 = 五层中至少三层（分配、合成、DPU）不在 API 里，而在窗口系统与协议里。berry-gfx 的 B 形态三件事（submit/frame_callback/release）正是把咬合面协议化，与 Android HWC、Fuchsia Flatland、Asahi modifier 教训同构。
- Caps 位 day-one 在场是对的：Asahi 逐特性列限制、Dawn 用 required-features 画地板，能力协商是所有成熟栈的公共结构。
- 软渲染是兜底不是路线：对标 WARP——一致性、可测、无卡 CI；别把性能叙事押在 TCG 上。
- 复用优先序：shader 编译器 > 软件栈 > 驱动骨架 > 语义子集；唯一必须自研的是与自家合成器咬合的边界。
- 翻译层税真实存在（4–22% 帧时间差 + 双份 conformance）：勿为“以后兼容 GL/跑 Linux 程序”在 v1 欠下 ANGLE 类维护面。
- 适配债务可用一致性测试定价管理：Asahi 以数万项测试换“可跑任意 ES 3.1 应用”；BerryOS 的 G0–G5 关卡 + 机器可读遥测是同构资产——先自测，再对外承诺兼容。

---

## 附录 E：w-lowpower——低功耗图形栈结构性设计

# 低功耗图形栈结构性深研（w-lowpower）

> 定位：对 `docs/GUI渲染调研-2026-10-v1.0.md` 轴五（低功耗）的定量与协议层补深。全部关键论断经一手来源核验（本轮逐条抓取原文），标注「推断」者为本人综合。协议洁净：均只取结构与思路。

## 1 脏区 / PSR / DPU overlay：三张牌的定量地位

**PSR = 面板内 RFB 缓存最后一帧，内容不变时彻底停 DDR 流量**。内核 i915 源码 DOC 原文："eliminates display refresh request to DDR memory completely as long as the frame buffer for that display is unchanged…allows us to power down the link and memory controller"（eDP PSR；DSI 面板同思想叫 "manual mode"，即 command-mode/GRAM 局部刷新）〔github.com/torvalds/linux drivers/gpu/drm/i915/display/intel_psr.c〕。**证据落点：硬件 PSR 的前置条件就是软件 damage 纪律——GUI-06 的「合成结果缓存=软件 PSR」与硬件 PSR 是同一机制的软/硬两面，GUI-1 present 首版带 damage 因此不是优化项而是 PSR 的门**（有证据）。

**内核侧先例**：DRM atomic 的 `FB_DAMAGE_CLIPS`（drm_mode_rect blob）即部分刷新 uAPI〔kernel.org/doc/html/latest/gpu/drm-uapi.html；torvalds/linux include/uapi/drm/drm_mode.h〕——它是 2017 年才补进 atomic 的「后补案例」，见 §5。

**overlay/DPU 的实际地位**：AOSP HWC 文档——overlay plane 在显示硬件而非 GPU 合成；"Android devices typically support four overlay planes"；"the number of layers used by an app can have a measurable impact on power consumption and performance"；且有一条反直觉证据：**"Overlay planes may be less efficient than GL composition when nothing on the screen is changing"——此时 HWC 回退 GLES 合成并保留结果**〔source.android.com/docs/core/graphics/hwc〕。这直接支持 GUI-06「停帧+合成缓存」：省电正解不是选合成路径，而是消灭无谓的合成本身。

带宽算术（推断，纯字节量）：1080p@60 一次全屏合成 ≈ 1.0GB/s 读写；5% 脏区 = ~50MB/s，降 20 倍。overlay 收益同构：0 次 GPU 读写替代 2 次。

## 2 事件驱动渲染 vs 常态 vsync；帧率协商

**合成器按需重绘是 Wayland 十年验证的常态**：Weston 核心开发者 Pekka 原文："When the compositor is idle graphics-wise…it is not repainting continuously according to the output scanout cycle"〔ppaalanen.blogspot.com/2015/02/weston-repaint-scheduling.html〕。无 damage ⇒ 不排帧 ⇒ 帧回调停发 ⇒ 客户端阻塞休眠：**idle 功耗 = 0 帧管线、0 唤醒，而非「低帧率照跑」**。GUI-06「停帧优先」由此从设计偏好升级为有生产验证的结构形态。

**Wayland 十年补的两块协议拼图（后补痛点的证据）**：`wp_fifo_v1` 把“完成一次刷新周期”做成提交 readiness constraint，给客户端「阻塞-恢复」节流原语〔wayland.app/protocols/fifo-v1〕；`wp_commit_timing_v1` 按时间戳提交，"presented as closely as possible to, but not before, a specified time"——一次定时器等待替代逐帧轮询〔wayland.app/protocols/commit-timing-v1〕。时序反馈基座 = `wp_presentation`（presented 带 refresh/seq/硬件时刻）〔wayland.app/protocols/presentation-time〕。

**帧率协商**：Android 裁决形态 = `Surface.setFrameRate`（API 31）——**每 surface 声明意图帧率+换档策略，系统聚合选刷新率**〔developer.android.com/reference/android/view/Surface〕；意图是呈现元数据、与提交解耦。面板侧 LTPO = LTPS 开关 TFT + IGZO 驱动 TFT，低漏电使低频维持可行〔en.wikipedia.org/wiki/Low-temperature_polycrystalline_silicon §LTPO〕。

## 3 AOD 的实现层次

分层（框架为推断，各层事实有证）：①面板层——command mode/PSR 自刷新；Apple Watch S5 有 "low-power display driver capable of refresh rates as low as once per second"〔en.wikipedia.org/wiki/Apple_Watch_Series_5〕；②控制器层——低频低亮档（推断：对应 HWC 电源档）；③合成器层——ambient 冻结帧+低亮重合成一次；④应用层——Wear OS 规范："Keep at least 85% of the screen black (an on-pixel ratio of 15% or less)"、ambient 数据 "typically once per minute"（AmbientTickEffect）、系统自动防烧屏移位、禁大面积纯色/渐变〔developer.android.com/training/wearables/always-on〕。GUI-06 所引「AMOLED 全亮静态 300–400mW」本轮未能再验原页，**引用降级为待再验**；已验证替代锚 = on-pixel ≤15% + 1 分钟一拍。

## 4 合成器常驻的功耗代价

Weston 证据表明合成器可做到 idle 零唤醒——**前提是没有恒满足的唤醒源**（常驻帧回调、恒真 wait 项、软 vsync 空转；与仓内 P5 A-2 教训同型）。fifo/commit-timing 的存在本身即证明：连客户端节流都要机制化，靠应用自律不可靠。**停帧的代价也须写进协议**：Pekka 同文实证合成器从 idle 醒来 "waits for an extra vblank first"，首帧多付 1–2 个刷新周期延迟（Weston 后专门改算法回收）——低功耗与响应不重合，停帧是默认态，交互要有快速起振路径且该延迟应可观测。

## 5 day-one 决定功耗天花板：不可逆 vs 可后补

**不可逆（跨信任边界/要求客户端或硬件配合）**：①damage 语义进 present 首版——FB_DAMAGE_CLIPS 2017 年才补，无 damage 的老客户端喂不了 PSR（后补≠自动生效）；②帧调度模型（帧回调制+零回调 vs 常驻定时器）——fifo/commit-timing 花十年补节流与时间戳两块即为实证；③时序反馈位——无它则低功耗策略正确性不可观测；④buffer 分配协商（格式/modifier/scanout 兼容）——决定 overlay 直通可用性；⑤per-surface 意图帧率元数据位。**可后补（服务端内部可改）**：仲裁启发式、合成缓存实现、背光/热策略、AOD 场景内容、LTPO 档位表。GUI-06「API 语义 day-one、机制分刀」与此表一致，本节给了它判定标准。

## 对 BerryUI 的可迁移启示

1. present 首版四件套已裁——**补服务端不变量：「无 damage 不提交」+ 稳态零唤醒 smoke 断言**（Weston 同构，可测）。
2. 软 vsync 按需起振再加固：idle 时连合成器都不该有周期唤醒。
3. ambient 帧用 commit-timing 同构：绝对时刻一次 timer sleep 提交，不轮询（仓 tickless 同构）。
4. 背压协议化（fifo 同构）：合成器过载时钉住客户端提交、恢复时放行——节流是服务端权力。
5. 每图层帧率意图 = 呈现元数据位 day-one，仲裁权在显示服务（setFrameRate 先例）。
6. 合成 API 按 layer list：AOSP 实证 ~4 planes + 层数即功耗；层语义/能力枚举 day-one，软件实现起步。
7. AOD 三层策略按面板能力谱系自动选（on-pixel ≤15%/1Hz 帽/防烧屏移位），执法在 PM 服务；300–400mW 数字再验后方可入册。
8. 停帧的 1–2 帧唤醒延迟进 FrameTimeline 遥测 + 遥测环收脏区面积/层数/合成路径/帧率档——低功耗验收的证据面。

**主要来源**（均本轮抓取核验）：torvalds/linux intel_psr.c、drm_mode.h；kernel.org drm-uapi；source.android.com hwc；developer.android.com（Surface/wearables always-on）；wayland.app（presentation-time/fifo-v1/commit-timing-v1）；ppaalanen.blogspot.com 2015；wikipedia（LTPO/Apple Watch S5）。

---

## 附录 F：w-sdk——声明式 UI 与 SDK 开发体验（补跑全文·权威版）

# 声明式 UI 与 SDK 开发体验 2026 状态调研

## 一、四大框架内部机制：diff / 重组 / skip 与状态细粒度演进

**Jetpack Compose**：智能重组——输入不变即跳过该 composable，且“重组乐观、可取消、可能每帧执行”（官方心智模型：developer.android.com/develop/ui/compose/mental-model）。**Strong Skipping 自 Kotlin 2.0.20 起默认开启**：带不稳定参数的 composable 也变为可跳过（按实例相等比较）、composable 内所有 lambda 自动 remember 化；代价极小——官方实测 Now in Android 应用 APK 仅 +4kB（developer.android.com/develop/ui/compose/performance/stability/strongskipping）。有证据的结论：skip 从手写约定（remember/stable 注解）演进为**编译器默认产物**。

**SwiftUI**：iOS 17 起 `@Observable` 宏（Swift 5.9 macro）改造普通类型；官方表述：“body 执行时 SwiftUI 自动追踪所有对 Observable 属性的访问”，据此精确失效——依赖追踪粒度从 ObservableObject 的对象级降到**属性级**（WWDC23 #10149：developer.apple.com/videos/play/wwdc2023/10149）。

**ArkUI**：状态管理 V2（API 12+）`@ObservedV2`+`@Trace` 实现深度观测：被 @Trace 装饰的属性变化“**仅通知 property 关联的组件刷新**”；官方文档同时自认 V1 局限——嵌套类属性变化无法直接观测，须靠 @ObjectLink 手工拆组件（OpenHarmony 官方文档：github.com/openharmony/docs，zh-cn/application-dev/ui/state-management/arkts-new-observedV2-and-trace.md）。

**Flutter**：无编译期 skip，Widget 每帧重建、靠 Element 树 diff 保守更新；性能重心押在渲染层：Impeller 将着色器/管线状态**离线预编译**以消除运行时编译卡顿，3.27 起成为 iOS 与 Android API 29+ 默认引擎（docs.flutter.dev/perf/impeller）。

三家状态方案殊途同归：细粒度 =（属性，视图）依赖边；路线各异——Compose 编译期 memo、SwiftUI 宏读侧追踪、ArkUI 显式注解写侧通知。

## 二、「容易写 / 高效 / 正确」机制清单与代表数据

- **容易写**：声明式描述 + 热重载即时反馈。JetBrains Compose Hot Reload（JetBrains 官方项目）：改 UI 代码即时生效免重启，JVM 目标、Kotlin 2.1.20+（github.com/JetBrains/compose-hot-reload）；Dioxus 0.7（2025-09-08）Subsecond 实现 **Rust 运行时热补丁**，覆盖 Web/桌面/移动、含 Android 真机（经 adb reverse，不启模拟器）。
- **高效**：skip / 脏区 / 差量提交（见§一）；渲染侧管线离线烘焙（Impeller）。
- **正确**：编译期类型检查 + diagnostics；单向数据流纪律（Compose 官方要求 composable 幂等、无副作用，skip 可能性写进契约——mental-model 同页）。

## 三、载体之争：文本 DSL vs 宿主宏 vs 代码生成

- **文本 DSL**（Slint）：VS Code 插件提供补全、**diagnostics（编译期诊断）与 live preview 独立预览器**——不启动应用即可看 UI（snapshots.slint.dev/master/docs/slint/guide/tooling/vscode）。DSL 是数据文件，可被预览器、设计工具、AI 直接消费。
- **宿主宏**（Swift result builder / Rust `view!`/`rsx!`）：与语言工具链同源、类型一致性最强；但宏展开后的类型报错可读性差是两生态公认痛点（**推断**，未找到定量研究）；独立预览必须先过完整宿主编译。
- **AI 友好度成新权重**：Dioxus 0.7 发布 llms.txt 浓缩语法提示，明言配合热补丁使用户更有效 "vibe code" 应用（dioxuslabs.com/blog/release-070）。
- 综合判断（推断）：文本 DSL 在「独立预览器 + 独立诊断 + AI 可读」三面占优，宏在类型安全占优——互补而非互斥。

## 四、Rust 原生路线成熟度（★=2026-10-27 GitHub API 实测）

Dioxus 39.5k★ > iced 31.9k★ > Slint 24.3k★ > Makepad 7.2k★ > xilem 5.6k★。

- Dioxus 0.7：Subsecond 热补丁 + WGPU 原生 HTML/CSS 渲染器（弃 webview）；已接入 Bevy/iced 生态热补丁。
- iced 0.14 发布于 2025-12-07（GitHub releases）；xilem README 仍自称 **experimental**（github.com/linebender/xilem）——细粒度架构重写未完成、无生产案例。
- Makepad：自称 "live-editable design language" + AI 后端集成（github.com/makepad/makepad）。
- Leptos：细粒度 signal 做 "targeted updates"（leptos.dev）——信号式细粒度在 Rust 已验证。
- **许可警示**：Slint 三许可任选——闭源桌面/移动/Web 免版税、GPLv3 开源、**闭源嵌入式须商业授权**（slint GitHub README License 节）；嵌入式恰是其主打场景，授权排除面真实存在。

## 五、AI 生成→验证闭环（2024–2026）

- **结构化 dump 优于截图**已成生态共识：Playwright MCP（38.8k★）核心 verb `browser_snapshot`（可访问性树）自述 "better than screenshot"、LLM-friendly、无需视觉模型、“确定性工具应用”；`browser_take_screenshot` 仅作补充（github.com/microsoft/playwright-mcp）。mobile-mcp（10.1k★）将同一模式搬上 iOS/Android 真机（github.com/mobile-next/mobile-mcp）。
- 生成端：screenshot-to-code 80.9k★，验证“渲染结果→代码”路线的巨大需求（github.com/abi/screenshot-to-code）。
- 该组合与 BerryOS SDK-4 的 dump/inject/screenshot/reload 四 verb 几乎同构，且已被大生态验证（有证据）。

## 对 BerryUI 的可迁移启示

1. **skip 做成编译器产物**：DSL 编译器静态产出可跳过性与比较元数据；strong skipping 证明自动 memo 可行、体积代价 kB 级（NIA +4kB）。
2. **状态粒度 =（属性，组件）依赖边**：解释器维护读集→失效表，弃对象级通知；@ObservedV2/@Trace 的“仅通知关联组件”是同一收敛点。
3. **设备侧解释器是热重载捷径**：DSL 改动=数据改动，FAT 热区热重载免 Dioxus 式二进制补丁的全部复杂度；但状态迁移语义须显式（Subsecond 明示 struct 字段变更不自动迁移）。
4. **载体裁决**：文本 DSL 为第一格式（独立预览器/诊断/AI 可读全挂在其上），Rust 宏只做类型安全绑定糖层。
5. **错误前移**：UI 错误在预览器报，不进 QEMU 才报——把 Slint 式 live-preview + diagnostics 做进 bsh/toolchain。
6. **渲染学 Impeller**：着色器/管线状态离线预烘焙，防运行时编译抖动；TCG 软渲染下同样受益。
7. **SDK-4 对齐 Playwright MCP 语义**：结构化 dump 为一等公民（ABI 单源生成保证 AI 可读），screenshot 仅作像素终验；reload 与 dump 构成“生成→渲染→结构校验”闭环，契合 AI 测试友好化支柱。
8. **许可护栏**：Slint 闭源嵌入式须商业授权印证 berryui 全自研裁决（ADR #50–#54）；xilem/iced/Dioxus/Leptos 均 MIT/Apache 系，读源学机制零传染风险。

---

## 附录 G：w-compositor——微内核/多进程 GUI 架构与交互延迟预算

证据已齐。综合成文：

---

# 微内核/多进程 GUI 架构与交互延迟深研（w-compositor）

> 证据基座：Fuchsia 官方文档（Scenic/Flatland/frame-scheduling/life-of-image/Flatland RFC）、AOSP 图形文档（VSync/BufferQueue/HWC/Sync/FramePacing）、Wayland 协议规范、QNX Screen 8.0 文档、AOSP 源码（DisplayEventReceiver）。与仓库 `docs/GUI渲染调研` 互补，不复述其 Android vsync 面。

## 一、四家谱系：架构与教训

**Fuchsia Scenic —— API 形态错配导致整体重写。** Gfx 向客户端暴露 3D 场景 API，而“Fuchsia 客户本质是 2D 产品”，官方明言该 API “从产品与性能两 standpoint 均不再适用”（[Flatland RFC/roadmap](https://fuchsia.dev/docs/contribute/roadmap/2021/flatland)）：3D 内容塞不进 VDC 的“2D X/Y 矩形层”范式，堵死了硬件 overlay/降功耗的优化路径。重写后的 Flatland 五要点：①批量命令 + 显式 `Present()` commit，不搞客户端侧入队；②有状态增量（两次 Present 间可渐改场景）；③尽量委托 VDC 合成，“降 Scenic GPU 占用、省功耗”；④**确定性 CPU 成本**：唯一 deadline 调度的渲染线程，每 session 独立 dispatcher 互不拖累；⑤分配强制走 Allocator 协议，Image 传递零拷贝（[Flatland](https://fuchsia.dev/fuchsia-src/concepts/ui/scenic/flatland)）。帧调度上 Scenic 用 per-client 队列 + **latch point**（过了 latch 才锁定本帧内容，迟到的顺延）+ **squashing**（默认可折叠：积压的多帧合并为一帧，`unsquashable` 保底至少显示一帧 vsync 周期）+ 把预测/实际 latch point 回传客户端供低延迟排程（[frame scheduling](https://fuchsia.dev/fuchsia-src/concepts/ui/scenic/frame_scheduling)）。教训：**API 形态即性能上限**，3D 通才 API 是合成器优化之敌。

**QNX Screen —— 有界队列 + pull 模型把背压写进合同。** Screen 的“窗口”可组合来自不同 renderer（可跨进程）的内容（[Windowing](https://www.qnx.com/developers/docs/8.0/com.qnx.doc.screen/topic/manual/cscreen_windowing.html)）；事件由**服务端每 context 维护固定队列（默认 200 条，不增长），客户端必须主动 `screen_get_event()` 拉，不拉就丢**（[Event handling](https://www.qnx.com/developers/docs/8.0/com.qnx.doc.screen/topic/manual/cscreen_event-handling.html)）；buffer 分 internal（API 创建）/external（应用自有内存 attach 后才能渲染）两态。教训：**慢消费者的后果是协议明文（丢事件），不是无限缓冲**——对低功耗 OS 这是正确的默认。

**Wayland/weston —— 消灭中间人 + 节流契约。** X 架构里合成器仍要绕 X server 多两次上下文切换；Wayland 直接“compositor 即 display server”（[架构文档](https://wayland.freedesktop.org/architecture.html)）。`wl_surface.frame` 明文是一份**节流契约**：“请求‘适合开始画下一帧’的通知……提前 commit 的更新很可能上不了屏，纯属浪费”（[协议](https://wayland.app/protocols/wayland)）；`wp_presentation` 补可靠性遥测：每个 commit 以 `presented`（带真实时间戳，可由 flip 完成 + 物理显示路径延迟近似）或 `discarded` 终态收口（[presentation-time](https://wayland.app/protocols/presentation-time)）。版本化 per-interface：`wl_registry.global` 携带 interface+version，bind 时协商。

**Android SurfaceFlinger —— BufferQueue 四口 + fence。** BufferQueue 由 **consumer 创建并拥有**、可跨进程连接 producer；`dequeue/queue/acquire/release` 四口，槽位池深度由生产消费双方协商；**buffer 永远按 handle 传递，绝无拷贝**（[BufferQueue and Gralloc](https://source.android.com/docs/core/graphics/arch-bq-gralloc)）。sync 框架（sync_timeline/sync_pt/sync_fence）把“像素写完了没”变成跨驱动/跨进程可传的显式依赖——GPU 未画完，fence 先行，显示控制器到点即显（[Sync framework](https://source.android.com/docs/core/graphics/sync)）。基线管线：显示在放 N、SF 合成 N+1、App 渲染 N+2，**天然 ≥2 帧延迟**；用 VSYNC/SF_VSYNC 相位偏移压缩；DispSync 是以 retire fence 时间戳为反馈的软件 PLL；HWC vsync 回调在 HAL_PRIORITY_URGENT_DISPLAY 线程上**典型 <0.5ms**（[VSync](https://source.android.com/docs/core/graphics/implement-vsync)）。

## 二、零拷贝与 direct scanout：何时值得

值得的判据：像素量大到拷贝结构性不可接受——Flatland 原文："avoid any copies because graphical buffers are big"（[life of a Flatland image](https://fuchsia.dev/fuchsia-src/concepts/ui/scenic/life_of_a_flatland_image)）。机制三件套在两家高度一致：共享内存对象（sysmem/dma-buf）+ **约束协商**（sysmem 全体参与者 token 交齐、约束求交才分配，任一 token 未用则挂起——有界等待是合同一部分）+ fence 序（AOSP sync / dma-buf fence，[内核文档](https://docs.kernel.org/driver-api/dma-buf.html)）。权限即能力：Flatland 用 Import/Export token 对，**“token 所有权模型而非显式 release，降低误用与泄漏”**——句柄掉光即资源释放信号。

何时不值得：AOSP HWC 原文给出反例——**屏幕静止且有透明像素叠加时，overlay 直通可能比 GL 客户合成更耗电**，HWC 会回落 GLES 并保留合成结果（[HWC](https://source.android.com/docs/core/graphics/hwc)）。direct scanout 的净收益 = 省 GPU 读写（Flatland RFC：降功耗降 GPU 占用）减去约束成本（格式/modifier 对齐、损坏申报需服务端校验、截屏/隐私面）。**推断**：BerryOS QEMU 期（TCG 无 GPU）零拷贝的收益不是省 GPU 而是省一次 1.83MB/帧（800×600×4）memcpy + 把多进程共享 VMO 通路立起来；直通只做语义留位。

## 三、输入→显示延迟预算拆解

| 段 | 量级（证据/推断） | 出处 |
| --- | --- | --- |
| vsync 管线总基线 | **≥2 帧**（33.3ms@60Hz），offsets 后可压至 ~1.x 帧 | AOSP VSync 原文“latency is at least two frames” |
| HWC vsync 回调注入 | <0.5ms | 同上 |
| App 内：输入消费+layout+录制 | 半帧内（帧头同拍消费，见下） | 推断 + Choreographer 语义 |
| 合成（SF/latch→提交） | latch 机制内 0~1 帧，迟到顺延 | Fuchsia frame scheduling |
| scanout+面板 | 设备相关，presented 时间戳 = flip 完成 + 物理路径延迟 | presentation-time |

**“输入与帧同拍”（doFrame）**：Choreographer “从显示子系统收 timing 脉冲（vsync），把动画、输入、绘制调度进下一个显示帧”（[Choreographer](https://developer.android.com/reference/android/view/Choreographer)）——输入在帧头被采样消费、结果当帧产出，配合 offset 相位（App 帧最早、SF 居中、显示收尾）。这正是仓库轴四“相位化回调序”的一手出处；QEMU 期软 vsync 下该预算表应原样进 FrameTimeline 双时间线断言（推断）。

## 四、高频帧信号为何不走通用 RPC——三个独立先例

1. **Android**：vsync/帧事件走 `DisplayEventReceiver` = **BitTube（一对 fd 的 socket 通道）**，Looper/epoll 监听，不经 Binder（[源码](https://android.googlesource.com/platform/frameworks/native/+/refs/heads/main/libs/gui/DisplayEventReceiver.cpp)：`mDataChannel = make_unique<gui::BitTube>()`；[BitTube.h](https://android.googlesource.com/platform/frameworks/native/+/refs/heads/main/libs/gui/include/private/gui/BitTube.h) 收发各持 unique_fd）。每秒 60–120 次的信号走 RPC 会在调度/优先级反转上输给专用 fd 通道。
2. **Wayland**：frame callback 是合成器**主动推 event**，客户端被动节流；协议即契约（见 §一）。
3. **QNX**：服务端固定队列 + 客户端 pull，事件可丢，永远不阻塞合成器。

共同形态：**高频信号 = fd/队列单向推送 + 消费端多路等待，低频控制 = 请求-响应**。与本仓 IPC 设计 §7“热路径走共享环”立法同构（推断）。

## 五、单 GUI 进程 → 多进程合成器的 day-one 缝

- **buffer 所有权**：池由**合成器侧（consumer）拥有**（BufferQueue 模式），客户端只持 handle/槽位；或 token 对所有权模型（Flatland）。两者都天然映射能力模型：句柄即权限，掉光即回收。
- **权限边界**：buffer 穿边界只传 handle + usage/约束三元组（gralloc usage 协商 = 使用意图声明）；约束求交失败即拒分配；damage 申报是信任声明，服务端必须校验（否则恶意小脏区实写大区域）。
- **协议形态**：统一为“pending 态 + 显式 commit 原子生效”（Wayland double-buffered state / Flatland Present 批量命令）——免撕裂、可折叠、可 latch，是拆分前后都不变的语义。
- **版本化**：interface 级 version 协商（Wayland global/bind）；特性协商（BufferQueue 深度）代替枚举爆炸。
- **拆分点唯一**：B 形态三件事（submit/frame_callback/release）已把“合成”抽象成协议；单进程期 guifbd 直接函数化消费，拆分时仅把合成侧搬进新进程，**协议零改动**（推断，与 ADR #50 一致）。

## 对 BerryUI 的可迁移启示

1. **API 形态定性能上限**（Scenic 之鉴）：berry-gfx 的 layer list 严格 2D X/Y 矩形层语义 day-one 冻结，给硬件 overlay 留零破坏升级缝。
2. **latch point + squashing 进 guifbd 协议**：submit 带 target present 时刻，迟到内容顺延并可折叠；unsquashable 位保底交互帧。
3. **每 context 有界事件队列 + pull**：背压写进合同（QNX），丢事件计数进遥测，不做无限缓冲。
4. **帧回调走专用 fd/SPSC 环**（BitTube 先例），绝不走通用 channel RPC；vsync 发生器 fd 化，无订阅者即停振。
5. **buffer 池归合成器侧所有**，客户端持 handle/槽位；Import/Export token 对所有权释放模型直接借（与能力模型同构）；约束协商“全体 token 交齐才分配”防半分配态。
6. **fence 等价物 v2 ADR 提前**：现在就把“submit 附带完成语义”留成协议位，防 v3 GPU 接入破 ABI。
7. **direct scanout 按“静止+透明反而更耗”的实证留为能力位**，软件遮挡感知先做，VDC/overlay 委托等硬件档。
8. **FrameTimeline 断言预算表化**：≥2 帧基线、<0.5ms 回调注入、latch 顺延——每段可归因，QEMU ppm 对拍走 icount 通道。
9. **协议版本 per-interface 协商**，guifbd 三件事 crate（berry-guiproto）现在落 version 字段。

**主要来源**：[Flatland RFC](https://fuchsia.dev/docs/contribute/roadmap/2021/flatland) · [Scenic](https://fuchsia.dev/fuchsia-src/concepts/ui/scenic) · [frame scheduling](https://fuchsia.dev/fuchsia-src/concepts/ui/scenic/frame_scheduling) · [life of image](https://fuchsia.dev/fuchsia-src/concepts/ui/scenic/life_of_a_flatland_image) · [QNX Screen](https://www.qnx.com/developers/docs/8.0/com.qnx.doc.screen/topic/manual/cscreen_about.html) · [Wayland 架构](https://wayland.freedesktop.org/architecture.html) · [wayland 协议](https://wayland.app/protocols/wayland) · [presentation-time](https://wayland.app/protocols/presentation-time) · [AOSP VSync](https://source.android.com/docs/core/graphics/implement-vsync) · [BufferQueue](https://source.android.com/docs/core/graphics/arch-bq-gralloc) · [HWC](https://source.android.com/docs/core/graphics/hwc) · [Sync](https://source.android.com/docs/core/graphics/sync) · [dma-buf](https://docs.kernel.org/driver-api/dma-buf.html)
