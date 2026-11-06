# plans/ — 没做的（proposal / spec / plan）

**装什么**：已承诺、会影响后续行动的工作——目标提案、长期规格、实施计划。没开始的、进行中的、做完留档的，都靠 `Status` 区分。

**不装什么**：决策因果（→ [../notes/](../notes/)）、已完成的事实与证据（→ [../research/](../research/)）、外部事实（→ 根 `docs/`）。

## 三个类型

| Type | 回答什么 | 做完之后 |
|---|---|---|
| `proposal` | 为什么值得做、做成什么样算成功、有哪些候选路线 | 翻 `done` 留档；因果写进 [../notes/](../notes/) |
| `spec` | 做成什么样：契约、接口、验收标准 | `done` 不是退役——它是长期可引用的权威规格 |
| `plan` | 分几步做、做到哪了、还剩什么 | 翻 `done`；事实与回执写成 research 的 `evidence` 或 `survey` |

建议节与骨架见[规范全文](../README.md)的「规范格式（骨架）」节。

## 状态与建档

- 三个类型共用 `draft`（没开始）→ `active`（进行中）→ `done`（完成留档）；任意态可 → `superseded`（必须带 `Superseded-by:` 相对链接，且是终态）。
- `active` 的计划必须维护「未完成事项」一节——它是这项工作唯一的活跃清单；本次做不完的项写 `DEFERRED: <原因 / 归属 / 触发条件>`。
- 建档：按[规范全文](../README.md)「规范格式（骨架）」手抄，文件名用 `yyyy-mm-dd-topic-title.md`。
- 找活跃计划：全文检索 `Status: active`，不建人工清单（禁 `INDEX.md`）。

## 流转

- 完成时：计划本体翻 `done`；事实与回执进 [../research/](../research/)；决策因果进 [../notes/](../notes/)。
- 过时：优先翻 `superseded`（终态）；冷历史走[归档四步](../archive/README.md)进 [../archive/](../archive/)。
