# OpenHarmony 内核体系 调研报告

> 调研日期 2026-10-15 · 基于公开资料与源码快照（Gitee master 分支 2026-10 快照）· 许可证：kernel_liteos_m = BSD-3-Clause；kernel_liteos_a = BSD-3-Clause；drivers_hdf_core/framework = GPLv2 与 BSD-3 双许可（任选）；communication_dsoftbus = Apache-2.0；kernel_linux_5.10 / kernel_linux_6.6 = Linux 上游 GPL-2.0（仓库含标准 Linux 目录与 LICENSES/）

## 0. 一句话定位

OpenHarmony 是开放原子开源基金会孵化、华为主导贡献的全场景开源操作系统，其内核层不是单一内核，而是一套**按设备资源分档的三内核矩阵**：轻量系统用 LiteOS-M（无 MMU 的 MCU RTOS 内核）、小型系统用 LiteOS-A（32 位宏内核，带 MMU/多进程）或 Linux、标准系统用 Linux（5.10/6.6 分支），三者之上用统一的用户态框架（ArkUI/分布式软总线/HDF 驱动框架）拉平差异——**内核选型服从于"同一套应用框架覆盖百 KB 到 GB 级设备"这一产品目标**，而非服从某种内核哲学。

## 1. 谁在做：组织/社区/治理/活跃度/商业背书

- **治理**：OpenHarmony 项目群由开放原子开源基金会（OpenAtom Foundation）孵化运营，代码 2020 年 9 月由华为捐赠（Gitee 仓库创建时间 2020-08-27，API 可查），企业按 A/B/C 类捐赠人分层参与。据基金会理事长谢少锋在鸿蒙生态大会 2026 披露，项目已于 **2025 年从基金会孵化毕业**，六年间完成 11 个版本迭代、代码超 1.4 亿行、13808 名贡献者（openatom.org 报道）。
- **华为的角色**：华为是最大代码贡献者（自述贡献 1.3 亿+ 行、生态伙伴 523 家），同时商业版 HarmonyOS/HarmonyOS NEXT 与开源版并行发展——**商业版所用的"鸿蒙微内核"未在 OpenHarmony 开源**，这是社区长期争议点（见 §6）。
- **版本节奏**：每年 Q1/Q3 两个 Release，每两年一个 LTS。近期节点：6.0 Release 2025-09-06、6.1 Beta 2026-01、6.1 Release 2026-03、6.1 LTS 2026-06-30，官方路线图预告 7.X 于 2026 年 9 月商用（gitee.com/openharmony/release-management 的 OpenHarmony-RoadMap.md）。
- **生态数据**：开放原子官网（2026-04）：超 13 亿台生态设备、1700 余款兼容性测评产品、13000 余名贡献者；工信部口径（2025-07，转引自中国科学报）：设备超 11.9 亿台、720 万开发者、2.5 万上架应用。
- **仓库活跃度**（Gitee API，2026-10-15 实测）：kernel_liteos_m 807 star / 595 fork，最近更新 2026-10-13；kernel_liteos_a 2280 star / 1082 fork，最近更新 2026-09-30——两个内核仓都仍在持续演进（LiteOS-M 近期还在合入 RISC-V nuclei、cortex-m55 等架构适配）。

## 2. 总体架构：内核形态、分层与组件

官方文档（docs 仓 `zh-cn/device-dev/kernel/kernel-small-overview.md`）明确三形态与选型边界：

| 形态 | 内核 | 目标设备 | 关键能力 |
|---|---|---|---|
| 轻量系统（mini） | LiteOS-M | MCU，百 KB 内存 | 无 MMU/无用户态，可选 MPU 隔离，对标 FreeRTOS/ThreadX |
| 小型系统（small） | LiteOS-A（或 Linux） | M 级内存 | MMU、多进程、每进程独立 4GiB 地址空间，官方自比"业界类似 Zircon 或 Darwin" |
| 标准系统（standard） | Linux 5.10/6.6 | 手机/平板/PC | 完整 Linux + OpenHarmony 用户态框架 |

