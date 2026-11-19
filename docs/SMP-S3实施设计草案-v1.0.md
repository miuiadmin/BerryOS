# SMP S-3（E1 per-CPU 纪元）实施设计草案

> 版本：**1.0**（v1.0）｜2026-11-06

> 状态：**立稿 + 侦察四面 + 三镜对抗审查收官 2026-11-01（同日三段）**——§8 侦察结果 + §9 三镜审查结果（45 findings 合折、两裁决落定：kill 链分段化 + LB per-child 归因）；**实施序 a→b→c 冻结**（§9.4 义务清单按步对表）；**S-3 整刀毕 2026-11-03（71f7bdd——a/b/c 全步收官，详见沿革与索引行层积）**。
> 母案：`docs/SMP设计草案-v1.0.md` §2.1 E1 行（语义权威）+ §4 S-3 行（范围权威）+ §3.3 SGI IPI 面（硬前置）+ §1.4 偏差对账（调度器设计 §3.5「per-CPU runqueue day-one」声明由本刀兑现改写）。
> 直接输入（S-2 §7 逐字移交）：`docs/SMP-S2实施设计草案-v1.0.md` §1 共享面盘点 54 项处置档总表（= 拆锁地图）+ §3.3 切换原语舞蹈表（= 取放点地图）+ §7「留给 S-3」清单。
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
| 队列 | A4 RUNQUEUE | KERN_LOCK 锁内全局单队（sched.rs:71 + `Topo::single()` :73） | **per-CPU 每核队列 + Topo 真化**（调度器设计 §3.5 兑现）；形态已定（§8.C）：sched 内 `PER_CPU_RQ:[{lock,rq};2]` const 初始化 + `Topo.capacity:[u32;2]` 由 `sched::init` 从 hal `cpu_capacity()` const 表填 + `rq()` 本核分诊；放置最小版 = 内核任务钉 CPU0、用户任务入本核、LB 交偷取 | 每核队列锁 |
| 队列 | A5 SLEEPQ | KERN_LOCK 锁内 | **独立自旋锁**（睡眠/唤醒/超时/expire 全路径改取睡锁）；unlink「锁内」契约表述照旧 | SLEEPQ 锁 |
| 队列 | A6 REAPQ | push/drain 均 KERN_LOCK 锁内 | **独立自旋锁**；drain 摸分配器 → 分配器锁序见 §3 嵌套序 | REAPQ 锁 |
| 调度 | A3 CURRENT | per-CPU 访问器（S-2b 过渡形态） | **真体化**：sched 内部对全局 static 的回落路径收编、注释对齐（行为不变，形态收敛） | per-CPU（免锁维持） |
| 调度 | A7 IDLE | IDLE_TASK 单份（CPU0）+ 副核 park ctx（S-2b §2.3） | **per-核 idle 环统一**：pick 本地 → steal 探测 → 空则 tickless wfi（§2.4）；CPU0 idle_loop 与副核 secondary_main 环合流为同一 per-core idle 例程；IDLE_TASK 符号退役或降格为 CPU0 idle ctx（侦察定形态——「不立第二个 IDLE_TASK」口径延续，深化非推翻） | per-CPU |
| 调度 | A8/A9 integrity/expire | tick 锁窗内 | 随 tick 面拆：本核队列锁内执行（integrity 的跨队走查面 = 侦察重设计点——只查本核 or 双锁聚合快照） | 随队列锁 |
| 调度 | A10 kill_all | KERN_LOCK 全程 | **分段化 + 五骑缝件**（§9.2 裁决一——三镜审查落定，否决大锁豁免/集齐四锁案：分配器×O(n²) 停摆 + 豁免位无执法 + F-L8 UAF 两案同病；骑缝件 = 介释放/保 F6 序/双杀 CAS/双队扫描/横核 Running 协议） | 收缩版 KERN_LOCK + 段间重验 |
| 遥测 | B12/B13 双环 | 单环 KERN_LOCK 串行（S-2 撤销记录维持正确） | **per-CPU 双环**（母案 E1 原位——S-2 §7 留面；可观测性设计原案拆核口径随侦察对齐）；读者（watchdog/BENCHWAKE）改聚合快照 API | per-CPU 写 + 聚合读 |
| 对象系 | C20 表/对象 | KERN_LOCK 锁内免费 | **KERN_LOCK 收缩为「对象系/表锁」**（更名或收窄——S-2 §7 退役路径第一步）；~~svc_enter/svc_exit 取放点不动~~ **修正（§9 F-L3）**：约 11 个 syscall 号触 SQ/RQ/REAPQ——入口全量取表锁 = 每号一条反序边，**dispatch 须分号类别取放**（S-3c 义务②） | 收缩版 KERN_LOCK |
| 对象系 | C21 Event.signals | 非 atomic + 纪律注释 | **AtomicU64 化**（S-2 §7 备选转正——拆锁后「跨核写者全收口」论证失效，原子化一劳永逸；成本极低） | 原子 |
| 对象系 | C22 ObjRef | 锁内免费 | 随收缩版 KERN_LOCK 维持 | 收缩版 |
| 分配 | D23 buddy/slab | KERN_LOCK 锁内（E0 无锁铁则） | **独立自旋锁、buddy+slab 合一**（§8.C 定案：slab 补页天然嵌套 buddy，分立须另立内序、合一代价可忽略；per-CPU slab 留后续）；`frames()` 类型面真修（D25）随本刀收账 | 分配器锁（合一） |
| 分配 | D25 frames() 别名 UB | 锁纪律兜底 | **类型面真修**：`frames()->&'static mut`（mm.rs:152）改 `frames_*` 包装，并发面 8 调用者（heap×3/vmo.rs:296/syscall×4）随锁改收账 | — |
| 分配 | （补行）ASID | 无锁（BKL 遮掩） | **独立小自旋锁整段包**（回卷计数器非位图，CAS 不适配 + flush_all_asid 副作用；全序补位分配器之下——立稿缺口由侦察 C 补） | ASID 锁 |
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
| 1 | `SGI_RESCHEDULE` | 跨核唤醒戳：**入队/唤醒路由到他核即无条件补发（幂等）——禁读对核 park 态条件发送**（§9 F-E6/F-A3：racy 读 = 丢唤醒成因；park 态判定根本不进发送条件）；收端 handler = 无条件重验 pick（pending 合并语义下丢/重均无害，pick 兜底） | S-3a |
| 2 | `SGI_TLB_DONE` | shootdown 完成回执 | **号位预留不消费**（S-4） |

SGI 0 保留不用（0 号是复位默认值域，观测面干净性）。

### 2.3 exception_irq 分流表加 SGI 段

trap.rs INTID 分流（:400 起）加 `0..=15 => SGI 段`：SGI_RESCHEDULE → 收端本核 pick 重验（不取大锁——只置 per-CPU `resched_pending` 原子位 + 尝试触发调度点；具体形态侦察定：直接 pick or 延迟到安全点）。未知 SGI 仍走 `irq? id=` loud 面。

### 2.4 park/release 改形 + tickless idle 恢复（E0 两大代价的根治）

- **唤醒延迟 ≤1 slice（S-2 R2）根治**：跨核唤醒路径（wake_matching 命中他核任务 / 入队路由到他核）**无条件**补发 SGI_RESCHEDULE（§9 修正：发送条件 = 「目标核非当前核」这一静态判定，不读对核 park 态）——被唤任务立刻上核，不再等目标核下次 tick。
- **tickless idle 恢复（S-2 §2.3 主核 idle 改形的回卷）**：per-核 idle 环空判（本队列空 + 无 steal 货 + **对核无 pinned!=0 货**（§9 F-A3 修②——A/B 忙环使「非空」恒真，防 parked-peer 睡穿偷取窗）+ SLEEPQ 无到点者——侦察细化判据）→ **mask CNTV + wfi**（真 tickless park）；唤醒源 = SGI_RESCHEDULE（无条件发——入队侧不赌 park 态）+ RX IRQ（CPU0，不可依赖只作兼带）+ 到点任务路径（SLEEPQ 最近 deadline → **归属核**该核 CVAL 单点武装——归属规则随 S-3a 成文 F-E5；不变量「SLEEPQ 插入只发生在运行（tick 武装）核上」入册 = 迟醒界 ≤1 slice 的真实来源）。gate 关（单核）维持现形态零漂移（S-2 先例口径；等价镜实证：分支门卫 par_enabled/cpu_id!=0/无发送者 SGI/mod4 纯 CPU1 面——单核结构性不走新分支）。
- **park 限频 mod4 退役**：副核满速参与（S-2 雷 2 治理的限频占空比在 IPI + 满速参与后失去存在理由——首跑盯 console 饱和面是否复发，复发则限频保留为诊断档）。

**S-3a 实施定案（2026-11-01 落码 + 义务①除名后口径；Step C 成文）**：③归属规则 = SLEEPQ 插入只发生在运行核（block_current 调用核）——插入核**单点**武装自身 CVAL 至该 deadline（`nearest_deadline()` 只读 API + `arm_at`，park 环逐轮重取）；深 park 核的 CNTV 恒不需要武装（不变量自洽：插入者必在运行核）。④迟醒界 ≤1 slice 的真实来源 = 运行核 slice 节奏（F-E5）——非 park 核自武装。义务①（enqueue 尾 SGI）除名后，跨核唤醒戳归 S-3b per-CPU 队列路由化（§9.4 S-3b 义务⑧）；S-3a 副核自续 = **截止驱动醒**（实证 27 笔/150s，判词 → §9.5 末）。

## §3 拆锁时序与锁序（S-2 §3.3 舞蹈表移交面）

**staging 裁决（语义等价小步的落地形态）**：

1. **S-3b 先拆结构、后缩窗口**：per-CPU 队列/睡锁/回收锁全部落地时 **SVC 入口仍取全量 KERN_LOCK**——结构已分、锁窗未缩：双核仍是内核串行，但放置/偷取/每核 tick 逻辑先行可证（语义等价 = 单核行为不变 + 双核行为等价于 E0 可能交错）。
2. **S-3c 再缩窗口**：svc_enter/svc_exit 的 KERN_LOCK 收缩为「对象系/表锁」，调度/睡眠/回收/分配器路径改取各自结构锁——此步后才真正「两核并行进内核不同区域」。

**锁序立法（死锁面预防——侦察 C 修正版 2026-11-01）**：

- 全序（**改序**——立稿拟序「队列>SLEEPQ>REAPQ>分配器>表>打印」被现实调用图证伪两处：①持表摸分配器恒常（Drop 链 E1-E10 十条边）→ 分配器须降至表之下；②SLEEPQ 与队列的现实操作序全族 SLEEPQ 先（wake/block/expire/kill E15-E18）→ SLEEPQ 须升至队列之上。反向边零命中；**等价镜补列漏边**（F-E1：spawn 的 enqueue 在表窗内 = 表→队列边，与 wake 族成 ABBA 对——E 表补列，修法 = enqueue 出表窗）；**三镜裁决：七层相对序无环、方向确认可冻结（§9.0）**）：**SLEEPQ 锁 > 队列锁 > REAPQ 锁 > 对象系/表锁（收缩版 KERN_LOCK）> 分配器锁（buddy+slab 合一）> ASID 锁 > 打印锁**——证据面 = §8.C 逆序边表 E1-E20 + §9 F-E1 补边；残余重排面三类定案：wake/PI 分段取放（E11-E14——fastpath 定案 = find+push+fast_wake 单 SQ 窗 F-L5）、kill 链分段化（§9.2 裁决一）、wait 表锁放点前移（E13）；**wait/wake 原子性锚 = SQ 锁窗**（F-L4：E0 锚 IRQ 屏蔽窗在分段后敞开 = 丢唤醒永睡——查位+入队同窗/写位+扫描同窗）；**不可重入立法**（F-L2：KERN_LOCK 朴素 TTAS 同核自锁——SQ/RQ/REAPQ/表四面 `_locked` 内层分层，自锁面三处 wake_matching→recompute/kill 钩子链/rx_irq_notify）。
- 偷取锁序特则：**只持远端**（偷取 = 放本地锁 → 取远端锁 → 远端 dequeue → 放 → 本核无锁持有该 Task——任务此刻无队列归属，入本核队列时再取本地锁；**双持一律核号升序（0→1）**（§9 F-L1 修正：原「先远端后本地×核号升序」两句互斥 = CPU0 取 1→0 / CPU1 取 0→1 ABBA——偷取「只持远端」不构成双持，单独成条）；「双队列同时出现」不变量由「出队后无归属窗 + 单点入队 + **偷取双过滤 `state==Ready && pinned!=0`**（F-E2——PickFilter 他核 Running 排除臂的偷取面化身，漏写 = 任务双核同时执行）」构造性保证。
- 切换原语舞蹈表（S-2 §3.3 五原语）逐行重映射：每原语的「切前放什么锁/切回取什么锁」从单锁位演化为锁集合枚举——**侦察面 A（最大细读面）**：yield_now/block_current/try_preempt/switch_away_forever_with/on_irq_tick 各自触达结构全列。
- IRQ 上下文纪律：SGI handler / tick / RX 的取锁面按「短窗 + 永不睡眠 + 永不嵌套打印锁之外的锁」复核（S-2 §2.2 对表结论重裁——母案「IRQ 侧收 per-CPU-only」偏好在 resched_pending 位上部分成立）。

**work stealing 初版**：偷取触发 = 本核 pick None 且对核队列非空（idle 环探测点）；粒度 = 单任务；方向 = 双向（CPU0↔CPU1 用户任务；内核任务永不偷——pinned==0 全钉 CPU0 维持）；per-CPU `steal_cnt` 诊断计数（验收面用）。**放置路由 v1（唯一定稿 §9 F-E3/F-E4/F-A1：「入队侧空核偏好」措辞废除）= 本核优先 + 深度失衡阈值 K=2（常规流量；K=2 下 4-burst 第 3 个任务即直接路由 CPU1——直接路由与偷取同格合格，金丝雀断言不赌路由形态，见 §9.2 裁决二）**；主动迁移不做（挂 v2）。

## §4 拆步表（每步独立可验收；缝 = 可回退面）

| 步 | 内容 | 验收 | 标记 |
|----|------|------|------|
| **S-3a SGI IPI 底座 + tickless 恢复** | §2 四件：SGIR 原语 + 实证记录 + SGI_RESCHEDULE 分流段 + 跨核唤醒补发 + per-核 idle 环统一（tickless park 恢复）+ park mod4 退役 | `just smoke`（单核零漂移——单核无发送者，SGI 面结构性无扰动）+ `just smoke-smp` 双相全绿 + `just test-host` + `just build-release` + `run-el2` | smoke-smp 照旧双相（提速不进门）；QEMU SGI 实证记录入母案执行结果区 |
| **S-3b per-CPU 队列 + 偷取（锁窗未缩）** | A4/A7/A8/A9 + 放置路由 + pick_filter 退役改形 + work stealing 初版 + LB 金丝雀（§5/§9.2）；SVC 入口仍全量锁；spawn 四写点前移 + enqueue 出表窗（§9 F-E1） | smoke-smp 全绿（rootfs 相 = 单核恒等 + **功能等价** + 无饿死 + 内核锁序串行——§9 F-E10 表述降格，调度序面按设计扩展交错集非严格 E0 等价）+ 主 smoke 零漂移 + test-host + icount 刷新（队列路径指令数变——S-2a 先例） | **`LB ok`（G2 验收项 1，§5/§9.2 per-child 归因）** |
| **S-3c 结构锁族 + KERN_LOCK 收缩 + 遥测双环** | A5/A6/D23/D25/C21/C20 收缩/E25 tick 最终形态/B12-B13 双环/F 族包络重映射 + ktest PI/FAST 契约面重验（锁纪元重验）+ kill 分段化五骑缝件（§9.2）| smoke-smp 全绿 + 主 smoke 零漂移 + test-host + build-release + ktest PI/FAST 面单核双核双绿（**ktest 串行本质——并发面覆盖归 LB/soak，如实记载 §9 F-A13**） | 内核并行见证（诊断面：KERN_LOCK 争用计数下降——不进门） |

依赖序：a → b → c 线性（b 的队列形态是 c 缩窗口的前提；a 的 IPI 是 b 满速参与的唤醒前提）。回退缝：a 的 SGI 面 = 单核零扰动（天然缝）；b/c 之间 = 锁窗缩放点（c 不做即停 b 态 = E0.5 形态仍全绿）。

## §5 验收设计（G2 验收项 1 = 负载均衡对）

- **LB 金丝雀（状态谓词非竞速——4-5a 铁律 + 母案 §5「计数/旗标谓词」口径；**参数 §8.D + 断言语义 §9.2 裁决二**）**：fw_cfg 门 `opt/berry/lb`（双门 gate + `LB gate on` 自证）；lb_master 内核任务（**私有 handle 表**——A45 铁律）svc 驱动 burst spawn N=4 个 Process-backed CPU-bound 用户任务（内嵌 asm 平像 `mrs CNTVCT_EL0` 自旋 D=8s → `svc 0x33`）；完成收割 = `wait_many` 挂 N 个 Process 的 `SIGNAL_TERMINATED`（C-4b 收集器同款，零 console 依赖）；断言单原子行 **`LB ok c0=Δ c1=Δ st1= n=4`**——**Δ/lb 计数按 per-child 归因**（`LB_CHILDREN` 4 槽静态数组，tick/偷取路径按 `cur ∈ LB_CHILDREN` 记——直接路由与偷取同格合格，矩阵噪声零污染；st1 全局计数降诊断）；smoke-smp.sh 第三相 gated bare（DEADLINE_LB=240，**锚点在标记轮询后起算**）+ 相 1/2 反向断言 `LB gate on` 不得出现 + R3 第四数；**相 3 语义 = `LB ok` 严格必需、`LB absent` 一票 FAIL**（§9 F-A4——禁克隆 PAR absent 放行语义）。`PAR ok` witness 门从 S-2b 翻转档**回收入场**（回收条件 = IPI + 满速参与；S-3b 同刀把 PAR 断言从 d1>0 档翻回 witness 档——§9 F-E12：路由后 PAR ≡ 偷取见证，保留为诊断非验收门）。
- **偷取测试**：并含于 LB 金丝雀③（steal 见证）；反例面 = 内核任务零偷取（pinned==0 全 CPU0——诊断计数可查）。
- **ktest PI/FAST 契约面重验**（母案 S-3 行点名；**清单定案 §8.D = 17 断言面 + 6 横切锁纪律项**，PI-K J0-J7 / FAST-K L0-L6 / boot `I PI ok` / `F FAST ok` 四锚）：PI 受捐链（4-5a 面）与 fastpath 恒等唤醒集（4-4 面）在锁纪元 E1 下单核 + 双核重跑——S-2 §3.4「单核不变量逐条保持」论证随锁重做；最危险三条首查 = send fastpath 对象锁→SLEEPQ 扫描逆序边（R1 同源）、recompute/for_each_sleeper 重入 `_locked` 分层、pi_probe klock 包络取序。
- **遥测聚合验收**：聚合快照 API 签名原样（total/check/last/span/wake_hist 五面）⇒ watchdog/tele_dump/ktest 三读者断言零改动即绿 = 双环切换零漂移的验收形态本身。
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
| R6 | 遥测双环读者聚合撕裂（watchdog/BENCHWAKE 跨环走查） | **定案（§8.D）**：每环 seqlock + 聚合快照 API 签名原样（total=Σ/逐核单调/vct 归并——CNTVCT 全局单源为跨核归并物理根基）⇒ 三读者零改动；不占锁序（原子序读）；首跑定标复核 |
| R7 | tickless 恢复后唤醒丢失（无中断源睡死——S-1 TCG wfi 教训同族） | 判据「SLEEPQ 最近到点者单点武装 CVAL」双态形态；gate 关单核维持现形态零漂移；首跑 PAR/LB 双金丝雀兜底 |
| R8 | release ADRP 面（新增 gic/sched 代码跨半引用） | build-release 挂每刀验收（K-2 纪律） |

**kill criteria（S-2 同款双线）**：S-3 单刀冻结线（连续两提交周期 >50% 主线工时未收官 → 冻结顺延）+ SMP 线整体冻结线（母案 §6：>20% 主线工时无里程碑产出 → 回单核冻结）。

