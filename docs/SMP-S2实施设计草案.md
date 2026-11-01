# SMP S-2（BKL 纪元 E0）实施设计草案

> 状态：**S-2 全刀收官（段 A + B + C）**（A = 锁底座 d1cf130；B = F31-F42 包络 + 前置件 2 + D24/I50 + 契约注释立法 5a9a00f——单核可证全绿，icount 三 delta 复位；**C = S-2b 副核参与 2026-11-01 收官**——gate 置位 + secondary_main 探测/参与环 + PickFilter 过滤出选（副核只读版 peek——雷 1 回卷饥饿修复）+ park mod4 限频（雷 2 console 深饿源头治理）+ 副核高半栈（雷 3 向量风暴修复）+ PAR 金丝雀 + smoke-smp 双相扩相；验收 = smoke-smp 双相 PASS（R3 三数 boot 6s/金丝雀 ≈0s/rootfs 129s 全远低帽 70% 线，免升档）+ 主 smoke 八跑零漂移 + test-host 665 + run-el2 同绿）。立稿 2026-10-31；行号锚 = 基线 d8162ba（实施后行号已漂，定位按符号）。
> 母案：`docs/SMP设计草案.md` §2.1（E0 三边界 = 规格源头，本立稿是细化不是推翻）+ §4 S-2 行 + §6 风险。
> 输入：五路侦察 2026-10-31（入口/退出面、睡眠/切换面、IRQ 与内核任务面、副核执行面、验收与纪律面——/tmp/s2-scout-*.md，已全文消化并入本文）。
> 话术限定（母案 §7）：E0 BKL 双核 = **内核串行 + 用户并行**，不是「SMP 内核」——S-3 才可说内核并发；本文一切「双核」表述受此限定。

## §0 定位与边界

**定位**：把 v1 的隐式临界区——「单核 + SVC 异常入口硬件置 DAIF 屏蔽 + handler 全程不解蔽」（CLAUDE.md 3-3 铁律）——显式化为**一把 KERN_LOCK 入口大自旋锁（BKL）**，按母案 E0 三边界执法取放点；在此之上让副核从 S-1 的 park-only 转为**最小参与**（只跑 EL0 用户任务）。内核行为与单核语义等价（§3.4 精确口径），用户任务获得真双核执行。

**输入**：母案 §2.1 + 五路侦察（头部注）。**基线**：d8162ba。

**S-2 只做 BKL，禁止混入清单**（以下均属 S-3/S-4/S-5 面，本刀出现任何一项即范围失控）：

| 禁项 | 归属 | 本刀替代形态 |
|------|------|--------------|
| per-CPU runqueue / 负载均衡 / 放置策略 | S-3 | RUNQUEUE 全局单队 + KERN_LOCK 串行（E0「内核串行」语义本体） |
| 细粒度锁拆分（对象系/表/分配器分锁） | S-3 | 一把 KERN_LOCK + 一把打印锁（两把为度——测试策略.md:35-38 纪律③「自旋锁+per-CPU+原子计数三件套为度」） |
| IPI/SGI、跨核唤醒快通道 | S-3 | 副核 wfi + 周期 tick 探测（唤醒延迟 ≤1 slice，E0 接受、§6 R2 记账） |
| TLB shootdown / canary 页 | S-4 | 不涉及（用户任务 vmar 操作全在锁内；shootdown 需求随 S-3 per-CPU 页表态才出现） |
| lockdep / 全抢占内核 | S-5/非目标 | 注释纪律执法（§6 R8） |

**例外前置件**（名义归 S-3/E1 但必须在 S-2 提前——侦察③ D 区结论：不提前则 S-2 首跑即炸）：

1. **IRQ_TICKS/SWITCHES 原子化 + USER_KILLS/LAST_KILLED 原子化**（sched.rs:20/:23/:325/:329，tick 路径 :1188/:1204 裸 RMW、kill 链 :339-340 裸写）：写点全在 KERN_LOCK 窗内（tick 协议 §2.2 / kill 链 :383 收口）= BKL 下已串行零竞争——**原子化的执法对象是锁外读者**（watchdog 忙等读 F37、`switches=` 金丝雀打印 A2）的撕裂读 + `static mut` 别名 UB 封死；金丝雀的**数据源本身**要先正确（§5.2）。per-CPU 化**否决**：BKL 下无并行增量可丢，per-CPU + 聚合打印/聚合读三块连带面全数蒸发（立稿后审查裁决——与初稿前置件 1 遥测双环矛盾面同源，见下注）。
2. **CPU0 per-CPU 块 + LAST_VCT per-CPU 化**（SmpCpu 现仅副核有，TPIDR_EL1 只 secondary_entry 写一次——smp.rs:113-137；LAST_VCT class.rs:29）：per-CPU 遥测采样/PAR 见证/CURRENT 访问器都要本核身份，主核侧补块是前置；**LAST_VCT 随块 per-CPU 化是双 tick 的结构性必须**（两核共享全局 LAST_VCT 时每核 tick 只向当前任务计「自对方核上次 tick 以来的增量」——slice 约半速且随交错不确定燃烧 = 单核不可能交错序列，§3.4 红线违例；母案 §1.2 点名项随本刀收账，原 E1 行提前）。

> **初稿「遥测双环 per-CPU 化」前置件撤销记录**：其必要性论证（「切走/唤醒记录点大多在 KERN_LOCK 之外」）与 §3.3 舞蹈表直接矛盾——note_switch 五点（:295/:382/:830/:998/:1206）与 note_wake 两点（:857/:1027）分别随 wake_matching/tick/切换原语全在锁窗内，BKL 串行下单环即正确（负延迟/vct 单调断言安全：CNTVCT 两核同源全局单调）；唯一锁外读者 = watchdog 环走查，收进 KERN_LOCK 窗（F37 处置升级）。per-CPU 双环归 S-3（母案 E1 原位——拆锁后的并行化前提，非本刀面）。

以上两项均为「E0 结构性必须的最小 per-CPU/原子面」，不是 runqueue 拆分——刀面归属在此裁定记录，S-3 不重复做。

**非目标**：G2 过关宣称（S-2 只是通往 G2 的中间刀——测试策略.md:23 的「负载均衡对/shootdown 对」是 S-3/S-4 验收项）；RX/FAT/持久化/B4 相进 smoke-smp（本刀验收面 = 裸相 + rootfs 相两相，母案 §4 S-2 行原文——侦察⑤ E6）；FAST ok 时序基线复制到 -smp 2（母案 §6：BKL 让跨核唤醒变慢，FAST 只锚单核相）。

## §1 共享面盘点总表（全草案地基）

五路侦察合并去重逐项处置，共 54 项。处置档六类：**KERN_LOCK 锁内**（进 BKL 临界区）/ **打印锁**（PRINT_LOCK）/ **per-CPU 化**（本核独占存储消除竞争）/ **原子化**（单点读写的计数/标志改 Atomic）/ **boot 期单核独占维持**（窗口结构性单核零改动）/ **钉核维持**（生产者消费者同核，同核屏蔽窗继续有效）。

### A. 调度器核心 statics 与队列（sched/src/sched.rs）

