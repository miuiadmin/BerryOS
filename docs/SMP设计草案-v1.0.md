# SMP 双核关卡（G2）设计草案（S-0..S-5 拆步）

> 版本：**1.0**（v1.0）｜2026-11-06

> 日期：2026-10-22。背景：Phase 6 全量收官（`P6收官盘点.md`）+ P7-x4 x86_64 最小集
> 收官后，v2 主线首刀 = SMP（调度器设计 **M1b**、测试策略关卡 **G2**、路线图
> 「SMP 是 v2 的锚点」战略裁决）。本文 = 设计先行（`TCP栈设计草案-v1.0.md` 体例）：
> 先清算 v1 免锁根基、裁决锁纪元与拆步，实施刀落地时逐条兑现并回写执行结果区。
> 素材 = 四维代码侦察（调度器 / MM 地址空间 / 对象系 syscall / HAL 中断 console，
> 2026-10 本会话）+ QEMU PSCI 通道外部核实；行号锚以侦察时点 main @ cc74f7c 为准。

## 0. 目标与非目标

- **目标（G2 盒子，`测试策略-v1.0.md` 关卡定义逐字）**：双核 SMP——`-smp 2` 下
  负载均衡对、TLB shootdown 对、无竞态（并发压测）。两核并行执行用户任务，
  内核共享结构在明确的同步协议下访问。挂钩：调度器设计 M1b（每核 runqueue +
  负载均衡/work stealing）、遥测「SMP(G2) 按核拆 per-CPU 环」既定方案。
- **非目标（v1/后续文档化取舍）**：
  - \>2 核拓扑与 capacity/EAS 大小核调度（M2，v3+——本刀恒 `-smp 2` 定值）；
  - 全抢占内核（G3——v1 syscall 主体不可抢占的免锁根基在 E0 纪元**保留**，见 §2）；
  - lockdep / 延迟基准（G3 面）；EEVDF band（M4）；PM 服务（M3）；CPU 热插拔；
  - ASID 分代位图完整方案（锁内轮转起步即可，`asid.rs:15-16` 自白的 v2+ 项）；
  - 读侧无原子的大规模 RCU 化（纪律③「少发明新同步原语」——自旋锁 + per-CPU
    够用为度）。

## 1. v1 免锁根基清算（四维侦察整合）

### 1.1 总根源：全系统一个隐式临界区

v1 的无锁不是逐对象设计的，是**一根隐式临界区罩住全系统**：SVC/异常入口硬件
置 DAIF（IRQ 屏蔽）→ 「syscall 主体 = 临界区」（`kernel/src/syscall.rs:196-197`
注释自白「单核无 TOCTOU」）+ 单核串行 → CURRENT/RUNQUEUE/SLEEPQ/REAPQ/对象系/
分配器/遥测环**全体免锁**。开 `-smp 2` 后每核各有一个这样的临界区、互不互斥——
所以 SMP 的最小正确路线**不是逐对象打补丁**，而是把这个隐式临界区**显式化**
（§2 E0：入口取一把锁复刻单核串行语义），再逐域细化。三处代码自白在册：
`sched/src/sched.rs:644`（block_current「SMP(G2) 重做」）、`kernel/src/heap.rs:15-18`
（「SMP 后需加锁或改每核 slab」）、`kernel/src/mm.rs:3-8`（「SMP 后整体重构」）。

另一根同构支柱是「`disable_irq` 包络 = 串行」：console 任务的 RX 消费包络
（`main.rs:1541-1545`）、运行期 spawn 包络（`main.rs:2215-2219`）、
`for_each_sleeper` 契约（sched.rs 文档注释）等十余处——**屏蔽本核 IRQ 挡不住
他核上的写者**，SMP 下集体失效（§1.2 逐条）。

### 1.2 四维竞态清单（SMP 下全失效/全竞争）

| 维度 | 全局单份/无锁结构 | SMP 死状 | 代表锚点 |
|---|---|---|---|
| 调度器 | `CURRENT` / `RUNQUEUE` 全局 | 两核 pick 同任务 → 双核运行同一 Task、上下文互踩 | sched.rs:26/:28 |
| 调度器 | `IDLE_TASK` 单份、idle 复用 boot 主栈 | 两核同落 idle 互踩栈 | sched.rs:37-67、boot.rs:61/81 |
| 调度器 | `SLEEPQ` 单队列（insert_sorted / wake / expire 三方） | 非幂等 `unlink` 双摘同节点扬全队；丢唤醒 | sched.rs:545-549/:619-634 |
| 调度器 | **零 IPI/SGI**（gic.rs 只配 PPI27 + SPI33） | 唤醒他核任务无戳醒手段 = 最大结构性缺口 | gic.rs、trap.rs:428-440 |
| 调度器 | `LAST_VCT` 全局单份 | per-CPU 的量记全局 → slice 记账错乱 | class.rs:29 |
| 调度器 | wait poll-then-block | 核0 poll 未命中、核1 signal 扫 SLEEPQ 空、核0 block → 无 deadline 永久睡死（经典丢唤醒） | syscall.rs:1552-1576、sched.rs:646-648 明文 |
| 调度器 | fastpath `find_single_waiter` 契约「同一屏蔽窗内有效」 | find 后他核摘队 → 不幂等 unlink 扬队；`try_preempt` 两核同 pick | sched.rs:766-789/:799-805/:827-864 |
| 调度器 | `REAPQ` 静态环 cap32 | 并发 push/drain = 条目丢失或 `Box::from_raw` double-free | sched.rs:381-383 |
| 调度器 | `IRQ_TICKS`/`SWITCHES`/`USER_KILLS` 非原子 | 计数丢失 → watchdog 金丝雀误杀 | sched.rs:20-23/:300/:304 |
| MM | buddy 帧分配器无锁 | `alloc_order`/`find_free_head` 同 PA 双授；free 合并链 RMW 交错 | mm/src/buddy.rs:37-46/:94-115/:123-164 |
| MM | `frames()` 返回 `&'static mut` | 本身即 UB 别名（单核侥幸，双核必炸） | mm.rs:24/:49-56 |
| MM | SlabHeap freelist（`unsafe impl Sync` G0 特赦自白） | 同头槽双返回；push 丢链 | kernel/src/heap.rs:31-45/:48-80/:115-127 |
| MM | ASID 分配器惰性无锁 | 同 ASID 双发 → `tlbi aside1is` 互相清对方；回卷需全核 quiesce | aspace.rs:16-32、asid.rs:12-17 |
| MM | Vmar `regions: Vec` check-then-act | 双登记；`next_table` 并发建表互相覆盖 + 表帧泄漏（`assert!(e==0)` 无锁下变误报源） | vmar.rs:293-305/:325-337 |
| MM | 改表-失效窗口无锁（clear_leaves→dsb→tlbi vs 他核 walk_leaf/还帧） | 广播生效前他核用旧叶/旧 TLB 项 → decommit 还帧 → buddy 复用 → 旧 VA 写坏新主人 | vmar.rs:262-265/:366-374 |
| MM | Vmo `pages`/`mapcnt` | 同页双取帧；非原子饱和加减 | vmo.rs:55/:59/:124-143/:246-257 |
| 对象系 | `ObjRef.refs` 普通字段（handle 计 + pair 互钉 + 钉住链） | close vs duplicate/retain/release 丢更新 → UAF 或泄漏；`-EBUSY` 拒面 check-then-act | handle.rs:196/:204-221/:277/:291-315 |
| 对象系 | HandleTable `slots: Vec` | `pi::holder_has` 他核只读遍历 vs 表主核 alloc_slot 重分配 → UAF 读；exit drain vs 并发 send | handle.rs:234-237/:433-441/:507-528 |
| 对象系 | ChannelEnd `queue: Vec`（非原子满检→push） | 两核 send push 同队列数据竞争、双 send 突破深度帽 | channel.rs:65-76/:119-148、syscall.rs:1156→1229 |
| 对象系 | Event `signals` 非原子 RMW + 唤醒对 | 置位/清位互抹；RMW 与 wake_matching 交错漏醒 | event.rs:14-29、syscall.rs:1474-1482 |
| 对象系 | Process `state/started/exit_code` 无序可见 | check-then-set 双 start；退出钩子四步序跨核交错（漏醒 + drain vs send 解引用 peer UAF） | process.rs:54-75、syscall.rs:971-991/:899/:949 |
| 对象系 | PI 全图重推遍历 SLEEPQ + handle 表反查 | 遍历与入队/摘队/表变更交错；`donor_edge` 解引用 wait_obj | pi.rs:57-88/:103/:126-135 |
| HAL | UART TX 直写 DR 无锁 | 两核打印字节级交织、多字节消息撕裂（dump 面不可读） | uart.rs:68-78 |
| HAL | RX 环生产者 = IRQ 上下文（IRQ33 只投 CPU0）vs 消费者 console 任务 | console 若跑 CPU1，`disable_irq` 包络挡不住 CPU0 生产者 → head/tail 竞争 | uart.rs:163-203、gic.rs:146 target=0x01 |
| HAL | GICC PMR/CTLR + banked ISENABLER0 仅 CPU0 配置 | 副核 IAR 恒读 0x3ff（spurious）、CNTV PPI 永不使能 → 副核 timer 死 | gic.rs:79-84 |
| HAL | 遥测双环单环 + `check()` vct 单调断言 | 两核并发写必误报；既定方案 = per-CPU 环 | telemetry.rs:14/:44-54/:129-142 |
| HAL | boot 单入口无 MPIDR 分流、无 secondary entry、`.Lhang` 仅 wfe | `-smp 2` 下所有核同跑 `_start` 清 bss/建表 = 全面灾难 | boot.rs:10-90、linker-aarch64.ld:9 |

