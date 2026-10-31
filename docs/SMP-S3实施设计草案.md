# SMP S-3（E1 per-CPU 纪元）实施设计草案

> 状态：**立稿初版 2026-11-01**——E1 分档视图 + SGI IPI 面 + 拆步骨架已立；**多路侦察细读 + 三镜对抗审查待补**（S-2 同型流程），审查后冻结实施序再动代码。
> 母案：`docs/SMP设计草案.md` §2.1 E1 行（语义权威）+ §4 S-3 行（范围权威）+ §3.3 SGI IPI 面（硬前置）+ §1.4 偏差对账（调度器设计 §3.5「per-CPU runqueue day-one」声明由本刀兑现改写）。
> 直接输入（S-2 §7 逐字移交）：`docs/SMP-S2实施设计草案.md` §1 共享面盘点 54 项处置档总表（= 拆锁地图）+ §3.3 切换原语舞蹈表（= 取放点地图）+ §7「留给 S-3」清单。
> 基线：eaab6c6（S-2b 收官态）；行号锚 = 立稿时点，实施后按符号定位（S-2 先例）。
> 话术限定（母案 §7）：E0 =「内核串行 + 用户并行」；**本刀落地后始可称「内核并发」**（两核可并行进内核不同区域）。

## §0 定位与边界

**定位**：把 E0 的「一把 KERN_LOCK 盖全内核」细化为**逐结构锁 + per-CPU 独占存储**——内核临界区从「全系统」缩到「逐结构」，两核可并行进内核不同区域。裁决依据三条照旧（母案 §2.1：语义等价小步 / 激进配置常驻 soak 逐块转正 / 少发明新同步原语——自旋锁 + per-CPU + 原子计数器三件套为度，RCU 级发明不做）。

**S-3 只做 E1，禁混清单**（出现任一项即范围失控）：

| 禁项 | 归属 | 本刀形态 |
|------|------|----------|
| per-ChannelEnd / per-Vmo/Vmar 逐域锁 | E2，S-4 起/v2 | 对象系维持**单把收缩版 KERN_LOCK**（「对象系/表锁」——S-2 §7 退役路径第一步，E2 才逐域下放） |
| TLB shootdown / canary 页 | S-4 | SGIR 号位**预留** SGI_TLB_DONE 不消费；shootdown 维持同步 IS 广播 |
| lockdep / 全抢占内核 | S-5/非目标 | 注释纪律执法延续 |
| affinity / 任务迁移 ABI 面 | v2 | ABI 无此号不破冻；「放置」= 入队侧策略（本刀新增的内核内部路由，非用户可见面） |
| RX 环 SPSC 化 / console 迁移 | 迁移放开时再议（母案 §3.4） | E26 钉核契约维持（IRQ33→CPU0 + console 任务钉核 0） |

**已在 S-2 前置收账项（本刀不重做，母案 E1 行相应子项即已兑现）**：LAST_VCT per-CPU（A11）、CURRENT per-CPU 访问器（A3，S-2b TPIDR_EL1 两核各自）、计数/kill 计原子化第一批（A1/A2/A12/D24/I50）、CPU0 per-CPU 块（前置件 2）。本刀将其「真体化/深化」而非重建。

## §1 E1 分档视图（S-2 §1 总表 54 项的 E1 重处置）

母案 E1 行五件套对表：**per-CPU 化**（CURRENT 真体化/RUNQUEUE/IDLE 深化/遥测双环）+ **自旋锁**（SLEEPQ/REAPQ/buddy/slab/ASID）+ **计数器 AtomicU64**（剩余面）+ **SGI IPI**（§2）+ **work stealing 初版**（§3）。逐项处置（编号承 S-2 总表）：

