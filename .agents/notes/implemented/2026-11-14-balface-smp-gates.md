# Agent Note: on_irq_tick SMP 门与偷取过滤判定律单源 berry-balface（W3-Q82）

Status: implemented

## Problem

`sched/src/sched.rs` 的 `on_irq_tick` slice 燃尽支内嵌三个 SMP 门 + `try_steal_one` 的 victim 双过滤，判定律只活在 aarch64-only crate——sched 依赖 HAL global_asm，host 测不可达，ktest 只覆盖 boot 相。这些门的回归语义（折叠除法边界、饱和年龄判、失衡 K=2 与「近空闲」联合门、偷取过滤三值合取）没有一张 host 可测的钉板：错一门 = 要么诊断行洪泛（折叠除法错）、要么漏报永久丢醒（高龄判错）、要么 SMP 反复迁移复发（拉取门错——run6 实录 20 万+ 切换）。

## Decision

- **判定律外迁即唯一事实源**：五个门 + 五常量（HEARTBEAT_DIV=500/STUCK_DIV=2500/STUCK_AGE_SECS=10/STUCK_MAX=8/PULL_K=2）迁 `berry-balface`，sched.rs 五点消费同一函数（零第二实现——wqmatch/sliceface 同款纪律，非镜像）。
- **swap 语义投影为纯函数**：`heartbeat_due(tot, prev)` 的 prev 是原子 swap 换出值——门只判「tot 变档否」，存储臂留 sched。
- **高龄判乘域不混用**：`stuck_aged(age, freq) = freq != 0 && age > freq*10` 与内核判据逐字同形（乘域）；报告字段 `age / freq` 只做展示换算，两数学独立不得混用——亚秒粒度下 div 形会漏报 10s+1tick 形态。`saturating_mul` = 防御形（真实 freq 域 ~24MHz 永不饱和）；freq==0 恒 false 不 panic。
- **拉取门域 = usize**：Runqueue::len 原生域，消费点零转换；`mine+PULL_K` 在 theirs 恒 < usize::MAX 的实域无溢出（饱和窗 host 钉板）。
- **物理臂留 sched**：per-CPU 原子槽（HB_LAST/STUCK_LAST/PULL_CNT）、TTAS 锁窗、环扫描/unlink/home 随迁、打印发射——本面只钉「门怎么判」的数定律。HAL 度（diag_readout/rq len）以参数注入（W3-Q24 dpgface 纯投影同款）。

## Related

- [timer slack 纯面 berry-slackface](2026-11-13-timer-slack-face.md)（同系 HAL-face 纪律先例：判定律 host 钉板 + 物理臂留壳）
- [调度器 API 参考 v1.16](../../../docs/调度器API参考-v1.19.md)（on_irq_tick 契约面）
