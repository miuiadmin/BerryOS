# SMP 遗留 band 偷取收口:try_steal_one 三档带偏好序

Status: implemented

## Problem

SMP-01 遗留之一(本刀任务书指认;报告见 [SMP-01 双核全路径验证](../../../research/调度深度调研/SMP-01-双核全路径验证.md),前批 A 面两修见 [SMP 双核全路径笔记](2026-11-16-SMP双核全路径-band跨环lazy执法与park核util折窗.md)):try_steal_one 的 victim 过滤(W3-Q82 `berry_balface::steal_filter` 三值合取:spread/ttbr0/eligible)不看带——utility 任务可能被偷到 interactive 密集核(挤占响应敏感负载的供给),interactive 任务也可能被偷离原核(丢响应局部性)。同报告另一遗留 F4(wake 路径 EAS 重放)归并行 lane——其 policy 侧 `placement_hint_ungated` 已在该 lane 在途(sched/src/policy.rs 工作树态,消费面指名 `eas_wake_replay`),本刀不越界。

## Decision

- **三档偏好序** `steal_band_rank(band, tgt_int, tgt_util)`(sched/src/sched.rs 纯决策面,ktest 钉板对象——`eas_pick_place` 同款纪律):interactive(0x02)=保护档(通常不偷,留原核保响应);utility(0x03)→interactive 密集核=降优先档(兜底——比偷 interactive 或空手而归优);其余(unit/无带/utility→非密集核)=兼容档见首即偷。
- **「interactive 密集」判据** = 目标核(偷取目的地 = 本核)环上两现役带在环数比较 `int > util`——本核 RQ 短锁窗内走查计数(cur-in-ring 不变量使当前任务在计;throttled/frozen 在环保留也在计 = 潜在 interactive 负载;空环〔idle 偷取常态〕= 不密集——空闲核无响应供给之争,utility 入住无害)。快照窗与远端扫描窗不重叠(先放本核锁再取远端——「只持远端」绝不双持的既有特则保持)。
- **环扫选优**:try_steal_one 从「首 eligible 即 victim」升级「最小档胜,同档环序先者胜(严格更优才替换——旧公平形保形)」,兼容档早退 = 无带/unit 负载零漂移(boot task_a/task_b 旧形逐位不变)。带的读法单源 `class_of_task`(intent_effective 查表 → FairBand.band_id;非带任务 = 0)。
- **ktest** `STEALBND ok n=3`(kernel/src/ktest.rs,末位新号纪律):判定律全向 + 真带任务 spawn_intent→class_of_task 派生 band_id + 任务书指定场景:CPU0 环 [int, util] → CPU1 只有 int(密集)→ 偷取应选 util——保护档硬于降优先档的优先序钉板,另附环扫 fold 镜像四面变体(非密集目标核/unit 档序反超环序/同档环序先者/孤立 interactive 末档兜底)。

## Alternatives considered

- 硬跳过(utility→密集与 interactive 双 hard skip):任务书验收场景反证——victim 环 [int, util] 面对密集核时双跳会空手而归,而正确答案是把 util 迁走(留 int 保响应是更硬约束);三档偏好序同时兑现「跳过(降优先)」与「应选 util」两面,interactive 保留末档兜底(「通常不偷」非绝不偷——idle 核空手不如偷)。
- 密集记账挂 runqueue.rs 结构层(仿 DL_ON_RING 的 per-RQ int 计数账本):多一处账本漂移面且 link/unlink 全部变更点要背书;短锁窗快照零结构改动,成本 = 一次本核锁对 + O(环长)(偷取本就扫对核环,量级同阶)。
- 偷取判密集读全局 band stats(stats.rs BAND_RUNTIME):stats 设计定谳「跨核一份全局账、非 per-CPU 决策输入」;per-core 带分布无现成记账,新造不如环快照直接。
- rank 律外迁 berry-balface(steal_filter 的 W3-Q82 同款纪律):本刀任务书文件域限 sched/class 两文件,且 eas_pick_place 纯决策面留 sched.rs 的既有先例在先(ktest 直调面同形);律若后续变复杂(域层级/多核泛化)再迁不迟。
- **F4 姊妹形实现后撤**:本刀曾按任务书头部一行实现 wake 路径条件性 EAS 重放(`eas_wake_rehome`:四道门 = place() 资格四与门 + policy hint 门 + overutil 门 + eas_pick_cpu 平局粘 prev=home,hint 门保 b-1「防 sleeper 被唤醒方弹核」零漂移,expire/wake 两再入队点共用,EASWAKE ktest 四面钉板),实现并双 QEMU 验证全绿后发现并行 lane 正在做同名修复(policy.rs `placement_hint_ungated` 已落盘,消费面指名 `eas_wake_replay`/`eas_wake_pick`——负载动机迁移形态,overutil 语义与姊妹形相反:wake 面刻意不吃 overutil 让位而以 home>80% 为触发)——为免同调用点双机制,F4 整体撤出归该 lane,本刀只保 band 偷取。姊妹形的可复用判据(prev=home 平局粘核防乒乓、hint 门窄形零漂移、wake 决策日志独立计数保 spawn 面节奏)已留本记录备该 lane 参酌。

## Consequences

- 无带/unit 负载全路径零漂移:偷取兼容档早退逐位保旧形;单核形态 par 门首行早退(单核零改)。
- 验证实跑(QEMU 带 timeout):单核 boot ktest 全套零回归(ktest done + STEALBND ok n=3 + BANDM4/SMPLZ/EASPROD/THRYLD 等全套 ok 行在席,零 panic);双核 -smp 2 裸 boot 过改后偷取路径健康(SMP ok ncores=2 + sS2b: steal #1 'task_b' + sS6: pull #1 'task_a' + PAR ok——真偷取/定时拉取两臂均见证)。
- 已知边界(标注为推断/容忍):密集快照与远端扫描间隙的对核唤醒入环竞态最多翻转边界档(int==util)一档,软偏好输入容忍;密集判据 int>util 是「分布比较」的最小形,阈值化(如带 runtime 份额)留后续若现误判再议。
- F4 收口状态以并行 lane 为准(其 policy 侧已开工);本刀与其零文件交集(policy.rs 归该 lane,本刀 sched.rs 改动仅偷取区与导出面)。

## Related

- [SMP-01 双核全路径验证](../../../research/调度深度调研/SMP-01-双核全路径验证.md)(偷取机制 §1.3;F4 记录 §3.4)
- [SMP 双核全路径:band 跨环 lazy 执法与 park 核 util 折窗](2026-11-16-SMP双核全路径-band跨环lazy执法与park核util折窗.md)(同报告 A 面两修)
- [balface SMP 门与偷取过滤判定律](2026-11-14-balface-smp-gates.md)(steal_filter 资格门单源——本刀在其上叠加带偏好序)