内核形态的诚实描述是：**LiteOS-M 是单核抢占式 RTOS 内核（无地址空间隔离），LiteOS-A 是带用户态进程的宏内核，Linux 是宏内核**。文档原文甚至主动说明："轻量级内核的文件系统、网络协议等扩展功能（**没有像微内核那样运行在用户态**）运行在内核地址空间，主要考虑组件之间直接函数调用比进程间通信或远程过程调用要快得多。"——这是对"鸿蒙微内核"宣传最重要的官方自证（见 §6）。

跨三形态共享的统一层：**HDF 驱动框架**（一次开发多系统部署）、**KAL 内核抽象层**（LiteOS-M 上是 CMSIS-RTOS2 适配，LiteOS-A 上是 1200+ POSIX 接口）、以及用户态的分布式软总线/ArkUI 等子系统。

## 3. 核心机制深挖

### 3.1 LiteOS-M 调度器：位图优先级 + 单次触发定时器（tickless）

源码 `kernel/src/los_sched.c`（624 行，单核）：

- **32 级优先级 × 32 位位图 O(1) 派发**：`g_priQueueList[32]` 双向链表数组 + `g_queueBitmap` 位图，取任务用 `CLZ`（前导零计数）一条指令找最高优先级非空队列：

```c
// kernel/src/los_sched.c
#define OS_PRIORITY_QUEUE_NUM      32
#define PRIQUEUE_PRIOR0_BIT        0x80000000U
STATIC LOS_DL_LIST g_priQueueList[OS_PRIORITY_QUEUE_NUM];
STATIC UINT32 g_queueBitmap;

LosTaskCB *OsGetTopTask(VOID)
{
    LosTaskCB *newTask = NULL;
    if (g_queueBitmap) {
        priority = CLZ(g_queueBitmap);
        newTask = LOS_DL_LIST_ENTRY(...&g_priQueueList[priority]..., LosTaskCB, pendList);
    } else {
        newTask = OS_TCB_FROM_TID(g_idleTaskID);   // 空闲任务兜底
    }
```

- **同优先级时间片轮转**：时间片以时钟周期数计（`SchedTimeBaseInit()` 中 `g_schedTimeSliceMin` = 最小 50 微秒），唤醒入队时若剩余时间片未耗尽插队头、耗尽则重置后插队尾（`OsSchedTaskEnQueue`），实现"优先级抢占 + 同级 RR"。
- **tickless 是 day-one 设计而非补丁**：没有周期性 tick 中断，定时器按需单次编程。`OsSchedSetNextExpireTime()` 取超时排序链表（sortlink）最近到期时间与当前任务时间片终点两者的较小值重装载定时器；空闲时把响应时间推到 `OS_SCHED_MAX_RESPONSE_TIME - g_tickResponsePrecision`。精度按最小 tick 周期的 75% 留裕量，避免频繁重编程抖动。
- **DVFS 感知的时间记账**：调频时 `OsSchedTimeConvertFreq()` 把所有任务的剩余时间片、startTime 从旧频率换算到新频率——时间片以"周期数"而非"tick 数"记账，改频不打断公平性。这对 BerryOS 的省电+AI 调频支柱是可直接借鉴的细节。
- **架构可换 tick 源**：`LOS_TickTimerRegister()` 允许芯片方注册自己的 `ArchTickTimer`（init/reload/start/stop），内核调度逻辑完全不动。

### 3.2 LiteOS-M 电源管理：内核内 PM 框架 + 任务冻结

`components/power/los_pm.c`（785 行，组件可裁剪）：定义 `LOS_SYS_NORMAL_SLEEP/LIGHT_SLEEP/DEEP_SLEEP/SHUTDOWN` 四档，PM 节点（tick、设备）用 `LOS_PmRegister()` 注册回调；进睡眠前 `OsPmTickTimerStop()` 停掉系统 tick，调度器配套提供 `OsSchedFreezeTask/OsSchedUnfreezeTask()`——把延迟/超时任务从 sortlink 摘下冻结（`OS_TASK_FLAG_FREEZE`），醒来后按剩余 tick 重新入链，**深睡期间不产生任何定时器中断**。`OsIsPmMode()` 被调度器在计算下次到期时间时查询，避免 PM 模式下被任务超时唤醒。

### 3.3 LiteOS-M 内存：TLSF 动态分配 + 静态盒 + 全静态 TCB 池

