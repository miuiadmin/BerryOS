---
name: write-doc
description: 在 Curia .agents 工作区新建或修改文档的完整流程——定家定型、按骨架建档、先核事实再写字、落头块、必要时翻状态或退役。当要写调研报告、实施规格、计划、事故复盘、证据快照、运维手册，决定一篇文档放哪、是什么类型，或用户说「写个文档」「补个规格」「整理成文档」时使用。
---

# 文档写作（write-doc）

句子与删改看 [prose-standard](../prose-standard/SKILL.md)，决策因果看 [write-note](../write-note/SKILL.md)。本技能是流程；类型的权威是[规范全文](../../README.md)的「9 类文档」表。

## 定家定型（两步）

1. **定家**：按四句口诀——为什么 → `notes/`（走 [write-note](../write-note/SKILL.md)）；没做的 → `plans/`；做完的 → `research/`；可复用的干法 → `skills/`；外部事实 → 根 `docs/`；冷历史 → `archive/`。
2. **定型**：同一家里类型互斥，判据查[规范全文](../../README.md)的「9 类文档」表（每类的家与建议节都在表里，不要背表）。两组最常混的：`survey` 会过时、`reference` 持续维护；`spec` 做完仍是权威（`done` 不退役）、`plan` 做完就是历史。

## 流程

1. **建档**：按[规范全文](../../README.md)「规范格式（骨架）」手抄骨架、定文件名（日期型带 `yyyy-mm-dd-` 前缀；`skill` 与 `runbook` 的 topic 是目录名，须 kebab-case）。
2. **先核事实，再写字**：每条命令、默认值、端点、路径、行为都要跑一遍或读到原文；核不到的写「未核实 + 核实归属」，不凭记忆、类比或隔壁文档下笔。
3. **填建议节**：按骨架顺序写；无内容的节可写 `_（无）_`；同族类型可按内容增删节，但不搬另一个家的事实。
4. **写正文**：只写当前状态；篇幅凭判断——事实写够即止，没有字数预算；格式硬规则（一段一行、中英空格、全角标点、禁 emoji）见[规范「写作规则」节](../../README.md)，句子层面的自查见 [prose-standard](../prose-standard/SKILL.md)。
5. **落头块**：`Updated:` 改今天；`Status: superseded` 必须带 `Superseded-by:` 相对链接。
6. **自检（可选）**：`node .agents/tools/check-docs.mjs` 报告模式看发现，`--strict` 任一发现即退出码 1；报错就地修文档，不要改脚本。装不装提交钩子归项目自定。
7. **同批写笔记**：非平凡变更同批新增或更新 `notes/`（走 [write-note](../write-note/SKILL.md)）。

## 状态与退役

- 想做的：不承诺的意图落成 `draft` 的 proposal；做完的计划翻 `done`，事实与因果分家互链。
- 过时了：优先状态翻转（`superseded` / `done` / `historical`）；冷历史走[归档四步](../../archive/README.md)——移动、`Archived:` 行、重写自身相对链接、修入链。**永不删除。**

## 改类型系统本身

类型有两份表示：规范「9 类文档」表（人读权威）与 `check-docs.mjs` 内联枚举（机器读）。增删类型必须两份同批改——一致性靠人工纪律，无机内自检。
