# Agent Note: timer slack 机制面首刀——bootparams 第三键 + LATE-only 顺延（§3.8 day-one 承诺清偿）

Status: implemented

## Problem

调度器设计 v1.2 §3.8 :156 承诺「timer 原语带 slack+policy 三元组（v1 day-one）」，三大设计承诺（timer slack + policy 三元组 + IntentProfile）中唯一机制面零命中（01 号审计；判定律纯面 slackface 2026-11-13 已落但 kernel/sched `grep slack` 仍空）。任务书（调度深水区①）指名修法：bootparams 第三键 `timer_slack` 读入 → 定时器到期聚合面应用 slack → 批量唤醒。

## Decision

1. **bootparams 词表 v1 扩第三键 `timer_slack`**（`kernel/src/bootparams.rs`）：u32 纳秒 ∈ [0,999999999]（上界 = 内核 `parse_u32` 九位帽对读，全域 < slackface `SLACK_MAINTENANCE_NS` 10s 帽），缺省 0 = 无松弛 = 空 params 镜像逐位零漂移（件缺静默合同照旧）；注入 `berry_sched::set_timer_slack_ns`（sched 全局，`sched/src/intent.rs`——`INTENT_DEFAULT` 同款原子静态 + G5 节位）；eco 行扩键 ` timer_slack=NS`（纳秒原值，只打在位键）。投放面同批：`tools/berry_assemble.py` BOOT_PARAMS_SPEC + selftest 两腿（三键正例 load/序列化黄金字节 + 域外 1000000000 拒）。
2. **LATE-only 顺延判定律内核消费面**（`berry_sched::intent::timer_slack_defer`，纯函数）：intent 档 ≥ UTILITY（UTILITY/BACKGROUND/MAINTENANCE 三档）的到期时间 +slack；**交互零牺牲**（INTERACTIVE/LATENCY_CRITICAL 恒等——slackface L2 的推论）+ **未声明零漂移**（UNSPECIFIED 恒等——「未声明不是声明了低档」，保存量任务 BLOCK/INTENT/G1.5 时序逐位不变；与 wip/cp11-timer-slack 参考线的 `timer_slack_ns_effective(UNSPECIFIED)=0` 同向裁决）；保留域保守回 exact。数学面：零 slack/零频/u64::MAX 无限等哨兵恒等；ns→VCT tick 截尾换算（实际顺延恒 ≤ 声明值——L1 late-only）；顶域夹 MAX-1 不回绕不撞哨兵。**消费点 = `block_current` 入队**（deadline 进 SLEEPQ 前一次落定）：下游 idle park `arm_at` / 忙核 `tighten_deadline` / `expire_timeouts` 头扫全读 SLEEPQ 队头，零改动继承——expire 一 pass 摘尽窗内到期者即批量唤醒的机制面下限。
3. **ktest 差分钉板**（`kernel/src/ktest.rs`）：`test_timer_slack`——同 slack 值 50ms@62.5MHz 下交互=精确（1000→1000）/ 非交互=+3125000 tick（1000→3126000）差分主线 + 恒等三面/截尾/顶域/24MHz 换算界/单调/全局往返（save/restore——真镜像 bootparams 可能已设全局）；`test_bootparams` 扩第三键（域执法 0 与 999999999 过、非数字/负/越九位拒 loud + 三键同件正例 + scan 链三键断言）。
4. **随刀收口 `parse_u32` 纯数字执法**：原 `checked_sub(b'0')` 只拦 <'0' 下溢，字母串走私为巨值（"abc"→5451）——两键时代由各键值域检查隐式兜住，全 u32 域的 timer_slack 暴露此洞（ktest 首轮实跑红，差分实证），显式化为 `b'0'..=b'9'` 闭域执法。

## Alternatives considered