- `kernel/src/mm/los_memory.c`（2311 行）：**TLSF（两级分离适配）**算法，一级/二级索引均用位图 + `CLZ` 查找（`OsMemFFS/OsMemFLS`），支持多内存池（`LOSCFG_MEM_MUL_POOL`）、池可扩展、可配无锁属性（`OS_MEM_POOL_UNLOCK_ENABLE`）。
- `kernel/src/mm/los_membox.c`：静态定长内存盒（membox），固定尺寸块 + 魔数校验，面向确定性场景。
- 任务控制块不用堆分配：`g_taskCBArray` 静态数组 + `OS_TCB_FROM_TID(taskID)` 宏直接索引（`kernel/include/los_task.h:1467`），上限由 `LOSCFG_BASE_CORE_TSK_LIMIT` 编译期定死——MCU 上杜绝了 TCB 分配失败路径。

### 3.4 LiteOS-M 裁剪机制与 CMSIS 适配

- 根 `Kconfig` 实测 78 个 config 项（编译器 choice、平台 choice、`KERNEL_BACKTRACE/KERNEL_CPPSUPPORT/BASE_CORE_CPUP` 等，多数 default n），配 GN/Ninja 组件化构建，最小系统可裁到极小体积；组件在 `components/`（cpup/backtrace/fs(fatfs/littlefs)/net(lwip 2.1)/shell/trace/security/lmk…）按需拼装。
- `kal/cmsis/cmsis_liteos2.c`（1820 行）把 CMSIS-RTOS2 API 完整映射到 LOS_*（`osThreadNew` → `LOS_TaskCreate`、`osDelay` → 空转前用 `HalDelay`），让存量 MCU 中间件零改动迁移——这是 LiteOS-M 生态铺开的关键一步棋。

### 3.5 LiteOS-A：进程模型与虚拟内存

- 进程控制块 `kernel/base/include/los_process_pri.h` 的 `LosProcessCB`：`processMode`（内核 0/用户 1）、`threadGroup`（进程=线程组，基于 Task 实现）、`vmSpace`、`ipcInfo`（LiteIPC 私有池）、`files`、`user/capability`（`LOSCFG_SECURITY_CAPABILITY`）、container/plimits 等全部 `#ifdef LOSCFG_*` 条件编译——**裁剪不是删目录而是编译期条件化，同一个结构体在小型系统上可瘦身**。
- 地址空间（文档明确）：32 位系统，内核静态映射 0–1GiB，用户空间 1–4GiB，每进程独立。`kernel/base/vm/` 有完整 VM 栈：`los_vm_map.c`（region 管理、`LOS_VmSpaceClone` 支持 fork 式复制）、`los_vm_fault.c`（缺页/demand paging）、`los_vm_scan.c`+`oom.c`（页面回收）、`shm.c`、filemap/iomap。
- 用户态启动：`kernel/user/src/los_user_init.c` 里内核用内联汇编 svc（r7=0x900001）execve `/bin/init`——内核态直接发起第一个系统调用拉起 PID 1，无独立 boot 服务。

### 3.6 LiteOS-A 调度器：每核 runqueue + HPF/EDF 双策略

`kernel/base/include/los_sched_pri.h`：`SchedRunqueue g_schedRunqueue[LOSCFG_KERNEL_CORE_NUM]` 每核一份，含 HPF（高优先级抢占，FIFO/RR）与 EDF（deadline）两个 runqueue；HPF 用**两级位图**（32 组 × 32 级），派发时外层 `CLZ` 找组、内层 `CLZ` 找级，并在同优先级遍历中内联检查 SMP 亲核掩码（`cpuAffiMask`）——找"最高优先级且允许跑在本核"的任务：

```c
// kernel/base/include/los_sched_pri.h
typedef struct {
    HPFQueue queueList[OS_PRIORITY_QUEUE_NUM];
    UINT32   queueBitmap;
} HPFRunqueue;
...
while (baseBitmap) {
    UINT32 basePrio = CLZ(baseBitmap);
    UINT32 bitmap = queueList->queueBitmap;
    while (bitmap) {
        UINT32 priority = CLZ(bitmap);
        LOS_DL_LIST_FOR_EACH_ENTRY(newTask, &queueList->priQueList[priority], LosTaskCB, pendList) {
#ifdef LOSCFG_KERNEL_SMP
            if (newTask->cpuAffiMask & (1U << cpuid)) { return newTask; }
```

