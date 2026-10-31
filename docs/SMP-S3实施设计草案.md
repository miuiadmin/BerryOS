# SMP S-3（E1 per-CPU 纪元）实施设计草案

> 状态：**立稿初版 2026-11-01 + 侦察四面收官同日**——§8 侦察结果 A-D 全落（锁序改序定案 / LB 验收照抄级 / SGI 预研 + 全禁坑）；**三镜对抗审查待**（S-2 同型流程），审查后冻结实施序再动代码。
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
| 队列 | A4 RUNQUEUE | KERN_LOCK 锁内全局单队（sched.rs:71 + `Topo::single()` :73） | **per-CPU 每核队列 + Topo 真化**（调度器设计 §3.5 兑现）；形态已定（§8.C）：sched 内 `PER_CPU_RQ:[{lock,rq};2]` const 初始化 + `Topo.capacity:[u32;2]` 由 `sched::init` 从 hal `cpu_capacity()` const 表填 + `rq()` 本核分诊；放置最小版 = 内核任务钉 CPU0、用户任务入本核、LB 交偷取 | 每核队列锁 |
| 队列 | A5 SLEEPQ | KERN_LOCK 锁内 | **独立自旋锁**（睡眠/唤醒/超时/expire 全路径改取睡锁）；unlink「锁内」契约表述照旧 | SLEEPQ 锁 |
| 队列 | A6 REAPQ | push/drain 均 KERN_LOCK 锁内 | **独立自旋锁**；drain 摸分配器 → 分配器锁序见 §3 嵌套序 | REAPQ 锁 |
| 调度 | A3 CURRENT | per-CPU 访问器（S-2b 过渡形态） | **真体化**：sched 内部对全局 static 的回落路径收编、注释对齐（行为不变，形态收敛） | per-CPU（免锁维持） |
| 调度 | A7 IDLE | IDLE_TASK 单份（CPU0）+ 副核 park ctx（S-2b §2.3） | **per-核 idle 环统一**：pick 本地 → steal 探测 → 空则 tickless wfi（§2.4）；CPU0 idle_loop 与副核 secondary_main 环合流为同一 per-core idle 例程；IDLE_TASK 符号退役或降格为 CPU0 idle ctx（侦察定形态——「不立第二个 IDLE_TASK」口径延续，深化非推翻） | per-CPU |
| 调度 | A8/A9 integrity/expire | tick 锁窗内 | 随 tick 面拆：本核队列锁内执行（integrity 的跨队走查面 = 侦察重设计点——只查本核 or 双锁聚合快照） | 随队列锁 |
| 调度 | A10 kill_all | KERN_LOCK 全程 | 维持大锁窗（杀链跨任务跨表，粒度不拆——「零阻塞零让出」红利延续；挂 E2 议）。**侦察 C E19/E20**：与 §3 全序冲突（持表跨四层）——三镜审查裁决分段化 or 大锁豁免位（豁免则须入全序特则） | 收缩版 KERN_LOCK |
| 遥测 | B12/B13 双环 | 单环 KERN_LOCK 串行（S-2 撤销记录维持正确） | **per-CPU 双环**（母案 E1 原位——S-2 §7 留面；可观测性设计原案拆核口径随侦察对齐）；读者（watchdog/BENCHWAKE）改聚合快照 API | per-CPU 写 + 聚合读 |
| 对象系 | C20 表/对象 | KERN_LOCK 锁内免费 | **KERN_LOCK 收缩为「对象系/表锁」**（更名或收窄——S-2 §7 退役路径第一步；svc_enter/svc_exit 取放点不动，锁的覆盖面从「一切」缩到「对象系/表」） | 收缩版 KERN_LOCK |
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

**锁序立法（死锁面预防——侦察 C 修正版 2026-11-01）**：

