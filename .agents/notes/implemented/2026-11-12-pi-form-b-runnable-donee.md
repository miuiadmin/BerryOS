# Agent Note: PI 形 B——runnable 受捐盲区收口（R3 最大残留风险）

Status: implemented

## Problem

`kernel/src/pi.rs::recompute_locked` 的全图重推只扫 SLEEPQ 找受捐方：高优 donor 阻塞在 channel 上时，若低优持有者已在 RUNQUEUE（不在 SLEEPQ），boost 永远落不到持有者头上——教科书 Mars Pathfinder 无界优先级反转。R3 实时分析（`docs/阶段沿革.md` R3 章）定谳：形 A（持有者在睡）PI 覆盖 ≤800ms 有界，**形 B（持有者 runnable）无界**，是 v1 最大残留风险。

## Decision

- **受捐全域扩为「SLEEPQ 在队者 ∪ 本核 RQ 在环者（持有面可识别）」**：`recompute_locked` 三趟（清零 / 不动点传播 / 尾刷 effective）全部并进 `for_each_runnable`（本核 RQ 环形走查，`lazy_unthrottle_deadlines` 同款模式）。判定与 SLEEPQ 受捐同形——同一 `holder_has(peer, need)`（持有 donor 所等 channel 的 peer 端点 + 解锁 right）。
- **共享表护栏（形 B 边界，smoke 实证后补）**：RQ 面受捐要求 `proc_tag != 0`——共享表（内核侧 bootstrap 表）「视作持有表内一切」的 v1 粗粒度对**睡眠者**无害（J7 面，4-3b 起语义不动），对**环上者**是 4-5a 式 boost 喷洒：无护栏首发 smoke 即炸——fa 对端点在共享表（main.rs `mk_fast_chan`），pong(32) 每轮入睡捐满全环（矩阵任务全升 32 → 优先级塌平 → G1.5 超时），且 ping（endA:SEND 持有者）升 32 塌掉 `try_preempt` 的 32>16 直切契约 → ping poll 空队列 -EAGAIN。受捐方须真实可识别；生产 client/server 拓扑（EL0 进程表 / spawn_with_tag 私有表）全覆盖，共享表 runnable 对维持 4-3b 盲区形态（v1 粒度债随「常驻服务勿用共享表」既有指引共存）。
- **RQ 窗先行、KL 子窗居内**：`pi_rq_lock`（位感知取本核 RQ 锁）在 KL 检查**之前**开窗、`pi_rq_unlock` 在 KL 释放**之后**闭窗。锁序依据 = 终态七层冻结序 **SQ > 队列 > REAPQ > 表（KL 最内）** 的升序前缀（`sched.rs::klock_release_switch` 注 / `kern_lock.rs` SVC 类注「KL 取用点一律结构先取 KL 后取」）——SQ 窗内取 RQ 合法、持 RQ 取 KL 合法、持 KL 取 RQ 违序（本形不制造后者的唯一结构保证 = 开窗序）。
- **热路径零新增锁活动**：钩子五发射点的 SVC 类入口（wait/send 族 0x2E / thread_sleep 0x0E / kill 全谱）恒持完整 SQ+RQ0+RQ1 结构位，`held_test` 位感知退化免物理取放；真取只发生在 IRQ/tick 与 ktest/boot 窗（RQ 窗恒 IRQ 屏蔽 ⇒ 被打断者必不在 RQ 窗内，无同核重入；「tick 打断持 KL 主体」形态由 KL 子窗既有 `kl_held` 同核退化兜住）。
- **冻结语义收敛为「门闭窗冻结」**：受捐方唤醒离队入环后由后续重推继续重算（资源已就绪的边自然消失、仍被等资源的边保真重推导）；环上 boost 只在 P2 门闭窗内冻结随行，门开或任务自身入睡（`pi_count_add` 按 pi≠0 入账 ②）时清零回落。**P2 门控零改动**：①（派生位等待者）非零仍是环上受捐的必要条件（donor 必在队）；②仍只数睡眠者（计数语义 = 睡眠者清零面），环上 boost 的清零由门开罩住——双零即返的早退照常工作。
- **尾趟计数乙不数环上者**：`pi_boost_recount` 口径不动（睡眠者清零面）；环上 stale boost 的生命周期 = 门闭冻结 → 入睡入账 → 门开重推清零，闭环无泄漏。
- **SMP 残留显式声明**：`for_each_runnable` 只罩本核环（`wake_one_inner` 的 `enqueue_with_kick` 在 SMP 下可能入对核环）——对核环持有者收口需远核 RQ 锁（SQ→双 RQ 升序序合法，v2 议）；单核（现役配置）全覆盖。
- **集合不相交不变式**：block 先摘环后发钩、wake 先入环后发钩（`wake_one_inner` 摘队→入环→调用方发钩）——钩子时刻 SLEEPQ 与 RQ 任务集不相交，无任务被双访双算。