| 组 | 项 | E0 现处置 | E1 处置 | 锁/形态 |
|----|----|-----------|---------|---------|
| 队列 | A4 RUNQUEUE | KERN_LOCK 锁内全局单队（sched.rs:71 + `Topo::single()` :73） | **per-CPU 每核队列 + Topo 真化**（调度器设计 §3.5 兑现）；放置 = 入队侧路由（内核任务 pinned==0 → CPU0 队；用户任务按负载/空核偏好入队——侦察定案） | 每核队列锁（`SpinLock<Runqueue>` 形态——侦察③） |
| 队列 | A5 SLEEPQ | KERN_LOCK 锁内 | **独立自旋锁**（睡眠/唤醒/超时/expire 全路径改取睡锁）；unlink「锁内」契约表述照旧 | SLEEPQ 锁 |
| 队列 | A6 REAPQ | push/drain 均 KERN_LOCK 锁内 | **独立自旋锁**；drain 摸分配器 → 分配器锁序见 §3 嵌套序 | REAPQ 锁 |
| 调度 | A3 CURRENT | per-CPU 访问器（S-2b 过渡形态） | **真体化**：sched 内部对全局 static 的回落路径收编、注释对齐（行为不变，形态收敛） | per-CPU（免锁维持） |
| 调度 | A7 IDLE | IDLE_TASK 单份（CPU0）+ 副核 park ctx（S-2b §2.3） | **per-核 idle 环统一**：pick 本地 → steal 探测 → 空则 tickless wfi（§2.4）；CPU0 idle_loop 与副核 secondary_main 环合流为同一 per-core idle 例程；IDLE_TASK 符号退役或降格为 CPU0 idle ctx（侦察定形态——「不立第二个 IDLE_TASK」口径延续，深化非推翻） | per-CPU |
| 调度 | A8/A9 integrity/expire | tick 锁窗内 | 随 tick 面拆：本核队列锁内执行（integrity 的跨队走查面 = 侦察重设计点——只查本核 or 双锁聚合快照） | 随队列锁 |
| 调度 | A10 kill_all | KERN_LOCK 全程 | 维持大锁窗（杀链跨任务跨表，粒度不拆——「零阻塞零让出」红利延续；挂 E2 议） | 收缩版 KERN_LOCK |
| 遥测 | B12/B13 双环 | 单环 KERN_LOCK 串行（S-2 撤销记录维持正确） | **per-CPU 双环**（母案 E1 原位——S-2 §7 留面；可观测性设计原案拆核口径随侦察对齐）；读者（watchdog/BENCHWAKE）改聚合快照 API | per-CPU 写 + 聚合读 |
| 对象系 | C20 表/对象 | KERN_LOCK 锁内免费 | **KERN_LOCK 收缩为「对象系/表锁」**（更名或收窄——S-2 §7 退役路径第一步；svc_enter/svc_exit 取放点不动，锁的覆盖面从「一切」缩到「对象系/表」） | 收缩版 KERN_LOCK |
| 对象系 | C21 Event.signals | 非 atomic + 纪律注释 | **AtomicU64 化**（S-2 §7 备选转正——拆锁后「跨核写者全收口」论证失效，原子化一劳永逸；成本极低） | 原子 |
| 对象系 | C22 ObjRef | 锁内免费 | 随收缩版 KERN_LOCK 维持 | 收缩版 |
| 分配 | D23 buddy/slab | KERN_LOCK 锁内（E0 无锁铁则） | **独立自旋锁**（buddy 一把 + slab 一把，或合一——侦察定；`frames()` 类型面真修（D25 挂账）随本刀收账：受赠者改 `&FrameAllocator` 只读或经锁封装） | 分配器锁 |
| 分配 | D25 frames() 别名 UB | 锁纪律兜底 | **类型面真修**（S-2 §7 留面 E1+——随分配器锁重构一次做掉） | — |
| 中断 | E25 on_irq_tick | tick 全局锁窗 | **tick 面最终形态**（S-2 §7 留面）：计数/金丝雀照旧原子；expire→SLEEPQ 锁；pick→本核队列锁；slice 记账→per-CPU（A11 已就位）；`switches=` 打印照旧打印锁 | 逐段取放 |
| 中断 | E27/E26 RX | KERN_LOCK 短窗 / 钉核 | E26 钉核维持；E27 短窗改取**对象系/表锁**（wake_matching 的 SLEEPQ 段改睡锁——锁序见 §3） | 逐段 |
| IPI | （新增）SGI 面 | 零（未知源打成 `irq? id=N` trap.rs:443-449） | **§2 全新** | 原子/硬件 |
| F/I 族 | 直调包络/已原子面 | klock_enter/exit | klock 包络**改取逐段锁**（klock_enter 语义 = 取对象锁 + 屏蔽 IRQ 维持——侦察重映射每包络点触达的结构集合） | 逐段 |

**不在本刀**（对表收口）：G43/G44 打印锁维持（争用剖面真机数据后再裁——S-2 §7）；H45-H48 boot 窗维持；I49/I51 维持；I50 已原子。

