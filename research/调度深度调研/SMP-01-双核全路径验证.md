# SMP-01:双核全路径验证——FairBand/EAS/slack/oneshot 在 SMP 下的现态、实证与最小修复

Type: survey
Status: current
Updated: 2026-11-17

> 定位:七轮审计(基线全绿/2138 测零败)后的「新鲜视角」A 面任务——不依赖旧审计结论,直接读代码、跑 `-smp 2` 双核 QEMU、找问题、修 sched 可修面。覆盖:par_enabled 门下特性生效面审计、双核 boot/ktest/LOADGEN 实证、问题清单、两件最小修复(band 跨核 lazy 执法 + parked 核 util 折窗)与 SMPLZ ktest 钉板。
> 方法:sched/src 全 13 模块通读(sched.rs 4180/class.rs 925/band.rs 374/runqueue.rs 525/class 族余件)+ kernel/src/ktest.rs 现役断言面对照 + arch/aarch64/src/smp.rs 与 kernel/src/main.rs boot 时序;实跑 = QEMU virt cortex-a72 TCG 单核与 `-smp 2` 双核各多轮(日志 out/logs/smp01-*.log 在册,基线副本 out/kernels/smp01-baseline);门禁 docs_guard PASS + writable_globals 85 条 ok。
> 证据分级:【证】= 本轮实跑串口输出或一手行号;【推】= 代码面论证未单独跑满;推断标「推断」。行号 = 2026-11-16 本轮工作树(含本刀修复后;修复前行号在 diff 中可对)。
> 纪律:只动 sched/ 与 kernel/src/ktest.rs(任务书指定开刀面)+ 本文档;git 零提交;vnet/tcp owner 文件零碰。并行会话(B 面观测增强)同时在 sched/ 落 OBS-01 钩子——本刀与其同文件不同函数,一次覆盖事故见 §4.5。

---

## 〇、结论速览

| # | 发现 | 定级 | 处置 |
|---|---|---|---|
| F1 | band 带预算**记账已全局原子但环执法单环**:越线核只 throttle 自己的环、翻期 CAS 赢家只解自己的环;残留成员使输家核 pick 恒 None → 深 park(mask CNTV)无 tick → 永不再争夺 CAS = 结构性无限饿死,且残留者仍占环长污染 place() K=2 深度判据 | 正确性(SMP 专属) | **已修**:band.rs `lazy_enforce` + class.rs pick 扫描门(§4.1) |
| F2 | parked 核 util EWMA 冻结(深 park 无 tick = 折叠永不发生),冻结值喂 `any_overutilized`(EAS 全域回退门)与 `eas_place` utils 快照——忙核 util>819 后全员退出 → 恒 overutil → EAS 整域死亡直到该核再跑任务(结构性可能永不) | 正确性/降级(SMP 专属) | **已修**:util.rs `util_idle_fold` + sched.rs 两 park 入口(§4.2) |
| F3 | band tick 面环扫(`FairBand::tick` 的 throttle/unthrottle、`band_period_sweep`)不持本地 RQ 锁,与对核 `try_steal_one`(持本环 RQ 锁 unlink)存在走查撕裂窗——§A113 族 | 竞态(低概率,预存) | **已修**(2026-11-17 F3 刀:`sched::try_rq_lock` 非阻塞窗——锁可用→扫环、不可用→跳本轮,永不阻塞故永不 wedge,§3.3) |
| F4 | EAS 选核机会主义窗口窄:place() 只在 spawn 调用,wake/expire 恒回 home(迁移粘性)——负载形态变化后无重放 | 设计 v1 定谳 | 记录,归 EAS v2(§3.4) |
| F5 | 被限流的 cur 垄断者(单任务独占核)跑到自然让点:pick None 不发生切换,THROTTLE 位只挡「再出选」不挡「继续跑」 | 单核同形(非 SMP 退化) | 已修(§3.5,2026-11-16 F5 刀) |
| F6 | smoke-smp 八相无 FairBand/EAS/slack/oneshot 任何专项相;计划 CP-03(SMPBENCH)/CP-05(BAND-S)未落——07 号「SMP 专属验收相零命中」复核**成立** | 验收缺口 | 本刀补决策面钉板(SMPLZ)+双核 boot 零回归;专项相归计划线(§5) |