| # | 项 | 位置 | 现访问纪律 | 处置档 |
|---|----|------|-----------|--------|
| A1 | `IRQ_TICKS` | sched.rs:20，tick 写 :1188 | 单核 IRQ 屏蔽 | **原子化**（AtomicU64 全局单份——写点在 tick 锁窗内 BKL 串行零竞争，原子化执法对象 = 锁外读者撕裂读 + static mut 别名 UB；per-CPU 化否决：BKL 下无并行增量可丢，前置件 1） |
| A2 | `SWITCHES` | sched.rs:23，tick 写 :1204（裸 `wrapping_add`）；watchdog 读 + `switches=` 行 :1213-1218 | 同上 | **原子化**（同 A1）；`switches=` 打印 = 任核 tick 命中 100 倍数即整行持 PRINT_LOCK（irqsave）拼行打印——全局原子计数恰一核打印每个 100 倍数、值全局单调 → smoke awk 严格递增口径零漂移（论证 §5.2「金丝雀打印口径」）；口径注记 = 现值只计 tick 抢占型切换 |
| A3 | `CURRENT` | sched.rs:26，五原语 pick/activate 段写（:282-294 / :368-380 / :816-828 / :986-998 / :1195-1203） | 同上 | 写者全进锁窗（§3.3 舞蹈表）；**S-2b 起改 per-CPU 访问器**（TPIDR_EL1 两核各自 current——副核参与硬前提；全局 static 拆除） |
| A4 | `RUNQUEUE` | sched.rs:28；spawn/enqueue/dequeue/pick/tick 全路径 | 同上 | **KERN_LOCK 锁内**（全局单队维持 = E0「内核串行」本体） |
| A5 | `SLEEPQ` | block :811 / wake :876-897 / expire :1015-1031 / unlink :762（非幂等） | 「IRQ 屏蔽窗内在队」契约 | **KERN_LOCK 锁内**；unlink 在队契约改「锁内」等价表述（铁律 B7/B8 的 E0 形态） |
| A6 | `REAPQ` | push :415（cap32 溢出 loud :421）/ drain :433（yield_now :303、idle_loop :1150 消费） | 屏蔽窗 | push 锁内；**drain 触共享堆分配器 → 锁内**（铁则：E0 分配器无锁，一切 alloc/free 锁内——D23） |
| A7 | `IDLE_TASK` | sched.rs:37（主核 rust_main 活上下文） | init-once | **boot 期单核独占维持**；副核 idle 用独立形态（§2.3，不立第二个 IDLE_TASK） |
| A8 | `integrity_check` | sched.rs:1105-1134，tick 每 100 切换 | 只读断言 | tick 锁窗内执行（他核中间态撕裂读 = 假阳性 panic 防护；断言语义不变） |
| A9 | `expire_timeouts` | sched.rs:1015-1031 | IRQ 语境 | tick 锁窗内（§2.2 tick 协议） |
| A10 | `kill_all_tasks_by_pinned` | sched.rs:483-557 | SVC，零切换零让出 | **KERN_LOCK 锁内全程**（「零阻塞零让出」设计在 E0 的红利：无需中途放锁） |
| A11 | `LAST_VCT` | class.rs:29（RR.tick 读写） | 单核 IRQ 串行 | **per-CPU 化**（SmpCpu 单字段，前置件 2——双 tick 结构性必须：共享全局时每核 tick 只计「自对方核上次 tick 的增量」，slice 约半速不确定燃烧 = 单核不可能交错，§3.4 红线违例、母案 §1.2 点名「slice 记账错乱」；本核只写本核块） |
| A12 | `USER_KILLS`/`LAST_KILLED` | sched.rs:325/:329（kill 链写 :339-340、读者 :1089/:1095） | 单核 | **原子化**（AtomicU64/AtomicPtr，前置件 1——写点全在 kill 链锁窗（A10/:383 收口），读者 watchdog 忙等锁外 → 原子读封撕裂；本行即初稿悬空「E2」引用的归属落位） |

### B. 切换原语与唤醒面（sched.rs；切走点全仓恰 5 处）

| # | 项 | 位置 | 现访问纪律 | 处置档 |
|---|----|------|-----------|--------|
| B11 | `context_switch` 5 调用点 | :296（yield_now）/ :384（switch_away_forever_with，不归）/ :835（block_current）/ :1000（try_preempt）/ :1222（on_irq_tick） | 「永不在 PSTATE.I=0 下到达」（task.rs:108-139 纯叶子 asm） | 放/取舞蹈集中 5 原语实现（§3.3 表——**该表即 S-3 拆锁取放点地图，直接移交**） |
| B12 | `note_switch`→`on_switch` | sched.rs:597-605 → telemetry.rs:77-91；五调用点（:295/:382/:830/:998/:1206） | 静态环零分配 | **单环维持**：五调用点全在锁窗内（§3.3——yield/block/preempt/:383/tick 均随切换原语锁窗）→ BKL 串行免竞争；per-CPU 双环撤销（前置件撤销记录）、归 S-3 母案 E1 原位 |
| B13 | `note_wake`→`on_wake` | sched.rs:627-634 → telemetry.rs:198-240（负延迟 panic :218-223） | 同上 | **单环维持**（同 B12——:857/:1027 两写点分别随 wake_matching/tick 锁窗，BKL 串行下负延迟 panic/vct 单调断言安全：CNTVCT 两核同源全局单调）；锁外读者收口见 F37 |
| B14 | `wake_matching` | sched.rs:875-899 | 三方调用：SVC / IRQ（rx_irq_notify main.rs:1799）/ 内核任务直调（task_waker main.rs:1502） | **KERN_LOCK 锁内**（三方全收口：SVC 入口锁、IRQ 经 §2.2 tick/RX 协议、直调包络升级 F31） |
| B15 | `find_single_waiter`/`fast_wake` | sched.rs:909-948 | fastpath，指针「同屏蔽窗有效」 | 锁内免费；契约注释改「同锁窗有效」（§3.4） |
| B16 | `for_each_sleeper` | sched.rs:1039-1049；pi_probe 直调 main.rs:1661-1673 + pi.rs recompute 内部 | 「IRQ 屏蔽窗内调用」 | 包络升级 KERN_LOCK（铁律 B8：屏蔽本核挡不住他核写者） |
| B17 | `SleepQueue insert_sorted/unlink` | :706/:762 | 经由 A5 各点 | 锁内（随 A5） |
| B18 | `note_sleepq_changed`→`pi::recompute` | 发射点 :833/:897/:946/:1030/:510；pi.rs:57（三重遍历 O(n²)） | 屏蔽窗 | 锁内；**持锁时长放大器记账**（§6 R1——v1 n≈个位数可忍，E1 拆细优先项） |
| B19 | `idle_loop` | sched.rs:1145-1169（空判 `rq().is_empty() && sleepq().len==0` :1152、reap_drain :1150、mask+wfi :1158-1159） | 主核专属 | reap_drain 锁内（A6）；**S-2b 判空入 KERN_LOCK 窗 + park 改形**（§2.3「主核 idle」行——wfi 前锁内复查空判 + 恒 arm_slice 武装探测，跨核入队 ≤1 slice 被本核 tick 发现不丢）；gate 关（S-2a/单核）维持现 tickless mask+wfi 形态零漂移；副核 idle 形态另立（§2.3） |

### C. 对象系与能力面

| # | 项 | 位置 | 现访问纪律 | 处置档 |
|---|----|------|-----------|--------|
| C20 | HandleTable（BOOTSTRAP_HT/私有表/进程表）、Process/Thread/VMO/Vmar/Channel | kernel/src/syscall.rs:137-160 分表 + 各对象模块 | SVC 主体串行 | **KERN_LOCK 锁内免费**（消费者全经 SVC/boot 窗；init 段归 boot 单核维持） |
| C21 | `Event.signals` 非 atomic RMW | event.rs（signal/dup/close 路径） | 单写者假设 | **维持非原子 + 纪律注释**：跨核写者全收口后成立（rx_irq_notify 经锁窗 E27、task_waker 包络升级 F31、SVC 天然锁内）——注释立法「新增非 SVC 写者禁止」；保守 AtomicU64 备选挂 S-3（成本极低，非本刀必需） |
| C22 | `ObjRef` retain/release、对象 Box 生命周期 | handle.rs 等 | SVC | 锁内免费（4-1 睡眠钉住纪律不变——retain 在锁内、睡眠放锁后引用仍钉住） |

### D. 分配器与内存面

| # | 项 | 位置 | 现访问纪律 | 处置档 |
|---|----|------|-----------|--------|
| D23 | heap/buddy/slab 全局单例 | mm/、kernel/src/heap.rs | 单核串行 | **KERN_LOCK 锁内**（E0 分配器无独立锁——一切 alloc/free 锁内铁则；消费者 = SVC 主体/spawn 包络/REAPQ drain，三者全在锁窗） |
| D24 | `frames().allocated_pages` 水位读 | mm/buddy.rs；读者 soakmon（main.rs:2904 附近）+ churn 金丝雀 | 单核 | **原子化**（AtomicUsize 水位——侦察③ B6：他核分配并发下撕裂读 = SOAK `pages∈[首拍,首拍+64]` 断言假红） |
| D25 | `frames() -> &'static mut FrameAllocator` 别名 UB | kernel/src/mm.rs:152（母案 §1.2 点名项——「SMP 后整体重构」） | 单核特赦（别名唯一） | **锁纪律兜底 + 挂账 E1+**：S-2 一切访问者（SVC 主体/spawn 包络/REAPQ drain/bench）均在 KERN_LOCK 窗内 = 跨核互斥由锁保证、别名 UB 的 UB 面不被触发；类型面真修（受赠者改 `&FrameAllocator` 只读或经锁封装）挂 E1 细粒度拆锁时随分配器重构——本刀不动签名（改签名牵动全调用面，非 BKL 刀面） |

### E. IRQ 面

