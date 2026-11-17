# SMP-01 F3 收口:band tick 面环扫入 RQ 锁 try 窗(永不阻塞故永不 wedge)

Status: implemented

## Problem

SMP-01 §3.3 定级的 F3:`FairBand::tick` 的 throttle_all/unthrottle_all 与 tick 入口 `band_period_sweep` 都在**不持本环 RQ 锁**的屏蔽窗内扫本环,而对核 `try_steal_one` 持本环 RQ 锁 unlink(被摘节点 prev/next 双字清 None,ringface `run_unlink`)——免锁走查可读到半摘节点:next 暂 None 时环走查静默截断(漏执法);更险的形态是摘走走查快照 head 后残子环自旋(band.rs 环扫无步数帽,两元环被摘一者后余者自指)可挂死 tick【推:代码面论证,竞态窗低概率未能确定性复现】。直觉修法「tick 面把 band 扫收入 `rq_lock` 阻塞窗」被实测 wedge 判例否决:SVC 窗入口预持双 RQ 跨核忙等,tick(IRQ 上下文)自旋抢锁 = 与对核「持锁等本核」成环,7.7K 切换即 wedge(runqueue.rs `sum_vruntime` 字段注/sched.rs `Fair::tick` 调用点注沿革一八五)——F1/F2 刀因此把 F3 挂账 CP-05/BAND-S 锁面裁决。

## Decision

- **try_lock 非阻塞形态绕开判例,不做锁序正面裁决**:锁可用→持有→扫环(撕裂窗闭合);不可用→立即让路跳过本轮。永不阻塞 = 永不加入忙等环 = 永不 wedge——失败是「迟一轮」不是「死」。
- 落点四件(自底向上):`spin.rs::SpinLock::try_acquire`(TTAS 同源 CAS 去自旋回廊);`runqueue.rs::Runqueue::try_lock_acquire`(持有语义与 lock_acquire 逐位同,唯一差异 = 失败让路);`sched.rs::rq_try_acquire`(lockdep 只记成功物理取)+ `try_rq_lock_of`(位感知退化支与 `rq_lock_of` 对称)+ pub 安全包装 `try_rq_lock`/`try_rq_unlock`(`pi_rq_lock` 先例形态,class 面出口)。
- **两个 tick 面调用点同面收口**:①`on_irq_tick` 入口的 `band_period_sweep` 整体入 try 窗,失败跳过整轮 sweep(翻期 CAS 与环扫同跳——状态「镜真位真」自洽,下轮 tick 追上);②`FairBand::tick` 的 ②③ 环执法步(unthrottle_all/throttle_all 含 `rearm_members_vd` 走查)各自入 try 窗,而 period_check/consume 的原子记账/翻期面**留在窗外照常落账**——跳的是环扫不是记账(consume 首越线全局恰一次,跳记账 = 永久漏执法)。
- 跳过可接受的兜底链:幂等追认制(RQ 锁持有窗都是 ns 级临界段,连续跨多个 tick 边界全失败测度趋零)+ F1 `lazy_enforce` 已在本核出选的 pick 扫描(RQ 锁窗内)逐节点双向对账——限流补位/翻期清残留不依赖 tick 环扫。
- ktest 零新增的根基:ktest 全程先于 `timer::init`(main.rs:992 < :1051)与 enable_irq,IRQ 屏蔽无重入面;TPIDR_EL1=0(`cpu_id`→0)且 PER_CPU_RQ 为 const 静态,锁无竞争恒成功——THRYLD「越线 tick 须置位 cur 限流位」/BANDM4 断言即「单核 try 恒成功→扫环照跑」的现役钉板,单核行为逐位零漂移。

## Alternatives considered

- tick 面 `rq_lock` 阻塞窗(F1 刀挂账的「完整修」):wedge 判例直接否决;本 try 形即其「不等待」变体。CP-05/BAND-S 若另裁锁序全局方案可再议,但本修后 tick 面已无免锁 band 环扫,该相紧迫性消解。
- 站点 A 拆窗(period_check 先行、仅环扫入 try):翻期 CAS 赢家清 throttled 镜像后若环扫被跳,「镜假位真」残留只能靠 lazy_enforce 愈合——而全限流环 pick 恒 None 时本核扫描不发生(SMP-01 §3.1 深 park 形态);整窗跳过保自洽,更稳。
- ktest 面为本地环跳过真锁(指针判 rq 是否 PER_CPU_RQ):多余——ktest 期 IRQ 未启无重入面,真锁恒成功,取放即还。

## Consequences

- F3 撕裂窗闭合:tick 面一切 band 环扫(steal 的 unlink 对面)均在持锁窗内执行;SMP-01 §〇/§3.3/§五 的 F3 条目同批翻「已修」。
- 延迟语义:对核持锁窗内 tick 到期的 band 执法至多迟一轮 tick(或至下一次 pick 的 lazy_enforce 兜底),F5 强制让点 + `nearest_throttle_wake` 醒源面下无饿死面。
- 验证:单核 QEMU ktest 全绿(BANDM4 ok n=2/SMPLZ ok n=3/THRYLD ok n=4/LOADGEN/EASPROD/BENCH/ktest done;dpg.rs:1016 诊断 panic 为基线日志同形间歇项,f5-run-4..8 在案);`-smp 2` 双核 boot 见 SMP-01 §2 补记。
- 零新增 static mut(writable_globals 面零变化);lockdep 语义保持(只在成功物理取放点记)。

## Related

- research/调度深度调研/SMP-01-双核全路径验证.md §3.3(F3 定级)/§4.1(F1 lazy 执法——本修兜底链)
- [2026-11-16-SMP双核全路径-band跨环lazy执法与park核util折窗.md](2026-11-16-SMP双核全路径-band跨环lazy执法与park核util折窗.md)(F1/F2 刀——F3 挂账源头)
- sched/src/spin.rs try_acquire;sched/src/runqueue.rs try_lock_acquire;sched/src/sched.rs rq_try_acquire/try_rq_lock_of/try_rq_lock;sched/src/class.rs FairBand::tick;sched/src/band.rs throttle_all Safety 契约