实证速览:`-smp 2` 下 ktest 全绿(BANDM4 ok n=2/LOADGEN ok 四组/EASPROD ok/SMPLZ ok n=3)、boot 32 标记全齐 + `SMP ok ncores=2` 恰一次、PAR ok c0=22 c1=33(真并行见证)、偷取/定时拉取/双核心跳全活(§2)。

---

## 一、SMP 现态审计:par_enabled 门下哪些特性生效

### 1.1 门与 boot 时序(ktest 的 SMP 语义边界)

boot 时序【证】:`ktest::run()` 在 kernel/src/main.rs:769,`smp_bringup` 在 :2214,`topo_smp_online`+`par_enable` 在 :2241-2242——**ktest 全程先于副核参与**,par 门恒关(ktest.rs:9724-9727 自己的留注亦证)。推论:

- ktest 一切断言(BANDM4/EASPROD/LOADGEN/SMPLZ)在 `-smp 2` 下跑的是**单核语义决策面**——副核此时在 secondary_main 探测环周期醒,on_irq_tick 入口早退门(`!par_enabled() && cpu_id() != 0`,sched.rs:3592)保证零共享触达。
- par 门开启后副核经 park_recheck(sched.rs:3958)承接派发;`PER_CPU_RQ: [Runqueue; 2]`(sched.rs:229)按核分诊,单核形态恒 `[0]` 等价旧全局队列(零漂移根基)。

### 1.2 特性逐项生效面

**FairBand 差异化在 SMP 下:生效,语义 per-task per-dispatch,无跨核耦合态。**链路:class_of_task per-task 动态派发(class.rs:887,intent_effective 即带键);授片走 `TaskClass::grant_cycles`(class.rs:859,Band 臂 = 本带 quantum_ns 换算周期数)——sched.rs 全部授片点(yield:1433/exit:1616/tick:3822/SGI:3914/park 派发:4137/idle 派发:4175/try_preempt:3028/block:2676 均为 per-task 分派);tick 记账基准 = per-CPU `SmpCpu.last_vct`(class.rs:269,TPIDR_EL1 寻址本核独占)。结论:quantum 在 per-CPU runqueue 中的语义 = **每次派发按任务带参数本核授片**,双核各授各的、账面(Σvruntime per-queue RMW,runqueue.rs:40-50 多写者 fetch_add/fetch_sub 已修)自洽——无「带 quantum 跨核漂移」面。带预算**记账**面自 SMPD-3 起全局原子(band.rs:75 CONSUMED/band.rs:82 DEADLINES——跨核一份,首越线 fetch_add 单赢家、翻期 CAS 防双翻),但**环执法**面是 F1 缺口(§3.1)。

**EAS pick_cpu 在 SMP 下:决策链全通,消费面 = spawn-only。**place() 四与门(sched.rs:757-760:ttbr0≠0 ∧ pinned≠0 ∧ spread ∧ par_enabled)——只有 EL0 计算线程(thread_start spread=true,kernel/src/syscall.rs:1705-1707)与显式 spread 内核任务(main.rs:887/894 spawn_as_spread)可路由上核;EAS 首接(eas_place→eas_pick_place,hint→prev 映射→berry-em 两段选核)→ None 回退 K=2 深度失衡(sched.rs:762-771)。utils 源 = per-CPU EWMA(util.rs:38)——即 F2 的失真入口。ktest EASPROD(ktest.rs:9745 起)钉决策面三源端到端(SVC COMMIT→policy 表→eas_place 消费,含 overutil 回退),place() 路由面按其留注归 SMP 相——本刀以双核 boot + B 面观测线补真机证据(§2.3)。

**timer slack 在多核下:无竞态,已验证。**顺延在 block_current 入队点一次落定(sched.rs:2611 `timer_slack_defer`——LATE-only,intent≥UTILITY 档才顺延);SLEEPQ 全局升序单队列,SQ 自旋锁串行一切摘队(expire_timeouts/wake 族),两核对同一 deadline 双 arm 至多一次冗余醒、锁内 expire 串行无撕(sched.rs:3362-3364 既有论证)。LOADGEN A3/A4(slack=10ms 组)在 `-smp 2` 下实跑通过【证】。