| # | 项 | 位置 | 现访问纪律 | 处置档 |
|---|----|------|-----------|--------|
| E25 | `on_irq_tick` 主体 | sched.rs:1186-1226 | 单核 IRQ | **拆分处置**：计数原子化（A1/A2，全局单份锁窗内 RMW）；expire/pick/CURRENT/integrity/switch = **tick 锁窗**（§2.2 协议；LAST_VCT per-CPU——A11）；`switches=` 打印走打印锁拼行（G43/G44）；:1222 切走前放锁、不重取（被打断者不持锁） |
| E26 | `rx_irq`/RX_RING | uart.rs:115-128 / rx_irq :163-187 | 单消费者 | **钉核维持**：IRQ33 经 SPI_ROUTES 钉 CPU0（gic.rs:163-165，target=0x01）+ console 任务钉核 0——**从隐性事实升为显性契约**（§2.3 副核过滤 + 注释立法「任何新增 RX 环读者必须同核」） |
| E27 | `rx_irq_notify` | main.rs:1791-1801（Event.signal + wake_matching :1799） | IRQ 语境 | **KERN_LOCK 短窗**：IRQ33 只投 CPU0，而 CPU0 持锁期本核 IRQ 不可达（入口 DAIF 屏蔽 + tick 窗内 PSTATE.I=1）→ **同核自旋结构性不可能**；跨核等待有界（持锁者永不睡，§3）；wake_matching 入锁窗后 Event.signals 维持非原子（C21） |
| E28 | GIC banked GICC / GICD | gic.rs:60-111 | 主核 init 一次 | **维持**（GICC 天然 per-CPU；GICD 全局只 boot 期碰——副核新中断源走 banked ISENABLER0 自己的 bank，绝不碰 GICD 全局面，S-1 纪律 smp.rs:225-239 延续） |
| E29 | timer CNTV/CVAL/CNTVCT | timer.rs:66-122 | 主核 arm | **维持**（CNTV/CVAL 系统寄存器天然 per-CPU；CNTVCT 两核同源只读——遥测跨核 vct 戳可比；副核自 arm_slice + banked PPI27 使能，§2.3） |
| E30 | 未知源分支 `UNKNOWN_SEEN` | trap.rs:428-440 | 已 AtomicU32 | **维持（已原子）**；首 4 loud 打印走打印锁 |

### F. 内核任务直调面（「入口取锁」的覆盖洞——侦察① §6 W1-W6 全清）

| # | 项 | 位置 | 现访问纪律 | 处置档 |
|---|----|------|-----------|--------|
| F31 | task_sleeper/task_waker 包络 | main.rs:1459-1510（disable_irq→block/wake→enable_irq） | 同核屏蔽窗 | **包络升级**为统一 `klock_enter/klock_exit`（§2.3；BLOCK ok 验收语义不变） |
| F32 | intent sinks 永睡 | main.rs:2128/:2148/:2200/:2235（block_current(0,0,u64::MAX)） | 同上 | 包络升级（睡前一次性放锁，进锁窗只为 enqueue 本身） |
| F33 | rt_probe / soakmon | main.rs:2796-2801 / :2904 | 同上 | 包络升级 + **顺序例外**：醒点先读 wake_vct（:2800，BENCHRT 零噪声契约）再重取锁 |
| F34 | ai_bench_round 运行期 spawn 包络 | main.rs:2596-2601 | 同上 | 包络升级（spawn 摸分配器 + RUNQUEUE 链接 → E0 锁窗必备，防「runqueue 半链接被 tick 走查」） |
| F35 | console RX 消费包络 | main.rs:1943-1947 | 同核单消费者 | **钉核维持**（E26 契约面；console 的 wait/recv/send 全走 SVC 天然锁内） |
| F36 | pi_probe 扫描包络 | main.rs:1661-1673 | 同上 | 包络升级（B16） |
| F37 | watchdog（唯一非 SVC 忙读者） | user.rs:1260-1339 | 忙等读 static mut | **读侧收口**：包络升级 `klock_enter/klock_exit` 短窗（环走查 check/wake_check/wake_hist 与 USER_KILLS/SWITCHES/遥测 total 快照同窗——B12/B13 单环 + BKL 写者串行 = 断言面与单核同构）+ 计数快照走原子读（A1/A2/A12/I50——锁外残余读者只剩原子面）；忙等空转在窗外；打印走打印锁 |
| F38 | 各内核任务尾 exit_current 直调（~15 处） | main.rs:1484/:1509/:1570/:1632/:1682/:1728/:2092/:2850 等 | 不持锁 | **零改动**：放锁统一在 :383 单点 + Task 位分诊（§2.3——非持锁者放锁幂等跳过） |
| F39 | task_intent_probe 扫描包络 | main.rs:2261-2289（for_each_sleeper 断言 :2277 + INTENT_* 计数读） | 同核屏蔽窗 | **包络升级 klock**（B16 同族——for_each_sleeper「IRQ 屏蔽窗」契约双核升格锁窗）；计数读走原子（I50） |
| F40 | task_ai_probe 扫描包络 | main.rs:2712-2725（64 轮 for_each_sleeper 循环——与 CPU1 tick 的 expire_timeouts 同扫 SLEEPQ） | 同上 | **包络升级 klock**（B16——S-2b 副核真跑 tick 后「本核屏蔽挡不住他核走查」即活面） |
| F41 | ai_demo policy::bind/unbind | main.rs:2653-2669（写 policy.rs:45 `static mut TABLE`） | 同核屏蔽窗 | **包络升级 klock**：TABLE 写与 CPU1 tick 锁窗内 pick_next→policy::apply（policy.rs:81 读）构成跨核竞速——写点入锁窗即 BKL 串行 |
| F42 | rt_probe BENCHRT 打印 | main.rs:2830-2842（七调用拼行） | 同核屏蔽窗（disable_irq 包络只挡本核） | 打印走 PRINT_LOCK 拼行 API（G43——整行持锁 + irqsave；CPU1 的 tick `switches=` 行同锁不同撕——裸多调用在双核下与他核行交错撕行） |

### G. 打印面

| # | 项 | 位置 | 现访问纪律 | 处置档 |
|---|----|------|-----------|--------|
| G43 | uart_putc/puts/hex64/put_dec 底座 | uart.rs:68-78（MMIO 逐字节直写） | 单写者 | **打印锁（PRINT_LOCK）**：TX 自旋锁、粒度 = **整条消息**（行完整性）、const-init（boot 最早打印前可用）、嵌套次序 **KERN→PRINT 单向**（持 KERN_LOCK 者可打印，持 PRINT_LOCK 者禁再取 KERN_LOCK）、**PANIC bypass**（panic 路径绕锁直写 + 关中断一次性防重入——panic 于持锁中不死锁；SMP halt 只停本核 v1 接受，IPI 停核挂 v2）。**irqsave 纪律（审查实抓的死锁面）**：任务态取锁前 save DAIF.I + disable、放锁后 restore（Linux `spin_lock_irqsave` 同构）；IRQ 上下文天然 PSTATE.I=1 直接 acquire——反例死锁形态 = 任务态**裸持** PRINT_LOCK 打印者被本核 tick 打断 → on_irq_tick 的 `switches=` 行（IRQ 上下文）自旋于被中断者持有的锁 → **同核硬死锁**（IRQ 上下文不可抢占、持锁者冻结直至 tick 返回——间歇性、全系统冻结面）。**拼行 API**：`with_print_lock(closure)` 唯一入口形态——多调用整行（BENCHRT 七调 main.rs:2830-2842、BENCHWAKE 十二调 user.rs:1301-1319、AIDEMO 行 main.rs:2685-2691、矩阵任务行、TERM kill 行 syscall.rs:1228-1245）一律经 closure 整行原子 + irqsave 一次成对（裸 TTAS 无此形态即埋同核死锁 + 双核撕行两个雷） |
| G44 | IRQ/异常上下文打印者 | tick `switches=` 行 sched.rs:1213-1218 / 未知源 loud trap.rs:428-440 / dump 族 trap.rs:344-382 / TERM kill 行 syscall.rs:1228-1245 | 屏蔽窗 | 打印锁内可同锁论证——临界区 = TX FIFO 就绪轮询，不睡眠、长度有界（FIFO drain 速率界）；**同核安全性来源分层**（审查修正：不可笼统说「持锁期本核 IRQ 不可达」——那只对 IRQ 态持有者成立）：IRQ 上下文持有者天然本核 IRQ 不可达（入口 PSTATE.I=1）；**任务态持有者靠 G43 irqsave 纪律**封死「持锁期被本核 tick 打断」窗；KERN_LOCK 持有者（SVC 入口硬件屏蔽 / klock_enter 屏蔽）同理本核不可打断；跨核自旋等对方 drain 有界（持锁者永不睡 §3 + 临界区有界）；console drain 的 256B 段按「一整段一条消息」粒度入锁 |

### H. boot 窗口（结构性单核，零改动）

| # | 项 | 位置 | 现访问纪律 | 处置档 |
|---|----|------|-----------|--------|
| H45 | ktest 全模块（svc4/svc6 含） | kernel/src/ktest.rs（run @ main.rs:451，sched::init :473 前、bringup :1382 前） | 单线程 IRQ 屏蔽 | **boot 期单核独占维持**（§5.3）；EL1 真 SVC 走同一 arch dispatch 入口 → 锁天然覆盖且零竞争 |
| H46 | bench.rs 全模块 | kernel/src/bench.rs（ktest 后、sched::init 前） | 同上 | 同上 |
| H47 | smp_bringup + 邮箱原子族 | smp.rs:328-425；HB1/HB2/RELEASE/SEC_SPINS AtomicU64 | boot DAIF 尾窗 | **维持（已原子）**；门铃序 store→dsb sy→sev（smp.rs:408-418）= S-2 唯一保留 dsb 场景 |
| H48 | 回调注册槽/USER_SIGNAL_BITS/SYSCALLS 表 | main.rs:1365-1375 / :2457；arch syscall.rs:31 | init-once 后只读 | **维持（静态只读）** |

