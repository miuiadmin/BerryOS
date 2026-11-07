# PM-03 Linux 电源机制全景与微内核归属（v1.0）

> 调研日期：2026-11-07。方法：联网检索（WebSearch/WebFetch，每条关键论断内联 URL）+ BerryOS 仓内文档对照（`docs/架构决策.md`）。确定度标注：**高** = 官方文档/一手源码/RFC/论文实证；**中** = 二手转述、社区实测或较旧文档；**低** = 材料稀缺或推断。
> 系列分工：PM-00 为总览。本报告（PM-03）负责回答「宏观内核攒出的电源机制栈，在**驱动全在用户态**的微内核里怎么组织」——Linux 四条主线（tickless / cpuidle / cpufreq+thermal / 设备电源与系统睡眠）逐条解剖，Fuchsia 实证（PowerBroker / System Activity Governor / RFC-0230）给出微内核答案，seL4/QNX 简述，收官给逐机制归属表并挂 BerryOS 已有物（tickless G1、intent #42/#58、vote 票面 #53）。渲染/显示侧省电不重复（见 `research/GUI-06-低功耗渲染策略.md`），调度器内部见 `research/SCHED-01`。

---

## §0 演化主线总览：Linux 电源栈 20 年时间线【确定度：高（各条目见对应节引用）】

| 年份 | 事件 | 所属主线 |
|---|---|---|
| 2007（2.6.22） | deferrable timer；tickless idle（NO_HZ）进主线 | 定时器 |
| 2008（2.6.28） | per-thread timer slack | 定时器 |
| 2010–2012 | Android wakelock 拉锯 → wakeup source + autosleep（3.5+） | 系统睡眠 |
| 2013（3.10） | nearly full tickless（NO_HZ_FULL） | 定时器 |
| 2014（4.2 前） | IPA power allocator thermal governor | thermal |
| 2015–2018 | Energy Model 框架 + EAS 任务摆放（5.0 收齐） | cpufreq/调度 |
| 2016 | schedutil governor（吃调度器 PELT 信号） | cpufreq |
| 2016 | PSCI OSI 模式支持（cluster idle 由 OS 决定） | cpuidle |
| 2018 | uclamp（util_min/util_max）+ cgroup 控制器 | cpufreq |
| 2018+ | TEO governor 替代 menu 成为 tickless 时代默认 | cpuidle |
| 2023 | Fuchsia RFC-0230（suspend-to-idle）定稿，PowerBroker/SAG 体系成型 | 微内核对照 |

四条主线的公共形状一句话：**每条主线都在从「周期性/采样式/固定档」走向「事件驱动/信号驱动/自报数据」**——tick 从周期变按需、调频从采样变吃调度信号、C-state 从阶梯变预测、设备电源从全局睡眠变每设备自治。这个方向本身就是微内核友好型的（事件驱动天然跨进程可仲裁）。

## §1 主线一：tickless 与定时器纪律——「唤醒次数是省电的通货」

### 1.1 NO_HZ_IDLE / NO_HZ_FULL【确定度：高】

| 档位 | 语义 | 收益对象 |
|---|---|---|
| `NO_HZ_IDLE`（默认） | 仅 CPU **空闲**时停 tick（dynticks-idle） | 省电（idle 不被周期 tick 戳醒） |
| `NO_HZ_FULL` | CPU 上只跑**单个**用户态任务时也停 tick（留 housekeeping CPU 保计时） | 低抖动（HPC/实时），省电是副产物 |

