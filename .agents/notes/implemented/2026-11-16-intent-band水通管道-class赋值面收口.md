# intent→band 水通管道: class 赋值面收口（class_of_task 六点分派）

Status: implemented

## Problem

06 号评估终判 §5「管道建成早于水」管道①（intent→band）：三段链路——manifest 声明 intent → svcmgr 透传 → kernel spawn（intent 参数）——前两段已建（svcmgr 透传见[透传首件](2026-11-15-intent-m4-svcmgr透传首件.md)），断点在**最后一环**：kernel spawn 后任务怎么获得调度 class？侦察结论：现役所有任务 → `sched.rs` 全局 `static RR: Fair`（unit Fair）单带——spawn 面有 intent（`spawn_intent` intent 双落 `intent_declared`/`intent_effective`；EL0 面经 thread attr `apply_sched_attr_to_task` spawn 后落档）但从未连到 [`class_fair_intent`](2026-11-16-m4首批band激活-现役集收窄与参数差异化断言.md) 工厂——「spawn 时 band 赋值」在 M4 骨架刀的预留窗注里明记「本刀不实装」。另侦察澄清任务书前提：`spawn_user`/`spawn_user_env` 七参中**无 intent 载荷**（arg = 句柄表/env 旗标；EL0 进程的 intent 走 attr 面不走 spawn_user 参数）——带赋值的接线点因此不在参数面而在 class 分派面。

## Decision

四件（2026-11-16，水通管道纪律——不求新机制，求链路闭合）：

1. **class 赋值面单源 `class_of_task`**（sched/src/class.rs）：`TaskClass` 二元分派枚举（`Unit` = 现役单带无载荷——unit `Fair` 是无状态单例，分派臂现取；`Band(FairBand)` = 带实例）+ `class_of_task(t: &Task) -> TaskClass`——按 `intent_effective`（§11 带键 = intent，零 Task 新字段）查 `class_fair_intent` 工厂：现役集 0x02/0x03 → `Band`（`INTENT_BAND_TABLE` 表行参数），其余/未声明 → `Unit`（缺省行为零漂移）。**动态派生、零缓存**（`Copy` 零分配，IRQ 热路径每次原语即时派生）——intent 结算借入改档或 EL0 attr spawn 后落档，下一次调度原语自然改带，「spawn 时 band 赋值」与「运行期 band 跟随」同一实现。配套：`FairBand` 补 `yield_slice_return` 委托 inherent 方法与 `PartialEq/Eq` derive。
2. **sched.rs 六点消费**（RR.* → class_of_task(t).*）：`enqueue_with_kick`（spawn+唤醒共用 funnel——`wake_one_inner` 亦经此）/偷取重入环（F-E2 回插本核环）/`on_irq_tick` 的 tick 记账/`switch_away_forever_with` 与 `kill_all_tasks_by_pinned` 的 dequeue×2/yield 片归还——全部改走 per-task 分派。`pick_next_filtered` 三调用点留 `RR`（队列级扫描——混合带同环出选，无 per-task class 语义，注释钉明）。FairBand 全方法委托 unit `Fair`——六点行为逐位一致，零漂移（M4 入场券纪律）。
3. **`spawn_intent` 串口可见行**（sched.rs）：现役带 spawn 打 `[band] <名> intent=<档> q_ns=<quantum> w=<lo>..<hi>`——intent→band→带参数经 boot 串口直接可见（QEMU 链路闭合证据面；boot 可信语境 DAIF 屏蔽直写 uart，同 boot 打印纪律）。
4. **ktest BANDM4 增 M4F 段**（kernel/src/ktest.rs，spawn 面端到端）：真 `spawn_intent`×3（interactive/utility/background 对照）→ `for_each_runnable` 走查（`pi_rq_lock` 窗）证三任务**真入 PER_CPU_RQ** 且逐名 `class_of_task` 断言——interactive/utility = `Band`（与工厂产物逐字段同）、background（首批外）= `Unit` 回落；spawn 面两带 quantum/权重包络分化断言；毕后 `rq_unlink_kt` 清场（F8 try_spawn_user 先例——断言后即摘不留 RQ 残留；任务体 leak 同 ktest 政策，`m4_dummy` 永不执行）。

## Alternatives considered

- Task 加 band 字段缓存 spawn 时赋值（否：§11「band 不加 attr 字段——intent 即带键」立法；且 intent 结算借入改档后缓存需失效逻辑，动态派生天然免）。
- `spawn_user`/`spawn_user_env` 加 intent 参数（否：侦察证实两面无 intent 载荷也无需有——EL0 进程 intent 走 thread attr 面 spawn 后落档，动态派生使其零改动自动获得 band 跟随；加参 = 动冻结合约面且无消费者）。
- 只改 `enqueue_with_kick` 一点（否：唤醒/偷取/tick/退出仍 unit Fair = 分派面残缺；六点全委托 Fair 行为逐位一致，扩展零风险）。
- trait 行为真差异化（per-band quantum 在 tick 执法/带序 priority 映射，否：M4 并线面，入场券〔v1 遥测实证 + loadgen + 尾延迟断言 + 空闲核金丝雀〕未过——同[前刀](2026-11-16-m4首批band激活-现役集收窄与参数差异化断言.md)立法，本刀差异化仍只落带参数轴）。

## Consequences

- **QEMU 实跑双证**（隔离 worktree = HEAD + 本刀 + 并行②会话 eas 件——主树 cargo build 被②在途 `sched/Cargo.toml` berry-em path 依赖卡嵌套 workspace 根冲突，归②/owner 收口）：ktest 全绿（`BANDM4 ok n=2` 含 M4F + `ktest done`）；boot 串口 5 行 `[band]` 证据——ktest 2 行（`m4spIA intent=2 q_ns=10000000 w=256..1024` / `m4spUT intent=3 q_ns=20000000 w=64..512`）+ boot 舰队 3 行（`it_driver intent=2 …` / `it_sinkA、it_sinkB intent=3 …`）——interactive（10ms/256..1024）vs utility（20ms/64..512）band 差异串口可见 = 管道①链路闭合。
- **零回归**：boot 标记对拍与 HEAD 基线逐字符同（仅 PERF/HDR 时序噪声行差异——TCG 常态）；既有 BANDM4 M4A–E 段零改动通过。
- 运行面行为 = 逐位零漂移（全委托 Fair）；「带实例按任务分派」从注释预留窗升为机器执法面，热路径 per-band quantum/权重执法仍归 M4 并线面。
- 遗留（非本刀域）：unsafe 台账漂移为 HEAD 预存（bootparams/ktest 早于本会话）；EL0 attr 面的 intent→band 跟随为动态派生自动生效，无需单独接线。

## Related

- [M4 首批 band 激活：现役集收窄与参数差异化断言](2026-11-16-m4首批band激活-现役集收窄与参数差异化断言.md)（工厂与表——本刀其外侧消费面）
- [intent M4 band 骨架纯面](2026-11-15-intent-m4-band-骨架纯面.md)
- [svcmgr intent 透传首件](2026-11-15-intent-m4-svcmgr透传首件.md)（管道①上游段）