### I. 已安全面（零改动记录）

| # | 项 | 位置 | 现访问纪律 | 处置档 |
|---|----|------|-----------|--------|
| I49 | 已原子族：A_COUNT/B_COUNT/CONSOLE_DRAINED/BLOCK_BOOT_DONE/UNKNOWN_SEEN/AI_BOUND/AI_RT_LAST/SMP 邮箱族 | main.rs:1399/:1415/:1735/:1446/:2498/:2502、trap.rs、smp.rs:100-111 | Atomic | **维持** |
| I50 | telemetry intent 三计数 + IPC fastpath 三计数 | telemetry.rs:371-458；写点全在 syscall 语境 | SVC | 写侧锁内免费；**读侧（watchdog/BENCHWAKE/fast_pinger 断言收割）AtomicU64 化一劳永逸**（E 系拆锁不返工） |
| I51 | 用户态服务（guifbd/svcmgr/bsh…） | user/ | 经 IPC | 天然串行（E0 锁内单序消费），无内核侧处置 |

## §2 KERN_LOCK 设计

### 2.1 原语：TTAS 自旋锁（cortex-a72 非 LSE → LDXR/STXR）

**形态**（新文件 `arch/aarch64/src/kern_lock.rs`，TTAS = test-and-test-and-set）：

```
locked: AtomicBool（const 初始化 = false）
acquire(): loop { 若 compare_exchange(false,true,Acquire,Relaxed) 成功则返回
                  否则 while load(Relaxed)==true { spin_loop() } 再试 }   // 等释放期间只本地 load
release(): store(false, Release)
```

- **屏障论证——不手写 dmb/dsb**：`Acquire`/`Release` 序在 aarch64 映射为 LL/SC 的 LDAXR/STLXR（带 acquire/release 语义的独占访问），临界区与锁外访问的序由这两个序语义保证——手写 dmb 属重复发明。cortex-a72 无 LSE（ARMv8.0——outline-atomics 先例），`compare_exchange` 走 LL/SC 循环正确；内层 `load(Relaxed)` 只探测释放、不参与序。**dsb 唯一保留场景** = S-1 门铃序（store→dsb sy→sev，smp.rs:408-418）——KERN_LOCK 不经邮箱传递，acquire 本身即同步点，无需 dsb。
- **TAS vs TTAS vs wfe 裁决**：TAS（每轮 compare_exchange）= 争用时 LL/SC 对失败重试风暴、TCG 下每轮走原子慢路径；**TTAS** = 等 holder 释放期间只本地缓存行自旋 + `spin_loop()` hint，释放时一次 Acquire CAS——**选 TTAS**。**wfe/sev 否决**：TCG 的 `wfe` = 立即返回的 hint（S-1 spins 疯涨、拖主核 ~40% 吞吐实测——CLAUDE.md Gotchas），wfe 环在 QEMU 上退化为忙转还多一层跳转；真机 wfe 需 DSB 门铃序配套。按「少发明原语」（测试策略.md:35-38 纪律③）E0 用纯 TTAS——若出现长自旋即 E0 违约信号（临界区 = syscall 主体本该短，§6 R1 观测面）。
- **const-init**：`static` + `AtomicBool::new(false)` 常量初始化——无 init 调用，boot 最早路径（含 ktest/bench 窗）即可用；单核非竞争路径 = 每次 SVC 一对原子操作的开销。
- **放置与依赖方向**：原语落 `arch/`（hal）——sched 与 kernel 都可引用的最底层。但 **sched 是架构无关 crate**（CLAUDE.md「架构无关优先」）不直接引 arch 符号：sched 侧切换原语的放/取舞蹈经**函数指针槽**注入（`sched::set_klock_ops`——EXIT_HOOK sched.rs:361 / irq_dispatch main.rs:1367 同款先例），kernel main 在 ktest 前安装；槽空 = 直通零开销（null-safe，覆盖 install 前窗口）。打印锁同文件伴生（独立实例 + panic bypass 通道），调用面直引（kernel/主核侧）。

### 2.2 取放点全表（E0 边界①总览；逐行锚见 §3.1）

| 入口/语境 | 取锁点 | 放锁点 |
|-----------|--------|--------|
| SVC（EL0/EL1 两路汇流） | arch/aarch64/src/syscall.rs:47-58 dispatch 内、handler 调用前——**单点覆盖两向量入口**（trap.rs:275/:323 EC 0x15 臂零改动） | dispatch 返回后、`regs[0]` 写回（:56）前后——栈写不触共享结构 |
| `-> !` 退出（0x33/0x36） | 同上（进入时取） | **sched.rs:383 单点**（EXIT_HOOK 之后、context_switch :384 之前；handler 尾 = 不可达死码勿补放——Plan B v0.5） |
| EL0 致命 fault kill 链 | trap.rs exception_sync_el0 fault 臂（:328-341）入口、dump 前——与 SVC 同构（硬件 PSTATE.I=1、不睡眠） | 同 :383（共用尾） |
| IRQ tick | on_irq_tick 入口（:1186） | :1221（switch :1222 前）；**切走后不重取**（被打断者可被抢占 = 不在屏蔽窗 = 不持锁） |
| IRQ RX（rx_irq_notify） | notify 入口（:1791）——仅 wake_matching 段 | notify 尾 |
| 内核任务直调点（F31-F36/F39-F42） | `klock_enter()`（= 屏蔽 IRQ + 取锁 + 置 Task 位，一次完成） | `klock_exit()` 对称 |

**对母案 §2.1 边界 1 的显式对表（审查 discipline 面）**：母案给 IRQ 上下文两分支——「同锁」或「先收进 per-CPU-only 操作」——并**倾向后者**（「IRQ 侧拿锁 = 优先级反转面，v1 无优先级继承的自旋锁别进 IRQ」）。本稿取**同锁**分支、弃其偏好，理由成文：tick 主体不进锁则副核无法 pick（E0 无 IPI，副核调度全靠本核 tick 驱动——runqueue 手术必须与主核对同一把锁串行）；RX 的 wake_matching 不进锁则丢唤醒（Event 置位与 SLEEPQ 摘队需要原子序，per-CPU-only 无处安放跨核唤醒）；母案警告的 IRQ 侧优先级反转面经「持锁者永不睡（§3——睡眠点放锁）+ 同核自旋结构性不可能（E27——KERN_LOCK 任务态持有者必在 IRQ 屏蔽窗内）」封死为**有界跨核自旋**（R1/R3 观测兜底）。显式声明此偏离及理由——母案边界条款不由此推翻，S-3 拆锁时按同表重裁。

### 2.3 内核任务语境与副核参与的加锁面

**双入口汇流（S-2 最难点——侦察② §0 结论 4）**：`block_current` 同时被 SVC 路径（syscall.rs:370/:1809/:1941，持锁）与内核任务直调（main.rs 八处，不持锁）调进同一函数——非重入自旋锁在函数内「无脑取锁」= 自死锁。**裁决：Task 携带 `klock_window: bool` 位**：

- 语义 = 「本任务当前处于持锁义务窗」**随任务走**（不放 per-CPU）：新任务 trampoline 起步 IRQ 未屏、天然无锁（task.rs:141-158，:154 `msr daifclr,#2`——侦察① §2 已核，无需出生路径补锁）；wait 多轮睡眠的重复放/取靠随 Task 位不错配（侦察② #34 指出的 R5 面）。
- 置位/清位：arch dispatch 取锁时置、放锁清；`-> !` :383 放锁清；`klock_enter/exit` 对称。
- sched 切换原语内分诊（经 §2.1 槽，5 原语集中实现）：切前「位=1 则 release」、切回「位=1 则 reacquire」——**~31 个 syscall handler 零改动**；直调睡者在包络内置位 → 切前放/醒后取同样正确（**悬垂持锁者睡去 = 跨核死锁面结构性封死**）；非持锁者（F38 任务尾、ktest 手搭）位=0 → 放锁幂等跳过。
- **方案 B（KERN_LOCK + SCHED_RAW 双锁）否决理由**：两锁单向嵌套 = 第二套原语发明（违纪律③）；SCHED_RAW 只盖队列手术、不盖 wait 重验的 signal_state 对象读——还得把语义面加回锁 = 分层收益为负；方案 A 的 sched「长出锁知识」经槽注入保持在位分层（EXIT_HOOK 同款张力先例）。

**直调包络统一升级**：F31-F42 各直调点的 `disable_irq→…→enable_irq` 机械替换为 `klock_enter()/klock_exit()`（锁窗内 IRQ 本就屏蔽，形态天然兼容）；rt_probe 例外顺序「切回 → 读 wake_vct → 重取锁」写进包络注释（F33/F42 分工：F42 打印改拼行 API、顺序例外仍归 wake_vct 读点）。

