# Agent Note: 忙核 arm-to-nearest-deadline——oneshot 精确唤醒三件套

Status: implemented

## Problem

idle/park 空闲路径早已按 SLEEPQ 最近 deadline 用 `arm_at` oneshot 精确武装（`sched.rs` idle_loop 两分支 / `kernel/src/main.rs` secondary_main park 环）；但忙核（有任务在跑）上 CVAL 只在 tick 入口 rearm 成满片终点（`arch/aarch64/src/timer.rs` `timer::handle` → `arm_slice`），`expire_timeouts` 唯一驱动 = `on_irq_tick`——睡眠者的唤醒粒度恒 1 slice（名义 100ms、TCG 实效约 200ms）。R-1 cyclictest 三档 oversleep 首验收 p50 3.5M-5.4M cyc 全被该粒度支配（[docs/基准与压力测试设计-v1.0](../../../docs/基准与压力测试设计-v1.0.md) §10；[research/调度深度调研/02-实时性](../../../research/调度深度调研/02-实时性.md) §1.3 尾「留后」注 + §⑥ 对位建议其三）。

开刀后发现第二层缺口：即便唤醒时刻精确，超时唤醒出口没有检查点——G3-1c 检查点族（「严格更高 eff 直切」）覆盖了 SVC 尾、UART RX、SGI 一切唤醒出口，唯超时唤醒（expire 摘队）出口缺位：唤醒者只入 runqueue，要等低优占用者 slice 燃尽支才被 pick。严格更高 eff 的超时醒者（cyclictest 形，prio 8 醒在 prio 0 忙环上）的重续延迟仍被片终点支配——oneshot 精确唤醒「只醒了核、没醒任务」，R-1 量的恰是重续延迟。

## Decision

三件套，收紧判同律单源 `sched/src/oneshot.rs::tighten_deadline`（nearest 严格早于现臂才收紧，永不推迟——与 `arm_slice` 的 S-6 单调夹取同向；过期值合法，`arm_at` compare 立即成立即醒、回环 expire 承接，park 先例同款；片终点恒为硬底，S-6「deadline 跑步机饿死 tick」判例的镜像防线）：

- 件一（`on_irq_tick`，expire 之后）：SLEEPQ 队头早于现臂即 `arm_at` 收紧——tick 处理器 oneshot 化本体；无到期/不更早保持现臂 = 周期 tick 兜底。
- 件二（`on_irq_tick`，件一之后）：超时唤醒出口检查点——`expire_timeouts` 改返「是否摘出过人」，摘出且 `preempt_count()==0`（G43 防 PRINT_LOCK 窗，同 slice 燃尽支门）时 `try_preempt`；空转 tick 零成本跳过，不改变「非超时唤醒等片尽」的既有语义。
- 件三（`block_current` 忙核授片臂，出选有货支）：授片处取 min（timer.rs 模块头「Phase 4+ 留后」项兑现）——新睡眠者的 deadline 刚 insert_sorted 进 SLEEPQ，这是唯一看得见该截止的授片点；SQ 窗内直读队头（免 `nearest_deadline` 重锁的同核自死锁）。

idle/park 三臂零改（idle_loop 单核与 SMP 分支、secondary_main park 环原样——忙核只是同走 `arm_at` 面）；timer.rs 仅模块头文档收口，rearm-before-EOIR 铁序与 `arm_slice` 夹取零改。RR.tick 按实耗 CNTVCT 增量记账（class.rs tick），收紧引出的早 tick 不多扣片——公平记账不受影响。

律与面板 host 钉板：`sched/host/oneshot_law_test.rs` 以 `#[path]` mod 同源编译（sched crate 本体 no_std + HAL 依赖不编 host；镜像 W3-Q2 face-crate 单源纪律的窗内变体——本刀律小不立新 crate），7 测全绿；justfile 配方 `test-oneshot` 收口跑法。

## Alternatives considered

- 仅件一（任务书字面最小形）：片内中途入睡的短睡者对 tick 不可见（截止落在入睡时刻与片终点之间，tick 臂只能看见「上 tick 时已在队」的截止）——唤醒仍落片界，δ 不塌缩，故补件三。
- 仅件一+件三不补件二：唤醒时刻精确但重续等片尽——demo 忙环（prio 0）常驻的 bench-rt boot 下实测数不会动，故补件二。
- 在 `timer::handle`（IRQ 入口 rearm 处）取 min：arch→sched 反向依赖；且入口时本拍到期头尚未摘（arm 到过期值 = 白烧一轮重入 IRQ）；入口 rearm 铁序不动。
- 无片终点兜底的裸 `arm_at(D)`：deadline 远于片终点时本核最长 D−now 无 tick——S-6 跑步机饿死判例的直接重演（抢占/记账全停），故收紧只在「更早」方向、片终点恒在。
- 跨核 push 时 kick 对端忙核重臂（Linux smp timer 面形）：SMP 残留缺口——对核入睡只收紧本核 CVAL，对端忙核要到下个片界 tick 才看见该截止；单核 R-1 验收面不涉，留后。

## Consequences

- 忙核超时唤醒两段都精确：醒核（件一/件三，arm 到截止时刻）+ 醒任务（件二，严格更高 eff 片内即时直切）——唤醒粒度 slice 级 → oneshot 级；R-1 三档 δ 即验收面（实测记录见验证段）。
- 行为变化面：严格更高 eff 的超时醒者现在片内即时抢占低优占用者（preempt-on-wake 语义落到超时出口，与 SVC/RX/SGI 出口同款同判据）——高优定时任务的尾延迟不再含「等低优片尽」项；低优长任务被切得更碎，但 vruntime/账本照常记账，轮转公平不受损。
- 验证：host 7 测全绿（`just test-oneshot`）；`docs_guard` PASS；`writable_globals` ok；QEMU 面受同宿主六实例以上长跑占用（QEMU 严格单实例串行纪律），bench-rt 双基线（HEAD 基线内核 / HEAD+本刀隔离 worktree 内核）与 smoke 回归经守窗器跑——结果记录见本仓验证日志，若窗未开则以 host 面 + 机制推断收口（推断：δ 三档 p50 应从 3.5M-5.4M cyc 塌缩至 10^3-10^4 cyc 级 = IRQ 链 + expire + 直切 + 重续开销）。
- 残留：SMP 跨核 push 不重臂对端忙核（对端下个片界 tick 承接，兜底不丢唤醒只损精度）；与并行 ① 号刀（sched/src/policy.rs shadow verify-loop）同窗零文件重叠，隔离 worktree 构建保证 bench 归因纯净。

## Related

- [2026-11-15-刀A应用槽合流与计数锁数](2026-11-15-刀A应用槽合流与计数锁数.md)——本刀两产物（sched/src/oneshot.rs + sched/host/oneshot_law_test.rs）原提交漏 add，刀 A 自终结者支线 2e775c12b blob 恢复（律测 7/7 复绿）
- [2026-11-15-svcmgr-demo-face-idle-闭合](2026-11-15-svcmgr-demo-face-idle-闭合.md)——idle 侧 arm_at 停泊补（本刀补忙核侧，同一精确唤醒面的两半）
- [docs/调度器设计-v1.2](../../../docs/调度器设计-v1.2.md) §4.1 绝对截止时间重编程纪律
- [research/调度深度调研/02-实时性](../../../research/调度深度调研/02-实时性.md) §1.3/§⑥——缺口定位与本刀挂账
- [docs/基准与压力测试设计-v1.0](../../../docs/基准与压力测试设计-v1.0.md) §10——R-1 cyclictest 验收面
