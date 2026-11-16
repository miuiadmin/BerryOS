# v3 四动作激活：berry-power 完整策略服务与 policy_bind 授权落位（POLICY4 刀）

Status: implemented

## Problem

15 号 EAS-PA 刀后 berry-power 只发 placement 单动作，且 svcmgr 无 stdin 预拉形态下 `self_proc_h()==0`，0x83 提交恒被 MANAGE 门拒（-EBADF——「生产接线完成、授权待窗」形态）。任务书要求四动作全激活（①placement/②frequency/③weight/④EPP 全经 policy_bind 0x83 注入），但其 ABI 预设（「四动作各调一次 policy_bind 不同 op 值」「policy table 收到 frequency 条目」）与真接口不符，须先侦察定谳再实施；授权面（MANAGE handle 递达）同刀缺位。

## Decision

四件（2026-11-16）：①**注入形态定谳**——0x83 op 词汇 1..6（COMMIT/STATUS/PROMOTE/DEMOTE/UNBIND/SHADOW_COMMIT）无动作对应位：①③④ = COMMIT（op=1）表条目三字段（`PolicyEntry` placement@12/weight@8/epp_flags@13）单条目一次整表提交；②频率**无 0x83 载体**——动作空间立法原文「频率/热限 hint 走 PM 服务、不经调度路径——不在表里」（sched/src/policy.rs 模块注），本服务即 PM：频率 hint 落自家 `FreqVerdict`，内核聚合点写面留窗（main.rs 窗 4）。②**hint 纯面**（lib `hint.rs`）——`weight_delta`（util ≥896 → +16、≤128 → -16，±64 包络复用 polface `WEIGHT_MAX` 单源；+Δ = 提带 race-to-idle，`prio_effective` 带序键高 = 优）、`epp_flags`（thermal 四档 0 nominal/1 fair/2 serious/3 critical〔berry-thermal 词汇〕：None/nominal → 无意见不覆盖，≥fair → Efficiency；二值词汇无 balance 档，内核默认 Efficiency 已在位）、`action_hints` 四动作一次读全，host 3 测（berry-power 33→36）。③**授权落位**（15 号留窗①前半）——svcmgr 预拉 berry-power 改 `stdin_feed: Some(&[])`（vblk/vnet 同款空喂+EOF），唯一目的 = 触发 berry-exec 自描述对条件装配 → gift[2] = 自身 Process handle（MANAGE|DUPLICATE）→ `self_proc_h()` 真值过 MANAGE 三门控真受理（bsh 子进程同款权利形态，非新权类）。④**main 壳四动作链**——`PowerCore::action_hints` → STATUS（gen 对账 + key=self proc_tag，EL0 进程表指针非零）→ COMMIT 单条目 → gen echo 回读对账 → 一行 `policy4` 诊断（place/freq(pm)/wd/epp/key/st/bd/gn/auth）后 park；单发形态，周期面（gen 重试/hint 动态重提交）随窗 2/3 立法刀。

## Alternatives considered

四动作各一次 policy_bind 不同 op（否：真 ABI 无动作 op；整表提交语义下三连 COMMIT 只留末表，中间态无意义）；频率经 0x83 表条目（否：sched/policy.rs 动作空间②立法排除「不在表里」+ 调度路径零频率写点验收线——频率裁决单点归 PM 是设计不是缺口）；nominal 档 → EPP=Perf 覆盖（否：二值词汇无 balance 档，内核默认 Efficiency〔省电身份支柱〕已在位，武断 Perf 不如沉默——无意见是 balance 的诚实形）；svcmgr 代子提交策略（否：那是跨进程 placement 主张，归授权立法刀，本刀只落自建议最小面）；ktest POLICY 断言扩展（阻：kernel/qemu = owner 零碰纪律——断言规格完整落在 [16 号研究件](../../../research/调度彻底计划/16-v3四动作激活-berry-power完整策略服务.md) §留窗②，待 owner 落地）。

## Consequences

berry-power 每 boot 真提交一条四动作条目（key = 自身 proc_tag——park 常驻睡眠，调度影响仅自体；表侧 0 键丢弃面天然免疫）；诊断行预期 st=0 bd=0 gn=gen（四动作真落表活证），授权缺席（裸跑档/旧 svcmgr 像未重建）时退回 st=bd=-9 诚实形态；svcmgr 平像仍恰 0x28000 = D3 帽/pager 钉位零余量（本刀增量恰页内吸收——elf2flat 实过 163840 墙，后续 svcmgr 增长须先升 D3 帽 = kernel 面）；berry-power 新增 berry-polface path 依赖（EPP 打包/权重包络词汇单源，Cargo.lock +2 包）。验证：berry-power host 36/36 绿（aarch64-apple-darwin）、wired musl bin + svcmgr none-target bin 产线形实编（elf2flat --max 163840 过）、`kernel/`/QEMU 零碰（boot/smoke/ktest 证据归 owner 下一轮 build-user+run-rootfs）。相关：[EAS 选核首接](2026-11-16-eas选核首接-placement-hint到eas-pick-cpu.md)、[CP-08 util 通道首环](2026-11-16-cp08-util通道首环-快照syscall裁决与首环闭合.md)、[intent M4 band 骨架](2026-11-15-intent-m4-band-骨架纯面.md)。