策略类（FIFO/RR/DEADLINE/IDLE）在 `kernel/base/sched/` 下分文件实现（`los_priority.c`、`los_deadline.c`、`los_idle.c`），SMP 配套在 `kernel/base/mp/`（spinlock/lockdep/percpu/跨核 IPI）。调度策略与调度框架的分离思路与 BerryOS 的"机制在内核、策略 trait 可插拔"同构，但 LiteOS-A 用的是 C 函数表+编译期 switch 而非 trait 对象。

### 3.7 LiteIPC：作为字符设备的轻量 IPC

`kernel/extended/liteipc/hm_liteipc.c`（1385 行）：LiteIPC **以 misc 字符设备形态实现**（`register_driver(LITEIPC_DRIVER, &g_liteIpcFops, ...)`，即 `/dev/liteipc`），用户态通过 open/ioctl/mmap 使用。核心机制：

- 每进程一块 IPC 专用内存池，mmap 进用户空间（映射时打 `VM_MAP_REGION_FLAG_LITEIPC` 标志），消息在池内零拷贝传递；
- 消息可携带三种 `SpecialObj`：`INV`/`FD`（文件描述符）、`PTR`（共享内存偏移）、`SVC`（服务句柄）——**SVC 句柄是能力式服务寻址的雏形**（`USE_TASKID_AS_HANDLE` 时直接用任务 ID 当句柄，`g_serviceHandleMap` 表管理）；
- 发送阻塞超时 5000ms（`LITEIPC_TIMEOUT_MS`），失败路径有完整回滚（`HandleSpecialObjects(..., isRollback=TRUE)`）。

### 3.8 HDF 驱动框架：一次开发、多内核部署，但 LiteOS 上驱动在内核态

`drivers_hdf_core`（framework 双许可 GPLv2/BSD-3）由 `framework/core`（驱动加载、DeviceManager、DeviceHost 设备容器、PnPManager）、`framework/model`（audio/display/input/sensor/network/misc 驱动模型）、`adapter`（LiteOS-M、LiteOS-A、用户态接口库三套适配）组成。事实分层：

- **LiteOS-M / LiteOS-A 上：HDF 驱动编译进内核镜像、跑在内核态**（LiteOS-M 根本没有用户态）；
- **标准系统（Linux）上：可选两种部署**——内核态（ko）或**用户态 Host 进程**，文档（`driver-hdf-manage.md`）给出用户态驱动的 uid/gid/Linux capabilities 配置项与"异常恢复（用户态驱动）"机制，并支持按需加载（`preload=2` 时用户态首次 `HdfIoServiceBind` 触发动态加载）；用户态与内核态驱动间有统一消息机制。
- 与 BerryOS"驱动全用户态服务"对照：HDF 证明了**驱动框架的平台抽象价值（配置化描述设备树 HCS、Host 容器分组、驱动模型分层）与部署位置是正交的**——同一套驱动模型可落在内核态或用户态。OpenHarmony 在资源极小的设备上选择内核态是务实，但也继承了宏内核的失败隔离弱点。

### 3.9 分布式软总线（简述）

`communication_dsoftbus`（Apache-2.0）：用户态组件，`core/` 下 `authentication`（认证）、`bus_center`（组网/拓扑）、`connection`（WiFi/蓝牙等链路）、`transmission`（消息/字节/流/文件四类通道）四层，对上层提供"不区分链路"的发现-组网-传输 API。与内核无耦合，是"内核形态可换、分布式能力放用户态"这一架构选择的直接受益者——这对 BerryOS 的 IPC 命名空间设计有参考意义：跨设备通信不需要内核支持，只需要稳定的本地 IPC 底座。

## 4. 源码走读：目录结构、关键文件、关键数据结构

**kernel_liteos_m**（BSD-3-Clause，2026-10 快照）：

