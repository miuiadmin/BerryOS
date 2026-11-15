# 07 号:syscall 全号 icount 屏蔽窗表——38 号全表 · 屏蔽段逐号定量 · 中断→任务 worst 新版

Type: survey
Status: current
Updated: 2026-11-15
版本:1.0

> 定位:[06 号综合定谳](06-综合定谳-v1.0.md)最紧迫三刀之③的落地件——02 号缺口一「长屏蔽窗无全量表(最大空白)」的收口:全号 syscall 中断屏蔽段的 icount 指令数定量 + 屏蔽形态逐号标注 + 交叉出屏蔽窗表,并随 FPSIMD lazy(2026-11-14 落地)刷新 R3 三路数字。02 号 §1.3 尾「中断→任务 worst = 最长全屏蔽 syscall 段 + 2000cy 链」中此前无表的前项,本件以两通道实测填上。
> 必读件已核:[02 号实时性](02-实时性.md)全文(§1.3 延迟链 + §② WCET 基线族直读);`tools/abi.toml` 全文(38 号 v1 在册表直读);`kernel/src/syscall.rs` 分派器与屏蔽段全文直读(install 表 :3912-4015、`irq_window` :2989-3004、阻塞位点全文 grep);`tools/bench.py` `--icount` 通道(run_icount :1064-1171);`tools/bench_baseline_icount.txt` + `tools/bench_baseline_icount.notes.md`(布局骰史档)直读。
> 方法与证据纪律:测量经**临时探针**(bench 形态,`svc #0` 真往返 per-op CNTVCT 三明治——测毕已整段还原,主工作树 kernel/ 零残留、git status 实核);构建钉在隔离 worktree(HEAD `f52d834b4` + 仅探针补丁)避开并行会话在途改动;icount 通道双跑 verdict=exact(42 行逐值全等);实时通道六跑取最小(墙钟噪声只增不减;偶发宿主污染样本实录在档);±2KB bss 扰动臂定敏感性(vmo_create ±88 tick、四行 ±1-4、余 exact);推断标「推断」;两通道数值不互换。

## 结论速览

一句话判词:38 号全表的屏蔽窗**第一次有数**——全号除 0x52/0x53 拷贝段局部解蔽外全程硬件 DAIF 屏蔽(SVC 入口置位、主体不解蔽),实测代表路径的全屏蔽段在**实时通道固定族 ≤~500cy**(vmar_protect 496 / process_create 444 / vmar 族 329-379)、icount 通道 ≤~1.7 万指令(process_create 16968),调度面 syscall(signal/wait/sleep/get_exit_code/handle 族)全部 ≤~180cy / ≤2600 指令;中断→任务 worst 从「无表的前项 + 2000cy」升为「**实测固定族 ≈500cy + 链 2000cy ≈ 2500cy(≈1.7µs @A72-1.5GHz)**」,且发现 02 号「FPSIMD lazy 后 worst 已低于 2000cy」的推断须修正——EL0 任务出生即 `fp_needed=true` 恒走全量档(`arch/aarch64/src/task.rs:151-153,169`),135cy 项只对 kernel↔kernel 切换对消失(链 ≈1880cy),EL0 RT 任务链(实时正主)不变。

## ① 屏蔽机制的结构事实(逐号标注的判据)

SVC 屏蔽窗的构成(代码锚全部直核):

