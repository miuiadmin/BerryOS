# PM-09 AI 用在电源上的实绩、学术与坑（v1.0）

> 调研日期：2026-11-07 · 方法：联网取证（WebSearch/WebFetch，中英文并用）+ 仓内精读（PM-00/01/02/06、`docs/AI调度设计-v1.0.md`、`pm-notes/00`）。
> 确定度图例：**高** = 官方文档/一手论文/官方博客；**中** = 二手转述/社区实测/综述口径；**低** = 推断或单一来源转述。
> 与一期分工：PM-06 §4.1 已给现役「AI 预测省电」清单概览与口径校准，PM-01 §1.4 已给 9.0 桶位 ML 时代线与 30% 转述出处，
> PM-02 已给 iOS 后台治理哲学——本篇**只引不重写**。本篇增量 = ①工业内幕深挖（Adaptive Battery/预测性充电/iOS 时机学习/国产 OEM）
> ②学术全景带批判 ③坑清单 ④带内票面映射 ⑤电源域 kill criteria 候选 ⑥「人无我有」claim 清单——全部对齐
> `docs/AI调度设计-v1.0.md` 既有框架（实验轨道/动作空间封顶/护栏），不另起炉灶。

---

## §0 一页结论（土话直答）

1. **工业八年，真正上量的 ML 只有一件：Adaptive Battery。** 口径校准：DeepMind 官宣原文只有
   "significant reduction in background activity"，不贴数字（[DeepMind 博客](https://deepmind.google/blog/deepmind-meet-android)【高】）；
   「CPU 唤醒最多减 30%」是 I/O 演讲转述（[Android Authority](https://www.androidauthority.com/adaptive-battery-google-android-863032)【中】，
   PM-01 §1.4 已载）；同博客里的 40% 是**数据中心制冷**不是手机——三组数字三个来源，引用时别串。
2. **第二梯队全是「时机学习」，不是「行为控制」。** 充电时机（Pixel/Apple）、刷新时机（iOS 后台刷新）、
   网络档时机（Pixel 自适应连接）——学的都是「你什么时候起床/开 app/要带宽」，**出口永远是既有机制**
   （充电控制器/任务调度器/网络切换），从不直接控频控帧率。
3. **学术 25–42% 的学习型 DVFS 数字几乎全部在模拟器/受控负载上取得**，无一进生产热路径——
   方差、分布漂移、不可回归测试是三座墙；这与 `docs/AI调度设计-v1.0.md:11` 已有裁决（「确证的生产先例为零」）互证。
4. **最贵的坑不是模型不准，是误伤不可逆**：Adaptive Battery 的头号用户投诉是**推送延迟**；
   业界解法不是把模型练准，而是高优先级通道豁免 + 用户自报豁免（对应 PM-06 教训 4 EcoQoS 误伤 Docker 同构）。
5. **BerryOS 兑付面 = 五张票 + 五条 kill criteria + 规则先行。**「AI 原生调度」在电源域的正确翻译 =
   **预测出票、机制执行、带内可审计**——与既有框架完全同构，零新机制；「人无我有」最强 claim 是
   预测性维护窗与 manifest 内嵌豁免位（Android 用 15 年才赎回的东西，BerryOS day-one 白拿，PM-00 §0.5 同判）。

---

## §1 主线：AI×电源的统一机制全景

**统一模型：观察什么 → 预测什么 → 出口到哪。** 工业界全部收敛为「预测用户/app 未来行为 → 出口到准入/时机/限额」；
学术主流才敢「预测负载 → 直接 DVFS」。这个分叉本身就是最大的证据。

### 1.1 工业实绩深挖总表【确定度：逐行标注】

| 产品 | 观察信号 | 预测目标 | 出口动作 | 机制类型 | 实绩口径 | 确定度 |
|---|---|---|---|---|---|---|
| Adaptive Battery（Android 9，2018） | app 使用历史 | 「未来几小时会用/不会用哪些 app」 | App Standby 桶位 → 网络/任务/闹钟/FCM 限额 | 端侧深度 CNN（DeepMind） | 官方不贴数字；30% 唤醒为转述 | 高（机制）/中（数字） |
| Adaptive Charging（Pixel） | 充电会话长度 + 闹钟 | 长夜充会话与起床时刻 | 充到 ~80% 平台→闹钟前 ~35 分钟充满 | 官方称用 Google AI | 支持论坛+社区实测 | 中 |
| Optimized Battery Charging（iPhone/Mac） | 充电习惯 + 常驻地（显著位置） | 拔充电器时刻 | 延迟 80% 后充电、拔电前补满 | 端侧 ML（官方口径） | Apple 支持文档 | 高（机制）/中（细节） |
| Background App Refresh（iOS） | app 打开模式/条件 | 下次常用打开时刻 | 刷新任务执行时机（约 30s 预算） | 系统「智能调度」（DAS 系） | WWDC/开发者转述 | 高（API）/中（学习面） |
| Adaptive Connectivity（Pixel） | 使用模式 | 当前活动真实带宽需求 | 5G/LTE/Wi-Fi 切换省电 | 规则+预测混合 | 官方支持页+切换投诉 | 中 |
| EMUI AI 调度（华为） | 行为习惯 | app 使用预测 | AI 预加载/核心分配/资源调度（NPU） | 宣传口径「智能行为预测」 | 发布会稿/社区拆解 | 中 |
| 应用智能省电 PowerKeeper（小米） | 应用行为健康度 | 「不良行为」判定（规则为主） | 逐 app 限制策略（可自查） | 规则式为主 | 官方开发者文档 | 高（机制）/中（ML 成分） |

读法：**现役没有一件「AI 直接控频/控帧率」的产品**；连最激进的华为，AI 出口也是预加载与调度建议。

### 1.2 Adaptive Battery 内幕专节（增量，不重抄 PM-01 §1.4）

- **模型**：deep convolutional neural net，预测「未来几小时会用的 app 与大概率不会用的 app」——
  DeepMind 官宣原话（[DeepMind, meet Android](https://deepmind.google/blog/deepmind-meet-android)【高】）。
- **部署形态**：完全端侧，是 DeepMind 首个缩到单台手机算力上的项目（[ZDNet 报道](https://www.zdnet.com/article/google-our-new-android-p-beta-shows-how-ai-gives-you-more-battery-life)【中】）；
  行为数据不出设备（[Android 电源管理文档](https://developer.android.com/about/versions/pie/power)【高】）——**隐私是形态约束不是选项**。
- **出口**：App Standby 桶位。9.0 首发 Active/Working set/Frequent/Rare 四档，后续版本补 Restricted 档（现文档五档）
  （[appstandby 官方文档](https://developer.android.com/topic/performance/appstandby)【高】）。桶决定限额，限额由确定性机制执行。
- **预测式而非回顾式**：刚用过的 app 也可能因「预测你几小时内不再用」被压进 rare 桶（PM-06 §2.1 已载，此处只引）。
- **翻车面与豁免**：受限 app 推送延迟是头号投诉；官方/社区通解 = 高优先级 FCM 消息豁免 + 用户手动把 app 设为 Unrestricted
  （[Medium 排查文](https://medium.com/@alexvukovic/missing-notifications-on-android-adaptive-battery-could-be-the-problem-599e1bf50cfe)【中】、
  [iPresso 技术说明](https://help.ipresso.com/hc/en-us/articles/28770816520989-Why-Are-My-Mobile-Push-Notifications-Delayed)【中】、
  [Pixel 支持帖](https://support.google.com/pixelphone/thread/135909987?hl=en&msgid=140658879)【中】）。
  **教训：ML 误伤靠豁免面兜底，不靠精度兜底。**

### 1.3 预测性充电三家对照（电池寿命轴，与运行时省电同构）

| 家 | 机制 | 冷启动/兜底 | 证据 | 确定度 |
|---|---|---|---|---|
| Pixel Adaptive Charging | 夜充先充 ~80% 后慢充/维持，闹钟前 ~35 分钟充满 | **无闹钟不启用**（预测输入缺失=直接禁用特性） | [Pixel 支持帖](https://support.google.com/pixelphone/thread/249387714/adaptive-charging?hl=en)【中】、[r/GooglePixel](https://www.reddit.com/r/GooglePixel/comments/11c9xzm/why_does_adaptive_charging_only_work_when_you)【中】、[Gearbrain](https://www.gearbrain.com/how-to-use-adaptive-charging-2649705544.html)【中】 |
| Apple Optimized Battery Charging | 学习日常充电节律，预测长充电时段，延迟 80% 后充电 | 学习期约两周量级；**不在常驻地不启用**；预测失败=不延迟 | [Apple 支持 108055](https://support.apple.com/en-us/108055)【高（机制）】、[Mac 版 102338](https://support.apple.com/en-us/102338)【高】 |
| Samsung 电池保护（对照项） | 固定 80% 上限 | 无预测、纯规则 | 用户设置项（公开产品面） | 高（纯规则判定） |

共同设计：**预测不确定就退回保守规则**（直接充满/不延迟/不启用）——冷启动与漂移的兜底全是降级不是硬扛。

### 1.4 iOS 侧：预测用在哪

- **刷新时机**：`BGAppRefreshTask` 只许 app「申请」刷新，**执行时机由系统定**——按使用模式与条件（电量/网络）择机，
  常排在「习惯性打开该 app 之前」，单次约 30 秒预算（[Apple 文档](https://developer.apple.com/documentation/backgroundtasks/bgapprefreshtask)【高】、
  [uynguyen 实践笔记](https://uynguyen.github.io/2020/09/26/Best-practice-iOS-background-processing-Background-App-Refresh-Task)【中】、
  [WWDC19 Advances in App Background Execution](https://developer.apple.com/videos/play/wwdc2019/707)【高】）。
  macOS 侧同构 = NSBackgroundActivityScheduler/DAS 按「能耗/热/CPU 余量」择机（PM-06 §1.2 已引，不重复）。
- **App Predictions（Siri 建议_WIDGET/主屏建议）**：产品面公开、机制不公开（公开资料仅到「on-device 个性化」粒度）【低，不作为论断支柱】。
- 口径：iOS 没有 Adaptive Battery 式「app 分桶」产品——苹果把预测用在**时机**（何时刷新/何时充电）而非**准入**（谁不许跑），治理哲学见 PM-02 §3。

### 1.5 国产 OEM：规则打底、预测点缀

- **华为 EMUI/HarmonyOS**：宣传口径「智能感知学习、智能行为预测、智能资源调度」+ AI 预加载
  （[知乎 EMUI 9.0 详解](https://zhuanlan.zhihu.com/p/47757783)【中】；[SegmentFault EMUI/MIUI 后台对比](https://segmentfault.com/a/1190000039760550)【中】）。
  预测的出口 = 预加载与核心分配，不碰频率直控。
- **小米 PowerKeeper/应用智能省电**：官方文档明说「没有不良系统健康行为的应用不受限」，且**提供 app 自查限制状态的接口**
  （[小米澎湃OS 开发者文档](https://dev.mi.com/xiaomihyperos/documentation/detail?pId=1628)【高】）——「先规则判不良、再限流、可自查」
  与 Android vitals 罚则（PM-01 §2.1）同构；神隐模式（后台网络/GPS/唤醒限制）为规则式【中】。
- 判读：国产系「AI 省电」营销浓度高，可取证的核心仍是**规则式管控 + 少量行为预测**，与 Google 收敛形状一致。

---

## §2 关键事实与证据：学术全景（带批判）

| 领域 | 代表工作 | 报告效果量级 | 批判/复现坑 | 确定度 |
|---|---|---|---|---|
| app 使用预测 | Baeza-Yates 等《Predicting The Next App That You Are Going To Use》（WWW'15 Companion，Yahoo Aviate 启动器日志，个性化 GBDT）（[ACM DL](https://dl.acm.org/doi/pdf/10.1145/2684822.2685302)【高】）；AppUsage2Vec（ICDE 2019，Doc2Vec 式嵌入）（[ResearchGate](https://www.researchgate.net/publication/328600667_AppUsage2Vec_Modeling_Smartphone_App_Usage_for_Prediction)【高】）；LLM 版（[arXiv 2309.08648](https://arxiv.org/html/2309.08648v4)【高】） | 个性化模型可达「可用」精度（转述口径 top-1 ~54%【低，未核到原文数字】）；后续嵌入/LLM 持续小幅抬升 | 单一启动器数据、跨用户泛化差；**预测精度≠省电收益**——论文极少把预测接成真机功耗闭环；Adaptive Battery 是唯一大规模「预测→桶位→省电」落地 | 高（存在性）/低（数字） |
| 学习型 DVFS | RL governor（Zhou 2023，RT 应用，[arXiv 2309.03779](https://arxiv.org/html/2309.03779v1)【高】）；DRL 多任务多 deadline（2025）；综述（2024–2026） | 综述口径：DRL 比 fixed-frequency 省 **25–35%**；per-core DVFS 省 **42%**（Mazumder 2022）【中，综述转述】 | 评估大量依赖模拟器/受控 benchmark；真机翻车三因=功耗模型误差、负载漂移、方差进热路径；**无生产部署先例**（`docs/AI调度设计-v1.0.md:11` 已有同判：「确证的端侧学习型调度生产先例为零」） | 高（研究存在）/中（数字） |
| thermal 预测 | DATE 2018 智能手机皮肤温度预测（[会议 PDF](https://www.date-conference.com/proceedings-archive/2018/pdf/0771.pdf)【高】）；USTA 用户特异皮肤温度感知 DVFS（宣称皮肤温度预测误差 <1%）【中】 | 预测误差 <1%（宣称）；可提前于温度阈值限频 | per-device 标定、真机专属；QEMU/仿真环境无热面——天然归 BerryOS 第二级遥测（真机门控，`docs/AI调度设计-v1.0.md:51`） | 高（存在性）/中（数字） |
| 屏幕态/屏后活动 | Falaki 等 Smart Screen Management（屏幕超时能耗建模）【中】；HUSH 屏后高耗后台抑制（[ResearchGate](https://www.researchgate.net/publication/299867019_Smartphone_Background_Activities_in_the_Wild)【高】）；Chen 等 Smartphone Energy Drain in the Wild（MobiCom'15）【中】 | 屏后后台活动占整机能耗显著份额（转述 ~12.6%）【中】 | 省电出口其实是「抑制/批处理」这类确定性动作——**预测只是触发器**，与工业界形状一致 | 中 |
| 度量方法论 | Android 官方功耗 A/B 指南（对照分屏 + 固定基线版本防漂移）（[Android Developers Blog 2024-04](https://android-developers.googleblog.com/2024/04/how-to-effectively-ab-test-power-consumption-for-your-android-app-features.html)【高】，PM-06 §3 已引） | 官方流程下游戏厂商省 9.68% 的案例 | 业界唯一可信的「功能级功耗收益」度量先例——kill criteria 的度量形状直接抄它 | 高 |

**跨领域批判（本篇综合判）**：论文链路是「预测准 → 假设省电」，工业链路是「预测 → 既有机制 → 轨级 A/B 验收」；
中间缺的那环（预测精度换算成能耗收益并在真机闭环复现）**没有一篇严格补上过**。这就是「论文数字在真机翻车」的结构性原因，
不是模型不行，是**换算链没人验收**。

---

## §3 教训与坑清单

| # | 坑 | 实证 | 业界现役对策 | BerryOS 对策 |
|---|---|---|---|---|
| 1 | **冷启动**：新机/新用户无历史，预测器空转 | Apple 充电优化约两周学习期；Pixel 无闹钟直接不启用（§1.3 证据）【中】 | 预测不置信→特性整体退回保守规则 | 规则默认档先行，ML 票带置信度门限，低置信=不出票 |
| 2 | **分布漂移**：换班/旅行/换季，历史失效 | Apple 自述作息不规律/不在常驻地时可能不启用（[108055](https://support.apple.com/en-us/108055)【高】） | 检测不到漂移就退化（不启用即最大兜底） | 票面短时效（世代计数已有，`docs/AI调度设计-v1.0.md:44`）+ 漂移监控进 kill criteria |
| 3 | **隐私**：行为预测需长期观察使用模式 | Adaptive Battery 全端侧（DeepMind 博客【高】）；Apple 全端侧【高】 | 端侧小模型，数据不出设备 | berry 侧 AI 服务 on-device 是硬约束，遥测不落云 |
| 4 | **误伤不可逆**：推送/闹钟/下载被预测掐住 | Adaptive Battery 推送延迟投诉群（§1.2 三源【中】）；Adaptive Connectivity Wi-Fi 切换投诉（[r/GooglePixel](https://www.reddit.com/r/GooglePixel/comments/180pwe3/turn_off_adaptive_connectivity_if_youre_phone)【中】） | 高优先级通道豁免 + 用户自报豁免 + 一键可关 | **manifest day-one 带豁免位**（高优先级推送/长时任务白名单），票对豁免 app 无效 |
| 5 | **不可回归测试**：ML 决策有方差，进不了 CI | 业界无「ML 电源决策进 CI」先例；现役全部靠灰度+遥测 | 可关开关 + 逐 app 覆盖 | 决策出口=确定性机制（桶位/预算/窗口），**机制面可断言**——「AI 出票」本身做成可重放输入（AI 测试友好化，设计总原则 3） |
| 6 | **功耗模型漂移**：per-app mAh 是估算不是实测 | `power_profile.xml` 模型估算（[AOSP](https://source.android.com/docs/core/power/values)【高】）；硬件实测优于软件模型（NEAT，[SenSys'14](http://sensys.acm.org/2014/papers/p16-brouwers.pdf)【高】）；PM-06 §3 轨级分摊上限已载 | 官方 A/B 用硬件轨级对照 + 固定基线版本（[官方指南](https://android-developers.googleblog.com/2024/04/how-to-effectively-ab-test-power-consumption-for-your-android-app-features.html)【高】） | 能耗收益只认记账面/轨级口径，模型估算不作为 kill criteria 判据 |
| 7 | **ML 自耗**：推理能耗与额外唤醒吃掉收益 | 手表平台「模型能耗净负」判例（`docs/AI调度设计-v1.0.md:95` 平台分层【高，在册决策】） | 平台分层（手表不上 advisor） | 推理走慢循环管家核、随 tickless 睡；自耗进 kill criteria（KC-P4） |
| 8 | **在线再训练缺位**：没人敢在手机上做闭环在线学习 | 现役全部=离线训练+OTA 批量更新；无在线再训练产品（Adaptive Battery/充电优化皆然）【中，以产品面反推】 | 批量 OTA + 可回滚 | 实验轨道只做「离线训练+版本化下发」，策略表世代化=天然回滚点 |

**为什么现役只有规则式+少量 ML（汇总判决）**：规则可解释、可审计、可回滚、翻车可定位到行；ML 四项全缺，
而电源域翻车的代价（闹钟不响/推送不到）由用户直接承受。所以业界把 ML 全部圈进「低风险出口」——
准入、时机、限额——把方差隔离在确定性机制之后。**这不是技术不够，是责任结构使然。**

---

## §4 BerryOS 启示（具体到接口/契约/协议位/验证断言）

### 4.1 带内纪律映射：AI 能出的五张票（不碰「直接控频」红线）

| 票 | 内容 | 出口机制（票兑现处） | 先例强度 | 风险与前置 |
|---|---|---|---|---|
| ① 桶位/准入票 | 预测 app 未来使用 → 建议进 rare/restricted 类桶 | berry-power 唤醒预算与后台准入（PM-01 B7 wake admission，确定性规则执行） | **强**（Adaptive Battery 在役 8 年） | 推送延迟误伤 → 必须先有高优先级通道豁免（manifest 位）才许开此票 |
| ② 维护窗票 | 预测「下一次用户交互窗口」→ 建议把批处理/同步/刷新排进窗口 | berry-power 维护窗调度（Doze maintenance 窗的 ML 版；PM-06 §4.2 空白一） | 中（iOS BAR 时机学习在役但机制不公开；Timer Coalescing 是其规则下界） | 冷启动 2 周；窗偏早=白干、偏晚=内容陈旧 |
| ③ 唤醒预算个性化票 | 按用户作息给每 app 唤醒次数/wakelock 等价时长预算**参数建议** | wake admission 参数表（规则本体不可由 ML 写） | 中（桶位是先例；参数个性化无现役先例=空白） | 漂移敏感；参数改动必须可解释、可列出 |
| ④ thermal 票 | 预测皮肤/结温越限提前量 → 热限频 **hint**（PM 裁决执行） | PM 服务热包络（`docs/AI调度设计-v1.0.md:126` AI 与 PM 同级、PM 执行） | 中（学术扎实：DATE'18/USTA；无现役产品） | 真机门控（第二级遥测，`docs/AI调度设计-v1.0.md:51`）；QEMU 无热面，CI 不可验 |
| ⑤ 预唤醒/预加载票 | intent 链 + app 使用预测 → 建议 appd 预 thaw/预加载 | appd 生命周期（墓碑/freeze 的反向操作） | 中（EMUI AI 预加载在役【中】；学术 app 预测成熟） | 预测错=白烧电+白占内存；必须带置信度与命中率统计 |

红线（不给票、任何模型不得触）：直接设频/设帧率、直接改带预算、直接动 RT/deadline 类、band/intent 归属表——
全部映射到 `docs/AI调度设计-v1.0.md:43`（动作空间四类封顶、band/intent 不入动作空间）与 :80（intent 只降不升）。

### 4.2 kill criteria 候选（对照 `docs/AI调度设计-v1.0.md:11` 框架的具体化）

母判据（在册）：M4 前在可复现 A/B 中拿不到可测收益 → AI 永久 hint-only。电源域实验轨道逐条细化：

| # | 判据（怎样判「ML 不如规则」） | 度量数据源 | 撤的形态 |
|---|---|---|---|
| KC-P1 | 预测质量：ML 唤醒/使用预测命中率（precision@k）打不过 **persistence 基线**（「昨日此刻=今日此刻」规则） | WAKE_RING（`docs/AI调度设计-v1.0.md:115` 已落） | 该票下线，维护窗/预算回固定规则 |
| KC-P2 | 能耗净收益：固定基线版本 A/B（抄 [Android 官方方法论](https://android-developers.googleblog.com/2024/04/how-to-effectively-ab-test-power-consumption-for-your-android-app-features.html)），收益 95% CI 下界 < 3% 场景功耗 → 撤 | berry-power 记账面（DVFS 档位×时长，`pm-notes/00` §1.2 第 11 条） | 永久 hint-only（母判据条款） |
| KC-P3 | 误伤闸门：开票组 vs 关票组，通知/交互 p99 延迟恶化超过阈值（建议 +500ms 量级起步标定） | 遥测环 + smoke 场景（run-rootfs 通道已有 SKT 断言面） | 该票下线并回滚参数 |
| KC-P4 | 模型自耗：推理能耗 + 自身唤醒增量 > 毛收益 1/3 → 撤（手表净负判例，`docs/AI调度设计-v1.0.md:95`） | 记账面 + WAKE_RING（AI 服务自身进程账） | 平台分层收缩（手表/低端档不上） |
| KC-P5 | 时效与漂移：票龄 > 2 个慢循环即弃（世代计数机制已在 :44）；上线后连续 N 天命中率衰减超阈且无再训练版本 → 撤 | 策略表世代 + 命中率统计落盘 | 自动回退启发式并告警（同 :87 崩溃回退路径） |

### 4.3 「人无我有」claim 候选清单

| claim | 先例强度 | 实现难度 | 翻车风险 | 评注 |
|---|---|---|---|---|
| 预测性维护窗（对齐用户而非对齐定时器） | 中 | 中 | 低（收益保守、误伤面小） | **首推**。Timer Coalescing 的 ML 版；PM-06 §4.2 已点名空白一；下界是确定性的批处理窗（先落规则，ML 只挑时机） |
| manifest 内嵌豁免位 + 桶位票 | 强（豁免教训是 Android 15 年补课） | 低 | 低 | **白拿的窗口**：自有 ABI day-one 把「高优先级通道/长时任务豁免」进契约（PM-00 §0.5 同判）；ML 误伤面天生比 Android 小 |
| 按作息的冻结/墓碑压力曲线 | 低-中（规则式 PowerKeeper 在役，无 ML 先例） | 低（规则先行） | 中（误杀定时类 app） | 配合 sched-1.1 token 冻结（`pm-notes/00` §1.1 第 9 条）；ML 只出压力参数 |
| 意图链预测预唤醒 | 中（EMUI 预加载在役；学术成熟） | 高（需 intent 观察面数据积累） | 中（预唤醒白烧） | 依赖 intent 消息传播五件套已落的观察面（`pm-notes/00` §1.1 第 5 条）；v4+ 轨道 |
| 个性化唤醒预算 | 强（桶位在役） | 中 | 高（Adaptive Battery 同款推送延迟翻车） | 必须 4.1 ①的前置豁免位先落 |
| 预测性充电/电池健康管理 | 强（Pixel/Apple 双先例） | 高（硬件面） | 低 | BerryOS v1 QEMU 主验、无充电控制器抽象——真机后轨道，现在只留记账协议位 |

### 4.4 验证断言（可执行，进 smoke/ktest）

1. **空表等价推广**：AI 票服务缺席/空表时，桶位/预算/窗口全等于默认规则——复用 `POLICY ok` 空表等价断言模式
   （`docs/AI调度设计-v1.0.md:115`；断言形态：绑空票表跑 N 轮与无票表基线逐位全等）。
2. **票不升权**：任何票序列不能把 app 待遇提升到高于 manifest 声明档（pledge 单调收窄，`docs/AI调度设计-v1.0.md:74,80`）；
   ktest 断言投「升档票」→ 拒收且记日志。
3. **豁免位生效**：持高优先级豁免位的 app，投 rare 桶票后其唤醒预算与通道限额不变（断言面：wake admission 计数器）。
4. **票龄丢弃**：票带世代计数，表龄超 2 个慢循环被 watchdog 判僵死自动回退（`docs/AI调度设计-v1.0.md:44`）。
5. **崩溃保活**：杀 berry AI 票服务 → 维护窗/批处理照常跑默认规则（smoke：kill 服务后跑批处理场景断言完成）。
6. **记帐不撒谎**：能耗收益断言只允许引用记账面（档位×时长）与轨级口径，禁止引用模型估算值（PM-06 教训 7 同款铁律）。

---

## §5 引用源列表

工业（一手/官方）：
- DeepMind, meet Android（Adaptive Battery 官宣）— https://deepmind.google/blog/deepmind-meet-android
- Android 9 电源管理（Adaptive Battery/App Standby）— https://developer.android.com/about/versions/pie/power
- App Standby Buckets（五档现行文档）— https://developer.android.com/topic/performance/appstandby
- Apple：iPhone 充电优化 — https://support.apple.com/en-us/108055 ；Mac — https://support.apple.com/en-us/102338
- Apple：BGAppRefreshTask — https://developer.apple.com/documentation/backgroundtasks/bgapprefreshtask
- WWDC19 Advances in App Background Execution — https://developer.apple.com/videos/play/wwdc2019/707
- Pixel Adaptive Connectivity（网络设置）— https://support.google.com/pixelphone/answer/2819583?hl=en
- 小米澎湃OS PowerKeeper 管控说明 — https://dev.mi.com/xiaomihyperos/documentation/detail?pId=1628
- Android Developers Blog 2024-04（功耗 A/B 方法论）— https://android-developers.googleblog.com/2024/04/how-to-effectively-ab-test-power-consumption-for-your-android-app-features.html
- AOSP Power values（power_profile 估算口径）— https://source.android.com/docs/core/power/values

工业（二手/社区，均标【中】）：
- Android Authority：Adaptive Battery（30% 唤醒转述）— https://www.androidauthority.com/adaptive-battery-google-android-863032
- ZDNet：Android P beta 与端侧 AI — https://www.zdnet.com/article/google-our-new-android-p-beta-shows-how-ai-gives-you-more-battery-life
- Pixel 支持帖：Adaptive Charging（闹钟前 ~35 分钟）— https://support.google.com/pixelphone/thread/249387714/adaptive-charging?hl=en
- r/GooglePixel：adaptive charging 闹钟依赖 — https://www.reddit.com/r/GooglePixel/comments/11c9xzm/why_does_adaptive_charging_only_work_when_you
- Medium：Adaptive Battery 推送延迟排查 — https://medium.com/@alexvukovic/missing-notifications-on-android-adaptive-battery-could-be-the-problem-599e1bf50cfe
- iPresso：推送延迟与高优先级 FCM — https://help.ipresso.com/hc/en-us/articles/28770816520989-Why-Are-My-Mobile-Push-Notifications-Delayed
- r/GooglePixel：Adaptive Connectivity 投诉 — https://www.reddit.com/r/GooglePixel/comments/180pwe3/turn_off_adaptive_connectivity_if_youre_phone
- 知乎：EMUI 9.0 AI 详解 — https://zhuanlan.zhihu.com/p/47757783 ；SegmentFault：EMUI/MIUI 后台对比 — https://segmentfault.com/a/1190000039760550
- uynguyen：iOS 后台刷新实践 — https://uynguyen.github.io/2020/09/26/Best-practice-iOS-background-processing-Background-App-Refresh-Task

学术：
- Baeza-Yates 等：Predicting The Next App（WWW'15 Companion）— https://dl.acm.org/doi/pdf/10.1145/2684822.2685302
- AppUsage2Vec（ICDE 2019）— https://www.researchgate.net/publication/328600667_AppUsage2Vec_Modeling_Smartphone_App_Usage_for_Prediction
- LLM 版 app 预测 — https://arxiv.org/html/2309.08648v4
- Zhou 2023：RL DVFS governor — https://arxiv.org/html/2309.03779v1
- DATE 2018：皮肤温度预测 — https://www.date-conference.com/proceedings-archive/2018/pdf/0771.pdf
- HUSH/屏后活动 — https://www.researchgate.net/publication/299867019_Smartphone_Background_Activities_in_the_Wild
- NEAT（硬件实测 vs 软件模型）— http://sensys.acm.org/2014/papers/p16-brouwers.pdf

仓内互引：
- `research/PM-00-全系统省电架构综合裁决-v1.0.md`（§0/§2/§5 四层模型与 berry-power 裁决）
- `research/PM-01-安卓省电演化史与历史包袱-v1.0.md`（§1.4 桶位 ML 时代线、§2.1 vitals 罚则、B7 wake admission）
- `research/PM-02-iOS与鸿蒙省电哲学对照-v1.0.md`（iOS 治理哲学，本篇 §1.4 只引）
- `research/PM-06-PC桌面与系统级协调架构-v1.0.md`（§4 现役 AI 清单与口径校准、教训 4/7）
- `docs/AI调度设计-v1.0.md`（:11 kill criteria 母判据、:43 动作空间封顶、:44 世代计数、:51 遥测两级、
  :74/:80 单调收窄与 intent 只降不升、:95 平台分层、:115 v1 落地注记、:126 PM 裁决执行）
- `research/pm-notes/00-BerryOS省电现状与缺口盘点.md`（§1.1 已落机制底账、§1.2 设计位）
