# FreeRTOS 与 Eclipse ThreadX 调研报告

> 调研日期 2026-10-15 · 基于公开资料与源码快照 · 许可证：两者均为 MIT

## 0. 一句话定位

FreeRTOS 是 AWS 主导、以"可配置宏宇宙 + 极广端口生态"统治中低端 MCU 的抢占式单核 RTOS（V11 起并入 SMP）；Eclipse ThreadX 是微软捐献 Eclipse 基金会、以"picokernel 无分层 + 每个 API 有界执行"著称的深嵌入式 RTOS——两者都是**无地址空间、函数调用即进内核的单核小内核**，与 BerryOS 的微内核形态相反，但其调度数据结构、tickless 省电链路与确定性纪律是教科书级参照。

## 1. 谁在做：组织/社区/治理/活跃度/商业背书

**FreeRTOS**：Richard Barry 2003 年创建（Real Time Engineers Ltd），2017 年加入 Amazon，V10 起转 MIT；此后由 AWS 主导开发与 LTS 维护（LTS 版本如 202406/202411，带安全补丁承诺）。版本节奏：V11.0.0（2023-11，SMP 并入主线）、V11.2.0（2025-03-04）、V11.3.0（2026-03-30，per-core 临界区嵌套与核间 yield 优化）、V11.3.1（2026-08-21）（[Release History](https://freertos.org/Documentation/04-Roadmap-and-release-note/02-Release-notes/00-Release-history)）。商业背书三层：AWS 参考集成、芯片厂商 SDK 全线内置（NXP/TI/ST/Espressif…）、以及 WHIS 的功能安全分支 **SafeRTOS**（TÜV SÜD 认证 IEC 61508-3 SIL 3、ISO 26262 ASIL D，2007 年首证，静态分配专用实现，与 FreeRTOS 同功能模型不同实现）。社区：论坛与 GitHub issues 长期活跃，多项嵌入式市场调查中稳居 RTOS 使用率第一梯队（具体百分比口径不一，待核实）。

**Eclipse ThreadX**：William E. Lamie 创建于 Express Logic（1997），2019 年微软收购改称 Azure RTOS，2023-11 捐献 Eclipse 基金会，2024-04 迁移完成，6.4.1 为首个完全 Eclipse 治理版本。最新版 6.4.2（2025-03-18，含 RISC-V QEMU 端口）与 6.5.1.202602（大版本：RISC-V 扩展 + Module Manager 修 9 个 CVE，[releases](https://github.com/eclipse-threadx/threadx/releases)）。GitHub 实测（2026-10 API）：eclipse-threadx/threadx 3542 stars，最后推送 2026-10-14，持续活跃。治理特色：**认证是产品**——ThreadX Alliance 会员制发放安全认证工件（计划 2026 认证 6.4.x 系列）；Eclipse SDV 工作组下设 ThreadX SIG 对接车载（与 AUTOSAR/COVESA/SOAFEE 协作）。部署量：微软 2021 口径 9.2B 设备，第三方现口径"超 12B"（待核实）。

## 2. 总体架构：内核形态、分层与组件

两者均为**单核 RTOS 内核**：无 MMU 地址空间、无 syscall 边界（内核与应用静态链接、函数直调、同一特权级，可选 MPU 保护），IPC 即内核同步原语本身。

- **FreeRTOS-Kernel**：核心仅 6 个 C 文件——`tasks.c`（调度，~9000 行）、`queue.c`（队列/信号量/互斥统一）、`list.c`（侵入式双链）、`timers.c`（软件定时器，靠守护任务）、`event_groups.c`、`stream_buffer.c`；加 `portable/MemMang/heap_1..5.c` 五选一的分配器与 `portable/<编译器>/<架构>/port.c + portmacro.h` 端口层（双层目录 = 编译器 × 架构正交）。聚合仓库 FreeRTOS/FreeRTOS 另含 demos/tests。
- **ThreadX**：`common/src/` 185 个 C 文件，按"对象 × 动词"一文件（`tx_queue_send.c`、`txe_queue_send.c`——txe_ 前缀是参数错误检查壳，与核心实现物理分离）；`common/inc/tx_api.h` 单头定义全部 API 与 `TX_THREAD` 等控制块；`ports/<arch>/<toolchain>/{inc,src,example_build}`；SMP 独立成 `common_smp + ports_smp` 双轨（arc_hs/cortex_a34/a35/a53/a72_smp 等，核数由端口 `tx_port.h` 宏配置）；`ports_module` 提供基于 MPU 的模块隔离。README 自述特性：picokernel 架构、preemption-threshold、event chaining。

## 3. 核心机制深挖

### 3.1 FreeRTOS 任务调度数据结构

核心是"按优先级分桶的链表数组 + 顶层优先级缓存"（`tasks.c:478`）：

```c
PRIVILEGED_DATA STATIC List_t pxReadyTasksLists[ configMAX_PRIORITIES ];
```

每级一条 FIFO 环链（同级轮转即时间片），入队即 `prvAddTaskToReadyList`（tasks.c:287-293）。选最高优先级两条路：通用法用 `uxTopReadyPriority` 高水位从上往下扫空链（tasks.c:199-211）；端口优化法把它当**就绪位图**，一条指令定位（portmacro.h:166-171）：

```c
#define portRECORD_READY_PRIORITY( uxPriority, uxReadyPriorities )  ( uxReadyPriorities ) |= ( 1UL << ( uxPriority ) )
#define portGET_HIGHEST_PRIORITY( uxTopPriority, uxReadyPriorities ) uxTopPriority = ( 31UL - ( uint32_t ) ucPortCountLeadingZeros( ( uxReadyPriorities ) ) )
```

事件等待链表复用 TCB 里的 `xEventListItem`，其 item value 存 `configMAX_PRIORITIES - 优先级`（反序，tasks.c:6766），使链表天然按"最该唤醒优先"排序。TCB（tasks.c:377-455）首成员强制是 `pxTopOfStack`（"THIS MUST BE THE FIRST MEMBER OF THE TCB STRUCT"——汇编按偏移 0 取栈指针），随后两个链表节点、`uxPriority`、可选的 `uxBasePriority/uxMutexesHeld`（优先级继承）、任务通知数组、TLS 指针；SMP 构建追加 `uxCoreAffinityMask` 与 `xTaskRunState`（-1 未运行 / -2 已标记让出 / ≥0 运行核号）。

### 3.2 PendSV/SVC/SysTick 三中断协作（Cortex-M 端口）

启动：`prvPortStartFirstTask`（port.c:278）重置 MSP 后 `svc 0` 触发 `vPortSVCHandler`（port.c:260）恢复第一个任务。之后所有切换走 **PendSV**（最低中断优先级，保证切换发生在无中断嵌套时）：SysTick 到期或任务阻塞让出都只"pend" PendSV，真正保存/恢复在其 handler 内完成（port.c:504-557 缩略）：

```asm
mrs r0, psp
ldr r3, =pxCurrentTCB
ldr r2, [r3]
tst r14, #0x10          /* EXC_RETURN bit4：是否带 FPU 上下文 */
it eq
vstmdbeq r0!, {s16-s31} /* 惰性 FPU 保存：按需 */
stmdb r0!, {r4-r11, r14}
str r0, [r2]
mov r0, #configMAX_SYSCALL_INTERRUPT_PRIORITY
msr basepri, r0         /* 只屏蔽可调 API 的中断，不关全局 */
bl  vTaskSwitchContext
...
```

三中断分工明确：SVC 只用一次（启动）、SysTick 只驱动 tick（`xPortSysTickHandler` 在最低中断优先级执行、直接调 `xTaskIncrementTick` 完成延时链唤醒与时间片轮转）、PendSV 专职切换；`basepri` 而非 `cpsid i` 做临界区，保证高优先级中断不被内核临界区拖住——中断延迟可控的关键设计。

### 3.3 tickless idle 全链路（直接对标 BerryOS NO_HZ）

五段式机制，层层可考：

1. **算预期空闲**（tasks.c:4002-4059）：`prvGetExpectedIdleTime()` = `xNextTaskUnblockTime - xTickCount`（下一个该醒任务的绝对时刻），若本核空闲层还有同级任务（要轮转）或更高优先级就绪则返回 0；
2. **门槛与钩子**（tasks.c:5980-6024）：≥ `configEXPECTED_IDLE_TIME_BEFORE_SLEEP`（默认 2 tick）才值得关 tick；挂起调度器后二次采样，`configPRE_SUPPRESS_TICKS_AND_SLEEP_PROCESSING` 允许应用否决；
3. **中止检查**：`eTaskConfirmSleepModeStatus()`（tasks.c:6045）查三件事——挂起期间就绪的 `xPendingReadyList` 非空、本核 `xYieldPendings` 置位、`xPendedTicks` 非零——任一命中返回 `eAbortSleep` 放弃入睡；
4. **端口层压 tick + 补偿**（port.c:589-790，weak 函数）：停 SysTick → 新装载值 = 剩余递减数 + 每 tick 计数 ×(预期-1)，若 IRQ 已 pending 还要再减一拍，并扣掉停表本身的 `ulStoppedTimerCompensation` → `configPRE_SLEEP_PROCESSING` → `wfi`；醒来分两路：被 tick 本身唤醒则重算本 tick 剩余量，被其他中断唤醒则按 SysTick 当前值折算整 tick 数。注释坦承"使用 tickless 必然带来内核时间与日历时间的微小漂移"；
5. **补 tick**：`vTaskStepTick()`（tasks.c:4665-4702）把 `xTickCount` 一次性跳到目标；若恰落在 `xNextTaskUnblockTime`，则只 `xPendedTicks++`，把真正的唤醒留给正常的 `xTaskIncrementTick()` 路径——补时与唤醒逻辑只有一份实现。

### 3.4 内存：heap_1..5

分配器不在内核里，是 `portable/MemMang/` 五个可换文件：heap_1 只分配不释放（"does NOT allow allocated memory to be freed"，最简、天然确定）；heap_2 best-fit 可释放不合并（已废弃）；heap_3 包一层编译器 malloc/free（临界区用 vTaskSuspendAll 保护）；heap_4 首次适应 + **相邻块合并**限碎片（事实默认）；heap_5 = heap_4 多段不连续内存区域。v9 起另有全静态分配路线（`xTaskCreateStatic`），SafeRTOS 即走此路。

### 3.5 queue 统一原语与优先级继承

`queue.c` 用一个 `Queue_t` 服务队列/二值/计数信号量/互斥/递归互斥（`xQueueGenericCreate` 的 type 参数分派），信号量 = 项长为 0 的队列；互斥在 union `u.xSemaphore` 里加 `xMutexHolder`。阻塞链两条（收/发各一）按优先序存任务。优先级继承在 `xTaskPriorityInherit`（tasks.c:6745）：持锁者优先级低于竞争者时临时抬升并搬移就绪链 + 更新事件项反序值，释放时 `xTaskPriorityDisinherit` 按记录的 `uxBasePriority` 回落（非传递、无优先级天花板、无死锁检测）。

### 3.6 FreeRTOS V11 SMP 与 SafeRTOS

SMP 进主线（`configNUMBER_OF_CORES`、`pxCurrentTCBs[]` 数组），选任务函数变为 `prvSelectHighestPriorityTask(xCoreID)`（tasks.c:994）：先把当前任务挪到本级链表尾（保证公平），再从 `uxTopReadyPriority` 向下扫，跳过 `xTaskRunState != -1`（他核在跑）与亲和位图不匹配的任务。耐人寻味的是默认 `configRUN_MULTIPLE_PRIORITIES == 0`：**任何核不得运行比另一核在跑任务更低优先级的任务**（tasks.c:1033-1039 显式降级到 idle）——为单核应用语义兼容而牺牲并行度。V11.3.0（2026-03）才补 per-core 临界区嵌套与核间唤醒优化。SafeRTOS 则是另一个极端：静态分配、无 malloc API、同功能模型独立实现 + TÜV 认证工件商业化。

### 3.7 ThreadX 调度循环与确定性设计

核心是一对全局指针：`_tx_thread_current_ptr`（在跑）与 `_tx_thread_execute_ptr`（该跑）。**所有调度决策在 C 路径算好写进 execute_ptr**（如 `_tx_thread_system_suspend`，tx_thread_system_suspend.c:300-400）：

```c
priority_map = _tx_thread_priority_maps[MAP_INDEX];      /* 就绪位图，0=最高优先级 */
if (priority_map == 0) { _tx_thread_execute_ptr = TX_NULL; ... }  /* 空转 */
else {
    TX_LOWEST_SET_BIT_CALCULATE(priority_map, priority_bit)  /* 32 优先级一张图，>32 级再加 active 图两级查找 */
    _tx_thread_highest_priority = base_priority + priority_bit;
}
_tx_thread_execute_ptr = _tx_thread_priority_list[_tx_thread_highest_priority];
```

汇编层（Cortex-M 为 PendSV handler，其他架构为真·`_tx_thread_schedule` 轮询循环）只负责把 current 同步到 execute。首次进入调度（tx_thread_schedule.S:78-112）由 `tx_initialize_kernel_enter` 落到 `_tx_thread_schedule`：清 `_tx_thread_preempt_disable`、pend PendSV 后原地自旋等待接管——此后 Cortex-M 上线程"返回系统"不再走 C 函数，而是直接 pend PendSV 回到同一段汇编，切换路径唯一。**时间片也搬进切换汇编**：全局 `_tx_timer_time_slice` 在保存现场时按偏移 24 写回被换出线程的 `tx_thread_time_slice` 并清零全局，恢复现场时再把新线程的时间片装回全局（tx_thread_schedule.S:168-183、214-221）——时间片状态随线程走，定时器中断只需递减一个全局量。空闲路径（tx_thread_schedule.S:251-296）值得精读：`TX_LOW_POWER` 钩子对 `tx_low_power_enter/exit`，`TX_ENABLE_WFI` 时先用 PRIMASK 关全局再清 BASEPRI——**让被 BASEPRI 屏蔽的中断也能唤醒 WFI**，醒来重查 execute_ptr 循环；若醒来时已有新线程就绪，还要清掉新 pend 的 PendSV 位（__tx_ts_ready，S:301-304）避免二次切换。独有机制 **preemption-threshold**：`TX_THREAD.tx_thread_preempt_threshold` 声明"低于阈值不得抢占我"，内核用独立 `_tx_thread_preempted_maps` 位图记录被 threshold 保护的被抢线程，是介于关中断与裸优先级之间的第三种保护粒度。确定性纪律落到实处：每个 API 文件头部固定 DESCRIPTION/INPUT/OUTPUT/CALLS/CALLED BY 注释块；搜索类循环显式有界（byte_pool 搜索以 `examine_blocks = fragments + 1` 封顶，tx_byte_pool_search.c:120）；`TX_NOT_INTERRUPTABLE` 构建选项省去逐 API 的中断开销；另有全 MISRA 版（tx_misra.c）。

### 3.8 ThreadX 内存与互斥

两类池：`TX_BYTE_POOL` 按字节（首次适应、可碎片理论估算 `available + (fragments-2)*overhead`，供 ISR 禁用）、`TX_BLOCK_POOL` 定长块（分配 O(1)，ISR 可用）。互斥的优先级继承是**每把锁可选开关**（`tx_mutex_inherit` 创建时决定），实现记录 `tx_mutex_highest_priority_waiting` 与持有者 `tx_thread_inherit_priority`，抬升走 `_tx_mutex_priority_change`（tx_mutex_get.c:285-310）；优先级 0 最高，故比较方向与 FreeRTOS 相反。

## 4. 源码走读：目录与关键文件

```
FreeRTOS-Kernel/                     eclipse-threadx/threadx/
├── tasks.c        调度/TCB/tickless ├── common/src/   185 个 tx_*.c + txe_*.c
├── queue.c        统一原语+锁继承    │   ├── tx_thread_system_suspend.c  调度决策核心
├── list.c         侵入式环链         │   ├── tx_queue_send.c / tx_mutex_get.c
├── timers.c       软定时器+守护任务  │   └── tx_byte_pool_search.c  有界搜索
├── event_groups.c / stream_buffer.c ├── common/inc/tx_api.h  TX_THREAD 全字段
├── portable/MemMang/heap_1..5.c     ├── common_smp/ + ports_smp/  SMP 双轨
└── portable/GCC/ARM_CM4F/           └── ports/cortex_m4/gnu/src/tx_thread_schedule.S
    ├── port.c      PendSV/SVC/tickless 调度循环与 WFI 空闲
    └── portmacro.h CLZ 位图/临界区宏  ports_module/  MPU 模块隔离
```

关键数据结构对照：`TCB_t`（首成员 pxTopOfStack 偏移约定）vs `TX_THREAD`（tx_api.h:423-599，前段注释明言"referenced by the port-specific assembly"——两者用同一招：**汇编只依赖控制块固定前缀**）。

## 5. 亮点：好在哪

1. **FreeRTOS tickless 是教科书级机制/策略分离**：期望空闲由 tasks.c 算（策略），压表/补偿在 port.c weak 函数（硬件机制），`configPRE/POST_SLEEP_PROCESSING` 再留应用层钩子——三层各可替换。
2. **FreeRTOS 中断协作干净**：SVC/SysTick/PendSV 三中断各司其职 + basepri 局部屏蔽，临界区不放大高优先级中断延迟（port.c:504/560 有注释论证）。
3. **FreeRTOS 位图+CLZ 选优先级 O(1)**（portmacro.h:166-171），且与通用高水位法可编译切换，正确的端口可移植姿势。
4. **queue 统一原语**把五种同步对象收敛到一个数据结构与一条等待/唤醒路径，API 面与代码量双降——BerryOS `object_*` 合并思路的成熟先例。
5. **ThreadX 确定性是制度不是口号**：注释块规范、txe_ 错误检查与实现分离、循环有界封顶、TX_NOT_INTERRUPTABLE/MISRA 构建选项——"每个 API 有界执行"可审计。
6. **ThreadX current/execute 双指针 + C 决策/汇编执行**：调度决策全部集中到 system_suspend/resume 一处，上下文切换汇编零判断逻辑，端口移植面极小。
7. **ThreadX 商业与认证体系完整**：2KB ROM/1KB RAM 最小占用 + SIL4/ASIL D 认证 + Alliance 会员发放认证工件，安全关键路径有闭环保付。
8. **控制块前缀 ABI 纪律**（两者一致）：汇编只碰固定偏移字段并写进注释，接口契约用布局而非文档保证。

## 6. 不足与争议

1. **无地址空间隔离**：内核与应用同特权级同镜像，任何任务野指针可毁内核；FreeRTOS 靠 MPU 端口、ThreadX 靠 ports_module 补，均为可选项而非默认（两仓库结构可证）。
2. **FreeRTOS 全局临界区/全局就绪结构不适应多核**：SMP 版以大自旋锁 + 单一 uxTopReadyPriority 起步，默认 `configRUN_MULTIPLE_PRIORITIES=0` 压制并行度换单核语义兼容（tasks.c:1031-1041），V11.3 才逐步细化——多核是补丁而非设计。
3. **优先级继承实现单薄**：FreeRTOS 非传递继承、无死锁保护、互斥不可用于 ISR；历史上还有 2021 年 Zimperium 披露 13 个漏洞、与 AWS 沟通不畅的治理争议（待核实细节）。
4. **ThreadX byte pool 碎片与吞吐**：按字节池首次适应 O(fragments) 搜索，深碎片下变慢，官方也只建议 ISR 用 block pool——内核级堆的老问题。
5. **ThreadX 单核与 SMP 是两套源码树**（common vs common_smp），同步成本高；社区规模与 FreeRTOS 生态差距明显（3.5k stars vs 芯片厂全线内置）。
6. **tickless 漂移被注释明认**（port.c:616-619），累积补偿而非绝对重编程，长期运行时钟走偏。

## 7. 对 BerryOS 的借鉴与警示

**支柱① 微内核**：借——控制块前缀 ABI（上下文切换 asm 只按固定偏移取 `pxTopOfStack`/`tx_thread_stack_ptr`，BerryOS 切换 asm 应同样冻结 TCB 前缀布局并注释声明）；借 queue 统一原语支撑 `object_*` 合并。警示——别学"单镜像无隔离"与"服务即内核函数"；FS/网络在用户态的 BerryOS 路线与两者根本相反，勿因它们代码简洁而动摇。

**支柱② 省电（最直接的财富）**：借五件套——(a) 期望空闲 = 下一唤醒绝对时刻 − 当前时刻（tasks.c:4054-4055），BerryOS 已按 next-event 重编 CNTV，可补上"同级就绪任务在轮转则不入睡"的短路检查；(b) 入睡前三查（pending ready/yield pending/pended tick）作 BerryOS idle 入睡护栏；(c) 补时与唤醒共用单一路径（vTaskStepTick 的 `xPendedTicks++` 技巧，避免两套唤醒逻辑漂移）；(d) `configPRE/POST_SLEEP_PROCESSING` 钩子即 BerryOS 内核↔用户态 PM 服务的接缝，方向正确但 BerryOS 应走共享环形缓冲而非函数回调；(e) ThreadX 清 BASEPRI 保 WFI 可被屏蔽中断唤醒的细节，对应 BerryOS mask CNTV 后 wfi 的中断掩码次序。警示——学 ThreadX TX_LOW_POWER 钩子够用但太粗；学 FreeRTOS 累积补偿会漂移，BerryOS 应每次以绝对截止时间重编定时器。

**支柱③ AI 原生调度**：两者皆无对应物，正说明 BerryOS 的差异化真实存在。可借三形——(a) ThreadX preemption-threshold 语义（"低于 N 不准抢我"）可直接表达 AI 调度的 RT 屏蔽护栏，且其 `_tx_thread_preempted_maps` 位图记账给了实现参照；(b) FreeRTOS 每任务通知（32bit 值轻量信箱，无队列开销）是内核↔策略服务遥测/指令通道的低成本形态；(c) 两者 trace 宏/性能计数全编译期可选（`traceMOVED_TASK_TO_READY_STATE`、`TX_THREAD_ENABLE_PERFORMANCE_INFO`）印证 BerryOS 可观测性"静态点 + 零成本关闭"的 day-one 设计。

**支柱④ PC 高性能**：借——TCB 内 `uxCoreAffinityMask` 位图与 `xTaskRunState` 核号字段设计，可平移进 BerryOS capacity 框架（亲和掩码与 EPP hint 同级放在任务元数据）。警示——FreeRTOS SMP 的教训清单：全局 uxTopReadyPriority + 默认 RUN_MULTIPLE_PRIORITIES=0 是"单核思维上 SMP"的反面教材，BerryOS 每核 runqueue + 负载均衡的起点比它高，不要为了单核语义兼容回头加全局锁。

**哲学取舍**：FreeRTOS（可配置生态）与 ThreadX（极简确定）是同一问题的两个极端解，且都用**编译期宏**实现可配置性。BerryOS"机制在内核、策略可插拔 trait + 用户态策略服务"实际是第三条路：机制学 ThreadX 的小而确定（每个 syscall 有界、注释即契约），策略接口学 FreeRTOS 的可配置广度，但把配置点从编译期搬到运行期（用户态服务可热替换）——这是对两者共同短板的回答。

## 8. 参考资料链接

- FreeRTOS 源码：https://github.com/FreeRTOS/FreeRTOS-Kernel （tasks.c / queue.c / portable/MemMang/ / portable/GCC/ARM_CM4F/）
- FreeRTOS Release History（V11.0–V11.3.1 日期）：https://freertos.org/Documentation/04-Roadmap-and-release-note/02-Release-notes/00-Release-history
- FreeRTOS SMP 文档：https://www.freertos.org/Documentation/02-Kernel/02-Kernel-features/13-Symmetric-multiprocessing-introduction
- FreeRTOS tickless 官方说明：https://www.freertos.org/low-power-tickless-idle.html
- SafeRTOS（WHIS，认证与静态分配）：https://www.highintegritysystems.com/safertos
- Eclipse ThreadX 源码：https://github.com/eclipse-threadx/threadx （common/src、ports/cortex_m4/gnu/src/tx_thread_schedule.S、CHANGELOG.md）
- ThreadX releases（6.4.1 治理迁移、6.4.2、6.5.1.202602）：https://github.com/eclipse-threadx/threadx/releases
- Azure RTOS→Eclipse 迁移完成公告：https://techcommunity.microsoft.com/blog/iotblog/azure-rtos-transition-to-open-source-is-now-complete/4105027
- ThreadX SMP 文档：https://github.com/eclipse-threadx/rtos-docs/blob/main/rtos-docs/threadx/threadx-smp/chapter1.md
- ThreadX Alliance 认证计划：https://threadxalliance.org/subscription/benefits ；Eclipse SDV ThreadX SIG：https://eclipsesdv.org/threadx
