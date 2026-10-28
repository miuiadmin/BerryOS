# GUI-09 综合裁决建议——八维调研收敛、七大区裁决与 M-GUI 逐刀对表

> 调研日期 2026-10-28 · 产出者 = M-GUI 综合裁决 workflow（run wf_ad173748-caa）
> **定位**：本报告是给用户做裁决用的支持文件——每个大项给「推荐 + 备选 + 不推荐」三档与依据，不替用户拍板。八份专项调研（GUI-01..08）已各自给证据；本报告只做三件事：跨维度收敛共识原则、七大裁决区给三档建议、对现行规划（`docs/GUI里程碑规划-2026-10.md`）逐刀给改/留/增。
> **输入材料**（全部通读）：
> - `research/GUI-01-安卓渲染响应慢教训.md`（279 行）· `research/GUI-02-领先图形管线实践.md`（272 行）· `research/GUI-03-先进渲染API全景与适配.md`（287 行）· `research/GUI-04-Rust渲染生态评估.md`（279 行）· `research/GUI-05-微内核OS的GUI架构.md`（420 行）· `research/GUI-06-低功耗渲染策略.md`（291 行）· `research/GUI-07-声明式UI框架对比.md`（249 行）· `research/GUI-08-输入管线与开发套件.md`（246 行）
> - 现行规划：`docs/GUI里程碑规划-2026-10.md`（275 行，GUI-1..9 刀序 + 裁决 A–F）
> - 姊妹综合（对表用，非本 workflow 产出）：`docs/GUI渲染调研-2026-10.md`（144 行，五轴定案 + 对抗校验 22 条）——两份综合的三处分歧在 §5.1 如实并列
> **证据纪律**：本报告为二次综合，论断一律回引报告编号（GUI-01..08）；报告内每条证据带 URL/路径与确定度标注，此处只在关键论断上沿用【高/中/低】标记。独立信息源计数 = 八份专项报告（合计 100+ 条联网来源 + 10 处本地源码/文档取证）+ 规划稿 + 姊妹综合 + 本仓 CLAUDE.md/既有调研，远超 8 个独立来源门槛；原始 URL 见各报告参考资料清单。
> **用户六条裁决**（2026-10-28，全报告对账基准）：①最先进图形渲染方案不将就 ②一切按移动设备设计 ③吸取安卓渲染响应慢教训 ④又要低功耗又要优秀 ⑤先进渲染 API 兼容但「兼容≠适配好」 ⑥开发套件要最先进（容易渲染、高效、正确）。

---

## 0. 一页总裁决

**核心张力的解法（本报告最重要的一条）**：用户要求「移动端基准的最先进方案、不自我设限」，现实是 QEMU 无 GPU、TCG 吞吐有限、VMO 32MiB 帽。八份报告联合给出的答案是：**架构面（协议语义 / API 形态 / 服务边界 / 升级缝）按终态设计，实现面按可达路径渐进**。检验现行规划后结论是——规划的**进程骨架（三薄服务 + 单 GUI 进程）与终态收敛形态（GUI-05 §6.1 的 B 形态：buffer 过界 + damage + frame callback）天然兼容，不需要推翻**；需要按终态**前置定形**的是五个协议语义面：damage 字段（GUI-05/06）、帧回调「未来时刻表 + credits」（GUI-05）、帧率投票区间票面（GUI-06）、pass 形态渲染 API 语义（GUI-03）、停帧三态与能耗代理验收（GUI-06）。这些在 GUI-1..4 定形成本最低，后补 = 协议版本破裂或全局改写（GUI-06「砍实现不砍协议位」）。「最先进」的兑现路径 = **协议面超前于实现面**，而不是实现面跳步。

| 裁决区 | 一句话结论（三档详见 §2） |
|---|---|
| a 渲染栈 | 薄 HAL（WebGPU 语义子集：pass/loadOp/storeOp/命令录制/VMO 资源/AOT 管线描述）+ 后端序列 soft（永久 CI 基线）→ virtio-gpu 2D → native Vulkan；光栅核 vello_cpu 依赖审计 spike 过门才定（GUI-03/04） |
| b UI 框架 | retained tree + dirty region 是 v1 必需（低功耗 + TCG 双约束），但 API 从第一天数据驱动（struct 属性/enum 事件/Msg-only 回调）为声明式叠加留缝（GUI-04/06/07） |
| c 服务摆位 | 维持三薄服务 + 单 GUI 进程；guifbd 契约从第一刀按 B 形态三件事协议化成 berry-guiproto；显示驱动契约按 HDI 三口（GUI-05） |
| d 帧调度 | 软 vsync = 三家真实运行形态（Android DispSync/Fuchsia/OH 全是软件生成分发）；request-driven 专用通道 + UI/合成错相 + 未来时刻表回调（GUI-01/02/05） |
| e 输入 | 服务化采集 → PointerEvent 带采样时间戳序列 + stable pointerId → wmd 窗口表命中 → 与帧同拍消费；重采样放消费侧最终一跳（GUI-08/05/01） |
| f 低功耗 | damage 协议字段 + 三态帧循环（IDLE 撤定时器）+ vote 区间票面 + power/mode op + PM 共享环 {target,actual} + 能耗代理断言，全部 day-one 留协议位（GUI-06） |
| g 开发套件 | SDK-1..5 演进序（dump/注入/截图 round-trip → DSL+解释器+FAT 热重载 → 帧时间线 → AI 生成闭环 → 设计器）；验证环是差异化（GUI-08/07） |

**刀序总评**：GUI-1..9 骨架全部保留（无一把推翻）；GUI-1/2/3/4 四把在「协议面/API 面」上改描或增强；建议增两把前置/并行刀（GUI-0 FPU/FPSIMD 上下文保存——硬门；SDK 线与 GUI 线并轨——GUI-8/9 挂 SDK-1）。裁决 A–F 全部保留，其中 D 升级措辞（运行时协商进协议版本域）、F 增 views/ 同刀。建议新增裁决 G（低功耗缝 day-one）/H（SDK 演进序）/I（berry-guiproto 提前）。

---

## 1. 跨维度共识原则（八维收敛，每条 ≥3 维度 / ≥3 系统佐证）

> 收敛判据：同一形状在 ≥3 份报告（覆盖不同调研维度）中、由 ≥3 个独立系统的证据支撑。这 12 条是七大区裁决与逐刀对表的公共依据。

**P1 提交「变更描述」而非像素——内容过界走 buffer 句柄 + 元数据，永不走 IPC 消息体。**
佐证：Wayland attach/damage/commit + wl_buffer.release、Fuchsia Flatland 保留树增量 Present、Android BLAST 每 buffer 一个原子事务（GUI-05 §2/§1/§3）；iOS CATransaction commit = 参数 + 曲线非帧（GUI-02 P1）；Android display list/RenderNode（GUI-01）；Chromium display list（GUI-02）；OH RSCommand 录制回放（GUI-05 §5.1）。BerryOS 映射：像素 ≤256B channel 消息装不下（800×600×4≈1.9MB），VMO handle 过 channel + 小消息传元数据是唯一形态——微内核天然做对，坚持住（GUI-05 §7、姊妹综合轴四同判）。【高】

**P2 帧时钟权威单点 + 客户端「未来时」倒排——回调载荷是未来 presentation 时刻表，不是脉冲。**
佐证：Flatland OnNextFrameBegin 携 FuturePresentationInfos（≤8 项）+ additional_present_credits；OH RequestNextVSync 请求驱动；Wayland frame callback 合成器主导；Android FrameTimeline desiredPresentTime 节流（GUI-05 §1.3/§5.3/§2.4/§3.1）；iOS ProMotion 系统仲裁 + 应用 hint（GUI-02）；Choreographer 输入/动画/绘制同拍（GUI-01/05）。BerryOS 映射：guifbd 为帧调度权威，帧回调从第一版就带未来时刻语义（软 vsync 周期表即可实现）（GUI-05 §9.3）。【高】

