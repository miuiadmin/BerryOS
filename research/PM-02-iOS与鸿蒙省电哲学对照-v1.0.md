# PM-02 iOS 与鸿蒙省电哲学对照——「从不给予」vs「给权力后收紧」（v1.0）

> 调研日期：2026-11-07。方法：联网检索（Apple/华为官方文档 + gitee openharmony 官方仓）+
> 本地 OH 浅克隆只读取证 + 社区实测交叉。许可边界：代码仅取自 Apache-2.0 仓（openharmony/*）。
> 确定度标注：**高** = 官方文档/一手源码；**中** = 二手解析或社区实测；**低** = 推断或单一来源。
> PM 系列分工：PM-00 为总览与索引；PM-02 专责「平台省电哲学 + 后台治理契约」。
> 渲染侧帧率/停帧细节见 `research/GUI-06-低功耗渲染策略.md` §2，调度器内因见 SCHED-01，
> Android 渲染教训见 GUI-01——本文对 LTPO/ProMotion 只交叉引用，不重复展开。

## §1 演化与机制主线

### 1.1 iOS：从不给予（省电靠"权力从未下放"）

| 时间 | 事件 | 意义 |
|---|---|---|
| 2007–2009 | 初代 iPhone 无第三方后台执行 API，应用切走即冻结 | 契约起点：没有"后台运行权"这个概念可申请【确定度：中，史实广泛转述】 |
| 2009 | APNs 随 iOS 3 上线，系统级单一长连接代所有应用收推送 | 推送不依赖应用存活，长连接保活失去存在理由【确定度：中（年表）】 |
| 2010 | iOS 4 引入多任务：fast app switching + 白名单（audio/location/VoIP 等） | 多任务从第一天就是"枚举式白名单"，不是通用权利【确定度：中】 |
| 2013 | OS X Mavericks 引入 App Nap + timer coalescing，后传导至 iOS | 定时器合并成为系统默认行为，应用只需声明容忍度【确定度：高，见 §2.6】 |
| 2015 | iOS 9 低电量模式（软件削减 + 硬件降频） | 系统单方面全局降档，应用无否决权【确定度：高，见 §2.4】 |
| 2019 | iOS 13 BGTaskScheduler（刷新/处理两类后台任务） | 后台工作彻底"预约制"：应用提案、系统排期【确定度：高，见 §2.3】 |

主线：**应用从来拿不到"保持运行"的权力**——退后台短时宽限后即挂起（内存驻留、零代码执行），
系统随时可终止；一切后台能力都是窄枚举 + 系统排期 + 可撤销。

### 1.2 Android → OpenHarmony：给权力后收紧（wakelock 血统的赎罪史）

| 时间 | Android 事件 | 代价 |
|---|---|---|
| 2009 起 | wakelock 进入应用框架，service 可常驻，广播拉起链 | 应用可单方面宣称"我要醒着"，举证责任落在系统【确定度：高（机制）】 |
| 2015 | 6.0 Doze + App Standby：静止+灭屏+不充电时推迟同步/任务/wakelock | 亡羊补牢第一轮【确定度：高】 |
| 2017 | 8.0 后台执行限制：禁止任意后台 service、砍隐式广播 | 生态对抗升级：前台服务保活、常驻通知、高优 FCM 滥用【确定度：高/中】 |
| 2019+ | 9.0 App Standby Buckets 分桶限权；14 前台服务须声明类型 | 收紧仍在继续，targetSdk 分级执行 = 永久兼容包袱【确定度：中】 |

OpenHarmony（2020+）出生即带着这份教训（【确定度：高，见 §2.9/§2.10 证据】）：

- 血统上继承 RunningLock（≈wakelock，连命名与 `ohos.permission.RUNNING_LOCK` 权限都同构）；
- 但**通用防睡眠锁 `BACKGROUND` 类型从 API 10 起废弃**——三方应用再无"阻止系统睡眠"通道，
  仅剩接近光锁与闲时锁两枚窄枚举；
- 后台默认治理 = 挂起 + 资源配额（内存/CPU/24h 磁盘写）+ 超限杀进程 + 接入规范与违规处罚；
- 推送（Push Kit）与实况窗刷新走系统级通道，明确"不依赖应用进程的存活"。

### 1.3 HarmonyOS 商业版：治理之上的用户可见层

应用启动管理三开关（允许自启动 / 允许关联启动 / 允许后台活动）+ 智能省电开关 +
省电模式（限制后台活动、减弱视觉效果）/ 超级省电 + 应用耗电管理
（华为官网支持页，【确定度：高，见 §2.13】）。模式：默认自动管理，用户可逐应用手动收紧——
治理在系统，开关给用户，应用没有申诉接口。

### 1.4 三哲学速览对照表

| 维度 | iOS（从不给予） | Android（给后收紧） | OH/HarmonyOS（出生即收紧） |
|---|---|---|---|
| 后台默认 | 短宽限→挂起（零执行）→可终止 | 演化中逐步逼近 iOS | 退后台一段时间即挂起；配额超限杀 |
| 防睡 API | 无等价物 | wakelock（仍在，Doze 压制） | RunningLock 通用类型已废弃 |
| 推送 | APNs 系统单通道 | FCM（国内缺失→保活黑产） | Push Kit 系统单通道 |
| 延迟任务 | BGTaskScheduler 系统排期 | JobScheduler/WorkManager | WorkScheduler（条件+系统状态） |
| 长时任务 | 后台模式白名单（审核制） | 前台服务+通知 | 长时任务白名单+系统校验+违规处罚 |
| 用户开关 | 后台 App 刷新（全局/逐应用） | 电池优化白名单（各家 UI 不一） | 应用启动管理三开关 |
| 低电量模式 | 软硬双层（关 P 核/降频） | 厂商各异 | 省电/超级省电/智能省电 |

## §2 关键事实与证据

### 2.1 iOS 进程生命周期：挂起 ≠ 后台运行【确定度：高】

来源：[Managing your app's life cycle](https://developer.apple.com/documentation/uikit/managing-your-app-s-life-cycle) ·
[Reduce terminations in your app](https://developer.apple.com/documentation/xcode/reduce-terminations-in-your-app)

| 状态 | 语义 | 省电含义 |
|---|---|---|
| 前台活跃 | 正常执行 | — |
| 前台不活跃 | 触摸中断/切换中 | — |
| 后台短宽限 | `beginBackgroundTask` 续命，数十秒级（约 30s，随系统状态浮动【确定度：中】） | 留给保存状态/发消息 |
| 挂起 suspended | 内存驻留、**零代码执行**、定时器/网络全冻结 | 待机耗电与应用数近似解耦 |
| 终止 | 系统按需回收全部资源 | 内存压力驱动，非惩罚 |

- `UIApplicationExitsOnSuspend`（已不推荐）可让应用退后台即退出而非挂起：
  [信息属性表键](https://developer.apple.com/documentation/bundleresources/information-property-list/uiapplicationexitsonsuspend)。
- 关键推论：**iOS 没有"后台运行中的应用"，只有"挂起中等待唤醒的应用"**——省电争议小的根因。

### 2.2 APNs：系统级单一长连接【确定度：高】

来源：[APNs 部署指南](https://support.apple.com/en-ng/guide/deployment/dep2de55389a/web) ·
[Apple 支持：推送与通知](https://support.apple.com/en-us/102266)

| 特性 | 机制 |
|---|---|
| 连接归属 | 操作系统与 Apple 服务器维持一条加密长连接，所有应用共享 |
| 应用侧成本 | 零——应用进程可被终止，推送照达 |
| 静默唤醒 | `content-available` 通知由系统按预算唤醒应用处理，应用不能自定时机 |
| 对省电的贡献 | N 个应用 = 0 条应用私有长连接（对比：Android 国内生态 = N 条保活连接） |

### 2.3 iOS 后台模式白名单 + BGTaskScheduler【确定度：高】

来源：[Using background tasks to update your app](https://developer.apple.com/documentation/uikit/using-background-tasks-to-update-your-app) ·
[BGTaskScheduler](https://developer.apple.com/documentation/backgroundtasks/bgtaskscheduler)

| 通道 | 类型 | 排期权 |
|---|---|---|
| 后台音频/定位/VoIP 等 | 声明式白名单（审核制 entitlement） | 系统持续校验实际使用 |
| BGAppRefreshTask | 短刷新（~30s 预算） | 系统按使用习惯排期 |
| BGProcessingTask | 长处理（充电/夜间窗口） | 系统选时机，可要求充电与网络条件 |
| 后台 App 刷新 | 用户总开关 + 逐应用开关 | 用户可直接关闭全部后台刷新 |

要点：**应用只能"提案"，排期、频率、是否执行全在系统**；用户开关存在且语义清晰。

### 2.4 iOS 低电量模式：软硬双层【确定度：高（行为）/ 中（硬件实测）】

来源：[官方行为列表](https://support.apple.com/en-us/101604) ·
[Reddit 实测拆解](https://www.reddit.com/r/iphone/comments/vlpfcv/a_full_breakdown_of_what_low_power_mode_actually)

| 层 | 内容 |
|---|---|
| 软件层（官方，高） | 自动锁屏降 30s、调暗屏、减弱视觉效果、停后台 App 刷新、减动画、5G 优化等 |
| 硬件层（社区实测，中） | 关闭 2 个 P 核；E 核 1.8→1.38GHz；关/降 GPU 核；整体性能约降 40% |
| 触发 | 电量 80%/20% 提示，用户确认；充电至 80% 自动退出 |

### 2.5 iOS 热治理：把降频决策下放给应用的"只读合同"【确定度：高】

来源：[ProcessInfo.thermalState](https://developer.apple.com/documentation/foundation/processinfo/3608555-thermalstate) ·
[Energy Efficiency Guide（archive 根目录，章节 Respond to Thermal State Changes）](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/EnergyGuide-iOS/)

| 热档 | 官方建议应用侧行为 |
|---|---|
| nominal | 正常 |
| fair | 开始省：减帧率、减定位精度 |
| serious | 显著减：帧率 60→30、降低定位频率、推后非关键工作 |
| critical | 最低 CPU/GPU/IO，只保关键交互 |

模式：**系统降频（硬件侧）+ 应用自查 thermalState 降负载（软件侧）双向配合**，
但应用只有"读"与"自愿配合"，没有议价权。

### 2.6 iOS 定时器合并与后台活动调度【确定度：高】

来源：[NSBackgroundActivityScheduler](https://developer.apple.com/documentation/foundation/nsbackgroundactivityscheduler) ·
[Energy Efficiency Guide（archive，章节 Timers / ScheduleActivity）](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/EnergyGuide-iOS/)

- timer coalescing：`Timer` 设 tolerance 后系统可将多个到期点合并到同一唤醒窗口执行；
- `NSBackgroundActivityScheduler`：声明"这类活动可延后"，系统自行选时机（可能整夜推迟，
  与其他活动合并到维护窗口）；
- App Nap（2013 起）：不可见窗口的定时器、IO 被系统节流，无需应用改造。
结论：**省电的本质单位是"唤醒次数"，iOS 把合并唤醒做成系统默认**。

### 2.7 ProMotion/LTPO 自适应刷新【交叉引用】

帧率档位/vote 协议细节见 `research/GUI-06-低功耗渲染策略.md` §2（含 iOS ProMotion 与
Android `setFrameRate` 投票对照），本文不重复。对 PM 的落点已在 ADR #53 三态停帧承接。

### 2.8 OpenHarmony 电源状态机【确定度：高（组件职责）/ 中（状态名与转换）】

来源：[powermgr_power_manager 仓](https://gitee.com/openharmony/powermgr_power_manager)（自述：系统电源状态管理、
电源状态转换、休眠唤醒、亮灭屏、休眠运行锁管理）·
[OH 论坛源码解析：熄屏和休眠机制](https://forums.openharmony.cn/forum.php?mod=viewthread&tid=1333)

| 状态（社区解析，中） | 对应 |
|---|---|
| AWAKE | 亮屏工作 |
| INACTIVE | 灭屏但未睡（ RunningLock 可抑制进入 SLEEP） |
| SLEEP | S3 suspend-to-RAM |
| Hibernate | S4（部分设备） |

子系统全景（Apache-2.0 官方仓，高）：
[powermgr_power_manager](https://gitee.com/openharmony/powermgr_power_manager)（状态机/运行锁）、
powermgr_display_manager（亮度/灭屏）、powermgr_battery_manager（电量/充放电）、
powermgr_thermal_manager（热）、powermgr_battery_statistics（耗电归因统计）。

### 2.9 OpenHarmony RunningLock：wakelock 血统与"废弃通用锁"【确定度：高】

来源：[@ohos.runningLock API 参考](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-runninglock)

| 事实 | 证据 |
|---|---|
| 权限门槛 | `ohos.permission.RUNNING_LOCK`（创建与持锁都要） |
| 类型表 | `BACKGROUND`(=1, 阻止系统睡眠) **自 API 10 起废弃**；`PROXIMITY_SCREEN_CONTROL`(=2, 接近光控亮灭屏)；`BACKGROUND_USER_IDLE`(=129, API 23+, 闲时防自动睡眠，且不能阻止合盖强睡) |
| 超时语义 | `hold(timeout)`：-1 永久（须主动 unhold）/ 0 默认 3s / >0 定时释放；后调覆盖前调 |
| 防泄漏设计 | JS 对象被 GC 时底层锁**自动释放**；电源服务重启后锁对象仍在但持有状态可能丢失（`isHolding()` 自查） |
| 线程约束 | 同一实例 hold/unhold/isHolding 必须同线程，跨线程静默不一致 |

**关键读数：通用"阻止睡眠"锁对三方废弃 = OH 正式放弃 Android 式给权**；
存活下来的锁全部绑定具体硬件场景（接近光/闲时），且系统内部仍用锁
（本地取证：`window_window_manager` 的 scene_session_manager 持 `RUNNINGLOCK_SCREEN` 锁保灭屏期窗口服务，
`~/Documents/harmony/openharmony/window_window_manager/window_scene/session_manager/src/scene_session_manager.cpp`
L6323-6338，Apache-2.0）——**锁没有消失，是收归系统内部**。

### 2.10 OpenHarmony 后台任务治理：规范内四通道 + 配额 + 处罚【确定度：高】

来源：[Background Tasks Kit 简介](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/background-task-overview) ·
[术语表](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/background-tasks-glossary) ·
[接入规范与违规处罚](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/bgtask-design-formula)

默认治理（高）：

- 应用退后台一段时间 → 进程**挂起**：公共事件/定时器等软件资源与 CPU/网络/GPS/蓝牙等
  硬件资源全部不可用；
- 资源不足 → 终止；**配额**：连续内存使用、CPU 占比、24h 磁盘写 IO 各有上限——
  前台超限 warning 日志，**后台超限直接杀进程**；
- 申请了规范内任务"仅提升回收优先级"，系统资源严重不足时照样可杀。

四类规范内任务（高）：

| 通道 | 适配场景 | 关键约束 |
|---|---|---|
| 短时任务 TransientTask | 高实时、短耗时（保存/发消息） | 单次上限 3 分钟（[best practice 实测 180000ms](https://developer.huawei.com/consumer/cn/doc/best-practices/bpta-back-task-implement)）+ 时间配额制，耗尽禁申 |
| 长时任务 ContinuousTask | 用户可感知（后台音乐/导航/设备连接） | 模式白名单（[backgroundTaskManager API](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-resourceschedule-backgroundtaskmanager)）；**系统持续校验**：音频被焦点打断→系统自动冻结/取消长时任务，重启播放须重新申请（[AVSession 后台场景](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/avsession-background-scene)）；通知栏必须展示 |
| 延迟任务 WorkScheduler | 可推迟 | 条件（网络类型/充电类型/存储状态/电池状态，至少一个）+ 系统状态（内存/**功耗**/温度/**用户使用习惯**）统一排期；循环任务间隔 ≥ 2 小时（[work-scheduler 指南](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/work-scheduler)、[WorkInfo 参考](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-resourceschedule-workscheduler)） |
| 代理提醒 | 倒计时/日历/闹钟 | 系统代理提醒，应用退后台甚至被终止都照常生效 |

执法细节（高）：

- 锁屏或退后台 **5 秒**内未申请长时任务的协同业务即被系统断开
  （[分布式协同指南](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/abilityconnectmanager-guidelines)）；
- 窗口退后台系统**自动释放常亮锁**；音视频流中断一段时间也自动释放
  （[亮度控制指南](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/brightness-control)）；
- 配置错误有独立错误码族（bgMode 无效/子类型未定义等，
  [错误码参考](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/errorcode-backgroundtaskmgr)）；
- 接入要写承诺书（媒体处理/音视频投播/运动三选一），违规有公开处罚标准
  （[bgtask-design-formula](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/bgtask-design-formula)）。

### 2.11 Push Kit 与实况窗：系统级通道 + 不依赖进程存活【确定度：高】

来源：[Push Kit 简介](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/push-kit-introduction) ·
[pushService API](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/push-pushservice) ·
[实况窗](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/liveview-introduction)

- 云到端系统级通道；每设备每应用 Push Token 唯一，云侧按 Token 下发；
- 设备离线时**云端缓存**、上线后补推（送达优先于实时）；
- 实况窗（打车/外卖/赛事等实时卡片）经 Push Kit 刷新，官方明示"不依赖应用进程的存活"；
- 频控内建：出行打车/赛事每设备 5 分钟 ≤30 次、每小时 ≤180 次；其余场景 5 分钟 ≤10、
  每小时 ≤60，超限丢弃；
- 连通话服务（Call Service Kit）都构建在 Push Kit 之上——**实时性需求也尽量走推送而非保活**。

### 2.12 HarmonyOS 商业版用户治理层【确定度：高】

来源：[开启智能省电](https://consumer.huawei.com/cn/support/content/zh-cn00764317) ·
[耗电快/待机短官方指引](https://consumer.huawei.com/cn/support/content/zh-cn00409892)

| 入口 | 内容 |
|---|---|
| 设置 > 应用和服务 > 应用启动管理 | 默认自动管理；手动模式 = 三开关：允许自启动 / 允许关联启动 / 允许后台活动 |
| 设置 > 电池 | 省电模式（限制后台应用活动、减弱视觉效果）/ 超级省电 / 智能省电开关 |
| 应用耗电管理 | 逐应用"允许完全后台行为"开关 |

### 2.13 Android 对照锚点【确定度：高】

来源：[Optimize for Doze and App Standby](https://developer.android.com/training/monitoring-device-state/doze-standby)

- Doze：设备静止+灭屏+不充电 → 推迟同步/Job、合并 wakelock 到维护窗口；
- App Standby：久未触碰的应用限制网络/CPU；
- 官方文档自述动机即 wakelock 与后台滥用——印证"给权力后收紧"主线。

### 2.14 待机耗电量化：平台差距 < 个体应用差距【确定度：中/低】

来源：[r/iphone 隔夜耗电讨论](https://www.reddit.com/r/iphone/comments/10c7e78/is_34_battery_drain_overnight_normal) ·
[Whizz-Mobile 隔夜耗电指南](https://whizz-mobile.com/guides/cell-phone-battery-drains-overnight-simple-fix)

- 社区基准：iPhone 隔夜 3–5% 属常态；优化好的设备 2–3%；异常 10%+ 通常对应失控应用/弱信号；
- **未找到公开的严格 iOS vs Android 隔夜对照实验**（变量：机型/系统版本/装的应用/信号）；
- 可下结论的只有机制层：iOS 把后台执行权收走 → 待机耗电与应用数量近似解耦；
  Android/OH 上限同可达个位数，但依赖每个应用守规矩或用户手动治理。
  【确定度：低（对照实验缺失，属机制推断）】

## §3 教训与历史包袱清单

| # | 教训 | 证据锚点 | 确定度 |
|---|---|---|---|
| 1 | wakelock 是"负向权限"：应用宣称"我要醒着"，系统难以反证；Android 用 8 年（2009–2017）才追平约束 | §1.2 | 高 |
| 2 | API 给出去再收回，代价是数量级的：Doze/后台限制都要 targetSdk 分级执行，老应用长期豁免，OEM 再各自加码，碎片化治理成本永远付不完 | §1.2/§2.13 | 高 |
| 3 | 开发者会对抗性进化：前台服务保活、常驻通知、高优先级推送滥用——治理设计必须预设"被滥用"并让滥用路径本身高成本 | §1.2 | 中 |
| 4 | 没有统一推送 = 保活是刚需（Android 国内生态是反例教材）；APNs/Push Kit 证明系统单通道是省电的一半 | §2.2/§2.11 | 高 |
| 5 | 白名单也需持续执法：iOS VoIP 后台被滥用后收成 PushKit、后台定位反复收紧；OH 直接给违规处罚标准——准入 + 校验 + 处罚三件套缺一不可 | §1.1/§2.10 | 高 |
| 6 | "系统校验声明"是对付挂羊头的关键：OH 检测音频焦点丢失即冻结长时任务，把声明的理由与可观测硬件使用交叉验证 | §2.10 | 高 |
| 7 | 通知既是权利也是监控点：长时任务必须常驻通知，用户划掉通知 = 任务退场；通知栏是用户可见的执法界面 | §2.10 | 高 |
| 8 | 挂起优于终止：iOS 挂起（内存驻留零执行）使恢复瞬时、待机省电，终止只作内存压力手段——失败隔离而非惩罚 | §2.1 | 高 |
| 9 | 锁不必消灭，收归系统即可：OH 废弃三方通用锁，但系统服务内部照用 RUNNINGLOCK_SCREEN | §2.9 本地取证 | 高 |
| 10 | 省电与实时性冲突由"系统排期 + 条件触发"化解：BGTaskScheduler/WorkScheduler 都让应用声明条件，系统选时机 | §2.3/§2.10 | 高 |

## §4 BerryOS 启示（映射到接口/契约/协议位）

前置对齐：BerryOS 自有 ABI + v1 无 fork → **无历史二进制包袱，契约可日清**
（对照教训 #2：这是对 Android 最大的一次性优势，过期作废）；微内核"能力即权限 +
失败隔离"与 intent(u8) 第一类输入（ADR #42 两段式、#58 六档词汇表）为既有地基；
ADR #53 已定低功耗 GUI 六协议位（damage 矩形集/三态停帧 RUN·SETTLE·IDLE/vote 区间票面/
power·mode op/PM 共享环/能耗代理断言）。

| # | 可执行动作 | 落点 |
|---|---|---|
| 1 | **SDK 首发即无通用 wakelock 等价物**：不提供"阻止休眠"syscall，也不进 35 个在册名额。防睡需求只经长时任务白名单通道表达 | syscall 表 + SDK 文档；对照 OH 教训（给了再废，成本一个数量级） |
| 2 | **长时任务 reason 用 u8 枚举进协议位**（对齐 intent u8 先例）：0x01 音频播放、0x02 导航、0x03 设备连接、0x04 下载、0x05 编译/训练类重载。未知枚举 = 一律拒绝；错误码即治理（学 OH bgMode 无效错误码族） | berry-sys ABI + admission 准入（现有 sched-1.1 准入检查器直接挂枚举表） |
| 3 | **"保持运行"建模为电源仲裁服务的带租约能力票**：申请必带 (reason:u8, duration, 续租 token)，到期自动失效，仲裁服务可单方撤销——复用 ADR #53 vote 区间票面语义（票面=有期限、可叠加、可撤销），杜绝 -1 永久持锁（OH 永久锁语义勿学） | 电源仲裁服务（用户态常驻，规则05 服务监督域）+ 能力句柄 |
| 4 | **系统校验写进监督合同**：申请 0x01 音频播放者必须已从音频服务取得焦点会话；焦点丢失 → 仲裁服务自动冻结任务并要求重申（OH avsession 同款）。声明理由与可观测硬件使用交叉验证作为服务验收断言 | 音频服务 + 电源仲裁服务间 IPC 契约；smoke 断言 |
| 5 | **统一推送为一等系统服务**：设备↔云单长连接归系统；应用终止不丢消息；实况卡片走系统代理渲染（OH 实况窗"不依赖进程存活"同构）。生态规范直接写"应用不得自建推送长连接" | 推送服务（用户态常驻）+ 卡片服务；协议位上等同 ADR #53 的系统侧通道 |
| 6 | **挂起 = 冻结进程：页全换出 + 调度器除名**，优于终止。demand paging 机制面已启（G4）正好承接：唤醒按需换入。终止只留给内存压力 | pager + 调度器除名接口；对照 iOS suspended 语义 |
| 7 | **per-进程资源配额内置**：CPU 占比、24h 磁盘写 IO、唤醒次数/小时三配额；前台超限 warning、后台超限冻结。配额余量经 PM 共享环（ADR #53）对应用可读——可观测性即合规 | 电源仲裁服务 + PM 共享环遥测（sched-1.1 freeze 遥测已铺） |
| 8 | **热与低电量是调度策略输入，不是应用 API**：thermal_state/lpm_mode 作为系统态只叠加不替换（#42 语义），由可插拔策略消费（E 核降频/帧率帽与 GUI-06 三态停帧联动）；应用只获只读查询（getattr 类），无议价权 | sched 策略插件 + getattr 只读位 |
| 9 | **延迟任务 = work_scheduler 域服务**：条件（网络/充电/电量/温度）+ 系统状态统一排期；循环任务最小间隔 2h 起步。"用户使用习惯"输入留给 AI 原生调度实验轨道（v2，带 kill criteria） | 用户态服务 + intent 叠加 |
| 10 | **通知即凭证 + 用户三开关**：长时任务存活绑定常驻通知（通知被划掉=任务退场）；设置内逐应用三开关（自启动/关联启动/后台活动）——HarmonyOS 商业版同款语义，用户可见、系统执法 | 通知服务 + 电源仲裁服务联动；设置项 |

一句话总纲：**iOS 证明"从不给予"可行，OH 证明"出生即收紧"是后来者最优解；
BerryOS 兼得两者起点——微内核能力模型天然支持"窄枚举 + 可撤销租约 + 系统校验"三件套，
唯一真正的敌人是"先给方便后收回"的诱惑。**

## §5 引用源列表

### Apple 官方（确定度：高）

- 生命周期：https://developer.apple.com/documentation/uikit/managing-your-app-s-life-cycle
- 减少终止：https://developer.apple.com/documentation/xcode/reduce-terminations-in-your-app
- UIApplicationExitsOnSuspend：
  https://developer.apple.com/documentation/bundleresources/information-property-list/uiapplicationexitsonsuspend
- 后台任务：https://developer.apple.com/documentation/uikit/using-background-tasks-to-update-your-app
- BGTaskScheduler：https://developer.apple.com/documentation/backgroundtasks/bgtaskscheduler
- APNs 部署：https://support.apple.com/en-ng/guide/deployment/dep2de55389a/web
- 推送与通知（支持页）：https://support.apple.com/en-us/102266
- 低电量模式：https://support.apple.com/en-us/101604
- thermalState：https://developer.apple.com/documentation/foundation/processinfo/3608555-thermalstate
- 能效指南（archive 根，章节 Timers/ScheduleActivity/Respond to Thermal State Changes）：
  https://developer.apple.com/library/archive/documentation/Performance/Conceptual/EnergyGuide-iOS/
- NSBackgroundActivityScheduler：
  https://developer.apple.com/documentation/foundation/nsbackgroundactivityscheduler

### HarmonyOS / OpenHarmony 官方（确定度：高）

- RunningLock API：https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-runninglock
- Background Tasks Kit 简介：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/background-task-overview
- 术语表：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/background-tasks-glossary
- 接入规范与违规处罚：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/bgtask-design-formula
- backgroundTaskManager API：
  https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-resourceschedule-backgroundtaskmanager
- 短时任务实测（3 分钟）：https://developer.huawei.com/consumer/cn/doc/best-practices/bpta-back-task-implement
- 长时任务系统校验：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/avsession-background-scene
- 延迟任务指南：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/work-scheduler
- WorkInfo 参考：
  https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-resourceschedule-workscheduler
- 后台任务错误码：https://developer.huawei.com/consumer/cn/doc/harmonyos-references/errorcode-backgroundtaskmgr
- 协同 5 秒断开：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/abilityconnectmanager-guidelines
- 亮度/常亮锁自动释放：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/brightness-control
- Push Kit 简介：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/push-kit-introduction
- pushService API：https://developer.huawei.com/consumer/cn/doc/harmonyos-references/push-pushservice
- 实况窗：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/liveview-introduction
- powermgr_power_manager 仓：https://gitee.com/openharmony/powermgr_power_manager

### 华为消费者支持（确定度：高）

- 智能省电：https://consumer.huawei.com/cn/support/content/zh-cn00764317
- 耗电快/待机短：https://consumer.huawei.com/cn/support/content/zh-cn00409892

### Android 官方（确定度：高）

- Doze 与 App Standby：https://developer.android.com/training/monitoring-device-state/doze-standby

### 二手解析与社区实测（确定度：中）

- OH 熄屏休眠源码解析（论坛帖）：https://forums.openharmony.cn/forum.php?mod=viewthread&tid=1333
- iOS 低电量模式硬件实测（Reddit）：
  https://www.reddit.com/r/iphone/comments/vlpfcv/a_full_breakdown_of_what_low_power_mode_actually
- iPhone 隔夜耗电（Reddit）：https://www.reddit.com/r/iphone/comments/10c7e78/is_34_battery_drain_overnight_normal
- 隔夜耗电诊断（Whizz-Mobile）：https://whizz-mobile.com/guides/cell-phone-battery-drains-overnight-simple-fix

### 本地代码取证（Apache-2.0，只读）

- `~/Documents/harmony/openharmony/window_window_manager/window_scene/session_manager/src/scene_session_manager.cpp`
  L6323-6338：窗口服务在灭屏流程持 `PowerMgr::RunningLockType::RUNNINGLOCK_SCREEN`
  ——系统内部用锁、三方无通用锁的直接实证。
