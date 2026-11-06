# GLOSSARY — 术语表（种子）

本表统一工作区术语：**一概念一词**，用新词前先查、写进正文前先登记。条目格式：`**术语（英文）**：定义。_Avoid_: 易混叫法`；`_Avoid_` 只作命名指引，机器不查。

## 制度

**Curia（枢机院）**：本套文档管理制度的总称——两根布局、9 类文档、四句口诀与轻量自检脚本；定位为可移植到任何项目的文档管理方案。_Avoid_: 文档平台、知识库系统

**两根（two roots）**：受管文档的封顶布局——`.agents/` 装制度与过程，`docs/` 装外部事实与知识。_Avoid_: 双目录方案

**四句口诀**：放哪的最后裁决——没做的→`plans/`、做完的→`research/`、为什么→`notes/`、过时的→翻状态或归档。_Avoid_: 目录规范

**家（home）**：`.agents/` 下的五个归属目录——`plans/`、`research/`、`notes/`、`skills/`、`archive/`。_Avoid_: 分类目录

**类型化文档（typed doc）**：带 `Type:` / `Status:` / `Updated:` 三行头块、按 9 类之一定义、住对应家的文档；目录说明文件、笔记、`skill` 与 `docs/` 文档不适用。_Avoid_: 正式文档

**头块（header block）**：文档开头的固定字段行块——类型化文档三行固定，笔记两行（标题与 `Status:`），`skill` 用 frontmatter；`docs/` 文档无头块。_Avoid_: frontmatter（只有技能用）

**笔记（note）**：`notes/` 下的决策因果记录，头块两行，取值与所在 lifecycle 目录一致。_Avoid_: 日记、流水账

**桥（bridge）**：把 `.agents/` 暴露给各 CLI 的入口（`CLAUDE.md`、`.claude/skills`、`.codex/skills`）——`init` 在目标路径空闲时建符号链接，被占用时保留原物并打印手工命令。_Avoid_: 适配层、同步脚本

**技能（skill）**：`skills/<name>/SKILL.md`——按需加载的工作流；frontmatter 的 `description` 决定触发时机。_Avoid_: 提示词模板、脚本

**归档四步**：移动、加 `Archived:` 行、重写自身相对链接、修入链；第 3、4 步缺一步都会留下断链或静默错链。_Avoid_: 备份、冷存储

**取代（superseded）**：文档被取代后的终态；必须带 `Superseded-by:` 指向取代者，不得迁出。_Avoid_: 废弃、作废

**存量树只读纳管**：采用本规范时存量文档保持原样、规则不回溯、下次实质修改时自然补齐。_Avoid_: 迁移、批量修复

**原始语料豁免（raw marker）**：目录内放空标记文件 `.agents-raw` 即整目录免检，用于外部镜像等批量采集物。_Avoid_: 白名单、忽略目录

## 写作

**slop 清单**：写作自查的九类脏东西（重复规则、状态注解、推理记录等），见 [prose-standard](./skills/prose-standard/SKILL.md)。_Avoid_: 检查表

**三档示例法**：校准写作规则的方法——每条规则配「过度裁剪 / 平衡 / 过度展开」对照。_Avoid_: 正反例

## 你的领域（示例）

**占位术语（placeholder term）**：换成你们项目的第一个领域词——一句话写清它指什么、与哪些近义概念怎么区分。_Avoid_: 易混叫法（换成真实要避开的词）

**占位术语二（second placeholder）**：第二个种子词；本表只给格式示例，项目自行增删。_Avoid_: 换个真实易混词