**P3 damage 是协议字段，不是渲染实现优化——「无 damage 不送帧/不合成」写进契约首版。**
佐证：Wayland damage request、OH HDI SetScreenClientDamage、ui_lite 脏区代数（AddInvalidateRect/DivideInvalidateRects/OptimizeInvalidateRects）、Android surface damage（GUI-05 §2.2、GUI-06 §1.2）；PSR2 selective update 田野故障证明 damage 认知必须软件显式化（GUI-06 §1.1）；Fuchsia 层更新粒度即脏区（GUI-05 §9.2）；Android overdraw/遮挡剔除与脏区合并并列（GUI-01）。BerryOS 落点：flush 恒带矩形集（K≤8）+ FULL 退化位，GUI-1 起协议一次定形（GUI-06 §7.1）。【高】

**P4 帧循环需求驱动——静止即零工作，三态停帧是「又要低功耗又要优秀」的统一杠杆。**
佐证：Android Choreographer 仅有 invalidate/动画/输入时排 vsync、SurfaceFlinger "wakes only when there's work to be done"（GUI-06 §1.4、GUI-05 §3.1）；OH 动画 idle 2000ms 降档（GUI-06）；LVGL timer_handler 返回距下一定时器毫秒数供睡（GUI-06）；Wayland 对离屏面停发回调（GUI-05 §2.4）；egui 即时模式与低功耗结构性冲突被四报告共同否决进主线（GUI-04/06/07）；显示是第一耗电大户（视频场景 38–68%）、120Hz 恒开多耗 10–30%（GUI-06 §3）。BerryOS 映射：RUN/SETTLE/IDLE 三态，IDLE 撤软 vsync 定时器睡在事件上——tickless「空闲不白醒」在 GUI 层的同一条铁律（GUI-06 §7.2）。【高】

**P5 帧率协商 = 区间投票 + 系统仲裁，票面是 ABI、规则是策略。**
佐证：iOS CAFrameRateRange(min/maximum/preferred)（GUI-06 §2.3、GUI-02 ProMotion）；OH VoteInfo{min,max} 具名 voter + MergeRangeByPriority（GUI-06 §2.4）；Android setFrameRate 投票语义 + ARR 最高票胜出/倍数取高/touch boost/清票纪律（GUI-06 §2.2、GUI-01）。收敛点：**票面是区间不是单值**（iOS/OH 双实证），仲裁规则可换（GUI-06 §7.3）。BerryOS 落点：QEMU 单帧率环境三扇出全 no-op + 遥测记录——接口先活、消费后到。【高】

**P6 缓冲深度 = 延迟——跟手性靠砍级数，不靠逐级调优。**
佐证：端到端典型 3–5 帧@60Hz ≈ 50–85ms vs 人可感知 ~24ms 级（CHI 2012 触笔研究）；Flutter 自绘层多一帧是公开 issue；HN「渲染快但延迟高」讨论（GUI-08 §1）；Android 无偏移时固有 ≥2 帧延迟（官方原文）+ DispSync 错相压一帧（GUI-05 §3.2、GUI-01）；iOS drawable pool 2–3 背压（GUI-02 P5）。BerryOS 红利：单 GUI 进程直持 fb 的三薄服务天然少两级缓冲（GUI-08 §1.3）；present credits ≤2 显式写进契约（GUI-05）。【高】

**P7 输入与帧同拍 + 消费侧重采样——采样/递送/渲染三解耦。**
佐证：Choreographer 输入/动画/绘制一次 doFrame（GUI-01/05 §3.2）；iOS touch = render loop 第一阶段 + coalesced/predicted 双 API（GUI-02、GUI-08 §2.2）；AOSP 重采样位于消费侧 InputTransport（内插优先/外推上限 min(Δ/2,8ms)/5ms 安全垫/一致性替换/硬件同步时禁用）（GUI-08 §2.1）；输入优先三层（URGENT_DISPLAY -8 / POWER_HINT_INTERACTION / ADPF 契约化）（GUI-08 §3.1）。BerryOS 落点：重采样放 gui-wmd→应用最后一跳、常量集中定义 + host 纯函数测（GUI-08 §2.5）。【高】

**P8 度量体系 day-one——expected/actual 双时间线 + 机器可读遥测，jank 判定按偏差不按固定 ms。**
佐证：Android FrameTimeline/JankStats/FrameMetrics（GUI-01）；Perfetto trace 可 SQL 查询 + EventLatency 输入→帧关联（GUI-08 §4.3、GUI-02 P9 dropped frames observable）；web.dev INP 三段分解（GUI-08）；Slint MCP / JetBrains MCP = AI agent 驱动运行中 GUI 已产品化（GUI-07 §6.1、GUI-08 §5.2）；帧预算断言走 icount 通道（GUI-07 §4）。BerryOS 映射：软 vsync 第一刀就打点 input_ts → frame_begin/end → flush_done 全链 CNTVCT 时间戳，同时喂 smoke 断言/性能工具/AI 遥测（GUI-08 §8.2）。【高】

**P9 无运行时编译、帧路径零分配——AOT 是移动端行业共识。**
佐证：runtime shader 编译是移动 jank 第一源 → Impeller/Metal 4 离线编译（GUI-03 §5）；Compose 用 ~5 年 + Strong Skipping（2024）才把「手动 memo 纪律」转给编译器、React Compiler 同步收敛（GUI-07 §3/§1）；Rust 无 GC 是最大结构性红利但零分配帧循环仍需纪律 + 断言（GUI-01、姊妹综合轴四）。BerryOS 映射：管线/模板静态提纯为 day-one 不变量（可序列化管线描述），帧循环零分配照遥测环纪律立法。【高】

**P10 软后端永久在册 + 升级缝走协议——「换实现不换契约」。**
佐证：Flutter GLES fallback 腐烂（#179268）、Skia Graphite 收缩到 Dawn 单语义层（GUI-03 六案库）；OH 渲染后端三分支 + RASTER 纯软回退是一等路线（GUI-05 附录 B L0）；HDI 三口（RegVBlank/Prepare/Commit）让 virtio-gpu 换 ramfb 不换契约（GUI-05 §5.2/§9.7）；CPU 软渲染在 GUI-04 是 berry-gfx 双核之一而非降级。BerryOS 落点：soft 后端永久进 smoke 主相防 fallback rot（GUI-03 §6.4）+ 双后端像素共识测试挂机器守卫（mkfat --check-fixture 先例）。【高】

**P11 表达力贴硬件范式（2D 矩形层 + 平移/缩放），不追图形学通用性。**
佐证：Fuchsia 放弃通用 3D 场景图转 Flatland 2D（客户端全是 2D 产品 + 显示硬件 overlay 严格 2D 范式，七年学费）（GUI-05 §1.1）；PSR2/SPI 面板/ui_lite/SurfaceFlinger 脏区全是矩形代数（GUI-06 §7.1）；tile GPU 四移动厂商收敛 pass/loadOp/storeOp/transient 语义（GUI-03 §3）。BerryOS 落点：berry-ui 视图树变换只做 2D 仿射不是将就；guifbd 契约里的「层」保持可直扫形态。【高】

**P12 自建薄 HAL/库，不引入整框架——公共地基是 crate 库而非服务内置。**
佐证：Google 自己从 N 后端收缩到 Dawn 单语义层 + 原生后端（GUI-03）；Rust GUI 无共识、所有框架预设桌面地基（winit/字体/剪贴板）BerryOS 必须自建（GUI-04）；wlroots/Smithay 教训 = 协议面可以小但实用层必须有可复用库（GUI-05 §2.5）；Slint GPL/商业双许可红线、egui/iced 各有结构性不适（GUI-04/07）；QNX 车载标杆也在吸收 Wayland 语义而非自造（GUI-05 §4）。BerryOS 落点：berry-gfx/berry-ui 做 crate（host 可离屏测 = AI 测试友好化），GUI 服务只是组装点；许可门禁 ADR（crates.io + LICENSE 双源核对，GPL/Slint 自动拒）。【高】

---

## 2. 七大裁决区逐项裁决（推荐 / 备选 / 不推荐 + 反对意见与风险）

### 2.a 渲染栈分层与后端序列

**现状**：规划稿 GUI-2 = 六原语 trait 化 + 软实现；姊妹综合轴一已改描为「自持编码层 + vello 生态（spike 过门定后端）」。