## §2 SGI IPI 面（从零新增——母案 §3.3 四件）

### 2.1 GICD_SGIR 写原语

`arch/aarch64/src/gic.rs` 新增 `send_sgi(id: u8, target: SgiTarget)`：GICD_SGIR（偏移 0xF00，**32-bit 整字 MMIO 写**——宽度敏感铁律 A6 同族）字段 = TargetListFilter[25:24]（0b00=列表 / 0b01=除己外全体 / 0b10=仅己）+ CPUTargetList[23:16]（affinity 0 簇内每核 1 bit）+ SGIINTID[3:0]。`SgiTarget::{List(u8), Others, Self_}` 三态直映。**实证纪律（S-0 同款——断言值钉实测，不照规范想象写死）**：QEMU GICv2 下 SGI 到端 INTID 形态（0..15 段）、pending 合并语义、banked ISENABLER0/IPRIORITYR 使能与优先级面，S-3a 首步 QEMU 实测记录入册（`-smp 2` 下两核互发 + 收端 `irq? id=` 观测起步）。

### 2.2 SGI 号分配（2 个起）

| 号 | 名 | 用途 | 消费者 |
|----|----|------|--------|
| 1 | `SGI_RESCHEDULE` | 跨核唤醒戳：入队者发现目标核非当前核/park 态即发；收端 handler = 无条件重验 pick（幂等——pending 合并语义下丢/重均无害，pick 兜底） | S-3a |
| 2 | `SGI_TLB_DONE` | shootdown 完成回执 | **号位预留不消费**（S-4） |

SGI 0 保留不用（0 号是复位默认值域，观测面干净性）。

### 2.3 exception_irq 分流表加 SGI 段

trap.rs INTID 分流（:400 起）加 `0..=15 => SGI 段`：SGI_RESCHEDULE → 收端本核 pick 重验（不取大锁——只置 per-CPU `resched_pending` 原子位 + 尝试触发调度点；具体形态侦察定：直接 pick or 延迟到安全点）。未知 SGI 仍走 `irq? id=` loud 面。

### 2.4 park/release 改形 + tickless idle 恢复（E0 两大代价的根治）

- **唤醒延迟 ≤1 slice（S-2 R2）根治**：跨核唤醒路径（wake_matching 命中他核任务 / 入队到他核队列）补发 SGI_RESCHEDULE——被唤任务立刻上核，不再等目标核下次 tick。
- **tickless idle 恢复（S-2 §2.3 主核 idle 改形的回卷）**：per-核 idle 环空判（本队列空 + 无 steal 货 + SLEEPQ 无到点者——侦察细化判据）→ **mask CNTV + wfi**（真 tickless park）；唤醒源 = SGI_RESCHEDULE（入队者发）+ RX IRQ（CPU0，不可依赖只作兼带）+ 到点任务路径（SLEEPQ 最近 deadline → 该核 CVAL 单点武装——侦察定「无到点者全 mask / 有则单点武装」双态形态）。gate 关（单核）维持现形态零漂移（S-2 先例口径）。
- **park 限频 mod4 退役**：副核满速参与（S-2 雷 2 治理的限频占空比在 IPI + 满速参与后失去存在理由——首跑盯 console 饱和面是否复发，复发则限频保留为诊断档）。

## §3 拆锁时序与锁序（S-2 §3.3 舞蹈表移交面）

**staging 裁决（语义等价小步的落地形态）**：

1. **S-3b 先拆结构、后缩窗口**：per-CPU 队列/睡锁/回收锁全部落地时 **SVC 入口仍取全量 KERN_LOCK**——结构已分、锁窗未缩：双核仍是内核串行，但放置/偷取/每核 tick 逻辑先行可证（语义等价 = 单核行为不变 + 双核行为等价于 E0 可能交错）。
2. **S-3c 再缩窗口**：svc_enter/svc_exit 的 KERN_LOCK 收缩为「对象系/表锁」，调度/睡眠/回收/分配器路径改取各自结构锁——此步后才真正「两核并行进内核不同区域」。

**锁序立法（死锁面预防——初版最小序）**：

