# P2-2 退出异步化：末线程 ht.drain 移交 HT_DRAINQ，REAP 排水点异步收口

Status: implemented

## Problem

[task_exit_hook](../../../kernel/src/syscall.rs)（末线程 Process 收口）在 IRQ 屏蔽 + KL 在身的上下文切换窗内同步 `ht.drain()`：O(句柄表) 的逐槽清理（release → 归零 drop Box → VMO Drop 还帧 / Vmar Drop 注销 demand / ChannelEnd Drop 放互钉）阻塞被选者上 CPU。句柄表大时（per-Process 型帽：VMO 256 + channel 端点 64 + …；页级 VMO 的 Drop 链还帧可达千帧）是调度延迟尖峰，rootfs 相 spawn/退出 churn 风暴放大——路线图 [P2-2](../../../docs/系统优化路线图-v1.0.md)（R2 内存-调度集成 P1 级）。方案权衡见下；「方案 C 同步够快」被否：drain 成本不止 O(槽位)——每槽的 Drop 链深度由对象几何决定（128MiB VMO 记账 ≈576KiB、还帧按 committed 页数），上界远超「表通常小」的直觉。

## Decision

**退出钩子只做 O(1) 簿记，drain 异步化**——形态 = REAPQ 的 Process-表姊妹面：

- 钩子（末线程）：`mark_exited`（首因保码）→ `wake_matching(pinned, TERMINATED)`（同步保留——join/TERMINATED 等待者不因异步化晚醒）→ Process 运行钉住引用**所有权移交** `HT_DRAINQ`（kernel/src/[process.rs](../../../kernel/src/process.rs)，256 槽，`defer_ht_drain`）。非末线程路径不变（只 release 钉住）。
- 异步半场：sched [reap_drain](../../../sched/src/sched.rs) 尾部新钩子 `REAP_POST_HOOK`（`set_reap_post_hook` 注册于 `syscall::install`）→ kernel `ht_drain_flush` 在 CPU0 idle/yield 排水点逐项 `drain → release`——与 Task Box 回收同一批排水点，永不在退出者自身上下文。每拍 ≤32 条（摊平 KL 持有窗），LIFO 弹出（条目间无序依赖）。
- 安全不变量：
  - **drain 先于 Process Drop（防 ht UAF）**：移交的钉住引用由队列独占持有，flush 侧 drain→release 序 = 旧钩子尾两步原样、只是迟到；Process 在 drain 完成前结构性存活。
  - **互斥（SMP 参与态）无需新锁**：push 点（退出钩子）恒 KL 在身（SVC 退出链 / EL0 fault kill 链 `SVC_CLASS_ALL` / kill_all 的 SVC 主体——三路临界区同构）；flush 点经 `kl_held` 门取 KL（`Vmar::Drop` 同款门先例）。队列访问与 Drop 链全在 KL 窗内，跨核 push/flush 由既有 KL 串行；flush 取 KL 时不持任何结构锁，无锁序倒置面。
  - **唤醒序不漂移**：drain 的 Drop 链本就不唤醒任何人（三 Drop 实现零 wake 面），drain 迟到不改变任何等待者的可见序；join 面（thread_key TERMINATED）与 Process TERMINATED 均仍在 mark_exited 后同步。
  - **demand 注销**：drain 在 KL 窗内执行，`Vmar::Drop` 的注销/放钉语境与同步时代一致（S-4 D23 分配器纪律维持）；不引入表帧回收（v1 政策不动——[P2-4 裁定](2026-11-13-p24-smp-dual-window-not-applicable.md) Consequences ② 的前提「若引入表帧回收」不触发）。
  - **观测滞后有界**：`PROCESS_LIVE` 减点与帧水位随 flush 滞后到下一排水点；收割链必经的 `wait TERMINATED` 阻塞必经 idle——churn 循环每轮至少排水一次。`PROCESS_MAX` 帽最坏瞬态多计已退出未排水进程（帽余量 ≥2.6×）；soak pages 断言以 5 周期潜伏首拍为基线，µs-ms 级滞后不触面。
  - **溢出宁慢勿错**：cap 256（REAPQ 同容量哲学）满则**同步回退**旧路径原样（钩子内 drain + release）+ loud ≤4 次——绝不泄漏钉住引用。
