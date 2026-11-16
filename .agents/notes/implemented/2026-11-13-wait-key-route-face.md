# Agent Note: 等待键路由判定律纯面（W3-Q75 wqmatch keyroute）

Status: implemented

## Problem

`sched/src/sched.rs` P1/P2-1 等待键路由族判定律全内联在原子计数臂里：`wait_bucket` Fibonacci 乘移散列（0x9E3779B97F4A7C15 取高 6 位）、真键判据（obj ∉ {0 纯睡, WAIT_ANY 哨兵}）、`wait_count_add/sub` 三形分账（纯睡不计数/哨兵独立面/真键入桶）、`pi_count_add/sub` donor 判据（真键 ∧ 派生位）、`pi_recompute_needed` 双零门——host 不可达（sched crate 依赖原子表 + KEY_HEADS 裸链），改分账判据或加减不对称时无测试可红。散列常量/桶数 64/SIG_DERIVED=3 等 ABI 冻结值无镜像执法。

## Decision

扩既有 berry-wqmatch（W3-Q2 唤醒匹配同源延续，零新 crate/members 不动）加 `keyroute` 模块：`wait_bucket`（乘移散列 + % WAIT_BUCKETS 保险钉域）/`key_chainable`（真键判据，与 `WaitKey::Obj` 值域恰等）/`CountKind` 三形分账枚举（携带桶号）/`pi_donor_candidate`（键门 ∧ 派生位门独立）/`pi_boost_counted`/`gate_clear`（P1 双零门）/`pi_gate_needed`（P2 双零门）。sched.rs 十判定点换单源 re-export（原子 fetch/KEY_HEADS 裸链留壳）；sched 侧 const assert 执法（SIG_DERIVED 镜像同值 + KEY_BUCKETS==WAIT_BUCKETS 既有）。host 12 测：散列全域域卫 + u128 全宽参考对拍 2000 键 + 0 键恒 0 桶 + 分布健全性 ≥60/64 桶 + 真键三形 + 分账对称模型（入队/离队逆序终态全零）+ donor 三门独立性（键门先于位门）+ boost 值域 + 双零门两族 + 常量冻结锚。

## Alternatives considered

新 crate berry-keyrouteface——否决：路由判定与 wqmatch 主面（键门谓词 matches_wake 同域——真键判据 key_chainable 就是键门的三形投影）同 crate 单源更贴，Q49/Q73 扩既有 face 先例同款。sched 侧 CountKind 枚举镜像——否决：枚举随 crate 单源，sched 侧 match 透明消费，镜像漂移编译红无执法必要。

## Consequences

P1/P2-1 键路由判定律有 host 钉板（分账不对称/散列改形/门控非双零即红）；donor 判据与 §A34 常驻服务形态解耦面文档化。随刀三件（#258–#262 同款 base 债）：wg 两行（KEY_HEADS 登记 + SLICE_HZ_OVR 组标签 arch→scheduler）+ sleepq 笔记补落 + `.gitignore /.target-*/` 补回——base 重写剥落后 wg selftest 必红。

Related: [2026-11-12-sleepq-per-key-wake-chain.md](2026-11-12-sleepq-per-key-wake-chain.md)（P2-1 键路由原文——随刀补落本件）；扩既有 face 先例同 [2026-11-13-timer-slack-face.md](2026-11-13-timer-slack-face.md)（Q72 同款纪律）。