补充两条跨维事实：①**`-smp` 全仓零出现**（justfile/smoke.sh/架构决策 grep 证实）
——单核是 QEMU 默认值的隐式事实，S-1 刀要把 `-smp` 显式化进 smoke-smp 线；
②同 tag 共享表的多任务（ai bench ping/pong 有意设计）与 `caller_table` 路由
（syscall.rs:115-133）在 BKL 下语义不变，是「无需动」项。

### 1.3 现成正确项（SMP-ready，本刀不动）

- **TLB 失效指令形式已按 SMP 立对**：全仓 EL1 域仅 `tlbi aside1is` / `tlbi
  vmalle1is`（mmu.rs:190/:248）+ TCR SH=inner-shareable（mmu.rs:63-73）——v1
  刻意选 IS 广播口径（mmu.rs:186、asid.rs:7 注释自证）。**真缺口在时序与同步**
  （改表-失效窗口的页表锁、ASID 回卷的全核 quiesce），不在指令形式。
- **IAR→去活→EOIR 铁序**对多核本核 GICC 依然正确（trap.rs:405-441），无需改。
- **每任务 16KiB 内核栈**（task.rs:28/:248-255）私有多核直接沿用；TPIDR_EL0
  每任务保存（task.rs:124-136）与 SMP 正交。
- **TTBR0/TTBR1/CONTEXTIDR/VBAR/CNTV 寄存器面天然 per-CPU**：副核各自执行
  enable_mmu/VBAR 写入即可（KERNEL_L1 boot 后只读、TTBR1 全核同值）；
  `activate_ttbr0` 序（mmu.rs:201-219）SMP 下仍成立——切换点比较基准需随
  per-CPU CURRENT 自洽（sched.rs:244-248，E1 刀面）。
- **fw_cfg/dtb boot 期只读**、FREQ/SLICE init-once 只读缓存、函数指针钩子槽
  boot 写定后只读、`RX_EV_H/RX_EV_REF` boot 写后只读共享——均无需动。
- 已是原子的量（CONSOLE_DRAINED/BLOCK_BOOT_DONE/DNS_ID/PROCESS_LIVE 等）
  保持；伪原子警示：`Event.signals`/`ObjRef.refs`/telemetry `static mut`/
  `SWITCHES` **都不是原子**，最易误判「已处理过」。

### 1.4 设计声明与代码现状的一处偏差（对账）

调度器设计 §3.5 声明「per-CPU runqueue 数据结构 day-one 按每核数组写，锁与
均衡逻辑 G2 才进」——**代码现状是全局单份**（`Topo::single()` 硬编码
sched.rs:30 + runqueue.rs:204-217，runqueue.rs:7 注释自认「G0 单核：全局一个」）。
声明未兑现。裁决：不回头补 day-one 形态（单核下改数组是无验收面的空转），
**E1 刀直接按每核数组 + 锁一起落**；本草案记录偏差，S-3 兑现时设计文档该行
随改（「按每核数组写」→「S-3 落地」）。

## 2. 锁纪元裁决（E0→E1→E2）+ 排除 8 问

### 2.1 纪元表

| 纪元 | 内容 | 语义 | 刀 |
|---|---|---|---|
| **E0 BKL** | SVC/异常入口取一把大自旋锁（KERN_LOCK），锁内 = 现 syscall 主体；IRQ 上下文写共享结构（tick/console RX/遥测）同锁或收进 per-CPU 面 | **复刻单核串行语义 = 隐式临界区显式化**；内核串行 / 用户并行 | S-2 |
| **E1 per-CPU + 细化** | CURRENT/RUNQUEUE/IDLE_TASK/LAST_VCT/遥测环 per-CPU 化（遥测环按设计文档原案拆核）；SLEEPQ/REAPQ/buddy/slab/ASID 自旋锁；计数器 AtomicU64；跨核唤醒 SGI IPI | 内核临界区从「全系统」缩到「逐结构」；两核可并行进内核不同区域 | S-3 |
| **E2 逐域下放** | per-ChannelEnd 队列锁（BKL 退役后**第一个**下放点——scout 裁决）；per-Vmo/Vmar 锁 | 接近常规 SMP 内核的锁粒度 | S-4 起/v2 |

裁决依据（`测试策略-v1.0.md` 并发长跑纪律三条的正面用法）：①**语义等价小步**——
E0 锁内代码与单核逐字节同语义，FreeBSD 5.0 大爆炸重写是反面教材；②**激进
配置常驻 soak、逐块转正**——PREEMPT_RT 模板，每纪元独立走四跑验收 + soak；
③**少发明新同步原语**——自旋锁 + per-CPU + 原子计数器三件套够用为度，
RCU 级发明不做。史实节奏：Linux BKL 15 年 / FreeBSD Giant 7 年才退役完毕
——「先粗后细」是正路不是债；本仓体量小得多，但方向同构。

**E0 的三处关键边界**（实施刀详设）：

1. **锁的取放点**：SVC 入口（DAIF 已屏——自旋锁在此是纯互斥不涉中断）取、
   eret 前放；异常入口（lower-EL sync/abort 类进语义层前）同构。**IRQ 上下文**
   （tick/console RX）要么同锁（tick 抢占检查改锁内快照），要么先收进
   per-CPU-only 操作（console RX 只写本核环 + Event 置位原子化）——S-2 裁决，
   倾向后者（IRQ 侧拿锁 = 优先级反转面，v1 无优先级继承的自旋锁别进 IRQ）。
