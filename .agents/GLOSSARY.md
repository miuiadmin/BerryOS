# GLOSSARY — 术语表（种子）

本表统一工作区术语：**一概念一词**，用新词前先查、写进正文前先登记。条目格式：`**术语（英文）**：定义。_Avoid_: 易混叫法`；`_Avoid_` 只作命名指引，机器不查。

## 制度

**Curia（枢机院）**：本套文档管理制度的总称——两根布局、10 类文档、四句口诀与轻量自检脚本；定位为可移植到任何项目的文档管理方案。_Avoid_: 文档平台、知识库系统

**两根（two roots）**：受管文档的封顶布局——`.agents/` 装过程与过程产物，`docs/` 装外部事实、自由知识与现行标准。_Avoid_: 双目录方案

**四句口诀**：放哪的最后裁决——没做的→`plans/`、做完的→`research/`、为什么→`notes/`、过时的→翻状态或归档。_Avoid_: 目录规范

**家（home）**：类型对应的固定归属目录——`.agents/` 下五个（`plans/`、`research/`、`notes/`、`skills/`、`archive/`）；`docs/` 是 `standard` 的家。_Avoid_: 分类目录

**类型化文档（typed doc）**：带 `Type:` / `Status:` / `Updated:` 三行头块、按 10 类之一定义、住对应家或 `docs/` 的文档；`docs/` 只允许 `standard`，目录说明文件、笔记、`skill` 与 `docs/` 自由知识不适用。_Avoid_: 正式文档

**头块（header block）**：文档开头的固定字段行块——类型化文档三行固定（`standard` 另加 `Version:`），笔记两行（标题与 `Status:`），`skill` 用 frontmatter；`docs/` 自由知识无头块。_Avoid_: frontmatter（只有技能用）

**标准（standard）**：`docs/` 下带 `Type: standard`、`Version:` 与 `Status:` 的现行契约——设计规范、UI 设计系统、API/数据契约、编码约定、写作与排版约定；状态取 `current` / `historical` / `superseded`。_Avoid_: 规格（`spec` 住 `plans/`）、普通说明

**docs 自由知识（free-form docs）**：`docs/` 下没有 `Type:` 的文档——外部事实、接入说明、架构导览与 FAQ；无状态机，当前文件即真相。_Avoid_: 把所有 `docs/` 文档都叫无头文档

**契约版本（contract version）**：模块设计文档与 `docs/` 标准头块里的 `Version: MAJOR.MINOR.PATCH`——记这份文档自己的契约演进；与包版本、取代链各管一件事。_Avoid_: 文档版本、发布版本

**笔记（note）**：`notes/` 下的决策因果记录，头块两行，取值与所在 lifecycle 目录一致。_Avoid_: 日记、流水账

**桥（bridge）**：把 `.agents/` 暴露给各 CLI 的入口（`CLAUDE.md`、`.claude/skills`、`.codex/skills`）——`init` 在目标路径空闲时建符号链接，被占用时保留原物并打印手工命令。_Avoid_: 适配层、同步脚本

**技能（skill）**：`skills/<name>/SKILL.md`——按需加载的工作流；frontmatter 的 `description` 决定触发时机。_Avoid_: 提示词模板、脚本

**归档四步**：移动、加 `Archived:` 行、重写自身相对链接、修入链；第 3、4 步缺一步都会留下断链或静默错链。_Avoid_: 备份、冷存储

**取代（superseded）**：文档被取代后的终态；必须带 `Superseded-by:` 指向取代者，不得迁出。_Avoid_: 废弃、作废

**取代链（supersession chain）**：从 `Superseded-by:` 逐跳追下去必须终结在现行态文档；成环、断链、终结于笔记/docs 自由知识/归档件/退役态都算错——检查器执法（2.3）。_Avoid_: 版本历史

**现行态（current status）**：链终点判据口径——`proposal`/`plan` 的 `draft`/`active`、`spec` 的 `draft`/`active`/`done`、`research` 族与 `runbook` 的 `current`、`standard` 的 `current`；其余取值是历史，可上链不可当终点（2.3，2.4 补 standard）。_Avoid_: 最新版

**一主题一现行承载点**：同一主题同一时刻只有一份现行文档；新全局指导要么原地改现行份、要么新开一份并取代旧的——跨两根生效（`plans/<模块>/design.md` 与 `docs/` 标准不同时声称同主题现行）（2.3 纪律，机器判不出同主题）。_Avoid_: 双轨并行

**模块设计文档（module design doc）**：模块总体设计的固定形态 `.agents/plans/<模块>/design.md`——`Type: spec` + `Version: MAJOR.MINOR.PATCH` 必填、免日期前缀、目录即索引；BerryOS 口径 = **新模块**默认形态，存量主题承载点留 `docs/` 原地演进（2.3）。_Avoid_: 设计规格书（泛称）

**存量树只读纳管**：采用本规范时存量文档保持原样、规则不回溯、下次实质修改时自然补齐。_Avoid_: 迁移、批量修复

**原始语料豁免（raw marker）**：目录内放空标记文件 `.agents-raw` 即整目录免检，用于外部镜像等批量采集物。_Avoid_: 白名单、忽略目录

## 写作

**slop 清单**：写作自查的九类脏东西（重复规则、状态注解、推理记录等），见 [prose-standard](./skills/prose-standard/SKILL.md)。_Avoid_: 检查表

**三档示例法**：校准写作规则的方法——每条规则配「过度裁剪 / 平衡 / 过度展开」对照。_Avoid_: 正反例

## BerryOS 领域

**雷册（leice / 判例库）**：`docs/雷册-判例库.md`——踩坑执法判例全文的单一事实源；新判例 = 域文件瘦行 + 雷册对应节 + 判例索引，三处一动。_Avoid_: 笔记（决策因果去 `.agents/notes/`）、复盘文档

**判例瘦行**：域规则文件里的一行判例摘要，行尾锚点（如 `→ 雷册 §A8`）直达雷册对应节全文。_Avoid_: 判例全文（瘦行不放全文）

**排除 8 问**：借鉴任何外部参考前的准入门（轴 + PICK / 冲突中心 / 许可边界 / G0–G5 关卡等，全文 → `docs/架构综合纲领.md` §4）。_Avoid_: 评审清单

**G0–G5**：QEMU 自动化验收关卡分级（G0 冷启动 → G5 满配），AI 测试友好化的机器门面。_Avoid_: 测试等级（易与 smoke 标记混淆）

**smoke**：无人值守多跑冒烟（标记清单单源 = `tools/smoke.sh`，语义注解 → 雷册 §C）。_Avoid_: 冒烟测试（口头泛称）

**intent（意图）**：调度器第一类输入 u8（ADR #42 两段式、#58 六档词汇表；公平默认、intent 只叠加）。_Avoid_: 优先级（intent 不是 prio）

**能力（capability / handle）**：内核态句柄即权限——对象访问权经 handle 传递，无路径式授权。_Avoid_: 文件描述符

**.bap**：BerryOS 应用包格式（`user/` 构建链 mkapp 打包，rootfs 由 mkcpio 聚合）。_Avoid_: app 镜像
