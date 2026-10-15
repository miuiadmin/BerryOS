# RT-Thread 调研报告

> 调研日期 2026-10-15 · 基于公开资料与源码快照（GitHub master @ 2026-10-14 push）· 许可证：内核与官方组件 Apache-2.0（3.1.0 及之前为 GPLv2；第三方组件随上游许可，如 lwIP 为 BSD）

## 0. 一句话定位

RT-Thread 是国内装机量最大的开源实时操作系统：以宏内核单核 RTOS 起家（驱动/FS/网络全在内核态），通过 RT-Smart 扩展出"混合内核"形态（MMU 用户态 lwp 进程 + musl libc），公司化运营（睿赛德）+ 庞大软件包生态（830+ 包）是其真正的护城河——技术上是优秀的中端 RTOS，不是微内核。

## 1. 谁在做：组织/社区/治理/活跃度/商业背书

- **起源与公司**：熊谱翔 2006 年发起；2017 年成立上海睿赛德电子科技有限公司运营。融资：2017-07 天使轮近千万（华强聚丰、思必驰），2019-11 B 轮近亿元（GGV 领投、君联资本追投，来源 36 氪 [36kr.com/p/1724740665345]）。公司主导内核方向 + 社区贡献外围的模式，治理不是基金会制。
- **GitHub 实测数据**（GitHub API，2026-10-15 查询）：stars 12,282 / forks 5,476 / open issues 495 / 贡献者 310 人（contributors API 末页）；仓库创建于 2013-01-02，最后 push 2026-10-14——日更活跃。Gitee 同步镜像（rtthread/rt-thread），国内外双仓。
- **版本节奏**（releases API）：v5.3.0（2026-09-10，最新）← v5.2.2（2025-10-31）← v5.2.1（2025-05-30）← v5.2.0（2025-03-19）← v5.1.0（2024-04-22）← v5.0.0（2023-03-31）← v4.1.x（2022）。每年 1–2 个 minor，**无 6.x**（2026-10 核实，网上"v6.0"说法不实）。
- **生态规模**（官方口径，多个信源交叉）：装机量超 **25 亿台**（2025-06 新华网、官网 about 页 "2.5B+"），开发者 25–30 万，服务近万家企业，软件包 830+–近 1000 个、累计下载 1200–1700 万次。
- **社区活动**：每年一届 RT-Thread 开发者大会（RDC）发布年度路线（2026 场主题即"AI 新生态/RISC-V"），配合高校嵌入式竞赛与"嵌入式软件人才计划"持续输送使用者——社区运营是国内开源项目里最成体系的。
- **商业动向（2025–2026）**：2022 推出"程翧"车载融合软件平台；2025 成为 **AUTOSAR 组织合作伙伴**并适配英飞凌 AURIX 车规芯片；2018 起布局 RISC-V（称已全系列适配主流 RISC-V 内核）；WAIC 2025 打"边端 AI 接入"牌。商业模式 = 免费开源内核 + Studio IDE/商业授权/技术支持/车规增值。

## 2. 总体架构：内核形态与分层

**内核形态：宏内核单核 RTOS（无地址空间隔离，单内核镜像），RT-Smart 开启后是"混合内核"**——内核态仍包含驱动/FS/网络，但增加 MMU 用户态进程。绝非微内核：服务不通过 IPC 组合，而是全部链接进同一内核镜像。

分层（master 目录实测）：

