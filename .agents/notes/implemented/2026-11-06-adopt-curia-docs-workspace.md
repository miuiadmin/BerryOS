# Agent Note: 采用 Curia 2.2 作为文档管理方式

Status: implemented

## Problem

BerryOS 文档此前全数平铺在 `docs/`（85 项）与 `research/`：知识性文档（设计/规则/路线图）与过程性文档（挂账、审查报告、战役复盘、战略裁决）同层混放，「这份放哪、那份过时没有」没有明文规则，人与 agent 各凭习惯；断链、头块、生命周期也无机械自检面。多会话并行时，索引/沿革这类共笔入口频繁成为冲突点。

## Decision

采用自研发布（miuiadmin/curia-md，Curia 2.2，MIT）的 fork-and-own 载荷，一次落地、之后随本仓维护：

- 两根分工：`.agents/` 管过程与制度（计划/决策笔记/技能/归档），`docs/` 继续做知识与事实根；`research/` 登记为存量调研根（等价 `.agents/research/` 的角色，不进默认扫描面，显式路径报告模式巡检）。
- 存量只读纳管：`docs/` 与 `research/` 既有文档命名、结构、版本约定（`名-v主.次.md`）保持原样，规则不回溯；新写文档的归属裁决表落 [docs/README.md](../../../docs/README.md)。
- 入口路由化：AGENTS.md 只并入 95 字符路由（仓库布局一行 + 维护合同一行，恰压 3000 警线零警），Curia 规格正文住 [.agents/README.md](../../README.md)——与本仓「AGENTS 只载当前状态 + 铁律路由」的单源纪律同构。
- 桥：`.claude/skills`、`.codex/skills` 两条符号链接桥接上；`CLAUDE.md` 保留 `@AGENTS.md` 导入指针形态（非 Curia 的符号链接形态）。

## Alternatives considered

- **Curia 三节全文并入 AGENTS.md**：README 对「宿主已有 AGENTS.md」的建议并法。被拒：并入约 1200 字符会击穿 3400 字符注入硬帽并触警，且与「入口只载路由、正文另住」的本仓纪律冲突；改用 95 字符路由行，「一事实一归属」判据词入行。
- **CLAUDE.md 换成 Curia 符号链接桥**：被拒——本仓 `tools/docs_guard.py` G5 执法要求 CLAUDE.md 含裸行 `@AGENTS.md`，符号链接后文件内容等于 AGENTS.md，G5 必 FAIL。`@import` 指针对 Claude Code 功能等价（同样读到 AGENTS.md 全文），保留现状零成本。
- **继续 docs/ 单根、命名约定自管**：被拒——无机械检查面，归属分歧与断链只能靠人审；且引入 curia-md 是仓主的直接要求。
- **外部文档平台（Notion/GitBook 类）**：被拒——引入服务与外部依赖，与「零依赖、纯文本、git 即历史」的仓内纪律冲突。

## Consequences

- 新写文档有明文归属：计划/提案 → `.agents/plans/`，决策因果 → `.agents/notes/`，技能/手册 → `.agents/skills/`，知识与契约 → `docs/`（版本命名照旧），调研 → `research/`；冷历史 → `.agents/archive/`，永不删除。
- 本批同批落地件：载荷 20 文件 + 2 桥 + `docs/README.md` 改写 + `.agents/research/README.md` BerryOS 特记 + 本笔记；check-docs 默认面（`.agents/` + `docs/`）基线 108 篇 0 告警，`docs_guard` PASS 0 警。
- `--strict` 提交闸门可挂未挂：本仓多会话重度并行，pre-commit 会因兄弟会话的工作树半成品而误拦无关提交；是否挂钩留给仓主决定。
- AGENTS.md 预算用到 3000/3400（警线恰压线零警）：入口再有任何增量即触警，扩充前需先迁出。
- 三份共笔登记文件（`docs/索引.md`、`docs/阶段沿革.md`、`docs/命令速览.md`）落地当时有兄弟会话活动改动，为不夹带未登记（并行会话纪律优先）；待文件空闲后补：索引补 `.agents/` 与 `docs/README.md` 条目、沿革追记一行、命令速览补 `node .agents/tools/check-docs.mjs`。
- fork-and-own：上游无 upgrade 子命令，日后同步靠对空目录重跑 init 做 diff。
- `.claude/skills` 桥因 `.claude/` 整目录在 `.gitignore` 而不入库（本机在位）；新 clone 后 Claude Code 技能桥需手工重建 `ln -s ../.agents/skills .claude/skills`，规则读取不受影响（CLAUDE.md `@import` 在位）；`.codex/skills` 桥以符号链接对象入库，clone 自动还原。

## Related

- [规范全文](../../README.md) · [docs/README.md（归属裁决表）](../../../docs/README.md) · [.agents/research/README.md（存量调研根特记）](../../research/README.md)