**副核参与（侦察④：今天 park-only——`.Lsmp_final: wfi` smp.rs:275-277 无条件死循环，无调度器参与/无工作邮箱/无定时器；S-2 最小面）**：

| 项 | 裁决 |
|----|------|
| 参与门（gate） | `SMP_PAR: AtomicBool`——S-2b 由主核在 idle_loop 入口（main.rs:1389）前置位；**S-2a 不置位 = 副核维持 park**——gate 即 S-2a/S-2b 拆步天然缝（§4） |
| 副核跑什么 | pick 过滤器：**只跑 `Task::pinned != 0`（用户进程成员）**；内核任务（pinned==0）全钉 CPU0——「内核串行/用户并行」的字面兑现 + CPU0 内核任务互相之间由同核屏蔽窗继续覆盖（第三类写者洞的另一半不洞）；零新字段（pinned 已在，kill_all_tasks_by_pinned/terminate 消费同源）；console 钉核 0 从隐性升显性（E26 契约） |
| CURRENT | per-CPU 访问器（TPIDR_EL1；CPU0 补块 = 前置件 2）；RUNQUEUE 全局单队 + KERN_LOCK 维持；**空态契约**（审查补——副核首派发前 current 为空的窗口）：访问器返空哨兵、tick 入口判空则跳过 RR.tick 的 slice 扣减与 note_switch（无当前任务可记账）、仍走 expire/pick；pick 过滤器命中 pinned!=0 即首派发——**首派发上下文 = SmpCpu 内副核私有 park ctx**（非调度语义 Task：不进 RUNQUEUE、无 TCB、仅 TaskContext 存档），派发真任务后 CURRENT 转真值——不立第二个 IDLE_TASK（A7 口径延续） |
| 入口改板 | `.Lsmp_final` wfi 死停 → **指针字 + blr** 调高半 `secondary_main`（链接脚本 `QUAD` 先例 linker-aarch64.ld:45-50——「高半 Rust 直取低半符号 = ADRP 断链第三形态」预防）；VBAR 同表（smp.rs:175-178）→ SVC 入口锁对副核自动生效 |
| 副核 tick | banked PPI27 使能（GICD_ISENABLER0 副核 bank，不碰 GICD 全局面——S-1 纪律）+ 本核 arm_slice；**LAST_VCT per-CPU**（A11——双 tick 结构性必须，随前置件 2 的 SmpCpu 块播种）；**无 IPI 的 E0 代价**：跨核唤醒延迟 ≤1 slice（被唤任务在目标核下一次 tick/pick 才上核——§6 R2 记账，S-3 SGI 根治） |
| 门控唤醒（gate 探测环） | bring-up 尾**不落** `.Lsmp_final` 式裸 wfi（PPI27 未使能 + CVAL 未武装 → wfi 无中断源 = 睡死永不可达——park 路径只配了 GICC CTLR/PMR；TCG wfi 真睡铁律）；secondary_main 起手**使能 banked PPI27 + 本核 arm_slice** 后进 `loop { gate.load(Acquire)==true → break; wfi }` 探测环——周期 tick 天然唤醒探测，**轮询免疫丢唤醒**（无 store-release/sev 门铃需求、无 dsb）；gate 未置位期间的 tick = on_irq_tick 入口按本核参与态**早退**（只 rearm + EOIR，零共享触达——S-2a 单写者纪律保持）；gate 置位 = 主核 store(Release)，副核下次 tick 醒来入正式参与 |
| 副核 idle | wfi + 周期 tick 探测循环（TCG wfi 真睡铁律；wfe 立返禁用于长驻——S-1 实测）；不立第二个 IDLE_TASK |
| 主核 idle（S-2b 改形） | gate 置位后 CPU0 idle_loop 放弃「mask CNTV + wfi」tickless park：判空/回收入 KERN_LOCK 窗（klock_enter → reap_drain + 空判 → klock_exit），空则**恒 arm_slice 武装探测态 + wfi**——CPU1 入队的 CPU0-bound 内核任务（pinned==0 全钉主核）最迟 1 slice 被本核下次 tick 发现（无 IPI 的 E0 代价，与 R2 同界；RX IRQ33 虽钉 CPU0 可作兼带唤醒源但不可依赖）；gate 关（S-2a/单核）保持现形态**零漂移**——tickless idle 恢复随 S-3 IPI |
| DAIF 初态 | 副核进调度循环前显式 `msr daifset` 钉板（侦察④ 缺口 9——EL1 直达路径 PSCI 入口态未实测记录） |
| rt_probe | 任务本体 pinned==0 钉 CPU0；CNTVCT 两核同源只读无竞争（E29）→ oversleep 采样不受扰，BENCHRT 基线不漂 |

## §3 三边界逐点清算

### 3.1 边界①——取放点逐锚

| # | 锚 | 动作 |
|---|-----|------|
| 3.1-1 | arch/aarch64/src/syscall.rs:47-58（nr 界检 :50、handler :52、写回 :56） | :52 前 acquire + 置 Task 位；:56 前后 release + 清位——EL0（trap.rs:325）与 EL1（:277）两入口单点覆盖 |
| 3.1-2 | trap.rs exception_sync_el0 fault 臂 :328-341 | dump（:329-331）前 acquire；kill 链走 :383 收口（dump 打印 = KERN→PRINT 单向嵌套） |
| 3.1-3 | sched.rs on_irq_tick :1186 入口 | acquire；:1221 release；:1222 切走后不重取（被打断者不持锁） |
| 3.1-4 | main.rs rx_irq_notify :1791 | wake_matching 段（:1799）acquire/release 短窗（同核自旋不可能性论证 = E27） |
| 3.1-5 | F31-F42 直调点 | `klock_enter/klock_exit`（§2.3） |

### 3.2 边界②——`-> !` 切换前放锁路径全列

| # | 路径 | 入口 | 放锁点 |
|---|------|------|--------|
| 3.2-1 | `process_exit`(0x33) | syscall.rs:1115-1134 → kill_all(:1131) → exit_current_with(:1133) | **sched.rs:383**：EXIT_HOOK/task_exit_hook（:361-365，syscall.rs:1080-1113——wake_matching/ht.drain 必须在锁内）+ dequeue/reapq_push/pick/arm_slice/CURRENT/note_switch（:366-382）全在锁内；放锁后仅剩 context_switch 纯 asm 不触共享 |
| 3.2-2 | `thread_exit`(0x36) | syscall.rs:1136-1143 | 同 :383 |
| 3.2-3 | EL0 fault kill 链 | trap.rs:336-339 → user_fault_exit（sched.rs:337-347） | 同 :383（USER_KILLS/LAST_KILLED 写在锁内、随 A12 原子化；dump 打印走打印锁） |
| 3.2-4 | 内核任务直调 exit_current（~15 处） | main.rs 各任务尾 | 同 :383（Task 位=0 幂等跳过；直调点零改动 F38） |
| — | `process_terminate`(0x39) 返回型 | kill_all 后正常返回 | **不经 :383**——锁内跑完杀链、dispatch 尾正常放（不误伤） |
| — | halt 面（unexpected/fatal） | trap.rs:374-382 等 | 机器已停不放（全核冻结面非 S-2 对象） |

**handler 尾补放 = 不可达死码**（`-> !` 经 context_switch 切走不归，arch/syscall.rs:52-56 写回结构性不可达——Plan B v0.5）：放锁只落 :383 一处。

### 3.3 边界③——睡眠点放锁/醒后重取清单（逐 file:line）

**SVC 层睡点（kernel/src/syscall.rs）**：

| # | 睡点 | 锚 | 舞蹈 |
|---|------|-----|------|
| 3.3-1 | sys_thread_sleep | :362-373（block_current :370） | :369 前 release / :371 后 reacquire（醒后无共享读，重取仅为 dispatch 尾放锁配平）——或统一收进 sched 分诊 |
| 3.3-2 | sys_object_wait_one | :1758-1815（retain :1785 / 重验 :1789 / block :1809 / release :1813 唯一出口） | 每轮 :1808 release / :1810 reacquire；**重验 signal_state 读共享对象必须在重取后的锁内**；retain/release（睡眠钉住）在锁内 |
| 3.3-3 | sys_object_wait_many | :1837-1948（retain 全体 :1900-1903 / block :1941） | :1940 release / :1942 reacquire；WAIT_ANY 醒后 idx 复扫在锁内 |
| 3.3-4 | fastpath 中途切走 | sys_channel_send :1460 → try_preempt | push+fast_wake（:1451/:1458）保持锁内（mutation 完整性）→ 切走放/切回取收进 try_preempt 原语 |

**切换原语舞蹈表（sched.rs——实现落点；直调睡点经 Task 位自动同构）**：