```
rt-thread/
├── src/              内核核心：scheduler_up.c/scheduler_mp.c/scheduler_comm.c、
│                     thread.c、timer.c、clock.c、ipc.c、object.c、
│                     mem.c|slab.c|memheap.c（三套堆）、idle.c、kservice.c
├── include/rtdef.h   所有核心结构体（rt_object/rt_thread/rt_device/rt_timer…）
├── libcpu/           CPU 架构移植层（上下文切换/异常入口）
├── components/
│   ├── drivers/      设备框架核心 device.c + 串口/SPI/I2C/PIN 框架 + pm/ + pci/
│   ├── dfs/          虚拟文件系统（内核态）
│   ├── net/          网络框架（lwIP，内核态）
│   ├── lwp/ + mm/    RT-Smart：用户态进程 + 地址空间子系统
│   ├── finsh/        shell；libc/ musl/newlib 适配；rust/ Rust 组件（新）
│   └── mprotect/ utest/ …
├── bsp/<厂商>/<芯片>/  两百多个板级支持包，驱动实现在此（内核态）
└── examples/
```

三个发行形态：**Nano**（极简内核，README_zh 称最小 1.2KB RAM / 3KB Flash）、**标准版**（全组件）、**RT-Smart**（master 上 `RT_USING_SMART` 宏开启，rt-smart 独立分支已删除、代码并入 master）。

## 3. 核心机制深挖

### 3.1 调度器：优先级位图 + 时间片轮转（O(1)）

核心数据结构是 32 优先级 × 每级双向链表 + 一个 32 位就绪位图（`src/scheduler_up.c:46-47`）：

```c
rt_list_t rt_thread_priority_table[RT_THREAD_PRIORITY_MAX];
rt_uint32_t rt_thread_ready_priority_group;
```

选取最高优先级线程（`src/scheduler_up.c:104-124`）只需一次"找第一个 1"：

```c
#if RT_THREAD_PRIORITY_MAX > 32
    number = __rt_ffs(rt_thread_ready_priority_group) - 1;
    highest_ready_priority = (number << 3) + __rt_ffs(rt_thread_ready_table[number]) - 1;  // 二级位图
#else
    highest_ready_priority = __rt_ffs(rt_thread_ready_priority_group) - 1;
#endif
    highest_priority_thread = RT_THREAD_LIST_NODE_ENTRY(
        rt_thread_priority_table[highest_ready_priority].next);
```

`__rt_ffs` 通用 C 版用 mod-37 查表位技巧（`src/kservice.c:1449`），ARM 上换 CLZ 指令一条搞定：

```c
int __rt_ffs(int value)
{
    return __lowest_bit_bitmap[(rt_uint32_t)(value & (value - 1) ^ value) % 37];
}
```

插入/摘除同步维护位图（`src/scheduler_up.c:512-529`）：`rt_list_insert_before(&rt_thread_priority_table[prio], &thread->tlist)` 后 `rt_thread_ready_priority_group |= number_mask`。时间片在 tick 里递减，耗尽即同优先级轮转（`src/scheduler_comm.c:294-325`）：

```c
if (RT_SCHED_PRIV(thread).remaining_tick > tick)
    RT_SCHED_PRIV(thread).remaining_tick -= tick;
else
    rt_sched_thread_yield(thread);   /* 转到同优先级链表尾部并请求重调度 */
```

v5.0 起调度器重构为 `scheduler_up.c`（单核）/`scheduler_mp.c`（SMP）/`scheduler_comm.c`（公共），SMP 下 `_SCHEDULER_CONTEXT` 宏（`rtdef.h:702-721`）把 `current_priority/priority_table/priority_group/tick` 全部做成**每 CPU 变量**——和 BerryOS"每核 runqueue"同方向。触发重调度的路径有三类：显式 `rt_schedule()`（yield/优先级变化处）、`rt_sched_unlock_n_resched()`（调度器锁释放时补切，见 scheduler_comm.c:321）、ISR 退出时的 `rt_hw_context_switch_interrupt` 延迟切换（scheduler_up.c:394 附近）；调度器锁 `rt_sched_lock` 与中断等级联动（scheduler_up.c:126-139 注释明言"关中断防抢占"）。注意调度触发仍是**抢占点式**：内核临界区（调度器锁/关中断窗）内不可抢占。调度钩子 `rt_scheduler_sethook`（scheduler_up.c:77）留了 from/to 观测点。