## Alternatives considered

- **KL 子窗内取 RQ**（先 KL 后 RQ）：违锁序——「持 KL 旋结构」是被 S-4 判例明令消灭的边（`kern_lock.rs::klock_enter` 注），弃。
- **只清不捐**（RQ 扫描仅清 stale boost，不注入新 boost）：修不掉反转——Mars Pathfinder 场景要的正是 boost 落到 runnable 持有者头上被 pick 看见，半修无意义，弃。
- **共享表 runnable 也捐（无护栏首形）**：smoke 实证炸——fa 对/全矩阵被喷升（见 Decision 护栏条），4-5a 级联借 RQ 门还魂，弃。
- **修消费侧替代护栏**（fa 端点迁私有表 + ping poll 改阻塞 + 放宽 direct≥N-2 断言）：三处改动只救一个 bench，且矩阵喷洒（sleeper/waker 等共享表任务）仍在——治标不治本，弃。
- **增量簿记**（受捐边记账、变更时局部更新）：与全图重推零簿记的既有取舍相悖（v1 睡眠者个位数，O(睡眠者×(睡眠者+环上者)×表槽) 可忽略），规模压力显性再换，弃。
- **扫双核 RQ**（SMP 全覆盖）：需远核 RQ 锁 + steal 交互论证，现役单核配置零收益纯风险，v2 议。

## Consequences

- 形 B 收口：高优 client 阻塞、低优 server 在环等 CPU 时，server 的 `pi_inherited`/`prio_effective` 立即抬升（尾趟刷 effective——pick/try_preempt 消费面），无界反转降为形 A 同界（≤8 跳传播 + ΣCS）。
- 行为面变化：环上任务的 boost 从「冻结到下次入睡」变为「每次门开重推都重算」——CHINA 链类时序只会更准（低优持有者更快被 pick），无标记契约依赖旧冻结语义（ktest J6 手搭者不在环上，断言原样过）。
- `sched` 新导出：`pi_rq_lock`/`pi_rq_unlock`/`for_each_runnable`（生产面）+ `rq_insert_kt`/`rq_unlink_kt`（ktest 手搭环面，账本经 `link_back`/`unlink` 结构原语层入账——ring_check 金丝雀不漂移）。
- ktest PI J 段新增 J8（私有表环上持有者受捐 36 + donor 退场门开清零回落）与 J8b（共享表 runnable 护栏负向 + 同边睡眠者正向对照——边界钉住），`PI-K ok` 断言面扩两环。
- 复杂度增项：门开重推多一圈 O(环上者) 扫描（传播趟每 donor 一圈 + 清零/尾趟各一圈）；单核环 ≤~20 任务，icount 量级不变（FAST bench 冒烟对拍）。
- 清零趟对环上者**全域**（含共享表者——stale boost 于下次门开时回落）：冻结语义由「重推盲区补偿」收敛为「门闭窗冻结」——冻结的本意（boost 带进 runqueue 供下一轮出选）由门闭窗承接（donor 全退场 ⇒ 门闭 ⇒ 冻结随行）。

Related: [R3 实时延迟界](../../../docs/阶段沿革.md)（形 B 无界定谳）· [PI 全图重推原始落地](../../../docs/IPC设计-v1.0.md)（4-3b）· [SLEEPQ per-key 唤醒链](../../../docs/阶段沿革.md)（同 P2 门控家族——沿革 P2-1 条；笔记本体随 2026-11-13 base 重整出库）
