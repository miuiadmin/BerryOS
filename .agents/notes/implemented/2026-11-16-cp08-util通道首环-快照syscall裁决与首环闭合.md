# CP-08 util 通道首环: OQ-8 快照 syscall 裁决与内核到 berry-power 首环闭合

Status: implemented

## Problem

调度深水区收官任务③（CP-08 util 通道首环）。任务书判词「berry-power EWMA 消费位在但输入通道零」——04 号复核（`research/调度彻底计划/04-PM双服务骨架复核.md` §6-1）把「util 通道形态终裁：共享环 vs 快照 VMO」列为 CP-08 接线窗 13 件之首（裁决类），`docs/调度彻底计划-v1.2.md:217` OQ-8 在册「CP-08 前裁」。侦察结论三件：

1. **生产侧已全备、零新累积**：`sched/src/util.rs` per-CPU EWMA 记账面在役——`Fair::tick` 每拍调 `util_accumulate`/`util_tick_window`（`sched/src/class.rs:265-267`），`cpu_util(cpu)` 即「PM 读面」唯一读出口（`sched/src/util.rs:94-102` 模块注自认 v1.3 §11 PM 环形态留窗）。任务书预设的「vruntime 代理」与「pick/put 累积」两案均不需要——记账粒度（CNTVCT 拍）比 vruntime 更准且已折叠。
2. **消费侧双输入口在**：`user/bins/berry-power/src/util.rs` 的 `ingest_window`/`ingest_sample`（04 号 §6-1「只差通道」）。
3. **缺口 = 导出面**：内核侧无任何用户态读出口；VMO 共享先例为零（`telemetry_open` 0x84 环面 stage=v3 未启，`kernel/src/telemetry.rs:13` 消费路径留 v3）。

## Decision

**OQ-8 首环裁决 = 快照 syscall（pull 形态）**，五件落法（2026-11-16）：

1. **加号 0x40 `sched_util_snapshot`**（`tools/abi.toml` 新组「调度遥测」，nr_total 38→39 / implemented 37→38，add-only 加号先例 0x86/0x87/0x88 同款；`tools/gen_abi.py` NO_GATE_SYSCALLS 白名单入册——零 handle 遥测读面）。裁决依据：零 VMO 共享先例（建环/共享内存面远超首环体量）；**PM-15 §4.1「六面零新 syscall」辖 SDK 服务 op 面、不辖内核→服务数据通道**（util 生产侧 v1.3 立法内定 PM 读面）；共享环 push 形态随 CP-10 通道窗叠加（add-only 不互斥——快照是首环不是终态）。契约：`count < 核数 → -EINVAL`；`≥` 只写实数槽、余槽不动；**返回 >0 = 实写槽位数（恰 = `UTIL_NR_CPU`，调用方以此探核数——count-wins 口径同 0x24 批供表）**。
2. **内核 handler**（`kernel/src/syscall.rs` `sys_sched_util_snapshot`，dispatch 走定长槽零开销）：`cpu_util(c)` Relaxed 原子读 → LE u32 数组 → `copy_to_user`；锁类 SVC_CLASS_TABLE（§A113 读者面，同 0x61/0x87）；头注/inventory 计数两处随迁（37/36→39/38）。
3. **berry-sys wrapper**（`berry_sched_util_snapshot(util: *mut u32, count: usize) -> Status`，>0 = 实写数）。
4. **berry-power 消费三面**：`UtilTrack::ingest_snapshot`（全或无拒面——长度 ≠ `UTIL_NR_CPU` 或任一核 >1024 → None 零改动）+ `PowerCore::ingest_util_snapshot` 组装面 + bin 起机自检真实快照读（失败回落合成窗账——`@synth`；boot 探测回退先例同 0x24）；berry-sys 挂 wired feature 门（host 测试相零 EL0 依赖，simbattery 依赖先例）。
5. **ktest F 块**（`kernel/src/ktest.rs` test_syscall 尾，SYSCALL ok 前）：F1 拒面（count 0/1 → -EINVAL）/ F2 泵窗（elapsed 恰等窗长 → ewma 精确 128）/ F3 svc 读回 [128, 0] + count=8 余槽哨兵不动 + rc=2 探核数 / F4 `util_reset` 双核收口（test_syscall 是 ktest 首族，后续 UTIL/OU/INTEG 族「未启态零」前提不破）。