- **arm_at/oneshot 臂上逐点顺延**（任务书字面「timer 到期编程处」的另一读法）：否——四消费臂（idle park/secondary park/授片 tighten/tick tighten）各需队头任务 intent 反查，顺延值与 SLEEPQ 存值漂移；入队点一次落定则三臂零改动继承（最小侵入）。
- **直接移植 wip/cp11-timer-slack 参考线**（iprofile per-tier 表值 + tickface `slack_defer_vct`）：否——该线无 bootparams 键（slack 值非产品可配）、且 tickface 纯律 crate 在本树不存在（参考线的重构件）；本任务书指名系统级单值旋钮 + intent 门，per-tier 表值消费留 CP-11 剩余项（两线在 UNSPECIFIED-exact 裁决上同向，合流无语义冲突）。
- **kernel path 依赖 user/berry-slackface**（判定律真单源直连）：否——user/ 工作区与内核工作区分治（slackface 头注「接线归接线窗」自认）；本刀消费其 L1/L2 律语义（文档锚），不复制聚合分桶实现（机制面不做第二实现纪律）。
- **timer_slack 域上界取 10s**（slackface policy_triple 帽）：否——u32 + 九位解析帽自然域 [0,999999999]，词表域与内核解析面逐值对齐（1000000000 双侧拒），不立第二套数。

## Consequences

- 词表 v1 两键 → 三键：product.toml `[platform.boot.params]` 可声明 `timer_slack`；默认镜像零行零漂移（QEMU 默认 rootfs 镜像 bootparams 计数 0 实证）。
- 启用语义：产品声明 timer_slack=N 后，UTILITY 及以下档任务的有限 deadline 睡眠最迟可顺延 min(N 换算 tick, …)——`zx_timer_set`「只迟不早」同构；dpg 压测臂等 kernel 任务随 intent_default 档位同受影响（默认 unspecified=恒等，故默认零影响）。
- 验证实跑全绿：cargo check；berry_assemble --selftest（含新两腿）；slackface 12 测 + oneshot 面板 7 测（host）；`SMOKE_PHASES=boot` smoke 32/32 标记；QEMU 三靴——params 镜像 eco 行逐字 `bootparams slice_hz=64 slice=976562 intent=utility timer_slack=50000000` + ktest done、默认 rootfs 镜像零 bootparams 行 + ktest done、bare smoke 零漂移（ktest 静默执法 = 无 panic 即绿）。
- QEMU 排障一笔：minimal rootfs（无用户件）boot 中 dpg_press 自证 panic 与本刀无关——差分靴（同镜像去 timer_slack 键）同位同阈值触发，系空 rootfs 单核独占形态的标定漂移（dpg.rs:1004 自述「θ_p 误报判据前提破」）。
- 仍留窗（CP-11 剩余项，docs/调度彻底计划-v1.2.md CP-11 行）：IntentProfile per-tier 表值消费（iprofile `timer_slack_ns` 列——现为系统级单值）、表运行时更新与保守回退通道、`zx_timer_set` 形三元组 syscall ABI、slackface 聚合分桶的批量唤醒编排（现机制面下限 = expire 头扫一 pass）。

## Related

- 判定律纯面前件：[timer slack 聚合判定律纯面](./2026-11-13-timer-slack-face.md)（本刀 = 其「接线窗」的首刀兑现——L1/L2 律的内核消费面）；设计锚 [docs/调度器设计-v1.2.md](../../../docs/调度器设计-v1.2.md) §3.8（v1.2 追记 + 机制面注）；词表合同 [docs/产品配置层与槽位合同设计-v1.1.md](../../../docs/产品配置层与槽位合同设计-v1.1.md) §1.6-补三（轻追记五）；计划行 [docs/调度彻底计划-v1.2.md](../../../docs/调度彻底计划-v1.2.md) CP-11（裁十债一部分清偿）。
- 同大窗姊妹刀：[intent M4 band 骨架纯面](./2026-11-15-intent-m4-band-骨架纯面.md)（任务②的纯面前件）、[PM 四 crate 联合场景](./2026-11-15-pm-four-crate-joint-scenarios.md)（CP-08 面）。