| 原语 | 切走锚 | 切前 release | 切回 reacquire | 备注 |
|------|--------|--------------|----------------|------|
| yield_now | :296 | :295 | :297 | reap_drain（:303）需再入锁窗（drain 摸分配器 D23）再放 |
| block_current | :835 | :834（note_sleepq_changed :833 之后） | :836 | 双入口按 Task 位分诊；wait 重验循环 = 每轮一对 |
| try_preempt | :1000 | :999 | :1001 | find_single_waiter/fast_wake 指针「同锁窗有效」（B15）；切回后遥测计数在重取后的锁内 |
| switch_away_forever_with | :384 | :383（note_switch :382 后） | **无（不归）** | 边界②收口点 |
| on_irq_tick | :1222 | :1221 | 无需 | 被打断者不持锁（§2.2） |

**直调睡点**（F31-F33）：包络置位 → block_current 内位分诊自动舞蹈；rt_probe 例外顺序「切回 → 读 wake_vct（:2800）→ 重取锁」。

### 3.4 语义等价精确口径

**E0 承诺 = 「双核行为等价于单核可能的任意交错序列」**：

- KERN_LOCK 把「两核并发进内核」归约为「单核串行进内核」——每个锁窗 = 一个**不可分割段**；睡眠点放锁 = 单核「让出 CPU」的同构（锁窗边界即让出点）。**不是**「逐字节串行」（锁窗内指令序、锁窗间交错序仍由调度决定），**也不是**「时序确定」（TCG 双核时序确定性更差——母案 §5）。
- 推论一：单核下一切成立的不变量（ktest 断言、integrity_check、fastpath 指针契约、PI 语义——母案 §1.2「BKL 下语义不变」）在 E0 双核下逐条保持；契约注释从「IRQ 屏蔽窗内有效」改「KERN_LOCK 锁窗内有效」（注释对齐面，侦察⑤ E4——ktest J/L 段随实施刀对齐，行为零变化）。
- 推论二：**放锁后零共享触达**是本口径的执法面——dispatch 尾收口点后仅剩 TrapFrame 栈写、:383 后仅剩 context_switch asm；后续任何「dispatch 尾打印/切换前遥测」即破不变量（§6 R8 注释立法）。

## §4 拆步（每步独立可验收）

| 步 | 内容 | 验收命令 | 预期 |
|----|------|----------|------|
| **S-2a 锁底座 + 单核零扰动 + 裸相** | kern_lock.rs（TTAS + PRINT_LOCK + panic bypass + irqsave/拼行 API G43/G44）+ arch dispatch 取放 + sched 槽 + Task 位 + 五切走点舞蹈（§3.3 表）+ `-> !` :383 + 直调包络升级（F31-F36/F39-F42）+ 前置件两件（计数/kill 计原子化 / CPU0 块 + LAST_VCT per-CPU）+ D24/I50 读侧原子化；**gate 不置位（副核维持 park）** | `just smoke`（八跑）+ `just smoke-smp`（S-1 形态不扩）+ `just test-host` + `just abi-check` + `just build-release`（release 链接守卫 + assert-flip——新增 arch 文件的 ADRP 面）+ `python3 tools/bench.py --icount` 双跑 | 主 smoke 八跑全绿**零漂移**（锁 = 单核非竞争纯开销；BENCH 3x 宽松因子门禁吸收——首跑盯）；smoke-smp 裸相 26 + `SMP ok ncores=2` 照旧（副核 park、锁在单核跑）；icount 双跑同值后 `--update` 基线刷新 diff 随源入册（S-1 先例——锁原子序进 SVC 入口 = 每 syscall 指令数变，L2 门禁必动） |
| **S-2b 副核参与 + rootfs 相 + 并发金丝雀** | gate 置位 + secondary_main（指针字 + blr + 门控唤醒探测环 §2.3）+ PPI27/副核 arm_slice + pick 过滤（pinned!=0）+ CURRENT per-CPU 访问器（空态契约）+ 主核 idle 改形（§2.3）+ PAR 金丝雀（§5.2）+ smoke-smp rootfs 相 harness（§5.1）+ §5.4 帽 ×2 账面（22 笔） | `just smoke-smp`（扩相：裸 + rootfs 双相）+ `just smoke`（主八跑）+ `just test-host` + `just run-el2` | smoke-smp：裸相 27 + `SMP ok` + **rootfs 相 45 全绿** + PAR 活证（**2026-11-01 验收翻转**：`PAR absent` 行诊断尾 `d1=` 非零即副核 park 派发活证——限频占空比下 witness 不作验收门；`PAR ok c0=… c1=…` 提前命中 = 超预期放行）；主 smoke 零漂移（单核非竞争路径不变）；rootfs 生态面（bsh/uu/THR/MMP/CLK/POLL…）在 -smp 2 全绿 = E0 语义等价的端到端证据。**首跑实录**：R3 三数 boot 6s/金丝雀 ≈0s/行/rootfs 129s——全远低帽 70% 线免升档；PAR witness 限频下仍提前命中（c0=256 c1=2） |

拆步缝 = gate（§2.3）：S-2a 全部改动单核可证；S-2b 才引入第二写核——问题域不混、回退面干净（gate 不置即回 S-2a 形态）。

## §5 验收

### 5.1 smoke-smp 扩相 harness（rootfs 相进 `-smp 2` 的工程面——侦察⑤ C 区全清）

| 项 | 处置 |
|----|------|
| 时限 | DEADLINE_ROOTFS 420→**840s**（MTTCG 0.61x → 一切帽 ×2 起步铁律）；**evsclient TOTAL_CAP_S=480 / ATTEMPT_CAP_S=45 双双硬编码（tools/evsclient.py:28/:30，无 argv/env 面）改 env 覆盖**——TOTAL：单核 worst boot→EVS ≈300s（smoke.sh:199 注）×1.6 ≈490s 已超帽 = 结构性假失败面，smoke-smp 线给 960s（保「client 帽 ≥ 相帽 + 60s」帽序，smoke.sh:204-206 口径）；ATTEMPT：smoke-smp 线给 90s——它是 vnet RT/CLOSE 内帽的「内帽 < 尝试帽」序（bsh main.rs:891 成文契约），内帽随 §5.4 整族 ×2 至 60s 后不放大即倒挂（60 > 45：单次对拍在 vnet 侧合法走满 60s 时 client 先弃 = 假失败 + 尸槽重试反吃 guest 释槽路）；evs/skt 对拍 30s→60s |
| 盘/端口 | 盘名专属 `disk.smp.img`（soak `disk.soak.img` / B4 `disk.pwr.img` 先例——防 mkfat 原地覆写兄弟正挂载的活盘）；echod 18080 起前 `listening on` 轮询 loud + **lsof 四端口让场检查**（现 pgrep-only 字段等待不够——echod 已起 QEMU 未起窗口；18080-18083 与主 smoke 同值 → 两线串行协议，端口整组偏移需 guest 侧 bsh/netd 定值连带改、不值得） |
| QEMU 形态 | 复制 smoke.sh rootfs 相全参（:471-477）+ **三个必要随迁件（审查 accept 补——照案首跑即红/假绿的工程面）**：① **fw_cfg 多卷发现环随迁**（smoke.sh:429-435 rootfs0..9 连续前缀拼接——mkcpio 主路恒走 `--split 32` 多卷产链（justfile:252-258），rootfs0.cpio 现 ≈30.8MB 已达单卷帽 32MiB 的 92%，硬编码单卷名随时踩墙）；② **rootfs 在席前置检查 + 构建依赖**：`[ -f rootfs.cpio ] || fail`（smoke.sh:423 同款）+ justfile `smoke-smp` recipe 补 `build-user` 依赖（:616 现仅 `build`——worktree/新环境无 rootfs*.cpio 时，缺前置检查 = ROOTFS-absent 双核相烧满 840s DEADLINE 才红，慢且误导）；③ **对拍 client 起动纪律**：evsclient QEMU 前起（自带重试环吃 hostfwd 未就绪态）+ **sktclient 懒启动**（等 guest `EVS ok` 再 exec——smoke.sh:337-342 实形态，非初稿误述的「QEMU 前起」；防 SYN 噪声拖慢 guest，A3-3 实测教训）。virtio-blk（`cache.writeback=off`）+ virtio-net hostfwd 三口 + force-legacy=false；stdin `< /dev/null`（bsh selftest 全内部自驱，smoke.sh:494） |
| 标记断言 | ROOTFS_MARKERS 44 项（smoke.sh:791-792 实值——母案 §4/§5 的 23/20 为 2026-10-22 陈值，按现值重述，单一事实源 = smoke.sh 实数组）；**TAB ok 前邻特判 `(^|[^G])TAB ok` 必须随迁**（TAGTAB ok boot 行在 -smp 2 裸相同样在席 = 裸 grep 假阳性，smoke.sh:371-376）；DEMO ok 特判 smoke-smp 已有（:82-88）；`ROOTFS vmo=none` 拒 + `app APPDEMO up` 附加断言随迁（:802-804/:820-821） |
| 单一事实源 | ROOTFS_MARKERS 复制进 smoke-smp.sh = 第二份漂移面（主 smoke 加标记、smoke-smp 漏跟 = 假绿/假红）——**抽 `tools/markers.sh`**（MARKERS/ROOTFS_MARKERS/assert_marker 三件共享，paths.sh 同款 source 模式；smoke-x86 经 justfile 注参先例）或最低限度把 `${#ROOTFS_MARKERS[@]}==44` 计数钉进断言文案 |
| 日志分相 | 每相 `: > "$LOG"` 清空（smoke.sh:466 先例）；`SMP ok ncores=2` 恰一次断言**按相各自成立**（两相两次 boot 各恰一次）；反向 `SMP: FAIL`/`SMP: skip` 全日志 grep 随相（smoke-smp.sh:103-104） |
| 金丝雀 | switches 行 ≥3 严格递增（awk 体沿用 smoke-smp.sh:126-139——数据源 A2 原子化全局单份：任核 tick 命中 100 倍数即整行持 PRINT_LOCK 打印，两核交替打印不破坏数值序列单调，论证 §5.2「金丝雀打印口径」）；收尾按 PID kill + wait + sleep 3（SIGTERM 后 QEMU 存活数秒，P6 A-6） |