2. **`-> !` 退出路径**：`process_exit` 切走不归——锁在切换前放（switch 窗口
   特判），勿在 handler 尾补放（不可达死码，Plan B v0.5 纪律）。
3. **禁中断下自旋**：取锁者 DAIF 屏蔽中，持锁者不可长时间让出（v1 syscall
   主体不可抢占的免锁根基**在 E0 保留**——BKL 语义本就如此；`block_current`
   类睡眠点在锁外或切换前放锁，S-2 逐睡眠点清算）。

### 2.2 排除 8 问过表

- 借鉴对象 = Linux BKL→细粒度 / FreeBSD Giant→5.0 / PREEMPT_RT 逐块转正的
  **史实节奏**（观察史实，不搬代码——GPL 不进）；seL4（certified SMP 研究
  线的验证方法论）/ Zircon（per-CPU runqueue + IPI 的形态）只看思路。
- 轴 PICK 无冲突：不碰「机制/策略分离」中心（锁是机制面）；不引入 IPC 统管；
  Theseus 进程内/hubris 静态/seL4 不变量各面不涉及；验证关 = G2 本身。
- 裁决：**可借史实、不搬任何参考代码**；BKL 路线是本仓自己的裁决（纪律①②
  的推论），非照抄某家。

## 3. 硬件面积木与实证前置（S-0 硬门）

### 3.1 副核启动通道（最大未知数，断言值钉实测之后）

- **现状**：全仓零 `hvc/smc/cpu_on/spin-table/mpidr` 痕迹（侦察 grep 证实）；
  zCore 参考树也只有 SYSTEM_OFF 无 CPU_ON 可抄；ELF 直加载下 QEMU **不递
  DTB**（x0=0，fwcfg.rs:3-5 实证）→ 运行时拿不到 enable-method/
  cpu-release-addr——**副核通道要么硬编码 PSCI、要么硬编码 QEMU 内建
  secondary-hold 停车场地址**。
- **外部核实结论（2026-10，待 QEMU 本机实证钉死）**：QEMU virt 的 PSCI
  conduit **HVC 默认**（SMC 随 virtualization/el3 配置才有——seL4 社区
  elfloader SMP 在 QEMU 失败案即 conduit 误设 SMC 的实证）；CPU_ON function
  ID = **0xC4000003**（SMCCC 64-bit 域）。S-0 刀用
  `qemu-system-aarch64 -machine virt,dumpdtb=…` + monitor/反汇编实证：
  ①conduit 形态（HVC vs SMC vs 停车场写邮箱）②CPU_ON 返回码语义
  ③副核入口 EL 与寄存器初值（x0 是否带 Park 状态指针）——**不照规范想象
  写死**（G-eco fw_cfg 四层实证 + TCP pcap 硬门同族纪律）。
- **碰撞风险**：帧分配器把 `[__kernel_end_pa, RAM_TOP)` 整段当空闲（mm.rs:26-40）
  ——QEMU 会在 guest RAM 放 DTB blob 与 secondary 停车场邮箱，落位是否侵入
  该区间未验证，S-0 实测/预留。

### 3.2 per-核 bring-up 清单（S-1 逐项复制）

副核被释放后入口代码必须自带（现 boot.rs 序列只服务一次）：

| 项 | 来源锚点 | 说明 |
|---|---|---|
| MPIDR 分流 | boot.rs:10-90（现无） | 主核继续、副核 park（wfe + 释放地址邮箱）；MPIDR_EL1 affine 值 = 核身份 |
| EL2→EL1 降级 + HCR_EL2.RW=1 | boot.rs:25-44 | 副核入口 EL 与主核起始 EL 同制式；run-el2 风格下必须复刻 |
| `.bss` 清零幂等性 | boot.rs:63-78 | 副核重跑会踩主核数据——入口代码按「不清 bss」定制 |
| enable_mmu 全套（TTBR1/TTBR0/TCR/MAIR/SCTLR） | mmu.rs:129-143 | TTBR1 全核同值（KERNEL_L1 只读）；本核空表起步 |
| VBAR 写入 | trap.rs:258-265 | per-CPU 复位 0——不装则任何异常跳 0 地址 |
| CPACR_EL1 FP 使能 | boot.rs:58-60 | per-CPU 必做 |
| GICC PMR/CTLR + banked ID0-31 清理/使能 | gic.rs:64-84 拆段 | GICD 全局段主核一次；per-CPU 段各核各跑；**GICD_CTLR 全局生效** |
| CNTV CTL 初始屏蔽 + slice 授片 | timer.rs:49-93 | 调度心跳 per-核；FREQ/SLICE 全局只读缓存共用 |
| 副核私有栈 + idle 上下文 | task.rs:248-255 / sched.rs:34-37 | 复用 TaskContext 机制；**IDLE_TASK 单份改每核**（boot 主栈只归核0） |
| per-CPU 基址寄存器 | 新增 | **TPIDR_EL1** 存本核 per-CPU 块指针（TPIDR_EL0 已被用户 TLS 占用——P6 A-3 教训勿混用） |

### 3.3 SGI IPI 面（E1 硬前置，从零新增）

现况零 SGI 使用、未知源分支会把 SGI 打成 `irq? id=N`（trap.rs:428-440）。
E1 需要：①`GICD_SGIR` 写原语（目标核列表/过滤模式）；②SGI 号分配
（建议 2 个起：`SGI_RESCHEDULE` 跨核唤醒戳、`SGI_TLB_DONE` shootdown 完成回执
——S-4 才用）；③`exception_irq` 分流表加 SGI 段；④wfe/sev 或 IPI 二选一的
park/release 同步（S-1 邮箱起步用 wfe+sev，S-3 起调度改 IPI）。
TLB shootdown v1 保持**同步 IS 广播**（S-4 在页表锁内做，异步 shootdown
+ 回执 IPI 是 v2+ 优化，非本刀）。

**S-3a 实施定案（2026-11-01）**：①②③已落（`send_sgi` 原语 +
`SGI_RESCHEDULE`=1 + trap.rs 分流臂 probe ② 收讫计数）；④的 enqueue 尾
发送面被实验证伪除名——全局队列 peek 序下零派发价值 + console 灌压压垮
30s 帽（判词 → 子案 `SMP-S3实施设计草案-v1.0.md` §9.5 末），生产发送面归零，
在册发送者仅 bringup probe ③ 观测块；跨核唤醒戳随 S-3b per-CPU 队列路由化
回归（发送条件 = 目标核队列真有货）。副核 park 形 = masked-wfi + **截止
驱动醒**（SLEEPQ 归属核单点武装 CVAL——规则 ③④成文 → 子案 §2.4 实施定案段）。

### 3.4 其余硬件面裁决

- **UART TX 交织**：S-2 起打印路径（uart_puts 族）挂一把打印自旋锁
  （PANIC_LOCK 分离——panic/dump 路径死锁时 bypass 打印，Linux panic()
  同思路）；行完整性 = 锁粒度整条消息。
- **RX 环跨核**：v1 裁决 = **IRQ33 仍投 CPU0 且 console 任务钉核0 不迁移**
  （单消费者钉核 = 零成本消除竞争面；任务迁移 S-3 议）——SPSC 原子环是
  迁移放开时的替代案，本刀不做。
- **SPI 路由表**：`enable_spi` 的 target 参数现硬编码 0x01（uart.rs:220）——
  收口为 per-源路由常量表（S-1 面，副核使能 PPI27 与路由决策解耦）。

