# EAS 选核首接：placement hint 到 eas_pick_cpu 的放置消费（06 号管道②）

Status: implemented

## Problem

06 号评估终判 §5「管道建成早于水」管道②（EAS 选核）：[berry-em 纯面](2026-11-13-em-energy-face-berry-em.md)已把两段选核律 host 钉板（`eas_pick_cpu` 8 测），sched 侧三源也全部现役——EM 表（boot `em_register_qemu_virt` 注册）、per-CPU util EWMA（[CP-08 util 通道](2026-11-16-cp08-util通道首环-快照syscall裁决与首环闭合.md)）、placement hint 读出口（`policy::placement_hint`，动作空间①）——但断点在消费：placement hint 有产无消（01 号实测三件之一），`sched.rs` 的 `place()` 只认队列深失衡 K=2，能量选核零调用。侦察澄清两点：①现役默认 boot 单核（`-smp 1`，SMP 是 smoke-smp 双核面——`par_enabled` 门控），按任务书分支走「SMP→eas_pick_cpu 调用」接入点 = spawn 放置路由 `place()`；②LB 金丝雀（smoke-smp 相 3）断言 `c0==0 且 c1≥24` 结构性依赖 K=2 深度失衡路由（run7/run10 定谳：子全落 rq0 时偷取结构性到不了子、c1=0 恒红）。

## Decision

三件（2026-11-16，水通管道纪律——链路闭合不求新机制）：①**hint 是 EAS 消费面第一入口**——`eas_pick_place`（纯决策面，输入全注入）+ `eas_place`（现役面收集：EM 域表 × util EWMA × `policy::placement_hint` 三源）接进 `place()` SMP 分支；policy 表对该 key 无放置意见（0/越域值）→ 不进 EAS 面、spawn 维持 K=2 原样——对称 QEMU 表下无 hint 的能量选核平局粘 prev（Linux EAS 不支持对称拓扑，berry-em 头注⑥），直替 K=2 会把 4-burst 全钉本核翻红 LB 门禁，hint 门是机制与门禁双正确的形态；hint∈{1,2} 词汇执法在此消费面收口（policy.rs「词汇执法随 SMP 真消费面定」的兑现位），经 prev 入座享 `eas_pick_cpu` 平局保护（偏好而非钉死，能量严格更省仍可越 hint——ktest EA-6/7 OPP 档界断言：Δ1600<Δ5185 越到对核）。②判序钉死：hint 词汇执法 → overutil 快照门（任一候选域成员核超域顶容量 80% → None 让位 K=2，`eas_pick_cpu` 头注「调用侧」义务，快照 utils 上判与选核同数据无 TOCTOU）→ prev 映射 → 两段选核（EM 无据/task_util 超域顶放不下 → None 回退）。新任务 util 无历史，`SPAWN_UTIL_EST=128`（1/8 满格——非零使对称表判据非退化，est=0 时边际恒平局退化纯 prev 偏好）。③berry-em 经 root workspace `exclude=["user/berry-em"]` 收编为 sched path 依赖（manifest 依赖树不允许两个 workspace root——exclude 保独立根形态又可被依赖，版本治理字面值不变）；ktest `test_eas_place`（末位新号）钉纯决策面七断言 + 现役面三源端到端（EB-0 无表→None 即 LB 零漂移机器断言）；`EAS pick` 选核决策自证行（一次性，par 门内才打=单核/ktest 零输出漂移）。

## Alternatives considered

EAS 直替 K=2 成为无 hint 时的默认放置律（否：对称表平局粘 prev=spawn 本核，4-burst 全落 rq0 → c1=0 翻红 smoke-smp 相 3 硬门禁——「门禁收尾」纪律否决）；单核形态走 spawn 容量检查（否：SMP 双核面在册——smoke-smp 八相 + par gate + thread_start spread 真消费链，任务书分支裁定 SMP→eas_pick_cpu；容量检查的决策面形态保留在 `eas_pick_cpu` 内部「放不下除名」）；berry-em 收编为 root workspace member（否：改独立小工作区治理形态；exclude 路保持「独立根+被依赖」且零碰 user/Cargo.toml members）；ktest 里测 `place()` 路由面（否：ktest 在 `smp_bringup` 前跑、par 门恒关结构性不可达——路由面归 SMP 相，决策面独立成 `eas_pick_place` 注入钉板）。

## Consequences

管道②端到端闭合：policy placement hint（AI 动作空间①）→ 能量比较 → spawn 放置路由，机会主义三回退（无 hint/overutil/EM 无据放不下）全落 K=2 现役启发式（零漂移机器断言 EB-0）。对称 QEMU 表下 EAS 实际只验机制不验收益（K3 kill criteria 压力在册——真收益等不对称拓扑/真机 EM=CP-18）；运行期活消费面 = SMP 相 thread_start（spread）spawn，裸 boot 无 policy 表 → `EAS pick` 行结构性不出现（单双核 boot 实证零漂移）；`capacity_for_util`/`spare_capacity` 容量查询面留 PM 频率环（管道③）共消费；选核决策遥测/overutil 执法面余量归 CP-09。验证：隔离 worktree（HEAD+本刀）单核 boot `EAS ok`+`ktest done`、双核 boot `SMP ok ncores=2`+`PAR gate on`+零 `EAS pick`、berry-em host 8/8 绿；主树合并态（与[管道① class 赋值面收口](2026-11-16-intent-band水通管道-class赋值面收口.md)共树）boot `BANDM4 ok`→`EAS ok`→`ktest done` 绿。相关：[M4 首批 band 激活](2026-11-16-m4首批band激活-现役集收窄与参数差异化断言.md)、[EM 能量比较律前置 berry-em](2026-11-13-em-energy-face-berry-em.md)。