### 5.2 并发金丝雀 `PAR ok`（状态谓词非竞速——4-5a 铁律；v1 无 affinity 的观测面裁决）

> **验收翻转（2026-10-31 裁决，2026-11-01 首跑实录）**：E0 无 IPI + park mod4 限频（雷 2 治理面）下副核 EL0 窗占空比 25%，witness 重叠窗结构性收窄——**真并行 witness 不作 S-2b 验收门，归 S-3**（IPI + per-CPU 队满速参与后的验收面）。S-2b 验收口径 = `PAR absent` 降级行诊断尾 **`d1=`（副核 park 派发计数）非零**即副核参与活证；`PAR ok c0=…` 提前命中 = 超预期放行（首跑实录即此态：witness 在限频下仍置位，c1=2 低采样边带——证据强度高于 absent 档，照收）。下方 D1/witness 设计原文保留（机制面未变，仅验收门翻转）。

- **裁决 = 选项 D1（per-CPU 计数见证，内核侧）**：SmpCpu 块加 per-core `el0_ticks`（本核 tick 采样时 CURRENT 为 EL0 用户任务的技术数）+ `switches_local` + `last_task`（tick 采样记本核当前任务 id）——**本核只写本核块**（per-CPU by construction 免锁；与 §2.2 对表不冲突——PAR 见证是本核私有观测数据，非共享内核状态，母案「IRQ 侧收 per-CPU-only」偏好在这类数据上成立）。
- **粘性 witness 位**：core0 tick 见自身=T1 且读 core1 `last_task==T2` 时单向置位——只在与件观察时置位、无 false-positive；false-negative（采样错过重叠窗）由持续运行的矩阵任务群 + 阈值窗口兜底。内存状态断言非竞速（侦察⑤ D0-6：S-1 SmpReport/哨兵 's' 同形态先例）。
- **标记形态**：`PAR ok c0=<el0_ticks0> c1=<el0_ticks1>`——打印任务（checkpoint 形态，**spawn_with_tag 私有表**——B13 铁律；ncores==2 才 spawn = 单核相零漂移自动成立）；断言 = 双核 el0_ticks ≥ 阈值 + witness 位置位（阈值首跑定标、宽松下界——TCG 慢夜漂移免疫；计数/旗标谓词，非时序——测试策略.md:86 口径）。
- **话术限定（B19）**：无 affinity/getcpu（ABI 无此号；MPIDR_EL1 EL0 读 = 陷阱）下只能诚实断言「双核都执行过用户任务 + tick 粒度重叠见证」；严格钉核断言留 S-3（per-CPU runqueue + 放置）。
- **反选项/可选项记录**：D3（console 输出来源核标记）**否决**——console 行汇到钉核 0 的 console 任务，不反映生产者所在核，零鉴别力；D2（共享 VMO 双任务进度见证）**可选不强制**——单核抢占交错也能过，证的是用户态共享面正确性非「同时执行」，与 D1 互补、按需追加。母案 §5 四件清单中 canary 页归 S-4、分配器完整性跨核探针归 S-5、ping-pong 对在无 affinity 下无法保证每核一个（以 D1 替代）——按拆步表归后续刀。
- **打印任务行为面（审查 accept 补——「观测窗没等到」与「真单核执行」要可区分）**：spawn 后每 2s 巡一轮双核块（klock 短窗读快照 + witness 位），60s 预算；预算内 witness 置位且双核 el0_ticks ≥ 阈值 → 打 `PAR ok …`；预算尽未见 → 先打一行降级诊断 `PAR: no-witness c0=<n> c1=<n>` 再退出（THR `eco: thr exit <n>` 降级行先例形态）——裸 FAIL 不带现场数据即丢「问题在参与面还是观测面」的第一手证据。
- **witness 读侧原子性论证**：last_task/el0_ticks 均为 64b 对齐单字、本核 tick 锁窗内写（无半字撕裂面）；跨核读最坏拿到旧值 = 该次采样少见一次重叠——只影响 witness **何时**置位不影响**是否**最终置位（粘性位只要求终值一致）；el0_ticks 在断言里只作宽松下界（阈值免疫陈旧读）。per-CPU 块本身按核独占写、无 RMW 竞争。
- **harness 断言口径**：rootfs 相 grep 到 `PAR: no-witness` 降级行 = **FAIL**（S-2b gate 已置位 + `SMP ok ncores=2` 已在席 = 双核 bring-up 闭环，无 witness 即参与面生效性 bug，不是「观测窗没等到」）；单核相 gate 不置位 → 不 spawn 打印任务 = 零漂移（无行即无断言，非缺行 FAIL）。
- **金丝雀打印口径（与 A2/E25/G43 合读，审查 accept 补）**：SWITCHES = 全局单 AtomicU64，两核 tick 在各自 KERN_LOCK 窗内 `fetch_add`——BKL 串行 ⇒ 恰一核的增量命中每个 100 倍数，该核**在窗内**整行持 PRINT_LOCK（irqsave）打印 `switches=<快照>`；打印值取自同一全局单调计数的原子快照 ⇒ 日志里 switches 数值序列严格递增，smoke-smp.sh:136-139 awk 断言口径零漂移（无需指定打印者/聚合层）；他核他行（矩阵任务等）插在两条金丝雀行之间不扰 awk——它只抽 `switches=N` 序列。

### 5.3 ktest/boot 期单核独占不受扰说明

boot 序铁锚：ktest（main.rs:451）→ sched::init（:473）→ bench → 全 spawn/回调注册（:1365-1375）→ smp_bringup（:1382，DAIF 屏蔽尾窗）→ arm_slice（:1385）→ enable_irq（:1386）→ yield_now（:1387）→ idle_loop（:1389）。ktest/bench 全在 bringup 前 = **副核结构性未起**——锁零竞争、断言面零漂移；EL1 真 SVC（svc4/svc6）在 ktest 期走同一 dispatch 取放 = 锁路径本身被 ktest 覆盖而对象语义不变。**S-2 不得改此序**（先放核后 ktest 即破单核独占前提——侦察② §6 #43；ktest「绝不可在手搭睡眠者在队时走 SVC send」铁律照旧成立，锁不改变其语义面）。

### 5.4 时序与观测纪律

- **guest 内编译期帽 ×2 审计**（MTTCG 多核版「client 收侧帽按 worst 路径成本定值」）：最高优先 = **CP_EOF_CAP_NS 30s**（bsh main.rs:3839——单核实测 ≈17s ×1.6 ≈28s 贴帽，C-5 定标实录）。**裁决 = 逐帽 ×2 重定值**（帽只兜结构性挂死不押时序——smoke.sh:180 既有口径；单核相帽变宽不掩单核回归，因单核回归证据 = 标记到位非时序）。清单：svcmgr 六帽 30→60s（U1_EOF/APP_EOF/PS_CAP/BC_CAP/THR_CAP/BLKD_REPLY，svcmgr main.rs:1779/:2041/:1135/:1398/:1594/:1721）/ DNS·TCPS 60→120s（:1743/:1757）/ NETS 30→60（bsh main.rs:1852）/ CP 30→60（:3839）/ libc CAP_SLOW 45→90（bsh main.rs:1436 注）。
- **vnet 四帽整族 ×2（审查 accept 补——初稿只放大 TCPS_CLOSE 一节 = 帽序倒挂制造机）**：TCPS_RT/TCPS_CLOSE 30→60s（drivers/bins/vnet/src/main.rs:129-130——初稿引注 justfile:342 有误，该处实为 bsh accept-wait 注）+ SELFTEST_CAP_NS/LSN_CAP_NS 30→60s（vnet/src/tcp.rs:53/:1034）。**禁止只放大链上一节**：帽序耦合是成文契约——bsh main.rs:891「vnet RT/CLOSE 内帽 30s → client 侧 45s」，只放大内帽即倒挂（60 > 45：vnet 侧合法走满 60s 时 client 先弃 = 假失败）。
- **bsh/evsclient 侧帽对账段（×2 后哪些动、哪些不动、依据成文）**：evsclient ATTEMPT_CAP_S 45→90（env 化，§5.1——随内帽 60 保序）/ bsh 侧 RT/CLOSE client 帽 45→90（main.rs:891 同款保序）/ dat_dl 30→60（成本基 = Poll RPC + CLOSE 释槽同族慢化）/ acc_dl 120→240（成本基 = client 总帽内的必达窗，随 TOTAL 480→960 同源放大）/ 尸槽释槽路 30s = vnet CLOSE 内帽（随族 60 已覆盖，不另立）/ **revives=3 维持 + 重定标条件写明**：定标基 ~47s client 周期随帽放大变 ~94s，覆盖窗 3×~94s ≈282s 不缩，首跑若「revive 耗尽未见活连接」率异常即按实测周期重定标并触发 R1 审计（吞吐退幅的下游症状面）。
- **FAST ok 保留论证**（B20）：fa 对在 bringup 前单核等价段跑（boot 窗）→ 双核相不劣化；hits/miss/direct 弹性 N-2 断言是计数谓词——保留；时序基线（bench.py）不复制进 -smp 2。
- **icount 刷新**：S-2a 验收含 `bench.py --icount` 双跑 + 基线刷新 diff 随源入册（S-1 先例）；S-2b 的 gate 置位不进单核相、不动单核 icount 面。
- **单核零漂移硬线**：主 smoke 八跑（裸 26 / rootfs 44 / RX·FAT 70×3 / B4 三 boot）+ EL2 + smoke-x86 + abi-check + assert-flip——每刀回归门（母案 §5）。

