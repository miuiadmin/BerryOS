# PM-06 PC/桌面演化与系统级电源协调架构与功耗归因（v1.0）

> 调研日期：2026-11-07 · 方法：联网检索（WebSearch/WebFetch，中英文源并用，每条关键论断内联 URL）· 确定度图例：**高** = 官方文档/一手源码/厂商权威材料；**中** = 二手转述/社区实测/媒体报道；**低** = 推断或单一来源。
> 本报告在 PM 系列中的分工：PM-00 为总览。本篇（第 6 路）覆盖 ① PC/桌面线电源演化（Windows/macOS/Linux）② 系统级电源协调中枢的收敛形态（状态机/投票/hint/归因四件套）③ 功耗归因与计量体系 ④ AI 预测省电实绩，收官给 BerryOS 中央电源服务画最小职责面。渲染/GUI 侧省电 → `research/GUI-06-低功耗渲染策略.md` + ADR #53；内核调度机制面 → `research/SCHED-01` + ADR #42/#58；本篇不重复其内容。

---

## §1 演化/机制主线：把「整个系统」的电源拢起来的那条线

### 1.1 Windows 线：S3 → Modern Standby（S0ix）强推史与翻车

主线一句话：**微软把手机电源模型强推进 PC，用十年时间补「整个桌面软件生态没有待机纪律」的课**。