### 3.2 时钟与定时器：软/硬双通道

tick 路径（`src/clock.c:136-168`，由时钟 ISR 调用）：

```c
rt_atomic_add(&(rt_tick), 1);      /* 全局 tick（SMP 为每核 tick） */
rt_sched_tick_increase(1);         /* 时间片递减 */
#ifdef RT_USING_SMP
if (rt_cpu_get_id() != 0) return;  /* 仅核 0 执行定时器检查 */
#endif
rt_timer_check();
```

定时器按到期 tick 排序存放在跳表层级链表 `_timer_list[RT_TIMER_SKIP_LIST_LEVEL]` / `_soft_timer_list[]`（`src/timer.c:36-51`，默认 1 级即有序链表）。**硬定时器**超时函数直接在 ISR 上下文执行；**软定时器**（`RT_TIMER_FLAG_SOFT_TIMER`）到期只 `rt_sem_release(&_soft_timer_sem)` 唤醒高优先级定时器线程 `_timer_thread_entry`（timer.c:806）在线程上下文执行。`rt_timer_check()`（timer.c:748-773）里还能看到回绕处理 `(current_tick - t->timeout_tick) < RT_TICK_MAX / 2` 与"先摘链表→解锁→调回调"（timer.c:517-530）的防重入细节。线程 `rt_thread_delay` 即把线程挂到自己内嵌的 `rt_timer` 上。

### 3.3 内存管理：三套堆算法 + Kconfig 选择

- **小内存算法**（`src/mem.c`）：首次适应 + 相邻合并。块头 `struct rt_small_mem_item { pool_ptr; next; prev; }`（mem.c:59-71，64 位 24 字节），`lfree` 缓存最近空闲块，`plug_holes()` 前后合并。分配路径在 `rt_hw_interrupt_disable()` 临界区内（mem.c:563）——小而确定，但**关中断窗口随碎片/堆大小增长**。
- **SLAB**（`src/slab.c`，FreeBSD malloc 移植）：`zoneindex()`（slab.c:411）按尺寸分类（<128B 按 8B 粒度 16 类、<256B 按 16B……），`ZALLOC_ZONE_LIMIT = 16KB` 以上直接走页分配器 `rt_slab_page_alloc`；zone 大小在 32K–128K 间按堆上限动态定（slab.c:362-370）。适合大 RAM、高频分配。
- **MemHeap**（`src/memheap.c`）：多内存堆统一管理——每个 `rt_memheap` 是一条自由块双向链表（`block_list`），支持相邻合并，且**多个不连续物理堆可挂接为一个堆**，`RT_MEMHEAP_SIZE` 块头 + `MEMITEM(ptr)` 宏换算（memheap.c:42-44）。
- **选择逻辑**：`src/Kconfig:305-354` 用 choice 菜单让用户选 `RT_USING_SMALL_MEM_AS_HEAP` / slab / memheap-as-heap。经验口径：小 RAM（<百 KB 级）用 small mem，大 RAM/高频分配用 slab，芯片 RAM + 外扩 SDRAM 多片堆用 memheap；`RT_USING_MEMHEAP_AS_HEAP` 还允许把多个 memheap 对象聚合为系统默认堆。**没有进程级地址空间**——那（传统 RT-Thread 里）不存在；地址空间是 RT-Smart `components/mm/` 子系统的事（见 3.5）。

### 3.4 设备框架：rt_device（驱动内核态的代价）

所有设备继承 `struct rt_device`（`rtdef.h:1377-1421`），统一 init/open/close/read/write/control 六操作 + 异步回调：

```c
struct rt_device {
    struct rt_object parent;                    /* 内嵌进全局对象容器 */
    enum rt_device_class_type type;
    rt_uint8_t ref_count;
    rt_err_t (*rx_indicate)(rt_device_t dev, rt_size_t size);
    rt_err_t (*tx_complete)(rt_device_t dev, void *buffer);
    const struct rt_device_ops *ops;            /* init/open/close/read/write/control */
    ...
};
```

