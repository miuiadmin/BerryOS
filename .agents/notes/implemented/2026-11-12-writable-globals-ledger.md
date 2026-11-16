# Agent Note: writable_globals 台账补册（NAME_HOOK / SPIN_BARK 两目，main 侧自修）

Status: implemented

## Problem

`writable_globals.py --check` 在 main 红：`arch/aarch64/src/kern_lock.rs::NAME_HOOK`
与 `sched/src/spin.rs::SPIN_BARK` 两处 `static mut` 声明点未入册（2026-11-06 起的
CI 旧伤，与 Curia 无关；上一刀 [2026-11-12-docs-guard-main-sync.md](2026-11-12-docs-guard-main-sync.md)
曾记"随 wip 侧修复合流"，用户裁决改为 main 侧彻底自修）。

## 处置

- owner 无争议：同文件兄弟条目（`kern_lock.rs::PREEMPT_ENVELOPE` / `SVC_WINDOW`）
  在台账即 `scheduler` 组，`--update` 推断一致。
- `python3 tools/writable_globals.py --update` → 台账恰 +2 行、零删（无陈旧条目）、
  组别 scheduler，按字典序落位。

## 验证

- `--check` → ok (64 entries)（exit 0）
- docs_guard PASS 0 警 / check-docs --strict 122 篇 0 告警（不回退）

## 边界

- wip 侧红目 `timer.rs::SLICE_HZ_OVR` 为 wip 专属代码（main 无此符号，无可修），
  仍归 wip 会话随刀入册。