1. **硬件入口窗(全域默认)**:`svc #0` 异常入口硬件置 PSTATE.I,handler 全程不解蔽——「syscall 主体天然不可抢占(单核)」的免锁根基,唯 send/recv 拷贝段例外(`kernel/src/syscall.rs:26-30` 模块注;派发面 `arch/aarch64/src/syscall.rs:51-88`)。故**每号的屏蔽窗下界 = 全号统一**:探针实测未注册槽往返 = 22 icount tick(176 指令)/ 57-63 实时 cy——与 B-1 用户态 `sys_null` 基线 63cy(`tools/bench_baseline.txt` BENCH1 段)互证,两通道地板各自钉板。
2. **局部解蔽域(唯两号)**:0x52 `channel_send` 拷贝段两处(`kernel/src/syscall.rs:3078,3124`)、0x53 `channel_recv` 六处(:3343,3351,3383,3405,3410,3415)经 `irq_window`(save→enable_irq→f→restore,:2989-3004)局部解蔽——窗内表零触达、段 `PreemptSpan` 禁切(:2966-2987);解蔽只覆盖用户缓冲拷贝,门控/队列手术/唤醒判定仍在屏蔽内。
3. **console 域不解蔽**:`with_print_lock` 窗从 irqsave 换 preempt 计数包络后**不触 DAIF**——入口屏蔽者(SVC 主体)维持屏蔽,出口按 DAIF 态分诊检查点(`arch/aarch64/src/kern_lock.rs:386-408`;`sched/src/sched.rs:189-215`)。即 syscall 内的 console 打印不构成解蔽窗——02 号 §1.2「解蔽域只有两域」中的 console 域是**任务态打印的正主域**,SVC 主体内不生效(本件实测旁证:terminate 全屏蔽窗 50 tick 与其诊断打印共存)。
4. **阻塞界窗**:0x04/0x05/0x21/0x23/0x24/0x37 睡下时经 `block_current` 的放/取锁舞蹈交出 CPU——屏蔽窗在 block 点**终结**(IRQ 由后继上下文受理),窗长 = 入口→block 段;0x52/0.53 v1 本体非阻塞(满 -EBUSY/空 -EAGAIN,等待面走 0x04,`kernel/src/syscall.rs:40-41`),故 IPC 阻塞形的全屏蔽段由 0x04/0x05 行代表。
5. **不归窗**:`-> !` handler(0x33/0x36)不返 eret,放锁在 `switch_away_forever_with` 舞蹈收口(派发尾对其不可达,`arch/aarch64/src/syscall.rs:53-58`)——屏蔽窗延伸进退出链;0x88 成功路 = PSCI 整机下电不归(失败路 = probe 缓存读 + HVC 拒收,极短)。
6. **入口锁类(SMP 结构窗,非 IRQ 窗)**:每号注册时的 `SVC_CLASS_*` 位集(ALL 0x3E / SQ_RQ_TABLE 0x2E / RQ_TABLE 0x2C / SQ_RQ 0x0E / TABLE 0x20,`kernel/src/syscall.rs:3928-4014` + `arch/aarch64/src/kern_lock.rs:191-207`)是结构锁包络——与 DAIF 屏蔽窗同生同灭但语义不同(禁他核写者 vs 禁本核 IRQ),表 ③ 一并收录供 SMP 面引用。

## ② 测量形态(临时探针 · 两通道 · 确定性证明)

探针 = bench 形态临时补丁(测毕已还原,见头块纪律):boot 窗(ktest 后、timer/sched init 前,单线程 IRQ 屏蔽)对每号做 256 轮**真 `svc #0` 往返**的 per-op CNTVCT 三明治(prep/teardown 在三明治外——bench_fa 先例),三明治含硬件异常级进出 + `svc_enter_class` + handler + `svc_exit_class` + eret,即 **SVC 硬件 DAIF 屏蔽窗的全跨度**。输出 `SYSPROBE,<tag>,<nr>,<iters>,<per>,<rc>`(rc = 末次返回码,实测路径判据——表 ③ 逐行携带,全 42 行 rc 与目标路径一致)。覆盖面:38 号 v1 在册中 **33 号有实测行**(32 实现号 + 0x85 桩行)+ 1.1 档 3 号(0x3C/0x3D/0x3E)+ 0x83 = 表中 37 行带数;未测 6 号(0x32/0x33/0x35/0x36/0x88/0x3F)各有硬因(§ ⑤)。

通道两臂:①**icount 臂**(`-icount shift=1,sleep=off`,B-2 同形;1 CNTVCT tick = 8 指令)双跑 **verdict=exact**(42 行逐值全等——「同指令流必同数」复现);②**实时臂**(TCG 实时,无 -icount;与 R3 的 2000cy 同通道族)六跑取最小——逐跑中位典型偏差 ≤3%,但偶发宿主调度污染样本(实录:vmar_map 单跑 1188 vs 干净簇 329-343、chan_recv_8b 单跑 926 vs 干净簇 337-340),墙钟噪声只增不减故取最小为代表。两通道数值**不可互换**(icount/实时跨模式铁律,`tools/bench.py:32-49`;实测逐号比值 2-9 不稳)。