**oneshot 收紧律在多核下:两臂各自独立 CVAL,无冲突。**忙核两臂:tick 入口(sched.rs:3617,tighten_deadline(nearest,armed) 只收紧不推迟)+ block 忙核授片臂(sched.rs:2692);idle/park 两臂按最近 deadline arm_at(sched.rs:3372 起 + main.rs:5545)或尽空 mask。每核 CVAL 独立编程,SGI 醒由 park_recheck 承接(main.rs:5490-5494 的 noSGI 150s 自续性实证注)。oneshot law 本体纯函数(oneshot.rs:32-37,host 钉板)。未发现 SMP 退化面【推——代码面论证 + 双核长跑零 oneshot 相关红】。

**PM util 在 SMP 下:记账正确,深 park 冻结 = F2(已修)。**写点 = 本核 tick(util_accumulate/util_tick_window,class.rs:312-315 调用),idle tick 不累计但翻窗(空窗折叠低 util);缺口在「不 tick」的深 park 态——见 §3.2。

### 1.3 带任务上 CPU1 的可达性(F1 的前提)

【证】`sys_thread_start`(kernel/src/syscall.rs:1664)以 spread=true 装配(kernel/src/syscall.rs:1705-1707「spread=true(b-1 判谳):同进程计算线程 = 铺核面」),且 `apply_sched_attr_to_task`(syscall.rs:1623,thunk 调用点 :1741)把 SchedAttr.intent 落 intent_declared/effective(syscall.rs:1625-1630)。即 **EL0 经标准 ABI 就能造出「intent∈{0x02,0x03} 且 spread=true」的带任务**,place() 的 EAS/K=2 可将其路由上 rq[1]——F1 不是理论面,是生产可达面。

---

## 二、SMP 双核 QEMU 实证

机型:qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2,内核 = 本刀修复后工作树构建(target-smp01,含 B 面 OBS-01 在途钩子)。日志:out/logs/smp01-final-1794815812.log(150s)/smp01-boot-1794814027.log(修复前基线,150s)/smp01-up 族(单核)。

### 2.1 boot 标记到齐(含 SMP ok)

【证】smoke-smp 相 1 的 32 标记(MARKERS 数组,tools/markers.sh:18)离线对拍 **32/32 全齐**;`SMP ok ncores=2` 恰一次(:149)、`SGI ok n=` 恰一次(:150)、SGI2/SGI3 族全落(:150-153)、`PAR gate on`(:155)。

### 2.2 ktest 与 LOADGEN 在 SMP 下

【证】`BANDM4 ok n=2`(:64)、`LOADGEN ok`(四组 A1-A4 + cmp 行,:75-84)、`EASPROD ok`(:85)、`SMPLZ ok n=3`(:86,本刀新增)、`ktest done`(:87)。ktest 与基线日志的前 87 行 diff 仅时序噪声(PERF policy base=12→10、LOADGEN p50 ±2%量级),零结构漂移【证】。

### 2.3 路径证据(双核全路径活的直接证)

【证】`sS2b: steal #1 'task_b'`(:160,CPU1 从 rq0 偷取)+ `sS6: pull #1 'task_a'`(:164,slice 燃尽支定时拉取);`PAR ok c0=22 c1=33`(:246,两核各跑过 ≥2 EL0 tick + 双核重叠执行不同 EL0 任务的粘性见证);hb 心跳双爬(c0/c1 同步增长至 2000+);switches 金丝雀爬至 40000+ 且 kl(KERN_LOCK 争用)低位;`EAS place cpu=-(K=2) hint=0 u=714/670 spare=310/354`(:227,B 面 OBS-01 周期决策线——EAS 无 hint 时 K=2 回退的现役证据)。`stuck` 7 条(it_*/dpg_* 年龄 82s)= ktest 遗留睡眠者超 10s 的既有 S-6 诊断行,基线与修复两跑**同数同形**,非本刀新增【证】。

### 2.4 环境限制(诚实记录)

smoke-smp 八相全量(tools/smoke-smp.sh)本轮未能起跑:同机 QEMU 字段被并行会话长跑占用(berry-live GUI 实例 + 探针实例),field_wait 5×15s 规则会 loud 退。相 1 断言形态已离线成立(§2.1);相 3-8(LB/ASIDR/TLBS/ALLOC/HT/PP)未复核,归后续(§5)。

---

## 三、问题发现(新鲜视角直查,不引旧判词)

### 3.1 F1:band 跨核执法缝隙 → 翻期残留 → 深 park 结构性饿死(已修)