## §7 侦察与审查待办（立稿初版 → 冻结实施序的门）

- ~~侦察面 A~~ ✅ **2026-11-01 完成**（§8.A——11 路径触达结构总表 + F1-F10 意外发现 + 锁序重映射 7 条）。
- ~~侦察面 B~~ ✅ **2026-11-01 完成**（§8.B——GICv2 dumpdtb 实证 + gic.rs:72 全禁坑 + probe 三步实测序；probe ①②③ 均命中 2026-11-01——实测记录入母案执行结果区 S-3a 段；③ 兼揪出 boot DAIF 屏蔽窗双洞（EL1 直入继承 reset 解蔽 + ktest trampoline 解蔽传染，判例 A105，随刀修复）。
- ~~侦察面 C~~ ✅ **2026-11-01 完成**（§8.C——E1-E20 逆序边表证伪立稿拟序两处（§3 已改序）+ 分配器合一锁定案 + D25 方案 + ASID 补位 + Topo 五点最小形态）。
- ~~侦察面 D~~ ✅ **2026-11-01 完成**（§8.D——LB 金丝雀照抄级方案 + ktest 重验 17+6 清单 + 遥测聚合 seqlock 定案）。
- ~~三镜对抗审查~~ ✅ **2026-11-01 完成**（§9——45 findings 合折：全序无环方向确认 + KERN_LOCK 不可重入前提纠错 + 两裁决落定（kill 链分段化五骑缝件 / LB per-child 归因解耦）+ §9.4 冻结后实施义务清单按步归属；正文随审修订 §9.3 所列处）。**实施序 a→b→c 冻结**。
- **文档义务**：调度器设计 §3.5「per-CPU runqueue day-one」行改写（母案 §1.4 偏差对账闭环）；母案执行结果区追记（S-3a 实证记录）；CLAUDE.md Gotchas 锁纪律三条增补面复核（S-2 文档义务项随本刀锁形态变化重裁）；进展追记 `docs/阶段沿革.md`。

## §8 侦察结果（2026-11-01 三路并行侦察 + B 面内联；行号锚 = main@c02258c 实读）

### 8.A 侦察面 A：切换五原语 + tick + spawn/enqueue 触达结构枚举（✅ 完成）

缩写：RUNQ=全局 runqueue 环 / SQ=SLEEPQ / REAPQ / TEL=遥测三环（static mut 非原子）/ CNT=已原子计数器 / HEAP=内核堆 / HT=对象·handle 表 / PCPU=SmpCpu 其余字段 / KL·PL=KERN_LOCK·PRINT_LOCK / CUR·LVCT=已 per-CPU 主位（回落 static）。