敏感性对照(±2KB bss 扰动体重掷布局骰,史档 §A95/§A109 方法复刻;41 行前身探针 A/B 两臂 + 加行重掷共三态观测):**分配器族五行敏感**——vmo_create ±88、vmo_commit/vmar_alloc/pager24_gate/proc_create ±1-4;余 36 行跨三态 exact。与官方基线史档漂移签名同构(分配器族敏感、长窗项不动,`tools/bench_baseline_icount.notes.md` g3-1c-2 条)。表 ③ icount 列对五行带 ± 口径,其余行视为精确;thread_create 行未过扰动臂(create 族,推断同族敏感——**推断**)。

## ③ 全号表:38 号 v1 在册 × 屏蔽形态 × 两通道窗长

屏蔽列词表:**全程** = 硬件 DAIF 入口→eret 全屏蔽(默认);**拷贝段解蔽** = 仅 0x52/0.53 的用户缓冲拷贝子段(净屏蔽窗 < 总长);**block 界** = 窗在 block_current 舞蹈终结;**不归** = 窗延伸进退出/下电链。icount 列 = tick 数(括号 ×8 指令;±n = 布局骰敏感性);实时列 = 六跑最小 cyc(TCG,与 R3 同族)。

| 号 | syscall | stage | 锁类 | 屏蔽形态 | 实测路径 | icount tick(×8 指令) | 实时 cyc |
|---|---|---|---|---|---|---|---|
| 0x01 | handle_close | v1 | SQ_RQ_TABLE | 全程 | Event 轻径 / chan 端重径 | 49 / 45(392/360) | 72 / 70 |
| 0x02 | handle_duplicate | v1 | TABLE | 全程 | 成功径(SAME 全 1) | 100(800) | 88 |
| 0x03 | handle_replace | v1 | TABLE | 全程 | 原子换槽 | 112(896) | 106 |
| 0x04 | object_wait_one | v1 | SQ_RQ_TABLE | 全程+block 界 | poll miss→-EAGAIN | 66(528) | 78 |
| 0x05 | object_wait_many | v1 | SQ_RQ_TABLE | 全程+block 界 | 2 项 poll miss | 323(2584) | 139 |
| 0x06 | object_signal | v1 | SQ_RQ_TABLE | 全程 | Event 用户位幂等 | 48(384) | 71 |
| 0x61 | clock_read | v1 | TABLE | 全程 | MONOTONIC 出参 | 50(400) | 73 |
| 0x11 | vmo_create | v1 | TABLE | 全程 | 2MiB 建删配平 | 1330±88(10640) | 302 |
| 0x12 | vmo_commit | v1 | TABLE | 全程 | 逐页新 commit(含清零) | 1446±1(11568) | 312 |
| 0x13 | vmo_decommit | v1 | TABLE | 全程 | 逐页归还 | 108(864) | 95 |
| 0x14 | vmo_read | v1 | TABLE | 全程 | 8B 读 | 172(1376) | 97 |
| 0x15 | vmo_write | v1 | TABLE | 全程 | 8B 写 | 171(1368) | 96 |
| 0x16 | vmar_map | v1 | TABLE | 全程 | 1 页 map | 119(952) | 329 |
| 0x17 | vmar_unmap | v1 | TABLE | 全程 | 1 页 unmap | 60(480) | 379 |
| 0x18 | vmar_protect | v1 | TABLE | 全程 | RW→RW 幂等 | 55(440) | 496 |
| 0x19 | vmar_allocate | v1 | TABLE | 全程(线性项) | 64K 子区间·累积扫描注 | 4980±4(39840) | 724 |
| 0x1A | vmo_get_size | v1 | TABLE | 全程 | u64 出参 | 77(616) | 80 |
| 0x1B | vmo_get_phys | v1 | TABLE | 全程 | 已提交页 PA | 83(664) | 81 |
| 0x21 | pager_supply_pages | v1 | SQ_RQ_TABLE | 全程+block 界 | 门控早退·真收货未测 | 35(280) | 73 |
| 0x23 | pager_supply_mapped | v1 | SQ_RQ_TABLE | 全程+block 界 | 门控早退·真收货未测 | 35(280) | 67 |
| 0x24 | pager_supply_mapped_table | v1 | SQ_RQ_TABLE | 全程+block 界 | n=0 参数径/单窗门控径 | 28 / 588±1(224/4704) | 64 / 161 |
| 0x31 | process_create | v1 | ALL | 全程 | 双出参+配额查 | 2121±1(16968) | 444 |
| 0x34 | thread_create | v1 | ALL | 全程 | 未启动 Thread+授柄 | 348(2784) | 128 |
| 0x3A | process_get_exit_code | v1 | TABLE | 全程 | 未退→-EAGAIN | 51(408) | 73 |
| 0x3B | process_terminate | v1 | ALL | 全程 | 未 start 轻路径 | 50(400) | 76 |
| 0x3C | sched_setattr | 1.1 | TABLE | 全程 | 56B WEIGHT→Thread | 288(2304) | 145 |
| 0x3D | sched_getattr | 1.1 | TABLE | 全程 | effective 回写 56B | 340(2720) | 172 |
| 0x3E | process_suspend | 1.1 | TABLE | 全程 | 空进程误差径·成功径未测 | 108(864) | 107 |
| 0x3F | thread_yield | 1.1 | RQ_TABLE | 全程+切舞 | **未测**(需调度器) | — | — |
| 0x37 | thread_sleep | v1 | SQ_RQ | 全程+block 界 | deadline=0 即返 poll 径 | 26(208) | 71 |
| 0x51 | channel_create | v1 | ALL | 全程 | 端点对+配额查 | 979(7832) | 278 |
| 0x52 | channel_send | v1 | SQ_RQ_TABLE | **拷贝段解蔽** | fastpath 8B/顶格 256B | 1056 / 1474(8448/11792) | 276 / 325 |
| 0x53 | channel_recv | v1 | SQ_RQ_TABLE | **拷贝段解蔽** | fastpath 8B/顶格 256B | 1268 / 1477(10144/11816) | 337 / 362 |
| 0x54 | event_create | v1 | ALL | 全程 | options=0 | 340(2720) | 124 |
| 0x85 | berry_attest | v1(未实现) | NONE | 全程 | 默认槽=-ENOTSUP | 22(176) | 60±量化 |
| 0x86 | signal_send | v1 | TABLE | 全程 | h≠0 MANAGE 门过全径 | 38(304) | 74 |
| 0x87 | signal_take | v1 | TABLE | 全程 | 误差径·真径差一原子 swap | 28(224) | 63 |
| 0x88 | power_off | v1 | TABLE | 不归(成功) | **未测**(下电不可测;失败路极短) | — | — |
| 0x83 | policy_bind | v3(提前实现) | TABLE | 全程 | auth 门控早退·COMMIT 全径未测 | 34(272) | 65 |
| 0x32 | process_start | v1 | RQ_TABLE | 全程+spawn | **未测**(需调度器) | — | — |
| 0x33 | process_exit | v1 | ALL | **不归** | **未测**(窗延伸进退出链) | — | — |
| 0x35 | thread_start | v1 | RQ_TABLE | 全程+spawn | **未测**(需调度器) | — | — |
| 0x36 | thread_exit | v1 | ALL | **不归** | **未测**(同 0x33) | — | — |
| — | 未注册槽(0x99) | — | NONE | 全程 | 默认槽往返(全域地板) | 22(176) | 57-63 |