## §6 风险表 + kill criteria

| # | 风险 | 缓解 |
|---|------|------|
| R1 | 持锁时长放大：pi::recompute O(n²)（B18）在锁内拉长临界区 → 副核自旋变长 | v1 n≈个位数可忍；TTAS 长自旋 = E0 违约信号；**可判定触发器**（审查 accept 补）：rootfs 相标记到齐 wall >1200s 或金丝雀行周期 > 单核均值 3x（`grep -o 'switches=[0-9]*' $LOG` 相邻值差 + 行时间戳）⇒ 触发 R1/R3 审计并冻结在 S-2a（gate 回退）；E1 拆细优先项记账 |
| R2 | 跨核唤醒延迟 ≤1 slice（无 IPI）：被唤任务在目标核下次 tick 才上核 | E0 接受（母案 §2.1 边界③同款权衡）；FAST 类时序面不进双核断言（§5.4）；S-3 SGI 根治 |
| R3 | TTAS 忙转在 TCG 双核 = 吞吐税（争用时两核互拖） | 临界区 = syscall 主体（短）；PAR/金丝雀首跑定标盯吞吐退幅；退幅异常即触发 R1 审计。**×2 基准的测量态承认（审查 accept 补）**：0.61x 系 S-1 在「副核 wfi park（零参与）」态测得——S-2b 新态（副核活跃跑 EL0 + TTAS 争用 + 主核 idle 改形）**无测量数据，×2 是否够正是 S-2b 首跑要回答的问题**：首跑记录三数（boot 到齐 / 金丝雀周期 / rootfs 相到齐 wall）为定标义务；任一数 > 帽 70% 即预授权 ×1.5 升档一次（只改一处定值：DEADLINE_ROOTFS 840→1200 / TOTAL 960→1440，非全案重标——升档后仍超 = R1 审计而非再升） |
| R5 | Task 位错配（多轮睡眠重复放/取、trampoline 出生态、直调包络漏置位） | 位随 Task 走 + 出生 0 + 包络对称置清——§2.3 三处不变量注释立法 |
| R6 | ktest 契约面「屏蔽窗」表述过时 | 注释对齐随实施刀（行为零变化，侦察⑤ E4）；§3.4 已给等价口径 |
| R7 | smoke-smp rootfs 相 20-25min worst | 不进 CI（挂 S-5 G2 过关再议——母案 §4/§5）；周跑/手跑 soak 化 |
| R8 | 「放锁后零共享触达」被后续提交无声破环 | dispatch 尾/:383/两收口点注释立法（CLAUDE.md Gotchas 随实施刀增补——本立稿不动） |
| R9 | watchdog 断言形态（遥测环走查 check/wake_check 单调）误报 | 环走查入 KERN_LOCK 短窗（F37——与 B12/B13 锁窗写者 BKL 串行，断言面与单核同构）+ 计数快照原子读（A1/A2/A12/I50）；首跑定标复核 |
| R10 | 副核 32KiB boot 栈深度（真 SVC 深路径 + IRQ 嵌套；栈含主核 boot 残留但单写者复用已核——boot.rs:93-94） | 首跑栈水位探针；**触发器** = 水位 >24KiB（75%）或任何副核栈溢出 dump（FAR 落栈界）⇒ 换 64KiB 专属栈（.bss.boot 段） |
| R11 | bringup 5s 心跳窗（smp.rs:370/:419）在双核慢夜 + 更重内核下不够 | S-1 实测默认机型 OK；**触发器** = `SMP ok` 行 5s 心跳窗超时（hb2 timeout FAIL 形态）⇒ 5s→10s 一处定值改（同 §5.4 口径，不牵连他帽） |

**kill criteria（双线并引，不互相替代）**：

- **S-2 单刀冻结线（Station-2 形）**：连续两提交周期 >50% 主线工时投入 S-2 未收官 → 冻结顺延（`docs/VM里程碑设计草案.md:148`——本刀处于 VM 里程碑 Station 语境的任务书线）。
- **SMP 线整体冻结线（母案）**：SMP 线占用 >20% 主线工时且无对应里程碑产出 → 回单核冻结（`docs/SMP设计草案.md:259-260`）。

## §7 遗留与 S-3 交接口

- **KERN_LOCK 退役路径（E1）**：S-3 per-CPU runqueue + 细粒度锁落地后，KERN_LOCK 从「入口大锁」收缩为「对象系/表锁」→ 随逐域拆锁退场；Task.klock_window 位随锁消亡；**§3.3 切换原语舞蹈表 + §1 总表直接移交 S-3 作拆锁取放点地图**（每行「现访问纪律 → 处置档」即拆锁时序输入——本立稿的最大沉淀价值）。
- **留给 S-3**：per-CPU RUNQUEUE/负载均衡/放置、IPI/SGI（R2 根治 + tickless idle 恢复）、CURRENT 真体化（S-2b 访问器为过渡形态）、tick 面最终形态（S-2 tick 仍全局锁窗——per-CPU 化随 runqueue 走；**LAST_VCT/CPU0 块已随本刀前置件 2 收账**）、遥测双环 per-CPU 化（母案 E1 原位——BKL 下单环锁窗串行即正确，per-CPU 是拆锁后的并行化前提）、`frames()` 类型面真修（D25）、Event.signals AtomicU64 化（C21 备选）、打印锁争用剖面（A/B 矩阵高频面——真机数据后再裁）。
- **留给 S-4**：TLB shootdown、canary 页金丝雀（母案 §5 清单归位）。
- **留给 S-5**：分配器完整性跨核探针、lockdep（非目标重申）、G2 过关判定（「负载均衡对/shootdown 对/无竞态」全绿才是 G2）。
- **文档义务**：实施刀落地时 CLAUDE.md Gotchas 增补锁纪律三条（持锁永不睡永不切 / `-> !` :383 单点放锁 / 放锁后零共享触达）；进展追记 `docs/阶段沿革.md`（本立稿不回写任何代码/CLAUDE 面）。

---

**立稿裁决存目（起草中最难三裁决 + 立稿后审查修订）**：① 锁挂载 = 单锁 + Task 位 + arch 槽注入（方案 A；否决 KERN_LOCK+SCHED_RAW 双锁——少发明原语 + 双锁不盖 wait 重验对象读）；② IRQ 边界 = tick 锁窗 / RX 短窗 / 计数原子化三分处置，显式取母案「同锁」分支弃其 per-CPU-only 偏好（§2.2 对表；同核自旋不可能性论证 = IRQ33 钉核 0 × KERN_LOCK 任务态持有者必在屏蔽窗 × 持锁者永不睡；打印锁同核安全 = G43 irqsave + G44 分层论证）；③ 副核形态 = gate 参与门 + 门控唤醒探测环 + pinned!=0 pick 过滤 + wfi 周期探测（无 IPI 的 E0 代价 ≤1 slice 显式记账，S-2a/S-2b 拆步缝即 gate）。**审查修订存目**：遥测双环前置件撤销（单环锁窗串行即正确）、SWITCHES 族 per-CPU 化否决改全局原子（A1/A2/R4 连带面蒸发）、PRINT_LOCK 补 irqsave + 拼行 API（G43——同核 tick 死锁雷）、F39-F42 四直调点补账、A11/A12/D25 补行、§5.1 三随迁件、§5.4 vnet 整族 ×2 + 帽对账、R1/R3/R10/R11 触发器量化、0.61x 测量态承认（S-2b 首跑定标义务）。
