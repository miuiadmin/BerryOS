# docs/ — 知识与事实根（Curia）

本目录是 BerryOS 的知识与事实根：设计规格、域规则、判例库、路线图、索引与沿革。**当前版即唯一真相**——改了直接改，历史在 git。

## 新写文档放哪（裁决表）

| 写什么 | 家 |
|---|---|
| 设计规格 / 域规则 / 路线图 / ADR（知识与契约） | 本目录（BerryOS 版本命名 `名-v主.次.md`，细则见 [索引](索引.md)） |
| 调研报告 | 根 `research/`（存量纳管根；等价 `.agents/research/` 的角色，不进默认扫描面） |
| 计划 / 提案（还没做的） | [`.agents/plans/`](../.agents/plans/)（3 行头块） |
| 决策因果（为什么、否决了什么） | [`.agents/notes/`](../.agents/notes/)（两行头块） |
| 可复用干法 / 技能 / 操作手册 | [`.agents/skills/`](../.agents/skills/) |
| 冷历史（不再维护、不再被引用） | [`.agents/archive/`](../.agents/archive/)（归档四步，永不删除） |

## 约定

- 存量只读纳管（2026-11-06 采用 Curia 起）：既有文档的命名与结构保持原样，规则不回溯；下次实质修改时自然对齐。
- 无头块、无状态机；不写计划与决策因果——那些归 `.agents/` 的家（规范全文见 [`.agents/README.md`](../.agents/README.md)）。
- 引用一律相对 Markdown 链接；指向 `.agents/` 时写 `../.agents/…`。
- 自检：`node .agents/tools/check-docs.mjs`（默认面 = 本目录 + `.agents/`）；提交把关加 `--strict`。