表注:①stage 列自 `tools/abi.toml:325-734`(v1 在册 38 号 = 37 实现 + 0x85 恒 -ENOTSUP,`tools/abi.toml:33-44`);0x3C-0x3F 为 1.1 档、0x83 为 v3 提前实现号——真注册面 42 号(38 v1 + 4×1.1 + 0x83),全部入表,其中 32 号有实测行。②icount 括号 = ×8 指令数;±n = 布局骰敏感性(§ ②);attest 行实时值在亚 tick 量化带(六样本 32-63)取上簇标注。③「未测」四类各有硬因:spawn/yield 面需调度器在场(boot 窗不可达,post-sched 探针留 § ⑤);不归面(0x33/0x36/0x88 成功)测即杀死测量载体。

## ④ 交叉:持屏蔽 × 窗长 = 屏蔽窗定量与 worst 项

**最长全屏蔽段(固定族实测)**:实时通道 vmar_protect 496cy > process_create 444 > vmar_unmap 379 > recv_256 总长 362(净屏蔽窗更小,见下)> vmar_map 329 > send_256 总长 325 > commit 312 > vmo_create 302;icount 通道 process_create 16968 指令为顶(2121 tick)。调度面(signal/wait/sleep/yield 族、handle 族、clock/getattr 族)全部 ≤~180cy / ≤2600 指令——**实时任务常用面的屏蔽窗比内存面小一个数量级**。