推理链(全部一手行号):带预算 consume 是全局原子账单赢家判定(band.rs:169-186),但赢家的 `throttle_all(band, rq)` 只扫**越线核自己的**环(class.rs:772 调用点带 `rq()` = 本核)——对核环上的同带成员照跑(全局配额、局部执法)。翻期侧:`period_check` CAS 单赢家(band.rs:230-233),赢家的 unthrottle_all 同样只解本环(class.rs:912-918 band_period_sweep);输家核的成员 THROTTLE 残留。**残留如何变成永久**:残留成员在环上但不 eligible(ringface eligible 的 THROTTLE 过滤,ringface/src/lib.rs:114-116)→ 输家核 pick 恒 None → idle/park 走 mask 深 park(sched.rs:3370-3376,main.rs:5542-5549)→ 该核无 tick → `band_period_sweep`(tick 入口,sched.rs:3625)永不再在其上运行 → 永不再争夺翻期 CAS;且残留者仍计入 rq len(sched.rs:3772 的 K=2 判据读 len)——**死重被当负载**,CPU0 的 place() 因此继续自留新任务,SGI 不发,深 park 无醒源。class.rs:903-905 旧注只承认「跨核残留留刀」,未识别深 park 使残留从「迟一期」升级为「无人再解」——这是本轮新鲜视角的新增量。

### 3.2 F2:parked 核 util 冻结喂跨核读者(已修)

深 park(mask CNTV)无 tick ⇒ `util_tick_window` 折叠永不发生(util.rs:69-92 只在 tick 面)⇒ EWMA 冻结在最后值。两个跨核读者受害:`any_overutilized`(util.rs:127,EAS 全域回退门)——核忙至 util>819 后任务全退 → 深 park → 冻结值使全域恒 overutil → `policy::placement_hint` 恒 0(policy.rs:474)且 `eas_place` 恒 None(eas_pick_place 的 overutil 门,sched.rs:833-841)——EAS 整域死亡;`eas_place` 的 utils 快照(sched.rs:854)同受冻结失真。单核形态无此面(idle_loop 单核分支 park 前无跨核读者)——SMP 专属。

### 3.3 F3:band tick 面环扫与对核偷取的撕裂窗(已修,2026-11-17 try 锁窗)

`FairBand::tick` 的 throttle_all/unthrottle_all 与 tick 入口 `band_period_sweep` 都在**不持本地 RQ 锁**的屏蔽窗内扫本环(band.rs:245-267/277-297 的 Safety 契约只写「屏蔽窗内」);而对核 `try_steal_one` 持本环 RQ 锁 unlink(sched.rs:4054-4102,run_unlink 对被摘节点双字清 None,ringface/src/ilist.rs:193-209)——扫走查的 `.expect("环上节点必有 next")` 可能读到撕态。修法直觉是「tick 面把 band 扫收入本核 RQ 锁窗」,但 tick 取 RQ 锁有实测 wedge 判例(SVC 窗入口预持 RQ0+RQ1 跨核忙等 + tick 抢锁成环,sched.rs:3667-3675 沿革注)——**F1 轮不碰**,归 CP-05/BAND-S 相的锁面统一裁决(F1 轮的 lazy 面在 pick 调用者已持的本核 RQ 锁窗内执行,恰是安全侧)。

**修复(2026-11-17 F3 刀,sched/ 内五文件,决策记录见 `.agents/notes/implemented/2026-11-17-SMP遗留F3收口-band环扫try锁窗.md`)**:try_lock 非阻塞形态绕开 wedge 判例——`SpinLock::try_acquire`(TTAS 同源 CAS 去自旋回廊)→ `Runqueue::try_lock_acquire` → `sched::rq_try_acquire`(lockdep 只记成功物理取)+ `try_rq_lock_of`(位感知退化与 rq_lock_of 对称)+ pub `try_rq_lock`/`try_rq_unlock`(pi_rq_lock 先例形态)。两个 tick 调用点同面收口:①tick 入口 `band_period_sweep` 整体入 try 窗,失败跳过整轮(「镜真位真」自洽,下轮追上);②`FairBand::tick` 的 unthrottle/throttle 环执法步(含 rearm_members_vd 走查)各自入 try 窗,period_check/consume 原子记账留窗外照常落账(跳环扫不跳记账——consume 首越线全局恰一次)。**永不阻塞 ⇒ 永不加入忙等环 ⇒ 永不 wedge**(失败是「迟一轮」不是「死」);跳过的兜底链 = 幂等追认(锁窗 ns 级 vs tick 边界)+ F1 lazy_enforce 的 pick 扫描补位。单核无对核持有者 try 恒成功,行为逐位零漂移(ktest THRYLD/BANDM4 的「越线 tick 须置位」断言即现役钉板;ktest 期先于 timer::init/enable_irq,无同核重入面)。