- **推荐**：**薄 HAL 自建 + 三档后端序列**。API 语义取 WebGPU 子集形态（device/queue/texture/render pass/loadOp/storeOp/命令录制/limits），**从 v1 就按 pass 形状定形、永不写 GL 形状**（GUI-03 §6.3：pass 形状是四家 tile GPU 的功耗正确性基线，GL 形状日后 = 破坏性改版）；资源 = VMO-backed；管线描述 = 可序列化 AOT 不变量；per-backend Caps/quirk 注册表 + 遥测。后端序列：**soft（永久 CI 基线，防 fallback rot）→ virtio-gpu 2D（virtio-mmio 挂 `virtio-gpu-device`，GUI-03 判定开箱即用）→ native Vulkan（v2+）**。光栅核：自研 rect/glyph 快速路径 + **vello_cpu（Apache-2.0 OR MIT、no_std、纯 CPU Sparse Strips）候选**——但依赖审计 spike 过门才定（aarch64/berry-libc 编译 + TCG SIMD 成本走 B-2 icount；备胎 = 内联 sparse strips 思想自研子集）（GUI-04 §8、GUI-03 §6.2、姊妹综合轴一主从关系修正）。「兼容≠适配好」落地三件：语义形态亲和（概念/命名 1:1 对齐 WebGPU，教程与 AI 语料直接可用）、真生态互操作推迟到 v3 wgpu-hal 自研后端、兼容层永远在翻译层不在原生路径内部（GUI-03 §6.3、姊妹综合轴二执法三条）。
- **备选**：wgpu 底座（v2+ 架缝候选再评估——v1/v2 无 GPU API 结构性不可达，GUI-04）；tiny-skia 作 CPU 对拍基准（GUI-04：作者自定位研究项目，不作主引擎）。
- **不推荐**：virgl（aarch64 issue #1141）、llvmpipe/lavapipe 移植（LLVM/C 栈不可移植）、SwiftShader（Chrome 已退役）——GUI-03 QEMU 地形判定【高】；Skia/rust-skia 实体引入（C++ 工具链缺失 + 几十 MB，方法论吸收即可）；GL/GLES 形状原生 API；六原语自研当**渲染核心**（撑不起「最先进」——无路径/抗锯齿/合成/文本；但留作 boot/诊断层与快速路径，GUI-03/04 与姊妹综合在此收敛）。
- **反对意见与风险**：①vello_cpu 论文优势集中在大形状负载，UI 主负载是小形状/圆角矩形/字形密集区间——spike 必须用真实 UI 负载对拍（姊妹综合对抗校验修正）；②「WebGPU 子集」无限膨胀风险——边界清单随 GUI-2 ADR 落纸（哪些概念进 v1/v2/v3）；③「兼容」超卖风险——真 wgpu 程序需要 WGSL 编译器/完整纹理枚举，CPU 语义子集承载不了，宣传口径必须是「语义形态亲和」（姊妹综合 wrong#1 修正）。

### 2.b UI 框架形态（含布局引擎子项）

**现状**：规划稿 GUI-3/6 = Rust 命令式保留模式 + 手动 Invalidate + 自研 GridLayout。

- **推荐**：**保留树 + 脏区为 v1 必需底座（裁决正确），API 从第一天按「数据驱动」设计**：View 属性全 struct 化、事件全枚举化、回调只投递 Msg 不执行闭包副作用（Elm 式 Model/Msg/update 纯函数内核，view/update 全 host 离屏测试 = fbcon 28 测同法）（GUI-07 §7.2/§9.1-2）。失效粒度两阶段：v1 视图级 Invalidate → v1.5 属性级 dirty 位（键路径 diff，**不引入运行时信号图**——规避 Leptos 借用/订阅图冲突，GUI-07 §7.2.4）→ v2 编译期模板提纯。表意层 v1.5 叠窄域 Rust 宏 DSL（rsx 风格：树字面量 → 构造 API 机械展开，rustc 当全强度验证器，DSL 域刻意窄无宏内语句层）；v2 增 JSON schema 组件树作 AI 动态 UI 缝（与宏共用组件注册表——AI 可生成集 ≡ 开发者可手写集）（GUI-07 §5.3/§7.2）。即时模式只给 DFX/调试 overlay 小面。动画属性 Transform/Layout 分型（合成器可计算属性 vs 需重布局属性），为服务端动画留缝（GUI-02 P1、GUI-05 附录 B）。布局子项：**Taffy（MIT）移植优先、自研线性+网格为备选**——flexbox 是 Web 背景开发者与 LLM 最熟悉面（姊妹综合轴三），且过 crate 预检后 grid 是其特例；GUI-07 §4 的「lite 线性+网格两算法即可表达桌面」成立但表达力上限低。
- **备选**：自研线性 + 网格两算法（更小、零依赖、host 测完全可控——GUI-07 立场，桌面 v1 确实够用）；berry-signal 信号核（TC39 语义，姊妹综合轴三）——与 GUI-07 的「无运行时信号图」立场冲突，见 §5.1 分歧 2，需用户裁决或 v1.5 原型刀对拍后定。
- **不推荐**：v1 直接上完整声明式 diff（Flutter setState 子树重建 = 七模型最粗失效粒度 + 最高分配压力，GUI-07 §2.8）；即时模式进主线（空闲重绘 + 无稳定标识弱化自动化验证，GUI-04/06/07 三报告共否）；独立标记语言（.slint 式——两套语言/类型系统税 + 现成参照 GPL）；setState 式子树重建；Cassowary 约束布局。
- **反对意见与风险**：①「手动 Invalidate 是代际落后」——GUI-07 自己承认行业终局是编译器 memo/属性级失效，v1 视图级只是过渡（风险 = 若 v1.5 属性级不兑现则「最先进」打折；缓解 = API 数据驱动纪律 day-one，使升级零重构）；②宏 DSL 的 IDE/热重载/增量编译是结构性税（rust-analyzer proc-macro 补全缺口），必须配「域窄 DSL + rustc 诊断闭环」缓解策略（GUI-07 §6.4）；③与姊妹综合的 DSL 载体分歧（HTML 形态外部 DSL vs 宏 DSL）是真分歧，见 §5.1。

### 2.c 合成器/服务摆位（含 berry-guiproto、跨进程 buffer）

**现状**：规划稿 = 三薄服务（guifbd/gui-inputd/gui-wmd）+ 单 GUI 进程，裁决 C 已定「不照搬 Rosen 重 IPC」。

- **推荐**：**维持进程骨架，把三服务的契约从「函数调用面」升级为「协议语义面」**（GUI-05 §0.4/§6.2——本条是五家谱系给出的「最大免费期权」）。guifbd 契约按 B 形态三件事定形成 **berry-guiproto** crate（fsproto 四家先例）：`submit(buffer, dirty_rects)` + `frame_callback(未来时刻表, credits)` + buffer 归还 release 事件；present credits ≤2（在途帧上限显式进契约）；显示驱动契约按 **HDI 三口**（reg_vblank/prepare/commit+fence）——ramfb 是第一个实现、virtio-gpu 是第二个，换实现不换契约（GUI-05 §5.2/§9.7）。跨进程 buffer = **VMO handle 过 channel + rights 收窄（只读给合成器）**，fence = event handle（acquire 恒空/release 显式）——零新内核对象、零新 syscall（GUI-05 §7.2，Magma 拓扑 = 内核只给通用原语的直接蓝图）。gui-wmd 窗口对象留 `parent/group` 字段（QNX window group 缝，成本极低）；hit test 归 wmd/合成侧（「屏上有什么」只存在于合成侧——RFC-0096 防 clickjacking 结构保证）。berry-gfx/berry-ui 做 crate 库而非服务内置（wlroots/Smithay 教训 + host 可离屏测）。
- **备选**：C 形态（Rosen 式双侧树 + 命令流）**后置**——其收益（GPU 集中/服务端动画）只在多客户端竞争单 GPU 时兑现，Flatland 本身就是 C 转 B 的现场（GUI-05 §6.1 谱系结论【高】）；BufferCollection token 协商推迟到「出现第二个生产者或真 GPU 双写」触发（v1 服务定格式即可，sysmem 语义本身是能力模型的届时直接平移）。
- **不推荐**：v1 拆多 GUI 进程；v1 双侧树命令流（OH 自己轻量档都不用——这是多进程多 GPU 时代的形态不是 GUI 的必要形态，GUI-05 §5.4）；gui-wmd 并入 guifbd（backboardd 同址形态——GUI-02 开放问题，建议 GUI-5 前 ADR 裁决，在此之前维持三薄）；任何「GUI 进内核」形态（微 kernel 纪律）。
- **反对意见与风险**：①「单客户端时协议化是过度设计」——GUI-05 反证：协议一次定形避免 GUI-3/8 再改 ABI，且 GUI-1 serve 循环正在写、协议头现在定形成本最低（GUI-06 §7.6.1）；②vsync/帧回调通道独立于 console/通用 RPC 的成本——每客户端一对 event handle 或专用小通道，24B 级载荷（OH socketpair 先例），可接受；③32MiB 帽下多窗 buffer 池核算（1080p×3 层双缓冲 ≈25MB 逼近帽）——v2 多窗刀前需 A1 容量工程联动（GUI-05 §10.4）。