## 4. 拆步表（S-0..S-5，每刀独立「实施→四跑验收→对抗审查→修复复验」闭环）

| 步 | 内容 | 验收 | 标记/测试 |
|---|---|---|---|
| **S-0 实证前置** | QEMU `-smp 2` 副核启动通道实证：PSCI conduit/CPU_ON 返回码/副核入口 EL 与寄存器/DTB blob 与停车场邮箱落位（§3.1 四问）；`-smp` 从 QEMU 默认值隐式单核**显式化**为 justfile 定值注释 | 实测记录入执行结果区（断言值钉实测之后） | 无代码面产出（调查刀） |
| **S-1 per-CPU 基建** | MPIDR 分流 + 副核 park/release 邮箱（wfe+sev）+ §3.2 bring-up 清单逐项 + IDLE_TASK 每核化 + TPIDR_EL1 per-CPU 块；副核起来后 idle 循环 + 心跳可见 | `SMP ok ncores=2`（副核自报 MPIDR/心跳拍数）+ **smoke-smp 独立冒烟**（smoke-x86 同款：独立脚本独立日志，ARM64 四跑零扰动） | smoke-smp PASS；主 smoke 零漂移 |
| **S-2 BKL 纪元（E0）** | KERN_LOCK 入口锁（§2.1 三边界逐项：IRQ 侧收 per-CPU/`-> !` 切换前放锁/睡眠点清算）+ 打印锁 + 双核并行用户态 | **第一个真双核验收点**：两用户任务各钉一核并行跑（串行语义等价——裸 23 标记 + rootfs 20 标记在 `-smp 2` 下全绿）+ smoke-smp 含并发金丝雀 | smoke-smp 扩并发相；`-smp 2` 下既有标记零漂移 |
| **S-3 调度器 per-CPU 化（E1 主刀）** | CURRENT/RUNQUEUE/LAST_VCT per-CPU + IDLE per-核深化 + SLEEPQ/REAPQ/分配器/ASID 自旋锁 + 计数器原子化 + 遥测 per-CPU 环 + 跨核唤醒 SGI_RESCHEDULE + work stealing 初版 | **G2 验收项 1：负载均衡对**——双核各自满载任务对 + 偷取测试；ktest PI/FAST 契约面随锁纪元重验 | 负载均衡标记 + smoke-smp 全绿 |
| **S-4 TLB shootdown（E1 收官）** | 页表锁（Vmar/走查/还帧互斥「改叶+dsb+tlbi」与「walk/拷贝」）+ ASID 回卷锁内全核 quiesce + `assert!(e==0)` 消音复核 | **G2 验收项 2：TLB shootdown 对**——双核并发 map/unmap/decommit 压测 + canary 页（旧 VA 写必 fault） | shootdown 压测标记 |
| **S-5 G2 关卡** | 并发压测集（分配器完整性 + HandleTable integrity + canary 页 + 双核 ping-pong IPC 压测）+ soak 长跑（纪律②：激进配置常驻、逐块转正）+ 对抗审查收官 | **G2 验收项 3：无竞态**——压测 soak 零断言失败；`测试策略-v1.0.md` G2 行从「待办」转「已过」 | G2 过关话术落 CLAUDE/AGENTS |

依赖序：S-0 → S-1 → S-2 → S-3 → S-4 → S-5 严格线性（每刀以前刀的
并行面为压测环境）。E2 下放（per-ChannelEnd 锁）不占刀——S-5 过关后按
性能证据再立项（v2 议程）。

## 5. 验收面

- **smoke-smp 独立冒烟**（新 `tools/smoke-smp.sh`，smoke-x86 同款模式）：
  `-smp 2` 定值、标记恰一次 + 快败 grep、PID 记录 kill 纪律、**不并入主
  smoke**（ARM64 四跑零扰动——五原语收官前不并 CI 的同款裁决）；挂 CI 随
  S-5 G2 过关时议（ci.yml 那时一并加 `-smp 2` 成本项）。
- **标记纪律**：`SMP ok` 一律带参形态（`ncores=`/心跳数——`FS ok n=` 防子串
  混叠先例）；标记必须状态谓词非竞速——**TCG 双核下时序确定性比单核更差**
  （两核 TCG 线程交错），任何「等两核都跑到 X」的断言都要做成计数/旗标谓词。
- **并发金丝雀**（S-2 起常驻 smoke-smp）：分配器完整性探针（alloc/free
  roundtrip + 帧位图对账）、HandleTable integrity_check 既有面跨核版、
  canary 页（unmap 后写必 fault——shootdown 正确性的机器断言）、
  双核 ping-pong（每核一个任务经 channel 对打，吞吐/零丢失对账）。
- **回归门**：每刀 `just smoke` 四跑全绿（裸 23 + rootfs 20 + RX 43 +
  持久化 43 零漂移）+ EL2 同绿 + host 200 测——**单核路径是每刀的回归基线**，
  S 系列刀只在 smoke-smp 线上开 `-smp 2`。
- **soak**：S-5 收官跑 ≥30min 双核满载（纪律②；单核 30s 长跑的 SMP 版放大）。

## 6. 风险与挂账

- **PSCI 通道未实测**（§3.1）——S-0 硬门：没实证过 conduit/返回码/入口 EL
  不写一行 bring-up 代码。外部核实（HVC 默认/0xC4000003）只是缩小了假设空间，
  不是实证。
- **DTB blob/停车场邮箱落位与帧分配器区间碰撞**——S-0 实测；规避 = RAM_TOP
  下方预留窗口或落位探测后 carve out。
- **TCG 双核非确定性**——标记全状态谓词化（§5）；soak 失败必须可复现三连
  才立案（单核确定性纪律的 SMP 松弛版，复现不了记 frequency 账）。
- **BKL 让 wait 面变慢**：跨核唤醒在 E0 下要等对核出锁——`FAST ok` RT 基线
  在 `-smp 2` 相**必然劣化**（fastpath 直切的前提是本核持有调度权）。裁决：
  FAST ok 只锚单核相（既有四跑），smoke-smp 不复制该标记；S-3 IPI 落地后
  双核 IPC 面才有自己的基线。
- **ktest 契约面随锁纪元漂移**：PI 重推/FAST find_single_waiter 的「屏蔽窗内
  有效」契约在锁纪元下改为「KERN_LOCK 内有效」——ktest J/L 段注释与断言
  随 S-2/S-3 对齐（注释对齐实现，A-3 errno 先例）。
- **挂账 v2+**：异步 TLB shootdown（回执 IPI）；SPSC RX 环 + 任务迁移；
  per-Vmo/Vmar/per-ChannelEnd 锁（E2）；ASID 分代位图；\>2 核 + 拓扑发现
  （DTB `/cpus` 真机路径）；锁dep/延迟基准（G3）。
- **观察点**：S 系列刀遵循与 ADR #41 x86 同款的「主时间线内加法零扰」——
  每刀验收含单核四跑零漂移断言；若 SMP 线连续两提交周期 >20% 主线工时
  且无对应里程碑产出，回单核冻结（kill criteria 移植，立项 ADR 时正式化）。

## 7. 话术限定（对表口径）

- **E0/BKL 纪元的「双核」= 内核串行 + 用户并行 ≠ 完整 SMP**——两核并行的是
  用户任务，内核仍一次只进一个核。把它说成「SMP 内核」是口嗨；S-3（per-CPU
  调度器）落了才可以说内核并发。
- **G2 过关 ≠ G3**：G2 = 双核正确性（无竞态）；全抢占kernel/lockdep/延迟基准
  是 G3 面，v1「syscall 主体不可抢占」的免锁根基在 G2 纪元保留。