```text
kernel_liteos_m/
├── arch/            # arm(cortex-m3/4/7/33/55,arm9)/risc-v(riscv32,nuclei)/csky v2/xtensa lx6
│   └── arm/common/los_common_interrupt.c   # g_hwiForm[] 中断向量表数组 + OsSetVector 注册
├── kernel/
│   ├── include/     # los_task.h(1799行,LosTaskCB) los_sched.h los_memory.h los_membox.h ...
│   └── src/         # los_sched.c(624) los_task.c(1545) los_sortlink.c(171) los_swtmr.c
│       └── mm/      # los_memory.c(2311,TLSF) los_membox.c(316,静态盒)
├── components/      # power/los_pm.c(785) cpup backtrace fs(fatfs/littlefs) net(lwip-2.1)
│                    # shell trace security(syscall/box) signal lmk lms ...
├── kal/             # cmsis/cmsis_liteos2.c(1820,CMSIS-RTOS2→LOS_*) libc
├── drivers/  utils/  tools/  testsuites/   # + 根 Kconfig(78 个 config 项)
```

**kernel_liteos_a**（BSD-3-Clause）：

```text
kernel_liteos_a/
├── kernel/
│   ├── base/core/    # los_process.c(2468) los_task.c los_smp.c
│   ├── base/sched/   # los_sched.c los_priority.c los_deadline.c los_idle.c los_sortlink.c
│   ├── base/mp/      # los_spinlock.c los_lockdep.c los_percpu.c los_mp.c
│   ├── base/vm/      # los_vm_map.c(1212) los_vm_fault.c los_vm_scan.c oom.c shm.c
│   ├── base/ipc/     # 信号量/互斥锁/队列/futex/rwlock（内核内核内 IPC）
│   ├── extended/     # liteipc/hm_liteipc.c(1385) container pipes vdso power perf trace lms plimit
│   ├── common/       # los_config.c console.c virtual_serial.c main.c
│   └── user/src/     # los_user_init.c（内核 svc execve /bin/init）
├── syscall/          # syscall_lookup.h（实测 203 个 SYSCALL_HAND_DEF，含 fork/vfork/clone/execve）
├── fs/               # vfs/(vnode.c path_cache.c mount.c bcache epoll) fat jffs2 nfs ramfs romfs procfs rootfs
├── compat/ bsd/ net/ security/ shell/ platform/ drivers/
```

关键数据结构速查：`LosTaskCB`（los_task.h:1475，stackPointer/taskStatus/priority/timeSlice/sortList/pendList 静态池索引）；`LosProcessCB`（los_process_pri.h，条件编译字段）；`HPFRunqueue`/`EDFRunqueue`/`SchedRunqueue`（los_sched_pri.h）；`IpcMsg/SpecialObj/ProcIpcInfo`（hm_liteipc.c 与 `kernel/base/include/los_ipc_pri.h`）；`OsMemPoolHead/OsMemNodeHead`（los_memory.c TLSF 池）。

## 5. 亮点：好在哪

1. **按资源分档的三内核矩阵 + 统一框架**，用 HDF/KAL/软总线把内核差异对上层抹平，产品定义（全场景）驱动架构，工程上极其务实（docs 内核概述 + 各仓库 README 可证）。
2. **LiteOS-M 的 tickless 是设计内生的**：单次触发定时器 + sortlink 超时链 + 75% 精度裕量 + `LOS_TickTimerRegister` 可换 tick 源，比很多"事后关 tick"的 RTOS 干净得多（los_sched.c/los_tick.c 源码可证）。
3. **DVFS 感知的时间片记账**（`OsSchedTimeConvertFreq` 按周期数记账）与 **PM 任务冻结**（freeze/unfreeze + `OsIsPmMode`），把"省电"做进了调度器本体，机制间耦合度低。
4. **TLSF 内存分配**（FLS/FFS 位图 + CLZ，O(1) 分配/释放、低碎片）是 MCU 上接近最优的选择，且池属性（扩展/无锁）可配。
5. **裁剪体系彻底**：Kconfig 78 项 + GN 组件化 + 结构体字段级 `#ifdef`，同一套代码覆盖从百 KB 到 M 级设备；CMSIS-RTOS2 适配层让存量生态零成本迁入。
6. **LiteOS-A 每核 runqueue + 两级位图 + EDF/deadline 类 + 亲核掩码内联检查**，是教科书级的单文件可读完的 SMP 实时调度实现（los_sched_pri.h 中 `HPFRunqueueTopTaskGet` 40 行内完成全部选核选任务逻辑）。
7. **HDF 驱动模型与部署位置解耦**、配置驱动（HCS）描述设备、按需加载、用户态驱动带 uid/gid/caps 与异常恢复——驱动框架的平台抽象层设计值得任何 OS 借鉴。
8. **治理与生态势能**：基金会托管 + 6 年 11 版 + 13 亿台设备 + 720 万开发者，证明"分层分档 + 统一框架"路线的商业可行性；内核仓 BSD-3-Clause 对商业嵌入友好。