### 3.4 F4:EAS 机会主义窗口窄(记录)

place() 只在 spawn 调用(sched.rs:755 注:「wake/expire 再入队回 home 不改道——防 sleeper 被唤醒方弹核」);偷取迁移是唯一的被动再平衡(随迁改写 home,sched.rs:4102)。负载形态剧变后无主动重放——Linux EAS 的 task placement 在 wake 路径每机会重判,此处 v1 收窄为 spawn-only + 偷取兜底。设计定谳在案,非缺陷;EAS v2 议。

### 3.5 F5:限流 cur 垄断者跑到自然让点(已修,单核同形)

THROTTLE 位使任务不可再出选,但单任务独占核时 pick None ⇒ on_irq_tick 不切换(sched.rs:3806-3830 的 go=None 路)⇒ cur 继续跑到 block/yield/exit。单核同形(非 SMP 退化)。

**修复(2026-11-16 F5 刀,判据/切换舞/醒源三件,sched/ 内)**:① 判据单源 `sched::throttle_yield_due(cur)`(HELD_THROTTLE 位——band 越线与 S5-c 逐任务预算两位写者的统一面);② on_irq_tick 片燃尽支出选 None 臂强制让点(RQ 窗内 cur Ready 化——在环保留不摘环;CPU0 切 IDLE_TASK/idle 环热旋支承接翻期,副核 set_current(null) 切 sec_park_ctx 由 park_recheck 承接,均 block/exit pick-None 臂同形);③ 醒源护栏 `band::nearest_throttle_wake`(armed+throttled 槽的 DEADLINES 最小值)并入 `nearest_deadline`——缺此面深 park mask CNTV 无 tick,让点变永久饿死(sweep 永不再跑)。ktest `THRYLD ok n=4` 钉决策面四面〔2026-11-18 醒源刀后 n=5——T5 增逐任务面〕;连带 dpg_press 硬 panic 门扩「带执法在飞降软」(any_armed——F5 使 utility 带进入真实配额占空比,翻期整批回场 Burst 合法推高缺页供给墙钟,单核静默场前提与 SMP 形同构性破,详 note)。

### 3.6 F6:验收面缺口复核

smoke-smp 八相(相 1-8 = 裸相/rootfs/LB/ASIDR/TLBS/ALLOC/HT/PP,tools/smoke-smp.sh:2-8)不含任何 FairBand/EAS/slack/oneshot 专项断言;计划文档(research/调度彻底计划/03-彻底计划骨架-刀序里程碑验证体系-v1.0.md:65-70)的 CP-03 `SMPBENCH ok`、CP-05 `BAND-S ok`(超卖窗上界断言)均未落地——07 号「SMP 专属验收相零命中」在本轮代码态复核**成立**。

---

## 四、修复(最小面,全部 sched/ + ktest 断言)

### 4.1 F1 → band lazy 执法(band.rs + class.rs)

机制(与 S5-c `lazy_unthrottle_deadlines` 同消费点形态):pick 扫描(`Fair::scan_filtered`,class.rs:459——生产八调用点全经此,且全在本核 RQ 锁窗内 = 环归属核执行)逐节点双向对账 `lazy_enforce`(band.rs:350):槽 throttled 镜像真 ⇒ 补置 THROTTLE(跨环执法缝隙收口);镜像假 ⇒ 清残留(翻期残留愈合——含深 park 醒后首扫自愈:SGI 醒 → park_recheck → pick 扫 → 残留清)。扫描门 = `any_armed`(band.rs:156,ARMED 计数 band.rs:89 由 configure band.rs:143-153 维护):零武装(unit 单带负载常态)时一次 Relaxed 原子读 + 逐节点一个分支,零执法动作。语义对齐:幂等/deadline 豁免(band_member 单源)/FROZEN 不碰——与 throttle_all/unthrottle_all 逐位一致;VD 重臂不在此面(残留者 VD 已是 low_weight 重臂值,EEVDF vruntime 前进兜底不饿死;high_weight 回场优势损失 = 已知代价,band.rs 段注明示)。旧行注同步翻新(class.rs:903-905 挂账 → 已收口注)。

