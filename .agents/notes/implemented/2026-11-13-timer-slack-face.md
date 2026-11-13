# Agent Note: timer slack 聚合判定律纯面(berry-slackface 落件)

Status: implemented

## Problem
调度器设计 v1.1 §3.8 :155 承诺「timer 原语带 slack+policy 三元组(学 zx_timer_set,v1 day-one)」,01 号对账实测代码零命中(kernel/sched grep slack 空,硬缺口 #9)——省电第二杠杆(唤醒合并)无判定律可依,且 slack 域值散在 iprofile 表列而无消费语义面。

## Decision
新建 user/berry-slackface(独立小工作区,零依赖,host 可测 12 测;no_std 面 aarch64-unknown-none 可编)立四件判定律:①任务 slack 域六档映射(数值 = iprofile 手表表 timer_slack_ns 列零依赖镜像 + 同步义务注;LC=0 exact 逃生舱);②聚合分桶(deadline 稳定排序贪心开桶,进桶判据 d_j ≤ min_e,桶持 max_d/min_e 两不变量);③三条可证律——L1 late-only 只迟不早(man7 PR_SET_TIMERSLACK 同构)、L2 最紧 slack 不越(交互零牺牲即推论:桶内 INTERACTIVE 成员钉桶于其 +10ms 内、LC 成员钉于 deadline 恰点)、L3 防饥饿上界(桶跨度 ≤ 开桶者 slack,松弛按原始锚点计永不重锚);④policy 三元组 (slice,intent_default,timer_slack) 形(G5 bootparams 两键之第三键补全,slice 域 [16,1024] 镜像)。醒点取窗内最迟 + 网格向下对齐(律赢于对齐)。sched 接线归接线窗:kernel/sched/壳族/justfile 零碰,内核定时器路径落线、bootparams 第三键解析、zx_timer_set 形 syscall 面全留窗。

## Alternatives considered
path 依赖 iprofile/intface 消镜像(否:任务书明令零依赖,镜像 + 同步义务注是零依赖纪律下的诚实形;接线窗解禁后应改 path 依赖);CENTER 形 slack(±双向,Zircon 有;否:Linux 量产面只迟不早,双向放大最紧成员违 L2 纯度);最优装箱替贪心(否:区间图团覆盖 NP 难,判定律取确定性 + 可证不变量优先);任务书括号序「MAINTENANCE 中」(否:iprofile 单源实序 MAINTENANCE 10s > BG 500ms,一事实一归属从单源,crate 头注「数值推断标注」①已记差异)。

## Related
[PM 双服务骨架](./2026-11-13-pm-dual-service-skeleton.md)(同 campaign 上轮:PM 双服务是省电第三杠杆服务面,本件是第二杠杆判定律面);设计锚 docs/调度器设计-v1.1.md §3.8;01 号对账 research/调度彻底计划/01-调度现状全景与在途线收编.md §1.2 缺口 #9。
