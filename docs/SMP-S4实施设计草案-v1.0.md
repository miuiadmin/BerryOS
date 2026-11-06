# SMP-S4 实施设计草案（TLB shootdown——E1 收官刀）

> 版本：**1.0**（v1.0）｜2026-11-06

> 母案：`SMP设计草案-v1.0.md` §4 拆步表 S-4 行 + G2 验收项 2。前刀：S-3 整刀毕
> （71f7bdd；per-CPU 化 + C20 拆锁收缩 + 七层锁序冻结）。本刀 = E1 收官：
> 页表锁 + ASID 回卷全核 quiesce + TLB shootdown 压测。**状态：v2 冻结稿
> （2026-11-03 三镜审查 9 confirmed findings 全量回写；workflow
> wf_a93be6f0-d88，15 agents）。**

## §0 replan 记录（LoopX periodic review 2026-11-03）

- **trigger**：periodic_review_due（6 settled turns，obligation replan-b26f273301e4bc9b）。
- **裁决**：keep——S-3 刚收官、S-4 为拆步表严格线性后继、无扰动证据。
- **next slice**：本刀（todo_6d3d4532b8b9）。**validation**：`just smoke-smp` 全相 +
  G2 项 2 canary 压测标记 + 主 smoke 零漂移 + test-host。**stop**：压测相绿 +
  对抗审查零 confirmed 即收官；撞保留门（新依赖/ABI 破冻）即停问人。

## §1 现状侦察结论（2026-11-03，基线 da11f41——0c/6c 并线零内核代码改动）

### 1.1 TLB 失效现役位点（全部 fire-and-forget，无跨核完成等待）

| 位点 | 形态 | 语义 |
|---|---|---|
| kernel/src/vmar.rs:227/265/285 | map 回滚 / unmap / protect | 「改叶 + `dsb ish` + `tlbi aside1is(asid)`」三连 |
| arch/aarch64/src/mmu.rs:231 | `activate_aspace` | 切 TTBR0 后无条件按号失效（拦同号残留主防线） |
| arch/aarch64/src/mmu.rs:261 | `flush_all_asid` | `tlbi vmalle1is`——ASID 回卷兜底 |
| mmu.rs:199 注释 | `tlbi_aside1` | **自认**「IS 口径——单核下与非 IS 等价，**多核语义先立对**」= 本刀立的对 |

**缺口**：`aside1is` 是广播形态，但发端广播后即继续——**无完成等待**。TCR_EL1
.SH0=0b11（mmu.rs:66，走查 inner WB）下架构语义「广播 TLBI + `dsb ish` = 全域
完成」成立（Linux 同簇做法）；但 **TCG 对该完成语义的建模可信度未证**（可能
建模为全核同步——影响 canary 可构造性，见 §4.5 变异标定）→ 母案钦定
**SGI #2 显式回执**（可观测、确定性、压测可断言）。

### 1.2 页表串行化现状 = KL 支点（syscall.rs reg 表）

- **页表写者**：vmo/vmar 十一号全部 `SVC_CLASS_ALL`（0x3E：分配器+表+kill 面）——
  map/unmap/protect/decommit 全持 KL；SVC 入口 `svc_enter_class` 物理取
  SLEEPQ→RQ0→RQ1→REAPQ（sched.rs:296-310）+ `kern_acquire` 类内末取
  （kern_lock.rs:150-152），**SVC 语境全程 IRQ 屏蔽（trap.rs:331/342/362）**。
- **走查读者**：`clock_read` = `SVC_CLASS_TABLE`（syscall.rs:2112 注释直书
  §A113 判例：copy_out 链 `access_ok→walk_leaf` 无锁走查读者面——升 TABLE
  持 KL 与写者互斥）。
- `walk_leaf`（vmar.rs:382）读者面数到底（§A115 问①）：业务读者全仓唯一入口
  = syscall.rs `access_ok`（:167→:183），调用点五处（copy 族 :204/:217/:228 +
  channel 面 :487/:517）；对账/观测读者零；callee 递归面零。
