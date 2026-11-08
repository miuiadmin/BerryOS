# docs/ — 知识与事实根（Curia）

本目录是 BerryOS 的知识与事实根：设计规格、域规则、判例库、路线图、索引与沿革。**当前版即唯一真相**——改了直接改，历史在 git。

## 新写文档放哪（裁决表）

| 写什么 | 家 |
|---|---|
| 设计/实施/规划/调研/验收类知识文档——含在途设计草案（BerryOS 版本化制度 `名-v主.次.md`，细则见 [索引](索引.md)）；**存量主题**演进原地改，同主题不另建 `.agents/plans/` 副本（一主题一现行承载点） | 本目录 |
| **新模块**总体设计（2.3 固定形态：`Type: spec` + `Version: MAJOR.MINOR.PATCH` 头块、免日期前缀、`ls .agents/plans/*/design.md` 即全模块索引） | [`.agents/plans/`](../.agents/plans/) 下 `<模块>/design.md` |
| **新的跨模块现行标准**（写作排版/编码约定/UI/API 契约这类需「现行第几版」的；2.4 起固定形态：`Type: standard` + `Status: current/historical/superseded` + `Version:`，取代链执法；存量版本化文档不批量转，下次实质修改时再补） | 本目录 |
| 调研报告 | 根 `research/`（存量纳管根；等价 `.agents/research/` 的角色，不进默认扫描面） |
| 轻量计划 / 工作提案（**不进版本化制度**的：步骤清单、立项对齐、agent 工作计划） | [`.agents/plans/`](../.agents/plans/)（3 行头块） |
| 决策因果（为什么、否决了什么） | [`.agents/notes/`](../.agents/notes/)（两行头块） |
| 可复用干法 / 技能 / 操作手册 | [`.agents/skills/`](../.agents/skills/) |
| 本目录知识文档的冷历史 | 按既有制度：原地冻结或 `docs/归档/`（索引维护规则 5；不走 `.agents/archive/`） |
| `.agents/` 内文档的冷历史 | [`.agents/archive/`](../.agents/archive/)（归档四步，永不删除） |

## 约定

- 存量只读纳管（2026-11-06 采用 Curia 起）：既有文档的命名与结构保持原样，规则不回溯；下次实质修改时自然对齐。
- 判例与笔记分工：踩坑执法类判例仍走雷册三处一动（域文件瘦行 + [雷册](雷册-判例库.md)全文 + 判例索引）；决策因果类（选型/否决/权衡）走 `.agents/notes/`——同一变更两类都涉及时各写各的、互链。
- 存量知识文档（版本化 `名-v主.次.md`）无头块、无状态机 = 自由知识，当前版即唯一真相；现行标准例外——`Type: standard` 带头块与轻量状态机（2.4 起，形态见[规范「docs 标准」节](../.agents/README.md)）。不写计划与决策因果——那些归 `.agents/` 的家（规范全文见 [`.agents/README.md`](../.agents/README.md)）。
- 引用一律相对 Markdown 链接；指向 `.agents/` 时写 `../.agents/…`；`索引.md` 是本目录既有详表（维护合同），Curia 的「禁全局 INDEX」不回溯于它。
- 自检：`node .agents/tools/check-docs.mjs`（默认面 = 本目录 + `.agents/`）；提交把关加 `--strict`。