注册入口在 `components/drivers/core/device.c:67` `rt_device_register()`。v5.x 新增可选**设备模型 DM**（`RT_USING_DM`）：`rt_bus/rt_driver` 总线挂载、OFW 设备树节点、power_domain、DVFS、dma_ops 字段（rtdef.h:1381-1395）——明显在向 Linux 设备模型靠拢。**代价**：驱动实现全部位于 `bsp/*/drivers/`，编译链接进内核镜像，与内核同特权级同地址空间——驱动野指针/越界 = 整机 fault，无隔离无重启；内核镜像随 BSP 膨胀。RT-Smart 下依然如此。

### 3.5 RT-Smart：混合内核与 lwp——离真微内核还差什么

RT-Smart 2020-09/11 发布（研发 2 年，官方称内核约 500KB、启动 <1s、musl libc 用户态），现已并入 master（`components/lwp/` 36 个文件：lwp.c/lwp_pid.c/lwp_mm.c/lwp_user_mm.c/lwp_shm.c/lwp_signal.c/lwp_futex.c/lwp_ipc.c/lwp_syscall.c 1.16 万行/vdso/utest）。进程结构 `struct rt_lwp`（lwp.h:141+）：`rt_aspace_t aspace`（ARCH_MM_MMU 时）/MPU 分支、`bind_cpu`、`parent`、`struct dfs_fdtable fdt`、POSIX signal、cwd……——一个缩微 Linux 进程。

地址空间子系统 `components/mm/` 是 v5.x 最大的架构演进：`mm_aspace.c`（含全局 `rt_kernel_space`）、`mm_page.c`、`mm_anon.c`（匿名页）、`mm_fault.c`、`mm_memblock.c`、`mm_object.c`。`struct rt_varea`（mm_aspace.h:59+）：

```c
typedef struct rt_varea {
    void *start;  rt_size_t size;  rt_size_t offset;
    rt_size_t attr;  rt_size_t flag;
    struct rt_aspace *aspace;
    struct rt_mem_obj *mem_obj;
    void (*on_page_fault)(struct rt_varea *varea, struct rt_aspace_fault_msg *msg);
    ...
} *rt_varea_t;
```

——varea + mem_obj + **缺页回调**，已经是 demand-paging 骨架，形态接近 BerryOS 的 VMO+Vmar 设计。exec 路径走 `lwp_execve()`（lwp.c:488）加载 ELF、装配 stdio，配套 futex（lwp_futex.c，全局 futex 表）、vdso、调试器支持（lwp_dbg.c）与 musl 运行时。syscall 是 Linux 风格编号表（`lwp_syscall.c:11323+`）：`const static struct rt_syscall_def func_table[] = { SYSCALL_SIGN(sys_exit) /* 01 */, sys_read, sys_write, ... }`，约 233 项，且把内核 IPC 对象一并 syscall 化（sys_sem_create/take/release、sys_mutex_* 等）。

**离真微内核还差**：① 驱动/DFS 文件系统/lwIP 网络仍在内核态，无用户态服务化；② IPC 对象（信号量/互斥量）是**内核全局命名对象**（`rt_object` 内嵌 `name[RT_NAME_MAX]` 字符串名，rtdef.h:291-310，全系统可枚举查找），`sys_sem_create` 等 syscall 只是把它暴露给用户态，无能力/权限模型——任何进程拿到名字即可访问；③ 无服务崩溃隔离/监督重启，内核态组件故障仍是整机故障；④ `RT_USING_SMART` 宏散布内核结构体（如 rt_object 的 `lwp_ref_count`，rtdef.h:305-307），两种形态耦合在同一份源码里。

### 3.6 低功耗 PM 与 tickless：补偿式补 tick

