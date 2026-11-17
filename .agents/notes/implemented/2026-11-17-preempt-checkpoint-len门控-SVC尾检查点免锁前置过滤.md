# preempt_checkpoint len 门控:SVC 尾检查点免锁前置过滤

Status: implemented

## Problem

SVC 派发尾无条件调用 preempt_checkpoint(arch/aarch64/src/syscall.rs 槽接线;sched/src/sched.rs preempt_checkpoint 本体),单任务/空环常态下每次系统调用恒付一整段:DAIF 存屏往返 + PREEMPT_CKPT_HITS 全局原子 fetch_add + try_preempt 的 RQ 锁对与 O(n) has_eff_above 探峰环扫——而 g3-1b 注自证「现役 SVC 全程 IRQ 屏蔽 ⇒ 单核 rq 状态窗内不可变 ⇒ 探峰恒空 = 纯空探」,注定落空的成本恒付。04 号调度审计判为 Top-1 机会(见 [04 调度热路径 Top-5 审计](../../../research/内核深度进化/04-调度热路径-Top5优化机会-v1.0.md) 机会一)。

## Decision

- **len<=1 免锁前置门**:preempt_checkpoint 入口最先判 `rq().len() <= 1` 直返。可靠性链:cur-in-ring 不变式(Running 常驻环,class.rs Fair 结构注;pick 不摘任务,仅 block/exit 动环)下 len<=1 ⟹ 环上至多 cur 自己 ⟹ eligible 集(my_cur 自豁免 + Running 排除)恒空 ⟹ 探峰必空、必不直切——跳过的本就是无副作用的空探,零行为变化。
- **判例双层结构**:on_sgi_resched 已有 len>=2 出选门(彼在 RQ 锁窗内、替探峰直接出选);本门照其 len 语义但放锁窗外做免锁前置过滤(Relaxed 原子读,runqueue.rs len 的跨核读者口径)。竞态双向安全:读到假阳(len 实已回落)走到锁窗权威探峰自然落空;读到假阴(跨核新入队未可见)漏看的他者由 enqueue_with_kick 窗外 SGI 唤醒戳兜底 → on_sgi_resched 出选,不丢切换。
- **计数语义随门收窄**:PREEMPT_CKPT_HITS「真探形」从「count==0 支」收窄为「len 门 + count==0 支」(计数点随门后移,空探不再摸全局 cache line——审计机会一③ 的移窗形态)。fa_ping 的 ckpt>=1 断言(kernel/src/main.rs fa 尾)是双任务 ping-pong 形态,len>=2 恒真,断言面与 PreemptSpan 泄漏绊网语义均存活。
- **ScanAcc 合并跳过**:任务书步骤 2 的跳过条款命中——pick 扫描现役已是单圈合并形(class.rs scan_filtered 注自证:「合并前形态 = 两圈……现单圈三分支等价合并」,S2 已收口),无需再合。

## Alternatives considered

- len 门放 try_preempt 的 RQ 锁窗内(逐字照搬 on_sgi_resched 形):门在窗内则锁对照付,单任务常态省不掉主要成本;on_sgi_resched 在窗内是因为它替探峰直接出选(len 读与出选须同窗不撕裂),此处是前置过滤 + 锁窗权威探峰的双层结构,免锁读的两向竞态均安全(见 Decision)。
- has_eff_above + pick 合一圈(审计机会一②):scan_filtered 的 lazy_enforce(带武装时逐节点双向对账愈合)只在 pick 扫描侧、has_eff_above 侧无——合一会让空探形也触发带限流愈合,armed 场景下非逐位零行为变化;且消费面 lazy 门不对称需专刀核证,超出本快赢「最小侵入·零行为变化」判分,留带武装场景实跑收益证据后再议(审计建议次序也把机会一排在机会二之后分刀)。
- PREEMPT_CKPT_HITS 直接 per-CPU 化(审计机会三):同族独立刀,读者面(watchdog/G1.5 配平断言)需同窗快照配平,不并入本刀。

## Consequences

- 单任务/空环 SVC 尾常态:每检查点省 1 全局原子 RMW + DAIF 存屏往返 + RQ 锁对 + O(n) 环扫;多任务形态(len>=2)路径与计数逐位不变。
- 验证实跑:just build 绿(隔离 target-pckpt);berry_verify host 五面绿(fwcfg 26 键/docs 406 篇 0 警告/ABI 五面/berry CLI 测试/test-host);QEMU smoke boot 相 32/32 标记到齐(exit 0)——含 ktest done(全套 ktest 零回归)、FAST ok(fa_ping ckpt>=1 断言面存活 = 计数门不破)、BENCH ok n=6(LOADGEN 断言不受影响——多任务场景 preempt 仍工作)、FRZCYC/G1.5 等全绿。
- 动面收敛:sched/src/sched.rs 单文件两处(preempt_checkpoint 入口 + PREEMPT_CKPT_HITS 注释计数语义);零 ABI 面、零 trait 面、零数据结构面。
