# Zephyr 调研报告
> 调研日期 2026-10-15 · 基于公开资料与源码快照 · 许可证：Apache-2.0（仓库根 LICENSE，源文件 SPDX 头一致标注）

## 0. 一句话定位

Zephyr 是 Linux 基金会主导、Apache-2.0 授权的**可伸缩单映像 RTOS**：以 tickless 内核 + 可配置调度器为核心，靠 Kconfig/devicetree 在"8KB 级 MCU"与"多核 Cortex-A + 蓝牙/IP 协议栈"之间伸缩，是当前社区动能最强的开源 RTOS，也是 BerryOS「tickless day-one + 可插拔调度」两大设计最成熟的先例。

## 1. 谁在做：组织/社区/治理/活跃度/商业背书

- **治理**：Linux 基金会中立项目（2016-02 立项，源自 Wind River Rocket 与 Intel 代码合流），Governing Board + TSC 双层。白金成员含 Nordic Semiconductor、NXP、Intel、Renesas、Wind River、Antmicro、Linaro 等（[Project Members](https://www.zephyrproject.org/project-members)）。Nordic 历史上是最大代码贡献方，其 nRF Connect SDK 整体基于 Zephyr。
- **节奏**：半年一个大版本。当前稳定版 **4.4.0**（2026-04-14 发布，含 WireGuard、Wi-Fi Direct、OpenRISC），补丁至 4.4.2；主线 `VERSION` 文件为 **4.5.0-rc1**（本次源码快照即 main@4.5-dev），4.5 按[发布时间表](https://docs.zephyrproject.org/latest/releases/index.html)应于 2026-10 出货；另有长期维护线 LTS3（v3.7.x）。
- **活跃度**：官方 2024 年度总结称全年 **1,100 名独立贡献者、过半为首次贡献**；二手来源（Beningo）称单个版本贡献者 2025-07 约 810 人、2026-04 创纪录 931 人（精确数待核实）。Rust 支持自 4.1（2025-03）以可选模块 `zephyr-lang-rust`（`CONFIG_RUST`）实验性引入。
- **商业背书**：2026-03 Linux 基金会《Zephyr Turns 10》报告：**70% 北美受访组织已在产品中使用 Zephyr，69% 计划扩大采用**。车载方向有 Automotive 工作组与 UDS 诊断栈；功能安全方面 Safety WG 正推进 IEC 61508 SIL 3 认证（尚未取证，ISO 26262 ASIL D 为后续路径，见 [Safety FAQ](https://github.com/zephyrproject-rtos/zephyr/wiki/Safety-FAQ)）。

## 2. 总体架构：内核形态、分层与组件

**形态：静态链接单映像 RTOS（宏内核式），非微内核。** kernel + 子系统 + 驱动在链接期裁剪进一个二进制，默认无内核/用户隔离；隔离是可选项 `CONFIG_USERSPACE`（MPU 沙箱 + syscall 门），无 MMU 每进程地址空间、无 fork。4.0 起有 `llext` 可加载 ELF 扩展（`include/zephyr/llext/`）。

分层：`kernel/`（调度、超时、IPC 对象 k_msgq/k_pipe/mailbox/futex）、`subsys/`（pm、bluetooth、net、fs、logging…）、`drivers/`、`boards/` + `dts/`、`include/zephyr/`。IPC 全是内核对象直接调用（同映像内函数调用 + 自旋锁），无消息跨地址空间拷贝——这与 BerryOS 的 channel IPC 是两个世界，也是"单映像 RTOS vs 微内核"的分界线。

## 3. 核心机制深挖

### 3.1 tickless 内核与超时子系统（对标 BerryOS NO_HZ）

`CONFIG_TICKLESS_KERNEL` 默认开启（`TICKLESS_CAPABLE` 时 `default y`）：**周期 tick 完全消失**，内核与定时器驱动之间只有三个函数的契约（`include/zephyr/drivers/timer/system_timer.h`）：`sys_clock_set_timeout(ticks)`（告知下次唤醒前没有 announce）、`sys_clock_announce(ticks)`（到点批量上报）、`sys_clock_elapsed()`（当前 tick 内已走的 cycles）。全部在 `timeout_lock` 下调用，SMP 下"announce 全系统恰好一次由驱动串行化"是文档化契约。

`kernel/timeout.c`（517 行）前端固定，**队列后端五选一**（kernel/Kconfig:788 `choice TIMEOUT_BACKEND`）：

```c
#if defined(CONFIG_TIMEOUT_BACKEND_MINHEAP)
#include "timeout/minheap.h"
#elif defined(CONFIG_TIMEOUT_BACKEND_WHEEL)
#include "timeout/wheel.h"
... /* BUCKET / SKIPLIST */
#else /* CONFIG_TIMEOUT_BACKEND_DLIST */
#include "timeout/list.h"
#endif
```

默认 DLIST（排序增量链表，O(n) 插入）；wheel 为分层时间轮（`kernel/timeout/wheel.h`：soon[32]/later[32]/distant 三级，近期插入 O(1)，位图跳空 tick，且自带 announce 循环 `_TIMEOUT_BACKEND_OWNS_ANNOUNCE`）；minheap/skiplist O(log n)。多后端改造较新（版权标注 2026），发布版固化情况待核实。

三个可直接抄的机制：

1. **唤醒上限**：`SYS_CLOCK_MAX_WAIT = UINT32_MAX/2`，`next_timeout()` 把下次唤醒钳在此预算内，保证 announce 计数不溢出、无超时时驱动仍周期醒来维护 uptime。
2. **elapsed 补偿不变式**：SMP 下其他 CPU 观测到 `T_real - curr_tick == announce_remaining + sys_clock_elapsed()`（timeout.c:84 注释），announce 窗口内时间读数保持单调——BerryOS 做 tickless 时钟读取时要同款不变式。
3. **深度停摆**：`CONFIG_SYSTEM_CLOCK_SLOPPY_IDLE` 下队列为空时调 `sys_clock_no_timeout()` 让驱动彻底停表，接受 uptime 漂移换取最省电；相对超时入队时 +1 tick 向上取整、而"正在 announce 的 CPU"免取整以避免周期定时器逐拍滞后（timeout.c:196-218）。

### 3.2 可插拔调度器（机制/策略分离的样板）

优先级空间分段：负数为**协作线程**（`NUM_COOP_PRIORITIES` 默认 16，其中最顶端可划出 `NUM_METAIRQ_PRIORITIES` 档 meta-IRQ），非负为**抢占线程**（默认 15 档）。就绪队列数据结构是编译期 Kconfig choice（kernel/Kconfig:280+），`kernel/include/priority_q.h` 用 `#define _priq_run_add z_priq_rb_add` 这类别名做**零开销静态分派**：

```c
#if defined(CONFIG_SCHED_SIMPLE)      /* 单链表，≤3 个就绪线程，省 ~2KB */
#define _priq_run_add   z_priq_simple_add
#elif defined(CONFIG_SCHED_SCALABLE)  /* 红黑树，>20 线程可扩展 */
#define _priq_run_add   z_priq_rb_add
#elif defined(CONFIG_SCHED_MULTIQ)    /* 教科书每优先级一队列 O(1) */
#define _priq_run_add   z_priq_mq_add
#endif
```

策略层同理可叠加：`SCHED_DEADLINE`（EDF）、`SCHED_CPU_MASK`（亲和）、`TIMESLICING`（**每 CPU 一个 slice timeout**，`kernel/timeslicing.c:16` `slice_timeouts[CONFIG_MP_MAX_NUM_CPUS]`）、meta-IRQ。EDF 的比较器值得抄（priority_q.h:91-110）：同优先级内按 deadline 排序，利用"deadline 同处 32 位半模空间"的无符号减法免读当前时钟；`kernel/deadline.c` 把相对 deadline 钳到 `INT32_MAX/2` 保住半模不变式。meta-IRQ 语义（`kernel/include/metairq.h:64`）：可抢占一切协作线程，但**必须回到被它抢占的那个协作线程**（`_current_cpu->metairq_preempted` 记录、`metairq_preempt_recover()` 恢复），实现"bottom half"。

**与 BerryOS 对比**：Zephyr 的"可插拔"= 编译期选数据结构 + 若干布尔开关叠层，策略运行时不可换、不可组合（`SCHED_MULTIQ depends on !SCHED_DEADLINE` 就是组合爆炸的伤疤）。BerryOS 的 trait 策略对象（运行时注册、AI 服务可换）是更强的形态；Zephyr 证明的是**接口收窄的价值**：调度核心只依赖 `_priq_run_add/remove/best/yield` 四个操作。

### 3.3 SMP：全局就绪队列 + IPI，而非每核 runqueue

关键事实（`kernel/include/run_q.h:17-40`）：默认所有 CPU 共享**一个全局就绪队列**（`_kernel.ready_q.runq`），取队列最优者后按 `cpu_mask` 过滤；仅 `CONFIG_SCHED_CPU_MASK_PIN_ONLY`（全线程钉死单核的应用，issue #38844）才给每个 `_cpu` 放独立队列。同步靠**单把 `_sched_spinlock`**（sched.c:36），跨核唤醒用 IPI（`flag_ipi(ipi_mask_create(thread))`，有 directed/broadcast 两档）。UP 下"就绪队列缓存最佳线程"（`ready_q.cache`）的优化在 SMP 下直接失效（sched.c:86 注释）。新主线还出现 IPI 空闲预约机制（`ipi_idle_reserved_take`）：给可运行线程在被 IPI 唤醒的空闲核上"占座"。这套设计与 BerryOS 的每核 runqueue + 负载均衡（Linux 式）是两条路线：Zephyr 用队列简单性换锁竞争，小核数（≤4）够用，其 PIN_ONLY 补丁恰是全局队列不够用的证据。

### 3.4 电源管理（对标 BerryOS 省电支柱）

链路（`kernel/idle.c:56` → `subsys/pm/pm.c`）：idle 线程关中断后把**下一个内核超时**交给 PM 核心：

```c
_kernel.idle = (int32_t)z_get_next_timeout_expiry();
system_suspended = pm_system_suspend(_kernel.idle);
```

`pm_system_suspend()`（pm.c:172）里策略只面对一个窄契约：`pm_policy_next_state(cpu, ticks) -> pm_state_info*`（`include/zephyr/pm/policy.h:101`），即"给定还能睡多久，选哪个状态"。状态表（`subsys/pm/state.c`）由 SoC/板级填。三处精细设计：

1. **预期唤醒补偿**：`ticks = min(内核超时, 下个 PM 事件)` 后再减去目标状态退出延迟——`idle_ticks = ticks - exit_latency_ticks`（pm.c:240-242），换算有 NEAR/CEIL/FLOOR 三模式（`CONFIG_PM_PREWAKEUP_CONV_MODE_*`）；无事件时传 `SYS_CLOCK_IDLE_FOREVER` 让时钟停走。
2. **设备两级 PM**：`PM_DEVICE_SYSTEM_MANAGED` 在系统suspend时按依赖序挂起全部设备；平时 `device_runtime.c` 是**使用计数模型**（`pm_device_runtime_get/put`，引用归零才真正挂起，支持异步 workqueue），最后持锁者负责跨 CPU 设备状态变更。
3. **QoS 通道**：`pm_policy_latency_request_add()` 让任意驱动/应用登记"最大可容忍唤醒延迟"，策略据此排除深睡——这是用户态约束流向电源决策的原语。

注意：Zephyr 的 policy 是**链接进内核的回调**，不是进程；BerryOS 把 PM 放用户态服务（共享环）更进一步，但"next_state(cpu,ticks) 单函数契约 + 延迟请求"这套接口形状可直接移植。

### 3.5 用户态与系统调用门（与 BerryOS MMU 路线对照）

`CONFIG_USERSPACE`：MPU 沙箱 + 双栈（`GEN_PRIV_STACKS` 特权栈/用户栈分离）+ **内存域** `k_mem_domain`（分区数受 MPU 限制，`kernel/userspace/mem_domain.c:402` `arch_mem_domain_max_partitions_get()`）。syscall 门是自动生成的：`z_impl_*`（实现）+ `z_vrfy_*`（验证，含 `K_OOPS(K_SYSCALL_OBJ(...))` 句柄类型/权限校验）+ 生成的 `_mrsh.c` 装入 `_k_syscall_table[K_SYSCALL_LIMIT]`（`include/zephyr/internal/syscall_handler.h`）。内核对象带**按线程的权限位图**（`k_object` + `k_thread_perms_all_clear`，userspace.c）——一个简化的句柄权限模型。局限：分区粒度粗（MPU 区域数个位数~32）、无页式 AS、对象权限是位图而非可传递能力；BerryOS 的 Zircon 式 handle + 每进程页表是更强的隔离，但 z_impl/z_vrfy 分层与"验证失败即 K_OOPS"的纪律值得保留。

### 3.6 构建体系与驱动模型

Kconfig（配置）+ devicetree（硬件拓扑）+ CMake/west，整套借自 Linux 生态。驱动是 ROM 常量描述符（`include/zephyr/device.h:543`）：

```c
struct device {
	const char *name;
	const void *config;        /* 实例配置（ROM） */
	const void *api;           /* 驱动 API 表 */
	struct device_state *state;
	void *data;                /* 实例私有数据（RAM） */
	struct device_ops ops;
	device_flags_t flags;
	/* 可选依赖句柄（CONFIG_DEVICE_DEPS） */
};
```

实例由 `DEVICE_DT_DEFINE()` 从 devicetree 节点生成，初始化分六级（`include/zephyr/init.h`：EARLY → PRE_KERNEL_1/2 → POST_KERNEL → APPLICATION → SMP）+ 级内优先级，启动顺序全静态可预测。4.4 单版新增 121 块板，DT 复用是直接原因。

## 4. 源码走读：目录结构、关键文件、关键数据结构

- `kernel/timeout.c` —— tickless 前端：`curr_tick`(u64)、`timeout_lock`、`announcing_cpu`/`announce_remaining`（SMP announce 单飞）、`inflight_timeout` 超superseded 位（跨 CPU 撤销在途回调）。
- `kernel/timeout/{wheel,bucket,minheap,skiplist,list}.h` —— 五个后端，仅被 timeout.c 私有包含。
- `kernel/sched.c`（994 行）—— `next_up()`（UP 走 cache/SMP 全量比较）、`z_get_next_switch_handle()`（上下文切换核心）、IPI/ halt-spin 全在。
- `kernel/include/{run_q.h, priority_q.h, metairq.h, ksched.h}` —— 队列抽象、EDF 比较器、meta-IRQ 簿记。
- `kernel/timeslicing.c` —— 每 CPU slice timeout 数组。
- `subsys/pm/{pm.c, state.c, device_runtime.c, policy/}` —— PM 核心/状态表/设备 runtime PM/内置策略。
- `kernel/userspace/{userspace.c, mem_domain.c}` —— 内核对象表、权限位图、内存域。
- 结构体：`struct _cpu`（kernel_structs.h:147，含 `metairq_preempted`、`swap_ok`）、`struct z_kernel`（`cpus[]` + 全局 `ready_q` + PM 用 `idle`）、`struct device`、`struct pm_state_info`。

## 5. 亮点：好在哪

1. **tickless 是契约级设计而非补丁**：三函数 driver 契约 + MAX_WAIT 预算 + elapsed 不变式 + sloppy 停摆，10 年演进成体系（源码可直接验证）。
2. **超时队列后端化**：同一前端下 dlist/minheap/wheel/bucket/skiplist 五实现按负载画像选型，且 wheel 自带 announce 循环的例外被显式建模。
3. **调度"机制固定、策略分层叠加"**：核心只认 4 个队列操作；EDF 用半模比较器做到零额外时钟读取。
4. **PM 契约极窄且带补偿**：`next_state(cpu,ticks)` 单函数 + 退出延迟预扣 + 设备 use-count 两级 PM，全部有源码佐证。
5. **生态飞轮**：~半年一版、单版贡献者约 800-900 人、LTS 线并行；devicetree 使 4.4 单版新增 121 块板。
6. **安全的工程纪律**：CVE 全量公开（docs/security/vulnerabilities），syscall 验证层自动生成，`SPIN_VALIDATE` 运行时抓锁违规。

## 6. 不足与争议：不好在哪

1. **C 代码库的内存安全债**：OpenCVE 收录约 **270 个 CVE**，集中在蓝牙栈（Sweyntooth 2020、Defensics 2021 八连洞、USENIX'25 BLuEMan）；HN Security 人工审计持续挖出内存破坏漏洞——单映像形态放大了后果。
2. **全局就绪队列 + 单把调度自旋锁**：多核扩展性受限，亲和只有 mask 过滤没有负载均衡；PIN_ONLY 每核队列是事后补丁。
3. **策略不可运行时替换**：Kconfig choice 编译期定死，`MULTIQ` 与 `DEADLINE` 互斥暴露组合爆炸；无法支撑"运行中换调度策略"。
4. **Kconfig+DT+west 学习曲线与构建复杂度**：三套语言/工具链叠加，新人门槛高，配置组合爆炸难以全测。
5. **USERSPACE 是二等公民**：MPU 分区少、无页式隔离、权限是位图；启用后性能与内存开销明显（特权/用户双栈）。
6. **安全认证未完成**：IEC 61508 SIL 3 仍在推进（Safety WG），车载 ASIL 路径更远；Rust 支持仍实验性。

## 7. 对 BerryOS 的借鉴与警示（对照四支柱）

**微内核支柱**
- 借鉴：z_impl/z_vrfy 分层——BerryOS 每个 `berry_*` syscall 应有独立 verify 函数（handle 类型/权限检查先行），验证代码生成而非手写。
- 借鉴：内核对象 + 按线程权限位图证明"最小能力"在 RTOS 尺度可落地；BerryOS 用可传递 handle 更强，但位图可作 v1 简化参考。
- 警示：驱动/协议栈编进同一映像 = 270 个 CVE 的放大器；BerryOS 驱动/FS/网络留用户态是正确取舍，别因"性能方便"回退。

**省电支柱**
- 借鉴：PM 契约形状——内核在环上只问 PM 服务一个问题"next_state(cpu, ticks_left)"；`pm_policy_latency_request` 即用户态 QoS 流入电源决策的原语，可直接映射为 BerryOS AI 调度的护栏 ABI。
- 借鉴：进入低功耗前把唤醒点预扣 `exit_latency`；`SLOPPY_IDLE` 式"无事件则停表"开关；设备 runtime PM 的 use-count 模型（用户态驱动服务的电源引用计数）。
- 警示：Zephyr policy 是内核内回调，无"PM 崩溃兜底"问题；BerryOS 必须设计 PM 服务监督重启时内核侧降级路径，这是 Zephyr 给不了的参考。

**AI 原生调度支柱**
- 借鉴：EDF 半模 deadline 比较器 + `INT32_MAX/2` 钳位；meta-IRQ"必须回到被抢占协线程"的 RT 屏蔽语义（BerryOS RT 屏蔽可类比为对普通负载的 meta-IRQ）。
- 警示：Zephyr 编译期定策的痛点正是 BerryOS trait 运行时注入要解的题——别用静态配置（cargo feature 全量组合）模拟策略插拔，trait 接口要像 `_priq_run_*` 一样窄（入队/出队/取最优/yield）。

**PC 高性能支柱**
- 借鉴：directed IPI 与 IPI 空闲预约（唤醒指向空闲核占座）——BerryOS 负载均衡选核后的执行机构。
- 警示：全局队列 + 单锁是 Zephyr 的天花板不是范本；BerryOS 每核 runqueue + 负载均衡路线更对，但要记取其教训：选线程路径必须无跨核共享读、UP 的 cache 优化思想（缓存活络线程）值得保留为单核快路径。

**构建/驱动**
- 借鉴：`struct device` ROM 常量 + init 六级 + 依赖句柄，可平移为 BerryOS 用户态驱动的注册与启动排序模型；devicetree 的"板卡=数据"思想可用 Rust 宏/常量表实现。
- 警示：别引入 Kconfig 式第三套配置语言；cargo feature + 类型系统足以表达依赖约束且可静态检查。

## 8. 参考资料链接

- 源码：https://github.com/zephyrproject-rtos/zephyr （本文走读基于 main@4.5.0-rc1 快照：`kernel/timeout.c`、`kernel/sched.c`、`kernel/include/run_q.h`、`kernel/include/priority_q.h`、`subsys/pm/pm.c`、`include/zephyr/device.h`）
- 官网/成员：https://www.zephyrproject.org · https://www.zephyrproject.org/project-members
- 文档：https://docs.zephyrproject.org/latest/releases/index.html（版本节奏）· https://docs.zephyrproject.org/latest/kernel/services/smp/smp.html · https://docs.zephyrproject.org/latest/services/pm/index.html · https://docs.zephyrproject.org/latest/safety/safety_overview.html
- Rust 模块：https://github.com/zephyrproject-rtos/zephyr-lang-rust · 4.1 公告 https://zephyrproject.org/zephyr-rtos-4-1-is-available
- 治理/数据：Renesas/Wind River 升白金 https://www.linuxfoundation.org/press/zephyr-rtos-expands-ecosystem-with-renesas-and-wind-river-upgrading-to-platinum · 《Zephyr Turns 10》报告 https://www.linuxfoundation.org/hubfs/Research%20Reports/ZephyrTurns10_Report_Revised_051826.pdf · FOSDEM 2025 幻灯 https://archive.fosdem.org/2025/events/attachments/fosdem-2025-6311-zephyr-open-source-project-best-practices-over-time/slides/238153/202501_FO_hPOnqUM.pdf
- 设计讨论：CPU_MASK_PIN_ONLY https://github.com/zephyrproject-rtos/zephyr/issues/38844 · Safety FAQ https://github.com/zephyrproject-rtos/zephyr/wiki/Safety-FAQ
- 安全：https://docs.zephyrproject.org/latest/security/vulnerabilities.html · CVE 库 https://app.opencve.io/cve/?vendor=zephyrproject · BLuEMan https://www.usenix.org/system/files/usenixsecurity25-kao.pdf · HN Security 审计 https://hnsecurity.it/blog/ost2-zephyr-rtos-and-a-bunch-of-cves
- 4.4 发布：https://www.zephyrproject.org/zephyr-rtos-4-4-now-available-wireguard-wi-fi-direct-openrisc-and-more
