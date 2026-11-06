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
- `--strict` 提交闸门已装**路径过滤版** pre-commit（`f5de7e0` 同日补）：仅当暂存面含 `docs/` 或 `.agents/` 路径时跑 `check-docs --strict`，纯代码提交零影响；卸载即 `rm .git/hooks/pre-commit`。本仓多会话重度并行，故不采用 Curia 文档示例的无过滤全局钩子。
- 三份共笔登记文件的补登（同日二刀 `f11c8fb`、四刀 `27b9a3d`，选择性构造提交——私有 index 双基 blob + 真 index 修补，零扰动兄弟会话）：`docs/索引.md` 已入册 README.md 行（总数 79→80、一区计数 5→6 同动）；`docs/命令速览.md` 已补 `check-docs.mjs` 行；`docs/阶段沿革.md` 已补 Curia 引入节——构造提交落（七十九）位，工作树在兄弟流水（七十八 S2a→八十 S3-b 尚未提交）之后占（八十一）尾号位，兄弟落地后节驻八十一位、编号连续、无撞号。
- 遗漏排查（同日终刀）：`research/`（46 篇）与根 README 显式路径巡检均 0 告警；GLOSSARY「你的领域」占位术语已换 BerryOS 领域 8 词（雷册/判例瘦行/排除 8 问/G0–G5/smoke/intent/能力/.bap）；裁决表消除三处歧义——版本化设计草案（含在途）仍归 `docs/`、`.agents/plans/` 只收不进版本化制度的轻量计划，冷历史双轨（`docs/` 知识文档走原地冻结或 `docs/归档/`，`.agents/` 内文档走归档四步），判例（雷册三处一动）与笔记（`.agents/notes/`）按「踩坑执法 vs 决策因果」分工；CI fail-fast 段补 `check-docs --strict`（与 docs-guard 并列文档面双门禁，ubuntu-latest 预装 Node 零新增依赖）。
- fork-and-own：上游无 upgrade 子命令，日后同步靠对空目录重跑 init 做 diff。
- `.claude/skills` 桥因 `.claude/` 整目录在 `.gitignore` 而不入库（本机在位）；新 clone 后 Claude Code 技能桥需手工重建 `ln -s ../.agents/skills .claude/skills`，规则读取不受影响（CLAUDE.md `@import` 在位）；`.codex/skills` 桥以符号链接对象入库，clone 自动还原。

## Related

- [规范全文](../../README.md) · [docs/README.md（归属裁决表）](../../../docs/README.md) · [.agents/research/README.md（存量调研根特记）](../../research/README.md)