## 6. 不足与争议：不好在哪

1. **"微内核"宣传与开源现实的落差是最大争议**：2019 年发布会以"微内核"为核心卖点，但开源的 LiteOS-A 文档自认"文件系统、网络协议等扩展功能没有像微内核那样运行在用户态"（kernel-small-overview.md 原文）；手机版 HarmonyOS 2.x 被逆向分析认定基于 Linux+AOSP（看雪论坛等）；真正的鸿蒙微内核仅存在于未开源的 HarmonyOS NEXT。技术社区（如"不吹不擂，一文看懂鸿蒙"）普遍结论：**LiteOS-A 按业界分类是宏内核**。
2. **驱动在 LiteOS 上全在内核态**（连同 shell、lwIP、VFS、FAT），单个驱动的缺陷可直接打穿系统；失败隔离、最小特权都无从谈起——与真微内核（seL4/Zircon）形成对照，也说明其"宏内核选择换性能"是有意为之的取舍。
3. **全 C 实现、无内存安全纵深**：203 个 syscall、内核内 VFS/网络栈、频繁的链表宏操作（`LOS_DL_LIST_FOR_EACH_ENTRY`）都是 CVE 温床；对比 BerryOS 的全 Rust 路线，这是时代局限。
4. **LiteOS-A 只覆盖 32 位**（ARM Cortex-A 32 位、4GiB 地址空间、r7 svc），无 64 位故事；想上富设备只能换 Linux——三内核矩阵的代价是**能力断层**：LiteOS-A 与 Linux 之间共享的用户态框架要同时迁就两套差异巨大的内核（LiteIPC vs binder 式 IPC）。
5. **syscall 面大且含 fork/clone**（syscall_lookup.h 实测 203 项），POSIX 兼容优先于接口最小化，内核暴露面与 BerryOS 的 ~26 个 object_* 合并式设计是两个极端，安全审计成本高。
6. **社区结构性依赖华为**：内核与框架的主力提交者仍是华为系，外部贡献多集中在板级适配（各内核仓 PR 以芯片厂商移植为主）；商业版 HarmonyOS 与开源版能力不同步（微内核不开源）也持续消耗社区信任。

## 7. 对 BerryOS 的借鉴与警示（对照四支柱）

**支柱① 微内核（Zircon 务实派）**
- 借鉴：LiteIPC 的**每进程 mmap 零拷贝消息池 + SVC 服务句柄**是"channel 之外最便宜的类能力 IPC"，BerryOS 的 channel 实现可直接对照其内存布局（池内偏移传消息、FD/PTR 特殊对象的下发与回滚路径 hm_liteipc.c:717-870）；`LosProcessCB` 的字段级条件编译是"同一内核跨档裁剪"的范本，BerryOS 的 v1→满配演进可借用该思路（cfg 驱动的结构体裁剪）。
- 警示：**别学"驱动/FS 进内核换性能"**——那是百 KB 设备的专利；BerryOS 目标是 M 级以上，用户态服务 + 失败隔离的正道不能动摇。也别学 203 个 syscall 的大接口面——LiteOS-A 的 POSIX 包袱证明接口只增不减。
- 事实校准：OpenHarmony 文档自比 LiteOS-A 与"Zircon/Darwin 同级"仅指 MMU 隔离能力；BerryOS 坚持 Zircon 式内核对象/handle 与纯能力，在隔离强度上确实高于 LiteOS-A 一档，这不是保守是定位差异。

**支柱② 省电（tickless day-one）**
- 借鉴：LiteOS-M 是 BerryOS tickless 最直接的同行验证——`OsSchedSetNextExpireTime` 的"取 min(最近超时, 时间片终点) 重装载 + 精度 75% 裕量"与 BerryOS 已落地的 idle mask CNTV 同构，可直接对齐细节；**两个机制值得抄**：(a) `OsSchedTimeConvertFreq` 的周期数记账+调频换算（BerryOS 未来 DVFS 必需）；(b) 深睡前**冻结任务超时链**（freeze/unfreeze），醒来按剩余量恢复，深睡期间零定时器唤醒。
- 警示：LiteOS 的 PM 框架在内核内（`los_pm.c`）；BerryOS 定的"PM 服务在用户态 + 内核降级保活 + 共享环形缓冲"路线更重但更符合微内核哲学，不要因为 LiteOS 的内核内实现更简单而回退。

