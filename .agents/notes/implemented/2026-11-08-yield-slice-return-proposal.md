# Agent Note: yield 缺片归还语义——sys_thread_yield 最 eligible 场景可能空操作

Status: implemented

## Problem

调度域 yield 自查（吸收路线图 15 号增量项②，对照 moss `yield_slice`）发现：BerryOS 的 `yield_now`（`sched/src/sched.rs:1135`）让出侧无"片归还"语义——prev 仅 `state = Ready` 化，vruntime 停在让出时刻（未涨满片）、vdeadline 保持旧值（授片重编只对 next）。环上扫描按 `(eligible, vdeadline, vruntime)` 取最小键：当让出者是环上最 eligible（常见于协作自旋任务——它刚让出、vruntime 相对最低）时，`sys_thread_yield`（0x3F）**立即再次选中让出者，yield 语义上成为空操作**——POSIX `sched_yield` 的"让给同优先级其他线程"预期落空。

## Proposal

借 moss `yield_slice` 语义（`v_eligible = max(v_eligible, v_deadline)`——显式归还未用片）翻译为 BerryOS 结构的最小刀：

- `yield_now_impl` 中 prev Ready 化时，若 `prev.vruntime < avg_vruntime(rq)` 则**抬到 avg**（复用 `enqueue` 抬均逻辑同款，class.rs:154-157 的防睡眠 lag 攒积面）——让出者未攒虚拟优势时零操作、攒了则抹平。
- 配 ktest：双任务同带，A 让出后断言下一选不是 A（除非环上仅 A）——钉"yield 真让出"回归面。
- 冻结任务 `lazy unthrottle` 与 deadline 任务的绝对 vdeadline 路径不受影响（抬均只动 vruntime）。

## Alternatives considered

- **moss 原式（动 vdeadline）**：BerryOS 的 vdeadline 承载 deadline 任务绝对截止语义（S5 手动设置不覆盖），动 vdeadline 会侵入 S5 面——拒；抬 vruntime 语义等价且不动 S5。
- **不改（接受弱 yield）**：被拒——自旋式协作任务（锁退避、池等待）在多任务下会让不出 CPU，tick 防饥饿兜底是最后防线而非语义；且 0x3F 已进 v1 ABI 面，弱实现会固化。

## Consequences

- 实施窗：**等 sched-1.3 SMP 线收官**（SMP-3 刚落写者锁，sched 面活跃）——本提案不抢刀。
- 若落地：sys_thread_yield 从"可能空操作"变为"真让出"；自旋场景公平性改善；moss 互证（其 `yield_slice` 在生产内核验证过同语义）。
- 不改 tick/抢占路径——零热路径开销（yield 是冷路径）。
- 调和注记（2026-11-08 落地时）：与并发 S2b+ Σvruntime 增量账本刀同函数交叠——调和 = `yield_slice_return` 抬均经 `rq.vruntime_add()` 账点 + ktest 直改后 `resync_vruntime_sum()` 洗账；合并树 QEMU 全绿（YIELD-K ok / ktest done / 52 标记 / ring_check 安静）。语义边界诚实记录：双任务环数学上抬均零操作区（handoff ⟺ 让出者 vruntime > avg，与抬均互斥）——修复有效面 = 3+ 任务环真 handoff；双任务 yield 一刀真让出需后续提案（抬到 max(ring) 形态）。
- 落地（2026-11-08）：`Fair::yield_slice_return`（class.rs，enqueue 抬均同款、最小暴露面 pub 方法）接入 `yield_now_impl` prev Ready 化处（RQ 窗内）；回归面 = kernel ktest `test_policy_yield_return`（标记 `YIELD-K ok`）。
- 双任务真让出补刀（2026-11-08）：抬 ring max 形态落地，Y4 三任务覆盖——`yield_slice_return` 从「抬到 avg」强化为「抬到环上 vruntime **max**」（协作让出 = EEVDF 形态排队尾；POSIX `sched_yield` 语义对齐——让出者本轮垫底），抬均的双任务环零操作区（avg < max ⟹ 让出者抬后仍最 eligible）由 max 形态封死。三任务环公平性：让出者抬到 max 后仍在环（tick 继续累加它），防饥饿兜底已有；锁退避场景方向正确（退避者让路）。实现 = 扫环取 max（`avg_vruntime` 旧全环扫描同款走查，RQ 锁/屏蔽窗内安全，O(n) 可接受——yield 冷路径）、只动 vruntime 不碰 vdeadline（S5 语义不动）、抬值经 `rq.vruntime_add()` 账点（前刀调和纪律延续）。ktest：Y2 断言改（0/1000 双任务 → 抬到 1000 非 500，并加 vdeadline 不动钉）、Y4 新增（0/600/1000 三任务环 → 让出者垫底 1000、中间者不动）、Y1（已是 max → 零操作）/Y3（单元素）语义保持；直改后 `resync_vruntime_sum()` 洗账纪律照旧。
- 边界补（2026-11-15，[svcmgr idle 后半刀](2026-11-15-svcmgr-demo-face-idle-闭合.md)）：「让出者在环」契约对 **IDLE 不成立**（永不在环）——单核 idle_loop「有活」支从 IDLE 让出时无条件抬值 = 账本灌水（ledger 独加、walk 不含环外者），idle 停泊补使其每醒一灌被 ring_check 捕获；修 = `yield_now_impl` 在环（`rq_prev` Some）才抬。任务侧 yield 语义不动。

## Related

- [吸收路线图](../../plans/2026-11-06-absorb-2026os-roadmap.md)（15 号增量项②）· moss `src/sched/sched_task/mod.rs` yield_slice · `sched/src/class.rs` enqueue 抬均（class.rs:154）· `sched/src/sched.rs:1135` yield_now_impl
