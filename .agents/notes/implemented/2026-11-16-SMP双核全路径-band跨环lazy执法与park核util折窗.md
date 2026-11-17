# SMP 双核全路径验证:band 跨环执法走 lazy-pick、parked 核 util 走入口折窗

Status: implemented

## Problem

SMP-01 双核全路径验证(research/调度深度调研/SMP-01-双核全路径验证.md)直查发现两个 SMP 专属缺口,均有生产可达前提(EL0 thread_start spread=true + SchedAttr.intent 经标准 ABI 即可造出带上 CPU1 的带任务,kernel/src/syscall.rs:1705-1741):

1. band 带预算记账已全局原子(band.rs CONSUMED/DEADLINES),但环执法单环——越线核只 throttle 自己的环、翻期 CAS 赢家只解自己的环;更深的洞 = 输家核残留成员使其 pick 恒 None → 深 park(mask CNTV)无 tick → 翻期扫永不再在该核运行 → 残留者结构性无限饿死,且其仍占 rq len 污染 place() K=2 路由。
2. 深 park 核无 tick ⇒ util EWMA 冻结,冻结值喂 any_overutilized(EAS 全域回退门)与 eas_place utils 快照——忙核 util>819 后全员退出可令 EAS 整域死亡。

## Decision

- 缺口 1 修在 pick 扫描:`Fair::scan_filtered` 逐节点 `band::lazy_enforce`(槽 throttled 镜像双向对账——真则补置 THROTTLE、假则清残留),`any_armed` 原子门保零武装负载零成本。选 pick 而非 tick 的原因:pick 调用者已持本核 RQ 锁(环归属核执行,无 §A113 撕裂窗),而 tick 面取 RQ 锁有双核 wedge 实测判例(SVC 窗预持 RQ0+RQ1 跨核忙等成环)。类内旧「SMP lazy-pick 执法刀」挂账注同步翻新。
- 缺口 2 修在 park 入口:`util_idle_fold` 无条件折一窗(accum 取真值、elapsed 实差、ewma_fold 同源数学,仅去「窗口到期」门——park 即对无限期空闲的自证),接线 park_recheck 门后(CPU1)+ idle_loop SMP 皆空臂(CPU0),单核/ktest 期两处均不可达(零漂移)。
- ktest 新增 test_smp_lazy(SMPLZ ok n=3):lazy 双向/豁免/武装门 + 折窗衰减律(泵 350 带 → 单调降至量化死区 <8,TCG 时钟粒度失速容忍)决策面钉板;SMP 路由面归 smoke-smp。

## Alternatives considered

- tick 面 band_period_sweep 扫双环(tick 取对核 RQ 锁):被 wedge 判例直接否决——SVC 窗预持双 RQ 跨核忙等 + tick 抢锁成环(实测 7.7K 切换 wedge 在案)。
- 输家核翻期后无条件 unthrottle_all 本环:无法区分「本期合法限流」与「上期残留」,会提前解限;lazy 以槽 throttled 镜像区分,单点判定。
- util 冻结修在唤醒首 tick:park 不醒就永远不折——缺口本体;park 入口折窗让冻结窗长度 = 0。
- tick 面 band 扫描收入本核 RQ 锁窗(F3 撕裂窗的完整修):涉及 tick/锁序全局裁决,归 CP-05/BAND-S,本轮只记录不修。

## Consequences

- 带任务跨核执法缝隙与残留饿死窗闭合;深 park 醒后首扫自愈(SGI 醒 → park_recheck → pick 扫 → 残留清)。
- 已知代价:残留愈合路径成员 VD 仍是 low_weight 重臂值(不重臂 high_weight),EEVDF vruntime 前进兜底不饿死,回场优势损失在 band.rs 注明示。
- 验证:单核与 -smp 2 双形态 boot 全绿(SMPLZ ok/ktest done/SMP ok ncores=2/PAR ok/steal/pull),ktest 段与基线 diff 仅时序噪声;docs_guard PASS、writable_globals ok(零新增 static mut)。
- 遗留:F3(band tick 面环扫无锁 vs 对核偷取的撕裂窗)已于 2026-11-17 以 try 非阻塞锁窗收口(见[2026-11-17-SMP遗留F3收口-band环扫try锁窗.md](2026-11-17-SMP遗留F3收口-band环扫try锁窗.md));CP-03 SMPBENCH/CP-05 BAND-S 专项验收相未落。
- 事故记录:与 B 面观测会话并行施工期间,sched.rs 两处 park 钩子被一次外部 git 操作瞬时覆盖(15:49),发觉后重落并全量重建重验——同文件多会话施工须每步 grep 自证 + 构建前 touch 强制重编。

## Related

- research/调度深度调研/SMP-01-双核全路径验证.md(本轮产出,问题全链与行号)
- research/调度彻底计划/03-彻底计划骨架-刀序里程碑验证体系-v1.0.md(CP-02..07 SMPD 线)
- sched/src/band.rs lazy_enforce/any_armed;sched/src/util.rs util_idle_fold;sched/src/class.rs scan_filtered 钩子;kernel/src/ktest.rs test_smp_lazy