1. **模型替换**：Windows 8.1 Connected Standby → Windows 10 Modern Standby。待机不再是 S3「全机断电只留内存自刷新」，而是 **S0 低功率闲置**——系统活着、监听网络包与输入，只以「短促受控突发」放行软件活动；微软自述「概念上等价于 S3，外加周期性放行增值软件活动的便利」（[MS: Modern Standby](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby)）【高】。
2. **强推手段**：官方明言 S3 与 Modern Standby **不能靠 BIOS 设置切换，不经完整重装系统不支持换电源模型**（同上）【高】——OEM 新平台随之大量只带 S0ix，用户失去退路。
3. **模型自身的修正**：Win8.1 Connected Standby 时代，系统在待机中**最少每 30 秒**被迫从闲置态拉起做内核维护（典型数百毫秒、不可配置）；Win10 Modern Standby 删除了这个固定心跳（同上）【高】——连微软自己都在跟「周期性白醒」斗争，这是 BerryOS tickless 铁律（空闲不白醒）的同构证据：**固定心跳是待机质量的头号敌人，哪怕是内核自己的**。
4. **田野翻车**：待机耗电 7%/小时（ASUS G14 用户实测帖）、10–15%/晚（HP 社区）、**背包发烫**（系统在散热受阻的包里继续跑后台）——社区自救三板斧 = 注册表强开 S3、禁唤醒设备、改用休眠（[r/ZephyrusG14](https://www.reddit.com/r/ZephyrusG14/comments/16u1zdv/modern_standby_proper_fix_for_all_users)）【中】。
5. **唤醒风暴**：待机会话被邮件/网络/更新活动反复拉回 active。微软官方 SleepStudy「常见问题示例」页直接列了高频 email 活动、高频网络活动、明显元凶高耗电三类（[MS: SleepStudy common problem examples](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-sleepstudy-common-problem-examples)）【高】。
6. **DRIPS（Deepest Runtime Idle Platform State）**：Modern Standby 里平台能达到的最深闲置态；**DDRIPS** = OS 主动指挥各设备进低功率态（directed power management）。SleepStudy 按会话报 DRIPS 占比，健康标准逼近 100%（红色档 80–94%）（[MS: Directed power management](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/directed-power-management)、[MS: SleepStudy](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-sleepstudy)）【高】——微软把「待机质量」压成了一个可归因的单一 KPI。
7. **activator 票据制**：待机态内能阻止系统回闲置态的组件必须**向 power manager 注册**，活动时长受控且可被 SleepStudy 观测（[MS: Modern Standby](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby)）【高】。
8. **DAM（Desktop Activity Moderator）**：进 Modern Standby 时**冻结 Win32 桌面进程、重节流系统服务**，只有 UWP 后台任务照跑（[MS Compatibility Cookbook: DAM](https://learn.microsoft.com/en-us/windows/compatibility/desktop-activity-moderator)）【高】；运维界甚至流传禁用偏方 `SC CONFIG dam START= DISABLED`（[Rackspace 文档](https://docs.rackspace.com/docs/disable-desktop-activity-moderator)）【中】。本质 = 两代应用模型并存的补丁：旧桌面应用从不预期「睡眠中系统还活着」，索性整体挂起。
9. **timer resolution 三十年还债**（本域最重要教训，细表见 §2.2）：官方文档原文承认「高分辨率会**阻止 CPU 电源管理系统进入省电模式**」（[MS: timeBeginPeriod](https://learn.microsoft.com/en-us/windows/win32/api/timeapi/nf-timeapi-timebeginperiod)）【高】。
10. **Power Throttling → EcoQoS**：2017 年 Win10 1709 由**系统猜**哪些是后台进程（Intel Speed Shift 硬件调度类压频，官方实验重度场景 CPU 功耗省最多 11%【中，官方博文现链路已迁移】）；2021 年 EcoQoS 改为开发者**自报** QoS 档，后台工作跑低频/能效核，Win11 任务管理器「效率模式」把它暴露给用户（[MS DevBlogs: Introducing EcoQoS](https://devblogs.microsoft.com/performance-diagnostics/introducing-ecoqos)、[TweakTown 综述](https://www.tweaktown.com/guides/11436/windows-11-is-secretly-throttling-your-apps-heres-how-to-catch-it/index.html)）【高/中】。翻车面：EcoQoS 被自动套到 Docker 等时间敏感后台任务上致定时节奏变形（社区报告，[TweakTown 同文](https://www.tweaktown.com/guides/11436/windows-11-is-secretly-throttling-your-apps-heres-how-to-catch-it/index.html)）【中】。

### 1.2 macOS 线：一套 2013 年落齐、至今仍在加码的「文明待机」

OS X 10.9 Mavericks（2013）一次落下整套（[Apple: Power Efficiency in OS X 技术总览 PDF](https://www.apple.com/media/us/osx/2013/docs/OSX_Power_Efficiency_Technology_Overview.pdf)）【高】：

| 机制 | 语义 | 备注 |
|---|---|---|
| **App Nap** | 应用「完全被遮挡 + 无音频 + 无未决重要工作」→ 系统对其**定时器降频 + I/O 节流 + 网络降速**；逃脱靠持 power assertion（IOPmlib） | 条件/豁免面见 Apple PDF 与 [Lazarus wiki](https://wiki.lazarus.freepascal.org/macOS_App_Nap)【高/中】 |
| **Timer Coalescing** | 系统级把各进程分散的定时器唤醒**对齐成簇**，CPU 成段深睡而非碎片化浅睡 | 与 App Nap 互补：一个治「谁该睡」，一个治「大家一起睡」【高】 |
| **powerd** | 常驻守护进程，持有电源状态迁移与策略（睡眠/dark wake/LPM），`pmset` 为其用户面 | 内部实现少公开，dark wake 取证靠 `pmset` 日志【中】（[ksred: 追查 dark wake 掉电](https://www.ksred.com/building-a-macos-power-monitor-in-go-solving-the-sleep-drain-mystery)） |
| **Power Nap** | 睡眠中周期性 dark wake 干活：邮件/iCloud/Time Machine/软件更新——macOS 版「待机白名单活动」，也是 MacBook 睡眠掉电头号投诉源 | [r/MacOS 讨论](https://www.reddit.com/r/MacOS/comments/14aw6j1/is_wake_for_network_access_same_as_power_nap/)、[Medium 排查](https://medium.com/macoclock/stop-your-macbook-battery-from-draining-overnight-866355cfe5bd)【中】 |
| **低电量模式** | macOS 12+：降系统时钟频率 + 降屏亮度省电；15.1 起附带降噪 | 官方页 [About Power Modes on your Mac](https://support.apple.com/en-us/101613)【高】 |
| **NSBackgroundActivityScheduler** | 应用把维护类任务交给系统，系统按**能耗/热状态/CPU 余量**择机执行——「任务级」而非「进程级」省电接口 | [Apple 文档](https://developer.apple.com/documentation/foundation/nsbackgroundactivityscheduler)【高】；现代实现在 DAS（Duet Activity Scheduler）/CTS【中】（[mjtsai 评述](https://mjtsai.com/blog/2015/09/08/nsbackgroundactivityscheduler)） |
| **powermetrics** | 官方计量工具：CPU/GPU/ANE 子系统功耗估计 + per-process CPU/GPU 归因（`--show-process-gpu`） | [Firefox 文档的用法与字段解释](https://firefox-source-docs.mozilla.org/performance/powermetrics.html)、[ss64 手册页](https://ss64.com/mac/powermetrics.html)【高】 |

macOS 线的要点：**进程级（App Nap）→ 任务级（NSBackgroundActivityScheduler）→ 系统级（powerd 状态机 + LPM）三层接口同时开放**，且都建立在一个「系统择机」中枢上，应用从不说「现在就跑」。

### 1.3 Linux 桌面简线：谁拥有电源决策？

- **答案：systemd-logind 仲裁，应用只能 delay/block，特权配置最终拍板**。inhibitor lock 域 = `sleep`/`shutdown`/`idle`/`handle-power-key`/`handle-suspend-key`/`handle-hibernate-key`/`handle-lid-switch`，模式分 `block`（阻止）与 `delay`（延迟有限时间）；应用走 D-Bus `org.freedesktop.login1.Manager.Inhibit` 拿锁，`systemd-inhibit --list` 可枚举谁锁着睡眠（[freedesktop 官方 man 页](https://www.freedesktop.org/software/systemd/man/systemd-inhibit.html)、[Arch man 页](https://man.archlinux.org/man/systemd-inhibit.1.en)）【高】。
- 桌面环境（GNOME/KDE）是 logind 的**特权客户端**：盖盖子等策略在 `logind.conf`（`HandleLidSwitch=`）配置，用户/发行版可覆盖（[NixOS Wiki: systemd/logind](https://wiki.nixos.org/wiki/Systemd/logind)）【高】。
- 内核面：wakeup source 框架按源累计 `event_count`/`wakeup_count`/`prevent_sleep_time`，sysfs/debugfs 可查（`/sys/power/wakeup_sources`），suspend 被谁打断可归因；但**真实硬件唤醒原因常要平台机制**（ACPI `/proc/acpi/wakeup`、ARM 侧 PSCI/TF-A→GPC 寄存器）才查得到（[kernel-internals.org: System Suspend](https://kernel-internals.org/power/suspend)、[Microchip 检查 wake-up sources](https://support.microchip.com/s/article/MPU-s--Checking-Linux-system-wake-up-sources)、[NXP i.MX8 唤醒原因](https://community.nxp.com/t5/i-MX-Processors/i-MX8MM-Detect-Wakeup-Cause/td-p/1190811)）【高/中】。
- 结构性问题：**Linux 没有单一电源中枢**——logind 管 sleep 仲裁、UPower 管电池计量、各 DE 自带电源守护、内核 governor/DVFS 独立运转；「谁拥有电源决策」在 Linux 上的答案就是「分层共治，logind 只握住 sleep/shutdown 那一小块」【中】。

### 1.4 hint 接口演化线：从 vendor 命令到「目标时长会话」

Android 把这条线走得最完整，也是 BerryOS 最该照抄的形状：

1. **Power HAL hints（旧）**：`powerHint()` 传厂商私有提示（如交互开始、抬频），实现方自行调 cpufreq governor——**命令式、不透明、不可归因**（[AOSP power/1.0/IPower.hal](https://android.googlesource.com/platform/hardware/interfaces/+/master/power/1.0/IPower.hal)）【高（接口存在）/中（「被取代」判断）】。
2. **ADPF Performance Hint API（新）**：应用建 **hint session**，报「这帧目标时长 + 实际耗时」，系统据此调频/选核；配套 thermal API 实时读热状态（[source.android.com: Performance Hint API](https://source.android.com/docs/core/perf/performance-hint-api)、[ARM 开发者博客](https://developer.arm.com/community/arm-community-blogs/b/mobile-graphics-and-gaming-blog/posts/getting-started-with-adpf)）【高】。
3. **语义跃迁**：旧 hint 是「请给我性能」（无反馈闭环）；ADPF 是「我承诺 N ms 完成并回报实际值」（有闭环、系统可学习该给多少）。这与 macOS QoS/EcoQoS 的「自报档位」同一方向：**hint 从命令变成带反馈的契约**。

---

## §2 关键事实与证据

### 2.1 收敛总表：四件套是不是各 OS 的共同形态？——是【确定度：高（四家并置互证）】

| 职责 | Windows | macOS | Android | Linux 桌面 |
|---|---|---|---|---|
| **状态机** | S0 active/idle 两态 + screen-off/sleep 分段，DRIPS 为深态（[MS](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby)） | powerd：active/sleep(dark wake)/LPM（[pmset 取证](https://www.ksred.com/building-a-macos-power-monitor-in-go-solving-the-sleep-drain-mystery)）【中】 | Doze（静止+屏灭+电池→深闲置，分级 maintenance/full）（[developer.android.com](https://developer.android.com/topic/performance/appstandby)）【高】 | logind idle→sleep 治理（[man 页](https://www.freedesktop.org/software/systemd/man/systemd-inhibit.html)） |
| **投票/仲裁** | activator 注册制：注册者才能阻止回 idle，时长受控可观测（[MS](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby)） | IOPMAssertion（App Nap 豁免同机制）（[Apple PDF](https://www.apple.com/media/us/osx/2013/docs/OSX_Power_Efficiency_Technology_Overview.pdf)） | wakelock/wakeup source 汇聚 + 桶位准入（[kernel-internals](https://kernel-internals.org/power/suspend)、[appstandby](https://developer.android.com/topic/performance/appstandby)） | inhibitor lock：block/delay 两档（[man 页](https://www.freedesktop.org/software/systemd/man/systemd-inhibit.html)） |
| **hint** | EcoQoS 自报档 + Power Throttling 系统猜（[DevBlogs](https://devblogs.microsoft.com/performance-diagnostics/introducing-ecoqos)） | QoS 档位 + NSBackgroundActivityScheduler 交任务（[Apple 文档](https://developer.apple.com/documentation/foundation/nsbackgroundactivityscheduler)） | Power HAL hints（旧）→ ADPF 目标时长会话（新）（[source.android.com](https://source.android.com/docs/core/perf/performance-hint-api)） | cpufreq governor 参数（无统一 app 面）【中】 |
| **归因** | SleepStudy：DRIPS 占比 + 元凶模块（[MS](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-sleepstudy)） | powermetrics：子系统功耗 + per-process（[Firefox 文档](https://firefox-source-docs.mozilla.org/performance/powermetrics.html)） | batterystats/ODPM（见 §3） | wakeup_sources 计数（[kernel-internals](https://kernel-internals.org/power/suspend)） |

**结论**：四家独立演化，收敛到同一形态——**中央电源服务持状态机，进深态靠投票汇聚（一票否决或限期拖延），性能期望靠 hint 上报（带反馈），一切以归因为闭环验收**。差异只在四件各自的「拥有者」和粗细。

### 2.2 Windows timer resolution 三段演化【确定度：高（官方文档原文）】

| 阶段 | 语义 | 教训 |
|---|---|---|
| < Win10 2004 | **全局**：所有进程取最低值（最高精度）——「一个进程要 1ms，全系统 1ms」 | 后台一个标签页即可让全系统定时器风暴；官方承认阻止 CPU 进省电模式（[MS 文档](https://learn.microsoft.com/en-us/windows/win32/api/timeapi/nf-timeapi-timebeginperiod)） |
| Win10 2004+ | **per-process**：只影响调用者；未调用者不保证高于默认 | 依赖旧全局语义的工具（给游戏全局拉精度）全灭；社区记为「The Great Rule Change」（[RandomASCII/Bruce Dawson](https://randomascii.wordpress.com/2020/10/04/windows-timer-resolution-the-great-rule-change)） |
| Win11+ | **按窗口可见性**：窗口完全遮挡/最小化/不可见不可闻 → 不保证高于默认精度 | 游戏/媒体社区实测「后台拿不到高精度」（[William's Blog](https://williamsmale.com/blog/tech/windows-11-lying-to-games)）【中】 |

### 2.3 Android 桶位：预测式准入的现役实现【确定度：高（官方文档全文取回）】

- 五桶 + 特殊桶：active / working_set / frequent / rare / restricted(+never)；桶决定 jobs 频率、alarms 能力、联网窗口；**厂商可自写分桶器**（[developer.android.com: App Standby Buckets](https://developer.android.com/topic/performance/appstandby)）。
- ML 分桶是「预测式」而非「回顾式」：刚用过的 app 也可能因预测「几小时内不会再用」被压进 rare 桶（同上）。
- restricted 桶（Android 12+）硬配额：**每天一次 10 分钟批处理会话跑 jobs（且必须与别的 job 拼车）+ 每天 1 个 alarm**；豁免面（VPN/电话/用户指定不受限等）显式列举（同上）。
- 桶位只在使用电池时生效，充电时不限制（同上）——**「省电约束」与「电源态」显式联动**。

### 2.4 闲置分级对照：各家的「深态」长什么样【确定度：高（逐行有源）/中（macOS 内部）】

| 系统 | 活跃 | 中间态（屏灭但系统活） | 深态 | 证据 |
|---|---|---|---|---|
| Windows | S0 active（屏亮） | Modern Standby screen-off/sleep 段：受控突发 + activator 票据 + 网络协议 offload | DRIPS（目标占比 ≈100%） | [MS](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby)、[SleepStudy](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-sleepstudy) |
| macOS | awake | dark wake（Power Nap 周期干活）/ App Nap（应用级假寐） | 睡眠态（powerd 管理）【中】 | [ksred](https://www.ksred.com/building-a-macos-power-monitor-in-go-solving-the-sleep-drain-mystery)、[Apple PDF](https://www.apple.com/media/us/osx/2013/docs/OSX_Power_Efficiency_Technology_Overview.pdf) |
| Android | active（Doze 豁免面除外） | Doze maintenance 窗口（批处理放行） | Doze full + 桶位限制叠加 | [appstandby](https://developer.android.com/topic/performance/appstandby)（含 Doze 链接） |
| Linux | on | s2idle（现代发行版常见默认） | suspend-to-RAM / hibernate | [kernel-internals](https://kernel-internals.org/power/suspend) |

共同形态：**深态之前普遍存在一个「系统还活着的低活跃档」**（Windows sleep 段 / dark wake / Doze maintenance）——后台活动被收编进这个档的受控窗口，而不是自由散跑。BerryOS 的 DOZE 设计原型即此档。

---

## §3 功耗归因与计量体系

### 3.1 三层计量：现役系统的分层【确定度：高】

| 层 | 形态 | 代表 | 证据 |
|---|---|---|---|
| L1 模型估算 | 每组件电流参数表 × 使用时长 → per-app 估算值 | Android `power_profile.xml` + batterystats → Battery Historian 可视化 | 官方指南（[developer.android.com](https://developer.android.com/topic/performance/power/setup-battery-historian)）；**数字是模型估算不是实测**【高】 |
| L2 轨级实测 | 硬件电表按电源轨（rail）实测功耗；一条轨供 1..n 个组件 | ODPM（Android 10+，需硬件支持；Pixel 6 约 12 条轨） | [Power Profiler 文档](https://developer.android.com/studio/profile/power-profiler)、[Perfetto 数据源](https://perfetto.dev/docs/data-sources/battery-counters)【高】 |
| L3 时间记账 | 唤醒次数/运行时长/持有 wakelock/assertion 时长按 UID 汇总 | batterystats、wakeup_sources、SleepStudy 会话、powermetrics per-process | 同上各源 |

### 3.2 per-app 归因的真正难点：共享外设怎么分摊【确定度：高（结构性事实）】

- ODPM 计量在**轨级**：一条 rail 供多个组件、一个组件被多个 app 共用（GPU/基带/内存/射频），**官方口径即「rail 级非 app 级」**，per-app 数字必须再叠 L1 模型分摊（[Power Profiler 文档](https://developer.android.com/studio/profile/power-profiler)、[Perfetto 文档](https://perfetto.dev/docs/data-sources/battery-counters)）。
- powermetrics 同理：子系统功耗是整机/SoC 级估计，per-process 列是「CPU ms/s + GPU 占比」这类**时间记账**而非电表分摊（[Firefox 文档](https://firefox-source-docs.mozilla.org/performance/powermetrics.html)）。
- 结论：**没有任何现役系统能给出「这个 app 昨晚耗了几毫安时」的实测数**；能给的是「轨级实测 + 时间记账 + 模型分摊」的合成估计。归因体系 day-one 就要按这个分层设计，别许诺做不到的精度。

### 3.3 计量-治理的闭环：A/B 是纪律，不是工具

- 标准工作流（Android 侧样板）：`adb shell dumpsys batterystats` 导出 → Battery Historian 渲染时间线（电量事件/唤醒/wakelock 按 UID）→ 定位异常 app（[官方指南](https://developer.android.com/topic/performance/power/setup-battery-historian)）【高】。
- Google 官方方法论：功能功耗验证用 Power Profiler 做对照组 A/B（分屏对照跑同场景），并用固定基线版本减少漂移（[Android Developers Blog 2024](https://android-developers.googleblog.com/2024/04/how-to-effectively-ab-test-power-consumption-for-your-android-app-features.html)）【高】。
- 工业实证：游戏厂商用该流程把功耗降 9.68%（[Android Developers Blog 案例文](https://android-developers.googleblog.com/2024/04/how-to-effectively-ab-test-power-consumption-for-your-android-app-features.html) 同系列）【中】。
- Windows 侧闭环 = SleepStudy：DRIPS 占比 < 阈值 → 列出 activator/模块元凶 → 修（[MS: SleepStudy](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-sleepstudy)）【高】。**归因必须能点名到「谁把我从深态拉出来」，否则治理无抓手**。

---

## §4 AI 预测省电：现役实绩与机会面

### 4.1 现役系统把「智能」用在电源上的实绩清单【确定度：高（机制）/中（数字口径）】

| 用例 | 机制类型 | 输入 → 输出 | 实绩口径 |
|---|---|---|---|
| Adaptive Battery（Android 9，2018，DeepMind 合作） | 端侧深度模型预测 app 使用 | 使用历史 → 桶位（预测式分桶，刚用过的 app 也可能进 rare 桶） | 「后台活动显著下降」（官方博客不贴数字）；「CPU 唤醒最多减 30%」出自 I/O 演讲转述（[DeepMind](https://deepmind.google/blog/deepmind-meet-android)【高】、[Android Authority](https://www.androidauthority.com/adaptive-battery-google-android-863032)【中】、[Wired 追问](https://www.wired.com/story/android-pie-adaptive-battery-deepmind-google)【中】） |
| Doze（Android 6.0+） | **规则式**传感融合 | 静止（运动传感器）+ 屏灭 + 电池 → 深闲置分级（maintenance/full） | 官方文档（[appstandby 页关联 Doze](https://developer.android.com/topic/performance/appstandby)）【高】 |
| Timer Coalescing（macOS） | **确定性**对齐 | 定时器集合 → 成簇唤醒 | Apple PDF（[技术总览](https://www.apple.com/media/us/osx/2013/docs/OSX_Power_Efficiency_Technology_Overview.pdf)）【高】 |

口径校准：现役「AI in 电源」真正上量的只有 Adaptive Battery 一件，且其出口是**分桶准入**而非直接控频；Doze/Coalescing 都是规则与确定性算法。别把「AI 省电」的现役水平想象得比实际高。

### 4.2 对 BerryOS「AI 原生调度」身份支柱的机会面

1. 现役 ML 只做**准入/降档预测**（谁进 rare 桶、谁被 App Nap），从不直接碰频率/定时器——决策出口永远是既有治理机制。这正好是 BerryOS 该守的形状：**模型出票，机制面执行，带内可审计**（对齐 ADR #42 两段式与「上层只投票不直接设频」纪律）。
2. 现役空白一：唤醒历史 → 预测「下一次用户交互窗口」，把维护任务批处理进窗口（Timer Coalescing 的 ML 版——把「对齐定时器」升级为「对齐用户」）。
3. 现役空白二：归因表在线学习 → 「异常耗电 app」自动降档建议 + 给用户可解释理由（SleepStudy 只诊断不决策）。
4. 结构优势：自有 ABI day-one，**桶位/hint/assertion 可以进应用契约与 manifest**，不似 Android 要兼容十年旧 app（DAM 教训的反面）。

---

## §5 教训与历史包袱清单

| # | 教训 | 证据 | 对 BerryOS 的直接含义 |
|---|---|---|---|
| 1 | 待机态不是二值开关：S0ix 把「睡眠」变「低功率活跃」后，**一切后台软件行为都成了待机耗电变量** | [MS Modern Standby](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby) + 翻车实录【中】 | 深闲置态的「谁能跑」必须 day-one 白名单制（票据+配额） |
| 2 | 无退路的模型切换 = 用户灾难：S3 不能切回，强推十年骂名未消 | 同上官方「不能切换」原文【高】 | 电源模型协议位要 add-only、可降级运行（旧应用在新策略下仍正确，只是不省电） |
| 3 | 全局定时器精度是全系统毒药：一个进程的 1ms 需求烧全系统的电，官方花了 20 年才拆成 per-process、又加可见性收紧 | [timeBeginPeriod](https://learn.microsoft.com/en-us/windows/win32/api/timeapi/nf-timeapi-timebeginperiod)【高】 | 定时器精度/slack **day-one 就是 per-thread 属性**，永不做全局开关 |
| 4 | 「系统猜哪些可慢」会误伤时间敏感任务（Power Throttling/EcoQoS 误伤 Docker） | [TweakTown](https://www.tweaktown.com/guides/11436/windows-11-is-secretly-throttling-your-apps-heres-how-to-catch-it/index.html)【中】 | 降档必须可自报豁免（音频播放/下载持 assertion 即免） |
| 5 | 旧应用模型靠挂起补课（DAM 冻结 Win32）→ 两代语义分裂 | [DAM Cookbook](https://learn.microsoft.com/en-us/windows/compatibility/desktop-activity-moderator)【高】 | 无历史包袱：全应用一套契约（挂起语义对所有 app 一致） |
| 6 | 没有归因的省电机制没法治理：SleepStudy 能点名元凶是 Windows 补救的起点 | [SleepStudy](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-sleepstudy)【高】 | 归因面与机制面同刀落地，不做事后补丁 |
| 7 | per-app 功耗数字本质是估算：轨级实测+模型分摊是物理上限 | [ODPM 文档](https://developer.android.com/studio/profile/power-profiler)【高】 | 遥测叙事别许诺「per-app 毫安时实测」 |
| 8 | 生态碎片化的 Linux 没人拥有全局：logind 只握 sleep 仲裁一块 | [systemd-inhibit man](https://www.freedesktop.org/software/systemd/man/systemd-inhibit.html)【高】 | 微内核下恰好可以 day-one 指定单一中枢拥有全链 |
| 9 | ML 出路在「预测准入」而非「直接控频」：现役全部收敛于此 | [DeepMind](https://deepmind.google/blog/deepmind-meet-android) + [appstandby](https://developer.android.com/topic/performance/appstandby)【高】 | AI 原生调度 = 模型出票进 berry_intent 六档，带内执行 |

---

## §6 BerryOS 启示：中央电源服务（暂名 berry-power）最小职责面

### 6.1 定位（微内核落法）

中枢 = **用户态常驻服务**，靠 IPC（同步 channel）+ 能力与内核机制面协作；内核只留机制（tickless 空闲、wakeup 记账、intent/EPP 通道——ADR #20/#42/#58 已备），**策略全在用户态**（「机制在内核、策略可插拔」原则的直接延伸）。四件套即最小职责面：

1. **状态机**（票面）：`ACTIVE → DOZE（屏灭低活跃：显示侧即 ADR #53 的 DIM/AOD 域）→ SUSPEND（深闲置，全系统断电至内存）`。迁移由 berry-power 仲裁，**power·mode op（NORMAL/DIM/AOD/OFF）是显示服务对状态机的映射，不是第二台状态机**——避免 Windows S3/S0ix 双模型分裂教训（§5-2）。
2. **投票/仲裁**（票据）：`power_assert {kind: block|delay, reason, lease_ns}`——logind 的 block/delay 两档语义 + Windows activator 的「注册者才能阻止回深态 + 时长受控 + 可枚举观测」。配套：
   - `berry-power query` 全量枚举在持票据（谁、为何、还剩多久）——SleepStudy「点名元凶」能力 day-one 化；
   - 票据必须带粗粒度 reason 枚举（audio/download/location/sync/ui…），进遥测与 smoke；
   - DOZE 内后台活动**无票即泄漏**：类 restricted 桶硬配额（如「维护类 jobs 每天 1 次 10 分钟批窗口」，直接抄 Android 数值起步）。
3. **hint 面**（带内契约）：全系统统一 ADPF 形态——**目标时长会话** `{target_ns, actual_ns}`（ADR #53 PM 共享环已给 GUI 帧节拍这个形状，泛化到全场景）；禁止 Power HAL 式「抬频命令」接口。应用侧调度意图唯一词汇表仍是 berry_intent 六档（ADR #58）；hint 会话是「承诺+回报」的反馈闭环，与 intent 投票正交：intent 说「我是谁」，hint 说「我干得怎样」。
4. **归因/计量**（三层）：
   - L1 模型参数表（类 power_profile：每组件电流/每次唤醒能耗）进 berry-manifest，构建期烘入；
   - L2 轨级计量**接口先立、实现后补**（QEMU 无硬件轨；协议位预留 rail→组件映射表，真机接 ODPM 同型数据）；
   - L3 时间记账：内核按持有者记每 IRQ/定时器唤醒次数与运行时长（wakeup_sources 同型表），berry-power 按 UID/服务汇总成 batterystats 同型报表；
   - smoke 断言分两级：**待机泄漏 = 0**（DOZE/SUSPEND 态无票据唤醒计数为 0——ADR #53「IDLE 唤醒计数 0 + icount≈0」断言的 per-source 强化版）+ **归因守恒**（记账表覆盖全部唤醒源，无「来源不明」项）。

**berry-power 通道 op 草案**（同步 channel，C-ABI，与 ~35 syscall 的「机制在内核」边界一致——服务面 op 不占 syscall 预算）：

```
POW_OP_SET_MODE      // 请求状态机迁移（带 reason；显示服务的 power·mode op 在此归一）
POW_OP_ASSERT        // 持票 {kind: block|delay, reason, lease_ns} → 票据句柄
POW_OP_RELEASE       // 还票（断线自动回收——失败隔离：持有者崩溃不泄漏票）
POW_OP_QUERY         // 枚举在持票据 + 现态 + 各态驻留时长（SleepStudy 式可观测）
POW_OP_HINT_SESSION  // {target_ns} 建会话 → {actual_ns} 回报（#53 PM 共享环的全系统泛化）
```

票据句柄 = 能力（能力即权限）：只能还/续自己的票；DOZE 内无票周期活动由内核记账面标记为泄漏，遥测红灯。

**遥测最小字段集**：

- per-source（wakeup_sources 同型）：唤醒计数、累计 prevent_sleep_ns；
- per-UID（batterystats 同型）：CPU 时间、唤醒次数、票据持有时长、无票活动计数（泄漏探测器）；
- per-session（DRIPS 同型 KPI）：DOZE/SUSPEND 驻留占比、进深态失败次数 + 最后否决票的 reason——归因直接点名「谁把我从深态拉出来」。

### 6.2 定时器纪律（Windows 三十年教训的 day-one 免疫）

- 定时器 slack 是 **per-thread 属性**，默认宽松（tickless 友好）；精确档（如 ≤1ms）需显式申请；
- 精确档**随前台/可见性自动降级**（Win11 按遮挡收紧同型）：窗口不可见或进程入 DOZE → 回默认松档；持 assertion 的豁免面显式（音频/媒体/下载）；
- berry-libc 不提供任何「全局定时器精度」入口——协议面根本不存在这个能力，而不是存在但劝阻。

### 6.3 与现有 ADR 挂接

| 现有资产 | 本报告结论的挂法 |
|---|---|
| ADR #53 power·mode op / PM 共享环 / 能耗代理断言 | 显示侧协议位 = berry-power 状态机在显示域的投影；{target_ns,actual_ns} 泛化为全系统 hint 会话；能耗断言升 per-source |
| ADR #42/#58 intent 六档带内 | hint 会话与 intent 并存正交；AI 预测输出 = 桶位/建议 intent 降档，仍走带内（模型出票、机制面执行） |
| ADR #21 带内接口对 app 可达性 | power_assert 与 hint 会话需同刀进 SDK 透传三口（pthread_attr/spawn/chan opt） |
| sched-1.1 饥饿界/admission | 桶位准入（rare/restricted 硬配额）与调度 admission 复用同一准入语义，不另立第二套 |

### 6.4 反面清单（别做的事）

- 别做「全局定时器精度/全局帧率/全局频率」任何形式的全局开关（§5-3）；
- 别让 GUI/网络/FS 各自为政协商电源态——状态机唯一持有者是 berry-power，其余服务只投票（§5-8）；
- 别承诺 per-app 实测毫安时——三层合成估算是物理上限，叙事按估算口径（§5-7）；
- 别给 AI 直接设频设帧率的通道——现役 ML 实绩全部收敛在「预测准入」，照抄（§5-9）；
- 别把 DOZE 白名单做成事后补丁（DAM 式挂起旧应用）——挂起/降档语义对全部 app 一套（§5-5）。

### 6.5 落地刀序建议（每刀独立可 smoke 验证）

| 刀 | 内容 | 验收面 |
|---|---|---|
| 1 | 内核 wakeup 记账面：per-source 计数 + 持有者（UID/服务）归因 + 遥测导出 | 归因守恒断言（Σ 记账 = 内核唤醒总数）；机制未动先可见 |
| 2 | berry-power 骨架：状态机 + assert/release/query op（DOZE 先落「屏灭即批处理窗口」最小语义） | query 可枚举全部票据；断线回收测试 |
| 3 | 定时器 per-thread slack + 可见性/前台降级（berry-libc/berry-sys 同刀，SDK 透传三口挂 ADR #21 议程） | 后台进程拿不到精确档的负向测试 |
| 4 | hint 会话泛化（#53 环 → 全系统）+ 桶位准入与 sched admission（sched-1.1）合流 | rare 桶硬配额（10 min/日批窗口）生效 |
| 5 | L1 模型参数表进 berry-manifest + 功耗 A/B smoke 配方（固定基线对照） | 对照组报告产出 |

刀 1 与刀 3 不依赖 berry-power 存在，可先行；刀序的原则 = **归因先于治理**（没有记账面的省电机制无法验收）。

---

## §7 引用源列表

**Windows**
- [MS Learn: Modern Standby](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby)（状态机/activator/S3 不可切换，全文取回）
- [MS Learn: SleepStudy](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-sleepstudy) · [常见问题示例](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-sleepstudy-common-problem-examples) · [Directed power management（DRIPS/DDRIPS）](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/directed-power-management)
- [MS Compatibility Cookbook: Desktop Activity Moderator](https://learn.microsoft.com/en-us/windows/compatibility/desktop-activity-moderator) · [Rackspace: Disable DAM](https://docs.rackspace.com/docs/disable-desktop-activity-moderator)
- [MS Learn: timeBeginPeriod](https://learn.microsoft.com/en-us/windows/win32/api/timeapi/nf-timeapi-timebeginperiod)（全文取回）· [RandomASCII: The Great Rule Change](https://randomascii.wordpress.com/2020/10/04/windows-timer-resolution-the-great-rule-change) · [William's Blog: Win11 与游戏](https://williamsmale.com/blog/tech/windows-11-lying-to-games)
- [MS DevBlogs: Introducing EcoQoS](https://devblogs.microsoft.com/performance-diagnostics/introducing-ecoqos) · [TweakTown: Efficiency Mode 实测](https://www.tweaktown.com/guides/11436/windows-11-is-secretly-throttling-your-apps-heres-how-to-catch-it/index.html)
- [r/ZephyrusG14: Modern Standby 修复帖](https://www.reddit.com/r/ZephyrusG14/comments/16u1zdv/modern_standby_proper_fix_for_all_users)（田野翻车）

**macOS**
- [Apple: Power Efficiency in OS X 技术总览 PDF](https://www.apple.com/media/us/osx/2013/docs/OSX_Power_Efficiency_Technology_Overview.pdf)（App Nap/Timer Coalescing 一手材料）
- [Apple 文档: NSBackgroundActivityScheduler](https://developer.apple.com/documentation/foundation/nsbackgroundactivityscheduler) · [Apple 支持: About Power Modes](https://support.apple.com/en-us/101613)
- [Lazarus wiki: macOS App Nap](https://wiki.lazarus.freepascal.org/macOS_App_Nap) · [mjtsai: NSBackgroundActivityScheduler 评述](https://mjtsai.com/blog/2015/09/08/nsbackgroundactivityscheduler)
- [r/MacOS: Wake for network access vs Power Nap](https://www.reddit.com/r/MacOS/comments/14aw6j1/is_wake_for_network_access_same_as_power_nap/) · [Medium: 睡眠掉电排查](https://medium.com/macoclock/stop-your-macbook-battery-from-draining-overnight-866355cfe5bd) · [ksred: dark wake 取证](https://www.ksred.com/building-a-macos-power-monitor-in-go-solving-the-sleep-drain-mystery)
- [Firefox 文档: powermetrics 用法](https://firefox-source-docs.mozilla.org/performance/powermetrics.html) · [ss64: powermetrics 手册](https://ss64.com/mac/powermetrics.html)

**Linux**
- [freedesktop: systemd-inhibit(1)](https://www.freedesktop.org/software/systemd/man/systemd-inhibit.html) · [Arch: systemd-inhibit man](https://man.archlinux.org/man/systemd-inhibit.1.en) · [NixOS Wiki: systemd/logind](https://wiki.nixos.org/wiki/Systemd/logind)
- [kernel-internals.org: System Suspend](https://kernel-internals.org/power/suspend) · [Microchip: 检查 wake-up sources](https://support.microchip.com/s/article/MPU-s--Checking-Linux-system-wake-up-sources) · [NXP: i.MX8 唤醒原因](https://community.nxp.com/t5/i-MX-Processors/i-MX8MM-Detect-Wakeup-Cause/td-p/1190811)

**Android/归因/AI**
- [source.android.com: Performance Hint API（ADPF）](https://source.android.com/docs/core/perf/performance-hint-api) · [AOSP: power/1.0/IPower.hal](https://android.googlesource.com/platform/hardware/interfaces/+/master/power/1.0/IPower.hal) · [ARM: Getting started with ADPF](https://developer.arm.com/community/arm-community-blogs/b/mobile-graphics-and-gaming-blog/posts/getting-started-with-adpf)
- [developer.android.com: App Standby Buckets](https://developer.android.com/topic/performance/appstandby)（全文取回）· [Batterystats/Battery Historian](https://developer.android.com/topic/performance/power/setup-battery-historian) · [Power Profiler（ODPM）](https://developer.android.com/studio/profile/power-profiler) · [Perfetto: power data sources](https://perfetto.dev/docs/data-sources/battery-counters)
- [Android Developers Blog 2024: 功耗 A/B 方法论](https://android-developers.googleblog.com/2024/04/how-to-effectively-ab-test-power-consumption-for-your-android-app-features.html)
- [DeepMind: DeepMind, meet Android](https://deepmind.google/blog/deepmind-meet-android)（全文取回）· [Android Authority: Adaptive Battery 与 30% 口径](https://www.androidauthority.com/adaptive-battery-google-android-863032) · [Wired: Adaptive Battery 深度报道](https://www.wired.com/story/android-pie-adaptive-battery-deepmind-google) · [Google 支持: Adaptive Battery 开启建议](https://support.google.com/pixelphone/answer/7015477?hl=en)