PM 组件在 `components/drivers/pm/pm.c`（1305 行）。模式分睡眠模式（NONE/IDLE/LIGHT/DEEP/STANDBY/SHUTDOWN）与运行模式（`rt_pm_run_enter`，pm.c:958，驱动 CPU 变频 `_pm_frequency_scaling`）。模块用 `rt_pm_request/release` **计数投票**聚合出最低可行睡眠档（`_pm_select_sleep_mode`，pm.c:238+），设备经 `rt_pm_device_register`（pm.c:835）挂 suspend/resume/frequency_change 钩子。tickless 关键路径（`_pm_change_sleep_mode`，pm.c:389-483）：

```c
/* 入睡前（pm.c:444-457 节选）：算出距下一事件的 tick 数，用低功耗定时器定时 */
timeout_tick = pm_timer_next_timeout_tick(pm->sleep_mode) - rt_tick_get();
pm_lptimer_start(pm, timeout_tick);
pm_sleep(pm, pm->sleep_mode);                       /* 进低功耗 */
/* 醒来后（pm.c:462-472 节选）：把睡眠期间流逝的 tick 一次性补上 */
delta_tick = pm_lptimer_get_timeout(pm);
if (delta_tick) { rt_interrupt_enter(); rt_tick_increase_tick(delta_tick); rt_interrupt_leave(); }
```

入口 `rt_system_power_manager()`（pm.c:488-500）由 **idle 线程**调用（`src/idle.c:150-155`：先 `rt_defunct_execute()` 收尸，再进 PM）。要点与局限：这是"systick 停了、醒来补账"的**补偿式 tickless**——平时周期 tick 照跑，只有进睡眠那一窗停 tick；改写 `rt_tick` 也意味着软件定时语义在睡眠窗内是粗粒度对齐的。对照 BerryOS 的 NO_HZ day-one（空闲直接 mask CNTV + wfi，one-shot 编程到下一事件）更彻底，且 RT-Thread 的 PM **策略（模式判决/频率切换）全在内核组件里**，机制策略未分离。

## 4. 源码走读小结（关键路径索引）

| 主题 | 文件:行 | 符号 |
|---|---|---|
| 就绪位图 | src/scheduler_up.c:46,104-124,512-529 | `rt_thread_priority_table` `rt_thread_ready_priority_group` `_scheduler_get_highest_priority_thread` |
| ffs | src/kservice.c:1449 | `__rt_ffs`（mod-37 表；ARM 用 CLZ） |
| 时间片 | src/scheduler_comm.c:294-325 | `rt_sched_tick_increase` `rt_sched_thread_yield` |
| SMP 每核上下文 | include/rtdef.h:702-721 | `_SCHEDULER_CONTEXT` |
| tick 路径 | src/clock.c:136-168 | `rt_tick_increase` |
| 定时器 | src/timer.c:36-56,748-812 | `_timer_list` `_soft_timer_list` `rt_timer_check` `_timer_thread_entry` |
| 三套堆 | src/mem.c:59-83,563；src/slab.c:411,601；src/memheap.c:42-126 | `rt_small_mem_item` `zoneindex` `rt_memheap_init` |
| 设备框架 | components/drivers/core/device.c:67,186；rtdef.h:1377-1421 | `rt_device_register` `struct rt_device` |
| lwp 进程 | components/lwp/lwp.h:141-213；lwp_syscall.c:11323 | `struct rt_lwp` `func_table[]` |
| 地址空间 | components/mm/mm_aspace.h:40-93 | `rt_aspace` `rt_varea` `on_page_fault` `rt_kernel_space` |
| PM/tickless | components/drivers/pm/pm.c:389-500；src/idle.c:150-155 | `_pm_change_sleep_mode` `pm_lptimer_*` `rt_system_power_manager` |
| 全局命名空间 | rtdef.h:291-310；src/object.c | `struct rt_object` `rt_object_get_information` |
| Rust 组件 | components/rust/（core/examples + SConscript） | C 内核上的 Rust 绑定（ARM/AArch64/RISC-V 自动检测） |

