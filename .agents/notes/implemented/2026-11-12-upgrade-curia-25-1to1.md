# Agent Note: Curia 2.4 → 2.5.0 一比一纯上游迁移

Status: implemented

## Problem

上游 curia-md 发 2.5.0（miuiadmin/curia-md @ curia-v2.5.0，tgz sha512 `06df98c5…6d2edfa10` 已复核一致）：`check-docs.mjs` 新增 `--health` / `--suggest-standards` 只读诊断、汇总行带组计数，其余为规范/模板/技能措辞同步。本仓自 2.4 起在载荷面积累了层层本地定制（.agents/README.md BerryOS 特记、GLOSSARY 领域词条、docs/README.md 裁决表、AGENTS.md 本仓规则、额外技能 workflow-implement），历次升级按文件级 diff 重涂合并，定制面越滚越宽。本次应用户指令改走 **1:1 纯上游**：载荷面与模板逐字节一致，不重涂、不合并、不留任何本地措辞。

## Decision

- **载荷面整目录覆写**（模板只含载荷与 .gitkeep 空骨架，无用户内容，整目录覆写不伤用户文件）：21 载荷件（.agents/** 19 + AGENTS.md + docs/README.md）与 `/Downloads/curia-2.5.0-staging/package/template` 逐字节 cmp 全过——9 件被覆写（GLOSSARY、.agents/README、notes/AGENTS、plans/README、research/README、skills/write-doc、tools/check-docs.mjs、AGENTS.md、docs/README.md），12 件原本即同。特记/裁决表/GLOSSARY 领域词条/本仓 AGENTS.md 规则全部随覆写消失，不保留。
- **删除额外技能 `.agents/skills/workflow-implement/`**（git 已跟踪，直接删）：上游技能面回到 prose-standard / write-doc / write-note 三件。
- **桥形态变更**：旧 CLAUDE.md 是 468B `@AGENTS.md` 导入指针文件（2.4 形态），按上游契约改为符号链接 `CLAUDE.md -> AGENTS.md`；旧指针备份于根目录 `CLAUDE.md.pre-curia`（保留原地、不入库）。`.claude/skills` 与 `.codex/skills` 均已为 `-> ../.agents/skills` 有效链接；detached worktree `berryos-g43c1` 缺 `.claude/skills`，本次补建（其 `.codex/skills` 原已有）。
- **回溯方式**：全部被移除/被覆写内容任务前均干净在库——git 历史 `725e5554`（迁移前 HEAD）；workflow-implement 最后入库于 `8bb34983`（2026-11-11，blob `4d32ea30`）。旧 CLAUDE.md 指针见 `CLAUDE.md.pre-curia`（也在 `725e5554:CLAUDE.md`）。

## Alternatives considered

- **沿用 2.4 的文件级 diff 重涂合并**：被拒——应用户指令本次为 1:1 纯上游；且定制面（特记/裁决表/词条/本仓规则）已与上游演进取交集，重涂成本与漂移风险逐版放大，先例见 [2.4 升级笔记](2026-11-08-upgrade-curia-24.md) Decision 首条的六处手工落。
- **保留 workflow-implement 技能**：被拒——属载荷面本地定制，1:1 面上不允许额外技能件；内容可经 `8bb34983` 回溯，需要时另行走用户内容面（如 notes）重立。
- **保留 @AGENTS.md 指针文件形态的 CLAUDE.md**：被拒——上游 2.5 契约为符号链接形态，且 `--health` 桥检查按 symlink 判定；指针文件留作 .pre-curia 备份兼顾回溯。

## Consequences

- 检查器前后对比：换装前 2.4 默认面 `检查 168 篇 / 告警 0 条`（exit 0）；换装后 2.5（检查器 sha256 f523db58、774 行）默认面 0 告警（exit 0），篇数较 2.4 恰少 workflow-implement 一件、加回本笔记后持平。`--health`：载荷 14/14 齐全、三桥全 symlink-ok、AGENTS.md merged、docs 标准 2 篇（exit 0）。`--suggest-standards`：8 篇候选（文件名含标准类关键词，只读提示）——存量告警/候选一律不修，如实记录，规则不回溯。
- 本仓从「上游底 + 本仓定制涂层」转为「纯上游载荷 + 用户内容（plans/notes/research/archive 条目与 docs/ 正文）」：今后升级可直接整目录覆写对拍，不再需要逐文件裁决；但原特记承载的本仓口径（如 standard 存量不迁、docs 版本化并存制）自此无文档面声明，如仍需要须另立用户内容面文档。
- `CLAUDE.md.pre-curia` 未跟踪未忽略，会常驻 `git status` 未跟踪区；确认放弃回溯指针形态后可手动删除。`berryos-g43c1` 的桥补建不进 git（`.claude/` 被忽略）。
- 在飞改动（staged 的 docs/路线图-v1.1.md 删除、kernel/ 两文件修改及未跟踪新件）与本次迁移严格隔离，提交走 pathspec。

## Related

- [2.4 升级笔记（前篇）](2026-11-08-upgrade-curia-24.md) · [2.3 升级笔记](2026-11-06-upgrade-curia-23.md) · [adopt-curia 引入笔记](2026-11-06-adopt-curia-docs-workspace.md) · 上游 miuiadmin/curia-md @ curia-v2.5.0