- **→ S-4 页表锁 = C20 型收缩**：KL（全局）→ per-AS（表树粒度）细锁；全程
  走 §A115 四问方法论。

### 1.3 ASID 面：七层立法已含、实装未落

- **分配器**（mm/src/asid.rs）：轮转计数器，回卷信号 `rolled` → 调用方
  `flush_all_asid()`（fire-and-forget）。文档自认「SMP（G2）后换分代位图」。
- **调用点**（kernel/src/aspace.rs:19-32 `alloc_asid`）：`static mut ASIDS`，
  SAFETY 注释仍写「单核顺序访问」——陈旧假设；唯一生产调用方 =
  `sys_process_create`（SVC_CLASS_ALL，syscall.rs:2128）。
- **七层全序**（S-3c §9.0）：SLEEPQ > 队列 > REAPQ > 表/KL > 分配器 >
  **ASID 锁** > 打印——ASID 锁**只立法未实装**。S-4 实装之（独立小自旋锁
  整段包），**锁语义与等待纪律同刀定**（见 §3.3——锁只护计数器推进）。

### 1.4 SGI 面就绪度（S-4 消费 SGI #2）

- `SGI_TLB_SHOOTDOWN = 2` 预留（gic.rs:34「S-4 预留不消费」；0 保留不用）。
- `send_sgi(id, SgiTarget::{List/Others/Self_})` 原语实证钉板（QEMU arm_gic.c
  源码核对）；`enable_sgi_this_core` banked 纪律现成；SGI_RESCHEDULE(1) 收端
  handler（**arch/aarch64/src/trap.rs:431 `exception_irq` SGI 分流臂**——
  probe ② 收讫计数形态，:410-437）可仿。
- **硬约束（三镜 P0 实锤）**：收端 handler 在 IRQ 上下文跑、**取锁面恒为零**
  （只本地 TLBI 补打 + `dsb ish` + 置回执位）；SGI 收端投递仅在 IRQ 入口
  （SVC 语境 PSTATE.I=1 结构性收不到——gic.rs:202 bank 锁存语义）。

### 1.5 还帧路径全谱（三镜 P0 改口径）

**还帧路径 = `vmo.decommit` + `Vmo::Drop`（close/drain 归零销毁）两条**——
后者是**生产主路径**（pthread join reap / _exec 链每次都走；vmo.rs 映射计数
归零 → Drop → 帧归还分配器）。只钉 decommit 面 = Drop 全链 stale 窗敞开。
- decommit（vmo.rs:147）：mapcnt 防线（区间内仍被映射则拒）。
- Drop：handle close/drain → refs 归零 → 销毁还帧（handle.rs:387 drain 链）。
- unmap **不退帧**（页归 VMO，vmar.rs:252）。
- **canary 必杀点**：unmap（TLBI 广播即返）→ decommit/Drop（帧回分配器）→
  帧即刻复用 → 旧 VA 写必须 fault。无完成等待时他核 stale TLB 项可命中复用帧。

### 1.6 assert 消音复核 + 锁域边界（三镜补形）

- vmar.rs:331 `next_table` 断言「中间层非表描述符」——写者串行下成立，
  随 per-AS 锁实装复核。
- **内部助手锁无关纪律（三镜 P2/equivalence 补）**：`l3_slot_peek`/
  `clear_leaves`/`next_table` 全部调用点都在写者窗**内部**——朴素不可重入
  自旋锁下若字面「读者入锁」= 同核重入自锁（单核 ktest 即挂）。**外部读者
  仅 walk_leaf 一处**；内层分层照 S-3c F-L2 `_locked` 先例。