- 全序（**改序**——立稿拟序「队列>SLEEPQ>REAPQ>分配器>表>打印」被现实调用图证伪两处：①持表摸分配器恒常（Drop 链 E1-E10 十条边）→ 分配器须降至表之下；②SLEEPQ 与队列的现实操作序全族 SLEEPQ 先（wake/block/expire/kill E15-E18）→ SLEEPQ 须升至队列之上。反向边零命中）：**SLEEPQ 锁 > 队列锁（双持时先远端后本地，按核号升序）> REAPQ 锁 > 对象系/表锁（收缩版 KERN_LOCK）> 分配器锁（buddy+slab 合一）> ASID 锁 > 打印锁**——证据面 = §8.C 逆序边表 E1-E20；残余重排面收敛为三类：wake/PI 分段取放（E11-E14）、kill 链分段化裁定（E19/E20，A10「维持大锁窗」与全序冲突——三镜审查裁决）、wait 表锁放点前移（E13）。
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

- **LB 金丝雀（状态谓词非竞速——4-5a 铁律 + 母案 §5「计数/旗标谓词」口径；**参数与实施粒度定案 §8.D**）**：fw_cfg 门 `opt/berry/lb`（双门 gate + `LB gate on` 自证）；lb_master 内核任务 burst spawn N=4 个 Process-backed CPU-bound 用户任务（svc 0x11/0x15/0x31/0x32 驱动，内嵌 asm 平像 `mrs CNTVCT_EL0` 自旋 D=8s → `svc 0x33`）；完成收割 = `wait_many` 挂 N 个 Process 的 `SIGNAL_TERMINATED`（C-4b 收集器同款，零 console 依赖）；断言单原子行 **`LB ok c0=Δ c1=Δ st1= n=4`**（Δ=el0_ticks 双采样差 ≥ TH=8、st1=CPU1 steal_cnt ≥1、n=完成数）+ 诚实降级行 `LB absent`；smoke-smp.sh 第三相 gated bare（DEADLINE_LB=240）+ 相 1/2 反向断言 `LB gate on` 不得出现 + R3 第四数。置失衡形态 = 放置路由本核优先 + 深度失衡阈值 K=2（§3）——burst 从 CPU0 侧结构性全落 CPU0 队，CPU1 得货唯一路径 = 偷取，无 force 旋钮。`PAR ok` witness 门从 S-2b 翻转档**回收入场**（S-2 §5.2 回收条件 = IPI + 满速参与，本刀兑现；S-3b 同刀把 PAR 断言从 d1>0 档翻回 witness 档）。
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
- ~~侦察面 B~~ ✅ **2026-11-01 完成**（§8.B——GICv2 dumpdtb 实证 + gic.rs:72 全禁坑 + probe 三步实测序；实测钉钉留 S-3a 首步）。
- ~~侦察面 C~~ ✅ **2026-11-01 完成**（§8.C——E1-E20 逆序边表证伪立稿拟序两处（§3 已改序）+ 分配器合一锁定案 + D25 方案 + ASID 补位 + Topo 五点最小形态）。
- ~~侦察面 D~~ ✅ **2026-11-01 完成**（§8.D——LB 金丝雀照抄级方案 + ktest 重验 17+6 清单 + 遥测聚合 seqlock 定案）。
- **三镜对抗审查（下一门）**：锁序死锁镜 / 语义等价镜（E0→E1 单核不变量逐条）/ 验收有效性镜（金丝雀谓词性）。裁决项 = A10 kill 链分段化 or 大锁豁免位（E19/E20）。审查后冻结拆步序再动代码。
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