### 2.d 帧调度与 vsync

**现状**：规划稿 GUI-3 = 软 vsync 16ms 定时器驱动 Measure→Render；§5 风险表载「ramfb 无 vsync，撕裂容忍」。

- **推荐**：**软 vsync 不只是妥协，恰是三家真实运行形态**（Android 硬件 vsync 只用于校准、锁定即关，分发走 DispSync 软件 PLL；Fuchsia/OH generator 软生成 + 硬件校准——GUI-05 §3.2/§5.3【高】）。具体形态五件：①**request-driven 分发**（客户端显式 RequestNext 才收拍——空闲 GUI 进程零唤醒，OH 形态；不复用 console/通用 RPC 通道）；②**双 vsync 错相**——给 UI 进程的拍早于给 guifbd 合成的拍一个相位（如周期 1/3），把 2 帧固有延迟压向 1 帧（DispSync 数学在软 vsync 下照搬，GUI-05 §9.4）；③帧回调载荷 = 未来时刻表 + credits（P2）；④软 vsync 周期 = 帧率协商结果的倒数而非 16ms 常量（GUI-06 §7.2）；⑤TCG 现实校准——CNTV 推进率 ≈1/2，周期/预算按 CNTVCT 时基或挂 -icount 通道（CLAUDE.md Gotchas + GUI-08 §1.3）。present 语义按 compositor 模型自有原语（present mode immediate/mailbox/FIFO + presentation-time 反馈——Wayland 十年验证的最小完备集，姊妹综合轴二）。
- **备选**：脉冲式 frame callback（Wayland 最小面）作退路——信息量低一档但实现最简；单进程双线程按相位排（v1 单 GUI 进程内 UI/渲染线程错相，姊妹综合映射表 #2）。
- **不推荐**：自由跑定时器帧循环（R3 反面决策——静止时每秒 60/120 次无意义唤醒，电池杀手 + 把 tickless 在 GUI 层戳洞）；等硬件 vsync（ramfb 没有）；16ms 常量烧死；无 vsync 锚的定时器动画（Android Butter 前史，相位漂移 jank 之源）。
- **反对意见与风险**：①ramfb 无真 vsync → 帧回调的「未来时刻表」无硬件校准、实际呈现时刻不可测——v1 只发预测不回确认（OnFramePresented 缺席），长期帧节奏漂移需实测定案（GUI-05 §10.2）；②TCG 下 16ms 周期实际抖动 + 双错相可行性未实测（spike S4）；③「撕裂容忍不设 kill」与「最先进」措辞冲突——姊妹综合改描为「时序验收走 icount + 帧序列状态谓词，撕裂容忍只是 ramfb 后端实现细节」，本报告同意改描。

### 2.e 输入管线

**现状**：规划稿 GUI-4 = 裁决 A 三层递降（virtio-input → USB 平板 → 串口鼠标协议）+ PointerEvent 多点位留位；事件重采样标「模拟器鼠标输入可后补」。

- **推荐**：**服务化管线按 RFC-0096 骨架 + GUI-08 全套常数**：采集（virtio-input）→ 归一（InputReport → PointerEvent）→ 按 wmd 窗口表命中分发 → UI 进程内与帧回调**同拍**处理（Choreographer 形态，一次 doFrame 内输入→动画→布局→渲染）。**PointerEvent 从第一版带采样时间戳序列 + 稳定 pointerId**（coalesced 语义——历史样本不在采集侧折叠）；重采样放消费侧最终一跳（gui-wmd → 应用），常量集中定义 + host 纯函数测（内插优先/外推上限 min(Δ/2,8ms)/5ms 安全垫按 Android 定值起步）；预测与真值分离（预测点只用于渲染跟手不进手势状态机）。intent/调度联动：焦点 GUI 进程 intent_declared = INTERACTIVE（0x01，ADR #42 冻结表内）；touch boost 走 PM 服务频率脉冲（交互窗事件 → PM 短窗抬频、touch up 后数百 ms 衰减 = POWER_HINT_INTERACTION 等价）；**GUI 事件流只进 AI 观测空间不进内核动作空间**（GUI-08 §3.3）；gui-inputd/guifbd 常驻等待一律 wait_many 阻塞形态 + 私有 handle 表（CLAUDE.md 4-5/4-6 雷册直接适用）。
- **备选**：重采样 v1 后补但结构留位（规划稿已如此标注，GUI-08 确认可行）；One Euro Filter 作平滑标准件叠加（GUI-08 §2.4）。
- **不推荐**：重采样放驱动/采集侧（把延迟塞给所有消费者且无法按窗口开关——Flutter #73286 教训）；输入通道与 UI 串行争锁；轮询式采集（touch boost 用事件真值不用轮询，GUI-06 §7.5）。
- **反对意见与风险**：①virtio-input 在 AAVMF+virtio-mmio 下固件枚举路径未实测（spike S3，裁决 A 兜底层保验收不塌）；②预测渲染在 60Hz 软 vsync + TCG 软渲染下可能放大错位（帧时间本身 >16ms 时）——建议 GUI-6 动画刀后用 SDK-3 指标 A/B 再定默认开关（Chromium 默认关的教训，GUI-08 §9.3）；③TCG 时基分歧——功能断言用事件序（确定性），数值报数挂 -icount 通道（GUI-08 §9.2）。

### 2.f 低功耗 day-one 面

**现状**：规划稿附录 B 载「v1 明确砍掉变帧率」；HGM 评「低——QEMU 单帧率环境无消费面」。

- **推荐**：**「砍实现不砍协议位」（GUI-06 §7 总原则，本区核心裁决）**。六件 day-one：①flush 协议恒带 damage 矩形集（K≤8）+ FULL 位 + 合并启发式定值进协议注释（面积 ≤ Σ×(1+1/4) 才并）；②三态帧循环 RUN/SETTLE/IDLE——IDLE 判据 = 脏区空 ∧ 动画表空 ∧ 无未决输入，IDLE 撤软 vsync 定时器睡在事件上，唤醒首帧立即渲（不欠拍）；③vote{min_hz, max_hz, preferred_hz, scene, sticky} 票面进 berry-guiproto（区间票面 = iOS/OH 双实证），仲裁规则 v0 写死服务端（最高票/倍数取高/touch boost/静态 2000ms 降 IDLE——OH 常量），QEMU 三扇出全 no-op + 遥测；④power/mode op（NORMAL/DIM/AOD/OFF）——协议只承诺软件纪律不承诺硬件行为（ramfb 上 AOD 档 = 帧循环按 1Hz 醒 + 秒级 damage）；⑤PM 联动——每帧向 PM 共享环写 {target_ns, actual_ns}（ADPF createHintSession 同构，映射表全在册）+ 帧态翻 intent/EPP（RUN→performance、IDLE→efficiency、AOD→maintenance 0x05——GUI 第一个真实消费者，ADR #42「机制预留」的兑现样本），GUI 只投票不设频；⑥能耗代理验收——IDLE 态注入零事件时帧循环唤醒计数恒 0 + icount 指令增量 ≈0，写进 GUI-9 DFX/smoke（QEMU 无能耗面下的确定性「静止即零功耗」断言）。**R1–R10 反面清单入评审**（GUI-06 §6.4：烧死 RGB32/整屏 flush/自由跑帧循环/假设 pageflip 在位/每帧全屏 damage/常驻大缓存/单值帧率/无 mode op/亚秒刷新假设/通用 IPC 传帧——越界即打回，尤其 GUI-1/3/4 三把定协议的刀）。
- **备选**：像素格式/几何运行时协商进协议版本域（fbcon `from_raw` 宁缺勿猜纪律上移——R1 留缝即可，RGB565/1bpp 留枚举位不实现）。
- **不推荐**：降帧跑完整管线（停帧 + 保留 > 稀疏重绘 > 连续低帧率重绘——Wear AOD 冻结帧 vs 管线照跑的教训）；协议烧死任何「假设」类决策（手表移植性死刑——会杀死移植的只有假设不是功能缺位，GUI-06 §6.4 反向检验）；AI 直接设频率/选帧率档（违 ADR #42 带内干涉纪律）。
- **反对意见与风险**：①「QEMU 验不了硬件收益」——如实承认：只能验软件纪律（协议、三态、唤醒计数），硬件收益（PSR/LTPO/OPR）留真机线；②AOD 与深睡交互（冻结/预扣/latency QoS 挂 v3）——1Hz AOD 的唤醒源在真面板场景需低功耗定时器面，v1 不做只留 mode 位；③vote 的安全/公平面（多 GUI 进程互投高帧率 DoS）——单 GUI 进程阶段不可达，多窗刀前需裁。

