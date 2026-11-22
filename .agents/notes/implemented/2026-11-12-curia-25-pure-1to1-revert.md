# Agent Note: Curia 2.5.0 终态翻转——撤混合收口，回归纯 1:1 上游

Status: implemented

## Problem

上轮会话内混合收口（86cc9c6a 宿主三件恢复 + 技能回溯、a61c4086 三修刀）被用户终裁推翻：Curia 面必须**彻底 1:1 跟随上游 curia-v2.5.0**，不留任何会话自定义涂层，自动执行不再询问。

## Decision

构造性 revert（等效反转两个提交，免 patch 冲突）：`AGENTS.md`/`CLAUDE.md`/`docs/README.md` 取 616e30b4 模板态（CLAUDE.md 回符号链接桥）；`docs/阶段沿革.md` 取 e6b3ab71（撤二百四十四补记）；删 `workflow-implement` 技能与上轮收口笔记；删 `CLAUDE.md.pre-curia` 备份。终态 = 616e30b4 的 Curia 面 + e6b3ab71 及此后的非 Curia 提交。

## Alternatives considered

- **保留宿主件恢复的混合终态**：被拒——用户明令纯 1:1 零自定义。
- **patch 式 `git revert` 两提交**：未走——连续反转同文件会撞三方合并冲突；构造法语义相同且确定。

## Consequences

- BerryOS 自有执法 `tools/docs_guard.py` 将 G2/G3/G5/G7 FAIL（探针针对已被移除的旧版 AGENTS.md 与旧 CLAUDE.md 指针形态），CI 推送即红——已知代价，属用户裁定范围；探针基线可自 725e5554 回溯，docs_guard 适配另行裁决。
- BerryOS 旧规则口径（域铁律路由/雷册分工/入口预算制度）无现行载体，基线 725e5554 在库可回溯；本仓实际工作纪律转由各会话自觉与 docs/ 既有正文承载。
- 后续 Curia 升级公式：载荷面整目录对拍上游，宿主件随模板演进，零回涂。

## Related

- [1:1 迁移笔记](2026-11-12-upgrade-curia-25-1to1.md) · 上游 miuiadmin/curia-md @ curia-v2.5.0（0958ae1）· 被撤提交 86cc9c6a / a61c4086（git 历史在案）