出处：内核官方文档（[docs.kernel.org: NO_HZ](https://docs.kernel.org/timers/no_hz.html)）；历史：3.10 起 nearly full tickless（[LWN 549580](https://lwn.net/Articles/549580/)）。tick 的本质开销不是那条指令本身，而是它把 CPU 从深 idle 里定期拽出来——所以「停 tick」与「进深 C-state」是耦合收益。

### 1.2 timer slack 与定时器合并——为什么是省电大头【确定度：高（机制）/ 中（历史版本号）】

- **语义**：线程的「当前 timer slack」允许内核把相邻定时器到期**归组**：到期可以晚最多 slack 纳秒，但绝不提前；`select/poll/epoll_wait/nanosleep/futex` 等全部生效（[man7: PR_SET_TIMERSLACK](https://man7.org/linux/man-pages/man2/pr_set_timerslack.2const.html)）。init（PID 1）默认 **50 µs**，fork 继承、execve 保留；实时调度线程不施加 slack（同上）。
- **为什么是大头**：每次唤醒都强制 CPU 退出深 idle（甚至唤醒整个 cluster），功耗成本是「唤醒次数 × 深度返回成本」。归组把 N 个零散唤醒变成 1 个，idle 驻留时间被拉长到能进更深的档（[LWN: Timer slack](https://lwn.net/Articles/369549/)）。这就是 powertop 十几年只优化一个指标——wakeups——的原因。
- **演化**：deferrable timer（2.6.22）→ per-thread timer slack（2.6.28）→ hrtimer range timers（[Wikipedia: Timer coalescing](https://en.wikipedia.org/wiki/Timer_coalescing)，确定度中）。传统 timer wheel 对落在第一级的定时器自带最大 12.5% 的额外松弛（[内核文档： delay/sleep 机制](https://dri.freedesktop.org/docs/drm/timers/delay_sleep_functions.html)）——**定时器精度本身就该是分层的：交互路径精确、后台路径宽松**。

### 1.3 hrtimer【确定度：高】

高精度定时器（one-shot 编程）是 tickless 的前提：没有 per-CPU one-shot 时钟设备就没有「按需编程下一次到期」的停 tick。深 idle 的副作用见 §2.4（本地时钟设备会停 → broadcast 接管，[kernel-internals.org: clocksource/clockevent](https://kernel-internals.org/time/clocksource)）。

---

## §2 主线二：cpuidle——C-state、governor、cluster 协调、时钟 broadcast

### 2.1 C-state 模型【确定度：高】

每个 idle 态用三元组描述：**功耗（越深越省）× 进入/退出延迟（越深越贵）× 最小驻留（住了才回本）**（[docs.kernel.org: cpuidle](https://docs.kernel.org/admin-guide/pm/cpuidle.html)、[Lenovo lp1945](https://lenovopress.lenovo.com/lp1945-using-processor-idle-c-states-with-linux-on-thinksystem-servers)）。ARM 侧对应硬件层级：Standby / Retention / Dormant / Power-down（[ARM 开发者文档: Idle management](https://support.arm.com/documentation/100960/0100/Idle-management)）。**深 idle 不只是「CPU 不跑」**：power-down 会断 cluster 的时钟/电源与一致性域——这是 §2.3 cluster 协调和 §2.4 时钟 broadcast 存在的原因。

### 2.2 governor 谱系【确定度：高】

| governor | 策略 | 适用 |
|---|---|---|
| ladder | 阶梯式：从最浅档开始，驻留够久才升级、频繁唤醒则降级 | 有周期 tick 的系统（设备树老平台） |
| menu | **预测 idle 时长**（历史模式 + 下一个定时器到期），据此选档；tick 已停时误判代价更高 | 无 tick 空闲的通用默认 |
| TEO（Timer Events Oriented） | 以「下一个定时器到期」为主锚的预测器，tickless 时代取代 menu 的新默认 | 现代平台 |
| haltpoll | guest 主动 poll | 虚拟化 |

出处：[docs.kernel.org: cpuidle](https://docs.kernel.org/admin-guide/pm/cpuidle.html)。关键教训：menu 在 tickless 系统上有系统性误判（短睡也当长睡预测），催生了 TEO（[LWN: Improving idle behavior in tickless systems](https://lwn.net/Articles/775618/)）——**预测器必须吃「定时器到期分布」这个信息源，而不是只看历史时长**。

### 2.3 cluster 级协调：PSCI 的 PCP vs OSI【确定度：高】

多核共享 cluster 电源域时「谁决定进 cluster 级深睡」有 Two 种模式（PSCI 规范定义，固件侧 TF-A 实现）：

- **PCP（Platform Coordinated）**：每个核各自投票，**固件仲裁**最终状态——OS 不掌握全局。
- **OSI（OS-Initiated）**：**OS 知道每个核的下一次唤醒时间**，由 OS 决定何时全 cluster 进深睡，固件照办；TF-A 基准显示 OSI 的扩展性更好（[TF-A: PSCI OS-initiated mode](https://www.trustedfirmware.org/docs/PSCI-OS-initiated.pdf)、[TF-A 文档](https://trustedfirmware-a.readthedocs.io/en/latest/components/psci-osi-mode.html)）。
- Linux 侧 2016 年接入 OSI 的 cluster idle 支持（[linux-arm-kernel 邮件列表](https://lists.infradead.org/pipermail/linux-arm-kernel/2016-July/446146.html)）。

**归属含义**：OSI 的前提是「唤醒时间全系统可见」——在微内核里这天然指向**内核或单一 PM 服务持有全局定时器视图**；按核散在多个服务里做不了 OSI。

### 2.4 deep idle 下的时钟 broadcast【确定度：高】

深 idle 会停本地 per-CPU 时钟设备（x86 深 C-state 停 local APIC timer；ARM 部分深态下 arch timer 不可靠），内核用 `CLOCK_EVT_FEAT_C3STOP` 标记此类设备，并切到 **tick broadcast**：由一个 always-on 的全局时钟设备（x86=HPET；ARM=板级 SoC 定时器）代打定时器中断唤醒 CPU（[kernel-internals.org](https://kernel-internals.org/time/clocksource)、[0xax: linux-insides Timers-6](https://0xax.gitbooks.io/linux-insides/content/Timers/linux-timers-6.html)、[2012 年给 ARM arch timer 打 C3_STOP 标记的补丁](https://lore.kernel.org/lkml/201204111648.10672.arnd@arndb.de/T/)）。**教训：深 idle 与「本地时钟恒在」是互斥假设，必须 day-one 知道平台的 always-on 时钟是谁**（BerryOS QEMU virt 的 CNTV 恒在，真机需验）。

---

## §3 主线三：cpufreq——governor 收敛、EAS、uclamp、thermal、race-to-idle

### 3.1 governor 谱系收敛到 schedutil【确定度：高】

- 旧世界：`performance` / `powersave` / `userspace` / `ondemand` / `conservative`——后两者靠**周期采样 CPU 空闲率**驱动升降频，响应滞后、开销大（[Red Hat PM 指南](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/6/html/power_management_guide/cpufreq_governors)、[Embedded Computing: cpufreq 与 schedutil](https://www.embeddedcomputing.com/technology/analog-and-power/dynamic-cpu-power-management-with-cpufreq-and-schedutil)）。
- 2016 年 Rafael Wysocki 提出 **schedutil**：直接吃调度器的 PELT 利用率信号算频，**被官方文档定位为 ondemand/conservative 的替代者**，与调度器紧耦合（[docs.kernel.org: cpufreq](https://docs.kernel.org/admin-guide/pm/cpufreq.html)、[Gentoo Wiki: Power_management/Processor](https://wiki.gentoo.org/wiki/Power_management/Processor)）。现代 x86 平台上 ondemand/conservative 已普遍消失（[Manjaro 论坛实测](https://forum.manjaro.org/t/missing-cpu-frequency-governors-on-kernel-6-6-9-ryzen/52764)）。
- **20 年收敛的终点是「调频器必须住在调度器隔壁」**——采样式独立 governor 是历史包袱。

### 3.2 EAS（Energy Aware Scheduling）【确定度：高】

- 前提是 **Energy Model（EM）框架**：板级/固件申报每个性能域在各频点的功耗容量表；调度器唤醒路径用 EM + PELT 信号挑「能耗最优的 CPU」摆放任务，不对称大小核（big.LITTLE/DynamIQ）上收益最大；过载（overutilization）时自动退回普通负载均衡（[docs.kernel.org: sched-energy](https://docs.kernel.org/scheduler/sched-energy.html)、[LWN: EAS in Linux 5.0](https://lwn.net/Articles/782695/)、[kernel-internals.org/sched/eas](https://kernel-internals.org/sched/eas)）。
- **归属含义**：EAS 决策依赖调度器内部信号（PELT、任务唤醒点）——微内核里调度器在内核，则 EAS 必然在内核；但 **EM 是纯数据**，可由用户态板级服务/manifest 申报。

### 3.3 uclamp：Android 的 top-app 提频通道【确定度：高】

- uclamp 把任务的利用率信号夹在 `util_min/util_max` 之间，**util_min 直接抬 schedutil 选频下限**（[docs.kernel.org: sched-util-clamp](https://docs.kernel.org/scheduler/sched-util-clamp.html)）。Android 把前台 cgroup 的 `cpu.uclamp.min` 抬高实现 top-app 提频，替代早年各种 input boost 黑魔法（同上 + [Qualcomm 内核指南](https://docs.qualcomm.com/)，后者确定度中）。
- **这就是「上层投票、下层执行」的内核化版本**：应用不设频，只声明「我的利用率下限」。

### 3.4 thermal：IPA 智能功率分配【确定度：高】

IPA（Intelligent Power Allocation，power allocator governor，4.2+）不用阶梯式降温，而是 **PID 控制器把「功率预算」按 Energy Model 分配给各 actor**（CPU/GPU 等），在温度约束下最大化性能（[docs.kernel.org: power_allocator](https://docs.kernel.org/driver-api/thermal/power_allocator.html)、[ARM 开发者页](https://developer.arm.com/dev2/tools-and-software/intelligent-power-allocation)）。**本质是 thermal 与 DVFS 的合流**：温度问题被翻译成功率预算问题。

### 3.5 race-to-idle vs 慢跑【确定度：中（结论是条件性的）】

「赶紧干完赶紧睡」还是「降频慢跑」没有普适赢家，取决于**功耗-频率曲线的凸性**和**深 idle 档的省电幅度**：

- DATE 2016 论文实证：1.53 GHz 档上 race-to-idle 更省、2.12 GHz 档上慢跑更省——最高频段能效急剧变差时冲顶不划算（[Das et al., DATE 2016](https://past.date-conference.com/proceedings-archive/2016/pdf/0018.pdf)）。
- 社区实践收敛为「**掐掉最高频段（pace-to-idle）+ 尽快进深 C-state**」混合（[Reddit 讨论](https://www.reddit.com/r/AndroidQuestions/comments/)，确定度低）；红帽官方调优指南也是「governor 选择 + C-state 限制」两件套（[RHEL 8 调优文档](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/8/html/monitoring_and_managing_system_status_and_performance/tuning-cpu-frequency-to-optimize-energy-consumption_monitoring-and-managing-system-status-and-performance)）。
- **移动端交互负载的默认答案偏向 race-to-idle**：burst 短、深 cluster idle 省电幅度大（Android suspend 功耗约为运行态 1/100，[Android 官方: Suspend mode](https://source.android.com/docs/core/audio/suspend)）。

裁决规则归纳（BerryOS 策略服务可直接抄的判定表）：

| 条件 | 选 race-to-idle | 选降频慢跑 |
|---|---|---|
| 负载形态 | 短 burst、交互触发、做完即静 | 长时间持续吞吐（编解码/下载/同步） |
| 功耗-频率曲线 | 顶频段能效未急剧恶化 | 顶频段能效急剧恶化（掐顶频） |
| 后续 idle | 可进深 cluster idle（差价大） | 只能进浅 idle（差价小） |
| 完成时限 | 有 deadline（交互 16ms 级） | 无硬时限（maintenance/background） |

---

## §4 主线四：设备电源与系统睡眠——runtime PM / PM QoS / wakeup source / autosleep / s2idle

### 4.1 runtime PM（每设备电源状态）【确定度：高】

每设备独立 runtime suspend/resume：usage counter + pm_wq 工作队列，空闲即挂起；**runtime 与 system sleep 的唤醒配置可以不同**（如允许远程唤醒用于 runtime、不允许用于睡眠）（[docs.kernel.org: runtime_pm](https://docs.kernel.org/power/runtime_pm.html)）。做全 runtime PM 后系统级睡眠近似免费获得（[Wysocki 2011 演讲观点](https://lwn.net/Articles/479841/)，确定度中）。

### 4.2 PM QoS（延迟约束汇聚）【确定度：高】

- 两层：**CPU latency QoS**（全系统，`/dev/cpu_dma_latency`，**fd 打开期间约束有效**，关 fd 即撤销）+ **per-device resume latency**（sysfs）。
- **聚合语义 = min()**：所有请求里最紧的赢——任何一个客户端都能否决深 idle（[docs.kernel.org: pm_qos_interface](https://docs.kernel.org/power/pm_qos_interface.html)、[cpuidle 文档中的 QoS 节](https://docs.kernel.org/admin-guide/pm/cpuidle.html)）。

### 4.3 wakeup source 与 autosleep——Android wakelock 战争的遗产【确定度：高】

- Android 出树补丁 wakelock/opportunistic suspend（2010–2012 两年 LKML 拉锯）最终以**主线的 wakeup source API + autosleep** 落地（3.5+）：唤醒事件激活 wakeup source 阻止睡眠，**事件处理的 race-free 记账**是核心设计点（[LWN: Autosleep and wake locks](https://lwn.net/Articles/479841/)、[Wysocki RFC 补丁集](https://lkml.iu.edu/hypermail/linux/kernel/1204.2/03641.html)）。
- 语义翻转：不是「到点请求睡眠」，而是「**系统永远准备好睡，只有活跃的 wakeup source 拦着**」——autosleep 线程在队列空时不断尝试进睡眠。这一模型后来成为 Android 全部待机功耗的地基。

### 4.4 suspend-to-RAM vs s2idle【确定度：高】

| 维度 | s2idle（freeze/S0ix） | STR（deep/S3，PSCI system suspend） |
|---|---|---|
| 机制 | 纯软件：设备 quiesce + CPU 进最深 idle 态，RAM 保持供电 | RAM 自刷新、CPU/域断电，固件参与 |
| 唤醒延迟 | 低 | 高（经固件恢复） |
| 功耗 | 较高 | 低一个量级 |
| 平台依赖 | 通用（mainline 兜底路径） | 需固件/板级支持 |

出处：[docs.kernel.org: sleep-states](https://docs.kernel.org/admin-guide/pm/sleep-states.html)、[TI: S2Idle 与 PSCI 集成](https://software-dl.ti.com/processor-sdk-linux/esd/AM62LX/latest/exports/docs/linux/Foundational_Components/Power_Management/pm_psci_s2idle.html)。田野常见坑：平台 fallback 到 s2idle 而非 deep，功耗差数倍（[Pine64 论坛实测](https://forum.pine64.org/showthread.php?tid=10250)，确定度中）。ARM mainline 手机普遍走 s2idle + PSCI 路径（同 TI 文档）。

---

## §5 Fuchsia 实证：驱动在用户态时的电源仲裁怎么解

Fuchsia 是「微内核 + 全用户态驱动」直面这个问题的唯一大规模在产样本。它的答案是三件套：**PowerBroker（仲裁器）+ SAG（系统活动治理）+ 用户态编排的 suspend 流**。

### 5.1 框架定位【确定度：高】

官方定位两条：**协调**（组件按依赖序开关资源，「资源恰好只在需要时存在」）+ **可调试性**（状态变化全可见）；明确声明「**电源框架的目标不是用聪明的调度或资源管理来省电**」——省电靠各资源所有者，框架只负责次序与可见性（[fuchsia.dev: power framework](https://fuchsia.dev/fuchsia-src/concepts/power/intro)）。**这是一条架构裁决：仲裁器不做策略、只做拓扑一致性。**

### 5.2 PowerBroker：lease/依赖图模型【确定度：高（FIDL 一手）】

- **原语**：power element（带有序 levels）+ 依赖边 + lease。所有元素注册成一张**全局拓扑图**（`Topology.AddElement`），客户端用 `Topology.Lease` 直接开租约（[fuchsia.power.broker FIDL](https://fuchsia.dev/reference/fidl/fuchsia.power.broker)、[源码 src/power/broker](https://fuchsia.googlesource.com/fuchsia/+/HEAD/src/power/broker)）。
- **租约语义**：开租约把目标元素（及其**全部直接+传递依赖**）抬到所需 level；broker 调元素所有者的 `ElementRunner.SetLevel`（**阻塞到迁移完成**）；状态机 `PENDING → SATISFIED → POWERING_DOWN → VACATED`；**最后一个 LeaseToken 句柄关闭即租约结束**（[FIDL 同上](https://fuchsia.dev/reference/fidl/fuchsia.power.broker)）。
- **依赖分两类**：**assertive**（主动抬升依赖方）与 **opportunistic**（依赖方已在高位才顺便用，不主动抬）（[fuchsia.dev: observing](https://fuchsia.dev/fuchsia-src/concepts/power/intro/observing)）。
- 经典例子（官方）：下载文件 = app 租「可下载」→ 框架按序唤醒 CPU → 网络驱动 → netstack → 回告 app；app 撤租后按逆序关停闲置资源（[power framework intro](https://fuchsia.dev/fuchsia-src/concepts/power/intro)）。**这是 runtime PM 的去中心化版本：设备电源由驱动服务自己拥有，全局一致性由图仲裁。**

### 5.3 System Activity Governor：execution_state 治理【确定度：高（FIDL 一手）】

SAG 是「系统级活动」这个特殊元素的官方持有者（[fuchsia.power.system FIDL](https://fuchsia.dev/reference/fidl/fuchsia.power.system)）：

- 持有三元素：**ExecutionState**（`INACTIVE/SUSPENDING/ACTIVE` 三级）、**ApplicationActivity**（INACTIVE/ACTIVE）、**Cpu**（INACTIVE/ACTIVE）——**连 CPU 也被建模成 power element**，SAG 充当内核 idle 与用户态拓扑之间的桥。
- **wake lease = 能力句柄**：`ActivityGovernor.AcquireWakeLease(name) → LeaseToken`；**「只要存在有效 LeaseToken，平台不会挂起」**——挂起阻塞只在**客户端收到响应之后**才生效（文档明确警告：中断必须等无错回复后才能 ack）（同上）。
- `RegisterSuspendBlocker`（BeforeSuspend/AfterResume 回调）+ `BootControl.SetBootComplete`——**开机完成标志没置位前 SAG 拒绝挂起**（防启动期挂起环）。
- **wake lease 跨组件交接三式**：接力棒传递（token 随消息传）／协议互锁（下游先拿租约再 ack 上游）／纯超时（官方明言 race 恶劣、强烈不推荐）（[fuchsia.dev: basic pipeline processing](https://fuchsia.dev/fuchsia-src/concepts/power/intro/basic_pipeline_processing)）。

### 5.4 suspend 实现：RFC-0230【确定度：高（RFC 一手）】

- 目标态类 ACPI S1：boot CPU 留 C0 最低功耗运行态、非 boot CPU 下线、内存自刷新、子设备落到恢复延迟允许的最低档（[RFC-0230](https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0230_suspend_to_idle)）。
- **分工**：用户态 Power Framework 持拓扑+板级配置+用户约束（恢复延迟/唤醒能力）并做**全部策略**；驱动只申报自己的 levels/依赖/唤醒延迟并实现 SetLevel；**内核原语**仅四件——暂停单调时钟（**挂起期间 monotonic time 不许走**，程序时间观一致性）、CPU 上/下线、编程唤醒中断、导出 CPU 指标（同 RFC）。
- **明确否决了两条路**：扩老 suspend 流（太弱）与**照抄 Linux 回调序模型**——「单体内核假设不 fits Fuchsia 多进程用户态」（同 RFC）。**驱动声明粒度的 power level（非固定 ACPI D-state）+ 唤醒延迟申报**被点名为相对 Linux/Windows 的差异化设计。

---

## §6 seL4 / QNX 电源面简述

| 系统 | 电源面事实 | 确定度 |
|---|---|---|
| QNX Neutrino | 硬件/软件协同模型：CPU 与 SDRAM 低功耗模式；IDLE 电源态由 idle 内核进入；用户态 `sysmgr_cpumode()`（如 GOTOIDLE）经 power callout 切换；唤醒/告警由 callback 体系管理 | 中（[QNX 6.3.0SP3 低功耗指南](https://www.qnx.com/developers/docs/6.3.0SP3/power_mgmt_en/user_guide/cpu_mem.html)、[QNX 系统架构: Power management](http://www.qnx.net/developers/docs/6.3.0SP3/neutrino/sys_arch/power.html)；文档较老） |
| seL4 | **无公开的内核电源管理框架文档**；内核有 idle 线程（手动计时可见）但 DVFS/深 idle/系统睡眠通常委托用户态（L4 纪律：内核极简）。未检索到类似 PowerBroker 的仲裁层公开材料 | 低（检索未见一手材料，属推断；[seL4 手册](https://sel4.systems/Info/Docs/seL4-manual-15.0.0.pdf) 可查 idle 线程计时） |

---

## §7 教训与历史包袱清单

1. **governor 动物园（ondemand/conservative/userspace/ladder）= 20 年弯路**：采样式独立调频器最终全部让位给「住在调度器隔壁」的 schedutil（§3.1）。起点选错邻居，收敛花了二十年。
2. **wakelock 战争两年，赢在语义翻转**：从「主动请求睡眠」翻成「永远准备睡、wakeup source 拦着」（§4.3）。Android 出树补丁→主线 wakeup source 的迁移成本极高，**醒锁语义必须 day-one 进主线协议**。
3. **menu 误判教训**：idle 预测器不吃「定时器到期分布」就会在 tickless 时代系统性选错档（§2.2）。
4. **C3STOP 教训**：深 idle 停本地时钟是常态不是例外，always-on 时钟依赖必须 day-one 显式建模（§2.4）。
5. **ACPI D-state 固定档太僵硬**：Fuchsia 用「驱动自报 levels + 唤醒延迟」替代，Linux 自己没有等价查询面（§5.4，RFC 原文点名）。
6. **PM 策略长进内核后搬不出来**：Linux thermal IPA、EAS、cpufreq 决策全在内核态，用户态只剩调参接口；微内核若重蹈「仲裁+策略都进仲裁服务」就是换了地方的宏内核。
7. **s2idle/STR 的平台分叉**：fallback 到 s2idle 的设备功耗差数倍且用户无感（§4.4）——睡眠档位语义要在协议里显式可见。
8. **定时器精度分层是省电的结构性杠杆**：slack/deferrable/range-timer 三代演进全在教一件事——**后台工作没有资格要精确唤醒**（§1.2）。
9. **挂起期间时钟语义必须定义**：Fuchsia 明确 monotonic 暂停；Linux 用 suspend 前后读时钟补偿，应用侧时间观历来有坑（§5.4）。

---

## §8 BerryOS 启示：逐机制归属表与可执行建议

### 8.1 逐机制归属表（收官）

| 机制 | Linux 归属 | 微内核归属建议 | BerryOS 已有对应物 |
|---|---|---|---|
| idle 入口（停 tick+WFI） | 内核 idle loop | **内核必备**（上下文/时钟归内核） | tickless G1 已落地（空闲 mask CNTV + wfi，`docs/架构决策.md` 冻结常量行） |
| 定时器栈 + slack/合并 | 内核 hrtimer | **内核必备**（定时器队列全系统单源，OSI 前提） | #42 IntentProfile 已列「timer slack 档」（机制未实装） |
| C-state 深度策略（menu/teo 类） | 内核 cpuidle governor | 表=驱动服务/板级申报；**策略=用户态 PM 服务**；入口指令=内核 | 无（v1 wfi 单档起步，合理） |
| cluster idle 协调（OSI 式） | 内核+PSCI 固件 | **内核**（需全局唤醒视图）+ 固件 | 无（v1 单核，SMP 线再启） |
| 时钟 broadcast | 内核 tick-broadcast | **内核必备** | QEMU CNTV 恒在天然满足；真机须查 always-on 定时器 |
| DVFS 决策（schedutil 式） | 内核 cpufreq | **机制在内核**（吃调度信号），**策略=用户态服务**下发频率帽/偏好 | 「机制在内核、策略可插拔」已定（冻结常量）；上层只经 intent 带内投票（#42 纪律） |
| EAS 摆放 + Energy Model | 内核调度器 + EM 框架 | 决策=**内核**（调度信号在内）；EM 数据=板级服务申报 | v1 单核延后；EM 表格式可先定（boot 配置参数表） |
| uclamp | 内核 + cgroup | **内核机制**；票面来源=intent 六档 | #42 IntentProfile 明列 uclamp 档 + #58 六档词汇表——**命名已对齐，只差实装** |
| thermal IPA | 内核 thermal governor | **用户态热服务**（PID over EM）+ 内核执行降频帽 | 无；挂 PM-00 框架统一规划 |
| runtime PM（每设备电源） | 内核驱动模型 | **驱动服务自理** + 向仲裁器声明依赖 | 驱动服务工作区在建（`drivers/`） |
| PM QoS 汇聚（min 聚合） | 内核 PM QoS | **PM 仲裁服务**（约束=租约属性） | #53 PM 共享环 {target_ns,actual_ns} 是 GUI 侧雏形 |
| wakeup source / 醒锁 | 内核 wakeup_source | **内核对象（能力句柄）**+ 仲裁服务命名——能力系统白送的引用计数 | 无 → **day-one 建议（8.2-1）** |
| autosleep / 机会睡眠 | 内核 autosleep | **仲裁服务策略**：租约图空 → 尝试挂起 | 无（与上一条同刀） |
| suspend 编排（s2idle 式） | 内核 PM core+驱动回调 | **用户态 PM 服务编排** + 内核四原语（停钟/CPU 上下线/唤醒 IRQ 编程/遥测） | 无；v1 移动端必须有，挂 PM-00 |
| 显示/帧率省电 | （无系统级对应） | 显示服务 | **#53 六件协议位齐备**（damage/三态停帧/vote 票面/power·mode op/PM 环/能耗断言）+ GUI-06 全文 |

### 8.2 可执行建议（接口/契约/协议位级）

1. **醒锁 = 能力句柄，day-one 进协议**：定义 wake lease 为内核能力对象（或 PM 服务通道端点），语义照抄两条实证——「最后一个句柄存活期间系统不得挂起」（Fuchsia power broker 的 `LeaseToken` 语义——FIDL 协议句柄，非 zx 内核对象，§5.3）+「唤醒阻塞仅在持有者收到 ack 之后生效」（防中断 ack 竞态，Fuchsia FIDL 原文警告）。autosleep 语义照 Linux 翻转式：**租约图空即尝试挂起，不设『请求睡眠』接口**（§4.3）。挂起阻塞协议位起名预留（如 `PM_WAKE_LEASE`，name ≤64 字节对齐 fuchsia 上限）。
2. **intent 六档 ×（uclamp 档、timer slack 档、EPP、latency-nice）查表 day-one 落地内核**（#42 第一段已点名）：这正是 Linux 用 8 年（uclamp 2018 主线）+ Android 前台提频验证过的「上层投票、下层执行」终态。**新增的具体裁决：slack 档给数值锚**——latency_critical=0（绝不迟到）、interactive≈50 µs（Linux init 默认值，§1.2）、utility≈毫秒级、maintenance/broadcast 可到 100 ms 级；`poll/epoll/futex/sleep` 类等待全部吃 slack。
3. **PM 拓扑服务（PowerBroker 对应物）v1 就用二元档**：驱动服务在 manifest/初始化时向 PM 服务注册 element（`ON/OFF` 二级 + 依赖边 + 唤醒延迟字段）；PM 服务算 required level、经 IPC 下发 SetLevel（阻塞到完成）。**契约要点：依赖边声明谁、唤醒延迟是必填字段**——Fuchsia 教训：固定 ACPI 档不如自报数据（§5.4/§7-5）。v2 再开放多级。
4. **DVFS/降频帽的带内纪律写进服务契约**：用户态策略服务读内核遥测（idle 驻留分布、利用率、intent 计数）算频率帽，内核执行；**应用与 GUI 永远只投 intent，不设频不设帧率**（#42/#53 已定，此处补：PM 服务是唯一合法的频率帽书写者，AI/GUI 都经它）。
5. **race-to-idle 设默认交互策略**：interactive intent → 允许冲高频尽快回深 idle（util 下限抬升）；background/maintenance → 限频长跑。裁决依据写进文档：功耗-频率曲线末端凸性 + cluster 深 idle 差价（§3.5），并把「掐顶频」做成策略服务的可调参数。
6. **挂起流抄 RFC-0230 的四原语分工**：内核只提供（a）暂停单调时钟、（b）CPU 下线/上线、（c）唤醒中断编程、（d）CPU 指标导出；编排、板级配置、恢复延迟约束全在用户态 PM 服务。**boot-complete 标志照抄**：未置位前禁止挂起（防启动挂起环，§5.3）。
7. **idle 深度策略做在用户态服务但要吃得到「下一个定时器到期」**：内核遥测必须导出 per-CPU 定时器到期分布（TEO 教训，§2.2），否则用户态 governor 只能重蹈 menu 误判。
8. **PM QoS 语义定为 min() 聚合**（任一客户端可否决深 idle），恢复延迟作为租约/约束属性进协议；GUI 侧 #53 的 PM 共享环 {target_ns,actual_ns} 泛化为全设备统一的「目标态 vs 实际态」遥测对。
9. **挂起档位显式化**：协议里区分 s2idle 式（内存保电、快醒）与 deep 式（断电、省一个量级），档位与实际落点进遥测——Linux 田野 fallback 坑的预防针（§4.4）。

### 8.3 与既有材料的缝合

- 渲染/显示侧（PSR/damage/停帧/帧率投票）→ `research/GUI-06-低功耗渲染策略.md` + ADR #53，本报告不重复；#53 的 vote 票面就是本报告 §3.3 uclamp 的 GUI 特化。
- 调度器内部（intent 分带/EEVDF/EPP）→ `research/SCHED-01-调度之王业界深研与综合裁决-v1.0.md` + ADR #42/#58；本报告确认其 IntentProfile（uclamp/slack 档）与 Linux 十五年演化终点同构。
- 全系统 PM 总框架（服务拓扑、基带/显示/触摸/网络/GPS 各外设的省电契约）→ PM-00 总览统一收口。

---

## §9 引用源列表

**Linux 内核官方文档**（docs.kernel.org / man7）：[NO_HZ](https://docs.kernel.org/timers/no_hz.html) · [cpuidle](https://docs.kernel.org/admin-guide/pm/cpuidle.html) · [cpufreq governors](https://docs.kernel.org/admin-guide/pm/cpufreq.html) · [sched-energy (EAS)](https://docs.kernel.org/scheduler/sched-energy.html) · [sched-util-clamp (uclamp)](https://docs.kernel.org/scheduler/sched-util-clamp.html) · [power_allocator (IPA)](https://docs.kernel.org/driver-api/thermal/power_allocator.html) · [runtime_pm](https://docs.kernel.org/power/runtime_pm.html) · [pm_qos_interface](https://docs.kernel.org/power/pm_qos_interface.html) · [sleep-states](https://docs.kernel.org/admin-guide/pm/sleep-states.html) · [PR_SET_TIMERSLACK man page](https://man7.org/linux/man-pages/man2/pr_set_timerslack.2const.html)

**LWN / 邮件列表 / 论文**：[Timer slack (2010)](https://lwn.net/Articles/369549/) · [Nearly full tickless in 3.10](https://lwn.net/Articles/549580/) · [Improving idle behavior in tickless systems](https://lwn.net/Articles/775618/) · [Autosleep and wake locks](https://lwn.net/Articles/479841/) · [EAS in Linux 5.0](https://lwn.net/Articles/782695/) · [Wysocki autosleep RFC (LKML)](https://lkml.iu.edu/hypermail/linux/kernel/1204.2/03641.html) · [PSCI cluster idle OSI (linux-arm-kernel)](https://lists.infradead.org/pipermail/linux-arm-kernel/2016-July/446146.html) · [Das et al., DATE 2016（race-to-idle 实证）](https://past.date-conference.com/proceedings-archive/2016/pdf/0018.pdf)

**固件/厂商/教学**：[TF-A: PSCI OS-initiated mode PDF](https://www.trustedfirmware.org/docs/PSCI-OS-initiated.pdf) · [ARM: Intelligent Power Allocation](https://developer.arm.com/dev2/tools-and-software/intelligent-power-allocation) · [ARM: ARMv8-A Idle management](https://support.arm.com/documentation/100960/0100/Idle-management) · [Lenovo lp1945: C-states on ThinkSystem](https://lenovopress.lenovo.com/lp1945-using-processor-idle-c-states-with-linux-on-thinksystem-servers) · [TI: S2Idle 与 PSCI 集成](https://software-dl.ti.com/processor-sdk-linux/esd/AM62LX/latest/exports/docs/linux/Foundational_Components/Power_Management/pm_psci_s2idle.html) · [kernel-internals.org: clocksource/C3STOP](https://kernel-internals.org/time/clocksource) · [kernel-internals.org/sched/eas](https://kernel-internals.org/sched/eas) · [0xax linux-insides: Timers-6（tick broadcast）](https://0xax.gitbooks.io/linux-insides/content/Timers/linux-timers-6.html) · [Embedded Computing: cpufreq 与 schedutil](https://www.embeddedcomputing.com/technology/analog-and-power/dynamic-cpu-power-management-with-cpufreq-and-schedutil) · [Gentoo Wiki: Power_management/Processor](https://wiki.gentoo.org/wiki/Power_management/Processor) · [RHEL 8 调优指南](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/8/html/monitoring_and_managing_system_status_and_performance/tuning-cpu-frequency-to-optimize-energy-consumption_monitoring-and-managing-system-status-and-performance) · [Android: Suspend mode](https://source.android.com/docs/core/audio/suspend)

**Fuchsia（一手）**：[Power framework 概念](https://fuchsia.dev/fuchsia-src/concepts/power/intro) · [fuchsia.power.broker FIDL](https://fuchsia.dev/reference/fidl/fuchsia.power.broker) · [fuchsia.power.system FIDL](https://fuchsia.dev/reference/fidl/fuchsia.power.system) · [wake lease 交接模式](https://fuchsia.dev/fuchsia-src/concepts/power/intro/basic_pipeline_processing) · [依赖类型（assertive/opportunistic）](https://fuchsia.dev/fuchsia-src/concepts/power/intro/observing) · [RFC-0230: Suspend-To-Idle](https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0230_suspend_to_idle) · [src/power/broker 源码](https://fuchsia.googlesource.com/fuchsia/+/HEAD/src/power/broker)

**QNX / seL4**：[QNX 6.3.0SP3 低功耗管理指南](https://www.qnx.com/developers/docs/6.3.0SP3/power_mgmt_en/user_guide/cpu_mem.html) · [QNX 系统架构: Power management](http://www.qnx.net/developers/docs/6.3.0SP3/neutrino/sys_arch/power.html) · [seL4 Reference Manual 15.0.0](https://sel4.systems/Info/Docs/seL4-manual-15.0.0.pdf)

**BerryOS 仓内对照**：`docs/架构决策.md`（ADR #42 intent 两段式 / #53 低功耗 GUI 六件 / #58 六档词汇表 / #39 主战场 / 冻结常量 tickless 行）· `research/GUI-06-低功耗渲染策略.md` · `research/SCHED-01-调度之王业界深研与综合裁决-v1.0.md`
