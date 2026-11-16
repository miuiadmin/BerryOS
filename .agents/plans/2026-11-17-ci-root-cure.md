# CI 根治：时序断言疫苗 + 第二 runner

Type: plan
Status: active
Updated: 2026-11-17

一句话说明：结束「慢宿主 panic → 打补丁」循环的两刀——① 内核自测时序断言从绝对值改运行时定标（疫苗，一次性退役整族病）；② 第二台原生 aarch64 runner（正加速 + 吞吐翻倍），硬件采购归 owner 裁决。

## 为什么现在做（账面）

- 近 100 run：34% cancelled（队列互踩）、8 深积压、单 runner 串行 ~20min/run——lane 推量已超吞吐。
- 11-01 以来验证面 82 笔中 ~7 笔是时序/帽类补丁，同期内核自测新增 24462 行（91 标记/深睡三族/满表探测环/dpg 反向臂）——验证内容指数涨、验证环境原地不动。
- 病族同根：绝对 cycles 阈值 × 无 KVM TCG 慢宿主（dpg_press Σ=12.9-33M vs 阈 437500 = 30-75×、PP-4 满表千分位、boot 帽 360→900、evsclient 帽三连）——全是「拿秒表测一台走时不齐的钟」。

## 刀 1：时序断言疫苗（代码侧根治，零成本）

范围（审计定谳）：绝对 cycles 断言族当前仅 dpg_press 一处（θ_p·W 绝对值判据）；PP-4 已是比值门（3000‰，TCG 噪声已按判例放宽）——疫苗迁移面小，正办 = 在 dpg_press 处直接推广「boot 期基准环定标」。

步骤：
1. boot 早期（ktest 跑前）插一个基准测量环：固定 N 次空操作迭代，CNTVCT 差值 → `SLOW_FACTOR: AtomicU64`（快速宿主 = 1.0×、HVF ~30×、TCG 10-75×）。
2. dpg_press 的 theta_w 判据乘 SLOW_FACTOR——慢宿主阈值自动放大，真回归（相对漂移）照样炸。
3. #428 的 10× 断层线降软保底不变（疫苗失效兜底）。
4. 验证：本地 HVF（慢）+ 服务器 TCG（更慢）双绿；CI smoke 快速窗不触发降软。
5. 随刀笔记 + 沿革登记。

## 刀 2：第二台原生 aarch64 runner（环境侧根治）

首选项（按成本/保真度排序）：
1. **二手 Apple Silicon Mac mini M2**（~¥3k 一次性，无月费）——aarch64 + HVF 原生加速，与 owner 本地同档（本地 8s/boot 全相绿实证），bcts 91 标记 ~15min；旧 x86 VPS 保留跑 host 门 + x86 面。注意：用户指示过 CI 别放 owner 本机 Mac——此为独立新机，需 owner 裁决放置（家宽/机房）。
2. Hetzner ARM 独服（~€40+/月）——须先验 /dev/kvm 可用（云 VPS 普遍不给嵌套 KVM，现 VPS 即此坑）；推荐先开按小时计费的最小 ARM 机验证 KVM 后再定。
3. 保留现状 + 只做刀 1（零成本底线）。

部署要点：labels [self-hosted, ARM64, arm64]；workflow runs-on 分流（heavy QEMU 面 → ARM64；host 测/checkout → x86）；共享 target 缓存复用 §7 双 runner 纪律（只共享 target 不共享 registry）；macOS runner 注意 bash5 显式路径（justfile 已有）+ TCC 面（runner 用户家目录跑勿入 Documents）。

## 验收

- [ ] 刀 1 落地后：bcts base push 形 run 连续 3 绿（当前 0 绿基线）
- [ ] CI 队列积压 < 30min p95（当前 98min 尖峰）
- [ ] 时序类补丁笔归零（每月盘点）

## 归属

- 刀 1 = kernel lane（ktest/dpg 面）——本计划即工单
- 刀 2 = owner 裁决（硬件采购/放置）；agent 可代执行部署