- **walk_leaf 锁句柄解析链须定形**：walk_leaf 无 Vmar 对象（经
  `current_ttbr0_pa()` 裸走）——读者取 per-AS PT 锁需 current→Process→Vmar
  （root 持锁）解析链，该链过表锁/KL 的序须入 §3 图（KL→PT 单向无环，但
  须成文防实装走样）。

### 1.7 压测基建现状（三镜 coverage 补——原侦察漏面）

- **ktest 有「串行本质」在案判例**——「ktest 双核并发压测」驱动形态须明写
  （双核各钉任务 + 跨核栅障，lb_master 同款 spawn 钉核先例）。
- **期望故障观测器**：trap.rs 期望故障槽——单槽结构、单核假设、ELR+=4
  定长假设、每命中打 UART——四个坑全须 SMP 化改造（per-核槽/位图、静默
  模式、写指令定长约定）后才能承载 canary 断言。
- smoke-smp 现三相（裸 27 / rootfs 46 / LB 相 3）；本刀加**相 4**。

## §2 拆步（v2 冻结——三镜回写后）

| slice | 内容 | 验收 |
|---|---|---|
| **4-1 SGI #2 回执协议基建 + 等待窗纪律** | ①收端 handler（trap.rs 分流臂加臂：本地 `tlbi` 补打 + `dsb ish` + 回执位置位，**取锁面恒零**）；②发端等待原语：**等待窗零锁持有纪律**（见 §3.2——等待恒在一切 SVC 类窗/PT 锁/ASID 锁之外）；③**单核短路契约**：`par_enabled()==false` → 发送与等待双短路（shootdown 退化本地 `tlbi_aside1`、quiesce 退化 `flush_all_asid`）；④**自核排除契约**：目标集恒排自核（Others 形态或位图清本核位），等待谓词永不含发核 bit | smoke-smp 探针标记 `SGI2 ok n=`；**单核 SGI 发送计数恒 0**（等价镜实证项）；主 smoke 零漂移 |
| **4-2 页表锁（per-AS）+ SVC 降档** | ①Vmar root 挂锁（子 Vmar 共享）；写者 = map/unmap/protect + decommit/Drop 触达面；**外部读者唯一 = walk_leaf**（同窗铁律见下）；②**walk+copy 同窗铁律（§A113 正确性前提）**：读者窗 = `access_ok` 走查 + `copy store` 同一 PT 锁窗（粒度可分页续取；分窗是未来优化须带证据）；③内部助手（peek/clear_leaves/next_table）锁无关、`_locked` 内层分层（F-L2）；④current→Vmar→PT 锁解析链入 §3 图；⑤unmap 面 = PT 锁内完成清叶+dsb+广播，**随即退出整个 SVC 类窗，包络外等回执**，等毕按需重入复验区间状态（Linux mmu_gather/return-to-user 同型）；⑥vmo/vmar 号降档评审（候选 = 分配器+表档；kill 面依赖结构性为零——drain 链 v1 零 teardown）——§A115 四问逐项过 | test-host + smoke-smp 全相 + 主 smoke 零漂移；icount 刷新 |
| **4-3 ASID 锁 + 回卷 quiesce（改形：锁外等待）** | ①ASID 独立小自旋锁实装（七层第六层位）——**锁只护计数器推进**：分配路径锁内取号、`rolled` 标记即放锁；②quiesce 等待**恒在锁外**：放锁 + 退出 SVC 包络 → `vmalle1is` → SGI #2 全核回执等待 → 等毕重取锁复验 generation（期间又有回卷则重试）；③aspace.rs SAFETY 注释翻新；④回卷强制测试 = fw_cfg 门（`opt/berry/asidroll` 同款双门）钉位 smoke-smp 相 4，ktest 只读重验零新增打印 | ktest ASID 面只读重验 + 相 4 回卷标记；主 smoke 零漂移 |
| **4-4 压测基建 + canary + 变异标定 + 收官** | ①压测驱动：双核各钉任务 + 跨核栅障（lb_master 先例）；②期望故障槽 SMP 化（per-核槽/位图、静默模式、ELR 定长约定）+「arm 后他核消费」往返探针；③**canary 五步协议**：预热（写者核先访问旧 VA 使 TLB 驻留）→ 栅障 → unmap/还帧（decommit **与 Drop 双构造**——后者与生产 join 同形）→ 复用 → 写旧 VA 期望 fault（计数断言）；写者核与 unmap 核分置 + 同 ASID 兄弟分核形态；④**变异标定（一次性验收步，不入常驻 smoke）**：故意破坏构建（去 tlbi）证明 canary 在 TCG 上真会红——若 TCG 全核同步建模致 stale 构造不出，G2 项 2 证据口径整体改写；标定结论写判例（兼作 §5.1 答案）；⑤`assert!(e==0)` 消音复核收官 | **`TLBS ok n=` 标记**（G2 项 2）+ 相 4 全绿（含「对核满载荷 SVC 风暴中发 shootdown」场景——P0 死锁环钉进可断言面）+ 变异标定结论入册 + 对抗审查零 confirmed |