- **双核 ≠ 大小核**：capacity/EAS/拓扑感知放置是 M2（v3+）；本刀恒
  `-smp 2` 同构双核，`Topo::single()` 的多核化只到 `nr_cpus=2` 定值。
- 验证面 = QEMU TCG `-smp 2`（真机多核未验，G 卡类——盘点话术限定同族）。

---

## 执行结果（追加区）

（S-0 起逐刀追加：实测记录/落地清单/对抗审查结论/验收证据。）

### S-0（2026-10-22 已完成）：`-smp 2` 副核启动通道实证——纯 PSCI/HVC 钉死，调查刀零内核代码面

- **静态证据（dumpdtb，QEMU 11.0.3）**：`qemu-system-aarch64 -machine virt,dumpdtb=… -cpu cortex-a72 -m 128M -smp 2` → dtc 反编译：
  - **PSCI node**：`compatible = "arm,psci-1.0", "arm,psci-0.2", "arm,psci"`、`method = "hvc"`、`cpu_on = <0xc4000003>`、`cpu_off = <0x84000002>`、`cpu_suspend = <0xc4000001>`、`migrate = <0xc4000005>`——§3.1 外部核实（HVC 默认 / 0xC4000003）就此被 QEMU 自产 DTB 钉死，假设变实证；
  - **cpus node**：cpu@0 `reg=<0x00>` / cpu@1 `reg=<0x01>`（MPIDR Aff0 = 核号）、双 `enable-method = "psci"`、cpu-map socket0/cluster0/core0+core1（双核同簇）；
  - **零 spin-table / cpu-release-addr**——纯 PSCI 通道：CPU_ON 的 entry 由调用方传入、副核由 QEMU 内部管理，**guest RAM 无停车场邮箱结构** → §3.1 碰撞风险在此配置下不存在；DTB blob 本就不递（G-eco 实证 x0=0/魔数零命中）→ `[__kernel_end_pa, RAM_TOP)` 全空闲的既有假设在 `-smp 2` 下维持成立（四问之④收口）。
- **动态证据（live boot + monitor）**：既有内核 ELF 以 `-smp 2` 启动跑 ~3min——**boot 23 标记恰一次 + 稳态矩阵流动 + switches 金丝雀 100→1600 严格递增**，主核零扰动；monitor `info registers -a`：CPU#0 PC=`0xffffff804001aee4`（高半稳态）；**CPU#1 PC=`0x40000000`、X00/X01 全零**——恰等于内核 ELF entry（`PT_LOAD paddr=0x40000000`，python 解 ELF 头钉板）：**复位 PC 落在 `_start` 但副核 powered-off 未执行过任何指令**（寄存器原始零 + 若执行过 _start 数微秒内 bss 重清即毁系统——全程零毁即反证未执行）。
- **§1.2 HAL 行「所有核同跑 `_start` 全面灾难」限定收口**：PSCI 模型下该场景**不触发**（副核 halted 等 CPU_ON）；但复位 PC 恰是 `_start`——S-1 的 MPIDR 分流仍是防御性必做（非 PSCI conduit / 真机 spin-table 路径直接踩中该行）。
- **钉板值（S-1 消费）**：conduit = **HVC**；`CPU_ON = 0xC4000003`（x0=target_cpu MPIDR、x1=entry PA、x2=context_id）；`CPU_OFF = 0x84000002`；副核 MPIDR Aff0 = 1。**S-1 首步动态检查**（静态钉不死、须真调 HVC 后断言的两项）：①CPU_ON 返回码（成功 0/失败负 errno，DEN0022 口径）②副核入口 EL 与寄存器初值（SMP 启动协议：入口 MMU off、entry 传 PA、x0=context_id——x0 是否真带 context_id 以实测为准，不照规范想象写死）。
- **`-smp` 显式化**：justfile `MACH` 定值行补注「不带 `-smp` = QEMU 默认 1 核是 v1 有意定值（免锁根基单核串行）；S-1 起双核只进 smoke-smp 线」——隐式事实转注释；smoke-smp 脚本本体随 S-1 落地。

### S-1（2026-10-28 已完成）：副核 bring-up 基建——§3.2 十项 + 心跳握手 + smoke-smp 独立线

**落地清单**：`arch/aarch64/src/smp.rs`（新模块：`secondary_entry` global_asm §3.2 十项 + 主核侧 `smp_bringup` + `SmpReport` 16 字段自报区（编译期偏移全钉板）+ HB1/HB2/RELEASE 邮箱 + `SmpCpu` per-CPU 块）；`boot.rs`（`_start` MPIDR 防御分流 + 入口 EL 经 x20 钉存过降级路径 → `.Lbss_done` 落 `BOOT_EL`）；`lib.rs`（`pub mod smp`）；`tools/linker-aarch64.ld`（`.rodata` 指针字 `__smp_sec_entry = QUAD(secondary_entry)`）；`gic.rs`（§3.4 `SPI_ROUTES` 常量表 + `enable_spi_routed`；`init()` 注钉「主核一次、副核绝不」）；`uart.rs`（rx_init 路由经表）；`kernel/src/main.rs`（boot DAIF 屏蔽尾窗调 `smp_bringup(efi_boot)`——`rx_init` 后、`arm_slice` 前）；`tools/smoke-smp.sh` + justfile `run-smp`/`smoke-smp`（**全仓唯一 `-smp 2` 入口**；smoke-x86 同款独立形态，不并入主 smoke 不进 CI）。

**S-0 两项动态检查收口（实测钉板）**：①CPU_ON 返回码——`-smp 2` 默认机型 **0（成功）**；单核（无 `-smp`）**-2（0xfffffffffffffffe，目标核不在）**→ 一行降续 boot；②入口 EL/寄存器——默认机型副核**直达 EL1**（entry_el=1）、mpidr=`0x80000001`（U 位+Aff0=1）、**x0=context_id 原样回显**（`0x5ec0deadbeef0001`）、x1..x3=0（SMCCC 口径成立，非想象）。

**conduit 分型（S-1 最大实测收获，probe C 相矛盾日志的破案）**：HVC→PSCI 拦截**只在默认 `-machine virt`（boot EL1）成立**；`virtualization=on`（boot EL2）下内核先降 EL1 再发 HVC → 陷阱到 EL2 且 QEMU **不再**拦截为 PSCI → VBAR_EL2=0 → PC=0 静默挂死（主树首跑实录：`SMP: cpu_on entry=` 后 G1.5/SVC/AIDEMO 全缺）。修法 = **`BOOT_EL` 入口 EL 门**：`_start` 处（降级**前**）读 CurrentEL 经 **x20（callee-saved，eret 不动 GPR）** 携带过 `.Lfrom_el2`，`.Lbss_done`（.bss 清零后）经 PA 写入高半 `BOOT_EL`；`smp_bringup` 见 el≠1 即打 `SMP: skip (boot el=N; HVC conduit = default machine only, S-1 box)` 一行降级。**坑实录**：初版在 `.Lbss_done` 处才读 CurrentEL——降级后读恒得 1，EL2 机型门失效照样挂死（挂死证据 /tmp/s1-main-el2.log，修后 /tmp/s1-main-el2b.log 绿）。SMC 变体留档不实现（S-1 盒子）；EFI/handoff 路 `efi_boot` 门先跳过（conduit 归固件管，未实测）。