- 未 start 的 `process_terminate` 侧收口保持同步：ht 出生为空（bootstrap 迁移在 start 才发生），drain 恒 O(1)，且该路径是 ktest F4d/F5 的闭环面（Exited → close → drop Process → `PROCESS_LIVE` 归位同步可见）。

## Alternatives considered

先搜（含 rejected/）：「退出异步/task_exit_hook/ht.drain/REAPQ」仅 [P2-4 裁定](2026-11-13-p24-smp-dual-window-not-applicable.md) Consequences ② 交叉引用本项，无 supersession。

- **方案 B（新建常驻 reaper 内核任务，boot spawn 低优先级循环等队列）——否决**：内核任务无「等队列通知」原语（wait 面只有 SLEEPQ 定时/键唤醒），只能周期睡醒轮询——常驻唤醒面 + 新任务的调度拓扑面；且 drain 需 KL + IRQ 屏蔽语境，内核任务运行态并非天然在窗。REAPQ 的「处理器」本就不是任务而是 idle/yield 排水点（Task Box 回收同款）——复用同一批点 = 零新任务、零新唤醒源、退出后排水延迟同 Task Box 量级。
- **方案 C（同步 drain 已够快，not-applicable）——否决**：见 Problem——drain 成本由对象几何主导非槽位数，页级 VMO Drop 链还帧可达千帧，churn 风暴下正是 P2-2 立项的尖峰面。
- **独立第二把队列锁（REAPQ_LOCK 姊妹）——否决**：push/flush 两点天然全在 KL 窗内（退出链三路临界区同构），KL 即互斥面——加锁纯冗余且引入 lockdep 第九类的序论证负担。
- **flush 全量一次排空——改为 32 条/拍**：SMP 下对核等 KL 的自旋面随批量线性涨；idle 环逐拍推进在 churn 峰值（33-60）≤2 拍清空，无饥饿面。

## Consequences

- 退出切换窗只剩 O(1) 簿记（mark_exited + wake + push）；churn 密集负载的调度延迟尖峰削除；REAPQ 排水点新增 ht-drain 半场（同点双队列）。方案权衡与验收依据见路线图 [P2-2](../../../docs/系统优化路线图-v1.0.md) §3。
- 语义漂移面 = 「Process Drop/表释放的观测时刻」从退出点滞后到下一 CPU0 排水点：`PROCESS_LIVE`、帧水位、`ht` 槽位回收（进程表 slot 回收在 drain 后的约束由此维持）都随 flush 迟到。依赖「退出即释放」的观测者（若有）须先经一次 idle/yield——收割链（wait 阻塞）天然满足。
- 承重事实，动其一须复审本设计：① 退出钩子三调用路恒 KL 在身（trap.rs fault kill 链 `svc_enter` 若降档，`defer_ht_drain` 的互斥论证失效——需补显式锁或断言）；② `Vmar::Drop`/`Vmo::Drop` 零 wake 面（新增「Drop 唤醒等待者」语义则迟到 drain 改变等待者可见序）；③ REAP 排水点仍在 CPU0（`yield_now` 尾 CPU0 门 + idle_loop CPU0 专属）。
- Related: [P2-1 per-key 唤醒链](../../../docs/阶段沿革.md)（同轮规模化护航件——沿革 P2-1 条；笔记本体随 2026-11-13 base 重整出库）；[P2-4 SMP 双窗裁定](2026-11-13-p24-smp-dual-window-not-applicable.md)（其 Consequences ② 由本笔记「不引入表帧回收」收口）。