- 全序：**队列锁（先远端后本地，按核号升序双持）> SLEEPQ 锁 > REAPQ 锁 > 分配器锁 > 对象系/表锁（收缩版 KERN_LOCK）> 打印锁**——侦察逐路径核对该序在现实调用图上无逆序边；任何逆序边即重排或合并（少发明原语纪律）。
- 偷取锁序特则：**只持远端**（偷取 = 放本地锁 → 取远端锁 → 远端 dequeue → 放 → 本核无锁持有该 Task——任务此刻无队列归属，入本核队列时再取本地锁；「双队列同时出现」不变量由「出队后无归属窗 + 单点入队」构造性保证——细案侦察定）。
- 切换原语舞蹈表（S-2 §3.3 五原语）逐行重映射：每原语的「切前放什么锁/切回取什么锁」从单锁位演化为锁集合枚举——**侦察面 A（最大细读面）**：yield_now/block_current/try_preempt/switch_away_forever_with/on_irq_tick 各自触达结构全列。
- IRQ 上下文纪律：SGI handler / tick / RX 的取锁面按「短窗 + 永不睡眠 + 永不嵌套打印锁之外的锁」复核（S-2 §2.2 对表结论重裁——母案「IRQ 侧收 per-CPU-only」偏好在 resched_pending 位上部分成立）。

**work stealing 初版**：偷取触发 = 本核 pick None 且对核队列非空（idle 环探测点）；粒度 = 单任务；方向 = 双向（CPU0↔CPU1 用户任务；内核任务永不偷——pinned==0 全钉 CPU0 维持）；per-CPU `steal_cnt` 诊断计数（验收面用）。负载均衡初版 = 「入队侧空核偏好 + idle 偷取」二件（主动迁移不做——挂 v2）。

## §4 拆步表（每步独立可验收；缝 = 可回退面）

| 步 | 内容 | 验收 | 标记 |
|----|------|------|------|
| **S-3a SGI IPI 底座 + tickless 恢复** | §2 四件：SGIR 原语 + 实证记录 + SGI_RESCHEDULE 分流段 + 跨核唤醒补发 + per-核 idle 环统一（tickless park 恢复）+ park mod4 退役 | `just smoke`（单核零漂移——单核无发送者，SGI 面结构性无扰动）+ `just smoke-smp` 双相全绿 + `just test-host` + `just build-release` + `run-el2` | smoke-smp 照旧双相（提速不进门）；QEMU SGI 实证记录入母案执行结果区 |
| **S-3b per-CPU 队列 + 偷取（锁窗未缩）** | A4/A7/A8/A9 + 放置路由 + pick_filter 退役改形 + work stealing 初版 + LB 金丝雀（§5）；SVC 入口仍全量锁 | smoke-smp 全绿（rootfs 相 = E0 语义等价再证）+ 主 smoke 零漂移 + test-host + icount 刷新（队列路径指令数变——S-2a 先例） | **`LB ok`（G2 验收项 1，§5）** |
| **S-3c 结构锁族 + KERN_LOCK 收缩 + 遥测双环** | A5/A6/D23/D25/C21/C20 收缩/E25 tick 最终形态/B12-B13 双环/F 族包络重映射 + ktest PI/FAST 契约面重验（锁纪元重验）| smoke-smp 全绿 + 主 smoke 零漂移 + test-host + build-release + ktest PI/FAST 面单核双核双绿 | 内核并行见证（诊断面：KERN_LOCK 争用计数下降——不进门） |

依赖序：a → b → c 线性（b 的队列形态是 c 缩窗口的前提；a 的 IPI 是 b 满速参与的唤醒前提）。回退缝：a 的 SGI 面 = 单核零扰动（天然缝）；b/c 之间 = 锁窗缩放点（c 不做即停 b 态 = E0.5 形态仍全绿）。

## §5 验收设计（G2 验收项 1 = 负载均衡对）