### 2.1 4-1 实装锚点（2026-11-03 侦察细化，基线 ba8ee33）

- **SGI #2 常量未声明**：gic.rs:37 现役仅 `SGI_RESCHEDULE=1`；:34-36 注释已预留
  语义——4-1 加 `pub const SGI_TLB_SHOOTDOWN: u8 = 2`。
- **banked 使能缺口（实装必做）**：gic.rs:93-97 `ICENABLER0=!0` 后主核 bank 仅
  使能 bit1，副核 = secondary_main 起手 `enable_sgi_this_core`（gic.rs:95 注释
  在案）——两点位各加 SGI #2。pending 在 bank 锁存（smp.rs:738 注释钉板：使能
  先后无竞态、早发不丢）。
- **收端加臂点**：trap.rs:438 `exception_irq` match 加 `SGI_TLB_US` 臂（现臂 =
  CNTV/PL011/SGI_RESCHED :457/未知兜底 :472）：EOIR → 本地 `tlbi` 补打 →
  `dsb ish` → 置回执位——取锁面恒零、零打印零分配（SGI_SEEN fetch_add 同款
  纪律，trap.rs:427-428）。
- **包络外等待落点 = dispatch 尾闩**：syscall.rs:66-68 `svc_enter_class → h →
  svc_exit_class`，闩检查放 `svc_exit_class` 之后（KL 已放 + 结构锁已放 = 零锁
  持有）。形态 = per-CPU pending 闩（目标集位图随闩存）：unmap handler 在 PT 锁
  内完成清叶+dsb+`send_sgi` 广播后**置闩即返**；dispatch 尾见闩则自旋等回执
  位图归零再清闩。等待 = 读内存位图（他核 handler 置位），**不依赖本核 IRQ
  开**——SVC 语境 PSTATE.I=1 不构成阻塞；NONE 类号多付一次 Acquire 原子读
  （单核闩恒 0，短路路径不置闩）。
- **单核短路判据 = `par_enabled()`**（smp.rs:295，Release/Acquire 门）：边界
  安全论证——par 门未置时副核（若已在探测环）从未 activate 用户 aspace、无
  用户 ASID TLB 项，主核本地 flush 语义完备；故判据取 par_enabled 而非
  ncores>1。
- **目标集原语现成**：`SgiTarget::Others` 天然排自核（smp.rs:762 先例）；cpu_id
  （smp.rs:231）供回执位图索引。

## §3 锁序与等待纪律（三镜重造——原 v1 §3 的取锁边分析被 P0/P1 证伪半壁）

### 3.1 取锁边（原分析成立部分）