**跨半 ADRP 坑（2b-1 双向坑第三形态）**：高半 Rust 直取低半 `.text.boot` 符号（`secondary_entry`/`.bss.boot` 里的 `BOOT_EL`）地址即生成 ADRP 越 ±4GB 断链（R_AARCH64_ADR_PREL_PG_HI21 out of range）。两解法：入口 PA 经链接脚本 `.rodata` 指针字 `QUAD(secondary_entry)` 间接递送（副核 asm 用 `ldr =` 字面量，无 ADRP）；`BOOT_EL` 放高半普通 `.bss`、boot asm 经 PA=VMA−基址 写（与 .bss 清零循环同口径）。`assert-flip` 全绿（.text.boot ADRP 5 条全低半）。

**wfe/wfi 实测（TCG）**：`wfe` = 立即返回的 hint（副核 spins 计数疯涨实证）——park 等待用它必拖主核吞吐；**稳态 park 必须 `wfi` 真睡**。握手邮箱用 wfe 无妨（窗口毫秒级）。心跳序 = 副核置 HB1 → wfe 邮箱（先查后 wfe 再查，丢唤醒免疫）→ 主核 dump 报告 → RELEASE.store+sev → HB2 → `SMP ok ncores=2`（两心跳皆内存状态断言，非竞速——4-5a 纪律）。

**per-core 复位证据（报告区实测值）**：CNTKCTL 副核复位 0（主核已开 bit1 不串核）→ 自开 EL0VCTEN 后读回 2；GICC CTLR 使能前 0（banked 独立）；GICD ITARGETSR[33]=1（主核 SPI_ROUTES 所写、副核只读可见——§3.4 路由表闭环）；TPIDR_EL1 写读回 `SMP_CPU1` VMA。**UART 单写者纪律成立**：副核全程只写 1 个哨兵字符 `'s'`（PA 直写、入口第一证据），日志形态 `sSMP: sec up …` 零撕行；主核从 CPU_ON 到 HB1 观测间静默。

**smoke-smp 线（gate 首跑全绿）**：断言三面 = `SMP ok ncores=2` 恰一次（grep -cF）+ 裸相 26 标记 + switches 金丝雀 ≥3 严格递增（smoke.sh 同款 awk 体）；反向断言 `SMP: FAIL`/`SMP: skip` 出现即 FAIL。时限 = 单核 ×2（boot 120→240s、金丝雀 180→400s——双核 MTTCG 吞吐实测 0.61x）。实测 canary 100/200/300 严格递增，PASS。

**单核零漂移（终值实录，2026-10-28 04:4x）**：主 smoke 五相全绿 SMOKE_RC=0（裸相 26 / rootfs 相 33 / RX·FAT 合相 59×3，canary 25 条严格递增——基线 33/59 系并行刀 C-3（PS/BC）/C-4（TAB）上移，非本刀漂移）+ run-el2 裸相 26/26（`21BerryOS boot` + `SMP: skip (boot el=2; …)` 恰一行）+ host 363/0（338 + C-3 berry-psproto 15 + C-4 berry-complete 10）+ abi-check + smoke-x86 7 标记恰一次 + assert-flip（.text.boot ADRP 5 条全低半）全过。**icount L2 刷新实录（K-1 骰子 benign 残留）**：frame_alloc4k 168→78、vmo_commit1p 444→354（确定性双跑同值；其余四项精确相等）——内核镜像变更（本刀 smp.rs/boot.rs/linker `+8B` 指针字，叠并行刀 bsh 平像增长）挪 `__kernel_end`、重掷 boot 期 buddy 空闲表拓扑（K-1 gotcha 预告形态的良性残余；mm/ 零改动，BENCHXH/BENCHCHURN/宿主位图-线性对拍全绿证不变量）→ `bench.py --icount --update` 刷新基线（双值 diff 随源入册），复验 RC=0。**f42t 边缘 flake 实录（两次）**：主 smoke 首两跑 RX 相 f42t 60s 未出现——根因 = 并行刀 C-4 的 console 逐字节转发**批不耐受**（单唤醒只发 1 字节，一次唤醒收 ≥2 字节即丢前留尾；忙窗内通道满时**连顺序单字节到达也会被 pending 覆盖丢**——非仅并聚一路，C-4 会话确认两路同修；`fdrop` 仅步 a 打一次、后续丢字不可见）+ 段 d `run fail` 内核搅动窗拉长唤醒延迟致字节并聚 → bsh 收到残片走 `usage:` 拒面；faithful 复刻（smoke.sh 同形 59 标记门 + 段 a-d）单核四段全绿（f42t 31s、fdrop=0，本刀内核在证：`SMP: cpu_on entry=` + `ret=-2` 降级行）+ 终跑 SMOKE_RC=0——宿主负载下的边缘竞速，非 S-1 确定性回归（诊断已送达 C-4 会话，修复归其刀）。

### S-1 修复轮（2026-10-28，三路审查去重 6 findings 修 6）

- **release 门铃序补 DSB（major）**：主核 release 序原为 `RELEASE.store` 紧跟 `sev`（中间零屏障）——AArch64 上 SEV 不保证此前 store 已对其他核全局可见，副核 park 循环（查条件→wfe→醒来重查一次）每记 SEV 恰兑换一次额外重查：store 在途则副核回 wfe 后事件寄存器已清、全系统再无唤醒源 → 真机按次随机睡死（收口 `SMP: FAIL hb2 timeout`）。TCG wfe=hint 立返（副核忙轮询）结构性掩蔽此窗，恰因此必须在代码修掉。修法 = `store → dsb sy → sev` 同门铃序（smp.rs，反汇编钉板 `stlr/dsb sy/sev` 三连）；smp.rs 头注/RELEASE 邮箱注的「丢唤醒免疫」表述同步勘正为「依赖主核侧 DSB 的配套序」。
- **`_start` MPIDR 防御分流改全亲和域（minor ×2 同根）**：原 `tbz x1,#0`（单测 Aff0 bit0）——偶数非零 Aff0（4 核平铺 core2/core3）与双簇拓扑（cpu1=Aff1:1,Aff0:0）都会误入主核路径（重清 .bss 全面灾难），恰是该防御分支自称要防的形态。修法 = `tst x1, #0xffffff`（Aff0..Aff2 bits[23:0] 合法掩码立即数）+ `b.ne secondary_entry`，指令数不变；S-1 盒内（PSCI 副核从不落 _start）行为零变化（单核/双核主核 MPIDR 亲和域均 0）。
- **`SmpCpu` 布局编译期钉板（minor）**：镜像 SmpReport 纪律补 `offset_of!`/`size_of` const 断言（cpu@0/mpidr@8/present@16/size=64）——secondary_entry asm 硬编码偏移 `[x24]/[x24,#8]/[x24,#16]` 此前无守卫，重排字段静默错位要到 S-3 per-CPU 消费时才显形。
- **PSCI asm! 补 x18 clobber（minor）**：SMCCC x18 = 平台寄存器、规范允许固件不保留（真机 ATF 可改写），原 x4-x17 out 列表漏它——与注释「不赌实现保留」自相矛盾。补 `out("x18") _` 同列。
- **进度锚 true-up（minor）**：CLAUDE.md/AGENTS.md「在途拆解」行 `S-0 已落` → `S-0/S-1 已落`（两文件同口径）。
- 修复后复跑：主 smoke 五相 / run-el2 / smoke-smp / assert-flip / abi-check 全绿（详见任务报告 rerun_results；boot 路径触及 → 单核 smoke + smoke-smp 必复跑面覆盖）。

### S-2 全刀收官（2026-10-31 段 A+B / 2026-11-01 段 C=S-2b）：E0 BKL 纪元——内核串行 + 用户并行立证

