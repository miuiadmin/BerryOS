# EM 能量比较律前置：berry-em 纯面落地（CP-09 前置）

Status: implemented

## Problem

CP-09（EAS 选核算法 compute_energy/FEuC）的能量比较单源 host 钉板缺口：EM 形状已单源 berry-emface（W3-Q14——表形状执法/极值/overutil 阈值律 host 可测），而能量算法「留 M2/真机」（docs/调度器设计-v1.1.md:346「零算法」；01 号 §2.4 EM「有产无消」三件之一）。CP-09 刀单要求「能量比较单源 host 钉板（emface 扩）」，但 emface/sched 域 owner 在途（emface 接 sched/Cargo.toml 依赖链、sched/src/lib.rs 有在途改动）——零碰约束下钉板无处落。

## Decision

user/berry-em 独立小工作区 crate（slackface/berry-power 同形：空 [workspace] escape，不入 user/Cargo.toml members，零碰 owner 树）。形状/overutil 律经 path 依赖**单源消费** berry-emface（零第二实现——不走 slackface 零依赖镜像路线：path 依赖可通时镜像 = 自造第二事实源）；新增单源三件：①EAS 能量律最简形——eff_opp（最低够用档，Linux em_pd_get_efficient_state 最简形）/domain_energy_uw（域级 OPP 由域内 max_util 定、域能耗 = sum_util×power/capacity「单域一频按 util 线性摊能」）/energy_diff_uw（放置边际）/eas_pick_cpu（两段选核：每域最高 spare capacity 候选 ∪ prev_cpu、最小边际胜、平局 prev 胜）；②QEMU virt 第一张假想表——双核表镜像 sched/src/em.rs:102-111 逐位钉值挂同步义务，单核表假想（推断标注：同 A72 核、power 单 CPU 口径）；③Linux sched-energy.rst Example 2 对照 fixture（cfg(test)，cap/pwr 取文档原表、freq 推断填充）——三案例逐数锚：本面 floor 1_365_774/1_438_801/1_486_130 μW 与文档 1364/1437/1485 同序同胜者。8 测全绿（表完整性/1024 归一/打分单调/档位边界/无据防御）。

## Alternatives considered

slackface 式零依赖镜像形状（否：同批 berry-power 先例已开 path 依赖消费 emface 的合规路，一事实一归属优先）；直接扩 emface（否：owner 占用零碰；收编路径留 CP-09 接线窗两选——emface 扩正式化 or 经 path 依赖引本面，berry-winplan 先例）；迁移语义 energy_diff（prev 减/dst 加——Linux 文档底景含 P 形态）（否：取**任务外底景 + 加法边际**——与唤醒放置同构，prev 的 PELT 残留归调用侧衰减口径；文档三案例在任务外底景下同样逐数复算）。

## Consequences

CP-09 开刀时能量比较律已 host 钉板（裁四「判定律先 host 钉板后进内核」义务预付）；对称拓扑 EAS 无收益在册（Linux 明言不支持对称拓扑——QEMU 表只验机制，K3「placement 打不过零假设即撤」压力真实）；假想表数值全推断标注，真机 EM = CP-18；justfile test-host 未接线（owner 占用零碰——host cargo test 显式行验证，slackface/berry-power 同批口径）；容量/功耗倒挂表的能量语义未裁（emface 形状律只执法频升序，本面测试只钉升序表）。

## Related

[调度彻底计划](../../../docs/调度彻底计划-v1.2.md) §4.2 CP-09/§3 裁四；[PM 双服务骨架](2026-11-13-pm-dual-service-skeleton.md)（重叠互链——同批 PM 侧，berry-power 同款 path 依赖消费 emface 先例）；Linux 对照 docs.kernel.org/scheduler/sched-energy.html + docs.kernel.org/power/energy-model.html（2026-11 取证）。