### 8.B 侦察面 B：SGI 硬件面预研 + probe 方案（✅ 完成——实测钉钉留 S-3a 首步）

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
- **置失衡形态（侦察定案）**：放置路由 v1 = **本核优先 + 队列深度失衡阈值 K=2**（非 idle 位）——burst 从 CPU0 侧入队结构性全落 CPU0 队，CPU1 得货唯一路径 = 偷取，**无需 force 旋钮**。偷取资格判据维持 `pinned != 0` ⇒ **LB 子任务必须 Process-backed**（svc 0x11/0x15/0x31/0x32 由 lb_master 内核任务驱动——EL1 同权 A14 + pi_client svc 先例）。
- **子任务**：内嵌 asm 平像（~20 指令）：`mrs CNTVCT_EL0` 自旋 8s（D_TICKS=500e6 @62.5MHz）→ `svc 0x33` 退出；纯 EL0 零 syscall（不扰 icount 剖面）。完成收割 = lb_master `wait_many` 挂 N 个 Process 的 `SIGNAL_TERMINATED`(1<<2，abi_gen.rs:96)——C-4b 收集器同款，**子任务零 console 依赖**。
- **参数**：N=4 / D=8s / 阈值 TH=8（TCG tick ~5Hz/核 ⇒ 均衡每核 ≥40 tick，5x 余量）。
- **落点**：`el0_ticks` 已在 par_sample（smp.rs:302-326，双核各写自己块）——Δ 窗采样归因干净（裸相 pinned!=0 只有 LB 子任务）；`steal_cnt` 新字段 @ SmpCpu offset 88（128B 块宽不变）+ `note_steal()`（镜像 note_dispatch）+ `steal_readout()`。
- **断言**：单原子行 `LB ok c0=Δ c1=Δ st1= n=4`（Δ=双采样 el0_ticks 差、st1≥1、n_done==N）；诚实降级行 `LB absent …`（PAR 同款）。
- **smoke-smp.sh 扩展**：第三相 gated bare 跑（`-fw_cfg name=opt/berry/lb,string=1`）：27 标记 + `LB gate on` 轮询 → `LB (ok|absent)` 专轮询（DEADLINE_LB=240，PAR 块 374-393 同型）+ 相 1/2 反向断言 `LB gate on` 不得出现 + canary_check + R3 第四数。**相邻义务**：S-3b 同刀把 PAR 断言从 d1>0 档**翻回 witness 档**（§5 回收条件兑现）。

**ktest PI/FAST 契约面重验清单 = 17 断言面 + 6 横切锁纪律项**（PI-K J0-J7 @ ktest.rs:2619-2851、FAST-K L0-L6 @ 3022-3152、boot `I PI ok` @ main.rs:1699-1748、`F FAST ok` @ 2435-2494；全表随实施逐项打勾）。最关键 3 条：

1. **wake_matching/find_single_waiter 锁序逆序边**（ktest.rs:1141 域）：现 send fastpath = 对象锁→睡队扫描，§3 全序要求 SLEEPQ>对象锁——**全图最可能死锁点（R1 首查）**；L1-L3 恒等唤醒集面全挂其上。修法方向 = fastpath 分段取放（E11 面）。
2. **recompute/for_each_sleeper 重入**：wake 钩子已在 SLEEPQ 锁内调 recompute（pi.rs:57-88），内层扫描不得重取同锁——需 `_locked` 分层；donor_edge 裸解引用窗（pi.rs:103）改述为「SLEEPQ 锁持有中」。
3. **pi_probe klock 包络取序**（main.rs:1726）：klock_enter→for_each_sleeper 在锁窗收缩后成逆序边——boot `I PI ok` 面的代码改动点。

**遥测聚合读 API（R6 定案）**：per-CPU 双环 + **每环 seqlock**（写钩只加 2 store，仍零分配——A37 合规）+ **聚合快照函数签名原样保留**（total=Σ 各环、check=逐核单调 + vct 一致、last/span 按 vct 归并——CNTVCT 全局单源是跨核归并的物理根基、wake_hist 逐桶相加）⇒ watchdog/tele_dump/ktest 三读者零改动；**不占锁序**（原子序读定案）+ 原始 per-core 访问器留给未来 trace 服务（对齐可观测性设计.md:26-33「每核写自己的环/服务按核轮询」原案——v1 消费面 = 内核聚合，S-3c 随刀在两处文档落同口径）。