### 2.g 开发套件

**现状**：规划稿 GUI-9 = DFX 三件（FPS 统计 + 视图树 dump + 事件注入器）；SDK 无独立线。

- **推荐**：**SDK-1..5 演进序锁定为独立裁决（GUI-08 §6 表）**：SDK-0（已有：CLI-first 全循环 + smoke 机器断言 + FIFO 注入）→ **SDK-1**（随 GUI-8/9 收口：视图树 JSON dump 带稳定节点 id + 事件注入器（dump→注入同源）+ FPS/帧双轨统计 + guifbd 截图服务 + 宿主像素对拍 + berryctl gui 子命令 + .bap 脚手架）→ **SDK-2**（声明式 UI 层：DSL + 设备侧解释器 + FAT 热区热重载——「宿主改文件 → guest 即刻可见 → UI 重求值」最后一跳；状态保持 + 结构变更自动重启由 appd 监督承接）→ **SDK-3**（帧时间线 trace 导出 + input_ts→frame 关联（EventLatency 同构）+ 交互延迟 p50/p90 机器可读输出）→ **SDK-4**（AI 生成闭环：组件语义库 + 生成产物校验器（schema 校验 + SDK-1 自动化验证 + 像素对拍 + agent 修复环）+ MCP 式 agent 口（dump/inject/screenshot/reload 四 verb，Slint MCP 业界首例 + JetBrains MCP 产品化双先例））→ **SDK-5**（设计器 + Rust 代码级热重载，等 v2 动态加载）。关键设计纪律：**dump 格式 day-one 设计成可 round-trip 注入**（稳定 id + 语义角色），一份数据喂调试器/自动化测试/AI agent 三消费者；`.bap` manifest 扩 `views/` 声明式资源位（AI 生成产物 = 数据文件非 Rust 代码，生成→验证循环不落编译，分钟级→秒级）；UI 框架保留「描述层可序列化」缝（builder/描述结构双入口——热重载、AI 生成、设计器三条终态线的共同依赖）。
- **备选**：DSL 形态 JSON 子集 vs 自定义文本语法——SDK-1 落地后用真实使用数据裁，或双面（JSON 为规范形、文本语法为糖）（GUI-08 §9.5）。
- **不推荐**：v1 Rust 代码级热重载（JIT/JVMTI 类魔法对 AOT 不可行，等 v2 动态加载自然解锁——Slint 形态已覆盖界面迭代主场景 90%）；自由代码生成无验证环（「完美 JSON ≠ 语义正确」，生成已商品化、确定性验证环才是差异化——BerryOS「AI 测试友好化」支柱与 AI 开发套件是同一根柱）。
- **反对意见与风险**：①SDK 线与 GUI 线工时冲突——SDK-1 挂 GUI-8/9 尾、SDK-2 起独立刀，与 GUI 线文件面基本互斥可并行；②DSL 载体与姊妹综合分歧（HTML 形态外部 DSL）见 §5.1；③热重载状态保持的语义边界（哪些状态保留/哪些重启）需 SDK-2 ADR 定。

---

## 3. 现行规划逐刀对表（GUI-1..9 + 裁决 A–F：改/留/增）

> **对表框架**（核心张力的执法口径）：逐刀先问「这把刀定的面里，哪些是**架构面**（协议语义/API 形态/服务边界/升级缝——按终态设计），哪些是**实现面**（按可达路径渐进）」。架构面凡与 §1 共识原则冲突或缺失终态语义的 = **改/增**；实现面按 QEMU 现实渐进的 = **留**。结论先行：**九把刀骨架全部保留，无一推翻；四把改描（GUI-1/2/3/6）、五处增强（GUI-4/5/7/8/9）、两把增刀（GUI-0 前置 + SDK-1 并轨）、裁决 A–F 全留（D 改措辞、F 增字段）、新增裁决候选 G/H/I。**