## Alternatives considered

- 共享环 / 快照 VMO（否：零现役先例——`telemetry_open` v3 未启、policy_bind 0x83 只落最小 COMMIT 面；首环体量用 pull 快照即闭合验收线，push 环留 CP-10 叠加不返工）。
- 复用 vruntime 做 util 代理（否：侦察实锤 per-CPU EWMA 记账已在役且粒度更准——任务书「侦察后零新累积」条款的更强兑现）。
- 快照走既有 `telemetry_open` 0x84（否：v3 号提前实现无先例——0x83 policy_bind 是「v3 提前」唯一特例且自带立法；为 2 个 u32 拉起整环映射面不成比例）。
- ktest 断言放弃（曾议：侦察期 ktest.rs 被并行任务②会话活跃占用，一度改道纯 guest 端到端验证；②收笔后落 F 块——拒面 -EINVAL 无 guest 覆盖是实缺口）。

## Consequences

- **首环闭合实证**（QEMU virt 单核，最小 rootfs 只装 bin/berry-power）：`berry-power: skeleton dry-run ... util=127@k cap=400000kHz@src1 ... freq req=124023khz`——`@k` = guest 内经 0x40 读到内核真值 EWMA 127（12.4% 引导负载；合成路恒 128，127 即真值非回落），`127×1e6/1024=124023` schedutil 同构算式逐位复算对；回落路在基线内核（无 0x40）同靴验证 `util=128@synth`。
- 验证全绿：abi-check PASS（五面 39 号单源）；berry-power host 32 测（31+1 快照 loopback/拒面）；内核 release 构建零 error；QEMU 终靴 panic=0、SYSCALL ok（F 块过）、ktest done、`eco: berry-power up`。
- **预存抖动面披露（非本刀回归）**：dpg_press 反向臂自检 panic（`kernel/src/dpg.rs:1004`，单核标定漂移检测面）在本树**基线内核（零本刀改动）1/4 复现**、带本刀 3/4——TCG 时序敏感 + 代码布局位移抬升触发率，因果源预存归 pager/DPG 线；panic 为自检子任务响亮诊断，全部 ktest 标记与引导不受影响（三靴对拍标记完整）。
- 残留：常驻循环周期轮询面（bin 现仅自检单读——随窗 2 recv 循环同刀）；CP-08 内核侧两断言（调度路径零频率写点 / PM 崩溃保频）归 kernel/sched 断言刀；全量 `just smoke` 因并行线在途件受阻（berry-launcher 编译红 + app `.bap` 打包产物缺 10 件——均 owner 域），本刀以最小 rootfs 靴 + 标记对拍代偿。
- `docs_guard` PASS；改动面 12 文件（abi.toml/gen_abi.py/abi_gen×2/syz/ABI 文档/syscall.rs/ktest.rs/berry-sys lib/berry-power×4）+ justfile 计数注 + 本笔记，零碰 owner 域（launcher/vnet/tcp/qemu.rs/gui8-smoke.py）。

## Related

[04 号 PM 双服务骨架复核](../../../research/调度彻底计划/04-PM双服务骨架复核.md) §6-1（util 通道形态终裁件——本刀兑现）；`docs/调度彻底计划-v1.2.md:100`（CP-08 验收线）/:217（OQ-8）；`docs/系统调用ABI设计-v1.0.md` §4/§8.3（0x40 行与 v1 计数 39）；[PM 双服务骨架落地](2026-11-13-pm-dual-service-skeleton.md) / [CP-08 PM 接线装配](2026-11-15-cp08-pm-wiring.md) / [CP-08 D3 帽与 PM 预拉](2026-11-15-cp08-d3-cap-pm-prepull.md)（CP-08 弧前驱三件——装配/预拉面先落，本刀补通道首环）；[M4 首批 band 激活](2026-11-16-m4首批band激活-现役集收窄与参数差异化断言.md)（并行任务②——ktest.rs 共居窗口的时序注记）；[timer slack 机制面](2026-11-15-cp11-timer-slack-机制面.md)（并行任务①）；`user/bins/berry-power/src/main.rs` 窗 3（首环闭合窗注）；任务书③（调度深水区收官三件之末）。