## 5. 亮点：好在哪

1. **调度器极简且真正 O(1)**：位图 + CLZ 一条指令选线程，代码约 700 行单核调度器，可读可裁——教学级清晰度（scheduler_up.c 全文可证）。
2. **软/硬定时器分离**是 RTOS 里少见的干净设计：ISR 上下文只发信号量，慢回调进高优先级定时器线程，护住了中断延迟（timer.c:748-812）。
3. **三套堆按场景可选**，Kconfig 一键切换；memheap 解决嵌入式"多片不连续 RAM 统一管理"真实痛点（memheap.c + src/Kconfig:305-354）。
4. **可裁剪性极致**：Nano 最小 1.2KB RAM/3KB Flash（README_zh），同一内核源码覆盖从 Cortex-M0 到 Cortex-A/RISC-V。
5. **生态是碾压级优势**：830+ 软件包、包管理器 + Studio IDE + 两百多 BSP + 25 亿装机 + 芯片厂（ST/NXP/乐鑫等）与 AUTOSAR 背书——国内嵌入式"事实标准"。
6. **演进务实**：v5.x 重构出 scheduler_comm/mm_aspace(带缺页回调)/设备模型 DM（总线/设备树/DVFS），向 Linux 架构靠拢但保持 RTOS 体量；master 出现 components/rust 说明社区在为 Rust 生态铺路。
7. **许可证策略教科书级**：2017 年后果断 GPLv2→Apache-2.0（官网 contribution 页），消除商用传染顾虑，是装机量起飞的制度前提。

## 6. 不足与争议：不好在哪

1. **驱动/FS/网络全内核态**：`bsp/*/drivers` 链接进内核镜像，无隔离——驱动 bug 即整机崩，且无法按服务重启；这正是它至今不是微内核的根本（对照 3.4/3.5）。
2. **全局对象命名空间，无能力模型**：`rt_object.name[RT_NAME_MAX]` 全局可枚举，任何线程/进程凭名字即可拿到信号量等对象（rtdef.h:291-310、object.c），安全性靠"大家守规矩"。
3. **补偿式 tickless + PM 策略在内核**：睡眠窗内 tick 靠醒来一次性 `rt_tick_increase_tick` 补账（pm.c:465-472），平时周期 tick 照跑；governor（模式判决/变频）长在内核组件，第三方策略难注入。
4. **实时性短板在锁粒度**：small mem/memheap 分配持有关中断临界区（mem.c:563、memheap.c:877），堆大碎片多时最大关中断时长不可控；SMP 下定时器只允许核 0 检查（clock.c:157-161），单点瓶颈。
5. **抢占点式调度**：内核临界区不可抢占，最坏延迟依赖代码纪律而非机制保证；无 deadline/带宽类实时策略（只有 256 级静态优先级 + 时间片）。
6. **质量参差与形态分裂**：master 同时维护 Nano/标准/Smart 三形态（`RT_USING_SMART` 宏侵入 rt_object 等基础结构体）；无形式化验证，安全靠 utest/BSP 回归；车规走流程认证而非证明。
7. **公司强主导**：核心方向由睿赛德定，无中立基金会，长期路线（如车规专有化"程翧"）与开源社区利益可能分叉——采用者需评估供应商锁定。

## 7. 对 BerryOS 的借鉴与警示（对照四支柱）

**支柱① 微内核**
- 借鉴：`components/mm/` 的 `rt_varea + mem_obj + on_page_fault` API 形状——用"区域对象 + 缺页回调 + mem_obj 后端"描述地址空间，与 BerryOS Phase 2b 的 VMO/Vmar 高度同构，其 `rt_kernel_space` 全局内核空间与每 lwp aspace 的双空间结构可直接对照实现；`rt_device` 六操作 + rx_indicate/tx_complete 回调的**接口形状**可平移为 BerryOS 用户态驱动服务的 RPC 语义。
- 警示：别学"驱动进内核镜像 + 全局对象名"——RT-Smart 做了 MMU 隔离仍被这两点钉死在混合内核；BerryOS 的 handle 能力传递必须坚持无全局名字查找。