**0x52/0x53 的解蔽分解**(拷贝段线性外推):send 8B→256B 斜率 = (1474−1056)/248B ≈ 1.69 tick/B(icount)/ (325−276)/248 ≈ 0.20 cy/B(实时)——增量部分即解蔽子段(用户缓冲走查+拷贝在 `irq_window` 内),净连续屏蔽窗 = 总长 − 解蔽段;对 IRQ 延迟而言 send/recv 的有效屏蔽窗以**固定框**(门控+队列手术+唤醒判定,≈270-340cy)为主导,载荷线性部分 IRQ 可入。

**无界/线性项如实列名**:vmar_allocate 屏蔽窗含 O(在册区间数) 重叠扫描(256 累积区间实测均值 724cy/4980 tick;区间数由调用方行为决定,v1 无帽——非恒定 worst 项,建议后续在容量工程面挂帽);wait_many 窗含 O(项数) 走查(2 项 139cy,帽 64 项线性外推至数百 cy 级——**推断**);vmo_read/write 随载荷线性增长且全程屏蔽(vmo 族解蔽被 §A126 判死,02 号 §1.2),4KB 顶格在官方基线 vmo_rw4k = 6948 icount / 771 实时 cy(读写各半量级——**推断**)。

## ⑤ 未测面与留窗

①**spawn/yield/start 面**(0x32/0x35/0x3F):需调度器在场,boot 窗探针结构不可达;post-sched 探针(boot spawn 活证面挂 BENCHWAKE 同族观测行)是纯工程延伸,预期窗 = spawn+enqueue+可能的跨核 SGI(wake p50=63cy 基线在册,`tools/sched_baseline.json`)。②**不归面**(0x33/0x36 退出链、0x88 成功路):退出链窗 = 轻路径(实测 0x3B 未 start 族 50 tick)+ REAPQ 摘队 + TERMINATED 唤醒 + drain 移交,结构上界 ≈ terminate 重路径 + wake 链(**推断**,≤~200cy 量级);真测需 post-sched 一次性探针(测即退出,单发)。③**pager 真收货路径**:poll-first 门控后即泊键阻塞,供给体 = pt_lock 窗 commit+填叶+tlbi+唤 faulter——与 vmar_map(实测 329cy)同构再加唤醒(**推断 ≈ 400-500cy**)。④**with-waiter send**(fastpath 定点唤醒+直切):实测为空队无 waiter 形,重形加 wake_matching+try_preempt(sched 基线 wake 63cy——**推断**)。⑤**policy_bind COMMIT 全径**:涉全局 SNAPS 换表(06 号刀① shadow 面所在),刻意不测以免扰动策略状态;表拷入成本由 pager24_gate(588 tick)同构代表。

## ⑥ R3 三路刷新(FPSIMD lazy 后)与旧值对照

R3 原始定谳(七十七轮 91ce41c2,`docs/系统优化路线图-v1.0.md:36-43` 一手直读):中断→任务基线 ≈830cy / worst ≈2000cy(固定 300 + ctxsw 135〔FPSIMD 全量〕+ pick 15cy/任务 + expire 80cy/到期者);IPC fastpath 典型 600-1200 / worst 6.6k;PI 形 A ≤800ms+ΣCS。

**本件刷新(逐项)**:

