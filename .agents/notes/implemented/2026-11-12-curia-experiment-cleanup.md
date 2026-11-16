# Agent Note: Curia 迁移实验面清零与三桥入库对齐——彻底收官刀

Status: implemented

## Problem

复验波确认 2.5.0 纯 1:1 现态成立后仍余三件尾巴：① `curia-2.5` 实验分支（tip 5bb53db4，唯一未并历史）+ `berryos-g43c1` 工作树（4 项脏态）；② 两个冗余指针分支；③ `.claude/skills` 桥被本分支 `.gitignore` 的 `.claude/` 整目录排除而永不入库——三桥二缺一，fresh checkout 后 docs_guard G7 必红（main 侧 run 43149417487 实证），与 main 已裁方案分叉。

## Decision

- **冗余分支清零**：`curia-2.5-main`（ec8c2094）与 `curia-2.5-claude-bridge`（26b9b72a）删前亲验两 tip 均为 `origin/main` 祖先（历史永存），本地+远端同删。
- **实验分支存档后清**：`curia-2.5`（5bb53db4）先打 `archive/curia-2.5-experiment` tag 并推 origin（历史零丢失、可逆），再移除 `berryos-g43c1` 工作树（脏态四件逐一核验全可弃：mode 位零内容变更 / 468B 旧指针备份（终裁已弃）/ 1.0M+16M 构建产物）并删本地+远端分支。
- **三桥入库对齐 main**：采纳 main 26b9b72a 已裁方案——`.gitignore` 换 `.claude/*` + `!.claude/skills` 取反惯用法（父目录被排除时 git 不下钻，单写 `!.claude/skills` 无效），`.claude/skills` 以 120000 symlink blob（与 `.codex/skills` 同 blob 2b7a412b）入库；任意全新检出三桥全绿，消除 wip→main 合并时的 `.gitignore` 冲突决策点。

## Alternatives considered

- **保留实验分支与工作树**：被拒——用户令「按彻底的去弄」；历史经 tag 存档后分支指针与工作树已无增量信息。
- **`.claude/skills` 维持不入库（等 wip 并 main 时再裁）**：被拒——fresh checkout G7 恒红是说谎的门禁；main 已有完整论证与先例，直接采纳零新决策。

## Consequences

- 分支面 curia 残留清零；工作树 18→15；tag `archive/curia-2.5-experiment` 在 origin 永存。
- 三桥（CLAUDE.md / .claude/skills / .codex/skills）全部以 symlink blob 在库，docs_guard G7 双桥绿、`--strict` 171 篇 0 告警、PASS 0 警。
- 至此 Curia 2.5.0 升级全链终态：纯 1:1 载荷 + guard 适配 + 活文档清残留 + 实验面清零 + 三桥入库，无已知待裁项。

## Related

- [遗漏清查笔记](2026-11-12-curia-omission-sweep-fixes.md) · [纯 1:1 翻转笔记](2026-11-12-curia-25-pure-1to1-revert.md) · main 先例 26b9b72a