| 刀/裁决 | 现行内容 | 裁决建议 | 依据 | 架构面/实现面检验 |
|---|---|---|---|---|
| **GUI-1** 显示链收口 | fb MMIO 窗 VMO 授能 + fbcon 接 guifbd 雏形；FBW 自验 | **留方向，改协议面**：serve 循环起步协议就按 B 形态三件事定形——flush 恒带 damage 矩形集（console 路恒 FULL）+ release 事件 + 帧回调未来时刻语义预留；协议头即 berry-guiproto 雏形（GUI-05 §9.1「无损叠加」+ GUI-06 §7.6.1「协议一次定形避免 GUI-3 再改 ABI」）。显示驱动契约按 HDI 三口（ramfb = RegVBlank(软)/Prepare(恒 layer)/Commit(fence 空)） | GUI-05/06 | **改**：现行「console fan-out 函数调用面」是实现面习惯；damage/credits/未来时刻表是架构面，后补 = ABI 破裂。协议窗口正开着（guifbd 在途，单缓冲直写无 flip 无 damage） |
| **GUI-2** berry-gfx | 六原语 trait 化 + 软实现 + 图标格式 + LRU + 字体档 | **留 crate 化，改 API 形态与定位**：①API 按 pass 形态 WebGPU 语义子集定形（loadOp/storeOp/命令录制/Caps 位 day-one 在场——CPU 实现可忽略其优化但语义必须在场，否则换硬件后端 = 破坏性 API 改版）；②软实现定位 = 永久 CI 基线非过渡；③光栅核升级：自研 rect/glyph 快速路径 + vello_cpu 候选（依赖审计 spike 过门才定，S2）；④双后端像素共识测试挂机器守卫（mkfat --check-fixture 先例）；⑤文本栈拆刀（skrifa 灰度先行，CJK 立项——移动端硬需求） | GUI-03/04 + 姊妹综合轴一 | **改**：「六原语 trait」是实现面；pass 语义/Caps/像素共识是架构面。六原语降级为 boot/诊断层 + 快速路径 |
| **GUI-3** berry-ui | View/ViewGroup/Invalidate 脏区 + Measure/Render 两遍 + 软 vsync 帧循环 + FPS 统计 | **留骨架（裁决正确），改四处 + 增两处**：改①帧循环 = 三态停帧（RUN/SETTLE/IDLE，IDLE 撤定时器）且为**验收项**（host 测：脏区合并代数 + IDLE 撤定时器状态机）；改②FPS 统计升 expected/actual 双轨打点（BerryOS 版 FrameTimeline 骨架）；改③API 数据驱动纪律（struct 属性/enum 事件/Msg-only 回调）；改④软 vsync 周期 = 协商结果倒数 + request-driven + UI/合成错相。增①输入与帧同拍 doFrame 形态；增②动画属性 Transform/Layout 分型 | GUI-06 §7.6.2、GUI-08 §8.6、GUI-07 §7.2、GUI-05 §9.4/§9.5 | **改**：三态停帧/双轨打点/数据驱动 API 全是架构面（后补 = 全局改写）；「16ms 定时器」是实现面可渐进 |
| **GUI-4** gui-inputd | 裁决 A 落地 + PointerEvent 多点位留位 | **留，改两处**：①PointerEvent day-one 带采样时间戳序列 + 稳定 pointerId（多点位留位保留）；②重采样结构留位（常量集中定义 + host 纯函数测，Android 定值起步）。增：intent INTERACTIVE + PM 交互脉冲接线（GUI 第一个 AI 调度真实消费者）。前置：virtio-input 可用性 spike（S3）提前到本刀前 | GUI-08 §8.1/§8.3/§8.4 | **改**：时间戳序列/pointerId 是事件结构 ABI（架构面）；virtio-input 驱动是实现面 |
| **GUI-5** gui-wmd | 两层窗 + Z 序表 + 避让 + WindowInfo 推送 | **留，增三处**：①WindowInfo 带 parent/group 字段（QNX 缝）；②hit test 归 wmd 侧明确立法（防 clickjacking——「屏上有什么」的知识只存在于合成侧）；③窗口表→inputd 推送契约按 OH WindowInfo 定形进 berry-guiproto。前置 ADR：gui-wmd 是否并入 guifbd（backboardd 同址问题，GUI-02 开放）——建议本刀前裁 | GUI-05 §6.2.3/§9.11 | **增**：parent/group 是协议字段（架构面，成本极低）；两层窗是实现面 |
| **GUI-6** 交互三件 | 手势 referee + 定点缓动 + GridLayout + SwipeView | **留三件，改布局子项 + 增两处**：改：布局 = Taffy（MIT）移植优先（flexbox 语料/AI 友好 + grid 是特例；过 crate 预检 f32 ULP 容忍策略），自研线性+网格为备选（v1 桌面确实够用——两案并列请用户裁，见 §5.1 分歧 3）；增①动画属性分型落地（合成器可计算属性服务端执行缝）；增②手势 referee 输入短路径（事件→目标视图标脏不经全局调度绕行——输入→首像素变化短路径） | GUI-07 §4、姊妹综合轴三、GUI-05 附录 B | **改**：布局算法选择影响 berry-ui API 面（架构面）；缓动曲线形态（定点 vs 浮点）是实现面但受 GUI-0 制约 |
| **GUI-7** appd 扩面 | manifest icon/label + mkapp 图标打包 + 增删事件 | **留，增一处**：manifest 同刀扩 `views/` 声明式资源位（add-only，SDK-2 前置——AI 生成 UI 产物 = 数据文件的挂点）；图标格式枚举位留 RGB565/1bpp（R1 缝） | GUI-08 §8.9、GUI-06 R1 | **增**：views/ 是 .bap manifest ABI（架构面）；图标解码是实现面 |
| **GUI-8** berry-launcher | 壁纸 + 页×网格 + 状态栏 + 点击 spawn + FAT 布局持久化 | **留全部（主验收不变），增一处**：布局 JSON 从第一版带 schema 版本域 + 结构化设计（它 = AI 生成面的第一个 schema 消费者；「AI 可读」从 launcher 数据面开始兑现） | GUI-08 §5.3 | **增**：持久化 schema 是数据 ABI（架构面）；壁纸/网格是实现面 |
| **GUI-9** DFX 三件 + smoke 相 | 视图树 dump + 事件注入器 + GUI smoke 段 | **留三件，改两处 + 增两处**：改①dump 格式 day-one round-trip 可注入（稳定 id + 语义角色，dump 的 id 即注入器寻址键）；改②FPS 统计消费双轨数据（expected/actual）。增①能耗代理断言（IDLE 唤醒计数 0 + icount 增量 ≈0——「静止即零功耗」模拟器版）；增②注入口开串口 + TCP 双通道（vnet TCP 先例）为 SDK-4 MCP 式 agent 口（dump/inject/screenshot/reload 四 verb）铺路 | GUI-08 §8.5/§8.6、GUI-06 §7.6.4 | **改**：dump 格式是三消费者（调试/测试/AI）共用的数据 ABI（架构面）；注入通道形态是实现面 |
| **GUI-0**（**新增**） | — | **增刀：FPU/FPSIMD 上下文保存**（ADR #34 trap+lazy-save）——现状 TaskContext 只存整数、CPACR FPEN=0b11，任何任务用 SIMD 即跨任务静默串味（CLAUDE.md Gotcha 明载）；一切浮点渲染数学/SIMD 光栅/物理动画的**硬前置**，不是优化项。可并入 GUI-2 前段 | 姊妹综合轴一 + CLAUDE.md | 架构面（内核上下文 ABI） |
| **SDK-1**（**新增并轨**） | — | **增：SDK-1 挂 GUI-8/9 收口刀**（dump/注入/截图/像素对拍/berryctl gui/脚手架）——DFX 三件与 SDK-1 是同一批交付物两面（GUI 面验收 + 开发套件面），拆开 = 同一格式定两遍 | GUI-08 §6 | 架构面（dump schema 三消费者 ABI） |
| 裁决 A 输入源 | virtio-input → USB 平板 → 串口鼠标 | **留**（三层递降已含兜底）；spike S3 前置 | 规划 §3-A | — |
| 裁决 B 图标格式 | 自定义紧凑格式 | **留**；格式枚举位留 RGB565/1bpp（R1） | GUI-06 | — |
| 裁决 C UI 框架形态 | 单 GUI 进程内嵌 UI 库 + 三薄服务 | **留**——本报告确认「lite 实现 + 完整架构缝」与 B 形态终态兼容（GUI-05 §6.2 谱系结论）；唯一改描：三服务契约从函数调用面升协议语义面（同 GUI-1 行） | GUI-05 | 架构面兼容是本表最大结论 |
| 裁决 D 像素格式 | RGB32 800x600 起步，TCG 不足降 RGB565/640×480 | **留，改措辞**：「运行时协商进显示协议版本域」（fbcon `from_raw` 纪律上移）——同一决策但按终态协议面定形，降载预案照留 | GUI-06 R1/§5.1 | **改**：像素格式烧进常量 = 移植性死刑 |
| 裁决 E 失败隔离 | appd 监督重启 | **留**；补注：ramfb 恒扫 DRAM → guifbd 重启后面板内容来源天然成立（监督重启即可恢复画面） | GUI-05 §10.2 推断 | — |
| 裁决 F icon/label | manifest 扩 icon/label | **留**；增 views/ 同刀（见 GUI-7 行） | GUI-08 | — |
| **裁决 G**（**新增候选**） | — | 低功耗缝 day-one：damage/停帧/vote/mode op 留位（§2.f 六件）——GUI-06 §7.6.5 建议回写规划 §3 裁决表；附录 B「v1 砍变帧率」补注「砍实现不砍协议位」 | GUI-06 | 架构面 |
| **裁决 H**（**新增候选**） | — | SDK 演进序锁定（SDK-1..5，§2.g）——GUI-08 §6 表整表入册 | GUI-08 | 架构面（schema/通道 ABI 序） |
| **裁决 I**（**新增候选**） | — | berry-guiproto 协议 crate 提前到 GUI-1/4（B 形态三件事 + vote + mode op + HDI 三口，host 测编解码无消费者不接线） | GUI-05 §9.1、GUI-06 §7.6.3/4 | 架构面 |

---

## 4. 风险、矛盾证据与开放问题

### 4.1 两份综合报告的分歧点（GUI-09 vs `docs/GUI渲染调研-2026-10.md`，需用户裁决）

> 两份综合出自不同 workflow、输入面不同（本报告 = 八维专项 GUI-01..08；姊妹 = 六维 + 对抗校验 22 条）。主体架构互证（B 形态协议、damage day-one、停帧、软 vsync、VMO 授能、Magma 拓扑、FPU 前置刀全部一致），以下三处真分歧如实并列：