- **LB 金丝雀（状态谓词非竞速——4-5a 铁律 + 母案 §5「计数/旗标谓词」口径）**：burst spawn N 个 CPU-bound 用户任务（自旋若干秒 → 完成行经 console 打印）；断言 = ①双核 `el0_ticks` ≥ 阈值（SmpCpu 已有字段）②N 完成行齐 ③steal_cnt 非零（偷取活证——置失衡形态：任务全由 CPU0 侧入队，CPU1 只能靠偷取获得货）。`PAR ok` witness 门从 S-2b 翻转档**回收入场**（S-2 §5.2 翻转记录的回收条件 = IPI + 满速参与，本刀兑现）。
- **偷取测试**：并含于 LB 金丝雀③（steal 见证）；反例面 = 内核任务零偷取（pinned==0 全 CPU0——诊断计数可查）。
- **ktest PI/FAST 契约面重验**（母案 S-3 行点名）：PI 受捐链（4-5a 面）与 fastpath 恒等唤醒集（4-4 面）在锁纪元 E1 下单核 + 双核重跑——S-2 §3.4「单核不变量逐条保持」论证随锁重做。
- **回归门每刀**：主 smoke 八跑零漂移 + smoke-smp 双相 + EL2 + test-host + abi-check + build-release（新代码 ADRP 面）+ icount（锁/队列路径变即刷基线 diff 随源入册）。
- **首跑定标义务**（S-2b R3 同款三数）：boot 到齐 / 金丝雀周期 / rootfs 相到齐 wall；任一 > 帽 70% 预授权 ×1.5 升档一次；IPI + 偷取新争用面是否引入吞吐退幅 = 首跑要回答的问题。

## §6 风险表 + kill criteria

| # | 风险 | 缓解 |
|---|------|------|
| R1 | 锁序逆序死锁（新结构锁族嵌套调用图复杂化） | §3 全序立法 + 侦察逐路径核边；逆序边即重排/合并；三镜审查专项镜 |
| R2 | SGI 丢/风暴（pending 合并语义） | 唤醒戳幂等设计（收端无条件重验 pick）；发送点只在「入队到非当前核」判定后 |
| R3 | QEMU SGI 仿真保真度（TCG 双核） | §2.1 实证前置（S-0 同款）；实测记录入册后才写消费逻辑 |
| R4 | 偷取不变量破坏（任务双队/丢队） | 「出队后无归属窗 + 单点入队」构造性保证；integrity_check 扩双队列对账（A8 重设计面） |
| R5 | KERN_LOCK 收缩后对象系竞态漏面（E0「锁内免费」项逐个显性化） | S-2 §1 C 族逐项过表（C20 收缩版/C21 原子化/C22 随锁）；三镜审查专项镜 |
| R6 | 遥测双环读者聚合撕裂（watchdog/BENCHWAKE 跨环走查） | 聚合快照 API（锁序内 or 原子序读——侦察定）；首跑定标复核 |
| R7 | tickless 恢复后唤醒丢失（无中断源睡死——S-1 TCG wfi 教训同族） | 判据「SLEEPQ 最近到点者单点武装 CVAL」双态形态；gate 关单核维持现形态零漂移；首跑 PAR/LB 双金丝雀兜底 |
| R8 | release ADRP 面（新增 gic/sched 代码跨半引用） | build-release 挂每刀验收（K-2 纪律） |

**kill criteria（S-2 同款双线）**：S-3 单刀冻结线（连续两提交周期 >50% 主线工时未收官 → 冻结顺延）+ SMP 线整体冻结线（母案 §6：>20% 主线工时无里程碑产出 → 回单核冻结）。

## §7 侦察与审查待办（立稿初版 → 冻结实施序的门）

- **侦察面 A（最大）**：sched.rs 切换五原语 + tick + spawn/enqueue 全路径的「触达结构集合」逐点枚举——§3 舞蹈表重映射的实证输入。
- **侦察面 B**：gic.rs SGI 硬件面 QEMU 实测（§2.1 实证纪律）；GICD_SGIR 32-bit 写 + banked 使能/优先级面。
- **侦察面 C**：锁序调用图核对（§3 全序 vs 现实调用图）；分配器锁粒度（buddy/slab 合一 or 分立）；Topo 真化形态（capacity 表/拓扑来源——调度器设计 §3.5 原案对齐）。
- **侦察面 D**：验收面——LB 金丝雀置失衡形态、ktest PI/FAST 重验清单、遥测聚合读 API、可观测性设计遥测拆核原案对齐。
- **三镜对抗审查**：锁序死锁镜 / 语义等价镜（E0→E1 单核不变量逐条）/ 验收有效性镜（金丝雀谓词性）。审查后冻结拆步序再动代码。
- **文档义务**：调度器设计 §3.5「per-CPU runqueue day-one」行改写（母案 §1.4 偏差对账闭环）；母案执行结果区追记（S-3a 实证记录）；CLAUDE.md Gotchas 锁纪律三条增补面复核（S-2 文档义务项随本刀锁形态变化重裁）；进展追记 `docs/阶段沿革.md`。
