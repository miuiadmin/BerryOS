# skills/ — 可复用的干法（skill / runbook）

**装什么**：按需加载的流程与准则。同一件事的两个视角同住一个技能目录——`SKILL.md` 给 Agent（frontmatter 决定何时被触发），`RUNBOOK.md` 给人（前置条件、步骤、每步验证、失败恢复）。

**不装什么**：事实与运行时契约（事实 → [../research/](../research/)，现行契约 → 根 `docs/` 的 `standard`）、决策因果（→ [../notes/](../notes/)）、一次性任务清单（→ [../plans/](../plans/)）、类型骨架文本（→ [规范「规范格式（骨架）」](../README.md)）。

## 两个类型

| Type | 文件 | 谁读 | 关键约束 |
|---|---|---|---|
| `skill` | `<name>/SKILL.md` | 各 CLI 经桥扫描加载 | frontmatter 的 `name` 等于目录名；`description` 写清「什么时候用我」，不是「我是什么」 |
| `runbook` | `<name>/RUNBOOK.md` | 人 | 每步要有可观察验证；失败路径要有恢复动作 |

步骤只写一份：两个视角同住时，另一份链接过来，不复制。骨架文本见[规范「规范格式（骨架）」](../README.md)与「10 类文档」表的建议节。

## 加一个技能

按[规范「规范格式（骨架）」](../README.md)建 `skills/<kebab-name>/SKILL.md`（frontmatter 两键：`name` 取目录名、`description` 写触发场景）；需要人向手册时，同目录再加 `RUNBOOK.md`。

## 桥的代价

`.agents/` 经三条桥暴露给各 CLI：`CLAUDE.md`、`.claude/skills`、`.codex/skills`——都是指向 `AGENTS.md` / `.agents/skills/` 的符号链接（不是副本）。代价是**本目录下不要放与技能无关的文件**——它们会被一起分发。