- 七层冻结序：SLEEPQ > 队列 > REAPQ > 表/KL > 分配器 > ASID > 打印。
- **PT 锁插位 = 表/KL 同层右侧、分配器之上**：①页表写者今天在 KL 窗内
  （ALL 类），PT 锁嵌 KL 窗取放 = §A108 过渡态合法；②map 建中间表出帧 =
  PT 锁内摸分配器 → PT 在分配器之上；③分配器无回调摸页表的反向边
  （三镜核验）；④drain 链 v1 零 teardown → 与 PT 锁无新边。
- walk_leaf 读者解析链：KL→（current→Process→Vmar 解析，过表锁）→PT 锁
  ——单向正序；实装不得走样。

### 3.2 等待窗纪律（三镜 P0 立法——新增全域铁律候选）

- **无环真判据（替代 v1 错误判据「handler 不取此锁」）**：发端等待时
  **持锁集**与他核**一切 IRQ 屏蔽自旋段**（SVC 入口 kern_acquire 旋 KL /
  tick 的 sq_lock 旋 SQ / ASID 锁旋 / 任何包络锁旋）的**交集为空**——空集
  才无环。判别问句：*发端等待时持什么锁？他核一切屏蔽自旋段能消费 SGI
  中断吗？*
- **推论落位**：SGI #2 回执等待恒在——①PT 锁外；②整个 SVC 类窗外
  （svc_exit 后、返回 EL0 前，或任务挂 shootdown-pending 闩）；③ASID 锁外
  （4-3 改形）。**任何「锁内 quiesce」形态（含母案原文）即 §A112 同族
  硬死锁环**——他核在 SVC 入口/tick 的 IRQ 屏蔽自旋段结构性无法消费 SGI。
- **性能注意（P2 级）**：包络外等待期间整核串行面短暂敞开（等待时长 =
  他核最长屏蔽自旋段）——v1 可接受；相 4 SVC 风暴场景实测记录在案。

### 3.3 ack 协议形态

- 回执 = per-核位图（核号索引）；发端构造目标集时**清自核位**；等待谓词
  = 目标位图归零（发核 bit 恒不在内）。
- 收端 handler 取锁面恒零（§1.4）；SGI pending bank 语义下多笔对飞合并
  风险经三镜反驳裁决**在双核阻塞式发送形态下不成立**——实装时保持阻塞式
  （发端等完才返），多 shootdown 对飞解禁留 v2 证据。

## §4 验收面（G2 项 2 全量——三镜补齐）

1. **canary 五步协议**（§2 4-4③）双构造（decommit + Drop）全绿——每次
   unmap 后旧 VA 写**必 fault**（计数断言非竞速）。
2. **`TLBS ok n=` 标记**（相 4 末位）：n = 压测轮数；恰一次。
3. **P0 死锁环可断言面**：相 4 含「对核满载荷 SVC 风暴中发 shootdown」
   场景——等待窗纪律违反即挂死即测出。
4. **等价镜**：单核 SGI 发送计数恒 0；主 smoke 46 标记零漂移；icount 刷新。
5. **变异标定**：一次性破坏构建（去 tlbi）→ canary 必红；结论入判例
  （TCG 是否全核同步建模 → §5.1 答案）。
6. 对抗审查三镜零 confirmed（v2 已回写 9 条；实施后再跑一轮含新代码面）。

## §5 开放问题（v2 更新）

1. **TCG 广播完成语义**：答案由 4-4 变异标定兼任（白盒：QEMU 源码核对
   TLBI IS 建模，send_sgi 先例同款）；结论写判例，回执协议不降级
   （确定性 + 可断言 + 母案钦定——即使广播+DSB 架构上足）。
2. ~~SVC 降档幅度~~ → 4-2⑥ 已定候选（分配器+表档），实施时 §A115 过审。
3. **分代位图缓做**确认：v1 进程个位数 + 回卷 quiesce 闭环，位图留按需。
4. ~~walk 读者快照 vs 同锁~~ → **同窗铁律已冻结**（§A113 正确性前提）；
   分窗优化留证据触发。