| # | 路径 | 触达结构（锚点） | 串行面 | 切换点 | HEAP |
|---|------|------------------|--------|--------|------|
| 1 | yield_now（sched.rs:415-472） | RUNQ(pick 消费游标)+CUR+MMU+CNTV+TEL+KL+REAPQ(尾 drain) | disable_irq + KL 舞蹈（位分诊） | :437/:452 | drain 释放 |
| 2 | block_current（:1000-1071） | CUR+**SQ(insert_sorted)**+RUNQ(unlink/pick)+TEL+**PI(SQ 全图×HT 读)**+KL | 调用方契约（SVC 锁窗/klock_enter） | :1063/:1038 | 零 |
| 3 | try_preempt（:1202-1245） | RUNQ(只读探峰→pick)+CUR+MMU+CNTV+TEL+KL | SVC 锁窗/内核任务屏蔽窗 | :1237（唯一「SVC 中途」抢占点） | 零 |
| 4 | switch_away_forever_with（:533-587） | CUR+**HT(EXIT_HOOK：wake→SQ/RUNQ/TEL/PI + ht.drain→HEAP free)**+RUNQ(dequeue/pick)+REAPQ(push)+MMU+CNTV+TEL+UART | SVC 锁窗/fault svc_enter(trap.rs:333) | :584/:561（`-> !` 放不取） | 钩子 drain/release 释放——**全路径最宽** |
| 5 | on_irq_tick（:1456-1614） | PCPU(ticks_pre 锁外)+KL+SQ(expire)+RUNQ(tick/pick/peek)+CUR+TEL+**PL×2 嵌套**(:1510/:1592)+CNTV+integrity | IRQ 上下文 + klock_acquire_raw | :1536/:1559/:1607（切走不取回） | 零 |
| 6 | spawn 家族 | HEAP×2(16KiB kstack+TCB)+RUNQ(link_back) | boot DAIF 屏蔽（单写者）；**process_start 面 = SVC 锁窗内 alloc+enqueue** | 无 | alloc×2 |
| 7 | wake_matching/fast_wake/find_single_waiter（:1081-1180） | SQ(摘)+RUNQ(入)+TEL(WAKE 全家)+PI | 调用方 KL 窗（SVC/EXIT_HOOK/**rx_irq_notify IRQ 上下文**） | 无（fastpath 后 try_preempt） | 零 |
| 8 | expire_timeouts（:1253-1269） | SQ(摘头)+RUNQ(入)+TEL+PI | tick KL 锁窗内 | 无 | 零 |
| 9 | REAPQ push/drain（:609-646） | REAPQ statics+HEAP(free)+UART(溢出裸打) | push=退出链锁窗（**可来自 CPU1**）；drain=CPU0 idle/yield | 无 | free |
| 10 | context_switch 邻域（arch task.rs:222-275） | 纯 asm 零共享触达；trampoline 内 `daifclr #2` 解蔽 | 放锁后零共享（执法点 :1600-1607） | 即本体 | 零 |
| 11 | idle_loop(CPU0)/secondary_main（sched.rs:1397-1439/main.rs:3050-3080） | idle 参与态 = KL 窗内 drain+空判（**结果未消费**）后恒 arm+wfi；副核环自身零共享（手术全在 tick 锁窗内） | 见左 | （经 tick 派发切走） | drain 释放 |

**意外发现（F1–F10，清单之外）**：

- **F1 spawn 半初始化窗口**：`try_spawn_user` 先 `RR.enqueue`（sched.rs:304）返回指针，process_start 才写 pinned/thread_key（syscall.rs:925/1044/1052）——任务已入全局环但 pinned==0。E0 靠 KL 窗内无并发 pick 安全；S-3 拆锁后 CPU0 全量视图可在窗口内 pick 到半初始化任务（副核 user_only 过滤恰好看 pinned!=0 反而滤掉）。**修法方向 = 字段写前移到 enqueue 之前**（S-3b 必改件）。
- **F2 无 PL 的打印段**：user_fault_exit 三段 uart_puts（sched.rs:516-518）+ REAPQ 溢出打印（:621）——同链 dump_el0 走了 PL（trap.rs:363），这两处裸奔；S-3 拆 KL 后撕行（S-3c 补 PL 面）。
- **F3 第二个 IRQ 上下文锁入口**：rx_irq_notify（main.rs:1858-1868）在 PL011 RX 臂（EOIR 后）裸取裸放 KL，内含 Event.signals 写 + wake_matching 全套面——独立于 tick 面，**S-3 重映射最易漏**。
- **F4 遥测环两核共写单 RING**：CPU1 的切换同样写 RING/WAKE_RING/WAKE_HIST（static mut 非原子）——E0 靠「切换都在 KL 窗内」串行；telemetry.rs:18 已预留 per-CPU 环注释。**B12/B13 双环从「E1 原位」升格为 S-3c 硬前置**（拆 KL 即竞态，锁窗收缩步内必须先落）。
- **F5 idle_loop 参与态空判结果未消费**（sched.rs:1406 `let _empty`）：恒 arm+wfi 形态遗留；S-3 per-核 idle 环统一时这段锁窗整体重审（= §2.4 tickless 恢复的落点）。
- **F6 EXIT_HOOK 在 dequeue 之前跑**（:536-541）：钩子内唤醒的 join 等待者先入环、随后 cur 才 dequeue+pick——单窗 RUNQ 手术交错两次；kill_all 反序（先摘队后钩子，防 unlink 不幂等扬队）。**两序并存有意为之，S-3 锁化保持各自理由**。
- **F7 PAR/witness 跨核 volatile 读依赖 BKL**：par_sample 读 SMP_CPU1.last_task（smp.rs:317）等明注「BKL 锁窗串行无撕裂」——S-3 拆锁后这些诊断读需原子化或接受撕裂（S-3b witness 面处置）。
- **F8 peek/pick 双视图 = 单游标临时补丁**：副核全走 peek_next_filtered 不消费游标（class.rs:203-223 头注实录 S-2b 首跑饿死）——per-CPU 队列化后 peek 整体退役；但 PickFilter 他核 Running 排除臂（:164-174）是防二派发现行执法面，**迁移语义时要带走或被 per-CPU 队替代**。
- **F9 ktest「无锁直通」窗**：KLOCK_OPS None 时 klock_* 全直通（sched.rs:126）、klock_flag_current 对 null CURRENT 跳过（:202-207）——**S-3 新锁必须保留安装前零开销直通**（ktest 在 sched::init 前跑的面不破坏）。
- **F10 REAPQ 消费权钉 CPU0 双点**（idle 顶 + yield 尾）：副核防御 yield 面不 drain——S-3 per-CPU 化后 REAPQ 消费权归属重定义（独立锁 or per-CPU REAPQ + 跨核转移，侦察 C 定形）。

**锁序重映射优先级**（A 面结论，供 §3 舞蹈表重映射 + 三镜锁序镜输入）：

1. 退出路径（#4）锁集合最大：单窗 RUNQ+SQ+REAPQ+HT+HEAP+TEL+UART；EXIT_HOOK 内嵌 wake（SQ→RUNQ→PI）+ drain（HT→HEAP）。
2. tick（#5）：expire(SQ)→enqueue(RUNQ)→pick(RUNQ) 即「SQ 锁→RUNQ 锁」定序点；KL→PL 嵌套两处已立单向。
3. block/wake 对（#2/#7/#8）：SQ 与 RUNQ 反向操作同锁集，必须同序——建议 **SQ → RUNQ → PI(HT 只读) → HEAP**。
4. fastpath 同窗契约：find_single_waiter 裸指针返回值「同窗有效」映射为「同锁窗」。
5. 切换五点「放锁后零共享触达」已是执法点（:1600-1607/:577-582），S-3 各切换位逐一保持。
6. HEAP 锁最内层：一切结构锁 → HEAP 单向；drain/release/drop 三处复合点（§12 分配器触达汇总）。
7. IRQ 双入口（tick + rx_irq_notify）都要覆盖——漏 rx 面即漏整条 RX 唤醒链。

### 8.B 侦察面 B：SGI 硬件面预研 + probe 方案（✅ 完成——probe ①②③ 均命中 2026-11-01，实测入母案执行结果区；③ 五跑双形态 + Self 锁存自证 + boot DAIF 双洞修复收官）

- **机型实证（2026-11-01 dumpdtb）**：QEMU virt 默认 GICv2——DTB `intc@8000000` compatible = `arm,cortex-a15-gic`（+ `arm,gic-v2m-frame`）；`-machine virt,help` 列 `gic-version={2,3,4,host,max}`，`run-smp`/`smoke-smp` 均未设 → 默认 v2。**GICD_SGIR @0xF00 原语成立**。
- **§2.1 未预见的坑**：`gic.rs:72` init 时 `ICENABLER0=!0` 把 ID 0..31 **全段禁了（含 SGI 0..15）**——SGI_RESCHEDULE(1) 起用必须**每核 banked 再使能**（CPU0 侧 init 后补 + 副核 secondary_main 起手补，`enable_cntv_this_core` :167 同款 banked 纪律：INTID<32 的 ISENABLER0/IPRIORITYR per-CPU banked，写落本核）。GICD_CTLR=1 已开，SGI 路由无需再动 distributor 全局面。
- **probe 通道零代码现成**：trap.rs 分流表无 SGI 臂，未接臂时 SGI 到端天然落 `irq? id=N` loud 面（UNKNOWN_SEEN 首 4 loud）——S-3a 首步实测序：①CPU0 经 SGIR（TargetListFilter=0b00 List=0b10 指 CPU1）发 SGIINTID=1 → CPU1 观测 `irq? id=1`；②补双核 banked 使能 + 分流臂后打 `SGI ok` 类标记；③同核自发（filter=0b10 仅己）与 Others（0b01）形态各记录一遍 + pending 合并语义观测。**实测记录入母案执行结果区后才写消费逻辑**（S-0 同款实证纪律）。
- TLB shootdown 维持同步 IS 广播（号 2 预留不消费）——母案口径照旧。

### 8.C 侦察面 C：锁序逆序边 + 分配器粒度 + Topo 真化形态（✅ 完成）

**逆序边表（对立稿拟序「队列>SLEEPQ>REAPQ>分配器>表>打印」核边，E 编号承侦察报告）**：

| 族 | 边 | 锚点 | 判定 |
|----|----|------|------|
| 表→分配器（拟序立反；现实「持表摸分配器」恒常，反向零命中） | E1 close 的 Drop 链（Vmo/ChannelEnd/Process/Thread Drop→frames free/Box drop） | syscall.rs:260→handle.rs:312→vmo.rs:296/channel.rs:155/process.rs:126/168 | 逆序 |
| | E2 表内分配（try_box + Vec 预留） | handle.rs:255/:519/:534 | 逆序 |
| | E3/E4 退出钩子 drain+release | syscall.rs:1109/:1263（drain 内 drop handle.rs:408） | 逆序 |
| | E5 kill 链钩子 | sched.rs:712/:744 | 逆序 |
| | E6 spawn 在表窗（Task Box+16KiB 栈） | sched.rs:224-307 经 syscall.rs:876/:948 | 逆序 |
| | E7/E8 对象创建/commit/map/process_create | syscall.rs:408/:452/:663/:798 | 逆序 |
| | E9 wait 尾 release | syscall.rs:1889/:2019 | 逆序（可移出表窗） |
| | E10 alloc_asid | aspace.rs:19 | 逆序——**ASID 锁未入全序（立稿缺口）** |
| 表→SLEEPQ/队列（需分段取放；waker 侧无原子性需求、sleeper 重验兜底在册 task.rs:54-56） | E11 send fast/slow | syscall.rs:1518/:1534/:1536/:1545 | 逆序 |
| | E12 recv/signal/pi::recompute（表读+SLEEPQ 走查） | syscall.rs:1663/:1784/pi.rs:57-88 | 逆序 |
| | E13 wait 的 block_current（rq+SLEEPQ） | syscall.rs:1885/:2017 | 逆序（表锁放点须前移） |
| | E14 RX IRQ signal+wake 同窗 | main.rs:1858-1870 | 逆序（**C21 原子化后 signal 免表锁即解**） |
| SLEEPQ↔队列（拟序又立反；现实操作序全族 SLEEPQ 先，反向零命中） | E15-E18 wake_one_inner/expire/block_current/kill_all | sched.rs:1081/:1253/:1000/:683 | 按拟序全部需重排——改序后自然合法 |
| | E19/E20 kill 链 A10「维持大锁窗」= 持表跨四层 | sched.rs:683-757 | **与全序冲突，需裁定**（倾向分段化——三镜审查裁决） |

**drop 链嵌套结论**：Task 无 Drop impl——REAPQ 窗内 drop 只触分配器不级联对象系（「REAPQ>分配器」序下合法 ✓）；真正的隐蔽逆序在**表锁窗内的 Drop 链**（E1/E3/E4/E5）——改序（分配器降至表下）后一族边零重排全合法化。

**分配器粒度结论**：buddy（mm/buddy.rs）+ slab（kernel/heap.rs）两套结构、**合一锁**（slab 补页 heap.rs:61/:76/:94 天然嵌套 buddy——分两把须另立内序；buddy 是慢路径，合一代价可忽略；per-CPU slab 留后续刀）。**D25 收账方案**：`frames() -> &'static mut`（mm.rs:152，FRAMES :54）别名 UB，并发面调用者 8 处（heap×3/vmo.rs:296/syscall×4）——随分配器锁改 `frames_*` 包装收账。**ASID 面**：非位图，是**回卷计数器**（asid.rs:35；aspace.rs:16 static Option）——独立小自旋锁整段包（CAS 不适配回卷 + flush_all_asid 副作用），全序补位在分配器之下。

**Topo 真化最小形态**：①`Topo` +`capacity:[u32; MAX_CPUS]`（MAX_CPUS=2 起步，runqueue.rs:211 现无 capacity）；②`TOPO` 改 static mut、`sched::init` 从 hal 填（nr_cpus = 1+smp_active()；capacity 走 hal 新增 const 表 `cpu_capacity()`，QEMU 对称 = 全 1023——「arch 填表」原则保持）；③per-CPU 队列 = sched 内 `static mut PER_CPU_RQ:[{lock:AtomicBool, rq:Runqueue}; 2]`（const 可初始化——**ktest 先于 init 契约不破**，F9 直通面），`rq()` 改本核分诊；④**SmpCpu 不动**（crate 依赖方向 sched→hal，禁 Runqueue 入 arch 块）；⑤放置最小版：内核任务钉 CPU0、用户任务入本核，LB 交偷取（主动迁移不做照旧）。

### 8.D 侦察面 D：验收面细化（✅ 完成）

**LB 金丝雀实施方案（S-3b `LB ok`，照抄级粒度）**：

- **门控**：fw_cfg string 键 `opt/berry/lb`（soak 先例 main.rs:1355-1391 读法）；gate 块克隆 PAR gate（main.rs:1411-1437）插其后，双门 `find_file && smp_active()` + `LB gate on` 自证行 ⇒ 单核主 smoke 零漂移。
- **置失衡形态（~~侦察定案~~ §9 修订 F-A1/F-A4）**：~~放置路由 v1 = 本核优先 + 队列深度失衡阈值 K=2（非 idle 位）——burst 从 CPU0 侧入队结构性全落 CPU0 队，CPU1 得货唯一路径 = 偷取，无需 force 旋钮~~ **K=2 与「4-burst 全落 CPU0」算术互斥（第 3 个任务即直接路由 CPU1）——路由保持 K=2（常规均衡语义），金丝雀断言改 per-child 归因与路由形态解耦（§9.2 裁决二），「置失衡」叙事废除**。偷取资格判据维持 `pinned != 0` ⇒ **LB 子任务必须 Process-backed**（svc 0x11/0x15/0x31/0x32 由 lb_master 内核任务驱动——EL1 同权 A14 + pi_client svc 先例；**链上补 vmar_map** §9 F-A16）。
- **lb_master 私有 handle 表（§9 F-A6 补）**：A45 铁律——不挂 tag 表则 4 个 Process handle 默认落共享 BOOTSTRAP_HT（PI 错捐面 A34/A45）；表形状 = 4 Process + VMO + 派生位 wait_many；子 VMO 收割撞 -EBUSY（A18 销毁性 close）降级不 trap（A74）。
- **子任务**：内嵌 asm 平像（~20 指令）：`mrs CNTVCT_EL0` 自旋 8s（D_TICKS=500e6 @62.5MHz）→ `svc 0x33` 退出；纯 EL0 零 syscall（不扰 icount 剖面）。完成收割 = lb_master `wait_many` 挂 N 个 Process 的 `SIGNAL_TERMINATED`(1<<2，abi_gen.rs:96)——C-4b 收集器同款，**子任务零 console 依赖**。
- **参数**：N=4 / D=8s / 阈值 TH=8（TCG tick ~5Hz/核 ⇒ 均衡每核 ≥40 tick，5x 余量）。
- **落点**：`el0_ticks` 已在 par_sample（smp.rs:302-326，双核各写自己块）——Δ 窗采样归因干净（裸相 pinned!=0 只有 LB 子任务）；`steal_cnt` 新字段 @ SmpCpu offset 88（128B 块宽不变）+ `note_steal()`（镜像 note_dispatch）+ `steal_readout()`。
- **断言**：单原子行 `LB ok c0=Δ c1=Δ st1= n=4`（Δ=双采样 el0_ticks 差、st1≥1、n_done==N）；诚实降级行 `LB absent …`（PAR 同款）。
- **smoke-smp.sh 扩展**：第三相 gated bare 跑（`-fw_cfg name=opt/berry/lb,string=1`）：27 标记 + `LB gate on` 轮询 → `LB (ok|absent)` 专轮询（DEADLINE_LB=240，PAR 块 374-393 同型）+ 相 1/2 反向断言 `LB gate on` 不得出现 + canary_check + R3 第四数。**相邻义务**：S-3b 同刀把 PAR 断言从 d1>0 档**翻回 witness 档**（§5 回收条件兑现）。

**ktest PI/FAST 契约面重验清单 = 17 断言面 + 6 横切锁纪律项**（PI-K J0-J7 @ ktest.rs:2619-2851、FAST-K L0-L6 @ 3022-3152、boot `I PI ok` @ main.rs:1699-1748、`F FAST ok` @ 2435-2494；全表随实施逐项打勾）。最关键 3 条：

1. **wake_matching/find_single_waiter 锁序逆序边**（ktest.rs:1141 域）：现 send fastpath = 对象锁→睡队扫描，§3 全序要求 SLEEPQ>对象锁——**全图最可能死锁点（R1 首查）**；L1-L3 恒等唤醒集面全挂其上。修法方向 = fastpath 分段取放（E11 面）。
2. **recompute/for_each_sleeper 重入**：wake 钩子已在 SLEEPQ 锁内调 recompute（pi.rs:57-88），内层扫描不得重取同锁——需 `_locked` 分层；donor_edge 裸解引用窗（pi.rs:103）改述为「SLEEPQ 锁持有中」。
3. **pi_probe klock 包络取序**（main.rs:1726）：klock_enter→for_each_sleeper 在锁窗收缩后成逆序边——boot `I PI ok` 面的代码改动点。

**遥测聚合读 API（R6 定案）**：per-CPU 双环 + **每环 seqlock**（写钩只加 2 store，仍零分配——A37 合规）+ **聚合快照函数签名原样保留**（total=Σ 各环、check=逐核单调 + vct 一致、last/span 按 vct 归并——CNTVCT 全局单源是跨核归并的物理根基、wake_hist 逐桶相加）⇒ watchdog/tele_dump/ktest 三读者零改动；**不占锁序**（原子序读定案）+ 原始 per-core 访问器留给未来 trace 服务（对齐可观测性设计-v1.0.md:29-35「每核写自己的环/服务按核轮询」原案（行锚随 §3 前插行 26-33→29-35）——v1 消费面 = 内核聚合，S-3c 随刀在两处文档落同口径）。

## §9 三镜对抗审查结果（2026-11-01 三路并行；全文报告 /tmp/s3_review_{lock,equiv,accept}.md——/tmp 易失，必修项与裁决已全量入册本节，LOW 项随实施刀顺手收）

### 9.0 总判定

**三镜齐一：方向正确、需修后冻结——本节修订落稿后实施序（a→b→c）即冻结。** §3 全序七层相对序经死锁镜全链查证**无环边**（Drop 链回边不存在 F-L13、ASID 面零冲突、E1-E20 外无新环——除等价镜补列的 spawn 边 F-E1）；SQ 最上的方向被确认（「表最外」备选否决 F-L15——保留 E0 级串行使 S-3c 目标归零）；staging 三步的单核零漂移承诺**结构性成立**（等价镜：cpu_id 防御窗返 0 + const 初始化锁无安装步骤 + TOPO const 默认 ≡ Topo::single()）。

**前提纠错（死锁镜）**：KERN_LOCK **不可重入**——`kern_lock.rs:34-43` 朴素 TTAS，同核重取自锁。§3 立法与全部 `_locked` 分层以此为基（E0 时代靠「syscall 主体不可抢占 + IRQ 屏蔽窗」使重取不可达，非靠重入）。

### 9.1 交叉证实面（多镜独立命中 = 必修无疑）

| 面 | 死锁镜 | 等价镜 | 验收镜 | 合折定案 |
|----|--------|--------|--------|----------|
| 放置路由两说并存 + K 算术矛盾 | — | F-E3/F-E4 | F-A1 | **路由 v1 唯一定稿 = 本核优先 + 深度失衡阈值 K=2（常规流量）**；§3「入队侧空核偏好」措辞废除；金丝雀断言与路由形态**解耦**（9.2 裁决二） |
| parked-peer 睡穿 / SGI 发送条件 | — | F-E6 | F-A3 | **无条件向对核发 SGI（幂等），禁读对核 park 态条件发送**（racy 读 = 丢唤醒成因）；park 判据收「对核有 pinned!=0 货则不 park」；SLEEPQ 到点武装归属核定义随 S-3a 落 |
| spawn 表窗内 enqueue 逆序边 | E 表漏列 | F-E1（HIGH） | — | **E 表补边（表→队列）**；修法 = 装配→写字段→放表→enqueue→重取表→簿记；thread_key/pinned **双字段四写点**（syscall.rs:925/:927/:1044/:1052）全前移（F1 侦察面结论扩充） |
| kill 扫描面 | F-L7 | F-E8 | — | kill_all / integrity_check 双面同修：**双队核号升序双持扫描**（integrity 定案 = 双锁聚合快照 + 计数对账 + 容忍偷取空窗瞬态） |
| 偷取判据未定义 | — | F-E2（HIGH） | — | **`state==Ready && pinned!=0` 双过滤**（PickFilter 他核 Running 排除臂的偷取面化身）；「出队后无归属」= 合法新瞬态，integrity/kill 容忍 |
| ktest 双核覆盖诚实性 | — | — | F-A13 | ktest 本质串行——「双核双绿」不证并发 PI/FAST；覆盖归 LB 金丝雀 + soak，§4 表述如实改 |

### 9.2 两裁决

**裁决一（A10 kill 链 E19/E20）= 分段化，不采大锁豁免位/集齐四锁案**。等价镜倾向「按全序集齐四锁再跑链」（语义逐位同今日、零例外条款）；死锁镜对抗后否决：①大锁窗含分配器 × kill_all O(n²) 重扫 = **两核分配停摆**（豁免位代价面）；②豁免位无 lockdep 执法 = 注释温床；③F-L8（kill 匹配 + REAPQ 释放他核 Running 任务 kstack → 远核写已释放栈 = **UAF 写**，E0 已存在、E1 放大；CONFIRMED 代码面/PLAUSIBLE 实命中）**两案同病**——横核 Running 面必须骑缝件，豁免位买不到豁免。采分段化 + **五骑缝件**：①介释放（段间放锁点重验存活）；②保 F6 序（EXIT_HOOK mark→wake→drain→release 序不换）；③双杀 CAS（kill 匹配与状态翻转原子）；④双队扫描（F-L7）；⑤横核 Running 协议（SGI 停等静默 or kstack 延后释放——F-L8 修法，S-3a 的 SGI 底座先行使其可选）。等价镜的「语义逐位」要求转化为分段化后的语义论证义务（随 S-3c 重做，表述按 F-E10 降格为功能等价）。

**裁决二（LB 金丝雀归因）= per-child 归因断言，与路由形态解耦**。验收镜 F-A1 证实 K=2 下 4-burst 第 3 个任务必直接路由 CPU1（失衡 ≥2 触发）——「结构性全落 CPU0」叙事假、st1=0 恒红；F-A2 证实 Δ/st1/n 三谓词被稳态矩阵常驻 EL0 用户（waiter/pinger/ponger/abuser/spawner，main.rs:507-795）污染 = 子任务偷取全坏也能假绿。定案：**4 槽子任务指针静态数组（`LB_CHILDREN`），tick/偷取路径按 `cur ∈ LB_CHILDREN` 记 `lb_ticks/lb_steal_cnt`**（矩阵噪声零污染、子任务仍零 syscall）；主断言 = 每子任务 ran-on 归因（G2 性质 = 均衡对——**直接路由到空核与偷取同格合格**）；st1（全局 steal_cnt）降诊断行。副产物：偷取活证的独立见证改由 lb_steal_cnt 承担，PAR witness 叙事随之改述（F-E12：路由后 PAR ≡ 偷取见证，与 LB 同质——保留为诊断非验收门）。

### 9.3 正文随审修订处（本 commit 内已改）

§3 队列双持规则（F-L1：原「先远端后本地×核号升序」两句互斥 = CPU0 取 1→0 / CPU1 取 0→1 ABBA——改纯核号升序）；§2.2/§2.4 SGI 无条件发送 + park 判据（F-E6/F-A3）；§3 work stealing 路由统一 + 偷取双过滤（F-E2/F-E3/F-E4/F-A1）；§1 A10 行裁决落定；§4/§5 表述降格与断言语义（F-E10/F-A4/F-A13）；§8.D 补 lb_master 私有表 + svc 链 vmar_map（F-A6/F-A16）。

### 9.4 冻结后实施义务清单（按步归属；每步验收前对表打勾）

**S-3a 义务**：~~①SGI 无条件发送纪律（禁 park 态条件读）落码~~（**2026-11-01 除名**——A+B 合并刀验出真回归，降级 S-3b 路由化件，判词 → §9.5 末）；②park 判据含「对核有货不 park」；③SLEEPQ 到点武装归属核规则成文；④单点武装不变量「SLEEPQ 插入只发生在运行（tick 武装）核上」入册（迟醒界 ≤1 slice 的真实来源 F-E5）；⑤rearm 执法点锚 timer::handle IRQ 入口（非 tick 尾 F-E11）；⑥mod4 退役判据 = A51 自旋审计 + LB 子任务零 console + 盯 -ETIMEDOUT/wall（相 2 帽余量 6.5× 低风险 F-E2 猎物②）。

**S-3b 义务**：①spawn 四写点前移 + enqueue 出表窗（F-E1，与 F1 侦察面同刀）；②偷取双过滤判据落码（F-E2）；③LB 金丝雀 per-child 归因（9.2 裁决二）+ lb_master **私有 handle 表**（A45 铁律——表形状 = 4 Process + VMO + 派生位 wait_many；子 VMO 收割撞 -EBUSY 降级不 trap A18/A74）+ svc 驱动链补 vmar_map（0x31 后 0x32 前，F-A16）+ 子任务 CNTVCT 依赖注（A9 面已由副核 bring-up 开 EL0VCTEN 覆盖——静默杀死状与偷取坏同形，排障先查 F-A9）；④smoke-smp 相 3：`LB ok` **严格必需、absent 一票 FAIL**（F-A4——偷取全坏时 n=4 仍可 CPU0 墙钟跑完，absent 放行 = 零成本假绿；「PAR 块同型」限定轮询骨架不含 absent 语义）；宿主侧 grep -E 提 c0/c1/st1/n 数值复验（run10 修订：c1≥TH∧n==4、c0 降观测面——K=2 深度路由下 rq0 常驻噪声致 4 线程全落空队时 c0 结构性 0，F-A5）+ Δ 采样点写死（首入队前/末收割后、klock 包络内 F-A8）+ 相 3 开工 `: > "$LOG"` 截断（canary 跨 boot 混日志必假败 F-A12）；⑤帽序三处（F-A7）：D=8s 名义 = TCG 墙钟 16s（CNTVCT 半速 A8 效应）；DEADLINE_LB=240 锚点在标记轮询**后**起算；guest 收割帽 60–90s（与 harness 240 分层，禁打平竞速）；⑥REAPQ 消费权显式裁决 = 维持 CPU0 双点（F-E9，= E0 等价，cap 32 有界性已论证）；⑦ktest 6 横切锁纪律项成文入 ktest.rs 注释区（F-A13——冻结前硬义务）；⑧跨核唤醒戳回归：enqueue 尾 SGI 随 per-CPU 队列路由化落——发送条件 = 目标核队列真有货（全局队列形态下零派发价值已实证，§9.5 末判词）；禁 park 态条件读纪律（F-E6/F-A3）不变。

**S-3c 义务**：①SQ/RQ/REAPQ/表四面 `_locked` 内层立法（F-L2：不可重入自锁面三处——wake_matching→recompute、kill 钩子链、rx_irq_notify；等价镜/pi_probe klock 包络点随缩窗逐个改序）；②dispatch 分号类别取放（F-L3：约 11 个 syscall 号触 SQ/RQ/REAPQ——svc_enter/exit 全量取锁 = 每号一条反序边；方案 A 分号类别 or 持有集位图，实施时定）；③wait/wake 原子性锚改 **SQ 锁窗**（F-L4：E0 锚 = IRQ 屏蔽窗，分段后窗口敞开 = 丢唤醒永睡——查位+入队同窗 / 写位+扫描同窗）；④send fastpath = find+push+fast_wake **单 SQ 窗**（F-L5：「find 与 wake 分窗」扬队——unlink 非幂等 A39）；⑤kill 分段化五骑缝件（9.2 裁决一）；⑥per-Task 持有集位图 u8 六位（F-L9：klock_window 单 bool 不足以表达 E1 持有集；「持结构锁禁切换」从结构巧合升立法）；⑦新锁 const-init + 零 ops 依赖铁则入 §4（F-L12：ktest KLOCK_OPS None 直通窗 × const-init 兼容确认）；⑧F2 裸打印清单补第三处 heap.rs:96-100（分配器锁内 F-L10；user_fault_exit/REAPQ 溢出两处 PL 包络兼容已核 F-L11）；⑨遥测 last/span 归并语义随双环重验（等价镜猎物⑧：CNTVCT 单源有 S-2b watchdog 全绿既有实证、锁序=时序=vct 序三位一体使归并不改 last 语义——首跑复核）。

### 9.5 S-3a 尾段实施拆解（2026-11-01 侦察定稿；行号锚 = d166b11 实读）

六件义务落 **B→A→C 三步**：B 先行单独可验（多 SGI 流量无害化），A 与 B **合并一刀验**（A 单独落 = 副核深 park 后无唤醒源 = 结构性饿死）；C 纯文档。（**实施后修正 2026-11-01**：①验出真回归除名，B 步实际落地仅 ⑥；「A 单独落 = 结构性饿死」前提被 noSGI 实验证伪——判词见本节末。）

**步骤 B（①+⑥ 代码）——发送咽喉 + mod4 退役**：
- ①咽喉（**实施后除名 2026-11-01**——判词见本节末）= `sched/src/class.rs:53 SchedClass::enqueue` 尾部（一切 Ready 转入共享插入点）：`par_enabled() && cpu_id()==0` → `unsafe { hal::gic::send_sgi(SGI_RESCHEDULE, List(1<<1)) }`。发送条件仅此两静态度（gate + 自核号），**永不读对核 park 态**（F-E6/F-A3 纪律落码）；内核任务入队也发（S-3a 全局队列无路由——浪费可受，SGI pending 合并语义下单写成本；S-3b 路由化自然消音）。CPU1 侧不发：CPU0 在 S-3a 恒不 park（console 心跳环维持 slice 节奏——主核 idle 回卷的深 park 件归 S-3b IDLE 深化，非本步面）。
- ⑥退役 = 删 sched.rs:1495 与 :1550 两处 `park_due()` 门（`smp.rs:362` fn 随手标 retired 注释保留诊断史）；「复发则限频保留为诊断档」的盯面 = smoke-smp 相 2 现存断言（rootfs 120s 帽 6.5× 余量 + echod/-ETIMEDOUT 标记链）+ A51 自旋审计随刀重跑——帽断言 FAIL 即复发信号，无需新增探针。

**步骤 A（②代码）——副核 tickless park 环 + park_recheck 提取**：
- 提取：on_irq_tick 的 cur==null park 派发段（sched.rs:1486-1541）整体迁出为 `pub unsafe fn park_recheck()`（同 crate）；on_irq_tick 留 expire_timeouts/par_sample/cur!=null 抢占面，cur==null 段改早退（release+return）。CNTV 醒与 SGI 醒**收敛单路径**：exception_irq →（tick 槽或 SGI 计数臂）→ eret 回 park 环 → park_recheck——SGI 不进 tick 槽纪律（trap.rs:411）不破。
- 派发路径补授片 arm：原 1521 注释假设「CVAL 已满片终点」成立于 tick 醒；SGI 醒无 CNTV IRQ、CVAL 或为深 park 残留——park_recheck 出选真任务时**自 arm_slice()**（授片 arm ≠ rearm；rearm 锚仍在 timer::handle IRQ 入口，⑤名义区分）。
- 环形（main.rs:3077）：`loop { park_recheck(); par_sample(0,false); park_arm(); wfi; enable_irq; disable_irq; }`；`park_arm` = gate 关 → arm_slice（探测态每 slice 醒查 gate，S-2b 统一环保形）；gate 开且无货 → **SLEEPQ 最近 deadline 者存在则 CVAL 编程至该 deadline（浅 park），否则 timer::mask()（深 park）**。SLEEPQ 空判需 sched 侧新只读 API `nearest_deadline() -> Option<u64>`（头未到期早停同 expire 口径）。
- **实施两补（2026-11-01 落码实证）**：①**masked-wfi 形**——上环若照旧「enable_irq 后 wfi」，enable→wfi 间到达的 IRQ 先被服务（pending 位消费掉）再进 wfi 直睡 = 丢唤醒窗（深 park 无 CNTV 兜底即永睡）；改 wfi 全程 PSTATE.I=1（GIC 线 pending 即醒——QEMU 实证可醒，run-smp t1=8），醒后 enable_irq 即取该中断，三步闭环（Linux arm64 cpu_do_idle 同款）。②**PAR 清钉洞**——深 park 窗无 tick 采样，SMP_CPU1.last_task 滞留上一个 EL0 任务指针 → CPU0 侧 witness 误置假并行见证；park 环 wfi 前补 `par_sample(0,false)` 清钉（本核块独占写免锁）。
- **②判据落形**：本核出选面空（peek_next_filtered null）即 park 判据成立——「对核无 pinned!=0 货」在 S-3a 全局队列形态下退化为「①SGI 兜底」（对核入队即戳，无睡穿窗）；per-CPU 队列真判据归 S-3b（F-A3 修②真面）。
- **③④归属规则成文载体**（随 A 步入册 §2.4 补段 + 调度器设计）：SLEEPQ 插入只发生在运行核（block_current 调用核）= 插入核**单点**武装自身 CVAL；深 park 核的 CNTV 恒不需要武装（插入者必在运行核——不变量自洽）。迟醒界 ≤1 slice 真实来源 = 运行核 slice 节奏（F-E5）。

**步骤 C（③④⑤ 文档）**：③④规则入子案 §2.4 补段 + 母案 §3.3；⑤rearm 锚执法点（timer::handle IRQ 入口，非 tick 尾——F-E11）在 timer.rs handle() 头注升格「执法点」字样 + 授片 arm/rearm 二分名义注记（A 步已带）。

**验证链（A+B 合并刀）**：`just smoke`（单核零漂移：gate 恒关 → 发送臂零触达、park 环探测态形状不变）+ `just smoke-smp` 双相（PAR ok 仍达 = 副核参与在 tickless park + SGI 唤醒下活证；金丝雀严格递增不破）+ `just test-host` + `run-el2`。观察项：相 2 墙钟对比 R3 定标 120s（mod4 退役 + 满速参与的 console 面首跑盯梢——⑥判据）。

**义务①除名判词（2026-11-01 A+B 合并刀实验裁决）**

A+B 合并刀落码后 smoke-smp run4/5 连 FAIL（缺 CHAIN/MANY/SVC/DEMO 四标记 + pinger G1.5 自陷）；SGI 二分定凶（class.rs 发送臂 `if false &&` 静音 → 四标记全绿）后裁决：

- **失效机理（实证）**：CPU1 的 peek_next_filtered 只读扫描恒先命中恒 Ready 的 pinned user_child——SGI 唤醒的「新入队任务」永排不到 = **派发价值今日为零**（post-knife 398/398 全 child vs pre-knife 317/317 全 child，/150s）；成本面 = 每次 CPU0 用户任务入队 SGI → CPU1 醒 → 派发 child 一整片 → console 灌压 +25%（398 vs 317）→ klock/MTTCG 护送队推慢 ponger/svcmgr 轮次 → pinger `wait_one(endA, 30s 帽)` 与 svcmgr console wait（10s 帽）双断 → -110 → 0x4000_0000 自陷（家族惯例故意未映射读）→ 四标记级联饿死。复现器 = `timeout 150 just run-smp` 全量捕获（~15s 起金丝雀，150s 内标记族定形）。
- **A 无 B 自续性（原子性前提修正）**：「A 单独落 = 副核深 park 结构性饿死」担忧**对当前面证伪**——child 自身阻塞带 deadline 入 SLEEPQ → park 环 arm_at(child deadline) → 到点醒 → park_recheck → 派发；noSGI 150s 自续 27 笔派发 + 金丝雀 500→1200 严格递增。未钉任务全由 CPU0 承载，CPU1 唯一客户 = 恒 deadline 阻塞的 pinned child。边界（S-3b 收口）：钉本核任务**无限期**阻塞 + SLEEPQ 空窗 mask 态下被外部唤醒 = 无醒源睡穿——当前面不触达。步 A「②判据落形」段中「①SGI 兜底」口径随除名作废，睡穿边界由本句承载。
- **裁决**：义务①（enqueue 尾 SGI）从 S-3a 除名，降级 S-3b per-CPU 队列路由化件（§9.4 S-3b 义务⑧——届时发送条件 = 目标核队列真有货，派发价值成立）；⑥mod4 退役随 A 半刀保留已验绿。SGI_RESCHEDULE 生产发送面归零——在册发送者仅 bringup probe ③ 观测块（smp.rs），trap.rs 分流臂转 probe 专用（头注已改）。
- **潜伏双雷记档（S-3b IDLE 深化件，本刀不扩）**：①on_irq_tick cur==null 槽原兼 CPU0 idle 再派发（idle_loop 注释「非空时 tick 锁内 pick 派发切走」）——S-3a 迁出后为 wedge；金丝雀永续 runnable 使 CPU0 测试面从不真空（未触达）。②`sec_park_ctx()`（smp.rs）= 全局单槽——旧码 cur==null 分支两核共穿，CPU0 idle 派发会覆写 CPU1 park 帧 = 跨核帧灾难；从未触发，wedge 比静默跨核腐坏更安全（per-CPU 化随 S-3b）。

### 9.6 S-3b 尾段实施拆解（2026-11-01 侦察定稿；行号锚 = 4e8a2a5 实读）

八件义务落 **b-1→b-2 两刀**：b-1 = 结构基座 + 放置路由 + 唤醒戳回归（**无偷取停态**——路由任务由对核队列自消费，功能正确、LB 不均，独立可验可回退）；b-2 = 偷取 + IDLE 深化（双雷收口）+ LB 验收全套。SVC 入口仍全量 KERN_LOCK（S-3c 才拆）——两刀全部队列手术都在 KERN_LOCK 窗内，天然原子。

**侦察基线（4e8a2a5 行号锚）**：sched.rs:71 `static mut RUNQUEUE` / :73 `TOPO=single()` / :116 `rq()` 单源访问器；enqueue 五点 :236/:251/:271/:304/:368（spawn 家族，:304=try_spawn_user 尾、:368=spawn_intent 尾）；pick 消费面五点 :420（yield）/:547（exit）/:1021（block）/:1217-1220（幻影防线 has_eff_above+pick）/:1504-1507（on_irq_tick CPU0 消费版 pick vs 副核 peek 只读版二分）；block/exit 副核 None 分支 = park 切点（:551-562/:1023 上下文，穿 `sec_park_ctx()` 全局单槽 = 雷②消费面）；park_recheck :1556+；idle_loop :1397-1440（参与态 `_empty` 空判未消费 = F5，无派发路径 = 雷① wedge 面）。syscall.rs 两对写点：:909 try_spawn_user → :925/:927 set_task_pinned/set_task_thread（thread_start 面）、:1014 → :1044/:1052（process_start 面）——enqueue 先行（:304）、字段后写 = F1 半初始化窗。class.rs PickFilter :146（user_only/my_cur 两臂）+ eligible :163-174（user_only 排 pinned==0 + 恒排他核 Running）。runqueue.rs：Runqueue const new ✓ / Topo{nr_cpus} 占位 / advance_past 游标原语。smoke-smp.sh：相1 裸 27 + 相2 rootfs 45 + 三帽 240/400/840 + `SGI ok n=` 恰一次（grep -cF 计打印次数非数值——生产 SGI 回归后 n 数值混流量不断言）。`cpu_capacity` **不存在**（hal 新加面）。

**步骤 b-1（结构 + 路由 + 义务⑧）——per-CPU 队列真化**：
- **PER_CPU_RQ**：sched.rs 新 `static PER_CPU_RQ: [CpuRq; 2]`（CpuRq = `{lock: Spinlock, rq: Runqueue}`，const init——按 §8.C A4 冻结形立**含 lock 字段**；b-1 全部触达在 KERN_LOCK 窗内，per-queue lock **不 acquire**（注释声明「S-3c 细化启用」），占位免二次动结构）。`rq()` 内部改分诊 `&mut PER_CPU_RQ[cpu_id()].rq`——单源访问器红利：全部消费点零改自动本核化；:71 RUNQUEUE static 退役删除。gate 关/smoke 单核时 cpu_id()==0 恒 → PER_CPU_RQ[0] 行为等价旧单队列（零漂移判据的根基）。
- **Topo 真化**：`capacity:[u32;2]` 新增（sched::init 从 hal `cpu_capacity()` 填——arch/aarch64/src/smp.rs 新加 pub fn，QEMU virt 同核返 1024 占位；真差异容量 = EAS 面 v2+）。nr_cpus 维持。
- **放置路由 v1**（place 落 enqueue 侧）：内核任务（ttbr0==0）恒 CPU0；用户任务**本核优先** + 深度失衡 K=2（`len[本核] >= len[对核]+2` → 路由对核；4-burst 第 3 个直接路由即此语义）。len 读在 KERN_LOCK 窗内（enqueue 调用点全在锁窗——S-3b 事实，S-3c 改锁时重验）。
- **义务⑧ 唤醒戳回归**：place 路由到非本核 → enqueue 后 `send_sgi(SGI_RESCHEDULE, List(1<<目标))`。发送条件 = 路由决策（目标核≠cpu_id()）三静态度，**永不读 park 态**（F-E6/F-A3 纪律维持）。S-3a 睡穿边界（mask 态外部唤醒）就此收口：SGI 醒 → eret 回 park 环 → park_recheck 读**本核**队列 → 有货派发。§9.5 判词「派发价值成立」的前提兑现——路由任务只在对核队列，peek 不再被恒 Ready child 遮蔽。probe ② 的 `SGI ok n=` 语义随变（bringup 单发 → 含生产流量），smoke 断言面不受动（恰一次打印门），脚本注释更新。
- **F-E1 写点前移（义务①）**：try_spawn_user 签名加 pinned/thread_key 两参，TCB 构造即写、enqueue 最后；syscall.rs 四写点（:925/:927/:1044/:1052）删除改传参。「enqueue 出表窗」= 调用者表锁窗内任务已 runnable 的窗——随 b-1 实施时实读 syscall 锁界定收口（若 spawn 全程在表锁内则 enqueue 移锁外，前移已消半初始化窗、出表窗消锁序窗）。
- **PickFilter 退役改形（F8 承接）**：user_only 臂删（放置保证 CPU1 队列无内核任务——pinned 过滤臂语义由路由接管）；my_cur 他核 Running 排除臂删（Running 任务不在任何队列，队列归属天然替代）。:1504-1507 二分退役——副核改**消费版** pick_next_filtered（队列私有游标，回卷饥饿不复在——peek/pick 双视图退役）。PickFilter struct 缩形或删除，五 pick 点同步。
- **不变量维持**：SLEEPQ 全局不分核（归属规则 ③④：插入核单点武装，§2.4 已成文）；REAPQ 维持 CPU0 双点（义务⑥——现形态已然，b-1 文书确认 + b-2 成文）；integrity_check 暂对 PER_CPU_RQ[0] 单队对账（b-2 扩双队）。
- **验收 b-1**：`just smoke` 八跑零漂移（单核 gate 恒关 → 队列面行为等价）+ `just smoke-smp` 双相 PASS（无偷取停态：路由任务 CPU1 自消费，PAR/金丝雀/27+45 标记不破；相 2 盯 -ETIMEDOUT 家族 = SGI 回归后 console 压力重演哨——K=2 失衡下 rootfs 相路由笔数有限，判词机理不复发）+ `just test-host` + `just run-el2` + unsafe 台账重产。

**步骤 b-2（偷取 + IDLE 深化 + LB 全套 = 义务②③④⑤⑥⑦）**：
- **偷取初版（义务② F-E2 双过滤）**：判据落码 = ①用户任务才偷（ttbr0!=0，内核任务永不偷）②Ready 态（环上即 Ready，天然）。触发 = 本核 pick None 且对核队列非空；粒度单任务、双向。落点两处：park_recheck 出选 None 尾（副核）+ idle 环探测点（CPU0，合流后同径）。手术全程 KERN_LOCK 窗内（S-3b 事实）。
- **A7 IDLE 环合流 + 双雷收口**：idle_loop 参与态分支与副核 park 环合流 per-核 idle 例程——CPU0 面补 reap_drain（义务⑥维持）+ 空判真消费（F5：`_empty` 未消费 → 消费）+ 空→偷取探测→深/浅 park、非空→park_recheck 派发（雷① wedge 收口 = CPU0 idle 得派发径）。雷② = `sec_park_ctx()` per-CPU 化（[ctx;2] 数组内部按 cpu_id 分诊，block/exit 两 park 切点调用零改）。
- **integrity 扩双队对账（R4）**：两队列 len 和 + 各环走查 + SLEEPQ 对账（孤儿段面维持）。
- **LB 金丝雀（义务③⑤）**：fw_cfg `opt/berry/lb` gate（boot 期读 + `LB gate on` 自证行）；lb_master 内核任务 gate 门控 spawn，私有 handle 表（A45）；svc 驱动链（**run7/8 定谳后修订——单进程五任务形态**：单 process_create/vmo/map + root 经 process_start（spread=false 恒家核保 Alive/started）+ 4 计算线程经 thread_create/thread_start（spread=true → place K=2 深度失衡直路路由；vmar_map 补 0x31 后 start 前 F-A16）→ 子任务 `mrs CNTVCT_EL0` 自旋 D=8s 名义（TCG 墙钟 16s，A8 半速；EL0VCTEN 已由副核 bring-up 开——静默杀死状排障先查 F-A9）→ **svc 0x36 thread_exit 汇报**（0x33 会 kill_all_tasks_by_pinned 连杀同进程兄弟——单进程形态生死线）→ wait_many 挂 4 个 Thread 的 SIGNAL_TERMINATED 收割（guest 帽 90s 与 harness 240 分层禁打平竞速，义务⑤帽序③）。断言 `LB ok c0=Δ c1=Δ st1= n=4`——**per-thread 归因**（LB_PROCS 4 槽 Thread ObjRef 地址表，tick 按 cur.thread_key∈LB_PROCS 记，root 为 Process 键天然排除，直接路由与偷取同格合格；st1 全局计数降诊断，9.2 裁决二）；Δ 采样点 = 首线程入队前/末收割后、klock 包络内（F-A8）。
- **smoke-smp 相 3（义务④）**：gated bare 第三 boot（fw_cfg 挂 lb=on，与 berry/par 同款机制）；`LB ok` **严格必需、absent 一票 FAIL**（F-A4——两路径全坏时 n=4 仍可单核墙钟跑完，absent 放行 = 零成本假绿）；`: > "$LOG"` 截断（F-A12 跨 boot 混日志必假败）；宿主 grep -E 提 c0/c1/st1/n 数值复验（**run10 修订：c1≥TH∧n==4，c0 降观测面**——裁决二「断言与路由形态解耦」：K=2 按各环实深改道，bare boot rq0 常驻噪声 → 4 线程全落空队 rq1，c0 结构性 0；TH 定标 24 vs 实测 c1=77，F-A5）；DEADLINE_LB=240 锚点在标记轮询**后**起算（F-A7 帽序②）。PAR 断言维持软档观察（F-E12 文书确认：路由后 d1 ≡ 偷取/路由见证语义注解更新）。
- **义务⑦ ktest 锁纪律成文**：ktest.rs 注释区六横切锁纪律项（锁序 SQ>队列>REAPQ>表>分配器>ASID>打印 + KLOCK_OPS None 直通窗保留声明 F9）。
- **验收 b-2**：`just smoke-smp` 三相 PASS（新增相 3）+ `just smoke` 八跑零漂移（lb gate 缺席 = 相 3 面零扰动）+ `just bench-icount` 基线复核（结构改动后确定性通道重验，漂则刷新）+ `just test-host` + 台账 + run-el2。

**风险与回退**：b-1 缝 = 无偷取停态（最坏 = CPU0 队列堆积墙钟拉长，相 2 帽 840s 兜底）；b-2 缝 = 相 3 独立 boot（LB fail 不污相 1/2 判定）。两刀各自独立 commit，revert 即回退。首刀前 QEMU 场协调照多会话纪律（吃 QEMU 验证前知会兄弟会话）。

**b-1 实施追记（2026-11-01 开刀实录；四项偏离/超集裁决）**：
- **A4 lock 字段否决**：:309 冻结形 `CpuRq{lock:Spinlock, rq}` **被依赖方向约束否决**——sched 架构无关 crate 不引 arch 锁类型（KLOCK_OPS 函数指针槽同款先例）。落地形 = `static PER_CPU_RQ: [Runqueue; 2]` 纯数组；per-queue lock 留 S-3c 拆锁立法时按彼时锁形经注入槽定形（届时一次动结构，非「免二次动」而是「免一次错误预埋」）。
- **nr_cpus 填充点后移 b-2**：`sched::init`（main.rs :497）远早于 `smp_bringup`（:1410）——init 期无 HAL 活性源，capacity 循环 `0..nr_cpus.min(2)` 实填 [0]（nr_cpus 恒 1）。b-1 零拓扑消费者（pick `_topo` 未用、place 只读队长）零行为差；b-2 LB 面在 bring-up 成功后刷 nr_cpus=2 + capacity[1]。
- **integrity_check 直接双队对账**：:315 原拆「b-1 单队、b-2 扩双」——实施直接 `for cpu in 0..2` 各自 ring_check×len 对账（未参与核槽恒空环零成本），b-2 的 R4 扩队义务就此提前完成。
- **enqueue 面实收窄 + enqueue_with_kick 统一入口**：环语义裁决 = preempt/yield 的 prev **从不离环**（FIFO 只推进游标，block 才 unlink/exit 才 dequeue）→ enqueue 实际只 spawn 族五点 + wake_one_inner + expire_timeouts 三处，统一走 `enqueue_with_kick(task, cpu)`（写 home_cpu + 入 rq_of(cpu) + 条件 SGI）；spawn 族传 `place(task)` 新放、wake/expire 传 `home_cpu` 回家不改道（防 sleeper 被唤醒方弹核）。PickFilter 退役成单臂 `eligible(my_cur, t)`（Running-elsewhere 排除**保留**——pick 选中者留环，环上恰有本核在跑的 cur 须跳过、my_cur 自豁免；user_only 臂由放置纯度替代消亡）。

**b-1 排障实录与终裁（2026-11-01；五连消融定谳 + 两面判死 + place 终裁）**：

首验形态：`just smoke` 单核八跑零漂移绿，但 `run-smp`/`smoke-smp` 相 1 FAIL——CHAIN/MANY 缺席（pinger endA wait_one 30s -110 断供自陷）+ rootfs 相 svcmgr 被杀。排障三连修（试错面）+ 消融五跑（A 中和 SGI 槽仍断 / B 回广播仍断 / C 真 noroute 仍断 / D 回原版 wake_matching 绿 / E 复启到达序抓现场）逐件定谳：

- **义务⑧ CPU0 半件（保留）**：`on_sgi_resched` 独立 SGI 槽（arch `SGI_DISPATCH` 槽反转 + trap SGI 分支 EOIR 后经槽调 + main 注册）——tickful 核跨核唤醒快速重调度：gate（par_enabled && cpu0）→ klock → cur null 护栏 → `rq().len()<2` 幻影出选防线 → pick_next_filtered **同 eff 也切** → arm_slice 显式重编（SGI 入口无 timer::handle rearm）→ 切走前放锁/切后不重取/early return（on_irq_tick 同款协议）。消融 A 证其**非**断链元凶（无辜保留），且修跨核唤醒 tick 延迟缺口（义务⑧ 原义）。
- **唤醒上限/到达序两面判死（撤回，判词入 wake_matching 文档）**：曾按「容量配对」改造唤醒面（send 醒 1 读方/recv 醒 1 写方 + `wait_since` 到达序选择）——WM 轨迹定谳**粘性独占者闭环**：user_child 连胜 60/60 轮（since 单调递增 = 每轮被首唤醒→首入队→首跑→抢走腾出的槽→最早重睡→仍「最早到达」）。到达序是**内生量**：唤醒序→入队序→运行序→重睡序→到达序闭环，选择器零熵，首动者永久锁死（广播同集合仅换序即断 CHAIN 链 = 集合等价但序不等价的实证）。旧 deadline 序绿非因序公平，而是 deadline 异构（30s/10s/哨兵各别）+ 超时重排提供外生扰动。**终裁：唤醒面 = 纯广播，公平/上限绝不进唤醒选择器；容量槽公平归放置面（spread 切分）与 b-2 偷取迁移。**wake_matching_n 全尸删除。
- **place 终裁 = thread/process 放置切分**：Task 加 `spread` 位（写入源 = try_spawn_user 调用方语境）——`thread_start` 同进程计算线程 spread=true（K=2 深度失衡判据下路由）、`process_start` 进程根恒家核。判据链：垄断者 = 进程（console burst 灌沟形态在快核 µs 级派发下全吃空槽——唤醒序/上限救不了派发延迟差）；THR 并行收益者 = 线程；APPDEMO(console=true) 留家正确；语义可长期辩护（服务根留 boot 核、计算并行铺开）。b-2 偷取迁移就绪后评估对进程根放开。
- **终形与验收**：原版广播 wake_matching + spread 切分路由 + on_sgi_resched + enqueue_with_kick；run-smp 27/27 标记恰一次（唯 rogue 预期死）→ smoke-smp 双相 + smoke 单核八跑 + run-el2 + test-host 全链（结果见沿革追记）。

**b-2 实施追记（2026-11-01 开刀实录；机制半刀 b-2a + LB 面 b-2b）**：

- **b-2a 机制半刀（commit a69b696）**：偷取初版 `try_steal_one` + F-E2 双过滤（无表内核任务永不跨核——内核串行纪元不变量）+ home 随迁改写（迁移粘性）。**偷取孤儿根因**（判例 §A107）：首版摘对核环后未回插本核环——「出选≠离环」合同（选中执行只推游标，block 才 unlink/exit 才 dequeue）下 victim preempt 让出即成「无环 Ready」孤儿永不被 pick（task_a/task_b 饿死绝迹 + rq0 偷空侧 CPU0 深 park 无醒源整机假死）；短路偷取 400s 对照复跑全绿定谳后补回插。**A7 IDLE 合流**：`SMP_SEC_PARK_CTX` per-CPU 化（雷②跨核帧覆写收口）+ `dispatch_from_park_ctx` 无条件激活派发 + idle_loop par 分支 pick→steal 链 + None 臂 nearest_deadline/mask 深驻留三步（雷① wedge 收口 = CPU0 idle 得派发径）。bench-icount 基线 82→37 随源刷新（idle 派发新形指令流，确定性通道变快也入册）。
- **b-2b svc 参位修**：`svc4(nr,a,b,c)` 只设 x0–x3、**尾寄存器不清**——thread_start 第 5 参 arg 位（x4）吃残值垃圾 → 新增 `svc6` 全参封装，thread_start 调用点显式传 0。
- **b-2b wait_many `!0` 掩码连坏（run1–3 两面机制闭环）**：睡脸全位掩码 = 哨兵收割者被**任何对象的任何新置位**唤醒：①重验风暴 + 僵尸形态（prio 持有者永 Runnable）逐层饿死低 prio（pi_server(0)/pi_probe(0) → pi_client 100s 超时；后链僵尸(16) 再饿死 lb_master(8) 与子(0)，LB ok 永不打印）；②`find_single_waiter` 哨兵分支恒命中 → send fastpath **全局**退 slowpath → 依赖「唤醒即直切」的 poll-0 面（ai_ping）结构性必 miss。修复两面 + 钉板：syscall.rs 请求位**并集**掩码（任一 S_i ⊆ want，无漏醒）+ sched.rs find_single_waiter **位相交**守卫（L3b：位不相交哨兵不挡快路——与 wake_matching 真唤醒集逐位对齐，§A42「不长第二套」执法点再现）+ ktest FAST L3b 新子案 → run7 全绿。
- **b-2b run7/8 `c1=0` 定谳 → thread 形态对齐**：子以 process_start 造（spread=false 恒家核）→ `LB ok c0=53 c1=0 n=4`——路由正确、验收形态选错（进程根留家核是 b-1 place 终裁本意）。修 = spawn 链体重写**单进程五任务**（root 经 process_start 保 Alive/started + 4 计算线程经 thread_create/thread_start，spread=true → K=2 深度失衡直路路由）+ tick 归因键换 **thread_key**（LB_PROCS 存 Thread ObjRef，root 的 Process 键天然排除）+ 出选臂/偷取臂计数打印。
- **b-2b 0x33→0x36 生死线**：模板出口原 svc 0x33 process_exit 会 `kill_all_tasks_by_pinned` **连杀同进程兄弟线程**——单进程五任务形态下谁先转完谁连杀其余（n=4 收割必败）；改 0x36 thread_exit（仅退自身 + join 唤醒 wait_many TERMINATED 位），末线程才 Process 收口。
- **b-2b run9 trampoline IRQ 窗口崩（判例 §A106）**：thread 形态首跑 G1.5 杀数 2>1——`user_entry_trampoline` 旧序先 daifclr 再写 ELR/SPSR，副核 park 派发在 deadline 到点态 unmask 即刻吃 IRQ，异常入口覆写 ELR → eret 以 SPSR=0 落 EL0 执行特权 msr 必杀（CPU0 派发径靠 rearm-before-EOIR「整片无 CNTV」不变式侥幸免疫——**新调度路径点亮旧窗口**）。修 = trampoline 全程不解蔽（EL0 DAIF 由 SPSR=0 给出）→ run10 全绿。
- **b-2b run10 `c0=0` 定谳 → 断言解耦（裁决二适用）**：K=2 按**各环实深**改道是 place() 设计本意——bare boot rq0 常驻字母打印噪声撑深 → 4 计算线程全落空队 rq1 = 正确调度行为，c0≥TH 构造性不可满足 → 降观测面；执法 = **c1≥TH ∧ n==4**（TH=24 定标 vs 实测 c1=77 ≈ 1/3 余量防轮转波动）；LB 行格式/帽锚（F-A7 帽序②）/截断（F-A12）三面钉同值。
- **验收 b-2（收官链）**：smoke-smp 三相 PASS（相 1 27/27 墙钟 4s；相 2 45 标记 + EVS/SKT 对拍 + **PAR ok 提前命中 witness c0=7 c1=18**——S-3b 偷取/路由面生效的端到端活证，S-2b 时代恒 absent d1；相 3 `LB ok c0=0 c1=77 st1=0 n=4` 与 run10 逐字一致）+ R3 定标 boot=4s/rootfs=225s（帽 26%/27%）免升档 + test-host 762/0（L3b 新面只增不减）；主 smoke 八跑零漂移 + run-el2 + bench-icount 复核见沿革追记。

### 9.7 S-3c 拆锁立法实施拆解（2026-11-01 侦察基线 + 刀序草案；行号锚 = main@792fe07 实读；**对抗复核毕（同日）——C1/C2/C4 勘正已回折、Q1–Q4 已补注刀序、C3 由 c-1 实施闭环；复核记录见节末**）

**侦察基线（792fe07 行号锚；§8.A 结构枚举仍有效，此处只录 S-3c 专属增量）**：

- **锁底座现形**（kern_lock.rs 158 行全读）：KERN_LOCK/PRINT_LOCK = TTAS 自旋对（static AtomicBool const-init）；`svc_enter/svc_exit` = SVC 派发包络（取放 KERN_LOCK + KLOCK_FLAG 槽位）；`klock_enter/klock_exit` = 内核任务直调对——**次序铁则：disable_irq 必须先于 kern_acquire**（否则同核 tick on_irq_tick klock_acquire_raw 自旋 = 同核硬死锁；exit 对称 = 清位→放锁→enable_irq 最后）；KLOCK_FLAG Option<fn(bool)> 槽（None = ktest/boot 窗直通）；`-> !` handler（0x33/0x36）不归 svc_exit——放锁在 sched.rs switch_away_forever_with 收口；with_print_lock = irqsave 整消息原子。klock_enter 计数：main.rs 32 / user.rs 1 / sched.rs 2（syscall.rs 走 svc_enter 槽不计）。
- **dispatch 触面实数 = 12 号**（复核 C1 勘正——原草案标题 12、清单实列 11：漏 0x01；F-L3「约 11」正解 = 11 队触号 + 0x01）：0x32 process_start（try_spawn_user→RQ）/ 0x33 process_exit（钩子链 SQ+RQ+REAPQ 全谱）/ 0x35 thread_start（RQ）/ 0x36 thread_exit（join wake SQ + 自 REAPQ 经 `-> !`）/ 0x37 thread_sleep（block_current SQ）/ 0x3B process_terminate（kill_all_tasks_by_pinned 全谱）/ 0x52 channel_send（find_single_waiter 扫 SQ + try_preempt RQ + slowpath wake）/ 0x53 channel_recv（wake_matching SQ）/ 0x06 object_signal（wake_matching SQ）/ 0x04 object_wait_one（block_current SQ）/ 0x05 object_wait_many（block_current SQ）/ **0x01 handle_close（SQ+表 双触面：成功尾直调 pi::recompute（syscall.rs:268）→ 三趟 for_each_sleeper 扫 SLEEPQ 写 pi_inherited/effective（pi.rs:60/65/87）+ 经 holder_table 按 proc_tag 解引用读他任务表（pi.rs:127-135））**。排除面：0x3A get_exit_code 纯 poll（state 检 + copy_out）；0x86/0x87 信号两号无队触（libc 轮询面）；0x03 handle_replace **纯表号**（复核 C2 勘正：replace 全程无 recompute——792fe07 全仓 syscall.rs recompute 仅 :268 close 一处，原判「close/replace 双双触 PI 重推」对 0x03 为假，replace 只做表槽搬迁）。
- **第 13 入口 = EL0 fault（复核 C4 成文）**：exception_sync_el0 非 0x15 臂（trap.rs:337 域）同走 svc_enter 全量 KL（S-2 E0 与 SVC 同构），随后跑全谱杀链（user_fault_exit → task_exit_hook → wake_matching SQ + REAPQ）。分号类别只覆盖 SVC 34 号——fault 向量无人归类；c-2 若按字面「svc_enter 不再全量取」，c-2↔c-3 间的 fault 杀链跑在仅表锁下 = SQ/RQ/REAPQ 全裸。裁决：fault 入口在 c-3 kill 分段化落地前**保持全量锁集**（与全谱号同类取放），否则 fault 杀链与 c-2/c-3 合刀——不得悬空。
- **队结构现形**（sched.rs）：PER_CPU_RQ `[Runqueue; 2]` 纯数组（b-1 落、无 lock 字段——A4 否决「免一次错误预埋」，本步按注入槽定形）；REAPQ static mut :688 + REAPQ_LEN + reapq_push :694 / reap_drain :712（S-2b 注 :538——参与态 CPU1 锁窗退出链可 push，drain 须同窗串行）；SLEEPQ 全局单队（block_current :1117 / wake_one_inner :1198 / wake_matching :1230 / find_single_waiter :1268 / kill_all_tasks_by_pinned :762）；task_exit_hook = syscall.rs:1091（钩子内 wake_matching 搬 SLEEPQ，外层重扫循环消化）。enqueue_with_kick :295（b-1 统一入口）。
- **自锁面三处**（F-L2 不可重入）：wake_matching→PI recompute（sched 内嵌调用）；kill 钩子链（terminate/exit 钩子内再醒人）；rx_irq_notify（main.rs:1403 注册——IRQ 上下文置位 RX Event + wake_matching，任何 SQ 锁方案必须 IRQ 上下文可入）。
- **平变量原子化欠账**（C21 + 同类面）：Event.signals 平 u64（event.rs:23）；Process.pending_signals 平 u64（process.rs:87）——现靠 KERN_LOCK 串行，拆 KL 后 IRQ/跨核读写即竞态，**必须先行于任何锁收缩**；R-0 直方图六平变量（复核 C3 补列：WAKE_HIST 桶×64 + N/MIN/MAX/SIG/TMO——原「写方 IRQ 屏蔽单窗完成全字段更新」的单写者推理明文依赖 KL 巧合；c-1 实施已一并原子化闭环：桶/极值写在前、N 的 Release 递增最后、读者 N Acquire 最先——min/max/桶和三条形状断言无锁窗下仍立）。
- **遥测双环**（telemetry.rs）：RING :45 / WAKE_RING :176 两环全 static mut + HEAD/TOTAL 平变量、64 槽、读者 last()/span() 无同步——现全靠 KERN_LOCK 串行（B12/B13 = per-CPU 双环 + seqlock 读者，F4 硬前置）。
- **持有集载体**：Task.klock_window 单 bool（sched.rs:104 字段 / :205/:213 消费 / :224 写）——义务⑥ 升 u8 六位持有集位图。
- **F2 裸打印第三处**（义务⑧）：heap.rs:96-100——大块 dealloc 失败路径 uart 直打（分配器锁内语境）。

**刀序草案（三步；每步独立可验可回退）**：

- **c-1 前置件（零锁序变化的原子化 + 遥测底座 + 立法载体）**：①C21 双面——Event.signals + Process.pending_signals → AtomicU64（拆 KL 前提，读改写全 cas 化）；②B12/B13——遥测 per-CPU 双环 + seqlock 读者（切换路径多核并发写的唯一现护 = KL，拆前必须自护）；③义务⑥——klock_window bool → 持有集位图 u8 六位（KL 位 + 结构锁五位，c-2 立法载体）；④义务⑦——新锁 const-init + 零 ops 依赖铁则成文（KLOCK_OPS None 直通窗兼容——注入槽形，与 A4 裁决同构）。KL 全程不动，验收 = 全量面绿（smoke/smoke-smp/test-host/icount——icount 预期漂移：原子化读改写指令流变化，prove 后随源）。
- **c-2 结构锁族 + `_locked` 分层 + dispatch 分号类别（义务①②③④ + C20）**：per-queue lock 经注入槽定形（PER_CPU_RQ 扩 lock 字段——一次动结构）+ SLEEPQ 锁 + REAPQ 锁 + 表锁；wake_matching/find_single_waiter/block_current 等内层 `_locked` 改形（自锁面三处 = 调用者已持锁重入路径）；svc 分号类别取放（12 号清单按触面类取：纯表号 / SQ 号 / RQ 号 / 全谱号——方案 A/B（分号 vs 持有集）按 c-1 位图实装定。**复核 C1**：0x01 归 **SQ+表 类且 SQ 先取**——表锁窗内跑 recompute = 表→SQ 逆序 vs 唤醒路 note_sleepq_changed（sched.rs:1252 域）摘队后 recompute 读表的 SQ→表 = 真 ABBA。**复核 Q1**：全 34 号分类义务——号号落类（纯表/SQ/RQ/全谱/无锁五选一）无未分类残留，0x86/0x87/0x3A 落类实施时定（Process.state/exit_code 平读跨核面随之入册）；0x87 take 协议钉死 = swap(0) 先行 + copy_out 失败 fetch_or 回滚（c-1 已实装——回滚只补原位、窗口期新到位位保留 pending 留下轮；分号类别不得改此协议）。**复核 Q3**：place() 远环 len 读（rq_of(other).len()，sched.rs:289）在表锁语境 = F-E1 同族逆序复发边——随队列锁化一并收口（RQ 序在表之上，表窗内碰队列面正是 F-E1 修掉的形态））；wait/wake 原子性锚 SQ 锁窗（义务③：查位+入队同窗 / 写位+扫描同窗——旧锚 = IRQ 屏蔽窗，分段后窗口敞开 = 丢唤醒永睡）；send fastpath 单 SQ 窗（义务④：find+push+fast_wake 同窗——unlink 非幂等 A39）；C20——KERN_LOCK 收缩为对象系/表锁（svc_enter 不再全量取；**EL0 fault 入口例外**——复核 C4：c-3 落地前保持全量锁集，或 fault 杀链与 c-2/c-3 合刀）。锁序 = §3 冻结序 SQ>队列>REAPQ>表>分配器>ASID>打印。
- **c-3 kill 分段化 + 收尾（义务⑤⑧⑨）**：terminate/exit 链按裁决一五骑缝件分段（介释放 / 保 F6 序 / 双杀 CAS / 双队扫描 / 横核 Running 协议）——**前置不变量入册（复核 Q2）：b-1 环语义「Running 仍在环上」**（重扫循环能找回「钩子唤醒后被对核派发」兄弟的唯一根基——kill RQ 扫描只跳 CURRENT；分段+介释放下漏杀论证据此闭合，PickFilter/eligible 高 churn 区不得破）；heap.rs:96-100 裸打印收 PL 包络（义务⑧）；遥测 last/span 归并语义随双环重验（义务⑨——锁序=时序=vct 序三位一体，归并不改 last 语义；**复核 Q4**：ktest/watchdog 既有断言逐条核对清单化——total() ≥ switches()、wt ≤ WAKE_RING_LEN 门、wake_reason_mask boot 双路径、PI/FAST 断言面——非笼统「零改动」）。

**验收（子案 :101 面）**：smoke-smp 全绿 + 主 smoke 零漂移 + test-host + build-release + ktest PI/FAST 面单核双核双绿（ktest 串行本质——并发面覆盖归 LB/soak，如实记载 F-A13）+ icount prove/随源。

**风险与回退**：c-2 = S-3 最大结构险步（锁序回归 = 死锁/丢唤醒两族死状）；缝 = c-1 全程 KL 不动零行为差、c-2/c-3 各自独立 commit revert 面清晰；死锁排障第一现场 = on_irq_tick/reap_drain（IRQ 上下文入锁点）与 rx_irq_notify（F-L2 第三处——IRQ 上下文 SQ 面）。

**对抗复核记录（2026-11-01 复核毕；锚 792fe07 逐行实读，独立复核代理）**：C1（HIGH）12 号清单标题与实列矛盾 + 0x01 handle_close 被错排除——close 成功尾直调 recompute = SQ+表 双触面（三趟扫 SLEEPQ + holder_table 读他表）；c-2 分号类别 0x01 归 SQ+表类 SQ 先取，否则表→SQ 逆序 vs 唤醒路 SQ→表 = 真 ABBA（**已回折 12 号 bullet 与 c-2 刀序**）。C2（LOW）0x03 replace 全程无 recompute = 纯表号（**已回折排除面**）。C3（MEDIUM）直方图六平变量原漏列 c-1 欠账（**c-1 实施已闭环**——复核按锚提交校验故标漏，工作树遥测重写含之；欠账 bullet 已补列并标注）。C4（MEDIUM-HIGH）EL0 fault 入口 = 第 13 入口无人归类，c-2 字面「svc_enter 不再全量取」即裸杀链（**已回折**：成文新 bullet + c-2 裁决——c-3 前保持全量锁集或合刀）。Q1（全 34 号分类义务 + take 协议 swap/回滚钉死）/Q2（「Running 仍在环上」前置不变量）/Q3（place() 远环 len 读 F-E1 同族）/Q4（双环归并断言逐条核对）——**均已补注 c-2/c-3 刀序**。复核 OK 面（核对无误未虚列）：锁底座/队结构/11 号触面/排除面其余（0x02/0x31/0x34/0x51/0x54/vmo/vmar 族无隐藏 reap 链——Process::Drop 只 release vmar）/自锁面三处/heap 裸打印/双环现状/双平变量锚全对；ktest LB/par 计数器已原子化、LAST_VCT 已 per-CPU 主位、IDLE_TASK 仅 CPU0 可达三处**非欠账**。

**c-1 实施追记（2026-11-01；工作树在验，随验证链收官一并 commit）**：六文件实装 = event.rs（signals → AtomicU64：置位 fetch_update / 读 load）、process.rs（pending_signals → AtomicU64）、syscall.rs（0x86 fetch_or 置位；0x87 take = **swap(0) 先行 + copy_out 失败 fetch_or(v) 回滚**——Q1 钉死协议落形：KL 纪元下旧序「读 v→成功才清 0」与新序观察等价（IRQ 屏蔽 + KL 双保险下窗口无并发写者），拆锁后新序严格更优：成功路径窗口期新到位位不丢、失败回滚只补原位、窗口期新位保留 pending 留下轮）、telemetry.rs（B12/B13：per-CPU 双环 + seqlock 读者——写方限本核（cpu_id().min(NCPUS-1) 收敛尾槽）；写序 = seq odd(Relaxed) → fence(Release) → 数据 → fence(Release) → seq+2 store(Release)；读序 = seq load(Acquire) 首读 → odd 重试 → head/total/槽 → fence(Acquire) → seq 复检相等；直方图原子化 = 桶/极值写在前、N 的 Release fetch_add 最后、读者 N 的 Acquire 最先——「桶和≥n」「n>0⇒min≠哨兵」「min≤max」三断言无锁窗下仍立；vct 归并读者随双环重写）、task.rs（held_locks: u8 持有集位图，HELD_KERN_LOCK=1<<0——义务⑥立法载体）、sched.rs（klock_window bool → 位图分诊四点）；unsafe 台账随源至 646 条。Q4 断言逐条核对（实装时核毕）：watchdog 稳态循环零遥测读（2000 tries × spin + klock_enter + kills/puts/switches 三计数器读）——无 O(span²) check() 持续读者拖累；kills==1 终局分支断言**一次性**（total() ≥ switches / check() / wake_total() ≥ 2 / wt ≤ WAKE_RING_LEN 门内 wake_reason_mask==0b11）与双环归并语义逐条对得上；ktest PI/FAST 面留 smoke-smp/icount 复验。**Boot C 帽债插曲（验收链归因定谳）**：首验八跑第 8 相 Boot C 300s 帽 CD ok 未到（fail dump 实录 301-302s 落地 = 输帽 ≤2s 竞态非功能断——全套 rootfs 标记 + switches=2500 在）；定向复现双跑（pwrcls --poison 同源态 + 八标记掐时）：基线（792fe07）CD ok @300s **恰贴帽零余量**（B-1/C-5/A2-3/A3-3 历次加链只升 DEADLINE_ROOTFS 300→420、BOOTC 未跟——b-2 收官八跑 PASS = 侥幸过线），c-1 @304s（FS +5/FAT +6/FATW +4/DF +4/CD +4 全段均匀小漂移 = per-event 原子化开销 ~2% 设计内）→ 定谳：主因帽债、次因 c-1 源致 +4~6s、TCG 波动 ±3-5s 噪声；修复 = tools/smoke.sh **掉电三连帽阶梯化**（A 240→300 守 FAT 前缀 / B 240→360 守 FATW / C 300→420 守全链 CD——C 对齐 ROOTFS 同源同帽口径，A2-3 升帽先例同型；帽只兜结构性挂死）：C 修复后复验八跑 **Boot B 又炸 240 帽**（FATW ok 未到——fail dump switches 钟 @2000 仍在爬 = 慢而非断；对照同 c-1 首跑过线态 FAT @sw1600→FATW @sw1900，本跑 FAT @sw1700 慢 ~6% = c-1 源致 +2% + TCG 波动），外推 FATW ~245-250s 撞穿 240——同债族早一相发作，三连一次清完（常态落点 FAT ~200-206s / FATW ~235-250s / CD ~285-305s，三帽各留 ~40% 余量）。**验证链终局（2026-11-01/02 smoke 面全绿）**：主 smoke 八跑 PASS（帽阶梯三连一次清完，见上）。smoke-smp 首验相 3 一炸（LB ok 240s 未出 + KERNEL PANIC ai_wait_ev -110 + G1.5 kill user_pinger + cpu1 park 反复派发 disp #7/#8 异形态）——三层排查**定谳环境时敏 flake 非 c-1 回归**：①六文件逐行复审全等价（KL 窗内原子化恒等：event/process/syscall 观察语义不变 + telemetry seqlock 写窗读者重试有界无挂点 + sched/task 位图分诊前窗已审毕）；②同内核快照独立复现 6/6 PASS；③差分 A/B（c-1 快照 vs 792fe07 base 主树内重建，逐对交错消时段效应 12+12）**24/24 全 PASS 且 LB tick 逐位同型（c1=76-78）**——宿主单 vCPU 线程 ~100s 停摆单假设全解释失败全家桶：pinger 冻结期 wait 帽早已到期→恢复瞬间链断按设计自陷（label 3 序寄存器 x4=0x40000000 与 dump 分毫吻合，非野指针跑飞——先前「PC=0x1004 跑飞」为 tail -40 截掉 PC/FAR 行的误读）+ ai_demo 100s 帽同窗到期 + LB 12s 段拖到 240s 未出 + 字母流静默，而内核自身活（park 派发/超时扫描持续在跑 = 非死锁非冻结）。c-1 累计 18/18 独立绿。**smoke-smp 复跑三相全 PASS**（相 1 裸相 27 + SGI ok 恰一次 + 金丝雀 100→300；相 2 rootfs 45 + EVS/SKT 逐字节 + PAR ok witness c0=13 c1=8 + 金丝雀 100→2300；相 3 LB ok c0=0 c1=77 st1=0 n=4——与 A/B 分布同型；R3 定标 boot=4s/帽 240、rootfs=198s/帽 840 免升档）。**固化**：smoke-smp.sh fail() 失败自动留全尸 out/smoke-smp-fail.\*.log（本案 tail -40 截行 + cleanup 删原件 = 现场不可逆丢失的教训；成功路径照旧即弃，SMOKE_KEEP_LOG 不变）。余段收官（2026-11-02）：**bench-icount prove 五跑 verdict=exact max_dev=0.000% + BENCHWAKE 五跑逐字节同**（确定性通道保持）→ 基线门禁两项**源致**漂移（frame_alloc4k 171→37 -78% / vmo_commit1p 1708→1574 -7.9%——c-1 位图化/原子化令分配路径变快，方向良性）→ `--update` 刷基线随源（ICOUNT_TOL=0.0 exact 档维持）；test-host 762 全绿（71 套件零失败）；build-release assert-flip OK（boot 低半 ADRP 5 + 主体高半镜像）。**c-1 验收链全链 ✅——下一步 = c-2 结构锁族（S-3 最大结构险步，锁序回归 = 死锁/丢唤醒两族死状）。**
**c-2 实施拆解（2026-11-02 起草；预授权线①自主拆解——§4 视 S-3c 为单步，§9.7 三步中的 c-2 再切三段，每段独立 commit + 全量面绿 + revert 缝清晰；拆解依据 = 本节刀序 + §3 七层冻结序 + staging 裁诀「先拆结构后缩窗」哲学沿用到锁族）**：**c-2a 锁族落形 + 义务③④ SQ 窗先行（KL 全量不动）**——①通用 TTAS SpinLock 原语落 sched crate（const-init + 零 ops 依赖——义务⑦：不经 KLOCK_OPS 槽，ktest None 直通窗兼容；TTAS 形与 arch kern_lock.rs 同源可复制，不引 HAL 依赖）；②Runqueue 扩 lock 字段（PER_CPU_RQ 两槽各一）+ SLEEPQ 锁 + REAPQ 锁落形；③held_locks 位定名启用（bit1=SQ / bit2=RQ0 / bit3=RQ1 / bit4=REAPQ；bit5 表锁 = C20 收缩后 KL 本尊预留）；④SQ 族路径（wake_matching/find_single_waiter/block_current/expire_timeouts）改「KL 窗内 + SQ 锁包络」——锁窗语义定形（义务③查位+入队同窗 / 写位+扫描同窗；义务④ find+push+fast_wake 单 SQ 窗）而 svc_enter 仍全量 KL ⇒ 结构锁全部单写者语境、行为零差；⑤**过渡态逆序显式标注**：KL>结构锁嵌套方向与终态七层序相反（终态 SQ 在表上），过渡态死锁结构不可能（KL 单持有者串行一切结构锁取放点）——代码注释 + 判例双处标注「C20 收缩时随分号类别消解」，防后人误引为终态；验收 = smoke 零漂移 + smoke-smp 三相 + test-host + icount（prove exact 应立；基线门禁预期漂移——TTAS 原语进热路径指令流，--update 随源，c-1 先例同型）。**c-2b RQ/REAPQ 族 + 自锁面三处 `_locked` 分层 + Q3 收口（KL 仍全量）**——①RQ 取放包络（enqueue/pick/unlink/steal/place 面）+ REAPQ 取放（push/drain——drain 摸分配器按 §3 分配器在表下）；②place() 远环 len 读收口（Q3：rq_of(other).len() 入对核 RQ 锁窗或 len 原子化，二选一实施时定）；③wake_matching→recompute / kill 钩子链 / rx_irq_notify 三处 `_locked` 内层改形（F-L2 不可重入立法——KL 在场时段纯结构等价，防 C20 后同核自锁）；④偷取面改锁形（§3 特则：只持远端 + 双持核号升序——S-3b 偷取全在 KL 窗内，本段四步改真锁包络）；验收 = c-2a 面 + LB 金丝雀复验（偷取改形后 LB ok 必绿）。**c-2c C20 KL 收缩 + 分号类别全量 + EL0 fault 例外（唯一行为变更段，最小爆径）**——①34 号号号落类表格入册（Q1 义务：纯表/SQ/RQ/全谱/无锁五选一无残留——0x3A 无锁纯 poll / 0x86/0x87 无锁原子面 / 0x03 纯表 C2 / 0x01 SQ+表且 SQ 先取 C1 / 12 号触面清单为纲，其余号默认全谱保守档）；②svc_enter 全量取 → 按号类别取放；EL0 fault 第 13 入口保持全量锁集（C4 裁决——c-3 落地前）；③KL 语义收缩为表锁（bit5 定名启用）+ 切换舞蹈分诊「非零即禁切换」执法生效（结构锁窗内禁切换立法）；④过渡态逆序边随分号类别消解（SQ/RQ 号不再经 KL 取结构锁）；⑤IRQ 上下文纪律复核（on_irq_tick/reap_drain/rx_irq_notify 入锁点 = §3「短窗 + 永不睡眠 + 永不嵌套打印锁外锁」）；验收 = smoke-smp 三相（内核并行首证：KERN_LOCK 争用计数下降诊断面，§4 S-3c 标记行）+ 主 smoke 零漂移 + test-host + build-release + ktest PI/FAST 锁纪元重验 + run-el2；c-2c 炸 = revert 回 c-2b 态（锁族全绿 E0.5+ 形态）。
**c-2a 实施追记（2026-11-02；验证链全绿随源 commit）**：六文件实装 = **sched/src/spin.rs**（新建——crate 内自持 TTAS SpinLock：compare_exchange Acquire/Relaxed + 内层 Relaxed 自旋 + spin_loop hint + Release 放；零 HAL/KLOCK_OPS 依赖 = 义务⑦；头注三条调用方纪律 = IRQ 屏蔽窗内取放/不可重入/持窗禁切换）、**sched.rs**（REAPQ_LOCK 落形 dead_code；held_mark(bit,on) 助手邻位 klock_flag_current；SLEEPQ_LOCK + sq_lock/sq_unlock **真取放**（acquire + held_mark(HELD_SQ) 对称）+ 过渡态逆序标注注释块 + 窗纪律注释；block_current/wake_matching/expire_timeouts 三路径 SQ 窗成形（两路切点前放锁/早退在锁前/loop 内无中间 return）；find_single_waiter 双生——公开自持窗版给 ktest L0-L4、`_locked` 持窗版；fast_wake_locked 整体改名）、**task.rs**（HELD_SQ=1<<1 / HELD_RQ0=1<<2 / HELD_RQ1=1<<3 / HELD_REAPQ=1<<4 定名 + held_locks 注释「c-2c 切换舞蹈非零即禁执法」）、**runqueue.rs**（`lock: SpinLock` 字段落形 dead_code——b-1 时「届时再扩」预告兑现，c-2b 启用取放）、**lib.rs**（pub mod spin + sq_lock/sq_unlock/find_single_waiter_locked/fast_wake_locked 四符号再导出，fast_wake 旧名移除）、**syscall.rs**（0x52 fastpath 义务④单 SQ 窗：nhandles==0 且 peer 空 → sq_lock + find_single_waiter_locked + push + fast_wake_locked 同窗，**闭窗在 try_preempt 之前**（可切换点不持锁），三出口配平 no-waiter/不可达防御/成功）。**零争用论证**：KL 全量在场 ⇒ SLEEPQ_LOCK 唯一生效锁的一切取放点都被 KL 单持有者串行（单写者语境）⇒ 行为零差；RQ/REAPQ 锁字段落形不取。**验证链**：build 绿 + test-host 762 全绿（71 套件零失败）；主 smoke 首跑阶段 3 rootfs 相 420s 帽炸（金丝雀全程递增 = 调度器活、非死锁形态；echod/evsclient 尾像齐 = 服务链在爬）→ SMOKE_KEEP_LOG=1 复跑同二进制**八跑 PASS 全绿** → 首跑定性冷 build 后 IO 爬行 flake（与 c-1 Boot 帽债不同族：本相全 45 标记无一到达 = 相级迟到非单段漂移，TCG 波动类；观测记录在案，三跑双证见沿革）；smoke-smp 三相 PASS（相 1 裸相 27 + SGI ok 恰一次 + 金丝雀 100→300；相 2 rootfs 45 + EVS/SKT 逐字节 + **PAR ok c0=9 c1=18 witness** + 金丝雀 100→1500；相 3 LB ok c0=0 c1=78 st1=0 n=4；R3 定标 boot=2s/rootfs=219s 帽内）；**icount gate 首跑 FAIL ×2 → 三步归因链定谳源致良性**（frame_alloc4k 37→82 +121.6% / vmo_commit1p 1574→1618 +2.8%，余四项零漂）：①git stash A/B——stash 态精确回 37/1574 且 L2 对拍过 = **100% 归因 c-2a**；②双 ELF 尸检——6 分配器函数（alloc_order 142/buddy_free 72/split 62/merge_up 99/frame_alloc 29/frame_free 16 指令）+ bench::run（2111 指令）+ Vmo::create/commit 尺寸与**归一化反汇编逐指令全同**（/usr/bin/objdump 抹绝对地址后 diff 空——排除 codegen 面；macOS 无 rustup llvm-objdump，Apple LLVM 17 支持 elf64-littleaarch64）；③段足迹算术——.text +0x800 / .bss +0x40 → 镜像末端页对齐恰 **+1 页** → 保留前缀 k 翻面 → ktest 后窗内 order-0 轨道 pop/put ↔ split/merge 翻面（**雷册 §A95 K-1 同骰温和面**——位图化只脱钩扫描面、轨道面仍随布局；漂移独中两漂移项共有的单 order-0 alloc+free 对 +44/+45、order-4 对零漂完全吻合）→ `--update` 随源（82/37/166/1618/6937）+ `--prove` 五跑 **verdict=exact max_dev=0.000%** + BENCHWAKE 五跑逐字节同。第三跑主 smoke **PASS 全绿**（八相：金丝雀 43 严格递增 + rootfs 全链 + RX 交互相五应答 + FAT 子目录/持久化 + B4 掉电采样 A/B+毒环注入+Boot C 恢复终验——三跑 2 PASS/1 flake 双证闭，首跑冷 build IO 爬行 flake 定谳成立；build-release assert-flip OK 同链补面）。**c-2a 收官 ✅——下一段 = c-2b RQ/REAPQ 取放启用 + 自锁面三处 `_locked` 分层 + Q3 place() 收口。**
**c-2b 实施追记（2026-11-02；五切片各自独立 commit + 全量面绿，预授权线①切片序 = RQ → REAPQ → Q3 → 自锁面 → 偷取）**：**零争用总论证（全段适用）**：KL 全量在场 ⇒ RQ/REAPQ 锁一切取放点被 KL 单持有者串行（单写者语境，§A108 过渡态）⇒ 恒零争用、行为零差、窗形先行。**slice 1 RQ 取放包络（91a8e3f）**：runqueue lock 字段去 dead_code 启用 + lock_acquire/lock_release；sched 侧 rq_lock_of/rq_unlock_of 助手（HELD_RQ0/RQ1 记账）+ 本核便捷形 + §A108 过渡态逆序全文注释；十触面穿窗（enqueue_with_kick SGI 戳窗外 / yield / exit / block / preempt 四切换点 / kill 环扫 / idle_loop / on_irq_tick / on_sgi_resched len 读入窗 / park_recheck）；**窗形立法** = 被选者置 Running 与 pick 同窗（Running 标记 = 对核偷取唯一结构屏障，picked-but-Ready 终态即同任务双核双跑）+ prev Ready 化同窗 + EXIT_HOOK 全部窗外（钩子 wake_matching 入队 = 同锁重入自死锁）+ SQ>RQ / RQ>REAPQ 终态七层正序嵌套；验证五环全绿（PAR c0=9 c1=18 d1 活证——计数分布随 RQ 取放真实指令微调时序，活证断言非冻结值）+ icount 逐字节零漂。**slice 2 REAPQ 取放（2da9aea）**：REAPQ_LOCK 启用（HELD_REAPQ bit4 记账在取放本面）；push 三点 = switch_away_forever_with RQ 窗内嵌套（RQ>REAPQ 正序）/ kill 链 SLEEPQ 臂（SQ 锁外 KL 窗内）/ kill 链 RQ 臂（rq_unlock 后）；drain 四点 = yield 双点 + idle_loop 双点（KL>REAPQ §A108 过渡态逆序，窗口短不跨切换）；溢出 loud 打印移窗外（持锁打印违七层末层）；首轮 EVS 相挂 = evsclient 45s 尝试帽 TCG 慢夜临界 + bsh evs 活重复载荷槽未防御——KEEP_LOG 复跑全绿定谳环境 flake 非本刀回归（bsh evs 槽加固独立小刀挂账）；icount 逐字节零漂（0/82/37/166/1618/6937）。**slice 3 Q3 收口（eb98dbd）**：二选一**定谳原子化**——入对核 RQ 锁窗方案被否（place() 表锁语境碰 RQ 面 = F-E1 同族逆序边 + C20 后双 RQ 升序双取）；len Cell→AtomicUsize（单写者 RQ 锁窗内 load+store 免 fetch_add——ldadd 破指令等形；两端 Relaxed 全序由锁提供，原子性只为跨核读不撕裂）；codegen 与 Cell 等形（ldr/str）icount 逐字节零漂实证；消费面三处免改。**slice 4 自锁面三处 `_locked` 分层（ad16830）**：F-L2 不可重入立法——①wake_matching/recompute 拆层（钩子改注册 recompute_locked body，SQ 已持契约六发射点；recompute 转自持窗薄壳 = ktest 直调面 + sys_handle_close 直调面后者原无窗裸跑顺手收口）②kill SLEEPQ 臂补 SQ 窗（扫描+摘队+发射同窗，闭窗先于 EXIT_HOOK——slice 1 RQ 臂同款）③rx_irq_notify 写位+扫描同窗（义务③ IRQ 生产者面，防分段后「位已置而队上无等待者」丢唤醒；wake_matching_locked #[inline] 零 bl 外联实证）；icount 三项漂移归因定谳源致良性：frame_alloc4k 82→37 / vmo_commit1p 1618→1574 = **k 翻面回摆**（.text +0x800 恰跨 0x41000 页界 → 镜像末端再过一页 = k0+2 ≡ k0，§A95 K-1 奇偶周期 2 → 恰回 c-1 值；stash A/B 双 ELF 段表算术实证）、handle_evt 166→170 = 拆壳 crate 内联级联（HandleTable 家族 insert/close/drain/drop_glue codegen 重排源码零改动；双 ELF 函数级尺寸 diff 103 符号 .llvm 哈希翻面佐证）→ 基线随源刷 37/37/170/1574/6937 + prove 五跑 verdict=exact；smoke 首跑相级迟到 flake（c-2a 同族冷构建 IO 爬行，KEEP_LOG 复跑双证）。**slice 5 偷取面改真锁（2a03426）**：§3 特则**只持远端四步包络** = ①远端窗（rq_lock_of(other) 扫描+摘环同窗，victim 定型到 unlink 原子防对核 pick 抢走；两出口 let-else 窗闭合配平）②放远端（无归属窗开始）③home_cpu 随迁独占写 ④本核窗（rq_lock_of(me) 回插 RR.enqueue——不走 dequeue 防置 Exited）；**绝不双持**（F-L1「双持一律核号升序」特则构造性不触发——无归属窗单点入队 = 「双队列同时出现」不变量的构造保证）；两调用面 idle_loop/park_recheck 均在本核 rq_unlock 之后 = 只持远端前提成立；KL 在场恒零争用行为零差，C20 收缩后本窗形即终态；验证五环 = LB ok c0=0 c1=77 st1=0 n=4 与 c-1/c-2a 期逐位同型（**偷取改形后金丝雀零漂复验 = 本段验收硬门 ✓**）+ PAR ok c0=9 c1=18 witness + icount 六项零漂（.text +~0x120 未跨 0x42000 页界 k 保持 k0+2——归因预测命中）。**c-2b 收官 ✅（五切片 91a8e3f/2da9aea/eb98dbd/ad16830/2a03426，锁族全绿 E0.5+ 形态 = revert 锚点）——下一段 = c-2c C20 KL 收缩 + 34 号分号类别全量 + EL0 fault 第 13 入口保持全量锁集（唯一行为变更段，最小爆径）。**

**c-2c 侦察追记（2026-11-02；锚 = fb8d344 工作树净，代码逐面实读）**：**核心新判——表读并发面（复核「无锁」裁决修正）**：对抗复核对 0x3A/0x86/0x87 的「无锁」判断是**队触面视角**（不碰 SQ/RQ/REAPQ ✓）但漏了 **per-Process HandleTable 表读竞态**——`HandleTable::lookup`（handle.rs:477）= `slots.get` + `SlotState` 普通解构读，零原子面；T-1 起同进程多任务可分跑双核（核 A 无锁读表 vs 核 B 写表 close/replace/insert）= 真数据竞态（Vec 结构 + enum 槽双撕裂面）。c-2c 前 KL 全量兜底；**c-2c 后无锁类资格 = 零表触 + 零平读共享字段**。据此逐号实证（handler 现形）：**0x87 take = 唯一真无锁**（current_pinned 不查表 + pending_signals swap/fetch_or 全原子——协议钉死不动；SAFETY 注释「单核 IRQ 屏蔽」依据随改 = 任务运行钉住的结构论证）；**0x3A 降级纯表**（sys_process_get_exit_code:1162 = t.get 查表 + state/exit_code 平读——同表进程并发写即炸）；**0x86 降级纯表**（sys_signal_send:1285 h≠0 路 t.get_raw 查表 + 两路共有 state 平读——state 检读陈旧 Alive 语义可容忍（位置已终止 pending 集 = 信号永不投递，无害面），表读不可容忍）；**default 面天然无锁**（未实现号 0x85 / 未知号 → sys_unimplemented 纯返回零触面）。**34 号落类表（Q1 义务收口，号号无残留）**：无锁 ×2（0x87 + default 0x85/未知）/ 纯表 ×4（0x02 dup、0x03 replace C2、0x3A、0x86——均降级或勘正后入册）/ **SQ+表 ×5**（0x01 close **SQ 先取** C1、0x04/0x05/0x06 wait/signal、0x53 recv——经 handle 找对象 = 表触，block/wake = SQ 触）/ **纯 SQ ×1**（0x37 sleep——唯一零 handle 参数零表触的 SQ 号）/ **RQ+表 ×2**（0x32 start、0x35 thread_start——查表 + try_spawn→RQ；RQ 位取双核升序，place 路由目标不定）/ **SQ+RQ+表 ×1**（0x52 send——fastpath 单 SQ 窗义务④ + slowpath find/wake + try_preempt RQ）/ **全谱 ×19**（0x33/0x36 `-> !` 双号 + 0x3B terminate + 默认保守 16 号：vmo/vmar 族 0x11..0x1B 十一号 + 0x31/0x34 create + 0x51/0x54 + 0x61 clock——低频生命周期号宁过勿缺）。合计 34 ✓。**改形设计（位集形态）**：类别 = u8 bitmask 直接复用 held_locks 位定义（bit1=SQ / bit2=RQ0 / bit3=RQ1 / bit4=REAPQ / bit5=TABLE）——按 §3 七层序**正序取逆序放**（SQ→RQ0→RQ1→REAPQ→表），0x01「SQ 先取」由位序自然实现；类别表存 kernel 侧（install 扩 (Handler, Class) 对——HAL 不依赖内核 crate 槽反转先例延续），HAL 只认位集语义。**svc_enter 改形落点**：dispatch（arch/aarch64/src/syscall.rs:49）在取锁前已握 nr = `tf.regs[0]`——分诊点天然在位，svc_enter 全量形态**保留转 fault 专用**（trap.rs:333 EL0 第 13 入口 C4 裁决同形调用不动）；KLOCK_FLAG 槽语义随收（KL 位 → HELD_TABLE bit5）。**待续侦察面（下一刀实施前补齐）**：①切换舞蹈「非零即禁」执法点现形（sched.rs 切换点查 held_locks != 0 改形 + klock_window 退役面）；②IRQ 三点纪律复核（on_irq_tick / reap_drain / rx_irq_notify 入锁点 = §3 短窗 + 永不睡眠 + 永不嵌套）；③`-> !` 双号收口链（0x33/0x36 全谱位集取的锁在 switch_away_forever_with 按位收口——现按 KL 单位收，须扩位集收口）；④state/exit_code 平读写侧锁窗验证（纯表降级后读写双在表窗——mark_exited（exit 钩子内）写侧须证在表锁窗内，否则 0x3A 读仍裸）；⑤KERN_LOCK 争用计数诊断面（§4 S-3c 验收标记——收缩后下降首证）。

**c-2c 侦察追记二段 + 实施拆解（2026-11-02；workflow 六面侦察归队 + 对抗审计合成的实施定谳——本段落类表**修正版**为准，前段六号作废）**：

- **落类表对抗审计修正 6 号（class-audit 面；根因 = 分类只看 handler 本层不看 handler→callee 全链触面）**：①**0x04/0x05/0x06/0x53 四号升档 SQ+RQ+表**（前段误标 SQ+表）——block_current 内嵌本核 RQ 窗（sched.rs:1328-1339 unlink+pick 与 SQ 窗同段）+ wake 路 wake_matching→enqueue_with_kick 触目标核 RQ（home 可在对核 → RQ0/RQ1 双位）；根因 = §9.7 十二号清单原文把 block/wake 只记「SQ 触」，与 c-2b slice 1 自家「block 属 RQ 十触面」矛盾；②**0x37 改 SQ+RQ（无表）**（零表触断言真但漏 block 内嵌 RQ）；③**0x61 clock_read 初审降档无锁、审查回改升纯表（2026-11-02 三镜对抗审查双镜坐实，§A113 追记）**——硬件读面（CNTVCT/CNTFRQ）确零共享，但出参 copy_out 链经 access_ok→walk_leaf **无锁走查本进程页表 = 跨核页表读者面**（同进程兄弟线程 thread_start spread 分置对核 / 父进程持 process_create 交付的子 vmar 句柄，其 unmap/protect 为 ALL 类写者——与 NONE 读者无共同串行化点；TLBI 落走查与 store 间 = EL1 D-abort→halt 用户态可触发内核 fatal）；0x87 signal_take 同链同改，NONE 实装号归零（deadline 轮询的 C20 争用收益改由 slice 3 结构锁收缩面兑现）。**修正版 34 号落类表（终版，审查回改后）**：无锁 ×1（default 0x85+未知——实装号零）/ 纯表 ×6（0x02/0x03/0x3A/0x61/0x86/0x87）/ **SQ+RQ+表 ×6**（0x01 SQ 先取 C1 / 0x04 / 0x05 / 0x06 / 0x52 / 0x53）/ SQ+RQ ×1（0x37 无表）/ RQ+表 ×2（0x32 / 0x35 双核升序）/ 全谱 ×18（0x33/0x36/0x3B + 默认 15：vmo/vmar 0x11..0x1B 十一号 + 0x31/0x34/0x51/0x54）。合计 34 ✓。凡触 RQ 的类别 RQ 位一律**双核升序全取**（路由目标不定 + held-bit 跳过需本核位在集——统一规则免逐号特判）。
- **架构定谳（悬点收口；noreturn-chain 二刀 + state-write-window 悬点句 + class-audit 张力提醒三方合成）**：**选「入口全位集预取 + 内层窗按持有位退化（幂等跳过）」，否决「入口只预取 TABLE」支**——handler 体内表读与结构窗自由交错（0x01 表锁窗内 recompute / wait 族 block 内嵌 RQ），入口先 TABLE 后内层 SQ = TABLE→SQ 逆序 vs IRQ 路 recompute SQ→TABLE 子窗 = 跨核 ABBA（C1 修 0x01 的同一几何，全类别泛化）。位集预取保七层正序：入口按类别位集正序取（SQ→RQ0→RQ1→REAPQ→TABLE），**内层 acquire 见位已置即跳过（含对应 release）**——内层窗语义 = 「位在 = 外层已持，跳过取放；位不在 = 自持窗照旧」；阻塞切点（block_current :1355/:1380 两切点 + yield/preempt dance）与 `-> !` 收口统一改「按位集逆序放 → 切 →（可归侧）按类掩码重取」——**Task 加 svc_mask: u8 字段**（svc_enter_class 写 / svc_exit 清 / dance 读；内核任务默认 = TABLE-only 掩码，klock_enter 语义随收不变）。**SVC 主体 IRQ 安全论证**：A64 异常入口硬件置 PSTATE.I=1 ⇒ SVC 主体天然 IRQ 屏蔽 ⇒ 全程持结构位无同核 IRQ 重入面（held_mark 无 IRQ 语境混淆）；klock_enter（内核任务直调）disable-first 铁则同构不变。阻塞号中途切换张力就此消解：切点放全部位（逆序）+ 续行重取（svc_mask），「结构锁窗内禁切换」立法不被违反（窗内 = 位在 = 切点前必先放）。
- **切换舞蹈改形（switch-dance 面七切点 + P1-P8）**：klock_release_switch（:208）/klock_acquire_switch（:218）现仅测 bit0（KL）；klock_flag_current（:227）= KLOCK_FLAG 槽唯一位写者——改写 bit5（HELD_TABLE）。**「非零即禁」执法取结构位零断言形态**（klock_release_switch 入口断言 bits1-4==0）——字面 held_locks != 0 不可行：义务位跨挂起随行（task.rs :146 位只在任务自身执行流写 ⇒ 挂起期稳定）。七任务侧切点（:616/:631/:750/:773/:1355/:1380/:1611）全经舞蹈单点执法；**bit0→bit5 迁移跨 arch/sched 两 crate 必须单 commit 原子迁移**（半迁移 = KL 永不放 → 全核自旋）。
- **state/exit_code 写侧零搬迁 + 五前置钉死（state-write-window 面）**：state/exit_code 全部写点（task_exit_hook :1103-1122 + terminate :1267/:1268 + kill 链钩子 :936/:974）在 c-2c 后天然跑在含 TABLE 的锁窗内——纯表号读侧安全由写侧锁窗保证。五前置：①全谱位集恒含 bit5 且正序末取；②EXIT_HOOK（:717-721）先于 TABLE 释放（:773）序禁动；③kill 两臂钩子维持 SQ/RQ 闭窗后、svc 包络内；④trap.rs:333 fault 硬编码全量形防误改（fault 专用 svc_enter 保留 = C4 落形）；⑤task_exit_hook 入口 debug_assert（CURRENT 非空时 held_locks & HELD_TABLE != 0）升运行时不变量。
- **IRQ 面扩容（irq-discipline 面 = 原③ + 审计新发现）**：on_irq_tick 剥 KL raw 包络（:1898）改「逐面自持窗」——expire_timeouts SQ 窗 + **recompute_locked 中段不动点环（pi.rs :79-99 holder_table/holder_has 唯一表读段）新增 bit5 子窗**（**硬正确性项**：holder Vec-realloc 竞态闭 + SQ→TABLE 正序子窗与入口位集序一致）；抢占臂 RQ 单窗已是终态；integrity_check 改逐结构分窗（金丝雀拍点裸走查对核改环 = torn 链 assert panic）；rx_irq_notify 删外层 KL（写位+扫描 SQ 窗 slice 4 已成形 + recompute TABLE 子窗同款）；**残量 KL raw 五处收口**（on_sgi_resched :2029 / park_recheck :2099 / nearest_deadline :2323（现表→SQ 语义错形改 SQ 对）/ yield drain :645 / idle :1797）。铁则泛化成文：任务态任何结构锁取必在 IRQ 屏蔽窗内（kern_lock.rs:94-96 铁则从 KL 泛化到结构锁族）。
- **KL 争用计数微刀（kl-contention-counter 面；硬前置时序）**：KL_CONTENDED_CAS AtomicU64 于 kern_acquire 外层 CAS 失败体 fetch_add（按趟计非按轮计——轮数计数有观察者效应；快路径零新指令）+ kl_contention() 读数 + switches=N 金丝雀行尾 ` kl=<n>` + smoke-smp 三相各自非门禁 echo。**硬前置 = 微刀先落 c-2b 尖端（独立 commit/revert 缝）→ smoke-smp 基线 ×2-3 记逐相末条 kl= → c-2c 落地 → 同法对照（判据 = 逐相 kl_after < kl_before；残值非零正确——全谱号 + tick 路 + fault 入口仍取表锁）**。不进门（§4 :101 非门禁裁决延续）。
- **实施拆解（slice 0-4；每片独立 commit + 全量面绿 + revert 缝；单写者纪律 = 主会话主刀，对抗审查 workflow 后置）**：**slice 0 计数微刀**（KL_CONTENDED_CAS + kl= 金丝雀尾 + smoke-smp echo；验收 = build + test-host + 主 smoke 零漂 + smoke-smp 三相 + icount 三步归因随源；随后基线 ×2-3）→ **slice 1 bit5 定名 + 槽语义原子迁移**（HELD_TABLE 常量 + klock_flag_current bit0→bit5 + 舞蹈双 helper 单诊位迁移 + svc_enter_class 骨架全号暂全谱 = 行为零差；跨 crate 单 commit 原子迁移 + 全仓 HELD_KERN_LOCK 残留清零裁决）→ **slice 2a 内层窗按位退化 + 收口链扩形**（sq/rq/reapq 取放对 + wake_matching 分诊形 + enqueue_with_kick 按位 + kill 链两臂 + Task.svc_mask 字段 + 舞蹈扩 release-all/re-acquire-all + switch_away_forever_with 位集收口 + task_exit_hook debug_assert；全号仍全谱 = 行为零差；验收含 LB 金丝雀零漂硬门）→ **slice 2b 分类表接通**（install 扩 (Handler, Class) 对 + dispatch 按号位集 + 0x61/0x37/default 无锁无表档生效 + fault 入口 svc_enter 全量形保位；**真行为变更点**；KL 争用对照取数 vs slice 0 基线）→ **slice 3 IRQ 面收口**（on_irq_tick KL 剥离 + recompute TABLE 子窗 + integrity_check 分窗 + rx KL 删 + 残量五处 + 铁则泛化成文）→ **slice 4 验收链全量 + 文书**（ktest PI/FAST 锁纪元重验 + run-el2 + build-release + G1.5 rogue 杀数==1 fault 路 C4 收口首证 + 判例/沿革/索引）。revert 锚 = c-2b 态（2a03426）。

**c-2c slice 2a 实施追记（2026-11-02；验证链全绿随源 commit）**：七文件实装 = **sched.rs**（①舞蹈泛化 release-all/acquire-all：release 按**位降序**物理放（TABLE→REAPQ→RQ1→RQ0→SQ）+ **位保持置位**（跨挂起逻辑持有义务——位只在任务自身执行流写 ⇒ 挂起期稳定）；acquire 按**位升序**物理重取（SQ→RQ0→RQ1→REAPQ→TABLE，与嵌套窗取序/2b 入口位集预取序一致防跨核 ABBA）；②`held_test(bit)` 位感知退化核——位在 = 外层逻辑持有，物理取放跳过返 `phys:false`；③sq/rq/reapq 三族取放对全部退化形化（`sq_lock() -> bool` / `sq_unlock(phys)` / `rq_lock_of/rq_unlock_of` / `rq_lock/rq_unlock` 便捷形 / reapq_push·reap_drain 内部退化）；④15 对窗位全穿线 `let phys = xx_lock(); …; xx_unlock(phys)`（enqueue_with_kick/yield/switch_away/kill 两臂/block 五点/wake_matching/find_single_waiter/try_preempt/expire_timeouts/idle_loop/on_irq_tick/on_sgi_resched/park_recheck/try_steal 远端三出口+本核窗）；⑤EXIT_HOOK 窗外契约执法 `assert_exit_hook_windows_closed()` 插三钩子调用点前置（结构位 bits1-4 任一置位即 fail-loud；bit5 随语境不诊））、**syscall.rs**（0x52 fastpath 一锁三闭窗位：`sq_phys` 外提 + miss/防御/hit 三出口互斥消费）、**pi.rs**（recompute 自持窗退化形）、**main.rs**（rx_irq_notify 同款）、**task.rs**（held_locks 文档随 2a 架构刷新：bit0 退役 + 舞蹈扩全位集 + 跨挂起义务 + 退化形）。**拆解偏差记往复**：`Task.svc_mask` 字段自本片挪 slice 2b（2a 全号仍全谱 ⇒ svc_mask 无写者 = 死字段，免死字段原则先落后用；2b 分类表接通时随写者落地）。**位记账错主 bug（§A111 判例全文）**：首验三段全 FAIL（boot 早期 fa_pong 退出带 HELD_SQ=0x2 残留，自己的执法断言当场抓获）——根因 = `block_current` 两分支（正常臂 + park 臂）`set_current` 翻转**后**才 `sq_unlock`，held_mark 绑 CURRENT ⇒ 清位落到被选者/park 空态：窗主位终身不清 → 此后同锁窗恒退化不取锁、位跨睡眠终身携带；定位法 = 断言打 bits+任务名命中 fa_pong → held_mark（HELD_SQ 面）+ 舞蹈放/取步任务名过滤打印 → 轨迹首环「fa_pong 开窗(0x22) → fa_ping 清位(0x0) → fa_pong 携 0x22 入睡」错主现场定谳；修法 = 闭窗（含 PI 钩子发射）整体挪到 CURRENT 翻转前（睡眠者早在 RQ 窗内全量发布，提前放锁无新竞态）——**全域切换点审计唯此两分支犯序**（yield/try_preempt/on_irq_tick/on_sgi/switch_away/park_recheck/idle_loop 全先闭后翻；dispatch_* klock raw 无位面）；教训 = CURRENT-绑定记账原语扩调用面先审该点 CURRENT 生命周期，c-2b 纯观测位欠的次序债机制启用时连本带利。DBG 插桩全数移除（断言本体保留）。**验证链（修复后全绿）**：主 smoke 八跑 PASS（金丝雀 43 严格递增 + rootfs 全链 + RX 交互相 + FAT 子目录/持久化 + B4 掉电 A/B+毒环+Boot C 终验）；smoke-smp 三相 PASS（SMP/SGI 各恰一次 + 裸 27 + rootfs 45 + PAR d1 活证 + LB c1≥24 n=4——**LB 金丝雀零漂硬门 ✓**）；bench-icount L2 六项零容差全在界（bench 面 = alloc/vmo/handle，不含调度窗 ⇒ 退化形指令不进测量面，零漂符合预期；BENCHWAKE 观测行不进门禁）；test-host 全绿；docs_guard PASS。

**c-2c slice 2b 实施追记（2026-11-02；分类表接通——真行为变更点）**：七文件实装 = **kern_lock.rs**（SVC_CLASS_NONE/TABLE/SQ_RQ_TABLE/RQ_TABLE/ALL 五常量（位值镜像 HELD_*，kernel main 编译期断言对齐）+ SVC_WINDOW 槽 `(u8,bool)` + `svc_enter_class/exit_class` 按类对 + `svc_enter` 翻全量形别名（fault 链专用五锁全取）+ 旧 `svc_exit` 死码删除（唯一调用方 dispatch 改按类））、**arch/syscall.rs**（表扩 `[(Handler,u8)]` 默认 `(unimplemented,NONE)` + `install` 三参 + dispatch 按槽取 `(h,class)` 走按类对；越界号 NONE 形态直达 -ENOTSUP）、**sched.rs**（`svc_window` 槽回调：enter = KL 已由 HAL 先取（TABLE 位类）→ 结构升序物理预取 SQ→RQ0→RQ1→REAPQ + `held |= class` + `svc_mask = class`；exit = debug 断言 `held == class`（越类持有/泄漏执法）→ 结构降序放 + 清位清 mask；CURRENT 空窗全跳过 = ktest/boot 形态不变 + **舞蹈双 helper 翻过渡序**：acquire 改 **KL-first**（TABLE→SQ→RQ0→RQ1→REAPQ）、release 改 **KL-last**（REAPQ→RQ1→RQ0→SQ→TABLE）+ `assert_exit_hook_windows_closed` 演化 `held & !svc_mask & !TABLE == 0`（结构位 ⊆ 声明类，全谱号合法携位过钩子点））、**task.rs**（`svc_mask: u8` 字段随写者落地（2a 挪欠兑现）+ IDLE_TASK 补零）、**kernel/syscall.rs**（34 号逐类实参：NONE ×2（0x61/0x87）/ 纯表 ×4 / SQ+RQ+表 ×7 / RQ+表 ×2 / 全谱 ×18 + default NONE）、**main.rs**（SVC_WINDOW 槽安装 + 跨 crate 位对齐 const 断言）、**lib.rs**（svc_window 导出）。

- **入口序 ABBA 预拦截（本段核心偏离；架构定谳段的正序预取在过渡态不成立）**：设计原定入口正序（SQ→…→TABLE 内取）与现役 IRQ 面 KL-first（tick/sgi/park_recheck/idle/yield-drain/nearest_deadline 六处 `klock_acquire_raw` 先取后结构窗，§A108 过渡态不变式）并存 = 跨核 ABBA（A 核 0x2E 类入口持 RQ0 旋 KL；B 核 tick 持 KL 旋 RQ0；双方 IRQ 屏蔽/自旋 = 永久挂死，smoke-smp 必挂）。**修正 = 2b 过渡态 KL-first 入口序 + 舞蹈同步翻序**——全局锁序保持 KL 最外（§A108「一切结构取放点在 KL 窗内」不变式延续保恃：入口/舞蹈/IRQ 三面的结构取放全在 KL 内 ⇒ 单持有者串行一切触点，跨核死锁构造性不可能）。**终态翻正序 = slice 3 与 IRQ 面 KL 剥离/recompute TABLE 子窗原子同刀**（两态全局锁序不可并存——判例 §A112 立法）。无锁类（0x61/0x87/default——前两号当日审查回改升 TABLE，见审查闭环追记）不取任何锁 ⇒ KL 争用下降验收不受影响。
- **0x37 过渡改判含表（落类表二次修正；终版表 SQ+RQ 无表档推迟 slice 3 降档）**：`nearest_deadline`（sched.rs）只持 KL 不持 SQ 锁读队头——0x37 若免表降档，其 block 写 SLEEPQ 与此读失去 KL 串行 = 过渡态竞态（stale 快照，bounded 但非零）。**无表分类必须审读者面不只写者面**：KL-only 免锁读 = 无表档的隐藏依赖。改判后 SLEEPQ 写者面全员持 KL（SVC 体/tick/rx/exit 钩子）⇒ nearest_deadline 串行保持；slice 3 该读面收口（现表→SQ 语义错形改 SQ 对）后 0x37 再降档。
- **舞蹈基源定谳 held 非 svc_mask**（架构定谳段「dance 读 svc_mask/内核任务默认 TABLE-only 掩码」修正）：位跨挂起**保持置位**（位只在任务自身执行流写）⇒ held 即物理真值全息，mask 纯断言元数据（出口 held==class + 钩子点 held⊆mask）；若舞蹈读 mask：内核任务窗外 held=0 而 mask≠0 的形态会漏取 KL 永不放 = 死锁。内核任务永不过 SVC 派发 ⇒ mask 恒 0 亦执法成立（构造器免分叉）。
- **验证链（全绿收官）**：worktree 隔离验证（HEAD ee4d7fc + 本刀补丁——主树 build-user 被并行 GUI 会话半程 WIP 卡住，绕行不碰；A/B 尸检后忘重建补丁 ELF 一度误 prove HEAD 内核——基线值未被污染，重建逐字节复验后重跑定谳）；build/test-host 71 套件零失败/docs_guard PASS（89 锚点）；主 smoke 八跑 PASS（金丝雀 44 严格递增 + rootfs 全链 + RX 交互相 + FAT 子目录/持久化 + B4 掉电采样/毒环/Boot C 恢复终验）；smoke-smp 三相 PASS（SMP/SGI 各恰一次 + 裸 27 + rootfs 45 + PAR ok c0=9 c1=18 witness + **LB ok c0=0 c1=77 st1=0 n=4——c1≥TH/n=4 硬门 ✓（c0=0 硬门与 st1 诊断位口径为审查回改厘清，见追记）** + R3 boot=2s/rootfs=217s 免升档）；bench-icount L2 三项漂移归因定谳源致良性（frame_alloc4k 37→127 / vmo_commit1p 1574→1663 = **镜像 +1 页 k 翻面**——.data SYSCALLS 表加倍 256×8B→256×16B（+0x800）顶 .bss 过页界（0x73000→0x74000）→ 镜像 0x8f→0x90 页，§A95 K-1 同族 order-0 ±45×2 对；handle_evt 170→166 = HandleTable 全家族地址搬迁 + .llvm 哈希翻面（drop_glue 0x1c4→0x1a4）布局级联回摆，c-2b slice 4 同族；frame_alloc64k/vmo_rw4k 零漂旁证）→ 基线随源 127/37/166/1663/6937 + **prove 五跑 verdict=exact max_dev=0.000%** + BENCHWAKE 五跑逐字节同。「零漂预期」预判修正：bench 面确实不经 dispatch，但镜像足迹变化经 k 翻面仍进测量面（§A95 铁面——表加倍是本刀不可避免副产）。KL 争用对照 vs slice 0 基线（相1 {48,77,65} / 相2 {643,606,629} / 相3 {20,21,23}）：实测 相1 kl=54（带内）/ 相2 kl=623（带内）/ 相3 kl=27（上沿 +4）——NONE 号免 KL 调用占 smoke 负载极小（CLK n=6 + SIG n=6）⇒ 2b 净变 ~零符合机制预期；相3 微升 = 舞蹈 KL-first 过渡序把切换点 KL 窗拓宽到结构重取段（§A112 有意代价，slice 3 与 IRQ 面剥离同刀消失）；**真争用下降判据 = slice 3 C20 收缩后兑现**（判据口径：kl_after(2b) 对 kl_before(slice0) 的逐相下降不在 2b 验收，在 c-2c 整段验收）。

**c-2c slice 2b 对抗审查闭环追记（2026-11-02；三镜 workflow：锁序/竞态/验收 + 每候选一驳斥者，55.6 万 token/5 agent）**：**confirmed P1 ×2 = 同一根缺陷双镜独立命中**（锁序镜 + 验收镜，双驳斥者均 high confidence 驳斥失败坐实）——NONE 类 0x61/0x87 的 copy_out 出参链（copy_out_u64→copy_to_user→access_ok→vmar::walk_leaf 三次 read_volatile 无锁游走 + copy_nonoverlapping 裸写用户 VA）与跨核 vmar 变更（ALL 类写者只与彼此 KL 串行）真并行竞态，三分支后果 = TLBI 落游走与 store 间→EL1 EC 0x25→dump+halt（用户态可触发内核 fatal）/ store 抢 TLBI→静默写刚 unmap 可释放帧 / map 装表落两读间→误返 -EFAULT；**是 acbea42 引入的真回归**（父提交 ee4d7fc svc_enter 无条件 KL 串行一切），荒诞自证 = 同链 copy_out 的 0x3A 落 TABLE 持 KL——同刀自相矛盾却漏审（§A113 只应用到 0x37 SLEEPQ 读者面，漏页表读者面）。**修复 = 两号升 SVC_CLASS_TABLE**（仅 KL 无结构锁开销，页表读者/写者 KL 上互斥）+ NONE 实装号归零。**unverified P2 ×4 处置**：①③同根（跨 crate 位断言只钉 ALL 并集）→ **断言扩面三条已修**（TABLE/RQ_TABLE/SQ_RQ_TABLE 逐档 == HELD_* 组合，main.rs）；②死亡路径断言空转（mask=0x3E 时结构位恒 ⊆ mask）→ **注释如实化已修**（sched.rs/task.rs 标注空转面）+ 快照级执法挂账 3-2；④LB 门禁口径错位（「零漂硬门」实只钉 c1≥TH+n）→ **c0=0 升硬门已修**（路由形态金丝雀执法化；st1 依裁决二维持诊断位，c1 阈值制保持）。**KL 预期改写**：CLK n=6 + SIG n=6 免锁负载消失→回升持 KL（smoke 负载占比极小，相值如实记）。验证链 = build ✓/test-host 195 passed ✓/主 smoke 八跑 PASS 零漂 ✓/smoke-smp 三相 PASS（SMP/SGI 各恰一次 + 裸 27 + rootfs 46 + PAR c0=11 c1=7 + **LB ok c0=0 c1=77 st1=0 n=4——c0=0 新硬门首验过** + R3 boot=5s/rootfs=202s 免升档）/KL 三相观测 81/4622/26（vs 2b 期 54/623/27——相2 +3999 = rootfs 用户态 deadline 轮询 clock_read 恢复持 KL 的 C20 直付：NONE 档想省、被 §A113 读者面否掉的开销，slice 3 结构锁收缩后另途兑现）/bench-icount frame_alloc4k 127→37 + vmo_commit1p 1663→1574 两翻 = **§A95 K-1 布局级联族三现**（本刀 diff 零运行时足迹——注释 + const 断言 + 等宽 u8 立即值；机制 = debuginfo 行号表随注释行移位撬页界；混合形态佐证 = 两值回 2b-worktree 形而 handle_evt 保 main 形 166）→ 归因后基线随源 37/37/166/1574/6937 + prove 五跑 verdict=exact max_dev=0.000%（期间两度撞兄弟会话 QEMU 字段分段补跑，icount 确定性通道对 TCG 共存自证不敏）。

**c-2c slice 3 实施拆解（侦察落盘 2026-11-02 @ acbea42；3-1a 已落地 2026-11-02，见 bullet 实施追记）**：KL 触面全景审计 + 终态翻序原子刀拆形。**内序三段（3-1a → 3-1b → 3-2）**：

- **3-1a nearest_deadline SQ 对收口 + 0x37 降档（独立小刀先行，可独立绿）**：`nearest_deadline`（sched.rs:2499）现 KL-only 裸读 SLEEPQ 队头 = §A113 判定的全域**最后一位 KL-only 读者**——c-2b slice 4 起 SLEEPQ 一切写者已持 SQ 窗（wake_matching_locked/expire/kill 链 SQ 臂/block_current 五发射点），该读换 `sq_lock/sq_unlock` 对即闭合（「现表语义错形改 SQ 对」）；落地后 SLEEPQ 全触面 = 纯 SQ 串行，**0x37 随刀降档 SQ_RQ_TABLE→SQ+RQ**（kernel/syscall.rs reg 实参一处 + 注释）。验证 = build/test-host + 主 smoke（0x37 行为面）+ smoke-smp 三相 + KL 对照中间取数（预期微降——该读点频度低）。**实施追记（2026-11-02 落地，二跑链全绿）**：三处实装 = ①`sq_lock()/sq_unlock(phys)` 物理对 + 调用契约补「不持任何结构位」；②新常量 `SVC_CLASS_SQ_RQ(0x0E)`（含死锁安全论证注）+ reg 实参 + main.rs 第五条逐位 const 断言；③**侦察勘误——`integrity_check` 对账走查 = 第二位 KL-only 读者**（对账/观测读者漏数，§A113 追记二），同刀裸物理三窗分窗收口（位感知对不可用：调用点在 set_current 后位说谎 + 屏蔽窗矛盾律免自死锁）= 3-1b ③ 终态形提前落定。验证链全绿：主 smoke 八跑零漂/smoke-smp 三相 PASS（KL 对照 59/4227/28 vs 2b 期 81/4622/26 同口径降 = 0x37 免 KL 首兑；R3 rootfs=203s 免升档）/icount TOL=0.0 六项零漂。**随刀判例刀（console 织入，§A114）**：首跑相 2 840s 假 FAIL——「LSN oBk n=7」行中撕（drain 逐字节裸 uart_putc × ktest 矩阵单字符，无共同锁面；七步自验实全过）；修 = drain + RX ok 两处整条 with_print_lock 包络；同树 6c 那发完整 = 概率窗口实证。
- **3-1b 原子大刀（§A112 两态不并存 ⇒ 单 commit）**：①**六 IRQ 面 + yield drain KL 删**（klock_acquire_raw/release 对整删七处）：`on_irq_tick`(:2070) 剥离后窗形 = expire(SQ 窗自持——**前置校验点：确认 expire_timeouts 内嵌 SQ 窗，c-2b slice 4 五发射点口径**)/RR.tick(cur 私有零锁)/pick(RQ 窗已有)/integrity_check(**已分窗，见③——提前落定于 3-1a**)/PRINT 面(print_lock 自持)/LB·PAR·SWITCHES(原子/per-CPU 零锁)；`on_sgi_resched`(:2206) = RQ 窗自持 + 授片/set_current/activate 全 per-CPU；`park_recheck`(:2276) = RQ 窗 + 偷取远端窗 + dispatch helper 体内自理；`idle_loop`(:1969) = reap_drain(REAPQ 锁) + RQ 窗 + 偷取 + 3-1a 后 nearest_deadline SQ 对；yield drain 臂(:763) = REAPQ 自持。②**入口/舞蹈翻终态正序同 commit**：`svc_enter_class`(kern_lock.rs:115) KL-first → 结构位升序先行 + TABLE 位类内末取 KL；舞蹈 `klock_acquire_switch`/`klock_release_switch`(sched.rs:218/244) 同步翻 KL-last 对称。③**integrity_check 分窗**（:1901）——**已提前落定于 3-1a**（勘误：该走查是第二位 KL-only 读者，见 3-1a 实施追记；裸物理三窗 RQ0→RQ1→SQ 逐窗走查、升序无嵌套各窗独立开闭——KL 剥离后本面零改）。④**rx_irq_notify KL 删**（main.rs:1905）——外层 kern_acquire/release 对删，只留 SQ 窗（位写+扫描同窗 slice 4 立法不变）。⑤**recompute TABLE 子窗**（pi.rs:73 recompute_locked）——SQ 窗内表读段（holder_table 反查）加 KL 子窗：SQ→TABLE 正序嵌套（终态 KL 归表锁本职、最内层），C20 后表读必经 KL 的唯一常驻新面。验证全量 = build/test-host/主 smoke 零漂/smoke-smp 三相 + LB 零漂硬门 + **KL 真降判据首兑**（vs slice 0 基线 {48,77,65}/{643,606,629}/{20,21,23} 逐相下降）+ icount 归因随源（预期 .text 缩 + 可能翻面）+ G1.5 rogue 抽验（fault 链全量形不动）。
  **实施追记（2026-11-03 定谳收官；4 文件 +154/-134，revert 锚 acbea42）**：五件实装与拆形一一对应——①七处 KL 对删（on_irq_tick 五放点/on_sgi_resched 四触点/park_recheck 三点/idle_loop 与 dispatch helper 体内/yield drain 臂，par_enabled 分支随删）；②入口翻「槽结构升序先行 + TABLE 类内末取 KL」+ 出口对称「KL 先放→槽结构降序」+ 舞蹈 klock_acquire_switch（SQ→RQ0→RQ1→REAPQ→KL 末取）/klock_release_switch（KL 先放→逆序放）同步翻，约 15 处注释随改（终态序注/§A108 过渡态注退役）；③integrity_check 零改（3-1a 已提前落定）；④rx_irq_notify 只留 SQ 单锁窗；⑤recompute_locked 不动点段 KL 子窗（第一趟清零/尾趟 refresh_effective 留窗外——纯 Task 字段不触表）。**首跑死锁 + 随刀判例**：ktest test_syscall B2 段 0x86（ktest 首个 TABLE 类号）同核自旋死锁——**位假锁真**形态（ktest 窗 CURRENT=null 位记账不存在，而 SVC TABLE 类主体物理持 KL 为真；sys_handle_close 直调 `pi::recompute()` 手动面重入 kern_acquire）；修 = HAL 层 KL_HELD 持有者镜像（kern_release 先清镜像再放锁——先放后清 = 别核立刻置新值被迟到清写覆盖）+ kl_held() 判据换真值源，与 3-1a 位真锁假合成两方向闭环（§A113 追记三）。**验证链**：test-host 五套件全绿；主 smoke 一修两跑——二跑 FAT 持久化相 `ai_pong: wait READABLE -110` fail-loud PANIC，同二进制复跑八跑全绿 PASS（危险相跨 boot 读回 f42t + M3DEMO + B4 毒环恢复终验 clean）⇒ §A110 单段贴帽临界 flake 定谳非回归；smoke-smp 三相 PASS（SMP/SGI 各恰一次 + 金丝雀严格递增 + PAR c0=11 c1=10 + **LB c0=0 c1=77 n=4 零漂硬门** + R3 boot=4s/rootfs=207s 帽 840s）；**KL 真降判据三相首兑：相1 kl=2 vs 基线 {48,77,65}、相2 kl=183 vs {643,606,629}、相3 kl=0 vs {20,21,23}**（约 -97%/-70%/-100%——tick/park/SGI/舞蹈面 KL 全剥后残余争用 = SVC TABLE 类 + recompute 子窗本体，C20 观测面兑现）。icount 归因 = L2 对拍 TOL=0.0 六项零漂（无翻面——免 --update/--prove；基准路径不触 KL 剥除面，零漂 = 无副作用确认）。
- **3-2 铁则泛化成文 + KL 残量审计**：全域 grep `kern_acquire|klock_acquire_raw` 残量清单落册（预期合法残量 = TABLE 类号 dispatch 24 号 + fault 链全量形 + recompute 表读子窗 + boot 安装期 + lb_master klock 包络 F-A8 Δ 采样——逐处判「表读本职/过渡残留」）；判例候选 §A115（§A114 已被 console 织入判例占用；全局锁收缩收口铁则：读面同刀收口 + 翻序原子同刀 + 分窗走查不裸跨核——§A112/§A113 的泛化合流）；ktest PI/FAST 锁纪元重验 + run-el2 + build-release 留 slice 4 验收链全量。
  **实施追记（2026-11-03；grep 口径 `kern_acquire()|klock_enter()` 全仓 23 直调点 + dispatch 共用点）**：残量清单落册——①**dispatch 共用点 1 处 24 号**（kern_lock.rs:151 svc_enter_class TABLE 类内末取；fault 链全量形 svc_enter()=ALL 经同点；表锁本职）；②**recompute 表读子窗 1**（pi.rs:88，3-1b ⑤ 位感知形；表锁本职）；③**boot 安装期 3**（fwcfg_rootfs_multivol :1598/:1611/:1636——rootfs 多卷装载窗，槽未装物理直取正确形态）；④**ktest 测试面 6**（:1797 PI probe 观测包络 F36 + it_h ×5（:2289/:2310/:2363/:2399/:2428）intent 链任务 F32/F36——锁纪元重验归 slice 4）；⑤**生产 F 位点族 11**（ai_bench_round ×7 + lb_match ×3（F-A8 Δ 采样，在册预期）+ user.rs:1316 G1.5 rogue 验收短窗 F37/F39 读侧收口——内核任务直调临界区合法形态，KL_HELD 镜像后位感知兼容）。**结论：零过渡残留**——全部残量 = 表锁本职/boot 安装期/测试面/生产 F 位点族四类，与终态七层序一致无漏删。§A115 立法落册（四问铁则 + 规则/04 瘦行）。**挂账件**：G1.5 rogue 短窗（user.rs:1316）三种死亡路径形态（成功路 exit_current 不归-舞蹈收口 / 循环续行对称 klock_exit / panic bypass）在终态序下的配平执法重验——归 slice 4 验收链。规则/07:26 陈旧行同刀修（smoke-smp 双相→三相 + 相 3 LB 门 + kl= 观测面）+ AGENTS 速览行同步。
- **slice 4 验收链全量（S-3c c-2c 收尾刀，零代码改）**：ktest PI/FAST 锁纪元重验 + G1.5 挂账销账 + run-el2 + build-release + 索引收尾。
  **实施追记（2026-11-03 收官，三验证面全绿）**：①**ktest PI/FAST 锁纪元重验**——证据三源合一：主 smoke 八跑 PASS（boot 门含 PI ok/FAST ok/TAGTAB ok，ktest 契约面在 boot 序恒断言）+ smoke-smp 三相 PASS（3-2 审计清单④测试面 6 位点 F32/F36 在换纪元后三相全绿）+ run-el2 全谱复证（:131 FAST ok/:185 PI ok）——PI 受捐/表反查在终态 KL 末取序下契约零漂。②**G1.5 挂账销账**——user.rs:1316 rogue 短窗三死亡路径形态配平走查：成功路 exit_current 不归（HELD_TABLE 位在 → 舞蹈 klock_release_switch KL 先放，位随 Task 终结消解）/ 循环续行对称 klock_exit / panic bypass 立法豁免（停机面）；G1.5 ok 双跑实证（smoke 链）+ run-el2 复证（:181）→ 3-2 挂账销。③**run-el2 PASS**——行首 `21BerryOS boot`（:55，virtualization=on 下 EL2 起始 → eret 降级 EL1，KL 剥除/终态翻序不扰引导序）+ 裸相全谱到 THR ok（smoke 46 标记表末位；THR 在列即稳态，搅动字母 = 正常运行态）。④**build-release PASS**——fat LTO 全链 + assert-flip OK（.text.boot ADRP 5 条全低半 bit63=0，目标 0x4000a000/0x4000b000/0x4000c000；boot 低半 + 主体高半镜像，entry=0x40000000——release 优化形态下终态序链接面无断链）。⑤**索引收尾**——本刀只追记既有文档（设计稿/沿革），docs/索引.md 无新落库零改。**S-3 整刀（E1 主刀 per-CPU 化）至此全链收官**：S-3a 双核 bring-up → S-3b 参与态 + LB 相 → S-3c C20 拆锁收缩（c-2a 结构位 → c-2b SVC 类号 → c-2c slice 0 计数器 / slice 1 ktest 纪元 / slice 2 过渡态拆锁 / slice 3 终态翻序原子刀 + 残量审计 + §A115 立法 / slice 4 验收链全量）。

**原子性论证（3-1b 为何单 commit 不可拆）**：入口 KL-first 与 IRQ 面 KL-first 是过渡态锁序图的两个支点——先删任一面，另一面单独在场时新序（入口正序或 IRQ 面无 KL）与残余面（舞蹈 KL-last 或 on_irq_tick KL-first）合成即可能成环（§A112 判例构造同族）；两态原子同刀 = 全域锁序图一次翻面无中间态。revert 锚 = acbea42（slice 2b 态）。