- **段 A+B（S-2a，单核可证）**：KERN_LOCK 锁底座（TTAS + PRINT_LOCK + panic bypass + irqsave/拼行 API）+ arch 槽注入 + Task.klock_window 位五切走点舞蹈 + F31-F42 直调包络 + 前置件 2（计数原子化 + CPU0 per-CPU 块/LAST_VCT）+ D24/I50 读侧原子化 + 契约注释立法——d1cf130 / 5a9a00f，单核全绿 + icount 三 delta 复位。**详录 → `docs/SMP-S2实施设计草案-v1.0.md`（子案，共享面 54 项处置档总表 = S-3 拆锁地图）。**
- **段 C（S-2b，副核参与）**：gate（`SMP_PAR`，smp_bringup + par_probe spawn 后置位）+ `secondary_main` 高半入口（指针字 blr + 探测/参与统一环 + banked PPI27）+ `PickFilter` 过滤出选（副核 user_only + Running-elsewhere 排除；**副核只读版 peek**——单游标回卷饥饿修复）+ park 三切点（`SMP_SEC_PARK_CTX`）+ **park mod4 限频**（console 多生产者深饿源头治理——占空比 25%）+ CURRENT per-CPU 访问器 + CPU0 idle 参与态改形 + PAR 金丝雀（验收翻转：限频下 witness 归 S-3，验收面 = `PAR absent` 行 `d1=` 非零）+ 副核高半栈（低半栈向量风暴修复）+ §5.4 帽 ×2 全账面 22 笔 + smoke-smp 双相扩相（裸 27 + rootfs 45 + EVS/SKT 对拍 + R3 三数）。
- **验收（2026-11-01 全链）**：test-host 665/0 + 主 smoke 八跑零漂移（单核非竞争路径不变）+ **扩相 smoke-smp 首跑 PASS**（R3 三数 boot 6s（帽 2.5%）/金丝雀 ≈0s/rootfs 129s（帽 15%）免升档；**PAR witness 限频下仍提前命中 `PAR ok c0=256 c1=2`**）+ run-el2 同绿。E0 话术兑现：**内核串行 + 用户并行**——rootfs 生态全谱在 `-smp 2` 全绿 = 串行语义等价的端到端证据。
- **三边界之外的实战三雷**（详录沿革 2026-11-01 段）：① RUNQUEUE 单游标回卷饥饿（副核消费版 pick 回卷 CPU0 FIFO 序——修 = 只读 peek）；② console 单通道多生产者深饿（fix4/fix5 双向实锤「无界生产下下游怎么调都错」——修 = 源头 mod4 限频）；③ 副核低半栈向量风暴（park 帧 SP 落已退役低半映射——修 = 高半静态栈）。**S-3 交接口**：per-CPU runqueue/IPI（witness 满速验收 + tickless idle 恢复）/CURRENT 真体化/遥测双环——§7 子案在册。

### S-3a（已收官——段② 2026-11-01 A 半刀 82ab456 + 义务① SGI 除名，probe ① 命中后全链落地）：SGI IPI 面首步——SGIR 原语 + 双核 banked 使能 + CPU0→CPU1 单发零代码观测

- **SGIR 字段实证钉板（写码前 QEMU 源码核对——S-0 实证纪律，防规范想象错码）**：QEMU `hw/intc/arm_gic.c` `gic_dist_writel`：SGIINTID=`value & 0xF`（bits[3:0]）、CPUTargetList=bits[23:16]（filter 0 `mask=(value>>16)&ALL_CPU_MASK`）、TargetListFilter=bits[25:24]（0b00 列表 / 0b01 除己 `ALL_CPU_MASK^(1<<cpu)` / 0b10 仅己 `1<<cpu`）；**仅受理 32-bit 整字写**（子宽落 bad_reg 丢弃——A6 宽度敏感同族）；REV_11MPCORE 与标准版 SGIR 解码无分支差异。与子案 §2.1 布局一致（曾疑与 ARM IHI0048 记忆冲突——QEMU 为 ground truth 裁定）。probe 字 = `0x0002_0001`（SGI1→CPU1）。
- **落地最小件（三文件）**：`gic.rs`——`SGI_RESCHEDULE=1` 常量（2=TLB 回执 S-4 预留不消费；**0 保留不用**，复位默认值域观测面干净性）+ `GICD_SGIR=0xf00`（宽度注同上）+ `SgiTarget{List/Others/Self_}` 直映 filter + `send_sgi`（任意上下文可调：无锁零分配零打印，IRQ 上下文纪律合规）+ `enable_sgi_this_core`（擦历史 pending + banked 使能，`enable_cntv_this_core` 同款纪律）+ init() 内 CPU0 banked 补使能（§8.B「全禁坑」收端义务；**单核相零漂移**：只使能无发送者，面结构性不触发）；`smp.rs`——`smp_bringup` 尾（`SMP ok ncores=2` 之后、boot DAIF 屏蔽尾窗）CPU0 单发 `send_sgi(SGI_RESCHEDULE, List(0b10))`；`main.rs`——`secondary_main` 起手序插 `enable_sgi_this_core`（`enable_cntv_this_core` 之后——副核 bank 复位态不可假设，收端使能面收端自理）。
- **probe ① 实测命中（2026-11-01，`just run-smp`，log /tmp/s3a_probe1.log 行 134/207）**：`SMP ok ncores=2` → `PAR gate on` → **`irq? id=1`** → `F FAST ok`——恰一次投递、无风暴无重复（trap.rs 未知源 loud 面零代码观测，§8.B probe 通道兑现）；boot 全程裸相 27 标记完整（BENCHWAKE 到齐；`FILE`/`EXIT` 属 rootfs 相非裸相）。
- **竞速免疫构造兑现**：probe 发送点（CPU0 bringup 尾）与副核使能点（secondary_main 起手）相互无序——pending 在 CPU1 bank 锁存、**使能前不投递**、使能即投递（实测恰一次无丢 = 构造依据兑现；「先发后使能不丢」语义自此刻入册）。
- **banked 纪律入册（§8.B 全禁坑的收口形态）**：INTID<32 的 ISENABLER0/IPRIORITYR per-CPU banked，init `ICENABLER0=!0` 每核各禁各的 bank——SGI 收端必须逐核补使能（CPU0 init 内 + 副核起手）；IPRIORITYR banked 复位 0（最高优先级）免配；GICD_CTLR=1 全局面已开 SGI 路由无需再动。
- **probe ② 落地（同日次段——消费臂 + 收讫标记）**：`trap.rs` exception_irq 分流补 `SGI_RESCHEDULE` 臂（EOIR + `SGI_SEEN` 原子计数——唤醒戳语义：到达即已出 wfi；SGI = bank pending 位一次性边沿**无去活电平**（CNTV/PL011 电平铁序不适用）；不进 tick 槽；真消费（重查 runqueue）随 S-3b per-CPU 队列落）+ `smp.rs` bringup 尾 `wait_secs(3)` 状态谓词收讫等待（HB2 同款非竞速）→ `SGI ok n=<计数>` 标记 / 超时 `SMP: FAIL sgi timeout` 链断 loud（进 smoke 反向快败）+ smoke-smp `smp_assertions` 扩 **`SGI ok` 两相各恰一次**（不进标记数组——主 smoke 单核结构性不触发零漂移）。**实测（run-smp 快观察）**：`SMP ok ncores=2` 次行即 `SGI ok n=1`（~1s 内）；probe ① 的 `irq? id=1` 观测通道退役（消费臂接管，还给真未知源）。
- **probe ③ 落地（同日末段——形态 + 合并 + Self 锁存三观测收官，SGI 观测面全毕）**：`gic.rs` 补 `GICD_ISPENDR0=0x200` + `sgi_pending_this_core`（本核 bank pending 直读；安全壳内 unsafe 块——`verify()` 同款形态）+ `kern_lock::irq_enabled` 升 pub（DAIF.I 直读观测位）；`smp.rs` bringup 尾 probe ② 块后追加三观测块（`SGI3` 前缀与 `SGI ok n=`/`SMP: FAIL` 断言面零碰撞、观察行不进 smoke 断言）。**实测（2026-11-01，-smp 2 五跑）**：① Others(0b01) `SGI3 others ok n=2` 五跑恒命中——双核上 ≡ List(0b10) 同效（QEMU `ALL_CPU_MASK^(1<<cpu)` 钉板兑现）；② 同源背靠背双发 `SGI3 merge ok n=3 merged` / `n=4` **跨跑双形态**（合并 vs 逐发竞速并存——GICv2 同源 SGI 一 pending 位，CPU1 active 窗内外两种时序皆可现；「无条件发送纪律」设计不受影响：发送侧无需知悉合并与否，五跑 n 单调不丢发为证）；③ Self(0b10) `SGI3 self ok pending=1 seen=<n> daif_i=0`——**屏蔽窗内 pending 锁存 ISPENDR0 直读自证**（发端版「先发后解蔽不丢」，与 probe ① 收端版互补；投递机器 ①② 已证，锁存后投递 = enable_irq 时由 SGI_SEEN 收讫，无观察点不赘）。
- **副产物：boot DAIF 屏蔽窗双洞修复（判例 A105）**：Self 观测首跑 pending=0（≠预期 1）→ daif 六锚定界法（rust_main 入口/ktest 后/bench 后/sched::init 后/bringup 入口/观测前——`irq_enabled()` 直读一行锚）揪出**两处窗违约**：洞① boot.rs `.Lat_el1` 直入路不动 DAIF、继承 QEMU reset DAIF=0（解蔽）——仅 EL2 eret 路 SPSR 0x3c5 屏蔽，双径不对称；洞② ktest `switch_ping_fn`/`fp_ping_fn` 经 task_entry_trampoline 起步 `daifclr #2`（真任务收 tick 的正当设计）后**带着解蔽态切回 MAIN**——boot 自首个 ktest 乒乓起整窗解蔽，兼违 context_switch `PSTATE.I==1` 不变式（当时无已使能源故 benign 不爆）。修复 = `.Lat_el1` 统一 `daifset #0xf`（EL2 路幂等）+ 两 ping fn 起手 `disable_irq()` 复蔽。修后六锚全 0、obs-③ pending=1 命中；run-el2 行首 `21` + boot 全绿（eret 路幂等实证）。「boot DAIF 屏蔽直到 enable_irq」自注释意图升**实测可证契约**。
- **下一步（§9.4 S-3a 六件义务）**：SGI 观测面（probe ①②③）收官；子案 §9.4 六件义务接棒——无条件发送纪律/park 判据含对核有货/SLEEPQ 到点武装归属核/F-E5 不变量/rearm 锚 timer IRQ 入口/mod4 退役判据。

