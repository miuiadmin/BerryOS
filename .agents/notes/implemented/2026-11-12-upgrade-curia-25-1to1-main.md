# Agent Note: Curia 2.3 → 2.5.0 直升 main（1:1 纯上游）

Status: implemented

## Problem

上游 curia-md 发布 2.5.0（curia-v2.5.0，2026-11-12）：新增 `--health` / `--suggest-standards` 只读诊断、汇总行组计数、规范/模板/技能措辞同步。main 停在 Curia 2.3。用户指令：彻底 1:1 纯上游迁移、不留本地定制、推送后合并 main。fork-and-own 无 upgrade 通道。

## Decision

- 从 origin/main（fe9e3268）开净分支 `curia-2.5-main`，载荷 21 件（`.agents/**` 19 + `AGENTS.md` + `docs/README.md`）由 2.5.0 模板**逐字节覆写**（模板 sha512 已对 release notes 校验）。
- 本地定制不保留：本仓 `AGENTS.md` 规则源、`docs/README.md` 裁决表、CLAUDE.md 指针文件均被模板版取代；旧内容回溯 = 本提交的父提交（fe9e3268）。
- 三桥按上游契约归位：`CLAUDE.md` → 符号链接 `AGENTS.md`（旧指针备份于 `CLAUDE.md.pre-curia`，不入库）；`.claude/skills`、`.codex/skills` → `../.agents/skills`。
- 存量用户内容（plans/notes/research/archive 条目、docs/ 其余文档）一律不动，规则不回溯。
- wip 分支与本 worktree 之外的升级记录见 `origin/wip/sched-1.1`（616e30b4）与 `origin/curia-2.5`（g43c1 worktree，5bb53db4）。

## Alternatives considered

- 沿用 2.4 升级笔记的"特记重涂"路线：被拒——用户明令 1:1 纯上游，不留本地措辞。
- 在 wip/sched-1.1 上继续 revert 并人（另一活跃会话 11:04 起以 CI 门禁为由恢复宿主件）：被拒——该分支有并行会话在飞，直接从 main 落净提交零冲突、零裹挟。

## Consequences

- main 载荷面与上游 2.5.0 完全一致；`--health` / `--suggest-standards` 可用。
- 本仓自有门禁 `tools/docs_guard.py` 若校验宿主件（AGENTS.md/docs/README.md 定制形态）将报红——该门禁守的正是本次按指令移除的定制，属预期信号，处置由用户定夺。
- wip/sched-1.1 / 其他 worktree 日后并入 main 时，载荷面与宿主件可能出冲突，届时以本 1:1 基线为一侧人工合流。

## Related

- 上游 `miuiadmin/curia-md` @ curia-v2.5.0 · [2.4 升级笔记（wip 分支）](../../notes/implemented/) 历史 2026-11-08-upgrade-curia-24.md（在 wip/sched-1.1）