1. **DSL 载体**：姊妹综合轴三推荐「HTML 形态外部 DSL + 解释器热换（v2 主交付）」并否决「proc-macro 唯一路线」；GUI-07 推荐「窄域 Rust 宏 DSL（v1.5 叠加、终态主形态）+ JSON schema v2 动态缝」并指出宏的 IDE/热重载税与外部语言的两套类型系统税各有代价；GUI-08 立场居中（Slint 形态 = 文本 DSL + 解释器，开放问题 5 提「JSON 规范形 + 文本语法糖」双面）。**三案并列**：A 外部 HTML 形态 DSL（热重载/AI 语料最优，付自建语言栈税）/ B 宏 DSL（rustc 验证最优，付 IDE 税）/ C 双面（JSON 规范形 + 文本糖，风险 = 两套并存缝合怪）。本报告倾向 = **v1 末拍板**（姊妹综合决策点 5 同判），拍板依据 = SDK-1 落地后的真实使用数据；无论选哪个，**「UI 即数据 + 组件注册表单一」是三条路线共同前提，day-one 无悔**。
2. **状态面形态**：姊妹综合轴三定「berry-signal 信号核（TC39 语义、故意不含 effect）」；GUI-07 明确「不引入运行时信号图——规避 Leptos 借用/订阅图冲突」，主张 Elm Model/Msg/update + 键路径 diff 到属性级。两者目标一致（属性级细粒度失效），机制相反（运行时依赖图 vs 编译期/帧循环推导）。**并列待裁**：GUI-3 API 数据驱动纪律对两者都兼容（struct/enum/Msg-only 是信号核与键路径 diff 共同的超集），裁决可推迟到 v1.5 原型刀对拍（键路径 diff vs 轻量信号，借用检查友好度 + TCG 性能，GUI-07 开放 2）。
3. **布局引擎**：姊妹综合定「Taffy 移植、GridLayout 退 flex-wrap 特例」；GUI-07 §4 说「lite 档线性+网格两算法即可表达桌面（规划在册）」。两案并列请用户裁（§3 GUI-6 行）：Taffy = 语料/AI 友好 + 免自研 + 表达力上限高，付 f32/依赖预检税；自研两算法 = 零依赖 + host 测完全可控 + 更小，付表达力上限 + 自研工时。

### 4.2 报告间矛盾/张力证据（如实并列）

1. **ProMotion 批评 vs 高刷基准**：GUI-06 引 Andrew Zuo「iPhone Pro 大部分时间不在 120Hz、用户感知与 60Hz 差异有限」——既是批评也是佐证（系统确实在静态时深度降频，正是省电设计起作用的形态）。设计含义：帧率协商按区间 + 系统仲裁（P5），不承诺「恒最高刷」。
2. **vello_cpu 可用 vs llvmpipe/lavapipe 不可移植**：不矛盾——vello_cpu 是纯 Rust no_std 无 LLVM 依赖（GUI-04），llvmpipe/lavapipe 是 LLVM JIT 栈（GUI-03）；但 vello_cpu 在 berry-libc/musl 子集 + TCG SIMD 的实际成本未实测，spike S2 是唯一裁决途径。姊妹综合已把「vello_cpu 默认后端」降级为「spike 过门才定」。
3. **vsync 专用通道 vs channel 有界队列已有背压**：GUI-05 一面说 BerryOS channel 天然背压、一面建议 vsync 递送独立于通用 IPC（OH socketpair 先例）——取舍点不在背压在**唤醒语义**（request-driven + 最新值 + 丢旧保新），通用 RPC 通道表达不了。结论：专用 event 对/小通道，不复用 console。
4. **「Compose 用 5 年追平 View」vs「自研 berry-ui 能达标」**：GUI-01 的新框架性能债是真实警示；但 berry-ui 的保留树 = 「全 skip 形态」（无 diff 可跳过的问题在 Rust 值语义 + 键路径 diff 下天然缓解），且帧成本走 icount 定标（GUI-07 §4）——风险在文本/光栅核不在失效模型，已被 GUI-2 光栅核裁决覆盖。
5. **「撕裂容忍不设 kill」vs「最先进不将就」**：规划 §5 原文 vs 用户裁决①——姊妹综合改描「时序验收走 icount + 帧序列状态谓词，撕裂容忍只是 ramfb 后端实现细节」；本报告同意，ramfb 撕裂是实现面限制非架构面妥协。
6. **GUI-02 提议 gui-wmd/guifbd 合并（backboardd 同址）vs GUI-05 维持三薄**：真开放问题——建议 GUI-5 前 ADR 定（合并省一次窗口表同步、分开保失败隔离粒度），本报告不预设立场。

### 4.3 低成本 spike 清单（按优先级，全部 <1 天量级）

- **S1 virtio-gpu 2D 可用性**（最高优先）：QEMU aarch64 virt + AAVMF 挂 `-device virtio-gpu-device`（virtio-mmio），验证 scanout/flush/dirty-rect/display-event 语义 + ramfb 并存固件行为。它是「HDI 三口 + overlay 语义」从纸面变实测的前提（GUI-05 §10.1、GUI-03 §5）。估 30–60 分钟。
- **S2 vello_cpu 依赖审计**：aarch64/berry-libc（musl 子集）交叉编译 + 小形状 UI 负载（圆角矩形/字形密集）icount 对拍——过门才定默认后端（GUI-04、姊妹综合轴一）。
- **S3 virtio-input 设备类可用性**：GUI-4 前置（裁决 A 第一层；固件枚举路径未验证）。
- **S4 软 vsync 双错相 TCG 实测**：16ms 周期抖动（CNTV 1/2 效应）+ UI/合成错相两拍可行性（GUI-05 §10.3）。
- **S5 全重画离屏 + damage blit vs 只渲脏区**：icount 对拍，GUI-2/3 刀内定夺（GUI-06 开放 4）。
- **S6 TCG 帧预算定标**：800×600 RGB32 全屏 fill/blit 的 icount 数——裁决 D 降载判据 + GUI-3 帧预算断言上限的数据面。

### 4.4 未决问题（超出本报告裁决权，留用户/后续刀）

1. fence 语义最小契约（v2 ADR：共享内存单调 seq + Event 唤醒 + 超时回退；生产者假死等多久、谁有权宣告缓冲可用——姊妹综合轴二修正）。
2. acquire fence 字段占位 vs YAGNI——建议 FIDL 式 table 字段号留位（add-only）不实现（GUI-05 §10.5）。
3. 多 GUI 进程拆分触发判据 + 32MiB 帽下窗口 buffer 池核算（1080p×3 层双缓冲 ≈25MB 逼近帽——届时帽值或分层池需 A1 联动，GUI-05 §10.4）。
4. JSON 动态 UI 与能力模型边界：AI 运行时生成/递送的 UI 是否受 .bap 能力声明约束（纯能力模型下需新裁决，GUI-07 开放 4）。
5. wgpu 底座评估时点（v2+ 架缝——寄宿成本专项 spike：std 面/T-1 线程面/ash 加载/surface 钩子）。
6. CJK 文本闭环（harfrust 只管 shaping，断行/竖排/回退链无成熟 Rust 方案 + 字库 5–20MB 对 48MiB 盘的账——「先算账再选型」强制适用，姊妹综合补漏 2/8）。
7. gui-wmd/guifbd 合并 ADR（GUI-5 前）。
8. 色彩管理/色深留位（sRGB/P3/HDR 概念位——零成本：v1 格式枚举 + surface color space 元数据位，不留 = v3 面板接入又一次破坏性改版，姊妹综合补漏 1）。
9. intent 档位边界：渲染会话复用 0x01/0x02 + EPP 组合（推荐）vs 开新档——ADR #42 冻结段扩表裁决（姊妹综合决策点 7）。

---

## 5. 关键发现