### S-3a 段② 收官（2026-11-01 A 半刀 + 义务①除名，commit 82ab456）：副核 tickless park——半刀独立成立

- **步 A 落地（②+⑥）**：`park_recheck` 自 on_irq_tick cur==null 段整体提取为 `pub unsafe fn`（klock→gate 早退→peek_next_filtered 只读→sS2b 诊断门→**出选真任务自 arm_slice 授片**→set_current→SWITCHES++→activate_aspace→放锁→context_switch(sec_park_ctx,next)）；on_irq_tick 留 expire/抢占面、cur==null 改早退；preempt 路径 mod4 门移除（⑥退役，`park_due` 标 retired）。副核 park 环（secondary_main）= `loop { park_recheck; par_sample(0,false) 清钉; gate 开→match nearest_deadline { Some(d)=>arm_at(d) 浅 park, None=>mask() 深 park }; gate 关→arm_slice 探测态; wfi; enable_irq; disable_irq }`——**masked-wfi 形**（enable→wfi 间 IRQ 先服务后直睡的丢唤醒窗收口，Linux arm64 cpu_do_idle 同款）+ **PAR 清钉洞**（深 park 窗 last_task 滞留 → CPU0 witness 假见证，wfi 前清）。`timer.rs` 补 `arm_at(deadline)`（CVAL 绝对编程）+ `handle()` 头注升格 **F-E11 执法点** + 授片 arm/rearm 二分名义（⑤）。
- **义务①除名（实验裁决，判词全文 → 子案 §9.5 末）**：A+B 合并刀（enqueue 尾 SGI 戳）落码后 smoke-smp 连 FAIL——peek 只读扫描恒先命中恒 Ready 的 pinned child（398/398 全 child，+25% 灌压）→ klock/MTTCG 护送队推慢 ponger/svcmgr → pinger/svcmgr wait 帽断 → 0x4000_0000 自陷 → CHAIN/MANY/SVC/DEMO 四标记级联饿死；SGI 二分静音后全绿 + **深 park 截止驱动自续 27 笔/150s**（「A 无 B 结构性饿死」前提证伪——child 恒 deadline 阻塞入 SLEEPQ，arm_at 到点自醒）。裁决 = ①除名降级 S-3b 路由化件（发送条件 = 目标核队列真有货，届时派发价值成立）；**SGI_RESCHEDULE 生产发送面归零，在册发送者仅 bringup probe ③ 观测块**；trap.rs 分流臂转 probe 专用。③④规则同批成文（子案 §2.4 实施定案段 + 母案 §3.3——SLEEPQ 插入只在运行核 = 插入核单点武装；迟醒界 ≤1 slice 来源 = 运行核 slice 节奏 F-E5）。
- **潜伏双雷记档（S-3b IDLE 深化件）**：① on_irq_tick cur==null 槽原兼 CPU0 idle 再派发——迁出后为 wedge（金丝雀永续 runnable 使测试面不真空，未触达）；② `sec_park_ctx()` 全局单槽——两核共穿会跨核覆写 park 帧（从未触发；wedge 比静默腐坏安全，per-CPU 化随 S-3b）。
- **验收（2026-11-01 全链，零欠账）**：smoke-smp 双相 PASS（SMP ok/SGI ok 各恰一次 + 裸 27 + rootfs 45 + **PAR ok c0=354 c1=4 满速参与下提前命中**（mod4 限频退役后 c1 从 absent 档升活证档）+ 金丝雀 500→8800 严格递增 + R3 三数 boot 5s/金丝雀≈0s/rootfs 129s 帽 840s）+ 主 smoke 八跑 PASS（39 递增 + 全相链——中途 echod 18080 与 GUI 线并行 smoke 边界竞态一次，等场重跑绿）+ run-el2 `21BerryOS boot` 恰一次 + test-host 全绿 + docs_guard 0 警 + unsafe 台账 631 --check ok（MMIO -1 = send_sgi 块除名）。
- **下一步（S-3b per-CPU 队列路由化）**：①spawn 四写点前移 + enqueue 出表窗（F-E1）②偷取双过滤（F-E2）③LB 金丝雀 per-child 归因 + 私有 handle 表 + svc 驱动链补 vmar_map ④smoke-smp 相 3 `LB ok` 严格必需 ⑤帽序三处 ⑥REAPQ 消费权维持 CPU0 ⑦ktest 横切锁纪律成文 ⑧**唤醒戳回归**（本刀除名件——发送条件随路由化成立）+ IDLE 深化（双雷 per-CPU 化）。