| 路 | 旧值(02 号 §② 口径) | 新值/新口径 | 变更依据 |
|---|---|---|---|
| 中断→任务·基线 | ≈830cy | **双态**:涉 EL0 对 ≈830cy 不变;kernel↔kernel 对 ≈710cy | ctxsw 项双态化:EL0 任务出生恒 `fp_needed=true`(`arch/aarch64/src/task.rs:151-153,169`)→ 任一侧 EL0 即 `switch_full`(135 项存续);双 false 走 `switch_no_fp` 快路径(FP 段 = 16 对 stp/ldp + 4 寄存器 ≈70 指令,`arch/aarch64/src/task.rs:203-205` 注——估 ~10-15cy,X6 裁决书口径) |
| 中断→任务·worst | ≈2000cy(1.3µs);前项「最长全屏蔽段」无表 | **链**:EL0 涉 ≈2000cy 不变;kernel 对 ≈1880cy;**全式 = 链 + 最长全屏蔽段(新有表)**:固定族 ≈500cy → **组合 worst ≈2500cy(EL0)/ ≈2380cy(kernel 对),≈1.6-1.7µs @1.5GHz** | 前项 = 本件表 ④ 实时通道;链 = 原分解逐项复核(300/15n/80m 项无刀触动,唯 ctxsw 双态) |
| IPC fastpath | 典型 600-1200 / worst 6.6k(大头 = 跨 AS ctxsw 640B) | **不变**(EL0 涉链全量档存续);纯 kernel 服务对 IPC −~120cy | 跨 AS = EL0 在场 → `switch_full`;同族实测旁证:本件 send/recv fastpath 固定框 ≈276-337cy(TCG 实时) |
| PI 反转 | 形 A ≈200ms / 链式 ≤800ms+ΣCS | **不变** | slice 尺度项,FPSIMD/syscall 窗均不涉 |

**对 02 号推断的修正(重要)**:02 号 §② 注意③「FPSIMD lazy 后非 FP 任务的中断→任务 worst 已低于 2000cy」——实施形把 EL0 任务**出生即置 `fp_needed=true`**(保守档:内核无法证明用户态不用 FP,`arch/aarch64/src/task.rs:151-153`),故「非 FP 任务」实际只覆盖 kernel↔kernel 切换对;RT 任务的主体 population(EL0 cyclictest 形)的链 worst **仍 ≈2000cy**。引用 R3 数字须带此双态口径与落地时点(2026-11-14 X6/P3-5)。

## 来源索引

仓内代码(2026-11-15 工作树直核;本件构建钉 HEAD `f52d834b4` 隔离 worktree,避开并行会话在途改动):

- `kernel/src/syscall.rs`:26-30(SVC 入口硬件屏蔽+唯 send/recv 解蔽模块注)、40-41(v1 IPC 非阻塞)、2966-3004(PreemptSpan+irq_window)、3078/3124(send 解蔽位)、3343-3415(recv 解蔽位×6)、3912-4015(install 全注册表+锁类)
- `arch/aarch64/src/syscall.rs`:18-35(派发表/NR_MAX/NONE 默认槽)、51-88(dispatch + `-> !` 注)
- `arch/aarch64/src/kern_lock.rs`:185-207(SVC_CLASS 位集)、226-257(enter/exit 包络)、355-408(PRINT_LOCK preempt 包络不触 DAIF)
- `sched/src/sched.rs`:189-215(print_window DAIF 分诊)
- `arch/aarch64/src/task.rs`:65-79(fp_needed 字段)、105/115-119(new_kernel false)、151-153/169(new_user true)、181-240(switch_full/switch_no_fp 双路径 asm,FP 段 ≈70 指令注)
- `tools/abi.toml`:31-49(nr_total=38/implemented=37)、325-734(编号全表)
- `tools/bench.py`:32-49(两通道不可比铁律)、1064-1171(run_icount 形态——本探针同构)
- `tools/bench_baseline_icount.txt` + `tools/bench_baseline_icount.notes.md`(基线现值 + 布局骰史档)
- `tools/bench_baseline.txt`(B-1 段 sys_null=63cy——槽地板互证)
- `docs/系统优化路线图-v1.0.md`:36-43(R3 原始定谳表,一手直读)

测量工件(/tmp,本机留存非仓内件):`/tmp/armA2-run1.txt`(icount 终版 42 行,双跑 exact 全文在 `/tmp/sysprobe-harvest-164603.txt`)、`/tmp/rt2-*.s` 与 `/tmp/rt*.s`(实时六跑)、`/tmp/armA.txt`/`/tmp/armB.txt`(敏感性对照两臂)、`/tmp/berryprobe-wt`(隔离 worktree,测毕弃)。

本件推断声明:④ 节三处线性外推(wait_many 帽外推、vmo_rw 顶格半量)、⑤ 节全部未测面的量级估计(退出链/pager 供给/with-waiter send/policy 表拷)、③ 表 thread_create 的敏感性归类——均已随文标注;其余数字均为两通道实测(确定性口径见 § ②)。02 号 FPSIMD 推断修正(§ ⑥)的依据是实施代码的出生档语义(task.rs:169),非重测——ctxsw 双态真值重测留 post-sched 探针(§ ⑤①同载体)。