1. **八维收敛出 12 条普适原则（§1 P1–P12）**，其中最强收敛（≥4 维度独立佐证）：变更描述而非像素（P1）、帧时钟权威 + 未来时倒排（P2）、damage 协议字段（P3）、需求驱动停帧（P4）、缓冲深度 = 延迟（P6）、度量 day-one（P8）——任何一条在 BerryOS 落成协议位都几乎零成本，后补全部是 ABI 破裂级代价。
2. **现行规划的进程骨架不需要推翻**：三薄服务 + 单 GUI 进程恰好是五家谱系收敛的 B 形态（buffer 过界）在单客户端的退化形；需要升级的只是契约语义面（GUI-05 §6.2「整个谱系给出的最大免费期权」）。
3. **软 vsync/软渲染/单进程不是将就而是行业真实运行形态**：Android 分发本来就是软件的（DispSync）、OH 软生成 + 硬件校准、渲染 RASTER 纯软回退是一等路线；「最先进」的兑现 = 协议面超前于实现面（本报告核心结论）。
4. **BerryOS 有两块结构性红利可直接兑现「不将就」**：①Rust 无 GC + 值语义保留树 = 「全 skip 形态」（Android 用 Strong Skipping 2024 才追平的东西天然在）；②单 GUI 进程直持 fb 比自绘框架 + 独立渲染服务少两级缓冲（GUI-08 输入→呈现 3–5 帧的级数直接砍）。
5. **「兼容≠适配好」有完整执法方案**（GUI-03 六案库 + 三条执法）：语义形态亲和（概念/命名对齐 WebGPU）≠ 生态互操作（v3 wgpu-hal 才是真兼容）；兼容层永远在翻译层不在原生路径内部。
6. **低功耗与「优秀」不是对立而是同一根杠杆**：有变化全速、无变化归零（GUI-06 三家独立收敛）；「砍实现不砍协议位」让 QEMU 阶段就能把缝全部留对（vote/mode/damage/停帧），能耗代理断言（IDLE 唤醒 0 + icount ≈0）是模拟器时代唯一确定的省电验收。
7. **GUI 是 AI 原生调度/AI 测试友好化两大支柱的第一个真实兑现面**：intent INTERACTIVE + PM {target,actual} 共享环（ADPF 同构、映射表全在册）+ 遥测三键（input_ts/frame_begin/frame_end/flush_done）同时喂 smoke 断言、性能工具、AI 观测——两根支柱在 GUI 面是同一份数据。
8. **SDK 的差异化在验证不在生成**：v0/Builder 证明生成已商品化；Slint MCP/JetBrains MCP 证明 agent 驱动运行中 GUI 已产品化；BerryOS 的确定性验证环（dump round-trip 注入 + 像素对拍 + 同输入同输出）恰是「AI 测试友好化」支柱的 GUI 展示位。
9. **两份综合报告三处分歧（DSL 载体/状态面/布局引擎）全部可推迟裁决且存在无悔公共前提**（UI 即数据 + 组件注册表单一 + API 数据驱动纪律）——不存在「必须现在拍否则返工」的分叉。
10. **唯一硬门是 GUI-0 FPU/FPSIMD 上下文**：现状任何任务用 SIMD 即跨任务静默串味（CLAUDE.md Gotcha 在册）；它是浮点缓动/NEON 光栅/物理动画的共同前置，应插在 GUI-2 前。

## 6. 对 BerryOS 的架构启示

1. **把 GUI-1 的 serve 循环协议头按 B 形态三件事一次定形**（submit 带 damage 矩形集 K≤8 + FULL 位 / frame_callback 未来时刻表 + credits / release 事件）——协议窗口正开着（guifbd 在途、单缓冲直写），此刻定形成本最低、收益覆盖 GUI-3/8 与 virtio-gpu 后端（GUI-05 §9.1、GUI-06 §7.6.1）。
2. **berry-gfx API 按 pass 形态 WebGPU 语义子集设计，软实现是第一实现而非回退**；soft 后端永久进 smoke 主相；双后端像素共识测试挂机器守卫（防 mkfat fixture 型基准漂移）。
3. **berry-ui API 第一天数据驱动**（struct 属性/enum 事件/Msg-only 回调/描述结构双入口）——这是声明式宏层、热重载、AI 生成、设计器四条终态线的共同前提，零代码量纯纪律。
4. **帧循环三态 + IDLE 撤定时器立为验收项**（host 状态机测试 + smoke 能耗代理断言）——tickless 铁律在 GUI 层的延伸，违者 = R3 反面决策。
5. **帧回调、vsync、PM 反馈三类小信号全部 request-driven + 专用通道**（event 对/共享环），永不复用 console/通用 RPC——OH socketpair、PM 共享环、IPC 设计 §7 三先例同向。
6. **输入事件结构 day-one 带采样时间戳序列 + 稳定 pointerId + 全链 CNTVCT 透传**——统一时基是 smoke 断言、INP 型指标、AI 遥测的共同前提；重采样常量集中 + host 纯函数测（berry-complete 先例）。
7. **hit test 与「屏上有什么」的知识只归 wmd/合成侧**（RFC-0096 结构保证）；窗口对象留 parent/group 缝；显示驱动契约按 HDI 三口——三个「成本极低、后补极贵」的协议位。
8. **GUI 服务进程的调度纪律照雷册全量执法**：wait_many 阻塞形态（永不自旋）、私有 handle 表、IRQ 去活电平在 EOIR 前、wait 超时是心跳非死线——console/vblk 全部前车之鉴在 GUI 面重演的代价是整机 GUI 死。
9. **验收指标全面转确定性代理**：功能断言用事件序、时序/成本走 icount（B-2 通道）、功耗走唤醒计数——「smoke 标记不得建立在竞速上」纪律在 GUI 面的三个落点。
10. **许可门禁 ADR 化**（crates.io + LICENSE 双源核对；GPL/Slint 自动拒；MPL 只借语义；引入任何渲染/UI 依赖先过协议洁净 + 体积/依赖审计双门）——vello_cpu 是目前唯一双门全过的重型候选，仍需 spike 实证。

## 7. 开放问题

1. DSL 载体终裁（外部 HTML 形态 vs 宏 DSL vs 双面）——v1 末拍板，依据 SDK-1 真实使用数据（§4.1 分歧 1，姊妹综合决策点 5 同判）。
2. 状态面机制（信号核 vs 键路径 diff）——v1.5 原型刀对拍（借用检查友好度 + TCG 性能）。
3. 布局引擎（Taffy vs 自研两算法）——需用户裁决或 GUI-6 刀内预检定夺。
4. gui-wmd 是否并入 guifbd（backboardd 同址）——GUI-5 前 ADR。
5. fence 最小契约与 acquire 字段占位——v2 ADR（占位建议 add-only table 字段号）。
6. 多 GUI 进程触发判据 + 32MiB 帽下 buffer 池核算——v2 多窗刀前与 A1 容量工程联动。
7. JSON 动态 UI 与 .bap 能力模型的边界——纯能力模型下需新裁决。
8. wgpu 底座寄宿成本——v3 前专项 spike（真生态互操作唯一路径）。
9. CJK 文本栈闭环（shaping 之后的断行/竖排/回退链 + 字库体积账）——移动端硬需求，单独立项。
10. ramfb 恒扫 DRAM 的 QEMU 源码级取证（影响 AOD 档验证形态：只能验软件纪律不能验硬件收益——当前按推断标注）。
11. 软 vsync 双错相的 TCG 实测（S4）与帧节奏长期漂移的定案（无硬件校准时 OnFramePresented 缺席的补偿策略）。
12. 色彩管理/色深概念位与 IME/preedit 协议缝——零成本留位项，随 GUI-2/文本组件 API 落纸（姊妹综合补漏 1/4）。

---

## 参考与来源计数

- **输入报告（8）**：`research/GUI-01-安卓渲染响应慢教训.md` … `research/GUI-08-输入管线与开发套件.md`（内部合计 100+ 联网来源 + OH/Fuchsia/QEMU/Android 官方文档与源码取证，URL 见各报告清单）。
- **对表文档（3）**：`docs/GUI里程碑规划-2026-10.md`（现行规划）、`docs/GUI渲染调研-2026-10.md`（姊妹综合）、CLAUDE.md（Gotchas 雷册/调度纪律/VMO 帽）。
- **本地源码取证（经 GUI-05/06 转引）**：`user/bins/guifbd/src/main.rs`（GUI-1 在途）、`user/berry-fbcon/src/render.rs`、OH 浅克隆 `~/Documents/harmony/openharmony/`（vsync_distributor/hdi_device/hgm_voter 等）。
- 独立信息源总数远超 8 个门槛；关键论断确定度沿用各报告【高/中/低】标注，二次综合未降级任何一手证据。
