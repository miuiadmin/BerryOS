# Agent Note: cbox 刀 1 内核半件（RAM 崩溃记录表 + trap 双钩 + ABI 0x42 排液）

Status: implemented

## Problem

wtdie 案（launcher 停摆连坐链）暴露取证主缺口：崩溃现场只活在串口 dump 里，撕毁率约四成；对标华为 AGC 流水线按单机无云端形态裁剪（[design.md](../../plans/cbox/design.md) v0.1.0，提交 72a754eb5），刀 1 落内核侧采集面。

## 处置

- `kernel/src/cbox.rs`：104B `CboxRec`（const assert 钉板布局）+ 8 槽环形表 + stamp seqlock 撕裂防护（写者奇标 / Release / 偶标，读者复读夹 Acquire）。
- `arch/aarch64/src/trap.rs` 双钩：EL0 击杀链 `svc_enter` 前写记录；EL1 fatal halt 两处经 HAL 槽镜像落表（`lib.rs` 槽反转先例）；恢复路径（BRK / 期望故障 / canary / demand）不写。
- `sched`：`current_task_name()` 只读出口（CURRENT 空态 = `(boot)` 占位）。
- ABI 0x42 `cbox_drain` 入册（异常组、stage v1）：`cap==0` 尺寸查询 / 不足 `-ENOMEM` 不部分写 / 全量快照 seq 升序；abi-gen 四产物同批再生。
- spec 回写实测布局（104B + stamp）与 `cap==0` 语义。

## 验证

- `cargo check -p berry-kernel --target aarch64-unknown-none` 零新告警；`just abi-check` 绿（NR v1 36 号在册，四面单源一致）。
- 排液语义（快照非消费、同 boot 幂等）host 侧无验面——用户态消费者在 [刀 2](2026-11-15-cbox-knife2-userland.md) 落地后经 smoke 全链验证。

## 边界

- 0x42 初判 SVC_CLASS_NONE 为漏审（出参 copy_out 走查页表 = §A113 读者面），刀 2 回修 SVC_CLASS_TABLE（见刀 2 笔记处置节）。
- 内核 panic 落盘 / 第二排液者挂刀 4（design.md 分期表 DEFERRED 行）。