**支柱② 省电**
- 借鉴：`rt_pm_request/release` 计数投票聚合睡眠档（多模块约束合成最低功耗模式）、per-device PM 钩子（suspend/resume/frequency_change）、按时长阈值分级睡（pm_get_sleep_threshold_mode）——这三件事应长在 BerryOS 用户态 PM 服务里，经共享环形缓冲下发内核。
- 警示：RT-Thread 的补 tick 式 tickless 是"systick 心智"的产物；BerryOS NO_HZ day-one（空闲 mask CNTV + wfi，one-shot 到下一事件）在机制上更优，别倒退回"醒来补账"。

**支柱③ AI 原生调度**
- 借鉴：`rt_scheduler_sethook`（from/to 观测）与 `RT_USING_CPU_USAGE_TRACER` 的每 tick 记账（clock.c:120-124）证明"内核留静态观测点"成本极低；BerryOS 的遥测 ABI 可同样以 hook/静态点起步。RT-Thread 没有策略注入接口（governor 在内核）——这正是 BerryOS 差异化空位：策略可插拔 trait + 用户态 AI 服务在 RT-Thread 生态里没有对应物。

**支柱④ PC 高性能（多核）**
- 借鉴：`_SCHEDULER_CONTEXT` 把优先级表/位图/tick 全做每 CPU 变量（rtdef.h:702-721），印证 BerryOS"每核 runqueue + 负载均衡"路线；软/硬定时器分离思路可平移为"中断上下文最小化 + 延迟工作线程"。
- 警示：RT-Thread SMP 的"仅核 0 检查定时器"是简化换来的单点瓶颈，BerryOS 每核本地定时器从 day-one 就该做对；其固定 256 级优先级无 capacity 概念——大小核放置需要 BerryOS 的 capacity(0..1023) 抽象，此处无可借鉴。

**通用借鉴**：memheap"多片不连续物理内存合并为一个堆"的思路对 BerryOS 物理页帧管理有参考价值；Kconfig/menuconfig 树内裁剪体系证明了"可配置性即生态吸引力"，但 BerryOS 应以 cargo features/可选 crate 达成同样效果，避免把组件耦合进内核源码树；Apache-2.0 宽松许可 + 商业双轨的运营经验值得学习（BerryOS 参考源码纪律本就要求 GPL 不进）。

## 8. 参考资料链接

- 源码仓库：https://github.com/RT-Thread/rt-thread （master，2026-10-14 push；文中文件:行号均为此快照）
- 版本发布：https://github.com/RT-Thread/rt-thread/releases （v5.3.0 2026-09-10 最新）
- 官网/简介（装机量 2.5B+、250K+ 开发者）：https://www.rt-thread.org/about.html
- 文档中心：https://www.rt-thread.org/document/site/
- RT-Thread Studio：https://www.rt-thread.io/studio.html
- 许可证沿革（GPLv2→Apache-2.0）：https://www.rt-thread.io/contribution.html
- RT-Smart 发布报道（2020-11，musl/内核 500KB/启动 <1s）：https://www.eet-china.com/kj/59008.html
- B 轮融资（2019-11，GGV 领投近亿元）：https://m.36kr.com/p/1724740665345
- 睿赛德入局整车 OS（中国日报 2025-04，近 1000 软件包）：https://ex.chinadaily.com.cn/exchange/partners/82/rss/channel/cn/columns/sz8srm/stories/WS68119215a310205377031276.html
- 官方新闻（RISC-V 生态/25 亿装机/30 万开发者，2026-06）：https://www.rt-thread.org/newsDetail.html?id=375de515bc016d74
- GitHub API 实测（stars/contributors/releases，2026-10-15）：https://api.github.com/repos/RT-Thread/rt-thread