### 4.2 F2 → parked 核 util 折窗(util.rs + sched.rs)

`util_idle_fold`(util.rs:162):park 入口无条件折一窗——accum 取走真值(park 前片段有运行账如实入 raw)、elapsed = 距上次翻窗实差、数学面与 util_tick_window 的 Fold 分支同源(ewma_fold 单源),仅去「窗口到期」门(park = 对即将无限期空闲的自证,等到期 = 等一个不再来的 tick,正是缺口本身)。逐 park 环 pass 各折一步(α=1/8)单调向 0。接线两处:sched.rs:3966(park_recheck 门后,CPU1 侧)+ sched.rs:3369(idle_loop SMP 分支皆空臂,CPU0 侧)——各 park pass 恰一次;单核形态两处均不可达(par 门),ktest 期 park_recheck 早退于折窗之前——**零漂移**。

### 4.3 SMPLZ ktest 钉板(kernel/src/ktest.rs:9943 起,末位新号)

`test_smp_lazy` 三段:L2 lazy 双向(越线补位/翻期残留愈合/deadline 豁免/unit 非成员不动/武装门关带零动作——以「越线只动账不动环」「翻期只 CAS 不扫环」分别模拟对核视角);L3 折窗衰减律(泵到 350 带 → 折窗单调降至量化死区 <8,含未开窗零动作与 TCG 时钟粒度失速容忍);L4 带参数真源复证(修复不碰带表)。输出行 `SMPLZ ok n=3`(ktest.rs:10115)——与 markers.sh 全表 grep -F 互查零子串碰撞(32/56 标记计数不受扰)。

### 4.4 验证矩阵

【证】单核 boot:SMPLZ ok + 既有全绿(BANDM4/LOADGEN/EASPROD/ktest done),ktest 段与基线 diff 仅时序噪声;`-smp 2` boot:同全绿 + SMP ok/SGI/PAR ok/steal/pull/EAS 决策线(§2)。门禁:docs_guard PASS(0 警)、writable_globals ok(85 条——本刀零新增 static mut,ARMED 为原子 static)。

### 4.5 并行会话事故记录(诚实面)

B 面会话(OBS-01)与本刀同时在 sched/ 施工:15:23-15:24 其落 stats 钩子(class.rs pick 记账/sched.rs SCHED 行/EAS 日志升格为周期线);15:49 一次外部 git 操作瞬时覆盖 sched.rs,本刀两处 park 折窗钩子被抹(band/class/util/lib/ktest 完好),15:52 发觉后重读现行区域重落并全量重建重验(§4.4 的终验即重落后二进制)。教训:同文件多会话施工须每步 grep 自证编辑在位,构建前 touch 强制重编。

---

## 五、遗留与移交

- **F3**(band tick 面环扫锁窗):已于 2026-11-17 收口——`sched::try_rq_lock` 非阻塞窗(锁可用→扫环、不可用→跳本轮;永不阻塞故永不 wedge,兜底 = lazy_enforce pick 面),详见 §3.3 修复段与决策记录;CP-05/BAND-S 的锁序全局裁决若另立方案可再议,但 tick 面免锁 band 环扫已不存在。
- **专项验收相**:CP-03 SMPBENCH(跨核唤醒延迟 p50/p99 对拍)与 CP-05 BAND-S(超卖窗上界 = 一 slice 内每核各一份 delta,进断言)未落;本刀 SMPLZ 只钉决策面。
- **F4**:部分收口(2026-11-17 补记)——wake 路重放+band 偷取已落(c1f695be,沿革撞号让号后=五百二十九),双门判定律单源化(cfdf94af9,沿革五百一十四);EAS v2 全量相仍开放待议。F5(限流 monopolist 强制让点)已于 2026-11-16 F5 刀收口(§3.5)。
- **smoke-smp 八相全量**:本轮受同机 QEMU 字段占用未跑,待字段空闲补跑(相 1 形态已离线成立)。
- 观测面(06 号扣分项的 B 面)由并行会话 OBS-01 承担(per-band pick/runtime、per-intent 吞吐、SCHED 周期行、EAS 决策周期线——本刀双核日志已见其生效),本报告不双写其结论。