**支柱③ AI 原生调度**
- 借鉴：LiteOS-A 的 EDF/deadline 调度类（los_deadline.c）+ 每任务运行时统计（los_statistics.c、cpup 组件）给出了"软实时 + 可观测"的最小实现，BerryOS 的 `class_deadline` trait 可以它为行为参照；HDF 的**按需加载触发**（用户态首次访问才加载驱动）思路同样适用于"AI 策略服务按需启停低优先级服务"。
- 警示：LiteOS 系完全没有策略注入接口——调度策略是编译期死的；BerryOS 的"内核留 ABI 让用户态 AI 干涉放置/频率/优先级"是差异化所在，别在实现中被简化掉。

**支柱④ PC 高性能（每核 runqueue+负载均衡）**
- 借鉴：LiteOS-A 的 `g_schedRunqueue[CORE_NUM]` 每核 runqueue + 亲核掩码内联过滤 + lockdep，与 BerryOS 设计文档逐点吻合，可作参考实现（尤其两级位图派发的简洁性——若 BerryOS 需要 >32 优先级或 deadline 队列加速，两级位图是 O(1) 且免堆的）。
- 警示：LiteOS-A 负载均衡弱（无 capacity 概念、无大小核抽象）；BerryOS 的 capacity(0..1023) 与 EPP hint 是超越项，不必参考它，参考 Linux EAS（另篇报告）。

**通用警示**：OpenHarmony 的最大教训是**宣传与架构脱节的信任成本**。BerryOS 在文档与对外表述中应始终如实描述内核形态（微内核就是微内核，MVP 阶段允许不完整但不允许名不副实）——本仓库 CLAUDE.md/架构决策.md 的"阶段化基线 + 关卡 G0-G5"纪律正是对这种风险的制度性防御。

## 8. 参考资料链接

- kernel_liteos_m 仓库：https://gitee.com/openharmony/kernel_liteos_m （los_sched.c / los_task.h / los_memory.c / los_pm.c / cmsis_liteos2.c / Kconfig / LICENSE / NOTICE）
- kernel_liteos_a 仓库：https://gitee.com/openharmony/kernel_liteos_a （hm_liteipc.c / los_sched_pri.h / los_process_pri.h / los_vm_map.c / los_user_init.c / syscall/syscall_lookup.h / LICENSE）
- HDF 驱动框架仓库：https://gitee.com/openharmony/drivers_hdf_core （LICENSE 双许可声明）
- HDF 总览与管理文档：https://gitee.com/openharmony/docs/blob/master/zh-cn/device-dev/driver/driver-overview-foundation.md 、driver-hdf-manage.md
- 内核概述文档（轻量/小型）：https://gitee.com/openharmony/docs/blob/master/zh-cn/device-dev/kernel/kernel-mini-overview.md 、kernel-small-overview.md
- 分布式软总线：https://gitee.com/openharmony/communication_dsoftbus （README_zh.md，Apache-2.0）
- Linux 内核仓：https://gitee.com/openharmony/kernel_linux_5.10 、https://gitee.com/openharmony/kernel_linux_6.6
- 版本路线图：https://gitee.com/openharmony/release-management/blob/master/OpenHarmony-RoadMap.md ；6.0 Release 说明：https://github.com/openharmony/docs/blob/master/zh-cn/release-notes/OpenHarmony-v6.0-release.md
- 开放原子基金会生态报道（13 亿设备/毕业）：https://www.openatom.org/journalism/detail/BY7bEUyPEmiX 、https://www.openatom.org/journalism/detail/1aPG6TmHHol2
- 华为开源页（贡献数据）：https://www.huawei.com/cn/open-source/about
- "微内核"争议分析：http://hellohtml5.com/tech/2021/07/01/harmonyos_investigation ；维基百科"鸿蒙内核"条目：https://zh.wikipedia.org/wiki/鸿蒙内核 ；看雪 HarmonyOS 2.0 逆向：https://bbs.kanxue.com/thread-267955.htm
