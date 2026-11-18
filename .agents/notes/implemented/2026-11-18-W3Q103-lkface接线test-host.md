# Agent Note: W3-Q103（lkface 补接线 test-host——判例族「有 host 测零自动执法」第三席）实施记录

Status: implemented
创建：2026-11-18。PR 收口以 [沿革五百六十五](../../../docs/阶段沿革.md) 为准。

## 问题
berry-lkface 自 #305（W3-Q36 lockdep 检查面纯函数外迁）入库起，host 测从未接线 justfile test-host。#483（W3-Q101）reviewer 底账明确备注此缺口并建议 W3 补接线行。13 测零自动执法——lkface 若随 HAL kern_lock.rs 演进发生判定律回归，test-host 与 CI 双盲（CI-light 走 paths 分流不含此面）。

## 方案
justfile test-host 配方 berry-balface 行后补一行 `cargo test -p berry-lkface --target {{host}}`，注面写实 W3-Q36（rank 七类全序/三检查）+ W3-Q101（kl_diag_gate 门律 + KL_SPIN_BARK_MASK 迁册）。零代码变化，纯接线刀。同款先例：W3-Q89 heapface 接线随刀补（Q39 交付漏接线）。

## 验证
- test-host 本树实测 2694/0 全绿 EXIT=0〔266 组 = 264 组 + lkface lib/doc 两 result 行；= base 系 2681 + lkface 13 计入〕。berry-appshell 首跑 2 败 = R127/R128 同签名 flaky，隔离三连 27/0 复跑定谳。
- 口径：Q101（#483）合入后 lkface 15 测全额 = 2696；07.md :1 已随刀滚新。

## 关联
- 判例族前席：berry-ksproto CI 接线（ci.yml :148 注）、heapface Q89。
- W3-Q101 实施笔记随 #483 在途（未入 base，暂不外链——合入后自然可链）。
