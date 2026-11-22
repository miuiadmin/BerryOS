# research/ — 做完的（survey / evidence / postmortem / reference）

**装什么**：已完成的研究产出与客观事实——调研报告、原始证据、事故复盘、长期事实参考。它回答「查到了什么、发生了什么」，是事实层。

**不装什么**：还没做的调研（→ [../plans/](../plans/)）；决策因果与取舍（→ [../notes/](../notes/)——研究是决策依据时，依据与决策分家、两边互链）。

## 四个类型

| Type | 回答什么 | 关键约束 |
|---|---|---|
| `survey` | 一次性调研的问题、方法、结论 | 结论必须能追到证据；会过时 |
| `evidence` | 原始事实：清单、命令输出、度量、语料镜像 | 保持原样，不下结论（结论去 `survey`） |
| `postmortem` | 事故的时间线、因果链、影响、预防 | 行动项全部关闭后翻 `historical` |
| `reference` | 长期事实参考：持续维护的映射与导览 | 外部事实登记去根 `docs/` |

建议节与骨架见[规范全文](../README.md)的「规范格式（骨架）」节。

## 状态

`current`（与现状一致）→ `historical`（属于历史时点，不再保证为真）；被取代 → `superseded` + `Superseded-by:`（`postmortem` 只有 `current` / `historical`）。

## 纪律

- **来源可回溯**：每条结论要有出处（相对链接、URL、commit SHA）。
- **事实与因果分家**：研究支撑了某个决策时，研究留在这里，决策写 [../notes/](../notes/)，两边互链。
- **批量原始语料**：整目录镜像放进来时，在该目录放一个名为 `.agents-raw` 的空标记文件整目录免检，并另写一篇 `survey` 或 `reference` 说明它是什么、怎么采的。
- 建档：按[规范全文](../README.md)「规范格式（骨架）」手抄，文件名用 `yyyy-mm-dd-topic-title.md`。
